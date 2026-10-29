# Third-Party Licenses (PXE Bootloader)

Folder ini (`pxe/`) berisi binary bootloader open source resmi yang diunduh langsung dari [GitHub Release iPXE](https://github.com/ipxe/ipxe/releases):

### 1. iPXE Bootloader (`ipxe.efi`, `ipxe-full.efi`)
- **Sumber Rilis**: [https://github.com/ipxe/ipxe/releases](https://github.com/ipxe/ipxe/releases)
- **Pemilik Hak Cipta**: Copyright © Michael Brown and contributors
- **Lisensi**: GNU General Public License v2 (GPLv2)
- **Teks Lisensi Lengkap**: Lihat file [`COPYING.GPLv2`](./COPYING.GPLv2)

### 2. UEFI Secure Boot Shim (`ipxe-shim.efi`)
- **Sumber Rilis**: Diunduh dari rilis resmi [iPXE Releases](https://github.com/ipxe/ipxe/releases)
- **Basis Kode Hulu (Upstream)**: rhboot/shim ([https://github.com/rhboot/shim](https://github.com/rhboot/shim))
- **Pemilik Hak Cipta Asli**: Copyright © Red Hat, Inc. and contributors
- **Lisensi**: BSD 2-Clause License

#### Teks Lisensi BSD 2-Clause:
```text
Copyright 2012 Red Hat, Inc <mjg@redhat.com>

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:

Redistributions of source code must retain the above copyright
notice, this list of conditions and the following disclaimer.

Redistributions in binary form must reproduce the above copyright
notice, this list of conditions and the following disclaimer in the
documentation and/or other materials provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT,
INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS
OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED
AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY
WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
POSSIBILITY OF SUCH DAMAGE.
```

---

### Catatan Lisensi Simple-Iscsi (*Mere Aggregation*)
Server Simple-Iscsi (Rust) bertindak sebagai penyedia layanan independen (DHCP, TFTP, dan iSCSI target daemon). Binary bootloader dieksekusi di sisi klien firmware UEFI dan hanya berinteraksi dengan server Simple-Iscsi melalui protokol jaringan standar tanpa melakukan linking pustaka langsung (*mere aggregation* sesuai Bagian 2 GPLv2). Seluruh kode sumber Simple-Iscsi tetap berlisensi **MIT License**.
