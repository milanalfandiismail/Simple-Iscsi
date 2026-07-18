pub mod dhcp;
pub mod dhcp_packet;
pub mod tftp;

use tokio::task;
use tracing::{info, error};
use std::sync::Arc;

use crate::config_manager::SharedConfig;
use dhcp::DhcpServer;
use tftp::TftpServer;

pub async fn start_netboot(config: SharedConfig, stats: Arc<crate::stats::ServerStats>) {
    tokio::spawn(async move {
        let mut dhcp_task: Option<task::JoinHandle<()>> = None;
        let mut tftp_task: Option<task::JoinHandle<()>> = None;
        let mut currently_enabled = false;

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3));
        loop {
            interval.tick().await;

            let current_config = config.read();
            let should_be_enabled = current_config.dhcp.as_ref().map(|d| d.enabled).unwrap_or(false);

            if should_be_enabled != currently_enabled {
                if should_be_enabled {
                    info!("DHCP/TFTP Server diaktifkan secara dinamis. Memulai layanan...");

                    match DhcpServer::new(config.clone(), stats.clone()).await {
                        Ok(dhcp_server) => {
                            let h = task::spawn(async move {
                                dhcp_server.run().await;
                            });
                            dhcp_task = Some(h);
                        }
                        Err(e) => {
                            error!("Gagal menginisialisasi DHCP Server dinamis: {}", e);
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
                            error!("Gagal menginisialisasi TFTP Server dinamis: {}", e);
                        }
                    }

                    currently_enabled = true;
                } else {
                    info!("DHCP/TFTP Server dinonaktifkan secara dinamis. Menghentikan layanan...");
                    if let Some(h) = dhcp_task.take() {
                        h.abort();
                        let _ = h.await; // Clean up resources
                    }
                    if let Some(h) = tftp_task.take() {
                        h.abort();
                        let _ = h.await; // Clean up resources
                    }
                    currently_enabled = false;
                }
            }
        }
    });
}
