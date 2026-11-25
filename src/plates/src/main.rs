//! Plates: `plates` opens the rack, and `plates hooks` and `plates listen` are the commands in
//! cli.rs.

#[cfg(not(any(windows, target_os = "macos")))]
compile_error!("Plates runs on Windows and macOS.");

mod app;
mod assets;
mod card;
mod cli;
mod connect;
mod hold;
mod markup;
mod menu;
mod motion;
mod native;
mod place;
mod press;
mod rack_view;
mod rows;
mod settings;
mod showcase;
mod swipe;
mod task_list;
mod tone;
mod tray;
mod updates;
mod view;

use std::{
    fs::OpenOptions,
    io::Write,
    process::ExitCode,
    time::{SystemTime, UNIX_EPOCH},
};

fn main() -> ExitCode {
    // Velopack's install, update, and uninstall hooks run here and may exit the process, so this
    // comes before anything else.
    // An update installs only from the rack's menu, after Plates stops its task servers, rather
    // than the next time any `plates` starts, such as a second one opened while the rack runs, or
    // `plates hooks install`. Velopack installs a downloaded update as the program starts unless
    // told not to (read in velopack 1.2.161, src/app.rs, VelopackApp::run).
    let mut velopack = velopack::VelopackApp::build().set_auto_apply_on_startup(false);
    #[cfg(windows)]
    {
        velopack =
            velopack.on_before_uninstall_fast_callback(|_| connect::disconnect_for_uninstall());
    }
    velopack.run();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        log_panics();
        app::run();
        return ExitCode::SUCCESS;
    }
    match cli::run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

// The rack runs with no console, so a panic's message would go nowhere, and a panic inside
// GPUI's window procedure, which can't unwind, ends the process at once with only an exception
// code in Windows' event log. Each panic is added to panic.log in Plates' data folder first.
fn log_panics() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let dir = app::data_dir();
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let backtrace = std::backtrace::Backtrace::force_capture();
        let entry = format!("unix time {at}: {info}\n{backtrace}\n");
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("panic.log"))
        {
            let _ = file.write_all(entry.as_bytes());
        }
        default(info);
    }));
}
