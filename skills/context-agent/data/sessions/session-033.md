# Sessão 033 — 2026-06-16
**Slug:** tmbilling-rupiah | **Branch:** feat/refactor-landingpage | **Duração:** ~90min

## Tópicos
- Standarisasi format rupiah nasional `Rp[nominal]` (tanpa spasi, sesuai EYD/PUEBI)
- Perbaikan bug delete menu: NOT NULL constraint + duplikat nama setelah soft-delete
- Filter laporan billing & kantin responsif mobile
- Checkbox Stok Unlimited pada modal Menu
- Input harga auto-format real-time (Menu & Paket)
- Commit & push ke remote `feat/refactor-landingpage`

## Decisões
- Format rupiah: `new Intl.NumberFormat('id-ID', { style: 'decimal', maximumFractionDigits: 0 }).format(value)` → `1.500` (titik, tanpa desimal); prefix `Rp` tanpa spasi
- Untuk input form: harga dikirim ke backend sebagai angka mentah (titik di-strip sebelum API call)
- Stok Unlimited: checkbox menyembunyikan input stok angka, saat submit kirim `null` atau `-1` ke backend
- Tipe pemesanan default: "Makan di Tempat" (bukan Take Away)
- Soft-delete menu: tambah kolom `is_active`; restore otomatis saat create nama sama; endpoint hard-delete permanen tersedia
- `Utils.formatRupiah(value)` → `Rp[nominal]`; `Utils.formatRawRupiah(value)` → `[nominal]` (tanpa prefix, untuk print struk); `Utils.formatInputRupiah(el)` → format on-the-fly sambil mengetik
- `helpers.py format_rupiah()` → ganti koma ke titik (sesuai EYD/PUEBI)

## Tarefas Concluídas
- [x] Tambah `formatRupiah`, `formatInputRupiah`, `formatRawRupiah` di `app/static/js/kasir/core/utils.js`
- [x] Update `app/utils/helpers.py` `format_rupiah()` → titik
- [x] Filter laporan billing & kantin responsif (`flex-wrap`, `w-full sm:w-auto`)
- [x] Checkbox Stok Unlimited di modal Tambah/Edit Menu
- [x] Input harga Menu: `type="text" inputmode="numeric"`, placeholder `Rp0`
- [x] Input harga Paket: `type="text" inputmode="numeric"`, auto-format oninput
- [x] Tipe default "Makan di Tempat" di modal Menu
- [x] Ganti seluruh `Intl.NumberFormat` / `toLocaleString` ke `Utils.formatRupiah` di 14+ JS file
- [x] Perbaikan `struk_preview.js` (preview + print): pakai `Utils.formatRupiah`
- [x] Jinja `member/dashboard.html` & `paket/index.html`: `{:,}` → `{:,}.replace(',', '.')`
- [x] Perbaikan bug delete menu: tambah `is_active`, soft-delete, restore otomatis, hard-delete endpoint
- [x] Commit `25a9a0f` & push ke `origin/feat/refactor-landingpage`

## Tarefas Pendentes
- [ ] (tidak ada — sesi ini tuntas)

## File yang Diubah (27 file)
**Backend:**
- `app/utils/helpers.py` — format rupiah titik
- `app/models/menu/menu.py` — tambah `is_active`
- `app/repositories/menu/menu_repository.py`
- `app/routes/menu/menu_routes.py` — restore otomatis + hard-delete
- `app/services/menu/menu_service.py`
- `app/services/report/report_service.py`
- `app/services/settings/settings_service.py`
- `app/static/js/kasir/core/api.js`
- `migrations/versions/378524089e68_add_is_active_to_menu_item.py` (baru)

**Frontend JS:**
- `app/static/js/kasir/core/utils.js` — `formatRupiah`/`formatInputRupiah`/`formatRawRupiah`
- `app/static/js/kasir/modules/menu/index.js` — toggleStokUnlimited, submitForm, modal
- `app/static/js/kasir/modules/paket/paket_modal.js` — input harga auto-format
- `app/static/js/kasir/modules/paket/index.js` — strip dots sebelum API
- `app/static/js/kasir/modules/shift/index.js` — Utils.formatRupiah
- `app/static/js/kasir/modules/dashboard/index.js` — statIncome
- `app/static/js/kasir/modules/struk/struk_preview.js` — renderPreview + printPreview
- `app/static/js/kasir/components/modal-buka.js`
- `app/static/js/kasir/modules/settings/index.js`

**Frontend HTML:**
- `app/templates/kasir/tabs/menu.html` — checkbox unlimited
- `app/templates/kasir/tabs/laporan.html` — filter responsif
- `app/templates/kasir/tabs/laporan_menu.html` — filter responsif
- `app/templates/kasir/tabs/settings.html`
- `app/templates/public/member/dashboard.html` — Jinja `replace(',', '.')`
- `app/templates/public/paket/index.html` — Jinja `replace(',', '.')`

**Asset:**
- `app/static/uploads/menu/128_1695564785.png` (baru)
- `app/static/uploads/menu/DOC-20250811-WA0004._2662052569.png` (baru)

## Commit
```
25a9a0f perbaikan: standarisasi format rupiah nasional & perbaikan bug delete menu
 27 files changed, 612 insertions(+), 156 deletions(-)
 push → origin/feat/refactor-landingpage
```

## Konvensi / Preferensi
- Commit message: bahasa Indonesia (per taste.md)
- Format rupiah: `Rp[nominal]` EYD/PUEBI (per taste.md)
- Label "Pelanggan POS" untuk Kantin/menu transaksi (per taste.md)
- Verifikasi syntax: `node --check` untuk .js, `python -m py_compile` untuk .py (per taste.md)
- Venv Python: `.venv\scripts\python.exe` (per taste.md)
- TMBilling project: load `/context-agent` context dulu sebelum kerja
