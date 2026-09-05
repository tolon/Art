//! A drawer inside an archive ART has not unpacked — the third shape a
//! WHDLoad collection takes.
//!
//! **Measured shape**: the owner's own `WHDLoadDemos100.lha` is 663 MB, 8858
//! entries, 893 slaves in 893 drawers. LhA headers are sequential and each
//! carries its own packed size, so [`core::archive::open`](crate::core::archive::open)'s
//! `entries()` walk seeks header to header without decompressing a single
//! payload byte. Only the `.slave` members — about a kilobyte each — are ever
//! decompressed, which is what keeps this cheap on the whole 663 MB rather
//! than on a handful of test fixtures. **Never decompress an entry that is not
//! a slave candidate**; a change that reads every member to find them would
//! pass every test here and be unusable on the real archive.
//!
//! **Log-and-continue per winner, the same as every other reader (I1).** A
//! truncated, oversized, or simply-not-a-slave member used to fail the whole
//! archive's scan with `?` — one junk `.slave` in an 8858-entry archive
//! reported as one error instead of the other 892 titles it sat beside. The
//! owner's own 893 slaves are uniformly clean, which is exactly why neither
//! the synthetic fixtures nor the real-material run ever surfaced this: a
//! behaviour tuned to one collection's shape rather than measured against
//! what somebody else's archive can contain.
//!
//! Two rules carried over unchanged from [`readers::drawer`](super::drawer),
//! the directory version of this same idea:
//!
//! - A slave's extension is `.slave` **or** `.Slave`, compared
//!   case-insensitively — the same `has_slave_extension` rule.
//! - A path with a `data`/`Data` component is payload, never a title — the
//!   same rule `scan::collect_drawers`'s directory walk follows and iGame's
//!   own `examineFolder` follows too.
//!
//! A third is carried over with a difference rather than borrowed unchanged:
//! a drawer inside the archive that holds two slaves is settled the same way
//! the directory reader settles it — its icon's `SLAVE=` ToolType, when the
//! icon exists and names one of the candidates — because the collection this
//! shape was measured against is evidence, never the specification, and a
//! user who unpacks an archive must not watch titles appear that the
//! archived scan silently dropped. The difference is *how* the icon is
//! reached: the directory reader opens `<dir>.info` off the filesystem for
//! every drawer it visits; this reader only ever decompresses one — the
//! `.info` named for the drawer, inside it, same rule
//! [`super::drawer::read_drawer`] applies — and only when a drawer is
//! *already* ambiguous, which is rare. Nothing changes for the common path
//! of one slave, one drawer. A drawer with no icon, or an icon naming
//! neither candidate, still gets no title, the same as before.
//!
//! **N1.** `<dir-name>.info` is the *only* icon either reader ever consults —
//! measured convention (every drawer and its Project icon share a name in the
//! 893 this was built against), not an AmigaDOS rule. A drawer named
//! `Turrican 3` whose own icon happens to be called `Turrican3.info` is
//! refused rather than guessed at, which is the right call (§14/§34) — but
//! the *rule* itself is parochial, and a future reader that wants to look
//! further should start here rather than assume the name always matches.

use std::collections::HashMap;
use std::path::Path;

use crate::core::amigaicon;
use crate::core::archive;
use crate::core::error::CoreResult;
use crate::core::gameindex::readers::slave;
use crate::core::gameindex::record::{
    derive_id, Fact, GameRecord, Media, Provenance, SourceRef, GAMEINDEX_SCHEMA,
};
use crate::core::hashing::sha256_bytes;
use crate::core::whdload;

/// The largest a slave member may decompress to before ART gives up on it.
///
/// Mirrors `readers::whdhdf::MAX_SLAVE_BYTES`: a slave is kilobytes, and a
/// header claiming more is not a slave — it is a claim from an archive ART
/// did not write, and `read`'s own bound is what keeps that claim from ever
/// being honoured.
const MAX_SLAVE_BYTES: u64 = 2 * 1024 * 1024;

/// Whether `name`'s extension is `.slave`, compared case-insensitively — the
/// same rule [`super::drawer::has_slave_extension`] applies to an unpacked
/// one.
fn has_slave_extension(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("slave"))
}

/// Whether `path` (a `/`-separated archive path) has a `data`/`Data`
/// component anywhere in it. Same rule as `scan::collect_drawers`'s
/// `is_payload_name`, applied to a string instead of a filesystem entry
/// because an archive has no directories to walk — only names.
fn has_payload_component(path: &str) -> bool {
    path.split('/')
        .any(|part| part.eq_ignore_ascii_case("data"))
}

/// `"Demos/T/Tag/Tag.Slave"` → `("Demos/T/Tag", "Tag.Slave")`. A name with no
/// `/` at all — a slave sitting at the archive root — has an empty drawer.
fn split_inner(path: &str) -> (&str, &str) {
    match path.rsplit_once('/') {
        Some((dir, name)) => (dir, name),
        None => ("", path),
    }
}

/// The last path component, for the fallback title when a slave states none —
/// the same fallback [`super::drawer::read_drawer`] uses for an unpacked one.
fn drawer_name_of(inner: &str) -> String {
    inner.rsplit('/').next().unwrap_or(inner).to_string()
}

/// `<drawer>.info`'s archive path, the same name [`super::drawer::read_drawer`]
/// looks for on the filesystem (`dir.join(format!("{dir_name}.info"))`) —
/// *inside* the drawer, named for the drawer, never a fixed `Icon.info` or
/// similar.
fn icon_entry_name(inner: &str, dir_name: &str) -> String {
    if inner.is_empty() {
        format!("{dir_name}.info")
    } else {
        format!("{inner}/{dir_name}.info")
    }
}

/// Settle which of several slave candidates is the drawer's title, by
/// consulting its icon — the one case that is answerable, the same as
/// [`super::drawer::resolve_ambiguous`]'s own reasoning for an unpacked
/// drawer. `Ok(None)` for every outcome that settles nothing: no icon in the
/// archive, an icon ART cannot parse, or a `SLAVE=` naming neither candidate.
/// Called only once a drawer is already known to hold more than one slave, so
/// an icon is decompressed only for the rare ambiguous case — never on the
/// 893-drawer common path.
fn settle_by_icon<'e>(
    backend: &mut dyn archive::ArchiveBackend,
    entries: &[archive::ArchiveEntry],
    inner: &str,
    candidates: &[(usize, &'e str)],
) -> CoreResult<Option<(usize, &'e str)>> {
    let dir_name = drawer_name_of(inner);
    let icon_name = icon_entry_name(inner, &dir_name);
    let Some(icon_index) = entries
        .iter()
        .position(|entry| !entry.is_dir && entry.name == icon_name)
    else {
        return Ok(None);
    };

    // Bounded, the same as a slave's own read: an icon is small, and this
    // archive is a file ART did not write.
    let bytes = backend.read(icon_index, MAX_SLAVE_BYTES)?;
    let Ok(tooltypes) = amigaicon::tooltypes(&bytes) else {
        return Ok(None);
    };
    let Some(named) = whdload::launch_options(&tooltypes).slave else {
        return Ok(None);
    };

    Ok(candidates
        .iter()
        .find(|(_, name)| name.eq_ignore_ascii_case(&named))
        .copied())
}

/// Catalogue every WHDLoad drawer inside the archive at `path`, without
/// unpacking anything.
///
/// Reads the archive's own entry list, keeps every `.slave`/`.Slave` member
/// whose parent path carries no `data`/`Data` component, and — for a parent
/// path named by exactly one such member — reads that member's bytes (bounded
/// by [`MAX_SLAVE_BYTES`]) and parses it with [`slave::read_slave`]. A parent
/// path named by more than one candidate is settled by [`settle_by_icon`]
/// when the drawer's icon names one of them; otherwise it is refused by
/// omission — no title is catalogued for it, and a debug line names the
/// drawer and its candidates.
pub fn read_archive_drawers(path: &Path) -> CoreResult<Vec<GameRecord>> {
    let mut backend = archive::open(path)?;
    let entries = backend.entries()?;
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());

    // Group every slave candidate by the drawer it sits in, so a drawer
    // holding two is spotted before either is read — reading one and then
    // discovering its sibling would mean decompressing a member this reader
    // is about to throw away.
    let mut by_inner: HashMap<&str, Vec<(usize, &str)>> = HashMap::new();
    for (index, entry) in entries.iter().enumerate() {
        if entry.is_dir || !has_slave_extension(&entry.name) {
            continue;
        }
        let (inner, slave_name) = split_inner(&entry.name);
        if has_payload_component(inner) {
            continue;
        }
        by_inner.entry(inner).or_default().push((index, slave_name));
    }

    // Settle which slave wins for every drawer first, without reading any of
    // them yet. `by_inner` is a `HashMap`, so this order says nothing about
    // where each winner actually sits in the archive.
    let mut winners: Vec<(usize, &str, String)> = Vec::new();
    for (inner, candidates) in by_inner {
        let (index, slave_name) = match candidates.as_slice() {
            [(index, slave_name)] => (*index, *slave_name),
            many => match settle_by_icon(backend.as_mut(), &entries, inner, many) {
                Ok(Some(settled)) => settled,
                Ok(None) => {
                    let names: Vec<&str> = many.iter().map(|(_, name)| *name).collect();
                    log::debug!(
                        "gameindex: skipping {inner}: {} candidate slaves ({}) and nothing \
                         states which is the title",
                        many.len(),
                        names.join(", ")
                    );
                    continue;
                }
                // A corrupt or oversized icon is a fact about *this* drawer,
                // not about the archive: the icon that would have settled it
                // could not be read, so the drawer is refused the same way as
                // "nothing states which one" — the other 892 drawers are not
                // this one's problem (I1).
                Err(err) => {
                    log::debug!(
                        "gameindex: skipping {inner}: could not read its icon to settle \
                         which of {} slaves is the title: {err}",
                        many.len()
                    );
                    continue;
                }
            },
        };
        winners.push((index, inner, slave_name.to_string()));
    }

    // `LhaBackend::seek_to` only avoids reopening and re-walking the archive
    // from the start when reads come in ascending entry index — its own doc
    // comment says so. `by_inner`'s hash order does not honour that, so the
    // actual slave reads are sorted by index here, before a single one runs.
    winners.sort_by_key(|(index, _, _)| *index);

    let mut found = Vec::new();
    for (index, inner, slave_name) in winners {
        // Bounded: a slave header is small and this archive is a file ART
        // did not write.
        //
        // **I1.** Both of these used to propagate with `?`, so one truncated,
        // oversized or simply-not-a-slave member took the whole archive's
        // scan down with it — 892 good titles reported as one error. Every
        // other reader in this module is log-and-continue per item
        // (`scan::scan_titles_with`'s own comment: "one unreadable file must
        // not lose the other 1696"); a winning candidate that turns out to be
        // junk gets exactly that same treatment, never the whole function's.
        let bytes = match backend.read(index, MAX_SLAVE_BYTES) {
            Ok(bytes) => bytes,
            Err(err) => {
                log::debug!("gameindex: skipping {inner}/{slave_name}: {err}");
                continue;
            }
        };
        let facts = match slave::read_slave(&bytes) {
            Ok(facts) => facts,
            Err(err) => {
                log::debug!("gameindex: skipping {inner}/{slave_name}: {err}");
                continue;
            }
        };

        let title = match facts.name.clone().filter(|name| !name.is_empty()) {
            Some(name) => Fact::new(name, Provenance::WhdloadSlave),
            // The header states no name: the drawer's own name inside the
            // archive is a suggestion, never a declaration.
            None => Fact::new(drawer_name_of(inner), Provenance::DrawerName),
        };

        let (stated_year, stated_publisher) = facts
            .copyright
            .as_deref()
            .map(slave::split_copyright)
            .unwrap_or((None, None));

        let sha256 = sha256_bytes(&bytes);
        let source = SourceRef {
            name: slave_name.clone(),
            sha256: sha256.clone(),
            bytes: bytes.len() as u64,
        };

        found.push(GameRecord {
            schema: GAMEINDEX_SCHEMA,
            id: derive_id(&title.value, &sha256),
            title,
            // A slave inside an archive holds an installed program; nothing
            // in it says whether that is a game or a demo, and §14/§34
            // forbid guessing — same as the unpacked drawer's own record.
            kind: None,
            year: stated_year.map(|y| Fact::new(y, Provenance::WhdloadSlave)),
            publisher: stated_publisher.map(|p| Fact::new(p, Provenance::WhdloadSlave)),
            genre: None,
            rating: None,
            chipset: slave::chipset_of(&facts).map(|c| Fact::new(c, Provenance::WhdloadSlave)),
            kickstart: (!facts.kickstart.is_empty())
                .then(|| Fact::new(facts.kickstart.clone(), Provenance::WhdloadSlave)),
            media: Media::WhdloadArchive {
                file: file_name.clone(),
                inner: inner.to_string(),
                slave: slave_name,
            },
            preview: None,
            source,
        });
    }

    // `by_inner` is a `HashMap`, so the order above is not deterministic —
    // sorted here the same way `scan::scan_titles_with`'s own final sort is,
    // so a rescan of an untouched archive lists the same 893 titles in the
    // same order rather than a diff nobody can read.
    found.sort_by(|a, b| {
        a.title
            .value
            .to_lowercase()
            .cmp(&b.title.value.to_lowercase())
    });

    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::amigaicon::tests_support::synthetic_icon;
    use crate::core::gameindex::readers::slave::tests_support::build_slave;
    use std::path::PathBuf;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-lhadrawer-{tag}-{}",
            crate::core::test_scratch_id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The common case: a valid slave stating a name and a copyright, at a
    /// fixed version — the exact shape `readers::drawer`'s own tests build,
    /// so the two readers are exercised against the same fixture shape.
    fn slave_bytes(name: &'static str) -> Vec<u8> {
        build_slave(name, "1992 Someone", 16)
    }

    /// Build a real `.lha` at runtime holding `files`, through `core::lha`'s
    /// own test writer — never a second archive writer, and never a packer
    /// shelled out to.
    fn synthetic_lha(root: &Path, files: &[(&str, Vec<u8>)]) -> PathBuf {
        let raw: Vec<(&str, &[u8])> = files.iter().map(|(n, c)| (*n, c.as_slice())).collect();
        let archive = root.join("test.lha");
        std::fs::write(&archive, crate::core::lha::tests::make_lha_with(&raw)).unwrap();
        archive
    }

    #[test]
    fn every_drawer_in_an_archive_becomes_a_title() {
        let root = scratch("every-drawer");
        let archive = synthetic_lha(
            &root,
            &[
                ("Demos/0-9/One/One.slave", slave_bytes("One")),
                ("Demos/0-9/One/data/01", b"payload".to_vec()),
                ("Demos/T/Tag/Tag.Slave", slave_bytes("Tag")),
                ("Demos/T/Tag/ReadMe", b"notes".to_vec()),
            ],
        );
        let found = read_archive_drawers(&archive).unwrap();
        assert_eq!(found.len(), 2);
        let inners: Vec<String> = found
            .iter()
            .map(|r| match &r.media {
                Media::WhdloadArchive { inner, .. } => inner.clone(),
                other => panic!("an archived drawer is WhdloadArchive, got {other:?}"),
            })
            .collect();
        assert!(inners.contains(&"Demos/0-9/One".to_string()));
        assert!(inners.contains(&"Demos/T/Tag".to_string()));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_archived_title_records_the_archive_it_came_from() {
        let root = scratch("names");
        let archive = synthetic_lha(&root, &[("D/Tag/Tag.Slave", slave_bytes("Tag"))]);
        let found = read_archive_drawers(&archive).unwrap();
        match &found[0].media {
            Media::WhdloadArchive { file, inner, slave } => {
                assert!(file.ends_with(".lha"));
                assert_eq!(inner, "D/Tag");
                assert_eq!(slave, "Tag.Slave");
            }
            other => panic!("got {other:?}"),
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_payload_directory_inside_an_archive_is_not_a_title() {
        let root = scratch("payload");
        let archive = synthetic_lha(
            &root,
            &[
                ("D/Tag/Tag.Slave", slave_bytes("Tag")),
                ("D/Tag/data/01", b"payload".to_vec()),
                ("D/Tag/data/02", b"payload".to_vec()),
            ],
        );
        assert_eq!(read_archive_drawers(&archive).unwrap().len(), 1);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_archive_with_no_slave_yields_no_titles() {
        let root = scratch("empty");
        let archive = synthetic_lha(&root, &[("Docs/ReadMe", b"x".to_vec())]);
        assert!(read_archive_drawers(&archive).unwrap().is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    /// A slave sitting directly inside a directory named `data` is payload,
    /// not a title — not just the sibling files beside a real slave. Without
    /// this, `a_payload_directory_inside_an_archive_is_not_a_title` alone
    /// cannot tell "the skip works" from "the payload files never looked
    /// like slaves in the first place", the same gap `readers::drawer`'s own
    /// `a_payload_directory_is_not_a_title` comment calls out.
    #[test]
    fn a_slave_sitting_inside_a_data_directory_is_not_a_title() {
        let root = scratch("data-slave");
        let archive = synthetic_lha(
            &root,
            &[
                ("D/Tag/Tag.Slave", slave_bytes("Tag")),
                ("D/Tag/data/Extra/Extra.slave", slave_bytes("Extra")),
            ],
        );
        let found = read_archive_drawers(&archive).unwrap();
        assert_eq!(
            found.len(),
            1,
            "the data-nested slave is payload, not a title"
        );
        match &found[0].media {
            Media::WhdloadArchive { inner, .. } => assert_eq!(inner, "D/Tag"),
            other => panic!("got {other:?}"),
        }
        std::fs::remove_dir_all(&root).ok();
    }

    /// **I1's own fix.** One drawer's `.slave` is junk — not a slave header at
    /// all — sitting beside a perfectly good one. Before this fix,
    /// `slave::read_slave`'s `?` took the whole function down with it: this
    /// asserts the good drawer still comes back rather than the call
    /// returning `Err` for the archive as a whole.
    #[test]
    fn one_junk_slave_does_not_lose_the_others_in_the_archive() {
        let root = scratch("one-junk");
        let archive = synthetic_lha(
            &root,
            &[
                ("D/Good/Good.slave", slave_bytes("Good")),
                ("D/Junk/Junk.slave", b"not a slave header at all".to_vec()),
                ("D/AlsoGood/AlsoGood.slave", slave_bytes("AlsoGood")),
            ],
        );
        let found = read_archive_drawers(&archive).unwrap();
        let titles: Vec<&str> = found.iter().map(|r| r.title.value.as_str()).collect();
        assert_eq!(
            titles,
            vec!["AlsoGood", "Good"],
            "the junk slave is skipped; the other two drawers still catalogue: {found:?}"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// The oversized half of the same fix: a slave whose header claims (or
    /// whose real bytes are) larger than `MAX_SLAVE_BYTES` is skipped the
    /// same way, not propagated as an archive-wide error.
    #[test]
    fn an_oversized_slave_does_not_lose_the_others_in_the_archive() {
        let root = scratch("one-oversized");
        let archive = synthetic_lha(
            &root,
            &[
                ("D/Good/Good.slave", slave_bytes("Good")),
                (
                    "D/Huge/Huge.slave",
                    vec![0u8; (MAX_SLAVE_BYTES + 1) as usize],
                ),
            ],
        );
        let found = read_archive_drawers(&archive).unwrap();
        let titles: Vec<&str> = found.iter().map(|r| r.title.value.as_str()).collect();
        assert_eq!(
            titles,
            vec!["Good"],
            "the oversized slave is skipped, not an error for the whole archive: {found:?}"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// Two slaves and **no icon in the archive at all**: nothing can settle
    /// which is the title, so the drawer is refused by omission — neither
    /// candidate becomes a title, but every other drawer in the archive
    /// still does.
    #[test]
    fn a_drawer_with_two_slaves_and_no_icon_yields_no_title_but_others_still_do() {
        let root = scratch("ambiguous");
        let archive = synthetic_lha(
            &root,
            &[
                ("D/Ambiguous/One.slave", slave_bytes("One")),
                ("D/Ambiguous/Two.slave", slave_bytes("Two")),
                ("D/Clean/Clean.slave", slave_bytes("Clean")),
            ],
        );
        let found = read_archive_drawers(&archive).unwrap();
        assert_eq!(found.len(), 1, "the ambiguous drawer yields no title");
        match &found[0].media {
            Media::WhdloadArchive { inner, .. } => assert_eq!(inner, "D/Clean"),
            other => panic!("got {other:?}"),
        }
        std::fs::remove_dir_all(&root).ok();
    }

    /// Two slaves, but the drawer's own icon states which — the one case
    /// that is answerable, the same reasoning `readers::drawer::resolve_ambiguous`
    /// already uses for an unpacked drawer. The icon is `Ambiguous.info`
    /// *inside* `D/Ambiguous/` (named for the drawer, per `icon_entry_name`),
    /// the same place `dir.join(format!("{dir_name}.info"))` looks on disk.
    ///
    /// A third, unrelated drawer with its own single slave proves the
    /// resolution did not accidentally consume or disturb it.
    #[test]
    fn a_drawer_with_two_slaves_is_settled_by_its_icons_slave_tooltype() {
        let root = scratch("icon-settles");
        let icon = synthetic_icon(&["SLAVE=Two.slave", "PRELOAD"], 0, b"");
        let archive = synthetic_lha(
            &root,
            &[
                ("D/Ambiguous/One.slave", slave_bytes("One")),
                ("D/Ambiguous/Two.slave", slave_bytes("Two")),
                ("D/Ambiguous/Ambiguous.info", icon),
                ("D/Clean/Clean.slave", slave_bytes("Clean")),
            ],
        );
        let found = read_archive_drawers(&archive).unwrap();
        assert_eq!(
            found.len(),
            2,
            "the icon settles the ambiguous drawer, and the clean one still stands: {found:?}"
        );
        let by_inner: HashMap<&str, &str> = found
            .iter()
            .map(|r| match &r.media {
                Media::WhdloadArchive { inner, slave, .. } => (inner.as_str(), slave.as_str()),
                other => panic!("got {other:?}"),
            })
            .collect();
        assert_eq!(by_inner.get("D/Ambiguous"), Some(&"Two.slave"));
        assert_eq!(by_inner.get("D/Clean"), Some(&"Clean.slave"));
        std::fs::remove_dir_all(&root).ok();
    }

    /// The grouping is a `HashMap`, whose own iteration order is not the
    /// point of anything here — but the result handed back **is** a list a
    /// user reads, so it must come back in the same order every time rather
    /// than shuffling with every rescan of an untouched archive. Titles
    /// chosen so their names sort differently than the archive's own entry
    /// order or a hash of their drawer paths would.
    #[test]
    fn the_result_is_sorted_by_title_not_left_to_hash_order() {
        let root = scratch("sorted");
        let archive = synthetic_lha(
            &root,
            &[
                ("Z/Zorro/Zorro.slave", slave_bytes("Zorro")),
                ("A/Alpha/Alpha.slave", slave_bytes("Alpha")),
                ("M/Mid/Mid.slave", slave_bytes("Mid")),
            ],
        );
        let found = read_archive_drawers(&archive).unwrap();
        let titles: Vec<&str> = found.iter().map(|r| r.title.value.as_str()).collect();
        assert_eq!(
            titles,
            vec!["Alpha", "Mid", "Zorro"],
            "a catalogue that lists the same titles in a different order each \
             scan is a diff nobody can read"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// The claim this file's own doc comment makes — that the scan seeks
    /// header to header and decompresses only slave candidates, never the
    /// whole archive — is only provable by timing it against the real
    /// archive. Every other test in this module packs a handful of entries;
    /// this is the one against the owner's own 663 MB, 8 858-entry `.lha`,
    /// never committed and never touched by the ordinary suite.
    ///
    /// ```text
    /// ART_LHA_ARCHIVE="E:\amiga\Amigatolon\paketler\WHDLoadDemos100.lha" \
    ///   cargo test --lib real_archive_scan_is_fast -- --ignored --nocapture
    /// ```
    ///
    /// Prints the count and the wall-clock time rather than asserting either:
    /// the count is a property of somebody's archive, not of this code, and a
    /// mismatch against the 893 measured elsewhere is a finding to report,
    /// not a regression to chase. The timing is the point — if this takes
    /// minutes rather than seconds, something is decompressing far more than
    /// the slave candidates it needs to, and a synthetic fixture could never
    /// surface that.
    #[test]
    #[ignore = "needs the owner's own archive; set ART_LHA_ARCHIVE"]
    fn real_archive_scan_is_fast() {
        use std::time::Instant;

        let Ok(path) = std::env::var("ART_LHA_ARCHIVE") else {
            eprintln!("ART_LHA_ARCHIVE is not set");
            return;
        };

        let started = Instant::now();
        let found = read_archive_drawers(Path::new(&path)).unwrap();
        let elapsed = started.elapsed();
        println!(
            "ART_LHA_RESULT drawers={} elapsed_ms={}",
            found.len(),
            elapsed.as_millis()
        );
    }
}
