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
//! | `entry(<missing>)` | `Ok(None)` — absence is not an error |
//! | `entry(<differently cased>)` | found; AmigaDOS is case-insensitive (ART-012) |
//! | `walk("")` | the whole media |
//! | `walk(<missing>)` | `Ok(vec![])` |
//! | `walk(<an empty drawer>)` | `Ok(vec![])` — and *not* an error |
//! | `walk(<a file>)` | `Err(InvalidInput)`, "is a file … not a drawer" |
//! | `read("")` | `Err(InvalidInput)`, "is a drawer … not a file" |
//! | `read(<missing>)` | `Err(InvalidInput)`, "is not on this media" |
//! | `read(<a file>)` | the bytes |
//!
//! The media are built independently — a real blank ADF written through
//! `VolumeWriter`, a real Joliet ISO9660 image through `IsoBuilder`, and a
//! real ZIP through the same writer `core::archive::zip`'s own tests use —
//! so agreement here is agreement about behaviour, not a shared fixture
//! agreeing with itself.
//!
//! Adding a further `MediaSource` means adding one line to [`sources`]. That
//! is deliberately the cheapest thing in this file to do — `ArchiveSource`
//! is the first one to actually do it, joining `AdfSource` and `CdSource`
//! rather than just being asked to.

use crate::core::error::CoreError;

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

/// A ZIP package archive carrying the shared tree under a single top-level
/// directory — the volume name an `ArchiveSource` states about itself, and
/// the shape a real BoingBag or catalog pack has.
fn archive(tag: &str) -> Box<dyn MediaSource> {
    let scratch = super::fixtures::scratch(&format!("contract-archive-{tag}"));
    let bytes = crate::core::archive::zip::tests::make_zip_with(&[
        ("Workbench3.2/C/LoadModule", FILE_BYTES),
        // A trailing-slash name is how a ZIP marks a directory (`is_dir()`
        // reads the name itself) — the archive equivalent of the empty
        // drawer `floppy`/`disc` each make with their own writer.
        ("Workbench3.2/Empty/", b""),
    ]);
    let path = scratch.join("package.zip");
    std::fs::write(&path, bytes).unwrap();

    Box::new(ArchiveSource::open(&path).unwrap())
}

/// Every implementation, named. **Add a line here when adding a
/// `MediaSource`** — every test below runs over all of them.
fn sources(tag: &str) -> Vec<(&'static str, Box<dyn MediaSource>)> {
    vec![
        ("AdfSource", floppy(tag)),
        ("CdSource", disc(tag)),
        ("ArchiveSource", archive(tag)),
    ]
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
