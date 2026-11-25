//! The rack's menus: a popup window of their own at the pointer that opened them, so they can
//! reach past the rack's edges without growing the rack's window.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gpui_kit::*;
use plates_core::{
    Tile, TileId,
    install::Connection,
    timer::{MENU_HOURS, MENU_MINUTES, SNOOZE_HOURS},
};

use crate::{
    motion::Rect,
    native,
    rack_view::RackView,
    updates::{State, Updates},
    view::{BORDER, MUTED, TEAL, TEXT, TILE, YOUR_TURN_TEXT, marker, white},
};

pub const MENU_W: f32 = 156.;

/// The lists of snoozed and archived tiles are wider than the other menus, for their titles.
const LIST_W: f32 = 260.;

/// The rack's own menu is wider still, for "Connect to Claude Code" and "Update needed" side by
/// side, which take 245px with the gap between them, and it still fits inside the 304px rack.
const RACK_MENU_W: f32 = 284.;

/// How many rows the archive's list shows before it scrolls.
const ARCHIVE_ROWS: usize = 8;

/// How long a menu takes to fade in.
const FADE: Duration = Duration::from_millis(120);

/// Which menu a right-click opens.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MenuKind {
    /// The timer lengths, from a tile's hourglass.
    Timer(TileId),
    /// Plates itself, from the grip band at the rack's top, with whether Plates starts at login,
    /// which the rack reads from the registry as it opens the menu, and whether Plates is
    /// connected to Claude Code, as the rack last read it.
    Rack {
        login: bool,
        connection: Option<Connection>,
    },
    /// A finished session's tile, from its text: whether to mark its turn read or unread, which
    /// the rack works out as it opens the menu, since the menu's first frame is drawn while the
    /// rack is still handling the click. (read in gpui-pre 0.3.12, src/app.rs, App::open_window)
    Tile(TileId, bool),
    /// The snoozed tiles, from the moon in the grip band, with how many there were as it opened.
    Snoozed { count: usize },
    /// The tiles you removed, from the archive's box in the grip band, with how many there were
    /// as it opened.
    Archived { count: usize },
}

impl MenuKind {
    pub fn height(self) -> f32 {
        match self {
            // A 6px pad top and bottom, a 23px heading, six 32px rows to remind you in, a 27px
            // heading, three 32px rows to snooze for, and a 1px border.
            MenuKind::Timer(_) => 352.,
            // A 6px pad top and bottom, four 32px rows, and a 1px border.
            MenuKind::Rack { .. } => 141.,
            // A 6px pad top and bottom, one 32px row, and a 1px border.
            MenuKind::Tile(..) => 46.,
            // A 6px pad top and bottom, a 23px heading, a 32px row per tile, and a 1px border.
            MenuKind::Snoozed { count } => 37. + 32. * count as f32,
            // The same with a 27px heading, which holds the Clear all button, and rows up to
            // `ARCHIVE_ROWS`, past which they scroll.
            MenuKind::Archived { count } => 41. + 32. * count.min(ARCHIVE_ROWS) as f32,
        }
    }

    pub fn width(self) -> f32 {
        match self {
            MenuKind::Rack { .. } => RACK_MENU_W,
            MenuKind::Snoozed { .. } | MenuKind::Archived { .. } => LIST_W,
            _ => MENU_W,
        }
    }

    /// Whether the menu opens from the grip band, under the band at the rack's right, rather
    /// than at the pointer: the snoozed and archived lists, and the rack's own menu, which is as
    /// wide as they are and would otherwise reach past the rack's right edge from the grip.
    pub fn opens_under_band(self) -> bool {
        matches!(
            self,
            MenuKind::Rack { .. } | MenuKind::Snoozed { .. } | MenuKind::Archived { .. }
        )
    }
}

pub struct MenuView {
    rack: WeakEntity<RackView>,
    kind: MenuKind,
    hwnd: isize,
    /// Whether the mouse buttons have been up since the menu opened, so the right-click that
    /// opened it doesn't close it.
    armed: bool,
    opened: Instant,
    /// The snoozed or archived tiles as they stood when the rack opened the list of them, with
    /// when each archived one was removed. The menu can't read the rack while drawing, since GPUI
    /// draws a new window once before open_window returns, and the rack is still handling the
    /// click then, so the read panics and Plates aborts. (read in gpui-pre 0.3.12, src/app.rs,
    /// App::open_window)
    listed: Vec<(Tile, Option<SystemTime>)>,
    /// Where an update stands. It's an entity of its own, so the menu can read it in its first
    /// frame while the rack still handles the click, as `listed` explains, and redraw when it
    /// changes.
    updates: Entity<Updates>,
}

impl MenuView {
    pub fn new(
        rack: WeakEntity<RackView>,
        kind: MenuKind,
        bounds: Rect,
        listed: Vec<(Tile, Option<SystemTime>)>,
        updates: Entity<Updates>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let hwnd = native::hwnd_of(window);
        cx.observe(&updates, |_, _, cx| cx.notify()).detach();
        // DWM draws the shadow at full strength around the whole window, so it waits for the
        // fade to finish rather than outline a menu that isn't there yet.
        cx.spawn(async move |_, cx| {
            native::take_over(hwnd);
            native::set_bounds(hwnd, bounds);
            native::set_shadow(hwnd, false);
            cx.background_executor().timer(FADE).await;
            native::set_shadow(hwnd, true);
        })
        .detach();
        MenuView {
            rack,
            kind,
            hwnd,
            armed: false,
            opened: Instant::now(),
            listed,
            updates,
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
            .gap(px(12.))
            .rounded(px(6.))
            .text_size(px(13.))
            .text_color(rgb(TEXT))
            .hover(|s| s.bg(rgb(BORDER)))
            // A label too long for its row ends in an ellipsis, so what follows it stays whole.
            .child(div().min_w_0().truncate().child(label.to_string()))
    }

    // A heading over a menu's rows, 23px tall, or 27px with `top` above it to part it from the
    // rows before it.
    fn heading(text: &'static str, top: f32) -> AnyElement {
        div()
            .pt(px(top))
            .pb(px(4.))
            .px(px(10.))
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(MUTED))
            .child(text)
            .into_any_element()
    }

    // The timer's menu has two parts: lengths to remind you in, which keep the tile on the rack
    // with its timer, and lengths to snooze it for, which take it off the rack until then
    // (user, 2026-11-16).
    fn timer_items(&self, tile: TileId, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut items = vec![Self::heading("REMIND ME IN", 6.)];
        for m in MENU_MINUTES {
            items.push(
                Self::row(("minutes", u64::from(m)), &format!("{m} min"))
                    .on_click(Self::pick(tile, m, cx))
                    .into_any_element(),
            );
        }
        for h in MENU_HOURS {
            let label = if h == 1 {
                "1 hour".to_string()
            } else {
                format!("{h} hours")
            };
            items.push(
                Self::row(("hours", u64::from(h)), &label)
                    .on_click(Self::pick(tile, h * 60, cx))
                    .into_any_element(),
            );
        }
        items.push(Self::heading("SNOOZE", 10.));
        for h in SNOOZE_HOURS {
            items.push(
                Self::row(("snooze", u64::from(h)), &format!("For {h} hours"))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.rack
                            .update(cx, |rack, cx| rack.snooze_for(tile, h, cx))
                            .ok();
                        window.remove_window();
                    }))
                    .into_any_element(),
            );
        }
        items.push(
            Self::row("tomorrow", "Until tomorrow")
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.rack
                        .update(cx, |rack, cx| rack.pick_tomorrow(tile, cx))
                        .ok();
                    window.remove_window();
                }))
                .into_any_element(),
        );
        items
    }

    // Each snoozed tile, with its marker as it stands now and when it comes back, and a click
    // brings it back now (user, 2026-11-16).
    fn snoozed_items(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut items = vec![Self::heading("SNOOZED", 6.)];
        let now = SystemTime::now();
        let offset = native::utc_offset();
        let hours24 = native::uses_24_hour_clock();
        let rows: Vec<_> = self
            .listed
            .iter()
            .map(|(t, _)| (t.id, marker(t, false), t.title(), t.timer.later_ends()))
            .collect();
        for (id, mark, title, ends) in rows {
            let when = ends
                .map(|e| back_at(e, now, offset, hours24))
                .unwrap_or_default();
            items.push(
                div()
                    .id(("snoozed", id.0))
                    .h(px(32.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(6.))
                    .hover(|s| s.bg(rgb(BORDER)))
                    .child(mark)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(13.))
                            .text_color(rgb(TEXT))
                            .child(title),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(11.5))
                            .text_color(rgb(MUTED))
                            .child(when),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.rack.update(cx, |rack, cx| rack.wake(id, cx)).ok();
                        window.remove_window();
                    }))
                    .into_any_element(),
            );
        }
        items
    }

    // The tiles you removed, newest first, each with its marker as it stands now and how long ago
    // you removed it, and a click brings one back. Clear all forgets them (user, 2026-11-16).
    fn archived_items(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let now = SystemTime::now();
        let clear = div()
            .id("clear")
            .px(px(7.))
            .py(px(2.))
            .rounded(px(5.))
            .text_size(px(11.))
            .text_color(rgb(MUTED))
            .hover(|s| s.bg(rgb(BORDER)).text_color(rgb(TEXT)))
            .child("Clear all")
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.rack.update(cx, |rack, cx| rack.clear_archive(cx)).ok();
                this.close(window, cx);
            }));
        let heading = div()
            .h(px(27.))
            .pl(px(10.))
            .pr(px(3.))
            .pb(px(4.))
            .flex()
            .items_end()
            .justify_between()
            .child(
                div()
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(MUTED))
                    .child("ARCHIVE"),
            )
            .child(clear);
        let rows: Vec<_> = self
            .listed
            .iter()
            .map(|(t, at)| (t.id, marker(t, false), t.title(), *at))
            .collect();
        let rows = rows.into_iter().map(|(id, mark, title, at)| {
            let when = at.map(|at| ago(at, now)).unwrap_or_default();
            let row: SharedString = format!("archived-{}", id.0).into();
            // Hovering a row swaps how long ago you removed it for an × that forgets it, as Clear
            // all forgets every row (user, 2026-11-17).
            let forget = div()
                .id(("forget", id.0))
                .absolute()
                .top(px(-4.))
                .right(px(-6.))
                .size(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .opacity(0.)
                .group_hover(row.clone(), |s| s.opacity(1.))
                .hover(|s| s.bg(white(0.08)))
                .child(
                    svg()
                        .path("plates/x.svg")
                        .size(px(14.))
                        .text_color(rgb(MUTED)),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    this.forget(id, window, cx);
                }));
            div()
                .id(("archived", id.0))
                .group(row.clone())
                .flex_none()
                .h(px(32.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(6.))
                .hover(|s| s.bg(rgb(BORDER)))
                .child(mark)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(13.))
                        .text_color(rgb(TEXT))
                        .child(title),
                )
                .child(
                    div()
                        .flex_none()
                        .relative()
                        .text_size(px(11.5))
                        .text_color(rgb(MUTED))
                        .child(div().group_hover(row, |s| s.opacity(0.)).child(when))
                        .child(forget),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.rack.update(cx, |rack, cx| rack.unarchive(id, cx)).ok();
                    window.remove_window();
                }))
        });
        vec![
            heading.into_any_element(),
            div()
                .id("archive-rows")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .children(rows)
                .into_any_element(),
        ]
    }

    /// What picking a length does: starts the tile's timer for `minutes` and closes the menu.
    fn pick(
        tile: TileId,
        minutes: u32,
        cx: &mut Context<Self>,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |this, _: &ClickEvent, window, cx| {
            this.rack
                .update(cx, |rack, cx| rack.pick_minutes(tile, minutes, cx))
                .ok();
            window.remove_window();
        })
    }

    fn rack_items(
        &self,
        login: bool,
        connection: Option<Connection>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let check = || {
            svg()
                .flex_none()
                .path("plates/check.svg")
                .size(px(16.))
                .text_color(rgb(TEXT))
                .into_any_element()
        };
        // A connection another copy or version of Plates left says so where the check would be,
        // as the tray's item does in its text.
        let connected = match connection {
            Some(Connection::On) => Some(check()),
            Some(Connection::Stale) => Some(
                div()
                    .flex_none()
                    .text_size(px(11.5))
                    .text_color(rgb(YOUR_TURN_TEXT))
                    .child("Update needed")
                    .into_any_element(),
            ),
            Some(Connection::Off) | None => None,
        };
        // Plates starts at login only once you turn it on, here or in the tray's menu, since it
        // adds Plates to what Windows runs when you sign in (user, 2026-11-16).
        let login_check = login.then(check);
        vec![
            Self::row("connect", "Connect to Claude Code")
                .children(connected)
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.rack
                        .update(cx, |rack, cx| rack.connect_from_menu(cx))
                        .ok();
                    this.close(window, cx);
                }))
                .into_any_element(),
            Self::row("login", "Start at login")
                .children(login_check)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    if !native::set_start_at_login(!login) {
                        eprintln!("Couldn't change whether Plates starts at login.");
                    }
                    this.rack.update(cx, |rack, _| rack.login_changed()).ok();
                    this.close(window, cx);
                }))
                .into_any_element(),
            self.update_row(cx),
            Self::row("quit", "Quit Plates")
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    if this.rack.update(cx, |rack, cx| rack.quit(cx)).is_err() {
                        cx.quit();
                    }
                }))
                .into_any_element(),
        ]
    }

    // The row for updates says where an update stands, with the version that runs at its right,
    // or the teal dot once an update is ready. A click looks again, and keeps the menu open while
    // the row follows the look, or installs a ready update, after which the rack quits for the
    // updater (record/areas/build/01-updates.md).
    fn update_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let (state, version) = {
            let updates = self.updates.read(cx);
            (updates.state.clone(), updates.version.clone())
        };
        let label = match &state {
            State::NotInstalled => "Development build".to_string(),
            State::Idle => "Check for updates".to_string(),
            State::Checking => "Checking for updates…".to_string(),
            State::UpToDate => "Up to date".to_string(),
            State::Failed => "Couldn't check for updates".to_string(),
            State::Downloading(v) => format!("Downloading {v}…"),
            State::Ready(v) => format!("Update to {v}"),
            State::Installing => "Updating…".to_string(),
        };
        let right = if matches!(state, State::Ready(_)) {
            div()
                .flex_none()
                .size(px(7.))
                .rounded_full()
                .bg(rgb(TEAL))
                .into_any_element()
        } else {
            div()
                .flex_none()
                .text_size(px(11.5))
                .text_color(rgb(MUTED))
                .child(version)
                .into_any_element()
        };
        Self::row("update", &label)
            .child(right)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                let state = this.updates.read(cx).state.clone();
                match state {
                    State::Ready(_) => {
                        this.updates.update(cx, |updates, cx| updates.install(cx));
                    }
                    State::Idle | State::UpToDate | State::Failed => {
                        this.updates.update(cx, |updates, cx| updates.check(cx));
                    }
                    _ => {}
                }
            }))
            .into_any_element()
    }

    fn tile_items(&self, tile: TileId, read: bool, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let label = if read {
            "Mark as read"
        } else {
            "Mark as unread"
        };
        vec![
            Self::row("read", label)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.rack
                        .update(cx, |rack, cx| rack.set_read(tile, read, cx))
                        .ok();
                    this.close(window, cx);
                }))
                .into_any_element(),
        ]
    }

    // Forgets an archived tile and takes its row out of the list, which shrinks from the bottom,
    // under the badge it opened from, or closes once it's empty.
    fn forget(&mut self, id: TileId, window: &mut Window, cx: &mut App) {
        self.rack
            .update(cx, |rack, cx| rack.forget_archived(id, cx))
            .ok();
        self.listed.retain(|(t, _)| t.id != id);
        if self.listed.is_empty() {
            self.close(window, cx);
            return;
        }
        let was = self.kind.height();
        self.kind = MenuKind::Archived {
            count: self.listed.len(),
        };
        let r = native::window_rect(self.hwnd);
        let scale = (r.bottom - r.top) / f64::from(was);
        let bottom = r.top + f64::from(self.kind.height()) * scale;
        native::set_bounds(self.hwnd, Rect { bottom, ..r });
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
            MenuKind::Rack { login, connection } => self.rack_items(login, connection, cx),
            MenuKind::Tile(tile, read) => self.tile_items(tile, read, cx),
            MenuKind::Snoozed { .. } => self.snoozed_items(cx),
            MenuKind::Archived { .. } => self.archived_items(cx),
        };
        // On macOS, a menu that faded in stayed see-through, the tiles under it showing through
        // its panel, which fits a window that stops drawing partway through the fade, as one
        // showcase run did after 129ms. A menu that opened at full opacity was opaque, so menus
        // open that way there (seen on macOS 26.5.2, 2026-11-24, record/areas/rack/log.md).
        let shown = if cfg!(target_os = "macos") {
            1.
        } else {
            (self.opened.elapsed().as_secs_f32() / FADE.as_secs_f32()).min(1.)
        };
        div()
            .opacity(1. - (1. - shown).powi(3))
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

// How long ago you removed a tile: "just now", "12m ago", "3h ago", "yesterday", or "4d ago".
fn ago(at: SystemTime, now: SystemTime) -> String {
    let minutes = now.duration_since(at).unwrap_or_default().as_secs() / 60;
    match minutes {
        0 => "just now".to_string(),
        1..60 => format!("{minutes}m ago"),
        60..1440 => format!("{}h ago", minutes / 60),
        1440..2880 => "yesterday".to_string(),
        _ => format!("{}d ago", minutes / 1440),
    }
}

// When a snoozed tile comes back, in local time on the clock the OS shows you: "Returns 4:30 PM"
// or "Returns 16:30" later today, and "Returns tomorrow 4:00 AM" or "Returns tomorrow 04:00" on
// a later day (user, 2026-11-16).
fn back_at(ends: SystemTime, now: SystemTime, utc_offset: i64, hours24: bool) -> String {
    let local = |t: SystemTime| {
        t.duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64)
            + utc_offset
    };
    let (at, today) = (local(ends), local(now));
    let (hour, minute) = (at.rem_euclid(86_400) / 3600, at.rem_euclid(3600) / 60);
    let clock = if hours24 {
        format!("{hour:02}:{minute:02}")
    } else {
        let half = if hour < 12 { "AM" } else { "PM" };
        format!("{}:{minute:02} {half}", (hour + 11) % 12 + 1)
    };
    if at.div_euclid(86_400) == today.div_euclid(86_400) {
        format!("Returns {clock}")
    } else {
        format!("Returns tomorrow {clock}")
    }
}

#[cfg(test)]
mod tests {
    // Only what the test needs, since the module's glob import of GPUI brings a `test` attribute
    // of its own, which shadows the standard one.
    use super::{ago, back_at};
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn ago_counts_minutes_hours_and_days() {
        let now = UNIX_EPOCH + Duration::from_secs(10 * 86_400);
        let before = |s: u64| now - Duration::from_secs(s);
        assert_eq!(ago(before(30), now), "just now");
        assert_eq!(ago(before(12 * 60), now), "12m ago");
        assert_eq!(ago(before(3 * 3600 + 59), now), "3h ago");
        assert_eq!(ago(before(30 * 3600), now), "yesterday");
        assert_eq!(ago(before(4 * 86_400), now), "4d ago");
    }

    #[test]
    fn a_return_time_follows_the_clock_you_read() {
        let now = UNIX_EPOCH + Duration::from_secs(14 * 3600 + 37 * 60);
        let at = |h: u64, m: u64| UNIX_EPOCH + Duration::from_secs(h * 3600 + m * 60);
        assert_eq!(back_at(at(18, 22), now, 0, false), "Returns 6:22 PM");
        assert_eq!(back_at(at(18, 22), now, 0, true), "Returns 18:22");
        assert_eq!(back_at(at(12, 5), now, 0, false), "Returns 12:05 PM");
        assert_eq!(
            back_at(at(28, 0), now, 0, false),
            "Returns tomorrow 4:00 AM"
        );
        assert_eq!(
            back_at(at(24, 30), now, 0, false),
            "Returns tomorrow 12:30 AM"
        );
        assert_eq!(back_at(at(28, 0), now, 0, true), "Returns tomorrow 04:00");
    }
}
