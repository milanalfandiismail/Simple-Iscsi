mod backend;
mod cache;
mod pdu;
mod scsi;
mod server;
mod session;
mod config;
mod vhd;
mod netboot;

use backend::Backend;
use std::fs;
use std::sync::Arc;
use tracing::{info, error, Level};
use tracing_subscriber::FmtSubscriber;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Inisialisasi logging subscriber console
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Memulai Rust iSCSI Server...");

    // Membaca file konfigurasi config.toml
    let config = match config::load_config("config.toml") {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Gagal meload config: {}", e);
            std::process::exit(1);
        }
    };

    info!("Server dikonfigurasi untuk listen di {}:{}", config.server.address, config.server.port);

    let config = Arc::new(config);

    // Jalankan service Netboot (DHCP, dsb) secara background
    netboot::start_netboot(config.clone()).await;

    // Inisialisasi storage backend untuk seluruh Gamedisk
    let mut gamedisk_backends = HashMap::new();
    for (i, gd_cfg) in config.gamedisk.iter().enumerate() {
        let lun_id = i as u8;
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

    // Buat direktori writeback/cache utama
    if let Err(e) = fs::create_dir_all(&config.cache.cache_dir) {
        error!("Gagal membuat direktori cache {:?}: {}", config.cache.cache_dir, e);
        std::process::exit(1);
    }
    info!("Cache dir siap di: {}", config.cache.cache_dir);

    // Mulai server TCP iSCSI
    if let Err(e) = server::start_server(
        config.clone(),
        Arc::new(gamedisk_backends),
    )
    .await
    {
        error!("Server terhenti karena fatal error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
