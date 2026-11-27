//! Finding and running the command-line programs Plates calls on: Claude Code's `claude`, and
//! GitHub's `gh`.
//!
//! Plates opened from Finder or at login gets a `PATH` without the folders your shell adds, so
//! it can't count on `PATH` alone to find either.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// Where the program `name` is: the first on `PATH`, then the first in `also`, then on macOS, the
/// one your shell finds.
pub fn find(name: &str, also: &[PathBuf]) -> Option<PathBuf> {
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join(&file))
                .find(|p| p.is_file())
        })
        .or_else(|| also.iter().map(|dir| dir.join(&file)).find(|p| p.is_file()))
        .or_else(|| from_shell(name))
}

/// A command for `program` that opens no window and reads nothing from Plates.
pub fn quiet(program: &Path) -> Command {
    let mut command = Command::new(program);
    command.stdin(Stdio::null());
    // A console program started from Plates, which has no console, would get a console window of
    // its own for the moment it runs, and `CREATE_NO_WINDOW` runs it without one (documented in
    // <https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags>).
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// The program an interactive login shell finds. A login shell alone didn't find `claude` on our
/// Mac, since the folder it's in joins `PATH` in `~/.zshrc`, which only an interactive zsh reads
/// (record/areas/hooks/log.md, 2026-11-24). An interactive shell can wait on a prompt of its own,
/// so it gets 5s before Plates gives up on it.
#[cfg(target_os = "macos")]
fn from_shell(name: &str) -> Option<PathBuf> {
    use std::{
        io::Read,
        time::{Duration, Instant},
    };
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/zsh".into());
    let mut child = Command::new(shell)
        .args(["-ilc", &format!("command -v {name}")])
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
fn from_shell(_: &str) -> Option<PathBuf> {
    None
}
