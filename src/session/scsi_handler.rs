use crate::pdu::{self, Pdu, OP_NOP_IN, OP_LOGOUT_RESP, OP_TEXT_RESP, OP_TMF_RESP};
use crate::session::Session;
use tracing::{error, info, warn, trace};

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
        self.send_packet(packet).await
    }

    pub(super) async fn handle_tmf_req(&mut self, req: Pdu) -> Result<(), std::io::Error> {
        info!("Menerima TMF Request. ITT: {}, Function: 0x{:02X}", req.initiator_task_tag, req.flags & 0x7F);

        let mut resp = Pdu::default();
        resp.opcode = OP_TMF_RESP;
        resp.flags = 0x80; // F (Final)
        resp.initiator_task_tag = req.initiator_task_tag;
        
        // StatSN dialokasikan dan diincrement untuk target-response
        resp.cmd_sn = self.stat_sn;
        self.stat_sn = self.stat_sn.wrapping_add(1);
        
        resp.exp_stat_sn = self.exp_cmd_sn;
        resp.max_cmd_sn = self.max_cmd_sn;

        // Byte 2: Response. 0x00 = Function Complete
        resp.opcode_specific[0] = 0x00;

        let packet = pdu::builder::build_pdu(&resp);
        self.send_packet(packet).await
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
        self.send_packet(packet).await
    }

    pub(super) async fn handle_text_req(&mut self, req: Pdu) -> Result<(), std::io::Error> {
        let params = pdu::parser::parse_text_parameters(&req.data);
        info!("Menerima Text Request parameters: {:?}", params);

        let mut resp_params = Vec::new();

        if params.get("SendTargets").map(|s| s.as_str()) == Some("All") {
            let local_addr = self.local_addr;
            let ip = local_addr.ip();
            let port = local_addr.port();
            
            let target_address = format!("{}:{},1", ip, port);
            
            let config_guard = self.config.read();
            if config_guard.gamedisk_target.discovery {
                info!("Discovery mengembalikan target portal: {} di {}", config_guard.gamedisk_target.target_iqn, target_address);
                resp_params.push(("TargetName".to_string(), config_guard.gamedisk_target.target_iqn.clone()));
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
        self.send_packet(packet).await
    }

    pub(super) async fn handle_scsi_cmd(&mut self, req: Pdu) -> Result<(), std::io::Error> {
        let cdb = req.custom_bhs;
        let opcode = cdb[0];
        let lun_id = ((req.lun >> 48) & 0xFF) as u8;
        trace!("SCSI opcode=0x{:02X} LUN={} is_imagedisk={}", opcode, lun_id, self.is_imagedisk);

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
            let backend_clone = backend.clone();
            let itt = req.initiator_task_tag;

            let mut buf = vec![0u8; total_bytes];

            // Fast-Path: Cek apakah ada di RAM Cache gamedisk langsung (Zero spawn_blocking)
            let cache_hit = {
                let mut hit = false;
                if let Some(ref cache) = cache_opt {
                    let mut in_writeback = false;
                    for i in 0..num_blocks {
                        if cache.contains_lba(lba + i as u64) {
                            in_writeback = true;
                            break;
                        }
                    }
                    if !in_writeback {
                        hit = backend_clone.try_read_from_cache(lba, num_blocks, &mut buf).is_some();
                    }
                } else {
                    hit = backend_clone.try_read_from_cache(lba, num_blocks, &mut buf).is_some();
                }
                hit
            };

            let res = if cache_hit {
                Ok(buf)
            } else {
                tokio::task::spawn_blocking(move || {
                    let mut buf = vec![0u8; total_bytes];
                    if let Some(cache) = cache_opt {
                        cache.read_blocks_cached(&backend_clone, lba, num_blocks, &mut buf)?;
                    } else {
                        backend_clone.read_blocks(lba, num_blocks, &mut buf)?;
                    }
                    Ok::<Vec<u8>, std::io::Error>(buf)
                }).await.unwrap()
            };

            match res {
                Ok(buf) => {
                    self.send_scsi_data_in(itt, &buf, 0x00, req.expected_data_len).await?;
                    self.stats.bytes_read.fetch_add(buf.len() as u64, std::sync::atomic::Ordering::Relaxed);
                }
                Err(e) => {
                    error!("Gagal membaca disk LUN {} LBA {}: {}", req.lun, itt, e);
                    self.send_scsi_check_condition(itt, 0x03, 0x11, 0x00).await?;
                }
            }

        // --- SYNCHRONIZE CACHE (10/16) ---
        } else if opcode == 0x35 || opcode == 0x91 {
            let cache_opt = cache_opt.clone();
            let backend_clone = backend.clone();
            let itt = req.initiator_task_tag;

            let res = tokio::task::spawn_blocking(move || {
                if let Some(cache) = cache_opt {
                    cache.flush()
                } else {
                    backend_clone.sync()
                }
            }).await.unwrap();

            match res {
                Ok(_) => {
                    self.send_scsi_response(itt, 0x00, 0, 0, 0).await?;
                }
                Err(e) => {
                    error!("Gagal melakukan sinkronisasi cache LUN {}: {}", req.lun, e);
                    self.send_scsi_check_condition(itt, 0x03, 0x0C, 0x00).await?;
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
            trace!("WRITE LUN={} LBA={} blocks={}", lun_id, lba, num_blocks);

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
            let backend_clone = self.backends.get(&pending.lun_id).unwrap().clone();
            let cache_opt = self.client_caches.get(&pending.lun_id).cloned();
            let pending_buffer = pending.buffer;
            let pending_lba = pending.lba;
            let pending_num_blocks = pending.num_blocks;
            let expected_len = pending.expected_len;

            self.throttle_write(pending_buffer.len()).await;

            let res = tokio::task::spawn_blocking(move || {
                if let Some(cache) = cache_opt {
                    cache.write_stream(pending_lba, 0, &pending_buffer)
                } else {
                    backend_clone.write_blocks(pending_lba, pending_num_blocks, &pending_buffer)
                }
            }).await.unwrap();

            match res {
                Ok(_) => {
                    self.send_scsi_response(itt, 0x00, 0, 0, 0).await?;
                    self.stats.bytes_written.fetch_add(expected_len as u64, std::sync::atomic::Ordering::Relaxed);
                }
                Err(e) => {
                    error!("Gagal menulis disk LUN {} LBA {}: {}", pending.lun_id, pending_lba, e);
                    self.send_scsi_response(itt, 0x02, 0x03, 0x0C, 0x00).await?;
                }
            }
        }
        
        Ok(())
    }
}

