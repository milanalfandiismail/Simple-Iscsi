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

        // --- READ10/16: shared path ---
        if opcode == 0x28 || opcode == 0x88 {
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

            // Large reads (>1MB) via spawn_blocking to avoid blocking tokio worker
            if total_bytes > 1024 * 1024 {
                let backend = backend.clone();
                let handle = tokio::task::spawn_blocking(move || {
                    let mut buf = vec![0u8; total_bytes];
                    backend.read_blocks(lba, num_blocks, &mut buf).map(|_| buf)
                });
                match handle.await {
                    Ok(Ok(data)) => {
                        self.read_buf[..total_bytes].copy_from_slice(&data);
                    }
                    Ok(Err(e)) => {
                        error!("Gagal membaca disk backend LUN {} untuk LBA {}: {}", lun_id, lba, e);
                        self.send_scsi_check_condition(req.initiator_task_tag, 0x03, 0x11, 0x00).await?;
                        return Ok(());
                    }
                    Err(_) => {
                        error!("spawn_blocking panicked untuk LUN {} LBA {}", lun_id, lba);
                        return Ok(());
                    }
                }
            } else {
                if let Err(e) = backend.read_blocks(lba, num_blocks, &mut self.read_buf[..total_bytes]) {
                    error!("Gagal membaca disk backend LUN {} untuk LBA {}: {}", lun_id, lba, e);
                    self.send_scsi_check_condition(req.initiator_task_tag, 0x03, 0x11, 0x00).await?;
                    return Ok(());
                }
            }

            // Cache overlay (gamedisk only — imagedisk has no .bin cache)
            if let Some(cache) = cache_opt {
                if cache.contains_range(lba, num_blocks) {
                    self.stats.cache_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if let Some(Ok(())) = cache.read_blocks(lba, num_blocks, &mut self.read_buf[..total_bytes]) {
                    }
                } else {
                    self.stats.cache_misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }

            let ptr = self.read_buf.as_ptr();
            let data = unsafe { std::slice::from_raw_parts(ptr, total_bytes) };
            self.send_scsi_data_in(req.initiator_task_tag, data, 0x00, req.expected_data_len).await?;
            self.stats.bytes_read.fetch_add(total_bytes as u64, std::sync::atomic::Ordering::Relaxed);
            info!("READ{} LUN={} LBA={}: {} blocks done in {}µs", 
                if opcode == 0x88 { "16" } else { "10" }, lun_id, lba, num_blocks, t0.elapsed().as_micros());

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
}
