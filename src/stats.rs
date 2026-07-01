use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Default)]
pub struct ServerStats {
    pub total_connections: AtomicU64,
    pub active_sessions: AtomicU64,
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
    pub bytes_read: AtomicU64,
    pub bytes_written: AtomicU64,
}

impl ServerStats {
    pub fn new() -> Arc<Self> {
        Arc::new(ServerStats::default())
    }

    pub fn log_summary(&self) {
        tracing::info!(
            "📊 Stats: {} conns, {} active, {} hits, {} misses, {}GB read, {}GB written",
            self.total_connections.load(Ordering::Relaxed),
            self.active_sessions.load(Ordering::Relaxed),
            self.cache_hits.load(Ordering::Relaxed),
            self.cache_misses.load(Ordering::Relaxed),
            self.bytes_read.load(Ordering::Relaxed) / (1024 * 1024 * 1024).max(1),
            self.bytes_written.load(Ordering::Relaxed) / (1024 * 1024 * 1024).max(1),
        );
    }

    /// Spawn a background thread that logs stats every 5 minutes
    pub fn start_periodic_logging(stats: Arc<Self>) {
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(300)); // 5 minutes
                stats.log_summary();
            }
        });
    }
}
