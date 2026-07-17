use crate::backend::Backend;
use crate::writeback_gamedisk::ClientCache;
use tracing::warn;

/// Representasi hasil eksekusi SCSI command.
pub enum ScsiResult {
    Status { status: u8 },
    Data { data: Vec<u8>, status: u8 },
    CheckCondition { key: u8, asc: u8, ascq: u8 },
}

/// Menangani SCSI command dari CDB (Command Descriptor Block).
pub fn handle_scsi_command(
    cdb: &[u8],
    backend: &Backend,
    cache: Option<&ClientCache>,
    block_size: u64,
    active_luns: &[u8],
    lun_id: u8,
) -> ScsiResult {
    let opcode = cdb[0];

    match opcode {
        0x00 => ScsiResult::Status { status: 0x00 }, // TEST UNIT READY
        0x03 => handle_request_sense(),
        0x12 => handle_inquiry(cdb, backend, lun_id),
        0x1A => handle_mode_sense_6(cdb),
        0x5A => handle_mode_sense_10(cdb),
        0x25 => handle_read_capacity_10(backend, block_size),
        0x9E => handle_service_action_in_16(cdb, backend, block_size),
        0x28 => handle_read_10(cdb, backend, cache, block_size),
        0x35 => handle_synchronize_cache(cache),
        0x1E => ScsiResult::Status { status: 0x00 }, // PREVENT ALLOW MEDIUM REMOVAL
        0xA0 => handle_report_luns(cdb, active_luns),
        _ => {
            warn!("SCSI command tidak dikenal/didukung: 0x{:02X}", opcode);
            ScsiResult::CheckCondition { key: 0x05, asc: 0x20, ascq: 0x00 }
        }
    }
}

fn handle_request_sense() -> ScsiResult {
    let mut data = vec![0u8; 18];
    data[0] = 0x70; // Fixed format, current errors
    data[2] = 0x00; // Sense key: No Sense
    data[7] = 0x0A; // Additional sense length
    ScsiResult::Data { data, status: 0x00 }
}

fn handle_inquiry(cdb: &[u8], backend: &Backend, lun_id: u8) -> ScsiResult {
    let evpd = (cdb[1] & 0x01) != 0;
    let page_code = cdb[2];
    let alloc_len = cdb[4] as usize;
    let mut response_data = Vec::new();

    if evpd {
        match page_code {
            0x00 => {
                response_data.extend_from_slice(&[0x00, 0x00, 0x00, 6, 0x00, 0x80, 0x83, 0xB0, 0xB1, 0xB2]);
            }
            0x80 => {
                let serial = format!("MGC{:04}", lun_id);
                response_data.extend_from_slice(&[0x00, 0x80, 0x00, serial.len() as u8]);
                response_data.extend_from_slice(serial.as_bytes());
            }
            0x83 => {
                let name_str = format!("iqn.2024-01.com.gamedisk:lun{}", lun_id);
                let total_len = 4 + name_str.len();
                response_data.extend_from_slice(&[0x00, 0x83, 0x00, total_len as u8, 0x02, 0x08, 0x00, name_str.len() as u8]);
                response_data.extend_from_slice(name_str.as_bytes());
            }
            0xB0 => {
                response_data.extend_from_slice(&[0x00, 0xB0, 0x00, 0x3C]);
                response_data.extend_from_slice(&[0u8; 60]);
            }
            0xB1 => {
                response_data.extend_from_slice(&[0x00, 0xB1, 0x00, 0x3C, 0x00, 0x01]);
                response_data.extend_from_slice(&[0u8; 58]);
            }
            0xB2 => {
                response_data.extend_from_slice(&[0x00, 0xB2, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00]);
            }
            _ => return ScsiResult::CheckCondition { key: 0x05, asc: 0x24, ascq: 0x00 },
        }
    } else {
        response_data.extend_from_slice(&[0x00, 0x00, 0x06, 0x02, 31, 0x00, 0x00, 0x00]);
        let mut vendor = vec![b' '; 8];
        let v_bytes = backend.vendor_id.as_bytes();
        vendor[..std::cmp::min(8, v_bytes.len())].copy_from_slice(&v_bytes[..std::cmp::min(8, v_bytes.len())]);
        response_data.extend_from_slice(&vendor);
        
        let mut product = vec![b' '; 16];
        let p_bytes = backend.product_id.as_bytes();
        product[..std::cmp::min(16, p_bytes.len())].copy_from_slice(&p_bytes[..std::cmp::min(16, p_bytes.len())]);
        response_data.extend_from_slice(&product);
        
        let mut rev = vec![b' '; 4];
        let r_bytes = backend.product_revision.as_bytes();
        rev[..std::cmp::min(4, r_bytes.len())].copy_from_slice(&r_bytes[..std::cmp::min(4, r_bytes.len())]);
        response_data.extend_from_slice(&rev);
    }

    if response_data.len() > alloc_len { response_data.truncate(alloc_len); }
    ScsiResult::Data { data: response_data, status: 0x00 }
}

fn handle_mode_sense_6(cdb: &[u8]) -> ScsiResult {
    let page_code = cdb[2] & 0x3F;
    let alloc_len = cdb[4] as usize;
    let mut response_data = Vec::new();

    if page_code == 0x08 || page_code == 0x3F {
        response_data.extend_from_slice(&[15, 0x00, 0x00, 0x00, 0x08, 0x0A, 0x04]);
        response_data.extend_from_slice(&[0; 9]);
    } else {
        response_data.extend_from_slice(&[3, 0x00, 0x00, 0x00]);
    }

    if response_data.len() > alloc_len { response_data.truncate(alloc_len); }
    ScsiResult::Data { data: response_data, status: 0x00 }
}

fn handle_mode_sense_10(cdb: &[u8]) -> ScsiResult {
    let page_code = cdb[2] & 0x3F;
    let alloc_len = u16::from_be_bytes([cdb[7], cdb[8]]) as usize;
    let mut response_data = Vec::new();

    if page_code == 0x08 || page_code == 0x3F {
        response_data.extend_from_slice(&[0, 18, 0x00, 0x00, 0, 0, 0, 0, 0x08, 0x0A, 0x04]);
        response_data.extend_from_slice(&[0; 9]);
    } else {
        response_data.extend_from_slice(&[0, 6, 0x00, 0x00, 0, 0, 0, 0]);
    }

    if response_data.len() > alloc_len { response_data.truncate(alloc_len); }
    ScsiResult::Data { data: response_data, status: 0x00 }
}

fn handle_read_capacity_10(backend: &Backend, block_size: u64) -> ScsiResult {
    let total_blocks = backend.total_size() / block_size;
    let max_lba = if total_blocks > 0 { (total_blocks - 1) as u32 } else { 0 };
    let mut data = vec![0u8; 8];
    data[0..4].copy_from_slice(&max_lba.to_be_bytes());
    data[4..8].copy_from_slice(&(block_size as u32).to_be_bytes());
    ScsiResult::Data { data, status: 0x00 }
}

fn handle_service_action_in_16(cdb: &[u8], backend: &Backend, block_size: u64) -> ScsiResult {
    let service_action = cdb[1] & 0x1F;
    if service_action == 0x10 {
        let total_blocks = backend.total_size() / block_size;
        let max_lba = if total_blocks > 0 { total_blocks - 1 } else { 0 };
        let mut data = vec![0u8; 32];
        data[0..8].copy_from_slice(&max_lba.to_be_bytes());
        data[8..12].copy_from_slice(&(block_size as u32).to_be_bytes());
        let alloc_len = u32::from_be_bytes(cdb[10..14].try_into().unwrap()) as usize;
        if data.len() > alloc_len { data.truncate(alloc_len); }
        ScsiResult::Data { data, status: 0x00 }
    } else {
        ScsiResult::CheckCondition { key: 0x05, asc: 0x24, ascq: 0x00 }
    }
}

fn handle_read_10(cdb: &[u8], backend: &Backend, cache: Option<&ClientCache>, block_size: u64) -> ScsiResult {
    let lba = u32::from_be_bytes(cdb[2..6].try_into().unwrap()) as u64;
    let num_blocks = u16::from_be_bytes(cdb[7..9].try_into().unwrap()) as u32;
    let total_bytes = (num_blocks as u64) * block_size;
    let mut data = Vec::with_capacity(total_bytes as usize);
    unsafe { data.set_len(total_bytes as usize); }

    if let Some(c) = cache {
        if c.read_blocks_cached(backend, lba, num_blocks, &mut data).is_err() {
            return ScsiResult::CheckCondition { key: 0x03, asc: 0x11, ascq: 0x00 };
        }
    } else if backend.read_blocks(lba, num_blocks, &mut data).is_err() {
        return ScsiResult::CheckCondition { key: 0x03, asc: 0x11, ascq: 0x00 };
    }
    ScsiResult::Data { data, status: 0x00 }
}

fn handle_synchronize_cache(cache: Option<&ClientCache>) -> ScsiResult {
    if let Some(c) = cache {
        if c.flush().is_err() {
            return ScsiResult::CheckCondition { key: 0x03, asc: 0x0C, ascq: 0x00 };
        }
    }
    ScsiResult::Status { status: 0x00 }
}

fn handle_report_luns(cdb: &[u8], active_luns: &[u8]) -> ScsiResult {
    let lun_list_len = (active_luns.len() * 8) as u32;
    let mut data = vec![0u8; 8 + active_luns.len() * 8];
    data[0..4].copy_from_slice(&lun_list_len.to_be_bytes());
    
    let mut sorted_luns = active_luns.to_vec();
    sorted_luns.sort_unstable();
    for (i, &lun_id) in sorted_luns.iter().enumerate() {
        let offset = 8 + i * 8;
        data[offset] = 0;
        data[offset + 1] = lun_id;
    }

    let alloc_len = u32::from_be_bytes(cdb[6..10].try_into().unwrap()) as usize;
    if data.len() > alloc_len { data.truncate(alloc_len); }
    ScsiResult::Data { data, status: 0x00 }
}


