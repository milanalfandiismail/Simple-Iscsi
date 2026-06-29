use tokio::net::UdpSocket;
use std::sync::Arc;
use tracing::{info, warn, error, debug};
use std::net::{Ipv4Addr, SocketAddrV4, SocketAddr};
use std::collections::HashMap;
use tokio::sync::Mutex;
use bytes::{BytesMut, BufMut, Buf};
use std::str::FromStr;

use crate::config::{Config, ClientConfig};

const DHCP_MAGIC_COOKIE: [u8; 4] = [99, 130, 83, 99];
const DHCP_SERVER_PORT: u16 = 67;
const DHCP_CLIENT_PORT: u16 = 68;

#[derive(Debug, Clone, PartialEq)]
enum DhcpMessageType {
    Discover = 1,
    Offer = 2,
    Request = 3,
    Decline = 4,
    Ack = 5,
    Nak = 6,
    Release = 7,
    Inform = 8,
    Unknown,
}

impl From<u8> for DhcpMessageType {
    fn from(v: u8) -> Self {
        match v {
            1 => DhcpMessageType::Discover,
            2 => DhcpMessageType::Offer,
            3 => DhcpMessageType::Request,
            4 => DhcpMessageType::Decline,
            5 => DhcpMessageType::Ack,
            6 => DhcpMessageType::Nak,
            7 => DhcpMessageType::Release,
            8 => DhcpMessageType::Inform,
            _ => DhcpMessageType::Unknown,
        }
    }
}

struct DhcpPacket {
    op: u8,
    htype: u8,
    hlen: u8,
    hops: u8,
    xid: u32,
    secs: u16,
    flags: u16,
    ciaddr: Ipv4Addr,
    yiaddr: Ipv4Addr,
    siaddr: Ipv4Addr,
    giaddr: Ipv4Addr,
    chaddr: [u8; 16],
    sname: [u8; 64],
    file: [u8; 128],
    options: HashMap<u8, Vec<u8>>,
}

impl DhcpPacket {
    fn parse(mut buf: &[u8]) -> Option<Self> {
        if buf.len() < 240 {
            return None;
        }

        let op = buf.get_u8();
        let htype = buf.get_u8();
        let hlen = buf.get_u8();
        let hops = buf.get_u8();
        let xid = buf.get_u32();
        let secs = buf.get_u16();
        let flags = buf.get_u16();
        
        let ciaddr = Ipv4Addr::from(buf.get_u32());
        let yiaddr = Ipv4Addr::from(buf.get_u32());
        let siaddr = Ipv4Addr::from(buf.get_u32());
        let giaddr = Ipv4Addr::from(buf.get_u32());
        
        let mut chaddr = [0u8; 16];
        buf.copy_to_slice(&mut chaddr);
        
        let mut sname = [0u8; 64];
        buf.copy_to_slice(&mut sname);
        
        let mut file = [0u8; 128];
        buf.copy_to_slice(&mut file);
        
        let mut magic = [0u8; 4];
        buf.copy_to_slice(&mut magic);
        if magic != DHCP_MAGIC_COOKIE {
            return None; // Not DHCP
        }
        
        let mut options = HashMap::new();
        while buf.has_remaining() {
            let opt_code = buf.get_u8();
            if opt_code == 255 {
                break; // End option
            }
            if opt_code == 0 {
                continue; // Pad
            }
            if buf.has_remaining() {
                let opt_len = buf.get_u8() as usize;
                if buf.remaining() >= opt_len {
                    let mut opt_data = vec![0u8; opt_len];
                    buf.copy_to_slice(&mut opt_data);
                    options.insert(opt_code, opt_data);
                } else {
                    break;
                }
            }
        }

        Some(DhcpPacket {
            op, htype, hlen, hops, xid, secs, flags,
            ciaddr, yiaddr, siaddr, giaddr, chaddr,
            sname, file, options
        })
    }

    fn serialize(&self) -> Vec<u8> {
        let mut buf = BytesMut::with_capacity(1024);
        buf.put_u8(self.op);
        buf.put_u8(self.htype);
        buf.put_u8(self.hlen);
        buf.put_u8(self.hops);
        buf.put_u32(self.xid);
        buf.put_u16(self.secs);
        buf.put_u16(self.flags);
        
        buf.put_slice(&self.ciaddr.octets());
        buf.put_slice(&self.yiaddr.octets());
        buf.put_slice(&self.siaddr.octets());
        buf.put_slice(&self.giaddr.octets());
        
        buf.put_slice(&self.chaddr);
        buf.put_slice(&self.sname);
        buf.put_slice(&self.file);
        
        buf.put_slice(&DHCP_MAGIC_COOKIE);
        
        for (code, data) in &self.options {
            buf.put_u8(*code);
            buf.put_u8(data.len() as u8);
            buf.put_slice(data);
        }
        buf.put_u8(255); // End option
        
        // Pad to at least 300 bytes for some legacy clients
        while buf.len() < 300 {
            buf.put_u8(0);
        }
        
        buf.to_vec()
    }
}

pub struct DhcpServer {
    config: Arc<Config>,
    socket: Arc<UdpSocket>,
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
    pub async fn new(config: Arc<Config>) -> std::io::Result<Arc<Self>> {
        let addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, DHCP_SERVER_PORT);
        let socket = UdpSocket::bind(addr).await?;
        socket.set_broadcast(true)?;
        
        // Parse IPs
        let start_ip = Ipv4Addr::from_str(&config.dhcp.start_ip).unwrap_or(Ipv4Addr::new(10, 10, 10, 100));
        let start_ip_u32 = u32::from_be_bytes(start_ip.octets());

        let mut clients_map = HashMap::new();
        if let Ok(loaded_clients) = crate::config::load_clients("clients.toml") {
            for (_, c) in loaded_clients.clients {
                if let Some(mac_bytes) = parse_mac(&c.mac) {
                    clients_map.insert(mac_bytes, c);
                }
            }
        }

        Ok(Arc::new(DhcpServer {
            config,
            socket: Arc::new(socket),
            leases: Mutex::new(HashMap::new()),
            next_ip: Mutex::new(start_ip_u32),
            clients: Mutex::new(clients_map),
        }))
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
        
        let mac_str = format!("{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}", mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
        
        let mut is_ipxe = false;
        if let Some(user_class) = req.options.get(&77) {
            if String::from_utf8_lossy(user_class).contains("iPXE") {
                is_ipxe = true;
            }
        }
        if req.options.contains_key(&175) {
            is_ipxe = true;
        }

        let mut is_uefi = false;
        if let Some(arch) = req.options.get(&93) {
            if arch.len() >= 2 {
                let arch_type = u16::from_be_bytes([arch[0], arch[1]]);
                if arch_type == 7 || arch_type == 9 {
                    is_uefi = true;
                }
            }
        }

        let mut is_new_client = false;
        let mut client_conf_clone: Option<ClientConfig> = None;

        {
            let mut clients_guard = self.clients.lock().await;
            if let Some(c) = clients_guard.get(&mac) {
                client_conf_clone = Some(c.clone());
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
                    gateway: Some(self.config.dhcp.router.clone()),
                    dns: Some(self.config.dhcp.dns.clone()),
                    pxe: Some("sb-custom".to_string()),
                    bootfile_uefi: None,
                    bootfile_legacy: None,
                    bootfile_ipxe: None,
                    next_server: Some(self.config.dhcp.next_server.clone()),
                    image_manager: None,
                };
                
                clients_guard.insert(mac, new_client.clone());
                client_conf_clone = Some(new_client.clone());
                is_new_client = true;
            }
        }

        if is_new_client {
            if let Some(ref conf) = client_conf_clone {
                if let Err(e) = crate::config::append_client("clients.toml", conf) {
                    error!("Gagal auto-add klien baru ke clients.toml: {}", e);
                } else {
                    info!("Berhasil Auto-Add Klien Baru: {} ({}) -> IP: {}", conf.hostname.as_deref().unwrap_or(""), conf.mac, conf.ip);
                }
            }
        }

        let pxe_folder = client_conf_clone
            .as_ref()
            .and_then(|c| c.pxe.clone())
            .unwrap_or_else(|| "sb-custom".to_string());

        let bootfile = if is_ipxe {
            client_conf_clone.as_ref().and_then(|c| c.bootfile_ipxe.clone())
                .unwrap_or_else(|| format!("{}/autoexec.ipxe", pxe_folder))
        } else if is_uefi {
            client_conf_clone.as_ref().and_then(|c| c.bootfile_uefi.clone())
                .unwrap_or_else(|| format!("{}/ipxe.efi", pxe_folder))
        } else {
            client_conf_clone.as_ref().and_then(|c| c.bootfile_legacy.clone())
                .unwrap_or_else(|| format!("{}/undionly.kpxe", pxe_folder))
        };

        match msg_type {
            DhcpMessageType::Discover => {
                info!("Menerima DHCPDISCOVER dari {} (UEFI: {}, iPXE: {})", mac_str, is_uefi, is_ipxe);
                self.send_offer(req, mac, bootfile, client_conf_clone.as_ref()).await;
            }
            DhcpMessageType::Request => {
                info!("Menerima DHCPREQUEST dari {} (UEFI: {}, iPXE: {})", mac_str, is_uefi, is_ipxe);
                self.send_ack(req, mac, bootfile, client_conf_clone.as_ref()).await;
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
            .unwrap_or_else(|| self.config.dhcp.next_server.clone());
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
            options: HashMap::new(),
        };

        resp.options.insert(54, server_ip.octets().to_vec());
        
        resp.options.insert(53, vec![msg_type.clone() as u8]);
        
        let subnet_str = self.config.dhcp.subnet_mask.clone();
        let subnet = Ipv4Addr::from_str(&subnet_str).unwrap_or(Ipv4Addr::new(255, 255, 255, 0));
        resp.options.insert(1, subnet.octets().to_vec());
        
        let router_str = client_conf.as_ref()
            .and_then(|c| c.gateway.clone())
            .unwrap_or_else(|| self.config.dhcp.router.clone());
        let router = Ipv4Addr::from_str(&router_str).unwrap_or(Ipv4Addr::UNSPECIFIED);
        if router != Ipv4Addr::UNSPECIFIED {
            resp.options.insert(3, router.octets().to_vec());
        }
        
        if let Some(ref hostname) = client_conf.as_ref().and_then(|c| c.hostname.clone()) {
            resp.options.insert(12, hostname.as_bytes().to_vec());
        }
        
        let dns_str = client_conf.as_ref()
            .and_then(|c| c.dns.clone())
            .unwrap_or_else(|| self.config.dhcp.dns.clone());
        let dns = Ipv4Addr::from_str(&dns_str).unwrap_or(Ipv4Addr::UNSPECIFIED);
        if dns != Ipv4Addr::UNSPECIFIED {
            resp.options.insert(6, dns.octets().to_vec());
        }
        
        resp.options.insert(51, 86400u32.to_be_bytes().to_vec());

        let mut bootfile_vec = bootfile.as_bytes().to_vec();
        bootfile_vec.push(0);
        resp.options.insert(67, bootfile_vec);
        
        let mut next_server_bytes = next_server_str.as_bytes().to_vec();
        next_server_bytes.push(0);
        resp.options.insert(66, next_server_bytes);

        let is_broadcast = (req.flags & 0x8000) != 0;
        let packet = resp.serialize();
        
        let server_ip = Ipv4Addr::from_str(&self.config.dhcp.next_server).unwrap_or(Ipv4Addr::UNSPECIFIED);
        let subnet = Ipv4Addr::from_str(&self.config.dhcp.subnet_mask).unwrap_or(Ipv4Addr::new(255, 255, 255, 0));
        
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

        if let Err(e) = self.socket.send_to(&packet, dest).await {
            error!("Gagal mengirim DHCP Reply ke {}: {}", dest, e);
        } else {
            // Fallback to global broadcast just in case
            let backup_dest = SocketAddrV4::new(Ipv4Addr::BROADCAST, DHCP_CLIENT_PORT);
            if dest.ip() != &Ipv4Addr::BROADCAST {
                let _ = self.socket.send_to(&packet, backup_dest).await;
            }
            info!("Sukses mengirim {:?} ke {} (Bootfile: {})", msg_type, dest, bootfile);
        }
    }
}
