//! The menu of timer lengths: a popup window of its own beside the hourglass that opened it, so
//! it can reach past the rack's edges without growing the rack's window.

use gpui_kit::*;
use plates_core::{TileId, timer::MENU_MINUTES};

use crate::{
    motion::Rect,
    native,
    rack_view::RackView,
    view::{BORDER, MUTED, TEXT, TILE},
};

pub const MENU_W: f32 = 156.;
/// A 6px pad top and bottom, a 23px heading, six 32px rows, and a 1px border.
pub const MENU_H: f32 = 230.;

pub struct MenuView {
    rack: WeakEntity<RackView>,
    tile: TileId,
    hwnd: isize,
    /// Whether the mouse buttons have been up since the menu opened, so the right-click that
    /// opened it doesn't close it.
    armed: bool,
}

impl MenuView {
    pub fn new(
        rack: WeakEntity<RackView>,
        tile: TileId,
        bounds: Rect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let hwnd = native::hwnd_of(window);
        cx.spawn(async move |_, _| {
            native::take_over(hwnd);
            native::set_bounds(hwnd, bounds);
        })
        .detach();
        MenuView {
            rack,
            tile,
            hwnd,
            armed: false,
        }
    }

    fn close(&mut self, window: &mut Window, cx: &mut App) {
        self.rack.update(cx, |rack, _| rack.menu_closed()).ok();
        window.remove_window();
    }
}

impl Render for MenuView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The menu never takes keyboard focus, so the OS doesn't tell it about a click elsewhere.
        // While it's open, it reads the mouse buttons each frame instead, and a press outside it
        // closes it.
        let down = native::left_button_down() || native::right_button_down();
        if !down {
            self.armed = true;
        } else if self.armed && !native::window_rect(self.hwnd).contains(native::cursor_pos()) {
            self.close(window, cx);
        }
        window.request_animation_frame();

        let tile = self.tile;
        let items = MENU_MINUTES.iter().map(|&m| {
            let label = if m == 60 {
                "1 hour".to_string()
            } else {
                format!("{m} min")
            };
            div()
                .id(("minutes", m as u64))
                .h(px(32.))
                .px(px(10.))
                .flex()
                .items_center()
                .justify_between()
                .rounded(px(6.))
                .text_size(px(13.))
                .text_color(rgb(TEXT))
                .hover(|s| s.bg(rgb(BORDER)))
                .cursor_pointer()
                .child(label)
                .children((m == 5).then(|| {
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(MUTED))
                        .child("click")
                }))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.rack
                        .update(cx, |rack, cx| rack.pick_minutes(tile, m, cx))
                        .ok();
                    window.remove_window();
                }))
        });
        div()
            .size_full()
            .font_family("Inter")
            .line_height(relative(1.21))
            .p(px(6.))
            .flex()
            .flex_col()
            .rounded(px(10.))
            .bg(rgb(TILE))
            .border_1()
            .border_color(rgb(0x34363C))
            .child(
                div()
                    .pt(px(6.))
                    .pb(px(4.))
                    .px(px(10.))
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(MUTED))
                    .child("REMIND ME IN"),
            )
            .children(items)
    }
}
