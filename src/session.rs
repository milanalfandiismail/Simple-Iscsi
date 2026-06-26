use crate::backend::Backend;
use crate::cache::ClientCache;
use crate::pdu::{
    self, Pdu, OP_LOGIN_REQ, OP_LOGIN_RESP, OP_SCSI_CMD, OP_SCSI_RESP, OP_DATA_IN, OP_DATA_OUT,
    OP_NOP_OUT, OP_NOP_IN, OP_LOGOUT_REQ, OP_LOGOUT_RESP, OP_TEXT_REQ, OP_TEXT_RESP,
    OP_R2T, STAGE_FULL_FEATURE_PHASE,
};
use crate::scsi::{self, ScsiResult};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tracing::{error, info, trace, warn};

pub struct Session {
    stream: TcpStream,
    client_ip: String,
    backend: Arc<Backend>,
    cache_dir: String,
    max_cache_gb: u64,
    client_cache: Option<ClientCache>,
    
    initiator_iqn: String,
    is_discovery: bool,
    stat_sn: u32,
    exp_cmd_sn: u32,
    max_cmd_sn: u32,
    max_recv_data_segment_len: usize,
}

impl Session {
    pub fn new(
        stream: TcpStream,
        client_ip: IpAddr,
        backend: Arc<Backend>,
        cache_dir: String,
        max_cache_gb: u64,
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
            backend,
            cache_dir,
            max_cache_gb,
            client_cache: None,
            initiator_iqn: String::new(),
            is_discovery: false,
            stat_sn: 1,
            exp_cmd_sn: 0,
            max_cmd_sn: 32,
            max_recv_data_segment_len: 1048576, // 1MB
        }
    }

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
            if let Some(val) = params.get("MaxRecvDataSegmentLength") {
                if let Ok(len) = val.parse::<usize>() {
                    self.max_recv_data_segment_len = len.min(262144);
                }
            }

            // Siapkan parameter negosiasi
            let mut resp_params = HashMap::new();
            if params.contains_key("AuthMethod") {
                resp_params.insert("AuthMethod".to_string(), "None".to_string());
            }
            if params.contains_key("HeaderDigest") {
                resp_params.insert("HeaderDigest".to_string(), "None".to_string());
            }
            if params.contains_key("DataDigest") {
                resp_params.insert("DataDigest".to_string(), "None".to_string());
            }
            if !self.is_discovery && csg == pdu::STAGE_SECURITY_NEGOTIATION {
                resp_params.insert("TargetPortalGroupTag".to_string(), "1".to_string());
            }
            
            // Negosiasikan opsi iSCSI standard jika di-request oleh client
            if let Some(val) = params.get("ImmediateData") {
                resp_params.insert("ImmediateData".to_string(), val.clone());
            }
            if params.contains_key("InitialR2T") {
                // Force No supaya initiator kirim unsolicited data seluas FirstBurstLength
                resp_params.insert("InitialR2T".to_string(), "No".to_string());
            }
            if params.contains_key("MaxOutstandingR2T") {
                // Kita hanya mendukung 1 outstanding R2T
                resp_params.insert("MaxOutstandingR2T".to_string(), "1".to_string());
            }
            if params.contains_key("MaxConnections") {
                // Kita hanya mendukung 1 koneksi per sesi
                resp_params.insert("MaxConnections".to_string(), "1".to_string());
            }
            if params.contains_key("ErrorRecoveryLevel") {
                // Kita hanya mendukung level 0
                resp_params.insert("ErrorRecoveryLevel".to_string(), "0".to_string());
            }
            if let Some(val) = params.get("DefaultTime2Wait") {
                resp_params.insert("DefaultTime2Wait".to_string(), val.clone());
            }
            if let Some(val) = params.get("DefaultTime2Retain") {
                resp_params.insert("DefaultTime2Retain".to_string(), val.clone());
            }
            if let Some(val) = params.get("DataPDUInOrder") {
                resp_params.insert("DataPDUInOrder".to_string(), val.clone());
            }
            if let Some(val) = params.get("DataSequenceInOrder") {
                resp_params.insert("DataSequenceInOrder".to_string(), val.clone());
            }
            if params.contains_key("MaxRecvDataSegmentLength") {
                resp_params.insert("MaxRecvDataSegmentLength".to_string(), "1048576".to_string()); // 1MB
            }
            if let Some(val) = params.get("FirstBurstLength") {
                resp_params.insert("FirstBurstLength".to_string(), val.clone());
            }
            if let Some(val) = params.get("MaxBurstLength") {
                resp_params.insert("MaxBurstLength".to_string(), val.clone());
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

        // 2. Inisialisasi Cache jika ini sesi normal (bukan discovery)
        if !self.is_discovery {
            info!("Membuat cache writeback untuk IQN: {}", self.initiator_iqn);
            let cache = ClientCache::new(
                &self.cache_dir,
                &self.client_ip,
                self.backend.block_size(),
                self.max_cache_gb,
            )?;
            self.client_cache = Some(cache);
        }

        // 3. FFP Message Loop
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

        info!("Koneksi dengan client {} selesai.", peer_addr);
        Ok(())
    }

    async fn handle_nop_out(&mut self, req: Pdu) -> Result<(), std::io::Error> {
        let mut resp = Pdu::default();
        resp.opcode = OP_NOP_IN;
        resp.flags = 0x80;
        resp.initiator_task_tag = req.initiator_task_tag;
        resp.cmd_sn = self.stat_sn;
        self.stat_sn = self.stat_sn.wrapping_add(1);
        resp.exp_stat_sn = self.exp_cmd_sn;
        resp.max_cmd_sn = self.max_cmd_sn;

        let packet = pdu::builder::build_pdu(&resp);
        self.stream.write_all(&packet).await?;
        self.stream.flush().await?;
        Ok(())
    }

    async fn handle_logout(&mut self, req: Pdu) -> Result<(), std::io::Error> {
        info!("Client logout request diterima.");
        let mut resp = Pdu::default();
        resp.opcode = OP_LOGOUT_RESP;
        resp.flags = 0x80;
        resp.initiator_task_tag = req.initiator_task_tag;
        resp.cmd_sn = self.stat_sn;
        self.stat_sn = self.stat_sn.wrapping_add(1);
        resp.exp_stat_sn = self.exp_cmd_sn;
        resp.max_cmd_sn = self.max_cmd_sn;

        let packet = pdu::builder::build_pdu(&resp);
        self.stream.write_all(&packet).await?;
        self.stream.flush().await?;
        Ok(())
    }

    async fn handle_text_req(&mut self, req: Pdu) -> Result<(), std::io::Error> {
        let params = pdu::parser::parse_text_parameters(&req.data);
        info!("Menerima Text Request parameters: {:?}", params);
        let mut resp_params = HashMap::new();

        if params.get("SendTargets").map(|s| s.as_str()) == Some("All") {
            let local_addr = self.stream.local_addr()?;
            let ip = local_addr.ip();
            let port = local_addr.port();
            
            let target_address = format!("{}:{},1", ip, port);
            info!("Discovery mengembalikan target portal: iqn.2024-01.com.gameserver:gamedisk di {}", target_address);
            resp_params.insert("TargetName".to_string(), "iqn.2024-01.com.gameserver:gamedisk".to_string());
            resp_params.insert("TargetAddress".to_string(), target_address);
        }
        info!("Mengirim Text Response parameters: {:?}", resp_params);

        let mut resp = Pdu::default();
        resp.opcode = OP_TEXT_RESP;
        resp.flags = 0x80;
        resp.initiator_task_tag = req.initiator_task_tag;
        resp.cmd_sn = self.stat_sn;
        self.stat_sn = self.stat_sn.wrapping_add(1);
        resp.exp_stat_sn = self.exp_cmd_sn;
        resp.max_cmd_sn = self.max_cmd_sn;
        resp.data = pdu::builder::build_text_parameters(&resp_params);

        let packet = pdu::builder::build_pdu(&resp);
        self.stream.write_all(&packet).await?;
        self.stream.flush().await?;
        Ok(())
    }

    async fn handle_scsi_cmd(&mut self, req: Pdu) -> Result<(), std::io::Error> {
        let cdb = req.custom_bhs;
        let opcode = cdb[0];
        trace!("Menerima SCSI Command opcode: 0x{:02X}, CDB: {:02X?}", opcode, cdb);

        // Jika ini sesi normal, pastikan cache telah diinisialisasi
        if self.client_cache.is_none() && !self.is_discovery {
            warn!("Normal SCSI Command diterima tanpa inisialisasi cache!");
            self.send_scsi_check_condition(req.initiator_task_tag, 0x05, 0x25, 0x00).await?;
            return Ok(());
        }

        if opcode == 0x2A {
            // WRITE (10)
            let lba = u32::from_be_bytes(cdb[2..6].try_into().unwrap());
            trace!("Mulai WRITE (10) LBA: {}", lba);
            self.handle_write10(req).await?;
        } else {
            // Perintah SCSI lainnya disalurkan ke handler scsi.rs
            let result = scsi::handle_scsi_command(&cdb, &self.backend, self.client_cache.as_ref(), self.backend.block_size());
            match result {
                ScsiResult::Status { status } => {
                    trace!("SCSI Command 0x{:02X} selesai dengan status: 0x{:02X}", opcode, status);
                    self.send_scsi_response(req.initiator_task_tag, status, 0, req.expected_data_len, 0).await?;
                }
                ScsiResult::Data { data, status } => {
                    trace!("SCSI Command 0x{:02X} selesai dengan data len: {}, status: 0x{:02X}", opcode, data.len(), status);
                    self.send_scsi_data_in(req.initiator_task_tag, data, status, req.expected_data_len).await?;
                }
                ScsiResult::CheckCondition { key, asc, ascq } => {
                    warn!("SCSI Command 0x{:02X} gagal dengan CheckCondition: Key 0x{:02X}, ASC 0x{:02X}, ASCQ 0x{:02X}", opcode, key, asc, ascq);
                    self.send_scsi_check_condition(req.initiator_task_tag, key, asc, ascq).await?;
                }
            }
        }
        Ok(())
    }

    async fn handle_write10(&mut self, req: Pdu) -> Result<(), std::io::Error> {
        let cdb = req.custom_bhs;
        let lba = u32::from_be_bytes(cdb[2..6].try_into().unwrap()) as u64;
        let num_blocks = u16::from_be_bytes(cdb[7..9].try_into().unwrap()) as u32;
        let block_size = self.backend.block_size();
        let expected_len = (num_blocks as usize) * (block_size as usize);

        let mut bytes_received = 0;

        // Tulis immediate data (unsolicited) ke cache langsung
        let immediate_len = req.data.len();
        if immediate_len > 0 {
            if let Some(ref cache) = self.client_cache {
                cache.write_stream(lba, 0, &req.data)?;
                bytes_received = req.data.len();
            } else {
                error!("Client cache tidak tersedia untuk immediate write!");
                self.send_scsi_check_condition(req.initiator_task_tag, 0x05, 0x25, 0x00).await?;
                return Ok(());
            }
        }

        // Kalo masih kurang, kirim R2T
        if bytes_received < expected_len {
            let remaining = (expected_len - bytes_received) as u32;
            info!("WRITE10 LBA {} ({} blocks): kirim R2T offset={} desired={}", lba, num_blocks, bytes_received, remaining);
            self.send_r2t(
                req.initiator_task_tag,
                req.lun,
                bytes_received as u32,
                remaining,
            ).await?;
        }

        // Data-Out loop: tiap PDU langsung stream ke .bin
        while bytes_received < expected_len {
            let data_out = match pdu::parser::read_pdu(&mut self.stream).await {
                Ok(p) => p,
                Err(e) => {
                    error!("Gagal membaca Data-Out PDU: {}", e);
                    return Err(e);
                }
            };

            if data_out.opcode != OP_DATA_OUT {
                warn!("Non Data-Out opcode 0x{:02X} saat menanti write data", data_out.opcode);
                self.send_scsi_check_condition(req.initiator_task_tag, 0x05, 0x00, 0x00).await?;
                return Ok(());
            }

            let data_len = data_out.data.len();
            let buf_offset = data_out.exp_stat_sn as u64;
            let chunk = data_out.data;

            if let Some(ref cache) = self.client_cache {
                cache.write_stream(lba, buf_offset, &chunk)?;
            } else {
                self.send_scsi_check_condition(req.initiator_task_tag, 0x05, 0x25, 0x00).await?;
                return Ok(());
            }

            bytes_received += data_len;
        }

        info!("WRITE10 LBA {} sukses ({} bytes)", lba, expected_len);
        self.send_scsi_response(req.initiator_task_tag, 0x00, 0, 0, 0).await?;
        Ok(())
    }
    async fn send_r2t(
        &mut self,
        itt: u32,
        lun: u64,
        buffer_offset: u32,
        desired_len: u32,
    ) -> Result<(), std::io::Error> {
        let mut resp = Pdu::default();
        resp.opcode = OP_R2T;
        resp.flags = 0x80; // F (Final) — kita cuma support 1 R2T per task
        resp.lun = lun;
        resp.initiator_task_tag = itt;
        // R2T bukan status PDU — StatSN (cmd_sn) = 0, jangan increment!
        resp.cmd_sn = 0;
        resp.exp_stat_sn = self.exp_cmd_sn;
        resp.max_cmd_sn = self.max_cmd_sn;

        // custom_bhs[4..8]  = bytes 36-39: R2TSN
        resp.custom_bhs[4..8].copy_from_slice(&0u32.to_be_bytes());
        // custom_bhs[8..12]  = bytes 40-43: Buffer Offset
        resp.custom_bhs[8..12].copy_from_slice(&buffer_offset.to_be_bytes());
        // custom_bhs[12..16] = bytes 44-47: Desired Data Transfer Length
        resp.custom_bhs[12..16].copy_from_slice(&desired_len.to_be_bytes());

        let packet = pdu::builder::build_pdu(&resp);
        self.stream.write_all(&packet).await?;
        self.stream.flush().await?;
        Ok(())
    }

    async fn send_scsi_data_in(
        &mut self,
        itt: u32,
        data: Vec<u8>,
        status: u8,
        expected_len: u32,
    ) -> Result<(), std::io::Error> {
        let max_chunk = self.max_recv_data_segment_len;
        let total_len = data.len();
        let mut offset = 0;
        let mut data_sn = 0;

        while offset < total_len {
            let chunk_len = (total_len - offset).min(max_chunk);
            let is_last = offset + chunk_len >= total_len;

            // Build DATA_IN BHS on stack — zero alloc, zero copy
            let mut bhs = [0u8; 48];
            bhs[0] = OP_DATA_IN;
            if is_last {
                bhs[1] = 0x80; // F (Final)
            }
            bhs[16..20].copy_from_slice(&itt.to_be_bytes());
            bhs[20..24].copy_from_slice(&0xFFFFFFFFu32.to_be_bytes()); // TTT
            bhs[24..28].copy_from_slice(&0u32.to_be_bytes()); // StatSN = 0 (no S bit)
            bhs[28..32].copy_from_slice(&self.exp_cmd_sn.to_be_bytes());
            bhs[32..36].copy_from_slice(&self.max_cmd_sn.to_be_bytes());
            bhs[36..40].copy_from_slice(&(data_sn as u32).to_be_bytes()); // DataSN
            bhs[40..44].copy_from_slice(&(offset as u32).to_be_bytes()); // Buffer Offset
            // Data Segment Length (24-bit)
            bhs[5] = ((chunk_len >> 16) & 0xFF) as u8;
            bhs[6] = ((chunk_len >> 8) & 0xFF) as u8;
            bhs[7] = (chunk_len & 0xFF) as u8;

            let pad = (4 - (chunk_len % 4)) % 4;
            let total_data_in_len = 48 + chunk_len + pad;

            if is_last {
                // Build SCSI_RESPONSE BHS on stack
                let mut resp = [0u8; 48];
                resp[0] = OP_SCSI_RESP;
                let mut flags = 0x80; // F (Final)
                if total_len < expected_len as usize {
                    flags |= 0x04; // U (Underflow)
                }
                resp[1] = flags;
                resp[2] = 0x00; // Response: Command completed
                resp[3] = status;
                resp[16..20].copy_from_slice(&itt.to_be_bytes());
                resp[20..24].copy_from_slice(&0xFFFFFFFFu32.to_be_bytes()); // TTT
                resp[24..28].copy_from_slice(&self.stat_sn.to_be_bytes()); // StatSN
                self.stat_sn = self.stat_sn.wrapping_add(1);
                resp[28..32].copy_from_slice(&self.exp_cmd_sn.to_be_bytes());
                resp[32..36].copy_from_slice(&self.max_cmd_sn.to_be_bytes());
                resp[36..40].copy_from_slice(&(data_sn as u32).to_be_bytes()); // ExpDataSN
                if total_len < expected_len as usize {
                    let residual = expected_len as usize - total_len;
                    resp[44..48].copy_from_slice(&(residual as u32).to_be_bytes());
                }

                // Satu write: DATA_IN BHS + data + pad + RESP BHS
                let mut combined = Vec::with_capacity(total_data_in_len + 48);
                combined.extend_from_slice(&bhs);
                combined.extend_from_slice(&data[offset..offset + chunk_len]);
                if pad > 0 {
                    combined.extend_from_slice(&[0u8; 3][..pad]);
                }
                combined.extend_from_slice(&resp);
                self.stream.write_all(&combined).await?;
            } else {
                // Satu write: DATA_IN BHS + data + pad
                let mut combined = Vec::with_capacity(total_data_in_len);
                combined.extend_from_slice(&bhs);
                combined.extend_from_slice(&data[offset..offset + chunk_len]);
                if pad > 0 {
                    combined.extend_from_slice(&[0u8; 3][..pad]);
                }
                self.stream.write_all(&combined).await?;
            }

            offset += chunk_len;
            data_sn += 1;
        }

        self.stream.flush().await?;
        Ok(())
    }

    async fn send_scsi_response(
        &mut self,
        itt: u32,
        status: u8,
        exp_data_sn: u32,
        expected_len: u32,
        actual_len: u32,
    ) -> Result<(), std::io::Error> {
        let mut resp = Pdu::default();
        resp.opcode = OP_SCSI_RESP;
        
        let mut flags = 0x80; // F (Final)
        if actual_len < expected_len {
            flags |= 0x04; // U (Underflow)
        }
        resp.flags = flags;

        resp.opcode_specific[0] = 0x00; // Command completed at target
        resp.opcode_specific[1] = status;

        resp.initiator_task_tag = itt;
        resp.cmd_sn = self.stat_sn;
        self.stat_sn = self.stat_sn.wrapping_add(1);
        resp.exp_stat_sn = self.exp_cmd_sn;
        resp.max_cmd_sn = self.max_cmd_sn;

        // Tulis ExpDataSN ke custom_bhs[4..8] (yang dipetakan ke bytes 36-39)
        resp.custom_bhs[4..8].copy_from_slice(&exp_data_sn.to_be_bytes());

        // Tulis Residual Count ke custom_bhs[12..16] jika terjadi underflow
        if actual_len < expected_len {
            let residual = expected_len - actual_len;
            resp.custom_bhs[12..16].copy_from_slice(&residual.to_be_bytes());
        }

        let packet = pdu::builder::build_pdu(&resp);
        self.stream.write_all(&packet).await?;
        self.stream.flush().await?;
        Ok(())
    }

    async fn send_scsi_check_condition(
        &mut self,
        itt: u32,
        key: u8,
        asc: u8,
        ascq: u8,
    ) -> Result<(), std::io::Error> {
        let mut sense_data = vec![0u8; 18];
        sense_data[0] = 0x70;
        sense_data[2] = key;
        sense_data[7] = 0x0A;
        sense_data[12] = asc;
        sense_data[13] = ascq;

        let mut data = vec![0u8; 2 + 18];
        data[0..2].copy_from_slice(&(18u16).to_be_bytes());
        data[2..20].copy_from_slice(&sense_data);

        let mut resp = Pdu::default();
        resp.opcode = OP_SCSI_RESP;
        resp.flags = 0x80;

        resp.opcode_specific[0] = 0x00;
        resp.opcode_specific[1] = 0x02; // Check Condition

        resp.initiator_task_tag = itt;
        resp.cmd_sn = self.stat_sn;
        self.stat_sn = self.stat_sn.wrapping_add(1);
        resp.exp_stat_sn = self.exp_cmd_sn;
        resp.max_cmd_sn = self.max_cmd_sn;
        resp.data = data;

        let packet = pdu::builder::build_pdu(&resp);
        self.stream.write_all(&packet).await?;
        self.stream.flush().await?;
        Ok(())
    }
}
