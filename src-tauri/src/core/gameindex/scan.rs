//! A folder into a catalogue.
//!
//! **The user's collection folder is read-only to ART.** Nothing here creates,
//! renames or writes anything under the folder being scanned; the catalogue is
//! a value the caller decides what to do with.
//!
//! Four readers answer for four shapes, and where two of them answer the same
//! question the record keeps the winner **and** where it came from. The rule is
//! always the same — a declaration beats an inference — but it is written out
//! at each record builder rather than hidden in a helper, because the
//! interesting case deserves to be readable: a slave's `ReqAGA` bit against a
//! filename containing the letters `AGA` is not the same kind of choice as a
//! manifest's year against a filename's.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::core::error::CoreResult;
use crate::core::gameindex::readers::{rp9, slave, tosec, whdhdf};
use crate::core::gameindex::record::{
    derive_id, ChipsetRequirement, Fact, GameRecord, KickstartNeed, Media, Provenance, SourceRef,
    GAMEINDEX_SCHEMA,
};
use crate::core::hashing::sha256_file;
use crate::core::jobs::{cancelled_error, NoProgress, ProgressSink};

/// How deep a collection scan will descend.
///
/// The same posture `core::collection`'s scan takes, and for the same reason:
/// a directory symlink pointing back up the tree would otherwise recurse until
/// the stack overflows, which with `panic = "abort"` takes the application with
/// it.
const MAX_SCAN_DEPTH: usize = 12;

/// One catalogued title and where its file sits.
///
/// The path lives **here** rather than in the record, because a record travels
/// to a card and a path does not belong on one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CatalogueEntry {
    pub path: String,
    pub record: GameRecord,
}

/// Scan a folder. Convenience wrapper for callers with nothing to report to.
pub fn scan_titles(dir: &Path) -> CoreResult<Vec<CatalogueEntry>> {
    scan_titles_with(dir, &NoProgress)
}

/// Scan a folder, reporting progress and honouring cancellation.
pub fn scan_titles_with(
    dir: &Path,
    progress: &dyn ProgressSink,
) -> CoreResult<Vec<CatalogueEntry>> {
    if !dir.is_dir() {
        return Err(crate::core::error::CoreError::InvalidInput(format!(
            "Directory not found at '{}'",
            dir.display()
        )));
    }

    // Walking the tree comes first and its size is unknown until it finishes —
    // an indefinite phase rather than an invented percentage.
    progress.report(0, None, "Looking for titles…");
    let mut files = Vec::new();
    collect(dir, &mut files, 0);
    files.sort();

    let total = files.len() as u64;
    let mut by_id: BTreeMap<String, CatalogueEntry> = BTreeMap::new();

    for (index, path) in files.iter().enumerate() {
        // Between whole files is the only safe place to stop, and nothing has
        // been written in any case.
        if progress.is_cancelled() {
            return Err(cancelled_error());
        }
        let short = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        progress.report(index as u64 + 1, Some(total), &short);

        match read_one(path) {
            Ok(Some(record)) => {
                // Identical bytes are one title. A listing that shows the same
                // game twice because it sits in two folders is the bug the
                // content-derived id exists to prevent.
                by_id.entry(record.id.clone()).or_insert(CatalogueEntry {
                    path: path.to_string_lossy().to_string(),
                    record,
                });
            }
            Ok(None) => {}
            // One unreadable file must not lose the other 1696. A cancellation
            // is the exception: it is the user's answer, not a bad file.
            Err(err) if is_cancelled_error(&err) => return Err(err),
            Err(err) => log::debug!("gameindex: skipping {}: {err}", path.display()),
        }
    }

    let mut out: Vec<_> = by_id.into_values().collect();
    out.sort_by(|a, b| {
        a.record
            .title
            .value
            .to_lowercase()
            .cmp(&b.record.title.value.to_lowercase())
    });
    Ok(out)
}

fn is_cancelled_error(err: &crate::core::error::CoreError) -> bool {
    matches!(err, crate::core::error::CoreError::Cancelled)
}

/// The largest file the index will read as a single title.
///
/// **Measured, after the screen was driven for the first time.** The user's
/// collection folder also holds `WHDLoadPiStorm-180224.img` — a 29 GB card
/// image sitting beside 1697 two-megabyte games. Nothing about it is a title,
/// but `.img` is a hardfile extension, so the scan tried to hash all 29 GB of
/// it and appeared to hang at "100%".
///
/// 512 MiB is far above any single-game hardfile (the largest real one here is
/// 93 MB) and far below a card. A file past it is a container, not a game, and
/// is skipped rather than hashed.
const MAX_TITLE_BYTES: u64 = 512 * 1024 * 1024;

/// Read one file, or `Ok(None)` when it is not something the index describes.
///
/// **Identify first, hash second.** The content hash is what gives a record its
/// path-independent identity, so it has to be taken — but taking it before
/// deciding whether the file is a title at all means the most expensive step
/// runs on files that are then thrown away. That is what a 29 GB card image in
/// the collection folder turned into a stall.
fn read_one(path: &Path) -> CoreResult<Option<GameRecord>> {
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if !matches!(extension.as_str(), "rp9" | "hdf" | "img" | "adf" | "adz") {
        return Ok(None);
    }

    let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if bytes > MAX_TITLE_BYTES {
        log::debug!(
            "gameindex: {} is {bytes} bytes, past the ceiling for a single title",
            path.display()
        );
        return Ok(None);
    }

    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let named = tosec::read_filename(&file_name);

    // Read what the file *is* before hashing what it holds.
    enum Read {
        Rp9(Box<rp9::Rp9Facts>),
        Hardfile(Box<whdhdf::HardfileGame>),
        NameOnly(Media),
    }

    let read = match extension.as_str() {
        "rp9" => Read::Rp9(Box::new(rp9::read_rp9(path)?)),
        "hdf" | "img" => match whdhdf::read_whdload_hardfile(path) {
            Ok(game) => Read::Hardfile(Box::new(game)),
            // A hard disk image that is not a single-game hardfile is still a
            // thing the user has; the filename is all ART can say about it.
            Err(_) => Read::NameOnly(Media::Hardfile {
                file: file_name.clone(),
            }),
        },
        _ => Read::NameOnly(Media::Floppies {
            ordered: vec![file_name.clone()],
        }),
    };

    let source = SourceRef {
        name: file_name.clone(),
        sha256: sha256_file(path)?,
        bytes,
    };

    Ok(Some(match read {
        Read::Rp9(facts) => from_rp9(*facts, &named, source),
        Read::Hardfile(game) => from_hardfile(*game, &named, source, &file_name),
        Read::NameOnly(media) => from_filename(&named, source, media),
    }))
}

/// Build a record from an `.rp9` manifest, with the filename filling gaps.
fn from_rp9(facts: rp9::Rp9Facts, named: &tosec::TosecFacts, source: SourceRef) -> GameRecord {
    let media = match (&facts.hardfile, facts.floppies.is_empty()) {
        (Some(file), _) => Media::Hardfile { file: file.clone() },
        (None, false) => Media::Floppies {
            ordered: facts.floppies.clone(),
        },
        (None, true) => Media::Floppies { ordered: vec![] },
    };

    let title = Fact::new(facts.title.clone(), Provenance::Rp9Manifest);
    let kickstart = facts.system_rom.map(|rom| {
        Fact::new(
            KickstartNeed {
                rom_version: Some(rom),
                ..KickstartNeed::default()
            },
            Provenance::Rp9Manifest,
        )
    });

    GameRecord {
        schema: GAMEINDEX_SCHEMA,
        id: derive_id(&facts.title, &source.sha256),
        title,
        kind: facts
            .kind
            .map(|kind| Fact::new(kind, Provenance::Rp9Manifest)),
        year: facts
            .year
            .map(|y| Fact::new(y, Provenance::Rp9Manifest))
            .or_else(|| named.year.map(|y| Fact::new(y, Provenance::TosecName))),
        publisher: facts
            .publisher
            .clone()
            .map(|p| Fact::new(p, Provenance::Rp9Manifest))
            .or_else(|| {
                named
                    .publisher
                    .clone()
                    .map(|p| Fact::new(p, Provenance::TosecName))
            }),
        genre: facts
            .genre
            .as_deref()
            .and_then(rp9::map_genre)
            .map(|g| Fact::new(g.to_string(), Provenance::Rp9Manifest)),
        rating: facts.rating.map(|r| Fact::new(r, Provenance::Rp9Manifest)),
        chipset: chipset_from_machine(facts.machine.as_deref())
            .map(|c| Fact::new(c, Provenance::Rp9Manifest))
            .or_else(|| named.chipset.map(|c| Fact::new(c, Provenance::TosecName))),
        kickstart,
        media,
        preview: facts.preview,
        source,
    }
}

/// Build a record from a bootable hardfile's slave.
fn from_hardfile(
    game: whdhdf::HardfileGame,
    named: &tosec::TosecFacts,
    source: SourceRef,
    file_name: &str,
) -> GameRecord {
    let (stated_year, stated_publisher) = game
        .slave
        .copyright
        .as_deref()
        .map(slave::split_copyright)
        .unwrap_or((None, None));

    // The slave's own name, then the drawer, then the filename. The middle one
    // is why `Moonstone Install` exists as a warning rather than a title.
    let title = match game.slave.name.clone().filter(|n| !n.is_empty()) {
        Some(name) => Fact::new(name, Provenance::WhdloadSlave),
        None if !game.drawer.is_empty() => Fact::new(game.drawer.clone(), Provenance::DrawerName),
        None => Fact::new(named.title.clone(), Provenance::TosecName),
    };

    let kickstart = (!game.slave.kickstart.is_empty())
        .then(|| Fact::new(game.slave.kickstart.clone(), Provenance::WhdloadSlave));

    GameRecord {
        schema: GAMEINDEX_SCHEMA,
        id: derive_id(&title.value, &source.sha256),
        title,
        // A WHDLoad hardfile holds an installed program; nothing in it says
        // whether that is a game or a demo, and §14/§34 forbid guessing.
        kind: None,
        year: stated_year
            .map(|y| Fact::new(y, Provenance::WhdloadSlave))
            .or_else(|| named.year.map(|y| Fact::new(y, Provenance::TosecName))),
        publisher: stated_publisher
            .map(|p| Fact::new(p, Provenance::WhdloadSlave))
            .or_else(|| {
                named
                    .publisher
                    .clone()
                    .map(|p| Fact::new(p, Provenance::TosecName))
            }),
        genre: None,
        rating: None,
        // The slave's ReqAGA bit is a statement and a filename is a guess, so
        // the bit wins — and when it is clear the filename is still offered,
        // because a clear bit states nothing either way.
        chipset: slave::chipset_of(&game.slave)
            .map(|c| Fact::new(c, Provenance::WhdloadSlave))
            .or_else(|| named.chipset.map(|c| Fact::new(c, Provenance::TosecName))),
        kickstart,
        media: Media::WhdloadDrawer {
            slave: game.slave_name.clone(),
        },
        preview: None,
        source: SourceRef {
            name: file_name.to_string(),
            ..source
        },
    }
}

/// Everything ART can say when only the filename is available.
fn from_filename(named: &tosec::TosecFacts, source: SourceRef, media: Media) -> GameRecord {
    GameRecord {
        schema: GAMEINDEX_SCHEMA,
        id: derive_id(&named.title, &source.sha256),
        title: Fact::new(named.title.clone(), Provenance::TosecName),
        kind: None,
        year: named.year.map(|y| Fact::new(y, Provenance::TosecName)),
        publisher: named
            .publisher
            .clone()
            .map(|p| Fact::new(p, Provenance::TosecName)),
        genre: None,
        rating: None,
        chipset: named.chipset.map(|c| Fact::new(c, Provenance::TosecName)),
        kickstart: None,
        media,
        preview: None,
        source,
    }
}

/// What an `.rp9`'s `<system>` says about the chipset.
///
/// Only the AGA machines state one. `a-500` does not mean "OCS required" — a
/// package targeting an A500 runs on an A1200 too — so silence stays silence.
fn chipset_from_machine(machine: Option<&str>) -> Option<ChipsetRequirement> {
    match machine?.to_ascii_lowercase().as_str() {
        "a-1200" | "a-4000" | "cd-32" => Some(ChipsetRequirement::Aga),
        _ => None,
    }
}

fn collect(dir: &Path, acc: &mut Vec<PathBuf>, depth: usize) {
    if depth >= MAX_SCAN_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // `symlink_metadata` does not follow links, so a directory symlink
        // pointing back up the tree is skipped rather than followed.
        let is_symlink = std::fs::symlink_metadata(&path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        if is_symlink {
            continue;
        }
        if path.is_dir() {
            collect(&path, acc, depth + 1);
        } else if path.is_file() {
            acc.push(path);
        }
    }
}

// There is deliberately no `merge(stated, inferred)` helper here. Each record
// builder above writes its own preference inline — the stronger source first,
// `.or_else` for the weaker one — so the choice is visible at the point it is
// made. A helper would move five different decisions behind one name and make
// the interesting one (a slave's ReqAGA bit versus a filename's "AGA") look
// like the boring ones.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gameindex::readers::slave::tests_support::build_slave;
    use crate::core::jobs::CancelToken;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-scan-{tag}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_loose_adf(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, format!("fake adf for {name}")).unwrap();
        path
    }

    fn write_hardfile_game(dir: &Path, drawer: &str, stem: &str, states: &'static str) -> PathBuf {
        use crate::core::volume::device::FileRegionMut;
        use crate::core::volume::fixture::make_ffs_volume;
        use crate::core::volume::write::{FileMeta, VolumeWriter};
        use crate::core::volume::{DosType, VolumeGeometry};

        const BLOCKS: u32 = 1843;
        let image = dir.join(format!("{drawer}.hdf"));
        std::fs::write(&image, make_ffs_volume(BLOCKS, "Game", &[])).unwrap();

        let geometry = VolumeGeometry::new(512, BLOCKS, 2, DosType::new(*b"DOS\x01")).unwrap();
        let mut device = FileRegionMut::open(&image, 0, BLOCKS as u64 * 512, 512).unwrap();
        let mut writer = VolumeWriter::open(&mut device, geometry, &image, 0).unwrap();
        let root = writer.geometry().root_block;
        writer.make_dir(root, drawer).unwrap();
        let drawer_block = writer.find(root, drawer).unwrap().unwrap().block;
        writer
            .add_file(
                drawer_block,
                &format!("{stem}.slave"),
                &build_slave(states, "1992 Gremlin", 16),
                FileMeta::default(),
            )
            .unwrap();
        drop(writer);
        image
    }

    fn write_rp9_game(dir: &Path, title: &str) -> PathBuf {
        use std::io::Write;

        let manifest = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<rp9 xmlns="http://www.retroplatform.com"><application><description>
<type>game</type><title>{title}</title><year>1996</year>
<entity type="publisher">Insane Software</entity><genre>driving-simulation</genre>
<systemrom>310</systemrom></description>
<configuration><system>a-1200</system></configuration>
<media><floppy priority="1">one.adf</floppy></media>
</application></rp9>"#
        );

        let path = dir.join(format!("{title}.rp9"));
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("rp9-manifest.xml", options).unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();
        zip.start_file("one.adf", options).unwrap();
        zip.write_all(b"fake").unwrap();
        zip.finish().unwrap();
        path
    }

    /// A folder holding one of each kind comes back with one record each, and
    /// each record names the source that produced it.
    #[test]
    fn a_mixed_folder_yields_one_record_per_title() {
        let dir = scratch("mixed");
        write_hardfile_game(&dir, "Lotus3HD", "Lotus3", "Lotus 3");
        write_rp9_game(&dir, "Aerial Racers");
        write_loose_adf(&dir, "Zool (1992)(Gremlin)(AGA).adf");

        let found = scan_titles(&dir).unwrap();
        assert_eq!(found.len(), 3);

        let titles: Vec<_> = found
            .iter()
            .map(|e| (e.record.title.value.as_str(), e.record.title.from))
            .collect();
        assert_eq!(
            titles,
            vec![
                ("Aerial Racers", Provenance::Rp9Manifest),
                ("Lotus 3", Provenance::WhdloadSlave),
                ("Zool", Provenance::TosecName),
            ],
            "the slave's own name beats the file's"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A file no reader can make sense of is skipped, and the scan carries on.
    /// One bad file in 1697 must not lose the other 1696.
    #[test]
    fn an_unreadable_file_does_not_stop_the_scan() {
        let dir = scratch("bad");
        write_loose_adf(&dir, "Fine (1992)(Someone).adf");
        std::fs::write(dir.join("broken.hdf"), b"not an image").unwrap();
        std::fs::write(dir.join("broken.rp9"), b"not a zip").unwrap();
        std::fs::write(dir.join("notes.txt"), b"ignored entirely").unwrap();

        let found = scan_titles(&dir).unwrap();
        // The ADF and the unreadable .hdf, which still catalogues by name.
        assert!(
            found.iter().any(|e| e.record.title.value == "Fine"),
            "{found:?}"
        );
        assert!(
            !found.iter().any(|e| e.record.title.value == "notes"),
            "a text file is not a title"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Cancellation is honoured between files and reported as a cancellation,
    /// never as a failure.
    #[test]
    fn the_scan_stops_when_asked() {
        struct CancelAfter {
            seen: AtomicU64,
            after: u64,
            token: CancelToken,
        }
        impl ProgressSink for CancelAfter {
            fn report(&self, _done: u64, _total: Option<u64>, _message: &str) {
                if self.seen.fetch_add(1, Ordering::SeqCst) >= self.after {
                    self.token.cancel();
                }
            }
            fn is_cancelled(&self) -> bool {
                self.token.is_cancelled()
            }
        }

        let dir = scratch("cancel");
        for n in 0..6 {
            write_loose_adf(&dir, &format!("Game {n} (1992)(Someone).adf"));
        }

        let sink = CancelAfter {
            seen: AtomicU64::new(0),
            after: 2,
            token: CancelToken::default(),
        };
        let err = scan_titles_with(&dir, &sink).unwrap_err();
        assert!(is_cancelled_error(&err), "{err}");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A file too large to be one title is skipped, and skipped **cheaply**.
    ///
    /// The user's collection folder holds a 29 GB card image beside 1697
    /// two-megabyte games. It is not a title, and the scan must decide that
    /// without hashing it — which is what it did before the screen was driven
    /// for the first time, and what made a finished-looking bar sit at 100%.
    #[test]
    fn a_file_too_large_for_a_title_is_skipped_without_being_read() {
        let dir = scratch("too-big");
        let path = dir.join("Card.img");

        // Sparse: `set_len` costs nothing to create and everything to hash, so
        // a reader that still hashed it would take visibly longer than a test
        // has patience for.
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_TITLE_BYTES + 1).unwrap();
        drop(file);

        write_loose_adf(&dir, "Zool (1992)(Gremlin).adf");

        let started = std::time::Instant::now();
        let found = scan_titles(&dir).unwrap();
        let elapsed = started.elapsed();

        assert_eq!(found.len(), 1, "only the ADF is a title");
        assert_eq!(found[0].record.title.value, "Zool");
        assert!(
            elapsed < std::time::Duration::from_secs(20),
            "the oversized file must be skipped, not hashed (took {elapsed:?})"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Two files whose bytes are identical are one record. The id is
    /// content-derived, and a listing showing the same game twice because it
    /// sits in two folders is the bug that prevents.
    #[test]
    fn identical_bytes_collapse_to_one_record() {
        let dir = scratch("dupe");
        let original = write_loose_adf(&dir, "Zool (1992)(Gremlin).adf");
        std::fs::create_dir_all(dir.join("backup")).unwrap();
        std::fs::copy(&original, dir.join("backup/Zool (1992)(Gremlin).adf")).unwrap();

        let found = scan_titles(&dir).unwrap();
        assert_eq!(found.len(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A stated chipset beats a guessed one, and the record says which won.
    ///
    /// `Agassi Tennis` is the filename reader's known false positive: the
    /// letters `AGA` are in the name and the game has no AGA requirement. A
    /// slave that does not set `ReqAGA` states nothing, so the guess is still
    /// offered — but a slave that *does* set it wins outright.
    #[test]
    fn a_stated_chipset_beats_a_guessed_one() {
        let dir = scratch("chipset");
        let image = write_hardfile_game(&dir, "AgassiTennis", "Agassi", "Agassi Tennis");
        let found = scan_titles(&dir).unwrap();

        let record = &found[0].record;
        assert_eq!(record.title.value, "Agassi Tennis");
        assert_eq!(record.title.from, Provenance::WhdloadSlave);
        // The fixture's slave sets no flags, so the filename's guess survives —
        // and is marked as a guess.
        assert_eq!(
            record.chipset.as_ref().map(|c| c.from),
            Some(Provenance::TosecName),
            "a clear ReqAGA bit states nothing, so the guess is what is left"
        );

        drop(image);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The whole collection, through the product's own path. `#[ignore]`d and
    /// env-gated.
    ///
    /// ```text
    /// cd src-tauri && ART_SCAN_DIR="E:\amiga\Amigatolon\WHDload" \
    ///   cargo test scan_a_real_collection_when_asked -- --nocapture --ignored
    /// ```
    #[test]
    #[ignore]
    fn scan_a_real_collection_when_asked() {
        let Ok(dir) = std::env::var("ART_SCAN_DIR") else {
            eprintln!("ART_SCAN_DIR unset — skipping");
            return;
        };

        let found = scan_titles(Path::new(&dir)).unwrap();
        let mut by_source: BTreeMap<String, usize> = BTreeMap::new();
        let mut with_year = 0usize;
        let mut with_publisher = 0usize;
        let mut with_kickstart = 0usize;

        for entry in &found {
            *by_source
                .entry(format!("{:?}", entry.record.title.from))
                .or_default() += 1;
            if entry.record.year.is_some() {
                with_year += 1;
            }
            if entry.record.publisher.is_some() {
                with_publisher += 1;
            }
            if entry.record.kickstart.is_some() {
                with_kickstart += 1;
            }
        }

        for entry in found.iter().take(8) {
            println!(
                "{:<40} {:?} by {:?} ({:?})",
                entry.record.title.value,
                entry.record.year.as_ref().map(|f| f.value),
                entry.record.publisher.as_ref().map(|f| &f.value),
                entry.record.title.from
            );
        }
        println!("\n{} titles", found.len());
        println!("titles by source: {by_source:?}");
        println!("{with_year} with a year, {with_publisher} with a publisher, {with_kickstart} asking for a Kickstart");
        assert!(!found.is_empty(), "nothing catalogued under {dir}");
    }
}
