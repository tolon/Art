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
    /// The same ROM the tree was planned against. Nothing to report.
    Paired,
    /// A different ROM, and the tree's requirement holds against it.
    Suitable {
        #[serde(rename = "rom")]
        rom: String,
    },
    /// The tree needs a newer Kickstart than the card carries.
    Unsuitable {
        needs: u16,
        /// `None` when the card's ROM states no version at all.
        found: Option<u16>,
        #[serde(rename = "rom")]
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

    if !tree.sha256.is_empty() && tree.sha256.eq_ignore_ascii_case(&card.sha256) {
        return Pairing::Paired;
    }

    match tree.requires_major {
        // The tree carries its own ROM modules, or never depended on the ROM
        // at all. See the design's "a floor nobody has measured": the recipe
        // states no lower bound, so neither does this.
        None => Pairing::Suitable {
            rom: card.name.clone(),
        },
        Some(needs) => match card.stated_major {
            Some(found) if found >= needs => Pairing::Suitable {
                rom: card.name.clone(),
            },
            found => Pairing::Unsuitable {
                needs,
                found,
                rom: card.name.clone(),
            },
        },
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
