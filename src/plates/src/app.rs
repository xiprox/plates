//! Starts Plates: loads your own tiles, starts the receiver for Claude Code's hook events, and
//! opens the rack's window.

use std::{
    borrow::Cow,
    collections::HashMap,
    path::PathBuf,
    sync::mpsc::{self, RecvTimeoutError},
    thread,
    time::{Duration, Instant, SystemTime},
};

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;
use plates_core::{
    Rack, SessionEvent,
    cuts::Cuts,
    desktop::{self, AppSessions},
    hooks::{self, HookMessage},
    receiver::{self, PORT, Received, Receiver},
    store, transcript,
};

use crate::{
    assets::Assets,
    rack_view::{RACK_W, RackView, rack_height},
    showcase, tray,
    view::SANS_FACES,
};

/// How often the events thread reads the transcripts of sessions in a turn, which is how long an
/// interrupt or a denial can take to reach a tile.
const POLL: Duration = Duration::from_secs(1);

/// Plates' data folder, `%APPDATA%\Plates\` on Windows and
/// `~/Library/Application Support/Plates/` on macOS, as record/areas/rack/02-tiles.md gives it.
pub fn data_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Plates")
}

// Opening a session in the Claude desktop app fires no hook event, but the app saves the time in
// its file for the session, so the events thread reads each session's file once a second and
// reports a look that's newer than the last it reported.
fn new_looks(
    looks: &mut HashMap<String, (PathBuf, Option<SystemTime>)>,
    app_sessions: &mut AppSessions,
) -> Vec<HookMessage> {
    let mut found = Vec::new();
    for (session_id, (cwd, last)) in looks.iter_mut() {
        let Some(at) = app_sessions.get(session_id).and_then(|a| a.focused_at) else {
            continue;
        };
        if *last != Some(at) {
            *last = Some(at);
            found.push(HookMessage {
                hook: "AppFile".to_string(),
                session_id: session_id.clone(),
                cwd: cwd.clone(),
                transcript_path: None,
                event: Some(SessionEvent::Seen { at }),
                status: None,
                app: None,
            });
        }
    }
    found
}

pub fn run() {
    // Only one Plates can hold the port its hooks post to, so a second one, such as one opened
    // from the Start menu while Start at login already opened the first, would show a rack that
    // never gets a session's tile. It asks the running Plates to show its rack instead, and quits
    // before reading or writing any of the data files.
    let (tx, rx) = mpsc::channel();
    let receiver = Receiver::start(PORT, tx, true);
    if receiver.is_err() && receiver::ask_to_show(PORT, Duration::from_millis(500)) {
        return;
    }

    let data_file = data_dir().join("rack.json");
    let mut rack = match store::load(&data_file) {
        Ok(loaded) => {
            if let Some(broken) = loaded.broken {
                eprintln!(
                    "{} didn't load, so it was moved to {}.",
                    data_file.display(),
                    broken.display()
                );
            }
            loaded.rack
        }
        Err(e) => {
            eprintln!("Couldn't read {}: {e}", data_file.display());
            Rack::new()
        }
    };
    // The session tiles from before Plates last closed come back, caught up with any turn that
    // ended or began while it was closed.
    let sessions_file = data_dir().join("sessions.json");
    match store::load_sessions(&mut rack, &sessions_file, SystemTime::now()) {
        Ok(Some(broken)) => eprintln!(
            "{} didn't load, so it was moved to {}.",
            sessions_file.display(),
            broken.display()
        ),
        Ok(None) => {}
        Err(e) => eprintln!("Couldn't read {}: {e}", sessions_file.display()),
    }
    hooks::catch_up(&mut rack);
    let restored: Vec<(String, PathBuf)> = rack
        .session_tiles()
        .filter_map(|(t, _)| t.session())
        .map(|s| (s.id.clone(), s.folder.clone()))
        .collect();

    // The receiver hands events on from a thread of its own, and the rack takes them on the UI
    // thread, so a thread between them moves each one onto a channel the UI can await. That
    // thread also reads each tool event's transcript for the session's status line, the desktop
    // app's file for the session's title and ID, and, once a second, the transcripts of sessions in a
    // turn for an interrupt or a denial, so none of these reads holds up the rack. A request to
    // show the rack goes onto the channel the tray's menu uses.
    if let Err(e) = &receiver {
        eprintln!("Couldn't listen on port {PORT}, so no session tiles will show: {e}");
    }
    let (events_tx, events) = async_channel::unbounded();
    let (commands_tx, commands) = async_channel::unbounded();
    let show = commands_tx.clone();
    thread::Builder::new()
        .name("plates-events".into())
        .spawn(move || {
            let mut app_sessions = AppSessions::new(desktop::sessions_folder());
            let mut cuts = Cuts::new();
            // Each session Plates has heard from, or restored, with its folder and the last time
            // the app says you opened it.
            let mut looks: HashMap<String, (PathBuf, Option<SystemTime>)> = restored
                .into_iter()
                .map(|(id, folder)| (id, (folder, None)))
                .collect();
            let mut polled = Instant::now();
            loop {
                let mut messages = Vec::new();
                match rx.recv_timeout(POLL) {
                    Ok(Received::Event(message, _)) => {
                        cuts.saw(&message);
                        looks
                            .entry(message.session_id.clone())
                            .or_insert_with(|| (message.cwd.clone(), None));
                        messages.push(*message);
                    }
                    Ok(Received::Show) => {
                        if show.send_blocking(tray::Command::Show).is_err() {
                            return;
                        }
                    }
                    Ok(Received::Unreadable(..)) | Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
                if polled.elapsed() >= POLL {
                    polled = Instant::now();
                    messages.extend(cuts.poll());
                    messages.extend(new_looks(&mut looks, &mut app_sessions));
                }
                for mut message in messages {
                    message.app = app_sessions.get(&message.session_id);
                    if hooks::wants_status(&message.hook)
                        && let Some(path) = &message.transcript_path
                    {
                        message.status = transcript::latest_status(path).ok().flatten();
                    }
                    if events_tx.send_blocking(message).is_err() {
                        return;
                    }
                }
            }
        })
        .expect("the events thread should start");

    application().with_assets(Assets).run(move |cx: &mut App| {
        // The receiver's thread runs for the life of the app.
        let _receiver = receiver;
        open_rack(rack, data_file, events, (commands_tx, commands), false, cx);
    });
}

/// Opens the rack on the made-up tiles of `showcase::rack`, which receives no hook events and
/// writes none of Plates' data files, and with `capture`, saves a picture of it and quits.
pub fn showcase(capture: Option<PathBuf>) {
    crate::view::STILL.store(true, std::sync::atomic::Ordering::Relaxed);
    let (rack, card_tile) = showcase::rack(SystemTime::now());
    let (_events_tx, events) = async_channel::unbounded();
    let (commands_tx, commands) = async_channel::unbounded();
    application().with_assets(Assets).run(move |cx: &mut App| {
        let data_file = data_dir().join("rack.json");
        let (window, view) = open_rack(rack, data_file, events, (commands_tx, commands), true, cx);
        if let Some(path) = capture {
            save_picture(window, view.downgrade(), card_tile, path, cx);
        }
    });
}

/// How long the showcase waits for the tiles to grow in before it opens the card, and then for
/// the card to grow before the picture.
#[cfg(feature = "capture")]
const SETTLE: Duration = Duration::from_millis(1000);

// GPUI draws a window's last frame again into an image of its own, at the screen's scale and with
// the transparent corners left clear, so the picture needs no permission to record the screen and
// shows nothing behind the rack. That drawing comes with GPUI's test support, which only a build
// with the `capture` feature turns on. The rack and the card are windows of their own, so each is
// drawn on its own, and `showcase::compose` lays them on a canvas where they sat on screen.
#[cfg(feature = "capture")]
fn save_picture(
    window: AnyWindowHandle,
    view: WeakEntity<RackView>,
    card_tile: plates_core::TileId,
    path: PathBuf,
    cx: &mut App,
) {
    cx.spawn(async move |cx| {
        let draw = |window: AnyWindowHandle, cx: &mut AsyncApp| {
            let image = window
                .update(cx, |_, window, _| window.render_to_image())
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
            Ok::<_, String>(showcase::Picture {
                width: image.width(),
                height: image.height(),
                rgba: image.into_raw(),
            })
        };
        cx.background_executor().timer(SETTLE).await;
        view.update(cx, |this, cx| this.showcase_card(card_tile, cx))
            .ok();
        cx.background_executor().timer(SETTLE).await;
        let saved = (|| {
            let bounds = |window: AnyWindowHandle, cx: &mut AsyncApp| {
                window
                    .update(cx, |_, window, _| window.bounds().origin)
                    .map_err(|e| e.to_string())
            };
            let rack = draw(window, cx)?;
            let rack_at = bounds(window, cx)?;
            let card_window = view
                .read_with(cx, |this, _| this.card_window())
                .map_err(|e| e.to_string())?;
            let card = match card_window {
                Some(w) => {
                    let at = bounds(w, cx)?;
                    let (dx, dy) = (at.x - rack_at.x, at.y - rack_at.y);
                    Some((draw(w, cx)?, f32::from(dx), f32::from(dy)))
                }
                None => None,
            };
            let picture = showcase::compose(&rack, card.as_ref().map(|(c, x, y)| (c, *x, *y)));
            write_png(&path, picture.width, picture.height, &picture.rgba)
        })();
        match saved {
            Ok(()) => println!("Saved {}", path.display()),
            Err(e) => eprintln!("Couldn't save {}: {e}", path.display()),
        }
        cx.update(|cx| cx.quit());
    })
    .detach();
}

#[cfg(feature = "capture")]
fn write_png(path: &std::path::Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut w| w.write_image_data(rgba))
        .map_err(|e| e.to_string())
}

#[cfg(not(feature = "capture"))]
fn save_picture(
    _: AnyWindowHandle,
    _: WeakEntity<RackView>,
    _: plates_core::TileId,
    _: PathBuf,
    cx: &mut App,
) {
    eprintln!(
        "Saving a picture needs a build with the capture feature: cargo run -p plates --features capture -- showcase --capture <file.png>"
    );
    cx.quit();
}

// Opens the rack's window. The theme and fonts are set up here, since the rack is the first
// window either way Plates starts.
fn open_rack(
    rack: Rack,
    data_file: PathBuf,
    events: async_channel::Receiver<HookMessage>,
    commands: (
        async_channel::Sender<tray::Command>,
        async_channel::Receiver<tray::Command>,
    ),
    showcase: bool,
    cx: &mut App,
) -> (AnyWindowHandle, Entity<RackView>) {
    gpui_kit::init(cx);
    cx.text_system()
        .add_fonts(SANS_FACES.iter().map(|face| Cow::Borrowed(*face)).collect())
        .expect("Inter's bundled faces should load");
    Theme::change(ThemeMode::Dark, None, cx);
    // gpui-component's Root fills the whole window with the theme's background, which would
    // show as dark squares outside the rack's rounded corners.
    // (read in gpui-component 0.9.1, src/root.rs, the Root plugin's style)
    cx.global_mut::<Theme>().tokens.background = transparent_black().into();

    let display = cx.primary_display().expect("there should be a display");
    let db = display.bounds();
    let size = size(px(RACK_W), px(rack_height(rack.tiles().len(), false)));
    // The rack opens at the primary display's center, and then RackView moves it to where it
    // last came to rest, if place.json says.
    let origin = point(
        db.left() + (db.size.width - size.width) / 2.,
        db.top() + (db.size.height - size.height) / 2.,
    );
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
        // The rack is never the active window, and GPUI caps an inactive window at about 30
        // frames a second by default, which would make every throw stutter.
        inactive_frame_interval: None,
        low_power_frame_interval: None,
        thermal_frame_interval: None,
        app_id: Some("plates".into()),
        ..Default::default()
    };
    gpui_kit::open_window(options, cx, move |window, cx| {
        cx.new(|cx| RackView::new(rack, data_file, events, commands, showcase, window, cx))
    })
    .expect("the rack's window should open")
}
