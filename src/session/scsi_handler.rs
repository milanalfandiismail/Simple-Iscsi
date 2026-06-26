use crate::pdu::{self, Pdu, OP_NOP_IN, OP_LOGOUT_RESP, OP_TEXT_RESP, OP_DATA_OUT};
use crate::scsi::ScsiResult;
use crate::session::Session;
use tracing::{debug, error, info, trace, warn};
use tokio::io::AsyncWriteExt;
use std::collections::HashMap;
use std::time::Instant;
use crate::scsi;

impl Session {
    pub(super) async fn handle_nop_out(&mut self, req: Pdu) -> Result<(), std::io::Error> {
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

    pub(super) async fn handle_logout(&mut self, req: Pdu) -> Result<(), std::io::Error> {
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

    pub(super) async fn handle_text_req(&mut self, req: Pdu) -> Result<(), std::io::Error> {
        let params = pdu::parser::parse_text_parameters(&req.data);
        info!("Menerima Text Request parameters: {:?}", params);
        let mut resp_params = HashMap::new();

        if params.get("SendTargets").map(|s| s.as_str()) == Some("All") {
            let local_addr = self.stream.local_addr()?;
            let ip = local_addr.ip();
            let port = local_addr.port();
            
            let target_address = format!("{}:{},1", ip, port);
            info!("Discovery mengembalikan target portal: {} di {}", self.target_iqn, target_address);
            resp_params.insert("TargetName".to_string(), self.target_iqn.clone());
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

    pub(super) async fn handle_scsi_cmd(&mut self, req: Pdu) -> Result<(), std::io::Error> {
        let cdb = req.custom_bhs;
        let opcode = cdb[0];
        trace!("Menerima SCSI Command opcode: 0x{:02X}, CDB: {:02X?}", opcode, cdb);

        // Jika ini sesi normal, pastikan cache telah diinisialisasi
        if self.client_cache.is_none() && !self.is_discovery {
            warn!("Normal SCSI Command diterima tanpa inisialisasi cache!");
            self.send_scsi_check_condition(req.initiator_task_tag, 0x05, 0x25, 0x00).await?;
            return Ok(());
        }

        if opcode == 0x28 {
            // READ (10) — inline untuk menghindari Vec alloc
            let lba = u32::from_be_bytes(cdb[2..6].try_into().unwrap()) as u64;
            let num_blocks = u16::from_be_bytes(cdb[7..9].try_into().unwrap()) as u32;
            let block_size = self.backend.block_size();
            let total_bytes = (num_blocks as u64 * block_size) as usize;

            let t0 = Instant::now();

            // Grow reusable buffer jika perlu (amortized O(1))
            if self.read_buf.len() < total_bytes {
                self.read_buf.resize(total_bytes, 0);
            }

            if let Err(e) = self.backend.read_blocks(lba, num_blocks, &mut self.read_buf[..total_bytes]) {
                error!("Gagal membaca disk backend untuk LBA {}: {}", lba, e);
                self.send_scsi_check_condition(req.initiator_task_tag, 0x03, 0x11, 0x00).await?;
                return Ok(());
            }

            // Overlay cache jika perlu
            if let Some(ref cache) = self.client_cache {
                if cache.contains_range(lba, num_blocks) {
                    if let Some(Ok(())) = cache.read_blocks(lba, num_blocks, &mut self.read_buf[..total_bytes]) {
                        // data sudah dikoreksi dari cache
                    }
                }
            }

            // SAFETY: read_buf unused during send_scsi_data_in (it only writes to TCP stream).
            // Raw pointer avoids simultaneous &self + &mut self borrow conflict.
            let ptr = self.read_buf.as_ptr();
            let data = unsafe { std::slice::from_raw_parts(ptr, total_bytes) };
            self.send_scsi_data_in(req.initiator_task_tag, data, 0x00, req.expected_data_len).await?;
            debug!("READ10 LBA={}: send_data_in {}µs", lba, t0.elapsed().as_micros());
        } else if opcode == 0x2A {
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
                    let is_read10 = opcode == 0x28;
                    let t0 = if is_read10 { Some(Instant::now()) } else { None };
                    trace!("SCSI Command 0x{:02X} selesai dengan data len: {}, status: 0x{:02X}", opcode, data.len(), status);
                    self.send_scsi_data_in(req.initiator_task_tag, &data, status, req.expected_data_len).await?;
                    if let Some(timer) = t0 {
                        let elapsed = timer.elapsed();
                        let lba = u32::from_be_bytes(cdb[2..6].try_into().unwrap());
                        debug!("READ10 LBA={}: send_data_in {}µs", lba, elapsed.as_micros());
                    }
                }
                ScsiResult::CheckCondition { key, asc, ascq } => {
                    warn!("SCSI Command 0x{:02X} gagal dengan CheckCondition: Key 0x{:02X}, ASC 0x{:02X}, ASCQ 0x{:02X}", opcode, key, asc, ascq);
                    self.send_scsi_check_condition(req.initiator_task_tag, key, asc, ascq).await?;
                }
            }
        }
        Ok(())
    }

    pub(super) async fn handle_write10(&mut self, req: Pdu) -> Result<(), std::io::Error> {
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
}
