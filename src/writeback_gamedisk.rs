use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

use crate::backend::Backend;
use crate::fs_utils::{file_read_exact_at, file_write_all_at};

const BLOCK_SIZE: u64 = 4096;

struct CacheInner {
    file: File,
    map: HashMap<u64, u64>, // block index -> byte offset in .bin
    next_offset: u64,
}

pub struct ClientCache {
    file_path: PathBuf,
    inner: Mutex<CacheInner>,
    base: Arc<Backend>,
    base_block_size: u64,
    block_size: u64,
    total_size: u64,
    is_super: bool,
}

impl ClientCache {
    pub fn new(
        writeback_dirs: &[String],
        client_ip: &str,
        target_name: &str,
        base: Arc<Backend>,
        _max_cache_gb: u64,
        is_super: bool,
        _max_write_speed_mbps: u64,
    ) -> io::Result<Self> {
        if writeback_dirs.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "writeback_dirs kosong"));
        }

        // No cap by user decision. Monitor free space on the writeback disk.

        static NEXT_DRIVE: AtomicUsize = AtomicUsize::new(0);
        let idx = NEXT_DRIVE.fetch_add(1, Ordering::Relaxed) % writeback_dirs.len();
        let dir_path = Path::new(&writeback_dirs[idx]);
        fs::create_dir_all(dir_path)?;

        let safe_ip: String = client_ip
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' })
            .collect();
        let safe_target: String = target_name
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        let file_path = dir_path.join(format!(
            "{}-{}-{}.bin",
            safe_ip,
            safe_target,
            uuid::Uuid::new_v4()
        ));

        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1 | 2 | 4); // READ | WRITE | DELETE
        }
        let file = options.open(&file_path)?;

        let base_block_size = base.block_size();
        if base_block_size == 0 {
            let _ = fs::remove_file(&file_path);
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "block_size base 0"));
        }
        let block_size = BLOCK_SIZE.max(base_block_size);
        if block_size % base_block_size != 0 {
            let _ = fs::remove_file(&file_path);
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "block_size base tidak kompatibel dengan .bin",
            ));
        }
        let total_size = base.total_size();
        if total_size == 0 || total_size % base_block_size != 0 {
            let _ = fs::remove_file(&file_path);
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "ukuran base tidak valid untuk .bin",
            ));
        }

        info!("Writeback .bin dibuat: {:?} (blok {} byte)", file_path, block_size);

        Ok(Self {
            file_path,
            inner: Mutex::new(CacheInner {
                file,
                map: HashMap::new(),
                next_offset: 0,
            }),
            base,
            base_block_size,
            block_size,
            total_size,
            is_super,
        })
    }

    pub fn dir(&self) -> String {
        self.file_path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    pub fn has_any_lba(&self, first_lba: u64, num_blocks: u32) -> bool {
        if num_blocks == 0 {
            return false;
        }
        let start_byte = first_lba.saturating_mul(self.base_block_size);
        if start_byte >= self.total_size {
            return false;
        }
        let end_byte = start_byte
            .saturating_add((num_blocks as u64).saturating_mul(self.base_block_size))
            .min(self.total_size);
        if end_byte <= start_byte {
            return false;
        }
        let start_block = start_byte / self.block_size;
        let end_block = (end_byte - 1) / self.block_size;
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        (start_block..=end_block).any(|block| inner.map.contains_key(&block))
    }

    pub fn try_read_ram_cache(
        &self,
        _first_lba: u64,
        _num_blocks: u32,
        _buf: &mut [u8],
    ) -> Option<()> {
        None
    }

    pub fn read_blocks_cached(
        &self,
        base: &Backend,
        first_lba: u64,
        num_blocks: u32,
        buf: &mut [u8],
    ) -> io::Result<()> {
        let read_len = (num_blocks as u64).saturating_mul(self.base_block_size);
        if buf.len() < read_len as usize {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "buffer terlalu kecil"));
        }
        let start_byte = first_lba.saturating_mul(self.base_block_size);
        if start_byte >= self.total_size {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "LBA melebihi ukuran disk"));
        }
        let max_len = read_len.min(self.total_size - start_byte);

        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut block_buf = vec![0u8; self.block_size as usize];
        let mut done = 0u64;
        let mut block = start_byte / self.block_size;
        let mut in_block = (start_byte % self.block_size) as usize;

        // shortcut: one base read per 4KB block; coalesce contiguous unallocated blocks if boot I/O is slow.
        while done < max_len {
            let block_start_byte = block * self.block_size;
            let block_len = self.block_size.min(self.total_size - block_start_byte);
            let bytes = (block_len as usize - in_block).min((max_len - done) as usize);
            if bytes == 0 {
                break;
            }

            if let Some(&offset) = inner.map.get(&block) {
                file_read_exact_at(
                    &inner.file,
                    offset + in_block as u64,
                    &mut buf[done as usize..done as usize + bytes],
                )?;
            } else {
                let lba = block_start_byte / self.base_block_size;
                let blocks = (block_len / self.base_block_size) as u32;
                block_buf[..block_len as usize].fill(0);
                base.read_blocks(
                    lba,
                    blocks,
                    &mut block_buf[..block_len as usize],
                )?;
                buf[done as usize..done as usize + bytes]
                    .copy_from_slice(&block_buf[in_block..in_block + bytes]);
            }

            done += bytes as u64;
            block += 1;
            in_block = 0;
        }

        Ok(())
    }

    pub fn write_stream(
        &self,
        first_lba: u64,
        buffer_byte_offset: u64,
        data: &[u8],
    ) -> io::Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        let start_byte = first_lba
            .saturating_mul(self.base_block_size)
            .saturating_add(buffer_byte_offset);
        let end_byte = start_byte.saturating_add(data.len() as u64);
        if end_byte > self.total_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "write melebihi ukuran disk",
            ));
        }

        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut block_buf = vec![0u8; self.block_size as usize];
        let mut written = 0usize;
        let mut block = start_byte / self.block_size;
        let mut in_block = (start_byte % self.block_size) as usize;

        while written < data.len() {
            let block_start_byte = block * self.block_size;
            let block_len = self.block_size.min(self.total_size - block_start_byte);
            let bytes = (block_len as usize - in_block).min(data.len() - written);
            if bytes == 0 {
                break;
            }

            if let Some(&offset) = inner.map.get(&block) {
                file_write_all_at(
                    &inner.file,
                    offset + in_block as u64,
                    &data[written..written + bytes],
                )?;
            } else {
                let offset = inner.next_offset;
                inner.next_offset += self.block_size;

                if in_block == 0 && bytes as u64 == block_len {
                    file_write_all_at(&inner.file, offset, &data[written..written + bytes])?;
                } else {
                    let lba = block_start_byte / self.base_block_size;
                    let blocks = (block_len / self.base_block_size) as u32;
                    block_buf[..block_len as usize].fill(0);
                    self.base.read_blocks(
                        lba,
                        blocks,
                        &mut block_buf[..block_len as usize],
                    )?;
                    block_buf[in_block..in_block + bytes]
                        .copy_from_slice(&data[written..written + bytes]);
                    file_write_all_at(
                        &inner.file,
                        offset,
                        &block_buf[..block_len as usize],
                    )?;
                }

                inner.map.insert(block, offset);
            }

            written += bytes;
            block += 1;
            in_block = 0;
        }

        Ok(())
    }

    pub fn flush(&self) -> io::Result<()> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.file.sync_all()
    }
}

impl Drop for ClientCache {
    fn drop(&mut self) {
        if self.is_super {
            info!("Sesi Super Client ditutup, .bin dipertahankan: {:?}", self.file_path);
            return;
        }
        if self.file_path.exists() {
            info!("Menghapus writeback .bin {:?}", self.file_path);
            if let Err(e) = fs::remove_file(&self.file_path) {
                warn!("Gagal menghapus writeback .bin {:?}: {}", self.file_path, e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn test_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("simpleiscsi_bin_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn test_base(dir: &Path) -> Arc<Backend> {
        let raw_path = dir.join("raw_base.bin");
        let mut file = File::create(&raw_path).unwrap();
        file.write_all(&vec![0x11u8; 1024 * 1024]).unwrap();
        file.flush().unwrap();
        Arc::new(
            Backend::new_raw_mb(
                raw_path.to_str().unwrap(),
                512,
                "TEST",
                "Base",
                "1.00",
                0,
            )
            .unwrap(),
        )
    }

    fn test_cache(dir: &Path, base: Arc<Backend>) -> ClientCache {
        ClientCache::new(
            &[dir.to_string_lossy().to_string()],
            "192.168.1.10",
            "gamedisk_lun0",
            base,
            0,
            false,
            0,
        )
        .unwrap()
    }

    #[test]
    fn partial_write_reads_fallback_from_base() {
        let dir = test_dir();
        let base = test_base(&dir);
        let cache = test_cache(&dir, Arc::clone(&base));

        cache.write_stream(0, 0, &vec![0xAAu8; 512]).unwrap();
        let mut buf = vec![0u8; 1024];
        cache.read_blocks_cached(&base, 0, 2, &mut buf).unwrap();
        assert_eq!(&buf[0..4], &[0xAAu8; 4]);
        assert_eq!(&buf[512..516], &[0x11u8; 4]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rewrite_same_block_does_not_grow_file() {
        let dir = test_dir();
        let base = test_base(&dir);
        let cache = test_cache(&dir, Arc::clone(&base));

        cache.write_stream(0, 0, &vec![0xAAu8; 512]).unwrap();
        let size_after_first = fs::metadata(&cache.file_path).unwrap().len();
        cache.write_stream(0, 0, &vec![0xBBu8; 512]).unwrap();
        let size_after_second = fs::metadata(&cache.file_path).unwrap().len();
        assert_eq!(size_after_first, size_after_second);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn has_any_lba_tracks_only_written_blocks() {
        let dir = test_dir();
        let base = test_base(&dir);
        let cache = test_cache(&dir, Arc::clone(&base));

        assert!(!cache.has_any_lba(0, 8));
        cache.write_stream(0, 0, &vec![0xAAu8; 512]).unwrap();
        assert!(cache.has_any_lba(0, 8));
        assert!(!cache.has_any_lba(8, 8));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scattered_4k_writes_allocate_only_touched_blocks() {
        let dir = test_dir();
        let base = test_base(&dir);
        let cache = test_cache(&dir, Arc::clone(&base));

        for i in 0..16u64 {
            let lba = (i * 65536) / 512;
            cache
                .write_stream(lba, 0, &vec![0xAAu8; 4096])
                .unwrap();
        }

        let size = fs::metadata(&cache.file_path).unwrap().len();
        assert_eq!(size, 16 * 4096);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn drop_deletes_bin_file() {
        let dir = test_dir();
        let base = test_base(&dir);
        let cache = test_cache(&dir, Arc::clone(&base));
        cache.write_stream(0, 0, &vec![0xAAu8; 512]).unwrap();
        let path = cache.file_path.clone();
        assert!(path.exists());
        drop(cache);
        assert!(!path.exists());

        let _ = fs::remove_dir_all(&dir);
    }
}
