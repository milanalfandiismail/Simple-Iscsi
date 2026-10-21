# VHD Compatibility & CCBoot Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menyamakan kompatibilitas penanganan VHD pada `Simple-Iscsi` agar seluruh varian VHD (Dynamic, Differencing, Fixed, serta hasil konversi pihak ketiga) dapat booting 100% sempurna tanpa crash seperti pada CCBoot.

**Architecture:** 
1. **VHD Format Engine Refactor (`src/vhd.rs`)**: Memperbaiki deteksi footer ganda (offset 0 vs EOF-512), mendukung Dynamic Block Size variabel, dan mengimplementasikan **Sector Bitmap Granular Checking** pada pembacaan Differencing VHD (`diff_read_blocks`).
2. **SCSI Emulation Parity (`src/session/scsi_handler_imagedisk.rs` & `src/scsi_imagedisk.rs`)**: Menyelaraskan respon SPC-4/SBC-3 (INQUIRY Standard, VPD Page 0x80, VPD Page 0x83, VPD Page 0xB0 Block Limits, READ CAPACITY 10/16) dengan standar Windows Storport/SANBOOT.
3. **Safety Fallback & CoW Consistency**: Memastikan sinkronisasi buffer cache dan bitmap state saat pembacaan sektor campuran (child vs parent).

**Tech Stack:** Rust (Tokio, byteorder/std bitwise, DashMap, Moka, std::fs::File)

---

## Global Constraints

- **Platform:** Windows 10/11 x64 (Native iSCSI Storport compatibility).
- **Protokol:** RFC 7143 iSCSI, Microsoft Virtual Hard Disk Image Format Specification v1.0.
- **SCSI Standards:** SPC-4 (SCSI Primary Commands) & SBC-3 (SCSI Block Commands).
- **Zero Third-Party Client Drivers:** Tetap mempertahankan arsitektur 100% native driverless Windows client.

---

### Task 1: Audit & Refactor Deteksi Header/Footer VHD (Fixed & Dynamic Support)

**Files:**
- Modify: `src/vhd.rs:40-110`
- Test: `tests/test_vhd_detection.rs` (atau unit test di `src/vhd.rs`)

**Interfaces:**
- Consumes: `std::fs::File`
- Produces: `VhdBackend::open(file: File) -> io::Result<Self>` yang mampu membuka baik Dynamic VHD (footer di offset 0 dan EOF-512) maupun Fixed VHD (footer hanya di EOF-512).

- [x] **Step 1: Tulis unit test untuk verifikasi deteksi footer**
Buat test case yang memvalidasi pembacaan footer di EOF ketika offset 0 berisi data (Fixed VHD) dan ketika offset 0 berisi copy footer (Dynamic VHD).

- [x] **Step 2: Jalankan test untuk memastikan kondisi gagal pada Fixed VHD**
`cargo test test_vhd_detection`
Ekspektasi: Gagal karena saat ini `vhd.rs` hanya membaca `SeekFrom::Start(0)`.

- [x] **Step 3: Implementasikan logika deteksi dual-location footer di `src/vhd.rs`**
Periksa footer di `EOF - 512` terlebih dahulu (posisi kanonikal resmi Microsoft). Jika valid, periksa `disk_type` (2 = Fixed, 3 = Dynamic, 4 = Differencing). Jika Dynamic/Diff, baca header di offset 512 (atau offset yang ditunjuk oleh `data_offset` footer).

- [x] **Step 4: Jalankan test kembali**
`cargo test test_vhd_detection`
Ekspektasi: PASS.

- [x] **Step 5: Commit perubahan**
```bash
git add src/vhd.rs
git commit -m "fix(vhd): support dual-location footer detection for fixed and dynamic VHDs"
```

---

### Task 2: Implementasi Sector Bitmap Check pada Differencing VHD (`diff_read_blocks`)

**Akar Masalah:**
Saat ini `diff_read_blocks` hanya memeriksa apakah `child_bat != 0xFFFFFFFF`. Jika teralokasi, seluruh blok 2 MB langsung dibaca dari child. Padahal spesifikasi Microsoft VHD menyatakan bahwa blok differencing hanya berisi sektor yang bit-nya bernilai `1` pada Sector Bitmap. Sektor dengan bit `0` **harus dibaca dari parent disk**. Jika dibaca dari child, sektor yang belum pernah ditulis mengembalikan nilai nol/korup sehingga Windows gagal boot (`INACCESSIBLE_BOOT_DEVICE`).

**Files:**
- Modify: `src/vhd.rs:320-375`
- Test: `tests/test_vhd_diff_bitmap.rs`

**Interfaces:**
- Consumes: `child.file`, `parent.file`, `bat`, `sector_bitmap_size`
- Produces: `diff_read_blocks` dengan resolusi per-sektor (512-byte granularity) berdasarkan bitmask Sector Bitmap.

- [x] **Step 1: Tulis unit test untuk skenario partial-written block**
Buat child VHD, tulis hanya 1 sektor (misal sektor 10), lalu baca sektor 0 sampai 20. Pastikan sektor 10 terbaca dari child, dan sektor lainnya terbaca dari parent (bukan nol).

- [x] **Step 2: Verifikasi test gagal pada implementasi lama**
`cargo test test_vhd_diff_bitmap`
Ekspektasi: Sektor non-10 mengembalikan nol atau korup.

- [x] **Step 3: Implementasikan fungsi pembacaan bit Sector Bitmap**
Tambahkan helper `is_sector_allocated_in_child(file, bat_entry, sector_in_block) -> bool`:
Membaca byte bitmap pada `(bat_entry * 512) + (sector_in_block / 8)`. Periksa bit `(0x80 >> (sector_in_block % 8))`.

- [x] **Step 4: Refactor loop `diff_read_blocks` di `src/vhd.rs`**
Untuk setiap rentang sektor dalam request:
- Jika `child_bat == 0xFFFFFFFF`: Baca langsung dari parent.
- Jika `child_bat != 0xFFFFFFFF`: Baca sector bitmap blok tersebut.
  - Sektor dengan bit `1`: Baca dari child disk (`bat_entry * 512 + bitmap_size + offset`).
  - Sektor dengan bit `0`: Baca dari parent disk.

- [x] **Step 5: Jalankan test dan verifikasi**
`cargo test test_vhd_diff_bitmap`
Ekspektasi: PASS.

- [x] **Step 6: Commit perubahan**
```bash
git add src/vhd.rs
git commit -m "fix(vhd): implement sector bitmap granular resolution in diff_read_blocks"
```

---

### Task 3: Penyelarasan Respon SCSI INQUIRY & VPD Pages dengan Standar Storport (CCBoot Parity)

**Files:**
- Modify: `src/session/scsi_handler_imagedisk.rs`
- Modify: `src/scsi_imagedisk.rs`
- Test: `tests/test_scsi_inquiry.rs`

**Interfaces:**
- Consumes: CDB SCSI Command (Opcode 0x12 INQUIRY)
- Produces: Respon SCSI SPC-4 terstandarisasi untuk Page 0x00 (Supported Pages), 0x80 (Serial Number), 0x83 (Device Identification), dan 0xB0 (Block Limits).

- [x] **Step 1: Tulis test untuk validasi VPD Page 0xB0 dan 0x83**
Pastikan respon SCSI INQUIRY mendukung Page 0xB0 dengan Max Transfer Length yang sesuai batas VHD block size (maksimal 4 MB atau kelipatan blok VHD) dan sektor granularity 512 byte / 4096 byte.

- [x] **Step 2: Jalankan test verifikasi kegagalan jika ada field yang belum terpenuhi**
`cargo test test_scsi_inquiry`

- [x] **Step 3: Implementasikan respon VPD yang presisi di `scsi_handler_imagedisk.rs`**
Pastikan:
- `CmdQue = 1` pada Standard INQUIRY (mengizinkan tagged command queuing 32-64).
- VPD 0x83 mengembalikan EUI-64 / NAA IEEE identifier yang konsisten per VHD UUID.
- VPD 0xB0 mengembalikan `Optimal Transfer Length Granularity = 8` (4K alignment) dan `Maximum Transfer Length = 8192 sectors` (4MB).

- [x] **Step 4: Verifikasi test lulus**
`cargo test test_scsi_inquiry`
Ekspektasi: PASS.

- [x] **Step 5: Commit perubahan**
```bash
git add src/session/scsi_handler_imagedisk.rs src/scsi_imagedisk.rs
git commit -m "feat(scsi): harmonize SCSI INQUIRY and VPD pages for Storport boot compliance"
```

---

### Task 4: Verifikasi End-to-End dengan Image VHD

**Files:**
- Verify: Server boot loop menggunakan `cargo run`
- Test: Booting client pada target IQN ImageDisk

- [x] **Step 1: Jalankan kompilasi proyek secara utuh**
`cargo check` & `cargo test`
Ekspektasi: Zero error, zero warning.

- [x] **Step 2: Validasi integritas pembacaan VHD master dan differencing**
Pastikan tidak ada kepanikan / panic error saat client melakukan inisialisasi boot awal.

- [x] **Step 3: Dokumentasikan hasil perbaikan di `DOCUMENTATION.md`**
Update catatan teknis VHD handler pada BAB 6 DOCUMENTATION.md.
