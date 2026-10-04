# C++ Native Boot Helper (`helper.exe` via `BootExecute`): Hostname Sync & Absolute IP Overwrite

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menghilangkan masalah timbulnya dua IP (misal IP master image Super User `10.10.10.21` dan IP baru dari DHCP `10.10.10.25`) pada klien diskless dengan menimpa (*overwrite*) secara mutlak konfigurasi network interface di registry Windows, membersihkan residual lease dan DHCP options lama, serta menyinkronkan Hostname komputer persis sesuai konfigurasi DHCP/iBFT di fase `BootExecute`.

**Architecture:** Helper dirancang sebagai aplikasi Native NT Subsystem murni (`ntdll.dll` tanpa ketergantungan Win32 user-mode maupun CRT) yang dieksekusi oleh `smss.exe` saat boot phase awal melalui registri `BootExecute`. Helper membaca parameter boot (IP, Mask, Gateway, DNS, Hostname) dari tabel ACPI iBFT (atau fallback parameter), kemudian meng-override seluruh subkey pada `HKLM\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces` dan registri `ComputerName`/`Tcpip\Parameters`.

**Tech Stack:** C++ (Native NT API, MSVC `cl.exe` dengan `/subsystem:native /NODEFAULTLIB ntdll.lib`), Windows Registry (`NtOpenKey`, `NtSetValueKey`, `NtDeleteValueKey`, `NtEnumerateKey`), ACPI iBFT Firmware Table parser.

**Spec:** Dokumen living memory [`ANTIGRAVITY.md`](file:///c:/Project%20GIT/Simple-Iscsi/ANTIGRAVITY.md) (Bab 4 & Bab 8).

---

## Global Constraints

- **Subsystem:** Native NT Subsystem (`/subsystem:native /entry:NtProcessStartup`).
- **No CRT Dependency:** Dilarang menggunakan fungsi C Runtime standar (`printf`, `malloc`, `wcscpy`, dll). Gunakan implementasi intrinsik (`memset`, `memcpy`) dan string helpers kustom tanpa runtime library.
- **BootExecute Key:** `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager` -> `BootExecute` = `autocheck autochk *\0helper.exe\0\0` (`REG_MULTI_SZ`).
- **Single-Entry MULTI_SZ:** Nilai `IPAddress`, `SubnetMask`, `DefaultGateway` **WAJIB** berupa buffer `REG_MULTI_SZ` dengan tepat satu string diakhiri double-null (`<string>\0\0`) untuk mencegah Windows mengikat multi-IP.
- **Hostname Clean:** Hostname yang ditulis harus persis sesuai apa yang didapat dari DHCP Option 12 / iBFT (menghilangkan suffix paksa `TM` dari implementasi lama).
- **Target Registry Keys:**
  1. `SYSTEM\CurrentControlSet\Control\ComputerName\ComputerName\ComputerName` (`REG_SZ`)
  2. `SYSTEM\CurrentControlSet\Control\ComputerName\ActiveComputerName\ComputerName` (`REG_SZ`)
  3. `SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Hostname` (`REG_SZ`)
  4. `SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\NV Hostname` (`REG_SZ`)
  5. `SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{GUID}`:
     - `EnableDHCP` = `0` (`REG_DWORD`)
     - `IPAddress` = `"<IP>\0\0"` (`REG_MULTI_SZ`)
     - `SubnetMask` = `"<MASK>\0\0"` (`REG_MULTI_SZ`)
     - `DefaultGateway` = `"<GATEWAY>\0\0"` (`REG_MULTI_SZ`)
     - `DefaultGatewayMetric` = `"0\0\0"` (`REG_MULTI_SZ`)
     - `NameServer` = `"<DNS1,DNS2>"` (`REG_SZ`)
     - Hapus/Kosongkan residual DHCP: `DhcpIPAddress`, `DhcpSubnetMask`, `DhcpServer`, `DhcpDefaultGateway`, `DhcpInterfaceOptions`.

---

## Tasks

### Task 1: Update Test Suite di `helper/test_ip_cleaner.cpp`

**Files:**
- Modify: `helper/test_ip_cleaner.cpp:68-150`

**Interfaces:**
- Consumes: `BuildMultiSz(const wchar_t* src, wchar_t* outBuf, unsigned long maxChars)`
- Produces: Unit test assertions untuk validasi format single IP `REG_MULTI_SZ` (menolak data gabungan/dobel), sanitasi hostname murni tanpa suffix, dan format DNS comma-separated.

- [ ] **Step 1: Tulis test case baru untuk validasi single-entry `REG_MULTI_SZ` dan hostname murni**
  Tambahkan pengujian:
  - Verifikasi bahwa hostname dari DHCP (misal `PC-01` atau `CLIENT-25`) dipertahankan 100% tanpa penambahan suffix `TM`.
  - Verifikasi bahwa `BuildMultiSz` menghasilkan buffer dengan panjang tepat `(wcslen + 2) * sizeof(wchar_t)` dan terminasi `\0\0`.
  - Verifikasi deteksi apakah buffer lama memiliki multiple IP dan fungsi pembersih menggantinya dengan nilai tunggal.

- [ ] **Step 2: Jalankan tes unit saat ini untuk memastikan baseline**
  Jalankan `helper/compile_test.bat` di shell PowerShell/CMD.
  Pastikan binary `helper/test_ip_cleaner.exe` dapat dikompilasi dan dijalankan.

- [ ] **Step 3: Update implementasi di test runner**
  Sesuaikan signature dan fungsi di `helper/test_ip_cleaner.cpp` agar mencerminkan logika hostname murni.

- [ ] **Step 4: Jalankan tes dan pastikan seluruh test PASS (Exit Code 0)**
  Jalankan `helper/test_ip_cleaner.exe` dan amati output terminal: `Test Results: X Passed, 0 Failed`.

---

### Task 2: Refactoring Logika Pembersihan & Override Registry di `helper/helper.cpp`

**Files:**
- Modify: `helper/helper.cpp:132-212` (Deklarasi Native API `NtDeleteValueKey`)
- Modify: `helper/helper.cpp:478-498` (Pembaruan fungsi hostname)
- Modify: `helper/helper.cpp:968-1160` (Pembersihan tuntas `Interfaces` & Override Hostname)

**Interfaces:**
- Consumes:
  - `NtDeleteValueKey(void* KeyHandle, PUNICODE_STRING ValueName) -> NTSTATUS`
  - `NtSetValueKey(void* KeyHandle, PUNICODE_STRING ValueName, unsigned long TitleIndex, unsigned long Type, const void* Data, unsigned long DataSize) -> NTSTATUS`
  - `ReadParametersFromIBFT(wchar_t* outHost, wchar_t* outIp, wchar_t* outMask, wchar_t* outGw, wchar_t* outDns1, wchar_t* outDns2) -> bool`
- Produces:
  - `FormatCleanHostname(const wchar_t* inHost, wchar_t* outHost, unsigned long maxChars)`: Format hostname bersih tanpa suffix.
  - `OverrideNetworkInterface(void* hKey, const wchar_t* ip, const wchar_t* mask, const wchar_t* gw, const wchar_t* dns)`: Timpa mutlak konfigurasi adapter dengan IP statis tunggal dan hapus seluruh residual DHCP/lease options.
  - `PurgeStaleInterface(void* hKey)`: Menghapus IP statis lama pada adapter non-aktif agar tidak menyimpan IP super user (misal `10.10.10.21`).

- [ ] **Step 1: Tambahkan deklarasi `NtDeleteValueKey` pada blok `extern "C"`**
  ```cpp
  NTSYSAPI NTSTATUS NTAPI NtDeleteValueKey(
      void* KeyHandle,
      PUNICODE_STRING ValueName
  );
  ```
  Dan pastikan konstanta hak akses `DELETE` (`0x00010000L`) atau `KEY_ALL_ACCESS` disertakan saat `NtOpenKey`.

- [ ] **Step 2: Ganti `FormatHostnameWithTm` dengan `FormatCleanHostname`**
  Hapus penambahan suffix paksa `TM`. Gunakan nama asli dari DHCP / iBFT. Jika kosong, fallback ke `PC-CLIENT`.

- [ ] **Step 3: Implementasikan fungsi penghapusan value registri residual**
  Buat helper function `DeleteRegValue(void* hKey, const wchar_t* valNameStr)` yang memanggil `NtDeleteValueKey`.
  Hapus entri:
  - `DhcpInterfaceOptions`
  - `DhcpIPAddress`
  - `DhcpSubnetMask`
  - `DhcpServer`
  - `DhcpDefaultGateway`
  - `DhcpNameServer`

- [ ] **Step 4: Implementasikan penimpaan mutlak (*Absolute Overwrite*) pada seluruh interface subkeys**
  Dalam loop enumerasi `HKLM\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces`:
  - Pada target interface aktif: Tulis `EnableDHCP = 0`, `IPAddress = multiSzIp` (hanya 1 IP dari DHCP/iBFT), `SubnetMask = multiSzMask`, `DefaultGateway = multiSzGateway`, `NameServer = combinedDns`.
  - Pada interface subkey lainnya yang bukan target: Jika ditemukan `IPAddress` lama (misal berisi `10.10.10.21`), timpa dengan `0.0.0.0\0\0` dan set `EnableDHCP = 1` agar IP master image tidak tertinggal sebagai secondary IP.

- [ ] **Step 5: Verifikasi sinkronisasi 4 path registri Hostname**
  Tulis `rawHostName` ke:
  1. `SYSTEM\CurrentControlSet\Control\ComputerName\ComputerName` (`ComputerName`)
  2. `SYSTEM\CurrentControlSet\Control\ComputerName\ActiveComputerName` (`ComputerName`)
  3. `SYSTEM\CurrentControlSet\Services\Tcpip\Parameters` (`Hostname`)
  4. `SYSTEM\CurrentControlSet\Services\Tcpip\Parameters` (`NV Hostname`)

---

### Task 3: Kompilasi Binary Native `helper.exe` & Validasi PE Subsystem

**Files:**
- Modify/Run: `helper/compile.bat`
- Output: `helper/helper.exe` (Native Subsystem executable)

**Interfaces:**
- Input: `helper/helper.cpp`
- Tool: MSVC `cl.exe`
- Compiler Flags: `/O2 /GS- /GR- /EHs-c- /subsystem:native /entry:NtProcessStartup /NODEFAULTLIB ntdll.lib`

- [ ] **Step 1: Jalankan kompilasi menggunakan script `helper/compile.bat`**
  Eksekusi `helper/compile.bat` melalui terminal.
  Pastikan compiler MSVC x64 menemukan library `ntdll.lib` dan menghasilkan `helper.exe`.

- [ ] **Step 2: Validasi header PE Subsystem pada `helper.exe`**
  Periksa binary `helper.exe`:
  - Subsystem harus bernilai `IMAGE_SUBSYSTEM_NATIVE` (Subsystem 1).
  - Entry point adalah `NtProcessStartup`.
  - Tidak ada dependensi CRT (`msvcrt.dll`, `ucrtbase.dll`, dll) — hanya link ke `ntdll.dll`.

---

### Task 4: Pembaruan Skrip Instalasi Client & Template Registry Standalone

**Files:**
- Modify: `helper/install_client.bat`
- Create: `helper/BOOTEXECUTE_HELPER.reg`

**Interfaces:**
- Input: Windows Registry Editor format 5.00
- Produces: Standalone installer dan registry file untuk mengaktifkan `helper.exe` di fase `BootExecute`.

- [ ] **Step 1: Perbarui `helper/install_client.bat`**
  Pastikan skrip menyalin `helper.exe` ke `C:\Windows\System32\helper.exe` dan mendaftarkan:
  ```cmd
  reg add "HKLM\SYSTEM\CurrentControlSet\Control\Session Manager" /v BootExecute /t REG_MULTI_SZ /d "autocheck autochk *\0helper.exe" /f
  ```

- [ ] **Step 2: Buat file standalone registry `helper/BOOTEXECUTE_HELPER.reg`**
  ```reg
  Windows Registry Editor Version 5.00

  [HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Session Manager]
  "BootExecute"=hex(7):61,00,75,00,74,00,6f,00,63,00,68,00,65,00,63,00,6b,00,20,\
    00,61,00,75,00,74,00,6f,00,63,00,68,00,6b,00,20,00,2a,00,00,00,68,00,65,00,\
    6c,00,70,00,65,00,72,00,2e,00,65,00,78,00,65,00,00,00,00,00
  ```

---

### Task 5: Dokumentasi di `ANTIGRAVITY.md` & Re-indexing Codebase Memory

**Files:**
- Modify: `ANTIGRAVITY.md` (Update Section 4 Arsitektur & Section 9 Change Log)

- [ ] **Step 1: Dokumentasikan arsitektur Override IP & Hostname di `ANTIGRAVITY.md`**
  Catat mekanisme pembersihan mutlak IP lama (`10.10.10.21` super user image) dan registrasi `BootExecute`.

- [ ] **Step 2: Jalankan MCP `codebase-memory:index_repository`**
  Perbarui graf simbol repositori untuk memasukkan perubahan fungsi pada `helper.cpp` dan file baru.

- [ ] **Step 3: Review kesesuaian implementasi dengan kebutuhan user**
  Pastikan seluruh checklist terpenuhi dengan bukti kompilasi sukses.
