//! One set of questions, asked of **every** [`MediaSource`] implementation.
//!
//! ## Why this file exists
//!
//! `AdfSource` and `CdSource` are two implementations of one trait, and the
//! engine above them (`core::osinstall::plan`, `::apply`) is written against
//! the trait, not against either name. Three separate divergences between
//! them were found on the AmigaOS 3.9 branch, **one at a time, each by a
//! human reading the two files side by side**:
//!
//! 1. `entry("")` — root entry on one, `None` on the other.
//! 2. case folding — exact-case only on `CdSource`, case-insensitive on
//!    `AdfSource` (ART-012's rule).
//! 3. `walk(<a file>)` — `Err` on `AdfSource`, `Ok(vec![])` on `CdSource`.
//!
//! A fourth and a fifth were then found the same way, by the final
//! whole-branch review reading three files side by side, and both are now
//! questions below rather than paragraphs in a review:
//!
//! 4. `entry()`'s returned casing — the caller's on `AdfSource`, the
//!    media's on `CdSource`/`ArchiveSource`, with the trait doc naming
//!    `entry` in the rule the whole time.
//! 5. `walk`'s descendant prefix — exact-case on `CdSource`, folded on
//!    `ArchiveSource`, each argued convincingly in its own file.
//!
//! Each was harmless the day it was found only because the one caller that
//! reached it happened to guard against it, and each would have become a
//! silently short or silently wrong install plan the day a second caller
//! did not. Reading two files side by side does not scale to three
//! implementations, and it did not scale to two: the fourth divergence
//! should fail a test, not wait for a reviewer.
//!
//! ## What it covers
//!
//! Both implementations are given the *same logical media* — one file in a
//! drawer, and one genuinely empty drawer — and asked the same eight
//! questions:
//!
//! | Question | Contract |
//! |---|---|
//! | `entry("")` | the media's own root, `is_dir`, path `""` |
//! | `entry(<a drawer>)` | found, `is_dir`, path as asked |
//! | `entry(<missing>)` | `Ok(None)` — absence is not an error |
//! | `entry(<differently cased>)` | found; AmigaDOS is case-insensitive (ART-012) |
//! | `entry(<differently cased>)`'s answer | the **media's** own casing, never the caller's |
//! | `walk(<differently cased drawer>)` | the same entries, spelled the media's way |
//! | `walk("")` | the whole media |
//! | `walk(<a drawer>)` | what is under it, not the drawer itself |
//! | `walk(<missing>)` | `Ok(vec![])` |
//! | `walk(<an empty drawer>)` | `Ok(vec![])` — and *not* an error |
//! | `walk(<a file>)` | `Err(InvalidInput)`, "is a file … not a drawer" |
//! | `read("")` | `Err(InvalidInput)`, "is a drawer … not a file" |
//! | `read(<missing>)` | `Err(InvalidInput)`, "is not on this media" |
//! | `read(<a file>)` | the bytes |
//!
//! The media are built independently — a real blank ADF written through
//! `VolumeWriter`, a real Joliet ISO9660 image through `IsoBuilder`, and a
//! real ZIP, a real LHA and a real 7z through the same writers each
//! format's own tests use — so agreement here is agreement about
//! behaviour, not a shared fixture agreeing with itself.
//!
//! **Three archive formats, not one (M4).** `ArchiveSource` is one
//! implementation of the trait over three quite different readers, and
//! every archive fixture on this branch was a ZIP while two of the three
//! shipped recipes name a `.lha`. That is how ART-168 — `core::lha`
//! replacing an entry name's high-bit bytes with U+FFFD — passed the whole
//! suite and was found by a real run instead.
//!
//! Adding a further `MediaSource` means adding one line to [`sources`]. That
//! is deliberately the cheapest thing in this file to do — `ArchiveSource`
//! is the first one to actually do it, joining `AdfSource` and `CdSource`
//! rather than just being asked to.

use crate::core::error::CoreError;

use super::scan::MediaKind;
use super::scan_cache::{listing_of, CachedSource};
use super::source::{AdfSource, MediaSource};
use super::source_archive::ArchiveSource;
use super::source_cd::CdSource;

/// The logical media every implementation below is given:
///
/// - `C/LoadModule`, a file holding `b"cmd"`
/// - `Empty`, a drawer with nothing in it
///
/// Nothing here is Amiga content — both media are synthesised in a tempdir
/// at test time, as this project requires.
const FILE_PATH: &str = "C/LoadModule";
/// The drawer `FILE_PATH` sits in. A real drawer on the ADF and the ISO;
/// on the ZIP nothing declares it at all and `ArchiveSource` synthesises it
/// from the path under it — which is exactly why it is asked about here
/// rather than only in that source's own tests: an implicit drawer has to
/// answer the same contract a real one does, or the engine above the trait
/// can tell them apart.
const DIR_PATH: &str = "C";
const FILE_BYTES: &[u8] = b"cmd";
const EMPTY_DIR: &str = "Empty";
const MISSING: &str = "Libs/Nothing";

/// A blank ADF carrying the shared tree.
fn floppy(tag: &str) -> Box<dyn MediaSource> {
    use crate::core::volume::device::FileRegionMut;
    use crate::core::volume::write::VolumeWriter;
    use crate::core::volume::{DosType, VolumeGeometry};

    let dir = super::fixtures::scratch(&format!("contract-adf-{tag}"));
    let image = super::fixtures::media(
        &dir,
        "Workbench3.2",
        "wb.adf",
        &[(FILE_PATH, FILE_BYTES, 0x20)],
    );

    // `fixtures::media` writes files, and an *empty* drawer is one of the
    // questions — so it is made here, through the same writer.
    let geometry = VolumeGeometry::floppy_dd(DosType::new(*b"DOS\x01"));
    {
        let mut device =
            FileRegionMut::open(&image, 0, geometry.total_bytes(), geometry.block_size).unwrap();
        let mut writer = VolumeWriter::open(&mut device, geometry, &image, 0).unwrap();
        writer.make_dir(0, EMPTY_DIR).unwrap();
    }

    Box::new(AdfSource::open(&image).unwrap())
}

/// A Joliet ISO9660 disc carrying the same tree.
fn disc(tag: &str) -> Box<dyn MediaSource> {
    use crate::core::iso::fixture::{dir as iso_dir, file as iso_file, IsoBuilder};

    let scratch = super::fixtures::scratch(&format!("contract-iso-{tag}"));
    let bytes = IsoBuilder {
        volume: "WORKBENCH32".to_string(),
        joliet_volume: "Workbench3.2".to_string(),
        joliet: true,
        children: vec![
            iso_dir(
                "C",
                "C",
                vec![iso_file("LOADMOD.;1", "LoadModule", FILE_BYTES)],
            ),
            iso_dir("EMPTY", "Empty", vec![]),
        ],
        ..Default::default()
    }
    .build();
    let path = scratch.join("wb.iso");
    std::fs::write(&path, bytes).unwrap();

    Box::new(CdSource::open(&path).unwrap())
}

/// The shared tree as a package archive's entry list — one file under a
/// single top-level directory, plus an explicitly declared empty drawer.
///
/// A trailing-slash name is how every one of the three formats marks a
/// directory, and it is the archive equivalent of the empty drawer
/// `floppy`/`disc` each make with their own writer.
const ARCHIVE_ENTRIES: &[(&str, &[u8])] = &[
    ("Workbench3.2/C/LoadModule", FILE_BYTES),
    ("Workbench3.2/Empty/", b""),
];

/// One `ArchiveSource` over `bytes`, written to a scratch file named
/// `file_name`. `core::archive::open` decides the format from the file's own
/// bytes, so the extension here is documentation rather than dispatch.
fn archive_source(tag: &str, file_name: &str, bytes: Vec<u8>) -> Box<dyn MediaSource> {
    let scratch = super::fixtures::scratch(&format!("contract-{tag}"));
    let path = scratch.join(file_name);
    std::fs::write(&path, bytes).unwrap();
    Box::new(ArchiveSource::open(&path).unwrap())
}

/// A ZIP package archive carrying the shared tree under a single top-level
/// directory — the volume name an `ArchiveSource` states about itself, and
/// the shape a real catalog pack has.
fn zip_archive(tag: &str) -> Box<dyn MediaSource> {
    archive_source(
        &format!("archive-zip-{tag}"),
        "package.zip",
        crate::core::archive::zip::tests::make_zip_with(ARCHIVE_ENTRIES),
    )
}

/// The same tree as an **LHA**.
///
/// `ArchiveSource` is one type over three formats, and until this every
/// fixture in the branch — the contract's, `source_archive.rs`'s, `scan.rs`'s,
/// `apply.rs`'s and the command layer's — was a ZIP, while **two of the three
/// shipped recipes name a `.lha`**. That is not a gap in coverage of an
/// unused path: it is the mechanism by which ART-168 (`core::lha::entry_path`
/// replacing an entry name's high-bit bytes with U+FFFD) survived the whole
/// suite and was found only by a real run against the owner's own
/// `BoingBag39-2-turkce.lha`. A fixture whose *format* is more helpful than
/// reality hides exactly as much as one whose contents are.
fn lha_archive(tag: &str) -> Box<dyn MediaSource> {
    archive_source(
        &format!("archive-lha-{tag}"),
        "package.lha",
        crate::core::lha::tests::make_lha_with(ARCHIVE_ENTRIES),
    )
}

/// The same tree as a **7z** — the third format `core::archive::open`
/// dispatches to, and the third `ArchiveSource` claims in its own module doc
/// ("LHA, ZIP or 7z"). Reachable the same way the other two are, so it is
/// asked the same questions rather than left as a claim.
fn sevenz_archive(tag: &str) -> Box<dyn MediaSource> {
    archive_source(
        &format!("archive-7z-{tag}"),
        "package.7z",
        crate::core::archive::sevenz::tests::make_7z_with(ARCHIVE_ENTRIES),
    )
}

/// Every implementation, named. **Add a line here when adding a
/// `MediaSource`** — every test below runs over all of them.
///
/// `ArchiveSource` appears three times, once per archive format: it is one
/// implementation of the trait, but three different readers underneath it,
/// and the contract's whole premise is that a caller written against the
/// trait cannot tell its backings apart.
fn sources(tag: &str) -> Vec<(&'static str, Box<dyn MediaSource>)> {
    vec![
        ("AdfSource", floppy(tag)),
        ("CdSource", disc(tag)),
        ("ArchiveSource/zip", zip_archive(tag)),
        ("ArchiveSource/lha", lha_archive(tag)),
        ("ArchiveSource/7z", sevenz_archive(tag)),
        // ART-194. `CachedSource` answers `entry` and `walk` out of a stored
        // listing rather than off the medium, which makes it a **sixth**
        // implementation of these three answers — and the most dangerous one
        // to get subtly wrong, because a divergence here shows up only on the
        // second preview of a disc, after the first one looked right. Two of
        // them, over the two backings whose listings differ most: a real
        // AmigaDOS volume and a Joliet-pressed ISO.
        ("CachedSource/adf", cached(floppy(tag), MediaKind::Floppy)),
        ("CachedSource/disc", cached(disc(tag), MediaKind::Disc)),
    ]
}

/// Wrap an open source in the cache's own [`CachedSource`], through the same
/// [`listing_of`] the cache stores.
///
/// `reopen` hands over **the very source the listing was read from**, once —
/// so the three `read` questions below reach the real medium (which is the
/// cache's own rule: it holds a listing, never bytes), while every listing
/// question is answered from the stored listing alone. A second `reopen` would
/// be a bug in `CachedSource`, so it panics rather than quietly rebuilding.
fn cached(mut source: Box<dyn MediaSource>, kind: MediaKind) -> Box<dyn MediaSource> {
    let listing = listing_of(source.as_mut(), kind).expect("the fixture has a listing");
    let mut once = Some(source);
    Box::new(CachedSource::new(listing, move || {
        Ok(once
            .take()
            .expect("`CachedSource` opens the medium at most once"))
    }))
}

#[test]
fn every_source_answers_an_empty_path_with_its_own_root() {
    for (name, mut source) in sources("root-entry") {
        let entry = source
            .entry("")
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .unwrap_or_else(|| panic!("{name}: `entry(\"\")` answered None"));
        assert!(entry.is_dir, "{name}: the root is a drawer");
        assert_eq!(entry.path, "", "{name}");
    }
}

#[test]
fn every_source_answers_a_drawer_with_a_directory_entry() {
    for (name, mut source) in sources("dir-entry") {
        let entry = source
            .entry(DIR_PATH)
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .unwrap_or_else(|| panic!("{name}: `entry(\"{DIR_PATH}\")` answered None"));
        assert!(entry.is_dir, "{name}: '{DIR_PATH}' is a drawer");
        assert_eq!(entry.path, DIR_PATH, "{name}");
    }
}

#[test]
fn every_source_walks_a_drawer_as_what_is_under_it() {
    for (name, mut source) in sources("dir-walk") {
        let walked = source
            .walk(DIR_PATH)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let paths: Vec<&str> = walked.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, vec![FILE_PATH], "{name}");
    }
}

#[test]
fn every_source_answers_a_missing_path_with_none_rather_than_an_error() {
    for (name, mut source) in sources("missing-entry") {
        assert!(
            source
                .entry(MISSING)
                .unwrap_or_else(|e| panic!("{name}: {e}"))
                .is_none(),
            "{name}: absence is not an error"
        );
    }
}

/// AmigaDOS is case-insensitive (ART-012), so a recipe's `from` spelled in
/// one case must resolve against media spelling it in another. This was the
/// second divergence: `CdSource` matched only the exact byte case.
#[test]
fn every_source_resolves_a_path_case_insensitively() {
    for (name, mut source) in sources("case") {
        assert!(
            source
                .entry("c/loadmodule")
                .unwrap_or_else(|e| panic!("{name}: {e}"))
                .is_some(),
            "{name}: a differently-cased path must still resolve"
        );
    }
}

/// **The fourth divergence (M1), and the first this file catches rather
/// than a reviewer.** The trait doc names `entry` in its casing rule — "the
/// casing of the returned paths is the media's own, not the caller's" —
/// and `AdfSource` returned the *caller's* while `CdSource` and
/// `ArchiveSource` returned the media's. `entry("c/loadmodule")` answered
/// `path: "c/loadmodule"` on a floppy and `path: "C/LoadModule"` on a disc
/// or an archive.
///
/// It was latent only because no production caller reads
/// `MediaEntry::path` off an `entry()` result today (`plan::expand_rules`
/// takes `is_dir` and `size`; `apply` takes `is_dir` and the sidecar
/// fields) — which is exactly what the first three divergences had going
/// for them too, right up until a second caller arrived.
///
/// The contract asserts the media's own spelling, not merely "some
/// spelling": a path a caller can hand straight back to `read` on a medium
/// that distinguishes case is the whole reason the rule was written that
/// way, and `is_some()` — all this test used to check — passes for either
/// answer.
#[test]
fn every_source_answers_with_the_medias_own_casing_never_the_callers() {
    for (name, mut source) in sources("case-answer") {
        let entry = source
            .entry("c/loadmodule")
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .unwrap_or_else(|| panic!("{name}: a differently-cased path must still resolve"));
        assert_eq!(entry.path, FILE_PATH, "{name}: the media's own casing");

        let drawer = source
            .entry("c")
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .unwrap_or_else(|| panic!("{name}: a differently-cased drawer must still resolve"));
        assert_eq!(drawer.path, DIR_PATH, "{name}: the media's own casing");
    }
}

/// **The fifth (M2).** Asking for a drawer in the wrong case must not
/// change *what* the walk contains or *how* its entries are spelled — the
/// prefix a `walk` filters by is the medium's own, so the answer is
/// identical to asking in the medium's own case.
///
/// This is the half of M2 every implementation can be asked. The other half
/// — a medium genuinely holding `Libs/x` beside `libs/y`, where the folded
/// prefix is what makes `walk` yield both — cannot be asked here, because a
/// real AmigaDOS volume cannot express it: `VolumeWriter` refuses the
/// second name (`dir::ensure_available`, case-insensitive, "AmigaDOS would
/// treat `Readme` and `README` as the same entry"). So it is pinned in the
/// two implementations that *can* express it, in their own files —
/// `source_cd.rs`'s `two_drawers_differing_only_in_case_are_one_drawer_to_amigados`
/// and `source_archive.rs`'s
/// `a_drawer_spelled_two_ways_walks_as_one_drawer`. Not a gap in the contract:
/// a question whose answer is undefined for one implementation does not
/// belong in a file whose whole premise is that every implementation
/// answers every question.
#[test]
fn every_source_walks_a_drawer_the_same_whatever_case_it_is_asked_in() {
    for (name, mut source) in sources("case-walk") {
        let asked = source.walk("c").unwrap_or_else(|e| panic!("{name}: {e}"));
        let paths: Vec<&str> = asked.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, vec![FILE_PATH], "{name}");
    }
}

#[test]
fn every_source_walks_the_whole_media_for_an_empty_path() {
    for (name, mut source) in sources("root-walk") {
        let walked = source.walk("").unwrap_or_else(|e| panic!("{name}: {e}"));
        let paths: Vec<&str> = walked.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&FILE_PATH), "{name}: {paths:?}");
        assert!(paths.contains(&EMPTY_DIR), "{name}: {paths:?}");
    }
}

#[test]
fn every_source_walks_a_missing_path_as_empty_rather_than_an_error() {
    for (name, mut source) in sources("missing-walk") {
        assert!(
            source
                .walk(MISSING)
                .unwrap_or_else(|e| panic!("{name}: {e}"))
                .is_empty(),
            "{name}"
        );
    }
}

/// A drawer that is genuinely empty is `Ok(vec![])` — the *other* half of
/// the test above, and the reason "missing" and "empty" answering alike is
/// stated in the trait doc rather than left to be discovered.
#[test]
fn every_source_walks_an_empty_drawer_as_empty_rather_than_an_error() {
    for (name, mut source) in sources("empty-walk") {
        let walked = source
            .walk(EMPTY_DIR)
            .unwrap_or_else(|e| panic!("{name}: an empty drawer is not an error: {e}"));
        assert!(walked.is_empty(), "{name}: {walked:?}");
    }
}

/// The third divergence, now pinned for every implementation at once:
/// asking for the contents of something that has no contents is a refusal,
/// never an empty answer indistinguishable from an empty drawer.
#[test]
fn every_source_refuses_to_walk_a_path_that_names_a_file() {
    for (name, mut source) in sources("file-walk") {
        let err = source
            .walk(FILE_PATH)
            .expect_err(&format!("{name}: walking a file must be refused"));
        assert!(
            matches!(err, CoreError::InvalidInput(_)),
            "{name}: {err:?} should be InvalidInput"
        );
        let text = err.to_string();
        assert!(
            text.contains("is a file on this media, not a drawer"),
            "{name}: {text}"
        );
    }
}

#[test]
fn every_source_refuses_to_read_the_root() {
    for (name, mut source) in sources("root-read") {
        let err = source
            .read("")
            .expect_err(&format!("{name}: the root is not a file"));
        assert!(
            matches!(err, CoreError::InvalidInput(_)),
            "{name}: {err:?} should be InvalidInput"
        );
        let text = err.to_string();
        assert!(
            text.contains("is a drawer on this media, not a file"),
            "{name}: {text}"
        );
    }
}

#[test]
fn every_source_refuses_to_read_a_missing_path() {
    for (name, mut source) in sources("missing-read") {
        let err = source
            .read(MISSING)
            .expect_err(&format!("{name}: a missing file is not readable"));
        let text = err.to_string();
        assert!(text.contains("is not on this media"), "{name}: {text}");
    }
}

#[test]
fn every_source_reads_a_files_bytes() {
    for (name, mut source) in sources("read") {
        assert_eq!(
            source
                .read(FILE_PATH)
                .unwrap_or_else(|e| panic!("{name}: {e}")),
            FILE_BYTES,
            "{name}"
        );
    }
}
