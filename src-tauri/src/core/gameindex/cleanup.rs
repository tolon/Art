//! Suggestions for a name the index could only take from a filename.
//!
//! ART's ADF path assumed TOSEC naming. Measured against a real 847-file
//! collection, **none** of it is TOSEC: the files are hand-named
//! (`A-Train Disk 1.adf`, `ADPro_D3.adf`, `CaptiveII_Disk1.adf`), so the title
//! carries a disk number, the same game appears as several entries, and nothing
//! matches an artwork index — 3 % against 60 % for the WHDLoad folder next to
//! it.
//!
//! **Nothing here is applied on its own.** The project's decision was that ART
//! must not guess at a name; these functions *propose* and the user accepts,
//! which is why a case with no confident answer returns `None` rather than a
//! best effort. A tool that quietly renames is the thing that was refused.
//!
//! Two functions and not one, because a title and a filename want opposite
//! things from a disk number: `A-Train Disk 1` and `A-Train Disk 2` are one
//! game, but they must stay two files.

/// Characters a Windows filename cannot hold.
const REFUSED_IN_FILENAME: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Separators a hand-named file uses before a disk marker.
const SEPARATORS: [char; 4] = [' ', '-', '_', '^'];

/// What a disk marker turned out to be, once removed.
struct Split {
    /// The name without the marker.
    stem: String,
    /// The disk number, when the marker carried one.
    disk: Option<u32>,
    /// Whether a marker was found at all.
    found: bool,
}

/// Normalise separators and collapse runs of whitespace.
fn tidy(raw: &str) -> String {
    let spaced: String = raw
        .chars()
        .map(|ch| if ch == '_' { ' ' } else { ch })
        .collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Take a trailing disk marker off a name.
///
/// Only markers that *say* disk are recognised — `Disk 3`, `_D3`, `-Disk^1`. A
/// bare trailing number is left where it is; see the test that explains why.
fn split_marker(raw: &str) -> Split {
    let tidied = tidy(raw);
    let lower = tidied.to_lowercase();

    // Trailing digits, if any, and where they start.
    let digits_start = lower
        .char_indices()
        .rev()
        .take_while(|(_, ch)| ch.is_ascii_digit())
        .last()
        .map(|(index, _)| index);

    let (before_digits, disk) = match digits_start {
        Some(index) => (&tidied[..index], tidied[index..].parse::<u32>().ok()),
        None => (tidied.as_str(), None),
    };

    // What sits between the name and the digits: separators, then the word.
    let trimmed = before_digits.trim_end_matches(SEPARATORS);
    let trimmed_lower = trimmed.to_lowercase();

    for word in ["disk", "d"] {
        let Some(head) = trimmed_lower.strip_suffix(word) else {
            continue;
        };
        // `d` is only a marker when something separates it from the name;
        // otherwise `Virus 3D` loses its D and `HD` becomes `H`.
        let head_ends_with_separator = head.is_empty() || head.ends_with(SEPARATORS);
        if word == "d" && !head_ends_with_separator {
            continue;
        }
        // `disk` needs a separator too, or `Diskette` would be cut short —
        // except when the whole name is the marker, which is handled by the
        // emptiness check below.
        if word == "disk" && !head_ends_with_separator {
            continue;
        }
        let stem = trimmed[..head.len()]
            .trim_end_matches(SEPARATORS)
            .to_string();
        return Split {
            stem,
            disk,
            found: true,
        };
    }

    Split {
        stem: tidied,
        disk: None,
        found: false,
    }
}

/// A cleaner title, or `None` when there is nothing to propose.
///
/// The disk number is dropped: several disks are one game.
pub fn suggest_title(raw: &str) -> Option<String> {
    let split = split_marker(raw);
    let candidate = split.stem.trim().to_string();

    // Removing the marker must not remove the name.
    if candidate.is_empty() {
        return None;
    }
    if candidate == raw.trim() {
        return None;
    }
    Some(candidate)
}

/// A cleaner filename stem, or `None` when there is nothing to propose.
///
/// The disk number is **kept**, in one form, because two files cannot share a
/// name. Characters a filesystem refuses are dropped rather than escaped: this
/// becomes a real filename.
pub fn suggest_stem(raw: &str) -> Option<String> {
    let split = split_marker(raw);
    let name = split.stem.trim();
    if name.is_empty() {
        return None;
    }

    let candidate = match split.disk {
        Some(disk) => format!("{name} (Disk {disk})"),
        None => name.to_string(),
    };
    let candidate: String = candidate
        .chars()
        .filter(|ch| !REFUSED_IN_FILENAME.contains(ch))
        .collect();
    let candidate = candidate.split_whitespace().collect::<Vec<_>>().join(" ");

    if candidate.is_empty() || candidate == raw.trim() {
        return None;
    }
    Some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every case here is a real filename from a real collection: 847 ADFs, of
    /// which **none** are TOSEC-named. The parser ART had assumed a naming
    /// convention this material does not use.
    #[test]
    fn a_disk_marker_is_removed_from_a_title() {
        assert_eq!(suggest_title("A-Train Disk 1").as_deref(), Some("A-Train"));
        assert_eq!(
            suggest_title("CaptiveII_Disk1").as_deref(),
            Some("CaptiveII")
        );
        assert_eq!(suggest_title("ADPro_D1").as_deref(), Some("ADPro"));
        assert_eq!(
            suggest_title("champ man 93 d1").as_deref(),
            Some("champ man 93")
        );
        assert_eq!(suggest_title("ADORAGE-Disk^1").as_deref(), Some("ADORAGE"));
        assert_eq!(suggest_title("ADORAGE-Disk").as_deref(), Some("ADORAGE"));
    }

    #[test]
    fn underscores_become_spaces() {
        assert_eq!(
            suggest_title("A_Impossible_Mission").as_deref(),
            Some("A Impossible Mission")
        );
    }

    /// Nothing to remove means no suggestion, and therefore no button. A tool
    /// that offers to "fix" a name that is already right teaches the user to
    /// click without reading.
    #[test]
    fn a_clean_title_gets_no_suggestion() {
        assert_eq!(suggest_title("688 Attack Sub"), None);
        assert_eq!(suggest_title("Turrican II"), None);
        assert_eq!(suggest_title("Shadow of the Beast"), None);
    }

    /// The rule this deliberately does **not** have.
    ///
    /// `4D Driving 1` is disk one; `Turrican 2` is a different game. Nothing in
    /// either name says which, and a collection holding `Turrican 2` and
    /// `Turrican 3` looks exactly like a two-disk set. Guessing here would
    /// rename a sequel after its predecessor, so a bare trailing number is left
    /// alone and the user edits it by hand.
    #[test]
    fn a_bare_trailing_number_is_left_alone() {
        assert_eq!(suggest_title("4D Driving 1"), None);
        assert_eq!(suggest_title("Turrican 2"), None);
        assert_eq!(suggest_title("Lemmings 2"), None);
    }

    /// Removing the marker must not remove the name.
    #[test]
    fn a_title_that_is_only_a_marker_gets_no_suggestion() {
        assert_eq!(suggest_title("Disk 1"), None);
        assert_eq!(suggest_title("_D2"), None);
        assert_eq!(suggest_title("   "), None);
    }

    /// A `d` inside a word is not a disk marker.
    #[test]
    fn a_word_ending_in_d_survives() {
        assert_eq!(suggest_title("Virus 3D"), None);
        assert_eq!(suggest_title("Speedball 2 HD"), None);
    }

    // -- filenames -----------------------------------------------------------

    /// A filename keeps the disk apart, because two files cannot share a name.
    ///
    /// This is the difference the tool has to get right: the *title* of
    /// `A-Train Disk 1` and `A-Train Disk 2` is the same game, but the two
    /// files must stay two files.
    #[test]
    fn a_filename_keeps_its_disk_number_in_a_normal_form() {
        assert_eq!(
            suggest_stem("A-Train Disk 1").as_deref(),
            Some("A-Train (Disk 1)")
        );
        assert_eq!(suggest_stem("ADPro_D3").as_deref(), Some("ADPro (Disk 3)"));
        assert_eq!(
            suggest_stem("CaptiveII_Disk5").as_deref(),
            Some("CaptiveII (Disk 5)")
        );
    }

    /// A marker with no number carries none.
    #[test]
    fn a_marker_without_a_number_leaves_no_number_behind() {
        assert_eq!(suggest_stem("ADORAGE-Disk").as_deref(), Some("ADORAGE"));
    }

    /// A file with no disk marker but underscores through it is still worth
    /// offering to rename — it is the same tidying the title gets, and the user
    /// confirms it either way.
    #[test]
    fn underscores_alone_are_enough_to_offer_a_rename() {
        assert_eq!(
            suggest_stem("A_Impossible_Mission").as_deref(),
            Some("A Impossible Mission")
        );
        assert_eq!(
            suggest_stem("AMOSPro_System").as_deref(),
            Some("AMOSPro System")
        );
    }

    /// A bare trailing number stays on the filename too, and for the same
    /// reason: nothing says whether it is a disk or a sequel.
    #[test]
    fn a_bare_trailing_number_stays_on_a_filename() {
        assert_eq!(suggest_stem("4D Driving 1"), None);
        assert_eq!(suggest_stem("Turrican 2"), None);
    }

    #[test]
    fn a_clean_filename_gets_no_suggestion() {
        assert_eq!(suggest_stem("688 Attack Sub"), None);
        assert_eq!(suggest_stem("Turrican II"), None);
    }

    /// A suggestion that is already in the normal form must not be offered
    /// again, or the button never goes away.
    #[test]
    fn an_already_normal_filename_gets_no_suggestion() {
        assert_eq!(suggest_stem("A-Train (Disk 1)"), None);
    }

    /// What these rules do to a real collection, rather than to the dozen
    /// examples above that were chosen because they are interesting.
    ///
    /// Run it against a folder of hand-named images:
    ///
    /// ```text
    /// set ART_REAL_TITLES=E:\amiga\Titles
    /// cargo test real_hand_named_files -- --ignored --nocapture
    /// ```
    ///
    /// It asserts nothing about the counts — they are a property of somebody's
    /// disk, not of the code — but it does assert the one invariant that must
    /// hold everywhere: a suggestion is never empty and never longer than what
    /// it replaced, because these rules only remove.
    #[test]
    #[ignore = "needs a real folder; set ART_REAL_TITLES"]
    fn real_hand_named_files() {
        let Ok(dir) = std::env::var("ART_REAL_TITLES") else {
            eprintln!("ART_REAL_TITLES is not set");
            return;
        };

        fn walk(dir: &std::path::Path, into: &mut Vec<String>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, into);
                } else if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("adf"))
                {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        into.push(stem.to_string());
                    }
                }
            }
        }

        let mut stems = Vec::new();
        walk(std::path::Path::new(&dir), &mut stems);
        assert!(!stems.is_empty(), "no .adf files under {dir}");

        let mut titled = 0;
        let mut renamed = 0;
        let mut shown = 0;
        for stem in &stems {
            let title = suggest_title(stem);
            let file = suggest_stem(stem);
            if title.is_some() {
                titled += 1;
            }
            if file.is_some() {
                renamed += 1;
            }
            if (title.is_some() || file.is_some()) && shown < 12 {
                shown += 1;
                eprintln!(
                    "  {stem:32} -> title {:?}  file {:?}",
                    title.as_deref().unwrap_or("-"),
                    file.as_deref().unwrap_or("-")
                );
            }
            // These rules only ever remove, so a proposal is never longer.
            if let Some(proposed) = &title {
                assert!(!proposed.trim().is_empty(), "empty title for {stem:?}");
                assert!(
                    proposed.chars().count() <= stem.chars().count(),
                    "{proposed:?} is longer than {stem:?}"
                );
            }
        }

        eprintln!(
            "\n{} .adf files: {titled} would get a title suggestion, {renamed} a filename one",
            stems.len()
        );
    }

    /// Whatever comes out is going to become a filename, so it must not carry
    /// anything a filesystem refuses.
    #[test]
    fn a_suggested_stem_carries_no_character_windows_refuses() {
        for raw in ["A/B Disk 1", "what?_D2", "a:b Disk 3", "x*y_Disk1"] {
            if let Some(stem) = suggest_stem(raw) {
                for bad in ['<', '>', ':', '"', '/', '\\', '|', '?', '*'] {
                    assert!(!stem.contains(bad), "{stem:?} still holds {bad:?}");
                }
            }
        }
    }
}
