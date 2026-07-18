use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use dashmap::DashMap;
use parking_lot::Mutex;

pub struct ClientIOStats {
    pub bytes_read: AtomicU64,
    pub bytes_written: AtomicU64,
    pub active_sessions: AtomicU64,
    pub last_bytes_read: AtomicU64,
    pub last_bytes_written: AtomicU64,
    pub session_start_time: Mutex<Option<Instant>>,
    pub last_session_duration: Mutex<Option<Duration>>,
}

impl Default for ClientIOStats {
    fn default() -> Self {
        Self {
            bytes_read: AtomicU64::new(0),
            bytes_written: AtomicU64::new(0),
            active_sessions: AtomicU64::new(0),
            last_bytes_read: AtomicU64::new(0),
            last_bytes_written: AtomicU64::new(0),
            session_start_time: Mutex::new(None),
            last_session_duration: Mutex::new(None),
        }
    }
}

pub struct ServerStats {
    pub total_connections: AtomicU64,
    pub active_sessions: AtomicU64,
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
    pub bytes_read: AtomicU64,
    pub bytes_written: AtomicU64,
    pub client_stats: DashMap<String, Arc<ClientIOStats>>,
    pub dhcp_leases: DashMap<String, String>,
}

impl Default for ServerStats {
    fn default() -> Self {
        Self {
            total_connections: AtomicU64::new(0),
            active_sessions: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            bytes_read: AtomicU64::new(0),
            bytes_written: AtomicU64::new(0),
            client_stats: DashMap::new(),
            dhcp_leases: DashMap::new(),
        }
    }
}

impl ServerStats {
    pub fn new() -> Arc<Self> {
        Arc::new(ServerStats::default())
    }

    pub fn record_session_start(&self, ip: &str) {
        let stats = self.client_stats.entry(ip.to_string()).or_default();
        stats.active_sessions.fetch_add(1, Ordering::Relaxed);
        *stats.session_start_time.lock() = Some(Instant::now());
    }

    pub fn record_session_end(&self, ip: &str) {
        if let Some(stats) = self.client_stats.get(ip) {
            let prev = stats.active_sessions.fetch_sub(1, Ordering::Relaxed);
            if prev <= 1 {
                let start = stats.session_start_time.lock().take();
                if let Some(start_time) = start {
                    let duration = start_time.elapsed();
                    *stats.last_session_duration.lock() = Some(duration);
                }
                
                // Reset stats when the client goes fully offline
                stats.bytes_read.store(0, Ordering::Relaxed);
                stats.bytes_written.store(0, Ordering::Relaxed);
                stats.last_bytes_read.store(0, Ordering::Relaxed);
                stats.last_bytes_written.store(0, Ordering::Relaxed);
            }
        }
    }

    pub fn record_read(&self, ip: &str, bytes: u64) {
        self.bytes_read.fetch_add(bytes, Ordering::Relaxed);
        let stats = self.client_stats.entry(ip.to_string()).or_default();
        stats.bytes_read.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn record_write(&self, ip: &str, bytes: u64) {
        self.bytes_written.fetch_add(bytes, Ordering::Relaxed);
        let stats = self.client_stats.entry(ip.to_string()).or_default();
        stats.bytes_written.fetch_add(bytes, Ordering::Relaxed);
    }

    fn format_bytes(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;
        const TB: u64 = GB * 1024;

        if bytes >= TB {
            format!("{:.2} TB", bytes as f64 / TB as f64)
        } else if bytes >= GB {
            format!("{:.2} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.2} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.2} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} B", bytes)
        }
    }

    fn format_duration(duration: Duration) -> String {
        let secs = duration.as_secs();
        if secs < 60 {
            format!("{}s", secs)
        } else if secs < 3600 {
            format!("{}m {}s", secs / 60, secs % 60)
        } else {
            format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
        }
    }

    pub fn log_summary(&self) {
        tracing::info!(
            "📊 Stats: {} conns, {} active, {} hits, {} misses, Read: {}, Written: {}",
            self.total_connections.load(Ordering::Relaxed),
            self.active_sessions.load(Ordering::Relaxed),
            self.cache_hits.load(Ordering::Relaxed),
            self.cache_misses.load(Ordering::Relaxed),
            Self::format_bytes(self.bytes_read.load(Ordering::Relaxed)),
            Self::format_bytes(self.bytes_written.load(Ordering::Relaxed)),
        );

        if !self.client_stats.is_empty() {
            tracing::info!("🖥️ Client Stats Summary:");
            for entry in self.client_stats.iter() {
                let ip = entry.key();
                let stats = entry.value();

                let is_active = stats.active_sessions.load(Ordering::Relaxed) > 0;
                let status_str = if is_active {
                    let uptime = stats.session_start_time.lock()
                        .map(|t| Self::format_duration(t.elapsed()))
                        .unwrap_or_else(|| "-".to_string());
                    format!("🟢 Online (Uptime: {})", uptime)
                } else {
                    let last_duration = stats.last_session_duration.lock()
                        .map(|d| Self::format_duration(d))
                        .unwrap_or_else(|| "-".to_string());
                    format!("🔴 Offline (Last Session: {})", last_duration)
                };

                let current_read = stats.bytes_read.load(Ordering::Relaxed);
                let last_read = stats.last_bytes_read.swap(current_read, Ordering::Relaxed);
                let read_speed = (current_read.saturating_sub(last_read) as f64) / 300.0; 

                let current_write = stats.bytes_written.load(Ordering::Relaxed);
                let last_write = stats.last_bytes_written.swap(current_write, Ordering::Relaxed);
                let write_speed = (current_write.saturating_sub(last_write) as f64) / 300.0;

                let read_speed_formatted = format!("{}/s", Self::format_bytes(read_speed as u64));
                let write_speed_formatted = format!("{}/s", Self::format_bytes(write_speed as u64));

                tracing::info!(
                    "  └─ IP: {} | {} | Read: {} ({}) | Written: {} ({})",
                    ip,
                    status_str,
                    Self::format_bytes(current_read),
                    read_speed_formatted,
                    Self::format_bytes(current_write),
                    write_speed_formatted
                );
            }
        }
    }

    pub fn start_periodic_logging(stats: Arc<Self>) {
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(300));
                stats.log_summary();
            }
        });
    }
}
