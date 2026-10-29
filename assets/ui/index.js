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
let selectedClientMacs = new Set();
let clientColumns = null;
let clientDrag = { active: false, mode: true, moved: false, suppressClickUntil: 0 };
let tftpFoldersCache = [];
let currentCacheDisks = [];

// App Lifecycle Initializer
document.addEventListener('DOMContentLoaded', async () => {
    initTheme();
    initContextMenus();
    initClientTableEvents();
    initClientPreferences();
    initPxeCombos();

    // Initial Data Sequence
    await loadInitialData();

    // Start Live Stats Polling Loop (1-second intervals)
    initStatsStream();

    // Background Auto-sync
    initAutoSyncIntervals();

    if (location.hash === '#logs' || location.search.includes('logs')) {
        switchTab('logs');
    }
});

// Theme Controller (Light / Dark Mode)
function initTheme() {
    const btn = document.getElementById('theme-toggle-btn');
    if (!btn) return;

    const applyTheme = (theme) => {
        if (theme === 'dark') {
            document.documentElement.classList.add('dark');
            document.body.classList.add('dark');
            btn.textContent = 'Mode Terang';
        } else {
            document.documentElement.classList.remove('dark');
            document.body.classList.remove('dark');
            btn.textContent = 'Mode Gelap';
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
let logsPollingTimer = null;
let logsText = '';

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

    // 5. Trigger cache stats load on tab switch
    if (tabId === 'cache') {
        loadCacheStats();
    }
    if (tabId === 'logs') {
        startLogsPolling();
    } else {
        stopLogsPolling();
    }
}

async function loadLogs(force = false) {
    if (!force && activeTab !== 'logs') return;
    const level = document.getElementById('logs-level')?.value || 'all';
    const data = await apiGet('/api/logs?lines=400&level=' + encodeURIComponent(level));
    if (!data) return;
    const lines = Array.isArray(data.lines) ? data.lines : [];
    logsText = lines.join('\n');
    const pre = document.getElementById('logs-pre');
    if (pre) {
        pre.textContent = logsText || '(log kosong)';
        const auto = document.getElementById('logs-autoscroll');
        if (auto && auto.checked) pre.scrollTop = pre.scrollHeight;
    }
    const pathEl = document.getElementById('logs-path');
    if (pathEl) pathEl.textContent = data.path || '-';
    const sizeEl = document.getElementById('logs-size');
    if (sizeEl) sizeEl.textContent = formatBytes(data.size || 0);
    const countEl = document.getElementById('logs-count');
    if (countEl) countEl.textContent = lines.length + ' baris';
}

function startLogsPolling() {
    if (logsPollingTimer) return;
    loadLogs(true);
    logsPollingTimer = setInterval(() => loadLogs(), 2000);
}

function stopLogsPolling() {
    if (logsPollingTimer) {
        clearInterval(logsPollingTimer);
        logsPollingTimer = null;
    }
}

function copyLogs() {
    if (!logsText) return;
    navigator.clipboard.writeText(logsText).then(() => showToast('Log disalin', 'success')).catch(() => showToast('Gagal menyalin log', 'error'));
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
        await checkInitialMergeStatus();
    } catch (err) {
        console.error('checkInitialMergeStatus error:', err);
    }
}

function initAutoSyncIntervals() {
    setInterval(() => {
        if (activeTab === 'disk-mgmt') {
            loadDiskPartitions();
                } else if (activeTab === 'cache') {
            loadCacheStats();
        }
    }, 2000);
    const levelEl = document.getElementById('logs-level');
    if (levelEl) levelEl.addEventListener('change', () => loadLogs(true));
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
    const timeoutId = timeoutMs > 0 ? setTimeout(() => controller.abort(), timeoutMs) : 0;
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
function escapeHtml(str) {
    if (str === null || str === undefined) return '';
    return String(str)
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

function renderCacheBadge(c) {
    if (!c || c.driver !== 'ccboot') return '';
    if (!c.cache_enabled) {
        return '<div class="mt-0.5"><span class="inline-flex items-center text-[10px] font-medium px-1.5 py-0.5 rounded-md bg-zinc-100 dark:bg-zinc-800 text-zinc-600 dark:text-zinc-400 border border-zinc-200 dark:border-zinc-700">Cache: Off</span></div>';
    }
    const label = c.cache_size_mb ? escapeHtml(String(c.cache_size_mb)) + ' MB' : 'Auto';
    return '<div class="mt-0.5"><span class="inline-flex items-center text-[10px] font-medium px-1.5 py-0.5 rounded-md bg-sky-50 dark:bg-sky-950/40 text-sky-700 dark:text-sky-400 border border-sky-200 dark:border-sky-800">Cache: ' + label + '</span></div>';
}

function onClientDriverChange() {
    const isCcboot = document.getElementById('client-driver').value === 'ccboot';
    const section = document.getElementById('client-ccachex-section');
    if (section) section.classList.toggle('hidden', !isCcboot);
    if (!isCcboot) {
        document.getElementById('client-cache-enabled').checked = false;
        document.getElementById('client-cache-size-mb').value = '0';
    }
    onClientCacheToggle();
    applyGlobalCacheState();
}

function applyGlobalCacheState() {
    const globalEl = document.getElementById('global-ccachex-enabled');
    const enabled = !!(globalEl && globalEl.checked);
    const cb = document.getElementById('client-cache-enabled');
    const size = document.getElementById('client-cache-size-mb');
    if (cb) {
        cb.disabled = !enabled;
        cb.title = enabled ? '' : 'Dimatikan global';
    }
    if (size) size.disabled = !enabled || !cb || !cb.checked;
}

function onClientCacheToggle() {
    const enabled = document.getElementById('client-cache-enabled').checked;
    const size = document.getElementById('client-cache-size-mb');
    size.disabled = !enabled;
    if (!enabled) size.value = '0';
}

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

function showToast(message, type = 'info', title = null) {
    const container = document.getElementById('toast-container');
    if (!container) return;

    const titles = { success: 'Berhasil', error: 'Gagal', warning: 'Perhatian', info: 'Info' };
    const dots = { success: 'bg-emerald-500', error: 'bg-rose-500', warning: 'bg-amber-500', info: 'bg-sky-500' };
    const durations = { success: 4000, info: 4000, warning: 5000, error: 6000 };
    const toast = document.createElement('div');
    toast.className = 'toast-msg pointer-events-auto flex items-start gap-2.5 px-3.5 py-3 rounded-md text-xs sm:text-sm border border-zinc-200 dark:border-zinc-700 bg-white dark:bg-zinc-900 text-zinc-900 dark:text-zinc-100 shadow-lg';
    toast.setAttribute('role', type === 'error' ? 'alert' : 'status');
    toast.innerHTML = '<span class="mt-1 h-2 w-2 shrink-0 rounded-full ' + (dots[type] || dots.info) + '"></span>'
        + '<div class="min-w-0 flex-1"><div class="font-medium leading-4">' + escapeHtml(title || titles[type] || titles.info) + '</div>'
        + '<div class="mt-0.5 text-[11.5px] leading-4 text-zinc-500 dark:text-zinc-400 break-words">' + escapeHtml(message == null ? '' : String(message)) + '</div></div>'
        + '<button type="button" class="shrink-0 text-zinc-400 hover:text-zinc-700 dark:hover:text-zinc-200 text-sm leading-none cursor-pointer" data-toast-close aria-label="Tutup">×</button>';

    const remove = () => {
        toast.classList.remove('show');
        setTimeout(() => toast.remove(), 200);
    };
    toast.querySelector('[data-toast-close]').addEventListener('click', remove);
    container.appendChild(toast);
    while (container.children.length > 3) container.firstElementChild.remove();
    requestAnimationFrame(() => toast.classList.add('show'));
    setTimeout(remove, durations[type] || 4000);
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
    const dhcpNext = document.getElementById('set-dhcp-next');
    const clientNext = document.getElementById('client-next-server');
    const nicSelect = document.getElementById('add-nic-ip-input');

    const currentServer = serverSelect ? serverSelect.value : '';
    const currentDhcp = dhcpNext ? dhcpNext.value : '';
    const currentClient = clientNext ? clientNext.value : '';
    const currentNic = nicSelect ? nicSelect.value : '';

    if (serverSelect) { serverSelect.innerHTML = ''; serverSelect.add(new Option('0.0.0.0 (Semua Interface)', '0.0.0.0')); }
    if (dhcpNext) { dhcpNext.innerHTML = ''; dhcpNext.add(new Option('Pilih IP...', '')); }
    if (clientNext) { clientNext.innerHTML = ''; clientNext.add(new Option('Auto / Global', '')); }
    if (nicSelect) { nicSelect.innerHTML = ''; nicSelect.add(new Option('Pilih IP NIC...', '')); }

    availableNetworkIps.forEach(ip => {
        if (ip !== '0.0.0.0') {
            if (serverSelect) serverSelect.add(new Option(ip, ip));
            if (dhcpNext) dhcpNext.add(new Option(ip, ip));
            if (clientNext) clientNext.add(new Option(ip, ip));
            if (nicSelect) nicSelect.add(new Option(ip, ip));
        }
    });

    const cfgAddr = configObj && configObj.server ? (Array.isArray(configObj.server.address) ? configObj.server.address[0] : configObj.server.address) : '';
    setSelectValueWithFallback(serverSelect, currentServer || cfgAddr || '0.0.0.0');
    setSelectValueWithFallback(dhcpNext, currentDhcp || (configObj && configObj.dhcp ? configObj.dhcp.next_server : '') || '');
    setSelectValueWithFallback(clientNext, currentClient || '');
    setSelectValueWithFallback(nicSelect, currentNic || '');
}

function setSelectValueWithFallback(select, value) {
    if (!select) return;
    if (value && !Array.from(select.options).some(o => o.value === value)) {
        select.add(new Option(value + ' (configured)', value));
    }
    select.value = value || (select.options[0] ? select.options[0].value : '');
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

            const statusText = statsInfo.active ? ' Online' : ' Offline';
            const statusClass = `client-status-badge inline-flex items-center gap-1 text-[10px] font-medium px-1.5 py-0.5 rounded-md ${statsInfo.active ? 'bg-emerald-50/70 dark:bg-emerald-950/40 text-emerald-700 dark:text-emerald-400 border border-emerald-200 dark:border-emerald-800' : 'bg-rose-50/70 dark:bg-rose-950/40 text-rose-700 dark:text-rose-400 border border-rose-200 dark:border-rose-800'}`;

            const updateRowStats = (row) => {
                if (!row) return;
                const badge = row.querySelector('.client-status-badge');
                if (badge) {
                    const dot = statsInfo.active ? 'bg-emerald-500' : 'bg-rose-500';
                    badge.innerHTML = '<span class="h-1.5 w-1.5 rounded-full ' + dot + '"></span>' + statusText;
                    badge.className = statusClass;
                }
                const readTotalEl = row.querySelector('.client-read-total');
                if (readTotalEl) setTextIfChanged(readTotalEl, formatBytes(statsInfo.bytes_read));

                const readSpeedEl = row.querySelector('.client-read-speed');
                if (readSpeedEl) setTextIfChanged(readSpeedEl, ` ${formatSpeed(speedInfo.readSpeed)}`);

                const writeTotalEl = row.querySelector('.client-write-total');
                if (writeTotalEl) setTextIfChanged(writeTotalEl, formatBytes(statsInfo.bytes_written));

                const writeSpeedEl = row.querySelector('.client-write-speed');
                if (writeSpeedEl) setTextIfChanged(writeSpeedEl, ` ${formatSpeed(speedInfo.writeSpeed)}`);

                const readGdEl = row.querySelector('.client-read-gd');
                if (readGdEl) setTextIfChanged(readGdEl, formatBytes(statsInfo.gamedisk_bytes_read || 0));
                const readIdEl = row.querySelector('.client-read-id');
                if (readIdEl) setTextIfChanged(readIdEl, formatBytes(statsInfo.imagedisk_bytes_read || 0));
                const writeGdEl = row.querySelector('.client-write-gd');
                if (writeGdEl) setTextIfChanged(writeGdEl, formatBytes(statsInfo.gamedisk_bytes_written || 0));
                const writeIdEl = row.querySelector('.client-write-id');
                if (writeIdEl) setTextIfChanged(writeIdEl, formatBytes(statsInfo.imagedisk_bytes_written || 0));
                const wbGdEl = row.querySelector('.client-wb-gamedisk');
                if (wbGdEl) setTextIfChanged(wbGdEl, (statsInfo.gamedisk_writeback_paths || []).join(', ') || '-');
                const wbIdEl = row.querySelector('.client-wb-imagedisk');
                if (wbIdEl) setTextIfChanged(wbIdEl, statsInfo.imagedisk_writeback_path || '-');

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
            pillEl.textContent = ' Enabled';
            pillEl.className = 'pill-status text-[11px] font-medium px-2 py-0.5 rounded-md bg-emerald-50 dark:bg-emerald-950/50 text-emerald-700 dark:text-emerald-400 border border-emerald-200 dark:border-emerald-800 whitespace-nowrap shrink-0';
        } else {
            pillEl.textContent = ' Disabled';
            pillEl.className = 'pill-status text-[11px] font-medium px-2 py-0.5 rounded-md bg-rose-50 dark:bg-rose-950/50 text-rose-700 dark:text-rose-400 border border-rose-200 dark:border-rose-800 whitespace-nowrap shrink-0';
        }
    }
}

// Client Listing for Dashboard (Purely Registered Clients from clients.toml)
function getMergedDashboardClients() {
    return (clientsObj && Array.isArray(clientsObj.client)) ? [...clientsObj.client] : [];
}

// Render Dashboard Clients Table

const CLIENT_COLUMN_DEFS = [
    { key: 'ip', label: 'IP Address', defaultOn: true },
    { key: 'read', label: 'Read I/O', defaultOn: true },
    { key: 'write', label: 'Write I/O', defaultOn: true },
    { key: 'wb_gamedisk', label: 'WB GameDisk', defaultOn: true },
    { key: 'wb_imagedisk', label: 'WB ImageDisk', defaultOn: true },
    { key: 'mac', label: 'MAC Address', defaultOn: true },
    { key: 'gateway', label: 'Gateway', defaultOn: true },
    { key: 'dns', label: 'DNS', defaultOn: true },
    { key: 'next_server', label: 'Next Server', defaultOn: true },
    { key: 'pxe', label: 'PXE', defaultOn: true },
    { key: 'image', label: 'Image', defaultOn: true },
    { key: 'driver', label: 'Driver', defaultOn: true },
    { key: 'cache', label: 'Cache', defaultOn: true },
    { key: 'uptime', label: 'Uptime', defaultOn: true }
];

function initClientPreferences() {
    clientColumns = {};
    CLIENT_COLUMN_DEFS.forEach(d => clientColumns[d.key] = d.defaultOn);
    try {
        const saved = JSON.parse(localStorage.getItem('simpleiscsi.clientColumns.v1') || '{}');
        CLIENT_COLUMN_DEFS.forEach(d => {
            if (typeof saved[d.key] === 'boolean') clientColumns[d.key] = saved[d.key];
        });
    } catch (_) { }
    renderClientColumnsMenu();
    applyClientColumns();
}

function saveClientColumns() {
    localStorage.setItem('simpleiscsi.clientColumns.v1', JSON.stringify(clientColumns));
}


function renderClientColumnsMenu() {
    const list = document.getElementById('client-columns-list');
    if (!list) return;
    list.innerHTML = CLIENT_COLUMN_DEFS.map(d =>
        `<label class="flex items-center gap-2 px-2 py-1 text-xs text-zinc-700 dark:text-zinc-300 cursor-pointer"><input type="checkbox" data-client-column="${d.key}" ${clientColumns[d.key] !== false ? 'checked' : ''} class="rounded border-zinc-300 dark:border-zinc-700"><span>${d.label}</span></label>`
    ).join('');
    list.querySelectorAll('[data-client-column]').forEach(cb => {
        cb.addEventListener('change', () => setClientColumn(cb.dataset.clientColumn, cb.checked));
    });
}

function toggleClientColumnsMenu() {
    const menu = document.getElementById('client-columns-menu');
    if (menu) menu.classList.toggle('hidden');
}

function setClientColumn(key, on) {
    clientColumns[key] = !!on;
    saveClientColumns();
    applyClientColumns();
    renderClientColumnsMenu();
}

function applyClientColumns() {
    CLIENT_COLUMN_DEFS.forEach(d => {
        const on = clientColumns[d.key] !== false;
        document.querySelectorAll('#clients-table [data-client-col="' + d.key + '"]').forEach(el => el.classList.toggle('hidden', !on));
        document.querySelectorAll('#clients-tbody .client-col-' + d.key).forEach(el => el.classList.toggle('hidden', !on));
    });
}







function dashboardClientCells(c, statsInfo) {
    const status = statsInfo.active
        ? '<span class="client-status-badge inline-flex items-center gap-1 text-[10px] font-medium px-1.5 py-0.5 rounded-md bg-emerald-50/70 dark:bg-emerald-950/40 text-emerald-700 dark:text-emerald-400 border border-emerald-200 dark:border-emerald-800"><span class="h-1.5 w-1.5 rounded-full bg-emerald-500"></span>Online</span>'
        : '<span class="client-status-badge inline-flex items-center gap-1 text-[10px] font-medium px-1.5 py-0.5 rounded-md bg-rose-50/70 dark:bg-rose-950/40 text-rose-700 dark:text-rose-400 border border-rose-200 dark:border-rose-800"><span class="h-1.5 w-1.5 rounded-full bg-rose-500"></span>Offline</span>';
    const isSuper = configObj && configObj.windows && configObj.windows.super_client_ip === c.ip;
    const superBadge = isSuper ? ' <span class="inline-flex items-center text-[10px] font-medium px-1.5 py-0.5 rounded-md bg-amber-50 dark:bg-amber-950/40 text-amber-700 dark:text-amber-400 border border-amber-200 dark:border-amber-800 ml-1">Super</span>' : '';

    return `<td class="py-2 px-2 whitespace-nowrap">${status}</td>`
        + `<td class="py-2 px-2 min-w-0 font-medium text-zinc-900 dark:text-zinc-100 break-words">${escapeHtml(c.hostname || c.ip)}${superBadge}</td>`
        + `<td class="py-2 px-2 min-w-0 font-mono text-zinc-700 dark:text-zinc-300 break-words">${escapeHtml(c.ip || '-')}</td>`
        + `<td class="py-2 px-2 min-w-0 font-mono text-zinc-700 dark:text-zinc-300 break-words">${escapeHtml(c.mac || '-')}</td>`;
}

function clientManagerCells(c, statsInfo, speedInfo) {
    const selected = selectedClientMacs.has(String(c.mac || '').toLowerCase());
    const status = statsInfo.active
        ? '<span class="client-status-badge inline-flex items-center gap-1 text-[10px] font-medium px-1.5 py-0.5 rounded-md bg-emerald-50/70 dark:bg-emerald-950/40 text-emerald-700 dark:text-emerald-400 border border-emerald-200 dark:border-emerald-800"><span class="h-1.5 w-1.5 rounded-full bg-emerald-500"></span>Online</span>'
        : '<span class="client-status-badge inline-flex items-center gap-1 text-[10px] font-medium px-1.5 py-0.5 rounded-md bg-rose-50/70 dark:bg-rose-950/40 text-rose-700 dark:text-rose-400 border border-rose-200 dark:border-rose-800"><span class="h-1.5 w-1.5 rounded-full bg-rose-500"></span>Offline</span>';
    const isSuper = configObj && configObj.windows && configObj.windows.super_client_ip === c.ip;
    const superBadge = isSuper ? ' <span class="inline-flex items-center text-[10px] font-medium px-1.5 py-0.5 rounded-md bg-amber-50 dark:bg-amber-950/40 text-amber-700 dark:text-amber-400 border border-amber-200 dark:border-amber-800 ml-1">Super</span>' : '';
    const driver = c.driver === 'ccboot' ? 'CCBoot' : 'Native';

    return `<td data-select-cell class="py-2 px-3 text-center"><input type="checkbox" data-client-select="${escapeHtml(c.mac)}" ${selected ? 'checked' : ''} aria-label="Pilih ${escapeHtml(c.hostname || c.ip)}" class="rounded border-zinc-300 dark:border-zinc-700"></td>`
        + `<td class="client-col-status py-2 px-3 whitespace-nowrap">${status}</td>`
        + `<td class="client-col-name py-2 px-3 whitespace-nowrap font-medium text-zinc-900 dark:text-zinc-100">${escapeHtml(c.hostname || c.ip)}${superBadge}</td>`
        + `<td class="client-col-ip py-2 px-3 whitespace-nowrap font-mono text-zinc-700 dark:text-zinc-300">${escapeHtml(c.ip || '-')}</td>`
        + '<td class="client-col-read py-2 px-3 whitespace-nowrap font-mono text-zinc-700 dark:text-zinc-300"><span class="client-read-total">' + formatBytes(statsInfo.bytes_read) + '</span> - <span class="client-read-speed">' + formatSpeed(speedInfo.readSpeed) + '</span><div class="text-[10px] text-zinc-400">GD <span class="client-read-gd">' + formatBytes(statsInfo.gamedisk_bytes_read || 0) + '</span> - ID <span class="client-read-id">' + formatBytes(statsInfo.imagedisk_bytes_read || 0) + '</span></div></td>'
        + '<td class="client-col-write py-2 px-3 whitespace-nowrap font-mono text-zinc-700 dark:text-zinc-300"><span class="client-write-total">' + formatBytes(statsInfo.bytes_written) + '</span> - <span class="client-write-speed">' + formatSpeed(speedInfo.writeSpeed) + '</span><div class="text-[10px] text-zinc-400">GD <span class="client-write-gd">' + formatBytes(statsInfo.gamedisk_bytes_written || 0) + '</span> - ID <span class="client-write-id">' + formatBytes(statsInfo.imagedisk_bytes_written || 0) + '</span></div></td>'
        + '<td class="client-col-wb_gamedisk py-2 px-3 whitespace-nowrap font-mono text-zinc-700 dark:text-zinc-300"><span class="client-wb-gamedisk">' + escapeHtml((statsInfo.gamedisk_writeback_paths || []).join(', ') || '-') + '</span></td>'
        + '<td class="client-col-wb_imagedisk py-2 px-3 whitespace-nowrap font-mono text-zinc-700 dark:text-zinc-300"><span class="client-wb-imagedisk">' + escapeHtml(statsInfo.imagedisk_writeback_path || '-') + '</span></td>'
        + `<td class="client-col-mac py-2 px-3 whitespace-nowrap font-mono text-zinc-700 dark:text-zinc-300">${escapeHtml(c.mac || '-')}</td>`
        + `<td class="client-col-gateway py-2 px-3 whitespace-nowrap font-mono text-zinc-700 dark:text-zinc-300">${escapeHtml(c.gateway || '-')}</td>`
        + `<td class="client-col-dns py-2 px-3 whitespace-nowrap font-mono text-zinc-700 dark:text-zinc-300">${escapeHtml(c.dns || '-')}</td>`
        + `<td class="client-col-next_server py-2 px-3 whitespace-nowrap font-mono text-zinc-700 dark:text-zinc-300">${escapeHtml(c.next_server || '-')}</td>`
        + `<td class="client-col-pxe py-2 px-3 whitespace-nowrap">${escapeHtml(c.pxe || 'Default')}</td>`
        + `<td class="client-col-image py-2 px-3 whitespace-nowrap font-mono">${escapeHtml(c.image_manager || 'Gamedisk')}</td>`
        + `<td class="client-col-driver py-2 px-3 whitespace-nowrap">${driver}</td>`
        + `<td class="client-col-cache py-2 px-3 whitespace-nowrap">${renderCacheBadge(c)}</td>`
        + `<td class="client-col-uptime py-2 px-3 whitespace-nowrap font-mono text-zinc-700 dark:text-zinc-300"><span class="client-uptime">${statsInfo.active ? formatDuration(statsInfo.uptime_secs) : 'Offline'}</span></td>`;
}

function getClientByMac(mac) {
    const key = String(mac || '').toLowerCase();
    return (clientsObj.client || []).find(c => String(c.mac || '').toLowerCase() === key);
}

function toggleClientSelection(mac, force) {
    const key = String(mac || '').toLowerCase();
    if (!key) return;
    const shouldSelect = typeof force === 'boolean' ? force : !selectedClientMacs.has(key);
    if (shouldSelect) selectedClientMacs.add(key); else selectedClientMacs.delete(key);
    updateClientSelectionUI();
}

function clearClientSelection() {
    selectedClientMacs.clear();
    updateClientSelectionUI();
}

function toggleSelectAllClients(checked) {
    selectedClientMacs.clear();
    if (checked) (clientsObj.client || []).forEach(c => selectedClientMacs.add(String(c.mac || '').toLowerCase()));
    updateClientSelectionUI();
}

function updateClientSelectionUI() {
    const all = clientsObj.client || [];
    const count = selectedClientMacs.size;
    const countEl = document.getElementById('client-selection-count');
    if (countEl) countEl.textContent = count + ' klien dipilih';
    ['btn-client-bulk-edit', 'btn-client-bulk-delete', 'btn-client-bulk-clear'].forEach(id => {
        const btn = document.getElementById(id);
        if (btn) btn.disabled = count === 0;
    });
    const header = document.getElementById('clients-select-all');
    if (header) {
        const selectedAll = all.length > 0 && all.every(c => selectedClientMacs.has(String(c.mac || '').toLowerCase()));
        header.checked = selectedAll;
        header.indeterminate = count > 0 && !selectedAll;
    }
    document.querySelectorAll('#clients-tbody tr[data-mac]').forEach(row => {
        const on = selectedClientMacs.has(String(row.dataset.mac || '').toLowerCase());
        const cb = row.querySelector('[data-client-select]');
        if (cb) cb.checked = on;
        row.classList.toggle('bg-zinc-50', on);
        row.classList.toggle('dark:bg-zinc-900/40', on);
    });
}

function initClientTableEvents() {
    const tbody = document.getElementById('clients-tbody');
    if (!tbody) return;
    tbody.addEventListener('click', (e) => {
        const cb = e.target.closest('[data-client-select]');
        if (cb) {
            e.stopPropagation();
            toggleClientSelection(cb.dataset.clientSelect, cb.checked);
            return;
        }
        if (Date.now() < clientDrag.suppressClickUntil) return;
        const row = e.target.closest('tr[data-mac]');
        if (!row) return;
        if (e.ctrlKey) {
            e.preventDefault();
            toggleClientSelection(row.dataset.mac);
            return;
        }
        const c = getClientByMac(row.dataset.mac);
        if (c) openClientCrudModal(c);
    });
    tbody.addEventListener('mousedown', (e) => {
        if (e.target.closest('input')) return;
        const cell = e.target.closest('[data-select-cell]');
        if (!cell) return;
        const row = cell.closest('tr[data-mac]');
        if (!row) return;
        e.preventDefault();
        clientDrag.active = true;
        clientDrag.moved = false;
        clientDrag.mode = !selectedClientMacs.has(String(row.dataset.mac || '').toLowerCase());
        clientDrag.suppressClickUntil = Date.now() + 250;
        toggleClientSelection(row.dataset.mac, clientDrag.mode);
    });
    tbody.addEventListener('mouseover', (e) => {
        if (!clientDrag.active) return;
        const row = e.target.closest('tr[data-mac]');
        if (!row) return;
        clientDrag.moved = true;
        toggleClientSelection(row.dataset.mac, clientDrag.mode);
    });
    tbody.addEventListener('contextmenu', (e) => {
        const row = e.target.closest('tr[data-mac]');
        if (!row) return;
        e.preventDefault();
        const c = getClientByMac(row.dataset.mac);
        if (!c) return;
        const s = activeSessionsMap.get(c.ip) || { active: false };
        showContextMenu(e, { ip: c.ip, active: s.active, image_manager: c.image_manager, clientObj: c });
    });
    document.addEventListener('mouseup', () => {
        if (clientDrag.active) {
            clientDrag.active = false;
            clientDrag.suppressClickUntil = Date.now() + 250;
        }
    });
}

function renderDashboardClientsTable() {
    const tbody = document.getElementById('dashboard-clients-tbody');
    if (!tbody) return;
    const all = getMergedDashboardClients();
    renderedDashboardIps = all.map(c => c.ip);
    if (all.length === 0) {
        tbody.innerHTML = '<tr><td colspan="4" class="py-8 px-5 text-center text-zinc-500 dark:text-zinc-400 text-xs">Belum ada klien terdaftar.</td></tr>';
        const totalPcsEl = document.getElementById('stat-total-pcs');
        if (totalPcsEl) totalPcsEl.textContent = 0;
        return;
    }
    const limit = window.innerWidth >= 1536 ? 8 : window.innerWidth >= 1280 ? 6 : 5;
    tbody.innerHTML = '';
    all.slice(0, limit).forEach(c => {
        const statsInfo = activeSessionsMap.get(c.ip) || { active: false, bytes_read: 0, bytes_written: 0, uptime_secs: 0 };
        const row = document.createElement('tr');
        row.dataset.ip = c.ip;
        row.className = 'hover:bg-zinc-50/80 dark:hover:bg-zinc-900/50 border-b border-zinc-100 dark:border-zinc-800/80';
        row.innerHTML = dashboardClientCells(c, statsInfo);
        row.addEventListener('contextmenu', (e) => {
            e.preventDefault();
            showContextMenu(e, { ip: c.ip, active: statsInfo.active, image_manager: c.image_manager, clientObj: c });
        });
        tbody.appendChild(row);
    });
    const totalPcsEl = document.getElementById('stat-total-pcs');
    if (totalPcsEl) totalPcsEl.textContent = all.length;
}

function renderClientsManagerTable() {
    const tbody = document.getElementById('clients-tbody');
    if (!tbody) return;
    const all = clientsObj.client || [];
    const macs = new Set(all.map(c => String(c.mac || '').toLowerCase()));
    Array.from(selectedClientMacs).forEach(m => { if (!macs.has(m)) selectedClientMacs.delete(m); });

    if (all.length === 0) {
        tbody.innerHTML = '<tr><td colspan="15" class="py-8 px-5 text-center text-zinc-500 dark:text-zinc-400 text-xs">Belum ada klien. Klik Tambah Klien untuk memulai.</td></tr>';
        updateClientSelectionUI();
        return;
    }

    tbody.innerHTML = '';
    all.forEach(c => {
        const statsInfo = activeSessionsMap.get(c.ip) || { active: false, bytes_read: 0, bytes_written: 0, uptime_secs: 0 };
        const speedInfo = clientSpeedHistory.get(c.ip) || { readSpeed: 0, writeSpeed: 0 };
        const row = document.createElement('tr');
        row.dataset.mac = String(c.mac || '').toLowerCase();
        row.dataset.ip = c.ip;
        row.className = 'hover:bg-zinc-50/80 dark:hover:bg-zinc-900/50 border-b border-zinc-100 dark:border-zinc-800/80 cursor-pointer';
        row.innerHTML = clientManagerCells(c, statsInfo, speedInfo);
        tbody.appendChild(row);
    });
    applyClientColumns();
    updateClientSelectionUI();
}

const BULK_FIELDS = [
    { key: 'gateway', label: 'Gateway', type: 'text', placeholder: '192.168.1.1' },
    { key: 'dns', label: 'DNS', type: 'text', placeholder: '1.1.1.1' },
    { key: 'next_server', label: 'Next Server', type: 'text', placeholder: '192.168.1.2' },
    { key: 'pxe', label: 'PXE Folder', type: 'text', placeholder: 'sb-custom' },
    { key: 'image_manager', label: 'Boot Image', type: 'select' },
    { key: 'bootfile_uefi', label: 'Bootfile UEFI', type: 'text', placeholder: 'ipxe.efi' },
    { key: 'bootfile_legacy', label: 'Bootfile Legacy', type: 'text', placeholder: 'undionly.kpxe' },
    { key: 'bootfile_ipxe', label: 'Bootfile iPXE', type: 'text', placeholder: 'ipxe.efi' },
    { key: 'driver', label: 'Driver', type: 'select', options: [['native', 'Native'], ['ccboot', 'CCBoot']], clearable: false },
    { key: 'cache_enabled', label: 'Cache CCacheX', type: 'select', options: [['false', 'Nonaktif'], ['true', 'Aktif']], clearable: false },
    { key: 'cache_size_mb', label: 'Cache Size (MB)', type: 'number', min: 0, max: 8192, step: 16 }
];

function bulkInputHtml(f) {
    if (f.key === 'image_manager') {
        const opts = [['', 'None (Gamedisk)']].concat(Object.keys((configObj && configObj.image_manager) || {}).map(k => [k, k]));
        return '<select data-bulk-input class="ui-input w-full px-2.5 py-1.5 text-xs rounded-md border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-950 text-zinc-900 dark:text-zinc-100" disabled>' + opts.map(o => '<option value="' + escapeHtml(o[0]) + '">' + escapeHtml(o[1]) + '</option>').join('') + '</select>';
    }
    if (f.type === 'select') {
        return '<select data-bulk-input class="ui-input w-full px-2.5 py-1.5 text-xs rounded-md border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-950 text-zinc-900 dark:text-zinc-100" disabled>' + f.options.map(o => '<option value="' + o[0] + '">' + o[1] + '</option>').join('') + '</select>';
    }
    const min = f.min !== undefined ? ' min="' + f.min + '"' : '';
    const max = f.max !== undefined ? ' max="' + f.max + '"' : '';
    const step = f.step !== undefined ? ' step="' + f.step + '"' : '';
    return '<input data-bulk-input type="' + f.type + '" placeholder="' + (f.placeholder || '') + '"' + min + max + step + ' class="ui-input w-full px-2.5 py-1.5 text-xs rounded-md border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-950 text-zinc-900 dark:text-zinc-100" disabled>';
}

function openClientBulkEditModal() {
    if (selectedClientMacs.size === 0) return;
    const selected = (clientsObj.client || []).filter(c => selectedClientMacs.has(String(c.mac || '').toLowerCase()));
    const names = selected.slice(0, 5).map(c => c.hostname || c.ip).join(', ');
    const summary = document.getElementById('client-bulk-summary');
    if (summary) summary.textContent = selected.length + ' klien dipilih: ' + names + (selected.length > 5 ? ', +' + (selected.length - 5) + ' lainnya' : '');
    const fields = document.getElementById('client-bulk-fields');
    if (!fields) return;
    fields.innerHTML = BULK_FIELDS.map(f =>
        '<div class="grid grid-cols-1 xl:grid-cols-[170px_1fr_95px] gap-2 items-center py-2 border-b border-zinc-100 dark:border-zinc-800" data-bulk-key="' + f.key + '">'
        + '<label class="flex items-center gap-2 text-xs font-medium text-zinc-700 dark:text-zinc-300"><input type="checkbox" data-bulk-toggle class="rounded border-zinc-300 dark:border-zinc-700">' + f.label + '</label>'
        + '<div>' + bulkInputHtml(f) + '</div>'
        + (f.clearable === false ? '' : '<label class="flex items-center gap-1 text-[11px] text-zinc-500 dark:text-zinc-400"><input type="checkbox" data-bulk-clear class="rounded border-zinc-300 dark:border-zinc-700" disabled>Kosongkan</label>')
        + '</div>'
    ).join('');
    fields.querySelectorAll('[data-bulk-key]').forEach(row => {        const toggle = row.querySelector('[data-bulk-toggle]');
        const input = row.querySelector('[data-bulk-input]');
        const clear = row.querySelector('[data-bulk-clear]');
        toggle.addEventListener('change', () => {
            input.disabled = !toggle.checked;
            if (clear) {
                clear.disabled = !toggle.checked;
                if (!toggle.checked) clear.checked = false;
            }
        });
        if (clear) clear.addEventListener('change', () => {
            input.disabled = clear.checked || !toggle.checked;
        });
    });
    attachPxeCombo(fields.querySelector('[data-bulk-key="pxe"] input[data-bulk-input]'));
    const modal = document.getElementById('client-bulk-edit-modal');
    if (modal) modal.style.display = 'flex';
}

function closeClientBulkEditModal() {
    closePxeCombo();
    const modal = document.getElementById('client-bulk-edit-modal');
    if (modal) modal.style.display = 'none';
}

function isValidIPv4(value) {
    return /^(\d{1,3}\.){3}\d{1,3}$/.test(value) && value.split('.').every(n => Number(n) >= 0 && Number(n) <= 255);
}

async function saveClientBulkEdit(e) {
    e.preventDefault();
    const updates = {};
    let validationError = '';
    document.querySelectorAll('#client-bulk-fields [data-bulk-key]').forEach(row => {
        const key = row.dataset.bulkKey;
        const toggle = row.querySelector('[data-bulk-toggle]');
        if (!toggle.checked) return;
        const input = row.querySelector('[data-bulk-input]');
        const clear = row.querySelector('[data-bulk-clear]');
        let value = (clear && clear.checked) ? '' : input.value.trim();
        if (key === 'cache_size_mb') value = parseInt(value || '0', 10) || 0;
        if (key === 'cache_enabled') value = value === 'true';
        updates[key] = value;
    });

    if (Object.keys(updates).length === 0) {
        showToast('Pilih minimal satu field untuk diubah.', 'warning', 'Tidak ada perubahan');
        return;
    }

    ['gateway', 'dns', 'next_server'].forEach(key => {
        if (updates[key] && !isValidIPv4(updates[key])) validationError = 'Format IP ' + key + ' tidak valid';
    });
    if (updates.driver === 'native') {
        updates.cache_enabled = false;
        updates.cache_size_mb = 0;
    }
    if (updates.cache_enabled === false) updates.cache_size_mb = 0;
    if (updates.cache_enabled === true && updates.cache_size_mb === undefined) updates.cache_size_mb = 0;
    if (updates.cache_enabled === true && !(updates.cache_size_mb === 0 || (updates.cache_size_mb >= 32 && updates.cache_size_mb <= 8192 && updates.cache_size_mb % 16 === 0))) {
        validationError = 'Ukuran cache harus 0 atau 32-8192 MB, kelipatan 16';
    }
    if (validationError) {
        showToast(validationError, 'error', 'Edit massal gagal');
        return;
    }

    let touched = 0;
    const original = clientsObj.client || [];
    const updated = original.map(c => {
        const key = String(c.mac || '').toLowerCase();
        if (!selectedClientMacs.has(key)) return c;
        const next = { ...c };
        Object.keys(updates).forEach(k => { next[k] = updates[k]; });
        if (next.driver === 'native') { next.cache_enabled = false; next.cache_size_mb = 0; }
        touched++;
        return next;
    });
    clientsObj.client = updated;
    const saved = await saveClientsJson();
    if (!saved) {
        clientsObj.client = original;
        showToast('Gagal menyimpan perubahan massal. Periksa data atau server.', 'error', 'Edit massal gagal');
        return;
    }
    closeClientBulkEditModal();
    clearClientSelection();
    renderClientsManagerTable();
    renderDashboardClientsTable();
    showToast(touched + ' klien diperbarui', 'success', 'Edit massal selesai');
}

function deleteSelectedClients() {
    const count = selectedClientMacs.size;
    if (!count) return;
    const selected = (clientsObj.client || []).filter(c => selectedClientMacs.has(String(c.mac || '').toLowerCase()));

    // Strict Block: Periksa apakah ada klien terpilih yang sedang ONLINE
    const onlineClients = selected.filter(c => {
        const s = activeSessionsMap.get(c.ip);
        return s && s.active;
    });

    if (onlineClients.length > 0) {
        const names = onlineClients.map(c => c.hostname || c.ip).join(', ');
        showToast(`Tidak dapat menghapus: Terdapat ${onlineClients.length} PC sedang ONLINE (${names}). Matikan PC terlebih dahulu.`, 'warning', 'Penghapusan Ditolak');
        return;
    }

    const names = selected.slice(0, 3).map(c => c.hostname || c.ip).join(', ');
    showConfirmModal('Hapus ' + count + ' klien?', 'Klien berikut akan dihapus permanen: ' + names + (count > 3 ? ', +' + (count - 3) + ' lainnya' : ''), async () => {
        const original = clientsObj.client || [];
        clientsObj.client = original.filter(c => !selectedClientMacs.has(String(c.mac || '').toLowerCase()));
        const saved = await saveClientsJson();
        if (!saved) {
            clientsObj.client = original;
            return;
        }
        clearClientSelection();
        renderClientsManagerTable();
        renderDashboardClientsTable();
        showToast(count + ' klien dihapus', 'success', 'Klien dihapus');
    });
}

function openListModal(title, subtitle, html) {
    const modal = document.getElementById('list-modal');
    if (!modal) return;
    document.getElementById('list-modal-title').textContent = title;
    document.getElementById('list-modal-subtitle').textContent = subtitle || '';
    document.getElementById('list-modal-body').innerHTML = html;
    modal.style.display = 'flex';
}

function closeListModal() {
    const modal = document.getElementById('list-modal');
    if (modal) modal.style.display = 'none';
}

function simpleListTable(headers, rows) {
    return '<div class="overflow-x-hidden rounded-md border border-zinc-200 dark:border-zinc-800"><table class="w-full table-fixed text-left text-xs"><thead class="bg-zinc-50 dark:bg-zinc-900/90 border-b border-zinc-200 dark:border-zinc-800"><tr>'
        + headers.map(h => '<th class="py-2 px-3 text-[10px] font-medium uppercase tracking-wider text-zinc-500 dark:text-zinc-400">' + h + '</th>').join('')
        + '</tr></thead><tbody class="divide-y divide-zinc-100 dark:divide-zinc-800/80">'
        + (rows.length ? rows.join('') : '<tr><td colspan="' + headers.length + '" class="py-8 px-4 text-center text-zinc-500 dark:text-zinc-400">Tidak ada data.</td></tr>')
        + '</tbody></table></div>';
}

function openDashboardClientsList() {
    const all = getMergedDashboardClients();
    const rows = all.map(c => {
        const s = activeSessionsMap.get(c.ip) || { active: false };
        return '<tr><td class="py-2 px-3 min-w-0"><div class="font-medium text-zinc-900 dark:text-zinc-100 break-words">' + escapeHtml(c.hostname || c.ip) + '</div><div class="text-[10px] font-mono text-zinc-500">' + escapeHtml(c.ip) + '</div></td>'
            + '<td class="py-2 px-3 text-[11px] font-mono text-zinc-500 break-words">' + escapeHtml(c.image_manager || 'Gamedisk') + '</td>'
            + '<td class="py-2 px-3 text-[11px]">' + (s.active ? 'Online' : 'Offline') + '</td></tr>';
    });
    openListModal('Semua Klien', all.length + ' klien terdaftar', simpleListTable(['Klien', 'Image', 'Status'], rows));
}

function openVhdListModal() {
    const entries = Object.entries((configObj && configObj.image_manager) || {});
    const rows = entries.map(([key, path]) => {
        const safeKey = String(key).replace(/'/g, "\\'");
        return '<tr><td class="py-2 px-3 min-w-0"><div class="font-mono text-xs font-semibold text-zinc-900 dark:text-zinc-100 break-words">' + escapeHtml(key) + '</div><div class="text-[10px] font-mono text-zinc-500 break-words">' + escapeHtml(path) + '</div></td>'
            + '<td class="py-2 px-3 text-right"><button class="btn-secondary px-2 py-1 text-[11px] cursor-pointer" onclick="openVhdCrudModal(\'' + safeKey + '\')">Edit</button> <button class="btn-primary px-2 py-1 text-[11px] cursor-pointer" onclick="showVhdSnapshots(\'' + safeKey + '\')">Snapshot</button> <button class="text-rose-600 dark:text-rose-400 hover:text-rose-700 px-1.5 py-1 text-[11px] font-medium cursor-pointer" onclick="deleteVhdMapping(\'' + safeKey + '\')">Hapus</button></td></tr>';
    });
    openListModal('Semua VHD Mapping', entries.length + ' mapping', simpleListTable(['Image & Path', 'Aksi'], rows));
}

function openCacheListModal() {
    const rows = (currentCacheDisks || []).map(d => '<tr><td class="py-2 px-3 min-w-0"><div class="font-medium text-zinc-900 dark:text-zinc-100 break-words">' + escapeHtml(d.name || '-') + '</div><div class="text-[10px] font-mono text-zinc-500 break-words">' + escapeHtml(d.disk_id || '') + '</div></td>'
        + '<td class="py-2 px-3 text-[11px] font-mono">' + (d.ram_cache_mb || 0) + ' MB</td><td class="py-2 px-3 text-[11px] font-mono">' + (d.used_cache_mb || 0) + ' MB</td><td class="py-2 px-3 text-[11px] font-mono">' + Number(d.hit_rate_pct || 0).toFixed(1) + '%</td>'
        + '<td class="py-2 px-3 text-right"><button type="button" data-cache-detail="' + escapeHtml(d.disk_id) + '" class="btn-secondary px-2 py-1 text-[11px] cursor-pointer">Detail</button></td></tr>');
    openListModal('Semua Cache Disk', (currentCacheDisks || []).length + ' pool', simpleListTable(['Disk', 'Alokasi', 'Terpakai', 'Hit Rate', 'Aksi'], rows));
    document.querySelectorAll('#list-modal-body [data-cache-detail]').forEach(btn => {
        btn.addEventListener('click', () => { closeListModal(); openCacheDetailModal(btn.dataset.cacheDetail); });
    });
}


function openTftpListModal() {
    const rows = (tftpFoldersCache || []).map(f => '<tr><td class="py-2 px-3 font-mono text-xs text-zinc-900 dark:text-zinc-100 break-words">' + escapeHtml(f) + '</td><td class="py-2 px-3 text-right"><button class="text-rose-600 dark:text-rose-400 text-[11px] font-medium cursor-pointer" onclick="deleteTftpFolderAction(\'' + String(f).replace(/'/g, "\\'") + '\')">Hapus</button></td></tr>');
    openListModal('Semua Folder TFTP', (tftpFoldersCache || []).length + ' folder', simpleListTable(['Nama Folder', 'Aksi'], rows));
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
        setSelectValueWithFallback(document.getElementById('client-next-server'), client.next_server || '');
        document.getElementById('client-pxe').value = client.pxe || '';
        document.getElementById('client-image-manager').value = client.image_manager || '';
        document.getElementById('client-driver').value = client.driver || 'native';
        document.getElementById('client-cache-enabled').checked = !!client.cache_enabled;
        document.getElementById('client-cache-size-mb').value = client.cache_size_mb || 0;
        onClientDriverChange();
        const delBtn = document.getElementById('btn-client-delete');
        delBtn.style.display = 'inline-flex';
        const statsInfo = activeSessionsMap.get(client.ip);
        const isOnline = statsInfo && statsInfo.active;
        if (isOnline) {
            delBtn.disabled = true;
            delBtn.classList.add('opacity-40', 'cursor-not-allowed', 'pointer-events-none');
            delBtn.title = 'Klien sedang ONLINE (tidak dapat dihapus)';
        } else {
            delBtn.disabled = false;
            delBtn.classList.remove('opacity-40', 'cursor-not-allowed', 'pointer-events-none');
            delBtn.title = 'Hapus data klien ini';
        }
    } else {
        document.getElementById('client-modal-title').textContent = 'Tambah Klien Baru';
        document.getElementById('client-old-mac').value = '';
        document.getElementById('client-crud-form').reset();
        document.getElementById('client-driver').value = 'native';
        document.getElementById('client-cache-enabled').checked = false;
        document.getElementById('client-cache-size-mb').value = '0';
        onClientDriverChange();
        const delBtn = document.getElementById('btn-client-delete');
        delBtn.style.display = 'none';
        delBtn.disabled = false;
        delBtn.classList.remove('opacity-40', 'cursor-not-allowed', 'pointer-events-none');
    }
}

function closeClientCrudModal() {
    closePxeCombo();
    document.getElementById('client-crud-modal').style.display = 'none';
}

async function populateClientImageDropdown() {
    const select = document.getElementById('client-image-manager');
    if (!select) return;
    select.innerHTML = '';
    select.add(new Option('None (Gamedisk Only)', ''));

    if (configObj && configObj.image_manager) {
        Object.keys(configObj.image_manager).forEach(alias => {
            select.add(new Option(` ${alias}`, alias));
        });
    }

    const vhdFiles = await apiGet('/api/system/vhds');
    if (vhdFiles && Array.isArray(vhdFiles)) {
        vhdFiles.forEach(file => {
            if (!configObj || !configObj.image_manager || !configObj.image_manager[file]) {
                select.add(new Option(` ${file}`, file));
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

    const driver = document.getElementById('client-driver').value;
    let cacheEnabled = document.getElementById('client-cache-enabled').checked;
    let cacheSize = parseInt(document.getElementById('client-cache-size-mb').value || '0', 10);
    if (Number.isNaN(cacheSize)) cacheSize = 0;

    if (clientData.hostname.includes('(') || clientData.hostname.includes(')')) {
        showToast('Hostname tidak boleh mengandung karakter kurung', 'error');
        return;
    }
    if (driver !== 'ccboot') {
        cacheEnabled = false;
        cacheSize = 0;
    }
    if (!document.getElementById('global-ccachex-enabled')?.checked) {
        cacheEnabled = false;
        cacheSize = 0;
    }
    if (cacheEnabled && !(cacheSize === 0 || (cacheSize >= 32 && cacheSize <= 8192 && cacheSize % 16 === 0))) {
        showToast('Ukuran cache harus 0 atau 32-8192 MB, kelipatan 16 MB', 'error');
        return;
    }
    clientData.driver = driver;
    clientData.cache_enabled = cacheEnabled;
    clientData.cache_size_mb = cacheSize;

    const existing = oldMac ? (clientsObj.client || []).find(c => c.mac.toLowerCase() === oldMac.toLowerCase()) : null;
    clientData.bootfile_uefi = existing ? (existing.bootfile_uefi || '') : '';
    clientData.bootfile_legacy = existing ? (existing.bootfile_legacy || '') : '';
    clientData.bootfile_ipxe = existing ? (existing.bootfile_ipxe || '') : '';

    if (!clientsObj.client) clientsObj.client = [];

    if (oldMac) {
        const idx = clientsObj.client.findIndex(c => c.mac.toLowerCase() === oldMac.toLowerCase());
        if (idx !== -1) clientsObj.client[idx] = clientData;
        else clientsObj.client.push(clientData);
    } else {
        clientsObj.client.push(clientData);
    }

    const saved = await saveClientsJson();
    if (!saved) {
        showToast('Gagal menyimpan data klien. Periksa ukuran cache / hostname.', 'error', 'Gagal menyimpan klien');
        return;
    }
    closeClientCrudModal();
    renderClientsManagerTable();
    renderDashboardClientsTable();
    showToast('Data klien berhasil disimpan.', 'success', 'Klien disimpan');
}

async function deleteClientAction() {
    const oldMac = document.getElementById('client-old-mac').value.trim();
    if (!oldMac) return;

    const targetClient = (clientsObj.client || []).find(c => c.mac.toLowerCase() === oldMac.toLowerCase());
    if (targetClient) {
        const statsInfo = activeSessionsMap.get(targetClient.ip);
        if (statsInfo && statsInfo.active) {
            showToast(`Klien '${targetClient.hostname || targetClient.ip}' sedang ONLINE dan tidak dapat dihapus. Matikan PC terlebih dahulu.`, 'warning', 'Aksi Ditolak');
            return;
        }
    }

    const clientName = targetClient ? (targetClient.hostname || targetClient.ip) : oldMac;
    showConfirmModal('Hapus Klien', `Apakah Anda yakin ingin menghapus data klien '${clientName}' (${oldMac})?`, async () => {
        const original = clientsObj.client || [];
        clientsObj.client = original.filter(c => c.mac.toLowerCase() !== oldMac.toLowerCase());
        const saved = await saveClientsJson();
        if (!saved) {
            clientsObj.client = original;
            return;
        }
        closeClientCrudModal();
        renderClientsManagerTable();
        renderDashboardClientsTable();
        showToast('Klien berhasil dihapus', 'success', 'Klien dihapus');
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
    if (!res || res.status !== 'ok') {
        const msg = (res && res.message) ? res.message : 'Gagal menyimpan konfigurasi klien';
        showToast(msg, 'error', 'Penyimpanan Gagal');
        return false;
    }
    return true;
}

// VHD Manager CRUD Handlers
function renderVhdTable() {
    const tbody = document.getElementById('vhds-tbody');
    if (!configObj || !configObj.image_manager || Object.keys(configObj.image_manager).length === 0) {
        tbody.innerHTML = `<tr><td colspan="3" class="py-12 px-6 text-center">
            <div class="flex flex-col items-center justify-center text-center gap-2">
                <h3 class="font-semibold text-sm text-zinc-900 dark:text-zinc-100 tracking-tight">Belum Ada VHD</h3>
                <p class="text-zinc-500 dark:text-zinc-400 text-xs max-w-sm">Daftarkan file VHD Windows yang akan di-boot oleh klien Anda.</p>
            </div>
        </td></tr>`;
        return;
    }

    tbody.innerHTML = '';
    Object.entries(configObj.image_manager).slice(0, 5).forEach(([key, path]) => {
        const row = document.createElement('tr');
        row.className = "hover:bg-zinc-50/80 dark:hover:bg-zinc-900/50 transition-colors border-b border-zinc-100 dark:border-zinc-800/80";
        const safeKey = key.replace(/'/g, "\\'");
        row.innerHTML = `
            <td class="py-2.5 px-3 sm:py-3 sm:px-3.5 min-w-0">
                <div class="flex items-center gap-2">

                    <span class="font-mono text-xs sm:text-sm font-semibold text-zinc-900 dark:text-zinc-100">${key}</span>
                </div>
                <div class="text-[11px] font-mono text-zinc-500 dark:text-zinc-400 max-w-full mt-0.5 break-words" title="${path}">${path}</div>
            </td>
            <td class="py-2.5 px-3 sm:py-3 sm:px-3.5 min-w-0">
                <div class="text-xs font-medium text-zinc-800 dark:text-zinc-200" id="snapshots-count-${key}">Loading...</div>
                <div class="text-[11px] text-zinc-400 dark:text-zinc-500 mt-0.5">Auto Snapshot Ready</div>
            </td>
            <td class="py-2.5 px-2.5 sm:py-3 sm:px-3.5 text-right">
                <div class="inline-flex items-center gap-1.5 justify-end">
                    <button class="btn-secondary inline-flex items-center justify-center px-2 sm:px-2.5 py-1 text-[11px] xl:text-xs font-medium cursor-pointer" onclick="openVhdCrudModal('${safeKey}')">Edit</button>
                    <button class="btn-primary inline-flex items-center justify-center px-2 sm:px-2.5 py-1 text-[11px] xl:text-xs font-medium shadow-2xs cursor-pointer" onclick="showVhdSnapshots('${safeKey}')">Snapshots</button>
                    <button class="text-rose-600 dark:text-rose-400 hover:text-rose-700 px-1.5 py-1 text-[11px] xl:text-xs font-medium cursor-pointer" onclick="deleteVhdMapping('${safeKey}')">Hapus</button>
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
    showToast('Pilih file di dialog Windows yang terbuka', 'info');
    const res = await apiPost('/api/system/select_vhd', {}, 0);
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

function deleteVhdAction() {
    const oldKey = document.getElementById('vhd-old-key').value.trim();
    if (oldKey) deleteVhdMapping(oldKey);
}

function deleteVhdMapping(key) {
    if (!configObj || !configObj.image_manager || configObj.image_manager[key] === undefined) return;

    showConfirmModal('Hapus VHD Mapping', `Yakin ingin menghapus alias mapping ${key}?`, async () => {
        delete configObj.image_manager[key];
        await saveConfigJsonFull();
        closeVhdCrudModal();
        renderVhdTable();
        const listModal = document.getElementById('list-modal');
        if (listModal && listModal.style.display === 'flex') openVhdListModal();
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

                    <span>${displayName}</span>
                    <span class="text-[10px] text-zinc-400 dark:text-zinc-500 font-normal">#${snap.index}</span>
                </div>
                <div class="text-[11px] font-mono text-zinc-500 dark:text-zinc-400 max-w-xs mt-0.5 break-words" title="${snap.path}">${snap.path}</div>
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

function formatDriveDisplay(path) {
    if (!path) return '-';
    const match = path.match(/([a-zA-Z]):/);
    if (match) {
        return `Drive ${match[1].toUpperCase()}`;
    }
    if (window._systemDrivesMap) {
        const norm = (path || '').replace(/\\+/g, '\\').toLowerCase().trim();
        for (const [letter, d] of Object.entries(window._systemDrivesMap)) {
            if (d && d.physical_disk && d.physical_disk.replace(/\\+/g, '\\').toLowerCase().trim() === norm) {
                return `Drive ${letter.toUpperCase()}`;
            }
        }
    }
    const physMatch = path.match(/PhysicalDrive(\d+)/i);
    if (physMatch) {
        return `Drive Disk ${physMatch[1]}`;
    }
    return path;
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
        if (letter === 'C') return; // Eksklusi mutlak Drive C:
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
                if (!gd) return false;
                if (gd.volume) return String(gd.volume).trim().toUpperCase() === letter + ':';
                if (!gd.physical_disk) return false;
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
                icon: '',
                badgeClass: 'bg-zinc-100 text-zinc-900 border-zinc-300 dark:bg-zinc-800 dark:text-zinc-100 dark:border-zinc-700',
                cardBorder: 'border-zinc-300 dark:border-zinc-700',
                desc: 'Master OS VHD',
                colorAccent: 'text-zinc-900 dark:text-zinc-100'
            },
            writeback: {
                label: 'WRITEBACK',
                icon: '',
                badgeClass: 'bg-amber-50 text-amber-700 border-amber-200 dark:bg-amber-950/50 dark:text-amber-400 dark:border-amber-800',
                cardBorder: 'border-zinc-200 dark:border-zinc-800',
                desc: 'Client Cache I/O',
                colorAccent: 'text-amber-600 dark:text-amber-400'
            },
            gamedisk: {
                label: 'GAMEDISK',
                icon: '',
                badgeClass: 'bg-emerald-50 text-emerald-700 border-emerald-200 dark:bg-emerald-950/50 dark:text-emerald-400 dark:border-emerald-800',
                cardBorder: 'border-zinc-200 dark:border-zinc-800',
                desc: 'Game Storage Target',
                colorAccent: 'text-emerald-600 dark:text-emerald-400'
            },
            none: {
                label: 'UNASSIGNED',
                icon: '',
                badgeClass: 'bg-zinc-100 text-zinc-500 border-zinc-200 dark:bg-zinc-800 dark:text-zinc-400 dark:border-zinc-700',
                cardBorder: 'border-zinc-200 dark:border-zinc-800',
                desc: 'Belum dialokasikan',
                colorAccent: 'text-zinc-500 dark:text-zinc-400'
            }
        }[currentRole] || {
            label: 'UNASSIGNED',
            icon: '',
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
                        <div class="min-w-0 flex-1">
                            <h3 class="font-semibold text-sm sm:text-base text-zinc-900 dark:text-zinc-100 leading-tight">Drive ${drive.letter}</h3>
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
    if (letter.toUpperCase() === 'C') {
        showToast('Drive C: adalah drive sistem host dan dilarang untuk iSCSI', 'error');
        return;
    }
    const drive = (window._systemDrivesMap && window._systemDrivesMap[letter]) || { letter, physical_disk: '' };
    const physicalDisk = drive.physical_disk || '';
    const modal = document.getElementById('partition-modal');
    modal.style.display = 'flex';
    document.getElementById('partition-modal-desc').textContent = `Pilih fungsi atau lepas alokasi untuk Drive ${letter}.`;
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
    if (letter === 'C') {
        showToast('Drive C: adalah drive sistem host dan dilarang untuk iSCSI', 'error');
        return;
    }
    const drive = (window._systemDrivesMap && window._systemDrivesMap[letter]) || {};
    const physicalDisk = drive.physical_disk || document.getElementById('partition-mount-point').dataset.physicalDisk || '';
    const selectedRadio = document.querySelector('input[name="partition-role"]:checked');
    const role = selectedRadio ? selectedRadio.value : 'none';

    if (!configObj) configObj = {};
    if (!configObj.windows) configObj.windows = {
        target_iqn_prefix: "iqn.2024-01.com.tmdebug:vhd-",
        vhd_dir: "",
        block_size: 512,
        vendor_id: "RUSTISCS",
        product_id: "WindowsBoot",
        product_revision: "1.00",
        discovery: false,
        super_client_ip: "",
        super_client_action: "none"
    };
    if (!configObj.writeback) configObj.writeback = {
        writeback_dirs: [],
        max_writeback_per_client_gb: 0,
        max_write_speed_mbps: 100000
    };
    if (!configObj.gamedisk) configObj.gamedisk = [];

    const normalizeDisk = (p) => (p || '').replace(/\\+/g, '\\').toLowerCase().trim();
    const normTargetPhys = normalizeDisk(physicalDisk);
    const normLetterVol = normalizeDisk(`\\\\.\\${letter}:`);

    // Catat ram_cache_mb existing jika disk ini sudah pernah dialokasikan sebagai gamedisk
    let previousRamCache = 0;
    if (configObj.gamedisk && Array.isArray(configObj.gamedisk)) {
        const existingGd = configObj.gamedisk.find(gd => {
            if (!gd) return false;
            if (gd.volume) return String(gd.volume).trim().toUpperCase() === letter + ':';
            if (!gd.physical_disk) return false;
            const normGd = normalizeDisk(gd.physical_disk);
            const matchPhys = normTargetPhys && normGd === normTargetPhys;
            const matchVol = normGd === normLetterVol || normGd.includes(letter.toLowerCase() + ":");
            return matchPhys || matchVol;
        });
        if (existingGd && existingGd.ram_cache_mb !== undefined && existingGd.ram_cache_mb !== null) {
            previousRamCache = existingGd.ram_cache_mb;
        }
    }

    // 1. Clear previous role for this drive letter
    if (configObj.windows && configObj.windows.vhd_dir && configObj.windows.vhd_dir.toUpperCase().startsWith(letter)) {
        configObj.windows.vhd_dir = "";
    }
    if (configObj.writeback && configObj.writeback.writeback_dirs) {
        configObj.writeback.writeback_dirs = configObj.writeback.writeback_dirs.filter(dir => dir && !dir.toUpperCase().startsWith(letter));
    }
    if (configObj.gamedisk && Array.isArray(configObj.gamedisk)) {
        configObj.gamedisk = configObj.gamedisk.filter(gd => {
            if (!gd) return false;
            if (gd.volume) return String(gd.volume).trim().toUpperCase() !== letter + ':';
            if (!gd.physical_disk) return false;
            const normGd = normalizeDisk(gd.physical_disk);
            const matchPhys = normTargetPhys && normGd === normTargetPhys;
            const matchVol = normGd === normLetterVol || normGd.includes(letter.toLowerCase() + ":");
            return !matchPhys && !matchVol;
        });
    }

    // 2. Apply new role if not 'none'
    if (role === 'boot') {
        configObj.windows.vhd_dir = `${letter}:\\`;
    } else if (role === 'writeback') {
        if (!configObj.writeback.writeback_dirs) configObj.writeback.writeback_dirs = [];
        configObj.writeback.writeback_dirs.push(`${letter}:\\writeback`);
    } else if (role === 'gamedisk') {
        if (!configObj.gamedisk) configObj.gamedisk = [];
        const defaultVendor = (configObj.gamedisk_target && configObj.gamedisk_target.vendor_id) || "RUSTISCS";
        const baseProduct = (configObj.gamedisk_target && configObj.gamedisk_target.product_id) || "GameDisk";
        const defaultRev = (configObj.gamedisk_target && configObj.gamedisk_target.product_revision) || "1.00";
        const newProduct = configObj.gamedisk.length > 0 ? baseProduct + '-' + configObj.gamedisk.length : baseProduct;
        configObj.gamedisk.push({
            volume: letter + ':',
            physical_disk: "",
            block_size: 512,
            vendor_id: defaultVendor,
            product_id: newProduct,
            product_revision: defaultRev,
            ram_cache_mb: previousRamCache
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
        const roleLabels = {
            boot: 'IMAGE DISK',
            writeback: 'WRITEBACK',
            gamedisk: 'GAMEDISK'
        };
        const roleName = roleLabels[role] || role.toUpperCase();
        showToast(role === 'none' ? `Alokasi peran Drive ${letter}: berhasil dilepas` : `Peran Drive ${letter}: berhasil diubah ke ${roleName}`, 'success');
    } else {
        showToast(`Gagal menyimpan alokasi partisi Drive ${letter}:`, 'error');
    }
}

async function saveGlobalStorageParams() {
    const throttleMb = parseInt(document.getElementById('disk-throttle-mb').value, 10) || 100000;

    if (!configObj) configObj = {};
    if (!configObj.writeback) configObj.writeback = {
        writeback_dirs: [],
        max_writeback_per_client_gb: 0,
        max_write_speed_mbps: 100000
    };

    // No cap: writeback .bin is not limited per client.
    configObj.writeback.max_writeback_per_client_gb = 0;
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
    const gtCfg = configObj.gamedisk_target || {};
    const gdList = configObj.gamedisk || [];
    const firstGd = gdList[0] || {};

    const osVendorEl = document.getElementById('disk-vendor-os');
    const osProductEl = document.getElementById('disk-product-os');
    const gmVendorEl = document.getElementById('disk-vendor-game');
    const gmProductEl = document.getElementById('disk-product-game');
    const revEl = document.getElementById('disk-revision-common');

    if (osVendorEl) osVendorEl.value = winCfg.vendor_id || 'RUSTISCS';
    if (osProductEl) osProductEl.value = winCfg.product_id || 'WindowsBoot';
    if (gmVendorEl) gmVendorEl.value = gtCfg.vendor_id || firstGd.vendor_id || 'RUSTISCS';
    if (gmProductEl) gmProductEl.value = gtCfg.product_id || firstGd.product_id || 'GameDisk-0';
    if (revEl) revEl.value = winCfg.product_revision || gtCfg.product_revision || firstGd.product_revision || '1.00';

    updateDiskBrandingPreview();
}

function updateDiskBrandingPreview() {
    const osVendor = (document.getElementById('disk-vendor-os')?.value || 'RUSTISCS').trim();
    const osProduct = (document.getElementById('disk-product-os')?.value || 'WindowsBoot').trim();
    const gmVendor = (document.getElementById('disk-vendor-game')?.value || 'RUSTISCS').trim();
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
    if (configObj.windows) {
        configObj.windows.client_cache_enabled = document.getElementById('global-ccachex-enabled')?.checked ?? false;
    }

    if (!configObj.gamedisk_target) {
        configObj.gamedisk_target = { target_iqn: 'iqn.2024-01.com.tmdebug:gamedisks', discovery: true };
    }

    const osVendor = (document.getElementById('disk-vendor-os')?.value || 'RUSTISCS').trim();
    const osProduct = (document.getElementById('disk-product-os')?.value || 'WindowsBoot').trim();
    const gmVendor = (document.getElementById('disk-vendor-game')?.value || 'RUSTISCS').trim();
    const gmProduct = (document.getElementById('disk-product-game')?.value || 'GameDisk-0').trim();
    const rev = (document.getElementById('disk-revision-common')?.value || '1.00').trim();

    configObj.windows.vendor_id = osVendor || 'RUSTISCS';
    configObj.windows.product_id = osProduct || 'WindowsBoot';
    configObj.windows.product_revision = rev || '1.00';

    // Simpan ke gamedisk_target agar tetap tersimpan di config.toml meski belum ada gamedisk yang dialokasikan
    configObj.gamedisk_target.vendor_id = gmVendor || 'RUSTISCS';
    configObj.gamedisk_target.product_id = gmProduct || 'GameDisk-0';
    configObj.gamedisk_target.product_revision = rev || '1.00';

    // Perbarui disk-disk yang sudah ada di array gamedisk
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

        const globalCacheEl = document.getElementById('global-ccachex-enabled');
        if (globalCacheEl) globalCacheEl.checked = !!(currentCfg.windows && currentCfg.windows.client_cache_enabled);
        applyGlobalCacheState();

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
            if (maxCacheEl) maxCacheEl.value = 0;
            const throttleEl = document.getElementById('disk-throttle-mb');
            if (throttleEl) throttleEl.value = currentCfg.writeback.max_write_speed_mbps || 100000;
            const wbWarningEl = document.getElementById('writeback-empty-warning');
            if (wbWarningEl) wbWarningEl.classList.toggle('hidden', !!(currentCfg.writeback.writeback_dirs && currentCfg.writeback.writeback_dirs.length));
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
            <button type="button" class="text-rose-600 dark:text-rose-400 hover:text-rose-700 font-medium p-0.5 cursor-pointer" onclick="removeNicIpAction('${ip}')" title="Hapus IP" aria-label="Hapus IP">✕</button>
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
        populateNetworkDropdowns();
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

    if (configObj.windows) {
        configObj.windows.client_cache_enabled = document.getElementById('global-ccachex-enabled')?.checked ?? false;
    }

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
    if (configObj && configObj.writeback) {
        delete configObj.writeback.max_cache_per_client_gb;
    }
    const res = await apiPost('/api/config/json', configObj);
    return res && res.status === 'ok';
}

// TFTP Bootloader Folders
let pxeComboPanel = null;
let pxeComboInput = null;
let pxeComboItems = [];
let pxeComboIndex = -1;

function closePxeCombo() {
    if (pxeComboPanel) pxeComboPanel.remove();
    pxeComboPanel = null;
    pxeComboInput = null;
    pxeComboItems = [];
    pxeComboIndex = -1;
}

function renderPxeCombo() {
    if (!pxeComboPanel || !pxeComboInput) return;
    const term = pxeComboInput.value.trim().toLowerCase();
    pxeComboItems = (tftpFoldersCache || []).filter(f => !term || String(f).toLowerCase().includes(term));
    if (pxeComboIndex >= pxeComboItems.length) pxeComboIndex = pxeComboItems.length - 1;
    pxeComboPanel.innerHTML = pxeComboItems.length
        ? pxeComboItems.map((f, i) => '<div data-pxe-idx="' + i + '" class="px-3 py-1.5 text-xs cursor-pointer ' + (i === pxeComboIndex ? 'bg-zinc-100 dark:bg-zinc-800' : 'hover:bg-zinc-100 dark:hover:bg-zinc-800') + '">' + escapeHtml(f) + '</div>').join('')
        : '<div class="px-3 py-1.5 text-xs text-zinc-500 dark:text-zinc-400">Tidak ada folder cocok. Nilai manual tetap dipakai.</div>';
}

function openPxeCombo(input) {
    if (!input) return;
    if (pxeComboPanel && pxeComboInput === input) { renderPxeCombo(); return; }
    closePxeCombo();
    pxeComboInput = input;
    pxeComboPanel = document.createElement('div');
    pxeComboPanel.className = 'fixed z-[100] max-h-56 overflow-auto rounded-md border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-900 shadow-lg py-1';
    pxeComboPanel.style.zIndex = '10000';
    const r = input.getBoundingClientRect();
    pxeComboPanel.style.left = r.left + 'px';
    pxeComboPanel.style.top = (r.bottom + 4) + 'px';
    pxeComboPanel.style.width = r.width + 'px';
    document.body.appendChild(pxeComboPanel);
    pxeComboPanel.addEventListener('mousedown', (e) => {
        const row = e.target.closest('[data-pxe-idx]');
        if (!row || !pxeComboInput) return;
        e.preventDefault();
        const val = pxeComboItems[Number(row.getAttribute('data-pxe-idx'))];
        if (val !== undefined) {
            pxeComboInput.value = val;
            pxeComboInput.dispatchEvent(new Event('input', { bubbles: true }));
        }
        closePxeCombo();
    });
    renderPxeCombo();
}

function attachPxeCombo(input) {
    if (!input || input.dataset.pxeCombo === '1') return;
    input.dataset.pxeCombo = '1';
    input.removeAttribute('list');
    input.addEventListener('focus', () => openPxeCombo(input));
    input.addEventListener('click', () => openPxeCombo(input));
    input.addEventListener('input', () => {
        if (pxeComboInput !== input) openPxeCombo(input);
        else { pxeComboIndex = -1; renderPxeCombo(); }
    });
    input.addEventListener('keydown', (e) => {
        if (!pxeComboPanel || pxeComboInput !== input) return;
        if (e.key === 'ArrowDown') {
            e.preventDefault();
            if (pxeComboItems.length) { pxeComboIndex = (pxeComboIndex + 1) % pxeComboItems.length; renderPxeCombo(); }
        } else if (e.key === 'ArrowUp') {
            e.preventDefault();
            if (pxeComboItems.length) { pxeComboIndex = pxeComboIndex <= 0 ? pxeComboItems.length - 1 : pxeComboIndex - 1; renderPxeCombo(); }
        } else if (e.key === 'Enter') {
            if (pxeComboIndex >= 0 && pxeComboItems[pxeComboIndex] !== undefined) {
                e.preventDefault();
                input.value = pxeComboItems[pxeComboIndex];
                input.dispatchEvent(new Event('input', { bubbles: true }));
                closePxeCombo();
            }
        } else if (e.key === 'Escape') {
            e.preventDefault();
            closePxeCombo();
        }
    });
}

function initPxeCombos() {
    attachPxeCombo(document.getElementById('set-pxe-default'));
    attachPxeCombo(document.getElementById('client-pxe'));
}

document.addEventListener('mousedown', (e) => {
    if (!pxeComboPanel) return;
    if (pxeComboPanel.contains(e.target) || e.target === pxeComboInput) return;
    closePxeCombo();
});
window.addEventListener('resize', closePxeCombo);
window.addEventListener('scroll', (e) => {
    if (pxeComboPanel && !pxeComboPanel.contains(e.target)) closePxeCombo();
}, true);

async function loadTftpFolders() {
    const folders = await apiGet('/api/system/tftp_folders');
    const tbody = document.getElementById('tftp-folders-tbody');
    tftpFoldersCache = Array.isArray(folders) ? folders : [];

    const rootInfo = await apiGet('/api/system/tftp_root');
    const hint = document.getElementById('tftp-folder-hint');
    if (hint) {
        if (!rootInfo || !rootInfo.absolute) {
            hint.textContent = 'TFTP root belum diatur. Edit config.toml [dhcp] tftp_dir.';
        } else if (!rootInfo.exists) {
            hint.textContent = 'Folder TFTP root belum ada: ' + rootInfo.absolute;
        } else {
            hint.textContent = 'Folder harus di dalam TFTP root: ' + rootInfo.absolute + ' (contoh: ' + rootInfo.absolute + '\\sb-custom)';
        }
    }

    if (!tbody) return;
    const visible = tftpFoldersCache.slice(0, 5);
    if (visible.length === 0) {
        tbody.innerHTML = '<tr><td colspan="2" class="py-6 px-4 text-center text-zinc-500 dark:text-zinc-400 text-xs">Belum ada folder bootloader kustom.</td></tr>';
        return;
    }

    tbody.innerHTML = '';
    visible.forEach(f => {
        const row = document.createElement('tr');
        row.className = 'hover:bg-zinc-50/80 dark:hover:bg-zinc-900/50 border-b border-zinc-100 dark:border-zinc-800/80';
        row.innerHTML = '<td class="py-2 px-4 font-mono text-xs font-medium text-zinc-900 dark:text-zinc-100">' + escapeHtml(f) + '</td>'
            + '<td class="py-2 px-4 text-right"><button type="button" data-tftp-delete class="text-rose-600 dark:text-rose-400 hover:text-rose-700 text-xs font-medium cursor-pointer">Hapus</button></td>';
        row.querySelector('[data-tftp-delete]').addEventListener('click', () => deleteTftpFolderAction(f));
        tbody.appendChild(row);
    });
}

async function pickTftpFolderAction() {
    showToast('Pilih folder di dialog Windows yang terbuka', 'info');
    const res = await apiPost('/api/system/tftp_folders/pick', {}, 0);
    if (!res || !res.name) return;
    const created = await apiPost('/api/system/tftp_folders/create', { name: res.name });
    if (created && created.success) {
        showToast('Folder TFTP ditambahkan', 'success');
        loadTftpFolders();
    } else {
        showToast((created && created.error) ? created.error : 'Gagal menambahkan folder TFTP', 'error');
    }
}

function deleteTftpFolderAction(folderName) {
    showConfirmModal('Hapus Folder TFTP', `Yakin ingin menghapus folder TFTP '${folderName}'?`, async () => {
        const res = await apiPost('/api/system/tftp_folders/delete', { name: folderName });
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

function showMergeModal(targetInfo = 'Base OS Image') {
    const modal = document.getElementById('modal-superclient-merge');
    if (!modal) return;

    const iconEl = document.getElementById('merge-modal-icon');
    const badgeEl = document.getElementById('merge-modal-status-badge');
    const targetInfoEl = document.getElementById('merge-modal-target-info');
    const pctEl = document.getElementById('merge-modal-pct');
    const barEl = document.getElementById('merge-modal-bar');
    const blocksEl = document.getElementById('merge-modal-blocks');
    const sizeEl = document.getElementById('merge-modal-size');
    const msgEl = document.getElementById('merge-modal-msg');
    const safetyNotice = document.getElementById('merge-modal-safety-notice');
    const successBox = document.getElementById('merge-modal-success-box');
    const errorBox = document.getElementById('merge-modal-error-box');
    const actionBtn = document.getElementById('merge-modal-action-btn');
    const btnText = document.getElementById('merge-modal-btn-text');
    const spinner = document.getElementById('merge-modal-spinner');

    if (iconEl) {
        iconEl.textContent = '';
        iconEl.className = 'animate-pulse text-amber-500';
    }
    if (badgeEl) {
        badgeEl.textContent = 'Sedang Berjalan';
        badgeEl.className = 'px-2 py-0.5 text-[10px] sm:text-xs font-semibold rounded-full bg-amber-50 dark:bg-amber-950/60 text-amber-600 dark:text-amber-400 border border-amber-200 dark:border-amber-800 shrink-0';
    }
    if (targetInfoEl) targetInfoEl.textContent = `Target: ${targetInfo}`;
    if (pctEl) pctEl.textContent = '0%';
    if (barEl) barEl.style.width = '0%';
    if (blocksEl) blocksEl.textContent = 'Blok: 0 / 0';
    if (sizeEl) sizeEl.textContent = '0 MB / 0 MB';
    if (msgEl) msgEl.textContent = 'Mempersiapkan snapshot & backup metadata...';

    if (safetyNotice) safetyNotice.classList.remove('hidden');
    if (successBox) successBox.classList.add('hidden');
    if (errorBox) errorBox.classList.add('hidden');

    if (actionBtn) {
        actionBtn.disabled = true;
        actionBtn.className = 'w-full py-2.5 px-4 rounded-lg bg-zinc-200 dark:bg-zinc-800 text-zinc-400 dark:text-zinc-500 text-xs sm:text-sm font-semibold cursor-not-allowed flex items-center justify-center gap-2 transition-all';
        actionBtn.onclick = null;
    }
    if (spinner) spinner.classList.remove('hidden');
    if (btnText) btnText.textContent = 'Sedang Menggabungkan VHD...';

    modal.style.display = 'flex';
}

function updateMergeModal(pct, currentBlock, totalBlocks, msg) {
    const pctEl = document.getElementById('merge-modal-pct');
    const barEl = document.getElementById('merge-modal-bar');
    const blocksEl = document.getElementById('merge-modal-blocks');
    const sizeEl = document.getElementById('merge-modal-size');
    const msgEl = document.getElementById('merge-modal-msg');

    const pctNum = Math.min(100, Math.max(0, parseInt(pct, 10) || 0));
    if (pctEl) pctEl.textContent = `${pctNum}%`;
    if (barEl) barEl.style.width = `${pctNum}%`;

    const curB = currentBlock || 0;
    const totB = totalBlocks || 0;
    if (blocksEl) blocksEl.textContent = `Blok: ${curB.toLocaleString()} / ${totB.toLocaleString()}`;

    // Dynamic VHD blocks are 2MB each
    const curMb = (curB * 2).toFixed(1);
    const totMb = (totB * 2).toFixed(1);
    if (sizeEl) sizeEl.textContent = `${curMb} MB / ${totMb} MB`;

    if (msgEl && msg) msgEl.textContent = msg;
}

function setMergeModalSuccess(msg) {
    updateMergeModal(100, 0, 0, msg || 'Commit Super Client selesai. Perubahan berhasil digabungkan.');

    const iconEl = document.getElementById('merge-modal-icon');
    const badgeEl = document.getElementById('merge-modal-status-badge');
    const safetyNotice = document.getElementById('merge-modal-safety-notice');
    const successBox = document.getElementById('merge-modal-success-box');
    const errorBox = document.getElementById('merge-modal-error-box');
    const actionBtn = document.getElementById('merge-modal-action-btn');
    const btnText = document.getElementById('merge-modal-btn-text');
    const spinner = document.getElementById('merge-modal-spinner');

    if (iconEl) {
        iconEl.textContent = '';
        iconEl.className = 'text-emerald-500 text-lg';
    }
    if (badgeEl) {
        badgeEl.textContent = 'Selesai 100%';
        badgeEl.className = 'px-2 py-0.5 text-[10px] sm:text-xs font-semibold rounded-full bg-emerald-50 dark:bg-emerald-950/60 text-emerald-600 dark:text-emerald-400 border border-emerald-200 dark:border-emerald-800 shrink-0';
    }

    if (safetyNotice) safetyNotice.classList.add('hidden');
    if (errorBox) errorBox.classList.add('hidden');
    if (successBox) successBox.classList.remove('hidden');

    if (spinner) spinner.classList.add('hidden');
    if (btnText) btnText.textContent = 'Selesai & Tutup';

    if (actionBtn) {
        actionBtn.disabled = false;
        actionBtn.className = 'w-full py-2.5 px-4 rounded-lg bg-emerald-600 hover:bg-emerald-500 active:bg-emerald-700 text-white text-xs sm:text-sm font-semibold shadow-md transition-all cursor-pointer flex items-center justify-center gap-2';
        actionBtn.onclick = closeMergeModal;
    }
}

function setMergeModalError(errMsg) {
    const iconEl = document.getElementById('merge-modal-icon');
    const badgeEl = document.getElementById('merge-modal-status-badge');
    const safetyNotice = document.getElementById('merge-modal-safety-notice');
    const errorBox = document.getElementById('merge-modal-error-box');
    const errorMsgEl = document.getElementById('merge-modal-error-msg');
    const actionBtn = document.getElementById('merge-modal-action-btn');
    const btnText = document.getElementById('merge-modal-btn-text');
    const spinner = document.getElementById('merge-modal-spinner');

    if (iconEl) {
        iconEl.textContent = '';
        iconEl.className = 'text-rose-500 text-lg';
    }
    if (badgeEl) {
        badgeEl.textContent = 'Gagal';
        badgeEl.className = 'px-2 py-0.5 text-[10px] sm:text-xs font-semibold rounded-full bg-rose-50 dark:bg-rose-950/60 text-rose-600 dark:text-rose-400 border border-rose-200 dark:border-rose-800 shrink-0';
    }

    if (safetyNotice) safetyNotice.classList.add('hidden');
    if (errorBox) errorBox.classList.remove('hidden');
    if (errorMsgEl) errorMsgEl.textContent = errMsg || 'Terjadi kesalahan internal saat menggabungkan differencing VHD';

    if (spinner) spinner.classList.add('hidden');
    if (btnText) btnText.textContent = 'Tutup (Error)';

    if (actionBtn) {
        actionBtn.disabled = false;
        actionBtn.className = 'w-full py-2.5 px-4 rounded-lg bg-rose-600 hover:bg-rose-500 text-white text-xs sm:text-sm font-semibold shadow-md transition-all cursor-pointer flex items-center justify-center gap-2';
        actionBtn.onclick = closeMergeModal;
    }
}

function closeMergeModal() {
    const modal = document.getElementById('modal-superclient-merge');
    if (modal) modal.style.display = 'none';
}

function startMergeProgressPolling(targetInfo = 'Base OS Image') {
    if (mergePollingTimer) {
        clearInterval(mergePollingTimer);
        mergePollingTimer = null;
    }

    showMergeModal(targetInfo);

    mergePollingTimer = setInterval(async () => {
        const data = await apiGet('/api/vhd/merge_status');
        if (!data) return;

        if (data.is_merging) {
            updateMergeModal(data.progress || 0, data.current_block || 0, data.total_blocks || 0, data.message || 'Menggabungkan differencing VHD...');
        } else {
            // Selesai atau Error
            clearInterval(mergePollingTimer);
            mergePollingTimer = null;

            if (data.error) {
                setMergeModalError(data.error);
                showToast(`Gagal merge VHD Super Client: ${data.error}`, 'error');
            } else {
                setMergeModalSuccess(data.message || 'Commit Super Client berhasil! Master VHD telah diperbarui.');
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
            }
        }
    }, 600);
}

async function checkInitialMergeStatus() {
    const data = await apiGet('/api/vhd/merge_status');
    if (data && data.is_merging) {
        startMergeProgressPolling(data.image_key || 'Base OS');
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
            if (res.requires_merge === false) {
                showToast(res.message || 'Mode Super Client berhasil dinonaktifkan.', 'success');
                if (!configObj) configObj = {};
                if (!configObj.windows) configObj.windows = {};
                configObj.windows.super_client_ip = '';
                configObj.windows.super_client_action = '';
                renderedDashboardIps = [];
                await loadConfigJson();
                await loadClientsJson();
                renderClientsManagerTable();
                renderDashboardClientsTable();
            } else {
                startMergeProgressPolling(hostname || ip);
            }
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



// ==========================================
// TAB 6: ISCSI RAM CACHE MONITOR & TELEMETRY
// ==========================================

async function loadCacheStats() {
    try {
        const data = await apiGet('/api/cache/stats');
        if (!data) return;

        // 1. Update Overview Stat Cards
        const allocMbEl = document.getElementById('cache-stat-total-allocated');
        const allocGbEl = document.getElementById('cache-stat-total-allocated-gb');
        const usedMbEl = document.getElementById('cache-stat-total-used');
        const usedPctEl = document.getElementById('cache-stat-used-pct');
        const usedBarEl = document.getElementById('cache-used-progress-bar');
        const hitRateEl = document.getElementById('cache-stat-hit-rate');
        const totalHitsEl = document.getElementById('cache-stat-total-hits');
        const totalMissesEl = document.getElementById('cache-stat-total-misses');

        const totalAllocMb = data.total_allocated_mb || 0;
        const totalUsedMb = data.total_used_mb || 0;
        const usedPct = totalAllocMb > 0 ? Math.min(100, (totalUsedMb / totalAllocMb * 100)).toFixed(1) : '0';

        if (allocMbEl) allocMbEl.textContent = `${totalAllocMb.toLocaleString()} MB`;
        if (allocGbEl) allocGbEl.textContent = `(${(totalAllocMb / 1024).toFixed(1)} GB)`;
        if (usedMbEl) usedMbEl.textContent = `${totalUsedMb.toLocaleString()} MB`;
        if (usedPctEl) usedPctEl.textContent = `(${usedPct}%)`;
        if (usedBarEl) usedBarEl.style.width = `${usedPct}%`;

        if (hitRateEl) {
            const hitRate = Number(data.overall_hit_rate_pct || 0).toFixed(1);
            hitRateEl.textContent = `${hitRate}%`;
            if (hitRate >= 90) {
                hitRateEl.className = 'text-xl sm:text-2xl font-bold font-mono text-emerald-600 dark:text-emerald-400 tracking-tight';
            } else if (hitRate >= 70) {
                hitRateEl.className = 'text-xl sm:text-2xl font-bold font-mono text-amber-600 dark:text-amber-400 tracking-tight';
            } else {
                hitRateEl.className = 'text-xl sm:text-2xl font-bold font-mono text-zinc-600 dark:text-zinc-400 tracking-tight';
            }
        }

        if (totalHitsEl) totalHitsEl.textContent = (data.total_hits || 0).toLocaleString();
        if (totalMissesEl) totalMissesEl.textContent = (data.total_misses || 0).toLocaleString();

        currentCacheDisks = data.disks || [];

        // 2. Render Per-Disk Cache Table
        const tbody = document.getElementById('cache-disks-tbody');
        if (!tbody) return;

        if (!currentCacheDisks.length) {
            tbody.innerHTML = `
                <tr>
                    <td colspan="6" class="px-4 py-8 text-center text-zinc-500">
                        Belum ada pool cache yang aktif. Boot klien iSCSI atau hubungkan disk untuk memulai pemantauan.
                    </td>
                </tr>
            `;
            return;
        }

        tbody.innerHTML = currentCacheDisks.slice(0, 6).map(disk => {
            const isReadCache = disk.disk_type === 'imagedisk_read' || disk.disk_type === 'gamedisk_read' || disk.disk_type === 'imagedisk' || disk.disk_type === 'gamedisk';
            const isWriteback = disk.disk_type === 'writeback_write' || disk.disk_type === 'writeback';
            const isOff = !disk.ram_cache_mb || disk.ram_cache_mb === 0;

            const hitRate = Number(disk.hit_rate_pct || 0);
            const hitRateStr = hitRate.toFixed(1);
            let barColor = 'bg-emerald-500';
            let badgeColor = 'text-emerald-700 bg-emerald-50 border-emerald-200 dark:bg-emerald-950/40 dark:text-emerald-400 dark:border-emerald-800';

            if (hitRate < 70) {
                barColor = 'bg-zinc-400 dark:bg-zinc-600';
                badgeColor = 'text-zinc-700 bg-zinc-50 border-zinc-200 dark:bg-zinc-900/60 dark:text-zinc-400 dark:border-zinc-800';
            } else if (hitRate < 90) {
                barColor = 'bg-amber-500';
                badgeColor = 'text-amber-700 bg-amber-50 border-amber-200 dark:bg-amber-950/40 dark:text-amber-400 dark:border-amber-800';
            }

            let typeBadge = '';
            let diskIcon = '';
            if (disk.disk_type === 'imagedisk_read' || disk.disk_type === 'imagedisk') {
                diskIcon = '';
                typeBadge = `<span class="inline-flex items-center gap-1 px-1.5 py-0.2 rounded text-[10px] font-medium bg-emerald-50 dark:bg-emerald-950/50 text-emerald-700 dark:text-emerald-400 border border-emerald-200 dark:border-emerald-800 whitespace-nowrap"> Image Disk (Read)</span>`;
            } else if (disk.disk_type === 'gamedisk_read' || disk.disk_type === 'gamedisk') {
                diskIcon = '';
                typeBadge = `<span class="inline-flex items-center gap-1 px-1.5 py-0.2 rounded text-[10px] font-medium bg-indigo-50 dark:bg-indigo-950/50 text-indigo-700 dark:text-indigo-400 border border-indigo-200 dark:border-indigo-800 whitespace-nowrap"> GameDisk (Read)</span>`;
            } else if (isWriteback) {
                diskIcon = '';
                typeBadge = `<span class="inline-flex items-center gap-1 px-1.5 py-0.2 rounded text-[10px] font-medium bg-amber-50 dark:bg-amber-950/50 text-amber-700 dark:text-amber-400 border border-amber-200 dark:border-amber-800 whitespace-nowrap"> Writeback (Write)</span>`;
            }

            const allocGb = (disk.ram_cache_mb / 1024).toFixed(1);

            return `
                <tr data-cache-id="${escapeHtml(disk.disk_id)}" class="hover:bg-zinc-50/70 dark:hover:bg-zinc-900/40 transition-colors cursor-pointer">
                    <td class="px-4 py-3">
                        <div class="font-medium text-zinc-900 dark:text-zinc-100 flex items-center gap-1.5 break-words" title="${escapeHtml(disk.name)}">

                            <span class="break-words">${escapeHtml(disk.name)}</span>
                        </div>
                        <div class="text-[11px] font-mono text-zinc-400 mt-0.5 break-words">${escapeHtml(disk.disk_id)}</div>
                    </td>
                    <td class="px-4 py-3">
                        <div class="mb-0.5">${typeBadge}</div>
                        <div class="text-[11px] font-mono font-medium text-zinc-700 dark:text-zinc-300 break-words" title="${escapeHtml(disk.path)}">${escapeHtml(formatDriveDisplay(disk.path))}</div>
                    </td>
                    <td class="px-4 py-3 font-mono">
                        ${isOff
                            ? `<div class="font-semibold text-zinc-400 dark:text-zinc-500 break-words">0 MB</div>
                               <div class="text-[10px] text-zinc-400 break-words"> Nonaktif</div>`
                            : `<div class="font-semibold text-zinc-900 dark:text-zinc-100 break-words">${disk.ram_cache_mb.toLocaleString()} MB</div>
                               <div class="text-[11px] text-zinc-400 break-words">(${allocGb} GB)</div>`
                        }
                    </td>
                    <td class="px-4 py-3 font-mono">
                        ${isOff
                            ? `<div class="text-zinc-400 dark:text-zinc-500 break-words">Direct Disk I/O</div>
                               <div class="text-[10px] text-zinc-400 break-words">Tanpa Buffer</div>`
                            : `<div class="text-zinc-800 dark:text-zinc-200 break-words">${(disk.used_cache_mb || 0).toLocaleString()} MB</div>
                               <div class="text-[10px] text-zinc-400 break-words">${(disk.entry_count || 0).toLocaleString()} chunks</div>`
                        }
                    </td>
                    <td class="px-4 py-3">
                        ${isOff
                            ? `<div class="flex items-center gap-1 mb-1">
                                   <span class="font-mono text-xs text-zinc-400">-</span>
                                   <span class="text-[9px] px-1 py-0.2 rounded border font-mono text-zinc-400 border-zinc-200 dark:border-zinc-800">DIRECT</span>
                               </div>
                               <div class="w-12 sm:w-16 xl:w-20 bg-zinc-100 dark:bg-zinc-800 rounded-full h-1.5 overflow-hidden border border-zinc-200 dark:border-zinc-700">
                                   <div class="bg-zinc-300 dark:bg-zinc-700 h-1.5 rounded-full" style="width: 0%"></div>
                               </div>`
                            : `<div class="flex items-center gap-1 mb-1">
                                   <span class="font-mono font-semibold text-xs text-zinc-900 dark:text-zinc-100">${hitRateStr}%</span>
                                   <span class="text-[9px] px-1 py-0.2 rounded border font-mono ${badgeColor}">${isWriteback ? 'BUFFER' : 'RAM'}</span>
                               </div>
                               <div class="w-12 sm:w-16 xl:w-20 bg-zinc-100 dark:bg-zinc-800 rounded-full h-1.5 overflow-hidden border border-zinc-200 dark:border-zinc-700">
                                   <div class="${barColor} h-1.5 rounded-full transition-all duration-300" style="width: ${hitRate}%"></div>
                               </div>`
                        }
                    </td>
                    <td class="px-4 py-3 font-mono text-[11px]">
                        ${isOff
                            ? `<div class="text-zinc-400 dark:text-zinc-500 break-words">Direct I/O</div>`
                            : isWriteback
                            ? `<div class="text-amber-600 dark:text-amber-400 font-medium break-words">Write Buffer</div>
                               <div class="text-zinc-500 dark:text-zinc-400 break-words">Flush Otomatis</div>`
                            : `<div class="text-emerald-600 dark:text-emerald-400 font-medium break-words">H: ${(disk.hit_count || 0).toLocaleString()}</div>
                               <div class="text-zinc-500 dark:text-zinc-400 break-words">M: ${(disk.miss_count || 0).toLocaleString()}</div>`
                        }
                    </td>
                </tr>
            `;
        }).join('');

        tbody.querySelectorAll('tr[data-cache-id]').forEach(row => {
            row.addEventListener('click', () => openCacheDetailModal(row.dataset.cacheId));
        });

    } catch (err) {
        console.error('loadCacheStats error:', err);
    }
}

function openCacheDetailModal(diskId) {
    const disk = (currentCacheDisks || []).find(d => d.disk_id === diskId);
    if (!disk) return;
    const modal = document.getElementById('cache-detail-modal');
    if (!modal) return;
    document.getElementById('cache-detail-title').textContent = disk.name || 'Detail Cache';
    document.getElementById('cache-detail-subtitle').textContent = disk.disk_id || '';
    const isOff = !disk.ram_cache_mb || disk.ram_cache_mb === 0;
    const rows = [
        ['Tipe', disk.disk_type || '-'],
        ['Path', formatDriveDisplay(disk.path) || '-'],
        ['Alokasi RAM', isOff ? '0 MB' : (disk.ram_cache_mb || 0).toLocaleString() + ' MB'],
        ['Terpakai', (disk.used_cache_mb || 0).toLocaleString() + ' MB'],
        ['Hit Rate', Number(disk.hit_rate_pct || 0).toFixed(1) + '%'],
        ['Hits / Miss', (disk.hit_count || 0).toLocaleString() + ' / ' + (disk.miss_count || 0).toLocaleString()],
        ['Entry', (disk.entry_count || 0).toLocaleString() + ' chunks']
    ];
    document.getElementById('cache-detail-body').innerHTML = rows.map(r =>
        '<div class="flex items-start justify-between gap-3 py-1.5 border-b border-zinc-100 dark:border-zinc-800/70"><span class="text-zinc-500 dark:text-zinc-400">' + r[0] + '</span><span class="font-mono text-zinc-900 dark:text-zinc-100 text-right break-all">' + escapeHtml(String(r[1])) + '</span></div>'
    ).join('');
    const edit = document.getElementById('cache-detail-edit');
    const flush = document.getElementById('cache-detail-flush');
    edit.onclick = () => { closeCacheDetailModal(); openEditCacheModal(disk.disk_id); };
    flush.disabled = isOff;
    flush.onclick = () => { if (!isOff) { closeCacheDetailModal(); clearDiskCache(disk.disk_id); } };
    modal.style.display = 'flex';
}

function closeCacheDetailModal() {
    const modal = document.getElementById('cache-detail-modal');
    if (modal) modal.style.display = 'none';
}

function openEditCacheModal(diskId) {
    const disk = currentCacheDisks.find(d => d.disk_id === diskId);
    const modal = document.getElementById('modal-edit-cache');
    if (!modal) return;

    const currentMb = disk ? disk.ram_cache_mb : 0;
    const name = disk ? disk.name : diskId;

    document.getElementById('edit-cache-disk-id').value = diskId;
    document.getElementById('edit-cache-disk-name').value = `${name} (${diskId})`;
    document.getElementById('edit-cache-mb-input').value = (currentMb !== undefined && currentMb !== null) ? currentMb : 0;
    document.getElementById('modal-edit-cache-title').textContent = `Ubah Alokasi RAM Cache: ${name}`;

    modal.style.display = 'flex';
}

function closeEditCacheModal() {
    const modal = document.getElementById('modal-edit-cache');
    if (modal) modal.style.display = 'none';
}

function setPresetSize(preset) {
    const input = document.getElementById('edit-cache-mb-input');
    if (!input) return;

    const map = {
        '0M': 0,
        '512M': 512,
        '1G': 1024,
        '2G': 2048,
        '4G': 4096,
        '8G': 8192,
        '16G': 16384,
    };
    if (map[preset] !== undefined) {
        input.value = map[preset];
    }
}

async function submitEditCache(e) {
    e.preventDefault();
    const diskId = document.getElementById('edit-cache-disk-id').value;
    const mbVal = parseInt(document.getElementById('edit-cache-mb-input').value, 10);

    if (isNaN(mbVal) || mbVal < 0) {
        showToast('Kapasitas RAM Cache tidak valid', 'error');
        return;
    }

    try {
        const res = await apiPost('/api/cache/config', {
            disk_id: diskId,
            ram_cache_mb: mbVal
        });

        if (res && res.status === 'ok') {
            showToast(res.message || 'Alokasi RAM cache berhasil diubah!', 'success');
            closeEditCacheModal();
            await loadCacheStats();
        } else {
            showToast(res ? (res.error || res.message) : 'Gagal memperbarui konfigurasi cache', 'error');
        }
    } catch (err) {
        showToast('Terjadi kesalahan saat menyimpan alokasi cache', 'error');
    }
}

async function clearDiskCache(diskId) {
    const targetLabel = diskId === 'all' ? 'seluruh cache server' : `cache untuk disk ${diskId}`;
    if (!confirm(`Apakah Anda yakin ingin membersihkan (flush) ${targetLabel}?`)) {
        return;
    }

    try {
        showToast(`Membersihkan ${targetLabel}...`, 'info');
        const res = await apiPost('/api/cache/clear', { disk_id: diskId });
        if (res && res.status === 'ok') {
            showToast(res.message || 'Cache berhasil dibersihkan!', 'success');
            await loadCacheStats();
        } else {
            showToast('Gagal membersihkan cache', 'error');
        }
    } catch (err) {
        showToast('Terjadi kesalahan saat membersihkan cache', 'error');
    }
}
