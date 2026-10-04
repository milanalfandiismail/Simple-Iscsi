//! VHD Merge Engine — block-level merge dari differencing VHD ke parent
//! Iterasi BAT child, copy allocated blocks ke parent di EOF, batch update parent BAT.
//! 🔁 ASYNC via spawn_blocking — biar server gak ngeblock.
//!
//! Juga: backup sebelum commit, list/restore backup

use crate::vhd::VhdBackend;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use parking_lot::RwLock;

// Global Atomics & State for VHD Merge Progress Tracking
pub static IS_MERGING: AtomicBool = AtomicBool::new(false);
pub static MERGE_PROGRESS: AtomicU32 = AtomicU32::new(0);
pub static MERGE_CURRENT_BLOCK: AtomicU64 = AtomicU64::new(0);
pub static MERGE_TOTAL_BLOCKS: AtomicU64 = AtomicU64::new(0);
pub static MERGE_MESSAGE: RwLock<String> = RwLock::new(String::new());
pub static MERGE_IMAGE: RwLock<String> = RwLock::new(String::new());
pub static MERGE_ERROR: RwLock<Option<String>> = RwLock::new(None);

#[derive(Debug, Clone, serde::Serialize)]
pub struct MergeStatus {
    pub is_merging: bool,
    pub progress: u32,
    pub current_block: u64,
    pub total_blocks: u64,
    pub image_key: String,
    pub message: String,
    pub error: Option<String>,
}

pub fn get_merge_status() -> MergeStatus {
    MergeStatus {
        is_merging: IS_MERGING.load(Ordering::SeqCst),
        progress: MERGE_PROGRESS.load(Ordering::Relaxed),
        current_block: MERGE_CURRENT_BLOCK.load(Ordering::Relaxed),
        total_blocks: MERGE_TOTAL_BLOCKS.load(Ordering::Relaxed),
        image_key: MERGE_IMAGE.read().clone(),
        message: MERGE_MESSAGE.read().clone(),
        error: MERGE_ERROR.read().clone(),
    }
}

pub fn set_merge_image(key: &str) {
    *MERGE_IMAGE.write() = key.to_string();
}

/// Merge differencing VHD ke parent-nya.
/// Block-level: untuk setiap block yang teralokasi di child → copy ke parent.
///
/// # Async
/// Fungsi ini blocking (I/O file), panggil via `tokio::task::spawn_blocking`.
pub fn merge_vhd_sync(child_path: &str, parent_path: &str) -> io::Result<()> {
    IS_MERGING.store(true, Ordering::SeqCst);
    MERGE_PROGRESS.store(0, Ordering::Relaxed);
    MERGE_CURRENT_BLOCK.store(0, Ordering::Relaxed);
    *MERGE_ERROR.write() = None;
    *MERGE_MESSAGE.write() = "Membuka file VHD differencing dan parent...".to_string();

    let res = (|| -> io::Result<u64> {
        let child_file = std::fs::File::open(child_path)?;
        let mut child = VhdBackend::open(child_file)?;
        let parent_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(parent_path)?;
        let mut parent = VhdBackend::open(parent_file)?;

        // Child dan parent harus punya ukuran block yang sama
        if child.vhd_block_size != parent.vhd_block_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "VHD block size mismatch: child={} parent={}",
                    child.vhd_block_size, parent.vhd_block_size
                ),
            ));
        }

        let vhd_block_size = child.vhd_block_size as u64;
        let bitmap_size = child.sector_bitmap_size as u64;

        // Hitung total block yang teralokasi di differencing VHD
        let allocated_blocks = child.bat.iter().filter(|&&e| e != 0xFFFFFFFF).count() as u64;
        MERGE_TOTAL_BLOCKS.store(allocated_blocks, Ordering::Relaxed);

        if allocated_blocks == 0 {
            MERGE_PROGRESS.store(100, Ordering::Relaxed);
            *MERGE_MESSAGE.write() = "Tidak ada block baru pada differencing VHD".to_string();
            return Ok(0);
        }

        *MERGE_MESSAGE.write() = format!("Memulai penggabungan {} block data...", allocated_blocks);

        // Pre-allocate reusable buffers
        let mut bitmap_buf = vec![0u8; bitmap_size as usize];
        let mut data_buf = vec![0u8; vhd_block_size as usize];

        // Baca footer parent dari posisi (EOF - 512)
        let parent_file_size = parent.file.seek(SeekFrom::End(0))?;
        if parent_file_size < 512 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Parent VHD terlalu kecil, bukan file VHD yang valid",
            ));
        }

        let mut parent_footer = vec![0u8; 512];
        parent.file.seek(SeekFrom::Start(parent_file_size - 512))?;
        parent.file.read_exact(&mut parent_footer)?;

        if &parent_footer[0..8] != b"conectix" {
            // Fallback baca footer dari offset 0 (footer copy)
            parent.file.seek(SeekFrom::Start(0))?;
            parent.file.read_exact(&mut parent_footer)?;
            if &parent_footer[0..8] != b"conectix" {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid VHD footer signature pada parent VHD",
                ));
            }
        }

        // Posisi write untuk blok baru jika belum ada di parent (mulai dari EOF - 512, menimpa footer lama)
        let mut next_write_pos: u64 = parent_file_size - 512;
        let mut total_merged = 0u64;

        for (block_idx, &child_bat_entry) in child.bat.iter().enumerate() {
            if child_bat_entry == 0xFFFFFFFF {
                continue; // Not allocated in child — skip
            }

            // Read sector bitmap + data block dari child
            let child_offset = (child_bat_entry as u64) * 512;
            child.file.seek(SeekFrom::Start(child_offset))?;
            child.file.read_exact(&mut bitmap_buf)?;
            child.file.read_exact(&mut data_buf)?;

            // Pastikan bitmap valid (jika all zero, isi dengan 0xFF untuk menandakan semua sektor pada blok terisi data)
            if bitmap_buf.iter().all(|&b| b == 0) {
                bitmap_buf.fill(0xFF);
            }

            // Cek apakah block sudah ada di parent
            let parent_bat_entry = parent.bat.get(block_idx).copied().unwrap_or(0xFFFFFFFF);
            if parent_bat_entry != 0xFFFFFFFF {
                // In-place overwrite block yang sudah ada di parent (tidak menambah ukuran file yang tidak perlu)
                let parent_offset = (parent_bat_entry as u64) * 512;
                parent.file.seek(SeekFrom::Start(parent_offset))?;
                parent.file.write_all(&bitmap_buf)?;
                parent.file.write_all(&data_buf)?;
            } else {
                // Alokasikan block baru di akhir parent file (posisi next_write_pos)
                let new_bat_entry = (next_write_pos / 512) as u32;
                parent.file.seek(SeekFrom::Start(next_write_pos))?;
                parent.file.write_all(&bitmap_buf)?;
                parent.file.write_all(&data_buf)?;

                next_write_pos += bitmap_size + vhd_block_size;

                if block_idx < parent.bat.len() {
                    parent.bat[block_idx] = new_bat_entry;
                }
            }

            total_merged += 1;
            MERGE_CURRENT_BLOCK.store(total_merged, Ordering::Relaxed);
            let pct = ((total_merged * 100) / allocated_blocks).min(99) as u32;
            MERGE_PROGRESS.store(pct, Ordering::Relaxed);

            if total_merged % 10 == 0 || total_merged == allocated_blocks {
                *MERGE_MESSAGE.write() = format!("Menggabungkan block {}/{} ({}%)...", total_merged, allocated_blocks, pct);
            }
        }

        // Sequential write seluruh BAT parent ke table_offset yang benar
        *MERGE_MESSAGE.write() = "Menulis pembaruan BAT tabel parent VHD...".to_string();
        parent.file.seek(SeekFrom::Start(parent.table_offset))?;
        for &entry in &parent.bat {
            parent.file.write_all(&entry.to_be_bytes())?;
        }

        // Tulis ulang footer di posisi akhir file yang baru
        parent.file.seek(SeekFrom::Start(next_write_pos))?;
        parent.file.write_all(&parent_footer)?;

        // Pastikan ukuran file tepat di posisi footer baru (next_write_pos + 512)
        parent.file.set_len(next_write_pos + 512)?;

        // Update juga footer copy di offset 0
        parent.file.seek(SeekFrom::Start(0))?;
        parent.file.write_all(&parent_footer)?;

        parent.file.sync_all()?;
        Ok(total_merged)

    })();

    match res {
        Ok(total_blocks) => {
            MERGE_PROGRESS.store(100, Ordering::Relaxed);
            *MERGE_MESSAGE.write() = format!("Merge selesai: {} blocks berhasil digabungkan", total_blocks);
            IS_MERGING.store(false, Ordering::SeqCst);
            tracing::info!(
                "Merge selesai: {} blocks di-merge dari {} ke {}",
                total_blocks,
                child_path,
                parent_path
            );
            Ok(())
        }
        Err(err) => {
            *MERGE_ERROR.write() = Some(err.to_string());
            *MERGE_MESSAGE.write() = format!("Merge gagal: {}", err);
            IS_MERGING.store(false, Ordering::SeqCst);
            tracing::error!("Merge VHD gagal: {}", err);
            Err(err)
        }
    }
}

/// Async wrapper untuk merge_vhd_sync
pub async fn merge_vhd(child_path: String, parent_path: String) -> io::Result<()> {
    tokio::task::spawn_blocking(move || merge_vhd_sync(&child_path, &parent_path))
        .await
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("merge_vhd panicked: {}", e)))?
}

/// Backup base image sebelum merge.
/// Alih-alih mengkopi 21GB base image, kita simpan:
/// 1. Copy dari super VHD (sebagai backup dari perubahan).
/// 2. File .meta yang berisi original EOF size, table_offset, dan original BAT dari base image.
pub fn backup_before_merge(base_path: &str, super_path: &str) -> io::Result<String> {
    let path = Path::new(base_path);
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("backup");

    // Cari nomor backup yang available
    let mut idx = 1;
    let (backup_vhd, backup_meta) = loop {
        let vhd_name = format!("{}_backup{}.vhd", stem, idx);
        let meta_name = format!("{}_backup{}.meta", stem, idx);
        let vhd_path = dir.join(&vhd_name);
        let meta_path = dir.join(&meta_name);
        
        if !vhd_path.exists() && !meta_path.exists() {
            break (vhd_path, meta_path);
        }
        idx += 1;
    };

    // 1. Dapatkan metadata base image
    let mut base_file = std::fs::OpenOptions::new().read(true).open(base_path)?;
    let eof = base_file.seek(SeekFrom::End(0))?;
    
    // Baca BAT dan table_offset dari base image
    let base_vhd = VhdBackend::open(base_file)?;
    let table_offset = base_vhd.table_offset;
    let bat = &base_vhd.bat;

    // 2. Simpan ke file .meta
    let mut meta_file = std::fs::File::create(&backup_meta)?;
    // Format meta:
    // [8 bytes: EOF u64 le]
    // [8 bytes: Table Offset u64 le]
    // [4 bytes: BAT len u32 le]
    // [N*4 bytes: BAT entries u32 le]
    meta_file.write_all(&eof.to_le_bytes())?;
    meta_file.write_all(&table_offset.to_le_bytes())?;
    meta_file.write_all(&(bat.len() as u32).to_le_bytes())?;
    for &entry in bat {
        meta_file.write_all(&entry.to_le_bytes())?;
    }
    meta_file.sync_all()?;

    // 3. Copy super VHD sebagai referensi backup (1-2GB)
    if Path::new(super_path).exists() {
        std::fs::copy(super_path, &backup_vhd)?;
    }

    let path_str = backup_meta.to_string_lossy().to_string();
    tracing::info!("📦 Metadata Backup created: {} (Restore point ke EOF {})", path_str, eof);
    if backup_vhd.exists() {
        tracing::info!("📦 Super VHD Backup copied: {} ({} bytes)", backup_vhd.display(), std::fs::metadata(&backup_vhd).map(|m| m.len()).unwrap_or(0));
    }

    Ok(path_str)
}

/// List semua backup yang tersedia untuk base path.
/// Return: Vec<(index, full_path)>
pub fn list_backups(base_path: &str) -> io::Result<Vec<(usize, String)>> {
    let path = Path::new(base_path);
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("backup");

    let mut backups = Vec::new();

    if !dir.exists() {
        return Ok(backups);
    }

    for entry in std::fs::read_dir(dir).map_err(|e| io::Error::new(io::ErrorKind::Other, e))? {
        let entry = entry.map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let name = entry.file_name().to_string_lossy().to_string();

        // Cek pattern: {stem}_backup{N}.meta (bukan .vhd karena restore menggunakan .meta)
        let prefix = format!("{}_backup", stem);
        if name.starts_with(&prefix) && name.ends_with(".meta") {
            let num_part = &name[prefix.len()..name.len() - 5]; // hapus "_backup" dan ".meta"
            if let Ok(idx) = num_part.parse::<usize>() {
                backups.push((idx, entry.path().to_string_lossy().to_string()));
            }
        }
    }

    // Sort by index
    backups.sort_by_key(|(idx, _)| *idx);

    Ok(backups)
}

/// Hapus file snapshot (.meta dan .vhd) untuk index tertentu dan index di atasnya
pub fn cleanup_backup_files(base_path: &str, from_idx: usize) {
    let path = Path::new(base_path);
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("backup");

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            let prefix = format!("{}_backup", stem);
            if file_name.starts_with(&prefix) && (file_name.ends_with(".meta") || file_name.ends_with(".vhd")) {
                let without_ext = if file_name.ends_with(".meta") {
                    &file_name[prefix.len()..file_name.len() - 5]
                } else {
                    &file_name[prefix.len()..file_name.len() - 4]
                };
                if let Ok(i) = without_ext.parse::<usize>() {
                    if i >= from_idx {
                        let _ = std::fs::remove_file(entry.path());
                        tracing::info!("🗑️ Snapshot backup file dibersihkan setelah revert: {}", entry.path().display());
                    }
                }
            }
        }
    }
}

/// Execute restore dari metadata file.
fn restore_from_meta(base_path: &str, meta_path: &str) -> io::Result<String> {
    let mut meta_file = std::fs::File::open(meta_path)?;
    let meta_len = meta_file.metadata()?.len();
    
    let mut eof_bytes = [0u8; 8];
    meta_file.read_exact(&mut eof_bytes)?;
    let eof = u64::from_le_bytes(eof_bytes);

    // Deteksi apakah format baru (ada table_offset u64 di bytes 8..16) atau format lama
    let (table_offset, bat_len) = if meta_len >= 20 {
        let mut second_8 = [0u8; 8];
        meta_file.read_exact(&mut second_8)?;
        let val_8 = u64::from_le_bytes(second_8);
        
        let mut len_bytes = [0u8; 4];
        meta_file.read_exact(&mut len_bytes)?;
        let len = u32::from_le_bytes(len_bytes);

        if meta_len == 20 + (len as u64) * 4 {
            (val_8, len)
        } else {
            let len = (val_8 & 0xFFFFFFFF) as u32;
            meta_file.seek(SeekFrom::Start(12))?;
            (1536u64, len)
        }
    } else {
        let mut len_bytes = [0u8; 4];
        meta_file.read_exact(&mut len_bytes)?;
        let len = u32::from_le_bytes(len_bytes);
        (1536u64, len)
    };

    let mut bat = Vec::with_capacity(bat_len as usize);
    for _ in 0..bat_len {
        let mut entry_bytes = [0u8; 4];
        meta_file.read_exact(&mut entry_bytes)?;
        bat.push(u32::from_le_bytes(entry_bytes));
    }

    tracing::info!("🔄 Memulai restore {} ke ukuran {} bytes (Truncate)...", base_path, eof);

    let mut base_file = std::fs::OpenOptions::new().read(true).write(true).open(base_path)?;
    
    // Truncate ke EOF awal
    base_file.set_len(eof)?;

    // Restore BAT ke table_offset yang benar
    base_file.seek(SeekFrom::Start(table_offset))?;
    for &entry in &bat {
        base_file.write_all(&entry.to_be_bytes())?;
    }

    // Tulis ulang footer di posisi EOF - 512 jika file valid
    let mut footer = [0u8; 512];
    base_file.seek(SeekFrom::Start(0))?;
    base_file.read_exact(&mut footer)?;
    if &footer[0..8] == b"conectix" && eof >= 512 {
        base_file.seek(SeekFrom::Start(eof - 512))?;
        base_file.write_all(&footer)?;
    }
    
    base_file.sync_all()?;
    
    tracing::info!("✅ Base image restored successfully (Metadata based restore).");
    Ok(meta_path.to_string())
}

/// Restore base image dari backup TERAKHIR.
pub fn restore_latest_backup(base_path: &str) -> io::Result<String> {
    let backups = list_backups(base_path)?;
    if backups.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Tidak ada backup untuk {}", base_path),
        ));
    }

    let (latest_idx, latest_path) = backups.last().unwrap().clone();
    let res = restore_from_meta(base_path, &latest_path);
    if res.is_ok() {
        cleanup_backup_files(base_path, latest_idx);
    }
    res
}

/// Restore base image dari backup spesifik (by index).
pub fn restore_backup_by_index(base_path: &str, idx: usize) -> io::Result<String> {
    let backups = list_backups(base_path)?;
    if backups.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Tidak ada backup untuk {}", base_path),
        ));
    }

    let backup = backups.iter().find(|(i, _)| *i == idx);
    let (_, backup_path) = match backup {
        Some(b) => b,
        None => {
            let available: Vec<String> = backups.iter().map(|(i, _)| i.to_string()).collect();
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Backup index {} tidak ditemukan. Tersedia: {}", idx, available.join(", ")),
            ));
        }
    };

    let path_clone = backup_path.clone();
    let res = restore_from_meta(base_path, &path_clone);
    if res.is_ok() {
        cleanup_backup_files(base_path, idx);
    }
    res
}
