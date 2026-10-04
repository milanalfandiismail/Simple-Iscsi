# Responsive Service Lifecycle & Windows System Tray Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:**  
Menghilangkan delay dan kebutuhan mematikan proses Rust saat restart layanan (DHCP, TFTP, iSCSI) dengan arsitektur *Service Controller* asinkronus berbasis kanal sinyal instan, perbaikan socket re-bind `SO_REUSEADDR`, jaminan **100% keandalan DHCP Client** pasca-restart (tanpa zombie task, tanpa port 67 lock, mempertahankan lease state), serta menambahkan controller **Windows System Tray** native untuk membuka Web Dashboard, menghentikan, memulai, me-restart layanan, dan menyembunyikan/menampilkan jendela konsol.

**Architecture:**  
1. **Centralized Service Manager (`src/service_manager.rs`):** Mengelola siklus hidup (*lifecycle*) worker DHCP, TFTP, dan iSCSI menggunakan `tokio::sync::broadcast` / `mpsc` dan `CancellationToken`, menggantikan *polling interval* pasif 3–5 detik menjadi event reaktif instan (< 100 ms).
2. **Jaminan Keandalan DHCP Client Pasca Restart:**
   - **Eliminasi Zombie Watcher Task:** Task loop pemantau `clients.toml` di dalam `DhcpServer` wajib dihentikan menggunakan `CancellationToken` saat restart agar `Arc<DhcpServer>` ter-drop sempurna dan socket UDP port 67 segera ditutup.
   - **Dual Socket Re-use (`SO_REUSEADDR`):** Socket receiver port 67 (`0.0.0.0:67`) dan sender port 67 (`server_ip:67`) keduanya wajib menggunakan `socket2` dengan `set_reuse_address(true)` dan `set_broadcast(true)`. Ini menjamin tidak ada lagi `os error 10048` (WSAEADDRINUSE) saat restart.
   - **Preserve Lease State Antar-Restart:** State alokasi IP (`leases` & `next_ip`) dipertahankan atau diwariskan dari instance lama / `stats.dhcp_leases`, sehingga PC client yang sedang aktif atau me-request renewal tidak mengalami bentrok IP atau pengabaian paket DHCPACK.
   - **Dual-Path Broadcast Delivery:** Menjamin balasan DHCPOFFER dan DHCPACK dikirim ke subnet broadcast terhitung (misal `192.168.137.255`) dan fallback global broadcast (`255.255.255.255`) agar terbaca oleh kartu jaringan client pada multi-NIC setup.
3. **Instant Socket Re-binding TFTP (Port 69):** Memperbaiki socket TFTP dengan `SO_REUSEADDR` agar file PXE (`ipxe.efi`) langsung dapat diunduh tanpa jeda.
4. **Graceful & Restartable iSCSI Daemon (`src/server.rs`):** Membungkus listener TCP iSCSI dalam loop yang dapat di-reload / di-restart secara terkontrol tanpa mematikan sesi client yang sedang aktif (hanya me-reload listener bind port).
5. **Service Control REST API:** Menyediakan endpoint `/api/services/status`, `/api/services/restart`, `/api/services/stop`, dan `/api/services/start`, serta memicu reload instan begitu konfigurasi disimpan melalui `/api/config/json`.
6. **Windows Native System Tray (`src/tray.rs`):** Mengintegrasikan crate `tray-item` pada thread terisolasi untuk menyediakan ikon notifikasi di taskbar Windows dengan menu interaktif (Buka Web UI, Status, Start/Stop/Restart, Toggle Console Window, Exit).
7. **Web UI Quick Actions:** Menambahkan kontrol tombol aksi langsung di kartu status iSCSI, DHCP, dan TFTP pada dashboard UI.

**Tech Stack:**  
* Rust 2021, Tokio 1.35 (Async Runtime, Broadcast Channels, Tasks)
* `socket2` (Low-level Socket Re-use & Options)
* `tray-item` 0.10 & Windows API (`ShowWindow`, `GetConsoleWindow`)
* Web API (HTTP REST, Server-Sent Events)
* Vanilla JS & Tailwind CSS

**Codebase Audit (MCP `codebase-memory`):**
* Simbol terkait:
  - `start_netboot`: [`src/netboot/mod.rs`](file:///c:/Project%20GIT/Simple-Iscsi/src/netboot/mod.rs#L13-L70) — caller: `main` (hop 1)
  - `DhcpServer::new`: [`src/netboot/dhcp.rs`](file:///c:/Project%20GIT/Simple-Iscsi/src/netboot/dhcp.rs#L37-L122)
  - `DhcpServer::run`: [`src/netboot/dhcp.rs`](file:///c:/Project%20GIT/Simple-Iscsi/src/netboot/dhcp.rs#L152-L168)
  - `TftpServer::new`: [`src/netboot/tftp.rs`](file:///c:/Project%20GIT/Simple-Iscsi/src/netboot/tftp.rs#L23-L31)
  - `start_server`: [`src/server.rs`](file:///c:/Project%20GIT/Simple-Iscsi/src/server.rs#L10-L118) — caller: `main` (hop 1)
  - `start_api_server`: [`src/server_api.rs`](file:///c:/Project%20GIT/Simple-Iscsi/src/server_api.rs#L15-L91) — caller: `main` (hop 1)
  - `start_config_watcher`: [`src/config_manager.rs`](file:///c:/Project%20GIT/Simple-Iscsi/src/config_manager.rs#L50-L140)
* Index Coverage: Semua file target berstatus `ready` dan `no_recorded_issue`.

---

## Global Constraints & Safety Rules
1. **Tidak Boleh Memutus Sesi iSCSI Aktif Tanpa Sengaja:** Saat hanya me-restart DHCP atau TFTP, sesi storage iSCSI client yang sedang bermain game / booting **tidak boleh terganggu**.
2. **Windows Socket Freedom:** Setiap pembukaan socket (UDP 67, UDP 69, TCP 3300/3260) wajib menggunakan `set_reuse_address(true)` agar dapat di-rebind seketika tanpa jeda TIME_WAIT kernel.
3. **Non-blocking Tray Message Loop:** Event loop Win32 untuk System Tray harus berjalan di thread terpisah (*dedicated thread*) agar tidak memblokir asynchronous executor Tokio.
4. **Preserve Existing Config Format:** Tidak mengubah skema `config.toml` atau `clients.toml` yang sudah ada.

---

### Task 1: Modul Sentral Pengelola Layanan (`src/service_manager.rs`)

**Files:**
- Create: [`src/service_manager.rs`](file:///c:/Project%20GIT/Simple-Iscsi/src/service_manager.rs)
- Modify: [`src/main.rs:1-25`](file:///c:/Project%20GIT/Simple-Iscsi/src/main.rs#L1-L25)

**Interfaces:**
- Consumes: `SharedConfig`, `Arc<ServerStats>`, `Arc<RwLock<HashMap<u8, Arc<Backend>>>>`
- Produces:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
  pub enum ServiceState { Running, Stopped, Restarting, Error }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct ServiceStatusReport {
      pub iscsi: ServiceState,
      pub dhcp: ServiceState,
      pub tftp: ServiceState,
  }

  pub struct ServiceManager { ... }
  impl ServiceManager {
      pub fn new(config: SharedConfig, stats: Arc<ServerStats>, backends: Arc<RwLock<...>>) -> Arc<Self>;
      pub async fn start_all(&self);
      pub async fn stop_all(&self);
      pub async fn restart_all(&self);
      pub async fn restart_dhcp(&self);
      pub async fn restart_tftp(&self);
      pub async fn restart_iscsi(&self);
      pub fn get_status(&self) -> ServiceStatusReport;
  }
  ```

- [ ] **Step 1: Buat struct data status dan enum ServiceState**
  Definisikan `ServiceState` (Running, Stopped, Error) dan `ServiceStatusReport` dengan derive `Serialize, Deserialize, Clone`.
- [ ] **Step 2: Implementasikan struct `ServiceManager` dengan kanal sinyal instan**
  Gunakan `tokio::sync::broadcast` untuk membroadcast event restart/stop ke worker, serta `Arc<Mutex<Option<JoinHandle<()>>>>` untuk memegang task handle DHCP, TFTP, dan iSCSI.
- [ ] **Step 3: Hubungkan method restart atomik per-layanan**
  Buat implementasi `restart_dhcp()`, `restart_tftp()`, `restart_iscsi()`, dan `restart_all()` yang menghentikan instance lama secara graceful lalu memicu inisialisasi instance baru seketika.
- [ ] **Step 4: Registrasikan modul di `src/main.rs`**
  Tambahkan `mod service_manager;` pada deklarasi modul di `src/main.rs`.
- [ ] **Step 5: Verifikasi kompilasi**
  Jalankan `cargo check` untuk memastikan tidak ada kesalahan borrowing atau tipe data.

---

### Task 2: Perbaikan Socket Re-bind, Eliminasi Zombie Task, & Jaminan Keandalan DHCP (`src/netboot/`)

**Files:**
- Modify: [`src/netboot/dhcp.rs:37-125`](file:///c:/Project%20GIT/Simple-Iscsi/src/netboot/dhcp.rs#L37-L125)
- Modify: [`src/netboot/dhcp.rs:152-170`](file:///c:/Project%20GIT/Simple-Iscsi/src/netboot/dhcp.rs#L152-L170)
- Modify: [`src/netboot/tftp.rs:22-55`](file:///c:/Project%20GIT/Simple-Iscsi/src/netboot/tftp.rs#L22-L55)
- Modify: [`src/netboot/mod.rs:13-70`](file:///c:/Project%20GIT/Simple-Iscsi/src/netboot/mod.rs#L13-L70)

**Interfaces:**
- Consumes: `ServiceManager` cancellation token & lease retention
- Produces: Pembuatan socket UDP port 67 & 69 dengan flag `SO_REUSEADDR`, pembersihan task background, & preservasi lease map

- [ ] **Step 1: Perbaiki socket receiver & sender DHCP (Port 67) dengan `socket2`**
  Ganti `UdpSocket::bind(addr).await?` standar dengan `socket2::Socket`:
  ```rust
  let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
  sock.set_reuse_address(true)?;
  sock.set_broadcast(true)?;
  sock.bind(&addr.into())?;
  sock.set_nonblocking(true)?;
  let socket = UdpSocket::from_std(sock.into())?;
  ```
  Terapkan `set_reuse_address(true)` pada SEMUA socket UDP (receiver maupun sender).
- [ ] **Step 2: Pasang `CancellationToken` pada task watcher `clients.toml`**
  Jangan biarkan task background di `DhcpServer::new` berjalan detached selamanya. Berikan sinyal token pembatalan sehingga saat server DHCP di-stop/restart, background task ini ikut berhenti dan melepaskan referensi `Arc<DhcpServer>`.
- [ ] **Step 3: Preservasi Lease Map Klien saat Restart**
  Agar client yang sedang terkoneksi tidak kehilangan IP atau ditolak saat mengirim DHCPREQUEST, teruskan lease map yang ada ke instance baru saat restart.
- [ ] **Step 4: Perbaiki socket TFTP (Port 69) dengan `socket2`**
  Terapkan konfigurasi `SO_REUSEADDR` yang sama pada port 69 di `src/netboot/tftp.rs`.
- [ ] **Step 5: Ganti polling interval 3 detik di `src/netboot/mod.rs`**
  Hapus polling interval pasif `interval(3 detik)`. Jadikan `start_netboot` menerima sinyal kontrol langsung dari `ServiceManager` sehingga respon restart terjadi seketika (< 10 ms).
- [ ] **Step 6: Verifikasi kompilasi**
  Jalankan `cargo check` dan pastikan binding socket bersih tanpa warning.

---

### Task 3: Graceful & Reloadable iSCSI Daemon (`src/server.rs`)

**Files:**
- Modify: [`src/server.rs:10-75`](file:///c:/Project%20GIT/Simple-Iscsi/src/server.rs#L10-L75)

**Interfaces:**
- Consumes: Sinyal shutdown dari `ServiceManager`
- Produces: `pub async fn run_iscsi_listener(...)` yang responsif terhadap sinyal restart/stop

- [ ] **Step 1: Tambahkan sinyal shutdown ke listener `start_server`**
  Ubah tanda tangan fungsi di `src/server.rs` agar menerima sinyal pembatalan (`shutdown_signal: tokio::sync::broadcast::Receiver<()>`).
- [ ] **Step 2: Integrasikan `tokio::select!` pada loop `listener.accept()`**
  Saat sinyal shutdown diterima, listener keluar dari loop dan socket port 3300/3260 langsung bebas di-rebind.
- [ ] **Step 3: Pastikan sesi client yang sudah tersambung tetap berjalan**
  Setiap koneksi client yang sudah aktif di-spawn ke task independen sehingga me-reload listener port tidak memutuskan sesi game yang sedang dimainkan client.
- [ ] **Step 4: Verifikasi kompilasi**
  Jalankan `cargo check` untuk mengonfirmasi kompatibilitas listener async.

---

### Task 4: REST API Endpoints untuk Service Control & Instant Hot-Reload (`src/server_api.rs` & `src/config_manager.rs`)

**Files:**
- Modify: [`src/server_api.rs:15-95`](file:///c:/Project%20GIT/Simple-Iscsi/src/server_api.rs#L15-L95)
- Modify: [`src/server_api.rs:184-211`](file:///c:/Project%20GIT/Simple-Iscsi/src/server_api.rs#L184-L211)
- Modify: [`src/config_manager.rs:60-138`](file:///c:/Project%20GIT/Simple-Iscsi/src/config_manager.rs#L60-L138)

**Interfaces:**
- Endpoints Baru:
  - `GET /api/services/status` -> JSON `{ "iscsi": "Running", "dhcp": "Running", "tftp": "Running" }`
  - `POST /api/services/restart` -> Body `{"service": "all" | "dhcp" | "tftp" | "iscsi"}`
  - `POST /api/services/stop` -> Body `{"service": "all" | "dhcp" | "tftp" | "iscsi"}`
  - `POST /api/services/start` -> Body `{"service": "all" | "dhcp" | "tftp" | "iscsi"}`
- Instant Hot-Reload: `POST /api/config/json` langsung memicu `service_manager.restart_all()` / reload tanpa menunggu interval 5 detik file watcher.

- [ ] **Step 1: Teruskan `Arc<ServiceManager>` ke `start_api_server`**
  Modifikasi `start_api_server` dan `handle_request` agar memiliki akses ke `Arc<ServiceManager>`.
- [ ] **Step 2: Implementasikan routing `/api/services/*`**
  Tambahkan handler untuk `GET /api/services/status`, `POST /api/services/restart`, `POST /api/services/stop`, dan `POST /api/services/start`.
- [ ] **Step 3: Hubungkan pembaruan config dengan reload seketika**
  Di handler `POST /api/config/json`, setelah file `config.toml` berhasil ditulis, panggil reload konfigurasi pada `ServiceManager` secara langsung.
- [ ] **Step 4: Turunkan interval file watcher atau sediakan trigger eksplisit**
  Sesuaikan `src/config_manager.rs` agar bekerja sinergis dengan `ServiceManager` tanpa memicu re-reload ganda.
- [ ] **Step 5: Verifikasi kompilasi**
  Jalankan `cargo check` dan validasi serialisasi JSON response.

---

### Task 5: Windows Native System Tray Icon (`src/tray.rs` & `src/main.rs`)

**Files:**
- Create: [`src/tray.rs`](file:///c:/Project%20GIT/Simple-Iscsi/src/tray.rs)
- Modify: [`src/main.rs:170-258`](file:///c:/Project%20GIT/Simple-Iscsi/src/main.rs#L170-L258)
- Modify: [`Cargo.toml`](file:///c:/Project%20GIT/Simple-Iscsi/Cargo.toml) (sudah ditambahkan `tray-item = "0.10.0"`)

**Interfaces:**
- Produces: `pub fn init_system_tray(service_mgr: Arc<ServiceManager>, web_url: String) -> Result<...>`
- Tray Menu Options:
  1. `🌐 Buka Web Dashboard`
  2. `---` (Separator)
  3. `🔄 Restart Semua Layanan`
  4. `⏹️ Hentikan Layanan`
  5. `▶️ Mulai Layanan`
  6. `---` (Separator)
  7. `🪟 Sembunyikan/Tampilkan Console`
  8. `❌ Keluar`

- [ ] **Step 1: Buat modul `src/tray.rs` dengan wrapper `tray-item`**
  Implementasikan inisialisasi tray icon pada Windows. Buat helper pembuka browser:
  ```rust
  pub fn open_browser(url: &str) {
      let _ = std::process::Command::new("cmd")
          .args(["/C", "start", url])
          .spawn();
  }
  ```
- [ ] **Step 2: Implementasikan toggle visibilitas jendela konsol (Hide/Show)**
  Gunakan fungsi Win32 bawaan untuk menyembunyikan/menampilkan konsol:
  ```rust
  #[cfg(windows)]
  pub fn toggle_console_window() {
      unsafe {
          use windows_sys::Win32::System::Console::GetConsoleWindow;
          use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, IsWindowVisible, SW_HIDE, SW_SHOW};
          let hwnd = GetConsoleWindow();
          if hwnd != 0 {
              if IsWindowVisible(hwnd) != 0 {
                  ShowWindow(hwnd, SW_HIDE);
              } else {
                  ShowWindow(hwnd, SW_SHOW);
              }
          }
      }
  }
  ```
- [ ] **Step 3: Hubungkan aksi tray menu ke `ServiceManager`**
  Sambungkan menu item "Restart Semua Layanan", "Hentikan Layanan", dan "Mulai Layanan" ke metode async `ServiceManager` melalui `tokio::runtime::Handle::current().spawn(...)`.
- [ ] **Step 4: Jalankan System Tray di dedicated thread pada `src/main.rs`**
  Di `main()`, spawn thread tersendiri untuk mengoperasikan tray loop tanpa mengganggu Tokio executor:
  ```rust
  let service_mgr_tray = service_manager.clone();
  std::thread::spawn(move || {
      if let Err(e) = tray::run_tray(service_mgr_tray, "http://127.0.0.1:8080".to_string()) {
          tracing::error!("Gagal menjalankan System Tray: {}", e);
      }
  });
  ```
- [ ] **Step 5: Verifikasi kompilasi**
  Jalankan `cargo check` dan pastikan tidak ada conflict symbol atau threading error.

---

### Task 6: Tampilan UI Responsif untuk Kontrol Layanan (`ui/index.html` & `ui/index.js`)

**Files:**
- Modify: [`ui/index.html:95-125`](file:///c:/Project%20GIT/Simple-Iscsi/ui/index.html#L95-L125)
- Modify: [`ui/index.js:1430-1460`](file:///c:/Project%20GIT/Simple-Iscsi/ui/index.js#L1430-L1460)

**Interfaces:**
- Consumes: Endpoints `/api/services/status`, `/api/services/restart`, `/api/services/start`, `/api/services/stop`
- Produces: Tombol aksi instan pada kartu status layanan dan umpan balik toast notifikasi yang cepat.

- [ ] **Step 1: Tambahkan tombol aksi pada Service Status Cards di `ui/index.html`**
  Pada masing-masing kartu (iSCSI, DHCP, TFTP), tambahkan tombol icon mini:
  - Tombol 🔄 `Restart`
  - Tombol ⏹️ / ▶️ `Stop/Start`
  Serta tambahkan tombol global `"🔄 Restart Semua Layanan"` di area header overview.
- [ ] **Step 2: Implementasikan fungsi aksi di `ui/index.js`**
  Buat fungsi helper `restartService(serviceName)`, `stopService(serviceName)`, `startService(serviceName)`, dan `restartAllServices()`.
- [ ] **Step 3: Berikan indikator visual loading & toast instan**
  Saat tombol diklik, ubah status pill menjadi `"⏳ Restarting..."` selama proses berlangsung, lalu refresh status segera setelah response diterima (< 200 ms).
- [ ] **Step 4: Verifikasi visual & alur data**
  Pastikan UI tetap rapi pada breakpoint mobile hingga desktop (2xl).

---

### Task 7: Verifikasi Menyeluruh, Audit MCP, & Living Memory Update

**Files:**
- Modify: [`ANTIGRAVITY.md`](file:///c:/Project%20GIT/Simple-Iscsi/ANTIGRAVITY.md)

- [ ] **Step 1: Validasi Kompilasi Lengkap**
  Jalankan `cargo check` dan `cargo build` untuk memastikan seluruh biner bersih dari error.
- [ ] **Step 2: Audit Perubahan dengan MCP `detect_changes`**
  Jalankan MCP `detect_changes` untuk memastikan semua simbol baru dan caller terpetakan dengan tepat.
- [ ] **Step 3: Update `ANTIGRAVITY.md`**
  Perbarui:
  - Bagian 4: Tambahkan modul `src/service_manager.rs` dan `src/tray.rs`.
  - Bagian 7: Perbarui status modul Service Management dan System Tray menjadi **STABLE / READY**.
  - Bagian 8: Tambahkan entri log riwayat pengerjaan fitur ini.
- [ ] **Step 4: Re-index repositori**
  Jalankan `index_repository` pada MCP `codebase-memory` agar graph simbol terbarukan.
