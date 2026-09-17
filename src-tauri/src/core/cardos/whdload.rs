//! WHDLoad, chosen by what its own `$VER:` says — the material folders, the
//! archives inside them, and the WHDLoad hardfiles among a partition's
//! sources — and written into a system tree (design § 5 phase 4, P6).
//!
//! **Why this exists at all.** A card whose games cannot start is exactly
//! the "confident, wrong" defect CLAUDE.md names: nothing crashes, nothing
//! logs an error, and the owner only finds out when a title refuses to run.
//! `find_whdload` is the one place ART decides which `C/WHDLoad` a card
//! gets, so the decision is made once, from the file's own stated version,
//! never from a filename or a folder's date.
//!
//! **Where ART looks, in order (material before hardfiles; within a
//! material folder, loose before archives):**
//!
//! - A top-level `WHDLoad` file in a material folder.
//! - `C/WHDLoad` in a material folder.
//! - A member `…/C/WHDLoad` (or `C/WHDLoad` at an archive's own root) inside
//!   an archive at a material folder's top level whose name starts
//!   `whdload` (case-insensitive) — `WHDLoad_usr.lha` is whdload.de's own
//!   name for the current release.
//! - `C/WHDLoad` on a WHDLoad hardfile named as one of a partition's
//!   sources.
//!
//! Each loose or archive candidate's `S/WHDLoad.prefs` — the same package,
//! the same top directory — travels with it, so the prefs a chosen WHDLoad
//! actually ships with are the ones offered to [`install_whdload`]. A
//! hardfile candidate's prefs come from its own volume's `S/WHDLoad.prefs`.
//!
//! **The highest `$VER:` wins; ties go to the first candidate found**, which
//! is why the search order above matters — material outranks a hardfile
//! stumbled on later, the way [`driver::find_pfs3_driver`](super::driver)
//! already keeps a folder's own loose file ahead of its own archive on a
//! tie. A candidate that reads fine but states no `$VER:` at all is not a
//! candidate — the same rule the driver search uses, for the same reason:
//! a file that cannot state its own version cannot honestly outrank one
//! that can, or lose to one either.
//!
//! **Nothing is silent.** Every candidate ART looked at and did not choose
//! — because a higher version won, or because ART could open it but
//! something about it kept it from being read — is named in
//! [`WhdloadChoice::passed_over`], so the choice is never a bare number with
//! no story behind it.
//!
//! **Installing never touches a prefs file that is already there and
//! differs.** `S/WHDLoad.prefs` is the user's own settings (QuitKey, splash
//! behaviour, per-game overrides); the WHDLoad Installer and HstWB both
//! refuse to overwrite one, and ART follows them (owner decision, W § 1.2).
//! **`C/WHDLoad` is not replaced either:** a tree that already has one keeps
//! it — the preparation chooses nothing to install, and [`install_whdload`]
//! keeps a differing binary it finds (`KeptExisting`). Keeping it is never
//! silent: the preparation says which version the tree's copy states beside
//! the best one found elsewhere (`core::cardos::prepare::TreeWhdload`, card
//! round 3, M2).

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use crate::core::adf::blocks::EntryKind;
use crate::core::adf::extract::extract_file_on;
use crate::core::adf::fs::{list_directory_on, read_header_on, FileEntry};
use crate::core::amigaver::{self, AmigaVersion};
use crate::core::archive;
use crate::core::clock::AmigaClock;
use crate::core::detect::{detect, FormatCategory};
use crate::core::error::{CoreError, CoreResult};
use crate::core::gameindex::readers::whdhdf::fs_type_of;
use crate::core::preload::amiga_fold;
use crate::core::safety::atomic::atomic_write;
use crate::core::volume::mount::{mount, scan_image};
use crate::core::volume::{BlockDevice, VolumeGeometry};

/// The most of a candidate `WHDLoad` binary ART will read into memory —
/// loose, out of an archive, or off a hardfile's volume. A real one is a few
/// tens of KB; a hostile file under that name is not let past this
/// regardless of what it claims to be.
pub const WHDLOAD_MAX_BYTES: u64 = 1024 * 1024;

/// The most of a candidate `WHDLoad.prefs` ART will read — a text config
/// file, never anywhere near this large in practice.
pub const PREFS_MAX_BYTES: u64 = 64 * 1024;

/// Where a chosen WHDLoad came from.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum WhdloadOrigin {
    /// `C/WHDLoad` beside a material folder's `S/WHDLoad.prefs`, or a top-level `WHDLoad` file.
    Loose { path: PathBuf },
    /// A member `…/C/WHDLoad` of an archive in a material folder (`WHDLoad_usr.lha`).
    Archive { archive: PathBuf, member: String },
    /// `C/WHDLoad` on a WHDLoad hardfile that is one of a partition's sources.
    Hardfile { image: PathBuf },
}

/// The WHDLoad ART chose, out of every candidate it could read.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhdloadChoice {
    pub origin: WhdloadOrigin,
    pub version: u32,
    pub revision: u32,
    /// Stated as the file states it: `WHDLoad 20.0`.
    pub name: String,
    #[serde(skip)]
    pub binary: Vec<u8>,
    /// The `S/WHDLoad.prefs` from the same package, when it has one.
    #[serde(skip)]
    pub prefs: Option<Vec<u8>>,
    /// Every candidate ART looked at and did not choose, with why — so the choice is never silent.
    pub passed_over: Vec<String>,
}

/// A candidate read into memory, with its origin and stated version, before
/// the search across every folder decides which one wins. Kept exactly the
/// way `core::cardos::driver::find_pfs3_driver`'s own `Candidate` is: bytes
/// in memory, nothing written until the winner is known.
struct Candidate {
    origin: WhdloadOrigin,
    version: AmigaVersion,
    binary: Vec<u8>,
    prefs: Option<Vec<u8>>,
}

/// Find WHDLoad: every material folder (a top-level `WHDLoad` file,
/// `C/WHDLoad`, and any archive whose name starts `whdload`) and every
/// hardfile source, each candidate's version read from its own `$VER:`. See
/// the module doc comment for the search order and the tie-break.
///
/// `Ok(None)` — not an error — when nothing readable states a version
/// anywhere ART looked; a card whose titles need WHDLoad and got `None` is
/// the caller's refusal to make (`CoreError::WhdloadNotFound`), not this
/// function's.
pub fn find_whdload(
    material: &[PathBuf],
    hardfiles: &[PathBuf],
    clock: &dyn AmigaClock,
) -> CoreResult<Option<WhdloadChoice>> {
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut unreadable: Vec<String> = Vec::new();

    for folder in material {
        if let Some(path) = find_named_file(folder, "WHDLoad") {
            note(
                &mut candidates,
                &mut unreadable,
                &path,
                read_loose_candidate(&path, folder),
            );
        }
        if let Some(c_dir) = find_named_dir(folder, "C") {
            if let Some(path) = find_named_file(&c_dir, "WHDLoad") {
                note(
                    &mut candidates,
                    &mut unreadable,
                    &path,
                    read_loose_candidate(&path, folder),
                );
            }
        }
        for path in candidate_archives(folder) {
            note(
                &mut candidates,
                &mut unreadable,
                &path,
                read_archive_candidate(&path),
            );
        }
    }

    for image in hardfiles {
        note(
            &mut candidates,
            &mut unreadable,
            image,
            read_hardfile_candidate(image, clock),
        );
    }

    if candidates.is_empty() {
        return Ok(None);
    }

    let mut winner = 0usize;
    for (index, candidate) in candidates.iter().enumerate().skip(1) {
        if candidate
            .version
            .compare_version(&candidates[winner].version)
            == Ordering::Greater
        {
            winner = index;
        }
    }

    let mut passed_over: Vec<String> = candidates
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != winner)
        .map(|(_, candidate)| describe_candidate(candidate))
        .collect();
    passed_over.extend(unreadable);

    let chosen = candidates.remove(winner);
    Ok(Some(WhdloadChoice {
        origin: chosen.origin,
        version: chosen.version.version,
        revision: chosen.version.revision,
        name: format!(
            "{} {}.{}",
            chosen.version.name, chosen.version.version, chosen.version.revision
        ),
        binary: chosen.binary,
        prefs: chosen.prefs,
        passed_over,
    }))
}

/// Record what one attempted candidate did: a real candidate goes onto the
/// list, a plain "not this" (no matching member, no `$VER:`) is dropped
/// silently, and a read that failed outright is named in `unreadable` —
/// `path` rather than `Ok(None)`'s reason, because the doc comment on
/// `find_whdload` promises unreadable candidates are named, not just
/// swallowed.
fn note(
    candidates: &mut Vec<Candidate>,
    unreadable: &mut Vec<String>,
    path: &Path,
    result: CoreResult<Option<Candidate>>,
) {
    match result {
        Ok(Some(candidate)) => candidates.push(candidate),
        Ok(None) => {}
        Err(err) => unreadable.push(format!("'{}': {err}", path.display())),
    }
}

/// One line naming a candidate ART did not choose: where it came from and
/// what it stated.
fn describe_candidate(candidate: &Candidate) -> String {
    format!(
        "{} (WHDLoad {}.{})",
        describe_origin(&candidate.origin),
        candidate.version.version,
        candidate.version.revision
    )
}

fn describe_origin(origin: &WhdloadOrigin) -> String {
    match origin {
        WhdloadOrigin::Loose { path } => format!("'{}'", path.display()),
        WhdloadOrigin::Archive { archive, member } => {
            format!("'{}' ({member})", archive.display())
        }
        WhdloadOrigin::Hardfile { image } => format!("'{}'", image.display()),
    }
}

// ---------------------------------------------------------------------------
// Host folder listing — case-insensitive by listing, not by probing a spelling
// ---------------------------------------------------------------------------

fn find_named_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    entries.flatten().map(|entry| entry.path()).find(|path| {
        path.is_file()
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|found| found.eq_ignore_ascii_case(name))
    })
}

fn find_named_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    entries.flatten().map(|entry| entry.path()).find(|path| {
        path.is_dir()
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|found| found.eq_ignore_ascii_case(name))
    })
}

/// Every top-level file in `folder` whose name starts `whdload`
/// (case-insensitive) and which `core::detect` calls an archive.
fn candidate_archives(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.to_ascii_lowercase().starts_with("whdload") {
            continue;
        }
        if matches!(detect(&path), Ok(detection) if detection.category == FormatCategory::Archive) {
            found.push(path);
        }
    }
    found
}

// ---------------------------------------------------------------------------
// Reading a candidate
// ---------------------------------------------------------------------------

fn read_bounded(path: &Path, cap: u64) -> CoreResult<Vec<u8>> {
    let len = std::fs::metadata(path)?.len();
    if len > cap {
        return Err(CoreError::LimitExceeded {
            subject: "WHDLoad".into(),
            detail: format!(
                "'{}' is {len} bytes, more than the {cap}-byte cap ART reads for it",
                path.display()
            ),
        });
    }
    Ok(std::fs::read(path)?)
}

/// A loose `WHDLoad`, at `bin_path`, whose package is `folder` — the folder
/// a same-package `S/WHDLoad.prefs` is looked for in, whichever loose
/// spelling (`folder/WHDLoad` or `folder/C/WHDLoad`) `bin_path` is.
fn read_loose_candidate(bin_path: &Path, folder: &Path) -> CoreResult<Option<Candidate>> {
    let bytes = read_bounded(bin_path, WHDLOAD_MAX_BYTES)?;
    let Some(version) = amigaver::read(&bytes) else {
        return Ok(None);
    };
    let prefs = find_named_dir(folder, "S")
        .and_then(|s_dir| find_named_file(&s_dir, "WHDLoad.prefs"))
        .and_then(|prefs_path| read_bounded(&prefs_path, PREFS_MAX_BYTES).ok());
    Ok(Some(Candidate {
        origin: WhdloadOrigin::Loose {
            path: bin_path.to_path_buf(),
        },
        version,
        binary: bytes,
        prefs,
    }))
}

/// An archive named `whdload*`: its member ending `/c/whdload` or equal to
/// `c/whdload` (case-insensitive) is the binary; a member in the same top
/// directory ending `/s/whdload.prefs` is its prefs.
fn read_archive_candidate(path: &Path) -> CoreResult<Option<Candidate>> {
    let mut backend = archive::open(path)?;
    let entries = backend.entries()?;
    let normalised: Vec<String> = entries.iter().map(|e| e.name.replace('\\', "/")).collect();

    let Some(bin_index) = normalised.iter().position(|name| {
        let lower = name.to_ascii_lowercase();
        lower == "c/whdload" || lower.ends_with("/c/whdload")
    }) else {
        return Ok(None);
    };

    let bytes = backend.read(bin_index, WHDLOAD_MAX_BYTES)?;
    let Some(version) = amigaver::read(&bytes) else {
        return Ok(None);
    };

    let member_lower = normalised[bin_index].to_ascii_lowercase();
    let expected_prefs = match member_lower.strip_suffix("/c/whdload") {
        Some(top) => format!("{top}/s/whdload.prefs"),
        None => "s/whdload.prefs".to_string(),
    };
    let prefs_index = normalised
        .iter()
        .position(|name| name.to_ascii_lowercase() == expected_prefs);
    let prefs = prefs_index.and_then(|index| backend.read(index, PREFS_MAX_BYTES).ok());

    Ok(Some(Candidate {
        origin: WhdloadOrigin::Archive {
            archive: path.to_path_buf(),
            member: normalised[bin_index].clone(),
        },
        version,
        binary: bytes,
        prefs,
    }))
}

/// One partition source's WHDLoad hardfile: its volume's `C/WHDLoad` and,
/// when there is one, its `S/WHDLoad.prefs`. `Ok(None)` for a hardfile that
/// mounts fine but has no `C/WHDLoad` at all — the ordinary case, not a
/// failure to report.
fn read_hardfile_candidate(image: &Path, clock: &dyn AmigaClock) -> CoreResult<Option<Candidate>> {
    let scanned = scan_image(image)?;
    let entry = scanned
        .volumes
        .iter()
        .find(|volume| volume.is_mountable())
        .ok_or_else(|| CoreError::Malformed {
            format: "whdload-hardfile".into(),
            detail: "no mountable volume in this image".into(),
        })?;
    let (device, geometry) = mount(image, entry)?;
    let root = geometry.root_block;

    let Some(c_block) = find_amiga_dir(&device, root, "C", clock)? else {
        return Ok(None);
    };
    let Some(bin_entry) = find_amiga_file(&device, c_block, "WHDLoad", clock)? else {
        return Ok(None);
    };
    if bin_entry.byte_size > WHDLOAD_MAX_BYTES {
        return Err(CoreError::LimitExceeded {
            subject: "WHDLoad".into(),
            detail: format!(
                "'{}' C/WHDLoad is {} bytes, more than the {WHDLOAD_MAX_BYTES}-byte cap ART \
                 reads for it",
                image.display(),
                bin_entry.byte_size
            ),
        });
    }
    let bytes = read_amiga_file(&device, &bin_entry, &geometry)?;
    let Some(version) = amigaver::read(&bytes) else {
        return Ok(None);
    };

    let prefs = read_hardfile_prefs(&device, root, &geometry, clock)?;

    Ok(Some(Candidate {
        origin: WhdloadOrigin::Hardfile {
            image: image.to_path_buf(),
        },
        version,
        binary: bytes,
        prefs,
    }))
}

fn read_hardfile_prefs(
    device: &impl BlockDevice,
    root: u32,
    geometry: &VolumeGeometry,
    clock: &dyn AmigaClock,
) -> CoreResult<Option<Vec<u8>>> {
    let Some(s_block) = find_amiga_dir(device, root, "S", clock)? else {
        return Ok(None);
    };
    let Some(prefs_entry) = find_amiga_file(device, s_block, "WHDLoad.prefs", clock)? else {
        return Ok(None);
    };
    if prefs_entry.byte_size > PREFS_MAX_BYTES {
        return Ok(None);
    }
    Ok(Some(read_amiga_file(device, &prefs_entry, geometry)?))
}

fn read_amiga_file(
    device: &impl BlockDevice,
    entry: &FileEntry,
    geometry: &VolumeGeometry,
) -> CoreResult<Vec<u8>> {
    let header = read_header_on(device, entry.header_block)?;
    extract_file_on(device, &header, fs_type_of(geometry))
}

/// A directory named `name` (Amiga-side fold, ART-012) directly under
/// `dir_block`, or `None` when there is none — never an error, since a
/// hardfile with no `C` (or no `S`) at all is the ordinary shape for
/// anything but the game's own drawer.
fn find_amiga_dir(
    device: &impl BlockDevice,
    dir_block: u32,
    name: &str,
    clock: &dyn AmigaClock,
) -> CoreResult<Option<u32>> {
    let listing = list_directory_on(device, dir_block, clock)?;
    Ok(listing
        .into_iter()
        .find(|entry| {
            entry.kind == EntryKind::Directory && amiga_fold(&entry.name) == amiga_fold(name)
        })
        .map(|entry| entry.header_block))
}

fn find_amiga_file(
    device: &impl BlockDevice,
    dir_block: u32,
    name: &str,
    clock: &dyn AmigaClock,
) -> CoreResult<Option<FileEntry>> {
    let listing = list_directory_on(device, dir_block, clock)?;
    Ok(listing
        .into_iter()
        .find(|entry| entry.kind == EntryKind::File && amiga_fold(&entry.name) == amiga_fold(name)))
}

// ---------------------------------------------------------------------------
// Installing
// ---------------------------------------------------------------------------

/// What [`install_whdload`] did with one file.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(
    tag = "outcome",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum TreeWrite {
    Written { path: String },
    AlreadyThere { path: String },
    KeptExisting { path: String },
}

/// What [`install_whdload`] did overall.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhdloadInstalled {
    pub binary: TreeWrite,
    pub prefs: Option<TreeWrite>,
}

/// Write `choice`'s `C/WHDLoad` and, when it has one, its `S/WHDLoad.prefs`
/// into `tree`. An identical file already there is `AlreadyThere`; a
/// different one is kept (`KeptExisting`), never replaced — prefs are user
/// data (owner decision, W § 1.2), and there is no reason to prefer ART's
/// own binary bytes over ones already on the tree when they already match.
pub fn install_whdload(choice: &WhdloadChoice, tree: &Path) -> CoreResult<WhdloadInstalled> {
    let c_dir = tree.join("C");
    let s_dir = tree.join("S");
    std::fs::create_dir_all(&c_dir)?;
    std::fs::create_dir_all(&s_dir)?;

    let binary = write_into(&c_dir, "WHDLoad", &choice.binary)?;
    let prefs = match &choice.prefs {
        Some(bytes) => Some(write_into(&s_dir, "WHDLoad.prefs", bytes)?),
        None => None,
    };
    Ok(WhdloadInstalled { binary, prefs })
}

/// Write `bytes` as `name` under `dir`, finding any existing spelling of
/// `name` case-insensitively first — a tree may already hold `c/whdload`.
fn write_into(dir: &Path, name: &str, bytes: &[u8]) -> CoreResult<TreeWrite> {
    match find_named_file(dir, name) {
        None => {
            let target = dir.join(name);
            atomic_write(&target, bytes)?;
            Ok(TreeWrite::Written {
                path: target.display().to_string(),
            })
        }
        Some(existing) => {
            let path = existing.display().to_string();
            // Compared by length first, and read only when the lengths
            // agree — a bounded read of a file already in the tree (M13).
            let same = std::fs::metadata(&existing)?.len() == bytes.len() as u64
                && read_bounded(&existing, bytes.len() as u64)? == bytes;
            if same {
                Ok(TreeWrite::AlreadyThere { path })
            } else {
                Ok(TreeWrite::KeptExisting { path })
            }
        }
    }
}

/// The tree's own `C/WHDLoad`, when it has one, and the version it states
/// (`None` when it states none). Read within [`WHDLOAD_MAX_BYTES`].
pub fn read_tree_whdload(tree: &Path) -> CoreResult<Option<(PathBuf, Option<AmigaVersion>)>> {
    let Some(path) = find_named_dir(tree, "C").and_then(|c_dir| find_named_file(&c_dir, "WHDLoad"))
    else {
        return Ok(None);
    };
    let bytes = read_bounded(&path, WHDLOAD_MAX_BYTES)?;
    Ok(Some((path, amigaver::read(&bytes))))
}

/// Whether `tree` already carries a `C/WHDLoad` (case-insensitive walk of `C`).
pub fn tree_has_whdload(tree: &Path) -> bool {
    find_named_dir(tree, "C")
        .and_then(|c_dir| find_named_file(&c_dir, "WHDLoad"))
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::cardos::content::test_support::build_hdf;
    use crate::core::clock::UtcClock;
    use crate::core::gameindex::readers::slave::tests_support::build_slave;

    fn scratch(tag: &str) -> (crate::core::ScratchDir, PathBuf) {
        crate::core::ScratchDir::pair("art-cardos-whdload", tag)
    }

    fn whdload_bytes(version: &str) -> Vec<u8> {
        let mut bytes = vec![0u8; 286];
        bytes.extend_from_slice(
            format!("$VER: WHDLoad {version} [build 7051] (27.03.2026)\0").as_bytes(),
        );
        bytes
    }

    fn write_lha(path: &Path, files: &[(&str, &[u8])]) {
        std::fs::write(path, crate::core::lha::tests::make_lha_with(files)).unwrap();
    }

    #[test]
    fn the_highest_whdload_wins_across_archive_loose_and_hardfile_and_the_rest_are_named() {
        let (_guard, dir) = scratch("choose");
        let material = dir.join("material");
        std::fs::create_dir_all(material.join("C")).unwrap();
        std::fs::write(material.join("C").join("WHDLoad"), whdload_bytes("18.9")).unwrap();
        write_lha(
            &material.join("WHDLoad_usr.lha"),
            &[
                ("WHDLoad/C/WHDLoad", &whdload_bytes("20.0")),
                ("WHDLoad/S/WHDLoad.prefs", b";all comments\n"),
            ],
        );
        let hdf = dir.join("Game.hdf");
        let slave = build_slave("Game", "2026 ART", 16);
        build_hdf(
            &hdf,
            &["C", "S", "Game"],
            &[
                ("C/WHDLoad", &whdload_bytes("19.1")),
                ("Game/Game.slave", &slave),
            ],
        );

        let choice = find_whdload(std::slice::from_ref(&material), &[hdf], &UtcClock)
            .unwrap()
            .unwrap();

        assert_eq!((choice.version, choice.revision), (20, 0));
        assert!(matches!(choice.origin, WhdloadOrigin::Archive { .. }));
        assert_eq!(choice.prefs.as_deref(), Some(&b";all comments\n"[..]));
        assert_eq!(choice.passed_over.len(), 2, "{:?}", choice.passed_over);
    }

    #[test]
    fn no_whdload_anywhere_is_none_not_a_guess() {
        let (_guard, dir) = scratch("none");
        let material = dir.join("material");
        std::fs::create_dir_all(&material).unwrap();
        let hdf = dir.join("Game.hdf");
        let slave = build_slave("Game", "2026 ART", 16);
        build_hdf(&hdf, &["Game"], &[("Game/Game.slave", &slave)]);

        let found = find_whdload(&[material], &[hdf], &UtcClock).unwrap();
        assert!(found.is_none());
    }

    #[test]
    fn installing_writes_both_files_and_keeps_a_different_prefs() {
        let (_guard, dir) = scratch("install");
        let tree = dir.join("tree");
        std::fs::create_dir_all(tree.join("S")).unwrap();
        std::fs::write(tree.join("S").join("WHDLoad.prefs"), b"QuitKey=$59\n").unwrap();

        let choice = WhdloadChoice {
            origin: WhdloadOrigin::Loose {
                path: dir.join("wherever"),
            },
            version: 20,
            revision: 0,
            name: "WHDLoad 20.0".into(),
            binary: whdload_bytes("20.0"),
            prefs: Some(b";x\n".to_vec()),
            passed_over: Vec::new(),
        };

        let done = install_whdload(&choice, &tree).unwrap();
        assert!(matches!(done.binary, TreeWrite::Written { .. }));
        assert!(matches!(done.prefs, Some(TreeWrite::KeptExisting { .. })));
        assert_eq!(
            std::fs::read(tree.join("S").join("WHDLoad.prefs")).unwrap(),
            b"QuitKey=$59\n"
        );
        assert_eq!(
            std::fs::read(tree.join("C").join("WHDLoad")).unwrap(),
            whdload_bytes("20.0")
        );
    }

    #[test]
    fn an_identical_binary_is_already_there_and_a_different_one_is_kept() {
        let (_guard, dir) = scratch("idempotent");
        let binary = whdload_bytes("20.0");
        let choice = WhdloadChoice {
            origin: WhdloadOrigin::Loose {
                path: dir.join("wherever"),
            },
            version: 20,
            revision: 0,
            name: "WHDLoad 20.0".into(),
            binary: binary.clone(),
            prefs: None,
            passed_over: Vec::new(),
        };

        // Arm 1: the same bytes already on the tree — nothing to write.
        let same_tree = dir.join("same");
        std::fs::create_dir_all(same_tree.join("C")).unwrap();
        std::fs::write(same_tree.join("C").join("WHDLoad"), &binary).unwrap();
        let done = install_whdload(&choice, &same_tree).unwrap();
        assert!(matches!(done.binary, TreeWrite::AlreadyThere { .. }));
        assert_eq!(
            std::fs::read(same_tree.join("C").join("WHDLoad")).unwrap(),
            binary
        );

        // Arm 2: different bytes already on the tree — kept, not replaced.
        let different_tree = dir.join("different");
        std::fs::create_dir_all(different_tree.join("C")).unwrap();
        std::fs::write(
            different_tree.join("C").join("WHDLoad"),
            b"an older whdload",
        )
        .unwrap();
        let done = install_whdload(&choice, &different_tree).unwrap();
        assert!(matches!(done.binary, TreeWrite::KeptExisting { .. }));
        assert_eq!(
            std::fs::read(different_tree.join("C").join("WHDLoad")).unwrap(),
            b"an older whdload"
        );
    }

    #[test]
    fn tree_has_whdload_walks_c_case_insensitively() {
        let (_guard, dir) = scratch("has");
        let tree = dir.join("tree");
        assert!(!tree_has_whdload(&tree));

        std::fs::create_dir_all(tree.join("c")).unwrap();
        std::fs::write(tree.join("c").join("whdload"), b"x").unwrap();
        assert!(tree_has_whdload(&tree));
    }
}
