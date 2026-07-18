use tokio::net::UdpSocket;
use std::sync::Arc;
use tracing::{info, warn, error, debug};
use std::net::{Ipv4Addr, SocketAddrV4, SocketAddr};
use std::collections::{HashMap, BTreeMap};
use tokio::sync::Mutex;
use bytes::Buf;
use std::str::FromStr;
use socket2::{Socket, Domain, Type, Protocol};

use crate::config::ClientConfig;
use crate::config_manager::SharedConfig;

use crate::netboot::dhcp_packet::*;

pub struct DhcpServer {
    config: SharedConfig,
    stats: Arc<crate::stats::ServerStats>,
    socket: Arc<UdpSocket>,
    sender: Arc<UdpSocket>,
    leases: Mutex<HashMap<[u8; 6], Ipv4Addr>>,
    next_ip: Mutex<u32>,
    clients: Mutex<HashMap<[u8; 6], ClientConfig>>,
}

fn parse_mac(mac: &str) -> Option<[u8; 6]> {
    let parts: Vec<&str> = mac.split(':').collect();
    if parts.len() != 6 { return None; }
    let mut bytes = [0u8; 6];
    for (i, p) in parts.iter().enumerate() {
        bytes[i] = u8::from_str_radix(p, 16).ok()?;
    }
    Some(bytes)
}

impl DhcpServer {
    pub async fn new(config: SharedConfig, stats: Arc<crate::stats::ServerStats>) -> std::io::Result<Arc<Self>> {
        let current_config = config.read();
        let addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, DHCP_SERVER_PORT);
        let socket = UdpSocket::bind(addr).await?;
        socket.set_broadcast(true)?;
        let socket_arc = Arc::new(socket);

        // Create a dedicated sender socket bound to the server IP:67
        // Uses SO_REUSEADDR so two sockets can share port 67
        // UEFI PXE firmware requires replies from source port 67
        let server_addr = Ipv4Addr::from_str(&current_config.server.address.as_vec().first().cloned().unwrap_or_default())
            .unwrap_or(Ipv4Addr::UNSPECIFIED);

        let sender = if server_addr.is_unspecified() {
            info!("DHCP Server: Server address is unspecified (0.0.0.0). Reusing receiver socket for sender.");
            socket_arc.clone()
        } else {
            let sock2 = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
            sock2.set_reuse_address(true)?;
            sock2.set_broadcast(true)?;
            let sender_addr: std::net::SocketAddr = SocketAddrV4::new(server_addr, DHCP_SERVER_PORT).into();
            match sock2.bind(&sender_addr.into()) {
                Ok(_) => {
                    let s = UdpSocket::from_std(sock2.into())?;
                    info!("Sender DHCP socket bound to {}:{}", server_addr, DHCP_SERVER_PORT);
                    Arc::new(s)
                }
                Err(e) => {
                    warn!("Failed to bind dedicated DHCP sender socket to {}:{}: {}. Falling back to receiver socket.", server_addr, DHCP_SERVER_PORT, e);
                    socket_arc.clone()
                }
            }
        };

        // Parse IPs
        let start_ip = Ipv4Addr::from_str(&current_config.dhcp.as_ref().unwrap().start_ip).unwrap_or(Ipv4Addr::new(10, 10, 10, 100));
        let start_ip_u32 = u32::from_be_bytes(start_ip.octets());

        let mut clients_map = HashMap::new();
        if let Ok(loaded_clients) = crate::config::load_clients("clients.toml") {
            for (_, c) in loaded_clients {
                if let Some(mac_bytes) = parse_mac(&c.mac) {
                    clients_map.insert(mac_bytes, c);
                }
            }
        }

        let server = Arc::new(DhcpServer {
            config,
            stats,
            socket: socket_arc,
            sender,
            leases: Mutex::new(HashMap::new()),
            next_ip: Mutex::new(start_ip_u32),
            clients: Mutex::new(clients_map),
        });

        // Spawn clients.toml watcher
        {
            let server_clone = server.clone();
            tokio::spawn(async move {
                let mut last_mtime = std::fs::metadata("clients.toml").and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
                loop {
                    interval.tick().await;
                    let current_mtime = std::fs::metadata("clients.toml").and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    if current_mtime != last_mtime {
                        let loaded = crate::config::load_clients("clients.toml").ok();
                        if let Some(loaded_clients) = loaded {
                            let mut new_map = HashMap::new();
                            for (_, c) in loaded_clients {
                                if let Some(mac_bytes) = parse_mac(&c.mac) {
                                    new_map.insert(mac_bytes, c);
                                }
                            }
                            *server_clone.clients.lock().await = new_map;
                            info!("DhcpServer: clients.toml di-reload.");
                        }
                        last_mtime = current_mtime;
                    }
                }
            });
        }

        Ok(server)
    }

    async fn allocate_ip(&self, mac: &[u8; 6], client_conf: Option<&ClientConfig>) -> Ipv4Addr {
        if let Some(c) = client_conf {
            let static_ip_str = &c.ip;
            if let Ok(ip) = Ipv4Addr::from_str(static_ip_str) {
                return ip;
            } else {
                warn!("IP statis tidak valid di clients.toml untuk MAC: {:?}", mac);
            }
        }

        let mut leases = self.leases.lock().await;
        if let Some(ip) = leases.get(mac) {
            return *ip;
        }

        let mut next = self.next_ip.lock().await;
        let ip = Ipv4Addr::from(*next);
        *next += 1;
        
        leases.insert(*mac, ip);

        let mac_str = format!("{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
        self.stats.dhcp_leases.insert(mac_str, ip.to_string());

        ip
    }

    pub async fn run(self: Arc<Self>) {
        info!("Memulai DHCP Server di 0.0.0.0:67...");
        let mut buf = [0u8; 2048];
        
        loop {
            match self.socket.recv_from(&mut buf).await {
                Ok((len, addr)) => {
                    if let Some(packet) = DhcpPacket::parse(&buf[..len]) {
                        self.handle_packet(packet, addr).await;
                    }
                }
                Err(e) => {
                    error!("Error menerima DHCP packet: {}", e);
                }
            }
        }
    }

    async fn handle_packet(&self, req: DhcpPacket, _src_addr: SocketAddr) {
        let msg_type = req.options.get(&53).and_then(|data| data.first()).map(|&v| DhcpMessageType::from(v)).unwrap_or(DhcpMessageType::Unknown);
        
        let mut mac = [0u8; 6];
        mac.copy_from_slice(&req.chaddr[0..6]);

        let mac_str = format!("{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);

        let (is_new_client, client_conf) = {
            let mut clients_guard = self.clients.lock().await;
            if let Some(c) = clients_guard.get(&mac) {
                (false, Some(c.clone()))
            } else {
                let pc_count = clients_guard.len() + 1;
                let hostname = format!("PC-{:02}", pc_count);

                let mut next = self.next_ip.lock().await;
                let ip_addr = Ipv4Addr::from(*next);
                *next += 1;

                let new_client = ClientConfig {
                    hostname: Some(hostname),
                    mac: mac_str.clone(),
                    ip: ip_addr.to_string(),
                    gateway: Some(self.config.read().dhcp.as_ref().unwrap().router.clone()),
                    dns: Some(self.config.read().dhcp.as_ref().unwrap().dns.clone()),
                    pxe: Some("sb-custom".to_string()),
                    bootfile_uefi: None,
                    bootfile_legacy: None,
                    bootfile_ipxe: None,
                    next_server: Some(self.config.read().dhcp.as_ref().unwrap().next_server.clone()),
                    image_manager: None,
                };

                clients_guard.insert(mac, new_client.clone());
                (true, Some(new_client))
            }
        };

        if is_new_client {
            if let Some(ref conf) = client_conf {
                if let Err(e) = crate::config::append_client("clients.toml", conf) {
                    error!("Gagal auto-add klien baru ke clients.toml: {}", e);
                } else {
                    info!("Berhasil Auto-Add Klien Baru: {} ({}) -> IP: {}", conf.hostname.as_deref().unwrap_or(""), conf.mac, conf.ip);
                }
            }
        }

        // ─── Bootfile selection ───────────────────────────────────────
        // client_arch dari DHCP option 93 (2-byte big-endian)
        // 0x0000 = BIOS x86 (Legacy), 0x0006 = UEFI x86
        // 0x0007 = UEFI x64, 0x0009 = UEFI x64 w/PXE, 0x000D = BIOS w/UEFI BC
        let client_arch = req.options.get(&93).and_then(|v| {
            if v.len() >= 2 {
                Some(u16::from_be_bytes([v[0], v[1]]))
            } else {
                None
            }
        });
        let has_opt_175 = req.options.contains_key(&175);
        let c = client_conf.as_ref();
        let default_bf = self.config.read().dhcp.as_ref().unwrap().pxe_default.as_deref().unwrap_or("sb-custom").to_string();

        let bootfile = match client_arch {
            // Legacy BIOS
            Some(0x0000) | Some(0x000D) => c
                .and_then(|c| c.bootfile_legacy.as_ref())
                .filter(|s| !s.is_empty())
                .or_else(|| c.and_then(|c| c.pxe.as_ref()))
                .cloned()
                .unwrap_or_else(|| default_bf.to_string()),

            // UEFI
            Some(0x0006) | Some(0x0007) | Some(0x0009) => {
                let ipxe_bf = c
                    .and_then(|c| c.bootfile_ipxe.as_ref())
                    .filter(|s| !s.is_empty());
                if has_opt_175 && ipxe_bf.is_some() {
                    ipxe_bf.cloned().unwrap()
                } else {
                    let uefi_bf = c
                        .and_then(|c| c.bootfile_uefi.as_ref())
                        .filter(|s| !s.is_empty());
                    if let Some(bf) = uefi_bf {
                        bf.clone()
                    } else {
                        let pxe_dir = c.and_then(|c| c.pxe.as_ref()).cloned()
                            .unwrap_or_else(|| default_bf.to_string());
                        format!("{}/ipxe-shim.efi", pxe_dir)
                    }
                }
            }

            // Unknown → fallback legacy
            _ => c
                .and_then(|c| c.bootfile_legacy.as_ref())
                .filter(|s| !s.is_empty())
                .or_else(|| c.and_then(|c| c.pxe.as_ref()))
                .cloned()
                .unwrap_or_else(|| default_bf.to_string()),
        };

        info!(
            "DHCP {:?} dari {} arch={:?} opt175={} flags=0x{:04X} bootfile={}",
            msg_type, mac_str, client_arch, has_opt_175, req.flags, bootfile
        );

        match msg_type {
            DhcpMessageType::Discover => {
                self.send_offer(req, mac, bootfile, client_conf.as_ref()).await;
            }
            DhcpMessageType::Request => {
                self.send_ack(req, mac, bootfile, client_conf.as_ref()).await;
            }
            _ => {
                debug!("Mengabaikan tipe DHCP: {:?}", msg_type);
            }
        }
    }

    async fn send_offer(&self, req: DhcpPacket, mac: [u8; 6], bootfile: String, client_conf: Option<&ClientConfig>) {
        let yiaddr = self.allocate_ip(&mac, client_conf).await;
        self.send_reply(req, mac, DhcpMessageType::Offer, yiaddr, bootfile, client_conf.cloned()).await;
    }

    async fn send_ack(&self, req: DhcpPacket, mac: [u8; 6], bootfile: String, client_conf: Option<&ClientConfig>) {
        let yiaddr = self.allocate_ip(&mac, client_conf).await;
        self.send_reply(req, mac, DhcpMessageType::Ack, yiaddr, bootfile, client_conf.cloned()).await;
    }

    async fn send_reply(&self, req: DhcpPacket, _mac: [u8; 6], msg_type: DhcpMessageType, yiaddr: Ipv4Addr, bootfile: String, client_conf: Option<ClientConfig>) {
        let mut file_buf = [0u8; 128];
        let bytes = bootfile.as_bytes();
        let len = std::cmp::min(bytes.len(), 127);
        file_buf[..len].copy_from_slice(&bytes[..len]);

        let next_server_str = client_conf.as_ref()
            .and_then(|c| c.next_server.clone())
            .unwrap_or_else(|| self.config.read().dhcp.as_ref().unwrap().next_server.clone());
        let server_ip = Ipv4Addr::from_str(&next_server_str).unwrap_or(Ipv4Addr::UNSPECIFIED);

        let mut resp = DhcpPacket {
            op: 2, // Reply
            htype: req.htype,
            hlen: req.hlen,
            hops: 0,
            xid: req.xid,
            secs: 0,
            flags: req.flags,
            ciaddr: req.ciaddr,
            yiaddr,
            siaddr: server_ip,
            giaddr: req.giaddr,
            chaddr: req.chaddr,
            sname: [0u8; 64],
            file: file_buf,
            options: BTreeMap::new(),
        };

        resp.options.insert(53, vec![msg_type.clone() as u8]);

        // Option 54: Server Identifier — IP DHCP server
        let dhcp_server_ip = {
            let ip = Ipv4Addr::from_str(&self.config.read().server.address.as_vec().first().cloned().unwrap_or_default()).unwrap_or(Ipv4Addr::UNSPECIFIED);
            if ip.is_unspecified() {
                server_ip
            } else {
                ip
            }
        };
        resp.options.insert(54, dhcp_server_ip.octets().to_vec());

        // Option 93: Echo-back Client System Architecture (required by EDK2 UEFI)
        if let Some(arch_data) = req.options.get(&93) {
            resp.options.insert(93, arch_data.clone());
        }

        let subnet_str = self.config.read().dhcp.as_ref().unwrap().subnet_mask.clone();
        let subnet = Ipv4Addr::from_str(&subnet_str).unwrap_or(Ipv4Addr::new(255, 255, 255, 0));
        resp.options.insert(1, subnet.octets().to_vec());

        let router_str = client_conf.as_ref()
            .and_then(|c| c.gateway.clone())
            .unwrap_or_else(|| self.config.read().dhcp.as_ref().unwrap().router.clone());
        let router = Ipv4Addr::from_str(&router_str).unwrap_or(Ipv4Addr::UNSPECIFIED);
        if router != Ipv4Addr::UNSPECIFIED {
            resp.options.insert(3, router.octets().to_vec());
        }

        if let Some(ref hostname) = client_conf.as_ref().and_then(|c| c.hostname.clone()) {
            resp.options.insert(12, hostname.as_bytes().to_vec());
        }

        let dns_str = client_conf.as_ref()
            .and_then(|c| c.dns.clone())
            .unwrap_or_else(|| self.config.read().dhcp.as_ref().unwrap().dns.clone());
        let dns = Ipv4Addr::from_str(&dns_str).unwrap_or(Ipv4Addr::UNSPECIFIED);
        if dns != Ipv4Addr::UNSPECIFIED {
            resp.options.insert(6, dns.octets().to_vec());
        }

        // Lease time: 86400 detik (24 jam)
        resp.options.insert(51, 86400u32.to_be_bytes().to_vec());
        // Renewal time (T1): 43200 (12 jam)
        resp.options.insert(58, 43200u32.to_be_bytes().to_vec());
        // Rebinding time (T2): 75600 (21 jam)
        resp.options.insert(59, 75600u32.to_be_bytes().to_vec());

        let mut bootfile_vec = bootfile.as_bytes().to_vec();
        bootfile_vec.push(0);
        resp.options.insert(67, bootfile_vec);

        let mut next_server_bytes = next_server_str.as_bytes().to_vec();
        next_server_bytes.push(0);
        resp.options.insert(66, next_server_bytes);

        // Option 168: iSCSI Target IP (iPXE reads as ${168})
        resp.options.insert(168, next_server_str.as_bytes().to_vec());

        // Option 169: iSCSI Target IQN (iPXE reads as ${169})
        let image_name = client_conf.as_ref()
            .and_then(|c| c.image_manager.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                self.config.read().image_manager.as_ref()
                    .and_then(|m| m.keys().next())
                    .cloned()
                    .unwrap_or_else(|| "windows_10".to_string())
            });
        let iscsi_iqn = if let Some(ref win) = self.config.read().windows {
            format!("{}{}", win.target_iqn_prefix, image_name)
        } else {
            image_name.clone()
        };
        resp.options.insert(169, iscsi_iqn.as_bytes().to_vec());

        let is_broadcast = (req.flags & 0x8000) != 0;
        let packet = resp.serialize();

        // Hex dump first 48 bytes of packet for debugging
        let hex_dump: String = packet.iter().take(48).enumerate().map(|(i, b)| {
            format!("{:02x}{}", b, if (i + 1) % 16 == 0 { "\n" } else { " " })
        }).collect();
        info!("DHCP {:?} packet hex dump (first 48 bytes):\n{}", msg_type, hex_dump);

        let server_ip = Ipv4Addr::from_str(&self.config.read().dhcp.as_ref().unwrap().next_server).unwrap_or(Ipv4Addr::UNSPECIFIED);
        let subnet = Ipv4Addr::from_str(&self.config.read().dhcp.as_ref().unwrap().subnet_mask).unwrap_or(Ipv4Addr::new(255, 255, 255, 0));
        
        // Calculate Subnet Broadcast Address (e.g., 10.10.10.255)
        let s_oct = server_ip.octets();
        let m_oct = subnet.octets();
        let broadcast_ip = Ipv4Addr::new(
            s_oct[0] | (!m_oct[0]),
            s_oct[1] | (!m_oct[1]),
            s_oct[2] | (!m_oct[2]),
            s_oct[3] | (!m_oct[3]),
        );

        let dest = if is_broadcast {
            SocketAddrV4::new(broadcast_ip, DHCP_CLIENT_PORT)
        } else {
            if req.giaddr != Ipv4Addr::UNSPECIFIED {
                SocketAddrV4::new(req.giaddr, DHCP_SERVER_PORT)
            } else {
                SocketAddrV4::new(yiaddr, DHCP_CLIENT_PORT)
            }
        };

        if let Err(e) = self.sender.send_to(&packet, dest).await {
            error!("Gagal mengirim DHCP Reply ke {}: {}", dest, e);
        } else {
            // Also send to global broadcast as fallback (for multi-homed setups)
            let backup_dest = SocketAddrV4::new(Ipv4Addr::BROADCAST, DHCP_CLIENT_PORT);
            if dest.ip() != &Ipv4Addr::BROADCAST {
                let _ = self.sender.send_to(&packet, backup_dest).await;
            }
            info!("Sukses mengirim {:?} ke {} (Bootfile: {})", msg_type, dest, bootfile);
        }
    }
}
