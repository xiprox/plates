//! The rack: Plates' one window, which holds the core's tiles, and moves, docks, and takes clicks
//! as record/areas/rack/01-window.md designs it. view.rs draws it.

use std::{
    cell::RefCell,
    collections::HashMap,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant, SystemTime},
};

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use plates_core::{
    Rack, Removed, SessionState, TileId, desktop,
    hooks::{self, HookMessage},
    store,
    timer::{DEFAULT_MINUTES, MENU_HOURS, MENU_MINUTES, Timer},
};

use crate::{
    card::{CARD_DELAY, CardView},
    menu::{MenuKind, MenuView},
    motion::{self, Ease, Rect, Side, Tween},
    native,
    place::{self, Place},
    rows::Rows,
    swipe::{Release, Swipe, Swiped},
    tray::{self, Tray},
};

pub const RACK_W: f32 = 304.;
pub const STRIP_W: f32 = 22.;
pub const POPOVER_W: f32 = 176.;
pub const POPOVER_GAP: f32 = 12.;
pub const TOAST_H: f32 = 44.;
const TOAST_FOR: Duration = Duration::from_secs(5);
/// How long without input, or asleep, counts as being away from the PC, after which your next
/// input brightens every time-up tile again (user, 2026-11-16).
const AWAY: Duration = Duration::from_secs(30 * 60);
/// How recent an input counts as you being back.
const BACK: Duration = Duration::from_secs(5);

/// The moon's badge in the grip band, which lists the snoozed tiles: its height, and how far it
/// sits inside the rack's border, from the top and from the right alike (user, 2026-11-16).
pub const MOON_H: f32 = 22.;
pub const MOON_INSET: f32 = 8.;

/// The grip band at the rack's top, as tall as the moon's badge and its inset above and below, so
/// the badge's middle is the band's middle, where the grip sits (user, 2026-11-16).
pub const BAND_H: f32 = MOON_H + 2. * MOON_INSET;

/// The rack's height: the band, a 60px row for each tile and for the phantom tile with 6px
/// between rows, the Undo row while it shows, 12px of padding below, and a 1px border.
pub fn rack_height(tiles: usize, toast: bool) -> f32 {
    rack_height_for(tiles as f32, if toast { 1. } else { 0. })
}

/// The rack's height while rows grow and shrink, with each row counted by how far it has grown.
pub fn rack_height_for(tile_rows: f32, undo_row: f32) -> f32 {
    BAND_H + 74. + 66. * tile_rows + (TOAST_H + 6.) * undo_row
}

/// The docked strip's height for `pips` marks, one per tile and a moon while any is snoozed.
pub fn strip_height(pips: usize) -> f32 {
    14. + 16. * pips.max(1) as f32
}

#[derive(Clone, Copy)]
struct Press {
    cursor: (f64, f64),
    origin: (f64, f64),
}

pub(crate) struct Drag {
    grab: (f64, f64),
    samples: motion::Samples,
    pub(crate) hold: Option<(Side, Instant)>,
}

#[derive(Clone, Copy)]
enum After {
    Dock(Side),
    Settle,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldFor {
    New,
    Rename(TileId),
}

pub(crate) struct Field {
    pub(crate) target: FieldFor,
    prev_focus: isize,
    /// Whether the rack's window has been active since the field opened, after which losing
    /// activation closes the field.
    been_active: bool,
    /// Which of `FIELD_REMINDERS` the new tile starts with, where 0 is none.
    pub(crate) remind: usize,
}

/// How many reminders Tab steps through in a new tile's field: none, then each of the timer
/// menu's "Remind me in" lengths, so the field and the menu offer the same ones (user, 2026-11-16).
pub(crate) const FIELD_REMINDERS: usize = 1 + MENU_MINUTES.len() + MENU_HOURS.len();

/// The minutes of the field's reminder `step`, or `None` for step 0, which sets none.
pub(crate) fn field_reminder(step: usize) -> Option<u32> {
    MENU_MINUTES
        .iter()
        .copied()
        .chain(MENU_HOURS.iter().map(|h| h * 60))
        .nth(step.checked_sub(1)?)
}

/// The menu of timer lengths, open in a window of its own.
pub(crate) struct Menu {
    window: WindowHandle<MenuView>,
    kind: MenuKind,
}

/// A session tile's hover card, open in a window of its own, or about to be.
pub(crate) struct Card {
    tile: TileId,
    window: Option<WindowHandle<CardView>>,
}

/// Where a session tile and its text sat in the rack's window at the last layout, in logical
/// pixels.
#[derive(Clone, Default)]
pub(crate) struct Spot {
    pub(crate) tile: Bounds<Pixels>,
    pub(crate) main: Bounds<Pixels>,
}

pub(crate) struct Toast {
    pub(crate) removed: Removed,
    /// What the Undo row says, such as "Dismissed “Plates”".
    pub(crate) title: String,
    until: Instant,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ended {
    Enter,
    Escape,
    /// A press on the rack outside the field.
    ClickedAway,
    Deactivated,
}

pub struct RackView {
    pub(crate) hwnd: isize,
    pub(crate) rack: Rack,
    data_file: PathBuf,
    place_file: PathBuf,
    sessions_file: PathBuf,
    /// Whether a session tile changed since the last save of sessions.json, which the tick
    /// saves, so a burst of tool events writes the file once.
    sessions_dirty: bool,
    /// The place last saved, which is `None` until the window has taken its saved place, so the
    /// window's first position isn't saved over it.
    saved_place: Option<Place>,
    pub(crate) rows: Rows,
    pub(crate) input: Entity<InputState>,
    pub(crate) field: Option<Field>,
    pub(crate) menu: Option<Menu>,
    card: Option<Card>,
    /// The session tile the pointer last moved over, and when, which opens the tile's card once
    /// the pointer has rested there for `CARD_DELAY`.
    rest: Option<(TileId, Instant)>,
    /// The tile last opened by a click, whose card stays closed until the pointer leaves the
    /// tile, since the click opened the session the card would show (user, 2026-11-16).
    opened: Option<TileId>,
    /// Whether the menu a button in the grip band opens, the moon's or the grip's, was open as
    /// the button was pressed, so the click closes it rather than opening it again.
    menu_was_open: bool,
    /// Which kind of menu last closed itself, at a press outside it, and when.
    menu_closed_at: Option<(std::mem::Discriminant<MenuKind>, Instant)>,
    /// Filled in by the tiles' layout each frame.
    pub(crate) spots: Rc<RefCell<HashMap<TileId, Spot>>>,
    /// Whether you've been away from the PC since the last input Plates saw.
    away: bool,
    /// The wall-clock time of the last tick, which a long gap shows the PC slept since.
    last_tick: SystemTime,
    pub(crate) toast: Option<Toast>,
    pub(crate) docked: Option<Side>,
    dock_area: Rect,
    dock_y: f64,
    pub(crate) peek: bool,
    nudge: Option<Instant>,
    /// The latest time a tile started needing you that the strip has nudged for, or that came
    /// before the rack docked.
    nudged_for: Option<SystemTime>,
    press: Option<Press>,
    pub(crate) drag: Option<Drag>,
    wait_release: bool,
    pub(crate) suppress_click: bool,
    last_press_focus: isize,
    pos: (f64, f64),
    vel: (f64, f64),
    tween: Option<(Tween, After)>,
    pub(crate) scale: f64,
    last_frame: Option<Instant>,
    applied: Option<(i32, i32, i32, i32)>,
    /// Plates' icon in the tray or the menu bar, or `None` when the OS wouldn't take it.
    tray: Option<Tray>,
    /// The tile whose hourglass you just clicked to start a timer, whose fill stays bright until
    /// the pointer leaves it.
    pub(crate) hourglass_held: Option<TileId>,
    /// A two-finger swipe on a tile, from the fingers touching until the tile settles.
    pub(crate) swipe: Option<Swipe>,
    /// The tile whose hourglass a force click is pressing, whose click then starts no timer.
    forced: Option<TileId>,
    /// Whether the rack shows the made-up tiles of `plates showcase`, and so writes no data files,
    /// leaves the tray, the menu bar, and the shortcut to a Plates that runs, and keeps a card it
    /// opens open without the pointer on it.
    pub(crate) showcase: bool,
    _subs: Vec<Subscription>,
    _tasks: Vec<Task<()>>,
}

impl RackView {
    pub fn new(
        rack: Rack,
        data_file: PathBuf,
        events: async_channel::Receiver<HookMessage>,
        (commands_tx, commands): (
            async_channel::Sender<tray::Command>,
            async_channel::Receiver<tray::Command>,
        ),
        showcase: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let hwnd = native::hwnd_of(window);
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Name it"));
        let subs = vec![
            cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    this.finish_field(Ended::Enter, window, cx);
                }
            }),
            cx.observe_keystrokes(|this, ev, window, cx| {
                if this.field.is_some() && ev.keystroke.key == "escape" {
                    this.finish_field(Ended::Escape, window, cx);
                }
            }),
            // Losing activation closes the field, as clicking into another app ends typing there.
            // On macOS, a field the shortcut opened while another app was in front closed at
            // once, before you could type, so the rack's window reported a change while it wasn't
            // yet active. So only a window that has been active since the field opened closes it
            // by losing activation. (seen on macOS 26.5.2, 2026-11-16, record/areas/rack/log.md)
            // The input binds Tab to indenting, and an interceptor runs before any binding, so
            // Tab and Shift+Tab reach the field's reminder first (read in gpui-pre 0.3.12,
            // src/window.rs, `dispatch_key_event`).
            {
                let view = cx.entity().downgrade();
                cx.intercept_keystrokes(move |ev, _, cx| {
                    if ev.keystroke.key != "tab" {
                        return;
                    }
                    let back = ev.keystroke.modifiers.shift;
                    if view
                        .update(cx, |this, cx| this.step_reminder(back, cx))
                        .unwrap_or(false)
                    {
                        cx.stop_propagation();
                    }
                })
            },
            cx.observe_window_activation(window, |this, window, cx| {
                let active = window.is_window_active();
                let Some(field) = this.field.as_mut() else {
                    return;
                };
                if active {
                    field.been_active = true;
                } else if field.been_active {
                    this.finish_field(Ended::Deactivated, window, cx);
                }
            }),
        ];

        // GPUI is still building the window here, and taking it over resizes it, so that waits
        // for a task of its own, which then puts the rack where it last came to rest.
        let take_over = cx.spawn(async move |this, cx| {
            native::take_over(hwnd);
            this.update(cx, |this, cx| {
                let r = native::window_rect(hwnd);
                this.pos = (r.left, r.top);
                if let Some(saved) = place::load(&this.place_file).filter(|_| !this.showcase) {
                    this.restore_place(saved);
                }
                this.saved_place = Some(this.current_place());
                this.applied = None;
                cx.notify();
            })
            .ok();
        });
        // The shortcut for a new tile, registered after the task above takes the window over,
        // which on Windows puts in place the subclass its presses arrive through.
        let hotkey = cx.spawn_in(window, async move |this, cx| {
            if showcase {
                return;
            }
            let Some(presses) = native::watch_hotkey(hwnd) else {
                eprintln!(
                    "Couldn't register the shortcut for a new tile; another app may hold it."
                );
                return;
            };
            while presses.recv().await.is_ok() {
                if this
                    .update_in(cx, |this, window, cx| this.hotkey(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        let receive = cx.spawn(async move |this, cx| {
            while let Ok(message) = events.recv().await {
                if this
                    .update(cx, |this, cx| this.receive(message, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        // A game in borderless full screen raises itself over the rack when it comes to the
        // front, and can do it a moment later, so the rack checks at once and 200ms after, and
        // once a second for a window that raises itself without coming to the front.
        let raised = cx.spawn(async move |_, cx| {
            let Some(foreground) = native::watch_foreground() else {
                return;
            };
            while foreground.recv().await.is_ok() {
                native::keep_on_top(hwnd);
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                native::keep_on_top(hwnd);
            }
        });
        let on_top = cx.spawn(async move |_, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                native::keep_on_top(hwnd);
            }
        });
        let tick = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                    break;
                }
            }
        });
        let tray = (!showcase)
            .then(|| {
                Tray::new(commands_tx)
                    .map_err(|e| eprintln!("Couldn't put Plates in the tray or the menu bar: {e}"))
                    .ok()
            })
            .flatten();
        let tray_commands = cx.spawn(async move |this, cx| {
            while let Ok(command) = commands.recv().await {
                if this
                    .update(cx, |this, cx| this.tray_command(command, cx))
                    .is_err()
                {
                    break;
                }
            }
        });

        let r = native::window_rect(hwnd);
        let place_file = data_file.with_file_name("place.json");
        let sessions_file = data_file.with_file_name("sessions.json");
        let mut view = RackView {
            hwnd,
            rack,
            data_file,
            place_file,
            sessions_file,
            sessions_dirty: true,
            saved_place: None,
            rows: Rows::default(),
            input,
            field: None,
            menu: None,
            card: None,
            rest: None,
            opened: None,
            menu_was_open: false,
            menu_closed_at: None,
            spots: Rc::default(),
            away: false,
            last_tick: SystemTime::now(),
            toast: None,
            docked: None,
            dock_area: Rect::default(),
            dock_y: 0.,
            peek: false,
            nudge: None,
            nudged_for: None,
            press: None,
            drag: None,
            wait_release: false,
            suppress_click: false,
            last_press_focus: 0,
            pos: (r.left, r.top),
            vel: (0., 0.),
            tween: None,
            scale: native::scale(window),
            last_frame: None,
            applied: None,
            tray,
            hourglass_held: None,
            swipe: None,
            forced: None,
            showcase,
            _subs: subs,
            _tasks: vec![
                take_over,
                hotkey,
                receive,
                tick,
                raised,
                on_top,
                tray_commands,
            ],
        };
        view.rack.tick(SystemTime::now());
        view.nudged_for = view.latest_need();
        view
    }

    fn receive(&mut self, message: HookMessage, cx: &mut Context<Self>) {
        hooks::apply(&mut self.rack, message, SystemTime::now());
        self.after_change(cx);
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        let now = SystemTime::now();
        // GPUI reports the pointer leaving the window only on a change, so a pointer that left
        // after a report `set_peek` ignored would leave the strip peeking. The tick closes it.
        if self.peek && !native::window_rect(self.hwnd).contains(native::cursor_pos()) {
            self.set_peek(false, cx);
        }
        if self.rack.tick(now) {
            self.save();
            self.after_change(cx);
        }
        // A tile brightens once as its time runs out, which nobody sees if you're away, so the
        // first input after you were away brightens every time-up tile again.
        let idle = native::idle_for();
        let slept = now
            .duration_since(self.last_tick)
            .is_ok_and(|gap| gap >= AWAY);
        self.last_tick = now;
        if idle >= AWAY || slept {
            self.away = true;
        }
        if self.away && idle < BACK {
            self.away = false;
            if self.rack.welcome_back(now) {
                self.after_change(cx);
            }
        }
        if self
            .toast
            .as_ref()
            .is_some_and(|t| t.until <= Instant::now())
        {
            self.toast = None;
        }
        if self.sessions_dirty && !self.showcase {
            self.sessions_dirty = false;
            if let Err(e) = store::save_sessions(&self.rack, &self.sessions_file) {
                eprintln!("Couldn't save {}: {e}", self.sessions_file.display());
            }
        }
        // The rings drain and the hover text counts minutes, so the rack redraws each second.
        cx.notify();
    }

    fn latest_need(&self) -> Option<SystemTime> {
        self.rack
            .tiles()
            .iter()
            .filter_map(|t| t.need.map(|(_, since)| since))
            .max()
    }

    // When a tile starts needing you while the rack is docked, the strip nudges out of its edge
    // once, as record/areas/rack/01-window.md asks.
    fn after_change(&mut self, cx: &mut Context<Self>) {
        self.sessions_dirty = true;
        let latest = self.latest_need();
        if latest > self.nudged_for {
            if self.docked.is_some() && self.tween.is_none() {
                self.nudge = Some(Instant::now());
            }
            self.nudged_for = latest;
        }
        cx.notify();
    }

    fn save(&self) {
        if self.showcase {
            return;
        }
        if let Err(e) = store::save(&self.rack, &self.data_file) {
            eprintln!("Couldn't save {}: {e}", self.data_file.display());
        }
    }

    fn current_place(&self) -> Place {
        match self.docked {
            Some(side) => Place {
                x: match side {
                    Side::Left => self.dock_area.left,
                    Side::Right => self.dock_area.right - 1.,
                },
                y: self.pos.1,
                docked: Some(side),
            },
            None => Place {
                x: self.pos.0,
                y: self.pos.1,
                docked: None,
            },
        }
    }

    /// Saves where the rack sits once it has come to rest somewhere new.
    pub(crate) fn remember_place(&mut self) {
        if self.showcase || self.animating() || self.saved_place.is_none() {
            return;
        }
        let place = self.current_place();
        if self.saved_place == Some(place) {
            return;
        }
        self.saved_place = Some(place);
        if let Err(e) = place::save(place, &self.place_file) {
            eprintln!("Couldn't save {}: {e}", self.place_file.display());
        }
    }

    // A saved place on a monitor that's gone, or partly off its monitor, moves onto the nearest
    // monitor's work area.
    fn restore_place(&mut self, saved: Place) {
        let (w, h) = self.rack_size();
        match saved.docked {
            None => {
                let a = native::edges_at((saved.x + w / 2., saved.y + h / 2.)).area;
                self.pos = (
                    saved.x.clamp(a.left, (a.right - w).max(a.left)),
                    saved.y.clamp(a.top, (a.bottom - h).max(a.top)),
                );
            }
            Some(side) => {
                let a = native::edges_at((saved.x, saved.y)).area;
                let strip_h = strip_height(self.strip_pips()) as f64 * self.scale;
                self.dock_area = a;
                self.docked = Some(side);
                self.peek = false;
                self.pos = (
                    match side {
                        Side::Left => a.left - w,
                        Side::Right => a.right,
                    },
                    saved.y.clamp(a.top, (a.bottom - strip_h).max(a.top)),
                );
                self.nudged_for = self.latest_need();
                native::set_shadow(self.hwnd, false);
            }
        }
    }

    pub(crate) fn animating(&self) -> bool {
        self.press.is_some()
            || self.wait_release
            || self.vel != (0., 0.)
            || self.tween.is_some()
            || self.nudge.is_some()
    }

    // The window's place and size for the rack's state, in physical pixels: the rack, or the
    // docked strip, widened while it peeks and lists what needs you.
    fn bounds(&self) -> Rect {
        let s = self.scale;
        match self.docked {
            Some(side) => {
                let w = self.strip_window_width() as f64 * s;
                let h = self.strip_window_height() as f64 * s;
                let out = self
                    .nudge
                    .and_then(|n| motion::nudge_offset(n.elapsed()))
                    .unwrap_or(0.)
                    * s;
                let x = match side {
                    Side::Left => self.dock_area.left + out,
                    Side::Right => self.dock_area.right - w - out,
                };
                Rect::new((x, self.pos.1), (w, h))
            }
            None => Rect::new(
                self.pos,
                (RACK_W as f64 * s, self.shown_height() as f64 * s),
            ),
        }
    }

    pub(crate) fn needing(&self) -> usize {
        self.rack.needs_you().count()
    }

    pub(crate) fn popover_shows(&self) -> bool {
        self.peek && self.needing() > 0
    }

    pub(crate) fn strip_window_width(&self) -> f32 {
        let mut w = STRIP_W;
        if self.peek {
            w += motion::PEEK as f32;
        }
        if self.popover_shows() {
            w += POPOVER_GAP + POPOVER_W;
        }
        w
    }

    pub(crate) fn popover_height(&self) -> f32 {
        20. + 14. + 22. * self.needing() as f32
    }

    fn strip_window_height(&self) -> f32 {
        let strip = strip_height(self.strip_pips());
        if self.popover_shows() {
            strip.max(self.popover_height())
        } else {
            strip
        }
    }

    /// Moves and sizes the window to the rack's state, through a foreground task, since GPUI
    /// handles the WM_MOVE a SetWindowPos sends by updating the window's state, which it can't
    /// do while it's drawing. (read in gpui-pre-windows 0.3.12, src/window.rs, WindowsWindow::resize)
    pub(crate) fn place(&mut self, cx: &mut Context<Self>) {
        let r = self.bounds();
        let key = (
            r.left.round() as i32,
            r.top.round() as i32,
            (r.right - r.left).round() as i32,
            (r.bottom - r.top).round() as i32,
        );
        if self.applied == Some(key) {
            return;
        }
        self.applied = Some(key);
        let hwnd = self.hwnd;
        cx.foreground_executor()
            .spawn(async move { native::set_bounds(hwnd, r) })
            .detach();
    }

    pub(crate) fn on_press(
        &mut self,
        _: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A press anywhere on the rack but the field closes the field without adding the tile, as
        // Esc does; the field stops its own presses reaching here (user, 2026-11-16).
        if self.field.is_some() {
            self.finish_field(Ended::ClickedAway, window, cx);
        }
        self.forced = None;
        self.last_press_focus = native::foreground_window();
        self.suppress_click = false;
        if self.docked.is_some() || self.tween.is_some() {
            return;
        }
        let r = native::window_rect(self.hwnd);
        self.pos = (r.left, r.top);
        self.press = Some(Press {
            cursor: native::cursor_pos(),
            origin: self.pos,
        });
        // A press catches a rack that's still sliding.
        self.vel = (0., 0.);
        cx.notify();
    }

    /// Fingers touching the trackpad over a tile, or over the Undo row, start a swipe on it,
    /// which the rack then follows through `on_scroll`, since it slides out from under the
    /// pointer.
    pub(crate) fn swipe_touched(&mut self, target: Swiped, e: &ScrollEvent) {
        if e.touch_phase == TouchPhase::Started && self.field.is_none() {
            self.swipe = Some(Swipe::new(target));
        }
    }

    /// Follows a two-finger swipe across the rack, sliding its tile with the fingers and, once
    /// they lift, back into place or out, which dismisses the tile as its × does
    /// (user, 2026-11-16).
    pub(crate) fn on_scroll(&mut self, e: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        // A mouse wheel's notches come as lines, and only a trackpad's or a touchscreen's scroll
        // comes in exact pixels.
        let ScrollDelta::Pixels(d) = e.delta else {
            return;
        };
        let Some(swipe) = self.swipe.as_mut() else {
            return;
        };
        let now = Instant::now();
        match e.touch_phase {
            TouchPhase::Started => {}
            TouchPhase::Moved => {
                if swipe.moved(f32::from(d.x), f32::from(d.y), now) {
                    // A swipe doesn't move the pointer, so the card's wait would run out mid-swipe.
                    if let Swiped::Tile(id) = swipe.target {
                        self.rest = None;
                        self.opened = Some(id);
                        if self.card.as_ref().is_some_and(|c| c.tile == id) {
                            self.close_card(cx);
                        }
                    }
                }
                cx.notify();
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                let cancel = e.touch_phase == TouchPhase::Cancelled;
                let target = swipe.target;
                if swipe.release(cancel, now) == Release::Dismiss {
                    match target {
                        Swiped::Tile(id) => self.take_off(id, cx),
                        // Swiping the Undo row away keeps the removal, as waiting it out does.
                        Swiped::Undo => {
                            self.toast = None;
                            self.after_change(cx);
                        }
                    }
                }
                cx.notify();
            }
        }
    }

    /// How far a tile sits sideways from its place, in logical px, while a swipe moves it. A
    /// `leaving` tile is one shrinking out of the rack, which a swipe that dismissed it moves.
    pub(crate) fn swipe_offset(&self, target: Swiped, leaving: bool, now: Instant) -> f32 {
        match &self.swipe {
            Some(s) if s.target == target && s.dismissed() == leaving => s.offset(now),
            _ => 0.,
        }
    }

    /// Pressing on past the click on a Mac trackpad, a force click, opens an hourglass's menu, as a
    /// right-click does, and the click the press ends in starts or cancels no timer
    /// (user, 2026-11-16).
    pub(crate) fn force_press(
        &mut self,
        id: TileId,
        e: &MousePressureEvent,
        cx: &mut Context<Self>,
    ) {
        if e.stage != PressureStage::Force || self.forced == Some(id) {
            return;
        }
        self.forced = Some(id);
        // The press goes on into the menu, so moving it mustn't drag or throw the rack.
        self.press = None;
        self.open_menu(MenuKind::Timer(id), e.position, cx);
    }

    pub(crate) fn note_press(&mut self) {
        self.last_press_focus = native::foreground_window();
    }

    /// Moves the window one frame: the drag, the throw, a slide into or out of the dock, or the
    /// strip's nudge.
    pub(crate) fn step(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        let dt = self
            .last_frame
            .map_or(0., |t| (now - t).as_secs_f64())
            .min(0.05);
        self.last_frame = Some(now);
        let scale = self.scale;

        if self
            .nudge
            .is_some_and(|n| motion::nudge_offset(n.elapsed()).is_none())
        {
            self.nudge = None;
        }

        if let Some((tween, after)) = self.tween {
            let (p, done) = tween.at(now);
            self.pos = p;
            if done {
                self.tween = None;
                if let After::Dock(side) = after {
                    self.finish_dock(side);
                }
            }
            return;
        }

        if self.wait_release {
            if !native::left_button_down() {
                self.wait_release = false;
            }
            return;
        }

        if let Some(press) = self.press {
            let cursor = native::cursor_pos();
            let moved = (cursor.0 - press.cursor.0).hypot(cursor.1 - press.cursor.1);
            if self.drag.is_none() && moved >= motion::DRAG_THRESHOLD * scale {
                self.drag = Some(Drag {
                    grab: (
                        press.cursor.0 - press.origin.0,
                        press.cursor.1 - press.origin.1,
                    ),
                    samples: motion::Samples::default(),
                    hold: None,
                });
                self.suppress_click = true;
            }
            if let Some(mut drag) = self.drag.take() {
                let want = (cursor.0 - drag.grab.0, cursor.1 - drag.grab.1);
                let size = self.rack_size();
                let (p, held) = motion::clamp_drag(want, size, &native::edges_at(cursor));
                self.pos = p;
                drag.hold = match (held, drag.hold) {
                    (Some(side), Some((was, since))) if side == was => Some((was, since)),
                    (Some(side), _) => Some((side, now)),
                    (None, _) => None,
                };
                drag.samples.push(now, want);
                let docks = drag.hold.filter(|(_, since)| now - *since >= motion::HOLD);
                self.drag = Some(drag);
                if let Some((side, _)) = docks {
                    self.begin_dock(side);
                    self.wait_release = true;
                    return;
                }
            }
            if !native::left_button_down() {
                self.press = None;
                if let Some(drag) = self.drag.take() {
                    self.vel = drag.samples.throw_velocity(now, scale);
                }
            }
            cx.notify();
            return;
        }

        if self.vel != (0., 0.) && self.docked.is_none() {
            let size = self.rack_size();
            let center = (self.pos.0 + size.0 / 2., self.pos.1 + size.1 / 2.);
            let edges = native::edges_at(center);
            let (p, v, _) = motion::step_throw(self.pos, self.vel, size, &edges, dt, scale);
            self.pos = p;
            self.vel = v;
        }
    }

    fn rack_size(&self) -> (f64, f64) {
        (
            RACK_W as f64 * self.scale,
            self.shown_height() as f64 * self.scale,
        )
    }

    /// The rack's height in logical pixels this frame, with its rows as far as they've grown.
    pub(crate) fn shown_height(&self) -> f32 {
        let now = Instant::now();
        let undo = self.rows.undo(now).map_or(0., |(_, f)| f);
        rack_height_for(self.rows.tile_rows(self.rack.tiles(), now), undo)
    }

    /// Notes the rows that changed since the last frame, so they grow in or shrink out.
    pub(crate) fn track_rows(&mut self) {
        let undo = self.toast.as_ref().map(|t| t.title.as_str());
        self.rows.track(
            self.rack.tiles(),
            undo,
            self.docked.is_none(),
            Instant::now(),
        );
    }

    // The rack slides into the edge it was held against, and the strip takes its place.
    fn begin_dock(&mut self, side: Side) {
        self.press = None;
        self.drag = None;
        self.vel = (0., 0.);
        let size = self.rack_size();
        let center = (self.pos.0 + size.0 / 2., self.pos.1 + size.1 / 2.);
        let a = native::edges_at(center).area;
        self.dock_area = a;
        self.dock_y = self.pos.1;
        let to_x = match side {
            Side::Left => a.left - size.0,
            Side::Right => a.right,
        };
        self.tween = Some((
            Tween {
                from: self.pos,
                to: (to_x, self.pos.1),
                start: Instant::now(),
                ease: Ease::In,
            },
            After::Dock(side),
        ));
    }

    fn finish_dock(&mut self, side: Side) {
        let s = self.scale;
        let a = self.dock_area;
        let h = strip_height(self.strip_pips()) as f64 * s;
        let y = (self.dock_y + 12. * s).clamp(a.top + 16. * s, (a.bottom - h - 16. * s).max(a.top));
        self.docked = Some(side);
        self.peek = false;
        self.pos.1 = y;
        self.nudged_for = self.latest_need();
        native::set_shadow(self.hwnd, false);
    }

    // The rack slides back out of the edge and stops flush with it.
    pub(crate) fn undock(&mut self, cx: &mut Context<Self>) {
        let Some(side) = self.docked.take() else {
            return;
        };
        let s = self.scale;
        let a = self.dock_area;
        let (w, h) = self.rack_size();
        let y = (self.pos.1 - 12. * s).clamp(a.top, (a.bottom - h).max(a.top));
        let (from_x, to_x) = match side {
            Side::Left => (a.left - w, a.left),
            Side::Right => (a.right, a.right - w),
        };
        self.peek = false;
        self.nudge = None;
        self.pos = (from_x, y);
        native::set_shadow(self.hwnd, true);
        self.tween = Some((
            Tween {
                from: (from_x, y),
                to: (to_x, y),
                start: Instant::now(),
                ease: Ease::Out,
            },
            After::Settle,
        ));
        cx.notify();
    }

    fn tray_command(&mut self, command: tray::Command, cx: &mut Context<Self>) {
        match command {
            tray::Command::Show => self.show_centered(cx),
            tray::Command::ToggleLogin => {
                if !native::set_start_at_login(!native::starts_at_login()) {
                    eprintln!("Couldn't change whether Plates starts at login.");
                }
                self.login_changed();
            }
            tray::Command::Quit => cx.quit(),
        }
    }

    /// Brings the tray's menu up to date after either menu changed whether Plates starts at login.
    pub(crate) fn login_changed(&self) {
        if let Some(tray) = &self.tray {
            tray.show_login(native::starts_at_login());
        }
    }

    // A rack can be out of sight under another window, docked at the edge of a screen you're not
    // looking at, or off every screen, so the tray's "Show Plates", and opening Plates again while
    // it runs, put it undocked at the middle of the primary screen, where it's always found.
    fn show_centered(&mut self, cx: &mut Context<Self>) {
        self.press = None;
        self.drag = None;
        self.wait_release = false;
        self.vel = (0., 0.);
        self.tween = None;
        self.nudge = None;
        self.peek = false;
        if self.docked.take().is_some() {
            native::set_shadow(self.hwnd, true);
        }
        // The rack's coordinates start at the primary screen's top-left corner on both OSes, so
        // (1, 1) is inside the primary screen and outside any screen that touches its edge.
        let a = native::edges_at((1., 1.)).area;
        let (w, h) = self.rack_size();
        self.pos = (
            a.left + ((a.right - a.left - w) / 2.).max(0.),
            a.top + ((a.bottom - a.top - h) / 2.).max(0.),
        );
        native::keep_on_top(self.hwnd);
        cx.notify();
    }

    pub(crate) fn set_peek(&mut self, on: bool, cx: &mut Context<Self>) {
        // Peeking widens the window inward, which moves it under a pointer that stays still. On
        // macOS, GPUI learns where the pointer is in a window only when the mouse moves, so it
        // reported the strip unhovered at once, the peek closed, and the strip flickered between
        // the two (seen on macOS 26.5.2, 2026-11-16, record/areas/rack/log.md). So the peek
        // closes only once the OS says the pointer has left the window.
        if !on && native::window_rect(self.hwnd).contains(native::cursor_pos()) {
            return;
        }
        if self.docked.is_some() && self.tween.is_none() && self.peek != on {
            self.peek = on;
            cx.notify();
        }
    }

    // Every click on the rack lands only if it didn't start a drag, and none of them takes
    // keyboard focus from the app you're in.
    pub(crate) fn click_tile(
        &mut self,
        id: TileId,
        clicks: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.suppress_click {
            return;
        }
        let Some(tile) = self.rack.tile(id) else {
            return;
        };
        if clicks >= 2 && tile.is_own() {
            self.start_field(FieldFor::Rename(id), window, cx);
            return;
        }
        self.open_tile(id, cx);
    }

    /// What a click on a tile or its card does: it reads a finished session's turn, as opening
    /// the session in the app does, opens the session there, and clears time-up. The tile's card
    /// waits for the pointer to leave the tile before it can open again.
    pub(crate) fn open_tile(&mut self, id: TileId, cx: &mut Context<Self>) {
        self.opened = Some(id);
        if self.rest.is_some_and(|(resting, _)| resting == id) {
            self.rest = None;
        }
        if self.rack.mark_read(id, SystemTime::now()) {
            self.after_change(cx);
        }
        let Some(tile) = self.rack.tile(id) else {
            return;
        };
        // A session the Claude desktop app keeps opens there by the app's own ID, and a session
        // it doesn't, such as one in a terminal, has nowhere to open.
        if let Some(link) = tile
            .session()
            .and_then(|s| s.app_id.as_deref())
            .and_then(desktop::session_link)
        {
            // On macOS, Plates hands the keyboard to the app that opens the link, and can only
            // once it's the active app itself, which a click on the rack doesn't make it, so it
            // asks to be and waits up to 300ms for macOS to make it so. native/mac.rs's
            // `open_url` says why.
            #[cfg(target_os = "macos")]
            {
                native::activate_self();
                cx.spawn(async move |_, cx| {
                    for _ in 0..30 {
                        if native::is_active() {
                            break;
                        }
                        cx.background_executor()
                            .timer(Duration::from_millis(10))
                            .await;
                    }
                    native::open_url(&link);
                })
                .detach();
            }
            #[cfg(not(target_os = "macos"))]
            native::open_url(&link);
        }
        if tile.timer.is_time_up() {
            self.rack.clear_time_up(id, SystemTime::now());
            self.save();
            self.after_change(cx);
        }
    }

    pub(crate) fn click_timer(&mut self, id: TileId, cx: &mut Context<Self>) {
        if self.suppress_click || self.forced == Some(id) {
            return;
        }
        let now = SystemTime::now();
        match self.rack.tile(id).map(|t| t.timer.is_set()) {
            Some(true) => self.rack.cancel_timer(id, now),
            Some(false) => {
                self.rack.start_timer(id, DEFAULT_MINUTES, now);
                self.hourglass_held = Some(id);
            }
            None => return,
        }
        self.save();
        self.after_change(cx);
    }

    /// Answers a session's wrap-up with one of its choices from the hover card: it opens the
    /// session, as a click on the tile does, and presses the app's own button for the choice once
    /// the app shows the session (user, 2026-11-15).
    pub(crate) fn answer(&mut self, id: TileId, choice: &str, cx: &mut Context<Self>) {
        let title = self
            .rack
            .tile(id)
            .and_then(|t| t.session())
            .and_then(|s| s.app_title.clone());
        self.open_tile(id, cx);
        if let Some(title) = title {
            crate::press::press_choice(title, choice.to_string());
        }
    }

    /// Right-clicking a finished session's text opens a menu that marks its turn read, without
    /// opening the session, or unread again once you've read it (user, 2026-11-16). Other tiles
    /// have nothing to mark, so the click opens nothing.
    pub(crate) fn open_tile_menu(&mut self, id: TileId, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(s) = self.rack.tile(id).and_then(|t| t.session()) else {
            return;
        };
        let read = match s.state {
            SessionState::Unread => true,
            SessionState::YourTurn => !s.read,
            SessionState::Done => false,
            _ => return,
        };
        self.open_menu(MenuKind::Tile(id, read), at, cx);
    }

    /// Marks a session's turn read or unread, from the tile's menu.
    pub(crate) fn set_read(&mut self, id: TileId, read: bool, cx: &mut Context<Self>) {
        self.menu = None;
        let now = SystemTime::now();
        let changed = if read {
            self.rack.mark_read(id, now)
        } else {
            self.rack.mark_unread(id, now)
        };
        if changed {
            self.after_change(cx);
        }
    }

    pub(crate) fn cancel_timer(&mut self, id: TileId, cx: &mut Context<Self>) {
        self.rack.cancel_timer(id, SystemTime::now());
        self.save();
        self.after_change(cx);
    }

    /// Notes the pointer moving over a session tile's text. A task waits until it has rested
    /// there for `CARD_DELAY`, then opens the tile's card, and each move pushes the wait back.
    pub(crate) fn pointer_moved(&mut self, id: TileId, cx: &mut Context<Self>) {
        if self.card.as_ref().is_some_and(|c| c.tile == id) || self.opened == Some(id) {
            return;
        }
        let waiting = self.rest.is_some();
        self.rest = Some((id, Instant::now()));
        if waiting {
            return;
        }
        cx.spawn(async move |this, cx| {
            loop {
                let Ok(Some(wait)) = this.update(cx, |this, cx| this.rest_left(cx)) else {
                    break;
                };
                cx.background_executor().timer(wait).await;
            }
        })
        .detach();
    }

    /// Notes the pointer leaving a session tile's text, after which a tile opened by a click can
    /// open its card again.
    pub(crate) fn pointer_left(&mut self, id: TileId) {
        if self.opened == Some(id) {
            self.opened = None;
        }
    }

    // How long the pointer has yet to rest, or `None` once the wait is over and the card has had
    // its chance to open.
    fn rest_left(&mut self, cx: &mut Context<Self>) -> Option<Duration> {
        let (id, at) = self.rest?;
        let left = (at + CARD_DELAY).saturating_duration_since(Instant::now());
        if !left.is_zero() {
            return Some(left);
        }
        self.rest = None;
        self.open_card(id, cx);
        None
    }

    // The card opens in a window of its own over the tile, like the menu, so it can reach past
    // the rack's edges. A pointer that left the rack sends the rack no more moves, so the card
    // opens only if the pointer is still on the tile's text.
    fn open_card(&mut self, id: TileId, cx: &mut Context<Self>) {
        let busy = self.press.is_some()
            || self.drag.is_some()
            || self.field.is_some()
            || self.menu.is_some()
            || self.card.is_some()
            || self.docked.is_some()
            || self.tween.is_some()
            || native::left_button_down()
            || native::right_button_down();
        let Some(folder) = self
            .rack
            .tile(id)
            .and_then(|t| t.session())
            .map(|s| s.folder.clone())
        else {
            return;
        };
        let Some(spot) = self.spots.borrow().get(&id).cloned().filter(|_| !busy) else {
            return;
        };
        if !self.on_screen(&spot.main).contains(native::cursor_pos()) {
            return;
        }
        self.show_card(id, folder, &spot, cx);
    }

    /// Opens a tile's card for the showcase's picture, with no pointer resting on the tile.
    #[cfg(feature = "capture")]
    pub(crate) fn showcase_card(&mut self, id: TileId, cx: &mut Context<Self>) {
        let folder = self
            .rack
            .tile(id)
            .and_then(|t| t.session())
            .map(|s| s.folder.clone());
        let spot = self.spots.borrow().get(&id).cloned();
        if let (Some(folder), Some(spot)) = (folder, spot) {
            self.show_card(id, folder, &spot, cx);
        }
    }

    /// The open card's window, once it has opened.
    #[cfg(feature = "capture")]
    pub(crate) fn card_window(&self) -> Option<AnyWindowHandle> {
        self.card.as_ref().and_then(|c| c.window).map(Into::into)
    }

    // Where bounds in the rack's window sit on screen, in the rack's pixels.
    fn on_screen(&self, b: &Bounds<Pixels>) -> Rect {
        let s = self.scale;
        let r = native::window_rect(self.hwnd);
        let n = |p: Pixels| f64::from(f32::from(p)) * s;
        Rect::new(
            (r.left + n(b.origin.x), r.top + n(b.origin.y)),
            (n(b.size.width), n(b.size.height)),
        )
    }

    fn show_card(&mut self, id: TileId, folder: PathBuf, spot: &Spot, cx: &mut Context<Self>) {
        let s = self.scale;
        let from = self.on_screen(&spot.tile);
        let rack = cx.entity().downgrade();
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px((from.left / s) as f32), px((from.top / s) as f32)),
                size(
                    px(((from.right - from.left) / s) as f32),
                    px(((from.bottom - from.top) / s) as f32),
                ),
            ))),
            titlebar: None,
            focus: false,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            inactive_frame_interval: None,
            ..Default::default()
        };
        self.card = Some(Card {
            tile: id,
            window: None,
        });
        // GPUI draws a new window once before open_window returns, and the card's frame reads
        // the rack, which it can't while the rack is being updated, as it is here. (read in
        // gpui-pre 0.3.12, src/app.rs, App::open_window)
        cx.defer(move |cx| {
            let opened = cx.open_window(options, |window, cx| {
                cx.new(|cx| CardView::new(rack.clone(), id, from, folder, window, cx))
            });
            rack.update(cx, |this, _| match opened {
                Ok(window) => {
                    if let Some(card) = this.card.as_mut().filter(|c| c.tile == id) {
                        card.window = Some(window);
                    }
                }
                Err(e) => {
                    eprintln!("Couldn't open a tile's card: {e}");
                    this.card_closed(id);
                }
            })
            .ok();
        });
    }

    /// The card window tells the rack it closed itself.
    pub(crate) fn card_closed(&mut self, id: TileId) {
        if self.card.as_ref().is_some_and(|c| c.tile == id) {
            self.card = None;
        }
    }

    // Closing a window can run while the rack draws, as when the rack docks, so it waits for
    // GPUI to finish the frame.
    fn close_card(&mut self, cx: &mut Context<Self>) {
        self.rest = None;
        if let Some(window) = self.card.take().and_then(|c| c.window) {
            cx.defer(move |cx| {
                window
                    .update(cx, |_, window, _| window.remove_window())
                    .ok();
            });
        }
    }

    // The menu opens in a window of its own at the pointer, so it can reach past the rack's edges
    // without growing the rack's window, whose shadow DWM would draw around the grown rectangle.
    // It opens below and to the right of the pointer, to the left near the screen's right edge,
    // and above it near the screen's bottom (user, 2026-11-15). The list of snoozed tiles opens
    // from the moon at the rack's right edge, so it opens leftward, its right edge on the tiles'
    // right edge, within the rack's width (user, 2026-11-16).
    pub(crate) fn open_menu(&mut self, kind: MenuKind, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.close_menu(cx);
        self.close_card(cx);
        let s = self.scale;
        let r = native::window_rect(self.hwnd);
        let pointer = (
            r.left + f64::from(f32::from(at.x)) * s,
            r.top + f64::from(f32::from(at.y)) * s,
        );
        let area = native::edges_at(pointer).area;
        let menu_h = kind.height();
        let menu_w = kind.width();
        let (w, h) = (menu_w as f64 * s, menu_h as f64 * s);
        let right = pointer.0 - 12. * s;
        let x = if let MenuKind::Snoozed { .. } = kind {
            // Its right edge lines up with the moon's badge, inside the panel's 1px border.
            (r.right - f64::from(1. + MOON_INSET) * s - w)
                .clamp(area.left, (area.right - w).max(area.left))
        } else if right + w <= area.right {
            right
        } else {
            (pointer.0 - w + 12. * s).clamp(area.left, (area.right - w).max(area.left))
        };
        // The list of snoozed tiles opens 4px under the moon's badge, below the panel's 1px border,
        // so the list leaves the badge whole (user, 2026-11-16).
        let below = match kind {
            MenuKind::Snoozed { .. } => r.top + f64::from(1. + MOON_INSET + MOON_H + 4.) * s,
            _ => pointer.1 + 6. * s,
        };
        let y = if below + h <= area.bottom {
            below
        } else {
            (pointer.1 - h - 6. * s).max(area.top)
        };
        let bounds = Rect::new((x, y), (w, h));
        let rack = cx.entity().downgrade();
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px((x / s) as f32), px((y / s) as f32)),
                size(px(menu_w), px(menu_h)),
            ))),
            titlebar: None,
            focus: false,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            inactive_frame_interval: None,
            ..Default::default()
        };
        let snoozed = match kind {
            MenuKind::Snoozed { .. } => self.rack.snoozed().cloned().collect(),
            _ => Vec::new(),
        };
        match cx.open_window(options, |window, cx| {
            cx.new(|cx| MenuView::new(rack, kind, bounds, snoozed, window, cx))
        }) {
            Ok(window) => self.menu = Some(Menu { window, kind }),
            Err(e) => eprintln!("Couldn't open a menu: {e}"),
        }
    }

    pub(crate) fn pick_minutes(&mut self, id: TileId, minutes: u32, cx: &mut Context<Self>) {
        self.menu = None;
        self.rack.start_timer(id, minutes, SystemTime::now());
        self.save();
        self.after_change(cx);
    }

    /// Snoozes a tile until tomorrow, from the timer's menu or the hover card (user, 2026-11-16).
    pub(crate) fn pick_tomorrow(&mut self, id: TileId, cx: &mut Context<Self>) {
        let now = SystemTime::now();
        self.snooze(id, Timer::tomorrow(now, native::utc_offset()), cx);
    }

    /// Snoozes a tile for `hours`, from the timer's menu.
    pub(crate) fn snooze_for(&mut self, id: TileId, hours: u32, cx: &mut Context<Self>) {
        self.snooze(id, Timer::later(SystemTime::now(), hours), cx);
    }

    // A snoozed tile leaves the rack until its time, and the moon in the grip band counts it, so
    // a session you can't act on yet stops asking for you (user, 2026-11-16).
    fn snooze(&mut self, id: TileId, until: Timer, cx: &mut Context<Self>) {
        self.menu = None;
        self.close_card(cx);
        if self.rack.snooze(id, until, SystemTime::now()) {
            self.save();
            self.after_change(cx);
        }
    }

    /// Brings a snoozed tile back now, from the list the moon opens.
    pub(crate) fn wake(&mut self, id: TileId, cx: &mut Context<Self>) {
        self.menu = None;
        if self.rack.wake(id, SystemTime::now()) {
            self.save();
            self.after_change(cx);
        }
    }

    /// The docked strip's marks: one per tile, and a moon while any tile is snoozed.
    pub(crate) fn strip_pips(&self) -> usize {
        self.rack.tiles().len() + usize::from(self.rack.snoozed().next().is_some())
    }

    /// The menu window tells the rack it closed itself.
    pub(crate) fn menu_closed(&mut self) {
        if let Some(menu) = self.menu.take() {
            self.menu_closed_at = Some((std::mem::discriminant(&menu.kind), Instant::now()));
        }
    }

    // The moon and the grip are buttons that open a menu, the moon's list of snoozed tiles and the
    // grip's the rack's menu, and a second click closes it (user, 2026-11-16). A press on the
    // button closes the open menu before its click lands, since a menu closes at any press
    // outside it, and depending on which sees the press first, the menu is still open at the
    // press or closed itself a moment before. Either way, the click that follows leaves it closed.
    pub(crate) fn menu_button_pressed(&mut self, kind: &MenuKind) {
        let kind = std::mem::discriminant(kind);
        let open = self
            .menu
            .as_ref()
            .is_some_and(|m| std::mem::discriminant(&m.kind) == kind);
        let just_closed = self
            .menu_closed_at
            .is_some_and(|(k, at)| k == kind && at.elapsed() < Duration::from_millis(300));
        self.menu_was_open = open || just_closed;
    }

    pub(crate) fn menu_button_clicked(
        &mut self,
        kind: MenuKind,
        at: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.suppress_click {
            return;
        }
        if std::mem::take(&mut self.menu_was_open) {
            self.close_menu(cx);
            return;
        }
        self.open_menu(kind, at, cx);
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = self.menu.take() {
            menu.window
                .update(cx, |_, window, _| window.remove_window())
                .ok();
        }
    }

    pub(crate) fn remove_tile(&mut self, id: TileId, cx: &mut Context<Self>) {
        if self.suppress_click {
            return;
        }
        self.take_off(id, cx);
    }

    // Takes a tile off the rack, with an Undo for your own tile and a dismissed session. A swipe
    // comes here directly, since a click left over from a drag of the rack has nothing to do with
    // it.
    fn take_off(&mut self, id: TileId, cx: &mut Context<Self>) {
        let Some(tile) = self.rack.tile(id) else {
            return;
        };
        // Your own tile and a dismissed session can come back with Undo, and an ended session's
        // tile, which has nothing left to show, goes without one.
        let title = tile.title();
        let label = match tile.session().map(|s| s.state) {
            None => Some(format!("Removed \u{201c}{title}\u{201d}")),
            Some(SessionState::Ended) => None,
            Some(_) => Some(format!("Dismissed \u{201c}{title}\u{201d}")),
        };
        let own = tile.is_own();
        let Some(removed) = self.rack.remove(id) else {
            return;
        };
        if let Some(title) = label {
            self.toast = Some(Toast {
                removed,
                title,
                until: Instant::now() + TOAST_FOR,
            });
        }
        if own {
            self.save();
        }
        self.after_change(cx);
    }

    pub(crate) fn undo_remove(&mut self, cx: &mut Context<Self>) {
        if let Some(toast) = self.toast.take() {
            // A tile swiped away comes back in place, not at the end of its slide.
            self.swipe = None;
            self.rack.restore(toast.removed);
            self.save();
            self.after_change(cx);
        }
    }

    // Opening the field is the one time the rack takes keyboard focus. Windows lets a process
    // bring its window forward only in certain cases, one being that it received the last input
    // event, which the click that opens the field is.
    // (documented, https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setforegroundwindow)
    // The shortcut for a new tile does what clicking the phantom tile does, from any app: it brings
    // the rack forward, slides it out of the dock if it's docked, and opens the new tile's field,
    // which takes keyboard focus until you press Enter or Esc (user, 2026-11-15).
    fn hotkey(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.docked.is_some() {
            self.undock(cx);
        }
        native::keep_on_top(self.hwnd);
        // A press left over from a drag would otherwise stop the field opening.
        self.suppress_click = false;
        self.start_field(FieldFor::New, window, cx);
    }

    pub(crate) fn start_field(
        &mut self,
        target: FieldFor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.suppress_click || self.field.is_some() {
            return;
        }
        let text = match target {
            FieldFor::New => String::new(),
            FieldFor::Rename(id) => self.rack.tile(id).map(|t| t.title()).unwrap_or_default(),
        };
        let hwnd = self.hwnd;
        self.field = Some(Field {
            target,
            prev_focus: native::foreground_window(),
            been_active: window.is_window_active(),
            remind: 0,
        });
        self.close_menu(cx);
        self.close_card(cx);
        native::allow_activation(hwnd, true);
        self.input.update(cx, |s, cx| {
            s.set_value(text, window, cx);
            s.focus(window, cx);
        });
        cx.spawn_in(window, async move |this, cx| {
            native::set_foreground(hwnd);
            this.update_in(cx, |this, window, cx| {
                this.input.update(cx, |s, cx| s.focus(window, cx));
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn finish_field(&mut self, ended: Ended, _: &mut Window, cx: &mut Context<Self>) {
        let Some(field) = self.field.take() else {
            return;
        };
        let text = self.input.read(cx).value().trim().to_string();
        if ended == Ended::Enter && !text.is_empty() {
            match field.target {
                FieldFor::New => {
                    let now = SystemTime::now();
                    let id = self.rack.add_own(&text, now);
                    if let Some(minutes) = field_reminder(field.remind) {
                        self.rack.start_timer(id, minutes, now);
                    }
                }
                FieldFor::Rename(id) => {
                    self.rack.rename_own(id, &text);
                }
            }
            self.save();
        }
        // After a click elsewhere, focus is already where you clicked, so nothing is handed back.
        let hand_back = ended != Ended::Deactivated;
        let (hwnd, prev) = (self.hwnd, field.prev_focus);
        cx.spawn(async move |_, _| {
            if hand_back {
                native::set_foreground(prev);
            }
            native::allow_activation(hwnd, false);
        })
        .detach();
        self.after_change(cx);
    }

    /// Steps the new tile's reminder to the next length, or the previous one if `back`, wrapping
    /// through none, and says whether a new tile's field was open to take it.
    pub(crate) fn step_reminder(&mut self, back: bool, cx: &mut Context<Self>) -> bool {
        let Some(field) = self.field.as_mut().filter(|f| f.target == FieldFor::New) else {
            return false;
        };
        let by = if back { FIELD_REMINDERS - 1 } else { 1 };
        field.remind = (field.remind + by) % FIELD_REMINDERS;
        cx.notify();
        true
    }

    /// The Enter hint beside the field, clicked: it adds or renames the tile, as Enter does.
    pub(crate) fn click_enter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_field(Ended::Enter, window, cx);
    }

    pub(crate) fn is_renaming(&self, id: TileId) -> bool {
        self.field
            .as_ref()
            .is_some_and(|f| f.target == FieldFor::Rename(id))
    }

    pub(crate) fn is_adding(&self) -> bool {
        self.field
            .as_ref()
            .is_some_and(|f| f.target == FieldFor::New)
    }
}
