# Plan: Assigned Disk Cache & Eksklusi Mutlak Drive C:

**Tanggal:** 2026-10-23  
**Status:** Draf Siap Konfirmasi  
**Tujuan:**
1. Menghilangkan total Drive C: dari Disk Management dan iSCSI Cache (backend & frontend).
2. Memastikan iSCSI Cache hanya berlaku untuk disk yang memiliki assignment aktif di Disk Management (Boot Disk, GameDisk, Writeback).
3. Hanya file Parent Base VHD yang di-cache untuk Boot Disk; child/differencing VHD tidak di-cache.
4. Menetapkan default RAM cache iSCSI menjadi 0 MB (OFF secara default) kecuali diset manual oleh admin.

---

## 1. Analisis & Scope Perubahan

### A. Backend - Eksklusi Mutlak Drive C:
* **File:** `src/api/routes_disk.rs`
  - Di fungsi `get_logical_drives_detail()`:
    - Modifikasi loop `for c in b'A'..=b'Z'`: lewati (`continue`) jika `c == b'C'`.
    - Drive C: tidak akan pernah dikembalikan ke pemanggil API `/api/system/logical_drives`.
* **File:** `src/api/routes_cache.rs`
  - Di fungsi `handle_get_cache_stats()`:
    - Lewati disk atau direktori yang berawalan `C:` atau `\\.\C:`.
  - Di fungsi `handle_post_cache_config()`:
    - Tambahkan validasi proteksi: jika target disk atau path merujuk ke drive `C:` atau `\\.\C:`, kembalikan HTTP 400 Bad Request (`"Drive C: adalah drive sistem host dan dilarang untuk iSCSI"`).

### B. Backend - Cache Berbasis Assigned Disk & Parent VHD Only
* **File:** `src/api/routes_cache.rs`
  - Boot Storage Disk:
    - Hanya sertakan entri `Boot Disk (Drive <Letter>:\)` jika `win.vhd_dir` tidak kosong dan bukan pada drive C:.
    - Jika belum ada disk yang di-assign sebagai `boot`, entri tidak dimunculkan di tabel cache.
    - Default `ram_cache_mb` di struktur inisialisasi adalah `0`.
  - GameDisk:
    - Hanya sertakan entri untuk GameDisk yang terkonfigurasi dan bukan pada drive C:.
    - Default `ram_cache_mb` default baru adalah `0`.
  - Writeback Storage:
    - Hanya sertakan jika `writeback_dirs` tidak kosong dan bukan pada drive C:.
    - Default `ram_cache_mb` adalah `0`.
  - Unassigned Disks:
    - Disk dengan status unassigned tidak pernah dimasukkan ke dalam daftar `disks` di `CacheSummaryResponse`.
* **File:** `src/writeback_imagedisk.rs`
  - Pastikan `get_or_create_image_cache` hanya dipanggil untuk Parent/Base VHD file.
  - Differencing disk / per-client COW VHD tidak membuat atau mengaitkan diri ke `ReadAheadCache` (hanya passthrough I/O).

### C. Backend - Konfigurasi Default Cache 0 MB
* **File:** `src/config.rs`
  - `WindowsConfig::default()`: ubah `ram_cache_mb` default menjadi `Some(0)` atau `None`.
  - `WritebackConfig::default()`: ubah `ram_cache_mb` default menjadi `Some(0)` atau `None`.
  - `GameDiskConfig`: default `ram_cache_mb` menjadi `Some(0)` atau `None`.

### D. Frontend - UI Disk Management & Cache Table
* **File:** `ui/index.js`
  - **Disk Management:**
    - Filter eksklusi: pastikan jika ada data dengan letter `'C'`, langsung di-skip (tidak dirender).
    - Hapus seluruh fallback string `C:\vhd` dan `C:\writeback` di inisialisasi `configObj.windows` dan `configObj.writeback`. Gunakan string kosong `""` atau `[]`.
    - Di modal assignment (`openPartitionModal`), tambahkan proteksi agar letter `C` ditolak jika dipanggil.
    - Default RAM cache gamedisk di input modal diubah dari `2048` menjadi `0`.
  - **iSCSI RAM Cache Monitor:**
    - Tabel cache hanya merender disk yang memiliki assignment aktif.
    - Label Boot Disk ditampilkan dengan jelas: `Boot Disk (Drive <Letter>:\)`.
    - Modal atur cache (`modal-edit-cache`): preset default awal adalah 0 MB.

---

## 2. Rencana Tahapan Eksekusi

1. **Task 1: Backend Disk Filtering (routes_disk.rs)**
   - Lewati `b'C'` di `get_logical_drives_detail()`.
2. **Task 2: Backend Cache Logic & Proteksi C: (routes_cache.rs)**
   - Filter ketat `C:` di `handle_get_cache_stats` dan `handle_post_cache_config`.
   - Hanya masukkan disk yang ter-assign (Boot disk jika ada `vhd_dir`, Gamedisk yang terdaftar, Writeback jika ada `writeback_dirs`).
3. **Task 3: Backend Config Defaults & Parent-only Cache (config.rs & writeback_imagedisk.rs)**
   - Set nilai default `ram_cache_mb` menjadi 0.
   - Verifikasi isolasi cache parent VHD.
4. **Task 4: Frontend UI Cleanup (ui/index.js)**
   - Hapus fallback `C:\` di script frontend.
   - Perbarui perenderan disk assignment dan tabel cache agar sinkron dengan assigned disks.
5. **Task 5: Build Asset UI**
   - Jalankan kompilasi CSS Tailwind: `npm run build:css` untuk menghasilkan file `./ui/tailwind.css` minified terbaru.
6. **Task 6: Verifikasi & Testing**
   - Kompilasi backend: `cargo check` dan `cargo test`.
   - Verifikasi output JSON dari `/api/system/logical_drives` dan `/api/cache/stats` via script sandbox.
   - Pastikan drive C: steril dan tabel cache hanya menampilkan assigned disks.
