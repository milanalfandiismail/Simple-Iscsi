//! Writeback untuk ImageDisk (Windows iSCSI sanboot)
//! Mengelola lifecycle VHD differencing:
//! - Super client: serve super VHD langsung (persistent)
//! - Normal client: create temporary child VHD dari base

use crate::backend::Backend;
use crate::config::Config;
use crate::read_ahead::ReadAheadCache;
use crate::writeback_super;
use dashmap::DashMap;
use std::sync::{Arc, OnceLock};
use tracing::{info, error};

/// Global Registry untuk Shared Base Image RAM Cache.
/// Seluruh PC diskless yang booting dari Base OS VHD yang sama akan berbagi pool cache memori ini.
pub static IMAGE_CACHE_REGISTRY: OnceLock<DashMap<String, Arc<ReadAheadCache>>> = OnceLock::new();

pub fn get_image_cache_registry() -> &'static DashMap<String, Arc<ReadAheadCache>> {
    IMAGE_CACHE_REGISTRY.get_or_init(DashMap::new)
}

/// Mendapatkan atau membuat instance ReadAheadCache terdistribusi untuk Base VHD tertentu.
pub fn get_or_create_image_cache(base_path: &str, ram_cache_mb: u64) -> Option<Arc<ReadAheadCache>> {
    if ram_cache_mb == 0 {
        return None;
    }
    // Normalisasi path agar konsisten untuk case-insensitive Windows filesystem
    let key = std::path::Path::new(base_path)
        .canonicalize()
        .map(|p| p.to_string_lossy().to_lowercase())
        .unwrap_or_else(|_| base_path.to_lowercase());

    let registry = get_image_cache_registry();
    if let Some(existing) = registry.get(&key) {
        return Some(Arc::clone(&existing));
    }

    if let Some(new_cache) = ReadAheadCache::new_mb(ram_cache_mb, 256 * 1024) {
        let arc_cache = Arc::new(new_cache);
        registry.insert(key, Arc::clone(&arc_cache));
        Some(arc_cache)
    } else {
        None
    }
}

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
    _client_ip: &str,
    target_suffix: &str,
    is_super: bool,
) -> Result<ChildVhdResult, std::io::Error> {
    // Resolve parent/base VHD path via image_manager config
    let base_path = writeback_super::resolve_base_path(config, target_suffix);

    // Ambil alokasi per-disk ram_cache_mb untuk windows image, default 0 (Zero Auto-Assign)
    let ram_cache_mb = config.windows.as_ref()
        .and_then(|w| w.ram_cache_mb)
        .unwrap_or(0);

    let shared_cache = get_or_create_image_cache(&base_path, ram_cache_mb);

    if is_super {
        // === SUPER CLIENT: serve super VHD langsung ===
        let super_path = writeback_super::get_super_path(config, target_suffix);

        // Init super VHD (create differencing kalo belum ada)
        writeback_super::init_super_vhd(&base_path, &super_path)?;

        // Buka super VHD sebagai differencing backend dengan shared cache
        let backend = Backend::new_vhd_diff_with_shared_cache(
            &super_path,
            &base_path,
            config.windows.as_ref().unwrap().block_size,
            &config.windows.as_ref().unwrap().vendor_id,
            &config.windows.as_ref().unwrap().product_id,
            &config.windows.as_ref().unwrap().product_revision,
            shared_cache,
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
        // === NORMAL CLIENT: open base VHD directly as read-only dengan shared cache ===
        let backend = Backend::new_vhd_with_shared_cache(
            &base_path,
            config.windows.as_ref().unwrap().block_size,
            &config.windows.as_ref().unwrap().vendor_id,
            &config.windows.as_ref().unwrap().product_id,
            &config.windows.as_ref().unwrap().product_revision,
            shared_cache,
        ).map_err(|e| {
            error!("Gagal membuka base VHD {}: {}", base_path, e);
            e
        })?;

        info!("Normal Client: membuka base VHD {} (menggunakan shared RAM cache)", base_path);

        Ok(ChildVhdResult {
            backend: Arc::new(backend),
            child_path: None, // No child VHD to cleanup!
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_shared_vhd_cache_multi_client() {
        let dir = std::env::temp_dir();
        let vhd_path = dir.join(format!("test_shared_base_{}.vhd", uuid::Uuid::new_v4()));
        let vhd_str = vhd_path.to_str().unwrap();

        // 1. Buat mock fixed VHD 1 MB
        {
            let mut file = File::create(&vhd_path).unwrap();
            let mut payload = vec![0x33u8; 1024 * 1024];
            // Tandai sektor pertama
            payload[0..4].copy_from_slice(b"BOOT");
            file.write_all(&payload).unwrap();

            let mut footer = vec![0u8; 512];
            footer[0..8].copy_from_slice(b"conectix");
            footer[16..24].copy_from_slice(&0xFFFFFFFFFFFFFFFFu64.to_be_bytes());
            footer[48..56].copy_from_slice(&(1024 * 1024u64).to_be_bytes());
            footer[60..64].copy_from_slice(&2u32.to_be_bytes()); // type 2 = Fixed
            file.write_all(&footer).unwrap();
        }

        // 2. Dapatkan shared cache pool (64 MB)
        let cache_pool = get_or_create_image_cache(vhd_str, 64).expect("Cache pool harus berhasil dibuat");

        // 3. Client 1 membuka base VHD dengan cache_pool
        let client1 = Backend::new_vhd_with_shared_cache(
            vhd_str,
            512,
            "RUSTISCS",
            "Win11Disk",
            "1.00",
            Some(Arc::clone(&cache_pool)),
        ).unwrap();

        // 4. Client 2 membuka base VHD yang sama dengan cache_pool
        let client2 = Backend::new_vhd_with_shared_cache(
            vhd_str,
            512,
            "RUSTISCS",
            "Win11Disk",
            "1.00",
            Some(Arc::clone(&cache_pool)),
        ).unwrap();

        // Client 1 membaca LBA 0 (1 sektor = 512 byte) -> Cache Miss, chunk dimuat ke shared cache
        let mut buf1 = vec![0u8; 512];
        client1.read_blocks(0, 1, &mut buf1).unwrap();
        assert_eq!(&buf1[0..4], b"BOOT");

        let stats_after_client1 = cache_pool.stats();
        assert_eq!(stats_after_client1.hit_count, 0);
        assert_eq!(stats_after_client1.miss_count, 1);

        // Client 2 membaca LBA 0 yang sama persis -> Harus 100% CACHE HIT!
        let mut buf2 = vec![0u8; 512];
        client2.read_blocks(0, 1, &mut buf2).unwrap();
        assert_eq!(&buf2[0..4], b"BOOT");

        let stats_after_client2 = cache_pool.stats();
        assert_eq!(stats_after_client2.hit_count, 1, "Client 2 harus mencatat cache hit!");
        assert_eq!(stats_after_client2.miss_count, 1);
        assert!((stats_after_client2.hit_rate_pct - 50.0).abs() < 0.1);

        // Cleanup
        let _ = std::fs::remove_file(vhd_path);
    }
}
