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
//! Two rules carried over unchanged from [`readers::drawer`](super::drawer),
//! the directory version of this same idea:
//!
//! - A slave's extension is `.slave` **or** `.Slave`, compared
//!   case-insensitively — the same `has_slave_extension` rule.
//! - A path with a `data`/`Data` component is payload, never a title — the
//!   same rule `scan::collect_drawers`'s directory walk follows and iGame's
//!   own `examineFolder` follows too.
//!
//! A third is new here rather than borrowed: a drawer inside the archive that
//! holds two slaves takes the same answer the directory reader gives —
//! refused rather than guessed — but there is no icon to consult without
//! decompressing it, and decompressing an icon to settle a case the material
//! does not contain is work for a case nobody has seen. So where the
//! directory reader can still ask `<dir>.info`'s `SLAVE=` ToolType, this
//! reader has nothing to ask and simply catalogues no title for that drawer.

use std::collections::HashMap;
use std::path::Path;

use crate::core::archive;
use crate::core::error::CoreResult;
use crate::core::gameindex::readers::slave;
use crate::core::gameindex::record::{
    derive_id, Fact, GameRecord, Media, Provenance, SourceRef, GAMEINDEX_SCHEMA,
};
use crate::core::hashing::sha256_bytes;

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

/// Catalogue every WHDLoad drawer inside the archive at `path`, without
/// unpacking anything.
///
/// Reads the archive's own entry list, keeps every `.slave`/`.Slave` member
/// whose parent path carries no `data`/`Data` component, and — for a parent
/// path named by exactly one such member — reads that member's bytes (bounded
/// by [`MAX_SLAVE_BYTES`]) and parses it with [`slave::read_slave`]. A parent
/// path named by more than one candidate is refused by omission: no title is
/// catalogued for it, the same answer the directory reader gives a drawer
/// with two slaves and nothing to settle which is the real one.
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

    let mut found = Vec::new();
    for (inner, candidates) in by_inner {
        let (index, slave_name) = match candidates.as_slice() {
            [(index, slave_name)] => (*index, *slave_name),
            many => {
                // Nothing here can ask an icon's `SLAVE=` ToolType without
                // decompressing it, so unlike `readers::drawer::resolve_ambiguous`
                // this has no way to settle it — the drawer is refused by
                // omission, and this is the only trace that it was ever seen.
                let names: Vec<&str> = many.iter().map(|(_, name)| *name).collect();
                log::debug!(
                    "gameindex: skipping {inner}: {} candidate slaves ({}) and nothing states \
                     which is the title",
                    many.len(),
                    names.join(", ")
                );
                continue;
            }
        };
        let slave_name = slave_name.to_string();

        // Bounded: a slave header is small and this archive is a file ART
        // did not write.
        let bytes = backend.read(index, MAX_SLAVE_BYTES)?;
        let facts = slave::read_slave(&bytes)?;

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

    /// Two slaves in the same drawer is not this reader's to settle — there
    /// is no icon to decompress and ask, unlike the directory reader. The
    /// drawer is refused by omission: neither candidate becomes a title, but
    /// every other drawer in the archive still does.
    #[test]
    fn a_drawer_with_two_slaves_yields_no_title_but_others_still_do() {
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
}
