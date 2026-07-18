use crate::pdu::Pdu;
use crate::scsi_imagedisk;
use crate::session::Session;
use tracing::{error, info};

impl Session {
    /// Execute imagedisk SCSI command dispatch (intercepts Windows-specific commands)
    pub(super) async fn handle_imagedisk_scsi_cmd(
        &mut self,
        req: &Pdu,
        cdb: &[u8],
        _opcode: u8,
        lun_id: u8,
    ) -> Result<(), std::io::Error> {
        let backend = self.backends.get(&lun_id).unwrap();
        let cache_opt = self.client_caches.get(&lun_id).map(|c| &**c);
        let active_luns: Vec<u8> = self.backends.keys().cloned().collect();
        let result = scsi_imagedisk::handle_imagedisk_scsi(
            cdb, backend.as_ref(), cache_opt, backend.block_size(), &active_luns, lun_id,
        );
        self.send_scsi_result(req, result).await
    }



    pub(super) async fn handle_imagedisk_write(
        &mut self,
        req: &Pdu,
        lun_id: u8,
        lba: u64,
        num_blocks: u32,
    ) -> Result<(), std::io::Error> {
        let backend = self.backends.get(&lun_id).cloned().unwrap();
        let block_size = backend.block_size();
        let expected_len = (num_blocks as usize) * (block_size as usize);

        let mut write_buf: Vec<u8> = Vec::with_capacity(expected_len);
        let mut bytes_received = 0;

        let immediate_len = req.data.len();
        if immediate_len > 0 {
            write_buf.extend_from_slice(&req.data);
            bytes_received = immediate_len;
        }

        if bytes_received < expected_len {
            let remaining = (expected_len - bytes_received) as u32;
            info!("WRITE (imagedisk) LUN {} LBA {} ({} blocks): R2T offset={} desired={}",
                lun_id, lba, num_blocks, bytes_received, remaining);
            
            self.pending_writes.insert(req.initiator_task_tag, crate::session::PendingWrite {
                lun_id,
                lba,
                num_blocks,
                expected_len,
                buffer: write_buf,
            });
            
            self.send_r2t(req.initiator_task_tag, req.lun, bytes_received as u32, remaining).await?;
            return Ok(());
        }

        let backend_clone = backend.clone();
        let cache_opt = self.client_caches.get(&lun_id).cloned();
        let write_buf_clone = write_buf;
        let itt = req.initiator_task_tag;

        self.throttle_write(expected_len).await;

        let res = tokio::task::spawn_blocking(move || {
            if let Some(cache) = cache_opt {
                cache.write_stream(lba, 0, &write_buf_clone)
            } else {
                backend_clone.write_blocks(lba, num_blocks, &write_buf_clone)
            }
        }).await.unwrap();

        match res {
            Ok(_) => {
                self.send_scsi_response(itt, 0x00, 0, 0, 0).await?;
                self.stats.record_write(&self.client_ip, expected_len as u64);
            }
            Err(e) => {
                error!("Gagal menulis disk LUN {} LBA {}: {}", lun_id, lba, e);
                self.send_scsi_response(itt, 0x02, 0x03, 0x0C, 0x00).await?;
            }
        }

        Ok(())
    }
}
