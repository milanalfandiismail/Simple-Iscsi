use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{info, warn, error};

use crate::fs_utils::{file_read_exact_at, file_write_all_at};
use crate::read_ahead::ReadAheadCache;

use crate::vhd::VhdBackend;

/// 8MB read-ahead buffer
const RA_SIZE: usize = 8 * 1024 * 1024;

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
    fn read_exact_at(&self, lba: u64, block_size: u64, buf: &mut [u8]) -> io::Result<()> {
        match self {
            BackendType::RawDisk(ref file) => {
                let offset = lba * block_size;
                file_read_exact_at(file, offset, buf)
            }
            BackendType::Vhd(ref vhd) => {
                vhd.read_blocks( lba, block_size, buf)
            }
            BackendType::VhdDiff { ref child, ref parent } => {
                child.diff_read_blocks( parent, lba, block_size, buf)
            }
        }
    }

    fn needs_allocation(&self, lba: u64, block_size: u64, buf_len: usize) -> bool {
        match self {
            BackendType::RawDisk(_) => false,
            BackendType::Vhd(ref vhd) => Self::vhd_needs_allocation(vhd, lba, block_size, buf_len),
            BackendType::VhdDiff { ref child, .. } => Self::vhd_needs_allocation(child, lba, block_size, buf_len),
        }
    }

    fn vhd_needs_allocation(vhd: &VhdBackend, lba: u64, block_size: u64, buf_len: usize) -> bool {
        let vhd_block_size = vhd.vhd_block_size as u64;
        let start_block = (lba * block_size) / vhd_block_size;
        let end_block = ((lba * block_size + buf_len as u64 - 1) / vhd_block_size) + 1;
        for block_idx in start_block..end_block.min(vhd.bat.len() as u64) {
            if vhd.bat[block_idx as usize] == 0xFFFFFFFF {
                return true;
            }
        }
        false
    }

    fn write_concurrently(&self, lba: u64, block_size: u64, buf: &[u8]) -> io::Result<()> {
        match self {
            BackendType::RawDisk(ref file) => {
                let offset = lba * block_size;
                file_write_all_at(file, offset, buf)?;
                Ok(())
            }
            BackendType::Vhd(ref vhd) => {
                vhd.write_blocks_concurrent( lba, block_size, buf)
            }
            BackendType::VhdDiff { ref child, .. } => {
                child.write_blocks_concurrent( lba, block_size, buf)
            }
        }
    }

    

    fn write_at(&mut self, lba: u64, block_size: u64, buf: &[u8]) -> io::Result<()> {
        match self {
            BackendType::RawDisk(ref mut file) => {
                let offset = lba * block_size;
                file.seek(SeekFrom::Start(offset))?;
                file.write_all(buf)?;
                Ok(())
            }
            BackendType::Vhd(ref mut vhd) => {
                vhd.write_blocks( lba, block_size, buf)
            }
            BackendType::VhdDiff { ref mut child, ref mut parent } => {
                // Copy-on-write: read parent data for newly allocated blocks
                child.diff_write_blocks( parent, lba, block_size, buf)
            }
        }
    }

    fn sync(&mut self) -> io::Result<()> {
        match self {
            BackendType::RawDisk(ref mut file) => file.sync_all(),
            BackendType::Vhd(ref mut vhd) => vhd.file.sync_all(),
            BackendType::VhdDiff { ref mut child, .. } => child.file.sync_all(),
        }
    }

    
}

struct BackendInner {
    backend: BackendType,
}

pub struct Backend {
    inner: Arc<RwLock<BackendInner>>,
    block_size: u64,
    total_size: u64,
    total_blocks: u64,
    pub vendor_id: String,
    pub product_id: String,
    pub product_revision: String,
    read_cache: Option<ReadAheadCache>,
    pub io_semaphore: Arc<tokio::sync::Semaphore>,
}

impl Backend {
    pub fn new_raw(path: &str, block_size: u64, vendor: &str, product: &str, rev: &str, read_cache_gb: u64) -> io::Result<Self> {
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

        // Re-open with FILE_FLAG_OVERLAPPED for concurrent lock-free reads
        let mut ov_options = std::fs::OpenOptions::new();
        ov_options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            ov_options.share_mode(1 | 2); // FILE_SHARE_READ | FILE_SHARE_WRITE
            ov_options.custom_flags(0x40000000); // FILE_FLAG_OVERLAPPED
        }
        let ov_file = ov_options.open(path)?;

        let inner = BackendInner {
            backend: BackendType::RawDisk(ov_file),
        };

        let read_cache = ReadAheadCache::new(read_cache_gb, 256 * 1024);

        Ok(Backend {
            inner: Arc::new(RwLock::new(inner)),
            block_size,
            total_size,
            total_blocks: total_size / block_size,
            vendor_id: vendor.to_string(),
            product_id: product.to_string(),
            product_revision: rev.to_string(),
            read_cache,
            io_semaphore: Arc::new(tokio::sync::Semaphore::new(32)),
        })
    }

    #[allow(dead_code)]
    pub fn new_vhd(path: &str, block_size: u64, vendor: &str, product: &str, rev: &str, read_cache_gb: u64) -> io::Result<Self> {
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

        let mut vhd = VhdBackend::open(file)?;
        let total_size = vhd.current_size;

        // Re-open VHD file with FILE_FLAG_OVERLAPPED
        let mut ov_options = std::fs::OpenOptions::new();
        ov_options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            ov_options.share_mode(1 | 2); // FILE_SHARE_READ | FILE_SHARE_WRITE
            ov_options.custom_flags(0x40000000); // FILE_FLAG_OVERLAPPED
        }
        let ov_file = ov_options.open(path)?;
        vhd.file = ov_file;

        info!("VHD backend dibuka. Ukuran: {} byte", total_size);

        let inner = BackendInner {
            backend: BackendType::Vhd(vhd),
        };

        let read_cache = ReadAheadCache::new(read_cache_gb, 256 * 1024);

        Ok(Backend {
            inner: Arc::new(RwLock::new(inner)),
            block_size,
            total_size,
            total_blocks: total_size / block_size,
            vendor_id: vendor.to_string(),
            product_id: product.to_string(),
            product_revision: rev.to_string(),
            read_cache,
            io_semaphore: Arc::new(tokio::sync::Semaphore::new(32)),
        })
    }

    pub fn new_vhd_diff(child_path: &str, parent_path: &str, block_size: u64, vendor: &str, product: &str, rev: &str, read_cache_gb: u64) -> io::Result<Self> {
        info!("Membuka VHD differencing child: {}, parent: {}", child_path, parent_path);

        let mut child_options = std::fs::OpenOptions::new();
        child_options.read(true).write(true);

        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            child_options.share_mode(1 | 2 | 4); // READ|WRITE|DELETE — biar bisa dihapus pas cleanup
        }

        let child_file = child_options.open(child_path)
            .map_err(|e| { error!("Gagal membuka child VHD: {}", e); e })?;
        let mut child = VhdBackend::open(child_file)?;
        let total_size = child.current_size;

        // Re-open child VHD with FILE_FLAG_OVERLAPPED
        let mut ov_child_options = std::fs::OpenOptions::new();
        ov_child_options.read(true).write(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            ov_child_options.share_mode(1 | 2 | 4); // READ|WRITE|DELETE
            ov_child_options.custom_flags(0x40000000); // FILE_FLAG_OVERLAPPED
        }
        let ov_child_file = ov_child_options.open(child_path)?;
        child.file = ov_child_file;

        // Buka parent (read-only)
        let parent = if let Some(ref parent_path_str) = child.parent_path {
            let mut parent_options = std::fs::OpenOptions::new();
            parent_options.read(true);

            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                parent_options.share_mode(1 | 2);
            }

            let mut opened_path = None;
            let parent_file_opt = match parent_options.open(parent_path_str) {
                Ok(f) => {
                    opened_path = Some(parent_path_str.clone());
                    Some(f)
                }
                Err(_) => match parent_options.open(parent_path) {
                    Ok(f) => {
                        opened_path = Some(parent_path.to_string());
                        Some(f)
                    }
                    Err(_) => None,
                }
            };

            if let Some(parent_file) = parent_file_opt {
                let mut parent_vhd = VhdBackend::open(parent_file)?;
                let path_str = opened_path.unwrap();

                // Re-open parent VHD with FILE_FLAG_OVERLAPPED
                let mut ov_parent_options = std::fs::OpenOptions::new();
                ov_parent_options.read(true);
                #[cfg(windows)]
                {
                    use std::os::windows::fs::OpenOptionsExt;
                    ov_parent_options.share_mode(1 | 2);
                    ov_parent_options.custom_flags(0x40000000); // FILE_FLAG_OVERLAPPED
                }
                let ov_parent_file = ov_parent_options.open(&path_str)?;
                parent_vhd.file = ov_parent_file;

                info!("Parent VHD dibuka: {} (size={})", path_str, parent_vhd.current_size);
                Some(parent_vhd)
            } else {
                warn!("Parent VHD tidak ditemukan: {} (child parent_path: {}), running without parent fallback", parent_path, parent_path_str);
                None
            }
        } else {
            warn!("Child VHD tidak memiliki parent_path!");
            None
        };

        info!("VHD differencing backend dibuka. Child size: {} byte", total_size);

        let inner = BackendInner {
            backend: BackendType::VhdDiff { child, parent },
        };

        let read_cache = ReadAheadCache::new(read_cache_gb, 256 * 1024);

        Ok(Backend {
            inner: Arc::new(RwLock::new(inner)),
            block_size,
            total_size,
            total_blocks: total_size / block_size,
            vendor_id: vendor.to_string(),
            product_id: product.to_string(),
            product_revision: rev.to_string(),
            read_cache,
            io_semaphore: Arc::new(tokio::sync::Semaphore::new(32)),
        })
    }

    pub fn block_size(&self) -> u64 {
        self.block_size
    }

    pub fn total_size(&self) -> u64 {
        self.total_size
    }

    pub fn try_read_from_cache(&self, lba: u64, num_blocks: u32, buf: &mut [u8]) -> Option<()> {
        let bs = self.block_size;
        let cache = self.read_cache.as_ref()?;

        let chunk_size_bytes = 256 * 1024;
        let start_byte = lba * bs;
        let actual_len = (num_blocks as u64) * bs;
        let end_byte = start_byte + actual_len;

        let start_chunk = start_byte / chunk_size_bytes;
        let end_chunk = if end_byte > 0 { (end_byte - 1) / chunk_size_bytes } else { 0 };

        let mut chunks = Vec::with_capacity((end_chunk - start_chunk + 1) as usize);
        for chunk_id in start_chunk..=end_chunk {
            if let Some(data) = cache.get(chunk_id) {
                chunks.push(data);
            } else {
                return None; // Cache miss
            }
        }

        let mut buf_offset = 0;
        for (i, chunk_id) in (start_chunk..=end_chunk).enumerate() {
            let chunk_start_byte = chunk_id * chunk_size_bytes;
            let chunk_end_byte = chunk_start_byte + chunk_size_bytes;

            let read_start = start_byte.max(chunk_start_byte);
            let read_end = end_byte.min(chunk_end_byte);
            let bytes_to_copy = (read_end - read_start) as usize;

            let chunk_data = &chunks[i];
            let offset_in_chunk = (read_start - chunk_start_byte) as usize;
            buf[buf_offset..buf_offset + bytes_to_copy].copy_from_slice(&chunk_data[offset_in_chunk..offset_in_chunk + bytes_to_copy]);
            buf_offset += bytes_to_copy;
        }

        Some(())
    }

    pub fn read_blocks(&self, lba: u64, num_blocks: u32, buf: &mut [u8]) -> io::Result<()> {
        let bs = self.block_size;
        let read_len = (num_blocks as u64) * bs;

        if buf.len() < read_len as usize {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Buffer terlalu kecil"));
        }

        let total_blocks = self.total_blocks;
        if lba >= total_blocks {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "LBA melebihi batas"));
        }
        let max_blocks = (total_blocks - lba) as u32;
        let num = num_blocks.min(max_blocks);
        let actual_len = (num as u64) * bs;

        let cache = match &self.read_cache {
            Some(c) => c,
            None => {
                let inner = self.inner.read();
                return inner.backend.read_exact_at(lba, bs, &mut buf[..actual_len as usize]);
            }
        };

        let chunk_size_bytes = 256 * 1024;
        let blocks_per_chunk = if bs > 0 { chunk_size_bytes / bs } else { 1 };

        let start_byte = lba * bs;
        let end_byte = start_byte + actual_len;

        let start_chunk = start_byte / chunk_size_bytes;
        let end_chunk = if end_byte > 0 { (end_byte - 1) / chunk_size_bytes } else { 0 };

        let mut buf_offset = 0;

        for chunk_id in start_chunk..=end_chunk {
            let chunk_start_byte = chunk_id * chunk_size_bytes;
            let chunk_end_byte = chunk_start_byte + chunk_size_bytes;

            let read_start = start_byte.max(chunk_start_byte);
            let read_end = end_byte.min(chunk_end_byte);
            let bytes_to_copy = (read_end - read_start) as usize;

            let chunk_data = if let Some(data) = cache.get(chunk_id) {
                data
            } else {
                let mut chunk_buf = vec![0u8; chunk_size_bytes as usize];
                let chunk_lba = chunk_start_byte / bs;
                let chunk_blocks = if chunk_lba + blocks_per_chunk > total_blocks {
                    (total_blocks - chunk_lba) as u32
                } else {
                    blocks_per_chunk as u32
                };

                let inner = self.inner.read();
                inner.backend.read_exact_at(chunk_lba, bs, &mut chunk_buf[.. (chunk_blocks as u64 * bs) as usize])?;

                let data = Arc::new(chunk_buf);
                cache.insert(chunk_id, Arc::clone(&data));
                data
            };

            let offset_in_chunk = (read_start - chunk_start_byte) as usize;
            buf[buf_offset..buf_offset + bytes_to_copy].copy_from_slice(&chunk_data[offset_in_chunk..offset_in_chunk + bytes_to_copy]);
            buf_offset += bytes_to_copy;
        }

        Ok(())
    }

    pub fn write_blocks(&self, lba: u64, num_blocks: u32, buf: &[u8]) -> io::Result<()> {
        let bs = self.block_size;
        let write_len = (num_blocks as u64) * bs;

        if buf.len() < write_len as usize {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Buffer terlalu kecil untuk write"));
        }

        if let Some(ref cache) = self.read_cache {
            cache.invalidate_range(lba * bs, write_len);
        }

        let needs_alloc = {
            let inner = self.inner.read();
            inner.backend.needs_allocation(lba, bs, write_len as usize)
        };

        if !needs_alloc {
            let inner = self.inner.read();
            inner.backend.write_concurrently(lba, bs, &buf[..write_len as usize])?;
        } else {
            let mut inner = self.inner.write();
            inner.backend.write_at(lba, bs, &buf[..write_len as usize])?;
        }

        Ok(())
    }

    /// Flush all pending writes to physical disk
    pub fn sync(&self) -> io::Result<()> {
        let mut inner = self.inner.write();
        inner.backend.sync()
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
