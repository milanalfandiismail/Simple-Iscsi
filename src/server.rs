use tokio::net::TcpSocket;
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

        let socket = if bind_addr.is_ipv4() {
            TcpSocket::new_v4()?
        } else {
            TcpSocket::new_v6()?
        };

        socket.set_reuseaddr(true)?;

        if let Err(e) = socket.bind(bind_addr) {
            error!("Gagal bind ke {}: {}", bind_addr, e);
            return Err(Box::new(e));
        }

        let listener = match socket.listen(1024) {
            Ok(l) => l,
            Err(e) => {
                error!("Gagal listen pada socket: {}", e);
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

                #[cfg(windows)]
                {
                    use std::os::windows::io::{AsRawSocket, FromRawSocket};
                    let raw_socket = stream.as_raw_socket();
                    unsafe {
                        let socket = socket2::Socket::from_raw_socket(raw_socket);
                        let keepalive = socket2::TcpKeepalive::new()
                            .with_time(std::time::Duration::from_secs(30))
                            .with_interval(std::time::Duration::from_secs(5));
                        let _ = socket.set_tcp_keepalive(&keepalive);
                        std::mem::forget(socket);
                    }
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
