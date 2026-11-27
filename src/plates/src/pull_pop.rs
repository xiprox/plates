//! The pull request chip's pop-out, which grows out of the chip as the hover card grows out of
//! its tile, in a popup window of its own so it can reach past the rack's edges
//! (user, 2026-11-27; record/areas/rack/04-pull-requests.md).

use std::{cell::Cell, rc::Rc, time::Instant};

use gpui_kit::*;
use plates_core::TileId;

use crate::{
    card::GROW,
    motion::Rect,
    native,
    pull_chip::{self, POP_W},
    rack_view::RackView,
    view::{SANS, TILE},
};

/// The space between the chip and its pop-out.
const GAP: f64 = 6.;

pub struct PullPopView {
    rack: WeakEntity<RackView>,
    tile: TileId,
    url: String,
    hwnd: isize,
    /// The chip's rectangle on screen, in physical pixels, which the pop-out grows from.
    from: Rect,
    /// The content's height as the last layout measured it, in logical pixels.
    measured: Rc<Cell<Option<f32>>>,
    grow_start: Option<Instant>,
    shadow: bool,
    applied: Option<(i32, i32, i32, i32)>,
    closed: bool,
}

impl PullPopView {
    pub fn new(
        rack: WeakEntity<RackView>,
        tile: TileId,
        url: String,
        from: Rect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let hwnd = native::hwnd_of(window);
        cx.spawn(async move |_, _| {
            native::take_over(hwnd);
            native::set_bounds(hwnd, from);
            native::set_shadow(hwnd, false);
        })
        .detach();
        PullPopView {
            rack,
            tile,
            url,
            hwnd,
            from,
            measured: Rc::default(),
            grow_start: None,
            shadow: false,
            applied: None,
            closed: false,
        }
    }

    fn close(&mut self, window: &mut Window, cx: &mut App) {
        self.closed = true;
        let (tile, url) = (self.tile, self.url.clone());
        self.rack
            .update(cx, |rack, _| rack.pull_pop_closed(tile, &url))
            .ok();
        window.remove_window();
    }

    // Where the pop-out sits once grown: under the chip from its left edge, or over it where
    // the screen has no room below, and shifted left where it has none on the right.
    fn full(&self, w: f64, h: f64, scale: f64) -> Rect {
        let f = self.from;
        let area = native::edges_at(((f.left + f.right) / 2., (f.top + f.bottom) / 2.)).area;
        let gap = GAP * scale;
        let left = f.left.min(area.right - w).max(area.left);
        let top = if f.bottom + gap + h <= area.bottom {
            f.bottom + gap
        } else {
            (f.top - gap - h).max(area.top)
        };
        Rect::new((left, top), (w, h))
    }

    // Sizes the window for this frame of the growth, as the card's `step` does, and returns
    // the full rectangle and whether the pop-out has reached it.
    fn step(&mut self, scale: f64, cx: &mut Context<Self>) -> Option<(Rect, bool)> {
        let height = self.measured.get()?;
        let full = self.full(POP_W as f64 * scale, (height + 2.) as f64 * scale, scale);
        let start = *self.grow_start.get_or_insert_with(Instant::now);
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
        let key = (
            r.left.round() as i32,
            r.top.round() as i32,
            (r.right - r.left).round() as i32,
            (r.bottom - r.top).round() as i32,
        );
        let hwnd = self.hwnd;
        if self.applied != Some(key) {
            self.applied = Some(key);
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
        Some((full, grown))
    }
}

impl Render for PullPopView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.closed {
            return div().into_any_element();
        }
        // Like the card, the pop-out reads where the pointer is each frame, and closes once it's
        // off the chip, the pop-out, and the gap between them, or once the tile shows another
        // pull request or none.
        window.request_animation_frame();
        let Some(rack) = self.rack.upgrade() else {
            self.close(window, cx);
            return div().into_any_element();
        };
        let view = rack.read(cx);
        let content = view
            .rack
            .tile(self.tile)
            .and_then(|t| view.shown_pull(t))
            .filter(|(link, _)| link.url == self.url)
            .filter(|_| view.pull_pop_is(self.tile, &self.url))
            .map(|(link, more)| pull_chip::pop_content(&link, more, view.pulls.get(&link.url)));
        let Some(content) = content else {
            self.close(window, cx);
            return div().into_any_element();
        };
        let scale = native::scale(window);
        let stepped = self.step(scale, cx);
        let cursor = native::cursor_pos();
        let over = match stepped {
            Some((full, _)) => {
                let f = self.from;
                let span = Rect {
                    left: f.left.min(full.left),
                    top: f.top.min(full.top),
                    right: f.right.max(full.right),
                    bottom: f.bottom.max(full.bottom),
                };
                span.contains(cursor)
            }
            None => self.from.contains(cursor),
        };
        if !over {
            self.close(window, cx);
            return div().into_any_element();
        }
        let grown = stepped.is_some_and(|(_, grown)| grown);
        let measured = self.measured.clone();
        let url = self.url.clone();
        div()
            .id("pull-pop")
            .size_full()
            .relative()
            .overflow_hidden()
            .rounded(px(8.))
            .bg(rgb(TILE))
            .border_1()
            .border_color(rgb(0x34363C))
            .font_family(SANS)
            .line_height(relative(1.21))
            .cursor_pointer()
            // A click opens the pull request, as a click on the chip does.
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                let link = url.clone();
                this.rack
                    .update(cx, |_, cx| RackView::open_link(link, cx))
                    .ok();
                this.close(window, cx);
            }))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(px(POP_W - 2.))
                    .on_children_prepainted(move |bounds, _, _| {
                        if let Some(b) = bounds.first() {
                            measured.set(Some(f32::from(b.size.height)));
                        }
                    })
                    // As in the card, the text shows once the pop-out is full size.
                    .child(content.opacity(if grown { 1. } else { 0. })),
            )
            .into_any_element()
    }
}
