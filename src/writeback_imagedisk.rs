//! Writeback untuk ImageDisk (Windows iSCSI sanboot)
//! Mengelola lifecycle VHD differencing:
//! - Super client: serve super VHD langsung (persistent)
//! - Normal client: base VHD read-only + .bin 4KB COW (fallback child VHD untuk base differencing)

use crate::backend::Backend;
use crate::vhd::VhdBackend;
use crate::config::Config;
use crate::read_ahead::ReadAheadCache;
use crate::writeback_super;
use dashmap::DashMap;
use std::sync::{Arc, OnceLock};
use tracing::{info, error, warn};

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
    client_ip: &str,
    target_suffix: &str,
    is_super: bool,
) -> Result<ChildVhdResult, std::io::Error> {
    // Resolve parent/base VHD path via image_manager config
    let base_path = writeback_super::resolve_base_path(config, target_suffix);

    // Cek apakah klien terdaftar dengan driver CCBoot di clients.toml
    let is_ccboot_client = if let Ok(clients) = crate::config::load_clients("clients.toml") {
        clients.values().any(|c| c.ip == client_ip && c.driver == crate::config::BootDriver::Ccboot)
    } else {
        false
    };

    // Resolusi SCSI Disk Identity: CCBoot terkunci ("CCBoot", "Modified", "1.00"), Native mengikuti config.toml
    let (effective_vendor, effective_product, effective_rev) = if is_ccboot_client {
        info!("ImageBoot LUN 0: Klien {} menggunakan driver CCBoot -> Identitas SCSI: 'CCBoot Modified 1.00'", client_ip);
        ("CCBoot".to_string(), "Modified".to_string(), "1.00".to_string())
    } else {
        let win = config.windows.as_ref();
        let v = win.map(|w| w.vendor_id.clone()).unwrap_or_else(|| "RUSTISCS".to_string());
        let p = win.map(|w| w.product_id.clone()).unwrap_or_else(|| "Virtual Disk".to_string());
        let r = win.map(|w| w.product_revision.clone()).unwrap_or_else(|| "1.00".to_string());
        info!("ImageBoot LUN 0: Klien {} menggunakan driver Native -> Identitas SCSI: '{} {} {}'", client_ip, v, p, r);
        (v, p, r)
    };

    if is_super {
        // === SUPER CLIENT: serve super VHD langsung ===
        let super_path = writeback_super::get_super_path(config, target_suffix);

        // Init super VHD (create differencing kalo belum ada)
        writeback_super::init_super_vhd(&base_path, &super_path)?;

        // Buka super VHD tanpa shared cache (key cache hanya chunk_id, rawan aliasing)
        let backend = Backend::new_vhd_diff_with_shared_cache(
            &super_path,
            &base_path,
            config.windows.as_ref().unwrap().block_size,
            &effective_vendor,
            &effective_product,
            &effective_rev,
            None,
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
        let disk_type = vhd_disk_type(&base_path)?;
        if disk_type == 4 {
            warn!(
                "Base VHD {} adalah differencing (type 4); fallback ke child VHD lama",
                base_path
            );
        }
        if disk_type != 4 {
            let win = config.windows.as_ref().unwrap();
            // ImageDisk ikut cache server.read_cache_gb kalau ram_cache_mb tidak diisi/0.
            let cache_mb = win
                .ram_cache_mb
                .filter(|mb| *mb > 0)
                .unwrap_or_else(|| config.server.read_cache_gb.saturating_mul(1024));
            info!("ImageDisk read cache: {} MB ({})", cache_mb, base_path);
            let shared = get_or_create_image_cache(&base_path, cache_mb);
            let backend = Backend::new_vhd_with_shared_cache(
                &base_path,
                win.block_size,
                &effective_vendor,
                &effective_product,
                &effective_rev,
                shared,
            )?;
            info!(
                "ImageDisk normal memakai base VHD + .bin: {} (base: {})",
                client_ip, base_path
            );
            return Ok(ChildVhdResult {
                backend: Arc::new(backend),
                child_path: None,
            });
        }

        // === NORMAL CLIENT: child VHD sementara dari base VHD ===
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

        let dirs = &config.writeback.writeback_dirs;
        let mut dir = if dirs.is_empty() {
            config.windows.as_ref().unwrap().vhd_dir.clone()
        } else {
            let idx = NEXT_DIR.fetch_add(1, Ordering::Relaxed) % dirs.len();
            dirs[idx].clone()
        };
        if dir.trim().is_empty() {
            dir = std::env::temp_dir().to_string_lossy().to_string();
        }
        std::fs::create_dir_all(&dir)?;

        let safe_ip: String = client_ip.chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' })
            .collect();
        let safe_target: String = target_suffix.chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        let child_path = format!("{}\\{}-{}-{}.vhd", dir, safe_ip, safe_target, uuid::Uuid::new_v4());

        VhdBackend::create_differencing(&base_path, &child_path)
            .map_err(|e| {
                error!("Gagal membuat child VHD {}: {}", child_path, e);
                e
            })?;

        let backend = Backend::new_vhd_diff_with_shared_cache(
            &child_path,
            &base_path,
            config.windows.as_ref().unwrap().block_size,
            &effective_vendor,
            &effective_product,
            &effective_rev,
            None,
        ).map_err(|e| {
            error!("Gagal membuka child VHD {}: {}", child_path, e);
            let _ = std::fs::remove_file(&child_path);
            e
        })?;

        info!("Normal Client: child VHD {} (base: {})", child_path, base_path);

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

fn vhd_disk_type(path: &str) -> Result<u32, std::io::Error> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    if len < 512 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file VHD terlalu kecil",
        ));
    }
    file.seek(SeekFrom::End(-512))?;
    let mut footer = [0u8; 512];
    file.read_exact(&mut footer)?;
    if &footer[0..8] != b"conectix" {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "footer VHD tidak valid",
        ));
    }
    Ok(u32::from_be_bytes(footer[60..64].try_into().unwrap()))
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

    #[test]
    fn test_dynamic_scsi_identity_ccboot_vs_native() {
        let dir = std::env::temp_dir();
        let vhd_path = dir.join(format!("test_vhd_ident_{}.vhd", uuid::Uuid::new_v4()));

        // Buat mock fixed VHD
        {
            let mut file = File::create(&vhd_path).unwrap();
            let payload = vec![0x33u8; 1024 * 1024];
            file.write_all(&payload).unwrap();
            let mut footer = vec![0u8; 512];
            footer[0..8].copy_from_slice(b"conectix");
            footer[16..24].copy_from_slice(&0xFFFFFFFFFFFFFFFFu64.to_be_bytes());
            footer[48..56].copy_from_slice(&(1024 * 1024u64).to_be_bytes());
            footer[60..64].copy_from_slice(&2u32.to_be_bytes());
            file.write_all(&footer).unwrap();
        }

        let mut config = Config::default();
        let mut img_map = std::collections::HashMap::new();
        img_map.insert("w11".to_string(), vhd_path.to_str().unwrap().to_string());
        config.image_manager = Some(img_map);
        config.windows = Some(crate::config::WindowsConfig {
            target_iqn_prefix: "iqn.win:".to_string(),
            vhd_dir: "".to_string(),
            block_size: 512,
            vendor_id: "CUSTOM_NATIVE".to_string(),
            product_id: "NativeDisk".to_string(),
            product_revision: "2.50".to_string(),
            discovery: false,
            super_client_ip: "".to_string(),
            super_client_action: "".to_string(),
            ram_cache_mb: Some(0),
            client_cache_enabled: false,
        });

        crate::config::test_support::with_clients_file(
            "[[client]]\nmac = \"00:0C:29:9B:72:2A\"\nip = \"192.168.180.250\"\nhostname = \"test-ccboot\"\ndriver = \"ccboot\"\n",
            || {
                // 1. Klien CCBoot (192.168.180.250 terdaftar sebagai driver ccboot di clients.toml)
                let res_ccboot = init_child_vhd(&config, "192.168.180.250", "w11", false).unwrap();
                assert_eq!(res_ccboot.backend.vendor_id, "CCBoot");
                assert_eq!(res_ccboot.backend.product_id, "Modified");
                assert_eq!(res_ccboot.backend.product_revision, "1.00");

                // 2. Klien Native (IP tidak terdaftar atau native)
                let res_native = init_child_vhd(&config, "192.168.180.99", "w11", false).unwrap();
                assert_eq!(res_native.backend.vendor_id, "CUSTOM_NATIVE");
                assert_eq!(res_native.backend.product_id, "NativeDisk");
                assert_eq!(res_native.backend.product_revision, "2.50");
            },
        );

        let _ = std::fs::remove_file(vhd_path);
    }

    #[test]
    fn test_normal_imagedisk_uses_base_bin_overlay() {
        let dir = std::env::temp_dir().join(format!("simpleiscsi_img_bin_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let vhd_path = dir.join("base.vhd");

        {
            let mut file = File::create(&vhd_path).unwrap();
            let payload = vec![0x33u8; 1024 * 1024];
            file.write_all(&payload).unwrap();
            let mut footer = vec![0u8; 512];
            footer[0..8].copy_from_slice(b"conectix");
            footer[16..24].copy_from_slice(&0xFFFFFFFFFFFFFFFFu64.to_be_bytes());
            footer[48..56].copy_from_slice(&(1024 * 1024u64).to_be_bytes());
            footer[60..64].copy_from_slice(&2u32.to_be_bytes());
            file.write_all(&footer).unwrap();
        }

        let mut config = Config::default();
        config.windows = Some(crate::config::WindowsConfig {
            target_iqn_prefix: "iqn.win:".to_string(),
            vhd_dir: "".to_string(),
            block_size: 512,
            vendor_id: "RUSTISCS".to_string(),
            product_id: "WindowsBoot".to_string(),
            product_revision: "1.00".to_string(),
            discovery: false,
            super_client_ip: "".to_string(),
            super_client_action: "".to_string(),
            ram_cache_mb: Some(0),
            client_cache_enabled: false,
        });
        let mut img_map = std::collections::HashMap::new();
        img_map.insert("w11".to_string(), vhd_path.to_str().unwrap().to_string());
        config.image_manager = Some(img_map);

        let res = init_child_vhd(&config, "192.168.1.99", "w11", false).unwrap();
        assert!(res.child_path.is_none());

        let cache = crate::writeback_gamedisk::ClientCache::new(
            &[dir.to_string_lossy().to_string()],
            "192.168.1.99",
            "w11_lun0",
            Arc::clone(&res.backend),
            0,
            false,
            0,
        )
        .unwrap();
        cache.write_stream(0, 0, &vec![0xAAu8; 512]).unwrap();

        let mut buf = vec![0u8; 1024];
        cache
            .read_blocks_cached(res.backend.as_ref(), 0, 2, &mut buf)
            .unwrap();
        assert_eq!(&buf[0..4], &[0xAAu8; 4]);
        assert_eq!(&buf[512..516], &[0x33u8; 4]);

        drop(cache);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
