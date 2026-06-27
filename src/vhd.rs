use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use tracing::info;

pub struct VhdBackend {
    pub file: File,
    pub bat: Vec<u32>,
    pub vhd_block_size: u32,
    pub sector_bitmap_size: u32,
    pub current_size: u64,
}

impl VhdBackend {
    pub fn open(mut file: File) -> io::Result<Self> {
        // Read footer (first 512 bytes)
        let mut footer = [0u8; 512];
        file.seek(SeekFrom::Start(0))?;
        file.read_exact(&mut footer)?;

        let cookie = &footer[0..8];
        if cookie != b"conectix" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid VHD signature"));
        }

        let disk_type = u32::from_be_bytes(footer[60..64].try_into().unwrap());
        if disk_type != 3 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Only Dynamic VHD (type 3) is supported"));
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

        // Sector bitmap size in sectors (512 bytes each)
        // bitmap_size (in bytes) = block_size / 512 / 8
        // padded to 512-byte boundary
        let sectors_per_block = vhd_block_size / 512;
        let mut bitmap_bytes = (sectors_per_block + 7) / 8;
        if bitmap_bytes % 512 != 0 {
            bitmap_bytes = ((bitmap_bytes / 512) + 1) * 512;
        }
        let sector_bitmap_size = bitmap_bytes;

        info!("VHD Info: size={} block_size={} table_entries={} table_offset={}", 
            current_size, vhd_block_size, max_table_entries, table_offset);

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
        })
    }
}
