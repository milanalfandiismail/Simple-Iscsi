# Design Spec: iSharePnp Parameters Parser & BootExecute Static IP / Hostname Injector

**Date:** 2026-08-22  
**Status:** Approved for Implementation  
**Component:** `helper/helper.cpp`, `helper/compile.bat`, `helper/test_ip_cleaner.cpp`  

---

## 1. Overview & Objective

Sistem diskless akan menggunakan parameter dari service **`iSharePnp`** di registry sebagai *Source of Truth* untuk konfigurasi jaringan dan identitas client.

Program **`helper.exe`** (berjalan di fase **`BootExecute`** / Native Subsystem) akan membaca parameter dari:
`HKLM\SYSTEM\CurrentControlSet\Services\iSharePnp\Parameters`

Dan langsung mengonfigurasikan **IP Statis Murni (Disable DHCP)** serta menyinkronkan **Hostname** ke dalam registry Windows sebelum subsistem Win32 dan driver TCP/IP aktif.

---

## 2. Parameter Source of Truth (`iSharePnp\Parameters`)

Helper akan membaca nilai-nilai registry berikut dari `\Registry\Machine\System\CurrentControlSet\Services\iSharePnp\Parameters`:

| Registry Value | Tipe | Deskripsi | Contoh Nilai |
| :--- | :--- | :--- | :--- |
| `HostName` | `REG_SZ` | Nama komputer target klien | `PC-03` / `PC-03TM` |
| `BindIP` | `REG_SZ` | IP Address statis klien | `192.168.180.3` |
| `Mask` | `REG_SZ` | Subnet Mask klien | `255.255.255.0` |
| `GatewayIP` | `REG_SZ` | Default Gateway klien | `192.168.180.1` |
| `Dns1` | `REG_SZ` | Primary DNS server | `8.8.8.8` |
| `Dns2` | `REG_SZ` | Secondary DNS server | `8.8.4.4` |
| `DHCP` | `REG_SZ` | Fallback IP jika `BindIP` kosong | `192.168.180.3` |

---

## 3. Target Registry Injection

### 3.1. Network Adapter Interfaces (Static IP Configuration)
Di setiap subkey `{GUID}` pada `\Registry\Machine\System\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{GUID}`:
1.  **Matikan DHCP:** `EnableDHCP = 0` (`REG_DWORD`).
2.  **Pasang IP Statis:** `IPAddress = [BindIP]\0\0` (`REG_MULTI_SZ`).
3.  **Pasang Subnet Mask:** `SubnetMask = [Mask]\0\0` (`REG_MULTI_SZ`).
4.  **Pasang Gateway:** `DefaultGateway = [GatewayIP]\0\0` (`REG_MULTI_SZ`).
5.  **Pasang DNS:** `NameServer = [Dns1],[Dns2]\0` (`REG_SZ`).

### 3.2. Hostname Synchronization
1.  `Control\ComputerName\ComputerName` -> `ComputerName = [HostName]` (REG_SZ)
2.  `Control\ComputerName\ActiveComputerName` -> `ComputerName = [HostName]` (REG_SZ)
3.  `Services\Tcpip\Parameters` -> `Hostname = [HostName]` (REG_SZ)
4.  `Services\Tcpip\Parameters` -> `NV Hostname = [HostName]` (REG_SZ)

*(Catatan: Jika HostName belum memiliki akhiran "TM" dan pengguna menginginkannya, helper dapat otomatis memastikan akhiran "TM" ditambahkan jika diperlukan).*

---

## 4. Keuntungan Arsitektur Ini
1.  **Zero DHCP Delay:** Komputer langsung menyala dengan IP statis aktif sejak milidetik pertama booting.
2.  **Bebas Konflik IP:** Tidak ada race condition antara DHCP client dan IP lama.
3.  **Ghost Adapters Tereliminasi:** Semua interface lama/phantom di-reset dan disamakan dengan konfigurasi statis yang valid.
4.  **Kompatibilitas 100%:** Menggunakan driver native Windows `msiscsi` dan native helper tanpa driver kernel pihak ketiga berbayar.
