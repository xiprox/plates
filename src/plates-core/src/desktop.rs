//! The Claude desktop app's own file for each of its sessions, which holds the title its sidebar
//! shows, generated or given, and the Claude Code session IDs the hooks carry.
//!
//! The app doesn't document these files (record/areas/hooks/log.md), so a title read from them is
//! a nicety: when a file is missing or unreadable, the tile falls back to its other titles.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

use serde_json::Value;

/// How long a session ID that no file names waits before another search, so a session the app
/// doesn't keep, such as one in a terminal, doesn't set off a search on every event.
const RETRY_AFTER: Duration = Duration::from_secs(30);

/// The folder of the app's session files on Windows,
/// `%LOCALAPPDATA%\Claude-Data\claude_<package id>\claude-code-sessions\`, for the app installed
/// as a Windows package (measured on Windows 11 25H2, 2026-11-15). Where the app keeps them on
/// macOS is unverified, so there it finds none.
pub fn sessions_folder() -> Option<PathBuf> {
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

/// Finds and reads the app's title for a Claude Code session ID.
pub struct Titles {
    folder: Option<PathBuf>,
    files: HashMap<String, PathBuf>,
    misses: HashMap<String, Miss>,
    read: HashMap<PathBuf, (SystemTime, Option<String>)>,
}

/// A search that found no file for a session ID.
struct Miss {
    at: Instant,
    /// The wall-clock time the search began, since a file last modified before then was read in
    /// that search and can't name the ID.
    began: SystemTime,
}

impl Titles {
    pub fn new(folder: Option<PathBuf>) -> Self {
        Titles {
            folder,
            files: HashMap::new(),
            misses: HashMap::new(),
            read: HashMap::new(),
        }
    }

    /// The title the app shows for the session, or `None` when no file of the app's names it.
    pub fn title(&mut self, session_id: &str) -> Option<String> {
        let file = match self.files.get(session_id) {
            Some(file) => file.clone(),
            None => {
                let file = self.find(session_id)?;
                self.files.insert(session_id.to_string(), file.clone());
                file
            }
        };
        let modified = fs::metadata(&file).and_then(|m| m.modified()).ok()?;
        if let Some((at, title)) = self.read.get(&file)
            && *at == modified
        {
            return title.clone();
        }
        let title = read_title(&file, session_id);
        self.read.insert(file, (modified, title.clone()));
        title
    }

    fn find(&mut self, session_id: &str) -> Option<PathBuf> {
        let since = match self.misses.get(session_id) {
            Some(miss) if miss.at.elapsed() < RETRY_AFTER => return None,
            Some(miss) => Some(miss.began),
            None => None,
        };
        let began = SystemTime::now();
        let found = self
            .folder
            .as_deref()
            .and_then(|folder| search(folder, session_id, since));
        if found.is_none() {
            let at = Instant::now();
            self.misses
                .insert(session_id.to_string(), Miss { at, began });
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

fn read_title(file: &Path, session_id: &str) -> Option<String> {
    let bytes = fs::read(file).ok()?;
    if !names_session(&bytes, session_id) {
        return None;
    }
    let v: Value = serde_json::from_slice(&bytes).ok()?;
    v.get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
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
        let mut titles = Titles::new(Some(dir));
        assert_eq!(titles.title("abc").as_deref(), Some("Dock tuning"));
        assert_eq!(titles.title("missing"), None);
    }

    #[test]
    fn a_cleared_sessions_earlier_id_finds_the_same_file() {
        let dir = folder("prior");
        write(
            &dir,
            "local_3.json",
            json!({ "cliSessionId": "new", "priorCliSessionIds": ["old"], "title": "Hooks" }),
        );
        let mut titles = Titles::new(Some(dir));
        assert_eq!(titles.title("old").as_deref(), Some("Hooks"));
    }

    #[test]
    fn a_changed_file_is_read_again() {
        let dir = folder("changed");
        let path = write(
            &dir,
            "local_4.json",
            json!({ "cliSessionId": "abc", "title": "First" }),
        );
        let mut titles = Titles::new(Some(dir));
        assert_eq!(titles.title("abc").as_deref(), Some("First"));
        std::thread::sleep(Duration::from_millis(20));
        fs::write(
            &path,
            json!({ "cliSessionId": "abc", "title": "Second" }).to_string(),
        )
        .unwrap();
        let later = SystemTime::now() + Duration::from_secs(5);
        let file = fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(later).unwrap();
        assert_eq!(titles.title("abc").as_deref(), Some("Second"));
    }

    #[test]
    fn an_empty_title_and_no_folder_give_none() {
        let dir = folder("empty");
        write(
            &dir,
            "local_5.json",
            json!({ "cliSessionId": "abc", "title": "  " }),
        );
        assert_eq!(Titles::new(Some(dir.clone())).title("abc"), None);
        assert_eq!(Titles::new(Some(dir)).title(""), None);
        assert_eq!(Titles::new(None).title("abc"), None);
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
