# Implementation Plan - Perbaikan Logika VHD Snapshot & Revert (Append-Only CoW Rollback)

Dokumen ini menganalisis akar masalah mengapa file baru (misal File A) masih ada setelah melakukan Revert Snapshot pada master VHD, serta menyusun rencana perbaikan arsitektur merge dan snapshot restore.

---

## 1. Analisis Akar Masalah (Root Cause Analysis)

### Mengapa File A Masih Ada Setelah Revert Snapshot?
Alur kerja yang terjadi saat ini:
1. **Super Client Update:**
   - Klien dinyalakan dalam mode Super Client.
   - Perubahan data (penambahan File A) ditulis ke dalam file differencing `super.vhd`.
   - Di dalam sistem berkas Windows (NTFS), pembuatan File A memodifikasi:
     - Struktur direktori (misal `C:\` atau `C:\Users\...`).
     - Master File Table (`$MFT`).
     - Cluster bitmap (`$Bitmap`).
     - Sektor isi File A itu sendiri.
   - Sebagian besar struktur metadata NTFS ini **sudah teralokasi di dalam parent/base VHD** (`parent.bat[block_idx] != 0xFFFFFFFF`).

2. **Commit Super Client (`merge_vhd_sync` di `src/vhd_merge.rs`):**
   - Sebelum merge, `backup_before_merge` mencatat snapshot:
     - `backup1.meta`: menyimpan `eof` (ukuran file parent sebelum merge), `table_offset`, dan array `bat` asli.
   - Pada saat proses merge:
     ```rust
     let parent_bat_entry = parent.bat.get(block_idx).copied().unwrap_or(0xFFFFFFFF);
     if parent_bat_entry != 0xFFFFFFFF {
         // In-place overwrite block yang sudah ada di parent
         let parent_offset = (parent_bat_entry as u64) * 512;
         parent.file.seek(SeekFrom::Start(parent_offset))?;
         parent.file.write_all(&bitmap_buf)?;
         parent.file.write_all(&data_buf)?;
     }
     ```
   - **Titik Fatal:** Blok yang sudah ada di parent **ditimpa secara langsung di tempat (*in-place overwrite*)** pada offset lamanya (`parent_offset < eof`).
   - Data lama dari blok-blok tersebut **tertimpa dan musnah** dari base image VHD.

3. **Revert Snapshot (`restore_from_meta` di `src/vhd_merge.rs`):**
   - Fungsi restore saat ini melakukan:
     ```rust
     base_file.set_len(eof)?; // Truncate ke ukuran awal
     base_file.seek(SeekFrom::Start(table_offset))?;
     for &entry in &bat { base_file.write_all(&entry.to_be_bytes())?; }
     ```
   - **Mengapa Revert Gagal:**
     - Offset blok yang ditimpa in-place berada di posisi `< eof`, sehingga perintah `set_len(eof)` sama sekali tidak memotong blok tersebut!
     - Nilai entry di tabel `bat` untuk blok tersebut tidak berubah (sebelum merge bernilai `X`, setelah merge bernilai `X`, saat restore ditulis kembali `X`).
     - Namun **isi data di dalam blok offset X tersebut sudah terlanjur berisi data baru File A**!
     - Akibatnya, saat Windows boot setelah revert, filesystem NTFS tetap membaca MFT dan direktori yang memuat File A. File A tetap muncul dan tidak hilang!

---

## 2. Perbandingan Pilihan Solusi Arsitektur

### Opsi 1: Append-Only CoW Merge (Rekomendasi Utama — Selaras dengan Format Snapshot Saat Ini)
* **Mekanisme:**
  - Setiap kali `merge_vhd_sync` menggabungkan blok dari differencing VHD, **JANGAN PERNAH menimpa blok lama parent secara in-place**.
  - Seluruh blok yang dimodifikasi selalu dialokasikan dan ditulis di posisi baru di akhir file (`next_write_pos` mulai dari `parent_file_size - 512`).
  - Pointer BAT parent diperbarui: `parent.bat[block_idx] = new_bat_entry`.
  - Seluruh blok asli di rentang `0..original_eof` tetap **100% Read-Only dan Utuh**.
* **Mekanisme Restore:**
  - `set_len(original_eof)` seketika memotong seluruh blok baru yang di-append saat merge.
  - Menulis ulang `original_bat` mengembalikan semua pointer blok ke posisi blok asli di `0..original_eof`.
  - Menulis ulang footer di `original_eof - 512`.
* **Kelebihan:**
  - Waktu Revert instan (0.1 detik) tanpa pemindahan data berat.
  - Ukuran file snapshot sangat kecil (hanya beberapa KB untuk `.meta`).
  - Revert secara otomatis menciutkan (*shrink*) ukuran file VHD kembali ke ukuran sebelum merge.
  - File A dan semua perubahan 100% hilang tanpa sisa.
* **Trade-off:**
  - Ukuran master VHD bertambah sesuai jumlah data perubahan yang di-commit (perilaku standar Microsoft VHD dynamic disk). Jika ingin mengecilkan VHD setelah banyak merge permanen, admin dapat menjalankan fitur compact VHD.

---

### Opsi 2: Pre-Image Undo Journal (In-Place Overwrite + Delta Backup)
* **Mekanisme:**
  - Jika tetap ingin in-place overwrite, sebelum menimpa blok lama di `parent_offset`, baca 2 MB blok lama dari parent dan tulis ke file jurnal `_backupX.undo`.
  - Saat restore, baca `_backupX.undo` dan tulis balik setiap blok ke offset aslinya di parent.
* **Kelebihan:**
  - Ukuran master VHD tidak bertambah untuk blok yang ditimpa.
* **Trade-off:**
  - I/O merge menjadi 2x lipat lebih lambat (harus baca blok lama parent lalu tulis ke file undo sebelum menulis blok baru).
  - File backup snapshot menjadi sangat besar (bisa beberapa GB).
  - Waktu restore lambat karena harus menyalin ulang seluruh blok lama.

---

## 3. Rencana Aksi Implementasi (Solusi Opsi 1 - Append-Only CoW)

### Tahap 1: Modifikasi `merge_vhd_sync` di `src/vhd_merge.rs`
1. Hapus percabangan `parent_bat_entry != 0xFFFFFFFF` yang melakukan in-place overwrite.
2. Setiap blok dari child VHD (`child_bat_entry != 0xFFFFFFFF`) selalu ditulis ke `next_write_pos` (append di akhir file).
3. Perbarui `parent.bat[block_idx] = (next_write_pos / 512) as u32`.
4. Tambahkan `next_write_pos += bitmap_size + vhd_block_size`.
5. Tulis ulang BAT parent ke `table_offset` dinamis.
6. Tulis ulang footer di akhir file baru (`next_write_pos`) dan potong file tepat dengan `set_len(next_write_pos + 512)`.
7. Sinkronkan footer copy di offset 0.

### Tahap 2: Validasi Logika `restore_from_meta` di `src/vhd_merge.rs`
1. Pastikan `restore_from_meta` membaca `eof`, `table_offset`, dan array `bat` dari `.meta`.
2. Lakukan `base_file.set_len(eof)` untuk memotong seluruh blok yang di-append saat merge.
3. Tulis ulang seluruh isi `bat` lama ke `table_offset`.
4. Tulis ulang footer valid di `eof - 512` dan perbarui header copy di offset 0.
5. Panggil `base_file.sync_all()`.

### Tahap 3: Pembersihan Snapshot Berantai (`cleanup_backup_files`)
1. Ketika user me-restore ke Snapshot #N, bersihkan file `.meta` dan `.vhd` untuk Snapshot #N dan index di atasnya (karena state di atasnya sudah di-rollback).
2. Bersihkan file `super.vhd` dan matikan mode Super Client di `config.toml` dan `SharedConfig`.

### Tahap 4: Unit Test & Pengujian End-to-End
1. Buat unit test mandiri di Rust yang:
   - Membuat mock VHD parent dengan beberapa blok data awal.
   - Membuat mock differencing VHD dengan modifikasi pada blok yang sudah ada dan blok baru.
   - Melakukan `backup_before_merge`.
   - Melakukan `merge_vhd_sync`.
   - Memvalidasi bahwa parent VHD memuat data baru.
   - Melakukan `restore_backup_by_index`.
   - Memvalidasi bahwa parent VHD kembali persis ke isi data sebelum merge (data baru lenyap 100%, ukuran file kembali ke awal).
2. Verifikasi dengan `cargo test` dan `cargo check`.

---

## 4. Kriteria Keberhasilan (Definition of Done)
1. **Determinisme Rollback:** Ketika mode Super Client digunakan untuk menambah File A lalu di-commit, kemudian admin melakukan Revert Snapshot, File A dipastikan hilang 100% dan filesystem kembali persis ke kondisi sebelum Super Client di-enable.
2. **Kecepatan Revert:** Proses revert berjalan instan (di bawah 1 detik) tanpa membebani disk I/O server.
3. **Integritas VHD:** VHD hasil revert tetap memiliki footer valid, header valid, dan checksum standar Microsoft yang lolos validasi Windows Disk Management / Hyper-V tanpa error korupsi.
