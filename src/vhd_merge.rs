//! VHD Merge Engine — block-level merge dari differencing VHD ke parent
//! Iterasi BAT child, copy allocated blocks ke parent di EOF, batch update parent BAT.
//! 🔁 ASYNC via spawn_blocking — biar server gak ngeblock.
//!
//! Juga: backup sebelum commit, list/restore backup

use crate::vhd::VhdBackend;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Merge differencing VHD ke parent-nya.
/// Block-level: untuk setiap block yang teralokasi di child → copy ke parent.
///
/// # Async
/// Fungsi ini blocking (I/O file), panggil via `tokio::task::spawn_blocking`.
pub fn merge_vhd_sync(child_path: &str, parent_path: &str) -> io::Result<()> {
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

    // Pre-allocate reusable buffers
    let mut bitmap_buf = vec![0u8; bitmap_size as usize];
    let mut data_buf = vec![0u8; vhd_block_size as usize];

    let mut total_blocks = 0u64;

    for (block_idx, &bat_entry) in child.bat.iter().enumerate() {
        if bat_entry == 0xFFFFFFFF {
            continue; // Not allocated in child — skip
        }

        // Read sector bitmap + data block dari child
        let child_offset = (bat_entry as u64) * 512;
        child.file.seek(SeekFrom::Start(child_offset))?;
        child.file.read_exact(&mut bitmap_buf)?;
        child.file.read_exact(&mut data_buf)?;

        // Allocate block di parent (append at EOF)
        let parent_eof = parent.file.seek(SeekFrom::End(0))?;
        let parent_bat_entry = (parent_eof / 512) as u32;

        parent.file.write_all(&bitmap_buf)?;
        parent.file.write_all(&data_buf)?;

        parent.bat[block_idx] = parent_bat_entry;
        total_blocks += 1;
    }

    // Batch update parent BAT — sequential write semua entries
    parent.file.seek(SeekFrom::Start(1536))?;
    for &entry in &parent.bat {
        parent.file.write_all(&entry.to_be_bytes())?;
    }

    parent.file.sync_all()?;

    tracing::info!(
        "Merge selesai: {} blocks di-merge dari {} ke {}",
        total_blocks,
        child_path,
        parent_path
    );

    Ok(())
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
/// 2. File .meta yang berisi original EOF size dan original BAT dari base image.
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
    
    // Baca BAT dari base image (Dynamic VHD header di 1536)
    let base_vhd = VhdBackend::open(base_file)?;
    let bat = &base_vhd.bat;

    // 2. Simpan ke file .meta
    let mut meta_file = std::fs::File::create(&backup_meta)?;
    // Format meta sederhana:
    // [8 bytes: EOF u64 le]
    // [4 bytes: BAT len u32 le]
    // [N bytes: BAT entries u32 le]
    meta_file.write_all(&eof.to_le_bytes())?;
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

/// Execute restore dari metadata file.
fn restore_from_meta(base_path: &str, meta_path: &str) -> io::Result<String> {
    let mut meta_file = std::fs::File::open(meta_path)?;
    
    let mut eof_bytes = [0u8; 8];
    meta_file.read_exact(&mut eof_bytes)?;
    let eof = u64::from_le_bytes(eof_bytes);

    let mut len_bytes = [0u8; 4];
    meta_file.read_exact(&mut len_bytes)?;
    let bat_len = u32::from_le_bytes(len_bytes);

    let mut bat = Vec::with_capacity(bat_len as usize);
    for _ in 0..bat_len {
        let mut entry_bytes = [0u8; 4];
        meta_file.read_exact(&mut entry_bytes)?;
        bat.push(u32::from_le_bytes(entry_bytes));
    }

    tracing::info!("🔄 Memulai restore {} ke ukuran {} bytes (Truncate)...", base_path, eof);

    let mut base_file = std::fs::OpenOptions::new().write(true).open(base_path)?;
    
    // Truncate ke EOF awal
    base_file.set_len(eof)?;

    // Restore BAT
    base_file.seek(SeekFrom::Start(1536))?;
    for &entry in &bat {
        base_file.write_all(&entry.to_be_bytes())?;
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

    let (_, latest_path) = backups.last().unwrap();
    restore_from_meta(base_path, latest_path)
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

    restore_from_meta(base_path, backup_path)
}
