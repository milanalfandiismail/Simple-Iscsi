//! Super VHD Lifecycle — mengelola super VHD untuk super client
//! Super VHD adalah differencing dari base VHD, di-serve langsung ke super client.
//! Perubahan ditulis langsung ke super VHD, bukan ke child VHD.

use crate::config::Config;
use crate::vhd::VhdBackend;
use tracing::{info, warn};

/// Init super VHD — create differencing dari base kalo belum ada.
/// Kalo sudah ada, no-op (return Ok).
pub fn init_super_vhd(base_path: &str, super_path: &str) -> Result<bool, std::io::Error> {
    if std::path::Path::new(super_path).exists() {
        info!("Super VHD sudah ada: {}", super_path);
        return Ok(false); // false = sudah ada, no new create
    }

    // Pastikan super dir ada
    if let Some(parent) = std::path::Path::new(super_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    VhdBackend::create_differencing(base_path, super_path).map_err(|e| {
        warn!("Gagal membuat super VHD {}: {}", super_path, e);
        std::io::Error::new(std::io::ErrorKind::Other, e)
    })?;

    info!("Super VHD dibuat: {} (differencing dari {})", super_path, base_path);
    Ok(true) // true = baru dibuat
}

/// Cek apakah super VHD exists
pub fn super_exists(super_path: &str) -> bool {
    std::path::Path::new(super_path).exists()
}

/// Hapus super VHD (discard)
pub fn delete_super(super_path: &str) -> Result<(), std::io::Error> {
    if !std::path::Path::new(super_path).exists() {
        warn!("Super VHD tidak ditemukan untuk dihapus: {}", super_path);
        return Ok(());
    }
    std::fs::remove_file(super_path)?;
    info!("Super VHD dihapus: {}", super_path);
    Ok(())
}

/// Dapatkan path super VHD — ikut folder base image + _super.vhd
/// Contoh:
///   Base: E:\Windows 24H2\Windows_24H2_Modern.vhd
///   Super: E:\Windows 24H2\Windows_24H2_Modern_super.vhd
pub fn get_super_path(config: &Config, image_key: &str) -> String {
    let base_path = resolve_base_path(config, image_key);
    let path = std::path::Path::new(&base_path);
    let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let stem = path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(image_key);
    format!("{}\\{}_super.vhd", dir.display(), stem)
}

/// Resolve base VHD path dari image_manager config
pub fn resolve_base_path(config: &Config, image_key: &str) -> String {
    config.image_manager.as_ref()
        .and_then(|m| m.get(image_key))
        .cloned()
        .unwrap_or_else(|| {
            let vhd_filename = format!("{}.vhd", image_key);
            if std::path::Path::new(&vhd_filename).is_absolute() {
                vhd_filename
            } else {
                format!("{}\\{}", config.windows.vhd_dir, vhd_filename)
            }
        })
}
