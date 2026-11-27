//! A popup window that grows out of a rectangle on the rack, as the hover card grows out of its
//! tile and the pull request chip's pop-out grows out of the chip. Each opens in a window of its
//! own, so it can reach past the rack's edges.

use std::time::{Duration, Instant};

use gpui_kit::*;

use crate::{motion::Rect, native};

/// How long a popup takes to grow to its full size (user, 2026-11-15).
pub const GROW: Duration = Duration::from_millis(180);

/// The options for a popup's window, which opens over `from`, a rectangle on screen in physical
/// pixels, at the rack's `scale`. It never takes the keyboard.
pub fn options(from: Rect, scale: f64) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            point(
                px((from.left / scale) as f32),
                px((from.top / scale) as f32),
            ),
            size(
                px(((from.right - from.left) / scale) as f32),
                px(((from.bottom - from.top) / scale) as f32),
            ),
        ))),
        titlebar: None,
        focus: false,
        show: true,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Transparent,
        inactive_frame_interval: None,
        ..Default::default()
    }
}

/// A popup's growth from the rectangle it opens over to its full size.
pub struct Grow {
    hwnd: isize,
    /// The rectangle on screen, in physical pixels, the popup grows from.
    from: Rect,
    start: Option<Instant>,
    shadow: bool,
    applied: Option<(i32, i32, i32, i32)>,
}

impl Grow {
    /// Takes over the popup's window `hwnd` and sets it over `from`. DWM draws the shadow around
    /// the whole window, so the shadow waits for the popup to finish growing, as the menu's waits
    /// for its fade.
    pub fn new<V: 'static>(hwnd: isize, from: Rect, cx: &mut Context<V>) -> Self {
        cx.spawn(async move |_, _| {
            native::take_over(hwnd);
            native::set_bounds(hwnd, from);
            native::set_shadow(hwnd, false);
        })
        .detach();
        Grow {
            hwnd,
            from,
            start: None,
            shadow: false,
            applied: Some(key(from)),
        }
    }

    /// Sizes the window for this frame of the growth toward `full`, and returns whether the popup
    /// has reached it.
    pub fn step<V: 'static>(&mut self, full: Rect, cx: &mut Context<V>) -> bool {
        let start = *self.start.get_or_insert_with(Instant::now);
        let t = (start.elapsed().as_secs_f64() / GROW.as_secs_f64()).min(1.);
        let k = 1. - (1. - t).powi(3);
        let f = self.from;
        let lerp = |a: f64, b: f64| a + (b - a) * k;
        let r = Rect {
            left: lerp(f.left, full.left),
            top: lerp(f.top, full.top),
            right: lerp(f.right, full.right),
            bottom: lerp(f.bottom, full.bottom),
        };
        let hwnd = self.hwnd;
        if self.applied != Some(key(r)) {
            self.applied = Some(key(r));
            // GPUI can't take the WM_MOVE a SetWindowPos sends while it draws, as the rack's
            // RackView::place says, so the move waits for a task.
            cx.foreground_executor()
                .spawn(async move { native::set_bounds(hwnd, r) })
                .detach();
        }
        let grown = t >= 1.;
        if grown && !self.shadow {
            self.shadow = true;
            cx.foreground_executor()
                .spawn(async move { native::set_shadow(hwnd, true) })
                .detach();
        }
        grown
    }
}

fn key(r: Rect) -> (i32, i32, i32, i32) {
    (
        r.left.round() as i32,
        r.top.round() as i32,
        (r.right - r.left).round() as i32,
        (r.bottom - r.top).round() as i32,
    )
}
