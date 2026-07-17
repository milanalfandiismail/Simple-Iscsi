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
use tracing::{info, error};
use std::collections::HashMap;
use config_manager::SharedConfig;
use std::time::SystemTime;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("Memulai Rust iSCSI Server...");

    let config_path = "config.toml".to_string();
    let clients_path = "clients.toml".to_string();

    // Auto-create config.toml if missing
    if !std::path::Path::new(&config_path).exists() {
        info!("{} tidak ditemukan. Membuat template default...", config_path);
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

    // === CLI: --reload (validasi clients.toml tanpa restart) ===
    // HARUS sebelum handler commit/discard biar gak ke-intercept
    if args.len() >= 2 && args[1] == "--reload" {
        info!("Reload: memvalidasi clients.toml...");
        let _clients = config::load_clients(&clients_path)?;
        info!("✅ clients.toml valid! {} client(s) dimuat.", _clients.len());
        return Ok(());
    }

    // === CLI: --restore-list <hostname> ===
    if args.len() >= 3 && args[1] == "--restore-list" {
        let hostname = &args[2];
        let config = Arc::new(config::load_config(&config_path)?);
        let clients = config::load_clients(&clients_path)?;

        let client = clients.values().find(|c| c.hostname.as_deref() == Some(hostname));
        let image_key = match client {
            Some(c) => c.image_manager.as_deref().unwrap_or(""),
            None => {
                error!("Client dengan hostname '{}' tidak ditemukan", hostname);
                std::process::exit(1);
            }
        };

        if image_key.is_empty() {
            error!("Client '{}' tidak memiliki image_manager", hostname);
            std::process::exit(1);
        }

        let base_path = writeback_super::resolve_base_path(&config, image_key);
        let backups = vhd_merge::list_backups(&base_path)?;

        if backups.is_empty() {
            info!("📋 Tidak ada backup untuk image '{}' ({})", image_key, base_path);
        } else {
            info!("📋 Backup untuk {}:", image_key);
            for (idx, path) in &backups {
                info!("  [{}] {}", idx, path);
            }
        }
        return Ok(());
    }

    // === CLI: --restore <hostname> [index] ===
    if args.len() >= 3 && args[1] == "--restore" {
        let hostname = &args[2];
        let restore_idx: Option<usize> = args.get(3).and_then(|s| s.parse().ok());

        let config = Arc::new(config::load_config(&config_path)?);
        let clients = config::load_clients(&clients_path)?;

        let client = clients.values().find(|c| c.hostname.as_deref() == Some(hostname));
        let image_key = match client {
            Some(c) => c.image_manager.as_deref().unwrap_or(""),
            None => {
                error!("Client dengan hostname '{}' tidak ditemukan", hostname);
                std::process::exit(1);
            }
        };

        if image_key.is_empty() {
            error!("Client '{}' tidak memiliki image_manager", hostname);
            std::process::exit(1);
        }

        let base_path = writeback_super::resolve_base_path(&config, image_key);

        let backup_path = if let Some(idx) = restore_idx {
            vhd_merge::restore_backup_by_index(&base_path, idx)?
        } else {
            vhd_merge::restore_latest_backup(&base_path)?
        };

        info!("✅ Restore selesai! Base image '{}' dikembalikan dari backup.", image_key);
        info!("   Backup: {}", backup_path);

        // Hapus super VHD biar sinkron
        let super_path = writeback_super::get_super_path(&config, image_key);
        if writeback_super::super_exists(&super_path) {
            info!("   Super VHD dihapus (biar sinkron): {}", super_path);
            let _ = writeback_super::delete_super(&super_path);
        }

        return Ok(());
    }

    // === CLI Args: --commit <hostname> atau --discard <hostname> ===
    if args.len() >= 3 {
        let action = &args[1];
        let hostname = &args[2];

        // Load config
        let config = Arc::new(config::load_config(&config_path)?);
        let clients = config::load_clients(&clients_path)?;

        // Cari client by hostname
        let client = clients.values().find(|c| {
            c.hostname.as_deref() == Some(hostname)
        });

        let image_key = match client {
            Some(c) => c.image_manager.as_deref().unwrap_or("").to_string(),
            None => {
                error!("Client dengan hostname '{}' tidak ditemukan di clients.toml", hostname);
                std::process::exit(1);
            }
        };

        if image_key.is_empty() {
            error!("Client '{}' tidak memiliki image_manager", hostname);
            std::process::exit(1);
        }

        let base_path = writeback_super::resolve_base_path(&config, &image_key);
        let super_path = writeback_super::get_super_path(&config, &image_key);

        match action.as_str() {
            "--commit" => {
                if !writeback_super::super_exists(&super_path) {
                    error!("Super VHD untuk {} tidak ditemukan: {}", hostname, super_path);
                    std::process::exit(1);
                }

                // 1. Backup base image dulu
                let backup_path = vhd_merge::backup_before_merge(&base_path, &super_path)?;
                info!("📦 Backup created: {}", backup_path);

                // 2. Merge super VHD → base
                info!("Meng-commit super VHD {} → base {}", super_path, base_path);
                vhd_merge::merge_vhd(super_path.clone(), base_path.clone()).await?;

                // 3. Hapus super VHD
                writeback_super::delete_super(&super_path)?;

                info!("✅ Commit selesai! Image '{}' diperbarui.", image_key);
            }
            "--discard" => {
                if !writeback_super::super_exists(&super_path) {
                    error!("Super VHD untuk {} tidak ditemukan: {}", hostname, super_path);
                    std::process::exit(1);
                }
                info!("Membuang super VHD: {}", super_path);
                writeback_super::delete_super(&super_path)?;
                info!("✅ Discard selesai! Perubahan di '{}' dibuang.", image_key);
            }
            _ => {
                error!("Argumen tidak dikenal: {}. Gunakan --commit <hostname> atau --discard <hostname>", action);
                std::process::exit(1);
            }
        }
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

    // Inisialisasi file watcher
    {
        let shared_config_clone = shared_config.clone();
        let config_path_clone = config_path.clone();
        let clients_path_clone = clients_path.clone();
        tokio::spawn(async move {
            let mut last_config_mtime = std::fs::metadata(&config_path_clone).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
            let mut last_clients_mtime = std::fs::metadata(&clients_path_clone).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
            
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                interval.tick().await;
                
                let current_config_mtime = std::fs::metadata(&config_path_clone).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
                let current_clients_mtime = std::fs::metadata(&clients_path_clone).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
                
                let config_changed = current_config_mtime != last_config_mtime;
                let clients_changed = current_clients_mtime != last_clients_mtime;
                
                if config_changed || clients_changed {
                    info!("Mendeteksi perubahan pada file konfigurasi...");
                    if let Some(ref dhcp_cfg) = shared_config_clone.read().dhcp {
                        let dhcp_end = dhcp_cfg.end_ip.clone().unwrap_or_else(|| {
                            let start_parts: Vec<&str> = dhcp_cfg.start_ip.split('.').collect();
                            format!("{}.{}.{}.{}", start_parts[0], start_parts[1], start_parts[2], 200)
                        });
                        let _ = config::auto_fix_duplicate_ips(&clients_path_clone, &dhcp_cfg.start_ip, &dhcp_end);
                    }

                    match config::load_config(&config_path_clone) {
                        Ok(new_config) => {
                            shared_config_clone.update(new_config);
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
