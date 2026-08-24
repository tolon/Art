//! Several complete systems on one card, and which one starts.
//!
//! SD-3 G16. *"Several complete AmigaOS environments on one card, chosen at
//! boot — 3.1 for compatibility, 3.2 for daily use, a games-only volume, a
//! recovery volume."*
//!
//! # There is no menu to write
//!
//! Worth saying first, because it is the thing a design would get wrong.
//! AmigaOS **already has** the boot menu: hold both mouse buttons at power-on
//! and the Early Startup screen lists every bootable partition to pick from.
//! ART writes no menu, no chooser and no loader. What ART decides is which one
//! starts when nobody holds anything, and that is one number per partition.
//!
//! Read 2026-08-24 rather than recalled: the Amiga RDB's `de_BootPri`, whose
//! *"suggested value is zero"* (ADCD 2.1, *RigidDiskBlock and Alternate
//! Filesystems*), and **higher boots first**.
//!
//! # What happens on a tie is not documented, so ART does not decide it
//!
//! Two bootable partitions at the same priority: nothing in the Amiga
//! documentation says which wins, and the answer would be whatever order the
//! filesystem's mount list happened to take. ART could pick one and be right
//! by accident. Instead it **names the pair** — a card whose two systems are
//! both "first" is a card whose owner does not know which one they are about
//! to boot, and finding out by booting it is exactly the sentence this project
//! does not make somebody chase.

use serde::Serialize;

use crate::core::rdb::PartitionSpec;

/// Something about which system starts that is worth saying first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "concern", rename_all = "kebab-case")]
pub enum BootConcern {
    /// Two or more bootable partitions claim the same priority.
    ///
    /// **Not resolved, named.** Higher boots first and a tie is undocumented,
    /// so ART says which partitions are tied rather than choosing between
    /// somebody's systems.
    TiedPriority {
        priority: i8,
        /// Every drive at that priority, in the order the card carries them.
        drive_names: Vec<String>,
    },
    /// A card with Amiga disks and nothing bootable on any of them.
    ///
    /// Legitimate — a card of pure data volumes is a real thing — but said
    /// out loud, because it is also what a mistyped `bootable` looks like.
    NothingBootable,
}

/// One Amiga disk's worth of partitions, as the card carries them.
///
/// Borrowed rather than owned so a caller can pass what it already has.
pub type Disk<'a> = &'a [PartitionSpec];

/// What to say about which system this card starts.
///
/// Takes every Amiga disk, because the tie that matters is **across** them:
/// two systems on two disks both at priority 1 is the ordinary way to build
/// this wrong, and a check that looked at one disk at a time would miss it
/// exactly there. The same shape `CardImage::file_systems` takes for the
/// neighbouring question ([ART-097](../../../../docs/ISSUES.md)).
pub fn boot_concerns(disks: &[Disk<'_>]) -> Vec<BootConcern> {
    let bootable: Vec<&PartitionSpec> = disks
        .iter()
        .flat_map(|disk| disk.iter())
        .filter(|partition| partition.bootable)
        .collect();

    if bootable.is_empty() {
        // Only worth saying when there was something to boot from.
        return if disks.iter().any(|disk| !disk.is_empty()) {
            vec![BootConcern::NothingBootable]
        } else {
            Vec::new()
        };
    }

    // Grouped by priority, in the order the card carries them — a report whose
    // rows move between two builds of the same card is one nobody can check.
    let mut priorities: Vec<i8> = Vec::new();
    for partition in &bootable {
        if !priorities.contains(&partition.boot_priority) {
            priorities.push(partition.boot_priority);
        }
    }

    priorities
        .into_iter()
        .filter_map(|priority| {
            let names: Vec<String> = bootable
                .iter()
                .filter(|partition| partition.boot_priority == priority)
                .map(|partition| partition.drive_name.clone())
                .collect();
            (names.len() > 1).then_some(BootConcern::TiedPriority {
                priority,
                drive_names: names,
            })
        })
        .collect()
}

/// Which partition an Amiga would start, when that can be said at all.
///
/// `None` when nothing is bootable **or** when the highest priority is tied —
/// the second is the point: answering "SDH0" about a tie ART cannot resolve
/// would be the confident wrong sentence, and every caller has
/// [`boot_concerns`] to say why instead.
pub fn starts_with<'a>(disks: &[Disk<'a>]) -> Option<&'a PartitionSpec> {
    let bootable: Vec<&'a PartitionSpec> = disks
        .iter()
        .flat_map(|disk| disk.iter())
        .filter(|partition| partition.bootable)
        .collect();

    let highest = bootable.iter().map(|p| p.boot_priority).max()?;
    let mut at_top = bootable
        .iter()
        .filter(|p| p.boot_priority == highest)
        .copied();
    let first = at_top.next()?;
    // A second partition at the top is a tie, and a tie has no answer.
    at_top.next().is_none().then_some(first)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::rdb::AmigaHardDiskFs;

    fn part(name: &str, bootable: bool, priority: i8) -> PartitionSpec {
        PartitionSpec {
            drive_name: name.into(),
            fs_type: AmigaHardDiskFs::Pfs3DirectScsi,
            size_mb: 512,
            bootable,
            boot_priority: priority,
            num_buffers: 0,
        }
    }

    /// The ordinary multiboot card: 3.2 daily, 3.1 for compatibility, and a
    /// data volume that boots nothing.
    #[test]
    fn a_card_with_a_clear_order_says_nothing_and_names_the_one_that_starts() {
        let first = [part("SDH0", true, 2), part("SDH1", false, 0)];
        let second = [part("SDH2", true, 1)];
        let disks: Vec<Disk> = vec![&first, &second];

        assert!(boot_concerns(&disks).is_empty());
        assert_eq!(starts_with(&disks).unwrap().drive_name, "SDH0");
    }

    /// **The ordinary way to build this wrong**, and the reason the check
    /// takes every disk at once: two systems on two disks, both left at the
    /// priority a single-system card uses.
    #[test]
    fn two_systems_both_called_first_are_named_rather_than_resolved() {
        let first = [part("SDH0", true, 1)];
        let second = [part("SDH2", true, 1)];
        let disks: Vec<Disk> = vec![&first, &second];

        let concerns = boot_concerns(&disks);
        let [BootConcern::TiedPriority {
            priority,
            drive_names,
        }] = concerns.as_slice()
        else {
            panic!("{concerns:?}");
        };
        assert_eq!(*priority, 1);
        assert_eq!(drive_names, &["SDH0".to_string(), "SDH2".to_string()]);

        // And nothing claims to know which starts.
        assert!(
            starts_with(&disks).is_none(),
            "a tie has no answer, and inventing one is the sentence this avoids"
        );
    }

    /// A tie **below** the top does not change which system starts, so it is
    /// still worth saying — the owner asked for an order and did not get one —
    /// but the card's answer is not in doubt.
    #[test]
    fn a_tie_below_the_top_is_said_and_does_not_hide_the_winner() {
        let first = [part("SDH0", true, 5), part("SDH1", true, 1)];
        let second = [part("SDH2", true, 1)];
        let disks: Vec<Disk> = vec![&first, &second];

        assert_eq!(boot_concerns(&disks).len(), 1);
        assert_eq!(starts_with(&disks).unwrap().drive_name, "SDH0");
    }

    /// A partition that is not bootable cannot tie with one that is.
    #[test]
    fn a_data_volume_is_not_in_the_running() {
        let first = [part("SDH0", true, 1), part("SDH1", false, 1)];
        let disks: Vec<Disk> = vec![&first];
        assert!(boot_concerns(&disks).is_empty());
        assert_eq!(starts_with(&disks).unwrap().drive_name, "SDH0");
    }

    /// Legitimate — a card of pure data volumes is a real thing — and said
    /// anyway, because it is also what a mistyped `bootable` looks like.
    #[test]
    fn a_card_that_boots_nothing_is_said_out_loud() {
        let first = [part("SDH0", false, 0)];
        let disks: Vec<Disk> = vec![&first];
        assert_eq!(boot_concerns(&disks), vec![BootConcern::NothingBootable]);
        assert!(starts_with(&disks).is_none());
    }

    /// An empty card is not a card that boots nothing; it is a card with
    /// nothing on it, and the two are different sentences.
    #[test]
    fn no_partitions_at_all_says_nothing() {
        assert!(boot_concerns(&[]).is_empty());
        let empty: [PartitionSpec; 0] = [];
        assert!(boot_concerns(&[&empty as Disk]).is_empty());
    }

    /// Negative priorities are legal — `de_BootPri`'s field is signed, and
    /// below zero is how a partition says "mount me but never boot me first".
    #[test]
    fn a_negative_priority_is_a_priority() {
        let first = [part("SDH0", true, -5), part("SDH1", true, -10)];
        let disks: Vec<Disk> = vec![&first];
        assert!(boot_concerns(&disks).is_empty());
        assert_eq!(starts_with(&disks).unwrap().drive_name, "SDH0");
    }

    /// Two separate ties are two separate sentences, in the order the card
    /// carries them — a report whose rows move between two builds of the same
    /// card is one nobody can check.
    #[test]
    fn two_ties_are_two_answers_in_a_stable_order() {
        let first = [part("SDH0", true, 2), part("SDH1", true, 1)];
        let second = [part("SDH2", true, 2), part("SDH3", true, 1)];
        let disks: Vec<Disk> = vec![&first, &second];

        let concerns = boot_concerns(&disks);
        assert_eq!(concerns.len(), 2);
        assert!(matches!(
            &concerns[0],
            BootConcern::TiedPriority { priority: 2, .. }
        ));
        assert!(matches!(
            &concerns[1],
            BootConcern::TiedPriority { priority: 1, .. }
        ));
    }

    /// Three at one priority is one sentence naming three, not three
    /// sentences naming pairs.
    #[test]
    fn three_at_one_priority_is_one_sentence() {
        let first = [
            part("SDH0", true, 1),
            part("SDH1", true, 1),
            part("SDH2", true, 1),
        ];
        let concerns = boot_concerns(&[&first as Disk]);
        assert_eq!(concerns.len(), 1);
        let [BootConcern::TiedPriority { drive_names, .. }] = concerns.as_slice() else {
            panic!("{concerns:?}");
        };
        assert_eq!(drive_names.len(), 3);
    }
}
