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
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    time::{SystemTime, UNIX_EPOCH},
};

use plates_core::{
    Rack, hooks,
    install::{self, InstallError, SERVER, ServerStep},
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

/// The program named `name` beside this one: `plates-hook`, which each hook runs, or
/// `plates-tasks`, the task server.
fn beside(name: &str) -> Result<PathBuf, String> {
    let exe =
        std::env::current_exe().map_err(|e| format!("Couldn't find Plates' own path: {e}"))?;
    let program = exe.with_file_name(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    if program.exists() {
        Ok(program)
    } else {
        Err(format!(
            "{} isn't there. Build it with `cargo build -p {name}` first.",
            program.display()
        ))
    }
}

fn home() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "Couldn't find your home folder.".to_string())
}

/// Claude Code's user settings file, `~/.claude/settings.json`.
fn settings_path() -> Result<PathBuf, String> {
    Ok(home()?.join(".claude").join("settings.json"))
}

/// `.claude.json`, which holds the MCP servers registered for every project: in the folder
/// `CLAUDE_CONFIG_DIR` names when it's set, as `claude mcp add` writes it, and in your home folder
/// otherwise (measured with Claude Code 2.1.284 on Windows 11 25H2, record/areas/tasks/log.md).
fn claude_json_path() -> Result<PathBuf, String> {
    match std::env::var_os("CLAUDE_CONFIG_DIR").filter(|dir| !dir.is_empty()) {
        Some(dir) => Ok(PathBuf::from(dir).join(".claude.json")),
        None => Ok(home()?.join(".claude.json")),
    }
}

/// A JSON file, or an empty object when there's none.
fn read_json(path: &Path) -> Result<Value, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes)
            .map_err(|e| format!("{} isn't JSON: {e}", path.display())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(format!("Couldn't read {}: {e}", path.display())),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Change {
    Install,
    Uninstall,
}

// Shows you the change to Claude Code's settings and the `claude mcp` commands for the task
// server, backs the settings file up beside itself, and makes the change only once you confirm,
// as record/areas/hooks/01-claude-code-hooks.md asks.
fn change_hooks(change: Change, yes: bool) -> Result<(), String> {
    let path = settings_path()?;
    let current = read_json(&path)?;
    let describe = |e: InstallError| format!("{}: {e}", path.display());
    let (next, server) = match change {
        Change::Install => (
            install::with(&current, &beside("plates-hook")?).map_err(describe)?,
            Some(beside("plates-tasks")?),
        ),
        Change::Uninstall => (install::without(&current).map_err(describe)?, None),
    };
    let claude_json = claude_json_path()?;
    let step = install::server_step(&read_json(&claude_json)?, server.as_deref())
        .map_err(|e| format!("{}: {e}", claude_json.display()))?;
    let commands = server_commands(step, server.as_deref());
    if next == current && commands.is_empty() {
        let (hooks, registers) = match change {
            Change::Install => ("has", "registers"),
            Change::Uninstall => ("has none of", "registers none of"),
        };
        println!(
            "{} already {hooks} Plates' hooks, and {} already {registers} Plates' task server, so nothing changes.",
            path.display(),
            claude_json.display(),
        );
        return Ok(());
    }
    if next != current {
        match change {
            Change::Install => {
                println!(
                    "This adds these hooks to {}, and leaves every other setting and hook as it is:\n",
                    path.display()
                );
                let ours = Value::Object(install::ours(&beside("plates-hook")?));
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
    }
    let (verb, preposition, done) = match change {
        Change::Install => ("registers", "for", "registered"),
        Change::Uninstall => ("removes", "from", "removed"),
    };
    if !commands.is_empty() {
        println!(
            "This {verb} Plates' task server, `{SERVER}`, {preposition} every project, with these commands:\n"
        );
        for args in &commands {
            println!("  {}", shown(args));
        }
        println!();
    }
    if !yes && !confirm("Make the change? [y/N] ")? {
        println!("Nothing was changed.");
        return Ok(());
    }
    if next != current {
        if path.exists() {
            let backup = path.with_extension(format!("json.plates-backup-{}", unix_secs()));
            fs::copy(&path, &backup)
                .map_err(|e| format!("Couldn't back up {}: {e}", path.display()))?;
            println!("Backed up the old file to {}", backup.display());
        }
        write_json(&path, &next)?;
        println!("Wrote {}", path.display());
    }
    for (i, args) in commands.iter().enumerate() {
        claude(args).map_err(|e| {
            let rest: Vec<String> = commands[i..].iter().map(|a| shown(a)).collect();
            format!(
                "{e}\nPlates' task server isn't {done}. To finish, run these in a terminal where `claude` works:\n\n  {}",
                rest.join("\n  ")
            )
        })?;
    }
    Ok(())
}

/// The arguments of the `claude` commands that take `step`, such as `mcp add --scope user plates
/// -- <program>`. Adding a server that's already registered fails, as does removing one that
/// isn't, so a server registered with another program is removed first (measured with Claude Code
/// 2.1.284 on Windows 11 25H2, record/areas/tasks/log.md).
fn server_commands(step: ServerStep, program: Option<&Path>) -> Vec<Vec<String>> {
    let remove = || {
        ["mcp", "remove", "--scope", "user", SERVER]
            .map(String::from)
            .to_vec()
    };
    let add = |program: &Path| {
        let mut args = ["mcp", "add", "--scope", "user", SERVER, "--"]
            .map(String::from)
            .to_vec();
        args.push(program.to_string_lossy().into_owned());
        args
    };
    match (step, program) {
        (ServerStep::Add, Some(p)) => vec![add(p)],
        (ServerStep::Replace, Some(p)) => vec![remove(), add(p)],
        (ServerStep::Remove, _) => vec![remove()],
        _ => Vec::new(),
    }
}

/// A `claude` command as you'd type it, with any argument that holds a space in quotes.
fn shown(args: &[String]) -> String {
    let quoted = args.iter().map(|a| {
        if a.contains(' ') {
            format!("\"{a}\"")
        } else {
            a.clone()
        }
    });
    std::iter::once("claude".to_string())
        .chain(quoted)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Runs Claude Code's `claude` command, which prints what it changed.
fn claude(args: &[String]) -> Result<(), String> {
    let status = Command::new("claude")
        .args(args)
        .status()
        .map_err(|e| format!("Couldn't run Claude Code's `claude` command: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{}` failed with {status}.", shown(args)))
    }
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
