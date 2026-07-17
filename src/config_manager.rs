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

pub fn start_config_watcher(shared_config: SharedConfig, config_path: String, clients_path: String) {
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
