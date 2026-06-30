//! SCSI handler khusus ImageDisk (Windows iSCSI sanboot)
//! Menangani MODE SENSE, VERIFY, START STOP UNIT, dll yang dibutuhkan Windows Boot Manager

use crate::backend::Backend;
use crate::cache::ClientCache;
use crate::pdu_image;
use crate::scsi::{self, ScsiResult};
use tracing::info;

/// ImageDisk SCSI command handler — intercepts Windows-specific commands
/// before delegating to shared scsi.rs
pub fn handle_imagedisk_scsi(
    cdb: &[u8],
    backend: &Backend,
    cache: Option<&ClientCache>,
    block_size: u64,
    active_luns: &[u8],
) -> ScsiResult {
    let opcode = cdb[0];

    // === MODE SENSE (6) — Windows extended support ===
    if opcode == 0x1A {
        let page_code = cdb[2] & 0x3F;
        let alloc_len = cdb[4] as usize;
        info!(
            "ImageDisk MODE SENSE (6) page=0x{:02X} alloc_len={}",
            page_code, alloc_len
        );
        let mut data = pdu_image::build_mode_sense_6(page_code, block_size);
        if data.len() > alloc_len {
            data.truncate(alloc_len);
        }
        return ScsiResult::Data { data, status: 0x00 };
    }

    // === MODE SENSE (10) — Windows extended support ===
    if opcode == 0x5A {
        let page_code = cdb[2] & 0x3F;
        let alloc_len = u16::from_be_bytes([cdb[7], cdb[8]]) as usize;
        info!(
            "ImageDisk MODE SENSE (10) page=0x{:02X} alloc_len={}",
            page_code, alloc_len
        );
        let data = pdu_image::build_mode_sense_10(page_code, block_size, alloc_len);
        return ScsiResult::Data { data, status: 0x00 };
    }

    // === VERIFY (10) — Windows sends this, no-op for VHD ===
    if opcode == 0x2F {
        info!("ImageDisk VERIFY (10) → OK (no-op)");
        return ScsiResult::Status { status: 0x00 };
    }

    // === START STOP UNIT — OK ===
    if opcode == 0x1B {
        info!("ImageDisk START STOP UNIT → OK");
        return ScsiResult::Status { status: 0x00 };
    }

    // === READ (16) — needed for large VHD disks ===
    if opcode == 0x88 {
        let lba = u64::from_be_bytes(cdb[2..10].try_into().unwrap());
        let num_blocks = u32::from_be_bytes(cdb[10..14].try_into().unwrap());
        let total_bytes = (num_blocks as u64 * block_size) as usize;
        info!(
            "ImageDisk READ (16) LBA={} blocks={} total_bytes={}",
            lba, num_blocks, total_bytes
        );
        return scsi::handle_scsi_command(cdb, backend, cache, block_size, active_luns);
    }

    // === WRITE (16) — needed for large VHD disks ===
    if opcode == 0x8A {
        let lba = u64::from_be_bytes(cdb[2..10].try_into().unwrap());
        let num_blocks = u32::from_be_bytes(cdb[10..14].try_into().unwrap());
        info!(
            "ImageDisk WRITE (16) LBA={} blocks={}",
            lba, num_blocks
        );
        // WRITE(16) already handled by handle_scsi_cmd dispatch (line 144)
        return ScsiResult::Status { status: 0x00 };
    }

    // === SERVICE ACTION IN (16) — read capacity 16 support ===
    if opcode == 0x9E {
        return scsi::handle_scsi_command(cdb, backend, cache, block_size, active_luns);
    }

    // === All other commands — delegate to shared handler ===
    // Covers: INQUIRY (0x12), READ CAPACITY (0x25), REPORT LUNS (0xA0),
    //         READ (0x28), SYNCHRONIZE CACHE (0x35), PREVENT ALLOW (0x1E)
    scsi::handle_scsi_command(cdb, backend, cache, block_size, active_luns)
}
