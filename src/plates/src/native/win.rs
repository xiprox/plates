//! The Win32 calls the rack makes through its window's native handle, for what GPUI's window
//! options leave out: setting the window's position, keeping clicks from activating it, and
//! reading the cursor, the monitors, and the foreground window.
//! record/areas/rack/log.md holds what the window test found for each.

use std::{
    ffi::c_void,
    sync::{
        OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use windows::Win32::{
    Foundation::{CloseHandle, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::{
        Dwm::{
            DWM_WINDOW_CORNER_PREFERENCE, DWMWA_BORDER_COLOR, DWMWA_CLOAKED,
            DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DWMWCP_ROUND,
            DwmExtendFrameIntoClientArea, DwmGetWindowAttribute, DwmSetWindowAttribute,
        },
        Gdi::{
            ClientToScreen, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTONULL,
            MONITORINFO, MonitorFromPoint,
        },
    },
    Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT},
    System::{
        Com::{COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx},
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
        Registry::{
            HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW,
            RegSetKeyValueW,
        },
        SystemInformation::{GetTickCount, GetTickCount64},
        Threading::{
            OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
            QueryFullProcessImageNameW, TerminateProcess,
        },
        Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION},
    },
    UI::{
        Accessibility::{HWINEVENTHOOK, SetWinEventHook},
        Controls::MARGINS,
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, GetLastInputInfo, LASTINPUTINFO, MOD_CONTROL, MOD_NOREPEAT,
            MOD_SHIFT, RegisterHotKey, VIRTUAL_KEY, VK_0, VK_9, VK_A, VK_BACK, VK_CONTROL,
            VK_DELETE, VK_DIVIDE, VK_LBUTTON, VK_LMENU, VK_LWIN, VK_NUMPAD0, VK_OEM_1, VK_OEM_3,
            VK_OEM_4, VK_OEM_8, VK_OEM_102, VK_RBUTTON, VK_RETURN, VK_RMENU, VK_RWIN, VK_SPACE,
            VK_TAB, VK_Z,
        },
        Input::{
            GetRawInputData, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RID_INPUT,
            RIDEV_INPUTSINK, RIM_TYPEKEYBOARD, RegisterRawInputDevices,
        },
        Shell::{DefSubclassProc, SetWindowSubclass, ShellExecuteW},
        WindowsAndMessaging::{
            ASFW_ANY, AllowSetForegroundWindow, CreateWindowExW, DispatchMessageW,
            EVENT_SYSTEM_FOREGROUND, GW_HWNDPREV, GWL_EXSTYLE, GetClassNameW, GetClientRect,
            GetCursorPos, GetForegroundWindow, GetMessageW, GetWindow, GetWindowLongPtrW,
            GetWindowRect, GetWindowThreadProcessId, HWND_MESSAGE, HWND_TOPMOST, IDOK,
            IsWindowVisible, MA_NOACTIVATE, MB_ICONINFORMATION, MB_ICONQUESTION, MB_OK,
            MB_OKCANCEL, MB_SETFOREGROUND, MB_TOPMOST, MESSAGEBOX_STYLE, MSG, MessageBoxW,
            RI_KEY_BREAK, SW_SHOWNORMAL, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
            SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow, SetWindowLongPtrW,
            SetWindowPos, WINDOW_EX_STYLE, WINDOW_STYLE, WINEVENT_OUTOFCONTEXT,
            WINEVENT_SKIPOWNPROCESS, WM_HOTKEY, WM_INPUT, WM_MOUSEACTIVATE, WM_NCCALCSIZE,
            WS_EX_NOACTIVATE, WS_EX_TOPMOST,
        },
    },
};
use windows::core::{PCWSTR, PWSTR, w};

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

/// How many of the rack's pixels, which are physical pixels here, make one of GPUI's pixels on the
/// monitor the window is on.
pub fn scale(window: &gpui_kit::Window) -> f64 {
    f64::from(window.scale_factor())
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
        // The new-tile shortcut, which `watch_hotkey` registers on this window.
        WM_HOTKEY if wparam.0 == HOTKEY_ID as usize => {
            if let Some(tx) = HOTKEY.get() {
                let _ = tx.try_send(());
            }
            return LRESULT(0);
        }
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

// Windows stacks the topmost windows by which went on top last, so a game in borderless full
// screen, itself a topmost window, covers the rack once Windows raises it (measured on Windows 11
// 25H2, 2026-11-15, record/areas/rack/log.md). The rack checks whether another program's topmost
// window covers any of it, and if one does, puts itself back on top without taking focus.
pub fn keep_on_top(h: isize) {
    if covered(h) {
        unsafe {
            let _ = SetWindowPos(
                hwnd(h),
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
            );
        }
    }
}

// Walks the windows above the rack in z-order, and finds one of another program's that's
// topmost, shown, and over the rack. A cloaked window, such as an app on another virtual
// desktop, counts as hidden, since Windows reports it visible but doesn't draw it.
fn covered(h: isize) -> bool {
    unsafe {
        let me = hwnd(h);
        let mut ours = 0u32;
        GetWindowThreadProcessId(me, Some(&mut ours));
        let mut rack = RECT::default();
        if GetWindowRect(me, &mut rack).is_err() {
            return false;
        }
        let mut next = GetWindow(me, GW_HWNDPREV);
        while let Ok(above) = next {
            let mut theirs = 0u32;
            GetWindowThreadProcessId(above, Some(&mut theirs));
            let topmost = GetWindowLongPtrW(above, GWL_EXSTYLE) & WS_EX_TOPMOST.0 as isize != 0;
            let mut r = RECT::default();
            let overlaps = GetWindowRect(above, &mut r).is_ok()
                && r.left < rack.right
                && rack.left < r.right
                && r.top < rack.bottom
                && rack.top < r.bottom;
            if theirs != ours
                && topmost
                && overlaps
                && IsWindowVisible(above).as_bool()
                && !cloaked(above)
            {
                return true;
            }
            next = GetWindow(above, GW_HWNDPREV);
        }
        false
    }
}

fn cloaked(h: HWND) -> bool {
    let mut value = 0u32;
    unsafe {
        DwmGetWindowAttribute(
            h,
            DWMWA_CLOAKED,
            &mut value as *mut u32 as *mut c_void,
            size_of::<u32>() as u32,
        )
        .is_ok()
            && value != 0
    }
}

static FOREGROUND: OnceLock<async_channel::Sender<()>> = OnceLock::new();

/// Sends on the returned channel each time another program's window comes to the foreground,
/// which is when a game raises itself over the rack. It must be called on the thread that runs
/// the window's message loop, since Windows delivers the event through that loop.
static HOTKEY: OnceLock<async_channel::Sender<()>> = OnceLock::new();
const HOTKEY_ID: i32 = 1;

/// Registers Ctrl + Shift + Space, the shortcut for a new tile, for the whole system, on the
/// rack's window, whose subclass hands each press to the receiver this returns. It returns `None`
/// when Windows refuses it, as when another program holds the shortcut.
/// (documented, https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerhotkey)
pub fn watch_hotkey(h: isize) -> Option<async_channel::Receiver<()>> {
    let (tx, rx) = async_channel::unbounded();
    HOTKEY.set(tx).ok()?;
    let modifiers = MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT;
    unsafe { RegisterHotKey(Some(hwnd(h)), HOTKEY_ID, modifiers, u32::from(VK_SPACE.0)) }.ok()?;
    Some(rx)
}

pub fn watch_foreground() -> Option<async_channel::Receiver<()>> {
    let (tx, rx) = async_channel::unbounded();
    FOREGROUND.set(tx).ok()?;
    let hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(on_foreground),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };
    (!hook.is_invalid()).then_some(rx)
}

unsafe extern "system" fn on_foreground(
    _: HWINEVENTHOOK,
    _: u32,
    _: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    if let Some(tx) = FOREGROUND.get() {
        let _ = tx.try_send(());
    }
}

/// How long since the last keyboard or mouse input to the PC, from any program.
/// (documented, https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getlastinputinfo)
pub fn idle_for() -> std::time::Duration {
    let mut info = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
        return std::time::Duration::ZERO;
    }
    // Both counts are milliseconds since Windows started, and wrap after 49.7 days.
    let now = unsafe { GetTickCount() };
    std::time::Duration::from_millis(u64::from(now.wrapping_sub(info.dwTime)))
}

/// The tick count, in milliseconds since Windows started, of the last typing key you pressed, in
/// any app, or 0 before the first.
static LAST_TYPED: AtomicU64 = AtomicU64::new(0);

/// Starts watching the keyboard of every app for typing keys, so a ping can wait while you type.
/// Raw input with `RIDEV_INPUTSINK` hands each key press to a window in the background, and a
/// message loop on GPUI's thread would hand GPUI tasks while it holds its own state, as
/// `ShellExecuteW`'s loop did, which ended Plates; so the window is a message-only window on a
/// thread of its own, with a loop of its own. Plates keeps the time of the last typing key, and
/// never which key. (record/areas/rack/03-holding-pings.md)
pub fn watch_typing() {
    let spawned = std::thread::Builder::new()
        .name("plates-typing".into())
        .spawn(|| unsafe {
            let window = match CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                PCWSTR::null(),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                None,
                None,
            ) {
                Ok(window) => window,
                Err(e) => {
                    eprintln!("Couldn't make a window to watch for typing: {e}");
                    return;
                }
            };
            // Usage page 1, usage 6: the keyboard.
            let keyboard = RAWINPUTDEVICE {
                usUsagePage: 1,
                usUsage: 6,
                dwFlags: RIDEV_INPUTSINK,
                hwndTarget: window,
            };
            if let Err(e) = RegisterRawInputDevices(&[keyboard], size_of::<RAWINPUTDEVICE>() as u32)
            {
                eprintln!("Couldn't watch the keyboard for typing: {e}");
                return;
            }
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if msg.message == WM_INPUT {
                    note_key(HRAWINPUT(msg.lParam.0 as *mut c_void));
                }
                DispatchMessageW(&msg);
            }
        });
    if let Err(e) = spawned {
        eprintln!("Couldn't start a thread to watch for typing: {e}");
    }
}

/// Notes the time of a typing key's press.
unsafe fn note_key(input: HRAWINPUT) {
    let mut raw: RAWINPUT = unsafe { std::mem::zeroed() };
    let mut size = size_of::<RAWINPUT>() as u32;
    let header = size_of::<RAWINPUTHEADER>() as u32;
    let into = (&mut raw as *mut RAWINPUT).cast::<c_void>();
    let read = unsafe { GetRawInputData(input, RID_INPUT, Some(into), &mut size, header) };
    if read == u32::MAX || raw.header.dwType != RIM_TYPEKEYBOARD.0 {
        return;
    }
    let key = unsafe { raw.data.keyboard };
    let pressed = u32::from(key.Flags) & RI_KEY_BREAK == 0;
    if pressed && types(key.VKey) && !in_shortcut() {
        LAST_TYPED.store(unsafe { GetTickCount64() }, Ordering::Relaxed);
    }
}

/// Whether a key pressed now is part of a shortcut, such as Ctrl+Tab or Alt+Tab, rather than
/// typing: Ctrl, the left Alt key, or a Windows key is held. Windows sends AltGr, which types
/// characters such as @ on the German layout, as Ctrl with the right Alt key, so Ctrl with the
/// right Alt key held still types. (unverified, record/areas/rack/03-holding-pings.md)
fn in_shortcut() -> bool {
    let held = |vk: VIRTUAL_KEY| unsafe { GetAsyncKeyState(i32::from(vk.0)) } < 0;
    held(VK_LWIN) || held(VK_RWIN) || held(VK_LMENU) || (held(VK_CONTROL) && !held(VK_RMENU))
}

/// Whether the Claude desktop app's window is in front, which holds pings while you read a reply
/// in it, as typing does (record/areas/rack/03-holding-pings.md).
pub fn claude_in_front() -> bool {
    let window = unsafe { GetForegroundWindow() };
    !window.is_invalid() && claude_app_window(window)
}

/// Whether a top-level window is one of the Claude desktop app's: a Chromium `Chrome_WidgetWin_1`
/// window of a process whose program is `claude.exe`. Claude Code's own program is `claude.exe`
/// too, and a console window may count as its process's (unverified), so the class keeps a
/// console window from counting. (record/areas/hooks/01-claude-code-hooks.md, "Answering a
/// wrap-up")
pub fn claude_app_window(window: HWND) -> bool {
    let mut class = [0u16; 64];
    let n = unsafe { GetClassNameW(window, &mut class) };
    if String::from_utf16_lossy(&class[..n.max(0) as usize]) != "Chrome_WidgetWin_1" {
        return false;
    }
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
    let Ok(process) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) })
    else {
        return false;
    };
    let path = program_of(process);
    unsafe {
        let _ = CloseHandle(process);
    }
    path.is_some_and(|p| p.to_ascii_lowercase().ends_with("\\claude.exe"))
}

// The full path of the program a process runs, from a handle opened with at least
// PROCESS_QUERY_LIMITED_INFORMATION.
fn program_of(process: HANDLE) -> Option<String> {
    let mut path = [0u16; 1024];
    let mut len = path.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut len,
        )
    }
    .ok()?;
    Some(String::from_utf16_lossy(&path[..len as usize]))
}

/// Stops each process that runs `program`, the task server beside Plates, before an update
/// replaces the folder's files, since Windows won't replace a running program's file and each
/// Claude Code session keeps a task server running. Claude Code then starts a session's server
/// again at the session's next call to a task tool, which it did on macOS, and on Windows is
/// unverified (record/areas/build/01-updates.md, "Installing").
pub fn stop_task_servers(program: &std::path::Path) {
    let ours = program.to_string_lossy().to_ascii_lowercase();
    let Some(file_name) = program
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
    else {
        return;
    };
    let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
        return;
    };
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok();
    while more {
        let end = entry
            .szExeFile
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(entry.szExeFile.len());
        let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
        let access = PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE;
        if name.eq_ignore_ascii_case(&file_name)
            && let Ok(process) = unsafe { OpenProcess(access, false, entry.th32ProcessID) }
        {
            if program_of(process).is_some_and(|p| p.to_ascii_lowercase() == ours) {
                unsafe {
                    let _ = TerminateProcess(process, 1);
                }
            }
            unsafe {
                let _ = CloseHandle(process);
            }
        }
        more = unsafe { Process32NextW(snapshot, &mut entry) }.is_ok();
    }
    unsafe {
        let _ = CloseHandle(snapshot);
    }
}

/// How long since the last typing key you pressed, in any app, or `None` before the first.
pub fn typed_ago() -> Option<Duration> {
    let at = LAST_TYPED.load(Ordering::Relaxed);
    (at != 0).then(|| Duration::from_millis(unsafe { GetTickCount64() }.saturating_sub(at)))
}

/// Whether a key, by its virtual-key code, types: a digit, a letter, a number-pad key,
/// punctuation, Space, Backspace, Enter, Tab, or Delete. The arrow and page keys scroll, and
/// Shift, Ctrl, Alt, and the Windows key alone do neither, so none of them counts. When you change
/// which keys count, change the list in record/areas/rack/03-holding-pings.md too.
fn types(vk: u16) -> bool {
    let within = |from: VIRTUAL_KEY, to: VIRTUAL_KEY| (from.0..=to.0).contains(&vk);
    let one_of = [VK_OEM_102, VK_SPACE, VK_BACK, VK_RETURN, VK_TAB, VK_DELETE];
    within(VK_0, VK_9)
        || within(VK_A, VK_Z)
        || within(VK_NUMPAD0, VK_DIVIDE)
        || within(VK_OEM_1, VK_OEM_3)
        || within(VK_OEM_4, VK_OEM_8)
        || one_of.iter().any(|key| key.0 == vk)
}

/// Whether Plates starts when you sign in to Windows: a value named "Plates" under the current
/// user's `Run` key that names this program.
/// (documented, https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys)
pub fn starts_at_login() -> bool {
    let Some(want) = run_command() else {
        return false;
    };
    let mut buf = [0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            w!("Plates"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut c_void),
            Some(&mut size),
        )
    };
    let len = (size as usize / 2).saturating_sub(1);
    read.is_ok() && String::from_utf16_lossy(&buf[..len.min(buf.len())]) == want
}

/// Adds or removes the `Run` value that starts Plates when you sign in, and returns whether
/// Windows took the change.
pub fn set_start_at_login(on: bool) -> bool {
    let result = if on {
        let Some(command) = run_command() else {
            return false;
        };
        let wide: Vec<u16> = command.encode_utf16().chain(Some(0)).collect();
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                w!("Plates"),
                REG_SZ.0,
                Some(wide.as_ptr() as *const c_void),
                (wide.len() * 2) as u32,
            )
        }
    } else {
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, w!("Plates")) }
    };
    result.is_ok()
}

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");

// The command the `Run` value holds: this program's path, in quotes, since it can hold spaces.
fn run_command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("\"{}\"", exe.display()))
}

/// Whether you read the time on a 24-hour clock, as Windows shows it to you: your time format,
/// such as "HH:mm:ss" or "h:mm:ss tt", has the 24-hour "H".
/// (documented, https://learn.microsoft.com/en-us/windows/win32/intl/locale-stimeformat)
pub fn uses_24_hour_clock() -> bool {
    use windows::Win32::Globalization::{GetLocaleInfoEx, LOCALE_STIMEFORMAT};
    let mut format = [0u16; 80];
    // A null locale name is the user's own locale.
    let len = unsafe { GetLocaleInfoEx(PCWSTR::null(), LOCALE_STIMEFORMAT, Some(&mut format)) };
    if len <= 0 {
        return true;
    }
    String::from_utf16_lossy(&format[..len as usize]).contains('H')
}

/// How far the local time is ahead of UTC, in seconds, by Windows' time zone setting, with
/// daylight saving time when it's in effect.
/// (documented, https://learn.microsoft.com/en-us/windows/win32/api/timezoneapi/nf-timezoneapi-gettimezoneinformation)
pub fn utc_offset() -> i64 {
    // The values GetTimeZoneInformation returns, which the windows crate defines only under its
    // Win32_System_SystemServices feature, which Plates doesn't enable.
    const TIME_ZONE_ID_STANDARD: u32 = 1;
    const TIME_ZONE_ID_DAYLIGHT: u32 = 2;
    let mut zone = TIME_ZONE_INFORMATION::default();
    let id = unsafe { GetTimeZoneInformation(&mut zone) };
    // UTC is the local time plus Bias, plus StandardBias or DaylightBias, all in minutes.
    let extra = match id {
        TIME_ZONE_ID_STANDARD => zone.StandardBias,
        TIME_ZONE_ID_DAYLIGHT => zone.DaylightBias,
        _ => 0,
    };
    -i64::from(zone.Bias + extra) * 60
}

/// Opens a link with the program Windows has for its scheme, such as the Claude desktop app for
/// `claude://`. Windows lets a process bring its window to the front only with the foreground's
/// leave, and the rack never takes the foreground, so the rack first gives that leave to any
/// process, which Windows lets the process that got the last input, the click on the rack, do.
/// (documented, https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-allowsetforegroundwindow)
pub fn open_url(url: &str) {
    unsafe {
        let _ = AllowSetForegroundWindow(ASFW_ANY);
    }
    // ShellExecuteW runs a message loop of its own, which on the rack's thread hands GPUI a task
    // while the click that called it still holds GPUI's state, and the panic that follows ends
    // Plates (measured on Windows 11 25H2, 2026-11-15, record/areas/rack/log.md). On a thread
    // with no windows, its loop has nothing of GPUI's to run. Microsoft asks for COM set up
    // first, since the shell may hand the link to a COM extension.
    // (documented, https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shellexecutew)
    let wide: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    let spawned = std::thread::Builder::new()
        .name("plates-open".into())
        .spawn(move || unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
            ShellExecuteW(
                None,
                w!("open"),
                PCWSTR(wide.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            );
        });
    if let Err(e) = spawned {
        eprintln!("Couldn't start a thread to open a link: {e}");
    }
}

/// Asks `question` in a dialog, with `explanation` under it, and says whether you chose OK, which
/// stands for `action`, rather than Cancel.
pub async fn confirm(question: &str, explanation: &str, _action: &str) -> bool {
    message_box(question, explanation, MB_OKCANCEL | MB_ICONQUESTION).await == IDOK.0
}

/// Tells you `message` in a dialog, with `explanation` under it.
pub async fn alert(message: &str, explanation: &str) {
    message_box(message, explanation, MB_OK | MB_ICONINFORMATION).await;
}

// A message box runs a message loop of its own, which on the rack's thread could hand GPUI a task
// while GPUI's state is held, as `ShellExecuteW`'s did in `open_url`, so the box runs on a thread
// with no windows. Its buttons' labels are Windows' own, such as OK and Cancel.
async fn message_box(message: &str, explanation: &str, style: MESSAGEBOX_STYLE) -> i32 {
    let text: Vec<u16> = format!("{message}\n\n{explanation}")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let (tx, rx) = async_channel::bounded(1);
    let spawned = std::thread::Builder::new()
        .name("plates-dialog".into())
        .spawn(move || {
            let answer = unsafe {
                MessageBoxW(
                    None,
                    PCWSTR(text.as_ptr()),
                    w!("Plates"),
                    style | MB_SETFOREGROUND | MB_TOPMOST,
                )
            };
            let _ = tx.send_blocking(answer.0);
        });
    if let Err(e) = spawned {
        eprintln!("Couldn't start a thread for a dialog: {e}");
        return 0;
    }
    rx.recv().await.unwrap_or(0)
}

/// Plays a WAV file held in memory, returns while it plays, and says whether it could. With
/// `SND_MEMORY`, `PlaySoundW` reads the sound from the pointer it's given rather than a file name,
/// so the file must live until it has played, which a `'static` one does.
/// (documented, https://learn.microsoft.com/previous-versions/dd743680(v=vs.85))
pub fn play_sound(wav: &'static [u8]) -> bool {
    let flags = SND_MEMORY | SND_ASYNC | SND_NODEFAULT;
    let played = unsafe { PlaySoundW(PCWSTR(wav.as_ptr().cast()), None, flags) }.as_bool();
    if !played {
        eprintln!("Couldn't play the tone.");
    }
    played
}

#[cfg(test)]
mod tests {
    use super::types;

    #[test]
    fn only_keys_that_type_count_as_typing() {
        // A letter, a digit, a comma, Space, Backspace, and Enter type.
        for vk in [0x41, 0x35, 0xBC, 0x20, 0x08, 0x0D] {
            assert!(types(vk), "{vk:#x} should type");
        }
        // The down arrow, Page Down, Shift, Ctrl, and the Windows key don't.
        for vk in [0x28, 0x22, 0x10, 0x11, 0x5B] {
            assert!(!types(vk), "{vk:#x} shouldn't type");
        }
    }
}
