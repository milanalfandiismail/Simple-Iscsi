use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use tracing::{info, warn};
use crate::backend::Backend;

use crate::fs_utils::{file_read_exact_at, file_write_all_at};

use dashmap::DashMap;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

const CACHE_VERSION: u32 = 3; // bump to auto-invalidate stale 4KB cluster maps

pub struct ClientCache {
    file_path: PathBuf,
    map_path: PathBuf,
    file_read: Option<Arc<std::fs::File>>,
    file_write: Option<Arc<std::fs::File>>,
    block_map: DashMap<u64, u64>, // LBA -> offset in cache.bin
    next_write_offset: AtomicU64,
    block_size: u64,
    max_cache_size: u64,
    is_super: bool,
    total_bytes_written: AtomicU64,
    max_write_bytes_per_sec: u64,
    throttle_bytes_this_window: AtomicU64,
    throttle_window_start: AtomicU64,
}

impl ClientCache {
    pub fn new(
        writeback_dirs: &[String],
        client_ip: &str,
        target_name: &str,
        block_size: u64,
        max_cache_gb: u64,
        is_super: bool,
        max_write_speed_mbps: u64,
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

        // Load existing map — ALL clients need .map to see .bin data.
        // Without .map, .bin block offsets are unknown → reads go to raw disk → corrupt.
        if file_path.exists() && map_path.exists() {
            // Verify .bin size — must be >= next_offset from .map
            let bin_size = std::fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0);
            if let Ok(map_content) = std::fs::read_to_string(&map_path) {
                let mut map_version: u32 = 0;
                let mut lines_iter = map_content.lines();
                // First line must be version marker: "@V:1"
                if let Some(ver_line) = lines_iter.next() {
                    if let Some(v) = ver_line.strip_prefix("@V:") {
                        map_version = v.parse().unwrap_or(0);
                    }
                }
                if map_version != CACHE_VERSION {
                    warn!(".map version {} != {} — cache dari kode lama, menghapus stale .bin", map_version, CACHE_VERSION);
                    let _ = std::fs::remove_file(&map_path);
                    let _ = std::fs::remove_file(&file_path);
                    // fall through to fresh state (block_map empty, next_write_offset=0)
                } else {
                    for line in lines_iter {
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
                    // Defensive: if .map points beyond .bin size, cache is corrupt → clear
                    if next_write_offset > bin_size {
                        warn!(".map inconsistent dgn .bin (map_next={} > bin_size={}) — menghapus cache stale", next_write_offset, bin_size);
                        block_map.clear();
                        next_write_offset = 0;
                        let _ = std::fs::remove_file(&map_path);
                        let _ = std::fs::remove_file(&file_path);
                    } else {
                        info!("Memuat {} block dari .map v{} (next_offset={} of {}MB .bin)", block_map.len(), map_version, next_write_offset, bin_size / 1048576);
                    }
                }
            }
        }

        let mut file_options = OpenOptions::new();
        file_options.write(true).create(true).read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            file_options.share_mode(1 | 2); // FILE_SHARE_READ | FILE_SHARE_WRITE
        }
        let file_handle = file_options.open(&file_path)?;
        let file_handle_arc = Arc::new(file_handle);

        // Periodically sync file writes to disk in the background (disabled to prevent FlushFileBuffers locks)
        /*
        let file_write_weak = Arc::downgrade(&file_handle_arc);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3));
                if let Some(file) = file_write_weak.upgrade() {
                    let _ = file.sync_all();
                } else {
                    break;
                }
            }
        });
        */

        Ok(Self {
            file_path,
            map_path,
            file_read: Some(file_handle_arc.clone()),
            file_write: Some(file_handle_arc),
            block_map,
            next_write_offset: AtomicU64::new(next_write_offset),
            block_size,
            max_cache_size: max_cache_gb * 1024 * 1024 * 1024,
            is_super,
            total_bytes_written: AtomicU64::new(next_write_offset),
            max_write_bytes_per_sec: max_write_speed_mbps * 1024 * 1024,
            throttle_bytes_this_window: AtomicU64::new(0),
            throttle_window_start: AtomicU64::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64
            ),
        })
    }

    pub fn contains_lba(&self, lba: u64) -> bool {
        self.block_map.contains_key(&lba)
    }

    pub fn read_blocks_cached(
        &self,
        backend: &Backend,
        first_lba: u64,
        num_blocks: u32,
        buf: &mut [u8],
    ) -> io::Result<()> {
        let n = num_blocks as usize;
        let block_size = self.block_size as usize;

        let mut i = 0;
        while i < n {
            let lba = first_lba + i as u64;
            if let Some(offset_ref) = self.block_map.get(&lba) {
                // Blok yang ada di cache (contigous)
                let start_idx = i;
                let mut current_off = *offset_ref;
                let base_off = current_off;
                i += 1;
                while i < n {
                    let next_lba = first_lba + i as u64;
                    if let Some(next_off_ref) = self.block_map.get(&next_lba) {
                        let next_off = *next_off_ref;
                        if next_off == current_off + self.block_size {
                            current_off = next_off;
                            i += 1;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                let byte_start = start_idx * block_size;
                let byte_end = i * block_size;
                file_read_exact_at(self.file_read.as_ref().unwrap(), base_off, &mut buf[byte_start..byte_end])?;
            } else {
                // Blok yang tidak ada di cache (baca dari base VHD)
                let start_idx = i;
                i += 1;
                while i < n {
                    let next_lba = first_lba + i as u64;
                    if self.block_map.contains_key(&next_lba) {
                        break;
                    }
                    i += 1;
                }
                let span_blocks = (i - start_idx) as u32;
                let byte_start = start_idx * block_size;
                let byte_end = i * block_size;
                let start_lba_of_run = first_lba + start_idx as u64;
                backend.read_blocks(start_lba_of_run, span_blocks, &mut buf[byte_start..byte_end])?;
            }
        }
        Ok(())
    }

    pub async fn throttle_write_async(&self, bytes_to_write: usize) {
        if self.max_write_bytes_per_sec == 0 {
            return;
        }

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let window_start = self.throttle_window_start.load(Ordering::Relaxed);
        let elapsed = now_ms.saturating_sub(window_start);

        if elapsed >= 100 {
            self.throttle_window_start.store(now_ms, Ordering::Relaxed);
            self.throttle_bytes_this_window.store(0, Ordering::Relaxed);
        }

        let max_per_window = self.max_write_bytes_per_sec / 10;
        let written = self.throttle_bytes_this_window.fetch_add(bytes_to_write as u64, Ordering::Relaxed);

        if written + bytes_to_write as u64 > max_per_window {
            let sleep_ms = 100u64.saturating_sub(elapsed);
            if sleep_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(sleep_ms)).await;
            }
        }
    }

    pub fn write_stream(&self, first_lba: u64, buffer_byte_offset: u64, data: &[u8]) -> io::Result<()> {
        let block_size = self.block_size as usize;
        let start_lba = first_lba + buffer_byte_offset / self.block_size;
        let num_blocks = data.len() / block_size;

        if data.len() % block_size != 0 || data.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Data harus block-aligned"));
        }

        // Count new blocks for capacity check
        let mut new_blocks_count = 0;
        for i in 0..num_blocks {
            let lba = start_lba + i as u64;
            if !self.block_map.contains_key(&lba) {
                new_blocks_count += 1;
            }
        }

        if new_blocks_count > 0 {
            let total_needed = (new_blocks_count as u64) * self.block_size;
            self.ensure_capacity(total_needed)?;
        }

        // ALWAYS APPEND: Write the entire buffer sequentially at the end of the bin file!
        let total_write_len = data.len() as u64;
        let base_offset = self.next_write_offset.fetch_add(total_write_len, Ordering::SeqCst);
        self.total_bytes_written.fetch_add((new_blocks_count as u64) * self.block_size, Ordering::SeqCst);

        // Update the block map to point to the new offsets in the appended data
        for i in 0..num_blocks {
            let lba = start_lba + i as u64;
            let off = base_offset + (i as u64) * self.block_size;
            self.block_map.insert(lba, off);
        }

        file_write_all_at(self.file_write.as_ref().unwrap(), base_offset, data)?;

        Ok(())
    }

    fn ensure_capacity(&self, needed: u64) -> io::Result<()> {
        let current = self.total_bytes_written.load(Ordering::Relaxed);
        if current + needed <= self.max_cache_size {
            return Ok(());
        }

        let batch_free = (256 * 1024 * 1024).min(self.max_cache_size / 10).max(self.block_size);
        let to_free = ((current + needed) - self.max_cache_size).max(batch_free);
        
        let evict_threshold = (current + needed).saturating_sub(self.max_cache_size).saturating_add(to_free);

        let mut freed_blocks = 0;
        self.block_map.retain(|_lba, off| {
            if *off < evict_threshold {
                freed_blocks += 1;
                false // Evict
            } else {
                true // Keep
            }
        });

        let freed_bytes = (freed_blocks as u64) * self.block_size;
        self.total_bytes_written.fetch_sub(freed_bytes.min(current), Ordering::SeqCst);
        
        info!(
            "Writeback Cache Eviction: Berhasil membebaskan {} MB (threshold offset < {})",
            freed_bytes / 1048576,
            evict_threshold
        );

        Ok(())
    }

    fn save_map(&self) {
        let mut map_content = format!("@V:{}\n", CACHE_VERSION);
        for entry in self.block_map.iter() {
            map_content.push_str(&format!("{}:{}\n", entry.key(), entry.value()));
        }
        if let Err(e) = std::fs::write(&self.map_path, map_content) {
            warn!("Gagal menyimpan block map {:?}: {}", self.map_path, e);
        }
    }


    pub fn flush(&self) -> io::Result<()> {
        Ok(())
    }

}

impl Drop for ClientCache {
    fn drop(&mut self) {
        self.save_map();
        
        self.file_read.take();
        self.file_write.take();
        
        if self.is_super {
            info!("Sesi Super Client ditutup, cache dipertahankan di disk.");
            return;
        }
        
        if self.file_path.exists() {
            info!("Menghapus file cache {:?}", self.file_path);
            if let Err(e) = std::fs::remove_file(&self.file_path) {
                warn!("Gagal menghapus file cache {:?}: {}", self.file_path, e);
            } else {
                info!("File cache {:?} berhasil dihapus.", self.file_path);
            }
        }
        if self.map_path.exists() {
            let _ = std::fs::remove_file(&self.map_path);
        }
    }
}
