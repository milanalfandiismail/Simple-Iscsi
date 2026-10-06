# Third-Party Licenses & Attributions for PXE Bootloader Binaries

Direktori ini (`pxe/`) berisi binary bootloader open source pihak ketiga yang telah dikompilasi sebelumnya (*pre-compiled binaries*) untuk mendukung kapabilitas Network Booting (PXE / UEFI iSCSI Boot).

Binary-binary ini didistribusikan sesuai dengan lisensi open source masing-masing proyek induknya.

---

## 1. iPXE (`ipxe.efi`, `ipxe-full.efi`, `uefi`)

- **Proyek**: [iPXE - Open Source Boot Firmware](https://ipxe.org/)
- **Repository Sumber**: [https://github.com/ipxe/ipxe](https://github.com/ipxe/ipxe)
- **Pemegang Hak Cipta**: Copyright © Michael Brown <mbrown@fensystems.co.uk> dan para kontributor iPXE.
- **Lisensi**: **GNU General Public License version 2 (GPL-2.0-only / GPL-2.0-or-later)**

### Ketentuan & Ketersediaan Kode Sumber (Source Code Notice)
Sesuai dengan ketentuan Bagian 3 dari Lisensi GNU GPL v2, kode sumber lengkap untuk iPXE tersedia secara bebas dan dapat diperoleh dari repository resmi:

```bash
# Clone repository resmi iPXE
git clone https://github.com/ipxe/ipxe.git

# Membangun binary UEFI x86_64:
cd ipxe/src
make bin-x86_64-efi/ipxe.efi
```

Teks lengkap lisensi GNU GPL v2 disertakan dalam file [`COPYING.GPLv2`](./COPYING.GPLv2) di direktori ini.

---

## 2. UEFI Shim Loader (`ipxe-shim.efi`)

- **Proyek**: [rhboot/shim - UEFI shim first-stage bootloader](https://github.com/rhboot/shim)
- **Repository Sumber**: [https://github.com/rhboot/shim](https://github.com/rhboot/shim)
- **Pemegang Hak Cipta**: Copyright © Red Hat, Inc. and other contributors.
- **Lisensi**: **BSD 2-Clause License**

```text
Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice,
   this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
POSSIBILITY OF SUCH DAMAGE.
```

---

## 3. Skrip Konfigurasi Otomatis (`autoexec.ipxe`)

Skrip `autoexec.ipxe` yang terdapat pada subdirektori ini dibuat secara spesifik sebagai bagian dari integrasi Simple-Iscsi dan didistribusikan di bawah ketentuan **MIT License** Simple-Iscsi (Copyright © 2026 Milan Alfandi Ismail).

---

## 4. Klarifikasi Lisensi Simple-Iscsi (*Mere Aggregation*)

Sesuai dengan Bagian 2 dari Lisensi GNU GPL v2:
> *"In addition, mere aggregation of another work not based on the Program with the Program (or with a work based on the Program) on a volume of a storage or distribution medium does not bring the other work under the scope of this License."*

Aplikasi server **Simple-Iscsi** (`rust-iscsi-server.exe`) dan layanan helper Windows (`helper-svc.exe`) merupakan perangkat lunak independen yang ditulis secara mandiri. Simple-Iscsi **tidak** melakukan linking (baik statis maupun dinamis) dengan kode pustaka iPXE. Interaksi antara Simple-Iscsi dan binary iPXE berlangsung secara murni melalui protokol jaringan standar (DHCP, TFTP, dan iSCSI).

Oleh karena itu, penyertaan binary iPXE dalam repository ini diperlakukan sebagai **agregasi independen (*mere aggregation*)**, sehingga seluruh kode sumber Simple-Iscsi tetap berlisensi **MIT License**.
