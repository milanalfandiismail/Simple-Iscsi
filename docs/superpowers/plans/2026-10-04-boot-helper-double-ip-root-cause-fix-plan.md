# Boot Helper Double IP Override Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menghilangkan masalah timbulnya dua IP address (IP master image `192.168.180.10` dan IP DHCP `192.168.180.2`) pada Windows client diskless dengan menimpa kedua lokasi registri TCP/IP (`Services\Tcpip\Parameters\Interfaces\{GUID}` DAN `Services\{GUID}\Parameters\Tcpip`), menambahkan pemanggilan `NtFlushKey`, serta menambahkan instrumentasi verifikasi read-back dan logging status.

**Architecture:** 
Hasil investigasi akar masalah (*Root Cause Investigation*) membuktikan bahwa Windows menyimpan konfigurasi static IP di **DUA lokasi registri**:
1. `HKLM\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{GUID}`
2. `HKLM\SYSTEM\CurrentControlSet\Services\{GUID}\Parameters\Tcpip`

Sebelumnya, `helper.exe` hanya menimpa Lokasi 1 (`Tcpip\Parameters\Interfaces\{GUID}`). Akibatnya, saat driver adapter dimuat oleh NDIS/TCPIP, Windows membaca Lokasi 2 (`Services\{GUID}\Parameters\Tcpip`) yang masih menyimpan static IP master (`192.168.180.10`), lalu menggabungkannya (*merge*) sehingga `192.168.180.10` menjadi IP utama (Row 1) dan `192.168.180.2` menjadi IP sekunder (Row 2). Solusinya adalah menimpa kedua lokasi registri tersebut secara serentak, menghapus value lama sebelum set, memanggil `NtFlushKey`, dan memvalidasi read-back.

**Tech Stack:** C++ Native NT Subsystem (`ntdll.dll`, `cl.exe /subsystem:native /NODEFAULTLIB ntdll.lib`), Win32 C++ unit testing.

**Spec:** [`ANTIGRAVITY.md`](file:///c:/Project%20GIT/Simple-Iscsi/ANTIGRAVITY.md) & [`DOCUMENTATION.md`](file:///c:/Project%20GIT/Simple-Iscsi/DOCUMENTATION.md) (BAB 9 Native Driverless).

---

## Global Constraints
- Target platform: Windows 10/11 x64 Native Subsystem (`smss.exe` BootExecute).
- Zero CRT dependency: No C runtime library, no `malloc`/`printf`, use pure native NT APIs (`NtSetValueKey`, `NtDeleteValueKey`, `NtFlushKey`, `NtOpenKey`, `NtClose`).
- Strict Single-Entry Multi-SZ: Buffer IPAddress, SubnetMask, DefaultGateway diakhiri double-null (`<string>\0\0`).
- Evidence-based verification: Log NTSTATUS hex codes and read-back verification to `\SystemRoot\helper.log`.

---

### Task 1: Update Unit Test Suite for Dual Registry Path & Verification

**Files:**
- Modify: `helper/test_ip_cleaner.cpp`

**Interfaces:**
- Consumes: `BuildMultiSz`, `IsValidIp`, `FormatCleanHostname`
- Produces: Test cases for validating dual registry path formatting (`Tcpip\Parameters\Interfaces\{GUID}` and `Services\{GUID}\Parameters\Tcpip`), clean buffer overwrite, and string counting.

- [x] **Step 1: Tambahkan unit test path generator untuk kedua target registri**
- [x] **Step 2: Jalankan test runner untuk memverifikasi fungsionalitas test** (`helper/compile_test.bat`)
- [x] **Step 3: Pastikan semua assertion (22+ assertions) lulus 100%**

---

### Task 2: Implementasi Dual-Path Registry Overwrite & NtFlushKey pada `helper.cpp`

**Files:**
- Modify: `helper/helper.cpp:132-225` (Deklarasi NtFlushKey & status logging)
- Modify: `helper/helper.cpp:970-1115` (Dual overwrite loop pada Interfaces & Services\{GUID}\Parameters\Tcpip)

**Interfaces:**
- Consumes: 
  - `NTSYSAPI NTSTATUS NTAPI NtFlushKey(void* KeyHandle)`
  - `NTSYSAPI NTSTATUS NTAPI NtDeleteValueKey(void* KeyHandle, PUNICODE_STRING ValueName)`
- Produces:
  - `bool OverwriteInterfaceTcpip(const wchar_t* basePath, const wchar_t* guidStr, const wchar_t* ip, const wchar_t* mask, const wchar_t* gw, const wchar_t* dns)`

- [x] **Step 1: Deklarasikan `NtFlushKey` di header block FFI `ntdll.dll`**
- [x] **Step 2: Buat helper function `OverwriteTcpipKey` yang meng-overwrite `IPAddress`, `SubnetMask`, `DefaultGateway`, `EnableDHCP = 0`, membaca ulang nilainya, dan memanggil `NtFlushKey`**
- [x] **Step 3: Terapkan penulisan ganda ke `Services\Tcpip\Parameters\Interfaces\{GUID}` DAN `Services\{GUID}\Parameters\Tcpip`**
- [x] **Step 4: Tambahkan sinkronisasi juga ke `\Registry\Machine\System\ControlSet001` untuk redundansi mutlak**
- [x] **Step 5: Kompilasi `helper.exe` menggunakan `helper/compile.bat` dan pastikan sukses 0 error**

---

### Task 3: Verifikasi Kompilasi & Logging Evidence

**Files:**
- Modify: `helper/compile.bat` (Hapus/jadikan optional `pause` untuk automation)
- Test: Compile execution and binary generation check (`helper.exe`)

- [x] **Step 1: Jalankan kompilasi `compile.bat` dan verifikasi file binary `helper/helper.exe` ter-update**
- [x] **Step 2: Jalankan unit test `compile_test.bat` dan catat output**
- [x] **Step 3: Verifikasi binary `helper.exe` siap diuji di VM client**

---

### Task 4: Dokumentasikan Root Cause & Solusi ke `ANTIGRAVITY.md`

**Files:**
- Modify: `ANTIGRAVITY.md` (Bagian 3, 7, 8, dan 9)

- [x] **Step 1: Tambahkan Kasus 6 di Playbook Debugging Section 3: Dual Location IP Registry Overwrite**
- [x] **Step 2: Tambahkan catatan teknis pilar registri di Section 7**
- [x] **Step 3: Perbarui Change Log Section 9 dengan ringkasan temuan empiris dan perbaikan**
- [x] **Step 4: Perbarui status matriks di Section 8**
