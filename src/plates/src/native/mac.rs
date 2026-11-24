//! The native calls on macOS, through AppKit, with the same signatures as win.rs, so the rack's
//! code is the same on both. CI's macOS job is the only check it gets until the plan's step 7
//! tries it on a Mac (record/decisions/003-releases-with-release-please-and-velopack.md).
//!
//! The rack works in pixels from the top-left corner of the primary screen, as Windows does.
//! AppKit works in points from the bottom-left corner of the primary screen, so every position
//! here is flipped against the primary screen's height and multiplied by its scale, on every
//! screen. `scale` hands the rack that same factor, so the rack keeps its size in points on a
//! screen with another scale.

// objc2 marks some of these calls unsafe and not others, and which ones changes between its
// versions, so every call sits in an unsafe block.
#![allow(unused_unsafe)]

use std::{
    ffi::{c_char, c_ulong, c_void},
    path::PathBuf,
    sync::OnceLock,
    time::Duration,
};

use objc2::{
    AllocAnyThread, MainThreadMarker, msg_send,
    rc::Retained,
    runtime::{AnyObject, NSObjectProtocol},
    sel,
};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSApplication, NSApplicationActivationOptions,
    NSApplicationActivationPolicy, NSEvent, NSFloatingWindowLevel, NSRunningApplication, NSScreen,
    NSSound, NSView, NSWindow, NSWindowCollectionBehavior, NSWorkspace,
    NSWorkspaceOpenConfiguration,
};
use objc2_foundation::{NSBundle, NSData, NSPoint, NSRect, NSSize, NSString, NSURL};

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

/// How many of the rack's pixels make one of GPUI's pixels.
/// A rack sized by the scale of the screen it's on comes out at half size on a 1x screen beside a
/// 2x primary, with GPUI still laying it out at full size, because every conversion here
/// multiplies by the primary screen's scale. So the rack's scale is the primary screen's on every
/// screen. (measured on macOS 26.5, 2026-11-16, record/areas/rack/log.md)
pub fn scale(_window: &gpui_kit::Window) -> f64 {
    primary().1
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
        // The rack resizes its window on every frame while a tile grows or a task list opens,
        // and some frames reach the screen drawn for the size before. macOS stretches such a
        // frame to the window's new size, so every tile wobbles by a few pixels for that frame
        // (measured on macOS 26.5.2, 2026-11-20, record/areas/rack/log.md). Pinning the drawing to
        // the window's top-left corner shows it unscaled instead, at most a few pixels short at
        // the rack's bottom edge.
        if let Some(content) = w.contentView() {
            pin_top_left(&content);
            for view in content.subviews().iter() {
                pin_top_left(&view);
            }
        }
    }
}

// Sets a view's layer to keep its drawing at its top-left corner when the layer's size changes,
// rather than stretch it to the new size, which is Core Animation's default. GPUI's view isn't
// flipped, so the layer's top is the visual top.
fn pin_top_left(view: &NSView) {
    unsafe {
        let layer: *mut AnyObject = msg_send![view, layer];
        if !layer.is_null() {
            let gravity = NSString::from_str("topLeft");
            let _: () = msg_send![layer, setContentsGravity: &*gravity];
        }
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
        unsafe { w.setFrame_display(snap(to_ns(r)), true) };
    }
}

// Without this, the rack's frames come late on macOS while it sits at fractions of a point, as it
// does through a throw, and through a drag when the pointer moves by fractions (unverified). A
// window rect there asks a width or height a hair off its real one, since to_ns computes each
// from two edges, and AppKit, which puts a window on whole points, can then make the window a
// point wider or taller. GPUI takes that as a resize: it draws the window at once, outside its
// display link, and restarts the link, which leaves gaps of two refreshes or more between frames.
// Rounding the rect to whole points keeps the window's size the same on every move. The rack
// grows down from its top, where take_over pins its drawing, so the top edge is the one rounded
// and the bottom follows from it: rounding AppKit's bottom edge and the height on their own would
// move the top by a point from frame to frame while the rack grows from a top at a fraction of a
// point. (measured on macOS 26.5.2, 2026-11-20, record/areas/rack/log.md, "On macOS, a window
// placed at fractions of a point can change size")
fn snap(r: NSRect) -> NSRect {
    let size = NSSize::new(r.size.width.round(), r.size.height.round());
    let top = (r.origin.y + r.size.height).round();
    NSRect::new(NSPoint::new(r.origin.x.round(), top - size.height), size)
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

static HOTKEY: OnceLock<async_channel::Sender<()>> = OnceLock::new();

#[repr(C)]
struct EventTypeSpec {
    class: u32,
    kind: u32,
}

#[repr(C)]
struct EventHotKeyId {
    signature: u32,
    id: u32,
}

// Carbon's hot keys are the system-wide shortcuts macOS gives an app without asking for
// Accessibility or Input Monitoring, which watching every key press with NSEvent would need.
// (documented, https://developer.apple.com/documentation/carbon/1514524-registereventhotkey)
// The declarations follow the global-hotkey crate's, 0.10.0, src/platform_impl/macos/ffi.rs.
#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn GetApplicationEventTarget() -> *mut c_void;
    fn InstallEventHandler(
        target: *mut c_void,
        handler: Option<unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> i32>,
        count: c_ulong,
        types: *const EventTypeSpec,
        user_data: *mut c_void,
        out: *mut *mut c_void,
    ) -> i32;
    fn RegisterEventHotKey(
        key_code: u32,
        modifiers: u32,
        id: EventHotKeyId,
        target: *mut c_void,
        options: u32,
        out: *mut *mut c_void,
    ) -> i32;
}

/// Registers ⌃⇧Space, the shortcut for a new tile, for the whole system, and hands each press to
/// the receiver it returns. It returns `None` when macOS refuses it, as when another app holds the
/// shortcut.
pub fn watch_hotkey(_h: isize) -> Option<async_channel::Receiver<()>> {
    // 'keyb' and 5 are kEventClassKeyboard and kEventHotKeyPressed, 0x31 is Space's key code,
    // and 0x1000 and 0x200 are controlKey and shiftKey.
    const KEYBOARD: u32 = u32::from_be_bytes(*b"keyb");
    const HOT_KEY_PRESSED: u32 = 5;
    const SPACE: u32 = 0x31;
    const CONTROL_SHIFT: u32 = 0x1000 | 0x200;
    let (tx, rx) = async_channel::unbounded();
    HOTKEY.set(tx).ok()?;
    let pressed = EventTypeSpec {
        class: KEYBOARD,
        kind: HOT_KEY_PRESSED,
    };
    let id = EventHotKeyId {
        signature: u32::from_be_bytes(*b"plts"),
        id: 1,
    };
    let (mut handler, mut hotkey) = (std::ptr::null_mut(), std::ptr::null_mut());
    unsafe {
        let target = GetApplicationEventTarget();
        if InstallEventHandler(
            target,
            Some(on_hotkey),
            1,
            &pressed,
            std::ptr::null_mut(),
            &mut handler,
        ) != 0
        {
            return None;
        }
        if RegisterEventHotKey(SPACE, CONTROL_SHIFT, id, target, 0, &mut hotkey) != 0 {
            return None;
        }
    }
    Some(rx)
}

unsafe extern "C" fn on_hotkey(_: *mut c_void, _: *mut c_void, _: *mut c_void) -> i32 {
    if let Some(tx) = HOTKEY.get() {
        let _ = tx.try_send(());
    }
    0
}

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

/// On macOS, Plates doesn't watch for typing, since whether reading key presses needs the Input
/// Monitoring permission is unmeasured, so no ping waits for your typing there
/// (record/areas/rack/03-holding-pings.md, "Left open").
pub fn watch_typing() {}

/// How long since the last typing key you pressed, which Plates doesn't watch for on macOS.
pub fn typed_ago() -> Option<Duration> {
    None
}

/// Whether the Claude desktop app is in front, which Plates doesn't ask on macOS, where no ping
/// waits for you (record/areas/rack/03-holding-pings.md, "Left open").
pub fn claude_in_front() -> bool {
    false
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

/// Whether you read the time on a 24-hour clock, as macOS shows it to you: the pattern your
/// locale, with its 24-hour setting, gives for an hour alone has no AM or PM, as "HH" has none
/// and "h a" does. (measured on macOS 26.5.2, 2026-11-16: "h a" for en_US)
pub fn uses_24_hour_clock() -> bool {
    use objc2_foundation::{NSDateFormatter, NSLocale, NSString};
    let locale = unsafe { NSLocale::currentLocale() };
    let pattern = unsafe {
        NSDateFormatter::dateFormatFromTemplate_options_locale(
            &NSString::from_str("j"),
            0,
            Some(&locale),
        )
    };
    pattern.is_none_or(|p| !p.to_string().contains('a'))
}

/// Whether Plates is the active app.
pub fn is_active() -> bool {
    NSApplication::sharedApplication(mtm()).isActive()
}

/// Asks macOS to make Plates the active app, which `open_url` needs before it can hand the
/// keyboard to the app it opens.
pub fn activate_self() {
    #[allow(deprecated)]
    NSApplication::sharedApplication(mtm()).activateIgnoringOtherApps(true);
}

/// Asks `question` in a dialog, with `explanation` under it, and says whether you chose `action`
/// rather than Cancel. AppKit's dialogs belong to the main thread, where this runs the dialog
/// before it returns.
pub async fn confirm(question: &str, explanation: &str, action: &str) -> bool {
    show_alert(question, explanation, &[action, "Cancel"]) == NSAlertFirstButtonReturn
}

/// Tells you `message` in a dialog, with `explanation` under it.
pub async fn alert(message: &str, explanation: &str) {
    show_alert(message, explanation, &["OK"]);
}

// Plates is an app with no Dock icon that's rarely the active app, so it becomes the active app
// first, or the dialog could open behind the app you're in (unverified).
fn show_alert(message: &str, explanation: &str, buttons: &[&str]) -> isize {
    let mtm = mtm();
    activate_self();
    unsafe {
        let alert = NSAlert::new(mtm);
        alert.setMessageText(&NSString::from_str(message));
        alert.setInformativeText(&NSString::from_str(explanation));
        for button in buttons {
            alert.addButtonWithTitle(&NSString::from_str(button));
        }
        alert.runModal()
    }
}

/// Opens a link with the app macOS has for its scheme, and hands that app the keyboard.
///
/// The rack is a panel that takes the keyboard on a click without making Plates the active app,
/// and a link opened that way showed the session in the Claude app while the rack kept the
/// keyboard. macOS activates an app at another's request once the active app yields to it
/// (documented, `activateFromApplication:options:` in AppKit's NSRunningApplication.h), so the
/// caller makes Plates active with `activate_self`, and this yields to the link's app and asks
/// for it to be activated from Plates, which handed the Claude app the keyboard within 100ms
/// (measured on macOS 26.5.2, 2026-11-16, record/areas/rack/log.md).
pub fn open_url(url: &str) {
    let Some(link) = NSURL::URLWithString(&NSString::from_str(url)) else {
        eprintln!("Couldn't open {url}: it isn't a URL");
        return;
    };
    let workspace = NSWorkspace::sharedWorkspace();
    let target = workspace
        .URLForApplicationToOpenURL(&link)
        .and_then(|app| NSBundle::bundleWithURL(&app))
        .and_then(|bundle| bundle.bundleIdentifier())
        .and_then(|id| {
            NSRunningApplication::runningApplicationsWithBundleIdentifier(&id).firstObject()
        });
    let me = NSApplication::sharedApplication(mtm());
    // Yielding and activating from another app came with macOS 14, and before it an app could
    // activate another without either.
    let cooperative = me.respondsToSelector(sel!(yieldActivationToApplication:));
    if let (Some(target), true) = (&target, cooperative) {
        me.yieldActivationToApplication(target);
    }
    let config = NSWorkspaceOpenConfiguration::configuration();
    config.setActivates(true);
    workspace.openURL_configuration_completionHandler(&link, &config, None);
    if let Some(target) = &target {
        if cooperative {
            target.activateFromApplication_options(
                &NSRunningApplication::currentApplication(),
                NSApplicationActivationOptions::empty(),
            );
        } else {
            target.activateWithOptions(NSApplicationActivationOptions::empty());
        }
    }
}

thread_local! {
    /// The sound playing, kept until the next one replaces it, so it isn't freed mid-play.
    static PLAYING: std::cell::RefCell<Option<Retained<NSSound>>> = const { std::cell::RefCell::new(None) };
}

/// Plays a WAV file held in memory, returns while it plays, and says whether it could.
pub fn play_sound(wav: &'static [u8]) -> bool {
    let data = NSData::with_bytes(wav);
    let Some(sound) = NSSound::initWithData(NSSound::alloc(), &data) else {
        eprintln!("Couldn't read the tone.");
        return false;
    };
    let played = sound.play();
    if !played {
        eprintln!("Couldn't play the tone.");
    }
    PLAYING.with(|p| *p.borrow_mut() = Some(sound));
    played
}

#[cfg(test)]
mod tests {
    use super::{NSPoint, NSRect, NSSize, snap};

    #[test]
    fn a_rack_growing_from_a_fractional_top_keeps_its_top_on_one_point() {
        // AppKit's y runs up from the screen's bottom, so the top edge is origin.y + height.
        let tops: Vec<f64> = (0..25)
            .map(|i| {
                let height = 400. + i as f64 * 2.4;
                let r = snap(NSRect::new(
                    NSPoint::new(10.3, 681.55 - height),
                    NSSize::new(304., height),
                ));
                r.origin.y + r.size.height
            })
            .collect();
        assert!(tops.iter().all(|&top| top == 682.), "{tops:?}");
    }

    #[test]
    fn a_size_a_hair_off_comes_out_whole() {
        let r = snap(NSRect::new(
            NSPoint::new(300.36, 200.18),
            NSSize::new(303.99999999999994, 400.00000000000006),
        ));
        assert_eq!((r.size.width, r.size.height), (304., 400.));
    }
}
