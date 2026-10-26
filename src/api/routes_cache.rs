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
    pub disk_type: String, // "imagedisk_read" | "gamedisk_read" | "writeback_write"
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

    // 1. Image Disk (Base OS) - Driverless Read Cache
    // Hanya sertakan jika disk sudah di-assign (vhd_dir terisi) dan BUKAN di drive C:
    if let Some(ref win) = cfg.windows {
        let vhd_dir = win.vhd_dir.trim();
        let is_c_drive = vhd_dir.starts_with("c:") || vhd_dir.starts_with("C:")
            || vhd_dir.starts_with(r"\\.\c:") || vhd_dir.starts_with(r"\\.\C:");

        if !vhd_dir.is_empty() && !is_c_drive {
            let win_mb = win.ram_cache_mb.unwrap_or(0);
            total_allocated_mb += win_mb;

            let mut used_cache_mb = 0u64;
            let mut entry_count = 0u64;
            let mut hit_count = 0u64;
            let mut miss_count = 0u64;
            let mut cached_read_bytes = 0u64;
            let mut direct_read_bytes = 0u64;

            if win_mb > 0 {
                let image_registry = get_image_cache_registry();
                for entry in image_registry.iter() {
                    let st = entry.value().stats();
                    used_cache_mb += st.used_cache_mb;
                    entry_count += st.entry_count;
                    hit_count += st.hit_count;
                    miss_count += st.miss_count;
                    cached_read_bytes += st.cached_read_bytes;
                    direct_read_bytes += st.direct_read_bytes;
                }
            }

            total_used_mb += used_cache_mb;
            total_hits += hit_count;
            total_misses += miss_count;

            let total_reqs = hit_count + miss_count;
            let hit_rate_pct = if total_reqs > 0 {
                (hit_count as f64 / total_reqs as f64) * 100.0
            } else {
                0.0
            };

            let total_chunks = if win_mb > 0 {
                (win_mb * 1024 * 1024) / (256 * 1024)
            } else {
                0
            };

            let drive_letter = vhd_dir.trim_start_matches(r"\\.\").chars().next().unwrap_or('?').to_ascii_uppercase();
            let name = format!("Image Disk (Drive {})", drive_letter);

            disks.push(DiskCacheStatItem {
                disk_id: "disk_image".to_string(),
                name,
                path: win.vhd_dir.clone(),
                disk_type: "imagedisk_read".to_string(),
                ram_cache_mb: win_mb,
                used_cache_mb,
                entry_count,
                total_chunks,
                hit_count,
                miss_count,
                hit_rate_pct,
                cached_read_bytes,
                direct_read_bytes,
            });
        }
    }

    // 2. GameDisk LUNs - Driverless Read Cache
    // Hanya sertakan jika disk sudah di-assign dan BUKAN di drive C:
    let backends_lock = service_manager.get_backends();
    let backends = backends_lock.read().unwrap();

    for (lun_idx, gd_cfg) in cfg.gamedisk.iter().enumerate() {
        let lun_id = lun_idx as u8;
        let path = crate::config::gamedisk_path(gd_cfg);
        let path_trim = path.trim();
        let is_c_drive = path_trim.starts_with("c:") || path_trim.starts_with("C:")
            || path_trim.starts_with(r"\\.\c:") || path_trim.starts_with(r"\\.\C:");

        if path_trim.is_empty() || is_c_drive {
            continue; // Abaikan unassigned atau drive C:
        }

        let name = format!("GameDisk LUN {}", lun_id);
        let configured_mb = gd_cfg.ram_cache_mb.unwrap_or(0);

        if configured_mb > 0 {
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
                        disk_type: "gamedisk_read".to_string(),
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
        }

        total_allocated_mb += configured_mb;
        let total_chunks = if configured_mb > 0 {
            (configured_mb * 1024 * 1024) / (256 * 1024)
        } else {
            0
        };

        disks.push(DiskCacheStatItem {
            disk_id: format!("gamedisk_{}", lun_id),
            name,
            path,
            disk_type: "gamedisk_read".to_string(),
            ram_cache_mb: configured_mb,
            used_cache_mb: 0,
            entry_count: 0,
            total_chunks,
            hit_count: 0,
            miss_count: 0,
            hit_rate_pct: 0.0,
            cached_read_bytes: 0,
            direct_read_bytes: 0,
        });
    }

    // 3. Writeback Storage Disks - Driverless Write Cache / Buffer
    // Hanya sertakan jika disk writeback sudah di-assign dan BUKAN di drive C:
    let wb_dirs: Vec<&String> = cfg.writeback.writeback_dirs.iter()
        .filter(|d| {
            let t = d.trim();
            !t.is_empty() && !t.starts_with("c:") && !t.starts_with("C:")
                && !t.starts_with(r"\\.\c:") && !t.starts_with(r"\\.\C:")
        })
        .collect();

    if !wb_dirs.is_empty() {
        let wb_mb = cfg.writeback.ram_cache_mb.unwrap_or(0);
        total_allocated_mb += wb_mb;

        for (idx, dir) in wb_dirs.iter().enumerate() {
            let name = if wb_dirs.len() == 1 {
                "Writeback Storage".to_string()
            } else {
                format!("Writeback Storage {}", idx + 1)
            };

            disks.push(DiskCacheStatItem {
                disk_id: format!("writeback_{}", idx),
                name,
                path: (*dir).clone(),
                disk_type: "writeback_write".to_string(),
                ram_cache_mb: wb_mb,
                used_cache_mb: 0,
                entry_count: 0,
                total_chunks: if wb_mb > 0 { (wb_mb * 1024 * 1024) / (64 * 1024) } else { 0 },
                hit_count: 0,
                miss_count: 0,
                hit_rate_pct: 0.0,
                cached_read_bytes: 0,
                direct_read_bytes: 0,
            });
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
    if target_id == "all" || target_id == "disk_image" || target_id.starts_with("image") {
        let image_registry = get_image_cache_registry();
        for entry in image_registry.iter() {
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
    handle_post_cache_config_with_path(body, config, service_manager, "config.toml")
}

pub fn handle_post_cache_config_with_path(
    body: &str,
    config: &SharedConfig,
    service_manager: &Arc<ServiceManager>,
    config_file_path: &str,
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
                let p = gd.physical_disk.trim();
                if p.starts_with("c:") || p.starts_with("C:") || p.starts_with(r"\\.\c:") || p.starts_with(r"\\.\C:") {
                    return build_response(400, "Bad Request", "application/json", r#"{"error":"Drive C: adalah drive sistem host dan dilarang untuk iSCSI cache"}"#);
                }

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
    } else if req.disk_id == "disk_image" || req.disk_id.starts_with("image") || req.disk_id == "boot_disk" {
        if let Some(ref mut win) = current_cfg.windows {
            let vhd_dir = win.vhd_dir.trim();
            if vhd_dir.is_empty() {
                return build_response(400, "Bad Request", "application/json", r#"{"error":"Belum ada disk yang di-assign sebagai Image Disk di Disk Management"}"#);
            }
            if vhd_dir.starts_with("c:") || vhd_dir.starts_with("C:") || vhd_dir.starts_with(r"\\.\c:") || vhd_dir.starts_with(r"\\.\C:") {
                return build_response(400, "Bad Request", "application/json", r#"{"error":"Drive C: adalah drive sistem host dan dilarang untuk iSCSI cache"}"#);
            }

            win.ram_cache_mb = Some(target_mb);
            info!("Mengubah RAM cache Image Disk menjadi {} MB", target_mb);
        } else {
            return build_response(404, "Not Found", "application/json", r#"{"error":"Konfigurasi Windows image tidak ditemukan"}"#);
        }
        let image_registry = get_image_cache_registry();
        if target_mb == 0 {
            image_registry.clear();
        } else {
            for mut entry in image_registry.iter_mut() {
                if let Some(new_cache) = crate::read_ahead::ReadAheadCache::new_mb(target_mb, 256 * 1024) {
                    *entry.value_mut() = Arc::new(new_cache);
                }
            }
        }
    } else if req.disk_id.starts_with("writeback_") || req.disk_id == "writeback" {
        if current_cfg.writeback.writeback_dirs.is_empty() {
            return build_response(400, "Bad Request", "application/json", r#"{"error":"Belum ada disk yang di-assign sebagai Writeback di Disk Management"}"#);
        }
        for dir in &current_cfg.writeback.writeback_dirs {
            let d = dir.trim();
            if d.starts_with("c:") || d.starts_with("C:") || d.starts_with(r"\\.\c:") || d.starts_with(r"\\.\C:") {
                return build_response(400, "Bad Request", "application/json", r#"{"error":"Drive C: adalah drive sistem host dan dilarang untuk iSCSI cache"}"#);
            }
        }

        current_cfg.writeback.ram_cache_mb = Some(target_mb);
        info!("Mengubah RAM cache Writeback menjadi {} MB", target_mb);
    } else {
        return build_response(400, "Bad Request", "application/json", r#"{"error":"Target disk_id tidak dikenali"}"#);
    }
    // Simpan ke config.toml
    match toml::to_string_pretty(&current_cfg) {
        Ok(toml_str) => {
            if let Err(e) = std::fs::write(config_file_path, &toml_str) {
                error!("Gagal menulis file {}: {}", config_file_path, e);
                return build_response(500, "Internal Server Error", "application/json", &json!({"error": e.to_string()}).to_string());
            }
            config.update(current_cfg);
            info!("Berhasil menyimpan konfigurasi RAM cache baru ke {}", config_file_path);
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
vhd_dir = 'D:\vhd'
block_size = 512
discovery = false
ram_cache_mb = 2048

[writeback]
writeback_dirs = ['E:\writeback']
max_writeback_per_client_gb = 10
ram_cache_mb = 512
"#;
        std::fs::write(&cfg_path, initial_toml).unwrap();
        let cfg = crate::config::load_config(cfg_str).unwrap();
        let shared_cfg = SharedConfig::new(cfg);
        let stats = ServerStats::new();

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
        let disks = val.get("disks").unwrap().as_array().unwrap();
        // Should have 3 assigned disks: disk_image, gamedisk_0, writeback_0
        assert_eq!(disks.len(), 3);
        assert_eq!(disks[0]["disk_id"], "disk_image");
        assert_eq!(disks[0]["disk_type"], "imagedisk_read");
        assert_eq!(disks[1]["disk_id"], "gamedisk_0");
        assert_eq!(disks[1]["disk_type"], "gamedisk_read");
        assert_eq!(disks[2]["disk_id"], "writeback_0");
        assert_eq!(disks[2]["disk_type"], "writeback_write");

        // 2. Test POST /api/cache/clear
        let clear_resp = handle_post_cache_clear(r#"{"disk_id":"all"}"#, &sm);
        assert!(clear_resp.starts_with("HTTP/1.1 200 OK"));
        assert!(clear_resp.contains("Berhasil membersihkan"));

        // 2b. Test POST /api/cache/config to drive C: must return 400 Bad Request
        let reject_c_resp = handle_post_cache_config_with_path(
            r#"{"disk_id":"gamedisk_0","size_str":"2G"}"#,
            &SharedConfig::new({
                let mut c_cfg = (*shared_cfg.read()).clone();
                c_cfg.gamedisk[0].physical_disk = r"\\.\C:".to_string();
                c_cfg
            }),
            &sm,
            cfg_str,
        );
        assert!(reject_c_resp.starts_with("HTTP/1.1 400 Bad Request"));

        // 3. Test POST /api/cache/config (Ubah ke 4G)
        let config_resp = handle_post_cache_config_with_path(
            r#"{"disk_id":"gamedisk_0","size_str":"4G"}"#,
            &shared_cfg,
            &sm,
            cfg_str,
        );
        assert!(config_resp.starts_with("HTTP/1.1 200 OK"));
        assert_eq!(shared_cfg.read().gamedisk[0].ram_cache_mb, Some(4096));
        assert_eq!(arc_backend.read_cache().unwrap().stats().ram_cache_mb, 4096);

        // Cleanup
        let _ = std::fs::remove_file(cfg_path);
    }
}
