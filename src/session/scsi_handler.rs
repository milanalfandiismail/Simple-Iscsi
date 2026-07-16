use crate::pdu::{self, Pdu, OP_NOP_IN, OP_LOGOUT_RESP, OP_TEXT_RESP};
use crate::session::Session;
use tracing::{error, info, warn, trace};
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

    pub(super) async fn handle_scsi_cmd_pipelined(
        &mut self,
        req: Pdu,
        tx: tokio::sync::mpsc::Sender<super::DiskOpResult>,
    ) -> Result<(), std::io::Error> {
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

            tokio::spawn(async move {
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
                    // Cache miss: panggil thread pool blocking langsung
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

                let _ = tx.send(super::DiskOpResult {
                    itt,
                    opcode,
                    req,
                    result: res,
                }).await;
            });

        // --- SYNCHRONIZE CACHE (10/16) ---
        } else if opcode == 0x35 || opcode == 0x91 {
            let cache_opt = cache_opt.clone();
            let backend_clone = backend.clone();
            let itt = req.initiator_task_tag;
            let req_clone = req.clone();

            tokio::spawn(async move {
                let res = tokio::task::spawn_blocking(move || {
                    if let Some(cache) = cache_opt {
                        cache.flush()
                    } else {
                        backend_clone.sync()
                    }
                }).await.unwrap();

                let _ = tx.send(super::DiskOpResult {
                    itt,
                    opcode,
                    req: req_clone,
                    result: res.map(|_| Vec::new()),
                }).await;
            });

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
                self.handle_imagedisk_write_pipelined(&req, lun_id, lba, num_blocks, tx).await?;
            } else {
                self.handle_gamedisk_write_pipelined(&req, lun_id, lba, num_blocks, tx).await?;
            }

        // --- Other SCSI commands: dispatch to specialized handlers ---
        } else if self.is_imagedisk {
            self.handle_imagedisk_scsi_cmd(&req, &cdb, opcode, lun_id).await?;
        } else {
            self.handle_gamedisk_scsi_cmd(&req, &cdb, opcode, lun_id).await?;
        }

        Ok(())
    }

    pub(super) async fn handle_data_out_pipelined(
        &mut self,
        req: Pdu,
        tx: tokio::sync::mpsc::Sender<super::DiskOpResult>,
    ) -> Result<(), std::io::Error> {
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
            let is_imagedisk = self.is_imagedisk;
            let pending_buffer = pending.buffer;
            let pending_lba = pending.lba;
            let pending_num_blocks = pending.num_blocks;
            let expected_len = pending.expected_len;
            let lun_id = pending.lun_id;
            let req_clone = req.clone();

            tokio::spawn(async move {
                if !is_imagedisk {
                    if let Some(ref cache) = cache_opt {
                        cache.throttle_write_async(pending_buffer.len()).await;
                    }
                }

                let res = tokio::task::spawn_blocking(move || {
                    if is_imagedisk {
                        backend_clone.write_blocks(pending_lba, pending_num_blocks, &pending_buffer)
                    } else {
                        if let Some(cache) = cache_opt {
                            cache.write_stream(pending_lba, 0, &pending_buffer)
                        } else {
                            Ok(())
                        }
                    }
                }).await.unwrap();

                if res.is_ok() {
                    trace!("WRITE LUN {} LBA {} selesai via Data-Out ({} bytes)", lun_id, pending_lba, expected_len);
                }
                let _ = tx.send(super::DiskOpResult {
                    itt,
                    opcode: 0x2A,
                    req: req_clone,
                    result: res.map(|_| Vec::new()),
                }).await;
            });
            
            self.stats.bytes_written.fetch_add(expected_len as u64, std::sync::atomic::Ordering::Relaxed);
        }
        
        Ok(())
    }
}

