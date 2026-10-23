# Analisis Cache Client CCacheX

> Status: reverse engineering statis; belum diuji runtime.
> Binary: <code>CCacheX.sys</code> versi 1.0.0.2, SHA-256 <code>d28029bc60bb56f36533bd2a39b5a771ff95f6c8a7b23ce8b644434b8df8df4c</code>.

## 1. Fungsi

<code>CCacheX.sys</code> adalah driver volume cache. Ia membaca tabel iBFT dan nilai registry <code>CCBootPnP</code>/<code>CCacheX\Parameters</code>.

## 2. iBFT dan hostname

- <code>FUN_00013904</code> membaca tabel ACPI <code>IBFT</code>/<code>iBFT</code> lewat <code>AuxKlibGetSystemFirmwareTable</code>; fallback scan fisik <code>0x80000</code>.
- <code>FUN_00013ad4</code> membaca NIC block tipe <code>0x03</code>, flag valid bit 0, lalu <code>hostname.offset</code> di offset <code>0x64</code>.
- Format hostname: <code>Hostname(Mode-Size-0)</code>.
- <code>Mode</code>:
  - <code>&lt; 2</code>: <code>ClientCache=1</code>, <code>WriteBack</code> tidak diubah.
  - <code>2</code>: <code>WriteBack=1</code>.
  - <code>3</code>: <code>ClientCacheToLocal=1</code>, <code>ClientCache=0</code>.
- <code>Size</code> disimpan dalam MB; <code>0</code> = otomatis 20% RAM; minimum 32 MB; dibulatkan ke kelipatan 16 MB.

## 2.1 Filter Vendor ID

- <code>FUN_000113c8</code> membaca <code>STORAGE_DEVICE_DESCRIPTOR</code>, mengambil <code>VendorIdOffset</code> di <code>+0xC</code>, lalu mencari string <code>CCBoot</code>, <code>CCDisk</code>, atau <code>VMware</code>.
- Jika <code>ClientCache=1</code> dan vendor tidak cocok, CCacheX melewati inisialisasi cache untuk volume itu.
- Default Simple-Iscsi adalah <code>RUSTISCS</code>, jadi tidak cocok. Set <code>[windows] vendor_id = "CCBoot"</code> di <code>config.toml</code>.
- Ini bukan karena driver VMware; VMware justru salah satu vendor yang diterima.

## 3. Hubungan dengan IQN

- Tidak ada string <code>iqn</code>, <code>TargetName</code>, atau <code>:0</code> di binary.
- Tidak ada pembacaan target block iBFT.
- Nilai <code>2A</code> pada <code>:02A</code> berasal dari generator Simple-Iscsi (FNV-1a MAC), bukan dari <code>CCacheX</code>.

## 4. CCBootPNPX dan hostname

- <code>FUN_14000ecc0</code> memotong hostname di karakter <code>(</code> sebelum menulis <code>ComputerName</code>/<code>NV Hostname</code>.
- <code>CCacheX</code> membaca salinan iBFT sendiri; Phase 0 plan wajib memverifikasi cache tetap aktif di runtime.

## 5. Verifikasi client

~~~powershell
scripts\check-ccachex.ps1 -ExpectedSizeMB 2048
~~~

Path registry yang diperiksa:

- <code>HKLM\SYSTEM\CurrentControlSet\Services\CCacheX\Parameters</code> untuk <code>CacheSize</code>, <code>ClientCache</code>, <code>WriteBack</code>.
- <code>HKLM\SYSTEM\CCBootPnP</code> untuk <code>ClientCacheToLocal</code>.
- <code>HKLM\SYSTEM\CurrentControlSet\Control\ComputerName\ComputerName</code> untuk <code>ComputerName</code>.

## 6. Batasan

- Analisis statis; perilaku runtime dan urutan load driver perlu diuji.
- <code>CCacheX.sys</code> versi lain bisa berbeda; verifikasi ulang jika binary berubah.
- Mode 2/3 tidak dipakai di Simple-Iscsi.
