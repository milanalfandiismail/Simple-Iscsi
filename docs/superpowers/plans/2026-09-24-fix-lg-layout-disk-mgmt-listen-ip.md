# Fix Breakpoint LG Layout, Disk Management Persistence, and 0.0.0.0 Listen IP

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Perbaiki tampilan tabel pada breakpoint LG agar kolom Uptime tidak terpisah atau keluar dari card, selaraskan schema penyimpanan Disk Management dengan backend Rust agar parameter tersimpan akurat, dan tambahkan `0.0.0.0` pada dropdown Server Listen Interface IP.

**Architecture:** 
- Frontend Tailwind CSS table layout dioptimalkan dengan menghilangkan `lg:overflow-x-visible`, memperketat padding pada breakpoint `lg`, dan menyederhanakan header sehingga tabel 6 kolom muat rapi di dalam card container.
- Frontend JS Disk Management diselaraskan dengan struct Rust `Config` (`writeback_dirs`, `max_cache_per_client_gb`, `max_write_speed_mbps`, `windows.vhd_dir`, `gamedisk`).
- Dropdown `set-server-address` selalu menyertakan `0.0.0.0 (Semua Interface)` sebagai pilihan default/utama di atas daftar interface fisik.

**Tech Stack:** HTML5, Vanilla JavaScript, Tailwind CSS v4, Rust (Axum/Tokio Engine).

---

### Task 1: Perbaiki Layout Tabel Breakpoint LG (Dashboard & Klien Manager)

**Files:**
- Modify: `ui/index.html:141-155, 177-195`
- Modify: `ui/index.js:445-480, 520-555`

- [ ] **Step 1: Update container overflow dan table header class di `ui/index.html`**
  - Ganti `overflow-x-auto lg:overflow-x-visible` menjadi `w-full overflow-x-auto rounded-lg border border-stone-200`.
  - Sesuaikan padding cell pada `th` dari `py-2.5 px-3 lg:py-3 lg:px-3.5 xl:py-3.5 xl:px-4` menjadi `py-2.5 px-2.5 lg:py-2.5 lg:px-3 xl:py-3 xl:px-4 text-[11px] lg:text-xs font-semibold uppercase tracking-wider text-stone-600`.
  - Sederhanakan judul header kolom: `Klien & Status`, `Jaringan`, `Image & PXE`, `Read I/O`, `Write I/O`, `Uptime`.

- [ ] **Step 2: Update row rendering class di `ui/index.js`**
  - Pada `renderDashboardClientsTable` dan `renderClientsManagerTable`, sesuaikan padding `td` menjadi `py-2 px-2.5 lg:py-2.5 lg:px-3 xl:py-3 xl:px-4`.
  - Kurangi ukuran teks atau beri `truncate max-w-[130px] lg:max-w-[120px] xl:max-w-[160px]` pada image manager badge.
  - Pastikan seluruh 6 kolom pas di dalam kontainer pada resolusi 1024px tanpa melebar keluar card.

---

### Task 2: Selaraskan Disk Management Schema & Storage Parameters Persistence

**Files:**
- Modify: `ui/index.js:860-960, 995-1060`

- [ ] **Step 1: Sesuaikan schema `loadConfigJson` untuk storage parameters**
  - Baca `configObj.writeback.max_cache_per_client_gb` ke `disk-max-cache-gb` (default 10).
  - Baca `configObj.writeback.max_write_speed_mbps` ke `disk-throttle-mb` (default 100000).

- [ ] **Step 2: Sesuaikan fungsi `saveGlobalStorageParams`**
  - Simpan ke `configObj.writeback.max_cache_per_client_gb = maxCacheGb`.
  - Simpan ke `configObj.writeback.max_write_speed_mbps = throttleMb`.
  - Panggil `await saveConfigJsonFull()` dan pastikan berhasil disimpan ke backend.

- [ ] **Step 3: Sesuaikan fungsi deteksi peran partisi dan `savePartitionRoleAction`**
  - Deteksi peran di `renderDiskGrid`:
    - Role `boot`: `configObj.windows?.vhd_dir?.toUpperCase().startsWith(drive.letter.toUpperCase())`
    - Role `writeback`: `configObj.writeback?.writeback_dirs?.some(d => d.toUpperCase().startsWith(drive.letter.toUpperCase()))`
    - Role `gamedisk`: `configObj.gamedisk?.some(gd => gd.physical_disk === drive.physical_disk)`
  - Di `savePartitionRoleAction`:
    - Jika role `boot`: set `configObj.windows.vhd_dir = \`${driveLetter}:\\\\vhd\``.
    - Jika role `writeback`: set `configObj.writeback.writeback_dirs = [\`${driveLetter}:\\\\writeback\`]`.
    - Jika role `gamedisk`: daftarkan physical disk ke `configObj.gamedisk` jika belum ada.
    - Simpan via `saveConfigJsonFull()`, refresh grid, dan tampilkan toast sukses.

---

### Task 3: Tambahkan `0.0.0.0` pada Dropdown Server Listen Address

**Files:**
- Modify: `ui/index.js:220-245`

- [ ] **Step 1: Update `populateNetworkDropdowns()`**
  - Pada `serverSelect` (`set-server-address`), tambahkan opsi `0.0.0.0 (Semua Interface)` sebagai opsi paling pertama (`new Option('0.0.0.0 (Semua Interface)', '0.0.0.0')`).
  - Tambahkan list interface fisik (`availableNetworkIps`) setelahnya.
  - Setel `serverSelect.value` sesuai konfigurasi aktif dari `configObj.server.address`.

---

### Task 4: Kompilasi & Verifikasi

**Files:**
- Output: `ui/tailwind.css`

- [ ] **Step 1: Jalankan `npm run build:css`**
- [ ] **Step 2: Jalankan `cargo test` untuk validasi backend**
- [ ] **Step 3: Update indeks `codebase-memory`**
- [ ] **Step 4: Commit perubahan ke Git**
