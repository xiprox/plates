//! A session's task list inside its tile (record/areas/tasks/01-task-list.md). Under the tile's
//! 60px top, a divider and a bar count the open tasks by who holds each one, and a click on the
//! bar opens the list under it. The bar stays where it was, so a second click in the same place
//! closes the list again, as a peek wants (user, 2026-11-20). The rack only grows taller, as when
//! a tile grows in, so nothing moves inside its window but the tiles below.
//!
//! An open list's height comes from its tasks: every row is a fixed height, and titles and
//! statuses run one line each, cut off at the tile's edge. The rack sizes its window from the
//! same numbers its rows draw at, so the two can't disagree.

use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use gpui_kit::*;
use plates_core::{
    Tile, TileId, store,
    tasks::{Holder, Holders, Task, TaskList},
};

use crate::{
    native,
    rack_view::{RACK_W, RackView},
    rows::{ENTER, ease_out},
    view::{AMBER, BLUE, GRAY, MUTED, TEXT, TEXT2, YOUR_TURN_TEXT, pause_mark, white},
};

const DIVIDER: f32 = 1.;
const BAR_H: f32 = 28.;
/// A tile's height under its top while its list is closed: the divider and the bar.
pub const CLOSED_H: f32 = DIVIDER + BAR_H;
const HEADING_H: f32 = 22.;
const TITLE_H: f32 = 16.;
const STATUS_H: f32 = 15.;
/// Above and below each task.
const TASK_PAD: f32 = 3.;
const FOLD_H: f32 = 24.;
/// Above and below the list.
const LIST_PAD: f32 = 4.;
/// The tallest an open list grows before it scrolls, about three and a half tiles, so an open
/// list leaves room on the screen for the rest of the rack.
const LIST_MAX_H: f32 = 232.;
/// How long a section takes to grow or shrink: as long as a tile takes to grow in.
const GROW: Duration = ENTER;

/// The tile whose list is open, and whether its done and dropped tasks are unfolded.
pub(crate) struct Open {
    pub(crate) tile: TileId,
    pub(crate) finished: bool,
    /// Where the list has scrolled to, which unfolding the done tasks moves.
    scroll: ScrollHandle,
}

impl Open {
    pub(crate) fn new(tile: TileId) -> Self {
        Open {
            tile,
            finished: false,
            scroll: ScrollHandle::new(),
        }
    }
}

/// How tall each tile's task section stands, easing to a new height over `GROW` whenever its list
/// appears, opens, closes, or goes.
#[derive(Default)]
pub(crate) struct Sections {
    heights: HashMap<TileId, Ease>,
}

struct Ease {
    from: f32,
    to: f32,
    start: Instant,
}

impl Ease {
    fn at(&self, now: Instant) -> f32 {
        let t =
            (now.saturating_duration_since(self.start).as_secs_f32() / GROW.as_secs_f32()).min(1.);
        self.from + (self.to - self.from) * ease_out(t)
    }
}

impl Sections {
    /// Sets each tile's height to come. A tile seen for the first time takes its height at once,
    /// since it grows in as a whole, and a tile left out is forgotten.
    pub(crate) fn track(&mut self, targets: Vec<(TileId, f32)>, now: Instant) {
        let mut seen = HashSet::new();
        for (id, to) in targets {
            seen.insert(id);
            match self.heights.get_mut(&id) {
                None => {
                    self.heights.insert(
                        id,
                        Ease {
                            from: to,
                            to,
                            start: now,
                        },
                    );
                }
                Some(e) if (e.to - to).abs() > 0.01 => {
                    *e = Ease {
                        from: e.at(now),
                        to,
                        start: now,
                    };
                }
                Some(_) => {}
            }
        }
        self.heights.retain(|id, _| seen.contains(id));
    }

    pub(crate) fn height(&self, id: TileId, now: Instant) -> f32 {
        self.heights.get(&id).map_or(0., |e| e.at(now))
    }

    pub(crate) fn animating(&self, now: Instant) -> bool {
        self.heights.values().any(|e| now < e.start + GROW)
    }
}

/// The key of a session tile's list in the tasks folder (`Session::list_key`).
pub(crate) fn list_key(t: &Tile) -> Option<&str> {
    Some(t.session()?.list_key())
}

const fn heading(holder: Holder) -> &'static str {
    match holder {
        Holder::You => "On you",
        Holder::Doing => "Doing",
        Holder::Others => "Waiting on others",
        Holder::Todo => "To do",
        Holder::Done => "Done",
        Holder::Dropped => "Dropped",
    }
}

const fn counted(holder: Holder) -> &'static str {
    match holder {
        Holder::You => "on you",
        Holder::Doing => "doing",
        Holder::Others => "waiting",
        Holder::Todo => "to do",
        Holder::Done => "done",
        Holder::Dropped => "dropped",
    }
}

// The line under a task's title: its status, and for a task waiting on someone other than you,
// who, such as "Asked on Tuesday · Dana".
fn status_line(task: &Task) -> String {
    let mut line = task.status.clone();
    if task.holder() == Holder::Others && !task.waiting_on.is_empty() {
        if !line.is_empty() {
            line.push_str(" · ");
        }
        line.push_str(&task.waiting_on);
    }
    line
}

fn row_height(task: &Task) -> f32 {
    let status = if status_line(task).is_empty() {
        0.
    } else {
        STATUS_H
    };
    2. * TASK_PAD + TITLE_H + status
}

fn of(list: &TaskList, holder: Holder) -> impl Iterator<Item = &Task> {
    list.tasks.iter().filter(move |t| t.holder() == holder)
}

/// How far below the list's top the fold of done and dropped tasks sits.
fn fold_top(list: &TaskList) -> f32 {
    let mut y = LIST_PAD;
    for holder in Holder::OPEN {
        let rows: f32 = of(list, holder).map(row_height).sum();
        if rows > 0. {
            y += HEADING_H + rows;
        }
    }
    y
}

/// The height an open list's rows take, before `LIST_MAX_H` caps it.
fn list_height(list: &TaskList, finished: bool) -> f32 {
    let mut h = fold_top(list) + LIST_PAD;
    let ended = || of(list, Holder::Done).chain(of(list, Holder::Dropped));
    if ended().next().is_some() {
        h += FOLD_H;
        if finished {
            h += ended().map(row_height).sum::<f32>();
        }
    }
    h
}

// Each holder's mark, in the rack's colors where the meaning matches: amber for what waits on
// you, as on a tile at your turn, and blue for work under way, as on a working tile.
fn mark(holder: Holder) -> AnyElement {
    let dot = |color: u32| div().size(px(7.)).rounded_full().bg(rgb(color));
    match holder {
        Holder::You => dot(AMBER).into_any_element(),
        Holder::Doing => dot(BLUE).into_any_element(),
        Holder::Others => pause_mark(7., 8., GRAY).into_any_element(),
        Holder::Todo => div()
            .size(px(7.))
            .rounded_full()
            .border(px(1.5))
            .border_color(rgb(GRAY))
            .into_any_element(),
        Holder::Done => svg()
            .path("plates/check.svg")
            .size(px(10.))
            .text_color(rgb(GRAY))
            .into_any_element(),
        Holder::Dropped => div()
            .w(px(7.))
            .h(px(1.5))
            .rounded(px(1.))
            .bg(rgb(GRAY))
            .into_any_element(),
    }
}

// A task's row: its mark, its title, and under the title its status line when it has one. A task
// that changed since you last opened the session says "new" after its title.
fn task_row(task: &Task, seen: Option<u64>) -> Div {
    let holder = task.holder();
    let color = match holder {
        Holder::Done => TEXT2,
        Holder::Dropped => MUTED,
        _ => TEXT,
    };
    let title = div()
        .min_w_0()
        .truncate()
        .text_size(px(12.5))
        .line_height(px(TITLE_H))
        .text_color(rgb(color))
        .child(task.title.clone());
    let title = if holder == Holder::Dropped {
        title.line_through()
    } else {
        title
    };
    let new = seen.is_some_and(|seen| task.updated > seen);
    let status = status_line(task);
    div()
        .flex_none()
        .h(px(row_height(task)))
        .px(px(12.))
        .py(px(TASK_PAD))
        .flex()
        .gap(px(8.))
        .child(
            div()
                .flex_none()
                .w(px(10.))
                .h(px(TITLE_H))
                .flex()
                .items_center()
                .justify_center()
                .child(mark(holder)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .h(px(TITLE_H))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(title)
                        .children(new.then(|| {
                            div()
                                .flex_none()
                                .text_size(px(10.5))
                                .text_color(rgb(BLUE))
                                .child("new")
                        })),
                )
                .children((!status.is_empty()).then(|| {
                    div()
                        .h(px(STATUS_H))
                        .truncate()
                        .text_size(px(11.5))
                        .line_height(px(STATUS_H))
                        .text_color(rgb(TEXT2))
                        .child(status)
                })),
        )
}

// The counts the bar shows, one per holder with open tasks, in the order the list shows them, or
// once nothing is open, how many are done and how many dropped.
fn counts(h: Holders) -> Vec<Div> {
    let item = |holder: Holder| {
        let color = if holder == Holder::You {
            YOUR_TURN_TEXT
        } else {
            MUTED
        };
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(4.))
            .text_color(rgb(color))
            .child(mark(holder))
            .child(format!("{} {}", h.of(holder), counted(holder)))
    };
    let open: Vec<Div> = Holder::OPEN
        .into_iter()
        .filter(|&holder| h.of(holder) > 0)
        .map(item)
        .collect();
    if !open.is_empty() {
        return open;
    }
    [Holder::Done, Holder::Dropped]
        .into_iter()
        .filter(|&holder| h.of(holder) > 0)
        .map(item)
        .collect()
}

impl RackView {
    /// The task list a tile shows, or none for a tile whose session keeps none, or whose list
    /// has no tasks.
    pub(crate) fn list_of(&self, t: &Tile) -> Option<&TaskList> {
        let list = self.task_lists.get(list_key(t)?)?;
        (!list.tasks.is_empty()).then_some(list)
    }

    fn section_target(&self, t: &Tile) -> f32 {
        let Some(list) = self.list_of(t) else {
            return 0.;
        };
        match self.open_list.as_ref().filter(|o| o.tile == t.id) {
            Some(o) => DIVIDER + list_height(list, o.finished).min(LIST_MAX_H) + DIVIDER + BAR_H,
            None => CLOSED_H,
        }
    }

    /// Closes the open list once its tile or its tasks are gone, or the rack docks, and sets each
    /// tile's section to the height it eases to. A tile shrinking out keeps the height it has.
    pub(crate) fn track_sections(&mut self) {
        let now = Instant::now();
        let gone = self.open_list.as_ref().is_some_and(|o| {
            self.docked.is_some()
                || self
                    .rack
                    .tile(o.tile)
                    .and_then(|t| self.list_of(t))
                    .is_none()
        });
        if gone {
            self.open_list = None;
        }
        let closed = self.closing_list.as_ref().is_none_or(|o| {
            self.rack.tile(o.tile).is_none() || self.sections.height(o.tile, now) <= CLOSED_H + 0.5
        });
        if closed {
            self.closing_list = None;
        }
        let present = self
            .rack
            .tiles()
            .iter()
            .map(|t| (t.id, self.section_target(t)));
        let leaving = self
            .rows
            .leaving()
            .iter()
            .map(|l| (l.tile.id, self.sections.height(l.tile.id, now)));
        let targets = present.chain(leaving).collect();
        self.sections.track(targets, now);
    }

    /// The height the tiles' sections add to the rack this frame, each as far as its row has
    /// grown in or shrunk out.
    pub(crate) fn sections_height(&self, now: Instant) -> f32 {
        let present: f32 = self
            .rack
            .tiles()
            .iter()
            .map(|t| self.rows.factor(t.id, now) * self.sections.height(t.id, now))
            .sum();
        let leaving: f32 = self
            .rows
            .leaving()
            .iter()
            .map(|l| l.factor(now) * self.sections.height(l.tile.id, now))
            .sum();
        present + leaving
    }

    /// Opens `id`'s list, closing any other, or closes it when it's open.
    pub(crate) fn toggle_list(&mut self, id: TileId, cx: &mut Context<Self>) {
        if self.suppress_click {
            return;
        }
        let closing = self.open_list.take();
        if closing.as_ref().is_none_or(|o| o.tile != id) {
            self.open_list = Some(Open::new(id));
        }
        if closing.is_some() {
            self.closing_list = closing;
        }
        cx.notify();
    }

    /// Unfolds the open list's done and dropped tasks, or folds them again. Unfolding scrolls the
    /// list until the fold sits at its top, or as far as the list scrolls, so the tasks it
    /// unfolds come into view under it rather than below the list's edge.
    fn toggle_finished(&mut self, cx: &mut Context<Self>) {
        if self.suppress_click {
            return;
        }
        let Some((tile, finished)) = self.open_list.as_ref().map(|o| (o.tile, o.finished)) else {
            return;
        };
        let scroll_to = (!finished)
            .then(|| self.rack.tile(tile).and_then(|t| self.list_of(t)))
            .flatten()
            .map(|list| fold_top(list).min((list_height(list, true) - LIST_MAX_H).max(0.)));
        if let Some(o) = &mut self.open_list {
            o.finished = !finished;
            if let Some(y) = scroll_to {
                o.scroll.set_offset(point(px(0.), px(-y)));
            }
            cx.notify();
        }
    }

    /// How far, in physical pixels, an open list lifts the window off the bottom of the screen,
    /// so the list opens on screen: by as much as the rack would reach below the screen's usable
    /// area, up to the height the list opened by, and never above the area's top. The rack's
    /// place stays where it was, so the window comes back down as the list closes.
    pub(crate) fn list_lift(&self) -> f64 {
        if self.docked.is_some() {
            return 0.;
        }
        let now = Instant::now();
        let opened_by = |id: TileId| (self.sections.height(id, now) - CLOSED_H).max(0.);
        let opened: f32 = self
            .rack
            .tiles()
            .iter()
            .map(|t| opened_by(t.id))
            .chain(
                self.rows
                    .leaving()
                    .iter()
                    .map(|l| l.factor(now) * opened_by(l.tile.id)),
            )
            .sum();
        if opened <= 0. {
            return 0.;
        }
        let s = self.scale;
        let h = self.shown_height() as f64 * s;
        let area = native::edges_at((self.pos.0 + RACK_W as f64 * s / 2., self.pos.1)).area;
        let below = self.pos.1 + h - area.bottom;
        below
            .clamp(0., opened as f64 * s)
            .min((self.pos.1 - area.top).max(0.))
    }

    /// A session tile's task section, as tall as it stands this frame: the divider and the bar,
    /// and under them, while the section stands taller than a closed bar, the list.
    pub(crate) fn task_section(&self, t: &Tile, cx: &mut Context<Self>) -> Option<Div> {
        let id = t.id;
        let height = self.sections.height(id, Instant::now());
        if height <= 0. {
            return None;
        }
        let divider = || div().flex_none().h(px(DIVIDER)).bg(white(0.06));
        let list = self.list_of(t);
        let open = self.open_list.as_ref().filter(|o| o.tile == id);
        // A closing list keeps its scroll and its unfolded tasks while its section shrinks.
        let shown = open.or(self.closing_list.as_ref().filter(|o| o.tile == id));
        let finished = shown.is_some_and(|o| o.finished);

        let chevron = if open.is_some() {
            "plates/chevron-up.svg"
        } else {
            "plates/chevron-down.svg"
        };
        let shows_list = height > CLOSED_H + 0.5;
        let bar = div()
            .id(("tasks-bar", id.0))
            .flex_none()
            .h(px(BAR_H))
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(6.))
            .overflow_hidden()
            .text_size(px(11.5))
            .text_color(rgb(MUTED))
            .hover(|s| s.bg(white(0.035)));
        // The tile clips its children to its rectangle, not to its rounded corners, as view.rs's
        // `tile` says, so the bar's hover fill rounds its own bottom corners to the tile's 10px
        // while it's the tile's bottom edge.
        let bar = if shows_list {
            bar
        } else {
            bar.rounded_b(px(10.))
        };
        // The counts stay while the list is open, so a click changes nothing on the bar but its
        // chevron (user, 2026-11-20). They sit in a box that cuts off at its edge, so with all
        // four holders and long counts, the last count gives way and the chevron stays.
        let counts = div()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .flex()
            .items_center()
            .gap(px(7.))
            .children(
                list.and_then(TaskList::holders)
                    .map(counts)
                    .unwrap_or_default(),
            );
        let bar = bar
            .child(counts)
            .child(
                svg()
                    .flex_none()
                    .path(chevron)
                    .size(px(12.))
                    .text_color(rgb(MUTED)),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_list(id, cx)));

        let rows = list.filter(|_| shows_list).map(|list| {
            let seen = t.session().and_then(|s| s.seen_at).map(store::ms);
            let mut content = div().flex().flex_col().py(px(LIST_PAD));
            for holder in Holder::OPEN {
                let tasks: Vec<&Task> = of(list, holder).collect();
                if tasks.is_empty() {
                    continue;
                }
                content = content
                    .child(
                        div()
                            .flex_none()
                            .h(px(HEADING_H))
                            .px(px(12.))
                            .pt(px(7.))
                            .text_size(px(11.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(MUTED))
                            .child(heading(holder)),
                    )
                    .children(tasks.into_iter().map(|task| task_row(task, seen)));
            }
            let done = of(list, Holder::Done).count();
            let dropped = of(list, Holder::Dropped).count();
            if done + dropped > 0 {
                let words: Vec<String> = [(done, Holder::Done), (dropped, Holder::Dropped)]
                    .into_iter()
                    .filter(|(n, _)| *n > 0)
                    .map(|(n, holder)| format!("{n} {}", counted(holder)))
                    .collect();
                let fold_chevron = if finished {
                    "plates/chevron-up.svg"
                } else {
                    "plates/chevron-down.svg"
                };
                content = content.child(
                    div()
                        .id(("tasks-finished", id.0))
                        .flex_none()
                        .h(px(FOLD_H))
                        .px(px(12.))
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(MUTED))
                        .hover(|s| s.text_color(rgb(TEXT2)))
                        .child(words.join(" · "))
                        .child(
                            svg()
                                .path(fold_chevron)
                                .size(px(10.))
                                .text_color(rgb(MUTED)),
                        )
                        .on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_finished(cx)),
                        ),
                );
                if finished {
                    content = content.children(
                        of(list, Holder::Done)
                            .chain(of(list, Holder::Dropped))
                            .map(|task| task_row(task, seen)),
                    );
                }
            }
            let scroller = div()
                .id(("tasks-list", id.0))
                .flex_1()
                .min_h_0()
                .overflow_y_scroll();
            match shown {
                Some(o) => scroller.track_scroll(&o.scroll),
                None => scroller,
            }
            .child(content)
        });

        Some(
            div()
                .flex_none()
                .h(px(height))
                .flex()
                .flex_col()
                .child(divider())
                .child(bar)
                .children(rows.map(|rows| {
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .child(divider())
                        .child(rows)
                })),
        )
    }
}

#[cfg(test)]
mod tests {
    // Only these, since the glob of GPUI's names above holds a `test` macro of its own.
    use super::{
        FOLD_H, HEADING_H, LIST_PAD, STATUS_H, TASK_PAD, TITLE_H, list_height, status_line,
    };
    use plates_core::tasks::{Change, State, TaskList};
    use std::time::UNIX_EPOCH;

    fn list(tasks: &[(&str, State, &str, &str)]) -> TaskList {
        let mut list = TaskList::default();
        for &(title, state, status, waiting_on) in tasks {
            let change = Change {
                title: Some(title.into()),
                state: Some(state),
                status: Some(status.into()),
                waiting_on: Some(waiting_on.into()),
            };
            list.add(change, UNIX_EPOCH).unwrap();
        }
        list
    }

    #[test]
    fn an_open_list_is_as_tall_as_its_headings_rows_and_fold() {
        let l = list(&[
            ("a", State::Waiting, "Draft in the PR", "you"),
            ("b", State::Doing, "", ""),
            ("c", State::Waiting, "", "Dana"),
            ("d", State::Done, "", ""),
        ]);
        // Three groups: on you with a status line, doing without one, and waiting on others,
        // whose line says who; then the fold for the done task.
        let rows = (2. * TASK_PAD + TITLE_H + STATUS_H)
            + (2. * TASK_PAD + TITLE_H)
            + (2. * TASK_PAD + TITLE_H + STATUS_H);
        let closed = 2. * LIST_PAD + 3. * HEADING_H + rows + FOLD_H;
        assert_eq!(list_height(&l, false), closed);
        assert_eq!(
            list_height(&l, true),
            closed + 2. * TASK_PAD + TITLE_H,
            "unfolding adds the done task's row"
        );
    }

    #[test]
    fn a_task_waiting_on_someone_else_names_them_under_its_title() {
        let l = list(&[("a", State::Waiting, "Asked on Tuesday", "Dana")]);
        assert_eq!(status_line(&l.tasks[0]), "Asked on Tuesday · Dana");
        let l = list(&[("a", State::Waiting, "", "you")]);
        assert_eq!(status_line(&l.tasks[0]), "");
    }
}
