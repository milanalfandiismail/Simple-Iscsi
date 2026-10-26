#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cli;
mod backend;
mod writeback_gamedisk;
mod writeback_imagedisk;
mod writeback_super;
mod pdu;
mod scsi_gamedisk;
mod scsi_imagedisk;
mod server;
pub mod fs_utils;
pub mod read_ahead;
mod session;
mod config;
mod vhd;
mod vhd_merge;
mod netboot;
mod stats;
mod config_manager;
mod server_api;
mod api;
mod service_manager;
mod native_picker;
mod tray;

use backend::Backend;
use std::fs;
use std::io::Write;
use std::sync::Arc;
use tracing::{info, error};
use std::collections::HashMap;
use config_manager::SharedConfig;

struct LogState {
    path: std::path::PathBuf,
    file: Option<std::fs::File>,
    bytes: u64,
    max_size: u64,
    max_files: usize,
}

impl LogState {
    fn open(path: std::path::PathBuf) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let bytes = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Self {
            path,
            file: Some(file),
            bytes,
            max_size: 512 * 1024,
            max_files: 3,
        })
    }

    fn rotate(&mut self) -> std::io::Result<()> {
        self.file.take();
        let base = self.path.to_string_lossy().to_string();
        let max = self.max_files.max(2);
        let _ = std::fs::remove_file(format!("{}.{}", base, max - 1));
        for i in (1..max - 1).rev() {
            let from = format!("{}.{}", base, i);
            let to = format!("{}.{}", base, i + 1);
            if std::path::Path::new(&from).exists() {
                let _ = std::fs::rename(&from, &to);
            }
        }
        let _ = std::fs::rename(&base, format!("{}.1", base));
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        self.file = Some(file);
        self.bytes = 0;
        Ok(())
    }

    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        if self.bytes.saturating_add(buf.len() as u64) > self.max_size {
            self.rotate()?;
        }
        if let Some(file) = self.file.as_mut() {
            file.write_all(buf)?;
            self.bytes = self.bytes.saturating_add(buf.len() as u64);
        }
        Ok(())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(file) = self.file.as_mut() {
            file.flush()?;
        }
        Ok(())
    }
}

struct LogWriter {
    state: std::sync::Arc<std::sync::Mutex<LogState>>,
    console: bool,
}

impl std::io::Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.console {
            let _ = std::io::stdout().write_all(buf);
        }
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.write_all(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if self.console {
            let _ = std::io::stdout().flush();
        }
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.flush()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let log_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("server.log")))
        .unwrap_or_else(|| std::path::PathBuf::from("server.log"));
    crate::api::routes_logs::set_log_path(log_path.clone());
    let log_state = std::sync::Arc::new(std::sync::Mutex::new(
        LogState::open(log_path.clone()).expect("Gagal membuat/membuka file log server.log"),
    ));

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn,rust_iscsi_server::service_manager=info,rust_iscsi_server::session=info,rust_iscsi_server::session::login=warn,rust_iscsi_server::session::scsi_handler=warn,rust_iscsi_server::writeback_gamedisk=info,rust_iscsi_server::writeback_imagedisk=info")))
        .with_ansi(false)
        .with_writer({
            let state = log_state.clone();
            move || LogWriter { state: state.clone(), console: cfg!(debug_assertions) }
        })
        .init();

    info!("Memulai Rust iSCSI Server...");

    let config_path = "config.toml".to_string();
    let clients_path = "clients.toml".to_string();

    match config::ensure_config_file(&config_path) {
        Ok(true) => info!("config.toml tidak ditemukan. Template default dibuat."),
        Ok(false) => {}
        Err(e) => {
            error!("Gagal membuat {}: {}", config_path, e);
            return Err(e.into());
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
    // We will initialize it after the gamedisk_backends are created, to pass the reference.

    // Inisialisasi Server Stats
    let stats = stats::ServerStats::new();
    stats::ServerStats::start_periodic_logging(stats.clone());

    // Inisialisasi Gamedisk backends
    let mut gamedisk_backends_map: HashMap<u8, Arc<Backend>> = HashMap::new();
    for (i, gd_cfg) in config.gamedisk.iter().enumerate() {
        let lun_id = i as u8;
        let gd_path = config::gamedisk_path(gd_cfg);
        info!("Membuka storage backend raw: {}", gd_path);

        let ram_cache_mb = gd_cfg.ram_cache_mb.unwrap_or(0);
        match Backend::new_raw_mb(
            &gd_path,
            gd_cfg.block_size,
            &gd_cfg.vendor_id,
            &gd_cfg.product_id,
            &gd_cfg.product_revision,
            ram_cache_mb,
        ) {
            Ok(b) => {
                gamedisk_backends_map.insert(lun_id, Arc::new(b));
                info!("Berhasil memuat Gamedisk LUN {}: {}", lun_id, gd_path);
            }
            Err(e) => {
                error!("Gagal menginisialisasi storage backend gamedisk ({}): {}", gd_path, e);
                error!("Silakan perbaiki konfigurasi gamedisk di halaman Disk Management.");
            }
        }
    }
    let gamedisk_backends = Arc::new(std::sync::RwLock::new(gamedisk_backends_map));

    // Inisialisasi ServiceManager
    let service_manager = service_manager::ServiceManager::new(
        shared_config.clone(),
        stats.clone(),
        Arc::clone(&gamedisk_backends),
    );

    // Inisialisasi API Server (Tauri Frontend & Web Dashboard Connector)
    {
        let config_api = shared_config.clone();
        let stats_api = stats.clone();
        let sm_api = service_manager.clone();
        tokio::spawn(async move {
            crate::server_api::start_api_server(config_api, stats_api, sm_api).await;
        });
    }

    // Inisialisasi file watcher via config_manager dengan gamedisk_backends
    config_manager::start_config_watcher(
        shared_config.clone(),
        Arc::clone(&gamedisk_backends),
        config_path.clone(),
        clients_path.clone()
    );

    // Buat direktori writeback/cache
    for dir in &config.writeback.writeback_dirs {
        if let Err(e) = fs::create_dir_all(dir) {
            error!("Gagal membuat direktori writeback {:?}: {}", dir, e);
            error!("Silakan perbaiki path direktori ini di halaman Pengaturan.");
        } else {
            info!("Writeback dir siap di: {}", dir);
        }
    }

    // Jalankan layanan sesuai status konfigurasi awal
    service_manager.start_configured().await;

    // Inisialisasi System Tray di dedicated OS thread
    {
        let sm_tray = service_manager.clone();
        let rt_handle = tokio::runtime::Handle::current();
        std::thread::spawn(move || {
            if let Err(e) = tray::run_tray(sm_tray, "http://127.0.0.1:8080".to_string(), rt_handle) {
                tracing::error!("Gagal menjalankan System Tray: {}", e);
            }
        });
    }

    // Ctrl+C hanya untuk mode console/debug. Di release tray-only, proses tetap hidup
    // sampai user memilih Keluar dari tray.
    let sm_ctrl = service_manager.clone();
    tokio::spawn(async move {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                info!("Menerima sinyal shutdown (Ctrl+C). Menghentikan seluruh layanan secara graceful...");
                sm_ctrl.stop_all().await;
                std::process::exit(0);
            }
            Err(e) => {
                info!("Ctrl+C tidak tersedia (mode tray): {}", e);
            }
        }
    });
    std::future::pending::<()>().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::LogState;

    #[test]
    fn log_state_rotates_at_max_size() {
        let dir = std::env::temp_dir().join(format!("simpleiscsi_log_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("server.log");

        let mut state = LogState::open(path.clone()).unwrap();
        state.max_size = 32;
        state.max_files = 3;
        state.write_all(&[b'a'; 20]).unwrap();
        state.write_all(&[b'b'; 20]).unwrap();

        assert!(path.exists());
        assert!(std::path::Path::new(&format!("{}.1", path.display())).exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
