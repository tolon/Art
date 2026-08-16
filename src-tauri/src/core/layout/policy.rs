//! Which drawer a thing goes in — the rules, as data.
//!
//! **A named field per drawer rather than a rule list**, and the reason is the
//! compiler: `drawer_for` matches every [`ItemKind`], so a kind added later
//! cannot quietly fall through to "somewhere". A `Vec<Rule>` keyed by kind
//! would need a runtime lookup and a default, and the default is exactly the
//! silent answer this design is trying not to give.

use serde::{Deserialize, Serialize};

use crate::core::layout::{ItemKind, RefusalReason};

/// What happens to an archive holding a WHDLoad pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WhdloadPlacement {
    /// Unpacked into a drawer with its icon beside it, so the card arrives
    /// ready and the game is visible on Workbench. The default, because that
    /// is the point of the feature.
    #[default]
    Unpack,
    /// Copied in as the `.lha` it is; unpacking is the user's job on the Amiga.
    AsArchive,
}

/// Where each kind of thing goes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    pub whdload: WhdloadPlacement,
    pub games: String,
    pub floppies: String,
    pub hard_disks: String,
    pub discs: String,
    pub unsorted: String,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            whdload: WhdloadPlacement::default(),
            games: "Games".into(),
            floppies: "Floppies".into(),
            hard_disks: "HardDisks".into(),
            discs: "CDs".into(),
            unsorted: "Unsorted".into(),
        }
    }
}

/// The drawer `kind` belongs in, or the reason it belongs on no Amiga volume.
///
/// `Err` is a **refusal with a reason**, not a shrug: the caller turns it
/// straight into a `Refusal` the preview shows, and this is the only place
/// that maps a refused kind to its reason — `plan()` does not have a second
/// match to keep in step. There is deliberately no fallback drawer for it — a
/// ROM quietly landing in `Unsorted/` is a file on the wrong partition that
/// nobody was told about.
pub fn drawer_for<'a>(kind: &ItemKind, policy: &'a Policy) -> Result<&'a str, RefusalReason> {
    match kind {
        ItemKind::WhdloadArchive { .. } | ItemKind::WhdloadDrawer { .. } => Ok(&policy.games),
        ItemKind::FloppyImage => Ok(&policy.floppies),
        ItemKind::HardDiskImage => Ok(&policy.hard_disks),
        ItemKind::OpticalImage => Ok(&policy.discs),
        ItemKind::Archive | ItemKind::Unknown => Ok(&policy.unsorted),
        ItemKind::Rom => Err(RefusalReason::BelongsOnBootPartition),
        ItemKind::Commodore8Bit => Err(RefusalReason::NoPlaceOnAnAmigaVolume),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::layout::ItemKind;

    /// The shipped defaults, pinned. A card built from them is what the user
    /// gets without touching anything.
    #[test]
    fn the_default_policy_puts_each_kind_where_the_spec_says() {
        let policy = Policy::default();
        let cases = [
            (
                ItemKind::WhdloadArchive {
                    name: "Turrican".into(),
                },
                Ok("Games"),
            ),
            (
                ItemKind::WhdloadDrawer {
                    name: "Zool".into(),
                },
                Ok("Games"),
            ),
            (ItemKind::FloppyImage, Ok("Floppies")),
            (ItemKind::HardDiskImage, Ok("HardDisks")),
            (ItemKind::OpticalImage, Ok("CDs")),
            (ItemKind::Archive, Ok("Unsorted")),
            (ItemKind::Unknown, Ok("Unsorted")),
        ];
        for (kind, expected) in cases {
            assert_eq!(drawer_for(&kind, &policy), expected, "{kind:?}");
        }
    }

    /// **Two kinds are refused rather than placed**, each with its **own**
    /// reason — that mapping now lives only here, so this is the assertion
    /// that carries it: a ROM belongs on the FAT32 partition and a 1541 disk
    /// has no business on an Amiga volume at all. `core/card/intake.rs` gives
    /// both the same answer for a card.
    #[test]
    fn a_rom_and_a_commodore_disk_get_no_drawer() {
        let policy = Policy::default();
        assert_eq!(
            drawer_for(&ItemKind::Rom, &policy),
            Err(RefusalReason::BelongsOnBootPartition)
        );
        assert_eq!(
            drawer_for(&ItemKind::Commodore8Bit, &policy),
            Err(RefusalReason::NoPlaceOnAnAmigaVolume)
        );
    }

    /// A renamed drawer is used everywhere that kind lands.
    #[test]
    fn a_drawer_the_user_renamed_is_the_one_used() {
        let policy = Policy {
            games: "Oyunlar".into(),
            ..Policy::default()
        };
        assert_eq!(
            drawer_for(
                &ItemKind::WhdloadDrawer {
                    name: "Zool".into()
                },
                &policy
            ),
            Ok("Oyunlar")
        );
    }
}
