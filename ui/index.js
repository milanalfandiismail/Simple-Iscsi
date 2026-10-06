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

    // 1. Toggle panels with smooth transition
    document.querySelectorAll('.tab-panel').forEach(panel => {
        panel.classList.remove('active');
    });
    const activePanel = document.getElementById(`tab-${tabId}`);
    if (activePanel) {
        activePanel.classList.add('active');
        window.scrollTo({ top: 0, behavior: 'instant' });
    }

    // 2. Identify active button if not provided directly
    if (!btnEl) {
        btnEl = document.querySelector(`.nav-item[onclick*="'${tabId}'"]`);
    }

    // 3. Toggle nav item styles (Shadcn Zinc Monochrome)
    document.querySelectorAll('.nav-item').forEach(btn => {
        btn.classList.remove('active', 'bg-zinc-100', 'dark:bg-zinc-900', 'text-zinc-900', 'dark:text-zinc-50', 'border', 'border-zinc-200', 'dark:border-zinc-800', 'font-medium', 'shadow-2xs');
        btn.classList.add('text-zinc-600', 'dark:text-zinc-400', 'hover:bg-zinc-100/70', 'dark:hover:bg-zinc-900/60', 'hover:text-zinc-900', 'dark:hover:text-zinc-100');
    });

    if (btnEl) {
        btnEl.classList.add('active', 'bg-zinc-100', 'dark:bg-zinc-900', 'text-zinc-900', 'dark:text-zinc-50', 'border', 'border-zinc-200', 'dark:border-zinc-800', 'font-medium', 'shadow-2xs');
        btnEl.classList.remove('text-zinc-600', 'dark:text-zinc-400', 'hover:bg-zinc-100/70', 'dark:hover:bg-zinc-900/60', 'hover:text-zinc-900', 'dark:hover:text-zinc-100');
    }

    // 4. Auto close mobile drawer if open
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

    try {
        await checkInitialMergeStatus();
    } catch (err) {
        console.error('checkInitialMergeStatus error:', err);
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

// Server Connection State & API Base Resolution
let isServerConnected = null;

function setServerConnectionStatus(connected) {
    if (isServerConnected === connected) return;
    isServerConnected = connected;
    const statusEl = document.getElementById('engine-status-indicator');
    if (statusEl) {
        if (connected) {
            statusEl.innerHTML = `<span class="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse"></span> <span class="text-zinc-600 dark:text-zinc-300">Live API</span>`;
        } else {
            statusEl.innerHTML = `<span class="w-1.5 h-1.5 rounded-full bg-amber-500"></span> <span class="text-zinc-500 dark:text-zinc-400">Offline</span>`;
        }
    }
}

function getApiBase() {
    try {
        const custom = localStorage.getItem('simple_iscsi_api_base');
        if (custom) return custom.replace(/\/+$/, '');
    } catch (_) {}

    // When opened directly as a local file (file:///...) or empty host, target default server port 8080
    if (typeof window !== 'undefined' && (window.location.protocol === 'file:' || !window.location.host)) {
        return 'http://127.0.0.1:8080';
    }
    return '';
}

function resolveApiUrl(url) {
    if (!url) return '';
    if (url.startsWith('http://') || url.startsWith('https://')) return url;
    const base = getApiBase();
    const cleanUrl = url.startsWith('/') ? url : `/${url}`;
    return base ? `${base}${cleanUrl}` : cleanUrl;
}

// HTTP API Fetch Helpers
async function apiGet(url, timeoutMs = 3500) {
    const fullUrl = resolveApiUrl(url);
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), timeoutMs);
    try {
        const res = await fetch(fullUrl, { signal: controller.signal });
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const data = await res.json();
        clearTimeout(timeoutId);
        setServerConnectionStatus(true);
        return data;
    } catch (err) {
        clearTimeout(timeoutId);
        if (err.name !== 'AbortError') {
            setServerConnectionStatus(false);
            console.warn(`[Simple-Iscsi API] GET ${fullUrl} unavailable (${err.message || 'Offline'})`);
        }
        return null;
    }
}

async function apiPost(url, body = {}, timeoutMs = 4500) {
    const fullUrl = resolveApiUrl(url);
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), timeoutMs);
    try {
        const res = await fetch(fullUrl, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: typeof body === 'string' ? body : JSON.stringify(body),
            signal: controller.signal
        });
        const contentType = res.headers.get('content-type');
        let result;
        if (contentType && contentType.includes('application/json')) {
            result = await res.json();
        } else {
            const text = await res.text();
            result = { status: res.ok ? 'ok' : 'error', message: text };
        }
        clearTimeout(timeoutId);
        if (!res.ok) {
            console.warn(`[Simple-Iscsi API] POST ${fullUrl} returned ${res.status}:`, result);
            return {
                status: 'error',
                message: (result && result.message) ? result.message : `HTTP ${res.status}`
            };
        }
        setServerConnectionStatus(true);
        return result;
    } catch (err) {
        clearTimeout(timeoutId);
        setServerConnectionStatus(false);
        console.warn(`[Simple-Iscsi API] POST ${fullUrl} failed:`, err.message || 'Network error');
        return { status: 'error', message: err.message || 'Network error / Server Offline' };
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
        success: 'bg-zinc-900 dark:bg-zinc-100 text-zinc-100 dark:text-zinc-900 border border-zinc-700 dark:border-zinc-300 shadow-md',
        error: 'bg-rose-950 text-rose-100 border border-rose-800 shadow-md',
        warning: 'bg-amber-950 text-amber-100 border border-amber-800 shadow-md',
        info: 'bg-zinc-900 dark:bg-zinc-100 text-zinc-100 dark:text-zinc-900 border border-zinc-700 dark:border-zinc-300 shadow-md'
    }[type] || 'bg-zinc-900 text-white';

    toast.className = `toast-msg flex items-center gap-2.5 px-3.5 py-2.5 rounded-md text-xs sm:text-sm font-medium ${colorClasses}`;
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
    let pollFailures = 0;

    const fetchStats = async () => {
        try {
            const data = await apiGet('/api/stats');
            if (data) {
                pollFailures = 0;
                handleStatsData(data);
            } else {
                pollFailures++;
                if (pollFailures >= 3) {
                    updateServiceCard('iscsi', { enabled: false, port: 0 });
                    updateServiceCard('dhcp', { enabled: false, port: 0 });
                    updateServiceCard('tftp', { enabled: false, port: 0 });
                }
            }
        } catch (e) {
            console.error('Stats poll error:', e);
            pollFailures++;
            if (pollFailures >= 3) {
                updateServiceCard('iscsi', { enabled: false, port: 0 });
                updateServiceCard('dhcp', { enabled: false, port: 0 });
                updateServiceCard('tftp', { enabled: false, port: 0 });
            }
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
            const statusClass = `client-status-badge inline-flex items-center gap-1 text-[10px] font-medium px-1.5 py-0.5 rounded-md ${statsInfo.active ? 'bg-emerald-50/70 dark:bg-emerald-950/40 text-emerald-700 dark:text-emerald-400 border border-emerald-200 dark:border-emerald-800' : 'bg-rose-50/70 dark:bg-rose-950/40 text-rose-700 dark:text-rose-400 border border-rose-200 dark:border-rose-800'}`;

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
                    uptimeEl.className = `client-uptime text-xs font-medium ${statsInfo.active ? 'text-zinc-900 dark:text-zinc-100' : 'text-zinc-400 dark:text-zinc-500'}`;
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
            pillEl.className = 'pill-status text-[11px] font-medium px-2 py-0.5 rounded-md bg-emerald-50 dark:bg-emerald-950/50 text-emerald-700 dark:text-emerald-400 border border-emerald-200 dark:border-emerald-800 whitespace-nowrap shrink-0';
        } else {
            pillEl.textContent = '🔴 Disabled';
            pillEl.className = 'pill-status text-[11px] font-medium px-2 py-0.5 rounded-md bg-rose-50 dark:bg-rose-950/50 text-rose-700 dark:text-rose-400 border border-rose-200 dark:border-rose-800 whitespace-nowrap shrink-0';
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
            <div class="flex flex-col items-center justify-center text-center gap-2">
                <div class="text-2xl">🔌</div>
                <h3 class="font-semibold text-sm text-zinc-900 dark:text-zinc-100 tracking-tight">Belum Ada Klien Aktif</h3>
                <p class="text-zinc-500 dark:text-zinc-400 text-xs max-w-sm">Klien yang terhubung dan menyala akan muncul di sini secara real-time.</p>
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
            ? `<span class="client-status-badge inline-flex items-center gap-1 text-[10px] font-medium px-1.5 py-0.5 rounded-md bg-emerald-50/70 dark:bg-emerald-950/40 text-emerald-700 dark:text-emerald-400 border border-emerald-200 dark:border-emerald-800">🟢 Online</span>`
            : `<span class="client-status-badge inline-flex items-center gap-1 text-[10px] font-medium px-1.5 py-0.5 rounded-md bg-rose-50/70 dark:bg-rose-950/40 text-rose-700 dark:text-rose-400 border border-rose-200 dark:border-rose-800">🔴 Offline</span>`;

        const isSuper = configObj && configObj.windows && configObj.windows.super_client_ip === c.ip;
        const superBadge = isSuper ? ` <span class="inline-flex items-center text-[10px] font-medium px-1.5 py-0.5 rounded-md bg-amber-50 dark:bg-amber-950/40 text-amber-700 dark:text-amber-400 border border-amber-200 dark:border-amber-800 ml-1">⚡ Super</span>` : '';
        const dynamicBadge = c.isDynamic ? ` <span class="inline-flex items-center text-[10px] font-medium px-1.5 py-0.5 rounded-md bg-zinc-100 dark:bg-zinc-800 text-zinc-700 dark:text-zinc-300 border border-zinc-200 dark:border-zinc-700 ml-1">DHCP</span>` : '';

        const row = document.createElement('tr');
        row.setAttribute('data-ip', c.ip);
        row.className = "hover:bg-zinc-50/80 dark:hover:bg-zinc-900/50 transition-colors border-b border-zinc-100 dark:border-zinc-800/80 cursor-pointer";
        row.innerHTML = `
            <td class="py-2 px-2.5 xl:px-3 min-w-0">
                <div class="flex items-center gap-1.5 min-w-0">
                    ${statusSpan}
                    <span class="font-medium text-zinc-900 dark:text-zinc-100 text-xs truncate max-w-[90px] sm:max-w-[130px] lg:max-w-[100px] xl:max-w-[140px]" title="${c.hostname || c.ip}">${c.hostname || c.ip}</span>
                    ${superBadge}${dynamicBadge}
                </div>
                <div class="text-[10.5px] text-zinc-500 dark:text-zinc-400 font-mono mt-0.5 truncate" title="${c.ip}${c.mac ? ' • ' + c.mac : ''}">${c.ip}<span class="hidden 2xl:inline">${c.mac ? ' • ' + c.mac : ''}</span></div>
            </td>
            <td class="py-2 px-2.5 xl:px-3 min-w-0">
                <div class="text-[11px] text-zinc-700 dark:text-zinc-300 font-mono truncate"><span class="text-zinc-400 dark:text-zinc-500 font-sans font-medium">GW:</span> ${c.gateway || '-'}</div>
                <div class="text-[10.5px] text-zinc-500 dark:text-zinc-400 font-mono mt-0.5 truncate"><span class="text-zinc-400 dark:text-zinc-500 font-sans font-medium">DNS:</span> ${c.dns || '-'} <span class="text-zinc-300 dark:text-zinc-700 mx-0.5">•</span> <span class="text-zinc-400 dark:text-zinc-500 font-sans font-medium">Next:</span> ${c.next_server || '-'}</div>
            </td>
            <td class="py-2 px-2.5 xl:px-3 min-w-0">
                <div class="text-xs font-medium text-zinc-800 dark:text-zinc-200 flex items-center gap-1 min-w-0">
                    <span class="text-zinc-400 text-xs shrink-0">💿</span>
                    <span class="bg-zinc-100 dark:bg-zinc-800 border border-zinc-200 dark:border-zinc-700 rounded px-1 py-0.5 font-mono text-[10.5px] text-zinc-900 dark:text-zinc-100 truncate max-w-[90px] lg:max-w-[95px] xl:max-w-[130px] inline-block" title="${c.image_manager || 'None (Gamedisk)'}">${c.image_manager || 'None (Gamedisk)'}</span>
                </div>
                <div class="text-[10.5px] text-zinc-500 dark:text-zinc-400 font-mono mt-0.5 truncate"><span class="text-zinc-400 dark:text-zinc-500 font-sans font-medium">PXE:</span> ${c.pxe || 'Default'}</div>
            </td>
            <td class="py-2 px-2.5 xl:px-3 min-w-0">
                <div class="client-read-total text-[11px] font-mono font-medium text-zinc-700 dark:text-zinc-300 truncate">${formatBytes(statsInfo.bytes_read)}</div>
                <div class="client-read-speed text-[10.5px] font-mono font-medium text-zinc-600 dark:text-zinc-400 mt-0.5 truncate">⚡ ${formatSpeed(speedInfo.readSpeed)}</div>
            </td>
            <td class="py-2 px-2.5 xl:px-3 min-w-0">
                <div class="client-write-total text-[11px] font-mono font-medium text-zinc-700 dark:text-zinc-300 truncate">${formatBytes(statsInfo.bytes_written)}</div>
                <div class="client-write-speed text-[10.5px] font-mono font-medium text-zinc-600 dark:text-zinc-400 mt-0.5 truncate">⚡ ${formatSpeed(speedInfo.writeSpeed)}</div>
            </td>
            <td class="py-2 px-2.5 xl:px-3 min-w-0">
                <div class="client-uptime text-xs font-medium ${statsInfo.active ? 'text-zinc-900 dark:text-zinc-100' : 'text-zinc-400 dark:text-zinc-500'} truncate">${statsInfo.active ? formatDuration(statsInfo.uptime_secs) : 'Offline'}</div>
                <div class="text-[10px] text-zinc-400 dark:text-zinc-500 font-mono mt-0.5 truncate">${statsInfo.active ? 'Live Session' : 'Standby'}</div>
            </td>
        `;

        // Right-click context menu
        row.addEventListener('contextmenu', (e) => {
            e.preventDefault();
            showContextMenu(e, { ip: c.ip, active: statsInfo.active, image_manager: c.image_manager, clientObj: c });
        });

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
            <div class="flex flex-col items-center justify-center text-center gap-2">
                <div class="text-2xl">💻</div>
                <h3 class="font-semibold text-sm text-zinc-900 dark:text-zinc-100 tracking-tight">Daftar Klien Kosong</h3>
                <p class="text-zinc-500 dark:text-zinc-400 text-xs max-w-sm">Klik tombol "Tambah Klien" untuk mulai mendaftarkan PC diskless Anda.</p>
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
            ? `<span class="client-status-badge inline-flex items-center gap-1 text-[10px] font-medium px-1.5 py-0.5 rounded-md bg-emerald-50/70 dark:bg-emerald-950/40 text-emerald-700 dark:text-emerald-400 border border-emerald-200 dark:border-emerald-800">🟢 Online</span>`
            : `<span class="client-status-badge inline-flex items-center gap-1 text-[10px] font-medium px-1.5 py-0.5 rounded-md bg-rose-50/70 dark:bg-rose-950/40 text-rose-700 dark:text-rose-400 border border-rose-200 dark:border-rose-800">🔴 Offline</span>`;

        const isSuper = configObj && configObj.windows && configObj.windows.super_client_ip === c.ip;
        const superBadge = isSuper ? ` <span class="inline-flex items-center text-[10px] font-medium px-1.5 py-0.5 rounded-md bg-amber-50 dark:bg-amber-950/40 text-amber-700 dark:text-amber-400 border border-amber-200 dark:border-amber-800 ml-1">⚡ Super</span>` : '';

        const row = document.createElement('tr');
        row.setAttribute('data-ip', c.ip);
        row.className = "hover:bg-zinc-50/80 dark:hover:bg-zinc-900/50 transition-colors border-b border-zinc-100 dark:border-zinc-800/80 cursor-pointer";
        row.innerHTML = `
            <td class="py-2 px-2.5 xl:px-3 min-w-0">
                <div class="flex items-center gap-1.5 min-w-0">
                    ${statusSpan}
                    <span class="font-medium text-zinc-900 dark:text-zinc-100 text-xs truncate max-w-[90px] sm:max-w-[130px] lg:max-w-[100px] xl:max-w-[140px]" title="${c.hostname || 'PC'}">${c.hostname || 'PC'}</span>
                    ${superBadge}
                </div>
                <div class="text-[10.5px] text-zinc-500 dark:text-zinc-400 font-mono mt-0.5 truncate" title="${c.ip} • ${c.mac}">${c.ip}<span class="hidden 2xl:inline"> • ${c.mac}</span></div>
            </td>
            <td class="py-2 px-2.5 xl:px-3 min-w-0">
                <div class="text-[11px] text-zinc-700 dark:text-zinc-300 font-mono truncate"><span class="text-zinc-400 dark:text-zinc-500 font-sans font-medium">GW:</span> ${c.gateway || '-'}</div>
                <div class="text-[10.5px] text-zinc-500 dark:text-zinc-400 font-mono mt-0.5 truncate"><span class="text-zinc-400 dark:text-zinc-500 font-sans font-medium">DNS:</span> ${c.dns || '-'} <span class="text-zinc-300 dark:text-zinc-700 mx-0.5">•</span> <span class="text-zinc-400 dark:text-zinc-500 font-sans font-medium">Next:</span> ${c.next_server || '-'}</div>
            </td>
            <td class="py-2 px-2.5 xl:px-3 min-w-0">
                <div class="text-xs font-medium text-zinc-800 dark:text-zinc-200 flex items-center gap-1 min-w-0">
                    <span class="text-zinc-400 text-xs shrink-0">💿</span>
                    <span class="bg-zinc-100 dark:bg-zinc-800 border border-zinc-200 dark:border-zinc-700 rounded px-1 py-0.5 font-mono text-[10.5px] text-zinc-900 dark:text-zinc-100 truncate max-w-[90px] lg:max-w-[95px] xl:max-w-[130px] inline-block" title="${c.image_manager || 'Gamedisk'}">${c.image_manager || 'Gamedisk'}</span>
                </div>
                <div class="text-[10.5px] text-zinc-500 dark:text-zinc-400 font-mono mt-0.5 truncate"><span class="text-zinc-400 dark:text-zinc-500 font-sans font-medium">PXE:</span> ${c.pxe || 'Default'}</div>
            </td>
            <td class="py-2 px-2.5 xl:px-3 min-w-0">
                <div class="client-read-total text-[11px] font-mono font-medium text-zinc-700 dark:text-zinc-300 truncate">${formatBytes(statsInfo.bytes_read)}</div>
                <div class="client-read-speed text-[10.5px] font-mono font-medium text-zinc-600 dark:text-zinc-400 mt-0.5 truncate">⚡ ${formatSpeed(speedInfo.readSpeed)}</div>
            </td>
            <td class="py-2 px-2.5 xl:px-3 min-w-0">
                <div class="client-write-total text-[11px] font-mono font-medium text-zinc-700 dark:text-zinc-300 truncate">${formatBytes(statsInfo.bytes_written)}</div>
                <div class="client-write-speed text-[10.5px] font-mono font-medium text-zinc-600 dark:text-zinc-400 mt-0.5 truncate">⚡ ${formatSpeed(speedInfo.writeSpeed)}</div>
            </td>
            <td class="py-2 px-2.5 xl:px-3 min-w-0">
                <div class="client-uptime text-xs font-medium ${statsInfo.active ? 'text-zinc-900 dark:text-zinc-100' : 'text-zinc-400 dark:text-zinc-500'} truncate">${statsInfo.active ? formatDuration(statsInfo.uptime_secs) : 'Offline'}</div>
                <div class="text-[10px] text-zinc-400 dark:text-zinc-500 font-mono mt-0.5 truncate">${statsInfo.active ? 'Live Session' : 'Standby'}</div>
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
            <div class="flex flex-col items-center justify-center text-center gap-2">
                <div class="text-2xl">💿</div>
                <h3 class="font-semibold text-sm text-zinc-900 dark:text-zinc-100 tracking-tight">Belum Ada VHD</h3>
                <p class="text-zinc-500 dark:text-zinc-400 text-xs max-w-sm">Daftarkan file VHD Windows yang akan di-boot oleh klien Anda.</p>
            </div>
        </td></tr>`;
        return;
    }

    tbody.innerHTML = '';
    Object.entries(configObj.image_manager).forEach(([key, path]) => {
        const row = document.createElement('tr');
        row.className = "hover:bg-zinc-50/80 dark:hover:bg-zinc-900/50 transition-colors border-b border-zinc-100 dark:border-zinc-800/80";
        const safeKey = key.replace(/'/g, "\\'");
        row.innerHTML = `
            <td class="py-2.5 px-3 sm:py-3 sm:px-3.5 min-w-0">
                <div class="flex items-center gap-2">
                    <span class="text-sm">💿</span>
                    <span class="font-mono text-xs sm:text-sm font-semibold text-zinc-900 dark:text-zinc-100">${key}</span>
                </div>
                <div class="text-[11px] font-mono text-zinc-500 dark:text-zinc-400 truncate max-w-full mt-0.5" title="${path}">${path}</div>
            </td>
            <td class="py-2.5 px-3 sm:py-3 sm:px-3.5 min-w-0">
                <div class="text-xs font-medium text-zinc-800 dark:text-zinc-200" id="snapshots-count-${key}">Loading...</div>
                <div class="text-[11px] text-zinc-400 dark:text-zinc-500 mt-0.5">Auto Snapshot Ready</div>
            </td>
            <td class="py-2.5 px-2.5 sm:py-3 sm:px-3.5 text-right whitespace-nowrap">
                <div class="inline-flex items-center gap-1.5 justify-end">
                    <button class="btn-secondary inline-flex items-center justify-center px-2 sm:px-2.5 py-1 text-[11px] xl:text-xs font-medium cursor-pointer" onclick="openVhdCrudModal('${safeKey}')">Edit</button>
                    <button class="btn-primary inline-flex items-center justify-center px-2 sm:px-2.5 py-1 text-[11px] xl:text-xs font-medium shadow-2xs cursor-pointer" onclick="showVhdSnapshots('${safeKey}')">Snapshots</button>
                </div>
            </td>
        `;
        tbody.appendChild(row);

        fetchSnapshotsCount(key);
    });
}

async function fetchSnapshotsCount(key) {
    const el = document.getElementById(`snapshots-count-${key}`);
    const data = await apiGet(`/api/vhd/backups?image_key=${encodeURIComponent(key)}`);
    if (data && Array.isArray(data) && el) {
        el.textContent = `${data.length} snapshots`;
    } else if (el) {
        el.textContent = '0 snapshots';
    }
}

function openVhdCrudModal(key = null) {
    const modal = document.getElementById('vhd-crud-modal');
    modal.style.display = 'flex';

    if (key && configObj && configObj.image_manager && configObj.image_manager[key] !== undefined) {
        const path = configObj.image_manager[key] || '';
        document.getElementById('vhd-modal-title').textContent = 'Edit VHD Mapping';
        document.getElementById('vhd-old-key').value = key;
        document.getElementById('vhd-key').value = key;
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

    if (!newKey) {
        showToast('Alias / Image Key wajib diisi!', 'error');
        return;
    }

    if (!path) {
        showToast('Path file VHD wajib diisi!', 'error');
        return;
    }

    // Validasi format path VHD
    const lowerPath = path.toLowerCase();
    const hasVhdExt = lowerPath.endsWith('.vhd') || lowerPath.endsWith('.vhdx');
    const hasPathSep = path.includes('\\') || path.includes('/');
    const hasDriveOrAbsolute = /^[a-zA-Z]:[\\\/]/.test(path) || path.startsWith('\\\\') || path.startsWith('/');

    if (!hasVhdExt) {
        showToast('File VHD harus berakhiran .vhd atau .vhdx!', 'error');
        return;
    }

    if (!hasPathSep && !hasDriveOrAbsolute) {
        showToast('Format path tidak valid! Masukkan path lengkap file VHD (contoh: D:\\Images\\Windows10.vhd)', 'error');
        return;
    }

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
    tbody.innerHTML = `<tr><td colspan="3" class="py-6 px-4 text-center text-zinc-500 dark:text-zinc-400">Memuat snapshot...</td></tr>`;

    const data = await apiGet(`/api/vhd/backups?image_key=${encodeURIComponent(imageKey)}`);
    if (!data || !Array.isArray(data) || data.length === 0) {
        tbody.innerHTML = `<tr><td colspan="3" class="py-6 px-4 text-center text-zinc-500 dark:text-zinc-400">Belum ada file snapshot backup untuk image ini.</td></tr>`;
        return;
    }

    tbody.innerHTML = '';
    data.forEach(snap => {
        const row = document.createElement('tr');
        row.className = "hover:bg-zinc-50/80 dark:hover:bg-zinc-900/50 border-b border-zinc-100 dark:border-zinc-800/80";
        const displayName = snap.name || (snap.path ? snap.path.split(/[\\/]/).pop() : `Snapshot #${snap.index}`);
        const displaySize = snap.size ? formatBytes(snap.size) : 'Auto Meta';
        const dateDisplay = snap.date ? `<span class="text-[10px] text-zinc-400 dark:text-zinc-500 block">${snap.date}</span>` : '';
        const safeImgKey = imageKey.replace(/'/g, "\\'");
        const safeName = displayName.replace(/'/g, "\\'");

        row.innerHTML = `
            <td class="py-2.5 px-3.5">
                <div class="font-mono text-xs font-medium text-zinc-900 dark:text-zinc-100 flex items-center gap-1.5">
                    <span>💾</span>
                    <span>${displayName}</span>
                    <span class="text-[10px] text-zinc-400 dark:text-zinc-500 font-normal">#${snap.index}</span>
                </div>
                <div class="text-[11px] font-mono text-zinc-500 dark:text-zinc-400 truncate max-w-xs mt-0.5" title="${snap.path}">${snap.path}</div>
                ${dateDisplay}
            </td>
            <td class="py-2.5 px-3.5 font-mono text-xs text-zinc-600 dark:text-zinc-400">
                <span class="inline-flex items-center px-2 py-0.5 rounded bg-zinc-100 dark:bg-zinc-800 border border-zinc-200 dark:border-zinc-700 text-zinc-700 dark:text-zinc-300 text-[11px]">${displaySize}</span>
            </td>
            <td class="py-2.5 px-3.5 text-right">
                <button class="btn-secondary inline-flex items-center justify-center px-2.5 py-1 text-xs font-medium hover:border-amber-400 dark:hover:border-amber-600 hover:text-amber-700 dark:hover:text-amber-400 transition-colors cursor-pointer" onclick="restoreSnapshotAction('${safeImgKey}', ${snap.index}, '${safeName}')">⏪ Restore</button>
            </td>
        `;
        tbody.appendChild(row);
    });
}

function closeSnapshotsModal() {
    document.getElementById('snapshots-modal').style.display = 'none';
}

async function restoreSnapshotAction(imageKey, index, displayName) {
    showConfirmModal('Restore Snapshot', `Apakah Anda yakin ingin me-restore master VHD (${imageKey}) ke snapshot "${displayName}"? Master VHD akan dikembalikan ke titik snapshot ini dan differencing snapshot terkait akan dibersihkan.`, async () => {
        showToast(`Memproses restore ${displayName}...`, 'info');
        const res = await apiPost('/api/vhd/restore', { image_key: imageKey, index: Number(index) });
        if (res && (res.status === 'ok' || res.status === 'success')) {
            showToast(res.message || 'Master VHD berhasil di-restore ke snapshot!', 'success');
            closeSnapshotsModal();
            if (configObj && configObj.windows) {
                configObj.windows.super_client_ip = '';
                configObj.windows.super_client_action = '';
            }
            await loadConfigJson();
            renderVhdTable();
            renderClientsManagerTable();
            renderDashboardClientsTable();
        } else {
            showToast((res && res.message) ? res.message : 'Gagal me-restore snapshot', 'error');
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
        container.innerHTML = `<div class="col-span-full py-10 text-center text-zinc-500 dark:text-zinc-400 text-xs">Memindai disk fisik...</div>`;
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

        // Shadcn role styling configuration
        const roleConfig = {
            boot: {
                label: 'BOOT VHD',
                icon: '💿',
                badgeClass: 'bg-zinc-100 text-zinc-900 border-zinc-300 dark:bg-zinc-800 dark:text-zinc-100 dark:border-zinc-700',
                cardBorder: 'border-zinc-300 dark:border-zinc-700',
                desc: 'Master OS VHD',
                colorAccent: 'text-zinc-900 dark:text-zinc-100'
            },
            writeback: {
                label: 'WRITEBACK',
                icon: '⚡',
                badgeClass: 'bg-amber-50 text-amber-700 border-amber-200 dark:bg-amber-950/50 dark:text-amber-400 dark:border-amber-800',
                cardBorder: 'border-zinc-200 dark:border-zinc-800',
                desc: 'Client Cache I/O',
                colorAccent: 'text-amber-600 dark:text-amber-400'
            },
            gamedisk: {
                label: 'GAMEDISK',
                icon: '🎮',
                badgeClass: 'bg-emerald-50 text-emerald-700 border-emerald-200 dark:bg-emerald-950/50 dark:text-emerald-400 dark:border-emerald-800',
                cardBorder: 'border-zinc-200 dark:border-zinc-800',
                desc: 'Game Storage Target',
                colorAccent: 'text-emerald-600 dark:text-emerald-400'
            },
            none: {
                label: 'UNASSIGNED',
                icon: '⚪',
                badgeClass: 'bg-zinc-100 text-zinc-500 border-zinc-200 dark:bg-zinc-800 dark:text-zinc-400 dark:border-zinc-700',
                cardBorder: 'border-zinc-200 dark:border-zinc-800',
                desc: 'Belum dialokasikan',
                colorAccent: 'text-zinc-500 dark:text-zinc-400'
            }
        }[currentRole] || {
            label: 'UNASSIGNED',
            icon: '⚪',
            badgeClass: 'bg-zinc-100 text-zinc-500 border-zinc-200 dark:bg-zinc-800 dark:text-zinc-400 dark:border-zinc-700',
            cardBorder: 'border-zinc-200 dark:border-zinc-800',
            desc: 'Belum dialokasikan',
            colorAccent: 'text-zinc-500 dark:text-zinc-400'
        };

        const card = document.createElement('div');
        card.className = `bg-white dark:bg-zinc-900 border rounded-lg p-4 sm:p-5 flex flex-col justify-between shadow-2xs transition-colors ${roleConfig.cardBorder}`;

        card.innerHTML = `
            <div>
                <!-- Top Header: Drive Name & Physical Disk -->
                <div class="flex items-center justify-between gap-2 pb-3 border-b border-zinc-100 dark:border-zinc-800 mb-3.5">
                    <div class="flex items-center gap-2.5 min-w-0">
                        <div class="w-8 h-8 rounded-md bg-zinc-100 dark:bg-zinc-800 flex items-center justify-center text-sm shrink-0 font-mono">
                            🗄️
                        </div>
                        <div class="min-w-0 flex-1">
                            <h3 class="font-semibold text-sm sm:text-base text-zinc-900 dark:text-zinc-100 leading-tight">Drive ${drive.letter}:\\</h3>
                            <p class="text-[11px] text-zinc-500 dark:text-zinc-400 font-mono mt-0.5 break-all leading-tight">${drive.physical_disk || 'Logical Volume (Direct)'}</p>
                        </div>
                    </div>
                    <span class="inline-flex items-center gap-1 text-[10px] font-medium px-2 py-0.5 rounded-md border shrink-0 ${roleConfig.badgeClass}">
                        <span>${roleConfig.icon}</span>
                        <span>${roleConfig.label}</span>
                    </span>
                </div>

                <!-- Role Info Description -->
                <div class="text-[11px] text-zinc-500 dark:text-zinc-400 mb-3.5 flex items-center justify-between px-0.5">
                    <span class="font-medium">Fungsi Disk:</span>
                    <span class="font-medium ${roleConfig.colorAccent}">${roleConfig.desc}</span>
                </div>

                <!-- Interactive Allocation Action Button -->
                <button type="button" class="w-full px-3 py-2 rounded-md border border-zinc-200 dark:border-zinc-800 bg-zinc-50/50 dark:bg-zinc-950/50 hover:bg-zinc-100 dark:hover:bg-zinc-800/80 cursor-pointer flex items-center justify-between transition-colors text-left" onclick="openPartitionModal('${letter}', '${currentRole}')">
                    <div class="flex items-center gap-1.5">
                        <span class="text-xs">⚙️</span>
                        <span class="font-medium text-xs text-zinc-800 dark:text-zinc-200">Ubah Alokasi Role</span>
                    </div>
                    <span class="text-xs text-zinc-400 font-medium">→</span>
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

// SCSI Disk Identity & Vendor Branding Handlers
function loadDiskBrandingParams() {
    if (!configObj) return;

    const winCfg = configObj.windows || {};
    const gdList = configObj.gamedisk || [];
    const firstGd = gdList[0] || {};

    const osVendorEl = document.getElementById('disk-vendor-os');
    const osProductEl = document.getElementById('disk-product-os');
    const gmVendorEl = document.getElementById('disk-vendor-game');
    const gmProductEl = document.getElementById('disk-product-game');
    const revEl = document.getElementById('disk-revision-common');

    if (osVendorEl) osVendorEl.value = winCfg.vendor_id || 'RUSTISCS';
    if (osProductEl) osProductEl.value = winCfg.product_id || 'WindowsBoot';
    if (gmVendorEl) gmVendorEl.value = firstGd.vendor_id || 'RUSTISCS';
    if (gmProductEl) gmProductEl.value = firstGd.product_id || 'GameDisk-0';
    if (revEl) revEl.value = winCfg.product_revision || firstGd.product_revision || '1.00';

    updateDiskBrandingPreview();
}

function updateDiskBrandingPreview() {
    const osVendor = (document.getElementById('disk-vendor-os')?.value || 'RUSTISCS').trim().toUpperCase();
    const osProduct = (document.getElementById('disk-product-os')?.value || 'WindowsBoot').trim();
    const gmVendor = (document.getElementById('disk-vendor-game')?.value || 'RUSTISCS').trim().toUpperCase();
    const gmProduct = (document.getElementById('disk-product-game')?.value || 'GameDisk-0').trim();

    const previewOs = document.getElementById('preview-os-disk-text');
    if (previewOs) {
        previewOs.textContent = `${osVendor || 'RUSTISCS'} ${osProduct || 'WindowsBoot'} SCSI Disk Device`;
    }

    const previewGame = document.getElementById('preview-game-disk-text');
    if (previewGame) {
        previewGame.textContent = `${gmVendor || 'RUSTISCS'} ${gmProduct || 'GameDisk-0'} SCSI Disk Device`;
    }
}

function applyDiskBrandingPreset(presetKey) {
    const presets = {
        hypernvme: {
            osVendor: 'HYPERVMD', osProduct: 'Gen5 NVMe Boost',
            gmVendor: 'HYPERVMD', gmProduct: 'GameVault Pro',
            rev: '2.00'
        },
        quantum: {
            osVendor: 'QUANTUM', osProduct: 'Q-Drive Master',
            gmVendor: 'QUANTUM', gmProduct: 'Q-Array HighIO',
            rev: '1.00'
        },
        apex: {
            osVendor: 'APEXPULS', osProduct: 'Prime OS Drive',
            gmVendor: 'APEXPULS', gmProduct: 'Titan GameDisk',
            rev: '1.00'
        },
        genesis: {
            osVendor: 'GENESIS', osProduct: 'Virtual Boot',
            gmVendor: 'GENESIS', gmProduct: 'Fast Storage',
            rev: '1.00'
        },
        stealth: {
            osVendor: 'STEALTH', osProduct: 'ZeroLatency SSD',
            gmVendor: 'STEALTH', gmProduct: 'StreamDisk Pro',
            rev: '1.00'
        },
        default: {
            osVendor: 'RUSTISCS', osProduct: 'WindowsBoot',
            gmVendor: 'RUSTISCS', gmProduct: 'GameDisk-0',
            rev: '1.00'
        }
    };

    const p = presets[presetKey] || presets.default;
    const osVendorEl = document.getElementById('disk-vendor-os');
    const osProductEl = document.getElementById('disk-product-os');
    const gmVendorEl = document.getElementById('disk-vendor-game');
    const gmProductEl = document.getElementById('disk-product-game');
    const revEl = document.getElementById('disk-revision-common');

    if (osVendorEl) osVendorEl.value = p.osVendor;
    if (osProductEl) osProductEl.value = p.osProduct;
    if (gmVendorEl) gmVendorEl.value = p.gmVendor;
    if (gmProductEl) gmProductEl.value = p.gmProduct;
    if (revEl) revEl.value = p.rev;

    updateDiskBrandingPreview();
}

async function saveDiskBrandingAction() {
    if (!configObj) configObj = {};
    if (!configObj.windows) configObj.windows = {};

    const osVendor = (document.getElementById('disk-vendor-os')?.value || 'RUSTISCS').trim().toUpperCase();
    const osProduct = (document.getElementById('disk-product-os')?.value || 'WindowsBoot').trim();
    const gmVendor = (document.getElementById('disk-vendor-game')?.value || 'RUSTISCS').trim().toUpperCase();
    const gmProduct = (document.getElementById('disk-product-game')?.value || 'GameDisk-0').trim();
    const rev = (document.getElementById('disk-revision-common')?.value || '1.00').trim();

    configObj.windows.vendor_id = osVendor || 'RUSTISCS';
    configObj.windows.product_id = osProduct || 'WindowsBoot';
    configObj.windows.product_revision = rev || '1.00';

    if (Array.isArray(configObj.gamedisk)) {
        configObj.gamedisk.forEach((gd, idx) => {
            gd.vendor_id = gmVendor || 'RUSTISCS';
            gd.product_id = configObj.gamedisk.length > 1 ? `${gmProduct}-${idx}` : gmProduct;
            gd.product_revision = rev || '1.00';
        });
    }

    const ok = await saveConfigJsonFull();
    if (ok) {
        showToast('Identitas vendor disk berhasil disimpan & diperbarui!', 'success');
        await loadConfigJson();
    } else {
        showToast('Gagal menyimpan identitas disk', 'error');
    }
}

async function loadWritebackFiles() {
    const data = await apiGet('/api/writeback/files');
    const tbody = document.getElementById('writeback-files-tbody');
    if (!tbody) return;

    if (!data || data.length === 0) {
        tbody.innerHTML = `<tr><td colspan="2" class="py-6 px-4 text-center text-zinc-500 dark:text-zinc-400 text-xs">Tidak ada file cache writeback aktif.</td></tr>`;
        return;
    }

    tbody.innerHTML = '';
    data.forEach(item => {
        const row = document.createElement('tr');
        row.className = "hover:bg-zinc-50/80 dark:hover:bg-zinc-900/50 border-b border-zinc-100 dark:border-zinc-800/80";
        row.innerHTML = `
            <td class="py-2.5 px-3.5 font-mono text-xs font-medium text-zinc-900 dark:text-zinc-100">${item.name || item.path}</td>
            <td class="py-2.5 px-3.5 font-mono text-xs text-zinc-500 dark:text-zinc-400">${formatBytes(item.size)}</td>
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
            const autoAddEl = document.getElementById('set-dhcp-auto-add');
            if (autoAddEl) autoAddEl.checked = currentCfg.dhcp.auto_add_client !== false;
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
        loadDiskBrandingParams();
        loadTftpFolders();
        populateNetworkDropdowns();
    }
}

function renderNicIpsList(nicIps) {
    const container = document.getElementById('nic-ips-list-container');
    if (!container) return;

    if (!nicIps || nicIps.length === 0) {
        container.innerHTML = `<span class="text-zinc-500 dark:text-zinc-400 text-center text-xs py-1">Belum ada IP ditambahkan.</span>`;
        return;
    }

    container.innerHTML = '';
    nicIps.forEach(ip => {
        const tag = document.createElement('div');
        tag.className = "flex items-center justify-between px-2.5 py-1 rounded-md bg-white dark:bg-zinc-900 border border-zinc-200 dark:border-zinc-800";
        tag.innerHTML = `
            <span class="font-mono text-xs text-zinc-900 dark:text-zinc-100">${ip}</span>
            <button type="button" class="text-rose-600 dark:text-rose-400 hover:text-rose-700 font-medium p-0.5 cursor-pointer" onclick="removeNicIpAction('${ip}')">✕</button>
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
        auto_add_client: document.getElementById('set-dhcp-auto-add')?.checked ?? true,
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
        if (tbody) tbody.innerHTML = `<tr><td colspan="2" class="py-6 px-4 text-center text-zinc-500 dark:text-zinc-400 text-xs">Belum ada folder bootloader kustom.</td></tr>`;
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
            row.className = "hover:bg-zinc-50/80 dark:hover:bg-zinc-900/50 border-b border-zinc-100 dark:border-zinc-800/80";
            row.innerHTML = `
                <td class="py-2.5 px-4 font-mono text-xs font-medium text-zinc-900 dark:text-zinc-100">${f}</td>
                <td class="py-2.5 px-4 text-right">
                    <button type="button" class="text-rose-600 dark:text-rose-400 hover:text-rose-700 text-xs font-medium cursor-pointer" onclick="deleteTftpFolderAction('${f}')">Hapus</button>
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

let superClientTarget = null;

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
    menu.style.top = `${Math.min(e.pageY, window.innerHeight - 160)}px`;
}

function hideContextMenu() {
    const menu = document.getElementById('context-menu');
    if (menu) menu.style.display = 'none';
}

async function ctxToggleSuperClient() {
    if (!selectedContextClient) return;
    const ip = selectedContextClient.ip;
    const isSuper = configObj && configObj.windows && configObj.windows.super_client_ip === ip;
    hideContextMenu();

    if (isSuper) {
        openSuperClientDisableModal(selectedContextClient);
    } else {
        // 1. Cek apakah client sedang online (active === true)
        const sessionInfo = activeSessionsMap.get(ip);
        const isOnline = (sessionInfo && sessionInfo.active === true) || 
                         (selectedContextClient && selectedContextClient.active === true);
        if (isOnline) {
            showToast(`Klien ${ip} sedang ONLINE! Matikan / shutdown PC klien terlebih dahulu sebelum mengaktifkan mode Super Client.`, 'error');
            return;
        }

        // 2. Cek apakah sudah ada PC lain yang sedang menjadi Super Client
        const activeSuperIp = configObj && configObj.windows && configObj.windows.super_client_ip;
        if (activeSuperIp && activeSuperIp !== ip && activeSuperIp !== '') {
            showToast(`Hanya 1 PC yang dapat menjadi Super Client. IP ${activeSuperIp} saat ini masih aktif sebagai Super Client!`, 'error');
            return;
        }

        const res = await apiPost('/api/superclient/set', { ip, action: 'enable' });
        if (res && res.status === 'ok') {
            if (!configObj) configObj = {};
            if (!configObj.windows) configObj.windows = {};
            configObj.windows.super_client_ip = ip;
            configObj.windows.super_client_action = 'enable';

            showToast(res.message || `Super Client diaktifkan untuk IP ${ip}`, 'success');
            await loadConfigJson();
            renderClientsManagerTable();
            renderDashboardClientsTable();
        } else {
            showToast((res && res.message) ? res.message : 'Gagal mengaktifkan Super Client', 'error');
        }
    }
}

function ctxEnableSuperClient() {
    ctxToggleSuperClient();
}

// Super Client Disable Modal Handlers
function openSuperClientDisableModal(clientData) {
    if (!clientData) return;

    // Validasi: klien harus dalam keadaan OFFLINE sebelum Commit / Discard
    // — Commit/Discard saat klien masih aktif dapat menyebabkan korupsi VHD differencing.
    const ip = clientData.ip;
    const sessionInfo = activeSessionsMap.get(ip);
    const isOnline = (sessionInfo && sessionInfo.active === true) ||
                     (clientData.active === true);
    if (isOnline) {
        const name = (clientData.clientObj && clientData.clientObj.hostname) || ip;
        showToast(
            `Klien ${name} (${ip}) sedang ONLINE! Matikan / shutdown PC klien terlebih dahulu sebelum melakukan Commit atau Discard. Operasi pada VHD yang sedang aktif dapat menyebabkan korupsi data.`,
            'error'
        );
        return;
    }

    superClientTarget = clientData;
    const modal = document.getElementById('superclient-disable-modal');
    const targetLabel = document.getElementById('superclient-modal-target');
    if (targetLabel) {
        const name = (clientData.clientObj && clientData.clientObj.hostname) || ip;
        targetLabel.textContent = `Klien: ${name} (${ip})`;
    }
    if (modal) modal.style.display = 'flex';
}

function closeSuperClientDisableModal() {
    const modal = document.getElementById('superclient-disable-modal');
    if (modal) modal.style.display = 'none';
    superClientTarget = null;
}

// VHD Merge Progress Widget Controller
let mergePollingTimer = null;

function showMergeProgressWidget(pct = 0, msg = 'Mempersiapkan merge...') {
    const widget = document.getElementById('vhd-merge-progress-widget');
    if (!widget) return;
    widget.style.display = 'block';
    setTimeout(() => {
        widget.classList.remove('translate-y-32', 'opacity-0', 'pointer-events-none');
        widget.classList.add('translate-y-0', 'opacity-100', 'pointer-events-auto');
    }, 20);

    updateMergeProgressWidget(pct, msg);
}

function updateMergeProgressWidget(pct, msg) {
    const pctEl = document.getElementById('vhd-merge-progress-pct');
    const msgEl = document.getElementById('vhd-merge-progress-msg');
    const barEl = document.getElementById('vhd-merge-progress-bar');
    
    if (pctEl) pctEl.textContent = `${pct}%`;
    if (msgEl) msgEl.textContent = msg;
    if (barEl) barEl.style.width = `${pct}%`;
}

function hideMergeProgressWidget() {
    const widget = document.getElementById('vhd-merge-progress-widget');
    if (!widget) return;
    widget.classList.remove('translate-y-0', 'opacity-100', 'pointer-events-auto');
    widget.classList.add('translate-y-32', 'opacity-0', 'pointer-events-none');
    setTimeout(() => {
        widget.style.display = 'none';
    }, 350);
}

function startMergeProgressPolling() {
    if (mergePollingTimer) {
        clearInterval(mergePollingTimer);
        mergePollingTimer = null;
    }

    showMergeProgressWidget(0, 'Memulai proses merge VHD di background...');

    mergePollingTimer = setInterval(async () => {
        const data = await apiGet('/api/vhd/merge_status');
        if (!data) return;

        if (data.is_merging) {
            updateMergeProgressWidget(data.progress || 0, data.message || 'Menggabungkan differencing VHD...');
        } else {
            // Selesai atau Error
            clearInterval(mergePollingTimer);
            mergePollingTimer = null;

            if (data.error) {
                updateMergeProgressWidget(data.progress || 0, `Gagal: ${data.error}`);
                showToast(`Gagal merge VHD Super Client: ${data.error}`, 'error');
                setTimeout(() => hideMergeProgressWidget(), 5000);
            } else {
                updateMergeProgressWidget(100, 'Merge Selesai! Mode Super Client dinonaktifkan.');
                showToast('Commit Super Client berhasil! Master VHD telah diperbarui.', 'success');
                
                // Pastikan status config & tabel di-refresh secara penuh
                if (!configObj) configObj = {};
                if (!configObj.windows) configObj.windows = {};
                configObj.windows.super_client_ip = '';
                configObj.windows.super_client_action = '';
                renderedDashboardIps = [];
                await loadConfigJson();
                await loadClientsJson();
                renderClientsManagerTable();
                renderDashboardClientsTable();
                renderVhdTable();

                setTimeout(() => hideMergeProgressWidget(), 3000);
            }
        }
    }, 750);
}

async function checkInitialMergeStatus() {
    const data = await apiGet('/api/vhd/merge_status');
    if (data && data.is_merging) {
        startMergeProgressPolling();
    }
}

async function handleSuperClientDisableChoice(choice) {
    if (!superClientTarget) {
        closeSuperClientDisableModal();
        return;
    }

    const ip = superClientTarget.ip;
    const hostname = (superClientTarget.clientObj && superClientTarget.clientObj.hostname) || ip;
    closeSuperClientDisableModal();

    if (choice === 'commit') {
        showToast(`Memproses Commit Super Client (${hostname})...`, 'info');
        const res = await apiPost('/api/superclient/commit', { ip, hostname }, 15000);
        if (res && res.status === 'ok') {
            startMergeProgressPolling();
        } else {
            showToast((res && res.message) ? res.message : 'Gagal memproses commit Super Client', 'error');
        }
    } else if (choice === 'discard') {
        showToast(`Membatalkan perubahan Super Client (${hostname})...`, 'info');
        const res = await apiPost('/api/superclient/discard', { ip, hostname });
        if (res && res.status === 'ok') {
            if (!configObj) configObj = {};
            if (!configObj.windows) configObj.windows = {};
            configObj.windows.super_client_ip = '';
            configObj.windows.super_client_action = '';
            renderedDashboardIps = [];
            showToast(res.message || 'Perubahan Super Client berhasil dibatalkan (differencing dihapus)', 'success');
            await loadConfigJson();
            await loadClientsJson();
            renderClientsManagerTable();
            renderDashboardClientsTable();
        } else {
            showToast((res && res.message) ? res.message : 'Gagal membatalkan perubahan Super Client', 'error');
        }
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

// Service Management Quick Actions
async function restartServiceAction(serviceName) {
    const pill = document.getElementById(`${serviceName}-status-pill`);
    if (pill) {
        pill.textContent = '⏳ Restarting...';
        pill.className = 'pill-status text-[11px] sm:text-xs font-semibold px-2.5 py-1 rounded-full bg-amber-50 text-amber-700 border border-amber-200 whitespace-nowrap';
    }
    showToast(`Memulai restart layanan ${serviceName.toUpperCase()}...`, 'info');
    const res = await apiPost('/api/services/restart', { service: serviceName });
    if (res && res.status === 'ok') {
        setTimeout(async () => {
            showToast(`Layanan ${serviceName.toUpperCase()} berhasil di-restart secara instan!`, 'success');
            await loadInitialData();
        }, 300);
    } else {
        showToast(`Gagal merestart layanan ${serviceName.toUpperCase()}`, 'error');
    }
}

async function restartAllServicesAction() {
    const btn = document.getElementById('btn-restart-all-services');
    if (btn) btn.disabled = true;
    showToast('Memulai restart seluruh layanan Simple-Iscsi...', 'info');
    
    ['iscsi', 'dhcp', 'tftp'].forEach(s => {
        const pill = document.getElementById(`${s}-status-pill`);
        if (pill) {
            pill.textContent = '⏳ Restarting...';
            pill.className = 'pill-status text-[11px] sm:text-xs font-semibold px-2.5 py-1 rounded-full bg-amber-50 text-amber-700 border border-amber-200 whitespace-nowrap';
        }
    });

    const res = await apiPost('/api/services/restart', { service: 'all' });
    if (res && res.status === 'ok') {
        setTimeout(async () => {
            showToast('Seluruh layanan berhasil di-restart secara instan!', 'success');
            await loadInitialData();
            if (btn) btn.disabled = false;
        }, 400);
    } else {
        showToast('Gagal merestart layanan', 'error');
        if (btn) btn.disabled = false;
    }
}

