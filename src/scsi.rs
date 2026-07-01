use crate::backend::Backend;
use crate::writeback_gamedisk::ClientCache;
use tracing::{warn, error};

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
        // TEST UNIT READY
        0x00 => {
            ScsiResult::Status { status: 0x00 }
        }

        // REQUEST SENSE
        0x03 => {
            let mut data = vec![0u8; 18];
            data[0] = 0x70; // Fixed format, current errors
            data[2] = 0x00; // Sense key: No Sense
            data[7] = 0x0A; // Additional sense length
            ScsiResult::Data { data, status: 0x00 }
        }

        // INQUIRY
        0x12 => {
            let evpd = (cdb[1] & 0x01) != 0;
            let page_code = cdb[2];
            let alloc_len = cdb[4] as usize;

            let mut response_data = Vec::new();

            if evpd {
                // Vital Product Data pages
                match page_code {
                    0x00 => {
                        // Supported VPD Pages
                        response_data.push(0x00); // Peripheral Device Type
                        response_data.push(0x00); // Page Code: 0x00
                        response_data.push(0x00); // Reserved
                        response_data.push(5);    // Page Length (5 pages)

                        response_data.push(0x00); // Supported VPD Pages
                        response_data.push(0x83); // Device Identification
                        response_data.push(0xB0); // Block Limits VPD
                        response_data.push(0xB1); // Block Device Characteristics VPD
                        response_data.push(0xB2); // Thin Provisioning VPD
                    }
                    0x83 => {
                        // Device Identification
                        response_data.push(0x00); // Peripheral Device Type
                        response_data.push(0x83); // Page Code
                        response_data.push(0x00); // Reserved
                        response_data.push(12);   // Page Length

                        // Designation Descriptor #1: Vendor Specific (SCSI name)
                        response_data.push(0x01); // Code Set: Binary
                        response_data.push(0x03); // Designator Type: NAA
                        response_data.push(0x00); // Reserved
                        response_data.push(8);    // Designator Length
                        response_data.extend_from_slice(&[
                            // NAA IEEE Registered Extended, unique per LUN
                            0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, lun_id,
                        ]);
                    }
                    0xB0 => {
                        // Block Limits VPD — minimal valid response
                        response_data.push(0x00); // Device Type
                        response_data.push(0xB0); // Page Code
                        response_data.push(0x00); // Reserved
                        response_data.push(0x3C); // Page Length (60 bytes)
                        response_data.extend_from_slice(&[0u8; 60]); // Semua zero = default/no special limits
                    }
                    0xB1 => {
                        // Block Device Characteristics VPD — SSD detection
                        // Windows cek byte 4-5: 0x0001 = Non-rotating (SSD)
                        response_data.push(0x00); // Device Type
                        response_data.push(0xB1); // Page Code
                        response_data.push(0x00); // Reserved
                        response_data.push(0x3C); // Page Length (60 bytes)
                        // Medium Rotation Rate = 0x0001 (Non-rotating medium / SSD)
                        response_data.push(0x00);
                        response_data.push(0x01);
                        response_data.extend_from_slice(&[0u8; 58]); // Sisanya zeros
                    }
                    0xB2 => {
                        // Thin Provisioning VPD — inform client
                        response_data.push(0x00); // Device Type
                        response_data.push(0xB2); // Page Code
                        response_data.push(0x00); // Reserved
                        response_data.push(0x04); // Page Length (4 bytes)
                        // Threshold exponent (0) + flags: no unmap, no write same, etc.
                        response_data.extend_from_slice(&[0u8; 4]);
                    }
                    _ => {
                        return ScsiResult::CheckCondition {
                            key: 0x05,  // Illegal Request
                            asc: 0x24,  // Invalid field in CDB
                            ascq: 0x00,
                        };
                    }
                }
            } else {
                // Standard Inquiry Data
                response_data.push(0x00); // Device Type: Direct Access Block Device
                response_data.push(0x00); // RMB = 0 (Non-removable)

                response_data.push(0x06); // Version: SPC-4 (lebih modern)
                response_data.push(0x02); // Response Data Format (RDF=2, SPC-2+)

                response_data.push(31);   // Additional Length (31 — total 36 bytes)

                // Flags penting
                response_data.push(0x00); // SCCS=0, ACC=0, TPGS=0, etc.
                response_data.push(0x00); // 3PC=0, PROTECT=0, etc.
                response_data.push(0x00); // BQUE=0, VS=0, etc. (byte 7)
                
                // Vendor ID (8 bytes)
                let mut vendor = vec![b' '; 8];
                let v_bytes = backend.vendor_id.as_bytes();
                let v_len = std::cmp::min(8, v_bytes.len());
                vendor[..v_len].copy_from_slice(&v_bytes[..v_len]);
                response_data.extend_from_slice(&vendor);
                
                // Product ID (16 bytes)
                let mut product = vec![b' '; 16];
                let p_bytes = backend.product_id.as_bytes();
                let p_len = std::cmp::min(16, p_bytes.len());
                product[..p_len].copy_from_slice(&p_bytes[..p_len]);
                response_data.extend_from_slice(&product);
                
                // Product Revision Level (4 bytes)
                let mut rev = vec![b' '; 4];
                let r_bytes = backend.product_revision.as_bytes();
                let r_len = std::cmp::min(4, r_bytes.len());
                rev[..r_len].copy_from_slice(&r_bytes[..r_len]);
                response_data.extend_from_slice(&rev);
            }

            // Potong sesuai allocation length yang diminta oleh initiator
            if response_data.len() > alloc_len {
                response_data.truncate(alloc_len);
            }

            ScsiResult::Data {
                data: response_data,
                status: 0x00,
            }
        }

        // MODE SENSE (6)
        0x1A => {
            let page_code = cdb[2] & 0x3F;
            let alloc_len = cdb[4] as usize;

            let mut response_data = Vec::new();

            if page_code == 0x08 || page_code == 0x3F {
                // Mode Parameter Header (4 bytes)
                response_data.push(15);   // Mode Data Length (16 bytes total - 1)
                response_data.push(0x00); // Medium Type
                response_data.push(0x00); // Device-Specific Parameter
                response_data.push(0x00); // Block Descriptor Length

                // Caching Page (Page 8) - 12 bytes
                response_data.push(0x08); // Page Code: Caching Page
                response_data.push(0x0A); // Page Length (10 bytes)
                response_data.push(0x04); // Byte 2: Write Cache Enabled (WCE = 1, bit 2 -> 0x04)
                response_data.extend_from_slice(&[0; 9]); // Sisa parameter caching
            } else {
                // Mode Parameter Header kosong (4 bytes)
                response_data.push(3);    // Mode Data Length
                response_data.push(0x00);
                response_data.push(0x00);
                response_data.push(0x00);
            }

            if response_data.len() > alloc_len {
                response_data.truncate(alloc_len);
            }

            ScsiResult::Data {
                data: response_data,
                status: 0x00,
            }
        }

        // MODE SENSE (10)
        0x5A => {
            let page_code = cdb[2] & 0x3F;
            let alloc_len = u16::from_be_bytes([cdb[7], cdb[8]]) as usize;

            let mut response_data = Vec::new();

            if page_code == 0x08 || page_code == 0x3F {
                // Mode Parameter Header (10) - 8 bytes
                response_data.extend_from_slice(&[0, 18]); // Mode Data Length (20 bytes total - 2)
                response_data.push(0x00); // Medium Type
                response_data.push(0x00); // Device-Specific Parameter
                response_data.extend_from_slice(&[0, 0]); // Reserved
                response_data.extend_from_slice(&[0, 0]); // Block Descriptor Length

                // Caching Page (Page 8) - 12 bytes
                response_data.push(0x08); // Page Code: Caching Page
                response_data.push(0x0A); // Page Length (10 bytes)
                response_data.push(0x04); // Byte 2: Write Cache Enabled (WCE = 1)
                response_data.extend_from_slice(&[0; 9]); // Sisa parameter caching
            } else {
                // Mode Parameter Header kosong (8 bytes)
                response_data.extend_from_slice(&[0, 6]); // Mode Data Length (8 bytes total - 2)
                response_data.push(0x00);
                response_data.push(0x00);
                response_data.extend_from_slice(&[0, 0]);
                response_data.extend_from_slice(&[0, 0]);
            }

            if response_data.len() > alloc_len {
                response_data.truncate(alloc_len);
            }

            ScsiResult::Data {
                data: response_data,
                status: 0x00,
            }
        }

        // READ CAPACITY (10)
        0x25 => {
            let total_blocks = backend.total_size() / block_size;
            let max_lba = if total_blocks > 0 { (total_blocks - 1) as u32 } else { 0 };
            let block_len = block_size as u32;

            let mut data = vec![0u8; 8];
            data[0..4].copy_from_slice(&max_lba.to_be_bytes());
            data[4..8].copy_from_slice(&block_len.to_be_bytes());

            ScsiResult::Data { data, status: 0x00 }
        }

        // SERVICE ACTION IN (16) -> READ CAPACITY (16)
        0x9E => {
            let service_action = cdb[1] & 0x1F;
            if service_action == 0x10 {
                let total_blocks = backend.total_size() / block_size;
                let max_lba = if total_blocks > 0 { total_blocks - 1 } else { 0 };
                let block_len = block_size as u32;

                let mut data = vec![0u8; 32];
                data[0..8].copy_from_slice(&max_lba.to_be_bytes());
                data[8..12].copy_from_slice(&block_len.to_be_bytes());

                let alloc_len = u32::from_be_bytes(cdb[10..14].try_into().unwrap()) as usize;
                if data.len() > alloc_len {
                    data.truncate(alloc_len);
                }

                ScsiResult::Data { data, status: 0x00 }
            } else {
                warn!("Service action tidak didukung untuk opcode 0x9E: 0x{:02X}", service_action);
                ScsiResult::CheckCondition {
                    key: 0x05,  // Illegal Request
                    asc: 0x24,  // Invalid field in CDB
                    ascq: 0x00,
                }
            }
        }

        // READ (10)
        0x28 => {
            let lba = u32::from_be_bytes(cdb[2..6].try_into().unwrap()) as u64;
            let num_blocks = u16::from_be_bytes(cdb[7..9].try_into().unwrap()) as u32;

            let total_bytes = (num_blocks as u64) * block_size;
            // SAFETY: read_blocks fills the entire buffer. Zero-init would be wasted
            // memory bandwidth (~91 MB/s at 1 GbE) since every byte gets overwritten.
            let mut data = Vec::with_capacity(total_bytes as usize);
            unsafe { data.set_len(total_bytes as usize); }

            // Baca dari backend (baseline — data asli disk)
            if let Err(e) = backend.read_blocks(lba, num_blocks, &mut data) {
                error!("Gagal membaca disk backend untuk LBA {}: {}", lba, e);
                return ScsiResult::CheckCondition {
                    key: 0x03,
                    asc: 0x11,
                    ascq: 0x00,
                };
            }

            // Overlay data dari cache untuk block yang pernah di-WRITE
            if let Some(cache) = cache {
                if cache.contains_range(lba, num_blocks) {
                    // Fast path: semua cached — baca bulk dari .bin (over backend data)
                    if let Some(Ok(())) = cache.read_blocks(lba, num_blocks, &mut data) {
                        // data sudah terisi dari cache
                    }
                }
                // Partial cache hit (jarang): fallback — baca per-block
                // skip for now — 99% game READ tanpa WRITE sebelumnya
            }

            ScsiResult::Data { data, status: 0x00 }
        }

        // SYNCHRONIZE CACHE (10)
        0x35 => {
            if let Some(c) = cache {
                if let Err(e) = c.flush() {
                    error!("Gagal melakukan sinkronisasi cache: {}", e);
                    return ScsiResult::CheckCondition {
                        key: 0x03,  // Medium Error
                        asc: 0x0C,  // Write error
                        ascq: 0x00,
                    };
                }
            }
            ScsiResult::Status { status: 0x00 }
        }

        // PREVENT ALLOW MEDIUM REMOVAL
        0x1E => {
            ScsiResult::Status { status: 0x00 }
        }

        // REPORT LUNS
        0xA0 => {
            let lun_list_len = (active_luns.len() * 8) as u32;
            let mut data = vec![0u8; 8 + active_luns.len() * 8];
            data[0..4].copy_from_slice(&lun_list_len.to_be_bytes()); // LUN list length

            for (i, &lun_id) in active_luns.iter().enumerate() {
                let offset = 8 + i * 8;
                // Single Level LUN Format
                // Address Method = 00b (Peripheral device addressing method), BUS Identifier = 0
                // LUN is placed in the second byte.
                data[offset] = 0;
                data[offset + 1] = lun_id;
                // rest of 6 bytes are zero
            }

            let alloc_len = u32::from_be_bytes(cdb[6..10].try_into().unwrap()) as usize;
            if data.len() > alloc_len {
                data.truncate(alloc_len);
            }

            ScsiResult::Data { data, status: 0x00 }
        }

        _ => {
            warn!("SCSI command tidak dikenal/didukung: 0x{:02X}", opcode);
            ScsiResult::CheckCondition {
                key: 0x05,  // Illegal Request
                asc: 0x20,  // Invalid command operation code
                ascq: 0x00,
            }
        }
    }
}


