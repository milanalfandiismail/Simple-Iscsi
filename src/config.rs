use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::Ipv4Addr;
use std::path::Path;
use tracing::{error, info, warn};

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct Config {
    pub server: ServerConfig,
    pub gamedisk_target: GamediskTargetConfig,
    #[serde(default)]
    pub gamedisk: Vec<GamediskConfig>,
    pub windows: Option<WindowsConfig>,
    pub writeback: WritebackConfig,
    #[serde(alias = "image_manager")]
    pub image_manager: Option<HashMap<String, String>>,
    pub dhcp: Option<DhcpConfig>,
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

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum AddressConfig {
    Single(String),
    Multiple(Vec<String>),
}

impl AddressConfig {
    pub fn as_vec(&self) -> Vec<String> {
        match self {
            AddressConfig::Single(s) => vec![s.clone()],
            AddressConfig::Multiple(v) => v.clone(),
        }
    }
}

impl Default for AddressConfig {
    fn default() -> Self {
        AddressConfig::Single("0.0.0.0".to_string())
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct ServerConfig {
    pub address: AddressConfig,
    pub port: u16,
    #[serde(default)]
    pub read_cache_gb: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            address: AddressConfig::default(),
            port: 3300,
            read_cache_gb: 2,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub struct GamediskTargetConfig {
    pub target_iqn: String,
    pub discovery: bool,
}

impl Default for GamediskTargetConfig {
    fn default() -> Self {
        Self {
            target_iqn: "iqn.2024-01.com.tmdebug:gamedisks".to_string(),
            discovery: true,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct GamediskConfig {
    pub physical_disk: String,
    pub block_size: u64,
    pub vendor_id: String,
    pub product_id: String,
    pub product_revision: String,
}

#[derive(Deserialize, Debug, Clone)]
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

#[derive(Deserialize, Debug, Clone)]
pub struct WritebackConfig {
    pub writeback_dirs: Vec<String>,
    pub max_cache_per_client_gb: u64,
    #[serde(default)]
    pub max_write_speed_mbps: u64,
}

impl Default for WritebackConfig {
    fn default() -> Self {
        Self {
            writeback_dirs: vec!["C:\\writeback".to_string()],
            max_cache_per_client_gb: 10,
            max_write_speed_mbps: 20,
        }
    }
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
        // Windows device paths (\\\\.\\PhysicalDriveN) can't be checked via Path::exists
        if !gd.physical_disk.starts_with("\\\\.\\") {
            if !Path::new(&gd.physical_disk).exists() {
                warn!("GameDisk[{}] path {} tidak ditemukan", i, gd.physical_disk);
            }
        }
    }
    if let Some(ref win) = config.windows {
        if !is_power_of_2(win.block_size) {
            error!("Windows block_size {} harus power of 2", win.block_size);
            return Err("block_size invalid".into());
        }
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
    #[serde(rename = "client", default)]
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

    // Validasi duplicate MAC (strict — HashMap key conflict)
    let mut mac_set = HashSet::new();
    for client in &config.clients {
        let mac_lower = client.mac.to_lowercase();
        if !mac_set.insert(mac_lower) {
            return Err(format!("Duplicate MAC address: {} (client: {})",
                client.mac, client.hostname.as_deref().unwrap_or("?")).into());
        }
    }

    // Cek duplicate IP (warning only — valid untuk DHCP beda client)
    let mut ip_set = HashSet::new();
    for client in &config.clients {
        if !ip_set.insert(client.ip.clone()) {
            warn!("Duplicate IP address: {} (client: {}) — allowed jika di subnet berbeda",
                client.ip, client.hostname.as_deref().unwrap_or("?"));
        }
    }

    let map: HashMap<String, ClientConfig> = config
        .clients
        .into_iter()
        .map(|c| (c.mac.clone(), c))
        .collect();
    info!("Loaded {} client(s) from {}", map.len(), path);
    Ok(map)
}

/// Auto-fix duplicate IPs di clients.toml
/// Kalo ada IP yang sama, assign IP baru dari range DHCP
pub fn auto_fix_duplicate_ips(clients_path: &str, start_ip: &str, end_ip: &str) -> Result<(), Box<dyn std::error::Error>> {
    let content = match fs::read_to_string(clients_path) {
        Ok(c) => c,
        Err(_) => return Ok(()), // File gak ada, skip
    };

    let config: ClientsConfig = toml::from_str(&content)?;
    if config.clients.is_empty() {
        return Ok(());
    }

    // Collect semua IP yang dipake
    let mut used_ips = HashSet::new();
    let mut has_duplicate = false;

    for client in &config.clients {
        if !used_ips.insert(client.ip.clone()) {
            has_duplicate = true;
        }
    }

    if !has_duplicate {
        return Ok(()); // Gak ada duplikat, skip
    }

    // Fix duplicate: assign ulang IP
    let start: Ipv4Addr = start_ip.parse()?;
    let end: Ipv4Addr = end_ip.parse()?;
    let start_u32 = u32::from(start);
    let end_u32 = u32::from(end);

    let mut fixed_clients: Vec<ClientConfig> = Vec::new();
    let mut seen_ips = HashSet::new();
    let mut next_ip_u32 = start_u32;

    for mut client in config.clients {
        if seen_ips.contains(&client.ip) {
            // Duplicate! Cari IP baru
            while next_ip_u32 <= end_u32 {
                let candidate = Ipv4Addr::from(next_ip_u32).to_string();
                if !seen_ips.contains(&candidate) {
                    let old_ip = client.ip.clone();
                    client.ip = candidate.clone();
                    seen_ips.insert(candidate);
                    warn!("⚠️ Duplicate IP: {} ({}). Auto-assigned → {}", old_ip, client.hostname.as_deref().unwrap_or("?"), client.ip);
                    break;
                }
                next_ip_u32 += 1;
            }
        } else {
            seen_ips.insert(client.ip.clone());
        }
        fixed_clients.push(client);
    }

    // Tulis ulang clients.toml
    let mut buf = String::new();
    for client in &fixed_clients {
        buf.push_str("[[client]]\n");
        buf.push_str(&format!("  hostname        = \"{}\"\n", client.hostname.as_deref().unwrap_or("")));
        buf.push_str(&format!("  mac             = \"{}\"\n", client.mac));
        buf.push_str(&format!("  ip              = \"{}\"\n", client.ip));
        buf.push_str(&format!("  gateway         = \"{}\"\n", client.gateway.as_deref().unwrap_or("")));
        buf.push_str(&format!("  dns             = \"{}\"\n", client.dns.as_deref().unwrap_or("")));
        buf.push_str(&format!("  pxe             = \"{}\"\n", client.pxe.as_deref().unwrap_or("")));
        buf.push_str(&format!("  bootfile_uefi   = \"{}\"\n", client.bootfile_uefi.as_deref().unwrap_or("")));
        buf.push_str(&format!("  bootfile_legacy = \"{}\"\n", client.bootfile_legacy.as_deref().unwrap_or("")));
        buf.push_str(&format!("  bootfile_ipxe   = \"{}\"\n", client.bootfile_ipxe.as_deref().unwrap_or("")));
        buf.push_str(&format!("  next_server     = \"{}\"\n", client.next_server.as_deref().unwrap_or("")));
        buf.push_str(&format!("  image_manager   = \"{}\"\n\n", client.image_manager.as_deref().unwrap_or("")));
    }

    fs::write(clients_path, buf)?;
    info!("✅ Duplicate IPs auto-fixed. File clients.toml diperbarui.");
    Ok(())
}

pub fn append_client(path: &str, client: &ClientConfig) -> Result<(), Box<dyn std::error::Error>> {
    // Load existing dulu untuk validasi duplicate
    let existing = load_clients(path)?;

    // Validasi duplicate MAC (strict)
    if existing.contains_key(&client.mac) {
        return Err(format!("MAC already exists: {} (used by {})",
            client.mac, existing[&client.mac].hostname.as_deref().unwrap_or("?")).into());
    }

    // Cek duplicate IP (warning only)
    for (_, existing_client) in &existing {
        if existing_client.ip == client.ip {
            warn!("IP {} already used by {} — allowed jika di subnet berbeda",
                client.ip, existing_client.hostname.as_deref().unwrap_or("?"));
        }
    }

    // Append ke file
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;

    let mut buf = String::new();
    buf.push_str("\n[[client]]\n");
    buf.push_str(&format!("  hostname        = \"{}\"\n", client.hostname.as_deref().unwrap_or("")));
    buf.push_str(&format!("  mac             = \"{}\"\n", client.mac));
    buf.push_str(&format!("  ip              = \"{}\"\n", client.ip));
    buf.push_str(&format!("  gateway         = \"{}\"\n", client.gateway.as_deref().unwrap_or("")));
    buf.push_str(&format!("  dns             = \"{}\"\n", client.dns.as_deref().unwrap_or("")));
    buf.push_str(&format!("  pxe             = \"{}\"\n", client.pxe.as_deref().unwrap_or("")));
    buf.push_str(&format!("  bootfile_uefi   = \"{}\"\n", client.bootfile_uefi.as_deref().unwrap_or("")));
    buf.push_str(&format!("  bootfile_legacy = \"{}\"\n", client.bootfile_legacy.as_deref().unwrap_or("")));
    buf.push_str(&format!("  bootfile_ipxe   = \"{}\"\n", client.bootfile_ipxe.as_deref().unwrap_or("")));
    buf.push_str(&format!("  next_server     = \"{}\"\n", client.next_server.as_deref().unwrap_or("")));
    buf.push_str(&format!("  image_manager   = \"{}\"\n", client.image_manager.as_deref().unwrap_or("")));

    file.write_all(buf.as_bytes())?;
    info!("Client '{}' ({}) appended to {}", 
        client.hostname.as_deref().unwrap_or("?"), client.mac, path);
    Ok(())
}
