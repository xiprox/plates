//! The pull request chip at the end of a session tile's title, which says by its icon's color where
//! the pull request stands, opens it in your browser on a click, and shows its number, title, and
//! state in a pop-out, `PullPopView`, when the pointer rests on it (user, 2026-11-27;
//! record/areas/rack/04-pull-requests.md).

use gpui_kit::*;
use plates_core::{
    Tile, TileId,
    pulls::{self, PullLink, Status},
};

use crate::{
    rack_view::{RackView, SeenPull},
    task_list::list_key,
    view::{BRIGHTEN, GREEN, MUTED, TEXT, TEXT2, swell, white},
};

/// Failed checks and conflicts, redder than an approval's orange, so a broken pull request doesn't
/// read as a session asking you something.
const RED: u32 = 0xF2706B;
/// A merged pull request, in a purple like the one GitHub marks merged pull requests with.
const PURPLE: u32 = 0xA98BF5;

/// The chip is gray, and only its icon takes the pull request's color, since a chip on every tile
/// in full color drew the eye from the tiles' own states (user, 2026-11-27). Its brighten washes
/// the gray with the icon's color, up to `FILL_PEAK` of it.
const FILL: f32 = 0.07;
const FILL_HOVER: f32 = 0.13;
const FILL_PEAK: f32 = 0.45;

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

// What the chip says until GitHub answers, or without `gh`: what the Claude desktop app's file
// says, open or merged.
fn status(link: &PullLink, seen: Option<&SeenPull>) -> Status {
    match seen {
        Some(seen) => seen.pull.status(),
        None if link.open => Status::Open,
        None => Status::Merged,
    }
}

impl RackView {
    /// The pull request tile `t` shows, if any, and how many more open ones its session has:
    /// `pulls::shown`'s pick from the Claude desktop app's links and the ones Claude made with
    /// `pr_link`, which the session's task file keeps.
    pub(crate) fn shown_pull(&self, t: &Tile) -> Option<(PullLink, usize)> {
        let s = t.session()?;
        let linked = list_key(t)
            .and_then(|key| self.task_lists.get(key))
            .map_or(&[][..], |list| list.pulls.as_slice());
        let all = pulls::combined(&s.pulls, linked);
        pulls::shown(&all).map(|(link, more)| (link.clone(), more))
    }

    /// The chip for the pull request `shown_pull` picked for tile `id`, with `more` other open
    /// ones.
    pub(crate) fn pull_chip(
        &self,
        id: TileId,
        link: &PullLink,
        more: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let seen = self.pulls.get(&link.url);
        let status = status(link, seen);
        let c = color(status);
        let wash = seen
            .and_then(|s| s.changed)
            .and_then(|at| swell(at.elapsed(), BRIGHTEN))
            .map(|k| {
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
        let label = match more {
            0 => format!("#{}", link.number),
            n => format!("#{} +{n}", link.number),
        };
        let url = link.url.clone();
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
            .text_color(rgb(TEXT2))
            .children(wash)
            .child(svg().path(icon).size(px(11.)).text_color(rgb(c)))
            .child(label)
            // The chip sits on the tile's text, whose click opens the session, so its own click
            // stops there.
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.click_pull(id, url.clone(), cx);
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

/// The pop-out's text: the number and repository, the title once GitHub has said it, and the
/// state in words.
pub(crate) fn pop_content(link: &PullLink, more: usize, seen: Option<&SeenPull>) -> Div {
    let c = color(status(link, seen));
    let words = match seen {
        Some(seen) => seen.pull.words(),
        None if link.open => "Open".into(),
        None => "Merged".into(),
    };
    let more = match more {
        0 => None,
        1 => Some("1 more open pull request".to_string()),
        n => Some(format!("{n} more open pull requests")),
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
        .children(more.map(|m| div().text_size(px(11.)).text_color(rgb(MUTED)).child(m)))
}
