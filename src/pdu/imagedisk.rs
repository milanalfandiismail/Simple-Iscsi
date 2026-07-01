//! MODE SENSE page builder khusus ImageDisk (Windows iSCSI boot)
//! Windows Boot Manager requires pages 0x08, 0x0A, 0x00, and 0x3F

/// Build MODE SENSE (6) response data for ImageDisk
pub fn build_mode_sense_6(page_code: u8, _block_size: u64) -> Vec<u8> {
    let mut data = Vec::new();

    // Mode Parameter Header (6) — 4 bytes
    let total_data_len = match page_code {
        0x08 => 4 + 12,          // header + Caching page
        0x0A => 4 + 12,          // header + Control page
        0x00 => 4,               // header only (empty vendor page)
        0x3F => 4 + 12 + 12,     // header + Caching + Control
        _ => 4,
    };
    let mode_data_len = (total_data_len - 1) as u8;

    data.push(mode_data_len);   // Mode Data Length
    data.push(0x00);            // Medium Type
    data.push(0x00);            // Device-Specific Parameter
    data.push(0x00);            // Block Descriptor Length

    match page_code {
        0x08 => append_caching_page(&mut data),
        0x0A => append_control_page(&mut data),
        0x3F => {
            append_caching_page(&mut data);
            append_control_page(&mut data);
        }
        _ => {} // 0x00 = empty, no page data
    }

    // Update Mode Data Length accurately
    data[0] = (data.len() - 1) as u8;
    data
}

/// Build MODE SENSE (10) response data for ImageDisk
pub fn build_mode_sense_10(page_code: u8, _block_size: u64, alloc_len: usize) -> Vec<u8> {
    let mut data = Vec::new();

    // Mode Parameter Header (10) — 8 bytes
    data.extend_from_slice(&[0, 0]); // placeholder for Mode Data Length
    data.push(0x00); // Medium Type
    data.push(0x00); // Device-Specific Parameter
    data.extend_from_slice(&[0, 0]); // Reserved
    data.extend_from_slice(&[0, 0]); // Block Descriptor Length

    match page_code {
        0x08 => append_caching_page(&mut data),
        0x0A => append_control_page(&mut data),
        0x3F => {
            append_caching_page(&mut data);
            append_control_page(&mut data);
        }
        _ => {}
    }

    // Update Mode Data Length (big-endian 16-bit, excludes the 2 length bytes)
    let data_len = (data.len() - 2) as u16;
    data[0] = (data_len >> 8) as u8;
    data[1] = data_len as u8;

    if data.len() > alloc_len {
        data.truncate(alloc_len);
    }

    data
}

/// SCSI Caching Page (0x08) — 12 bytes
fn append_caching_page(data: &mut Vec<u8>) {
    data.push(0x08); // Page Code
    data.push(0x0A); // Page Length (10 bytes)
    data.push(0x04); // WCE = 1 (Write Cache Enabled)
    data.extend_from_slice(&[0; 9]); // rest
}

/// SCSI Control Page (0x0A) — 12 bytes
fn append_control_page(data: &mut Vec<u8>) {
    data.push(0x0A); // Page Code
    data.push(0x0A); // Page Length (10 bytes)
    data.push(0x00); // TST=0, TMF_ONLY=0, DPICZ=0, D_SENSE=0
    data.push(0x00); // GLTSD=0, RLEC=0
    data.push(0x00); // Queue Algorithm = Unrestricted
    data.extend_from_slice(&[0; 7]); // rest
}
