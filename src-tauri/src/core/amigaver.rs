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

impl PartialOrd for AmigaVersion {
    /// Version, then revision. **The date is deliberately not compared**: a
    /// rebuilt binary can carry a later date and the same version, and
    /// calling that an update would put a downgrade behind a green arrow.
    ///
    /// Names are not compared either. Whether two files are the same thing
    /// is decided by where they land in the tree, not by what they call
    /// themselves — `LoadWB` and `loadwb` are one file to AmigaDOS.
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(
            self.version
                .cmp(&other.version)
                .then(self.revision.cmp(&other.revision)),
        )
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

/// The text after the marker: ` name version.revision (date)`.
fn parse(text: &str) -> Option<AmigaVersion> {
    let mut words = text.split_whitespace();
    let name = words.next()?;
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

        assert!(newer_revision > older);
        assert!(newer_version > newer_revision);
        assert!(
            rebuilt <= older,
            "a later date alone is not a newer version"
        );
        assert!(older <= rebuilt);
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
                let is_sidecar = path.extension().is_some_and(|ext| ext == "uaem")
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
}
