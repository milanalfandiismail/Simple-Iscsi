use dashmap::DashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use parking_lot::Mutex;
use std::io::BufWriter;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use tracing::{info, warn};

const BUFFER_SIZE: usize = 2 * 1024 * 1024; // 2MB buffer

pub struct ClientCache {
    file_path: PathBuf,
    map_path: PathBuf,
    file: Arc<Mutex<BufWriter<std::fs::File>>>,
    block_map: DashMap<u64, u64>,
    next_write_offset: AtomicU64,
    block_size: u64,
    max_cache_size: u64,
    is_super: bool,
}

impl ClientCache {
    pub fn new(
        writeback_dirs: &[String],
        client_ip: &str,
        target_name: &str,
        block_size: u64,
        max_cache_gb: u64,
        is_super: bool,
    ) -> io::Result<Self> {
        // Round-robin drive picker for load balancing across writeback drives
        static NEXT_DRIVE: AtomicUsize = AtomicUsize::new(0);
        let idx = NEXT_DRIVE.fetch_add(1, Ordering::Relaxed) % writeback_dirs.len();
        let dir = &writeback_dirs[idx];

        let dir_path = Path::new(dir);
        fs::create_dir_all(dir_path)?;

        let safe_ip = client_ip.chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' })
            .collect::<String>();
            
        let safe_target = target_name.chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect::<String>();

        let file_path = dir_path.join(format!("{}-{}.bin", safe_ip, safe_target));
        let map_path = dir_path.join(format!("{}-{}.map", safe_ip, safe_target));

        let mut next_write_offset = 0;
        let block_map = DashMap::new();

        // Load existing map if super client and file exists
        if is_super && file_path.exists() && map_path.exists() {
            if let Ok(map_content) = std::fs::read_to_string(&map_path) {
                for line in map_content.lines() {
                    let parts: Vec<&str> = line.split(':').collect();
                    if parts.len() == 2 {
                        if let (Ok(lba), Ok(offset)) = (parts[0].parse::<u64>(), parts[1].parse::<u64>()) {
                            block_map.insert(lba, offset);
                            if offset + block_size > next_write_offset {
                                next_write_offset = offset + block_size;
                            }
                        }
                    }
                }
            }
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(!is_super) // Hanya truncate jika bukan super_client
            .open(&file_path)?;

        let buffered = BufWriter::with_capacity(BUFFER_SIZE, file);

        Ok(ClientCache {
            file_path,
            map_path,
            file: Arc::new(Mutex::new(buffered)),
            block_map,
            next_write_offset: AtomicU64::new(next_write_offset),
            block_size,
            max_cache_size: max_cache_gb * 1024 * 1024 * 1024,
            is_super,
        })
    }

    pub fn read_blocks(&self, first_lba: u64, num_blocks: u32, buf: &mut [u8]) -> Option<io::Result<()>> {
        let n = num_blocks as usize;
        let mut offsets = Vec::with_capacity(n);
        for i in 0..n {
            let lba = first_lba + i as u64;
            if let Some(offset) = self.block_map.get(&lba) {
                offsets.push(*offset);
            } else {
                return None;
            }
        }

        let mut writer = self.file.lock();
        writer.flush().ok()?; // ⚠️ flush BufWriter sebelum read biar data latest
        let file = writer.get_mut();

        let mut span_start = 0;
        let block_size = self.block_size as usize;
        while span_start < n {
            let base_off = offsets[span_start];
            let mut span_end = span_start + 1;
            while span_end < n && offsets[span_end] == base_off + (span_end - span_start) as u64 * self.block_size {
                span_end += 1;
            }

            let byte_start = span_start * block_size;
            let byte_end = span_end * block_size;

            file.seek(SeekFrom::Start(base_off)).ok()?;
            file.read_exact(&mut buf[byte_start..byte_end]).ok()?;

            span_start = span_end;
        }
        Some(Ok(()))
    }

    /// Cek apakah seluruh range LBA (first_lba..first_lba+n) ada di cache
    /// dengan offset berurutan (contiguous). Cukup cek 2 blocks: pertama & terakhir.
    pub fn contains_range(&self, first_lba: u64, n: u32) -> bool {
        if n == 0 { return false; }
        if n == 1 { return self.block_map.contains_key(&first_lba); }
        let last_lba = first_lba + n as u64 - 1;
        // Cek apakah first & last ada di cache
        let first_off = match self.block_map.get(&first_lba) { Some(v) => *v, None => return false };
        let last_off  = match self.block_map.get(&last_lba)  { Some(v) => *v, None => return false };
        // Kalau contiguous, offset last harus = first + (n-1) * block_size
        last_off == first_off + (n as u64 - 1) * self.block_size
    }

    pub fn write_stream(&self, first_lba: u64, buffer_byte_offset: u64, data: &[u8]) -> io::Result<()> {
        let block_size = self.block_size as usize;
        let start_lba = first_lba + buffer_byte_offset / self.block_size;
        let num_blocks = data.len() / block_size;

        if data.len() % block_size != 0 || data.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Data harus block-aligned"));
        }

        if self.block_map.get(&start_lba).is_none() {
            let total_len = data.len() as u64;
            let base = self.next_write_offset.fetch_add(total_len, Ordering::SeqCst);

            if base + total_len > self.max_cache_size {
                return Err(io::Error::new(io::ErrorKind::OutOfMemory, "Cache full"));
            }

            for i in 0..num_blocks {
                self.block_map.insert(start_lba + i as u64, base + (i as u64) * self.block_size);
            }

            let mut writer = self.file.lock();
            writer.get_mut().seek(SeekFrom::Start(base))?;
            writer.write_all(data)?;
            writer.flush()?; // ⚠️ flush ke disk sebelum unlock!
            return Ok(());
        }

        let mut offsets = Vec::with_capacity(num_blocks);
        for i in 0..num_blocks {
            let lba = start_lba + i as u64;
            let offset = if let Some(entry) = self.block_map.get(&lba) {
                *entry
            } else {
                let off = self.next_write_offset.fetch_add(self.block_size, Ordering::SeqCst);
                if off + self.block_size > self.max_cache_size {
                    return Err(io::Error::new(io::ErrorKind::OutOfMemory, "Cache full"));
                }
                self.block_map.insert(lba, off);
                off
            };
            offsets.push(offset);
        }

        let mut writer = self.file.lock();
        let file = writer.get_mut();

        let mut span_start = 0;
        while span_start < num_blocks {
            let base_off = offsets[span_start];
            let mut span_end = span_start + 1;
            while span_end < num_blocks && offsets[span_end] == base_off + (span_end - span_start) as u64 * self.block_size {
                span_end += 1;
            }

            let byte_start = span_start * block_size;
            let byte_end = span_end * block_size;

            file.seek(SeekFrom::Start(base_off))?;
            file.write_all(&data[byte_start..byte_end])?;

            span_start = span_end;
        }
        writer.flush()?; // ⚠️ flush ke disk sebelum unlock!
        Ok(())
    }

    pub fn flush(&self) -> io::Result<()> {
        let mut writer = self.file.lock();
        writer.flush()?;
        writer.get_mut().sync_all()?;
        
        if self.is_super {
            // Save map to file
            let mut map_content = String::new();
            for entry in self.block_map.iter() {
                map_content.push_str(&format!("{}:{}\n", entry.key(), entry.value()));
            }
            if let Err(e) = std::fs::write(&self.map_path, map_content) {
                warn!("Gagal menyimpan block map {:?}: {}", self.map_path, e);
            }
        }
        
        Ok(())
    }

    pub fn cleanup(&self) {
        if self.is_super {
            info!("Sesi Super Client ditutup, cache dipertahankan di disk.");
            return; // Jangan hapus cache jika super client
        }
        
        if self.file_path.exists() {
            info!("Menghapus file cache {:?}", self.file_path);
            if let Err(e) = fs::remove_file(&self.file_path) {
                warn!("Gagal menghapus file cache {:?}: {}", self.file_path, e);
            } else {
                info!("File cache {:?} berhasil dihapus.", self.file_path);
            }
        }
        if self.map_path.exists() {
            let _ = fs::remove_file(&self.map_path);
        }
    }
}

impl Drop for ClientCache {
    fn drop(&mut self) {
        self.cleanup();
    }
}
