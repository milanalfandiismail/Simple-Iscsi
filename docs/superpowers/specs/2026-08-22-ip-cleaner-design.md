# Design Spec: IP Cleaner & DHCP IP Auto-Detection (Native Subsystem)

**Date:** 2026-08-22  
**Status:** Approved for Implementation  
**Component:** `helper/helper.cpp`, `helper/compile.bat`, `helper/test_ip_cleaner.cpp`  

---

## 1. Overview & Objective

Fitur **IP Cleaner (DHCP IP Auto Detection)** bertujuan untuk membersihkan konfigurasi IP lama (stale / ghost) pada Windows client diskless/iSCSI dan memastikan konfigurasi jaringan yang digunakan oleh Windows selalu selaras dengan konfigurasi DHCP terkini.

Program ini diimplementasikan sebagai **Native Subsystem Application** (`helper.exe`) yang berjalan di fase `BootExecute` (Session Manager), sebelum subsistem Win32 dan driver TCP/IP mengunci antarmuka registry.

---

## 2. Architecture & Data Flow

```
+-------------------------------------------------------------------+
|                        Client Boot Sequence                       |
+-------------------------------------------------------------------+
                                  |
                                  v
            [PXE / UEFI Firmware / iPXE Handshake]
                                  |
                                  v
            [DHCP Server (Simple-Iscsi: src/netboot/dhcp.rs)]
            - Assigns yiaddr (Client IP)
            - Options: Subnet (1), Router (3), DNS (6), Hostname (12)
                                  |
                                  v
            [Windows Bootmgr / winload.efi / msiscsi.sys]
            - Mounts iSCSI Root Drive C:\
                                  |
                                  v
            [smss.exe -> BootExecute: helper.exe (Native Subsystem)]
            =======================================================
            1. Enumerate Tcpip\Parameters\Interfaces\{GUID}
            2. Detect & clean stale static IPs (IPAddress, SubnetMask, DefaultGateway)
            3. Force EnableDHCP = 1 on interfaces
            4. Inspect active DhcpIPAddress (validate != 0.0.0.0 and != 169.254.x.x)
            5. Auto-generate Hostname: "PC-[IP_LastOctet]TM"
            6. Synchronize ComputerName, ActiveComputerName, Tcpip\Parameters
            7. NtTerminateProcess(0)
            =======================================================
                                  |
                                  v
            [Windows Win32 Subsystem (csrss.exe) & services.exe]
            - Loads clean TCPIP stack with active DHCP lease
            - Starts all services with updated Hostname ("PC-03TM")
```

---

## 3. Detailed Component Specifications

### 3.1. Registry Keys Handled

#### A. Network Interfaces (IP Cleaner Target)
*   **Root Key:** `\Registry\Machine\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces`
*   **Subkeys:** `{GUID}` untuk setiap adapter (fisik maupun phantom).
*   **Values yang Dimodifikasi/Dibersihkan:**
    *   `EnableDHCP` (REG_DWORD): Diset ke `1` (memaksa DHCP aktif, mengabaikan static IP lama).
    *   `IPAddress` (REG_MULTI_SZ / REG_SZ): Jika berisi IP statis lama non-nol, dibersihkan/direset ke `0.0.0.0`.
    *   `SubnetMask` (REG_MULTI_SZ / REG_SZ): Direset ke `0.0.0.0` jika `EnableDHCP` diaktifkan.
    *   `DefaultGateway` (REG_MULTI_SZ): Direset jika stale.
    *   `DhcpIPAddress` (REG_SZ): Dibaca untuk mendapatkan IP DHCP aktual.

#### B. Hostname Keys (Sinkronisasi Nama PC)
*   `\Registry\Machine\SYSTEM\CurrentControlSet\Control\ComputerName\ComputerName` -> `ComputerName` (REG_SZ)
*   `\Registry\Machine\SYSTEM\CurrentControlSet\Control\ComputerName\ActiveComputerName` -> `ComputerName` (REG_SZ)
*   `\Registry\Machine\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters` -> `Hostname` (REG_SZ) & `NV Hostname` (REG_SZ)

---

## 4. Edge Cases & Resilience Strategy

| Edge Case | Problem / Risk | Handling Strategy |
| :--- | :--- | :--- |
| **DHCP Not Ready / 0.0.0.0** | IP DHCP belum tersimpan di registry saat awal boot | Helper tidak melakukan crash/error; tetap mengeset `EnableDHCP = 1` agar saat Windows memuat DHCP client, IP dari DHCP server langsung terikat tanpa konflik statis. Hostname dipertahankan. |
| **APIPA (169.254.x.x)** | Klien mendapatkan auto-IP lokal karena DHCP sempat delay | Helper mengidentifikasi prefix `169.254.` sebagai invalid/stale IP dan tidak menggunakannya sebagai dasar penamaan hostname. |
| **Multiple NICs / Ghost Adapters** | Ada adapter lama dari image master PC lain | Helper melakukan enumerasi dan membersihkan stale static IP di *semua* subkey interface, lalu memilih interface yang memiliki `DhcpIPAddress` valid pada subnet lokal (misal `192.168.x.x` atau `10.x.x.x`). |
| **DHCP IP Changed on Reboot** | Klien dipindahkan ke IP lain (misal dari `.3` ke `.15`) | Pada setiap boot, helper membaca `DhcpIPAddress` baru dan memperbarui Hostname menjadi `PC-15TM`. |
| **Already Clean / Same IP** | IP dan Hostname sudah sesuai | Helper mendeteksi string sudah cocok dan tidak melakukan penulisan ulang yang tidak perlu. |

---

## 5. Verification & Testing Plan

1.  **Unit / Simulation Test (`test_ip_cleaner.cpp`):**
    *   Test IP parser (validasi format IPv4, ekstraksi oktet terakhir, zero-padding).
    *   Test filter APIPA (169.254.x.x) dan 0.0.0.0.
    *   Test generator Hostname (format `PC-[Octet]TM`).
    *   Test deteksi stale IP vs clean IP.
2.  **Compilation Verification:**
    *   Jalankan `compile.bat` untuk memverifikasi bahwa `helper.exe` terkompilasi bersih tanpa link error (output size ~4-5 KB).
