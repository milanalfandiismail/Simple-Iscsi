use tokio::net::TcpListener;
use tracing::{info, error};
use std::sync::Arc;
use crate::backend::Backend;
use crate::session::Session;
use crate::config_manager::SharedConfig;
use crate::stats::ServerStats;
use std::collections::HashMap;

pub async fn start_server(
    config: SharedConfig,
    gamedisk_backends: Arc<std::sync::RwLock<HashMap<u8, Arc<Backend>>>>,
    stats: Arc<ServerStats>,
) -> Result<(), Box<dyn std::error::Error>> {
    let current_config = config.read();
    let addrs = current_config.server.address.as_vec();
    let port = current_config.server.port;
    
    let mut handles = Vec::new();

    for addr in addrs {
        let bind_addr: std::net::SocketAddr = match format!("{}:{}", addr, port).parse() {
            Ok(sa) => sa,
            Err(e) => {
                error!("Format alamat bind tidak valid: {}", e);
                return Err(Box::new(e));
            }
        };

        let socket = match socket2::Socket::new(
            if bind_addr.is_ipv4() { socket2::Domain::IPV4 } else { socket2::Domain::IPV6 },
            socket2::Type::STREAM,
            Some(socket2::Protocol::TCP),
        ) {
            Ok(s) => s,
            Err(e) => {
                error!("Gagal membuat socket: {}", e);
                return Err(Box::new(e));
            }
        };

        let _ = socket.set_recv_buffer_size(256 * 1024);
        let _ = socket.set_send_buffer_size(256 * 1024);
        let _ = socket.set_reuse_address(true);

        if let Err(e) = socket.bind(&bind_addr.into()) {
            error!("Gagal bind ke {}: {}", bind_addr, e);
            return Err(Box::new(e));
        }

        if let Err(e) = socket.listen(1024) {
            error!("Gagal listen pada socket: {}", e);
            return Err(Box::new(e));
        }

        let std_listener: std::net::TcpListener = socket.into();
        if let Err(e) = std_listener.set_nonblocking(true) {
            error!("Gagal menyetel non-blocking listener: {}", e);
            return Err(Box::new(e));
        }

        let listener = match TcpListener::from_std(std_listener) {
            Ok(l) => l,
            Err(e) => {
                error!("Gagal konversi ke tokio TcpListener: {}", e);
                return Err(Box::new(e));
            }
        };
        info!("Server iSCSI berjalan di: iSCSI://{}", bind_addr);

        let gamedisk_backends_clone = Arc::clone(&gamedisk_backends);
        let config_clone = config.clone();
        let stats_clone = Arc::clone(&stats);

        let handle = tokio::spawn(async move {
            loop {
                let (stream, peer) = match listener.accept().await {
                    Ok(conn) => conn,
                    Err(e) => {
                        error!("Gagal menerima koneksi TCP masuk di {}: {}", bind_addr, e);
                        continue;
                    }
                };

                stats_clone.total_connections.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                stats_clone.active_sessions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                if let Err(e) = stream.set_nodelay(true) {
                    error!("Gagal mengaktifkan TCP_NODELAY untuk {}: {}", peer, e);
                }

                let session_gamedisk = Arc::clone(&gamedisk_backends_clone);
                let session_config = config_clone.clone();
                let session_stats = Arc::clone(&stats_clone);

                tokio::spawn(async move {
                    let session = Session::new(
                        stream,
                        peer.ip(),
                        session_gamedisk,
                        session_config,
                        session_stats,
                    );
                    
                    if let Err(e) = session.run().await {
                        error!("Sesi iSCSI client {} terputus dengan error: {}", peer.ip(), e);
                    } else {
                        info!("Sesi iSCSI client {} ditutup dengan normal.", peer.ip());
                    }
                });
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}
