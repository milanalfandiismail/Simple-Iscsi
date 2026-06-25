use dashmap::DashMap;
use std::fs::{self, File};
use std::io::{self, Read, Write, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tracing::{info, warn};

pub struct ClientCache {
    file_path: PathBuf,
    file: Mutex<File>,
    block_map: DashMap<u64, u64>, // LBA -> Cache Offset
    next_write_offset: AtomicU64,
    block_size: u64,
    max_cache_size: u64,
}

/// Mensanitasi IQN agar aman digunakan sebagai nama file.
pub fn sanitize_iqn(iqn: &str) -> String {
    iqn.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

impl ClientCache {
    pub fn new(cache_dir: &str, client_ip: &str, block_size: u64, max_cache_gb: u64) -> io::Result<Self> {
        let dir_path = Path::new(cache_dir);
        if !dir_path.exists() {
            fs::create_dir_all(dir_path)?;
        }

        // Nama file: {client_ip}-gamedisk.bin
        let safe_ip: String = client_ip.chars()
            .map(|c| if c == '.' || c == '-' || c.is_alphanumeric() { c } else { '_' })
            .collect();
        let file_path = dir_path.join(format!("{}-gamedisk.bin", safe_ip));
        
        info!("Menginisialisasi file cache untuk client: {:?}", file_path);

        // Buka dengan mode baca/tulis, buat jika belum ada, kosongkan jika sudah ada
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)  // JANGAN hapus isi lama, append aja
            .open(&file_path)?;

        let max_cache_size = max_cache_gb * 1024 * 1024 * 1024;

        Ok(ClientCache {
            file_path,
            file: Mutex::new(file),
            block_map: DashMap::new(),
            next_write_offset: AtomicU64::new(0),
            block_size,
            max_cache_size,
        })
    }

    /// Membaca block dari cache jika hits.
    pub fn read_block(&self, lba: u64, buf: &mut [u8]) -> Option<io::Result<()>> {
        if let Some(offset) = self.block_map.get(&lba) {
            let offset = *offset;
            let mut file = self.file.lock().unwrap();
            
            if let Err(e) = file.seek(SeekFrom::Start(offset)) {
                return Some(Err(e));
            }
            if let Err(e) = file.read_exact(buf) {
                return Some(Err(e));
            }
            Some(Ok(()))
        } else {
            None // Cache Miss
        }
    }

    /// Batch-read blocks. Return Some(Ok) jika semua di-cache, None jika miss.
    /// Fast path: offset kontigu → 1 seek + 1 read (dramatis kurangi syscalls).
    pub fn read_blocks(&self, first_lba: u64, num_blocks: u32, buf: &mut [u8]) -> Option<io::Result<()>> {
        let block_size = self.block_size as usize;
        let n = num_blocks as usize;
        let mut offsets = Vec::with_capacity(n);
        for i in 0..n {
            let lba = first_lba + i as u64;
            match self.block_map.get(&lba) {
                Some(offset) => offsets.push(*offset),
                None => return None,
            }
        }
        let mut file = self.file.lock().unwrap();
        let mut span_start = 0;
        while span_start < n {
            let base_off = offsets[span_start];
            let mut span_end = span_start + 1;
            while span_end < n && offsets[span_end] == base_off + (span_end - span_start) as u64 * self.block_size {
                span_end += 1;
            }
            let byte_start = span_start * block_size;
            let byte_end = span_end * block_size;
            if let Err(e) = file.seek(SeekFrom::Start(base_off)) {
                return Some(Err(e));
            }
            if let Err(e) = file.read_exact(&mut buf[byte_start..byte_end]) {
                return Some(Err(e));
            }
            span_start = span_end;
        }
        Some(Ok(()))
    }
    #[allow(dead_code)]
    pub fn write_block(&self, lba: u64, data: &[u8]) -> io::Result<()> {
        if data.len() != self.block_size as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Ukuran data tidak sesuai dengan ukuran blok",
            ));
        }

        let offset = if let Some(existing_offset) = self.block_map.get(&lba) {
            *existing_offset
        } else {
            let current_offset = self.next_write_offset.load(Ordering::SeqCst);
            if current_offset + self.block_size > self.max_cache_size {
                return Err(io::Error::new(
                    io::ErrorKind::OutOfMemory,
                    "Batas maksimal cache per-client terlampaui",
                ));
            }
            
            let new_offset = self.next_write_offset.fetch_add(self.block_size, Ordering::SeqCst);
            self.block_map.insert(lba, new_offset);
            new_offset
        };

        let mut file = self.file.lock().unwrap();
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(data)?;
        Ok(())
    }

    /// Batch-write block data kontigu. Hitung offset SEMUA block dulu, lalu lock file SEKALI.
    /// Fast path (semua block baru): alloc contiguous + single seek+write.
    /// Slow path (campur existing): alloc per-block, seek per-block tapi lock sekali.
    /// Menulis data stream langsung ke cache tanpa buffer besar.
    /// FAST PATH: block pertama baru → semua baru → 1 alloc bulk + 1 write.
    /// SLOW PATH: block existing → per-block lookup + group kontigu.
    pub fn write_stream(&self, first_lba: u64, buffer_byte_offset: u64, data: &[u8]) -> io::Result<()> {
        let block_size = self.block_size as usize;
        let start_lba = first_lba + buffer_byte_offset / self.block_size;
        let num_blocks = data.len() / block_size;

        if data.len() % block_size != 0 || data.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Data harus block-aligned dan tidak kosong"));
        }

        // FAST PATH: block pertama baru → semua baru → alloc bulk + write 1 shot
        if self.block_map.get(&start_lba).is_none() {
            let total_len = data.len() as u64;
            let base = self.next_write_offset.fetch_add(total_len, Ordering::SeqCst);
            if base + total_len > self.max_cache_size {
                return Err(io::Error::new(io::ErrorKind::OutOfMemory, "Batas cache per-client terlampaui"));
            }
            for i in 0..num_blocks {
                self.block_map.insert(start_lba + i as u64, base + (i as u64) * self.block_size);
            }
            let mut file = self.file.lock().unwrap();
            file.seek(SeekFrom::Start(base))?;
            file.write_all(data)?;
            return Ok(());
        }

        // SLOW PATH: resolve offset per-block
        let mut offsets = Vec::with_capacity(num_blocks);
        for i in 0..num_blocks {
            let lba = start_lba + i as u64;
            let offset = if let Some(entry) = self.block_map.get(&lba) {
                *entry
            } else {
                let off = self.next_write_offset.fetch_add(self.block_size, Ordering::SeqCst);
                if off + self.block_size > self.max_cache_size {
                    return Err(io::Error::new(io::ErrorKind::OutOfMemory, "Batas cache per-client terlampaui"));
                }
                self.block_map.insert(lba, off);
                off
            };
            offsets.push(offset);
        }

        // Write dengan single lock, group contiguous offsets
        let mut file = self.file.lock().unwrap();
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
        Ok(())
    }

#[allow(dead_code)]
pub fn write_blocks_batch(&self, first_lba: u64, data: &[u8]) -> io::Result<()> {
        let block_size = self.block_size as usize;
        let total_len = data.len();
        if total_len % block_size != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Ukuran data tidak kelipatan block_size",
            ));
        }

        let num_blocks = total_len / block_size;

        // Pre-check kapasitas: hitung berapa block baru yang perlu dialokasi
        let mut new_count = 0u64;
        for i in 0..num_blocks {
            let lba = first_lba + i as u64;
            if !self.block_map.contains_key(&lba) {
                new_count += 1;
            }
        }

        let need_space = new_count * self.block_size;
        if need_space > 0 {
            let current_offset = self.next_write_offset.load(Ordering::SeqCst);
            if current_offset + need_space > self.max_cache_size {
                return Err(io::Error::new(
                    io::ErrorKind::OutOfMemory,
                    "Batas maksimal cache per-client terlampaui",
                ));
            }
        }

        // Fast path: SEMUA block baru → alloc kontigu, tulis sekali
        if new_count == num_blocks as u64 {
            let start_offset = self.next_write_offset.fetch_add(total_len as u64, Ordering::SeqCst);
            for i in 0..num_blocks {
                let lba = first_lba + i as u64;
                let block_off = start_offset + (i as u64) * self.block_size;
                self.block_map.insert(lba, block_off);
            }

            let mut file = self.file.lock().unwrap();
            file.seek(SeekFrom::Start(start_offset))?;
            file.write_all(data)?;
            return Ok(());
        }

        // Slow path: campur block baru & existing — hitung offset per-block
        let offsets: Vec<u64> = (0..num_blocks)
            .map(|i| {
                let lba = first_lba + i as u64;
                if let Some(entry) = self.block_map.get(&lba) {
                    *entry
                } else {
                    let off = self.next_write_offset.fetch_add(self.block_size, Ordering::SeqCst);
                    self.block_map.insert(lba, off);
                    off
                }
            })
            .collect();

        let mut file = self.file.lock().unwrap();
        for (i, &offset) in offsets.iter().enumerate() {
            let start = i * block_size;
            let end = start + block_size;
            file.seek(SeekFrom::Start(offset))?;
            file.write_all(&data[start..end])?;
        }
        Ok(())
    }

    /// Melakukan sinkronisasi data cache ke SSD.
    pub fn flush(&self) -> io::Result<()> {
        let file = self.file.lock().unwrap();
        file.sync_all()
    }

    /// Menghapus file cache secara fisik dari storage.
    pub fn cleanup(&self) {
        if self.file_path.exists() {
            info!("Menghapus file cache {:?}", self.file_path);
            if let Err(e) = fs::remove_file(&self.file_path) {
                warn!("Gagal menghapus file cache {:?}: {}", self.file_path, e);
            } else {
                info!("File cache {:?} berhasil dihapus.", self.file_path);
            }
        }
    }
}

impl Drop for ClientCache {
    fn drop(&mut self) {
        self.cleanup();
    }
}
