//! A volume table proposed for a card, scaled to the card in front of you.
//!
//! SD-5 G13's other half. The gap analysis calls it *"pick a profile, see the
//! proposed volume table scaled to the actual card size, adjust, go"*, and the
//! card builder already has the *adjust* and the *go* — what was missing is
//! anything that proposes.
//!
//! # Every number here was measured off a real card
//!
//! Not chosen, and not scaled by a rule somebody invented. Both of the cards
//! ART's card model was built from were read again on 2026-08-24 with ART's own
//! reader (`read_real_card_when_asked`), and they agree:
//!
//! | | CaffeineOS 9317 (64 GB) | MultibootOS 2.2 (128 GB) |
//! |---|---|---|
//! | boot partition ends at | 1 178 599 424 | **1 178 599 424** |
//! | system partition | 534 cyl x 1.5 MiB = **801 MiB** | 200 cyl x 4 MiB = **800 MiB** |
//! | filesystem | PFS3 (`PDS\3`) throughout | PFS3 throughout |
//! | `num_buffers` | 600 | 600 |
//!
//! The interesting one is the second row: **the system partition does not grow
//! with the card.** A 128 GB card gets the same ~800 MiB system volume as a
//! 64 GB one, and the difference goes to work space. A planner that scaled
//! everything proportionally would have got that wrong, and would have looked
//! reasonable doing it.
//!
//! # What it adds over a form with defaults
//!
//! One thing, and it is the reason this is worth building rather than leaving
//! the fields at their defaults: **it will not propose a partition that
//! corrupts the drive.** [`super::capacity`] warns when an FFS partition runs
//! past what a pre-v46 Kickstart can address - which does not fail, it wraps
//! and writes over the start of the volume. Here there is nothing to warn
//! about, because the proposal splits the work space into pieces that fit
//! instead. Preventing beats reporting, when the alternative is a corrupted
//! card and a sentence nobody read.

use serde::Serialize;

use super::capacity::{FIRST_LARGE_AWARE_ROM_MAJOR, KICKSTART_FFS_LIMIT};
use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec, MAX_PARTITIONS};

/// The FAT32 boot partition, to the byte.
///
/// **The same on both real cards**, which is why it is a constant rather than
/// a fraction of the card. `core::mbr`'s own default already carries this
/// number; it is restated here against the two cards it came from.
pub const MEASURED_BOOT_BYTES: u64 = 1_178_599_424;

/// The system partition, measured off both real cards and **not scaled**.
pub const MEASURED_SYSTEM_MB: u32 = 800;

/// Buffers per partition, as both real cards set them (and ART-096's own
/// measured default).
pub const MEASURED_BUFFERS: u32 = 600;

/// The smallest work space worth proposing a table for.
///
/// Below this the card holds a system volume and almost nothing else, which is
/// a card somebody built by mistake rather than one ART should lay out.
const MIN_WORK_BYTES: u64 = 512 * 1024 * 1024;

/// Something about the proposal that the table itself does not say.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "note", rename_all = "kebab-case")]
pub enum ProposalNote {
    /// The work space is more than one partition, because one that size would
    /// run past what this Kickstart's FFS can address.
    ///
    /// **Not a warning.** The table being described has already avoided the
    /// hazard; this says why it looks the way it does, so nobody "simplifies"
    /// it back into one partition.
    SplitForKickstartFfs {
        pieces: usize,
        limit: u64,
        rom_major: Option<u16>,
    },
    /// PFS3 was asked for and it carries its own filesystem in the RDB, so the
    /// Kickstart's limit does not apply and the work space is one partition.
    OneWorkVolumeBecausePfs3,
    /// The card is larger than the table covers, because covering it would
    /// need more partitions than an RDB holds.
    ///
    /// Said out loud rather than silently rounded away - a user who bought a
    /// 256 GB card is owed the sentence about why 30 GB of it is unallocated.
    TailUnallocated { bytes: u64 },
    /// No Kickstart was chosen, so the FFS question could not be asked and the
    /// safe answer was taken.
    ///
    /// A different sentence from "your Kickstart is too old", which is
    /// [ART-232](../../../../docs/ISSUES.md)'s neighbouring lesson: the two
    /// send somebody to different places.
    SplitBecauseNoRomChosen,
}

/// Why no table could be proposed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "refusal", rename_all = "kebab-case")]
pub enum ProposalRefusal {
    /// After the boot partition and a system volume there is nothing left.
    CardTooSmall { need_bytes: u64, have_bytes: u64 },
}

/// A whole card, proposed.
#[derive(Debug, Clone, Serialize)]
pub struct ProposedTable {
    pub boot_bytes: u64,
    /// The first Amiga disk's partitions, in order.
    pub partitions: Vec<PartitionSpec>,
    pub notes: Vec<ProposalNote>,
}

/// Whether this filesystem leans on the Kickstart's own FFS.
///
/// PFS3 and SFS carry their driver in the RDB, so the ROM's addressing is not
/// their business - the same call [`super::capacity`] makes, and made in one
/// place rather than two.
fn uses_kickstarts_own(fs: AmigaHardDiskFs) -> bool {
    matches!(
        fs,
        AmigaHardDiskFs::FfsStandard | AmigaHardDiskFs::FfsDirCache
    )
}

fn rom_is_large_aware(rom_major: Option<u16>) -> bool {
    rom_major.is_some_and(|major| major >= FIRST_LARGE_AWARE_ROM_MAJOR)
}

/// Propose a table for this card.
///
/// `rom_major` is the Kickstart the card will carry, when one has been chosen.
/// `None` is not "assume the best": it takes the safe branch and says so.
pub fn propose(
    card_bytes: u64,
    fs: AmigaHardDiskFs,
    rom_major: Option<u16>,
) -> Result<ProposedTable, ProposalRefusal> {
    let system_bytes = u64::from(MEASURED_SYSTEM_MB) * 1024 * 1024;
    let need = MEASURED_BOOT_BYTES + system_bytes + MIN_WORK_BYTES;
    if card_bytes < need {
        return Err(ProposalRefusal::CardTooSmall {
            need_bytes: need,
            have_bytes: card_bytes,
        });
    }

    let work_bytes = card_bytes - MEASURED_BOOT_BYTES - system_bytes;
    let mut notes = Vec::new();

    let mut partitions = vec![PartitionSpec {
        drive_name: "SDH0".into(),
        fs_type: fs,
        size_mb: MEASURED_SYSTEM_MB,
        bootable: true,
        // The priority `defaultPartition` uses, and the one both real cards'
        // bootable partition carries.
        boot_priority: 1,
        num_buffers: MEASURED_BUFFERS,
    }];

    // Does the Kickstart's 4 GiB addressing apply to the work space?
    let capped = uses_kickstarts_own(fs) && !rom_is_large_aware(rom_major);

    if !capped {
        if !uses_kickstarts_own(fs) {
            notes.push(ProposalNote::OneWorkVolumeBecausePfs3);
        }
        partitions.push(work_partition("SDH1", fs, 0));
        return Ok(ProposedTable {
            boot_bytes: MEASURED_BOOT_BYTES,
            partitions,
            notes,
        });
    }

    // Split. `div_ceil` rather than a `+ limit - 1` dance, and the last piece
    // takes what is left rather than being sized - so the arithmetic here can
    // never disagree with `create_rdb_layout`'s cylinder rounding.
    let wanted = work_bytes.div_ceil(KICKSTART_FFS_LIMIT) as usize;
    let room = MAX_PARTITIONS - partitions.len();
    let pieces = wanted.min(room);

    for index in 0..pieces {
        let last = index + 1 == pieces;
        // Every piece but the last is sized at the limit; the last says `0`
        // and takes the remainder. When the tail had to be dropped, the last
        // piece is sized too, so nothing runs past the limit.
        let size_mb = if last && pieces == wanted {
            0
        } else {
            (KICKSTART_FFS_LIMIT / (1024 * 1024)) as u32
        };
        partitions.push(work_partition(&format!("SDH{}", index + 1), fs, size_mb));
    }

    notes.push(if rom_major.is_none() {
        ProposalNote::SplitBecauseNoRomChosen
    } else {
        ProposalNote::SplitForKickstartFfs {
            pieces,
            limit: KICKSTART_FFS_LIMIT,
            rom_major,
        }
    });

    if pieces < wanted {
        let covered = KICKSTART_FFS_LIMIT * pieces as u64;
        notes.push(ProposalNote::TailUnallocated {
            bytes: work_bytes - covered,
        });
    }

    Ok(ProposedTable {
        boot_bytes: MEASURED_BOOT_BYTES,
        partitions,
        notes,
    })
}

fn work_partition(name: &str, fs: AmigaHardDiskFs, size_mb: u32) -> PartitionSpec {
    PartitionSpec {
        drive_name: name.into(),
        fs_type: fs,
        size_mb,
        bootable: false,
        boot_priority: 0,
        num_buffers: MEASURED_BUFFERS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::card::capacity::{approximate_sizes, size_concerns};

    const GB: u64 = 1024 * 1024 * 1024;

    /// The card ART's own model was measured from, proposed for.
    #[test]
    fn a_64gb_card_with_pfs3_gets_the_shape_both_real_cards_have() {
        let table = propose(64 * GB, AmigaHardDiskFs::Pfs3DirectScsi, Some(40)).unwrap();

        assert_eq!(table.boot_bytes, 1_178_599_424);
        assert_eq!(table.partitions.len(), 2);
        assert_eq!(table.partitions[0].drive_name, "SDH0");
        assert_eq!(table.partitions[0].size_mb, 800);
        assert!(table.partitions[0].bootable);
        assert_eq!(
            table.partitions[1].size_mb, 0,
            "the work volume takes the rest"
        );
        assert!(!table.partitions[1].bootable);
        assert_eq!(table.notes, vec![ProposalNote::OneWorkVolumeBecausePfs3]);
    }

    /// **The system volume does not grow with the card**, which is the
    /// measured fact a proportional planner would have got wrong.
    #[test]
    fn a_128gb_card_gets_the_same_system_volume_as_a_64gb_one() {
        let small = propose(64 * GB, AmigaHardDiskFs::Pfs3DirectScsi, Some(47)).unwrap();
        let large = propose(128 * GB, AmigaHardDiskFs::Pfs3DirectScsi, Some(47)).unwrap();
        assert_eq!(small.partitions[0].size_mb, large.partitions[0].size_mb);
        assert_eq!(large.partitions[0].size_mb, 800);
    }

    /// **The point of the whole module.** FFS on a Kickstart that cannot
    /// address past 4 GiB: the proposal splits rather than offering one
    /// partition that would write over the start of the volume.
    #[test]
    fn ffs_on_an_old_kickstart_is_split_rather_than_left_to_corrupt() {
        let table = propose(64 * GB, AmigaHardDiskFs::FfsStandard, Some(40)).unwrap();

        // The boot partition is asserted **on this path too**. Mutation found
        // it unasserted here: only the early return had a test looking at it,
        // so a `boot_bytes` computed from the card size survived on the split
        // branch - and the split branch is the one this module exists for.
        assert_eq!(table.boot_bytes, MEASURED_BOOT_BYTES);

        // 64 GB - 1.098 GiB boot - 800 MiB system, over 4 GiB pieces.
        assert!(table.partitions.len() > 2, "{:?}", table.partitions);
        assert!(table
            .notes
            .iter()
            .any(|n| matches!(n, ProposalNote::SplitForKickstartFfs { .. })));

        // And the proof it worked, asked of the module that judges it rather
        // than of this one's own arithmetic: no partition in the proposed
        // table draws a concern.
        let area = 64 * GB - 1_178_599_424;
        let sizes = approximate_sizes(area, &table.partitions);
        assert_eq!(
            size_concerns(&table.partitions, &sizes, Some(40)),
            vec![],
            "a proposal that still trips capacity.rs has proposed the hazard"
        );
    }

    /// The same card, a Kickstart that can address it: one work volume.
    #[test]
    fn ffs_on_a_v46_kickstart_is_not_split() {
        let table = propose(64 * GB, AmigaHardDiskFs::FfsStandard, Some(47)).unwrap();
        assert_eq!(table.partitions.len(), 2);
        assert!(table.notes.is_empty(), "{:?}", table.notes);
    }

    /// **"No Kickstart chosen" is its own sentence.** The table is the same
    /// safe one, and the reason it is safe is different — and sends somebody
    /// somewhere different.
    #[test]
    fn no_kickstart_takes_the_safe_branch_and_says_which_one_it_took() {
        let table = propose(64 * GB, AmigaHardDiskFs::FfsStandard, None).unwrap();
        assert!(table.partitions.len() > 2);
        assert!(table.notes.contains(&ProposalNote::SplitBecauseNoRomChosen));
        assert!(
            !table
                .notes
                .iter()
                .any(|n| matches!(n, ProposalNote::SplitForKickstartFfs { .. })),
            "an unchosen ROM must not be reported as an old one"
        );
    }

    /// An RDB holds 32 partitions. A card too large to cover in 31 work
    /// volumes leaves a tail, and **says so** rather than rounding it away.
    #[test]
    fn a_card_too_large_to_cover_leaves_a_named_tail() {
        let table = propose(512 * GB, AmigaHardDiskFs::FfsStandard, Some(40)).unwrap();

        assert_eq!(table.partitions.len(), MAX_PARTITIONS);
        let tail = table
            .notes
            .iter()
            .find_map(|n| match n {
                ProposalNote::TailUnallocated { bytes } => Some(*bytes),
                _ => None,
            })
            .expect("a tail this size must be named");
        assert!(tail > 0);

        // And nothing in the table runs past the limit even at the edge - the
        // last piece is sized rather than taking "the rest", which would have
        // handed it the whole tail.
        let area = 512 * GB - 1_178_599_424;
        let sizes = approximate_sizes(area, &table.partitions);
        assert_eq!(size_concerns(&table.partitions, &sizes, Some(40)), vec![]);
    }

    /// A card with room for the system volume and nothing to work in is
    /// refused by name, with both numbers.
    #[test]
    fn a_card_too_small_is_refused_with_both_numbers() {
        let refusal = propose(2 * GB, AmigaHardDiskFs::Pfs3DirectScsi, Some(47)).unwrap_err();
        let ProposalRefusal::CardTooSmall {
            need_bytes,
            have_bytes,
        } = refusal;
        assert_eq!(have_bytes, 2 * GB);
        assert!(need_bytes > have_bytes);
    }

    /// Every partition carries the buffers both real cards carry.
    #[test]
    fn every_partition_gets_the_measured_buffer_count() {
        let table = propose(64 * GB, AmigaHardDiskFs::FfsStandard, Some(40)).unwrap();
        assert!(table.partitions.iter().all(|p| p.num_buffers == 600));
    }

    /// Exactly one bootable partition, and it is the first.
    #[test]
    fn only_the_system_volume_boots() {
        let table = propose(64 * GB, AmigaHardDiskFs::FfsStandard, Some(40)).unwrap();
        let bootable: Vec<&PartitionSpec> =
            table.partitions.iter().filter(|p| p.bootable).collect();
        assert_eq!(bootable.len(), 1);
        assert_eq!(bootable[0].drive_name, "SDH0");
    }

    /// Drive names are unique and in order - two volumes answering to one name
    /// on the same disk is a card whose owner cannot say what they copied to.
    #[test]
    fn the_drive_names_are_unique_and_in_order() {
        let table = propose(64 * GB, AmigaHardDiskFs::FfsStandard, Some(40)).unwrap();
        let names: Vec<&str> = table
            .partitions
            .iter()
            .map(|p| p.drive_name.as_str())
            .collect();
        let unique: std::collections::HashSet<&&str> = names.iter().collect();
        assert_eq!(unique.len(), names.len(), "{names:?}");
        assert_eq!(names[0], "SDH0");
        assert_eq!(names[1], "SDH1");
    }
}
