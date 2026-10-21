//! SCSI handler khusus ImageDisk (Windows iSCSI sanboot)
//! Menangani MODE SENSE, VERIFY, START STOP UNIT, dll yang dibutuhkan Windows Boot Manager

use crate::backend::Backend;
use crate::writeback_gamedisk::ClientCache;
use crate::pdu::imagedisk;
use crate::scsi_gamedisk::{self, ScsiResult};

/// ImageDisk SCSI command handler — intercepts Windows-specific commands
/// before delegating to scsi_gamedisk for shared SCSI commands
pub fn handle_imagedisk_scsi(
    cdb: &[u8],
    backend: &Backend,
    cache: Option<&ClientCache>,
    block_size: u64,
    active_luns: &[u8],
    lun_id: u8,
) -> ScsiResult {
    let opcode = cdb[0];

    match opcode {
        // INQUIRY (0x12) — Windows Boot Manager & Storport SAN Boot requirements
        0x12 => handle_imagedisk_inquiry(cdb, backend, lun_id),

        // MODE SENSE (6) / (10) — Windows Boot Manager requirements
        0x1A | 0x5A => {
            let page_code = cdb[2] & 0x3F;
            if opcode == 0x1A {
                let alloc_len = cdb[4] as usize;
                let mut data = imagedisk::build_mode_sense_6(page_code, block_size);
                if data.len() > alloc_len {
                    data.truncate(alloc_len);
                }
                return ScsiResult::Data { data, status: 0x00 };
            } else {
                let alloc_len = u16::from_be_bytes([cdb[7], cdb[8]]) as usize;
                let data = imagedisk::build_mode_sense_10(page_code, block_size, alloc_len);
                return ScsiResult::Data { data, status: 0x00 };
            }
        }
        // VERIFY (10) — Windows sends this, no-op for VHD
        0x2F => {
            return ScsiResult::Status { status: 0x00 };
        }
        // START STOP UNIT
        0x1B => {
            return ScsiResult::Status { status: 0x00 };
        }
        // REPORT LUNS — agar Windows lihat semua LUN di target ini
        0xA0 => {
            let lun_list_len = (active_luns.len() * 8) as u32;
            let mut data = vec![0u8; 8 + active_luns.len() * 8];
            data[0..4].copy_from_slice(&lun_list_len.to_be_bytes());
            for (i, &lun_id) in active_luns.iter().enumerate() {
                let offset = 8 + i * 8;
                data[offset] = 0;
                data[offset + 1] = lun_id;
            }
            let alloc_len = u32::from_be_bytes(cdb[6..10].try_into().unwrap()) as usize;
            if data.len() > alloc_len {
                data.truncate(alloc_len);
            }
            return ScsiResult::Data { data, status: 0x00 };
        }
        // READ (16)
        0x88 => {
            return scsi_gamedisk::handle_scsi_command(cdb, backend, cache, block_size, active_luns, lun_id);
        }
        // WRITE (16) — handled by handle_scsi_cmd dispatch (line 144 of scsi_handler)
        0x8A => {
            return ScsiResult::Status { status: 0x00 };
        }
        // SERVICE ACTION IN (16)
        0x9E => {
            return scsi_gamedisk::handle_scsi_command(cdb, backend, cache, block_size, active_luns, lun_id);
        }
        // All other commands → delegate to scsi_gamedisk
        _ => scsi_gamedisk::handle_scsi_command(cdb, backend, cache, block_size, active_luns, lun_id),
    }
}

fn handle_imagedisk_inquiry(cdb: &[u8], backend: &Backend, lun_id: u8) -> ScsiResult {
    let evpd = (cdb[1] & 0x01) != 0;
    let page_code = cdb[2];
    let alloc_len = if cdb[3] > 0 {
        u16::from_be_bytes([cdb[3], cdb[4]]) as usize
    } else {
        cdb[4] as usize
    };
    let mut response_data = Vec::new();

    if evpd {
        match page_code {
            0x00 => {
                // Supported VPD Pages: 0x00, 0x80, 0x83, 0xB0, 0xB1, 0xB2
                response_data.extend_from_slice(&[0x00, 0x00, 0x00, 6, 0x00, 0x80, 0x83, 0xB0, 0xB1, 0xB2]);
            }
            0x80 => {
                // Unit Serial Number
                let serial = format!("IMG{:04}", lun_id);
                response_data.extend_from_slice(&[0x00, 0x80, 0x00, serial.len() as u8]);
                response_data.extend_from_slice(serial.as_bytes());
            }
            0x83 => {
                // Device Identification (SPC-4)
                let name_str = format!("iqn.2024-01.com.imagedisk:lun{}", lun_id);
                let name_len = name_str.len();

                // Descriptor 1: NAA IEEE Registered Extended (Type 3, 16 bytes)
                // Header: CodeSet=1 (binary), Pivot/Assoc=0 (LUN), Type=3 (NAA), Length=16
                let mut desc_naa = vec![0u8; 20];
                desc_naa[0] = 0x01; // Binary
                desc_naa[1] = 0x03; // Association: Logical Unit (0x00), Designator Type: NAA (0x03)
                desc_naa[2] = 0x00; // Reserved
                desc_naa[3] = 0x10; // Length = 16 bytes
                // NAA format 6 (IEEE Registered Extended)
                desc_naa[4] = 0x60;
                desc_naa[5] = 0x02;
                desc_naa[6] = 0x4b;
                desc_naa[7] = 0x00;
                desc_naa[8] = 0x00;
                desc_naa[9] = 0x00;
                desc_naa[10] = 0x00;
                desc_naa[11] = lun_id;
                desc_naa[12..20].copy_from_slice(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]);

                // Descriptor 2: SCSI Name String (Type 8)
                let mut desc_name = vec![0u8; 4 + name_len];
                desc_name[0] = 0x02; // UTF-8
                desc_name[1] = 0x08; // Association: Logical Unit (0x00), Designator Type: SCSI Name String (0x08)
                desc_name[2] = 0x00; // Reserved
                desc_name[3] = name_len as u8;
                desc_name[4..].copy_from_slice(name_str.as_bytes());

                let payload_len = (desc_naa.len() + desc_name.len()) as u16;
                response_data.extend_from_slice(&[0x00, 0x83, ((payload_len >> 8) & 0xFF) as u8, (payload_len & 0xFF) as u8]);
                response_data.extend_from_slice(&desc_naa);
                response_data.extend_from_slice(&desc_name);
            }
            0xB0 => {
                // Block Limits VPD Page (SPC-4 / SBC-3)
                response_data.extend_from_slice(&[0x00, 0xB0, 0x00, 0x3C]);
                let mut page_b0 = [0u8; 60];
                // Offset 2..4: Optimal transfer length granularity = 8 blocks (4 KB)
                page_b0[2..4].copy_from_slice(&(8u16).to_be_bytes());
                // Offset 4..8: Maximum transfer length = 8192 blocks (4 MB)
                page_b0[4..8].copy_from_slice(&(8192u32).to_be_bytes());
                // Offset 8..12: Optimal transfer length = 4096 blocks (2 MB - matching VHD block size)
                page_b0[8..12].copy_from_slice(&(4096u32).to_be_bytes());
                // Offset 12..16: Maximum prefetch length = 8192 blocks (4 MB)
                page_b0[12..16].copy_from_slice(&(8192u32).to_be_bytes());
                // Offset 16..20: Maximum unmap LBA count = 8192 blocks
                page_b0[16..20].copy_from_slice(&(8192u32).to_be_bytes());
                // Offset 20..24: Maximum unmap block descriptor count = 256
                page_b0[20..24].copy_from_slice(&(256u32).to_be_bytes());
                // Offset 24..28: Optimal unmap granularity = 8 blocks (4 KB)
                page_b0[24..28].copy_from_slice(&(8u32).to_be_bytes());
                // Offset 32..36: Maximum WRITE SAME length = 8192 blocks
                page_b0[32..36].copy_from_slice(&(8192u32).to_be_bytes());
                response_data.extend_from_slice(&page_b0);
            }
            0xB1 => {
                // Block Device Characteristics (SSD / non-rotational)
                response_data.extend_from_slice(&[0x00, 0xB1, 0x00, 0x3C]);
                let mut page_b1 = [0u8; 60];
                page_b1[0..2].copy_from_slice(&[0x00, 0x01]); // Non-rotating medium (SSD)
                page_b1[3] = 0x05; // 2.5" form factor
                response_data.extend_from_slice(&page_b1);
            }
            0xB2 => {
                // Logical Block Provisioning (Thin Provisioning)
                response_data.extend_from_slice(&[0x00, 0xB2, 0x00, 0x04, 0x00, 0xE0, 0x00, 0x00]);
            }
            _ => return ScsiResult::CheckCondition { key: 0x05, asc: 0x24, ascq: 0x00 },
        }
    } else {
        // Standard INQUIRY (36 bytes default)
        // Byte 0: 0x00 (Direct-access block device)
        // Byte 1: 0x00 (RMB = 0, non-removable)
        // Byte 2: 0x06 (SPC-4)
        // Byte 3: 0x02 (Response data format)
        // Byte 4: 31 (Additional length, total 36 bytes)
        // Byte 7: 0x02 (CmdQue = 1: Tagged Command Queuing supported!)
        response_data.extend_from_slice(&[0x00, 0x00, 0x06, 0x02, 31, 0x00, 0x00, 0x02]);

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

    if alloc_len > 0 && response_data.len() > alloc_len {
        response_data.truncate(alloc_len);
    }
    ScsiResult::Data { data: response_data, status: 0x00 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_imagedisk_inquiry_standard_and_vpd() {
        let backend = Backend::new_dummy("CCBoot", "ImageDisk", "1.00");

        // 1. Standard INQUIRY
        let cdb_std = [0x12, 0x00, 0x00, 0x00, 36, 0x00];
        let res_std = handle_imagedisk_scsi(&cdb_std, &backend, None, 512, &[0], 0);
        match res_std {
            ScsiResult::Data { data, status } => {
                assert_eq!(status, 0x00);
                assert_eq!(data.len(), 36);
                assert_eq!(data[2], 0x06); // SPC-4
                assert_eq!(data[7] & 0x02, 0x02); // CmdQue = 1
                assert_eq!(&data[8..16], b"CCBoot  ");
                assert_eq!(&data[16..32], b"ImageDisk       ");
            }
            _ => panic!("Expected ScsiResult::Data for Standard INQUIRY"),
        }

        // 2. VPD 0x83 (Device Identification)
        let cdb_83 = [0x12, 0x01, 0x83, 0x00, 64, 0x00];
        let res_83 = handle_imagedisk_scsi(&cdb_83, &backend, None, 512, &[0], 0);
        match res_83 {
            ScsiResult::Data { data, status } => {
                assert_eq!(status, 0x00);
                assert_eq!(data[1], 0x83);
                // Must contain imagedisk target identifier, not gamedisk
                let s = String::from_utf8_lossy(&data);
                assert!(s.contains("imagedisk:lun0"), "VPD 0x83 must contain imagedisk identifier: {}", s);
                // Must contain NAA descriptor (Type 3)
                assert_eq!(data[5] & 0x0F, 0x03, "VPD 0x83 first descriptor must be NAA type 3");
            }
            _ => panic!("Expected ScsiResult::Data for VPD 0x83"),
        }

        // 3. VPD 0xB0 (Block Limits)
        let cdb_b0 = [0x12, 0x01, 0xB0, 0x00, 64, 0x00];
        let res_b0 = handle_imagedisk_scsi(&cdb_b0, &backend, None, 512, &[0], 0);
        match res_b0 {
            ScsiResult::Data { data, status } => {
                assert_eq!(status, 0x00);
                assert_eq!(data[1], 0xB0);
                let opt_gran = u16::from_be_bytes([data[6], data[7]]);
                assert_eq!(opt_gran, 8, "Optimal transfer length granularity must be 8 blocks (4K)");
                let max_xfer = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
                assert_eq!(max_xfer, 8192, "Max transfer length must be 8192 blocks (4MB)");
            }
            _ => panic!("Expected ScsiResult::Data for VPD 0xB0"),
        }
    }
}
