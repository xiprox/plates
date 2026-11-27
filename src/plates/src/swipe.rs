//! Swiping a tile sideways with two fingers on a trackpad to take it off the rack, as its menu's
//! last row does.
//!
//! The swipe arrives as scroll events with phases: `Started`, a `Moved` for each move, and `Ended`.
//! The `Moved` events after `Ended` are the scroll coasting, and the swipe ignores them.
//! On macOS, `Ended` comes as the fingers lift, which is unverified until a Mac run swipes a tile.
//! On Windows, GPUI sends `Ended` only after the coast, so there the tile follows the coast too.
//! The fingers don't move the pointer, so the tile slides out from under it, and the rack follows
//! the gesture from the tile under the pointer as the fingers touched.
//! (record/areas/rack/02-tiles.md cites the GPUI sources.)

use std::time::{Duration, Instant};

use plates_core::TileId;

/// How far the fingers move before the swipe decides whether it's sideways, in logical px.
const DECIDE: f32 = 8.;
/// How far a tile slides before letting go dismisses it, in logical px: about a third of a tile.
const DISMISS: f32 = 96.;
/// A release at this speed or faster dismisses a tile that slid less than `DISMISS`, in px/s, as
/// long as it slid `FLICK_MIN` the same way.
const FLICK: f32 = 600.;
const FLICK_MIN: f32 = 20.;
/// The span of moves the release speed is measured over: the rack's throw's, for the same reason.
const SPEED_SPAN: Duration = crate::motion::SAMPLE_SPAN;
/// Where a dismissed tile slides to, past any tile's width, in logical px.
pub const OUT: f32 = 320.;
/// How long a tile takes to slide back, or out, once the fingers lift.
pub const SETTLE: Duration = Duration::from_millis(180);
/// The farthest a held tile, a session with sub-sessions, slides however far the fingers go, in
/// logical px, and how hard it resists on the way: at `DISMISS`, the distance that would take any
/// other tile off, it has slid 23px.
const BAND: f32 = 40.;
const BAND_GIVE: f32 = 0.55;

/// What a swipe moves: a tile, or the Undo row a removed tile leaves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Swiped {
    Tile(TileId),
    Undo,
}

/// A swipe from the fingers touching until what it moves has settled.
pub struct Swipe {
    pub target: Swiped,
    /// The sub-sessions nested under a session's tile, which slide with it, since dismissing it
    /// takes them along.
    pub family: Vec<TileId>,
    /// Whether the swipe can't take its tile off: a session with sub-sessions, whose tile follows
    /// the fingers only against a growing pull, and springs back when they lift, since a swipe
    /// that took the sub-sessions along was too easy to make by accident (user, 2026-11-27).
    held: bool,
    dx: f32,
    dy: f32,
    /// Whether the fingers move sideways, once they've moved `DECIDE` to tell.
    sideways: Option<bool>,
    /// The last moves, for the release speed.
    moves: Vec<(Instant, f32)>,
    /// Once the fingers lift: where the tile slides from and to, and when it started.
    settle: Option<(f32, f32, Instant)>,
}

/// What lifting the fingers does.
#[derive(Debug, PartialEq)]
pub enum Release {
    /// The swipe never went sideways, or was already over.
    Nothing,
    /// The tile slides back into place.
    Back,
    /// The tile slides out and is dismissed.
    Dismiss,
}

impl Swipe {
    pub fn new(target: Swiped, family: Vec<TileId>) -> Self {
        Self {
            target,
            held: !family.is_empty(),
            family,
            dx: 0.,
            dy: 0.,
            sideways: None,
            moves: Vec::new(),
            settle: None,
        }
    }

    /// Adds a move of the fingers, and says whether this move decided the swipe goes sideways.
    /// Moves after the fingers lift are the scroll coasting, and change nothing.
    pub fn moved(&mut self, dx: f32, dy: f32, now: Instant) -> bool {
        if self.settle.is_some() || self.sideways == Some(false) {
            return false;
        }
        self.dx += dx;
        self.dy += dy;
        self.moves.push((now, self.dx));
        self.moves
            .retain(|(at, _)| now.duration_since(*at) <= SPEED_SPAN);
        if self.sideways.is_none() && self.dx.hypot(self.dy) >= DECIDE {
            let sideways = self.dx.abs() > self.dy.abs();
            self.sideways = Some(sideways);
            return sideways;
        }
        false
    }

    /// Lifts the fingers: the tile slides out if it slid far enough or was flicked, and back
    /// otherwise. A `cancel`, the OS taking the gesture, always slides it back.
    pub fn release(&mut self, cancel: bool, now: Instant) -> Release {
        if self.sideways != Some(true) || self.settle.is_some() {
            self.settle.get_or_insert((0., 0., now));
            return Release::Nothing;
        }
        // Only the moves of the last `SPEED_SPAN` before the lift count, so fingers that stopped
        // before lifting flick nothing.
        self.moves
            .retain(|(at, _)| now.duration_since(*at) <= SPEED_SPAN);
        let speed = match (self.moves.first(), self.moves.last()) {
            (Some(&(t0, x0)), Some(&(t1, x1))) if t1 > t0 => {
                (x1 - x0) / t1.duration_since(t0).as_secs_f32()
            }
            _ => 0.,
        };
        let flicked = speed.abs() >= FLICK
            && speed.signum() == self.dx.signum()
            && self.dx.abs() >= FLICK_MIN;
        let dismiss = !cancel && !self.held && (self.dx.abs() >= DISMISS || flicked);
        let to = if dismiss { OUT.copysign(self.dx) } else { 0. };
        self.settle = Some((self.slid(), to, now));
        if dismiss {
            Release::Dismiss
        } else {
            Release::Back
        }
    }

    /// How far the tile sits from its place at `now`, in logical px.
    pub fn offset(&self, now: Instant) -> f32 {
        match self.settle {
            Some((from, to, at)) => {
                let t = (now.duration_since(at).as_secs_f32() / SETTLE.as_secs_f32()).min(1.);
                from + (to - from) * (1. - (1. - t).powi(3))
            }
            None if self.sideways == Some(true) => self.slid(),
            None => 0.,
        }
    }

    // How far the fingers have slid the tile: as far as they moved, or for a held tile, a rubber
    // band's pull that nears `BAND`, as iOS pulls a scroll view past its end.
    fn slid(&self) -> f32 {
        if !self.held {
            return self.dx;
        }
        let pulled = BAND * (1. - 1. / (self.dx.abs() * BAND_GIVE / BAND + 1.));
        pulled.copysign(self.dx)
    }

    /// Whether the tile is still sliding after the fingers lifted.
    pub fn settling(&self, now: Instant) -> bool {
        self.settle
            .is_some_and(|(from, to, at)| from != to && now.duration_since(at) < SETTLE)
    }

    /// Whether the swipe dismissed its tile, which then slides out while it shrinks away.
    pub fn dismissed(&self) -> bool {
        self.settle.is_some_and(|(_, to, _)| to != 0.)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn swipe_by(steps: &[(f32, f32)], every: Duration) -> (Swipe, Instant) {
        let mut s = Swipe::new(Swiped::Tile(TileId(1)), Vec::new());
        let mut now = Instant::now();
        for &(dx, dy) in steps {
            now += every;
            s.moved(dx, dy, now);
        }
        (s, now)
    }

    #[test]
    fn a_slow_short_swipe_slides_back() {
        let (mut s, now) = swipe_by(&[(10., 0.); 5], Duration::from_millis(100));
        assert_eq!(s.offset(now), 50.);
        assert_eq!(s.release(false, now), Release::Back);
        assert_eq!(s.offset(now + SETTLE), 0.);
    }

    #[test]
    fn a_long_swipe_dismisses_either_way() {
        let (mut s, now) = swipe_by(&[(-20., 1.); 6], Duration::from_millis(100));
        assert_eq!(s.release(false, now), Release::Dismiss);
        assert_eq!(s.offset(now + SETTLE), -OUT);
    }

    #[test]
    fn a_flick_dismisses_before_the_distance() {
        let (mut s, now) = swipe_by(&[(15., 0.); 3], Duration::from_millis(16));
        assert_eq!(s.release(false, now), Release::Dismiss);
    }

    #[test]
    fn a_flick_that_stopped_before_the_lift_slides_back() {
        let (mut s, now) = swipe_by(&[(15., 0.); 3], Duration::from_millis(16));
        assert_eq!(
            s.release(false, now + Duration::from_millis(200)),
            Release::Back
        );
    }

    #[test]
    fn a_vertical_swipe_never_moves_the_tile() {
        let (mut s, now) = swipe_by(&[(2., 12.), (40., 0.)], Duration::from_millis(16));
        assert_eq!(s.offset(now), 0.);
        assert_eq!(s.release(false, now), Release::Nothing);
    }

    #[test]
    fn coasting_after_the_lift_changes_nothing() {
        let (mut s, now) = swipe_by(&[(10., 0.); 3], Duration::from_millis(100));
        assert_eq!(s.release(false, now), Release::Back);
        s.moved(200., 0., now);
        assert_eq!(s.offset(now + SETTLE), 0.);
    }

    #[test]
    fn a_session_with_sub_sessions_pulls_against_the_fingers_and_always_slides_back() {
        let mut s = Swipe::new(Swiped::Tile(TileId(1)), vec![TileId(2)]);
        let mut now = Instant::now();
        for _ in 0..10 {
            now += Duration::from_millis(16);
            s.moved(30., 0., now);
        }
        let slid = s.offset(now);
        assert!(slid > 0. && slid < BAND, "{slid}");
        assert_eq!(s.release(false, now), Release::Back);
        assert_eq!(s.offset(now), slid);
        assert_eq!(s.offset(now + SETTLE), 0.);
        assert!(!s.dismissed());
    }

    #[test]
    fn a_canceled_swipe_slides_back() {
        let (mut s, now) = swipe_by(&[(30., 0.); 5], Duration::from_millis(16));
        assert_eq!(s.release(true, now), Release::Back);
    }
}
