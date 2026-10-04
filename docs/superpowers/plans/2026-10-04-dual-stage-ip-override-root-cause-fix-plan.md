 # Dual-Stage Boot & User-Mode IP Override Root Cause Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menghilangkan residu static IP super client (`192.168.180.10`) secara permanen dan memastikan client diskless hanya memiliki satu-satunya IP yang didapat dari DHCP/iBFT (`192.168.180.2`) menggunakan arsitektur dua tahap (*Dual-Stage Network Alignment*: Native `BootExecute` + User-Mode IP Purge Companion).

**Architecture:** 
1. **Akar Masalah (*Root Cause*):** 
   - Driver `tcpip.sys` merupakan *Boot-Start Driver* (`Start = 0`, `Group = "PNP_TDI"`, `BootFlags = 1`) yang dimuat oleh kernel pada **Phase 0**. Pada saat itu, `tcpip.sys` membaca file SYSTEM hive dari disk VHD yang masih menyimpan IP statis lama master image (`192.168.180.10`).
   - Driver `msiscsi.sys` membaca tabel ACPI iBFT (`192.168.180.2`) dan secara otomatis menyuntikkan IP ini sebagai IP sekunder agar koneksi boot disk iSCSI tidak putus.
   - `helper.exe` berjalan di `smss.exe` (**Phase 1** / `BootExecute`). Walaupun `helper.exe` menimpa registri, `tcpip.sys` yang sudah aktif di RAM kernel tidak me-reload registri secara otomatis tanpa pemanggilan IP Helper API.
   - Akibatnya, saat GUI Windows desktop terbuka, `tcpip.sys` melaporkan `192.168.180.10` sebagai Row 1 dan `192.168.180.2` sebagai Row 2.
2. **Solusi Arsitektur Dua Tahap (*Dual-Stage Architecture*):**
   - **Stage 1 (Native Subsystem - `helper.exe` di `BootExecute`):**
     - Memperbaiki `ReadRegMultiSz` untuk observabilitas total (mencatat IP lama dan verifikasi read-back Multi-SZ).
     - Menimpa registri `Tcpip\Parameters\Interfaces` dan seluruh service adapter `{GUID}`.
     - Menyinkronkan Hostname (`RISMA`) dan tuning performa iSCSI.
     - Menulis penanda konfigurasi bersih di `HKLM\SYSTEM\CurrentControlSet\Services\SimpleIscsiBoot` (`TargetIp = 192.168.180.2`).
     - Menonaktifkan residu service diskless legacy yang berpotensi merestorasi IP (`Start = 4`).
   - **Stage 2 (User-Mode Purge Companion - `helper-svc.exe` saat Windows Startup/Service):**
     - Berjalan saat transisi user-mode (`services.exe` / Windows Service / Startup).
     - Membaca `TargetIp` dari `SimpleIscsiBoot` atau ACPI iBFT via `GetSystemFirmwareTable`.
     - Memanggil `GetUnicastIpAddressTable(AF_INET, ...)`.
     - Menemukan IP yang tidak cocok dengan `TargetIp` (yakni `192.168.180.10`), lalu memanggil `DeleteUnicastIpAddressEntry`.
     - Fungsi ini langsung memerintahkan `tcpip.sys` di kernel untuk membuang `192.168.180.10` dari RAM dan registri secara aman tanpa memutus sesi iSCSI.
     - Hanya IP `192.168.180.2` yang tersisa sebagai satu-satunya IP aktif.

**Tech Stack:** C++ Native NT Subsystem (`ntdll.dll`), Win32 C++ API (`iphlpapi.lib`, `ws2_32.lib`, `advapi32.lib`), Windows Service Architecture, MSVC x64.

**Spec:** [`ANTIGRAVITY.md`](file:///c:/Project%20GIT/Simple-Iscsi/ANTIGRAVITY.md) & [`DOCUMENTATION.md`](file:///c:/Project%20GIT/Simple-Iscsi/DOCUMENTATION.md).

---

## Global Constraints
- Target platform: Windows 10/11 x64 Native Subsystem (`smss.exe` BootExecute) + Win32 Service/Startup Subsystem.
- Zero CRT dependency pada `helper.exe` (murni `ntdll.lib`).
- Native Win32 API pada `helper-svc.exe` (menggunakan `iphlpapi.dll` standar Microsoft).
- Evidence-based verification: Log NTSTATUS dan hasil purge ke `\SystemRoot\helper.log` dan `\SystemRoot\helper-svc.log`.

---

### Task 1: Perbaiki Multi-SZ Reading & Service Disarming pada `helper.cpp`

**Files:**
- Modify: `helper/helper.cpp` (Implementasi `ReadRegMultiSz` untuk parsing semua string dalam buffer `REG_MULTI_SZ`, disarm service diskless legacy, buat marker `SimpleIscsiBoot`)
- Modify: `helper/test_ip_cleaner.cpp` (Tambahkan unit test untuk `ReadRegMultiSz`)

**Interfaces:**
- Consumes:
  - `NtQueryValueKey`, `NtSetValueKey`, `NtDeleteValueKey`, `NtOpenKey`
- Produces:
  - `bool ReadRegMultiSz(void* hKey, const wchar_t* valNameStr, wchar_t* outBuf, unsigned long maxChars)`
  - Marker registry `HKLM\SYSTEM\CurrentControlSet\Services\SimpleIscsiBoot`

- [x] **Step 1: Tulis unit test untuk Multi-SZ parser di `helper/test_ip_cleaner.cpp`**
- [x] **Step 2: Jalankan unit test untuk memastikan fungsionalitas parser sebelum diterapkan**
- [x] **Step 3: Implementasikan `ReadRegMultiSz` dan perbaiki logging di `helper/helper.cpp`**
- [x] **Step 4: Tambahkan pemindaian dan disarm service legacy (`Start = 4`) serta pembuatan marker `SimpleIscsiBoot`**
- [x] **Step 5: Kompilasi `helper.exe` dan pastikan sukses 0 error**

---

### Task 2: Buat User-Mode IP Purge Companion (`helper/helper-svc.cpp`)

**Files:**
- Create: `helper/helper-svc.cpp`
- Create: `helper/compile_svc.bat`

**Interfaces:**
- Consumes:
  - `GetUnicastIpAddressTable`, `DeleteUnicastIpAddressEntry` dari `iphlpapi.lib`
  - `GetSystemFirmwareTable` dari `kernel32.lib`
  - Registry API dari `advapi32.lib`
- Produces:
  - `helper-svc.exe` (Binary Win32 berukuran kecil yang membersihkan IP statis lama secara real-time di user-mode)

- [ ] **Step 1: Buat file `helper/helper-svc.cpp` dengan logika enumerasi unicast IP dan pemanggilan `DeleteUnicastIpAddressEntry`**
- [ ] **Step 2: Tambahkan dukungan dual-mode: dapat berjalan sebagai Windows Service (`services.exe`) maupun direct execution (`/run`)**
- [ ] **Step 3: Buat script kompilasi `helper/compile_svc.bat` menggunakan MSVC x64**
- [ ] **Step 4: Kompilasi `helper-svc.exe` dan verifikasi file biner terbentuk tanpa error**

---

### Task 3: Perbarui Script Installer Client (`helper/install_client.bat`) & `compile.bat`

**Files:**
- Modify: `helper/install_client.bat`
- Modify: `helper/compile.bat`

**Interfaces:**
- Consumes:
  - `helper.exe` dan `helper-svc.exe`
- Produces:
  - Otomatisasi instalasi client lengkap (BootExecute + Windows Service/Run Key)

- [ ] **Step 1: Perbarui `compile.bat` agar sekaligus mengompilasi `helper.exe` dan `helper-svc.exe`**
- [ ] **Step 2: Perbarui `install_client.bat` agar menyalin kedua biner ke `System32`**
- [ ] **Step 3: Daftarkan `helper-svc.exe` sebagai Windows Service otomatis (`SimpleIscsiHelper`) atau Run Key**
- [ ] **Step 4: Uji jalankan `compile.bat nopause` untuk memverifikasi proses build menyeluruh**

---

### Task 4: Dokumentasikan Solusi Dua Tahap ke `ANTIGRAVITY.md`

**Files:**
- Modify: `ANTIGRAVITY.md` (Bagian 3 Playbook Kasus 6, Pilar 7, Status Matriks, dan Change Log)

- [ ] **Step 1: Tambahkan analisis arsitektural Phase 0 vs Phase 1 di Playbook Kasus 6 `ANTIGRAVITY.md`**
- [ ] **Step 2: Dokumentasikan arsitektur Dual-Stage (BootExecute + User-Mode Purge Companion)**
- [ ] **Step 3: Sinkronkan status matriks dan perbarui catatan perubahan (Change Log)**
- [ ] **Step 4: Jalankan MCP `detect_changes` dan `index_repository` untuk memastikan integritas graf memory**
