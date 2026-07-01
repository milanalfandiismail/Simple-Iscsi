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

use backend::Backend;
use std::fs;
use std::sync::Arc;
use tracing::{info, error};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("Memulai Rust iSCSI Server...");

    let config_path = "config.toml".to_string();

    // === CLI Args: --commit <hostname> atau --discard <hostname> ===
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 {
        let action = &args[1];
        let hostname = &args[2];

        // Load config
        let config = Arc::new(config::load_config(&config_path)?);
        let clients = config::load_clients("clients.toml")?;

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
                info!("Meng-commit super VHD {} → base {}", super_path, base_path);
                vhd_merge::merge_vhd(super_path.clone(), base_path.clone()).await?;
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
    let config = Arc::new(config::load_config(&config_path)?);

    info!(
        "Server dikonfigurasi untuk listen di {}:{}",
        config.server.address, config.server.port
    );

    // Load konfigurasi klien DHCP
    let clients = config::load_clients("clients.toml")?;
    info!("Memuat {} konfigurasi klien DHCP.", clients.len());

    // Inisialisasi Netboot
    {
        let clients_config = config.clone();
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

    // Buat direktori super VHD
    if let Err(e) = fs::create_dir_all(&config.windows.super_vhd_dir) {
        error!("Gagal membuat direktori super VHD {:?}: {}", config.windows.super_vhd_dir, e);
        std::process::exit(1);
    }
    info!("Super VHD dir siap di: {}", config.windows.super_vhd_dir);

    // Mulai server TCP iSCSI
    let stats = stats::ServerStats::new();
    stats::ServerStats::start_periodic_logging(stats.clone());
    
    if let Err(e) = server::start_server(
        config.clone(),
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
