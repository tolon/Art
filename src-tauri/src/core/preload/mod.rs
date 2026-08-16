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

pub mod native;
pub mod pfs3dev;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::card::read_card;
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;

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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopySummary {
    pub files: u64,
    pub directories: u64,
    pub bytes: u64,
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

/// Formatting an Amiga volume and putting files in it.
///
/// Outside `core` because every method runs a program. The trait is here so
/// the planning above it, and the tests below, need no binary.
pub trait VolumeFormatter {
    fn probe(&self) -> CoreResult<ToolVersion>;

    /// Embed a filesystem driver in the image's RDB.
    fn import_filesystem(
        &self,
        image: &Path,
        slot: Option<usize>,
        driver: &Path,
        dostype: &str,
        name: &str,
        sink: &dyn ProgressSink,
    ) -> CoreResult<()>;

    /// Format one partition. **Destructive**: whatever was in it is gone.
    fn format_partition(
        &self,
        image: &Path,
        slot: Option<usize>,
        index: usize,
        volume: &str,
        sink: &dyn ProgressSink,
    ) -> CoreResult<()>;

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
    /// A filesystem driver to embed first, if the card does not carry one for
    /// the DosType its partitions name.
    pub driver: Option<PathBuf>,
    pub partitions: Vec<PreloadPartition>,
}

/// One thing that will be run, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "kebab-case")]
pub enum PreloadStep {
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

/// What a preload would do, before any of it is done (§92's PREVIEW).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreloadPlan {
    pub image: PathBuf,
    pub steps: Vec<PreloadStep>,
}

impl PreloadPlan {
    /// How many partitions this would erase.
    pub fn formats(&self) -> usize {
        self.steps
            .iter()
            .filter(|step| matches!(step, PreloadStep::FormatPartition { .. }))
            .count()
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
    let mut steps = Vec::new();
    let mut imported = false;

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

        // Which MBR slot the disk sits in, so a tool can be pointed at the
        // RDB *inside* the card rather than at byte zero.
        let slot = mbr_slot_of(&card, wanted.area - 1);

        // **ART-084 as a gate, one more time.** Formatting a `PDS` partition
        // on a card that carries no PFS3 driver produces a volume an Amiga
        // ignores in silence — the format would succeed and the card would
        // come up without it.
        if !card.provides_file_system(part.dostype) && !kickstart_carries(part.dostype) {
            match (&request.driver, imported) {
                (Some(driver), false) => {
                    steps.push(PreloadStep::ImportFilesystem {
                        slot,
                        driver: driver.clone(),
                        dostype: part.dostype_str.clone(),
                        name: driver_name(driver),
                    });
                    imported = true;
                }
                (Some(_), true) => {}
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

        steps.push(PreloadStep::FormatPartition {
            slot,
            index: wanted.index,
            drive_name: part.drive_name.clone(),
            volume_name: wanted.volume_name.clone(),
        });

        if let Some(content) = &wanted.content {
            if !content.is_dir() {
                return Err(CoreError::InvalidInput(format!(
                    "'{}' is not a folder",
                    content.display()
                )));
            }
            steps.push(PreloadStep::CopyIn {
                slot,
                drive_name: part.drive_name.clone(),
                source: content.clone(),
            });
        }
    }

    Ok(PreloadPlan {
        image: request.image.clone(),
        steps,
    })
}

/// What a finished preload did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreloadOutcome {
    pub formatted: Vec<String>,
    pub copied: CopySummary,
    /// What the formatter reported itself to be, for the manifest and the log.
    pub tool: Option<ToolVersion>,
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
) -> CoreResult<PreloadOutcome> {
    let mut outcome = PreloadOutcome {
        tool: formatter.probe().ok(),
        ..Default::default()
    };

    let total = plan.steps.len() as u64;
    for (done, step) in plan.steps.iter().enumerate() {
        if sink.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        sink.report(done as u64, Some(total), &step_label(step));

        match step {
            PreloadStep::ImportFilesystem {
                slot,
                driver,
                dostype,
                name,
            } => formatter.import_filesystem(&plan.image, *slot, driver, dostype, name, sink)?,
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
                outcome.copied.files += summary.files;
                outcome.copied.directories += summary.directories;
                outcome.copied.bytes += summary.bytes;
            }
        }
    }

    sink.report(total, Some(total), "done");
    Ok(outcome)
}

fn step_label(step: &PreloadStep) -> String {
    match step {
        PreloadStep::ImportFilesystem { name, .. } => format!("Embedding {name}"),
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
    use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("art-preload-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
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

    /// FFS needs nothing in the RDB, so a plan for it is one step.
    #[test]
    fn a_plan_formats_the_partition_it_was_asked_for() {
        let dir = scratch("ffs");
        let image = card(&dir, AmigaHardDiskFs::FfsStandard);

        let made = plan(&PreloadRequest {
            image: image.clone(),
            driver: None,
            partitions: one(1, "Work", None),
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
        let dir = scratch("no-driver");
        let image = card(&dir, AmigaHardDiskFs::Pfs3Standard);

        let err = plan(&PreloadRequest {
            image,
            driver: None,
            partitions: one(1, "Work", None),
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
        let dir = scratch("driver");
        let image = card(&dir, AmigaHardDiskFs::Pfs3Standard);
        let driver = dir.join("pfs3aio.lha");
        std::fs::write(&driver, b"driver").unwrap();

        let made = plan(&PreloadRequest {
            image,
            driver: Some(driver.clone()),
            partitions: one(1, "Work", None),
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
        let dir = scratch("twice");
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
        let driver = dir.join("pfs3aio.lha");
        std::fs::write(&driver, b"driver").unwrap();

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
        let dir = scratch("content");
        let image = card(&dir, AmigaHardDiskFs::FfsStandard);
        let tree = dir.join("tree");
        std::fs::create_dir_all(&tree).unwrap();

        let made = plan(&PreloadRequest {
            image,
            driver: None,
            partitions: one(1, "Work", Some(tree.clone())),
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
        let dir = scratch("missing");
        let image = card(&dir, AmigaHardDiskFs::FfsStandard);

        for index in [0, 2, 99] {
            let err = plan(&PreloadRequest {
                image: image.clone(),
                driver: None,
                partitions: one(index, "Work", None),
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
        fn import_filesystem(
            &self,
            _i: &Path,
            _slot: Option<usize>,
            _d: &Path,
            dostype: &str,
            name: &str,
            _s: &dyn ProgressSink,
        ) -> CoreResult<()> {
            self.record(format!("import {name} {dostype}"))
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
                bytes: 36,
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
        }
    }

    /// The steps run in the order the plan lists them, and the outcome adds up
    /// what happened.
    #[test]
    fn a_plan_runs_in_order_and_reports_what_it_did() {
        let recorder = Recorder::default();
        let plan = plan_of(vec![
            PreloadStep::ImportFilesystem {
                slot: None,
                driver: PathBuf::from("pfs3aio.lha"),
                dostype: "PDS3".into(),
                name: "pfs3aio".into(),
            },
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

        assert_eq!(
            *recorder.calls.borrow(),
            vec!["import pfs3aio PDS3", "format 1 Work", "copy DH0"]
        );
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

        let err = run(&plan, &recorder, &crate::core::jobs::NoProgress).unwrap_err();

        assert_eq!(err.code(), "ART-FORMAT-MALFORMED", "{err}");
        assert_eq!(
            recorder.calls.borrow().len(),
            2,
            "the copy after the failed format must not run"
        );
    }

    #[test]
    fn content_that_is_not_a_folder_is_refused() {
        let dir = scratch("not-folder");
        let image = card(&dir, AmigaHardDiskFs::FfsStandard);
        let file = dir.join("a-file");
        std::fs::write(&file, b"x").unwrap();

        let err = plan(&PreloadRequest {
            image,
            driver: None,
            partitions: one(1, "Work", Some(file)),
        })
        .unwrap_err();

        assert!(err.to_string().contains("is not a folder"), "{err}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
