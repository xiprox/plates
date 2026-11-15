//! The rack: Plates' one window, which holds the core's tiles, and moves, docks, and takes clicks
//! as record/areas/rack/01-window.md designs it. view.rs draws it.

use std::{
    path::PathBuf,
    time::{Duration, Instant, SystemTime},
};

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use plates_core::{
    Rack, Removed, TileId, desktop,
    hooks::{self, HookMessage},
    store,
    timer::DEFAULT_MINUTES,
};

use crate::{
    menu::{MENU_W, MenuKind, MenuView},
    motion::{self, Ease, Rect, Side, Tween},
    native,
    place::{self, Place},
    rows::Rows,
};

pub const RACK_W: f32 = 304.;
pub const STRIP_W: f32 = 22.;
pub const POPOVER_W: f32 = 176.;
pub const POPOVER_GAP: f32 = 12.;
pub const TOAST_H: f32 = 44.;
const TOAST_FOR: Duration = Duration::from_secs(5);

/// The rack's height: a 28px band, a 60px row for each tile and for the phantom tile with 6px
/// between rows, the Undo row while it shows, 12px of padding below, and a 1px border.
pub fn rack_height(tiles: usize, toast: bool) -> f32 {
    rack_height_for(tiles as f32, if toast { 1. } else { 0. })
}

/// The rack's height while rows grow and shrink, with each row counted by how far it has grown.
pub fn rack_height_for(tile_rows: f32, undo_row: f32) -> f32 {
    102. + 66. * tile_rows + (TOAST_H + 6.) * undo_row
}

pub fn strip_height(tiles: usize) -> f32 {
    14. + 16. * tiles.max(1) as f32
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
}

/// The menu of timer lengths, open in a window of its own.
pub(crate) struct Menu {
    window: WindowHandle<MenuView>,
}

pub(crate) struct Toast {
    pub(crate) removed: Removed,
    pub(crate) title: String,
    until: Instant,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ended {
    Enter,
    Escape,
    Deactivated,
}

pub struct RackView {
    pub(crate) hwnd: isize,
    pub(crate) rack: Rack,
    data_file: PathBuf,
    place_file: PathBuf,
    /// The place last saved, which is `None` until the window has taken its saved place, so the
    /// window's first position isn't saved over it.
    saved_place: Option<Place>,
    pub(crate) rows: Rows,
    pub(crate) input: Entity<InputState>,
    pub(crate) field: Option<Field>,
    pub(crate) menu: Option<Menu>,
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
    _subs: Vec<Subscription>,
    _tasks: Vec<Task<()>>,
}

impl RackView {
    pub fn new(
        rack: Rack,
        data_file: PathBuf,
        events: async_channel::Receiver<HookMessage>,
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
            cx.observe_window_activation(window, |this, window, cx| {
                if this.field.is_some() && !window.is_window_active() {
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
                if let Some(saved) = place::load(&this.place_file) {
                    this.restore_place(saved);
                }
                this.saved_place = Some(this.current_place());
                this.applied = None;
                cx.notify();
            })
            .ok();
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
        let tick = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                    break;
                }
            }
        });

        let r = native::window_rect(hwnd);
        let place_file = data_file.with_file_name("place.json");
        let mut view = RackView {
            hwnd,
            rack,
            data_file,
            place_file,
            saved_place: None,
            rows: Rows::default(),
            input,
            field: None,
            menu: None,
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
            scale: window.scale_factor() as f64,
            last_frame: None,
            applied: None,
            _subs: subs,
            _tasks: vec![take_over, receive, tick],
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
        if self.rack.tick(now) {
            self.save();
            self.after_change(cx);
        }
        if self
            .toast
            .as_ref()
            .is_some_and(|t| t.until <= Instant::now())
        {
            self.toast = None;
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
        if self.animating() || self.saved_place.is_none() {
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
                let strip_h = strip_height(self.rack.tiles().len()) as f64 * self.scale;
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
        let strip = strip_height(self.rack.tiles().len());
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

    pub(crate) fn on_press(&mut self, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.last_press_focus = native::foreground_window();
        self.suppress_click = false;
        if self.field.is_some() || self.docked.is_some() || self.tween.is_some() {
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
        let h = strip_height(self.rack.tiles().len()) as f64 * s;
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

    pub(crate) fn set_peek(&mut self, on: bool, cx: &mut Context<Self>) {
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
        // A session the Claude desktop app keeps opens there by the app's own ID, and a session
        // it doesn't, such as one in a terminal, has nowhere to open.
        if let Some(link) = tile
            .session()
            .and_then(|s| s.app_id.as_deref())
            .and_then(desktop::session_link)
            && !native::open_url(&link)
        {
            eprintln!("Couldn't open {link}");
        }
        if tile.timer.is_time_up() {
            self.rack.clear_time_up(id, SystemTime::now());
            self.save();
            self.after_change(cx);
        }
    }

    pub(crate) fn click_timer(&mut self, id: TileId, cx: &mut Context<Self>) {
        if self.suppress_click {
            return;
        }
        let now = SystemTime::now();
        match self.rack.tile(id).map(|t| t.timer.is_running()) {
            Some(true) => self.rack.cancel_timer(id, now),
            Some(false) => self.rack.start_timer(id, DEFAULT_MINUTES, now),
            None => return,
        }
        self.save();
        self.after_change(cx);
    }

    // The menu opens in a window of its own at the pointer, so it can reach past the rack's edges
    // without growing the rack's window, whose shadow DWM would draw around the grown rectangle.
    // It opens below and to the left of the pointer, and above it near the screen's bottom.
    pub(crate) fn open_menu(&mut self, kind: MenuKind, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.close_menu(cx);
        let s = self.scale;
        let r = native::window_rect(self.hwnd);
        let pointer = (
            r.left + f64::from(f32::from(at.x)) * s,
            r.top + f64::from(f32::from(at.y)) * s,
        );
        let area = native::edges_at(pointer).area;
        let menu_h = kind.height();
        let (w, h) = (MENU_W as f64 * s, menu_h as f64 * s);
        let x = (pointer.0 - w + 12. * s).clamp(area.left, (area.right - w).max(area.left));
        let below = pointer.1 + 6. * s;
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
                size(px(MENU_W), px(menu_h)),
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
        match cx.open_window(options, |window, cx| {
            cx.new(|cx| MenuView::new(rack, kind, bounds, window, cx))
        }) {
            Ok(window) => self.menu = Some(Menu { window }),
            Err(e) => eprintln!("Couldn't open a menu: {e}"),
        }
    }

    pub(crate) fn pick_minutes(&mut self, id: TileId, minutes: u32, cx: &mut Context<Self>) {
        self.menu = None;
        self.rack.start_timer(id, minutes, SystemTime::now());
        self.save();
        self.after_change(cx);
    }

    /// The menu window tells the rack it closed itself.
    pub(crate) fn menu_closed(&mut self) {
        self.menu = None;
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
        let title = self.rack.tile(id).map(|t| t.title()).unwrap_or_default();
        let Some(removed) = self.rack.remove(id) else {
            return;
        };
        if removed.tile.is_own() {
            self.toast = Some(Toast {
                removed,
                title,
                until: Instant::now() + TOAST_FOR,
            });
            self.save();
        }
        self.after_change(cx);
    }

    pub(crate) fn undo_remove(&mut self, cx: &mut Context<Self>) {
        if let Some(toast) = self.toast.take() {
            self.rack.restore(toast.removed);
            self.save();
            self.after_change(cx);
        }
    }

    // Opening the field is the one time the rack takes keyboard focus. Windows lets a process
    // bring its window forward only in certain cases, one being that it received the last input
    // event, which the click that opens the field is.
    // (documented, https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setforegroundwindow)
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
        });
        self.close_menu(cx);
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
                    self.rack.add_own(&text, SystemTime::now());
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
