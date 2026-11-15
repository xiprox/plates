//! Starts Plates: loads your own tiles, starts the receiver for Claude Code's hook events, and
//! opens the rack's window.

use std::{path::PathBuf, sync::mpsc, thread};

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;
use plates_core::{
    Rack,
    receiver::{PORT, Received, Receiver},
    store,
};

use crate::{
    assets::Assets,
    rack_view::{RACK_W, RackView, rack_height},
};

/// Plates' data folder, `%APPDATA%\Plates\` on Windows and
/// `~/Library/Application Support/Plates/` on macOS, as record/areas/rack/02-tiles.md gives it.
fn data_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Plates")
}

pub fn run() {
    let data_file = data_dir().join("rack.json");
    let rack = match store::load(&data_file) {
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

    // The receiver hands events on from a thread of its own, and the rack takes them on the UI
    // thread, so a thread between them moves each one onto a channel the UI can await.
    let (tx, rx) = mpsc::channel();
    let receiver = Receiver::start(PORT, tx);
    if let Err(e) = &receiver {
        eprintln!("Couldn't listen on port {PORT}, so no session tiles will show: {e}");
    }
    let (events_tx, events) = async_channel::unbounded();
    thread::Builder::new()
        .name("plates-events".into())
        .spawn(move || {
            for received in rx {
                if let Received::Event(message, _) = received
                    && events_tx.send_blocking(message).is_err()
                {
                    break;
                }
            }
        })
        .expect("the events thread should start");

    application().with_assets(Assets).run(move |cx: &mut App| {
        // The receiver's thread runs for the life of the app.
        let _receiver = receiver;
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        // gpui-component's Root fills the whole window with the theme's background, which would
        // show as dark squares outside the rack's rounded corners.
        // (read in gpui-component 0.9.1, src/root.rs, the Root plugin's style)
        cx.global_mut::<Theme>().tokens.background = transparent_black().into();

        let display = cx.primary_display().expect("there should be a display");
        let db = display.bounds();
        let size = size(px(RACK_W), px(rack_height(rack.tiles().len(), false)));
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
            // The rack is never the active window, and GPUI caps an inactive window at about 30
            // frames a second by default, which would make every throw stutter.
            inactive_frame_interval: None,
            low_power_frame_interval: None,
            thermal_frame_interval: None,
            app_id: Some("plates".into()),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, move |window, cx| {
            cx.new(|cx| RackView::new(rack, data_file, events, window, cx))
        })
        .expect("the rack's window should open");
    });
}
