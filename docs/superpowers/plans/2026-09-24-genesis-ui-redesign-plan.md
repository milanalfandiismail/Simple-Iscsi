# Complete UI Redesign Based on Genesis Design System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the entire Web UI (`index.html`, `index.js`, `input.css`) with Tailwind CSS adhering to `genesis-DESIGN.md`, ensuring flawless responsiveness (`sm`, `md`, `lg`, `xl`, `2xl`) without horizontal overflow, modern dark/light mode, and retaining the application branding **Simple Iscsi**.

**Architecture:** A single-page dashboard with a clean sidebar navigation adhering to Genesis tokens (Indigo `#6366F1` interactive accents, General Sans / DM Sans typography, 1px subtle borders `#E8E8EC`, 6px button/input radius, 12px card radius, 4px grid spacing, and zero static shadows). State management handles SSE real-time streaming, zero-flicker table updates, full Client/VHD/Disk/DHCP CRUD, and instant theme switching.

**Tech Stack:** HTML5, Vanilla JavaScript (ES6+), Tailwind CSS v4 (compiled via `@tailwindcss/cli`), SSE (`/api/stats`), RESTful APIs (`/api/*`).

**Spec:** [`genesis-DESIGN.md`](file:///c:/Project%20GIT/Simple-Iscsi/genesis-DESIGN.md)

## Global Constraints
- Naming & Branding: Application name must be **Simple Iscsi**.
- Design System: Adhere strictly to [`genesis-DESIGN.md`](file:///c:/Project%20GIT/Simple-Iscsi/genesis-DESIGN.md) color tokens, 4px grid spacing, 6px input/button radius, 12px card radius, minimal shadow elevation, and typography.
- Responsiveness: Flawless layout across `sm` (640px), `md` (768px), `lg` (1024px), `xl` (1280px), `2xl` (1536px+) without unwanted horizontal scrollbars.
- Dark Mode: Comprehensive light/dark theme system persisted in `localStorage`.
- Cleanup: Remove obsolete [`design-lovable.md`](file:///c:/Project%20GIT/Simple-Iscsi/design-lovable.md).
- Verification: Must compile CSS via `npm run build:css` and pass `cargo test` on every task.

---

### Task 1: Cleanup & CSS Design System (`genesis-DESIGN.md` Foundation)

**Files:**
- Delete: `design-lovable.md`
- Modify: `ui/input.css`
- Build: `ui/tailwind.css`

**Interfaces:**
- Produces: CSS utility tokens, custom `@theme` typography, `.dark` theme rules, animations, custom scrollbars.

- [ ] **Step 1: Remove obsolete design-lovable.md file**
  - Delete `design-lovable.md` from the project root.

- [ ] **Step 2: Build comprehensive `ui/input.css` based on Genesis tokens**
  - Define General Sans / DM Sans font families and JetBrains Mono.
  - Setup CSS variables and semantic color tokens for Light (`#FAFAFA` bg, `#FFFFFF` surface, `#0A0A0A` text, `#E8E8EC` border, `#6366F1` primary) and Dark (`#09090B` bg, `#18181B` surface, `#FAFAFA` text, `#27272A` border).
  - Include toast notifications, modal backdrops, context menus, and custom smooth scrollbars.

- [ ] **Step 3: Compile Tailwind CSS and verify**
  - Run `npm run build:css` and ensure clean exit.

---

### Task 2: Reconstruct `ui/index.html` Markup

**Files:**
- Modify: `ui/index.html`

**Interfaces:**
- Produces: Complete semantic HTML layout with Sticky Sidebar, Header, Theme Toggle Button, Dashboard Tab, Clients Manager Tab, Image Manager Tab, Disk Management Tab, Central Settings Tab, DHCP/TFTP Config, and Modals (Client CRUD, VHD CRUD, Snapshots, Context Menu, Confirm Modal).

- [ ] **Step 1: Write header, sidebar navigation, and branding**
  - Sidebar with app title **Simple Iscsi**, version tag, nav tabs (`Dashboard`, `Klien Manager`, `Image Manager`, `Disk Management`, `Pengaturan`), and bottom Theme Toggle (`🌙 Mode Gelap`).

- [ ] **Step 2: Write Tab 1 - Dashboard (Overview & Real-time I/O Table)**
  - Service status cards (`iSCSI Daemon`, `DHCP Server`, `TFTP Server`) with live badges.
  - Counter cards (`Total Active Connections`, `Total Configured PCs`).
  - Real-time I/O Clients table formatted in 6 clean multi-line columns (`Klien & Status`, `Jaringan`, `Boot Image & PXE`, `Read I/O`, `Write I/O`, `Uptime`) without fixed min-width overflow.

- [ ] **Step 3: Write Tab 2 - Klien Manager**
  - Header actions: `Auto-Allocate IPs` and `➕ Tambah Klien`.
  - 6-column multi-line client list with hover states, row click to edit, right-click context menu.

- [ ] **Step 4: Write Tab 3 - Image Manager & Snapshots**
  - Header action: `➕ Tambah VHD Mapping`.
  - 3-column multi-line table (`Image Key & VHD Path`, `Snapshot Backups`, `Aksi`).

- [ ] **Step 5: Write Tab 4 - Disk Management**
  - Dynamic partition visualizer grid and Global Storage parameters form.

- [ ] **Step 6: Write Tab 5 - Central Settings (DHCP, TFTP, iSCSI)**
  - 3-card responsive grid for Server & iSCSI, DHCP Server & Multi-NIC load balancing, TFTP bootloader manager.

- [ ] **Step 7: Write Modals & Context Menus**
  - Client CRUD Modal, VHD CRUD Modal, Snapshot History Modal, Context Menu, and Toast Container.

---

### Task 3: Reconstruct `ui/index.js` JavaScript Application

**Files:**
- Modify: `ui/index.js`

**Interfaces:**
- Produces: App state, API handlers, SSE real-time stats stream, zero-flicker cell updater, full CRUD workflows, native file dialog triggers, context menu, dark theme persistence.

- [ ] **Step 1: Setup Theme, Navigation & Global State**
  - Implement `initTheme()` syncing `.dark` on `document.documentElement` and `document.body` with `localStorage`.
  - Implement `initTabs()` supporting smooth tab switching and URL/hash preservation.

- [ ] **Step 2: Implement Real-time SSE Stream & Zero-Flicker Updates**
  - Implement `initStatsStream()` connecting to `/api/stats`.
  - Implement `handleStatsData()` using targeted class selectors (`.client-status-badge`, `.client-read-total`, `.client-read-speed`, `.client-write-total`, `.client-write-speed`, `.client-uptime`) to update cells without DOM re-renders.

- [ ] **Step 3: Implement Dashboard & Clients Manager Renderers**
  - Implement `renderDashboardClientsTable()` and `renderClientsManagerTable()` matching the Genesis 6-column multi-line layout.
  - Implement `openClientCrudModal()`, `saveClientAction()`, `deleteClientAction()`, `autoAllocateNextServerIpsAction()`.

- [ ] **Step 4: Implement Image Manager & Snapshot Handlers**
  - Implement `renderVhdTable()`, `openVhdCrudModal()`, `selectVhdFileViaExplorer()`, `saveVhdAction()`, `showVhdSnapshots()`, `restoreSnapshotAction()`.

- [ ] **Step 5: Implement Disk Management & Storage Settings**
  - Implement `loadDiskPartitions()`, `renderDiskGrid()`, `openPartitionModal()`, `saveGlobalStorageParams()`, `clearWritebackCache()`.

- [ ] **Step 6: Implement Central Settings & DHCP/TFTP Operations**
  - Implement `loadConfigJson()`, `saveConfigJson()`, `loadNetworkInterfaces()`, `addNicIpAction()`, `removeNicIpAction()`, `loadTftpFolders()`, `createNewTftpFolderPrompt()`.

- [ ] **Step 7: Implement Context Menus & Helpers**
  - Implement `initContextMenus()`, `showContextMenu()`, `ctxEnableSuperClient()`, `ctxClearWriteback()`, `showToast()`.

---

### Task 4: Verification, Testing & Codebase Memory Sync

**Files:**
- Test: Build CSS via `npm run build:css`
- Test: Rust backend unit tests via `cargo test`
- MCP: Codebase memory `detect_changes` and `index_repository`

- [ ] **Step 1: Compile CSS bundle**
  - Execute `npm run build:css` and confirm 0 errors.

- [ ] **Step 2: Run Rust unit tests**
  - Execute `cargo test` and confirm all 4 unit tests pass.

- [ ] **Step 3: Codebase memory index sync**
  - Run MCP `detect_changes` and `index_repository`.

- [ ] **Step 4: Commit changes to git**
  - Stage and commit with descriptive commit message.
