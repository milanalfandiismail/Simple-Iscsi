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
    let parent_file = std::fs::File::open(parent_path)?;
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
/// Format: {base_stem}_backup{N}.vhd — increment dari 1.
pub fn backup_before_merge(base_path: &str) -> io::Result<String> {
    let path = Path::new(base_path);
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("backup");

    // Cari nomor backup yang available
    let mut idx = 1;
    loop {
        let backup_name = format!("{}_backup{}.vhd", stem, idx);
        let backup_path = dir.join(&backup_name);
        if !backup_path.exists() {
            std::fs::copy(base_path, &backup_path)?;
            let path_str = backup_path.to_string_lossy().to_string();
            tracing::info!("📦 Backup created: {} ({} bytes)", path_str, std::fs::metadata(&backup_path).map(|m| m.len()).unwrap_or(0));
            return Ok(path_str);
        }
        idx += 1;
    }
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

        // Cek pattern: {stem}_backup{N}.vhd
        let prefix = format!("{}_backup", stem);
        if name.starts_with(&prefix) && name.ends_with(".vhd") {
            let num_part = &name[prefix.len()..name.len() - 4]; // hapus "_backup" dan ".vhd"
            if let Ok(idx) = num_part.parse::<usize>() {
                backups.push((idx, entry.path().to_string_lossy().to_string()));
            }
        }
    }

    // Sort by index
    backups.sort_by_key(|(idx, _)| *idx);

    Ok(backups)
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
    std::fs::copy(&latest_path, base_path)?;
    tracing::info!("✅ Base image restored from: {}", latest_path);
    Ok(latest_path.clone())
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

    std::fs::copy(backup_path, base_path)?;
    tracing::info!("✅ Base image restored from backup [{}]: {}", idx, backup_path);
    Ok(backup_path.clone())
}
