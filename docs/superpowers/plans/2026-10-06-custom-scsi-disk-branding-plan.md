# Custom SCSI Disk Identity & Vendor Branding Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Memungkinkan operator mengubah identitas disk SCSI (Vendor ID, Product ID, dan Revision Level) untuk Windows Boot VHD dan GameDisk secara bebas dan kreatif (misal: "SAMSUNG 980 PRO", "WD_BLACK SN850X", "GENESIS DISKLESS") dengan live preview Device Manager Windows pada dashboard Disk Management, backend Rust SCSI Inquiry SPC-4 compliance, dan tata letak responsif `sm`, `md`, `lg`, `xl`, `2xl`.

**Architecture:**
1. **SCSI Inquiry Protocol Layer (`src/scsi_gamedisk.rs`, `src/backend.rs`):** Respons Inquiry Standar SCSI SPC-4 (Bytes 8..15 Vendor ID [8-byte ASCII space-padded], Bytes 16..31 Product ID [16-byte ASCII space-padded], Bytes 32..35 Product Revision [4-byte ASCII space-padded]) diuji dan diamankan agar menerima string kustom tanpa buffer overflow/panics.
2. **Configuration & Lifecycle Management (`src/config.rs`, `src/config_manager.rs`, `src/server_api.rs`):** Menambahkan serde defaults yang tangguh untuk `vendor_id`, `product_id`, dan `product_revision` pada `WindowsConfig` dan `GamediskConfig`. Memastikan reload handler di `config_manager.rs` mendeteksi pergantian vendor/product sehingga instance backend di-refresh tanpa me-reuse backend lama yang usang.
3. **Frontend UI & Interactive Live Preview (`ui/index.html`, `ui/index.js`, `ui/input.css`):** Menambahkan kartu "Identitas & Vendor SCSI Disk" di halaman Disk Management (`#tab-disk-mgmt`) berdampingan dengan "Global Storage Parameters" dalam grid 2-kolom seimbang (`lg`, `xl`, `2xl`), dilengkapi simulasi live preview Device Manager Windows secara realtime dan preset vendor populer (Samsung, WD Black, Kingston, Genesis, Default).

**Tech Stack:**
- Backend: Rust 2021, Tokio, Serde TOML/JSON, SCSI SPC-4 Standard
- Frontend: Vanilla HTML5, Vanilla JavaScript (ES6+), Tailwind CSS v4 (Local Build CLI)

**Spec:** Living memory [`ANTIGRAVITY.md`](file:///c:/Project%20GIT/Simple-Iscsi/ANTIGRAVITY.md) & [`DOCUMENTATION.md`](file:///c:/Project%20GIT/Simple-Iscsi/DOCUMENTATION.md)

## Global Constraints
- Jangan lakukan commit atau push Git sebelum ada perintah eksplisit dari pengguna.
- Wajib menggunakan Tailwind CSS lokal via `npm run build:css` (dilarang menggunakan CDN eksternal).
- Tampilan UI harus responsif pada semua breakpoint: `sm`, `md`, `lg`, `xl`, `2xl` dengan tema Shadcn / Zinc.
- Semua unit test Rust (`cargo test`) harus 100% lulus (0 failures).

---

### Task 1: Backend Protocol & Serde Defaults Hardening for Custom SCSI Branding

**Files:**
- Modify: `src/config.rs:100-140`
- Modify: `src/config_manager.rs:135-165`
- Test: `src/scsi_gamedisk.rs:250-271`

**Interfaces:**
- Consumes: `WindowsConfig`, `GamediskConfig`, `Backend::new_raw`, `Backend::new_vhd`
- Produces: Sanitized fallback defaults untuk `vendor_id` (default `"RUSTISCS"`), `product_id` (default `"WindowsBoot"` / `"GameDisk"`), `product_revision` (default `"1.00"`), serta deteksi perubahan identitas disk pada file watcher `config_manager`.

- [ ] **Step 1: Tulis unit test untuk SCSI Inquiry custom vendor & product padding**

Tambahkan pengujian di `src/scsi_gamedisk.rs`:
```rust
#[test]
fn test_inquiry_custom_vendor_and_product_branding() {
    let mut backend = Backend {
        path: "dummy".to_string(),
        block_size: 512,
        total_blocks: 1000,
        vendor_id: "SAMSUNG".to_string(),
        product_id: "980 PRO NVMe".to_string(),
        product_revision: "2.00".to_string(),
        is_raw: true,
        is_vhd: false,
        is_vhd_diff: false,
        raw_file: None,
        vhd_meta: None,
        read_cache: None,
        io_semaphore: std::sync::Arc::new(tokio::sync::Semaphore::new(100)),
    };

    let cdb_std = [0x12, 0x00, 0x00, 0x00, 36, 0x00];
    let res = handle_inquiry(&cdb_std, &backend);
    match res {
        ScsiResult::Data { data, status } => {
            assert_eq!(status, 0x00);
            assert_eq!(data.len(), 36);
            // Vendor bytes 8..16 (8 bytes, padded with spaces)
            assert_eq!(&data[8..16], b"SAMSUNG ");
            // Product bytes 16..32 (16 bytes, padded with spaces)
            assert_eq!(&data[16..32], b"980 PRO NVMe    ");
            // Revision bytes 32..36 (4 bytes)
            assert_eq!(&data[32..36], b"2.00");
        }
        _ => panic!("Expected ScsiResult::Data"),
    }
}
```

- [ ] **Step 2: Jalankan `cargo test` untuk memverifikasi test gagal atau error**

Command: `cargo test -- test_inquiry_custom_vendor_and_product_branding`
Expected: Compile error atau test failure jika field/metode belum sesuai.

- [ ] **Step 3: Perbarui `src/config.rs` dengan serde default helper functions**

Di `src/config.rs`, tambahkan fallback helper functions:
```rust
fn default_vendor_id() -> String {
    "RUSTISCS".to_string()
}
fn default_windows_product_id() -> String {
    "WindowsBoot".to_string()
}
fn default_gamedisk_product_id() -> String {
    "GameDisk".to_string()
}
fn default_product_revision() -> String {
    "1.00".to_string()
}
```
Lalu pasang pada atribut struct:
```rust
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GamediskConfig {
    pub physical_disk: String,
    pub block_size: u64,
    #[serde(default = "default_vendor_id")]
    pub vendor_id: String,
    #[serde(default = "default_gamedisk_product_id")]
    pub product_id: String,
    #[serde(default = "default_product_revision")]
    pub product_revision: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[allow(dead_code)]
pub struct WindowsConfig {
    pub target_iqn_prefix: String,
    pub vhd_dir: String,
    pub block_size: u64,
    #[serde(default = "default_vendor_id")]
    pub vendor_id: String,
    #[serde(default = "default_windows_product_id")]
    pub product_id: String,
    #[serde(default = "default_product_revision")]
    pub product_revision: String,
    pub discovery: bool,
    #[serde(default)]
    pub super_client_ip: String,
    #[serde(default)]
    pub super_client_action: String,
}
```

- [ ] **Step 4: Perbarui deteksi cache backend di `src/config_manager.rs`**

Pastikan pada baris 137–145 `config_manager.rs`, pembandingan disk backend memeriksa `vendor_id`, `product_id`, dan `product_revision`:
```rust
for (old_i, old_gd_cfg) in old_config.gamedisk.iter().enumerate() {
    if old_i as u8 == lun_id 
        && old_gd_cfg.physical_disk == gd_cfg.physical_disk
        && old_gd_cfg.vendor_id == gd_cfg.vendor_id
        && old_gd_cfg.product_id == gd_cfg.product_id
        && old_gd_cfg.product_revision == gd_cfg.product_revision 
    {
        if let Some(b) = backends_map.get(&lun_id) {
            new_map.insert(lun_id, Arc::clone(b));
            reused = true;
            break;
        }
    }
}
```

- [ ] **Step 5: Jalankan `cargo test` untuk memverifikasi semua test backend berhasil**

Command: `cargo test`
Expected: 9 passed, 0 failed.

---

### Task 2: Frontend UI - SCSI Disk Identity & Branding Card in Disk Management

**Files:**
- Modify: `ui/index.html:273-290` (Section `#tab-disk-mgmt`)
- Modify: `ui/index.html:600-640` (`#partition-modal`)

**Interfaces:**
- Produces:
  - Form container grid 2-kolom di `#tab-disk-mgmt`:
    - Kolom 1: Global Storage Parameters Card
    - Kolom 2: Identitas & Vendor SCSI Disk Card
  - Inputs:
    - `#disk-vendor-os` (maxlength 8, placeholder "e.g. SAMSUNG")
    - `#disk-product-os` (maxlength 16, placeholder "e.g. 980 PRO NVMe")
    - `#disk-vendor-game` (maxlength 8, placeholder "e.g. WD_BLACK")
    - `#disk-product-game` (maxlength 16, placeholder "e.g. SN850X GAME")
    - `#disk-revision-common` (maxlength 4, placeholder "1.00")
  - Live Preview Windows Simulation Badges:
    - `#preview-os-disk-text`
    - `#preview-game-disk-text`
  - Preset Quick Buttons: `[Samsung]`, `[WD_Black]`, `[Kingston]`, `[Genesis]`, `[Reset RUSTISCS]`

- [ ] **Step 1: Desain tata letak 2-kolom di `#tab-disk-mgmt` (`ui/index.html`)**

Ubah kontainer storage parameters di `ui/index.html` menjadi grid 2-kolom:
```html
<div class="grid grid-cols-1 lg:grid-cols-2 xl:grid-cols-2 2xl:grid-cols-2 gap-4 mb-5 sm:mb-6">
    <!-- Card 1: Global Storage Parameters -->
    <div class="bg-white dark:bg-zinc-900/70 border border-zinc-200 dark:border-zinc-800 rounded-lg p-4 sm:p-5 flex flex-col justify-between shadow-2xs">
        <div>
            <h2 class="text-sm sm:text-base font-bold text-zinc-900 dark:text-zinc-50 pb-3 border-b border-zinc-100 dark:border-zinc-800 mb-4">Global Storage Parameters</h2>
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-4 mb-5">
                <div>
                    <label class="block text-xs font-medium uppercase tracking-wider text-zinc-500 dark:text-zinc-400 mb-1.5">Max Cache Size Per Client (GB)</label>
                    <input type="number" id="disk-max-cache-gb" required class="w-full px-3 py-2 text-xs sm:text-sm rounded-md border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-950 text-zinc-900 dark:text-zinc-100 focus:outline-none focus:ring-1 focus:ring-zinc-400 dark:focus:ring-zinc-600 transition-colors">
                </div>
                <div>
                    <label class="block text-xs font-medium uppercase tracking-wider text-zinc-500 dark:text-zinc-400 mb-1.5">Max Write Speed Throttle (MB/s)</label>
                    <input type="number" id="disk-throttle-mb" required class="w-full px-3 py-2 text-xs sm:text-sm rounded-md border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-950 text-zinc-900 dark:text-zinc-100 focus:outline-none focus:ring-1 focus:ring-zinc-400 dark:focus:ring-zinc-600 transition-colors">
                </div>
            </div>
        </div>
        <div>
            <button type="button" class="btn-primary px-4 py-2 text-xs font-medium shadow-2xs cursor-pointer" onclick="saveGlobalStorageParams()">💾 Simpan Parameter Storage</button>
        </div>
    </div>

    <!-- Card 2: Identitas & Vendor SCSI Disk -->
    <div class="bg-white dark:bg-zinc-900/70 border border-zinc-200 dark:border-zinc-800 rounded-lg p-4 sm:p-5 flex flex-col justify-between shadow-2xs">
        <div>
            <div class="flex items-center justify-between pb-3 border-b border-zinc-100 dark:border-zinc-800 mb-4">
                <div class="flex items-center gap-2">
                    <span class="text-base">🏷️</span>
                    <h2 class="text-xs sm:text-sm font-bold text-zinc-900 dark:text-zinc-50 m-0">Identitas & Vendor SCSI Disk</h2>
                </div>
                <span class="text-[10.5px] text-zinc-400 dark:text-zinc-500 font-mono">SPC-4 Inquiry</span>
            </div>

            <!-- Preset Buttons -->
            <div class="flex flex-wrap items-center gap-1.5 mb-3.5">
                <span class="text-[11px] text-zinc-500 dark:text-zinc-400 mr-1">Preset:</span>
                <button type="button" class="btn-secondary px-2 py-0.5 text-[11px] font-medium" onclick="applyDiskBrandingPreset('samsung')">Samsung NVMe</button>
                <button type="button" class="btn-secondary px-2 py-0.5 text-[11px] font-medium" onclick="applyDiskBrandingPreset('wd')">WD_Black</button>
                <button type="button" class="btn-secondary px-2 py-0.5 text-[11px] font-medium" onclick="applyDiskBrandingPreset('kingston')">Kingston</button>
                <button type="button" class="btn-secondary px-2 py-0.5 text-[11px] font-medium" onclick="applyDiskBrandingPreset('genesis')">Genesis</button>
                <button type="button" class="btn-secondary px-2 py-0.5 text-[11px] font-medium" onclick="applyDiskBrandingPreset('default')">Reset</button>
            </div>

            <!-- Form Inputs -->
            <div class="space-y-3 text-xs sm:text-sm mb-4">
                <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
                    <div>
                        <label class="block text-[11px] font-medium uppercase tracking-wider text-zinc-500 dark:text-zinc-400 mb-1">Vendor OS Disk (Maks 8 char)</label>
                        <input type="text" id="disk-vendor-os" maxlength="8" placeholder="SAMSUNG" oninput="updateDiskBrandingPreview()" class="w-full px-3 py-1.5 text-xs rounded-md border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-950 text-zinc-900 dark:text-zinc-100 font-mono focus:outline-none focus:ring-1 focus:ring-zinc-400">
                    </div>
                    <div>
                        <label class="block text-[11px] font-medium uppercase tracking-wider text-zinc-500 dark:text-zinc-400 mb-1">Model OS Disk (Maks 16 char)</label>
                        <input type="text" id="disk-product-os" maxlength="16" placeholder="980 PRO NVMe" oninput="updateDiskBrandingPreview()" class="w-full px-3 py-1.5 text-xs rounded-md border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-950 text-zinc-900 dark:text-zinc-100 font-mono focus:outline-none focus:ring-1 focus:ring-zinc-400">
                    </div>
                </div>

                <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
                    <div>
                        <label class="block text-[11px] font-medium uppercase tracking-wider text-zinc-500 dark:text-zinc-400 mb-1">Vendor GameDisk (Maks 8 char)</label>
                        <input type="text" id="disk-vendor-game" maxlength="8" placeholder="WD_BLACK" oninput="updateDiskBrandingPreview()" class="w-full px-3 py-1.5 text-xs rounded-md border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-950 text-zinc-900 dark:text-zinc-100 font-mono focus:outline-none focus:ring-1 focus:ring-zinc-400">
                    </div>
                    <div>
                        <label class="block text-[11px] font-medium uppercase tracking-wider text-zinc-500 dark:text-zinc-400 mb-1">Model GameDisk (Maks 16 char)</label>
                        <input type="text" id="disk-product-game" maxlength="16" placeholder="SN850X GAME" oninput="updateDiskBrandingPreview()" class="w-full px-3 py-1.5 text-xs rounded-md border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-950 text-zinc-900 dark:text-zinc-100 font-mono focus:outline-none focus:ring-1 focus:ring-zinc-400">
                    </div>
                </div>

                <div>
                    <label class="block text-[11px] font-medium uppercase tracking-wider text-zinc-500 dark:text-zinc-400 mb-1">Firmware Revision (Maks 4 char)</label>
                    <input type="text" id="disk-revision-common" maxlength="4" placeholder="1.00" class="w-28 px-3 py-1.5 text-xs rounded-md border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-950 text-zinc-900 dark:text-zinc-100 font-mono focus:outline-none focus:ring-1 focus:ring-zinc-400">
                </div>
            </div>

            <!-- Live Windows Simulation Preview Box -->
            <div class="rounded-md border border-zinc-200 dark:border-zinc-800 bg-zinc-50/70 dark:bg-zinc-950/60 p-2.5 mb-4 space-y-1.5">
                <div class="text-[10.5px] font-medium text-zinc-500 dark:text-zinc-400 flex items-center gap-1.5">
                    <span>🖥️</span>
                    <span>Simulasi Nama Disk di Device Manager / Task Manager Windows:</span>
                </div>
                <div class="text-xs font-mono text-zinc-900 dark:text-zinc-100 bg-white dark:bg-zinc-900 px-2.5 py-1 rounded border border-zinc-200/80 dark:border-zinc-800/80 flex items-center justify-between">
                    <span class="text-zinc-400 text-[11px]">Boot:</span>
                    <span id="preview-os-disk-text" class="font-semibold text-emerald-600 dark:text-emerald-400 truncate ml-2">SAMSUNG 980 PRO SCSI Disk Device</span>
                </div>
                <div class="text-xs font-mono text-zinc-900 dark:text-zinc-100 bg-white dark:bg-zinc-900 px-2.5 py-1 rounded border border-zinc-200/80 dark:border-zinc-800/80 flex items-center justify-between">
                    <span class="text-zinc-400 text-[11px]">Game:</span>
                    <span id="preview-game-disk-text" class="font-semibold text-sky-600 dark:text-sky-400 truncate ml-2">WD_BLACK SN850X SCSI Disk Device</span>
                </div>
            </div>
        </div>

        <div>
            <button type="button" class="btn-primary w-full sm:w-auto px-4 py-2 text-xs font-medium shadow-2xs cursor-pointer" onclick="saveDiskBrandingAction()">💾 Simpan Identitas Disk</button>
        </div>
    </div>
</div>
```

- [ ] **Step 2: Tambahkan input kustom vendor di `partition-modal` (`ui/index.html`)**

Buka `partition-modal` dan tambahkan opsi kustomisasi vendor & model ketika role yang dipilih adalah GameDisk.

---

### Task 3: Frontend JavaScript Logic & Dynamic Live Preview

**Files:**
- Modify: `ui/index.js:1450-1550`
- Modify: `ui/index.js:1300-1340`

**Interfaces:**
- Consumes: `configObj.windows`, `configObj.gamedisk`
- Produces:
  - `loadDiskBrandingParams()`
  - `updateDiskBrandingPreview()`
  - `applyDiskBrandingPreset(presetKey)`
  - `saveDiskBrandingAction()`

- [ ] **Step 1: Implementasikan helper pemuatan dan live preview di `ui/index.js`**

Tambahkan fungsi-fungsi berikut ke `ui/index.js`:
```javascript
function loadDiskBrandingParams() {
    if (!configObj) return;

    const winCfg = configObj.windows || {};
    const gdList = configObj.gamedisk || [];
    const firstGd = gdList[0] || {};

    const osVendorEl = document.getElementById('disk-vendor-os');
    const osProductEl = document.getElementById('disk-product-os');
    const gmVendorEl = document.getElementById('disk-vendor-game');
    const gmProductEl = document.getElementById('disk-product-game');
    const revEl = document.getElementById('disk-revision-common');

    if (osVendorEl) osVendorEl.value = winCfg.vendor_id || 'RUSTISCS';
    if (osProductEl) osProductEl.value = winCfg.product_id || 'WindowsBoot';
    if (gmVendorEl) gmVendorEl.value = firstGd.vendor_id || 'RUSTISCS';
    if (gmProductEl) gmProductEl.value = firstGd.product_id || 'GameDisk';
    if (revEl) revEl.value = winCfg.product_revision || firstGd.product_revision || '1.00';

    updateDiskBrandingPreview();
}

function updateDiskBrandingPreview() {
    const osVendor = (document.getElementById('disk-vendor-os')?.value || 'RUSTISCS').trim().toUpperCase();
    const osProduct = (document.getElementById('disk-product-os')?.value || 'WindowsBoot').trim();
    const gmVendor = (document.getElementById('disk-vendor-game')?.value || 'RUSTISCS').trim().toUpperCase();
    const gmProduct = (document.getElementById('disk-product-game')?.value || 'GameDisk').trim();

    const previewOs = document.getElementById('preview-os-disk-text');
    if (previewOs) {
        previewOs.textContent = `${osVendor} ${osProduct} SCSI Disk Device`;
    }

    const previewGame = document.getElementById('preview-game-disk-text');
    if (previewGame) {
        previewGame.textContent = `${gmVendor} ${gmProduct} SCSI Disk Device`;
    }
}

function applyDiskBrandingPreset(presetKey) {
    const presets = {
        samsung: {
            osVendor: 'SAMSUNG', osProduct: '980 PRO NVMe',
            gmVendor: 'SAMSUNG', gmProduct: '990 PRO 2TB',
            rev: '2.00'
        },
        wd: {
            osVendor: 'WD_BLACK', osProduct: 'SN850X NVMe',
            gmVendor: 'WD_BLACK', gmProduct: 'SN850X GAME',
            rev: '1.00'
        },
        kingston: {
            osVendor: 'KINGSTON', osProduct: 'FURY Renegade',
            gmVendor: 'KINGSTON', gmProduct: 'KC3000 PCIe',
            rev: '1.00'
        },
        genesis: {
            osVendor: 'GENESIS', osProduct: 'DISKLESS BOOT',
            gmVendor: 'GENESIS', gmProduct: 'FAST GAMEDISK',
            rev: '1.00'
        },
        default: {
            osVendor: 'RUSTISCS', osProduct: 'WindowsBoot',
            gmVendor: 'RUSTISCS', gmProduct: 'GameDisk-0',
            rev: '1.00'
        }
    };

    const p = presets[presetKey] || presets.default;
    const osVendorEl = document.getElementById('disk-vendor-os');
    const osProductEl = document.getElementById('disk-product-os');
    const gmVendorEl = document.getElementById('disk-vendor-game');
    const gmProductEl = document.getElementById('disk-product-game');
    const revEl = document.getElementById('disk-revision-common');

    if (osVendorEl) osVendorEl.value = p.osVendor;
    if (osProductEl) osProductEl.value = p.osProduct;
    if (gmVendorEl) gmVendorEl.value = p.gmVendor;
    if (gmProductEl) gmProductEl.value = p.gmProduct;
    if (revEl) revEl.value = p.rev;

    updateDiskBrandingPreview();
}

async function saveDiskBrandingAction() {
    if (!configObj) configObj = {};
    if (!configObj.windows) configObj.windows = {};

    const osVendor = (document.getElementById('disk-vendor-os')?.value || 'RUSTISCS').trim();
    const osProduct = (document.getElementById('disk-product-os')?.value || 'WindowsBoot').trim();
    const gmVendor = (document.getElementById('disk-vendor-game')?.value || 'RUSTISCS').trim();
    const gmProduct = (document.getElementById('disk-product-game')?.value || 'GameDisk').trim();
    const rev = (document.getElementById('disk-revision-common')?.value || '1.00').trim();

    configObj.windows.vendor_id = osVendor;
    configObj.windows.product_id = osProduct;
    configObj.windows.product_revision = rev;

    if (Array.isArray(configObj.gamedisk)) {
        configObj.gamedisk.forEach((gd, idx) => {
            gd.vendor_id = gmVendor;
            gd.product_id = configObj.gamedisk.length > 1 ? `${gmProduct}-${idx}` : gmProduct;
            gd.product_revision = rev;
        });
    }

    const ok = await saveConfigJsonFull();
    if (ok) {
        showToast('Identitas vendor disk berhasil disimpan & diperbarui!', 'success');
        await loadConfigJson();
        loadDiskBrandingParams();
    } else {
        showToast('Gagal menyimpan identitas disk', 'error');
    }
}
```

- [ ] **Step 2: Sambungkan `loadDiskBrandingParams()` di `loadConfigJson()`**

Di `loadConfigJson()` di `ui/index.js`, panggil `loadDiskBrandingParams()` pada blok `finally`.

---

### Task 4: Local Tailwind CSS Rebuild, Responsive Layout Audit & Test Suite

**Files:**
- Modify: `ui/input.css` (jika dibutuhkan utilities tambahan)
- Build: `ui/tailwind.css`
- Run: `npm run build:css`
- Run: `cargo test`

- [ ] **Step 1: Rebuild file CSS lokal**

Command: `npm run build:css`
Expected: exit code 0 (`tailwindcss v4.3.3`, Done in <300ms).

- [ ] **Step 2: Jalankan full suite test backend Rust**

Command: `cargo test`
Expected: 100% tests passing tanpa kegagalan.

- [ ] **Step 3: Uji responsivitas layout HTML**

Pastikan:
- Mobile (`sm: 640px`): Card 1 dan Card 2 tersusun 1 kolom vertikal.
- Laptop / Desktop (`lg: 1024px`, `xl: 1280px`, `2xl: 1536px`): Card 1 (Storage Parameters) dan Card 2 (Identitas & Vendor SCSI Disk) sejajar simetris dalam 2 kolom (`grid-cols-2`).

---

### Task 5: Dokumentasi Living Memory di `ANTIGRAVITY.md` & User Handoff

**Files:**
- Modify: `ANTIGRAVITY.md` (Section 3.2 Kasus Baru & Section 9 Changelog)

- [ ] **Step 1: Dokumentasikan Arsitektur SCSI Disk Branding di `ANTIGRAVITY.md`**

Tambahkan sub-bab baru di `ANTIGRAVITY.md` yang menjelaskan:
- Standar SCSI SPC-4 INQUIRY response 36-byte.
- Mekanisme parsing dan pembentukan nama perangkat di Windows Device Manager.
- Sinkronisasi file watcher `config_manager.rs` saat vendor/product diperbarui.

- [ ] **Step 2: Catat Changelog resmi di Bagian 9 `ANTIGRAVITY.md`**

- [ ] **Step 3: Verifikasi status Git tanpa commit otomatis**

Command: `git status`
Confirm: File termodifikasi ada di working tree, siap untuk review pengguna.
