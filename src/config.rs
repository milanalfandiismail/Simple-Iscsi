use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::Ipv4Addr;
use std::path::Path;
use tracing::{error, info, warn};

pub const DEFAULT_CONFIG_TOML: &str = include_str!("../assets/config.default.toml");

/// Membuat config.toml default hanya jika file belum ada.
pub fn ensure_config_file(path: &str) -> std::io::Result<bool> {
    if Path::new(path).exists() {
        return Ok(false);
    }
    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(path, DEFAULT_CONFIG_TOML)?;
    Ok(true)
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::Mutex;

    static CLIENTS_LOCK: Mutex<()> = Mutex::new(());

    pub(crate) fn with_clients_file<T>(content: &str, f: impl FnOnce() -> T) -> T {
        let _guard = CLIENTS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let original = std::fs::read_to_string("clients.toml").ok();
        std::fs::write("clients.toml", content).expect("gagal menulis fixture clients.toml");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        match original {
            Some(s) => {
                let _ = std::fs::write("clients.toml", s);
            }
            None => {
                let _ = std::fs::remove_file("clients.toml");
            }
        }
        match result {
            Ok(v) => v,
            Err(e) => std::panic::resume_unwind(e),
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
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

fn default_true() -> bool {
    true
}

#[derive(Deserialize, Serialize, Debug, Clone)]
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
    #[serde(default)]
    pub nic_ips: Option<Vec<String>>,
    #[serde(default = "default_true")]
    pub auto_add_client: bool,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
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

#[derive(Deserialize, Serialize, Debug, Clone)]
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

#[derive(Deserialize, Serialize, Debug, Clone)]
#[allow(dead_code)]
pub struct GamediskTargetConfig {
    pub target_iqn: String,
    pub discovery: bool,
    #[serde(default = "default_vendor_id")]
    pub vendor_id: String,
    #[serde(default = "default_gamedisk_product_id")]
    pub product_id: String,
    #[serde(default = "default_product_revision")]
    pub product_revision: String,
}

impl Default for GamediskTargetConfig {
    fn default() -> Self {
        Self {
            target_iqn: "iqn.2024-01.com.tmdebug:gamedisks".to_string(),
            discovery: true,
            vendor_id: default_vendor_id(),
            product_id: default_gamedisk_product_id(),
            product_revision: default_product_revision(),
        }
    }
}

fn default_vendor_id() -> String {
    "RUSTISCS".to_string()
}

fn default_windows_product_id() -> String {
    "WindowsBoot".to_string()
}

fn default_gamedisk_product_id() -> String {
    "GameDisk".to_string()
}

fn default_product_revision() -> String {
    "1.00".to_string()
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GamediskConfig {
    pub physical_disk: String,
    #[serde(default)]
    pub volume: Option<String>,
    pub block_size: u64,
    #[serde(default = "default_vendor_id")]
    pub vendor_id: String,
    #[serde(default = "default_gamedisk_product_id")]
    pub product_id: String,
    #[serde(default = "default_product_revision")]
    pub product_revision: String,
    #[serde(default)]
    pub ram_cache_mb: Option<u64>,
}

/// Path raw yang dipakai GameDisk. Volume menang atas physical_disk.
pub fn gamedisk_path(gd: &GamediskConfig) -> String {
    if let Some(v) = &gd.volume {
        let letter = v.trim().trim_end_matches(|c| c == ':' || c == '\\');
        if !letter.is_empty() {
            return format!("\\\\.\\{}:", letter.to_uppercase());
        }
    }
    gd.physical_disk.clone()
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[allow(dead_code)]
pub struct WindowsConfig {
    pub target_iqn_prefix: String,
    pub vhd_dir: String,
    pub block_size: u64,
    #[serde(default = "default_vendor_id")]
    pub vendor_id: String,
    #[serde(default = "default_windows_product_id")]
    pub product_id: String,
    #[serde(default = "default_product_revision")]
    pub product_revision: String,
    pub discovery: bool,
    #[serde(default)]
    pub super_client_ip: String,
    #[serde(default)]
    pub super_client_action: String,
    #[serde(default)]
    pub ram_cache_mb: Option<u64>,
    #[serde(default)]
    pub client_cache_enabled: bool,
}

#[derive(Serialize, Debug, Clone)]
pub struct WritebackConfig {
    pub writeback_dirs: Vec<String>,
    pub max_writeback_per_client_gb: u64,
    pub max_write_speed_mbps: u64,
    pub ram_cache_mb: Option<u64>,
}

impl<'de> Deserialize<'de> for WritebackConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default)]
            writeback_dirs: Vec<String>,
            #[serde(default)]
            max_writeback_per_client_gb: Option<u64>,
            #[serde(default)]
            max_cache_per_client_gb: Option<u64>,
            #[serde(default)]
            max_write_speed_mbps: u64,
            #[serde(default)]
            ram_cache_mb: Option<u64>,
        }

        let raw = Raw::deserialize(deserializer)?;
        Ok(Self {
            writeback_dirs: raw.writeback_dirs,
            max_writeback_per_client_gb: raw
                .max_writeback_per_client_gb
                .or(raw.max_cache_per_client_gb)
                .unwrap_or(0),
            max_write_speed_mbps: raw.max_write_speed_mbps,
            ram_cache_mb: raw.ram_cache_mb,
        })
    }
}

impl Default for WritebackConfig {
    fn default() -> Self {
        Self {
            writeback_dirs: Vec::new(),
            max_writeback_per_client_gb: 0,
            max_write_speed_mbps: 20,
            ram_cache_mb: None,
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
    if config.server.port != 3260 && config.server.port != 3300 {
        error!("Port server {} tidak didukung; gunakan 3260 atau 3300", config.server.port);
        return Err("port server harus 3260 atau 3300".into());
    }
    let is_power_of_2 = |v: u64| v.count_ones() == 1;
    let mut gamedisk_volumes = HashSet::new();
    for (i, gd) in config.gamedisk.iter().enumerate() {
        if !is_power_of_2(gd.block_size) {
            error!("GameDisk[{}] block_size {} harus power of 2 (512/1024/4096)", i, gd.block_size);
            return Err("block_size invalid".into());
        }
        if let Some(v) = &gd.volume {
            let letter = v.trim().trim_end_matches(|c| c == ':' || c == '\\').to_uppercase();
            if letter.is_empty() {
                error!("GameDisk[{}] volume kosong", i);
                return Err("volume invalid".into());
            }
            if letter == "C" {
                error!("GameDisk[{}] tidak boleh memakai volume C:", i);
                return Err("volume C: dilarang".into());
            }
            if !gamedisk_volumes.insert(letter.clone()) {
                error!("GameDisk[{}] volume {} duplikat", i, letter);
                return Err("volume duplikat".into());
            }
            if !Path::new(&format!("{}:\\", letter)).exists() {
                warn!("GameDisk[{}] volume {}:\\ tidak ditemukan", i, letter);
            }
        } else {
            // Windows device paths (\\\\.\\PhysicalDriveN) can't be checked via Path::exists
            if !gd.physical_disk.starts_with("\\\\.\\") {
                if !Path::new(&gd.physical_disk).exists() {
                    warn!("GameDisk[{}] path {} tidak ditemukan", i, gd.physical_disk);
                }
            }
        }
    }
    if let Some(ref win) = config.windows {
        if !is_power_of_2(win.block_size) {
            error!("Windows block_size {} harus power of 2", win.block_size);
            return Err("block_size invalid".into());
        }
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

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ClientsConfig {
    #[serde(rename = "client", default)]
    pub clients: Vec<ClientConfig>,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BootDriver {
    Native,
    Ccboot,
}

impl Default for BootDriver {
    fn default() -> Self {
        Self::Native
    }
}

pub fn boot_driver_str(driver: &BootDriver) -> &'static str {
    match driver {
        BootDriver::Native => "native",
        BootDriver::Ccboot => "ccboot",
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
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
    #[serde(default)]
    pub driver: BootDriver,
    #[serde(default)]
    pub cache_enabled: bool,
    #[serde(default)]
    pub cache_size_mb: u32,
}

pub fn normalize_cache_size_mb(size: u32) -> Result<u32, String> {
    // shortcut: 8192 MB cap is a safety policy, not a CCacheX limit; raise if needed.
    if size == 0 {
        return Ok(0);
    }
    if !(32..=8192).contains(&size) {
        return Err("cache_size_mb harus 0 atau 32-8192".to_string());
    }
    if size % 16 != 0 {
        return Err("cache_size_mb harus kelipatan 16".to_string());
    }
    Ok(size)
}

pub fn normalize_client_cache(client: &mut ClientConfig) -> Result<(), String> {
    if let Some(h) = client.hostname.as_deref() {
        if h.contains('(') || h.contains(')') {
            return Err("hostname tidak boleh mengandung '(' atau ')'".to_string());
        }
    }
    if client.driver != BootDriver::Ccboot {
        client.cache_enabled = false;
        client.cache_size_mb = 0;
        return Ok(());
    }
    if client.cache_enabled {
        client.cache_size_mb = normalize_cache_size_mb(client.cache_size_mb)?;
    } else {
        client.cache_size_mb = 0;
    }
    Ok(())
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
        buf.push_str(&format!("  image_manager   = \"{}\"\n", client.image_manager.as_deref().unwrap_or("")));
        buf.push_str(&format!("  driver          = \"{}\"\n", boot_driver_str(&client.driver)));
        buf.push_str(&format!("  cache_enabled   = {}\n", client.cache_enabled));
        buf.push_str(&format!("  cache_size_mb   = {}\n\n", client.cache_size_mb));
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
    buf.push_str(&format!("  driver          = \"{}\"\n", boot_driver_str(&client.driver)));
    buf.push_str(&format!("  cache_enabled   = {}\n", client.cache_enabled));
    buf.push_str(&format!("  cache_size_mb   = {}\n", client.cache_size_mb));

    file.write_all(buf.as_bytes())?;
    info!("Client '{}' ({}) appended to {}", 
        client.hostname.as_deref().unwrap_or("?"), client.mac, path);
    Ok(())
}

/// Mengonversi input ramah pengguna seperti "512M", "1G", "2GB", "4G", atau "2048" ke MB murni.
pub fn parse_human_cache_size(val: &str) -> u64 {
    let s = val.trim().to_uppercase();
    if s.ends_with("GB") {
        let num_str = s.trim_end_matches("GB").trim();
        num_str.parse::<u64>().unwrap_or(0) * 1024
    } else if s.ends_with('G') {
        let num_str = s.trim_end_matches('G').trim();
        num_str.parse::<u64>().unwrap_or(0) * 1024
    } else if s.ends_with("MB") {
        let num_str = s.trim_end_matches("MB").trim();
        num_str.parse::<u64>().unwrap_or(0)
    } else if s.ends_with('M') {
        let num_str = s.trim_end_matches('M').trim();
        num_str.parse::<u64>().unwrap_or(0)
    } else {
        s.parse::<u64>().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gamedisk_volume_path() {
        let gd: GamediskConfig = toml::from_str("physical_disk = ''\nblock_size = 512\nvolume = 'G:'").unwrap();
        assert_eq!(gamedisk_path(&gd), "\\\\.\\G:");
    }

    #[test]
    fn test_default_config_template_complete() {
        let cfg: Config = toml::from_str(DEFAULT_CONFIG_TOML).unwrap();
        assert!(cfg.windows.is_some());
        assert!(cfg.dhcp.is_some());
        assert!(cfg.image_manager.is_some());
        assert_eq!(cfg.writeback.max_writeback_per_client_gb, 0);
        assert_eq!(cfg.windows.as_ref().unwrap().ram_cache_mb, Some(0));
        assert_eq!(cfg.writeback.ram_cache_mb, Some(0));
        assert_eq!(cfg.dhcp.as_ref().unwrap().nic_ips, Some(vec![]));
        assert!(cfg.dhcp.as_ref().unwrap().auto_add_client);
        assert_eq!(cfg.gamedisk_target.vendor_id, "RUSTISCS");
        assert_eq!(cfg.gamedisk_target.product_id, "GameDisk");
        assert_eq!(cfg.gamedisk_target.product_revision, "1.00");
        assert!(cfg.gamedisk.is_empty());
        assert!(cfg.writeback.writeback_dirs.is_empty());
    }

    #[test]
    fn test_ensure_config_file_only_creates() {
        let path = std::env::temp_dir().join(format!("simpleiscsi_cfg_{}.toml", uuid::Uuid::new_v4()));
        let p = path.to_str().unwrap();
        assert!(ensure_config_file(p).unwrap());
        assert_eq!(std::fs::read_to_string(p).unwrap(), DEFAULT_CONFIG_TOML);
        std::fs::write(p, "custom = true
").unwrap();
        assert!(!ensure_config_file(p).unwrap());
        assert_eq!(std::fs::read_to_string(p).unwrap(), "custom = true
");
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn test_writeback_duplicate_json_keys_are_tolerated() {
        let cfg: Config = serde_json::from_str(
            r#"{"writeback":{"writeback_dirs":[],"max_writeback_per_client_gb":5,"max_cache_per_client_gb":7,"max_write_speed_mbps":0}}"#,
        )
        .unwrap();
        assert_eq!(cfg.writeback.max_writeback_per_client_gb, 5);
    }

    #[test]
    fn test_writeback_duplicate_keys_are_tolerated() {
        let cfg: Config = toml::from_str(
            "[writeback]\nwriteback_dirs = []\nmax_writeback_per_client_gb = 5\nmax_cache_per_client_gb = 7\n",
        )
        .unwrap();
        assert_eq!(cfg.writeback.max_writeback_per_client_gb, 5);
    }

    #[test]
    fn test_legacy_writeback_limit_alias() {
        let cfg: Config = toml::from_str("[writeback]
writeback_dirs = []
max_cache_per_client_gb = 7
").unwrap();
        assert_eq!(cfg.writeback.max_writeback_per_client_gb, 7);
    }

    #[test]
    fn test_zero_writeback_limit_allowed() {
        let path = std::env::temp_dir().join(format!("simpleiscsi_cfg_zero_{}.toml", uuid::Uuid::new_v4()));
        let p = path.to_str().unwrap();
        std::fs::write(p, "[writeback]
writeback_dirs = []
max_writeback_per_client_gb = 0
").unwrap();
        assert!(load_config(p).is_ok());
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn test_example_config_parses() {
        let txt = std::fs::read_to_string("config.toml.example").unwrap();
        let _: Config = toml::from_str(&txt).unwrap();
    }

    #[test]
    fn test_load_current_config() {
        let res = load_config("config.toml");
        println!("load_config result: {:?}", res);
        assert!(res.is_ok());
    }

    #[test]
    fn test_parse_human_cache_size() {
        assert_eq!(parse_human_cache_size("512M"), 512);
        assert_eq!(parse_human_cache_size("1G"), 1024);
        assert_eq!(parse_human_cache_size("2GB"), 2048);
        assert_eq!(parse_human_cache_size("4G"), 4096);
        assert_eq!(parse_human_cache_size("2048"), 2048);
    }

    fn sample_ccboot_client() -> ClientConfig {
        ClientConfig {
            mac: "00:11:22:33:44:55".to_string(),
            ip: "192.168.1.101".to_string(),
            hostname: Some("PC-01".to_string()),
            gateway: None,
            dns: None,
            pxe: None,
            bootfile_uefi: None,
            bootfile_legacy: None,
            bootfile_ipxe: None,
            next_server: None,
            image_manager: None,
            driver: BootDriver::Ccboot,
            cache_enabled: true,
            cache_size_mb: 2048,
        }
    }

    #[test]
    fn test_client_config_defaults_are_native_cache_off() {
        let cfg: ClientsConfig = toml::from_str(
            "[[client]]\nmac = \"00:11:22:33:44:55\"\nip = \"192.168.1.101\"\n",
        ).unwrap();
        let c = &cfg.clients[0];
        assert_eq!(c.driver, BootDriver::Native);
        assert!(!c.cache_enabled);
        assert_eq!(c.cache_size_mb, 0);
    }

    #[test]
    fn test_normalize_cache_size_zero_auto() {
        assert_eq!(normalize_cache_size_mb(0).unwrap(), 0);
        assert_eq!(normalize_cache_size_mb(32).unwrap(), 32);
        assert_eq!(normalize_cache_size_mb(1008).unwrap(), 1008);
        assert_eq!(normalize_cache_size_mb(8192).unwrap(), 8192);
        assert!(normalize_cache_size_mb(31).is_err());
        assert!(normalize_cache_size_mb(33).is_err());
        assert!(normalize_cache_size_mb(8193).is_err());
    }

    #[test]
    fn test_normalize_native_clears_cache() {
        let mut c = sample_ccboot_client();
        c.driver = BootDriver::Native;
        normalize_client_cache(&mut c).unwrap();
        assert!(!c.cache_enabled);
        assert_eq!(c.cache_size_mb, 0);
    }

    #[test]
    fn test_normalize_ccboot_validates_and_keeps() {
        let mut c = sample_ccboot_client();
        normalize_client_cache(&mut c).unwrap();
        assert_eq!(c.cache_size_mb, 2048);

        c.cache_size_mb = 1000;
        assert!(normalize_client_cache(&mut c).is_err());

        c.cache_enabled = false;
        normalize_client_cache(&mut c).unwrap();
        assert_eq!(c.cache_size_mb, 0);
    }

    #[test]
    fn test_clients_toml_roundtrip_ccachex() {
        let cfg = ClientsConfig { clients: vec![sample_ccboot_client()] };
        let s = toml::to_string(&cfg).unwrap();
        let back: ClientsConfig = toml::from_str(&s).unwrap();
        assert_eq!(back.clients[0].driver, BootDriver::Ccboot);
        assert!(back.clients[0].cache_enabled);
        assert_eq!(back.clients[0].cache_size_mb, 2048);
    }

    #[test]
    fn test_normalize_rejects_hostname_parentheses() {
        let mut c = sample_ccboot_client();
        c.hostname = Some("PC-01(1-2048-0)".to_string());
        assert!(normalize_client_cache(&mut c).is_err());
    }
}
