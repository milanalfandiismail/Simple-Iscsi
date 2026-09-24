use std::sync::Arc;
use serde_json::json;
use crate::config_manager::SharedConfig;
use crate::stats::ServerStats;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

pub fn get_stats_payload(config: &SharedConfig, stats: &Arc<ServerStats>) -> serde_json::Value {
    let client_list: Vec<serde_json::Value> = stats.client_stats.iter().map(|entry| {
        let ip = entry.key();
        let client_s = entry.value();
        let is_active = client_s.active_sessions.load(std::sync::atomic::Ordering::Relaxed) > 0;
        
        let uptime_secs = if is_active {
            client_s.session_start_time.lock()
                .map(|t| t.elapsed().as_secs())
                .unwrap_or(0)
        } else {
            0
        };

        let last_duration_secs = client_s.last_session_duration.lock()
            .map(|d| d.as_secs())
            .unwrap_or(0);

        json!({
            "ip": ip,
            "active": is_active,
            "uptime_secs": uptime_secs,
            "last_duration_secs": last_duration_secs,
            "bytes_read": client_s.bytes_read.load(std::sync::atomic::Ordering::Relaxed),
            "bytes_written": client_s.bytes_written.load(std::sync::atomic::Ordering::Relaxed),
        })
    }).collect();

    let config_guard = config.read();
    let dhcp_enabled = config_guard.dhcp.as_ref().map(|d| d.enabled).unwrap_or(false);
    let tftp_enabled = config_guard.dhcp.as_ref().map(|d| d.enabled && !d.tftp_dir.is_empty()).unwrap_or(false);
    let iscsi_port = config_guard.server.port;

    json!({
        "total_connections": stats.total_connections.load(std::sync::atomic::Ordering::Relaxed),
        "active_sessions": stats.active_sessions.load(std::sync::atomic::Ordering::Relaxed),
        "cache_hits": stats.cache_hits.load(std::sync::atomic::Ordering::Relaxed),
        "cache_misses": stats.cache_misses.load(std::sync::atomic::Ordering::Relaxed),
        "bytes_read": stats.bytes_read.load(std::sync::atomic::Ordering::Relaxed),
        "bytes_written": stats.bytes_written.load(std::sync::atomic::Ordering::Relaxed),
        "clients": client_list,
        "services": {
            "iscsi": { "enabled": true, "port": iscsi_port },
            "dhcp": { "enabled": dhcp_enabled, "port": 67 },
            "tftp": { "enabled": tftp_enabled, "port": 69 }
        }
    })
}

pub async fn handle_sse_stream(mut socket: TcpStream, config: SharedConfig, stats: Arc<ServerStats>) {
    let headers = "HTTP/1.1 200 OK\r\n\
                   Content-Type: text/event-stream\r\n\
                   Cache-Control: no-cache\r\n\
                   Connection: keep-alive\r\n\
                   Access-Control-Allow-Origin: *\r\n\r\n";
    
    if socket.write_all(headers.as_bytes()).await.is_err() {
        return;
    }

    loop {
        let payload = get_stats_payload(&config, &stats);
        let msg = format!("data: {}\n\n", payload.to_string());
        if socket.write_all(msg.as_bytes()).await.is_err() {
            break; // Client disconnected
        }
        if socket.flush().await.is_err() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    }
}
