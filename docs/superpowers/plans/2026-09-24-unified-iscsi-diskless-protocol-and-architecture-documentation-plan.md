# Rencana Dokumentasi Simple-Iscsi: README.md & DOCUMENTATION.md (All-in-One)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Menyusun dua file dokumentasi utama di repository Simple-Iscsi tanpa ada materi yang terpisah-pisah:
1. `README.md` (Root) $\rightarrow$ Panduan ringkas, simpel, dan profesional untuk pengguna & developer (Quick Start, Konfigurasi, Dev Guide, dan Panduan Kontribusi).
2. `DOCUMENTATION.md` (Root) $\rightarrow$ Dokumentasi master teknis super lengkap dari *scratch* (8 Bab terpadu mencakup Network Booting, Protokol iSCSI RFC 7143, SCSI SPC-4/SBC-3, Writeback Cache 128 MB, ACPI iBFT Boot Helper, dan Analisis Kinerja 900+ Mbps).

**Architecture:** Seluruh detail arsitektur teknis, diagram Mermaid, struktur PDU 48-byte, dan spesifikasi protokol digabungkan secara utuh ke dalam satu file `DOCUMENTATION.md`. Sementara `README.md` difokuskan menjadi gerbang utama pengguna dan kontributor open-source.

**Tech Stack:** Markdown, Mermaid.js Diagrams, iSCSI RFC 7143, SCSI SBC-3 / SPC-4, DHCP RFC 2131/2132, iSCSI Boot URI RFC 4173, UEFI / ACPI iBFT Specification, Rust (Tokio / DashMap), C++ (Win32 Native API).

**Spec:** Audit Protokol & Spesifikasi Arsitektur Diskless Simple-Iscsi.

---

## Global Constraints

- **Tepat 2 File Dokumentasi Utama:**
  - `README.md`: Panduan praktis, bersih, dan ringkas.
  - `DOCUMENTATION.md`: Dokumentasi teknis terpadu dari A sampai Z tanpa file terpisah.
- **Bahasa Indonesia:** Disusun dalam Bahasa Indonesia teknis yang runtut, baku, dan mudah dipahami.
- **Diagram Mermaid Komprehensif:** Menyertakan diagram alur dan sequence diagram (DHCP/TFTP Handshake, iSCSI State Machine, SCSI Command Pipeline, Writeback Cache Flow, dan iBFT ACPI Flow).
- **Rincian Bit & Byte:** Menyertakan tabel BHS 48-byte, matriks OpCode (`0x00` - `0x3F`), dan layout byte SCSI VPD 0xB0.
- **Tautan Kode Sumber:** Menyertakan link ke simbol dan file kode sumber menggunakan format `file:///...`.

---

## Struktur Pembagian Kedua File

### 1. `README.md` (User & Developer Guide)
- **Ringkasan Proyek:** Pengenalan Simple-Iscsi Target Server & fitur unggulan (SANBOOT Windows, Multi-LUN GameDisk, 900+ Mbps wire-speed).
- **Quick Start (Panduan Penggunaan):**
  - Kebutuhan sistem (OS, RAM, NIC).
  - Cara build binary rilis (`cargo build --release`).
  - Cara konfigurasi dasar `config.toml`.
  - Cara menjalankan server.
- **Panduan Pengembang (Developer Guide):**
  - Prasyarat toolchain (Rust, MSVC C++ untuk helper).
  - Struktur folder dan modul kode sumber (`src/netboot/`, `src/pdu/`, `src/session/`, `src/backend.rs`, `helper/`).
  - Cara menjalankan unit test (`cargo test`).
- **Panduan Kontribusi (Contributing Guidelines):**
  - Aturan branching, standar pesan commit (Conventional Commits), dan verifikasi protokol.
- **Tautan Dokumentasi Mendalam:** Link ke `DOCUMENTATION.md`.

---

### 2. `DOCUMENTATION.md` (Deep Technical & Protocol Specification)
Dokumen master yang memuat 8 Bab komprehensif:
1. **BAB 1: Pengenalan & Arsitektur Global (End-to-End System Overview)**
2. **BAB 2: Network Booting (PXE, DHCP Handshake, TFTP Chainloading, & iPXE Scripting)**
3. **BAB 3: Protokol iSCSI (RFC 7143) & Struktur PDU (BHS/AHS, OpCode, Sequence Numbering)**
4. **BAB 4: Siklus Hidup Sesi & State Machine iSCSI (SNS Stage 0 $\rightarrow$ OPNS Stage 1 $\rightarrow$ FFP Stage 3)**
5. **BAB 5: SCSI Emulation Layer (SPC-4 & SBC-3: INQUIRY `CmdQue`, VPD 0xB0, READ/WRITE 10/16, Multi-LUN)**
6. **BAB 6: Storage Backend & Writeback Cache Engine (Dual-Layer RAM + Disk 128 MB Pre-Allocation)**
7. **BAB 7: Windows Client Boot Helper (`helper.exe`: ACPI iBFT Parser & Deep IP Cleaner)**
8. **BAB 8: Analisis Kinerja, Audit Optimasi, & Matriks Troubleshooting (10 MB/s ke 900+ Mbps)**

---

## Rincian Tahapan Rencana Kerja (Task Breakdown)

### Task 1: Audit Kode Sumber & Penyusunan `DOCUMENTATION.md` Lengkap

**Files:**
- Create/Overwrite: `DOCUMENTATION.md`
- Source Reference: `src/netboot/dhcp.rs`, `src/netboot/dhcp_packet.rs`, `src/netboot/tftp.rs`, `pxe/sb-custom/autoexec.ipxe`, `src/pdu/mod.rs`, `src/pdu/builder.rs`, `src/pdu/parser.rs`, `src/session/login.rs`, `src/session/pdu_io.rs`, `src/session/scsi_handler.rs`, `src/scsi_gamedisk.rs`, `src/scsi_imagedisk.rs`, `src/backend.rs`, `src/vhd.rs`, `src/writeback_gamedisk.rs`, `helper/helper.cpp`

**Interfaces & Contents:**
- Menulis seluruh 8 Bab teknis secara mendalam dari scratch dalam satu file master `DOCUMENTATION.md`.
- Menyertakan seluruh diagram Mermaid (End-to-End Architecture, DHCP/TFTP Handshake, iSCSI State Machine, SCSI Read/Write Flow, Dual-Layer Cache, dan iBFT ACPI Flow).
- Menyertakan tabel BHS 48-byte bit-by-bit, tabel OpCode, parameter negosiasi, SCSI VPD Page 0xB0 layout, dan alasan penetapan alokasi awal 128 MB serta penguncian slot `--drive 0x80` & `0x81`.

- [ ] **Step 1: Tulis master file `DOCUMENTATION.md` secara utuh dan terpadu.**
- [ ] **Step 2: Validasi integritas seluruh diagram Mermaid dan referensi kode.**
- [ ] **Step 3: Commit `DOCUMENTATION.md`.**

---

### Task 2: Penyusunan `README.md` Ringkas, Bersih, dan Terstruktur

**Files:**
- Modify: `README.md`
- Source Reference: `config.toml`, `Cargo.toml`, `DOCUMENTATION.md`

**Interfaces & Contents:**
- Menulis ulang `README.md` agar ringkas, menarik, dan fokus pada:
  - Deskripsi Proyek & Fitur Utama.
  - Quick Start Guide (Build, Konfigurasi `config.toml`, Menjalankan Server, Menghubungkan Client).
  - Developer & Build Guide (`cargo build`, `cargo test`, arsitektur folder).
  - Panduan Kontribusi (Branching, Commit convention, PR flow).
  - Tautan tebal ke `DOCUMENTATION.md` untuk spesifikasi protokol mendalam.

- [ ] **Step 1: Update `README.md` dengan struktur yang simpel dan informatif.**
- [ ] **Step 2: Validasi tautan navigasi dan format markdown.**
- [ ] **Step 3: Commit `README.md`.**

---

### Task 3: Pembersihan File Duplikat, Pengujian, & Final Push

**Files:**
- Remove/Cleanup: File-file dokumen sementara di `docs/` jika ada yang duplikat agar hanya ada `README.md` dan `DOCUMENTATION.md` sebagai sumber acuan utama.
- Verify: `cargo test`

- [ ] **Step 1: Bersihkan file dokumentasi parsial yang tidak diperlukan.**
- [ ] **Step 2: Jalankan `cargo test` untuk memastikan codebase tetap 100% green.**
- [ ] **Step 3: Commit final dan push ke `origin main`.**
