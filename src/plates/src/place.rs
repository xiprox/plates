//! Where the rack sat when it last came to rest, so Plates opens it there again: `place.json`
//! beside `rack.json` in Plates' data folder, in physical pixels on the virtual screen, such as
//! `{"x":3200,"y":96,"docked":"right"}`.
//! A rack docked at an edge keeps the side and a point just inside that edge, so the strip comes
//! back on the same monitor.

use std::{fs, io, path::Path};

use serde_json::{Value, json};

use crate::motion::Side;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub x: f64,
    pub y: f64,
    pub docked: Option<Side>,
}

/// The saved place, or `None` when there's none or it doesn't parse, which opens the rack at the
/// screen's center.
pub fn load(path: &Path) -> Option<Place> {
    let v: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    let docked = match v.get("docked").and_then(Value::as_str) {
        Some("left") => Some(Side::Left),
        Some("right") => Some(Side::Right),
        _ => None,
    };
    Some(Place {
        x: v.get("x")?.as_f64()?,
        y: v.get("y")?.as_f64()?,
        docked,
    })
}

/// Writes the rack's place to `path`, so a crash mid-save leaves the old place whole.
pub fn save(place: Place, path: &Path) -> io::Result<()> {
    let docked = place.docked.map(|side| match side {
        Side::Left => "left",
        Side::Right => "right",
    });
    let body = json!({ "x": place.x.round(), "y": place.y.round(), "docked": docked });
    plates_core::store::write_whole(path, body.to_string().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_place_loads_back_and_a_broken_one_is_none() {
        let dir = std::env::temp_dir().join(format!("plates-place-{}", std::process::id()));
        let path = dir.join("place.json");
        let place = Place {
            x: 3200.,
            y: 96.,
            docked: Some(Side::Right),
        };
        save(place, &path).unwrap();
        assert_eq!(load(&path), Some(place));
        fs::write(&path, "{").unwrap();
        assert_eq!(load(&path), None);
    }
}
