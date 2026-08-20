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
//! ## The rule, and the mistake it is written against
//!
//! **Compare content, never size.** The first version of this module compared
//! lengths and inferred sameness from them, and the review broke it twice in a
//! minute: a WHDLoad drawer with the right lengths and the wrong bytes was
//! judged already-in-place, and so were two trees differing only by an empty
//! directory. A length is evidence about a file; it is not the file.
//!
//! **Where content cannot be read in full, the answer is
//! [`Presence::Different`] and the item is written.** The two errors are not
//! equally bad and that asymmetry is the whole design:
//!
//! | wrong answer | cost |
//! |---|---|
//! | `Different` for something that was already right | one write that was not needed |
//! | `AlreadyInPlace` for something that differs | a wrong file left on the user's volume, and nothing says so |
//!
//! When one side is a wasted write and the other is silent data loss, the
//! cheap side is the only defensible default. Every branch below therefore
//! ends in `Different` unless it has positively established sameness by
//! reading the bytes.
//!
//! ## What is compared, per placement
//!
//! - [`Placement::CopyFile`] — same length as a cheap reject, then **byte for
//!   byte**, streamed in chunks so a 700 MB HDF costs a compare and not two
//!   copies in memory.
//! - [`Placement::CopyTree`] — the same entries on both sides, **directories
//!   included**, no extras either way, and every file compared as above.
//! - [`Placement::UnpackWhdload`] — every entry inside the pack is
//!   decompressed and compared against the file on disk, in one forward pass
//!   through the backend (which is what a solid 7z archive needs). No declared
//!   size is trusted for anything but the read bound.
//!
//! ## The icon is part of the item
//!
//! ART-106 made a WHDLoad item's `.info` a destination of its own. Presence
//! has to consider it or a resumed apply produces a tree that boots without
//! its icons and says nothing — §82's failure, reached from the resume side.
//! A drawer that matches with its icon missing is [`Presence::IconMissing`],
//! which `apply` repairs by writing the icon alone.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::error::CoreResult;
use crate::core::layout::{icon_destination, LayoutItem, Placement};
use crate::core::security::path::safe_join;

/// How much of a file is compared per read.
const CHUNK: usize = 64 * 1024;

/// The most entries a tree comparison will walk before giving up and saying
/// `Different`. A drawer past this is not something to decide silently.
const MAX_TREE_ENTRIES: usize = 100_000;

/// The deepest a tree comparison goes.
const MAX_DEPTH: usize = 32;

/// What the staging tree already holds at an item's destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Presence {
    /// Nothing there. The ordinary case: the item is new.
    Absent,
    /// Exactly what this item would place, icon and all. Skipped, and counted.
    AlreadyInPlace,
    /// The drawer is exactly right and its `.info` is missing. Not a
    /// collision — there is nothing in the way — and not "already in place"
    /// either, because §82 is not satisfied until the icon is beside the
    /// drawer. `apply` writes the icon and nothing else.
    IconMissing,
    /// Something is there and it is not this. A collision, as before.
    Different,
}

/// One thing found on disk or expected in an archive.
///
/// Directories are entries in their own right, which is the fix for the second
/// case the review broke: two trees differing only by an **empty directory**
/// compared equal while the map held files alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Node {
    Dir,
    File(u64),
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

    let body = match item.placement {
        Placement::CopyFile => same_file(&item.source, &target).unwrap_or(false),
        Placement::CopyTree => same_tree(&item.source, &target).unwrap_or(false),
        Placement::UnpackWhdload => same_pack(&item.source, &target).unwrap_or(false),
    };
    if !body {
        return Presence::Different;
    }

    // The drawer matches. §82's icon is a second destination (ART-106) and
    // has to be right too, or a resumed apply leaves a tree Workbench cannot
    // see and reports it as finished.
    match icon_on_disk(root, item) {
        IconState::NotWanted | IconState::Correct => Presence::AlreadyInPlace,
        IconState::Missing => Presence::IconMissing,
        IconState::Wrong => Presence::Different,
    }
}

/// What the item's `.info` is doing at the destination.
enum IconState {
    /// This item writes no icon.
    NotWanted,
    Correct,
    Missing,
    Wrong,
}

fn icon_on_disk(root: &Path, item: &LayoutItem) -> IconState {
    let Some(relative) = icon_destination(item) else {
        return IconState::NotWanted;
    };
    let Ok(path) = safe_join(root, &relative) else {
        return IconState::Wrong;
    };
    if !path.exists() {
        return IconState::Missing;
    }
    // The icon's own bytes come out of the archive, so checking them means
    // asking the archive. `pack_icon_matches` reads exactly that one entry.
    match pack_icon_matches(&item.source, &path) {
        Ok(true) => IconState::Correct,
        _ => IconState::Wrong,
    }
}

/// Two files with the same bytes.
fn same_file(source: &Path, target: &Path) -> CoreResult<bool> {
    let (a, b) = (std::fs::metadata(source)?, std::fs::metadata(target)?);
    if !a.is_file() || !b.is_file() || a.len() != b.len() {
        return Ok(false);
    }
    same_stream(source, target)
}

/// The byte-for-byte half, on two files already known to be the same length.
fn same_stream(source: &Path, target: &Path) -> CoreResult<bool> {
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

/// Two directory trees holding the same entries — directories included — and
/// every file with the same bytes.
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
    for (relative, node) in &left {
        if matches!(node, Node::Dir) {
            continue;
        }
        // Lengths already agree (they are part of the map above); this is the
        // half that makes it a comparison rather than a guess.
        if !same_stream(&joined(source, relative), &joined(target, relative))? {
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

/// Every entry under `root`, as `relative path → node`.
///
/// `None` when the tree is bigger or deeper than this comparison will walk —
/// which the caller turns into `Different`, never into a silent skip.
fn walk(root: &Path) -> CoreResult<Option<BTreeMap<String, Node>>> {
    let mut out: BTreeMap<String, Node> = BTreeMap::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];

    while let Some((dir, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            return Ok(None);
        }
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            let Ok(relative) = path.strip_prefix(root) else {
                return Ok(None);
            };
            // `/`-separated, so the key means the same thing whatever the host
            // separator is — the archive side speaks `/`, and `core/` may not
            // assume a platform.
            let Some(key) = relative.to_str() else {
                return Ok(None);
            };
            let key = key.replace('\\', "/");

            if kind.is_dir() {
                out.insert(key, Node::Dir);
                stack.push((path, depth + 1));
            } else if kind.is_file() {
                out.insert(key, Node::File(entry.metadata()?.len()));
            } else {
                // A symlink or something stranger. Not evidence of sameness.
                return Ok(None);
            }
            if out.len() > MAX_TREE_ENTRIES {
                return Ok(None);
            }
        }
    }
    Ok(Some(out))
}

/// What an archive says its pack contains, resolved against the drawer that
/// would hold it.
struct PackContents {
    /// Entry index in listing order → `/`-separated path inside the drawer.
    files: Vec<(usize, String)>,
    /// Every path the drawer must hold, files and directories alike.
    expected: BTreeMap<String, Node>,
    /// The entry index of the pack's icon, when it carries one.
    icon: Option<usize>,
}

/// Read an archive's listing and work out what its drawer should look like.
type Backend = Box<dyn crate::core::archive::ArchiveBackend>;

fn pack_contents(archive: &Path) -> CoreResult<Option<(Backend, PackContents)>> {
    use crate::core::whdload::{analyse, Entry};

    let Ok(mut backend) = crate::core::archive::open(archive) else {
        return Ok(None);
    };
    let Ok(entries) = backend.entries() else {
        return Ok(None);
    };
    let listed: Vec<Entry> = entries
        .iter()
        .map(|entry| Entry {
            relative: entry.name.clone(),
            is_dir: entry.is_dir,
        })
        .collect();
    let Ok(layout) = analyse(&listed) else {
        return Ok(None);
    };

    let prefix = if layout.root.is_empty() {
        String::new()
    } else {
        format!("{}/", layout.root)
    };

    let mut files = Vec::new();
    let mut expected: BTreeMap<String, Node> = BTreeMap::new();
    let mut icon = None;

    for (index, entry) in entries.iter().enumerate() {
        let name = entry.name.replace('\\', "/");
        if layout.icon.as_deref() == Some(name.as_str()) {
            icon = Some(index);
            continue;
        }
        if layout.outside.contains(&name) {
            continue;
        }
        let Some(relative) = name.strip_prefix(&prefix) else {
            continue;
        };
        let relative = relative.trim_end_matches('/');
        if relative.is_empty() {
            continue;
        }

        if entry.is_dir {
            expected.insert(relative.to_string(), Node::Dir);
            continue;
        }
        expected.insert(relative.to_string(), Node::File(entry.declared_bytes));
        files.push((index, relative.to_string()));

        // Every parent the extraction would create. Without these an
        // archive that carries no explicit directory entries — most of them —
        // would look different from the drawer it produced.
        let mut parent = relative;
        while let Some(cut) = parent.rfind('/') {
            parent = &parent[..cut];
            if parent.is_empty() {
                break;
            }
            expected.insert(parent.to_string(), Node::Dir);
        }
    }

    if files.is_empty() {
        return Ok(None);
    }
    Ok(Some((
        backend,
        PackContents {
            files,
            expected,
            icon,
        },
    )))
}

/// A drawer on disk against the archive that would produce it — **every entry
/// decompressed and compared**, in one forward pass through the backend.
///
/// One pass rather than one `read` per entry because a 7z archive is solid by
/// default: pulling index *n* on its own decodes everything before it, so
/// index-at-a-time is quadratic on exactly the archives people have. That is
/// the same reason `read_selected` exists for the extraction gate.
fn same_pack(archive: &Path, drawer: &Path) -> CoreResult<bool> {
    let Some((mut backend, pack)) = pack_contents(archive)? else {
        return Ok(false);
    };
    let Some(on_disk) = walk(drawer)? else {
        return Ok(false);
    };

    // Shape first: same paths, same kinds, same declared lengths, nothing
    // extra either way. A cheap reject, and *only* a reject — passing it
    // proves nothing on its own, which is what the previous version of this
    // function got wrong.
    if on_disk != pack.expected {
        return Ok(false);
    }

    // Then the bytes.
    let count = backend.entries()?.len();
    let mut wanted = vec![false; count];
    let mut by_index: BTreeMap<usize, &str> = BTreeMap::new();
    let mut limit = 0u64;
    for (index, relative) in &pack.files {
        wanted[*index] = true;
        by_index.insert(*index, relative.as_str());
        if let Some(Node::File(len)) = on_disk.get(relative.as_str()) {
            limit = limit.max(*len);
        }
    }

    let mut all_match = true;
    backend.read_selected(&wanted, limit.max(1), &mut |index, data| {
        if !all_match {
            return Ok(());
        }
        let Some(relative) = by_index.get(&index) else {
            return Ok(());
        };
        match data.and_then(|bytes| {
            let path = joined(drawer, relative);
            Ok((bytes, std::fs::read(path)?))
        }) {
            Ok((from_archive, from_disk)) => {
                if from_archive != from_disk {
                    all_match = false;
                }
            }
            // Could not read it in full — from either side. That is the case
            // the module doc is written about: no certainty, so no skip.
            Err(_) => all_match = false,
        }
        Ok(())
    })?;

    Ok(all_match)
}

/// Whether the `.info` on disk is the one this archive carries.
fn pack_icon_matches(archive: &Path, on_disk: &Path) -> CoreResult<bool> {
    let Some((mut backend, pack)) = pack_contents(archive)? else {
        return Ok(false);
    };
    let Some(index) = pack.icon else {
        return Ok(false);
    };
    let disk = std::fs::read(on_disk)?;
    // Bounded by what is actually there: an entry that decompresses to more
    // than the file on disk cannot be the same file, and stopping at the
    // bound is cheaper than reading it to find out.
    let Ok(bytes) = backend.read(index, disk.len() as u64) else {
        return Ok(false);
    };
    Ok(bytes == disk)
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

    /// A `.lha` holding `Turrican/Turrican.slave`, `Turrican/data/level1` and
    /// `Turrican.info` beside the drawer.
    fn whdload_lha(path: &Path) {
        std::fs::write(
            path,
            crate::core::lha::tests::make_lha_with(&[
                ("Turrican/Turrican.slave", &b"slave"[..]),
                ("Turrican/data/level1", &b"level"[..]),
                ("Turrican.info", &b"icon"[..]),
            ]),
        )
        .unwrap();
    }

    /// The drawer as a finished apply leaves it, icon beside it.
    fn place_pack(root: &Path) -> PathBuf {
        let drawer = root.join("Games").join("Turrican");
        std::fs::create_dir_all(drawer.join("data")).unwrap();
        std::fs::write(drawer.join("Turrican.slave"), b"slave").unwrap();
        std::fs::write(drawer.join("data").join("level1"), b"level").unwrap();
        std::fs::write(root.join("Games").join("Turrican.info"), b"icon").unwrap();
        drawer
    }

    fn pack_item(archive: PathBuf) -> LayoutItem {
        LayoutItem {
            source: archive,
            kind: ItemKind::WhdloadArchive {
                name: "Turrican".into(),
            },
            destination: "Games/Turrican".into(),
            placement: Placement::UnpackWhdload,
            bytes: 0,
            writes_icon: true,
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

    /// **The same length is not enough.** A check that stopped at the size
    /// would call a different disk image of the same size "already done" and
    /// silently never copy the real one.
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

    /// **G1, the review's second case: an empty directory is a difference.**
    /// The map used to hold files alone, so a tree missing a whole (empty)
    /// drawer compared equal to the one that had it — and a resumed apply
    /// would have skipped placing it.
    #[test]
    fn two_trees_differing_only_by_an_empty_directory_are_different() {
        let dir = scratch("empty-dir");
        let source = dir.join("Game");
        std::fs::create_dir_all(source.join("Saves")).unwrap();
        std::fs::write(source.join("Game.slave"), b"slave").unwrap();

        let root = dir.join("staging");
        let landed = root.join("Games").join("Game");
        std::fs::create_dir_all(&landed).unwrap();
        std::fs::write(landed.join("Game.slave"), b"slave").unwrap();

        let made = item(source, "Games/Game", Placement::CopyTree);
        assert_eq!(
            presence_of(&root, &made),
            Presence::Different,
            "the destination has no Saves/ — every file matches and the tree does not"
        );

        std::fs::create_dir_all(landed.join("Saves")).unwrap();
        assert_eq!(presence_of(&root, &made), Presence::AlreadyInPlace);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_whdload_drawer_matching_its_archive_is_already_in_place() {
        let dir = scratch("pack");
        let archive = dir.join("Turrican.lha");
        whdload_lha(&archive);

        let root = dir.join("staging");
        let drawer = root.join("Games").join("Turrican");
        std::fs::create_dir_all(drawer.join("data")).unwrap();
        std::fs::write(drawer.join("Turrican.slave"), b"slave").unwrap();

        let made = pack_item(archive);
        assert_eq!(
            presence_of(&root, &made),
            Presence::Different,
            "a drawer missing half the pack is not the pack"
        );

        std::fs::write(drawer.join("data").join("level1"), b"level").unwrap();
        std::fs::write(root.join("Games").join("Turrican.info"), b"icon").unwrap();
        assert_eq!(presence_of(&root, &made), Presence::AlreadyInPlace);

        // The icon lands *beside* the drawer, so its absence inside must not
        // make the drawer look wrong.
        assert!(!drawer.join("Turrican.info").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **G1, the review's first case: right lengths, wrong bytes.** The first
    /// version of this module compared the archive's *declared sizes* against
    /// the lengths on disk and skipped on a match, so a drawer whose files had
    /// been replaced byte for byte with something else of the same size was
    /// judged already-in-place and never rewritten.
    #[test]
    fn a_drawer_with_the_right_lengths_and_the_wrong_bytes_is_different() {
        let dir = scratch("pack-wrong-bytes");
        let archive = dir.join("Turrican.lha");
        whdload_lha(&archive);

        let root = dir.join("staging");
        let drawer = place_pack(&root);
        let made = pack_item(archive);
        assert_eq!(presence_of(&root, &made), Presence::AlreadyInPlace);

        // Same length, different content — `slave` → `SLAVE`.
        std::fs::write(drawer.join("Turrican.slave"), b"SLAVE").unwrap();
        assert_eq!(
            presence_of(&root, &made),
            Presence::Different,
            "the lengths still agree, and the file is not the file"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **G1, the review's third case: a resumed apply must restore a missing
    /// `.info`.** ART-106 made the icon a destination; presence has to
    /// consider it, or a resume leaves a tree Workbench cannot see and calls
    /// it finished (§82).
    #[test]
    fn a_drawer_whose_icon_is_missing_asks_for_the_icon_and_not_a_collision() {
        let dir = scratch("pack-no-icon");
        let archive = dir.join("Turrican.lha");
        whdload_lha(&archive);

        let root = dir.join("staging");
        place_pack(&root);
        let made = pack_item(archive);
        assert_eq!(presence_of(&root, &made), Presence::AlreadyInPlace);

        std::fs::remove_file(root.join("Games").join("Turrican.info")).unwrap();
        assert_eq!(
            presence_of(&root, &made),
            Presence::IconMissing,
            "there is nothing in the way, so this is work to do and not a clash"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An icon that is there and is somebody else's is a collision, not a
    /// repair: writing over it would be an overwrite (§93).
    #[test]
    fn a_drawer_whose_icon_is_someone_elses_is_different() {
        let dir = scratch("pack-wrong-icon");
        let archive = dir.join("Turrican.lha");
        whdload_lha(&archive);

        let root = dir.join("staging");
        place_pack(&root);
        std::fs::write(root.join("Games").join("Turrican.info"), b"ICON").unwrap();

        assert_eq!(presence_of(&root, &pack_item(archive)), Presence::Different);

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

    /// The same for an archive that will not open: no listing, no certainty,
    /// no skip.
    #[test]
    fn an_archive_that_will_not_open_is_different() {
        let dir = scratch("bad-archive");
        let archive = dir.join("Turrican.lha");
        std::fs::write(&archive, b"not an archive at all").unwrap();

        let root = dir.join("staging");
        place_pack(&root);

        assert_eq!(presence_of(&root, &pack_item(archive)), Presence::Different);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
