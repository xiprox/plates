//! Holding pings while you're busy: a timer's tone, a tile's brighten, and the docked strip's
//! nudge wait while you've typed in the last `QUIET` or the Claude desktop app is in front, and
//! come together once you're neither, `SETTLE` after Claude hears from you, at once when you do
//! something on the rack, or after `CAP`, as record/areas/rack/03-holding-pings.md gives it.
//!
//! Every ping goes through the hold, held and then given at once while you aren't busy, so a
//! ping that starts as earlier ones come is given with them, and none comes twice.

use std::time::{Duration, Instant, SystemTime};

/// How long Plates waits after your last typing key before it takes you as done typing, long
/// enough for a pause to think in the middle of a message (user, 2026-11-17).
pub const QUIET: Duration = Duration::from_secs(30);

/// How long at most a ping waits, so a long stretch of writing or reading can't hold it for good
/// (user, 2026-11-17).
pub const CAP: Duration = Duration::from_secs(5 * 60);

/// How long after Claude hears from you the pings that waited come, so they follow your message
/// rather than land on the key that sent it (user, 2026-11-17).
pub const SETTLE: Duration = Duration::from_secs(3);

#[derive(Default)]
pub struct Hold {
    /// When the first ping that waits started, or `None` while none waits. A tile that started
    /// needing you since then waits to brighten.
    since: Option<SystemTime>,
    /// The earliest time after which a timer ran out while pings waited, so the tone rings as
    /// they come for a timer that ran out since and is still time-up.
    ran_out_after: Option<SystemTime>,
    /// When Claude last heard from you, by a prompt you typed or an answer, or you last did
    /// something on the rack.
    done: Option<Done>,
}

/// When you were last done with something, which lets the pings that started by then come.
#[derive(Clone, Copy)]
struct Done {
    /// When, by a clock a change of the PC's clock can't move, which takes the typing before it as
    /// done.
    instant: Instant,
    /// When, by the clock the pings' times are in.
    at: SystemTime,
    /// When the pings come: `SETTLE` after Claude heard from you, or at once for something you
    /// did on the rack, which you're looking at.
    ready: SystemTime,
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
    fn done_ago(&self) -> Option<Duration> {
        self.done.map(|done| done.instant.elapsed())
    }

    /// Whether any ping waits.
    pub fn waiting(&self) -> bool {
        self.since.is_some()
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

    /// Notes that Claude just heard from you, at `now`, so the pings that waited come `SETTLE`
    /// later, unless you type again first.
    pub fn heard_from_you(&mut self, now: SystemTime) {
        self.done(now, now + SETTLE);
    }

    /// Notes that you just did something on the rack, at `now`, so the pings that waited come at
    /// once, with a change that comes of it at the same `now`, such as a tile you marked unread.
    pub fn at_rack(&mut self, now: SystemTime) {
        self.done(now, now);
    }

    fn done(&mut self, at: SystemTime, ready: SystemTime) {
        self.done = Some(Done {
            instant: Instant::now(),
            at,
            ready,
        });
    }

    /// Takes the pings that wait, if they come now, from how long ago you last pressed a typing
    /// key and whether the Claude app is in front: see `due`.
    pub fn take_due(
        &mut self,
        typed_ago: Option<Duration>,
        claude_in_front: bool,
        now: SystemTime,
    ) -> Option<Held> {
        let since = self.since?;
        let done_ago = self.done_ago();
        let busy = typing(typed_ago, done_ago) || claude_in_front;
        let typed_since_done = typed_ago.is_some_and(|ago| done_ago.is_some_and(|done| ago < done));
        let done = self
            .done
            .filter(|_| !typed_since_done)
            .map(|done| (done.at, done.ready));
        if !due(since, busy, done, now) {
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

// The pings that started at `since` come when the first has waited `CAP`; or, once you were done
// at or after `since`, at the done's ready time; or else when you aren't `busy`, typing or having
// the Claude app in front. `done` is the done's time and ready time, or `None` if you've typed
// since it, which puts the pings back to waiting for you to stop being busy.
fn due(
    since: SystemTime,
    busy: bool,
    done: Option<(SystemTime, SystemTime)>,
    now: SystemTime,
) -> bool {
    if now.duration_since(since).is_ok_and(|waited| waited >= CAP) {
        return true;
    }
    match done {
        Some((at, ready)) if at >= since => now >= ready,
        _ => !busy,
    }
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
        assert!(hold.take_due(None, false, t0).is_none(), "nothing waits");

        hold.hold(t0 + secs(10));
        hold.hold_ran_out(t0 - secs(1), t0);
        assert!(hold.holds(t0 + secs(5)));
        assert!(
            !hold.holds(t0 - secs(1)),
            "a tile that needed you before brightened then"
        );
        assert!(hold.take_due(None, true, t0 + secs(299)).is_none());

        let held = Held {
            since: t0,
            ran_out_after: Some(t0 - secs(1)),
        };
        assert_eq!(
            hold.take_due(None, true, t0 + secs(300)),
            Some(held),
            "the cap"
        );
        assert!(!hold.holds(t0 + secs(5)));

        hold.hold(t0 + secs(400));
        let held = hold
            .take_due(None, false, t0 + secs(401))
            .expect("you stopped being busy");
        assert_eq!(
            held.ran_out_after, None,
            "the tone came with the earlier pings"
        );
    }

    #[test]
    fn pings_come_3s_after_claude_hears_from_you_unless_you_type_again() {
        let t0 = SystemTime::UNIX_EPOCH + secs(1_000);
        let since = t0 - secs(60);
        let done = Some((t0, t0 + SETTLE));
        // You sent a prompt at t0 with the Claude app in front: the pings settle for 3s first.
        assert!(!due(since, true, done, t0 + secs(2)));
        assert!(due(since, true, done, t0 + secs(3)));
        // With the Claude app not in front, they still settle first.
        assert!(!due(since, false, done, t0 + secs(1)));
        // Typing since the prompt puts them back to waiting for you to stop being busy.
        assert!(!due(since, true, None, t0 + secs(3)));
        assert!(due(since, false, None, t0 + secs(3)));
        // A done before the first ping started lets nothing through.
        assert!(!due(t0 + secs(5), true, done, t0 + secs(10)));
    }

    #[test]
    fn something_you_do_on_the_rack_lets_the_pings_through_at_once_with_its_own() {
        let t0 = SystemTime::UNIX_EPOCH + secs(1_000);
        let mut hold = Hold::default();
        hold.hold(t0);
        assert!(
            hold.take_due(None, true, t0 + secs(60)).is_none(),
            "reading Claude"
        );
        // A tile you mark unread starts needing you at the same time as the press.
        hold.at_rack(t0 + secs(61));
        hold.hold(t0 + secs(61));
        assert!(hold.take_due(None, true, t0 + secs(61)).is_some());

        // A ping that starts after it waits again while the Claude app is in front.
        hold.hold(t0 + secs(70));
        assert!(hold.waiting());
        assert!(hold.take_due(None, true, t0 + secs(71)).is_none());
    }
}
