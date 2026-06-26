use crate::pdu::{self, Pdu, OP_SCSI_RESP, OP_DATA_IN, OP_R2T};
use crate::session::Session;
use tokio::io::AsyncWriteExt;

impl Session {
    pub(super) async fn send_r2t(
        &mut self,
        itt: u32,
        lun: u64,
        buffer_offset: u32,
        desired_len: u32,
    ) -> Result<(), std::io::Error> {
        let mut resp = Pdu::default();
        resp.opcode = OP_R2T;
        resp.flags = 0x80; // F (Final) — kita cuma support 1 R2T per task
        resp.lun = lun;
        resp.initiator_task_tag = itt;
        // R2T bukan status PDU — StatSN (cmd_sn) = 0, jangan increment!
        resp.cmd_sn = 0;
        resp.exp_stat_sn = self.exp_cmd_sn;
        resp.max_cmd_sn = self.max_cmd_sn;

        // custom_bhs[4..8]  = bytes 36-39: R2TSN
        resp.custom_bhs[4..8].copy_from_slice(&0u32.to_be_bytes());
        // custom_bhs[8..12]  = bytes 40-43: Buffer Offset
        resp.custom_bhs[8..12].copy_from_slice(&buffer_offset.to_be_bytes());
        // custom_bhs[12..16] = bytes 44-47: Desired Data Transfer Length
        resp.custom_bhs[12..16].copy_from_slice(&desired_len.to_be_bytes());

        let packet = pdu::builder::build_pdu(&resp);
        self.stream.write_all(&packet).await?;
        self.stream.flush().await?;
        Ok(())
    }

    pub(super) async fn send_scsi_data_in(
        &mut self,
        itt: u32,
        data: &[u8],
        status: u8,
        expected_len: u32,
    ) -> Result<(), std::io::Error> {
        let max_chunk = self.max_recv_data_segment_len;
        let total_len = data.len();
        let mut offset = 0;
        let mut data_sn = 0;
        let pad_arr = [0u8; 3];

        while offset < total_len {
            let chunk_len = (total_len - offset).min(max_chunk);
            let is_last = offset + chunk_len >= total_len;

            // Build DATA_IN BHS on stack
            let mut bhs = [0u8; 48];
            bhs[0] = OP_DATA_IN;
            if is_last {
                bhs[1] = 0x80; // F (Final)
            }
            bhs[5] = ((chunk_len >> 16) & 0xFF) as u8;
            bhs[6] = ((chunk_len >> 8) & 0xFF) as u8;
            bhs[7] = (chunk_len & 0xFF) as u8;
            bhs[16..20].copy_from_slice(&itt.to_be_bytes());
            bhs[20..24].copy_from_slice(&0xFFFFFFFFu32.to_be_bytes()); // TTT
            bhs[24..28].copy_from_slice(&0u32.to_be_bytes()); // StatSN = 0
            bhs[28..32].copy_from_slice(&self.exp_cmd_sn.to_be_bytes());
            bhs[32..36].copy_from_slice(&self.max_cmd_sn.to_be_bytes());
            bhs[36..40].copy_from_slice(&(data_sn as u32).to_be_bytes()); // DataSN
            bhs[40..44].copy_from_slice(&(offset as u32).to_be_bytes()); // Buffer Offset

            let pad = (4 - (chunk_len % 4)) % 4;

            if is_last {
                // Build SCSI_RESPONSE BHS on stack
                let mut resp = [0u8; 48];
                resp[0] = OP_SCSI_RESP;
                let mut flags = 0x80;
                if total_len < expected_len as usize {
                    flags |= 0x04; // U (Underflow)
                }
                resp[1] = flags;
                resp[2] = 0x00;
                resp[3] = status;
                resp[16..20].copy_from_slice(&itt.to_be_bytes());
                resp[20..24].copy_from_slice(&0xFFFFFFFFu32.to_be_bytes());
                resp[24..28].copy_from_slice(&self.stat_sn.to_be_bytes());
                self.stat_sn = self.stat_sn.wrapping_add(1);
                resp[28..32].copy_from_slice(&self.exp_cmd_sn.to_be_bytes());
                resp[32..36].copy_from_slice(&self.max_cmd_sn.to_be_bytes());
                resp[36..40].copy_from_slice(&(data_sn as u32).to_be_bytes());
                if total_len < expected_len as usize {
                    let residual = expected_len as usize - total_len;
                    resp[44..48].copy_from_slice(&(residual as u32).to_be_bytes());
                }

                // write_vectored BHS + data, lalu pad, baru RESP
                let iov = [
                    std::io::IoSlice::new(&bhs),
                    std::io::IoSlice::new(&data[offset..offset + chunk_len]),
                ];
                self.stream.write_vectored(&iov).await?;
                if pad > 0 {
                    self.stream.write_all(&pad_arr[..pad]).await?;
                }
                self.stream.write_all(&resp).await?;
            } else {
                let iov = [
                    std::io::IoSlice::new(&bhs),
                    std::io::IoSlice::new(&data[offset..offset + chunk_len]),
                ];
                self.stream.write_vectored(&iov).await?;
                if pad > 0 {
                    self.stream.write_all(&pad_arr[..pad]).await?;
                }
            }

            offset += chunk_len;
            data_sn += 1;
        }

        Ok(())
    }

    pub(super) async fn send_scsi_response(
        &mut self,
        itt: u32,
        status: u8,
        exp_data_sn: u32,
        expected_len: u32,
        actual_len: u32,
    ) -> Result<(), std::io::Error> {
        let mut resp = Pdu::default();
        resp.opcode = OP_SCSI_RESP;
        
        let mut flags = 0x80; // F (Final)
        if actual_len < expected_len {
            flags |= 0x04; // U (Underflow)
        }
        resp.flags = flags;

        resp.opcode_specific[0] = 0x00; // Command completed at target
        resp.opcode_specific[1] = status;

        resp.initiator_task_tag = itt;
        resp.cmd_sn = self.stat_sn;
        self.stat_sn = self.stat_sn.wrapping_add(1);
        resp.exp_stat_sn = self.exp_cmd_sn;
        resp.max_cmd_sn = self.max_cmd_sn;

        // Tulis ExpDataSN ke custom_bhs[4..8] (yang dipetakan ke bytes 36-39)
        resp.custom_bhs[4..8].copy_from_slice(&exp_data_sn.to_be_bytes());

        // Tulis Residual Count ke custom_bhs[12..16] jika terjadi underflow
        if actual_len < expected_len {
            let residual = expected_len - actual_len;
            resp.custom_bhs[12..16].copy_from_slice(&residual.to_be_bytes());
        }

        let packet = pdu::builder::build_pdu(&resp);
        self.stream.write_all(&packet).await?;
        self.stream.flush().await?;
        Ok(())
    }

    pub(super) async fn send_scsi_check_condition(
        &mut self,
        itt: u32,
        key: u8,
        asc: u8,
        ascq: u8,
    ) -> Result<(), std::io::Error> {
        let mut sense_data = vec![0u8; 18];
        sense_data[0] = 0x70;
        sense_data[2] = key;
        sense_data[7] = 0x0A;
        sense_data[12] = asc;
        sense_data[13] = ascq;

        let mut data = vec![0u8; 2 + 18];
        data[0..2].copy_from_slice(&(18u16).to_be_bytes());
        data[2..20].copy_from_slice(&sense_data);

        let mut resp = Pdu::default();
        resp.opcode = OP_SCSI_RESP;
        resp.flags = 0x80;

        resp.opcode_specific[0] = 0x00;
        resp.opcode_specific[1] = 0x02; // Check Condition

        resp.initiator_task_tag = itt;
        resp.cmd_sn = self.stat_sn;
        self.stat_sn = self.stat_sn.wrapping_add(1);
        resp.exp_stat_sn = self.exp_cmd_sn;
        resp.max_cmd_sn = self.max_cmd_sn;
        resp.data = data;

        let packet = pdu::builder::build_pdu(&resp);
        self.stream.write_all(&packet).await?;
        self.stream.flush().await?;
        Ok(())
    }
}
