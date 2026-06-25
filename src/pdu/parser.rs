use crate::pdu::Pdu;
use std::collections::HashMap;
use tokio::io::{AsyncRead, AsyncReadExt};

/// Membaca PDU iSCSI dari TCP Stream.
pub async fn read_pdu<R: AsyncRead + Unpin>(reader: &mut R) -> std::io::Result<Pdu> {
    let mut bhs = [0u8; 48];
    reader.read_exact(&mut bhs).await?;

    let opcode = bhs[0] & 0x3F;
    let is_immediate = (bhs[0] & 0x80) != 0;
    let flags = bhs[1];
    let opcode_specific = [bhs[2], bhs[3], bhs[4]];
    
    // Byte 4 berisi AHS Length dalam kata (4-byte words)
    let ahs_len_words = bhs[4] as usize;
    
    // Byte 5-7 berisi Data Segment Length (24-bit big endian)
    let data_len = (((bhs[5] as u32) << 16) | ((bhs[6] as u32) << 8) | (bhs[7] as u32)) as usize;

    let lun = u64::from_be_bytes(bhs[8..16].try_into().unwrap());
    let initiator_task_tag = u32::from_be_bytes(bhs[16..20].try_into().unwrap());
    
    let (expected_data_len, cmd_sn, exp_stat_sn, max_cmd_sn) = if opcode == 0x01 { // OP_SCSI_CMD
        (
            u32::from_be_bytes(bhs[20..24].try_into().unwrap()),
            u32::from_be_bytes(bhs[24..28].try_into().unwrap()),
            u32::from_be_bytes(bhs[28..32].try_into().unwrap()),
            0u32,
        )
    } else if opcode == 0x05 { // OP_DATA_OUT
        (
            0u32,
            u32::from_be_bytes(bhs[24..28].try_into().unwrap()),
            u32::from_be_bytes(bhs[28..32].try_into().unwrap()),
            0u32,
        )
    } else {
        (
            0u32,
            u32::from_be_bytes(bhs[20..24].try_into().unwrap()),
            u32::from_be_bytes(bhs[24..28].try_into().unwrap()),
            u32::from_be_bytes(bhs[28..32].try_into().unwrap()),
        )
    };

    let mut custom_bhs = [0u8; 16];
    custom_bhs.copy_from_slice(&bhs[32..48]);

    // Baca dan abaikan Additional Header Segment (AHS) jika ada
    if ahs_len_words > 0 {
        let ahs_len_bytes = ahs_len_words * 4;
        let mut ahs_dummy = vec![0u8; ahs_len_bytes];
        reader.read_exact(&mut ahs_dummy).await?;
    }

    // Baca Data Segment
    let mut data = vec![0u8; data_len];
    if data_len > 0 {
        reader.read_exact(&mut data).await?;
        
        // iSCSI menyelaraskan segment data ke batas 4 byte (padding)
        let padding_len = (4 - (data_len % 4)) % 4;
        if padding_len > 0 {
            let mut pad = vec![0u8; padding_len];
            reader.read_exact(&mut pad).await?;
        }
    }

    Ok(Pdu {
        opcode,
        is_immediate,
        flags,
        opcode_specific,
        data_segment_len: data_len as u32,
        lun,
        initiator_task_tag,
        expected_data_len,
        cmd_sn,
        exp_stat_sn,
        max_cmd_sn,
        custom_bhs,
        data,
    })
}

/// Parsing teks key-value dari iSCSI Data Segment (biasanya null-terminated `Key=Value\0`).
pub fn parse_text_parameters(data: &[u8]) -> HashMap<String, String> {
    let mut params = HashMap::new();
    let mut current = Vec::new();
    
    for &b in data {
        if b == 0 {
            if !current.is_empty() {
                if let Ok(s) = String::from_utf8(current.clone()) {
                    if let Some(pos) = s.find('=') {
                        let (k, v) = s.split_at(pos);
                        let k = k.trim().to_string();
                        let v = v[1..].trim().to_string();
                        params.insert(k, v);
                    }
                }
                current.clear();
            }
        } else {
            current.push(b);
        }
    }
    
    // Jika data tidak diakhiri null byte
    if !current.is_empty() {
        if let Ok(s) = String::from_utf8(current) {
            if let Some(pos) = s.find('=') {
                let (k, v) = s.split_at(pos);
                let k = k.trim().to_string();
                let v = v[1..].trim().to_string();
                params.insert(k, v);
            }
        }
    }
    
    params
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_text_parameters() {
        let data = b"InitiatorName=iqn.client\0TargetName=iqn.target\0";
        let params = parse_text_parameters(data);
        assert_eq!(params.get("InitiatorName").unwrap(), "iqn.client");
        assert_eq!(params.get("TargetName").unwrap(), "iqn.target");
    }
}
