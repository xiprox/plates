//! Draws the rack and its docked strip as the mockups do, from their styles in Main.dc.html and
//! Docked.dc.html, and from the rules in record/areas/rack/02-tiles.md.

use std::time::{Duration, Instant, SystemTime};

use gpui_kit::component::input::Input;
use gpui_kit::*;
use plates_core::{Need, SessionState, Tile, Timer, rack::ENDED_CLEARS_AFTER};

use crate::{
    markup::{self, Mark},
    menu::MenuKind,
    motion::{PEEK, Side},
    native,
    rack_view::{FieldFor, POPOVER_GAP, POPOVER_W, RackView, STRIP_W, TOAST_H},
    swipe::Swiped,
};

const BG: u32 = 0x131417;
pub(crate) const BORDER: u32 = 0x2A2C31;
pub(crate) const TILE: u32 = 0x1C1D21;
pub(crate) const TEXT: u32 = 0xECEDEF;
pub(crate) const TEXT2: u32 = 0xA3A6AE;
pub(crate) const MUTED: u32 = 0x8B8E97;
pub(crate) const BLUE: u32 = 0x74AEFF;
pub(crate) const AMBER: u32 = 0xF2B441;
const HOT: u32 = 0xFF7A45;
pub(crate) const YELLOW: u32 = 0xF4D35E;
/// A reminder for tomorrow, apart from yellow's timer running now. It's about as bright as the
/// yellow, so the moon reads at 20px, and redder than the working blue, so the two don't blur
/// (user, 2026-11-16).
pub(crate) const VIOLET: u32 = 0xB39DFF;
pub(crate) const GREEN: u32 = 0x5BCB8A;
pub(crate) const GRAY: u32 = 0x5E626B;
/// The text colors of a working tile, a your-turn tile, and an approval.
pub(crate) const WORKING_TEXT: u32 = 0x9CC4FF;
pub(crate) const YOUR_TURN_TEXT: u32 = 0xF2C46A;
pub(crate) const APPROVAL_TEXT: u32 = 0xFF9B6E;
/// An update that's ready: the grip's dot and the dot in the rack's menu, apart from every
/// tile state's color so it doesn't read as a session needing you (user, 2026-11-25).
pub(crate) const TEAL: u32 = 0x2DD4BF;

/// Tiles shrinking out are drawn with indexes from here up, so their group and element names
/// never match a present tile's.
const LEAVING: usize = 10_000;

/// How long a tile's background brightens when it starts needing you.
pub(crate) const BRIGHTEN: Duration = Duration::from_millis(1400);
/// A new session's tile brightens once it has grown in.
const ENTER: Duration = crate::rows::ENTER;
/// How long a starting timer's hourglass takes to turn over before it fills with yellow.
const FLIP: Duration = Duration::from_millis(500);

/// The font the rack's text uses. The root of the rack's view sets the same family.
pub(crate) const SANS: &str = "Inter";
/// Inter's faces for the four weights the rack draws, which app.rs hands GPUI before the rack
/// opens. Without them, a Mac with no Inter installed draws the rack in a monospace font, since
/// GPUI's fallback for a missing family starts with `.ZedMono`. (read in gpui-pre 0.3.12,
/// src/text_system.rs, `TextSystem::new`)
/// When the rack starts drawing another weight, add that face from Inter's release to fonts/.
pub(crate) const SANS_FACES: [&[u8]; 4] = [
    include_bytes!("../fonts/Inter-Regular.ttf"),
    include_bytes!("../fonts/Inter-Medium.ttf"),
    include_bytes!("../fonts/Inter-SemiBold.ttf"),
    include_bytes!("../fonts/Inter-Bold.ttf"),
];
/// A monospace font each OS ships with, for the code in Claude's lines.
pub(crate) const MONO: &str = if cfg!(windows) { "Consolas" } else { "Menlo" };

pub(crate) fn white(alpha: f32) -> Hsla {
    hsla(0., 0., 1., alpha)
}

// A row of the rack, `full` px tall once grown, at `factor` of its height and opacity, with what
// doesn't fit cut off below. A tile swiped sideways is cut off at the window's edges rather than
// at the panel's padding, since the row reaches out through the panel's 12px padding and 1px
// border and pads itself back in (user, 2026-11-16).
fn grown(row: AnyElement, full: f32, factor: f32) -> AnyElement {
    div()
        .flex_none()
        .mx(px(-13.))
        .px(px(13.))
        .h(px(full * factor))
        .overflow_hidden()
        .opacity(factor)
        .child(row)
        .into_any_element()
}

/// The bell's size in the grip band.
const BELL_SIZE: f32 = 14.;
/// How long the bell swings after it turns.
const SWING: Duration = Duration::from_millis(600);

// The bell's angle, in radians, `SWING` into its turn: three and a half swings either way, up to
// 18°, each smaller than the last, and still at the end.
fn bell_angle(turned: Option<Instant>) -> f32 {
    let Some(at) = turned else {
        return 0.;
    };
    let t = at.elapsed().as_secs_f32() / SWING.as_secs_f32();
    if t >= 1. {
        return 0.;
    }
    18f32.to_radians() * (1. - t).powi(2) * (std::f32::consts::TAU * 3.5 * t).sin()
}

/// How far a sub-session's tile sits in from its parent's, while its right edge stays flush with
/// the other tiles' (user, 2026-11-15).
const NEST: f32 = 26.;

/// A tile's height, which the phantom tile and the field for a new tile share.
const TILE_H: f32 = 60.;

/// A tile's row once grown: the tile and the 6px gap below it.
pub(crate) const ROW_H: f32 = TILE_H + 6.;

/// How many levels deep the rack nests sub-sessions, as the Claude desktop app's sidebar nests a
/// sub-session's own sub-sessions under it. A session started further down sits at the last level,
/// so no tile is narrower than two indents (user, 2026-11-16).
const MAX_DEPTH: usize = 2;

// How deep to draw each of the rack's tiles: 0 for a tile that heads a family or has none, and
// one level below its parent's for a sub-session whose parent is earlier in the same run of
// nested tiles, up to MAX_DEPTH, so a family reads as one group. A sub-session whose parent's tile
// is gone, or sits elsewhere, keeps its place at 0.
fn nesting(tiles: &[Tile]) -> Vec<usize> {
    // The app IDs of the run's tiles so far, each with its depth.
    let mut family: Vec<(&str, usize)> = Vec::new();
    tiles
        .iter()
        .map(|t| {
            let Some(s) = t.session() else {
                family.clear();
                return 0;
            };
            let depth = s
                .app_parent
                .as_deref()
                .and_then(|p| family.iter().find(|(id, _)| *id == p))
                .map_or(0, |(_, d)| (d + 1).min(MAX_DEPTH));
            if depth == 0 {
                family.clear();
            }
            family.extend(s.app_id.as_deref().map(|id| (id, depth)));
            depth
        })
        .collect()
}

// For each row, drawn at `depths`, and each level from 1 to the row's depth, whether that level's
// line runs on past the row: whether a later row sits at that level before any row sits above
// it. At the row's own level that makes its line a ├ rather than a └, and at a level above, a line
// passing beside the row on its way to a later sibling of the row's ancestor.
fn runs_on(depths: &[usize]) -> Vec<Vec<bool>> {
    depths
        .iter()
        .enumerate()
        .map(|(i, &depth)| {
            (1..=depth)
                .map(|level| depths[i + 1..].iter().find(|&&d| d <= level) == Some(&level))
                .collect()
        })
        .collect()
}

// A sub-session's tile, indented a NEST for each level, with a line that comes down from under
// its parent's marker and turns into the tile, as the Claude desktop app's sidebar draws a
// session's side sessions. Where a level's line runs on, it goes straight down the row, through
// the gap below the tile, to meet the next tile's line at that level, so a family's lines have no
// breaks (user, 2026-11-16). A swipe that moves the tile with an ancestor moves the lines from
// that ancestor's marker and the ones below it with them: the levels past `fixed`, `shift` px
// sideways.
fn nested_row(
    tile: AnyElement,
    runs_on: &[bool],
    shift: f32,
    fixed: usize,
    row_h: f32,
) -> AnyElement {
    let depth = runs_on.len();
    let color = white(0.16);
    let mut lines: Vec<Div> = Vec::new();
    for (k, &on) in runs_on.iter().enumerate() {
        // A tile's marker is centered 16.5px in from its left edge: 12px of padding and half of
        // the 9px marker. So level k + 1's line runs down at 16px in from the left edge of a tile
        // at level k, which sits k NESTs in, and meets the tile at y 29.
        let x = k as f32 * NEST + 16. + if k >= fixed { shift } else { 0. };
        let own = k + 1 == depth;
        if on {
            // A border on a box with no width draws nothing, so these lines are filled 1px boxes
            // (record/areas/rack/log.md, 2026-11-16). The row is the tile and the gap below it,
            // taller than ROW_H by any task section, so the line reaches the next row.
            let down = div().w(px(1.)).h(px(row_h)).left(px(x)).top_0();
            lines.push(down.absolute().bg(color));
            if own {
                let across = div()
                    .w(px(NEST - 20.))
                    .h(px(1.))
                    .left(px(x + 1.))
                    .top(px(29.));
                lines.push(across.absolute().bg(color));
            }
        } else if own {
            lines.push(
                div()
                    .absolute()
                    .left(px(x))
                    .top_0()
                    .w(px(NEST - 19.))
                    .h(px(30.))
                    .border_l_1()
                    .border_b_1()
                    .rounded_bl(px(5.))
                    .border_color(color),
            );
        }
    }
    div()
        .flex()
        .child(
            div()
                .flex_none()
                .relative()
                .w(px(depth as f32 * NEST))
                .h(px(row_h - 6.))
                .children(lines),
        )
        .child(div().flex_1().min_w_0().child(tile))
        .into_any_element()
}

// A line Claude wrote, with its `code` in the monospace font on a faint background and its
// **bold** in bold, and the marks around each left out.
pub(crate) fn styled_line(line: &str, color: impl Into<Hsla>) -> StyledText {
    styled_line_weighted(line, color, FontWeight::NORMAL)
}

// A styled line at a weight of its own, such as a heading's. Each run of the line carries its
// whole font, so a parent element's font weight doesn't reach the text, and the weight has to be
// the runs'. (read in gpui-pre 0.3.12, src/elements/text.rs, StyledText::with_runs)
pub(crate) fn styled_line_weighted(
    line: &str,
    color: impl Into<Hsla>,
    weight: FontWeight,
) -> StyledText {
    let color: Hsla = color.into();
    let (text, spans) = markup::parse(line);
    // A link's text holds marks of its own, so the line splits into runs at every span's edges,
    // and each run takes the marks of every span around it.
    let mut cuts: Vec<usize> = vec![0, text.len()];
    cuts.extend(spans.iter().flat_map(|(r, _)| [r.start, r.end]));
    cuts.sort_unstable();
    cuts.dedup();
    // A link is underlined, faintly, and isn't clickable (user, 2026-11-15).
    let mut underline = color;
    underline.a *= 0.45;
    let runs = cuts
        .windows(2)
        .map(|w| {
            let has = |mark: Mark| {
                spans
                    .iter()
                    .any(|(r, m)| *m == mark && r.start <= w[0] && w[1] <= r.end)
            };
            let code = has(Mark::Code);
            let mut f = font(if code { MONO } else { SANS });
            f.weight = weight;
            if has(Mark::Bold) {
                f = f.bold();
            }
            TextRun {
                len: w[1] - w[0],
                font: f,
                color,
                background_color: code.then(|| white(0.08)),
                underline: has(Mark::Link).then_some(UnderlineStyle {
                    thickness: px(1.),
                    color: Some(underline),
                    wavy: false,
                }),
                strikethrough: None,
            }
        })
        .filter(|r| r.len > 0)
        .collect::<Vec<_>>();
    StyledText::new(text).with_runs(runs)
}

// The mockups' text color, #ECEDEF, at an alpha.
pub(crate) fn text(alpha: f32) -> Hsla {
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

/// How far a timer that started less than `FLIP` ago has turned its hourglass over, from 0 to 1.
fn flipping(t: &Tile, now: SystemTime) -> Option<f32> {
    let Timer::Running { started, .. } = t.timer else {
        return None;
    };
    let gone = since(started, now);
    (gone < FLIP).then(|| gone.as_secs_f32() / FLIP.as_secs_f32())
}

fn since(t: SystemTime, now: SystemTime) -> Duration {
    now.duration_since(t).unwrap_or_default()
}

pub(crate) fn minutes(d: Duration) -> String {
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
        (Need::Unread, _) => 0x1D3A2B,
        (Need::TimeUp, None) => 0x4A4119,
        (Need::TimeUp, Some(SessionState::NeedsApproval)) => 0x52301F,
        (Need::TimeUp, Some(_)) => 0x4C3B1D,
    }
}

// The mockups' brighten: up to the peak color 35% of the way in, then back to rest.
fn brightened(base: u32, peak: u32, elapsed: Duration, length: Duration) -> Option<u32> {
    swell(elapsed, length).map(|k| mix(base, peak, k))
}

/// How far a brighten `elapsed` into its `length` stands from rest toward its peak, from 0 to 1,
/// or `None` once it's over.
pub(crate) fn swell(elapsed: Duration, length: Duration) -> Option<f32> {
    let t = elapsed.as_secs_f32() / length.as_secs_f32();
    if t >= 1. {
        return None;
    }
    let k = if t < 0.35 {
        t / 0.35
    } else {
        1. - (t - 0.35) / 0.65
    };
    Some(k * (2. - k))
}

// A tile's marker, or with `pip` its mark on the docked strip. The strip has no room for the
// timer, so your own tile's pip is outlined in yellow while its timer runs, or violet while it's
// set for tomorrow, and turns solid yellow once it's time-up, which needs you where a running
// timer doesn't (user, 2026-11-15 and 2026-11-16).
/// Whether the working dots hold still, as in `plates showcase`, whose picture would catch a
/// breathing dot at whatever opacity its breath had reached.
pub(crate) static STILL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) fn marker(t: &Tile, pip: bool) -> AnyElement {
    let size = if pip { 8. } else { 9. };
    let base = div().flex_none().size(px(size));
    let (shape, color, breathe) = match state(t) {
        Some(SessionState::Working) => (base.rounded_full(), Some(BLUE), true),
        Some(SessionState::New) => (base.rounded_full(), Some(BLUE), false),
        // A your-turn tile you've read keeps its amber as a ring, since your reply is still
        // owed, without the solid dot that asks for attention (user, 2026-11-15).
        Some(SessionState::YourTurn) if t.session().is_some_and(|s| s.read) => {
            return base
                .rounded_full()
                .shadow(vec![ring(rgb(AMBER).into(), 1.5)])
                .into_any_element();
        }
        Some(SessionState::YourTurn) => (base.rounded_full(), Some(AMBER), false),
        Some(SessionState::Unread) => (base.rounded_full(), Some(GREEN), false),
        Some(SessionState::Done) => (base.rounded_full(), Some(GRAY), false),
        Some(SessionState::NeedsApproval) => (base.rounded_full(), Some(HOT), false),
        Some(SessionState::Ended) => (base.rounded_full(), None, false),
        // A session waiting on something outside it takes the mark of a task waiting on someone
        // else, two bars, since nothing in it needs you (record/areas/tasks/02-waits-and-pings.md).
        Some(SessionState::Waiting) => {
            return pause_mark(size - 1., size, MUTED)
                .flex_none()
                .into_any_element();
        }
        None => (
            base.rounded(px(2.)),
            Some(if t.timer.is_time_up() { YELLOW } else { GRAY }),
            false,
        ),
    };
    let dot = match color {
        Some(c) if pip && t.is_own() && t.timer.is_set() => {
            let outline = if t.timer.is_tomorrow() {
                VIOLET
            } else {
                YELLOW
            };
            shape
                .bg(rgb(c))
                .shadow(vec![ring(rgb(outline).into(), 1.5)])
        }
        Some(c) => shape.bg(rgb(c)),
        None => shape.shadow(vec![ring(rgb(0x6B6E76).into(), 1.5)]),
    };
    if !breathe || STILL.load(std::sync::atomic::Ordering::Relaxed) {
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

/// The pause mark (‖), two bars, which a task waiting on someone else shows in its list and a
/// waiting session shows as its marker (record/areas/tasks/02-waits-and-pings.md).
pub(crate) fn pause_mark(w: f32, h: f32, color: u32) -> Div {
    let bar = || div().w(px(2.)).h_full().rounded(px(1.)).bg(rgb(color));
    div()
        .w(px(w))
        .h(px(h))
        .flex()
        .justify_between()
        .child(bar())
        .child(bar())
}

/// A session tile's state in words, with how long it has been in it, such as "Your turn ·
/// waiting 12m", for the hover card and an ended tile's summary line.
pub(crate) fn session_words(t: &Tile, now: SystemTime) -> String {
    let Some(s) = t.session() else {
        return String::new();
    };
    let in_state = since(s.state_since, now);
    // A timer set for tomorrow, or one that's run out, holds an ended tile past the time it would
    // clear, so the tile then says only that the session ended.
    let held = t.timer.is_tomorrow() || t.timer.is_time_up();
    match s.state {
        SessionState::New => "New".into(),
        SessionState::Working => format!("Working for {}", minutes(in_state)),
        SessionState::NeedsApproval if s.question => {
            format!("Needs your input · {}", minutes(in_state))
        }
        SessionState::NeedsApproval => format!("Needs your approval · {}", minutes(in_state)),
        SessionState::YourTurn if s.read => {
            format!("Your turn · read · waiting {}", minutes(in_state))
        }
        SessionState::YourTurn => format!("Your turn · waiting {}", minutes(in_state)),
        SessionState::Unread => format!("Finished {} ago · not read yet", minutes(in_state)),
        SessionState::Done => format!("Done · read {} ago", minutes(in_state)),
        SessionState::Waiting => format!(
            "Waiting on {} · {}",
            s.summary.as_deref().unwrap_or("something"),
            minutes(in_state)
        ),
        SessionState::Ended if held || in_state >= ENDED_CLEARS_AFTER => "Ended".into(),
        SessionState::Ended => {
            let left = ENDED_CLEARS_AFTER.saturating_sub(in_state);
            format!(
                "Ended · clears in {}",
                minutes(left + Duration::from_secs(59))
            )
        }
    }
}

impl RackView {
    fn tile(&self, i: usize, t: &Tile, now: SystemTime, cx: &mut Context<Self>) -> AnyElement {
        if self.is_renaming(t.id) {
            return self.field_row(t, cx);
        }
        let id = t.id;
        let rest = tile_bg(t);
        let mut bg = rest;
        // A tile that started needing you while you were busy waits to brighten until the held
        // pings come (record/areas/rack/03-holding-pings.md).
        if let Some((need, since_at)) = t.need
            && !self.hold.holds(since_at)
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
                .map(|s| (s, APPROVAL_TEXT)),
            Some(SessionState::YourTurn) => t.session().and_then(|s| {
                let color = if s.read { TEXT2 } else { YOUR_TURN_TEXT };
                s.summary.clone().map(|line| (line, color))
            }),
            // A finished turn you haven't read shows Claude's last line in full white, like a new
            // message, and once you've read it, in the secondary text color (user, 2026-11-15).
            Some(SessionState::Unread) => t
                .session()
                .and_then(|s| s.summary.clone())
                .map(|s| (s, TEXT)),
            Some(SessionState::Done) => t
                .session()
                .and_then(|s| s.summary.clone())
                .map(|s| (s, TEXT2)),
            // An ended tile says when it clears itself, which a tooltip alone hid (user,
            // 2026-11-15).
            Some(SessionState::Ended) => Some((session_words(t, now), MUTED)),
            // A waiting tile counts up how long it has waited, so a wait whose session stopped
            // shows how long it has gone unanswered.
            Some(SessionState::Waiting) => Some((session_words(t, now), TEXT2)),
            // A working tile shows Claude's newest text and the tool it used after it, as the
            // core's Session::line picks them (user, 2026-11-15).
            Some(SessionState::Working) => t
                .session()
                .and_then(|s| s.line.clone())
                .map(|s| (s, WORKING_TEXT)),
            _ => None,
        };
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
        // A pull request's chip sits at the end of the title: beside its one line, or level with
        // the first of its two.
        let chip = self.tile_pulls(t).map(|on| self.pull_chip(id, &on, cx));
        let title = match chip {
            Some(chip) => {
                let row = div().flex().gap(px(6.)).min_w_0();
                let row = if summary.is_some() {
                    row.items_center()
                } else {
                    row.items_start()
                };
                row.child(title.min_w_0()).child(chip).into_any_element()
            }
            None => title.into_any_element(),
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
                    .child(styled_line(&markup::summary_line(&s), rgb(c)))
            }));
        let mut main = div()
            .id(("main", id.0))
            .relative()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(10.))
            .pl(px(12.))
            // Beside the timer slot, the text stops 6px short of its divider, and the pull
            // request chip's right margin in pull_chip.rs builds on that, so change both
            // together. An ended tile has no timer slot, so its text stops as far from the
            // tile's edge as it starts.
            .pr(px(if ended { 12. } else { 6. }))
            .child(marker(t, false))
            .child(txt)
            .on_click(cx.listener(move |this, e: &ClickEvent, window, cx| {
                this.click_tile(id, e.click_count(), window, cx)
            }));
        // Resting the pointer on a session tile's text grows the tile into its hover card, which
        // shows what the tile cuts short (user, 2026-11-15). The timer slot sits beside the text,
        // so a pointer on its way to it opens no card.
        let live = i < LEAVING;
        // Right-clicking any tile's text opens its menu, which takes the tile off the rack
        // (record/areas/rack/02-tiles.md).
        if live {
            main = main.on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    this.open_tile_menu(id, e.position, cx)
                }),
            );
        }
        if live && t.session().is_some() {
            let spots = self.spots.clone();
            main = main
                .on_mouse_move(
                    cx.listener(move |this, _: &MouseMoveEvent, _, cx| this.pointer_moved(id, cx)),
                )
                .on_hover(cx.listener(move |this, hovered: &bool, _, _| {
                    if !*hovered {
                        this.pointer_left(id);
                    }
                }))
                .child(
                    canvas(
                        move |b, _, _| spots.borrow_mut().entry(id).or_default().main = b,
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                );
        }

        let hg = (!ended).then(|| self.timer_slot(i, t, now, cx));
        // The card grows from the tile's own rectangle, which only layout knows.
        let spot = (live && t.session().is_some()).then(|| {
            let spots = self.spots.clone();
            canvas(
                move |b, _, _| spots.borrow_mut().entry(id).or_default().tile = b,
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
        });

        // A session that keeps a task list has its section under the tile's 60px top, as tall
        // as it stands this frame. Hovering lights the top alone, since a click there opens the
        // session and the section's bar does something else. The tile clips its children to its
        // rectangle, not to its rounded corners, so the top's hover fill rounds its own corners
        // (read in gpui-pre 0.3.12, src/window.rs, `ContentMask`, which holds only bounds).
        let below = self.sections.height(id, Instant::now());
        let section = self.task_section(t, cx);
        // A two-finger swipe dismisses the tile only from its top, so swiping across an open task
        // list, or scrolling it, leaves the tile where it is.
        let top = div()
            .relative()
            .flex_none()
            .h(px(TILE_H))
            .flex()
            .hover(move |s| s.bg(rgb(hover_bg)))
            .on_scroll(cx.listener(move |this, e: &ScrollEvent, _, _| {
                if live {
                    this.swipe_touched(Swiped::Tile(id), e)
                }
            }))
            .child(main)
            .children(hg)
            .children(spot);
        let top = if below > 0. {
            top.rounded_t(px(10.))
        } else {
            top.rounded(px(10.))
        };

        // A swipe slides the tile with the fingers, fading it as it goes, and the row it sits in
        // cuts it off at the row's edges.
        let shift = self.swipe_offset(Swiped::Tile(id), !live, Instant::now());
        div()
            .id(("tile", id.0))
            .relative()
            .left(px(shift))
            .opacity(1. - 0.7 * (shift.abs() / crate::swipe::OUT).min(1.))
            .h(px(TILE_H + below))
            .flex()
            .flex_col()
            .rounded(px(10.))
            .overflow_hidden()
            .bg(rgb(bg))
            .child(top)
            .children(section)
            .into_any_element()
    }

    // The timer slot: the hourglass, which starts a 5-minute timer, and while one runs, fills
    // with yellow and cancels it, and a right-click for the menu of lengths.
    fn timer_slot(
        &self,
        i: usize,
        t: &Tile,
        now: SystemTime,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let id = t.id;
        let hg_group: SharedString = format!("hg-{i}").into();
        let held = self.hourglass_held == Some(id);
        let icon = match t.timer.left(now) {
            // A reminder for tomorrow shows a moon, since an hourglass that filled over the night
            // wouldn't visibly move, and hovering it dims the moon, as on a running timer, since
            // clicking cancels it (user, 2026-11-16).
            None if t.timer.is_tomorrow() => {
                let layer =
                    |path: SharedString| svg().absolute().top_0().left_0().size(px(20.)).path(path);
                let mut dim: Hsla = rgb(VIOLET).into();
                dim.a = 0.25;
                div()
                    .relative()
                    .size(px(20.))
                    .child(
                        layer("plates/moon.svg".into())
                            .text_color(rgb(VIOLET))
                            .group_hover(hg_group.clone(), move |s| s.text_color(dim)),
                    )
                    .into_any_element()
            }
            // A running timer fills the hourglass with yellow from the bottom as its time runs
            // out. The fill rises over the whole icon, since filling only its lower bulb, like
            // sand, is too small to read at 20px (user, 2026-11-16).
            // A timer that just started turns its hourglass over first, as you'd turn one over to
            // start it, and the yellow fill shows once it's upright (user, 2026-11-16).
            Some(_) if flipping(t, now).is_some() => {
                let turned = ease_in_out(flipping(t, now).unwrap_or(1.));
                div()
                    .relative()
                    .size(px(20.))
                    .child(
                        svg()
                            .absolute()
                            .top_0()
                            .left_0()
                            .size(px(20.))
                            .path("plates/hourglass.svg")
                            .with_transformation(Transformation::rotate(radians(
                                std::f32::consts::PI * turned,
                            )))
                            .text_color(text(0.5)),
                    )
                    .into_any_element()
            }
            Some(left) => {
                let glyph = || {
                    svg()
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .size(px(20.))
                        .path("plates/hourglass.svg")
                };
                let mut dim: Hsla = rgb(YELLOW).into();
                dim.a = 0.25;
                // The hourglass's lines run from 2.25px above the slot's bottom to 17.75px, so a
                // fill from 0 would show nothing for the timer's first minutes. It starts at 5px,
                // over the bottom bar and the tip of the lower bulb, so a timer that just started
                // shows at once, and reaches the top bar's edge as the time runs out
                // (user, 2026-11-16).
                let filled = 5. + (17.75 - 5.) * (1. - left).clamp(0., 1.);
                // Hovering a running timer dims it, since clicking cancels it, except right after a
                // click started it, while the pointer that clicked still rests there: the dimming
                // would answer the click that started the timer by taking it back. The hourglass
                // shows no × on hover, since an × tried there crowded the 20px icon
                // (user, 2026-11-16).
                let on_hover = |svg: Svg, hovered: Hsla| {
                    if held {
                        svg
                    } else {
                        svg.group_hover(hg_group.clone(), move |s| s.text_color(hovered))
                    }
                };
                div()
                    .relative()
                    .size(px(20.))
                    .child(on_hover(glyph().text_color(text(0.5)), text(0.15)))
                    .child(
                        div()
                            .absolute()
                            .bottom_0()
                            .left_0()
                            .w(px(20.))
                            .h(px(filled))
                            .overflow_hidden()
                            .child(on_hover(glyph().text_color(rgb(YELLOW)), dim)),
                    )
                    .into_any_element()
            }
            // A reminder that has come due leaves the hourglass full of yellow, where a running
            // timer's fill ends, so the tile's yellow reads as the timer having run out. A click
            // turns it over to start another 5 minutes rather than cancel, so hovering doesn't
            // dim it as it does a running timer (user, 2026-11-16).
            None if t.timer.is_time_up() => svg()
                .path("plates/hourglass.svg")
                .size(px(20.))
                .text_color(rgb(YELLOW))
                .into_any_element(),
            None => svg()
                .path("plates/hourglass.svg")
                .size(px(20.))
                .text_color(text(0.5))
                .group_hover(hg_group.clone(), |s| s.text_color(rgb(TEXT)))
                .into_any_element(),
        };
        let slot = div()
            .id(("hg", id.0))
            .group(hg_group)
            .flex_none()
            .w(px(44.))
            .flex()
            .items_center()
            .justify_center()
            .border_l_1()
            .border_color(white(0.06));
        // The tile clips its children to its rectangle, not to its rounded corners, as `tile` says,
        // so the slot's
        // hover fill rounds its own right corners to the tile's 10px, or only the top one over a
        // task section.
        let slot = if self.sections.height(id, Instant::now()) > 0. {
            slot.rounded_tr(px(10.))
        } else {
            slot.rounded_r(px(10.))
        };
        slot.hover(|s| s.bg(white(0.06)))
            .child(icon)
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if !*hovered && this.hourglass_held == Some(id) {
                    this.hourglass_held = None;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.click_timer(id, cx)))
            .on_mouse_pressure(
                cx.listener(move |this, e: &MousePressureEvent, _, cx| this.force_press(id, e, cx)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    this.open_menu(MenuKind::Timer(id), e.position, cx)
                }),
            )
    }

    // The inline field, for a new tile in place of the phantom tile, or for renaming your own
    // tile in place of it.
    fn field_row(&self, t: &Tile, cx: &mut Context<Self>) -> AnyElement {
        let marker = div()
            .flex_none()
            .size(px(9.))
            .rounded(px(2.))
            .bg(rgb(if t.timer.is_time_up() { YELLOW } else { GRAY }));
        self.field_shell(marker, cx)
    }

    // The field's hints after its text: for a new tile, the reminder it starts with, which Tab
    // steps through, then Enter. Both are keys, and clicking either does what its key does
    // (user, 2026-11-16).
    fn field_shell(&self, marker: Div, cx: &mut Context<Self>) -> AnyElement {
        let keycap = |id: &'static str| {
            div()
                .id(id)
                .flex_none()
                .flex()
                .items_center()
                .gap(px(4.))
                .px(px(6.))
                .py(px(2.))
                .rounded(px(5.))
                .bg(rgb(0x25272C))
                .hover(|s| s.bg(rgb(0x2F3137)))
                .text_size(px(10.5))
                .text_color(rgb(TEXT2))
        };
        let remind = self
            .field
            .as_ref()
            .filter(|f| f.target == FieldFor::New)
            .map(|f| {
                let minutes = crate::rack_view::field_reminder(f.remind);
                let words = match minutes {
                    Some(m) if m >= 60 => format!("{}h", m / 60),
                    Some(m) => format!("{m}m"),
                    None => "Tab".to_string(),
                };
                let (icon, words_color): (Hsla, Hsla) = if minutes.is_some() {
                    (rgb(YELLOW).into(), rgb(TEXT).into())
                } else {
                    (text(0.4), rgb(TEXT2).into())
                };
                keycap("field-remind")
                    .text_color(words_color)
                    .child(
                        svg()
                            .path("plates/hourglass.svg")
                            .size(px(12.))
                            .text_color(icon),
                    )
                    .child(words)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.step_reminder(false, cx);
                    }))
            });
        let enter = keycap("field-enter")
            .child("Enter")
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.click_enter(window, cx)));
        div()
            // A press here stays with the field, since a press anywhere else on the rack closes it.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .h(px(TILE_H))
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
                    Input::new(&self.input)
                        .appearance(false)
                        .px(px(0.))
                        .text_size(px(13.5))
                        .font_weight(FontWeight::SEMIBOLD),
                ),
            )
            .children(remind)
            .child(enter)
            .into_any_element()
    }

    fn last_row(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.is_adding() {
            let marker = div().flex_none().size(px(9.)).rounded(px(2.)).bg(rgb(GRAY));
            return self.field_shell(marker, cx);
        }
        let phantom = div()
            .id("phantom")
            .group("phantom")
            .h(px(TILE_H))
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
                    // The shortcut as each OS writes it, which native's watch_hotkey registers.
                    .child(if cfg!(target_os = "macos") {
                        "⌃⇧Space"
                    } else {
                        "Ctrl + Shift + Space"
                    }),
            )
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.start_field(crate::rack_view::FieldFor::New, window, cx)
            }));
        border_width(phantom, 1.5).into_any_element()
    }

    // The Undo row, which shows for 5s after you remove your own tile, and grows in and shrinks
    // out as a tile does.
    fn undo_row(&self, now: Instant, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (title, factor) = self.rows.undo(now)?;
        // The Undo row swipes away as a tile does, and shrinks out while it slides.
        let leaving = self.toast.is_none();
        let shift = self.swipe_offset(Swiped::Undo, leaving, now);
        let row = div()
            .relative()
            .left(px(shift))
            .opacity(1. - 0.7 * (shift.abs() / crate::swipe::OUT).min(1.))
            .on_scroll(
                cx.listener(|this, e: &ScrollEvent, _, _| this.swipe_touched(Swiped::Undo, e)),
            )
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
            .child(div().min_w_0().truncate().child(title.to_string()))
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
        // A shrinking tile is drawn above the tile it sat above, or after the last tile.
        let at = Instant::now();
        // Each frame's layout records where the tiles are again, so a tile that went has none.
        self.spots.borrow_mut().clear();
        let present = self.rack.tiles();
        let leaving = self.rows.leaving();
        let depths = nesting(present);
        // A sub-session shrinking out keeps its indent, one level below its parent's, while its
        // parent is still on the rack or shrinks out with it.
        fn app_id(t: &Tile) -> Option<&str> {
            t.session().and_then(|s| s.app_id.as_deref())
        }
        let mut leaving_depths: Vec<usize> = Vec::new();
        for l in leaving {
            let parent = l.tile.session().and_then(|s| s.app_parent.as_deref());
            let depth = parent
                .and_then(|p| {
                    present
                        .iter()
                        .zip(&depths)
                        .chain(leaving.iter().map(|l| &l.tile).zip(&leaving_depths))
                        .find(|(o, _)| app_id(o) == Some(p))
                })
                .map_or(0, |(_, d)| (d + 1).min(MAX_DEPTH));
            leaving_depths.push(depth);
        }
        // Each row in the order it's drawn, with its tile's id, whether it's shrinking out, its
        // depth, and how far it has grown.
        let mut rows: Vec<(AnyElement, plates_core::TileId, bool, usize, f32)> = Vec::new();
        for (i, t) in present.iter().enumerate() {
            for (k, l) in leaving.iter().enumerate() {
                if l.before == Some(t.id) {
                    let el = self.tile(LEAVING + k, &l.tile, now, cx);
                    rows.push((el, l.tile.id, true, leaving_depths[k], l.factor(at)));
                }
            }
            let el = self.tile(i, t, now, cx);
            rows.push((el, t.id, false, depths[i], self.rows.factor(t.id, at)));
        }
        for (k, l) in leaving.iter().enumerate() {
            if l.before.is_none_or(|b| present.iter().all(|t| t.id != b)) {
                let el = self.tile(LEAVING + k, &l.tile, now, cx);
                rows.push((el, l.tile.id, true, leaving_depths[k], l.factor(at)));
            }
        }
        let lines = runs_on(&rows.iter().map(|r| r.3).collect::<Vec<_>>());
        // The lines from above the swiped tile's own level stay put while it slides.
        let swiped = self.swiped_tile();
        let fixed = rows.iter().find(|r| Some(r.1) == swiped).map_or(0, |r| r.3);
        let tiles: Vec<AnyElement> = rows
            .into_iter()
            .zip(lines)
            .map(|((el, id, leaving, depth, factor), lines)| {
                let row_h = ROW_H + self.sections.height(id, at);
                let el = if depth > 0 {
                    let shift = self.branch_offset(id, leaving, at);
                    nested_row(el, &lines, shift, fixed, row_h)
                } else {
                    el
                };
                grown(el, row_h, factor)
            })
            .collect();
        let undo = self.undo_row(at, cx);
        let last = self.last_row(cx);
        // While any tile is snoozed, a violet moon with their count sits at the band's right,
        // above the tiles' timer slots, with an orange dot while a snoozed session needs you,
        // which leaves it snoozed. Clicking it lists the snoozed tiles (user, 2026-11-16).
        let snoozed = self.rack.snoozed().count();
        let moon = (snoozed > 0).then(|| {
            let needs = self.rack.snoozed_need_you();
            div()
                .id("snoozed")
                .h(px(crate::rack_view::MOON_H))
                .px(px(6.))
                .flex()
                .items_center()
                .gap(px(4.))
                .rounded(px(6.))
                .hover(|s| s.bg(rgb(BORDER)))
                .child(
                    svg()
                        .path("plates/moon.svg")
                        .size(px(12.))
                        .text_color(rgb(VIOLET)),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(VIOLET))
                        .child(snoozed.to_string()),
                )
                .children(needs.then(|| div().size(px(5.)).rounded_full().bg(rgb(HOT))))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _: &MouseDownEvent, _, _| {
                        this.menu_button_pressed(&MenuKind::Snoozed { count: snoozed })
                    }),
                )
                .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                    this.menu_button_clicked(MenuKind::Snoozed { count: snoozed }, e.position(), cx)
                }))
        });
        // While you've removed any tile, a box with their count sits left of the moon, in gray,
        // since nothing in it needs you. Clicking it lists them, to bring one back or clear them
        // all (user, 2026-11-16).
        let archived = self.rack.archived().len();
        let archive = (archived > 0).then(|| {
            div()
                .id("archived")
                .h(px(crate::rack_view::MOON_H))
                .px(px(6.))
                .flex()
                .items_center()
                .gap(px(4.))
                .rounded(px(6.))
                .hover(|s| s.bg(rgb(BORDER)))
                .child(
                    svg()
                        .path("plates/archive.svg")
                        .size(px(13.))
                        .text_color(rgb(MUTED)),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(MUTED))
                        .child(archived.to_string()),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _: &MouseDownEvent, _, _| {
                        this.menu_button_pressed(&MenuKind::Archived { count: archived })
                    }),
                )
                .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                    this.menu_button_clicked(
                        MenuKind::Archived { count: archived },
                        e.position(),
                        cx,
                    )
                }))
        });
        // A bell at the band's left turns the tone a timer rings as it runs out on and off, struck
        // through while it's off, and swings on its loop as it turns the tone on, as iOS' bell
        // does when you flip the ring switch to ring (user, 2026-11-17). It sits as far from the rack's side and top as
        // the badges at the right do.
        let bell = {
            let angle = bell_angle(self.bell_turned);
            // GPUI turns an icon about its center, so moving it by how far the turn carries the
            // center around the loop, half the icon's height above it, turns it about the loop.
            // (read in gpui-pre 0.3.12, src/elements/svg.rs, Transformation::into_matrix, and
            // src/scene.rs, TransformationMatrix::rotate)
            let half = BELL_SIZE / 2.;
            let swing = point(px(-half * angle.sin()), px(half * (angle.cos() - 1.)));
            let (icon, color) = if self.sound {
                ("plates/bell.svg", MUTED)
            } else {
                ("plates/bell-off.svg", GRAY)
            };
            div()
                .id("sound")
                .absolute()
                .left(px(crate::rack_view::MOON_INSET - 12.))
                .top(px(crate::rack_view::MOON_INSET))
                .h(px(crate::rack_view::MOON_H))
                .px(px(5.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .hover(|s| s.bg(rgb(BORDER)))
                .child(
                    svg()
                        .path(icon)
                        .size(px(BELL_SIZE))
                        .text_color(rgb(color))
                        .with_transformation(
                            Transformation::rotate(radians(angle)).with_translation(swing),
                        ),
                )
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_sound(cx)))
        };
        // The band sits inside the rack's 12px padding, so the badges reach out of it to sit
        // MOON_INSET from the rack's side, as they do from the top.
        let badges = div()
            .absolute()
            .right(px(crate::rack_view::MOON_INSET - 12.))
            .top(px(crate::rack_view::MOON_INSET))
            .flex()
            .gap(px(2.))
            .children(archive)
            .children(moon);
        // The grip is a button too, as tall as the moon's badge, and clicking it opens the
        // rack's menu, which a right-click on the band opens as well (user, 2026-11-16).
        // Pressing it and moving drags the rack, as anywhere else on it does.
        let connection = self.connection;
        let rack_menu = move || MenuKind::Rack {
            login: native::starts_at_login(),
            connection,
        };
        // While an update is ready, a teal dot on the grip's top-right corner says the menu has
        // news before you open it (record/areas/build/01-updates.md).
        let update_ready = self.updates.read(cx).is_ready();
        let grip = div()
            .id("grip")
            .relative()
            .h(px(crate::rack_view::MOON_H))
            .px(px(8.))
            .flex()
            .items_center()
            .rounded(px(6.))
            .hover(|s| s.bg(rgb(BORDER)))
            .child(
                svg()
                    .path("plates/grip.svg")
                    .w(px(16.))
                    .h(px(10.))
                    .text_color(rgb(GRAY)),
            )
            .children(update_ready.then(|| {
                div()
                    .absolute()
                    .top(px(3.))
                    .right(px(3.))
                    .size(px(6.))
                    .rounded_full()
                    .bg(rgb(TEAL))
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, _, _| {
                    this.menu_button_pressed(&rack_menu())
                }),
            )
            .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                this.menu_button_clicked(rack_menu(), e.position(), cx)
            }));
        let band = div()
            .relative()
            .h(px(crate::rack_view::BAND_H))
            .flex()
            .items_center()
            .justify_center()
            .child(bell)
            .child(badges)
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    this.open_menu(rack_menu(), e.position, cx)
                }),
            )
            .child(grip);
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
            // The rack shows the arrow, as a desktop app's controls do, and the closed hand only
            // while you drag it, as feedback that it moves (user, 2026-11-16).
            .cursor(if self.drag.is_some() {
                CursorStyle::ClosedHand
            } else {
                CursorStyle::Arrow
            })
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_press))
            .on_scroll(cx.listener(Self::on_scroll))
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
            .children(self.rack.tiles().iter().map(|t| marker(t, true)))
            .children(self.rack.snoozed().next().is_some().then(|| {
                svg()
                    .path("plates/moon.svg")
                    .size(px(9.))
                    .text_color(rgb(VIOLET))
            }));
        let filler = self.peek.then(|| div().flex_none().w(px(PEEK as f32)));
        let strip = div()
            .id("strip")
            .flex_none()
            .h(px(crate::rack_view::strip_height(self.strip_pips())))
            .flex()
            .bg(rgb(BG))
            .border_color(rgb(BORDER))
            .border_t_1()
            .border_b_1()
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
        // The peek follows the pointer over the whole window, the list included, so moving from
        // the strip onto the list keeps it open.
        let row = div()
            .id("strip-window")
            .size_full()
            .flex()
            .items_start()
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| this.set_peek(*hovered, cx)));
        match side {
            Side::Right => row.children(popover).children(gap).child(strip),
            Side::Left => row.child(strip).children(gap).children(popover),
        }
        .into_any_element()
    }

    /// Whether a tile is brightening, a new tile is sliding in, an hourglass is turning over, a
    /// swipe is settling, or the bell is swinging, each of which needs a frame each display
    /// refresh until it ends.
    pub(crate) fn tiles_animating(&self) -> bool {
        let now = SystemTime::now();
        if self.bell_turned.is_some_and(|at| at.elapsed() < SWING) {
            return true;
        }
        let swiped = self
            .swipe
            .as_ref()
            .is_some_and(|s| s.settling(Instant::now()));
        let chip_brightens = self
            .pulls
            .values()
            .any(|p| p.changed.is_some_and(|at| at.elapsed() < BRIGHTEN));
        swiped
            || chip_brightens
            || self.rack.tiles().iter().any(|t| {
                t.need
                    .is_some_and(|(_, at)| !self.hold.holds(at) && since(at, now) < BRIGHTEN)
                    || flipping(t, now).is_some()
                    || t.session().is_some_and(|s| {
                        s.state == SessionState::New && since(s.state_since, now) < ENTER + BRIGHTEN
                    })
            })
    }
}

impl Render for RackView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.scale = native::scale(window);
        // The rows learn which tiles left before the sections read them, so a tile shrinking out
        // keeps its task section's height.
        self.track_rows();
        self.track_sections();
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

#[cfg(test)]
mod tests {
    // Only these, since the glob of GPUI's names above holds a `test` macro of its own.
    use super::{nesting, runs_on};
    use plates_core::{Rack, SessionEvent, desktop::AppSession};
    use std::{path::Path, time::SystemTime};

    #[test]
    fn a_family_nests_two_levels_deep_and_each_levels_line_runs_on_to_the_next_tile_at_it() {
        let mut rack = Rack::new();
        let add = |rack: &mut Rack, id: &str, parent: Option<&str>| {
            let started = SessionEvent::Started {
                title: None,
                resumed: false,
            };
            rack.apply(id, Path::new("/w"), started, SystemTime::UNIX_EPOCH);
            let app = AppSession {
                id: Some(format!("local_{id}")),
                parent: parent.map(|p| format!("local_{p}")),
                ..AppSession::default()
            };
            rack.set_app_session(id, &app);
            rack.place_under_parent(id);
        };
        add(&mut rack, "stage", None);
        add(&mut rack, "idle", Some("stage"));
        add(&mut rack, "crash", Some("idle"));
        add(&mut rack, "deeper", Some("crash"));
        add(&mut rack, "second", Some("stage"));
        add(&mut rack, "other", None);

        // The great-grandchild sits at the last level, beside its parent.
        let depths = nesting(rack.tiles());
        assert_eq!(depths, [0, 1, 2, 2, 1, 0]);
        // Stage's line passes beside its grandchildren on its way to its second sub-session.
        assert_eq!(
            runs_on(&depths),
            [
                vec![],
                vec![true],
                vec![true, true],
                vec![true, false],
                vec![false],
                vec![]
            ]
        );
    }
}
