// A throwaway GPUI app that tries the rack's window from record/areas/rack/01-window.md:
// borderless and on top with no taskbar entry, dragged and thrown with its position set every
// frame, docked to a strip, clicked without taking keyboard focus, and holding a text field that
// takes focus and hands it back.
//
// `cargo run --release` opens it for trying by hand, and the × in its top band quits it and
// writes the report. `cargo run --release -- --selftest` throws, docks, and undocks it by itself
// without touching the mouse or keyboard, captures its look, and quits. `--style-only` leaves
// WM_MOUSEACTIVATE to GPUI, to see whether WS_EX_NOACTIVATE alone keeps clicks from activating
// the window. Each run writes its report to ../results/.

mod motion;
mod report;

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
    input::{Input, InputEvent, InputState},
};
use gpui_kit::*;
use motion::Side;
use native::MoveRecord;
use report::{Episode, FieldTest, Log};

const RACK_W: f32 = 304.;
const BAND_H: f32 = 28.;
const TILE_H: f32 = 60.;
const GAP: f32 = 6.;
const PAD: f32 = 12.;
const READOUT_H: f32 = 72.;
const STRIP_W: f32 = 22.;

const BG: u32 = 0x131417;
const BORDER: u32 = 0x2A2C31;
const TILE: u32 = 0x1C1D21;
const TILE_HOVER: u32 = 0x222328;
const TEXT: u32 = 0xECEDEF;
const TEXT2: u32 = 0xA3A6AE;
const BLUE: u32 = 0x74AEFF;
const AMBER: u32 = 0xF2B441;
const OWN: u32 = 0x5E626B;

fn rack_height(items: usize) -> f32 {
    BAND_H + items as f32 * TILE_H + items.saturating_sub(1) as f32 * GAP + GAP + READOUT_H + PAD
}

fn strip_height(tiles: usize) -> f32 {
    20. + tiles.max(1) as f32 * 16. - 8.
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

#[derive(Clone, Copy)]
enum Marker {
    Circle(u32),
    Square(u32),
}

struct Tile {
    title: SharedString,
    marker: Marker,
    clicks: u32,
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
    press: Option<Press>,
    drag: Option<Drag>,
    wait_release: bool,
    suppress_click: bool,
    last_press_focus: isize,
    pos: (f64, f64),
    size: (f64, f64),
    vel: (f64, f64),
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
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("New tile"));
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
            // The self-test captures the window as GPUI draws it before squaring off DWM's
            // corners and border, so the report shows what each does.
            let squared = (!selftest).then(|| native::square_off(hwnd));
            let (w, c) = (native::window_rect(hwnd), native::client_rect(hwnd));
            this.update(cx, |this, cx| {
                this.log.subclass = Some(installed);
                this.log.squared_off = squared;
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
            tiles: vec![
                Tile {
                    title: "Working session".into(),
                    marker: Marker::Circle(BLUE),
                    clicks: 0,
                },
                Tile {
                    title: "Your turn".into(),
                    marker: Marker::Circle(AMBER),
                    clicks: 0,
                },
                Tile {
                    title: "Your own tile".into(),
                    marker: Marker::Square(OWN),
                    clicks: 0,
                },
            ],
            docked: None,
            press: None,
            drag: None,
            wait_release: false,
            suppress_click: false,
            last_press_focus: 0,
            pos: (r.left, r.top),
            size: (r.right - r.left, r.bottom - r.top),
            vel: (0., 0.),
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
        self.press.is_some() || self.wait_release || self.vel != (0., 0.) || self.selftest.is_some()
    }

    fn on_press(&mut self, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.last_press_focus = native::foreground_window();
        self.suppress_click = false;
        if self.adding.is_some() || self.docked.is_some() {
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
                drag.samples.push(now, p);
                self.move_window(cx);
                let docks = drag.hold.filter(|(_, since)| now - *since >= motion::HOLD);
                self.drag = Some(drag);
                if let Some((side, _)) = docks {
                    self.dock(side, cx);
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

    fn dock(&mut self, side: Side, cx: &mut Context<Self>) {
        let s = self.scale;
        let edges = native::edges_at(self.center());
        let a = edges.area;
        let (w, h) = (
            STRIP_W as f64 * s,
            strip_height(self.tiles.len()) as f64 * s,
        );
        let x = match side {
            Side::Left => a.left,
            Side::Right => a.right - w,
        };
        let y = (self.pos.1 + 12. * s).clamp(a.top + 16. * s, (a.bottom - h - 16. * s).max(a.top));
        self.press = None;
        self.drag = None;
        self.vel = (0., 0.);
        self.end_episode();
        self.docked = Some(side);
        self.pos = (x, y);
        self.size = (w, h);
        self.set_bounds(cx);
        self.log.docks += 1;
        cx.notify();
    }

    fn undock(&mut self, cx: &mut Context<Self>) {
        let Some(side) = self.docked.take() else {
            return;
        };
        let s = self.scale;
        let a = native::edges_at(self.center()).area;
        let (w, h) = (
            RACK_W as f64 * s,
            rack_height(self.tiles.len() + 1) as f64 * s,
        );
        let y = (self.pos.1 - 12. * s).clamp(a.top, (a.bottom - h).max(a.top));
        let (x, vx) = match side {
            Side::Left => (a.left, motion::UNDOCK_SPEED * s),
            Side::Right => (a.right - w, -motion::UNDOCK_SPEED * s),
        };
        self.pos = (x, y);
        self.size = (w, h);
        self.set_bounds(cx);
        self.vel = (vx, 0.);
        self.begin_episode("slide out of the dock");
        self.log.undocks += 1;
        cx.notify();
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
                if now == before && now != hwnd {
                    this.log.clicks_kept_focus += 1;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn click_tile(&mut self, i: usize, cx: &mut Context<Self>) {
        if self.suppress_click {
            return;
        }
        self.tiles[i].clicks += 1;
        self.check_click_focus(cx);
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
                marker: Marker::Square(OWN),
                clicks: 0,
            });
            self.refresh_rect();
            self.size.1 = rack_height(self.tiles.len() + 1) as f64 * self.scale;
            self.set_bounds(cx);
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
        let line = match native::capture(r, 12, &path) {
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
        let still = self.vel == (0., 0.);
        let next = match phase {
            0 if t > Duration::from_millis(1200) => {
                self.capture("as-gpui-draws-it");
                self.log.squared_off = Some(native::square_off(self.hwnd));
                true
            }
            1 if t > Duration::from_millis(600) => {
                self.capture("dwm-corners-and-border-off");
                self.throw((-2400., -700.), "throw up and to the left");
                true
            }
            2 if still && t > Duration::from_millis(300) => {
                self.throw((3200., 600.), "throw into the right edge, with a bounce");
                true
            }
            3 if still && t > Duration::from_millis(300) => {
                self.refresh_rect();
                self.dock(Side::Right, cx);
                true
            }
            4 if t > Duration::from_millis(800) => {
                self.capture("docked-strip");
                self.undock(cx);
                true
            }
            5 if still && t > Duration::from_millis(300) => {
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

    fn marker(m: Marker) -> Div {
        match m {
            Marker::Circle(c) => div().size(px(8.)).rounded_full().bg(rgb(c)),
            Marker::Square(c) => div().size(px(8.)).rounded(px(2.)).bg(rgb(c)),
        }
    }

    fn readout(&self) -> Div {
        let moves = self.moves.borrow();
        let last = self
            .log
            .episodes
            .last()
            .map(|e| report::episode_line(e, &moves, 0))
            .map(|l| l.split(';').take(2).collect::<Vec<_>>().join(";"))
            .unwrap_or_else(|| "Drag or throw the rack".into());
        let fields = &self.log.fields;
        let took = fields.iter().filter(|f| f.took_focus == Some(true)).count();
        let back = fields
            .iter()
            .filter(|f| f.handed_back == Some(true))
            .count();
        let backs = fields.iter().filter(|f| f.handed_back.is_some()).count();
        let loops = native::OS_MOVE_LOOPS.load(std::sync::atomic::Ordering::Relaxed);
        div()
            .absolute()
            .left(px(PAD))
            .right(px(PAD))
            .bottom(px(PAD))
            .h(px(READOUT_H))
            .flex()
            .flex_col()
            .gap(px(2.))
            .text_size(px(11.))
            .text_color(rgb(TEXT2))
            .child(last)
            .child(format!(
                "Clicks {}, focus stayed put after {}",
                self.log.clicks, self.log.clicks_kept_focus
            ))
            .child(format!(
                "Field took focus {took} of {}, handed it back {back} of {backs}",
                fields.len()
            ))
            .child(format!("OS move loops {loops}, docked {}", self.log.docks))
    }

    fn render_rack(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let now = Instant::now();
        let hold = self
            .drag
            .as_ref()
            .and_then(|d| d.hold)
            .map(|(side, since)| (side, ((now - since).as_secs_f32() / 1.5).min(1.)));
        let tiles = self.tiles.iter().enumerate().map(|(i, t)| {
            let clicks = report::plural(t.clicks as usize, "click", "clicks");
            div()
                .id(("tile", i))
                .h(px(TILE_H))
                .flex()
                .items_center()
                .gap(px(10.))
                .px(px(12.))
                .rounded(px(10.))
                .bg(rgb(TILE))
                .hover(|s| s.bg(rgb(TILE_HOVER)))
                .child(Self::marker(t.marker))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(13.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(TEXT))
                                .child(t.title.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(rgb(TEXT2))
                                .child(clicks),
                        ),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.click_tile(i, cx)))
                .into_any_element()
        });
        let last = if self.adding.is_some() {
            div()
                .h(px(TILE_H))
                .flex()
                .items_center()
                .px(px(8.))
                .rounded(px(10.))
                .bg(rgb(TILE))
                .child(Input::new(&self.input))
                .into_any_element()
        } else {
            div()
                .id("phantom")
                .h(px(TILE_H))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .rounded(px(10.))
                .border_1()
                .border_dashed()
                .border_color(rgb(BORDER))
                .text_color(rgb(TEXT2))
                .child("+")
                .child(div().text_size(px(11.)).child("Ctrl + Shift + Space"))
                .on_click(
                    cx.listener(|this, _: &ClickEvent, window, cx| this.start_adding(window, cx)),
                )
                .into_any_element()
        };
        let grip = div()
            .flex()
            .gap(px(4.))
            .children((0..6).map(|_| div().size(px(3.)).rounded_full().bg(rgba(0xA3A6AE66))));
        let quit = div()
            .id("quit")
            .absolute()
            .right(px(10.))
            .top(px(5.))
            .text_size(px(13.))
            .text_color(rgb(TEXT2))
            .hover(|s| s.text_color(rgb(TEXT)))
            .child("×")
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                if !this.suppress_click {
                    this.finish(cx);
                }
            }));
        div()
            .size_full()
            .relative()
            .bg(rgb(BG))
            .border_1()
            .border_color(rgb(BORDER))
            .rounded(px(14.))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_press))
            .child(
                div()
                    .h(px(BAND_H))
                    .flex()
                    .items_center()
                    .justify_center()
                    .relative()
                    .child(grip)
                    .child(quit),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(GAP))
                    .px(px(PAD))
                    .children(tiles)
                    .child(last),
            )
            .child(self.readout())
            .children(hold.map(|(side, p)| {
                let bar = div()
                    .absolute()
                    .top_0()
                    .w(px(3.))
                    .h(relative(p))
                    .bg(rgb(BLUE));
                match side {
                    Side::Left => bar.left_0(),
                    Side::Right => bar.right_0(),
                }
            }))
            .into_any_element()
    }

    fn render_strip(&mut self, side: Side, cx: &mut Context<Self>) -> AnyElement {
        let strip = div()
            .id("strip")
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.))
            .py(px(10.))
            .bg(rgb(BG))
            .border_1()
            .border_color(rgb(BORDER))
            .children(self.tiles.iter().map(|t| Self::marker(t.marker)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, _| {
                    this.last_press_focus = native::foreground_window();
                }),
            )
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.check_click_focus(cx);
                this.undock(cx);
            }));
        match side {
            Side::Left => strip.rounded_r(px(8.)),
            Side::Right => strip.rounded_l(px(8.)),
        }
        .into_any_element()
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
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx: &mut App| {
            gpui_kit::init(cx);
            Theme::change(ThemeMode::Dark, None, cx);
            // gpui-component's Root fills the whole window with the theme's background, which
            // would show as dark squares outside the rack's rounded corners.
            // (read in gpui-component 0.9.1, src/root.rs, the Root plugin's style)
            cx.global_mut::<Theme>().tokens.background = transparent_black().into();
            let display = cx.primary_display().expect("there should be a display");
            let db = display.bounds();
            let size = size(px(RACK_W), px(rack_height(4)));
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
