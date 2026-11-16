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
const BLUE: u32 = 0x74AEFF;
const AMBER: u32 = 0xF2B441;
const HOT: u32 = 0xFF7A45;
pub(crate) const YELLOW: u32 = 0xF4D35E;
/// A reminder for tomorrow, apart from yellow's timer running now. It's about as bright as the
/// yellow, so the moon reads at 20px, and redder than the working blue, so the two don't blur
/// (user, 2026-11-16).
pub(crate) const VIOLET: u32 = 0xB39DFF;
const GREEN: u32 = 0x5BCB8A;
const GRAY: u32 = 0x5E626B;
/// The text colors of a working tile, a your-turn tile, and an approval.
pub(crate) const WORKING_TEXT: u32 = 0x9CC4FF;
pub(crate) const YOUR_TURN_TEXT: u32 = 0xF2C46A;
pub(crate) const APPROVAL_TEXT: u32 = 0xFF9B6E;

/// Tiles shrinking out are drawn with indexes from here up, so their group and element names
/// never match a present tile's.
const LEAVING: usize = 10_000;

/// How long a tile's background brightens when it starts needing you.
const BRIGHTEN: Duration = Duration::from_millis(1400);
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

/// How far a sub-session's tile sits in from its parent's, while its right edge stays flush with
/// the other tiles' (user, 2026-11-15).
const NEST: f32 = 26.;

// Which of the rack's tiles to draw as sub-sessions under their parent: a session tile whose
// parent is the tile above it, or is a sub-session drawn under that tile, so a family reads as one
// group, one level deep. A sub-session whose parent's tile is gone, or sits elsewhere, keeps its
// place without an indent.
fn nesting(tiles: &[Tile]) -> Vec<bool> {
    let mut family: Vec<&str> = Vec::new();
    tiles
        .iter()
        .map(|t| {
            let Some(s) = t.session() else {
                family.clear();
                return false;
            };
            let nested = s.app_parent.as_deref().is_some_and(|p| family.contains(&p));
            if !nested {
                family.clear();
            }
            family.extend(s.app_id.as_deref());
            nested
        })
        .collect()
}

// A sub-session's tile, indented, with a line that comes down from under its parent's marker and
// turns into the tile, as the Claude desktop app's sidebar draws a session's side sessions.
fn nested_row(tile: AnyElement) -> AnyElement {
    // The parent's marker is centered 16.5px in from its tile's left edge: 12px of padding and
    // half of the 9px marker.
    let branch = div()
        .absolute()
        .left(px(16.))
        .top_0()
        .w(px(NEST - 19.))
        .h(px(30.))
        .border_l_1()
        .border_b_1()
        .rounded_bl(px(5.))
        .border_color(white(0.16));
    div()
        .flex()
        .child(
            div()
                .flex_none()
                .relative()
                .w(px(NEST))
                .h(px(60.))
                .child(branch),
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
// timer, so your own tile's pip is outlined in yellow while its timer runs, or violet while it's
// set for tomorrow, and turns solid yellow once it's time-up, which needs you where a running
// timer doesn't (user, 2026-11-15 and 2026-11-16).
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
            .relative()
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
        // Resting the pointer on a session tile's text grows the tile into its hover card, which
        // shows what the tile cuts short (user, 2026-11-15). The × and the timer slot sit beside
        // the text, so a pointer on its way to them opens no card.
        let live = i < LEAVING;
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
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                        this.open_tile_menu(id, e.position, cx)
                    }),
                )
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

        // Every tile shows an × on hover, 8px from the timer slot or from the tile's edge, which
        // removes your own tile or an ended session's, and dismisses any other session's until
        // it needs you again (user, 2026-11-15).
        let side = {
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
        };

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

        // A swipe slides the tile with the fingers, fading it as it goes, and the row it sits in
        // cuts it off at the row's edges.
        let shift = self.swipe_offset(Swiped::Tile(id), !live, Instant::now());
        div()
            .id(("tile", id.0))
            .group(group)
            .relative()
            .left(px(shift))
            .opacity(1. - 0.7 * (shift.abs() / crate::swipe::OUT).min(1.))
            .h(px(60.))
            .flex()
            .rounded(px(10.))
            .overflow_hidden()
            .bg(rgb(bg))
            .hover(move |s| s.bg(rgb(hover_bg)))
            .on_scroll(cx.listener(move |this, e: &ScrollEvent, _, _| {
                if live {
                    this.swipe_touched(Swiped::Tile(id), e)
                }
            }))
            .child(main)
            .child(side)
            .children(hg)
            .children(spot)
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
            // The tile clips its children to its rectangle, not to its rounded corners, so the
            // slot's hover fill rounds its own right corners to the tile's 10px.
            .rounded_r(px(10.))
            .hover(|s| s.bg(white(0.06)))
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
        let nested = nesting(present);
        // A sub-session shrinking out keeps its indent while its parent is still on the rack.
        let parent_shown = |t: &Tile| {
            t.session()
                .and_then(|s| s.app_parent.as_deref())
                .is_some_and(|p| {
                    present
                        .iter()
                        .any(|o| o.session().and_then(|s| s.app_id.as_deref()) == Some(p))
                })
        };
        let row = |el: AnyElement, nest: bool| if nest { nested_row(el) } else { el };
        let mut tiles: Vec<AnyElement> = Vec::new();
        for (i, t) in present.iter().enumerate() {
            for (k, l) in leaving.iter().enumerate() {
                if l.before == Some(t.id) {
                    let el = self.tile(LEAVING + k, &l.tile, now, cx);
                    tiles.push(grown(row(el, parent_shown(&l.tile)), 66., l.factor(at)));
                }
            }
            let el = self.tile(i, t, now, cx);
            tiles.push(grown(row(el, nested[i]), 66., self.rows.factor(t.id, at)));
        }
        for (k, l) in leaving.iter().enumerate() {
            if l.before.is_none_or(|b| present.iter().all(|t| t.id != b)) {
                let el = self.tile(LEAVING + k, &l.tile, now, cx);
                tiles.push(grown(row(el, parent_shown(&l.tile)), 66., l.factor(at)));
            }
        }
        let undo = self.undo_row(at, cx);
        let last = self.last_row(cx);
        // While any tile is snoozed, a violet moon with their count sits at the band's right,
        // above the tiles' timer slots, with an orange dot while a snoozed session needs you,
        // which leaves it snoozed. Clicking it lists the snoozed tiles (user, 2026-11-16).
        let snoozed = self.rack.snoozed().count();
        let moon = (snoozed > 0).then(|| {
            let needs = self.rack.snoozed_need_you();
            // The band sits inside the rack's 12px padding, so the badge reaches out of it to sit
            // MOON_INSET from the rack's side, as it does from the top.
            div()
                .id("snoozed")
                .absolute()
                .right(px(crate::rack_view::MOON_INSET - 12.))
                .top(px(crate::rack_view::MOON_INSET))
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
        // The grip is a button too, as tall as the moon's badge, and clicking it opens the
        // rack's menu, which a right-click on the band opens as well (user, 2026-11-16).
        // Pressing it and moving drags the rack, as anywhere else on it does.
        let rack_menu = || MenuKind::Rack {
            login: native::starts_at_login(),
        };
        let grip = div()
            .id("grip")
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
            .children(moon)
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

    /// Whether a tile is brightening, a new tile is sliding in, an hourglass is turning over, or
    /// a swipe is settling, each of which needs a frame each display refresh until it ends.
    pub(crate) fn tiles_animating(&self) -> bool {
        let now = SystemTime::now();
        let swiped = self
            .swipe
            .as_ref()
            .is_some_and(|s| s.settling(Instant::now()));
        swiped
            || self.rack.tiles().iter().any(|t| {
                t.need.is_some_and(|(_, at)| since(at, now) < BRIGHTEN)
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
