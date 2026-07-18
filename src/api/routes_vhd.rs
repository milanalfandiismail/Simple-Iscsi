use std::fs;
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

pub fn post_vhd_restore(config: &SharedConfig, body: &str) -> String {
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(body);
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
