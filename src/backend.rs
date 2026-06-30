use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::sync::Arc;
use parking_lot::Mutex;
use tracing::{info, warn, error};

use crate::vhd::VhdBackend;

/// 1MB read-ahead buffer
const RA_SIZE: usize = 1024 * 1024;

#[allow(dead_code)]
enum BackendType {
    RawDisk(File),
    Vhd(VhdBackend),
    VhdDiff {
        child: VhdBackend,
        parent: Option<VhdBackend>,
    },
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
                Self::vhd_read_blocks(vhd, lba, block_size, buf)
            }
            BackendType::VhdDiff { ref mut child, ref mut parent } => {
                Self::vhd_diff_read_blocks(child, parent, lba, block_size, buf)
            }
        }
    }

    fn write_at(&mut self, lba: u64, block_size: u64, buf: &[u8]) -> io::Result<()> {
        match self {
            BackendType::RawDisk(ref mut file) => {
                let offset = lba * block_size;
                file.seek(SeekFrom::Start(offset))?;
                file.write_all(buf)?;
                file.sync_all()?;
                Ok(())
            }
            BackendType::Vhd(ref mut vhd) => {
                Self::vhd_write_blocks(vhd, lba, block_size, buf)
            }
            BackendType::VhdDiff { ref mut child, ref mut parent } => {
                // Copy-on-write: read parent data for newly allocated blocks
                Self::vhd_diff_write_blocks(child, parent, lba, block_size, buf)
            }
        }
    }

    /// Read from single VHD (dynamic or simple)
    fn vhd_read_blocks(vhd: &mut VhdBackend, lba: u64, block_size: u64, buf: &mut [u8]) -> io::Result<()> {
        let mut buf_offset = 0;
        let mut current_byte_offset = lba * block_size;
        let vhd_block_size = vhd.vhd_block_size as u64;

        while buf_offset < buf.len() {
            let vhd_block_idx = current_byte_offset / vhd_block_size;
            let offset_in_vhd_block = current_byte_offset % vhd_block_size;

            let bytes_to_read = std::cmp::min(
                buf.len() - buf_offset,
                (vhd_block_size - offset_in_vhd_block) as usize
            );

            let bat_val = *vhd.bat.get(vhd_block_idx as usize).unwrap_or(&0xFFFFFFFF);
            if bat_val == 0xFFFFFFFF {
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

    /// Read from differencing disk: child first, fallback to parent, fallback to zeros
    fn vhd_diff_read_blocks(child: &mut VhdBackend, parent: &mut Option<VhdBackend>, lba: u64, block_size: u64, buf: &mut [u8]) -> io::Result<()> {
        let mut buf_offset = 0;
        let mut current_byte_offset = lba * block_size;
        let vhd_block_size = child.vhd_block_size as u64;

        while buf_offset < buf.len() {
            let vhd_block_idx = current_byte_offset / vhd_block_size;
            let offset_in_vhd_block = current_byte_offset % vhd_block_size;

            let bytes_to_read = std::cmp::min(
                buf.len() - buf_offset,
                (vhd_block_size - offset_in_vhd_block) as usize
            );

            let child_bat = *child.bat.get(vhd_block_idx as usize).unwrap_or(&0xFFFFFFFF);
            if child_bat != 0xFFFFFFFF {
                // Read from child
                let physical_offset = (child_bat as u64) * 512 + (child.sector_bitmap_size as u64) + offset_in_vhd_block;
                child.file.seek(SeekFrom::Start(physical_offset))?;
                child.file.read_exact(&mut buf[buf_offset..buf_offset + bytes_to_read])?;
            } else if let Some(ref mut parent) = parent {
                let parent_bat = *parent.bat.get(vhd_block_idx as usize).unwrap_or(&0xFFFFFFFF);
                if parent_bat != 0xFFFFFFFF {
                    // Read from parent
                    let physical_offset = (parent_bat as u64) * 512 + (parent.sector_bitmap_size as u64) + offset_in_vhd_block;
                    parent.file.seek(SeekFrom::Start(physical_offset))?;
                    parent.file.read_exact(&mut buf[buf_offset..buf_offset + bytes_to_read])?;
                } else {
                    for i in 0..bytes_to_read {
                        buf[buf_offset + i] = 0;
                    }
                }
            } else {
                for i in 0..bytes_to_read {
                    buf[buf_offset + i] = 0;
                }
            }
            buf_offset += bytes_to_read;
            current_byte_offset += bytes_to_read as u64;
        }
        Ok(())
    }

    /// Write to differencing disk with Copy-on-Write: allocate child block,
    /// copy parent data first, then overlay write data.
    fn vhd_diff_write_blocks(child: &mut VhdBackend, parent: &mut Option<VhdBackend>, lba: u64, block_size: u64, buf: &[u8]) -> io::Result<()> {
        let vhd_block_size = child.vhd_block_size as u64;
        let bitmap_size = child.sector_bitmap_size as u64;
        let start_block = (lba * block_size) / vhd_block_size;
        let end_block = ((lba * block_size + buf.len() as u64 - 1) / vhd_block_size) + 1;

        // Phase 1: Allocate new blocks — copy from parent if available (CoW)
        for block_idx in start_block..end_block.min(child.bat.len() as u64) {
            if child.bat[block_idx as usize] == 0xFFFFFFFF {
                // Allocate at EOF
                let eof = child.file.seek(SeekFrom::End(0))?;
                let bat_entry = (eof / 512) as u32;

                // Write sector bitmap (all zeros = all sectors dirty)
                let zero_bitmap = vec![0u8; bitmap_size as usize];
                child.file.write_all(&zero_bitmap)?;

                // COPY-ON-WRITE: read full block from parent
                let mut block_data = vec![0u8; vhd_block_size as usize];
                if let Some(ref mut p) = parent {
                    let parent_bat = *p.bat.get(block_idx as usize).unwrap_or(&0xFFFFFFFF);
                    if parent_bat != 0xFFFFFFFF {
                        let parent_offset = (parent_bat as u64) * 512 + (p.sector_bitmap_size as u64);
                        p.file.seek(SeekFrom::Start(parent_offset))?;
                        p.file.read_exact(&mut block_data)?;
                    }
                }

                child.file.write_all(&block_data)?;

                // Update BAT
                child.bat[block_idx as usize] = bat_entry;
                let bat_offset = 1536 + (block_idx * 4) as u64;
                child.file.seek(SeekFrom::Start(bat_offset))?;
                child.file.write_all(&bat_entry.to_be_bytes())?;
            }
        }

        // Phase 2: Overlay write data onto child blocks
        let mut buf_offset = 0;
        let mut current_byte_offset = lba * block_size;
        while buf_offset < buf.len() {
            let vhd_block_idx = current_byte_offset / vhd_block_size;
            let offset_in_vhd_block = current_byte_offset % vhd_block_size;
            let bytes_to_write = std::cmp::min(
                buf.len() - buf_offset,
                (vhd_block_size - offset_in_vhd_block) as usize
            );
            let bat_val = child.bat[vhd_block_idx as usize];
            let physical_offset = (bat_val as u64) * 512 + bitmap_size + offset_in_vhd_block;
            child.file.seek(SeekFrom::Start(physical_offset))?;
            child.file.write_all(&buf[buf_offset..buf_offset + bytes_to_write])?;
            buf_offset += bytes_to_write;
            current_byte_offset += bytes_to_write as u64;
        }

        child.file.sync_all()?;
        Ok(())
    }

    /// Write blocks to VHD — allocate new BAT entries, update sector bitmap
    fn vhd_write_blocks(vhd: &mut VhdBackend, lba: u64, block_size: u64, buf: &[u8]) -> io::Result<()> {
        let vhd_block_size = vhd.vhd_block_size as u64;
        let bitmap_size = vhd.sector_bitmap_size as u64;
        let start_block = (lba * block_size) / vhd_block_size;
        let end_block = ((lba * block_size + buf.len() as u64 - 1) / vhd_block_size) + 1;

        // Allocate new blocks for any BAT entries that are 0xFFFFFFFF
        for block_idx in start_block..end_block.min(vhd.bat.len() as u64) {
            if vhd.bat[block_idx as usize] == 0xFFFFFFFF {
                // Append to EOF: bitmap + data block
                let eof = vhd.file.seek(SeekFrom::End(0))?;
                let bat_entry = (eof / 512) as u32;

                // Write sector bitmap (all zeros = all sectors dirty)
                let zero_bitmap = vec![0u8; bitmap_size as usize];
                vhd.file.write_all(&zero_bitmap)?;

                // Write zeroed data block
                let zero_block = vec![0u8; vhd_block_size as usize];
                vhd.file.write_all(&zero_block)?;

                // Update BAT in-memory
                vhd.bat[block_idx as usize] = bat_entry;

                // Update BAT on disk
                let bat_offset = 1536 + (block_idx * 4) as u64;
                vhd.file.seek(SeekFrom::Start(bat_offset))?;
                vhd.file.write_all(&bat_entry.to_be_bytes())?;
            }
        }

        // Write data block-by-block
        let mut buf_offset = 0;
        let mut current_byte_offset = lba * block_size;

        while buf_offset < buf.len() {
            let vhd_block_idx = current_byte_offset / vhd_block_size;
            let offset_in_vhd_block = current_byte_offset % vhd_block_size;

            let bytes_to_write = std::cmp::min(
                buf.len() - buf_offset,
                (vhd_block_size - offset_in_vhd_block) as usize
            );

            let bat_val = vhd.bat[vhd_block_idx as usize];
            let physical_offset = (bat_val as u64) * 512 + bitmap_size + offset_in_vhd_block;
            vhd.file.seek(SeekFrom::Start(physical_offset))?;
            vhd.file.write_all(&buf[buf_offset..buf_offset + bytes_to_write])?;

            buf_offset += bytes_to_write;
            current_byte_offset += bytes_to_write as u64;
        }

        vhd.file.sync_all()?;
        Ok(())
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

    #[allow(dead_code)]
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

    pub fn new_vhd_diff(child_path: &str, parent_path: &str, block_size: u64, vendor: &str, product: &str, rev: &str) -> io::Result<Self> {
        info!("Membuka VHD differencing child: {}, parent: {}", child_path, parent_path);

        let mut child_options = std::fs::OpenOptions::new();
        child_options.read(true).write(true);

        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            child_options.share_mode(1 | 2);
        }

        let child_file = child_options.open(child_path)
            .map_err(|e| { error!("Gagal membuka child VHD: {}", e); e })?;
        let child = VhdBackend::open(child_file)?;

        let total_size = child.current_size;

        // Buka parent (read-only)
        let parent = if let Some(ref parent_path_str) = child.parent_path {
            let mut parent_options = std::fs::OpenOptions::new();
            parent_options.read(true);

            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                parent_options.share_mode(1 | 2);
            }

            match parent_options.open(parent_path_str) {
                Ok(parent_file) => {
                    let parent_vhd = VhdBackend::open(parent_file)?;
                    info!("Parent VHD dibuka: {} (size={})", parent_path_str, parent_vhd.current_size);
                    Some(parent_vhd)
                }
                Err(_e) => {
                    // Try the provided parent_path as fallback
                    match parent_options.open(parent_path) {
                        Ok(parent_file) => {
                            let parent_vhd = VhdBackend::open(parent_file)?;
                            info!("Parent VHD dibuka (fallback): {} (size={})", parent_path, parent_vhd.current_size);
                            Some(parent_vhd)
                        }
                        Err(_) => {
                            warn!("Parent VHD tidak ditemukan: {} (child parent_path: {}), running without parent fallback", parent_path, parent_path_str);
                            None
                        }
                    }
                }
            }
        } else {
            warn!("Child VHD tidak memiliki parent_path!");
            None
        };

        info!("VHD differencing backend dibuka. Child size: {} byte", total_size);

        let inner = BackendInner {
            backend: BackendType::VhdDiff { child, parent },
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

    pub fn write_blocks(&self, lba: u64, num_blocks: u32, buf: &[u8]) -> io::Result<()> {
        let bs = self.block_size;
        let write_len = (num_blocks as u64) * bs;

        if buf.len() < write_len as usize {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Buffer terlalu kecil untuk write"));
        }

        let mut inner = self.inner.lock();
        inner.backend.write_at(lba, bs, &buf[..write_len as usize])
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
