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
                        let response = handle_request(&request, &config_clone, &stats_clone).await;
                        let _ = socket.write_all(response.as_bytes()).await;
                        let _ = socket.flush().await;
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
            let client_list: Vec<serde_json::Value> = stats.client_stats.iter().map(|entry| {
                let ip = entry.key();
                let client_s = entry.value();
                let is_active = client_s.active_sessions.load(std::sync::atomic::Ordering::Relaxed) > 0;
                
                let uptime_secs = if is_active {
                    client_s.session_start_time.lock()
                        .map(|t| t.elapsed().as_secs())
                        .unwrap_or(0)
                } else {
                    0
                };

                let last_duration_secs = client_s.last_session_duration.lock()
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                json!({
                    "ip": ip,
                    "active": is_active,
                    "uptime_secs": uptime_secs,
                    "last_duration_secs": last_duration_secs,
                    "bytes_read": client_s.bytes_read.load(std::sync::atomic::Ordering::Relaxed),
                    "bytes_written": client_s.bytes_written.load(std::sync::atomic::Ordering::Relaxed),
                })
            }).collect();

            let config_guard = config.read();
            let dhcp_enabled = config_guard.dhcp.as_ref().map(|d| d.enabled).unwrap_or(false);
            let tftp_enabled = config_guard.dhcp.as_ref().map(|d| d.enabled && !d.tftp_dir.is_empty()).unwrap_or(false);
            let iscsi_port = config_guard.server.port;

            let payload = json!({
                "total_connections": stats.total_connections.load(std::sync::atomic::Ordering::Relaxed),
                "active_sessions": stats.active_sessions.load(std::sync::atomic::Ordering::Relaxed),
                "cache_hits": stats.cache_hits.load(std::sync::atomic::Ordering::Relaxed),
                "cache_misses": stats.cache_misses.load(std::sync::atomic::Ordering::Relaxed),
                "bytes_read": stats.bytes_read.load(std::sync::atomic::Ordering::Relaxed),
                "bytes_written": stats.bytes_written.load(std::sync::atomic::Ordering::Relaxed),
                "clients": client_list,
                "services": {
                    "iscsi": { "enabled": true, "port": iscsi_port },
                    "dhcp": { "enabled": dhcp_enabled, "port": 67 },
                    "tftp": { "enabled": tftp_enabled, "port": 69 }
                }
            });

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
            match fs::read_to_string("clients.toml") {
                Ok(content) => build_response(200, "OK", "text/plain", &content),
                Err(e) => build_response(500, "Internal Server Error", "text/plain", &e.to_string()),
            }
        }

        ("POST", "/api/clients") => {
            if let Err(e) = fs::write("clients.toml", &body) {
                build_response(500, "Internal Server Error", "text/plain", &e.to_string())
            } else {
                build_response(200, "OK", "text/plain", "Clients saved successfully")
            }
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
            match fs::read_to_string("clients.toml") {
                Ok(content) => {
                    match toml::from_str::<crate::config::ClientsConfig>(&content) {
                        Ok(clients_cfg) => {
                            match serde_json::to_string(&clients_cfg) {
                                Ok(json_str) => build_response(200, "OK", "application/json", &json_str),
                                Err(e) => build_response(500, "Internal Server Error", "text/plain", &e.to_string()),
                            }
                        }
                        Err(e) => build_response(500, "Internal Server Error", "text/plain", &e.to_string()),
                    }
                }
                Err(e) => build_response(500, "Internal Server Error", "text/plain", &e.to_string()),
            }
        }

        ("POST", "/api/clients/json") => {
            match serde_json::from_str::<crate::config::ClientsConfig>(&body) {
                Ok(clients_cfg) => {
                    match toml::to_string(&clients_cfg) {
                        Ok(toml_str) => {
                            if let Err(e) = fs::write("clients.toml", &toml_str) {
                                build_response(500, "Internal Server Error", "text/plain", &e.to_string())
                            } else {
                                build_response(200, "OK", "text/plain", "Clients saved successfully")
                            }
                        }
                        Err(e) => build_response(500, "Internal Server Error", "text/plain", &e.to_string()),
                    }
                }
                Err(e) => build_response(400, "Bad Request", "text/plain", &format!("Invalid JSON: {}", e)),
            }
        }

        ("POST", "/api/clients/autofix") => {
            let config_guard = config.read();
            if let Some(ref dhcp_cfg) = config_guard.dhcp {
                let dhcp_end = dhcp_cfg.end_ip.clone().unwrap_or_else(|| {
                    let start_parts: Vec<&str> = dhcp_cfg.start_ip.split('.').collect();
                    format!("{}.{}.{}.{}", start_parts[0], start_parts[1], start_parts[2], 200)
                });
                match crate::config::auto_fix_duplicate_ips("clients.toml", &dhcp_cfg.start_ip, &dhcp_end) {
                    Ok(_) => build_response(200, "OK", "text/plain", "Duplicate client IPs auto-fixed"),
                    Err(e) => build_response(500, "Internal Server Error", "text/plain", &e.to_string()),
                }
            } else {
                build_response(400, "Bad Request", "text/plain", "DHCP is not configured")
            }
        }

        ("GET", "/api/system/drives") => {
            let mut drives = Vec::new();
            for i in 0..16 {
                let path = format!("\\\\.\\PhysicalDrive{}", i);
                let file_opts = std::fs::OpenOptions::new()
                    .read(true)
                    .open(&path);
                if let Ok(file) = file_opts {
                    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
                    drives.push(json!({
                        "path": path,
                        "size": size,
                    }));
                }
            }
            build_response(200, "OK", "application/json", &json!(drives).to_string())
        }

        ("GET", "/api/system/logical_drives_detail") => {
            use std::os::windows::io::AsRawHandle;
            
            extern "system" {
                fn DeviceIoControl(
                    hDevice: *mut std::ffi::c_void,
                    dwIoControlCode: u32,
                    lpInBuffer: *mut std::ffi::c_void,
                    nInBufferSize: u32,
                    lpOutBuffer: *mut std::ffi::c_void,
                    nOutBufferSize: u32,
                    lpBytesReturned: *mut u32,
                    lpOverlapped: *mut std::ffi::c_void,
                ) -> i32;
            }

            #[repr(C)]
            struct DiskExtent {
                disk_number: u32,
                starting_offset: i64,
                extent_length: i64,
            }

            #[repr(C)]
            struct VolumeDiskExtents {
                number_of_disk_extents: u32,
                extents: [DiskExtent; 1],
            }

            let mut details = Vec::new();
            for c in b'A'..=b'Z' {
                let letter = (c as char).to_string();
                let path = format!("{}:\\", letter);
                if std::path::Path::new(&path).exists() {
                    let disk_num = {
                        let dev_path = format!("\\\\.\\{}:", letter);
                        if let Ok(file) = std::fs::OpenOptions::new().read(true).open(&dev_path) {
                            let handle = file.as_raw_handle();
                            unsafe {
                                let mut extents: VolumeDiskExtents = std::mem::zeroed();
                                let mut bytes_returned = 0u32;
                                let res = DeviceIoControl(
                                    handle as _,
                                    5636096, // IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS
                                    std::ptr::null_mut(),
                                    0,
                                    &mut extents as *mut _ as _,
                                    std::mem::size_of::<VolumeDiskExtents>() as u32,
                                    &mut bytes_returned,
                                    std::ptr::null_mut(),
                                );
                                if res != 0 && extents.number_of_disk_extents > 0 {
                                    Some(extents.extents[0].disk_number)
                                } else {
                                    None
                                }
                            }
                        } else {
                            None
                        }
                    };

                    details.push(json!({
                        "letter": letter,
                        "physical_disk": disk_num.map(|num| format!("\\\\.\\PhysicalDrive{}", num)),
                    }));
                }
            }
            build_response(200, "OK", "application/json", &json!(details).to_string())
        }

        ("GET", "/api/system/network_interfaces") => {
            #[repr(C)]
            #[derive(Clone, Copy)]
            struct MibIpAddrRow {
                dw_addr: u32,
                dw_index: u32,
                dw_mask: u32,
                dw_bc_addr: u32,
                dw_reasm_size: u32,
                unused1: u16,
                unused2: u16,
            }

            extern "system" {
                fn GetIpAddrTable(
                    pIpAddrTable: *mut u8,
                    pdwSize: *mut u32,
                    bOrder: i32,
                ) -> u32;
            }

            let mut size = 0;
            unsafe {
                GetIpAddrTable(std::ptr::null_mut(), &mut size, 0);
            }

            let mut ips = Vec::new();
            if size > 0 {
                let mut buf = vec![0u8; size as usize];
                let ret = unsafe {
                    GetIpAddrTable(buf.as_mut_ptr(), &mut size, 0)
                };

                if ret == 0 {
                    let num_entries = unsafe { *(buf.as_ptr() as *const u32) };
                    let row_ptr = unsafe { buf.as_ptr().add(4) as *const MibIpAddrRow };
                    
                    for i in 0..num_entries {
                        let row = unsafe { *row_ptr.add(i as usize) };
                        let ip_addr = std::net::Ipv4Addr::from(u32::from_be(row.dw_addr));
                        let ip_str = ip_addr.to_string();
                        
                        if !ip_addr.is_loopback() 
                            && !ip_addr.is_unspecified() 
                            && !ip_str.starts_with("169.254.") 
                            && !ip_str.starts_with("0.") 
                        {
                            ips.push(ip_str);
                        }
                    }
                }
            }
            build_response(200, "OK", "application/json", &json!(ips).to_string())
        }

        ("GET", "/api/system/tftp_folders") => {
            let tftp_dir = config.read().dhcp.as_ref().map(|d| d.tftp_dir.clone()).unwrap_or_default();
            let mut folders = Vec::new();
            if !tftp_dir.is_empty() {
                if let Ok(entries) = fs::read_dir(&tftp_dir) {
                    for entry in entries.flatten() {
                        if let Ok(file_type) = entry.file_type() {
                            if file_type.is_dir() {
                                if let Some(name) = entry.file_name().to_str() {
                                    folders.push(name.to_string());
                                }
                            }
                        }
                    }
                }
            }
            build_response(200, "OK", "application/json", &json!(folders).to_string())
        }

        ("POST", "/api/system/tftp_folders/create") => {
            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body) {
                if let Some(name) = json_val["name"].as_str() {
                    let tftp_dir = config.read().dhcp.as_ref().map(|d| d.tftp_dir.clone()).unwrap_or_default();
                    if !tftp_dir.is_empty() {
                        let path = Path::new(&tftp_dir).join(name);
                        if let Err(e) = fs::create_dir_all(&path) {
                            build_response(500, "Error", "application/json", &json!({"error": e.to_string()}).to_string())
                        } else {
                            build_response(200, "OK", "application/json", &json!({"success": true}).to_string())
                        }
                    } else {
                        build_response(400, "Error", "application/json", &json!({"error": "TFTP Dir not configured"}).to_string())
                    }
                } else {
                    build_response(400, "Error", "application/json", &json!({"error": "Missing name"}).to_string())
                }
            } else {
                build_response(400, "Error", "application/json", &json!({"error": "Invalid JSON"}).to_string())
            }
        }

        ("POST", "/api/system/tftp_folders/delete") => {
            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body) {
                if let Some(name) = json_val["name"].as_str() {
                    let tftp_dir = config.read().dhcp.as_ref().map(|d| d.tftp_dir.clone()).unwrap_or_default();
                    if !tftp_dir.is_empty() {
                        let path = Path::new(&tftp_dir).join(name);
                        if path.exists() {
                            if let Err(e) = fs::remove_dir_all(&path) {
                                build_response(500, "Error", "application/json", &json!({"error": e.to_string()}).to_string())
                            } else {
                                build_response(200, "OK", "application/json", &json!({"success": true}).to_string())
                            }
                        } else {
                            build_response(404, "Not Found", "application/json", &json!({"error": "Folder not found"}).to_string())
                        }
                    } else {
                        build_response(400, "Error", "application/json", &json!({"error": "TFTP Dir not configured"}).to_string())
                    }
                } else {
                    build_response(400, "Error", "application/json", &json!({"error": "Missing name"}).to_string())
                }
            } else {
                build_response(400, "Error", "application/json", &json!({"error": "Invalid JSON"}).to_string())
            }
        }

        ("GET", "/api/system/vhds") => {
            let vhd_dir = config.read().windows.as_ref().map(|w| w.vhd_dir.clone()).unwrap_or_default();
            let mut vhds = Vec::new();
            if let Ok(entries) = fs::read_dir(&vhd_dir) {
                for entry in entries.flatten() {
                    if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                        if ext.eq_ignore_ascii_case("vhd") {
                            if let Some(name) = entry.file_name().to_str() {
                                vhds.push(name.to_string());
                            }
                        }
                    }
                }
            }
            build_response(200, "OK", "application/json", &json!(vhds).to_string())
        }

        ("POST", "/api/system/select_vhd") => {
            let selected_path = tokio::task::spawn_blocking(|| {
                rfd::FileDialog::new()
                    .add_filter("VHD Disk Image", &["vhd"])
                    .pick_file()
            }).await.unwrap_or(None);

            let path_str = selected_path.map(|p| p.to_string_lossy().to_string());
            build_response(200, "OK", "application/json", &json!({ "path": path_str }).to_string())
        }

        ("GET", "/api/vhd") => {
            let vhd_dir = config.read().windows.as_ref().map(|w| w.vhd_dir.clone()).unwrap_or_default();
            let mut list = Vec::new();
            if let Ok(entries) = fs::read_dir(&vhd_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map_or(false, |ext| ext.eq_ignore_ascii_case("vhd")) {
                        let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or_default().to_string();
                        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        list.push(json!({
                            "name": filename,
                            "size": size,
                        }));
                    }
                }
            }
            build_response(200, "OK", "application/json", &json!(list).to_string())
        }

        ("GET", "/api/vhd/backups") => {
            let image_key = query_params.get("image_key").cloned().unwrap_or("");
            if image_key.is_empty() {
                return build_response(400, "Bad Request", "text/plain", "Missing image_key param");
            }
            let base_path = writeback_super::resolve_base_path(&config.read(), image_key);
            match vhd_merge::list_backups(&base_path) {
                Ok(backups) => {
                    let list: Vec<serde_json::Value> = backups.into_iter().map(|(idx, path)| {
                        json!({
                            "index": idx,
                            "path": path,
                        })
                    }).collect();
                    build_response(200, "OK", "application/json", &json!(list).to_string())
                }
                Err(e) => build_response(500, "Internal Server Error", "text/plain", &e.to_string()),
            }
        }

        ("POST", "/api/vhd/restore") => {
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(&body);
            match parsed {
                Ok(json_body) => {
                    let image_key = json_body["image_key"].as_str().unwrap_or("");
                    let index = json_body["index"].as_u64();
                    
                    let config_ref = config.read();
                    let base_path = writeback_super::resolve_base_path(&config_ref, image_key);
                    
                    let res = if let Some(idx) = index {
                        vhd_merge::restore_backup_by_index(&base_path, idx as usize)
                    } else {
                        vhd_merge::restore_latest_backup(&base_path)
                    };

                    match res {
                        Ok(backup_path) => {
                            let super_path = writeback_super::get_super_path(&config_ref, image_key);
                            if writeback_super::super_exists(&super_path) {
                                let _ = writeback_super::delete_super(&super_path);
                            }
                            build_response(200, "OK", "application/json", &json!({
                                "status": "success",
                                "backup_path": backup_path,
                            }).to_string())
                        }
                        Err(e) => build_response(500, "Internal Server Error", "text/plain", &e.to_string()),
                    }
                }
                Err(_) => build_response(400, "Bad Request", "text/plain", "Invalid JSON body"),
            }
        }

        ("POST", "/api/vhd/merge") => {
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(&body);
            match parsed {
                Ok(json_body) => {
                    let child = json_body["child"].as_str().unwrap_or("").to_string();
                    let parent = json_body["parent"].as_str().unwrap_or("").to_string();
                    tokio::spawn(async move {
                        let _ = vhd_merge::merge_vhd(child, parent).await;
                    });
                    build_response(200, "OK", "text/plain", "Merge VHD task spawned in background")
                }
                Err(_) => build_response(400, "Bad Request", "text/plain", "Invalid JSON body"),
            }
        }

        ("POST", "/api/superclient/set") => {
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(&body);
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
                    build_response(200, "OK", "text/plain", "Super client IP and action configured")
                }
                Err(_) => build_response(400, "Bad Request", "text/plain", "Invalid JSON body"),
            }
        }

        ("POST", "/api/superclient/commit") => {
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(&body);
            match parsed {
                Ok(json_body) => {
                    let hostname = json_body["hostname"].as_str().unwrap_or("");
                    if let Ok(clients) = crate::config::load_clients("clients.toml") {
                        let client = clients.values().find(|c| c.hostname.as_deref() == Some(hostname) || c.ip == hostname);
                        if let Some(c) = client {
                            let image_key = c.image_manager.as_deref().unwrap_or("");
                            let config_ref = config.read();
                            let base_path = writeback_super::resolve_base_path(&config_ref, image_key);
                            let super_path = writeback_super::get_super_path(&config_ref, image_key);
                            
                            if writeback_super::super_exists(&super_path) {
                                let _ = vhd_merge::backup_before_merge(&base_path, &super_path);
                                let config_path = "config.toml".to_string();
                                tokio::spawn(async move {
                                    if let Ok(_) = vhd_merge::merge_vhd(super_path.clone(), base_path).await {
                                        let _ = writeback_super::delete_super(&super_path);
                                        let _ = clear_super_client_config(&config_path);
                                    }
                                });
                                build_response(200, "OK", "text/plain", "Commit VHD task spawned in background")
                            } else {
                                build_response(404, "Not Found", "text/plain", "Super VHD file not found")
                            }
                        } else {
                            build_response(404, "Not Found", "text/plain", "Client hostname not found")
                        }
                    } else {
                        build_response(500, "Internal Server Error", "text/plain", "Failed to load clients.toml")
                    }
                }
                Err(_) => build_response(400, "Bad Request", "text/plain", "Invalid JSON body"),
            }
        }

        ("POST", "/api/superclient/discard") => {
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(&body);
            match parsed {
                Ok(json_body) => {
                    let hostname = json_body["hostname"].as_str().unwrap_or("");
                    if let Ok(clients) = crate::config::load_clients("clients.toml") {
                        let client = clients.values().find(|c| c.hostname.as_deref() == Some(hostname) || c.ip == hostname);
                        if let Some(c) = client {
                            let image_key = c.image_manager.as_deref().unwrap_or("");
                            let config_ref = config.read();
                            let super_path = writeback_super::get_super_path(&config_ref, image_key);
                            
                            let _ = writeback_super::delete_super(&super_path);
                            let _ = clear_super_client_config("config.toml");
                            
                            build_response(200, "OK", "text/plain", "Super client VHD changes discarded successfully")
                        } else {
                            build_response(404, "Not Found", "text/plain", "Client hostname not found")
                        }
                    } else {
                        build_response(500, "Internal Server Error", "text/plain", "Failed to load clients.toml")
                    }
                }
                Err(_) => build_response(400, "Bad Request", "text/plain", "Invalid JSON body"),
            }
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
            let tftp_dir = config.read().dhcp.as_ref().map(|d| d.tftp_dir.clone()).unwrap_or_default();
            let mut files = Vec::new();
            if let Ok(entries) = fs::read_dir(&tftp_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or_default().to_string();
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    files.push(json!({
                        "name": filename,
                        "size": size,
                        "is_dir": path.is_dir(),
                    }));
                }
            }
            build_response(200, "OK", "application/json", &json!(files).to_string())
        }

        ("GET", "/api/tftp/read") => {
            let filename = query_params.get("file").cloned().unwrap_or("");
            if filename.is_empty() || filename.contains("..") {
                return build_response(400, "Bad Request", "text/plain", "Invalid file name");
            }
            let tftp_dir = config.read().dhcp.as_ref().map(|d| d.tftp_dir.clone()).unwrap_or_default();
            let full_path = Path::new(&tftp_dir).join(filename);
            match fs::read_to_string(&full_path) {
                Ok(content) => build_response(200, "OK", "text/plain", &content),
                Err(e) => build_response(500, "Internal Server Error", "text/plain", &e.to_string()),
            }
        }

        ("POST", "/api/tftp/write") => {
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(&body);
            match parsed {
                Ok(json_body) => {
                    let filename = json_body["file"].as_str().unwrap_or("");
                    let content = json_body["content"].as_str().unwrap_or("");
                    if filename.is_empty() || filename.contains("..") {
                        return build_response(400, "Bad Request", "text/plain", "Invalid file name");
                    }
                    let tftp_dir = config.read().dhcp.as_ref().map(|d| d.tftp_dir.clone()).unwrap_or_default();
                    let full_path = Path::new(&tftp_dir).join(filename);
                    if let Err(e) = fs::write(&full_path, content) {
                        build_response(500, "Internal Server Error", "text/plain", &e.to_string())
                    } else {
                        build_response(200, "OK", "text/plain", "File saved successfully")
                    }
                }
                Err(_) => build_response(400, "Bad Request", "text/plain", "Invalid JSON body"),
            }
        }

        ("GET", "/api/writeback/files") => {
            let writeback_dirs = &config.read().writeback.writeback_dirs;
            let mut list = Vec::new();
            for dir in writeback_dirs {
                if let Ok(entries) = fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or_default().to_string();
                        if filename.ends_with(".bin") || filename.ends_with(".map") {
                            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                            list.push(json!({
                                "name": filename,
                                "size": size,
                                "path": path.to_str().unwrap_or_default(),
                            }));
                        }
                    }
                }
            }
            build_response(200, "OK", "application/json", &json!(list).to_string())
        }

        ("POST", "/api/writeback/clear") => {
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(&body);
            match parsed {
                Ok(json_body) => {
                    let file_path_str = json_body["file_path"].as_str().unwrap_or("");
                    if file_path_str.is_empty() || file_path_str.contains("..") {
                        return build_response(400, "Bad Request", "text/plain", "Invalid file path");
                    }
                    
                    let path = Path::new(file_path_str);
                    let is_safe = config.read().writeback.writeback_dirs.iter().any(|dir| {
                        path.starts_with(dir)
                    });

                    if is_safe && path.exists() {
                        if let Err(e) = fs::remove_file(path) {
                            build_response(500, "Internal Server Error", "text/plain", &e.to_string())
                        } else {
                            build_response(200, "OK", "text/plain", "Writeback cache cleared successfully")
                        }
                    } else {
                        build_response(403, "Forbidden", "text/plain", "Unauthorized cache cleanup path")
                    }
                }
                Err(_) => build_response(400, "Bad Request", "text/plain", "Invalid JSON body"),
            }
        }

        ("GET", "/") | ("GET", "/index.html") => {
            match fs::read_to_string("ui/index.html") {
                Ok(content) => build_response(200, "OK", "text/html", &content),
                Err(_) => build_response(404, "Not Found", "text/plain", "ui/index.html not found"),
            }
        }

        ("GET", "/index.css") => {
            match fs::read_to_string("ui/index.css") {
                Ok(content) => build_response(200, "OK", "text/css", &content),
                Err(_) => build_response(404, "Not Found", "text/plain", "ui/index.css not found"),
            }
        }

        ("GET", "/index.js") => {
            match fs::read_to_string("ui/index.js") {
                Ok(content) => build_response(200, "OK", "application/javascript", &content),
                Err(_) => build_response(404, "Not Found", "text/plain", "ui/index.js not found"),
            }
        }

        _ => build_response(404, "Not Found", "text/plain", "API Endpoint Not Found"),
    }
}

fn build_response(status_code: u16, status_text: &str, content_type: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {} {}\r\n\
         Content-Type: {}\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type\r\n\
         Connection: close\r\n\r\n\
         {}",
        status_code, status_text, content_type, body.len(), body
    )
}
