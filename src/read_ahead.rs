use std::sync::Arc;
use moka::sync::Cache;
use std::time::Duration;
use tracing::info;

pub struct ReadAheadCache {
    pub cache: Cache<u64, Arc<Vec<u8>>>,
    pub chunk_size_bytes: u64,
}

impl ReadAheadCache {
    pub fn new(read_cache_gb: u64, chunk_size_bytes: u64) -> Option<Self> {
        if read_cache_gb > 0 {
            let max_chunks = (read_cache_gb * 1024 * 1024 * 1024) / chunk_size_bytes;
            info!("Inisialisasi Read-Ahead cache: {} GB ({} chunks of {} bytes)", read_cache_gb, max_chunks, chunk_size_bytes);
            Some(Self {
                cache: Cache::builder()
                    .max_capacity(max_chunks)
                    .time_to_idle(Duration::from_secs(300))
                    .build(),
                chunk_size_bytes,
            })
        } else {
            None
        }
    }

    pub fn get(&self, chunk_id: u64) -> Option<Arc<Vec<u8>>> {
        self.cache.get(&chunk_id)
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
}
