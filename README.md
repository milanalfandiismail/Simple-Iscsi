# Simple iSCSI Target Server

iSCSI target server in Rust untuk diskless game client. Support read cache writeback + read-ahead 256KB.

## 🚀 Quick Start

1. **Build**
   ```powershell
   cargo build --release
   ```

2. **Konfigurasi** — edit `config.toml`:
   ```toml
   [server]
   address = "0.0.0.0"
   port = 3300
   target_iqn = "iqn.2024-01.com.gameserver:gamedisk"

   [storage]
   physical_disk = "PhysicalDrive1"
   cache_dir = 'E:\gamedisk-cache'
   block_size = 512

   [cache]
   max_cache_per_client_gb = 32
   ```

3. **Jalankan** (butuh Administrator untuk akses physical disk):
   ```powershell
   .\target\release\rust-iscsi-server.exe
   ```

4. **Client** — Connect via Windows iSCSI Initiator ke `server-ip:3300`

## ⚡ Performance Tuning (Windows)

Untuk mencapai throughput maksimal (≥950 Mbps / ~112 MB/s), terapkan tweak berikut **di kedua sisi (client & server)**:

### 1. 🎯 QoS — Nonaktifkan Reservable Bandwidth

Ini tweak paling berdampak — beri lompatan drastis (~200 Mbps tambahan).

**Group Policy Editor** (`gpedit.msc`):
```
Computer Configuration → Administrative Templates → Network → QoS Packet Scheduler
→ Limit reservable bandwidth → Enabled → Set to 0
```

**Atau via Registry:**
```reg
[HKEY_LOCAL_MACHINE\SOFTWARE\Policies\Microsoft\Windows\Psched]
"NonBestEffortLimit"=dword:00000000
```

### 2. 📡 TCP Auto-Tuning

Pastikan Receive Window Auto-Tuning aktif (default = `normal`):
```powershell
netsh int tcp set global autotuninglevel=normal
netsh int tcp show global  # verifikasi
```

### 3. 🔄 Receive Side Scaling (RSS)

Enable RSS di NIC:
```powershell
Get-NetAdapterRss              # cek status
Enable-NetAdapterRss -Name *   # enable semua
```

### 4. 🖧 RSS Queue & Vtune

Set RSS queue ke maksimum di driver NIC (Properti NIC → Advanced → RSS Queues → Maximum).

### 5. 🔌 Lalu Lintas & Jaringan

- **Gunakan kabel LAN** (jangan WiFi)
- **Nonaktifkan antrian QoS level lain** jika ada
- **Nonaktifkan semua metering/bandwidth limiter** di NIC

### 6. 💾 Cluster Size (Format Disk)

Untuk mendapatkan performa baca/tulis (I/O) yang paling optimal, sangat disarankan untuk melakukan format disk dengan **Cluster Size minimal 32KB atau 64KB**.
Pastikan aturan ini diterapkan di semua disk yang terlibat:
- Disk Writeback (Cache)
- Gamedisk (Tempat penyimpanan game)
- Imagedisk / VHD OS

### 7. 🧠 Server-side code (done)

- `set_nodelay(true)` — disable Nagle
- `SO_SNDBUF = 512KB` — TCP send buffer besar
- `info!()` → `trace!()` di hot path — zero-cost di release
- Read-ahead 256KB di backend (cache sequential)
- Single `Vec` batch per DATA_IN + SCSI_RESPONSE — minimal syscall

## 📊 CrystalDiskMark Target

| Metric | Target |
|--------|--------|
| Sequential Read | ≥ 112 MB/s (950 Mbps) |
| 4K QD32 | Max possible |

## 🏗️ Architecture

```
iSCSI Initiator (Client)
    ↓ TCP :3300
Server (tokio async)
    ├── Session (per-client state machine)
    ├── PDU (builder + parser)
    ├── Backend (physical disk read-ahead)
    └── Cache (writeback per-client)
```

## 🧩 Dependencies

- `tokio` — async runtime
- `parking_lot` — fast mutex
- `toml` / `serde` — config parsing
- `tracing` / `tracing-subscriber` — logging

## 📄 License

MIT
