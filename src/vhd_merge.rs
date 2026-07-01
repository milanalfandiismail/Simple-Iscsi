//! VHD Merge Engine — block-level merge dari differencing VHD ke parent
//! Iterasi BAT child, copy allocated blocks ke parent di EOF, batch update parent BAT.
//! 🔁 ASYNC via spawn_blocking — biar server gak ngeblock.

use crate::vhd::VhdBackend;
use std::io::{self, Read, Seek, SeekFrom, Write};

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
