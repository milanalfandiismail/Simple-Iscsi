# Plan: Driverless Disk Cache Refactor & Super Client Commit Sync Fix

**Tanggal:** 21 Oktober 2026  
**Status:** Siap Dieksekusi  
**Tujuan:**
1. Merombak arsitektur iSCSI RAM Cache Monitor dari level file `.vhd` menjadi **Driverless Assigned Disk Cache** (Image Disk, GameDisk, Writeback Disk) dengan prinsip **Zero Auto-Assign** (manual, default 0 MB / Nonaktif).
2. Memperbaiki bug status Super Client yang tetap aktif (`⚡ Super`) setelah proses Commit / Save selesai akibat race condition pada background merge polling dan sinkronisasi config.

---

## 1. Masalah Utama & Root Cause Analysis

### 1.1 Masalah iSCSI RAM Cache
* **Root Cause 1:** Backend (`routes_cache.rs` dan `writeback_imagedisk.rs`) mendaftarkan setiap file `.vhd` individual (seperti `win10.vhd`) ke dalam `IMAGE_CACHE_REGISTRY` secara otomatis saat booting.
* **Root Cause 2:** Nilai `read_cache_gb = 2` pada section `[server]` di `config.toml` dijadikan fallback otomatis (`ram_cache_mb.unwrap_or(2048)`), sehingga server mencadangkan 2 GB RAM secara otomatis tanpa persetujuan user.
* **Target Solusi:** 
  - Hapus penjejakan file `.vhd` individual pada cache monitor.
  - Tampilkan hanya disk fungsional yang ter-assign di sistem:
    1. **Image Disk**: Read-Only Cache (akselerasi boot OS).
    2. **GameDisk**: Read-Only Cache (akselerasi loading game).
    3. **Writeback Disk**: Write-Only Cache / Write Buffer (menampung penulisan klien).
  - Alokasi awal untuk setiap disk yang di-assign adalah murni **0 MB (Nonaktif / Bypass RAM)**. Server dilarang membuat pool RAM sebelum user menekan tombol "Atur" dan memilih ukuran MB/GB.

### 1.2 Masalah Super Client Status Tetap Aktif Pasca-Commit
* **Root Cause 1 (Race Condition Polling):** Saat user menekan tombol **Commit** pada modal Super Client, backend memanggil `tokio::spawn` yang terlebih dahulu menjalankan `backup_before_merge` secara asinkron. Flag `IS_MERGING` baru di-set `true` di dalam `merge_vhd_sync`.
* **Root Cause 2 (Early Interval Termination):** Frontend langsung memicu polling `/api/vhd/merge_status` 750ms setelah request commit. Karena `backup_before_merge` masih berjalan dan `IS_MERGING` belum sempat menjadi `true`, endpoint mengembalikan `is_merging: false` dan `error: null`. Frontend menganggap merge sudah selesai seketika, menghentikan timer polling (`clearInterval`), dan memanggil `loadConfigJson()`.
* **Root Cause 3 (Stale Config Reload):** Karena proses merge di background belum selesai, `clear_super_client()` belum dipanggil di backend. `loadConfigJson()` mengambil file `config.toml` yang masih memuat `super_client_ip = "192.168.x.x"`. Akibatnya, badge `⚡ Super` tetap menempel pada klien dan timer polling sudah mati.
* **Target Solusi:**
  - Set `IS_MERGING = true` secara sinkron dan instan sebelum mengembalikan respon 200 OK dari endpoint `/api/superclient/commit`.
  - Pastikan siklus `is_merging` mencakup tahap backup, merge, hapus super VHD, hingga `clear_super_client()`.
  - Perbarui state memori `SharedConfig` dan file `config.toml` seketika setelah merge selesai, lalu kirim status bersih ke frontend.

---


### 1.3 Kebutuhan UI Khusus: Dedicated Merge Progress Modal (Dukungan Perubahan Besar s/d 10GB+)
* **Problem Saat Ini:** Widget progress merge saat ini berupa *floating toast* kecil di pojok kanan bawah (`#vhd-merge-progress-widget`). Jika super client melakukan perubahan besar (misal install game/update Windows sebesar 5GB–10GB+), proses merge membutuhkan waktu puluhan detik hingga menit. Widget pojok rentan terlewatkan atau terabaikan, dan user bisa tidak sengaja mengklik menu lain saat penulisan VHD kritis sedang berlangsung.
* **Solusi Desain Modal:**
  - Ubah tampilan progress merge menjadi **Dedicated Centered Modal Dialog** (`#modal-superclient-merge`):
    * **Modal Backdrop:** Menutup layar dengan backdrop blur semi-transparan untuk mencegah interaksi atau navigasi yang tidak disengaja selama penulisan file VHD.
    * **Header Dialog:** Ikon animasi ⚡ dan judul "Memproses Commit Super Client".
    * **Detail Progres Real-Time:** 
      - Bar progress visual halus (0% s/d 100%).
      - Indikator teks blok: misal `Menggabungkan: 3,450 / 8,192 blok data (42%)`.
      - Indikator status tahapan: `Mempersiapkan snapshot metadata...` -> `Menyalin blok data differencing ke Master VHD...` -> `Membersihkan file sementara...`.
    * **Proteksi Selama Proses:** Selama merge masih berjalan (`is_merging === true`), tombol tutup / escape di-disable untuk melindungi integritas disk.
    * **State Sukses (100%):** Ikon berganti menjadi ✅ hijau, progress bar berubah hijau penuh, menampilkan pesan konfirmasi jelas: *"Berhasil! Perubahan disk sebesar {size} telah sukses digabungkan ke Master VHD dan mode Super Client resmi dinonaktifkan."*, dan memunculkan tombol **"Tutup / Selesai"**.
    * **State Gagal (Error):** Menampilkan badge merah dan pesan error dengan opsi "Tutup" untuk memeriksa log.

## 2. Rincian Langkah Implementasi

### Tahap 1: Backend Refactor — Driverless Assigned Disk Cache

1. **Struktur Konfigurasi (`src/config.rs`):**
   - Tambahkan parameter `pub ram_cache_mb: Option<u64>` ke dalam `WritebackConfig`.
   - Ubah fallback default: jika `ram_cache_mb` bernilai `None` atau `Some(0)`, maka nilainya adalah 0 (Nonaktif), BUKAN fallback ke `server.read_cache_gb`.
   - Depresiasi fallback otomatis dari `server.read_cache_gb`.

2. **Cache Registry & Backends (`src/read_ahead.rs`, `src/writeback_imagedisk.rs`, `src/writeback_gamedisk.rs`):**
   - Hapus registrasi path file `.vhd` individual di `IMAGE_CACHE_REGISTRY`.
   - Ganti menjadi **Disk-Level Shared Cache Pool**:
     * Pool 1: `Image Disk Cache Pool` (diasosiasikan dengan `[windows].vhd_dir` atau drive tempat OS images disimpan). Jika `ram_cache_mb == 0`, shared read cache tidak dialokasikan.
     * Pool 2..N: `GameDisk Cache Pool` (diasosiasikan dengan setiap `physical_disk` di `[[gamedisk]]`).
     * Pool N+1..M: `Writeback Write Cache Pool` (diasosiasikan dengan direktori writeback di `[writeback].writeback_dirs`).
   - Sediakan fungsi live resize (`update_cache_capacity_mb`) agar perubahan kapasitas langsung aktif di RAM tanpa restart service.

3. **API Cache Stats & Tuning (`src/api/routes_cache.rs`):**
   - Rombak `handle_get_cache_stats`:
     * Jangan membaca file `.vhd` individual.
     * Hanya iterasi disk yang ter-assign:
       1. **Image Disk**: disk_id `disk_image`, name `Image Disk (Base OS)`, path dari `windows.vhd_dir`, disk_type `imagedisk_read`.
       2. **GameDisk**: disk_id `gamedisk_{lun}`, name `GameDisk LUN {lun}`, path dari `gamedisk.physical_disk`, disk_type `gamedisk_read`.
       3. **Writeback Disk**: disk_id `writeback_{idx}`, name `Writeback Storage {idx}`, path dari `writeback_dirs[idx]`, disk_type `writeback_write`.
     * Jika `ram_cache_mb` adalah 0, set status: Nonaktif / 0 MB.
   - Sempurnakan `handle_post_cache_tune`:
     * Menerima disk_id (`disk_image`, `gamedisk_{lun}`, `writeback_{idx}`).
     * Terapkan live hot-apply ke instance cache yang sedang berjalan.
     * Tulis dan simpan nilai baru ke `config.toml` pada section yang bersangkutan (`[windows]`, `[[gamedisk]]`, atau `[writeback]`).

---

### Tahap 2: Backend Bugfix — Super Client Commit & State Sync

1. **Perbaikan Race Condition Status Merge (`src/vhd_merge.rs`):**
   - Tambahkan fungsi `start_merge_job(image_key: &str)` yang segera men-set `IS_MERGING.store(true, Ordering::SeqCst)`, `MERGE_PROGRESS = 0`, dan `MERGE_MESSAGE = "Mempersiapkan snapshot & backup metadata..."`.
   - Panggil `start_merge_job` **sebelum** `backup_before_merge` dimulai.

2. **Perbaikan Lifecycle Commit Super Client (`src/api/routes_client.rs`):**
   - Pada `post_superclient_commit`:
     * Panggil `start_merge_job` secara sinkron.
     * Resolusi `super_path` dan `base_path` dengan validasi eksistensi file yang akurat (dukung format nama target iSCSI maupun image key).
     * Di dalam async background task, setelah `merge_vhd` sukses:
       1. Hapus file differencing super VHD.
       2. Bersihkan konfigurasi file: `clear_super_client_config(&config_path)`.
       3. Bersihkan memori server: `cfg_clone.clear_super_client()`.
       4. Set `MERGE_PROGRESS = 100`, `MERGE_MESSAGE = "Commit Super Client selesai."`, dan `IS_MERGING = false`.
     * Jika merge gagal: catat `MERGE_ERROR`, log error, dan biarkan user dapat mencoba lagi tanpa korupsi data.

3. **Status Sanitization (`src/api/routes_vhd.rs`):**
   - Di `get_merge_status`, pastikan nilai `super_client_ip` yang dikembalikan mencerminkan status live terkini setelah `clear_super_client()`.

---

### Tahap 3: Web UI Refactor (`ui/index.html` & `ui/index.js`)

1. **Penyesuaian Tabel iSCSI RAM Cache Monitor (`#tab-cache`):**
   - Tampilkan badge fungsional yang tegas:
     * `💿 Image Disk (Read Cache)`
     * `💽 GameDisk (Read Cache)`
     * `💾 Writeback (Write Cache)`
   - Untuk baris Writeback:
     * Alokasi RAM: Menampilkan RAM Write Buffer (MB/GB).
     * Metrik I/O: Menampilkan statistik penulisan sektor (Bytes Written & Flush Status).
   - Tampilan jika 0 MB:
     * Badge `⚪ Nonaktif / Direct Disk I/O`, progress bar abu-abu 0%, alokasi 0 MB.
   - Tombol "Atur":
     * Membuka modal dengan pilihan preset cepat (512 MB, 1 GB, 2 GB, 4 GB, 8 GB, 16 GB, atau 0 MB untuk Matikan).

2. **Implementasi Dedicated Merge Modal & Polling di Frontend (`ui/index.html` & `ui/index.js`):**
   - **Template Modal Baru (`#modal-superclient-merge` di `ui/index.html`):**
     * Modal dialog terpusat dengan backdrop blur pelindung.
     * Header dengan ikon status (⚡ animasi saat memproses, ✅ hijau saat 100% sukses, ❌ merah jika gagal).
     * Progress bar responsif dengan persentase besar dan counter blok (`current_block` / `total_blocks`).
     * Area teks informatif: menampilkan tahapan pekerjaan dan instruksi aman (jangan mematikan server/komputer).
     * Tombol aksi: Disembunyikan saat sedang proses; setelah 100% selesai, tombol "Selesai & Tutup" muncul aktif.
   - **Alur Polling & Sanitasi State (`startMergeProgressPolling()` di `ui/index.js`):**
     * Tampilkan modal `#modal-superclient-merge` seketika saat tombol Commit ditekan.
     * Pastikan polling tidak keluar secara prematur dengan validasi flag siklus merge aktif.
     * Update progress bar, teks blok data, dan status persentase setiap 500–750ms.
     * Ketika `data.is_merging === false` dan tidak ada error:
       1. Set progress modal ke 100% dengan status sukses ✅ *"Commit Super Client Berhasil!"*.
       2. Muat ulang konfigurasi `loadConfigJson()` dan `loadClientsJson()`.
       3. Re-render `renderClientsManagerTable()` dan `renderDashboardClientsTable()` seketika.
       4. Munculkan tombol "Selesai & Tutup" di dalam modal.

3. **Tailwind CSS Compilation:**
   - Jalankan `npm run build:css` untuk mengompilasi seluruh kelas badge dan styling baru.

---

### Tahap 4: Verifikasi & Testing

1. **Verifikasi Cache Assigned Disks:**
   - Cek API `/api/cache/stats`: pastikan tidak ada lagi nama file `.vhd`. Hanya ada Disk Image, GameDisk, dan Writeback.
   - Verifikasi default disk baru adalah 0 MB (Nonaktif).
   - Uji ubah kapasitas via tombol "Atur" (misal 2048 MB pada Image Disk): pastikan hot-apply instan dan tersimpan di `config.toml`.
2. **Verifikasi Super Client Commit:**
   - Aktifkan super client pada salah satu klien -> pastikan status `⚡ Super` muncul.
   - Lakukan Commit Super Client -> amati progress widget merge VHD berjalan di background.
   - Setelah progress 100%, pastikan status super client otomatis dinonaktifkan (`config.toml` bersih, badge `⚡ Super` hilang seketika di tabel).
3. **Verifikasi Tampilan Responsif:**
   - Uji di browser Playwright pada breakpoint `lg`, `xl`, dan `2xl`: pastikan tidak ada horizontal/vertical scrollbar pada jendela.

---

## 3. Checklist Eksekusi Bertahap

- [ ] **Langkah 1:** Tambahkan `ram_cache_mb` di `WritebackConfig` (`src/config.rs`) & nonaktifkan fallback otomatis ke `read_cache_gb`.
- [ ] **Langkah 2:** Rombak `routes_cache.rs` agar murni menampilkan Assigned Disks (Image, GameDisk, Writeback) dan hapus penjejakan file `.vhd`.
- [ ] **Langkah 3:** Implementasikan live hot-apply dan penyimpanan konfigurasi per-disk di `handle_post_cache_tune`.
- [ ] **Langkah 4:** Perbaiki bug sinkronisasi status Super Client di `src/vhd_merge.rs` dan `src/api/routes_client.rs` (eliminasi race condition `is_merging`).
- [ ] **Langkah 5:** Buat modal dialog progress merge Super Client terpusat (`#modal-superclient-merge` 0-100% + state sukses) dan perbarui UI cache monitor untuk Read Cache vs Write Cache.
- [ ] **Langkah 6:** Jalankan `npm run build:css` dan uji fungsionalitas menyeluruh via Playwright.
