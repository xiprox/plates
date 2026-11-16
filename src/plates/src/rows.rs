//! Tiles growing into the rack and shrinking out of it, so the tiles below a change slide rather
//! than jump. GPUI lays the rack out afresh each frame and keeps nothing of a tile that's gone, so
//! the rack keeps a copy of each tile it drew, and draws a removed tile from that copy while it
//! shrinks. A row's height runs between 0 and its full 66px, a 60px tile and the 6px below it, and
//! the window's height follows the rows.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use plates_core::{Tile, TileId};

/// How long a tile, or the Undo row, takes to grow in.
pub const ENTER: Duration = Duration::from_millis(220);
/// How long a tile, or the Undo row, takes to shrink out.
pub const LEAVE: Duration = Duration::from_millis(180);

/// A tile that's gone from the rack, drawn from its last copy while it shrinks.
pub struct Leaving {
    pub tile: Tile,
    /// The tile it sat above, which it's drawn above while it shrinks, or `None` for the last.
    pub before: Option<TileId>,
    since: Instant,
}

impl Leaving {
    pub fn factor(&self, now: Instant) -> f32 {
        1. - ease_in_out(progress(now, self.since, LEAVE))
    }
}

struct Undo {
    title: String,
    since: Instant,
    leaving: bool,
}

#[derive(Default)]
pub struct Rows {
    /// The tiles the last frame drew, or `None` before the first.
    shown: Option<Vec<Tile>>,
    entering: HashMap<TileId, Instant>,
    leaving: Vec<Leaving>,
    undo: Option<Undo>,
}

impl Rows {
    /// Compares `tiles` and the Undo row's title with the last frame's, and starts a row growing
    /// or shrinking for each change. With `animate` false, such as while the rack is docked and
    /// the tiles aren't drawn, each change takes effect at once.
    pub fn track(&mut self, tiles: &[Tile], undo_title: Option<&str>, animate: bool, now: Instant) {
        if let Some(shown) = &self.shown
            && animate
        {
            let present = |id: TileId| tiles.iter().any(|t| t.id == id);
            for (k, old) in shown.iter().enumerate() {
                if !present(old.id) {
                    let before = shown[k + 1..].iter().map(|t| t.id).find(|&id| present(id));
                    self.leaving.push(Leaving {
                        tile: old.clone(),
                        before,
                        since: now,
                    });
                }
            }
            for t in tiles {
                if shown.iter().all(|old| old.id != t.id) {
                    self.entering.insert(t.id, now);
                }
            }
        }
        // A tile that comes back, as Undo brings it, grows in again rather than still shrinking.
        self.leaving.retain(|l| {
            tiles.iter().all(|t| t.id != l.tile.id) && now.duration_since(l.since) < LEAVE
        });
        self.entering
            .retain(|_, &mut at| now.duration_since(at) < ENTER);
        self.shown = Some(tiles.to_vec());

        match (undo_title, &mut self.undo) {
            (Some(title), Some(undo)) if !undo.leaving => undo.title = title.to_string(),
            (Some(title), _) => {
                self.undo = Some(Undo {
                    title: title.to_string(),
                    since: if animate { now } else { now - ENTER },
                    leaving: false,
                })
            }
            (None, Some(undo)) if !undo.leaving => {
                undo.leaving = true;
                undo.since = if animate { now } else { now - LEAVE };
            }
            (None, _) => {}
        }
        if self
            .undo
            .as_ref()
            .is_some_and(|u| u.leaving && now.duration_since(u.since) >= LEAVE)
        {
            self.undo = None;
        }
    }

    /// How far a tile has grown in, from 0 to 1.
    pub fn factor(&self, id: TileId, now: Instant) -> f32 {
        self.entering
            .get(&id)
            .map_or(1., |&at| ease_out(progress(now, at, ENTER)))
    }

    pub fn leaving(&self) -> &[Leaving] {
        &self.leaving
    }

    /// The Undo row's title and how far it has grown, while it shows or shrinks.
    pub fn undo(&self, now: Instant) -> Option<(&str, f32)> {
        let undo = self.undo.as_ref()?;
        let factor = if undo.leaving {
            1. - ease_in_out(progress(now, undo.since, LEAVE))
        } else {
            ease_out(progress(now, undo.since, ENTER))
        };
        Some((&undo.title, factor))
    }

    /// The tile rows' height, in rows, with a growing or shrinking row counted by how far it's
    /// grown.
    pub fn tile_rows(&self, tiles: &[Tile], now: Instant) -> f32 {
        let present: f32 = tiles.iter().map(|t| self.factor(t.id, now)).sum();
        let leaving: f32 = self.leaving.iter().map(|l| l.factor(now)).sum();
        present + leaving
    }

    pub fn animating(&self, now: Instant) -> bool {
        !self.entering.is_empty()
            || !self.leaving.is_empty()
            || self
                .undo
                .as_ref()
                .is_some_and(|u| u.leaving || now.duration_since(u.since) < ENTER)
    }
}

fn progress(now: Instant, since: Instant, over: Duration) -> f32 {
    (now.duration_since(since).as_secs_f32() / over.as_secs_f32()).clamp(0., 1.)
}

// Cubic easing: a row that grows in starts fast and settles, and one that shrinks out starts and
// ends gently, so it leaves without a snap at either end.
fn ease_out(t: f32) -> f32 {
    1. - (1. - t).powi(3)
}

fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 {
        4. * t * t * t
    } else {
        1. - (-2. * t + 2.).powi(3) / 2.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plates_core::Rack;
    use std::time::SystemTime;

    fn rack_with(titles: &[&str]) -> Rack {
        let mut rack = Rack::new();
        for t in titles {
            rack.add_own(t, SystemTime::UNIX_EPOCH);
        }
        rack
    }

    #[test]
    fn a_new_tile_grows_in_and_a_removed_one_shrinks_out_above_its_neighbor() {
        let t0 = Instant::now();
        let mut rack = rack_with(&["A", "B"]);
        let mut rows = Rows::default();
        rows.track(rack.tiles(), None, true, t0);
        assert!(!rows.animating(t0), "the first frame draws what's there");

        let c = rack.add_own("C", SystemTime::UNIX_EPOCH);
        rows.track(rack.tiles(), None, true, t0);
        assert_eq!(rows.factor(c, t0), 0.);
        assert_eq!(rows.factor(c, t0 + ENTER), 1.);

        let a = rack.tiles()[0].id;
        let b = rack.tiles()[1].id;
        let removed = rack.remove(a, SystemTime::UNIX_EPOCH).unwrap();
        let later = t0 + ENTER;
        rows.track(rack.tiles(), Some("A"), true, later);
        assert_eq!(rows.leaving().len(), 1);
        assert_eq!(rows.leaving()[0].before, Some(b));
        assert_eq!(rows.tile_rows(rack.tiles(), later), 3.);
        assert_eq!(
            rows.undo(later).map(|(t, f)| (t.to_string(), f)),
            Some(("A".into(), 0.))
        );

        rack.restore(removed);
        rows.track(rack.tiles(), None, true, later + LEAVE / 2);
        assert!(rows.leaving().is_empty(), "Undo brings the tile back");
        assert_eq!(rows.factor(a, later + LEAVE / 2), 0.);
    }

    #[test]
    fn changes_while_docked_take_effect_at_once() {
        let t0 = Instant::now();
        let mut rack = rack_with(&["A"]);
        let mut rows = Rows::default();
        rows.track(rack.tiles(), None, true, t0);
        rack.add_own("B", SystemTime::UNIX_EPOCH);
        rows.track(rack.tiles(), None, false, t0);
        assert!(!rows.animating(t0));
        assert_eq!(rows.tile_rows(rack.tiles(), t0), 2.);
    }
}
