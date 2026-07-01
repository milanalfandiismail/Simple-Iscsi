//! SCSI handler khusus ImageDisk (Windows iSCSI sanboot)
//! Menangani MODE SENSE, VERIFY, START STOP UNIT, dll yang dibutuhkan Windows Boot Manager

use crate::backend::Backend;
use crate::pdu_image;
use crate::scsi::{self, ScsiResult};
use crate::writeback_gamedisk::ClientCache;

/// ImageDisk SCSI command handler — intercepts Windows-specific commands
/// before delegating to shared scsi.rs
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
        // MODE SENSE (6) / (10) — Windows Boot Manager requirements
        0x1A | 0x5A => {
            let page_code = cdb[2] & 0x3F;
            if opcode == 0x1A {
                let alloc_len = cdb[4] as usize;
                let mut data = pdu_image::build_mode_sense_6(page_code, block_size);
                if data.len() > alloc_len {
                    data.truncate(alloc_len);
                }
                return ScsiResult::Data { data, status: 0x00 };
            } else {
                let alloc_len = u16::from_be_bytes([cdb[7], cdb[8]]) as usize;
                let data = pdu_image::build_mode_sense_10(page_code, block_size, alloc_len);
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
            return scsi::handle_scsi_command(cdb, backend, cache, block_size, active_luns, lun_id);
        }
        // WRITE (16) — handled by handle_scsi_cmd dispatch (line 144 of scsi_handler)
        0x8A => {
            return ScsiResult::Status { status: 0x00 };
        }
        // SERVICE ACTION IN (16)
        0x9E => {
            return scsi::handle_scsi_command(cdb, backend, cache, block_size, active_luns, lun_id);
        }
        // All other commands → delegate to shared handler
        _ => scsi::handle_scsi_command(cdb, backend, cache, block_size, active_luns, lun_id),
    }
}
