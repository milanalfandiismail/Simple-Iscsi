use crate::backend::Backend;
use crate::writeback_gamedisk::ClientCache;
use crate::writeback_imagedisk;
use crate::stats::ServerStats;
use crate::pdu::{
    self, Pdu, OP_LOGIN_REQ, OP_LOGIN_RESP, OP_SCSI_CMD,
    OP_NOP_OUT, OP_LOGOUT_REQ, OP_TEXT_REQ, STAGE_FULL_FEATURE_PHASE,
};
use std::net::IpAddr;
use std::sync::Arc;
use std::collections::HashMap;

use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
pub mod scsi_handler;
pub mod scsi_handler_gamedisk;
pub mod scsi_handler_imagedisk;
pub mod pdu_io;
use tracing::{info, warn, error};

pub struct Session {
    stream: TcpStream,
    client_ip: String,
    gamedisk_backends: Arc<HashMap<u8, Arc<Backend>>>,
    backends: HashMap<u8, Arc<Backend>>,
    config: Arc<crate::config::Config>,
    client_caches: HashMap<u8, ClientCache>,
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
    /// Reusable read buffer – eliminate alloc per READ10.
    read_buf: Vec<u8>,
}

impl Session {
    pub fn new(
        stream: TcpStream,
        client_ip: IpAddr,
        gamedisk_backends: Arc<HashMap<u8, Arc<Backend>>>,
        config: Arc<crate::config::Config>,
        stats: Arc<ServerStats>,
    ) -> Self {
        // Konfigurasi TCP: disable Nagle
        let _ = stream.set_nodelay(true);

        // Set SO_SNDBUF = 512KB via raw socket (stdlib set_send_buffer_size requires
        // into_std which consumes the stream — raw FFI avoids the ownership dance)
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawSocket;

            type SOCKET = u64;
            #[allow(non_camel_case_types)]
            type c_int = i32;

            const SOL_SOCKET: c_int = 0xFFFF;   // SOL_SOCKET on Windows
            const SO_SNDBUF: c_int = 0x1001;    // SO_SNDBUF on Windows

            extern "system" {
                fn setsockopt(
                    s: SOCKET,
                    level: c_int,
                    optname: c_int,
                    optval: *const std::ffi::c_void,
                    optlen: c_int,
                ) -> c_int;
            }

            let socket = stream.as_raw_socket() as SOCKET;
            let val: u32 = 512 * 1024;
            unsafe {
                setsockopt(
                    socket,
                    SOL_SOCKET,
                    SO_SNDBUF,
                    &val as *const u32 as *const std::ffi::c_void,
                    std::mem::size_of::<u32>() as c_int,
                );
            }
        }

        Session {
            stream,
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
            max_cmd_sn: 32,
            max_recv_data_segment_len: 16777216, // 16MB
            read_buf: Vec::new(),
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stats.active_sessions.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Session {
    /// Menjalankan state machine sesi.
    pub async fn run(mut self) -> Result<(), std::io::Error> {
        let peer_addr = self.stream.peer_addr()?;
        info!("Sesi baru dimulai dari client: {}", peer_addr);

        // 1. Fase Login
        let mut in_login = true;
        while in_login {
            let req = pdu::parser::read_pdu(&mut self.stream).await?;
            if req.opcode != OP_LOGIN_REQ {
                warn!("Menerima opcode non-login selama fase login: 0x{:02X}", req.opcode);
                return Ok(());
            }

            // Flag stage transitions
            let req_flags = req.flags;
            let transit = (req_flags & 0x80) != 0;
            let csg = (req_flags & 0x0C) >> 2;
            let nsg = req_flags & 0x03;

            let params = pdu::parser::parse_text_parameters(&req.data);
            info!("Menerima Login Request parameters: {:?}", params);
            if let Some(iqn) = params.get("InitiatorName") {
                self.initiator_iqn = iqn.clone();
            }
            if let Some(st) = params.get("SessionType") {
                self.is_discovery = st == "Discovery";
            }
            if let Some(tn) = params.get("TargetName") {
                self.target_iqn = tn.clone();
            }
            if let Some(val) = params.get("MaxRecvDataSegmentLength") {
                if let Ok(len) = val.parse::<usize>() {
                    self.max_recv_data_segment_len = len.min(16 * 1024 * 1024);
                }
            }

            // Siapkan parameter negosiasi
            let mut resp_params = Vec::new();
            if params.contains_key("AuthMethod") {
                resp_params.push(("AuthMethod".to_string(), "None".to_string()));
            }
            if params.contains_key("HeaderDigest") {
                resp_params.push(("HeaderDigest".to_string(), "None".to_string()));
            }
            if params.contains_key("DataDigest") {
                resp_params.push(("DataDigest".to_string(), "None".to_string()));
            }
            if !self.is_discovery && csg == pdu::STAGE_SECURITY_NEGOTIATION {
                resp_params.push(("TargetPortalGroupTag".to_string(), "1".to_string()));
            }
            
            // Negosiasikan opsi iSCSI standard jika di-request oleh client
            if let Some(val) = params.get("ImmediateData") {
                resp_params.push(("ImmediateData".to_string(), val.clone()));
            }
            if params.contains_key("InitialR2T") {
                // Force No supaya initiator kirim unsolicited data seluas FirstBurstLength
                resp_params.push(("InitialR2T".to_string(), "No".to_string()));
            }
            if params.contains_key("MaxOutstandingR2T") {
                // Kita hanya mendukung 1 outstanding R2T
                resp_params.push(("MaxOutstandingR2T".to_string(), "1".to_string()));
            }
            if params.contains_key("MaxConnections") {
                // 4 koneksi per sesi
                resp_params.push(("MaxConnections".to_string(), "4".to_string()));
            }
            if params.contains_key("ErrorRecoveryLevel") {
                // Kita hanya mendukung level 0
                resp_params.push(("ErrorRecoveryLevel".to_string(), "0".to_string()));
            }
            if let Some(val) = params.get("DefaultTime2Wait") {
                resp_params.push(("DefaultTime2Wait".to_string(), val.clone()));
            }
            if let Some(val) = params.get("DefaultTime2Retain") {
                resp_params.push(("DefaultTime2Retain".to_string(), val.clone()));
            }
            if let Some(val) = params.get("DataPDUInOrder") {
                resp_params.push(("DataPDUInOrder".to_string(), val.clone()));
            }
            if let Some(val) = params.get("DataSequenceInOrder") {
                resp_params.push(("DataSequenceInOrder".to_string(), val.clone()));
            }
            if params.contains_key("MaxRecvDataSegmentLength") {
                resp_params.push(("MaxRecvDataSegmentLength".to_string(), "16777216".to_string())); // 1MB
            }
            if let Some(val) = params.get("FirstBurstLength") {
                resp_params.push(("FirstBurstLength".to_string(), val.clone()));
            }
            if let Some(val) = params.get("MaxBurstLength") {
                resp_params.push(("MaxBurstLength".to_string(), val.clone()));
            }

            let mut resp = Pdu::default();
            resp.opcode = OP_LOGIN_RESP;
            resp.flags = (csg << 2) | nsg;
            if transit {
                resp.flags |= 0x80;
                if nsg == STAGE_FULL_FEATURE_PHASE {
                    in_login = false;
                }
            }

            let isid = req.lun & 0xFFFFFFFFFFFF0000;
            let tsih: u16 = if self.is_discovery { 0 } else { 1 };
            resp.lun = isid | (tsih as u64);
            resp.initiator_task_tag = req.initiator_task_tag;
            
            // Response sequence numbers
            resp.cmd_sn = self.stat_sn;
            self.stat_sn = self.stat_sn.wrapping_add(1);
            
            self.exp_cmd_sn = req.cmd_sn.wrapping_add(1);
            self.max_cmd_sn = self.exp_cmd_sn.wrapping_add(32);
            
            resp.exp_stat_sn = self.exp_cmd_sn;
            resp.max_cmd_sn = self.max_cmd_sn;
            
            info!("Mengirim Login Response parameters: {:?}", resp_params);
            resp.data = pdu::builder::build_text_parameters(&resp_params);

            let packet = pdu::builder::build_pdu(&resp);
            self.stream.write_all(&packet).await?;
            self.stream.flush().await?;
        }

        info!("Transisi login sukses. Client masuk ke FFP (Full Feature Phase).");

        let target_name: String;

        if self.is_discovery {
            target_name = "discovery".to_string();
            info!("Sesi adalah Discovery Session. Melewati inisialisasi backend.");
        } else if self.target_iqn == self.config.gamedisk_target.target_iqn {
            target_name = "gamedisk".to_string();
            // Gamedisk target -> muat semua LUN gamedisk
            for (lun_id, backend) in self.gamedisk_backends.iter() {
                self.backends.insert(*lun_id, Arc::clone(backend));
            }
        } else if self.target_iqn.starts_with(&self.config.windows.target_iqn_prefix) {
            self.is_imagedisk = true;
            let suffix = &self.target_iqn[self.config.windows.target_iqn_prefix.len()..];
            target_name = suffix.to_string();

            // Gunakan writeback_imagedisk — kalo super client, serve super VHD langsung
            self.is_super = self.client_ip == self.config.windows.super_client_ip;
            match writeback_imagedisk::init_child_vhd(
                &self.config,
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

        // 2. Inisialisasi Cache — SKIP untuk imagedisk (child VHD handles writes directly)
        if !self.is_discovery && !self.is_imagedisk {
            for (lun_id, backend) in self.backends.iter() {
                let cache_name = format!("{}_lun{}", target_name, lun_id);
                info!("Membuat cache writeback untuk LUN {} ({})", lun_id, cache_name);
                let cache = ClientCache::new(
                    &self.config.writeback.writeback_dirs,
                    &self.client_ip,
                    &cache_name,
                    backend.block_size(),
                    self.config.writeback.max_cache_per_client_gb,
                    false, // ⚠️ FIX: gamedisk tidak boleh pakai is_super, selalu false
                )?;
                self.client_caches.insert(*lun_id, cache);
            }
        } else if self.is_imagedisk {
            info!("ImageDisk session — write langsung ke child VHD, tanpa cache .bin");
        }

        // 3. FFP Message Loop
        let mut logged_out = false;
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
                    self.handle_logout(req).await?;
                    logged_out = true;
                    break; // Selesai
                }
                OP_TEXT_REQ => {
                    self.handle_text_req(req).await?;
                }
                OP_SCSI_CMD => {
                    self.handle_scsi_cmd(req).await?;
                }
                _ => {
                    warn!("Menerima opcode PDU tidak didukung di FFP: 0x{:02X}", req.opcode);
                }
            }
        }

        // Cleanup VHD via writeback_imagedisk — super VHD persistent, child VHD dihapus
        if self.is_imagedisk {
            writeback_imagedisk::cleanup_child_vhd(
                self.child_vhd_path.as_deref(),
                &self.client_ip,
                &self.config,
            );
        }

        // On explicit LOGOUT, delete gamedisk .bin caches (selesai bermain)
        // On TCP disconnect (reboot), keep .bin so data survives
        if logged_out {
            for (lun_id, cache) in self.client_caches.iter() {
                info!("Client logout — menghapus gamedisk cache LUN {}", lun_id);
                cache.cleanup();
            }
        } else {
            // TCP disconnect → flush but keep .bin files
            for (lun_id, cache) in self.client_caches.iter() {
                info!("TCP disconnect — mempertahankan gamedisk cache LUN {}", lun_id);
                if let Err(e) = cache.flush() {
                    warn!("Gagal flush cache LUN {}: {}", lun_id, e);
                }
            }
        }

        info!("Koneksi dengan client {} selesai.", peer_addr);
        Ok(())
    }

}
