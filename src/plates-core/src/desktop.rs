//! The Claude desktop app's own file for each of its sessions, which holds the title its sidebar
//! shows, generated or given, the app's own ID for the session, and the Claude Code session IDs
//! the hooks carry.
//!
//! The app doesn't document these files (record/areas/hooks/log.md), so what Plates reads from
//! them is a nicety: when a file is missing or unreadable, the tile falls back to its other
//! titles, and clicking it opens nothing.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

/// How long a session ID that no file names waits before another search: `FIRST_RETRY` after its
/// first miss, doubling after each miss up to `RETRY_AFTER`. A new sub-session's file names its
/// parent only after the session's first hook events (record/areas/hooks/log.md, 2026-11-16), so
/// the first retries come soon, and a session the app doesn't keep, such as one in a terminal,
/// soon searches only every 30s rather than on every event.
const FIRST_RETRY: Duration = Duration::from_secs(2);
const RETRY_AFTER: Duration = Duration::from_secs(30);

/// The folder of the app's session files: on Windows,
/// `%LOCALAPPDATA%\Claude-Data\claude_<package id>\claude-code-sessions\`, for the app installed
/// as a Windows package (measured on Windows 11 25H2, 2026-11-15), and on macOS,
/// `~/Library/Application Support/Claude/claude-code-sessions/` (measured on macOS 26.5.2,
/// 2026-11-16). Both hold each session's file a few folders down.
pub fn sessions_folder() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        let folder = dirs::config_dir()?
            .join("Claude")
            .join("claude-code-sessions");
        return folder.is_dir().then_some(folder);
    }
    if !cfg!(windows) {
        return None;
    }
    let data = dirs::data_local_dir()?.join("Claude-Data");
    fs::read_dir(&data)
        .ok()?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("claude_"))
        .map(|e| e.path().join("claude-code-sessions"))
        .find(|p| p.is_dir())
}

/// What the app's file says about one session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppSession {
    /// The title the app shows for the session.
    pub title: Option<String>,
    /// The app's own ID for the session, such as `local_e4c2b1b0-16ce-4c53-be73-25a5e5fc8bd2`.
    pub id: Option<String>,
    /// When you last opened the session in the app, its `lastFocusedAt`. The app sets it each
    /// time the session becomes visible, and not while you keep watching it (read in the Claude
    /// desktop app 5.11492.5.0, app.asar, the handler that logs
    /// `desktop_ccd_session_visibility_changed`).
    pub focused_at: Option<SystemTime>,
    /// The app's own ID for the session this one was started from, its `spawnedFrom.sessionId`,
    /// which the app writes for a session one of its sessions started (measured on Windows 11
    /// 25H2, 2026-11-15, record/areas/hooks/log.md).
    pub parent: Option<String>,
}

/// The link that opens a session in the app by its own ID:
/// `claude://code/continue?session=local_<id>`. The app takes only an ID that matches
/// `^local_[A-Za-z0-9-]{1,64}$`, and opens the session unless it's archived (read in the Claude
/// desktop app 5.11492.5.0, app.asar, its `claude://` link handler; measured on Windows 11 25H2,
/// 2026-11-15, record/areas/hooks/log.md).
pub fn session_link(app_id: &str) -> Option<String> {
    let rest = app_id.strip_prefix("local_")?;
    let valid = (1..=64).contains(&rest.len())
        && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    valid.then(|| format!("claude://code/continue?session={app_id}"))
}

/// Finds and reads the app's file for a Claude Code session ID.
pub struct AppSessions {
    folder: Option<PathBuf>,
    files: HashMap<String, PathBuf>,
    misses: HashMap<String, Miss>,
    read: HashMap<PathBuf, (SystemTime, Option<AppSession>)>,
}

/// A search that found no file for a session ID.
struct Miss {
    at: Instant,
    /// The wall-clock time the search began, since a file last modified before then was read in
    /// that search and can't name the ID.
    began: SystemTime,
    /// How long until the next search.
    wait: Duration,
}

impl AppSessions {
    pub fn new(folder: Option<PathBuf>) -> Self {
        AppSessions {
            folder,
            files: HashMap::new(),
            misses: HashMap::new(),
            read: HashMap::new(),
        }
    }

    /// What the app's file says about the session, or `None` when no file of the app's names it.
    pub fn get(&mut self, session_id: &str) -> Option<AppSession> {
        let file = match self.files.get(session_id) {
            Some(file) => file.clone(),
            None => {
                let file = self.find(session_id)?;
                self.files.insert(session_id.to_string(), file.clone());
                file
            }
        };
        let modified = fs::metadata(&file).and_then(|m| m.modified()).ok()?;
        if let Some((at, session)) = self.read.get(&file)
            && *at == modified
        {
            return session.clone();
        }
        let session = read_session(&file, session_id);
        self.read.insert(file, (modified, session.clone()));
        session
    }

    fn find(&mut self, session_id: &str) -> Option<PathBuf> {
        let (since, wait) = match self.misses.get(session_id) {
            Some(miss) if miss.at.elapsed() < miss.wait => return None,
            Some(miss) => (Some(miss.began), (miss.wait * 2).min(RETRY_AFTER)),
            None => (None, FIRST_RETRY),
        };
        let began = SystemTime::now();
        let found = self
            .folder
            .as_deref()
            .and_then(|folder| search(folder, session_id, since));
        if found.is_none() {
            let at = Instant::now();
            self.misses
                .insert(session_id.to_string(), Miss { at, began, wait });
        }
        found
    }
}

// The app keeps hundreds of files of about 300KB each, 101MB in all on our PC, so reading them
// all takes about 200ms (measured on Windows 11 25H2, 2026-11-15). The app writes a session's
// file as the session runs, so the search reads the newest first, and after a miss it reads only
// the files changed since. It looks for the ID in each file's bytes, and parses only a file that
// holds it, to check the ID is the session's own.
fn search(folder: &Path, session_id: &str, since: Option<SystemTime>) -> Option<PathBuf> {
    let needle = session_id.as_bytes();
    if needle.is_empty() {
        return None;
    }
    let mut files = Vec::new();
    let mut dirs = vec![folder.to_path_buf()];
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
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !(name.starts_with("local_") && name.ends_with(".json")) {
                continue;
            }
            let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else {
                continue;
            };
            if since.is_none_or(|since| modified >= since) {
                files.push((modified, path));
            }
        }
    }
    files.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    files.into_iter().map(|(_, path)| path).find(|path| {
        fs::read(path).is_ok_and(|bytes| {
            bytes.windows(needle.len()).any(|w| w == needle) && names_session(&bytes, session_id)
        })
    })
}

fn names_session(bytes: &[u8], session_id: &str) -> bool {
    let Ok(v) = serde_json::from_slice::<Value>(bytes) else {
        return false;
    };
    v.get("cliSessionId").and_then(Value::as_str) == Some(session_id)
        || v.get("priorCliSessionIds")
            .and_then(Value::as_array)
            .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(session_id)))
}

fn read_session(file: &Path, session_id: &str) -> Option<AppSession> {
    let bytes = fs::read(file).ok()?;
    if !names_session(&bytes, session_id) {
        return None;
    }
    let v: Value = serde_json::from_slice(&bytes).ok()?;
    let text = |key: &str| {
        v.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string)
    };
    Some(AppSession {
        title: text("title"),
        id: text("sessionId"),
        focused_at: v
            .get("lastFocusedAt")
            .and_then(Value::as_u64)
            .map(|ms| UNIX_EPOCH + Duration::from_millis(ms)),
        parent: v
            .get("spawnedFrom")
            .and_then(|s| s.get("sessionId"))
            .and_then(Value::as_str)
            .filter(|id| id.starts_with("local_"))
            .map(str::to_string),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn folder(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("plates-core-desktop-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("account").join("org")).unwrap();
        dir
    }

    fn write(dir: &Path, name: &str, body: Value) -> PathBuf {
        let path = dir.join("account").join("org").join(name);
        fs::write(&path, body.to_string()).unwrap();
        path
    }

    fn title(sessions: &mut AppSessions, session_id: &str) -> Option<String> {
        sessions.get(session_id).and_then(|s| s.title)
    }

    #[test]
    fn a_session_gives_its_app_id_and_a_link_that_opens_it() {
        let dir = folder("id");
        write(
            &dir,
            "local_7.json",
            json!({ "cliSessionId": "abc", "sessionId": "local_e4c2b1b0-16ce", "title": "Plates",
                    "lastFocusedAt": 1794772321608u64 }),
        );
        let session = AppSessions::new(Some(dir.clone())).get("abc").unwrap();
        assert_eq!(session.id.as_deref(), Some("local_e4c2b1b0-16ce"));
        assert_eq!(
            session.focused_at,
            Some(UNIX_EPOCH + Duration::from_millis(1794772321608))
        );
        assert_eq!(
            session_link("local_e4c2b1b0-16ce").as_deref(),
            Some("claude://code/continue?session=local_e4c2b1b0-16ce")
        );
        assert_eq!(session.parent, None);
        write(
            &dir,
            "local_8.json",
            json!({ "cliSessionId": "child", "sessionId": "local_f00d",
                    "spawnedFrom": { "sessionId": "local_e4c2b1b0-16ce", "taskId": "task_25ace5e8",
                                     "sideSession": true } }),
        );
        let child = AppSessions::new(Some(dir)).get("child").unwrap();
        assert_eq!(child.parent.as_deref(), Some("local_e4c2b1b0-16ce"));
        assert_eq!(session_link("local_"), None);
        assert_eq!(session_link("local_a&b=c"), None);
        assert_eq!(session_link("cse_abc"), None);
    }

    #[test]
    fn a_session_takes_the_title_of_the_file_that_names_it() {
        let dir = folder("title");
        write(
            &dir,
            "local_1.json",
            json!({ "cliSessionId": "other", "title": "Other", "note": "mentions abc only here" }),
        );
        write(
            &dir,
            "local_2.json",
            json!({ "cliSessionId": "abc", "title": "Dock tuning", "titleSource": "auto" }),
        );
        let mut titles = AppSessions::new(Some(dir));
        assert_eq!(title(&mut titles, "abc").as_deref(), Some("Dock tuning"));
        assert_eq!(title(&mut titles, "missing"), None);
    }

    #[test]
    fn a_cleared_sessions_earlier_id_finds_the_same_file() {
        let dir = folder("prior");
        write(
            &dir,
            "local_3.json",
            json!({ "cliSessionId": "new", "priorCliSessionIds": ["old"], "title": "Hooks" }),
        );
        let mut titles = AppSessions::new(Some(dir));
        assert_eq!(title(&mut titles, "old").as_deref(), Some("Hooks"));
    }

    #[test]
    fn a_changed_file_is_read_again() {
        let dir = folder("changed");
        let path = write(
            &dir,
            "local_4.json",
            json!({ "cliSessionId": "abc", "title": "First" }),
        );
        let mut titles = AppSessions::new(Some(dir));
        assert_eq!(title(&mut titles, "abc").as_deref(), Some("First"));
        std::thread::sleep(Duration::from_millis(20));
        fs::write(
            &path,
            json!({ "cliSessionId": "abc", "title": "Second" }).to_string(),
        )
        .unwrap();
        let later = SystemTime::now() + Duration::from_secs(5);
        let file = fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(later).unwrap();
        assert_eq!(title(&mut titles, "abc").as_deref(), Some("Second"));
    }

    #[test]
    fn an_empty_title_and_no_folder_give_none() {
        let dir = folder("empty");
        write(
            &dir,
            "local_5.json",
            json!({ "cliSessionId": "abc", "title": "  " }),
        );
        assert_eq!(title(&mut AppSessions::new(Some(dir.clone())), "abc"), None);
        assert_eq!(title(&mut AppSessions::new(Some(dir)), ""), None);
        assert_eq!(title(&mut AppSessions::new(None), "abc"), None);
    }

    #[test]
    fn a_search_after_a_miss_reads_only_the_files_changed_since() {
        let dir = folder("since");
        let path = write(
            &dir,
            "local_6.json",
            json!({ "cliSessionId": "abc", "title": "Old" }),
        );
        let file = fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(60))
            .unwrap();
        let since = SystemTime::now() - Duration::from_secs(30);
        assert_eq!(search(&dir, "abc", Some(since)), None);
        assert_eq!(search(&dir, "abc", None), Some(path));
    }
}
