# Plan: Decouple GameDisk Cache Input & Murni ImageDisk Role Assignment

**Tanggal:** 2026-10-23  
**Status:** Draf Disetujui, Siap Eksekusi  
**Tujuan:**
1. Menghapus input alokasi RAM cache dari modal Disk Management saat memilih GameDisk (alokasi cache 100% dipusatkan di tab iSCSI Cache).
2. Menghilangkan keterikatan folder kaku `\vhd` pada role Boot/ImageDisk di Disk Management: drive yang dipilih di-assign langsung sebagai ImageDisk storage (misal `I:\`), sehingga muncul di tab iSCSI Cache untuk diatur alokasi RAM cachenya.
3. Menjaga alokasi cache yang sudah ada sebelumnya jika role di-update (default 0 untuk disk baru).

---

## 1. Analisis & Scope Perubahan

### A. Frontend HTML (`ui/index.html`)
* **File:** `ui/index.html`
  - Hapus blok `#partition-gamedisk-cache-box` di dalam modal `#partition-modal` (garis ~889-906).
  - Modal role assignment di Disk Management murni hanya menampilkan pilihan radio:
    - 💿 Image Disk (Base OS)
    - 💽 GameDisk (Storage Game)
    - 💾 Writeback (Storage Klien)
    - ⚪ Lepas Alokasi (None)
  - Label role pertama diselaraskan menjadi: **Image Disk (Base OS Image)**.

### B. Frontend JavaScript (`ui/index.js`)
* **File:** `ui/index.js`
  - **`openPartitionModal()`**:
    - Bersihkan kode yang mengatur `display` dan `value` untuk `partition-gamedisk-cache-box` dan `partition-gamedisk-ram-cache`.
  - **`savePartitionRoleAction()`**:
    - Untuk `role === 'boot'`: Simpan `vhd_dir = `${letter}:\\`` (root drive storage Image Disk, tanpa embel-embel subfolder kaku `\vhd`).
    - Untuk `role === 'gamedisk'`:
      - Cek apakah disk ini sebelumnya sudah punya `ram_cache_mb` di `configObj.gamedisk`. Jika ada, pertahankan nilainya.
      - Jika baru, set `ram_cache_mb: 0`.
      - Tidak ada pembacaan input cache di modal.
  - **`renderDiskGrid()`**:
    - Perbarui deteksi role `boot`: jika `vhd_dir` berawalan huruf drive tersebut (baik `I:\` maupun `I:\vhd`), tandai sebagai role `boot` (Image Disk).

### C. Backend Cache & Display Sync (`src/api/routes_cache.rs`)
* **File:** `src/api/routes_cache.rs`
  - Di `handle_get_cache_stats()`:
    - Normalisasi pembacaan drive letter dari `win.vhd_dir` (misal jika bernilai `I:\`, `I:`, atau `I:\vhd`).
    - Nama display di tabel: `Image Disk (Drive <Letter>:\)` atau `Boot Disk (Drive <Letter>:\)`.

### D. Build & Verifikasi
1. Jalankan `npm run build:css` untuk rebuild Tailwind CSS minified.
2. Jalankan `cargo check` dan `cargo test` untuk memastikan tidak ada regresi backend.
3. Verifikasi file `config.toml` tetap utuh dan tidak terganggu.

---

## 2. Tahapan Eksekusi

1. **Task 1: Hapus Cache Box di Modal Disk Management (`ui/index.html`)**
2. **Task 2: Update Logika Role Assignment di Frontend (`ui/index.js`)**
3. **Task 3: Rebuild Tailwind CSS (`npm run build:css`)**
4. **Task 4: Verifikasi Backend & Testing (`cargo check` & `cargo test`)**
