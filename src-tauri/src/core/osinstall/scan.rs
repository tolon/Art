//! Which install disks are in a folder — found by opening each candidate and
//! reading its volume name from inside it, never by trusting its filename
//! (`source`'s own rule, carried one level up: `AdfSource::open` already
//! reads the label out of the root block, not off the path it was given).
//!
//! ## One level, no recursion, no symlinks
//!
//! A real media folder is flat: the user's own 36 ADFs, sitting beside a
//! `readme.txt`, some `.info` files, maybe an HDF that is not install media
//! at all. It is not walked the way `core::layout::scan` walks a WHDLoad
//! collection — a subdirectory here is something else (a backup, an
//! unrelated folder a file manager created), not more install media, and a
//! symlink is never followed, so neither its target's contents nor its
//! target's name get read as if they were a file the user placed in this
//! folder directly. `std::fs::symlink_metadata` — never `metadata`, which
//! would follow the link — is what makes both of those `false` in one check:
//! a symlink is never `is_file()`, whatever it points at, and neither is a
//! subdirectory. This is the same, undemonstrated-by-a-test choice
//! `core::layout::scan` and `core::collection` already make for the same
//! reason: creating a symlink to prove it is skipped needs privileges CI
//! does not have on this project's own Windows runner.
//!
//! ## Skipped, not an error
//!
//! [`AdfSource::open`] already refuses anything that is not a single bare
//! AmigaDOS volume — an RDB-partitioned HDF, a file with no recognisable
//! signature, one truncated below a single block. Every one of those is a
//! normal thing to find beside install media, so `find_media` treats
//! `Err` from `open` as "this candidate is not media", not as a reason to
//! fail the whole scan. A candidate that cannot even be `stat`-ed —
//! removed between `read_dir` listing it and this function looking at it,
//! which is ordinary on a filesystem someone is actively using, or simply
//! unreadable — is skipped the same way, for the same reason: one bad entry
//! must not fail every other, genuinely readable disk in the folder. Only
//! listing the folder itself (`folder` does not exist, or is not readable
//! at all) propagates as `Err`.
//!
//! ## Cost: opening 36 candidates
//!
//! `AdfSource::open` does not read a whole user file just to learn its
//! label — but for a floppy-sized image that guarantee is weaker than it
//! looks. `core::volume::mount::scan_image` reads `min(1 MiB, file_len)`
//! bytes looking for a signature, and every ADF this project writes is
//! under 1 MiB (880 KB, DD), so that "window" *is* the whole file for a
//! floppy image — opening one was never going to avoid the read for this
//! case, only for something bigger, and install media never is. Scanning
//! the user's real 36-disk folder therefore reads roughly 36 x 880 KB,
//! about 31 MB, off disk — a few milliseconds of sequential I/O on any
//! storage this runs on, not the multi-second scan a first guess might
//! fear. Said plainly, and checked rather than assumed, because the
//! alternative — trusting that "windowed" means "cheap" without reading
//! `scan_image` — is exactly the kind of thing that turns into a real
//! complaint the first time someone points ART at a folder of images
//! bigger than a floppy.
//!
//! ## Duplicate volume names are data, not a scan failure
//!
//! Two files in one folder can carry the same volume name: a stray backup
//! copy of one disk, or two different revisions of "the same" disk that
//! happen to share a label. Silently keeping the first one found is the
//! wrong call — [`media_for`] is what `apply` (a later task) calls to
//! answer "which file holds volume X" before copying bytes onto the user's
//! system, and a wrong guess there installs the wrong disk under the right
//! name. But failing the *whole folder scan* over one duplicate is too
//! blunt: a user with a stray copy of a Locale disk they will never select
//! could not install anything at all, not even an English-only run that
//! never touches it. So the ambiguity is reported as data, not raised as an
//! error here — [`find_media`] keeps every match, and [`media_for`] returns
//! [`MediaMatch::Ambiguous`] carrying every path that claimed the name. It
//! is the caller's job (`apply`, matching a component against its own
//! `Component::media`) to turn that into
//! [`super::RefusalReason::MediaAmbiguous`] for the one component actually
//! affected — the same shape `MediaMissing` and `MediaPathMissing` already
//! use, a named refusal for the thing that is actually broken, not a
//! blanket failure for the whole folder.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::core::error::CoreResult;

use super::source::{AdfSource, MediaSource};
use super::source_archive::ArchiveSource;
use super::source_cd::CdSource;

/// Which reader a piece of found media needs — the fact `find_media` learns
/// by successfully opening a candidate, and the only thing [`open_media`]
/// needs to know to hand back the right one without re-probing the file.
///
/// `Serialize`, kebab-case: this crosses the wire alongside [`FoundMedia`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MediaKind {
    Floppy,
    Disc,
}

/// One piece of install media [`find_media`] opened successfully.
///
/// `Serialize` (Task 12): this is what `commands::osinstall::osinstall_scan_media`
/// hands back to the screen, so it needs to cross the wire — never
/// `Deserialize`, since nothing ever sends one back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundMedia {
    pub path: PathBuf,
    /// The volume name read from **inside** the image — never derived from
    /// `path`.
    pub volume_name: String,
    /// Which reader opened it — carried forward so [`open_media`] never has
    /// to re-probe the file to answer the same question `find_media` already
    /// answered once.
    pub kind: MediaKind,
}

/// Open whichever kind of media `found` was identified as, without a caller
/// ever having to ask what it is holding.
///
/// Re-opens by `found.path` rather than caching a handle from `find_media`'s
/// own probe — a scan can find far more candidates than a single install
/// touches, and holding every one open (or every one's read window) for the
/// whole scan would cost memory nothing here needs.
pub fn open_media(found: &FoundMedia) -> CoreResult<Box<dyn MediaSource>> {
    Ok(match found.kind {
        MediaKind::Floppy => Box::new(AdfSource::open(&found.path)?),
        MediaKind::Disc => Box::new(CdSource::open(&found.path)?),
    })
}

/// Identify one candidate file as install media, or `None` if it is neither.
///
/// A floppy is tried first: it is the cheaper open (a bounded signature
/// scan, see the module doc's "Cost" section) and by far the commoner case
/// in a real media folder — never because a file could plausibly be both.
/// Anything either refuses (not an Amiga volume at all, an RDB, an ISO
/// whose walk hit ART's own limits) comes back `None`.
///
/// The one place this decision is made — [`find_media`] calls it once per
/// candidate in a folder scan, and `apply()` (`core::osinstall::apply`)
/// calls it again per medium a plan already resolved, so `plan.media_paths`
/// (volume name -> path, with no [`MediaKind`] of its own) can still be
/// opened the right way rather than assumed to be a floppy. Before this was
/// pulled out, only `find_media`'s own loop carried the try-floppy-then-disc
/// order, and `apply()` opened every medium through `AdfSource::open`
/// unconditionally — which a real AmigaOS 3.9 disc refuses outright
/// (ART-153).
pub fn identify(path: &Path) -> Option<FoundMedia> {
    let (volume_name, kind) = if let Ok(source) = AdfSource::open(path) {
        (source.volume_name().to_string(), MediaKind::Floppy)
    } else if let Ok(source) = CdSource::open(path) {
        (source.volume_name().to_string(), MediaKind::Disc)
    } else {
        return None;
    };
    Some(FoundMedia {
        path: path.to_path_buf(),
        volume_name,
        kind,
    })
}

/// Every install disk found directly inside `folder`, duplicates included.
///
/// Opens each regular file one directory level deep (no recursion, no
/// symlinks followed) and keeps the ones that open as a single bare
/// AmigaDOS volume; anything else in the folder is skipped, not reported as
/// an error — see the module doc. Two files sharing a volume name are both
/// kept: resolving the ambiguity, or refusing over it, is [`media_for`]'s
/// and its caller's job, not this function's — a folder-wide failure here
/// would refuse an install that never touches the duplicated disk.
pub fn find_media(folder: &Path) -> CoreResult<Vec<FoundMedia>> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(folder)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    // Deterministic order: a directory listing's own order is not
    // guaranteed by any filesystem this runs on, and `MediaMatch::Ambiguous`
    // should not depend on it either — sorted by path, so the paths a
    // caller reports for an ambiguous name are stable from run to run.
    entries.sort();

    let mut found: Vec<FoundMedia> = Vec::new();

    for path in entries {
        // `symlink_metadata`, not `metadata`: a symlink must never be
        // followed, whether it points at a file, a directory, or nothing at
        // all. Its own file type is never `is_file()`, so this one check
        // skips both a symlink and a subdirectory (no recursion) together.
        // An `Err` here — the entry vanished between `read_dir` listing it
        // and this stat, or it is simply unreadable — is skipped the same
        // way as a non-Amiga file, not propagated: one ordinary race on a
        // live filesystem must not fail the whole scan.
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }

        // `identify` tries floppy then disc and comes back `None` for
        // anything neither opens — one unreadable candidate must not fail
        // the whole scan, so that is skipped exactly like today.
        let Some(media) = identify(&path) else {
            continue;
        };
        found.push(media);
    }

    Ok(found)
}

/// Every package archive in `folder`, identified from **inside** each file.
///
/// Deliberately separate from [`find_media`] rather than a third arm of its
/// probe. Install media and packages are different questions asked of
/// different folders — the owner keeps discs in one and archives in another
/// — and folding them together would mean opening every 469 MiB disc in the
/// package folder to find out it is not a package.
///
/// A file that is not an archive, or is an archive of a shape
/// [`ArchiveSource`] refuses, is **skipped rather than fatal** — the same
/// rule `find_media` follows, and for the same reason: one unreadable
/// candidate in a folder of fifty-eight must not fail the scan.
///
/// Sorted by path, the same deterministic order `find_media` promises and
/// for the same reason: a caller's report of what it found must not depend
/// on a directory listing's own, filesystem-dependent order.
pub fn find_packages(folder: &Path) -> CoreResult<Vec<FoundPackage>> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(folder)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    entries.sort();

    let mut found: Vec<FoundPackage> = Vec::new();

    for path in entries {
        // `symlink_metadata`, not `metadata` — a symlink is never followed,
        // matching `find_media`'s own rule for the same reason.
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }

        // Anything that is not an archive, or an archive `ArchiveSource`
        // refuses (no single top-level directory, or more than one), is
        // simply not a package — skipped, not a scan failure.
        let Ok(source) = ArchiveSource::open(&path) else {
            continue;
        };
        found.push(FoundPackage {
            path,
            media: source.volume_name().to_string(),
            refused_names: source.refused_names().to_vec(),
        });
    }

    Ok(found)
}

/// One archive [`find_packages`] opened, and the name it gave for itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundPackage {
    pub path: PathBuf,
    /// The archive's single top-level directory — never its filename.
    pub media: String,
    /// Every entry name in this archive that
    /// [`safe_join`](crate::core::security::safe_join) refused, verbatim —
    /// see [`ArchiveSource::refused_names`]. Carried up from the scan
    /// because this is the one place ART opens every candidate archive in
    /// a folder, so it is the cheapest place to learn the fact and the
    /// earliest place that can say it: the checklist renders it beside the
    /// package's own row, before the tick, rather than after a
    /// confirmation (m6 of the final whole-branch review — the names were
    /// collected from Task 6 onwards and shown nowhere).
    pub refused_names: Vec<String>,
}

/// What resolving a volume name against a scanned folder found.
///
/// **Always `match` this exhaustively — never `if let MediaMatch::Found(..)
/// = ...`.** An `if let` that only names `Found` silently treats
/// `Ambiguous` as if it were `Missing`: the file goes unused, the component
/// looks unsatisfied, and the actual problem (two candidates, one of them
/// possibly wrong) never surfaces. That is the same arbitrary-winner
/// mistake this type exists to rule out, arriving through the back door of
/// an incomplete pattern instead of a bare `Option`. This call decides
/// which bytes become part of somebody's operating system; every arm needs
/// to be handled on purpose.
/// Generic over what was matched (Task 6) rather than fixed to
/// [`FoundMedia`]: a package archive is resolved by exactly the same
/// three-way question — none, one, or several files claiming one name — and
/// two enums of identical shape would be two places for the
/// arbitrary-winner rule to drift apart. The default type parameter keeps
/// every existing `MediaMatch<'a>` spelling meaning what it always did.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub enum MediaMatch<'a, T = FoundMedia> {
    /// No file in `found` carries this name.
    Missing,
    /// Exactly one file does — the ordinary case.
    Found(&'a T),
    /// More than one file claims this name, in `find_media`'s (or
    /// [`find_packages`]'s) own sorted-path order. The caller decides what
    /// to do — typically [`super::RefusalReason::MediaAmbiguous`] for the
    /// one component that actually names this volume, or
    /// [`super::RefusalReason::PackageArchiveAmbiguous`] for the one package
    /// that names this archive.
    Ambiguous(Vec<&'a T>),
}

/// Resolve `volume_name` against `found`.
///
/// Returns an enum rather than an `Option` so ambiguity cannot collapse
/// into an arbitrary "first match": a caller that only wanted `Option`'s
/// `Some`/`None` shape would still have to decide what to do when there are
/// two, and matching on [`MediaMatch`] forces that decision to be made
/// explicitly at the call site instead of being made implicitly, once, in
/// here.
/// Whether a name a medium gave for **itself** — a volume label off a root
/// block, an archive's single top-level directory — is the name a recipe
/// asked for.
///
/// **Case-insensitively, and that is a ruling rather than an accident**
/// (m5 of the final whole-branch review: `package_for` was the one identity
/// comparison in this module tree still exact-case, and the choice was
/// stated nowhere). Both sides of this comparison are AmigaDOS names, and
/// AmigaDOS itself compares them without regard to case (ART-012) — the
/// same rule `destination_key` applies to destinations, `find_entry`
/// applies inside a volume, and all three `MediaSource` implementations
/// apply to a rule's `from`. An exact comparison here made this the one
/// place a user's own `workbench3.2`-labelled disk, or an archive repacked
/// under `boingbag3.9-1/`, was reported as simply not present.
///
/// Folding cannot turn a refusal into a wrong file: two candidates whose
/// names differ only in case become [`MediaMatch::Ambiguous`], which is a
/// named refusal listing both, never an arbitrary winner.
///
/// ASCII-only, matching `eq_ignore_ascii_case` — the same fold
/// [`super::destination_key`] documents at length and for the same reason.
fn same_identity(found: &str, wanted: &str) -> bool {
    found.eq_ignore_ascii_case(wanted)
}

pub fn media_for<'a>(found: &'a [FoundMedia], volume_name: &str) -> MediaMatch<'a> {
    let matches: Vec<&FoundMedia> = found
        .iter()
        .filter(|f| same_identity(&f.volume_name, volume_name))
        .collect();
    match matches.len() {
        0 => MediaMatch::Missing,
        1 => MediaMatch::Found(matches[0]),
        _ => MediaMatch::Ambiguous(matches),
    }
}

/// Does `archive` carry `inner` — a path below its own top-level directory?
///
/// The second half of a package's identity (ART-167), asked of the archive
/// itself rather than of its filename. Re-opens the file rather than
/// caching a listing on [`FoundPackage`] for two reasons: the listing would
/// have to cross the wire with the rest of that struct (1,112 entries for
/// the owner's `BoingBag39-1.lha` alone), and asking through
/// [`MediaSource::entry`] means the path is resolved by the **one** rule
/// `ArchiveSource` already uses for every `from` in every recipe — exact
/// first, then `eq_ignore_ascii_case`, whole path, never a prefix. Two of
/// the eight language drawers are `português` and `português-brasil`, so a
/// prefix match would hand the first the second's archive.
///
/// The cost is bounded by how many archives claim one top-level name: eight
/// small `.lha`s (18–155 entries each, measured) for `LocaleUpdate`, two for
/// `BoingBag3.9-2`, one for everything else in the owner's folder.
///
/// **An archive that will not re-open answers `true`.** `find_packages`
/// opened it moments ago, so a failure here is a genuine anomaly — and the
/// safe direction for an anomaly is to leave the candidate standing, which
/// keeps a two-candidate name `Ambiguous` (a refusal) instead of letting the
/// other candidate win by default. A single surviving candidate that cannot
/// be opened is no loss either: whatever opens it next fails loudly, with a
/// real error, instead of this function inventing a quiet `Missing`.
fn archive_carries(archive: &Path, inner: &str) -> bool {
    let Ok(mut source) = ArchiveSource::open(archive) else {
        return true;
    };
    match source.entry(inner) {
        Ok(found) => found.is_some(),
        Err(_) => true,
    }
}

/// Resolve a package's own identity against what [`find_packages`] found:
/// its `media` — the archive's single top-level directory — **and**, when it
/// declares one, the path that must exist inside the archive
/// ([`super::package::Package::distinguished_by`]).
///
/// Same three-way semantics [`media_for`] gives a volume name, and for the
/// same reason. What is new is the second filter, and **it is applied
/// whether or not `media` alone was ambiguous** — see `package.rs`'s
/// "Identity needs two things" section for the measurement and for why the
/// always-on form is the point rather than an optimisation missed: eight of
/// the owner's archives carry `LocaleUpdate`, and with only the German one
/// present the Turkish package used to resolve to it with no ambiguity to
/// warn anybody.
///
/// Narrowing never picks a winner. Zero survivors is [`MediaMatch::Missing`]
/// (no archive here is this package's); more than one is
/// [`MediaMatch::Ambiguous`] over the narrowed list, which the caller turns
/// into a refusal naming exactly those candidates.
pub fn package_for<'a>(
    found: &'a [FoundPackage],
    media: &str,
    distinguished_by: Option<&str>,
) -> MediaMatch<'a, FoundPackage> {
    let matches: Vec<&FoundPackage> = found
        .iter()
        .filter(|f| same_identity(&f.media, media))
        .filter(|f| match distinguished_by {
            Some(inner) => archive_carries(&f.path, inner),
            None => true,
        })
        .collect();
    match matches.len() {
        0 => MediaMatch::Missing,
        1 => MediaMatch::Found(matches[0]),
        _ => MediaMatch::Ambiguous(matches),
    }
}

/// A package archive resolved to a real file, plus the member inside it that
/// actually holds the payload.
///
/// **This is the [`MediaKind`] bridge.** `MediaKind` names what
/// [`identify`] can learn by probing a file — `Floppy` or `Disc` — and a
/// package is not something probing can answer: which archive is a package
/// comes from [`find_packages`] (a different folder, scanned by a different
/// question), and whether its payload sits at the archive's own paths or
/// inside a nested member comes from the package *recipe*
/// ([`super::package::Package::member`]), which no amount of looking at the
/// file can tell you. So a package travels on a plan as its own kind of
/// resolved medium rather than as a third `MediaKind`, and
/// [`open_package`] is the counterpart to [`open_media`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageMedium {
    pub path: PathBuf,
    /// The nested payload archive, from the package's own recipe. `None`
    /// when the package's files sit at the archive's own paths.
    pub member: Option<String>,
}

/// Open a package archive as a [`MediaSource`] — the package counterpart of
/// [`open_media`], and the one place the `member` distinction is acted on.
pub fn open_package(medium: &PackageMedium) -> CoreResult<Box<dyn MediaSource>> {
    Ok(match &medium.member {
        Some(member) => Box::new(ArchiveSource::open_nested(&medium.path, member)?),
        None => Box::new(ArchiveSource::open(&medium.path)?),
    })
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::osinstall::fixtures::{media, scratch, write_test_iso};

    #[test]
    fn media_is_found_by_its_volume_name_not_its_filename() {
        let dir = scratch("scan-by-volume-name");
        media(&dir, "Workbench3.2", "wb.adf", &[]);
        // The point of the task: a disk that reached this folder under a
        // name that says nothing about what is on it still has to resolve —
        // and resolve to *this* file specifically, not just to something.
        let renamed = media(&dir, "Extras3.2", "totally-unrelated-name.bin", &[]);

        let found = find_media(&dir).unwrap();
        match media_for(&found, "Extras3.2") {
            MediaMatch::Found(entry) => assert_eq!(entry.path, renamed),
            other => panic!("expected Found({}), got {other:?}", renamed.display()),
        }
        match media_for(&found, "Workbench3.2") {
            MediaMatch::Found(entry) => assert_eq!(entry.volume_name, "Workbench3.2"),
            other => panic!("expected Found, got {other:?}"),
        }
    }

    #[test]
    fn a_file_that_is_not_an_amiga_image_is_skipped_not_an_error() {
        let dir = scratch("scan-skip-non-amiga");
        std::fs::write(dir.join("readme.txt"), b"hello").unwrap();
        // Decision 2's other named case: an HDF is a real AmigaDOS image,
        // just not install media — `AdfSource::open` refuses it by name
        // (RDB, not a bare volume), and this scan must skip it the same way
        // it skips `readme.txt`, not fail over it.
        crate::core::hdf::create_hdf(
            &dir.join("games.hdf"),
            32 * 1024 * 1024,
            true,
            &[crate::core::rdb::PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: crate::core::rdb::AmigaHardDiskFs::FfsStandard,
                size_mb: 10,
                bootable: true,
                boot_priority: 0,
                num_buffers: 100,
            }],
            &[],
        )
        .unwrap();
        media(&dir, "Workbench3.2", "wb.adf", &[]);

        let found = find_media(&dir).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].volume_name, "Workbench3.2");
    }

    /// The user's own 3.2 folder holds 36 ADFs; a scan must not descend into
    /// a subdirectory looking for more, nor follow a symlink into one (the
    /// symlink half is documented, not tested here — see the module doc).
    #[test]
    fn the_scan_is_not_recursive() {
        let dir = scratch("scan-not-recursive");
        let nested = dir.join("sub");
        std::fs::create_dir(&nested).unwrap();
        media(&nested, "Workbench3.2", "wb.adf", &[]);

        assert!(find_media(&dir).unwrap().is_empty());
    }

    /// Two files sharing a volume name must both come back from
    /// `find_media` — neither silently dropped — and `media_for` must
    /// report the ambiguity rather than picking one.
    ///
    /// Created in the *opposite* of lexicographic order on purpose: `first`
    /// (`wb-copy-1.adf`) is written second, after `second`
    /// (`wb-copy-2.adf`), so the assertion actually pins the *sorted* order
    /// `find_media` promises rather than passing by coincidence whenever
    /// creation order already happens to match it — which is what the
    /// previous version of this test did (both fixture calls in
    /// lexicographic order), and why it could not tell a present
    /// `entries.sort()` apart from an absent one. Measured, not assumed:
    /// removing `entries.sort()` and running just this test on this
    /// project's own Windows/NTFS dev machine *still* passes, because NTFS
    /// happens to enumerate a directory in name order regardless of
    /// creation order — so this test cannot prove the sort call is load-
    /// bearing on every filesystem, only that `find_media`'s contract
    /// (sorted output) actually holds. The sort stays in `find_media`
    /// because that contract must not depend on which filesystem is under
    /// it, even though this particular test can't force a failure to prove
    /// it either way.
    #[test]
    fn two_same_named_files_are_both_reported_and_neither_is_silently_dropped() {
        let dir = scratch("scan-duplicate-names");
        let second = media(&dir, "Workbench3.2", "wb-copy-2.adf", &[]);
        let first = media(&dir, "Workbench3.2", "wb-copy-1.adf", &[]);

        let found = find_media(&dir).unwrap();
        let matching: Vec<&PathBuf> = found
            .iter()
            .filter(|f| f.volume_name == "Workbench3.2")
            .map(|f| &f.path)
            .collect();
        assert_eq!(
            matching,
            vec![&first, &second],
            "both copies must survive the scan, in sorted (not creation) order"
        );

        match media_for(&found, "Workbench3.2") {
            MediaMatch::Ambiguous(paths) => {
                let ambiguous_paths: Vec<&PathBuf> = paths.iter().map(|f| &f.path).collect();
                assert_eq!(
                    ambiguous_paths,
                    vec![&first, &second],
                    "both candidates must be reported, in order"
                );
            }
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn media_for_returns_missing_when_no_file_carries_the_name() {
        let dir = scratch("scan-media-for-missing");
        media(&dir, "Workbench3.2", "wb.adf", &[]);

        let found = find_media(&dir).unwrap();
        assert_eq!(media_for(&found, "Extras3.2"), MediaMatch::Missing);
    }

    // ---- Task 2: a disc is media too ----

    /// A disc in the media folder is found the same way a floppy is, and by
    /// the volume name recorded inside it.
    #[test]
    fn a_disc_is_found_and_named_by_its_own_volume() {
        let dir = scratch("disc-found");
        write_test_iso(&dir, "os39.iso", "AmigaOS3.9");

        let found = find_media(&dir).unwrap();

        let disc = found
            .iter()
            .find(|m| m.volume_name == "AmigaOS3.9")
            .expect("the disc is media");
        assert_eq!(disc.kind, MediaKind::Disc);
    }

    /// And a floppy is still a floppy — this must not reclassify what
    /// already works.
    #[test]
    fn a_floppy_is_still_found_as_a_floppy() {
        let dir = scratch("floppy-still");
        media(&dir, "Workbench3.2", "wb.adf", &[]);

        let found = find_media(&dir).unwrap();

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, MediaKind::Floppy);
    }

    /// One factory, so a caller never has to ask what it is holding.
    #[test]
    fn the_factory_opens_whichever_kind_was_found() {
        let dir = scratch("factory");
        media(&dir, "Workbench3.2", "wb.adf", &[]);
        write_test_iso(&dir, "os39.iso", "AmigaOS3.9");

        for m in find_media(&dir).unwrap() {
            let source = open_media(&m).unwrap();
            assert_eq!(source.volume_name(), m.volume_name);
        }
    }

    /// A folder holding neither is still not an error — the module's own
    /// rule.
    #[test]
    fn something_that_is_neither_is_skipped_rather_than_failing_the_scan() {
        let dir = scratch("neither");
        std::fs::write(dir.join("notes.txt"), b"not an image").unwrap();

        assert!(find_media(&dir).unwrap().is_empty());
    }

    // ---- Task 2: a package archive is found too, but by a separate scan ----

    /// A package archive in a folder, built the same shape `source_archive`'s
    /// own tests use: a single top-level directory, identified from inside.
    fn package(dir: &Path, filename: &str, top: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let path = dir.join(filename);
        let nested: Vec<(String, &[u8])> = files
            .iter()
            .map(|(name, bytes)| (format!("{top}/{name}"), *bytes))
            .collect();
        let refs: Vec<(&str, &[u8])> = nested.iter().map(|(n, b)| (n.as_str(), *b)).collect();
        std::fs::write(
            &path,
            crate::core::archive::zip::tests::make_zip_with(&refs),
        )
        .unwrap();
        path
    }

    #[test]
    fn a_valid_package_is_found_and_a_non_archive_file_is_skipped_not_fatal() {
        let dir = scratch("packages-mixed");
        std::fs::write(dir.join("readme.txt"), b"not an archive").unwrap();
        package(&dir, "bb1.zip", "BoingBag3.9-1", &[("C/Assign", b"a")]);

        let found = find_packages(&dir).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].media, "BoingBag3.9-1");
    }

    /// Two packages, reported in deterministic (sorted-path) order — the
    /// same contract `find_media`'s own duplicate-name test pins.
    #[test]
    fn two_packages_are_both_reported_in_deterministic_order() {
        let dir = scratch("packages-two");
        // Written in the *opposite* of lexicographic order, for the same
        // reason `find_media`'s duplicate-name test is: passing by
        // coincidence whenever creation order already matches sorted order
        // would not prove `entries.sort()` is load-bearing.
        let second = package(&dir, "z-second.zip", "LocaleUpdate", &[("Locale/x", b"x")]);
        let first = package(&dir, "a-first.zip", "BoingBag3.9-1", &[("C/Assign", b"a")]);

        let found = find_packages(&dir).unwrap();
        let paths: Vec<&PathBuf> = found.iter().map(|f| &f.path).collect();
        assert_eq!(paths, vec![&first, &second]);
    }

    #[test]
    fn an_unreadable_folder_is_an_error_not_an_empty_list() {
        let dir = scratch("packages-unreadable");
        let missing = dir.join("does-not-exist");

        assert!(find_packages(&missing).is_err());
    }

    // ---- resolving a package to its archive (Task 6) ---------------------

    #[test]
    fn a_package_is_resolved_by_the_name_inside_its_archive() {
        let dir = scratch("packages-resolve");
        crate::core::osinstall::fixtures::package_test_archive(&dir, "renamed-by-the-user.zip");
        let found = find_packages(&dir).unwrap();

        match package_for(&found, "TestPack", None) {
            MediaMatch::Found(package) => {
                assert_eq!(package.path, dir.join("renamed-by-the-user.zip"))
            }
            other => panic!("expected one match, got {other:?}"),
        }
        assert_eq!(package_for(&found, "NotHere", None), MediaMatch::Missing);
    }

    /// The real case this enum exists for: one archive beside a copy of
    /// itself. Every claimant is reported, never one of them chosen.
    #[test]
    fn two_archives_claiming_one_name_come_back_ambiguous() {
        let dir = scratch("packages-ambiguous-resolve");
        crate::core::osinstall::fixtures::package_test_archive(&dir, "a.zip");
        crate::core::osinstall::fixtures::package_test_archive(&dir, "b.zip");
        let found = find_packages(&dir).unwrap();

        match package_for(&found, "TestPack", None) {
            MediaMatch::Ambiguous(matches) => assert_eq!(matches.len(), 2),
            other => panic!("expected an ambiguity, got {other:?}"),
        }
    }

    // ---- ART-167: identity needs two things --------------------------

    /// One language pack, shaped like the owner's own.
    ///
    /// **Measured, not invented** (`scripts/lha-package-identity.py`,
    /// 2026-08-20, over all 44 `.lha` archives in the owner's package
    /// folder). Every real one is a level-0 LHA whose names carry the whole
    /// path in a single field, separated by `\\`, encoded Latin-1, and
    /// **declaring no directory entry at all** — `BoingBag39-2-turkce.lha`
    /// is 40 entries and all 40 are files, so `locale/catalogs/türkçe` only
    /// ever exists because `ArchiveSource::with_implicit_directories`
    /// synthesises it. A fixture that wrote explicit drawer rows would be
    /// the more-helpful-than-reality kind: it would resolve without that
    /// synthesis, which every real archive needs.
    ///
    /// The four entries all eight of the real ones share are included for
    /// the same reason — they are what makes the archives genuinely
    /// indistinguishable until the language drawer is looked at.
    fn language_pack(dir: &Path, filename: &str, language: &[u8]) -> PathBuf {
        let mut catalog: Vec<u8> = b"LocaleUpdate\\locale\\catalogs\\".to_vec();
        catalog.extend_from_slice(language);
        catalog.extend_from_slice(b"\\sys\\workbench.catalog");

        let path = dir.join(filename);
        std::fs::write(
            &path,
            crate::core::lha::tests::make_lha_with_raw_names(&[
                (b"LocaleUpdate.info", b"icon" as &[u8]),
                (b"LocaleUpdate\\getlocale", b"amiga program"),
                (b"LocaleUpdate\\Install Locale", b"installer script"),
                (b"LocaleUpdate\\Install Locale.info", b"icon"),
                (&catalog, b"catalog bytes"),
            ]),
        )
        .unwrap();
        path
    }

    /// The Turkish pack's own drawer name — Latin-1 `türkçe`, the bytes read
    /// out of the owner's archive header (`74 FC 72 6B E7 65`, the same ones
    /// `core::lha`'s ART-168 test pins).
    const TURKCE: &[u8] = &[0x74, 0xFC, 0x72, 0x6B, 0xE7, 0x65];
    /// `português` — with `português-brasil` built from it below, the pair
    /// among the real eight that makes a prefix match unsafe.
    const PORTUGUES: &[u8] = &[0x70, 0x6F, 0x72, 0x74, 0x75, 0x67, 0x75, 0xEA, 0x73];

    /// **ART-167.** Eight of the owner's archives carry `LocaleUpdate`, so a
    /// top-level directory alone cannot name one — and everything that
    /// separates them is inside the archive, never in its filename.
    ///
    /// The first assertion is this test's own vacuity guard: asked without a
    /// distinguisher, the same four archives are still `Ambiguous`, which is
    /// exactly what `package_for` answered before the fix. Revert the second
    /// filter and that arm keeps passing while every one below fails.
    #[test]
    fn eight_archives_claiming_localeupdate_are_separated_by_what_is_inside_them() {
        let dir = scratch("packages-language-variants");
        let turkce = language_pack(&dir, "BoingBag39-2-turkce.lha", TURKCE);
        language_pack(&dir, "BoingBag39-2-deutsch.lha", b"deutsch");
        let portugues = language_pack(&dir, "BoingBag39-2-portugues.lha", PORTUGUES);
        let mut brasil = PORTUGUES.to_vec();
        brasil.extend_from_slice(b"-brasil");
        let brasil_path = language_pack(&dir, "BoingBag39-2-portugues-brasil.lha", &brasil);

        let found = find_packages(&dir).unwrap();
        assert_eq!(found.len(), 4, "all four claim one top-level directory");
        assert!(found.iter().all(|f| f.media == "LocaleUpdate"));

        // Vacuity guard: the pre-fix question still has the pre-fix answer.
        match package_for(&found, "LocaleUpdate", None) {
            MediaMatch::Ambiguous(matches) => assert_eq!(matches.len(), 4),
            other => panic!("without a distinguisher this must stay ambiguous, got {other:?}"),
        }

        // The Turkish pack, named by the one thing only it carries.
        match package_for(
            &found,
            "LocaleUpdate",
            Some("locale/catalogs/t\u{FC}rk\u{E7}e"),
        ) {
            MediaMatch::Found(package) => assert_eq!(package.path, turkce),
            other => panic!("expected the Turkish archive, got {other:?}"),
        }

        // Whole path, never a prefix: `português` must not claim
        // `português-brasil`'s archive, nor the other way round.
        match package_for(
            &found,
            "LocaleUpdate",
            Some("locale/catalogs/portugu\u{EA}s"),
        ) {
            MediaMatch::Found(package) => assert_eq!(package.path, portugues),
            other => panic!("expected the Portuguese archive alone, got {other:?}"),
        }
        match package_for(
            &found,
            "LocaleUpdate",
            Some("locale/catalogs/portugu\u{EA}s-brasil"),
        ) {
            MediaMatch::Found(package) => assert_eq!(package.path, brasil_path),
            other => panic!("expected the Brazilian archive alone, got {other:?}"),
        }
    }

    /// The half of ART-167 that has nothing to do with ambiguity, and the
    /// reason the second filter runs unconditionally: with only the German
    /// archive in the folder there is exactly **one** candidate, so
    /// `package_for` answered `Found` and the Turkish package would have
    /// been placed out of a German archive with nothing to warn anybody.
    #[test]
    fn a_single_candidate_of_the_wrong_variant_is_missing_not_found() {
        let dir = scratch("packages-wrong-variant-alone");
        language_pack(&dir, "BoingBag39-2-deutsch.lha", b"deutsch");
        let found = find_packages(&dir).unwrap();

        // Vacuity guard from the other side: the media name on its own still
        // resolves, which is precisely the behaviour being corrected.
        assert!(matches!(
            package_for(&found, "LocaleUpdate", None),
            MediaMatch::Found(_)
        ));
        assert_eq!(
            package_for(
                &found,
                "LocaleUpdate",
                Some("locale/catalogs/t\u{FC}rk\u{E7}e")
            ),
            MediaMatch::Missing,
            "a German archive is not the Turkish package's archive"
        );
    }

    /// An ambiguity the distinguisher cannot settle stays a refusal naming
    /// its candidates — never a winner picked by sort order. The realistic
    /// shape: the same language pack downloaded twice.
    #[test]
    fn an_ambiguity_the_distinguisher_cannot_settle_still_names_both() {
        let dir = scratch("packages-still-ambiguous");
        let a = language_pack(&dir, "BoingBag39-2-turkce.lha", TURKCE);
        let b = language_pack(&dir, "BoingBag39-2-turkce (1).lha", TURKCE);
        // A third archive of another language sits beside them, so this
        // test is not vacuous: without the second filter the refusal would
        // name three candidates including a German pack nobody asked for,
        // and the assertion below would fail.
        let german = language_pack(&dir, "BoingBag39-2-deutsch.lha", b"deutsch");
        let found = find_packages(&dir).unwrap();

        match package_for(
            &found,
            "LocaleUpdate",
            Some("locale/catalogs/t\u{FC}rk\u{E7}e"),
        ) {
            MediaMatch::Ambiguous(matches) => {
                let paths: Vec<&PathBuf> = matches.iter().map(|m| &m.path).collect();
                assert_eq!(paths.len(), 2, "only the two real claimants: {paths:?}");
                assert!(paths.contains(&&a) && paths.contains(&&b));
                assert!(
                    !paths.contains(&&german),
                    "a refusal must name the candidates, not every archive sharing the name"
                );
            }
            other => panic!("expected both candidates named, got {other:?}"),
        }
    }

    /// `member` is the whole reason a package cannot be a third
    /// `MediaKind`: nothing about the file on disk says whether its payload
    /// sits at the archive's own paths or inside a nested archive — the
    /// package recipe says, and `open_package` is where that is acted on.
    #[test]
    fn open_package_opens_the_nested_member_when_the_recipe_names_one() {
        let dir = scratch("packages-open-nested");
        let inner = crate::core::archive::zip::tests::make_zip_with(&[
            ("C/", b"" as &[u8]),
            ("C/Version", b"cmd bytes"),
        ]);
        let outer = dir.join("wrapper.zip");
        std::fs::write(
            &outer,
            crate::core::archive::zip::tests::make_zip_with(&[
                ("Wrapper/", b"" as &[u8]),
                ("Wrapper/Payload", &inner),
                ("Wrapper/C/Updater", b"an amiga program ART does not run"),
            ]),
        )
        .unwrap();

        // Without the member: the wrapper's own paths, installer and all.
        let mut wrapper = open_package(&PackageMedium {
            path: outer.clone(),
            member: None,
        })
        .unwrap();
        assert_eq!(
            wrapper.read("C/Updater").unwrap(),
            b"an amiga program ART does not run"
        );

        // With it: the payload's paths, and the wrapper's identity.
        let mut payload = open_package(&PackageMedium {
            path: outer,
            member: Some("Payload".to_string()),
        })
        .unwrap();
        assert_eq!(payload.volume_name(), "Wrapper");
        assert_eq!(payload.read("C/Version").unwrap(), b"cmd bytes");
    }
}
