//! Draws the rack and its docked strip as the mockups do, from their styles in Main.dc.html and
//! Docked.dc.html, and from the rules in record/areas/rack/02-tiles.md.

use std::time::{Duration, Instant, SystemTime};

use gpui_kit::component::{input::Input, tooltip::Tooltip};
use gpui_kit::*;
use plates_core::{Need, SessionState, Tile, rack::ENDED_CLEARS_AFTER, timer::Timer};

use crate::{
    markup::{self, Mark},
    menu::MenuKind,
    motion::{PEEK, Side},
    rack_view::{POPOVER_GAP, POPOVER_W, RackView, STRIP_W, TOAST_H},
};

const BG: u32 = 0x131417;
pub(crate) const BORDER: u32 = 0x2A2C31;
pub(crate) const TILE: u32 = 0x1C1D21;
pub(crate) const TEXT: u32 = 0xECEDEF;
const TEXT2: u32 = 0xA3A6AE;
pub(crate) const MUTED: u32 = 0x8B8E97;
const BLUE: u32 = 0x74AEFF;
const AMBER: u32 = 0xF2B441;
const HOT: u32 = 0xFF7A45;
const YELLOW: u32 = 0xF4D35E;
const GRAY: u32 = 0x5E626B;

/// How long a tile's background brightens when it starts needing you.
const BRIGHTEN: Duration = Duration::from_millis(1400);
/// A new session's tile brightens once it has grown in.
const ENTER: Duration = crate::rows::ENTER;

/// The font the rack's text uses. The root of the rack's view sets the same family.
const SANS: &str = "Inter";
/// A monospace font each OS ships with, for the code in Claude's lines.
const MONO: &str = if cfg!(windows) { "Consolas" } else { "Menlo" };

fn white(alpha: f32) -> Hsla {
    hsla(0., 0., 1., alpha)
}

// A row of the rack, `full` px tall once grown, at `factor` of its height and opacity, with what
// doesn't fit cut off below.
fn grown(row: AnyElement, full: f32, factor: f32) -> AnyElement {
    div()
        .flex_none()
        .h(px(full * factor))
        .overflow_hidden()
        .opacity(factor)
        .child(row)
        .into_any_element()
}

// A line Claude wrote, with its `code` in the monospace font on a faint background and its
// **bold** in bold, and the marks around each left out.
fn styled_line(line: &str, color: impl Into<Hsla>) -> StyledText {
    let color = color.into();
    let (text, spans) = markup::parse(line);
    let run = |len: usize, font: Font, background_color: Option<Hsla>| TextRun {
        len,
        font,
        color,
        background_color,
        underline: None,
        strikethrough: None,
    };
    let mut runs = Vec::new();
    let mut at = 0;
    for (range, mark) in spans {
        if range.start > at {
            runs.push(run(range.start - at, font(SANS), None));
        }
        runs.push(match mark {
            Mark::Code => run(range.len(), font(MONO), Some(white(0.08))),
            Mark::Bold => run(range.len(), font(SANS).bold(), None),
        });
        at = range.end;
    }
    if text.len() > at {
        runs.push(run(text.len() - at, font(SANS), None));
    }
    StyledText::new(text).with_runs(runs)
}

// The mockups' text color, #ECEDEF, at an alpha.
fn text(alpha: f32) -> Hsla {
    let mut c: Hsla = rgb(TEXT).into();
    c.a = alpha;
    c
}

fn mix(a: u32, b: u32, t: f32) -> u32 {
    let channel = |shift: u32| {
        let (x, y) = (((a >> shift) & 0xFF) as f32, ((b >> shift) & 0xFF) as f32);
        ((x + (y - x) * t).round() as u32) << shift
    };
    channel(16) | channel(8) | channel(0)
}

// The mockups lay 3.5% white over a hovered tile, so a tinted tile keeps its tint.
fn hovered(c: u32) -> u32 {
    mix(c, 0xFFFFFF, 0.035)
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

fn since(t: SystemTime, now: SystemTime) -> Duration {
    now.duration_since(t).unwrap_or_default()
}

fn minutes(d: Duration) -> String {
    let m = d.as_secs() / 60;
    match m {
        0 => "under a minute".into(),
        1..=59 => format!("{m}m"),
        _ => format!("{}h {}m", m / 60, m % 60),
    }
}

fn state(t: &Tile) -> Option<SessionState> {
    t.session().map(|s| s.state)
}

// The tile's background at rest: its time-up tint, in its status color.
fn tile_bg(t: &Tile) -> u32 {
    if !t.timer.is_time_up() {
        return TILE;
    }
    match state(t) {
        None => 0x2B2715,
        Some(SessionState::NeedsApproval) => 0x321F17,
        Some(_) => 0x2F2616,
    }
}

// The color a tile's background brightens to when it starts needing you, from the mockups'
// keyframes.
fn brighten_to(t: &Tile, need: Need) -> u32 {
    match (need, state(t)) {
        (Need::Approval, _) => 0x3D2519,
        (Need::YourTurn, _) => 0x3A311C,
        (Need::TimeUp, None) => 0x4A4119,
        (Need::TimeUp, Some(SessionState::NeedsApproval)) => 0x52301F,
        (Need::TimeUp, Some(_)) => 0x4C3B1D,
    }
}

// The mockups' brighten: up to the peak color 35% of the way in, then back to rest.
fn brightened(base: u32, peak: u32, elapsed: Duration, length: Duration) -> Option<u32> {
    let t = elapsed.as_secs_f32() / length.as_secs_f32();
    if t >= 1. {
        return None;
    }
    let k = if t < 0.35 {
        t / 0.35
    } else {
        1. - (t - 0.35) / 0.65
    };
    Some(mix(base, peak, k * (2. - k)))
}

// A tile's marker, or with `pip` its mark on the docked strip. The strip has no room for the
// timer, so your own tile's pip is outlined in yellow while its timer runs, and turns solid
// yellow once it's time-up, which needs you where a running timer doesn't (user, 2026-11-15).
fn marker(t: &Tile, pip: bool) -> AnyElement {
    let size = if pip { 8. } else { 9. };
    let base = div().flex_none().size(px(size));
    let (shape, color, breathe) = match state(t) {
        Some(SessionState::Working) => (base.rounded_full(), Some(BLUE), true),
        Some(SessionState::New) => (base.rounded_full(), Some(BLUE), false),
        Some(SessionState::YourTurn) => (base.rounded_full(), Some(AMBER), false),
        Some(SessionState::Done) => (base.rounded_full(), Some(GRAY), false),
        Some(SessionState::NeedsApproval) => (base.rounded_full(), Some(HOT), false),
        Some(SessionState::Ended) => (base.rounded_full(), None, false),
        None => (
            base.rounded(px(2.)),
            Some(if t.timer.is_time_up() { YELLOW } else { GRAY }),
            false,
        ),
    };
    let dot = match color {
        Some(c) if pip && t.is_own() && t.timer.is_running() => {
            shape.bg(rgb(c)).shadow(vec![ring(rgb(YELLOW).into(), 1.5)])
        }
        Some(c) => shape.bg(rgb(c)),
        None => shape.shadow(vec![ring(rgb(0x6B6E76).into(), 1.5)]),
    };
    if !breathe {
        return dot.into_any_element();
    }
    // A working session's dot breathes between 45% and 100% opacity every 2.4s. Drawing it 30
    // times a second keeps that smooth without redrawing the rack on every frame.
    let id = if pip { "pip-breathe" } else { "breathe" };
    dot.with_animation(
        (id, t.id.0),
        Animation::new(Duration::from_millis(2400))
            .repeat()
            .with_max_fps(30.),
        |d, t| d.opacity(0.45 + 0.55 * (0.5 - 0.5 * (t * std::f32::consts::TAU).cos())),
    )
    .into_any_element()
}

// A session tile's state in words, for its tooltip.
fn state_words(t: &Tile, now: SystemTime) -> Option<String> {
    let s = t.session()?;
    let in_state = since(s.state_since, now);
    let words = match t.timer {
        Timer::TimeUp { .. } => "Time's up".to_string(),
        Timer::Running { .. } => format!(
            "Reminder in {}",
            minutes(t.timer.remaining(now).unwrap_or_default() + Duration::from_secs(59))
        ),
        Timer::Idle => match s.state {
            SessionState::New => "New".into(),
            SessionState::Working => "Working".into(),
            SessionState::NeedsApproval => {
                format!("Needs your approval · {}", minutes(in_state))
            }
            SessionState::YourTurn => format!("Your turn · waiting {}", minutes(in_state)),
            SessionState::Done => format!("Done · finished {} ago", minutes(in_state)),
            SessionState::Ended => {
                let left = ENDED_CLEARS_AFTER.saturating_sub(in_state);
                format!(
                    "Ended · clears in {}",
                    minutes(left + Duration::from_secs(59))
                )
            }
        },
    };
    Some(words)
}

impl RackView {
    fn tile(&self, i: usize, t: &Tile, now: SystemTime, cx: &mut Context<Self>) -> AnyElement {
        if self.is_renaming(t.id) {
            return self.field_row(t, cx);
        }
        let id = t.id;
        let group: SharedString = format!("tile-{i}").into();
        let rest = tile_bg(t);
        let mut bg = rest;
        if let Some((need, since_at)) = t.need
            && let Some(c) = brightened(rest, brighten_to(t, need), since(since_at, now), BRIGHTEN)
        {
            bg = c;
        }
        let new_for = t
            .session()
            .filter(|s| s.state == SessionState::New)
            .map(|s| since(s.state_since, now));
        if let Some(c) = new_for
            .and_then(|d| d.checked_sub(ENTER))
            .and_then(|d| brightened(rest, 0x1F2C44, d, BRIGHTEN))
        {
            bg = c;
        }
        let hover_bg = hovered(bg);

        let summary = match state(t) {
            Some(SessionState::NeedsApproval) => t
                .session()
                .and_then(|s| s.summary.clone())
                .map(|s| (s, 0xFF9B6E)),
            Some(SessionState::YourTurn) => t
                .session()
                .and_then(|s| s.summary.clone())
                .map(|s| (s, 0xF2C46A)),
            // A done session's summary is in the secondary text color, so it reads as finished
            // rather than waiting on you (user, 2026-11-15).
            Some(SessionState::Done) => t
                .session()
                .and_then(|s| s.summary.clone())
                .map(|s| (s, TEXT2)),
            // A working tile shows Claude's newest text and the tool it used after it, as the
            // core's Session::line picks them (user, 2026-11-15).
            Some(SessionState::Working) => t
                .session()
                .and_then(|s| s.line.clone())
                .map(|s| (s, 0x9CC4FF)),
            _ => None,
        };
        // The tile cuts its subtitle to one line, so the tooltip shows it whole, above the
        // tile's state in words (user, 2026-11-15).
        let tip =
            state_words(t, now).map(|words| (summary.as_ref().map(|(s, _)| s.clone()), words));
        let ended = state(t) == Some(SessionState::Ended);
        let title = div()
            .text_size(px(13.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(if ended { 0x9A9DA5 } else { TEXT }))
            .child(t.title());
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
                    .child(styled_line(&s, rgb(c)))
            }));
        let mut main = div()
            .id(("main", id.0))
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(10.))
            .pl(px(12.))
            .pr(px(6.))
            .child(marker(t, false))
            .child(txt)
            .on_click(cx.listener(move |this, e: &ClickEvent, window, cx| {
                this.click_tile(id, e.click_count(), window, cx)
            }));
        if self.drag.is_none() {
            main = main.cursor_pointer();
        }
        if let Some((full, words)) = tip {
            main = main.tooltip(move |window, cx| {
                let (full, words) = (full.clone(), words.clone());
                Tooltip::element(move |_, _| {
                    div()
                        .max_w(px(260.))
                        .py(px(2.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .children(full.as_deref().map(|f| styled_line(f, rgb(TEXT))))
                        .child(div().text_color(rgb(MUTED)).child(words.clone()))
                })
                .build(window, cx)
            });
        }

        // Your own tile and an ended session's tile both show an × on hover, which removes the
        // tile, 8px from the timer slot or from the tile's edge (user, 2026-11-15).
        let removable = t.is_own() || ended;
        let side = removable.then(|| {
            let btn: SharedString = format!("x-{i}").into();
            div().flex().items_center().mr(px(8.)).child(
                div()
                    .id(("x", id.0))
                    .group(btn.clone())
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
                            .path("plates/x.svg")
                            .size(px(16.))
                            .text_color(text(0.6))
                            .group_hover(btn, |s| s.text_color(rgb(TEXT))),
                    )
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.remove_tile(id, cx)),
                    ),
            )
        });

        let hg = (!ended).then(|| self.timer_slot(i, t, now, cx));

        div()
            .id(("tile", id.0))
            .group(group)
            .h(px(60.))
            .flex()
            .rounded(px(10.))
            .overflow_hidden()
            .bg(rgb(bg))
            .hover(move |s| s.bg(rgb(hover_bg)))
            .child(main)
            .children(side)
            .children(hg)
            .into_any_element()
    }

    // The timer slot: the hourglass, which starts a 5-minute timer, or the draining ring, which
    // cancels it, and a right-click for the menu of lengths.
    fn timer_slot(
        &self,
        i: usize,
        t: &Tile,
        now: SystemTime,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let id = t.id;
        let hg_group: SharedString = format!("hg-{i}").into();
        let icon = match t.timer.left(now) {
            Some(left) => {
                let layer =
                    |path: SharedString| svg().absolute().top_0().left_0().size(px(20.)).path(path);
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
            None => svg()
                .path("plates/hourglass.svg")
                .size(px(20.))
                .text_color(text(0.5))
                .group_hover(hg_group.clone(), |s| s.text_color(rgb(TEXT)))
                .into_any_element(),
        };
        div()
            .id(("hg", id.0))
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
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.click_timer(id, cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    this.open_menu(MenuKind::Timer(id), e.position, cx)
                }),
            )
    }

    // The inline field, for a new tile in place of the phantom tile, or for renaming your own
    // tile in place of it.
    fn field_row(&self, t: &Tile, _cx: &mut Context<Self>) -> AnyElement {
        let marker = div()
            .flex_none()
            .size(px(9.))
            .rounded(px(2.))
            .bg(rgb(if t.timer.is_time_up() { YELLOW } else { GRAY }));
        Self::field_shell(marker, &self.input)
    }

    fn field_shell(
        marker: Div,
        input: &Entity<gpui_kit::component::input::InputState>,
    ) -> AnyElement {
        div()
            .h(px(60.))
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(12.))
            .rounded(px(10.))
            .bg(rgb(TILE))
            .shadow(vec![ring(rgb(BLUE).into(), 1.5)])
            .child(marker)
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(input)
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
                    .text_color(rgb(TEXT2))
                    .child("Enter"),
            )
            .into_any_element()
    }

    fn last_row(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.is_adding() {
            let marker = div().flex_none().size(px(9.)).rounded(px(2.)).bg(rgb(GRAY));
            return Self::field_shell(marker, &self.input);
        }
        let mut phantom = div()
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
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.start_field(crate::rack_view::FieldFor::New, window, cx)
            }));
        if self.drag.is_none() {
            phantom = phantom.cursor_pointer();
        }
        border_width(phantom, 1.5).into_any_element()
    }

    // The Undo row, which shows for 5s after you remove your own tile, and grows in and shrinks
    // out as a tile does.
    fn undo_row(&self, now: Instant, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (title, factor) = self.rows.undo(now)?;
        let row = div()
            .h(px(TOAST_H))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .px(px(14.))
            .rounded(px(9.))
            .bg(rgb(BG))
            .border_1()
            .border_color(rgb(BORDER))
            .text_size(px(12.5))
            .text_color(rgb(TEXT))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .child(format!("Removed \u{201c}{title}\u{201d}")),
            )
            .child(
                div()
                    .id("undo")
                    .flex_none()
                    .h(px(26.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .bg(rgb(BORDER))
                    .hover(|s| s.bg(rgb(0x34363C)))
                    .text_color(rgb(0x9CC4FF))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .child("Undo")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.undo_remove(cx))),
            );
        Some(grown(row.into_any_element(), TOAST_H + 6., factor))
    }

    pub(crate) fn render_rack(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let now = SystemTime::now();
        let hold = self
            .drag
            .as_ref()
            .and_then(|d| d.hold)
            .map(|(side, since)| {
                let p = since.elapsed().as_secs_f32() / crate::motion::HOLD.as_secs_f32();
                (side, p.min(1.))
            });
        // A shrinking tile is drawn above the tile it sat above, or after the last tile. Its
        // group and element names count from LEAVING, so they never match a present tile's.
        const LEAVING: usize = 10_000;
        let at = Instant::now();
        let present = self.rack.tiles();
        let leaving = self.rows.leaving();
        let mut tiles: Vec<AnyElement> = Vec::new();
        for (i, t) in present.iter().enumerate() {
            for (k, l) in leaving.iter().enumerate() {
                if l.before == Some(t.id) {
                    let el = self.tile(LEAVING + k, &l.tile, now, cx);
                    tiles.push(grown(el, 66., l.factor(at)));
                }
            }
            let el = self.tile(i, t, now, cx);
            tiles.push(grown(el, 66., self.rows.factor(t.id, at)));
        }
        for (k, l) in leaving.iter().enumerate() {
            if l.before.is_none_or(|b| present.iter().all(|t| t.id != b)) {
                let el = self.tile(LEAVING + k, &l.tile, now, cx);
                tiles.push(grown(el, 66., l.factor(at)));
            }
        }
        let undo = self.undo_row(at, cx);
        let last = self.last_row(cx);
        let band = div()
            .h(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, e: &MouseDownEvent, _, cx| {
                    this.open_menu(MenuKind::Rack, e.position, cx)
                }),
            )
            .child(
                svg()
                    .path("plates/grip.svg")
                    .w(px(16.))
                    .h(px(10.))
                    .text_color(rgb(GRAY)),
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
        let height = self.shown_height();
        let panel = div()
            .relative()
            .w_full()
            .h(px(height))
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
            // Each tile's row and the Undo row carry the 6px below them, so a row that grows or
            // shrinks takes its gap with it.
            .child(
                div()
                    .flex()
                    .flex_col()
                    .children(tiles)
                    .children(undo)
                    .child(last),
            )
            .children(dwell);
        div().size_full().relative().child(panel).into_any_element()
    }

    // The strip sits flush with the screen edge, rounded on its inner side only. Hovering it
    // peeks it 6px out of the edge, by widening the window on the inner side so the pointer at
    // the very edge stays over it, and lists the tiles that need you beside it.
    pub(crate) fn render_strip(&mut self, side: Side, cx: &mut Context<Self>) -> AnyElement {
        let pips = div()
            .flex_none()
            .w(px(STRIP_W))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.))
            .py(px(10.))
            .children(self.rack.tiles().iter().map(|t| marker(t, true)));
        let filler = self.peek.then(|| div().flex_none().w(px(PEEK as f32)));
        let strip = div()
            .id("strip")
            .flex_none()
            .h(px(crate::rack_view::strip_height(self.rack.tiles().len())))
            .flex()
            .bg(rgb(BG))
            .border_color(rgb(BORDER))
            .border_t_1()
            .border_b_1()
            .cursor_pointer()
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| this.set_peek(*hovered, cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, _| this.note_press()),
            )
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.undock(cx)));
        let strip = match side {
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
        };
        let popover = self.popover_shows().then(|| {
            let needing: Vec<&Tile> = self.rack.needs_you().collect();
            let header = format!(
                "{} NEED{} YOU",
                needing.len(),
                if needing.len() == 1 { "S" } else { "" }
            );
            div()
                .flex_none()
                .w(px(POPOVER_W))
                .px(px(12.))
                .py(px(10.))
                .flex()
                .flex_col()
                .gap(px(7.))
                .rounded(px(10.))
                .bg(rgb(BG))
                .border_1()
                .border_color(rgb(BORDER))
                .child(
                    div()
                        .text_size(px(10.5))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(MUTED))
                        .child(header),
                )
                .children(needing.into_iter().map(|t| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .min_w_0()
                        .text_size(px(11.5))
                        .text_color(rgb(TEXT))
                        .child(marker(t, true))
                        .child(div().min_w_0().truncate().child(t.title()))
                }))
        });
        let gap = popover
            .is_some()
            .then(|| div().flex_none().w(px(POPOVER_GAP)));
        let row = div().size_full().flex().items_start();
        match side {
            Side::Right => row.children(popover).children(gap).child(strip),
            Side::Left => row.child(strip).children(gap).children(popover),
        }
        .into_any_element()
    }

    /// Whether a tile is brightening or a new tile is sliding in, which needs a frame each
    /// display refresh until it ends.
    pub(crate) fn tiles_animating(&self) -> bool {
        let now = SystemTime::now();
        self.rack.tiles().iter().any(|t| {
            t.need.is_some_and(|(_, at)| since(at, now) < BRIGHTEN)
                || t.session().is_some_and(|s| {
                    s.state == SessionState::New && since(s.state_since, now) < ENTER + BRIGHTEN
                })
        })
    }
}

impl Render for RackView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.scale = window.scale_factor() as f64;
        self.track_rows();
        self.step(cx);
        self.place(cx);
        self.remember_place();
        if self.animating() || self.tiles_animating() || self.rows.animating(Instant::now()) {
            window.request_animation_frame();
        }
        let content = match self.docked {
            Some(side) => self.render_strip(side, cx),
            None => self.render_rack(cx),
        };
        div()
            .size_full()
            .font_family(SANS)
            // GPUI's default line height is 1.618, where a browser lays Inter out at about 1.21,
            // the mockups' line height, so the default spreads a title and its summary apart.
            // (read in gpui-pre 0.3.12, src/style.rs, `TextStyle::default`)
            .line_height(relative(1.21))
            .child(content)
    }
}
