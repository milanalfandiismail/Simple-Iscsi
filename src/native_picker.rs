use std::path::{Path, PathBuf};

// Picker native bisa muncul di belakang browser karena proses ini tray app tanpa
// window foreground. Di Windows dialog dipaksa topmost selama terbuka.

#[cfg(windows)]
static PICKER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(windows)]
pub fn pick_file(title: &str, extensions: &[&str]) -> Option<PathBuf> {
    let Ok(_guard) = PICKER_LOCK.try_lock() else {
        return None;
    };
    run_dialog(|| {
        rfd::FileDialog::new()
            .add_filter("VHD", extensions)
            .set_title(title)
            .pick_file()
    })
}

#[cfg(windows)]
pub fn pick_folder(title: &str, start: &Path) -> Option<PathBuf> {
    let Ok(_guard) = PICKER_LOCK.try_lock() else {
        return None;
    };
    run_dialog(|| {
        rfd::FileDialog::new()
            .set_directory(start)
            .set_title(title)
            .pick_folder()
    })
}

#[cfg(windows)]
fn run_dialog<F: FnOnce() -> Option<PathBuf>>(show: F) -> Option<PathBuf> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let stop = Arc::new(AtomicBool::new(false));
    let watcher = {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || watch_dialog(&stop))
    };
    let result = show();
    stop.store(true, Ordering::Relaxed);
    let _ = watcher.join();
    result
}

#[cfg(windows)]
fn watch_dialog(stop: &std::sync::atomic::AtomicBool) {
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    // shortcut: polling window milik proses ini, bukan hook pembuatan window;
    // upgrade kalau z-order masih meleset di mesin lain.
    let mut found: isize = 0;
    let mut foreground_tries = 0;
    while !stop.load(Ordering::Relaxed) {
        if found == 0 {
            let mut hwnd: isize = 0;
            unsafe {
                win32::EnumWindows(Some(enum_dialog), &mut hwnd as *mut isize as isize);
            }
            found = hwnd;
        }
        if found != 0 {
            unsafe {
                win32::SetWindowPos(
                    found,
                    win32::HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    win32::SWP_NOMOVE | win32::SWP_NOSIZE | win32::SWP_SHOWWINDOW,
                );
                // Jangan rebut fokus berkali-kali; cukup sampai dialog aktif.
                if foreground_tries < 4 && win32::GetForegroundWindow() != found {
                    win32::SetForegroundWindow(found);
                    foreground_tries += 1;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(120));
    }
}

#[cfg(windows)]
unsafe extern "system" fn enum_dialog(hwnd: isize, lparam: isize) -> i32 {
    if win32::IsWindowVisible(hwnd) == 0 {
        return 1;
    }
    let mut pid: u32 = 0;
    win32::GetWindowThreadProcessId(hwnd, &mut pid);
    if pid != win32::GetCurrentProcessId() {
        return 1;
    }
    let mut buf = [0u16; 64];
    let len = win32::GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
    if len > 0 && String::from_utf16_lossy(&buf[..len as usize]) == "ConsoleWindowClass" {
        return 1;
    }
    *(lparam as *mut isize) = hwnd;
    0
}

#[cfg(not(windows))]
pub fn pick_file(title: &str, extensions: &[&str]) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("VHD", extensions)
        .set_title(title)
        .pick_file()
}

#[cfg(not(windows))]
pub fn pick_folder(title: &str, start: &Path) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_directory(start)
        .set_title(title)
        .pick_folder()
}

#[cfg(windows)]
mod win32 {
    pub const HWND_TOPMOST: isize = -1;
    pub const SWP_NOSIZE: u32 = 0x0001;
    pub const SWP_NOMOVE: u32 = 0x0002;
    pub const SWP_SHOWWINDOW: u32 = 0x0040;

    #[link(name = "user32")]
    extern "system" {
        pub fn EnumWindows(
            lpEnumFunc: Option<unsafe extern "system" fn(isize, isize) -> i32>,
            lParam: isize,
        ) -> i32;
        pub fn GetWindowThreadProcessId(hWnd: isize, lpdwProcessId: *mut u32) -> u32;
        pub fn GetClassNameW(hWnd: isize, lpClassName: *mut u16, nMaxCount: i32) -> i32;
        pub fn IsWindowVisible(hWnd: isize) -> i32;
        pub fn GetForegroundWindow() -> isize;
        pub fn SetWindowPos(
            hWnd: isize,
            hWndInsertAfter: isize,
            X: i32,
            Y: i32,
            cx: i32,
            cy: i32,
            uFlags: u32,
        ) -> i32;
        pub fn SetForegroundWindow(hWnd: isize) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn GetCurrentProcessId() -> u32;
    }
}
