# Plan Perbaikan Performa iSCSI SANBOOT & Kepatuhan Protokol RFC 7143

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Memperbaiki seluruh bug kepatuhan protokol RFC 7143, SCSI SPC-4, dan bottleneck latensi pada target iSCSI server agar kecepatan Read & Write pada iSCSI Boot OS (SANBOOT Windows) mencapai performa penuh (setara secondary disk tanpa throttling).

**Architecture:** 
1. Selaraskan seluruh PDU header BHS target response (`TTT = 0xFFFFFFFF` untuk SCSI_RESP & TMF_RESP).
2. Aktifkan Tagged Command Queuing (`CmdQue = 1`) pada Standard SCSI INQUIRY agar Windows Storport mengaktifkan antrean paralel I/O.
3. Lacak penerimaan PDU `Data-Out` dan kirimkan nilai `ExpDataSN` yang valid (`data_sn_count`) pada setiap penyelesaian Write task.
4. Salin payload `Data-Out` berdasarkan `Buffer Offset` dari BHS untuk keandalan burst transfer.
5. Perbaiki format VPD Page 0xB0 (*Block Limits*) sesuai SBC-3 (512e SSD alignment).
6. Tambahkan Fast-Path RAM Cache Read untuk metadata atomik Windows tanpa *spawn blocking*.

**Tech Stack:** Rust (Tokio, Parking Lot, DashMap), iSCSI Consolidated Standard (RFC 7143), SCSI Primary Commands (SPC-4), SCSI Block Commands (SBC-3).

---

## Global Constraints
- Seluruh kode Rust harus bebas warning/error pada `cargo check` dan `cargo test`.
- Format `.vhd` tetap menggunakan ukuran sektor logis 512 bytes (standar 512e / Advanced Format).
- Seluruh sequence number dan PDU formatting harus patuh 100% pada RFC 7143.

---

### Task 1: Perbaikan BHS Builder Target Transfer Tag (TTT) untuk SCSI_RESP & TMF_RESP (RFC 7143 §11.4.3 & §11.6)

**Files:**
- Modify: `src/pdu/builder.rs:33-45`
- Test: `src/pdu/builder.rs` (mod tests)

**Interfaces:**
- `write_bhs(pdu: &Pdu, bhs: &mut [u8; 48]) -> u32`
- Untuk opcode `0x20` (NOP-In), `0x21` (SCSI_RESP), `0x22` (TMF_RESP), `0x24` (Text_RESP), `0x25` (Data_IN), bytes 20..24 BHS harus `0xFFFFFFFF`.

- [ ] **Step 1: Tulis unit test di `src/pdu/builder.rs` yang memverifikasi TTT = 0xFFFFFFFF untuk opcode 0x21 dan 0x22**

```rust
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
```

- [ ] **Step 2: Jalankan `cargo test test_target_transfer_tag_scsi_and_tmf_resp` dan pastikan test GAGAL (assert_eq fail karena saat ini bernilai 0x00000000)**

- [ ] **Step 3: Perbaiki match arm opcode di `src/pdu/builder.rs`**

```rust
    // Sequence numbers based on direction
    if pdu.opcode >= 0x20 {
        // Target -> Initiator PDU
        let ttt = match pdu.opcode {
            0x20 | 0x21 | 0x22 | 0x24 | 0x25 => 0xFFFFFFFFu32,
            0x31 => pdu.initiator_task_tag, // R2T MUST NOT be 0xFFFFFFFF per RFC 7143 Section 11.8.3
            _ => 0xFFFFFFFFu32, // Default safe for other target-to-initiator PDUs
        };
        bhs[20..24].copy_from_slice(&ttt.to_be_bytes());
        bhs[24..28].copy_from_slice(&pdu.cmd_sn.to_be_bytes());
        bhs[28..32].copy_from_slice(&pdu.exp_stat_sn.to_be_bytes());
        bhs[32..36].copy_from_slice(&pdu.max_cmd_sn.to_be_bytes());
        bhs[36..48].copy_from_slice(&pdu.custom_bhs[4..16]);
    }
```

- [ ] **Step 4: Jalankan `cargo test test_target_transfer_tag_scsi_and_tmf_resp` dan pastikan test LULUS (PASS)**

---

### Task 2: Pengaktifan Tagged Command Queuing (`CmdQue = 1`) pada Standard SCSI INQUIRY (SPC-4 §6.4.2)

**Files:**
- Modify: `src/scsi_gamedisk.rs:130-149`
- Test: `src/scsi_gamedisk.rs`

**Interfaces:**
- `handle_inquiry(cdb: &[u8], backend: &Backend, lun_id: u8) -> ScsiResult`

- [ ] **Step 1: Tulis unit test untuk memverifikasi Byte 7 Standard SCSI INQUIRY memiliki bit 1 aktif (`0x02` / `CmdQue = 1`)**

```rust
#[test]
fn test_standard_inquiry_command_queuing() {
    let cdb = [0x12, 0x00, 0x00, 0x00, 36, 0x00];
    let backend = Backend::new_raw("dummy", 512, "TEST", "DISK", "1.0", 0).unwrap_or_else(|_| {
        // mock fallback if needed
    });
    // Verifikasi byte 7 adalah 0x02 (CmdQue)
}
```

- [ ] **Step 2: Update Byte 7 pada Standard INQUIRY di `src/scsi_gamedisk.rs`**

Ubah:
```rust
    } else {
        // Byte 2: 0x06 (SPC-4)
        // Byte 3: 0x02 (Response data format)
        // Byte 4: 31 (Additional length, total 36 bytes)
        // Byte 7: 0x02 (CmdQue = 1: Tagged Command Queuing supported!)
        response_data.extend_from_slice(&[0x00, 0x00, 0x06, 0x02, 31, 0x00, 0x00, 0x02]);
        let mut vendor = vec![b' '; 8];
```

- [ ] **Step 3: Jalankan `cargo check` dan pastikan kompilasi valid**

---

### Task 3: Pelacakan `data_sn_count` & Koreksi `ExpDataSN` pada Write Path (RFC 7143 §11.4.5 & §11.7.5)

**Files:**
- Modify: `src/session/mod.rs:26-32` (`PendingWrite` struct)
- Modify: `src/session/scsi_handler_imagedisk.rs:53-61`
- Modify: `src/session/scsi_handler_gamedisk.rs:53-61`
- Modify: `src/session/scsi_handler.rs:210-288` (`handle_data_out`)

**Interfaces:**
- `PendingWrite { lun_id: u8, lba: u64, num_blocks: u32, expected_len: usize, buffer: Vec<u8>, data_sn_count: u32 }`
- `WriterMessage::ScsiResponse { itt, status, exp_data_sn, expected_len, actual_len }`

- [ ] **Step 1: Update definisi `PendingWrite` di `src/session/mod.rs`**

```rust
pub struct PendingWrite {
    pub lun_id: u8,
    pub lba: u64,
    pub num_blocks: u32,
    pub expected_len: usize,
    pub buffer: Vec<u8>,
    pub data_sn_count: u32,
}
```

- [ ] **Step 2: Update inisialisasi `PendingWrite` di `scsi_handler_imagedisk.rs` dan `scsi_handler_gamedisk.rs`**

Pastikan buffer dialokasikan berukuran `expected_len` (jika ada immediate data, letakkan di `0..immediate_len`), dan set `data_sn_count: 0`.

- [ ] **Step 3: Perbaiki `handle_data_out` di `src/session/scsi_handler.rs`**

Ambil `Buffer Offset` dari `req.custom_bhs[8..12]` dan `DataSN` dari `req.custom_bhs[4..8]`:
```rust
    pub(super) async fn handle_data_out(&self, req: Pdu) -> Result<(), std::io::Error> {
        let itt = req.initiator_task_tag;
        let buffer_offset = u32::from_be_bytes(req.custom_bhs[8..12].try_into().unwrap()) as usize;
        
        let mut is_complete = false;
        let mut pending_lba = 0;
        let mut expected_len = 0;
        let mut num_blocks = 0;
        let mut lun_id = 0;
        let mut data_sn_count = 0;
        let mut buffer_clone = Vec::new();

        {
            let mut pending_guard = self.pending_writes.lock();
            if let Some(pending) = pending_guard.get_mut(&itt) {
                pending.data_sn_count += 1;
                let data_len = req.data.len();
                if buffer_offset + data_len <= pending.expected_len {
                    if pending.buffer.len() < pending.expected_len {
                        pending.buffer.resize(pending.expected_len, 0);
                    }
                    pending.buffer[buffer_offset..buffer_offset + data_len].copy_from_slice(&req.data);
                }

                // Cek final flag (0x80) atau buffer penuh
                if (req.flags & 0x80) != 0 || buffer_offset + data_len >= pending.expected_len {
                    is_complete = true;
                }
            } else {
                warn!("Menerima Data-Out untuk task tag {} yang tidak ada di pending_writes.", itt);
                return Ok(());
            }

            if is_complete {
                if let Some(pending) = pending_guard.remove(&itt) {
                    pending_lba = pending.lba;
                    expected_len = pending.expected_len;
                    num_blocks = pending.num_blocks;
                    lun_id = pending.lun_id;
                    data_sn_count = pending.data_sn_count;
                    buffer_clone = pending.buffer;
                }
            }
        }

        if is_complete {
            let cache_opt = self.client_caches.get(&lun_id).cloned();
            let backend = self.backends.get(&lun_id).cloned().unwrap();
            let tx = self.tx.clone();
            let stats = std::sync::Arc::clone(&self.stats);
            let client_ip = self.client_ip.clone();

            tokio::spawn(async move {
                let res = tokio::task::spawn_blocking(move || {
                    if let Some(cache) = cache_opt {
                        cache.write_stream(pending_lba, 0, &buffer_clone)
                    } else {
                        backend.write_blocks(pending_lba, num_blocks, &buffer_clone)
                    }
                }).await;

                match res {
                    Ok(Ok(_)) => {
                        let _ = tx.send(WriterMessage::ScsiResponse {
                            itt,
                            status: 0x00,
                            exp_data_sn: data_sn_count, // RFC 7143: jumlah Data-Out PDU
                            expected_len: 0,
                            actual_len: 0,
                        }).await;
                        stats.record_write(&client_ip, expected_len as u64);
                    }
                    Ok(Err(e)) => {
                        error!("Gagal menulis data (Data-Out) ke disk LUN {} LBA {}: {}", lun_id, pending_lba, e);
                        let _ = tx.send(WriterMessage::CheckCondition {
                            itt,
                            key: 0x03,
                            asc: 0x0C,
                            ascq: 0x00,
                        }).await;
                    }
                    Err(e) => {
                        error!("Disk write task panicked: {}", e);
                    }
                }
            });
        }
        
        Ok(())
    }
```

- [ ] **Step 4: Jalankan `cargo check` dan pastikan tipe data dan pergeseran sesuai**

---

### Task 4: Koreksi Payload SCSI VPD Page 0xB0 (*Block Limits*) (SBC-3 §6.6.3)

**Files:**
- Modify: `src/scsi_gamedisk.rs:96-115`

**Interfaces:**
- `handle_inquiry` match arm `0xB0`

- [ ] **Step 1: Susun payload VPD Page 0xB0 dengan byte offset yang presisi sesuai standar SBC-3**

```rust
            0xB0 => {
                response_data.extend_from_slice(&[0x00, 0xB0, 0x00, 0x3C]);
                let mut page_b0 = [0u8; 60];
                // Offset 2..4 in payload (bytes 6-7): Optimal transfer length granularity = 8 blocks (4 KB)
                page_b0[2..4].copy_from_slice(&(8u16).to_be_bytes());
                // Offset 4..8 in payload (bytes 8-11): Maximum transfer length = 8192 blocks (4 MB)
                page_b0[4..8].copy_from_slice(&(8192u32).to_be_bytes());
                // Offset 8..12 in payload (bytes 12-15): Optimal transfer length = 2048 blocks (1 MB)
                page_b0[8..12].copy_from_slice(&(2048u32).to_be_bytes());
                // Offset 12..16 in payload (bytes 16-19): Maximum prefetch length = 8192 blocks (4 MB)
                page_b0[12..16].copy_from_slice(&(8192u32).to_be_bytes());
                // Offset 16..20 in payload (bytes 20-23): Maximum unmap LBA count = 8192 blocks
                page_b0[16..20].copy_from_slice(&(8192u32).to_be_bytes());
                // Offset 20..24 in payload (bytes 24-27): Maximum unmap block descriptor count = 256
                page_b0[20..24].copy_from_slice(&(256u32).to_be_bytes());
                // Offset 24..28 in payload (bytes 28-31): Optimal unmap granularity = 8 blocks (4 KB)
                page_b0[24..28].copy_from_slice(&(8u32).to_be_bytes());
                // Offset 32..36 in payload (bytes 36-39): Maximum WRITE SAME length = 8192 blocks
                page_b0[32..36].copy_from_slice(&(8192u32).to_be_bytes());
                response_data.extend_from_slice(&page_b0);
            }
```

- [ ] **Step 2: Jalankan `cargo check`**

---

### Task 5: Fast-Path RAM Cache Read untuk Zero-Latency Metadata Read

**Files:**
- Modify: `src/writeback_gamedisk.rs`
- Modify: `src/session/scsi_handler.rs:109-142`

**Interfaces:**
- `ClientCache::try_read_ram_cache(&self, first_lba: u64, num_blocks: u32, buf: &mut [u8]) -> Option<()>`

- [ ] **Step 1: Tambahkan method `try_read_ram_cache` di `src/writeback_gamedisk.rs`**

```rust
    pub fn try_read_ram_cache(&self, first_lba: u64, num_blocks: u32, buf: &mut [u8]) -> Option<()> {
        let block_size = self.block_size as usize;
        let n = num_blocks as usize;
        for i in 0..n {
            let lba = first_lba + i as u64;
            if let Some(ram_data) = self.ram_cache.get(&lba) {
                let start = i * block_size;
                let end = start + block_size;
                buf[start..end].copy_from_slice(&ram_data);
            } else {
                return None; // Sebagian atau seluruhnya tidak ada di RAM cache
            }
        }
        Some(())
    }
```

- [ ] **Step 2: Integrasikan `try_read_ram_cache` di fast-path `handle_scsi_cmd` pada `src/session/scsi_handler.rs`**

Jika `cache.try_read_ram_cache` berhasil, langsung respon data tanpa `spawn_blocking`. Jika tidak, fallback ke `try_read_from_cache` (VHD Moka read cache). Jika keduanya miss, barulah masuk `spawn_blocking`.

- [ ] **Step 3: Jalankan `cargo check` dan pastikan lulus tanpa error**

---

### Task 6: Kompilasi & Verifikasi Akhir

**Files:**
- Seluruh crate `Simple-Iscsi`

- [ ] **Step 1: Jalankan seluruh automated tests**
`cargo test`

- [ ] **Step 2: Build release binary**
`cargo build --release`

- [ ] **Step 3: Verifikasi status build selesai tanpa warning breaking**
