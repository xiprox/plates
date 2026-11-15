//! The Win32 calls the rack makes through its window's native handle, for what GPUI's window
//! options leave out: setting the window's position, keeping clicks from activating it, and
//! reading the cursor, the monitors, and the foreground window.
//! record/areas/rack/log.md holds what the window test found for each.

use std::{
    ffi::c_void,
    sync::atomic::{AtomicBool, Ordering},
};

use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::{
        Dwm::{
            DWM_WINDOW_CORNER_PREFERENCE, DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE,
            DWMWCP_DONOTROUND, DWMWCP_ROUND, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
        },
        Gdi::{
            ClientToScreen, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTONULL,
            MONITORINFO, MonitorFromPoint,
        },
    },
    UI::{
        Controls::MARGINS,
        Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON},
        Shell::{DefSubclassProc, SetWindowSubclass},
        WindowsAndMessaging::{
            GWL_EXSTYLE, GetClientRect, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW,
            GetWindowRect, MA_NOACTIVATE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
            SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow, SetWindowLongPtrW,
            SetWindowPos, WM_MOUSEACTIVATE, WM_NCCALCSIZE, WS_EX_NOACTIVATE,
        },
    },
};

use crate::motion::{Edges, Rect};

static ALLOW_ACTIVATION: AtomicBool = AtomicBool::new(false);

fn hwnd(h: isize) -> HWND {
    HWND(h as *mut c_void)
}

fn rect(r: RECT) -> Rect {
    Rect {
        left: r.left as f64,
        top: r.top as f64,
        right: r.right as f64,
        bottom: r.bottom as f64,
    }
}

pub fn hwnd_of(window: &gpui_kit::Window) -> isize {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = HasWindowHandle::window_handle(window)
        .expect("the rack's window should have a native handle");
    match handle.as_raw() {
        RawWindowHandle::Win32(h) => h.hwnd.get(),
        other => panic!("the rack's window should have a Win32 handle, not {other:?}"),
    }
}

// This subclass sees the rack's window messages before GPUI's window procedure does.
// (read in gpui-pre-windows 0.3.12, src/events.rs, WindowsWindowInner::handle_msg and
// handle_calc_client_size)
unsafe extern "system" fn subclass_proc(
    h: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    match msg {
        // GPUI answers every WM_MOUSEACTIVATE with MA_ACTIVATE, so a click on the rack would
        // take keyboard focus from the app you're in. The subclass answers MA_NOACTIVATE
        // instead, except while the inline field wants focus.
        WM_MOUSEACTIVATE if !ALLOW_ACTIVATION.load(Ordering::Relaxed) => {
            return LRESULT(MA_NOACTIVATE as isize);
        }
        // GPUI keeps Windows' 8px resize frame on the left, right, and bottom of a window it
        // draws without a title bar. The frame is invisible but takes clicks, and it would hold
        // a docked rack 8px off the screen edge, so the whole window is the client area.
        WM_NCCALCSIZE if wparam.0 != 0 => return LRESULT(0),
        _ => {}
    }
    unsafe { DefSubclassProc(h, msg, wparam, lparam) }
}

/// Makes GPUI's window the rack's: the subclass in place, no frame, no activation on a click, and
/// DWM's shadow outside it. The client area stays where it was on screen.
pub fn take_over(h: isize) {
    let client = client_rect(h);
    unsafe {
        let _ = SetWindowSubclass(hwnd(h), Some(subclass_proc), 1, 0);
    }
    set_noactivate(h, true);
    set_bounds(h, client);
    dwm_shadow(h);
}

fn set_noactivate(h: isize, on: bool) {
    let bit = WS_EX_NOACTIVATE.0 as isize;
    unsafe {
        let style = GetWindowLongPtrW(hwnd(h), GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd(h),
            GWL_EXSTYLE,
            if on { style | bit } else { style & !bit },
        );
        let _ = SetWindowPos(
            hwnd(h),
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// The inline field needs keyboard focus, so while it's open the rack lets itself be activated.
pub fn allow_activation(h: isize, allowed: bool) {
    ALLOW_ACTIVATION.store(allowed, Ordering::Relaxed);
    set_noactivate(h, !allowed);
}

// DWM draws a window's shadow outside it for a window whose frame reaches 1px into its client
// area, so the rack gets the mockups' shadow without a margin that would take clicks meant for
// the windows below, and DWM's own 1px border is turned off, since the rack draws its own.
fn dwm_shadow(h: isize) {
    // DWMWA_COLOR_NONE, from dwmapi.h.
    const COLOR_NONE: u32 = 0xFFFF_FFFE;
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd(h),
            DWMWA_BORDER_COLOR,
            &COLOR_NONE as *const _ as *const c_void,
            size_of::<u32>() as u32,
        );
    }
    set_shadow(h, true);
}

/// Turns DWM's shadow on or off. DWM draws it around the window's whole rectangle, with 8px
/// corners, so it fits the rack and the menu, and looks wrong around the docked strip, which is
/// rounded on its inner side only and widens to list what needs you. Without a frame reaching
/// into the client area, DWM draws no shadow (record/areas/rack/log.md).
pub fn set_shadow(h: isize, on: bool) {
    let width = i32::from(on);
    let margins = MARGINS {
        cxLeftWidth: width,
        cxRightWidth: width,
        cyTopHeight: width,
        cyBottomHeight: width,
    };
    unsafe {
        let _ = DwmExtendFrameIntoClientArea(hwnd(h), &margins);
    }
    set_round_corners(h, on);
}

/// DWM rounds a window's corners, and its shadow, at 8px. The rack draws its 14px corners inside
/// that, and the docked strip, which is flush with the screen edge, takes square corners.
fn set_round_corners(h: isize, round: bool) {
    let pref: DWM_WINDOW_CORNER_PREFERENCE = if round {
        DWMWCP_ROUND
    } else {
        DWMWCP_DONOTROUND
    };
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd(h),
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const _ as *const c_void,
            size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );
    }
}

pub fn window_rect(h: isize) -> Rect {
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd(h), &mut r);
    }
    rect(r)
}

fn client_rect(h: isize) -> Rect {
    let mut r = RECT::default();
    let mut origin = POINT::default();
    unsafe {
        let _ = GetClientRect(hwnd(h), &mut r);
        let _ = ClientToScreen(hwnd(h), &mut origin);
    }
    Rect {
        left: origin.x as f64,
        top: origin.y as f64,
        right: (origin.x + r.right) as f64,
        bottom: (origin.y + r.bottom) as f64,
    }
}

pub fn set_bounds(h: isize, r: Rect) {
    unsafe {
        let _ = SetWindowPos(
            hwnd(h),
            None,
            r.left.round() as i32,
            r.top.round() as i32,
            (r.right - r.left).round() as i32,
            (r.bottom - r.top).round() as i32,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        );
    }
}

pub fn cursor_pos() -> (f64, f64) {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    (p.x as f64, p.y as f64)
}

// GetAsyncKeyState reads the physical buttons, so with the buttons swapped for a left hand these
// read the wrong ones.
pub fn left_button_down() -> bool {
    unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0 }
}

pub fn right_button_down() -> bool {
    unsafe { GetAsyncKeyState(VK_RBUTTON.0 as i32) < 0 }
}

/// The work area of the monitor at `p`, and which of its edges have another monitor beyond them.
pub fn edges_at(p: (f64, f64)) -> Edges {
    unsafe {
        let pt = POINT {
            x: p.0 as i32,
            y: p.1 as i32,
        };
        let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(monitor, &mut info);
        let (work, mon) = (info.rcWork, info.rcMonitor);
        let cx = pt.x.clamp(mon.left, mon.right - 1);
        let cy = pt.y.clamp(mon.top, mon.bottom - 1);
        let other = |x: i32, y: i32| {
            let found = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONULL);
            !found.is_invalid() && found != monitor
        };
        Edges {
            area: rect(work),
            open_left: other(mon.left - 1, cy),
            open_right: other(mon.right, cy),
            open_top: other(cx, mon.top - 1),
            open_bottom: other(cx, mon.bottom),
        }
    }
}

pub fn foreground_window() -> isize {
    unsafe { GetForegroundWindow().0 as isize }
}

pub fn set_foreground(h: isize) -> bool {
    unsafe { SetForegroundWindow(hwnd(h)).as_bool() }
}
