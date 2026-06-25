# Planning: Rust iSCSI Server dengan Writeback Cache

## Overview

iSCSI server custom berbasis Rust untuk game disk, menggantikan iSCSI game dari CCBoot yang throttling. OS tetap menggunakan CCBoot, hanya game disk yang dialihkan ke server ini.

---

## Arsitektur

```
Client (iPXE)
├── iSCSI #1 → CCBoot Server       (OS disk, tidak diubah)
└── iSCSI #2 → Rust Server (BARU)  (Game disk)
                      │
                      ▼
            Per-client writeback cache
            cache_<mac>.bin (sparse, dinamis)
                      │
              ┌───────┴───────┐
              ▼               ▼
          SSD Cache       Physical Disk
        (write target)    (read source)
```

### Flow Writeback

```
READ:   cek block_map → hit? baca SSD cache : baca Physical Disk
WRITE:  tulis ke SSD cache → update block_map → selesai (non-blocking)
```

### Lifecycle Cache

```
Client connect    → buat cache_<mac>.bin (sparse, 0 bytes awal)
Client write      → file bertambah sesuai data yang ditulis
Client disconnect → cache_<mac>.bin DIHAPUS → fresh session
```

---

## Project Structure

```
rust-iscsi-server/
├── Cargo.toml
├── config.toml
└── src/
    ├── main.rs          ← CLI entry point, load config, start server
    ├── server.rs        ← TCP listener, accept connection, spawn session
    ├── session.rs       ← Per-client iSCSI state machine
    ├── pdu/
    │   ├── mod.rs       ← PDU types & constants
    │   ├── parser.rs    ← Raw bytes → PDU struct
    │   └── builder.rs   ← PDU struct → Raw bytes (response)
    ├── scsi.rs          ← Handle SCSI commands (READ10, WRITE10, dll)
    ├── cache.rs         ← Sparse writeback cache per-client
    └── backend.rs       ← Baca physical disk (read-only)
```

---

## Dependencies (Cargo.toml)

| Crate | Fungsi |
|-------|--------|
| `tokio` | Async runtime, TCP listener, per-client task |
| `bytes` | Buffer handling PDU yang efisien |
| `dashmap` | Concurrent HashMap untuk block map |
| `tracing` | Logging per-session |
| `serde` + `toml` | Config file parsing |
| `thiserror` | Error handling yang ergonomis |
| `uuid` | Session ID generator |

---

## Config (config.toml)

```toml
[server]
port = 3260
target_iqn = "iqn.2024-01.com.gameserver:gamedisk"

[storage]
physical_disk = "\\.\PhysicalDrive1"   # Game HDD (read-only)
cache_dir     = "D:\\iscsi-cache"       # SSD path untuk cache
block_size    = 512                     # bytes per block

[cache]
max_cache_per_client_gb = 50            # batas maksimal cache per client
```

---

## iSCSI PDU yang Diimplementasi

### Login Phase
| PDU | Opcode | Keterangan |
|-----|--------|------------|
| `LoginRequest` | `0x03` | Dari client saat pertama connect |
| `LoginResponse` | `0x23` | Balasan server, negotiate parameter |

### Full Feature Phase
| PDU | Opcode | Keterangan |
|-----|--------|------------|
| `SCSICommand` | `0x01` | Perintah dari client |
| `SCSIResponse` | `0x21` | Balasan status dari server |
| `DataOut` | `0x05` | Data write dari client ke server |
| `DataIn` | `0x25` | Data read dari server ke client |
| `LogoutRequest` | `0x06` | Client minta disconnect |
| `LogoutResponse` | `0x26` | Balasan server |
| `Nop-Out` / `Nop-In` | `0x00` / `0x20` | Keepalive |

---

## SCSI Commands yang Diimplementasi

| Command | Opcode | Keterangan |
|---------|--------|------------|
| `TEST UNIT READY` | `0x00` | Cek disk siap |
| `INQUIRY` | `0x12` | Info device |
| `MODE SENSE (6)` | `0x1A` | Parameter disk |
| `READ CAPACITY (10)` | `0x25` | Ukuran disk |
| `READ (10)` | `0x28` | Baca data |
| `WRITE (10)` | `0x2A` | Tulis data |

---

## Detail Implementasi Cache

### Block Map (RAM)

```rust
// Key   = LBA (Logical Block Address) dari client
// Value = offset byte di dalam cache file
HashMap<u64, u64>
```

### Sparse File Strategy

```
Awal connect      : cache_aabbccddeeff.bin = 0 bytes
Client tulis block 512   : file = 512 bytes, map[512] = 0
Client tulis block 9999  : file = 1024 bytes, map[9999] = 512
Client tulis block 1     : file = 1536 bytes, map[1] = 1024
Client disconnect : file DIHAPUS
```

File hanya tumbuh saat ada block baru yang ditulis. Block yang sama ditulis ulang → update di offset yang sama (tidak nambah file).

### Identifier Client

Menggunakan **Initiator IQN** dari iSCSI Login Request sebagai nama cache file. Lebih reliable dari IP karena tidak berubah meski DHCP.

```
iqn.2024-01.com.client:aa-bb-cc-dd-ee-ff
→ cache_aabbccddeeff.bin
```

---

## Flow Detail per Fase

### A. Startup

```
main.rs
  → baca config.toml
  → buka physical disk (read-only, O_RDONLY)
  → bind TCP 0.0.0.0:3260
  → mulai tokio runtime
  → loop accept connection
```

### B. Client Connect

```
server.rs
  → accept TCP stream
  → spawn tokio::task untuk session baru
  → buat Session { state: LoginPhase, ... }
```

### C. iSCSI Login

```
session.rs
  → recv LoginRequest PDU
  → parse Initiator IQN
  → negotiate: MaxRecvDataSegmentLength, HeaderDigest, dll
  → kirim LoginResponse (success)
  → state → FullFeaturePhase
  → panggil cache::init(initiator_iqn)
```

### D. Cache Init

```
cache.rs
  → derive filename dari IQN → cache_<id>.bin
  → buka/buat file di cache_dir (sparse)
  → inisialisasi block_map: HashMap::new()
  → next_write_offset = 0
```

### E. SCSI Command Loop

```
session.rs
  → recv SCSICommand PDU
  → dispatch ke scsi.rs

  READ(10):
    lba, length dari PDU
    untuk tiap block:
      jika block_map.contains(lba) → baca dari cache file
      jika tidak → baca dari physical disk
    kirim DataIn PDU

  WRITE(10):
    terima DataOut PDU dari client
    untuk tiap block:
      jika block_map.contains(lba) → overwrite di offset lama
      jika tidak → append ke akhir cache file
                   block_map.insert(lba, next_offset)
                   next_offset += block_size
    kirim SCSIResponse (success)

  INQUIRY / READ CAPACITY:
    jawab dari metadata physical disk
    (size, vendor string, dll)
```

### F. Client Disconnect

```
session.rs
  → detect TCP close atau LogoutRequest
  → cache::cleanup(initiator_iqn)
      → hapus cache_<id>.bin dari SSD
      → drop block_map dari memory
  → log "Session ended, cache cleaned"
```

---

## Milestone Testing

| Phase | Target | Sukses Jika |
|-------|--------|-------------|
| **1** | TCP + Login | Client connect, login berhasil, tidak crash |
| **2** | INQUIRY + READ CAPACITY | Client detect ukuran disk |
| **3** | READ (tanpa cache) | Client bisa baca data dari physical disk |
| **4** | WRITE + cache | Data tersimpan di `.bin` SSD |
| **5** | READ dengan cache hit | Data tulis → baca dari SSD, bukan HDD |
| **6** | Disconnect cleanup | `.bin` terhapus otomatis saat client disconnect |
| **7** | Multi-client | 2+ client konek bersamaan, cache terpisah |

---

## iPXE Script (Testing)

```bash
#!ipxe

# Tetap boot OS dari CCBoot
set initiator-iqn iqn.2024-01.com.client:${mac}

# Attach game disk dari Rust server sebagai drive tambahan
sanhook --drive 0x81 iscsi:192.168.1.100::3260:1:iqn.2024-01.com.gameserver:gamedisk

# Boot OS dari CCBoot seperti biasa
sanboot iscsi:192.168.1.50::3260:1:iqn.2024-01.com.ccboot:osdisk
```

---

## Keuntungan Dibanding CCBoot iSCSI Game

| Aspek | CCBoot Game iSCSI | Rust Server Kita |
|-------|-------------------|------------------|
| Throughput | ~600 Mbps (75 MB/s) | ~900+ Mbps (~110 MB/s) |
| Write latency | Tinggi (langsung ke HDD) | Rendah (ke SSD cache) |
| Throttling | Ada (lisensi/software) | Tidak ada |
| Update game | Di server, semua dapat | Di server, semua dapat ✅ |
| Anti-cheat | Client bisa modif | Tidak bisa (fresh tiap sesi) ✅ |
| Cache cleanup | Manual | Otomatis saat disconnect ✅ |
