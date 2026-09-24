# Responsive UI (sm-2xl), Real-Time Dashboard, dan Perbaikan Restart DHCP Server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Memperbaiki seluruh antarmuka web Simple-Iscsi agar responsif di semua breakpoint (`sm`, `md`, `lg`, `xl`, `2xl`), menyatukan form pengaturan dengan 1 tombol simpan tunggal tanpa scroll di desktop (`lg+`), menyajikan pembaruan data UI secara real-time tanpa delay/refresh manual, memperbaiki backend agar restart DHCP server berjalan sempurna dan instan saat `config.toml` disimpan, serta mengompilasi CSS lokal via Tailwind CLI.

**Architecture:** 
1. **Backend Netboot & Config Synchronization:** Mengubah endpoint `POST /api/config/json` agar langsung memperbarui `SharedConfig` di memori dan memicu mekanisme *live reload / clean restart* pada `start_netboot` (menghentikan task lama, melepaskan socket UDP port 67/69 secara tuntas, dan menginisialisasi `DhcpServer` & `TftpServer` baru dengan IP pool/subnets teranyar).
2. **Real-Time UI & SSE Stream:** Memperkaya stream SSE `/api/stats/stream` untuk mengirim metrik I/O real-time, status layanan, dan daftar klien aktif (termasuk klien dinamis DHCP), serta menerapkan reactive DOM patching di `index.js` agar data terbarui seketika tanpa *page reload* atau *table flicker*.
3. **Responsive & Compact UI (Tailwind & CSS):** Mengonsolidasikan form pengaturan terpisah menjadi satu formulir terpadu dengan 1 tombol Simpan, merancang grid 3-kolom kompak pada breakpoint `lg`, `xl`, dan `2xl` sehingga bebas scrollable di layar desktop, serta menyempurnakan drawer sidebar mobile dan tabel di semua resolusi.
4. **Local Tailwind CSS Build:** Mengompilasi `ui/input.css` menjadi `ui/tailwind.css` (minified) menggunakan `@tailwindcss/cli`.

**Tech Stack:**
- **Backend:** Rust, Tokio (Async I/O & Tasks), Parking Lot (RwLock), Serde JSON/TOML, Tracing.
- **Frontend:** Vanilla HTML5, Vanilla JavaScript (ES6+, EventSource SSE, DOM Diffing), Tailwind CSS v4 (@tailwindcss/cli), CSS Custom Properties (Design System).

**Spec:** Rencana ini dibuat untuk memenuhi seluruh instruksi spesifikasi user terkait responsivitas `sm` hingga `2xl`, pembaruan real-time tanpa reload, perbaikan restart DHCP saat simpan konfigurasi, 1 tombol simpan di tab setting tanpa scroll di layar besar, dan build CSS Tailwind lokal.

---

## Global Constraints
- Seluruh kode Rust harus lulus `cargo test` tanpa error kompilasi.
- Form pengaturan (`tab-settings`) hanya memiliki **1 tombol simpan utama** yang menyimpan seluruh konfigurasi Server, DHCP, dan TFTP secara atomik.
- Pada breakpoint `lg`, `xl`, dan `2xl`, tab pengaturan harus kompak dan pas di layar tanpa mengharuskan pengguna melakukan scroll vertikal.
- Pembaruan UI untuk status klien, kecepatan baca/tulis, dan keaktifan server harus terjadi secara *real-time* via Server-Sent Events (SSE).
- Build Tailwind CSS harus dijalankan secara lokal via `npx @tailwindcss/cli -i ./ui/input.css -o ./ui/tailwind.css --minify`.

---

## Rincian Task Pelaksanaan

### Task 1: Perbaikan Backend DHCP Dynamic Reload & Instant `SharedConfig` Update

**Files:**
- Modify: `src/server_api.rs:184-211`
- Modify: `src/netboot/mod.rs:13-71`
- Modify: `src/netboot/dhcp.rs:37-122`
- Test: `src/main.rs` atau `src/netboot/dhcp.rs`

**Interfaces:**
- Consumes: `SharedConfig` dari `src/config_manager.rs`, `DhcpServer` & `TftpServer` dari `src/netboot/`.
- Produces: Live reload trigger pada netboot subsystem dan update instan `SharedConfig` saat `POST /api/config/json` dieksekusi.

- [x] **Step 1: Update `POST /api/config/json` dan `POST /api/config` di `src/server_api.rs`**
  - Tambahkan pemanggilan `config.update(cfg.clone())` setelah file `config.toml` berhasil ditulis ke disk, agar `SharedConfig` di memori langsung ter-update seketika (0 ms delay).

- [x] **Step 2: Tingkatkan deteksi perubahan konfigurasi di `src/netboot/mod.rs`**
  - Buat mekanisme pembanding konfigurasi DHCP (`DhcpConfig` comparison: `enabled`, `start_ip`, `end_ip`, `subnet_mask`, `router`, `dns`, `next_server`, `nic_ips`, `tftp_dir`, `pxe_default`, dan `server.address`).
  - Jika terjadi perubahan nilai atau status `enabled`, lakukan:
    1. Log: `Mendeteksi pembaruan konfigurasi DHCP/TFTP. Me-restart layanan netboot...`
    2. Abort `dhcp_task` dan `tftp_task`.
    3. Tunggu hingga task selesai (`let _ = h.await;`) dan berikan `tokio::time::sleep(Duration::from_millis(100))` agar socket UDP port 67 & 69 dilepaskan seutuhnya oleh OS kernel.
    4. Buat instansiasi `DhcpServer::new(config.clone(), stats.clone()).await` dan `TftpServer::new(config.clone()).await` yang baru.
    5. Spawn task baru dan simpan handle-nya.

- [x] **Step 3: Pastikan `DhcpServer::new` di `src/netboot/dhcp.rs` memproses IP Server dan Pool Baru**
  - Pastikan sender socket `DhcpServer` di-bind dengan `SO_REUSEADDR` ke alamat IP server yang baru (atau fallback ke receiver socket).
  - Pastikan `next_ip` diinisialisasi ulang dari `start_ip` baru dan antrean lease DHCP siap melayani client yang menyala.

- [x] **Step 4: Jalankan pengujian unit Rust**
  - Run: `cargo test`
  - Expected: Semua unit test lulus 100%.

- [x] **Step 5: Commit Perubahan Task 1**
  - `git add src/server_api.rs src/netboot/mod.rs src/netboot/dhcp.rs`
  - `git commit -m "fix(dhcp): instant config update and clean socket restart on save"`

---

### Task 2: Real-time UI Engine & Auto-sync Tanpa Delay

**Files:**
- Modify: `src/api/routes_stats.rs:8-55`
- Modify: `ui/index.js:123-255`
- Modify: `ui/index.js:476-520`

**Interfaces:**
- Consumes: `/api/stats/stream` (SSE) dan `/api/clients/json`.
- Produces: Real-time dashboard updates tanpa refresh halaman, auto-discovery klien dinamis, dan zero-flicker cell updating.

- [x] **Step 1: Tingkatkan Payload SSE di `src/api/routes_stats.rs`**
  - Sertakan informasi status layanan DHCP, TFTP, dan iSCSI, serta daftar klien aktif dan lease DHCP aktif dalam payload JSON yang dikirimkan setiap 1000 ms.

- [x] **Step 2: Tingkatkan fungsi `handleStatsData` di `ui/index.js`**
  - Sinkronkan daftar klien yang ada di `stats.clients` dengan tabel Dashboard dan tabel Klien Manager secara reaktif.
  - Jika ada klien yang terdeteksi aktif/booting tetapi belum terdaftar dalam `clientsObj`, tampilkan baris dinamis secara otomatis sehingga operator langsung melihat klien yang sedang menyala tanpa perlu refresh halaman.
  - Gunakan `requestAnimationFrame` dan pembaruan parsial sel tabel (`setTextIfChanged` / `setHtmlIfChanged`) agar UI tidak berkedip (*no flicker*).

- [x] **Step 3: Tambahkan Auto-Sync Polling Interval Ringan untuk Writeback & Disk Management**
  - Saat tab `writeback` atau `disk-mgmt` aktif, lakukan sinkronisasi data otomatis setiap 3-5 detik sehingga perubahan kapasitas disk/file cache selalu mutakhir.

- [x] **Step 4: Verifikasi di Browser / Node**
  - Pastikan stream SSE `/api/stats/stream` berjalan lancar dan data terbarui secara kontinu tanpa interupsi.

- [x] **Step 5: Commit Perubahan Task 2**
  - `git add src/api/routes_stats.rs ui/index.js`
  - `git commit -m "feat(ui): real-time dynamic client updates and zero-delay stats streaming"`

---

### Task 3: Redesign Tab Pengaturan (Single Save Button, IP Dropdowns, Compact Grid, Non-Scrollable pada `lg`, `xl`, `2xl`)

**Files:**
- Modify: `ui/index.html:274-405`
- Modify: `ui/index.js:870-925`
- Modify: `ui/style.css:144-250`

**Interfaces:**
- Consumes: Endpoint `GET /api/config/json`, `POST /api/config/json`, dan `GET /api/system/network_interfaces`.
- Produces: Formulir pengaturan tunggal yang rapi, padat, dan pas dalam viewport desktop (`lg:`, `xl:`, `2xl:`) dengan 1 tombol simpan utama serta pemilih dropdown IP interface jaringan.

- [x] **Step 1: Restrukturisasi HTML Tab Pengaturan di `ui/index.html`**
  - Ganti 3 `<form>` terpisah menjadi 1 formulir terpadu: `<form id="settings-form" onsubmit="saveConfigJson(event)">`.
  - Ubah input IP menjadi elemen dropdown/select interaktif dengan opsi input kustom:
    - **Server Listen IP Address (`#set-server-address`):** Dropdown yang memuat `0.0.0.0 (Semua Interface / Any)` serta seluruh IP interface fisik lokal yang terdeteksi di server.
    - **DHCP Next Server (`#set-dhcp-next`):** Dropdown yang memuat **hanya IP interface fisik lokal aktif** (tanpa `0.0.0.0`).
    - **DHCP Gateway (`#set-dhcp-gateway`):** Dropdown / datalist saran IP interface lokal (tanpa `0.0.0.0`) dengan kemampuan custom IP input.
  - Buat tata letak grid responsif yang ringkas:
    - Di layar kecil (`sm`, `md`): 1 kolom dengan scroll vertikal alami.
    - Di layar besar (`lg`, `xl`, `2xl`): Grid 3 kolom horizontal (`grid grid-cols-1 lg:grid-cols-3 gap-3 xl:gap-4`) yang membagi:
      1. **Kolom 1: Server & iSCSI Daemon** (Listen IP Dropdown, Port, Cache Size, Target IQN).
      2. **Kolom 2: DHCP Server Configuration** (Toggle Aktif, Start/End IP Pool, Subnet Mask, Gateway, DNS, Next Server Dropdown, List NIC IP).
      3. **Kolom 3: TFTP Service & Bootloader** (Root Directory, Default PXE Bootloader, Tabel Folder Bootloader ringkas).
  - Letakkan **1 Tombol Simpan Tunggal Utama** (`💾 Simpan Semua Pengaturan`) di header atau action bar atas tab Pengaturan dengan styling yang jelas dan tegas.

- [x] **Step 2: Implementasikan `loadNetworkInterfaces()` dan Perbarui `saveConfigJson` di `ui/index.js`**
  - Buat fungsi `loadNetworkInterfaces()` untuk memanggil `GET /api/system/network_interfaces` dan mengisikan opsi ke dropdown Server Address (termasuk `0.0.0.0`) dan DHCP Next Server / Gateway (tanpa `0.0.0.0`).
  - Pastikan satu fungsi `saveConfigJson` membaca seluruh input dari ketiga seksi (Server, DHCP, TFTP, dan NIC IPs), menyusun objek `configObj` lengkap, dan mengirimkannya ke `POST /api/config/json`.
  - Tampilkan toast notifikasi sukses: `✅ Semua pengaturan berhasil disimpan & server di-reload!`.

- [x] **Step 3: Terapkan Penyesuaian Ukuran Input & Padding di `ui/style.css` / Tailwind**
  - Gunakan ukuran padding yang kompak (`p-3 lg:p-4`), input tinggi `36px` / `text-sm`, dan margin kecil pada breakpoint `lg`, `xl`, `2xl` sehingga seluruh kartu pengaturan pas dalam satu layar tanpa perlu scroll.

- [x] **Step 4: Verifikasi Tampilan Desktop & Simpan Pengaturan**
  - Buka browser / tes rendering HTML untuk memastikan dropdown terisi rapi dan tidak ada elemen yang meluap (*overflow*) pada resolusi 1080p (`lg`/`xl`/`2xl`).

- [x] **Step 5: Commit Perubahan Task 3**
  - `git add ui/index.html ui/index.js ui/style.css`
  - `git commit -m "feat(ui): unified single-save settings form with network IP dropdowns and compact non-scrollable desktop layout"`

---

### Task 4: Comprehensive Responsive Optimization across `sm`, `md`, `lg`, `xl`, `2xl` & Modal Responsiveness

**Files:**
- Modify: `ui/index.html:16-140` (Layout, Sidebar, Mobile Header, Dashboard)
- Modify: `ui/index.html:143-270` (Klien, VHD, Disk Management, Writeback)
- Modify: `ui/index.html:408-568` (Modals: Client CRUD, VHD, Snapshots, Disk Config)
- Modify: `ui/style.css`

**Interfaces:**
- Consumes: Tailwind responsive utility classes (`sm:`, `md:`, `lg:`, `xl:`, `2xl:`).
- Produces: UI yang sepenuhnya adaptif dan proporsional dari smartphone (360px) hingga layar monitor 4K (2560px+).

- [x] **Step 1: Optimasi Navigasi & Mobile Header (`sm`, `md`)**
  - Pastikan tombol toggle menu mobile (`#mobile-menu-btn`) membuka dan menutup sidebar secara mulus dengan animasi transisi dan overlay gelap (`#sidebar-overlay`).
  - Menutup drawer otomatis saat pengguna memilih salah satu item menu navigasi di mobile.

- [x] **Step 2: Optimasi Grid Dashboard & Card Metrik (`sm` s/d `2xl`)**
  - `services-row`: `grid-cols-1 sm:grid-cols-3 gap-2 lg:gap-3`.
  - `stats-grid`: `grid-cols-2 lg:grid-cols-4 gap-2 lg:gap-3`.
  - Tabel I/O klien: dibungkus dalam container dengan `overflow-x-auto` yang memiliki styling scrollbar modern dan responsif.

- [x] **Step 3: Optimasi Tab Klien Manager, VHD Manager, Disk Mgmt, dan Writeback**
  - Header dengan flex-wrap untuk aksi tombol (`Tambah Klien`, `Refresh`, `Super Client`).
  - Penataan tabel dengan lebar kolom proporsional dan teks tidak bertumpuk di layar kecil maupun besar.

- [x] **Step 4: Optimasi Seluruh Modal Dialog & Tambahkan Opsi Snapshot Backup pada Super Client Confirm**
  - Pastikan seluruh modal dialog (`client-crud-modal`, `vhd-crud-modal`, `vhd-snapshots-modal`, `disk-config-modal`, `confirm-modal`) memiliki `max-h-[90vh]`, `overflow-y-auto`, dan padding responsif.
  - Pada dialog konfirmasi Super Client (`#confirm-modal`), tambahkan opsi checkbox:
    `[x] Buat Snapshot Backup (.meta) sebelum Commit` (default checked).
  - Di `ui/index.js`, teruskan parameter `create_backup: boolean` saat mengirim request `POST /api/super_client/commit`.
  - Di `src/api/routes_client.rs` (`post_superclient_commit`), baca `create_backup` (boolean). Jika `false`, lewati pemanggilan `backup_before_merge` dan langsung lakukan *fast merge* VHD.

- [x] **Step 5: Commit Perubahan Task 4**
  - `git add ui/index.html ui/style.css ui/index.js src/api/routes_client.rs`
  - `git commit -m "style(ui): comprehensive responsiveness across sm-2xl and optional super client snapshot backup"`

---

### Task 5: Build CSS Tailwind Local & Verifikasi E2E

**Files:**
- Modify: `ui/input.css`
- Output: `ui/tailwind.css`
- Modify: `package.json`

**Interfaces:**
- Consumes: `@tailwindcss/cli` dan seluruh class Tailwind di `ui/index.html` dan `ui/index.js`.
- Produces: File `ui/tailwind.css` yang telah terkompilasi dan terminifikasi.

- [x] **Step 1: Perbarui script build di `package.json`**
  - Pastikan script `npm run build:css` menggunakan perintah `npx @tailwindcss/cli -i ./ui/input.css -o ./ui/tailwind.css --minify`.

- [x] **Step 2: Jalankan Kompilasi Tailwind CSS**
  - Jalankan: `npx @tailwindcss/cli -i ./ui/input.css -o ./ui/tailwind.css --minify`
  - Pastikan file `ui/tailwind.css` ter-generate dengan sempurna tanpa error.

- [x] **Step 3: Jalankan Pengujian Backend & Build Rust**
  - Jalankan: `cargo test`
  - Jalankan: `cargo build`
  - Pastikan tidak ada regresi pada server binary.

- [x] **Step 4: Commit dan Push Perubahan Akhir**
  - `git add ui/package.json ui/input.css ui/tailwind.css`
  - `git commit -m "build(css): compile minified tailwind css with full responsive utilities"`

---

## Rencana Pengujian & Verifikasi

1. **Pengujian Simpan Pengaturan DHCP & Server:**
   - Ubah `start_ip`, `end_ip`, atau `router` di tab Pengaturan UI.
   - Klik tombol **`💾 Simpan Semua Pengaturan`**.
   - Verifikasi log server bahwa pembaruan instan terjadi dan `start_netboot` me-restart `DhcpServer` dan `TftpServer` pada port 67 & 69 secara mulus.
2. **Pengujian Real-Time UI Tanpa Refresh:**
   - Jalankan koneksi iSCSI atau DHCP client baru.
   - Pastikan metrik I/O, status Online/Offline, dan daftar klien di dashboard langsung ter-update otomatis tanpa me-refresh browser (F5).
3. **Pengujian Responsivitas Layout:**
   - Uji tampilan pada viewport mobile (`sm`: 375px–640px), tablet (`md`: 768px), laptop (`lg`: 1024px), desktop (`xl`: 1280px), dan widescreen (`2xl`: 1536px+).
   - Pastikan tab Pengaturan di layar `lg` ke atas tampil ringkas dan pas tanpa scroll vertikal yang tidak perlu.
4. **Pengujian Build CSS:**
   - Pastikan `ui/tailwind.css` memuat semua utility class breakpoint yang digunakan.
