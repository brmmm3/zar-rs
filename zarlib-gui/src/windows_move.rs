//! The subclass is installed on the window's owning thread and holds no borrowed state.
//! Windows invokes its callback with a valid window and removes it when the window is destroyed.

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::InvalidateRect;
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, EnumThreadWindows, GetClassNameW, GetPropW, GetWindowThreadProcessId,
    HTCAPTION, IsWindow, RemovePropW, SWP_NOSIZE, SetPropW, WINDOWPOS, WM_EXITSIZEMOVE,
    WM_NCDESTROY, WM_NCLBUTTONDOWN, WM_PAINT, WM_WINDOWPOSCHANGED,
};

const SUBCLASS_ID: usize = 1;

pub(crate) fn install(context: &eframe::CreationContext<'_>) {
    let Ok(handle) = context.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(window) = handle.as_raw() else {
        return;
    };
    install_window(window.hwnd.get() as HWND);
}

fn install_window(window: HWND) {
    let property = windows_sys::core::w!("zarg.windows_move.installed");
    if !unsafe { GetPropW(window, property) }.is_null() {
        return;
    }
    if unsafe { SetWindowSubclass(window, Some(window_proc), SUBCLASS_ID, 0) } == 0 {
        eprintln!("zarg: could not install Windows move workaround");
    } else if unsafe { SetPropW(window, property, 1usize as _) } == 0 {
        unsafe { RemoveWindowSubclass(window, Some(window_proc), SUBCLASS_ID) };
        eprintln!("zarg: could not mark Windows move workaround as installed");
    }
}

pub(crate) fn install_viewports(frame: &eframe::Frame) {
    let Ok(handle) = frame.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(window) = handle.as_raw() else {
        return;
    };
    install_related_windows(window.hwnd.get() as HWND);
}

fn install_related_windows(window: HWND) {
    let mut class = [0u16; 256];
    let length = unsafe { GetClassNameW(window, class.as_mut_ptr(), class.len() as i32) };
    if length == 0 {
        return;
    }
    let class = &class[..length as usize];
    let thread = unsafe { GetWindowThreadProcessId(window, std::ptr::null_mut()) };
    unsafe {
        EnumThreadWindows(
            thread,
            Some(install_matching_window),
            &class as *const _ as LPARAM,
        )
    };
}

unsafe extern "system" fn install_matching_window(
    window: HWND,
    data: LPARAM,
) -> windows_sys::core::BOOL {
    let class = unsafe { &*(data as *const &[u16]) };
    let mut candidate = [0u16; 256];
    let length = unsafe { GetClassNameW(window, candidate.as_mut_ptr(), candidate.len() as i32) };
    if length > 0 && &candidate[..length as usize] == *class {
        install_window(window);
    }
    1
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    subclass_id: usize,
    moving: usize,
) -> LRESULT {
    if message == WM_NCDESTROY {
        unsafe { RemovePropW(window, windows_sys::core::w!("zarg.windows_move.installed")) };
    }
    if message == WM_PAINT && moving != 0 {
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }
    if message == WM_WINDOWPOSCHANGED
        && moving != 0
        && lparam != 0
        && unsafe { (*(lparam as *const WINDOWPOS)).flags & SWP_NOSIZE != 0 }
    {
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }
    if message == WM_NCLBUTTONDOWN && wparam == HTCAPTION as WPARAM {
        unsafe { SetWindowSubclass(window, Some(window_proc), subclass_id, 1) };
        let result = unsafe { DefSubclassProc(window, message, wparam, lparam) };
        if unsafe { IsWindow(window) } != 0 {
            unsafe {
                SetWindowSubclass(window, Some(window_proc), subclass_id, 0);
                InvalidateRect(window, std::ptr::null(), 0);
            }
        }
        return result;
    }
    if message == WM_EXITSIZEMOVE && moving != 0 {
        unsafe {
            SetWindowSubclass(window, Some(window_proc), subclass_id, 0);
            InvalidateRect(window, std::ptr::null(), 0);
        }
    }
    unsafe { DefSubclassProc(window, message, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, SendMessageW,
    };

    unsafe extern "system" fn count_paints(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _subclass_id: usize,
        count: usize,
    ) -> LRESULT {
        if message == WM_PAINT {
            unsafe { &*(count as *const AtomicUsize) }.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { DefSubclassProc(window, message, wparam, lparam) }
    }

    unsafe extern "system" fn count_position_changes(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _subclass_id: usize,
        count: usize,
    ) -> LRESULT {
        if message == WM_WINDOWPOSCHANGED {
            unsafe { &*(count as *const AtomicUsize) }.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { DefSubclassProc(window, message, wparam, lparam) }
    }

    #[test]
    fn move_defers_paint_and_resumes_on_exit() {
        let count = AtomicUsize::new(0);
        let window = unsafe {
            CreateWindowExW(
                0,
                windows_sys::core::w!("STATIC"),
                windows_sys::core::w!(""),
                0,
                0,
                0,
                100,
                100,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        assert!(!window.is_null());
        unsafe {
            assert_ne!(
                SetWindowSubclass(window, Some(count_paints), 2, &count as *const _ as usize),
                0
            );
            assert_ne!(
                SetWindowSubclass(window, Some(window_proc), SUBCLASS_ID, 0),
                0
            );
            SendMessageW(window, WM_PAINT, 0, 0);
            assert_eq!(count.load(Ordering::Relaxed), 1);

            assert_ne!(
                SetWindowSubclass(window, Some(window_proc), SUBCLASS_ID, 1),
                0
            );
            InvalidateRect(window, std::ptr::null(), 0);
            SendMessageW(window, WM_PAINT, 0, 0);
            assert_eq!(count.load(Ordering::Relaxed), 1);

            SendMessageW(window, WM_EXITSIZEMOVE, 0, 0);
            SendMessageW(window, WM_PAINT, 0, 0);
            assert_eq!(count.load(Ordering::Relaxed), 2);
            assert_ne!(DestroyWindow(window), 0);
        }
    }

    #[test]
    fn installs_on_matching_windows_without_resetting_active_moves() {
        let create_window = |class| unsafe {
            CreateWindowExW(
                0,
                class,
                windows_sys::core::w!(""),
                0,
                0,
                0,
                100,
                100,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        let root = create_window(windows_sys::core::w!("STATIC"));
        let dialog = create_window(windows_sys::core::w!("STATIC"));
        let unrelated = create_window(windows_sys::core::w!("BUTTON"));
        assert!(!root.is_null() && !dialog.is_null() && !unrelated.is_null());

        let count = AtomicUsize::new(0);
        unsafe {
            assert_ne!(
                SetWindowSubclass(dialog, Some(count_paints), 2, &count as *const _ as usize),
                0
            );
            install_related_windows(root);
            let property = windows_sys::core::w!("zarg.windows_move.installed");
            assert!(!GetPropW(root, property).is_null());
            assert!(!GetPropW(dialog, property).is_null());
            assert!(GetPropW(unrelated, property).is_null());

            assert_ne!(
                SetWindowSubclass(dialog, Some(window_proc), SUBCLASS_ID, 1),
                0
            );
            install_related_windows(root);
            SendMessageW(dialog, WM_PAINT, 0, 0);
            assert_eq!(count.load(Ordering::Relaxed), 0);
            SendMessageW(dialog, WM_EXITSIZEMOVE, 0, 0);
            SendMessageW(dialog, WM_PAINT, 0, 0);
            assert_eq!(count.load(Ordering::Relaxed), 1);

            assert_ne!(DestroyWindow(dialog), 0);
            let replacement = create_window(windows_sys::core::w!("STATIC"));
            assert!(!replacement.is_null());
            install_related_windows(root);
            assert!(!GetPropW(replacement, property).is_null());
            assert_ne!(DestroyWindow(replacement), 0);
            assert_ne!(DestroyWindow(unrelated), 0);
            assert_ne!(DestroyWindow(root), 0);
        }
    }

    #[test]
    fn move_only_notifications_are_deferred_but_resize_notifications_pass() {
        let count = AtomicUsize::new(0);
        let window = unsafe {
            CreateWindowExW(
                0,
                windows_sys::core::w!("STATIC"),
                windows_sys::core::w!(""),
                0,
                0,
                0,
                100,
                100,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        assert!(!window.is_null());
        unsafe {
            assert_ne!(
                SetWindowSubclass(
                    window,
                    Some(count_position_changes),
                    2,
                    &count as *const _ as usize,
                ),
                0
            );
            assert_ne!(
                SetWindowSubclass(window, Some(window_proc), SUBCLASS_ID, 1),
                0
            );
            let mut position = WINDOWPOS {
                hwnd: window,
                hwndInsertAfter: std::ptr::null_mut(),
                x: 10,
                y: 10,
                cx: 100,
                cy: 100,
                flags: SWP_NOSIZE,
            };
            SendMessageW(
                window,
                WM_WINDOWPOSCHANGED,
                0,
                &position as *const _ as LPARAM,
            );
            assert_eq!(count.load(Ordering::Relaxed), 0);
            position.flags = 0;
            SendMessageW(
                window,
                WM_WINDOWPOSCHANGED,
                0,
                &position as *const _ as LPARAM,
            );
            assert_eq!(count.load(Ordering::Relaxed), 1);
            SendMessageW(window, WM_EXITSIZEMOVE, 0, 0);
            position.flags = SWP_NOSIZE;
            SendMessageW(
                window,
                WM_WINDOWPOSCHANGED,
                0,
                &position as *const _ as LPARAM,
            );
            assert_eq!(count.load(Ordering::Relaxed), 2);
            assert_ne!(DestroyWindow(window), 0);
        }
    }
}
