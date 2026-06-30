use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use tracing::info;

#[allow(dead_code)]
pub struct VhdBackend {
    pub file: File,
    pub bat: Vec<u32>,
    pub vhd_block_size: u32,
    pub sector_bitmap_size: u32,
    pub current_size: u64,
    pub disk_type: u32,
    pub parent_path: Option<String>,
    pub parent_uuid: Option<[u8; 16]>,
}

impl VhdBackend {
    pub fn open(mut file: File) -> io::Result<Self> {
        // Read footer copy (first 512 bytes — dynamic/diff VHD has footer at offset 0)
        let mut footer = [0u8; 512];
        file.seek(SeekFrom::Start(0))?;
        file.read_exact(&mut footer)?;

        let cookie = &footer[0..8];
        if cookie != b"conectix" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid VHD signature"));
        }

        let disk_type = u32::from_be_bytes(footer[60..64].try_into().unwrap());
        if disk_type != 3 && disk_type != 4 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Only Dynamic (3) and Differencing (4) VHD supported, got type {}", disk_type),
            ));
        }

        let current_size = u64::from_be_bytes(footer[48..56].try_into().unwrap());

        // Read dynamic header (starts at offset 512)
        let mut header = [0u8; 1024];
        file.seek(SeekFrom::Start(512))?;
        file.read_exact(&mut header)?;

        let header_cookie = &header[0..8];
        if header_cookie != b"cxsparse" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid VHD dynamic header signature"));
        }

        let table_offset = u64::from_be_bytes(header[16..24].try_into().unwrap());
        let max_table_entries = u32::from_be_bytes(header[28..32].try_into().unwrap());
        let vhd_block_size = u32::from_be_bytes(header[32..36].try_into().unwrap());

        // Parse parent info for differencing disk (type 4)
        let (parent_path, parent_uuid) = if disk_type == 4 {
            let uuid: [u8; 16] = header[40..56].try_into().unwrap();
            // Parent unicode name at header offset 64, 512 bytes UTF-16LE, null-terminated
            let parent_raw = &header[64..576];
            let mut name = String::new();
            for chunk in parent_raw.chunks(2) {
                if chunk.len() < 2 { break; }
                let ch = u16::from_le_bytes([chunk[0], chunk[1]]);
                if ch == 0 { break; } // null terminator
                if let Some(c) = char::from_u32(ch as u32) {
                    name.push(c);
                }
            }
            info!("VHD differencing: parent={}", name);
            (Some(name), Some(uuid))
        } else {
            (None, None)
        };

        // Sector bitmap size in sectors (512 bytes each)
        let sectors_per_block = vhd_block_size / 512;
        let mut bitmap_bytes = (sectors_per_block + 7) / 8;
        if bitmap_bytes % 512 != 0 {
            bitmap_bytes = ((bitmap_bytes / 512) + 1) * 512;
        }
        let sector_bitmap_size = bitmap_bytes;

        info!("VHD Info: size={} block_size={} table_entries={} table_offset={} type={}",
            current_size, vhd_block_size, max_table_entries, table_offset, disk_type);

        // Read BAT
        file.seek(SeekFrom::Start(table_offset))?;
        let mut bat_bytes = vec![0u8; (max_table_entries * 4) as usize];
        file.read_exact(&mut bat_bytes)?;

        let mut bat = Vec::with_capacity(max_table_entries as usize);
        for chunk in bat_bytes.chunks_exact(4) {
            bat.push(u32::from_be_bytes(chunk.try_into().unwrap()));
        }

        Ok(VhdBackend {
            file,
            bat,
            vhd_block_size,
            sector_bitmap_size,
            current_size,
            disk_type,
            parent_path,
            parent_uuid,
        })
    }

    /// Membuat VHD differencing disk (type 4) — child linked ke parent master
    pub fn create_differencing(parent_path: &str, child_path: &str) -> io::Result<()> {
        info!("Membuat VHD differencing child: {} → parent: {}", child_path, parent_path);

        // 1. Buka parent VHD untuk baca metadata
        let mut parent_options = std::fs::OpenOptions::new();
        parent_options.read(true);

        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            parent_options.share_mode(1 | 2);
        }

        let mut parent_file = parent_options.open(parent_path)?;

        // Read parent's OWN UUID from footer (offset 68-84) BEFORE opening VhdBackend
        let parent_uuid: [u8; 16] = {
            let mut uuid_buf = [0u8; 16];
            parent_file.seek(SeekFrom::Start(68))?;
            parent_file.read_exact(&mut uuid_buf)?;
            uuid_buf
        };

        let parent = VhdBackend::open(parent_file)?;

        let max_table_entries = parent.bat.len() as u32;
        let table_offset: u64 = 1536;
        let bat_size = max_table_entries as u64 * 4;
        let data_start = table_offset + bat_size;
        // Align data_start to 512-byte boundary
        let data_start = ((data_start + 511) / 512) * 512;

        // 2. Generate random UUID untuk child disk
        let child_uuid: [u8; 16] = {
            use std::time::{SystemTime, UNIX_EPOCH};
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
            let nanos = now.as_nanos();
            [
                (nanos >> 56) as u8, (nanos >> 48) as u8, (nanos >> 40) as u8, (nanos >> 32) as u8,
                (nanos >> 24) as u8, (nanos >> 16) as u8, (nanos >> 8) as u8, nanos as u8,
                0, 0, 0, 0, 0, 0, 0, 0,
            ]
        };

        // 3. Encode parent path as UTF-16LE (padded to 512 bytes)
        let parent_utf16: Vec<u16> = parent_path.encode_utf16().chain(std::iter::once(0)).collect();
        let mut parent_name_bytes = vec![0u8; 512];
        for (i, &ch) in parent_utf16.iter().take(256).enumerate() {
            let bytes = ch.to_le_bytes();
            parent_name_bytes[i * 2] = bytes[0];
            parent_name_bytes[i * 2 + 1] = bytes[1];
        }

        // 4. Build dynamic header (1024 bytes)
        let mut header = vec![0u8; 1024];
        header[0..8].copy_from_slice(b"cxsparse");
        header[16..24].copy_from_slice(&table_offset.to_be_bytes());
        header[28..32].copy_from_slice(&max_table_entries.to_be_bytes());
        header[32..36].copy_from_slice(&parent.vhd_block_size.to_be_bytes());
        // Parent UUID (own UUID, from footer)
        header[40..56].copy_from_slice(&parent_uuid);
        header[64..576].copy_from_slice(&parent_name_bytes);

        // Parent locator entry (platform code W2ku = Windows, at header[576])
        let parent_path_bytes = parent_path.as_bytes();
        let platform_code: [u8; 4] = [0x57, 0x32, 0x6B, 0x75]; // "W2ku" in ASCII
        let locator_offset: u32 = data_start as u32;
        let locator_len = parent_path_bytes.len() as u32;
        header[576..580].copy_from_slice(&platform_code);
        header[580..584].copy_from_slice(&0u32.to_be_bytes()); // platform_data_space
        header[584..588].copy_from_slice(&locator_len.to_be_bytes());
        header[588..592].copy_from_slice(&locator_offset.to_be_bytes());

        // Header checksum field at header[12..16]
        let mut header_sum: u32 = 0;
        for chunk in header.chunks(4) {
            let val = u32::from_be_bytes([chunk[0], chunk.get(1).copied().unwrap_or(0),
                chunk.get(2).copied().unwrap_or(0), chunk.get(3).copied().unwrap_or(0)]);
            header_sum = header_sum.wrapping_add(val);
        }
        let _header_checksum = !header_sum;

        // 5. Build footer (512 bytes)
        let mut footer = vec![0u8; 512];
        footer[0..8].copy_from_slice(b"conectix");               // cookie
        footer[8..12].copy_from_slice(&0x00000002u32.to_be_bytes()); // features
        footer[12..16].copy_from_slice(&0x01000000u32.to_be_bytes()); // file format version
        footer[16..24].copy_from_slice(&0xFFFFFFFFFFFFFFFFu64.to_be_bytes()); // data_offset (unused for diff)
        footer[24..28].copy_from_slice(&0u32.to_be_bytes());        // timestamp
        footer[28..32].copy_from_slice(&0u32.to_be_bytes());        // creator_app
        footer[32..36].copy_from_slice(&0u32.to_be_bytes());        // creator_version
        footer[36..40].copy_from_slice(&0u32.to_be_bytes());        // creator_host_os
        footer[40..48].copy_from_slice(&parent.current_size.to_be_bytes());   // original_size
        footer[48..56].copy_from_slice(&parent.current_size.to_be_bytes());   // current_size
        footer[56..60].copy_from_slice(&2u32.to_be_bytes()); // disk geometry: cylinders
        footer[60..64].copy_from_slice(&4u32.to_be_bytes()); // disk_type = 4 (differencing)
        footer[68..72].copy_from_slice(&child_uuid[0..4]);
        footer[72..76].copy_from_slice(&child_uuid[4..8]);
        footer[76..80].copy_from_slice(&child_uuid[8..12]);
        footer[80..84].copy_from_slice(&child_uuid[12..16]);

        // Compute footer checksum
        let mut footer_sum: u32 = 0;
        for chunk in footer.chunks(4) {
            let val = u32::from_be_bytes([chunk[0], chunk.get(1).copied().unwrap_or(0),
                chunk.get(2).copied().unwrap_or(0), chunk.get(3).copied().unwrap_or(0)]);
            footer_sum = footer_sum.wrapping_add(val);
        }
        let footer_checksum = !footer_sum;
        footer[64..68].copy_from_slice(&footer_checksum.to_be_bytes()); // checksum field

        // 6. Write child VHD file
        let mut child_options = std::fs::OpenOptions::new();
        child_options.write(true).create(true).truncate(true);

        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            child_options.share_mode(1 | 2);
        }

        let mut child_file = child_options.open(child_path)?;
        use std::io::Write;

        // VHD structure:
        //   offset 0:      footer COPY
        //   offset 512:    header (1024 bytes)
        //   offset 1536:   BAT (max_table_entries * 4)
        //   data_start:    parent path bytes
        //   EOF - 512:     footer ORIGINAL

        // Write footer copy at start
        child_file.write_all(&footer)?;

        // Write header
        child_file.write_all(&header)?;

        // Write zero BAT (all 0xFFFFFFFF = read from parent)
        let zero_bat = vec![0xFFu8; bat_size as usize];
        child_file.write_all(&zero_bat)?;

        // Write parent path after table (for parent locator)
        if data_start > child_file.seek(SeekFrom::Current(0))? {
            let diff = data_start - child_file.seek(SeekFrom::Current(0))?;
            let padding = vec![0u8; diff as usize];
            child_file.write_all(&padding)?;
        }
        child_file.write_all(parent_path_bytes)?;

        // Write original footer at EOF (required!)
        child_file.write_all(&footer)?;

        child_file.sync_all()?;

        info!("Child VHD dibuat: {} ({} bytes, {} blocks)",
            child_path, parent.current_size, max_table_entries);

        Ok(())
    }
}
