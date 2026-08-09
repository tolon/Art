//! Aminet `INDEX` parsing (§41.5.2).
//!
//! The index is fixed-column text with a short `|`-prefixed header, then one
//! package per line — **five columns: name, directory, size, age,
//! description**:
//!
//! ```text
//! |
//! | Aminet index, created on 9-Aug-2026
//! |
//! A2KDeck.lha                    biz/dbase  671K 999 DataBase For AMWAY Distr
//! AB.lha                         biz/dbase   31K 999 Nice address book program
//! ```
//!
//! The full repository path is the directory and the name joined: there is no
//! column that already contains one.
//!
//! Column *widths* can drift between mirrors, so this parser never slices at
//! absolute offsets. It reads the line structurally — four whitespace-delimited
//! tokens and then the description — which needs no per-mirror table to
//! maintain.
//!
//! Verified against a live mirror on 2026-08-09: 3 026 of 3 026 data lines in
//! a 256 KB sample of `ftp.fau.de/aminet/INDEX` parse with zero skips.
//!
//! ## Malformed lines are counted, never guessed
//!
//! A line that does not yield a usable path is skipped and recorded in the
//! [`SyncReport`], which Power User Mode displays. A mirror that changes format
//! therefore shows up as a visible number rather than as a silently short
//! catalog — which is the failure mode that would otherwise make ART quietly
//! claim a package does not exist.
//!
//! ## The size column is a claim, not an allocation
//!
//! Nothing here is reserved from the size field. The download pipeline compares
//! the real byte count against it and aborts on a mismatch; a line claiming
//! 4 TB costs eight bytes of `u64`.

use super::text::{sanitise_text, truncate_at_char_boundary};
use super::{Claim, PackageMeta, PackageRef};

/// The longest index line ART will look at. Real lines are well under 200
/// bytes; anything past this is not an index line.
const MAX_LINE_BYTES: usize = 4096;

/// The longest composed repository path ART will accept.
const MAX_PATH_BYTES: usize = 512;

/// The longest file name. Real Aminet names run to about thirty characters.
const MAX_NAME_BYTES: usize = 128;

/// The longest directory. Aminet's are two components, around ten characters.
const MAX_DIRECTORY_BYTES: usize = 256;

/// Aminet's age column saturates: an entry older than this reports exactly
/// this value rather than its real age.
///
/// So `age_weeks == AGE_CAP_WEEKS` means "this old **or older**", and the UI
/// must not render it as a precise nineteen years.
pub const AGE_CAP_WEEKS: u32 = 999;

/// The longest description kept from the index.
const MAX_SHORT_BYTES: usize = 512;

/// The most bytes of index text parsed in one sync.
///
/// Aminet's full index measured 6.9 MB on 2026-08-09. This is a ceiling on
/// hostile input, not a target.
const MAX_INDEX_BYTES: usize = 64 * 1024 * 1024;

/// The most entries kept from one index.
const MAX_ENTRIES: usize = 500_000;

/// The most skipped-line examples kept for the report.
const MAX_SKIPPED_EXAMPLES: usize = 20;

/// The longest version string guessed from a file name.
const MAX_GUESSED_VERSION: usize = 16;

/// One line the parser refused, kept so a user can see *why* a sync came back
/// short.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SkippedLine {
    /// 1-based line number in the index.
    pub line_number: usize,
    /// Why it was skipped, in the user's language.
    pub reason: String,
    /// A bounded, sanitised excerpt of the offending line.
    pub excerpt: String,
}

/// What a catalog sync actually managed to read.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct SyncReport {
    pub parsed: usize,
    pub skipped: usize,
    /// The first few skipped lines, for diagnosis. Bounded.
    pub examples: Vec<SkippedLine>,
    /// True when the index was longer than ART was willing to read, so the
    /// catalog is knowingly incomplete.
    pub truncated: bool,
}

impl SyncReport {
    /// Whether the sync looks healthy enough to trust.
    ///
    /// A handful of odd lines is normal in thirty years of uploads; losing a
    /// large fraction means the format moved and the catalog should not be
    /// presented as complete.
    pub fn looks_complete(&self) -> bool {
        !self.truncated && self.skipped * 20 <= self.parsed
    }
}

/// Parse index bytes. Invalid UTF-8 is replaced rather than rejected, and the
/// input is bounded before anything else happens.
pub fn parse_index_bytes(bytes: &[u8], provider: &str) -> (Vec<PackageMeta>, SyncReport) {
    let over_limit = bytes.len() > MAX_INDEX_BYTES;
    let bounded = &bytes[..bytes.len().min(MAX_INDEX_BYTES)];

    let (entries, mut report) = parse_index(&String::from_utf8_lossy(bounded), provider);
    report.truncated |= over_limit;
    (entries, report)
}

/// Parse a repository index into package metadata plus a report of what was
/// dropped.
pub fn parse_index(text: &str, provider: &str) -> (Vec<PackageMeta>, SyncReport) {
    let head = truncate_at_char_boundary(text, MAX_INDEX_BYTES);

    let mut entries = Vec::new();
    let mut report = SyncReport {
        truncated: head.len() < text.len(),
        ..SyncReport::default()
    };

    for (idx, line) in head.lines().enumerate() {
        if entries.len() >= MAX_ENTRIES {
            report.truncated = true;
            break;
        }

        match parse_index_line(line, provider) {
            Ok(Some(entry)) => {
                entries.push(entry);
                report.parsed += 1;
            }
            Ok(None) => {}
            Err(reason) => {
                report.skipped += 1;
                if report.examples.len() < MAX_SKIPPED_EXAMPLES {
                    report.examples.push(SkippedLine {
                        line_number: idx + 1,
                        reason: reason.to_string(),
                        excerpt: sanitise_text(line, 120),
                    });
                }
            }
        }
    }

    (entries, report)
}

/// Parse one index line.
///
/// `Ok(None)` means the line carries no package and is not a defect — a blank
/// line, a comment, a rule of dashes. `Err` means the line looked like data and
/// could not be read, which the caller counts.
///
/// Exposed so a streaming sync can reuse it without holding the whole index in
/// memory.
pub fn parse_index_line(line: &str, provider: &str) -> Result<Option<PackageMeta>, &'static str> {
    if line.len() > MAX_LINE_BYTES {
        return Err("line is too long to be an index entry");
    }

    let line = line.trim_end_matches('\r');
    let trimmed = line.trim();

    if trimmed.is_empty() || is_decoration(trimmed) {
        return Ok(None);
    }

    let (name, rest) = take_token(trimmed);
    validate_name(name)?;

    let (directory, rest) = take_token(rest.trim_start());
    validate_directory(directory)?;

    let (size_token, rest) = take_token(rest.trim_start());
    let size_bytes = parse_size(size_token).ok_or("size column is not a size")?;

    let (age_weeks, description) = split_age(rest.trim_start());

    let path = format!("{directory}/{name}");
    if path.len() > MAX_PATH_BYTES {
        return Err("path is unreasonably long");
    }

    Ok(Some(PackageMeta {
        reference: PackageRef::new(provider, path),
        version: guess_version_from_name(name).map(Claim::from_filename),
        name: name.to_string(),
        directory: directory.to_string(),
        size_bytes,
        age_weeks,
        short: sanitise_text(description, MAX_SHORT_BYTES),
        requires: Vec::new(),
        author: None,
        distribution: None,
    }))
}

/// A header, separator or comment line rather than data.
///
/// Aminet's own index opens with three `|`-prefixed lines carrying the
/// generation date. They are not defects and must not inflate the skipped
/// count, or every healthy sync would report damage.
fn is_decoration(line: &str) -> bool {
    line.starts_with('|')
        || line.starts_with('#')
        || line.starts_with(';')
        || line.chars().all(|c| matches!(c, '-' | '=' | '_' | ' '))
}

/// Split off the leading whitespace-delimited token.
fn take_token(s: &str) -> (&str, &str) {
    match s.find(char::is_whitespace) {
        Some(end) => (&s[..end], &s[end..]),
        None => (s, ""),
    }
}

/// Reject anything that is not a plain file name.
///
/// An entry that fails is dropped from the catalog entirely rather than
/// sanitised: a name ART had to repair is a name ART does not understand, and
/// guessing at a hostile one is how traversal bugs are born. Local paths are
/// *still* built with [`safe_join`](crate::core::security::path::safe_join) at
/// the point of use — this is the first gate, not the only one.
fn validate_name(name: &str) -> Result<(), &'static str> {
    if name.is_empty() {
        return Err("entry has no name");
    }
    if name.len() > MAX_NAME_BYTES {
        return Err("name is unreasonably long");
    }
    if !name.bytes().all(|b| b.is_ascii_graphic()) {
        return Err("name contains spaces or non-printable characters");
    }
    if name.contains('/') || name.contains('\\') {
        return Err("name contains a path separator");
    }
    if name.contains(':') {
        return Err("name contains a drive or volume separator");
    }
    if name == "." || name == ".." {
        return Err("name is a relative component");
    }
    Ok(())
}

/// Reject anything that is not a plain repository directory.
///
/// Depth is not fixed at two even though every Aminet directory currently has
/// exactly that shape: the check is about what a component may *contain*, and
/// hard-coding today's depth would break on a repository that nests deeper for
/// no safety gain.
fn validate_directory(directory: &str) -> Result<(), &'static str> {
    if directory.is_empty() {
        return Err("entry has no directory");
    }
    if directory.len() > MAX_DIRECTORY_BYTES {
        return Err("directory is unreasonably long");
    }
    if !directory.bytes().all(|b| b.is_ascii_graphic()) {
        return Err("directory contains spaces or non-printable characters");
    }
    if directory.contains('\\') {
        return Err("directory contains a backslash");
    }
    if directory.contains(':') {
        return Err("directory contains a drive or volume separator");
    }
    if directory.starts_with('/') {
        return Err("directory is absolute");
    }
    if directory.ends_with('/') {
        return Err("directory has a trailing separator");
    }

    for segment in directory.split('/') {
        match segment {
            "" => return Err("directory has an empty component"),
            "." | ".." => return Err("directory contains a relative component"),
            _ => {}
        }
    }

    Ok(())
}

/// Parse `318K`, `1.2M`, `2G` or a plain byte count.
///
/// All arithmetic is checked: the result is a `u64` or nothing, never a wrap.
fn parse_size(token: &str) -> Option<u64> {
    if token.is_empty() || token.len() > 16 || !token.is_ascii() {
        return None;
    }

    let bytes = token.as_bytes();
    let (digits, multiplier) = match bytes[bytes.len() - 1].to_ascii_uppercase() {
        b'K' => (&token[..token.len() - 1], 1024u64),
        b'M' => (&token[..token.len() - 1], 1024 * 1024),
        b'G' => (&token[..token.len() - 1], 1024 * 1024 * 1024),
        b'B' => (&token[..token.len() - 1], 1),
        _ => (token, 1),
    };

    let (whole, fraction) = match digits.split_once('.') {
        Some((w, f)) => (w, Some(f)),
        None => (digits, None),
    };

    if whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut total = whole.parse::<u64>().ok()?.checked_mul(multiplier)?;

    if let Some(fraction) = fraction {
        // More than three decimals in a size column is noise, not precision.
        if fraction.is_empty()
            || fraction.len() > 3
            || !fraction.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let scale = 10u64.pow(fraction.len() as u32);
        let part = fraction
            .parse::<u64>()
            .ok()?
            .checked_mul(multiplier)?
            .checked_div(scale)?;
        total = total.checked_add(part)?;
    }

    Some(total)
}

/// Split the age column from the description.
///
/// The age is its own whitespace-delimited column of digits, and the
/// description is everything after it. A fourth column that is *not* all
/// digits means the layout is not what ART expects: rather than discard the
/// line, the age is left unknown and the column is treated as the start of the
/// description. Losing a sort key beats losing the package.
fn split_age(rest: &str) -> (Option<u32>, &str) {
    let (token, after) = take_token(rest);

    if token.is_empty() || !token.bytes().all(|b| b.is_ascii_digit()) {
        return (None, rest);
    }

    // A digit run too long to be a week count is still structurally the age
    // column; consume it, but claim nothing about its value.
    let age = token.parse::<u32>().ok();
    (age, after.trim_start())
}

/// Guess a version from a file name, e.g. `AmiSSL-5.5.lha` → `5.5`.
///
/// Deliberately conservative: the digits must follow a separator, so
/// `MUI38usr.lha` yields nothing rather than `38`. The result is only ever a
/// `Low`-confidence claim, and a readme version replaces it (§41.5.2).
fn guess_version_from_name(name: &str) -> Option<String> {
    let stem = match name.rsplit_once('.') {
        Some((stem, _ext)) => stem,
        None => name,
    };

    let bytes = stem.as_bytes();
    let mut best: Option<String> = None;
    let mut i = 0;

    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }

        // A run of digits and dots is one candidate. Walking it whole matters:
        // scanning character by character would let the trailing "5" of "5.5"
        // overwrite the full match.
        let mut end = i;
        while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
            end += 1;
        }

        // Must start right after a separator, so `MUI38usr` yields nothing.
        if i > 0 && matches!(bytes[i - 1], b'-' | b'_' | b'.' | b'v' | b'V') {
            let candidate = stem[i..end].trim_end_matches('.');
            if !candidate.is_empty() && candidate.len() <= MAX_GUESSED_VERSION {
                best = Some(candidate.to_string());
            }
        }

        i = end.max(i + 1);
    }

    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::sources::{ClaimSource, PROVIDER_AMINET};
    use crate::core::workflow::types::Confidence;

    /// The real Aminet layout, verified against a live mirror on 2026-08-09:
    /// a `|`-prefixed header, then name · directory · size · age · description.
    ///
    /// Synthetic content in the verified shape — ART ships no Aminet data.
    const SAMPLE: &str = "\
|
| Aminet index, created on 9-Aug-2026
|
AmigaBase.lha                  biz/dbase  318K   4 A programmable, hierarchical database
AmiTCP-SDK-4.3.lha             comm/tcp   1.2M  12 SDK for AmiTCP
AmiSSL-5.5.lha                 util/libs 1234K   0 Portable SSL/TLS library
";

    fn parse(text: &str) -> (Vec<PackageMeta>, SyncReport) {
        parse_index(text, PROVIDER_AMINET)
    }

    #[test]
    fn reads_the_real_aminet_layout() {
        let (entries, report) = parse(SAMPLE);

        assert_eq!(report.parsed, 3);
        assert_eq!(report.skipped, 0, "the |-header must not count as damage");
        assert!(report.looks_complete());

        let first = &entries[0];
        // The path is composed: no column already holds one.
        assert_eq!(first.reference.path, "biz/dbase/AmigaBase.lha");
        assert_eq!(first.reference.provider, PROVIDER_AMINET);
        assert_eq!(first.directory, "biz/dbase");
        assert_eq!(first.name, "AmigaBase.lha");
        assert_eq!(first.size_bytes, 318 * 1024);
        assert_eq!(first.age_weeks, Some(4));
        assert_eq!(first.short, "A programmable, hierarchical database");

        // 1.2M is 1 MiB plus two tenths of one, computed in integer maths.
        assert_eq!(entries[1].size_bytes, 1024 * 1024 + (2 * 1024 * 1024) / 10);
        assert_eq!(entries[2].age_weeks, Some(0));
    }

    /// Column widths can drift between mirrors; the parser must not care.
    #[test]
    fn column_drift_does_not_break_parsing() {
        let wide =
            "AmigaBase.lha                          biz/dbase        318K      4 A database\n";
        let narrow = "AmigaBase.lha biz/dbase 318K 4 A database\n";

        for text in [wide, narrow] {
            let (entries, report) = parse(text);
            assert_eq!(report.skipped, 0, "failed on {text:?}");
            assert_eq!(entries[0].reference.path, "biz/dbase/AmigaBase.lha");
            assert_eq!(entries[0].size_bytes, 318 * 1024);
            assert_eq!(entries[0].age_weeks, Some(4));
            assert_eq!(entries[0].short, "A database");
        }
    }

    /// Aminet's age column saturates at 999, so the oldest entries are all
    /// "this old or older" rather than precisely nineteen years.
    #[test]
    fn a_capped_age_is_recognisable_as_a_cap() {
        let (entries, _) = parse("Old.lha a/b 1K 999 Ancient\nNew.lha a/b 1K 3 Fresh\n");
        assert!(entries[0].age_is_capped());
        assert!(!entries[1].age_is_capped());
    }

    #[test]
    fn crlf_and_a_missing_final_newline_are_fine() {
        let (entries, report) = parse("One.lha a/b 1K 0 One\r\nTwo.lha a/b 2K 1 Two");
        assert_eq!(report.parsed, 2);
        assert_eq!(entries[1].short, "Two");
    }

    #[test]
    fn blank_lines_and_decoration_are_not_defects() {
        let (_, report) = parse("\n| header\n# a comment\n----------\n\nOne.lha a/b 1K 0 One\n");
        assert_eq!(report.parsed, 1);
        assert_eq!(report.skipped, 0);
    }

    /// The load-bearing honesty rule: a line ART cannot read is counted, so a
    /// mirror that changes format is visible instead of silently halving the
    /// catalog.
    #[test]
    fn malformed_lines_are_counted_with_examples() {
        let (entries, report) = parse("Good.lha a/b 1K 0 Fine\nthis is not an index line at all\n");

        assert_eq!(entries.len(), 1);
        assert_eq!(report.skipped, 1);
        assert_eq!(report.examples.len(), 1);
        assert_eq!(report.examples[0].line_number, 2);
        assert!(!report.examples[0].reason.is_empty());
    }

    #[test]
    fn a_mostly_broken_index_does_not_look_complete() {
        let mut text = String::from("Good.lha a/b 1K 0 Fine\n");
        text.push_str(&"garbage line\n".repeat(50));

        let (_, report) = parse(&text);
        assert!(!report.looks_complete());
    }

    // ---- path hardening ----

    #[test]
    fn a_hostile_name_or_directory_is_dropped_not_repaired() {
        let hostile = [
            ("../../../etc/passwd a/b 1K 0 traversal", "traversal name"),
            ("evil.lha ../../etc 1K 0 traversal", "traversal directory"),
            ("/absolute.lha a/b 1K 0 x", "absolute name"),
            ("evil.lha /absolute 1K 0 x", "absolute directory"),
            ("C:evil.lha a/b 1K 0 x", "drive in the name"),
            ("evil.lha C:/windows 1K 0 x", "drive in the directory"),
            ("a\\b.lha a/b 1K 0 x", "backslash in the name"),
            ("evil.lha a\\b 1K 0 x", "backslash in the directory"),
            ("evil.lha util//double 1K 0 x", "empty component"),
            ("evil.lha a/./b 1K 0 x", "relative component"),
            ("evil.lha a/b/ 1K 0 x", "trailing separator"),
            (".. a/b 1K 0 x", "the name is a relative component"),
        ];

        for (line, why) in hostile {
            let (entries, report) = parse(&format!("{line}\n"));
            assert!(entries.is_empty(), "accepted {why}: {line}");
            assert_eq!(report.skipped, 1, "not counted ({why}): {line}");
        }
    }

    #[test]
    fn an_absurdly_long_name_or_directory_is_refused() {
        for line in [
            format!("{}.lha a/b 1K 0 x\n", "n".repeat(MAX_NAME_BYTES)),
            format!("x.lha a/{} 1K 0 x\n", "d".repeat(MAX_DIRECTORY_BYTES)),
        ] {
            let (entries, report) = parse(&line);
            assert!(entries.is_empty());
            assert_eq!(report.skipped, 1);
        }
    }

    #[test]
    fn an_absurdly_long_line_is_refused_without_being_walked() {
        let line = format!("x.lha a/b 1K 0 {}\n", "d".repeat(MAX_LINE_BYTES));
        let (entries, report) = parse(&line);
        assert!(entries.is_empty());
        assert_eq!(report.skipped, 1);
    }

    /// §45.5.7 groundwork: a description cannot carry its own formatting, and
    /// cannot smuggle a newline that would forge a second record.
    #[test]
    fn a_hostile_description_is_sanitised_and_bounded() {
        let (entries, _) = parse("Evil.lha a/b 1K 0 IGNORE \u{1b}[2J PREVIOUS\u{0}INSTRUCTIONS\n");
        let short = &entries[0].short;
        assert!(!short.contains('\u{1b}'));
        assert!(!short.contains('\0'));
        assert!(!short.contains('\n'));

        let long = format!("L.lha a/b 1K 0 {}\n", "x".repeat(2000));
        let (entries, _) = parse(&long);
        assert!(entries[0].short.len() <= MAX_SHORT_BYTES);
    }

    // ---- size column ----

    #[test]
    fn sizes_parse_in_every_shipped_form() {
        assert_eq!(parse_size("318K"), Some(318 * 1024));
        assert_eq!(
            parse_size("1.2M"),
            Some(1024 * 1024 + (2 * 1024 * 1024) / 10)
        );
        assert_eq!(parse_size("2G"), Some(2 * 1024 * 1024 * 1024));
        assert_eq!(parse_size("4096"), Some(4096));
        assert_eq!(parse_size("512b"), Some(512));
        assert_eq!(parse_size("1m"), Some(1024 * 1024));
    }

    #[test]
    fn a_nonsense_size_is_a_skipped_line_not_a_zero() {
        for token in ["", "K", "-1", "1.2.3M", "1.2345M", "abc", "1e9"] {
            assert_eq!(parse_size(token), None, "accepted {token:?}");
        }

        let (entries, report) = parse("x.lha a/b notasize 0 x\n");
        assert!(entries.is_empty());
        assert_eq!(report.skipped, 1);
    }

    /// A size field is a claim. It must never overflow and never be used to
    /// reserve anything.
    #[test]
    fn an_overflowing_size_is_refused_rather_than_wrapped() {
        assert_eq!(parse_size("99999999999999999999G"), None);
        assert_eq!(parse_size(&format!("{}G", u64::MAX)), None);
    }

    // ---- age column ----

    #[test]
    fn the_age_column_is_its_own_field() {
        assert_eq!(
            split_age("12 Some description"),
            (Some(12), "Some description")
        );
        assert_eq!(split_age("999 Ancient thing"), (Some(999), "Ancient thing"));
        assert_eq!(split_age("No digits here"), (None, "No digits here"));
    }

    /// A description that opens with a number must not be eaten as an age.
    /// "1000 Miles" is a game.
    #[test]
    fn a_description_starting_with_a_number_keeps_its_number() {
        let (entries, report) = parse("Miles.lha game/misc 1K 4 1000 Miles racing game\n");
        assert_eq!(report.skipped, 0);
        assert_eq!(entries[0].age_weeks, Some(4));
        assert_eq!(entries[0].short, "1000 Miles racing game");
    }

    #[test]
    fn an_unreadable_age_still_leaves_a_clean_description() {
        let (age, description) = split_age("99999999999999 Description");
        assert_eq!(age, None);
        assert_eq!(description, "Description");
    }

    /// Losing a sort key beats losing the package: a fourth column that is not
    /// a number becomes part of the description rather than a skipped line.
    #[test]
    fn a_missing_age_column_keeps_the_whole_description() {
        let (entries, report) = parse("x.lha a/b 1K A description with no age\n");
        assert_eq!(report.skipped, 0);
        assert_eq!(entries[0].age_weeks, None);
        assert_eq!(entries[0].short, "A description with no age");
    }

    // ---- version guessing ----

    #[test]
    fn a_version_in_the_file_name_is_a_low_confidence_claim() {
        let (entries, _) = parse("AmiSSL-5.5.lha util/libs 1K 0 x\n");
        let claim = entries[0].version.as_ref().expect("a guess");
        assert_eq!(claim.value, "5.5");
        assert_eq!(claim.confidence, Confidence::Low);
        assert_eq!(claim.source, ClaimSource::Filename);
    }

    #[test]
    fn version_guessing_stays_conservative() {
        assert_eq!(
            guess_version_from_name("AmiSSL-5.5.lha").as_deref(),
            Some("5.5")
        );
        assert_eq!(
            guess_version_from_name("Foo_1.2.3.lha").as_deref(),
            Some("1.2.3")
        );
        assert_eq!(
            guess_version_from_name("Thing-v2.0.lha").as_deref(),
            Some("2.0")
        );

        // No separator before the digits: not a version, just a name. Both of
        // these shapes are common in the real index.
        assert_eq!(guess_version_from_name("MUI38usr.lha"), None);
        assert_eq!(guess_version_from_name("AmigaBase.lha"), None);
        assert_eq!(guess_version_from_name("APrint3.2.lha"), None);
    }

    #[test]
    fn an_index_with_no_version_information_claims_none() {
        let (entries, _) = parse("AmigaBase.lha biz/dbase 318K 4 x\n");
        assert!(entries[0].version.is_none());
    }

    // ---- input bounds ----

    #[test]
    fn an_oversized_index_is_truncated_and_says_so() {
        let mut huge = Vec::new();
        while huge.len() <= MAX_INDEX_BYTES {
            huge.extend_from_slice(b"x.lha a/b 1K 0 x\n");
        }

        let (_, report) = parse_index_bytes(&huge, PROVIDER_AMINET);
        assert!(report.truncated);
        assert!(!report.looks_complete());
    }

    /// Aminet descriptions are effectively Latin-1 and do carry the odd
    /// high byte. A mangled character must cost that character, not the entry.
    #[test]
    fn a_non_utf8_byte_in_a_description_does_not_lose_the_entry() {
        let bytes = b"Audithec.lha biz/dbase 227K 984 AudioCD Database 1.1\xb1 + v1.0.2\n";
        let (entries, report) = parse_index_bytes(bytes, PROVIDER_AMINET);

        assert_eq!(report.skipped, 0);
        assert_eq!(entries[0].reference.path, "biz/dbase/Audithec.lha");
        assert!(entries[0].short.starts_with("AudioCD Database"));
    }

    /// A non-ASCII *name* is a different matter: it becomes a path, so it is
    /// dropped rather than repaired.
    #[test]
    fn a_non_ascii_name_is_dropped_while_the_rest_parses() {
        let bytes = b"Caf\xe9.lha a/b 1K 0 Latin-1 name\nOk.lha util/x 1K 0 Fine\n";
        let (entries, report) = parse_index_bytes(bytes, PROVIDER_AMINET);

        assert_eq!(report.skipped, 1);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].reference.path, "util/x/Ok.lha");
    }
}
