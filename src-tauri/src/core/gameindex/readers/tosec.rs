//! A filename, read for what it suggests.
//!
//! This is the weakest of the readers and the record says so: every fact it
//! produces carries `Provenance::TosecName`. It is still the only source for
//! 847 loose `.adf` files on this machine.
//!
//! Moved here from `core/collection.rs` rather than copied. Two parsers for one
//! convention is the shape `core/adf/mutate.rs` was retired to avoid.
//!
//! **The chipset guess is a substring match and it is wrong sometimes.** A name
//! containing `AGA` anywhere — `Agassi Tennis`, `Sagaland` — reads as AGA. That
//! was true before the move and is left alone here, because a reader is the
//! wrong place to fix it: the record carries where the fact came from, so a
//! caller with a slave's own `ReqAGA` bit can and does overrule this.

use crate::core::gameindex::record::ChipsetRequirement;

/// What a filename suggests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TosecFacts {
    pub title: String,
    pub year: Option<u16>,
    pub publisher: Option<String>,
    /// `None` when the name says nothing. **Not** `OcsEcs` — the old parser
    /// defaulted, and a default rendered beside stated facts reads as one.
    pub chipset: Option<ChipsetRequirement>,
    /// `(index, total)` when the name says which disk this is.
    pub disk: Option<(usize, usize)>,
}

/// Parse TOSEC formatted filename metadata.
///
/// Example: `Sensible World of Soccer 96-97 (1996)(Renegade)(AGA)(Disk 1 of 2)[!]`
pub fn read_filename(filename: &str) -> TosecFacts {
    let name_without_ext = if let Some(dot_idx) = filename.rfind('.') {
        &filename[..dot_idx]
    } else {
        filename
    };

    let mut clean_title;
    let mut year = None;
    let mut publisher = None;
    let mut chipset = None;
    let mut disk_info = None;

    // Check for AGA indicators in filename
    let upper = filename.to_uppercase();
    if upper.contains("AGA")
        || upper.contains("CD32")
        || upper.contains("A1200")
        || upper.contains("68020")
    {
        chipset = Some(ChipsetRequirement::Aga);
    }

    // Extract parentheses tokens: (1996), (Renegade), (Disk 1 of 2), etc.
    let mut tokens = Vec::new();
    let mut base_name = String::new();
    let mut in_paren = false;
    let mut in_bracket = false;
    let mut cur_token = String::new();

    for c in name_without_ext.chars() {
        if c == '(' {
            in_paren = true;
            cur_token.clear();
        } else if c == ')' {
            in_paren = false;
            tokens.push(cur_token.trim().to_string());
            cur_token.clear();
        } else if c == '[' {
            in_bracket = true;
        } else if c == ']' {
            in_bracket = false;
        } else if in_paren {
            cur_token.push(c);
        } else if !in_bracket && !c.is_control() {
            base_name.push(c);
        }
    }

    clean_title = base_name
        .trim()
        .trim_end_matches('_')
        .trim_end_matches('-')
        .trim()
        .to_string();
    if clean_title.is_empty() {
        clean_title = name_without_ext.to_string();
    }

    for t in tokens {
        let t_upper = t.to_uppercase();

        // 1. Year pattern (e.g. "1991", "1996")
        if t.len() == 4 && t.chars().all(|ch| ch.is_ascii_digit()) {
            if let Ok(y) = t.parse::<u16>() {
                if (1980..=2030).contains(&y) {
                    year = Some(y);
                    continue;
                }
            }
        }

        // 2. Disk pattern (e.g. "Disk 1 of 2", "Disk 1", "Disk A")
        if t_upper.contains("DISK") {
            let parts: Vec<&str> = t_upper.split_whitespace().collect();
            if let Some(pos) = parts.iter().position(|&x| x == "DISK") {
                if pos + 1 < parts.len() {
                    let d_num = parts[pos + 1].parse::<usize>().unwrap_or(1);
                    let mut total = d_num;
                    if let Some(of_pos) = parts.iter().position(|&x| x == "OF") {
                        if of_pos + 1 < parts.len() {
                            total = parts[of_pos + 1].parse::<usize>().unwrap_or(d_num);
                        }
                    }
                    disk_info = Some((d_num, total));
                    continue;
                }
            }
        }

        // 3. Publisher
        if publisher.is_none()
            && !t_upper.contains("AGA")
            && !t_upper.contains("PAL")
            && !t_upper.contains("NTSC")
            && !t_upper.contains("CRACK")
        {
            publisher = Some(t);
        }
    }

    TosecFacts {
        title: clean_title,
        year,
        publisher,
        chipset,
        disk: disk_info,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape TOSEC names actually take.
    #[test]
    fn a_tosec_name_yields_its_parts() {
        let facts = read_filename(
            "Sensible World of Soccer 96-97 (1996)(Renegade)(AGA)(Disk 1 of 2)[!].adf",
        );
        assert_eq!(facts.title, "Sensible World of Soccer 96-97");
        assert_eq!(facts.year, Some(1996));
        assert_eq!(facts.publisher.as_deref(), Some("Renegade"));
        assert_eq!(facts.chipset, Some(ChipsetRequirement::Aga));
        assert_eq!(facts.disk, Some((1, 2)));
    }

    /// A name with no tokens at all still gives a title and admits it knows
    /// nothing else — the Enzo collection is almost entirely this shape.
    #[test]
    fn a_bare_name_gives_a_title_and_no_claims() {
        let facts = read_filename("A Prehistoric Tale v1.1.hdf");
        assert_eq!(facts.title, "A Prehistoric Tale v1.1");
        assert_eq!(facts.year, None);
        assert_eq!(facts.publisher, None);
        assert_eq!(facts.chipset, None, "an unmarked name states no chipset");
        assert_eq!(facts.disk, None);
    }

    /// The old parser defaulted chipset to OCS/ECS when nothing said otherwise,
    /// which reads as a claim. Here silence is `None`, so the record cannot
    /// present a default as a fact.
    #[test]
    fn silence_about_chipset_is_not_a_claim_of_ocs() {
        assert_eq!(read_filename("Zool.adf").chipset, None);
        assert_eq!(
            read_filename("Zool (1992)(Gremlin)(AGA).adf").chipset,
            Some(ChipsetRequirement::Aga)
        );
    }

    /// The substring match's own false positive, pinned rather than hidden.
    ///
    /// `Agassi Tennis` has no AGA requirement; the letters just happen to be
    /// there. This is why `Fact` records its provenance — a slave's `ReqAGA`
    /// bit overrules this, and a reader of the record can see which answered.
    #[test]
    fn the_chipset_guess_has_a_known_false_positive() {
        assert_eq!(
            read_filename("Agassi Tennis.adf").chipset,
            Some(ChipsetRequirement::Aga),
            "a substring match cannot tell AGA from Agassi"
        );
    }

    /// A multi-disk name without an "of" still says which disk it is.
    #[test]
    fn a_disk_without_a_total_reports_itself_as_the_total() {
        assert_eq!(read_filename("Game (Disk 3).adf").disk, Some((3, 3)));
    }

    /// Moved from `core/collection.rs` with the parser, not rewritten there —
    /// the same way `core/adf/mutate.rs`'s tests travelled to
    /// `core/volume/write` when it was retired.
    ///
    /// One assertion changed and only one: it used to expect `OcsEcs` for a
    /// name that says nothing about the chipset. That default now lives at the
    /// call site in `core/collection.rs`, so what the *reader* returns here is
    /// `None`.
    #[test]
    fn parse_tosec_filename() {
        let facts = read_filename(
            "Monkey Island 2 - LeChuck's Revenge (1992)(LucasArts)(Disk 1 of 11)[!].adf",
        );
        assert_eq!(facts.title, "Monkey Island 2 - LeChuck's Revenge");
        assert_eq!(facts.year, Some(1992));
        assert_eq!(facts.publisher, Some("LucasArts".into()));
        assert_eq!(facts.chipset, None);
        assert_eq!(facts.disk, Some((1, 11)));
    }

    /// Also moved with the parser. Unchanged: this name really does state AGA.
    #[test]
    fn parse_aga_game_filename() {
        let facts = read_filename("Alien Breed 3D (1995)(Ocean)(AGA)(Disk 1 of 3).adf");
        assert_eq!(facts.title, "Alien Breed 3D");
        assert_eq!(facts.year, Some(1995));
        assert_eq!(facts.publisher, Some("Ocean".into()));
        assert_eq!(facts.chipset, Some(ChipsetRequirement::Aga));
        assert_eq!(facts.disk, Some((1, 3)));
    }
}
