use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tracing::{info, error};

use crate::config::parse_human_cache_size;
use crate::config_manager::SharedConfig;
use crate::server_api::build_response;
use crate::service_manager::ServiceManager;
use crate::writeback_imagedisk::get_image_cache_registry;

#[derive(Debug, Serialize, Deserialize)]
pub struct DiskCacheStatItem {
    pub disk_id: String,
    pub name: String,
    pub path: String,
    pub disk_type: String, // "gamedisk" | "imagedisk"
    pub ram_cache_mb: u64,
    pub used_cache_mb: u64,
    pub entry_count: u64,
    pub total_chunks: u64,
    pub hit_count: u64,
    pub miss_count: u64,
    pub hit_rate_pct: f64,
    pub cached_read_bytes: u64,
    pub direct_read_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheSummaryResponse {
    pub total_allocated_mb: u64,
    pub total_used_mb: u64,
    pub total_hits: u64,
    pub total_misses: u64,
    pub overall_hit_rate_pct: f64,
    pub disks: Vec<DiskCacheStatItem>,
}

pub fn handle_get_cache_stats(
    config: &SharedConfig,
    service_manager: &Arc<ServiceManager>,
) -> String {
    let cfg = config.read();
    let mut disks = Vec::new();
    let mut total_allocated_mb = 0u64;
    let mut total_used_mb = 0u64;
    let mut total_hits = 0u64;
    let mut total_misses = 0u64;

    // 1. Ambil data Gamedisk dari konfigurasi dan sinkronkan dengan backends jika aktif
    let backends_lock = service_manager.get_backends();
    let backends = backends_lock.read().unwrap();
    let mut processed_luns = std::collections::HashSet::new();

    for (lun_idx, gd_cfg) in cfg.gamedisk.iter().enumerate() {
        let lun_id = lun_idx as u8;
        processed_luns.insert(lun_id);
        let path = gd_cfg.physical_disk.clone();
        let name = format!("GameDisk LUN {}", lun_id);
        let configured_mb = gd_cfg.ram_cache_mb.unwrap_or(0);

        if let Some(backend) = backends.get(&lun_id) {
            if let Some(cache) = backend.read_cache() {
                let stats = cache.stats();
                total_allocated_mb += stats.ram_cache_mb;
                total_used_mb += stats.used_cache_mb;
                total_hits += stats.hit_count;
                total_misses += stats.miss_count;

                disks.push(DiskCacheStatItem {
                    disk_id: format!("gamedisk_{}", lun_id),
                    name,
                    path,
                    disk_type: "gamedisk".to_string(),
                    ram_cache_mb: stats.ram_cache_mb,
                    used_cache_mb: stats.used_cache_mb,
                    entry_count: stats.entry_count,
                    total_chunks: stats.total_chunks,
                    hit_count: stats.hit_count,
                    miss_count: stats.miss_count,
                    hit_rate_pct: stats.hit_rate_pct,
                    cached_read_bytes: stats.cached_read_bytes,
                    direct_read_bytes: stats.direct_read_bytes,
                });
                continue;
            }
        }

        // Tampilkan slot GameDisk terkonfigurasi meskipun service / disk sedang offline
        total_allocated_mb += configured_mb;
        disks.push(DiskCacheStatItem {
            disk_id: format!("gamedisk_{}", lun_id),
            name,
            path,
            disk_type: "gamedisk".to_string(),
            ram_cache_mb: configured_mb,
            used_cache_mb: 0,
            entry_count: 0,
            total_chunks: (configured_mb * 1024 * 1024) / (256 * 1024),
            hit_count: 0,
            miss_count: 0,
            hit_rate_pct: 0.0,
            cached_read_bytes: 0,
            direct_read_bytes: 0,
        });
    }

    // Periksa backend tambahan jika ada yang tidak terdaftar di cfg.gamedisk
    for (lun_id, backend) in backends.iter() {
        if !processed_luns.contains(lun_id) {
            let path = format!("LUN-{}", lun_id);
            let name = format!("GameDisk LUN {}", lun_id);
            if let Some(cache) = backend.read_cache() {
                let stats = cache.stats();
                total_allocated_mb += stats.ram_cache_mb;
                total_used_mb += stats.used_cache_mb;
                total_hits += stats.hit_count;
                total_misses += stats.miss_count;

                disks.push(DiskCacheStatItem {
                    disk_id: format!("gamedisk_{}", lun_id),
                    name,
                    path,
                    disk_type: "gamedisk".to_string(),
                    ram_cache_mb: stats.ram_cache_mb,
                    used_cache_mb: stats.used_cache_mb,
                    entry_count: stats.entry_count,
                    total_chunks: stats.total_chunks,
                    hit_count: stats.hit_count,
                    miss_count: stats.miss_count,
                    hit_rate_pct: stats.hit_rate_pct,
                    cached_read_bytes: stats.cached_read_bytes,
                    direct_read_bytes: stats.direct_read_bytes,
                });
            }
        }
    }

    // 2. Ambil data Shared Base Image backends dari IMAGE_CACHE_REGISTRY
    let image_registry = get_image_cache_registry();
    for entry in image_registry.iter() {
        let path = entry.key().clone();
        let cache = entry.value();
        let stats = cache.stats();

        total_allocated_mb += stats.ram_cache_mb;
        total_used_mb += stats.used_cache_mb;
        total_hits += stats.hit_count;
        total_misses += stats.miss_count;

        let filename = std::path::Path::new(&path)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());

        let disk_id = format!("image_{}", filename.replace('.', "_"));

        disks.push(DiskCacheStatItem {
            disk_id,
            name: format!("Base Image ({})", filename),
            path,
            disk_type: "imagedisk".to_string(),
            ram_cache_mb: stats.ram_cache_mb,
            used_cache_mb: stats.used_cache_mb,
            entry_count: stats.entry_count,
            total_chunks: stats.total_chunks,
            hit_count: stats.hit_count,
            miss_count: stats.miss_count,
            hit_rate_pct: stats.hit_rate_pct,
            cached_read_bytes: stats.cached_read_bytes,
            direct_read_bytes: stats.direct_read_bytes,
        });
    }

    // Jika image belum pernah di-boot oleh klien tapi terkonfigurasi, tampilkan slot statusnya
    if image_registry.is_empty() {
        if let Some(ref win) = cfg.windows {
            let win_mb = win.ram_cache_mb.unwrap_or(cfg.server.read_cache_gb * 1024);
            disks.push(DiskCacheStatItem {
                disk_id: "image_windows_base".to_string(),
                name: "Base Image (Master VHD)".to_string(),
                path: win.vhd_dir.clone(),
                disk_type: "imagedisk".to_string(),
                ram_cache_mb: win_mb,
                used_cache_mb: 0,
                entry_count: 0,
                total_chunks: (win_mb * 1024 * 1024) / (256 * 1024),
                hit_count: 0,
                miss_count: 0,
                hit_rate_pct: 0.0,
                cached_read_bytes: 0,
                direct_read_bytes: 0,
            });
            total_allocated_mb += win_mb;
        }
    }

    let total_all = total_hits + total_misses;
    let overall_hit_rate_pct = if total_all > 0 {
        (total_hits as f64 / total_all as f64) * 100.0
    } else {
        0.0
    };

    let summary = CacheSummaryResponse {
        total_allocated_mb,
        total_used_mb,
        total_hits,
        total_misses,
        overall_hit_rate_pct,
        disks,
    };

    build_response(200, "OK", "application/json", &serde_json::to_string(&summary).unwrap_or_default())
}

#[derive(Deserialize)]
pub struct ClearCacheRequest {
    pub disk_id: Option<String>,
}

pub fn handle_post_cache_clear(
    body: &str,
    service_manager: &Arc<ServiceManager>,
) -> String {
    let req: Option<ClearCacheRequest> = serde_json::from_str(body).ok();
    let target_id = req.and_then(|r| r.disk_id).unwrap_or_else(|| "all".to_string());

    let backends_lock = service_manager.get_backends();
    let backends = backends_lock.read().unwrap();

    let mut cleared_count = 0;

    // Check gamedisks
    for (lun_id, backend) in backends.iter() {
        let cur_id = format!("gamedisk_{}", lun_id);
        if target_id == "all" || target_id == cur_id {
            if let Some(c) = backend.read_cache() {
                c.clear();
                cleared_count += 1;
            }
        }
    }

    // Check imagedisks
    let image_registry = get_image_cache_registry();
    for entry in image_registry.iter() {
        let path = entry.key();
        let filename = std::path::Path::new(path)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());
        let cur_id = format!("image_{}", filename.replace('.', "_"));

        if target_id == "all" || target_id == cur_id || target_id == "image_windows_base" {
            entry.value().clear();
            cleared_count += 1;
        }
    }

    info!("Cache clear dieksekusi untuk target '{}', {} pool dibersihkan", target_id, cleared_count);
    build_response(200, "OK", "application/json", &json!({
        "status": "ok",
        "message": format!("Berhasil membersihkan {} pool cache", cleared_count),
        "target": target_id
    }).to_string())
}

#[derive(Deserialize)]
pub struct UpdateCacheConfigRequest {
    pub disk_id: String,
    pub ram_cache_mb: Option<u64>,
    pub size_str: Option<String>,
}

pub fn handle_post_cache_config(
    body: &str,
    config: &SharedConfig,
    service_manager: &Arc<ServiceManager>,
) -> String {
    let req: UpdateCacheConfigRequest = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(e) => return build_response(400, "Bad Request", "application/json", &json!({"error": e.to_string()}).to_string()),
    };

    let target_mb = if let Some(mb) = req.ram_cache_mb {
        mb
    } else if let Some(ref s) = req.size_str {
        parse_human_cache_size(s)
    } else {
        return build_response(400, "Bad Request", "application/json", r#"{"error":"Ukuran cache tidak diberikan (ram_cache_mb atau size_str)"}"#);
    };

    let mut current_cfg = (*config.read()).clone();

    if req.disk_id.starts_with("gamedisk_") {
        let lun_str = &req.disk_id["gamedisk_".len()..];
        if let Ok(lun_id) = lun_str.parse::<u8>() {
            let lun_idx = lun_id as usize;
            if let Some(gd) = current_cfg.gamedisk.get_mut(lun_idx) {
                gd.ram_cache_mb = Some(target_mb);
                info!("Mengubah RAM cache Gamedisk LUN {} menjadi {} MB", lun_id, target_mb);

                // Live dynamic update on running backend!
                let backends_lock = service_manager.get_backends();
                let backends = backends_lock.read().unwrap();
                if let Some(backend) = backends.get(&lun_id) {
                    let new_cache = if target_mb > 0 {
                        crate::read_ahead::ReadAheadCache::new_mb(target_mb, 256 * 1024).map(Arc::new)
                    } else {
                        None
                    };
                    backend.set_read_cache(new_cache);
                    info!("Live dynamic cache backend LUN {} berhasil diubah ke {} MB", lun_id, target_mb);
                }
            } else {
                return build_response(404, "Not Found", "application/json", r#"{"error":"LUN Gamedisk tidak ditemukan"}"#);
            }
        } else {
            return build_response(400, "Bad Request", "application/json", r#"{"error":"Format disk_id tidak valid"}"#);
        }
    } else if req.disk_id.starts_with("image_") {
        if let Some(ref mut win) = current_cfg.windows {
            win.ram_cache_mb = Some(target_mb);
            info!("Mengubah RAM cache Base Image menjadi {} MB", target_mb);
        } else {
            return build_response(404, "Not Found", "application/json", r#"{"error":"Konfigurasi Windows image tidak ditemukan"}"#);
        }
        // Invalidate entry di registry agar diinisialisasi ulang dengan ukuran baru pada boot berikutnya
        let image_registry = get_image_cache_registry();
        image_registry.clear();
    } else {
        return build_response(400, "Bad Request", "application/json", r#"{"error":"Target disk_id tidak dikenali"}"#);
    }

    // Simpan ke config.toml
    match toml::to_string_pretty(&current_cfg) {
        Ok(toml_str) => {
            if let Err(e) = std::fs::write("config.toml", &toml_str) {
                error!("Gagal menulis file config.toml: {}", e);
                return build_response(500, "Internal Server Error", "application/json", &json!({"error": e.to_string()}).to_string());
            }
            config.update(current_cfg);
            info!("Berhasil menyimpan konfigurasi RAM cache baru ke config.toml");
            build_response(200, "OK", "application/json", &json!({
                "status": "ok",
                "message": format!("Alokasi cache untuk {} berhasil diubah menjadi {} MB", req.disk_id, target_mb),
                "disk_id": req.disk_id,
                "ram_cache_mb": target_mb
            }).to_string())
        }
        Err(e) => {
            error!("Gagal serialisasi konfigurasi TOML: {}", e);
            build_response(500, "Internal Server Error", "application/json", &json!({"error": e.to_string()}).to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::backend::Backend;
    use crate::stats::ServerStats;

    #[test]
    fn test_cache_api_stats_clear_and_config() {
        let temp_dir = std::env::temp_dir();
        let cfg_path = temp_dir.join(format!("test_cache_cfg_{}.toml", uuid::Uuid::new_v4()));
        let cfg_str = cfg_path.to_str().unwrap();

        let initial_toml = r#"
[server]
address = "127.0.0.1"
port = 3260
read_cache_gb = 2

[gamedisk_target]
target_iqn = "iqn.2026-01.com.example:gamedisk"
discovery = true

[[gamedisk]]
physical_disk = '\\.\PhysicalDrive1'
block_size = 4096
ram_cache_mb = 1024

[windows]
target_iqn_prefix = "iqn.2026-01.com.example:win-"
vhd_dir = 'C:\vhd'
block_size = 512
discovery = false
ram_cache_mb = 2048

[writeback]
writeback_dirs = ['C:\writeback']
max_cache_per_client_gb = 10
"#;
        std::fs::write(&cfg_path, initial_toml).unwrap();
        let cfg = crate::config::load_config(cfg_str).unwrap();
        let shared_cfg = SharedConfig::new(cfg);
        let stats = ServerStats::new();

        // Siapkan dummy backend dengan cache
        let mut backends_map: HashMap<u8, Arc<Backend>> = HashMap::new();
        let dummy = Backend::new_dummy("TEST", "CACHE", "1.0");
        dummy.set_read_cache(crate::read_ahead::ReadAheadCache::new_mb(1024, 256 * 1024).map(Arc::new));
        let arc_backend = Arc::new(dummy);
        backends_map.insert(0, Arc::clone(&arc_backend));

        let sm = ServiceManager::new(
            shared_cfg.clone(),
            stats,
            Arc::new(std::sync::RwLock::new(backends_map)),
        );

        // 1. Test GET /api/cache/stats
        let resp = handle_get_cache_stats(&shared_cfg, &sm);
        assert!(resp.starts_with("HTTP/1.1 200 OK"));
        let json_body = resp.split("\r\n\r\n").nth(1).unwrap();
        let val: serde_json::Value = serde_json::from_str(json_body).unwrap();
        assert!(val.get("disks").unwrap().as_array().unwrap().len() >= 1);
        let allocated = val.get("total_allocated_mb").unwrap().as_u64().unwrap();
        assert!(allocated >= 1024);

        // 2. Test POST /api/cache/clear
        let clear_resp = handle_post_cache_clear(r#"{"disk_id":"all"}"#, &sm);
        assert!(clear_resp.starts_with("HTTP/1.1 200 OK"));
        assert!(clear_resp.contains("Berhasil membersihkan"));

        // 3. Test POST /api/cache/config (Ubah ke 4G)
        let config_resp = handle_post_cache_config(
            r#"{"disk_id":"gamedisk_0","size_str":"4G"}"#,
            &shared_cfg,
            &sm,
        );
        assert!(config_resp.starts_with("HTTP/1.1 200 OK"));
        assert_eq!(shared_cfg.read().gamedisk[0].ram_cache_mb, Some(4096));
        assert_eq!(arc_backend.read_cache().unwrap().stats().ram_cache_mb, 4096);

        // Cleanup
        let _ = std::fs::remove_file(cfg_path);
    }
}
