use crate::pdu::{self, Pdu, OP_NOP_IN, OP_LOGOUT_RESP, OP_TEXT_RESP};
use crate::session::Session;
use tracing::{error, info, warn};
use tokio::io::AsyncWriteExt;
use std::time::Instant;

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

        let cache_opt = self.client_caches.get(&lun_id).cloned();
        if cache_opt.is_none() && !self.is_discovery && !self.is_imagedisk {
            warn!("Normal SCSI Command diterima untuk LUN {} tanpa inisialisasi cache!", lun_id);
            self.send_scsi_check_condition(req.initiator_task_tag, 0x05, 0x25, 0x00).await?;
            return Ok(());
        }

        // --- READ10/16: shared path ---
        if opcode == 0x28 || opcode == 0x88 {
            let (lba, num_blocks) = if opcode == 0x88 {
                (u64::from_be_bytes(cdb[2..10].try_into().unwrap()),
                 u32::from_be_bytes(cdb[10..14].try_into().unwrap()))
            } else {
                (u32::from_be_bytes(cdb[2..6].try_into().unwrap()) as u64,
                 u16::from_be_bytes(cdb[7..9].try_into().unwrap()) as u32)
            };
            let total_bytes = (num_blocks as usize) * (backend.block_size() as usize);
            let t0 = Instant::now();

            let mut buf = std::mem::take(&mut self.read_buf);
            if buf.len() < total_bytes {
                buf.resize(total_bytes, 0);
            }

            // All reads via spawn_blocking to avoid blocking tokio worker threads
            let backend = backend.clone();
            let handle = tokio::task::spawn_blocking(move || {
                backend.read_blocks(lba, num_blocks, &mut buf[..total_bytes])?;
                // Cache overlay (gamedisk only — imagedisk has no .bin cache)
                if let Some(cache) = cache_opt {
                    let _ = cache.read_partial_blocks(lba, num_blocks, &mut buf[..total_bytes]);
                }
                Ok::<Vec<u8>, std::io::Error>(buf)
            });
            match handle.await {
                Ok(Ok(returned_buf)) => {
                    self.read_buf = returned_buf;
                    
                    let ptr = self.read_buf.as_ptr();
                    let data = unsafe { std::slice::from_raw_parts(ptr, total_bytes) };
                    self.send_scsi_data_in(req.initiator_task_tag, data, 0x00, req.expected_data_len).await?;
                    
                    self.stats.bytes_read.fetch_add(total_bytes as u64, std::sync::atomic::Ordering::Relaxed);
                    info!("READ{} LUN={} LBA={}: {} blocks done in {}µs", 
                        if opcode == 0x88 { "16" } else { "10" }, lun_id, lba, num_blocks, t0.elapsed().as_micros());
                }
                Ok(Err(e)) => {
                    error!("Gagal membaca disk backend LUN {} untuk LBA {}: {}", lun_id, lba, e);
                    self.send_scsi_check_condition(req.initiator_task_tag, 0x03, 0x11, 0x00).await?;
                }
                Err(_) => {
                    error!("spawn_blocking panicked untuk LUN {} LBA {}", lun_id, lba);
                }
            }

        // --- WRITE10/16: dispatch to specialized handlers ---
        } else if opcode == 0x2A || opcode == 0x8A {
            let lba = if opcode == 0x8A {
                u64::from_be_bytes(cdb[2..10].try_into().unwrap())
            } else {
                u32::from_be_bytes(cdb[2..6].try_into().unwrap()) as u64
            };
            let num_blocks = if opcode == 0x8A {
                u32::from_be_bytes(cdb[10..14].try_into().unwrap())
            } else {
                u16::from_be_bytes(cdb[7..9].try_into().unwrap()) as u32
            };
            info!("WRITE LUN={} LBA={} blocks={}", lun_id, lba, num_blocks);

            if self.is_imagedisk {
                self.handle_imagedisk_write(&req, lun_id, lba, num_blocks).await?;
            } else {
                self.handle_gamedisk_write(&req, lun_id, lba, num_blocks).await?;
            }

        // --- Other SCSI commands: dispatch to specialized handlers ---
        } else if self.is_imagedisk {
            self.handle_imagedisk_scsi_cmd(&req, &cdb, opcode, lun_id).await?;
        } else {
            self.handle_gamedisk_scsi_cmd(&req, &cdb, opcode, lun_id).await?;
        }

        Ok(())
    }

    pub(super) async fn handle_data_out(&mut self, req: Pdu) -> Result<(), std::io::Error> {
        let itt = req.initiator_task_tag;
        
        let mut is_complete = false;
        
        if let Some(pending) = self.pending_writes.get_mut(&itt) {
            pending.buffer.extend_from_slice(&req.data);
            
            if pending.buffer.len() >= pending.expected_len {
                is_complete = true;
            }
        } else {
            warn!("Menerima Data-Out untuk task tag {} yang tidak ada di pending_writes", itt);
            return Ok(());
        }

        if is_complete {
            let pending = self.pending_writes.remove(&itt).unwrap();
            
            let backend = self.backends.get(&pending.lun_id).unwrap().clone();
            let cache_opt = self.client_caches.get(&pending.lun_id).cloned();
            let is_imagedisk = self.is_imagedisk;
            let pending_buffer = pending.buffer;
            let pending_lba = pending.lba;
            let pending_num_blocks = pending.num_blocks;

            let handle = tokio::task::spawn_blocking(move || {
                if is_imagedisk {
                    backend.write_blocks(pending_lba, pending_num_blocks, &pending_buffer)
                } else {
                    if let Some(cache) = cache_opt {
                        cache.write_stream(pending_lba, 0, &pending_buffer)
                    } else {
                        Ok(())
                    }
                }
            });

            match handle.await {
                Ok(Ok(())) => {
                    self.stats.bytes_written.fetch_add(pending.expected_len as u64, std::sync::atomic::Ordering::Relaxed);
                    self.send_scsi_response(itt, 0x00, 0, 0, 0).await?;
                }
                Ok(Err(e)) => {
                    error!("Gagal menulis blok pada LUN {}: {}", pending.lun_id, e);
                    self.send_scsi_response(itt, 0x02, 0x03, 0x0C, 0x00).await?;
                }
                Err(e) => {
                    error!("Task panic saat menulis: {}", e);
                    self.send_scsi_response(itt, 0x02, 0x03, 0x0C, 0x00).await?;
                }
            }
            
            info!("WRITE LUN {} LBA {} selesai via Data-Out ({} bytes)", pending.lun_id, pending.lba, pending.expected_len);
        }
        
        Ok(())
    }
}

