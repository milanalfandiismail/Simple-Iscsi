use crate::pdu::{self, Pdu};
use crate::scsi_gamedisk;
use crate::session::Session;
use tracing::{error, info, warn, trace};

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
        let cache_opt = self.client_caches.get(&lun_id).map(|c| &**c);
        let active_luns: Vec<u8> = self.backends.keys().cloned().collect();
        let result = scsi_gamedisk::handle_scsi_command(
            cdb, backend.as_ref(), cache_opt, backend.block_size(), &active_luns, lun_id,
        );
        self.send_scsi_result(req, result).await
    }



    pub(super) async fn handle_gamedisk_write_pipelined(
        &mut self,
        req: &Pdu,
        lun_id: u8,
        lba: u64,
        num_blocks: u32,
        tx: tokio::sync::mpsc::Sender<super::DiskOpResult>,
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
            trace!("WRITE (gamedisk) LUN {} LBA {} ({} blocks): R2T offset={} desired={}",
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

        let cache_opt = self.client_caches.get(&lun_id).cloned();
        let write_buf_clone = write_buf;
        let itt = req.initiator_task_tag;
        let opcode = req.custom_bhs[0];
        let req_clone = req.clone();

        tokio::spawn(async move {
            if let Some(ref cache) = cache_opt {
                cache.throttle_write_async(write_buf_clone.len()).await;
            }

            let res = tokio::task::spawn_blocking(move || {
                if let Some(cache) = cache_opt {
                    cache.write_stream(lba, 0, &write_buf_clone)
                } else {
                    Err(std::io::Error::new(std::io::ErrorKind::NotFound, "Cache not found for LUN"))
                }
            }).await.unwrap();
            
            let _ = tx.send(super::DiskOpResult {
                itt,
                opcode,
                req: req_clone,
                result: res.map(|_| Vec::new()),
            }).await;
        });

        self.stats.bytes_written.fetch_add(expected_len as u64, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
}
