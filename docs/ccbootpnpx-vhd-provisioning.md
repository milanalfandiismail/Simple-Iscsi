# Provinsi VHD - CCBootPNPX.sys (Pure CCBoot, Tanpa Helper)

> Tanggal: 2026-10-23
> Scope: Phase 3 - hanya driver CCBootPNPX.sys + service registry.
> Prinsip: TIDAK memakai helper kita, TIDAK install control-plane CCBootClient, TIDAK menulis CCBootPnP dari luar.

## 1. Tujuan

Memasukkan driver CCBootPNPX.sys ke image VHD Windows secara offline, sehingga saat client boot:

1. Driver dimuat sebagai boot-start driver.
2. Driver dapat membaca iBFT dan menjalankan logika identitas.
3. Tidak ada komponen helper/control-plane tambahan.

## 2. Prasyarat

- VHD dalam keadaan OFFLINE (tidak sedang di-serve iSCSI, tidak di-mount oleh VM lain).
- Backup VHD sebelum modifikasi.
- Akses Administrator di server.
- File driver asli bertanda tangan: CCBootPNPX.sys (versi 2.0.0.12, SHA-256 6872982d...).
- Drive letter sementara untuk partisi Windows di VHD (contoh V:).

## 3. Langkah 1 - Mount VHD

Jalankan diskpart (manual, agar tidak salah partisi):

```text
diskpart
select vdisk file="C:\vhd\w11.vhd"
attach vdisk
list volume
select volume <nomor volume Windows>
assign letter=V
exit
```

Verifikasi: `V:\Windows\System32\config\SYSTEM` ada.

## 4. Langkah 2 - Copy driver

```powershell
Copy-Item 'C:\Project GIT\Simple-Iscsi\CCBootPNPX.sys' "V:\Windows\System32\drivers\CCBootPNPX.sys" -Force
Get-Item "V:\Windows\System32\drivers\CCBootPNPX.sys" | Select-Object Length, VersionInfo
```

Jangan copy CCacheX.sys (di luar scope). Jangan copy CCBootClient.exe/CCBootPnP.exe (control-plane dilarang di phase ini).

## 5. Langkah 3 - Daftarkan service di hive SYSTEM (offline)

```powershell
reg load HKLM\CCBOOT_SYS "V:\Windows\System32\config\SYSTEM"
```

Tentukan control set aktif:

```powershell
$cs = (Get-ItemProperty 'HKLM:\CCBOOT_SYS\Select').Current
$csName = 'ControlSet{0:D3}' -f $cs
$svc = "HKLM:\CCBOOT_SYS\$csName\Services\CCBootPNPX"
```

Buat key dan values:

```powershell
New-Item -Path $svc -Force | Out-Null
New-ItemProperty -Path $svc -Name 'Type'         -PropertyType DWord -Value 1 -Force | Out-Null
New-ItemProperty -Path $svc -Name 'Start'        -PropertyType DWord -Value 0 -Force | Out-Null
New-ItemProperty -Path $svc -Name 'ErrorControl' -PropertyType DWord -Value 1 -Force | Out-Null
New-ItemProperty -Path $svc -Name 'ImagePath'    -PropertyType ExpandString -Value '\SystemRoot\System32\drivers\CCBootPNPX.sys' -Force | Out-Null
New-ItemProperty -Path $svc -Name 'Group'        -PropertyType String -Value 'Boot Bus Extender' -Force | Out-Null
```

| Value | Tipe | Nilai | Alasan |
|---|---|---|---|
| Type | DWORD | 1 | kernel driver |
| Start | DWORD | 0 | boot-start; driver memanggil IoRegisterBootDriverReinitialization |
| ErrorControl | DWORD | 1 | normal error handling |
| ImagePath | ExpandString | \SystemRoot\System32\drivers\CCBootPNPX.sys | lokasi driver |
| Group | String | Boot Bus Extender | urutan load setelah bus |

> Catatan: nilai Group/Start di atas adalah pilihan konservatif berdasarkan perilaku driver (boot reinitialization). Nilai persis yang dipakai installer CCBoot belum diverifikasi dari binary.

## 6. Langkah 4 - Verifikasi

```powershell
Get-ItemProperty $svc | Select-Object Type,Start,ErrorControl,ImagePath,Group
```

Pastikan tidak ada key `StartOverride` yang menonaktifkan service:

```powershell
if (Test-Path "$svc\StartOverride") { Get-ItemProperty "$svc\StartOverride" }
```

## 7. Langkah 5 - Unload dan detach

```powershell
[gc]::Collect()
reg unload HKLM\CCBOOT_SYS
```

```text
diskpart
select vdisk file="C:\vhd\w11.vhd"
detach vdisk
exit
```

## 8. Larangan tegas (Phase 3)

- JANGAN import `CCBootPnP_config.reg` ke image. Nilai CCBootPnP dibiarkan diisi oleh driver/komponen CCBoot sendiri.
- JANGAN install `CCBootClient.exe`, `CCBootPnP.exe`, atau service control-plane.
- JANGAN menulis `HKLM\SYSTEM\CCBootPnP` dari script ini.
- JANGAN memodifikasi `Enum\PCI` secara manual.

## 9. Rollback

1. Mount VHD seperti Langkah 1.
2. Hapus `V:\Windows\System32\drivers\CCBootPNPX.sys`.
3. `reg load`, hapus key `...\Services\CCBootPNPX`, `reg unload`.
4. Detach VHD.
5. Atau restore dari backup VHD.

## 10. Acceptance Phase 3

- [ ] File driver ada di `System32\drivers` dan versi benar.
- [ ] Service key ada di control set aktif dengan Start=0 dan Type=1.
- [ ] Tidak ada helper/control-plane yang terpasang.
- [ ] VHD bisa di-attach/detach tanpa error.
- [ ] Boot pertama: driver termuat (cek log/registry `CCBootPnP_Debug` bila ada).

## 11. Yang belum pasti

- Nilai `Group`/`Start` persis dari installer CCBoot (belum diverifikasi).
- Apakah driver membutuhkan value awal `CCBootPnP` untuk mengaktifkan gate identitas (Phase 0 belum membuktikan).
- Perilaku bila driver unsigned/modified (DSE akan menolak; gunakan file asli bertanda tangan).

