# CLI Reference — Simple iSCSI Server

> Dokumentasi semua perintah CLI biar gak lupa.

---

## 🚀 Start Server

```bash
cargo run
```
Atau kalo binary:
```bash
simple-iscsi.exe
```

---

## 💾 Super Client — Commit

Merge perubahan super VHD ke base image (otomatis backup dulu).

```bash
cargo run -- --commit <hostname>
```

**Contoh:**
```bash
cargo run -- --commit PC-01
```

**Alur:**
1. ✅ Backup base image → `{base}_backup1.vhd`
2. ✅ Merge super VHD → base image
3. ✅ Hapus super VHD
4. ✅ Info selesai

---

## 🗑️ Super Client — Discard

Buang semua perubahan super VHD (kembali ke base image semula).

```bash
cargo run -- --discard <hostname>
```

**Contoh:**
```bash
cargo run -- --discard PC-01
```

**Alur:**
1. ✅ Hapus super VHD
2. ✅ Semua perubahan di client hilang
3. ✅ Pas login ulang, super VHD bikin baru dari base

---

## 📋 Restore List — Lihat Daftar Backup

Lihat semua file backup yang tersedia untuk image milik suatu client.

```bash
cargo run -- --restore-list <hostname>
```

**Contoh:**
```bash
cargo run -- --restore-list PC-01
```

**Output:**
```
📋 Backup untuk windows_11_tm:
  [1] E:\Windows 24H2\Windows_24H2_Modern_backup1.vhd
  [2] E:\Windows 24H2\Windows_24H2_Modern_backup2.vhd
```

---

## ↩️ Restore — Kembalikan Base Image

Kembalikan base image dari file backup.

### Restore backup terakhir:
```bash
cargo run -- --restore <hostname>
```

### Restore backup spesifik:
```bash
cargo run -- --restore <hostname> <index>
```

**Contoh:**
```bash
cargo run -- --restore PC-01         # backup terakhir
cargo run -- --restore PC-01 1       # backup ke-1
cargo run -- --restore PC-01 2       # backup ke-2
```

**Alur:**
1. ✅ Cari file backup sesuai index
2. ✅ Copy backup → timpa base image
3. ✅ Hapus super VHD (biar sinkron)
4. ✅ Info sukses

---

## ✅ Reload — Validasi Config

Validasi `clients.toml` tanpa restart server.

```bash
cargo run -- --reload
```

**Output:**
```
✅ clients.toml valid! 3 client(s) dimuat.
```

---

## 📝 Catatan Penting

| Perintah | Argumen | Keterangan |
|----------|---------|------------|
| `--commit` | `<hostname>` | Hostname dari `clients.toml` (contoh: `PC-01`) |
| `--discard` | `<hostname>` | Sama |
| `--restore-list` | `<hostname>` | Sama |
| `--restore` | `<hostname> [index]` | Index opsional, default: terakhir |
| `--reload` | *(none)* | Validasi config aja |

> [!TIP]
> Hostname adalah value dari field `hostname = "..."` di `clients.toml`, **bukan** IP address.
