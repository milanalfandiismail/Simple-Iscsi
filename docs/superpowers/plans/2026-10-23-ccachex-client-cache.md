# Rencana Cache Client CCacheX (Hanya Driver CCBoot)

> Tanggal: 2026-10-23
> Status: plan baru, belum ada implementasi.
> Cakupan: cache client <code>CCacheX.sys</code> untuk client yang memakai driver <code>CCBootPNPX.sys</code>.
> Prinsip: driverless tidak berubah; cache default OFF; hanya mode read-cache; tanpa helper runtime dan tanpa control-plane CCBoot.

## 1. Tujuan

Client CCBoot bisa memakai cache RAM <code>CCacheX</code> yang dikonfigurasi per client dari Simple-Iscsi:

1. Admin mengaktifkan cache dan menentukan ukuran MB di UI.
2. Server mengirim hostname iBFT berformat <code>Hostname(1-SizeMB-0)</code>.
3. <code>CCacheX.sys</code> membaca hostname itu dan mengaktifkan cache read-only.
4. <code>CCBootPNPX.sys</code> tetap menulis <code>ComputerName</code> tanpa suffix, karena driver memotong string di karakter <code>(</code>.
5. Client driverless tidak menerima suffix dan tidak berubah.

Definisi selesai:

- Client CCBoot boot normal dengan cache aktif; nilai <code>CacheSize</code> sesuai.
- <code>ComputerName</code> tetap nama dasar tanpa <code>(...)</code>.
- Client native/driverless tetap identik seperti sebelumnya.
- Tidak ada BSOD, freeze, atau regresi boot.

## 2. Non-goals

- Tidak mengekspos mode 2 (writeback) atau mode 3 (ClientCacheToLocal).
- Tidak menulis registry <code>CCacheX\Parameters</code> secara runtime dari server.
- Tidak mengubah server writeback overlay, IQN suffix, atau binary driver.
- Tidak menambah helper/agent/control-plane CCBoot.
- Tidak mengaktifkan cache untuk client driverless.

## 3. Fakta Terverifikasi dari Binary

| Fakta | Bukti |
|---|---|
| <code>CCacheX.sys</code> versi 1.0.0.2, SHA-256 d28029bc...8df4c, deskripsi "the driver for volume cache" | REA open_binary + string resource |
| <code>CCacheX</code> membaca iBFT lewat ACPI <code>IBFT</code>/<code>iBFT</code> | <code>FUN_00013904</code>, <code>AuxKlibGetSystemFirmwareTable(0x41435049, 0x54464249/0x54464269)</code> |
| Parser cache membaca NIC block tipe 0x03, flag valid bit 0, lalu <code>hostname.offset</code> di offset 0x64 | <code>FUN_00013ad4</code> |
| Format hostname: <code>Hostname(Mode-Size-0)</code>; <code>Mode</code> = angka pertama, <code>Size</code> = angka kedua | <code>strstr(hostname,"(")</code> lalu <code>atoi</code>; <code>strstr(...,"-")</code> lalu <code>atoi</code> |
| Mode &lt; 2: <code>ClientCache=1</code>, <code>WriteBack</code> tidak diubah | <code>FUN_00013ad4</code> |
| Mode 2: <code>WriteBack=1</code> | <code>FUN_00013ad4</code> |
| Mode 3: <code>ClientCacheToLocal=1</code>, <code>ClientCache=0</code> | <code>FUN_00013ad4</code> |
| Size disimpan di <code>+0x1e5</code> dalam MB; minimum 32 MB; dibulatkan ke kelipatan 16 MB | <code>FUN_00012854</code> |
| Size 0 = otomatis 20% RAM fisik | <code>FUN_00013ad4</code> memakai <code>MmGetPhysicalMemoryRanges</code> |
| <code>CCBootPNPX.sys</code> memotong hostname di <code>(</code> sebelum menulis <code>ComputerName</code>/<code>NV Hostname</code> | <code>FUN_14000ecc0</code>: <code>strstr(hostname,"(")</code> lalu <code>*(byte*)pcVar4=0</code>; string "(" di 0x1400f90fe = 28 00 |
| <code>CCacheX</code> tidak membaca IQN/":0XX" sama sekali | tidak ada string iqn/TargetName/:0; tidak ada pembacaan target block |
| Nilai 2A pada :02A adalah hash FNV-1a MAC dari generator kita, bukan kode CCBoot | <code>ccboot_iqn_suffix</code>; FNV-1a("00:0C:29:9B:72:2A") low byte = 0x2A |

Konsekuensi: suffix hostname aman untuk <code>ComputerName</code>, karena driver memotongnya. Cache tetap hanya aktif jika <code>CCacheX.sys</code> terpasang dan service-nya berjalan.

Catatan: <code>CCacheX</code> membaca salinan iBFT sendiri (<code>FUN_00013904</code> mengalokasikan buffer via <code>ExAllocatePoolWithTag</code>), sehingga pemotongan yang dilakukan <code>CCBootPNPX</code> di buffer-nya sendiri tidak boleh mengubah data yang dibaca <code>CCacheX</code>. Phase 0 memverifikasi ini di runtime.

## 4. Desain

### 4.1 Field clients.toml

~~~toml
[[client]]
hostname = "PC-01"
mac = "00:11:22:33:44:55"
ip = "192.168.1.101"
gateway = "192.168.1.1"
dns = "8.8.8.8"
pxe = "sb-custom"
next_server = "192.168.1.2"
image_manager = "win10"

# Hanya untuk client CCBoot
driver = "ccboot"          # "native" (default) | "ccboot"
cache_enabled = true       # default false
cache_size_mb = 2048       # 0 = auto 20% RAM; 32-8192; kelipatan 16
~~~

Untuk client driverless, tiga field ini boleh tidak ada. Nilai default: <code>driver="native"</code>, <code>cache_enabled=false</code>, <code>cache_size_mb=0</code>.

### 4.2 Format hostname

| Kondisi | Hostname yang dikirim |
|---|---|
| native | <code>PC-01</code> |
| ccboot, cache off | <code>PC-01</code> |
| ccboot, cache auto | <code>PC-01(1-0-0)</code> |
| ccboot, cache 2048 MB | <code>PC-01(1-2048-0)</code> |

Aturan:

- Base hostname disanitasi: alfanumerik + <code>-</code>, maksimal 15 karakter.
- Suffix selalu mode <code>1</code> (read cache), ukuran MB, lalu <code>-0</code>.
- Field <code>cache_size_mb</code> adalah satu-satunya input ukuran; tidak ada konversi GB otomatis di UI.
- Jika hostname mengandung <code>(</code> atau <code>)</code>, UI menolak; jangan menulis suffix manual.

### 4.3 Alur data

~~~text
clients.toml
  driver=ccboot, cache_enabled=true, cache_size_mb=2048
        |
        v
DHCP option 12 = "PC-01(1-2048-0)"
        |
        v
iPXE net0/hostname -> sanboot -> iBFT NIC hostname
        |
        +--> CCacheX.sys: baca (Mode=1, Size=2048), aktifkan read cache
        |
        +--> CCBootPNPX.sys: potong di "(", tulis ComputerName="PC-01"
~~~

### 4.4 Aturan aman

1. Cache default OFF; hanya aktif jika <code>driver="ccboot"</code> dan <code>cache_enabled=true</code>.
2. Mode dikunci 1; mode 2/3 tidak diekspos di UI/API.
3. <code>WriteBack</code> harus 0 di image; mode 1 tidak memaksa writeback, tetapi juga tidak mematikannya. Wajib preflight.
4. <code>ClientCacheToLocal</code> harus 0; client diskless tidak boleh bergantung pada local disk.
5. Ukuran 0 (auto 20% RAM) adalah default; fixed maksimal 8192 MB.
6. Base hostname maksimal 15 karakter; suffix tidak ikut ke <code>ComputerName</code>.
7. Cache off tidak otomatis mematikan service CCacheX yang sudah enabled lewat registry; lihat Rollback.

## 5. Phase 0 - Spike Validasi Hostname (BLOCKING)

Tujuan: membuktikan di client nyata bahwa suffix <code>(1-2048-0)</code> sampai ke CCacheX dan tidak merusak <code>ComputerName</code>.

- [ ] 0.1 Backup <code>clients.toml</code>, <code>pxe/sb-custom/autoexec.ipxe</code>, dan snapshot VHD client uji.
- [ ] 0.2 Tambahkan sementara di <code>pxe/sb-custom/autoexec.ipxe</code>, setelah echo hostname dan sebelum sanboot:

~~~ipxe
set net0/hostname TEST(1-2048-0)
~~~

- [ ] 0.3 Boot client uji. Di layar iPXE, pastikan echo hostname menampilkan <code>TEST(1-2048-0)</code>.
- [ ] 0.4 Setelah Windows boot, jalankan di client:

~~~powershell
reg query "HKLM\SYSTEM\CurrentControlSet\Control\ComputerName\ComputerName" /v ComputerName
reg query "HKLM\SYSTEM\CurrentControlSet\Services\CCacheX\Parameters" /s
reg query "HKLM\SYSTEM\CCBootPnP" /s
sc query CCacheX
~~~

Kriteria lulus:

- <code>ComputerName</code> = <code>TEST</code> (tanpa kurung).
- <code>CacheSize</code> = 2048 (atau nilai yang dikirim).
- <code>ClientCache</code> = 1.
- <code>WriteBack</code> = 0.
- <code>ClientCacheToLocal</code> = 0.
- Service <code>CCacheX</code> RUNNING; tidak ada BSOD/freeze.

- [ ] 0.5 Catat path registry aktual dan nilai sebelum/sesudah. Revert baris <code>set net0/hostname</code>.
- [ ] 0.6 Gate: jika <code>ComputerName</code> mengandung kurung, atau cache tidak aktif, STOP. Jangan lanjut ke implementasi hostname; evaluasi fallback registry provisioning.

## 6. Phase 1 - Model Konfigurasi

File: <code>src/config.rs</code>, <code>clients.toml.example</code>.

- [ ] 1.1 Tambahkan enum dan field:

~~~rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BootDriver {
    Native,
    Ccboot,
}

impl Default for BootDriver {
    fn default() -> Self { Self::Native }
}

// di ClientConfig
#[serde(default)]
pub driver: BootDriver,
#[serde(default)]
pub cache_enabled: bool,
#[serde(default)]
pub cache_size_mb: u32,
~~~

- [ ] 1.2 Tambahkan helper validasi/normalisasi:

~~~rust
pub fn normalize_cache_size_mb(size: u32) -> Result<u32, String> {
    if size == 0 { return Ok(0); }
    if !(32..=8192).contains(&size) {
        return Err("cache_size_mb harus 0 atau 32-8192".to_string());
    }
    if size % 16 != 0 {
        return Err("cache_size_mb harus kelipatan 16".to_string());
    }
    Ok(size)
}

pub fn normalize_client_cache(client: &mut ClientConfig) -> Result<(), String> {
    if client.driver != BootDriver::Ccboot {
        client.cache_enabled = false;
        client.cache_size_mb = 0;
        return Ok(());
    }
    if client.cache_enabled {
        client.cache_size_mb = normalize_cache_size_mb(client.cache_size_mb)?;
    } else {
        client.cache_size_mb = 0;
    }
    Ok(())
}
~~~

- [ ] 1.2b Tolak hostname yang mengandung '(' atau ')' di backend; UI juga menolak. Ini mencegah suffix manual ganda.

- [ ] 1.3 Update <code>append_client()</code> agar menulis:

~~~text
driver          = "..."
cache_enabled   = true/false
cache_size_mb   = 0/2048
~~~

- [ ] 1.4 Update <code>clients.toml.example</code> dengan satu contoh client CCBoot dan komentar aturan ukuran.
- [ ] 1.5 Update template default di <code>post_clients_json()</code> agar menyebut field baru.
- [ ] 1.6 Unit test di <code>src/config.rs</code>:

~~~text
test_client_config_defaults_are_native_cache_off
test_normalize_cache_size_zero_auto
test_normalize_cache_size_min_max_multiple_16
test_normalize_native_clears_cache
test_clients_toml_roundtrip_with_ccachex_fields
~~~

Verifikasi: <code>cargo test config::tests</code>.

## 7. Phase 2 - DHCP Hostname

File: <code>src/netboot/dhcp.rs</code>.

- [ ] 7.1 Tambahkan helper:

~~~rust
fn format_ccachex_hostname(raw: &str, client: &ClientConfig) -> String {
    let base_raw = raw.split('(').next().unwrap_or(raw);
    let base = sanitize_hostname(base_raw);
    if client.driver == crate::config::BootDriver::Ccboot && client.cache_enabled {
        let size = client.cache_size_mb;
        format!("{}(1-{}-0)", base, size)
    } else {
        base
    }
}
~~~

- [ ] 7.2 Ganti pemakaian option 12:

~~~rust
let hostname = client_conf
    .as_ref()
    .map(|c| format_ccachex_hostname(c.hostname.as_deref().unwrap_or(""), c))
    .unwrap_or_else(|| {
        let code = client_conf.as_ref()
            .map(|c| fnv1a(c.mac.as_bytes()) & 0xFF)
            .unwrap_or(1);
        format!("PC-{:02}", code)
    });
resp.options.insert(12, hostname.as_bytes().to_vec());
info!("DHCP option 12 hostname: {}", hostname);
~~~

- [ ] 7.3 Jangan mengubah option 169/17; suffix IQN tetap dari <code>with_ccboot_suffix</code>.
- [ ] 7.4 Unit test di <code>src/netboot/dhcp.rs</code>:

~~~text
test_hostname_native_unchanged
test_hostname_ccboot_cache_auto
test_hostname_ccboot_cache_fixed
test_hostname_ccboot_cache_off
test_hostname_sanitizes_invalid_chars
test_hostname_strips_manual_suffix
~~~

Verifikasi: <code>cargo test netboot::dhcp</code>.

## 8. Phase 3 - UI/UX

File: <code>ui/index.html</code>, <code>ui/index.js</code>. Tailwind lokal; jangan edit <code>ui/tailwind.css</code> manual.

### 8.1 Form modal client

Tambahkan select driver dan section cache di <code>#client-crud-form</code>:

~~~html
<div>
  <label class="block text-xs font-medium uppercase tracking-wider text-zinc-500 dark:text-zinc-400 mb-1.5">Boot Driver</label>
  <select id="client-driver" onchange="onClientDriverChange()" class="w-full px-3 py-2 text-xs sm:text-sm rounded-md border border-zinc-200 dark:border-zinc-800 bg-zinc-50/50 dark:bg-zinc-950/60 text-zinc-900 dark:text-zinc-100 focus:border-zinc-900 dark:focus:border-zinc-100 focus:outline-none transition-colors">
    <option value="native">Native (driverless)</option>
    <option value="ccboot">CCBoot (CCBootPNPX.sys)</option>
  </select>
</div>

<div id="client-ccachex-section" class="hidden border border-zinc-200 dark:border-zinc-800 rounded-md p-3.5 space-y-3">
  <div class="flex items-center justify-between gap-3">
    <div>
      <div class="text-xs font-medium text-zinc-900 dark:text-zinc-100">CCacheX Client Cache</div>
      <div class="text-[10.5px] text-zinc-500 dark:text-zinc-400">Hanya untuk driver CCBoot. Mode read-cache; writeback tidak diaktifkan.</div>
    </div>
    <label class="inline-flex items-center gap-2 text-xs text-zinc-700 dark:text-zinc-300">
      <input type="checkbox" id="client-cache-enabled" onchange="onClientCacheToggle()" class="rounded border-zinc-300 dark:border-zinc-700">
      Aktifkan
    </label>
  </div>
  <div>
    <label class="block text-xs font-medium uppercase tracking-wider text-zinc-500 dark:text-zinc-400 mb-1.5">Ukuran cache (MB)</label>
    <input type="number" id="client-cache-size-mb" min="0" max="8192" step="16" value="0"
      class="w-full px-3 py-2 text-xs sm:text-sm rounded-md border border-zinc-200 dark:border-zinc-800 bg-zinc-50/50 dark:bg-zinc-950/60 text-zinc-900 dark:text-zinc-100 focus:border-zinc-900 dark:focus:border-zinc-100 focus:outline-none transition-colors">
    <p class="text-[10.5px] text-zinc-500 dark:text-zinc-400 mt-1.5">0 = otomatis 20% RAM. Min 32, maks 8192, kelipatan 16 MB. 1024 MB = 1 GB.</p>
  </div>
</div>
~~~

### 8.2 JavaScript

- [ ] 8.2.1 <code>openClientCrudModal(client)</code>: isi <code>client-driver</code>, <code>client-cache-enabled</code>, <code>client-cache-size-mb</code>; panggil <code>onClientDriverChange()</code> dan <code>onClientCacheToggle()</code>.
- [ ] 8.2.2 Tambahkan fungsi:

~~~javascript
function onClientDriverChange() {
    const isCcboot = document.getElementById('client-driver').value === 'ccboot';
    document.getElementById('client-ccachex-section').classList.toggle('hidden', !isCcboot);
    if (!isCcboot) {
        document.getElementById('client-cache-enabled').checked = false;
        document.getElementById('client-cache-size-mb').value = '0';
    }
    onClientCacheToggle();
}

function onClientCacheToggle() {
    const enabled = document.getElementById('client-cache-enabled').checked;
    const size = document.getElementById('client-cache-size-mb');
    size.disabled = !enabled;
    if (!enabled) size.value = '0';
}
~~~

- [ ] 8.2.3 <code>saveClientAction()</code>: validasi sebelum push:

~~~javascript
const driver = document.getElementById('client-driver').value;
const cacheEnabled = document.getElementById('client-cache-enabled').checked;
let cacheSize = parseInt(document.getElementById('client-cache-size-mb').value || '0', 10);

if (driver !== 'ccboot') {
    cacheSize = 0;
    cacheEnabled = false;
}
if (cacheEnabled && !(cacheSize === 0 || (cacheSize >= 32 && cacheSize <= 8192 && cacheSize % 16 === 0))) {
    showToast('Ukuran cache harus 0 atau 32-8192 MB, kelipatan 16 MB', 'error');
    return;
}
if (cacheEnabled && !document.getElementById('client-hostname').value.trim()) {
    showToast('Hostname wajib diisi untuk cache CCacheX', 'error');
    return;
}
~~~

lalu masukkan <code>driver</code>, <code>cache_enabled</code>, <code>cache_size_mb</code> ke <code>clientData</code>.

- [ ] 8.2.4 Badge di <code>renderDashboardClientsTable()</code> dan <code>renderClientsManagerTable()</code>:

~~~javascript
function renderCacheBadge(c) {
    if (c.driver !== 'ccboot') return '';
    if (!c.cache_enabled) {
        return '<span class="...">Cache: Off</span>';
    }
    const label = c.cache_size_mb ? escapeHtml(String(c.cache_size_mb)) + ' MB' : 'Auto';
    return '<span class="...">Cache: ' + label + '</span>';
}
~~~

Letakkan di bawah baris <code>PXE:</code> pada kolom image.

- [ ] 8.2.5 Pastikan <code>clientData</code> tidak menghapus field lama saat edit; gunakan objek baru yang lengkap, dan <code>getMergedDashboardClients()</code> mempertahankan field <code>driver</code>/<code>cache_enabled</code>/<code>cache_size_mb</code>.
- [ ] 8.2.6 Manual test:

~~~text
1. Tambah client native -> section cache tersembunyi.
2. Ganti ke CCBoot -> section muncul.
3. Aktifkan cache, isi 2048 -> simpan, buka lagi -> 2048 persist.
4. Isi 1000 -> error kelipatan 16.
5. Isi 9000 -> error maksimum.
6. Native + cache aktif -> cache dibersihkan saat simpan.
7. Badge tabel: Off / Auto / 2048 MB.
~~~

## 9. Phase 4 - API & Validasi Backend

File: <code>src/api/routes_client.rs</code>, <code>src/config.rs</code>.

- [ ] 9.1 Di <code>post_clients_json()</code>, buat <code>clients_cfg</code> mutable, loop semua client, panggil <code>normalize_client_cache()</code>. Jika <code>Err</code>, kembalikan HTTP 400 dengan pesan error.
- [ ] 9.2 <code>get_clients_json()</code> tidak perlu diubah; serde otomatis menyertakan field baru.
- [ ] 9.3 Update komentar template default agar menyebut <code>driver</code>, <code>cache_enabled</code>, <code>cache_size_mb</code>.
- [ ] 9.4 Pastikan <code>append_client()</code> dan <code>post_clients_json()</code> menghasilkan TOML yang valid.
- [ ] 9.5 API test manual:

~~~text
GET  /api/clients/json -> field baru muncul.
POST valid (ccboot, true, 2048) -> 200.
POST invalid (1000) -> 400.
POST native + cache true -> 200 dan field cache dinormalisasi ke false/0.
~~~

## 10. Phase 5 - Dokumentasi

- [ ] 10.1 Buat <code>docs/ccachex-cache-analysis.md</code>: ringkasan RE, format hostname, mode, unit MB, bukti alamat fungsi, dan batasan.
- [ ] 10.2 Update <code>docs/panduan-kompatibilitas-driver-ccboot.md</code>:
  - bagian 3.3 Hostname: jelaskan suffix <code>(Mode-Size-0)</code> dan pemotongan di <code>(</code> oleh CCBootPNPX;
  - tambah subbagian cache client: default off, mode 1, size MB, verifikasi registry.
- [ ] 10.3 Update <code>DOCUMENTATION.md</code> BAB 10 dengan ringkasan cache dan link ke panduan.
- [ ] 10.4 Koreksi klaim lama di <code>docs/ccbootpnpx-ibft-verification.md</code> poin 0.3: bukan "tidak ada perbandingan" tetapi "tidak ada validasi format; driver memotong di karakter (".
- [ ] 10.5 Update <code>clients.toml.example</code> dan komentar template API.
- [ ] 10.6 Buat <code>scripts/check-ccachex.ps1</code>: query <code>ComputerName</code>, <code>CCacheX\Parameters</code>, dan <code>CCBootPnP</code>; exit 1 jika <code>WriteBack != 0</code> atau <code>ClientCacheToLocal != 0</code>; parameter <code>-Disable</code> untuk set <code>AutoStart=0</code>/<code>ClientCache=0</code>.

## 11. Phase 6 - Acceptance

Jalankan berurutan:

~~~powershell
cargo fmt --check
cargo check
cargo test
npm run build:css
~~~

Manual:

- [ ] 11.1 Server log DHCP menampilkan option 12 <code>PC-01(1-2048-0)</code> untuk client CCBoot.
- [ ] 11.2 Client native tetap menerima hostname tanpa suffix.
- [ ] 11.3 Di client CCBoot:

~~~powershell
reg query "HKLM\SYSTEM\CurrentControlSet\Control\ComputerName\ComputerName" /v ComputerName
reg query "HKLM\SYSTEM\CurrentControlSet\Services\CCacheX\Parameters" /s
reg query "HKLM\SYSTEM\CCBootPnP" /s
sc query CCacheX
~~~

- [ ] 11.4 <code>ComputerName</code> = base hostname; <code>CacheSize</code> = nilai yang diminta; <code>ClientCache</code> = 1; <code>WriteBack</code> = 0; <code>ClientCacheToLocal</code> = 0.
- [ ] 11.5 Boot 2x berturut-turut; tidak ada BSOD/freeze; cache tetap aktif.
- [ ] 11.6 Client driverless boot normal; tidak ada perubahan registry/CCacheX.
- [ ] 11.7 UI: badge, validasi, dan persistensi field benar.
- [ ] 11.8 <code>npm run build:css</code> dijalankan **terakhir** setelah semua perubahan UI; commit <code>ui/tailwind.css</code> yang baru.

## 12. Rollback

1. Set <code>cache_enabled=false</code> atau <code>driver="native"</code>; simpan; reboot client.
2. Pastikan DHCP option 12 kembali tanpa suffix.
3. Jika cache masih aktif karena registry image, di client:

~~~powershell
reg add "HKLM\SYSTEM\CurrentControlSet\Services\CCacheX\Parameters" /v AutoStart /t REG_DWORD /d 0 /f
reg add "HKLM\SYSTEM\CurrentControlSet\Services\CCacheX\Parameters" /v ClientCache /t REG_DWORD /d 0 /f
~~~

4. Jika boot bermasalah, restore snapshot VHD dan <code>clients.toml</code>.
5. Hapus baris spike <code>set net0/hostname</code> jika masih ada.

## 13. Risiko & Mitigasi

| Risiko | Mitigasi |
|---|---|
| Hostname suffix tidak sampai ke iBFT | Phase 0 wajib; cek echo iPXE + registry |
| <code>ComputerName</code> rusak | Phase 0; driver memotong di <code>(</code>; jika gagal, fallback registry |
| <code>WriteBack=1</code> di image | Preflight registry; dokumentasi; jangan aktifkan cache jika writeback belum 0 |
| Cache terlalu besar | Default 0 auto; cap 8192 MB; validasi kelipatan 16 |
| Mode 2/3 terpakai | UI/API hanya mode 1; tidak ada field mode |
| Client native ikut kena suffix | Gate <code>driver == ccboot</code> + unit test |
| UI lupa build Tailwind | Langkah terakhir wajib <code>npm run build:css</code> |
| Versi CCacheX berbeda | Plan berdasarkan 1.0.0.2; verifikasi ulang jika binary berubah |

## 14. Deliverables

1. Perubahan <code>src/config.rs</code> + <code>src/netboot/dhcp.rs</code> + test.
2. Perubahan <code>src/api/routes_client.rs</code>.
3. Perubahan <code>ui/index.html</code> + <code>ui/index.js</code> + <code>ui/tailwind.css</code> hasil build.
4. <code>docs/ccachex-cache-analysis.md</code>.
5. Update <code>docs/panduan-kompatibilitas-driver-ccboot.md</code> + <code>DOCUMENTATION.md</code>.
6. <code>scripts/check-ccachex.ps1</code> untuk verifikasi client.
7. Log Phase 0 + acceptance.

## 15. Urutan Eksekusi

~~~text
Phase 0 (spike, blocking)
  -> Phase 1 (config)
  -> Phase 2 (DHCP)
  -> Phase 3 (UI)
  -> Phase 4 (API)
  -> Phase 5 (docs)
  -> Phase 6 (acceptance)
  -> npm run build:css (paling akhir)
~~~

Setiap phase berhenti jika gate gagal. Jangan lanjut ke UI/implementasi sebelum Phase 0 lulus.
