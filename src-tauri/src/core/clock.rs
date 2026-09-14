//! Which clock an Amiga date is read against (ART-317).
//!
//! AmigaDOS `DateStamp()` is local wall time with no zone, and pfs3aio stamps
//! with it. An instant — a host file's mtime, a disc's recording date, "now" —
//! becomes an Amiga date only through the offset **in force on that date**,
//! and that offset is a platform question. So `core/` declares the trait and
//! `tools/local_time.rs` implements it (CLAUDE.md's core-independence rule).
//! `UtcClock` is ART's behaviour before ART-317, kept for tests and for
//! callers that read no date.

use crate::core::adf::bcpl::{AmigaDate, AMIGA_EPOCH_UNIX, TICKS_PER_SEC};

/// Where "now" and the UTC offset come from.
pub trait AmigaClock: Send + Sync {
    /// Seconds since 1970-01-01 UTC.
    fn now_unix(&self) -> i64;

    /// Seconds east of UTC in force at the UTC instant `unix`.
    fn offset_at(&self, unix: i64) -> i32;

    /// The Amiga date a wall clock here showed at the instant `unix`.
    fn amiga_from_unix(&self, unix: i64) -> AmigaDate {
        amiga_from_wall(unix + i64::from(self.offset_at(unix)))
    }

    /// Now, as AmigaDOS would stamp it.
    fn amiga_now(&self) -> AmigaDate {
        self.amiga_from_unix(self.now_unix())
    }

    /// The instant an Amiga date names. Two passes: the first guesses the
    /// instant with the offset at the wall time, the second corrects with the
    /// offset at that guess, which is right on both sides of a DST switch. A
    /// wall time that happens twice (autumn) resolves to one of them, and one
    /// that never happens (spring) to the nearest instant.
    fn unix_from_amiga(&self, date: AmigaDate) -> i64 {
        let wall = date.to_wall_seconds();
        let first = wall - i64::from(self.offset_at(wall));
        wall - i64::from(self.offset_at(first))
    }
}

/// The Amiga triplet for wall-clock seconds since 1970. Anything before 1978
/// clamps to the epoch rather than wrapping (§4.1): a negative day count shows
/// as a date far in the future on a real Amiga.
pub fn amiga_from_wall(wall: i64) -> AmigaDate {
    let since = (wall - AMIGA_EPOCH_UNIX).max(0);
    AmigaDate {
        days: (since / 86_400) as u32,
        mins: ((since % 86_400) / 60) as u32,
        ticks: ((since % 60) * TICKS_PER_SEC) as u32,
    }
}

/// The system clock, in whole seconds since 1970 UTC. A clock set before
/// 1970 reads as the Amiga epoch, as ART's writers always did.
pub fn system_now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(AMIGA_EPOCH_UNIX)
}

/// UTC: ART's behaviour before ART-317.
#[derive(Debug, Default, Clone, Copy)]
pub struct UtcClock;

impl AmigaClock for UtcClock {
    fn now_unix(&self) -> i64 {
        system_now_unix()
    }
    fn offset_at(&self, _unix: i64) -> i32 {
        0
    }
}

/// A stopped clock with one offset — deterministic stamping.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock {
    pub now: i64,
    pub offset: i32,
}

impl AmigaClock for FixedClock {
    fn now_unix(&self) -> i64 {
        self.now
    }
    fn offset_at(&self, _unix: i64) -> i32 {
        self.offset
    }
}

/// A stopped clock with one DST switch: `before` until `switch_at`, `after`
/// from it on.
#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub struct SeasonalClock {
    pub now: i64,
    pub switch_at: i64,
    pub before: i32,
    pub after: i32,
}

#[cfg(test)]
impl AmigaClock for SeasonalClock {
    fn now_unix(&self) -> i64 {
        self.now
    }
    fn offset_at(&self, unix: i64) -> i32 {
        if unix >= self.switch_at {
            self.after
        } else {
            self.before
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::adf::bcpl::{AmigaDate, AMIGA_EPOCH_UNIX};

    const JAN_15_NOON_UTC: i64 = 1_768_478_400;
    const JUL_15_NOON_UTC: i64 = 1_784_116_800;
    const EU_SPRING_SWITCH: i64 = 1_774_746_000;

    fn seasons() -> SeasonalClock {
        SeasonalClock {
            now: JUL_15_NOON_UTC,
            switch_at: EU_SPRING_SWITCH,
            before: 7_200,
            after: 10_800,
        }
    }

    #[test]
    fn utc_changes_nothing() {
        let date = UtcClock.amiga_from_unix(JAN_15_NOON_UTC);
        assert_eq!(
            date,
            AmigaDate {
                days: 17_546,
                mins: 720,
                ticks: 0
            }
        );
        assert_eq!(UtcClock.unix_from_amiga(date), JAN_15_NOON_UTC);
    }

    #[test]
    fn a_fixed_offset_moves_the_wall_clock_and_comes_back() {
        let clock = FixedClock {
            now: JAN_15_NOON_UTC,
            offset: 10_800,
        };
        assert_eq!(
            clock.amiga_now(),
            AmigaDate {
                days: 17_546,
                mins: 900,
                ticks: 0
            }
        );
        assert_eq!(clock.unix_from_amiga(clock.amiga_now()), JAN_15_NOON_UTC);
    }

    /// ART-317's DST rule: the offset in force on the date itself, not today's.
    /// `FileTimeToLocalFileTime` gets exactly this wrong (Microsoft Learn, "File Times").
    #[test]
    fn the_offset_is_the_one_in_force_at_the_date_not_now() {
        let clock = seasons();
        assert_eq!(
            clock.amiga_from_unix(JAN_15_NOON_UTC).mins,
            14 * 60,
            "winter, UTC+2"
        );
        assert_eq!(
            clock.amiga_from_unix(JUL_15_NOON_UTC).mins,
            15 * 60,
            "summer, UTC+3"
        );
    }

    #[test]
    fn a_round_trip_survives_both_sides_of_the_switch() {
        let clock = seasons();
        for unix in [
            EU_SPRING_SWITCH - 1_800,
            EU_SPRING_SWITCH,
            EU_SPRING_SWITCH + 1_800,
            JAN_15_NOON_UTC,
            JUL_15_NOON_UTC,
        ] {
            assert_eq!(
                clock.unix_from_amiga(clock.amiga_from_unix(unix)),
                unix,
                "at {unix}"
            );
        }
    }

    #[test]
    fn a_date_before_the_amiga_epoch_clamps_after_the_offset() {
        let clock = FixedClock {
            now: 0,
            offset: -3_600,
        };
        assert_eq!(
            clock.amiga_from_unix(AMIGA_EPOCH_UNIX),
            AmigaDate::default()
        );
    }
}
