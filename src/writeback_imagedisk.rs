//! Writeback untuk ImageDisk (Windows iSCSI sanboot)
//! Mengelola lifecycle child VHD differencing:
//! - Init: buat child dari parent, buka differencing backend
//! - Cleanup: hapus child saat disconnect (diskless mode)

use crate::backend::Backend;
use crate::config::Config;
use crate::vhd::VhdBackend;
use std::sync::Arc;
use tracing::{info, error};

/// Hasil inisialisasi child VHD
pub struct ChildVhdResult {
    pub backend: Arc<Backend>,
    pub child_path: String,
}

/// Inisialisasi child differencing VHD untuk ImageDisk session.
/// Membuat child baru dari parent path, atau buka ulang jika sudah ada (super client).
pub fn init_child_vhd(
    config: &Config,
    client_ip: &str,
    target_suffix: &str,
) -> Result<ChildVhdResult, std::io::Error> {
    // Resolve parent VHD path via image_manager config
    let vhd_filename = config.image_manager.as_ref()
        .and_then(|m| m.get(target_suffix))
        .cloned()
        .unwrap_or_else(|| format!("{}.vhd", target_suffix));

    let parent_path = if std::path::Path::new(&vhd_filename).is_absolute() {
        vhd_filename.clone()
    } else {
        format!("{}\\{}", config.windows.vhd_dir, vhd_filename)
    };

    // Child VHD path: {writeback_dirs[0]}\{client_ip}-{target_name}.vhd
    let child_dir = config.writeback.writeback_dirs.first()
        .cloned()
        .unwrap_or_else(|| config.windows.vhd_dir.clone());
    let _ = std::fs::create_dir_all(&child_dir);
    let safe_ip = client_ip.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '.' { c } else { '_' })
        .collect::<String>();
    let child_path = format!("{}\\{}-{}.vhd", child_dir, safe_ip, target_suffix);

    let is_super = client_ip == config.windows.super_client_ip;

    // Buat child VHD jika belum ada (atau selalu buat ulang untuk diskless)
    if !std::path::Path::new(&child_path).exists() || !is_super {
        if std::path::Path::new(&child_path).exists() {
            info!("Menghapus child VHD lama: {}", child_path);
            let _ = std::fs::remove_file(&child_path);
        }
        VhdBackend::create_differencing(&parent_path, &child_path)
            .map_err(|e| {
                error!("Gagal membuat child VHD {}: {}", child_path, e);
                std::io::Error::new(std::io::ErrorKind::Other, e)
            })?;
    }

    // Buka differencing backend
    let backend = Backend::new_vhd_diff(
        &child_path,
        &parent_path,
        config.windows.block_size,
        &config.windows.vendor_id,
        &config.windows.product_id,
        &config.windows.product_revision,
    ).map_err(|e| {
        error!("Gagal membuka VHD differencing {}: {}", child_path, e);
        e
    })?;

    info!("Child VHD siap: {} (parent: {})", child_path, parent_path);

    Ok(ChildVhdResult {
        backend: Arc::new(backend),
        child_path,
    })
}

/// Hapus child VHD saat disconnect (diskless — hanya pertahankan untuk super client)
pub fn cleanup_child_vhd(child_path: Option<&str>, client_ip: &str, config: &Config) {
    let is_super = client_ip == config.windows.super_client_ip;
    if let Some(ref path) = child_path {
        if !is_super {
            info!("Menghapus child VHD (diskless): {}", path);
            let _ = std::fs::remove_file(path);
        } else {
            info!("Super client — child VHD dipertahankan: {}", path);
        }
    }
}
