//! A tile's timer, which runs on the wall clock, so a timer that runs out while Plates is closed
//! or the PC sleeps is time-up at the first tick after.

use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

/// The minutes a click on the hourglass starts, and that a session tile starts on its own.
pub const DEFAULT_MINUTES: u32 = 5;

/// The lengths the hourglass's menu offers, in minutes.
pub const MENU_MINUTES: [u32; 6] = [5, 10, 15, 20, 30, 60];

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

    /// The timer as it stands at `now`: a running timer whose time has passed is time-up since
    /// the time it ran out, not since `now`.
    pub fn at(self, now: SystemTime) -> Self {
        match self {
            Timer::Running { ends, .. } if ends <= now => Timer::TimeUp { since: ends },
            other => other,
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self, Timer::Running { .. })
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

    /// The time left on a running timer at `now`, for its tooltip.
    pub fn remaining(&self, now: SystemTime) -> Option<Duration> {
        match *self {
            Timer::Running { ends, .. } => Some(ends.duration_since(now).unwrap_or_default()),
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
