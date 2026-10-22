# Verifikasi iBFT dan Suffix IQN - CCBootPNPX.sys

> Tanggal: 2026-10-23
> Target: C:/Project GIT/Simple-Iscsi/CCBootPNPX.sys
> SHA-256: 6872982dd28943c242b3c47da84eb82597a659f8e8ca1dd6451906cd03ec51d4
> Metode: MCP REA + Ghidra 12.1.4 headless (statis).

## 1. Ringkasan hasil

| Klaim | Status | Bukti |
|---|---|---|
| Driver memakai tabel ACPI iBFT | TERVERIFIKASI | MOV EDX,0x54464269 ('iBFT') + MOV ECX,0x41435049 ('ACPI') + CALL 0x140101000 |
| Target IQN harus punya ':' di posisi len-4 | TERVERIFIKASI | 0x140014131, 0x1400072E0, 0x140007CEE, 0x1400329C1 |
| Karakter berikutnya harus '0' (len-3) | TERVERIFIKASI | 0x140014228, 0x140007441, 0x140007E33, 0x140032ACF |
| Dua karakter terakhir wajib digit | BELUM TERVERIFIKASI | tidak ditemukan pemeriksaan isdigit di jalur ini |
| Hostname wajib format <Hostname>(<Number>-<Hostname>-0) | TIDAK DIDUKUNG | tidak ada perbandingan '(' / ')' / '-' di binary |

## 2. Parser iBFT (FUN_140001DF0)

Driver memanggil rutin firmware dengan provider 'ACPI' dan tabel 'iBFT':

```asm
0x140002be5: MOV ECX, 0x41435049      ; 'ACPI'
0x140002bea: MOV EDX, 0x54464269      ; 'iBFT'
0x140002bfe: CALL 0x140101000         ; ambil tabel firmware
0x140003545: SUB EAX, 0x54464269      ; cek signature
0x14000354a: SETNZ AL
```

Signature yang sama muncul 7 kali di FUN_140001DF0 (0x140002BEB, 0x140002E42, 0x140003546, 0x1400041FE, 0x140004288, 0x1400043A3, dan data 0x1400F9213).

## 3. Struktur NIC yang dibaca driver (FUN_140008EA0)

Fungsi dump membaca entri NIC iBFT pada offset berikut (relatif terhadap pointer entri):

| Offset | Isi | Bukti |
|---|---|---|
| +0x04 | index NIC (byte) | MOVZX/byte lalu DbgPrint 'Found iBFT NIC %d' |
| +0x05 | flag/tipe (bitmask, dicek ^0xFB dan ^0xFE) | 0x1400090C6, 0x140009755 |
| +0x18 | string (dialokasikan/dikonversi) | FUN_140011160(lVar13+0x18) |
| +0x28 | string | FUN_140011160(lVar13+0x28) |
| +0x38 | string | FUN_140011160(lVar13+0x38) |
| +0x48 | string (dipakai 'DHCP = %s') | 0x140009958 |
| +0x58 | u16 | 0x140009966 |
| +0x5A..+0x5F | byte (VLAN/flag) | 'VLAN = %04x' |
| +0x60 | u16 PCI BDF (bus/dev/func) | 'PCI = %02x:%02x.%x' 0x140009A66 |
| +0x62 | string hostname | 'Hostname = %s' 0x140009A9C |

Catatan: ini struktur internal hasil olahan, bukan layout mentah iBFT. Layout mentah iBFT ada di FUN_140001DF0.

## 4. Pemeriksaan suffix target IQN (TERVERIFIKASI)

Pola yang sama muncul di 4 tempat. Contoh di FUN_1400130D0:

```asm
0x140014109: MOV RAX, [RBP+0x20]
0x14001410D: MOV RCX, [RAX]
0x140014114: CALL 0x1400F8070        ; strlen
0x14001411D: MOV RCX, RAX           ; RCX = panjang
0x140014124: XOR EDX, EDX
0x140014126: SUB RDX, 4             ; -4
0x14001412A: ADD RCX, RDX           ; len-4
0x14001412D: MOVSX EAX, byte ptr [RAX+RCX]
0x140014131: CMP EAX, 0x3A          ; ':'
0x140014134: SETZ AL
0x140014139: MOV [RBP+0x39], AL
```

```asm
0x140014207: CALL 0x1400F8070        ; strlen
0x14001421B: XOR EDX, EDX
0x14001421D: SUB RDX, 3             ; -3
0x140014221: ADD RCX, RDX
0x140014224: MOVSX EAX, byte ptr [RAX+RCX]
0x140014228: CMP EAX, 0x30          ; '0'
0x14001422B: SETZ AL
0x140014230: MOV [RBP+0x3A], AL
```

Lokasi lengkap:

| Fungsi | Cek ':' (len-4) | Cek '0' (len-3) |
|---|---|---|
| FUN_140004520 | 0x1400072E0 | 0x140007441 |
| FUN_140004520 | 0x140007CEE | 0x140007E33 |
| FUN_1400130D0 | 0x140014131 | 0x140014228 |
| FUN_140031740 | 0x1400329C1 | 0x140032ACF |

Implikasi: target IQN harus berakhir dengan pola ':0' pada posisi len-4 dan len-3, misalnya '...:0AB' atau '...:001'. Dua karakter terakhir belum terbukti wajib digit.

## 5. Hostname (klaim lama TIDAK DIDUKUNG)

- Tidak ada perbandingan karakter '(' (0x28), ')' (0x29), atau '-' (0x2D) di seluruh binary (pencarian byte 83 F8 28 / 80 F8 28 / 3C 28 = 0 hasil).
- Driver hanya menyalin string hostname iBFT ke:
  - HKLM/SYSTEM/CurrentControlSet/Control/ComputerName/ComputerName
  - HKLM/SYSTEM/CurrentControlSet/Services/Tcpip/Parameters (NV Hostname)
  - value Hostname
  (FUN_14000ECC0, string 'NV Hostname' 0x1400FAD7E, 'ComputerName' 0x1400FADB0)
- Maka format '<Hostname>(<Number>-<Hostname>-0)' BUKAN validasi driver. Jangan implementasikan format aneh atas nama driver.

## 6. Versi

- Driver: 2.0.0.12.
- Bundle CCBootClient di repo: 3.0 (aplikasi user-mode).
- Untuk parsing iBFT/suffix, versi aplikasi client tidak relevan. Yang relevan adalah file .sys.

## 7. Konsekuensi untuk server Simple-Iscsi

1. Option 169 / root-path harus mengirim target IQN yang berakhiran ':0XX' (minimal ':0' di posisi len-4/len-3).
2. Server harus menerima IQN bersuffix dan memetakan ke image yang sama (strip suffix).
3. iBFT NIC entry harus menunjuk NIC yang benar (PCI BDF cocok).
4. Hostname: gunakan hostname valid biasa; tidak perlu format khusus.

## 8. Bukti / Evidence ID

| Evidence | Operasi |
|---|---|
| ev_88e066fffb1e9e79dd4b53bf26cbe1114a05c86a2d4610ad6c1477470b8e1ce1 | pseudo FUN_140001DF0 (parser iBFT) |
| ev_bf6ede481db20357a9b9f3354fadf9ace5392fbb0475344390f486e0fe31d0dc | pseudo FUN_140008EA0 (dump NIC iBFT) |
| ev_532db3fe74305468fcdf92d43697adb2aa2ef48ef2707e589c2c4427476f85e9 | pseudo FUN_14000ECC0 (hostname/ComputerName) |

## 9. Belum pasti

- Apakah dua karakter terakhir wajib digit.
- Apakah ada validasi panjang minimum (len >= 4) sebelum indeks; saat ini terlihat langsung len-4/len-3.
- Perilaku bila suffix tidak cocok (fallback/abaikan) belum ditelusuri penuh.

