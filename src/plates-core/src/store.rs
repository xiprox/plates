//! Saving your own tiles and their timers in `rack.json`, and session tiles in `sessions.json`,
//! and loading them back. Session tiles are saved because a session that finished before Plates
//! restarted sends no event to bring its tile back (user, 2026-11-15).

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{
    rack::Rack,
    session::{BackgroundTask, Session, SessionState, WrapUp},
    timer::Timer,
};

/// The version `rack.json` is written in, which a later format checks before reading.
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Saved {
    version: u32,
    own_tiles: Vec<SavedTile>,
}

#[derive(Serialize, Deserialize)]
struct SavedTile {
    title: String,
    timer: Timer,
}

/// What loading found: the rack, and where a file that didn't parse was moved to.
#[derive(Debug)]
pub struct Loaded {
    pub rack: Rack,
    pub broken: Option<PathBuf>,
}

/// Writes your own tiles to `path`.
/// It writes a new file beside the old one and renames it over the old one, so a crash mid-save
/// leaves the old file whole.
pub fn save(rack: &Rack, path: &Path) -> io::Result<()> {
    let saved = Saved {
        version: VERSION,
        own_tiles: rack
            .own_tiles()
            .map(|(title, timer)| SavedTile {
                title: title.to_string(),
                timer,
            })
            .collect(),
    };
    let json = serde_json::to_vec_pretty(&saved).map_err(io::Error::other)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let temp = path.with_extension("json.new");
    fs::write(&temp, json)?;
    fs::rename(&temp, path)
}

/// Reads your own tiles from `path`.
/// A missing file is an empty rack. A file that doesn't parse is renamed `rack.json.broken`, so
/// the next save can't overwrite what you had, and the rack starts empty.
pub fn load(path: &Path) -> io::Result<Loaded> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(Loaded {
                rack: Rack::new(),
                broken: None,
            });
        }
        Err(e) => return Err(e),
    };
    match serde_json::from_slice::<Saved>(&bytes) {
        Ok(saved) if saved.version == VERSION => Ok(Loaded {
            rack: Rack::with_own_tiles(saved.own_tiles.into_iter().map(|t| (t.title, t.timer))),
            broken: None,
        }),
        _ => {
            let broken = path.with_extension("json.broken");
            fs::rename(path, &broken)?;
            Ok(Loaded {
                rack: Rack::new(),
                broken: Some(broken),
            })
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SavedSessions {
    version: u32,
    sessions: Vec<SavedSession>,
}

/// A session tile as `sessions.json` holds it: what the tile shows between turns. What changes
/// within a turn, such as the activity line, isn't kept, since the session's next event brings
/// it again.
#[derive(Serialize, Deserialize)]
struct SavedSession {
    id: String,
    folder: PathBuf,
    transcript: Option<PathBuf>,
    app_title: Option<String>,
    app_id: Option<String>,
    custom_title: Option<String>,
    first_prompt: Option<String>,
    state: SessionState,
    /// Milliseconds since 1970, as `rack.json` writes its times.
    state_since: u64,
    summary: Option<String>,
    /// A file without these, such as one an older Plates wrote, loads with them empty.
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    wrap_up: Option<WrapUp>,
    #[serde(default)]
    background: Vec<BackgroundTask>,
    asks: Option<bool>,
    read: bool,
    ended_at: Option<u64>,
    timer: Timer,
    dismissed: bool,
}

fn ms(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn time(ms: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(ms)
}

/// Writes the rack's session tiles, shown and dismissed, to `path`, as `save` writes your own.
pub fn save_sessions(rack: &Rack, path: &Path) -> io::Result<()> {
    let sessions = rack
        .session_tiles()
        .filter_map(|(tile, dismissed)| {
            let s = tile.session()?;
            Some(SavedSession {
                id: s.id.clone(),
                folder: s.folder.clone(),
                transcript: s.transcript.clone(),
                app_title: s.app_title.clone(),
                app_id: s.app_id.clone(),
                custom_title: s.custom_title.clone(),
                first_prompt: s.first_prompt.clone(),
                state: s.state,
                state_since: ms(s.state_since),
                summary: s.summary.clone(),
                message: s.message.clone(),
                wrap_up: s.wrap_up.clone(),
                background: s.background.clone(),
                asks: s.asks,
                read: s.read,
                ended_at: s.ended_at.map(ms),
                timer: tile.timer,
                dismissed,
            })
        })
        .collect();
    let saved = SavedSessions {
        version: VERSION,
        sessions,
    };
    let json = serde_json::to_vec_pretty(&saved).map_err(io::Error::other)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let temp = path.with_extension("json.new");
    fs::write(&temp, json)?;
    fs::rename(&temp, path)
}

/// Puts the session tiles saved in `path` back on the rack, and returns where a file that didn't
/// parse was moved to. A missing file restores nothing.
pub fn load_sessions(rack: &mut Rack, path: &Path, now: SystemTime) -> io::Result<Option<PathBuf>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let saved = match serde_json::from_slice::<SavedSessions>(&bytes) {
        Ok(saved) if saved.version == VERSION => saved,
        _ => {
            let broken = path.with_extension("json.broken");
            fs::rename(path, &broken)?;
            return Ok(Some(broken));
        }
    };
    for s in saved.sessions {
        let mut session = Session::new(&s.id, &s.folder, time(s.state_since));
        session.transcript = s.transcript;
        session.app_title = s.app_title;
        session.app_id = s.app_id;
        session.custom_title = s.custom_title;
        session.first_prompt = s.first_prompt;
        session.state = s.state;
        session.summary = s.summary;
        session.message = s.message;
        session.wrap_up = s.wrap_up;
        session.background = s.background;
        session.asks = s.asks;
        session.read = s.read;
        session.ended_at = s.ended_at.map(time);
        session.refresh_line(now);
        rack.restore_session(session, s.timer, s.dismissed, now);
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionEvent;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    // A folder of the test's own under the OS's temporary folder, emptied first.
    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("plates-core-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn at(ms: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_millis(1_800_000_000_000 + ms)
    }

    #[test]
    fn a_saved_rack_loads_back_the_same() {
        let path = folder("round-trip").join("rack.json");
        let mut rack = Rack::new();
        rack.apply(
            "a",
            Path::new("/work"),
            SessionEvent::Started { title: None },
            at(0),
        );
        let ci = rack.add_own("Check release CI", at(0));
        rack.start_timer(ci, 10, at(1_234));
        let domain = rack.add_own("Renew the domain", at(0));
        rack.start_timer(domain, 5, at(0));
        rack.tick(at(400_000));
        rack.add_own("Waiting on review", at(0));

        save(&rack, &path).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.broken, None);
        let saved: Vec<_> = rack.own_tiles().collect();
        let back: Vec<_> = loaded.rack.own_tiles().collect();
        assert_eq!(back, saved);
        assert_eq!(loaded.rack.tiles().len(), 3, "session tiles aren't saved");
    }

    #[test]
    fn saved_sessions_load_back_shown_and_dismissed() {
        let path = folder("sessions").join("sessions.json");
        let mut rack = Rack::new();
        let a = rack.apply(
            "a",
            Path::new("/work/a"),
            SessionEvent::TurnEnded {
                last_message: Some("Commit it?".into()),
                background: Vec::new(),
            },
            at(1_000),
        );
        rack.set_transcript("a", Path::new("/t/a.jsonl"));
        let b = rack.apply(
            "b",
            Path::new("/work/b"),
            SessionEvent::Started { title: None },
            at(0),
        );
        rack.remove(b);
        rack.add_own("Check release CI", at(0));
        save_sessions(&rack, &path).unwrap();

        let mut back = Rack::new();
        assert_eq!(load_sessions(&mut back, &path, at(2_000)).unwrap(), None);
        let shown: Vec<_> = back.session_tiles().filter(|(_, d)| !d).collect();
        assert_eq!(shown.len(), 1);
        let s = shown[0].0.session().unwrap();
        assert_eq!(s.id, "a");
        assert_eq!(s.state, SessionState::YourTurn);
        assert_eq!(s.summary.as_deref(), Some("Commit it?"));
        assert_eq!(s.message.as_deref(), Some("Commit it?"));
        assert_eq!(s.state_since, at(1_000));
        assert_eq!(s.transcript.as_deref(), Some(Path::new("/t/a.jsonl")));
        assert_eq!(shown[0].0.timer, rack.tile(a).unwrap().timer);
        assert!(back.knows("b"), "a dismissed session stays dismissed");
        assert_eq!(back.tiles().len(), 1);
    }

    #[test]
    fn a_missing_file_is_an_empty_rack() {
        let loaded = load(&folder("missing").join("rack.json")).unwrap();
        assert!(loaded.rack.tiles().is_empty());
        assert_eq!(loaded.broken, None);
    }

    #[test]
    fn a_file_that_doesnt_parse_is_moved_aside_and_kept() {
        let dir = folder("broken");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rack.json");
        fs::write(&path, b"{ not json").unwrap();
        let loaded = load(&path).unwrap();
        assert!(loaded.rack.tiles().is_empty());
        let broken = loaded.broken.unwrap();
        assert_eq!(broken, dir.join("rack.json.broken"));
        assert_eq!(fs::read(&broken).unwrap(), b"{ not json");
        assert!(!path.exists());
    }

    #[test]
    fn a_time_up_timer_saved_while_closed_needs_you_from_the_first_tick() {
        let path = folder("closed").join("rack.json");
        let mut rack = Rack::new();
        let id = rack.add_own("Check release CI", at(0));
        rack.start_timer(id, 5, at(0));
        save(&rack, &path).unwrap();

        let mut back = load(&path).unwrap().rack;
        assert!(back.tick(at(3_600_000)));
        let tile = &back.tiles()[0];
        assert_eq!(tile.timer, Timer::TimeUp { since: at(300_000) });
        assert!(tile.need.is_some());
    }
}
