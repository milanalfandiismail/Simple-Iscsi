use std::fs;
use crate::config::ClientsConfig;

pub fn get_clients_json() -> String {
    match fs::read_to_string("clients.toml") {
        Ok(content) => {
            match toml::from_str::<ClientsConfig>(&content) {
                Ok(clients_cfg) => {
                    match serde_json::to_string(&clients_cfg) {
                        Ok(json_str) => crate::server_api::build_response(200, "OK", "application/json", &json_str),
                        Err(e) => crate::server_api::build_response(500, "Internal Server Error", "text/plain", &e.to_string()),
                    }
                }
                Err(e) => crate::server_api::build_response(500, "Internal Server Error", "text/plain", &e.to_string()),
            }
        }
        Err(e) => crate::server_api::build_response(500, "Internal Server Error", "text/plain", &e.to_string()),
    }
}

pub fn post_clients_json(body: &str) -> String {
    match serde_json::from_str::<ClientsConfig>(body) {
        Ok(clients_cfg) => {
            if clients_cfg.clients.is_empty() {
                // If there are no clients, write a template instead of `client = []`
                // because `client = []` followed by manual `[[client]]` causes a TOML syntax error.
                let default_clients = r#"# clients.toml - DHCP Clients configuration
# Make sure to indent client properties with 2 spaces for a clean structure.

# Example client entry:
# [[client]]
#   hostname        = "PC-01"
#   mac             = "00:0C:29:A4:BC:F2"
#   ip              = "192.168.137.100"
#   gateway         = "192.168.137.1"
#   dns             = "8.8.8.8"
#   pxe             = "sb-custom"
#   next_server     = "192.168.137.1"
#   image_manager   = "windows_11"
"#;
                if let Err(e) = fs::write("clients.toml", default_clients) {
                    crate::server_api::build_response(500, "Internal Server Error", "text/plain", &e.to_string())
                } else {
                    crate::server_api::build_response(200, "OK", "text/plain", "Clients saved successfully (empty)")
                }
            } else {
                match toml::to_string(&clients_cfg) {
                    Ok(toml_str) => {
                        if let Err(e) = fs::write("clients.toml", &toml_str) {
                            crate::server_api::build_response(500, "Internal Server Error", "text/plain", &e.to_string())
                        } else {
                            crate::server_api::build_response(200, "OK", "text/plain", "Clients saved successfully")
                        }
                    }
                    Err(e) => crate::server_api::build_response(500, "Internal Server Error", "text/plain", &e.to_string()),
                }
            }
        }
        Err(e) => crate::server_api::build_response(400, "Bad Request", "text/plain", &format!("Invalid JSON: {}", e)),
    }
}

pub fn get_clients() -> String {
    match fs::read_to_string("clients.toml") {
        Ok(content) => crate::server_api::build_response(200, "OK", "text/plain", &content),
        Err(e) => crate::server_api::build_response(500, "Internal Server Error", "text/plain", &e.to_string()),
    }
}

pub fn post_clients(body: &str) -> String {
    if let Err(e) = fs::write("clients.toml", body) {
        crate::server_api::build_response(500, "Internal Server Error", "text/plain", &e.to_string())
    } else {
        crate::server_api::build_response(200, "OK", "text/plain", "Clients saved successfully")
    }
}

pub fn post_clients_autofix(config: &crate::config_manager::SharedConfig) -> String {
    let config_guard = config.read();
    if let Some(ref dhcp_cfg) = config_guard.dhcp {
        let dhcp_end = dhcp_cfg.end_ip.clone().unwrap_or_else(|| {
            let start_parts: Vec<&str> = dhcp_cfg.start_ip.split('.').collect();
            format!("{}.{}.{}.{}", start_parts[0], start_parts[1], start_parts[2], 200)
        });
        match crate::config::auto_fix_duplicate_ips("clients.toml", &dhcp_cfg.start_ip, &dhcp_end) {
            Ok(_) => crate::server_api::build_response(200, "OK", "text/plain", "Duplicate client IPs auto-fixed"),
            Err(e) => crate::server_api::build_response(500, "Internal Server Error", "text/plain", &e.to_string()),
        }
    } else {
        crate::server_api::build_response(400, "Bad Request", "text/plain", "DHCP is not configured")
    }
}

pub fn post_superclient_set(
    config: &crate::config_manager::SharedConfig,
    stats: &std::sync::Arc<crate::stats::ServerStats>,
    body: &str,
) -> String {
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(body);
    match parsed {
        Ok(json_body) => {
            let raw_ip = json_body["ip"].as_str().unwrap_or("").trim();
            let raw_action = json_body["action"].as_str().unwrap_or("").trim().to_lowercase();
            
            let is_disable = raw_action == "disable" || raw_action == "none" || raw_action.is_empty() || raw_ip.is_empty();
            
            if !is_disable {
                // 1. VALIDASI ONLINE: Klien tidak boleh sedang online saat enable Super Client
                if let Some(client_s) = stats.client_stats.get(raw_ip) {
                    if client_s.active_sessions.load(std::sync::atomic::Ordering::Relaxed) > 0 {
                        let resp = serde_json::json!({
                            "status": "error",
                            "message": format!("Klien dengan IP {} sedang ONLINE! Matikan / shutdown PC klien terlebih dahulu sebelum mengaktifkan mode Super Client.", raw_ip)
                        });
                        return crate::server_api::build_response(400, "Bad Request", "application/json", &resp.to_string());
                    }
                }

                // 2. VALIDASI SINGLE SUPER CLIENT: Hanya 1 PC yang boleh menjadi Super Client sekaligus
                let current_cfg = config.read();
                if let Some(ref win) = current_cfg.windows {
                    if !win.super_client_ip.is_empty() && win.super_client_ip != raw_ip {
                        let resp = serde_json::json!({
                            "status": "error",
                            "message": format!("Super Client saat ini sedang aktif pada IP {}! Hanya 1 PC yang dapat menjadi Super Client sekaligus. Harap nonaktifkan klien tersebut terlebih dahulu.", win.super_client_ip)
                        });
                        return crate::server_api::build_response(400, "Bad Request", "application/json", &resp.to_string());
                    }
                }

                // 3. LANGSUNG BUAT DIFFERENCING VHD (.super.vhd) SAAT ENABLE
                let mut image_key = String::new();
                if let Ok(clients) = crate::config::load_clients("clients.toml") {
                    if let Some(c) = clients.values().find(|c| c.ip == raw_ip) {
                        image_key = c.image_manager.clone().unwrap_or_default();
                    }
                }
                if image_key.is_empty() {
                    if let Some(ref img_map) = current_cfg.image_manager {
                        if let Some(first_key) = img_map.keys().next() {
                            image_key = first_key.clone();
                        }
                    }
                }

                if !image_key.is_empty() {
                    let base_path = crate::writeback_super::resolve_base_path(&current_cfg, &image_key);
                    let super_path = crate::writeback_super::get_super_path(&current_cfg, &image_key);
                    
                    if !std::path::Path::new(&base_path).exists() {
                        let resp = serde_json::json!({
                            "status": "error",
                            "message": format!("File Base VHD untuk image '{}' tidak ditemukan di disk: {}", image_key, base_path)
                        });
                        return crate::server_api::build_response(404, "Not Found", "application/json", &resp.to_string());
                    }

                    match crate::writeback_super::init_super_vhd(&base_path, &super_path) {
                        Ok(is_new) => {
                            if is_new {
                                tracing::info!("⚡ Differencing VHD Super Client langsung dibuat di disk: {}", super_path);
                            } else {
                                tracing::info!("⚡ Differencing VHD Super Client siap digunakan: {}", super_path);
                            }
                        }
                        Err(e) => {
                            let resp = serde_json::json!({
                                "status": "error",
                                "message": format!("Gagal membuat differencing VHD Super Client: {}", e)
                            });
                            return crate::server_api::build_response(500, "Internal Server Error", "application/json", &resp.to_string());
                        }
                    }
                }
            }

            let (target_ip, target_action) = if is_disable {
                ("", "")
            } else {
                (raw_ip, "enable")
            };

            // 1. Update in-memory SharedConfig immediately so sessions reflect the change instantly
            config.set_super_client(target_ip.to_string(), target_action.to_string());

            // 2. Persist to config.toml
            if let Err(e) = crate::config_manager::update_super_client_config_file("config.toml", target_ip, target_action) {
                tracing::error!("Gagal menulis status super client ke config.toml: {}", e);
            }

            let msg = if is_disable {
                "Super Client berhasil dinonaktifkan".to_string()
            } else {
                format!("Super Client berhasil diaktifkan untuk IP {} (VHD differencing siap)", target_ip)
            };

            tracing::info!("Super client status diperbarui: ip='{}', action='{}'", target_ip, target_action);
            let resp_json = serde_json::json!({
                "status": "ok",
                "super_client_ip": target_ip,
                "super_client_action": target_action,
                "message": msg
            });
            crate::server_api::build_response(200, "OK", "application/json", &resp_json.to_string())
        }
        Err(_) => crate::server_api::build_response(400, "Bad Request", "application/json", r#"{"status":"error","message":"Format JSON request tidak valid"}"#),
    }
}

pub fn post_superclient_commit(
    config: &crate::config_manager::SharedConfig,
    stats: &std::sync::Arc<crate::stats::ServerStats>,
    body: &str,
) -> String {
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(body);
    match parsed {
        Ok(json_body) => {
            let hostname = json_body["hostname"].as_str().unwrap_or("").trim();
            let ip = json_body["ip"].as_str().unwrap_or("").trim();

            // VALIDASI ONLINE: Klien tidak boleh sedang online saat commit Super Client
            // — Commit saat klien masih konek ke iSCSI dapat menyebabkan partial merge dan korupsi VHD.
            if !ip.is_empty() {
                if let Some(client_s) = stats.client_stats.get(ip) {
                    if client_s.active_sessions.load(std::sync::atomic::Ordering::Relaxed) > 0 {
                        let resp = serde_json::json!({
                            "status": "error",
                            "message": format!("Klien dengan IP {} sedang ONLINE! Matikan / shutdown PC klien terlebih dahulu sebelum melakukan Commit. Merge VHD saat klien aktif dapat menyebabkan korupsi data.", ip)
                        });
                        return crate::server_api::build_response(400, "Bad Request", "application/json", &resp.to_string());
                    }
                }
            }

            // 1. Resolve image_key dari client mapping
            let mut image_key = String::new();
            if let Ok(clients) = crate::config::load_clients("clients.toml") {
                let client = clients.values().find(|c| {
                    (!hostname.is_empty() && c.hostname.as_deref() == Some(hostname)) ||
                    (!ip.is_empty() && c.ip == ip) ||
                    (!hostname.is_empty() && c.ip == hostname)
                });
                if let Some(c) = client {
                    image_key = c.image_manager.clone().unwrap_or_default();
                }
            }

            // Fallback: Jika tidak terdaftar di clients.toml, gunakan image pertama dari image_manager config
            let config_ref = config.read();
            if image_key.is_empty() {
                if let Some(ref img_map) = config_ref.image_manager {
                    if let Some(first_key) = img_map.keys().next() {
                        image_key = first_key.clone();
                    }
                }
            }

            if image_key.is_empty() {
                let _ = crate::config_manager::clear_super_client_config("config.toml");
                config.clear_super_client();
                let resp = serde_json::json!({
                    "status": "ok",
                    "message": "Tidak ada image VHD aktif yang terpasang. Mode Super Client telah dinonaktifkan."
                });
                return crate::server_api::build_response(200, "OK", "application/json", &resp.to_string());
            }

            let base_path = crate::writeback_super::resolve_base_path(&config_ref, &image_key);
            let super_path = crate::writeback_super::get_super_path(&config_ref, &image_key);

            if crate::writeback_super::super_exists(&super_path) {
                let _ = crate::vhd_merge::backup_before_merge(&base_path, &super_path);
                let config_path = "config.toml".to_string();
                let cfg_clone = config.clone();
                let img_key_clone = image_key.clone();
                crate::vhd_merge::set_merge_image(&img_key_clone);

                tokio::spawn(async move {
                    match crate::vhd_merge::merge_vhd(super_path.clone(), base_path).await {
                        Ok(_) => {
                            let _ = crate::writeback_super::delete_super(&super_path);
                            let _ = crate::config_manager::clear_super_client_config(&config_path);
                            cfg_clone.clear_super_client();
                            tracing::info!("Merge Super Client untuk image '{}' berhasil, konfigurasi super client dinonaktifkan.", img_key_clone);
                        }
                        Err(e) => {
                            tracing::error!("Merge Super Client gagal untuk image '{}': {}", img_key_clone, e);
                        }
                    }
                });
                let resp = serde_json::json!({
                    "status": "ok",
                    "message": "Proses merge VHD Super Client sedang berjalan di background"
                });
                crate::server_api::build_response(200, "OK", "application/json", &resp.to_string())
            } else {
                // File differencing .super.vhd belum terbentuk (klien belum booting)
                let _ = crate::config_manager::clear_super_client_config("config.toml");
                config.clear_super_client();
                let resp = serde_json::json!({
                    "status": "ok",
                    "message": "Tidak ada file differencing VHD (klien belum melakukan perubahan). Mode Super Client berhasil dinonaktifkan."
                });
                crate::server_api::build_response(200, "OK", "application/json", &resp.to_string())
            }
        }
        Err(_) => crate::server_api::build_response(400, "Bad Request", "application/json", r#"{"status":"error","message":"Format JSON request tidak valid"}"#),
    }
}

pub fn post_superclient_discard(
    config: &crate::config_manager::SharedConfig,
    stats: &std::sync::Arc<crate::stats::ServerStats>,
    body: &str,
) -> String {
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(body);
    match parsed {
        Ok(json_body) => {
            let hostname = json_body["hostname"].as_str().unwrap_or("").trim();
            let ip = json_body["ip"].as_str().unwrap_or("").trim();

            // VALIDASI ONLINE: Klien tidak boleh sedang online saat discard Super Client
            // — Menghapus differencing VHD saat klien masih menulis ke dalamnya via iSCSI
            //   akan menyebabkan VHD corrupt dan tidak bisa dibooting.
            if !ip.is_empty() {
                if let Some(client_s) = stats.client_stats.get(ip) {
                    if client_s.active_sessions.load(std::sync::atomic::Ordering::Relaxed) > 0 {
                        let resp = serde_json::json!({
                            "status": "error",
                            "message": format!("Klien dengan IP {} sedang ONLINE! Matikan / shutdown PC klien terlebih dahulu sebelum melakukan Discard. Menghapus differencing VHD saat klien aktif akan menyebabkan korupsi data.", ip)
                        });
                        return crate::server_api::build_response(400, "Bad Request", "application/json", &resp.to_string());
                    }
                }
            }

            // 1. Resolve image_key dari client mapping
            let mut image_key = String::new();
            if let Ok(clients) = crate::config::load_clients("clients.toml") {
                let client = clients.values().find(|c| {
                    (!hostname.is_empty() && c.hostname.as_deref() == Some(hostname)) ||
                    (!ip.is_empty() && c.ip == ip) ||
                    (!hostname.is_empty() && c.ip == hostname)
                });
                if let Some(c) = client {
                    image_key = c.image_manager.clone().unwrap_or_default();
                }
            }

            // Fallback: Jika tidak terdaftar di clients.toml, gunakan image pertama dari image_manager config
            let config_ref = config.read();
            if image_key.is_empty() {
                if let Some(ref img_map) = config_ref.image_manager {
                    if let Some(first_key) = img_map.keys().next() {
                        image_key = first_key.clone();
                    }
                }
            }

            if !image_key.is_empty() {
                let super_path = crate::writeback_super::get_super_path(&config_ref, &image_key);
                let _ = crate::writeback_super::delete_super(&super_path);
            }

            let _ = crate::config_manager::clear_super_client_config("config.toml");
            config.clear_super_client();
            
            let resp = serde_json::json!({
                "status": "ok",
                "message": "Perubahan Super Client berhasil dibatalkan dan mode super dinonaktifkan."
            });
            crate::server_api::build_response(200, "OK", "application/json", &resp.to_string())
        }
        Err(_) => crate::server_api::build_response(400, "Bad Request", "application/json", r#"{"status":"error","message":"Format JSON request tidak valid"}"#),
    }
}
