use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use serde_json::json;
use tracing::{info, error};
use std::path::Path;
use std::fs;
use std::collections::HashMap;

use crate::config_manager::{SharedConfig, clear_super_client_config};
use crate::stats::ServerStats;
use crate::vhd_merge;
use crate::writeback_super;

pub async fn start_api_server(config: SharedConfig, stats: Arc<ServerStats>) {
    let addr = "127.0.0.1:8080";
    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            error!("Gagal bind API Server ke {}: {}", addr, e);
            return;
        }
    };
    info!("API Server berjalan di http://{}", addr);

    loop {
        match listener.accept().await {
            Ok((mut socket, _client_addr)) => {
                let config_clone = config.clone();
                let stats_clone = stats.clone();
                tokio::spawn(async move {
                    let mut req_bytes = Vec::new();
                    let mut buf = [0u8; 4096];
                    let mut content_length = None;
                    let mut headers_end = None;

                    loop {
                        match socket.read(&mut buf).await {
                            Ok(n) if n > 0 => {
                                req_bytes.extend_from_slice(&buf[..n]);

                                // Cari batas header \r\n\r\n
                                if headers_end.is_none() {
                                    if let Some(pos) = req_bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                                        headers_end = Some(pos + 4);
                                        // Parse Content-Length dari header
                                        let header_str = String::from_utf8_lossy(&req_bytes[..pos]);
                                        for line in header_str.lines() {
                                            if line.to_lowercase().starts_with("content-length:") {
                                                if let Some(val_str) = line.split(':').nth(1) {
                                                    if let Ok(len) = val_str.trim().parse::<usize>() {
                                                        content_length = Some(len);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // Jika header sudah terbaca dan body sudah lengkap
                                if let Some(h_end) = headers_end {
                                    let expected_len = h_end + content_length.unwrap_or(0);
                                    if req_bytes.len() >= expected_len {
                                        break;
                                    }
                                }
                            }
                            _ => break,
                        }
                    }

                    if !req_bytes.is_empty() {
                        let request = String::from_utf8_lossy(&req_bytes);
                        
                        // Intercept SSE Stream explicitly
                        if request.starts_with("GET /api/stats/stream ") {
                            crate::api::routes_stats::handle_sse_stream(socket, config_clone, stats_clone).await;
                        } else {
                            let response = handle_request(&request, &config_clone, &stats_clone).await;
                            let _ = socket.write_all(response.as_bytes()).await;
                            let _ = socket.flush().await;
                        }
                    }
                });
            }
            Err(e) => {
                error!("Gagal menerima koneksi API Server: {}", e);
            }
        }
    }
}

async fn handle_request(req: &str, config: &SharedConfig, stats: &Arc<ServerStats>) -> String {
    let mut lines = req.lines();
    let request_line = match lines.next() {
        Some(l) => l,
        None => return build_response(400, "Bad Request", "text/plain", "Invalid Request"),
    };

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return build_response(400, "Bad Request", "text/plain", "Invalid Request Line");
    }

    let method = parts[0];
    let path_and_query = parts[1];

    // Handle OPTIONS for CORS preflight
    if method == "OPTIONS" {
        return format!(
            "HTTP/1.1 204 No Content\r\n\
             Access-Control-Allow-Origin: *\r\n\
             Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
             Access-Control-Allow-Headers: Content-Type\r\n\
             Connection: close\r\n\r\n"
        );
    }

    // Split path and query parameters
    let path_parts: Vec<&str> = path_and_query.split('?').collect();
    let path = path_parts[0];
    let query_str = path_parts.get(1).cloned().unwrap_or("");

    // Parse query params simple helper
    let mut query_params = HashMap::new();
    for pair in query_str.split('&') {
        let kv: Vec<&str> = pair.split('=').collect();
        if kv.len() == 2 {
            query_params.insert(kv[0], kv[1]);
        }
    }

    // Extract POST body
    let mut body = String::new();
    if method == "POST" {
        let req_str = req.to_string();
        if let Some(pos) = req_str.find("\r\n\r\n") {
            body = req_str[pos + 4..].to_string();
        }
    }

    // API Routing
    match (method, path) {
        ("GET", "/api/stats") => {
            let payload = crate::api::routes_stats::get_stats_payload(config, stats);
            build_response(200, "OK", "application/json", &payload.to_string())
        }

        ("GET", "/api/config") => {
            match fs::read_to_string("config.toml") {
                Ok(content) => build_response(200, "OK", "text/plain", &content),
                Err(e) => build_response(500, "Internal Server Error", "text/plain", &e.to_string()),
            }
        }

        ("POST", "/api/config") => {
            if let Err(e) = fs::write("config.toml", &body) {
                build_response(500, "Internal Server Error", "text/plain", &e.to_string())
            } else {
                build_response(200, "OK", "text/plain", "Config saved successfully")
            }
        }

        ("GET", "/api/clients") => {
            crate::api::routes_client::get_clients()
        }

        ("POST", "/api/clients") => {
            crate::api::routes_client::post_clients(&body)
        }

        ("GET", "/api/config/json") => {
            match crate::config::load_config("config.toml") {
                Ok(cfg) => {
                    match serde_json::to_string(&cfg) {
                        Ok(json_str) => build_response(200, "OK", "application/json", &json_str),
                        Err(e) => build_response(500, "Internal Server Error", "text/plain", &e.to_string()),
                    }
                }
                Err(e) => build_response(500, "Internal Server Error", "text/plain", &e.to_string()),
            }
        }

        ("POST", "/api/config/json") => {
            info!("Menerima request pembaruan config: {}", body);
            match serde_json::from_str::<crate::config::Config>(&body) {
                Ok(cfg) => {
                    info!("Berhasil parsing config JSON ke struct: {:?}", cfg);
                    match toml::to_string(&cfg) {
                        Ok(toml_str) => {
                            info!("Berhasil serialisasi struct ke TOML:\n{}", toml_str);
                            if let Err(e) = fs::write("config.toml", &toml_str) {
                                error!("Gagal menulis file config.toml: {}", e);
                                build_response(500, "Internal Server Error", "text/plain", &e.to_string())
                            } else {
                                info!("Berhasil memperbarui file config.toml di disk.");
                                build_response(200, "OK", "text/plain", "Config saved successfully")
                            }
                        }
                        Err(e) => {
                            error!("Gagal serialisasi TOML: {}", e);
                            build_response(500, "Internal Server Error", "text/plain", &e.to_string())
                        }
                    }
                }
                Err(e) => {
                    error!("Gagal parsing JSON body: {}", e);
                    build_response(400, "Bad Request", "text/plain", &format!("Invalid JSON: {}", e))
                }
            }
        }

        ("GET", "/api/clients/json") => {
            crate::api::routes_client::get_clients_json()
        }

        ("POST", "/api/clients/json") => {
            crate::api::routes_client::post_clients_json(&body)
        }

        ("POST", "/api/clients/autofix") => {
            crate::api::routes_client::post_clients_autofix(config)
        }

        ("GET", "/api/system/drives") => {
            crate::api::routes_disk::get_system_drives()
        }

        ("GET", "/api/system/logical_drives_detail") => {
            crate::api::routes_disk::get_logical_drives_detail()
        }

        ("GET", "/api/system/network_interfaces") => {
            crate::api::routes_disk::get_network_interfaces()
        }

        ("GET", "/api/system/tftp_folders") => {
            crate::api::routes_tftp::get_tftp_folders(config)
        }

        ("POST", "/api/system/tftp_folders/create") => {
            crate::api::routes_tftp::post_tftp_folders_create(config, &body)
        }

        ("POST", "/api/system/tftp_folders/delete") => {
            crate::api::routes_tftp::post_tftp_folders_delete(config, &body)
        }

        ("GET", "/api/system/vhds") => {
            crate::api::routes_vhd::get_system_vhds(config)
        }

        ("POST", "/api/system/select_vhd") => {
            crate::api::routes_vhd::post_system_select_vhd()
        }

        ("GET", "/api/vhd") => {
            crate::api::routes_vhd::get_vhd(config)
        }

        ("GET", "/api/vhd/backups") => {
            crate::api::routes_vhd::get_vhd_backups(config, &query_params)
        }

        ("POST", "/api/vhd/restore") => {
            crate::api::routes_vhd::post_vhd_restore(config, &body)
        }

        ("POST", "/api/vhd/merge") => {
            crate::api::routes_vhd::post_vhd_merge(&body)
        }

        ("POST", "/api/superclient/set") => {
            crate::api::routes_client::post_superclient_set(&body)
        }

        ("POST", "/api/superclient/commit") => {
            crate::api::routes_client::post_superclient_commit(config, &body)
        }

        ("POST", "/api/superclient/discard") => {
            crate::api::routes_client::post_superclient_discard(config, &body)
        }

        ("GET", "/api/dhcp/leases") => {
            let mut list = Vec::new();
            for entry in stats.dhcp_leases.iter() {
                list.push(json!({
                    "mac": entry.key(),
                    "ip": entry.value(),
                }));
            }
            build_response(200, "OK", "application/json", &json!(list).to_string())
        }

        ("GET", "/api/tftp/files") => {
            crate::api::routes_tftp::get_tftp_files(config)
        }

        ("GET", "/api/tftp/read") => {
            crate::api::routes_tftp::get_tftp_read(config, &query_params)
        }

        ("POST", "/api/tftp/write") => {
            crate::api::routes_tftp::post_tftp_write(config, &body)
        }

        ("GET", "/api/writeback/files") => {
            crate::api::routes_disk::get_writeback_files(config)
        }

        ("POST", "/api/writeback/clear") => {
            crate::api::routes_disk::post_writeback_clear(config, &body)
        }

        ("GET", path) if !path.starts_with("/api/") => {
            crate::api::static_files::handle_static_files(path)
        }

        _ => build_response(404, "Not Found", "text/plain", "API Endpoint Not Found"),
    }
}

pub fn build_response(status_code: u16, status_text: &str, content_type: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {} {}\r\n\
         Content-Type: {}\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type\r\n\
         Connection: close\r\n\r\n{}",
        status_code, status_text, content_type, body.len(), body
    )
}
