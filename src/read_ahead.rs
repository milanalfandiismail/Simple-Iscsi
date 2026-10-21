use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use moka::sync::Cache;
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStatsSnapshot {
    pub ram_cache_mb: u64,
    pub chunk_size_bytes: u64,
    pub total_chunks: u64,
    pub entry_count: u64,
    pub used_cache_mb: u64,
    pub hit_count: u64,
    pub miss_count: u64,
    pub hit_rate_pct: f64,
    pub cached_read_bytes: u64,
    pub direct_read_bytes: u64,
}

pub struct ReadAheadCache {
    pub cache: Cache<u64, Arc<Vec<u8>>>,
    pub chunk_size_bytes: u64,
    pub ram_cache_mb: u64,
    pub max_chunks: u64,
    pub hit_count: Arc<AtomicU64>,
    pub miss_count: Arc<AtomicU64>,
    pub cached_read_bytes: Arc<AtomicU64>,
    pub direct_read_bytes: Arc<AtomicU64>,
}

impl ReadAheadCache {
    /// Inisialisasi cache dengan kapasitas berbasis Megabyte (MB) dan 8 jam retention.
    pub fn new_mb(ram_cache_mb: u64, chunk_size_bytes: u64) -> Option<Self> {
        if ram_cache_mb > 0 {
            let max_chunks = (ram_cache_mb * 1024 * 1024) / chunk_size_bytes;
            info!(
                "Inisialisasi Read-Ahead cache: {} MB ({} chunks of {} bytes, 8-hour retention)",
                ram_cache_mb, max_chunks, chunk_size_bytes
            );
            Some(Self {
                cache: Cache::builder()
                    .max_capacity(max_chunks)
                    .time_to_idle(Duration::from_secs(28800)) // 8 Jam (28,800 detik)
                    .build(),
                chunk_size_bytes,
                ram_cache_mb,
                max_chunks,
                hit_count: Arc::new(AtomicU64::new(0)),
                miss_count: Arc::new(AtomicU64::new(0)),
                cached_read_bytes: Arc::new(AtomicU64::new(0)),
                direct_read_bytes: Arc::new(AtomicU64::new(0)),
            })
        } else {
            None
        }
    }

    /// Kompatibilitas mundur dengan parameter GB lama.
    pub fn new(read_cache_gb: u64, chunk_size_bytes: u64) -> Option<Self> {
        Self::new_mb(read_cache_gb * 1024, chunk_size_bytes)
    }

    pub fn get(&self, chunk_id: u64) -> Option<Arc<Vec<u8>>> {
        if let Some(val) = self.cache.get(&chunk_id) {
            self.hit_count.fetch_add(1, Ordering::Relaxed);
            self.cached_read_bytes.fetch_add(self.chunk_size_bytes, Ordering::Relaxed);
            Some(val)
        } else {
            self.miss_count.fetch_add(1, Ordering::Relaxed);
            self.direct_read_bytes.fetch_add(self.chunk_size_bytes, Ordering::Relaxed);
            None
        }
    }

    pub fn try_get_or_load<F>(&self, chunk_id: u64, init: F) -> Result<Arc<Vec<u8>>, std::io::Error>
    where
        F: FnOnce() -> Result<Arc<Vec<u8>>, std::io::Error>,
    {
        if let Some(data) = self.cache.get(&chunk_id) {
            self.hit_count.fetch_add(1, Ordering::Relaxed);
            self.cached_read_bytes.fetch_add(self.chunk_size_bytes, Ordering::Relaxed);
            return Ok(data);
        }

        self.miss_count.fetch_add(1, Ordering::Relaxed);
        self.direct_read_bytes.fetch_add(self.chunk_size_bytes, Ordering::Relaxed);

        match self.cache.try_get_with(chunk_id, init) {
            Ok(data) => Ok(data),
            Err(e) => Err(std::io::Error::new(e.kind(), e.to_string())),
        }
    }

    pub fn insert(&self, chunk_id: u64, data: Arc<Vec<u8>>) {
        self.cache.insert(chunk_id, data);
    }

    pub fn invalidate_range(&self, start_byte: u64, write_len: u64) {
        let end_byte = start_byte + write_len;
        let start_chunk = start_byte / self.chunk_size_bytes;
        let end_chunk = if end_byte > 0 { (end_byte - 1) / self.chunk_size_bytes } else { 0 };
        for chunk_id in start_chunk..=end_chunk {
            self.cache.invalidate(&chunk_id);
        }
    }

    pub fn clear(&self) {
        self.cache.invalidate_all();
        self.cache.run_pending_tasks();
    }

    pub fn stats(&self) -> CacheStatsSnapshot {
        self.cache.run_pending_tasks();
        let entry_count = self.cache.entry_count();
        let used_cache_mb = (entry_count * self.chunk_size_bytes) / (1024 * 1024);
        let hits = self.hit_count.load(Ordering::Relaxed);
        let misses = self.miss_count.load(Ordering::Relaxed);
        let total = hits + misses;
        let hit_rate_pct = if total > 0 {
            (hits as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        CacheStatsSnapshot {
            ram_cache_mb: self.ram_cache_mb,
            chunk_size_bytes: self.chunk_size_bytes,
            total_chunks: self.max_chunks,
            entry_count,
            used_cache_mb,
            hit_count: hits,
            miss_count: misses,
            hit_rate_pct,
            cached_read_bytes: self.cached_read_bytes.load(Ordering::Relaxed),
            direct_read_bytes: self.direct_read_bytes.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_ahead_telemetry() {
        let cache = ReadAheadCache::new_mb(64, 256 * 1024).expect("Cache must initialize with 64 MB");
        assert_eq!(cache.ram_cache_mb, 64);

        // Initial stats
        let stats0 = cache.stats();
        assert_eq!(stats0.hit_count, 0);
        assert_eq!(stats0.miss_count, 0);
        assert_eq!(stats0.hit_rate_pct, 0.0);

        // 1. Miss
        assert!(cache.get(10).is_none());
        let stats1 = cache.stats();
        assert_eq!(stats1.hit_count, 0);
        assert_eq!(stats1.miss_count, 1);
        assert_eq!(stats1.hit_rate_pct, 0.0);

        // 2. Insert and Hit
        let dummy = Arc::new(vec![0xAAu8; 256 * 1024]);
        cache.insert(10, dummy);
        assert!(cache.get(10).is_some());

        let stats2 = cache.stats();
        assert_eq!(stats2.hit_count, 1);
        assert_eq!(stats2.miss_count, 1);
        assert!((stats2.hit_rate_pct - 50.0).abs() < 0.1);

        // 3. Clear
        cache.clear();
        assert!(cache.get(10).is_none());

        // 4. try_get_or_load (Miss then Hit)
        let loaded = cache.try_get_or_load(20, || {
            Ok(Arc::new(vec![0xBBu8; 256 * 1024]))
        }).expect("Must load chunk");
        assert_eq!(loaded[0], 0xBB);

        // Second time must be HIT without calling closure
        let hit = cache.try_get_or_load(20, || {
            panic!("Should not be called on cache hit!");
        }).expect("Must hit cache");
        assert_eq!(hit[0], 0xBB);
    }
}
