# Native Helper File Logger & Deep IP/Gateway Cleaner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Mengimplementasikan fitur File Logging (`\SystemRoot\helper.log`) menggunakan Native NT File API dan Deep IP/Gateway Cleaner pada `helper.exe` untuk menghapus seluruh IP sangkut (secondary IPs) dan gateway sangkut di dialog "Advanced TCP/IP Settings".

**Architecture:** Helper berjalan di `BootExecute`, membuka `\SystemRoot\helper.log` via `NtCreateFile`, membaca parameter `iSharePnp`, menimpa nilai `IPAddress`, `SubnetMask`, `DefaultGateway`, `DefaultGatewayMetric`, dan membersihkan residual DHCP lease di seluruh interfaces, mencatat setiap langkah ke file log, menyinkronkan Hostname, lalu keluar bersih.

**Tech Stack:** C++, MSVC Native Subsystem (`/subsystem:native`), NT Native API (`ntdll.dll`).

**Spec:** `docs/superpowers/specs/2026-08-22-helper-logger-and-deep-ip-cleaner-design.md`

## Global Constraints
- Target Subsistem: `/subsystem:native`
- Entry Point: `NtProcessStartup`
- Tanpa runtime library C/C++ standar (`/NODEFAULTLIB`)
- Bebas memory leak & bebas crash/BSOD

---

### Task 1: Native File Logger Subsystem

**Files:**
- Modify: `helper/helper.cpp`
- Modify: `helper/test_ip_cleaner.cpp`

**Interfaces:**
- Produces:
  - `typedef struct _IO_STATUS_BLOCK`: Struktur status I/O NT.
  - `NtCreateFile`, `NtWriteFile`: Deklarasi Native API untuk file I/O.
  - `void LogOpen()`: Membuka file `\SystemRoot\helper.log`.
  - `void LogWrite(const char* text)`: Menulis teks ASCII ke file log.
  - `void LogWriteW(const wchar_t* wtext)`: Menulis teks Unicode (dikonversi ke ASCII) ke file log.
  - `void LogClose()`: Menutup handle file log.

- [ ] **Step 1: Tambahkan deklarasi `NtCreateFile`, `NtWriteFile`, dan `IO_STATUS_BLOCK` di `helper.cpp`**
- [ ] **Step 2: Implementasikan fungsi `LogOpen`, `LogWrite`, `LogWriteW`, dan `LogClose`**
- [ ] **Step 3: Update `test_ip_cleaner.cpp` untuk menguji string formatter logger dan pastikan lulus uji**

---

### Task 2: Implementasi Deep IP/Gateway Cleaner

**Files:**
- Modify: `helper/helper.cpp`

**Interfaces:**
- Consumes: Logger subsystem dari Task 1.
- Produces: Deep cleaning logic untuk membersihkan seluruh residual secondary IP, mask, gateway, metric, dan DHCP leftover keys.

- [ ] **Step 1: Tambahkan pembersihan `DefaultGatewayMetric` (REG_MULTI_SZ = "0\0\0")**
- [ ] **Step 2: Tambahkan pembersihan/reset `DhcpIPAddress`, `DhcpSubnetMask`, `DhcpDefaultGateway`, `DhcpServer`, `DhcpNameServer`**
- [ ] **Step 3: Integrasikan pencatatan log pada setiap interface `{GUID}` yang diproses**

---

### Task 3: Kompilasi Binary & Verifikasi Akhir

**Files:**
- Target: `helper/helper.exe`
- Runner: `helper/compile.bat`

- [ ] **Step 1: Jalankan `compile.bat` untuk membuild `helper.exe`**
- [ ] **Step 2: Verifikasi binary berukuran ~12-15 KB dan bebas error kompilasi**
- [ ] **Step 3: Update `walkthrough.md` dengan detail rencana dan cara pengujian file log**
