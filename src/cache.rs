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
const FLUSH_THRESHOLD: u64 = 64;              // flush after 64 unwritten blocks

pub struct ClientCache {
    file_path: PathBuf,
    map_path: PathBuf,
    file: Arc<Mutex<BufWriter<std::fs::File>>>,
    block_map: DashMap<u64, u64>,
    next_write_offset: AtomicU64,
    block_size: u64,
    max_cache_size: u64,
    is_super: bool,
    unflushed_writes: AtomicU64,
    total_bytes_written: AtomicU64,
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

        // Load existing map — ALL clients need .map to see .bin data.
        // Without .map, .bin block offsets are unknown → reads go to raw disk → corrupt.
        if file_path.exists() && map_path.exists() {
            // Verify .bin size — must be >= next_offset from .map
            let bin_size = std::fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0);
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
                // Defensive: if .map points beyond .bin size, cache is corrupt → clear
                if next_write_offset > bin_size {
                    warn!(".map inconsistent dgn .bin (map_next={} > bin_size={}) — menghapus cache stale", next_write_offset, bin_size);
                    block_map.clear();
                    next_write_offset = 0;
                    let _ = std::fs::remove_file(&map_path);
                    let _ = std::fs::remove_file(&file_path);
                } else {
                    info!("Memuat {} block dari .map (next_offset={} of {}MB .bin)", block_map.len(), next_write_offset, bin_size / 1048576);
                }
            }
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            // NEVER truncate — .bin adalah write layer untuk gamedisk.
            // Cleanup di-handle eksplisit saat LOGOUT, bukan saat reconnect.
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
            unflushed_writes: AtomicU64::new(0),
            total_bytes_written: AtomicU64::new(next_write_offset),
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

    /// Cek apakah seluruh range LBA (first_lba..first_lba+n) ada di cache.
    /// Game installers write files in multiple WRITE commands — blocks may
    /// be at non-contiguous offsets in .bin. Check every LBA individually.
    pub fn contains_range(&self, first_lba: u64, n: u32) -> bool {
        if n == 0 { return false; }
        for i in 0..n {
            if !self.block_map.contains_key(&(first_lba + i as u64)) {
                return false;
            }
        }
        true
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
            self.ensure_capacity(total_len)?;
            let base = self.next_write_offset.fetch_add(total_len, Ordering::SeqCst);
            self.total_bytes_written.fetch_add(total_len, Ordering::SeqCst);

            for i in 0..num_blocks {
                self.block_map.insert(start_lba + i as u64, base + (i as u64) * self.block_size);
            }

            let mut writer = self.file.lock();
            writer.get_mut().seek(SeekFrom::Start(base))?;
            writer.write_all(data)?;
            // CRITICAL: sync_all() BEFORE saving .map — ensures .bin data
            // is on disk before .map entries point to it.
            writer.get_mut().sync_all()?;
            drop(writer);

            // Save .map immediately after sync — .map & .bin always consistent
            self.save_map();

            self.maybe_flush(num_blocks as u64);
            return Ok(());
        }

        let mut offsets = Vec::with_capacity(num_blocks);
        for i in 0..num_blocks {
            let lba = start_lba + i as u64;
            let offset = if let Some(entry) = self.block_map.get(&lba) {
                *entry
            } else {
                self.ensure_capacity(self.block_size)?;
                let off = self.next_write_offset.fetch_add(self.block_size, Ordering::SeqCst);
                self.total_bytes_written.fetch_add(self.block_size, Ordering::SeqCst);
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
        drop(writer);
        self.maybe_flush(num_blocks as u64);
        Ok(())
    }

    /// Pastikan kapasitas cache tidak melebihi max_cache_size.
    /// Jika melebihi, evict oldest blocks sampai cukup.
    fn ensure_capacity(&self, needed: u64) -> io::Result<()> {
        let current = self.total_bytes_written.load(Ordering::Relaxed);
        if current + needed <= self.max_cache_size {
            return Ok(());
        }
        // Evict oldest blocks: hapus entries dengan offset terkecil
        let to_free = (current + needed).saturating_sub(self.max_cache_size);
        let mut freed: u64 = 0;
        let mut entries: Vec<(u64, u64)> = self.block_map.iter().map(|e| (*e.key(), *e.value())).collect();
        entries.sort_by_key(|(_, off)| *off); // oldest first (lowest offset)

        for (lba, _off) in entries {
            if freed >= to_free { break; }
            self.block_map.remove(&lba);
            freed += self.block_size;
        }
        self.total_bytes_written.fetch_sub(freed.min(current), Ordering::SeqCst);
        Ok(())
    }

    /// Flush ke disk hanya jika counter melebihi threshold
    fn maybe_flush(&self, blocks: u64) {
        let count = self.unflushed_writes.fetch_add(blocks, Ordering::Relaxed) + blocks;
        if count >= FLUSH_THRESHOLD {
            let mut writer = self.file.lock();
            let _ = writer.flush();
            let _ = writer.get_mut().sync_all(); // ensure data hits physical disk
            self.unflushed_writes.store(0, Ordering::Relaxed);
        }
    }

    fn save_map(&self) {
        let mut map_content = String::new();
        for entry in self.block_map.iter() {
            map_content.push_str(&format!("{}:{}\n", entry.key(), entry.value()));
        }
        if let Err(e) = std::fs::write(&self.map_path, map_content) {
            warn!("Gagal menyimpan block map {:?}: {}", self.map_path, e);
        }
    }

    pub fn flush(&self) -> io::Result<()> {
        let mut writer = self.file.lock();
        writer.flush()?;
        writer.get_mut().sync_all()?;
        drop(writer);
        
        self.save_map();
        
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
        // Flush on drop but DON'T auto-delete — cleanup handled explicitly
        // by session state (logout vs TCP disconnect)
        if !self.is_super {
            let mut writer = self.file.lock();
            let _ = writer.flush();
            let _ = writer.get_mut().sync_all();
        }
    }
}
