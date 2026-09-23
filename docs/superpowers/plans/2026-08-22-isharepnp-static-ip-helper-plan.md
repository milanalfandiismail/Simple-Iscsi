# iSharePnp Parameters Parser & Static IP Injector Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Mengimplementasikan program pembantu C++ Native Subsystem (`helper.exe`) di `BootExecute` untuk membaca konfigurasi dari `Services\iSharePnp\Parameters` (HostName, BindIP, Mask, GatewayIP, Dns1/Dns2) dan langsung memasangnya sebagai **IP Statis (EnableDHCP = 0)** di seluruh interfaces network adapter serta menyinkronkan Hostname pada saat booting.

**Architecture:** Helper berjalan di `BootExecute` (sebelum Win32 & TCPIP stack aktif), membaca data `iSharePnp\Parameters`, menonaktifkan DHCP (`EnableDHCP = 0`), menulis nilai REG_MULTI_SZ `IPAddress`, `SubnetMask`, `DefaultGateway`, dan REG_SZ `NameServer` di registry interface adapter, serta memperbarui nama komputer.

**Tech Stack:** C++, MSVC Native Subsystem (`/subsystem:native`), NT Native API (`ntdll.dll`).

**Spec:** `docs/superpowers/specs/2026-08-22-isharepnp-static-ip-helper-design.md`

## Global Constraints
- Target Subsistem: `/subsystem:native`
- Entry Point: `NtProcessStartup`
- Tanpa runtime library C/C++ standar (`/NODEFAULTLIB`)
- Bebas memory leak & bebas crash/BSOD

---

### Task 1: Unit Test Suite & String / Multi-SZ Serializer

**Files:**
- Modify: `helper/test_ip_cleaner.cpp`

**Interfaces:**
- Produces:
  - `void BuildMultiSz(const wchar_t* str, wchar_t* outBuf, unsigned long maxChars)`: Membuat format string REG_MULTI_SZ (diakhiri double null `\0\0`).
  - `void CombineDns(const wchar_t* dns1, const wchar_t* dns2, wchar_t* outDns, unsigned long maxChars)`: Menggabungkan dua DNS menjadi string comma-separated (`8.8.8.8,8.8.4.4`).
  - Unit tests untuk memvalidasi pembuatan buffer static IP, Subnet Mask, Gateway, dan Hostname.

- [ ] **Step 1: Tulis unit test untuk `BuildMultiSz`, `CombineDns`, dan parsing parameter iSharePnp**
- [ ] **Step 2: Jalankan `compile_test.bat` dan pastikan seluruh unit test lolos**

---

### Task 2: Implementasi C++ Native Subsystem di `helper/helper.cpp`

**Files:**
- Modify: `helper/helper.cpp`

**Interfaces:**
- Consumes: Fungsi dari Task 1.
- Produces: Binary `helper.exe` yang membaca `iSharePnp\Parameters` dan memasang Static IP + Hostname ke registry Windows.

- [ ] **Step 1: Buka `Services\iSharePnp\Parameters` dan baca `HostName`, `BindIP` (fallback `DHCP`), `Mask`, `GatewayIP`, `Dns1`, `Dns2`**
- [ ] **Step 2: Format data menjadi REG_MULTI_SZ dan pasang ke seluruh subkey `Services\Tcpip\Parameters\Interfaces\{GUID}` dengan `EnableDHCP = 0`**
- [ ] **Step 3: Tulis `HostName` ke 4 lokasi registry ComputerName & Tcpip Parameters**
- [ ] **Step 4: Keluar bersih via `NtTerminateProcess`**

---

### Task 3: Kompilasi & Verifikasi Akhir

**Files:**
- Target: `helper/helper.exe`
- Runner: `helper/compile.bat`

- [ ] **Step 1: Jalankan `compile.bat` untuk membuild `helper.exe`**
- [ ] **Step 2: Verifikasi binary berukuran ~6-7 KB dan bebas dari ketergantungan library luar**
- [ ] **Step 3: Buat laporan akhir di `walkthrough.md`**
