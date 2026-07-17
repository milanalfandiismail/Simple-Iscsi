mod cli;
mod backend;
mod writeback_gamedisk;
mod writeback_imagedisk;
mod writeback_super;
mod pdu;
mod scsi_gamedisk;
mod scsi_imagedisk;
mod server;
mod session;
mod config;
mod vhd;
mod vhd_merge;
mod netboot;
mod stats;
mod config_manager;

use backend::Backend;
use std::fs;
use std::sync::Arc;
use tracing::{info, error, warn};
use std::collections::HashMap;
use config_manager::SharedConfig;
use std::time::SystemTime;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .init();

    info!("Memulai Rust iSCSI Server...");

    let config_path = "config.toml".to_string();
    let clients_path = "clients.toml".to_string();

    // Auto-create config.toml if missing
    if !std::path::Path::new(&config_path).exists() {
        info!("config.toml tidak ditemukan. Membuat template default...");
        let default_config = r#"[server]
  address        = "0.0.0.0"
  port           = 3300
  read_cache_gb  = 2

[dhcp]
  enabled        = false
  start_ip       = "192.168.137.100"
  end_ip         = "192.168.137.200"
  subnet_mask    = "255.255.255.0"
  router         = "192.168.137.1"
  dns            = "8.8.8.8"
  next_server    = "192.168.137.1"
  tftp_dir       = "pxe"
  pxe_default    = "sb-custom"

[gamedisk_target]
  target_iqn     = "iqn.2024-01.com.tmdebug:gamedisks"
  discovery      = true

# [[gamedisk]]
#   physical_disk    = '\\.\PhysicalDrive1'
#   block_size       = 512
#   vendor_id        = "TM"
#   product_id       = "GameDisk-1"
#   product_revision = "1.00"

[windows]
  target_iqn_prefix   = "iqn.2024-01.com.tmdebug:vhd-"
  vhd_dir             = 'C:\vhd'
  block_size          = 512
  vendor_id           = "RUSTISCS"
  product_id          = "WindowsBoot"
  product_revision    = "1.00"
  discovery           = false
  super_client_ip     = ""
  super_client_action = "none"

[writeback]
  writeback_dirs          = ['C:\writeback']
  max_cache_per_client_gb = 10
  max_write_speed_mbps    = 20

[image_manager]
  # windows_11 = 'C:\vhd\windows_11.vhd'
"#;
        if let Err(e) = std::fs::write(&config_path, default_config) {
            error!("Gagal membuat {}: {}", config_path, e);
        }
    }

    // Auto-create clients.toml if missing
    if !std::path::Path::new(&clients_path).exists() {
        info!("{} tidak ditemukan. Membuat template default...", clients_path);
        let default_clients = r#"# clients.toml - DHCP Clients configuration
# Make sure to indent client properties with 2 spaces for a clean structure.

# Example client entry:
# [[client]]
#   hostname        = "PC-01"
#   mac             = "00:0C:29:A4:BC:F2"
#   ip              = "192.168.137.100"
#   gateway         = "192.168.137.1"
#   dns             = "8.8.8.8"
#   pxe             = "sb-custom"
#   next_server     = "192.168.137.1"
#   image_manager   = "windows_11"
"#;
        if let Err(e) = std::fs::write(&clients_path, default_clients) {
            error!("Gagal membuat {}: {}", clients_path, e);
        }
    }

    let args: Vec<String> = std::env::args().collect();
    
    // Delegasi ke CLI Handler
    if cli::handle_cli_args(&args, &config_path, &clients_path).await? {
        return Ok(());
    }

    // === Normal Server Start ===
    let config = config::load_config(&config_path)?;
    let shared_config = SharedConfig::new(config.clone());

    info!(
        "Server dikonfigurasi untuk listen di {}:{}",
        config.server.address.as_vec().join(", "),  config.server.port
    );

    // Auto-fix duplicate IPs di clients.toml sebelum start
    if let Some(ref dhcp_cfg) = config.dhcp {
        let dhcp_end = dhcp_cfg.end_ip.clone().unwrap_or_else(|| {
            // Auto-calculate end dari start_ip + 100
            let start_parts: Vec<&str> = dhcp_cfg.start_ip.split('.').collect();
            format!("{}.{}.{}.{}", start_parts[0], start_parts[1], start_parts[2], 200)
        });
        let _ = config::auto_fix_duplicate_ips(&clients_path, &dhcp_cfg.start_ip, &dhcp_end);
    }

    // Load konfigurasi klien DHCP (dengan validasi duplicate)
    let clients = config::load_clients(&clients_path)?;
    info!("Memuat {} konfigurasi klien DHCP.", clients.len());

    // Inisialisasi file watcher via config_manager
    config_manager::start_config_watcher(shared_config.clone(), config_path.clone(), clients_path.clone());

    // Inisialisasi Netboot
    {
        let clients_config = shared_config.clone();
        tokio::spawn(async move {
            crate::netboot::start_netboot(clients_config).await;
        });
    }

    // Inisialisasi Gamedisk backends
    let mut gamedisk_backends: HashMap<u8, Arc<Backend>> = HashMap::new();
    for (i, gd_cfg) in config.gamedisk.iter().enumerate() {
        let lun_id = i as u8;
        info!("Membuka storage backend raw: {}", gd_cfg.physical_disk);

        match Backend::new_raw(
            &gd_cfg.physical_disk,
            gd_cfg.block_size,
            &gd_cfg.vendor_id,
            &gd_cfg.product_id,
            &gd_cfg.product_revision,
            config.server.read_cache_gb,
        ) {
            Ok(b) => {
                gamedisk_backends.insert(lun_id, Arc::new(b));
                info!("Berhasil memuat Gamedisk LUN {}: {}", lun_id, gd_cfg.physical_disk);
            }
            Err(e) => {
                error!("Fatal: Gagal menginisialisasi storage backend gamedisk ({}): {}", gd_cfg.physical_disk, e);
                error!("Pastikan path physical_disk ada dan dapat diakses (Hak Administrator diperlukan untuk raw drive).");
                std::process::exit(1);
            }
        }
    }

    // Buat direktori writeback/cache
    for dir in &config.writeback.writeback_dirs {
        if let Err(e) = fs::create_dir_all(dir) {
            error!("Gagal membuat direktori writeback {:?}: {}", dir, e);
            std::process::exit(1);
        }
        info!("Writeback dir siap di: {}", dir);
    }

    // Mulai server TCP iSCSI
    let stats = stats::ServerStats::new();
    stats::ServerStats::start_periodic_logging(stats.clone());
    
    if let Err(e) = server::start_server(
        shared_config.clone(),
        Arc::new(gamedisk_backends),
        stats,
    )
    .await
    {
        error!("Server terhenti karena fatal error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
