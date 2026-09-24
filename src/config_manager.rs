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
}

pub fn clear_super_client_config(config_path: &str) -> std::io::Result<()> {
    use std::io::{BufRead, Write};
    let file = std::fs::File::open(config_path)?;
    let reader = std::io::BufReader::new(file);
    let mut new_lines = Vec::new();
    
    for line in reader.lines() {
        let line = line?;
        if line.trim().starts_with("super_client_ip") {
            new_lines.push("super_client_ip = \"\"".to_string());
        } else if line.trim().starts_with("super_client_action") {
            new_lines.push("super_client_action = \"\"".to_string());
        } else {
            new_lines.push(line);
        }
    }
    
    let mut out = std::fs::File::create(config_path)?;
    for line in new_lines {
        writeln!(out, "{}", line)?;
    }
    Ok(())
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
                            
                            // Cek apakah konfigurasi disk ini persis sama dengan yang lama
                            for (old_i, old_gd_cfg) in old_config.gamedisk.iter().enumerate() {
                                if old_i as u8 == lun_id && old_gd_cfg.physical_disk == gd_cfg.physical_disk {
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
