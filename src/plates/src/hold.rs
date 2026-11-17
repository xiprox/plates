//! Holding pings while you type: a timer's tone, a tile's brighten, and the docked strip's nudge
//! wait while you've typed in the last `QUIET`, and come together once you stop, once Claude hears
//! from you, or after `CAP`, as record/areas/rack/03-holding-pings.md gives it.
//!
//! Every ping goes through the hold, held and then given at once while you aren't typing, so a
//! ping that starts as earlier ones come is given with them, and none comes twice.

use std::time::{Duration, Instant, SystemTime};

/// How long Plates waits after your last typing key before it takes you as done typing, long
/// enough for a pause to think in the middle of a message (user, 2026-11-17).
pub const QUIET: Duration = Duration::from_secs(30);

/// How long at most a ping waits, so a long stretch of writing can't hold it for good (user,
/// 2026-11-17).
pub const CAP: Duration = Duration::from_secs(5 * 60);

#[derive(Default)]
pub struct Hold {
    /// When the first ping that waits started, or `None` while none waits. A tile that started
    /// needing you since then waits to brighten.
    since: Option<SystemTime>,
    /// The earliest time after which a timer ran out while pings waited, so the tone rings as
    /// they come for a timer that ran out since and is still time-up.
    ran_out_after: Option<SystemTime>,
    /// When Claude last heard from you, by a prompt you typed or an answer, which takes the typing
    /// before it as done.
    done_at: Option<Instant>,
}

/// The pings that waited and come now.
#[derive(Debug, PartialEq, Eq)]
pub struct Held {
    /// When the first started.
    pub since: SystemTime,
    /// The earliest time after which a timer ran out while they waited, if one did.
    pub ran_out_after: Option<SystemTime>,
}

impl Hold {
    /// Whether you're typing, from how long ago you last pressed a typing key.
    pub fn typing(&self, typed_ago: Option<Duration>) -> bool {
        typing(typed_ago, self.done_at.map(|at| at.elapsed()))
    }

    /// Makes a ping that started at `at` wait.
    pub fn hold(&mut self, at: SystemTime) {
        self.since = Some(self.since.map_or(at, |since| since.min(at)));
    }

    /// Makes the tone of a timer that ran out after `after` wait, with the ping it started at
    /// `at`.
    pub fn hold_ran_out(&mut self, after: SystemTime, at: SystemTime) {
        self.hold(at);
        self.ran_out_after = Some(self.ran_out_after.map_or(after, |a| a.min(after)));
    }

    /// Whether a tile that started needing you at `at` waits to brighten.
    pub fn holds(&self, at: SystemTime) -> bool {
        self.since.is_some_and(|since| at >= since)
    }

    /// Notes that Claude just heard from you.
    pub fn done(&mut self) {
        self.done_at = Some(Instant::now());
    }

    /// Takes the pings that wait, if they come now, since you've stopped typing or the first has
    /// waited `CAP`.
    pub fn take_due(&mut self, typing: bool, now: SystemTime) -> Option<Held> {
        let since = self.since?;
        if !due(since, typing, now) {
            return None;
        }
        self.since = None;
        Some(Held {
            since,
            ran_out_after: self.ran_out_after.take(),
        })
    }
}

// You're typing when you pressed a typing key in the last `QUIET`, and after Claude last heard
// from you, if it has.
fn typing(typed_ago: Option<Duration>, done_ago: Option<Duration>) -> bool {
    typed_ago.is_some_and(|ago| ago < QUIET && done_ago.is_none_or(|done| ago < done))
}

fn due(since: SystemTime, typing: bool, now: SystemTime) -> bool {
    !typing || now.duration_since(since).is_ok_and(|waited| waited >= CAP)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    #[test]
    fn typing_lasts_30s_after_the_last_key_unless_claude_heard_from_you_since() {
        assert!(!typing(None, None), "no key yet");
        assert!(typing(Some(secs(29)), None));
        assert!(!typing(Some(secs(30)), None));
        // A prompt sent 5s ago takes the key 10s ago as done, and a key 2s ago as typing again.
        assert!(!typing(Some(secs(10)), Some(secs(5))));
        assert!(typing(Some(secs(2)), Some(secs(5))));
    }

    #[test]
    fn pings_come_together_once_you_stop_or_after_5_minutes() {
        let t0 = SystemTime::UNIX_EPOCH + secs(1_000);
        let mut hold = Hold::default();
        assert!(hold.take_due(false, t0).is_none(), "nothing waits");

        hold.hold(t0 + secs(10));
        hold.hold_ran_out(t0 - secs(1), t0);
        assert!(hold.holds(t0 + secs(5)));
        assert!(
            !hold.holds(t0 - secs(1)),
            "a tile that needed you before brightened then"
        );
        assert!(hold.take_due(true, t0 + secs(299)).is_none());

        let held = Held {
            since: t0,
            ran_out_after: Some(t0 - secs(1)),
        };
        assert_eq!(hold.take_due(true, t0 + secs(300)), Some(held), "the cap");
        assert!(!hold.holds(t0 + secs(5)));

        hold.hold(t0 + secs(400));
        let held = hold
            .take_due(false, t0 + secs(401))
            .expect("you stopped typing");
        assert_eq!(
            held.ran_out_after, None,
            "the tone came with the earlier pings"
        );
    }
}
