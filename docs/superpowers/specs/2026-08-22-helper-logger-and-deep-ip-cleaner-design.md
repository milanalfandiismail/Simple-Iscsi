# Design Spec: Native Helper File Logger & Deep IP/Gateway Cleaner

**Date:** 2026-08-22  
**Status:** In Review (Plan Phase)  
**Component:** `helper/helper.cpp`, `helper/test_ip_cleaner.cpp`, `helper/compile.bat`  

---

## 1. Background & Objectives

Berdasarkan investigasi pada konfigurasi jaringan Windows client (seperti yang terlihat pada dialog **"Advanced TCP/IP Settings"**):
1. **Residual / Stale Secondary IP & Gateway:**
   Windows sering kali menyimpan lebih dari 1 IP address dan Gateway di dalam list "IP addresses" (misal: `10.10.10.253`) dan "Default gateways" (misal: `10.10.10.1`) di bawah `HKLM\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{GUID}`.
2. **Kebutuhan File Logging di `BootExecute`:**
   Karena `helper.exe` berjalan di fase awal (`BootExecute` / Session Manager) di mana konsol GUI belum ada, diperlukan mekanisme **File Logging Mandiri** berbasis Native NT API (`NtCreateFile`, `NtWriteFile`) ke file `C:\Windows\helper.log` (atau `\SystemRoot\helper.log`) untuk mencatat seluruh proses pembacaan registry, IP cleaning, penimpaan gateway, dan sinkronisasi hostname.

---

## 2. Arsitektur & Spesifikasi Detail

### 2.1. File Logger Subsystem (Native API)
Karena berjalan tanpa CRT (`/NODEFAULTLIB`), helper akan menggunakan NT System Calls:
*   **Target Log Path:** `\SystemRoot\helper.log` (secara fisik tersimpan di `C:\Windows\helper.log`).
*   **NT API Digunakan:**
    *   `NtCreateFile` (dengan opsi `FILE_SUPERSEDE` atau `FILE_OVERWRITE_IF`, `FILE_SYNCHRONOUS_IO_NONALERT`).
    *   `NtWriteFile` (menulis string log ASCII/UTF-8 secara synchronous).
    *   `NtClose` (menutup handle log sebelum terminate).
*   **Format Log Output:**
    ```text
    ================================================================
    [Simple-Iscsi BootHelper] Started in BootExecute
    ================================================================
    [+] Parameters from iSharePnp:
        - HostName  : PC-02
        - Target IP : 192.168.180.4 (Source: BindIP/DHCP)
        - Mask      : 255.255.255.0
        - Gateway   : 192.168.180.1
        - DNS1/DNS2 : 8.8.8.8
    [+] Cleaning & Configuring Network Interfaces:
        [Interface] {GUID-XXX}
        - Force EnableDHCP = 0
        - Clean & Set IPAddress: 192.168.180.4 (Stale secondary IPs removed)
        - Clean & Set SubnetMask: 255.255.255.0
        - Clean & Set DefaultGateway: 192.168.180.1 (Stale gateways removed)
        - Clean & Set DefaultGatewayMetric: 0
        - Cleared DHCP Stale Leases (DhcpIPAddress, DhcpDefaultGateway, etc.)
        - Set NameServer (DNS): 8.8.8.8
    [+] Synchronizing ComputerName:
        - Hostname Target: PC-02TM
        - Updated Control\ComputerName\ComputerName
        - Updated Control\ComputerName\ActiveComputerName
        - Updated Services\Tcpip\Parameters (Hostname & NV Hostname)
    ================================================================
    [Simple-Iscsi BootHelper] Completed Successfully (Status: 0)
    ================================================================
    ```

---

### 2.2. Deep IP Cleaner (Pembersihan Total IP & Gateway Sangkut)

Untuk setiap subkey `{GUID}` di `Services\Tcpip\Parameters\Interfaces\{GUID}`:

1. **Pembersihan IPAddress (`REG_MULTI_SZ`):**
   * Format `multiSzIp` dibentuk **hanya berisi 1 IP target**, diakhiri double null: `192.168.180.4\0\0`.
   * Menimpa (*overwrite*) seluruh entri multi-IP lama (seperti `10.10.10.253`, `192.168.2.2`).
2. **Pembersihan SubnetMask (`REG_MULTI_SZ`):**
   * Format `multiSzMask` dibentuk **hanya berisi 1 Mask target**: `255.255.255.0\0\0`.
3. **Pembersihan DefaultGateway (`REG_MULTI_SZ`) & Metric:**
   * Format `multiSzGateway` dibentuk **hanya berisi 1 Gateway target**: `192.168.180.1\0\0`.
   * Nilai `DefaultGatewayMetric` (`REG_MULTI_SZ`) di-reset ke `0\0\0` (Automatic).
4. **Pembersihan Residual DHCP Keys:**
   * Hapus/kosongkan `DhcpIPAddress`, `DhcpSubnetMask`, `DhcpDefaultGateway`, `DhcpServer`, `DhcpNameServer` agar tidak ada residual lease yang membingungkan stack TCP/IP.
5. **Konfigurasi DNS:**
   * Tulis `NameServer` (`REG_SZ`) dengan daftar DNS (`8.8.8.8`).

---

## 3. Rencana Pengujian & Verifikasi

1. **Unit Test (`test_ip_cleaner.cpp`):**
   * Test serialization Multi-SZ string single vs multi entry.
   * Test format log buffer dan string converter.
2. **Kompilasi Binary (`helper.exe`):**
   * Build via MSVC x64 `/subsystem:native` `/NODEFAULTLIB` linking `ntdll.lib`.
   * Ukuran binary diperkirakan ~12-15 KB.
3. **Verifikasi Operasional:**
   * Di Windows client: periksa keberadaan file `C:\Windows\helper.log`.
   * Buka dialog "Advanced TCP/IP Settings" di Windows client $\rightarrow$ pastikan daftar IP addresses dan Default gateways **hanya berisi 1 IP dan 1 Gateway yang bersih**.
