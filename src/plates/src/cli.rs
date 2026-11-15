//! Plates' commands:
//!
//! - `plates hooks install` adds Plates' hooks to Claude Code's user settings, after showing you
//!   the change, and `plates hooks uninstall` takes them out again. `--yes` skips the question.
//! - `plates listen` runs the hook receiver and prints what each event does to a rack, and
//!   `--log <file>` also appends each event's JSON to that file, one per line.

use std::{
    fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    sync::mpsc,
    time::{SystemTime, UNIX_EPOCH},
};

use plates_core::{
    Rack, hooks,
    install::{self, InstallError},
    receiver::{self, PORT, Received, Receiver},
};
use serde_json::{Value, json};

/// Runs the command `args` names, or returns the usage when it names none.
pub fn run(args: &[String]) -> Result<(), String> {
    let flag = |name: &str| args.iter().any(|a| a == name);
    match args.first().map(String::as_str) {
        Some("hooks") => match args.get(1).map(String::as_str) {
            Some("install") => change_hooks(Change::Install, flag("--yes")),
            Some("uninstall") => change_hooks(Change::Uninstall, flag("--yes")),
            _ => Err(usage()),
        },
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
    "Usage: plates | plates hooks install [--yes] | plates hooks uninstall [--yes] | plates listen [--log <file>]"
        .to_string()
}

/// The `plates-hook` program beside this one, which each hook runs.
fn hook_program() -> Result<PathBuf, String> {
    let exe =
        std::env::current_exe().map_err(|e| format!("Couldn't find Plates' own path: {e}"))?;
    let program = exe.with_file_name(format!("plates-hook{}", std::env::consts::EXE_SUFFIX));
    if program.exists() {
        Ok(program)
    } else {
        Err(format!(
            "{} isn't there. Build it with `cargo build -p plates-hook` first.",
            program.display()
        ))
    }
}

/// Claude Code's user settings file, `~/.claude/settings.json`.
fn settings_path() -> Result<PathBuf, String> {
    dirs::home_dir()
        .map(|home| home.join(".claude").join("settings.json"))
        .ok_or_else(|| "Couldn't find your home folder.".to_string())
}

#[derive(Clone, Copy, PartialEq)]
enum Change {
    Install,
    Uninstall,
}

// Shows you the change to Claude Code's settings, backs the file up beside itself, and writes
// the change only once you confirm, as record/areas/hooks/01-claude-code-hooks.md asks.
fn change_hooks(change: Change, yes: bool) -> Result<(), String> {
    let path = settings_path()?;
    let current = match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes)
            .map_err(|e| format!("{} isn't JSON: {e}", path.display()))?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => json!({}),
        Err(e) => return Err(format!("Couldn't read {}: {e}", path.display())),
    };
    let describe = |e: InstallError| format!("{}: {e}", path.display());
    let next = match change {
        Change::Install => install::with(&current, &hook_program()?).map_err(describe)?,
        Change::Uninstall => install::without(&current).map_err(describe)?,
    };
    if next == current {
        println!(
            "{} already {} Plates' hooks, so nothing changes.",
            path.display(),
            if change == Change::Install {
                "has"
            } else {
                "has none of"
            }
        );
        return Ok(());
    }
    match change {
        Change::Install => {
            println!(
                "This adds these hooks to {}, and leaves every other setting and hook as it is:\n",
                path.display()
            );
            let ours = Value::Object(install::ours(&hook_program()?));
            println!(
                "{}\n",
                serde_json::to_string_pretty(&ours).unwrap_or_default()
            );
        }
        Change::Uninstall => println!(
            "This takes Plates' hooks out of {}, and leaves every other setting and hook as it is.\n",
            path.display()
        ),
    }
    if !yes && !confirm("Write the change? [y/N] ")? {
        println!("Nothing was written.");
        return Ok(());
    }
    if path.exists() {
        let backup = path.with_extension(format!("json.plates-backup-{}", unix_secs()));
        fs::copy(&path, &backup)
            .map_err(|e| format!("Couldn't back up {}: {e}", path.display()))?;
        println!("Backed up the old file to {}", backup.display());
    }
    write_json(&path, &next)?;
    println!("Wrote {}", path.display());
    Ok(())
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

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let mut text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    text.push('\n');
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(path, text).map_err(|e| format!("Couldn't write {}: {e}", path.display()))
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// Runs the receiver and prints each event with what it did to the session's tile, which is how
// the hooks log measured what each hook event carries in a desktop session.
fn listen(log: Option<PathBuf>) -> Result<(), String> {
    let (tx, rx) = mpsc::channel();
    let _receiver =
        Receiver::start(PORT, tx).map_err(|e| format!("Couldn't listen on port {PORT}: {e}"))?;
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
        }
    }
    Ok(())
}
