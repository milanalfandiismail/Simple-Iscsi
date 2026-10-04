use std::fs;
use std::path::Path;
use serde_json::json;
use crate::config_manager::SharedConfig;
use crate::server_api::build_response;
use std::collections::HashMap;
use crate::writeback_super;
use crate::vhd_merge;

pub fn get_system_vhds(config: &SharedConfig) -> String {
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

pub fn post_system_select_vhd() -> String {
    let ps_script = r#"
Add-Type -AssemblyName System.Windows.Forms
$form = New-Object System.Windows.Forms.Form
$form.TopMost = $true
$form.ShowInTaskbar = $false
$form.WindowState = 'Minimized'
$dialog = New-Object System.Windows.Forms.OpenFileDialog
$dialog.Filter = "VHD Disk Image|*.vhd;*.vhdx"
$dialog.Title = "Pilih File VHD"
$result = $dialog.ShowDialog($form)
if ($result -eq [System.Windows.Forms.DialogResult]::OK) {
    Write-Output $dialog.FileName
}
    "#;

    let output = std::process::Command::new("powershell")
        .arg("-NoProfile")
        .arg("-Command")
        .arg(ps_script)
        .output();

    let mut path_str: Option<String> = None;
    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !stdout.is_empty() {
            path_str = Some(stdout);
        }
    }

    build_response(200, "OK", "application/json", &json!({ "path": path_str }).to_string())
}

pub fn get_vhd(config: &SharedConfig) -> String {
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

pub fn get_vhd_backups(config: &SharedConfig, query_params: &HashMap<&str, &str>) -> String {
    let image_key = query_params.get("image_key").cloned().unwrap_or("").trim();
    if image_key.is_empty() {
        let resp = json!({ "status": "error", "message": "Missing image_key parameter" });
        return build_response(400, "Bad Request", "application/json", &resp.to_string());
    }
    let base_path = writeback_super::resolve_base_path(&config.read(), image_key);
    if base_path.is_empty() {
        let resp = json!({ "status": "error", "message": format!("Image key '{}' tidak ditemukan", image_key) });
        return build_response(404, "Not Found", "application/json", &resp.to_string());
    }

    match vhd_merge::list_backups(&base_path) {
        Ok(backups) => {
            let path_obj = Path::new(&base_path);
            let dir = path_obj.parent().unwrap_or_else(|| Path::new("."));
            let stem = path_obj.file_stem().and_then(|s| s.to_str()).unwrap_or("backup");

            let list: Vec<serde_json::Value> = backups.into_iter().map(|(idx, meta_path)| {
                let vhd_name = format!("{}_backup{}.vhd", stem, idx);
                let vhd_path = dir.join(&vhd_name);
                
                let mut size_bytes: u64 = 0;
                let mut date_str = String::new();

                if let Ok(meta_file) = fs::metadata(&vhd_path) {
                    size_bytes = meta_file.len();
                    if let Ok(mod_time) = meta_file.modified() {
                        if let Ok(elapsed) = mod_time.elapsed() {
                            let secs = elapsed.as_secs();
                            if secs < 60 {
                                date_str = "Baru saja".to_string();
                            } else if secs < 3600 {
                                date_str = format!("{} menit lalu", secs / 60);
                            } else if secs < 86400 {
                                date_str = format!("{} jam lalu", secs / 3600);
                            } else {
                                date_str = format!("{} hari lalu", secs / 86400);
                            }
                        }
                    }
                } else if let Ok(meta_file) = fs::metadata(&meta_path) {
                    size_bytes = meta_file.len();
                }

                json!({
                    "index": idx,
                    "name": vhd_name,
                    "path": meta_path,
                    "size": size_bytes,
                    "date": date_str,
                })
            }).collect();
            build_response(200, "OK", "application/json", &json!(list).to_string())
        }
        Err(e) => {
            let resp = json!({ "status": "error", "message": e.to_string() });
            build_response(500, "Internal Server Error", "application/json", &resp.to_string())
        }
    }
}

pub fn post_vhd_restore(config: &SharedConfig, body: &str) -> String {
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(body);
    match parsed {
        Ok(json_body) => {
            let image_key = json_body["image_key"].as_str().unwrap_or("").trim();
            let index = json_body["index"].as_u64();
            
            if image_key.is_empty() {
                let resp = json!({
                    "status": "error",
                    "message": "Parameter image_key tidak boleh kosong"
                });
                return build_response(400, "Bad Request", "application/json", &resp.to_string());
            }

            let config_ref = config.read();
            let base_path = writeback_super::resolve_base_path(&config_ref, image_key);
            if base_path.is_empty() || !Path::new(&base_path).exists() {
                let resp = json!({
                    "status": "error",
                    "message": format!("File Base VHD untuk image '{}' tidak ditemukan di disk", image_key)
                });
                return build_response(404, "Not Found", "application/json", &resp.to_string());
            }
            
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
                    let _ = crate::config_manager::clear_super_client_config("config.toml");
                    config.clear_super_client();

                    let idx_str = index.map(|i| format!(" #{}", i)).unwrap_or_default();
                    let resp = json!({
                        "status": "ok",
                        "message": format!("Master VHD '{}' berhasil di-revert ke snapshot{}. File snapshot dan differencing terkait telah dibersihkan.", image_key, idx_str),
                        "backup_path": backup_path,
                    });
                    build_response(200, "OK", "application/json", &resp.to_string())
                }
                Err(e) => {
                    let resp = json!({
                        "status": "error",
                        "message": format!("Gagal me-restore snapshot: {}", e)
                    });
                    build_response(500, "Internal Server Error", "application/json", &resp.to_string())
                }
            }
        }
        Err(_) => {
            let resp = json!({
                "status": "error",
                "message": "Format JSON request tidak valid"
            });
            build_response(400, "Bad Request", "application/json", &resp.to_string())
        }
    }
}

pub fn post_vhd_merge(body: &str) -> String {
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(body);
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

pub fn get_merge_status(config: &SharedConfig) -> String {
    let status = vhd_merge::get_merge_status();
    let super_ip = config.read().windows.as_ref().map(|w| w.super_client_ip.clone()).unwrap_or_default();
    let super_action = config.read().windows.as_ref().map(|w| w.super_client_action.clone()).unwrap_or_default();

    let resp = json!({
        "is_merging": status.is_merging,
        "progress": status.progress,
        "current_block": status.current_block,
        "total_blocks": status.total_blocks,
        "image_key": status.image_key,
        "message": status.message,
        "error": status.error,
        "super_client_ip": super_ip,
        "super_client_action": super_action,
    });
    build_response(200, "OK", "application/json", &resp.to_string())
}
