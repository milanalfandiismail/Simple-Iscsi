use crate::backend::Backend;
use crate::writeback_gamedisk::ClientCache;
use crate::writeback_imagedisk;
use crate::stats::ServerStats;
use crate::pdu::{
    self, Pdu, OP_SCSI_CMD, OP_TMF_REQ,
    OP_NOP_OUT, OP_LOGOUT_REQ, OP_TEXT_REQ, OP_DATA_OUT,
};
use std::net::IpAddr;
use std::sync::Arc;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use parking_lot::Mutex;

use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc;
use tracing::{info, warn, error};

pub mod scsi_handler;
pub mod scsi_handler_gamedisk;
pub mod scsi_handler_imagedisk;
pub mod pdu_io;
pub mod login;

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

// Messages sent from worker tasks to the dedicated Writer Task
pub enum WriterMessage {
    Pdu(Pdu),
    DataIn {
        itt: u32,
        data: Vec<u8>,
        status: u8,
        expected_len: u32,
    },
    R2T {
        itt: u32,
        lun: u64,
        buffer_offset: u32,
        desired_len: u32,
    },
    ScsiResponse {
        itt: u32,
        status: u8,
        exp_data_sn: u32,
        expected_len: u32,
        actual_len: u32,
    },
    CheckCondition {
        itt: u32,
        key: u8,
        asc: u8,
        ascq: u8,
    },
}

pub struct SessionContext {
    pub client_ip: String,
    pub peer_addr: std::net::SocketAddr,
    pub local_addr: std::net::SocketAddr,
    pub backends: HashMap<u8, Arc<Backend>>,
    pub client_caches: HashMap<u8, Arc<ClientCache>>,
    pub is_imagedisk: bool,
    pub is_super: bool,
    pub is_discovery: bool,
    pub max_recv_data_segment_len: usize,
    pub pending_writes: Mutex<HashMap<u32, PendingWrite>>,
    pub exp_cmd_sn: AtomicU32,
    pub max_cmd_sn: AtomicU32,
    pub stats: Arc<ServerStats>,
    pub config: crate::config_manager::SharedConfig,
    pub throttle_window_start: AtomicU64,
    pub throttle_bytes_this_window: AtomicU64,
    pub tx: mpsc::Sender<WriterMessage>,
}

impl Drop for SessionContext {
    fn drop(&mut self) {
        self.stats.active_sessions.fetch_sub(1, Ordering::Relaxed);
        self.stats.record_session_end(&self.client_ip);
    }
}

impl SessionContext {
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

        let window_start = self.throttle_window_start.load(Ordering::Relaxed);
        let elapsed = now_ms.saturating_sub(window_start);

        if elapsed >= 100 {
            self.throttle_window_start.store(now_ms, Ordering::Relaxed);
            self.throttle_bytes_this_window.store(0, Ordering::Relaxed);
        }

        let max_per_window = max_write_bytes_per_sec / 10;
        let written = self.throttle_bytes_this_window.fetch_add(bytes_to_write as u64, Ordering::Relaxed);

        if written + bytes_to_write as u64 > max_per_window {
            let sleep_ms = 100u64.saturating_sub(elapsed);
            if sleep_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(sleep_ms)).await;
            }
        }
    }
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
    throttle_window_start: AtomicU64,
    throttle_bytes_this_window: AtomicU64,
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
            is_imagedisk: false,
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
            throttle_window_start: AtomicU64::new(0),
            throttle_bytes_this_window: AtomicU64::new(0),
            chosen_writeback_dir: chosen_dir,
        }
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

            self.is_super = self.client_ip == win.super_client_ip;
            match writeback_imagedisk::init_child_vhd(
                &config_guard,
                &self.client_ip,
                suffix,
                self.is_super,
            ) {
                Ok(result) => {
                    self.backends.insert(0, result.backend);
                    self.child_vhd_path = result.child_path;
                }
                Err(e) => {
                    error!("Gagal init VHD: {}", e);
                    return Ok(());
                }
            }
        } else {
            error!("Target IQN tidak valid atau tidak dikenali: {}", self.target_iqn);
            return Ok(());
        }

        // 2. Inisialisasi Cache
        if !self.is_discovery {
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
                        false,
                        self.config.read().writeback.max_write_speed_mbps,
                    )?;
                    self.client_caches.insert(*lun_id, Arc::new(cache));
                }
            } else {
                info!("Super Client ImageDisk session — write langsung ke differencing VHD, tanpa cache");
            }
        }

        // Destructure self to allow moving out non-copy fields cleanly
        let Session {
            stream,
            peer_addr,
            local_addr,
            client_ip,
            backends,
            client_caches,
            is_imagedisk,
            is_super,
            max_recv_data_segment_len,
            pending_writes,
            exp_cmd_sn,
            max_cmd_sn,
            stats,
            config,
            throttle_window_start,
            throttle_bytes_this_window,
            is_discovery,
            stat_sn,
            child_vhd_path,
            ..
        } = self;

        // 3. Split TCP Stream dan Setup Channel
        let (mut read_half, mut write_half) = stream.into_split();
        let (tx, mut rx) = mpsc::channel::<WriterMessage>(1024);

        let context = Arc::new(SessionContext {
            client_ip,
            peer_addr,
            local_addr,
            backends,
            client_caches: client_caches.clone(),
            is_imagedisk,
            is_super,
            is_discovery,
            max_recv_data_segment_len,
            pending_writes: Mutex::new(pending_writes),
            exp_cmd_sn: AtomicU32::new(exp_cmd_sn),
            max_cmd_sn: AtomicU32::new(max_cmd_sn),
            stats: Arc::clone(&stats),
            config,
            throttle_window_start,
            throttle_bytes_this_window,
            tx,
        });

        // 4. Spawn Writer Task
        let context_writer = Arc::clone(&context);
        let writer_handle = tokio::spawn(async move {
            let mut local_stat_sn = stat_sn;
            while let Some(msg) = rx.recv().await {
                if let Err(e) = pdu_io::write_message(&mut write_half, &context_writer, msg, &mut local_stat_sn).await {
                    info!("Writer task exited: {}", e);
                    break;
                }
            }
        });

        // 5. Run Reader Loop (FFP Message Loop)
        let context_reader = Arc::clone(&context);
        let loop_result: Result<(), std::io::Error> = async {
            loop {
                let req = match pdu::parser::read_pdu(&mut read_half).await {
                    Ok(p) => p,
                    Err(e) => {
                        info!("TCP connection closed or errored: {}", e);
                        break;
                    }
                };

                let is_immediate = req.is_immediate;
                if !is_immediate && req.cmd_sn != 0xFFFFFFFF {
                    let next_exp = req.cmd_sn.wrapping_add(1);
                    context_reader.exp_cmd_sn.store(next_exp, Ordering::Relaxed);
                    context_reader.max_cmd_sn.store(next_exp.wrapping_add(32), Ordering::Relaxed);
                }

                match req.opcode {
                    OP_NOP_OUT => {
                        let ctx = Arc::clone(&context_reader);
                        tokio::spawn(async move {
                            if let Err(e) = ctx.handle_nop_out(req).await {
                                error!("Gagal memproses NOP-Out: {}", e);
                            }
                        });
                    }
                    OP_LOGOUT_REQ => {
                        let _ = context_reader.handle_logout(req).await;
                        break;
                    }
                    OP_TEXT_REQ => {
                        let ctx = Arc::clone(&context_reader);
                        tokio::spawn(async move {
                            if let Err(e) = ctx.handle_text_req(req).await {
                                error!("Gagal memproses Text-Req: {}", e);
                            }
                        });
                    }
                    OP_TMF_REQ => {
                        let ctx = Arc::clone(&context_reader);
                        tokio::spawn(async move {
                            if let Err(e) = ctx.handle_tmf_req(req).await {
                                error!("Gagal memproses TMF-Req: {}", e);
                            }
                        });
                    }
                    OP_SCSI_CMD => {
                        let scsi_opcode = req.custom_bhs[0];
                        let is_write = scsi_opcode == 0x2A || scsi_opcode == 0x8A;
                        
                        if is_write {
                            // Jalankan secara synchronous untuk WRITE agar terhindar dari race condition dengan OP_DATA_OUT
                            if let Err(e) = context_reader.handle_scsi_cmd(req).await {
                                error!("Gagal memproses SCSI-Cmd WRITE secara synchronous: {}", e);
                            }
                        } else {
                            // READ dan command lain tetap asynchronous untuk performa pipelining
                            let ctx = Arc::clone(&context_reader);
                            tokio::spawn(async move {
                                if let Err(e) = ctx.handle_scsi_cmd(req).await {
                                    error!("Gagal memproses SCSI-Cmd: {}", e);
                                }
                            });
                        }
                    }
                    OP_DATA_OUT => {
                        // Jalankan secara synchronous agar data yang masuk berurutan masuk ke buffer yang tepat
                        if let Err(e) = context_reader.handle_data_out(req).await {
                            error!("Gagal memproses Data-Out secara synchronous: {}", e);
                        }
                    }
                    _ => {
                        warn!("Menerima opcode PDU tidak didukung di FFP: 0x{:02X}", req.opcode);
                    }
                }
            }
            Ok(())
        }.await;

        drop(context_reader); // Lepaskan referensi reader agar SessionContext bisa di-drop dan cache unwrap berhasil

        if let Err(e) = loop_result {
            info!("Session error atau terputus: {}", e);
        }

        // Wait for writer task to finish processing any pending packets
        let client_ip_cleanup = context.client_ip.clone();
        let config_cleanup = context.config.clone();
        drop(context); // Drop the last sender in this thread to notify the receiver
        let _ = writer_handle.await;

        // Cleanup VHD via writeback_imagedisk
        if is_imagedisk {
            writeback_imagedisk::cleanup_child_vhd(
                child_vhd_path.as_deref(),
                &client_ip_cleanup,
                &config_cleanup.read(),
            );
        }

        // Cleanup writeback gamedisk caches
        info!("Sesi berakhir (logout/disconnect) — membersihkan gamedisk cache.");
        drop(client_caches);

        info!("Koneksi dengan client {} selesai.", peer_addr);
        Ok(())
    }
}
