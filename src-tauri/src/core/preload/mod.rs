//! Putting a filesystem, and content, onto a card's Amiga volumes
//! (SD-2 · G3, route E).
//!
//! SD-1 builds a card whose Amiga volumes are **not formatted** — an Amiga
//! sees the partitions and offers to format them. This is what formats them
//! from Windows and copies content in, so a card arrives ready.
//!
//! ## Two implementations of one trait
//!
//! [`native::NativeFormatter`] writes PFS3 through `libpfs3` and FFS through
//! ART's own `core/volume/write`, launching nothing (G5). Route E's
//! `hst-imager` is the other implementation, and it does not retire: it is
//! what `native`'s fixtures are checked against, the independent oracle a
//! reader and writer that only agree with each other cannot be
//! (`scripts/pfs3-oracle-check.py`).
//!
//! ## The trait, and why
//!
//! `core/` does not launch programs (CLAUDE.md). [`VolumeFormatter`] is the
//! boundary; `tools/hst_imager.rs` was the first implementation, needing a
//! test double so none of this needed the binary to be present.
//! [`native::NativeFormatter`] needs no double: it launches nothing, so it is
//! testable in CI exactly as written.
//!
//! ## What ART can check afterwards, and what it cannot
//!
//! Stopped being true the moment `native::NativeFormatter` started using
//! `libpfs3` itself (this module): ART now has *some* way to look inside a
//! PFS3 volume, because the library that writes it also reads it back.
//! `core/osinstall/verify.rs` is where that gets used for real, one field at
//! a time rather than as a blanket refusal — and it is exactly *because*
//! the reader and the writer are one and the same crate that it stops short
//! of `Pass`: presence, size and protection reach real `Fail` on a genuine
//! disagreement, but a PFS3 file's content is never re-hashed through the
//! same library that placed it (`verify.rs`'s own "Decision 2" explains
//! why), so a PFS3 record never reaches `Pass`, only `Fail` or `NotChecked`
//! with a reason. That is still weaker evidence than FFS gets from ART's own
//! independently-written reader, and weaker still than Task 11's
//! `hst-imager` oracle — but "not one file inside the volume" is no longer
//! the honest description of what this layer can do, and a claim the code
//! has outgrown is exactly the kind of thing this project keeps finding and
//! removing (ART-060 and others). The FAT32 side is unchanged: ART writes
//! that filesystem with `fatfs` and has no reader for it at all, so it stays
//! a blanket `not-checked` in G8's report.

pub mod amiga_names;
pub mod embed;
pub mod native;
#[cfg(test)]
pub(crate) mod pfs3_test_device;
pub mod pfs3dev;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::card::read_card;
use crate::core::error::{CoreError, CoreResult, RdbEditRefusal};
use crate::core::jobs::ProgressSink;
use crate::core::rdbedit::EditKind;
use embed::{DriverVersion, EmbedReport, EmbedTarget, Prepared};

/// What a formatter reports about itself.
///
/// Recorded rather than checked against a whitelist: ART's command set was
/// derived from 1.6.616's own scripts, and a different version is worth
/// saying rather than refusing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolVersion {
    pub raw: String,
}

/// How much a copy moved.
///
/// **`bytes` is optional, and `None` is not zero (ART-125).** ART's own
/// writer counts every byte it writes and always answers; `hst-imager` is
/// asked afterwards and reports a *rounded* total (`12.2 MB`), which is not a
/// byte count — deriving one from it would invent digits nothing measured. A
/// question ART cannot answer is left unanswered rather than answered with a
/// zero the screen then prints as a fact (§89, the same rule G8's
/// `not-checked` state follows).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopySummary {
    pub files: u64,
    pub directories: u64,
    pub bytes: Option<u64>,
    /// ART-116: how many entries carried a `.uaem` comment that could not be
    /// written. Always `0` on the FFS branch, whose own writer (`FileMeta`)
    /// does carry a comment through; only `native::copy_in_pfs3` ever
    /// increments this, because `libpfs3` 0.1.3 exposes no setter for one.
    /// Not a refusal — information the caller can choose to say something
    /// about, same as `dates_lost`.
    #[serde(default)]
    pub comments_lost: u64,
    /// The same, for a `.uaem` date. See `comments_lost`.
    #[serde(default)]
    pub dates_lost: u64,
}

impl Default for CopySummary {
    /// A copy that has moved nothing has moved a **known** zero bytes — the
    /// accumulators start here, so `None` can keep meaning "not answered"
    /// rather than doubling as "nothing yet".
    fn default() -> Self {
        Self {
            files: 0,
            directories: 0,
            bytes: Some(0),
            comments_lost: 0,
            dates_lost: 0,
        }
    }
}

impl CopySummary {
    /// Fold one step's counts into a running total.
    ///
    /// **Bytes survive only while every contributing step knew its own**
    /// (ART-125): one unanswered step makes the total unanswerable, which is
    /// the honest result — a sum missing an unknown addend is not a sum.
    pub fn absorb(&mut self, other: &CopySummary) {
        self.files += other.files;
        self.directories += other.directories;
        self.comments_lost += other.comments_lost;
        self.dates_lost += other.dates_lost;
        self.bytes = match (self.bytes, other.bytes) {
            (Some(mine), Some(theirs)) => Some(mine + theirs),
            _ => None,
        };
    }
}

/// Formatting an Amiga volume and putting files in it.
///
/// Outside `core` because every method runs a program. The trait is here so
/// the planning above it, and the tests below, need no binary.
pub trait VolumeFormatter {
    fn probe(&self) -> CoreResult<ToolVersion>;

    /// Format one partition. **Destructive**: whatever was in it is gone.
    fn format_partition(
        &self,
        image: &Path,
        slot: Option<usize>,
        index: usize,
        volume: &str,
        sink: &dyn ProgressSink,
    ) -> CoreResult<()>;

    /// Whether this formatter could run [`copy_in`](Self::copy_in) for
    /// `source` — asked **before** the partition is formatted, and answering
    /// with the same `CoreError` `copy_in` itself would refuse with.
    ///
    /// **Why this exists (ART-122).** A partition must be formatted and
    /// filled by *one* implementation: `hst-imager`'s first write into a
    /// volume `NativeFormatter` formatted dies `ERROR_DISK_FULL`, because the
    /// two size the PFS3 reserved area differently (ART's number is
    /// `pfs3aio`'s own, computed by its own algorithm). So the caller has to
    /// know which tool will do the *copy* before it runs the *format*, and
    /// asking by attempting the copy is not available — the format has to
    /// come first. This is that question, and it writes nothing.
    ///
    /// The default is `Ok(())`: a formatter that has not stated a limitation
    /// is taken to have none, which is right for `hst-imager` (it is the
    /// fallback precisely because it can do what the native path cannot).
    fn can_copy_in(
        &self,
        _image: &Path,
        _slot: Option<usize>,
        _drive: &str,
        _source: &Path,
    ) -> CoreResult<()> {
        Ok(())
    }

    /// Copy a tree from the PC into a partition.
    fn copy_in(
        &self,
        image: &Path,
        slot: Option<usize>,
        drive: &str,
        source: &Path,
        sink: &dyn ProgressSink,
    ) -> CoreResult<CopySummary>;
}

/// One partition to prepare.
///
/// **Two numbers, and both are needed.** A card is a list of Amiga disks and
/// each carries its own RDB, so "partition 1" means nothing until you say
/// which disk. Flattening them into one list is the mistake `read_card`
/// exists to stop callers making (ART-095).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreloadPartition {
    /// Which Amiga disk on the card, **from one**. A plain HDF has one.
    pub area: usize,
    /// Which partition inside that disk's own RDB, **from one** — the way the
    /// disk numbers them, not the way a flattened list would.
    pub index: usize,
    /// The volume name it gets. `Work`, `Games`.
    pub volume_name: String,
    /// A folder on the PC whose tree goes in. `None` formats and stops.
    pub content: Option<PathBuf>,
}

/// What a screen asks for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreloadRequest {
    pub image: PathBuf,
    /// A filesystem driver to embed first when the card carries none for the
    /// DosType its partitions name, or to replace the card's own with when it
    /// is a newer copy of the same driver (ART-117).
    pub driver: Option<PathBuf>,
    pub partitions: Vec<PreloadPartition>,
    /// Where the RDB area is copied before an embed or replace step writes a
    /// block (ART-117, decision 7). Chosen for each run, never remembered: the
    /// next run would meet the file this one made.
    #[serde(default)]
    pub rdb_backup: Option<PathBuf>,
}

/// One thing that will be run, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "kebab-case")]
pub enum PreloadStep {
    /// Append a driver for a DosType the card does not carry (ART-117).
    ImportFilesystem {
        /// Which MBR slot holds the Amiga disk, or `None` for a plain image
        /// whose RDB is at offset zero. **This is what a card needs and a
        /// hard-disk image does not**: an external tool looking at byte zero
        /// of a card finds a partition table, not an RDB, and says so — which
        /// is how this field came to exist.
        slot: Option<usize>,
        driver: PathBuf,
        dostype: String,
        name: String,
        /// What the driver's own `$VER:` says.
        file_version: DriverVersion,
        /// `[first, last]` RDB blocks the edit writes.
        blocks: [u32; 2],
        /// `[before, after]` when `RDBBlocksHi` is raised (decision 12).
        rdb_blocks_hi_raised: Option<[u32; 2]>,
    },
    /// Replace the card's driver with a newer one (ART-117, decision 1).
    ReplaceFilesystem {
        slot: Option<usize>,
        driver: PathBuf,
        dostype: String,
        name: String,
        card_version: DriverVersion,
        file_version: DriverVersion,
        blocks: [u32; 2],
        rdb_blocks_hi_raised: Option<[u32; 2]>,
    },
    /// **Destructive.** Named as its own step so a preview can say so.
    FormatPartition {
        slot: Option<usize>,
        index: usize,
        drive_name: String,
        volume_name: String,
    },
    CopyIn {
        slot: Option<usize>,
        drive_name: String,
        source: PathBuf,
    },
}

impl PreloadStep {
    /// The RDB edit this step is, if it is one.
    pub fn embed_target<'a>(&'a self, image: &'a Path) -> Option<EmbedTarget<'a>> {
        match self {
            Self::ImportFilesystem {
                slot,
                driver,
                dostype,
                ..
            } => Some(EmbedTarget {
                image,
                slot: *slot,
                dostype,
                kind: EditKind::Append,
                driver,
            }),
            Self::ReplaceFilesystem {
                slot,
                driver,
                dostype,
                ..
            } => Some(EmbedTarget {
                image,
                slot: *slot,
                dostype,
                kind: EditKind::Replace,
                driver,
            }),
            Self::FormatPartition { .. } | Self::CopyIn { .. } => None,
        }
    }
}

/// Something the preview must say that is not a step (ART-117). A value, never
/// a sentence (ART-060): `src/lib/preload.ts::notePhrase` renders it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "note", rename_all = "kebab-case")]
pub enum PlanNote {
    /// The card's driver is not older than the chosen file's.
    DriverKept {
        dostype: String,
        card_version: DriverVersion,
        file_version: Option<DriverVersion>,
    },
    /// A replace ART refused; the card's own driver stays and still mounts.
    ReplaceRefused {
        dostype: String,
        card_version: DriverVersion,
        code: String,
        detail: String,
    },
    /// The card's driver and the chosen file name different programs, or
    /// either side's `$VER:` names none (spec decision 13). hst-imager can
    /// replace it.
    DifferentDriver {
        dostype: String,
        card_version: DriverVersion,
        card_name: Option<String>,
        file_name: Option<String>,
    },
    /// Another DosType's driver was not looked at: one RDB edit per run.
    SecondEditSkipped { dostype: String },
}

/// What a preload would do, before any of it is done (§92's PREVIEW).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreloadPlan {
    pub image: PathBuf,
    pub steps: Vec<PreloadStep>,
    #[serde(default)]
    pub notes: Vec<PlanNote>,
    /// The request's backup path, echoed so the run needs nothing else.
    #[serde(default)]
    pub rdb_backup: Option<PathBuf>,
}

impl PreloadPlan {
    /// How many partitions this would erase.
    pub fn formats(&self) -> usize {
        self.steps
            .iter()
            .filter(|step| matches!(step, PreloadStep::FormatPartition { .. }))
            .count()
    }

    /// Whether this plan writes into an RDB.
    pub fn edits_rdb(&self) -> bool {
        self.steps
            .iter()
            .any(|step| step.embed_target(&self.image).is_some())
    }

    /// Decision 6's run-time refusals, asked before a job starts and again by
    /// `run`, so the engine does not depend on a screen having asked.
    pub fn ready_to_run(&self) -> CoreResult<()> {
        if !self.edits_rdb() {
            return Ok(());
        }
        let Some(backup) = &self.rdb_backup else {
            return Err(CoreError::RdbEditRefused(RdbEditRefusal::NoBackup));
        };
        if backup.exists() {
            return Err(CoreError::RdbEditRefused(RdbEditRefusal::BackupExists {
                path: backup.clone(),
            }));
        }
        Ok(())
    }
}

/// Work out what a preload would run, and refuse what cannot work.
///
/// Reads the card rather than trusting the request: a partition index, a
/// DosType and whether a driver is already there are all facts about the
/// image.
pub fn plan(request: &PreloadRequest) -> CoreResult<PreloadPlan> {
    if request.partitions.is_empty() {
        return Err(CoreError::InvalidInput(
            "choose at least one partition to prepare".into(),
        ));
    }

    let card = read_card(&request.image)?;

    // Every chosen partition checked first — its disk, its index, its content
    // folder — so nothing below is planned for one that is not there.
    let mut chosen = Vec::with_capacity(request.partitions.len());
    for wanted in &request.partitions {
        let Some(area) = wanted
            .area
            .checked_sub(1)
            .and_then(|index| card.areas.get(index))
        else {
            return Err(CoreError::InvalidInput(format!(
                "this image has {} Amiga disk(s); there is no disk {}",
                card.areas.len(),
                wanted.area
            )));
        };

        let Some(part) = wanted
            .index
            .checked_sub(1)
            .and_then(|index| area.rdb.partitions.get(index))
        else {
            return Err(CoreError::InvalidInput(format!(
                "Amiga disk {} has {} partition(s); there is no partition {}",
                wanted.area,
                area.rdb.partitions.len(),
                wanted.index
            )));
        };

        if let Some(content) = &wanted.content {
            if !content.is_dir() {
                return Err(CoreError::InvalidInput(format!(
                    "'{}' is not a folder",
                    content.display()
                )));
            }
        }

        // Which MBR slot the disk sits in, so a tool can be pointed at the
        // RDB *inside* the card rather than at byte zero.
        chosen.push((wanted, part, mbr_slot_of(&card, wanted.area - 1)));
    }

    // One RDB edit per run (decision 1): the DosType it is for, once made.
    let mut edited: Option<u32> = None;
    // **Every RDB edit step goes before every format and copy** (final review
    // M2): the edit is refused, validated and backed up while nothing on the
    // card has been erased yet — "never destroy the original before
    // successful validation".
    let mut edits = Vec::new();
    let mut notes = Vec::new();

    // **Pass 1 — the appends, ART-084 as a gate one more time.** Formatting a
    // `PDS` partition on a card that carries no PFS3 driver produces a volume
    // an Amiga ignores in silence. An append is required and a replace is
    // not, so the appends are planned first: a replace can never take the one
    // edit a later partition needs (final review M1).
    for (wanted, part, slot) in &chosen {
        if card.provides_file_system(part.dostype) || kickstart_carries(part.dostype) {
            continue;
        }
        match (&request.driver, edited) {
            (Some(driver), None) => {
                let ready =
                    embed::prepare_append(&request.image, *slot, &part.dostype_str, driver)?;
                edits.push(embed_step(*slot, driver, &part.dostype_str, &ready));
                edited = Some(part.dostype);
            }
            (Some(_), Some(done)) if done == part.dostype => {}
            (Some(_), Some(done)) => {
                return Err(CoreError::InvalidInput(format!(
                    concat!(
                        "partition {} is {} and nothing on this card provides it, but this ",
                        "run already embeds a {} driver — ART makes one RDB edit per run, so ",
                        "prepare this partition in a second run"
                    ),
                    wanted.index,
                    part.dostype_str,
                    crate::core::rdb::dos_type_string(done)
                )));
            }
            (None, _) => {
                return Err(CoreError::InvalidInput(format!(
                    concat!(
                        "partition {} is {} and nothing on this card provides it — ",
                        "supply the filesystem driver, or an Amiga will ignore ",
                        "the volume without saying why"
                    ),
                    wanted.index, part.dostype_str
                )));
            }
        }
    }

    // **Pass 2 — the replaces, each one optional.** The card's own driver
    // still mounts, so every way a replace cannot go ahead is a note.
    if let Some(driver) = &request.driver {
        // DosTypes already asked about, so two partitions of one DosType
        // produce one note, not two.
        let mut considered: Vec<u32> = Vec::new();
        for (_, part, _) in &chosen {
            if !card.provides_file_system(part.dostype) || considered.contains(&part.dostype) {
                continue;
            }
            considered.push(part.dostype);
            if edited.is_some() {
                notes.push(PlanNote::SecondEditSkipped {
                    dostype: part.dostype_str.clone(),
                });
                continue;
            }
            // The first area carrying the DosType — the record the gate above
            // read (decision 1).
            let target_slot = card
                .areas
                .iter()
                .position(|area| area.rdb.provides_file_system(part.dostype))
                .and_then(|index| mbr_slot_of(&card, index));
            let card_version = card
                .file_systems()
                .iter()
                .find(|fs| fs.dos_type == part.dostype)
                .map(|fs| DriverVersion {
                    version: fs.version,
                    revision: fs.revision,
                })
                .unwrap_or(DriverVersion {
                    version: 0,
                    revision: 0,
                });
            match embed::prepare_replace(&request.image, target_slot, &part.dostype_str, driver) {
                Ok(Prepared::Edit(ready)) => {
                    edits.push(embed_step(target_slot, driver, &part.dostype_str, &ready));
                    edited = Some(part.dostype);
                }
                Ok(Prepared::Keep { card, file }) => notes.push(PlanNote::DriverKept {
                    dostype: part.dostype_str.clone(),
                    card_version: card,
                    file_version: file,
                }),
                // Spec decision 13: a different program has a note of its
                // own, naming both.
                Err(CoreError::RdbEditRefused(RdbEditRefusal::DifferentDriver { card, file })) => {
                    notes.push(PlanNote::DifferentDriver {
                        dostype: part.dostype_str.clone(),
                        card_version,
                        card_name: card,
                        file_name: file,
                    })
                }
                // Spec decision 6 (pre-flight F5): every other way a replace
                // cannot go ahead is a note too — a refusal, a remembered
                // driver file that has gone, an area ART cannot read. The
                // card's own driver still mounts, and a remembered path is not
                // intent.
                Err(err) => notes.push(PlanNote::ReplaceRefused {
                    dostype: part.dostype_str.clone(),
                    card_version,
                    code: err.code().to_string(),
                    detail: err.to_string(),
                }),
            }
        }
    }

    // **Pass 3 — the destructive steps**, in the order the request named the
    // partitions, each copy after its own format.
    let mut steps = edits;
    for (wanted, part, slot) in &chosen {
        steps.push(PreloadStep::FormatPartition {
            slot: *slot,
            index: wanted.index,
            drive_name: part.drive_name.clone(),
            volume_name: wanted.volume_name.clone(),
        });
        if let Some(content) = &wanted.content {
            steps.push(PreloadStep::CopyIn {
                slot: *slot,
                drive_name: part.drive_name.clone(),
                source: content.clone(),
            });
        }
    }

    Ok(PreloadPlan {
        image: request.image.clone(),
        steps,
        notes,
        rdb_backup: request.rdb_backup.clone(),
    })
}

/// The step a planned edit shows: its versions, its blocks, and a raise.
fn embed_step(
    slot: Option<usize>,
    driver: &Path,
    dostype: &str,
    ready: &embed::EditReady,
) -> PreloadStep {
    let allocation = &ready.plan.allocation;
    let blocks = [allocation.fshd_block, allocation.last_block];
    let rdb_blocks_hi_raised = allocation
        .raised_from
        .map(|from| [from, allocation.rdb_blocks_hi]);
    let file_version = DriverVersion::from(ready.plan.file_version);
    match ready.plan.card_version {
        Some(card) => PreloadStep::ReplaceFilesystem {
            slot,
            driver: driver.to_path_buf(),
            dostype: dostype.to_string(),
            name: driver_name(driver),
            card_version: card.into(),
            file_version,
            blocks,
            rdb_blocks_hi_raised,
        },
        None => PreloadStep::ImportFilesystem {
            slot,
            driver: driver.to_path_buf(),
            dostype: dostype.to_string(),
            name: driver_name(driver),
            file_version,
            blocks,
            rdb_blocks_hi_raised,
        },
    }
}

/// What a finished preload did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreloadOutcome {
    pub formatted: Vec<String>,
    pub copied: CopySummary,
    /// What the formatter reported itself to be, for the manifest and the log.
    pub tool: Option<ToolVersion>,
    /// The RDB edit, when the plan made one (ART-117).
    #[serde(default)]
    pub embedded: Option<EmbedReport>,
}

/// A run that stopped before it finished, and what it had already done
/// (final review I3).
///
/// A format or a copy that fails, or a cancel, after the RDB edit committed
/// must not lose the edit: `outcome.embedded` is what the card now holds, and
/// names the backup. `outcome.formatted` likewise names what was erased.
#[derive(Debug)]
pub struct RunStopped {
    pub error: CoreError,
    pub outcome: PreloadOutcome,
}

impl std::fmt::Display for RunStopped {
    /// The error the run stopped with.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

impl From<CoreError> for RunStopped {
    /// A stop before any step ran: nothing done.
    fn from(error: CoreError) -> Self {
        Self {
            error,
            outcome: PreloadOutcome::default(),
        }
    }
}

/// Run a plan.
///
/// **Cancellation is checked between whole steps and never inside one**
/// (§54): a format that has begun is finished, because a partition abandoned
/// halfway is worse than one formatted. Every step reports through the sink,
/// so a copy of a hundred thousand files does not look like a hang.
pub fn run(
    plan: &PreloadPlan,
    formatter: &dyn VolumeFormatter,
    sink: &dyn ProgressSink,
) -> Result<PreloadOutcome, Box<RunStopped>> {
    plan.ready_to_run()
        .map_err(|error| Box::new(RunStopped::from(error)))?;
    let mut outcome = PreloadOutcome {
        tool: formatter.probe().ok(),
        ..Default::default()
    };
    match run_steps(plan, formatter, sink, &mut outcome) {
        Ok(()) => Ok(outcome),
        // Boxed: an error and a whole outcome are too large to return by value
        // beside a success (clippy::result_large_err).
        Err(error) => Err(Box::new(RunStopped { error, outcome })),
    }
}

/// [`run`]'s steps, each finished one folded into `outcome` as it goes, so a
/// stop still has what was done.
fn run_steps(
    plan: &PreloadPlan,
    formatter: &dyn VolumeFormatter,
    sink: &dyn ProgressSink,
    outcome: &mut PreloadOutcome,
) -> CoreResult<()> {
    let total = plan.steps.len() as u64;
    for (done, step) in plan.steps.iter().enumerate() {
        if sink.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        sink.report(done as u64, Some(total), &step_label(step));

        match step {
            PreloadStep::ImportFilesystem { .. } | PreloadStep::ReplaceFilesystem { .. } => {
                if let Some(target) = step.embed_target(&plan.image) {
                    outcome.embedded = Some(embed::run(target, plan.rdb_backup.as_deref(), sink)?);
                }
            }
            PreloadStep::FormatPartition {
                slot,
                index,
                drive_name,
                volume_name,
            } => {
                formatter.format_partition(&plan.image, *slot, *index, volume_name, sink)?;
                outcome.formatted.push(drive_name.clone());
            }
            PreloadStep::CopyIn {
                slot,
                drive_name,
                source,
            } => {
                let summary = formatter.copy_in(&plan.image, *slot, drive_name, source, sink)?;
                outcome.copied.absorb(&summary);
            }
        }
    }

    sink.report(total, Some(total), "done");
    Ok(())
}

/// `pub(crate)`: `commands/preload.rs::run_with_fallback` reuses this for its
/// own per-step progress line rather than duplicating the match — the choice
/// of *which tool* runs a step is a `commands/` concern (CLAUDE.md), but the
/// step's own label is not.
pub(crate) fn step_label(step: &PreloadStep) -> String {
    match step {
        PreloadStep::ImportFilesystem { name, .. } => format!("Embedding {name}"),
        PreloadStep::ReplaceFilesystem { name, .. } => format!("Replacing the driver with {name}"),
        PreloadStep::FormatPartition {
            drive_name,
            volume_name,
            ..
        } => format!("Formatting {drive_name} as {volume_name}"),
        PreloadStep::CopyIn { drive_name, .. } => format!("Copying into {drive_name}"),
    }
}

/// Which MBR slot the `n`th Amiga disk occupies.
///
/// `None` for a plain hard-disk image, which has no partition table and whose
/// RDB is simply at the start.
fn mbr_slot_of(card: &crate::core::card::CardImage, area_index: usize) -> Option<usize> {
    card.mbr
        .as_ref()?
        .amiga_areas()
        .get(area_index)
        .map(|part| part.slot_number())
}

/// DosTypes Kickstart mounts itself, which need nothing in the RDB.
fn kickstart_carries(dostype: u32) -> bool {
    dostype & 0xFFFF_FF00 == 0x444F_5300 && (dostype & 0xFF) <= 7
}

/// What the driver will be called in the RDB. Its own file name, lowercased
/// and without an extension — `pfs3aio.lha` becomes `pfs3aio`.
fn driver_name(driver: &Path) -> String {
    driver
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| "filesystem".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::preload::embed::DriverVersion;
    use crate::core::rdb::{AmigaHardDiskFs, FileSystemSpec, PartitionSpec};
    use crate::core::rdbedit::fixtures::{hunk_driver, named_driver};

    fn scratch(tag: &str) -> (crate::core::ScratchDir, PathBuf) {
        crate::core::ScratchDir::pair("art-preload", tag)
    }

    /// A card with one partition of `fs`, and no filesystem driver in its RDB.
    fn card(dir: &Path, fs: AmigaHardDiskFs) -> PathBuf {
        let path = dir.join("card.hdf");
        crate::core::hdf::create_hdf(
            &path,
            32 * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: fs,
                size_mb: 10,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[],
        )
        .unwrap();
        path
    }

    fn one(index: usize, volume: &str, content: Option<PathBuf>) -> Vec<PreloadPartition> {
        vec![PreloadPartition {
            area: 1,
            index,
            volume_name: volume.into(),
            content,
        }]
    }

    fn driver(dir: &Path, version: &str) -> PathBuf {
        let path = dir.join("pfs3aio");
        std::fs::write(&path, hunk_driver(62_604, version)).unwrap();
        path
    }

    fn lha(dir: &Path) -> PathBuf {
        let path = dir.join("pfs3aio.lha");
        let mut bytes = vec![0u8; 1024];
        bytes[..7].copy_from_slice(b"\x2a\x00-lh5-");
        std::fs::write(&path, bytes).unwrap();
        path
    }

    /// A card ART built with a `PDS\3` partition and that driver in its RDB
    /// (`RDBBlocksHi` 130).
    fn card_with_pds3_driver(dir: &Path, version: (u16, u16)) -> PathBuf {
        let path = dir.join("card.hdf");
        crate::core::hdf::create_hdf(
            &path,
            32 * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::Pfs3DirectScsi,
                size_mb: 10,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[FileSystemSpec {
                dos_type: 0x5044_5303,
                version: version.0,
                revision: version.1,
                data: hunk_driver(62_604, &format!("{}.{}", version.0, version.1)),
            }],
        )
        .unwrap();
        path
    }

    fn request(image: PathBuf, driver: Option<PathBuf>) -> PreloadRequest {
        PreloadRequest {
            image,
            driver,
            partitions: one(1, "Work", None),
            rdb_backup: None,
        }
    }

    /// Decision 1: a newer file replaces the card's driver, and the step says
    /// both versions and the blocks — here with decision 12's raise.
    #[test]
    fn a_newer_driver_replaces_the_cards_own_before_the_format() {
        let (_guard, dir) = scratch("replace");
        let image = card_with_pds3_driver(&dir, (19, 2));
        let file = driver(&dir, "19.3");
        let made = plan(&request(image, Some(file.clone()))).unwrap();
        assert_eq!(
            made.steps[0],
            PreloadStep::ReplaceFilesystem {
                slot: None,
                driver: file,
                dostype: "PDS3".into(),
                name: "pfs3aio".into(),
                card_version: DriverVersion {
                    version: 19,
                    revision: 2
                },
                file_version: DriverVersion {
                    version: 19,
                    revision: 3
                },
                blocks: [131, 259],
                rdb_blocks_hi_raised: Some([130, 259]),
            }
        );
        assert!(matches!(made.steps[1], PreloadStep::FormatPartition { .. }));
        assert!(made.notes.is_empty(), "{:?}", made.notes);
    }

    /// "An ignored driver is never silent."
    #[test]
    fn a_driver_that_is_not_newer_plans_no_edit_and_says_so() {
        let (_guard, dir) = scratch("keep");
        let image = card_with_pds3_driver(&dir, (19, 2));
        let made = plan(&request(image, Some(driver(&dir, "19.2")))).unwrap();
        assert!(
            matches!(made.steps.as_slice(), [PreloadStep::FormatPartition { .. }]),
            "{:?}",
            made.steps
        );
        assert_eq!(
            made.notes,
            vec![PlanNote::DriverKept {
                dostype: "PDS3".into(),
                card_version: DriverVersion {
                    version: 19,
                    revision: 2
                },
                file_version: Some(DriverVersion {
                    version: 19,
                    revision: 2
                }),
            }]
        );
    }

    /// Decision 6: a replace refusal is a note — the card's driver still mounts.
    #[test]
    fn a_replace_the_card_cannot_take_is_a_note_and_the_format_still_plans() {
        let (_guard, dir) = scratch("replace-refused");
        let image = card_with_pds3_driver(&dir, (19, 2));
        let made = plan(&request(image, Some(lha(&dir)))).unwrap();
        assert!(matches!(
            made.steps.as_slice(),
            [PreloadStep::FormatPartition { .. }]
        ));
        match made.notes.as_slice() {
            [PlanNote::ReplaceRefused {
                dostype,
                code,
                detail,
                ..
            }] => {
                assert_eq!(
                    (dostype.as_str(), code.as_str()),
                    ("PDS3", "ART-RDB-EDIT-NOT-EXECUTABLE")
                );
                assert!(detail.contains("unpacked first"), "{detail}");
            }
            other => panic!("{other:?}"),
        }
    }

    /// Spec decision 13: a different program on the card is a note naming
    /// both, and the format still plans.
    #[test]
    fn a_replace_across_different_drivers_is_a_note_naming_both() {
        let (_guard, dir) = scratch("different-driver");
        let image = dir.join("card.hdf");
        crate::core::hdf::create_hdf(
            &image,
            32 * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::Pfs3DirectScsi,
                size_mb: 10,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[FileSystemSpec {
                dos_type: 0x5044_5303,
                version: 1,
                revision: 293,
                data: named_driver(62_604, "SmartFilesystem", "1.293"),
            }],
        )
        .unwrap();
        let made = plan(&request(image, Some(driver(&dir, "19.3")))).unwrap();
        assert!(
            matches!(made.steps.as_slice(), [PreloadStep::FormatPartition { .. }]),
            "{:?}",
            made.steps
        );
        assert_eq!(
            made.notes,
            vec![PlanNote::DifferentDriver {
                dostype: "PDS3".into(),
                card_version: DriverVersion {
                    version: 1,
                    revision: 293
                },
                card_name: Some("SmartFilesystem".into()),
                file_name: Some("pfs3aio".into()),
            }]
        );
    }

    /// Spec decision 6 (pre-flight F5): a remembered driver file that has gone
    /// is a note, not a failed preview, on a card that already carries its
    /// driver.
    #[test]
    fn a_missing_remembered_driver_file_is_a_note_and_the_plan_still_formats() {
        let (_guard, dir) = scratch("driver-gone");
        let image = card_with_pds3_driver(&dir, (19, 2));
        let made = plan(&request(image, Some(dir.join("gone")))).unwrap();
        assert!(
            matches!(made.steps.as_slice(), [PreloadStep::FormatPartition { .. }]),
            "{:?}",
            made.steps
        );
        match made.notes.as_slice() {
            [PlanNote::ReplaceRefused { code, .. }] => assert_eq!(code, "ART-IO"),
            other => panic!("{other:?}"),
        }
    }

    /// Decision 6: an append refusal fails the plan — formatting on would make
    /// a volume an Amiga ignores.
    #[test]
    fn an_append_the_card_cannot_take_fails_the_plan() {
        let (_guard, dir) = scratch("append-refused");
        let image = card(&dir, AmigaHardDiskFs::Pfs3Standard);
        let err = plan(&request(image, Some(lha(&dir)))).unwrap_err();
        assert_eq!(err.code(), "ART-RDB-EDIT-NOT-EXECUTABLE", "{err}");
    }

    #[test]
    fn a_second_append_for_another_dostype_fails_the_plan_by_name() {
        let (_guard, dir) = scratch("second-append");
        let image = dir.join("two.hdf");
        let part = |name: &str, fs| PartitionSpec {
            drive_name: name.into(),
            fs_type: fs,
            size_mb: 10,
            bootable: false,
            boot_priority: 0,
            num_buffers: 0,
        };
        crate::core::hdf::create_hdf(
            &image,
            64 * 1024 * 1024,
            true,
            &[
                part("DH0", AmigaHardDiskFs::Pfs3Standard),
                part("DH1", AmigaHardDiskFs::Pfs3DirectScsi),
            ],
            &[],
        )
        .unwrap();
        let mut asked = request(image, Some(driver(&dir, "19.3")));
        asked.partitions = vec![
            PreloadPartition {
                area: 1,
                index: 1,
                volume_name: "Work".into(),
                content: None,
            },
            PreloadPartition {
                area: 1,
                index: 2,
                volume_name: "Games".into(),
                content: None,
            },
        ];
        let err = plan(&asked).unwrap_err();
        assert_eq!(err.code(), "ART-INPUT-INVALID");
        assert!(err.to_string().contains("one RDB edit per run"), "{err}");
    }

    /// A card with `DH0` of `first` and `DH1` of `second`, and a PDS3 19.2
    /// driver in its RDB when `pds3_driver`.
    fn two_partition_card(
        dir: &Path,
        first: AmigaHardDiskFs,
        second: AmigaHardDiskFs,
        pds3_driver: bool,
    ) -> PathBuf {
        let path = dir.join("two.hdf");
        let part = |name: &str, fs| PartitionSpec {
            drive_name: name.into(),
            fs_type: fs,
            size_mb: 10,
            bootable: false,
            boot_priority: 0,
            num_buffers: 0,
        };
        let drivers: Vec<FileSystemSpec> = if pds3_driver {
            vec![FileSystemSpec {
                dos_type: 0x5044_5303,
                version: 19,
                revision: 2,
                data: hunk_driver(62_604, "19.2"),
            }]
        } else {
            Vec::new()
        };
        crate::core::hdf::create_hdf(
            &path,
            64 * 1024 * 1024,
            true,
            &[part("DH0", first), part("DH1", second)],
            &drivers,
        )
        .unwrap();
        path
    }

    /// Both partitions of a two-partition card, in `order`.
    fn both(order: [usize; 2]) -> Vec<PreloadPartition> {
        order
            .iter()
            .map(|&index| PreloadPartition {
                area: 1,
                index,
                volume_name: format!("V{index}"),
                content: None,
            })
            .collect()
    }

    fn step_kinds(made: &PreloadPlan) -> Vec<&'static str> {
        made.steps
            .iter()
            .map(|step| match step {
                PreloadStep::ImportFilesystem { .. } => "import",
                PreloadStep::ReplaceFilesystem { .. } => "replace",
                PreloadStep::FormatPartition { .. } => "format",
                PreloadStep::CopyIn { .. } => "copy",
            })
            .collect()
    }

    /// **Final review M1.** A replace is optional — the card's own driver
    /// still mounts, and a remembered driver is not intent (decision 6) — and
    /// an append is required. So the append is planned and the replace is the
    /// note, whichever partition the request names first.
    #[test]
    fn an_optional_replace_never_fails_a_required_append_in_either_order() {
        let (_guard, dir) = scratch("append-before-replace");
        let image = two_partition_card(
            &dir,
            AmigaHardDiskFs::Pfs3DirectScsi,
            AmigaHardDiskFs::Pfs3Standard,
            true,
        );
        let file = driver(&dir, "19.3");
        for order in [[1, 2], [2, 1]] {
            let mut asked = request(image.clone(), Some(file.clone()));
            asked.partitions = both(order);
            let made = plan(&asked).unwrap_or_else(|err| panic!("{order:?}: {err}"));
            assert_eq!(
                step_kinds(&made),
                vec!["import", "format", "format"],
                "{order:?}: {:?}",
                made.steps
            );
            assert!(
                matches!(&made.steps[0], PreloadStep::ImportFilesystem { dostype, .. } if dostype == "PFS3"),
                "{order:?}: {:?}",
                made.steps
            );
            assert_eq!(
                made.notes,
                vec![PlanNote::SecondEditSkipped {
                    dostype: "PDS3".into()
                }],
                "{order:?}"
            );
        }
    }

    /// **Final review M2.** Every RDB edit is planned before any format or
    /// copy, whichever partition it is for: it is validated and backed up
    /// before anything on the card is erased.
    #[test]
    fn every_rdb_edit_is_planned_before_the_first_format() {
        let (_guard, dir) = scratch("edits-first");
        let image = two_partition_card(
            &dir,
            AmigaHardDiskFs::FfsStandard,
            AmigaHardDiskFs::Pfs3Standard,
            false,
        );
        let tree = dir.join("tree");
        std::fs::create_dir_all(&tree).unwrap();
        let mut asked = request(image, Some(driver(&dir, "19.3")));
        asked.partitions = both([1, 2]);
        asked.partitions[0].content = Some(tree);
        let made = plan(&asked).unwrap();
        assert_eq!(
            step_kinds(&made),
            vec!["import", "format", "copy", "format"],
            "{:?}",
            made.steps
        );
    }

    /// M2 where it matters: an edit that fails at run time — its backup
    /// cannot be written — stops the run before a partition is erased.
    #[test]
    fn an_rdb_edit_that_fails_at_run_time_erases_nothing() {
        let (_guard, dir) = scratch("edit-fails-first");
        let image = two_partition_card(
            &dir,
            AmigaHardDiskFs::FfsStandard,
            AmigaHardDiskFs::Pfs3Standard,
            false,
        );
        let mut asked = request(image, Some(driver(&dir, "19.3")));
        asked.partitions = both([1, 2]);
        asked.rdb_backup = Some(dir.join("no-such-folder").join("rdb.bin"));
        let made = plan(&asked).unwrap();

        let recorder = Recorder::default();
        let err = run(&made, &recorder, &crate::core::jobs::NoProgress)
            .unwrap_err()
            .error;
        assert_eq!(err.code(), "ART-RDB-BACKUP-FAILED", "{err}");
        assert!(
            recorder.calls.borrow().is_empty(),
            "nothing may be formatted before the edit: {:?}",
            recorder.calls.borrow()
        );
    }

    /// A PFS3 card with no driver, a 19.3 driver file, and a plan that embeds
    /// it and formats DH0 with the backup at the returned path.
    fn embed_then_format(tag: &str) -> (crate::core::ScratchDir, PathBuf, PathBuf, PreloadPlan) {
        let (guard, dir) = scratch(tag);
        let image = card(&dir, AmigaHardDiskFs::Pfs3Standard);
        let backup = dir.join("rdb.bin");
        let mut asked = request(image.clone(), Some(driver(&dir, "19.3")));
        asked.rdb_backup = Some(backup.clone());
        let made = plan(&asked).unwrap();
        (guard, image, backup, made)
    }

    /// **Final review I3.** A format that fails after the RDB edit committed
    /// does not lose the edit: the stop still carries what the card holds and
    /// where its backup is.
    #[test]
    fn a_format_that_fails_after_the_edit_still_reports_the_edit() {
        let (_guard, image, backup, made) = embed_then_format("stop-after-embed");
        let recorder = Recorder {
            fail_at: Some(1),
            ..Default::default()
        };
        let stopped = run(&made, &recorder, &crate::core::jobs::NoProgress).unwrap_err();
        assert_eq!(
            stopped.error.code(),
            "ART-FORMAT-MALFORMED",
            "{}",
            stopped.error
        );
        let embedded = stopped
            .outcome
            .embedded
            .expect("the edit the card holds is still reported");
        assert_eq!(
            (embedded.dostype.as_str(), embedded.backup),
            ("PFS3", backup)
        );
        assert!(read_card(&image).unwrap().provides_file_system(0x5046_5303));
    }

    /// Says "not cancelled" until it has been asked `from` times.
    struct CancelFrom {
        asked: std::sync::atomic::AtomicUsize,
        from: usize,
    }

    impl ProgressSink for CancelFrom {
        fn report(&self, _done: u64, _total: Option<u64>, _message: &str) {}
        fn is_cancelled(&self) -> bool {
            self.asked
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                + 1
                >= self.from
        }
    }

    /// I3's other ending: a cancel between the edit and the format.
    #[test]
    fn a_cancel_after_the_edit_still_reports_the_edit() {
        let (_guard, _image, backup, made) = embed_then_format("cancel-after-embed");
        let recorder = Recorder::default();
        let sink = CancelFrom {
            asked: std::sync::atomic::AtomicUsize::new(0),
            from: 2,
        };
        let stopped = run(&made, &recorder, &sink).unwrap_err();
        assert!(
            matches!(stopped.error, CoreError::Cancelled),
            "{:?}",
            stopped.error
        );
        assert!(
            recorder.calls.borrow().is_empty(),
            "the cancel came before the format"
        );
        assert_eq!(
            stopped.outcome.embedded.map(|e| e.backup),
            Some(backup),
            "a cancel after the edit still reports it"
        );
    }

    #[test]
    fn ready_to_run_asks_for_a_new_backup_file_only_when_the_plan_edits_the_rdb() {
        let (_guard, dir) = scratch("ready");
        let image = card(&dir, AmigaHardDiskFs::Pfs3Standard);
        let mut made = plan(&request(image, Some(driver(&dir, "19.3")))).unwrap();
        assert!(made.edits_rdb());

        let err = made.ready_to_run().unwrap_err();
        assert_eq!(err.code(), "ART-RDB-EDIT-NO-BACKUP");

        let taken = dir.join("taken.bin");
        std::fs::write(&taken, b"x").unwrap();
        made.rdb_backup = Some(taken);
        assert_eq!(
            made.ready_to_run().unwrap_err().code(),
            "ART-RDB-EDIT-BACKUP-EXISTS"
        );

        made.rdb_backup = Some(dir.join("fresh.bin"));
        made.ready_to_run().unwrap();

        let formats_only = plan_of(vec![PreloadStep::FormatPartition {
            slot: None,
            index: 1,
            drive_name: "DH0".into(),
            volume_name: "Work".into(),
        }]);
        assert!(!formats_only.edits_rdb());
        formats_only.ready_to_run().unwrap();
    }

    #[test]
    fn run_embeds_before_the_format_and_reports_it() {
        let (_guard, dir) = scratch("run-embed");
        let image = card(&dir, AmigaHardDiskFs::Pfs3Standard);
        let backup = dir.join("card-rdb-backup.bin");
        let mut asked = request(image.clone(), Some(driver(&dir, "19.3")));
        asked.rdb_backup = Some(backup.clone());
        let made = plan(&asked).unwrap();
        assert_eq!(
            made.rdb_backup,
            Some(backup.clone()),
            "the plan echoes the request"
        );

        let recorder = Recorder::default();
        let outcome = run(&made, &recorder, &crate::core::jobs::NoProgress).unwrap();

        assert_eq!(*recorder.calls.borrow(), vec!["format 1 Work"]);
        let embedded = outcome.embedded.expect("the embed is reported");
        assert_eq!(
            (embedded.first_block, embedded.last_block, embedded.backup),
            (2, 130, backup)
        );
        assert!(crate::core::card::read_card(&image)
            .unwrap()
            .provides_file_system(0x5046_5303));
    }

    /// FFS needs nothing in the RDB, so a plan for it is one step.
    #[test]
    fn a_plan_formats_the_partition_it_was_asked_for() {
        let (_guard, dir) = scratch("ffs");
        let image = card(&dir, AmigaHardDiskFs::FfsStandard);

        let made = plan(&PreloadRequest {
            image: image.clone(),
            driver: None,
            partitions: one(1, "Work", None),
            rdb_backup: None,
        })
        .unwrap();

        assert_eq!(
            made.steps,
            vec![PreloadStep::FormatPartition {
                slot: None,
                index: 1,
                drive_name: "DH0".into(),
                volume_name: "Work".into(),
            }]
        );
        assert_eq!(made.formats(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **ART-084, as a refusal before anything runs.** Formatting a `PDS\3`
    /// partition on a card carrying no PFS3 driver succeeds and produces a
    /// volume an Amiga ignores in silence — the worst kind of failure, because
    /// every step reports success.
    #[test]
    fn formatting_pfs3_with_no_driver_anywhere_is_refused() {
        let (_guard, dir) = scratch("no-driver");
        let image = card(&dir, AmigaHardDiskFs::Pfs3Standard);

        let err = plan(&PreloadRequest {
            image,
            driver: None,
            partitions: one(1, "Work", None),
            rdb_backup: None,
        })
        .unwrap_err();

        assert_eq!(err.code(), "ART-INPUT-INVALID", "{err}");
        assert!(err.to_string().contains("ignore the volume"), "{err}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Supply the driver and it is embedded **before** the format, because a
    /// volume formatted for a filesystem the card does not carry is the same
    /// silent failure.
    ///
    /// The DosType is the partition's own — `PFS` here, `PDS` for the
    /// DirectSCSI variant SD-0's command set used. Passed through rather than
    /// chosen: which of the two a card wants is a fact about the card.
    #[test]
    fn a_supplied_driver_is_imported_before_the_format() {
        let (_guard, dir) = scratch("driver");
        let image = card(&dir, AmigaHardDiskFs::Pfs3Standard);
        let driver = driver(&dir, "19.3");

        let made = plan(&PreloadRequest {
            image,
            driver: Some(driver.clone()),
            partitions: one(1, "Work", None),
            rdb_backup: None,
        })
        .unwrap();

        assert_eq!(
            made.steps[0],
            PreloadStep::ImportFilesystem {
                // A plain HDF: no partition table, so the RDB is at byte zero
                // and the tool is pointed at the image itself.
                slot: None,
                driver,
                dostype: "PFS3".into(),
                name: "pfs3aio".into(),
                file_version: DriverVersion {
                    version: 19,
                    revision: 3,
                },
                blocks: [2, 130],
                rdb_blocks_hi_raised: Some([1, 130]),
            },
            "{:?}",
            made.steps
        );
        assert!(matches!(made.steps[1], PreloadStep::FormatPartition { .. }));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Two PFS3 partitions need the driver once, not twice.
    #[test]
    fn the_driver_is_imported_once_for_a_whole_card() {
        let (_guard, dir) = scratch("twice");
        let image = dir.join("two.hdf");
        crate::core::hdf::create_hdf(
            &image,
            64 * 1024 * 1024,
            true,
            &[
                PartitionSpec {
                    drive_name: "DH0".into(),
                    fs_type: AmigaHardDiskFs::Pfs3Standard,
                    size_mb: 10,
                    bootable: true,
                    boot_priority: 0,
                    num_buffers: 0,
                },
                PartitionSpec {
                    drive_name: "DH1".into(),
                    fs_type: AmigaHardDiskFs::Pfs3Standard,
                    size_mb: 10,
                    bootable: false,
                    boot_priority: 0,
                    num_buffers: 0,
                },
            ],
            &[],
        )
        .unwrap();
        let driver = driver(&dir, "19.3");

        let made = plan(&PreloadRequest {
            image,
            driver: Some(driver),
            partitions: vec![
                PreloadPartition {
                    area: 1,
                    index: 1,
                    volume_name: "Work".into(),
                    content: None,
                },
                PreloadPartition {
                    area: 1,
                    index: 2,
                    volume_name: "Games".into(),
                    content: None,
                },
            ],
            rdb_backup: None,
        })
        .unwrap();

        assert_eq!(
            made.steps
                .iter()
                .filter(|s| matches!(s, PreloadStep::ImportFilesystem { .. }))
                .count(),
            1,
            "{:?}",
            made.steps
        );
        assert_eq!(made.formats(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Content is copied after its own partition's format, never before it.
    #[test]
    fn content_is_copied_after_the_format_that_makes_room_for_it() {
        let (_guard, dir) = scratch("content");
        let image = card(&dir, AmigaHardDiskFs::FfsStandard);
        let tree = dir.join("tree");
        std::fs::create_dir_all(&tree).unwrap();

        let made = plan(&PreloadRequest {
            image,
            driver: None,
            partitions: one(1, "Work", Some(tree.clone())),
            rdb_backup: None,
        })
        .unwrap();

        assert_eq!(
            made.steps,
            vec![
                PreloadStep::FormatPartition {
                    slot: None,
                    index: 1,
                    drive_name: "DH0".into(),
                    volume_name: "Work".into(),
                },
                PreloadStep::CopyIn {
                    slot: None,
                    drive_name: "DH0".into(),
                    source: tree,
                },
            ]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A partition number the card does not have is refused with the count,
    /// before anything is run.
    #[test]
    fn a_partition_that_is_not_there_is_refused() {
        let (_guard, dir) = scratch("missing");
        let image = card(&dir, AmigaHardDiskFs::FfsStandard);

        for index in [0, 2, 99] {
            let err = plan(&PreloadRequest {
                image: image.clone(),
                driver: None,
                partitions: one(index, "Work", None),
                rdb_backup: None,
            })
            .unwrap_err();
            assert!(
                err.to_string().contains("no partition"),
                "index {index}: {err}"
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // Running one
    // -----------------------------------------------------------------------

    /// A formatter that records what it was asked to do and runs nothing.
    ///
    /// The whole reason [`VolumeFormatter`] is a trait: the orchestration is
    /// provable without `hst-imager` on the machine, and CI never shells out.
    #[derive(Default)]
    struct Recorder {
        calls: std::cell::RefCell<Vec<String>>,
        fail_at: Option<usize>,
    }

    impl VolumeFormatter for Recorder {
        fn probe(&self) -> CoreResult<ToolVersion> {
            Ok(ToolVersion {
                raw: "test-double".into(),
            })
        }
        fn format_partition(
            &self,
            _i: &Path,
            _slot: Option<usize>,
            index: usize,
            volume: &str,
            _s: &dyn ProgressSink,
        ) -> CoreResult<()> {
            self.record(format!("format {index} {volume}"))
        }
        fn copy_in(
            &self,
            _i: &Path,
            _slot: Option<usize>,
            drive: &str,
            _src: &Path,
            _s: &dyn ProgressSink,
        ) -> CoreResult<CopySummary> {
            self.record(format!("copy {drive}"))?;
            Ok(CopySummary {
                files: 2,
                directories: 1,
                bytes: Some(36),
                ..Default::default()
            })
        }
    }

    impl Recorder {
        fn record(&self, what: String) -> CoreResult<()> {
            let mut calls = self.calls.borrow_mut();
            calls.push(what);
            match self.fail_at {
                Some(at) if calls.len() == at => Err(CoreError::Malformed {
                    format: "hst-imager".into(),
                    detail: "the tool said no".into(),
                }),
                _ => Ok(()),
            }
        }
    }

    fn plan_of(steps: Vec<PreloadStep>) -> PreloadPlan {
        PreloadPlan {
            image: PathBuf::from("card.img"),
            steps,
            notes: Vec::new(),
            rdb_backup: None,
        }
    }

    /// The steps run in the order the plan lists them, and the outcome adds up
    /// what happened.
    #[test]
    fn a_plan_runs_in_order_and_reports_what_it_did() {
        let recorder = Recorder::default();
        let plan = plan_of(vec![
            PreloadStep::FormatPartition {
                slot: None,
                index: 1,
                drive_name: "DH0".into(),
                volume_name: "Work".into(),
            },
            PreloadStep::CopyIn {
                slot: None,
                drive_name: "DH0".into(),
                source: PathBuf::from("staging"),
            },
        ]);

        let outcome = run(&plan, &recorder, &crate::core::jobs::NoProgress).unwrap();

        assert_eq!(*recorder.calls.borrow(), vec!["format 1 Work", "copy DH0"]);
        assert_eq!(outcome.formatted, vec!["DH0"]);
        assert_eq!(outcome.copied.files, 2);
        assert_eq!(outcome.tool.unwrap().raw, "test-double");
    }

    /// **A step that fails stops the run.** Carrying on would copy content
    /// into a volume the format did not make.
    #[test]
    fn a_failed_step_stops_the_rest() {
        let recorder = Recorder {
            fail_at: Some(2),
            ..Default::default()
        };
        let plan = plan_of(vec![
            PreloadStep::FormatPartition {
                slot: None,
                index: 1,
                drive_name: "DH0".into(),
                volume_name: "Work".into(),
            },
            PreloadStep::FormatPartition {
                slot: None,
                index: 2,
                drive_name: "DH1".into(),
                volume_name: "Games".into(),
            },
            PreloadStep::CopyIn {
                slot: None,
                drive_name: "DH1".into(),
                source: PathBuf::from("staging"),
            },
        ]);

        let err = run(&plan, &recorder, &crate::core::jobs::NoProgress)
            .unwrap_err()
            .error;

        assert_eq!(err.code(), "ART-FORMAT-MALFORMED", "{err}");
        assert_eq!(
            recorder.calls.borrow().len(),
            2,
            "the copy after the failed format must not run"
        );
    }

    #[test]
    fn content_that_is_not_a_folder_is_refused() {
        let (_guard, dir) = scratch("not-folder");
        let image = card(&dir, AmigaHardDiskFs::FfsStandard);
        let file = dir.join("a-file");
        std::fs::write(&file, b"x").unwrap();

        let err = plan(&PreloadRequest {
            image,
            driver: None,
            partitions: one(1, "Work", Some(file)),
            rdb_backup: None,
        })
        .unwrap_err();

        assert!(err.to_string().contains("is not a folder"), "{err}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
