use serde_json::json;
use crate::config_manager::SharedConfig;
use crate::server_api::build_response;
use std::path::Path;
use std::fs;
use std::os::windows::io::AsRawHandle;

pub fn get_system_drives() -> String {
    let mut drives = Vec::new();
    for i in 0..16 {
        let path = format!("\\\\.\\PhysicalDrive{}", i);
        let file_opts = std::fs::OpenOptions::new()
            .read(true)
            .open(&path);
        if let Ok(file) = file_opts {
            let size = file.metadata().map(|m| m.len()).unwrap_or(0);
            drives.push(json!({
                "path": path,
                "size": size,
            }));
        }
    }
    build_response(200, "OK", "application/json", &json!(drives).to_string())
}

pub fn get_logical_drives_detail() -> String {
    extern "system" {
        fn DeviceIoControl(
            hDevice: *mut std::ffi::c_void,
            dwIoControlCode: u32,
            lpInBuffer: *mut std::ffi::c_void,
            nInBufferSize: u32,
            lpOutBuffer: *mut std::ffi::c_void,
            nOutBufferSize: u32,
            lpBytesReturned: *mut u32,
            lpOverlapped: *mut std::ffi::c_void,
        ) -> i32;
    }

    #[repr(C)]
    struct DiskExtent {
        disk_number: u32,
        starting_offset: i64,
        extent_length: i64,
    }

    #[repr(C)]
    struct VolumeDiskExtents {
        number_of_disk_extents: u32,
        extents: [DiskExtent; 1],
    }

    let mut details = Vec::new();
    for c in b'A'..=b'Z' {
        let letter = (c as char).to_string();
        let path = format!("{}:\\", letter);
        if std::path::Path::new(&path).exists() {
            let disk_num = {
                let dev_path = format!("\\\\.\\{}:", letter);
                if let Ok(file) = std::fs::OpenOptions::new().read(true).open(&dev_path) {
                    let handle = file.as_raw_handle();
                    unsafe {
                        let mut extents: VolumeDiskExtents = std::mem::zeroed();
                        let mut bytes_returned = 0u32;
                        let res = DeviceIoControl(
                            handle as _,
                            5636096, // IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS
                            std::ptr::null_mut(),
                            0,
                            &mut extents as *mut _ as _,
                            std::mem::size_of::<VolumeDiskExtents>() as u32,
                            &mut bytes_returned,
                            std::ptr::null_mut(),
                        );
                        if res != 0 && extents.number_of_disk_extents > 0 {
                            Some(extents.extents[0].disk_number)
                        } else {
                            None
                        }
                    }
                } else {
                    None
                }
            };

            details.push(json!({
                "letter": letter,
                "physical_disk": disk_num.map(|num| format!("\\\\.\\PhysicalDrive{}", num)),
            }));
        }
    }
    build_response(200, "OK", "application/json", &json!(details).to_string())
}

pub fn get_network_interfaces() -> String {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct MibIpAddrRow {
        dw_addr: u32,
        dw_index: u32,
        dw_mask: u32,
        dw_bc_addr: u32,
        dw_reasm_size: u32,
        unused1: u16,
        unused2: u16,
    }

    extern "system" {
        fn GetIpAddrTable(
            pIpAddrTable: *mut u8,
            pdwSize: *mut u32,
            bOrder: i32,
        ) -> u32;
    }

    let mut size = 0;
    unsafe {
        GetIpAddrTable(std::ptr::null_mut(), &mut size, 0);
    }

    let mut ips = Vec::new();
    if size > 0 {
        let mut buf = vec![0u8; size as usize];
        let ret = unsafe {
            GetIpAddrTable(buf.as_mut_ptr(), &mut size, 0)
        };

        if ret == 0 {
            let num_entries = unsafe { *(buf.as_ptr() as *const u32) };
            let row_ptr = unsafe { buf.as_ptr().add(4) as *const MibIpAddrRow };
            
            for i in 0..num_entries {
                let row = unsafe { *row_ptr.add(i as usize) };
                let ip_addr = std::net::Ipv4Addr::from(u32::from_be(row.dw_addr));
                let ip_str = ip_addr.to_string();
                
                if !ip_addr.is_loopback() 
                    && !ip_addr.is_unspecified() 
                    && !ip_str.starts_with("169.254.") 
                    && !ip_str.starts_with("0.") 
                {
                    ips.push(ip_str);
                }
            }
        }
    }
    build_response(200, "OK", "application/json", &json!(ips).to_string())
}

pub fn get_writeback_files(config: &SharedConfig) -> String {
    let writeback_dirs = &config.read().writeback.writeback_dirs;
    let mut list = Vec::new();
    for dir in writeback_dirs {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or_default().to_string();
                if filename.ends_with(".bin") || filename.ends_with(".map") {
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    list.push(json!({
                        "name": filename,
                        "size": size,
                        "path": path.to_str().unwrap_or_default(),
                    }));
                }
            }
        }
    }
    build_response(200, "OK", "application/json", &json!(list).to_string())
}

pub fn post_writeback_clear(config: &SharedConfig, body: &str) -> String {
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(body);
    match parsed {
        Ok(json_body) => {
            let file_path_str = json_body["file_path"].as_str().unwrap_or("");
            if file_path_str.is_empty() || file_path_str.contains("..") {
                return build_response(400, "Bad Request", "text/plain", "Invalid file path");
            }
            
            let path = Path::new(file_path_str);
            let is_safe = config.read().writeback.writeback_dirs.iter().any(|dir| {
                path.starts_with(dir)
            });

            if is_safe && path.exists() {
                if let Err(e) = fs::remove_file(path) {
                    build_response(500, "Internal Server Error", "text/plain", &e.to_string())
                } else {
                    build_response(200, "OK", "text/plain", "Writeback cache cleared successfully")
                }
            } else {
                build_response(403, "Forbidden", "text/plain", "Unauthorized cache cleanup path")
            }
        }
        Err(_) => build_response(400, "Bad Request", "text/plain", "Invalid JSON body"),
    }
}
