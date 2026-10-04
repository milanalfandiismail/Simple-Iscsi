# Windows System Tray Service Controls (Enable & Disable) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menambahkan kontrol lengkap Enable (Start), Disable (Stop), dan Restart untuk setiap layanan (DHCP, TFTP, iSCSI) serta kontrol massal (Semua Layanan) pada Windows System Tray icon, lengkap dengan indikator status aktif (🟢 Aktif / 🔴 Nonaktif).

**Architecture:** Memanfaatkan metode `start_*`, `stop_*`, `restart_*`, dan `get_status` dari `ServiceManager` yang sudah teruji di `src/service_manager.rs`. Pada `src/tray.rs`, menu klik-kanan diperluas menggunakan Win32 Popup Submenus (`MF_POPUP`) agar rapi, compact, dan menampilkan status real-time tiap layanan tanpa memakan banyak ruang vertikal. Dispatch aksi async dilakukan melalui `runtime_handle.spawn(...)`.

**Tech Stack:** Rust 2021, Pure Win32 API (`user32.dll`, `shell32.dll`, `kernel32.dll`), Tokio 1.35 runtime handle, DashMap / ServiceManager.

**Codebase Audit (MCP `codebase-memory`):**
- Simbol diperiksa:
  - `C-Project-GIT-Simple-Iscsi.src.service_manager.ServiceManager.get_status` (`src/service_manager.rs:74-76`)
  - `C-Project-GIT-Simple-Iscsi.src.service_manager.ServiceManager.start_dhcp` (`src/service_manager.rs:97-127`)
  - `C-Project-GIT-Simple-Iscsi.src.service_manager.ServiceManager.stop_dhcp` (`src/service_manager.rs:129-151`)
  - `C-Project-GIT-Simple-Iscsi.src.service_manager.ServiceManager.start_tftp` (`src/service_manager.rs:165-195`)
  - `C-Project-GIT-Simple-Iscsi.src.service_manager.ServiceManager.stop_tftp` (`src/service_manager.rs:197-217`)
  - `C-Project-GIT-Simple-Iscsi.src.service_manager.ServiceManager.start_iscsi` (`src/service_manager.rs:230-256`)
  - `C-Project-GIT-Simple-Iscsi.src.service_manager.ServiceManager.stop_iscsi` (`src/service_manager.rs:258-278`)
  - `C-Project-GIT-Simple-Iscsi.src.tray.show_tray_menu` (`src/tray.rs:223-265`)
  - `C-Project-GIT-Simple-Iscsi.src.tray.wnd_proc` (`src/tray.rs:268-336`)
- Blast Radius: `src/tray.rs` (inbound caller dari `src/main.rs`).
- Index Coverage: `status: ready`, `parse_partial: 0`, 1.207 nodes terindeks.

---

### Task 1: Audit Simbol & Definisi Menu IDs di `src/tray.rs`

**Files:**
- Modify: `src/tray.rs:30-50`

**Interfaces:**
- Menambahkan konstanta Win32:
  `pub const MF_POPUP: u32 = 0x00000010;`
  `pub const MF_CHECKED: u32 = 0x00000008;`
- Menambahkan Menu Command IDs untuk Enable/Disable:
  - `ID_START_ALL = 1002`, `ID_STOP_ALL = 1003`, `ID_RESTART_ALL = 1004`
  - `ID_START_DHCP = 1011`, `ID_STOP_DHCP = 1012`, `ID_RESTART_DHCP = 1013`
  - `ID_START_TFTP = 1021`, `ID_STOP_TFTP = 1022`, `ID_RESTART_TFTP = 1023`
  - `ID_START_ISCSI = 1031`, `ID_STOP_ISCSI = 1032`, `ID_RESTART_ISCSI = 1033`

- [x] **Step 1: Definisikan seluruh konstanta Menu ID dan Win32 Menu Flag**
- [x] **Step 2: Validasi kompilasi lokal** (`cargo check`)

---

### Task 2: Implementasi Win32 Popup Submenus & Dynamic Status di `show_tray_menu`

**Files:**
- Modify: `src/tray.rs:220-270`

**Interfaces:**
- Consumes:
  - `state.service_manager.get_status() -> ServiceStatusReport`
  - `win32::CreatePopupMenu() -> HMENU`
  - `win32::AppendMenuW(hMenu, uFlags, uIDNewItem, lpNewItem)`
- Produces:
  - Hierarchical menu dengan status visual:
    - Menu `🌐 Buka Web Dashboard`
    - Separator
    - Submenu `⚡ Kontrol Semua Layanan`:
      - `▶️ Jalankan Semua Layanan (Enable All)`
      - `⏹️ Hentikan Semua Layanan (Disable All)`
      - `🔄 Restart Semua Layanan`
    - Separator
    - Submenu `[🟢/🔴] Layanan DHCP (UDP 67)`:
      - `▶️ Aktifkan (Enable)`
      - `⏹️ Matikan (Disable)`
      - `🔄 Restart`
    - Submenu `[🟢/🔴] Layanan TFTP (UDP 69)`:
      - `▶️ Aktifkan (Enable)`
      - `⏹️ Matikan (Disable)`
      - `🔄 Restart`
    - Submenu `[🟢/🔴] Layanan iSCSI (TCP 3260)`:
      - `▶️ Aktifkan (Enable)`
      - `⏹️ Matikan (Disable)`
      - `🔄 Restart`
    - Separator
    - Menu `🪟 Sembunyikan / Tampilkan Console`
    - Separator
    - Menu `❌ Keluar Aplikasi`

- [x] **Step 1: Buat logika pembentukan submenu per-layanan dengan pembacaan `get_status()`**
- [x] **Step 2: Sambungkan submenu ke menu root menggunakan `MF_POPUP`**
- [x] **Step 3: Pastikan seluruh submenu di-clean up dengan benar saat menu ditutup**

---

### Task 3: Implementasi Dispatch Aksi Enable/Disable pada `wnd_proc`

**Files:**
- Modify: `src/tray.rs:280-340`

**Interfaces:**
- Consumes:
  - `state.runtime_handle.spawn(async move { ... })`
  - `sm.start_all().await`, `sm.stop_all().await`, `sm.restart_all().await`
  - `sm.start_dhcp().await`, `sm.stop_dhcp().await`, `sm.restart_dhcp().await`
  - `sm.start_tftp().await`, `sm.stop_tftp().await`, `sm.restart_tftp().await`
  - `sm.start_iscsi().await`, `sm.stop_iscsi().await`, `sm.restart_iscsi().await`

- [x] **Step 1: Pasang match branches pada `WM_COMMAND` untuk seluruh ID Enable/Disable baru**
- [x] **Step 2: Dispatch ke `runtime_handle.spawn` secara thread-safe**
- [x] **Step 3: Verifikasi kompilasi global** (`cargo check`)

---

### Task 4: Verifikasi Kompilasi & Build Binary

**Files:**
- Verify: `target/debug/rust-iscsi-server.exe`

- [x] **Step 1: Jalankan `cargo check` dan pastikan zero error/panic**
- [x] **Step 2: Jalankan `cargo build` dan pastikan binary sukses terbentuk**

---

### Task 5: Living Memory & MCP Re-index

**Files:**
- Modify: [`ANTIGRAVITY.md`](file:///c:/Project%20GIT/Simple-Iscsi/ANTIGRAVITY.md)

- [x] **Step 1: Catat fitur Enable & Disable System Tray pada Bagian 8 (Matriks Fitur) dan Bagian 9 (Change Log)**
- [x] **Step 2: Jalankan MCP `codebase-memory` `index_repository` untuk menjaga kesinambungan memory**
