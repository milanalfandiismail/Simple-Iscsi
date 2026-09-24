// Simple Iscsi - High Performance Diskless Boot & Storage Manager
// Genesis Design System Core Controller

// Global App State
let activeTab = 'dashboard';
let stats = {};
let configObj = null; // Parsed config.toml representation
let clientsObj = { client: [] }; // Parsed clients.toml representation
let activeSessionsMap = new Map();
let clientSpeedHistory = new Map();
let availableNetworkIps = [];
let renderedDashboardIps = [];
let selectedContextClient = null;
let confirmCallback = null;

// App Lifecycle Initializer
document.addEventListener('DOMContentLoaded', async () => {
    initTheme();
    initContextMenus();

    // Initial Data Sequence
    await loadInitialData();

    // Start Live Stats Polling Loop (1-second intervals)
    initStatsStream();

    // Background Auto-sync
    initAutoSyncIntervals();
});

// Theme Controller (Light / Dark Mode)
function initTheme() {
    const btn = document.getElementById('theme-toggle-btn');
    if (!btn) return;

    const applyTheme = (theme) => {
        if (theme === 'dark') {
            document.documentElement.classList.add('dark');
            document.body.classList.add('dark');
            btn.textContent = '🌞 Mode Terang';
        } else {
            document.documentElement.classList.remove('dark');
            document.body.classList.remove('dark');
            btn.textContent = '🌙 Mode Gelap';
        }
    };

    const savedTheme = localStorage.getItem('theme') || 'light';
    applyTheme(savedTheme);

    btn.addEventListener('click', () => {
        const isDark = document.documentElement.classList.contains('dark');
        const nextTheme = isDark ? 'light' : 'dark';
        localStorage.setItem('theme', nextTheme);
        applyTheme(nextTheme);
    });
}

// Navigation Tabs Controller
function switchTab(tabId, btnEl = null) {
    activeTab = tabId;

    // Toggle panels
    document.querySelectorAll('.tab-panel').forEach(panel => {
        panel.classList.remove('active');
    });
    const activePanel = document.getElementById(`tab-${tabId}`);
    if (activePanel) activePanel.classList.add('active');

    // Toggle nav item styles
    document.querySelectorAll('.nav-item').forEach(btn => {
        btn.classList.remove('active', 'bg-indigo-50', 'text-indigo-600', 'border', 'border-indigo-200/60', 'font-semibold');
        btn.classList.add('text-stone-600', 'hover:bg-stone-100', 'hover:text-stone-900');
    });

    if (btnEl) {
        btnEl.classList.add('active', 'bg-indigo-50', 'text-indigo-600', 'border', 'border-indigo-200/60', 'font-semibold');
        btnEl.classList.remove('text-stone-600', 'hover:bg-stone-100', 'hover:text-stone-900');
    }

    // Auto close mobile drawer if open
    const navContainer = document.getElementById('sidebar-nav');
    if (window.innerWidth < 1024 && navContainer && !navContainer.classList.contains('hidden')) {
        navContainer.classList.add('hidden');
    }
}

function toggleMobileNav() {
    const navContainer = document.getElementById('sidebar-nav');
    if (navContainer) {
        navContainer.classList.toggle('hidden');
    }
}

// Initial Data Loader
async function loadInitialData() {
    try {
        await loadNetworkInterfaces();
    } catch (err) {
        console.error('loadNetworkInterfaces error:', err);
    }

    try {
        await loadConfigJson();
    } catch (err) {
        console.error('loadConfigJson error:', err);
    }

    try {
        await loadClientsJson();
    } catch (err) {
        console.error('loadClientsJson error:', err);
    }

    try {
        loadWritebackFiles();
    } catch (err) {
        console.error('loadWritebackFiles error:', err);
    }
}

function initAutoSyncIntervals() {
    setInterval(() => {
        if (activeTab === 'disk-mgmt') {
            loadDiskPartitions();
            loadWritebackFiles();
        }
    }, 5000);
}

// HTTP API Fetch Helpers
async function apiGet(url, timeoutMs = 3500) {
    try {
        const controller = new AbortController();
        const timeoutId = setTimeout(() => controller.abort(), timeoutMs);
        const res = await fetch(url, { signal: controller.signal });
        clearTimeout(timeoutId);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return await res.json();
    } catch (err) {
        console.error(`GET ${url} failed:`, err);
        return null;
    }
}

async function apiPost(url, body = {}, timeoutMs = 4500) {
    try {
        const controller = new AbortController();
        const timeoutId = setTimeout(() => controller.abort(), timeoutMs);
        const res = await fetch(url, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: typeof body === 'string' ? body : JSON.stringify(body),
            signal: controller.signal
        });
        clearTimeout(timeoutId);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const contentType = res.headers.get('content-type');
        if (contentType && contentType.includes('application/json')) {
            return await res.json();
        }
        const text = await res.text();
        return { status: 'ok', message: text };
    } catch (err) {
        console.error(`POST ${url} failed:`, err);
        return null;
    }
}

// Formatters & DOM Helpers
function formatBytes(bytes) {
    if (!bytes || bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

function formatSpeed(bytesPerSec) {
    if (!bytesPerSec || bytesPerSec === 0) return '0 B/s';
    const k = 1024;
    const sizes = ['B/s', 'KB/s', 'MB/s', 'GB/s'];
    const i = Math.floor(Math.log(bytesPerSec) / Math.log(k));
    return parseFloat((bytesPerSec / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

function formatDuration(seconds) {
    if (!seconds || seconds <= 0) return '0s';
    const d = Math.floor(seconds / 86400);
    const h = Math.floor((seconds % 86400) / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = Math.floor(seconds % 60);

    if (d > 0) return `${d}d ${h}h`;
    if (h > 0) return `${h}h ${m}m`;
    if (m > 0) return `${m}m ${s}s`;
    return `${s}s`;
}

function setTextIfChanged(el, text) {
    if (el && el.textContent !== text) {
        el.textContent = text;
    }
}

function setHtmlIfChanged(el, html) {
    if (el && el.innerHTML !== html) {
        el.innerHTML = html;
    }
}

function showToast(message, type = 'info') {
    const container = document.getElementById('toast-container');
    if (!container) return;

    const toast = document.createElement('div');
    const colorClasses = {
        success: 'bg-emerald-600 text-white shadow-lg shadow-emerald-600/20',
        error: 'bg-rose-600 text-white shadow-lg shadow-rose-600/20',
        warning: 'bg-amber-600 text-white shadow-lg shadow-amber-600/20',
        info: 'bg-stone-900 text-white shadow-lg shadow-stone-900/20'
    }[type] || 'bg-stone-900 text-white';

    toast.className = `toast-msg flex items-center gap-2.5 px-4 py-3 rounded-lg text-xs sm:text-sm font-medium ${colorClasses}`;
    toast.innerHTML = `<span>${type === 'success' ? '✅' : type === 'error' ? '❌' : type === 'warning' ? '⚠️' : 'ℹ️'}</span><span>${message}</span>`;

    container.appendChild(toast);

    requestAnimationFrame(() => {
        toast.classList.add('show');
    });

    setTimeout(() => {
        toast.classList.remove('show');
        setTimeout(() => toast.remove(), 350);
    }, 3500);
}

// Server Network Interfaces
async function loadNetworkInterfaces() {
    const data = await apiGet('/api/system/network_interfaces');
    if (data && Array.isArray(data)) {
        availableNetworkIps = data;
        populateNetworkDropdowns();
    }
}

function populateNetworkDropdowns() {
    const serverSelect = document.getElementById('set-server-address');
    const datalist = document.getElementById('network-ips-datalist');

    if (serverSelect) {
        serverSelect.innerHTML = '';
        serverSelect.add(new Option('0.0.0.0 (Semua Interface)', '0.0.0.0'));
    }
    if (datalist) datalist.innerHTML = '';

    availableNetworkIps.forEach(ip => {
        if (ip !== '0.0.0.0') {
            if (serverSelect) serverSelect.add(new Option(ip, ip));
        }
        if (datalist) {
            const opt = document.createElement('option');
            opt.value = ip;
            datalist.appendChild(opt);
        }
    });

    if (configObj && configObj.server && configObj.server.address && serverSelect) {
        const addr = Array.isArray(configObj.server.address) ? configObj.server.address[0] : configObj.server.address;
        serverSelect.value = addr || '0.0.0.0';
    }
}

// Live Stats Polling Loop & Realtime Client Sync
function initStatsStream() {
    let clientsSyncCounter = 0;

    const fetchStats = async () => {
        try {
            const data = await apiGet('/api/stats');
            if (data) {
                handleStatsData(data);
            } else {
                updateServiceCard('iscsi', { enabled: false, port: 0 });
                updateServiceCard('dhcp', { enabled: false, port: 0 });
                updateServiceCard('tftp', { enabled: false, port: 0 });
            }
        } catch (e) {
            console.error('Stats poll error:', e);
        }

        // Realtime sync clients.toml every 2 seconds to instantly capture auto-added clients
        clientsSyncCounter++;
        if (clientsSyncCounter >= 2) {
            clientsSyncCounter = 0;
            refreshClientsDataSilently();
        }
    };

    fetchStats();
    setInterval(fetchStats, 1000);
}

// Background silent client sync
async function refreshClientsDataSilently() {
    try {
        const data = await apiGet('/api/clients/json');
        if (data && Array.isArray(data.client)) {
            const oldStr = JSON.stringify(clientsObj ? clientsObj.client : []);
            const newStr = JSON.stringify(data.client);
            if (oldStr !== newStr) {
                clientsObj = data;
                renderClientsManagerTable();
                renderDashboardClientsTable();
            }
        }
    } catch (err) {
        console.error('refreshClientsDataSilently error:', err);
    }
}

function handleStatsData(data) {
    if (!data) return;
    stats = data;

    // 1. Update Service Cards
    if (data.services) {
        updateServiceCard('iscsi', data.services.iscsi);
        updateServiceCard('dhcp', data.services.dhcp);
        updateServiceCard('tftp', data.services.tftp);
    }

    // 2. Active Connections Counter
    const connsEl = document.getElementById('stat-conns');
    if (connsEl) connsEl.textContent = data.active_sessions || 0;

    // Track sessions map
    activeSessionsMap.clear();
    if (data.clients) {
        data.clients.forEach(c => {
            activeSessionsMap.set(c.ip, c);
        });
    }

    const mergedClients = getMergedDashboardClients();
    const currentIps = mergedClients.map(c => c.ip).join(',');

    // Re-render table structure if client list changed
    if (currentIps !== renderedDashboardIps.join(',')) {
        renderDashboardClientsTable();
    }

    // 3. Zero-Flicker Cell Upgrades
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

            const statusText = statsInfo.active ? '🟢 Online' : '🔴 Offline';
            const statusClass = `client-status-badge inline-flex items-center gap-1 text-[11px] font-semibold px-2 py-0.5 rounded-full ${statsInfo.active ? 'bg-emerald-50 text-emerald-700 border border-emerald-200' : 'bg-rose-50 text-rose-700 border border-rose-200'}`;

            const updateRowStats = (row) => {
                if (!row) return;
                const badge = row.querySelector('.client-status-badge');
                if (badge && badge.textContent !== statusText) {
                    badge.textContent = statusText;
                    badge.className = statusClass;
                }
                const readTotalEl = row.querySelector('.client-read-total');
                if (readTotalEl) setTextIfChanged(readTotalEl, formatBytes(statsInfo.bytes_read));

                const readSpeedEl = row.querySelector('.client-read-speed');
                if (readSpeedEl) setTextIfChanged(readSpeedEl, `⚡ ${formatSpeed(speedInfo.readSpeed)}`);

                const writeTotalEl = row.querySelector('.client-write-total');
                if (writeTotalEl) setTextIfChanged(writeTotalEl, formatBytes(statsInfo.bytes_written));

                const writeSpeedEl = row.querySelector('.client-write-speed');
                if (writeSpeedEl) setTextIfChanged(writeSpeedEl, `⚡ ${formatSpeed(speedInfo.writeSpeed)}`);

                const uptimeEl = row.querySelector('.client-uptime');
                if (uptimeEl) {
                    setTextIfChanged(uptimeEl, statsInfo.active ? formatDuration(statsInfo.uptime_secs) : 'Offline');
                    uptimeEl.className = `client-uptime text-xs font-medium ${statsInfo.active ? 'text-stone-900' : 'text-stone-400'}`;
                }
            };

            updateRowStats(document.querySelector(`#dashboard-clients-tbody tr[data-ip="${c.ip}"]`));
            updateRowStats(document.querySelector(`#clients-tbody tr[data-ip="${c.ip}"]`));
        });
    });
}

function updateServiceCard(name, service) {
    const portEl = document.getElementById(`${name}-port-info`);
    const pillEl = document.getElementById(`${name}-status-pill`);
    if (portEl && pillEl) {
        portEl.textContent = `Port: ${service.port || 0}`;
        if (service.enabled) {
            pillEl.textContent = '🟢 Enabled';
            pillEl.className = 'pill-status text-[11px] sm:text-xs font-semibold px-2.5 py-1 rounded-full bg-emerald-50 text-emerald-700 border border-emerald-200 whitespace-nowrap shrink-0';
        } else {
            pillEl.textContent = '🔴 Disabled';
            pillEl.className = 'pill-status text-[11px] sm:text-xs font-semibold px-2.5 py-1 rounded-full bg-rose-50 text-rose-700 border border-rose-200 whitespace-nowrap shrink-0';
        }
    }
}

// Client Merging (Static clients.toml + Dynamic Live DHCP Clients + Active Sessions)
function getMergedDashboardClients() {
    const staticClients = (clientsObj && Array.isArray(clientsObj.client)) ? [...clientsObj.client] : [];
    const staticIps = new Set(staticClients.map(c => c.ip));

    if (stats && Array.isArray(stats.clients)) {
        stats.clients.forEach(activeClient => {
            if (activeClient && activeClient.ip && !staticIps.has(activeClient.ip)) {
                staticClients.push({
                    hostname: `DHCP-${activeClient.ip.split('.').pop()}`,
                    ip: activeClient.ip,
                    mac: activeClient.mac || 'Dynamic DHCP',
                    dns: '-',
                    gateway: '-',
                    next_server: '-',
                    image_manager: 'Gamedisk Only',
                    pxe: '-',
                    isDynamic: true
                });
                staticIps.add(activeClient.ip);
            }
        });
    }

    if (stats && Array.isArray(stats.dhcp_leases)) {
        stats.dhcp_leases.forEach(lease => {
            if (lease && lease.ip && !staticIps.has(lease.ip)) {
                staticClients.push({
                    hostname: `DHCP-${lease.ip.split('.').pop()}`,
                    ip: lease.ip,
                    mac: lease.mac || 'DHCP Lease',
                    dns: '-',
                    gateway: '-',
                    next_server: '-',
                    image_manager: 'Gamedisk Only',
                    pxe: '-',
                    isDynamic: true
                });
                staticIps.add(lease.ip);
            }
        });
    }

    return staticClients;
}

// Render Dashboard Clients Table
function renderDashboardClientsTable() {
    const tbody = document.getElementById('dashboard-clients-tbody');
    if (!tbody) return;

    const mergedClients = getMergedDashboardClients();
    renderedDashboardIps = mergedClients.map(c => c.ip);

    if (mergedClients.length === 0) {
        tbody.innerHTML = `<tr><td colspan="6" class="py-12 px-6 text-center">
            <div class="flex flex-col items-center justify-center text-center gap-3">
                <div class="text-4xl">🔌</div>
                <h3 class="font-bold text-base text-stone-900 font-['General_Sans','Outfit',sans-serif]">Belum Ada Klien Aktif</h3>
                <p class="text-stone-500 text-xs sm:text-sm max-w-sm">Klien yang terhubung dan menyala akan muncul di sini secara real-time.</p>
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
            ? `<span class="client-status-badge inline-flex items-center gap-1 text-[10.5px] lg:text-[9.5px] xl:text-[11px] font-semibold px-1.5 py-0.5 rounded-full bg-emerald-50 text-emerald-700 border border-emerald-200">🟢 Online</span>`
            : `<span class="client-status-badge inline-flex items-center gap-1 text-[10.5px] lg:text-[9.5px] xl:text-[11px] font-semibold px-1.5 py-0.5 rounded-full bg-rose-50 text-rose-700 border border-rose-200">🔴 Offline</span>`;

        const isSuper = configObj && configObj.windows && configObj.windows.super_client_ip === c.ip;
        const superBadge = isSuper ? ` <span class="inline-flex items-center text-[9.5px] lg:text-[9px] xl:text-[10px] font-semibold px-1.5 py-0.5 rounded-full bg-amber-100 text-amber-900 border border-amber-300 ml-1">⚡ Super</span>` : '';
        const dynamicBadge = c.isDynamic ? ` <span class="inline-flex items-center text-[9.5px] lg:text-[9px] xl:text-[10px] font-semibold px-1.5 py-0.5 rounded-full bg-indigo-100 text-indigo-900 border border-indigo-200 ml-1">DHCP</span>` : '';

        const row = document.createElement('tr');
        row.setAttribute('data-ip', c.ip);
        row.className = "hover:bg-stone-50 transition-colors border-b border-stone-100";
        row.innerHTML = `
            <td class="py-2 px-2 lg:py-2 lg:px-1.5 xl:py-3 xl:px-3 2xl:py-3.5 2xl:px-4 whitespace-nowrap">
                <div class="flex items-center gap-1.5 flex-nowrap whitespace-nowrap">
                    ${statusSpan}
                    <span class="font-bold text-stone-900 text-xs lg:text-[11px] xl:text-xs 2xl:text-sm font-['General_Sans','Outfit',sans-serif]">${c.hostname || c.ip}</span>
                    ${superBadge}${dynamicBadge}
                </div>
                <div class="text-[10.5px] lg:text-[9.5px] xl:text-[11px] text-stone-500 font-mono mt-0.5 whitespace-nowrap">${c.ip}${c.mac ? ' • ' + c.mac : ''}</div>
            </td>
            <td class="py-2 px-2 lg:py-2 lg:px-1.5 xl:py-3 xl:px-3 2xl:py-3.5 2xl:px-4 whitespace-nowrap">
                <div class="text-xs lg:text-[9.5px] xl:text-xs text-stone-800 font-mono whitespace-nowrap"><span class="text-stone-400 font-sans font-medium">GW:</span> ${c.gateway || '-'} <span class="text-stone-300 mx-0.5">•</span> <span class="text-stone-400 font-sans font-medium">DNS:</span> ${c.dns || '-'}</div>
                <div class="text-[10.5px] lg:text-[9.5px] xl:text-[11px] text-stone-500 font-mono mt-0.5 whitespace-nowrap"><span class="text-stone-400 font-sans font-medium">Next:</span> ${c.next_server || '-'}</div>
            </td>
            <td class="py-2 px-2 lg:py-2 lg:px-1.5 xl:py-3 xl:px-3 2xl:py-3.5 2xl:px-4 whitespace-nowrap">
                <div class="text-xs font-semibold text-stone-800 flex items-center gap-1 flex-nowrap whitespace-nowrap">
                    <span class="text-stone-400 text-xs">💿</span>
                    <span class="bg-stone-100 border border-stone-200/60 rounded px-1.5 py-0.5 font-mono text-[10.5px] lg:text-[9.5px] xl:text-[11px] text-stone-900 truncate max-w-[90px] lg:max-w-[75px] xl:max-w-[130px] 2xl:max-w-[160px] inline-block" title="${c.image_manager || 'None (Gamedisk)'}">${c.image_manager || 'None (Gamedisk)'}</span>
                </div>
                <div class="text-[10.5px] lg:text-[9.5px] xl:text-[11px] text-stone-500 font-mono mt-0.5 whitespace-nowrap"><span class="text-stone-400 font-sans font-medium">PXE:</span> ${c.pxe || 'Default'}</div>
            </td>
            <td class="py-2 px-2 lg:py-2 lg:px-1.5 xl:py-3 xl:px-3 2xl:py-3.5 2xl:px-4 whitespace-nowrap">
                <div class="client-read-total text-xs lg:text-[9.5px] xl:text-xs font-mono font-medium text-stone-700 whitespace-nowrap">${formatBytes(statsInfo.bytes_read)}</div>
                <div class="client-read-speed text-[10.5px] lg:text-[9.5px] xl:text-[11px] font-mono font-semibold text-indigo-600 mt-0.5 whitespace-nowrap">⚡ ${formatSpeed(speedInfo.readSpeed)}</div>
            </td>
            <td class="py-2 px-2 lg:py-2 lg:px-1.5 xl:py-3 xl:px-3 2xl:py-3.5 2xl:px-4 whitespace-nowrap">
                <div class="client-write-total text-xs lg:text-[9.5px] xl:text-xs font-mono font-medium text-stone-700 whitespace-nowrap">${formatBytes(statsInfo.bytes_written)}</div>
                <div class="client-write-speed text-[10.5px] lg:text-[9.5px] xl:text-[11px] font-mono font-semibold text-amber-600 mt-0.5 whitespace-nowrap">⚡ ${formatSpeed(speedInfo.writeSpeed)}</div>
            </td>
            <td class="py-2 px-2 lg:py-2 lg:px-1.5 xl:py-3 xl:px-3 2xl:py-3.5 2xl:px-4 whitespace-nowrap">
                <div class="client-uptime text-xs lg:text-[9.5px] xl:text-xs font-medium ${statsInfo.active ? 'text-stone-900' : 'text-stone-400'} whitespace-nowrap">${statsInfo.active ? formatDuration(statsInfo.uptime_secs) : 'Offline'}</div>
                <div class="text-[10px] lg:text-[9px] xl:text-[10px] text-stone-400 font-mono mt-0.5 whitespace-nowrap">${statsInfo.active ? 'Live Session' : 'Standby'}</div>
            </td>
        `;
        tbody.appendChild(row);
    });

    const totalPcsEl = document.getElementById('stat-total-pcs');
    if (totalPcsEl) totalPcsEl.textContent = mergedClients.length;
}

// Render Clients Manager Tab Table
function renderClientsManagerTable() {
    const tbody = document.getElementById('clients-tbody');
    if (!clientsObj.client || clientsObj.client.length === 0) {
        tbody.innerHTML = `<tr><td colspan="6" class="py-12 px-6 text-center">
            <div class="flex flex-col items-center justify-center text-center gap-3">
                <div class="text-4xl">💻</div>
                <h3 class="font-bold text-base text-stone-900 font-['General_Sans','Outfit',sans-serif]">Daftar Klien Kosong</h3>
                <p class="text-stone-500 text-xs sm:text-sm max-w-sm">Klik tombol "Tambah Klien" untuk mulai mendaftarkan PC diskless Anda.</p>
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
            ? `<span class="client-status-badge inline-flex items-center gap-1 text-[10.5px] lg:text-[9.5px] xl:text-[11px] font-semibold px-1.5 py-0.5 rounded-full bg-emerald-50 text-emerald-700 border border-emerald-200">🟢 Online</span>`
            : `<span class="client-status-badge inline-flex items-center gap-1 text-[10.5px] lg:text-[9.5px] xl:text-[11px] font-semibold px-1.5 py-0.5 rounded-full bg-rose-50 text-rose-700 border border-rose-200">🔴 Offline</span>`;

        const isSuper = configObj && configObj.windows && configObj.windows.super_client_ip === c.ip;
        const superBadge = isSuper ? ` <span class="inline-flex items-center text-[9.5px] lg:text-[9px] xl:text-[10px] font-semibold px-1.5 py-0.5 rounded-full bg-amber-100 text-amber-900 border border-amber-300 ml-1">⚡ Super</span>` : '';

        const row = document.createElement('tr');
        row.setAttribute('data-ip', c.ip);
        row.className = "hover:bg-stone-50 transition-colors border-b border-stone-100 cursor-pointer";
        row.innerHTML = `
            <td class="py-2 px-2 lg:py-2 lg:px-1.5 xl:py-3 xl:px-3 2xl:py-3.5 2xl:px-4 whitespace-nowrap">
                <div class="flex items-center gap-1.5 flex-nowrap whitespace-nowrap">
                    ${statusSpan}
                    <span class="font-bold text-stone-900 text-xs lg:text-[11px] xl:text-xs 2xl:text-sm font-['General_Sans','Outfit',sans-serif]">${c.hostname || 'PC'}</span>
                    ${superBadge}
                </div>
                <div class="text-[10.5px] lg:text-[9.5px] xl:text-[11px] text-stone-500 font-mono mt-0.5 whitespace-nowrap">${c.ip} • ${c.mac}</div>
            </td>
            <td class="py-2 px-2 lg:py-2 lg:px-1.5 xl:py-3 xl:px-3 2xl:py-3.5 2xl:px-4 whitespace-nowrap">
                <div class="text-xs lg:text-[9.5px] xl:text-xs text-stone-800 font-mono whitespace-nowrap"><span class="text-stone-400 font-sans font-medium">GW:</span> ${c.gateway || '-'} <span class="text-stone-300 mx-0.5">•</span> <span class="text-stone-400 font-sans font-medium">DNS:</span> ${c.dns || '-'}</div>
                <div class="text-[10.5px] lg:text-[9.5px] xl:text-[11px] text-stone-500 font-mono mt-0.5 whitespace-nowrap"><span class="text-stone-400 font-sans font-medium">Next:</span> ${c.next_server || '-'}</div>
            </td>
            <td class="py-2 px-2 lg:py-2 lg:px-1.5 xl:py-3 xl:px-3 2xl:py-3.5 2xl:px-4 whitespace-nowrap">
                <div class="text-xs font-semibold text-stone-800 flex items-center gap-1 flex-nowrap whitespace-nowrap">
                    <span class="text-stone-400 text-xs">💿</span>
                    <span class="bg-stone-100 border border-stone-200/60 rounded px-1.5 py-0.5 font-mono text-[10.5px] lg:text-[9.5px] xl:text-[11px] text-stone-900 truncate max-w-[90px] lg:max-w-[75px] xl:max-w-[130px] 2xl:max-w-[160px] inline-block" title="${c.image_manager || 'Gamedisk'}">${c.image_manager || 'Gamedisk'}</span>
                </div>
                <div class="text-[10.5px] lg:text-[9.5px] xl:text-[11px] text-stone-500 font-mono mt-0.5 whitespace-nowrap"><span class="text-stone-400 font-sans font-medium">PXE:</span> ${c.pxe || 'Default'}</div>
            </td>
            <td class="py-2 px-2 lg:py-2 lg:px-1.5 xl:py-3 xl:px-3 2xl:py-3.5 2xl:px-4 whitespace-nowrap">
                <div class="client-read-total text-xs lg:text-[9.5px] xl:text-xs font-mono font-medium text-stone-700 whitespace-nowrap">${formatBytes(statsInfo.bytes_read)}</div>
                <div class="client-read-speed text-[10.5px] lg:text-[9.5px] xl:text-[11px] font-mono font-semibold text-indigo-600 mt-0.5 whitespace-nowrap">⚡ ${formatSpeed(speedInfo.readSpeed)}</div>
            </td>
            <td class="py-2 px-2 lg:py-2 lg:px-1.5 xl:py-3 xl:px-3 2xl:py-3.5 2xl:px-4 whitespace-nowrap">
                <div class="client-write-total text-xs lg:text-[9.5px] xl:text-xs font-mono font-medium text-stone-700 whitespace-nowrap">${formatBytes(statsInfo.bytes_written)}</div>
                <div class="client-write-speed text-[10.5px] lg:text-[9.5px] xl:text-[11px] font-mono font-semibold text-amber-600 mt-0.5 whitespace-nowrap">⚡ ${formatSpeed(speedInfo.writeSpeed)}</div>
            </td>
            <td class="py-2 px-2 lg:py-2 lg:px-1.5 xl:py-3 xl:px-3 2xl:py-3.5 2xl:px-4 whitespace-nowrap">
                <div class="client-uptime text-xs lg:text-[9.5px] xl:text-xs font-medium ${statsInfo.active ? 'text-stone-900' : 'text-stone-400'} whitespace-nowrap">${statsInfo.active ? formatDuration(statsInfo.uptime_secs) : 'Offline'}</div>
                <div class="text-[10px] lg:text-[9px] xl:text-[10px] text-stone-400 font-mono mt-0.5 whitespace-nowrap">${statsInfo.active ? 'Live Session' : 'Standby'}</div>
            </td>
        `;

        // Click to edit modal
        row.addEventListener('click', () => openClientCrudModal(c));

        // Right-click context menu
        row.addEventListener('contextmenu', (e) => {
            e.preventDefault();
            showContextMenu(e, { ip: c.ip, active: statsInfo.active, image_manager: c.image_manager, clientObj: c });
        });

        tbody.appendChild(row);
    });
}

// Client CRUD Handlers
async function openClientCrudModal(client = null) {
    const modal = document.getElementById('client-crud-modal');
    modal.style.display = 'flex';

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
    if (!select) return;
    select.innerHTML = '';
    select.add(new Option('None (Gamedisk Only)', ''));

    if (configObj && configObj.image_manager) {
        Object.keys(configObj.image_manager).forEach(alias => {
            select.add(new Option(`💿 ${alias}`, alias));
        });
    }

    const vhdFiles = await apiGet('/api/system/vhds');
    if (vhdFiles && Array.isArray(vhdFiles)) {
        vhdFiles.forEach(file => {
            if (!configObj || !configObj.image_manager || !configObj.image_manager[file]) {
                select.add(new Option(`📁 ${file}`, file));
            }
        });
    }
}

async function saveClientAction(e) {
    e.preventDefault();
    const oldMac = document.getElementById('client-old-mac').value.trim();
    const clientData = {
        hostname: document.getElementById('client-hostname').value.trim(),
        mac: document.getElementById('client-mac').value.trim(),
        ip: document.getElementById('client-ip').value.trim(),
        gateway: document.getElementById('client-gateway').value.trim(),
        dns: document.getElementById('client-dns').value.trim(),
        next_server: document.getElementById('client-next-server').value.trim(),
        pxe: document.getElementById('client-pxe').value.trim(),
        image_manager: document.getElementById('client-image-manager').value.trim()
    };

    if (!clientsObj.client) clientsObj.client = [];

    if (oldMac) {
        const idx = clientsObj.client.findIndex(c => c.mac.toLowerCase() === oldMac.toLowerCase());
        if (idx !== -1) clientsObj.client[idx] = clientData;
        else clientsObj.client.push(clientData);
    } else {
        clientsObj.client.push(clientData);
    }

    await saveClientsJson();
    closeClientCrudModal();
    renderClientsManagerTable();
    renderDashboardClientsTable();
    showToast('Data klien berhasil disimpan', 'success');
}

async function deleteClientAction() {
    const oldMac = document.getElementById('client-old-mac').value.trim();
    if (!oldMac) return;

    showConfirmModal('Hapus Klien', `Apakah Anda yakin ingin menghapus data klien dengan MAC ${oldMac}?`, async () => {
        clientsObj.client = clientsObj.client.filter(c => c.mac.toLowerCase() !== oldMac.toLowerCase());
        await saveClientsJson();
        closeClientCrudModal();
        renderClientsManagerTable();
        renderDashboardClientsTable();
        showToast('Klien berhasil dihapus', 'success');
    });
}

async function autoAllocateNextServerIpsAction() {
    if (!configObj || !configObj.dhcp || !configObj.dhcp.nic_ips || configObj.dhcp.nic_ips.length === 0) {
        showToast('Tambahkan IP NIC di Pengaturan Sentral > DHCP terlebih dahulu', 'warning');
        return;
    }

    const nics = configObj.dhcp.nic_ips;
    clientsObj.client.forEach((c, idx) => {
        c.next_server = nics[idx % nics.length];
    });

    await saveClientsJson();
    renderClientsManagerTable();
    renderDashboardClientsTable();
    showToast(`Next Server IP berhasil dialokasikan merata (${nics.length} NICs)`, 'success');
}

async function loadClientsJson() {
    try {
        const data = await apiGet('/api/clients/json');
        if (data && data.client) {
            clientsObj = data;
        } else {
            clientsObj = { client: [] };
        }
    } catch (err) {
        console.error('loadClientsJson error:', err);
        clientsObj = { client: [] };
    } finally {
        renderClientsManagerTable();
        renderDashboardClientsTable();
    }
}

async function saveClientsJson() {
    const res = await apiPost('/api/clients/json', clientsObj);
    return res && res.status === 'ok';
}

// VHD Manager CRUD Handlers
function renderVhdTable() {
    const tbody = document.getElementById('vhds-tbody');
    if (!configObj || !configObj.image_manager || Object.keys(configObj.image_manager).length === 0) {
        tbody.innerHTML = `<tr><td colspan="3" class="py-12 px-6 text-center">
            <div class="flex flex-col items-center justify-center text-center gap-3">
                <div class="text-4xl">💿</div>
                <h3 class="font-bold text-base text-stone-900 font-['General_Sans','Outfit',sans-serif]">Belum Ada VHD</h3>
                <p class="text-stone-500 text-xs sm:text-sm max-w-sm">Daftarkan file VHD Windows yang akan di-boot oleh klien Anda.</p>
            </div>
        </td></tr>`;
        return;
    }

    tbody.innerHTML = '';
    Object.entries(configObj.image_manager).forEach(([key, path]) => {
        const row = document.createElement('tr');
        row.className = "hover:bg-stone-50 transition-colors border-b border-stone-100";
        row.innerHTML = `
            <td class="py-3 px-3.5 sm:py-3.5 sm:px-5">
                <div class="flex items-center gap-2">
                    <span class="text-base">💿</span>
                    <span class="font-mono text-xs sm:text-sm font-bold text-stone-900 font-['General_Sans','Outfit',sans-serif]">${key}</span>
                </div>
                <div class="text-[11px] font-mono text-stone-500 truncate max-w-md mt-0.5" title="${path}">${path}</div>
            </td>
            <td class="py-3 px-3.5 sm:py-3.5 sm:px-5">
                <div class="text-xs font-semibold text-stone-800" id="snapshots-count-${key}">Loading...</div>
                <div class="text-[11px] text-stone-400 mt-0.5">Auto Snapshot Ready</div>
            </td>
            <td class="py-3 px-3.5 sm:py-3.5 sm:px-5" style="text-align: right;">
                <div class="inline-flex items-center gap-2 justify-end">
                    <button class="inline-flex items-center justify-center px-3 py-1.5 text-xs font-medium rounded-md bg-white border border-stone-300 text-stone-700 hover:bg-stone-50 shadow-xs transition-all" onclick="openVhdCrudModal('${key}', '${path}')">Edit</button>
                    <button class="btn-primary inline-flex items-center justify-center px-3 py-1.5 text-xs shadow-xs" onclick="showVhdSnapshots('${key}')">Snapshots</button>
                </div>
            </td>
        `;
        tbody.appendChild(row);

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
        document.getElementById('vhd-key').value = key;
        document.getElementById('vhd-path').value = path || '';
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
    const res = await apiPost('/api/system/select_vhd');
    if (res && res.path) {
        document.getElementById('vhd-path').value = res.path;
    }
}

async function saveVhdAction(e) {
    e.preventDefault();
    const oldKey = document.getElementById('vhd-old-key').value.trim();
    const newKey = document.getElementById('vhd-key').value.trim();
    const path = document.getElementById('vhd-path').value.trim();

    if (!configObj) configObj = {};
    if (!configObj.image_manager) configObj.image_manager = {};

    if (oldKey && oldKey !== newKey) {
        delete configObj.image_manager[oldKey];
    }
    configObj.image_manager[newKey] = path;

    await saveConfigJsonFull();
    closeVhdCrudModal();
    renderVhdTable();
    showToast('VHD Mapping berhasil disimpan', 'success');
}

async function deleteVhdAction() {
    const oldKey = document.getElementById('vhd-old-key').value.trim();
    if (!oldKey) return;

    showConfirmModal('Hapus VHD Mapping', `Yakin ingin menghapus alias mapping ${oldKey}?`, async () => {
        delete configObj.image_manager[oldKey];
        await saveConfigJsonFull();
        closeVhdCrudModal();
        renderVhdTable();
        showToast('VHD Mapping berhasil dihapus', 'success');
    });
}

// Snapshots History Modal
async function showVhdSnapshots(imageKey) {
    const modal = document.getElementById('snapshots-modal');
    modal.style.display = 'flex';
    document.getElementById('snapshots-modal-title').textContent = `Riwayat Snapshot (${imageKey})`;

    const tbody = document.getElementById('snapshots-tbody');
    tbody.innerHTML = `<tr><td colspan="3" class="py-6 px-4 text-center text-stone-500">Memuat snapshot...</td></tr>`;

    const data = await apiGet(`/api/vhd/backups?image_key=${imageKey}`);
    if (!data || data.length === 0) {
        tbody.innerHTML = `<tr><td colspan="3" class="py-6 px-4 text-center text-stone-500">Belum ada file snapshot backup untuk image ini.</td></tr>`;
        return;
    }

    tbody.innerHTML = '';
    data.forEach(snap => {
        const row = document.createElement('tr');
        row.className = "hover:bg-stone-50 border-b border-stone-100";
        row.innerHTML = `
            <td class="py-2.5 px-3.5 font-mono text-xs font-semibold text-stone-900">${snap.path || snap.name}</td>
            <td class="py-2.5 px-3.5 font-mono text-xs text-stone-500">${snap.index !== undefined ? '#' + snap.index : ''}</td>
            <td class="py-2.5 px-3.5 text-right">
                <button class="inline-flex items-center justify-center px-3 py-1 text-xs font-semibold rounded-md bg-amber-50 border border-amber-200 text-amber-800 hover:bg-amber-100 active:scale-[0.98] transition-all" onclick="restoreSnapshotAction('${imageKey}', '${snap.path || snap.name}')">⏪ Restore</button>
            </td>
        `;
        tbody.appendChild(row);
    });
}

function closeSnapshotsModal() {
    document.getElementById('snapshots-modal').style.display = 'none';
}

async function restoreSnapshotAction(imageKey, snapshotName) {
    showConfirmModal('Restore Snapshot', `Apakah Anda yakin ingin me-restore master VHD ${imageKey} ke snapshot ${snapshotName}?`, async () => {
        const res = await apiPost('/api/vhd/restore', { image_key: imageKey, snapshot_name: snapshotName });
        if (res && res.status === 'ok') {
            showToast('VHD berhasil di-restore ke snapshot', 'success');
            closeSnapshotsModal();
        } else {
            showToast('Gagal me-restore snapshot', 'error');
        }
    });
}

// Disk Management & Dynamic Storage Handlers
async function loadDiskPartitions() {
    const drives = await apiGet('/api/system/logical_drives_detail');
    if (drives && Array.isArray(drives)) {
        renderDiskGrid(drives);
    }
}

function renderDiskGrid(drives) {
    const container = document.getElementById('disk-grid-container');
    if (!container) return;

    if (!Array.isArray(drives) || drives.length === 0) {
        container.innerHTML = `<div class="col-span-full py-10 text-center text-stone-500">Memindai disk fisik...</div>`;
        return;
    }

    window._systemDrivesMap = {};
    container.innerHTML = '';

    const normalizeDisk = (p) => (p || '').replace(/\\+/g, '\\').toLowerCase().trim();

    drives.forEach(drive => {
        const letter = (drive.letter || '').toUpperCase();
        window._systemDrivesMap[letter] = drive;
        let currentRole = 'none';

        const drivePhysNorm = normalizeDisk(drive.physical_disk);
        const letterVolNorm = normalizeDisk(`\\\\.\\${letter}:`);

        if (configObj) {
            // 1. Check Boot VHD directory
            if (configObj.windows && configObj.windows.vhd_dir && configObj.windows.vhd_dir.toUpperCase().startsWith(letter)) {
                currentRole = 'boot';
            }
            // 2. Check Writeback directory
            else if (configObj.writeback && configObj.writeback.writeback_dirs && configObj.writeback.writeback_dirs.some(dir => dir && dir.toUpperCase().startsWith(letter))) {
                currentRole = 'writeback';
            }
            // 3. Check Gamedisk physical drives or volume
            else if (configObj.gamedisk && Array.isArray(configObj.gamedisk) && configObj.gamedisk.some(gd => {
                if (!gd || !gd.physical_disk) return false;
                const normGd = normalizeDisk(gd.physical_disk);
                const matchPhys = drivePhysNorm && normGd === drivePhysNorm;
                const matchVol = normGd === letterVolNorm || normGd.includes(letter.toLowerCase() + ":");
                return matchPhys || matchVol;
            })) {
                currentRole = 'gamedisk';
            }
        }

        // Genesis role styling configuration
        const roleConfig = {
            boot: {
                label: 'BOOT VHD',
                icon: '💿',
                badgeClass: 'bg-indigo-50 text-indigo-700 border-indigo-200 dark:bg-indigo-950/60 dark:text-indigo-300 dark:border-indigo-800/80',
                cardBorder: 'border-indigo-300/80 dark:border-indigo-800/60 ring-1 ring-indigo-500/15',
                desc: 'Master OS VHD',
                colorAccent: 'text-indigo-600 dark:text-indigo-400'
            },
            writeback: {
                label: 'WRITEBACK',
                icon: '⚡',
                badgeClass: 'bg-amber-50 text-amber-700 border-amber-200 dark:bg-amber-950/60 dark:text-amber-300 dark:border-amber-800/80',
                cardBorder: 'border-amber-300/80 dark:border-amber-800/60 ring-1 ring-amber-500/15',
                desc: 'Client Cache I/O',
                colorAccent: 'text-amber-600 dark:text-amber-400'
            },
            gamedisk: {
                label: 'GAMEDISK',
                icon: '🎮',
                badgeClass: 'bg-emerald-50 text-emerald-700 border-emerald-200 dark:bg-emerald-950/60 dark:text-emerald-300 dark:border-emerald-800/80',
                cardBorder: 'border-emerald-300/80 dark:border-emerald-800/60 ring-1 ring-emerald-500/15',
                desc: 'Game Storage Target',
                colorAccent: 'text-emerald-600 dark:text-emerald-400'
            },
            none: {
                label: 'UNASSIGNED',
                icon: '⚪',
                badgeClass: 'bg-stone-100 text-stone-600 border-stone-200 dark:bg-stone-800 dark:text-stone-400 dark:border-stone-700',
                cardBorder: 'border-stone-200 dark:border-stone-800',
                desc: 'Belum dialokasikan',
                colorAccent: 'text-stone-500 dark:text-stone-400'
            }
        }[currentRole] || {
            label: 'UNASSIGNED',
            icon: '⚪',
            badgeClass: 'bg-stone-100 text-stone-600 border-stone-200 dark:bg-stone-800 dark:text-stone-400 dark:border-stone-700',
            cardBorder: 'border-stone-200 dark:border-stone-800',
            desc: 'Belum dialokasikan',
            colorAccent: 'text-stone-500 dark:text-stone-400'
        };

        const card = document.createElement('div');
        card.className = `bg-white border rounded-xl p-4 sm:p-5 flex flex-col justify-between shadow-xs transition-all hover:shadow-md ${roleConfig.cardBorder}`;

        card.innerHTML = `
            <div>
                <!-- Top Header: Drive Name & Physical Disk -->
                <div class="flex items-center justify-between gap-2 pb-3 border-b border-stone-100 dark:border-stone-800/80 mb-3.5">
                    <div class="flex items-center gap-2.5 min-w-0">
                        <div class="w-8 h-8 rounded-lg bg-stone-100 dark:bg-stone-800 flex items-center justify-center text-base shrink-0 font-mono">
                            🗄️
                        </div>
                        <div class="min-w-0 flex-1">
                            <h3 class="font-bold text-sm sm:text-base text-stone-900 dark:text-white leading-tight font-['General_Sans','Outfit',sans-serif]">Drive ${drive.letter}:\\</h3>
                            <p class="text-[11px] text-stone-500 dark:text-stone-400 font-mono mt-1 break-all leading-tight">${drive.physical_disk || 'Logical Volume (Direct)'}</p>
                        </div>
                    </div>
                    <span class="inline-flex items-center gap-1 text-[10px] font-bold px-2 py-0.5 rounded-full border shrink-0 ${roleConfig.badgeClass}">
                        <span>${roleConfig.icon}</span>
                        <span>${roleConfig.label}</span>
                    </span>
                </div>

                <!-- Role Info Description -->
                <div class="text-[11px] text-stone-500 dark:text-stone-400 mb-3.5 flex items-center justify-between px-0.5">
                    <span class="font-medium">Fungsi Disk:</span>
                    <span class="font-semibold ${roleConfig.colorAccent}">${roleConfig.desc}</span>
                </div>

                <!-- Interactive Allocation Action Button -->
                <button type="button" class="w-full group px-3 py-2.5 rounded-lg border border-stone-200 dark:border-stone-700/80 bg-stone-50/80 hover:bg-indigo-50/70 dark:bg-stone-800/50 dark:hover:bg-indigo-950/40 hover:border-indigo-300 dark:hover:border-indigo-600/60 cursor-pointer flex items-center justify-between transition-all active:scale-[0.99] text-left" onclick="openPartitionModal('${letter}', '${currentRole}')">
                    <div class="flex items-center gap-2">
                        <span class="text-xs group-hover:scale-110 transition-transform">⚙️</span>
                        <span class="font-semibold text-xs text-stone-800 dark:text-stone-200 group-hover:text-indigo-600 dark:group-hover:text-indigo-400 font-['General_Sans','Outfit',sans-serif]">Ubah Alokasi Role</span>
                    </div>
                    <span class="text-xs text-stone-400 group-hover:text-indigo-500 font-bold transition-colors">→</span>
                </button>
            </div>
        `;
        container.appendChild(card);
    });
}

function openPartitionModal(letter, currentRole) {
    const drive = (window._systemDrivesMap && window._systemDrivesMap[letter]) || { letter, physical_disk: '' };
    const physicalDisk = drive.physical_disk || '';
    const modal = document.getElementById('partition-modal');
    modal.style.display = 'flex';
    document.getElementById('partition-modal-desc').textContent = `Pilih fungsi atau lepas alokasi untuk Drive ${letter}:\\ (${physicalDisk || 'Logical Volume'}).`;
    const mountInput = document.getElementById('partition-mount-point');
    mountInput.value = letter;
    mountInput.dataset.physicalDisk = physicalDisk;

    const radios = document.querySelectorAll('input[name="partition-role"]');
    radios.forEach(r => {
        r.checked = (r.value === currentRole);
    });
}

function closePartitionModal() {
    document.getElementById('partition-modal').style.display = 'none';
}

async function savePartitionRoleAction() {
    const letter = (document.getElementById('partition-mount-point').value || '').toUpperCase();
    const drive = (window._systemDrivesMap && window._systemDrivesMap[letter]) || {};
    const physicalDisk = drive.physical_disk || document.getElementById('partition-mount-point').dataset.physicalDisk || '';
    const selectedRadio = document.querySelector('input[name="partition-role"]:checked');
    const role = selectedRadio ? selectedRadio.value : 'none';

    if (!configObj) configObj = {};
    if (!configObj.windows) configObj.windows = {
        target_iqn_prefix: "iqn.2024-01.com.tmdebug:vhd-",
        vhd_dir: "C:\\vhd",
        block_size: 512,
        vendor_id: "RUSTISCS",
        product_id: "WindowsBoot",
        product_revision: "1.00",
        discovery: false,
        super_client_ip: "",
        super_client_action: "none"
    };
    if (!configObj.writeback) configObj.writeback = {
        writeback_dirs: ["C:\\writeback"],
        max_cache_per_client_gb: 10,
        max_write_speed_mbps: 100000
    };
    if (!configObj.gamedisk) configObj.gamedisk = [];

    const normalizeDisk = (p) => (p || '').replace(/\\+/g, '\\').toLowerCase().trim();
    const normTargetPhys = normalizeDisk(physicalDisk);
    const normLetterVol = normalizeDisk(`\\\\.\\${letter}:`);

    // 1. Clear previous role for this drive letter
    if (configObj.windows && configObj.windows.vhd_dir && configObj.windows.vhd_dir.toUpperCase().startsWith(letter)) {
        configObj.windows.vhd_dir = "";
    }
    if (configObj.writeback && configObj.writeback.writeback_dirs) {
        configObj.writeback.writeback_dirs = configObj.writeback.writeback_dirs.filter(dir => dir && !dir.toUpperCase().startsWith(letter));
    }
    if (configObj.gamedisk && Array.isArray(configObj.gamedisk)) {
        configObj.gamedisk = configObj.gamedisk.filter(gd => {
            if (!gd || !gd.physical_disk) return false;
            const normGd = normalizeDisk(gd.physical_disk);
            const matchPhys = normTargetPhys && normGd === normTargetPhys;
            const matchVol = normGd === normLetterVol || normGd.includes(letter.toLowerCase() + ":");
            return !matchPhys && !matchVol;
        });
    }

    // 2. Apply new role if not 'none'
    if (role === 'boot') {
        configObj.windows.vhd_dir = `${letter}:\\vhd`;
    } else if (role === 'writeback') {
        if (!configObj.writeback.writeback_dirs) configObj.writeback.writeback_dirs = [];
        configObj.writeback.writeback_dirs.push(`${letter}:\\writeback`);
    } else if (role === 'gamedisk') {
        const targetDiskPath = physicalDisk ? physicalDisk : `\\\\.\\${letter}:`;
        if (!configObj.gamedisk) configObj.gamedisk = [];
        configObj.gamedisk.push({
            physical_disk: targetDiskPath,
            block_size: 512,
            vendor_id: "RUSTISCS",
            product_id: `GameDisk-${configObj.gamedisk.length}`,
            product_revision: "1.00"
        });
    }

    // 3. Fallback ensure writeback_dirs is array
    if (!configObj.writeback.writeback_dirs) {
        configObj.writeback.writeback_dirs = [];
    }

    const saved = await saveConfigJsonFull();
    if (saved) {
        closePartitionModal();
        await loadConfigJson();
        await loadDiskPartitions();
        showToast(role === 'none' ? `Alokasi peran Drive ${letter}: berhasil dilepas` : `Peran Drive ${letter}: berhasil diubah ke ${role.toUpperCase()}`, 'success');
    } else {
        showToast(`Gagal menyimpan alokasi partisi Drive ${letter}:`, 'error');
    }
}

async function saveGlobalStorageParams() {
    const maxCacheGb = parseInt(document.getElementById('disk-max-cache-gb').value, 10) || 10;
    const throttleMb = parseInt(document.getElementById('disk-throttle-mb').value, 10) || 100000;

    if (!configObj) configObj = {};
    if (!configObj.writeback) configObj.writeback = {
        writeback_dirs: ["C:\\writeback"],
        max_cache_per_client_gb: 10,
        max_write_speed_mbps: 100000
    };

    configObj.writeback.max_cache_per_client_gb = maxCacheGb;
    configObj.writeback.max_write_speed_mbps = throttleMb;

    const saved = await saveConfigJsonFull();
    if (saved) {
        showToast('Parameter storage berhasil disimpan', 'success');
    } else {
        showToast('Gagal menyimpan parameter storage', 'error');
    }
}

async function loadWritebackFiles() {
    const data = await apiGet('/api/writeback/files');
    const tbody = document.getElementById('writeback-files-tbody');
    if (!tbody) return;

    if (!data || data.length === 0) {
        tbody.innerHTML = `<tr><td colspan="2" class="py-6 px-4 text-center text-stone-500">Tidak ada file cache writeback aktif.</td></tr>`;
        return;
    }

    tbody.innerHTML = '';
    data.forEach(item => {
        const row = document.createElement('tr');
        row.className = "hover:bg-stone-50 border-b border-stone-100";
        row.innerHTML = `
            <td class="py-2.5 px-3.5 font-mono text-xs font-semibold text-stone-900">${item.name || item.path}</td>
            <td class="py-2.5 px-3.5 font-mono text-xs text-stone-500">${formatBytes(item.size)}</td>
        `;
        tbody.appendChild(row);
    });
}

async function clearWritebackCache() {
    showConfirmModal('Bersihkan Cache', 'Apakah Anda yakin ingin menghapus semua file cache writeback yang tersimpan?', async () => {
        const res = await apiPost('/api/writeback/clear', {});
        if (res && res.status === 'ok') {
            showToast('Cache writeback berhasil dibersihkan', 'success');
            loadWritebackFiles();
        } else {
            showToast('Gagal membersihkan cache writeback', 'error');
        }
    });
}

// Central Settings & Config Handlers
async function loadConfigJson() {
    try {
        const data = await apiGet('/api/config/json');
        if (data) {
            configObj = data;
        } else if (!configObj) {
            configObj = {
                server: { address: '0.0.0.0', port: 3260, read_cache_gb: 4 },
                dhcp: { enabled: true, start_ip: '10.10.10.100', end_ip: '10.10.10.200', router: '10.10.10.1', dns: '8.8.8.8', subnet_mask: '255.255.255.0', next_server: '10.10.10.1', nic_ips: [] },
                image_manager: {}
            };
        }

        const currentCfg = configObj || {};

        // Server
        if (currentCfg.server) {
            const addrEl = document.getElementById('set-server-address');
            if (addrEl) {
                const addr = Array.isArray(currentCfg.server.address) ? currentCfg.server.address[0] : currentCfg.server.address;
                addrEl.value = addr || '0.0.0.0';
            }
            const portEl = document.getElementById('set-server-port');
            if (portEl) portEl.value = currentCfg.server.port || 3260;
            const cacheEl = document.getElementById('set-server-cache');
            if (cacheEl) cacheEl.value = currentCfg.server.read_cache_gb || 4;
        }

        // Target IQN Prefix
        const iqnEl = document.getElementById('set-gamedisk-iqn');
        if (iqnEl) {
            if (currentCfg.gamedisk_target && currentCfg.gamedisk_target.target_iqn) {
                iqnEl.value = currentCfg.gamedisk_target.target_iqn;
            } else if (currentCfg.windows && currentCfg.windows.target_iqn_prefix) {
                iqnEl.value = currentCfg.windows.target_iqn_prefix;
            }
        }

        // DHCP
        if (currentCfg.dhcp) {
            const enabledEl = document.getElementById('set-dhcp-enabled');
            if (enabledEl) enabledEl.checked = !!currentCfg.dhcp.enabled;
            const nextEl = document.getElementById('set-dhcp-next');
            if (nextEl) nextEl.value = currentCfg.dhcp.next_server || '';
            const startIpEl = document.getElementById('set-dhcp-start-ip');
            if (startIpEl) startIpEl.value = currentCfg.dhcp.start_ip || '';
            const endIpEl = document.getElementById('set-dhcp-end-ip');
            if (endIpEl) endIpEl.value = currentCfg.dhcp.end_ip || '';
            const maskEl = document.getElementById('set-dhcp-mask');
            if (maskEl) maskEl.value = currentCfg.dhcp.subnet_mask || currentCfg.dhcp.netmask || '255.255.255.0';
            const gwEl = document.getElementById('set-dhcp-gateway');
            if (gwEl) gwEl.value = currentCfg.dhcp.router || currentCfg.dhcp.gateway || '';
            const dnsEl = document.getElementById('set-dhcp-dns');
            if (dnsEl) dnsEl.value = currentCfg.dhcp.dns || '8.8.8.8';

            renderNicIpsList(currentCfg.dhcp.nic_ips || []);
        }

        // TFTP
        const dirEl = document.getElementById('set-tftp-dir');
        const pxeEl = document.getElementById('set-pxe-default');
        if (currentCfg.dhcp && currentCfg.dhcp.tftp_dir && dirEl) dirEl.value = currentCfg.dhcp.tftp_dir;
        else if (currentCfg.tftp && currentCfg.tftp.tftp_dir && dirEl) dirEl.value = currentCfg.tftp.tftp_dir;

        if (currentCfg.dhcp && currentCfg.dhcp.pxe_default && pxeEl) pxeEl.value = currentCfg.dhcp.pxe_default;
        else if (currentCfg.tftp && currentCfg.tftp.pxe_default && pxeEl) pxeEl.value = currentCfg.tftp.pxe_default;

        // Storage Parameters
        if (currentCfg.writeback) {
            const maxCacheEl = document.getElementById('disk-max-cache-gb');
            if (maxCacheEl) maxCacheEl.value = currentCfg.writeback.max_cache_per_client_gb || 10;
            const throttleEl = document.getElementById('disk-throttle-mb');
            if (throttleEl) throttleEl.value = currentCfg.writeback.max_write_speed_mbps || 100000;
        }
    } catch (err) {
        console.error('loadConfigJson error:', err);
    } finally {
        renderVhdTable();
        loadDiskPartitions();
        loadTftpFolders();
        populateNetworkDropdowns();
    }
}

function renderNicIpsList(nicIps) {
    const container = document.getElementById('nic-ips-list-container');
    if (!container) return;

    if (!nicIps || nicIps.length === 0) {
        container.innerHTML = `<span class="text-stone-500 text-center text-xs py-1">Belum ada IP ditambahkan.</span>`;
        return;
    }

    container.innerHTML = '';
    nicIps.forEach(ip => {
        const tag = document.createElement('div');
        tag.className = "flex items-center justify-between px-2.5 py-1 rounded bg-white border border-stone-200";
        tag.innerHTML = `
            <span class="font-mono text-xs text-stone-900">${ip}</span>
            <button type="button" class="text-rose-600 hover:text-rose-800 font-bold p-0.5" onclick="removeNicIpAction('${ip}')">✕</button>
        `;
        container.appendChild(tag);
    });
}

function addNicIpAction() {
    const input = document.getElementById('add-nic-ip-input');
    const val = input.value.trim();
    if (!val) return;

    if (!configObj) configObj = {};
    if (!configObj.dhcp) configObj.dhcp = {};
    if (!configObj.dhcp.nic_ips) configObj.dhcp.nic_ips = [];

    if (!configObj.dhcp.nic_ips.includes(val)) {
        configObj.dhcp.nic_ips.push(val);
        renderNicIpsList(configObj.dhcp.nic_ips);
        input.value = '';
    }
}

function removeNicIpAction(ip) {
    if (configObj && configObj.dhcp && configObj.dhcp.nic_ips) {
        configObj.dhcp.nic_ips = configObj.dhcp.nic_ips.filter(item => item !== ip);
        renderNicIpsList(configObj.dhcp.nic_ips);
    }
}

async function saveConfigJson(e) {
    if (e && typeof e.preventDefault === 'function') e.preventDefault();
    if (!configObj) configObj = {};

    const serverAddrInput = document.getElementById('set-server-address');
    const serverPortInput = document.getElementById('set-server-port');
    const serverCacheInput = document.getElementById('set-server-cache');
    const gamediskIqnInput = document.getElementById('set-gamedisk-iqn');

    configObj.server = {
        address: serverAddrInput ? serverAddrInput.value.trim() : (configObj.server?.address || '0.0.0.0'),
        port: serverPortInput ? (parseInt(serverPortInput.value, 10) || 3260) : (configObj.server?.port || 3260),
        read_cache_gb: serverCacheInput ? (parseInt(serverCacheInput.value, 10) || 4) : (configObj.server?.read_cache_gb || 4)
    };

    if (!configObj.gamedisk_target) {
        configObj.gamedisk_target = {
            target_iqn: gamediskIqnInput ? gamediskIqnInput.value.trim() : "iqn.2024-01.com.tmdebug:gamedisks",
            discovery: true
        };
    } else {
        configObj.gamedisk_target.target_iqn = gamediskIqnInput ? gamediskIqnInput.value.trim() : configObj.gamedisk_target.target_iqn;
    }

    const tftpDirVal = document.getElementById('set-tftp-dir')?.value?.trim() || 'pxe';
    const pxeDefaultVal = document.getElementById('set-pxe-default')?.value?.trim() || 'sb-custom';
    const dhcpEnabled = document.getElementById('set-dhcp-enabled')?.checked ?? true;
    const nextServerVal = document.getElementById('set-dhcp-next')?.value?.trim() || '';
    const startIpVal = document.getElementById('set-dhcp-start-ip')?.value?.trim() || '';
    const endIpVal = document.getElementById('set-dhcp-end-ip')?.value?.trim() || '';
    const routerVal = document.getElementById('set-dhcp-gateway')?.value?.trim() || '';
    const dnsVal = document.getElementById('set-dhcp-dns')?.value?.trim() || '8.8.8.8';
    const subnetMaskVal = document.getElementById('set-dhcp-mask')?.value?.trim() || '255.255.255.0';

    configObj.dhcp = {
        enabled: dhcpEnabled,
        next_server: nextServerVal,
        start_ip: startIpVal,
        end_ip: endIpVal,
        router: routerVal,
        dns: dnsVal,
        subnet_mask: subnetMaskVal,
        tftp_dir: tftpDirVal,
        pxe_default: pxeDefaultVal,
        nic_ips: (configObj.dhcp && configObj.dhcp.nic_ips) ? configObj.dhcp.nic_ips : []
    };

    const success = await saveConfigJsonFull();
    if (success) {
        showToast('Konfigurasi sentral berhasil disimpan', 'success');
        await loadConfigJson();
    } else {
        showToast('Gagal menyimpan konfigurasi sentral', 'error');
    }
}

async function saveConfigJsonFull() {
    const res = await apiPost('/api/config/json', configObj);
    return res && res.status === 'ok';
}

// TFTP Bootloader Folders
async function loadTftpFolders() {
    const folders = await apiGet('/api/system/tftp_folders');
    const datalist = document.getElementById('tftp-folders-list');
    const tbody = document.getElementById('tftp-folders-tbody');

    if (datalist) datalist.innerHTML = '';
    if (tbody) tbody.innerHTML = '';

    if (!folders || folders.length === 0) {
        if (tbody) tbody.innerHTML = `<tr><td colspan="2" class="py-6 px-4 text-center text-stone-500">Belum ada folder bootloader kustom.</td></tr>`;
        return;
    }

    folders.forEach(f => {
        if (datalist) {
            const opt = document.createElement('option');
            opt.value = f;
            datalist.appendChild(opt);
        }

        if (tbody) {
            const row = document.createElement('tr');
            row.className = "hover:bg-stone-50 border-b border-stone-100";
            row.innerHTML = `
                <td class="py-2.5 px-4 font-mono text-xs font-semibold text-stone-900">${f}</td>
                <td class="py-2.5 px-4 text-right">
                    <button type="button" class="text-rose-600 hover:text-rose-800 text-xs font-semibold" onclick="deleteTftpFolderAction('${f}')">Hapus</button>
                </td>
            `;
            tbody.appendChild(row);
        }
    });
}

function createNewTftpFolderPrompt() {
    const name = prompt('Masukkan nama folder TFTP boot loader baru:');
    if (name && name.trim()) {
        apiPost('/api/system/tftp_folders/create', { folder_name: name.trim() }).then(res => {
            if (res && res.status === 'ok') {
                showToast(`Folder TFTP '${name}' berhasil dibuat`, 'success');
                loadTftpFolders();
            } else {
                showToast('Gagal membuat folder TFTP', 'error');
            }
        });
    }
}

function deleteTftpFolderAction(folderName) {
    showConfirmModal('Hapus Folder TFTP', `Yakin ingin menghapus folder TFTP '${folderName}'?`, async () => {
        const res = await apiPost('/api/system/tftp_folders/delete', { folder_name: folderName });
        if (res && res.status === 'ok') {
            showToast(`Folder TFTP '${folderName}' berhasil dihapus`, 'success');
            loadTftpFolders();
        } else {
            showToast('Gagal menghapus folder TFTP', 'error');
        }
    });
}

// Context Menu Handlers
function initContextMenus() {
    document.addEventListener('click', () => hideContextMenu());
    window.addEventListener('blur', () => hideContextMenu());
}

function showContextMenu(e, clientData) {
    selectedContextClient = clientData;
    const menu = document.getElementById('context-menu');
    if (!menu) return;

    const isSuper = configObj && configObj.windows && configObj.windows.super_client_ip === clientData.ip;
    const superLabel = document.getElementById('ctx-super-client-label');
    if (superLabel) {
        superLabel.textContent = isSuper ? 'Disable Super Client' : 'Enable Super Client';
    }

    menu.style.display = 'block';
    menu.style.left = `${Math.min(e.pageX, window.innerWidth - 220)}px`;
    menu.style.top = `${Math.min(e.pageY, window.innerHeight - 150)}px`;
}

function hideContextMenu() {
    const menu = document.getElementById('context-menu');
    if (menu) menu.style.display = 'none';
}

async function ctxEnableSuperClient() {
    if (!selectedContextClient) return;
    const ip = selectedContextClient.ip;
    const isSuper = configObj && configObj.windows && configObj.windows.super_client_ip === ip;

    const action = isSuper ? 'disable' : 'enable';
    const res = await apiPost('/api/superclient/set', { ip, action });

    if (res) {
        showToast(isSuper ? `Super Client dinonaktifkan untuk IP ${ip}` : `Super Client diaktifkan untuk IP ${ip}`, 'success');
        await loadConfigJson();
        renderClientsManagerTable();
        renderDashboardClientsTable();
    }
}

async function ctxClearWriteback() {
    if (!selectedContextClient) return;
    const ip = selectedContextClient.ip;
    showConfirmModal('Clear Writeback', `Hapus writeback cache klien ${ip}?`, async () => {
        await apiPost('/api/writeback/clear', { ip });
        showToast(`Cache writeback ${ip} berhasil dibersihkan`, 'success');
        loadWritebackFiles();
    });
}

function ctxEditClient() {
    if (selectedContextClient && selectedContextClient.clientObj) {
        openClientCrudModal(selectedContextClient.clientObj);
    }
}

// Confirmation Dialog Modal
function showConfirmModal(title, msg, callback) {
    const modal = document.getElementById('confirm-modal');
    if (!modal) return;
    document.getElementById('confirm-title').textContent = title;
    document.getElementById('confirm-message').textContent = msg;
    confirmCallback = callback;
    modal.style.display = 'flex';
}

function closeConfirmModal(confirmed = false) {
    const modal = document.getElementById('confirm-modal');
    if (modal) modal.style.display = 'none';
    if (confirmed && typeof confirmCallback === 'function') {
        confirmCallback();
    }
    confirmCallback = null;
}
