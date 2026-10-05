# Implementation Plan - Perbaikan Boot Helper & Proteksi Registri Client Diskless

Dokumen ini menganalisis akar masalah mengapa instalasi `helper.exe` menyebabkan Windows client diskless tidak bisa booting (freeze / BSOD `0x7B`), serta menyusun rencana perbaikan agar helper aman, tidak merusak registri, dan menjaga integritas 6 Pilar Native Driverless SANBOOT.

---

## 1. Analisis Akar Masalah (Root Cause Analysis)

### Mengapa Client Langsung Tidak Bisa Booting Setelah Install Helper?

Setelah melakukan audit mendalam pada [`helper/helper.cpp`](file:///c:/Project%20GIT/Simple-Iscsi/helper/helper.cpp) dan [`helper/install_client.bat`](file:///c:/Project%20GIT/Simple-Iscsi/helper/install_client.bat), ditemukan 4 penyebab fatal:

1. **Blind Loop & Penghapusan Nilai Registri di `Tcpip\Parameters\Interfaces` (`helper.cpp:1220-1265`):**
   - `helper.exe` berjalan di fase **Phase 1 (`smss.exe` / `BootExecute`)** sebelum user-mode siap.
   - `helper.exe` melakukan loop ke **SEMUA** subkey interface yang ada (termasuk virtual adapter, WAN miniports, loopback).
   - Pada setiap subkey, helper mengeksekusi `OverwriteTcpipRegistryBlock`:
     - Menghapus `DhcpInterfaceOptions`, `DhcpIPAddress`, `DhcpSubnetMask`, `DhcpServer`, `DhcpDefaultGateway`, `DhcpNameServer`.
     - Mengubah paksa `EnableDHCP = 0` pada semua adapter.
     - Menulis nilai `DhcpIPAddress = 0.0.0.0` dan `DhcpServer = 0.0.0.0`.
   - **Efek Fatal:** Pada saat booting Phase 0/Phase 1, driver `tcpip.sys` dan `msiscsi.sys` membutuhkan parameter interface asli agar binding jaringan ke server iSCSI tetap hidup. Menghapus nilai DHCP dan memaksa `EnableDHCP = 0` pada interface yang sedang aktif memutus komunikasi socket iSCSI $\rightarrow$ **BSOD `0x0000007B` (INACCESSIBLE_BOOT_DEVICE)**.

2. **Blind Injection ke `Control\Class\{4D36E97B...}\0000` (`helper.cpp:1327-1341`):**
   - Helper menulis parameter tuning `MaxRecvDataSegmentLength`, `MaxTransferLength`, `MaxBurstLength` secara membabi-buta ke slot `0000` dan `0001` di bawah class `{4D36E97B-E325-11CE-BFC1-08002BE10318}` (*SCSIAdapter*).
   - Di Windows, slot `0000` pada class SCSIAdapter seringkali merupakan storage controller fisik motherboard (seperti SATA AHCI / StorAHCI atau NVMe controller), **bukan** Microsoft iSCSI Initiator driver.
   - Menulis nilai iSCSI ke controller SATA/NVMe merusak konfigurasi controller storage lokal.

3. **Penimpaan ke `ControlSet001` saat `smss.exe` Berjalan:**
   - `helper.exe` menimpa 4 target sekaligus (`CurrentControlSet` dan `ControlSet001`).
   - Pada `BootExecute`, `CurrentControlSet` adalah symlink yang belum tentu aman untuk modifikasi agresif ganda, sehingga memicu inkonsistensi hive registri kernel.

4. **Pelajaran Penting dari 6 Pilar Native Driverless (BAB 9 `ANTIGRAVITY.md`):**
   - Slot `0000` pada network card (`Control\Class\{4D36E972...}\0000`) dan GUID `NetCfgInstanceId` **BERSIFAT SAKRAL & MUTLAK DILARANG DIRUSAK/DIHAPUS**.
   - Menghapus value DHCP atau memodifikasi sembarang interface di `BootExecute` sangat berisiko merusak rantai binding L2/L3 (`Linkage`) yang dibutuhkan `msiscsi.sys`.

---

## 2. Solusi Arsitektur (Safe & Clean Boot Helper)

Untuk menjamin client **100% selalu bisa boot dengan lancar dan stabil**:

1. **Non-Destructive BootExecute Helper (`helper.exe`):**
   - **DILARANG KERAS** menghapus value registri apa pun (`DhcpInterfaceOptions`, `EnableDHCP`, dll.) di fase `BootExecute`.
   - **DILARANG KERAS** melakukan blind loop ke semua interface atau memodifikasi class `SCSIAdapter\0000`.
   - Jika `helper.exe` berjalan di `BootExecute`, tugasnya HANYA:
     1. Membaca iBFT murni dari ACPI Firmware.
     2. Menulis marker `SimpleIscsiBoot` di `HKLM\SYSTEM\CurrentControlSet\Services\SimpleIscsiBoot` (`TargetIp`, `Hostname`).
     3. Menyinkronkan Hostname Windows (`ComputerName`).
     4. **TIDAK MENYENTUH ATAU MENGHAPUS** kunci `Interfaces` di fase boot kernel.

2. **Pembersihan IP Residu Didelegasikan Penuh ke `helper-svc.exe` (User-Mode):**
   - `helper-svc.exe` berjalan aman di user-mode (`services.exe` atau Startup Run Key) setelah Windows selesai boot.
   - `helper-svc.exe` menggunakan Windows API resmi `iphlpapi.dll` (`GetUnicastIpAddressTable` dan `DeleteUnicastIpAddressEntry`).
   - API ini mencabut IP duplikat/residu super client langsung dari RAM `tcpip.sys` secara aman dan terbukti **100% stabil tanpa risiko BSOD**.

3. **Perbaikan `install_client.bat`:**
   - Menjaga integritas `BootExecute`.
   - Memastikan helper tidak merusak registri default Windows.

---

## 3. Rencana Aksi (Implementation Tasks)

- [x] **Task 1: Sterilisasi `helper/helper.cpp` dari Operasi Destruktif Registri**
  - Hapus fungsi `DeleteRegValue` dari subkey `Interfaces`.
  - Hapus pemaksaan `EnableDHCP = 0` dan zeroing `DhcpIPAddress`.
  - Hapus injeksi parameter ke `SCSIAdapter\{4D36E97B...}\0000\Parameters`.
  - Pastikan `helper.exe` hanya menyinkronkan Hostname dan membuat marker `SimpleIscsiBoot`.

- [x] **Task 2: Kompilasi & Validasi Biner C++ Helper**
  - Kompilasi ulang `helper.exe` (Native Subsystem) via `helper/compile.bat`.
  - Kompilasi ulang `helper-svc.exe` (Win32 Service) via `helper/compile_svc.bat`.
  - Jalankan `helper/test_ip_cleaner.exe` untuk memastikan unit test lulus 100%.

- [x] **Task 3: Perbarui Skrip `helper/install_client.bat`**
  - Pastikan registrasi `BootExecute` dan `Service` bersih dan aman tanpa merusak bootloader.

- [x] **Task 4: Dokumentasi Living Memory di [`ANTIGRAVITY.md`](file:///c:/Project%20GIT/Simple-Iscsi/ANTIGRAVITY.md)**
  - Tambahkan Kasus #17 pada Playbook Debugging Section 3 mengenai proteksi integritas registri boot helper agar bebas dari BSOD 0x7B.

