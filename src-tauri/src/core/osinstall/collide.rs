//! What a package would land on, before anything is written (spec §3).
//!
//! Entirely read-only: nothing in this file opens a file for writing, and
//! nothing here calls anywhere that does. It exists so a downgrade — "the
//! whole engine exists for" this, per `ModulesA1200_3.2.adf`'s own thirteen
//! stale commands — is a fact the user sees on screen before it happens,
//! never a fact discovered afterwards by noticing something broke.
//!
//! ## Why this reads bytes off disk directly, not through a `MediaSource`
//!
//! [`preview`] runs before a plan is ever applied, and its own signature
//! carries only a tree root and the planned items — no `media_paths`, no
//! open archive, no package folder. An ordinary [`PlanItem::from`] is
//! resolved against a [`super::source::MediaSource`] opened from
//! [`PlanItem::media`]'s volume name (`apply.rs`'s own `sources` map,
//! built from `InstallPlan::media_paths`), but that resolution needs a
//! media folder this function's signature does not carry — and reopening
//! one here, by whatever means, would be exactly the "second, nearly
//! identical resolver" this round's own plan warns a caller against
//! growing (`osinstall_collisions`'s brief, verbatim). So `preview` takes
//! its items with `from` already naming a real, readable path to the
//! incoming bytes — the same "read the bytes once, hand back something
//! path-shaped so ordinary code can use them" move `source_archive.rs`'s
//! own `open_nested` already makes for the identical problem (a caller
//! that has bytes from inside an archive and needs a path). The caller
//! that builds those items — the command layer, once one exists — is the
//! one place a package's real archive genuinely gets opened; this module
//! never does.
//!
//! ## The bound is two different questions, not one
//!
//! **Finding `$VER:`.** [`crate::core::amigaver::read`] scans for the
//! marker wherever it sits, so it needs real bytes, not a fabricated
//! window — but it does not need the *whole* file. `apply.rs`'s own
//! real-tree scanner already measured this: "the first 1 MiB is enough to
//! hold a `$VER:` marker in any real Amiga file" ([`VERSION_SEARCH_BOUND`]
//! reuses that same figure, not a new one invented for this file).
//!
//! **Whether the bytes are the same at all.** A length comparison is free
//! — no content read at all — and it already answers "identical" for
//! every pair whose sizes differ: two files of different length cannot be
//! byte-for-byte the same, so there is nothing to gain from reading either
//! one whole just to confirm that. Only when the lengths agree is identity
//! even on the table, and only then is a real, full-content read (never a
//! truncated one, or every reader here risks a coincidental shared prefix
//! reading as `Identical` when the files actually differ past the bound)
//! worth paying for — a real AmigaOS file is, at most, a few hundred
//! kilobytes (this is floppy-era software), so that cost is genuinely
//! small. [`MAX_COLLISION_FILE`] is a sanity ceiling on that full read,
//! not a bound this codebase expects to ever hit on real data; a file that
//! size claiming to be an AmigaOS file is itself the anomaly ART should
//! name rather than silently compare a truncated slice of.
//!
//! This is why `classify` and the private `classify_by_version` are two
//! functions rather than one: `preview` calls `classify_by_version`
//! directly, never `classify`, whenever a length comparison has already
//! settled the identity question — so a truncated, size-mismatched pair
//! can never reach the `existing == incoming` check that would otherwise
//! risk reporting `Identical` on two files a metadata call already proved
//! are not.

use std::cmp::Ordering;
use std::io::Read as _;
use std::path::Path;

use serde::Serialize;

use super::apply::{DistributionManifest, MANIFEST_FILE_NAME};
use super::package;
use super::plan::PlanItem;
use crate::core::amigaver;
use crate::core::error::{CoreError, CoreResult};
use crate::core::security::path::safe_join;

/// How much of a size-mismatched pair `preview` reads looking for a
/// `$VER:` marker — see the module doc comment's "The bound" section.
const VERSION_SEARCH_BOUND: u64 = 1024 * 1024;

/// The sanity ceiling on a full, whole-file read when two files' lengths
/// already agree — see the module doc comment. Far past any real AmigaOS
/// file; a file this large is the thing worth refusing to guess about, not
/// a bound this codebase expects real data to reach.
const MAX_COLLISION_FILE: u64 = 64 * 1024 * 1024;

/// What landing `incoming` over `existing` would actually do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Collision {
    /// The bytes are the same. Not an overwrite at all.
    Identical,
    /// Both sides say what they are, and the incoming one is newer.
    Upgrade { from: String, to: String },
    /// Both sides say what they are, and the incoming one is **older** — or
    /// carries the identical version number over different bytes. Nothing
    /// that is not a strictly newer version is ever reported `Upgrade`; see
    /// [`classify_by_version`]'s own doc comment for why that direction of
    /// mistake is the one this module refuses to make.
    Downgrade { from: String, to: String },
    /// One side or both says nothing. Sizes, and no invented version.
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

/// Turn an [`amigaver::AmigaVersion`] into the `"version.revision"` text
/// [`Collision::Upgrade`]/[`Collision::Downgrade`] carry — the same shape
/// `$VER:` itself uses, so what is shown is what the file actually says.
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
/// comparison) — see the module doc comment for why that split exists.
///
/// Two versions that both parse are compared with
/// [`amigaver::AmigaVersion::compare_version`], and only a *strictly*
/// greater incoming version is ever reported [`Collision::Upgrade`] —
/// [`Ordering::Equal`] (the identical version number over different bytes,
/// a rebuild or a corrupt copy) and [`Ordering::Less`] both land on
/// [`Collision::Downgrade`]. That is not a symmetric choice: nothing that
/// is not a proven upgrade is ever allowed to render as one, because
/// `ModulesA1200_3.2.adf`'s own thirteen stale commands are exactly what a
/// wrongly-green arrow would hide, and this module exists because of that
/// case, not the reverse one. A version found on only one side, or on
/// neither, falls back to sizes — never an invented version (§89).
fn classify_by_version(existing: &[u8], incoming: &[u8]) -> Collision {
    match (amigaver::read(existing), amigaver::read(incoming)) {
        (Some(from), Some(to)) => {
            if to.compare_version(&from) == Ordering::Greater {
                Collision::Upgrade {
                    from: version_label(&from),
                    to: version_label(&to),
                }
            } else {
                Collision::Downgrade {
                    from: version_label(&from),
                    to: version_label(&to),
                }
            }
        }
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

/// What every planned item in `items` would land on inside the tree already
/// built at `tree_root` — the preview §3 requires. Nothing here writes
/// anything.
///
/// Only a planned **file** whose destination already exists is worth a
/// report at all: a directory item never collides with content, and a
/// destination nobody occupies yet is not an overwrite of anything.
/// [`Collision::Identical`] is excluded from the result for the same
/// reason it exists as its own variant — see that type's own doc comment:
/// it is not an overwrite, and keeping it in the list would bury the rows
/// that are.
///
/// Every `item.to` is resolved through [`safe_join`] before it is ever
/// read, the same discipline `apply.rs`'s own module doc comment states
/// for writing: a recipe's `to` is data a human typed, and a `../` in a
/// text box is the same hole a `../` in a zip is, whichever direction the
/// bytes travel.
pub fn preview(tree_root: &Path, items: &[PlanItem]) -> CoreResult<Vec<CollisionReport>> {
    // Loaded at most once, lazily — most calls preview a plan against a
    // tree where most items land on nothing or on identical bytes, and
    // neither of those needs the manifest at all.
    let mut manifest: Option<DistributionManifest> = None;
    let mut reports = Vec::new();

    for item in items {
        if item.is_dir {
            continue;
        }

        let target = safe_join(tree_root, &item.to).map_err(|err| {
            CoreError::SafetyRefused(format!(
                "'{}' does not stay inside the distribution root: {err}",
                item.to
            ))
        })?;
        if !target.is_file() {
            continue;
        }

        let incoming_path = Path::new(&item.from);
        if !incoming_path.is_file() {
            // Nothing to compare against — never invent a byte count or a
            // version for a file that is not actually there (§89).
            return Err(CoreError::InvalidInput(format!(
                "'{}' names an incoming source that does not exist: '{}'",
                item.to, item.from
            )));
        }

        let existing_len = std::fs::metadata(&target)?.len();
        let incoming_len = std::fs::metadata(incoming_path)?.len();

        let collision = if existing_len == incoming_len {
            if existing_len > MAX_COLLISION_FILE || incoming_len > MAX_COLLISION_FILE {
                return Err(CoreError::InvalidInput(format!(
                    "'{}' is {existing_len} bytes — too large for ART to compare \
                     byte-for-byte here; no real AmigaOS file is this size",
                    item.to
                )));
            }
            // Lengths agree: identity is on the table, and only a full
            // read answers it honestly — see the module doc comment.
            let existing_bytes = std::fs::read(&target)?;
            let incoming_bytes = std::fs::read(incoming_path)?;
            classify(&existing_bytes, &incoming_bytes)
        } else {
            // Lengths already disagree, so identity is settled without
            // reading either file whole — `classify_by_version`, never
            // `classify`, so a coincidentally-equal truncated prefix can
            // never be reported `Identical`. See the module doc comment.
            let existing_bytes = read_bounded(&target, VERSION_SEARCH_BOUND)?;
            let incoming_bytes = read_bounded(incoming_path, VERSION_SEARCH_BOUND)?;
            classify_by_version(&existing_bytes, &incoming_bytes)
        };

        // `Unversioned`'s byte counts are the tree's own measured sizes,
        // never whatever a bounded read happened to see — a size-mismatch
        // read above is capped at `VERSION_SEARCH_BOUND`, and a real file
        // bigger than that bound would otherwise be under-reported.
        let collision = match collision {
            Collision::Unversioned { .. } => Collision::Unversioned {
                from_bytes: existing_len,
                to_bytes: incoming_len,
            },
            other => other,
        };

        if collision == Collision::Identical {
            continue;
        }

        let declared = declared_override(tree_root, &mut manifest, &item.to, &item.component)?;

        reports.push(CollisionReport {
            path: item.to.clone(),
            collision,
            declared,
        });
    }

    Ok(reports)
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

    // An id that names no shipped package declares nothing — absence of
    // evidence is not treated as an override (§89), it is `false`.
    Ok(package::by_id(component)
        .map(|package| package.component.overrides.iter().any(|over| over == owner))
        .unwrap_or(false))
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::osinstall::apply::{FileRecord, MediaRecord};
    use crate::core::osinstall::fixtures;

    // ---- `classify` — verbatim from the brief ----

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

    /// Equal versions, different bytes. Not an upgrade — and specifically
    /// not silently called one.
    #[test]
    fn the_same_version_with_different_bytes_is_not_an_upgrade() {
        let a = b"$VER: assign 37.4 (25.4.91)\x00A".as_slice();
        let b = b"$VER: assign 37.4 (25.4.91)\x00B".as_slice();
        assert!(!matches!(classify(a, b), Collision::Upgrade { .. }));
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

    /// A date alone never moves the verdict.
    #[test]
    fn a_rebuild_with_a_later_date_is_not_an_upgrade() {
        let a = b"$VER: thing 44.1 (1.1.99)\x00x".as_slice();
        let b = b"$VER: thing 44.1 (31.12.02)\x00y".as_slice();
        assert!(!matches!(classify(a, b), Collision::Upgrade { .. }));
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

    /// A `PlanItem` for a file, with `from` already a real path on disk —
    /// see the module doc comment for why `preview` expects that rather
    /// than a media-relative one.
    fn file_item(component: &str, from: &Path, to: &str) -> PlanItem {
        PlanItem {
            component: component.to_string(),
            media: "irrelevant-to-preview".into(),
            from: from.to_string_lossy().into_owned(),
            to: to.to_string(),
            is_dir: false,
            bytes: 0,
        }
    }

    #[test]
    fn an_item_landing_on_nothing_is_absent_from_the_report() {
        let dir = fixtures::scratch("collide-lands-on-nothing");
        let tree = dir.join("tree");
        std::fs::create_dir_all(&tree).unwrap();

        // `from` need not even exist — nothing that missing a destination
        // ever reads it.
        let item = file_item("boingbag-39-1", &dir.join("never-read"), "C/Nothing");

        let reports = preview(&tree, std::slice::from_ref(&item)).unwrap();
        assert!(reports.is_empty());
    }

    #[test]
    fn an_item_landing_on_identical_bytes_is_absent_from_the_report() {
        let dir = fixtures::scratch("collide-lands-identical");
        let tree = dir.join("tree");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C").join("Same"), b"identical content").unwrap();

        let incoming = dir.join("incoming-same");
        std::fs::write(&incoming, b"identical content").unwrap();

        let item = file_item("boingbag-39-1", &incoming, "C/Same");

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

        let incoming = dir.join("incoming-assign");
        std::fs::write(&incoming, b"$VER: assign 37.4 (25.4.91)").unwrap();

        let item = file_item("boingbag-39-1", &incoming, "C/Assign");

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

        let incoming = dir.join("incoming-catalog");
        std::fs::write(&incoming, b"new catalog bytes, longer").unwrap();

        let item = file_item("boingbag-39-1", &incoming, "Locale/Catalog");

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

        let incoming = dir.join("incoming-library");
        std::fs::write(&incoming, b"new library bytes, longer").unwrap();

        let item = file_item("boingbag-39-1", &incoming, "Libs/x.library");

        let reports = preview(&tree, std::slice::from_ref(&item)).unwrap();
        assert_eq!(reports.len(), 1);
        assert!(reports[0].declared, "workbench-base is workbench-base");
    }

    #[test]
    fn a_directory_item_is_never_a_collision() {
        let dir = fixtures::scratch("collide-directory");
        let tree = dir.join("tree");
        std::fs::create_dir_all(tree.join("C")).unwrap();

        let item = PlanItem {
            component: "boingbag-39-1".into(),
            media: "irrelevant".into(),
            from: String::new(),
            to: "C".into(),
            is_dir: true,
            bytes: 0,
        };

        let reports = preview(&tree, std::slice::from_ref(&item)).unwrap();
        assert!(reports.is_empty());
    }

    /// `safe_join`'s own discipline, applied here: a destination that
    /// climbs out of the tree root is refused, never read.
    #[test]
    fn a_destination_that_climbs_out_of_the_root_is_refused() {
        let dir = fixtures::scratch("collide-traversal");
        let tree = dir.join("tree");
        std::fs::create_dir_all(&tree).unwrap();

        let item = file_item("boingbag-39-1", &dir.join("never-read"), "../escaped");

        assert!(preview(&tree, std::slice::from_ref(&item)).is_err());
    }
}
