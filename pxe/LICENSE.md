# Third-Party Licenses (PXE Bootloader)

Folder ini (`pxe/`) berisi binary bootloader open source resmi yang diunduh langsung dari [GitHub Release iPXE](https://github.com/ipxe/ipxe/releases):

### 1. iPXE Bootloader (`ipxe.efi`, `ipxe-full.efi`)
- **Sumber Rilis**: [https://github.com/ipxe/ipxe/releases](https://github.com/ipxe/ipxe/releases)
- **Pemilik Hak Cipta**: Copyright © Michael Brown and contributors
- **Lisensi**: GNU General Public License v2 (GPLv2)
- **Teks Lisensi**: Lihat file [`COPYING.GPLv2`](./COPYING.GPLv2)

### 2. UEFI Secure Boot Shim (`ipxe-shim.efi`)
- **Sumber Rilis**: Diunduh dari rilis resmi [iPXE Releases](https://github.com/ipxe/ipxe/releases)
- **Basis Kode Hulu (Upstream)**: rhboot/shim ([https://github.com/rhboot/shim](https://github.com/rhboot/shim))
- **Pemilik Hak Cipta Asli**: Copyright © Red Hat, Inc. and contributors
- **Lisensi**: BSD 2-Clause License

---

### Catatan Lisensi Simple-Iscsi (*Mere Aggregation*)
Server Simple-Iscsi (Rust) bertindak sebagai penyedia layanan independen (DHCP, TFTP, dan iSCSI target daemon). Binary bootloader dieksekusi di sisi klien firmware UEFI dan hanya berinteraksi dengan server Simple-Iscsi melalui protokol jaringan standar tanpa melakukan linking pustaka langsung (*mere aggregation* sesuai Bagian 2 GPLv2). Seluruh kode sumber Simple-Iscsi tetap berlisensi **MIT License**.
