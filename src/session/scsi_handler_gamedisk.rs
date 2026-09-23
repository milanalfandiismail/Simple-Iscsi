use crate::pdu::Pdu;
use crate::scsi_gamedisk;
use crate::session::SessionContext;
use tracing::trace;

impl SessionContext {
    /// Execute gamedisk SCSI command dispatch (non-READ, non-WRITE)
    pub(super) async fn handle_gamedisk_scsi_cmd(
        &self,
        req: &Pdu,
        cdb: &[u8],
        _opcode: u8,
        lun_id: u8,
    ) -> Result<(), std::io::Error> {
        let backend = self.backends.get(&lun_id).unwrap();
        let cache_opt = self.client_caches.get(&lun_id).map(|c| &**c);
        let active_luns: Vec<u8> = self.backends.keys().cloned().collect();
        let result = scsi_gamedisk::handle_scsi_command(
            cdb, backend.as_ref(), cache_opt, backend.block_size(), &active_luns, lun_id,
        );
        self.send_scsi_result(req, result).await
    }

    pub(super) async fn handle_gamedisk_write(
        &self,
        req: &Pdu,
        lun_id: u8,
        lba: u64,
        num_blocks: u32,
    ) -> Result<(), std::io::Error> {
        let backend = self.backends.get(&lun_id).cloned().unwrap();
        let block_size = backend.block_size();
        let expected_len = req.expected_data_len as usize;
        let calculated_len = (num_blocks as usize) * (block_size as usize);
        trace!("SCSI_WRITE (gamedisk) ITT {}: expected_data_len={}, calculated_len={}, num_blocks={}, immediate_len={}",
              req.initiator_task_tag, expected_len, calculated_len, num_blocks, req.data.len());

        let mut write_buf = vec![0u8; expected_len];
        let mut bytes_received = 0;

        let immediate_len = req.data.len();
        if immediate_len > 0 {
            let copy_len = immediate_len.min(expected_len);
            write_buf[..copy_len].copy_from_slice(&req.data[..copy_len]);
            bytes_received = copy_len;
        }

        if bytes_received < expected_len {
            let remaining = (expected_len - bytes_received) as u32;
            trace!("WRITE (gamedisk) LUN {} LBA {} ({} blocks): R2T offset={} desired={}",
                lun_id, lba, num_blocks, bytes_received, remaining);
            
            {
                let mut pending_guard = self.pending_writes.lock();
                pending_guard.insert(req.initiator_task_tag, crate::session::PendingWrite {
                    lun_id,
                    lba,
                    num_blocks,
                    expected_len,
                    buffer: write_buf,
                    data_sn_count: 0,
                });
            }
            
            self.send_r2t(req.initiator_task_tag, req.lun, bytes_received as u32, remaining).await?;
            return Ok(());
        }

        let itt = req.initiator_task_tag;
        let cache_opt = self.client_caches.get(&lun_id).cloned();
        let backend = self.backends.get(&lun_id).cloned().unwrap();
        let tx = self.tx.clone();
        let stats = std::sync::Arc::clone(&self.stats);
        let client_ip = self.client_ip.clone();

        tokio::spawn(async move {
            let res = tokio::task::spawn_blocking(move || {
                if let Some(cache) = cache_opt {
                    cache.write_stream(lba, 0, &write_buf)
                } else {
                    backend.write_blocks(lba, num_blocks, &write_buf)
                }
            }).await;

            match res {
                Ok(Ok(_)) => {
                    let _ = tx.send(crate::session::WriterMessage::ScsiResponse {
                        itt,
                        status: 0x00,
                        exp_data_sn: 0,
                        expected_len: 0,
                        actual_len: 0,
                    }).await;
                    stats.record_write(&client_ip, expected_len as u64);
                }
                Ok(Err(e)) => {
                    tracing::error!("Gagal menulis data ke disk LUN {} LBA {}: {}", lun_id, lba, e);
                    let _ = tx.send(crate::session::WriterMessage::CheckCondition {
                        itt,
                        key: 0x03,
                        asc: 0x0C,
                        ascq: 0x00,
                    }).await;
                }
                Err(e) => {
                    tracing::error!("Disk write task panicked: {}", e);
                }
            }
        });
        Ok(())
    }
}
