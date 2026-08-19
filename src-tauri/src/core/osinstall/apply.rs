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
//!
//! ## `S:User-Startup`, composed after every file, before the manifest
//!
//! [`super::startup::merge_user_startup`] is a pure function; this is where
//! it gets called. Three decisions, each forced by something already true
//! about this module:
//!
//! **Where.** After the main loop, so every media-sourced file — including
//! a real `S/User-Startup` a `Subtree "S"` rule may have copied off the
//! release media itself — is already on disk to merge into. Before the
//! manifest, so a run that stops here still leaves no `distribution.json`
//! behind. It gets the same cancellation checkpoint as every item in the
//! loop above: checked once, before the single atomic write that composes
//! it, never mid-write — so cancelling here is indistinguishable, on disk,
//! from cancelling between any two ordinary items.
//!
//! **What lands in the manifest.** A composed file has no one component
//! that "put it there" the way [`FileRecord::component`] means for every
//! other file — it can carry lines from several. Recording it once, under
//! whichever component happened to write last, would be a lie of exactly
//! the shape this manifest exists to prevent (a file claiming a single
//! origin it does not have) — that is the wrong answer the brief warned
//! was "obviously unconsidered". So `apply` records it **once per
//! contributing component**, all four sharing the same `path`, `sha256`
//! and `bytes` — the file has exactly one real content, and every
//! contributor's entry describes that same content — with `media: ""`,
//! since nothing here came off a disk image. A future "remove this
//! component cleanly" flow can then find every component that touched
//! `S/User-Startup` by filtering `files` on the path, the same way it
//! would find any other file. This is also why `ApplyOutcome::files` and
//! `manifest.files.len()` are no longer guaranteed equal — the former
//! counts real files on disk, the latter counts *records*, and a composed
//! file with more than one contributor now makes the second the larger of
//! the two on purpose.
//!
//! **A missing `S` drawer.** Reachable whenever a component carries
//! `user_startup` lines without `workbench-base` (or an equivalent) also
//! being on — a hand-built plan can do this even though the shipped
//! recipe's `workbench-base` is `required` and always creates `S`. Handled
//! exactly the way every ordinary item in the loop above already handles a
//! missing parent: `create_dir_all` on the target's parent before writing.
//! Refusing here would single out one file for a rule nothing else in this
//! function follows, over a directory that is entirely ART's own to create.
//!
//! ## Latin-1, not UTF-8
//!
//! `core/adf/bcpl.rs` already documents this rule for one BCPL field at a
//! time; `S/User-Startup` needs the same rule applied to a whole text file.
//! AmigaDOS text is Latin-1 — one byte per character, the identity mapping
//! on code points `0..=255` — and `String::from_utf8` on a media-provided
//! starter file or a hand-edited one turns the first accented character
//! into `CoreError::Malformed`, failing the install on its very last step,
//! after every other file has already landed. This project's user is
//! Turkish and the shipped recipe carries a `Locale-TR` component, so that
//! is an ordinary byte, not a hypothetical one. [`latin1_decode`] cannot
//! fail — every byte value has a Latin-1 character — so the read side of
//! this step never rejects a file for what it says. [`latin1_encode`]
//! mirrors `write_bcpl_string`'s own choice for a character with no Latin-1
//! byte at all: it becomes `?` rather than failing, since ART's own
//! generated block content and every shipped component's lines are plain
//! ASCII today, and a mis-rendered character composed by some future
//! component is a smaller problem than an install that cannot finish.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::plan::InstallPlan;
use super::source::MediaSource;
use super::startup::merge_user_startup;
use crate::core::error::{CoreError, CoreResult};
use crate::core::hashing::{sha256_bytes, sha256_file};
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
///
/// Ordinarily one record per `path` — but `S/User-Startup` is composed, not
/// copied (see the module doc comment's "S:User-Startup" section), so it
/// gets one record **per contributing component**, all sharing the one
/// `path`, `sha256` and `bytes` the composed file actually has. `component`
/// still names a single component for every other file; for that one path
/// it names "one of possibly several".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRecord {
    /// `/`-separated, relative to the distribution root — matches
    /// [`super::plan::PlanItem::to`] exactly, except for `S/User-Startup`.
    pub path: String,
    pub component: String,
    /// The volume name, not the medium's filename — matches
    /// [`super::plan::PlanItem::media`]. Empty for `S/User-Startup`, which
    /// came from no single medium.
    pub media: String,
    pub sha256: String,
    /// What was actually written, not [`super::plan::PlanItem::bytes`]'s
    /// estimate — see the module doc comment.
    pub bytes: u64,
    /// `HSPARWED` exactly as read off the source media (`MediaEntry::protection`
    /// — same convention throughout `core::volume`, `RWED` inverted). `None`
    /// for `S/User-Startup`: a composed file has no single medium's byte to
    /// carry over, and nothing in `apply` ever sets one on it (see the module
    /// doc comment's "S:User-Startup" section) — recording `Some(0)` there
    /// would assert an intent nobody actually stated. `#[serde(default)]` so a
    /// manifest written before this field existed deserializes as `None`
    /// rather than refusing to load — Task 10's `verify` reads exactly that as
    /// "this manifest never recorded an expected protection for this file",
    /// which is the truth.
    #[serde(default)]
    pub protection: Option<u32>,
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
    /// The Kickstart this tree was built for (G9). `#[serde(default)]` so a
    /// tree written by an older ART still reads back.
    #[serde(default)]
    pub paired_rom: Option<super::PairedRom>,
}

/// The manifest's own file name, at the distribution root.
pub const MANIFEST_FILE_NAME: &str = "distribution.json";

/// Where `S:User-Startup` lives in the tree, `/`-separated — the one file
/// `apply` composes itself rather than copying from a `PlanItem`. See the
/// module doc comment's "S:User-Startup" section.
const USER_STARTUP_PATH: &str = "S/User-Startup";

/// What one call to [`apply`] actually did.
///
/// `rename_all = "camelCase"` (Task 12 fix round 1): every field is a single
/// word today, so this changes nothing on the wire yet — added anyway so a
/// future multi-word field cannot repeat `VerifyReport::not_checked`'s
/// mistake by omission.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyOutcome {
    pub root: PathBuf,
    pub files: u64,
    pub directories: u64,
    pub bytes: u64,
}

/// Decode raw bytes as Latin-1 — see the module doc comment's "Latin-1, not
/// UTF-8" section. Latin-1 is exactly the first 256 Unicode code points, so
/// this is a plain cast that can never fail, unlike `String::from_utf8`.
/// The identical rule `core/adf/bcpl.rs::read_bcpl_string` already applies
/// to one BCPL field; this applies it to a whole file.
fn latin1_decode(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

/// The write side of the same rule. A character above `U+00FF` has no
/// Latin-1 byte; `core/adf/bcpl.rs::write_bcpl_string` maps that case to
/// `?` rather than failing, and this follows the same choice.
fn latin1_encode(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| if (c as u32) <= 0xFF { c as u8 } else { b'?' })
        .collect()
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
    // `source.rs`'s own module doc — and hashed by streaming
    // (`hashing::sha256_file`), never read whole into memory: CLAUDE.md's
    // own rule ("never read a whole user file into memory when only its
    // header is needed") applies with more force here than anywhere else in
    // this module — a floppy image is under 1 MB, but a real AmigaOS 3.9
    // disc is 469 MB, and `std::fs::read` used to pull the whole thing into
    // a `Vec` purely to feed `sha256_bytes`. `distribution.json` still ends
    // up saying exactly which physical image each component came out of.
    //
    // Opened through `scan::identify` — the same floppy-then-disc probe
    // `find_media` itself uses — rather than `AdfSource::open`
    // unconditionally (ART-153): `plan.media_paths` carries a path, never a
    // `MediaKind`, so `apply()` has to ask the same question `find_media`
    // already answered once, at plan time, on media that has since gone
    // stale (removed, replaced, or simply never was what the plan thinks it
    // is) is a real failure at apply time, not a skip — named by path so the
    // user knows which medium moved.
    let mut sources: BTreeMap<String, Box<dyn MediaSource>> = BTreeMap::new();
    let mut built_from = Vec::new();
    for (volume, path) in &plan.media_paths {
        let identified = super::scan::identify(path).ok_or_else(|| {
            CoreError::InvalidInput(format!(
                "'{}' no longer identifies as install media (expected volume '{volume}') — it \
                 may have been moved, replaced or removed since this plan was made",
                path.display()
            ))
        })?;
        let sha256 = sha256_file(path)?;
        built_from.push(MediaRecord {
            volume_name: volume.clone(),
            sha256,
        });
        sources.insert(volume.clone(), super::scan::open_media(&identified)?);
    }

    std::fs::create_dir_all(root)?;

    let total = plan.items.len() as u64;
    let mut outcome = ApplyOutcome {
        root: root.to_path_buf(),
        ..Default::default()
    };
    let mut files: Vec<FileRecord> = Vec::new();
    // **ART-124: what the tree holds, not what the run did.** An `overrides`
    // relationship means two components write the same destination on
    // purpose, and a directory named by two components is created once.
    // Counting items therefore over-reports both — and the manifest, which is
    // the only surviving record of where each file came from, carried the
    // overridden component's claim beside the winner's, one of them always
    // false. Keyed by `item.to`, the same key `plan::detect_collisions` pairs
    // claimants by, so the two cannot disagree about what "the same
    // destination" means.
    let mut written: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let mut made_dirs: std::collections::HashSet<&str> = std::collections::HashSet::new();

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

        // Ancestors no rule names — `Prefs/Presets` on the way to
        // `Prefs/Presets/Backdrops` — are created by `create_dir_all` and
        // were counted nowhere, which left `directories` short of the tree
        // even once the double-counting was fixed (ART-124). Both branches
        // below create them, so this sits above both; each distinct prefix
        // is stat'd once, not once per entry inside it.
        for (at, _) in item.to.match_indices('/') {
            let prefix = &item.to[..at];
            if made_dirs.insert(prefix) && !root.join(prefix).is_dir() {
                outcome.directories += 1;
            }
        }

        if item.is_dir {
            std::fs::create_dir_all(&target)?;
            if made_dirs.insert(item.to.as_str()) {
                outcome.directories += 1;
            }
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

        let size = bytes.len() as u64;
        let record = FileRecord {
            path: item.to.clone(),
            component: item.component.clone(),
            media: item.media.clone(),
            sha256: sha256_bytes(&bytes),
            bytes: size,
            protection: Some(entry.protection),
        };

        // An override replaces its predecessor's record rather than adding a
        // second one: the bytes on disk are this component's, so the record
        // describing them has to be too (ART-124). `bytes` follows, since
        // what the tree holds is the winner's size, not the sum of every
        // write that landed on the path.
        match written.get(item.to.as_str()) {
            Some(&at) => {
                outcome.bytes -= files[at].bytes;
                outcome.bytes += size;
                files[at] = record;
            }
            None => {
                written.insert(item.to.as_str(), files.len());
                outcome.bytes += size;
                outcome.files += 1;
                files.push(record);
            }
        }
    }

    // `S:User-Startup` — composed, not copied; see the module doc comment's
    // "S:User-Startup" section for all three decisions made here. Skipped
    // entirely when nothing has anything to add, so a plan with no
    // `user_startup` contributions (every shipped component today) never
    // touches the file at all.
    if !plan.user_startup.is_empty() {
        // Same cancellation checkpoint every item above already gets:
        // checked once, before the one atomic write this step performs,
        // never mid-write.
        if sink.is_cancelled() {
            return Err(if outcome.files > 0 {
                CoreError::CancelledPartway {
                    files: outcome.files,
                }
            } else {
                CoreError::Cancelled
            });
        }

        let target = safe_join(root, USER_STARTUP_PATH).map_err(|err| {
            CoreError::SafetyRefused(format!(
                "'{USER_STARTUP_PATH}' does not stay inside the distribution root: {err}"
            ))
        })?;

        // A missing `S` drawer is created, exactly like every other item's
        // missing parent above — see the module doc comment.
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // A `Subtree "S"` rule earlier in `plan.items` may already have
        // copied a real `S/User-Startup` off the release media — the
        // release's own starter file. If so it is already on disk at
        // `target`, and it is read back as the starting point rather than
        // assumed absent, so composing the file never discards content a
        // plan item just wrote. Latin-1, not UTF-8 — see the module doc
        // comment's "Latin-1, not UTF-8" section; this read can never fail
        // on account of what the file says.
        let existing = match std::fs::read(&target) {
            Ok(bytes) => Some(latin1_decode(&bytes)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
            Err(err) => return Err(CoreError::Io(err)),
        };

        let merged = plan
            .user_startup
            .iter()
            .fold(existing, |acc, contribution| {
                Some(merge_user_startup(
                    acc.as_deref(),
                    &contribution.component,
                    &contribution.lines,
                ))
            });
        // `plan.user_startup` is non-empty (checked above), so the fold ran
        // at least once and always produces `Some`.
        let merged = merged.expect("at least one contribution was folded");

        // What actually reaches disk — Latin-1 bytes, not `merged`'s own
        // UTF-8 representation — so `bytes`/`sha256` below describe the
        // real file, the same rule `FileRecord::bytes`'s own doc comment
        // states for every other file this function writes.
        let merged_disk_bytes = latin1_encode(&merged);
        crate::core::safety::atomic::atomic_write(&target, &merged_disk_bytes)?;
        // Reported after the write actually lands, not before it starts —
        // an earlier version of this line ran ahead of the write itself,
        // claiming `done == total` while the one atomic write this step
        // performs had not happened yet.
        sink.report(total, Some(total), USER_STARTUP_PATH);

        let merged_bytes = merged_disk_bytes.len() as u64;
        let merged_sha256 = sha256_bytes(&merged_disk_bytes);

        // Drop whatever record the copy loop above made for this path (a
        // media-provided starter file, if there was one) before adding the
        // composed file's own records — the pre-merge sha256/bytes it
        // carried no longer describe what is actually on disk.
        let previous_bytes = files
            .iter()
            .find(|f| f.path == USER_STARTUP_PATH)
            .map(|f| f.bytes);
        files.retain(|f| f.path != USER_STARTUP_PATH);
        match previous_bytes {
            Some(previous_bytes) => {
                outcome.bytes = outcome.bytes - previous_bytes + merged_bytes;
            }
            None => {
                outcome.files += 1;
                outcome.bytes += merged_bytes;
            }
        }

        for contribution in &plan.user_startup {
            files.push(FileRecord {
                path: USER_STARTUP_PATH.to_string(),
                component: contribution.component.clone(),
                media: String::new(),
                sha256: merged_sha256.clone(),
                bytes: merged_bytes,
                // No single medium's byte to carry over — see the field's own
                // doc comment.
                protection: None,
            });
        }
    }

    sink.report(total, Some(total), "done");

    // Last, deliberately — see the module doc comment. Everything above this
    // line can fail or be cancelled without the tree claiming to be whole;
    // this line is the only thing that makes the claim.
    let manifest = DistributionManifest {
        release: plan.release.clone(),
        built_from,
        files,
        paired_rom: plan.paired_rom.clone(),
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
    use crate::core::osinstall::plan::{PlanItem, UserStartupContribution};
    use crate::core::osinstall::source::AdfSource;
    use crate::core::osinstall::source_cd::CdSource;

    /// Diagnostic-only, for `build_the_real_39_tree_when_asked`'s failure
    /// path: count what actually landed under `root` after a partial
    /// `apply()` run — a real, measured number rather than a guess about
    /// how far it got.
    fn count_tree(root: &Path) -> (u64, u64) {
        let mut files = 0u64;
        let mut dirs = 0u64;
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(read) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in read.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    dirs += 1;
                    stack.push(path);
                } else {
                    files += 1;
                }
            }
        }
        (files, dirs)
    }

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
            paired_rom: None,
            media_paths,
            user_startup: Vec::new(),
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
        assert_eq!(record.sha256, sha256_bytes(b"cmd"));

        assert_eq!(manifest.release, "AmigaOS 3.2");
        let media_record = manifest
            .built_from
            .iter()
            .find(|m| m.volume_name == "ModulesA1200_3.2")
            .unwrap();
        let expected_media_hash = {
            let raw = std::fs::read(media_folder(&dir).join("modules.adf")).unwrap();
            sha256_bytes(&raw)
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

        // Items and entries are the same number *here* because one component
        // alone writes no destination twice. Where that stops being true —
        // an `overrides` relationship — the report counts entries, and
        // `the_report_counts_what_the_tree_holds_not_what_the_plan_did` is
        // the test for it (ART-124).
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

    /// **ART-124 — the report has to describe the tree, not the work.**
    ///
    /// `outcome.files` and the manifest both counted *plan items*, and an
    /// override writes one destination twice by design (that is what an
    /// override is — ART-112 was a missing one). So a real 3.2 install
    /// announced 4047 files where 3950 existed, and `distribution.json` —
    /// whose own doc comment calls it "the only record… because the media
    /// itself is gone by then" — carried 94 paths twice, each claiming a
    /// different component put it there. One of those two claims was always
    /// false.
    ///
    /// Asserted against the **filesystem**, not against a number worked out
    /// by hand: a count derived from `plan.items` is exactly the count that
    /// was wrong, so it cannot be the thing that checks it.
    #[test]
    fn the_report_counts_what_the_tree_holds_not_what_the_plan_did() {
        // `classes` overrides `workbench-base` — the double-count — and
        // `backdrops` writes into `Prefs/Presets/Backdrops`, whose middle
        // directory no rule names: the two halves of ART-124 in one plan.
        let (plan, dir) = fixtures::planned_with(
            &["classes", "backdrops"],
            &["Workbench3.2", "Classes3.2", "Install3.2", "Backdrops3.2"],
            Some(47),
        );
        assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
        assert!(
            plan.items
                .iter()
                .any(|i| i.to.starts_with("Prefs/Presets/"))
                && !plan.items.iter().any(|i| i.to == "Prefs/Presets"),
            "this test needs a destination whose parent no rule names"
        );

        // The fixture has to actually contain the shape this is about.
        let mut claimed: std::collections::BTreeMap<&str, usize> =
            std::collections::BTreeMap::new();
        for item in plan.items.iter().filter(|i| !i.is_dir) {
            *claimed.entry(item.to.as_str()).or_default() += 1;
        }
        let overridden = claimed.values().filter(|n| **n > 1).count();
        assert!(
            overridden > 0,
            "this test needs a plan where one destination is written twice; \
             `classes` overrides `workbench-base` in the shipped recipe"
        );

        let root = dir.join("dist");
        let outcome = apply(&plan, &root, &NoProgress).unwrap();

        // What is actually there, counted the way the tree is counted: a
        // `.uaem` sidecar is metadata beside an entry, and the manifest is
        // the report itself, not one of the files it reports.
        fn walk(dir: &Path, files: &mut u64, dirs: &mut u64) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    *dirs += 1;
                    walk(&path, files, dirs);
                } else if !path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("uaem"))
                    && path.file_name().is_some_and(|n| n != MANIFEST_FILE_NAME)
                {
                    *files += 1;
                }
            }
        }
        let (mut on_disk_files, mut on_disk_dirs) = (0, 0);
        walk(&root, &mut on_disk_files, &mut on_disk_dirs);

        assert_eq!(
            outcome.files, on_disk_files,
            "the report claims {} files; the tree holds {on_disk_files}",
            outcome.files
        );
        assert_eq!(outcome.directories, on_disk_dirs);
        assert_eq!(
            outcome.files,
            claimed.len() as u64,
            "one count per destination, not per item"
        );

        let manifest: DistributionManifest =
            serde_json::from_str(&std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
                .unwrap();
        assert_eq!(manifest.files.len() as u64, outcome.files);
        let mut paths: Vec<&str> = manifest.files.iter().map(|f| f.path.as_str()).collect();
        paths.sort_unstable();
        let before = paths.len();
        paths.dedup();
        assert_eq!(
            paths.len(),
            before,
            "the manifest may name a path once: two records for one file means \
             one of them credits a component that did not write what is there"
        );

        // And the record kept is the one that won — the override, which is
        // the component that wrote last.
        let winner = manifest
            .files
            .iter()
            .find(|f| claimed.get(f.path.as_str()).is_some_and(|n| *n > 1))
            .expect("an overridden path is in the manifest");
        assert_eq!(
            winner.component, "classes",
            "the surviving record must credit the component whose bytes are on \
             disk, not the one it overrode"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The coordinator's review, Critical 1, closed end to end. Before this
    /// fix, exclusion was applied by editing the `InstallPlan` *object* on
    /// the frontend — which left `plan.media_paths` untouched, so a
    /// component the user excluded was still opened by `apply` and still
    /// recorded in `distribution.json`'s `built_from`, a manifest whose own
    /// doc comment calls it "the only record… because the media itself is
    /// gone by then". Now `InstallRequest::excluded` is subtracted inside
    /// `resolve_components_on`, so `plan()` itself never adds the excluded
    /// component's media to `media_paths` in the first place.
    ///
    /// Proved here through the real `plan()` → `apply()` seam, not by
    /// inspecting the plan alone: `ModulesA1200_3.2`'s media is built as a
    /// normal, valid disk and then **deleted from disk** before `apply`
    /// runs. Under the old, client-filtered shape this would have failed
    /// `apply` outright — reading a file that just moved is exactly the
    /// review's "a disk that moved between plan and build fails a job over
    /// a component the user turned off". Succeeding here proves `apply`
    /// never reached for it at all.
    #[test]
    fn excluding_a_component_means_apply_never_opens_or_records_its_media() {
        let dir = fixtures::scratch("apply-excluded-media-untouched");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();

        let recipe = crate::core::osinstall::recipe::amigaos_32().unwrap();
        let wb = fixtures::entries_for(&recipe, "Workbench3.2");
        let wb_refs: Vec<(&str, &[u8], u32)> = wb
            .iter()
            .map(|(path, bytes, protection)| (path.as_str(), bytes.as_slice(), *protection))
            .collect();
        fixtures::media(&folder, "Workbench3.2", "wb.adf", &wb_refs);
        fixtures::required_media(&folder, &recipe, &["Workbench3.2"]);

        let modules = fixtures::entries_for(&recipe, "ModulesA1200_3.2");
        let modules_refs: Vec<(&str, &[u8], u32)> = modules
            .iter()
            .map(|(path, bytes, protection)| (path.as_str(), bytes.as_slice(), *protection))
            .collect();
        let modules_path =
            fixtures::media(&folder, "ModulesA1200_3.2", "modules.adf", &modules_refs);

        let request = crate::core::osinstall::plan::InstallRequest {
            media_folder: folder,
            rom: Some(fixtures::fake_rom(&dir, 40)), // pre-V47: the condition holds
            chosen: vec!["workbench-base".to_string()],
            excluded: vec!["modules-a1200".to_string()],
            destination: dir.join("dist"),
        };
        let built_plan = crate::core::osinstall::plan::plan(&request, &recipe).unwrap();
        assert!(built_plan.refusals.is_empty(), "{:?}", built_plan.refusals);
        assert!(!built_plan
            .components_on
            .iter()
            .any(|c| c == "modules-a1200"));
        assert!(
            !built_plan.media_paths.contains_key("ModulesA1200_3.2"),
            "the excluded component's media must never enter media_paths: {:?}",
            built_plan.media_paths
        );

        std::fs::remove_file(&modules_path).unwrap();

        let root = dir.join("dist");
        let outcome = apply(&built_plan, &root, &NoProgress).unwrap();
        assert!(outcome.files > 0);

        let manifest: DistributionManifest =
            serde_json::from_str(&std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
                .unwrap();
        assert!(
            !manifest
                .built_from
                .iter()
                .any(|m| m.volume_name == "ModulesA1200_3.2"),
            "the excluded component's media must not be recorded as something \
             this tree was built from: {:?}",
            manifest.built_from
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
            paired_rom: None,
            media_paths,
            user_startup: Vec::new(),
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

    // ---- Task 7: `S:User-Startup` ----

    /// `planned()`'s own plan never touches `S` at all — its two items are
    /// `C/LoadModule` and `C/Other` — so this is also the "no `S` drawer"
    /// case (decision 3): the drawer has to be created from nothing.
    #[test]
    fn a_contribution_composes_the_file_and_creates_a_missing_s_drawer() {
        let (mut plan, dir) = planned();
        plan.user_startup = vec![UserStartupContribution {
            component: "modules-a1200".into(),
            lines: vec!["Assign Foo: SYS:".into()],
        }];
        let root = dir.join("dist");

        let outcome = apply(&plan, &root, &NoProgress).unwrap();

        let content = std::fs::read_to_string(root.join("S").join("User-Startup")).unwrap();
        assert_eq!(
            content,
            ";BEGIN modules-a1200\nAssign Foo: SYS:\n;END modules-a1200\n"
        );

        // A real file that was not one of `plan.items` — `ApplyOutcome`
        // must count it too, not just the copied ones.
        assert_eq!(outcome.files, 3, "2 copied items + the composed file");
        assert_eq!(
            outcome.bytes,
            3 + 4 + content.len() as u64,
            "the composed file's real size, added once"
        );
    }

    /// The manifest side of the same run: one record, naming the
    /// contributing component, with `media` empty because nothing here came
    /// off a disk image, and the real sha256/bytes of the composed file —
    /// not the plan's `bytes` estimate, which does not exist for this file
    /// at all (it is not a `PlanItem`).
    #[test]
    fn the_manifest_names_the_contributing_component_with_no_media() {
        let (mut plan, dir) = planned();
        plan.user_startup = vec![UserStartupContribution {
            component: "modules-a1200".into(),
            lines: vec!["Assign Foo: SYS:".into()],
        }];
        let root = dir.join("dist");
        apply(&plan, &root, &NoProgress).unwrap();

        let content = std::fs::read_to_string(root.join("S").join("User-Startup")).unwrap();
        let manifest: DistributionManifest =
            serde_json::from_str(&std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
                .unwrap();
        let records: Vec<_> = manifest
            .files
            .iter()
            .filter(|f| f.path == "S/User-Startup")
            .collect();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].component, "modules-a1200");
        assert_eq!(records[0].media, "");
        assert_eq!(records[0].sha256, sha256_bytes(content.as_bytes()));
        assert_eq!(records[0].bytes, content.len() as u64);
    }

    /// Two contributing components: both blocks land, in the order
    /// `plan.user_startup` carries them (recipe order — `plan.rs`'s own
    /// concern, not this one's), and the manifest carries **two** records
    /// for the one path, both sharing the one file's real content — the
    /// property the module doc comment's "S:User-Startup" section commits
    /// to. Falsification: a version that pushed only the *last*
    /// contribution's record (the "obviously unconsidered" shape the brief
    /// warned about) would still pass every single-contribution test above,
    /// since they only ever supply one. This is the one test that shape
    /// fails.
    #[test]
    fn two_contributions_both_land_and_both_get_a_manifest_record() {
        let (mut plan, dir) = planned();
        plan.user_startup = vec![
            UserStartupContribution {
                component: "modules-a1200".into(),
                lines: vec!["Assign Alpha: SYS:".into()],
            },
            UserStartupContribution {
                component: "storage".into(),
                lines: vec!["Assign Beta: SYS:".into()],
            },
        ];
        let root = dir.join("dist");
        apply(&plan, &root, &NoProgress).unwrap();

        let content = std::fs::read_to_string(root.join("S").join("User-Startup")).unwrap();
        assert!(content.contains(";BEGIN modules-a1200\nAssign Alpha: SYS:\n;END modules-a1200\n"));
        assert!(content.contains(";BEGIN storage\nAssign Beta: SYS:\n;END storage\n"));

        // Review item 3: order is claimed everywhere (the field doc,
        // `plan.rs`'s comment, `apply`'s own fold) and, until now, tested
        // nowhere. `modules-a1200` is first in `plan.user_startup`, so its
        // block must land at a lower byte offset than `storage`'s — these
        // are `Assign` lines, and which one runs first changes what the
        // Amiga does.
        let modules_offset = content.find(";BEGIN modules-a1200").unwrap();
        let storage_offset = content.find(";BEGIN storage").unwrap();
        assert!(
            modules_offset < storage_offset,
            "modules-a1200 is first in plan.user_startup and must land first \
             in the file: modules at {modules_offset}, storage at {storage_offset}"
        );

        let manifest: DistributionManifest =
            serde_json::from_str(&std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
                .unwrap();
        let records: Vec<_> = manifest
            .files
            .iter()
            .filter(|f| f.path == "S/User-Startup")
            .collect();
        assert_eq!(records.len(), 2);
        let components: Vec<&str> = records.iter().map(|r| r.component.as_str()).collect();
        assert!(components.contains(&"modules-a1200"));
        assert!(components.contains(&"storage"));
        assert!(
            records.iter().all(|r| r.sha256 == records[0].sha256),
            "one real file has one real sha256, whichever component's record you read"
        );
    }

    /// A `Subtree "S"` rule can genuinely copy a real `S/User-Startup` off
    /// the release media before this step ever runs — the release's own
    /// starter file. That copy must be read back and edited in place, never
    /// treated as absent (which would silently discard it) or left as a
    /// stale manifest record with the pre-merge sha256/bytes.
    #[test]
    fn a_media_provided_starter_file_is_merged_into_not_discarded() {
        let (mut plan, dir) = planned();
        let folder = media_folder(&dir);
        let starter = b"; the release's own starter file\n".as_slice();
        fixtures::media(
            &folder,
            "Workbench3.2",
            "wb.adf",
            &[("S/User-Startup", starter, 0)],
        );
        plan.media_paths
            .insert("Workbench3.2".to_string(), folder.join("wb.adf"));
        plan.items.push(PlanItem {
            component: "workbench-base".into(),
            media: "Workbench3.2".into(),
            from: "S/User-Startup".into(),
            to: "S/User-Startup".into(),
            is_dir: false,
            bytes: starter.len() as u64,
        });
        plan.user_startup = vec![UserStartupContribution {
            component: "amissl".into(),
            lines: vec!["Assign AmiSSL: SYS:Libs/AmiSSL".into()],
        }];
        let root = dir.join("dist");

        let outcome = apply(&plan, &root, &NoProgress).unwrap();

        let content = std::fs::read_to_string(root.join("S").join("User-Startup")).unwrap();
        assert!(
            content.starts_with(std::str::from_utf8(starter).unwrap()),
            "the release's own starter text comes first and unchanged"
        );
        assert!(content.contains(";BEGIN amissl\nAssign AmiSSL: SYS:Libs/AmiSSL\n;END amissl\n"));

        let manifest: DistributionManifest =
            serde_json::from_str(&std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
                .unwrap();
        let records: Vec<_> = manifest
            .files
            .iter()
            .filter(|f| f.path == "S/User-Startup")
            .collect();
        assert_eq!(
            records.len(),
            1,
            "the workbench-base copy's own record must be gone, replaced by \
             the composed file's — not left behind alongside it"
        );
        assert_eq!(records[0].component, "amissl");
        assert_eq!(records[0].sha256, sha256_bytes(content.as_bytes()));

        // Still one real file on disk, not two — `outcome.files` counts
        // disk reality, and must not double-count the starter copy and the
        // composed result as though they were separate files.
        let copied_bytes: u64 = plan
            .items
            .iter()
            .filter(|i| !i.is_dir && i.to != "S/User-Startup")
            .map(|i| i.bytes)
            .sum();
        assert_eq!(outcome.files, 3, "C/LoadModule, C/Other, S/User-Startup");
        assert_eq!(outcome.bytes, copied_bytes + content.len() as u64);
    }

    /// The cancellation checkpoint this step gets is the same shape every
    /// item in the loop above already gets: checked once, before its one
    /// atomic write. Cancelling exactly here — after both of `planned()`'s
    /// two items have landed, before the composed file is ever written —
    /// must report `CancelledPartway { files: 2 }` and leave neither
    /// `S/User-Startup` nor `distribution.json` behind.
    #[test]
    fn cancelling_at_the_user_startup_step_writes_nothing_and_reports_two_files() {
        let (mut plan, dir) = planned();
        plan.user_startup = vec![UserStartupContribution {
            component: "modules-a1200".into(),
            lines: vec!["Assign Foo: SYS:".into()],
        }];
        let root = dir.join("dist");
        let sink = fixtures::CancelAfter::new(2);

        let err = apply(&plan, &root, &sink).unwrap_err();

        assert!(
            matches!(err, CoreError::CancelledPartway { files: 2 }),
            "{err:?}"
        );
        assert!(root.join("C").join("LoadModule").exists());
        assert!(root.join("C").join("Other").exists());
        assert!(
            !root.join("S").join("User-Startup").exists(),
            "the composed file is one atomic write — cancelling before it \
             starts must leave none of it behind"
        );
        assert!(!root.join(MANIFEST_FILE_NAME).exists());
    }

    /// The negative control for every test above: a plan with no
    /// `user_startup` contributions at all — the shape every shipped
    /// component produces today — must never touch `S` in any way. Without
    /// this, a version of `apply` that always wrote an (empty) `S/User-
    /// Startup` regardless of `plan.user_startup` would still pass every
    /// test above, since none of them check for the file's *absence*.
    #[test]
    fn no_contributions_means_the_file_is_never_touched_at_all() {
        let (plan, dir) = planned();
        assert!(plan.user_startup.is_empty(), "sanity");
        let root = dir.join("dist");

        apply(&plan, &root, &NoProgress).unwrap();

        assert!(!root.join("S").exists());
    }

    /// Review item 5: AmigaDOS text is Latin-1, not UTF-8, and this
    /// project's user is Turkish — an accented byte in a media-provided
    /// starter file is ordinary input, not a hypothetical one. `0xE7` alone
    /// is not valid UTF-8 (it is a lead byte that promises two continuation
    /// bytes that never come), so a `String::from_utf8` read of this exact
    /// byte is the one that used to fail the whole install on its very last
    /// step, after every other file had already landed. Latin-1 decodes it
    /// as `ç` — this must not fail at all.
    #[test]
    fn a_latin1_starter_file_does_not_fail_the_install() {
        let (mut plan, dir) = planned();
        let folder = media_folder(&dir);
        let starter: &[u8] = b"; caf\xE7 comment\n";
        fixtures::media(
            &folder,
            "Workbench3.2",
            "wb.adf",
            &[("S/User-Startup", starter, 0)],
        );
        plan.media_paths
            .insert("Workbench3.2".to_string(), folder.join("wb.adf"));
        plan.items.push(PlanItem {
            component: "workbench-base".into(),
            media: "Workbench3.2".into(),
            from: "S/User-Startup".into(),
            to: "S/User-Startup".into(),
            is_dir: false,
            bytes: starter.len() as u64,
        });
        plan.user_startup = vec![UserStartupContribution {
            component: "amissl".into(),
            lines: vec!["Assign AmiSSL: SYS:Libs/AmiSSL".into()],
        }];
        let root = dir.join("dist");

        // Must not fail — that is the whole point of the fix. Before it,
        // this returned `CoreError::Malformed` here.
        let outcome = apply(&plan, &root, &NoProgress).unwrap();

        let disk_bytes = std::fs::read(root.join("S").join("User-Startup")).unwrap();
        assert!(
            disk_bytes.starts_with(starter),
            "the release's own Latin-1 starter text survives byte for byte, \
             including the non-UTF-8 byte"
        );
        assert!(disk_bytes
            .windows(b";BEGIN amissl".len())
            .any(|w| w == b";BEGIN amissl"));

        // The manifest's own bytes/sha256 must describe what is really on
        // disk (Latin-1-encoded bytes), not the UTF-8 length of the Rust
        // `String` used internally to compose it — those two only coincide
        // here because the *new* block content happens to be plain ASCII;
        // the starter's own non-ASCII byte is what would expose a mismatch.
        let manifest: DistributionManifest =
            serde_json::from_str(&std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
                .unwrap();
        let record = manifest
            .files
            .iter()
            .find(|f| f.path == "S/User-Startup")
            .unwrap();
        assert_eq!(record.bytes, disk_bytes.len() as u64);
        assert_eq!(record.sha256, sha256_bytes(&disk_bytes));

        // `ApplyOutcome::bytes` must also agree with disk reality: the sum
        // of every ordinary copied item plus the composed file's real
        // (Latin-1-encoded) size.
        let copied_bytes: u64 = plan
            .items
            .iter()
            .filter(|i| !i.is_dir && i.to != USER_STARTUP_PATH)
            .map(|i| i.bytes)
            .sum();
        assert_eq!(outcome.bytes, copied_bytes + disk_bytes.len() as u64);
    }

    /// **Task 14 — the real run.** Drives the whole engine — `find_media`
    /// (through `plan()`'s own call), `plan()`, `apply()` — against the
    /// user's own AmigaOS 3.2 media and a real Kickstart ROM, never a
    /// synthetic fixture. Skipped cleanly unless all three environment
    /// variables are set, the same convention `core/preload/native.rs`'s
    /// oracle hooks (`ART_PFS3_WRITE_OUT`, `ART_PFS3_READ_IN`) already use:
    ///
    /// ```text
    /// ART_OSINSTALL_MEDIA="E:\amiga\Amigatolon\paketler\3.2\AmigaOs 3.2\ADF" ^
    /// ART_OSINSTALL_ROM="E:\amiga\Amigatolon\kickstart\Kickstart v3.1 rev 40.68 (1993)(Commodore)(A1200).rom" ^
    /// ART_OSINSTALL_DEST="E:\amiga\ProjeART\dist-3.2" ^
    /// cargo test run_the_real_engine_against_the_users_own_media_when_asked -- --nocapture --ignored
    /// ```
    ///
    /// (`--ignored` because this test is also marked `#[ignore]` below —
    /// belt and braces: even with the env vars unset by accident, a plain
    /// `cargo test` run must never touch anything outside the repo's own
    /// tempdir, which is this project's own standing fixture rule.)
    ///
    /// `chosen` names every component the shipped recipe marks reachable
    /// (`available: true` and no `condition`) except the two the engine
    /// decides for itself: `workbench-base` (`required`, always on) and
    /// `modules-a1200` (`condition: rom-older-than 47`), which is
    /// deliberately left **out** of `chosen` — the point of this run is to
    /// prove the condition switches it on by itself against the user's real
    /// V40 Kickstart, not to force it on the way `chosen` would.
    /// `update-3.2.1` is left out because it is `available: false` in the
    /// recipe (registered, not implemented — see `CLAUDE.md`'s "don't claim
    /// support that isn't implemented and tested"), so a real screen would
    /// never offer it. `backdrops` **is** chosen now: it stopped being a
    /// guess when the running system named its own path (ART-127), and a
    /// tree without it boots to a Preferences error about a missing
    /// wallpaper.
    #[test]
    #[ignore = "touches the user's real media and E:\\amiga\\ProjeART; run explicitly, see the doc comment"]
    fn run_the_real_engine_against_the_users_own_media_when_asked() {
        let (Ok(media), Ok(rom), Ok(dest)) = (
            std::env::var("ART_OSINSTALL_MEDIA"),
            std::env::var("ART_OSINSTALL_ROM"),
            std::env::var("ART_OSINSTALL_DEST"),
        ) else {
            return;
        };

        let chosen: Vec<String> = [
            "extras",
            "locale-base",
            "locale-de",
            "locale-dk",
            "locale-en",
            "locale-es",
            "locale-fr",
            "locale-gr",
            "locale-it",
            "locale-nl",
            "locale-no",
            "locale-pl",
            "locale-pt",
            "locale-ru",
            "locale-se",
            "locale-tr",
            "locale-uk",
            "fonts",
            "classes",
            "glowicons",
            "diskdoctor",
            "mmulibs",
            "hdtools",
            "storage",
            "backdrops",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let request = crate::core::osinstall::plan::InstallRequest {
            media_folder: PathBuf::from(&media),
            rom: Some(PathBuf::from(&rom)),
            chosen,
            excluded: Vec::new(),
            destination: PathBuf::from(&dest),
        };

        let found = crate::core::osinstall::scan::find_media(&request.media_folder).unwrap();
        println!("find_media: {} volume(s) found", found.len());
        for entry in &found {
            println!("  {} -> {}", entry.volume_name, entry.path.display());
        }

        let recipe = crate::core::osinstall::recipe::amigaos_32().unwrap();
        let planned = crate::core::osinstall::plan::plan(&request, &recipe).unwrap();

        println!("release={}", planned.release);
        println!("components_on={:?}", planned.components_on);
        println!("refusals={:?}", planned.refusals);
        println!("total_bytes={}", planned.total_bytes);
        println!("items={}", planned.items.len());

        let modules_on = planned.components_on.iter().any(|id| id == "modules-a1200");
        println!("modules-a1200 on without being chosen: {modules_on}");

        assert!(
            planned.refusals.is_empty(),
            "the real plan refused: {:?}",
            planned.refusals
        );
        // **Asked of the ROM rather than assumed of it.** The hook was
        // written against the user's 3.1 (V40) dump and asserted
        // `modules-a1200` on, flatly — which made it fail when pointed at
        // their 3.2 (V47) ROM, where the component is correctly *off*. Both
        // ROMs are real material worth running against (the V47 one is what
        // boots the tree in WinUAE), so the assertion now states the rule the
        // condition actually encodes instead of one ROM's answer to it.
        let rom_major = crate::core::rom::stated_version(&std::fs::read(&rom).unwrap())
            .map(|(major, _)| major)
            .expect("the paired Kickstart states its own version");
        println!("rom stated major={rom_major}");
        assert_eq!(
            modules_on,
            rom_major < 47,
            "modules-a1200 must switch on by its own condition — unchosen — \
             exactly when the paired ROM's stated major is below 47, and stay \
             off otherwise. The ROM here states {rom_major}. If this fails, \
             something upstream (ROM read, Cloanto strip, condition_holds) is \
             wrong, which is the finding the brief expects this run to surface"
        );

        // Review fix round 1: these were printed, never asserted, so none of
        // them could regress. The real 36-ADF media set is fixed (it is the
        // user's own material, not expected to change), so the exact counts
        // are asserted rather than left as text a reader has to trust — if
        // this ever fails because the media folder genuinely changed, that
        // is itself worth knowing, not a flaky test to work around.
        // **One set of numbers per ROM, because the ROM changes the plan.**
        // A pre-V47 machine gets `modules-a1200` (LoadModule and the module
        // sets its own condition switched on); a V47 machine correctly does
        // not, and its tree is that much smaller. Pinning one ROM's answer as
        // *the* answer is what made this hook fail the first time it was
        // pointed at the user's 3.2 ROM — the material it was written against
        // was never the only real material.
        let (want_components, want_files, want_dirs, want_bytes) = if rom_major < 47 {
            (28, 3954, 281, 12_700_698)
        } else {
            (27, 3950, 278, 12_651_178)
        };
        assert_eq!(
            planned.components_on.len(),
            want_components,
            "components_on={:?}",
            planned.components_on
        );

        let root = PathBuf::from(&dest);
        let outcome = apply(&planned, &root, &NoProgress).unwrap();
        println!(
            "apply: files={} directories={} bytes={}",
            outcome.files, outcome.directories, outcome.bytes
        );
        assert_eq!(outcome.files, want_files);
        assert_eq!(outcome.directories, want_dirs);
        assert_eq!(outcome.bytes, want_bytes);

        let manifest_text = std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap();
        let manifest: DistributionManifest = serde_json::from_str(&manifest_text).unwrap();
        println!(
            "manifest: {} file(s) recorded from {} medium/media",
            manifest.files.len(),
            manifest.built_from.len()
        );
        assert_eq!(manifest.files.len(), want_files as usize);
        assert_eq!(manifest.built_from.len(), want_components);
    }

    /// Throwaway diagnostic, not part of the suite's real coverage — lists
    /// what is actually inside three of the user's real ADFs, to diagnose
    /// the refusals `run_the_real_engine_against_the_users_own_media_when_asked`
    /// found on the first real run. Same env-var-gated skip convention.
    #[test]
    #[ignore = "diagnostic only; touches the user's real media"]
    fn inspect_real_media_when_asked() {
        let Ok(media) = std::env::var("ART_OSINSTALL_MEDIA") else {
            return;
        };
        let folder = PathBuf::from(&media);
        let found = crate::core::osinstall::scan::find_media(&folder).unwrap();
        for volume in ["Storage3.2", "Classes3.2", "GlowIcons3.2"] {
            let entry = found.iter().find(|f| f.volume_name == volume).unwrap();
            let mut source = AdfSource::open(&entry.path).unwrap();
            let mut all = source.walk("").unwrap();
            all.sort_by(|a, b| a.path.cmp(&b.path));
            println!("--- {volume} ({}) ---", entry.path.display());
            for e in &all {
                println!("  {}{}", e.path, if e.is_dir { "/" } else { "" });
            }
        }
    }

    /// Throwaway diagnostic (Task 4): lists what the real 3.9 disc actually
    /// holds near its root, to find the real path the shipped recipe's
    /// `OS-Version3.9/Workbench3.5/*` rules got wrong. Same env-var gate as
    /// `build_the_real_39_tree_when_asked`, deleted once its answer is known.
    #[test]
    #[ignore = "diagnostic only; touches the user's real media"]
    fn inspect_real_39_disc_when_asked() {
        let Ok(iso) = std::env::var("ART_OS39_ISO") else {
            return;
        };
        let mut source = CdSource::open(&PathBuf::from(&iso)).unwrap();
        println!("volume_name={}", source.volume_name());
        let mut root = source.walk("").unwrap();
        root.sort_by(|a, b| a.path.cmp(&b.path));
        println!("--- OS-VERSION3.9/WORKBENCH3.9 direct children ---");
        for e in &root {
            if e.path.starts_with("OS-VERSION3.9/WORKBENCH3.9/") && e.path.matches('/').count() == 2
            {
                println!("  {}{}", e.path, if e.is_dir { "/" } else { "" });
            }
        }
        for candidate in [
            "OS-Version3.9",
            "OS-Version3.9/Workbench3.5",
            "Workbench3.5",
            "OS-VERSION3.9/WORKBENCH3.9",
            "OS-VERSION3.9/WORKBENCH3.9/C",
            "OS-VERSION3.9/WORKBENCH3.9/L",
        ] {
            match source.entry(candidate) {
                Ok(Some(e)) => println!("{candidate} -> present, is_dir={}", e.is_dir),
                Ok(None) => println!("{candidate} -> absent"),
                Err(e) => println!("{candidate} -> error: {e}"),
            }
        }

        println!("--- anywhere on the disc named L, REXXC or EXPANSION ---");
        for e in &root {
            let last = e.path.rsplit('/').next().unwrap_or("");
            if e.is_dir && (last == "L" || last == "REXXC" || last == "EXPANSION") {
                println!("  {}", e.path);
            }
        }

        println!("--- OS-VERSION3.9/WORKBENCH3.5 direct children ---");
        for e in &root {
            if e.path.starts_with("OS-VERSION3.9/WORKBENCH3.5/") && e.path.matches('/').count() == 2
            {
                println!("  {}{}", e.path, if e.is_dir { "/" } else { "" });
            }
        }
    }

    /// Throwaway diagnostic (Task 4, fix round 1): `apply()`'s real write
    /// against the disc failed with Windows error 123 (`InvalidFilename`) —
    /// this walks every name `workbench-base`'s rules actually resolve to
    /// under `OS-VERSION3.9/WORKBENCH3.5` and flags anything Windows'
    /// `CreateFile` would refuse: the reserved characters
    /// (`< > : " | ? *`), a trailing dot or space in any segment, or one of
    /// the reserved device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1-9`,
    /// `LPT1-9`), matching or not matching its extension.
    #[test]
    #[ignore = "diagnostic only; touches the user's real media"]
    fn find_windows_illegal_names_when_asked() {
        let Ok(iso) = std::env::var("ART_OS39_ISO") else {
            return;
        };
        let mut source = CdSource::open(&PathBuf::from(&iso)).unwrap();
        let all = source.walk("OS-VERSION3.9/WORKBENCH3.5").unwrap();

        const RESERVED: &[&str] = &[
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ];

        let mut flagged = 0;
        for e in &all {
            for segment in e.path.split('/') {
                let bad_char = segment
                    .chars()
                    .any(|c| "<>:\"|?*".contains(c) || (c as u32) < 32);
                let trailing = segment.ends_with('.') || segment.ends_with(' ');
                let stem = segment.split('.').next().unwrap_or(segment);
                let reserved = RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem));
                if bad_char || trailing || reserved {
                    flagged += 1;
                    println!(
                        "FLAGGED: {} (segment '{segment}': bad_char={bad_char} trailing={trailing} reserved={reserved})",
                        e.path
                    );
                }
            }
        }
        println!("{flagged} flagged segment(s) out of {} entries", all.len());
    }

    /// Throwaway diagnostic (Task 5, Step 1): the exact bytes behind the
    /// three `?`-bearing `.country` names ART-155 names. `CdSource`/
    /// `IsoImage::list` already decode a name before handing it back, so
    /// this walks the same directory *underneath* that decoding — reading
    /// the raw sector by hand and printing each flagged identifier's own
    /// bytes in hex beside what `decode_iso646` made of them — because
    /// Step 2's charset question can only be answered from the bytes the
    /// disc actually carries, never from ART's own already-lossy `?`.
    #[test]
    #[ignore = "diagnostic only; touches the user's real media"]
    fn read_the_raw_country_name_bytes_when_asked() {
        let Ok(iso) = std::env::var("ART_OS39_ISO") else {
            return;
        };
        let image = crate::core::iso::IsoImage::open(&PathBuf::from(&iso)).unwrap();

        // Descend by name. Every segment down to COUNTRIES-EURO itself is
        // plain ASCII (confirmed by ART-155's own report), so decoding loses
        // nothing before the leaf directory this diagnostic actually cares
        // about — matched case-insensitively because the disc's Primary
        // tree mixes case (`OS-VERSION3.9` beside `STORAGE`) rather than
        // sticking to strict ISO9660 Level 1 uppercase.
        let (mut extent, mut length) = image.root();
        for segment in [
            "OS-VERSION3.9",
            "WORKBENCH3.5",
            "STORAGE",
            "LOCALE",
            "COUNTRIES-EURO",
        ] {
            let entries = image.list(extent, length).unwrap();
            let found = entries
                .iter()
                .find(|e| e.name.eq_ignore_ascii_case(segment))
                .unwrap_or_else(|| {
                    panic!(
                        "{segment} not found; entries here: {:?}",
                        entries.iter().map(|e| &e.name).collect::<Vec<_>>()
                    )
                });
            assert!(found.is_dir, "{segment} is not a directory");
            extent = found.extent;
            length = found.bytes as u32;
        }

        // `extent`/`length` now name COUNTRIES-EURO itself. Read its raw
        // sectors and walk records by hand — the same record shape
        // `directory::parse_directory_extent` walks, but keeping the
        // identifier bytes instead of feeding them through `decode_iso646`,
        // which is exactly the step this diagnostic exists to look behind.
        use std::io::{Read, Seek, SeekFrom};
        let layout = image.layout();
        let sector = crate::core::iso::LOGICAL_SECTOR_SIZE as u64;
        let sectors = (length as u64).div_ceil(sector);
        let mut file = std::fs::File::open(&iso).unwrap();
        let mut buf = vec![0u8; (sectors * sector) as usize];
        file.seek(SeekFrom::Start(layout.data_offset_of(extent).unwrap()))
            .unwrap();
        file.read_exact(&mut buf).unwrap();

        let mut pos = 0usize;
        let mut printed = 0;
        while pos < buf.len() {
            let len = buf[pos] as usize;
            if len == 0 {
                // Padding to the next sector, not a record — same rule
                // `directory::parse_directory_extent` follows.
                pos = pos.div_ceil(2048) * 2048;
                continue;
            }
            let id_len = buf[pos + 32] as usize;
            let id = &buf[pos + 33..pos + 33 + id_len];
            let is_dot_or_dotdot = id_len == 1 && (id[0] == 0x00 || id[0] == 0x01);
            if !is_dot_or_dotdot {
                let decoded = crate::core::iso::descriptor::decode_iso646(id);
                if decoded.contains('?') {
                    let hex: Vec<String> = id.iter().map(|b| format!("{b:02x}")).collect();
                    println!("decoded={decoded:?} raw_hex=[{}]", hex.join(" "));
                    printed += 1;
                }
            }
            pos += len;
        }
        println!("{printed} entries with '?' found under COUNTRIES-EURO");
    }

    /// Throwaway investigation (Task 5, Step 5), kept as the evidence for
    /// **ART-156**: `build_the_real_39_tree_when_asked` wrote 54,094 fewer
    /// bytes than `plan()` predicted even though every one of the 663
    /// planned items became a real file or a real directory and every real
    /// *file*'s size matches its `PlanItem::bytes` exactly (checked below,
    /// per item, against the previous run's output — never deleted, so
    /// `ART_OS39_DEST` still holds it). The 75 directory items are the
    /// difference: `PlanItem::bytes` for a directory sourced from a CD is
    /// `IsoEntry::bytes` — the ISO9660 directory record's own declared
    /// extent length, a real, nonzero, sector-rounded number, not the `0`
    /// an `AdfSource` directory reports — but a directory becomes a plain
    /// host folder with no "content" of its own, so `plan::total_bytes`
    /// (`items.iter().map(|i| i.bytes).sum()`, unconditionally over every
    /// item) counts bytes for 75 directories that `apply()` correctly never
    /// writes anywhere. Not a naming defect and not `decode_iso646` — a
    /// distinct, pre-existing miscount in `core/osinstall/plan.rs` that
    /// this task's fix to ART-155 simply let `apply()` run far enough to
    /// expose for the first time. Not fixed here (out of this task's scope
    /// — `plan.rs` is not in its file list — and "one measured defect per
    /// task").
    #[test]
    #[ignore = "diagnostic only; touches the user's real media and a prior real run's output"]
    fn find_the_directory_byte_overcount_when_asked() {
        let (Ok(iso), Ok(dest)) = (
            std::env::var("ART_OS39_ISO"),
            std::env::var("ART_OS39_DEST"),
        ) else {
            return;
        };
        let iso_path = PathBuf::from(&iso);
        let media_folder = iso_path.parent().unwrap().to_path_buf();
        let request = crate::core::osinstall::plan::InstallRequest {
            media_folder,
            rom: None,
            chosen: Vec::new(),
            excluded: Vec::new(),
            destination: PathBuf::from(&dest),
        };
        let recipe = crate::core::osinstall::recipe::amigaos_39().unwrap();
        let planned = crate::core::osinstall::plan::plan(&request, &recipe).unwrap();

        let root = PathBuf::from(&dest);
        let mut sum_file_bytes: u64 = 0;
        let mut sum_dir_bytes: u64 = 0;
        let mut mismatches = 0;
        for item in &planned.items {
            if item.is_dir {
                sum_dir_bytes += item.bytes;
                continue;
            }
            sum_file_bytes += item.bytes;
            match std::fs::metadata(root.join(&item.to)) {
                Ok(meta) if meta.len() == item.bytes => {}
                Ok(meta) => {
                    mismatches += 1;
                    println!(
                        "MISMATCH {} planned={} actual={}",
                        item.to,
                        item.bytes,
                        meta.len()
                    );
                }
                Err(err) => {
                    mismatches += 1;
                    println!("MISSING {} ({err})", item.to);
                }
            }
        }
        println!(
            "sum_file_bytes={sum_file_bytes} sum_dir_bytes={sum_dir_bytes} \
             total_bytes={} (= sum_file_bytes + sum_dir_bytes: {}) per-file mismatches={mismatches}",
            planned.total_bytes,
            sum_file_bytes + sum_dir_bytes == planned.total_bytes
        );
    }

    /// **Task 4 (amigaos-39 plan), the real run.** Drives the whole engine —
    /// `find_media` (through `plan()`'s own call), `plan()`, `apply()` —
    /// against the owner's own AmigaOS 3.9 CD image, never a synthetic
    /// fixture. Modelled exactly on
    /// `run_the_real_engine_against_the_users_own_media_when_asked` above:
    /// same env-var-gated skip, same doubled `#[ignore]` belt-and-braces, same
    /// shape of report.
    ///
    /// `ART_OS39_ISO` names the disc image itself; `find_media` is handed its
    /// **parent folder**, not a folder holding only this one file — the
    /// owner's real `iso/` folder also holds `AmigaOS3.2CD(ZaP).iso` and other
    /// discs beside it, on purpose (see the task dispatch), so this run is
    /// what proves the scan is not confused by the neighbour rather than
    /// assuming a tidy folder that does not exist on this machine.
    ///
    /// No ROM is supplied: the shipped 3.9 recipe's one component
    /// (`workbench-base`) is `required` and carries no `Condition`, so a plan
    /// against it needs nothing decided from a Kickstart.
    ///
    /// ```text
    /// cd src-tauri && ART_OS39_ISO="E:\amiga\Amigatolon\iso\AmigaOS39.iso" \
    ///   ART_OS39_DEST="E:\amiga\ProjeART\dist-3.9" \
    ///   cargo test build_the_real_39_tree_when_asked -- --nocapture --ignored
    /// ```
    ///
    /// **Measured against the owner's real 469 MiB disc (Task 4, fix round
    /// 1 — after ART-153 was fixed so `apply()` can open a disc at all):**
    /// `find_media` sees all 4 discs in the shared `iso/` folder and resolves
    /// `AmigaOS3.9` correctly, never confused by the neighbouring
    /// `AmigaOS3.2CD(ZaP)`; `plan()` succeeds clean — 1 component on
    /// (`workbench-base`), 0 refusals, 663 items, 6 108 319 planned bytes.
    ///
    /// At that point `apply()` reached 1,020 files and 71 directories before
    /// hitting **ART-155**: a real disc name it could not write as a literal
    /// Windows path segment — three `Storage/Locale/Countries-Euro/*.country`
    /// files whose accented letters `core/iso/descriptor.rs::decode_iso646`
    /// rendered as `?`, a character Windows refuses in a path.
    ///
    /// **Task 5 fixed ART-155's real cause** (`decode_iso646` now decodes a
    /// high-bit byte as ISO-8859-1 instead of `?` — see that function's own
    /// doc comment for the full case) **and corrected its other named
    /// cause**: a reserved DOS device name, `Storage/DOSDrivers/AUX`, was
    /// measured on this machine (Windows 11 Pro 26200) to write and list
    /// back just fine — it was never what failed. Re-run against the same
    /// disc, `apply()` now completes: 588 files, 75 directories, exactly the
    /// 663 planned items, every file's bytes matching its plan exactly. The
    /// one number that does *not* match is `planned.total_bytes` itself —
    /// 6,108,319 against 6,054,225 actually written — and that turned out to
    /// be a second, distinct, previously-unreachable defect: `total_bytes`
    /// sums `PlanItem::bytes` over directories too, and a CD-sourced
    /// directory's `bytes` is its ISO9660 extent length, not `0`. Filed as
    /// **ART-156**, not fixed here — see
    /// `find_the_directory_byte_overcount_when_asked` above and ART-156 in
    /// `docs/ISSUES.md`.
    #[test]
    #[ignore = "touches the user's real media and E:\\amiga\\ProjeART; run explicitly, see the doc comment"]
    fn build_the_real_39_tree_when_asked() {
        let (Ok(iso), Ok(dest)) = (
            std::env::var("ART_OS39_ISO"),
            std::env::var("ART_OS39_DEST"),
        ) else {
            return;
        };

        let iso_path = PathBuf::from(&iso);
        let media_folder = iso_path
            .parent()
            .expect("ART_OS39_ISO names a file inside some folder")
            .to_path_buf();

        let found = crate::core::osinstall::scan::find_media(&media_folder).unwrap();
        println!(
            "find_media: {} volume(s) found in {}",
            found.len(),
            media_folder.display()
        );
        for entry in &found {
            println!(
                "  {:?} {} -> {}",
                entry.kind,
                entry.volume_name,
                entry.path.display()
            );
        }

        let request = crate::core::osinstall::plan::InstallRequest {
            media_folder,
            rom: None,
            chosen: Vec::new(),
            excluded: Vec::new(),
            destination: PathBuf::from(&dest),
        };

        let recipe = crate::core::osinstall::recipe::amigaos_39().unwrap();
        let planned = crate::core::osinstall::plan::plan(&request, &recipe).unwrap();

        println!("release={}", planned.release);
        println!("components_on={:?}", planned.components_on);
        println!("refusals={:?}", planned.refusals);
        println!("total_bytes={}", planned.total_bytes);
        println!("items={}", planned.items.len());

        assert!(
            planned.refusals.is_empty(),
            "the real plan refused: {:?}",
            planned.refusals
        );
        assert_eq!(
            planned.components_on,
            vec!["workbench-base".to_string()],
            "the shipped 3.9 recipe carries exactly one component today"
        );

        // ART-153 fixed (fix round 1): `apply()` now opens each medium
        // through `scan::identify`/`scan::open_media`, the same probe
        // `find_media` uses, so this reaches a real write against the disc
        // rather than failing at the first byte.
        let start = std::time::Instant::now();
        let root = PathBuf::from(&dest);
        match apply(&planned, &root, &NoProgress) {
            Ok(outcome) => {
                let elapsed = start.elapsed();
                println!(
                    "apply: files={} directories={} bytes={} in {:.2}s",
                    outcome.files,
                    outcome.directories,
                    outcome.bytes,
                    elapsed.as_secs_f64()
                );
                // `planned.total_bytes` is *not* compared against
                // `outcome.bytes` here — that is ART-156
                // (`find_the_directory_byte_overcount_when_asked` above): a
                // CD-sourced directory's `PlanItem::bytes` is its ISO9660
                // extent length, not `0`, so `total_bytes` (summed over every
                // item, files and directories alike) overstates what
                // `apply()` actually writes as file content by exactly the
                // 75 directories' worth of extent bytes. Comparing
                // `outcome.bytes` against the sum of *file* items only is the
                // invariant that actually holds — checked below.
                println!(
                    "plan.total_bytes={} vs apply.outcome.bytes={} (difference={})",
                    planned.total_bytes,
                    outcome.bytes,
                    outcome.bytes as i64 - planned.total_bytes as i64
                );
                let planned_file_bytes: u64 = planned
                    .items
                    .iter()
                    .filter(|item| !item.is_dir)
                    .map(|item| item.bytes)
                    .sum();
                assert_eq!(
                    outcome.bytes, planned_file_bytes,
                    "apply() wrote a different byte total than plan()'s file items predicted"
                );

                // This disc is real, fixed material (the owner's own AmigaOS
                // 3.9 CD): these exact counts are pinned, not just checked
                // non-zero, the same way
                // `run_the_real_engine_against_the_users_own_media_when_asked`
                // pins its own real media's counts — a change here means the
                // media folder or the recipe genuinely changed, worth
                // knowing rather than a flaky assertion to loosen.
                assert_eq!(outcome.files, 588, "files written");
                assert_eq!(outcome.directories, 75, "directories written");
                assert_eq!(outcome.bytes, 6_054_225, "file bytes written");

                let manifest_text = std::fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap();
                let manifest: DistributionManifest = serde_json::from_str(&manifest_text).unwrap();
                println!(
                    "manifest: {} file(s) recorded from {} medium/media",
                    manifest.files.len(),
                    manifest.built_from.len()
                );
                assert_eq!(manifest.files.len(), outcome.files as usize);
                assert_eq!(manifest.built_from.len(), planned.components_on.len());
            }
            Err(err) => {
                // ART-153 and ART-155 are both fixed as of this task — a
                // failure reaching here is a *new*, as-yet-unfiled defect,
                // not either of those. `root` is left as `apply()` stopped
                // it (writes commit as they land, never rolled back), so a
                // real, measured partial count is possible even though the
                // run did not finish — matching spec §89 ("report what
                // happened, including what did not work").
                let (files_written, dirs_written) = count_tree(&root);
                println!(
                    "apply failed partway: files_written={files_written} directories_written={dirs_written}"
                );
                panic!(
                    "apply() failed against the real disc: {err}\n\n\
                     Neither ART-153 nor ART-155 (docs/ISSUES.md) — both are fixed as of this \
                     task, and this run's failure is something else. File it as a new ART-NNN \
                     with what actually happened, the same rigour ART-155 itself was filed \
                     with, rather than assuming it is one of the two closed causes above."
                );
            }
        }
    }
}
