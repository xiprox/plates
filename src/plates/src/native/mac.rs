//! The native calls on macOS, through AppKit, with the same signatures as win.rs, so the rack's
//! code is the same on both. CI's macOS job is the only check it gets until the plan's step 7
//! tries it on a Mac (record/decisions/003-releases-with-release-please-and-velopack.md).
//!
//! The rack works in pixels from the top-left corner of the primary screen, as Windows does.
//! AppKit works in points from the bottom-left corner of the primary screen, so every position
//! here is flipped against the primary screen's height and multiplied by its scale. A second
//! screen with another scale isn't handled.

// objc2 marks some of these calls unsafe and not others, and which ones changes between its
// versions, so every call sits in an unsafe block.
#![allow(unused_unsafe)]

use std::{ffi::c_char, path::PathBuf, time::Duration};

use objc2::{MainThreadMarker, rc::Retained};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSEvent, NSFloatingWindowLevel, NSScreen, NSView,
    NSWindow, NSWindowCollectionBehavior,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use crate::motion::{Edges, Rect};

// AppKit's windows and screens belong to the main thread, where GPUI runs the rack.
fn mtm() -> MainThreadMarker {
    MainThreadMarker::new().expect("AppKit is called on the main thread")
}

fn window(h: isize) -> Option<&'static NSWindow> {
    (h != 0).then(|| unsafe { &*(h as *const NSWindow) })
}

// The primary screen's height in points, and its scale.
fn primary() -> (f64, f64) {
    let screens = unsafe { NSScreen::screens(mtm()) };
    match screens.iter().next() {
        Some(s) => unsafe { (s.frame().size.height, s.backingScaleFactor()) },
        None => (0., 1.),
    }
}

fn to_rect(r: NSRect) -> Rect {
    let (height, scale) = primary();
    Rect {
        left: r.origin.x * scale,
        top: (height - r.origin.y - r.size.height) * scale,
        right: (r.origin.x + r.size.width) * scale,
        bottom: (height - r.origin.y) * scale,
    }
}

fn to_ns(r: Rect) -> NSRect {
    let (height, scale) = primary();
    NSRect::new(
        NSPoint::new(r.left / scale, height - r.bottom / scale),
        NSSize::new((r.right - r.left) / scale, (r.bottom - r.top) / scale),
    )
}

pub fn hwnd_of(window: &gpui_kit::Window) -> isize {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = HasWindowHandle::window_handle(window)
        .expect("the rack's window should have a native handle");
    match handle.as_raw() {
        RawWindowHandle::AppKit(h) => {
            let view: &NSView = unsafe { h.ns_view.cast::<NSView>().as_ref() };
            unsafe { view.window() }.map_or(0, |w| Retained::as_ptr(&w) as isize)
        }
        other => panic!("the rack's window should have an AppKit handle, not {other:?}"),
    }
}

/// Makes GPUI's window the rack's: above other apps' windows, on every Space, and with a shadow.
/// Plates also leaves the Dock, as a floating overlay.
pub fn take_over(h: isize) {
    let Some(w) = window(h) else {
        return;
    };
    unsafe {
        w.setLevel(NSFloatingWindowLevel);
        w.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        w.setHasShadow(true);
        let _ = NSApplication::sharedApplication(mtm())
            .setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    }
}

/// The inline field needs keyboard focus, so while it's open, Plates comes to the front.
pub fn allow_activation(h: isize, allowed: bool) {
    if allowed {
        set_foreground(h);
    }
}

pub fn set_shadow(h: isize, on: bool) {
    if let Some(w) = window(h) {
        unsafe { w.setHasShadow(on) };
    }
}

pub fn window_rect(h: isize) -> Rect {
    window(h).map_or_else(Rect::default, |w| to_rect(unsafe { w.frame() }))
}

pub fn set_bounds(h: isize, r: Rect) {
    if let Some(w) = window(h) {
        unsafe { w.setFrame_display(to_ns(r), true) };
    }
}

pub fn cursor_pos() -> (f64, f64) {
    let (height, scale) = primary();
    let p = unsafe { NSEvent::mouseLocation() };
    (p.x * scale, (height - p.y) * scale)
}

pub fn left_button_down() -> bool {
    let buttons = unsafe { NSEvent::pressedMouseButtons() };
    buttons & 1 != 0
}

pub fn right_button_down() -> bool {
    let buttons = unsafe { NSEvent::pressedMouseButtons() };
    buttons & 2 != 0
}

/// The visible area of the screen at `p`, without the menu bar and the Dock, and which of its
/// edges have another screen beyond them.
pub fn edges_at(p: (f64, f64)) -> Edges {
    let screens = unsafe { NSScreen::screens(mtm()) };
    let all: Vec<(Rect, Rect)> = screens
        .iter()
        .map(|s| unsafe { (to_rect(s.frame()), to_rect(s.visibleFrame())) })
        .collect();
    let distance = |r: &Rect| {
        let dx = (r.left - p.0).max(0.).max(p.0 - r.right);
        let dy = (r.top - p.1).max(0.).max(p.1 - r.bottom);
        dx.hypot(dy)
    };
    let Some(i) =
        (0..all.len()).min_by(|&a, &b| distance(&all[a].0).total_cmp(&distance(&all[b].0)))
    else {
        let none = Rect::new((0., 0.), (1920., 1080.));
        return Edges {
            area: none,
            open_left: false,
            open_right: false,
            open_top: false,
            open_bottom: false,
        };
    };
    let (frame, area) = all[i];
    let cx = p.0.clamp(frame.left, frame.right - 1.);
    let cy = p.1.clamp(frame.top, frame.bottom - 1.);
    let other = |x: f64, y: f64| {
        all.iter()
            .enumerate()
            .any(|(j, (f, _))| j != i && f.contains((x, y)))
    };
    Edges {
        area,
        open_left: other(frame.left - 1., cy),
        open_right: other(frame.right, cy),
        open_top: other(cx, frame.top - 1.),
        open_bottom: other(cx, frame.bottom),
    }
}

/// The key window while Plates is the active app, and 0 while another app is, which
/// `set_foreground` takes as handing focus back to it.
pub fn foreground_window() -> isize {
    let app = unsafe { NSApplication::sharedApplication(mtm()) };
    if !unsafe { app.isActive() } {
        return 0;
    }
    unsafe { app.keyWindow() }.map_or(0, |w| Retained::as_ptr(&w) as isize)
}

pub fn set_foreground(h: isize) -> bool {
    let app = unsafe { NSApplication::sharedApplication(mtm()) };
    match window(h) {
        Some(w) => unsafe {
            #[allow(deprecated)]
            app.activateIgnoringOtherApps(true);
            w.makeKeyAndOrderFront(None);
        },
        None => unsafe { app.deactivate() },
    }
    true
}

/// The rack's window sits at the floating level, above other apps' windows, so there's nothing to
/// put back.
pub fn keep_on_top(_h: isize) {}

/// macOS has no foreground change to watch for, since the floating level keeps the rack above
/// other apps' windows.
pub fn watch_foreground() -> Option<async_channel::Receiver<()>> {
    None
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
}

/// How long since the last keyboard or mouse input, from any app.
/// (documented, https://developer.apple.com/documentation/coregraphics/cgeventsource/secondssincelasteventtype(_:eventtype:))
pub fn idle_for() -> Duration {
    // kCGEventSourceStateCombinedSessionState, and kCGAnyInputEventType.
    let secs = unsafe { CGEventSourceSecondsSinceLastEventType(0, u32::MAX) };
    Duration::from_secs_f64(secs.max(0.))
}

// Start at login is a launch agent in the user's LaunchAgents folder, which macOS loads when you
// sign in. (documented, https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html)
const AGENT: &str = "plates.start-at-login";

fn agent_file() -> Option<PathBuf> {
    Some(
        dirs::home_dir()?
            .join("Library/LaunchAgents")
            .join(format!("{AGENT}.plist")),
    )
}

fn agent_plist() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let exe = exe
        .display()
        .to_string()
        .replace('&', "&amp;")
        .replace('<', "&lt;");
    Some(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{AGENT}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#
    ))
}

pub fn starts_at_login() -> bool {
    match (agent_file(), agent_plist()) {
        (Some(file), Some(want)) => std::fs::read_to_string(file).is_ok_and(|have| have == want),
        _ => false,
    }
}

pub fn set_start_at_login(on: bool) -> bool {
    let Some(file) = agent_file() else {
        return false;
    };
    if !on {
        return std::fs::remove_file(&file).is_ok() || !file.exists();
    }
    let Some(plist) = agent_plist() else {
        return false;
    };
    file.parent()
        .is_some_and(|dir| std::fs::create_dir_all(dir).is_ok())
        && std::fs::write(&file, plist).is_ok()
}

// The C library's `struct tm` on macOS, for `tm_gmtoff`.
#[repr(C)]
struct Tm {
    tm_sec: i32,
    tm_min: i32,
    tm_hour: i32,
    tm_mday: i32,
    tm_mon: i32,
    tm_year: i32,
    tm_wday: i32,
    tm_yday: i32,
    tm_isdst: i32,
    tm_gmtoff: i64,
    tm_zone: *mut c_char,
}

unsafe extern "C" {
    fn time(t: *mut i64) -> i64;
    fn localtime_r(t: *const i64, out: *mut Tm) -> *mut Tm;
}

/// How far the local time is ahead of UTC, in seconds, with daylight saving time when it's in
/// effect.
pub fn utc_offset() -> i64 {
    let mut tm = Tm {
        tm_sec: 0,
        tm_min: 0,
        tm_hour: 0,
        tm_mday: 0,
        tm_mon: 0,
        tm_year: 0,
        tm_wday: 0,
        tm_yday: 0,
        tm_isdst: 0,
        tm_gmtoff: 0,
        tm_zone: std::ptr::null_mut(),
    };
    unsafe {
        let now = time(std::ptr::null_mut());
        if localtime_r(&now, &mut tm).is_null() {
            return 0;
        }
    }
    tm.tm_gmtoff
}

/// Opens a link with the app macOS has for its scheme, through `open`.
pub fn open_url(url: &str) {
    if let Err(e) = std::process::Command::new("/usr/bin/open").arg(url).spawn() {
        eprintln!("Couldn't open {url}: {e}");
    }
}
