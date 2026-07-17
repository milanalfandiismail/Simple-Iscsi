use std::net::Ipv4Addr;
use std::collections::BTreeMap;
use bytes::{BytesMut, BufMut, Buf};

pub const DHCP_MAGIC_COOKIE: [u8; 4] = [99, 130, 83, 99];
pub const DHCP_SERVER_PORT: u16 = 67;
pub const DHCP_CLIENT_PORT: u16 = 68;

#[derive(Debug, Clone, PartialEq)]
pub enum DhcpMessageType {
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

pub struct DhcpPacket {
    pub op: u8,
    pub htype: u8,
    pub hlen: u8,
    pub hops: u8,
    pub xid: u32,
    pub secs: u16,
    pub flags: u16,
    pub ciaddr: Ipv4Addr,
    pub yiaddr: Ipv4Addr,
    pub siaddr: Ipv4Addr,
    pub giaddr: Ipv4Addr,
    pub chaddr: [u8; 16],
    pub sname: [u8; 64],
    pub file: [u8; 128],
    pub options: BTreeMap<u8, Vec<u8>>,
}

impl DhcpPacket {
    pub fn parse(mut buf: &[u8]) -> Option<Self> {
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
        
        let mut options = BTreeMap::new();
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

    pub fn serialize(&self) -> Vec<u8> {
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
