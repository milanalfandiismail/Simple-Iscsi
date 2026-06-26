use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;
use parking_lot::Mutex;
use tracing::{info, error};

/// 256KB read-ahead buffer — matched to typical game file access patterns.
/// Sequential reads hit cache after first miss. Random reads bypass cleanly.
const RA_SIZE: usize = 256 * 1024;

struct BackendInner {
    file: File,
    /// Read-ahead buffer: holds raw bytes from last miss + prefetched data.
    ra_buf: Vec<u8>,
    /// Starting LBA of the read-ahead cache window.
    ra_lba: u64,
    /// Number of blocks currently held in ra_buf.
    ra_blocks: u32,
}

pub struct Backend {
    inner: Arc<Mutex<BackendInner>>,
    block_size: u64,
    total_size: u64,
    total_blocks: u64,
}

impl Backend {
    pub fn new(path: &str, block_size: u64) -> io::Result<Self> {
        info!("Membuka storage backend: {}", path);

        let mut options = std::fs::OpenOptions::new();
        options.read(true);

        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1 | 2); // FILE_SHARE_READ | FILE_SHARE_WRITE
        }

        #[allow(unused_mut)]
        let mut file = match options.open(path) {
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
                            error!("Gagal menentukan ukuran target drive menggunakan DeviceIoControl: {}", e);
                            return Err(e);
                        }
                    }
                }
                #[cfg(not(windows))]
                {
                    match file.seek(SeekFrom::End(0)) {
                        Ok(pos) => {
                            let _ = file.seek(SeekFrom::Start(0));
                            pos
                        }
                        Err(e) => {
                            error!("Gagal menentukan ukuran target drive menggunakan seek: {}", e);
                            return Err(e);
                        }
                    }
                }
            }
        };

        info!(
            "Storage backend berhasil dibuka. Ukuran: {} byte ({:.2} GB, {} block)",
            total_size,
            (total_size as f64) / 1024.0 / 1024.0 / 1024.0,
            total_size / block_size
        );

        let inner = BackendInner {
            file,
            ra_buf: vec![0u8; RA_SIZE],
            ra_lba: u64::MAX,
            ra_blocks: 0,
        };

        Ok(Backend {
            inner: Arc::new(Mutex::new(inner)),
            block_size,
            total_size,
            total_blocks: total_size / block_size,
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
        let read_len = (num_blocks as u64) * bs;

        if buf.len() < read_len as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Buffer target terlalu kecil dibanding request baca",
            ));
        }

        let mut inner = self.inner.lock();
        let total_blocks = self.total_blocks;

        // Guard: clamp to disk boundary (os error 27 jika baca melewati akhir disk)
        if lba >= total_blocks {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "LBA melebihi total blocks disk",
            ));
        }
        let max_blocks = (total_blocks - lba) as u32;
        let num = num_blocks.min(max_blocks);
        let actual_len = (num as u64) * bs;

        // Small request: try read-ahead cache
        if actual_len <= RA_SIZE as u64 {
            // Cache hit?
            if lba >= inner.ra_lba
                && (lba - inner.ra_lba) < inner.ra_blocks as u64
                && (lba - inner.ra_lba + num as u64) <= inner.ra_blocks as u64
            {
                let off = ((lba - inner.ra_lba) * bs) as usize;
                buf[..actual_len as usize]
                    .copy_from_slice(&inner.ra_buf[off..off + actual_len as usize]);
                return Ok(());
            }

            // Miss: do 256KB read-ahead
            let offset = lba * bs;
            inner.file.seek(SeekFrom::Start(offset))?;

            let max_readable = (total_blocks - lba) * bs;
            let ra_bytes = (RA_SIZE as u64).min(max_readable) as usize;
            // Can't borrow inner.file & inner.ra_buf simultaneously through MutexGuard.
            // Read into temp buf, then copy to ra_buf — only on cache miss, fine.
            let mut tmp = vec![0u8; ra_bytes];
            inner.file.read_exact(&mut tmp)?;
            inner.ra_buf[..ra_bytes].copy_from_slice(&tmp);
            inner.ra_lba = lba;
            inner.ra_blocks = (ra_bytes / bs as usize) as u32;

            buf[..actual_len as usize]
                .copy_from_slice(&inner.ra_buf[..actual_len as usize]);
        } else {
            // Read larger than RA_SIZE: bypass cache, direct I/O
            let offset = lba * bs;
            inner.file.seek(SeekFrom::Start(offset))?;
            inner.file.read_exact(&mut buf[..actual_len as usize])?;
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

    // IOCTL_DISK_GET_LENGTH_INFO
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
