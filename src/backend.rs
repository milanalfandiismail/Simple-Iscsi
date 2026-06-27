use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;
use parking_lot::Mutex;
use tracing::{info, error};

use crate::vhd::VhdBackend;

/// 1MB read-ahead buffer
const RA_SIZE: usize = 1024 * 1024;

enum BackendType {
    RawDisk(File),
    Vhd(VhdBackend),
}

impl BackendType {
    fn read_exact_at(&mut self, lba: u64, block_size: u64, buf: &mut [u8]) -> io::Result<()> {
        match self {
            BackendType::RawDisk(ref mut file) => {
                let offset = lba * block_size;
                file.seek(SeekFrom::Start(offset))?;
                file.read_exact(buf)
            }
            BackendType::Vhd(ref mut vhd) => {
                let mut buf_offset = 0;
                let mut current_byte_offset = lba * block_size;
                
                while buf_offset < buf.len() {
                    let vhd_block_idx = current_byte_offset / (vhd.vhd_block_size as u64);
                    let offset_in_vhd_block = current_byte_offset % (vhd.vhd_block_size as u64);
                    
                    let bytes_to_read = std::cmp::min(
                        buf.len() - buf_offset,
                        (vhd.vhd_block_size as u64 - offset_in_vhd_block) as usize
                    );

                    let bat_val = *vhd.bat.get(vhd_block_idx as usize).unwrap_or(&0xFFFFFFFF);
                    if bat_val == 0xFFFFFFFF {
                        // Unallocated block -> zeros
                        for i in 0..bytes_to_read {
                            buf[buf_offset + i] = 0;
                        }
                    } else {
                        let physical_offset = (bat_val as u64) * 512 + (vhd.sector_bitmap_size as u64) + offset_in_vhd_block;
                        vhd.file.seek(SeekFrom::Start(physical_offset))?;
                        vhd.file.read_exact(&mut buf[buf_offset..buf_offset + bytes_to_read])?;
                    }
                    buf_offset += bytes_to_read;
                    current_byte_offset += bytes_to_read as u64;
                }
                Ok(())
            }
        }
    }
}

struct BackendInner {
    backend: BackendType,
    ra_buf: Vec<u8>,
    ra_lba: u64,
    ra_blocks: u32,
}

pub struct Backend {
    inner: Arc<Mutex<BackendInner>>,
    block_size: u64,
    total_size: u64,
    total_blocks: u64,
    pub vendor_id: String,
    pub product_id: String,
    pub product_revision: String,
}

impl Backend {
    pub fn new_raw(path: &str, block_size: u64, vendor: &str, product: &str, rev: &str) -> io::Result<Self> {
        info!("Membuka storage backend raw: {}", path);
        let mut options = std::fs::OpenOptions::new();
        options.read(true);

        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1 | 2);
        }

        let file = match options.open(path) {
            Ok(f) => f,
            Err(e) => {
                error!("Gagal membuka storage backend di {:?}: {}", path, e);
                return Err(e);
            }
        };

        let total_size = match file.metadata().map(|m| m.len()) {
            Ok(len) if len > 0 => len,
            _ => {
                #[cfg(windows)]
                {
                    match get_windows_drive_size(&file) {
                        Ok(len) => len,
                        Err(e) => {
                            error!("Gagal menentukan ukuran drive: {}", e);
                            return Err(e);
                        }
                    }
                }
                #[cfg(not(windows))]
                {
                    // Fallback using seek
                    let mut f = file.try_clone()?;
                    let pos = f.seek(SeekFrom::End(0))?;
                    f.seek(SeekFrom::Start(0))?;
                    pos
                }
            }
        };

        info!("Storage raw dibuka. Ukuran: {} byte", total_size);

        let inner = BackendInner {
            backend: BackendType::RawDisk(file),
            ra_buf: vec![0u8; RA_SIZE],
            ra_lba: u64::MAX,
            ra_blocks: 0,
        };

        Ok(Backend {
            inner: Arc::new(Mutex::new(inner)),
            block_size,
            total_size,
            total_blocks: total_size / block_size,
            vendor_id: vendor.to_string(),
            product_id: product.to_string(),
            product_revision: rev.to_string(),
        })
    }

    pub fn new_vhd(path: &str, block_size: u64, vendor: &str, product: &str, rev: &str) -> io::Result<Self> {
        info!("Membuka storage backend VHD: {}", path);
        let mut options = std::fs::OpenOptions::new();
        options.read(true);

        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1 | 2);
        }

        let file = match options.open(path) {
            Ok(f) => f,
            Err(e) => {
                error!("Gagal membuka VHD di {:?}: {}", path, e);
                return Err(e);
            }
        };

        let vhd = VhdBackend::open(file)?;
        let total_size = vhd.current_size;

        info!("VHD backend dibuka. Ukuran: {} byte", total_size);

        let inner = BackendInner {
            backend: BackendType::Vhd(vhd),
            ra_buf: vec![0u8; RA_SIZE],
            ra_lba: u64::MAX,
            ra_blocks: 0,
        };

        Ok(Backend {
            inner: Arc::new(Mutex::new(inner)),
            block_size,
            total_size,
            total_blocks: total_size / block_size,
            vendor_id: vendor.to_string(),
            product_id: product.to_string(),
            product_revision: rev.to_string(),
        })
    }

    pub fn block_size(&self) -> u64 {
        self.block_size
    }

    pub fn total_size(&self) -> u64 {
        self.total_size
    }

    pub fn read_blocks(&self, lba: u64, num_blocks: u32, buf: &mut [u8]) -> io::Result<()> {
        let bs = self.block_size;
        let _read_len = (num_blocks as u64) * bs;

        if buf.len() < _read_len as usize {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Buffer terlalu kecil"));
        }

        let mut inner = self.inner.lock();
        let total_blocks = self.total_blocks;

        if lba >= total_blocks {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "LBA melebihi batas"));
        }
        let max_blocks = (total_blocks - lba) as u32;
        let num = num_blocks.min(max_blocks);
        let actual_len = (num as u64) * bs;

        if actual_len <= RA_SIZE as u64 {
            if lba >= inner.ra_lba
                && (lba - inner.ra_lba) < inner.ra_blocks as u64
                && (lba - inner.ra_lba + num as u64) <= inner.ra_blocks as u64
            {
                let off = ((lba - inner.ra_lba) * bs) as usize;
                buf[..actual_len as usize]
                    .copy_from_slice(&inner.ra_buf[off..off + actual_len as usize]);
                return Ok(());
            }

            let max_readable = (total_blocks - lba) * bs;
            let ra_bytes = (RA_SIZE as u64).min(max_readable) as usize;
            
            // Read into read-ahead cache
            let ra_slice = unsafe { &mut *std::ptr::addr_of_mut!(inner.ra_buf[..ra_bytes]) };
            inner.backend.read_exact_at(lba, bs, ra_slice)?;
            inner.ra_lba = lba;
            inner.ra_blocks = (ra_bytes as u64 / bs) as u32;

            buf[..actual_len as usize]
                .copy_from_slice(&inner.ra_buf[..actual_len as usize]);
        } else {
            inner.backend.read_exact_at(lba, bs, &mut buf[..actual_len as usize])?;
        }

        Ok(())
    }
}

#[cfg(windows)]
fn get_windows_drive_size(file: &std::fs::File) -> std::io::Result<u64> {
    use std::os::windows::io::AsRawHandle;

    type HANDLE = *mut std::ffi::c_void;
    type BOOL = std::ffi::c_int;
    type DWORD = std::ffi::c_ulong;
    type LPOVERLAPPED = *mut std::ffi::c_void;
    type LPVOID = *mut std::ffi::c_void;

    extern "system" {
        fn DeviceIoControl(
            hDevice: HANDLE,
            dwIoControlCode: DWORD,
            lpInBuffer: LPVOID,
            nInBufferSize: DWORD,
            lpOutBuffer: LPVOID,
            nOutBufferSize: DWORD,
            lpBytesReturned: *mut DWORD,
            lpOverlapped: LPOVERLAPPED,
        ) -> BOOL;
    }

    const IOCTL_DISK_GET_LENGTH_INFO: DWORD = 0x0007405C;

    let handle = file.as_raw_handle() as HANDLE;
    let mut length: u64 = 0;
    let mut bytes_returned: DWORD = 0;

    let success = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_DISK_GET_LENGTH_INFO,
            std::ptr::null_mut(),
            0,
            &mut length as *mut u64 as LPVOID,
            std::mem::size_of::<u64>() as DWORD,
            &mut bytes_returned,
            std::ptr::null_mut(),
        )
    };

    if success != 0 {
        Ok(length)
    } else {
        Err(std::io::Error::last_os_error())
    }
}
