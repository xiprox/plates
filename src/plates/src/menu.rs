//! The rack's menus: a popup window of their own at the pointer that opened them, so they can
//! reach past the rack's edges without growing the rack's window.

use gpui_kit::*;
use plates_core::{TileId, timer::MENU_MINUTES};

use crate::{
    motion::Rect,
    native,
    rack_view::RackView,
    view::{BORDER, MUTED, TEXT, TILE},
};

pub const MENU_W: f32 = 156.;

/// Which menu a right-click opens.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MenuKind {
    /// The timer lengths, from a tile's hourglass.
    Timer(TileId),
    /// Plates itself, from the grip band at the rack's top.
    Rack,
}

impl MenuKind {
    pub fn height(self) -> f32 {
        match self {
            // A 6px pad top and bottom, a 23px heading, six 32px rows, and a 1px border.
            MenuKind::Timer(_) => 230.,
            // A 6px pad top and bottom, one 32px row, and a 1px border.
            MenuKind::Rack => 46.,
        }
    }
}

pub struct MenuView {
    rack: WeakEntity<RackView>,
    kind: MenuKind,
    hwnd: isize,
    /// Whether the mouse buttons have been up since the menu opened, so the right-click that
    /// opened it doesn't close it.
    armed: bool,
}

impl MenuView {
    pub fn new(
        rack: WeakEntity<RackView>,
        kind: MenuKind,
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
            kind,
            hwnd,
            armed: false,
        }
    }

    fn row(id: impl Into<ElementId>, label: &str) -> Stateful<Div> {
        div()
            .id(id)
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
            .child(label.to_string())
    }

    fn timer_items(&self, tile: TileId, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let heading = div()
            .pt(px(6.))
            .pb(px(4.))
            .px(px(10.))
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(MUTED))
            .child("REMIND ME IN")
            .into_any_element();
        let rows = MENU_MINUTES.iter().map(|&m| {
            let label = if m == 60 {
                "1 hour".to_string()
            } else {
                format!("{m} min")
            };
            Self::row(("minutes", m as u64), &label)
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
                .into_any_element()
        });
        std::iter::once(heading).chain(rows).collect()
    }

    fn rack_items(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        vec![
            Self::row("quit", "Quit Plates")
                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.quit()))
                .into_any_element(),
        ]
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

        let items = match self.kind {
            MenuKind::Timer(tile) => self.timer_items(tile, cx),
            MenuKind::Rack => self.rack_items(cx),
        };
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
            .children(items)
    }
}
