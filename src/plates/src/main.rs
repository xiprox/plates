//! Plates: `plates` opens the rack, and `plates hooks` and `plates listen` are the commands in
//! cli.rs.

#[cfg(not(windows))]
compile_error!("Plates runs on Windows so far, and its macOS half comes in the plan's step 7.");

mod app;
mod assets;
mod cli;
mod markup;
mod menu;
mod motion;
mod native;
mod place;
mod rack_view;
mod view;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
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
