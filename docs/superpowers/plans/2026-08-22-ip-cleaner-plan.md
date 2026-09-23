# IP Cleaner & DHCP IP Auto-Detection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Mengimplementasikan fitur IP Cleaner pada `helper.exe` berbasis C++ Native Subsystem untuk membersihkan konfigurasi IP statis lama/stale pada seluruh network interface di registry dan menyinkronkan hostname berdasarkan IP DHCP aktual.

**Architecture:** Helper berjalan di `BootExecute` (Session Manager), mengiterasi semua subkey network adapter di `Services\Tcpip\Parameters\Interfaces`, mereset `EnableDHCP = 1`, membersihkan stale static IP/mask/gateway, mendeteksi `DhcpIPAddress` aktif, dan memperbarui Hostname menjadi `PC-[LastOctet]TM`.

**Tech Stack:** C++, MSVC Native Subsystem (`/subsystem:native`), NT Native API (`ntdll.dll`).

**Spec:** `docs/superpowers/specs/2026-08-22-ip-cleaner-design.md`

## Global Constraints
- Target Subsistem: `/subsystem:native`
- Entry Point: `NtProcessStartup`
- Tanpa runtime library C/C++ standar (`/NODEFAULTLIB`)
- Bebas memory leak & bebas crash/BSOD

---

### Task 1: Test Suite & Core Algoritma IP Cleaner (Unit Testing)

**Files:**
- Create: `helper/test_ip_cleaner.cpp`
- Create: `helper/compile_test.bat`

**Interfaces:**
- Produces:
  - `bool IsValidDhcpIp(const wchar_t* ipStr)`: Memvalidasi IP bukan 0.0.0.0, bukan APIPA (169.254.x.x), dan format IPv4 valid.
  - `bool GenerateHostnameFromIp(const wchar_t* ipStr, wchar_t* outHostname, unsigned long maxLen)`: Menghasilkan string `PC-[Octet]TM`.
  - `unsigned int ParseLastOctet(const wchar_t* ipStr)`: Mengekstrak angka oktet ke-4.

- [ ] **Step 1: Write the unit test and core functions in `test_ip_cleaner.cpp`**
- [ ] **Step 2: Create compilation script `compile_test.bat` for test runner**
- [ ] **Step 3: Run the test suite and verify all test cases pass**

---

### Task 2: Implementasi Lengkap IP Cleaner di `helper/helper.cpp`

**Files:**
- Modify: `helper/helper.cpp`

**Interfaces:**
- Consumes: Fungsi validasi IP dan pembuat hostname dari Task 1.
- Produces: Native executable `helper.exe` dengan fungsi enumerasi registry interface dan IP cleaner otomatis.

- [ ] **Step 1: Tambahkan deklarasi Native API `NtEnumerateKey` dan struktur `KEY_BASIC_INFORMATION`**
- [ ] **Step 2: Implementasikan logika pembersihan stale static IP (`EnableDHCP = 1`, reset `IPAddress`) di seluruh interface `{GUID}`**
- [ ] **Step 3: Implementasikan pembacaan `DhcpIPAddress` aktif dan sinkronisasi 4 key registry ComputerName**

---

### Task 3: Kompilasi & Verifikasi Akhir

**Files:**
- Modify: `helper/compile.bat`
- Target: `helper/helper.exe`

- [ ] **Step 1: Jalankan kompilasi menggunakan `compile.bat`**
- [ ] **Step 2: Verifikasi binary `helper.exe` dihasilkan dengan subsistem Native dan ukuran ~4-5 KB**
- [ ] **Step 3: Jalankan verifikasi menyeluruh dan buat laporan akhir**
