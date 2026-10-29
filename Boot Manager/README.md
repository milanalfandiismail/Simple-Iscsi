# Panduan Boot Manager — `bootmgfw.efi` / `bootx64.efi`

Kumpulan panduan teknis dan referensi Windows Boot Manager yang telah teruji untuk SAN boot iSCSI di Simple-Iscsi.

---

## ⚠️ Kebijakan Lisensi & Anti-Redistribusi (*Bring Your Own Boot Manager*)

File binary `bootmgfw.efi` dan `bootx64.efi` adalah hak cipta dan karya berpemilik (*proprietary software*) milik **Microsoft Corporation**.

Sesuai ketentuan Lisensi Pengguna Akhir (EULA) Microsoft dan etika open source:
- Repositori publik Simple-Iscsi **TIDAK MENDISTRIBUSIKAN file binary biner Windows Boot Manager**.
- Pengguna wajib mengambil/mengekstrak file tersebut secara mandiri dari media instalasi resmi Windows milik masing-masing (**Bring Your Own Boot Manager**).
- Folder ini disediakan sebagai **dokumentasi hasil riset, referensi hash SHA-256, dan panduan ekstraksi otomatis**.

---

## 📊 Daftar Versi & Hasil Uji Performa

Berikut adalah hasil uji laboratorium Simple-Iscsi terhadap performa bootloader Windows saat memuat kernel melalui jaringan iSCSI:

| Versi / Rilis | Nomor Build Windows | Ukuran File | SHA-256 (16 Karakter Awal) | Status SAN Boot iSCSI |
| :--- | :--- | :--- | :--- | :--- |
| **Windows 11 23H2** | `10.0.22621.2792` | ~1.692.144 bytes | `A20B359849CD...` | ✅ **Sangat Cepat** (Rekomendasi Utama) |
| **Windows 11 25H2** | `10.0.28000.352`  | ~3.086.728 bytes | `B6F5FF8A5F60...` | ✅ **Cepat & Stabil** |
| **Windows 11 26H2** | `10.0.28000.367`  | ~3.087.200 bytes | `25D8869E3C79...` | ❌ **Lambat** (Menunggu handshake ±7 menit) |

> **Rekomendasi Operasional:**
> Urutan paling stabil dan cepat untuk SAN boot: **`23H2`** → **`25H2`**. Hindari build `26H2` pada partisi boot EFI kecuali untuk pengujian atau rollback.

---

## 📥 Panduan Ekstraksi Mandiri dari ISO Windows Resmi

Anda dapat mengekstrak file `bootmgfw.efi` / `bootx64.efi` secara legal dari file ISO resmi Windows yang Anda miliki:

### Metode 1: Ekstraksi Langsung dari File ISO Windows (Paling Praktis)

File ISO Windows resmi (seperti ISO Windows 11 23H2 dari situs Microsoft) sudah menyertakan bootloader UEFI standar di foldernya.

Jalankan perintah berikut di PowerShell (sebagai Administrator):

```powershell
# 1. Tentukan path file ISO Windows Anda
$isoPath = "D:\ISO\Win11_23H2_Indonesian_x64.iso"
$targetFolder = "C:\Project GIT\Simple-Iscsi\Boot Manager\23H2"

# 2. Mount ISO Windows
$mount = Mount-DiskImage -ImagePath $isoPath -PassThru
$drive = ($mount | Get-Volume).DriveLetter + ":"

# 3. Buat folder target lokal jika belum ada
New-Item -ItemType Directory -Force -Path $targetFolder | Out-Null

# 4. Salin bootloader UEFI dari ISO
# File bootx64.efi di folder efi\boot pada ISO adalah boot manager resmi Microsoft
Copy-Item "$drive\efi\boot\bootx64.efi" "$targetFolder\bootx64.efi" -Force

# bootmgfw.efi memiliki isi biner yang identik dengan bootx64.efi
Copy-Item "$drive\efi\boot\bootx64.efi" "$targetFolder\bootmgfw.efi" -Force

# 5. Lepas mount ISO
Dismount-DiskImage -ImagePath $isoPath

Write-Host "Ekstraksi bootloader berhasil disimpan di: $targetFolder" -ForegroundColor Green
```

---

### Metode 2: Ekstraksi dari Partisi EFI (ESP) Komputer Windows Fisik

Jika Anda sudah memiliki PC dengan Windows 11 23H2 atau 25H2 yang terpasang:

```powershell
# 1. Mount partisi EFI sistem ke drive letter S: sementara
mountvol S: /s

# 2. Buat folder target lokal
$targetFolder = "C:\Project GIT\Simple-Iscsi\Boot Manager\23H2"
New-Item -ItemType Directory -Force -Path $targetFolder | Out-Null

# 3. Salin file boot manager asli dari partisi ESP
Copy-Item "S:\EFI\Microsoft\Boot\bootmgfw.efi" "$targetFolder\bootmgfw.efi" -Force
Copy-Item "S:\EFI\Boot\bootx64.efi"             "$targetFolder\bootx64.efi" -Force

# 4. Lepas drive letter partisi EFI
mountvol S: /d

Write-Host "File boot manager berhasil diambil dari sistem lokal!" -ForegroundColor Green
```

---

### Metode 3: Ekstraksi Menggunakan 7-Zip dari `sources\boot.wim`

Jika ingin mengambil versi spesifik dari dalam image instalasi:
1. Buka file ISO Windows menggunakan **7-Zip**.
2. Masuk ke folder `sources\` lalu buka file `boot.wim`.
3. Buka folder `1\Windows\Boot\EFI\`.
4. Ekstrak file `bootmgfw.efi` ke folder versi yang diinginkan (misal `Boot Manager\23H2\`).
5. Buat duplikat file tersebut dengan nama `bootx64.efi` di folder yang sama.

---

## 🚀 Panduan Memasang Boot Manager ke Image VHD Client

Setelah file boot manager diekstrak ke folder lokal (misal `Boot Manager\23H2\`), pasang file tersebut ke dalam partisi ESP file virtual disk (VHD):

> **PENTING:** Pastikan file VHD sedang **TIDAK DIGUNAKAN** (server Simple-Iscsi dalam keadaan stop dan VHD tidak ter-attach di hypervisor seperti VMware/Hyper-V).

```powershell
# Tentukan path VHD dan versi yang akan dipasang
$vhd   = 'I:\VHD\W11-Master-Image.vhd'
$versi = '23H2'   # Pilihan: '23H2' (sangat cepat) atau '25H2'

# Hentikan proses server jika sedang berjalan
Get-Process rust-iscsi-server -ErrorAction SilentlyContinue | Stop-Process -Force

# Mount VHD tanpa otomatis memberi drive letter ke semua partisi
Mount-DiskImage -ImagePath $vhd -NoDriveLetter

# Temukan nomor disk dan partisi bertipe 'System' (ESP / FAT32)
$d   = (Get-DiskImage -ImagePath $vhd | Get-Disk).Number
$esp = (Get-Partition -DiskNumber $d | Where-Object Type -eq 'System').PartitionNumber

# Beri drive letter Z: sementara ke partisi ESP
Set-Partition -DiskNumber $d -PartitionNumber $esp -NewDriveLetter Z

# Salin file boot manager baru ke partisi ESP
$src = "C:\Project GIT\Simple-Iscsi\Boot Manager\$versi"
Copy-Item "$src\bootmgfw.efi" 'Z:\EFI\Microsoft\Boot\bootmgfw.efi' -Force
Copy-Item "$src\bootx64.efi"  'Z:\EFI\Boot\bootx64.efi'            -Force

# Lepas mount VHD
Dismount-DiskImage -ImagePath $vhd

Write-Host "Boot manager versi $versi berhasil dipasang ke $vhd!" -ForegroundColor Green
```

> **Catatan Penting:**
> File `winload.efi` **TIDAK BOLEH** diganti lintas versi Windows (hanya `bootmgfw.efi` dan `bootx64.efi` yang diganti). Penjelasan lengkap lihat **DOCUMENTATION.md BAB 11**.

---

## 🔍 Verifikasi Setelah Dipasang

Untuk memastikan file boot manager yang aktif di VHD sesuai dengan versi yang diinginkan:

```powershell
# Cek Hash SHA-256
(Get-FileHash 'Z:\EFI\Microsoft\Boot\bootmgfw.efi').Hash

# Cek File Version Metadata
[System.Diagnostics.FileVersionInfo]::GetVersionInfo('Z:\EFI\Microsoft\Boot\bootmgfw.efi').FileVersion
```

Cocokkan nilai hash yang dihasilkan dengan **Tabel Hasil Uji Laboratorium** di atas.
