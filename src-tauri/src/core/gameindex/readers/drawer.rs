//! A directory can be a title — the second shape a WHDLoad collection takes.
//!
//! **Measured shape**: 893 drawers in the owner's own collection, one slave
//! each, at a uniform depth, each alongside a `.info`, a `ReadMe` and a
//! payload that is a bare file, a `Disk.N` image, or a `data/` subdirectory.
//! iGame's own `examineFolder` skips directories named `data`/`Data` for the
//! same reason this one does: `Demos/T/Tag`'s payload is `data/01` …
//! `data/82`, and a scan that descends into it invents titles where there is
//! one.
//!
//! This reader answers one directory at a time and never recurses —
//! [`scan::collect_drawers`](crate::core::gameindex::scan::collect_drawers)
//! is the walk that decides *which* directories to ask.

use std::path::Path;

use crate::core::amigaicon;
use crate::core::error::{CoreError, CoreResult};
use crate::core::gameindex::readers::slave;
use crate::core::gameindex::record::{
    derive_id, Fact, GameRecord, Media, Provenance, SourceRef, GAMEINDEX_SCHEMA,
};
use crate::core::hashing::sha256_bytes;
use crate::core::whdload;

/// Whether `name`'s extension is `.slave`, compared case-insensitively — the
/// same way iGame's own `strcasestr` does it. A real drawer carries either
/// `.slave` or `.Slave`, and neither is more correct than the other.
fn has_slave_extension(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("slave"))
}

/// Every `#?.slave` a directory holds **itself** — never a subdirectory.
/// Sorted so a refusal listing several candidates names them in a stable
/// order.
fn slave_candidates(dir: &Path) -> CoreResult<Vec<String>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if has_slave_extension(name) {
            found.push(name.to_string());
        }
    }
    found.sort();
    Ok(found)
}

/// Whether `dir` is a WHDLoad drawer at all — the question the directory
/// walk asks of every folder it visits, without reading anything else about
/// it. Any reason `slave_candidates` cannot answer (the path is not a
/// directory, permissions, …) reads as "no", the same way a file no reader
/// understands is skipped rather than reported.
pub fn is_drawer(dir: &Path) -> bool {
    slave_candidates(dir)
        .map(|found| !found.is_empty())
        .unwrap_or(false)
}

fn dir_name_of(dir: &Path) -> String {
    dir.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string()
}

/// What `<dir-name>.info`'s `SLAVE=` ToolType names, when the icon exists,
/// parses, and says one.
///
/// Every other outcome — no icon, an icon ART cannot parse, an icon that
/// names nothing — comes back as `None`, because none of those settle an
/// ambiguous drawer either; only a stated name does.
fn icon_named_slave(dir: &Path, dir_name: &str) -> Option<String> {
    let bytes = std::fs::read(dir.join(format!("{dir_name}.info"))).ok()?;
    let tooltypes = amigaicon::tooltypes(&bytes).ok()?;
    whdload::launch_options(&tooltypes).slave
}

/// Settle which of several slaves is the title's.
///
/// Two slaves is not ART's to choose between — the one case that is
/// answerable is the icon itself stating which, through its `SLAVE=`
/// ToolType. Anything else is refused **by name**: the drawer and every
/// candidate, so the refusal is something a person can act on rather than a
/// bare "ambiguous".
fn resolve_ambiguous(dir: &Path, candidates: &[String]) -> CoreResult<String> {
    let dir_name = dir_name_of(dir);
    if let Some(named) = icon_named_slave(dir, &dir_name) {
        if let Some(settled) = candidates
            .iter()
            .find(|candidate| candidate.eq_ignore_ascii_case(&named))
        {
            return Ok(settled.clone());
        }
    }
    Err(CoreError::InvalidInput(format!(
        "'{dir_name}' holds {} candidate slaves ({}) and nothing states which one is the \
         title — only the icon's SLAVE= ToolType can settle that",
        candidates.len(),
        candidates.join(", "),
    )))
}

/// Read a directory as an unpacked WHDLoad drawer.
///
/// `Ok(None)` — not an error — for a directory holding no slave at all: most
/// of what sits beside a real collection's 893 drawers is not a title, and a
/// folder with no slave in it is not a defect.
pub fn read_drawer(dir: &Path) -> CoreResult<Option<GameRecord>> {
    let candidates = slave_candidates(dir)?;
    let slave_name = match candidates.as_slice() {
        [] => return Ok(None),
        [only] => only.clone(),
        many => resolve_ambiguous(dir, many)?,
    };

    let bytes = std::fs::read(dir.join(&slave_name))?;
    let facts = slave::read_slave(&bytes)?;

    let title = match facts.name.clone().filter(|name| !name.is_empty()) {
        Some(name) => Fact::new(name, Provenance::WhdloadSlave),
        // The header states no name: the directory's own is a suggestion,
        // never a declaration — the same distinction `readers::whdhdf` keeps
        // between a slave's stated name and the drawer it happens to sit in.
        None => Fact::new(dir_name_of(dir), Provenance::DrawerName),
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

    Ok(Some(GameRecord {
        schema: GAMEINDEX_SCHEMA,
        id: derive_id(&title.value, &sha256),
        title,
        // A WHDLoad drawer holds an installed program; nothing in it says
        // whether that is a game or a demo, and §14/§34 forbid guessing.
        kind: None,
        year: stated_year.map(|y| Fact::new(y, Provenance::WhdloadSlave)),
        publisher: stated_publisher.map(|p| Fact::new(p, Provenance::WhdloadSlave)),
        genre: None,
        rating: None,
        chipset: slave::chipset_of(&facts).map(|c| Fact::new(c, Provenance::WhdloadSlave)),
        kickstart: (!facts.kickstart.is_empty())
            .then(|| Fact::new(facts.kickstart.clone(), Provenance::WhdloadSlave)),
        media: Media::WhdloadDrawer {
            dir: dir.to_string_lossy().into_owned(),
            slave: slave_name,
        },
        preview: None,
        source,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gameindex::readers::slave::tests_support::build_slave;
    use crate::core::gameindex::scan::collect_drawers;
    use std::path::PathBuf;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-drawer-{tag}-{}",
            crate::core::test_scratch_id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A minimal Amiga icon whose ToolTypes are `["SLAVE=<slave>",
    /// "PRELOAD"]` — the same `DiskObject` shape `core::amigaicon`'s own
    /// tests build by hand (that helper is private to its module, so this is
    /// a second, smaller instance of the same layout rather than a second
    /// format): `do_Magic` at offset 0, a fixed 78-byte header, then a
    /// `ToolTypes` block whose `u32` size is `(count + 1) * 4` followed by
    /// that many length-prefixed strings.
    fn icon_naming(slave: &str) -> Vec<u8> {
        const MAGIC: u16 = 0xE310;
        const HEADER_LEN: usize = 78;
        const OFF_TOOL_TYPES: usize = 54;

        let tooltypes = [format!("SLAVE={slave}"), "PRELOAD".to_string()];

        let mut buf = vec![0u8; HEADER_LEN];
        buf[0..2].copy_from_slice(&MAGIC.to_be_bytes());
        buf[2..4].copy_from_slice(&1u16.to_be_bytes()); // do_Version
        buf[OFF_TOOL_TYPES..OFF_TOOL_TYPES + 4].copy_from_slice(&1u32.to_be_bytes());

        let size = ((tooltypes.len() + 1) * 4) as u32;
        buf.extend_from_slice(&size.to_be_bytes());
        for tt in &tooltypes {
            let bytes = tt.as_bytes();
            buf.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
            buf.extend_from_slice(bytes);
        }
        buf
    }

    /// A synthetic drawer: one slave, an icon naming it, a ReadMe, a payload.
    fn synthetic_drawer(root: &Path, name: &'static str, slave_file: &str) -> PathBuf {
        let dir = root.join(name);
        std::fs::create_dir_all(dir.join("data")).unwrap();
        std::fs::write(dir.join(slave_file), build_slave(name, "1992 Someone", 16)).unwrap();
        std::fs::write(dir.join(format!("{name}.info")), icon_naming(slave_file)).unwrap();
        std::fs::write(dir.join("ReadMe"), b"notes").unwrap();
        std::fs::write(dir.join("data").join("01"), b"payload").unwrap();
        dir
    }

    #[test]
    fn a_directory_holding_one_slave_is_a_title() {
        let root = scratch("one");
        let dir = synthetic_drawer(&root, "Turrican", "Turrican.slave");
        let record = read_drawer(&dir).unwrap().expect("this is a title");
        match record.media {
            Media::WhdloadDrawer { dir: d, slave } => {
                assert!(d.ends_with("Turrican"));
                assert_eq!(slave, "Turrican.slave");
            }
            other => panic!("a drawer is a drawer, got {other:?}"),
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_slave_is_found_whatever_the_extensions_case() {
        let root = scratch("case");
        let dir = synthetic_drawer(&root, "Tag", "Tag.Slave");
        let record = read_drawer(&dir).unwrap().expect("`.Slave` is a slave");
        assert!(matches!(record.media, Media::WhdloadDrawer { .. }));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_payload_directory_is_not_a_title() {
        // `Demos/T/Tag/data/01`…`data/82` is payload. iGame skips
        // `data`/`Data` for the same reason, and a scan that descends into
        // it invents titles where there is one.
        let root = scratch("payload");
        let dir = synthetic_drawer(&root, "Tag", "Tag.Slave");
        // The real payload's numbered files (`data/01`…`data/82`) never look
        // like a drawer themselves, so a fixture with only those does not
        // actually exercise the skip — descending into it or not finds the
        // same one title either way. A spurious slave-bearing subdirectory
        // inside `data/` is what a walk that fails to skip it would
        // mistakenly catalogue as a second title.
        std::fs::create_dir_all(dir.join("data").join("Extra")).unwrap();
        std::fs::write(
            dir.join("data").join("Extra").join("Extra.slave"),
            build_slave("Extra", "1992 Someone", 16),
        )
        .unwrap();
        let found = collect_drawers(&root);
        assert_eq!(found.len(), 1, "one title, not one per payload directory");
        assert!(found[0].ends_with("Tag"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_drawer_with_two_slaves_is_refused_by_name() {
        let root = scratch("two");
        let dir = synthetic_drawer(&root, "Ambiguous", "One.slave");
        // `synthetic_drawer`'s own icon names the one slave it was built with
        // ("One.slave"), which is one of the two candidates below — so left
        // in place it would *settle* the ambiguity by coincidence, rather
        // than test the case where nothing does. Removing it is what makes
        // this the no-signal case `the_icons_slave_tooltype_settles_a_drawer_
        // that_has_two` is the other half of.
        std::fs::remove_file(dir.join("Ambiguous.info")).unwrap();
        std::fs::write(
            dir.join("Two.slave"),
            build_slave("Two", "1992 Someone", 16),
        )
        .unwrap();
        let err = read_drawer(&dir).expect_err("two slaves is not ART's to choose between");
        let text = err.to_string();
        assert!(
            text.contains("Ambiguous") && text.contains("One.slave") && text.contains("Two.slave"),
            "the refusal names the drawer and both candidates, got: {text}"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_icons_slave_tooltype_settles_a_drawer_that_has_two() {
        // The one case where two slaves is answerable: the icon says which.
        let root = scratch("two-icon");
        let dir = synthetic_drawer(&root, "Decided", "One.slave");
        std::fs::write(
            dir.join("Two.slave"),
            build_slave("Two", "1992 Someone", 16),
        )
        .unwrap();
        std::fs::write(dir.join("Decided.info"), icon_naming("Two.slave")).unwrap();
        let record = read_drawer(&dir).unwrap().expect("the icon states it");
        match record.media {
            Media::WhdloadDrawer { slave, .. } => assert_eq!(slave, "Two.slave"),
            other => panic!("got {other:?}"),
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_directory_with_no_slave_is_not_a_title() {
        let root = scratch("none");
        std::fs::create_dir_all(root.join("Docs")).unwrap();
        std::fs::write(root.join("Docs").join("ReadMe"), b"x").unwrap();
        assert!(read_drawer(&root.join("Docs")).unwrap().is_none());
        std::fs::remove_dir_all(&root).ok();
    }
}
