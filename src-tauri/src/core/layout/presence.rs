//! Is this already there, and is it *ours*? (ART-177)
//!
//! `apply` refuses to overwrite, which is right (§93) and which made a
//! half-finished run a dead end: every file the interrupted run had already
//! placed came back in the next preview as an ordinary collision, with nothing
//! saying it was the wreckage of that run, and the only way forward was the
//! file manager.
//!
//! The owner's answer is that a destination already holding **exactly what
//! this plan would put there** is skipped, and the preview says how many. Re-
//! running a half-finished apply then resumes by itself, with no "continue"
//! button and no new mode.
//!
//! ## The rule this module has to keep
//!
//! **When ART cannot be sure, the answer is [`Presence::Different`].** A false
//! `AlreadyInPlace` means a file the user asked for is silently not copied; a
//! false `Different` means a collision they have to look at. The first is data
//! loss dressed as success, the second is a nuisance. Every branch below
//! therefore ends in `Different` unless it has positively established
//! sameness.
//!
//! ## What "the same" means, per placement
//!
//! - [`Placement::CopyFile`] — same length, then **byte for byte**. Read in
//!   chunks, so a 700 MB HDF costs a streaming compare and not two copies in
//!   memory.
//! - [`Placement::CopyTree`] — the same relative paths on both sides, no
//!   extras either way, and every file compared as above.
//! - [`Placement::UnpackWhdload`] — compared against the archive's **entry
//!   list**, without decompressing anything: every entry inside the pack has
//!   a file at the matching relative path whose length equals the entry's
//!   declared uncompressed size, and the destination holds nothing else.
//!
//!   A declared size is an adversarial claim everywhere else in ART, and it
//!   is one here too — the difference is which way a lie pushes the answer. A
//!   lie that makes the check *fail* costs a collision report. A lie that
//!   makes it *pass* causes ART to write nothing at all, leaving the files
//!   already on disk untouched. Neither outcome writes attacker-chosen bytes
//!   anywhere, which is why the cheap check is the right one here and the
//!   wrong one at the extraction gate.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::error::CoreResult;
use crate::core::layout::{LayoutItem, Placement};
use crate::core::security::path::safe_join;

/// How much of a file is compared per read.
const CHUNK: usize = 64 * 1024;

/// The most entries a tree comparison will walk before giving up and saying
/// `Different`. A drawer past this is not something to decide silently.
const MAX_TREE_ENTRIES: usize = 100_000;

/// The deepest a tree comparison goes, matching `scan::MAX_SCAN_DEPTH`'s role.
const MAX_DEPTH: usize = 32;

/// What the staging tree already holds at an item's destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Presence {
    /// Nothing there. The ordinary case: the item is new.
    Absent,
    /// Exactly what this item would place. Skipped, and counted.
    AlreadyInPlace,
    /// Something is there and it is not this. A collision, as before.
    Different,
}

/// What the tree holds at `item`'s destination.
///
/// Never errors: a destination that cannot be read, a source that has gone
/// away, an archive that will not open — all of them are `Different`, because
/// none of them is positive evidence of sameness.
pub fn presence_of(root: &Path, item: &LayoutItem) -> Presence {
    let Ok(target) = safe_join(root, &item.destination) else {
        // `apply` refuses this destination on its own, with a better reason
        // than "this name is taken". Not our answer to give.
        return Presence::Different;
    };
    if !target.exists() {
        return Presence::Absent;
    }

    let same = match item.placement {
        Placement::CopyFile => same_file(&item.source, &target).unwrap_or(false),
        Placement::CopyTree => same_tree(&item.source, &target).unwrap_or(false),
        Placement::UnpackWhdload => same_pack(&item.source, &target).unwrap_or(false),
    };

    if same {
        Presence::AlreadyInPlace
    } else {
        Presence::Different
    }
}

/// Two files with the same bytes.
fn same_file(source: &Path, target: &Path) -> CoreResult<bool> {
    let (a, b) = (std::fs::metadata(source)?, std::fs::metadata(target)?);
    if !a.is_file() || !b.is_file() || a.len() != b.len() {
        return Ok(false);
    }

    let mut left = std::fs::File::open(source)?;
    let mut right = std::fs::File::open(target)?;
    let mut lbuf = vec![0u8; CHUNK];
    let mut rbuf = vec![0u8; CHUNK];

    loop {
        let n = read_full(&mut left, &mut lbuf)?;
        let m = read_full(&mut right, &mut rbuf)?;
        if n != m {
            return Ok(false);
        }
        if n == 0 {
            return Ok(true);
        }
        if lbuf[..n] != rbuf[..n] {
            return Ok(false);
        }
    }
}

/// Fill `buf` as far as the file allows; `0` at end of file.
fn read_full(file: &mut std::fs::File, buf: &mut [u8]) -> CoreResult<usize> {
    use std::io::Read;
    let mut filled = 0;
    while filled < buf.len() {
        match file.read(&mut buf[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

/// Two directory trees holding the same files, and nothing else.
fn same_tree(source: &Path, target: &Path) -> CoreResult<bool> {
    if !source.is_dir() || !target.is_dir() {
        return Ok(false);
    }
    let Some(left) = walk(source)? else {
        return Ok(false);
    };
    let Some(right) = walk(target)? else {
        return Ok(false);
    };
    if left != right {
        return Ok(false);
    }
    for relative in left.keys() {
        let (a, b) = (joined(source, relative), joined(target, relative));
        if !same_file(&a, &b)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// A `/`-separated relative key, back onto a real path.
fn joined(root: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(root.to_path_buf(), |path, part| path.join(part))
}

/// Every file under `root`, as `relative path → length`.
///
/// `None` when the tree is bigger or deeper than this comparison will walk —
/// which the caller turns into `Different`, never into a silent skip.
fn walk(root: &Path) -> CoreResult<Option<BTreeMap<String, u64>>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];

    while let Some((dir, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            return Ok(None);
        }
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                stack.push((path, depth + 1));
            } else if kind.is_file() {
                let Ok(relative) = path.strip_prefix(root) else {
                    return Ok(None);
                };
                // `/`-separated, so the key means the same thing whatever the
                // host separator is — the archive side speaks `/` and
                // `core/` may not assume a platform.
                let Some(key) = relative.to_str() else {
                    return Ok(None);
                };
                out.insert(key.replace('\\', "/"), entry.metadata()?.len());
                if out.len() > MAX_TREE_ENTRIES {
                    return Ok(None);
                }
            } else {
                // A symlink or something stranger. Not evidence of sameness.
                return Ok(None);
            }
        }
    }
    Ok(Some(out))
}

/// A drawer on disk against the archive that would produce it — entry list
/// only, no decompression. See the module doc for why declared sizes are the
/// right currency in this one place.
fn same_pack(archive: &Path, drawer: &Path) -> CoreResult<bool> {
    use crate::core::whdload::{analyse, Entry};

    let Ok(mut backend) = crate::core::archive::open(archive) else {
        return Ok(false);
    };
    let Ok(entries) = backend.entries() else {
        return Ok(false);
    };
    let listed: Vec<Entry> = entries
        .iter()
        .map(|entry| Entry {
            relative: entry.name.clone(),
            is_dir: entry.is_dir,
        })
        .collect();
    let Ok(layout) = analyse(&listed) else {
        return Ok(false);
    };

    // What `apply` would place inside the drawer: every archive entry inside
    // the pack, minus its wrapper prefix, minus anything `analyse` marked as
    // outside the pack, minus the icon (which lands beside the drawer).
    let prefix = if layout.root.is_empty() {
        String::new()
    } else {
        format!("{}/", layout.root)
    };
    let mut wanted: BTreeMap<String, u64> = BTreeMap::new();
    for entry in entries.iter().filter(|entry| !entry.is_dir) {
        let name = entry.name.replace('\\', "/");
        if layout.outside.contains(&name) {
            continue;
        }
        if layout.icon.as_deref() == Some(name.as_str()) {
            continue;
        }
        let Some(relative) = name.strip_prefix(&prefix) else {
            continue;
        };
        if relative.is_empty() {
            continue;
        }
        wanted.insert(relative.to_string(), entry.declared_bytes);
    }
    if wanted.is_empty() {
        return Ok(false);
    }

    let Some(on_disk) = walk(drawer)? else {
        return Ok(false);
    };
    Ok(on_disk == wanted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::layout::{ItemKind, LayoutItem};

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-presence-{tag}-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn item(source: PathBuf, destination: &str, placement: Placement) -> LayoutItem {
        LayoutItem {
            source,
            kind: ItemKind::FloppyImage,
            destination: destination.into(),
            placement,
            bytes: 0,
            writes_icon: false,
        }
    }

    #[test]
    fn a_destination_that_is_not_there_is_absent() {
        let dir = scratch("absent");
        let source = dir.join("Disk.adf");
        std::fs::write(&source, b"bytes").unwrap();
        let root = dir.join("staging");

        let made = item(source, "Floppies/Disk.adf", Placement::CopyFile);
        assert_eq!(presence_of(&root, &made), Presence::Absent);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_same_file_already_written_is_already_in_place() {
        let dir = scratch("same-file");
        let source = dir.join("Disk.adf");
        std::fs::write(&source, b"the very same bytes").unwrap();
        let root = dir.join("staging");
        std::fs::create_dir_all(root.join("Floppies")).unwrap();
        std::fs::copy(&source, root.join("Floppies").join("Disk.adf")).unwrap();

        let made = item(source, "Floppies/Disk.adf", Placement::CopyFile);
        assert_eq!(presence_of(&root, &made), Presence::AlreadyInPlace);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The assertion that matters most: **the same length is not enough.**
    /// A check that stopped at the size would call a different disk image of
    /// the same size "already done" and silently never copy the real one.
    #[test]
    fn a_different_file_of_the_same_length_is_different() {
        let dir = scratch("same-size");
        let source = dir.join("Disk.adf");
        std::fs::write(&source, b"aaaaaaaaaaaaaaaaaaaa").unwrap();
        let root = dir.join("staging");
        std::fs::create_dir_all(root.join("Floppies")).unwrap();
        std::fs::write(
            root.join("Floppies").join("Disk.adf"),
            b"bbbbbbbbbbbbbbbbbbbb",
        )
        .unwrap();

        let made = item(source, "Floppies/Disk.adf", Placement::CopyFile);
        assert_eq!(presence_of(&root, &made), Presence::Different);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_drawer_copied_whole_is_already_in_place_and_a_short_one_is_not() {
        let dir = scratch("tree");
        let source = dir.join("TurricanII");
        std::fs::create_dir_all(source.join("data")).unwrap();
        std::fs::write(source.join("TurricanII.slave"), b"slave").unwrap();
        std::fs::write(source.join("data").join("level1"), b"level").unwrap();

        let root = dir.join("staging");
        let landed = root.join("Games").join("TurricanII");
        std::fs::create_dir_all(landed.join("data")).unwrap();
        std::fs::write(landed.join("TurricanII.slave"), b"slave").unwrap();

        let made = item(source.clone(), "Games/TurricanII", Placement::CopyTree);
        assert_eq!(
            presence_of(&root, &made),
            Presence::Different,
            "one file short is a half-copied drawer, and must not be skipped"
        );

        std::fs::write(landed.join("data").join("level1"), b"level").unwrap();
        assert_eq!(presence_of(&root, &made), Presence::AlreadyInPlace);

        // …and a stray extra file is not the same drawer either.
        std::fs::write(landed.join("stray"), b"x").unwrap();
        assert_eq!(presence_of(&root, &made), Presence::Different);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_whdload_drawer_matching_its_archive_is_already_in_place() {
        let dir = scratch("pack");
        let archive = dir.join("Turrican.lha");
        std::fs::write(
            &archive,
            crate::core::lha::tests::make_lha_with(&[
                ("Turrican/Turrican.slave", &b"slave"[..]),
                ("Turrican/data/level1", &b"level"[..]),
                ("Turrican.info", &b"icon"[..]),
            ]),
        )
        .unwrap();

        let root = dir.join("staging");
        let drawer = root.join("Games").join("Turrican");
        std::fs::create_dir_all(drawer.join("data")).unwrap();
        std::fs::write(drawer.join("Turrican.slave"), b"slave").unwrap();

        let made = item(archive, "Games/Turrican", Placement::UnpackWhdload);
        assert_eq!(
            presence_of(&root, &made),
            Presence::Different,
            "a drawer missing half the pack is not the pack"
        );

        std::fs::write(drawer.join("data").join("level1"), b"level").unwrap();
        assert_eq!(presence_of(&root, &made), Presence::AlreadyInPlace);

        // The icon lands *beside* the drawer, so its absence inside must not
        // make the drawer look wrong.
        assert!(!drawer.join("Turrican.info").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// When ART cannot read the source at all it must say `Different`, never
    /// `AlreadyInPlace`: silence in that direction is a file the user asked
    /// for that never arrives.
    #[test]
    fn a_source_that_cannot_be_read_is_different_not_already_in_place() {
        let dir = scratch("unreadable");
        let root = dir.join("staging");
        std::fs::create_dir_all(root.join("Floppies")).unwrap();
        std::fs::write(root.join("Floppies").join("Disk.adf"), b"something").unwrap();

        let made = item(
            dir.join("gone.adf"),
            "Floppies/Disk.adf",
            Placement::CopyFile,
        );
        assert_eq!(presence_of(&root, &made), Presence::Different);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
