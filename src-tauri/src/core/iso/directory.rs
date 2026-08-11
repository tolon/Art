//! ISO9660 directory records.
//!
//! A directory is a run of variable-length records packed into whole sectors.
//! Every field here comes from a file ART did not write, so every one of them
//! is bounded before it is used.
//!
//! The one that bites hardest is the record length at offset 0. **Zero does
//! not mean "empty record"; it means "no more records in this sector".** A
//! parser that adds a zero length to its cursor spins forever, and with
//! `panic = "abort"` in the release profile a spin is the best case. The walk
//! below is written so that cannot happen: the outer loop steps sector by
//! sector and a zero length can only ever `break` out of the inner one.

use serde::Serialize;

use super::descriptor::{decode_iso646, decode_ucs2_be, LOGICAL_SECTOR_SIZE};
use crate::core::error::{CoreError, CoreResult};

/// Smallest legal directory record: 33 bytes of fixed fields plus at least
/// one byte of identifier.
const RECORD_MIN_LEN: usize = 34;

/// Offset of the identifier length byte within a record.
const ID_LEN_OFFSET: usize = 32;

/// `file flags` bit 1: this record describes a directory.
const FLAG_DIRECTORY: u8 = 0x02;

/// Most entries ART will parse out of a single directory.
///
/// The same order of magnitude as `MAX_COPY_ENTRIES`: large enough that no
/// real disc reaches it, small enough that a crafted extent cannot make ART
/// build an unbounded list.
pub const MAX_ENTRIES_PER_DIRECTORY: usize = 20_000;

/// One entry in a directory on an optical disc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IsoEntry {
    /// The name as the disc spells it, with any `;1` version suffix removed.
    pub name: String,
    pub is_dir: bool,
    /// Size in bytes. Zero for a directory is legal but unusual.
    pub bytes: u64,
    /// Starting logical block of this entry's data.
    pub extent: u32,
    /// Recording time in Unix seconds, or `None` when the disc left the
    /// field blank or filled it with something impossible.
    pub date: Option<i64>,
}

/// Parse every record in a directory's extent.
///
/// `buf` is the extent's bytes, already read as whole sectors — records never
/// cross a sector boundary in ISO9660, and one that claims to is malformed.
/// `joliet` selects the identifier encoding and must come from the descriptor
/// the root extent was taken from, never guessed per record.
///
/// `.` and `..` (identifiers `0x00` and `0x01`) are dropped: they are
/// structure, not contents.
pub fn parse_directory_extent(buf: &[u8], joliet: bool) -> CoreResult<Vec<IsoEntry>> {
    let mut out: Vec<IsoEntry> = Vec::new();

    // Outer loop: sectors. This is what makes the parse terminate — it
    // advances by a constant whatever the records inside say.
    let mut sector_start = 0usize;
    while sector_start < buf.len() {
        let sector_end = (sector_start + LOGICAL_SECTOR_SIZE).min(buf.len());
        let mut pos = sector_start;

        while pos < sector_end {
            let len = buf[pos] as usize;
            if len == 0 {
                // Padding to the end of the sector. Not an error, not a
                // record, and above all not a zero-width step.
                break;
            }
            if len < RECORD_MIN_LEN {
                return Err(malformed(format!(
                    "a directory record claims to be {len} bytes; the smallest legal record is {RECORD_MIN_LEN}"
                )));
            }
            if pos + len > sector_end {
                return Err(malformed(
                    "a directory record runs past the end of its sector".to_string(),
                ));
            }

            let record = &buf[pos..pos + len];
            if let Some(entry) = parse_record(record, joliet)? {
                if out.len() >= MAX_ENTRIES_PER_DIRECTORY {
                    return Err(malformed(format!(
                        "a directory on this disc lists more than {MAX_ENTRIES_PER_DIRECTORY} entries"
                    )));
                }
                out.push(entry);
            }
            pos += len;
        }

        sector_start += LOGICAL_SECTOR_SIZE;
    }

    Ok(out)
}

/// Parse one record. `None` means "this record is `.` or `..`" — skipped,
/// not an error.
fn parse_record(record: &[u8], joliet: bool) -> CoreResult<Option<IsoEntry>> {
    let id_len = record[ID_LEN_OFFSET] as usize;
    // 33 fixed bytes then the identifier; the record's own length must cover
    // both. Checked with `checked_add` because both halves are file data.
    let needed = 33usize
        .checked_add(id_len)
        .ok_or_else(|| malformed("a directory record's identifier length overflows".to_string()))?;
    if needed > record.len() {
        return Err(malformed(format!(
            "a directory record declares a {id_len}-byte name that does not fit in its {} bytes",
            record.len()
        )));
    }
    let id = &record[33..33 + id_len];

    // `.` and `..`, the only two identifiers that are a single control byte.
    if id_len == 1 && (id[0] == 0x00 || id[0] == 0x01) {
        return Ok(None);
    }

    let extent = u32::from_le_bytes([record[2], record[3], record[4], record[5]]);
    let bytes = u32::from_le_bytes([record[10], record[11], record[12], record[13]]) as u64;
    let date = decode_recording_date(&record[18..25]);
    let is_dir = record[25] & FLAG_DIRECTORY != 0;

    let name = if joliet {
        decode_ucs2_be(id)
    } else {
        decode_iso646(id)
    };
    let name = strip_version_suffix(&name, is_dir);

    if name.is_empty() {
        return Err(malformed(
            "a directory record has an empty name".to_string(),
        ));
    }

    Ok(Some(IsoEntry {
        name,
        is_dir,
        bytes,
        extent,
        date,
    }))
}

/// Remove the `;1` a file identifier carries, and the trailing `.` that an
/// extension-less 8.3 name is left with once it goes.
///
/// Directories never carry a version, so they are returned untouched — a
/// directory legitimately named `BACKUP;1` would otherwise lose two
/// characters.
fn strip_version_suffix(name: &str, is_dir: bool) -> String {
    if is_dir {
        return name.to_string();
    }
    let base = match name.rfind(';') {
        // Only strip when what follows is actually a version number.
        Some(i)
            if !name[i + 1..].is_empty() && name[i + 1..].bytes().all(|b| b.is_ascii_digit()) =>
        {
            &name[..i]
        }
        _ => name,
    };
    base.strip_suffix('.').unwrap_or(base).to_string()
}

/// Decode the 7-byte recording date of a directory record into Unix seconds.
///
/// Layout: years since 1900, month, day, hour, minute, second, then the
/// offset from GMT in 15-minute intervals as a signed byte.
///
/// Returns `None` for the all-zero field every mastering tool writes when it
/// has no date, and for anything out of range — a disc claiming month 19 gets
/// no date rather than a nonsense one.
pub fn decode_recording_date(field: &[u8]) -> Option<i64> {
    if field.len() < 7 || field[..7].iter().all(|&b| b == 0) {
        return None;
    }
    let year = field[0] as i64 + 1900;
    let month = field[1] as u32;
    let day = field[2] as u32;
    let hour = field[3] as i64;
    let minute = field[4] as i64;
    let second = field[5] as i64;
    let tz_quarters = field[6] as i8 as i64;

    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
        || !(-48..=52).contains(&tz_quarters)
    {
        return None;
    }

    let days = days_from_civil(year, month, day);
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second - tz_quarters * 15 * 60)
}

/// Days between 1970-01-01 and a proleptic Gregorian date.
///
/// Howard Hinnant's `days_from_civil`, which is exact for every year this can
/// produce (1900..=2155) and needs no table.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = ((m as i64) + 9) % 12; // March = 0
    let doy = (153 * mp + 2) / 5 + (d as i64) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn malformed(detail: String) -> CoreError {
    CoreError::Malformed {
        format: "iso9660".to_string(),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build one directory record for the tests below.
    fn record(id: &[u8], extent: u32, bytes: u32, is_dir: bool, date: [u8; 7]) -> Vec<u8> {
        let mut len = 33 + id.len();
        if len % 2 == 1 {
            len += 1; // records are padded to an even length
        }
        let mut r = vec![0u8; len];
        r[0] = len as u8;
        r[2..6].copy_from_slice(&extent.to_le_bytes());
        r[6..10].copy_from_slice(&extent.to_be_bytes());
        r[10..14].copy_from_slice(&bytes.to_le_bytes());
        r[14..18].copy_from_slice(&bytes.to_be_bytes());
        r[18..25].copy_from_slice(&date);
        r[25] = if is_dir { FLAG_DIRECTORY } else { 0 };
        r[ID_LEN_OFFSET] = id.len() as u8;
        r[33..33 + id.len()].copy_from_slice(id);
        r
    }

    fn sector_of(records: &[Vec<u8>]) -> Vec<u8> {
        let mut buf = vec![0u8; LOGICAL_SECTOR_SIZE];
        let mut pos = 0;
        for r in records {
            buf[pos..pos + r.len()].copy_from_slice(r);
            pos += r.len();
        }
        buf
    }

    const NO_DATE: [u8; 7] = [0; 7];

    #[test]
    fn dot_and_dotdot_are_not_entries() {
        let buf = sector_of(&[
            record(&[0x00], 20, 2048, true, NO_DATE),
            record(&[0x01], 20, 2048, true, NO_DATE),
            record(b"README.TXT;1", 30, 11, false, NO_DATE),
        ]);
        let entries = parse_directory_extent(&buf, false).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "README.TXT");
    }

    #[test]
    fn a_version_suffix_is_stripped_from_files_only() {
        let buf = sector_of(&[
            record(b"DISK.INFO;1", 30, 5, false, NO_DATE),
            record(b"BACKUP;1", 31, 2048, true, NO_DATE),
            record(b"NOVERSION", 32, 5, false, NO_DATE),
            record(b"README.;1", 33, 5, false, NO_DATE),
        ]);
        let entries = parse_directory_extent(&buf, false).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        // The directory keeps its semicolon; the files lose theirs, and the
        // trailing dot of an extension-less 8.3 name goes with it.
        assert_eq!(names, ["DISK.INFO", "BACKUP;1", "NOVERSION", "README"]);
    }

    #[test]
    fn a_record_length_of_zero_ends_the_sector_and_does_not_loop() {
        // Two sectors: the first holds one record then padding, the second
        // holds another. A parser that treated the zero as a step of zero
        // would never reach the second sector — or return at all.
        let mut buf = sector_of(&[record(b"FIRST.TXT;1", 30, 1, false, NO_DATE)]);
        buf.extend_from_slice(&sector_of(&[record(
            b"SECOND.TXT;1",
            31,
            2,
            false,
            NO_DATE,
        )]));
        let entries = parse_directory_extent(&buf, false).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].name, "SECOND.TXT");
    }

    #[test]
    fn a_record_shorter_than_the_fixed_fields_is_malformed() {
        let mut buf = vec![0u8; LOGICAL_SECTOR_SIZE];
        buf[0] = 12;
        let err = parse_directory_extent(&buf, false).unwrap_err();
        assert!(err.to_string().contains("smallest legal record"), "{err}");
    }

    #[test]
    fn a_record_running_past_its_sector_is_malformed() {
        // A record placed near the end of the sector, then given a length
        // that reaches past it. Records may not straddle a sector boundary.
        let mut buf = vec![0u8; LOGICAL_SECTOR_SIZE];
        let first = record(b"A.TXT;1", 29, 1, false, NO_DATE);
        let r = record(b"X.TXT;1", 30, 1, false, NO_DATE);
        let start = LOGICAL_SECTOR_SIZE - r.len();
        // Fill everything before it with well-formed records so the parser
        // actually walks up to the bad one.
        let mut pos = 0;
        while pos + first.len() <= start {
            buf[pos..pos + first.len()].copy_from_slice(&first);
            pos += first.len();
        }
        let start = pos;
        buf[start..start + r.len()].copy_from_slice(&r);
        buf[start] = 200; // now claims to extend beyond the sector
        let err = parse_directory_extent(&buf, false).unwrap_err();
        assert!(
            err.to_string().contains("past the end of its sector"),
            "{err}"
        );
    }

    #[test]
    fn an_identifier_longer_than_its_record_is_malformed() {
        let mut r = record(b"X.TXT;1", 30, 1, false, NO_DATE);
        r[ID_LEN_OFFSET] = 250;
        let buf = sector_of(&[r]);
        let err = parse_directory_extent(&buf, false).unwrap_err();
        assert!(err.to_string().contains("does not fit"), "{err}");
    }

    #[test]
    fn joliet_names_are_decoded_as_ucs2() {
        let name: Vec<u8> = "Grüße.txt"
            .encode_utf16()
            .flat_map(|u| u.to_be_bytes())
            .collect();
        let buf = sector_of(&[record(&name, 40, 7, false, NO_DATE)]);
        let entries = parse_directory_extent(&buf, true).unwrap();
        assert_eq!(entries[0].name, "Grüße.txt");
        // Read as ISO 646 the same bytes are unrecognisable, which is why the
        // flag must come from the descriptor rather than a guess. Here the
        // leading NUL of the first UCS-2 unit makes the name empty, which is
        // rejected outright — either way it is never the Joliet name.
        match parse_directory_extent(&buf, false) {
            Ok(entries) => assert_ne!(entries[0].name, "Grüße.txt"),
            Err(e) => assert!(e.to_string().contains("empty name"), "{e}"),
        }
    }

    #[test]
    fn a_directory_of_pure_padding_is_empty_not_an_error() {
        let buf = vec![0u8; LOGICAL_SECTOR_SIZE * 3];
        assert!(parse_directory_extent(&buf, false).unwrap().is_empty());
    }

    #[test]
    fn a_recording_date_becomes_unix_seconds() {
        // 1994-05-17 08:30:00, GMT+2 (8 quarter-hours).
        let field = [94, 5, 17, 8, 30, 0, 8];
        let unix = decode_recording_date(&field).unwrap();
        // 1994-05-17T06:30:00Z
        assert_eq!(unix, 769_156_200);
    }

    #[test]
    fn the_unix_epoch_itself_round_trips() {
        assert_eq!(decode_recording_date(&[70, 1, 1, 0, 0, 0, 0]).unwrap(), 0);
    }

    #[test]
    fn a_blank_or_impossible_date_is_none_not_a_wrong_answer() {
        assert_eq!(decode_recording_date(&NO_DATE), None);
        assert_eq!(decode_recording_date(&[94, 19, 17, 8, 30, 0, 0]), None); // month 19
        assert_eq!(decode_recording_date(&[94, 5, 0, 8, 30, 0, 0]), None); // day 0
        assert_eq!(decode_recording_date(&[94, 5, 17, 25, 0, 0, 0]), None); // hour 25
        assert_eq!(decode_recording_date(&[94, 5, 17, 1, 0, 0, 120]), None); // tz absurd
        assert_eq!(decode_recording_date(&[0, 0, 0]), None); // short field
    }

    #[test]
    fn the_entry_cap_is_an_error_not_an_unbounded_vec() {
        // A record of 34 bytes with a one-byte name; 60 fit per sector, so
        // enough sectors to pass the cap.
        let one = record(b"A", 30, 1, false, NO_DATE);
        let per_sector = LOGICAL_SECTOR_SIZE / one.len();
        let sectors = MAX_ENTRIES_PER_DIRECTORY / per_sector + 2;
        let filled = sector_of(&vec![one; per_sector]);
        let mut buf = Vec::with_capacity(sectors * LOGICAL_SECTOR_SIZE);
        for _ in 0..sectors {
            buf.extend_from_slice(&filled);
        }
        let err = parse_directory_extent(&buf, false).unwrap_err();
        assert!(err.to_string().contains("more than"), "{err}");
        assert_eq!(err.code(), "ART-FORMAT-MALFORMED");
    }
}
