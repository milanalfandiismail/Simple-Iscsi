# Shadcn / Minimalist Tech UI & UX Revamp Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transform the Simple-Iscsi Web Management Dashboard into a **Shadcn / Minimalist Tech** aesthetic featuring a hyper-clean monochrome zinc palette, 1px hairline borders, small border radii (rounded-md/lg), 12-hour eye comfort dark mode, and seamless responsiveness across all Tailwind breakpoints (`sm`, `md`, `lg`, `xl`, `2xl`), built with the local Tailwind v4 compiler.

**Architecture:** 
1. **Design System & CSS Tokens (`ui/input.css`):** Replace the saturated Genesis design with a modern Shadcn Zinc monochrome design system. Define dark/light surface tokens, thin 1px borders, subtle hover elevations, and ergonomic dark-mode surfaces.
2. **HTML Structure & Layout Shell (`ui/index.html`):** Refactor the main layout container, responsive sidebar/sheet navigation, header, service status cards, stat cards, data tables, central settings form grid, and modal dialogs with strict Shadcn styling and responsive utility classes (`sm:`, `md:`, `lg:`, `xl:`, `2xl:`). Preserve all 50+ DOM element IDs and JavaScript event bindings.
3. **Dynamic Render Script Harmonization (`ui/index.js`):** Update dynamic HTML generation for client tables, status badges, disk partition cards, snapshot items, and toast notifications to output monochrome Shadcn classes instead of legacy colorful pills.
4. **Local Tailwind Build & Verification:** Compile CSS via `npm run build:css` (Tailwind CLI v4) and verify visual harmony and responsive behavior without any missing styles.

**Tech Stack:**
- **Core:** HTML5, Vanilla JavaScript ES6+, CSS3
- **CSS Framework:** Tailwind CSS v4 (`@tailwindcss/cli` v4.3.3)
- **Typography:** Inter / DM Sans (UI), JetBrains Mono (Technical Data / IPs / Ports / Speeds)
- **Design Inspiration:** Shadcn UI / Vercel Minimalist Tech

## Global Constraints
- **Preserve All DOM IDs & Handlers:** Do not rename or remove any existing element IDs (`#tab-dashboard`, `#clients-tbody`, `#settings-form`, `#theme-toggle-btn`, etc.) or inline JS handler calls (`switchTab`, `saveConfigJson`, `openClientCrudModal`, etc.).
- **Strict Monochrome Palette:** Primary UI surfaces must be Zinc/Neutral (`zinc-950`, `zinc-900`, `zinc-800`, `zinc-500`, `zinc-400`, `zinc-200`, `zinc-100`, `zinc-50`). No saturated indigo/purple accents. Semantic colors (Emerald for Online, Rose for Offline/Delete, Amber for Super Client/Warning) are restricted to small 6px dots, thin borders, or subdued badges.
- **Hairline Borders & Small Radii:** Borders must be 1px solid (`border border-zinc-200 dark:border-zinc-800`). Radii must be `rounded-md` (6px) or `rounded-lg` (8px). No bubble/pill shapes for cards or main action buttons.
- **Responsive Across 5 Breakpoints:** Explicit support for `sm` (640px), `md` (768px), `lg` (1024px), `xl` (1280px), and `2xl` (1536px).
- **Local Tailwind Build Requirement:** Must execute `npm run build:css` to generate production `./ui/tailwind.css`.

---

## Tasks

### Task 1: Design System Foundation & CSS Variables in `ui/input.css`

**Files:**
- Modify: `ui/input.css`

**Interfaces:**
- Consumes: Tailwind CSS v4 theme directives (`@theme`, `@custom-variant`, `@layer utilities`).
- Produces: CSS utility tokens for `.tab-panel`, `.modal-backdrop`, `.context-menu`, `.toast-container`, `.btn-primary`, `.btn-secondary`, `.btn-destructive`, `.badge`, `.card-flat`, `.card-interactive`, and global dark mode overrides matching Shadcn Zinc.

- [ ] **Step 1: Define Shadcn Zinc typography and tokens in `ui/input.css`**
  - Set `--font-sans` to `"Inter", "DM Sans", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif`.
  - Set `--font-mono` to `"JetBrains Mono", ui-monospace, SFMono-Regular, monospace`.
  - Replace `.btn-primary` with Shadcn dark/light monochrome styles:
    ```css
    .btn-primary {
      background-color: #18181b;
      color: #fafafa;
      border: 1px solid #27272a;
      border-radius: 6px;
      font-weight: 500;
      transition: background-color 150ms ease, border-color 150ms ease;
    }
    .btn-primary:hover {
      background-color: #27272a;
      border-color: #3f3f46;
    }
    html.dark .btn-primary {
      background-color: #fafafa;
      color: #18181b;
      border: 1px solid #e4e4e7;
    }
    html.dark .btn-primary:hover {
      background-color: #f4f4f5;
    }
    ```
  - Define `.btn-secondary`:
    ```css
    .btn-secondary {
      background-color: transparent;
      color: #18181b;
      border: 1px solid #e4e4e7;
      border-radius: 6px;
      font-weight: 500;
      transition: background-color 150ms ease, border-color 150ms ease;
    }
    .btn-secondary:hover {
      background-color: #f4f4f5;
    }
    html.dark .btn-secondary {
      color: #fafafa;
      border-color: #27272a;
    }
    html.dark .btn-secondary:hover {
      background-color: #27272a;
    }
    ```
  - Define ergonomic 12-hour dark mode base:
    `html.dark, body.dark { background-color: #09090b !important; color: #fafafa !important; }`
    Surface cards: `#121215` / `#09090b` with `border: 1px solid #27272a`.
  - Replace saturated blue/purple card hover effects with flat hairline borders (`border-zinc-300 dark:border-zinc-700`).

- [ ] **Step 2: Run local Tailwind build to verify CSS compilation**
  - Run: `npm run build:css`
  - Expected: `tailwindcss v4.x` finishes in < 300ms without syntax errors.

- [ ] **Step 3: Commit Task 1 changes**
  - Run: `git add ui/input.css`
  - Run: `git commit -m "style(ui): establish Shadcn Zinc monochrome design tokens and utility classes"`

---

### Task 2: Structural Shell, Navigation, & Responsive Breakpoints in `ui/index.html`

**Files:**
- Modify: `ui/index.html:1-85` (Layout Shell, Aside Sidebar, Header)

**Interfaces:**
- Consumes: Tailwind breakpoint utilities (`sm:`, `md:`, `lg:`, `xl:`, `2xl:`).
- Produces: Clean minimalist sidebar with hairline borders, refined brand mark (monochrome with ⚡), nav items with subtle zinc background when active (`bg-zinc-100 dark:bg-zinc-800 text-zinc-900 dark:text-zinc-100 rounded-md`), theme toggle, and engine status bar.

- [ ] **Step 1: Refactor Sidebar & Brand in `ui/index.html`**
  - Change `<aside>` styling to:
    `w-full lg:w-56 xl:w-64 bg-zinc-50/50 dark:bg-zinc-950/80 border-b lg:border-b-0 lg:border-r border-zinc-200 dark:border-zinc-800 backdrop-blur-sm flex flex-col justify-between shrink-0 sticky top-0 z-40 lg:h-screen`
  - Update Brand icon from saturated purple circle to minimalist black/white square with 1px border:
    `w-8 h-8 rounded-md bg-zinc-900 dark:bg-zinc-100 text-white dark:text-zinc-900 border border-zinc-700 dark:border-zinc-300 flex items-center justify-center font-bold text-sm shadow-xs`
  - Update Nav Buttons (`.nav-item`):
    Active state: `bg-zinc-200/70 dark:bg-zinc-800 text-zinc-900 dark:text-zinc-50 border border-zinc-300/80 dark:border-zinc-700 font-medium rounded-md px-3 py-2 text-xs xl:text-sm`
    Inactive state: `text-zinc-600 dark:text-zinc-400 hover:bg-zinc-100 dark:hover:bg-zinc-900 hover:text-zinc-900 dark:hover:text-zinc-100 rounded-md px-3 py-2 text-xs xl:text-sm`
  - Update Theme Toggle Button:
    `bg-zinc-100 dark:bg-zinc-900 hover:bg-zinc-200 dark:hover:bg-zinc-800 border border-zinc-200 dark:border-zinc-800 text-zinc-700 dark:text-zinc-300 rounded-md px-3 py-1.5 text-xs font-medium`

- [ ] **Step 2: Update `switchTab()` in `ui/index.js` to match new Nav classes**
  - Update active class toggle logic in `ui/index.js:60-80` to remove `bg-indigo-50`, `text-indigo-600`, etc., and replace with `bg-zinc-200/70 dark:bg-zinc-800 text-zinc-900 dark:text-zinc-50 border border-zinc-300/80 dark:border-zinc-700`.

- [ ] **Step 3: Test local compilation & responsive layout**
  - Run: `npm run build:css`
  - Expected: Zero build errors.

- [ ] **Step 4: Commit Task 2 changes**
  - Run: `git add ui/index.html ui/index.js`
  - Run: `git commit -m "feat(ui): implement Shadcn minimalist sidebar and responsive layout shell"`

---

### Task 3: Dashboard Overview & Service Grid Revamp in `ui/index.html` & `ui/index.js`

**Files:**
- Modify: `ui/index.html:86-176` (Tab Dashboard)
- Modify: `ui/index.js:515-603` (`renderDashboardClientsTable`)

**Interfaces:**
- Consumes: Real-time service status & SSE live client metrics.
- Produces: 
  - Service status cards (iSCSI, DHCP, TFTP) in a clean 1-col (`sm:grid-cols-2`, `lg:grid-cols-3`) grid with 1px border `border-zinc-200 dark:border-zinc-800`, subtle status dots, and minimal restart action buttons.
  - Stat counters (Active Connections, Configured PCs) with high typography clarity (`text-zinc-900 dark:text-zinc-50`).
  - Real-time I/O Clients table with hairline borders, subtle hover row highlighting (`hover:bg-zinc-50/70 dark:hover:bg-zinc-900/60`), and compact status dots (🟢 Online, ⚪ Offline).

- [ ] **Step 1: Refactor Service Cards & Stats Grid in `ui/index.html`**
  - Replace stone cards with Shadcn flat cards:
    `bg-white dark:bg-zinc-900/70 border border-zinc-200 dark:border-zinc-800 rounded-lg p-4 sm:p-5 flex items-center justify-between gap-3 shadow-2xs`
  - Refactor Status Badges (`#iscsi-status-pill`, `#dhcp-status-pill`, `#tftp-status-pill`):
    Online: `bg-emerald-500/10 text-emerald-700 dark:text-emerald-400 border border-emerald-500/20 px-2 py-0.5 rounded-md text-[11px] font-medium`
    Offline: `bg-zinc-100 dark:bg-zinc-800 text-zinc-600 dark:text-zinc-400 border border-zinc-200 dark:border-zinc-700 px-2 py-0.5 rounded-md text-[11px] font-medium`
  - Restart Action Buttons:
    `p-1.5 hover:bg-zinc-100 dark:hover:bg-zinc-800 text-zinc-600 dark:text-zinc-400 rounded-md border border-zinc-200 dark:border-zinc-800 text-xs transition cursor-pointer`

- [ ] **Step 2: Refactor `renderDashboardClientsTable()` in `ui/index.js`**
  - Update row rendering in `ui/index.js`:
    - Row class: `hover:bg-zinc-50/70 dark:hover:bg-zinc-900/60 transition-colors border-b border-zinc-100 dark:border-zinc-800/80 cursor-pointer`
    - Online badge: `<span class="inline-flex items-center gap-1.5 text-[11px] font-medium px-2 py-0.5 rounded-md bg-emerald-500/10 text-emerald-700 dark:text-emerald-400 border border-emerald-500/20"><span class="w-1.5 h-1.5 rounded-full bg-emerald-500"></span> Online</span>`
    - Offline badge: `<span class="inline-flex items-center gap-1.5 text-[11px] font-medium px-2 py-0.5 rounded-md bg-zinc-100 dark:bg-zinc-800/80 text-zinc-500 dark:text-zinc-400 border border-zinc-200 dark:border-zinc-700"><span class="w-1.5 h-1.5 rounded-full bg-zinc-400 dark:bg-zinc-500"></span> Offline</span>`
    - Super Client badge: `<span class="inline-flex items-center text-[10px] font-medium px-1.5 py-0.5 rounded-md bg-amber-500/10 text-amber-700 dark:text-amber-400 border border-amber-500/20 ml-1">⚡ Super</span>`
    - Image badge: `<span class="bg-zinc-100 dark:bg-zinc-800 border border-zinc-200 dark:border-zinc-700 rounded px-1.5 py-0.5 font-mono text-[10.5px] text-zinc-800 dark:text-zinc-200">...</span>`
    - Speed metrics: monochrome with subtle contrast (read speed: `text-zinc-900 dark:text-zinc-100 font-mono`, write speed: `text-zinc-600 dark:text-zinc-400 font-mono`).

- [ ] **Step 3: Test local build & inspect Dashboard styling**
  - Run: `npm run build:css`
  - Expected: PASS without error.

- [ ] **Step 4: Commit Task 3 changes**
  - Run: `git add ui/index.html ui/index.js`
  - Run: `git commit -m "feat(ui): update Dashboard overview, service cards, and real-time client table to Shadcn minimalist tech"`

---

### Task 4: Management Tabs Revamp (Clients, Images, & Central Settings)

**Files:**
- Modify: `ui/index.html:177-456` (Tabs: clients, vhd, dhcp/settings)
- Modify: `ui/index.js:605-685` (`renderClientsManagerTable`), `ui/index.js:710-770` (`renderVhdsTable`)

**Interfaces:**
- Consumes: Config and client state.
- Produces:
  - Klien Manager table with search/action bar.
  - Image Manager VHD table with snapshot view action.
  - Central Settings form layout: 3 clean cards (`sm:grid-cols-1`, `lg:grid-cols-2`, `xl:grid-cols-3`) with Shadcn styled text inputs (`bg-transparent border border-zinc-200 dark:border-zinc-800 rounded-md px-3 py-2 text-xs sm:text-sm focus:border-zinc-900 dark:focus:border-zinc-100 focus:outline-none`).

- [ ] **Step 1: Refactor Klien Manager & Image Manager in `ui/index.html`**
  - Replace action buttons with Shadcn buttons:
    Auto-Allocate: `inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-md border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-900 text-zinc-900 dark:text-zinc-100 hover:bg-zinc-50 dark:hover:bg-zinc-800`
    Tambah Klien / Tambah VHD: `btn-primary inline-flex items-center gap-1.5 px-3.5 py-1.5 text-xs font-medium shadow-2xs`
  - Table headers:
    `bg-zinc-50/80 dark:bg-zinc-900/80 border-b border-zinc-200 dark:border-zinc-800 text-[11px] font-medium text-zinc-500 dark:text-zinc-400 uppercase tracking-wider`

- [ ] **Step 2: Refactor `renderClientsManagerTable()` and `renderVhdsTable()` in `ui/index.js`**
  - Update row rendering in `ui/index.js` to match the monochrome table style with 1px border dividers.
  - Edit/Delete buttons: minimal icon/text buttons with `hover:bg-zinc-100 dark:hover:bg-zinc-800 rounded-md p-1.5 text-xs`.

- [ ] **Step 3: Refactor Central Settings Form Cards in `ui/index.html`**
  - Card 1 (Server & iSCSI Daemon), Card 2 (DHCP Server), Card 3 (TFTP Bootloader).
  - Modern form controls:
    Inputs: `w-full px-3 py-2 text-xs sm:text-sm rounded-md border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-950 text-zinc-900 dark:text-zinc-100 focus:outline-none focus:ring-1 focus:ring-zinc-400 dark:focus:ring-zinc-600 transition-colors`
    Checkboxes: `rounded border-zinc-300 dark:border-zinc-700 text-zinc-900 focus:ring-0 cursor-pointer`
    Load Balancing NIC list container: `border border-zinc-200 dark:border-zinc-800 rounded-md bg-zinc-50/50 dark:bg-zinc-900/50 p-2.5`

- [ ] **Step 4: Test local build**
  - Run: `npm run build:css`
  - Expected: PASS without error.

- [ ] **Step 5: Commit Task 4 changes**
  - Run: `git add ui/index.html ui/index.js`
  - Run: `git commit -m "feat(ui): update Clients, Images, and Central Settings tabs to Shadcn minimalist styling"`

---

### Task 5: Disk Management Grid & Storage Parameters

**Files:**
- Modify: `ui/index.html:243-297` (Tab disk-mgmt)
- Modify: `ui/index.js:820-950` (`loadDiskPartitions`, `renderDiskCard`)

**Interfaces:**
- Consumes: Windows physical disk and volume partition partition information.
- Produces: Dynamic responsive disk grid (`grid-cols-1 sm:grid-cols-2 lg:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4`), partition role indicator cards, and writeback file manager.

- [ ] **Step 1: Refactor Disk Management Tab HTML in `ui/index.html`**
  - Update disk grid container to support full breakpoints up to `2xl:grid-cols-4`.
  - Update Global Storage Parameters card and Writeback Cache Files card to Shadcn flat cards with 1px hairline borders.

- [ ] **Step 2: Refactor `renderDiskCard()` in `ui/index.js`**
  - Style disk cards with `bg-white dark:bg-zinc-900 border border-zinc-200 dark:border-zinc-800 rounded-lg p-4 sm:p-5 shadow-2xs`.
  - Partition bars: clean geometric bars with subtle monochrome/muted colors (Booting: `bg-zinc-800 dark:bg-zinc-200`, Writeback: `bg-amber-500/80`, GameDisk: `bg-emerald-500/80`, Unassigned: `bg-zinc-200 dark:bg-zinc-800`).
  - Disk badge indicators: `border border-zinc-200 dark:border-zinc-700 rounded-md px-1.5 py-0.5 text-[10px] font-mono`.

- [ ] **Step 3: Test local build**
  - Run: `npm run build:css`
  - Expected: PASS without error.

- [ ] **Step 4: Commit Task 5 changes**
  - Run: `git add ui/index.html ui/index.js`
  - Run: `git commit -m "feat(ui): revamp Disk Management partition cards and storage parameters"`

---

### Task 6: Modals, Dialogs, Context Menu, & Floating Widgets

**Files:**
- Modify: `ui/index.html:459-756` (All Modals, Context Menu, Floating Widget)
- Modify: `ui/index.js:1000-1150` (Modal triggers and helper renders)

**Interfaces:**
- Consumes: Modal state, user interactions.
- Produces:
  - Shadcn Dialogs (`rounded-lg`, border `border-zinc-200 dark:border-zinc-800`, subtle backdrop blur `rgba(0,0,0,0.5)`, clean header with title and muted description).
  - Context Menu (`border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-900 rounded-md shadow-md p-1 text-xs`).
  - Floating Async VHD Merge Progress widget (`border border-zinc-200 dark:border-zinc-800 rounded-lg bg-white dark:bg-zinc-900 shadow-lg`).
  - Toast Notifications (`border border-zinc-200 dark:border-zinc-800 rounded-md bg-zinc-900 dark:bg-zinc-100 text-zinc-50 dark:text-zinc-900 shadow-md text-xs px-3.5 py-2.5`).

- [ ] **Step 1: Refactor Modals in `ui/index.html`**
  - `#client-crud-modal`: Change `rounded-2xl` to `rounded-lg border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-900 p-5 sm:p-6 shadow-xl max-w-xl`.
  - `#vhd-crud-modal`: Change `rounded-2xl` to `rounded-lg border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-900 p-5 sm:p-6 shadow-xl max-w-lg`.
  - `#snapshots-modal`: Change `rounded-2xl` to `rounded-lg border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-900 p-5 sm:p-6 shadow-xl max-w-2xl`.
  - `#partition-modal`: Update role option cards to clean border radio cards (`border border-zinc-200 dark:border-zinc-800 rounded-md p-3 hover:bg-zinc-50 dark:hover:bg-zinc-800/50`).
  - `#confirm-modal`: Clean confirm dialog with subtle warning indicator and Shadcn destructive button (`bg-rose-600 hover:bg-rose-700 text-white rounded-md text-xs font-medium px-4 py-2`).
  - `#superclient-disable-modal`: Clean choice cards (Commit vs Discard) with hairline borders.

- [ ] **Step 2: Refactor Context Menu, Floating Widget, & Toasts**
  - Context menu: `bg-white dark:bg-zinc-900 border border-zinc-200 dark:border-zinc-800 rounded-md p-1 shadow-lg text-xs w-52 divide-y-0`. Items: `rounded px-2.5 py-1.5 hover:bg-zinc-100 dark:hover:bg-zinc-800 text-zinc-700 dark:text-zinc-300`.
  - Floating merge widget: `rounded-lg border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-900 shadow-xl p-4`.
  - Toast messages: `rounded-md border border-zinc-200 dark:border-zinc-800 bg-zinc-900 dark:bg-zinc-100 text-zinc-100 dark:text-zinc-900 shadow-lg text-xs font-medium px-3.5 py-2.5`.

- [ ] **Step 3: Test local build**
  - Run: `npm run build:css`
  - Expected: PASS without error.

- [ ] **Step 4: Commit Task 6 changes**
  - Run: `git add ui/index.html ui/index.js`
  - Run: `git commit -m "feat(ui): update all modals, context menu, widgets, and toasts to Shadcn styling"`

---

### Task 7: Tailwind Local Build, Responsive Verification, & Living Memory Update

**Files:**
- Output: `ui/tailwind.css` (compiled minified bundle)
- Modify: `ANTIGRAVITY.md` (Update UI architectural specifications & feature changelog)

**Interfaces:**
- Consumes: All UI changes across HTML, JS, CSS.
- Produces: Final minified CSS bundle, updated codebase memory graph, and updated living memory.

- [ ] **Step 1: Run production Tailwind CSS build**
  - Run: `npm run build:css`
  - Verify that `ui/tailwind.css` is generated and minified without errors.

- [ ] **Step 2: Re-index codebase memory with MCP `codebase-memory`**
  - Run `index_repository(repo_path: "c:\\Project GIT\\Simple-Iscsi")`.
  - Verify that UI files are cleanly indexed.

- [ ] **Step 3: Update `ANTIGRAVITY.md`**
  - Record the new **Shadcn / Minimalist Tech** design guidelines into Section 4 (Arsitektur Web Dashboard) replacing Genesis.
  - Add `[2026-10-06]` entry in Section 9 (Riwayat Pengerjaan).

- [ ] **Step 4: Commit & push final UI revamp**
  - Run: `git add ui/tailwind.css ANTIGRAVITY.md docs/superpowers/plans/2026-10-06-shadcn-minimalist-tech-ui-revamp-plan.md`
  - Run: `git commit -m "feat(ui): complete Shadcn Minimalist Tech UI revamp with full responsive breakpoints"`
  - Run: `git push origin v1.0.0-final`

---

## Execution Choice
After this plan is reviewed and approved, execution proceeds using either:
1. **Subagent-Driven Development (Recommended):** Dispatch fresh subagents per task with two-stage reviews.
2. **Inline Execution:** Execute tasks step-by-step in the current session.
