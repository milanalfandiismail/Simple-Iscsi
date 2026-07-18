// State variables
let activeTab = 'dashboard';
let stats = {};
let configObj = null; // Parsed config.toml JSON representation
let clientsObj = { client: [] }; // Parsed clients.toml JSON representation
let activeSessionsMap = new Map();
let clientSpeedHistory = new Map();

// Initialization
document.addEventListener('DOMContentLoaded', async () => {
    initTabs();
    initContextMenus();
    initTheme();
    
    // Initial data loading sequence
    await loadInitialData();

    // Start stats polling loop every 2 seconds
    pollStats();
    setInterval(pollStats, 2000);
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
    await loadConfigJson();
    await loadClientsJson();
    loadWritebackFiles();
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
            if (activeTab === 'settings') { loadConfigJson(); loadTftpFolders(); }
            if (activeTab === 'clients') loadClientsJson();
            if (activeTab === 'vhd') loadConfigJson();
            if (activeTab === 'disk-mgmt') loadConfigJson();
            if (activeTab === 'writeback') loadWritebackFiles();
        });
    });
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
async function pollStats() {
    const data = await apiGet('/api/stats');
    if (!data) return;

    stats = data;

    // 1. Dynamic service cards updates
    if (data.services) {
        updateServiceCard('iscsi', data.services.iscsi);
        updateServiceCard('dhcp', data.services.dhcp);
        updateServiceCard('tftp', data.services.tftp);
    }

    // 2. Active connections total value
    document.getElementById('stat-conns').textContent = data.active_sessions;

    // Track active sessions map for table referencing
    activeSessionsMap.clear();
    if (data.clients) {
        data.clients.forEach(c => {
            activeSessionsMap.set(c.ip, c);
        });
    }

    // 3. Update existing table cells in DOM instead of rebuilding them (stops flickering/flashing)
    if (clientsObj && clientsObj.client) {
        const now = Date.now();
        clientsObj.client.forEach(c => {
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
                dbRow.cells[0].innerHTML = statusText;
                dbRow.cells[6].textContent = formatBytes(statsInfo.bytes_read);
                dbRow.cells[7].textContent = formatSpeed(speedInfo.readSpeed);
                dbRow.cells[8].textContent = formatBytes(statsInfo.bytes_written);
                dbRow.cells[9].textContent = formatSpeed(speedInfo.writeSpeed);
                dbRow.cells[10].textContent = statsInfo.active ? formatDuration(statsInfo.uptime_secs) : 'Offline';
            }

            // Update Clients Manager Table Row
            const cmRow = document.querySelector(`#clients-tbody tr[data-ip="${c.ip}"]`);
            if (cmRow) {
                cmRow.cells[0].innerHTML = statusText;
                cmRow.cells[8].textContent = formatBytes(statsInfo.bytes_read);
                cmRow.cells[9].textContent = formatSpeed(speedInfo.readSpeed);
                cmRow.cells[10].textContent = formatBytes(statsInfo.bytes_written);
                cmRow.cells[11].textContent = formatSpeed(speedInfo.writeSpeed);
                cmRow.cells[12].textContent = statsInfo.active ? formatDuration(statsInfo.uptime_secs) : 'Offline';
            }
        });
    }
}

function updateServiceCard(name, service) {
    const portEl = document.getElementById(`${name}-port-info`);
    const pillEl = document.getElementById(`${name}-status-pill`);
    if (portEl && pillEl) {
        portEl.textContent = `Port: ${service.port}`;
        if (service.enabled) {
            pillEl.textContent = '🟢 Enabled';
            pillEl.className = 'pill-status active';
        } else {
            pillEl.textContent = '🔴 Disabled';
            pillEl.className = 'pill-status';
        }
    }
}

// Render Dashboard Clients Table (Static clients with real-time stats)
function renderDashboardClientsTable() {
    const tbody = document.getElementById('dashboard-clients-tbody');
    if (!clientsObj.client || clientsObj.client.length === 0) {
        tbody.innerHTML = `<tr><td colspan="11" style="text-align: center; color: var(--color-muted);">Tidak ada klien terdaftar di clients.toml.</td></tr>`;
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
            ? `<span style="color: #22c55e;">🟢 Online</span>`
            : `<span style="color: #ef4444;">🔴 Offline</span>`;

        const isSuper = configObj && configObj.windows && configObj.windows.super_client_ip === c.ip;
        const superBadge = isSuper ? ` <span class="pill-status" style="background-color: #fef08a; color: #854d0e; font-size: 11px; padding: 2px 6px;">⚡ Super Client</span>` : '';

        const row = document.createElement('tr');
        row.setAttribute('data-ip', c.ip);
        row.innerHTML = `
            <td>${statusSpan}</td>
            <td><strong>${c.ip}${superBadge}</strong></td>
            <td>${c.dns || '-'}</td>
            <td>${c.gateway || '-'}</td>
            <td><code>${c.image_manager || 'None (Gamedisk)'}</code></td>
            <td>${c.next_server || '-'}</td>
            <td>${formatBytes(statsInfo.bytes_read)}</td>
            <td>${formatSpeed(speedInfo.readSpeed)}</td>
            <td>${formatBytes(statsInfo.bytes_written)}</td>
            <td>${formatSpeed(speedInfo.writeSpeed)}</td>
            <td>${statsInfo.active ? formatDuration(statsInfo.uptime_secs) : 'Offline'}</td>
        `;
        tbody.appendChild(row);
    });

    document.getElementById('stat-total-pcs').textContent = clientsObj.client.length;
}

// Render Clients Manager Tab Table (Full List)
function renderClientsManagerTable() {
    const tbody = document.getElementById('clients-tbody');
    if (!clientsObj.client || clientsObj.client.length === 0) {
        tbody.innerHTML = `<tr><td colspan="13" style="text-align: center; color: var(--color-muted);">Tidak ada klien terdaftar. Silakan tambah baru.</td></tr>`;
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
            ? `<span style="color: #22c55e;">🟢 Online</span>`
            : `<span style="color: #ef4444;">🔴 Offline</span>`;

        const isSuper = configObj && configObj.windows && configObj.windows.super_client_ip === c.ip;
        const superBadge = isSuper ? ` <span class="pill-status" style="background-color: #fef08a; color: #854d0e; font-size: 11px; padding: 2px 6px;">⚡ Super Client</span>` : '';

        const row = document.createElement('tr');
        row.setAttribute('data-ip', c.ip);
        row.innerHTML = `
            <td>${statusSpan}</td>
            <td><strong>${c.hostname || 'PC'}${superBadge}</strong></td>
            <td>${c.ip}</td>
            <td><code>${c.mac}</code></td>
            <td>${c.dns || '-'}</td>
            <td>${c.gateway || '-'}</td>
            <td><code>${c.image_manager || 'Gamedisk'}</code></td>
            <td>${c.next_server || '-'}</td>
            <td>${formatBytes(statsInfo.bytes_read)}</td>
            <td>${formatSpeed(speedInfo.readSpeed)}</td>
            <td>${formatBytes(statsInfo.bytes_written)}</td>
            <td>${formatSpeed(speedInfo.writeSpeed)}</td>
            <td>${statsInfo.active ? formatDuration(statsInfo.uptime_secs) : 'Offline'}</td>
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
    closeClientCrudModal();
}

async function deleteClientAction() {
    const oldMac = document.getElementById('client-old-mac').value;
    if (!oldMac) return;

    if (confirm('Apakah Anda yakin ingin menghapus data klien ini?')) {
        clientsObj.client = clientsObj.client.filter(c => c.mac !== oldMac);
        await saveClientsJson();
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
        tbody.innerHTML = `<tr><td colspan="4" style="text-align: center; color: var(--color-muted);">Tidak ada boot image VHD terdaftar di config.toml.</td></tr>`;
        return;
    }

    tbody.innerHTML = '';
    Object.entries(configObj.image_manager).forEach(([key, path]) => {
        const row = document.createElement('tr');
        row.innerHTML = `
            <td><strong><code>${key}</code></strong></td>
            <td>${path}</td>
            <td id="snapshots-count-${key}">Loading...</td>
            <td>
                <button class="btn btn-small btn-ghost" onclick="openVhdCrudModal('${key}', '${path}')">Edit</button>
                <button class="btn btn-small btn-ghost" onclick="showVhdSnapshots('${key}')">Snapshots</button>
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
        alert("Nama Alias (Image Key) tidak boleh mengandung spasi karena sensitif terhadap format iSCSI IQN!");
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
    closeVhdCrudModal();
}

async function deleteVhdAction() {
    const oldKey = document.getElementById('vhd-old-key').value;
    if (!oldKey) return;

    if (confirm(`Apakah Anda yakin ingin menghapus mapping VHD '${oldKey}'?`)) {
        delete configObj.image_manager[oldKey];
        await saveConfigJsonFull();
        closeVhdCrudModal();
    }
}

// Dynamic Writeback cache list from config.toml
async function loadWritebackFiles() {
    const data = await apiGet('/api/writeback/files');
    const tbody = document.getElementById('writeback-tbody');
    if (!data || data.length === 0) {
        tbody.innerHTML = '<tr><td colspan="4" style="text-align: center; color: var(--color-muted);">Tidak ada file writeback aktif.</td></tr>';
        return;
    }

    tbody.innerHTML = '';
    data.forEach(f => {
        const row = document.createElement('tr');
        row.innerHTML = `
            <td><code>${f.name}</code></td>
            <td>${formatBytes(f.size)}</td>
            <td style="font-size:12px; color:var(--color-muted);">${f.path}</td>
            <td>
                <button class="btn btn-small btn-ghost" style="color:#ef4444; border-color:#ef4444;" onclick="clearWritebackCache('${f.path}')">Hapus</button>
            </td>
        `;
        tbody.appendChild(row);
    });
}

async function clearWritebackCache(path) {
    if (!confirm('Apakah Anda yakin ingin menghapus file cache writeback ini? PC klien harus offline!')) return;
    
    const res = await apiPost('/api/writeback/clear', { file_path: path });
    if (res) {
        loadWritebackFiles();
    }
}

// Central Settings JSON mapping
async function loadConfigJson() {
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
        document.getElementById('set-server-address').value = addrVal;
        document.getElementById('set-server-port').value = data.server.port;
        document.getElementById('set-server-cache').value = data.server.read_cache_gb;
        document.getElementById('set-gamedisk-iqn').value = data.gamedisk_target.target_iqn;

        // DHCP Inputs
        if (data.dhcp) {
            document.getElementById('set-dhcp-enabled').checked = data.dhcp.enabled;
            document.getElementById('set-dhcp-start-ip').value = data.dhcp.start_ip || '';
            document.getElementById('set-dhcp-end-ip').value = data.dhcp.end_ip || '';
            document.getElementById('set-dhcp-mask').value = data.dhcp.subnet_mask || '';
            document.getElementById('set-dhcp-gateway').value = data.dhcp.router || '';
            document.getElementById('set-dhcp-dns').value = data.dhcp.dns || '';
            document.getElementById('set-dhcp-next').value = data.dhcp.next_server || '';
            document.getElementById('set-tftp-dir').value = data.dhcp.tftp_dir || '';
            document.getElementById('set-pxe-default').value = data.dhcp.pxe_default || '';
            serverNicIps = data.dhcp.nic_ips || [];
            renderNicIpsList();
        }

        if (data.writeback) {
            document.getElementById('disk-max-cache-gb').value = data.writeback.max_cache_per_client_gb;
            document.getElementById('disk-max-speed-mbps').value = data.writeback.max_write_speed_mbps;
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
        container.innerHTML = '<span style="color: var(--color-muted);">Memuat disk...</span>';
        const detailList = await apiGet('/api/system/logical_drives_detail');
        if (detailList && Array.isArray(detailList)) {
            systemDrivesList = detailList;
        }
    }

    if (systemDrivesList.length === 0) {
        container.innerHTML = '<span style="color: var(--color-muted);">Gagal mendeteksi drive sistem.</span>';
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
            card.className = 'card disk-card';
            card.style.cssText = 'cursor: pointer; transition: transform 0.15s ease, border-color 0.15s ease; border: 1px solid var(--color-border); padding: 20px; border-radius: 8px; background-color: var(--color-white); text-align: center;';
            
            card.addEventListener('mouseenter', () => {
                card.style.transform = 'translateY(-2px)';
                card.style.borderColor = 'var(--color-text)';
            });
            card.addEventListener('mouseleave', () => {
                card.style.transform = 'translateY(0)';
                card.style.borderColor = 'var(--color-border)';
            });

            let badgeHTML = '';
            if (isBoot) {
                badgeHTML += `<span class="pill-status" style="background-color: #dbeafe; color: #1e40af; border: 1px solid #bfdbfe; font-size: 11px; padding: 2px 8px; margin: 2px; display: inline-block; border-radius: 4px; font-weight: 500;">💿 Boot VHD Storage</span>`;
            }
            if (isWb) {
                badgeHTML += `<span class="pill-status" style="background-color: #fef3c7; color: #92400e; border: 1px solid #fde68a; font-size: 11px; padding: 2px 8px; margin: 2px; display: inline-block; border-radius: 4px; font-weight: 500;">💾 Writeback Cache</span>`;
            }
            if (isGd) {
                badgeHTML += `<span class="pill-status" style="background-color: #f3e8ff; color: #6b21a8; border: 1px solid #e9d5ff; font-size: 11px; padding: 2px 8px; margin: 2px; display: inline-block; border-radius: 4px; font-weight: 500;">🎮 Raw GameDisk</span>`;
            }
            if (badgeHTML === '') {
                badgeHTML = `<span class="pill-status" style="background-color: #f3f4f6; color: #374151; border: 1px solid #e5e7eb; font-size: 11px; padding: 2px 8px; margin: 2px; display: inline-block; border-radius: 4px; font-weight: 500;">Unallocated</span>`;
            }

            card.innerHTML = `
                <div style="font-size: 36px; margin-bottom: 8px;">💽</div>
                <h3 style="margin: 0; font-family: 'Outfit', sans-serif; font-size: 18px;">Disk ${d.letter}:</h3>
                <div style="margin-top: 10px; display: flex; flex-wrap: wrap; justify-content: center; gap: 4px;">
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

    // Reconstruct nested configObj structures
    configObj.server.address = configObj.server.address = document.getElementById('set-server-address').value;
    configObj.server.port = parseInt(document.getElementById('set-server-port').value);
    configObj.server.read_cache_gb = parseInt(document.getElementById('set-server-cache').value);
    configObj.gamedisk_target.target_iqn = document.getElementById('set-gamedisk-iqn').value;

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
    configObj.dhcp.start_ip = document.getElementById('set-dhcp-start-ip').value;
    configObj.dhcp.end_ip = document.getElementById('set-dhcp-end-ip').value || null;
    configObj.dhcp.subnet_mask = document.getElementById('set-dhcp-mask').value;
    configObj.dhcp.router = document.getElementById('set-dhcp-gateway').value;
    configObj.dhcp.dns = document.getElementById('set-dhcp-dns').value;
    configObj.dhcp.next_server = document.getElementById('set-dhcp-next').value;
    configObj.dhcp.tftp_dir = document.getElementById('set-tftp-dir').value;
    configObj.dhcp.pxe_default = document.getElementById('set-pxe-default').value || null;
    
    configObj.dhcp.nic_ips = serverNicIps;

    await saveConfigJsonFull();
}

// Deprecated saveDiskMgmtSettings

async function saveConfigJsonFull() {
    const success = await apiPost('/api/config/json', configObj);
    if (success) {
        await loadConfigJson();
        alert('Pengaturan sukses disimpan!');
    }
}

// Redirect helpers for snapshot details
async function showVhdSnapshots(key) {
    const modal = document.getElementById('vhd-snapshots-modal');
    if (!modal) return;
    modal.style.display = 'flex';
    document.getElementById('snapshot-modal-title-key').textContent = key;

    const tbody = document.getElementById('vhd-snapshots-tbody');
    tbody.innerHTML = '<tr><td colspan="3" style="text-align: center; color: var(--color-muted);">Memuat snapshots...</td></tr>';

    const data = await apiGet(`/api/vhd/backups?image_key=${key}`);
    if (data && Array.isArray(data) && data.length > 0) {
        tbody.innerHTML = '';
        data.forEach(snapshot => {
            const row = document.createElement('tr');
            const filename = snapshot.path.split(/[/\\]/).pop();
            row.innerHTML = `
                <td><strong>Snapshot #${snapshot.index}</strong></td>
                <td style="font-family: monospace; font-size: 12px;" title="${snapshot.path}">${filename}</td>
                <td>
                    <button class="btn btn-small btn-primary" onclick="restoreSnapshotAction('${key}', ${snapshot.index})">🔄 Restore</button>
                </td>
            `;
            tbody.appendChild(row);
        });
    } else {
        tbody.innerHTML = '<tr><td colspan="3" style="text-align: center; color: var(--color-muted);">Tidak ada snapshot (backup) untuk image ini.</td></tr>';
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
        alert(`Sukses merestore VHD '${imageKey}' ke Snapshot #${index}!`);
        closeVhdSnapshotsModal();
        renderVhdTable();
    } else {
        alert('Gagal merestore snapshot.');
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
        pollStats();
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
    closeModal();
    if (!selectedClientForCtx) return;
    
    const endpoint = action === 'commit' ? '/api/superclient/commit' : '/api/superclient/discard';
    await apiPost(endpoint, { hostname: selectedClientForCtx.ip });
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
            tbody.innerHTML = '<tr><td colspan="2" style="text-align: center; color: var(--color-muted);">Tidak ada folder boot loader terdaftar.</td></tr>';
            return;
        }
        tbody.innerHTML = '';
        folders.forEach(f => {
            const row = document.createElement('tr');
            row.innerHTML = `
                <td><strong>${f}</strong></td>
                <td style="text-align: right;">
                    <button class="btn btn-ghost" style="color: #ef4444; border-color: #ef4444; padding: 4px 8px; font-size: 12px;" onclick="deleteTftpFolderAction('${f}')">🗑️ Hapus</button>
                </td>
            `;
            tbody.appendChild(row);
        });
    } else {
        tbody.innerHTML = '<tr><td colspan="2" style="text-align: center; color: var(--color-muted);">Gagal memuat folder TFTP (Periksa konfigurasi TFTP root directory).</td></tr>';
    }
}

async function createNewTftpFolderPrompt() {
    const name = prompt('Nama folder boot loader baru (TFTP):');
    if (!name || name.trim() === '') return;
    
    const success = await apiPost('/api/system/tftp_folders/create', { name: name.trim() });
    if (success) {
        await loadTftpFolders();
        alert('Folder boot loader berhasil dibuat!');
    } else {
        alert('Gagal membuat folder boot loader.');
    }
}

async function deleteTftpFolderAction(name) {
    if (!confirm(`Apakah Anda yakin ingin menghapus folder boot loader "${name}" beserta seluruh file di dalamnya?`)) return;
    
    const success = await apiPost('/api/system/tftp_folders/delete', { name });
    if (success) {
        await loadTftpFolders();
        alert('Folder boot loader berhasil dihapus!');
    } else {
        alert('Gagal menghapus folder boot loader.');
    }
}

let serverNicIps = [];

function renderNicIpsList() {
    const container = document.getElementById('nic-ips-list-container');
    if (!container) return;

    if (serverNicIps.length === 0) {
        container.innerHTML = '<span style="color: var(--color-muted); font-size: 13px; text-align: center;">Belum ada IP ditambahkan.</span>';
        return;
    }

    container.innerHTML = '';
    serverNicIps.forEach((ip, idx) => {
        const row = document.createElement('div');
        row.style.cssText = 'display: flex; justify-content: space-between; align-items: center; background: var(--color-white); padding: 6px 12px; border-radius: 4px; border: 1px solid var(--color-border); font-family: monospace; font-size: 13px;';
        row.innerHTML = `
            <span>🌐 ${ip}</span>
            <button type="button" class="btn btn-small btn-ghost" style="color: #ef4444; border-color: #ef4444; padding: 2px 6px; font-size: 11px; margin: 0;" onclick="removeNicIpAction(${idx})">🗑️ Remove</button>
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
        alert('Format alamat IP tidak valid!');
        return;
    }

    if (serverNicIps.includes(ip)) {
        alert('Alamat IP ini sudah terdaftar!');
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
        alert("Gagal melakukan alokasi otomatis: Silakan isi daftar IP adapter jaringan (Load Balancing NIC IPs) di halaman Pengaturan terlebih dahulu!");
        return;
    }

    if (!clientsObj || !Array.isArray(clientsObj.client) || clientsObj.client.length === 0) {
        alert("Tidak ada klien terdaftar untuk dialokasikan.");
        return;
    }

    if (!confirm(`Apakah Anda yakin ingin membagi ${clientsObj.client.length} klien secara merata (Load Balancing) ke ${nics.length} IP adapter server berikut?\n${nics.join(', ')}`)) {
        return;
    }

    clientsObj.client.forEach((c, index) => {
        c.next_server = nics[index % nics.length];
    });

    await saveClientsJson();
    alert(`Sukses membagi rata ${clientsObj.client.length} klien ke ${nics.length} adapter IP server!`);
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
