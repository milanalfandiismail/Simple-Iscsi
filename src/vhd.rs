use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use tracing::info;
use crate::fs_utils::{file_read_exact_at, file_write_all_at};

#[allow(dead_code)]
pub struct VhdBackend {
    pub file: File,
    pub bat: Vec<u32>,
    pub vhd_block_size: u32,
    pub sector_bitmap_size: u32,
    pub current_size: u64,
    pub disk_type: u32,
    pub table_offset: u64,
    pub max_table_entries: u32,
    pub parent_path: Option<String>,
    pub parent_uuid: Option<[u8; 16]>,
}

/// Menghitung VHD checksum sesuai spesifikasi resmi Microsoft VHD:
/// One's complement dari penjumlahan seluruh byte (u8 sebagai u32) dalam struktur header/footer,
/// dengan field checksum (4 byte) dianggap 0.
pub fn calculate_vhd_checksum(data: &[u8], checksum_offset: usize) -> u32 {
    let mut sum: u32 = 0;
    for (i, &b) in data.iter().enumerate() {
        if i >= checksum_offset && i < checksum_offset + 4 {
            continue;
        }
        sum = sum.wrapping_add(b as u32);
    }
    !sum
}

impl VhdBackend {
    pub fn open(mut file: File) -> io::Result<Self> {
        let file_len = file.metadata()?.len();
        if file_len < 512 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "File VHD terlalu kecil (< 512 bytes)"));
        }

        // 1. Coba baca footer kanonikal dari EOF - 512 (spesifikasi resmi Microsoft VHD)
        let mut footer = [0u8; 512];
        file.seek(SeekFrom::Start(file_len - 512))?;
        file.read_exact(&mut footer)?;

        // 2. Jika di EOF bukan "conectix", coba fallback ke offset 0 (copy footer pada dynamic/diff VHD)
        if &footer[0..8] != b"conectix" {
            file.seek(SeekFrom::Start(0))?;
            file.read_exact(&mut footer)?;
        }

        let cookie = &footer[0..8];
        if cookie != b"conectix" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid VHD signature"));
        }

        let disk_type = u32::from_be_bytes(footer[60..64].try_into().unwrap());
        if disk_type != 2 && disk_type != 3 && disk_type != 4 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Only Fixed (2), Dynamic (3), and Differencing (4) VHD supported, got type {}", disk_type),
            ));
        }

        let current_size = u64::from_be_bytes(footer[48..56].try_into().unwrap());
        let data_offset = u64::from_be_bytes(footer[16..24].try_into().unwrap());

        // Handle Fixed VHD (type 2) — raw flat sectors, no BAT, no dynamic header
        if disk_type == 2 {
            info!("VHD Info (Fixed VHD): size={} bytes", current_size);
            return Ok(Self {
                file,
                bat: Vec::new(),
                vhd_block_size: 512,
                sector_bitmap_size: 0,
                current_size,
                disk_type,
                table_offset: 0,
                max_table_entries: 0,
                parent_path: None,
                parent_uuid: None,
            });
        }

        // Read dynamic header (mengikuti data_offset dari footer, default 512)
        let header_offset = if data_offset >= 512 && data_offset < file_len && data_offset != 0xFFFFFFFFFFFFFFFF {
            data_offset
        } else {
            512
        };
        let mut header = [0u8; 1024];
        file.seek(SeekFrom::Start(header_offset))?;
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
            table_offset,
            max_table_entries,
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

        // Parent locator entry (24 bytes at header[576], per VHD spec):
        //   [576..580] platform_code (4)
        //   [580..584] platform_data_space (4)
        //   [584..588] platform_data_length (4)
        //   [588..592] reserved (4)
        //   [592..600] platform_data_offset (8)
        let parent_path_bytes = parent_path.as_bytes();
        let platform_code: [u8; 4] = [0x57, 0x32, 0x6B, 0x75]; // "W2ku"
        let locator_offset: u64 = data_start;
        let locator_len = parent_path_bytes.len() as u32;
        header[576..580].copy_from_slice(&platform_code);
        header[580..584].copy_from_slice(&0u32.to_be_bytes());       // platform_data_space
        header[584..588].copy_from_slice(&locator_len.to_be_bytes()); // platform_data_length
        header[588..592].copy_from_slice(&0u32.to_be_bytes());        // reserved
        header[592..600].copy_from_slice(&locator_offset.to_be_bytes()); // platform_data_offset (u64)

        // Compute header checksum (sum of all bytes with checksum field 12..16 = 0, then one's complement)
        let header_checksum = calculate_vhd_checksum(&header, 12);
        header[12..16].copy_from_slice(&header_checksum.to_be_bytes());

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

        // Compute footer checksum (sum of all bytes with checksum field 64..68 = 0, then one's complement)
        let footer_checksum = calculate_vhd_checksum(&footer, 64);
        footer[64..68].copy_from_slice(&footer_checksum.to_be_bytes());

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

        // Pad to 512-byte boundary before footer — EOF MUST be sector-aligned
        // so that bat_entry = eof/512 maps exactly to the bitmap position.
        let current_pos = child_file.seek(SeekFrom::Current(0))?;
        let aligned = ((current_pos + 511) / 512) * 512;
        if aligned > current_pos {
            let footer_pad = vec![0u8; (aligned - current_pos) as usize];
            child_file.write_all(&footer_pad)?;
        }

        // Write original footer at EOF (required!)
        child_file.write_all(&footer)?;

        child_file.sync_all()?;

        info!("Child VHD dibuat: {} ({} bytes, {} blocks)",
            child_path, parent.current_size, max_table_entries);

        Ok(())
    }


    /// Membuat dynamic VHD kosong (type 3) untuk writeback overlay.
    pub fn create_dynamic(path: &str, current_size: u64, vhd_block_size: u32) -> io::Result<()> {
        info!("Membuat dynamic VHD writeback: {} ({} bytes)", path, current_size);

        let table_offset: u64 = 1536;
        let max_table_entries = ((current_size + vhd_block_size as u64 - 1) / vhd_block_size as u64) as u32;
        let bat_size = max_table_entries as u64 * 4;
        let data_start = ((table_offset + bat_size + 511) / 512) * 512;

        let uuid_bytes = *uuid::Uuid::new_v4().as_bytes();

        let mut header = vec![0u8; 1024];
        header[0..8].copy_from_slice(b"cxsparse");
        header[16..24].copy_from_slice(&table_offset.to_be_bytes());
        header[28..32].copy_from_slice(&max_table_entries.to_be_bytes());
        header[32..36].copy_from_slice(&vhd_block_size.to_be_bytes());
        let header_checksum = calculate_vhd_checksum(&header, 12);
        header[12..16].copy_from_slice(&header_checksum.to_be_bytes());

        let mut footer = vec![0u8; 512];
        footer[0..8].copy_from_slice(b"conectix");
        footer[8..12].copy_from_slice(&0x00000002u32.to_be_bytes());
        footer[12..16].copy_from_slice(&0x01000000u32.to_be_bytes());
        footer[16..24].copy_from_slice(&512u64.to_be_bytes());
        footer[40..48].copy_from_slice(&current_size.to_be_bytes());
        footer[48..56].copy_from_slice(&current_size.to_be_bytes());
        footer[56..60].copy_from_slice(&2u32.to_be_bytes());
        footer[60..64].copy_from_slice(&3u32.to_be_bytes());
        footer[68..72].copy_from_slice(&uuid_bytes[0..4]);
        footer[72..76].copy_from_slice(&uuid_bytes[4..8]);
        footer[76..80].copy_from_slice(&uuid_bytes[8..12]);
        footer[80..84].copy_from_slice(&uuid_bytes[12..16]);
        let footer_checksum = calculate_vhd_checksum(&footer, 64);
        footer[64..68].copy_from_slice(&footer_checksum.to_be_bytes());

        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1 | 2 | 4);
        }
        let mut file = options.open(path)?;
        use std::io::Write;
        file.write_all(&footer)?;
        file.write_all(&header)?;
        file.write_all(&vec![0xFFu8; bat_size as usize])?;

        let pos = file.seek(SeekFrom::Current(0))?;
        if data_start > pos {
            file.write_all(&vec![0u8; (data_start - pos) as usize])?;
        }
        file.write_all(&footer)?;
        file.sync_all()?;
        Ok(())
    }


    /// Read from single VHD (dynamic, fixed, or simple)
        pub fn read_blocks(&self, lba: u64, block_size: u64, buf: &mut [u8]) -> io::Result<()> {
            if self.disk_type == 2 {
                let offset = lba * block_size;
                return file_read_exact_at(&self.file, offset, buf);
            }
            let mut buf_offset = 0;
            let mut current_byte_offset = lba * block_size;
            let vhd_block_size = self.vhd_block_size as u64;
    
            while buf_offset < buf.len() {
                let vhd_block_idx = current_byte_offset / vhd_block_size;
                let offset_in_vhd_block = current_byte_offset % vhd_block_size;
    
                let bytes_to_read = std::cmp::min(
                    buf.len() - buf_offset,
                    (vhd_block_size - offset_in_vhd_block) as usize
                );
    
                let bat_val = *self.bat.get(vhd_block_idx as usize).unwrap_or(&0xFFFFFFFF);
                if bat_val == 0xFFFFFFFF {
                    for i in 0..bytes_to_read {
                        buf[buf_offset + i] = 0;
                    }
                } else {
                    let physical_offset = (bat_val as u64) * 512 + (self.sector_bitmap_size as u64) + offset_in_vhd_block;
                    file_read_exact_at(&self.file, physical_offset, &mut buf[buf_offset..buf_offset + bytes_to_read])?;
                }
                buf_offset += bytes_to_read;
                current_byte_offset += bytes_to_read as u64;
            }
            Ok(())
        }

    /// Read from differencing disk: child first with sector bitmap check, fallback to parent, fallback to zeros
        pub fn diff_read_blocks(&self, parent: &Option<VhdBackend>, lba: u64, block_size: u64, buf: &mut [u8]) -> io::Result<()> {
            if self.disk_type == 2 {
                let offset = lba * block_size;
                return file_read_exact_at(&self.file, offset, buf);
            }
            let mut buf_offset = 0;
            let mut current_byte_offset = lba * block_size;
            let vhd_block_size = self.vhd_block_size as u64;
    
            while buf_offset < buf.len() {
                let vhd_block_idx = current_byte_offset / vhd_block_size;
                let offset_in_vhd_block = current_byte_offset % vhd_block_size;
    
                let bytes_to_read = std::cmp::min(
                    buf.len() - buf_offset,
                    (vhd_block_size - offset_in_vhd_block) as usize
                );
    
                let child_bat = *self.bat.get(vhd_block_idx as usize).unwrap_or(&0xFFFFFFFF);
                if child_bat != 0xFFFFFFFF {
                    // Check sector bitmap in child: only sectors with bit 1 are valid in child.
                    // Sectors with bit 0 must be read from parent (per Microsoft VHD spec).
                    let sec_start = (offset_in_vhd_block / 512) as usize;
                    let sec_count = ((bytes_to_read + 511) / 512) as usize;
                    let bitmap_phys = (child_bat as u64) * 512;
                    let bm_len = (self.sector_bitmap_size as usize).min(512);
                    let mut bitmap = [0u8; 512];
                    file_read_exact_at(&self.file, bitmap_phys, &mut bitmap[..bm_len])?;

                    let mut all_set = true;
                    let mut all_clear = true;
                    for s in sec_start..(sec_start + sec_count) {
                        let bit_set = (bitmap[s / 8] & (0x80 >> (s % 8))) != 0;
                        if bit_set {
                            all_clear = false;
                        } else {
                            all_set = false;
                        }
                    }

                    if all_set {
                        let physical_offset = (child_bat as u64) * 512 + (self.sector_bitmap_size as u64) + offset_in_vhd_block;
                        file_read_exact_at(&self.file, physical_offset, &mut buf[buf_offset..buf_offset + bytes_to_read])?;
                    } else if all_clear {
                        let cur_lba = current_byte_offset / block_size;
                        let _cur_blocks = (bytes_to_read as u64 + block_size - 1) / block_size;
                        if let Some(ref p) = parent {
                            p.read_blocks(cur_lba, block_size, &mut buf[buf_offset..buf_offset + bytes_to_read])?;
                        } else {
                            for b in &mut buf[buf_offset..buf_offset + bytes_to_read] { *b = 0; }
                        }
                    } else {
                        // Mixed: resolve sector by sector
                        for s_idx in 0..sec_count {
                            let s = sec_start + s_idx;
                            let bit_set = (bitmap[s / 8] & (0x80 >> (s % 8))) != 0;
                            let sec_buf_start = buf_offset + s_idx * 512;
                            let sec_buf_len = 512.min(buf.len() - sec_buf_start);
                            if bit_set {
                                let phys_sec = (child_bat as u64) * 512 + (self.sector_bitmap_size as u64) + (s as u64) * 512;
                                file_read_exact_at(&self.file, phys_sec, &mut buf[sec_buf_start..sec_buf_start + sec_buf_len])?;
                            } else {
                                let cur_lba = (current_byte_offset + s_idx as u64 * 512) / block_size;
                                if let Some(ref p) = parent {
                                    p.read_blocks(cur_lba, block_size, &mut buf[sec_buf_start..sec_buf_start + sec_buf_len])?;
                                } else {
                                    for b in &mut buf[sec_buf_start..sec_buf_start + sec_buf_len] { *b = 0; }
                                }
                            }
                        }
                    }
                } else if let Some(ref parent) = parent {
                    let parent_bat = *parent.bat.get(vhd_block_idx as usize).unwrap_or(&0xFFFFFFFF);
                    if parent_bat != 0xFFFFFFFF {
                        // Read from parent
                        let physical_offset = (parent_bat as u64) * 512 + (parent.sector_bitmap_size as u64) + offset_in_vhd_block;
                        file_read_exact_at(&parent.file, physical_offset, &mut buf[buf_offset..buf_offset + bytes_to_read])?;
                    } else {
                        for i in 0..bytes_to_read {
                            buf[buf_offset + i] = 0;
                        }
                    }
                } else {
                    for i in 0..bytes_to_read {
                        buf[buf_offset + i] = 0;
                    }
                }
                buf_offset += bytes_to_read;
                current_byte_offset += bytes_to_read as u64;
            }
            Ok(())
        }

    /// copy parent data first, then overlay write data.
        pub fn diff_write_blocks(&mut self, parent: &Option<VhdBackend>, lba: u64, block_size: u64, buf: &[u8]) -> io::Result<()> {
            let vhd_block_size = self.vhd_block_size as u64;
            let bitmap_size = self.sector_bitmap_size as u64;
            let start_block = (lba * block_size) / vhd_block_size;
            let end_block = ((lba * block_size + buf.len() as u64 - 1) / vhd_block_size) + 1;
    
            // Bitmap 0xFF = semua sektor pada 2MB blok terisi data valid
            let full_bitmap = vec![0xFFu8; bitmap_size as usize];
            let mut block_data = vec![0u8; vhd_block_size as usize];
            let mut bat_updates: Vec<(u64, u32)> = Vec::new();
    
            // Phase 1: Allocate + CoW copy — batch BAT updates
            for block_idx in start_block..end_block.min(self.bat.len() as u64) {
                if self.bat[block_idx as usize] == 0xFFFFFFFF {
                    let mut eof = self.file.metadata()?.len();
                    let bat_entry = (eof / 512) as u32;
    
                    // Write sector bitmap (0xFF = valid sectors)
                    file_write_all_at(&self.file, eof, &full_bitmap)?;
                    eof += full_bitmap.len() as u64;
    
                    // COPY-ON-WRITE: read full block from parent
                    if let Some(ref p) = parent {
                        let parent_bat = *p.bat.get(block_idx as usize).unwrap_or(&0xFFFFFFFF);
                        if parent_bat != 0xFFFFFFFF {
                            let parent_offset = (parent_bat as u64) * 512 + (p.sector_bitmap_size as u64);
                            file_read_exact_at(&p.file, parent_offset, &mut block_data)?;
                        } else {
                            block_data.fill(0);
                        }
                    } else {
                        block_data.fill(0);
                    }
    
                    file_write_all_at(&self.file, eof, &block_data)?;
    
                    // Buffer BAT update
                    self.bat[block_idx as usize] = bat_entry;
                    bat_updates.push((block_idx, bat_entry));
                }
            }
    
            // Batch write all BAT updates at exact table_offset
            for (blk_idx, entry) in &bat_updates {
                let off = self.table_offset + (*blk_idx * 4) as u64;
                file_write_all_at(&self.file, off, &entry.to_be_bytes())?;
            }
    
            // Phase 2: Overlay write data — contiguous blocks = 1 seek
            let mut buf_offset = 0;
            let mut current_byte_offset = lba * block_size;
            while buf_offset < buf.len() {
                let vhd_block_idx = current_byte_offset / vhd_block_size;
                let offset_in_vhd_block = current_byte_offset % vhd_block_size;
                let chunk = std::cmp::min(
                    buf.len() - buf_offset,
                    (vhd_block_size - offset_in_vhd_block) as usize
                );
                let bat_val = self.bat[vhd_block_idx as usize];
                let physical_offset = (bat_val as u64) * 512 + bitmap_size + offset_in_vhd_block;
                file_write_all_at(&self.file, physical_offset, &buf[buf_offset..buf_offset + chunk])?;
                buf_offset += chunk;
                current_byte_offset += chunk as u64;
            }
            
            Ok(())
        }

    /// Read overlay VHD dengan fallback ke raw base per sektor.
    pub fn read_overlay_blocks<F>(&self, lba: u64, block_size: u64, buf: &mut [u8], mut raw_read: F) -> io::Result<()>
    where
        F: FnMut(u64, &mut [u8]) -> io::Result<()>,
    {
        let mut buf_offset = 0usize;
        let mut current_byte_offset = lba * block_size;
        let vhd_block_size = self.vhd_block_size as u64;
        let bm_len = self.sector_bitmap_size as usize;

        while buf_offset < buf.len() {
            let block_idx = current_byte_offset / vhd_block_size;
            let offset_in = current_byte_offset % vhd_block_size;
            let bytes = std::cmp::min(buf.len() - buf_offset, (vhd_block_size - offset_in) as usize);
            let cur_lba = current_byte_offset / block_size;
            let bat_val = *self.bat.get(block_idx as usize).unwrap_or(&0xFFFFFFFF);

            if bat_val == 0xFFFFFFFF {
                raw_read(cur_lba, &mut buf[buf_offset..buf_offset + bytes])?;
            } else {
                let sec_start = (offset_in / 512) as usize;
                let sec_count = (bytes + 511) / 512;
                let mut bitmap = vec![0u8; bm_len];
                file_read_exact_at(&self.file, bat_val as u64 * 512, &mut bitmap)?;

                let mut all_set = true;
                let mut all_clear = true;
                for s in sec_start..(sec_start + sec_count) {
                    if (bitmap[s / 8] & (0x80 >> (s % 8))) != 0 {
                        all_clear = false;
                    } else {
                        all_set = false;
                    }
                }

                if all_set {
                    let phys = bat_val as u64 * 512 + self.sector_bitmap_size as u64 + offset_in;
                    file_read_exact_at(&self.file, phys, &mut buf[buf_offset..buf_offset + bytes])?;
                } else if all_clear {
                    raw_read(cur_lba, &mut buf[buf_offset..buf_offset + bytes])?;
                } else {
                    for s_idx in 0..sec_count {
                        let s = sec_start + s_idx;
                        let dst_start = buf_offset + s_idx * 512;
                        let dst_len = 512.min(buf.len() - dst_start);
                        if (bitmap[s / 8] & (0x80 >> (s % 8))) != 0 {
                            let phys = bat_val as u64 * 512 + self.sector_bitmap_size as u64 + (s as u64) * 512;
                            file_read_exact_at(&self.file, phys, &mut buf[dst_start..dst_start + dst_len])?;
                        } else {
                            let raw_lba = (current_byte_offset + s_idx as u64 * 512) / block_size;
                            raw_read(raw_lba, &mut buf[dst_start..dst_start + dst_len])?;
                        }
                    }
                }
            }

            buf_offset += bytes;
            current_byte_offset += bytes as u64;
        }
        Ok(())
    }

    /// Tulis overlay VHD: alokasikan blok, tandai bitmap, tulis sektor berubah saja.
    pub fn write_overlay_blocks(&mut self, lba: u64, block_size: u64, buf: &[u8]) -> io::Result<()> {
        if buf.is_empty() || buf.len() % 512 != 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Data overlay harus kelipatan 512"));
        }
        let mut buf_offset = 0usize;
        let mut current_byte_offset = lba * block_size;
        let vhd_block_size = self.vhd_block_size as u64;
        let bm_len = self.sector_bitmap_size as usize;

        while buf_offset < buf.len() {
            let block_idx = current_byte_offset / vhd_block_size;
            let offset_in = current_byte_offset % vhd_block_size;
            let bytes = std::cmp::min(buf.len() - buf_offset, (vhd_block_size - offset_in) as usize);
            if block_idx as usize >= self.bat.len() {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "LBA writeback melebihi ukuran disk"));
            }

            let bat_val = *self.bat.get(block_idx as usize).unwrap_or(&0xFFFFFFFF);
            let mut bitmap = vec![0u8; bm_len];
            let bat_val = if bat_val == 0xFFFFFFFF {
                let eof = self.file.metadata()?.len();
                let new_bat = (eof / 512) as u32;
                self.bat[block_idx as usize] = new_bat;
                file_write_all_at(&self.file, self.table_offset + block_idx * 4, &new_bat.to_be_bytes())?;
                self.file.set_len(eof + self.sector_bitmap_size as u64 + vhd_block_size)?;
                new_bat
            } else {
                file_read_exact_at(&self.file, bat_val as u64 * 512, &mut bitmap)?;
                bat_val
            };

            let sec_start = (offset_in / 512) as usize;
            let sec_count = (bytes + 511) / 512;
            for s in sec_start..(sec_start + sec_count) {
                bitmap[s / 8] |= 0x80 >> (s % 8);
            }
            file_write_all_at(&self.file, bat_val as u64 * 512, &bitmap)?;
            let phys = bat_val as u64 * 512 + self.sector_bitmap_size as u64 + offset_in;
            file_write_all_at(&self.file, phys, &buf[buf_offset..buf_offset + bytes])?;

            buf_offset += bytes;
            current_byte_offset += bytes as u64;
        }
        Ok(())
    }


    /// Write blocks to VHD — allocate new BAT entries, update sector bitmap
        pub fn write_blocks(&mut self, lba: u64, block_size: u64, buf: &[u8]) -> io::Result<()> {
            if self.disk_type == 2 {
                let offset = lba * block_size;
                return file_write_all_at(&self.file, offset, buf);
            }
            let vhd_block_size = self.vhd_block_size as u64;
            let bitmap_size = self.sector_bitmap_size as u64;
            let start_block = (lba * block_size) / vhd_block_size;
            let end_block = ((lba * block_size + buf.len() as u64 - 1) / vhd_block_size) + 1;
    
            let full_bitmap = vec![0xFFu8; bitmap_size as usize];
            let zero_data = vec![0u8; vhd_block_size as usize];
            let mut bat_updates: Vec<(u64, u32)> = Vec::new();
    
            // Phase 1: Allocate new blocks — batch BAT updates
            for block_idx in start_block..end_block.min(self.bat.len() as u64) {
                if self.bat[block_idx as usize] == 0xFFFFFFFF {
                    let mut eof = self.file.metadata()?.len();
                    let bat_entry = (eof / 512) as u32;
    
                    // Write bitmap + zero data
                    file_write_all_at(&self.file, eof, &full_bitmap)?;
                    eof += full_bitmap.len() as u64;
                    
                    file_write_all_at(&self.file, eof, &zero_data)?;
    
                    self.bat[block_idx as usize] = bat_entry;
                    bat_updates.push((block_idx, bat_entry));
                }
            }
    
            // Batch write all BAT updates at exact table_offset
            for (blk_idx, entry) in &bat_updates {
                let off = self.table_offset + (*blk_idx * 4) as u64;
                file_write_all_at(&self.file, off, &entry.to_be_bytes())?;
            }
    
            // Phase 2: Write data — contiguous per VHD block = 1 seek per VHD block
            let mut buf_offset = 0;
            let mut current_byte_offset = lba * block_size;
            while buf_offset < buf.len() {
                let vhd_block_idx = current_byte_offset / vhd_block_size;
                let offset_in_vhd_block = current_byte_offset % vhd_block_size;
                let chunk = std::cmp::min(
                    buf.len() - buf_offset,
                    (vhd_block_size - offset_in_vhd_block) as usize
                );
                let bat_val = self.bat[vhd_block_idx as usize];
                let physical_offset = (bat_val as u64) * 512 + bitmap_size + offset_in_vhd_block;
                file_write_all_at(&self.file, physical_offset, &buf[buf_offset..buf_offset + chunk])?;
                buf_offset += chunk;
                current_byte_offset += chunk as u64;
            }
    
            Ok(())
        }

    pub fn write_blocks_concurrent(&self, lba: u64, block_size: u64, buf: &[u8]) -> io::Result<()> {
            if self.disk_type == 2 {
                let offset = lba * block_size;
                return file_write_all_at(&self.file, offset, buf);
            }
            let vhd_block_size = self.vhd_block_size as u64;
            let bitmap_size = self.sector_bitmap_size as u64;
            let mut buf_offset = 0;
            let mut current_byte_offset = lba * block_size;
            
            while buf_offset < buf.len() {
                let vhd_block_idx = current_byte_offset / vhd_block_size;
                let offset_in_vhd_block = current_byte_offset % vhd_block_size;
                let chunk = std::cmp::min(
                    buf.len() - buf_offset,
                    (vhd_block_size - offset_in_vhd_block) as usize
                );
                
                let bat_val = self.bat[vhd_block_idx as usize];
                if bat_val == 0xFFFFFFFF {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "concurrent write hit unallocated block"));
                }
                
                let physical_offset = (bat_val as u64) * 512 + bitmap_size + offset_in_vhd_block;
                file_write_all_at(&self.file, physical_offset, &buf[buf_offset..buf_offset + chunk])?;
                
                buf_offset += chunk;
                current_byte_offset += chunk as u64;
            }
            Ok(())
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dynamic_overlay_raw_fallback() {
        let dir = std::env::temp_dir();
        let raw_path = dir.join(format!("raw_overlay_{}.bin", uuid::Uuid::new_v4()));
        let vhd_path = dir.join(format!("overlay_{}.vhd", uuid::Uuid::new_v4()));
        let block_size = 512u64;
        let total = 4 * 1024 * 1024u64;
        let raw = vec![0x11u8; total as usize];
        std::fs::write(&raw_path, &raw).unwrap();

        VhdBackend::create_dynamic(vhd_path.to_str().unwrap(), total, 2 * 1024 * 1024).unwrap();
        let f = std::fs::OpenOptions::new().read(true).write(true).open(&vhd_path).unwrap();
        let mut vhd = VhdBackend::open(f).unwrap();
        vhd.write_overlay_blocks(0, block_size, &vec![0xAAu8; 512]).unwrap();

        let mut buf = vec![0u8; 1024];
        vhd.read_overlay_blocks(0, block_size, &mut buf, |lba, slice| {
            let off = (lba * block_size) as usize;
            slice.copy_from_slice(&raw[off..off + slice.len()]);
            Ok(())
        }).unwrap();
        assert_eq!(&buf[0..4], &[0xAAu8; 4]);
        assert_eq!(&buf[512..516], &[0x11u8; 4]);

        let meta = std::fs::metadata(&vhd_path).unwrap().len();
        assert!(meta < total, "overlay harus lebih kecil dari virtual disk");
        let _ = std::fs::remove_file(&vhd_path);
        let _ = std::fs::remove_file(&raw_path);
    }
    use std::io::Write;

    #[test]
    fn test_fixed_vhd_detection_and_read() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_fixed.vhd");
        let path_str = path.to_str().unwrap();

        // Create a mock fixed VHD: 1 MB data + 512-byte footer at EOF
        {
            let mut file = File::create(&path).unwrap();
            let data = vec![0xABu8; 1024 * 1024]; // 1MB payload
            file.write_all(&data).unwrap();

            let mut footer = vec![0u8; 512];
            footer[0..8].copy_from_slice(b"conectix");
            footer[16..24].copy_from_slice(&0xFFFFFFFFFFFFFFFFu64.to_be_bytes()); // data_offset for fixed
            footer[48..56].copy_from_slice(&(1024 * 1024u64).to_be_bytes());
            footer[60..64].copy_from_slice(&2u32.to_be_bytes()); // type 2 = Fixed
            file.write_all(&footer).unwrap();
        }

        // Open and read
        let file = File::open(&path).unwrap();
        let vhd = VhdBackend::open(file).unwrap();
        assert_eq!(vhd.disk_type, 2);
        assert_eq!(vhd.current_size, 1024 * 1024);

        let mut buf = vec![0u8; 1024];
        vhd.read_blocks(0, 512, &mut buf).unwrap();
        assert_eq!(buf[0], 0xAB);
        assert_eq!(buf[1023], 0xAB);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_diff_vhd_sector_bitmap_resolution() {
        let dir = std::env::temp_dir();
        let parent_path = dir.join("test_parent.vhd");
        let child_path = dir.join("test_child.vhd");

        let block_size: u32 = 2 * 1024 * 1024; // 2MB
        let sectors_per_block = block_size / 512; // 4096
        let _bitmap_bytes: u32 = 512;

        // 1. Create Parent VHD with block 0 filled with 0x11
        {
            let mut f = File::create(&parent_path).unwrap();
            // Footer at start
            let mut footer = vec![0u8; 512];
            footer[0..8].copy_from_slice(b"conectix");
            footer[16..24].copy_from_slice(&512u64.to_be_bytes());
            footer[48..56].copy_from_slice(&(block_size as u64).to_be_bytes());
            footer[60..64].copy_from_slice(&3u32.to_be_bytes()); // dynamic
            f.write_all(&footer).unwrap();

            // Dynamic header
            let mut header = vec![0u8; 1024];
            header[0..8].copy_from_slice(b"cxsparse");
            header[16..24].copy_from_slice(&1536u64.to_be_bytes()); // table_offset
            header[28..32].copy_from_slice(&1u32.to_be_bytes());   // max_entries
            header[32..36].copy_from_slice(&block_size.to_be_bytes());
            f.write_all(&header).unwrap();

            // BAT: entry 0 points to sector 4 (byte 2048)
            f.write_all(&4u32.to_be_bytes()).unwrap();
            // Pad BAT to 512 boundary
            f.write_all(&vec![0u8; 508]).unwrap();

            // Data block at byte 2048:
            // Sector bitmap (all 1s)
            f.write_all(&vec![0xFFu8; 512]).unwrap();
            // 2MB data filled with 0x11
            f.write_all(&vec![0x11u8; block_size as usize]).unwrap();

            // Footer at EOF
            f.write_all(&footer).unwrap();
        }

        // 2. Create Child Differencing VHD:
        // Sector bitmap has bit 1 set (sector 1 modified with 0x22), but bit 0 is CLEAR (must read from parent 0x11)
        {
            let mut f = File::create(&child_path).unwrap();
            // Footer at start
            let mut footer = vec![0u8; 512];
            footer[0..8].copy_from_slice(b"conectix");
            footer[16..24].copy_from_slice(&512u64.to_be_bytes());
            footer[48..56].copy_from_slice(&(block_size as u64).to_be_bytes());
            footer[60..64].copy_from_slice(&4u32.to_be_bytes()); // diff
            f.write_all(&footer).unwrap();

            // Dynamic header
            let mut header = vec![0u8; 1024];
            header[0..8].copy_from_slice(b"cxsparse");
            header[16..24].copy_from_slice(&1536u64.to_be_bytes());
            header[28..32].copy_from_slice(&1u32.to_be_bytes());
            header[32..36].copy_from_slice(&block_size.to_be_bytes());
            f.write_all(&header).unwrap();

            // BAT: entry 0 points to sector 4
            f.write_all(&4u32.to_be_bytes()).unwrap();
            f.write_all(&vec![0u8; 508]).unwrap();

            // Data block at byte 2048:
            // Sector bitmap: byte 0 = 0b01000000 (0x40 -> bit 1 is set for sector 1, bit 0 is clear for sector 0)
            let mut bitmap = vec![0u8; 512];
            bitmap[0] = 0x40; // Sector 1 is modified, sector 0 is not!
            f.write_all(&bitmap).unwrap();

            // Block data: Sector 0 = 0x00, Sector 1 = 0x22
            let mut block_data = vec![0u8; block_size as usize];
            for i in 512..1024 {
                block_data[i] = 0x22;
            }
            f.write_all(&block_data).unwrap();

            // Footer at EOF
            f.write_all(&footer).unwrap();
        }

        // Test reading through diff_read_blocks
        let parent_file = File::open(&parent_path).unwrap();
        let parent_backend = VhdBackend::open(parent_file).unwrap();

        let child_file = File::open(&child_path).unwrap();
        let child_backend = VhdBackend::open(child_file).unwrap();

        let mut read_buf = vec![0u8; 1024]; // 2 sectors: sector 0 and sector 1
        child_backend.diff_read_blocks(&Some(parent_backend), 0, 512, &mut read_buf).unwrap();

        // Sector 0 (first 512 bytes) must have come from parent (0x11)!
        assert_eq!(read_buf[0], 0x11, "Sector 0 must be read from parent disk (0x11)");
        assert_eq!(read_buf[511], 0x11, "Sector 0 end must be read from parent disk (0x11)");

        // Sector 1 (next 512 bytes) must have come from child (0x22)!
        assert_eq!(read_buf[512], 0x22, "Sector 1 must be read from child disk (0x22)");
        assert_eq!(read_buf[1023], 0x22, "Sector 1 end must be read from child disk (0x22)");

        let _ = std::fs::remove_file(parent_path);
        let _ = std::fs::remove_file(child_path);
    }
}
