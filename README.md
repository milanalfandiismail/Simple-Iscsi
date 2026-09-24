# Simple iSCSI Target Server 🚀

Target Server iSCSI & Infrastruktur Network Booting (PXE/DHCP/iPXE) modern berbasis **Rust** untuk sistem diskless (*diskless gaming client* & Windows SANBOOT OS). 

Mendukung kecepatan transfer hingga **900+ Mbps (~112 MB/s)** pada jaringan kabel LAN Gigabit (1 Gbps) standar melalui emulasi SCSI SPC-4/SBC-3 (Tagged Command Queuing / Queue Depth 32–64), dual-layer RAM/Disk writeback cache 128 MB, dan ACPI iBFT driverless auto-configuration.

---

> 📖 **Dokumentasi Lengkap Protokol & Arsitektur (All-in-One):**  
> Seluruh spesifikasi mendalam mengenai format DHCP Option (17, 168, 169, 170), protokol iSCSI RFC 7143 (BHS 48-byte, OpCodes, Seq Numbers), SCSI Layer (INQUIRY, VPD 0xB0, Multi-LUN), Writeback Engine, dan ACPI iBFT Helper terdokumentasi secara utuh di:  
> 👉 [**`DOCUMENTATION.md`**](file:///c:/Project%20GIT/Simple-Iscsi/DOCUMENTATION.md)

> [!NOTE]
> **Status Driver Client (Third-Party Driver):**  
> Untuk saat ini, inisialisasi awal binding adapter jaringan (NIC PNP) pada image master Windows client masih menggunakan filter driver pihak ketiga (**CCBoot** atau **iSharedisk** seperti `CCBootPnp.sys` / `iSharePnp.sys`). Seluruh storage I/O, SANBOOT, PDU RFC 7143, SCSI SPC-4/SBC-3, dan alokasi writeback cache sepenuhnya diproses secara independen oleh **Target Server Simple-Iscsi**.

---

## ✨ Fitur Utama
* **Saturasi Bandwidth Wire-Speed (900+ Mbps):** Membuka antrean paralel Windows Storport (Queue Depth 32–64) dengan `CmdQue = 1` dan SCSI Block Limits VPD Page 0xB0 (Granularity 4K / Max Transfer 4MB).
* **Dual-Layer Writeback Cache (128 MB Pre-Allocation):** Cache baca/tulis instan di RAM (DashMap lock-free) + sinkronisasi background disk cache berukuran awal 128 MB dengan kemampuan *auto-grow* dinamis.
* **Multi-LUN GameDisk Architecture:** Mendukung banyak harddisk game (Drive D:, E:, F:, dst.) dalam 1 Target IQN tunggal via perintah SCSI `REPORT LUNS` (`0xA0`).
* **Built-in DHCP & TFTP Engine:** Mengirim seluruh opsi SANBOOT (RFC 4173 URI) secara dinamis tanpa perlu server DHCP pihak ketiga.
* **Driverless Boot Helper (`helper.exe`):** Membaca parameter IP, Subnet, Gateway, DNS, dan Hostname langsung dari firmware ACPI iBFT serta membersihkan ghost adapter jaringan secara otomatis.

---

## 🚀 Panduan Penggunaan Cepat (Quick Start)

### 1. Prasyarat Sistem
* **Server:** Windows 10/11 atau Windows Server (Jalankan sebagai *Administrator* untuk akses raw physical disk).
* **Toolchain:** Rust (stable) + Cargo.

### 2. Kompilasi Server
```powershell
# Build binary rilis berkecepatan tinggi
cargo build --release
```
Binary hasil kompilasi akan berada di `target/release/rust-iscsi-server.exe`.

### 3. Konfigurasi (`config.toml`)
Sesuaikan file [`config.toml`](file:///c:/Project%20GIT/Simple-Iscsi/config.toml) sesuai alamat IP server dan path storage Anda:

```toml
[server]
address = "10.10.10.253"
port = 3300
read_cache_gb = 4

[gamedisk_target]
target_iqn = "iqn.2024-01.com.tmdebug:gamedisks"
discovery = true

[[gamedisk]]
physical_disk = '\\.\PhysicalDrive1'
block_size = 512
vendor_id = "RUSTISCS"
product_id = "GameDisk-0"
product_revision = "1.00"

[windows]
target_iqn_prefix = "iqn.2024-01.com.tmdebug:vhd-"
vhd_dir = 'C:\vhd'
block_size = 512
vendor_id = "RUSTISCS"
product_id = "WindowsBoot"
product_revision = "1.00"

[writeback]
writeback_dirs = ['C:\writeback']
max_cache_per_client_gb = 10
max_write_speed_mbps = 100000

[image_manager]
win10 = 'C:\vhd\Windows_10_Master.vhd'

[dhcp]
enabled = true
start_ip = "10.10.10.244"
end_ip = "10.10.10.254"
router = "10.10.10.1"
dns = "8.8.8.8"
next_server = "10.10.10.253"
subnet_mask = "255.255.255.0"
tftp_dir = "pxe"
pxe_default = "sb-custom"
```

### 4. Menjalankan Server
Buka terminal PowerShell sebagai **Administrator**:
```powershell
.\target\release\rust-iscsi-server.exe
```

---

## 🛠️ Panduan Pengembang (Developer Guide)

### Struktur Modul Kode Sumber
```text
Simple-Iscsi/
├── src/
│   ├── main.rs                   # Entry point server & inisialisasi thread
│   ├── config.rs                 # Parser dan struktur data config.toml
│   ├── backend.rs                # Abstraksi storage (VHD & Raw Physical Drive)
│   ├── vhd.rs                    # Parser VHD Dynamic (Type 3) & Differencing (Type 4)
│   ├── scsi_gamedisk.rs          # Emulasi SCSI SPC-4 & SBC-3 (INQUIRY, VPD, READ/WRITE)
│   ├── scsi_imagedisk.rs         # Handler SCSI spesifik Windows Boot Manager
│   ├── writeback_gamedisk.rs     # Engine Dual-Layer Writeback Cache 128 MB
│   ├── netboot/
│   │   ├── dhcp.rs               # Implementasi DHCP Server & injeksi Options 17/170
│   │   ├── dhcp_packet.rs        # Parser & serializer paket BOOTP/DHCP
│   │   └── tftp.rs               # TFTP server berkecepatan tinggi
│   ├── pdu/
│   │   ├── mod.rs                # Struktur data PDU Basic Header Segment
│   │   ├── builder.rs            # PDU response builder (RFC 7143)
│   │   └── parser.rs             # PDU stream decoder
│   └── session/
│       ├── mod.rs                # Session context & state router
│       ├── login.rs              # State machine Login Stage 0 -> 1 -> 3
│       └── scsi_handler.rs       # SCSI command dispatcher & Data-Out slicer
├── helper/
│   └── helper.cpp                # Native Windows Client Boot Helper (ACPI iBFT)
├── pxe/
│   └── sb-custom/autoexec.ipxe   # Script bootloader iPXE (SANHOOK 0x81 & SANBOOT 0x80)
├── DOCUMENTATION.md              # Spesifikasi teknis & protokol lengkap
└── README.md                     # Panduan ringkas pengguna & developer
```

### Menjalankan Pengujian Unit
```powershell
cargo test
```

---

## 🤝 Panduan Kontribusi (Contributing)

1. **Fork** repository ini ke akun GitHub Anda.
2. Buat branch fitur baru:
   ```powershell
   git checkout -b feat/nama-fitur-keren
   ```
3. Patuhi standar pesan commit (*Conventional Commits*):
   * `feat(...)`: Fitur atau kapabilitas baru
   * `fix(...)`: Perbaikan bug atau kompatibilitas protokol
   * `perf(...)`: Optimasi throughput atau latensi
   * `docs(...)`: Pembaruan dokumentasi
4. Pastikan semua unit test lulus:
   ```powershell
   cargo test
   ```
5. Buka **Pull Request (PR)** ke branch `main`.

---

## 📄 Lisensi
Proyek ini didistribusikan di bawah lisensi **MIT License** © 2026 **Milan Alfandi Ismail**.  
Lihat file [**`LICENSE`**](file:///c:/Project%20GIT/Simple-Iscsi/LICENSE) untuk ketentuan lisensi lengkap.
