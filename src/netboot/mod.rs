pub mod dhcp;
pub mod dhcp_packet;
pub mod tftp;

use std::sync::Arc;
use crate::service_manager::ServiceManager;

#[allow(dead_code)]
pub async fn start_netboot(service_manager: Arc<ServiceManager>) {
    service_manager.start_dhcp().await;
    service_manager.start_tftp().await;
}
