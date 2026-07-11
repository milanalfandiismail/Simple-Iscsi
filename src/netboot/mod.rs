pub mod dhcp;
pub mod tftp;

use std::sync::Arc;
use tokio::task;
use tracing::{info, error};

use crate::config::Config;
use crate::config_manager::SharedConfig;
use dhcp::DhcpServer;
use tftp::TftpServer;

pub async fn start_netboot(config: SharedConfig) {
    let current_config = config.read();
    let dhcp_cfg = match &current_config.dhcp {
        Some(d) => d,
        None => {
            info!("DHCP Server dinonaktifkan di konfigurasi.");
            return;
        }
    };

    if !dhcp_cfg.enabled {
        info!("DHCP Server dinonaktifkan di konfigurasi.");
        return;
    }

    info!("Inisialisasi modul Netboot...");

    match DhcpServer::new(config.clone()).await {
        Ok(dhcp_server) => {
            task::spawn(async move {
                dhcp_server.run().await;
            });
        }
        Err(e) => {
            error!("Gagal menginisialisasi DHCP Server: {}", e);
        }
    }

    match TftpServer::new(config.clone()).await {
        Ok(tftp_server) => {
            task::spawn(async move {
                tftp_server.run().await;
            });
        }
        Err(e) => {
            error!("Gagal menginisialisasi TFTP Server: {}", e);
        }
    }
}
