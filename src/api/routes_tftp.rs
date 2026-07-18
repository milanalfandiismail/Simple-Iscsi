use std::fs;
use std::path::Path;
use serde_json::json;
use crate::config_manager::SharedConfig;
use crate::server_api::build_response;
use std::collections::HashMap;

pub fn get_tftp_files(config: &SharedConfig) -> String {
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

pub fn get_tftp_read(config: &SharedConfig, query_params: &HashMap<&str, &str>) -> String {
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

pub fn post_tftp_write(config: &SharedConfig, body: &str) -> String {
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(body);
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

pub fn get_tftp_folders(config: &SharedConfig) -> String {
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

pub fn post_tftp_folders_create(config: &SharedConfig, body: &str) -> String {
    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(body) {
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

pub fn post_tftp_folders_delete(config: &SharedConfig, body: &str) -> String {
    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(body) {
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
