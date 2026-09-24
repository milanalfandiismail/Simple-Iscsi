pub mod dhcp;
pub mod dhcp_packet;
pub mod tftp;

use tokio::task;
use tracing::{info, error};
use std::sync::Arc;
use std::time::Duration;

use crate::config_manager::SharedConfig;
use crate::config::{DhcpConfig, AddressConfig};
use dhcp::DhcpServer;
use tftp::TftpServer;

pub async fn start_netboot(config: SharedConfig, stats: Arc<crate::stats::ServerStats>) {
    tokio::spawn(async move {
        let mut dhcp_task: Option<task::JoinHandle<()>> = None;
        let mut tftp_task: Option<task::JoinHandle<()>> = None;
        let mut currently_enabled = false;
        let mut last_dhcp_config: Option<DhcpConfig> = None;
        let mut last_server_address: Option<AddressConfig> = None;

        let mut interval = tokio::time::interval(Duration::from_millis(500));
        loop {
            interval.tick().await;

            let current_config = config.read();
            let should_be_enabled = current_config.dhcp.as_ref().map(|d| d.enabled).unwrap_or(false);
            let current_dhcp_config = current_config.dhcp.clone();
            let current_server_address = Some(current_config.server.address.clone());

            let config_changed = should_be_enabled && (
                current_dhcp_config != last_dhcp_config ||
                current_server_address != last_server_address
            );

            if (should_be_enabled != currently_enabled) || (currently_enabled && config_changed) {
                if should_be_enabled {
                    if currently_enabled {
                        info!("Pembaruan konfigurasi DHCP/TFTP terdeteksi. Me-restart layanan netboot...");
                        if let Some(h) = dhcp_task.take() {
                            h.abort();
                            let _ = h.await;
                        }
                        if let Some(h) = tftp_task.take() {
                            h.abort();
                            let _ = h.await;
                        }
                        // Jeda kecil untuk memastikan OS melepaskan port socket secara tuntas
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    } else {
                        info!("DHCP/TFTP Server diaktifkan secara dinamis. Memulai layanan...");
                    }

                    match DhcpServer::new(config.clone(), stats.clone()).await {
                        Ok(dhcp_server) => {
                            let h = task::spawn(async move {
                                dhcp_server.run().await;
                            });
                            dhcp_task = Some(h);
                        }
                        Err(e) => {
                            error!("Gagal menginisialisasi DHCP Server: {}", e);
                        }
                    }

                    match TftpServer::new(config.clone()).await {
                        Ok(tftp_server) => {
                            let h = task::spawn(async move {
                                tftp_server.run().await;
                            });
                            tftp_task = Some(h);
                        }
                        Err(e) => {
                            error!("Gagal menginisialisasi TFTP Server: {}", e);
                        }
                    }

                    currently_enabled = true;
                    last_dhcp_config = current_dhcp_config;
                    last_server_address = current_server_address;
                    info!("✅ Layanan DHCP & TFTP Server aktif dengan konfigurasi terbaru!");
                } else {
                    info!("DHCP/TFTP Server dinonaktifkan secara dinamis. Menghentikan layanan...");
                    if let Some(h) = dhcp_task.take() {
                        h.abort();
                        let _ = h.await;
                    }
                    if let Some(h) = tftp_task.take() {
                        h.abort();
                        let _ = h.await;
                    }
                    currently_enabled = false;
                    last_dhcp_config = None;
                    last_server_address = None;
                }
            }
        }
    });
}
