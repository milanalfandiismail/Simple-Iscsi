use serde::Deserialize;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use tracing::{error, warn};

#[derive(Deserialize, Debug)]
pub struct Config {
    pub server: ServerConfig,
    pub gamedisk_target: GamediskTargetConfig,
    pub gamedisk: Vec<GamediskConfig>,
    pub windows: WindowsConfig,
    pub writeback: WritebackConfig,
    #[serde(alias = "image_manager")]
    pub image_manager: Option<HashMap<String, String>>,
    pub dhcp: DhcpConfig,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DhcpConfig {
    pub enabled: bool,
    pub start_ip: String,
    #[allow(dead_code)]
    pub end_ip: Option<String>,
    pub router: String,
    pub dns: String,
    pub next_server: String,
    pub subnet_mask: String,
    pub tftp_dir: String,
    pub pxe_default: Option<String>,
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
    pub super_vhd_dir: String,
    pub block_size: u64,
    pub vendor_id: String,
    pub product_id: String,
    pub product_revision: String,
    pub discovery: bool,
    pub super_client_ip: String,
    pub super_client_action: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct WritebackConfig {
    pub writeback_dirs: Vec<String>,
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

    // === Validation ===
    let is_power_of_2 = |v: u64| v.count_ones() == 1;
    for (i, gd) in config.gamedisk.iter().enumerate() {
        if !is_power_of_2(gd.block_size) {
            error!("GameDisk[{}] block_size {} harus power of 2 (512/1024/4096)", i, gd.block_size);
            return Err("block_size invalid".into());
        }
        // Windows device paths (\\.\PhysicalDriveN) can't be checked via Path::exists
        if !gd.physical_disk.starts_with("\\\\.\\") {
            if !Path::new(&gd.physical_disk).exists() {
                warn!("GameDisk[{}] path {} tidak ditemukan", i, gd.physical_disk);
            }
        }
    }
    if !is_power_of_2(config.windows.block_size) {
        error!("Windows block_size {} harus power of 2", config.windows.block_size);
        return Err("block_size invalid".into());
    }
    if config.writeback.max_cache_per_client_gb < 1 {
        error!("max_cache_per_client_gb harus >= 1");
        return Err("max_cache_per_client_gb invalid".into());
    }
    for (i, dir) in config.writeback.writeback_dirs.iter().enumerate() {
        let p = Path::new(dir);
        if p.exists() && !p.is_dir() {
            error!("writeback_dirs[{}] {:?} bukan direktori", i, dir);
            return Err("writeback_dirs invalid".into());
        }
    }

    Ok(config)
}

#[derive(Deserialize, Debug, Clone)]
pub struct ClientsConfig {
    #[serde(rename = "client")]
    pub clients: Vec<ClientConfig>,
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

pub fn load_clients(path: &str) -> Result<HashMap<String, ClientConfig>, Box<dyn std::error::Error>> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Ok(HashMap::new()),
    };
    let config: ClientsConfig = toml::from_str(&content)?;
    let map: HashMap<String, ClientConfig> = config
        .clients
        .into_iter()
        .map(|c| (c.mac.clone(), c))
        .collect();
    Ok(map)
}

pub fn append_client(path: &str, client: &ClientConfig) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;

    let mut buf = String::new();
    buf.push_str("\n[[client]]\n");
    buf.push_str(&format!("hostname = \"{}\"\n", client.hostname.as_deref().unwrap_or("")));
    buf.push_str(&format!("mac = \"{}\"\n", client.mac));
    buf.push_str(&format!("ip = \"{}\"\n", client.ip));
    buf.push_str(&format!("gateway = \"{}\"\n", client.gateway.as_deref().unwrap_or("")));
    buf.push_str(&format!("dns = \"{}\"\n", client.dns.as_deref().unwrap_or("")));
    buf.push_str(&format!("pxe = \"{}\"\n", client.pxe.as_deref().unwrap_or("")));
    buf.push_str(&format!("bootfile_uefi = \"{}\"\n", client.bootfile_uefi.as_deref().unwrap_or("")));
    buf.push_str(&format!("bootfile_legacy = \"{}\"\n", client.bootfile_legacy.as_deref().unwrap_or("")));
    buf.push_str(&format!("bootfile_ipxe = \"{}\"\n", client.bootfile_ipxe.as_deref().unwrap_or("")));
    buf.push_str(&format!("next_server = \"{}\"\n", client.next_server.as_deref().unwrap_or("")));
    buf.push_str(&format!("image_manager = \"{}\"\n", client.image_manager.as_deref().unwrap_or("")));

    file.write_all(buf.as_bytes())?;
    Ok(())
}
