// A throwaway GPUI app that tries the rack's window from record/areas/rack/01-window.md, drawn
// as the mockups draw it: borderless and on top with no taskbar entry, dragged and thrown with
// its position set every frame, docked to a strip, clicked without taking keyboard focus, and
// holding a text field that takes focus and hands it back.
//
// `cargo run --release` opens it for trying by hand, and right-clicking the grip at its top quits
// it and writes the report. `cargo run --release -- --selftest` throws, docks, and undocks it by
// itself without touching the mouse or keyboard, captures its look, and quits. `--style-only`
// leaves WM_MOUSEACTIVATE to GPUI, to see whether WS_EX_NOACTIVATE alone keeps clicks from
// activating the window. Each run writes its report to ../results/.

mod assets;
mod motion;
mod report;
mod view;

#[cfg(windows)]
mod native_windows;
#[cfg(windows)]
use native_windows as native;
#[cfg(not(windows))]
compile_error!("The window test runs on Windows so far, and its macOS half isn't written yet.");

use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

use gpui_kit::component::{
    Theme, ThemeMode,
    input::{InputEvent, InputState},
};
use gpui_kit::*;
use motion::{Ease, Rect, Side, Tween};
use native::MoveRecord;
use report::{Episode, FieldTest, Log};

const RACK_W: f32 = 304.;
const STRIP_W: f32 = 22.;

// The rack's height in the mockups: a 28px band, a 60px row for each tile and for the phantom
// tile with 6px between rows, 12px of padding below, and a 1px border.
fn rack_height(tiles: usize) -> f32 {
    102. + 66. * tiles as f32
}

fn strip_height(tiles: usize) -> f32 {
    14. + 16. * tiles.max(1) as f32
}

fn results_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("results")
}

fn captures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("captures")
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Working,
    YourTurn,
    Approval,
    Ended,
}

#[derive(Clone, Copy, PartialEq)]
enum Timer {
    Idle,
    // The share of the timer's time that's left, from 0 to 1.
    Running(f32),
    TimeUp,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Session(State),
    Own,
}

struct Tile {
    title: SharedString,
    kind: Kind,
    summary: Option<SharedString>,
    timer: Timer,
}

// The tiles the mockups' interactive prototype starts with.
fn sample_tiles() -> Vec<Tile> {
    let tile = |title: &str, kind, summary: Option<&str>, timer| Tile {
        title: title.to_string().into(),
        kind,
        summary: summary.map(|s| s.to_string().into()),
        timer,
    };
    vec![
        tile(
            "Stage 11b: app shells",
            Kind::Session(State::Working),
            None,
            Timer::Idle,
        ),
        tile(
            "Parse crash dumps in the log collector",
            Kind::Session(State::Approval),
            Some("Wants to run: cargo test -p collector"),
            Timer::Running(0.6),
        ),
        tile(
            "Donor page copy pass",
            Kind::Session(State::YourTurn),
            Some("Drafted three headlines. Which tone do you want?"),
            Timer::Running(0.4),
        ),
        tile(
            "Flight plan import",
            Kind::Session(State::Ended),
            None,
            Timer::Idle,
        ),
        tile("Check release CI", Kind::Own, None, Timer::Running(0.4)),
        tile("Waiting on review feedback", Kind::Own, None, Timer::Idle),
        tile(
            "Renew the domain before Friday",
            Kind::Own,
            None,
            Timer::TimeUp,
        ),
        tile(
            "Smoke-test the build when it lands",
            Kind::Own,
            None,
            Timer::Running(0.6),
        ),
    ]
}

#[derive(Clone, Copy)]
struct Press {
    cursor: (f64, f64),
    origin: (f64, f64),
}

struct Drag {
    grab: (f64, f64),
    samples: motion::Samples,
    hold: Option<(Side, Instant)>,
}

#[derive(Clone, Copy, PartialEq)]
enum Ended {
    Enter,
    Escape,
    Deactivated,
}

impl Ended {
    fn name(self) -> &'static str {
        match self {
            Ended::Enter => "Enter",
            Ended::Escape => "Esc",
            Ended::Deactivated => "a click elsewhere",
        }
    }
}

#[derive(Clone, Copy)]
enum After {
    Dock(Side),
    Settle,
}

struct Config {
    selftest: bool,
    style_only: bool,
    focus_before_open: isize,
}

struct SelfTest {
    phase: u8,
    since: Instant,
}

struct Rack {
    hwnd: isize,
    cfg: Config,
    tiles: Vec<Tile>,
    docked: Option<Side>,
    dock_area: Rect,
    dock_y: f64,
    peek: bool,
    press: Option<Press>,
    drag: Option<Drag>,
    wait_release: bool,
    suppress_click: bool,
    last_press_focus: isize,
    pos: (f64, f64),
    size: (f64, f64),
    vel: (f64, f64),
    tween: Option<(Tween, After)>,
    scale: f64,
    last_frame: Option<Instant>,
    adding: Option<isize>,
    input: Entity<InputState>,
    episode: Option<Episode>,
    moves: Rc<RefCell<Vec<MoveRecord>>>,
    log: Log,
    selftest: Option<SelfTest>,
    _subs: Vec<Subscription>,
}

impl Rack {
    fn new(cfg: Config, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let hwnd = native::hwnd_of(window);
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Name it"));
        let subs = vec![
            cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    this.finish_adding(Ended::Enter, window, cx);
                }
            }),
            cx.observe_keystrokes(|this, ev, window, cx| {
                if this.adding.is_some() && ev.keystroke.key == "escape" {
                    this.finish_adding(Ended::Escape, window, cx);
                }
            }),
            cx.observe_window_activation(window, |this, window, cx| {
                if this.adding.is_some() && !window.is_window_active() {
                    this.finish_adding(Ended::Deactivated, window, cx);
                }
            }),
        ];

        let log = Log {
            mode: if cfg.selftest { "self-test" } else { "by-hand" },
            answers_mouse_activate: !cfg.style_only,
            ..Default::default()
        };

        // The native setup resizes the window, and GPUI is still building it here, so the setup
        // waits for a task of its own.
        let (before, style_only, selftest) = (cfg.focus_before_open, cfg.style_only, cfg.selftest);
        cx.spawn(async move |this, cx| {
            let installed = native::install_subclass(hwnd, !style_only);
            native::set_noactivate(hwnd, true);
            // The self-test captures the window without DWM's shadow first, so the report shows
            // what the shadow adds.
            let shadow = (!selftest).then(|| native::dwm_shadow(hwnd, true));
            let (w, c) = (native::window_rect(hwnd), native::client_rect(hwnd));
            this.update(cx, |this, cx| {
                this.log.subclass = Some(installed);
                this.log.dwm_shadow = shadow;
                this.log.ex_style = native::ex_style(hwnd);
                this.log.frame = Some((
                    w.right - w.left,
                    w.bottom - w.top,
                    c.right - c.left,
                    c.bottom - c.top,
                ));
                this.refresh_rect();
                cx.notify();
            })
            .ok();
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;
            let now = native::foreground_window();
            this.update(cx, |this, cx| {
                this.log.focus_kept_on_open = Some(now == before && now != hwnd);
                cx.notify();
            })
            .ok();
        })
        .detach();

        let r = native::window_rect(hwnd);
        Self {
            hwnd,
            selftest: cfg.selftest.then(|| SelfTest {
                phase: 0,
                since: Instant::now(),
            }),
            cfg,
            tiles: sample_tiles(),
            docked: None,
            dock_area: Rect::default(),
            dock_y: 0.,
            peek: false,
            press: None,
            drag: None,
            wait_release: false,
            suppress_click: false,
            last_press_focus: 0,
            pos: (r.left, r.top),
            size: (r.right - r.left, r.bottom - r.top),
            vel: (0., 0.),
            tween: None,
            scale: window.scale_factor() as f64,
            last_frame: None,
            adding: None,
            input,
            episode: None,
            moves: Rc::new(RefCell::new(Vec::new())),
            log,
            _subs: subs,
        }
    }

    fn center(&self) -> (f64, f64) {
        (self.pos.0 + self.size.0 / 2., self.pos.1 + self.size.1 / 2.)
    }

    fn refresh_rect(&mut self) {
        let r = native::window_rect(self.hwnd);
        self.pos = (r.left, r.top);
        self.size = (r.right - r.left, r.bottom - r.top);
    }

    fn begin_episode(&mut self, kind: &'static str) {
        self.end_episode();
        let n = self.moves.borrow().len();
        self.episode = Some(Episode {
            kind,
            frames: Vec::new(),
            moves_from: n,
            moves_to: n,
        });
    }

    fn end_episode(&mut self) {
        if let Some(mut e) = self.episode.take() {
            e.moves_to = self.moves.borrow().len();
            self.log.episodes.push(e);
        }
    }

    // GPUI handles WM_MOVE by updating the window's state, and a SetWindowPos made while GPUI is
    // drawing would deliver that WM_MOVE into it, so the move runs in a foreground task, as GPUI's
    // own resize does. (read in gpui-pre-windows 0.3.12, src/window.rs, WindowsWindow::resize)
    fn move_window(&self, cx: &mut Context<Self>) {
        let (hwnd, x, y) = (
            self.hwnd,
            self.pos.0.round() as i32,
            self.pos.1.round() as i32,
        );
        let moves = self.moves.clone();
        cx.foreground_executor()
            .spawn(async move { moves.borrow_mut().push(native::move_window(hwnd, x, y)) })
            .detach();
    }

    fn set_bounds(&self, cx: &mut Context<Self>) {
        let hwnd = self.hwnd;
        let (x, y) = (self.pos.0.round() as i32, self.pos.1.round() as i32);
        let (w, h) = (self.size.0.round() as i32, self.size.1.round() as i32);
        cx.foreground_executor()
            .spawn(async move { native::set_bounds(hwnd, x, y, w, h) })
            .detach();
    }

    fn animating(&self) -> bool {
        self.press.is_some()
            || self.wait_release
            || self.vel != (0., 0.)
            || self.tween.is_some()
            || self.selftest.is_some()
    }

    fn on_press(&mut self, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.last_press_focus = native::foreground_window();
        self.suppress_click = false;
        if self.adding.is_some() || self.docked.is_some() || self.tween.is_some() {
            return;
        }
        self.refresh_rect();
        self.press = Some(Press {
            cursor: native::cursor_pos(),
            origin: self.pos,
        });
        // A press catches a rack that's still sliding.
        if self.vel != (0., 0.) {
            self.vel = (0., 0.);
            self.end_episode();
        }
        cx.notify();
    }

    fn step(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        let dt = self
            .last_frame
            .map_or(0., |t| (now - t).as_secs_f64())
            .min(0.05);
        self.last_frame = Some(now);
        if let Some(e) = &mut self.episode {
            e.frames.push(now);
        }
        let scale = self.scale;

        if let Some((tween, after)) = self.tween {
            let (p, done) = tween.at(now);
            self.pos = p;
            self.move_window(cx);
            if done {
                self.tween = None;
                self.end_episode();
                if let After::Dock(side) = after {
                    self.finish_dock(side, cx);
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
                self.begin_episode("drag");
            }
            if let Some(mut drag) = self.drag.take() {
                let want = (cursor.0 - drag.grab.0, cursor.1 - drag.grab.1);
                let (p, held) = motion::clamp_drag(want, self.size, &native::edges_at(cursor));
                self.pos = p;
                drag.hold = match (held, drag.hold) {
                    (Some(side), Some((was, since))) if side == was => Some((was, since)),
                    (Some(side), _) => Some((side, now)),
                    (None, _) => None,
                };
                drag.samples.push(now, want);
                self.move_window(cx);
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
                    self.end_episode();
                    if self.vel != (0., 0.) {
                        self.begin_episode("throw");
                    }
                }
            }
            return;
        }

        if self.vel != (0., 0.) && self.docked.is_none() {
            let edges = native::edges_at(self.center());
            let (p, v, moving) =
                motion::step_throw(self.pos, self.vel, self.size, &edges, dt, scale);
            self.pos = p;
            self.vel = v;
            self.move_window(cx);
            if !moving {
                self.end_episode();
            }
        }
    }

    fn throw(&mut self, v: (f64, f64), kind: &'static str) {
        self.refresh_rect();
        self.vel = (v.0 * self.scale, v.1 * self.scale);
        self.begin_episode(kind);
    }

    // The rack slides into the edge it was held against, and the strip takes its place.
    fn begin_dock(&mut self, side: Side) {
        self.press = None;
        self.drag = None;
        self.vel = (0., 0.);
        let a = native::edges_at(self.center()).area;
        self.dock_area = a;
        self.dock_y = self.pos.1;
        let to_x = match side {
            Side::Left => a.left - self.size.0,
            Side::Right => a.right,
        };
        self.begin_episode("slide into the dock");
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

    fn finish_dock(&mut self, side: Side, cx: &mut Context<Self>) {
        let s = self.scale;
        let a = self.dock_area;
        let (w, h) = (
            STRIP_W as f64 * s,
            strip_height(self.tiles.len()) as f64 * s,
        );
        let x = match side {
            Side::Left => a.left,
            Side::Right => a.right - w,
        };
        let y = (self.dock_y + 12. * s).clamp(a.top + 16. * s, (a.bottom - h - 16. * s).max(a.top));
        self.docked = Some(side);
        self.peek = false;
        self.pos = (x, y);
        self.size = (w, h);
        self.set_bounds(cx);
        native::set_round_corners(self.hwnd, false);
        self.log.docks += 1;
        cx.notify();
    }

    // The rack slides back out of the edge and stops flush with it.
    fn undock(&mut self, cx: &mut Context<Self>) {
        let Some(side) = self.docked.take() else {
            return;
        };
        let s = self.scale;
        let a = self.dock_area;
        let (w, h) = (RACK_W as f64 * s, rack_height(self.tiles.len()) as f64 * s);
        let y = (self.pos.1 - 12. * s).clamp(a.top, (a.bottom - h).max(a.top));
        let (from_x, to_x) = match side {
            Side::Left => (a.left - w, a.left),
            Side::Right => (a.right, a.right - w),
        };
        self.peek = false;
        self.pos = (from_x, y);
        self.size = (w, h);
        self.set_bounds(cx);
        native::set_round_corners(self.hwnd, true);
        self.begin_episode("slide out of the dock");
        self.tween = Some((
            Tween {
                from: (from_x, y),
                to: (to_x, y),
                start: Instant::now(),
                ease: Ease::Out,
            },
            After::Settle,
        ));
        self.log.undocks += 1;
        cx.notify();
    }

    fn set_peek(&mut self, on: bool, cx: &mut Context<Self>) {
        let Some(side) = self.docked else {
            return;
        };
        if self.peek == on || self.tween.is_some() {
            return;
        }
        self.peek = on;
        let w = (STRIP_W as f64 + if on { motion::PEEK } else { 0. }) * self.scale;
        self.pos.0 = match side {
            Side::Left => self.dock_area.left,
            Side::Right => self.dock_area.right - w,
        };
        self.size.0 = w;
        self.set_bounds(cx);
        cx.notify();
    }

    fn resize_rack(&mut self, cx: &mut Context<Self>) {
        self.refresh_rect();
        self.size.1 = rack_height(self.tiles.len()) as f64 * self.scale;
        self.set_bounds(cx);
    }

    fn check_click_focus(&mut self, cx: &mut Context<Self>) {
        let (before, hwnd) = (self.last_press_focus, self.hwnd);
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(150))
                .await;
            let now = native::foreground_window();
            this.update(cx, |this, cx| {
                this.log.clicks += 1;
                // While the inline field is open the rack has focus on purpose, so a click then
                // says nothing about whether clicks take focus.
                if before == hwnd {
                    this.log.clicks_with_rack_focused += 1;
                } else if now == before {
                    this.log.clicks_kept_focus += 1;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // Clicking a session tile would open the session, and clicking your own time-up tile clears
    // time-up. Every click is measured for whether it left keyboard focus where it was.
    fn click_main(&mut self, i: usize, cx: &mut Context<Self>) {
        if self.suppress_click {
            return;
        }
        self.check_click_focus(cx);
        let t = &mut self.tiles[i];
        if t.kind == Kind::Own && t.timer == Timer::TimeUp {
            t.timer = Timer::Idle;
        }
        cx.notify();
    }

    fn toggle_timer(&mut self, i: usize, cx: &mut Context<Self>) {
        if self.suppress_click {
            return;
        }
        self.check_click_focus(cx);
        let t = &mut self.tiles[i];
        t.timer = match t.timer {
            Timer::Running(_) => Timer::Idle,
            Timer::Idle | Timer::TimeUp => Timer::Running(1.),
        };
        cx.notify();
    }

    fn remove_tile(&mut self, i: usize, cx: &mut Context<Self>) {
        if self.suppress_click || i >= self.tiles.len() {
            return;
        }
        self.check_click_focus(cx);
        self.tiles.remove(i);
        self.resize_rack(cx);
        cx.notify();
    }

    // Opening the field is the one time the rack takes keyboard focus. Windows lets a process
    // bring its window forward only in certain cases, one being that it received the last input
    // event, which the click that opens the field is.
    // (documented, https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setforegroundwindow)
    fn start_adding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.suppress_click || self.adding.is_some() {
            return;
        }
        let (hwnd, prev) = (self.hwnd, native::foreground_window());
        self.adding = Some(prev);
        self.log.fields.push(FieldTest {
            took_focus: None,
            ended_by: "nothing yet",
            handed_back: None,
        });
        native::allow_activation(hwnd, true);
        self.input.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.focus(window, cx);
        });
        cx.spawn_in(window, async move |this, cx| {
            let asked = native::set_foreground(hwnd);
            cx.background_executor()
                .timer(Duration::from_millis(150))
                .await;
            let got = native::foreground_window() == hwnd;
            this.update_in(cx, |this, window, cx| {
                if let Some(f) = this.log.fields.last_mut() {
                    f.took_focus = Some(asked && got);
                }
                this.input.update(cx, |s, cx| s.focus(window, cx));
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn finish_adding(&mut self, ended: Ended, _: &mut Window, cx: &mut Context<Self>) {
        let Some(prev) = self.adding.take() else {
            return;
        };
        let text = self.input.read(cx).value().trim().to_string();
        if ended == Ended::Enter && !text.is_empty() {
            self.tiles.push(Tile {
                title: text.into(),
                kind: Kind::Own,
                summary: None,
                timer: Timer::Idle,
            });
            self.resize_rack(cx);
        }
        if let Some(f) = self.log.fields.last_mut() {
            f.ended_by = ended.name();
        }
        // After a click elsewhere, focus is already where you clicked, so nothing is handed back.
        let hand_back = ended != Ended::Deactivated;
        let hwnd = self.hwnd;
        cx.spawn(async move |this, cx| {
            if hand_back {
                native::set_foreground(prev);
            }
            native::allow_activation(hwnd, false);
            cx.background_executor()
                .timer(Duration::from_millis(150))
                .await;
            let now = native::foreground_window();
            this.update(cx, |this, cx| {
                if hand_back && let Some(f) = this.log.fields.last_mut() {
                    f.handed_back = Some(now == prev);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn finish(&mut self, cx: &mut Context<Self>) {
        self.end_episode();
        let moves = self.moves.borrow();
        let text = report::write(&self.log, &moves, self.center(), self.scale);
        let (date, time) = report::stamp();
        let mode = if self.cfg.selftest {
            "selftest"
        } else {
            "by-hand"
        };
        let path = results_dir().join(format!("{date}-windows-{mode}-{time}.txt"));
        let _ = std::fs::create_dir_all(results_dir());
        match std::fs::write(&path, &text) {
            Ok(()) => println!("{text}\nWrote {}", path.display()),
            Err(e) => println!("{text}\nCouldn't write {}: {e}", path.display()),
        }
        cx.quit();
    }

    fn capture(&mut self, name: &str) {
        let _ = std::fs::create_dir_all(captures_dir());
        let path = captures_dir().join(format!("{name}.png"));
        let r = native::window_rect(self.hwnd);
        let line = match native::capture(r, 32, &path) {
            Ok(()) => format!("{name}, saved beside the build in target/captures/{name}.png"),
            Err(e) => format!("{name} failed: {e}"),
        };
        self.log.captures.push(line);
    }

    // The self-test moves the rack the way a person would, but without input, so it measures the
    // window's motion and leaves your mouse and keyboard alone.
    fn run_selftest(&mut self, cx: &mut Context<Self>) {
        let Some(st) = &self.selftest else {
            return;
        };
        let (phase, t) = (st.phase, st.since.elapsed());
        let still = self.vel == (0., 0.) && self.tween.is_none();
        let next = match phase {
            0 if t > Duration::from_millis(1200) => {
                self.capture("rack");
                self.log.dwm_shadow = Some(native::dwm_shadow(self.hwnd, true));
                true
            }
            1 if t > Duration::from_millis(600) => {
                self.capture("rack-with-dwm-shadow");
                self.throw((-6000., -1500.), "fast throw up and to the left");
                true
            }
            2 if still && t > Duration::from_millis(300) => {
                self.throw((8000., 2500.), "fast throw down and to the right");
                true
            }
            3 if still && t > Duration::from_millis(300) => {
                self.refresh_rect();
                self.begin_dock(Side::Right);
                true
            }
            4 if self.docked.is_some() && t > Duration::from_millis(800) => {
                self.capture("docked-strip");
                self.undock(cx);
                true
            }
            // The field is shown without taking focus, which only a click or the hotkey may do.
            5 if still && t > Duration::from_millis(400) => {
                self.adding = Some(native::foreground_window());
                true
            }
            6 if t > Duration::from_millis(300) => {
                self.capture("quick-add-field");
                self.adding = None;
                true
            }
            7 if t > Duration::from_millis(200) => {
                self.selftest = None;
                self.finish(cx);
                false
            }
            _ => false,
        };
        if next && let Some(st) = &mut self.selftest {
            st.phase += 1;
            st.since = Instant::now();
        }
    }
}

impl Render for Rack {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.scale = window.scale_factor() as f64;
        self.step(cx);
        self.run_selftest(cx);
        if self.animating() {
            window.request_animation_frame();
        } else {
            self.last_frame = None;
        }
        match self.docked {
            Some(side) => self.render_strip(side, cx),
            None => self.render_rack(cx),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let selftest = args.iter().any(|a| a == "--selftest");
    let style_only = args.iter().any(|a| a == "--style-only");
    let focus_before_open = native::foreground_window();

    gpui_kit::application()
        .with_assets(assets::Assets)
        .run(move |cx: &mut App| {
            gpui_kit::init(cx);
            Theme::change(ThemeMode::Dark, None, cx);
            // gpui-component's Root fills the whole window with the theme's background, which
            // would show as dark squares outside the rack's rounded corners.
            // (read in gpui-component 0.9.1, src/root.rs, the Root plugin's style)
            cx.global_mut::<Theme>().tokens.background = transparent_black().into();
            let display = cx.primary_display().expect("there should be a display");
            let db = display.bounds();
            let size = size(px(RACK_W), px(rack_height(sample_tiles().len())));
            let origin = point(db.right() - px(RACK_W + 48.), db.top() + px(96.));
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(origin, size))),
                titlebar: None,
                focus: false,
                show: true,
                kind: WindowKind::PopUp,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                // The rack is never the active window, and GPUI caps an inactive window at about
                // 30 frames a second by default, which would make every throw stutter.
                inactive_frame_interval: None,
                low_power_frame_interval: None,
                thermal_frame_interval: None,
                app_id: Some("plates-window-test".into()),
                ..Default::default()
            };
            let cfg = Config {
                selftest,
                style_only,
                focus_before_open,
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| Rack::new(cfg, window, cx))
            })
            .expect("the rack's window should open");
        });
}
