# Plan: Perbaikan Logika DHCP/TFTP Separation, Multi-NIC iSCSI Allocation, dan SCSI Vendor Case Sensitivity

**Tanggal:** 2026-10-24
**Status:** Siap Eksekusi

---

## 1. Analisis & Latar Belakang

1. **TFTP Server vs iSCSI Boot Server Separation di DHCP**:
   - **Masalah:** Saat ini `siaddr` (Next Server BOOTP) dan Option 66 (TFTP Server) mengambil IP dari konfigurasi per-client (`client_conf.next_server`). Jika sebuah client dialokasikan ke IP interface sekunder (misal `10.10.10.12` untuk multi-NIC), firmware PXE mencoba menghubungi TFTP ke `10.10.10.12` yang menyebabkan bootloader PXE gagal di-download (TFTP timeout).
   - **Solusi Tepat:**
     - **TFTP Server (`siaddr` & Option 66):** Wajib **selalu 100% menggunakan IP server terpusat** (`[dhcp].next_server` dari `config.toml`, misal `10.10.10.10`), karena servis DHCP dan TFTP berjalan terpusat pada host tersebut.
     - **iSCSI Target Server (Option 168, Option 17 Root-Path, Option 170 GameDisk Root-Path):** Menggunakan `client_conf.next_server` jika ada (misal `10.10.10.12` hasil alokasi multi-NIC), atau fallback ke `[dhcp].next_server`. Dengan cara ini, iSCSI traffic terbagi merata di multi-LAN/NIC tanpa merusak tahap TFTP boot.

2. **Form Klien & Fitur Auto-Balance Multi-NIC (UI)**:
   - Tetap dipertahankan sesuai kebutuhan pengguna.
   - Berikan tooltip/label penjelas pada modal klien: `iSCSI Boot Server IP (Multi-NIC)` agar memperjelas bahwa IP tersebut dialokasikan untuk iSCSI target traffic.

3. **SCSI Vendor Case-Sensitivity di Disk Management**:
   - **Masalah:** Input Vendor di Disk Management dipaksa huruf kapital (Caps Lock) oleh CSS class `uppercase` di `ui/index.html` dan method `.toUpperCase()` di `ui/index.js`. Hal ini mencegah penulisan nama vendor mixed-case seperti `CCBoot` (yang dibutuhkan driver CCBootPNPX/CCacheX) atau `Samsung`, `Intel`, dsb.
   - **Solusi:** Hapus class `uppercase` dari input `#disk-vendor-os` dan `#disk-vendor-game` di `ui/index.html`, serta hapus pemanggilan `.toUpperCase()` di `updateDiskBrandingPreview()` dan `saveDiskBrandingAction()` pada `ui/index.js`.

---

## 2. Rincian Perubahan File

### A. Backend (`src/netboot/dhcp.rs`)
- Pisahkan pembacaan IP server menjadi 2 variabel:
  1. `tftp_server_ip_str` & `tftp_server_ip`: Diambil murni dari `self.config.read().dhcp.as_ref().unwrap().next_server`. Digunakan untuk:
     - `resp.siaddr = tftp_server_ip`
     - Option 66 (TFTP Server Name)
  2. `iscsi_server_str`: Diambil dari `client_conf.next_server` (jika diset dan tidak kosong), atau fallback ke `tftp_server_ip_str`. Digunakan untuk:
     - Option 168 (iSCSI Target IP)
     - Option 17 (Root-Path: `iscsi:<iscsi_server_str>::<port>:0:<iscsi_iqn>`)
     - Option 170 (GameDisk Root-Path: `iscsi:<iscsi_server_str>::<port>:0:<gd_iqn>`)

### B. Frontend HTML (`ui/index.html`)
- Hapus class `uppercase` pada input:
  - `#disk-vendor-os` (baris ~331)
  - `#disk-vendor-game` (baris ~342)
- Berikan label penjelas pada form modal klien:
  - Label `Next Server (Server IP)` diperjelas menjadi `iSCSI Boot Server IP (Multi-NIC / Next Server)`.

### C. Frontend JavaScript (`ui/index.js`)
- Di fungsi `updateDiskBrandingPreview()`:
  - Ganti `.trim().toUpperCase()` menjadi `.trim()` untuk `osVendor` dan `gmVendor`.
- Di fungsi `saveDiskBrandingAction()`:
  - Ganti `.trim().toUpperCase()` menjadi `.trim()` untuk `osVendor` dan `gmVendor`.

---

## 3. Langkah Verifikasi
1. Rebuild CSS Tailwind: `npm run build:css`
2. Jalankan `cargo check` dan `cargo test` (pastikan seluruh 33 test lulus).
3. Verifikasi file `config.toml` dan `clients.toml` tetap utuh dan aman.