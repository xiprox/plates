//! A tile's timer, which runs on the wall clock, so a timer that runs out while Plates is closed
//! or the PC sleeps is time-up at the first tick after.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// The minutes a click on the hourglass starts, and that a session tile starts on its own.
pub const DEFAULT_MINUTES: u32 = 5;

/// The lengths in minutes the hourglass's menu offers to remind you in, which the hover card
/// offers as chips too.
pub const MENU_MINUTES: [u32; 5] = [5, 10, 15, 20, 30];

/// The lengths in hours the hourglass's menu offers to remind you in, after the minutes.
pub const MENU_HOURS: [u32; 1] = [1];

/// The lengths in hours the hourglass's menu offers to snooze a tile for, before Tomorrow
/// (user, 2026-11-16 and 2026-11-27).
pub const SNOOZE_HOURS: [u32; 3] = [1, 2, 4];

/// When a reminder for tomorrow runs out: 4:00 local time. Before 4:00, a night that went past
/// midnight still counts as the day before, so tomorrow is the coming 4:00 (user, 2026-11-16).
pub const MORNING: Duration = Duration::from_secs(4 * 3600);

const DAY: i64 = 24 * 3600;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum Timer {
    #[default]
    Idle,
    Running {
        #[serde(with = "unix_ms")]
        started: SystemTime,
        #[serde(with = "unix_ms")]
        ends: SystemTime,
    },
    /// A reminder for later: tomorrow, which runs out at `MORNING`, or a snooze's hours from
    /// when you set it. Unlike a running timer, a session's change of state doesn't stop or
    /// replace it, since it's for when you're back.
    Tomorrow {
        #[serde(with = "unix_ms")]
        ends: SystemTime,
    },
    TimeUp {
        #[serde(with = "unix_ms")]
        since: SystemTime,
    },
}

impl Timer {
    pub fn start(now: SystemTime, minutes: u32) -> Self {
        Timer::Running {
            started: now,
            ends: now + Duration::from_secs(u64::from(minutes) * 60),
        }
    }

    /// A reminder for tomorrow, from `now`, where the local time is `utc_offset` seconds ahead
    /// of UTC, as 10,800 is for UTC+3.
    pub fn tomorrow(now: SystemTime, utc_offset: i64) -> Self {
        let secs = now
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let morning = MORNING.as_secs() as i64;
        // Counted from the morning, the local day that's still going, and then the next one.
        let today = (secs + utc_offset - morning).div_euclid(DAY);
        let ends = (today + 1) * DAY + morning - utc_offset;
        Timer::Tomorrow {
            ends: UNIX_EPOCH + Duration::from_secs(ends.max(0) as u64),
        }
    }

    /// A reminder for `hours` from `now`, which a snoozed tile waits for.
    pub fn later(now: SystemTime, hours: u32) -> Self {
        Timer::Tomorrow {
            ends: now + Duration::from_secs(u64::from(hours) * 3600),
        }
    }

    /// When a reminder for later runs out, for the list of snoozed tiles.
    pub fn later_ends(&self) -> Option<SystemTime> {
        match *self {
            Timer::Tomorrow { ends } => Some(ends),
            _ => None,
        }
    }

    /// The timer as it stands at `now`: a running timer or a reminder for tomorrow whose time has
    /// passed is time-up since the time it ran out, not since `now`.
    pub fn at(self, now: SystemTime) -> Self {
        match self {
            Timer::Running { ends, .. } | Timer::Tomorrow { ends } if ends <= now => {
                Timer::TimeUp { since: ends }
            }
            other => other,
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self, Timer::Running { .. })
    }

    pub fn is_tomorrow(&self) -> bool {
        matches!(self, Timer::Tomorrow { .. })
    }

    /// Stops a running timer, or ends time-up, as reading or dismissing a session does, and
    /// leaves a reminder for tomorrow, which is for when you're back.
    pub fn stop(&mut self) {
        if !self.is_tomorrow() {
            *self = Timer::Idle;
        }
    }

    /// Whether a timer or a reminder for tomorrow is counting down, which a click on the timer
    /// slot cancels.
    pub fn is_set(&self) -> bool {
        self.is_running() || self.is_tomorrow()
    }

    pub fn is_time_up(&self) -> bool {
        matches!(self, Timer::TimeUp { .. })
    }

    /// The share of a running timer's time that's left at `now`, from 1 at the start to 0, which
    /// the rack draws as the ring.
    pub fn left(&self, now: SystemTime) -> Option<f32> {
        let Timer::Running { started, ends } = *self else {
            return None;
        };
        let total = ends.duration_since(started).ok()?.as_secs_f32();
        let left = ends.duration_since(now).unwrap_or_default().as_secs_f32();
        Some(if total > 0.0 {
            (left / total).min(1.0)
        } else {
            0.0
        })
    }

    /// The time left on a running timer or a reminder for tomorrow at `now`.
    pub fn remaining(&self, now: SystemTime) -> Option<Duration> {
        match *self {
            Timer::Running { ends, .. } | Timer::Tomorrow { ends } => {
                Some(ends.duration_since(now).unwrap_or_default())
            }
            _ => None,
        }
    }
}

/// A `SystemTime` saved as whole milliseconds since 1970, which is what `rack.json` holds.
mod unix_ms {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(t: &SystemTime, s: S) -> Result<S::Ok, S::Error> {
        let ms = t
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        s.serialize_u64(ms)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<SystemTime, D::Error> {
        Ok(UNIX_EPOCH + Duration::from_millis(u64::deserialize(d)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    fn at(s: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_800_000_000 + s)
    }

    #[test]
    fn a_started_timer_runs_for_its_minutes() {
        let t = Timer::start(at(0), 5);
        assert_eq!(
            t,
            Timer::Running {
                started: at(0),
                ends: at(300)
            }
        );
        assert_eq!(t.at(at(299)), t);
        assert_eq!(t.left(at(150)), Some(0.5));
        assert_eq!(t.remaining(at(240)), Some(Duration::from_secs(60)));
    }

    #[test]
    fn a_timer_is_time_up_since_it_ran_out_not_since_the_tick() {
        let t = Timer::start(at(0), 5);
        assert_eq!(t.at(at(300)), Timer::TimeUp { since: at(300) });
        // A tick hours later, after the PC slept, still dates time-up from when it ran out.
        assert_eq!(t.at(at(9_000)), Timer::TimeUp { since: at(300) });
    }

    #[test]
    fn tomorrow_runs_out_at_the_coming_morning_after_today() {
        // 2026-11-15T20:00:00Z is 23:00 in UTC+3.
        let utc = |text: &str| crate::transcript::parse_timestamp(text).unwrap();
        let plus3 = 3 * 3600;
        let ends = |now: &str| match Timer::tomorrow(utc(now), plus3) {
            Timer::Tomorrow { ends } => ends,
            other => panic!("expected a reminder for tomorrow, got {other:?}"),
        };
        // 4:00 local on the 16th is 01:00Z.
        assert_eq!(ends("2026-11-15T20:00:00Z"), utc("2026-11-16T01:00:00Z"));
        assert_eq!(
            ends("2026-11-15T23:30:00Z"),
            utc("2026-11-16T01:00:00Z"),
            "02:30 local is still the night of the 15th"
        );
        assert_eq!(
            ends("2026-11-16T01:00:00Z"),
            utc("2026-11-17T01:00:00Z"),
            "from 4:00 local on, tomorrow is the next day"
        );
        assert_eq!(
            Timer::tomorrow(utc("2026-11-15T20:00:00Z"), plus3).at(utc("2026-11-16T06:00:00Z")),
            Timer::TimeUp {
                since: utc("2026-11-16T01:00:00Z")
            }
        );
    }

    #[test]
    fn idle_and_time_up_timers_stay_as_they_are() {
        assert_eq!(Timer::Idle.at(at(1_000)), Timer::Idle);
        let up = Timer::TimeUp { since: at(10) };
        assert_eq!(up.at(at(1_000)), up);
        assert_eq!(up.left(at(1_000)), None);
    }

    #[test]
    fn a_timer_saves_and_loads_to_the_millisecond() {
        let t = Timer::start(UNIX_EPOCH + Duration::from_millis(1_800_000_000_123), 10);
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(
            json,
            r#"{"state":"running","started":1800000000123,"ends":1800000600123}"#
        );
        assert_eq!(serde_json::from_str::<Timer>(&json).unwrap(), t);
    }
}
