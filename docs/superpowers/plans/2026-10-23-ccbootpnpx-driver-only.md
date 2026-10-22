# CCBootPNPX.sys Driver-Only Compatibility Plan

> Tanggal: 2026-10-23
> Status: plan baru (menggantikan 3 plan 2026-10-22 yang sudah dihapus).
> Cakupan: HANYA kompatibilitas driver CCBootPNPX.sys di client.
> Prinsip: pure CCBoot di client; tanpa helper buatan kita; tanpa control plane port 1000; tanpa console/cloud/update CCBoot.

## 1. Tujuan

Client bisa boot dari iSCSI server Simple-Iscsi dengan CCBootPNPX.sys asli, dan driver berhasil:

1. Membaca tabel iBFT (NIC, target IQN, target IP) yang disediakan firmware/iPXE.
2. Memetakan target IQN bersuffix CCBoot ke image/LUN yang benar.
3. Menjaga identitas PCI/NIC dan registry CCBootPnP sesuai perilaku driver.

Definisi selesai: boot 2x berturut-turut menghasilkan instance ID PCI yang sama, CCBootPnP terisi, dan tidak ada BSOD 0x7B.

## 2. Non-goals (tegas di luar scope)

- Helper/service buatan kita di dalam image.
- Daemon TCP port 1000 / protokol servercmd CCBoot.
- CCBootClient control plane, console, cloud, update, lisensi.
- CCacheX dan cache RAM CCBoot.
- Rename manual key Enum PCI dari user-mode.

## 3. Fakta terverifikasi (dari binary)

| Fakta | Bukti |
|---|---|
| Driver version 2.0.0.12, SHA-256 6872982dd28943c242b3c47da84eb82597a659f8e8ca1dd6451906cd03ec51d4 | open_binary |
| Driver TIDAK mem-parsing DHCP option; tidak ada socket/WSK/NDIS | inventory import + string |
| Driver memakai iBFT: string 'Found iBFT NIC %d:' di 0x1400F920D | list_strings |
| Driver mencetak info NIC/DHCP/PCI: '  DHCP = %s' 0x1400F91B9, '  PCI = %02x:%02x.%x' 0x1400F9136 | list_strings |
| Fungsi terkait: 0x140008EA0 (dump NIC+iBFT), 0x140001DF0 (parser tabel firmware), 0x140099030 (jalur EFI-fail), 0x14009C080 (ExGetFirmwareEnvironmentVariable CCBootPciInfo) | call graph + string |
| Registry callback hanya bereaksi pada RegNtPreRenameKey (0x1C); status balik selalu 0; hanya kernel-mode | 0x1400B0DAA, 0x1400B098D |
| Bundle client di repo = versi 3.0; driver yang dianalisis = 2.0.0.12 | VersionInfo file |

## 4. Asumsi yang WAJIB diverifikasi lebih dulu (jangan dipercaya buta)

Plan lama 2026-10-22 mengklaim driver memvalidasi:

1. Suffix IQN ':0XX' dengan target_name[len-4] == ':' dan target_name[len-3] == '0'.
2. Hostname iBFT berformat '<Hostname>(<Number>-<Hostname>-0)'.

Hasil Phase 0: klaim suffix TERVERIFIKASI (minimal ':0' di len-4/len-3); klaim hostname TIDAK DIDUKUNG driver. Lihat docs/ccbootpnpx-ibft-verification.md.

## 5. Phase 0 - Verifikasi driver (BLOCKING, tanpa kode produksi)

- [x] 0.1 SELESAI: parser iBFT = FUN_140001DF0 (ACPI/iBFT via 0x140101000); struktur NIC dibaca FUN_140008EA0. Lihat docs/ccbootpnpx-ibft-verification.md.
- [x] 0.2 SELESAI: TERVERIFIKASI s[len-4]==0x3A (':') dan s[len-3]==0x30 ('0') di FUN_140004520 (0x1400072E0/0x140007441, 0x140007CEE/0x140007E33), FUN_1400130D0 (0x140014131/0x140014228), FUN_140031740 (0x1400329C1/0x140032ACF).
- [x] 0.3 SELESAI: TIDAK ADA validasi format hostname (tidak ada perbandingan '(' / ')' / '-'). Driver hanya menyalin iBFT hostname ke ComputerName/NV Hostname/Hostname (FUN_14000ECC0). Klaim format khusus DIBATALKAN.
- [x] 0.4 SELESAI: target = file .sys (2.0.0.12); aplikasi CCBootClient 3.0 tidak relevan untuk parsing iBFT/suffix. Tidak perlu samakan versi aplikasi.
- [x] 0.5 SELESAI: docs/ccbootpnpx-ibft-verification.md sudah dibuat.

Gate: Phase 1+ tidak boleh mulai sebelum 0.1-0.4 selesai.

## 6. Phase 1 - DHCP + iPXE yang dibaca driver

- [x] 1.1 SELESAI: with_ccboot_suffix() menambahkan ':0XX' ke IQN (dari nomor hostname atau hash MAC). Option 169 dan root-path memakai IQN yang sama.
- [x] 1.2 SELESAI: option 12 selalu diisi; hostname disanitasi (maks 15, alfanumerik/-) dan fallback PC-XX dari hash MAC.
- [x] 1.3 SELESAI: root-path memakai IQN bersuffix yang sama; 168 = IP server; 170 = gamedisk terpisah (tanpa suffix).
- [ ] 1.4 NIC iBFT: verifikasi runtime bahwa NIC boot = NIC entri iBFT (PCI BDF/MAC). Tidak ada perubahan kode server.
- [x] 1.5 SELESAI: pxe/sb-custom/autoexec.ipxe menampilkan Target IQN, Hostname (net0/hostname + opsi 12), dan Root-Path sebelum sanboot.

File kandidat: src/netboot/dhcp.rs, src/netboot/dhcp_packet.rs, pxe/*/autoexec.ipxe.

## 7. Phase 2 - iSCSI target: IQN bersuffix

- [x] 2.1 SELESAI: strip_ccboot_suffix() di src/session/mod.rs; suffix dibuang 4 char sebelum init_child_vhd dan cache naming.
- [x] 2.2 SELESAI: starts_with(prefix) tetap cocok untuk IQN bersuffix; hasil strip dipakai untuk lookup image dan cache.
- [x] 2.3 SELESAI (tidak diubah): LUN 0 tetap boot disk; INQUIRY/serial/read-write tidak tersentuh perubahan ini.
- [ ] 2.4 Verifikasi runtime: login iscsicli dengan IQN bersuffix dan tanpa suffix (belum dijalankan di environment ini).

File kandidat: src/session/login.rs, src/config_manager.rs, src/scsi_imagedisk.rs.

## 8. Phase 3 - Image provisioning pure CCBoot (minimal)

- [x] 3.1 SELESAI: docs/ccbootpnpx-vhd-provisioning.md + scripts/provision-ccbootpnpx.ps1 (copy driver + service key Start=0/Type=1).
- [x] 3.2 SELESAI: script tidak menyentuh CCBootClient/CCBootPnP control-plane.
- [x] 3.3 SELESAI: script tidak mengimpor CCBootPnP_config.reg dan tidak menulis HKLM/SYSTEM/CCBootPnP.
- [x] 3.4 SELESAI: langkah mount/copy/reg load/unload/detach + rollback ada di docs/ccbootpnpx-vhd-provisioning.md.

## 9. Phase 4 - Acceptance

- [ ] 4.1 Boot 2x: instance ID PCI sama (bandingkan Enum/PCI).
- [ ] 4.2 HKLM/SYSTEM/CCBootPnP terisi (PciIdOrg, InstanceIdOrg, ConfigFlags) dan CCBootServerIP masuk akal.
- [ ] 4.3 Identitas PCI/NIC sesuai; tidak ada BSOD 0x7B / freeze.
- [ ] 4.4 Simpan log boot + dump registry sebelum/sesudah.

## 10. Risiko

| Risiko | Mitigasi |
|---|---|
| Klaim suffix/hostname salah | Phase 0 wajib; jangan implementasi sebelum terbukti |
| Versi driver 2.0.0.12 vs client 3.0 tidak cocok | 0.4: samakan paket |
| Driver butuh CCBootServerIP dari jalur CCBoot | uji dengan nilai dari DHCP/server kita; catat perilaku |
| IP/route berubah setelah driver restore | siapkan rollback registry + snapshot VHD |

## 11. Deliverables

1. docs/ccbootpnpx-ibft-verification.md (hasil Phase 0).
2. Patch Phase 1-2 (DHCP/iPXE + iSCSI suffix).
3. Catatan provisioning VHD (Phase 3).
4. Log acceptance (Phase 4).

## 12. Urutan eksekusi

Phase 0 -> 1 -> 2 -> 3 -> 4. Setiap phase berhenti bila gate gagal; tidak ada lompat ke implementasi.

## 13. Hasil Phase 1 (2026-10-23)

- File diubah: src/netboot/dhcp.rs (helper suffix/hostname, option 169 + root-path), pxe/sb-custom/autoexec.ipxe (echo diagnostik).
- Verifikasi: cargo check exit 0; cargo test netboot::dhcp -> 3 passed.
- Catatan: boot masih akan gagal di login iSCSI sampai Phase 2 (strip suffix) selesai.

## 14. Hasil Phase 2 (2026-10-23)

- File diubah: src/session/mod.rs (strip_ccboot_suffix + pemakaian di jalur Windows image).
- Verifikasi: cargo check exit 0; cargo test session::tests -> 2 passed.
- Perilaku: IQN 'iqn.2026-01.com.example:win-w11:0AB' dipetakan ke image 'w11'.
- Catatan: gamedisk (option 170) sengaja tidak di-strip karena tidak dikirim dengan suffix.

## 15. Hasil Phase 3 (2026-10-23)

- Dokumen: docs/ccbootpnpx-vhd-provisioning.md
- Script: scripts/provision-ccbootpnpx.ps1 (belum dijalankan; butuh VHD offline + admin).
- Tidak ada helper, tidak ada control-plane, tidak ada penulisan CCBootPnP.
