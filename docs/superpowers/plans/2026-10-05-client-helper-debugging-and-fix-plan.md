# Client Boot Helper Debugging & Stabilization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menghilangkan semua potensi crash/BSOD `0x78` (PHASE1_INITIALIZATION_FAILED) pada `helper.exe` di `BootExecute`, memperbaiki parsing memori iBFT & registri yang aman, serta menyederhanakan sinkronisasi IP/Hostname tanpa merusak konfigurasi DHCP default image master.

**Architecture:**
- **Stage 1 (BootExecute `helper.exe`):** Fokus murni pada pembacaan aman iBFT tanpa crash/overflow memori, sinkronisasi Hostname ke registri, penulisan marker `SimpleIscsiBoot`, dan tuning fast-boot `iScsiPrt` (`WaitForNetworkAtBoot = 1`, `DelayForNetworkAtBoot = 5`). Menghilangkan pemaksaan `EnableDHCP = 0` agar master image tetap bersih untuk multi-client boot.
- **Stage 2 (User-Mode `helper-svc.exe`):** Menjalankan pembersihan IP unicast duplikat menggunakan Windows IP Helper API resmi (`DeleteUnicastIpAddressEntry`) saat user-mode aktif.
- **Installer (`install_client.bat` & `uninstall_client.bat`):** Registrasi rapi, validasi delimiter `\0` pada `REG_MULTI_SZ`, dan error handling yang aman.

**Tech Stack:** C++ Native NT API (`ntdll.dll`, `/subsystem:native`), C++ Win32 (`iphlpapi.dll`, `ws2_32.dll`, `advapi32.dll`), MSVC `cl.exe`.

---

### Task 1: Perbaikan Memory Safety & String Handling di `helper.cpp`

**Files:**
- Modify: `helper/helper.cpp`

**Interfaces:**
- `bool StrEqualN(const wchar_t* a, const wchar_t* b, unsigned long nChars)`: Perbandingan string dengan batas panjang pasti (mencegah overflow membaca `PKEY_BASIC_INFORMATION`).
- `ReadParametersFromIBFT(...)`: Penjaga batas buffer ketat `tableLen = min(pFirmware->TableBufferLength, sizeof(queryBuffer) - ...)`.
- Hapus `NtFlushKey` berulang di Phase 1 yang memicu `STATUS_REGISTRY_IO_FAILED`.

- [ ] **Step 1: Implementasikan helper perbandingan string berbatas panjang (`StrEqualN` / `StrStartsWithN`)**
- [ ] **Step 2: Tambahkan bound-check ketat pada pembacaan iBFT dan registri ACPI**
- [ ] **Step 3: Hapus pemaksaan `EnableDHCP = 0` dan hapus pemanggilan `NtFlushKey` di Phase 1**
- [ ] **Step 4: Pastikan `helper.cpp` hanya menulis marker `SimpleIscsiBoot`, Hostname, dan parameter tuning `iScsiPrt`**

---

### Task 2: Verifikasi & Kompilasi Binary Helper

**Files:**
- Modify: `helper/compile.bat`
- Build Output: `helper/helper.exe` & `helper/helper-svc.exe`

- [ ] **Step 1: Jalankan `compile.bat` untuk mengompilasi `helper.exe` (Native Subsystem) dan `helper-svc.exe` (Win32 Console/Service)**
- [ ] **Step 2: Verifikasi ukuran file dan ketiadaan error kompilasi / unresolved externals**

---

### Task 3: Penyempurnaan Skrip Installer & Uninstaller

**Files:**
- Modify: `helper/install_client.bat`
- Modify: `helper/uninstall_client.bat`

- [ ] **Step 1: Perbaiki format penulisan `BootExecute` agar sintaks `REG_MULTI_SZ` 100% kompatibel di semua versi Windows**
- [ ] **Step 2: Uji coba instalasi dan uninstalasi lokal secara aman**

---

### Task 4: Validasi & Dokumentasi Living Memory (`ANTIGRAVITY.md`)

**Files:**
- Modify: `ANTIGRAVITY.md`

- [ ] **Step 1: Update bagian 8 & 9 di ANTIGRAVITY.md dengan hasil analisis akar masalah helper dan solusi safe boot helper**
- [ ] **Step 2: Commit dan push perubahan ke repositori Git**
