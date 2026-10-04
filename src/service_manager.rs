use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};
use serde::{Deserialize, Serialize};

use crate::backend::Backend;
use crate::config_manager::SharedConfig;
use crate::stats::ServerStats;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceState {
    Running,
    Stopped,
    Restarting,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatusReport {
    pub iscsi: ServiceState,
    pub dhcp: ServiceState,
    pub tftp: ServiceState,
}

pub struct ServiceManager {
    config: SharedConfig,
    stats: Arc<ServerStats>,
    backends: Arc<std::sync::RwLock<HashMap<u8, Arc<Backend>>>>,
    
    // Task Handles & Sinyal Kontrol
    dhcp_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    dhcp_shutdown_tx: Mutex<Option<tokio::sync::broadcast::Sender<()>>>,
    
    tftp_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    tftp_shutdown_tx: Mutex<Option<tokio::sync::broadcast::Sender<()>>>,
    
    iscsi_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    iscsi_shutdown_tx: Mutex<Option<tokio::sync::broadcast::Sender<()>>>,
    
    status_lock: Arc<std::sync::RwLock<ServiceStatusReport>>,
}

impl ServiceManager {
    pub fn new(
        config: SharedConfig,
        stats: Arc<ServerStats>,
        backends: Arc<std::sync::RwLock<HashMap<u8, Arc<Backend>>>>,
    ) -> Arc<Self> {
        let initial_dhcp_state = if config.read().dhcp.as_ref().map(|d| d.enabled).unwrap_or(false) {
            ServiceState::Stopped
        } else {
            ServiceState::Stopped
        };

        Arc::new(ServiceManager {
            config,
            stats,
            backends,
            dhcp_task: Mutex::new(None),
            dhcp_shutdown_tx: Mutex::new(None),
            tftp_task: Mutex::new(None),
            tftp_shutdown_tx: Mutex::new(None),
            iscsi_task: Mutex::new(None),
            iscsi_shutdown_tx: Mutex::new(None),
            status_lock: Arc::new(std::sync::RwLock::new(ServiceStatusReport {
                iscsi: ServiceState::Stopped,
                dhcp: initial_dhcp_state,
                tftp: initial_dhcp_state,
            })),
        })
    }

    pub fn get_status(&self) -> ServiceStatusReport {
        self.status_lock.read().unwrap().clone()
    }

    fn set_dhcp_status(&self, state: ServiceState) {
        let mut report = self.status_lock.write().unwrap();
        report.dhcp = state;
    }

    fn set_tftp_status(&self, state: ServiceState) {
        let mut report = self.status_lock.write().unwrap();
        report.tftp = state;
    }

    fn set_iscsi_status(&self, state: ServiceState) {
        let mut report = self.status_lock.write().unwrap();
        report.iscsi = state;
    }

    // ──────────────────────────────────────────────────────────────────────────
    // DHCP Service Management
    // ──────────────────────────────────────────────────────────────────────────

    pub async fn start_dhcp(&self) {
        if self.config.read().dhcp.is_none() {
            error!("❌ Gagal memulai DHCP Server: Konfigurasi [dhcp] tidak ditemukan di config.toml.");
            self.set_dhcp_status(ServiceState::Error);
            return;
        }

        // Aktifkan flag enabled pada runtime config saat user menyalakan DHCP
        self.config.set_dhcp_enabled(true);

        // Hentikan instance DHCP lama jika sedang berjalan
        self.stop_dhcp_internal().await;

        self.set_dhcp_status(ServiceState::Restarting);

        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
        
        match crate::netboot::dhcp::DhcpServer::new(self.config.clone(), self.stats.clone(), shutdown_tx.subscribe()).await {
            Ok(dhcp_server) => {
                let handle = tokio::spawn(async move {
                    dhcp_server.run(shutdown_rx).await;
                });

                let mut task_guard = self.dhcp_task.lock().await;
                let mut tx_guard = self.dhcp_shutdown_tx.lock().await;
                *task_guard = Some(handle);
                *tx_guard = Some(shutdown_tx);
                self.set_dhcp_status(ServiceState::Running);
                info!("✅ DHCP Server berhasil dijalankan di 0.0.0.0:67.");
            }
            Err(e) => {
                error!("❌ Gagal memulai DHCP Server: {}", e);
                self.set_dhcp_status(ServiceState::Error);
            }
        }
    }

    pub async fn stop_dhcp(&self) {
        self.set_dhcp_status(ServiceState::Restarting);

        // Tandai disabled pada runtime config saat user mematikan DHCP
        self.config.set_dhcp_enabled(false);

        self.stop_dhcp_internal().await;

        self.set_dhcp_status(ServiceState::Stopped);
        info!("🛑 DHCP Server telah dihentikan.");
    }

    async fn stop_dhcp_internal(&self) {
        // Kirim sinyal shutdown
        {
            let mut tx_guard = self.dhcp_shutdown_tx.lock().await;
            if let Some(tx) = tx_guard.take() {
                let _ = tx.send(());
            }
        }

        // Abort dan tunggu task keluar
        {
            let mut task_guard = self.dhcp_task.lock().await;
            if let Some(handle) = task_guard.take() {
                handle.abort();
                let _ = handle.await;
            }
        }
    }

    pub async fn restart_dhcp(&self) {
        info!("🔄 Merestart DHCP Server secara instan...");
        self.stop_dhcp_internal().await;
        // Jeda mikro 15ms untuk memastikan OS socket cleanup
        tokio::time::sleep(std::time::Duration::from_millis(15)).await;
        self.start_dhcp().await;
    }

    // ──────────────────────────────────────────────────────────────────────────
    // TFTP Service Management
    // ──────────────────────────────────────────────────────────────────────────

    pub async fn start_tftp(&self) {
        if self.config.read().dhcp.is_none() {
            error!("❌ Gagal memulai TFTP Server: Konfigurasi netboot/TFTP tidak ditemukan di config.toml.");
            self.set_tftp_status(ServiceState::Error);
            return;
        }

        // Hentikan instance TFTP lama jika sedang berjalan
        self.stop_tftp_internal().await;

        self.set_tftp_status(ServiceState::Restarting);

        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);

        match crate::netboot::tftp::TftpServer::new(self.config.clone()).await {
            Ok(tftp_server) => {
                let handle = tokio::spawn(async move {
                    tftp_server.run(shutdown_rx).await;
                });

                let mut task_guard = self.tftp_task.lock().await;
                let mut tx_guard = self.tftp_shutdown_tx.lock().await;
                *task_guard = Some(handle);
                *tx_guard = Some(shutdown_tx);
                self.set_tftp_status(ServiceState::Running);
                info!("✅ TFTP Server berhasil dijalankan di 0.0.0.0:69.");
            }
            Err(e) => {
                error!("❌ Gagal memulai TFTP Server: {}", e);
                self.set_tftp_status(ServiceState::Error);
            }
        }
    }

    pub async fn stop_tftp(&self) {
        self.set_tftp_status(ServiceState::Restarting);
        self.stop_tftp_internal().await;
        self.set_tftp_status(ServiceState::Stopped);
        info!("🛑 TFTP Server telah dihentikan.");
    }

    async fn stop_tftp_internal(&self) {
        {
            let mut tx_guard = self.tftp_shutdown_tx.lock().await;
            if let Some(tx) = tx_guard.take() {
                let _ = tx.send(());
            }
        }

        {
            let mut task_guard = self.tftp_task.lock().await;
            if let Some(handle) = task_guard.take() {
                handle.abort();
                let _ = handle.await;
            }
        }
    }

    pub async fn restart_tftp(&self) {
        info!("🔄 Merestart TFTP Server secara instan...");
        self.stop_tftp_internal().await;
        tokio::time::sleep(std::time::Duration::from_millis(15)).await;
        self.start_tftp().await;
    }

    // ──────────────────────────────────────────────────────────────────────────
    // iSCSI Server Management
    // ──────────────────────────────────────────────────────────────────────────

    pub async fn start_iscsi(&self) {
        self.stop_iscsi_internal().await;

        self.set_iscsi_status(ServiceState::Restarting);

        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);

        let config_clone = self.config.clone();
        let backends_clone = Arc::clone(&self.backends);
        let stats_clone = Arc::clone(&self.stats);

        let handle = tokio::spawn(async move {
            if let Err(e) = crate::server::start_server(
                config_clone,
                backends_clone,
                stats_clone,
                shutdown_rx,
            ).await {
                error!("iSCSI listener terhenti dengan error: {}", e);
            }
        });

        let mut task_guard = self.iscsi_task.lock().await;
        let mut tx_guard = self.iscsi_shutdown_tx.lock().await;
        *task_guard = Some(handle);
        *tx_guard = Some(shutdown_tx);
        self.set_iscsi_status(ServiceState::Running);
        info!("✅ iSCSI Daemon listener aktif.");
    }

    pub async fn stop_iscsi(&self) {
        self.set_iscsi_status(ServiceState::Restarting);
        self.stop_iscsi_internal().await;
        self.set_iscsi_status(ServiceState::Stopped);
        info!("🛑 iSCSI Daemon listener telah dihentikan.");
    }

    async fn stop_iscsi_internal(&self) {
        {
            let mut tx_guard = self.iscsi_shutdown_tx.lock().await;
            if let Some(tx) = tx_guard.take() {
                let _ = tx.send(());
            }
        }

        {
            let mut task_guard = self.iscsi_task.lock().await;
            if let Some(handle) = task_guard.take() {
                handle.abort();
                let _ = handle.await;
            }
        }
    }

    pub async fn restart_iscsi(&self) {
        info!("🔄 Merestart iSCSI Daemon listener...");
        self.stop_iscsi_internal().await;
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        self.start_iscsi().await;
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Global Multi-Service Operations
    // ──────────────────────────────────────────────────────────────────────────

    pub async fn start_configured(&self) {
        info!("🚀 Memulai layanan Simple-Iscsi sesuai konfigurasi awal...");
        self.start_iscsi().await;

        let dhcp_enabled = self.config.read().dhcp.as_ref().map(|d| d.enabled).unwrap_or(false);
        if dhcp_enabled {
            self.start_dhcp().await;
            self.start_tftp().await;
        } else {
            info!("ℹ️ DHCP & TFTP Server dinonaktifkan di config.toml (dapat diaktifkan lewat Tray/Dashboard).");
        }
    }

    pub async fn start_all(&self) {
        info!("🚀 Memulai seluruh layanan Simple-Iscsi (iSCSI, DHCP, TFTP)...");
        self.start_iscsi().await;
        self.start_dhcp().await;
        self.start_tftp().await;
    }

    pub async fn stop_all(&self) {
        info!("🛑 Menghentikan seluruh layanan Simple-Iscsi...");
        self.stop_dhcp().await;
        self.stop_tftp().await;
        self.stop_iscsi().await;
    }

    pub async fn restart_all(&self) {
        info!("🔄 Merestart seluruh layanan Simple-Iscsi (Instant Hot-Reload)...");
        self.restart_dhcp().await;
        self.restart_tftp().await;
        self.restart_iscsi().await;
    }
}
