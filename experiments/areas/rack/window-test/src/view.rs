// Draws the rack and its docked strip as the mockups do, from their styles in Main.dc.html.
// The tiles are the mockups' sample tiles, and their timers only switch on and off.

use std::time::{Duration, Instant};

use gpui_kit::component::input::Input;
use gpui_kit::*;

use crate::motion::{PEEK, Side};
use crate::{Kind, Rack, State, Tile, Timer};

const BG: u32 = 0x131417;
const BORDER: u32 = 0x2A2C31;
const TILE: u32 = 0x1C1D21;
const TEXT: u32 = 0xECEDEF;
const BLUE: u32 = 0x74AEFF;
const YELLOW: u32 = 0xF4D35E;
const GRAY: u32 = 0x5E626B;

fn white(alpha: f32) -> Hsla {
    hsla(0., 0., 1., alpha)
}

// The mockups' text color, #ECEDEF, at an alpha.
fn text(alpha: f32) -> Hsla {
    let mut c: Hsla = rgb(TEXT).into();
    c.a = alpha;
    c
}

// The mockups lay 3.5% white over a hovered tile, so a tinted tile keeps its tint.
fn hovered(c: u32) -> u32 {
    let lift = |shift: u32| {
        let v = ((c >> shift) & 0xFF) as f32;
        ((v + (255. - v) * 0.035).round() as u32) << shift
    };
    lift(16) | lift(8) | lift(0)
}

fn border_width<E: Styled>(mut e: E, w: f32) -> E {
    let b = &mut e.style().border_widths;
    b.top = Some(px(w).into());
    b.right = Some(px(w).into());
    b.bottom = Some(px(w).into());
    b.left = Some(px(w).into());
    e
}

fn ring(color: Hsla, width: f32) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(width),
        inset: true,
    }
}

fn tile_bg(t: &Tile) -> u32 {
    match (t.timer, t.kind) {
        (Timer::TimeUp, Kind::Own) => 0x2B2715,
        (Timer::TimeUp, Kind::Session(State::YourTurn)) => 0x2F2616,
        (Timer::TimeUp, Kind::Session(State::Approval)) => 0x321F17,
        _ => TILE,
    }
}

// A tile's marker, or with `pip` its mark on the docked strip, which turns your own tile's
// square yellow while its timer runs, since the strip has no room for the timer.
fn marker(i: usize, t: &Tile, pip: bool) -> AnyElement {
    let size = if pip { 8. } else { 9. };
    let base = div().flex_none().size(px(size));
    let (shape, color, breathe) = match t.kind {
        Kind::Session(State::Working) => (base.rounded_full(), Some(BLUE), true),
        Kind::Session(State::YourTurn) => (base.rounded_full(), Some(0xF2B441), false),
        Kind::Session(State::Approval) => (base.rounded_full(), Some(0xFF7A45), false),
        Kind::Session(State::Ended) => (base.rounded_full(), None, false),
        Kind::Own => {
            let yellow = t.timer == Timer::TimeUp || (pip && matches!(t.timer, Timer::Running(_)));
            (
                base.rounded(px(2.)),
                Some(if yellow { YELLOW } else { GRAY }),
                false,
            )
        }
    };
    let dot = match color {
        Some(c) => shape.bg(rgb(c)),
        None => shape.shadow(vec![ring(rgb(0x6B6E76).into(), 1.5)]),
    };
    if !breathe {
        return dot.into_any_element();
    }
    // The mockups breathe a working session's dot between 45% and 100% opacity every 2.4s.
    // Drawing it 30 times a second keeps that smooth without redrawing the rack on every frame.
    let id = if pip {
        ("pip-breathe", i)
    } else {
        ("breathe", i)
    };
    dot.with_animation(
        id,
        Animation::new(Duration::from_millis(2400))
            .repeat()
            .with_max_fps(30.),
        |d, t| d.opacity(0.45 + 0.55 * (0.5 - 0.5 * (t * std::f32::consts::TAU).cos())),
    )
    .into_any_element()
}

impl Rack {
    fn tile(&self, i: usize, t: &Tile, cx: &mut Context<Self>) -> AnyElement {
        let group: SharedString = format!("tile-{i}").into();
        let bg = tile_bg(t);
        let hover = hovered(bg);
        let summary = match t.kind {
            Kind::Session(State::Approval) => t.summary.clone().map(|s| (s, 0xFF9B6E)),
            Kind::Session(State::YourTurn) => t.summary.clone().map(|s| (s, 0xF2C46A)),
            _ => None,
        };
        let title_color = if t.kind == Kind::Session(State::Ended) {
            0x9A9DA5
        } else {
            TEXT
        };
        let title = div()
            .text_size(px(13.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(title_color))
            .child(t.title.clone());
        let title = if summary.is_some() {
            title.truncate()
        } else {
            title.line_clamp(2).line_height(relative(1.25))
        };
        let txt = div()
            .flex()
            .flex_col()
            .gap(px(3.))
            .min_w_0()
            .flex_1()
            .child(title)
            .children(summary.map(|(s, c)| {
                div()
                    .text_size(px(12.))
                    .text_color(rgb(c))
                    .truncate()
                    .child(s)
            }));
        let mut main = div()
            .id(("main", i))
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(10.))
            .pl(px(12.))
            .pr(px(6.))
            .child(marker(i, t, false))
            .child(txt)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.click_main(i, cx)));
        if self.drag.is_none() {
            main = main.cursor_pointer();
        }

        // Your own tile and an ended session's tile both show an × on hover, which removes the
        // tile, 8px from the timer slot or from the tile's edge (user, 2026-11-15).
        let removable = matches!(t.kind, Kind::Own | Kind::Session(State::Ended));
        let side = removable.then(|| {
            let (path, size, margin) = ("plates/x.svg", 16., 8.);
            let btn_group: SharedString = format!("done-{i}").into();
            div().flex().items_center().mr(px(margin)).child(
                div()
                    .id(("done", i))
                    .group(btn_group.clone())
                    .size(px(32.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .opacity(0.)
                    .group_hover(group.clone(), |s| s.opacity(1.))
                    .hover(|s| s.bg(white(0.08)))
                    .cursor_pointer()
                    .child(
                        svg()
                            .path(path)
                            .size(px(size))
                            .text_color(text(0.6))
                            .group_hover(btn_group, |s| s.text_color(rgb(TEXT))),
                    )
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.remove_tile(i, cx)),
                    ),
            )
        });

        let hg = (t.kind != Kind::Session(State::Ended)).then(|| {
            let hg_group: SharedString = format!("hg-{i}").into();
            let icon = match t.timer {
                Timer::Running(left) => {
                    let layer = |path: SharedString| {
                        svg().absolute().top_0().left_0().size(px(20.)).path(path)
                    };
                    div()
                        .relative()
                        .size(px(20.))
                        .child(layer("plates/ring-track.svg".into()).text_color(white(0.14)))
                        .child(
                            layer(format!("plates/ring-{:.0}.svg", left * 100.).into())
                                .text_color(rgb(YELLOW)),
                        )
                        .child(
                            layer("plates/ring-x.svg".into())
                                .text_color(text(0.))
                                .group_hover(hg_group.clone(), |s| s.text_color(text(0.55))),
                        )
                        .into_any_element()
                }
                _ => svg()
                    .path("plates/hourglass.svg")
                    .size(px(20.))
                    .text_color(text(0.5))
                    .group_hover(hg_group.clone(), |s| s.text_color(rgb(TEXT)))
                    .into_any_element(),
            };
            div()
                .id(("hg", i))
                .group(hg_group)
                .flex_none()
                .w(px(44.))
                .flex()
                .items_center()
                .justify_center()
                .border_l_1()
                .border_color(white(0.06))
                .hover(|s| s.bg(white(0.06)))
                .cursor_pointer()
                .child(icon)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_timer(i, cx)))
        });

        div()
            .id(("tile", i))
            .group(group)
            .h(px(60.))
            .flex()
            .rounded(px(10.))
            .overflow_hidden()
            .bg(rgb(bg))
            .hover(move |s| s.bg(rgb(hover)))
            .child(main)
            .children(side)
            .children(hg)
            .into_any_element()
    }

    fn last_row(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.adding.is_some() {
            return div()
                .h(px(60.))
                .flex()
                .items_center()
                .gap(px(10.))
                .px(px(12.))
                .rounded(px(10.))
                .bg(rgb(TILE))
                .shadow(vec![ring(rgb(BLUE).into(), 1.5)])
                .child(div().flex_none().size(px(9.)).rounded(px(2.)).bg(rgb(GRAY)))
                .child(
                    div().flex_1().min_w_0().child(
                        Input::new(&self.input)
                            .appearance(false)
                            .px(px(0.))
                            .text_size(px(13.5))
                            .font_weight(FontWeight::SEMIBOLD),
                    ),
                )
                .child(
                    div()
                        .flex_none()
                        .px(px(6.))
                        .py(px(2.))
                        .rounded(px(5.))
                        .bg(rgb(0x25272C))
                        .text_size(px(10.5))
                        .text_color(rgb(0xA3A6AE))
                        .child("Enter"),
                )
                .into_any_element();
        }
        let phantom = div()
            .id("phantom")
            .group("phantom")
            .h(px(60.))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .rounded(px(10.))
            .border_dashed()
            .border_color(white(0.12))
            .hover(|s| s.border_color(white(0.22)).bg(white(0.03)))
            .child(
                svg()
                    .path("plates/plus.svg")
                    .size(px(18.))
                    .text_color(text(0.4))
                    .group_hover("phantom", |s| s.text_color(rgb(TEXT))),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(text(0.28))
                    .group_hover("phantom", |s| s.text_color(text(0.5)))
                    .child("Ctrl + Shift + Space"),
            )
            .on_click(
                cx.listener(|this, _: &ClickEvent, window, cx| this.start_adding(window, cx)),
            );
        let phantom = if self.drag.is_none() {
            phantom.cursor_pointer()
        } else {
            phantom
        };
        border_width(phantom, 1.5).into_any_element()
    }

    pub(crate) fn render_rack(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let now = Instant::now();
        let hold = self
            .drag
            .as_ref()
            .and_then(|d| d.hold)
            .map(|(side, since)| {
                let p = (now - since).as_secs_f32() / crate::motion::HOLD.as_secs_f32();
                (side, p.min(1.))
            });
        let tiles: Vec<AnyElement> = (0..self.tiles.len())
            .map(|i| self.tile(i, &self.tiles[i], cx))
            .collect();
        let last = self.last_row(cx);
        let band = div()
            .id("band")
            .h(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .child(
                svg()
                    .path("plates/grip.svg")
                    .w(px(16.))
                    .h(px(10.))
                    .text_color(rgb(GRAY)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _: &MouseDownEvent, _, cx| this.finish(cx)),
            );
        let dwell = hold.map(|(side, p)| {
            let bar = div()
                .absolute()
                .top_0()
                .w(px(4.))
                .h(relative(p))
                .rounded(px(4.))
                .bg(rgb(BLUE))
                .shadow(vec![BoxShadow {
                    color: hsla(215. / 360., 1., 0.73, 0.6),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(10.),
                    spread_radius: px(0.),
                    inset: false,
                }]);
            match side {
                Side::Left => bar.left_0(),
                Side::Right => bar.right_0(),
            }
        });
        div()
            .size_full()
            .relative()
            .font_family("Inter")
            // GPUI's default line height is 1.618, where a browser lays Inter out at about 1.21,
            // the mockups' line height, so the default spreads a title and its summary apart.
            // (read in gpui-pre 0.3.12, src/style.rs, `TextStyle::default`)
            .line_height(relative(1.21))
            .bg(rgb(BG))
            .rounded(px(14.))
            .border_1()
            .border_color(rgb(BORDER))
            .px(px(12.))
            .pb(px(12.))
            .cursor(if self.drag.is_some() {
                CursorStyle::ClosedHand
            } else {
                CursorStyle::OpenHand
            })
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_press))
            .child(band)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .children(tiles)
                    .child(last),
            )
            .children(dwell)
            .into_any_element()
    }

    // The strip sits flush with the screen edge, rounded on its inner side only.
    // Hovering it peeks it 6px out of the edge by widening the window on the inner side, so the
    // pointer at the very edge of the screen stays over it.
    pub(crate) fn render_strip(&mut self, side: Side, cx: &mut Context<Self>) -> AnyElement {
        let pips = div()
            .flex_none()
            .w(px(crate::STRIP_W))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.))
            .py(px(10.))
            .children(
                self.tiles
                    .iter()
                    .enumerate()
                    .map(|(i, t)| marker(i, t, true)),
            );
        let filler = self.peek.then(|| div().flex_none().w(px(PEEK as f32)));
        let strip = div()
            .id("strip")
            .size_full()
            .flex()
            .bg(rgb(BG))
            .border_color(rgb(BORDER))
            .border_t_1()
            .border_b_1()
            .cursor_pointer()
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| this.set_peek(*hovered, cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, _| {
                    this.last_press_focus = crate::native::foreground_window();
                }),
            )
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.check_click_focus(cx);
                this.undock(cx);
            }));
        match side {
            Side::Right => strip
                .border_l_1()
                .rounded_l(px(10.))
                .child(pips)
                .children(filler),
            Side::Left => strip
                .border_r_1()
                .rounded_r(px(10.))
                .children(filler)
                .child(pips),
        }
        .into_any_element()
    }
}
