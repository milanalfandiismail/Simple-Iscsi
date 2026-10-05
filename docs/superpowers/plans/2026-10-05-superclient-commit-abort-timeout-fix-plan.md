# Implementation Plan - Perbaikan HTTP AbortError pada Commit Super Client

Dokumen ini menganalisis akar masalah timbulnya error `POST /api/superclient/commit failed: AbortError: signal is aborted without reason` pada browser saat commit Super Client, serta menyusun rencana perbaikannya.

---

## 1. Analisis Akar Masalah (Root Cause Analysis)

### Mengapa Terjadi AbortError tetapi File Backup Sudah Ada di Disk?

Alur kejadian:
1. **Frontend Fetch Timeout (4.5 Detik):**
   - Pada [`ui/index.js:154`](file:///c:/Project%20GIT/Simple-Iscsi/ui/index.js#L154), fungsi pembungkus HTTP `apiPost` memiliki timeout default:
     ```javascript
     async function apiPost(url, body = {}, timeoutMs = 4500) {
         const controller = new AbortController();
         const timeoutId = setTimeout(() => controller.abort(), timeoutMs);
     ```
   - Jika response HTTP dari server belum selesai dalam 4.5 detik, browser otomatis membatalkan (*abort*) request dengan pesan `AbortError: signal is aborted without reason`.

2. **Synchronous Disk Copying di Thread Handler HTTP Backend:**
   - Pada [`src/api/routes_client.rs:272`](file:///c:/Project%20GIT/Simple-Iscsi/src/api/routes_client.rs#L272), saat request `POST /api/superclient/commit` diterima:
     ```rust
     if crate::writeback_super::super_exists(&super_path) {
         let _ = crate::vhd_merge::backup_before_merge(&base_path, &super_path); // <-- SYNCHRONOUS BLOCKING I/O
         ...
         tokio::spawn(async move {
             match crate::vhd_merge::merge_vhd(...)
         });
     ```
   - Handler memanggil `backup_before_merge` secara sinkronus **sebelum** mengirimkan HTTP response.
   - Di dalam `backup_before_merge` ([`src/vhd_merge.rs`](file:///c:/Project%20GIT/Simple-Iscsi/src/vhd_merge.rs)):
     ```rust
     if Path::new(super_path).exists() {
         std::fs::copy(super_path, &backup_vhd)?; // <-- Mengkopi file super.vhd (1 - 5 GB)
     }
     ```
   - Perintah `std::fs::copy` menyalin seluruh isi file differencing `super.vhd` (berukuran 1 hingga beberapa Gigabyte).
   - Pada disk fisik, penyalinan file sebesar ini membutuhkan waktu 5–15 detik.

3. **Titik Tabrakan (Race Condition / Timeout):**
   - Pada detik ke **4.5**, timer browser habis -> browser memunculkan error `AbortError`.
   - Namun di sisi server, operasi `std::fs::copy` tetap terus berjalan di kernel hingga selesai (itulah sebabnya file `_backup1.vhd` dan `_backup1.meta` **sudah terbentuk di disk**).
   - Server kemudian mengirimkan response ke socket yang sayangnya sudah ditutup oleh browser.

---

## 2. Solusi Arsitektur

Dengan arsitektur **Append-Only Copy-on-Write (CoW)**:
1. **Penghapusan Redundant Multi-GB Disk Copying:**
   - Proses Revert Snapshot murni hanya membutuhkan file metadata `.meta` (`eof` dan `bat`) untuk memotong file (*truncate*) dan memulihkan BAT.
   - Blok lama di `0..original_eof` pada Master VHD tetap read-only dan tidak pernah disentuh.
   - File `_backupN.vhd` (hasil copy `super.vhd`) **sama sekali tidak pernah dibaca atau digunakan saat restore**.
   - Menghapus `std::fs::copy` membuat pembuatan snapshot selesai dalam **< 1 milidetik** (hanya menulis file kecil ~10 KB).

2. **Asinkronus Background Execution:**
   - Memindahkan pemanggilan `backup_before_merge` ke dalam task background `tokio::spawn` / `spawn_blocking` tepat sebelum `merge_vhd`.
   - Endpoint `POST /api/superclient/commit` langsung mengembalikan `200 OK` seketika (**0.001 detik**).
   - Frontend langsung memunculkan widget floating progress bar dan melakukan live polling status merge.

3. **Peningkatan Timeout Frontend:**
   - Pada `ui/index.js`, berikan timeout yang lebih longgar (misal `15000ms`) pada pemanggilan commit.

---

## 3. Rencana Aksi (Implementation Tasks)

- [ ] **Task 1: Optimalisasi `backup_before_merge` di [`src/vhd_merge.rs`](file:///c:/Project%20GIT/Simple-Iscsi/src/vhd_merge.rs)**
  - Hapus operasi `std::fs::copy(super_path, &backup_vhd)`.
  - Pastikan pembuatan `.meta` berjalan instan (<1ms) dan hemat disk space.

- [ ] **Task 2: Jadikan Commit Handler Non-Blocking di [`src/api/routes_client.rs`](file:///c:/Project%20GIT/Simple-Iscsi/src/api/routes_client.rs)**
  - Pindahkan `backup_before_merge` ke dalam task background `tokio::spawn` bersama `merge_vhd`.
  - Pastikan endpoint `post_superclient_commit` langsung mengembalikan response JSON `200 OK`.

- [ ] **Task 3: Penyesuaian Timeout di [`ui/index.js`](file:///c:/Project%20GIT/Simple-Iscsi/ui/index.js)**
  - Berikan parameter timeout `15000` pada `apiPost('/api/superclient/commit', ..., 15000)`.

- [ ] **Task 4: Pengujian & Verifikasi**
  - Jalankan `cargo test` untuk memvalidasi seluruh unit tests.
  - Jalankan `cargo check` untuk memvalidasi integritas kompilasi.

- [ ] **Task 5: Living Memory Update di [`ANTIGRAVITY.md`](file:///c:/Project%20GIT/Simple-Iscsi/ANTIGRAVITY.md)**
  - Catat Playbook Debugging Kasus #16 mengenai penanganan timeout Commit Super Client.
