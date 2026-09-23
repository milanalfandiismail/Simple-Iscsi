# iSCSI Sanboot Speed Optimization & BootExecute Tuning Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Mengatasi batas kecepatan baca 10 MB/s pada sanboot dengan cara: membersihkan konflik gateway ganda via helper di BootExecute, menginjeksi parameter performa iSCSI Initiator (256 KB / 2 MB) ke driver class Windows, dan mengoptimalkan script iPXE.

**Architecture:** Helper berjalan di `BootExecute`, menimpa seluruh gateway statis lama dengan gateway tunggal subnet aktif (`.1`), menyuntikkan parameter performa iSCSI ke instance class `{4D36E97B-E325-11CE-BFC1-08002BE10318}`, dan memperbarui `autoexec.ipxe`.

**Tech Stack:** C++, MSVC Native Subsystem (`/subsystem:native`), NT Native API (`ntdll.dll`), iPXE scripting.

**Spec:** `docs/superpowers/specs/2026-08-22-iscsi-performance-and-sanboot-speed-design.md`

## Global Constraints
- Target Subsistem: `/subsystem:native`
- Entry Point: `NtProcessStartup`
- Tanpa runtime library C/C++ standar (`/NODEFAULTLIB`)
- Bebas memory leak & bebas crash/BSOD

---

### Task 1: Update `helper/helper.cpp` dengan Gateway Auto-Fallback & iSCSI Tuning

**Files:**
- Modify: `helper/helper.cpp`

**Interfaces:**
- Produces:
  - `WriteRegDword`: Fungsi pembantu NT Native API untuk menulis nilai REG_DWORD.
  - Auto-generate gateway `.1` dari `targetIp` jika `GatewayIP` kosong.
  - Injeksi `MaxRecvDataSegmentLength = 262144`, `MaxTransferLength = 262144`, `FirstBurstLength = 262144`, `MaxBurstLength = 2097152`, `MaxOutstandingR2T = 16` ke registry Microsoft iSCSI Initiator instance `0000` dan `0001`.

- [ ] **Step 1: Implementasikan fungsi `WriteRegDword` di `helper.cpp`**
- [ ] **Step 2: Tambahkan logika auto-generate single gateway dan penimpaan `DefaultGateway` di `helper.cpp`**
- [ ] **Step 3: Tambahkan penulisan tuning DWORD ke path `{4D36E97B-E325-11CE-BFC1-08002BE10318}`**

---

### Task 2: Optimasi iPXE Script `pxe/sb-custom/autoexec.ipxe`

**Files:**
- Modify: `pxe/sb-custom/autoexec.ipxe`

- [ ] **Step 1: Pastikan gateway disetel bersih sebelum perintah `sanboot` dijalankan**

---

### Task 3: Kompilasi Binary & Verifikasi

**Files:**
- Target: `helper/helper.exe`
- Runner: `helper/compile.bat`

- [ ] **Step 1: Jalankan `compile.bat` untuk menghasilkan binary `helper.exe` terbaru**
- [ ] **Step 2: Verifikasi binary berukuran ~11 KB dan terkompilasi bersih**
- [ ] **Step 3: Update `walkthrough.md`**
