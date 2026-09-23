# Design Spec: iSCSI Sanboot Speed Optimization & BootExecute Tuning

**Date:** 2026-08-22  
**Status:** In Progress / Plan Review  
**Component:** `helper/helper.cpp`, `helper/compile.bat`, `pxe/sb-custom/autoexec.ipxe`  

---

## 1. Masalah & Analisis Teknis (Speed Read Cap 10 MB/s)

Pengguna mengalami kecepatan baca disk (`sanboot`) yang mentok di maksimal **10 MB/s** setelah booting masuk ke Windows.

### Analisis Akar Masalah:
1.  **Konflik Gateway Ganda (Terdeteksi di `ipconfig`):**
    *   `Default Gateway` memiliki 2 entri: `192.168.180.1` dan `192.168.2.1`.
    *   Windows me-load gateway statis lama dari image VHD (`192.168.2.1`) bersamaan dengan gateway aktif (`192.168.180.1`).
    *   Karena paket TCP iSCSI terdistribusi ke gateway mati, terjadi packet drop dan retransmission masif yang menurunkan throughput TCP ke ~10 MB/s.
2.  **Mengapa Regedit Biasa di Desktop Tidak Mempan?**
    *   Koneksi boot iSCSI (`sanboot`) adalah **Boot-Critical Storage Connection**.
    *   Parameter koneksi telah di-load ke memori kernel (`msiscsi.sys` & `storport.sys`) sebelum service desktop Win32 aktif.
    *   Perubahan registry saat Windows berjalan tidak akan mengubah parameter sesi iSCSI boot yang sedang aktif.
3.  **Mengapa `helper.exe` di `BootExecute` Bekerja?**
    *   `helper.exe` berjalan di fase **`smss.exe` / `BootExecute`** (sebelum subsistem Win32 dan network stack mengunci konfigurasi).
    *   Helper dapat membersihkan seluruh gateway lama, mengunci satu gateway aktif, dan menginjeksi parameter transfer rate (256 KB) langsung ke driver class Microsoft iSCSI Initiator.

---

## 2. Rencana Implementasi

### Bagian A: Update `helper.exe` (BootExecute)
1.  **Auto-Resolve Gateway Tunggal:**
    *   Jika `GatewayIP` dari iSharePnp kosong/invalid, helper mengekstrak 3 oktet pertama dari `targetIp` (misal `192.168.180.4`) dan menetapkan gateway ke `.1` (`192.168.180.1`).
    *   Menimpa `DefaultGateway` di semua interfaces `{GUID}` agar gateway lama (`192.168.2.1`) terhapus total.
2.  **Injeksi Parameter Performa iSCSI Initiator:**
    *   Menulis nilai DWORD berikut ke `HKLM\SYSTEM\CurrentControlSet\Control\Class\{4D36E97B-E325-11CE-BFC1-08002BE10318}\0000\Parameters`:
        *   `MaxRecvDataSegmentLength` = `262144` (256 KB)
        *   `MaxTransferLength` = `262144` (256 KB)
        *   `FirstBurstLength` = `262144` (256 KB)
        *   `MaxBurstLength` = `2097152` (2 MB)
        *   `MaxOutstandingR2T` = `16`

### Bagian B: Optimasi iPXE Boot Script (`autoexec.ipxe`)
*   Tambahkan `set gateway 0.0.0.0` sebelum perintah `sanboot` pada `pxe/sb-custom/autoexec.ipxe` agar tabel iBFT dari iPXE tidak menyisipkan gateway lama ke tabel ACPI Windows.
