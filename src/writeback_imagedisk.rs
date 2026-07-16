//! Writeback untuk ImageDisk (Windows iSCSI sanboot)
//! Mengelola lifecycle VHD differencing:
//! - Super client: serve super VHD langsung (persistent)
//! - Normal client: create temporary child VHD dari base

use crate::backend::Backend;
use crate::config::Config;
use crate::vhd::VhdBackend;
use crate::writeback_super;
use std::sync::Arc;
use tracing::{info, error};

/// Hasil inisialisasi VHD untuk session
pub struct ChildVhdResult {
    pub backend: Arc<Backend>,
    /// Path ke VHD yang perlu cleanup (None = super VHD, jangan dihapus)
    pub child_path: Option<String>,
}

/// Inisialisasi VHD untuk ImageDisk session.
/// - Super client → serve super VHD langsung (persistent, no cleanup)
/// - Normal client → create temporary child VHD dari base (diskless)
pub fn init_child_vhd(
    config: &Config,
    client_ip: &str,
    target_suffix: &str,
    is_super: bool,
) -> Result<ChildVhdResult, std::io::Error> {
    // Resolve parent/base VHD path via image_manager config
    let base_path = writeback_super::resolve_base_path(config, target_suffix);

    if is_super {
        // === SUPER CLIENT: serve super VHD langsung ===
        let super_path = writeback_super::get_super_path(config, target_suffix);

        // Init super VHD (create differencing kalo belum ada)
        writeback_super::init_super_vhd(&base_path, &super_path)?;

        // Buka super VHD sebagai differencing backend
        let backend = Backend::new_vhd_diff(
            &super_path,
            &base_path,
            config.windows.as_ref().unwrap().block_size,
            &config.windows.as_ref().unwrap().vendor_id,
            &config.windows.as_ref().unwrap().product_id,
            &config.windows.as_ref().unwrap().product_revision,
            config.server.read_cache_gb,
        ).map_err(|e| {
            error!("Gagal membuka super VHD {}: {}", super_path, e);
            e
        })?;

        info!("Super VHD siap: {} (base: {})", super_path, base_path);

        Ok(ChildVhdResult {
            backend: Arc::new(backend),
            child_path: None, // No cleanup — super VHD persistent
        })
    } else {
        // === NORMAL CLIENT: create temporary child VHD ===
        let child_dir = config.writeback.writeback_dirs.first()
            .cloned()
            .unwrap_or_else(|| config.windows.as_ref().unwrap().vhd_dir.clone());
        let _ = std::fs::create_dir_all(&child_dir);
        let safe_ip = client_ip.chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '.' { c } else { '_' })
            .collect::<String>();
        let child_path = format!("{}\\{}-{}.vhd", child_dir, safe_ip, target_suffix);

        // Buat child VHD — hapus dulu kalo sudah ada (diskless)
        if std::path::Path::new(&child_path).exists() {
            info!("Menghapus child VHD lama: {}", child_path);
            let _ = std::fs::remove_file(&child_path);
        }
        VhdBackend::create_differencing(&base_path, &child_path)
            .map_err(|e| {
                error!("Gagal membuat child VHD {}: {}", child_path, e);
                std::io::Error::new(std::io::ErrorKind::Other, e)
            })?;

        // Buka differencing backend
        let backend = Backend::new_vhd_diff(
            &child_path,
            &base_path,
            config.windows.as_ref().unwrap().block_size,
            &config.windows.as_ref().unwrap().vendor_id,
            &config.windows.as_ref().unwrap().product_id,
            &config.windows.as_ref().unwrap().product_revision,
            config.server.read_cache_gb,
        ).map_err(|e| {
            error!("Gagal membuka VHD differencing {}: {}", child_path, e);
            e
        })?;

        info!("Child VHD siap: {} (parent: {})", child_path, base_path);

        Ok(ChildVhdResult {
            backend: Arc::new(backend),
            child_path: Some(child_path),
        })
    }
}

/// Hapus child VHD saat disconnect (diskless)
/// Super VHD tidak dihapus — dibiarkan utuh (persistent)
pub fn cleanup_child_vhd(child_path: Option<&str>, client_ip: &str, config: &Config) {
    let is_super = client_ip == config.windows.as_ref().unwrap().super_client_ip;
    match child_path {
        Some(path) if !is_super => {
            info!("Menghapus child VHD (diskless): {}", path);
            let _ = std::fs::remove_file(path);
        }
        Some(_) => {
            info!("Super client — child VHD dipertahankan");
        }
        None => {
            // Super VHD — tidak perlu cleanup (persistent)
        }
    }
}
