# Rencana Dokumentasi Lengkap Protokol & Arsitektur Simple-Iscsi dari Scratch

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Membuat dokumentasi teknis komprehensif, mendalam, dan terstruktur dari *scratch* mengenai seluruh arsitektur sistem Simple-Iscsi (Netboot PXE/DHCP/TFTP/iPXE, Protokol iSCSI RFC 7143, SCSI Layer SPC-4/SBC-3, Writeback Cache Engine 128MB, dan ACPI iBFT Boot Helper).

**Architecture:** Dokumentasi akan disusun secara modular dan disimpan di folder `docs/architecture/` serta satu master documentation file `docs/architecture/README.md` dengan diagram alur (Mermaid), tabel PDU header bit-by-bit, trace sequence diagram, dan referensi implementasi kode Rust & C++.

**Tech Stack:** Markdown, Mermaid Diagrams, RFC 7143 (iSCSI), SCSI SBC-3 / SPC-4, RFC 2131 / 2132 (DHCP), RFC 4173 (iSCSI Boot URI), UEFI / ACPI iBFT Specification.

---

## Global Constraints

- Bahasa utama dokumentasi adalah **Bahasa Indonesia** yang teknis, presisi, runtut, dan mudah dipahami.
- Menyertakan referensi file kode sumber yang relevan dengan tautan markdown github (`file:///...`).
- Menyediakan diagram Mermaid untuk setiap tahapan (Handshake DHCP/iPXE, State Machine Login iSCSI, Data-Out/In FFP Pipeline, Writeback Cache Flow, dan iBFT ACPI Flow).
- Menyertakan rincian struktur byte PDU BHS/AHS, OpCode, dan bit flag secara eksplisit.

---

### Task 1: Dokumentasi Arsitektur Global & Network Booting (PXE, DHCP, TFTP, iPXE)

**Files:**
- Create: `docs/architecture/01-netboot-dhcp-ipxe.md`
- Source Reference: `src/netboot/dhcp.rs`, `src/netboot/dhcp_packet.rs`, `src/netboot/tftp.rs`, `pxe/sb-custom/autoexec.ipxe`

**Interfaces:**
- Menjelaskan seluruh alur dari PC Client mati $\rightarrow$ Power On $\rightarrow$ UEFI PXE ROM $\rightarrow$ DHCP Handshake $\rightarrow$ TFTP Chainload $\rightarrow$ iPXE SANHOOK & SANBOOT.
- Menjelaskan arti dan fungsi seluruh DHCP Option:
  - Option 1 (Subnet Mask), Option 3 (Router/Gateway), Option 6 (DNS)
  - Option 17 (Root-Path RFC 4173: `iscsi:<server>::<port>:0:<iqn>`)
  - Option 51, 58, 59 (Lease Time, T1, T2)
  - Option 66 (Next Server), Option 67 (Bootfile)
  - Option 168 (iSCSI Server IP), Option 169 (iSCSI Boot Target IQN), Option 170 (GameDisk Target URI)
  - Option 175 (iPXE Encapsulated Options)
- Menjelaskan perbedaan Legacy BIOS vs UEFI iPXE, dan alasan teknis penggunaan `--drive 0x81` untuk GameDisk dan `--drive 0x80` untuk Boot Disk OS.

- [ ] **Step 1: Tulis dokumen `docs/architecture/01-netboot-dhcp-ipxe.md` lengkap dengan Mermaid sequence diagram.**
- [ ] **Step 2: Validasi integritas tautan kode dan diagram.**
- [ ] **Step 3: Commit dokumentasi Task 1.**

---

### Task 2: Dokumentasi Protokol iSCSI (RFC 7143), BHS/AHS Structure & State Machine

**Files:**
- Create: `docs/architecture/02-iscsi-protocol-rfc7143.md`
- Source Reference: `src/pdu/mod.rs`, `src/pdu/builder.rs`, `src/pdu/parser.rs`, `src/session/login.rs`, `src/session/pdu_io.rs`

**Interfaces:**
- Menjelaskan struktur 48-byte Basic Header Segment (BHS) dan Additional Header Segment (AHS).
- Menjelaskan seluruh OpCode penting:
  - Initiator -> Target: `0x00` (NOP-Out), `0x01` (SCSI Command), `0x02` (TMF Request), `0x03` (Login Request), `0x04` (Text Request), `0x05` (SCSI Data-Out), `0x06` (Logout Request).
  - Target -> Initiator: `0x20` (NOP-In), `0x21` (SCSI Response), `0x22` (TMF Response), `0x23` (Login Response), `0x24` (Text Response), `0x25` (SCSI Data-In), `0x26` (Logout Response), `0x31` (R2T), `0x3F` (Reject).
- Menjelaskan penomoran paket dan sinkronisasi: `ISID`, `TSIH`, `CmdSN`, `ExpCmdSN`, `MaxCmdSN`, `StatSN`, `ExpStatSN`, `DataSN`, `ExpDataSN`, `ITT`, dan `TTT` (`0xFFFFFFFF` rule).
- Menjelaskan Fase Login (Security Negotiation Stage 0 $\rightarrow$ Operational Parameter Negotiation Stage 1 $\rightarrow$ Full Feature Phase Stage 3).
- Menjelaskan parameter negosiasi: `MaxRecvDataSegmentLength` (4MB), `FirstBurstLength` (2MB), `MaxBurstLength` (2MB), `ImmediateData`, `InitialR2T`, `MaxOutstandingR2T` (16), `MaxConnections` (1), `DataPDUInOrder`, `DataSequenceInOrder`, dll.

- [ ] **Step 1: Tulis dokumen `docs/architecture/02-iscsi-protocol-rfc7143.md` lengkap dengan tabel BHS dan diagram Login State Machine.**
- [ ] **Step 2: Validasi akurasi terhadap RFC 7143 dan implementasi Rust.**
- [ ] **Step 3: Commit dokumentasi Task 2.**

---

### Task 3: Dokumentasi SCSI Emulation Layer (SPC-4 & SBC-3) & Diskless Compatibility

**Files:**
- Create: `docs/architecture/03-scsi-layer-spc4-sbc3.md`
- Source Reference: `src/scsi_gamedisk.rs`, `src/scsi_imagedisk.rs`, `src/pdu/imagedisk.rs`

**Interfaces:**
- Menjelaskan bagaimana perintah SCSI dieksekusi di atas iSCSI PDU:
  - **Standard INQUIRY (`0x12`):** Byte 7 `CmdQue = 1` (Tagged Command Queuing / TCQ / NCQ) untuk membuka antrean paralel Windows Storport Queue Depth (QD 32-64).
  - **VPD Pages (Vital Product Data):**
    - `0x00`: Supported VPD Pages list
    - `0x80`: Unit Serial Number
    - `0x83`: Device Identification (NAA IEEE Descriptor)
    - `0xB0`: Block Limits (Optimal Transfer Granularity 4 KB / 8 sektor, Maximum Transfer Length 4 MB / 8192 sektor, Optimal Transfer Length 1 MB, UNMAP / TRIM limits)
    - `0xB1`: Block Device Characteristics (Non-rotating media = SSD 0x0001)
    - `0xB2`: Thin Provisioning
  - **READ CAPACITY 10 (`0x25`) & 16 (`0x9E`):** Perhitungan total LBA dan ukuran sektor logis 512-byte (512e).
  - **READ 10 (`0x28`) & 16 (`0x88`):** Fast-path RAM Cache hit vs Disk read vs Fallback.
  - **WRITE 10 (`0x2A`) & 16 (`0x8A`):** Immediate Data processing, R2T generation, dan Data-Out Slicing berbasis `Buffer Offset`.
  - **REPORT LUNS (`0xA0`):** Arsitektur Multi-LUN GameDisk (LUN 0 s/d LUN 9 dalam 1 Target IQN).
  - **MODE SENSE (`0x1A`/`0x5A`), START STOP UNIT (`0x1B`), TEST UNIT READY (`0x00`), VERIFY (`0x2F`).**

- [ ] **Step 1: Tulis dokumen `docs/architecture/03-scsi-layer-spc4-sbc3.md` lengkap dengan rincian byte layout SBC-3.**
- [ ] **Step 2: Validasi akurasi fungsi SCSI handler.**
- [ ] **Step 3: Commit dokumentasi Task 3.**

---

### Task 4: Dokumentasi Storage Backend & Writeback Cache Engine (128MB Pre-alloc & Auto-grow)

**Files:**
- Create: `docs/architecture/04-storage-and-writeback-engine.md`
- Source Reference: `src/backend.rs`, `src/vhd.rs`, `src/writeback_gamedisk.rs`, `src/writeback_super.rs`, `src/writeback_imagedisk.rs`

**Interfaces:**
- Menjelaskan tipe storage backend:
  - Raw Physical Drive (`\\.\PhysicalDriveX`) dengan `FILE_FLAG_NO_BUFFERING` / direct sector I/O.
  - Dynamic VHD (Type 3) & Differencing VHD (Type 4) dengan Block Allocation Table (BAT).
- Menjelaskan Writeback Cache Engine:
  - Dual-Layer Caching: DashMap RAM Cache (latensi < 0.001 ms) + Background Worker Sync Channel.
  - Disk Cache (`.bin`) + Map (`.map`).
  - Pre-Alokasi Awal **128 MB** (`target_alloc`) untuk mencegah fragmentasi NTFS dan syscall `ExtendFile` saat Windows boot.
  - Auto-Grow dinamis hingga `max_cache_per_client_gb`.
  - Eviction strategy saat storage mendekati batas.
  - Isolasi Super Client (write langsung ke VHD differencing / persistent) vs Normal Client (writeback terisolasi per-IP / auto-discard on logout).

- [ ] **Step 1: Tulis dokumen `docs/architecture/04-storage-and-writeback-engine.md` lengkap dengan diagram alur I/O caching.**
- [ ] **Step 2: Validasi alur writeback.**
- [ ] **Step 3: Commit dokumentasi Task 4.**

---

### Task 5: Dokumentasi Boot Helper Windows Client (ACPI iBFT & IP Cleaner)

**Files:**
- Create: `docs/architecture/05-windows-boot-helper-ibft.md`
- Source Reference: `helper/helper.cpp`, `helper/test_ip_cleaner.cpp`

**Interfaces:**
- Menjelaskan cara kerja driverless `helper.exe`:
  - **Parsing ACPI iBFT (iSCSI Boot Firmware Table):**
    - Membaca tabel firmware `GetSystemFirmwareTable('ACPI', 'TFBI')` atau pencarian memory ACPI / Registry.
    - Ekstraksi informasi client IP, Subnet Mask, Gateway, Primary/Secondary DNS, Hostname, Initiator IQN, dan Target IQN secara otomatis tanpa perlu DHCP client di Windows.
  - **Deep IP Cleaner:**
    - Deteksi adapter jaringan virtual / non-aktif (ghost network adapters).
    - Membersihkan registrasi IP ganda di `HKLM\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces`.
  - **Injeksi IP Statis Instan:**
    - Menetapkan IP dari iBFT langsung ke adapter fisik aktif agar Windows tidak mengalami pause 30-60 detik menunggu DHCP Windows.
  - **Penyesuaian Registry iSCSI Windows:**
    - `MaxTransferLength = 262144` dan `MaxBurstLength = 2097152` pada service `msiscsi`.

- [ ] **Step 1: Tulis dokumen `docs/architecture/05-windows-boot-helper-ibft.md`.**
- [ ] **Step 2: Validasi struktur iBFT parser dan registry cleaner.**
- [ ] **Step 3: Commit dokumentasi Task 5.**

---

### Task 6: Master Summary & Master Index Document (`README.md`)

**Files:**
- Create: `docs/architecture/README.md`
- Modify: `README.md`

**Interfaces:**
- Menggabungkan seluruh modul ke dalam Master Architecture Guide.
- Menyediakan diagram End-to-End lifecycle (dari PC power-on hingga 900+ Mbps disk benchmark di Windows).
- Menyediakan panduan troubleshooting cepat jika ada kendala performa atau boot error.

- [ ] **Step 1: Tulis `docs/architecture/README.md` sebagai gerbang utama dokumentasi.**
- [ ] **Step 2: Update `README.md` utama di root project untuk menautkan ke `docs/architecture/`.**
- [ ] **Step 3: Commit master documentation dan push ke `origin main`.**
