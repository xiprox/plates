//! The hover card: a session tile you rest the pointer on grows into a larger card, in a popup
//! window of its own so it can reach past the rack's edges, and shows what the tile cuts short,
//! as record/areas/rack/02-tiles.md describes.

use std::{
    cell::Cell,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant, SystemTime},
};

use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::*;
use plates_core::{
    Session, SessionState, Tile, TileId, WrapUp, desktop, git,
    timer::{MENU_MINUTES, Timer},
};

use crate::{
    markup::{self, Block},
    motion::Rect,
    native,
    rack_view::RackView,
    view::{
        APPROVAL_TEXT, BORDER, MONO, MUTED, SANS, TEXT, TEXT2, TILE, WORKING_TEXT, YELLOW,
        YOUR_TURN_TEXT, marker, minutes, session_words, styled_line, text, white,
    },
};

/// The card's width, in logical pixels.
pub const CARD_W: f32 = 360.;
/// How long the pointer rests on a tile's text before the tile grows into its card.
pub const CARD_DELAY: Duration = Duration::from_millis(400);
/// How long the card takes to grow from the tile to its full size (user, 2026-11-15).
const GROW: Duration = Duration::from_millis(180);
/// The tallest the card's middle, between its title and its controls, grows before it scrolls.
const BODY_MAX_H: f32 = 300.;
/// The card shows Claude's last message whole, and lays it out on every frame it's open, so a
/// message past this many characters is cut.
const MESSAGE_CHARS: usize = 12_000;

pub struct CardView {
    rack: WeakEntity<RackView>,
    tile: TileId,
    hwnd: isize,
    /// The tile's rectangle on screen, in physical pixels, which the card grows from. The card
    /// stays open while the pointer is on it or on the tile.
    from: Rect,
    /// The content's height in logical pixels, from the last frame's layout, which the window
    /// takes once the card has grown.
    measured: Rc<Cell<Option<f32>>>,
    grow_start: Option<Instant>,
    shadow: bool,
    applied: Option<(i32, i32, i32, i32)>,
    /// The session's folder and branch, such as "plates · main", read once when the card opens.
    place: String,
    closed: bool,
}

impl CardView {
    pub fn new(
        rack: WeakEntity<RackView>,
        tile: TileId,
        from: Rect,
        folder: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let hwnd = native::hwnd_of(window);
        // DWM draws the shadow around the whole window, so it waits for the card to finish
        // growing, as the menu's waits for its fade.
        cx.spawn(async move |_, _| {
            native::take_over(hwnd);
            native::set_bounds(hwnd, from);
            native::set_shadow(hwnd, false);
        })
        .detach();
        let name = folder.file_name().map_or_else(
            || folder.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let place = match git::branch(&folder) {
            Some(branch) => format!("{name} · {branch}"),
            None => name,
        };
        CardView {
            rack,
            tile,
            hwnd,
            from,
            measured: Rc::default(),
            grow_start: None,
            shadow: false,
            applied: Some(key(from)),
            place,
            closed: false,
        }
    }

    fn close(&mut self, window: &mut Window, cx: &mut App) {
        self.closed = true;
        let tile = self.tile;
        self.rack.update(cx, |rack, _| rack.card_closed(tile)).ok();
        window.remove_window();
    }

    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tile = self.tile;
        self.rack
            .update(cx, |rack, cx| rack.open_tile(tile, cx))
            .ok();
        self.close(window, cx);
    }

    // Sizes the window for this frame of the growth, and returns whether the card is full size.
    // The first frame lays the content out unseen, so the card knows the height it grows to.
    fn step(&mut self, scale: f64, cx: &mut Context<Self>) -> bool {
        let Some(height) = self.measured.get() else {
            return false;
        };
        let f = self.from;
        let area = native::edges_at(((f.left + f.right) / 2., (f.top + f.bottom) / 2.)).area;
        // The content sits inside the card's 1px border.
        let full = card_rect(f, CARD_W as f64 * scale, (height + 2.) as f64 * scale, area);
        let start = *self.grow_start.get_or_insert_with(Instant::now);
        let t = (start.elapsed().as_secs_f64() / GROW.as_secs_f64()).min(1.);
        let k = 1. - (1. - t).powi(3);
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

    fn content(&self, t: &Tile, s: &Session, cx: &mut Context<Self>) -> Div {
        let now = SystemTime::now();
        let link = s
            .app_id
            .as_deref()
            .and_then(desktop::session_link)
            .is_some();
        let header = div()
            .flex()
            .gap(px(10.))
            .child(div().flex_none().pt(px(5.)).child(marker(t, false)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(13.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .line_height(relative(1.25))
                            .line_clamp(2)
                            .child(t.title()),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(MUTED))
                            .child(session_words(s, now)),
                    ),
            );
        // The middle scrolls once it reaches BODY_MAX_H, so a long message can be read whole
        // while the title, the wrap-up, and the controls stay in place (user, 2026-11-15).
        let (blocks, pinned) = body(s);
        let middle = (!blocks.is_empty()).then(|| {
            div()
                .id("card-body")
                .flex()
                .flex_col()
                .gap(px(10.))
                .pr(px(8.))
                .max_h(px(BODY_MAX_H))
                .overflow_y_scrollbar()
                .children(blocks)
        });
        // Clicking the card above its controls opens the session, as clicking the tile does.
        let mut top = div()
            .id("card-top")
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(header)
            .children(middle)
            .children(pinned);
        if link {
            top = top
                .cursor_pointer()
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(window, cx)));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .px(px(14.))
            .pt(px(12.))
            .pb(px(10.))
            .child(top)
            .child(self.footer(t, s, link, cx))
    }

    fn footer(&self, t: &Tile, s: &Session, link: bool, cx: &mut Context<Self>) -> Div {
        let place = div()
            .text_size(px(11.))
            .text_color(rgb(MUTED))
            .truncate()
            .child(self.place.clone());
        let timer = (s.state != SessionState::Ended).then(|| self.timer_row(t, cx));
        let open = link.then(|| {
            div()
                .id("open")
                .flex_none()
                .h(px(26.))
                .px(px(10.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .bg(rgb(BORDER))
                .hover(|s| s.bg(rgb(0x34363C)))
                .text_size(px(12.))
                .text_color(rgb(WORKING_TEXT))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .child("Open")
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(window, cx)))
        });
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .pt(px(10.))
            .border_t_1()
            .border_color(white(0.06))
            .child(place)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .children(timer)
                    .child(div().flex_1())
                    .children(open),
            )
    }

    // The tile's timer: a chip for each length the timer menu offers, or while a timer runs, how
    // long it has left and a chip that cancels it.
    fn timer_row(&self, t: &Tile, cx: &mut Context<Self>) -> Div {
        let id = self.tile;
        let row = div().flex().items_center().gap(px(4.));
        if t.timer.is_running() {
            let left = t.timer.remaining(SystemTime::now()).unwrap_or_default();
            return row
                .child(
                    div()
                        .mr(px(4.))
                        .text_size(px(11.5))
                        .text_color(rgb(YELLOW))
                        .child(format!(
                            "Reminder in {}",
                            minutes(left + Duration::from_secs(59))
                        )),
                )
                .child(chip("cancel", "Cancel").on_click(cx.listener(
                    move |this, _: &ClickEvent, _, cx| {
                        this.rack
                            .update(cx, |rack, cx| rack.cancel_timer(id, cx))
                            .ok();
                    },
                )));
        }
        let hourglass = svg()
            .path("plates/hourglass.svg")
            .size(px(14.))
            .mr(px(2.))
            .text_color(if matches!(t.timer, Timer::TimeUp { .. }) {
                rgb(YELLOW).into()
            } else {
                text(0.5)
            });
        row.child(hourglass).children(MENU_MINUTES.iter().map(|&m| {
            let label = if m == 60 {
                "1h".to_string()
            } else {
                format!("{m}m")
            };
            chip(("minutes", m as u64), &label).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| {
                    this.rack
                        .update(cx, |rack, cx| rack.pick_minutes(id, m, cx))
                        .ok();
                },
            ))
        }))
    }
}

impl Render for CardView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.closed {
            return div().into_any_element();
        }
        // The card never takes keyboard focus or captures the mouse, so it reads where the
        // pointer is each frame, as the menu does, and closes once it's off both the card and
        // the tile, or once the tile has gone.
        window.request_animation_frame();
        let cursor = native::cursor_pos();
        let over = native::window_rect(self.hwnd).contains(cursor) || self.from.contains(cursor);
        let tile = self
            .rack
            .upgrade()
            .and_then(|rack| rack.read(cx).rack.tile(self.tile).cloned());
        let Some(tile) = tile.filter(|t| over && t.session().is_some()) else {
            self.close(window, cx);
            return div().into_any_element();
        };
        let Some(session) = tile.session() else {
            return div().into_any_element();
        };
        let grown = self.step(f64::from(window.scale_factor()), cx);
        let measured = self.measured.clone();
        // The text shows once the card is full size, rather than fading in as it grows, since
        // fades on hover read as slow (user, 2026-11-15).
        let content = self
            .content(&tile, session, cx)
            .opacity(if grown { 1. } else { 0. });
        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .rounded(px(10.))
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
                    .w(px(CARD_W - 2.))
                    .on_children_prepainted(move |bounds, _, _| {
                        if let Some(b) = bounds.first() {
                            measured.set(Some(f32::from(b.size.height)));
                        }
                    })
                    .child(content),
            )
            .into_any_element()
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

// The card's full rectangle: from the tile's top-left corner, out to the right and down, or to
// the left from the tile's right edge where the screen has no room on the right, and up from
// its bottom edge where it has none below (user, 2026-11-15).
fn card_rect(from: Rect, w: f64, h: f64, area: Rect) -> Rect {
    let h = h.min(area.bottom - area.top);
    let left = if from.left + w <= area.right {
        from.left
    } else {
        (from.right - w).max(area.left)
    };
    let top = if from.top + h <= area.bottom {
        from.top
    } else {
        (from.bottom - h).max(area.top)
    };
    Rect::new((left, top), (w, h))
}

fn chip(id: impl Into<ElementId>, label: &str) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(24.))
        .px(px(7.))
        .flex()
        .items_center()
        .rounded(px(6.))
        .bg(white(0.06))
        .hover(|s| s.bg(white(0.12)))
        .cursor_pointer()
        .text_size(px(11.5))
        .text_color(rgb(TEXT2))
        .child(label.to_string())
}

fn paragraph(line: &str, color: u32) -> Div {
    div()
        .text_size(px(12.))
        .text_color(rgb(color))
        .line_height(relative(1.35))
        .child(styled_line(line, rgb(color)))
}

// What the card shows below its title, by the session's state, in the order the session's
// transcript runs in the Claude desktop app: what came first at the top (user, 2026-11-15).
// The first part scrolls, and the second, the wrap-up's box, stays below it.
fn body(s: &Session) -> (Vec<AnyElement>, Option<AnyElement>) {
    let mut out: Vec<AnyElement> = Vec::new();
    let mut pinned = None;
    match s.state {
        SessionState::New => {}
        // A turn that ended with background agents still running stays working, and its card
        // shows the turn's last message and what Claude waits on.
        SessionState::Working if s.waiting_on() > 0 => {
            if let Some(message) = &s.message {
                out.push(message_blocks(message, TEXT).into_any_element());
            }
            out.push(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .children(s.background.iter().filter(|t| t.reports_back()).map(|t| {
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(MUTED))
                            .truncate()
                            .child(format!("{} · {}", task_kind(&t.kind), t.description))
                    }))
                    .into_any_element(),
            );
            if let Some(line) = &s.line {
                out.push(paragraph(line, WORKING_TEXT).into_any_element());
            }
        }
        SessionState::Working => {
            let earlier: Vec<&String> = s
                .recent
                .iter()
                .filter(|r| Some(*r) != s.line.as_ref())
                .collect();
            if !earlier.is_empty() {
                out.push(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .children(earlier.into_iter().map(|r| {
                            div()
                                .text_size(px(11.5))
                                .text_color(rgb(MUTED))
                                .truncate()
                                .child(styled_line(r, rgb(MUTED)))
                        }))
                        .into_any_element(),
                );
            }
            if let Some(line) = &s.line {
                out.push(paragraph(line, WORKING_TEXT).into_any_element());
            }
        }
        SessionState::NeedsApproval => {
            if let Some(summary) = &s.summary {
                out.push(paragraph(summary, APPROVAL_TEXT).into_any_element());
            }
            if let Some(command) = s.pending_tool.as_ref().and_then(|t| t.command.as_deref()) {
                let lines: Vec<String> = command.lines().map(str::to_string).collect();
                out.push(code(&lines).into_any_element());
            }
        }
        SessionState::YourTurn
        | SessionState::Unread
        | SessionState::Done
        | SessionState::Ended => {
            match &s.message {
                Some(message) => out.push(message_blocks(message, TEXT).into_any_element()),
                None if s.wrap_up.is_none() => {
                    if let Some(summary) = &s.summary {
                        out.push(paragraph(summary, TEXT).into_any_element());
                    }
                }
                None => {}
            }
            pinned = s.wrap_up.as_ref().map(wrap_up_box);
        }
    }
    (out, pinned)
}

// The wrap-up as the Claude desktop app draws it under Claude's message: the recap, then the
// question and its choices. It stays in view below the message, however far the message
// scrolls, so the question is never out of sight (user, 2026-11-15).
fn wrap_up_box(w: &WrapUp) -> AnyElement {
    let recap = w.recap.as_deref().map(|r| paragraph(r, TEXT2));
    let ask = w
        .ask
        .as_deref()
        .map(|a| paragraph(a, YOUR_TURN_TEXT).font_weight(FontWeight::SEMIBOLD));
    // The choices are the Claude desktop app's buttons, so here they only say what the choices
    // are, and a click on one opens the session, as a click anywhere above the card's controls
    // does.
    let choices = (!w.choices.is_empty()).then(|| {
        div()
            .flex()
            .flex_wrap()
            .gap(px(6.))
            .children(w.choices.iter().map(|c| {
                div()
                    .h(px(22.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(white(0.14))
                    .text_size(px(11.5))
                    .text_color(rgb(TEXT2))
                    .child(c.clone())
            }))
    });
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .p(px(10.))
        .rounded(px(8.))
        .bg(white(0.04))
        .border_1()
        .border_color(white(0.06))
        .children(recap)
        .children(ask)
        .children(choices)
        .into_any_element()
}

// A background task's `type` in words, such as "Subagent" for `subagent`.
fn task_kind(kind: &str) -> String {
    let mut chars = kind.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn message_blocks(message: &str, color: u32) -> Div {
    div().flex().flex_col().gap(px(6.)).children(
        markup::blocks(message, MESSAGE_CHARS)
            .into_iter()
            .map(|b| match b {
                Block::Text(line) => paragraph(&line, color),
                Block::Code(lines) => code(&lines),
            }),
    )
}

// Code in the monospace font, a line each, with a line too long for the card wrapped so all of
// it shows.
fn code(lines: &[String]) -> Div {
    div()
        .flex()
        .flex_col()
        .px(px(8.))
        .py(px(6.))
        .rounded(px(6.))
        .bg(white(0.05))
        .font_family(MONO)
        .text_size(px(11.5))
        .text_color(rgb(TEXT2))
        .line_height(relative(1.35))
        .children(lines.iter().map(|l| div().child(l.clone())))
}
