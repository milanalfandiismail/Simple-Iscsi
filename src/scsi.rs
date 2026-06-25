use crate::backend::Backend;
use crate::cache::ClientCache;
use tracing::{warn, error};

#[derive(Debug)]
pub enum ScsiResult {
    Status { status: u8 },
    Data { data: Vec<u8>, status: u8 },
    CheckCondition { key: u8, asc: u8, ascq: u8 },
}

pub fn handle_scsi_command(
    cdb: &[u8; 16],
    backend: &Backend,
    cache: Option<&ClientCache>,
    block_size: u64,
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
            let alloc_len = u16::from_be_bytes([cdb[3], cdb[4]]) as usize;

            let mut response_data = Vec::new();

            if evpd {
                match page_code {
                    // Supported EVPD Pages
                    0x00 => {
                        response_data.push(0x00); // Peripheral Qualifier + Device Type
                        response_data.push(0x00); // Page Code
                        response_data.push(0x00); // Reserved
                        response_data.push(0x08); // Page length (8) — match page count
                        response_data.extend_from_slice(&[0x00, 0x80, 0x83, 0xB0, 0xB1, 0xB2]);
                    }
                    // Unit Serial Number
                    0x80 => {
                        response_data.push(0x00);
                        response_data.push(0x80);
                        response_data.push(0x00);
                        response_data.push(0x08); // Page length (8)
                        response_data.extend_from_slice(b"RUST1234"); // Serial number ASCII
                    }
                    // Block Device Characteristics VPD (0xB1) — SSD detection
                    0xB1 => {
                        response_data.push(0x00);           // Peripheral Qualifier + Device Type
                        response_data.push(0xB1);           // Page Code
                        response_data.push(0x00);           // Reserved
                        response_data.push(0x3C);           // Page Length = 60 (0x3C) — standard length

                        // Bytes 4-5: Medium Rotation Rate
                        response_data.extend_from_slice(&[0x00, 0x01]); // 0x0001 = Non-rotating (SSD)

                        // Bytes 6-7: Reserved
                        response_data.extend_from_slice(&[0x00, 0x00]);

                        // Bytes 8-9: Nominal Rotation Rate (sama, untuk redundansi)
                        response_data.extend_from_slice(&[0x00, 0x01]);

                        // Sisanya diisi 0 (total page length 60 bytes)
                        response_data.extend_from_slice(&[0u8; 52]);
                    }
                    // Block Limits VPD (0xB0)
                    0xB0 => {
                        response_data.push(0x00);
                        response_data.push(0xB0);
                        response_data.push(0x00);
                        response_data.push(0x10); // Page length (16)
                        response_data.extend_from_slice(&[0; 16]); // Semua 0 = no limits
                    }
                    // Thin Provisioning VPD (0xB2)
                    0xB2 => {
                        response_data.push(0x00);
                        response_data.push(0xB2);
                        response_data.push(0x00);
                        response_data.push(0x04); // Page length (4)
                        // Byte 4 bit 5: TPE (Thin Provisioning Enabled) = 0
                        // Byte 4 bit 4: TPU (Thin Provisioning Unmap) = 0
                        response_data.extend_from_slice(&[0, 0, 0, 0]);
                    }
                    // Device Identification
                    0x83 => {
                        response_data.push(0x00);
                        response_data.push(0x83);
                        response_data.push(0x00);
                        response_data.push(0x14); // Page length (20)
                        
                        // Descriptor 1: T10 Vendor ID
                        response_data.push(0x02); // Code set: ASCII
                        response_data.push(0x01); // Association: LU, Designator: Vendor specific
                        response_data.push(0x00); // Reserved
                        response_data.push(0x10); // Designator length (16)
                        response_data.extend_from_slice(b"RUST_ISCSI_DRV00");
                    }
                    _ => {
                        warn!("EVPD page tidak didukung: 0x{:02X}", page_code);
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

                response_data.push(31);   // Additional Length (31 → total 36 bytes)

                // Flags penting
                response_data.push(0x00); // SCCS=0, ACC=0, TPGS=0, etc.
                response_data.push(0x00); // 3PC=0, PROTECT=0, etc.
                response_data.push(0x00); // BQUE=0, VS=0, etc. (byte 7)
                
                // Vendor ID (8 bytes)
                response_data.extend_from_slice(b"RUSTISCS");
                
                // Product ID (16 bytes)
                response_data.extend_from_slice(b"GameDiskCache   ");
                
                // Product Revision Level (4 bytes)
                response_data.extend_from_slice(b"1.00");
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
            let mut data = vec![0u8; total_bytes as usize];

            // Bulk read — coba dari cache dulu
            let cache_hit = cache.and_then(|c| c.read_blocks(lba, num_blocks, &mut data));
            match cache_hit {
                Some(Ok(())) => {}
                Some(Err(e)) => {
                    error!("Gagal membaca cache untuk LBA {}: {}", lba, e);
                    return ScsiResult::CheckCondition {
                        key: 0x03,
                        asc: 0x11,
                        ascq: 0x00,
                    };
                }
                None => {
                    // Cache miss → baca dari backend (bulk, 1 seek+1 read)
                    if let Err(e) = backend.read_blocks(lba, num_blocks, &mut data) {
                        error!("Gagal membaca disk backend untuk LBA {}: {}", lba, e);
                        return ScsiResult::CheckCondition {
                            key: 0x03,
                            asc: 0x11,
                            ascq: 0x00,
                        };
                    }
                }
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
            let mut data = vec![0u8; 16];
            data[3] = 8; // LUN list length = 8 bytes
            // LUN 0 = 0 (data[8..16] tetap 0)
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

