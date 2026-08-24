//! How big a partition may be, and who says so.
//!
//! SD-5 G13's safety half. The planner half of that item is comfort; this is
//! not: **ART will currently build a 20 GB FFS partition, and on a Kickstart
//! 3.1 machine that corrupts the whole disk.** Nothing in ART said so until
//! now — `core::rdb` knows a card may hold at most 32 partitions and nothing
//! about how large one may be.
//!
//! # The limit is not FFS's; it is the FFS *on the machine*
//!
//! Read 2026-08-24 rather than recalled, because the difference is the whole
//! design. Sources: Wikipedia's *Amiga Fast File System* (the version table),
//! classicamiga's *Installing a large Harddrive (4GB or larger)*, and the
//! AmigaOS documentation wiki on the **TrackDisk64** and **NSD** standards.
//!
//! | Filesystem | Where it comes from | Limit |
//! |---|---|---|
//! | Original FFS | Kickstart 3.1's ROM | **~4 GB**, and *"attempting to use FFS partitions beyond this limit caused serious data corruption all through the drive"* |
//! | FFS v45 | AmigaOS 3.5 / 3.9, on disk | NSD 64-bit — lifted |
//! | FFS v46+ | AmigaOS 3.1.4 / 3.2, in ROM | TD_64 **and** NSD natively — lifted |
//! | PFS3, SFS | a driver in the RDB | not this limit's business |
//!
//! So "is this partition too big?" cannot be answered from the partition. It
//! needs to know **which Kickstart the card boots**, and ART already does: the
//! build carries one ROM ([ART-223](../../../../docs/ISSUES.md)) and
//! `core::rom` reads its major.
//!
//! # It warns; it does not refuse
//!
//! A refusal would be wrong here, and knowing why matters. ART cannot see
//! everything that lifts the limit: a 3.9 system loads FFS v45 from disk, and
//! `SetPatch` replaces the ROM's filesystem at boot. So a >4 GB FFS partition
//! on a Kickstart 3.1 card is **usually** wrong and **not always**, and the
//! honest output is a sentence naming the condition rather than a verdict ART
//! cannot stand behind.
//!
//! That is the same shape `partitions_missing_driver` already takes for the
//! neighbouring question, and for the same reason.

use serde::Serialize;

use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

/// What Kickstart's own FFS can address.
///
/// 4 GiB. Beyond it the original FFS corrupts the drive rather than refusing,
/// which is why this is a warning ART raises rather than an error a machine
/// would give.
pub const KICKSTART_FFS_LIMIT: u64 = 4 * 1024 * 1024 * 1024;

/// The first Kickstart whose ROM filesystem addresses past [`KICKSTART_FFS_LIMIT`].
///
/// FFS **v46** ships in AmigaOS 3.1.4 and 3.2 and *"natively supports the APIs
/// for TD_64, NSD, and/or the classic 32-bit TD_ calls"*. 3.1.4's Kickstart is
/// major 46 and 3.2's is 47; 3.1's is 40.
pub const FIRST_LARGE_AWARE_ROM_MAJOR: u16 = 46;

/// Something about a partition's size worth saying before the card is written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "concern", rename_all = "kebab-case")]
pub enum SizeConcern {
    /// An FFS partition larger than Kickstart's own filesystem can address,
    /// on a card whose ROM is older than [`FIRST_LARGE_AWARE_ROM_MAJOR`] or
    /// unknown.
    ///
    /// **The dangerous one.** The original FFS does not refuse; it writes past
    /// its own addressing and takes the rest of the disk with it.
    BeyondKickstartFfs {
        drive_name: String,
        bytes: u64,
        limit: u64,
        /// `None` when no ROM has been chosen — which is *not* the same as an
        /// old one, and the sentence says so.
        rom_major: Option<u16>,
    },
}

/// Does this filesystem rely on whatever Kickstart provides?
///
/// `DOS\0`…`DOS\7` are in every Kickstart, which is what makes them free of a
/// driver **and** subject to that Kickstart's own limits. PFS3 and SFS carry
/// their own and are not this function's business.
fn uses_kickstarts_own(fs: AmigaHardDiskFs) -> bool {
    matches!(
        fs,
        AmigaHardDiskFs::FfsStandard | AmigaHardDiskFs::FfsDirCache
    )
}

/// Whether a ROM addresses beyond the 4 GiB line.
///
/// `None` — no ROM chosen — counts as **not** knowing, and the caller's
/// sentence must say "no Kickstart chosen" rather than "an old Kickstart".
fn rom_is_large_aware(rom_major: Option<u16>) -> bool {
    rom_major.is_some_and(|major| major >= FIRST_LARGE_AWARE_ROM_MAJOR)
}

/// Roughly how large each partition will be, for warning purposes only.
///
/// **Deliberately not the real arithmetic.** `create_rdb_layout` rounds to
/// whole cylinders and is the one place that may, because — as
/// `PartitionSpec::size_mb`'s own doc says — *"a screen recomputing
/// `bytes_per_cyl` is a second copy of the rounding that lives here, and a
/// second copy is how the two start disagreeing."*
///
/// This is allowed to be approximate because of what it is for: a few
/// megabytes of cylinder rounding cannot move a partition across the 4 GiB
/// line, and nothing here decides a layout. A `size_mb` of `0` means *"whatever
/// is left"*, which only the last partition may say.
pub fn approximate_sizes(area_bytes: u64, partitions: &[PartitionSpec]) -> Vec<u64> {
    let named: u64 = partitions
        .iter()
        .map(|p| u64::from(p.size_mb) * 1024 * 1024)
        .sum();
    let remainder = area_bytes.saturating_sub(named);
    partitions
        .iter()
        .map(|p| {
            if p.size_mb == 0 {
                remainder
            } else {
                u64::from(p.size_mb) * 1024 * 1024
            }
        })
        .collect()
}

/// Everything worth saying about these partitions' sizes.
///
/// `sizes` is each partition's real size in bytes, in the same order — the
/// caller works those out, because a `PartitionSpec` may say `0` for *"whatever
/// is left"* and only the layout knows what that came to.
pub fn size_concerns(
    partitions: &[PartitionSpec],
    sizes: &[u64],
    rom_major: Option<u16>,
) -> Vec<SizeConcern> {
    if rom_is_large_aware(rom_major) {
        return Vec::new();
    }
    partitions
        .iter()
        .zip(sizes)
        .filter(|(partition, bytes)| {
            uses_kickstarts_own(partition.fs_type) && **bytes > KICKSTART_FFS_LIMIT
        })
        .map(|(partition, bytes)| SizeConcern::BeyondKickstartFfs {
            drive_name: partition.drive_name.clone(),
            bytes: *bytes,
            limit: KICKSTART_FFS_LIMIT,
            rom_major,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(name: &str, fs: AmigaHardDiskFs) -> PartitionSpec {
        PartitionSpec {
            drive_name: name.into(),
            fs_type: fs,
            size_mb: 0,
            bootable: true,
            boot_priority: 0,
            num_buffers: 0,
        }
    }

    const OVER: u64 = 20 * 1024 * 1024 * 1024;
    const UNDER: u64 = 2 * 1024 * 1024 * 1024;

    /// **The live hazard.** ART will build this today and nothing says a word:
    /// the original FFS does not refuse a partition past its addressing, it
    /// corrupts the drive.
    #[test]
    fn a_large_ffs_partition_on_a_kickstart_31_card_is_named() {
        let concerns = size_concerns(
            &[part("SDH0", AmigaHardDiskFs::FfsStandard)],
            &[OVER],
            Some(40),
        );
        let [SizeConcern::BeyondKickstartFfs {
            drive_name,
            bytes,
            limit,
            rom_major,
        }] = concerns.as_slice()
        else {
            panic!("{concerns:?}");
        };
        assert_eq!(drive_name, "SDH0");
        assert_eq!(*bytes, OVER);
        assert_eq!(*limit, KICKSTART_FFS_LIMIT);
        assert_eq!(*rom_major, Some(40));
    }

    /// AmigaOS 3.1.4 and 3.2 carry FFS v46, which addresses TD_64 and NSD
    /// natively. Warning there would be ART crying wolf about the machine most
    /// of these cards are built for.
    #[test]
    fn a_kickstart_that_knows_about_large_media_gets_no_warning() {
        for major in [FIRST_LARGE_AWARE_ROM_MAJOR, 47] {
            assert!(size_concerns(
                &[part("SDH0", AmigaHardDiskFs::FfsStandard)],
                &[OVER],
                Some(major)
            )
            .is_empty());
        }
    }

    /// **No ROM chosen is not an old ROM**, and the concern carries `None` so
    /// the sentence can say which of the two it is. Warning is still right:
    /// ART does not know, and the failure is silent corruption.
    #[test]
    fn no_rom_chosen_still_warns_and_says_it_does_not_know() {
        let concerns = size_concerns(&[part("SDH0", AmigaHardDiskFs::FfsStandard)], &[OVER], None);
        assert!(matches!(
            concerns.as_slice(),
            [SizeConcern::BeyondKickstartFfs {
                rom_major: None,
                ..
            }]
        ));
    }

    #[test]
    fn a_partition_inside_the_limit_says_nothing() {
        assert!(size_concerns(
            &[part("SDH0", AmigaHardDiskFs::FfsStandard)],
            &[UNDER],
            Some(40)
        )
        .is_empty());
    }

    /// Exactly at the line is inside it: the limit is what FFS *can* address.
    #[test]
    fn the_boundary_is_inclusive() {
        assert!(size_concerns(
            &[part("SDH0", AmigaHardDiskFs::FfsStandard)],
            &[KICKSTART_FFS_LIMIT],
            Some(40)
        )
        .is_empty());
        assert_eq!(
            size_concerns(
                &[part("SDH0", AmigaHardDiskFs::FfsStandard)],
                &[KICKSTART_FFS_LIMIT + 1],
                Some(40)
            )
            .len(),
            1
        );
    }

    /// **PFS3 and SFS carry their own filesystem in the RDB**, so Kickstart's
    /// limit is not theirs — and warning about them would push somebody off
    /// the filesystem the PiStorm card path actually uses.
    #[test]
    fn a_filesystem_that_carries_its_own_driver_is_not_kickstarts_business() {
        for fs in [
            AmigaHardDiskFs::Pfs3DirectScsi,
            AmigaHardDiskFs::Pfs3Standard,
            AmigaHardDiskFs::Sfs0,
        ] {
            assert!(
                size_concerns(&[part("SDH0", fs)], &[OVER], Some(40)).is_empty(),
                "{fs:?} brings its own filesystem"
            );
        }
    }

    /// The directory-cache flavour is still Kickstart's own — `DOS\3` is in
    /// the ROM exactly as `DOS\1` is.
    #[test]
    fn the_dircache_flavour_is_kickstarts_too() {
        assert_eq!(
            size_concerns(
                &[part("SDH0", AmigaHardDiskFs::FfsDirCache)],
                &[OVER],
                Some(40)
            )
            .len(),
            1
        );
    }

    /// One concern per partition, and the one inside the limit is not named.
    #[test]
    fn several_partitions_are_answered_one_by_one() {
        let concerns = size_concerns(
            &[
                part("SDH0", AmigaHardDiskFs::FfsStandard),
                part("SDH1", AmigaHardDiskFs::FfsStandard),
                part("SDH2", AmigaHardDiskFs::Pfs3DirectScsi),
            ],
            &[UNDER, OVER, OVER],
            Some(40),
        );
        assert_eq!(concerns.len(), 1);
        assert!(matches!(
            &concerns[0],
            SizeConcern::BeyondKickstartFfs { drive_name, .. } if drive_name == "SDH1"
        ));
    }

    /// A caller that hands in fewer sizes than partitions gets answers for the
    /// ones it described, not a panic — `zip` stops at the shorter, and this
    /// says so on purpose rather than leaving it to be discovered.
    #[test]
    fn a_short_list_of_sizes_is_not_a_panic() {
        let concerns = size_concerns(
            &[
                part("SDH0", AmigaHardDiskFs::FfsStandard),
                part("SDH1", AmigaHardDiskFs::FfsStandard),
            ],
            &[OVER],
            Some(40),
        );
        assert_eq!(concerns.len(), 1);
    }

    /// A `size_mb` of `0` takes what the named ones leave — the idiom both
    /// real PiStorm cards use for their second partition.
    #[test]
    fn the_last_partition_gets_what_is_left() {
        let sizes = approximate_sizes(
            10 * 1024 * 1024 * 1024,
            &[
                PartitionSpec {
                    size_mb: 2048,
                    ..part("SDH0", AmigaHardDiskFs::FfsStandard)
                },
                part("SDH1", AmigaHardDiskFs::FfsStandard),
            ],
        );
        assert_eq!(sizes[0], 2 * 1024 * 1024 * 1024);
        assert_eq!(sizes[1], 8 * 1024 * 1024 * 1024);
    }

    /// **The case this whole module is for**, end to end: a 32 GB card, a 2 GB
    /// system partition and the rest as work, all FFS, on a Kickstart 3.1.
    #[test]
    fn a_thirty_two_gigabyte_card_warns_about_its_work_partition() {
        let partitions = [
            PartitionSpec {
                size_mb: 2048,
                ..part("SDH0", AmigaHardDiskFs::FfsStandard)
            },
            part("SDH1", AmigaHardDiskFs::FfsStandard),
        ];
        let sizes = approximate_sizes(32 * 1024 * 1024 * 1024, &partitions);
        let concerns = size_concerns(&partitions, &sizes, Some(40));
        assert_eq!(concerns.len(), 1, "only the big one: {concerns:?}");
        assert!(matches!(
            &concerns[0],
            SizeConcern::BeyondKickstartFfs { drive_name, .. } if drive_name == "SDH1"
        ));
    }

    /// Asking for more than the area holds gives zero rather than an
    /// underflow — the layout refuses that case by name, and this must not
    /// panic on the way to being told so.
    #[test]
    fn asking_for_more_than_there_is_does_not_underflow() {
        let sizes = approximate_sizes(
            1024 * 1024 * 1024,
            &[
                PartitionSpec {
                    size_mb: 4096,
                    ..part("SDH0", AmigaHardDiskFs::FfsStandard)
                },
                part("SDH1", AmigaHardDiskFs::FfsStandard),
            ],
        );
        assert_eq!(sizes[1], 0);
    }

    #[test]
    fn nothing_at_all_is_no_concerns() {
        assert!(size_concerns(&[], &[], Some(40)).is_empty());
    }
}
