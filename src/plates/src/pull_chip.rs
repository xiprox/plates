//! The pull request chip at the end of a session tile's title, which says by its icon's color where
//! the session's pull requests stand, and the rows of the pop-out, `PullPopView`, that lists them
//! (record/areas/rack/04-pull-requests.md).

use std::time::Instant;

use gpui_kit::*;
use plates_core::{
    Tile, TileId,
    pulls::{self, OnTile, PullLink, PullState, Status},
};

use crate::{
    rack_view::{RackView, SeenPull},
    task_list::list_key,
    view::{BRIGHTEN, GREEN, MUTED, TEXT, TEXT2, mix, swell, white},
};

/// Failed checks and conflicts, redder than an approval's orange, so a broken pull request doesn't
/// read as a session asking you something.
const RED: u32 = 0xF2706B;
/// A merged pull request, in a purple like the one GitHub marks merged pull requests with.
const PURPLE: u32 = 0xA98BF5;

/// How much white the chip's fill takes at rest and on hover. The chip is gray, and only its icon
/// takes the pull request's color, since a chip on every tile in full color drew the eye from the
/// tiles' own states (user, 2026-11-27).
const FILL: f32 = 0.07;
const FILL_HOVER: f32 = 0.13;
/// How much of the icon's color the chip's brighten washes its fill with at its peak, which the
/// user tuned on a bench of every status (user, 2026-11-27).
const FILL_PEAK: f32 = 0.2;

/// The pop-out's width, in logical pixels.
pub(crate) const POP_W: f32 = 260.;

// An open pull request is green whether its checks passed or still run, and gray is for a draft,
// as on GitHub (user, 2026-11-27). The pop-out says which checks are running.
fn color(status: Status) -> u32 {
    match status {
        Status::Merged => PURPLE,
        Status::Failed | Status::Conflicts => RED,
        Status::Passed | Status::Running | Status::Open => GREEN,
        Status::Draft | Status::Closed => MUTED,
    }
}

fn tint(c: u32, alpha: f32) -> Hsla {
    Hsla::from(rgb(c)).alpha(alpha)
}

// What a pull request's row and the chip say until GitHub answers, or without `gh`: what the
// Claude desktop app's file says, open or merged.
fn status(link: &PullLink, seen: Option<&SeenPull>) -> Status {
    match seen {
        Some(seen) => seen.pull.status(),
        None if link.open => Status::Open,
        None => Status::Merged,
    }
}

impl RackView {
    /// The pull requests tile `t` shows, from the Claude desktop app's links and the ones Claude
    /// made with `pr_link`, which the session's task file keeps. Once GitHub has answered about
    /// one, its answer says whether it's open, rather than the app's file or the link.
    pub(crate) fn tile_pulls(&self, t: &Tile) -> Option<OnTile> {
        let s = t.session()?;
        let linked = list_key(t)
            .and_then(|key| self.task_lists.get(key))
            .map_or(&[][..], |list| list.pulls.as_slice());
        let mut all = pulls::combined(&s.pulls, linked);
        for link in &mut all {
            if let Some(seen) = self.pulls.get(&link.url) {
                link.open = seen.pull.state == PullState::Open;
            }
        }
        pulls::on_tile(&all)
    }

    /// The chip for tile `id`'s pull requests: the number of the one it names, "+1" for each
    /// other open one, and the status that most wants your eye among the open ones.
    pub(crate) fn pull_chip(&self, id: TileId, on: &OnTile, cx: &mut Context<Self>) -> AnyElement {
        let watched: Vec<&PullLink> = if on.open.is_empty() {
            vec![&on.named]
        } else {
            on.open.iter().collect()
        };
        let status = pulls::most_pressing(
            watched
                .iter()
                .map(|link| status(link, self.pulls.get(&link.url))),
        )
        .unwrap_or(Status::Open);
        let c = color(status);
        // The chip brightens when where any of its pull requests stands changes.
        let changed = watched
            .iter()
            .filter_map(|link| self.pulls.get(&link.url)?.changed)
            .max();
        let swelled =
            changed.and_then(|at| swell(Instant::now().saturating_duration_since(at), BRIGHTEN));
        // The number takes the icon's color as the chip brightens, since the gray number on the
        // colored wash read as washed out (user, 2026-11-27).
        let number = mix(TEXT2, c, swelled.unwrap_or(0.));
        let wash = swelled.map(|k| {
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .rounded(px(5.))
                .bg(tint(c, FILL_PEAK * k))
        });
        let icon = if status == Status::Merged {
            "plates/merged.svg"
        } else {
            "plates/pull.svg"
        };
        let label = match on.more() {
            0 => format!("#{}", on.named.number),
            n => format!("#{} +{n}", on.named.number),
        };
        let spots = self.spots.clone();
        div()
            .id(("pull", id.0))
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(3.))
            .h(px(17.))
            .px(px(5.))
            // The tile's text stops 6px short of the timer slot, its `pr` in view.rs, and 4px
            // more keeps a chip at the end of a long title 10px from the slot. Matching its 13px
            // from the tile's top looked too far in (user, 2026-11-27). When you change the `pr`,
            // change this too.
            .mr(px(4.))
            .rounded(px(5.))
            .bg(white(FILL))
            .hover(|s| s.bg(white(FILL_HOVER)))
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(number))
            .children(wash)
            .child(svg().path(icon).size(px(11.)).text_color(rgb(c)))
            .child(label)
            // The chip sits on the tile's text, whose click opens the session, so its own click
            // stops there.
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.click_pull(id, cx);
            }))
            .child(
                canvas(
                    move |b, _, _| spots.borrow_mut().entry(id).or_default().chip = b,
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .into_any_element()
    }
}

/// The pull requests the pop-out lists, newest first: every open one, or once none is open, the
/// one the chip names.
pub(crate) fn listed(on: &OnTile) -> Vec<PullLink> {
    if on.open.is_empty() {
        return vec![on.named.clone()];
    }
    on.open.iter().rev().cloned().collect()
}

/// One pull request's row in the pop-out: its number and repository, its title once GitHub has
/// said it, and where it stands in words, in its icon's color.
pub(crate) fn pop_row(link: &PullLink, seen: Option<&SeenPull>) -> Div {
    let c = color(status(link, seen));
    let words = match seen {
        Some(seen) => seen.pull.words(),
        None if link.open => "Open".into(),
        None => "Merged".into(),
    };
    div()
        .flex()
        .flex_col()
        .gap(px(3.))
        .px(px(12.))
        .py(px(9.))
        .child(
            div()
                .text_size(px(11.))
                .text_color(rgb(MUTED))
                .truncate()
                .child(format!("#{} · {}", link.number, link.repo)),
        )
        .children(seen.map(|s| {
            div()
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT))
                .line_clamp(3)
                .child(s.pull.title.clone())
        }))
        .child(div().text_size(px(12.)).text_color(rgb(c)).child(words))
}
