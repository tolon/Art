//! Editing a card's RDB in place: the I/O half of ART-117.
//!
//! `core::rdbedit` decides what to write; this module reads the range, writes
//! it through an undo journal in four synced stages, reads it back and keeps
//! the change only when every block is what was planned (spec decisions 3, 8,
//! 9). Nothing here decides a block number.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::core::card::read_card;
use crate::core::error::{CoreError, CoreResult};
use crate::core::rdb::{dos_type_string, BLOCK_SIZE};
use crate::core::rdbedit::{block_at, walk_strict, EditPlan, Stage, EDIT_WINDOW_BYTES};
use crate::core::volume::device::FileRegion;
use crate::core::volume::journal::{journal_path_for, Journalled};
use crate::core::volume::{read_block_vec, BlockDeviceMut};

/// Up to 8 MiB of the Amiga disk at `area_offset` — the window decision 5
/// walks, plans from and backs up.
pub fn read_range(image: &Path, area_offset: u64) -> CoreResult<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(image)?;
    let available = file
        .metadata()?
        .len()
        .saturating_sub(area_offset)
        .min(EDIT_WINDOW_BYTES as u64) as usize;
    let mut bytes = vec![0u8; available];
    file.seek(SeekFrom::Start(area_offset))?;
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}

/// How a journalled write ended when it did not succeed — kept apart so the
/// caller can say which (decision 10).
#[derive(Debug)]
pub(crate) enum WriteFailure {
    /// Nothing reached the card.
    BeforeFirstWrite(CoreError),
    /// Written, failed or not verified, and undone: the card is as it was.
    RolledBack(String),
    /// Written, and the journal is still beside the image.
    RollbackFailed { detail: String, journal: PathBuf },
}

/// S1–S4, a sync after each, and `after` told each finished stage. No
/// rollback here — the crash-point tests stop in `after` and walk away.
pub(crate) fn write_stages(
    journal: &mut Journalled<'_>,
    plan: &EditPlan,
    after: &mut dyn FnMut(Stage) -> CoreResult<()>,
) -> CoreResult<()> {
    for (stage, writes) in &plan.stages {
        for write in writes {
            journal.write_block(write.block, &write.bytes)?;
        }
        journal.sync()?;
        after(*stage)?;
    }
    Ok(())
}

fn not_verified(detail: impl Into<String>) -> CoreError {
    CoreError::Malformed {
        format: "RDB edit".into(),
        detail: detail.into(),
    }
}

/// Decision 9, over a fresh read-only handle.
pub(crate) fn verify(
    image: &Path,
    area_offset: u64,
    before: &[u8],
    plan: &EditPlan,
    driver: &[u8],
) -> CoreResult<()> {
    let hi = plan.allocation.rdb_blocks_hi;
    let length = u64::from(hi + 1) * BLOCK_SIZE as u64;
    let region = FileRegion::open(image, area_offset, length, BLOCK_SIZE)?;
    let mut after = Vec::with_capacity(length as usize);
    for block in 0..=hi {
        after.extend_from_slice(&read_block_vec(&region, block)?);
    }

    let mut expected: BTreeMap<u32, &[u8]> = BTreeMap::new();
    for (_, writes) in &plan.stages {
        for write in writes {
            expected.insert(write.block, &write.bytes);
        }
    }
    for block in 0..=hi {
        let now = block_at(&after, block);
        match expected.get(&block) {
            Some(planned) if now == Some(*planned) => {}
            Some(_) => {
                return Err(not_verified(format!(
                    "block {block} does not hold what ART wrote"
                )))
            }
            None if now == block_at(before, block) => {}
            None => {
                return Err(not_verified(format!(
                    "block {block} changed, and ART did not write it"
                )))
            }
        }
    }

    let walked = walk_strict(&after)
        .map_err(|refusal| not_verified(format!("the edited RDB no longer walks: {refusal}")))?;
    let label = dos_type_string(plan.dos_type);
    let on_chain: Vec<_> = walked
        .fshds
        .iter()
        .filter(|f| f.dos_type == plan.dos_type)
        .collect();
    let as_planned = matches!(
        on_chain.as_slice(),
        [one] if one.block == plan.allocation.fshd_block
            && (one.version, one.revision) == plan.file_version
            && one.payload == driver
    );
    if !as_planned {
        return Err(not_verified(format!(
            "the {label} driver on the edited chain is not the one ART planned"
        )));
    }

    let card = read_card(image)?;
    if card
        .partitions_missing_driver()
        .iter()
        .any(|(_, part)| part.dostype == plan.dos_type)
    {
        return Err(not_verified(format!(
            "a {label} partition still has no driver after the edit"
        )));
    }
    Ok(())
}

/// Journal exactly the planned blocks, write the stages, verify, then commit
/// or roll back.
pub(crate) fn write_journalled(
    device: &mut dyn BlockDeviceMut,
    image: &Path,
    area_offset: u64,
    before: &[u8],
    plan: &EditPlan,
    driver: &[u8],
    description: &str,
) -> Result<(), WriteFailure> {
    let mut journal = Journalled::begin(device, image, area_offset, description, &plan.blocks())
        .map_err(WriteFailure::BeforeFirstWrite)?;
    let journal_path = journal_path_for(image);
    let written = write_stages(&mut journal, plan, &mut |_| Ok(()))
        .and_then(|()| verify(image, area_offset, before, plan, driver));
    match written {
        Ok(()) => journal
            .commit()
            .map_err(|err| WriteFailure::RollbackFailed {
                detail: format!(
                    "the edit was written and verified, but its journal could not be closed: {err}"
                ),
                journal: journal_path,
            }),
        Err(err) => match journal.roll_back() {
            Ok(()) => Err(WriteFailure::RolledBack(err.to_string())),
            Err(rollback) => Err(WriteFailure::RollbackFailed {
                detail: format!("{err}; undoing it failed too: {rollback}"),
                journal: journal_path,
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::core::rdbedit::fixtures::*;
    use crate::core::rdbedit::{block_at, plan_append, walk_strict, EditPlan, Stage};
    use crate::core::volume::device::FileRegionMut;
    use crate::core::volume::journal::find_journal;

    pub(super) fn region(image: &Path, offset: u64, plan: &EditPlan) -> FileRegionMut {
        let length = u64::from(plan.allocation.rdb_blocks_hi + 1) * BLOCK_SIZE as u64;
        FileRegionMut::open(image, offset, length, BLOCK_SIZE).unwrap()
    }

    fn append_plan(range: &[u8], version: &str) -> (EditPlan, Vec<u8>) {
        let driver = hunk_driver(62_604, version);
        let digits: Vec<u16> = version.split('.').map(|d| d.parse().unwrap()).collect();
        let plan = plan_append(
            range,
            &walk_strict(range).unwrap(),
            PDS3,
            (digits[0], digits[1]),
            &driver,
        )
        .unwrap();
        (plan, driver)
    }

    #[test]
    fn an_append_on_the_caffeine_shape_writes_132_to_260_and_nothing_else() {
        let (_guard, dir) = crate::core::ScratchDir::pair("art-rdbedit", "append-caffeine");
        let range = caffeine_like(false);
        let image = write_image(&dir, &range, 16 * 1024 * 1024);
        let (plan, driver) = append_plan(&range, "19.3");

        let mut device = region(&image, 0, &plan);
        write_journalled(
            &mut device,
            &image,
            0,
            &range,
            &plan,
            &driver,
            "test append",
        )
        .unwrap_or_else(|failure| panic!("{failure:?}"));
        drop(device);

        let after = read_range(&image, 0).unwrap();
        for block in (0..16_384u32).filter(|b| *b != 0 && !(132..=260).contains(b)) {
            assert_eq!(
                block_at(&after, block),
                block_at(&range, block),
                "block {block}"
            );
        }
        let walked = walk_strict(&after).unwrap();
        assert_eq!(walked.fshds.len(), 1);
        assert_eq!(
            (
                walked.fshds[0].block,
                walked.fshds[0].version,
                walked.fshds[0].revision
            ),
            (132, 19, 3)
        );
        assert_eq!(walked.fshds[0].payload, driver);
        assert_eq!(walked.rdsk.high_rdsk_block, 260);
        assert!(
            find_journal(&image).unwrap().is_none(),
            "a committed edit leaves no journal"
        );
        assert!(crate::core::card::read_card(&image)
            .unwrap()
            .provides_file_system(PDS3));
    }

    /// Block numbers are area-relative: a card's MBR and FAT32 area are never touched.
    #[test]
    fn an_append_into_a_cards_amiga_area_stays_inside_that_area() {
        let (_guard, dir) = crate::core::ScratchDir::pair("art-rdbedit", "append-card");
        let range = caffeine_like(false);
        let image = write_card(&dir, &range);
        let before = std::fs::read(&image).unwrap();
        let offset = 8192 * 512;
        let (plan, driver) = append_plan(&range, "19.3");

        let mut device = region(&image, offset, &plan);
        write_journalled(
            &mut device,
            &image,
            offset,
            &range,
            &plan,
            &driver,
            "test append",
        )
        .unwrap_or_else(|failure| panic!("{failure:?}"));
        drop(device);

        let after = std::fs::read(&image).unwrap();
        assert_eq!(
            after[..offset as usize],
            before[..offset as usize],
            "MBR and FAT32 area"
        );
        let area = read_range(&image, offset).unwrap();
        assert_eq!(walk_strict(&area).unwrap().fshds[0].block, 132);
        assert_eq!(
            after[offset as usize + 261 * 512..],
            before[offset as usize + 261 * 512..]
        );
    }

    #[test]
    fn an_append_on_an_art_built_card_raises_rdb_blocks_hi_and_verifies() {
        let (_guard, dir) = crate::core::ScratchDir::pair("art-rdbedit", "append-art");
        let range = art_like(false);
        let image = write_image(&dir, &range, 64 * 1024 * 1024);
        let (plan, driver) = append_plan(&range, "19.3");

        let mut device = region(&image, 0, &plan);
        write_journalled(
            &mut device,
            &image,
            0,
            &range,
            &plan,
            &driver,
            "test append",
        )
        .unwrap_or_else(|failure| panic!("{failure:?}"));
        drop(device);

        let walked = walk_strict(&read_range(&image, 0).unwrap()).unwrap();
        assert_eq!(
            (walked.rdsk.rdb_blocks_hi, walked.rdsk.high_rdsk_block),
            (130, 130)
        );
        assert_eq!(walked.fshds[0].lseg_blocks, (3..=130).collect::<Vec<u32>>());
        assert_eq!(
            walked.first_partition_block(),
            Some(2016),
            "the partition did not move"
        );
    }

    /// Decision 9 is a gate, not a report: an edit that does not read back as
    /// planned is undone.
    #[test]
    fn a_write_that_does_not_verify_is_rolled_back_byte_for_byte() {
        let (_guard, dir) = crate::core::ScratchDir::pair("art-rdbedit", "append-unverified");
        let range = caffeine_like(false);
        let image = write_image(&dir, &range, 16 * 1024 * 1024);
        let before = std::fs::read(&image).unwrap();
        let (plan, _) = append_plan(&range, "19.3");
        let other_driver = hunk_driver(62_604, "19.4");

        let mut device = region(&image, 0, &plan);
        let failure = write_journalled(
            &mut device,
            &image,
            0,
            &range,
            &plan,
            &other_driver,
            "test append",
        )
        .unwrap_err();
        drop(device);

        match failure {
            WriteFailure::RolledBack(detail) => {
                assert!(detail.contains("not the one ART planned"), "{detail}")
            }
            other => panic!("expected RolledBack, got {other:?}"),
        }
        assert_eq!(std::fs::read(&image).unwrap(), before);
        assert!(find_journal(&image).unwrap().is_none());
    }

    pub(super) fn crash_during(image: &Path, offset: u64, plan: &EditPlan, crash_after: Stage) {
        let mut device = region(image, offset, plan);
        let mut journal =
            Journalled::begin(&mut device, image, offset, "crash test", &plan.blocks()).unwrap();
        let err = write_stages(&mut journal, plan, &mut |done| {
            if done == crash_after {
                Err(CoreError::Cancelled)
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert!(matches!(err, CoreError::Cancelled), "{err:?}");
        // `journal` goes out of scope with neither commit nor roll_back: what a crash leaves.
    }

    /// Spec Testing, "Crash points": after each stage the RDB walks, no used
    /// block is above `HighRDSKBlock`, the PARTs are untouched, the chain is
    /// the old one before S4 and the new one after, and the journal restores
    /// the pre-image. Two shapes, because an empty list links through the
    /// RDSK — which also carries `HighRDSKBlock` — and would hide a reordered
    /// S3.
    #[test]
    fn a_crash_after_any_append_stage_leaves_a_valid_rdb_and_a_journal_that_restores_it() {
        for (shape, range) in [
            ("empty-list", caffeine_like(false)),
            ("dos3-first", dos3_first()),
        ] {
            let old_chain: Vec<u32> = walk_strict(&range)
                .unwrap()
                .fshds
                .iter()
                .map(|f| f.block)
                .collect();
            for crash_after in [Stage::Data, Stage::Header, Stage::HighRdsk, Stage::Link] {
                let tag = format!("crash-{shape}-{crash_after:?}");
                let (_guard, dir) = crate::core::ScratchDir::pair("art-rdbedit", &tag);
                let image = write_image(&dir, &range, 16 * 1024 * 1024);
                let (plan, _) = append_plan(&range, "19.3");
                crash_during(&image, 0, &plan, crash_after);

                let after = read_range(&image, 0).unwrap();
                let walked = walk_strict(&after).unwrap_or_else(|r| panic!("{tag}: {r}"));
                assert!(
                    walked
                        .used
                        .iter()
                        .all(|b| *b <= walked.rdsk.high_rdsk_block),
                    "{tag}: a used block above HighRDSKBlock {}",
                    walked.rdsk.high_rdsk_block
                );
                for part in [1u32, 2] {
                    assert_eq!(
                        block_at(&after, part),
                        block_at(&range, part),
                        "{tag}: PART {part}"
                    );
                }
                let chain: Vec<u32> = walked.fshds.iter().map(|f| f.block).collect();
                if crash_after < Stage::Link {
                    assert_eq!(chain, old_chain, "{tag}: before the link");
                } else {
                    let mut new_chain = old_chain.clone();
                    new_chain.push(plan.allocation.fshd_block);
                    assert_eq!(chain, new_chain, "{tag}: after the link");
                }
                let pending = find_journal(&image)
                    .unwrap()
                    .unwrap_or_else(|| panic!("{tag}: no journal"));
                pending.roll_back().unwrap();
                assert_eq!(
                    read_range(&image, 0).unwrap(),
                    range,
                    "{tag}: the journal restores the pre-image"
                );
            }
        }
    }
}
