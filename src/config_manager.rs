use std::sync::Arc;
use parking_lot::RwLock;
use crate::config::Config;

#[derive(Clone)]
pub struct SharedConfig {
    pub inner: Arc<RwLock<Arc<Config>>>,
}

impl SharedConfig {
    pub fn new(config: Config) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Arc::new(config))),
        }
    }

    pub fn read(&self) -> Arc<Config> {
        self.inner.read().clone()
    }

    pub fn update(&self, new_config: Config) {
        *self.inner.write() = Arc::new(new_config);
    }

    pub fn set_dhcp_enabled(&self, enabled: bool) {
        let mut current_cfg = (*self.read()).clone();
        if let Some(ref mut d) = current_cfg.dhcp {
            d.enabled = enabled;
        }
        self.update(current_cfg);
    }

    pub fn set_super_client(&self, ip: String, action: String) {
        let mut current_cfg = (*self.read()).clone();
        if let Some(ref mut win) = current_cfg.windows {
            win.super_client_ip = ip;
            win.super_client_action = action;
        }
        self.update(current_cfg);
    }

    pub fn clear_super_client(&self) {
        self.set_super_client(String::new(), String::new());
    }
}

pub fn update_super_client_config_file(config_path: &str, ip: &str, action: &str) -> std::io::Result<()> {
    let content = std::fs::read_to_string(config_path)?;
    let mut new_lines = Vec::new();
    let mut ip_found = false;
    let mut action_found = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("super_client_ip") {
            new_lines.push(format!("super_client_ip = \"{}\"", ip));
            ip_found = true;
        } else if trimmed.starts_with("super_client_action") {
            new_lines.push(format!("super_client_action = \"{}\"", action));
            action_found = true;
        } else {
            new_lines.push(line.to_string());
        }
    }

    // Jika super_client_ip atau super_client_action belum ada di config.toml, sisipkan di bawah section [windows]
    if !ip_found || !action_found {
        let mut final_lines = Vec::new();
        for line in new_lines {
            let is_win = line.trim() == "[windows]";
            final_lines.push(line);
            if is_win {
                if !ip_found {
                    final_lines.push(format!("super_client_ip = \"{}\"", ip));
                }
                if !action_found {
                    final_lines.push(format!("super_client_action = \"{}\"", action));
                }
            }
        }
        std::fs::write(config_path, final_lines.join("\r\n"))?;
    } else {
        std::fs::write(config_path, new_lines.join("\r\n"))?;
    }

    Ok(())
}

pub fn clear_super_client_config(config_path: &str) -> std::io::Result<()> {
    update_super_client_config_file(config_path, "", "")
}

pub fn start_config_watcher(
    shared_config: SharedConfig, 
    gamedisk_backends: Arc<std::sync::RwLock<std::collections::HashMap<u8, Arc<crate::backend::Backend>>>>,
    config_path: String, 
    clients_path: String
) {
    use std::time::SystemTime;
    use tracing::{info, error};
    
    tokio::spawn(async move {
        let mut last_config_mtime = std::fs::metadata(&config_path).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
        let mut last_clients_mtime = std::fs::metadata(&clients_path).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
        
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            
            let current_config_mtime = std::fs::metadata(&config_path).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
            let current_clients_mtime = std::fs::metadata(&clients_path).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
            
            let config_changed = current_config_mtime != last_config_mtime;
            let clients_changed = current_clients_mtime != last_clients_mtime;
            
            if config_changed || clients_changed {
                info!("Mendeteksi perubahan pada file konfigurasi...");
                if let Some(ref dhcp_cfg) = shared_config.read().dhcp {
                    let dhcp_end = dhcp_cfg.end_ip.clone().unwrap_or_else(|| {
                        let start_parts: Vec<&str> = dhcp_cfg.start_ip.split('.').collect();
                        format!("{}.{}.{}.{}", start_parts[0], start_parts[1], start_parts[2], 200)
                    });
                    let _ = crate::config::auto_fix_duplicate_ips(&clients_path, &dhcp_cfg.start_ip, &dhcp_end);
                }

                match crate::config::load_config(&config_path) {
                    Ok(new_config) => {
                        let old_config = shared_config.read();
                        let mut backends_map = gamedisk_backends.write().unwrap();
                        let mut new_map = std::collections::HashMap::new();

                        for (i, gd_cfg) in new_config.gamedisk.iter().enumerate() {
                            let lun_id = i as u8;
                            let mut reused = false;
                            
                            // Cek apakah konfigurasi disk ini persis sama dengan yang lama (termasuk vendor/product)
                            for (old_i, old_gd_cfg) in old_config.gamedisk.iter().enumerate() {
                                if old_i as u8 == lun_id 
                                    && old_gd_cfg.physical_disk == gd_cfg.physical_disk
                                    && old_gd_cfg.vendor_id == gd_cfg.vendor_id
                                    && old_gd_cfg.product_id == gd_cfg.product_id
                                    && old_gd_cfg.product_revision == gd_cfg.product_revision
                                {
                                    if let Some(b) = backends_map.get(&lun_id) {
                                        new_map.insert(lun_id, Arc::clone(b));
                                        reused = true;
                                        break;
                                    }
                                }
                            }
                            
                            if !reused {
                                info!("Memuat ulang / menambahkan Gamedisk LUN {}: {}", lun_id, gd_cfg.physical_disk);
                                match crate::backend::Backend::new_raw(
                                    &gd_cfg.physical_disk,
                                    gd_cfg.block_size,
                                    &gd_cfg.vendor_id,
                                    &gd_cfg.product_id,
                                    &gd_cfg.product_revision,
                                    new_config.server.read_cache_gb,
                                ) {
                                    Ok(b) => {
                                        new_map.insert(lun_id, Arc::new(b));
                                        info!("Berhasil memuat Gamedisk LUN {}", lun_id);
                                    }
                                    Err(e) => {
                                        error!("Gagal menginisialisasi storage gamedisk ({}): {}", gd_cfg.physical_disk, e);
                                    }
                                }
                            }
                        }
                        
                        *backends_map = new_map;
                        shared_config.update(new_config);
                        info!("✅ Konfigurasi berhasil di-reload!");
                        last_config_mtime = current_config_mtime;
                        last_clients_mtime = current_clients_mtime;
                    }
                    Err(e) => {
                        error!("❌ Gagal me-reload konfigurasi: {}", e);
                        last_config_mtime = current_config_mtime;
                        last_clients_mtime = current_clients_mtime;
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_super_client_file_and_shared_config_lifecycle() {
        let temp_dir = std::env::temp_dir();
        let test_cfg_path = temp_dir.join("test_super_config.toml");
        let path_str = test_cfg_path.to_str().unwrap();

        // 1. Initial config file without super_client keys
        let initial_content = r#"
[server]
address = "127.0.0.1"
port = 3260
read_cache_gb = 4

[gamedisk_target]
target_iqn = "iqn.test:gamedisk"
discovery = true

[windows]
target_iqn_prefix = "iqn.test:vhd-"
vhd_dir = "C:\\vhd"
block_size = 512
vendor_id = "RUSTISCS"
product_id = "WindowsBoot"
product_revision = "1.00"
discovery = false

[writeback]
writeback_dirs = ["C:\\writeback"]
max_cache_per_client_gb = 10
"#;
        std::fs::write(&test_cfg_path, initial_content).unwrap();

        // 2. Enable Super Client (inserts keys under [windows])
        update_super_client_config_file(path_str, "192.168.180.2", "enable").unwrap();
        let loaded = crate::config::load_config(path_str).unwrap();
        assert_eq!(loaded.windows.as_ref().unwrap().super_client_ip, "192.168.180.2");
        assert_eq!(loaded.windows.as_ref().unwrap().super_client_action, "enable");

        // 3. Test SharedConfig in-memory update
        let shared = SharedConfig::new(loaded);
        assert_eq!(shared.read().windows.as_ref().unwrap().super_client_ip, "192.168.180.2");

        // 4. Disable Super Client (clears IP and action to empty string)
        update_super_client_config_file(path_str, "", "").unwrap();
        let loaded_disabled = crate::config::load_config(path_str).unwrap();
        assert_eq!(loaded_disabled.windows.as_ref().unwrap().super_client_ip, "");
        assert_eq!(loaded_disabled.windows.as_ref().unwrap().super_client_action, "");

        // 5. Test SharedConfig clear_super_client
        shared.clear_super_client();
        assert_eq!(shared.read().windows.as_ref().unwrap().super_client_ip, "");
        assert_eq!(shared.read().windows.as_ref().unwrap().super_client_action, "");

        // Cleanup
        let _ = std::fs::remove_file(test_cfg_path);
    }
}
