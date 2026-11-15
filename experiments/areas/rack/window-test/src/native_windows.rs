// The Win32 calls the window test makes through the rack's native window handle, for what
// GPUI's window options leave out: setting the window's position, keeping clicks from
// activating it, and reading the cursor, the monitors, and the foreground window.

use std::{
    ffi::c_void,
    path::Path,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
    time::{Duration, Instant},
};

use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::{
        Dwm::{
            DWM_WINDOW_CORNER_PREFERENCE, DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE,
            DWMWCP_DONOTROUND, DwmSetWindowAttribute,
        },
        Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, ClientToScreen,
            CreateCompatibleBitmap, CreateCompatibleDC, DEVMODEW, DIB_RGB_COLORS, DeleteDC,
            DeleteObject, ENUM_CURRENT_SETTINGS, EnumDisplaySettingsW, GetDC, GetDIBits,
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTONULL, MONITORINFO,
            MONITORINFOEXW, MonitorFromPoint, ReleaseDC, SRCCOPY, SelectObject,
        },
    },
    UI::{
        Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON},
        Shell::{DefSubclassProc, SetWindowSubclass},
        WindowsAndMessaging::{
            GWL_EXSTYLE, GetClientRect, GetCursorPos, GetForegroundWindow, GetSystemMetrics,
            GetWindowLongPtrW, GetWindowRect, MA_NOACTIVATE, SM_CMONITORS, SWP_FRAMECHANGED,
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER,
            SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, WM_ENTERSIZEMOVE,
            WM_MOUSEACTIVATE, WM_NCCALCSIZE, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
            WS_EX_TOPMOST,
        },
    },
};
use windows::core::PCWSTR;

use crate::motion::{Edges, Rect};

static ALLOW_ACTIVATION: AtomicBool = AtomicBool::new(false);
static ANSWER_MOUSE_ACTIVATE: AtomicBool = AtomicBool::new(true);
pub static MOUSE_ACTIVATIONS: AtomicU32 = AtomicU32::new(0);
pub static OS_MOVE_LOOPS: AtomicU32 = AtomicU32::new(0);

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
        WM_MOUSEACTIVATE => {
            MOUSE_ACTIVATIONS.fetch_add(1, Ordering::Relaxed);
            if ANSWER_MOUSE_ACTIVATE.load(Ordering::Relaxed)
                && !ALLOW_ACTIVATION.load(Ordering::Relaxed)
            {
                return LRESULT(MA_NOACTIVATE as isize);
            }
        }
        // GPUI keeps Windows' 8px resize frame on the left, right, and bottom of a window it
        // draws without a title bar, even one that can't be resized. The frame is invisible but
        // takes clicks, and it would hold a docked rack 8px off the screen edge, so the
        // subclass makes the whole window its client area.
        WM_NCCALCSIZE if wparam.0 != 0 => return LRESULT(0),
        // Windows Snap acts only inside the OS's own move loop, so counting the loops shows
        // whether the OS ever moved the rack itself.
        WM_ENTERSIZEMOVE => {
            OS_MOVE_LOOPS.fetch_add(1, Ordering::Relaxed);
        }
        _ => {}
    }
    unsafe { DefSubclassProc(h, msg, wparam, lparam) }
}

// Installs the subclass and drops GPUI's frame, keeping the client area where it was on screen.
// `answer_mouse_activate` false leaves WM_MOUSEACTIVATE to GPUI, so WS_EX_NOACTIVATE alone has
// to keep clicks from activating the window.
pub fn install_subclass(h: isize, answer_mouse_activate: bool) -> bool {
    ANSWER_MOUSE_ACTIVATE.store(answer_mouse_activate, Ordering::Relaxed);
    let client = client_rect(h);
    let installed = unsafe { SetWindowSubclass(hwnd(h), Some(subclass_proc), 1, 0).as_bool() };
    unsafe {
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
    set_bounds(
        h,
        client.left as i32,
        client.top as i32,
        (client.right - client.left) as i32,
        (client.bottom - client.top) as i32,
    );
    installed
}

pub fn client_rect(h: isize) -> Rect {
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

pub fn ex_style(h: isize) -> u32 {
    unsafe { GetWindowLongPtrW(hwnd(h), GWL_EXSTYLE) as u32 }
}

pub fn describe_ex_style(style: u32) -> String {
    let has = |bit: u32| if style & bit != 0 { "yes" } else { "no" };
    format!(
        "WS_EX_TOOLWINDOW {}, WS_EX_TOPMOST {}, WS_EX_NOACTIVATE {}, WS_EX_APPWINDOW {}",
        has(WS_EX_TOOLWINDOW.0),
        has(WS_EX_TOPMOST.0),
        has(WS_EX_NOACTIVATE.0),
        has(WS_EX_APPWINDOW.0),
    )
}

pub fn set_noactivate(h: isize, on: bool) {
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

// The inline field needs keyboard focus, so while it's open the rack lets itself be activated.
pub fn allow_activation(h: isize, allowed: bool) {
    ALLOW_ACTIVATION.store(allowed, Ordering::Relaxed);
    set_noactivate(h, !allowed);
}

// Windows 11 rounds a window's corners and draws a border around it, and the rack draws its own
// 14px corners inside a transparent window, so the window test turns both off.
pub fn square_off(h: isize) -> (bool, bool) {
    // DWMWA_COLOR_NONE, from dwmapi.h.
    const COLOR_NONE: u32 = 0xFFFF_FFFE;
    unsafe {
        let pref: DWM_WINDOW_CORNER_PREFERENCE = DWMWCP_DONOTROUND;
        let corners = DwmSetWindowAttribute(
            hwnd(h),
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const _ as *const c_void,
            size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        )
        .is_ok();
        let border = DwmSetWindowAttribute(
            hwnd(h),
            DWMWA_BORDER_COLOR,
            &COLOR_NONE as *const _ as *const c_void,
            size_of::<u32>() as u32,
        )
        .is_ok();
        (corners, border)
    }
}

pub fn window_rect(h: isize) -> Rect {
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd(h), &mut r);
    }
    rect(r)
}

pub struct MoveRecord {
    pub at: Instant,
    pub took: Duration,
    pub altered: bool,
}

pub fn move_window(h: isize, x: i32, y: i32) -> MoveRecord {
    let at = Instant::now();
    unsafe {
        let _ = SetWindowPos(
            hwnd(h),
            None,
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        );
    }
    let took = at.elapsed();
    let r = window_rect(h);
    MoveRecord {
        at,
        took,
        altered: r.left as i32 != x || r.top as i32 != y,
    }
}

pub fn set_bounds(h: isize, x: i32, y: i32, w: i32, height: i32) {
    unsafe {
        let _ = SetWindowPos(
            hwnd(h),
            None,
            x,
            y,
            w,
            height,
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

// GetAsyncKeyState reads the physical left button, so with the buttons swapped for a left hand
// this reads the wrong one.
pub fn left_button_down() -> bool {
    unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0 }
}

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

pub struct Display {
    pub width: i32,
    pub height: i32,
    pub hz: u32,
}

pub fn display_at(p: (f64, f64)) -> Display {
    unsafe {
        let monitor = MonitorFromPoint(
            POINT {
                x: p.0 as i32,
                y: p.1 as i32,
            },
            MONITOR_DEFAULTTONEAREST,
        );
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        let _ = GetMonitorInfoW(monitor, &mut info as *mut _ as *mut MONITORINFO);
        let mut mode = DEVMODEW {
            dmSize: size_of::<DEVMODEW>() as u16,
            ..Default::default()
        };
        let hz = if EnumDisplaySettingsW(
            PCWSTR(info.szDevice.as_ptr()),
            ENUM_CURRENT_SETTINGS,
            &mut mode,
        )
        .as_bool()
        {
            mode.dmDisplayFrequency
        } else {
            0
        };
        let m = info.monitorInfo.rcMonitor;
        Display {
            width: m.right - m.left,
            height: m.bottom - m.top,
            hz,
        }
    }
}

pub fn monitor_count() -> i32 {
    unsafe { GetSystemMetrics(SM_CMONITORS) }
}

pub fn os_version() -> String {
    let read = || -> windows::core::Result<String> {
        let key = windows_registry::LOCAL_MACHINE
            .open(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")?;
        let build: u32 = key.get_string("CurrentBuild")?.parse().unwrap_or(0);
        let ubr = key.get_u32("UBR").unwrap_or(0);
        let display = key.get_string("DisplayVersion").unwrap_or_default();
        let name = if build >= 22000 {
            "Windows 11"
        } else {
            "Windows 10"
        };
        Ok(format!("{name} {display}, build {build}.{ubr}"))
    };
    read().unwrap_or_else(|e| format!("unknown ({e})"))
}

// Copies what the screen shows around the rack into a PNG, so the window test can look at its
// own corners and edges.
pub fn capture(r: Rect, margin: i32, path: &Path) -> Result<(), String> {
    let (x, y) = (r.left as i32 - margin, r.top as i32 - margin);
    let w = (r.right - r.left) as i32 + 2 * margin;
    let h = (r.bottom - r.top) as i32 + 2 * margin;
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bitmap.into());
        let _ = BitBlt(mem, 0, 0, w, h, Some(screen), x, y, SRCCOPY | CAPTUREBLT);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        GetDIBits(
            mem,
            bitmap,
            0,
            h as u32,
            Some(pixels.as_mut_ptr() as *mut c_void),
            &mut info,
            DIB_RGB_COLORS,
        );
        SelectObject(mem, old);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
    }
    for px in pixels.chunks_exact_mut(4) {
        px.swap(0, 2);
        px[3] = 255;
    }
    image::save_buffer(path, &pixels, w as u32, h as u32, image::ColorType::Rgba8)
        .map_err(|e| e.to_string())
}
