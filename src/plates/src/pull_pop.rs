//! The pull request chip's pop-out, which lists the session's open pull requests, one row each,
//! and grows out of the chip as the hover card grows out of its tile
//! (record/areas/rack/04-pull-requests.md).

use std::{cell::Cell, rc::Rc};

use gpui_kit::*;
use plates_core::TileId;

use crate::{
    motion::Rect,
    native,
    popup::Grow,
    pull_chip::{self, POP_W},
    rack_view::RackView,
    view::{SANS, TILE, white},
};

/// The space between the chip and its pop-out, in logical pixels.
const GAP: f64 = 6.;

pub struct PullPopView {
    rack: WeakEntity<RackView>,
    tile: TileId,
    hwnd: isize,
    /// The chip's rectangle on screen, in physical pixels, which the pop-out grows from.
    from: Rect,
    /// The content's height as the last layout measured it, in logical pixels.
    measured: Rc<Cell<Option<f32>>>,
    grow: Grow,
    closed: bool,
}

impl PullPopView {
    pub fn new(
        rack: WeakEntity<RackView>,
        tile: TileId,
        from: Rect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let hwnd = native::hwnd_of(window);
        let grow = Grow::new(hwnd, from, cx);
        PullPopView {
            rack,
            tile,
            hwnd,
            from,
            measured: Rc::default(),
            grow,
            closed: false,
        }
    }

    fn close(&mut self, window: &mut Window, cx: &mut App) {
        self.closed = true;
        let tile = self.tile;
        self.rack
            .update(cx, |rack, _| rack.pull_pop_closed(tile))
            .ok();
        window.remove_window();
    }

    // Where the pop-out sits once grown: under the chip from its left edge, or over it where the
    // screen has no room below, and shifted left where it has none on the right.
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

    // Whether the pointer is on the chip, on the pop-out's window, or in the gap between them, as
    // wide as the chip.
    fn over(&self, full: Option<Rect>, cursor: (f64, f64)) -> bool {
        let f = self.from;
        if f.contains(cursor) || native::window_rect(self.hwnd).contains(cursor) {
            return true;
        }
        full.is_some_and(|full| {
            let (top, bottom) = if full.top >= f.bottom {
                (f.bottom, full.top)
            } else {
                (full.bottom, f.top)
            };
            Rect {
                left: f.left,
                top,
                right: f.right,
                bottom,
            }
            .contains(cursor)
        })
    }
}

impl Render for PullPopView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.closed {
            return div().into_any_element();
        }
        // Like the card, the pop-out reads where the pointer is each frame, and closes once it's
        // off the chip, the pop-out, and the gap between them, or once the tile shows no pull
        // request, or once the rack stops wanting it.
        window.request_animation_frame();
        let Some(rack) = self.rack.upgrade() else {
            self.close(window, cx);
            return div().into_any_element();
        };
        let view = rack.read(cx);
        let rows: Option<Vec<_>> = view
            .rack
            .tile(self.tile)
            .and_then(|t| view.tile_pulls(t))
            .filter(|_| view.pull_pop_is(self.tile))
            .map(|on| {
                pull_chip::listed(&on)
                    .into_iter()
                    .map(|link| {
                        let row = pull_chip::pop_row(&link, view.pulls.get(&link.url));
                        (link.url, row)
                    })
                    .collect()
            });
        let Some(rows) = rows else {
            self.close(window, cx);
            return div().into_any_element();
        };
        let scale = native::scale(window);
        let full = self.measured.get().map(|height| {
            // The content sits inside the pop-out's 1px border.
            self.full(POP_W as f64 * scale, (height + 2.) as f64 * scale, scale)
        });
        let grown = full.is_some_and(|full| self.grow.step(full, cx));
        if !self.over(full, native::cursor_pos()) {
            self.close(window, cx);
            return div().into_any_element();
        }
        let measured = self.measured.clone();
        let last = rows.len().saturating_sub(1);
        let rows = rows.into_iter().enumerate().map(|(i, (url, row))| {
            let row = div()
                .id(("pull-row", i))
                .hover(|s| s.bg(white(0.04)))
                // A click on a row opens its pull request, as a click on the chip does.
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    let link = url.clone();
                    this.rack
                        .update(cx, |_, cx| RackView::open_link(link, cx))
                        .ok();
                    this.close(window, cx);
                }))
                .child(row);
            // A line under every row but the last.
            if i < last {
                row.border_b_1().border_color(white(0.06))
            } else {
                row
            }
        });
        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .rounded(px(8.))
            .bg(rgb(TILE))
            .border_1()
            .border_color(rgb(0x34363C))
            .font_family(SANS)
            .line_height(relative(1.21))
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
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .opacity(if grown { 1. } else { 0. })
                            .children(rows),
                    ),
            )
            .into_any_element()
    }
}
