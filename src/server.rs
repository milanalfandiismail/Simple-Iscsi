use tokio::net::TcpListener;
use tracing::{info, error};
use std::sync::Arc;
use crate::backend::Backend;
use crate::session::Session;
use crate::config::Config;
use crate::stats::ServerStats;
use std::collections::HashMap;

pub async fn start_server(
    config: Arc<Config>,
    gamedisk_backends: Arc<HashMap<u8, Arc<Backend>>>,
    stats: Arc<ServerStats>,
) -> Result<(), Box<dyn std::error::Error>> {
    let bind_addr = format!("{}:{}", config.server.address, config.server.port);
    let listener = TcpListener::bind(&bind_addr).await?;
    info!("Server iSCSI berjalan di: iSCSI://{}", bind_addr);

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                error!("Gagal menerima koneksi TCP masuk: {}", e);
                continue;
            }
        };

        stats.total_connections.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        stats.active_sessions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Set TCP nodelay demi meminimalkan latency pengiriman paket data disk game
        if let Err(e) = stream.set_nodelay(true) {
            error!("Gagal mengaktifkan TCP_NODELAY untuk {}: {}", peer, e);
        }

        let gamedisk_backends_clone = Arc::clone(&gamedisk_backends);
        let config_clone = Arc::clone(&config);
        let stats_clone = Arc::clone(&stats);

        tokio::spawn(async move {
            let session = Session::new(
                stream,
                peer.ip(),
                gamedisk_backends_clone,
                config_clone,
                stats_clone,
            );
            
            if let Err(e) = session.run().await {
                error!("Sesi iSCSI client {} terputus dengan error: {}", peer.ip(), e);
            } else {
                info!("Sesi iSCSI client {} ditutup dengan normal.", peer.ip());
            }
        });
    }
}
