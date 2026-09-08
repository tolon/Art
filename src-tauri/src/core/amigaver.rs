//! AmigaOS `$VER:` strings — what a file says about its own version.
//!
//! AmigaOS embeds a version marker in a file's own bytes so that `Version`
//! can report it without knowing the format. ART reads the same marker for
//! one reason: when a package is about to overwrite a file, "45.1 → 45.127"
//! is an answer a person can act on and "different bytes" is not.
//!
//! **It is not a fact about every file.** 181 of the 588 files in a real
//! AmigaOS 3.9 tree carry one — 31%. The other 69% have nothing to say, and
//! saying nothing about them is the correct behaviour, not a gap
//! (spec §89).

use std::cmp::Ordering;

/// The marker, exactly as AmigaOS writes it.
const MARKER: &[u8] = b"$VER:";

/// The most of a marker's text ART will look at.
///
/// A name and two integers is tens of bytes; anything past this is not a
/// version string, and reading further would let a hostile file decide how
/// much work ART does — see `a_marker_followed_by_megabytes_of_digits_is_bounded`.
const MAX_MARKER_TEXT: usize = 128;

/// What a file says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmigaVersion {
    pub name: String,
    pub version: u32,
    pub revision: u32,
}

impl AmigaVersion {
    /// Compare by version, then revision — a named method rather than
    /// `PartialOrd`/`Ord`, and deliberately not either.
    ///
    /// `PartialOrd` promises (Rust's own documented contract) that
    /// `a == b` iff `partial_cmp(a, b) == Some(Equal)`. This type's derived
    /// `PartialEq` compares every field, `name` included, while the
    /// comparison that matters for an upgrade/downgrade decision compares
    /// only `version` and `revision` — two records for different programs
    /// that happen to share a version number must stay `!=` while still
    /// comparing `Equal` here. An operator (`>`, `<`) that silently means
    /// less than it looks like is exactly the trap: something that sorts,
    /// dedups, or takes a `max` of `AmigaVersion` values via `PartialOrd`
    /// would see two different programs collapse into one. A named method
    /// makes every call site say what it is comparing instead. It also
    /// isn't really partial — the comparison never has an incomparable
    /// case — so `Ordering` rather than `Option<Ordering>` says that
    /// honestly.
    ///
    /// **The date is deliberately not compared**: a rebuilt binary can
    /// carry a later date and the same version, and calling that an update
    /// would put a downgrade behind a green arrow.
    ///
    /// Names are not compared either. Whether two files are the same thing
    /// is decided by where they land in the tree, not by what they call
    /// themselves — `LoadWB` and `loadwb` are one file to AmigaDOS.
    pub fn compare_version(&self, other: &Self) -> Ordering {
        self.version
            .cmp(&other.version)
            .then(self.revision.cmp(&other.revision))
    }
}

/// Decode a bounded byte window as ISO-8859-1 (Latin-1).
///
/// AmigaDOS's native character set is Latin-1 — the same byte-transparent
/// choice `core::adf::bcpl::write_bcpl_string` and
/// `core::iso::descriptor::decode_iso646` already make for AmigaDOS text
/// elsewhere in this codebase, for the same reason: Unicode's first 256 code
/// points are Latin-1 by construction, so `b as char` is exact for every
/// byte value and the decode can never fail. That matters here specifically
/// because it removes a failure mode a hostile or merely unusual `$VER:`
/// string could otherwise trigger for no benefit — a version string's name
/// is free-form AmigaDOS text (spec places no ASCII restriction on it), and
/// refusing a file over one accented byte in that name would report "no
/// version" for a file that plainly states one. Decoding Latin-1
/// unconditionally, rather than attempting UTF-8 first, also collapses what
/// the brief's sketch wrote as two identical UTF-8 attempts into one
/// decision: bytes 0x00..=0x7F are identical in both encodings (the common
/// case, since real `$VER:` names and numbers are ASCII), so nothing that
/// used to parse under a UTF-8-first reading stops parsing under this one.
fn decode_latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

/// Read the first `$VER:` marker in `bytes`, if there is one ART can parse.
///
/// Returns `None` for a file with no marker **and** for a marker ART cannot
/// turn into two integers. Those two are deliberately the same answer: a
/// half-understood version on screen is worse than none, because the reader
/// cannot tell which half was guessed.
pub fn read(bytes: &[u8]) -> Option<AmigaVersion> {
    let at = bytes.windows(MARKER.len()).position(|w| w == MARKER)?;
    let start = at + MARKER.len();
    let end = bytes.len().min(start.saturating_add(MAX_MARKER_TEXT));
    let text = decode_latin1(&bytes[start..end]);
    parse(&text)
}

/// Is every character in a candidate name one AmigaDOS would plausibly put
/// in a program identifier?
///
/// `$VER:` is found by raw substring search over bytes ART has no other
/// reason to trust — unlike `core::iso::descriptor`'s ISO9660 identifiers,
/// which sit in a field the volume descriptor structurally guarantees is
/// text, a `$VER:` match can land anywhere inside an arbitrary binary, so
/// nothing but the bytes that happen to follow it says this is a version
/// string rather than four coincidental ASCII bytes in compiled code or
/// packed data. Requiring valid UTF-8 used to be an incidental filter
/// against exactly that: binary noise decoded to `Err` far more often than
/// it decoded to a plausible name. Decoding as Latin-1 unconditionally
/// (see `decode_latin1`) removes that filter — Latin-1 never fails — so
/// this restores an equivalent one that does not depend on the encoding:
/// reject a name containing a control character (C0 `0x00..=0x1F`, `0x7F`,
/// or C1 `0x80..=0x9F`, all of which `char::is_control` recognises for
/// Latin-1's code-point-for-byte-value range). A genuine AmigaDOS program
/// name is printable text; binary noise that happens to contain a `NN.NN`
/// shape after four accidental `$VER:` bytes is not.
fn is_plausible_name(name: &str) -> bool {
    !name.chars().any(|c| c.is_control())
}

/// Read the version a **library** states about itself in its resident tag's
/// id string, anchored on the library's own file name.
///
/// ## Why this exists beside [`read`], and how it was measured
///
/// `read` looks for `$VER:`, and 31% of a real AmigaOS 3.9 tree carries one.
/// `Libs/xadmaster.library` is in the other 69%: searched byte by byte on
/// 2026-09-08, the owner's own copies carry **no `$VER:` marker at all** —
/// not the 105 332-byte 9.0 off the AmigaOS 3.9 CD, not the 105 368-byte 9.1
/// BoingBag 3.9-1 leaves, not the 110 100-byte 10.0 the `XAD-Update` payload
/// carries. What each one does carry, at offset ~384, is the `struct
/// Resident` its own `rt_Name`/`rt_IdString` pair:
///
/// ```text
/// …\x02\xfc  xadmaster.library\0  xadmaster 9.0 (25.11.2000) AmigaOS…
/// …\x02\xfe  xadmaster.library\0  xadmaster 10.0 (31.03.2001) AmigaOS…
/// ```
///
/// That id string is what AmigaDOS's own `Version <file> <n> FILE` reports
/// for a library, and it is the string BoingBag 3.9-2's `XAD-Update` gate is
/// written against (`Version "SYS:Libs/xadmaster.library" 10 FILE` — HstWB
/// Installer's `Install-Boing-Bag-2`, lines 32-36, MIT). So a host-side
/// reader that only knew `$VER:` would answer *nothing* for the one file the
/// gate is about, and a gate that cannot read its own file has to either
/// refuse everything or open for everything — both of them wrong, and both
/// quietly.
///
/// ## Anchored on the file's own name, never on the first thing that parses
///
/// `name` is the file's own stem (`xadmaster` for `xadmaster.library`), and
/// only an occurrence of *that word* followed by `version.revision` counts.
/// This is [`crate::core::osinstall::collide`]'s own rule, applied to a
/// second kind of marker: a version label taken from a string that names
/// some *other* program is how a file gets labelled with another program's
/// numbers. A bare "first `N.N` in the file" search would find the compiler's
/// own build number as readily as the library's.
///
/// Matching is case-insensitive (AmigaDOS names are, ART-012) and the match
/// must begin at a word boundary, so `unxadmaster 3.0` never answers for
/// `xadmaster`.
///
/// Returns `None` when nothing in `bytes` states a version for `name` — the
/// same answer, deliberately, as a name whose numbers ART cannot parse. Half
/// a version is worse than none.
pub fn read_id_string(bytes: &[u8], name: &str) -> Option<AmigaVersion> {
    if name.is_empty() {
        return None;
    }
    let needle: Vec<u8> = name.as_bytes().to_ascii_lowercase();
    let mut at = 0usize;
    while at + needle.len() <= bytes.len() {
        let found = bytes[at..]
            .windows(needle.len())
            .position(|window| window.to_ascii_lowercase() == needle)?;
        let start = at + found;
        // A word boundary before it: `unxadmaster 3.0` is not a statement
        // about `xadmaster`.
        let boundary = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        if boundary {
            let end = bytes.len().min(
                start
                    .saturating_add(needle.len())
                    .saturating_add(MAX_MARKER_TEXT),
            );
            // `parse` reads ` name version.revision`, which is exactly the
            // shape of the id string from `start` — the name is the first
            // word, so the same parser serves both markers rather than a
            // second one that could drift from it.
            if let Some(version) = parse(&decode_latin1(&bytes[start..end])) {
                return Some(version);
            }
        }
        at = start + 1;
    }
    None
}

/// The text after the marker: ` name version.revision (date)`.
fn parse(text: &str) -> Option<AmigaVersion> {
    let mut words = text.split_whitespace();
    let name = words.next()?;
    if !is_plausible_name(name) {
        return None;
    }
    let number = words.next()?;
    let (version, revision) = number.split_once('.')?;
    Some(AmigaVersion {
        name: name.to_string(),
        version: version.parse().ok()?,
        revision: revision.parse().ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real strings, read out of the owner's own AmigaOS 3.9 tree rather
    /// than invented — a parser tested only against what its author
    /// imagined is a parser tested against its author.
    #[test]
    fn reads_the_version_strings_real_amiga_files_carry() {
        for (raw, name, version, revision) in [
            ("$VER: assign 37.4 (25.4.91)", "assign", 37, 4),
            ("$VER: adddatatypes 44.4 (4.8.99)", "adddatatypes", 44, 4),
            ("$VER: ConClip 44.3 (22.9.99)", "ConClip", 44, 3),
            ("$VER: binddrivers 38.2 (31.3.92)", "binddrivers", 38, 2),
        ] {
            let got = read(raw.as_bytes()).unwrap_or_else(|| panic!("no version in {raw}"));
            assert_eq!(got.name, name, "{raw}");
            assert_eq!((got.version, got.revision), (version, revision), "{raw}");
        }
    }

    /// The marker is found wherever it sits, because it sits wherever the
    /// compiler put it — never at a fixed offset.
    #[test]
    fn finds_the_marker_anywhere_in_the_file() {
        let mut bytes = vec![0u8; 4096];
        bytes.extend_from_slice(b"$VER: assign 37.4 (25.4.91)");
        bytes.extend_from_slice(&[0u8; 512]);
        assert_eq!(read(&bytes).unwrap().version, 37);
    }

    /// 69% of a real tree. Absence is the common case and not an error.
    #[test]
    fn a_file_with_no_version_string_answers_none() {
        assert!(read(b"just some bytes, no marker here").is_none());
        assert!(read(&[0u8; 8192]).is_none());
    }

    /// A marker ART cannot parse into two integers is treated as absent.
    /// Guessing at a malformed one is exactly the invention the fourth
    /// collision class exists to prevent (spec §3).
    #[test]
    fn a_malformed_version_is_treated_as_absent() {
        for raw in [
            "$VER: nameonly",
            "$VER: thing x.y (1.1.99)",
            "$VER: thing 44 (1.1.99)",
            "$VER:",
            "$VER: thing 44. (1.1.99)",
        ] {
            assert!(read(raw.as_bytes()).is_none(), "{raw} should not parse");
        }
    }

    /// Version first, then revision — and the date is not part of it, so a
    /// rebuild carrying a later date and the same version is not an update.
    #[test]
    fn compares_version_then_revision_and_ignores_the_date() {
        let older = read(b"$VER: assign 37.4 (25.4.91)").unwrap();
        let newer_revision = read(b"$VER: assign 37.9 (1.1.99)").unwrap();
        let newer_version = read(b"$VER: assign 45.1 (1.1.99)").unwrap();
        let rebuilt = read(b"$VER: assign 37.4 (31.12.99)").unwrap();

        assert_eq!(newer_revision.compare_version(&older), Ordering::Greater);
        assert_eq!(
            newer_version.compare_version(&newer_revision),
            Ordering::Greater
        );
        assert_ne!(
            rebuilt.compare_version(&older),
            Ordering::Greater,
            "a later date alone is not a newer version"
        );
        assert_ne!(older.compare_version(&rebuilt), Ordering::Greater);
    }

    /// The `PartialEq`/`compare_version` split findings 1 and 2 asked for:
    /// two records for different programs that happen to share a version
    /// number compare `Equal` under `compare_version` but stay `!=` under
    /// the derived `PartialEq`, because `PartialEq` still compares `name`.
    #[test]
    fn compare_version_ignores_name_but_partial_eq_does_not() {
        let assign = read(b"$VER: assign 37.4 (25.4.91)").unwrap();
        let other = read(b"$VER: other 37.4 (25.4.91)").unwrap();

        assert_eq!(assign.compare_version(&other), Ordering::Equal);
        assert_ne!(assign, other, "different programs are not the same file");
    }

    /// A version marker can run to the end of the file with no date and no
    /// terminator. Real files do this.
    #[test]
    fn a_version_with_no_date_still_reads() {
        assert_eq!(read(b"$VER: thing 45.12").unwrap().revision, 12);
    }

    /// A hostile file must not make this expensive or make it allocate:
    /// the marker's own text is bounded, whatever follows it.
    #[test]
    fn a_marker_followed_by_megabytes_of_digits_is_bounded() {
        let mut bytes = b"$VER: thing 44.".to_vec();
        bytes.extend(std::iter::repeat_n(b'9', 4 * 1024 * 1024));
        // Either it parses a bounded prefix or it declines. It must not hang
        // and must not try to hold a four-megabyte integer.
        let _ = read(&bytes);
    }

    /// The decision this task had to make explicit: a non-UTF-8 byte after
    /// the marker is decoded as Latin-1, not refused. `0xE9` is not valid
    /// UTF-8 on its own, but it is `é` in Latin-1 — the same byte AmigaDOS
    /// itself would have written for that letter.
    #[test]
    fn a_non_utf8_name_decodes_as_latin1_rather_than_being_refused() {
        let mut bytes = b"$VER: caf".to_vec();
        bytes.push(0xE9); // Latin-1 'e'-acute; not valid standalone UTF-8.
        bytes.extend_from_slice(b" 37.4 (1.1.99)");
        let got = read(&bytes).unwrap();
        assert_eq!(got.name, "caf\u{e9}");
        assert_eq!((got.version, got.revision), (37, 4));
    }

    /// A coincidental `$VER:` match inside binary noise — four bytes that
    /// happen to line up, not a real version marker — must not be reported
    /// just because what follows happens to have a `NN.NN` shape.
    /// `is_plausible_name`'s doc comment explains why control bytes are the
    /// filter: real AmigaDOS program names are printable text.
    #[test]
    fn binary_noise_with_a_coincidental_number_shape_is_rejected() {
        let mut bytes = b"$VER: ".to_vec();
        bytes.extend_from_slice(&[0x01, 0x02, 0x03]);
        bytes.extend_from_slice(b" 12.34 (1.1.99)");
        assert!(read(&bytes).is_none());
    }

    /// The 31% figure the spec rests on, re-measurable rather than quoted.
    #[test]
    #[ignore = "reads a real distribution tree; run explicitly"]
    fn count_version_strings_in_a_real_tree_when_asked() {
        let Ok(root) = std::env::var("ART_VER_TREE") else {
            return;
        };

        let mut total = 0usize;
        let mut with_version = 0usize;
        let mut examples = Vec::new();
        let mut stack = vec![std::path::PathBuf::from(&root)];

        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if file_type.is_dir() {
                    stack.push(path);
                    continue;
                }
                if !file_type.is_file() {
                    continue;
                }
                // `core::osinstall`'s distribution tree carries a `.uaem`
                // metadata sidecar beside every real file plus one root
                // `distribution.json` (see CLAUDE.md's "Installing an OS"
                // section) — ART's own bookkeeping, not Amiga content, and
                // it can never carry a `$VER:` marker. Counting it as a file
                // here would silently double the denominator against a
                // distribution-tree export and understate the real 31%.
                let is_sidecar = path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("uaem"))
                    || path
                        .file_name()
                        .is_some_and(|name| name == "distribution.json");
                if is_sidecar {
                    continue;
                }
                total += 1;
                // Bounded read: the first 1 MiB is enough to hold a $VER:
                // marker in any real Amiga file, and never reading further
                // is what keeps this safe to run over an arbitrary tree.
                let Ok(bytes) = std::fs::read(&path) else {
                    continue;
                };
                let window = &bytes[..bytes.len().min(1024 * 1024)];
                if let Some(version) = read(window) {
                    with_version += 1;
                    if examples.len() < 10 {
                        examples.push(format!(
                            "{} -> {} {}.{}",
                            path.display(),
                            version.name,
                            version.version,
                            version.revision
                        ));
                    }
                }
            }
        }

        println!("{with_version} of {total} files carry a $VER: marker");
        for example in &examples {
            println!("  {example}");
        }
    }

    // ---- read_id_string (2026-09-08) -------------------------------------

    /// **The real bytes, transcribed from the owner's own copies** rather
    /// than invented: `Libs/xadmaster.library` at 9.0 off the AmigaOS 3.9
    /// CD, 9.1 after BoingBag 3.9-1 and 10.0 after `XAD-Update`. Each one's
    /// resident tag reads `rt_Name` then `rt_IdString`, so the id string is
    /// preceded by the library's own filename and a NUL — which is what a
    /// naive scan would trip over and this reader must not.
    #[test]
    fn reads_the_id_string_a_library_with_no_ver_marker_carries() {
        for (id, version, revision) in [
            ("xadmaster 9.0 (25.11.2000) AmigaOS", 9, 0),
            ("xadmaster 9.1 (05.01.2001) AmigaOS", 9, 1),
            ("xadmaster 10.0 (31.03.2001) AmigaOS", 10, 0),
        ] {
            let mut bytes: Vec<u8> = vec![0x90, 0x16, 0x00, 0x00, 0x80, 0x18];
            bytes.extend_from_slice(b"xadmaster.library\x00");
            bytes.extend_from_slice(id.as_bytes());
            bytes.push(0);

            // The premise: there is no `$VER:` here at all, so `read` — the
            // only reader ART had — answers nothing.
            assert_eq!(read(&bytes), None, "{id}");

            let got = read_id_string(&bytes, "xadmaster")
                .unwrap_or_else(|| panic!("no id string found in {id}"));
            assert_eq!((got.version, got.revision), (version, revision), "{id}");
            assert_eq!(got.name, "xadmaster");
        }
    }

    /// **Anchored on the file's own name.** A version label taken from a
    /// string naming some other program is how a file gets labelled with
    /// another program's numbers — `core::osinstall::collide`'s own rule,
    /// and the reason this is not "the first N.N in the file".
    #[test]
    fn a_version_belonging_to_another_program_is_not_this_files() {
        let bytes = b"gcc 2.95 (19.7.1999)\x00some other noise\x00";
        assert_eq!(read_id_string(bytes, "xadmaster"), None);
    }

    /// A word boundary, so a longer name that merely ends in the one asked
    /// for never answers for it.
    #[test]
    fn a_longer_name_ending_in_the_one_asked_for_does_not_answer() {
        let bytes = b"unxadmaster 3.0 (1.1.2000)\x00";
        assert_eq!(read_id_string(bytes, "xadmaster"), None);

        // The control: the same bytes with the boundary present do answer,
        // so the test above is failing on the boundary and not on the shape.
        let ok = b"un xadmaster 3.0 (1.1.2000)\x00";
        assert_eq!(read_id_string(ok, "xadmaster").unwrap().version, 3);
    }

    /// The name occurs before the id string does — `rt_Name` is literally
    /// `xadmaster.library`, and the first match is followed by `.library`,
    /// not by a version. The scan must go on to the next occurrence rather
    /// than giving up at the first one that does not parse.
    #[test]
    fn the_first_occurrence_that_states_no_version_does_not_end_the_search() {
        let mut bytes: Vec<u8> = b"xadmaster.library".to_vec();
        bytes.push(0);
        bytes.extend_from_slice(b"xadmaster 10.0 (31.03.2001)");
        assert_eq!(read_id_string(&bytes, "xadmaster").unwrap().version, 10);
    }

    /// AmigaDOS names are case-insensitive (ART-012), and a library's own
    /// file name and its id string need not agree on case.
    #[test]
    fn the_name_is_matched_case_insensitively() {
        let bytes = b"XADMaster 10.0 (31.03.2001)";
        assert_eq!(read_id_string(bytes, "xadmaster").unwrap().version, 10);
    }

    /// An empty name would match at every position; refused rather than
    /// answering whatever the first parseable thing in the file happens to
    /// be.
    #[test]
    fn an_empty_name_answers_nothing() {
        assert_eq!(read_id_string(b"anything 1.2 (x)", ""), None);
    }
}
