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

pub fn post_superclient_set(body: &str) -> String {
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(body);
    match parsed {
        Ok(json_body) => {
            let ip = json_body["ip"].as_str().unwrap_or("");
            let action = json_body["action"].as_str().unwrap_or("none");
            
            if let Ok(config_content) = fs::read_to_string("config.toml") {
                let mut new_lines = Vec::new();
                for line in config_content.lines() {
                    if line.trim().starts_with("super_client_ip") {
                        new_lines.push(format!("  super_client_ip     = \"{}\"", ip));
                    } else if line.trim().starts_with("super_client_action") {
                        new_lines.push(format!("  super_client_action = \"{}\"", action));
                    } else {
                        new_lines.push(line.to_string());
                    }
                }
                let _ = fs::write("config.toml", new_lines.join("\n"));
            }
            crate::server_api::build_response(200, "OK", "text/plain", "Super client IP and action configured")
        }
        Err(_) => crate::server_api::build_response(400, "Bad Request", "text/plain", "Invalid JSON body"),
    }
}

pub fn post_superclient_commit(config: &crate::config_manager::SharedConfig, body: &str) -> String {
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(body);
    match parsed {
        Ok(json_body) => {
            let hostname = json_body["hostname"].as_str().unwrap_or("");
            if let Ok(clients) = crate::config::load_clients("clients.toml") {
                let client = clients.values().find(|c| c.hostname.as_deref() == Some(hostname) || c.ip == hostname);
                if let Some(c) = client {
                    let image_key = c.image_manager.as_deref().unwrap_or("");
                    let config_ref = config.read();
                    let base_path = crate::writeback_super::resolve_base_path(&config_ref, image_key);
                    let super_path = crate::writeback_super::get_super_path(&config_ref, image_key);
                    
                    if crate::writeback_super::super_exists(&super_path) {
                        let _ = crate::vhd_merge::backup_before_merge(&base_path, &super_path);
                        let config_path = "config.toml".to_string();
                        tokio::spawn(async move {
                            if let Ok(_) = crate::vhd_merge::merge_vhd(super_path.clone(), base_path).await {
                                let _ = crate::writeback_super::delete_super(&super_path);
                                let _ = crate::config_manager::clear_super_client_config(&config_path);
                            }
                        });
                        crate::server_api::build_response(200, "OK", "text/plain", "Commit VHD task spawned in background")
                    } else {
                        crate::server_api::build_response(404, "Not Found", "text/plain", "Super VHD file not found")
                    }
                } else {
                    crate::server_api::build_response(404, "Not Found", "text/plain", "Client hostname not found")
                }
            } else {
                crate::server_api::build_response(500, "Internal Server Error", "text/plain", "Failed to load clients.toml")
            }
        }
        Err(_) => crate::server_api::build_response(400, "Bad Request", "text/plain", "Invalid JSON body"),
    }
}

pub fn post_superclient_discard(config: &crate::config_manager::SharedConfig, body: &str) -> String {
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(body);
    match parsed {
        Ok(json_body) => {
            let hostname = json_body["hostname"].as_str().unwrap_or("");
            if let Ok(clients) = crate::config::load_clients("clients.toml") {
                let client = clients.values().find(|c| c.hostname.as_deref() == Some(hostname) || c.ip == hostname);
                if let Some(c) = client {
                    let image_key = c.image_manager.as_deref().unwrap_or("");
                    let config_ref = config.read();
                    let super_path = crate::writeback_super::get_super_path(&config_ref, image_key);
                    
                    let _ = crate::writeback_super::delete_super(&super_path);
                    let _ = crate::config_manager::clear_super_client_config("config.toml");
                    
                    crate::server_api::build_response(200, "OK", "text/plain", "Super client VHD changes discarded successfully")
                } else {
                    crate::server_api::build_response(404, "Not Found", "text/plain", "Client hostname not found")
                }
            } else {
                crate::server_api::build_response(500, "Internal Server Error", "text/plain", "Failed to load clients.toml")
            }
        }
        Err(_) => crate::server_api::build_response(400, "Bad Request", "text/plain", "Invalid JSON body"),
    }
}
