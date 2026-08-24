//! The Aminet chain, run against the real Aminet.
//!
//! Every other test of `core/sources` hands it a `MockMirror`, which is right:
//! the security decisions must be checkable without a socket. What no mock can
//! say is whether the mirrors ART **ships** still answer, and whether the
//! bytes a real one sends still pass ART's own gates. Those are claims about
//! the world, and the world moves.
//!
//! # Why a test rather than a script
//!
//! ART's other outside checks are scripts because they compare ART's *output*
//! against another tool — amitools reads what ART wrote, 7-Zip reads the FAT32
//! ART laid down. This one is the other shape: what has to be exercised is
//! ART's *own* fetching path, so re-implementing the fetch in Python would
//! measure Python.
//!
//! # Nothing here runs in CI
//!
//! These leave the machine, so they are `#[ignore]`d and run on purpose:
//!
//! ```text
//! cd src-tauri && cargo test live_aminet -- --ignored --nocapture
//! ```
//!
//! Failing one does not mean ART is broken. A mirror going down is a fact
//! about that mirror, and the sentence it prints says which — the list is
//! configuration, editable in the Aminet studio, not a constant to patch.
//!
//! # Politeness
//!
//! One index per mirror and exactly one package, chosen small. The package is
//! picked out of the index **at run time** rather than named here: a hard-coded
//! `AmiSSL-5.5.lha` rots the first time somebody uploads 5.6, and a check that
//! fails because a package moved says nothing about the chain it was written
//! to measure.

use std::path::Path;

use crate::core::jobs::NoProgress;
use crate::core::sources::cache::CacheLayout;
use crate::core::sources::catalog::jsonl::JsonlCatalogStore;
use crate::core::sources::catalog::CatalogStore;
use crate::core::sources::fetch::fetch_package;
use crate::core::sources::index::parse_index_bytes;
use crate::core::sources::install::unpack_for_install;
use crate::core::sources::mirror::{Mirror, MirrorClient};
use crate::core::sources::sync::{aminet_defaults, sync_catalog};
use crate::core::sources::PackageMeta;
use crate::core::ScratchDir;
use crate::net::http_mirror::HttpMirrorClient;

/// The window a package must fall in to be the one this check downloads.
///
/// Above the floor because a 900-byte `.lha` is a stub rather than a program,
/// and unpacking one would prove less than it looks; below the ceiling out of
/// manners towards a volunteer-run mirror.
const SMALL_ENOUGH: std::ops::RangeInclusive<u64> = 4 * 1024..=64 * 1024;

/// Fetch one mirror's index and say how it parsed.
fn index_from(mirror: &Mirror, index_path: &str) -> Result<(usize, usize, u64), String> {
    let client = HttpMirrorClient::new();
    let url = mirror.url_for(index_path).map_err(|e| e.to_string())?;

    let mut bytes = Vec::new();
    client
        .fetch(&url, 0, &mut bytes, &NoProgress)
        .map_err(|e| format!("{e}"))?;

    let (entries, report) = parse_index_bytes(&bytes, "aminet");
    if !report.looks_complete() {
        return Err(format!(
            "answered {} bytes but the parse was not complete: {} parsed, {} skipped",
            bytes.len(),
            report.parsed,
            report.skipped
        ));
    }
    Ok((entries.len(), report.skipped, bytes.len() as u64))
}

/// The smallest package in the window, ties broken by path.
///
/// Deterministic on purpose: two runs a minute apart download the same file,
/// so a failure is about the chain rather than about which package the run
/// happened to land on.
fn smallest_in_window(entries: &[PackageMeta]) -> Option<&PackageMeta> {
    entries
        .iter()
        .filter(|p| p.name.to_lowercase().ends_with(".lha"))
        .filter(|p| SMALL_ENOUGH.contains(&p.size_bytes))
        .min_by(|a, b| {
            a.size_bytes
                .cmp(&b.size_bytes)
                .then_with(|| a.reference.path.cmp(&b.reference.path))
        })
}

/// **Every shipped mirror, one at a time.**
///
/// The reason this is not "sync and see if it works": `fetch_with_failover`
/// tries them in order and stops at the first that answers, so a dead mirror
/// in position two is invisible in an ordinary sync and stays in the shipped
/// list until somebody with only that mirror reachable finds it. The list in
/// `aminet_defaults` already carries four names dropped for TLS failures on
/// 2026-08-09; this is how the next four get noticed.
///
/// Every mirror is tried before anything is asserted, so one dead mirror does
/// not hide the state of the others.
#[test]
#[ignore = "leaves the machine: fetches the real index from every shipped mirror"]
fn every_shipped_mirror_still_answers_a_complete_index() {
    let provider = aminet_defaults().expect("the shipped defaults must parse");
    let mut failures = Vec::new();

    for mirror in &provider.mirrors {
        match index_from(mirror, &provider.index_path) {
            Ok((parsed, skipped, bytes)) => {
                println!(
                    "  ok   {:<28} {bytes:>9} bytes, {parsed} packages, {skipped} skipped",
                    mirror.name
                );
            }
            Err(why) => {
                println!("  DOWN {:<28} {why}", mirror.name);
                failures.push(format!("{}: {why}", mirror.name));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "shipped mirrors that did not answer a complete index:\n  {}",
        failures.join("\n  ")
    );
}

/// A sync into a real catalogue file, through the same call the command makes.
///
/// Asserts the two things a mock cannot: that a live index is large enough to
/// be the whole of Aminet rather than an error page that happened to parse,
/// and that what `sync_catalog` wrote can be read back and searched.
#[test]
#[ignore = "leaves the machine: syncs the real Aminet index"]
fn the_real_index_syncs_into_a_catalogue_that_reads_back() {
    let scratch = ScratchDir::new("art-live", "aminet-sync");
    let catalogue = scratch.path().join("catalog.jsonl");
    let (store, _) = JsonlCatalogStore::load(&catalogue).expect("an absent catalogue is empty");

    let provider = aminet_defaults().expect("the shipped defaults must parse");
    let client = HttpMirrorClient::new();

    let outcome = sync_catalog(&provider, &client, &store, &NoProgress).expect("sync");
    println!(
        "  {} answered {} bytes: {} parsed, {} skipped",
        outcome.mirror, outcome.index_bytes, outcome.report.parsed, outcome.report.skipped
    );

    assert!(
        outcome.applied,
        "a complete parse must replace the catalogue"
    );
    assert!(
        outcome.report.parsed > 50_000,
        "the whole of Aminet, not a fragment: {} parsed",
        outcome.report.parsed
    );

    // Read it back through a second store, the way the next launch would.
    let (reloaded, report) = JsonlCatalogStore::load(&catalogue).expect("reload");
    assert_eq!(
        report.damaged, 0,
        "a catalogue ART just wrote must reload clean"
    );
    assert_eq!(
        report.loaded, outcome.report.parsed,
        "every entry written must come back"
    );

    let stats = reloaded.stats().expect("stats");
    println!("  reloaded {} entries", report.loaded);
    assert!(stats.total > 50_000);
}

/// **The button, done for real**: index → pick → download → gates → unpack.
///
/// This is the one the work list meant by *"nobody has clicked the button"*.
/// Everything a mock stands in for is real here — the mirror, the bytes, the
/// SHA-256, the size the index claimed, and an `.lha` written by somebody
/// thirty years ago rather than by a fixture.
#[test]
#[ignore = "leaves the machine: downloads one real package from Aminet"]
fn a_real_package_downloads_past_every_gate_and_unpacks() {
    let scratch = ScratchDir::new("art-live", "aminet-fetch");
    let provider = aminet_defaults().expect("defaults");
    let client = HttpMirrorClient::new();

    // ---- index ----
    let mut bytes = Vec::new();
    let url = provider.mirrors[0]
        .url_for(&provider.index_path)
        .expect("index url");
    client
        .fetch(&url, 0, &mut bytes, &NoProgress)
        .expect("the first shipped mirror must answer");
    let (entries, _) = parse_index_bytes(&bytes, "aminet");

    let picked = smallest_in_window(&entries)
        .expect("Aminet has small .lha packages")
        .clone();
    println!(
        "  picked {} ({} bytes) from {}",
        picked.reference.path, picked.size_bytes, picked.directory
    );

    // ---- download ----
    let cache = CacheLayout::new(scratch.path().join("cache"));
    let outcome = fetch_package(&picked, &provider.mirrors, &client, &cache, &NoProgress)
        .expect("a package the index lists must download and pass every gate");

    assert!(!outcome.from_cache, "an empty cache cannot answer");
    assert_eq!(outcome.sha256.len(), 64, "a SHA-256 is 64 hex characters");

    // The index's figure is **rounded**, which `check_size` is built around and
    // which the first live run confirmed: `DefectForm.lha` is listed as 4096
    // and is 4363 bytes. What is worth pinning is not the difference — the gate
    // already allowed it — but the **unit**. Aminet states these as `318K` and
    // `parse_size` multiplies by 1024, so in the K-stated window this check
    // deliberately downloads from, a claim is a whole number of KiB and the
    // real file is inside one of them. An index that moved to exact bytes, or
    // to powers of ten, would still parse and would silently shift every
    // package's claimed size; this is what would say so.
    let claimed = picked.size_bytes;
    println!(
        "  index claimed {claimed}, real {} (+{})",
        outcome.bytes,
        outcome.bytes as i64 - claimed as i64
    );
    assert_eq!(
        claimed % 1024,
        0,
        "a K-stated index entry parses to a whole number of KiB"
    );
    assert!(
        outcome.bytes.abs_diff(claimed) < 1024,
        "rounded to whole KiB means within one of them: claimed {claimed}, real {}",
        outcome.bytes
    );
    assert!(outcome.path.is_file(), "the object must be in the cache");
    println!("  {} bytes, sha256 {}", outcome.bytes, outcome.sha256);

    // ---- the same fetch again is answered by the cache, not the mirror ----
    let again = fetch_package(&picked, &provider.mirrors, &client, &cache, &NoProgress)
        .expect("a cached package must not need the network");
    assert!(again.from_cache, "content-addressed: the same bytes once");
    assert_eq!(again.sha256, outcome.sha256);

    // ---- unpack ----
    //
    // The second value is what was **skipped**, not what came out — an empty
    // one is the good answer, and reading it as a file list is how this check
    // first failed against an archive that had unpacked perfectly well.
    let (staged, skipped) = unpack_for_install(&outcome.path, scratch.path(), &NoProgress)
        .expect("a real .lha from Aminet must unpack");
    assert!(
        skipped.is_empty(),
        "a plain Aminet package should need nothing refused: {skipped:?}"
    );

    // So the file list is read off the disk, which is also where the install
    // step reads it. An archive ART reported as unpacked and left an empty
    // directory behind for is the failure this project keeps naming: the
    // confident sentence about work that did not happen.
    let root: &Path = staged.path();
    let files = files_under(root);
    println!("  unpacked {} files under {}", files.len(), root.display());
    for file in files.iter().take(5) {
        println!("    {}", file.strip_prefix(root).unwrap_or(file).display());
    }
    assert!(
        !files.is_empty(),
        "unpacking reported success and produced no files"
    );

    // None of them escaped the staging directory — `safe_join`'s guarantee,
    // measured against an archive nobody here wrote.
    for file in &files {
        assert!(
            file.starts_with(root),
            "{} escaped the staging directory",
            file.display()
        );
    }
}

/// Every file below `root`, depth first.
fn files_under(root: &Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}
