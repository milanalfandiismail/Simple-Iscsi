use crate::pdu::{self, Pdu, OP_NOP_IN, OP_LOGOUT_RESP, OP_TEXT_RESP, OP_DATA_OUT};
use crate::scsi::ScsiResult;
use crate::session::Session;
use tracing::{debug, error, info, trace, warn};
use tokio::io::AsyncWriteExt;
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
        let mut resp_params = Vec::new();

        if params.get("SendTargets").map(|s| s.as_str()) == Some("All") {
            let local_addr = self.stream.local_addr()?;
            let ip = local_addr.ip();
            let port = local_addr.port();
            
            let target_address = format!("{}:{},1", ip, port);
            
            if self.config.gamedisk_target.discovery {
                info!("Discovery mengembalikan target portal: {} di {}", self.config.gamedisk_target.target_iqn, target_address);
                resp_params.push(("TargetName".to_string(), self.config.gamedisk_target.target_iqn.clone()));
                resp_params.push(("TargetAddress".to_string(), target_address.clone()));
            }
            
            if self.config.windows.discovery {
                // If we had a list of specific VHDs we could list them, but for now just list the prefix
                // Since Windows usually connects directly to a specific target_iqn for boot,
                // discovery for Windows targets might not be needed, but if enabled we can return a generic one or skip.
                // Usually gamedisk is the main one discovered.
            }
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
        let lun_id = ((req.lun >> 48) & 0xFF) as u8;
        info!("SCSI opcode=0x{:02X} LUN={} is_imagedisk={}", opcode, lun_id, self.is_imagedisk);

        let backend = match self.backends.get(&lun_id) {
            Some(b) => b,
            None => {
                warn!("SCSI Command untuk LUN {} yang tidak ada. Mengirim Check Condition.", lun_id);
                self.send_scsi_check_condition(req.initiator_task_tag, 0x05, 0x25, 0x00).await?;
                return Ok(());
            }
        };

        let cache_opt = self.client_caches.get(&lun_id);
        if cache_opt.is_none() && !self.is_discovery && !self.is_imagedisk {
            warn!("Normal SCSI Command diterima untuk LUN {} tanpa inisialisasi cache!", lun_id);
            self.send_scsi_check_condition(req.initiator_task_tag, 0x05, 0x25, 0x00).await?;
            return Ok(());
        }

        if opcode == 0x28 || opcode == 0x88 {
            // READ (10) or READ (16)
            let (lba, num_blocks) = if opcode == 0x88 {
                (u64::from_be_bytes(cdb[2..10].try_into().unwrap()),
                 u32::from_be_bytes(cdb[10..14].try_into().unwrap()))
            } else {
                (u32::from_be_bytes(cdb[2..6].try_into().unwrap()) as u64,
                 u16::from_be_bytes(cdb[7..9].try_into().unwrap()) as u32)
            };
            let block_size = backend.block_size();
            let total_bytes = (num_blocks as u64 * block_size) as usize;

            let t0 = Instant::now();

            if self.read_buf.len() < total_bytes {
                self.read_buf.resize(total_bytes, 0);
            }

            if let Err(e) = backend.read_blocks(lba, num_blocks, &mut self.read_buf[..total_bytes]) {
                error!("Gagal membaca disk backend LUN {} untuk LBA {}: {}", lun_id, lba, e);
                self.send_scsi_check_condition(req.initiator_task_tag, 0x03, 0x11, 0x00).await?;
                return Ok(());
            }

            if let Some(cache) = cache_opt {
                if cache.contains_range(lba, num_blocks) {
                    if let Some(Ok(())) = cache.read_blocks(lba, num_blocks, &mut self.read_buf[..total_bytes]) {
                    }
                }
            }

            let ptr = self.read_buf.as_ptr();
            let data = unsafe { std::slice::from_raw_parts(ptr, total_bytes) };
            self.send_scsi_data_in(req.initiator_task_tag, data, 0x00, req.expected_data_len).await?;
            info!("READ{} LUN={} LBA={}: {} blocks done in {}µs", 
                if opcode == 0x88 { "16" } else { "10" }, lun_id, lba, num_blocks, t0.elapsed().as_micros());
        } else if opcode == 0x2A || opcode == 0x8A {
            // WRITE (10) or WRITE (16)
            let lba = if opcode == 0x8A {
                u64::from_be_bytes(cdb[2..10].try_into().unwrap())
            } else {
                u32::from_be_bytes(cdb[2..6].try_into().unwrap()) as u64
            };
            info!("WRITE LUN={} LBA={}", lun_id, lba);
            self.handle_write10(req, lun_id).await?;
        } else if self.is_imagedisk {
            // ImageDisk path → use Windows-compatible SCSI handler
            let active_luns: Vec<u8> = self.backends.keys().cloned().collect();
            let result = crate::session::scsi_image::handle_imagedisk_scsi(
                &cdb, backend.as_ref(), cache_opt, backend.block_size(), &active_luns);
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
        } else {
            // GameDisk path → use existing lightweight SCSI handler
            let active_luns: Vec<u8> = self.backends.keys().cloned().collect();
            let result = scsi::handle_scsi_command(&cdb, backend.as_ref(), cache_opt, backend.block_size(), &active_luns);
            match result {
                ScsiResult::Status { status } => {
                    trace!("SCSI Command 0x{:02X} selesai dengan status: 0x{:02X}", opcode, status);
                    self.send_scsi_response(req.initiator_task_tag, status, 0, req.expected_data_len, 0).await?;
                }
                ScsiResult::Data { data, status } => {
                    trace!("SCSI Command 0x{:02X} selesai dengan data len: {}, status: 0x{:02X}", opcode, data.len(), status);
                    self.send_scsi_data_in(req.initiator_task_tag, &data, status, req.expected_data_len).await?;
                }
                ScsiResult::CheckCondition { key, asc, ascq } => {
                    warn!("SCSI Command 0x{:02X} gagal dengan CheckCondition: Key 0x{:02X}, ASC 0x{:02X}, ASCQ 0x{:02X}", opcode, key, asc, ascq);
                    self.send_scsi_check_condition(req.initiator_task_tag, key, asc, ascq).await?;
                }
            }
        }
        Ok(())
    }

    pub(super) async fn handle_write10(&mut self, req: Pdu, lun_id: u8) -> Result<(), std::io::Error> {
        let cdb = req.custom_bhs;
        let opcode = cdb[0];
        let (lba, num_blocks) = if opcode == 0x8A {
            // WRITE(16): 64-bit LBA, 32-bit transfer length
            (u64::from_be_bytes(cdb[2..10].try_into().unwrap()),
             u32::from_be_bytes(cdb[10..14].try_into().unwrap()))
        } else {
            // WRITE(10): 32-bit LBA, 16-bit transfer length
            (u32::from_be_bytes(cdb[2..6].try_into().unwrap()) as u64,
             u16::from_be_bytes(cdb[7..9].try_into().unwrap()) as u32)
        };
        
        let backend = self.backends.get(&lun_id).unwrap();
        let block_size = backend.block_size();
        let expected_len = (num_blocks as usize) * (block_size as usize);

        // Buffer semua data dulu — baru satu write_stream di akhir
        let mut write_buf: Vec<u8> = Vec::with_capacity(expected_len);
        let mut bytes_received = 0;

        // Tampung immediate data (unsolicited)
        let immediate_len = req.data.len();
        if immediate_len > 0 {
            write_buf.extend_from_slice(&req.data);
            bytes_received = immediate_len;
        }

        // Kalo masih kurang, kirim R2T
        if bytes_received < expected_len {
            let remaining = (expected_len - bytes_received) as u32;
            info!("WRITE10 LUN {} LBA {} ({} blocks): kirim R2T offset={} desired={}", lun_id, lba, num_blocks, bytes_received, remaining);
            self.send_r2t(
                req.initiator_task_tag,
                req.lun,
                bytes_received as u32,
                remaining,
            ).await?;
        }

        // Data-Out loop: buffer semua PDU ke write_buf
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

            write_buf.extend_from_slice(&data_out.data);
            bytes_received += data_out.data.len();
        }

        // Satu kali write — untuk imagedisk: langsung ke backend, untuk gamedisk: ke cache
        if self.is_imagedisk {
            // Write langsung ke differencing VHD backend
            let backend = self.backends.get(&lun_id).unwrap();
            backend.write_blocks(lba, num_blocks, &write_buf)?;
            info!("WRITE10 (ImageDisk) LUN {} LBA {} sukses ({} bytes) → child VHD", lun_id, lba, expected_len);
        } else if let Some(cache) = self.client_caches.get(&lun_id) {
            cache.write_stream(lba, 0, &write_buf)?;
        } else {
            self.send_scsi_check_condition(req.initiator_task_tag, 0x05, 0x25, 0x00).await?;
            return Ok(());
        }

        info!("WRITE10 LUN {} LBA {} sukses ({} bytes)", lun_id, lba, expected_len);
        self.send_scsi_response(req.initiator_task_tag, 0x00, 0, 0, 0).await?;
        Ok(())
    }
}
