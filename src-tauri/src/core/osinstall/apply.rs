//! Building the distribution tree and the manifest that says what built it.
//!
//! `plan()` (Task 5) is a description; this is the one place that turns it
//! into files a user keeps. Everything downstream — booting the tree,
//! removing a component cleanly — depends on two things being true about
//! what lands here: every byte came from a `.uaem`-preserving copy of the
//! user's own media, and `distribution.json` at the root says, for every
//! file, which component put it there, which medium it came out of, and its
//! SHA-256. That manifest is not bookkeeping; it is the only record of what
//! an install actually did, because the media it was read from is not kept
//! around afterwards — it cannot be reconstructed later by re-reading
//! anything.
//!
//! ## `SAFE_CREATE`, before anything else is touched
//!
//! A distribution folder already there is somebody's work — possibly a
//! previous install this one would silently interleave with. `apply` refuses
//! the moment `root` exists, before it opens a single medium.
//!
//! ## Every destination through `safe_join`
//!
//! `item.to` came from a recipe rule, and a recipe is data a human typed
//! (`Component::rules` in `mod.rs`). `core/layout/apply.rs`'s own module doc
//! says it plainly: "a `../` in a text box is the same hole a `../` in a zip
//! is." The same discipline applies here, and for the same reason — a review
//! of that module once found a genuine escape from the staging root before
//! the guard was restored.
//!
//! ## Two decisions worth stating, not just making
//!
//! **Cancellation.** `core/layout/apply.rs` answers "how much landed?" with
//! `CoreError::CancelledPartway { files }` rather than a bare `Cancelled`,
//! because the two read the same on screen even though one of them left real
//! work behind. This module follows that lead, with one adjustment: the
//! threshold is *files*, not *files-or-directories*. An empty drawer created
//! moments before cancellation is not work a user needs told about — the
//! `CancelledPartway` message is literally "cancelled after writing N
//! file(s)", and a lone empty directory does not deserve to be a `0` in that
//! sentence. So `apply` only reaches for `CancelledPartway` once at least one
//! real file is durably on disk; a cancellation that only got as far as
//! creating a directory reports the plain `Cancelled` `core/layout/apply.rs`
//! itself falls back to when nothing landed at all.
//!
//! **`FileRecord::bytes`.** This is the size that was actually written —
//! `bytes.len()` on what `MediaSource::read` handed back — never
//! `PlanItem::bytes`, the plan's own estimate. The two should always agree,
//! since `media_paths` is exactly the plan's own promise to reopen the same
//! media it already measured; the one way they would not is a floppy that
//! changed on disk between `plan()` and `apply()`, which is also exactly the
//! situation `core/layout/apply.rs`'s own precedent for this question warns
//! about (`ApplyOutcome.bytes` there is "the real size copied, not the
//! plan's estimate", proven by a test that deliberately makes them disagree).
//! `apply` does not treat a mismatch as an error — the file that was
//! actually read is the one on disk and the one hashed, so recording
//! anything else would make the manifest describe bytes nobody wrote.
//!
//! ## The manifest is written last
//!
//! Every other write in this module can fail or be cancelled without the
//! tree lying about itself, because nothing reads `distribution.json` to
//! decide what is really there — until it exists. Writing it last, after
//! every item has landed, is what keeps a half-built tree from claiming to
//! be a whole one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::plan::InstallPlan;
use super::source::{AdfSource, MediaSource};
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::security::path::safe_join;
use crate::core::volume::write::copy::sidecar_for;
use crate::core::volume::write::uaem::{render, sidecar_path};

/// One medium the plan actually read from, and the SHA-256 of the whole
/// image file — not of any one entry inside it. Removing a component later
/// needs to know it is looking at the same disk this install actually used;
/// the media itself is not kept around, so this is the only place that fact
/// is ever recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaRecord {
    pub volume_name: String,
    pub sha256: String,
}

/// One file in the finished tree, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRecord {
    /// `/`-separated, relative to the distribution root — matches
    /// [`super::plan::PlanItem::to`] exactly.
    pub path: String,
    pub component: String,
    /// The volume name, not the medium's filename — matches
    /// [`super::plan::PlanItem::media`].
    pub media: String,
    pub sha256: String,
    /// What was actually written, not [`super::plan::PlanItem::bytes`]'s
    /// estimate — see the module doc comment.
    pub bytes: u64,
}

/// What lives at the distribution root's own `distribution.json`: which
/// components built this tree, off which media, and — file by file — where
/// each one came from. Removing a component cleanly reads this back; it is
/// the only record, because the media itself is gone by then.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DistributionManifest {
    pub release: String,
    pub built_from: Vec<MediaRecord>,
    pub files: Vec<FileRecord>,
}

/// The manifest's own file name, at the distribution root.
pub const MANIFEST_FILE_NAME: &str = "distribution.json";

/// What one call to [`apply`] actually did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ApplyOutcome {
    pub root: PathBuf,
    pub files: u64,
    pub directories: u64,
    pub bytes: u64,
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Build the distribution tree `plan` describes under `root`.
///
/// `SAFE_CREATE` first: `root` must not already exist. Every medium named in
/// `plan.media_paths` is then opened once, read-only, and hashed whole — the
/// media is never modified by anything below this line. Items are placed one
/// at a time, checking `sink.is_cancelled()` between them and never inside
/// one, so stopping always leaves whole files behind. `distribution.json` is
/// written only after every item has landed — see the module doc comment.
pub fn apply(plan: &InstallPlan, root: &Path, sink: &dyn ProgressSink) -> CoreResult<ApplyOutcome> {
    // SAFE_CREATE. Nothing below this line touches `root` or any medium
    // until this has passed.
    if root.exists() {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' already exists — a distribution tree is never built over one \
             that is already there",
            root.display()
        )));
    }

    // A refused plan is not a smaller plan — `plan()` empties `items` and
    // `media_paths` the moment any refusal exists (see its own module doc),
    // so building one anyway would create `root` and write a
    // `distribution.json` with empty `files` and `built_from`: a manifest
    // asserting a complete, empty tree. That is requirement 5's failure —
    // a manifest lying about what it describes — arriving through a
    // different door, so it is refused here by the same rule.
    if !plan.refusals.is_empty() {
        return Err(CoreError::InvalidInput(format!(
            "this plan has {} unresolved refusal(s) and cannot be built",
            plan.refusals.len()
        )));
    }

    // Every medium the plan resolved, opened once — read-only, per
    // `source.rs`'s own module doc — and hashed whole from its raw bytes, so
    // `distribution.json` can say exactly which physical image each
    // component came out of.
    let mut sources: BTreeMap<String, Box<dyn MediaSource>> = BTreeMap::new();
    let mut built_from = Vec::new();
    for (volume, path) in &plan.media_paths {
        let raw = std::fs::read(path)?;
        built_from.push(MediaRecord {
            volume_name: volume.clone(),
            sha256: hex_sha256(&raw),
        });
        sources.insert(volume.clone(), Box::new(AdfSource::open(path)?));
    }

    std::fs::create_dir_all(root)?;

    let total = plan.items.len() as u64;
    let mut outcome = ApplyOutcome {
        root: root.to_path_buf(),
        ..Default::default()
    };
    let mut files = Vec::new();

    for (done, item) in plan.items.iter().enumerate() {
        // Between whole items, never inside one — see the module doc
        // comment on which of the two cancellation errors this reaches for.
        if sink.is_cancelled() {
            return Err(if outcome.files > 0 {
                CoreError::CancelledPartway {
                    files: outcome.files,
                }
            } else {
                CoreError::Cancelled
            });
        }
        sink.report(done as u64, Some(total), &item.to);

        let target = safe_join(root, &item.to).map_err(|err| {
            CoreError::SafetyRefused(format!(
                "'{}' does not stay inside the distribution root: {err}",
                item.to
            ))
        })?;

        if item.is_dir {
            std::fs::create_dir_all(&target)?;
            outcome.directories += 1;
            continue;
        }

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let source = sources.get_mut(&item.media).ok_or_else(|| {
            CoreError::InvalidInput(format!(
                "'{}' names media '{}', which this plan never opened",
                item.to, item.media
            ))
        })?;
        let bytes = source.read(&item.from)?;
        let entry = source.entry(&item.from)?.ok_or_else(|| {
            CoreError::InvalidInput(format!(
                "'{}' is no longer on media '{}'",
                item.from, item.media
            ))
        })?;

        crate::core::safety::atomic::atomic_write(&target, &bytes)?;

        // Only when there is something worth recording — see `sidecar_for`'s
        // own doc comment. Never itself copied as a file: it is written
        // beside `target`, under a name `uaem::sidecar_path` builds by
        // appending `.uaem` rather than replacing the extension. A medium
        // that genuinely carried a file literally called `X.uaem` next to
        // `X` would still collide with `X`'s own sidecar — vanishingly
        // unlikely on real Amiga media, but the code does not rule it out,
        // so this comment shouldn't claim more than it does.
        if let Some(sidecar) = sidecar_for(entry.protection, entry.date, &entry.comment) {
            crate::core::safety::atomic::atomic_write(
                &sidecar_path(&target),
                render(&sidecar).as_bytes(),
            )?;
        }

        let written = bytes.len() as u64;
        let sha256 = hex_sha256(&bytes);
        outcome.bytes += written;
        outcome.files += 1;
        files.push(FileRecord {
            path: item.to.clone(),
            component: item.component.clone(),
            media: item.media.clone(),
            sha256,
            bytes: written,
        });
    }

    sink.report(total, Some(total), "done");

    // Last, deliberately — see the module doc comment. Everything above this
    // line can fail or be cancelled without the tree claiming to be whole;
    // this line is the only thing that makes the claim.
    let manifest = DistributionManifest {
        release: plan.release.clone(),
        built_from,
        files,
    };
    let manifest_text =
        serde_json::to_string_pretty(&manifest).map_err(|err| CoreError::Malformed {
            format: "distribution manifest".into(),
            detail: err.to_string(),
        })?;
    crate::core::safety::atomic::atomic_write(
        &root.join(MANIFEST_FILE_NAME),
        manifest_text.as_bytes(),
    )?;

    Ok(outcome)
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jobs::NoProgress;
    use crate::core::osinstall::fixtures;
    use crate::core::osinstall::plan::PlanItem;

    /// A plan `apply` can run against directly, without going through
    /// `plan()` — `apply` only ever consumes the `InstallPlan` struct, so a
    /// test of `apply` alone should not have to satisfy every rule `plan()`
    /// enforces along the way. Built by hand instead, the way `plan.rs`'s
    /// own hand-built-recipe tests already do for shapes the shipped recipe
    /// cannot produce.
    ///
    /// Two files, both real entries on one medium, `ModulesA1200_3.2` —
    /// matching the shipped recipe's own `modules-a1200` component and its
    /// `C/LoadModule` rule, so a test reading the manifest afterwards is
    /// checking a real, recognisable shape rather than an invented one.
    /// `C/LoadModule` carries protection `0x20` (`--p-rwed`) — the exact
    /// fixture `source.rs`'s own protection test uses, and the bit pattern
    /// the module doc comment on `uaem.rs` calls out as load-bearing:
    /// AmigaOS 3.2's `Startup-Sequence` runs `Resident C:Assign PURE` and
    /// fails without it. `C/Other` exists so a cancellation test has a
    /// second file to stop before reaching.
    fn planned() -> (InstallPlan, PathBuf) {
        // A fresh scratch directory every call, not a fixed tag: several of
        // this module's own tests call `planned()` and run in parallel
        // threads of the same test binary (same pid), and `fixtures::scratch`
        // keys only on tag + pid — a shared tag would let two tests race over
        // the same directory, which is exactly what happened before this
        // counter was added (see the report).
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = fixtures::scratch(&format!("apply-planned-{n}"));
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        fixtures::media(
            &folder,
            "ModulesA1200_3.2",
            "modules.adf",
            &[("C/LoadModule", b"cmd", 0x20), ("C/Other", b"more", 0x00)],
        );

        let mut media_paths = BTreeMap::new();
        media_paths.insert("ModulesA1200_3.2".to_string(), folder.join("modules.adf"));

        let items = vec![
            PlanItem {
                component: "modules-a1200".into(),
                media: "ModulesA1200_3.2".into(),
                from: "C/LoadModule".into(),
                to: "C/LoadModule".into(),
                is_dir: false,
                bytes: 3,
            },
            PlanItem {
                component: "modules-a1200".into(),
                media: "ModulesA1200_3.2".into(),
                from: "C/Other".into(),
                to: "C/Other".into(),
                is_dir: false,
                bytes: 4,
            },
        ];
        let total_bytes = items.iter().map(|i| i.bytes).sum();

        let plan = InstallPlan {
            release: "AmigaOS 3.2".into(),
            items,
            refusals: Vec::new(),
            total_bytes,
            components_on: vec!["modules-a1200".into()],
            media_paths,
        };
        (plan, dir)
    }

    fn media_folder(dir: &Path) -> PathBuf {
        dir.join("media")
    }

    #[test]
    fn the_tree_carries_a_uaem_sidecar_for_every_file_with_something_to_say() {
        let (plan, dir) = planned();
        let root = dir.join("dist");
        apply(&plan, &root, &NoProgress).unwrap();

        let sidecar = root.join("C").join("LoadModule.uaem");
        assert!(sidecar.exists());
        assert!(std::fs::read_to_string(&sidecar)
            .unwrap()
            .starts_with("--p-rwed"));
    }

    #[test]
    fn the_manifest_says_which_component_and_which_media_each_file_came_from() {
        let (plan, dir) = planned();
        let root = dir.join("dist");
        apply(&plan, &root, &NoProgress).unwrap();

        let manifest: DistributionManifest =
            serde_json::from_str(&std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
                .unwrap();
        let record = manifest
            .files
            .iter()
            .find(|f| f.path == "C/LoadModule")
            .unwrap();
        assert_eq!(record.component, "modules-a1200");
        assert_eq!(record.media, "ModulesA1200_3.2");
        // The actual digest of the known content, not merely its length — a
        // hash of the path, the sidecar text, or a placeholder string would
        // also happen to be 64 hex characters long. `FileRecord::bytes` is
        // covered on its own by `the_manifest_records_the_real_size_written_
        // not_the_plans_estimate` below, which is the one that can actually
        // fail for a wrong size (`plan.items[0].bytes` here is 3, which
        // happens to already be correct, so asserting it here again would
        // not prove anything that test doesn't already prove better).
        assert_eq!(record.sha256, hex_sha256(b"cmd"));

        assert_eq!(manifest.release, "AmigaOS 3.2");
        let media_record = manifest
            .built_from
            .iter()
            .find(|m| m.volume_name == "ModulesA1200_3.2")
            .unwrap();
        let expected_media_hash = {
            let raw = std::fs::read(media_folder(&dir).join("modules.adf")).unwrap();
            hex_sha256(&raw)
        };
        assert_eq!(media_record.sha256, expected_media_hash);
    }

    /// The `plan()` → `apply()` seam, exercised for real. Every other test
    /// in this module hand-builds an `InstallPlan` (see `planned()`'s own
    /// doc comment for why), and every one of those hand-built plans sets
    /// `is_dir: false` throughout — so none of them ever walked `apply`'s
    /// directory branch (`create_dir_all` + `outcome.directories += 1`) at
    /// all. That branch is the one a real plan hits *first*, on almost
    /// every component: `workbench-base`'s own rules are all `Subtree`, and
    /// `plan()` always emits a `Subtree` rule's own root directory before
    /// anything inside it (see `plan.rs`'s comment: "the subtree's own
    /// root, so an empty drawer still gets created"). This test runs the
    /// real `plan()` — via `fixtures::planned_with`, the same helper
    /// `plan.rs`'s own tests use — over the shipped recipe's
    /// `workbench-base` component, and checks the two things nothing else
    /// in this module checked: `ApplyOutcome` itself, and that the manifest
    /// agrees with it.
    #[test]
    fn a_real_plan_builds_a_tree_that_matches_the_plan_including_its_directories() {
        let (plan, dir) = fixtures::planned_with(&["workbench-base"], &["Workbench3.2"], Some(47));
        assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
        let root = dir.join("dist");

        let outcome = apply(&plan, &root, &NoProgress).unwrap();

        let expected_files = plan.items.iter().filter(|i| !i.is_dir).count() as u64;
        let expected_dirs = plan.items.iter().filter(|i| i.is_dir).count() as u64;
        assert_eq!(outcome.files, expected_files);
        assert_eq!(outcome.directories, expected_dirs);
        assert!(
            outcome.directories > 0,
            "workbench-base's rules are all Subtree — this plan must have \
             produced at least one directory item, or this test is not \
             exercising the branch it claims to"
        );

        // Every item lands, and as the right kind — proves the directory
        // branch actually created directories rather than, say, silently
        // treating every item as a file.
        for item in &plan.items {
            let target = root.join(&item.to);
            assert!(target.exists(), "'{}' was never created", item.to);
            assert_eq!(
                target.is_dir(),
                item.is_dir,
                "'{}' landed as the wrong kind",
                item.to
            );
        }

        let manifest: DistributionManifest =
            serde_json::from_str(&std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
                .unwrap();
        assert_eq!(
            manifest.files.len() as u64,
            outcome.files,
            "the manifest must name exactly the files ApplyOutcome counted"
        );
    }

    /// Requirement 5's failure arriving through a different door: a plan
    /// `plan()` itself refused (empty `items`/`media_paths`, per its own
    /// module doc) must not be silently "built" into an empty tree with a
    /// manifest that claims completeness. `extras`'s media is deliberately
    /// absent, so `plan()` returns a real `MediaMissing` refusal.
    #[test]
    fn a_plan_with_refusals_is_refused_not_silently_built_empty() {
        let (plan, dir) = fixtures::planned_with(&["extras"], &["Workbench3.2"], Some(47));
        assert!(
            !plan.refusals.is_empty(),
            "sanity: this plan should have refused (extras's media is absent)"
        );
        let root = dir.join("dist");

        assert!(apply(&plan, &root, &NoProgress).is_err());
        assert!(
            !root.exists(),
            "nothing is built from a plan that never resolved"
        );
    }

    /// The negative half of "only when there is something worth recording":
    /// `fixtures::media` always stamps a file with the current wall-clock
    /// time (`write_entries` passes `date: None`, and `add_file` falls back
    /// to `amiga_now()`), so every file built through it carries a non-default
    /// date and always gets a sidecar — the test above never actually
    /// exercises the "nothing to record" branch of `sidecar_for`. This test
    /// builds a file the same way `source.rs`'s own
    /// `an_entry_carries_its_size_date_and_comment` does — straight through
    /// `VolumeWriter`, so the date can be pinned to `AmigaDate::default()`
    /// deliberately — with default protection, no comment and the Amiga
    /// epoch itself as its date, so there is genuinely nothing worth a
    /// `.uaem` for.
    #[test]
    fn a_file_with_nothing_worth_recording_gets_no_sidecar() {
        use crate::core::adf::bcpl::AmigaDate;
        use crate::core::volume::device::FileRegionMut;
        use crate::core::volume::write::{FileMeta, VolumeWriter};
        use crate::core::volume::{DosType, VolumeGeometry};

        let dir = fixtures::scratch("apply-no-sidecar");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        let image = fixtures::media(&folder, "Plain", "plain.adf", &[]);
        let geometry = VolumeGeometry::floppy_dd(DosType::new(*b"DOS\x01"));
        {
            let mut device =
                FileRegionMut::open(&image, 0, geometry.total_bytes(), geometry.block_size)
                    .unwrap();
            let mut writer = VolumeWriter::open(&mut device, geometry, &image, 0).unwrap();
            writer
                .add_file(
                    0,
                    "Plain",
                    b"nothing special",
                    FileMeta {
                        protection: Some(0),
                        date: Some(AmigaDate::default()),
                    },
                )
                .unwrap();
        }

        let mut media_paths = BTreeMap::new();
        media_paths.insert("Plain".to_string(), image);
        let items = vec![PlanItem {
            component: "a".into(),
            media: "Plain".into(),
            from: "Plain".into(),
            to: "Plain".into(),
            is_dir: false,
            bytes: 16,
        }];
        let plan = InstallPlan {
            release: "Test".into(),
            items,
            refusals: Vec::new(),
            total_bytes: 16,
            components_on: vec!["a".into()],
            media_paths,
        };

        let root = dir.join("dist");
        apply(&plan, &root, &NoProgress).unwrap();

        assert!(root.join("Plain").exists());
        assert!(
            !root.join("Plain.uaem").exists(),
            "default protection, no comment, and the Amiga epoch itself as the \
             date is nothing a sidecar needs to preserve"
        );
    }

    /// The mismatch itself: `PlanItem::bytes` is wrong on purpose, and the
    /// manifest must record what was actually read off the media, not the
    /// plan's stale guess — the same falsification `core/layout/apply.rs`'s
    /// own `a_file_lands_at_its_destination_and_the_source_is_untouched`
    /// uses for the identical question.
    #[test]
    fn the_manifest_records_the_real_size_written_not_the_plans_estimate() {
        let (mut plan, dir) = planned();
        plan.items[0].bytes = 999; // b"cmd" is really 3 bytes.
        let root = dir.join("dist");
        apply(&plan, &root, &NoProgress).unwrap();

        let manifest: DistributionManifest =
            serde_json::from_str(&std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
                .unwrap();
        let record = manifest
            .files
            .iter()
            .find(|f| f.path == "C/LoadModule")
            .unwrap();
        assert_eq!(record.bytes, 3, "the real size read, not the plan's 999");
    }

    /// `SAFE_CREATE`. A distribution folder already there is somebody's work.
    /// Strengthened past the brief's own `is_err()`: the folder the test
    /// pre-created must come back out exactly as empty as it went in — a
    /// version that only checked the return type could still pass while
    /// happily writing into an existing folder before hitting some later
    /// error.
    #[test]
    fn an_existing_destination_is_refused_never_written_into() {
        let (plan, dir) = planned();
        let root = dir.join("dist");
        std::fs::create_dir_all(&root).unwrap();
        assert!(apply(&plan, &root, &NoProgress).is_err());
        assert_eq!(
            std::fs::read_dir(&root).unwrap().count(),
            0,
            "nothing was written into the folder that was already there"
        );
    }

    /// The rule G11 proved by measurement: removing `safe_join` genuinely
    /// wrote outside the staging root.
    #[test]
    fn a_destination_that_climbs_out_of_the_root_is_refused() {
        let (mut plan, dir) = planned();
        plan.items[0].to = "../escaped".into();
        let root = dir.join("dist");
        assert!(apply(&plan, &root, &NoProgress).is_err());
        assert!(!dir.join("escaped").exists());
    }

    #[test]
    fn the_media_is_byte_for_byte_unchanged_afterwards() {
        let (plan, dir) = planned();
        let before = fixtures::digest_of_folder(&media_folder(&dir));
        apply(&plan, &dir.join("dist"), &NoProgress).unwrap();
        assert_eq!(fixtures::digest_of_folder(&media_folder(&dir)), before);
    }

    /// Cancelling after the first file has landed gets `CancelledPartway`
    /// with the real count — not just a different-shaped error, which is all
    /// the brief's own version of this test checked (`matches!(err,
    /// CoreError::Cancelled)` alone cannot fail if `apply` always returns
    /// plain `Cancelled`, so it never actually proved anything "says how
    /// many landed"). This pins the count and proves the file really is on
    /// disk.
    #[test]
    fn a_cancelled_apply_stops_between_files_and_says_how_many_landed() {
        let (plan, dir) = planned();
        let root = dir.join("dist");
        let sink = fixtures::CancelAfter::new(1);

        let err = apply(&plan, &root, &sink).unwrap_err();

        assert!(
            matches!(err, CoreError::CancelledPartway { files: 1 }),
            "{err:?}"
        );
        assert!(root.join("C").join("LoadModule").exists());
        assert!(
            !root.join("C").join("Other").exists(),
            "the second item was never begun"
        );
    }

    /// The other half of the same decision: cancelled before any file has
    /// landed reports the plain `Cancelled` `core/layout/apply.rs` itself
    /// falls back to — there is no count worth a sentence about.
    #[test]
    fn a_cancellation_before_any_file_lands_reports_plain_cancelled() {
        let (plan, dir) = planned();
        let root = dir.join("dist");
        let sink = fixtures::CancelAfter::new(0);

        let err = apply(&plan, &root, &sink).unwrap_err();

        assert!(matches!(err, CoreError::Cancelled), "{err:?}");
    }

    /// The manifest-written-last ordering, proved directly rather than
    /// assumed from the code's own shape: stopping partway must never leave
    /// a `distribution.json` behind, cancelled or not — a manifest is a
    /// claim of completeness, and a half-built tree must never make it.
    #[test]
    fn a_cancelled_run_leaves_no_manifest_behind() {
        let (plan, dir) = planned();
        let root = dir.join("dist");
        let sink = fixtures::CancelAfter::new(1);

        assert!(apply(&plan, &root, &sink).is_err());
        assert!(
            !root.join(MANIFEST_FILE_NAME).exists(),
            "a run that was stopped partway must not claim a complete tree"
        );
    }
}
