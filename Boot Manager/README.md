# Boot Manager — arsip `bootmgfw.efi` / `bootx64.efi`

Kumpulan Windows Boot Manager yang sudah teruji untuk SAN boot iSCSI di Simple-Iscsi.

Setiap folder berisi **tepat dua file** dan keduanya **isinya identik**:

```
<VERSI>\
├─ bootmgfw.efi    → dipasang ke  \EFI\Microsoft\Boot\bootmgfw.efi
└─ bootx64.efi     → dipasang ke  \EFI\Boot\bootx64.efi
```

## Daftar versi

| Folder | Versi | Ukuran | SHA-256 (16 awal) | Status boot |
|---|---|---|---|---|
| `23H2\` | 10.0.**22621.2792** | 1.692.144 | `A20B359849CD` | ✅ cepat |
| `25H2\` | 10.0.**28000.352** | 3.086.728 | `B6F5FF8A5F60` | ✅ cepat |
| `26H2\` | 10.0.**28000.367** | 3.087.200 | `25D8869E3C79` | ❌ **lambat ±7 menit** |

Urutan aman (dari paling cepat): `23H2` → `25H2`. Hindari `26H2` kecuali untuk rollback ke kondisi pabrik.

## Cara pakai

VHD harus dalam keadaan **tidak dipakai** (server berhenti, tidak ter-attach di VMware).

```powershell
$vhd   = 'I:\VHD\W11-26H2-26300.9550-lannn-0.0.1 - Copy.vhd'
$versi = '25H2'          # atau '23H2'

Get-Process rust-iscsi-server -ErrorAction SilentlyContinue | Stop-Process -Force
Mount-DiskImage -ImagePath $vhd -NoDriveLetter

$d   = (Get-DiskImage -ImagePath $vhd | Get-Disk).Number
$esp = (Get-Partition -DiskNumber $d | Where-Object Type -eq 'System').PartitionNumber
Set-Partition -DiskNumber $d -PartitionNumber $esp -NewDriveLetter Z

$src = "C:\Project GIT\Simple-Iscsi\Boot Manager\$versi"
Copy-Item "$src\bootmgfw.efi" 'Z:\EFI\Microsoft\Boot\bootmgfw.efi' -Force
Copy-Item "$src\bootx64.efi"  'Z:\EFI\Boot\bootx64.efi'            -Force

Dismount-DiskImage -ImagePath $vhd
```

`winload.efi` **tidak boleh** ikut diganti lintas feature update — lihat `DOCUMENTATION.md` BAB 11.

## Verifikasi setelah dipasang

```powershell
(Get-FileHash 'Z:\EFI\Microsoft\Boot\bootmgfw.efi').Hash
[System.Diagnostics.FileVersionInfo]::GetVersionInfo('Z:\EFI\Microsoft\Boot\bootmgfw.efi').FileVersion
```

Versi harus sama dengan tabel di atas. Kalau `FileVersion` menampilkan angka lain setelah
file ditimpa, percayai **hash**-nya (metadata versi kadang masih di-cache Windows).

## Catatan

- `23H2` di sini build `22621.2792` (keluarga 22H2/23H2). Windows 26H2 tetap mau boot dengannya.
- `bootmgfw.efi` dan `bootx64.efi` di setiap folder **byte-identik** — memang file yang sama,
  hanya beda nama sesuai lokasi di ESP.
- Semua file **asli Microsoft, masih ditandatangani** — jangan pernah di-patch atau di-re-sign.
