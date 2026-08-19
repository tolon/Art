//! What a package would land on, before anything is written (spec §3).
//!
//! Entirely read-only: nothing in this file opens a file for writing, and
//! nothing here calls anywhere that does. It exists so a downgrade — "the
//! whole engine exists for" this, per `ModulesA1200_3.2.adf`'s own thirteen
//! stale commands — is a fact the user sees on screen before it happens,
//! never a fact discovered afterwards by noticing something broke.
//!
//! ## Why `preview` takes [`Incoming`], not a `PlanItem`
//!
//! Fix round 1. The first version of this module reused
//! [`super::plan::PlanItem`] and reinterpreted its `from` field — which
//! `plan.rs` documents, and `apply.rs` uses, as a path relative to a
//! *medium* — as an already-resolved filesystem path instead. Review
//! caught it (F3): a real caller who reasonably assumes `PlanItem`'s own
//! documented meaning gets a hard error on every row, or silently reads
//! whatever happens to sit at a CWD-relative path that is not the
//! incoming file at all. [`Incoming`] exists so that mistake cannot
//! compile: `bytes_at` says, in its own type, "this is a path to read
//! bytes from directly," and nothing here can be confused with a
//! medium-relative `PathRule::from`. Building an `[Incoming]` from a real
//! plan — resolving each candidate collision's medium, reading its bytes,
//! and handing `preview` a real path (the same "read the bytes once, hand
//! back something path-shaped" move `source_archive.rs`'s own
//! `open_nested` already makes for the identical problem) — is the
//! command layer's job, once one exists; this module never opens a medium
//! itself.
//!
//! ## The bound is two different questions, not one
//!
//! **Finding `$VER:`.** [`crate::core::amigaver::read`] scans for the
//! marker wherever it sits, so it needs real bytes, not a fabricated
//! window — but it does not need the *whole* file.
//! `amigaver.rs`'s own `count_version_strings_in_a_real_tree_when_asked`
//! (an `#[ignore]`d, manually-run test, not something CI measures) reads a
//! bounded 1 MiB window on the stated assumption — not a measurement
//! either that test or this module has independently verified — that "the
//! first 1 MiB is enough to hold a `$VER:` marker in any real Amiga file."
//! [`VERSION_SEARCH_BOUND`] reuses that same assumption rather than
//! inventing a second, different one; if it turns out to be wrong for some
//! real file, that is a fact worth finding and fixing in one place, not
//! two (fix round 1, F4).
//!
//! **Whether the bytes are the same at all.** A length comparison is free
//! — no content read at all — and it already answers "identical" for
//! every pair whose sizes differ: two files of different length cannot be
//! byte-for-byte the same, so there is nothing to gain from reading either
//! one whole just to confirm that. Only when the lengths agree, *and*
//! neither exceeds [`MAX_COLLISION_FILE`], is identity even attempted with
//! a real, full-content read; a real AmigaOS file is, at most, a few
//! hundred kilobytes (this is floppy-era software), so that cost is
//! genuinely small. An oversized or size-mismatched pair falls back to the
//! bounded, version-only comparison below — never a whole-preview failure
//! (fix round 1, F7): one unusually large file degrades its own row, not
//! every other answer in the same call.
//!
//! This is why `classify` and the private `classify_by_version` are two
//! functions rather than one: `preview` calls `classify_by_version`
//! directly, never `classify`, whenever a length comparison (or the size
//! ceiling) has already settled the identity question — so a truncated,
//! size-mismatched pair can never reach the `existing == incoming` check
//! that would otherwise risk reporting `Identical` on two files a metadata
//! call already proved are not.
//!
//! ## A version marker names a program, and the wrong one can be found
//!
//! Fix round 1, F1 — a real defect, not a hypothetical one. `$VER:` is
//! found by raw substring search (`amigaver::read`'s own doc comment), so
//! the *first* marker in a file is not necessarily the marker for the
//! program the file's own name says it is — a command can embed, or sit
//! ahead of, another program's own copy of the string. Comparing two
//! files' version *numbers* without first checking that
//! [`amigaver::AmigaVersion::name`] agrees on both sides lets an older
//! `assign` file classify as an *upgrade* purely because whatever marker
//! `read` happened to find first in the incoming file belonged to
//! `dos.library`, not `assign` — an older file behind a green arrow, which
//! is precisely the failure this whole module exists to prevent. So
//! [`classify_by_version`] compares names before numbers, and a mismatch
//! is not a downgrade or an upgrade — it is treated exactly like a version
//! found on only one side: ART has not measured a version *for this
//! program* in that file, so it says nothing rather than guessing (§89).

use std::cmp::Ordering;
use std::collections::HashMap;
use std::io::Read as _;
use std::path::Path;

use serde::Serialize;

use super::apply::{DistributionManifest, MANIFEST_FILE_NAME};
use super::package;
use crate::core::amigaver;
use crate::core::error::{CoreError, CoreResult};
use crate::core::security::path::safe_join;

/// How much of a size-mismatched (or oversized) pair `preview` reads
/// looking for a `$VER:` marker — see the module doc comment's "The bound"
/// section.
const VERSION_SEARCH_BOUND: u64 = 1024 * 1024;

/// The ceiling on a full, whole-file read attempted when two files'
/// lengths already agree — see the module doc comment. Far past any real
/// AmigaOS file; crossing it degrades that one row to the bounded,
/// version-only comparison rather than failing the whole preview (fix
/// round 1, F7).
const MAX_COLLISION_FILE: u64 = 64 * 1024 * 1024;

/// What landing `incoming` over `existing` would actually do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Collision {
    /// The bytes are the same. Not an overwrite at all.
    Identical,
    /// Both sides name the same program, and the incoming one is a
    /// strictly newer version.
    Upgrade { from: String, to: String },
    /// Both sides name the same program, and the incoming one is
    /// **older**. Nothing that is not a proven strictly-newer version is
    /// ever reported [`Collision::Upgrade`] — see
    /// [`classify_by_version`]'s own doc comment for why that direction of
    /// mistake is the one this module refuses to make.
    Downgrade { from: String, to: String },
    /// Both sides name the same program and carry the **same** version
    /// number, over different bytes — a rebuild, a recompression, or a
    /// corrupt copy. Added in spec fix `aa753f3` (fix round 1, F2):
    /// neither older nor newer, so it is not `Downgrade`, and both sides
    /// *do* say what they are, so collapsing it into `Unversioned` would
    /// throw away the one useful fact the row has — that they agree.
    SameVersion {
        version: String,
        from_bytes: u64,
        to_bytes: u64,
    },
    /// One side or both says nothing **about the same program** — no
    /// marker at all, or the two sides' markers name different programs
    /// (see the module doc comment's last section). Sizes, and no invented
    /// version.
    Unversioned { from_bytes: u64, to_bytes: u64 },
}

/// One planned item that would land on something already in the tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollisionReport {
    /// Where in the tree, `/`-separated.
    pub path: String,
    pub collision: Collision,
    /// Whether the package's recipe declared it may write over what is
    /// there — see [`declared_override`].
    pub declared: bool,
}

/// One file [`preview`] is asked to land somewhere in the tree.
///
/// Deliberately not [`super::plan::PlanItem`] — see the module doc
/// comment's "Why `preview` takes `Incoming`" section. `bytes_at` names
/// its own contract: a real, directly-readable path to the incoming
/// bytes, resolved by whoever builds this (opening the package's medium
/// and writing the entry out, or any other real source), never a
/// medium-relative name `preview` would have no way to resolve itself.
#[derive(Debug, Clone)]
pub struct Incoming<'a> {
    /// Where it would land in the tree, `/`-separated — matches
    /// [`super::plan::PlanItem::to`].
    pub to: String,
    /// The id of the component this file would come from — for a package,
    /// the package's own id (see [`declared_override`]'s doc comment).
    pub component: String,
    /// A real path to the incoming bytes.
    pub bytes_at: &'a Path,
}

/// Turn an [`amigaver::AmigaVersion`] into the `"version.revision"` text
/// [`Collision::Upgrade`]/[`Collision::Downgrade`]/[`Collision::SameVersion`]
/// carry.
///
/// These are the **parsed** integers, not the file's own literal digits
/// (fix round 1, F8) — `AmigaVersion::version`/`::revision` are already
/// `u32`, so `$VER: thing 007.4` becomes `"7.4"` here, the same leading
/// zero `AmigaVersion` itself already discarded on the way in. This shows
/// the version the file names, not a reproduction of its exact text.
fn version_label(version: &amigaver::AmigaVersion) -> String {
    format!("{}.{}", version.version, version.revision)
}

/// Classify what landing `incoming` over `existing` would do, from bytes
/// already in hand.
///
/// `existing == incoming` is checked first and unconditionally: two equal
/// byte slices are the same file regardless of what either one's `$VER:`
/// marker says, and answering that question needs the full content of
/// both — [`preview`] is the one place that decides how much of a real
/// file is worth reading to reach this call; see the module doc comment.
pub fn classify(existing: &[u8], incoming: &[u8]) -> Collision {
    if existing == incoming {
        return Collision::Identical;
    }
    classify_by_version(existing, incoming)
}

/// The version-only half of [`classify`]: never returns
/// [`Collision::Identical`], because the caller either already knows the
/// bytes are unequal (`classify`'s own check) or already knows, more
/// cheaply, that they cannot possibly be equal (`preview`'s length
/// comparison, or its size ceiling) — see the module doc comment for why
/// that split exists.
///
/// A version is only ever compared between two markers that **name the
/// same program** — see the module doc comment's last section for the
/// real input that made this necessary (fix round 1, F1). When the names
/// agree, [`amigaver::AmigaVersion::compare_version`] decides: strictly
/// greater is [`Collision::Upgrade`]; strictly less is
/// [`Collision::Downgrade`]; equal is [`Collision::SameVersion`] (fix
/// round 1, F2 — neither an upgrade nor a downgrade, and not nothing
/// either, since both sides do say what they are). Everything else — a
/// version on only one side, no version on either, or two markers naming
/// different programs — falls back to sizes, never an invented or
/// mismatched-program version (§89).
fn classify_by_version(existing: &[u8], incoming: &[u8]) -> Collision {
    match (amigaver::read(existing), amigaver::read(incoming)) {
        (Some(from), Some(to)) if from.name == to.name => match to.compare_version(&from) {
            Ordering::Greater => Collision::Upgrade {
                from: version_label(&from),
                to: version_label(&to),
            },
            Ordering::Equal => Collision::SameVersion {
                version: version_label(&to),
                from_bytes: existing.len() as u64,
                to_bytes: incoming.len() as u64,
            },
            Ordering::Less => Collision::Downgrade {
                from: version_label(&from),
                to: version_label(&to),
            },
        },
        _ => Collision::Unversioned {
            from_bytes: existing.len() as u64,
            to_bytes: incoming.len() as u64,
        },
    }
}

/// Read at most `bound` bytes of `path` — never the whole file when only a
/// leading window is needed (CLAUDE.md's own rule, applied here to an
/// ordinary tree file rather than a disk image).
fn read_bounded(path: &Path, bound: u64) -> CoreResult<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(bound).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// A real `existing_bytes`/`incoming_bytes` size correction, applied after
/// classification: [`Collision::Unversioned`] and [`Collision::SameVersion`]
/// both carry byte counts, and when the comparison went through the
/// bounded, version-only path (`classify_by_version` on a truncated
/// window) those counts would otherwise be whatever the window happened to
/// hold rather than the file's real, measured size. The tree's own
/// `std::fs::metadata` sizes are always what gets reported — never a
/// truncated read's `.len()` — regardless of which path produced the
/// classification.
fn with_real_sizes(collision: Collision, existing_len: u64, incoming_len: u64) -> Collision {
    match collision {
        Collision::Unversioned { .. } => Collision::Unversioned {
            from_bytes: existing_len,
            to_bytes: incoming_len,
        },
        Collision::SameVersion { version, .. } => Collision::SameVersion {
            version,
            from_bytes: existing_len,
            to_bytes: incoming_len,
        },
        other => other,
    }
}

/// What one [`Incoming`] item would do to the tree, or `None` when there is
/// nothing to report: the destination is empty, or the bytes are identical
/// (see [`preview`]'s own doc comment for why `Identical` is excluded
/// rather than listed).
fn classify_incoming(
    tree_root: &Path,
    entry: &Incoming<'_>,
    manifest: &mut Option<DistributionManifest>,
) -> CoreResult<Option<CollisionReport>> {
    let target = safe_join(tree_root, &entry.to).map_err(|err| {
        CoreError::SafetyRefused(format!(
            "'{}' does not stay inside the distribution root: {err}",
            entry.to
        ))
    })?;
    if !target.is_file() {
        return Ok(None);
    }
    if !entry.bytes_at.is_file() {
        // Nothing to compare against — never invent a byte count or a
        // version for a file that is not actually there (§89).
        return Err(CoreError::InvalidInput(format!(
            "'{}' names an incoming source that does not exist: '{}'",
            entry.to,
            entry.bytes_at.display()
        )));
    }

    let existing_len = std::fs::metadata(&target)?.len();
    let incoming_len = std::fs::metadata(entry.bytes_at)?.len();
    let small_enough = existing_len <= MAX_COLLISION_FILE && incoming_len <= MAX_COLLISION_FILE;

    let collision = if existing_len == incoming_len && small_enough {
        // Lengths agree and both are small enough to read whole: identity
        // is on the table, and only a full read answers it honestly — see
        // the module doc comment.
        let existing_bytes = std::fs::read(&target)?;
        let incoming_bytes = std::fs::read(entry.bytes_at)?;
        classify(&existing_bytes, &incoming_bytes)
    } else {
        // Either the lengths already disagree (identity is settled without
        // reading either file whole) or at least one file is too large to
        // safely prove identity from a full read (fix round 1, F7 — this
        // row degrades to the bounded comparison rather than the whole
        // preview failing). Either way `classify_by_version`, never
        // `classify`, so a coincidentally-equal truncated prefix can never
        // be reported `Identical`. See the module doc comment.
        let existing_bytes = read_bounded(&target, VERSION_SEARCH_BOUND)?;
        let incoming_bytes = read_bounded(entry.bytes_at, VERSION_SEARCH_BOUND)?;
        classify_by_version(&existing_bytes, &incoming_bytes)
    };
    let collision = with_real_sizes(collision, existing_len, incoming_len);

    if collision == Collision::Identical {
        return Ok(None);
    }

    let declared = declared_override(tree_root, manifest, &entry.to, &entry.component)?;
    Ok(Some(CollisionReport {
        path: entry.to.clone(),
        collision,
        declared,
    }))
}

/// What every item in `incoming` would land on inside the tree already
/// built at `tree_root` — the preview §3 requires. Nothing here writes
/// anything.
///
/// Only a destination that already exists is worth a report at all, and
/// [`Collision::Identical`] is excluded from the result for the same
/// reason it exists as its own variant — see that type's own doc comment:
/// it is not an overwrite, and keeping it in the list would bury the rows
/// that are.
///
/// **Two entries naming the same `to`.** (Fix round 1, F9.) The **last**
/// one wins, matching what would actually land on disk if this plan were
/// applied — an earlier entry's own row is replaced, not kept alongside
/// it, the same "the winner is the one that wrote last" rule `apply.rs`'s
/// own `written` map already applies for exactly the same reason
/// (ART-124: two records for one file means one of them describes bytes
/// nobody wrote). First-seen order is kept for the result's own ordering,
/// so a caller grouping by class does not see rows reshuffle depending on
/// which entry happened to win; only the *data* in that slot changes.
///
/// Every destination is resolved through [`safe_join`] before it is ever
/// read, the same discipline `apply.rs`'s own module doc comment states
/// for writing: a recipe's `to` is data a human typed, and a `../` in a
/// text box is the same hole a `../` in a zip is, whichever direction the
/// bytes travel.
pub fn preview(tree_root: &Path, incoming: &[Incoming<'_>]) -> CoreResult<Vec<CollisionReport>> {
    // Loaded at most once, lazily — most calls preview a plan against a
    // tree where most items land on nothing or on identical bytes, and
    // neither of those needs the manifest at all.
    let mut manifest: Option<DistributionManifest> = None;

    // `Option` slots rather than a plain `Vec<CollisionReport>`: a later
    // entry for a `to` already seen can resolve to "nothing to report"
    // (identical, or the destination turned out absent), which has to be
    // able to *remove* an earlier row, not just fail to add a new one. See
    // this function's own doc comment on the dedup rule.
    let mut slots: Vec<Option<CollisionReport>> = Vec::new();
    let mut index_by_path: HashMap<String, usize> = HashMap::new();

    for entry in incoming {
        let row = classify_incoming(tree_root, entry, &mut manifest)?;
        match index_by_path.get(entry.to.as_str()) {
            Some(&at) => slots[at] = row,
            None => {
                index_by_path.insert(entry.to.clone(), slots.len());
                slots.push(row);
            }
        }
    }

    Ok(slots.into_iter().flatten().collect())
}

/// Whether `component` — the incoming item's own component id, which for a
/// package is the package's own id ([`super::package::RawPackage::into_package`]
/// sets `component.id` to it) — declares, in its own `overrides`, the
/// component that put the existing file at `to` there.
///
/// The owner is read from `distribution.json`, never guessed (§89):
/// measured directly, both shipped BoingBags declare
/// `overrides: ["workbench-base"]` and 100% of their files land on paths
/// `workbench-base` placed, so a correct reading of this makes `declared`
/// true on essentially every one of their rows — see `package.rs`'s own
/// module doc comment for the measurement. `manifest` is loaded into the
/// caller's own `Option` at most once per [`preview`] call, since every
/// collision in one call reads the same tree's one manifest.
///
/// `component` naming no shipped package is refused rather than silently
/// answered `false` (fix round 1, F5): a component id that resolves to
/// nothing is a real inconsistency in whatever built `incoming`, not a
/// fact about this file, and swallowing it would make every one of that
/// caller's rows read as "nothing declared" for a reason that has nothing
/// to do with what was actually declared.
fn declared_override(
    tree_root: &Path,
    manifest: &mut Option<DistributionManifest>,
    to: &str,
    component: &str,
) -> CoreResult<bool> {
    if manifest.is_none() {
        let text = std::fs::read_to_string(tree_root.join(MANIFEST_FILE_NAME))?;
        let parsed: DistributionManifest =
            serde_json::from_str(&text).map_err(|err| CoreError::Malformed {
                format: "distribution manifest".into(),
                detail: err.to_string(),
            })?;
        *manifest = Some(parsed);
    }

    let owner = manifest
        .as_ref()
        .expect("just populated above if it was empty")
        .files
        .iter()
        .find(|file| file.path == to)
        .map(|file| file.component.as_str());

    let Some(owner) = owner else {
        // The tree holds a file `distribution.json` never recorded — not
        // something any shipped component could have written, so nothing
        // could have declared an override over it either.
        return Ok(false);
    };

    let package = package::by_id(component).map_err(|err| {
        CoreError::InvalidInput(format!(
            "'{component}' does not name a shipped package, so ART cannot say whether it \
             declared an override for '{to}': {err}"
        ))
    })?;
    Ok(package.component.overrides.iter().any(|over| over == owner))
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::osinstall::apply::{FileRecord, MediaRecord};
    use crate::core::osinstall::fixtures;

    // ---- `classify` ----

    /// Same bytes is not an overwrite, and a user asked to confirm one has
    /// been asked a question with no content.
    #[test]
    fn identical_bytes_are_not_an_overwrite() {
        assert_eq!(classify(b"same", b"same"), Collision::Identical);
    }

    #[test]
    fn a_newer_version_is_an_upgrade() {
        let old = b"$VER: assign 37.4 (25.4.91)".as_slice();
        let new = b"$VER: assign 45.9 (1.1.99)".as_slice();
        assert_eq!(
            classify(old, new),
            Collision::Upgrade {
                from: "37.4".into(),
                to: "45.9".into()
            }
        );
    }

    /// The case the whole engine exists because of: `ModulesA1200_3.2.adf`
    /// holds fourteen commands and thirteen are *older* than the ones
    /// `Workbench3.2` already carries.
    #[test]
    fn an_older_version_is_a_downgrade() {
        let new = b"$VER: assign 45.9 (1.1.99)".as_slice();
        let old = b"$VER: assign 37.4 (25.4.91)".as_slice();
        assert!(matches!(classify(new, old), Collision::Downgrade { .. }));
    }

    /// Equal versions, different bytes: neither older nor newer, and both
    /// sides do say what they are — `SameVersion`, not `Unversioned` and
    /// not silently `Upgrade` (spec fix `aa753f3`, fix round 1 F2). A
    /// positive expectation, replacing the brief's own weaker
    /// `!matches!(.., Upgrade)` — that assertion is exactly what let the
    /// name-mismatch defect (F1) through unnoticed.
    #[test]
    fn the_same_version_with_different_bytes_is_a_same_version_collision() {
        let a = b"$VER: assign 37.4 (25.4.91)\x00A".as_slice();
        let b = b"$VER: assign 37.4 (25.4.91)\x00B".as_slice();
        assert_eq!(
            classify(a, b),
            Collision::SameVersion {
                version: "37.4".into(),
                from_bytes: a.len() as u64,
                to_bytes: b.len() as u64
            }
        );
    }

    /// 69% of a real tree. One side saying nothing is enough.
    #[test]
    fn a_file_with_no_version_reports_sizes_and_no_version() {
        let existing = b"plain bytes".as_slice();
        let incoming = b"$VER: thing 45.1 (1.1.99)".as_slice();
        assert_eq!(
            classify(existing, incoming),
            Collision::Unversioned {
                from_bytes: existing.len() as u64,
                to_bytes: incoming.len() as u64
            }
        );
    }

    /// A date alone never moves the verdict — same version, different
    /// bytes, positive expectation (see the test above).
    #[test]
    fn a_rebuild_with_a_later_date_is_a_same_version_collision() {
        let a = b"$VER: thing 44.1 (1.1.99)\x00x".as_slice();
        let b = b"$VER: thing 44.1 (31.12.02)\x00y".as_slice();
        assert_eq!(
            classify(a, b),
            Collision::SameVersion {
                version: "44.1".into(),
                from_bytes: a.len() as u64,
                to_bytes: b.len() as u64
            }
        );
    }

    /// Fix round 1, F1 — the reviewer's own input. `read` finds the
    /// *first* `$VER:` in a file wherever it sits (`amigaver.rs`'s own
    /// doc comment), so an incoming file can carry a real, newer-looking
    /// version number that belongs to an entirely different program —
    /// here, `dos.library 47.0` sitting ahead of the file's own
    /// `assign 37.4`. Comparing the raw numbers would call this an
    /// upgrade from `assign 45.9`; comparing names first must instead
    /// treat it exactly like no version was found at all.
    #[test]
    fn a_different_programs_version_marker_is_never_compared_against_this_ones() {
        let existing = b"$VER: assign 45.9 (1.1.99)".as_slice();
        let incoming = [
            b"$VER: dos.library 47.0 (1.1.99)\x00".as_slice(),
            b"$VER: assign 37.4 (25.4.91)",
        ]
        .concat();

        assert_eq!(
            classify(existing, &incoming),
            Collision::Unversioned {
                from_bytes: existing.len() as u64,
                to_bytes: incoming.len() as u64
            }
        );
    }

    // ---- `preview` — over a real tempdir tree ----

    /// A minimal `distribution.json`, naming exactly one file's owner.
    fn write_manifest(root: &Path, owner_path: &str, owner_component: &str) {
        let manifest = DistributionManifest {
            release: "Test".into(),
            built_from: vec![MediaRecord {
                volume_name: "Test".into(),
                sha256: "0".repeat(64),
            }],
            files: vec![FileRecord {
                path: owner_path.into(),
                component: owner_component.into(),
                media: "Test".into(),
                sha256: "0".repeat(64),
                bytes: 0,
                protection: None,
            }],
            paired_rom: None,
        };
        std::fs::write(
            root.join(MANIFEST_FILE_NAME),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn incoming<'a>(component: &str, bytes_at: &'a Path, to: &str) -> Incoming<'a> {
        Incoming {
            to: to.to_string(),
            component: component.to_string(),
            bytes_at,
        }
    }

    #[test]
    fn an_item_landing_on_nothing_is_absent_from_the_report() {
        let dir = fixtures::scratch("collide-lands-on-nothing");
        let tree = dir.join("tree");
        std::fs::create_dir_all(&tree).unwrap();

        // `bytes_at` need not even exist — nothing that missing a
        // destination ever reads it.
        let never_read = dir.join("never-read");
        let item = incoming("boingbag-39-1", &never_read, "C/Nothing");

        let reports = preview(&tree, std::slice::from_ref(&item)).unwrap();
        assert!(reports.is_empty());
    }

    #[test]
    fn an_item_landing_on_identical_bytes_is_absent_from_the_report() {
        let dir = fixtures::scratch("collide-lands-identical");
        let tree = dir.join("tree");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C").join("Same"), b"identical content").unwrap();

        let incoming_path = dir.join("incoming-same");
        std::fs::write(&incoming_path, b"identical content").unwrap();

        let item = incoming("boingbag-39-1", &incoming_path, "C/Same");

        // No manifest at all — an identical item must never need to read
        // one, since it never reaches the point of asking who declared it.
        let reports = preview(&tree, std::slice::from_ref(&item)).unwrap();
        assert!(reports.is_empty());
    }

    #[test]
    fn an_older_incoming_version_is_reported_as_a_downgrade() {
        let dir = fixtures::scratch("collide-downgrade");
        let tree = dir.join("tree");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C").join("Assign"), b"$VER: assign 45.9 (1.1.99)").unwrap();
        write_manifest(&tree, "C/Assign", "workbench-base");

        let incoming_path = dir.join("incoming-assign");
        std::fs::write(&incoming_path, b"$VER: assign 37.4 (25.4.91)").unwrap();

        let item = incoming("boingbag-39-1", &incoming_path, "C/Assign");

        let reports = preview(&tree, std::slice::from_ref(&item)).unwrap();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].path, "C/Assign");
        assert_eq!(
            reports[0].collision,
            Collision::Downgrade {
                from: "45.9".into(),
                to: "37.4".into()
            }
        );
    }

    /// `boingbag-39-1` declares `overrides: ["workbench-base"]` (shipped
    /// data, `package.rs`'s own module doc comment) — so a file whose
    /// owner in `distribution.json` is some other component is a
    /// collision this package never said it may write over.
    #[test]
    fn a_collision_the_recipe_did_not_declare_is_marked_undeclared() {
        let dir = fixtures::scratch("collide-undeclared");
        let tree = dir.join("tree");
        std::fs::create_dir_all(tree.join("Locale")).unwrap();
        std::fs::write(tree.join("Locale").join("Catalog"), b"old catalog bytes").unwrap();
        write_manifest(&tree, "Locale/Catalog", "locale-base");

        let incoming_path = dir.join("incoming-catalog");
        std::fs::write(&incoming_path, b"new catalog bytes, longer").unwrap();

        let item = incoming("boingbag-39-1", &incoming_path, "Locale/Catalog");

        let reports = preview(&tree, std::slice::from_ref(&item)).unwrap();
        assert_eq!(reports.len(), 1);
        assert!(!reports[0].declared, "locale-base is not workbench-base");
    }

    /// The positive case beside it, in the same test run rather than
    /// trusted on its own: `boingbag-39-1` *does* declare `workbench-base`,
    /// so a file that component owns is a declared collision.
    #[test]
    fn a_collision_the_recipe_did_declare_is_marked_declared() {
        let dir = fixtures::scratch("collide-declared");
        let tree = dir.join("tree");
        std::fs::create_dir_all(tree.join("Libs")).unwrap();
        std::fs::write(tree.join("Libs").join("x.library"), b"old library bytes").unwrap();
        write_manifest(&tree, "Libs/x.library", "workbench-base");

        let incoming_path = dir.join("incoming-library");
        std::fs::write(&incoming_path, b"new library bytes, longer").unwrap();

        let item = incoming("boingbag-39-1", &incoming_path, "Libs/x.library");

        let reports = preview(&tree, std::slice::from_ref(&item)).unwrap();
        assert_eq!(reports.len(), 1);
        assert!(reports[0].declared, "workbench-base is workbench-base");
    }

    /// Fix round 1, F5 — an id that names no shipped package is refused,
    /// not silently read as "nothing declared".
    #[test]
    fn an_unresolvable_component_id_is_refused_rather_than_read_as_undeclared() {
        let dir = fixtures::scratch("collide-unresolvable-component");
        let tree = dir.join("tree");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C").join("Foo"), b"old bytes").unwrap();
        write_manifest(&tree, "C/Foo", "workbench-base");

        let incoming_path = dir.join("incoming-foo");
        std::fs::write(&incoming_path, b"new bytes, longer than old").unwrap();

        let item = incoming("not-a-real-package-id", &incoming_path, "C/Foo");

        let err = preview(&tree, std::slice::from_ref(&item))
            .unwrap_err()
            .to_string();
        assert!(err.contains("not-a-real-package-id"), "got {err}");
    }

    /// Fix round 1, F9 — two entries landing on the same destination: the
    /// last one wins, matching what would actually land on disk. Built so
    /// the two entries would classify *differently* (downgrade, then
    /// identical) if either alone decided the row, proving the dedup is
    /// really keyed on `to` and really keeps the last result.
    #[test]
    fn the_last_of_two_entries_landing_on_the_same_destination_wins() {
        let dir = fixtures::scratch("collide-dedup-last-wins");
        let tree = dir.join("tree");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        let existing_bytes: &[u8] = b"$VER: assign 45.9 (1.1.99)";
        std::fs::write(tree.join("C").join("Assign"), existing_bytes).unwrap();
        write_manifest(&tree, "C/Assign", "workbench-base");

        // First entry: a genuine downgrade, which alone would produce a row.
        let older = dir.join("incoming-older");
        std::fs::write(&older, b"$VER: assign 37.4 (25.4.91)").unwrap();

        // Second entry, same destination: identical to what's already
        // there. If the last entry truly wins, the file lands unchanged
        // and the whole path drops out of the report.
        let same = dir.join("incoming-same-as-tree");
        std::fs::write(&same, existing_bytes).unwrap();

        let items = vec![
            incoming("boingbag-39-1", &older, "C/Assign"),
            incoming("boingbag-39-1", &same, "C/Assign"),
        ];

        let reports = preview(&tree, &items).unwrap();
        assert!(
            reports.is_empty(),
            "the last entry is identical, so nothing should be reported: {reports:?}"
        );
    }

    /// `safe_join`'s own discipline, applied here: a destination that
    /// climbs out of the tree root is refused, never read.
    #[test]
    fn a_destination_that_climbs_out_of_the_root_is_refused() {
        let dir = fixtures::scratch("collide-traversal");
        let tree = dir.join("tree");
        std::fs::create_dir_all(&tree).unwrap();

        let never_read = dir.join("never-read");
        let item = incoming("boingbag-39-1", &never_read, "../escaped");

        assert!(preview(&tree, std::slice::from_ref(&item)).is_err());
    }
}
