use crate::pdu::{self, Pdu};
use crate::scsi_gamedisk;
use crate::session::Session;
use tracing::{error, info, warn};

impl Session {
    /// Execute gamedisk SCSI command dispatch (non-READ, non-WRITE)
    pub(super) async fn handle_gamedisk_scsi_cmd(
        &mut self,
        req: &Pdu,
        cdb: &[u8],
        _opcode: u8,
        lun_id: u8,
    ) -> Result<(), std::io::Error> {
        let backend = self.backends.get(&lun_id).unwrap();
        let cache_opt = self.client_caches.get(&lun_id);
        let active_luns: Vec<u8> = self.backends.keys().cloned().collect();
        let result = scsi_gamedisk::handle_scsi_command(
            cdb, backend.as_ref(), cache_opt, backend.block_size(), &active_luns, lun_id,
        );
        self.send_scsi_result(req, result).await
    }

    /// Gamedisk write: buffer all Data-Out PDUs, then write to cache
    pub(super) async fn handle_gamedisk_write(
        &mut self,
        req: &Pdu,
        lun_id: u8,
        lba: u64,
        num_blocks: u32,
    ) -> Result<(), std::io::Error> {
        // Clone backend Arc to avoid borrow conflict with self.send_r2t
        let backend = self.backends.get(&lun_id).cloned().unwrap();
        let block_size = backend.block_size();
        let expected_len = (num_blocks as usize) * (block_size as usize);

        let mut write_buf: Vec<u8> = Vec::with_capacity(expected_len);
        let mut bytes_received = 0;

        // Tampung immediate data (unsolicited)
        let immediate_len = req.data.len();
        if immediate_len > 0 {
            write_buf.extend_from_slice(&req.data);
            bytes_received = immediate_len;
        }

        // Kalo masih kurang, kirim R2T dan simpan ke pending_writes
        if bytes_received < expected_len {
            let remaining = (expected_len - bytes_received) as u32;
            info!("WRITE (gamedisk) LUN {} LBA {} ({} blocks): R2T offset={} desired={}",
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

        // Jika semua data diterima via Immediate Data
        if let Some(cache) = self.client_caches.get(&lun_id) {
            cache.write_stream(lba, 0, &write_buf)?;
        } else {
            self.send_scsi_check_condition(req.initiator_task_tag, 0x05, 0x25, 0x00).await?;
            return Ok(());
        }

        self.stats.bytes_written.fetch_add(expected_len as u64, std::sync::atomic::Ordering::Relaxed);
        info!("WRITE (gamedisk) LUN {} LBA {} sukses ({} bytes)", lun_id, lba, expected_len);
        self.send_scsi_response(req.initiator_task_tag, 0x00, 0, 0, 0).await?;
        Ok(())
    }
}
