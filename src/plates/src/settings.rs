//! Plates' settings that belong to no tile: `settings.json` beside `rack.json` in Plates' data
//! folder, such as `{"sound":false}`. A missing or broken file, or a missing setting, takes the
//! default.

use std::{fs, io, path::Path};

use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Whether a timer that runs out rings the tone, which the bell in the grip band turns on
    /// and off.
    pub sound: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { sound: true }
    }
}

pub fn load(path: &Path) -> Settings {
    let saved: Option<Value> = fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let default = Settings::default();
    Settings {
        sound: saved
            .as_ref()
            .and_then(|v| v.get("sound"))
            .and_then(Value::as_bool)
            .unwrap_or(default.sound),
    }
}

/// Writes a new file beside the old one and renames it over the old one, as rack.json's save
/// does, so a crash mid-save leaves the old settings whole.
pub fn save(settings: Settings, path: &Path) -> io::Result<()> {
    let body = json!({ "sound": settings.sound });
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let temp = path.with_extension("json.new");
    fs::write(&temp, body.to_string())?;
    fs::rename(&temp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_settings_load_back_and_a_broken_file_takes_the_default() {
        let dir = std::env::temp_dir().join(format!("plates-settings-{}", std::process::id()));
        let path = dir.join("settings.json");
        assert_eq!(load(&path), Settings::default());
        save(Settings { sound: false }, &path).unwrap();
        assert_eq!(load(&path), Settings { sound: false });
        fs::write(&path, "{").unwrap();
        assert_eq!(load(&path), Settings::default());
    }
}
