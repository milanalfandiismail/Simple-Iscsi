# Third-Party Licenses (PXE Bootloader)

Folder ini (`pxe/`) berisi binary bootloader open source pihak ketiga untuk mendukung kebutuhan UEFI PXE Network Boot:

### 1. iPXE
- **Lisensi**: GNU General Public License v2 (GPLv2)
- **Hak Cipta**: Copyright © Michael Brown and contributors
- **Sumber Kode**: [https://github.com/ipxe/ipxe](https://github.com/ipxe/ipxe) (Resmi: [ipxe.org](https://ipxe.org))
- **Teks Lisensi**: Lihat file [`COPYING.GPLv2`](./COPYING.GPLv2)

### 2. UEFI Shim
- **Lisensi**: BSD 2-Clause License
- **Hak Cipta**: Copyright © Red Hat, Inc. and contributors
- **Sumber Kode**: [https://github.com/rhboot/shim](https://github.com/rhboot/shim)

---

### Catatan Lisensi Simple-Iscsi
Aplikasi server Simple-Iscsi dan bootloader iPXE/Shim berinteraksi secara independen murni melalui protokol jaringan standar (DHCP, TFTP, dan iSCSI) tanpa linking pustaka langsung (*mere aggregation* sesuai Bagian 2 GPLv2). Seluruh kode sumber Simple-Iscsi tetap berlisensi **MIT License**.
