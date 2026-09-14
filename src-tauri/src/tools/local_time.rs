//! The machine's own UTC offset, as ART's [`AmigaClock`] (ART-317).
//!
//! `core/clock.rs` declares the trait and this answers it, for the same reason
//! `recycle_bin.rs` beside it exists: the answer is Windows' (CLAUDE.md's
//! core-independence rule). `chrono::Local` asks `GetTimeZoneInformationForYear`
//! for the year of each instant, uncached, so the offset is the one in force on
//! that date and follows a time-zone change without a restart.

use chrono::{Local, TimeZone};

use crate::core::clock::{system_now_unix, AmigaClock};

/// Windows' time zone, per date. Stateless.
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsLocalTime;

/// The clock every product call site passes.
pub static LOCAL_TIME: WindowsLocalTime = WindowsLocalTime;

impl AmigaClock for WindowsLocalTime {
    fn now_unix(&self) -> i64 {
        system_now_unix()
    }

    /// `0` only for an instant outside chrono's ±262 000-year range, where no
    /// file or disc date can be.
    fn offset_at(&self, unix: i64) -> i32 {
        Local
            .timestamp_opt(unix, 0)
            .single()
            .map(|at| at.offset().local_minus_utc())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::clock::system_now_unix;

    /// What this can prove on CI, whose runners are UTC: the answer has the
    /// shape of a real offset. It cannot prove it is *this* machine's offset
    /// — the ignored test below does that.
    #[test]
    fn the_offset_is_a_whole_quarter_hour_inside_the_real_range() {
        for unix in [1_768_478_400, 1_784_116_800, LOCAL_TIME.now_unix()] {
            let offset = LOCAL_TIME.offset_at(unix);
            assert!(
                (-14 * 3_600..=14 * 3_600).contains(&offset),
                "{offset} at {unix}"
            );
            assert_eq!(offset % 900, 0, "{offset} at {unix}");
        }
    }

    #[test]
    fn now_is_the_system_clock() {
        let before = system_now_unix();
        let now = LOCAL_TIME.now_unix();
        assert!((before..=before + 2).contains(&now), "{before} vs {now}");
    }

    /// The real check, on the owner's machine: .NET's `TimeZoneInfo` is a
    /// separate implementation of the same registry rules. Istanbul observed
    /// DST until 2016, so 2015's January and July disagree there, and 2026's
    /// agree. Run with `ART_TZ_ORACLE=1`.
    #[test]
    #[ignore = "asks .NET TimeZoneInfo on this machine; set ART_TZ_ORACLE=1"]
    fn agrees_with_dotnet_timezoneinfo_across_seasons_and_years() {
        if std::env::var_os("ART_TZ_ORACLE").is_none() {
            return;
        }
        // 2015-01-01 12:00Z, 2015-07-15 12:00Z, 2026-01-15 12:00Z, 2026-07-15 12:00Z
        for unix in [
            1_420_113_600_i64,
            1_436_961_600,
            1_768_478_400,
            1_784_116_800,
        ] {
            let script = format!(
                "[int][System.TimeZoneInfo]::Local.GetUtcOffset([DateTimeOffset]::FromUnixTimeSeconds({unix})).TotalSeconds"
            );
            let out = std::process::Command::new("powershell")
                .args(["-NoProfile", "-Command", &script])
                .output()
                .expect("powershell runs");
            let dotnet: i32 = String::from_utf8_lossy(&out.stdout)
                .trim()
                .parse()
                .expect("an integer");
            println!(
                "{unix}: chrono {} / .NET {dotnet}",
                LOCAL_TIME.offset_at(unix)
            );
            assert_eq!(LOCAL_TIME.offset_at(unix), dotnet, "at {unix}");
            // M6 (final review): `unix_from_amiga` is proved against
            // `SeasonalClock` in `core::clock`'s own tests, never against the
            // real `chrono::Local` this machine actually runs. Round-trip
            // each instant this oracle already checks through the real
            // clock, on both sides of whichever DST boundary the machine
            // observes.
            assert_eq!(
                LOCAL_TIME.unix_from_amiga(LOCAL_TIME.amiga_from_unix(unix)),
                unix,
                "round trip at {unix}"
            );
        }
    }
}
