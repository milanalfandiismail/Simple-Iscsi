use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use tracing::{info, warn};
use crate::backend::Backend;

use crate::fs_utils::{file_read_exact_at, file_write_all_at};

use dashmap::DashMap;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

const CACHE_VERSION: u32 = 2; // bump to auto-invalidate stale 512B sector maps

pub struct ClientCache {
    file_path: PathBuf,
    map_path: PathBuf,
    file_read: Arc<std::fs::File>,
    file_write: Arc<std::fs::File>,
    block_map: DashMap<u64, u64>, // cluster_idx -> offset in cache.bin
    next_write_offset: AtomicU64,
    block_size: u64, // sector size, e.g. 512
    max_cache_size: u64,
    is_super: bool,
    total_bytes_written: AtomicU64,
    max_write_bytes_per_sec: u64,
    throttle_bytes_this_window: AtomicU64,
    throttle_window_start: AtomicU64,
    backend: Arc<Backend>,
}

impl ClientCache {
    pub fn new(
        writeback_dirs: &[String],
        client_ip: &str,
        target_name: &str,
        backend: Arc<Backend>,
        max_cache_gb: u64,
        is_super: bool,
        max_write_speed_mbps: u64,
    ) -> io::Result<Self> {
        let block_size = backend.block_size();
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
                            if let (Ok(c_idx), Ok(offset)) = (parts[0].parse::<u64>(), parts[1].parse::<u64>()) {
                                block_map.insert(c_idx, offset);
                                if offset + 4096 > next_write_offset {
                                    next_write_offset = offset + 4096;
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
                        info!("Memuat {} cluster dari .map v{} (next_offset={} of {}MB .bin)", block_map.len(), map_version, next_write_offset, bin_size / 1048576);
                    }
                }
            }
        }

        let mut write_options = OpenOptions::new();
        write_options.write(true).create(true).read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            write_options.share_mode(1 | 2); // FILE_SHARE_READ | FILE_SHARE_WRITE
        }
        let file_write = write_options.open(&file_path)?;
        let file_write_arc = Arc::new(file_write);

        let mut read_options = OpenOptions::new();
        read_options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            read_options.share_mode(1 | 2); // FILE_SHARE_READ | FILE_SHARE_WRITE
        }
        let file_read = read_options.open(&file_path)?;

        Ok(Self {
            file_path,
            map_path,
            file_read: Arc::new(file_read),
            file_write: file_write_arc,
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
            backend,
        })
    }

    pub fn contains_lba(&self, lba: u64) -> bool {
        self.block_map.contains_key(&(lba / 8))
    }

    pub fn read_blocks_cached(
        &self,
        backend: &Backend,
        first_lba: u64,
        num_blocks: u32,
        buf: &mut [u8],
    ) -> io::Result<()> {
        let n = num_blocks as usize;
        let sector_size = self.block_size as usize;
        let cluster_sectors = 8u64; // 4 KB / 512 bytes

        let mut i = 0;
        while i < n {
            let sector_lba = first_lba + i as u64;
            let c_idx = sector_lba / cluster_sectors;

            if let Some(offset_ref) = self.block_map.get(&c_idx) {
                let start_idx = i;
                let mut current_c_idx = c_idx;
                let mut current_offset = *offset_ref;

                i += 1;
                while i < n {
                    let next_lba = first_lba + i as u64;
                    let next_c_idx = next_lba / cluster_sectors;

                    if next_c_idx == current_c_idx {
                        i += 1;
                    } else if let Some(next_offset_ref) = self.block_map.get(&next_c_idx) {
                        let next_offset = *next_offset_ref;
                        if next_offset == current_offset + 4096 {
                            current_c_idx = next_c_idx;
                            current_offset = next_offset;
                            i += 1;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }

                let byte_start = start_idx * sector_size;
                let byte_end = i * sector_size;

                let start_lba_of_run = first_lba + start_idx as u64;
                let start_c_idx = start_lba_of_run / cluster_sectors;
                let sector_offset_in_start_cluster = (start_lba_of_run % cluster_sectors) * self.block_size;

                let cache_read_offset = *self.block_map.get(&start_c_idx).unwrap() + sector_offset_in_start_cluster;

                file_read_exact_at(&self.file_read, cache_read_offset, &mut buf[byte_start..byte_end])?;
            } else {
                let start_idx = i;
                i += 1;
                while i < n {
                    let next_lba = first_lba + i as u64;
                    let next_c_idx = next_lba / cluster_sectors;
                    if self.block_map.contains_key(&next_c_idx) {
                        break;
                    }
                    i += 1;
                }
                let span_blocks = (i - start_idx) as u32;
                let byte_start = start_idx * sector_size;
                let byte_end = i * sector_size;
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
        let sector_size = self.block_size as usize;
        let start_sector = first_lba + buffer_byte_offset / self.block_size;
        let num_sectors = data.len() / sector_size;

        if data.len() % sector_size != 0 || data.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Data harus sector-aligned"));
        }

        let cluster_sectors = 8u64; // 4 KB / 512 bytes
        let cluster_size = 4096usize;

        let first_cluster = start_sector / cluster_sectors;
        let last_cluster = (start_sector + num_sectors as u64 - 1) / cluster_sectors;
        let num_clusters = (last_cluster - first_cluster + 1) as usize;

        let total_needed = (num_clusters as u64) * 4096;
        self.ensure_capacity(total_needed)?;

        let base_offset = self.next_write_offset.fetch_add(total_needed, Ordering::SeqCst);
        self.total_bytes_written.fetch_add(total_needed, Ordering::SeqCst);

        let mut write_buf = vec![0u8; num_clusters * cluster_size];

        for i in 0..num_clusters {
            let c_idx = first_cluster + i as u64;
            let c_start_sector = c_idx * cluster_sectors;

            let overlap_start_sector = start_sector.max(c_start_sector);
            let overlap_end_sector = (start_sector + num_sectors as u64).min(c_start_sector + cluster_sectors);
            let overlap_len_sectors = overlap_end_sector - overlap_start_sector;

            let dest_offset_in_cluster = ((overlap_start_sector - c_start_sector) * self.block_size) as usize;
            let src_offset_in_data = ((overlap_start_sector - start_sector) * self.block_size) as usize;
            let data_len = (overlap_len_sectors * self.block_size) as usize;

            let write_buf_offset = i * cluster_size;

            if overlap_len_sectors == cluster_sectors {
                // Overwrite seluruh cluster - tidak butuh RMW
                write_buf[write_buf_offset..write_buf_offset + cluster_size]
                    .copy_from_slice(&data[src_offset_in_data..src_offset_in_data + cluster_size]);
            } else {
                // Cluster parsial - butuh Read-Modify-Write (RMW)
                let mut cluster_temp = vec![0u8; cluster_size];

                if let Some(existing_off) = self.block_map.get(&c_idx) {
                    file_read_exact_at(&self.file_read, *existing_off, &mut cluster_temp)?;
                } else {
                    self.backend.read_blocks(c_start_sector, cluster_sectors as u32, &mut cluster_temp)?;
                }

                cluster_temp[dest_offset_in_cluster..dest_offset_in_cluster + data_len]
                    .copy_from_slice(&data[src_offset_in_data..src_offset_in_data + data_len]);

                write_buf[write_buf_offset..write_buf_offset + cluster_size].copy_from_slice(&cluster_temp);
            }
        }

        file_write_all_at(&self.file_write, base_offset, &write_buf)?;

        for i in 0..num_clusters {
            let c_idx = first_cluster + i as u64;
            let off = base_offset + (i as u64) * 4096;
            self.block_map.insert(c_idx, off);
        }

        Ok(())
    }

    fn ensure_capacity(&self, needed: u64) -> io::Result<()> {
        let current = self.total_bytes_written.load(Ordering::Relaxed);
        if current + needed <= self.max_cache_size {
            return Ok(());
        }

        let batch_free = (256 * 1024 * 1024).min(self.max_cache_size / 10).max(4096);
        let to_free = ((current + needed) - self.max_cache_size).max(batch_free);
        
        let evict_threshold = (current + needed).saturating_sub(self.max_cache_size).saturating_add(to_free);

        let mut freed_blocks = 0;
        self.block_map.retain(|_c_idx, off| {
            if *off < evict_threshold {
                freed_blocks += 1;
                false // Evict
            } else {
                true // Keep
            }
        });

        let freed_bytes = (freed_blocks as u64) * 4096;
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
        self.file_write.sync_all()?;
        Ok(())
    }

    pub fn cleanup_and_drop(self) {
        let is_super = self.is_super;
        let file_path = self.file_path.clone();
        let map_path = self.map_path.clone();
        
        // Drop the struct to trigger its Drop implementation (which flushes data) 
        // AND releases the OS file lock before we attempt to delete it!
        drop(self);
        
        if is_super {
            info!("Sesi Super Client ditutup, cache dipertahankan di disk.");
            return; // Jangan hapus cache jika super client
        }
        
        if file_path.exists() {
            info!("Menghapus file cache {:?}", file_path);
            if let Err(e) = fs::remove_file(&file_path) {
                warn!("Gagal menghapus file cache {:?}: {}", file_path, e);
            } else {
                info!("File cache {:?} berhasil dihapus.", file_path);
            }
        }
        if map_path.exists() {
            let _ = fs::remove_file(&map_path);
        }
    }
}

impl Drop for ClientCache {
    fn drop(&mut self) {
        self.save_map();
    }
}
