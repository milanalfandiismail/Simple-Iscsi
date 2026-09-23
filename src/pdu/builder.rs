use crate::pdu::Pdu;

/// Serializes PDU header (BHS) into a fixed 48-byte buffer. Zero-alloc.
/// Returns the data segment length for padding calculation.
pub fn write_bhs(pdu: &Pdu, bhs: &mut [u8; 48]) -> u32 {
    // Byte 0: Opcode
    bhs[0] = pdu.opcode & 0x3F;
    if pdu.is_immediate {
        bhs[0] |= 0x80;
    }

    // Byte 1: Flags
    bhs[1] = pdu.flags;

    // Byte 2-4: Opcode-specific
    bhs[2] = pdu.opcode_specific[0];
    bhs[3] = pdu.opcode_specific[1];
    bhs[4] = pdu.opcode_specific[2]; // AHS length

    // Byte 5-7: Data Segment Length (24-bit big endian)
    let data_len = pdu.data.len() as u32;
    bhs[5] = ((data_len >> 16) & 0xFF) as u8;
    bhs[6] = ((data_len >> 8) & 0xFF) as u8;
    bhs[7] = (data_len & 0xFF) as u8;

    // Byte 8-15: LUN
    bhs[8..16].copy_from_slice(&pdu.lun.to_be_bytes());

    // Byte 16-19: Initiator Task Tag (ITT)
    bhs[16..20].copy_from_slice(&pdu.initiator_task_tag.to_be_bytes());

    // Sequence numbers based on direction
    if pdu.opcode >= 0x20 {
        // Target -> Initiator PDU
        let ttt = match pdu.opcode {
            0x20 | 0x21 | 0x22 | 0x24 | 0x25 => 0xFFFFFFFFu32,
            0x31 => pdu.initiator_task_tag, // R2T MUST NOT be 0xFFFFFFFF per RFC 7143 Section 11.8.3
            _ => 0xFFFFFFFFu32, // Default safe 0xFFFFFFFF per RFC 7143 Section 11.4.3 & 11.6
        };
        bhs[20..24].copy_from_slice(&ttt.to_be_bytes());
        bhs[24..28].copy_from_slice(&pdu.cmd_sn.to_be_bytes());
        bhs[28..32].copy_from_slice(&pdu.exp_stat_sn.to_be_bytes());
        bhs[32..36].copy_from_slice(&pdu.max_cmd_sn.to_be_bytes());
        bhs[36..48].copy_from_slice(&pdu.custom_bhs[4..16]);
    } else {
        // Initiator -> Target PDU
        bhs[20..24].copy_from_slice(&pdu.cmd_sn.to_be_bytes());
        bhs[24..28].copy_from_slice(&pdu.exp_stat_sn.to_be_bytes());
        bhs[28..32].copy_from_slice(&pdu.max_cmd_sn.to_be_bytes());
        bhs[32..48].copy_from_slice(&pdu.custom_bhs);
    }

    data_len
}

/// Build PDU as Vec<u8>. Uses write_bhs internally.
pub fn build_pdu(pdu: &Pdu) -> Vec<u8> {
    let mut bhs = [0u8; 48];
    let data_len = write_bhs(pdu, &mut bhs);

    let mut packet = Vec::with_capacity(48 + data_len as usize + 3);
    packet.extend_from_slice(&bhs);
    packet.extend_from_slice(&pdu.data);

    // Padding to 4-byte boundary
    let padding_len = (4 - (pdu.data.len() % 4)) % 4;
    for _ in 0..padding_len {
        packet.push(0);
    }

    packet
}

/// Membuat data segment berisi parameter text key-value dalam format `Key=Value\0`.
pub fn build_text_parameters(params: &[(String, String)]) -> Vec<u8> {
    let mut data = Vec::new();
    for (k, v) in params {
        data.extend_from_slice(k.as_bytes());
        data.push(b'=');
        data.extend_from_slice(v.as_bytes());
        data.push(0);
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_pdu() {
        let mut pdu = Pdu::default();
        pdu.opcode = 0x23; // Login Response
        pdu.flags = 0x80;
        pdu.initiator_task_tag = 0x12345678;
        pdu.data = b"TargetPortalGroupTag=1\0".to_vec();

        let packet = build_pdu(&pdu);
        assert_eq!(packet.len(), 48 + 24); // 23 bytes + 1 byte padding = 24
        assert_eq!(packet[0], 0x23);
        assert_eq!(packet[1], 0x80);

        let parsed_len = (((packet[5] as u32) << 16) | ((packet[6] as u32) << 8) | (packet[7] as u32)) as usize;
        assert_eq!(parsed_len, 23);
    }

    #[test]
    fn test_target_transfer_tag_scsi_and_tmf_resp() {
        let mut scsi_resp = Pdu::default();
        scsi_resp.opcode = 0x21; // OP_SCSI_RESP
        let mut bhs = [0u8; 48];
        write_bhs(&scsi_resp, &mut bhs);
        assert_eq!(&bhs[20..24], &[0xFF, 0xFF, 0xFF, 0xFF]);

        let mut tmf_resp = Pdu::default();
        tmf_resp.opcode = 0x22; // OP_TMF_RESP
        let mut bhs_tmf = [0u8; 48];
        write_bhs(&tmf_resp, &mut bhs_tmf);
        assert_eq!(&bhs_tmf[20..24], &[0xFF, 0xFF, 0xFF, 0xFF]);
    }
}
