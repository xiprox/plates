# Rack: log

What we found about the rack's window and tiles, newest entry first.

## 2026-11-15: On Windows, GPUI's PopUp window moves on every frame at 240Hz, once a subclass removes its frame and keeps clicks from activating it

**Asked:** can GPUI give the rack the window [01-window](01-window.md) designs, on Windows?
This entry covers what the window test, `experiments/areas/rack/window-test/`, measures with its `--selftest` flag, which moves the rack without a person's mouse or keyboard.
The checks that need a person's hands come later.

**Expected:** GPUI's window options to cover part of the window, and calls through its native handle to cover the rest.

**Found:** we read gpui-pre 0.3.12's source, then ran the self-test on Windows 11 25H2, build 26200.9711, with one 5120x1440 display at 100% scale and 240Hz (measured on Windows 11 25H2, 2026-11-15, three reports in `experiments/areas/rack/results/`).

- `WindowKind::PopUp` gives the window `WS_EX_TOOLWINDOW` and `WS_EX_TOPMOST`, which keep it off the taskbar and on top of other windows, and `focus: false` shows it without activating it (read in gpui-pre-windows 0.3.12, src/window.rs, `WindowsWindow::new`).
  The rack opened without taking keyboard focus from the app that had it.
- GPUI can set a window's size but not its position, so the window test calls `SetWindowPos` with the native handle that GPUI's `Window` exposes through `raw-window-handle` (read in gpui-pre 0.3.12, src/window.rs, `Window::resize` and `impl HasWindowHandle for Window`).
- Moving the window from a foreground task on every frame moved it 240 times a second through two throws and a slide out of the dock, 1,240 moves in all.
  The median gap between moves was 4.2ms, one refresh interval, and the longest was 6.5ms.
  `SetWindowPos` took a median of 0.3ms.
  The OS never put the window anywhere other than where the window test moved it, and never started a move loop of its own.
- GPUI caps an inactive window at about 30 frames a second by default (read in gpui-pre 0.3.12, src/platform.rs, `WindowOptions::default`), and the rack is never the active window, so the window test sets `inactive_frame_interval` to `None`.
- GPUI answers every `WM_MOUSEACTIVATE` with `MA_ACTIVATE` (read in gpui-pre-windows 0.3.12, src/events.rs, `WindowsWindowInner::handle_msg`), so a click on the rack would take keyboard focus.
  A window subclass answers `MA_NOACTIVATE` before GPUI sees the message, and the window also carries `WS_EX_NOACTIVATE`.
  Whether a click then leaves focus where it was takes a person's click to measure.
- GPUI keeps Windows' resize frame on the left, right, and bottom of a window without a title bar, even one that can't be resized (read in gpui-pre-windows 0.3.12, src/events.rs, `handle_calc_client_size`).
  The frame is invisible, and it made the window 320x376 around a 304x368 rack, so a docked rack would have stopped 8px short of the screen edge.
  The subclass answers `WM_NCCALCSIZE` with the whole window as its client area, and the window then measured the rack's own size.
- With `WindowBackgroundAppearance::Transparent`, GPUI clears the window to transparent, but gpui-component's `Root` fills it with the theme's background, `#0A0A0A` (read in gpui-component 0.9.1, src/root.rs, the `Root` plugin's `style`).
  That showed as dark squares outside the rack's 14px corners until the window test set the theme's background to transparent.
- Without its frame, the window gets no rounded corners, border, or shadow from DWM.
  Captures before and after turning off DWM's corner rounding and border matched inside the window, and the screen beside the window was no darker than the screen further from it.

**Changed:** nothing in the design.
The rack's window in step 4 takes what the window test does: a PopUp window with its frame removed, clicks answered with `MA_NOACTIVATE`, the inactive frame cap off, moves made from a foreground task, and a transparent theme background.

**Affects:** [01-window](01-window.md) and [decision 001](../../decisions/001-rust-and-gpui.md), which this entry supports without settling, since the checks by hand and macOS remain.
