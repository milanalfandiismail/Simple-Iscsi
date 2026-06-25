use crate::backend::Backend;
use crate::session::Session;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{info, error};

/// Memulai listener TCP server iSCSI dan meng-accept koneksi secara asinkron.
pub async fn start_server(
    address: &str,
    port: u16,
    backend: Arc<Backend>,
    cache_dir: String,
    max_cache_gb: u64,
) -> Result<(), std::io::Error> {
    let bind_addr = format!("{}:{}", address, port);
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

        // Set TCP nodelay demi meminimalkan latency pengiriman paket data disk game
        if let Err(e) = stream.set_nodelay(true) {
            error!("Gagal mengaktifkan TCP_NODELAY untuk {}: {}", peer, e);
        }

        let backend_clone = Arc::clone(&backend);
        let cache_dir_clone = cache_dir.clone();
        
        tokio::spawn(async move {
            let session = Session::new(
                stream,
                peer.ip(),
                backend_clone,
                cache_dir_clone,
                max_cache_gb,
            );
            if let Err(e) = session.run().await {
                error!("Error terjadi pada sesi client {}: {}", peer, e);
            }
        });
    }
}
