use crate::backend::Backend;
use crate::writeback_gamedisk::ClientCache;
use crate::writeback_imagedisk;
use crate::stats::ServerStats;
use crate::pdu::{
    self, OP_SCSI_CMD, OP_TMF_REQ,
    OP_NOP_OUT, OP_LOGOUT_REQ, OP_TEXT_REQ, OP_DATA_OUT,
};
use std::net::IpAddr;
use std::sync::Arc;
use std::collections::HashMap;

use tokio::net::TcpStream;
pub mod scsi_handler;
pub mod scsi_handler_gamedisk;
pub mod scsi_handler_imagedisk;
pub mod pdu_io;
pub mod login;
use tracing::{info, warn, error};



pub struct PendingWrite {
    pub lun_id: u8,
    pub lba: u64,
    pub num_blocks: u32,
    pub expected_len: usize,
    pub buffer: Vec<u8>,
}

pub struct WriteJob {
    pub lun_id: u8,
    pub lba: u64,
    pub num_blocks: u32,
    pub buffer: Vec<u8>,
}

pub struct Session {
    stream: TcpStream,
    peer_addr: std::net::SocketAddr,
    local_addr: std::net::SocketAddr,
    client_ip: String,
    gamedisk_backends: Arc<std::sync::RwLock<HashMap<u8, Arc<Backend>>>>,
    backends: HashMap<u8, Arc<Backend>>,
    config: crate::config_manager::SharedConfig,
    client_caches: HashMap<u8, Arc<ClientCache>>,
    is_imagedisk: bool,
    child_vhd_path: Option<String>,
    is_super: bool,
    stats: Arc<ServerStats>,

    target_iqn: String,
    initiator_iqn: String,
    is_discovery: bool,
    stat_sn: u32,
    exp_cmd_sn: u32,
    max_cmd_sn: u32,
    max_recv_data_segment_len: usize,
    pub pending_writes: HashMap<u32, PendingWrite>,
    throttle_window_start: std::sync::atomic::AtomicU64,
    throttle_bytes_this_window: std::sync::atomic::AtomicU64,
    chosen_writeback_dir: String,
}

impl Session {
    pub fn new(
        stream: TcpStream,
        client_ip: IpAddr,
        gamedisk_backends: Arc<std::sync::RwLock<HashMap<u8, Arc<Backend>>>>,
        config: crate::config_manager::SharedConfig,
        stats: Arc<ServerStats>,
    ) -> Self {
        // Konfigurasi TCP: disable Nagle
        let _ = stream.set_nodelay(true);





        let peer_addr = stream.peer_addr().unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap());
        let local_addr = stream.local_addr().unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap());

        let writeback_dirs = &config.read().writeback.writeback_dirs;
        let chosen_dir = if !writeback_dirs.is_empty() {
            let client_ip_str = client_ip.to_string();
            let idx = if let Some(last_octet_str) = client_ip_str.split('.').last() {
                if let Ok(octet) = last_octet_str.parse::<usize>() {
                    octet % writeback_dirs.len()
                } else {
                    0
                }
            } else {
                0
            };
            writeback_dirs[idx].clone()
        } else {
            String::new()
        };

        Session {
            stream,
            peer_addr,
            local_addr,
            client_ip: client_ip.to_string(),
            gamedisk_backends,
            backends: HashMap::new(),
            config,
            client_caches: HashMap::new(),
            is_imagedisk: false,  // will be set in run() after login IQN check
            child_vhd_path: None,
            is_super: false,
            stats,
            target_iqn: String::new(),
            initiator_iqn: String::new(),
            is_discovery: false,
            stat_sn: 1,
            exp_cmd_sn: 0,
            max_cmd_sn: 256,
            max_recv_data_segment_len: 262144, // 256KB
            pending_writes: HashMap::new(),
            throttle_window_start: std::sync::atomic::AtomicU64::new(0),
            throttle_bytes_this_window: std::sync::atomic::AtomicU64::new(0),
            chosen_writeback_dir: chosen_dir,
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stats.active_sessions.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        self.stats.record_session_end(&self.client_ip);
    }
}

impl Session {
    /// Menjalankan state machine sesi.
    pub async fn run(mut self) -> Result<(), std::io::Error> {
        let peer_addr = self.peer_addr;
        info!("Sesi baru dimulai dari client: {}", peer_addr);

        self.handle_login_phase().await?;
        self.stats.record_session_start(&self.client_ip);

        let target_name: String;

        if self.is_discovery {
            target_name = "discovery".to_string();
            info!("Sesi adalah Discovery Session. Melewati inisialisasi backend.");
        } else if self.target_iqn == self.config.read().gamedisk_target.target_iqn {
            target_name = "gamedisk".to_string();
            // Gamedisk target -> muat semua LUN gamedisk
            let gamedisks = self.gamedisk_backends.read().unwrap();
            for (lun_id, backend) in gamedisks.iter() {
                self.backends.insert(*lun_id, Arc::clone(backend));
            }
        } else if self.config.read().windows.as_ref().map_or(false, |win| self.target_iqn.starts_with(&win.target_iqn_prefix)) {
            let config_guard = self.config.read();
            let win = config_guard.windows.as_ref().unwrap();
            self.is_imagedisk = true;
            let suffix = &self.target_iqn[win.target_iqn_prefix.len()..];
            target_name = suffix.to_string();

            // Gunakan writeback_imagedisk — kalo super client, serve super VHD langsung
            self.is_super = self.client_ip == win.super_client_ip;
            match writeback_imagedisk::init_child_vhd(
                &config_guard,
                &self.client_ip,
                suffix,
                self.is_super,
            ) {
                Ok(result) => {
                    self.backends.insert(0, result.backend);
                    self.child_vhd_path = result.child_path; // None untuk super VHD, Some untuk child
                }
                Err(e) => {
                    error!("Gagal init VHD: {}", e);
                    return Ok(());
                }
            }
        } else {
            error!("Target IQN tidak valid atau tidak dikenali: {}", self.target_iqn);
            return Ok(()); // Putuskan koneksi
        }

        // 2. Inisialisasi Cache
        if !self.is_discovery {
            // Buat cache untuk gamedisk, dan untuk imagedisk (normal client saja)
            let is_imagedisk_normal = self.is_imagedisk && !self.is_super;
            if !self.is_imagedisk || is_imagedisk_normal {
                for (lun_id, backend) in self.backends.iter() {
                    let cache_name = format!("{}_lun{}", target_name, lun_id);
                    info!("Membuat cache writeback untuk LUN {} ({})", lun_id, cache_name);
                    let dirs = if self.chosen_writeback_dir.is_empty() {
                        vec![]
                    } else {
                        vec![self.chosen_writeback_dir.clone()]
                    };
                    
                    let cache = ClientCache::new(
                        &dirs,
                        &self.client_ip,
                        &cache_name,
                        backend.block_size(),
                        self.config.read().writeback.max_cache_per_client_gb,
                        false, // is_super is always false for cache
                        self.config.read().writeback.max_write_speed_mbps,
                    )?;
                    self.client_caches.insert(*lun_id, Arc::new(cache));
                }
            } else {
                info!("Super Client ImageDisk session — write langsung ke differencing VHD, tanpa cache");
            }
        }



        // 3. FFP Message Loop
        let mut logged_out = false;
        
        let loop_result: Result<(), std::io::Error> = async {
            loop {
                let req = match pdu::parser::read_pdu(&mut self.stream).await {
                    Ok(p) => p,
                    Err(e) => {
                        info!("TCP connection closed or errored: {}", e);
                        break;
                    }
                };

                let is_immediate = req.is_immediate;
                if !is_immediate && req.cmd_sn != 0xFFFFFFFF {
                    self.exp_cmd_sn = req.cmd_sn.wrapping_add(1);
                    self.max_cmd_sn = self.exp_cmd_sn.wrapping_add(32);
                }

                match req.opcode {
                    OP_NOP_OUT => {
                        self.handle_nop_out(req).await?;
                    }
                    OP_LOGOUT_REQ => {
                        // ignore error if client closes socket early after sending LOGOUT
                        let _ = self.handle_logout(req).await;
                        logged_out = true;
                        break; // Selesai
                    }
                    OP_TEXT_REQ => {
                        self.handle_text_req(req).await?;
                    }
                    OP_TMF_REQ => {
                        self.handle_tmf_req(req).await?;
                    }
                    OP_SCSI_CMD => {
                        self.handle_scsi_cmd(req).await?;
                    }
                    OP_DATA_OUT => {
                        self.handle_data_out(req).await?;
                    }
                    _ => {
                        warn!("Menerima opcode PDU tidak didukung di FFP: 0x{:02X}", req.opcode);
                    }
                }
            }
            Ok(())
        }.await;

        if let Err(e) = loop_result {
            info!("Session error atau terputus: {}", e);
        }

        // Cleanup VHD via writeback_imagedisk — super VHD persistent, child VHD dihapus
        if self.is_imagedisk {
            writeback_imagedisk::cleanup_child_vhd(
                self.child_vhd_path.as_deref(),
                &self.client_ip,
                &self.config.read(),
            );
        }

        // Sesi selesai (karena LOGOUT atau TCP disconnect) -> hapus writeback gamedisk
        for (lun_id, cache_arc) in self.client_caches.drain() {
            info!("Sesi berakhir (logout/disconnect) — menghapus gamedisk cache LUN {}", lun_id);
            if let Ok(cache) = Arc::try_unwrap(cache_arc) {
                cache.cleanup_and_drop();
            } else {
                warn!("Tidak dapat cleanup LUN {} karena cache masih direferensikan oleh thread lain", lun_id);
            }
        }

        info!("Koneksi dengan client {} selesai.", peer_addr);
        Ok(())
    }

    pub async fn throttle_write(&self, bytes_to_write: usize) {
        let max_speed_mbps = self.config.read().writeback.max_write_speed_mbps;
        if max_speed_mbps == 0 {
            return;
        }

        let max_write_bytes_per_sec = max_speed_mbps * 1024 * 1024;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let window_start = self.throttle_window_start.load(std::sync::atomic::Ordering::Relaxed);
        let elapsed = now_ms.saturating_sub(window_start);

        if elapsed >= 100 {
            self.throttle_window_start.store(now_ms, std::sync::atomic::Ordering::Relaxed);
            self.throttle_bytes_this_window.store(0, std::sync::atomic::Ordering::Relaxed);
        }

        let max_per_window = max_write_bytes_per_sec / 10;
        let written = self.throttle_bytes_this_window.fetch_add(bytes_to_write as u64, std::sync::atomic::Ordering::Relaxed);

        if written + bytes_to_write as u64 > max_per_window {
            let sleep_ms = 100u64.saturating_sub(elapsed);
            if sleep_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(sleep_ms)).await;
            }
        }
    }
}
