use serde::Deserialize;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use tracing::error;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub server: ServerConfig,
    pub gamedisk_target: GamediskTargetConfig,
    pub gamedisk: Vec<GamediskConfig>,
    pub windows: WindowsConfig,
    pub cache: CacheConfig,
    pub image_manager: Option<HashMap<String, String>>,
    pub dhcp: DhcpConfig,
}

#[derive(Deserialize, Debug)]
pub struct DhcpConfig {
    pub enabled: bool,
    pub start_ip: String,
    pub router: String,
    pub dns: String,
    pub next_server: String,
    pub subnet_mask: String,
    pub tftp_dir: String,
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

#[derive(Deserialize, Debug, Clone)]
pub struct ClientsConfig {
    pub clients: HashMap<String, ClientConfig>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ClientConfig {
    pub mac: String,
    pub ip: String,
    pub hostname: Option<String>,
    pub gateway: Option<String>,
    pub dns: Option<String>,
    pub pxe: Option<String>,
    pub bootfile_uefi: Option<String>,
    pub bootfile_legacy: Option<String>,
    pub bootfile_ipxe: Option<String>,
    pub next_server: Option<String>,
    pub image_manager: Option<String>,
}

pub fn load_clients(path: &str) -> Result<ClientsConfig, Box<dyn std::error::Error>> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Ok(ClientsConfig { clients: HashMap::new() }),
    };
    let config: ClientsConfig = toml::from_str(&content)?;
    Ok(config)
}

pub fn append_client(path: &str, client: &ClientConfig) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    
    let mut toml_str = String::new();
    toml_str.push_str(&format!("\n[clients.\"{}\"]\n", client.mac));
    toml_str.push_str(&format!("mac = \"{}\"\n", client.mac));
    toml_str.push_str(&format!("ip = \"{}\"\n", client.ip));
    
    if let Some(ref hostname) = client.hostname {
        toml_str.push_str(&format!("hostname = \"{}\"\n", hostname));
    }
    if let Some(ref gw) = client.gateway {
        toml_str.push_str(&format!("gateway = \"{}\"\n", gw));
    }
    if let Some(ref dns) = client.dns {
        toml_str.push_str(&format!("dns = \"{}\"\n", dns));
    }
    
    file.write_all(toml_str.as_bytes())?;
    Ok(())
}
