use tokio::net::UdpSocket;
use std::sync::Arc;
use tracing::{info, warn, error, debug};
use std::net::{Ipv4Addr, SocketAddrV4, SocketAddr};
use std::path::Path;
use bytes::{BytesMut, BufMut, Buf};

use crate::config::Config;
use crate::config_manager::SharedConfig;

const TFTP_PORT: u16 = 69;
const TFTP_OP_RRQ: u16 = 1;
const TFTP_OP_DATA: u16 = 3;
const TFTP_OP_ACK: u16 = 4;
const TFTP_OP_ERROR: u16 = 5;
const TFTP_OP_OACK: u16 = 6;

pub struct TftpServer {
    config: SharedConfig,
    socket: Arc<UdpSocket>,
}

impl TftpServer {
    pub async fn new(config: SharedConfig) -> std::io::Result<Arc<Self>> {
        let addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, TFTP_PORT);
        let socket = UdpSocket::bind(addr).await?;
        
        Ok(Arc::new(TftpServer {
            config,
            socket: Arc::new(socket),
        }))
    }

    pub async fn run(self: Arc<Self>) {
        info!("Memulai TFTP Server di 0.0.0.0:69 (dir: {})...", self.config.read().dhcp.as_ref().unwrap().tftp_dir);
        let mut buf = [0u8; 2048];
        
        loop {
            match self.socket.recv_from(&mut buf).await {
                Ok((len, addr)) => {
                    let data = buf[..len].to_vec();
                    let server = Arc::clone(&self);
                    
                    tokio::spawn(async move {
                        server.handle_request(data, addr).await;
                    });
                }
                Err(e) => {
                    error!("Error menerima TFTP packet: {}", e);
                }
            }
        }
    }

    async fn handle_request(&self, data: Vec<u8>, addr: SocketAddr) {
        if data.len() < 4 {
            return;
        }

        let mut buf = &data[..];
        let opcode = buf.get_u16();

        if opcode != TFTP_OP_RRQ {
            debug!("Hanya RRQ (Read Request) yang didukung. Opcode: {}", opcode);
            self.send_error(addr, 4, "Illegal TFTP operation.").await;
            return;
        }

        // Parse filename
        let mut filename_bytes = Vec::new();
        while buf.has_remaining() {
            let b = buf.get_u8();
            if b == 0 { break; }
            filename_bytes.push(b);
        }
        let filename = String::from_utf8_lossy(&filename_bytes).to_string();
        
        // Parse mode
        let mut mode_bytes = Vec::new();
        while buf.has_remaining() {
            let b = buf.get_u8();
            if b == 0 { break; }
            mode_bytes.push(b);
        }
        
        // Parse options
        let mut options: Vec<(String, String)> = Vec::new();
        while buf.has_remaining() {
            let mut opt_name = Vec::new();
            while buf.has_remaining() {
                let b = buf.get_u8();
                if b == 0 { break; }
                opt_name.push(b);
            }
            if opt_name.is_empty() { break; }
            
            let mut opt_val = Vec::new();
            while buf.has_remaining() {
                let b = buf.get_u8();
                if b == 0 { break; }
                opt_val.push(b);
            }
            
            let key = String::from_utf8_lossy(&opt_name).to_lowercase();
            let val = String::from_utf8_lossy(&opt_val).to_string();
            options.push((key, val));
        }

        info!("TFTP RRQ dari {}: meminta file '{}' ({} options)", addr, filename, options.len());

        // Path Sanitization
        if filename.contains("..") {
            warn!("TFTP Path Traversal terdeteksi dari {}", addr);
            self.send_error(addr, 2, "Access violation.").await;
            return;
        }

        let clean_filename = filename.replace("/", "\\");
        let base_dir_str = self.config.read().dhcp.as_ref().unwrap().tftp_dir.clone();
        let base_dir = Path::new(&base_dir_str);
        let full_path = base_dir.join(clean_filename);

        let file_data = match std::fs::read(&full_path) {
            Ok(data) => data,
            Err(e) => {
                warn!("TFTP File tidak ditemukan: {:?} ({})", full_path, e);
                self.send_error(addr, 1, "File not found.").await;
                return;
            }
        };

        debug!("Mulai transfer file {:?} ({} bytes) ke {}", full_path, file_data.len(), addr);
        self.transfer_file(addr, file_data, options).await;
    }

    async fn transfer_file(&self, client_addr: SocketAddr, file_data: Vec<u8>, options: Vec<(String, String)>) {
        let bind_ip = self.config.read().server.address.as_vec().first().cloned().unwrap_or_else(|| "0.0.0.0".to_string());
        let bind_addr = format!("{}:0", bind_ip);
        
        let socket = match UdpSocket::bind(&bind_addr).await {
            Ok(s) => s,
            Err(e) => {
                error!("Gagal bind ephemeral port untuk TFTP di {}: {}", bind_addr, e);
                return;
            }
        };

        let mut blksize: usize = 512;
        let mut send_oack = false;
        let mut oack_packet = BytesMut::with_capacity(512);
        oack_packet.put_u16(TFTP_OP_OACK);

        for (opt, val) in options {
            if opt == "blksize" {
                if let Ok(size) = val.parse::<usize>() {
                    blksize = size.clamp(512, 1468);
                    send_oack = true;
                    oack_packet.put_slice(b"blksize\0");
                    oack_packet.put_slice(blksize.to_string().as_bytes());
                    oack_packet.put_u8(0);
                }
            } else if opt == "tsize" {
                send_oack = true;
                oack_packet.put_slice(b"tsize\0");
                oack_packet.put_slice(file_data.len().to_string().as_bytes());
                oack_packet.put_u8(0);
            }
        }

        let mut recv_buf = vec![0u8; blksize + 100]; // Buffer for receiving ACKs

        // Jika ada opsi, kirim OACK dan tunggu ACK(0)
        if send_oack {
            let mut retries = 0;
            let mut acked = false;

            while retries < 3 && !acked {
                if let Err(e) = socket.send_to(&oack_packet, client_addr).await {
                    error!("Gagal mengirim TFTP OACK: {}", e);
                    return;
                }

                let timeout = tokio::time::timeout(std::time::Duration::from_secs(2), socket.recv_from(&mut recv_buf)).await;
                match timeout {
                    Ok(Ok((len, peer_addr))) => {
                        if peer_addr == client_addr && len >= 4 {
                            let mut ack_buf = &recv_buf[..len];
                            let ack_op = ack_buf.get_u16();
                            let ack_block = ack_buf.get_u16();

                            if ack_op == TFTP_OP_ACK && ack_block == 0 {
                                acked = true;
                            } else if ack_op == TFTP_OP_ERROR {
                                error!("TFTP client mengirim Error: kode {}", ack_block);
                                return;
                            }
                        }
                    }
                    Ok(Err(e)) => error!("TFTP Socket error: {}", e),
                    Err(_) => {
                        warn!("TFTP OACK timeout, retry...");
                        retries += 1;
                    }
                }
            }
            if !acked {
                error!("TFTP transfer gagal (OACK timeout) ke {}", client_addr);
                return;
            }
        }

        let mut block_num: u16 = 1;
        let mut offset = 0;

        loop {
            let end = std::cmp::min(offset + blksize, file_data.len());
            let chunk = &file_data[offset..end];

            let mut packet = BytesMut::with_capacity(blksize + 4);
            packet.put_u16(TFTP_OP_DATA);
            packet.put_u16(block_num);
            packet.put_slice(chunk);

            let mut retries = 0;
            let mut acked = false;

            while retries < 3 && !acked {
                if let Err(e) = socket.send_to(&packet, client_addr).await {
                    error!("Gagal mengirim TFTP DATA blok {}: {}", block_num, e);
                    return;
                }

                let timeout = tokio::time::timeout(std::time::Duration::from_secs(2), socket.recv_from(&mut recv_buf)).await;
                
                match timeout {
                    Ok(Ok((len, peer_addr))) => {
                        if peer_addr == client_addr && len >= 4 {
                            let mut ack_buf = &recv_buf[..len];
                            let ack_op = ack_buf.get_u16();
                            let ack_block = ack_buf.get_u16();

                            if ack_op == TFTP_OP_ACK && ack_block == block_num {
                                acked = true;
                            } else if ack_op == TFTP_OP_ERROR {
                                error!("TFTP client mengirim Error: kode {}", ack_block);
                                return;
                            }
                        }
                    }
                    Ok(Err(e)) => error!("TFTP Socket error: {}", e),
                    Err(_) => {
                        warn!("TFTP ACK timeout untuk blok {}, retry...", block_num);
                        retries += 1;
                    }
                }
            }

            if !acked {
                error!("TFTP transfer gagal (timeout) ke {}", client_addr);
                return;
            }

            if chunk.len() < blksize {
                info!("TFTP transfer selesai ke {} (Total: {} bytes)", client_addr, file_data.len());
                break;
            }

            block_num = block_num.wrapping_add(1);
            offset += blksize;
        }
    }

    async fn send_error(&self, addr: SocketAddr, error_code: u16, msg: &str) {
        let bind_ip = self.config.read().server.address.as_vec().first().cloned().unwrap_or_else(|| "0.0.0.0".to_string());
        if let Ok(socket) = UdpSocket::bind(format!("{}:0", bind_ip)).await {
            let mut packet = BytesMut::with_capacity(100);
            packet.put_u16(TFTP_OP_ERROR);
            packet.put_u16(error_code);
            packet.put_slice(msg.as_bytes());
            packet.put_u8(0);
            
            let _ = socket.send_to(&packet, addr).await;
        }
    }
}
