//! Does the Kickstart on this card suit the volume about to be written to it?
//! (SD-2 · G9)
//!
//! **A check, not an object.** Both sides already record what they know — the
//! tree its planning ROM (`core::osinstall::PairedRom`), the card its boot
//! files and which of them is the Kickstart — and this compares them. It reads
//! no files, launches nothing and decides nothing: the caller renders the
//! verdict beside a confirmation, and the user chooses.
//!
//! It does **not** ask "is this the same ROM". A different Kickstart is
//! perfectly ordinary; the question is whether the tree's own requirement —
//! recorded at plan time from the recipe's `Condition::RomOlderThan` — still
//! holds against this one.

use serde::{Deserialize, Serialize};

use crate::core::osinstall::PairedRom;

/// The Kickstart a card carries, as its manifest describes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardRom {
    /// The on-card file name, from `SourceFacts::kickstart_file`.
    pub name: String,
    /// Of the bytes placed in the boot partition, from the manifest's
    /// `boot_files` entry — not of the source file (ART-128).
    pub sha256: String,
    /// What that ROM states about itself, when the caller could read it.
    pub stated_major: Option<u16>,
}

/// What ART can say about a tree and a card put together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "kebab-case")]
pub enum Pairing {
    /// The same ROM the tree was planned against, **and** the tree's own
    /// requirement holds against it. Nothing to report.
    ///
    /// Both halves are load-bearing: a tree planned against a ROM that never
    /// satisfied its recipe is not paired with it, however identical the
    /// hashes are.
    Paired,
    /// A different ROM, and the tree's requirement holds against it.
    Suitable { rom: String },
    /// The tree needs a newer Kickstart than the card carries.
    Unsuitable {
        needs: u16,
        /// `None` when the card's ROM states no version at all.
        found: Option<u16>,
        rom: String,
    },
    /// One of the two sides did not answer. **Never rendered as a pass.**
    NotChecked { why: NotCheckedReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NotCheckedReason {
    /// No `distribution.json`, or one written before ART recorded this.
    TreeRecordsNoRom,
    /// No manifest beside the card, no Kickstart in it, or a manifest written
    /// before it named its own Kickstart file.
    CardRecordsNoRom,
}

/// Compare what the tree was planned for against what the card carries.
pub fn compare(tree: Option<&PairedRom>, card: Option<&CardRom>) -> Pairing {
    let Some(tree) = tree else {
        return Pairing::NotChecked {
            why: NotCheckedReason::TreeRecordsNoRom,
        };
    };
    let Some(card) = card else {
        return Pairing::NotChecked {
            why: NotCheckedReason::CardRecordsNoRom,
        };
    };

    // **The requirement is asked first, always.** Identity is not permission:
    // a tree can record the very ROM it was planned against *and* a
    // requirement that ROM never satisfied — plan against a V40 with the
    // ROM-modules component excluded (a supported choice) and the tree says
    // "planned for this V40, needs V47" in the same breath. Taking a `Paired`
    // exit on the hash would then render nothing at all above a destructive
    // confirmation, for exactly the pairing this check exists to warn about.
    // So the recipe's question is put to the card's ROM first; identity only
    // chooses between `Paired` and `Suitable` once the answer is yes.
    //
    // A tree that requires nothing carries its own ROM modules, or never
    // depended on the ROM at all. See the design's "a floor nobody has
    // measured": the recipe states no lower bound, so neither does this.
    if let Some(needs) = tree.requires_major {
        match card.stated_major {
            Some(found) if found >= needs => {}
            found => {
                return Pairing::Unsuitable {
                    needs,
                    found,
                    rom: card.name.clone(),
                }
            }
        }
    }

    // An empty hash is an absent fact, not a value: two of them must never
    // add up to the most reassuring verdict ART has.
    if !tree.sha256.is_empty() && tree.sha256.eq_ignore_ascii_case(&card.sha256) {
        return Pairing::Paired;
    }

    Pairing::Suitable {
        rom: card.name.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(sha: &str, requires: Option<u16>) -> PairedRom {
        PairedRom {
            name: "Kickstart 47.102 (A1200)".into(),
            sha256: sha.into(),
            stated_major: Some(47),
            compatible_models: vec!["A1200".into()],
            requires_major: requires,
        }
    }

    fn card(sha: &str, major: Option<u16>) -> CardRom {
        CardRom {
            name: "kick.rom".into(),
            sha256: sha.into(),
            stated_major: major,
        }
    }

    fn card_named(name: &str, sha: &str, major: Option<u16>) -> CardRom {
        CardRom {
            name: name.into(),
            sha256: sha.into(),
            stated_major: major,
        }
    }

    #[test]
    fn the_same_rom_is_paired_and_says_nothing() {
        let verdict = compare(Some(&tree("aa", Some(47))), Some(&card("aa", Some(47))));
        assert!(matches!(verdict, Pairing::Paired));
    }

    #[test]
    fn a_tree_that_carries_its_modules_suits_any_rom() {
        let verdict = compare(
            Some(&tree("aa", None)),
            Some(&card_named("fallback-v40.rom", "bb", Some(40))),
        );
        match verdict {
            Pairing::Suitable { rom } => {
                assert_eq!(rom, "fallback-v40.rom");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_newer_rom_than_required_suits() {
        let verdict = compare(
            Some(&tree("aa", Some(47))),
            Some(&card_named("suitable-v47.rom", "bb", Some(47))),
        );
        match verdict {
            Pairing::Suitable { rom } => {
                assert_eq!(rom, "suitable-v47.rom");
            }
            other => panic!("{other:?}"),
        }
    }

    /// The pairing that failed on 2026-08-16: a V47 tree, a V40 card.
    #[test]
    fn an_older_rom_than_required_is_unsuitable_and_says_both_numbers() {
        let verdict = compare(
            Some(&tree("aa", Some(47))),
            Some(&card_named("unsuitable-v40.rom", "bb", Some(40))),
        );
        match verdict {
            Pairing::Unsuitable { needs, found, rom } => {
                assert_eq!(needs, 47);
                assert_eq!(found, Some(40));
                assert_eq!(rom, "unsuitable-v40.rom");
            }
            other => panic!("{other:?}"),
        }
    }

    /// A ROM that states nothing cannot be the V47 the tree needs.
    #[test]
    fn a_rom_that_states_no_version_cannot_satisfy_a_requirement() {
        let verdict = compare(
            Some(&tree("aa", Some(47))),
            Some(&card_named("unknown-version.rom", "bb", None)),
        );
        match verdict {
            Pairing::Unsuitable { found, rom, .. } => {
                assert_eq!(found, None);
                assert_eq!(rom, "unknown-version.rom");
            }
            other => panic!("{other:?}"),
        }
    }

    /// **The silence that blocked the merge.** A tree can record a ROM it was
    /// planned against *and* a requirement that ROM does not satisfy — the
    /// user excludes `modules-a1200` while planning against their V40, which
    /// `OsInstall.tsx` supports on purpose, and `plan()` then records
    /// `{ stated_major: 40, requires_major: 47 }`. Build the card with that
    /// same ROM and the two hashes match. Identity must not be allowed to
    /// answer a question it was never asked: a pairing that never satisfied
    /// its own recipe is not one to be silent about.
    #[test]
    fn identity_does_not_excuse_a_requirement_the_rom_never_met() {
        let same = "deadbeef";
        let verdict = compare(
            Some(&tree(same, Some(47))),
            Some(&card_named("planned-against-v40.rom", same, Some(40))),
        );
        match verdict {
            Pairing::Unsuitable { needs, found, rom } => {
                assert_eq!(needs, 47);
                assert_eq!(found, Some(40));
                assert_eq!(rom, "planned-against-v40.rom");
            }
            other => {
                panic!("the same ROM is not a pass when it never met the requirement: {other:?}")
            }
        }
    }

    /// The other half of the same restructure: identity still decides between
    /// `Paired` and `Suitable`, once the requirement holds.
    #[test]
    fn identity_still_pairs_when_the_requirement_holds() {
        let same = "deadbeef";
        let verdict = compare(
            Some(&tree(same, Some(47))),
            Some(&card_named("the-very-rom.rom", same, Some(47))),
        );
        assert_eq!(verdict, Pairing::Paired);
    }

    /// Two absent facts are not a match. Without the emptiness guard an
    /// unhashed tree and an unhashed card would produce the most reassuring
    /// verdict ART has out of nothing at all.
    #[test]
    fn two_empty_hashes_are_not_the_same_rom() {
        let verdict = compare(
            Some(&tree("", None)),
            Some(&card_named("unhashed.rom", "", Some(47))),
        );
        match verdict {
            Pairing::Suitable { rom } => assert_eq!(rom, "unhashed.rom"),
            other => panic!("an empty hash matches nothing: {other:?}"),
        }
    }

    /// What `src/lib/preload.ts`'s `Pairing` union has to match, pinned.
    ///
    /// The two `#[serde(rename = "rom")]` attributes deleted here were no-ops
    /// — a container's `rename_all` renames an enum's *variants*, not a
    /// struct-variant's fields — so they implied a rule that was never in
    /// force. This is the rule, in the only form that can fail: the variant
    /// tags are kebab-case, and every field reaches the screen under the name
    /// the frontend reads it by.
    #[test]
    fn the_wire_shape_is_what_the_frontend_reads() {
        let json = |verdict: Pairing| serde_json::to_value(verdict).unwrap();

        assert_eq!(
            json(Pairing::Paired),
            serde_json::json!({ "verdict": "paired" })
        );
        assert_eq!(
            json(Pairing::Suitable {
                rom: "kick.rom".into()
            }),
            serde_json::json!({ "verdict": "suitable", "rom": "kick.rom" })
        );
        assert_eq!(
            json(Pairing::Unsuitable {
                needs: 47,
                found: Some(40),
                rom: "kick.rom".into()
            }),
            serde_json::json!({
                "verdict": "unsuitable",
                "needs": 47,
                "found": 40,
                "rom": "kick.rom"
            })
        );
        assert_eq!(
            json(Pairing::NotChecked {
                why: NotCheckedReason::CardRecordsNoRom
            }),
            serde_json::json!({ "verdict": "not-checked", "why": "card-records-no-rom" })
        );
        assert_eq!(
            json(Pairing::NotChecked {
                why: NotCheckedReason::TreeRecordsNoRom
            }),
            serde_json::json!({ "verdict": "not-checked", "why": "tree-records-no-rom" })
        );
    }

    /// A missing answer is a missing answer, never a pass (§89).
    #[test]
    fn a_missing_side_is_not_checked_rather_than_paired() {
        let no_tree = compare(None, Some(&card_named("no-tree.rom", "aa", Some(47))));
        assert_eq!(
            no_tree,
            Pairing::NotChecked {
                why: NotCheckedReason::TreeRecordsNoRom
            }
        );

        let no_card = compare(Some(&tree("aa", Some(47))), None);
        assert_eq!(
            no_card,
            Pairing::NotChecked {
                why: NotCheckedReason::CardRecordsNoRom
            }
        );
    }
}
