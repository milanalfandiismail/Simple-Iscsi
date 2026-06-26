mod backend;
mod cache;
mod pdu;
mod scsi;
mod server;
mod session;

use backend::Backend;
use serde::Deserialize;
use std::fs;
use std::sync::Arc;
use tracing::{info, error, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Deserialize)]
struct Config {
    server: ServerConfig,
    storage: StorageConfig,
    cache: CacheConfig,
}

#[derive(Deserialize)]
struct ServerConfig {
    address: String,
    port: u16,
    target_iqn: String,
}

#[derive(Deserialize)]
struct StorageConfig {
    physical_disk: String,
    writeback_dirs: Vec<String>,
    block_size: u64,
}

#[derive(Deserialize)]
struct CacheConfig {
    max_cache_per_client_gb: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Inisialisasi logging subscriber console
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Memulai Rust iSCSI Server...");

    // Membaca file konfigurasi config.toml
    let config_content = match fs::read_to_string("config.toml") {
        Ok(content) => content,
        Err(e) => {
            error!("Gagal membaca config.toml di root directory: {}", e);
            std::process::exit(1);
        }
    };

    let config: Config = match toml::from_str(&config_content) {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Gagal parsing config.toml: {}", e);
            std::process::exit(1);
        }
    };

    info!("Target IQN dikonfigurasi: {}", config.server.target_iqn);

    // Inisialisasi storage backend (HDD game read-only)
    let backend = match Backend::new(&config.storage.physical_disk, config.storage.block_size) {
        Ok(b) => Arc::new(b),
        Err(e) => {
            error!("Fatal: Gagal menginisialisasi storage backend: {}", e);
            error!("Pastikan path physical_disk ada dan dapat diakses (Hak Administrator diperlukan untuk raw drive).");
            std::process::exit(1);
        }
    };

    // Buat semua direktori writeback
    for dir in &config.storage.writeback_dirs {
        if let Err(e) = fs::create_dir_all(dir) {
            error!("Gagal membuat direktori writeback {:?}: {}", dir, e);
            std::process::exit(1);
        }
    }
    info!("Writeback dirs: {} ({:?})",
        config.storage.writeback_dirs.len(),
        config.storage.writeback_dirs);

    // Mulai server TCP iSCSI
    if let Err(e) = server::start_server(
        &config.server.address,
        config.server.port,
        backend,
        config.storage.writeback_dirs,
        config.cache.max_cache_per_client_gb,
        config.server.target_iqn.clone(),
    )
    .await
    {
        error!("Server terhenti karena fatal error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
