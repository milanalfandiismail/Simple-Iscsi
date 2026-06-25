use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use tracing::{info, error};

pub struct Backend {
    file: std::sync::Mutex<File>,
    block_size: u64,
    total_size: u64,
}

impl Backend {
    pub fn new(path: &str, block_size: u64) -> io::Result<Self> {
        info!("Membuka storage backend: {}", path);
        
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // FILE_SHARE_READ = 1, FILE_SHARE_WRITE = 2
            options.share_mode(1 | 2);
        }

        #[allow(unused_mut)]
        let mut file = match options.open(path) {
            Ok(f) => f,
            Err(e) => {
                error!("Gagal membuka storage backend di {:?}: {}", path, e);
                return Err(e);
            }
        };

        // Dapatkan ukuran drive/file
        let total_size = match file.metadata().map(|m| m.len()) {
            Ok(len) if len > 0 => len,
            _ => {
                // Pada Windows, raw block device tidak mendukung seek ke akhir untuk ukuran.
                // Kita gunakan DeviceIoControl.
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

        Ok(Backend {
            file: std::sync::Mutex::new(file),
            block_size,
            total_size,
        })
    }

    pub fn block_size(&self) -> u64 {
        self.block_size
    }

    pub fn total_size(&self) -> u64 {
        self.total_size
    }

    pub fn read_blocks(&self, lba: u64, num_blocks: u32, buf: &mut [u8]) -> io::Result<()> {
        let offset = lba * self.block_size;
        let read_len = (num_blocks as u64) * self.block_size;
        
        if buf.len() < read_len as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Buffer target terlalu kecil dibanding request baca",
            ));
        }

        let mut file = self.file.lock().unwrap();
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut buf[..read_len as usize])?;
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
