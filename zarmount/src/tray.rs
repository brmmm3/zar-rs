//! System tray icon and message loop for the zarmount daemon.
//!
//! Per `zarmount.md`:
//! The tray icon has menu:
//! - Entry for each mounted archive
//!   - "Open" opens file explorer with destination path / drive
//!   - "Map drive letter" if not already mapped
//!   - "Unmount" unmounts archive
//! - Settings
//! - About
//! - Exit unmounts all mounted archives and exits. Ensure to exit elevated process too.

use std::process::ExitCode;
use std::sync::OnceLock;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CW_USEDEFAULT, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DispatchMessageW, GetCursorPos, GetMessageW, IDC_ARROW, IDI_APPLICATION, LoadCursorW,
    LoadIconW, MF_POPUP, MF_SEPARATOR, MF_STRING, MSG, PostQuitMessage, RegisterClassExW,
    SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage,
    WINDOW_EX_STYLE, WM_DESTROY, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSEXW, WS_OVERLAPPEDWINDOW,
};
use windows::core::PCWSTR;

/// A menu entry for a mounted archive.
pub struct MenuEntry {
    pub label: String,
    pub has_drive_letter: bool,
}

pub mod cmd {
    pub const OPEN_BASE: u32 = 1000;
    pub const MAP_BASE: u32 = 2000;
    pub const UNMOUNT_BASE: u32 = 3000;
    pub const SETTINGS: u32 = 997;
    pub const ABOUT: u32 = 998;
    pub const EXIT: u32 = 999;
}

static GET_ENTRIES: OnceLock<Box<dyn Fn() -> Vec<MenuEntry> + Send + Sync>> = OnceLock::new();
static ON_COMMAND: OnceLock<Box<dyn Fn(u32) + Send + Sync>> = OnceLock::new();

const WM_TRAY: u32 = 0x8001;
const ICON_ID: u32 = 1;

/// Run the tray icon message loop.
#[allow(clippy::manual_dangling_ptr)]
pub fn run(
    get_entries: impl Fn() -> Vec<MenuEntry> + Send + Sync + 'static,
    on_command: impl Fn(u32) + Send + Sync + 'static,
    on_ready: impl Fn() + Send + 'static,
) -> ExitCode {
    let _ = GET_ENTRIES.set(Box::new(get_entries));
    let _ = ON_COMMAND.set(Box::new(on_command));

    unsafe {
        let hinstance: HINSTANCE = GetModuleHandleW(None).unwrap().into();

        let icon = LoadIconW(Some(hinstance), PCWSTR(1 as *const u16))
            .unwrap_or_else(|_| LoadIconW(None, IDI_APPLICATION).expect("LoadIconW failed"));
        let cursor = LoadCursorW(None, IDC_ARROW).expect("LoadCursorW failed");

        let class_name_wide: Vec<u16> = "zarmount_daemon"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let window_name_wide: Vec<u16> = "zarmount"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance,
            hIcon: icon,
            hCursor: cursor,
            lpszClassName: PCWSTR(class_name_wide.as_ptr()),
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_name_wide.as_ptr()),
            PCWSTR(window_name_wide.as_ptr()),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            None,
            None,
            Some(hinstance),
            None,
        )
        .expect("CreateWindowExW failed");

        // Register daemon HWND for UAC elevation if needed
        crate::elevate::set_daemon_hwnd(hwnd);

        // Notify caller that the window is ready
        on_ready();

        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: ICON_ID,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            hIcon: icon,
            ..Default::default()
        };
        let tip: Vec<u16> = "zarmount\0".encode_utf16().collect();
        nid.szTip[..tip.len()].copy_from_slice(&tip);

        let _ = Shell_NotifyIconW(NIM_ADD, &nid);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }

    ExitCode::SUCCESS
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    _wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TRAY => {
            let event = lparam.0 as u32;
            if event == WM_RBUTTONUP || event == WM_LBUTTONUP {
                show_menu(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, _wparam, lparam) },
    }
}

fn show_menu(hwnd: HWND) {
    let get_entries = match GET_ENTRIES.get() {
        Some(f) => f,
        None => return,
    };
    let entries = get_entries();

    unsafe {
        let menu = CreatePopupMenu().expect("CreatePopupMenu failed");

        for (i, entry) in entries.iter().enumerate() {
            let sub = CreatePopupMenu().expect("CreatePopupMenu failed");
            let open_id = cmd::OPEN_BASE + (i as u32) * 10;
            let map_id = cmd::MAP_BASE + (i as u32) * 10;
            let unmount_id = cmd::UNMOUNT_BASE + (i as u32) * 10;

            let open_w: Vec<u16> = "Open\0".encode_utf16().collect();
            let _ = AppendMenuW(sub, MF_STRING, open_id as usize, PCWSTR(open_w.as_ptr()));

            // "Map drive letter" if not already mapped
            if !entry.has_drive_letter {
                let map_w: Vec<u16> = "Map drive letter\0".encode_utf16().collect();
                let _ = AppendMenuW(sub, MF_STRING, map_id as usize, PCWSTR(map_w.as_ptr()));
            }

            let unmount_w: Vec<u16> = "Unmount\0".encode_utf16().collect();
            let _ = AppendMenuW(sub, MF_STRING, unmount_id as usize, PCWSTR(unmount_w.as_ptr()));

            let label_w: Vec<u16> = entry
                .label
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let _ = AppendMenuW(menu, MF_POPUP, sub.0 as usize, PCWSTR(label_w.as_ptr()));
        }

        if !entries.is_empty() {
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        }

        let settings_w: Vec<u16> = "Settings\0".encode_utf16().collect();
        let _ = AppendMenuW(menu, MF_STRING, cmd::SETTINGS as usize, PCWSTR(settings_w.as_ptr()));

        let about_w: Vec<u16> = "About\0".encode_utf16().collect();
        let _ = AppendMenuW(menu, MF_STRING, cmd::ABOUT as usize, PCWSTR(about_w.as_ptr()));

        let exit_w: Vec<u16> = "Exit\0".encode_utf16().collect();
        let _ = AppendMenuW(menu, MF_STRING, cmd::EXIT as usize, PCWSTR(exit_w.as_ptr()));

        let mut pos = POINT::default();
        let _ = GetCursorPos(&mut pos);
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD,
            pos.x,
            pos.y,
            None,
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);

        let cmd_id = cmd.0 as u32;
        if cmd_id != 0 {
            ON_COMMAND.get().unwrap()(cmd_id);
        }
    }
}
