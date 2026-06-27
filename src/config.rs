use serde::Deserialize;
use std::fs;
use tracing::error;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub server: ServerConfig,
    pub gamedisk_target: GamediskTargetConfig,
    pub gamedisk: Vec<GamediskConfig>,
    pub windows: WindowsConfig,
    pub cache: CacheConfig,
}

#[derive(Deserialize, Debug)]
pub struct ServerConfig {
    pub address: String,
    pub port: u16,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
pub struct GamediskTargetConfig {
    pub target_iqn: String,
    pub discovery: bool,
}

#[derive(Deserialize, Debug)]
pub struct GamediskConfig {
    pub physical_disk: String,
    pub block_size: u64,
    pub vendor_id: String,
    pub product_id: String,
    pub product_revision: String,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
pub struct WindowsConfig {
    pub target_iqn_prefix: String,
    pub vhd_dir: String,
    pub block_size: u64,
    pub vendor_id: String,
    pub product_id: String,
    pub product_revision: String,
    pub discovery: bool,
    pub super_client_ip: String,
    pub super_client_action: String,
}

#[derive(Deserialize, Debug)]
pub struct CacheConfig {
    pub cache_dir: String,
    pub max_cache_per_client_gb: u64,
}

pub fn load_config(path: &str) -> Result<Config, Box<dyn std::error::Error>> {
    let config_content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) => {
            error!("Gagal membaca config file {}: {}", path, e);
            return Err(Box::new(e));
        }
    };

    let config: Config = match toml::from_str(&config_content) {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Gagal parsing config file {}: {}", path, e);
            return Err(Box::new(e));
        }
    };

    Ok(config)
}
