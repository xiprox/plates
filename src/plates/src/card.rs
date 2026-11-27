//! The hover card: a session tile you rest the pointer on grows into a larger card, in a popup
//! window of its own so it can reach past the rack's edges, and shows what the tile cuts short,
//! as record/areas/rack/02-tiles.md describes.

use std::{
    cell::Cell,
    path::PathBuf,
    rc::Rc,
    time::{Duration, SystemTime},
};

use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::*;
use plates_core::{
    Session, SessionState, Tile, TileId, WrapUp, desktop, git,
    session::{Recent, StepStatus},
    timer::{MENU_HOURS, MENU_MINUTES, SNOOZE_HOURS, Timer},
};

use crate::{
    markup::{self, Align, Block},
    motion::Rect,
    native,
    popup::Grow,
    rack_view::RackView,
    view::{
        APPROVAL_TEXT, BLUE, GRAY, MONO, MUTED, SANS, TEXT, TEXT2, TILE, VIOLET, WORKING_TEXT,
        YELLOW, YOUR_TURN_TEXT, marker, minutes, session_words, styled_line, styled_line_weighted,
        text, white,
    },
};

/// The card's width, in logical pixels.
pub const CARD_W: f32 = 360.;
/// How long the pointer rests on a tile's text before the tile grows into its card.
pub const CARD_DELAY: Duration = Duration::from_millis(400);
/// The tallest the card's middle, between its title and its controls, grows before it scrolls.
const BODY_MAX_H: f32 = 300.;
/// The card shows Claude's last message whole, and lays it out on every frame it's open, so a
/// message past this many characters is cut.
const MESSAGE_CHARS: usize = 12_000;
/// A table cell's height: a line of 12px text at 1.35 times its size, and 5px above and below.
const TABLE_CELL_H: f32 = 26.;

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
    grow: Grow,
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
        let grow = Grow::new(hwnd, from, cx);
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
            grow,
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
        self.grow.step(full, cx)
    }

    fn content(&self, t: &Tile, s: &Session, parent: Option<&str>, cx: &mut Context<Self>) -> Div {
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
                            .child(session_words(t, now)),
                    ),
            );
        // The middle scrolls once it reaches BODY_MAX_H, so a long message can be read whole
        // while the title, the wrap-up, and the controls stay in place (user, 2026-11-15).
        let (blocks, boxed) = body(s);
        let pinned = s
            .wrap_up
            .as_ref()
            .filter(|_| boxed)
            .map(|w| self.wrap_up_box(w, link, cx));
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
            top =
                top.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(window, cx)));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .px(px(14.))
            .pt(px(12.))
            .pb(px(10.))
            .child(top)
            .child(self.footer(t, s, parent, cx))
    }

    // The wrap-up as the Claude desktop app draws it under Claude's message: the recap, then the
    // question and its choices. It stays in view below the message, however far the message
    // scrolls, so the question is never out of sight (user, 2026-11-15).
    fn wrap_up_box(&self, w: &WrapUp, link: bool, cx: &mut Context<Self>) -> AnyElement {
        let recap = w.recap.as_deref().map(|r| paragraph(r, TEXT2));
        let ask = w
            .ask
            .as_deref()
            .map(|a| paragraph_weighted(a, YOUR_TURN_TEXT, FontWeight::SEMIBOLD));
        // A choice answers the session as its button in the app does, by opening the session and
        // pressing that button (user, 2026-11-15). A session the app doesn't keep has no button
        // to press, so there the choices only say what they are.
        let pressable = link;
        let tile = self.tile;
        let choices = (!w.choices.is_empty()).then(|| {
            div()
                .flex()
                .flex_wrap()
                .gap(px(6.))
                .children(w.choices.iter().enumerate().map(|(i, c)| {
                    let label = div()
                        .id(("choice", i))
                        .h(px(24.))
                        .px(px(9.))
                        .flex()
                        .items_center()
                        .rounded(px(6.))
                        .text_size(px(11.5))
                        .child(c.clone());
                    if !pressable {
                        return label
                            .border_1()
                            .border_color(white(0.14))
                            .text_color(rgb(TEXT2))
                            .into_any_element();
                    }
                    let choice = c.clone();
                    label
                        .bg(white(0.08))
                        .hover(|s| s.bg(white(0.14)))
                        .text_color(rgb(TEXT))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            // The box sits in the part of the card whose click opens the session,
                            // which this click does already.
                            cx.stop_propagation();
                            this.rack
                                .update(cx, |rack, cx| rack.answer(tile, &choice, cx))
                                .ok();
                            this.close(window, cx);
                        }))
                        .into_any_element()
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

    fn footer(&self, t: &Tile, s: &Session, parent: Option<&str>, cx: &mut Context<Self>) -> Div {
        let meta = |text: String| {
            div()
                .text_size(px(11.))
                .text_color(rgb(MUTED))
                .truncate()
                .child(text)
        };
        let started = parent.map(|p| meta(format!("Started from \u{201c}{p}\u{201d}")));
        let timer = (s.state != SessionState::Ended).then(|| self.timer_row(t, cx));
        // The folder line says where the session runs, and the timer acts on it, so each gets a
        // divider of its own, since under one divider the folder line read as part of the timer
        // (user, 2026-11-16).
        let section = || {
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .pt(px(10.))
                .border_t_1()
                .border_color(white(0.06))
        };
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(section().children(started).child(meta(self.place.clone())))
            .children(timer.map(|row| section().child(row)))
    }

    // The tile's timer: a chip for each length in the timer menu's grid, and one for tomorrow, or
    // while a timer or a reminder for tomorrow is set, what's left and a chip that cancels it.
    // The menu's hours aren't chips too, since ten chips crowd the card (user, 2026-11-16).
    fn timer_row(&self, t: &Tile, cx: &mut Context<Self>) -> Div {
        let id = self.tile;
        let row = div().flex().items_center().gap(px(4.));
        let set = match t.timer {
            Timer::Running { .. } => {
                let left = t.timer.remaining(SystemTime::now()).unwrap_or_default();
                Some(format!(
                    "Reminder in {}",
                    minutes(left + Duration::from_secs(59))
                ))
            }
            Timer::Tomorrow { .. } => Some("Reminder tomorrow".to_string()),
            _ => None,
        };
        if let Some(words) = set {
            let color = if t.timer.is_tomorrow() {
                VIOLET
            } else {
                YELLOW
            };
            return row
                .child(
                    div()
                        .mr(px(4.))
                        .text_size(px(11.5))
                        .text_color(rgb(color))
                        .child(words),
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
        let remind = row.child(hourglass).children(
            MENU_MINUTES
                .iter()
                .map(|&m| (m, format!("{m}m")))
                .chain(MENU_HOURS.iter().map(|&h| (h * 60, format!("{h}h"))))
                .map(|(m, label)| {
                    chip(("minutes", m as u64), &label).on_click(cx.listener(
                        move |this, _: &ClickEvent, _, cx| {
                            this.rack
                                .update(cx, |rack, cx| rack.pick_minutes(id, m, cx))
                                .ok();
                        },
                    ))
                }),
        );
        // A second row snoozes the tile, after the moon, as the timer menu's second part does
        // (user, 2026-11-16).
        let moon = svg()
            .path("plates/moon.svg")
            .size(px(14.))
            .mr(px(2.))
            // Gray, as the hourglass beside the reminders is, since it names the row rather than
            // showing a snooze (user, 2026-11-16).
            .text_color(text(0.5));
        let tomorrow = chip("tomorrow", "Tomorrow").on_click(cx.listener(
            move |this, _: &ClickEvent, _, cx| {
                this.rack
                    .update(cx, |rack, cx| rack.pick_tomorrow(id, cx))
                    .ok();
            },
        ));
        let snooze = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .child(moon)
            .children(SNOOZE_HOURS.iter().map(|&h| {
                chip(("snooze", h as u64), &format!("{h}h")).on_click(cx.listener(
                    move |this, _: &ClickEvent, _, cx| {
                        this.rack
                            .update(cx, |rack, cx| rack.snooze_for(id, h, cx))
                            .ok();
                    },
                ))
            }))
            .child(tomorrow);
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(remind)
            .child(snooze)
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
        let pinned = self.rack.upgrade().is_some_and(|r| r.read(cx).showcase);
        let over =
            pinned || native::window_rect(self.hwnd).contains(cursor) || self.from.contains(cursor);
        // A sub-session's card names the session it was started from, while that one's tile is
        // on the rack.
        let (tile, parent) = match self.rack.upgrade() {
            Some(rack) => {
                let rack = &rack.read(cx).rack;
                let tile = rack.tile(self.tile).cloned();
                let parent = tile
                    .as_ref()
                    .and_then(|t| t.session())
                    .and_then(|s| s.app_parent.as_deref())
                    .and_then(|p| {
                        rack.tiles()
                            .iter()
                            .find(|o| o.session().and_then(|s| s.app_id.as_deref()) == Some(p))
                    })
                    .map(|o| o.title());
                (tile, parent)
            }
            None => (None, None),
        };
        let Some(tile) = tile.filter(|t| over && t.session().is_some()) else {
            self.close(window, cx);
            return div().into_any_element();
        };
        let Some(session) = tile.session() else {
            return div().into_any_element();
        };
        let grown = self.step(native::scale(window), cx);
        let measured = self.measured.clone();
        // The text shows once the card is full size, rather than fading in as it grows, since
        // fades on hover read as slow (user, 2026-11-15).
        let content = self
            .content(&tile, session, parent.as_deref(), cx)
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
        .text_size(px(11.5))
        .text_color(rgb(TEXT2))
        .child(label.to_string())
}

fn paragraph(line: &str, color: u32) -> Div {
    paragraph_weighted(line, color, FontWeight::NORMAL)
}

/// The column the card's log lines start with: a check, a cross, or the working dots, 14px wide.
const MARK_W: f32 = 14.;

fn mark(icon: &'static str) -> Div {
    div()
        .flex_none()
        .w(px(MARK_W))
        .flex()
        .justify_center()
        .child(svg().path(icon).size(px(11.)).text_color(rgb(MUTED)))
}

// What Claude does now, after three dots that light in turn every 400ms, as Claude's own
// working dots do. They step rather than glide, since the card draws every frame anyway and a
// step reads as calmer (user, 2026-11-16).
fn working_line(line: &str) -> Div {
    let step = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() / 400 % 3) as usize;
    let dots = div()
        .flex_none()
        .w(px(MARK_W))
        // The dots sit on the middle of the text's first line, which is 12px at 1.35.
        .h(px(16.))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(2.))
        .children((0..3).map(|i| {
            div()
                .size(px(3.))
                .rounded_full()
                .bg(rgb(WORKING_TEXT))
                .opacity(if i == step { 1. } else { 0.3 })
        }));
    div()
        .flex()
        .items_start()
        .gap(px(6.))
        .child(dots)
        .child(paragraph(line, WORKING_TEXT).flex_1().min_w_0())
}

fn paragraph_weighted(line: &str, color: u32, weight: FontWeight) -> Div {
    div()
        .text_size(px(12.))
        .text_color(rgb(color))
        .line_height(relative(1.35))
        .child(styled_line_weighted(line, rgb(color), weight))
}

// What the card shows below its title, by the session's state, in the order the session's
// transcript runs in the Claude desktop app: what came first at the top (user, 2026-11-15).
// The part it returns scrolls, and the wrap-up's box, when it says to show one, stays below it.
fn body(s: &Session) -> (Vec<AnyElement>, bool) {
    let mut out: Vec<AnyElement> = Vec::new();
    let mut boxed = false;
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
                            .child(format!("{} · {}", t.kind_name(), t.description))
                    }))
                    .into_any_element(),
            );
            if let Some(line) = &s.line {
                out.push(working_line(line).into_any_element());
            }
        }
        SessionState::Working if !s.plan.is_empty() => {
            out.push(plan_steps(s).into_any_element());
            if let Some(line) = &s.line {
                out.push(working_line(line).into_any_element());
            }
        }
        // The turn's earlier tools read as a log: a check for each one that finished, a cross for
        // one that failed, and the stepping dots beside what Claude does now (user, 2026-11-16).
        SessionState::Working => {
            let earlier: Vec<&Recent> = s
                .recent
                .iter()
                .filter(|r| Some(&r.text) != s.line.as_ref())
                .collect();
            if !earlier.is_empty() {
                out.push(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .children(earlier.into_iter().map(|r| {
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .child(mark(if r.failed {
                                    "plates/x.svg"
                                } else {
                                    "plates/check.svg"
                                }))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_size(px(11.5))
                                        .text_color(rgb(MUTED))
                                        .truncate()
                                        .child(styled_line(&r.text, rgb(MUTED))),
                                )
                        }))
                        .into_any_element(),
                );
            }
            if let Some(line) = &s.line {
                out.push(working_line(line).into_any_element());
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
        | SessionState::Waiting
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
            boxed = s.wrap_up.is_some();
        }
    }
    (out, boxed)
}

// The app's plan for the request, which says where the work stands in it where the turn's newest
// tools don't (user, 2026-11-21). Each step has the mark the task list gives a task in the same
// state.
fn plan_steps(s: &Session) -> Div {
    let mut rows = div().flex().flex_col().gap(px(3.));
    for step in &s.plan {
        let (mark, color) = match step.status {
            StepStatus::Completed => (
                svg()
                    .path("plates/check.svg")
                    .size(px(11.))
                    .text_color(rgb(GRAY))
                    .into_any_element(),
                MUTED,
            ),
            StepStatus::InProgress => (
                div()
                    .size(px(7.))
                    .rounded_full()
                    .bg(rgb(BLUE))
                    .into_any_element(),
                TEXT,
            ),
            StepStatus::Pending => (
                div()
                    .size(px(7.))
                    .rounded_full()
                    .border(px(1.5))
                    .border_color(rgb(GRAY))
                    .into_any_element(),
                MUTED,
            ),
        };
        rows = rows.child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .flex_none()
                        .w(px(MARK_W))
                        .flex()
                        .justify_center()
                        .child(mark),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(11.5))
                        .text_color(rgb(color))
                        .truncate()
                        .child(styled_line(&step.text, rgb(color))),
                ),
        );
    }
    rows
}

// A message as paragraphs: a heading in semibold, and a list item with its marker in a gutter of
// its own, so the item's wrapped lines line up with its text rather than with its marker, as the
// Claude desktop app draws a list (user, 2026-11-16).
fn message_blocks(message: &str, color: u32) -> Div {
    div().flex().flex_col().gap(px(6.)).children(
        markup::blocks(message, MESSAGE_CHARS)
            .into_iter()
            .enumerate()
            .map(|(i, b)| match b {
                Block::Text(line) => paragraph(&line, color),
                Block::Heading(line) => {
                    paragraph_weighted(&line, color, FontWeight::SEMIBOLD).text_size(px(12.5))
                }
                Block::Item {
                    marker,
                    depth,
                    text,
                } => {
                    // A number, such as "2.", takes more room than a bullet or a task's box.
                    let bullet = !marker.starts_with(|c: char| c.is_ascii_digit());
                    div()
                        .flex()
                        .pl(px(depth as f32 * 14.))
                        .child(
                            div()
                                .flex_none()
                                .w(px(if bullet { 14. } else { 20. }))
                                .text_size(px(12.))
                                .line_height(relative(1.35))
                                .text_color(rgb(if bullet { TEXT2 } else { color }))
                                .child(marker),
                        )
                        .child(div().flex_1().min_w_0().child(paragraph(&text, color)))
                }
                Block::Code(lines) => code(&lines),
                Block::Table {
                    header,
                    align,
                    rows,
                } => table(i, &header, &align, &rows, color).my(px(6.)),
            }),
    )
}

// A table as a grid of one-line cells, each column as wide as its widest cell, and the grid
// scrolls sideways when it's wider than the card, since cells wrapped to fit the card read as a
// jumble (user, 2026-11-23). The scroll takes only sideways deltas, so a trackpad swipe or a
// horizontal wheel moves the table while a vertical wheel scrolls the card past it. The header
// is in semibold over a rule, with a fainter rule between rows, and a header of empty cells
// draws nothing. `index`, the block's place in the message, keeps each table's scroll its own.
fn table(
    index: usize,
    header: &[String],
    align: &[Align],
    rows: &[Vec<String>],
    color: u32,
) -> Div {
    // Every cell is one line of the same height, so a column's cells line up with the other
    // columns' without the grid knowing any row's height. A column's gap to the next is padding
    // inside its cells, so the rules run on across the gap.
    let last = header.len() - 1;
    let cell = |text: &str, color: u32, weight: FontWeight, col: usize, rule: Option<f32>| {
        let c = div()
            .h(px(TABLE_CELL_H))
            .flex()
            .items_center()
            .whitespace_nowrap()
            .pr(px(if col < last { 16. } else { 0. }));
        let c = match align[col] {
            Align::Left => c,
            Align::Center => c.justify_center(),
            Align::Right => c.justify_end(),
        };
        let c = match rule {
            Some(alpha) => c.border_t_1().border_color(white(alpha)),
            None => c,
        };
        if text.is_empty() {
            c
        } else {
            c.child(paragraph_weighted(text, color, weight))
        }
    };
    let headed = header.iter().any(|c| !c.is_empty());
    let columns = (0..header.len()).map(|col| {
        let head = headed.then(|| cell(&header[col], TEXT2, FontWeight::SEMIBOLD, col, None));
        let body = rows.iter().enumerate().map(|(i, row)| {
            let rule = match (i, headed) {
                (0, false) => None,
                (0, true) => Some(0.14),
                _ => Some(0.06),
            };
            cell(&row[col], color, FontWeight::NORMAL, col, rule)
        });
        let column = div().flex().flex_col().children(head).children(body);
        // The last column takes the room the others leave, so a table narrower than the card
        // draws its rules across the card.
        if col < last {
            column.flex_none()
        } else {
            column.flex_grow_1()
        }
    });
    // A scroller has something to scroll only when its child reaches past it. GPUI lays out a div
    // as a block unless told otherwise, and a block stretches its child to its own width, so the
    // scroller is a flex row, in which the grid keeps its own width (record/areas/rack/log.md,
    // 2026-11-23).
    div().flex().child(
        div()
            .id(("card-table", index))
            .flex_1()
            .min_w_0()
            .flex()
            .overflow_x_scroll()
            .restrict_scroll_to_axis()
            .child(div().flex().flex_none().min_w_full().children(columns)),
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
