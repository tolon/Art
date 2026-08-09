//! `.uaem` sidecars: the Amiga metadata Windows cannot hold (brief §4.2).
//!
//! Windows has no protection bits, no file comment, and no 1/50-second ticks.
//! Copying `Game.slave` out of an image and back in would therefore lose its
//! `S` and `P` bits, which WHDLoad packs actually depend on (§7.2).
//!
//! WinUAE solved this for directory hardfiles by writing a sidecar next to each
//! file: `Game.slave` gets `Game.slave.uaem`. ART uses **the same format**, so
//! a folder ART extracts round-trips through WinUAE and back into ART without
//! losing anything.
//!
//! ## The format
//!
//! One line, space-separated:
//!
//! ```text
//! ----rwed 2026-08-09 14:30:00.00 the file's comment
//! ^^^^^^^^ ^^^^^^^^^^^^^^^^^^^^^^ ^^^^^^^^^^^^^^^^^^
//! bits     date, ticks as .NN     everything after the third space
//! ```
//!
//! The eight bits are `HSPARWED` in that order, each a letter when set and `-`
//! when not — **except** that `RWED` are stored inverted in the header, so a
//! zero bit means the permission is granted and the letter shows. That
//! inversion is the single easiest thing to get backwards, and getting it
//! backwards produces files nobody can read.
//!
//! ART writes the format WinUAE writes and reads what WinUAE writes. It does
//! not invent fields: a sidecar with more on the line keeps whatever it had in
//! the comment, because that is what the format says the rest of the line is.

use crate::core::adf::bcpl::AmigaDate;
use crate::core::error::{CoreError, CoreResult};

/// The extension WinUAE uses.
pub const UAEM_EXTENSION: &str = "uaem";

/// The eight protection bits, most significant first.
///
/// `HSPARWED`: Hold, Script, Pure, Archive, Read, Write, Execute, Delete.
pub const BIT_LETTERS: [char; 8] = ['h', 's', 'p', 'a', 'r', 'w', 'e', 'd'];

/// Bit 7 down to bit 0, matching [`BIT_LETTERS`].
const BIT_VALUES: [u32; 8] = [
    1 << 7, // H
    1 << 6, // S
    1 << 5, // P
    1 << 4, // A
    1 << 3, // R
    1 << 2, // W
    1 << 1, // E
    1,      // D
];

/// The low four bits — `RWED` — are stored inverted.
const INVERTED_MASK: u32 = 0x0F;

/// The longest sidecar ART will read.
///
/// A comment is 79 characters and the rest of the line is fixed, so anything
/// past this is not a sidecar.
pub const MAX_UAEM_BYTES: u64 = 4096;

/// The most a comment field holds: 80 bytes as a BSTR, so 79 characters.
pub const MAX_COMMENT_LEN: usize = 79;

/// Everything a sidecar carries.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Sidecar {
    /// `HSPARWED` as stored in the header block, inversion included.
    pub protection: u32,
    pub date: AmigaDate,
    pub comment: String,
}

/// The sidecar path for a file.
pub fn sidecar_path(file: &std::path::Path) -> std::path::PathBuf {
    let mut name = file.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(UAEM_EXTENSION);
    file.with_file_name(name)
}

/// Render the protection bits as `hsparwed`, in WinUAE's spelling.
pub fn format_bits(protection: u32) -> String {
    BIT_LETTERS
        .iter()
        .zip(BIT_VALUES)
        .map(|(letter, value)| {
            // RWED are inverted in the stored field: a clear bit means the
            // permission is granted, so the letter shows.
            let set = if value & INVERTED_MASK != 0 {
                protection & value == 0
            } else {
                protection & value != 0
            };
            if set {
                *letter
            } else {
                '-'
            }
        })
        .collect()
}

/// Parse `hsparwed` back into the stored field.
pub fn parse_bits(text: &str) -> CoreResult<u32> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() != 8 {
        return Err(CoreError::Malformed {
            format: "uaem".into(),
            detail: format!("protection bits must be 8 characters, got '{text}'"),
        });
    }

    let mut protection = 0u32;
    for (index, (letter, value)) in BIT_LETTERS.iter().zip(BIT_VALUES).enumerate() {
        let shown = chars[index].to_ascii_lowercase() == *letter;
        if !shown && chars[index] != '-' {
            return Err(CoreError::Malformed {
                format: "uaem".into(),
                detail: format!(
                    "'{}' is not '{letter}' or '-' in position {}",
                    chars[index],
                    index + 1
                ),
            });
        }

        let stored = if value & INVERTED_MASK != 0 {
            !shown
        } else {
            shown
        };
        if stored {
            protection |= value;
        }
    }
    Ok(protection)
}

/// Write a sidecar's single line.
///
/// The date is rendered from the Amiga triplet through Unix time, so it says
/// what an Amiga would say rather than what the host clock thinks.
pub fn render(sidecar: &Sidecar) -> String {
    let (date, time) = civil_from_amiga(sidecar.date);
    let comment: String = sidecar
        .comment
        .chars()
        .filter(|c| *c != '\n' && *c != '\r')
        .take(MAX_COMMENT_LEN)
        .collect();

    format!(
        "{} {date} {time} {comment}\n",
        format_bits(sidecar.protection)
    )
}

/// Read a sidecar back.
///
/// A line ART cannot parse is an error rather than a silent default: applying
/// `----rwed` to a file whose sidecar said `--p--rw-` would quietly change
/// what the file is, and for a WHDLoad slave that is the difference between
/// working and not.
pub fn parse(text: &str) -> CoreResult<Sidecar> {
    let line = text.lines().next().unwrap_or("").trim_end();
    if line.is_empty() {
        return Err(CoreError::Malformed {
            format: "uaem".into(),
            detail: "the sidecar is empty".into(),
        });
    }

    let mut parts = line.splitn(4, ' ');
    let bits = parts.next().unwrap_or("");
    let date = parts.next().unwrap_or("");
    let time = parts.next().unwrap_or("");
    let comment = parts.next().unwrap_or("");

    let protection = parse_bits(bits)?;
    let date = amiga_from_civil(date, time)?;

    Ok(Sidecar {
        protection,
        date,
        comment: comment.chars().take(MAX_COMMENT_LEN).collect(),
    })
}

/// `YYYY-MM-DD` and `HH:MM:SS.TT` from an Amiga triplet.
fn civil_from_amiga(date: AmigaDate) -> (String, String) {
    let (year, month, day) = civil_from_days(date.days as i64 + AMIGA_EPOCH_DAYS);
    let hours = date.mins / 60;
    let minutes = date.mins % 60;
    let seconds = date.ticks / 50;
    let hundredths = (date.ticks % 50) * 2;

    (
        format!("{year:04}-{month:02}-{day:02}"),
        format!("{hours:02}:{minutes:02}:{seconds:02}.{hundredths:02}"),
    )
}

fn amiga_from_civil(date: &str, time: &str) -> CoreResult<AmigaDate> {
    let bad = |what: &str| CoreError::Malformed {
        format: "uaem".into(),
        detail: format!("'{what}' is not a date ART can read"),
    };

    let mut date_parts = date.split('-');
    let year: i64 = date_parts
        .next()
        .unwrap_or("")
        .parse()
        .map_err(|_| bad(date))?;
    let month: i64 = date_parts
        .next()
        .unwrap_or("")
        .parse()
        .map_err(|_| bad(date))?;
    let day: i64 = date_parts
        .next()
        .unwrap_or("")
        .parse()
        .map_err(|_| bad(date))?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(bad(date));
    }

    let mut time_parts = time.split(':');
    let hours: u32 = time_parts
        .next()
        .unwrap_or("")
        .parse()
        .map_err(|_| bad(time))?;
    let minutes: u32 = time_parts
        .next()
        .unwrap_or("")
        .parse()
        .map_err(|_| bad(time))?;
    let rest = time_parts.next().unwrap_or("0");
    let mut seconds_parts = rest.split('.');
    let seconds: u32 = seconds_parts
        .next()
        .unwrap_or("")
        .parse()
        .map_err(|_| bad(time))?;
    let hundredths: u32 = seconds_parts.next().unwrap_or("0").parse().unwrap_or(0);

    if hours > 23 || minutes > 59 || seconds > 59 {
        return Err(bad(time));
    }

    let days = days_from_civil(year, month, day) - AMIGA_EPOCH_DAYS;
    if days < 0 {
        // Before 1978. Clamped rather than refused: a sidecar written by a
        // tool with a wrong clock should not stop a copy.
        return Ok(AmigaDate {
            days: 0,
            mins: 0,
            ticks: 0,
        });
    }

    Ok(AmigaDate {
        days: days as u32,
        mins: hours * 60 + minutes,
        ticks: seconds * 50 + hundredths / 2,
    })
}

/// Days from 1970-01-01 to 1978-01-01.
const AMIGA_EPOCH_DAYS: i64 = 2922;

/// Howard Hinnant's `days_from_civil`, the standard branch-free civil-calendar
/// conversion. Written out rather than pulled in as a dependency: `core/` takes
/// `std` + serde + sha2 + thiserror + delharc and nothing else.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The inverse of [`days_from_civil`].
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u32;
    let month = (mp + if mp < 10 { 3 } else { -9 }) as u32;

    (year + i64::from(month <= 2), month, day)
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// The inversion, in both directions. Getting it backwards produces files
    /// nobody can read — and it looks right until someone boots the disk.
    #[test]
    fn the_default_protection_reads_as_rwed() {
        assert_eq!(format_bits(0), "----rwed");
        assert_eq!(parse_bits("----rwed").unwrap(), 0);
    }

    #[test]
    fn every_bit_set_reads_as_hspa_with_no_permissions() {
        // All eight bits set: HSPA show, and RWED are inverted so they do not.
        assert_eq!(format_bits(0xFF), "hspa----");
        assert_eq!(parse_bits("hspa----").unwrap(), 0xFF);
    }

    /// §7.2: WHDLoad packs rely on `S` and `P`, so they have to survive a
    /// round trip exactly.
    #[test]
    fn the_script_and_pure_bits_survive_a_round_trip() {
        let script_and_pure = (1 << 6) | (1 << 5);
        assert_eq!(format_bits(script_and_pure), "-sp-rwed");
        assert_eq!(parse_bits("-sp-rwed").unwrap(), script_and_pure);
    }

    #[test]
    fn every_possible_bit_pattern_round_trips() {
        for protection in 0u32..=0xFF {
            let text = format_bits(protection);
            assert_eq!(
                parse_bits(&text).unwrap(),
                protection,
                "'{text}' did not come back as {protection:#04x}"
            );
        }
    }

    #[test]
    fn a_protection_string_of_the_wrong_shape_is_refused() {
        assert!(parse_bits("rwed").is_err());
        assert!(parse_bits("----rwedx").is_err());
        assert!(parse_bits("----rwzd").is_err());
    }

    #[test]
    fn a_sidecar_round_trips_whole() {
        let sidecar = Sidecar {
            protection: (1 << 6) | (1 << 5),
            date: AmigaDate {
                days: 17_752,
                mins: 14 * 60 + 30,
                ticks: 25 * 50 + 10,
            },
            comment: "Slave for the 1993 release".into(),
        };

        let text = render(&sidecar);
        assert_eq!(parse(&text).unwrap(), sidecar);
    }

    #[test]
    fn a_comment_with_spaces_survives() {
        let sidecar = Sidecar {
            comment: "several words in a row".into(),
            ..Default::default()
        };
        assert_eq!(parse(&render(&sidecar)).unwrap().comment, sidecar.comment);
    }

    /// AmigaDOS comments are Latin-1, and so is everything else about a name;
    /// the sidecar has to carry them without mangling.
    #[test]
    fn a_latin_1_comment_survives() {
        let sidecar = Sidecar {
            comment: "Grüße aus München — ¾ fertig".into(),
            ..Default::default()
        };
        assert_eq!(parse(&render(&sidecar)).unwrap().comment, sidecar.comment);
    }

    #[test]
    fn a_comment_longer_than_the_field_is_cut_to_what_fits() {
        let sidecar = Sidecar {
            comment: "x".repeat(200),
            ..Default::default()
        };
        let back = parse(&render(&sidecar)).unwrap();
        assert_eq!(back.comment.chars().count(), MAX_COMMENT_LEN);
    }

    /// A newline in a comment would turn one sidecar into two lines and the
    /// second would be read as nothing.
    #[test]
    fn a_newline_in_a_comment_is_dropped_rather_than_breaking_the_file() {
        let sidecar = Sidecar {
            comment: "first\nsecond".into(),
            ..Default::default()
        };
        let text = render(&sidecar);
        assert_eq!(text.matches('\n').count(), 1);
        assert_eq!(parse(&text).unwrap().comment, "firstsecond");
    }

    #[test]
    fn the_amiga_epoch_renders_as_the_first_of_january_1978() {
        let text = render(&Sidecar::default());
        assert!(text.contains("1978-01-01"), "{text}");
        assert!(text.contains("00:00:00.00"), "{text}");
    }

    #[test]
    fn a_known_date_renders_the_way_a_person_would_write_it() {
        // 2026-08-09 is 17_752 days after 1978-01-01.
        let sidecar = Sidecar {
            date: AmigaDate {
                days: 17_752,
                mins: 9 * 60 + 5,
                ticks: 0,
            },
            ..Default::default()
        };
        let text = render(&sidecar);
        assert!(text.contains("2026-08-09"), "{text}");
        assert!(text.contains("09:05:00.00"), "{text}");
    }

    #[test]
    fn a_leap_day_survives_the_round_trip() {
        // 2024-02-29.
        let days = days_from_civil(2024, 2, 29) - AMIGA_EPOCH_DAYS;
        let sidecar = Sidecar {
            date: AmigaDate {
                days: days as u32,
                mins: 0,
                ticks: 0,
            },
            ..Default::default()
        };
        let text = render(&sidecar);
        assert!(text.contains("2024-02-29"), "{text}");
        assert_eq!(parse(&text).unwrap().date, sidecar.date);
    }

    #[test]
    fn a_date_before_the_amiga_epoch_clamps_rather_than_refusing_the_copy() {
        let sidecar = parse("----rwed 1970-01-01 00:00:00.00 old").unwrap();
        assert_eq!(sidecar.date.days, 0);
    }

    /// Silently defaulting would change what a file is. For a WHDLoad slave
    /// that is the difference between working and not.
    #[test]
    fn a_sidecar_that_cannot_be_read_is_an_error_not_a_default() {
        assert!(parse("").is_err());
        assert!(parse("nonsense").is_err());
        assert!(parse("----rwed not-a-date 00:00:00.00").is_err());
        assert!(parse("----rwed 2026-08-09 99:99:99.00").is_err());
        assert!(parse("----rwed 2026-13-09 00:00:00.00").is_err());
    }

    #[test]
    fn the_sidecar_sits_next_to_its_file_with_the_full_name_kept() {
        assert_eq!(
            sidecar_path(std::path::Path::new("D:/out/Game.slave")),
            std::path::PathBuf::from("D:/out/Game.slave.uaem"),
            "the extension is appended, never replaced — Game.uaem would collide \
             with the sidecar of a file called Game"
        );
    }

    #[test]
    fn the_calendar_conversion_is_its_own_inverse() {
        for days in [0i64, 1, 365, 366, 1000, 17_752, 30_000] {
            let (year, month, day) = civil_from_days(days + AMIGA_EPOCH_DAYS);
            assert_eq!(
                days_from_civil(year, month as i64, day as i64) - AMIGA_EPOCH_DAYS,
                days,
                "{year:04}-{month:02}-{day:02} did not come back as day {days}"
            );
        }
    }
}
