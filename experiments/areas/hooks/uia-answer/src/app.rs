//! What the probe reads about the Claude desktop app without UI Automation: its file for each
//! session, the session's transcript, its windows, and its processes' command lines.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible,
};
use windows::core::{BOOL, PWSTR};

/// What the app's file `local_<id>.json` says about one session.
pub struct SessionFile {
    pub id: String,
    pub title: String,
    pub cli_id: String,
    pub archived: bool,
    pub focused_at_ms: u64,
}

/// Reads every session file under
/// `%LOCALAPPDATA%\Claude-Data\claude_<package id>\claude-code-sessions\`.
pub fn session_files() -> Vec<SessionFile> {
    let Some(local) = std::env::var_os("LOCALAPPDATA") else {
        return Vec::new();
    };
    let data = Path::new(&local).join("Claude-Data");
    let Some(folder) = fs::read_dir(&data).ok().and_then(|entries| {
        entries
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("claude_"))
            .map(|e| e.path().join("claude-code-sessions"))
            .find(|p| p.is_dir())
    }) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut dirs = vec![folder];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !(name.starts_with("local_") && name.ends_with(".json")) {
                continue;
            }
            let Some(v) = fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            else {
                continue;
            };
            let text = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            out.push(SessionFile {
                id: text("sessionId"),
                title: text("title"),
                cli_id: text("cliSessionId"),
                archived: v.get("isArchived").and_then(Value::as_bool) == Some(true),
                focused_at_ms: v.get("lastFocusedAt").and_then(Value::as_u64).unwrap_or(0),
            });
        }
    }
    out
}

/// The session's transcript, `%USERPROFILE%\.claude\projects\<project>\<cliSessionId>.jsonl`.
pub fn transcript(cli_id: &str) -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE")?;
    let projects = Path::new(&home).join(".claude").join("projects");
    let file = format!("{cli_id}.jsonl");
    fs::read_dir(projects)
        .ok()?
        .flatten()
        .map(|e| e.path().join(&file))
        .find(|p| p.is_file())
}

/// The last call to the app's wrap-up tool in a transcript, and what came after it.
#[derive(Default)]
pub struct WrapUp {
    pub ask: String,
    pub recap: String,
    /// The text of each reply button, which the app sends as your message when you click it.
    pub labels: Vec<String>,
    /// The app actions, such as `commit`, whose buttons the app labels itself.
    pub app_actions: Vec<String>,
    pub at: String,
    /// Prompts you typed after the call, so 0 means the wrap-up is still the session's last.
    pub prompts_after: usize,
}

pub struct Transcript {
    pub wrap_up: Option<WrapUp>,
    pub last_text: String,
}

pub fn read_transcript(path: &Path) -> Transcript {
    let text = fs::read_to_string(path).unwrap_or_default();
    let mut wrap_up: Option<WrapUp> = None;
    let mut last_text = String::new();
    for line in text.lines() {
        let Ok(r) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if r.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let kind = r.get("type").and_then(Value::as_str);
        let parts = r
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(Value::as_array);
        if kind == Some("user") && is_typed_prompt(&r) {
            if let Some(w) = wrap_up.as_mut() {
                w.prompts_after += 1;
            }
            continue;
        }
        if kind != Some("assistant") {
            continue;
        }
        for p in parts.into_iter().flatten() {
            let ptype = p.get("type").and_then(Value::as_str);
            if ptype == Some("text") {
                if let Some(t) = p.get("text").and_then(Value::as_str).filter(|t| !t.trim().is_empty()) {
                    last_text = t.to_string();
                }
            }
            if ptype == Some("tool_use")
                && p.get("name").and_then(Value::as_str) == Some("mcp__ccd_turn__wrap_up")
            {
                let input = p.get("input").cloned().unwrap_or(Value::Null);
                let s = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                let mut w = WrapUp {
                    ask: s("ask"),
                    recap: s("recap"),
                    at: r.get("timestamp").and_then(Value::as_str).unwrap_or("").to_string(),
                    ..WrapUp::default()
                };
                for a in input.get("actions").and_then(Value::as_array).into_iter().flatten() {
                    if let Some(action) = a.get("action").and_then(Value::as_str) {
                        w.app_actions.push(action.to_string());
                    } else if let Some(label) = a.get("label").and_then(Value::as_str) {
                        w.labels.push(label.to_string());
                    }
                }
                wrap_up = Some(w);
            }
        }
    }
    Transcript { wrap_up, last_text }
}

// A desktop session's transcript marks a prompt you typed with `origin.kind` "human"
// (record/areas/hooks/log.md, the 2026-11-15 entry on typed prompts).
fn is_typed_prompt(r: &Value) -> bool {
    r.get("origin").and_then(|o| o.get("kind")).and_then(Value::as_str) == Some("human")
}

pub struct AppWindow {
    pub hwnd: HWND,
    pub pid: u32,
    pub title: String,
    pub class: String,
    pub rect: RECT,
}

/// The visible top-level windows of processes whose image is `claude.exe`.
pub fn app_windows() -> Vec<AppWindow> {
    let mut found: Vec<HWND> = Vec::new();
    unsafe extern "system" fn each(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let found = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
        if unsafe { IsWindowVisible(hwnd) }.as_bool() {
            found.push(hwnd);
        }
        BOOL(1)
    }
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut found as *mut _ as isize));
    }
    found
        .into_iter()
        .filter_map(|hwnd| {
            let pid = window_pid(hwnd);
            if !is_app_process(pid) {
                return None;
            }
            let mut rect = RECT::default();
            unsafe {
                let _ = GetWindowRect(hwnd, &mut rect);
            }
            Some(AppWindow {
                hwnd,
                pid,
                title: window_text(hwnd),
                class: window_class(hwnd),
                rect,
            })
        })
        .collect()
}

pub fn window_pid(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    pid
}

pub fn window_text(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

pub fn window_class(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

pub fn is_app_process(pid: u32) -> bool {
    process_image(pid).is_some_and(|p| p.to_ascii_lowercase().ends_with("\\claude.exe"))
}

/// The app's version, from its package folder's name, as in `Claude_5.11492.5.0_x64__<id>`.
pub fn app_version(pid: u32) -> String {
    let Some(image) = process_image(pid) else {
        return "?".to_string();
    };
    image
        .split('\\')
        .find_map(|part| part.strip_prefix("Claude_"))
        .and_then(|rest| rest.split('_').next())
        .unwrap_or("not a package")
        .to_string()
}

fn process_image(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(handle);
        ok.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// The foreground window, and whether it belongs to the app.
pub fn foreground() -> (HWND, bool) {
    let hwnd = unsafe { GetForegroundWindow() };
    (hwnd, is_app_process(window_pid(hwnd)))
}

/// Opens a `claude://` link the way `Start-Process` does, through the shell, on a thread of its
/// own initialized for single-threaded COM, which the shell asks of its callers.
pub fn open_link(url: &str) -> bool {
    let url = url.to_string();
    std::thread::spawn(move || unsafe {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        use windows::core::HSTRING;
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let result = ShellExecuteW(None, &HSTRING::from("open"), &HSTRING::from(url.as_str()), None, None, SW_SHOWNORMAL);
        result.0 as isize > 32
    })
    .join()
    .unwrap_or(false)
}

/// The command-line switches of the app's processes that mention accessibility, counted per
/// switch, so the report carries no paths.
pub fn accessibility_switches() -> Vec<(String, usize)> {
    let out = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance Win32_Process -Filter \"Name='claude.exe'\" | ForEach-Object { $_.CommandLine }",
        ])
        .output();
    let Ok(out) = out else {
        return vec![("(could not read command lines)".to_string(), 0)];
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let processes = text.lines().filter(|l| !l.trim().is_empty()).count();
    let mut counts: Vec<(String, usize)> = Vec::new();
    for line in text.lines() {
        for word in line.split_whitespace() {
            let switch = word.split('=').next().unwrap_or(word).trim_matches('"');
            if switch.starts_with("--") && switch.to_ascii_lowercase().contains("accessib") {
                match counts.iter_mut().find(|(s, _)| s == switch) {
                    Some((_, n)) => *n += 1,
                    None => counts.push((switch.to_string(), 1)),
                }
            }
        }
    }
    counts.insert(0, ("processes read".to_string(), processes));
    counts
}

/// Replaces the account name, home folder, machine name, and git email in what the probe
/// writes, since its output is committed to a repository that can go public.
pub struct Scrub(Vec<(String, &'static str)>);

impl Scrub {
    pub fn new() -> Scrub {
        let mut pairs = Vec::new();
        let var = |k: &str| std::env::var(k).ok().filter(|v| v.len() >= 3);
        if let Some(home) = var("USERPROFILE") {
            pairs.push((home.replace('\\', "/"), "<home>"));
            pairs.push((home.replace('\\', "\\\\"), "<home>"));
            pairs.push((home, "<home>"));
        }
        if let Some(email) = std::process::Command::new("git")
            .args(["config", "user.email"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|e| e.len() >= 3)
        {
            pairs.push((email, "<email>"));
        }
        if let Some(machine) = var("COMPUTERNAME") {
            pairs.push((machine, "<machine>"));
        }
        if let Some(user) = var("USERNAME") {
            pairs.push((user, "<user>"));
        }
        Scrub(pairs)
    }

    pub fn apply(&self, text: &str) -> String {
        let mut out = text.to_string();
        for (needle, with) in &self.0 {
            out = replace_ignoring_case(&out, needle, with);
        }
        out
    }
}

fn replace_ignoring_case(text: &str, needle: &str, with: &str) -> String {
    // ASCII lowercasing keeps every byte offset, so offsets found in the lowered copy cut the
    // original at the same places.
    let lower = text.to_ascii_lowercase();
    let needle = needle.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    while let Some(at) = lower[from..].find(&needle) {
        out.push_str(&text[from..from + at]);
        out.push_str(with);
        from += at + needle.len();
    }
    out.push_str(&text[from..]);
    out
}
