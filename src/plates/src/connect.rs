//! Connecting Plates to Claude Code, for `plates hooks install` and the tray's "Connect to Claude
//! Code" alike: adding Plates' hooks to Claude Code's user settings and registering its task
//! server for every project, or taking both out (record/areas/hooks/01-claude-code-hooks.md).
//! A plan works out the whole change before any of it is made, so each caller can show it to you
//! and ask first.

use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use plates_core::install::{self, Connection, InstallError, SERVER, ServerStep};
use serde_json::{Value, json};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Change {
    /// Adds Plates' hooks and registers its task server, replacing any that another copy or
    /// version of Plates left.
    Connect,
    Disconnect,
}

/// Where `claude`'s output goes: to the terminal `plates hooks install` runs in, or nowhere, for
/// the tray, which has no terminal.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Output {
    Terminal,
    Quiet,
}

/// A change to Claude Code's settings, worked out but not made.
pub struct Plan {
    pub change: Change,
    /// Claude Code's user settings file, `~/.claude/settings.json`.
    pub settings_path: PathBuf,
    /// The file that registers MCP servers for every project, which `claude mcp` writes.
    pub claude_json_path: PathBuf,
    current: Value,
    next: Value,
    /// Plates' hooks as a connection writes them, which `plates hooks install` shows you.
    pub hooks: Option<Value>,
    /// The arguments of the `claude` commands that register or remove the task server.
    pub commands: Vec<Vec<String>>,
}

impl Plan {
    pub fn new(change: Change) -> Result<Plan, String> {
        let settings_path = settings_path()?;
        let current = read_json(&settings_path)?;
        let describe = |e: InstallError| format!("{}: {e}", settings_path.display());
        let (next, hooks, server) = match change {
            Change::Connect => {
                let hook = beside("plates-hook")?;
                (
                    install::with(&current, &hook).map_err(describe)?,
                    Some(Value::Object(install::ours(&hook))),
                    Some(beside("plates-tasks")?),
                )
            }
            Change::Disconnect => (install::without(&current).map_err(describe)?, None, None),
        };
        let claude_json_path = claude_json_path()?;
        let step = install::server_step(&read_json(&claude_json_path)?, server.as_deref())
            .map_err(|e| format!("{}: {e}", claude_json_path.display()))?;
        Ok(Plan {
            change,
            commands: server_commands(step, server.as_deref()),
            settings_path,
            claude_json_path,
            current,
            next,
            hooks,
        })
    }

    pub fn changes_settings(&self) -> bool {
        self.next != self.current
    }

    pub fn is_empty(&self) -> bool {
        !self.changes_settings() && self.commands.is_empty()
    }

    /// Backs the settings file up beside itself and writes the change, then runs the `claude`
    /// commands, telling `report` each thing it did, such as "Wrote <path>".
    pub fn apply(&self, output: Output, mut report: impl FnMut(String)) -> Result<(), String> {
        if self.changes_settings() {
            if self.settings_path.exists() {
                let backup = self
                    .settings_path
                    .with_extension(format!("json.plates-backup-{}", unix_secs()));
                fs::copy(&self.settings_path, &backup).map_err(|e| {
                    format!("Couldn't back up {}: {e}", self.settings_path.display())
                })?;
                report(format!("Backed up the old file to {}", backup.display()));
            }
            write_json(&self.settings_path, &self.next)?;
            report(format!("Wrote {}", self.settings_path.display()));
        }
        let done = match self.change {
            Change::Connect => "registered",
            Change::Disconnect => "removed",
        };
        for (i, args) in self.commands.iter().enumerate() {
            claude(args, output).map_err(|e| {
                let rest: Vec<String> = self.commands[i..].iter().map(|a| shown(a)).collect();
                format!(
                    "{e}\nPlates' task server isn't {done}. To finish, run these in a terminal where `claude` works:\n\n  {}",
                    rest.join("\n  ")
                )
            })?;
        }
        Ok(())
    }
}

/// How Claude Code's settings stand toward this copy of Plates.
pub fn status() -> Result<Connection, String> {
    Ok(install::connection(
        &read_json(&settings_path()?)?,
        &read_json(&claude_json_path()?)?,
        &beside("plates-hook")?,
        &beside("plates-tasks")?,
    ))
}

/// Takes Plates out of Claude Code's settings as Plates is uninstalled, so Claude Code doesn't go
/// on running a `plates-hook` that's gone. Velopack runs this on Windows only, and gives it 30s.
#[cfg(windows)]
pub fn disconnect_for_uninstall() {
    if let Ok(plan) = Plan::new(Change::Disconnect)
        && !plan.is_empty()
    {
        let _ = plan.apply(Output::Quiet, |_| {});
    }
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
pub fn shown(args: &[String]) -> String {
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
fn claude(args: &[String], output: Output) -> Result<(), String> {
    let program = find_claude().ok_or("Couldn't find Claude Code's `claude` command.")?;
    let mut command = Command::new(&program);
    command.args(args);
    let (status, said) = match output {
        Output::Terminal => command.status().map(|s| (s, String::new())),
        Output::Quiet => {
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped());
            // A console program started from Plates' tray, which has no console, would open a
            // console window of its own for the moment it runs.
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                command.creation_flags(CREATE_NO_WINDOW);
            }
            command.output().map(|o| {
                (
                    o.status,
                    String::from_utf8_lossy(&o.stderr).trim().to_string(),
                )
            })
        }
    }
    .map_err(|e| format!("Couldn't run {}: {e}", program.display()))?;
    if status.success() {
        Ok(())
    } else if said.is_empty() {
        Err(format!("`{}` failed with {status}.", shown(args)))
    } else {
        Err(format!("`{}` failed with {status}: {said}", shown(args)))
    }
}

/// Where Claude Code's `claude` command is: the first on `PATH`, then the one in `~/.local/bin`,
/// where Claude Code's installer put it on our Mac, then on macOS, the one your shell finds.
/// Plates opened from Finder or at login gets a `PATH` without the folders your shell adds, so
/// the tray can't count on `PATH` alone.
fn find_claude() -> Option<PathBuf> {
    let name = format!("claude{}", std::env::consts::EXE_SUFFIX);
    std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join(&name))
                .find(|p| p.is_file())
        })
        .or_else(|| {
            dirs::home_dir()
                .map(|home| home.join(".local").join("bin").join(&name))
                .filter(|p| p.is_file())
        })
        .or_else(from_shell)
}

/// The `claude` an interactive login shell finds. A login shell alone didn't find it on our Mac,
/// since the folder it's in joins `PATH` in `~/.zshrc`, which only an interactive zsh reads
/// (record/areas/hooks/log.md, 2026-11-24). An interactive shell can wait on a prompt of its own,
/// so it gets 5s before Plates gives up on it.
#[cfg(target_os = "macos")]
fn from_shell() -> Option<PathBuf> {
    use std::{
        io::Read,
        time::{Duration, Instant},
    };
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/zsh".into());
    let mut child = Command::new(shell)
        .args(["-ilc", "command -v claude"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut said = String::new();
    child.stdout.take()?.read_to_string(&mut said).ok()?;
    said.lines()
        .map(str::trim)
        .filter(|line| line.starts_with('/'))
        .map(PathBuf::from)
        .rfind(|p| p.is_file())
}

#[cfg(not(target_os = "macos"))]
fn from_shell() -> Option<PathBuf> {
    None
}
