# iSCSI Performance Optimization Plan: Target 60 MB/s Read/Write

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Achieve stable, sustained 60 MB/s iSCSI Read and 60 MB/s iSCSI Write throughput (~480–700 Mbps network link) for diskless Windows 10/11 clients without drop or oscillation.

**Architecture:** 
1. Zero-Copy Data-In pipeline from storage/cache into TCP writer channel.
2. NTFS Container Preallocation for writeback storage to eliminate cluster allocation and `$MFT` metadata locks.
3. Vectored Non-Allocating Network Transmission (`write_all` on slice components without dynamic `pack_iov` vector allocation).
4. Socket Buffer Window Scaling (2 MB send/receive buffers) to saturate Gigabit/2.5G links.

**Tech Stack:** Rust (Tokio async runtime, parking_lot, socket2, std::os::windows::fs::FileExt, DashMap).

**Spec:** [`implementation_plan.md`](file:///C:/Users/lannnn/.gemini/antigravity-ide/brain/8158bdc7-7ea5-4479-9060-e42b714cd360/implementation_plan.md)

## Global Constraints
- Must maintain 100% compliance with RFC 3720 (iSCSI) and SCSI Block Commands (SBC-3).
- Must preserve multi-client isolation and diskless clean rollback on disconnect.
- No third-party kernel drivers or custom signing required.

---

### Task 1: Zero-Copy Data-In Pipeline & Vectored Socket Transmission

**Files:**
- Modify: `src/session/pdu_io.rs:1-135`
- Modify: `src/session/pdu_io.rs:210-245`
- Modify: `src/session/scsi_handler.rs:140-160`

**Interfaces:**
- `send_scsi_data_in(itt: u32, data: Vec<u8>, status: u8, expected_len: u32)` consumes `Vec<u8>` directly by value instead of borrowing `&[u8]` and allocating `to_vec()`.
- `write_message` writes BHS, data slice, padding, and response directly without `pack_iov` heap allocations.

- [ ] **Step 1: Update `send_scsi_data_in` signature and call sites**
Change `send_scsi_data_in` in `src/session/pdu_io.rs` to take `data: Vec<u8>` by value. Update callers in `src/session/scsi_handler.rs` to pass `buf` by value.

- [ ] **Step 2: Optimize `write_message` for DataIn**
Eliminate `pack_iov` in `write_message` by sending `bhs`, `data slice`, `padding`, and optional `resp` directly using sequential `write_all` calls on `write_half`.

- [ ] **Step 3: Run `cargo check` to verify types and lifetimes**
Run: `cargo check`
Expected: Success with zero errors.

---

### Task 2: Writeback Container Preallocation (NTFS Metadata Lock Removal)

**Files:**
- Modify: `src/writeback_gamedisk.rs:105-155`
- Modify: `src/writeback_gamedisk.rs:245-285`

**Interfaces:**
- `ClientCache::new`: Preallocate `.bin` container using `file_write_handle.set_len(...)` to prevent continuous NTFS cluster allocation and $MFT metadata locks during client writes.
- `write_stream`: Keep track of preallocated capacity in chunks of 512 MB to ensure writes are always within pre-allocated physical sectors.

- [ ] **Step 1: Implement preallocation in `ClientCache::new`**
Add initial reservation logic in `src/writeback_gamedisk.rs` so newly created `.bin` files have contiguous disk space reserved upfront.

- [ ] **Step 2: Optimize dynamic extension in `write_stream`**
Ensure that when write offset approaches container limit, `set_len` expands in 512 MB increments instead of 64 KB increments.

- [ ] **Step 3: Run `cargo check` and compile release build**
Run: `cargo check`
Expected: Success with zero errors.

---

### Task 3: Socket Buffer Window Scaling & Keep-Alive Tuning

**Files:**
- Modify: `src/server.rs:70-90`

**Interfaces:**
- `set_send_buffer_size(2 * 1024 * 1024)`
- `set_recv_buffer_size(2 * 1024 * 1024)`

- [ ] **Step 1: Update socket buffer configuration**
Set `SO_SNDBUF` and `SO_RCVBUF` to 2 MB (2,097,152 bytes) in `src/server.rs` on accepted TCP streams.

- [ ] **Step 2: Compile release executable**
Run: `cargo build --release`
Expected: `target/release/rust-iscsi-server.exe` successfully generated.

---

### Task 4: End-to-End Benchmark & Verification

**Files:**
- Test target: Live Windows Diskless Client over iSCSI.

- [ ] **Step 1: Launch release server**
Run: `cargo run --release`

- [ ] **Step 2: Run sequential 1 GB Read and Write test on Windows Client**
Execute CrystalDiskMark / large file transfer on client machine and observe sustained throughput in Dashboard.

- [ ] **Step 3: Verify performance criteria**
Confirm: Read $\ge$ 60 MB/s, Write $\ge$ 60 MB/s, no packet drops or session disconnects.
