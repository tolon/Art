//! Editing a card's RDB in place: the I/O half of ART-117.
//!
//! `core::rdbedit` decides what to write; this module reads the range, writes
//! it through an undo journal in four synced stages, reads it back and keeps
//! the change only when every block is what was planned (spec decisions 3, 8,
//! 9). Nothing here decides a block number.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::card::{is_dynamic_vhd, read_card};
use crate::core::error::{CoreError, CoreResult, RdbEditRefusal};
use crate::core::jobs::ProgressSink;
use crate::core::rdb::{dos_type_string, version_from_ver_string, BLOCK_SIZE};
use crate::core::rdbedit::{
    block_at, check_driver_bytes, check_same_driver, compare_versions, dostype_from_label,
    plan_append, plan_replace, walk_strict, EditKind, EditPlan, Stage, StrictRdb, VersionVerdict,
    DRIVER_MAX_BYTES, EDIT_WINDOW_BYTES,
};
use crate::core::safety::{atomic_create_new, Created};
use crate::core::volume::device::{FileRegion, FileRegionMut};
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

/// A driver's version as the FSHD stores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriverVersion {
    pub version: u16,
    pub revision: u16,
}

impl From<(u16, u16)> for DriverVersion {
    fn from((version, revision): (u16, u16)) -> Self {
        Self { version, revision }
    }
}

impl std::fmt::Display for DriverVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.version, self.revision)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedMode {
    Append,
    Replace,
}

/// An edit ART has planned and can carry out: the range it read is the
/// backup, the baseline for verification and what the plan was made from.
#[derive(Debug)]
pub struct EditReady {
    pub area_offset: u64,
    pub range: Vec<u8>,
    pub plan: EditPlan,
    pub driver: Vec<u8>,
}

/// What a replace comes to.
#[derive(Debug)]
pub enum Prepared {
    Edit(Box<EditReady>),
    /// The card's driver is not older than the file's: nothing to write.
    Keep {
        card: DriverVersion,
        file: Option<DriverVersion>,
    },
}

fn refuse(refusal: RdbEditRefusal) -> CoreError {
    CoreError::RdbEditRefused(refusal)
}

fn dos_type_of(label: &str) -> CoreResult<u32> {
    dostype_from_label(label)
        .ok_or_else(|| CoreError::InvalidInput(format!("'{label}' is not a DosType ART can name")))
}

/// The driver: its size from metadata before a byte is read, then its bytes.
#[allow(clippy::type_complexity)]
fn load_driver(path: &Path) -> CoreResult<(Vec<u8>, Option<(u16, u16)>)> {
    let bytes = std::fs::metadata(path)?.len();
    if bytes > DRIVER_MAX_BYTES {
        return Err(refuse(RdbEditRefusal::DriverTooLarge {
            bytes,
            cap: DRIVER_MAX_BYTES,
        }));
    }
    let data = std::fs::read(path)?;
    check_driver_bytes(&data).map_err(refuse)?;
    let version = version_from_ver_string(&data);
    Ok((data, version))
}

/// Where the Amiga disk in `slot` starts: `None` means a plain image's one
/// disk at byte 0, and a slot must be a readable `0x76` area.
fn area_offset_for(image: &Path, slot: Option<usize>) -> CoreResult<u64> {
    let card = read_card(image)?;
    let offset = match (&card.mbr, slot) {
        (None, None) => card.areas.first().map(|area| area.offset_bytes),
        (Some(mbr), Some(slot)) => mbr
            .amiga_areas()
            .into_iter()
            .find(|part| part.slot_number() == slot)
            .map(|part| part.start_bytes())
            .filter(|start| card.areas.iter().any(|area| area.offset_bytes == *start)),
        _ => None,
    };
    offset.ok_or_else(|| {
        CoreError::InvalidInput(match slot {
            Some(slot) => format!("this card has no readable Amiga disk in MBR slot {slot}"),
            None => "this image has no readable Amiga disk at its start".into(),
        })
    })
}

/// The card's refusals, then its range walked strictly.
fn open_rdb(image: &Path, slot: Option<usize>) -> CoreResult<(u64, Vec<u8>, StrictRdb)> {
    if is_dynamic_vhd(image)? {
        return Err(refuse(RdbEditRefusal::DynamicVhd));
    }
    let journal = journal_path_for(image);
    if journal.exists() {
        return Err(refuse(RdbEditRefusal::JournalPending { journal }));
    }
    let area_offset = area_offset_for(image, slot)?;
    let range = read_range(image, area_offset)?;
    let walk = walk_strict(&range).map_err(refuse)?;
    Ok((area_offset, range, walk))
}

/// Plan an append, or say why not. Reads; writes nothing.
pub fn prepare_append(
    image: &Path,
    slot: Option<usize>,
    dostype: &str,
    driver: &Path,
) -> CoreResult<EditReady> {
    let dos_type = dos_type_of(dostype)?;
    let (data, version) = load_driver(driver)?;
    let version = version.ok_or_else(|| refuse(RdbEditRefusal::DriverNoVersion))?;
    let (area_offset, range, walk) = open_rdb(image, slot)?;
    let plan = plan_append(&range, &walk, dos_type, version, &data).map_err(refuse)?;
    Ok(EditReady {
        area_offset,
        range,
        plan,
        driver: data,
    })
}

/// Plan a replace, keep the card's driver, or say why not. Reads; writes
/// nothing.
pub fn prepare_replace(
    image: &Path,
    slot: Option<usize>,
    dostype: &str,
    driver: &Path,
) -> CoreResult<Prepared> {
    let dos_type = dos_type_of(dostype)?;
    let (data, version) = load_driver(driver)?;
    let (area_offset, range, walk) = open_rdb(image, slot)?;
    let Some(on_card) = walk.fshds.iter().find(|fs| fs.dos_type == dos_type) else {
        return Err(refuse(RdbEditRefusal::Unaccounted {
            block: walk.rdsk.block,
            check: format!("this RDB carries no {dostype} driver to replace"),
        }));
    };
    // Spec decision 13, before the versions mean anything: the same program
    // on both sides, or no replace.
    check_same_driver(&on_card.payload, &data).map_err(refuse)?;
    let card = DriverVersion {
        version: on_card.version,
        revision: on_card.revision,
    };
    match (
        compare_versions((card.version, card.revision), version),
        version,
    ) {
        (VersionVerdict::Newer, Some(file)) => {
            let plan = plan_replace(&range, &walk, dos_type, file, &data).map_err(refuse)?;
            Ok(Prepared::Edit(Box::new(EditReady {
                area_offset,
                range,
                plan,
                driver: data,
            })))
        }
        _ => Ok(Prepared::Keep {
            card,
            file: version.map(DriverVersion::from),
        }),
    }
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

/// What a finished edit did — for the result panel and the operation log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbedReport {
    pub slot: Option<usize>,
    pub dostype: String,
    /// The replaced driver's version; `None` for an append.
    pub card_version: Option<DriverVersion>,
    pub file_version: DriverVersion,
    pub first_block: u32,
    pub last_block: u32,
    /// `[before, after]` when decision 12 raised `RDBBlocksHi`.
    pub rdb_blocks_hi_raised: Option<[u32; 2]>,
    pub backup: PathBuf,
}

/// Which edit, on which card.
#[derive(Debug, Clone, Copy)]
pub struct EmbedTarget<'a> {
    pub image: &'a Path,
    pub slot: Option<usize>,
    pub dostype: &'a str,
    pub mode: EmbedMode,
    pub driver: &'a Path,
}

/// Decision 7: area blocks `0..blocks` into a new file — SAFE_CREATE, written
/// to a temporary and renamed, then read back and compared.
pub fn write_backup(path: &Path, range: &[u8], blocks: u32) -> CoreResult<()> {
    let failed = |detail: String| CoreError::RdbBackupFailed {
        path: path.to_path_buf(),
        detail,
    };
    let bytes = range
        .get(..blocks as usize * BLOCK_SIZE)
        .ok_or_else(|| failed("the RDB area ART read is shorter than its reserved range".into()))?;
    match atomic_create_new(path, bytes) {
        Ok(Created::Yes) => {}
        Ok(Created::AlreadyThere) => {
            return Err(refuse(RdbEditRefusal::BackupExists {
                path: path.to_path_buf(),
            }))
        }
        Err(err) => return Err(failed(err.to_string())),
    }
    let back = std::fs::read(path).map_err(|err| failed(err.to_string()))?;
    if back != bytes {
        return Err(failed("the file read back is not what ART wrote".into()));
    }
    Ok(())
}

pub(crate) type OpenDevice = dyn Fn(&Path, u64, u64) -> CoreResult<Box<dyn BlockDeviceMut>>;

fn open_region(image: &Path, offset: u64, length: u64) -> CoreResult<Box<dyn BlockDeviceMut>> {
    Ok(Box::new(FileRegionMut::open(
        image, offset, length, BLOCK_SIZE,
    )?))
}

/// Carry out one RDB edit: plan it again from the card, back the range up,
/// write it journalled, verify, and report — or end in one of decision 10's
/// sentences.
pub fn run(
    target: EmbedTarget<'_>,
    backup: Option<&Path>,
    sink: &dyn ProgressSink,
) -> CoreResult<EmbedReport> {
    run_with(target, backup, sink, &open_region)
}

pub(crate) fn run_with(
    target: EmbedTarget<'_>,
    backup: Option<&Path>,
    sink: &dyn ProgressSink,
    open: &OpenDevice,
) -> CoreResult<EmbedReport> {
    let Some(backup) = backup else {
        return Err(refuse(RdbEditRefusal::NoBackup));
    };
    let ready = match target.mode {
        EmbedMode::Append => {
            prepare_append(target.image, target.slot, target.dostype, target.driver)?
        }
        EmbedMode::Replace => {
            match prepare_replace(target.image, target.slot, target.dostype, target.driver)? {
                Prepared::Edit(ready) => *ready,
                Prepared::Keep { card, file } => {
                    return Err(CoreError::InvalidInput(format!(
                        "the card's {} driver is {card} and the file states {}, so there is \
                         nothing newer to write",
                        target.dostype,
                        file.map(|f| f.to_string())
                            .unwrap_or_else(|| "no version".into())
                    )))
                }
            }
        }
    };
    let plan = &ready.plan;
    let blocks = plan.allocation.rdb_blocks_hi + 1;

    sink.report(
        0,
        None,
        &format!("Copying the RDB area to {}", backup.display()),
    );
    write_backup(backup, &ready.range, blocks)?;

    let length = u64::from(blocks) * BLOCK_SIZE as u64;
    let mut device = open(target.image, ready.area_offset, length).map_err(|err| {
        CoreError::RdbEditUntouched {
            backup: backup.to_path_buf(),
            detail: err.to_string(),
        }
    })?;
    sink.report(
        0,
        None,
        &format!(
            "Writing {} to RDB blocks {}–{}",
            target.dostype, plan.allocation.fshd_block, plan.allocation.last_block
        ),
    );
    let verb = match plan.kind {
        EditKind::Append => "Embed",
        EditKind::Replace => "Replace",
    };
    let description = format!(
        "{verb} {} {} in the RDB",
        target.dostype,
        DriverVersion::from(plan.file_version)
    );

    match write_journalled(
        &mut *device,
        target.image,
        ready.area_offset,
        &ready.range,
        plan,
        &ready.driver,
        &description,
    ) {
        Ok(()) => Ok(EmbedReport {
            slot: target.slot,
            dostype: target.dostype.to_string(),
            card_version: plan.card_version.map(DriverVersion::from),
            file_version: plan.file_version.into(),
            first_block: plan.allocation.fshd_block,
            last_block: plan.allocation.last_block,
            rdb_blocks_hi_raised: plan
                .allocation
                .raised_from
                .map(|from| [from, plan.allocation.rdb_blocks_hi]),
            backup: backup.to_path_buf(),
        }),
        Err(WriteFailure::BeforeFirstWrite(err)) => Err(CoreError::RdbEditUntouched {
            backup: backup.to_path_buf(),
            detail: err.to_string(),
        }),
        Err(WriteFailure::RolledBack(detail)) => Err(CoreError::RdbEditFailed {
            backup: backup.to_path_buf(),
            restored: true,
            journal: journal_path_for(target.image),
            detail,
        }),
        Err(WriteFailure::RollbackFailed { detail, journal }) => Err(CoreError::RdbEditFailed {
            backup: backup.to_path_buf(),
            restored: false,
            journal,
            detail,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::core::error::RdbEditRefusal;
    use crate::core::jobs::NoProgress;
    use crate::core::rdbedit::fixtures::*;
    use crate::core::rdbedit::{block_at, plan_append, plan_replace, walk_strict, EditPlan, Stage};
    use crate::core::volume::journal::find_journal;
    use crate::core::volume::BlockDevice;

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

    fn replace_plan(range: &[u8]) -> (EditPlan, Vec<u8>) {
        let driver = hunk_driver(62_604, "19.3");
        let plan =
            plan_replace(range, &walk_strict(range).unwrap(), PDS3, (19, 3), &driver).unwrap();
        (plan, driver)
    }

    /// Spec Testing: 132–260 new; the old driver at 3–131, the stale copy at
    /// 2048–2179 and the PFS3 blocks from 5120 byte-identical afterwards.
    #[test]
    fn a_replace_on_the_caffeine_shape_swaps_one_pointer_and_leaves_the_old_driver_where_it_was() {
        let (_guard, dir) = crate::core::ScratchDir::pair("art-rdbedit", "replace-caffeine");
        let range = caffeine_like(true);
        let image = write_image(&dir, &range, 16 * 1024 * 1024);
        let (plan, driver) = replace_plan(&range);

        let mut device = region(&image, 0, &plan);
        write_journalled(
            &mut device,
            &image,
            0,
            &range,
            &plan,
            &driver,
            "test replace",
        )
        .unwrap_or_else(|failure| panic!("{failure:?}"));
        drop(device);

        let after = read_range(&image, 0).unwrap();
        for block in (1..16_384u32).filter(|b| !(132..=260).contains(b)) {
            assert_eq!(
                block_at(&after, block),
                block_at(&range, block),
                "block {block}"
            );
        }
        let walked = walk_strict(&after).unwrap();
        assert_eq!(
            walked
                .fshds
                .iter()
                .map(|f| (f.block, f.version, f.revision))
                .collect::<Vec<_>>(),
            vec![(132, 19, 3)]
        );
        assert_eq!(walked.fshds[0].payload, driver);
        assert_eq!(&after[132 * 512 + 172..132 * 512 + 184], b"L:pfs3aio040");
    }

    #[test]
    fn a_replace_on_an_art_built_card_raises_rdb_blocks_hi_to_259() {
        let (_guard, dir) = crate::core::ScratchDir::pair("art-rdbedit", "replace-art");
        let range = art_like(true);
        let image = write_image(&dir, &range, 64 * 1024 * 1024);
        let (plan, driver) = replace_plan(&range);

        let mut device = region(&image, 0, &plan);
        write_journalled(
            &mut device,
            &image,
            0,
            &range,
            &plan,
            &driver,
            "test replace",
        )
        .unwrap_or_else(|failure| panic!("{failure:?}"));
        drop(device);

        let walked = walk_strict(&read_range(&image, 0).unwrap()).unwrap();
        assert_eq!(
            (walked.rdsk.rdb_blocks_hi, walked.rdsk.high_rdsk_block),
            (259, 259)
        );
        assert_eq!(walked.fshds[0].block, 131);
        assert_eq!(walked.first_partition_block(), Some(2016));
    }

    #[test]
    fn a_crash_after_any_replace_stage_leaves_a_valid_rdb_and_a_journal_that_restores_it() {
        for (shape, range) in [
            ("head", caffeine_like(true)),
            ("after-dos3", two_drivers(DOS3, PDS3)),
            ("raise", art_like(true)),
        ] {
            let old_chain: Vec<u32> = walk_strict(&range)
                .unwrap()
                .fshds
                .iter()
                .map(|f| f.block)
                .collect();
            for crash_after in [Stage::Data, Stage::Header, Stage::HighRdsk, Stage::Link] {
                let tag = format!("crash-replace-{shape}-{crash_after:?}");
                let (_guard, dir) = crate::core::ScratchDir::pair("art-rdbedit", &tag);
                let image = write_image(&dir, &range, 64 * 1024 * 1024);
                let (plan, _) = replace_plan(&range);
                crash_during(&image, 0, &plan, crash_after);

                let after = read_range(&image, 0).unwrap();
                let walked = walk_strict(&after).unwrap_or_else(|r| panic!("{tag}: {r}"));
                assert!(
                    walked
                        .used
                        .iter()
                        .all(|b| *b <= walked.rdsk.high_rdsk_block),
                    "{tag}"
                );
                let chain: Vec<u32> = walked.fshds.iter().map(|f| f.block).collect();
                let expected: Vec<u32> = if crash_after < Stage::Link {
                    old_chain.clone()
                } else {
                    old_chain
                        .iter()
                        .map(|b| {
                            if Some(*b) == plan.replaced_fshd {
                                plan.allocation.fshd_block
                            } else {
                                *b
                            }
                        })
                        .collect()
                };
                assert_eq!(chain, expected, "{tag}");
                find_journal(&image)
                    .unwrap()
                    .unwrap_or_else(|| panic!("{tag}: no journal"))
                    .roll_back()
                    .unwrap();
                assert_eq!(
                    read_range(&image, 0).unwrap(),
                    range,
                    "{tag}: the journal restores the pre-image"
                );
            }
        }
    }

    fn driver_file(dir: &Path, bytes: &[u8]) -> PathBuf {
        let path = dir.join("pfs3aio");
        std::fs::write(&path, bytes).unwrap();
        path
    }

    /// A refusal says which one it is, in a sentence, and the image is the
    /// bytes it was.
    fn assert_refused(image: &Path, before: &[u8], err: CoreError, code: &str, needle: &str) {
        assert!(matches!(err, CoreError::RdbEditRefused(_)), "{err:?}");
        assert_eq!(err.code(), code, "{err}");
        let sentence = err.to_string();
        assert!(sentence.contains(needle), "{sentence}");
        assert!(sentence.contains("Nothing was written."), "{sentence}");
        assert_eq!(
            std::fs::read(image).unwrap(),
            before,
            "a refusal writes nothing"
        );
    }

    fn scratch_card(
        tag: &str,
        range: &[u8],
    ) -> (crate::core::ScratchDir, PathBuf, PathBuf, Vec<u8>) {
        let (guard, dir) = crate::core::ScratchDir::pair("art-rdbedit", tag);
        let image = write_image(&dir, range, 16 * 1024 * 1024);
        let before = std::fs::read(&image).unwrap();
        (guard, dir, image, before)
    }

    #[test]
    fn unaccounted_is_refused_through_prepare() {
        let mut range = caffeine_like(false);
        range[2 * 512 + 100] ^= 1;
        let (_guard, dir, image, before) = scratch_card("refuse-unaccounted", &range);
        let driver = driver_file(&dir, &hunk_driver(62_604, "19.3"));
        let err = prepare_append(&image, None, "PDS3", &driver).unwrap_err();
        assert_refused(
            &image,
            &before,
            err,
            "ART-RDB-EDIT-UNACCOUNTED",
            "PART block 2 fails its checksum",
        );
    }

    #[test]
    fn a_bad_block_list_is_refused_through_prepare() {
        let mut range = caffeine_like(false);
        put(&mut range, 0, 6, 9);
        seal(&mut range, 0);
        let (_guard, dir, image, before) = scratch_card("refuse-badb", &range);
        let driver = driver_file(&dir, &hunk_driver(62_604, "19.3"));
        let err = prepare_append(&image, None, "PDS3", &driver).unwrap_err();
        assert_refused(
            &image,
            &before,
            err,
            "ART-RDB-EDIT-BAD-BLOCKS",
            "hst-imager can",
        );
    }

    #[test]
    fn no_room_is_refused_through_prepare() {
        let mut range = art_like(false);
        range[50 * 512] = 1;
        let (_guard, dir, image, before) = scratch_card("refuse-no-room", &range);
        let driver = driver_file(&dir, &hunk_driver(62_604, "19.3"));
        let err = prepare_append(&image, None, "PDS3", &driver).unwrap_err();
        assert_refused(
            &image,
            &before,
            err,
            "ART-RDB-EDIT-NO-ROOM",
            "block 50 above it is not empty",
        );
    }

    #[test]
    fn an_archive_is_refused_as_not_executable() {
        let (_guard, dir, image, before) = scratch_card("refuse-lha", &caffeine_like(false));
        let mut lha = vec![0u8; 1024];
        lha[..7].copy_from_slice(b"\x2a\x00-lh5-");
        let driver = driver_file(&dir, &lha);
        let err = prepare_append(&image, None, "PDS3", &driver).unwrap_err();
        assert_refused(
            &image,
            &before,
            err,
            "ART-RDB-EDIT-NOT-EXECUTABLE",
            "An .lha archive has to be unpacked first",
        );
    }

    #[test]
    fn a_driver_over_512_kib_is_refused_by_its_size() {
        let (_guard, dir, image, before) = scratch_card("refuse-large", &caffeine_like(false));
        let mut big = hunk_driver(1024, "19.3");
        big.resize(512 * 1024 + 4, 0);
        let driver = driver_file(&dir, &big);
        let err = prepare_append(&image, None, "PDS3", &driver).unwrap_err();
        assert_refused(
            &image,
            &before,
            err,
            "ART-RDB-EDIT-DRIVER-TOO-LARGE",
            "524292 bytes, over the 524288-byte limit",
        );
    }

    #[test]
    fn an_append_whose_driver_states_no_version_is_refused() {
        let (_guard, dir, image, before) = scratch_card("refuse-no-ver", &caffeine_like(false));
        let mut silent = hunk_driver(62_604, "19.3");
        silent[64..69].copy_from_slice(b"$XXX:");
        let driver = driver_file(&dir, &silent);
        let err = prepare_append(&image, None, "PDS3", &driver).unwrap_err();
        assert_refused(
            &image,
            &before,
            err,
            "ART-RDB-EDIT-DRIVER-NO-VERSION",
            "will not write one it guessed",
        );
    }

    #[test]
    fn a_dynamic_vhd_is_refused() {
        let (_guard, dir) = crate::core::ScratchDir::pair("art-rdbedit", "refuse-vhd");
        let image = dir.join("card.vhd");
        let mut bytes = vec![0u8; 4096];
        bytes[..8].copy_from_slice(b"conectix");
        std::fs::write(&image, &bytes).unwrap();
        let driver = driver_file(&dir, &hunk_driver(62_604, "19.3"));
        let err = prepare_append(&image, None, "PDS3", &driver).unwrap_err();
        assert_refused(
            &image,
            &bytes,
            err,
            "ART-RDB-EDIT-DYNAMIC-VHD",
            "a write can grow a dynamic VHD",
        );
    }

    #[test]
    fn a_journal_beside_the_image_is_refused_by_its_path() {
        let (_guard, dir, image, before) = scratch_card("refuse-journal", &caffeine_like(false));
        let journal = journal_path_for(&image);
        std::fs::write(&journal, b"left by a crash").unwrap();
        let driver = driver_file(&dir, &hunk_driver(62_604, "19.3"));
        let err = prepare_append(&image, None, "PDS3", &driver).unwrap_err();
        let path = journal.display().to_string();
        assert_refused(&image, &before, err, "ART-RDB-EDIT-JOURNAL-PENDING", &path);
    }

    /// Decision 1: equal, older and silent files plan no edit — and say so.
    #[test]
    fn a_replace_with_an_equal_older_or_silent_file_keeps_the_cards_driver() {
        let (_guard, dir, image, _) = scratch_card("replace-keep", &caffeine_like(true));
        let card = DriverVersion {
            version: 19,
            revision: 2,
        };
        for (version, file) in [("19.2", Some((19, 2))), ("19.1", Some((19, 1)))] {
            let driver = driver_file(&dir, &hunk_driver(62_604, version));
            match prepare_replace(&image, None, "PDS3", &driver).unwrap() {
                Prepared::Keep {
                    card: kept,
                    file: stated,
                } => {
                    assert_eq!(
                        (kept, stated),
                        (card, file.map(DriverVersion::from)),
                        "{version}"
                    );
                }
                Prepared::Edit(_) => panic!("{version} is not newer than 19.2"),
            }
        }
        let mut silent = hunk_driver(62_604, "19.3");
        silent[64..69].copy_from_slice(b"$XXX:");
        let driver = driver_file(&dir, &silent);
        assert!(matches!(
            prepare_replace(&image, None, "PDS3", &driver).unwrap(),
            Prepared::Keep { file: None, .. }
        ));

        let driver = driver_file(&dir, &hunk_driver(62_604, "19.10"));
        match prepare_replace(&image, None, "PDS3", &driver).unwrap() {
            Prepared::Edit(ready) => assert_eq!(ready.plan.file_version, (19, 10)),
            Prepared::Keep { .. } => panic!("19.10 is newer than 19.2"),
        }
    }

    /// Spec decision 13 through prepare: the names are asked before the
    /// versions. An `SFS\0` driver is not replaced by pfs3aio although 19.3 >
    /// 1.293, and a card driver that names no program is refused even against
    /// an older file — a "not newer" note would hide that ART never knew what
    /// the card's driver was.
    #[test]
    fn a_replace_across_different_drivers_is_refused_before_anything_is_written() {
        let shape = |dos_type: u32, driver: Vec<u8>, version: (u16, u16)| {
            build(&Shape {
                cylinders: 38_488,
                heads: 12,
                sectors: 256,
                rdb_blocks_hi: 6143,
                lo_cylinder: 2,
                parts: vec![("SDH0", 2, 535, dos_type)],
                fshds: vec![FshdSpec {
                    dos_type,
                    version,
                    driver,
                    name: None,
                    summed_longs: 64,
                }],
                high_rdsk_block: None,
                total_blocks: WINDOW_BLOCKS,
            })
        };

        let sfs = shape(
            SFS0,
            named_driver(4096, "SmartFilesystem", "1.293"),
            (1, 293),
        );
        let (_guard, dir, image, before) = scratch_card("replace-different", &sfs);
        let driver = driver_file(&dir, &hunk_driver(62_604, "19.3"));
        let err = prepare_replace(&image, None, "SFS0", &driver).unwrap_err();
        assert!(
            matches!(&err, CoreError::RdbEditRefused(RdbEditRefusal::DifferentDriver { card: Some(card), file })
                if card == "SmartFilesystem" && file == "pfs3aio"),
            "{err:?}"
        );
        assert_refused(
            &image,
            &before,
            err,
            "ART-RDB-EDIT-DIFFERENT-DRIVER",
            "calls itself 'SmartFilesystem'",
        );

        let mut unnamed = hunk_driver(4096, "19.2");
        unnamed[64..69].copy_from_slice(b"$XXX:");
        let (_guard2, dir2, image2, before2) =
            scratch_card("replace-unnamed", &shape(PDS3, unnamed, (19, 2)));
        let older = driver_file(&dir2, &hunk_driver(62_604, "19.1"));
        let err = prepare_replace(&image2, None, "PDS3", &older).unwrap_err();
        assert!(
            matches!(
                &err,
                CoreError::RdbEditRefused(RdbEditRefusal::DifferentDriver { card: None, .. })
            ),
            "{err:?}"
        );
        assert_refused(
            &image2,
            &before2,
            err,
            "ART-RDB-EDIT-DIFFERENT-DRIVER",
            "does not say what it is",
        );
    }

    /// A card is a list of disks: the step's MBR slot picks the area.
    #[test]
    fn the_mbr_slot_picks_the_area_and_a_slot_that_is_not_one_is_refused() {
        let (_guard, dir) = crate::core::ScratchDir::pair("art-rdbedit", "slot");
        let image = write_card(&dir, &caffeine_like(false));
        let driver = driver_file(&dir, &hunk_driver(62_604, "19.3"));
        let ready = prepare_append(&image, Some(2), "PDS3", &driver).unwrap();
        assert_eq!(ready.area_offset, 8192 * 512);
        for slot in [Some(1), None] {
            let err = prepare_append(&image, slot, "PDS3", &driver).unwrap_err();
            assert_eq!(err.code(), "ART-INPUT-INVALID", "{slot:?}: {err}");
            assert!(
                err.to_string().contains("no readable Amiga disk"),
                "{slot:?}: {err}"
            );
        }
    }

    enum Fault {
        /// Only this write (counting from 1) fails; every other one lands.
        OnceAt(usize),
        /// This write and every later one fails — the rollback's too.
        FromWrite(usize),
    }

    struct Faulty {
        inner: FileRegionMut,
        writes: usize,
        fault: Fault,
    }

    impl BlockDevice for Faulty {
        fn block_size(&self) -> usize {
            self.inner.block_size()
        }
        fn total_blocks(&self) -> u32 {
            self.inner.total_blocks()
        }
        fn read_block(&self, n: u32, buf: &mut [u8]) -> CoreResult<()> {
            self.inner.read_block(n, buf)
        }
    }

    impl BlockDeviceMut for Faulty {
        fn write_block(&mut self, n: u32, buf: &[u8]) -> CoreResult<()> {
            self.writes += 1;
            let fails = match self.fault {
                Fault::OnceAt(at) => self.writes == at,
                Fault::FromWrite(at) => self.writes >= at,
            };
            if fails {
                return Err(CoreError::Io(std::io::Error::other("the card was pulled")));
            }
            self.inner.write_block(n, buf)
        }
        fn sync(&mut self) -> CoreResult<()> {
            self.inner.sync()
        }
    }

    /// A struct guard goes last (`scripts/scratch-guard-sweep.py`): fields
    /// drop in order, so the folder is removed after everything naming it.
    struct Run {
        dir: PathBuf,
        image: PathBuf,
        driver: PathBuf,
        before: Vec<u8>,
        range: Vec<u8>,
        _guard: crate::core::ScratchDir,
    }

    fn art_run(tag: &str) -> Run {
        let (guard, dir) = crate::core::ScratchDir::pair("art-rdbedit", tag);
        let range = art_like(false);
        let image = write_image(&dir, &range, 64 * 1024 * 1024);
        let driver = driver_file(&dir, &hunk_driver(62_604, "19.3"));
        let before = std::fs::read(&image).unwrap();
        Run {
            dir,
            image,
            driver,
            before,
            range,
            _guard: guard,
        }
    }

    fn target(run: &Run) -> EmbedTarget<'_> {
        EmbedTarget {
            image: &run.image,
            slot: None,
            dostype: "PDS3",
            mode: EmbedMode::Append,
            driver: &run.driver,
        }
    }

    fn with_fault(
        fault: fn() -> Fault,
    ) -> impl Fn(&Path, u64, u64) -> CoreResult<Box<dyn BlockDeviceMut>> {
        move |image, offset, length| {
            let inner = FileRegionMut::open(image, offset, length, BLOCK_SIZE)?;
            Ok(Box::new(Faulty {
                inner,
                writes: 0,
                fault: fault(),
            }) as Box<dyn BlockDeviceMut>)
        }
    }

    #[test]
    fn a_run_backs_up_the_range_as_it_was_then_embeds_and_reports() {
        let run = art_run("run-ok");
        let backup = run.dir.join("card-rdb-backup.bin");
        let report = super::run(target(&run), Some(&backup), &NoProgress).unwrap();
        assert_eq!(
            report,
            EmbedReport {
                slot: None,
                dostype: "PDS3".into(),
                card_version: None,
                file_version: DriverVersion {
                    version: 19,
                    revision: 3
                },
                first_block: 2,
                last_block: 130,
                rdb_blocks_hi_raised: Some([1, 130]),
                backup: backup.clone(),
            }
        );
        assert_eq!(
            std::fs::read(&backup).unwrap(),
            run.range[..131 * 512],
            "the pre-image, raised range included"
        );
        assert!(crate::core::card::read_card(&run.image)
            .unwrap()
            .provides_file_system(PDS3));
    }

    #[test]
    fn no_backup_location_is_refused_and_nothing_is_made() {
        let run = art_run("run-no-backup");
        let err = super::run(target(&run), None, &NoProgress).unwrap_err();
        assert_eq!(err.code(), "ART-RDB-EDIT-NO-BACKUP");
        assert!(
            err.to_string().contains("choose where the RDB backup goes"),
            "{err}"
        );
        assert_eq!(std::fs::read(&run.image).unwrap(), run.before);
        assert_eq!(
            std::fs::read_dir(&run.dir).unwrap().count(),
            2,
            "only the image and the driver"
        );
    }

    #[test]
    fn an_existing_backup_file_is_refused_and_left_as_it_was() {
        let run = art_run("run-backup-exists");
        let backup = run.dir.join("mine.bin");
        std::fs::write(&backup, b"mine").unwrap();
        let err = super::run(target(&run), Some(&backup), &NoProgress).unwrap_err();
        assert_eq!(err.code(), "ART-RDB-EDIT-BACKUP-EXISTS");
        assert!(
            err.to_string().contains(&backup.display().to_string()),
            "{err}"
        );
        assert_eq!(std::fs::read(&backup).unwrap(), b"mine");
        assert_eq!(std::fs::read(&run.image).unwrap(), run.before);
    }

    #[test]
    fn a_backup_that_cannot_be_written_leaves_the_card_untouched() {
        let run = art_run("run-backup-failed");
        let backup = run.dir.join("no-such-folder").join("rdb.bin");
        let err = super::run(target(&run), Some(&backup), &NoProgress).unwrap_err();
        assert!(matches!(err, CoreError::RdbBackupFailed { .. }), "{err:?}");
        assert_eq!(err.code(), "ART-RDB-BACKUP-FAILED");
        let sentence = err.to_string();
        assert!(
            sentence.contains(&backup.display().to_string())
                && sentence.contains("The card was not touched."),
            "{sentence}"
        );
        assert_eq!(std::fs::read(&run.image).unwrap(), run.before);
    }

    #[test]
    fn a_device_that_will_not_open_stops_before_the_first_write_and_names_the_backup() {
        let run = art_run("run-untouched");
        let backup = run.dir.join("rdb.bin");
        let refuse_open = |_: &Path, _: u64, _: u64| -> CoreResult<Box<dyn BlockDeviceMut>> {
            Err(CoreError::Io(std::io::Error::other("the image is locked")))
        };
        let err = run_with(target(&run), Some(&backup), &NoProgress, &refuse_open).unwrap_err();
        assert_eq!(err.code(), "ART-RDB-EDIT-UNTOUCHED", "{err}");
        assert!(
            err.to_string().contains(&backup.display().to_string()),
            "{err}"
        );
        assert_eq!(
            std::fs::read(&backup).unwrap(),
            run.range[..131 * 512],
            "the backup exists before any write"
        );
        assert_eq!(std::fs::read(&run.image).unwrap(), run.before);
    }

    #[test]
    fn a_write_that_fails_is_rolled_back_and_names_the_backup() {
        let run = art_run("run-rolled-back");
        let backup = run.dir.join("rdb.bin");
        let err = run_with(
            target(&run),
            Some(&backup),
            &NoProgress,
            &with_fault(|| Fault::OnceAt(5)),
        )
        .unwrap_err();
        assert!(
            matches!(err, CoreError::RdbEditFailed { restored: true, .. }),
            "{err:?}"
        );
        assert_eq!(err.code(), "ART-RDB-EDIT-ROLLED-BACK");
        let sentence = err.to_string();
        assert!(
            sentence.contains("put every block it wrote back")
                && sentence.contains(&backup.display().to_string()),
            "{sentence}"
        );
        assert_eq!(std::fs::read(&run.image).unwrap(), run.before);
        assert!(!journal_path_for(&run.image).exists());
    }

    #[test]
    fn a_rollback_that_fails_leaves_the_journal_and_names_it_and_the_backup() {
        let run = art_run("run-rollback-failed");
        let backup = run.dir.join("rdb.bin");
        let err = run_with(
            target(&run),
            Some(&backup),
            &NoProgress,
            &with_fault(|| Fault::FromWrite(5)),
        )
        .unwrap_err();
        assert_eq!(err.code(), "ART-RDB-EDIT-ROLLBACK-FAILED", "{err}");
        let journal = journal_path_for(&run.image);
        let sentence = err.to_string();
        assert!(
            sentence.contains(&journal.display().to_string()),
            "{sentence}"
        );
        assert!(
            sentence.contains(&backup.display().to_string()),
            "{sentence}"
        );
        assert!(
            sentence.contains("undo it in the File Manager"),
            "{sentence}"
        );
        // And the File Manager's own recovery does put the card back.
        find_journal(&run.image)
            .unwrap()
            .unwrap()
            .roll_back()
            .unwrap();
        assert_eq!(std::fs::read(&run.image).unwrap(), run.before);
    }

    /// ART-117 on the owner's own card — on a byte copy the owner made.
    ///
    /// `TMP`/`TEMP` no longer need setting by hand (ART-320,
    /// `src-tauri/.cargo/config.toml` already forces them onto
    /// `E:\amiga\ProjeART\build\tmp`), so this runs as a plain `cargo test --lib`
    /// and its own TMP-on-`E:` pre-flight below passes without the
    /// compiled-binary workaround ART-320 needed before the fix.
    ///
    /// ```text
    /// cd src-tauri
    /// ART_CARD_IN="E:\amiga\Amigatolon\caffeine\CaffeineOS_Storm_9317.img" \
    /// ART_RDB_EMBED_COPY="E:\amiga\ProjeART\caffeine-copy.img" \
    /// ART_RDB_EMBED_DRIVER="E:\amiga\ProjeART\pfs3aio-newer" \
    /// ART_RDB_EMBED_OUT="E:\amiga\ProjeART\art117-owner" \
    ///   cargo test --lib replace_the_driver_on_a_copy_of_the_owners_card_when_asked -- --nocapture --ignored
    /// ```
    ///
    /// The card carries `pfs3aio` 19.2. If no newer `pfs3aio` exists, the
    /// driver is a scratch copy with its `$VER:` digits raised.
    #[test]
    #[ignore = "writes to a byte copy of the owner's card under E:\\amiga\\ProjeART; run explicitly"]
    fn replace_the_driver_on_a_copy_of_the_owners_card_when_asked() {
        let var = |name: &str| {
            std::env::var(name).unwrap_or_else(|_| {
                panic!("set {name}: this hook refuses without ART_CARD_IN, ART_RDB_EMBED_COPY, ART_RDB_EMBED_DRIVER and ART_RDB_EMBED_OUT")
            })
        };
        let original = PathBuf::from(var("ART_CARD_IN"));
        let copy = PathBuf::from(var("ART_RDB_EMBED_COPY"));
        let driver = PathBuf::from(var("ART_RDB_EMBED_DRIVER"));
        let out = PathBuf::from(var("ART_RDB_EMBED_OUT"));
        assert!(
            out.is_dir(),
            "ART_RDB_EMBED_OUT must be an existing folder: {}",
            out.display()
        );
        assert!(
            std::env::temp_dir().starts_with(r"E:\amiga\ProjeART\build\tmp"),
            "set TMP and TEMP to E:\\amiga\\ProjeART\\build\\tmp: nothing of this hook goes to C:"
        );

        assert_ne!(
            std::fs::canonicalize(&original).unwrap(),
            std::fs::canonicalize(&copy).unwrap(),
            "ART_RDB_EMBED_COPY names the original card: make a byte copy and point at that"
        );
        assert_eq!(
            std::fs::metadata(&original).unwrap().len(),
            std::fs::metadata(&copy).unwrap().len(),
            "the copy is not the original's size"
        );
        let original_mtime = std::fs::metadata(&original).unwrap().modified().unwrap();

        let card = read_card(&copy).unwrap();
        let area = card
            .areas
            .iter()
            .find(|area| area.rdb.provides_file_system(0x5044_5303))
            .expect("a PDS3 driver on the card");
        let slot = card
            .mbr
            .as_ref()
            .and_then(|mbr| {
                mbr.amiga_areas()
                    .into_iter()
                    .find(|p| p.start_bytes() == area.offset_bytes)
            })
            .map(|p| p.slot_number());
        let original_range = read_range(&original, area.offset_bytes).unwrap();
        let before = walk_strict(&original_range).unwrap();
        println!(
            "area at {} slot {slot:?}: RDBBlocksHi {} HighRDSKBlock {} used {:?}..={:?} partition from block {:?}",
            area.offset_bytes,
            before.rdsk.rdb_blocks_hi,
            before.rdsk.high_rdsk_block,
            before.used.first(),
            before.used.last(),
            before.first_partition_block()
        );

        match prepare_replace(&copy, slot, "PDS3", &driver).unwrap() {
            Prepared::Keep { card, file } => panic!(
                "the driver states {file:?}, which is not newer than the card's {card}; use a scratch copy with its $VER: digits raised"
            ),
            Prepared::Edit(ready) => {
                for (stage, writes) in &ready.plan.stages {
                    let blocks: Vec<u32> = writes.iter().map(|w| w.block).collect();
                    println!("  {stage:?}: {} block(s), {:?}..={:?}", blocks.len(), blocks.first(), blocks.last());
                }
                println!("  allocation: {:?}", ready.plan.allocation);
            }
        }

        // Working files through `ScratchDir` (ART-281); what must outlive the
        // test is copied to `out` below.
        let (_guard, scratch) = crate::core::ScratchDir::pair("art117-owner", "copy");
        let backup = scratch.join("caffeine-rdb-backup.bin");
        let report = run(
            EmbedTarget {
                image: &copy,
                slot,
                dostype: "PDS3",
                mode: EmbedMode::Replace,
                driver: &driver,
            },
            Some(&backup),
            &crate::core::jobs::NoProgress,
        )
        .unwrap();
        println!("report: {report:?}");

        let after = read_range(&copy, area.offset_bytes).unwrap();
        let hi = walk_strict(&after).unwrap().rdsk.rdb_blocks_hi as usize;
        // Into the owner's folder, never over a file already there.
        let keep = |name: &str, bytes: &[u8]| {
            let path = out.join(name);
            let mut file = std::fs::File::create_new(&path)
                .unwrap_or_else(|err| panic!("{}: {err}", path.display()));
            std::io::Write::write_all(&mut file, bytes).unwrap();
            path
        };
        let range_file = keep(
            "copy-rdb-after.bin",
            after
                .get(..(hi + 1) * BLOCK_SIZE)
                .expect("the walked range is inside what was read"),
        );
        let backup_kept = keep("caffeine-rdb-backup.bin", &std::fs::read(&backup).unwrap());
        println!("post-edit range: {}", range_file.display());
        println!("backup: {}", backup_kept.display());
        println!(
            "outside checks: hst.imager rdb info \"{}\"; rdbtool \"{}\" info; rdbtool \"{}\" fsget <n> <file> (rdbtool sums all 128 longwords)",
            copy.display(),
            range_file.display(),
            range_file.display()
        );

        assert_eq!(
            read_range(&original, area.offset_bytes).unwrap(),
            original_range,
            "the original's reserved range changed"
        );
        assert_eq!(
            std::fs::metadata(&original).unwrap().modified().unwrap(),
            original_mtime,
            "the original's mtime changed"
        );
    }
}
