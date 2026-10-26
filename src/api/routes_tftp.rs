use std::fs;
use std::path::{Path, PathBuf};
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

fn tftp_root(config: &SharedConfig) -> Option<PathBuf> {
    let configured = config.read().dhcp.as_ref().map(|d| d.tftp_dir.clone()).unwrap_or_default();
    if configured.trim().is_empty() {
        return None;
    }
    let path = Path::new(&configured);
    let absolute = path
        .canonicalize()
        .or_else(|_| std::env::current_dir().map(|cwd| cwd.join(path)))
        .ok()?;
    if absolute.exists() {
        Some(absolute)
    } else {
        None
    }
}

pub fn get_tftp_root(config: &SharedConfig) -> String {
    let configured = config.read().dhcp.as_ref().map(|d| d.tftp_dir.clone()).unwrap_or_default();
    let absolute = if configured.trim().is_empty() {
        String::new()
    } else {
        let path = Path::new(&configured);
        path.canonicalize()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| {
                if path.is_absolute() {
                    configured.clone()
                } else {
                    std::env::current_dir()
                        .map(|cwd| cwd.join(path).to_string_lossy().to_string())
                        .unwrap_or_else(|_| configured.clone())
                }
            })
    };
    let exists = !absolute.is_empty() && Path::new(&absolute).exists();
    let display = absolute
        .strip_prefix(r"\\?\")
        .map(|s| s.to_string())
        .unwrap_or_else(|| absolute.clone());
    build_response(
        200,
        "OK",
        "application/json",
        &json!({ "configured": configured, "absolute": display, "exists": exists }).to_string(),
    )
}

// Shell dialog menolak path verbatim (\\?\...), jadi kirim path biasa.
// shortcut: path >260 karakter bisa gagal; upgrade kalau TFTP root dipindah ke path sangat panjang.
fn dialog_path(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{}", rest))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        p.to_path_buf()
    }
}

pub fn post_tftp_pick_folder(config: &SharedConfig) -> String {
    let Some(root) = tftp_root(config) else {
        return build_response(400, "Bad Request", "application/json", r#"{"error":"TFTP root tidak ditemukan"}"#);
    };
    let Some(picked) = crate::native_picker::pick_folder(
        "Pilih folder di dalam TFTP root",
        &dialog_path(&root),
    )
    else {
        return build_response(200, "OK", "application/json", r#"{"name":null}"#);
    };
    let Ok(picked) = picked.canonicalize() else {
        return build_response(400, "Bad Request", "application/json", r#"{"error":"Folder tidak valid"}"#);
    };
    let Ok(relative) = picked.strip_prefix(&root) else {
        return build_response(400, "Bad Request", "application/json", r#"{"error":"Folder harus di dalam TFTP root"}"#);
    };
    let name = relative.to_string_lossy().replace('\\', "/");
    if name.is_empty() {
        return build_response(400, "Bad Request", "application/json", r#"{"error":"Pilih subfolder, bukan root"}"#);
    }
    build_response(200, "OK", "application/json", &json!({ "name": name }).to_string())
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
    let Ok(json_val) = serde_json::from_str::<serde_json::Value>(body) else {
        return build_response(400, "Error", "application/json", r#"{"error":"Invalid JSON"}"#);
    };
    let Some(name) = json_val["name"]
        .as_str()
        .or_else(|| json_val["folder_name"].as_str())
    else {
        return build_response(400, "Error", "application/json", r#"{"error":"Missing name"}"#);
    };
    if name.trim().is_empty() || Path::new(name).is_absolute() || name.contains("..") {
        return build_response(400, "Error", "application/json", r#"{"error":"Invalid folder name"}"#);
    }
    let Some(root) = tftp_root(config) else {
        return build_response(400, "Error", "application/json", r#"{"error":"TFTP Dir not configured"}"#);
    };
    let path = root.join(name);
    if let Err(e) = fs::create_dir_all(&path) {
        build_response(500, "Error", "application/json", &json!({"error": e.to_string()}).to_string())
    } else {
        build_response(200, "OK", "application/json", r#"{"success":true}"#)
    }
}

pub fn post_tftp_folders_delete(config: &SharedConfig, body: &str) -> String {
    let Ok(json_val) = serde_json::from_str::<serde_json::Value>(body) else {
        return build_response(400, "Error", "application/json", r#"{"error":"Invalid JSON"}"#);
    };
    let Some(name) = json_val["name"]
        .as_str()
        .or_else(|| json_val["folder_name"].as_str())
    else {
        return build_response(400, "Error", "application/json", r#"{"error":"Missing name"}"#);
    };
    if name.trim().is_empty() || Path::new(name).is_absolute() || name.contains("..") {
        return build_response(400, "Error", "application/json", r#"{"error":"Invalid folder name"}"#);
    }
    let Some(root) = tftp_root(config) else {
        return build_response(400, "Error", "application/json", r#"{"error":"TFTP Dir not configured"}"#);
    };
    let path = root.join(name);
    let Ok(path) = path.canonicalize() else {
        return build_response(404, "Not Found", "application/json", r#"{"error":"Folder not found"}"#);
    };
    if !path.starts_with(&root) || path == root {
        return build_response(400, "Error", "application/json", r#"{"error":"Folder harus di dalam TFTP root"}"#);
    }
    if let Err(e) = fs::remove_dir_all(&path) {
        build_response(500, "Error", "application/json", &json!({"error": e.to_string()}).to_string())
    } else {
        build_response(200, "OK", "application/json", r#"{"success":true}"#)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_path_strips_verbatim_prefix() {
        assert_eq!(dialog_path(Path::new(r"\\?\C:\pxe")), PathBuf::from(r"C:\pxe"));
        assert_eq!(dialog_path(Path::new(r"\\?\UNC\srv\share")), PathBuf::from(r"\\srv\share"));
        assert_eq!(dialog_path(Path::new(r"C:\pxe")), PathBuf::from(r"C:\pxe"));
    }
}
