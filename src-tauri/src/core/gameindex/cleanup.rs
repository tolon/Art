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

use std::collections::{BTreeMap, BTreeSet};

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

    for word in ["disk", "disc", "d"] {
        let Some(head) = trimmed_lower.strip_suffix(word) else {
            continue;
        };
        // A bare `d` needs something separating it from the name, or `Virus 3D`
        // loses its D and `Speedball 2 HD` becomes `H`. The spelled-out words
        // do not: `LightwaveDisk2` is real and carries no separator, while
        // nothing a game is called ends in "disk" by accident — `Diskette`
        // fails the suffix test on its own.
        if word == "d" && !(head.is_empty() || head.ends_with(SEPARATORS)) {
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

// ---------------------------------------------------------------------------
// Disk sets, which one name can never settle on its own.
// ---------------------------------------------------------------------------

/// The bases that look like real multi-disk sets, given every name present.
///
/// Measured against a real collection: 551 of 847 ADFs belong to a numbered
/// set, in 174 groups, and 163 of those groups begin at disk one.
///
/// **No disk one, no set**, and that rule is carrying more weight than it
/// looks. `LSD_042` … `LSD_064` are eighteen issues of a disk magazine, not one
/// program's disks; they share a base and they are numbered, and folding them
/// together would collapse eighteen separate things into one. The same rule
/// leaves `Turrican 2` beside `Turrican 3` alone, which is the other way this
/// goes wrong. What it cannot settle — `brian the lion 2` with no disk one
/// anywhere — is left for the user to type, which is where a guess belongs.
pub fn disk_sets(all_names: &[String]) -> BTreeSet<String> {
    let mut seen: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
    for name in all_names {
        if let Some((base, number)) = split_trailing_number(name) {
            seen.entry(base).or_default().insert(number);
        }
    }
    seen.into_iter()
        .filter(|(_, numbers)| numbers.len() > 1 && numbers.contains(&1))
        .map(|(base, _)| base)
        .collect()
}

/// Split a name into its base and a trailing number, if it has one.
///
/// The separator may be absent — `apoc1` and `another world1` are both real.
fn split_trailing_number(raw: &str) -> Option<(String, u32)> {
    let tidied = tidy(raw);
    let digits_start = tidied
        .char_indices()
        .rev()
        .take_while(|(_, ch)| ch.is_ascii_digit())
        .last()
        .map(|(index, _)| index)?;

    let number = tidied[digits_start..].parse::<u32>().ok()?;
    let base = tidied[..digits_start].trim_end_matches(SEPARATORS).trim();
    if base.is_empty() {
        return None;
    }
    Some((base.to_string(), number))
}

/// The set base this name belongs to, if `sets` says it is one.
pub fn base_for(raw: &str, sets: &BTreeSet<String>) -> Option<String> {
    let (base, _) = split_trailing_number(raw)?;
    sets.contains(&base).then_some(base)
}

/// The title to propose for one name, given every name beside it.
///
/// An explicit disk word wins: `LightwaveDisk2` says what it is whether or not
/// disk one was ever copied. Only when the name says nothing do the neighbours
/// get a vote.
pub fn suggest_in_set(raw: &str, sets: &BTreeSet<String>) -> Option<String> {
    if let Some(from_marker) = suggest_title(raw) {
        return Some(from_marker);
    }
    let base = base_for(raw, sets)?;
    (base != raw.trim()).then_some(base)
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

    // -- disk sets, which only the neighbours can settle -----------------------

    /// The case the whole sibling rule exists for.
    ///
    /// `dune2-2` is Dune II's second disk, and nothing in that name says so —
    /// `dune2` itself ends in a digit. What settles it is `dune2-1` lying
    /// beside it.
    #[test]
    fn a_numbered_file_with_a_first_disk_beside_it_is_a_disk_set() {
        let all = vec!["dune2-1".to_string(), "dune2-2".to_string()];
        let sets = disk_sets(&all);
        assert_eq!(base_for("dune2-2", &sets).as_deref(), Some("dune2"));
        assert_eq!(base_for("dune2-1", &sets).as_deref(), Some("dune2"));
    }

    #[test]
    fn the_separators_a_real_collection_uses_all_work() {
        let all: Vec<String> = [
            "4D Driving 1",
            "4D Driving 2",
            "apoc1",
            "apoc2",
            "apoc3",
            "another world1",
            "another world2",
            "ADPro_D1",
            "ADPro_D2",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let sets = disk_sets(&all);

        assert_eq!(
            base_for("4D Driving 2", &sets).as_deref(),
            Some("4D Driving")
        );
        assert_eq!(base_for("apoc3", &sets).as_deref(), Some("apoc"));
        assert_eq!(
            base_for("another world2", &sets).as_deref(),
            Some("another world")
        );
    }

    /// A number with nobody beside it says nothing at all. `Turrican 2` on its
    /// own is a game, not a disk.
    #[test]
    fn a_lone_numbered_file_is_not_a_disk_set() {
        let all = vec!["Turrican 2".to_string(), "Shadow of the Beast".to_string()];
        assert_eq!(base_for("Turrican 2", &disk_sets(&all)), None);
    }

    /// The eighteen files that made this rule careful.
    ///
    /// `LSD_042` … `LSD_064` are issues of the LSD Legal Tools disk magazine,
    /// not one program's disks. They share a base and they are numbered, and
    /// folding them into a single "LSD" would collapse eighteen separate things
    /// into one. **No disk one, no set** is what keeps them apart — and it is
    /// also what leaves `Turrican 2` and `Turrican 3` alone.
    #[test]
    fn a_numbered_series_that_does_not_start_at_one_is_left_alone() {
        let all: Vec<String> = ["LSD_042", "LSD_046", "LSD_048", "LSD_049"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let sets = disk_sets(&all);
        assert_eq!(base_for("LSD_046", &sets), None);

        let sequels: Vec<String> = ["Turrican 2", "Turrican 3"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(base_for("Turrican 2", &disk_sets(&sequels)), None);
    }

    /// An explicit disk word does not need the neighbours: `LightwaveDisk2`
    /// says what it is, whether or not disk one was ever copied.
    #[test]
    fn an_explicit_disk_word_needs_no_sibling() {
        let all = vec!["LightwaveDisk2".to_string(), "LightwaveDisk7".to_string()];
        let sets = disk_sets(&all);
        // Not through the sibling rule — through the marker, which
        // `suggest_title` applies on its own.
        assert_eq!(base_for("LightwaveDisk2", &sets), None);
        assert_eq!(
            suggest_title("LightwaveDisk2").as_deref(),
            Some("Lightwave")
        );
        assert_eq!(
            suggest_title("dawn_patrol_disc2").as_deref(),
            Some("dawn patrol")
        );
        assert_eq!(
            suggest_title("mortal kombat 2 d2").as_deref(),
            Some("mortal kombat 2")
        );
    }

    /// The whole point, end to end: the suggestion a screen would show.
    #[test]
    fn suggest_in_set_prefers_the_marker_then_falls_back_to_the_siblings() {
        let all: Vec<String> = ["dune2-1", "dune2-2", "A-Train Disk 1", "Turrican 2"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let sets = disk_sets(&all);

        assert_eq!(suggest_in_set("dune2-2", &sets).as_deref(), Some("dune2"));
        assert_eq!(
            suggest_in_set("A-Train Disk 1", &sets).as_deref(),
            Some("A-Train")
        );
        assert_eq!(suggest_in_set("Turrican 2", &sets), None);
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

        let sets = disk_sets(&stems);
        eprintln!("{} names form {} disk sets\n", stems.len(), sets.len());

        let mut titled = 0;
        let mut renamed = 0;
        let mut shown = 0;
        for stem in &stems {
            let title = suggest_in_set(stem, &sets);
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

        // How much the library actually shrinks, which is the point of all of
        // it: 847 files are not 847 games.
        let distinct: std::collections::BTreeSet<String> = stems
            .iter()
            .map(|stem| suggest_in_set(stem, &sets).unwrap_or_else(|| stem.clone()))
            .collect();
        eprintln!(
            "{} files resolve to {} distinct titles",
            stems.len(),
            distinct.len()
        );

        // The case a looser rule would have swallowed, asserted as the property
        // that actually matters rather than as "no suggestion".
        //
        // `LSD_042` … `LSD_064` are issues of a disk magazine. They may be
        // tidied — `LSD_042` reads better as `LSD 042` — but they must stay
        // **as many titles as there are issues**. Folding eighteen separate
        // things into one "LSD" is the failure this rule exists to avoid, and
        // an earlier version of this assertion tested the wrong thing: it
        // demanded no suggestion at all, which a cosmetic fix quite properly
        // gave it.
        let issues: Vec<&String> = stems.iter().filter(|s| s.starts_with("LSD_")).collect();
        if !issues.is_empty() {
            let resolved: std::collections::BTreeSet<String> = issues
                .iter()
                .map(|s| suggest_in_set(s, &sets).unwrap_or_else(|| (*s).clone()))
                .collect();
            assert_eq!(
                resolved.len(),
                issues.len(),
                "{} magazine issues collapsed into {} titles",
                issues.len(),
                resolved.len()
            );
        }
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
