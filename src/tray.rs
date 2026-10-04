use std::sync::Arc;
use std::sync::OnceLock;
use tracing::{info, error};
use crate::service_manager::ServiceManager;

pub fn open_browser(url: &str) {
    info!("Membuka browser ke: {}", url);
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", url])
        .spawn();
}

#[cfg(windows)]
#[allow(non_snake_case, dead_code)]
mod win32 {
    pub type HWND = isize;
    pub type HMENU = isize;
    pub type HICON = isize;
    pub type HINSTANCE = isize;
    pub type HCURSOR = isize;
    pub type HBRUSH = isize;
    pub type BOOL = i32;

    pub const WM_APP: u32 = 0x8000;
    pub const WM_TRAY_CALLBACK: u32 = WM_APP + 100;
    pub const WM_COMMAND: u32 = 0x0111;
    pub const WM_RBUTTONUP: u32 = 0x0205;
    pub const WM_LBUTTONDBLCLK: u32 = 0x0203;
    pub const WM_CONTEXTMENU: u32 = 0x007B;
    pub const WM_DESTROY: u32 = 0x0002;
    pub const WM_NULL: u32 = 0x0000;

    pub const NIM_ADD: u32 = 0x00000000;
    pub const NIM_MODIFY: u32 = 0x00000001;
    pub const NIM_DELETE: u32 = 0x00000002;

    pub const NIF_MESSAGE: u32 = 0x00000001;
    pub const NIF_ICON: u32 = 0x00000002;
    pub const NIF_TIP: u32 = 0x00000004;

    pub const MF_STRING: u32 = 0x00000000;
    pub const MF_GRAYED: u32 = 0x00000001;
    pub const MF_CHECKED: u32 = 0x00000008;
    pub const MF_POPUP: u32 = 0x00000010;
    pub const MF_SEPARATOR: u32 = 0x00000800;

    pub const TPM_LEFTALIGN: u32 = 0x0000;
    pub const TPM_BOTTOMALIGN: u32 = 0x0020;
    pub const TPM_RIGHTBUTTON: u32 = 0x0002;

    pub const IDI_APPLICATION: usize = 32512;
    pub const SW_HIDE: i32 = 0;
    pub const SW_SHOW: i32 = 5;

    #[repr(C)]
    pub struct POINT {
        pub x: i32,
        pub y: i32,
    }

    #[repr(C)]
    pub struct MSG {
        pub hwnd: HWND,
        pub message: u32,
        pub wParam: usize,
        pub lParam: isize,
        pub time: u32,
        pub pt: POINT,
    }

    #[repr(C)]
    pub struct WNDCLASSW {
        pub style: u32,
        pub lpfnWndProc: Option<unsafe extern "system" fn(HWND, u32, usize, isize) -> isize>,
        pub cbClsExtra: i32,
        pub cbWndExtra: i32,
        pub hInstance: HINSTANCE,
        pub hIcon: HICON,
        pub hCursor: HCURSOR,
        pub hbrBackground: HBRUSH,
        pub lpszMenuName: *const u16,
        pub lpszClassName: *const u16,
    }

    #[repr(C)]
    pub struct NOTIFYICONDATAW {
        pub cbSize: u32,
        pub hWnd: HWND,
        pub uID: u32,
        pub uFlags: u32,
        pub uCallbackMessage: u32,
        pub hIcon: HICON,
        pub szTip: [u16; 128],
        pub dwState: u32,
        pub dwStateMask: u32,
        pub szInfo: [u16; 256],
        pub uTimeoutOrVersion: u32,
        pub szInfoTitle: [u16; 64],
        pub dwInfoFlags: u32,
        pub guidItem: [u8; 16],
        pub hBalloonIcon: HICON,
    }

    #[link(name = "user32")]
    #[link(name = "kernel32")]
    #[link(name = "shell32")]
    extern "system" {
        pub fn GetModuleHandleW(lpModuleName: *const u16) -> HINSTANCE;
        pub fn RegisterClassW(lpWndClass: *const WNDCLASSW) -> u16;
        pub fn CreateWindowExW(
            dwExStyle: u32,
            lpClassName: *const u16,
            lpWindowName: *const u16,
            dwStyle: u32,
            X: i32,
            Y: i32,
            nWidth: i32,
            nHeight: i32,
            hWndParent: HWND,
            hMenu: HMENU,
            hInstance: HINSTANCE,
            lpParam: *mut std::ffi::c_void,
        ) -> HWND;
        pub fn DefWindowProcW(hWnd: HWND, Msg: u32, wParam: usize, lParam: isize) -> isize;
        pub fn PostQuitMessage(nExitCode: i32);
        pub fn GetMessageW(lpMsg: *mut MSG, hWnd: HWND, wMsgFilterMin: u32, wMsgFilterMax: u32) -> BOOL;
        pub fn TranslateMessage(lpMsg: *const MSG) -> BOOL;
        pub fn DispatchMessageW(lpMsg: *const MSG) -> isize;
        pub fn CreatePopupMenu() -> HMENU;
        pub fn AppendMenuW(hMenu: HMENU, uFlags: u32, uIDNewItem: usize, lpNewItem: *const u16) -> BOOL;
        pub fn DestroyMenu(hMenu: HMENU) -> BOOL;
        pub fn TrackPopupMenu(
            hMenu: HMENU,
            uFlags: u32,
            x: i32,
            y: i32,
            nReserved: i32,
            hWnd: HWND,
            prcRect: *const std::ffi::c_void,
        ) -> BOOL;
        pub fn GetCursorPos(lpPoint: *mut POINT) -> BOOL;
        pub fn SetForegroundWindow(hWnd: HWND) -> BOOL;
        pub fn PostMessageW(hWnd: HWND, Msg: u32, wParam: usize, lParam: isize) -> BOOL;
        pub fn LoadIconW(hInstance: HINSTANCE, lpIconName: *const u16) -> HICON;
        pub fn Shell_NotifyIconW(dwMessage: u32, lpData: *const NOTIFYICONDATAW) -> BOOL;
        pub fn GetConsoleWindow() -> HWND;
        pub fn IsWindowVisible(hWnd: HWND) -> BOOL;
        pub fn ShowWindow(hWnd: HWND, nCmdShow: i32) -> BOOL;
    }
}

#[cfg(windows)]
struct TrayAppState {
    service_manager: Arc<ServiceManager>,
    dashboard_url: String,
    runtime_handle: tokio::runtime::Handle,
}

#[cfg(windows)]
static TRAY_STATE: OnceLock<TrayAppState> = OnceLock::new();

const ID_DASHBOARD: usize = 1001;

const ID_START_ALL: usize = 1002;
const ID_STOP_ALL: usize = 1003;
const ID_RESTART_ALL: usize = 1004;

const ID_START_DHCP: usize = 1011;
const ID_STOP_DHCP: usize = 1012;
const ID_RESTART_DHCP: usize = 1013;

const ID_START_TFTP: usize = 1021;
const ID_STOP_TFTP: usize = 1022;
const ID_RESTART_TFTP: usize = 1023;

const ID_START_ISCSI: usize = 1031;
const ID_STOP_ISCSI: usize = 1032;
const ID_RESTART_ISCSI: usize = 1033;

const ID_TOGGLE_CONSOLE: usize = 1040;
const ID_EXIT: usize = 1041;

#[cfg(windows)]
fn to_wide_str(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
pub fn toggle_console_window() {
    unsafe {
        let hwnd = win32::GetConsoleWindow();
        if hwnd != 0 {
            if win32::IsWindowVisible(hwnd) != 0 {
                info!("Menyembunyikan jendela konsol.");
                win32::ShowWindow(hwnd, win32::SW_HIDE);
            } else {
                info!("Menampilkan jendela konsol.");
                win32::ShowWindow(hwnd, win32::SW_SHOW);
            }
        }
    }
}

#[cfg(not(windows))]
pub fn toggle_console_window() {
    info!("Toggle console window hanya tersedia di platform Windows.");
}

#[cfg(windows)]
unsafe fn create_notify_icon_data(hwnd: win32::HWND) -> win32::NOTIFYICONDATAW {
    let mut nid: win32::NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = std::mem::size_of::<win32::NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uFlags = win32::NIF_MESSAGE | win32::NIF_ICON | win32::NIF_TIP;
    nid.uCallbackMessage = win32::WM_TRAY_CALLBACK;
    
    // Load standard application icon directly from Windows without requiring .rc resource
    nid.hIcon = win32::LoadIconW(0, win32::IDI_APPLICATION as *const u16);
    
    let tip = "Simple-iSCSI Diskless Server".encode_utf16().collect::<Vec<u16>>();
    for (i, &c) in tip.iter().enumerate().take(127) {
        nid.szTip[i] = c;
    }
    nid
}

#[cfg(windows)]
unsafe fn remove_tray_icon(hwnd: win32::HWND) {
    let mut nid: win32::NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = std::mem::size_of::<win32::NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    win32::Shell_NotifyIconW(win32::NIM_DELETE, &nid);
}

#[cfg(windows)]
unsafe fn show_tray_menu(hwnd: win32::HWND) {
    let state = match TRAY_STATE.get() {
        Some(s) => s,
        None => return,
    };

    let status = state.service_manager.get_status();

    let hmenu_root = win32::CreatePopupMenu();
    if hmenu_root == 0 {
        return;
    }

    let title = to_wide_str("=== Simple-iSCSI Server ===");
    let m_dash = to_wide_str("🌐 Buka Web Dashboard");
    let m_cons = to_wide_str("🪟 Sembunyikan / Tampilkan Console");
    let m_exit = to_wide_str("❌ Keluar Aplikasi");

    // Header & Dashboard
    win32::AppendMenuW(hmenu_root, win32::MF_STRING | win32::MF_GRAYED, 0, title.as_ptr());
    win32::AppendMenuW(hmenu_root, win32::MF_SEPARATOR, 0, std::ptr::null());
    win32::AppendMenuW(hmenu_root, win32::MF_STRING, ID_DASHBOARD, m_dash.as_ptr());
    win32::AppendMenuW(hmenu_root, win32::MF_SEPARATOR, 0, std::ptr::null());

    // 1. Submenu Semua Layanan
    let hmenu_all = win32::CreatePopupMenu();
    let m_all_title = to_wide_str("⚡ Kontrol Semua Layanan");
    let m_all_start = to_wide_str("▶️ Jalankan Semua Layanan (Enable All)");
    let m_all_stop = to_wide_str("⏹️ Hentikan Semua Layanan (Disable All)");
    let m_all_restart = to_wide_str("🔄 Restart Semua Layanan");
    win32::AppendMenuW(hmenu_all, win32::MF_STRING, ID_START_ALL, m_all_start.as_ptr());
    win32::AppendMenuW(hmenu_all, win32::MF_STRING, ID_STOP_ALL, m_all_stop.as_ptr());
    win32::AppendMenuW(hmenu_all, win32::MF_STRING, ID_RESTART_ALL, m_all_restart.as_ptr());
    win32::AppendMenuW(hmenu_root, win32::MF_POPUP, hmenu_all as usize, m_all_title.as_ptr());

    win32::AppendMenuW(hmenu_root, win32::MF_SEPARATOR, 0, std::ptr::null());

    // 2. Submenu DHCP Server
    let dhcp_icon = match status.dhcp {
        crate::service_manager::ServiceState::Running => "🟢",
        crate::service_manager::ServiceState::Stopped => "🔴",
        crate::service_manager::ServiceState::Restarting => "🟡",
        crate::service_manager::ServiceState::Error => "❌",
    };
    let dhcp_title_str = format!("{} DHCP Server (Port 67)", dhcp_icon);
    let m_dhcp_title = to_wide_str(&dhcp_title_str);
    let hmenu_dhcp = win32::CreatePopupMenu();
    let m_dhcp_start = to_wide_str("▶️ Aktifkan (Enable)");
    let m_dhcp_stop = to_wide_str("⏹️ Matikan (Disable)");
    let m_dhcp_restart = to_wide_str("🔄 Restart Layanan");
    win32::AppendMenuW(hmenu_dhcp, win32::MF_STRING, ID_START_DHCP, m_dhcp_start.as_ptr());
    win32::AppendMenuW(hmenu_dhcp, win32::MF_STRING, ID_STOP_DHCP, m_dhcp_stop.as_ptr());
    win32::AppendMenuW(hmenu_dhcp, win32::MF_STRING, ID_RESTART_DHCP, m_dhcp_restart.as_ptr());
    win32::AppendMenuW(hmenu_root, win32::MF_POPUP, hmenu_dhcp as usize, m_dhcp_title.as_ptr());

    // 3. Submenu TFTP Server
    let tftp_icon = match status.tftp {
        crate::service_manager::ServiceState::Running => "🟢",
        crate::service_manager::ServiceState::Stopped => "🔴",
        crate::service_manager::ServiceState::Restarting => "🟡",
        crate::service_manager::ServiceState::Error => "❌",
    };
    let tftp_title_str = format!("{} TFTP Server (Port 69)", tftp_icon);
    let m_tftp_title = to_wide_str(&tftp_title_str);
    let hmenu_tftp = win32::CreatePopupMenu();
    let m_tftp_start = to_wide_str("▶️ Aktifkan (Enable)");
    let m_tftp_stop = to_wide_str("⏹️ Matikan (Disable)");
    let m_tftp_restart = to_wide_str("🔄 Restart Layanan");
    win32::AppendMenuW(hmenu_tftp, win32::MF_STRING, ID_START_TFTP, m_tftp_start.as_ptr());
    win32::AppendMenuW(hmenu_tftp, win32::MF_STRING, ID_STOP_TFTP, m_tftp_stop.as_ptr());
    win32::AppendMenuW(hmenu_tftp, win32::MF_STRING, ID_RESTART_TFTP, m_tftp_restart.as_ptr());
    win32::AppendMenuW(hmenu_root, win32::MF_POPUP, hmenu_tftp as usize, m_tftp_title.as_ptr());

    // 4. Submenu iSCSI Listener
    let iscsi_icon = match status.iscsi {
        crate::service_manager::ServiceState::Running => "🟢",
        crate::service_manager::ServiceState::Stopped => "🔴",
        crate::service_manager::ServiceState::Restarting => "🟡",
        crate::service_manager::ServiceState::Error => "❌",
    };
    let iscsi_title_str = format!("{} iSCSI Target (Port 3260)", iscsi_icon);
    let m_iscsi_title = to_wide_str(&iscsi_title_str);
    let hmenu_iscsi = win32::CreatePopupMenu();
    let m_iscsi_start = to_wide_str("▶️ Aktifkan (Enable)");
    let m_iscsi_stop = to_wide_str("⏹️ Matikan (Disable)");
    let m_iscsi_restart = to_wide_str("🔄 Restart Layanan");
    win32::AppendMenuW(hmenu_iscsi, win32::MF_STRING, ID_START_ISCSI, m_iscsi_start.as_ptr());
    win32::AppendMenuW(hmenu_iscsi, win32::MF_STRING, ID_STOP_ISCSI, m_iscsi_stop.as_ptr());
    win32::AppendMenuW(hmenu_iscsi, win32::MF_STRING, ID_RESTART_ISCSI, m_iscsi_restart.as_ptr());
    win32::AppendMenuW(hmenu_root, win32::MF_POPUP, hmenu_iscsi as usize, m_iscsi_title.as_ptr());

    // Separator, Console Toggle & Exit
    win32::AppendMenuW(hmenu_root, win32::MF_SEPARATOR, 0, std::ptr::null());
    win32::AppendMenuW(hmenu_root, win32::MF_STRING, ID_TOGGLE_CONSOLE, m_cons.as_ptr());
    win32::AppendMenuW(hmenu_root, win32::MF_SEPARATOR, 0, std::ptr::null());
    win32::AppendMenuW(hmenu_root, win32::MF_STRING, ID_EXIT, m_exit.as_ptr());

    let mut pt = win32::POINT { x: 0, y: 0 };
    win32::GetCursorPos(&mut pt);
    win32::SetForegroundWindow(hwnd);
    win32::TrackPopupMenu(
        hmenu_root,
        win32::TPM_BOTTOMALIGN | win32::TPM_LEFTALIGN | win32::TPM_RIGHTBUTTON,
        pt.x,
        pt.y,
        0,
        hwnd,
        std::ptr::null(),
    );
    win32::PostMessageW(hwnd, win32::WM_NULL, 0, 0);
    win32::DestroyMenu(hmenu_root);
}

#[cfg(windows)]
unsafe extern "system" fn wnd_proc(
    hwnd: win32::HWND,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    match msg {
        win32::WM_TRAY_CALLBACK => {
            let event = lparam as u32;
            if event == win32::WM_RBUTTONUP || event == win32::WM_CONTEXTMENU {
                show_tray_menu(hwnd);
            } else if event == win32::WM_LBUTTONDBLCLK {
                if let Some(state) = TRAY_STATE.get() {
                    open_browser(&state.dashboard_url);
                }
            }
            0
        }
        win32::WM_COMMAND => {
            let id = wparam & 0xFFFF;
            if let Some(state) = TRAY_STATE.get() {
                match id {
                    ID_DASHBOARD => {
                        open_browser(&state.dashboard_url);
                    }
                    ID_START_ALL => {
                        let sm = state.service_manager.clone();
                        state.runtime_handle.spawn(async move {
                            sm.start_all().await;
                        });
                    }
                    ID_STOP_ALL => {
                        let sm = state.service_manager.clone();
                        state.runtime_handle.spawn(async move {
                            sm.stop_all().await;
                        });
                    }
                    ID_RESTART_ALL => {
                        let sm = state.service_manager.clone();
                        state.runtime_handle.spawn(async move {
                            sm.restart_all().await;
                        });
                    }
                    ID_START_DHCP => {
                        let sm = state.service_manager.clone();
                        state.runtime_handle.spawn(async move {
                            sm.start_dhcp().await;
                        });
                    }
                    ID_STOP_DHCP => {
                        let sm = state.service_manager.clone();
                        state.runtime_handle.spawn(async move {
                            sm.stop_dhcp().await;
                        });
                    }
                    ID_RESTART_DHCP => {
                        let sm = state.service_manager.clone();
                        state.runtime_handle.spawn(async move {
                            sm.restart_dhcp().await;
                        });
                    }
                    ID_START_TFTP => {
                        let sm = state.service_manager.clone();
                        state.runtime_handle.spawn(async move {
                            sm.start_tftp().await;
                        });
                    }
                    ID_STOP_TFTP => {
                        let sm = state.service_manager.clone();
                        state.runtime_handle.spawn(async move {
                            sm.stop_tftp().await;
                        });
                    }
                    ID_RESTART_TFTP => {
                        let sm = state.service_manager.clone();
                        state.runtime_handle.spawn(async move {
                            sm.restart_tftp().await;
                        });
                    }
                    ID_START_ISCSI => {
                        let sm = state.service_manager.clone();
                        state.runtime_handle.spawn(async move {
                            sm.start_iscsi().await;
                        });
                    }
                    ID_STOP_ISCSI => {
                        let sm = state.service_manager.clone();
                        state.runtime_handle.spawn(async move {
                            sm.stop_iscsi().await;
                        });
                    }
                    ID_RESTART_ISCSI => {
                        let sm = state.service_manager.clone();
                        state.runtime_handle.spawn(async move {
                            sm.restart_iscsi().await;
                        });
                    }
                    ID_TOGGLE_CONSOLE => {
                        toggle_console_window();
                    }
                    ID_EXIT => {
                        info!("Menutup aplikasi Simple-Iscsi dari System Tray.");
                        remove_tray_icon(hwnd);
                        std::process::exit(0);
                    }
                    _ => {}
                }
            }
            0
        }
        win32::WM_DESTROY => {
            remove_tray_icon(hwnd);
            win32::PostQuitMessage(0);
            0
        }
        _ => win32::DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(windows)]
pub fn run_tray(
    service_manager: Arc<ServiceManager>,
    dashboard_url: String,
    runtime_handle: tokio::runtime::Handle,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Menginisialisasi Windows System Tray GUI secara native...");

    let _ = TRAY_STATE.set(TrayAppState {
        service_manager,
        dashboard_url,
        runtime_handle,
    });

    unsafe {
        let hinstance = win32::GetModuleHandleW(std::ptr::null());
        let class_name = to_wide_str("SimpleIscsiTrayWindowClass");

        let mut wc: win32::WNDCLASSW = std::mem::zeroed();
        wc.lpfnWndProc = Some(wnd_proc);
        wc.hInstance = hinstance;
        wc.lpszClassName = class_name.as_ptr();

        win32::RegisterClassW(&wc);

        let window_title = to_wide_str("Simple-iSCSI Tray Helper");
        let hwnd = win32::CreateWindowExW(
            0,
            class_name.as_ptr(),
            window_title.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            hinstance,
            std::ptr::null_mut(),
        );

        if hwnd == 0 {
            error!("Gagal membuat hidden window untuk System Tray");
            return Err("Gagal membuat window tray".into());
        }

        let nid = create_notify_icon_data(hwnd);
        let res = win32::Shell_NotifyIconW(win32::NIM_ADD, &nid);
        if res == 0 {
            error!("Gagal menambahkan ikon ke System Tray (Shell_NotifyIconW gagal)");
            return Err("Shell_NotifyIconW gagal".into());
        }

        info!("✅ Windows System Tray aktif di area notifikasi taskbar (Native Win32).");

        // Windows Message Pump Loop
        let mut msg: win32::MSG = std::mem::zeroed();
        while win32::GetMessageW(&mut msg, 0, 0, 0) > 0 {
            win32::TranslateMessage(&msg);
            win32::DispatchMessageW(&msg);
        }

        remove_tray_icon(hwnd);
    }

    Ok(())
}

#[cfg(not(windows))]
pub fn run_tray(
    _service_manager: Arc<ServiceManager>,
    _dashboard_url: String,
    _runtime_handle: tokio::runtime::Handle,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("System Tray hanya aktif di platform Windows.");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
