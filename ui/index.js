// State variables
let activeTab = 'dashboard';
let stats = {};
let configObj = null; // Parsed config.toml JSON representation
let clientsObj = { client: [] }; // Parsed clients.toml JSON representation
let activeSessionsMap = new Map();
let clientSpeedHistory = new Map();
let availableNetworkIps = [];
let renderedDashboardIps = [];

// Initialization
document.addEventListener('DOMContentLoaded', async () => {
    initTabs();
    initContextMenus();
    initTheme();
    
    // Initial data loading sequence
    await loadInitialData();

    // Start SSE stream for real-time stats
    initStatsStream();

    // Background auto-sync for disk and writeback
    initAutoSyncIntervals();
});

function initTheme() {
    const btn = document.getElementById('theme-toggle-btn');
    const currentTheme = localStorage.getItem('theme') || 'light';
    
    if (currentTheme === 'dark') {
        document.body.classList.add('dark-theme');
        btn.textContent = '🌞 Mode Terang';
    } else {
        document.body.classList.remove('dark-theme');
        btn.textContent = '🌙 Mode Gelap';
    }

    btn.addEventListener('click', () => {
        if (document.body.classList.contains('dark-theme')) {
            document.body.classList.remove('dark-theme');
            btn.textContent = '🌙 Mode Gelap';
            localStorage.setItem('theme', 'light');
        } else {
            document.body.classList.add('dark-theme');
            btn.textContent = '🌞 Mode Terang';
            localStorage.setItem('theme', 'dark');
        }
    });
}

async function loadInitialData() {
    await loadNetworkInterfaces();
    await loadConfigJson();
    await loadClientsJson();
    loadWritebackFiles();
}

function initAutoSyncIntervals() {
    setInterval(() => {
        if (activeTab === 'writeback') {
            loadWritebackFiles();
        } else if (activeTab === 'disk-mgmt') {
            populateSystemDrives();
        }
    }, 4000);
}

// Tab Navigation
function initTabs() {
    const navItems = document.querySelectorAll('.nav-item');
    navItems.forEach(item => {
        item.addEventListener('click', (e) => {
            e.preventDefault();
            const targetTab = item.getAttribute('data-tab');
            
            navItems.forEach(i => i.classList.remove('active'));
            item.classList.add('active');

            document.querySelectorAll('.tab-panel').forEach(panel => {
                panel.classList.remove('active');
            });
            document.getElementById(`tab-${targetTab}`).classList.add('active');
            activeTab = targetTab;
            
            // Reload tab specific components dynamically
            if (activeTab === 'settings') { loadNetworkInterfaces(); loadConfigJson(); loadTftpFolders(); }
            if (activeTab === 'clients') loadClientsJson();
            if (activeTab === 'vhd') loadConfigJson();
            if (activeTab === 'disk-mgmt') loadConfigJson();
            if (activeTab === 'writeback') loadWritebackFiles();
        });
    });
}

// Network Interfaces Loader & Dropdown Populator
async function loadNetworkInterfaces() {
    const ips = await apiGet('/api/system/network_interfaces');
    if (ips && Array.isArray(ips)) {
        availableNetworkIps = ips;
        populateIpDropdowns();
    }
}

function populateIpDropdowns() {
    // 1. #set-server-address dropdown (includes 0.0.0.0 (Semua Interface / Any))
    const serverSelect = document.getElementById('set-server-address');
    if (serverSelect) {
        const savedVal = serverSelect.value || (configObj && configObj.server ? (Array.isArray(configObj.server.address) ? configObj.server.address[0] : configObj.server.address) : '0.0.0.0');
        serverSelect.innerHTML = '<option value="0.0.0.0">0.0.0.0 (Semua Interface / Any)</option>';
        availableNetworkIps.forEach(ip => {
            const opt = document.createElement('option');
            opt.value = ip;
            opt.textContent = `${ip} (Interface Lokal)`;
            serverSelect.appendChild(opt);
        });
        if (savedVal && !['0.0.0.0', ...availableNetworkIps].includes(savedVal)) {
            const opt = document.createElement('option');
            opt.value = savedVal;
            opt.textContent = `${savedVal} (Custom)`;
            serverSelect.appendChild(opt);
        }
        serverSelect.value = savedVal || '0.0.0.0';
    }

    // 2. #set-dhcp-next dropdown (ONLY physical IPs, NO 0.0.0.0)
    const nextSelect = document.getElementById('set-dhcp-next');
    if (nextSelect) {
        const savedNext = nextSelect.value || (configObj && configObj.dhcp ? configObj.dhcp.next_server : '') || '';
        nextSelect.innerHTML = '<option value="">-- Pilih IP Adapter Server --</option>';
        availableNetworkIps.forEach(ip => {
            const opt = document.createElement('option');
            opt.value = ip;
            opt.textContent = `${ip}`;
            nextSelect.appendChild(opt);
        });
        if (savedNext && !availableNetworkIps.includes(savedNext) && savedNext !== '0.0.0.0') {
            const opt = document.createElement('option');
            opt.value = savedNext;
            opt.textContent = `${savedNext} (Custom)`;
            nextSelect.appendChild(opt);
        }
        nextSelect.value = savedNext;
    }

    // 3. #network-ips-datalist (for #set-dhcp-gateway, #add-nic-ip-input, without 0.0.0.0)
    const datalist = document.getElementById('network-ips-datalist');
    if (datalist) {
        datalist.innerHTML = '';
        availableNetworkIps.forEach(ip => {
            const opt = document.createElement('option');
            opt.value = ip;
            datalist.appendChild(opt);
        });
    }
}

// API Helpers
async function apiGet(endpoint) {
    try {
        const res = await fetch(endpoint);
        if (!res.ok) throw new Error(`HTTP error! status: ${res.status}`);
        return await res.json();
    } catch (e) {
        console.error(`Failed to GET ${endpoint}:`, e);
        return null;
    }
}

async function apiPost(endpoint, body) {
    try {
        const res = await fetch(endpoint, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(body)
        });
        if (!res.ok) throw new Error(`HTTP error! status: ${res.status}`);
        return true;
    } catch (e) {
        console.error(`Failed to POST to ${endpoint}:`, e);
        return false;
    }
}

async function apiPostJson(endpoint, body) {
    try {
        const res = await fetch(endpoint, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(body)
        });
        if (!res.ok) throw new Error(`HTTP error! status: ${res.status}`);
        return await res.json();
    } catch (e) {
        console.error(`Failed to POST JSON to ${endpoint}:`, e);
        return null;
    }
}

// Polling and Statistics
function initStatsStream() {
    const evtSource = new EventSource('/api/stats/stream');
    
    evtSource.onmessage = (event) => {
        try {
            const data = JSON.parse(event.data);
            handleStatsData(data);
        } catch (e) {
            console.error("Error parsing SSE data:", e);
        }
    };
    
    evtSource.onerror = (err) => {
        console.error("EventSource failed:", err);
    };
}

function setTextIfChanged(el, text) {
    if (el && el.textContent !== text) el.textContent = text;
}

function setHtmlIfChanged(el, html) {
    if (el && el.innerHTML !== html) el.innerHTML = html;
}

function getMergedDashboardClients() {
    let clientsList = Array.isArray(clientsObj?.client) ? [...clientsObj.client] : [];
    const knownIps = new Set(clientsList.map(c => c.ip));

    if (stats && Array.isArray(stats.clients)) {
        stats.clients.forEach(sc => {
            if (!knownIps.has(sc.ip)) {
                clientsList.push({
                    ip: sc.ip,
                    hostname: `DHCP-${sc.ip.split('.').pop()}`,
                    mac: '-',
                    dns: '-',
                    gateway: '-',
                    image_manager: 'Dynamic / Booting',
                    next_server: '-',
                    isDynamic: true
                });
                knownIps.add(sc.ip);
            }
        });
    }
    return clientsList;
}

function handleStatsData(data) {
    if (!data) return;

    stats = data;

    // 1. Dynamic service cards updates
    if (data.services) {
        updateServiceCard('iscsi', data.services.iscsi);
        updateServiceCard('dhcp', data.services.dhcp);
        updateServiceCard('tftp', data.services.tftp);
    }

    // 2. Active connections total value
    const connsEl = document.getElementById('stat-conns');
    if (connsEl) connsEl.textContent = data.active_sessions;

    // Track active sessions map for table referencing
    activeSessionsMap.clear();
    if (data.clients) {
        data.clients.forEach(c => {
            activeSessionsMap.set(c.ip, c);
        });
    }

    const mergedClients = getMergedDashboardClients();
    const currentIps = mergedClients.map(c => c.ip).join(',');

    // Re-render table structure if dynamic client list changed
    if (currentIps !== renderedDashboardIps.join(',')) {
        renderDashboardClientsTable();
    }

    // 3. Update existing table cells in DOM without rebuilding (zero flicker)
    const now = Date.now();
    requestAnimationFrame(() => {
        mergedClients.forEach(c => {
            const statsInfo = activeSessionsMap.get(c.ip) || {
                active: false,
                bytes_read: 0,
                bytes_written: 0,
                uptime_secs: 0
            };

            let speedInfo = clientSpeedHistory.get(c.ip);
            if (!speedInfo) {
                speedInfo = {
                    lastTime: now,
                    lastRead: statsInfo.bytes_read,
                    lastWrite: statsInfo.bytes_written,
                    readSpeed: 0,
                    writeSpeed: 0
                };
                clientSpeedHistory.set(c.ip, speedInfo);
            } else {
                const elapsedSecs = (now - speedInfo.lastTime) / 1000.0;
                if (elapsedSecs >= 0.5) {
                    const deltaRead = statsInfo.bytes_read - speedInfo.lastRead;
                    const deltaWrite = statsInfo.bytes_written - speedInfo.lastWrite;

                    speedInfo.readSpeed = deltaRead > 0 ? (deltaRead / elapsedSecs) : 0;
                    speedInfo.writeSpeed = deltaWrite > 0 ? (deltaWrite / elapsedSecs) : 0;

                    speedInfo.lastTime = now;
                    speedInfo.lastRead = statsInfo.bytes_read;
                    speedInfo.lastWrite = statsInfo.bytes_written;
                }
            }

            const statusText = statsInfo.active
                ? `<span style="color: #22c55e;">🟢 Online</span>`
                : `<span style="color: #ef4444;">🔴 Offline</span>`;

            // Update Dashboard Table Row
            const dbRow = document.querySelector(`#dashboard-clients-tbody tr[data-ip="${c.ip}"]`);
            if (dbRow) {
                setHtmlIfChanged(dbRow.cells[0], statusText);
                setTextIfChanged(dbRow.cells[6], formatBytes(statsInfo.bytes_read));
                setTextIfChanged(dbRow.cells[7], formatSpeed(speedInfo.readSpeed));
                setTextIfChanged(dbRow.cells[8], formatBytes(statsInfo.bytes_written));
                setTextIfChanged(dbRow.cells[9], formatSpeed(speedInfo.writeSpeed));
                setTextIfChanged(dbRow.cells[10], statsInfo.active ? formatDuration(statsInfo.uptime_secs) : 'Offline');
            }

            // Update Clients Manager Table Row
            const cmRow = document.querySelector(`#clients-tbody tr[data-ip="${c.ip}"]`);
            if (cmRow) {
                setHtmlIfChanged(cmRow.cells[0], statusText);
                setTextIfChanged(cmRow.cells[8], formatBytes(statsInfo.bytes_read));
                setTextIfChanged(cmRow.cells[9], formatSpeed(speedInfo.readSpeed));
                setTextIfChanged(cmRow.cells[10], formatBytes(statsInfo.bytes_written));
                setTextIfChanged(cmRow.cells[11], formatSpeed(speedInfo.writeSpeed));
                setTextIfChanged(cmRow.cells[12], statsInfo.active ? formatDuration(statsInfo.uptime_secs) : 'Offline');
            }
        });
    });
}

function updateServiceCard(name, service) {
    const portEl = document.getElementById(`${name}-port-info`);
    const pillEl = document.getElementById(`${name}-status-pill`);
    if (portEl && pillEl) {
        portEl.textContent = `Port: ${service.port}`;
        if (service.enabled) {
            pillEl.textContent = '🟢 Enabled';
            pillEl.className = 'pill-status text-xs font-medium px-3 py-1 rounded-full bg-emerald-500/10 text-emerald-800 border border-emerald-500/20';
        } else {
            pillEl.textContent = '🔴 Disabled';
            pillEl.className = 'pill-status text-xs font-medium px-3 py-1 rounded-full bg-[#1c1c1c]/[0.04] text-[#1c1c1c] border border-[#eceae4]';
        }
    }
}

// Render Dashboard Clients Table (Static clients with real-time stats)
function renderDashboardClientsTable() {
    const tbody = document.getElementById('dashboard-clients-tbody');
    if (!tbody) return;

    const mergedClients = getMergedDashboardClients();
    renderedDashboardIps = mergedClients.map(c => c.ip);

    if (mergedClients.length === 0) {
        tbody.innerHTML = `<tr><td colspan="11" class="py-12 px-6 text-center">
            <div class="flex flex-col items-center justify-center text-center gap-3">
                <div class="text-4xl">🔌</div>
                <h3 class="font-bold text-base text-[#1c1c1c] font-['Outfit']">Belum Ada Klien Aktif</h3>
                <p class="text-[#5f5f5d] text-xs sm:text-sm max-w-sm">Klien yang terhubung dan menyala akan muncul di sini secara real-time.</p>
            </div>
        </td></tr>`;
        const totalPcsEl = document.getElementById('stat-total-pcs');
        if (totalPcsEl) totalPcsEl.textContent = 0;
        return;
    }

    tbody.innerHTML = '';
    mergedClients.forEach(c => {
        const statsInfo = activeSessionsMap.get(c.ip) || {
            active: false,
            bytes_read: 0,
            bytes_written: 0,
            uptime_secs: 0
        };

        const speedInfo = clientSpeedHistory.get(c.ip) || { readSpeed: 0, writeSpeed: 0 };

        const statusSpan = statsInfo.active
            ? `<span class="inline-flex items-center gap-1.5 text-xs font-semibold px-2.5 py-1 rounded-full bg-emerald-500/10 text-emerald-800 border border-emerald-500/20">🟢 Online</span>`
            : `<span class="inline-flex items-center gap-1.5 text-xs font-semibold px-2.5 py-1 rounded-full bg-rose-500/10 text-rose-800 border border-rose-500/20">🔴 Offline</span>`;

        const isSuper = configObj && configObj.windows && configObj.windows.super_client_ip === c.ip;
        const superBadge = isSuper ? ` <span class="inline-flex items-center text-[11px] font-semibold px-2 py-0.5 rounded-full bg-amber-100 text-amber-900 border border-amber-300 ml-1.5">⚡ Super Client</span>` : '';
        const dynamicBadge = c.isDynamic ? ` <span class="inline-flex items-center text-[10px] font-semibold px-2 py-0.5 rounded-full bg-indigo-100 text-indigo-900 border border-indigo-200 ml-1.5">DHCP Auto</span>` : '';

        const row = document.createElement('tr');
        row.setAttribute('data-ip', c.ip);
        row.className = "hover:bg-[#1c1c1c]/[0.02] transition-colors";
        row.innerHTML = `
            <td class="py-4 px-5">${statusSpan}</td>
            <td class="py-4 px-5 font-semibold text-[#1c1c1c]">${c.ip}${superBadge}${dynamicBadge}</td>
            <td class="py-4 px-5 text-[#5f5f5d]">${c.dns || '-'}</td>
            <td class="py-4 px-5 text-[#5f5f5d]">${c.gateway || '-'}</td>
            <td class="py-4 px-5 font-mono text-xs text-[#1c1c1c] bg-[#1c1c1c]/[0.02] rounded px-1.5 py-0.5">${c.image_manager || 'None (Gamedisk)'}</td>
            <td class="py-4 px-5 text-[#5f5f5d]">${c.next_server || '-'}</td>
            <td class="py-4 px-5 font-mono text-xs">${formatBytes(statsInfo.bytes_read)}</td>
            <td class="py-4 px-5 font-mono text-xs text-blue-600 font-semibold">${formatSpeed(speedInfo.readSpeed)}</td>
            <td class="py-4 px-5 font-mono text-xs">${formatBytes(statsInfo.bytes_written)}</td>
            <td class="py-4 px-5 font-mono text-xs text-amber-600 font-semibold">${formatSpeed(speedInfo.writeSpeed)}</td>
            <td class="py-4 px-5 text-xs text-[#5f5f5d]">${statsInfo.active ? formatDuration(statsInfo.uptime_secs) : 'Offline'}</td>
        `;
        tbody.appendChild(row);
    });

    const totalPcsEl = document.getElementById('stat-total-pcs');
    if (totalPcsEl) totalPcsEl.textContent = mergedClients.length;
}

// Render Clients Manager Tab Table (Full List)
function renderClientsManagerTable() {
    const tbody = document.getElementById('clients-tbody');
    if (!clientsObj.client || clientsObj.client.length === 0) {
        tbody.innerHTML = `<tr><td colspan="13" class="py-12 px-6 text-center">
            <div class="flex flex-col items-center justify-center text-center gap-3">
                <div class="text-4xl">💻</div>
                <h3 class="font-bold text-base text-[#1c1c1c] font-['Outfit']">Daftar Klien Kosong</h3>
                <p class="text-[#5f5f5d] text-xs sm:text-sm max-w-sm">Klik tombol "Tambah Klien" untuk mulai mendaftarkan PC diskless Anda.</p>
            </div>
        </td></tr>`;
        return;
    }

    tbody.innerHTML = '';
    clientsObj.client.forEach(c => {
        const statsInfo = activeSessionsMap.get(c.ip) || {
            active: false,
            bytes_read: 0,
            bytes_written: 0,
            uptime_secs: 0
        };

        const speedInfo = clientSpeedHistory.get(c.ip) || { readSpeed: 0, writeSpeed: 0 };

        const statusSpan = statsInfo.active
            ? `<span class="inline-flex items-center gap-1.5 text-xs font-semibold px-2.5 py-1 rounded-full bg-emerald-500/10 text-emerald-800 border border-emerald-500/20">🟢 Online</span>`
            : `<span class="inline-flex items-center gap-1.5 text-xs font-semibold px-2.5 py-1 rounded-full bg-rose-500/10 text-rose-800 border border-rose-500/20">🔴 Offline</span>`;

        const isSuper = configObj && configObj.windows && configObj.windows.super_client_ip === c.ip;
        const superBadge = isSuper ? ` <span class="inline-flex items-center text-[11px] font-semibold px-2 py-0.5 rounded-full bg-amber-100 text-amber-900 border border-amber-300 ml-1.5">⚡ Super Client</span>` : '';

        const row = document.createElement('tr');
        row.setAttribute('data-ip', c.ip);
        row.className = "hover:bg-[#1c1c1c]/[0.02] transition-colors cursor-pointer";
        row.innerHTML = `
            <td class="py-4 px-5">${statusSpan}</td>
            <td class="py-4 px-5 font-semibold text-[#1c1c1c]">${c.hostname || 'PC'}${superBadge}</td>
            <td class="py-4 px-5 font-mono text-xs">${c.ip}</td>
            <td class="py-4 px-5 font-mono text-xs text-[#5f5f5d]">${c.mac}</td>
            <td class="py-4 px-5 text-[#5f5f5d]">${c.dns || '-'}</td>
            <td class="py-4 px-5 text-[#5f5f5d]">${c.gateway || '-'}</td>
            <td class="py-4 px-5 font-mono text-xs text-[#1c1c1c]">${c.image_manager || 'Gamedisk'}</td>
            <td class="py-4 px-5 text-[#5f5f5d]">${c.next_server || '-'}</td>
            <td class="py-4 px-5 font-mono text-xs">${formatBytes(statsInfo.bytes_read)}</td>
            <td class="py-4 px-5 font-mono text-xs text-blue-600 font-semibold">${formatSpeed(speedInfo.readSpeed)}</td>
            <td class="py-4 px-5 font-mono text-xs">${formatBytes(statsInfo.bytes_written)}</td>
            <td class="py-4 px-5 font-mono text-xs text-amber-600 font-semibold">${formatSpeed(speedInfo.writeSpeed)}</td>
            <td class="py-4 px-5 text-xs text-[#5f5f5d]">${statsInfo.active ? formatDuration(statsInfo.uptime_secs) : 'Offline'}</td>
        `;

        // Double click or click to edit client configuration
        row.addEventListener('click', () => openClientCrudModal(c));

        // Add context menu handler
        row.addEventListener('contextmenu', (e) => {
            e.preventDefault();
            showContextMenu(e, { ip: c.ip, active: statsInfo.active, image_manager: c.image_manager });
        });

        tbody.appendChild(row);
    });
}

// Client CRUD Modal Handlers
async function openClientCrudModal(client = null) {
    const modal = document.getElementById('client-crud-modal');
    modal.style.display = 'flex';

    // Populate dropdown images (includes both config aliases and physical VHD filenames)
    await populateClientImageDropdown();
    await loadTftpFolders();

    if (client) {
        document.getElementById('client-modal-title').textContent = 'Edit Data Klien';
        document.getElementById('client-old-mac').value = client.mac;
        document.getElementById('client-hostname').value = client.hostname || '';
        document.getElementById('client-mac').value = client.mac;
        document.getElementById('client-ip').value = client.ip;
        document.getElementById('client-gateway').value = client.gateway || '';
        document.getElementById('client-dns').value = client.dns || '';
        document.getElementById('client-next-server').value = client.next_server || '';
        document.getElementById('client-pxe').value = client.pxe || '';
        document.getElementById('client-image-manager').value = client.image_manager || '';
        document.getElementById('btn-client-delete').style.display = 'inline-flex';
    } else {
        document.getElementById('client-modal-title').textContent = 'Tambah Klien Baru';
        document.getElementById('client-old-mac').value = '';
        document.getElementById('client-crud-form').reset();
        document.getElementById('btn-client-delete').style.display = 'none';
    }
}

function closeClientCrudModal() {
    document.getElementById('client-crud-modal').style.display = 'none';
}

async function populateClientImageDropdown() {
    const select = document.getElementById('client-image-manager');
    select.innerHTML = '<option value="">-- Tanpa Image (Gamedisk Only) --</option>';
    
    // Add VHD manager / Image manager keys
    if (configObj && configObj.image_manager) {
        Object.keys(configObj.image_manager).forEach(key => {
            const opt = document.createElement('option');
            opt.value = key;
            opt.textContent = `${key} (Alias)`;
            select.appendChild(opt);
        });
    }

    // Add physical VHD files scanned in the system
    const vhds = await apiGet('/api/system/vhds');
    if (vhds && Array.isArray(vhds)) {
        vhds.forEach(v => {
            // Avoid adding it if it is already present as an alias key
            if (configObj && configObj.image_manager && configObj.image_manager[v]) return;
            const opt = document.createElement('option');
            opt.value = v;
            opt.textContent = `${v} (Physical File)`;
            select.appendChild(opt);
        });
    }
}

async function saveClientAction(e) {
    e.preventDefault();
    const oldMac = document.getElementById('client-old-mac').value;
    
    const clientData = {
        mac: document.getElementById('client-mac').value.trim(),
        ip: document.getElementById('client-ip').value.trim(),
        hostname: document.getElementById('client-hostname').value.trim() || null,
        gateway: document.getElementById('client-gateway').value.trim() || null,
        dns: document.getElementById('client-dns').value.trim() || null,
        pxe: document.getElementById('client-pxe').value.trim() || null,
        next_server: document.getElementById('client-next-server').value.trim() || null,
        image_manager: document.getElementById('client-image-manager').value || null,
        bootfile_uefi: null,
        bootfile_legacy: null,
        bootfile_ipxe: null
    };

    if (oldMac) {
        // Edit flow
        const idx = clientsObj.client.findIndex(c => c.mac === oldMac);
        if (idx !== -1) {
            clientsObj.client[idx] = clientData;
        }
    } else {
        // Create flow
        clientsObj.client.push(clientData);
    }

    await saveClientsJson();
    showToast(oldMac ? 'Data klien berhasil diperbarui.' : 'Klien baru berhasil ditambahkan.', 'success');
    closeClientCrudModal();
}

async function deleteClientAction() {
    const oldMac = document.getElementById('client-old-mac').value;
    if (!oldMac) return;

    if (confirm('Apakah Anda yakin ingin menghapus data klien ini?')) {
        clientsObj.client = clientsObj.client.filter(c => c.mac !== oldMac);
        await saveClientsJson();
        showToast('Data klien berhasil dihapus.', 'success');
        closeClientCrudModal();
    }
}

async function loadClientsJson() {
    const data = await apiGet('/api/clients/json');
    if (data) {
        clientsObj = data;
        if (!clientsObj.client) clientsObj.client = [];
        renderClientsManagerTable();
        renderDashboardClientsTable();
    }
}

async function saveClientsJson() {
    const success = await apiPost('/api/clients/json', clientsObj);
    if (success) {
        await loadClientsJson();
    }
}

// VHD Manager CRUD Handlers
function renderVhdTable() {
    const tbody = document.getElementById('vhds-tbody');
    if (!configObj || !configObj.image_manager || Object.keys(configObj.image_manager).length === 0) {
        tbody.innerHTML = `<tr><td colspan="4" class="py-12 px-6 text-center">
            <div class="flex flex-col items-center justify-center text-center gap-3">
                <div class="text-4xl">💿</div>
                <h3 class="font-bold text-base text-[#1c1c1c] font-['Outfit']">Belum Ada VHD</h3>
                <p class="text-[#5f5f5d] text-xs sm:text-sm max-w-sm">Daftarkan file VHD Windows yang akan di-boot oleh klien Anda.</p>
            </div>
        </td></tr>`;
        return;
    }

    tbody.innerHTML = '';
    Object.entries(configObj.image_manager).forEach(([key, path]) => {
        const row = document.createElement('tr');
        row.className = "hover:bg-[#1c1c1c]/[0.02] transition-colors";
        row.innerHTML = `
            <td class="py-4 px-5 font-mono text-xs font-semibold text-[#1c1c1c]">${key}</td>
            <td class="py-4 px-5 font-mono text-xs text-[#5f5f5d]">${path}</td>
            <td class="py-4 px-5 text-xs text-[#5f5f5d]" id="snapshots-count-${key}">Loading...</td>
            <td class="py-4 px-5" style="text-align: right;">
                <button class="inline-flex items-center justify-center px-3 py-1.5 text-xs font-medium rounded-md bg-transparent text-[#1c1c1c] border border-[#1c1c1c]/30 hover:bg-[#1c1c1c]/[0.04] transition-all mr-2" onclick="openVhdCrudModal('${key}', '${path}')">Edit</button>
                <button class="inline-flex items-center justify-center px-3 py-1.5 text-xs font-medium rounded-md bg-[#1c1c1c] text-[#fcfbf8] hover:bg-[#1c1c1c]/90 transition-all shadow-xs" onclick="showVhdSnapshots('${key}')">Snapshots</button>
            </td>
        `;
        tbody.appendChild(row);

        // Fetch snapshots count asynchronously
        fetchSnapshotsCount(key);
    });
}

async function fetchSnapshotsCount(key) {
    const el = document.getElementById(`snapshots-count-${key}`);
    const data = await apiGet(`/api/vhd/backups?image_key=${key}`);
    if (data && el) {
        el.textContent = `${data.length} snapshots`;
    } else if (el) {
        el.textContent = '0 snapshots';
    }
}

function openVhdCrudModal(key = null, path = null) {
    const modal = document.getElementById('vhd-crud-modal');
    modal.style.display = 'flex';

    if (key) {
        document.getElementById('vhd-modal-title').textContent = 'Edit VHD Mapping';
        document.getElementById('vhd-old-key').value = key;
        document.getElementById('vhd-image-key').value = key;
        document.getElementById('vhd-path').value = path;
        document.getElementById('btn-vhd-delete').style.display = 'inline-flex';
    } else {
        document.getElementById('vhd-modal-title').textContent = 'Tambah VHD Mapping';
        document.getElementById('vhd-old-key').value = '';
        document.getElementById('vhd-crud-form').reset();
        document.getElementById('btn-vhd-delete').style.display = 'none';
    }
}

function closeVhdCrudModal() {
    document.getElementById('vhd-crud-modal').style.display = 'none';
}

async function selectVhdFileViaExplorer() {
    const res = await apiPostJson('/api/system/select_vhd');
    if (res && res.path) {
        document.getElementById('vhd-path').value = res.path;
    }
}

async function saveVhdAction(e) {
    e.preventDefault();
    const oldKey = document.getElementById('vhd-old-key').value;
    const newKey = document.getElementById('vhd-image-key').value.trim();
    const fullPath = document.getElementById('vhd-path').value.trim();
    if (!fullPath) return;

    if (/\s/.test(newKey)) {
        showToast('Nama Alias (Image Key) tidak boleh mengandung spasi karena sensitif terhadap format iSCSI IQN!', 'error');
        return;
    }

    if (!configObj.image_manager) configObj.image_manager = {};

    let clientsChanged = false;
    if (oldKey && oldKey !== newKey) {
        if (clientsObj && Array.isArray(clientsObj.client)) {
            clientsObj.client.forEach(c => {
                if (c.image_manager === oldKey) {
                    c.image_manager = newKey;
                    clientsChanged = true;
                }
            });
        }
    }

    if (oldKey) {
        delete configObj.image_manager[oldKey];
    }
    configObj.image_manager[newKey] = fullPath;

    await saveConfigJsonFull();
    if (clientsChanged) {
        await saveClientsJson();
    }
    showToast('Mapping VHD berhasil disimpan.', 'success');
    closeVhdCrudModal();
}

async function deleteVhdAction() {
    const oldKey = document.getElementById('vhd-old-key').value;
    if (!oldKey) return;

    if (confirm(`Apakah Anda yakin ingin menghapus mapping VHD '${oldKey}'?`)) {
        delete configObj.image_manager[oldKey];
        await saveConfigJsonFull();
        showToast('Mapping VHD berhasil dihapus.', 'success');
        closeVhdCrudModal();
    }
}

// Dynamic Writeback cache list from config.toml
async function loadWritebackFiles() {
    const data = await apiGet('/api/writeback/files');
    const tbody = document.getElementById('writeback-tbody');
    if (!data || data.length === 0) {
        tbody.innerHTML = `<tr><td colspan="4" class="py-12 px-6 text-center">
            <div class="flex flex-col items-center justify-center text-center gap-3">
                <div class="text-4xl">✨</div>
                <h3 class="font-bold text-base text-[#1c1c1c] font-['Outfit']">Writeback Bersih</h3>
                <p class="text-[#5f5f5d] text-xs sm:text-sm max-w-sm">Belum ada file cache writeback aktif. Cache akan terbuat otomatis saat klien menyala.</p>
            </div>
        </td></tr>`;
        return;
    }

    tbody.innerHTML = '';
    data.forEach(f => {
        const row = document.createElement('tr');
        row.className = "hover:bg-[#1c1c1c]/[0.02] transition-colors";
        row.innerHTML = `
            <td class="py-4 px-5 font-mono text-xs font-semibold text-[#1c1c1c]">${f.name}</td>
            <td class="py-4 px-5 font-mono text-xs">${formatBytes(f.size)}</td>
            <td class="py-4 px-5 text-xs text-[#5f5f5d] font-mono">${f.path}</td>
            <td class="py-4 px-5" style="text-align: right;">
                <button class="inline-flex items-center justify-center px-3 py-1.5 text-xs font-medium rounded-md bg-transparent text-rose-600 border border-rose-600/40 hover:bg-rose-50/50 transition-all" onclick="clearWritebackCache('${f.path}')">Hapus</button>
            </td>
        `;
        tbody.appendChild(row);
    });
}

async function clearWritebackCache(path) {
    if (!confirm('Apakah Anda yakin ingin menghapus file cache writeback ini? PC klien harus offline!')) return;
    
    const res = await apiPost('/api/writeback/clear', { file_path: path });
    if (res) {
        showToast('Cache berhasil dihapus.', 'success');
        loadWritebackFiles();
    } else {
        showToast('Gagal menghapus cache.', 'error');
    }
}

// Central Settings JSON mapping
async function loadConfigJson() {
    await loadNetworkInterfaces();
    const data = await apiGet('/api/config/json');
    if (data) {
        configObj = data;
        
        // Map elements to centralized form inputs
        let addrVal = '0.0.0.0';
        if (typeof data.server.address === 'string') {
            addrVal = data.server.address;
        } else if (Array.isArray(data.server.address)) {
            addrVal = data.server.address[0] || '0.0.0.0';
        }
        
        populateIpDropdowns();

        const serverAddrEl = document.getElementById('set-server-address');
        if (serverAddrEl) serverAddrEl.value = addrVal;
        
        const serverPortEl = document.getElementById('set-server-port');
        if (serverPortEl) serverPortEl.value = data.server.port;
        
        const serverCacheEl = document.getElementById('set-server-cache');
        if (serverCacheEl) serverCacheEl.value = data.server.read_cache_gb;
        
        const gdIqnEl = document.getElementById('set-gamedisk-iqn');
        if (gdIqnEl) gdIqnEl.value = data.gamedisk_target.target_iqn;

        // DHCP Inputs
        if (data.dhcp) {
            const dhcpEnabledEl = document.getElementById('set-dhcp-enabled');
            if (dhcpEnabledEl) dhcpEnabledEl.checked = data.dhcp.enabled;
            
            const startIpEl = document.getElementById('set-dhcp-start-ip');
            if (startIpEl) startIpEl.value = data.dhcp.start_ip || '';
            
            const endIpEl = document.getElementById('set-dhcp-end-ip');
            if (endIpEl) endIpEl.value = data.dhcp.end_ip || '';
            
            const maskEl = document.getElementById('set-dhcp-mask');
            if (maskEl) maskEl.value = data.dhcp.subnet_mask || '';
            
            const gwEl = document.getElementById('set-dhcp-gateway');
            if (gwEl) gwEl.value = data.dhcp.router || '';
            
            const dnsEl = document.getElementById('set-dhcp-dns');
            if (dnsEl) dnsEl.value = data.dhcp.dns || '';
            
            const nextEl = document.getElementById('set-dhcp-next');
            if (nextEl) nextEl.value = data.dhcp.next_server || '';
            
            const tftpDirEl = document.getElementById('set-tftp-dir');
            if (tftpDirEl) tftpDirEl.value = data.dhcp.tftp_dir || '';
            
            const pxeDefEl = document.getElementById('set-pxe-default');
            if (pxeDefEl) pxeDefEl.value = data.dhcp.pxe_default || '';
            
            serverNicIps = data.dhcp.nic_ips || [];
            renderNicIpsList();
        }

        if (data.writeback) {
            const maxCacheEl = document.getElementById('disk-max-cache-gb');
            if (maxCacheEl) maxCacheEl.value = data.writeback.max_cache_per_client_gb;
            
            const maxSpeedEl = document.getElementById('disk-max-speed-mbps');
            if (maxSpeedEl) maxSpeedEl.value = data.writeback.max_write_speed_mbps;
        }

        // Render disk cards
        populateSystemDrives();

        // Render VHD Manager sub-tab using this loaded config
        renderVhdTable();
    }
}

let systemDrivesList = [];

async function populateSystemDrives() {
    if (!configObj) return;
    const container = document.getElementById('disk-grid-container');
    if (!container) return;

    if (systemDrivesList.length === 0) {
        container.innerHTML = '<span class="text-[#5f5f5d] text-sm py-4">Memuat disk...</span>';
        const detailList = await apiGet('/api/system/logical_drives_detail');
        if (detailList && Array.isArray(detailList)) {
            systemDrivesList = detailList;
        }
    }

    if (systemDrivesList.length === 0) {
        container.innerHTML = '<span class="text-[#5f5f5d] text-sm py-4">Gagal mendeteksi drive sistem.</span>';
        return;
    }

    container.innerHTML = '';
    systemDrivesList.forEach(d => {
            // Check active flags in configObj
            const isBoot = configObj.windows?.vhd_dir?.substring(0, 1).toUpperCase() === d.letter;
            const isWb = configObj.writeback?.writeback_dirs?.some(dir => dir.substring(0, 1).toUpperCase() === d.letter) || false;
            
            let isGd = false;
            if (d.physical_disk && configObj.gamedisk) {
                isGd = configObj.gamedisk.some(gd => gd.physical_disk === d.physical_disk);
            }

            const card = document.createElement('div');
            card.className = 'bg-[#fcfbf8] border border-[#eceae4] hover:border-[#1c1c1c]/40 rounded-xl p-6 text-center cursor-pointer transition-all hover:-translate-y-0.5 shadow-xs flex flex-col items-center justify-between';

            let badgeHTML = '';
            if (isBoot) {
                badgeHTML += `<span class="inline-flex items-center text-xs font-medium px-2.5 py-1 rounded-md bg-blue-50 text-blue-800 border border-blue-200">💿 Boot VHD</span>`;
            }
            if (isWb) {
                badgeHTML += `<span class="inline-flex items-center text-xs font-medium px-2.5 py-1 rounded-md bg-amber-50 text-amber-800 border border-amber-200">💾 Writeback</span>`;
            }
            if (isGd) {
                badgeHTML += `<span class="inline-flex items-center text-xs font-medium px-2.5 py-1 rounded-md bg-purple-50 text-purple-800 border border-purple-200">🎮 Raw GameDisk</span>`;
            }
            if (badgeHTML === '') {
                badgeHTML = `<span class="inline-flex items-center text-xs font-medium px-2.5 py-1 rounded-md bg-gray-100 text-gray-700 border border-gray-200">Unallocated</span>`;
            }

            card.innerHTML = `
                <div class="text-4xl mb-3">💽</div>
                <h3 class="font-['Outfit'] font-bold text-base text-[#1c1c1c] mb-3">Disk ${d.letter}:</h3>
                <div class="flex flex-wrap justify-center gap-2">
                    ${badgeHTML}
                </div>
            `;

            card.addEventListener('click', () => openDiskConfigModal(d.letter, isBoot, isWb, isGd, d.physical_disk));
            container.appendChild(card);
        });
}

function openDiskConfigModal(letter, isBoot, isWb, isGd, physicalDisk) {
    const modal = document.getElementById('disk-config-modal');
    modal.style.display = 'flex';

    document.getElementById('disk-config-title').textContent = `Alokasi Disk ${letter}:\\`;
    document.getElementById('disk-config-letter').value = letter;

    document.getElementById('disk-cfg-vhd').checked = isBoot;
    document.getElementById('disk-cfg-wb').checked = isWb;

    const gdCheckbox = document.getElementById('disk-cfg-gd');
    gdCheckbox.checked = isGd;
    if (physicalDisk) {
        gdCheckbox.disabled = false;
        gdCheckbox.dataset.physicalDisk = physicalDisk;
    } else {
        gdCheckbox.checked = false;
        gdCheckbox.disabled = true;
    }
}

function closeDiskConfigModal() {
    document.getElementById('disk-config-modal').style.display = 'none';
}

async function applyDiskConfigAction(e) {
    e.preventDefault();
    const letter = document.getElementById('disk-config-letter').value;
    
    // 1. Boot VHD allocation
    const isBootChecked = document.getElementById('disk-cfg-vhd').checked;
    if (isBootChecked) {
        if (!configObj.windows) {
            configObj.windows = {
                target_iqn_prefix: 'iqn.2024-01.com.tmdebug:vhd-',
                vhd_dir: '',
                block_size: 512,
                vendor_id: 'RUSTISCS',
                product_id: 'WindowsBoot',
                product_revision: '1.00',
                discovery: false,
                super_client_ip: '',
                super_client_action: 'none'
            };
        }
        configObj.windows.vhd_dir = `${letter}:\\vhd`;
    } else {
        if (configObj.windows?.vhd_dir?.substring(0, 1).toUpperCase() === letter) {
            configObj.windows.vhd_dir = '';
        }
    }

    // 2. Writeback Cache allocation
    const isWbChecked = document.getElementById('disk-cfg-wb').checked;
    const wbPath = `${letter}:\\writeback`;
    if (!configObj.writeback.writeback_dirs) {
        configObj.writeback.writeback_dirs = [];
    }
    if (isWbChecked) {
        if (!configObj.writeback.writeback_dirs.includes(wbPath)) {
            configObj.writeback.writeback_dirs.push(wbPath);
        }
    } else {
        configObj.writeback.writeback_dirs = configObj.writeback.writeback_dirs.filter(dir => dir !== wbPath);
    }

    // 3. Gamedisk allocation
    const gdCheckbox = document.getElementById('disk-cfg-gd');
    const physicalDisk = gdCheckbox.dataset.physicalDisk;
    const isGdChecked = gdCheckbox.checked;

    if (!configObj.gamedisk) {
        configObj.gamedisk = [];
    }

    if (physicalDisk) {
        if (isGdChecked) {
            const alreadyExists = configObj.gamedisk.some(gd => gd.physical_disk === physicalDisk);
            if (!alreadyExists) {
                const diskNum = physicalDisk.replace(/\D/g, "");
                configObj.gamedisk.push({
                    physical_disk: physicalDisk,
                    block_size: 512,
                    vendor_id: "RUSTISCS",
                    product_id: `GameDisk-${diskNum}`,
                    product_revision: "1.00"
                });
            }
        } else {
            configObj.gamedisk = configObj.gamedisk.filter(gd => gd.physical_disk !== physicalDisk);
        }
    }

    await saveConfigJsonFull();
    closeDiskConfigModal();
}

async function saveDiskMgmtGlobals() {
    configObj.writeback.max_cache_per_client_gb = parseInt(document.getElementById('disk-max-cache-gb').value);
    configObj.writeback.max_write_speed_mbps = parseInt(document.getElementById('disk-max-speed-mbps').value);
    await saveConfigJsonFull();
}

async function saveConfigJson(e) {
    if (e) e.preventDefault();

    if (!configObj) configObj = {};
    if (!configObj.server) configObj.server = {};
    if (!configObj.gamedisk_target) configObj.gamedisk_target = {};

    // Reconstruct nested configObj structures
    configObj.server.address = document.getElementById('set-server-address').value || '0.0.0.0';
    configObj.server.port = parseInt(document.getElementById('set-server-port').value) || 3260;
    configObj.server.read_cache_gb = parseInt(document.getElementById('set-server-cache').value) || 4;
    configObj.gamedisk_target.target_iqn = document.getElementById('set-gamedisk-iqn').value || 'iqn.2024-01.com.tmdebug:gamedisks';

    if (!configObj.dhcp) {
        configObj.dhcp = {
            enabled: false,
            start_ip: '',
            end_ip: null,
            router: '',
            dns: '',
            next_server: '',
            subnet_mask: '',
            tftp_dir: '',
            pxe_default: null
        };
    }

    configObj.dhcp.enabled = document.getElementById('set-dhcp-enabled').checked;
    configObj.dhcp.start_ip = document.getElementById('set-dhcp-start-ip').value.trim();
    const endIp = document.getElementById('set-dhcp-end-ip').value.trim();
    configObj.dhcp.end_ip = endIp.length > 0 ? endIp : null;
    configObj.dhcp.subnet_mask = document.getElementById('set-dhcp-mask').value.trim();
    configObj.dhcp.router = document.getElementById('set-dhcp-gateway').value.trim();
    configObj.dhcp.dns = document.getElementById('set-dhcp-dns').value.trim();
    configObj.dhcp.next_server = document.getElementById('set-dhcp-next').value.trim();
    configObj.dhcp.tftp_dir = document.getElementById('set-tftp-dir').value.trim();
    const pxeDef = document.getElementById('set-pxe-default').value.trim();
    configObj.dhcp.pxe_default = pxeDef.length > 0 ? pxeDef : null;
    
    configObj.dhcp.nic_ips = serverNicIps;

    const success = await apiPost('/api/config/json', configObj);
    if (success) {
        await loadConfigJson();
        showToast('✅ Semua pengaturan berhasil disimpan & server di-reload!', 'success');
    } else {
        showToast('❌ Gagal menyimpan pengaturan!', 'error');
    }
}

async function saveConfigJsonFull() {
    const success = await apiPost('/api/config/json', configObj);
    if (success) {
        await loadConfigJson();
        showToast('Pengaturan sukses disimpan!', 'success');
    }
}

// Redirect helpers for snapshot details
async function showVhdSnapshots(key) {
    const modal = document.getElementById('vhd-snapshots-modal');
    if (!modal) return;
    modal.style.display = 'flex';
    document.getElementById('snapshot-modal-title-key').textContent = key;

    const tbody = document.getElementById('vhd-snapshots-tbody');
    tbody.innerHTML = `<tr><td colspan="3" class="py-12 px-6 text-center">
        <div class="animate-pulse flex flex-col gap-3">
            <div class="h-4 bg-[rgba(28,28,28,0.1)] rounded w-3/4 mx-auto"></div>
            <div class="h-4 bg-[rgba(28,28,28,0.1)] rounded w-1/2 mx-auto"></div>
        </div>
    </td></tr>`;

    const data = await apiGet(`/api/vhd/backups?image_key=${key}`);
    if (data && Array.isArray(data) && data.length > 0) {
        tbody.innerHTML = '';
        data.forEach(snapshot => {
            const row = document.createElement('tr');
            row.className = "hover:bg-[#1c1c1c]/[0.02] transition-colors";
            const filename = snapshot.path.split(/[/\\]/).pop();
            row.innerHTML = `
                <td class="py-4 px-5 font-semibold text-[#1c1c1c]">Snapshot #${snapshot.index}</td>
                <td class="py-4 px-5 font-mono text-xs text-[#5f5f5d]" title="${snapshot.path}">${filename}</td>
                <td class="py-4 px-5" style="text-align: right;">
                    <button class="inline-flex items-center justify-center px-3 py-1.5 text-xs font-medium rounded-md bg-[#1c1c1c] text-[#fcfbf8] hover:bg-[#1c1c1c]/90 transition-all shadow-xs" onclick="restoreSnapshotAction('${key}', ${snapshot.index})">🔄 Restore</button>
                </td>
            `;
            tbody.appendChild(row);
        });
    } else {
        tbody.innerHTML = '<tr><td colspan="3" class="py-8 px-5 text-center text-[#5f5f5d]">Tidak ada snapshot (backup) untuk image ini.</td></tr>';
    }
}

function closeVhdSnapshotsModal() {
    document.getElementById('vhd-snapshots-modal').style.display = 'none';
}

async function restoreSnapshotAction(imageKey, index) {
    if (!confirm(`Apakah Anda yakin ingin merestore VHD '${imageKey}' ke Snapshot #${index}?\nSemua data saat ini pada VHD tersebut akan digantikan oleh snapshot ini.`)) {
        return;
    }

    const res = await apiPost('/api/vhd/restore', {
        image_key: imageKey,
        index: index
    });

    if (res) {
        showToast(`Sukses merestore VHD '${imageKey}' ke Snapshot #${index}!`, 'success');
        closeVhdSnapshotsModal();
        renderVhdTable();
    } else {
        showToast('Gagal merestore snapshot.', 'error');
    }
}

// Context Menu Klien (Klik Kanan)
let selectedClientForCtx = null;
function initContextMenus() {
    document.addEventListener('click', () => hideContextMenu());
}

function showContextMenu(e, client) {
    selectedClientForCtx = client;
    const menu = document.getElementById('clients-context-menu');
    menu.style.display = 'block';
    menu.style.left = `${e.pageX}px`;
    menu.style.top = `${e.pageY}px`;

    const ctxEnable = document.getElementById('ctx-enable-super');
    const ctxDisable = document.getElementById('ctx-disable-super');

    const isCurrentSuper = configObj && configObj.windows && configObj.windows.super_client_ip === client.ip;
    const hasAnySuperClient = configObj && configObj.windows && configObj.windows.super_client_ip && configObj.windows.super_client_ip.trim() !== "";
    const hasBootVhd = client.image_manager && client.image_manager.trim() !== "";

    if (client.active || !hasBootVhd) {
        // Cannot enable if PC is online OR doesn't have a valid VHD boot image
        ctxEnable.classList.add('disabled');
        ctxDisable.classList.add('disabled');
    } else {
        if (isCurrentSuper) {
            ctxEnable.classList.add('disabled');
            ctxDisable.classList.remove('disabled');
        } else {
            if (hasAnySuperClient) {
                ctxEnable.classList.add('disabled');
            } else {
                ctxEnable.classList.remove('disabled');
            }
            ctxDisable.classList.add('disabled');
        }
    }
}

function hideContextMenu() {
    document.getElementById('clients-context-menu').style.display = 'none';
}

async function ctxEnableSuperClient() {
    if (!selectedClientForCtx) return;
    const res = await apiPost('/api/superclient/set', {
        ip: selectedClientForCtx.ip,
        action: 'commit'
    });
    if (res) {
        showToast('Super client diaktifkan.', 'success');
        // It will automatically update via SSE stream
    }
}

function ctxDisableSuperClientPrompt() {
    if (!selectedClientForCtx) return;
    document.getElementById('confirm-modal').style.display = 'flex';
}

function closeModal() {
    document.getElementById('confirm-modal').style.display = 'none';
}

async function modalAction(action) {
    const isCommit = action === 'commit';
    const createBackupCheckbox = document.getElementById('chk-create-backup');
    const createBackup = createBackupCheckbox ? createBackupCheckbox.checked : true;
    closeModal();
    if (!selectedClientForCtx) return;
    
    const endpoint = isCommit ? '/api/superclient/commit' : '/api/superclient/discard';
    const payload = isCommit 
        ? { hostname: selectedClientForCtx.ip, create_backup: createBackup }
        : { hostname: selectedClientForCtx.ip };
    const res = await apiPost(endpoint, payload);
    if (res) {
        showToast(isCommit ? 'Perubahan super client berhasil di-commit!' : 'Perubahan super client berhasil dibuang.', 'success');
    }
}

// Formatter Helpers
function formatBytes(bytes) {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}

function formatDuration(secs) {
    if (secs < 60) return `${secs}s`;
    const mins = Math.floor(secs / 60);
    const s = secs % 60;
    if (mins < 60) return `${mins}m ${s}s`;
    const hrs = Math.floor(mins / 60);
    const m = mins % 60;
    return `${hrs}h ${m}m`;
}

function formatSpeed(bytesPerSec) {
    if (bytesPerSec <= 0) return '0 B/s';
    const k = 1024;
    const sizes = ['B/s', 'KB/s', 'MB/s', 'GB/s'];
    const i = Math.floor(Math.log(bytesPerSec) / Math.log(k));
    return parseFloat((bytesPerSec / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

// TFTP Folder CRUD Handlers
async function loadTftpFolders() {
    const folders = await apiGet('/api/system/tftp_folders');
    const tbody = document.getElementById('tftp-folders-tbody');
    const datalist = document.getElementById('tftp-folders-list');
    
    if (datalist) {
        datalist.innerHTML = '';
        if (folders && Array.isArray(folders)) {
            folders.forEach(f => {
                const opt = document.createElement('option');
                opt.value = f;
                datalist.appendChild(opt);
            });
        }
    }
    
    if (!tbody) return;
    
    if (folders && Array.isArray(folders)) {
        if (folders.length === 0) {
            tbody.innerHTML = '<tr><td colspan="2" class="py-6 px-5 text-center text-[#5f5f5d]">Tidak ada folder boot loader terdaftar.</td></tr>';
            return;
        }
        tbody.innerHTML = '';
        folders.forEach(f => {
            const row = document.createElement('tr');
            row.className = "hover:bg-[#1c1c1c]/[0.02] transition-colors";
            row.innerHTML = `
                <td class="py-4 px-5 font-semibold text-[#1c1c1c]">${f}</td>
                <td class="py-4 px-5" style="text-align: right;">
                    <button class="inline-flex items-center justify-center px-3 py-1.5 text-xs font-medium rounded-md bg-transparent text-rose-600 border border-rose-600/40 hover:bg-rose-50/50 transition-all" onclick="deleteTftpFolderAction('${f}')">🗑️ Hapus</button>
                </td>
            `;
            tbody.appendChild(row);
        });
    } else {
        tbody.innerHTML = '<tr><td colspan="2" class="py-6 px-5 text-center text-[#5f5f5d]">Gagal memuat folder TFTP (Periksa konfigurasi TFTP root directory).</td></tr>';
    }
}

async function createNewTftpFolderPrompt() {
    const name = prompt('Nama folder boot loader baru (TFTP):');
    if (!name || name.trim() === '') return;
    
    const success = await apiPost('/api/system/tftp_folders/create', { name: name.trim() });
    if (success) {
        await loadTftpFolders();
        showToast('Folder boot loader berhasil dibuat!', 'success');
    } else {
        showToast('Gagal membuat folder boot loader.', 'error');
    }
}

async function deleteTftpFolderAction(name) {
    if (!confirm(`Apakah Anda yakin ingin menghapus folder boot loader "${name}" beserta seluruh file di dalamnya?`)) return;
    
    const success = await apiPost('/api/system/tftp_folders/delete', { name });
    if (success) {
        await loadTftpFolders();
        showToast('Folder boot loader berhasil dihapus!', 'success');
    } else {
        showToast('Gagal menghapus folder boot loader.', 'error');
    }
}

let serverNicIps = [];

function renderNicIpsList() {
    const container = document.getElementById('nic-ips-list-container');
    if (!container) return;

    if (serverNicIps.length === 0) {
        container.innerHTML = '<span class="text-[#5f5f5d] text-center text-xs py-1">Belum ada IP ditambahkan.</span>';
        return;
    }

    container.innerHTML = '';
    serverNicIps.forEach((ip, idx) => {
        const row = document.createElement('div');
        row.className = 'flex justify-between items-center bg-[#f7f4ed] px-3.5 py-2 rounded-md border border-[#eceae4] font-mono text-xs text-[#1c1c1c]';
        row.innerHTML = `
            <span>🌐 ${ip}</span>
            <button type="button" class="inline-flex items-center justify-center px-2.5 py-1 text-xs font-medium rounded-md bg-transparent text-rose-600 border border-rose-600/40 hover:bg-rose-50/50 transition-all" onclick="removeNicIpAction(${idx})">🗑️ Remove</button>
        `;
        container.appendChild(row);
    });
}

function addNicIpAction() {
    const input = document.getElementById('add-nic-ip-input');
    if (!input) return;
    const ip = input.value.trim();
    if (!ip) return;
    
    const ipRegex = /^(?:[0-9]{1,3}\.){3}[0-9]{1,3}$/;
    if (!ipRegex.test(ip)) {
        showToast('Format alamat IP tidak valid!', 'error');
        return;
    }

    if (serverNicIps.includes(ip)) {
        showToast('Alamat IP ini sudah terdaftar!', 'error');
        return;
    }

    serverNicIps.push(ip);
    input.value = '';
    renderNicIpsList();
}

function removeNicIpAction(idx) {
    serverNicIps.splice(idx, 1);
    renderNicIpsList();
}

async function autoAllocateNextServerIpsAction() {
    const nics = (configObj && configObj.dhcp && configObj.dhcp.nic_ips) || [];

    if (nics.length === 0) {
        showToast('Gagal melakukan alokasi otomatis: Silakan isi daftar IP adapter jaringan (Load Balancing NIC IPs) di halaman Pengaturan terlebih dahulu!', 'error');
        return;
    }

    if (!clientsObj || !Array.isArray(clientsObj.client) || clientsObj.client.length === 0) {
        showToast('Tidak ada klien terdaftar untuk dialokasikan.', 'error');
        return;
    }

    if (!confirm(`Apakah Anda yakin ingin membagi ${clientsObj.client.length} klien secara merata (Load Balancing) ke ${nics.length} IP adapter server berikut?\n${nics.join(', ')}`)) {
        return;
    }

    clientsObj.client.forEach((c, index) => {
        c.next_server = nics[index % nics.length];
    });

    await saveClientsJson();
    showToast(`Sukses membagi rata ${clientsObj.client.length} klien ke ${nics.length} adapter IP server!`, 'success');
}

// Mobile Sidebar Toggle Logic
document.addEventListener('DOMContentLoaded', () => {
    const mobileMenuBtn = document.getElementById('mobile-menu-btn');
    const sidebar = document.querySelector('.sidebar');
    const sidebarOverlay = document.getElementById('sidebar-overlay');

    if (mobileMenuBtn && sidebar && sidebarOverlay) {
        function toggleSidebar() {
            const isOpen = sidebar.classList.contains('translate-x-0');
            if (isOpen) {
                sidebar.classList.remove('translate-x-0');
                sidebar.classList.add('-translate-x-full');
                sidebarOverlay.classList.add('hidden');
            } else {
                sidebar.classList.remove('-translate-x-full');
                sidebar.classList.add('translate-x-0');
                sidebarOverlay.classList.remove('hidden');
            }
        }

        mobileMenuBtn.addEventListener('click', toggleSidebar);
        sidebarOverlay.addEventListener('click', toggleSidebar);

        // Close sidebar when a nav item is clicked on mobile
        const navItems = document.querySelectorAll('.nav-item');
        navItems.forEach(item => {
            item.addEventListener('click', () => {
                if (window.innerWidth <= 768 && sidebar.classList.contains('translate-x-0')) {
                    toggleSidebar();
                }
            });
        });
    }
});

// Toast Notification System
function showToast(message, type = 'info') {
    const container = document.getElementById('toast-container');
    if (!container) return;

    const toast = document.createElement('div');
    toast.className = 'toast-msg';
    
    // Lovable-style colors based on type
    if (type === 'success') toast.style.cssText = 'background: #22c55e; color: white;';
    else if (type === 'error') toast.style.cssText = 'background: #ef4444; color: white;';
    else if (type === 'warning') toast.style.cssText = 'background: #f59e0b; color: white;';
    else toast.style.cssText = 'background: var(--color-text); color: var(--color-bg);';

    let icon = 'ℹ️';
    if (type === 'success') icon = '✅';
    if (type === 'error') icon = '❌';
    if (type === 'warning') icon = '⚠️';

    toast.innerHTML = `<span class="toast-icon">${icon}</span><span class="toast-content">${message}</span>`;
    container.appendChild(toast);

    // Animate in
    requestAnimationFrame(() => {
        requestAnimationFrame(() => {
            toast.classList.add('show');
        });
    });

    // Auto dismiss
    setTimeout(() => {
        toast.classList.remove('show');
        setTimeout(() => toast.remove(), 400); // match transition duration
    }, 4500); // give it a bit more time to read full info
}

