use crate::pdu::{self, Pdu, OP_LOGIN_REQ, OP_LOGIN_RESP, STAGE_FULL_FEATURE_PHASE};
use crate::session::Session;
use tracing::{info, warn};
use tokio::io::AsyncWriteExt;

impl Session {
    pub(super) async fn handle_login_phase(&mut self) -> Result<(), std::io::Error> {
        let mut in_login = true;
        while in_login {
            let req = pdu::parser::read_pdu(&mut self.stream).await?;
            if req.opcode != OP_LOGIN_REQ {
                warn!("Menerima opcode non-login selama fase login: 0x{:02X}", req.opcode);
                return Ok(());
            }

            // Flag stage transitions
            let req_flags = req.flags;
            let transit = (req_flags & 0x80) != 0;
            let csg = (req_flags & 0x0C) >> 2;
            let nsg = req_flags & 0x03;

            let params = pdu::parser::parse_text_parameters(&req.data);
            info!("Menerima Login Request parameters: {:?}", params);
            if let Some(iqn) = params.get("InitiatorName") {
                self.initiator_iqn = iqn.clone();
            }
            if let Some(st) = params.get("SessionType") {
                self.is_discovery = st == "Discovery";
            }
            if let Some(tn) = params.get("TargetName") {
                self.target_iqn = tn.clone();
            }
            if let Some(val) = params.get("MaxRecvDataSegmentLength") {
                if let Ok(len) = val.parse::<usize>() {
                    self.max_recv_data_segment_len = len.min(262144);
                }
            }

            // Siapkan parameter negosiasi
            let mut resp_params = Vec::new();
            if params.contains_key("AuthMethod") {
                resp_params.push(("AuthMethod".to_string(), "None".to_string()));
            }
            if params.contains_key("HeaderDigest") {
                resp_params.push(("HeaderDigest".to_string(), "None".to_string()));
            }
            if params.contains_key("DataDigest") {
                resp_params.push(("DataDigest".to_string(), "None".to_string()));
            }
            if !self.is_discovery && csg == pdu::STAGE_SECURITY_NEGOTIATION {
                resp_params.push(("TargetPortalGroupTag".to_string(), "1".to_string()));
            }
            
            // Negosiasikan opsi iSCSI standard jika di-request oleh client
            if let Some(val) = params.get("ImmediateData") {
                // ImmediateData: AND (server supports Yes, so we agree if client is Yes, else No)
                let resp_val = if val == "Yes" { "Yes" } else { "No" };
                resp_params.push(("ImmediateData".to_string(), resp_val.to_string()));
            }
            if let Some(val) = params.get("InitialR2T") {
                // InitialR2T: OR (server prefers No, but if client demands Yes, we must return Yes)
                let resp_val = if val == "Yes" { "Yes" } else { "No" };
                resp_params.push(("InitialR2T".to_string(), resp_val.to_string()));
            }
            if let Some(val) = params.get("MaxOutstandingR2T") {
                // MaxOutstandingR2T: MIN (client, server).
                let client_val = val.parse::<u32>().unwrap_or(1);
                let resp_val = client_val.min(16).max(1);
                resp_params.push(("MaxOutstandingR2T".to_string(), resp_val.to_string()));
            }
            if let Some(val) = params.get("MaxConnections") {
                let client_val = val.parse::<u32>().unwrap_or(1);
                let resp_val = client_val.min(1);
                resp_params.push(("MaxConnections".to_string(), resp_val.to_string()));
            }
            if let Some(val) = params.get("ErrorRecoveryLevel") {
                let client_val = val.parse::<u32>().unwrap_or(0);
                let resp_val = client_val.min(0);
                resp_params.push(("ErrorRecoveryLevel".to_string(), resp_val.to_string()));
            }
            if let Some(val) = params.get("DefaultTime2Wait") {
                resp_params.push(("DefaultTime2Wait".to_string(), val.clone()));
            }
            if let Some(val) = params.get("DefaultTime2Retain") {
                resp_params.push(("DefaultTime2Retain".to_string(), val.clone()));
            }
            if let Some(val) = params.get("DataPDUInOrder") {
                resp_params.push(("DataPDUInOrder".to_string(), val.clone()));
            }
            if let Some(val) = params.get("DataSequenceInOrder") {
                resp_params.push(("DataSequenceInOrder".to_string(), val.clone()));
            }
            if let Some(val) = params.get("MaxRecvDataSegmentLength") {
                // MaxRecvDataSegmentLength: declarative. We store client's MaxRecvDataSegmentLength
                // as the limit for sending data to client.
                let client_max = val.parse::<usize>().unwrap_or(8192);
                self.max_recv_data_segment_len = client_max;
                
                // We respond with our own receive limit (what server can receive from client).
                // 262144 (256 KB) is optimal and safe.
                resp_params.push(("MaxRecvDataSegmentLength".to_string(), "262144".to_string()));
            }
            if let Some(val) = params.get("FirstBurstLength") {
                // FirstBurstLength: MIN (client, server).
                let client_val = val.parse::<u32>().unwrap_or(65536);
                let resp_val = client_val.min(2097152);
                resp_params.push(("FirstBurstLength".to_string(), resp_val.to_string()));
            }
            if let Some(val) = params.get("MaxBurstLength") {
                // MaxBurstLength: MIN (client, server).
                let client_val = val.parse::<u32>().unwrap_or(262144);
                let resp_val = client_val.min(2097152);
                resp_params.push(("MaxBurstLength".to_string(), resp_val.to_string()));
            }

            let mut resp = Pdu::default();
            resp.opcode = OP_LOGIN_RESP;
            resp.flags = (csg << 2) | nsg;
            if transit {
                resp.flags |= 0x80;
                if nsg == STAGE_FULL_FEATURE_PHASE {
                    in_login = false;
                }
            }

            let isid = req.lun & 0xFFFFFFFFFFFF0000;
            let tsih: u16 = if self.is_discovery { 0 } else { 1 };
            resp.lun = isid | (tsih as u64);
            resp.initiator_task_tag = req.initiator_task_tag;
            
            // Response sequence numbers
            resp.cmd_sn = self.stat_sn;
            self.stat_sn = self.stat_sn.wrapping_add(1);
            
            self.exp_cmd_sn = req.cmd_sn.wrapping_add(1);
            self.max_cmd_sn = self.exp_cmd_sn.wrapping_add(32);
            
            resp.exp_stat_sn = self.exp_cmd_sn;
            resp.max_cmd_sn = self.max_cmd_sn;
            
            info!("Mengirim Login Response parameters: {:?}", resp_params);
            resp.data = pdu::builder::build_text_parameters(&resp_params);

            let packet = pdu::builder::build_pdu(&resp);
            self.stream.write_all(&packet).await?;
            self.stream.flush().await?;
        }

        info!("Transisi login sukses. Client masuk ke FFP (Full Feature Phase).");
        Ok(())
    }
}
