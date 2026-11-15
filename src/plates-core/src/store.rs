//! Saving your own tiles and their timers in `rack.json`, and loading them back.
//! Session tiles aren't saved, since a session's next hook event brings its tile back.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{rack::Rack, timer::Timer};

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
