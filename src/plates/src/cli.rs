//! Plates' commands:
//!
//! - `plates hooks install` adds Plates' hooks to Claude Code's user settings and registers its
//!   task server for every project, after showing you the change, and `plates hooks uninstall`
//!   takes both out again. `--yes` skips the question.
//! - `plates listen` runs the hook receiver and prints what each event does to a rack, and
//!   `--log <file>` also appends each event's JSON to that file, one per line.
//! - `plates showcase` opens the rack on made-up tiles, and `--capture <file.png>` saves a picture
//!   of the rack three times, plain, with a tile's task list open, and with the tile's card, and
//!   quits, in a build with the `capture` feature on macOS. `--tasks` gives that tile its made-up
//!   task list, open, for working on how it looks. `--capture` opens and closes the list itself,
//!   so `--tasks` adds nothing to it.

use std::{
    fs,
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::mpsc,
    time::SystemTime,
};

use plates_core::{
    Rack, hooks,
    install::SERVER,
    receiver::{self, PORT, Received, Receiver},
};
use serde_json::Value;

use crate::connect::{self, Change, Output, Plan};

/// Runs the command `args` names, or returns the usage when it names none.
pub fn run(args: &[String]) -> Result<(), String> {
    let flag = |name: &str| args.iter().any(|a| a == name);
    match args.first().map(String::as_str) {
        Some("hooks") => match args.get(1).map(String::as_str) {
            Some("install") => change_hooks(Change::Connect, flag("--yes")),
            Some("uninstall") => change_hooks(Change::Disconnect, flag("--yes")),
            _ => Err(usage()),
        },
        Some("showcase") => {
            let capture = args
                .iter()
                .position(|a| a == "--capture")
                .map(|i| args.get(i + 1).map(PathBuf::from).ok_or_else(usage))
                .transpose()?;
            crate::app::showcase(capture, flag("--tasks"));
            Ok(())
        }
        Some("listen") => {
            let log = args
                .iter()
                .position(|a| a == "--log")
                .and_then(|i| args.get(i + 1))
                .map(PathBuf::from);
            listen(log)
        }
        _ => Err(usage()),
    }
}

fn usage() -> String {
    "Usage: plates | plates hooks install [--yes] | plates hooks uninstall [--yes] | plates listen [--log <file>] | plates showcase [--capture <file.png>] [--tasks]"
        .to_string()
}

// Shows you the change to Claude Code's settings and the `claude mcp` commands for the task
// server, and makes the change only once you confirm, as record/areas/hooks/01-claude-code-hooks.md
// asks.
fn change_hooks(change: Change, yes: bool) -> Result<(), String> {
    let plan = Plan::new(change)?;
    if plan.is_empty() {
        let (hooks, registers) = match change {
            Change::Connect => ("has", "registers"),
            Change::Disconnect => ("has none of", "registers none of"),
        };
        println!(
            "{} already {hooks} Plates' hooks, and {} already {registers} Plates' task server, so nothing changes.",
            plan.settings_path.display(),
            plan.claude_json_path.display(),
        );
        return Ok(());
    }
    if plan.changes_settings() {
        match &plan.hooks {
            Some(hooks) => println!(
                "This adds these hooks to {}, and leaves every other setting and hook as it is:\n\n{}\n",
                plan.settings_path.display(),
                serde_json::to_string_pretty(hooks).unwrap_or_default()
            ),
            None => println!(
                "This takes Plates' hooks out of {}, and leaves every other setting and hook as it is.\n",
                plan.settings_path.display()
            ),
        }
    }
    let (verb, preposition) = match change {
        Change::Connect => ("registers", "for"),
        Change::Disconnect => ("removes", "from"),
    };
    if !plan.commands.is_empty() {
        println!(
            "This {verb} Plates' task server, `{SERVER}`, {preposition} every project, with these commands:\n"
        );
        for args in &plan.commands {
            println!("  {}", connect::shown(args));
        }
        println!();
    }
    if !yes && !confirm("Make the change? [y/N] ")? {
        println!("Nothing was changed.");
        return Ok(());
    }
    plan.apply(Output::Terminal, |line| println!("{line}"))
}

fn confirm(question: &str) -> Result<bool, String> {
    print!("{question}");
    io::stdout().flush().map_err(|e| e.to_string())?;
    let mut answer = String::new();
    io::stdin()
        .lock()
        .read_line(&mut answer)
        .map_err(|e| e.to_string())?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}

// Runs the receiver and prints each event with what it did to the session's tile, which is how
// the hooks log measured what each hook event carries in a desktop session.
fn listen(log: Option<PathBuf>) -> Result<(), String> {
    let (tx, rx) = mpsc::channel();
    let _receiver = Receiver::start(PORT, tx, false)
        .map_err(|e| format!("Couldn't listen on port {PORT}: {e}"))?;
    println!("Listening on {}", receiver::url(PORT));
    let mut log = match log {
        Some(path) => Some(
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|e| format!("Couldn't open {}: {e}", path.display()))?,
        ),
        None => None,
    };
    let mut rack = Rack::new();
    for received in rx {
        let raw = match &received {
            Received::Event(_, raw) | Received::Unreadable(_, raw) => raw,
            // A receiver that shows no rack never hands one on.
            Received::Show => continue,
        };
        if let Some(file) = log.as_mut() {
            let line = serde_json::from_slice::<Value>(raw)
                .map(|v| v.to_string())
                .unwrap_or_else(|_| String::from_utf8_lossy(raw).into_owned());
            let _ = writeln!(file, "{line}");
        }
        match received {
            Received::Event(message, _) => {
                let short: String = message.session_id.chars().take(8).collect();
                let name = message.hook.clone();
                let Some(id) = hooks::apply(&mut rack, *message, SystemTime::now()) else {
                    println!("{name} {short}: ignored");
                    continue;
                };
                if let Some(tile) = rack.tile(id) {
                    let s = tile.session();
                    println!(
                        "{name} {short}: {:?}, \"{}\"{}",
                        s.map(|s| s.state),
                        tile.title(),
                        s.and_then(|s| s.summary.as_deref())
                            .map(|summary| format!(", {summary}"))
                            .unwrap_or_default()
                    );
                }
            }
            Received::Unreadable(error, _) => eprintln!("Couldn't read an event: {error}"),
            Received::Show => {}
        }
    }
    Ok(())
}
