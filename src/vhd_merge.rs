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

            // Alokasikan block selalu di akhir parent file (posisi next_write_pos) — Append-Only Copy-on-Write (CoW).
            // Seluruh blok asli parent di 0..original_eof tetap 100% read-only & utuh.
            // Hal ini menjamin revert/rollback snapshot dapat memotong file (truncate) dan memulihkan BAT secara deterministik,
            // sehingga file/data baru super client musnah 100% tanpa meninggalkan residu di blok lama.
            let new_bat_entry = (next_write_pos / 512) as u32;
            parent.file.seek(SeekFrom::Start(next_write_pos))?;
            parent.file.write_all(&bitmap_buf)?;
            parent.file.write_all(&data_buf)?;

            next_write_pos += bitmap_size + vhd_block_size;

            if block_idx < parent.bat.len() {
                parent.bat[block_idx] = new_bat_entry;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom, Write};

    fn create_test_vhd(
        path: &str,
        disk_type: u32,
        block_size: u32,
        bat_entries: &[u32],
        blocks_data: &[(usize, u8)], // (bat_index, byte_fill)
    ) -> io::Result<u64> {
        let mut file = File::create(path)?;
        
        let sectors_per_block = block_size / 512;
        let mut bitmap_bytes = (sectors_per_block + 7) / 8;
        if bitmap_bytes % 512 != 0 {
            bitmap_bytes = ((bitmap_bytes / 512) + 1) * 512;
        }

        // 1. Footer (512 bytes)
        let mut footer = vec![0u8; 512];
        footer[0..8].copy_from_slice(b"conectix");
        let virtual_size = (bat_entries.len() as u64) * (block_size as u64);
        footer[48..56].copy_from_slice(&virtual_size.to_be_bytes());
        footer[60..64].copy_from_slice(&disk_type.to_be_bytes());
        file.write_all(&footer)?;

        // 2. Dynamic Header (1024 bytes)
        let mut header = vec![0u8; 1024];
        header[0..8].copy_from_slice(b"cxsparse");
        header[16..24].copy_from_slice(&1536u64.to_be_bytes()); // table_offset = 1536
        header[28..32].copy_from_slice(&(bat_entries.len() as u32).to_be_bytes());
        header[32..36].copy_from_slice(&block_size.to_be_bytes());
        file.write_all(&header)?;

        // 3. BAT Table (table_offset = 1536)
        let mut bat_bytes = vec![0u8; 512]; // at least 512 bytes
        for (i, &entry) in bat_entries.iter().enumerate() {
            let off = i * 4;
            bat_bytes[off..off + 4].copy_from_slice(&entry.to_be_bytes());
        }
        file.write_all(&bat_bytes)?;

        // 4. Data Blocks
        for &(bat_idx, byte_fill) in blocks_data {
            let sector_offset = bat_entries[bat_idx];
            let byte_offset = (sector_offset as u64) * 512;
            file.seek(SeekFrom::Start(byte_offset))?;
            
            // Bitmap
            let bitmap = vec![0xFFu8; bitmap_bytes as usize];
            file.write_all(&bitmap)?;
            // Data
            let data = vec![byte_fill; block_size as usize];
            file.write_all(&data)?;
        }

        // 5. Trailing Footer
        let final_pos = file.seek(SeekFrom::End(0))?;
        file.write_all(&footer)?;
        file.sync_all()?;
        Ok(final_pos + 512)
    }

    #[test]
    fn test_vhd_append_cow_merge_and_snapshot_rollback() {
        let temp_dir = std::env::temp_dir().join(format!("vhd_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let parent_path = temp_dir.join("parent.vhd").to_string_lossy().to_string();
        let child_path = temp_dir.join("child.vhd").to_string_lossy().to_string();

        let block_size = 65536u32; // 64KB per block for fast testing
        let sectors_per_block = block_size / 512; // 128 sectors
        let bitmap_sectors = 1u32; // 512 bytes = 1 sector
        let block_total_sectors = bitmap_sectors + sectors_per_block;

        // Parent BAT: 4 blocks total.
        // Block 0 allocated at sector 4 (offset 2048)
        // Block 1 allocated at sector 4 + block_total_sectors
        // Block 2 & 3 unallocated
        let s0 = 4u32;
        let s1 = s0 + block_total_sectors;
        let parent_bat = vec![s0, s1, 0xFFFFFFFF, 0xFFFFFFFF];
        
        let parent_initial_size = create_test_vhd(
            &parent_path,
            3, // Dynamic
            block_size,
            &parent_bat,
            &[(0, 0x11), (1, 0x22)], // Block 0 has 0x11, Block 1 has 0x22
        ).unwrap();

        // Verify parent initial state
        {
            let p_file = File::open(&parent_path).unwrap();
            let mut p_vhd = VhdBackend::open(p_file).unwrap();
            assert_eq!(p_vhd.bat[0], s0);
            assert_eq!(p_vhd.bat[1], s1);
            assert_eq!(p_vhd.bat[2], 0xFFFFFFFF);
            
            // Read Block 0
            let mut buf = vec![0u8; block_size as usize];
            p_vhd.file.seek(SeekFrom::Start((s0 as u64 + 1) * 512)).unwrap();
            p_vhd.file.read_exact(&mut buf).unwrap();
            assert!(buf.iter().all(|&b| b == 0x11));
        }

        // Child (Differencing):
        // Modifies Block 0 with 0xAA (Simulating File A added by Super Client)
        // Adds Block 2 with 0xBB
        let cs0 = 4u32;
        let cs2 = cs0 + block_total_sectors;
        let child_bat = vec![cs0, 0xFFFFFFFF, cs2, 0xFFFFFFFF];
        create_test_vhd(
            &child_path,
            4, // Differencing
            block_size,
            &child_bat,
            &[(0, 0xAA), (2, 0xBB)],
        ).unwrap();

        // 1. Take Backup before merge
        let backup_meta_path = backup_before_merge(&parent_path, &child_path).unwrap();
        assert!(Path::new(&backup_meta_path).exists());

        // 2. Perform Merge (Append-Only CoW)
        merge_vhd_sync(&child_path, &parent_path).unwrap();

        // Verify parent state after merge:
        // - Parent file size MUST be larger than parent_initial_size (due to append)
        // - Parent BAT[0] MUST point to a new offset >= (parent_initial_size - 512)
        // - Parent Block 0 MUST now contain 0xAA
        // - Parent Block 2 MUST now contain 0xBB
        // - Original Block 0 at sector s0 (offset 2048) in the parent MUST STILL CONTAIN 0x11 (Read-Only untouched)!
        {
            let p_file = File::open(&parent_path).unwrap();
            let mut p_vhd = VhdBackend::open(p_file).unwrap();
            assert_ne!(p_vhd.bat[0], s0, "BAT[0] must point to newly appended offset, not old in-place offset!");
            assert_ne!(p_vhd.bat[2], 0xFFFFFFFF);

            // Read new Block 0 via BAT[0]
            let new_s0 = p_vhd.bat[0];
            let mut buf = vec![0u8; block_size as usize];
            p_vhd.file.seek(SeekFrom::Start((new_s0 as u64 + 1) * 512)).unwrap();
            p_vhd.file.read_exact(&mut buf).unwrap();
            assert!(buf.iter().all(|&b| b == 0xAA), "New Block 0 data must be 0xAA");

            // Read old sector s0 at offset 2048 to PROVE it was untouched (CoW preservation!)
            let mut old_buf = vec![0u8; block_size as usize];
            p_vhd.file.seek(SeekFrom::Start((s0 as u64 + 1) * 512)).unwrap();
            p_vhd.file.read_exact(&mut old_buf).unwrap();
            assert!(old_buf.iter().all(|&b| b == 0x11), "Original parent block 0 sector at offset 2048 MUST BE UNTOUCHED 0x11!");
        }

        // 3. Perform Restore / Revert Snapshot
        let restore_res = restore_backup_by_index(&parent_path, 1).unwrap();
        assert_eq!(restore_res, backup_meta_path);

        // 4. Verify parent state after restore:
        // - Parent file size MUST be exactly restored to parent_initial_size
        // - Parent BAT[0] MUST point back to s0
        // - Parent Block 0 MUST contain 0x11 (0xAA / File A is 100% GONE!)
        // - Parent Block 2 MUST be back to 0xFFFFFFFF
        // - VHD signature and header MUST be valid
        {
            let meta = std::fs::metadata(&parent_path).unwrap();
            assert_eq!(meta.len(), parent_initial_size, "Restored file length must match parent_initial_size exactly!");

            let p_file = File::open(&parent_path).unwrap();
            let mut p_vhd = VhdBackend::open(p_file).unwrap();
            assert_eq!(p_vhd.bat[0], s0);
            assert_eq!(p_vhd.bat[1], s1);
            assert_eq!(p_vhd.bat[2], 0xFFFFFFFF);
            assert_eq!(p_vhd.bat[3], 0xFFFFFFFF);

            let mut buf = vec![0u8; block_size as usize];
            p_vhd.file.seek(SeekFrom::Start((s0 as u64 + 1) * 512)).unwrap();
            p_vhd.file.read_exact(&mut buf).unwrap();
            assert!(buf.iter().all(|&b| b == 0x11), "Block 0 MUST be 0x11 after restore, File A is completely wiped!");
        }

        // Cleanup temp dir
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_multilevel_snapshot_rollback() {
        let temp_dir = std::env::temp_dir().join(format!("vhd_multilevel_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let parent_path = temp_dir.join("master.vhd").to_string_lossy().to_string();
        let child_path = temp_dir.join("child.vhd").to_string_lossy().to_string();

        let block_size = 65536u32;
        let s0 = 4u32;
        let parent_bat = vec![s0, 0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF];

        // Base VHD: Block 0 = 0x10
        let s0_size = create_test_vhd(
            &parent_path,
            3,
            block_size,
            &parent_bat,
            &[(0, 0x10)],
        ).unwrap();

        // --- Commit 1 (Chrome update): modifies Block 0 to 0x20 ---
        create_test_vhd(&child_path, 4, block_size, &[s0, 0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF], &[(0, 0x20)]).unwrap();
        backup_before_merge(&parent_path, &child_path).unwrap(); // backup1
        merge_vhd_sync(&child_path, &parent_path).unwrap();
        let s1_size = std::fs::metadata(&parent_path).unwrap().len();

        // --- Commit 2 (Steam update): modifies Block 0 to 0x30 ---
        create_test_vhd(&child_path, 4, block_size, &[s0, 0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF], &[(0, 0x30)]).unwrap();
        backup_before_merge(&parent_path, &child_path).unwrap(); // backup2
        merge_vhd_sync(&child_path, &parent_path).unwrap();
        let s2_size = std::fs::metadata(&parent_path).unwrap().len();

        // --- Commit 3 (File A error): modifies Block 0 to 0x40 ---
        create_test_vhd(&child_path, 4, block_size, &[s0, 0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF], &[(0, 0x40)]).unwrap();
        backup_before_merge(&parent_path, &child_path).unwrap(); // backup3
        merge_vhd_sync(&child_path, &parent_path).unwrap();
        let s3_size = std::fs::metadata(&parent_path).unwrap().len();
        assert!(s3_size > s2_size, "Commit 3 must increase file size due to append-only CoW");

        // Check we have 3 backups available
        let backups = list_backups(&parent_path).unwrap();
        assert_eq!(backups.len(), 3);
        assert_eq!(backups[0].0, 1);
        assert_eq!(backups[1].0, 2);
        assert_eq!(backups[2].0, 3);

        // 1. Revert to Backup 3 (Rollback Commit 3 / File A -> Leaves disk at Commit 2 / Steam)
        restore_backup_by_index(&parent_path, 3).unwrap();
        assert_eq!(std::fs::metadata(&parent_path).unwrap().len(), s2_size);
        {
            let p_file = File::open(&parent_path).unwrap();
            let mut p_vhd = VhdBackend::open(p_file).unwrap();
            let bat0 = p_vhd.bat[0];
            let mut buf = vec![0u8; block_size as usize];
            p_vhd.file.seek(SeekFrom::Start((bat0 as u64 + 1) * 512)).unwrap();
            p_vhd.file.read_exact(&mut buf).unwrap();
            assert!(buf.iter().all(|&b| b == 0x30), "Disk is back at Commit 2 (0x30)");
        }
        let remaining_backups = list_backups(&parent_path).unwrap();
        assert_eq!(remaining_backups.len(), 2, "Backup 3 should be cleaned, 1 and 2 remain");

        // 2. Revert to Backup 2 (Rollback Commit 2 / Steam -> Leaves disk at Commit 1 / Chrome)
        restore_backup_by_index(&parent_path, 2).unwrap();
        assert_eq!(std::fs::metadata(&parent_path).unwrap().len(), s1_size);
        {
            let p_file = File::open(&parent_path).unwrap();
            let mut p_vhd = VhdBackend::open(p_file).unwrap();
            let bat0 = p_vhd.bat[0];
            let mut buf = vec![0u8; block_size as usize];
            p_vhd.file.seek(SeekFrom::Start((bat0 as u64 + 1) * 512)).unwrap();
            p_vhd.file.read_exact(&mut buf).unwrap();
            assert!(buf.iter().all(|&b| b == 0x20), "Disk is back at Commit 1 (0x20)");
        }

        // 3. Revert to Backup 1 (Rollback Commit 1 / Chrome -> Leaves disk at Base 0x10)
        restore_backup_by_index(&parent_path, 1).unwrap();
        assert_eq!(std::fs::metadata(&parent_path).unwrap().len(), s0_size);
        {
            let p_file = File::open(&parent_path).unwrap();
            let mut p_vhd = VhdBackend::open(p_file).unwrap();
            let bat0 = p_vhd.bat[0];
            let mut buf = vec![0u8; block_size as usize];
            p_vhd.file.seek(SeekFrom::Start((bat0 as u64 + 1) * 512)).unwrap();
            p_vhd.file.read_exact(&mut buf).unwrap();
            assert!(buf.iter().all(|&b| b == 0x10), "Disk is back at original Base (0x10)");
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

