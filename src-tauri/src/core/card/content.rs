//! What one source for a card partition is, where its contents land, and what
//! they will cost — decided before a single byte is unpacked or written.
//!
//! The one-button card (design § 7) takes a list of sources per partition —
//! folders, archives, WHDLoad hardfiles, floppy images — and copies them in.
//! This module is the half of that which reads: [`classify`] says what a path
//! is or why it cannot be used, [`place_archive`] decides where an archive's
//! entries go, [`measure`] counts what the partition will hold, and
//! [`check_partition`] refuses a combination no writer can copy. Task 9's
//! `prepare` stages what this module decided.
//!
//! **Placement is decided from the listing** (R1 § 6). An archive's entry
//! list costs one header walk and no decompression, so the partition can be
//! sized and checked — too-long names, colliding names, names neither writer
//! can carry — before anything is unpacked. A refusal found after a 663 MB
//! collection was staged is a refusal that cost the user the wait for nothing.
//!
//! Three shapes, because WHDLoad archives come in three:
//!
//! - **Plain** — no slave, or a slave at the archive root beside other titles:
//!   every entry at its own path.
//! - **Pack** — exactly one drawer holds a slave: the drawer goes in under its
//!   own name and its icon beside it, the way `core::whdload::analyse` has
//!   always installed one (R2 § 8).
//! - **Collection** — two or more drawers (R1 § 6: `WHDLoadDemos100.lha` is 893
//!   drawers under `Demos/<letter>/`). `analyse` alone would pick one demo
//!   and leave 892 behind, so the tree goes in as it is, from its top drawers.
//!
//! Level-0 LhA names use `\` (R3 § 1: every one of `BoingBag39-1.lha`'s 1 112
//! names). Placement reads them as drawers; the archive's own path decoding
//! is not touched.
//!
//! **Nothing is dropped silently.** What placement does not copy — a readme
//! beside a pack, an install script at a collection's root, the boot files
//! around a hardfile's game, and ART's own records an archive must never plant
//! on a card (R7) — is listed in `left_behind`, so the user can see that ART
//! left it on purpose.
//!
//! `core/card` importing `core/preload` is accepted for this round (R6):
//! `preload/embed.rs` already imports `card`, and the name rules — the ART-113
//! non-ASCII test, the PFS3 name limit, the case fold — must be the copy's own,
//! not a second copy that can disagree with it.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::core::adf::blocks::EntryKind;
use crate::core::adf::fs::{list_directory_on, FileEntry};
use crate::core::archive::{self, ArchiveEntry};
use crate::core::card::sizing::ContentMeasure;
use crate::core::clock::AmigaClock;
use crate::core::detect::{detect, FormatCategory};
use crate::core::error::{CoreError, CoreResult};
use crate::core::gameindex::readers::lhadrawer::slave_drawers;
use crate::core::gameindex::readers::whdhdf::{read_whdload_hardfile, HardfileGame};
use crate::core::jobs::{cancelled_error, ProgressSink};
use crate::core::preload::amiga_names::AMIGA_NAMES_RECORD;
use crate::core::preload::native::{
    collect_entries, needs_latin1, pfs3_name_limit, read_sidecar, MAX_NAMED_NON_ASCII,
};
use crate::core::preload::{amiga_fold, first_collision};
use crate::core::volume::mount::{mount, scan_image};
use crate::core::volume::write::copy::{windows_safe_name, MAX_COPY_DEPTH};
use crate::core::volume::write::uaem::MAX_COMMENT_LEN;
use crate::core::volume::BlockDevice;
use crate::core::whdload::{self, MAX_SLAVE_DEPTH};

/// What a usable source is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SourceKind {
    Folder,
    Archive { format: String },
    WhdloadHardfile,
    Adf,
}

/// Why a source cannot be used, each with its own next step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
pub enum Unusable {
    Missing,
    Unreadable { detail: String },
    ArchiveUnreadable { detail: String },
    HardfileNotWhdload { detail: String },
    NotAnAmigaSource { format_hint: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "verdict", rename_all = "kebab-case")]
pub enum Classified {
    Usable { kind: SourceKind },
    NotUsable { why: Unusable },
}

fn not_usable(why: Unusable) -> Classified {
    Classified::NotUsable { why }
}

/// Say what `path` is as a card source, or why it cannot be one.
///
/// Reads a header and a listing, never a payload. A hardfile is only a source
/// when it is a WHDLoad game in a drawer: a slave at the volume root cannot be
/// told apart from the boot scaffold around it, so there is nothing to copy
/// but the whole volume. `clock` is only what the volume listing needs to
/// read a date; no date decides anything here.
pub fn classify(path: &Path, clock: &dyn AmigaClock) -> Classified {
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return not_usable(Unusable::Missing)
        }
        Err(err) => {
            return not_usable(Unusable::Unreadable {
                detail: err.to_string(),
            })
        }
    };
    if meta.is_dir() {
        return Classified::Usable {
            kind: SourceKind::Folder,
        };
    }
    let detection = match detect(path) {
        Ok(detection) => detection,
        Err(err) => {
            return not_usable(Unusable::Unreadable {
                detail: err.to_string(),
            })
        }
    };
    match detection.format_hint.as_str() {
        "lha" | "zip" | "7z" | "lzx" if detection.category == FormatCategory::Archive => {
            let listed = archive::open(path).and_then(|mut backend| {
                backend.entries()?;
                Ok(backend.format())
            });
            match listed {
                Ok(format) => Classified::Usable {
                    kind: SourceKind::Archive {
                        format: format.to_string(),
                    },
                },
                Err(err) => not_usable(Unusable::ArchiveUnreadable {
                    detail: err.to_string(),
                }),
            }
        }
        "adf" => Classified::Usable {
            kind: SourceKind::Adf,
        },
        "hdf" | "rdb" => match open_hardfile_game(path, clock) {
            Ok(_) => Classified::Usable {
                kind: SourceKind::WhdloadHardfile,
            },
            Err(err) => not_usable(Unusable::HardfileNotWhdload {
                detail: err.to_string(),
            }),
        },
        other => not_usable(Unusable::NotAnAmigaSource {
            format_hint: other.to_string(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Placing an archive
// ---------------------------------------------------------------------------

/// One archive entry and where it lands, relative to the partition root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    /// The entry's index in the archive's own listing.
    pub index: usize,
    pub amiga_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveShape {
    Plain,
    Pack { drawer: String },
    Collection { drawers: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchivePlacement {
    pub shape: ArchiveShape,
    /// In the archive's own order.
    pub placed: Vec<Placed>,
    /// What is not copied, sorted, each once.
    pub left_behind: Vec<String>,
}

/// ART's own records, which an archive must never plant at a partition's root
/// (R7): the escaped-names record a staged folder carries, and the manifest a
/// distribution tree carries.
fn is_art_record(root_name: &str) -> bool {
    root_name.eq_ignore_ascii_case(AMIGA_NAMES_RECORD)
        || root_name.eq_ignore_ascii_case("distribution.json")
}

/// An archive name as a placement path: `\` read as a drawer separator (a
/// level-0 LhA header's, R3 § 1) and a trailing separator dropped.
fn normalise(name: &str) -> String {
    name.replace('\\', "/").trim_end_matches('/').to_string()
}

fn first_component(path: &str) -> &str {
    path.split('/').next().unwrap_or(path)
}

/// Decide where each entry of an archive goes on the partition, from its
/// listing alone. See the module doc comment for the three shapes.
pub fn place_archive(entries: &[ArchiveEntry]) -> CoreResult<ArchivePlacement> {
    let normalised: Vec<ArchiveEntry> = entries
        .iter()
        .map(|entry| ArchiveEntry {
            name: normalise(&entry.name),
            ..entry.clone()
        })
        .collect();

    let mut left_behind = Vec::new();
    let mut usable = Vec::new();
    for (index, entry) in normalised.iter().enumerate() {
        if entry.name.is_empty() {
            continue;
        }
        if !entry.name.contains('/') && is_art_record(&entry.name) {
            left_behind.push(entry.name.clone());
            continue;
        }
        usable.push(index);
    }

    let drawers = slave_drawers(&normalised);
    let (shape, placed) = match drawers.as_slice() {
        [] => plain(&normalised, &usable),
        [only] if only.is_empty() || only.matches('/').count() < MAX_SLAVE_DEPTH => {
            pack(&normalised, &usable, &mut left_behind)?
        }
        // A single slave deeper than `analyse` looks is not a pack it could
        // install; the archive goes in whole rather than be refused.
        [_] => plain(&normalised, &usable),
        many if many.iter().any(|drawer| drawer.is_empty()) => plain(&normalised, &usable),
        many => collection(&normalised, &usable, many, &mut left_behind),
    };

    left_behind.sort();
    left_behind.dedup();
    Ok(ArchivePlacement {
        shape,
        placed,
        left_behind,
    })
}

fn plain(entries: &[ArchiveEntry], usable: &[usize]) -> (ArchiveShape, Vec<Placed>) {
    let placed = usable
        .iter()
        .map(|&index| Placed {
            index,
            amiga_path: entries[index].name.clone(),
        })
        .collect();
    (ArchiveShape::Plain, placed)
}

fn pack(
    entries: &[ArchiveEntry],
    usable: &[usize],
    left_behind: &mut Vec<String>,
) -> CoreResult<(ArchiveShape, Vec<Placed>)> {
    let listed: Vec<whdload::Entry> = usable
        .iter()
        .map(|&index| whdload::Entry {
            relative: entries[index].name.clone(),
            is_dir: entries[index].is_dir,
        })
        .collect();
    let layout = whdload::analyse(&listed)?;
    if layout.needs_installer {
        return Err(CoreError::InvalidInput(format!(
            "'{}' is a WHDLoad install source; run its installer first",
            layout.name
        )));
    }

    let inside = format!("{}/", layout.root);
    let mut placed = Vec::new();
    for &index in usable {
        let name = &entries[index].name;
        let amiga_path = if layout.icon.as_ref() == Some(name) {
            layout.icon_name()
        } else if layout.root.is_empty() {
            format!("{}/{name}", layout.name)
        } else if *name == layout.root {
            layout.name.clone()
        } else if let Some(rest) = name.strip_prefix(&inside) {
            format!("{}/{rest}", layout.name)
        } else {
            continue;
        };
        placed.push(Placed { index, amiga_path });
    }
    left_behind.extend(layout.outside);
    Ok((
        ArchiveShape::Pack {
            drawer: layout.name,
        },
        placed,
    ))
}

fn collection(
    entries: &[ArchiveEntry],
    usable: &[usize],
    drawers: &[String],
    left_behind: &mut Vec<String>,
) -> (ArchiveShape, Vec<Placed>) {
    let tops: BTreeSet<&str> = drawers.iter().map(|d| first_component(d)).collect();
    let mut placed = Vec::new();
    for &index in usable {
        let name = &entries[index].name;
        let first = first_component(name);
        let is_top_icon = !name.contains('/')
            && name
                .strip_suffix(".info")
                .is_some_and(|stem| tops.contains(stem));
        if tops.contains(first) || is_top_icon {
            placed.push(Placed {
                index,
                amiga_path: name.clone(),
            });
        } else {
            left_behind.push(first.to_string());
        }
    }
    (
        ArchiveShape::Collection {
            drawers: drawers.len(),
        },
        placed,
    )
}

// ---------------------------------------------------------------------------
// Measuring
// ---------------------------------------------------------------------------

/// What a source will put on a partition, and the names in it that a writer
/// will have to refuse or rename.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceMeasure {
    pub content: ContentMeasure,
    /// Amiga paths whose own name is not ASCII (ART-113).
    pub non_ascii: Vec<String>,
    pub non_ascii_more: usize,
    /// Amiga paths whose own name is longer than PFS3 stores (ART-314).
    pub too_long: Vec<String>,
    pub too_long_more: usize,
    /// Amiga paths whose host name will differ.
    pub escaped: Vec<String>,
    pub escaped_more: usize,
    /// The distinct names this source puts at the partition's top.
    pub top_level: Vec<String>,
    /// What the source holds that is not copied (see the module doc comment).
    pub left_behind: Vec<String>,
}

/// The most nodes a hardfile's drawer walk visits before refusing.
const MAX_HARDFILE_NODES: usize = 100_000;

/// One node on the partition. Keyed by Amiga path in [`Tally`].
struct Node {
    is_dir: bool,
    bytes: u64,
    comment_bytes: u64,
}

/// Every node a source puts on the partition, once each.
#[derive(Default)]
struct Tally {
    nodes: BTreeMap<String, Node>,
}

impl Tally {
    /// An explicit node, and the drawers its path implies. An explicit node
    /// replaces an implied one; an implied one never replaces anything.
    fn add(&mut self, path: &str, is_dir: bool, bytes: u64, comment_bytes: u64) {
        if path.is_empty() {
            return;
        }
        for (slash, _) in path.match_indices('/') {
            let parent = &path[..slash];
            if !parent.is_empty() {
                self.nodes.entry(parent.to_string()).or_insert(Node {
                    is_dir: true,
                    bytes: 0,
                    comment_bytes: 0,
                });
            }
        }
        self.nodes.insert(
            path.to_string(),
            Node {
                is_dir,
                bytes: if is_dir { 0 } else { bytes },
                comment_bytes,
            },
        );
    }

    fn finish(self, left_behind: Vec<String>) -> SourceMeasure {
        let limit = pfs3_name_limit(libpfs3::format::FORMAT_FNSIZE);
        let mut measure = SourceMeasure {
            left_behind,
            ..Default::default()
        };
        let mut top = BTreeSet::new();
        for (path, node) in &self.nodes {
            let leaf = path.rsplit('/').next().unwrap_or(path);
            if node.is_dir {
                measure
                    .content
                    .add_directory_with_comment(leaf, node.comment_bytes);
            } else {
                measure
                    .content
                    .add_file_with_comment(leaf, node.comment_bytes, node.bytes);
            }
            if needs_latin1(leaf) {
                bounded(&mut measure.non_ascii, &mut measure.non_ascii_more, path);
            }
            if leaf.chars().count() > limit {
                bounded(&mut measure.too_long, &mut measure.too_long_more, path);
            }
            if windows_safe_name(leaf) != leaf {
                bounded(&mut measure.escaped, &mut measure.escaped_more, path);
            }
            top.insert(first_component(path).to_string());
        }
        measure.top_level = top.into_iter().collect();
        measure
    }
}

fn bounded(list: &mut Vec<String>, more: &mut usize, path: &str) {
    if list.len() < MAX_NAMED_NON_ASCII {
        list.push(path.to_string());
    } else {
        *more += 1;
    }
}

/// A comment's cost in a directory entry: Latin-1, one byte a character, at
/// most what AmigaDOS stores.
fn comment_bytes(comment: &str) -> u64 {
    comment.chars().count().min(MAX_COMMENT_LEN) as u64
}

/// Count what `path` (already classified as `kind`) will put on a partition.
///
/// `clock` is what a hardfile's volume listing needs to read an entry's date;
/// nothing measured depends on the date. Stops between whole nodes when
/// `progress` is cancelled.
pub fn measure(
    path: &Path,
    kind: &SourceKind,
    clock: &dyn AmigaClock,
    progress: &dyn ProgressSink,
) -> CoreResult<SourceMeasure> {
    let mut tally = Tally::default();
    let left_behind = match kind {
        SourceKind::Folder => {
            for entry in collect_entries(path)? {
                if progress.is_cancelled() {
                    return Err(cancelled_error());
                }
                let comment = read_sidecar(&entry.host_path)?
                    .map_or(0, |sidecar| comment_bytes(&sidecar.comment));
                tally.add(&entry.relative, entry.is_dir, entry.size, comment);
            }
            Vec::new()
        }
        SourceKind::Archive { .. } => {
            let entries = archive::open(path)?.entries()?;
            let placement = place_archive(&entries)?;
            for placed in &placement.placed {
                if progress.is_cancelled() {
                    return Err(cancelled_error());
                }
                let entry = &entries[placed.index];
                let comment = entry.amiga.comment.as_deref().map_or(0, comment_bytes);
                tally.add(
                    &placed.amiga_path,
                    entry.is_dir,
                    entry.declared_bytes,
                    comment,
                );
            }
            placement.left_behind
        }
        SourceKind::WhdloadHardfile => measure_hardfile(path, clock, progress, &mut tally)?,
        SourceKind::Adf => {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or(CoreError::NonUtf8Path)?;
            tally.add(name, false, std::fs::metadata(path)?.len(), 0);
            Vec::new()
        }
    };
    Ok(tally.finish(left_behind))
}

/// A WHDLoad hardfile's game drawer, found on its mounted volume.
struct HardfileDrawer<D> {
    device: D,
    game: HardfileGame,
    /// The drawer's header block.
    block: u32,
    /// The drawer's path from the volume root, `/`-separated.
    path: String,
    /// The drawer's icon, as its parent directory lists it.
    icon: Option<FileEntry>,
    root_block: u32,
}

/// Read the game in a hardfile and find its drawer on the volume.
///
/// `read_whdload_hardfile` names the drawer by its leaf and, for a slave at
/// the volume root, by the slave's stem; the drawer itself is found here, as
/// a directory of that name holding that slave, at most `MAX_SLAVE_DEPTH`
/// below the root. Not finding one is the volume-root case, refused.
fn open_hardfile_game(
    path: &Path,
    clock: &dyn AmigaClock,
) -> CoreResult<HardfileDrawer<impl BlockDevice>> {
    let game = read_whdload_hardfile(path)?;
    let scanned = scan_image(path)?;
    let entry = scanned
        .volumes
        .iter()
        .find(|volume| volume.is_mountable())
        .ok_or_else(|| CoreError::Malformed {
            format: "whdload-hardfile".into(),
            detail: "no mountable volume in this image".into(),
        })?;
    let (device, geometry) = mount(path, entry)?;
    let root_block = geometry.root_block;

    // Breadth first, so the shallowest drawer wins, as the reader's own
    // tie-break does.
    let mut level = vec![(root_block, String::new())];
    let mut seen = 0usize;
    for _depth in 0..MAX_SLAVE_DEPTH {
        let mut next = Vec::new();
        for (dir_block, dir_path) in &level {
            let listing = list_directory_on(&device, *dir_block, clock)?;
            for found in &listing {
                seen += 1;
                if seen > MAX_HARDFILE_NODES {
                    return Err(hardfile_limit(path));
                }
                if found.kind != EntryKind::Directory {
                    continue;
                }
                let child_path = if dir_path.is_empty() {
                    found.name.clone()
                } else {
                    format!("{dir_path}/{}", found.name)
                };
                if found.name == game.drawer {
                    let holds_slave = list_directory_on(&device, found.header_block, clock)?
                        .iter()
                        .any(|e| e.kind == EntryKind::File && e.name == game.slave_name);
                    if holds_slave {
                        let icon = game.icon.as_ref().and_then(|icon| {
                            listing
                                .iter()
                                .find(|e| e.kind == EntryKind::File && e.name == *icon)
                                .cloned()
                        });
                        return Ok(HardfileDrawer {
                            block: found.header_block,
                            path: child_path,
                            icon,
                            game,
                            device,
                            root_block,
                        });
                    }
                }
                next.push((found.header_block, child_path));
            }
        }
        level = next;
    }
    Err(CoreError::InvalidInput(format!(
        "'{}' holds its slave '{}' at the volume root, not in a drawer, so the game cannot be \
         told apart from the files that boot it; copy the whole image instead",
        path.display(),
        game.slave_name
    )))
}

fn hardfile_limit(path: &Path) -> CoreError {
    CoreError::LimitExceeded {
        subject: "hardfile walk".into(),
        detail: format!(
            "'{}' holds more than {MAX_HARDFILE_NODES} entries or drawers nested deeper than \
             {MAX_COPY_DEPTH}, more than ART will measure; copy its game drawer out first",
            path.display()
        ),
    }
}

fn measure_hardfile(
    path: &Path,
    clock: &dyn AmigaClock,
    progress: &dyn ProgressSink,
    tally: &mut Tally,
) -> CoreResult<Vec<String>> {
    let found = open_hardfile_game(path, clock)?;
    let leaf = found.game.drawer.clone();
    let header = crate::core::adf::fs::read_header_on(&found.device, found.block)?;
    tally.add(&leaf, true, 0, comment_bytes(&header.comment));

    let mut stack = vec![(found.block, leaf.clone(), 1usize)];
    let mut nodes = 0usize;
    while let Some((block, amiga_path, depth)) = stack.pop() {
        if progress.is_cancelled() {
            return Err(cancelled_error());
        }
        for entry in list_directory_on(&found.device, block, clock)? {
            nodes += 1;
            if nodes > MAX_HARDFILE_NODES {
                return Err(hardfile_limit(path));
            }
            let child = format!("{amiga_path}/{}", entry.name);
            let is_dir = entry.kind == EntryKind::Directory;
            tally.add(
                &child,
                is_dir,
                entry.byte_size,
                comment_bytes(&entry.comment),
            );
            if is_dir {
                if depth >= MAX_COPY_DEPTH {
                    return Err(hardfile_limit(path));
                }
                stack.push((entry.header_block, child, depth + 1));
            }
        }
    }

    if let Some(icon) = &found.icon {
        tally.add(
            &format!("{leaf}.info"),
            false,
            icon.byte_size,
            comment_bytes(&icon.comment),
        );
    }

    let drawer_top = first_component(&found.path);
    let at_root = !found.path.contains('/');
    let mut left_behind: Vec<String> = list_directory_on(&found.device, found.root_block, clock)?
        .into_iter()
        .map(|entry| entry.name)
        .filter(|name| name != drawer_top)
        .filter(|name| !(at_root && found.icon.as_ref().is_some_and(|i| i.name == *name)))
        .collect();
    left_behind.sort();
    Ok(left_behind)
}

// ---------------------------------------------------------------------------
// Checking a partition
// ---------------------------------------------------------------------------

/// Refuse a partition's sources that no writer can copy, in this order: a name
/// too long for PFS3 (no fallback has a longer limit, ART-314); two sources
/// putting one AmigaDOS name at the top; non-ASCII names (only hst-imager can
/// write them, ART-113) beside escaped names (only ART's own writer can put
/// them back, ART-160).
pub fn check_partition(sources: &[(PathBuf, SourceMeasure)]) -> CoreResult<()> {
    let mut paths = Vec::new();
    let mut more = 0usize;
    for (_, measured) in sources {
        for path in &measured.too_long {
            bounded(&mut paths, &mut more, path);
        }
        more += measured.too_long_more;
    }
    if !paths.is_empty() {
        return Err(CoreError::Pfs3NamesTooLong {
            paths,
            more,
            max_bytes: pfs3_name_limit(libpfs3::format::FORMAT_FNSIZE),
        });
    }

    // One source never collides with itself here: its own top level is
    // de-duplicated under the same fold first.
    let mut names: Vec<(&str, usize)> = Vec::new();
    for (index, (_, measured)) in sources.iter().enumerate() {
        let mut folds = HashSet::new();
        for name in &measured.top_level {
            if folds.insert(amiga_fold(name)) {
                names.push((name.as_str(), index));
            }
        }
    }
    if let Some(((_, first), (name, second))) = first_collision(names) {
        return Err(CoreError::SourceNamesCollide {
            name: name.to_string(),
            first: sources[first].0.display().to_string(),
            second: sources[second].0.display().to_string(),
        });
    }

    let non_ascii: Vec<String> = sources
        .iter()
        .flat_map(|(_, m)| m.non_ascii.iter().cloned())
        .collect();
    let escaped: Vec<String> = sources
        .iter()
        .flat_map(|(_, m)| m.escaped.iter().cloned())
        .collect();
    if !non_ascii.is_empty() && !escaped.is_empty() {
        return Err(CoreError::NamesNoWriterCanCopy { non_ascii, escaped });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::archive::lzx::tests::{archive as lzx_archive, make_lzx_with, Rec};
    use crate::core::clock::UtcClock;
    use crate::core::gameindex::readers::slave::tests_support::build_slave;
    use crate::core::jobs::NoProgress;
    use crate::core::preload::amiga_names::write_record;
    use crate::core::volume::device::FileRegionMut;
    use crate::core::volume::fixture::make_ffs_volume;
    use crate::core::volume::write::{uaem, FileMeta, VolumeWriter};
    use crate::core::volume::{DosType, VolumeGeometry};
    use std::collections::BTreeMap;

    fn scratch(tag: &str) -> (crate::core::ScratchDir, PathBuf) {
        crate::core::ScratchDir::pair("art-card-content", tag)
    }

    fn file(name: &str, bytes: u64) -> ArchiveEntry {
        ArchiveEntry {
            name: name.into(),
            is_dir: false,
            declared_bytes: bytes,
            amiga: Default::default(),
        }
    }

    fn paths(placement: &ArchivePlacement) -> Vec<&str> {
        placement
            .placed
            .iter()
            .map(|p| p.amiga_path.as_str())
            .collect()
    }

    #[test]
    fn a_single_title_archive_places_its_drawer_and_the_icon_beside_it() {
        let entries = [
            file("Games/Lotus3/Lotus3.slave", 600),
            file("Games/Lotus3/data/disk.1", 900),
            file("Games/Lotus3.info", 900),
            file("ReadMe.txt", 10),
        ];
        let placed = place_archive(&entries).unwrap();
        assert_eq!(
            placed.shape,
            ArchiveShape::Pack {
                drawer: "Lotus3".into()
            }
        );
        assert_eq!(
            paths(&placed),
            ["Lotus3/Lotus3.slave", "Lotus3/data/disk.1", "Lotus3.info"]
        );
        assert_eq!(placed.left_behind, ["ReadMe.txt"]);
    }

    /// R1 § 6: WHDLoadDemos100 is 893 drawers under `Demos/<letter>/`; `analyse`
    /// alone would pick one demo and leave 892 behind.
    #[test]
    fn a_collection_goes_in_as_its_tree_and_root_files_stay_behind() {
        let entries = [
            file("InstallScript", 1232),
            file("Demos.info", 1494),
            file("Demos/A/One/One.slave", 600),
            file("Demos/A/One.info", 900),
            file("Demos/B/Two/Two.slave", 600),
        ];
        let placed = place_archive(&entries).unwrap();
        assert_eq!(placed.shape, ArchiveShape::Collection { drawers: 2 });
        assert_eq!(
            paths(&placed),
            [
                "Demos.info",
                "Demos/A/One/One.slave",
                "Demos/A/One.info",
                "Demos/B/Two/Two.slave"
            ]
        );
        assert_eq!(placed.left_behind, ["InstallScript"]);
    }

    #[test]
    fn an_archive_with_no_slave_goes_in_whole() {
        let entries = [file("SnoopDos/SnoopDos", 100), file("SnoopDos.info", 10)];
        let placed = place_archive(&entries).unwrap();
        assert_eq!(placed.shape, ArchiveShape::Plain);
        assert_eq!(placed.placed.len(), 2);
        assert_eq!(paths(&placed), ["SnoopDos/SnoopDos", "SnoopDos.info"]);
        assert!(placed.left_behind.is_empty());
    }

    /// R3 § 1: `BoingBag39-1.lha`'s 1 112 level-0 names use `\`.
    #[test]
    fn level_zero_backslashes_are_drawers_on_the_card() {
        let entries = [file(r"BoingBag3.9-1\C\Catalogs\dansk\Updater.catalog", 10)];
        let placed = place_archive(&entries).unwrap();
        assert_eq!(
            placed.placed[0].amiga_path,
            "BoingBag3.9-1/C/Catalogs/dansk/Updater.catalog"
        );
    }

    /// R7: an archive must not plant ART's own records at the partition's
    /// root — they are listed as left behind, never placed. The same name one
    /// drawer down is an ordinary file.
    #[test]
    fn art_s_own_records_at_an_archive_root_are_left_behind() {
        let entries = [
            file(".art-amiga-names.json", 10),
            file("distribution.json", 10),
            file("Tools/distribution.json", 10),
            file("Tools/Tool", 10),
        ];
        let placed = place_archive(&entries).unwrap();
        assert_eq!(placed.shape, ArchiveShape::Plain);
        assert_eq!(paths(&placed), ["Tools/distribution.json", "Tools/Tool"]);
        assert_eq!(
            placed.left_behind,
            [".art-amiga-names.json", "distribution.json"]
        );
    }

    /// An archive holding an `Install` script is a pack nobody has installed
    /// yet; copying its raw disks and calling it a game is refused by name.
    #[test]
    fn an_uninstalled_pack_is_refused_naming_it() {
        let entries = [
            file("Lotus3Install/Lotus3.slave", 600),
            file("Lotus3Install/Install", 900),
        ];
        let err = place_archive(&entries).unwrap_err();
        let text = err.to_string();
        assert!(
            text.contains("'Lotus3Install' is a WHDLoad install source; run its installer first"),
            "{text}"
        );
    }

    #[test]
    fn amiga_fold_folds_ascii_and_latin1_letters_only() {
        use crate::core::preload::amiga_fold;
        assert_eq!(amiga_fold("Demos"), "DEMOS");
        assert_eq!(amiga_fold("français"), "FRANÇAIS");
        assert_eq!(amiga_fold("àþ÷ß1_"), "ÀÞ÷ß1_");
    }

    #[test]
    fn measuring_finds_non_ascii_long_and_escaped_names_before_anything_is_written() {
        let (_guard, dir) = scratch("measure-folder");
        let src = dir.join("src");
        std::fs::create_dir_all(src.join("français")).unwrap();
        std::fs::write(src.join("français").join("türkiye.country"), [0u8; 700]).unwrap();
        let long = "x".repeat(107);
        std::fs::write(src.join(&long), b"x").unwrap();
        std::fs::write(src.join("Plain"), b"plain").unwrap();
        std::fs::write(
            src.join("Plain.uaem"),
            uaem::render(&uaem::Sidecar {
                protection: 0,
                date: crate::core::adf::bcpl::AmigaDate {
                    days: 1,
                    mins: 0,
                    ticks: 0,
                },
                comment: "hello".into(),
            }),
        )
        .unwrap();
        // Windows cannot create a file called AUX; the record is how a staged
        // folder says what it really is.
        std::fs::write(src.join("_AUX"), b"aux").unwrap();
        write_record(
            &src,
            &BTreeMap::from([("_AUX".to_string(), "AUX".to_string())]),
        )
        .unwrap();

        let m = measure(&src, &SourceKind::Folder, &UtcClock, &NoProgress).unwrap();
        assert_eq!(m.non_ascii, ["français", "français/türkiye.country"]);
        assert_eq!(m.too_long, std::slice::from_ref(&long));
        assert_eq!(m.escaped, ["AUX"]);
        assert_eq!(m.content.files, 4, "the names record is not measured");
        assert_eq!(m.content.directories, 1);
        assert_eq!(m.top_level, ["AUX", "Plain", "français", long.as_str()]);
        assert!(m.left_behind.is_empty());

        let mut expected = ContentMeasure::default();
        expected.add_directory("français");
        expected.add_file("türkiye.country", 700);
        expected.add_file(&long, 1);
        expected.add_file_with_comment("Plain", 5, 5);
        expected.add_file("AUX", 3);
        assert_eq!(m.content, expected, "the sidecar's comment is charged");
    }

    /// An archive is measured from its listing, placed: implied drawers count
    /// once, a comment is charged, and nothing is unpacked to learn it.
    #[test]
    fn an_archive_is_measured_as_it_will_be_placed() {
        let (_guard, dir) = scratch("measure-lzx");
        let path = dir.join("tool.lzx");
        let data = b"tool!";
        let crc = crate::core::hashing::crc32_ieee(data);
        std::fs::write(
            &path,
            lzx_archive(&[Rec {
                name: "Tool/Tool",
                comment: "hello",
                attrs: 0x0F,
                unpacked: data.len() as u32,
                method: 0,
                data_crc: crc,
                packed: data,
            }]),
        )
        .unwrap();
        let kind = SourceKind::Archive {
            format: "lzx".into(),
        };
        let m = measure(&path, &kind, &UtcClock, &NoProgress).unwrap();
        let mut expected = ContentMeasure::default();
        expected.add_directory("Tool");
        expected.add_file_with_comment("Tool", 5, 5);
        assert_eq!(m.content, expected);
        assert_eq!(m.top_level, ["Tool"]);
    }

    #[test]
    fn an_adf_is_measured_as_one_file_under_its_own_name() {
        let (_guard, dir) = scratch("measure-adf");
        let path = dir.join("Workbench.adf");
        std::fs::write(&path, vec![0u8; 901_120]).unwrap();
        let m = measure(&path, &SourceKind::Adf, &UtcClock, &NoProgress).unwrap();
        let mut expected = ContentMeasure::default();
        expected.add_file("Workbench.adf", 901_120);
        assert_eq!(m.content, expected);
        assert_eq!(m.top_level, ["Workbench.adf"]);
    }

    const HDF_BLOCKS: u32 = 1843;

    /// A bare FFS hardfile written by ART's own writer: `dirs` are made in
    /// order (a parent before its child), then `files` — `(path, bytes)`.
    fn build_hdf(path: &Path, dirs: &[&str], files: &[(&str, &[u8])]) {
        std::fs::write(path, make_ffs_volume(HDF_BLOCKS, "Game", &[])).unwrap();
        let geometry = VolumeGeometry::new(512, HDF_BLOCKS, 2, DosType::new(*b"DOS\x01")).unwrap();
        let mut device = FileRegionMut::open(path, 0, HDF_BLOCKS as u64 * 512, 512).unwrap();
        let mut writer = VolumeWriter::open(&mut device, geometry, path, 0).unwrap();
        let root = writer.geometry().root_block;
        let block_of = |writer: &VolumeWriter, dir: &str| -> u32 {
            let mut block = root;
            for part in dir.split('/').filter(|p| !p.is_empty()) {
                block = writer.find(block, part).unwrap().unwrap().block;
            }
            block
        };
        for dir in dirs {
            let (parent, leaf) = dir.rsplit_once('/').unwrap_or(("", dir));
            let parent = block_of(&writer, parent);
            writer.make_dir(parent, leaf).unwrap();
        }
        for (file, bytes) in files {
            let (parent, leaf) = file.rsplit_once('/').unwrap_or(("", file));
            let parent = block_of(&writer, parent);
            writer
                .add_file(parent, leaf, bytes, FileMeta::default())
                .unwrap();
        }
        drop(writer);
    }

    #[test]
    fn a_whdload_hardfile_is_measured_as_its_drawer_and_icon_only() {
        let (_guard, dir) = scratch("measure-hdf");
        let path = dir.join("Lotus3.hdf");
        let slave = build_slave("Lotus 3", "1992 Gremlin", 16);
        build_hdf(
            &path,
            &["C", "Devs", "Devs/Kickstarts", "s", "Lotus3HD"],
            &[
                ("C/WHDLoad", b"whdload"),
                ("Devs/Kickstarts/kick34005.A500", b"kick"),
                ("s/startup-sequence", b"WHDLoad Lotus3.slave"),
                ("Disk.info", b"disk icon"),
                ("Lotus3HD/Lotus3.slave", &slave),
                ("Lotus3HD/Disk.1", &[7u8; 1500]),
                ("Lotus3HD.info", b"icon"),
            ],
        );

        assert_eq!(
            classify(&path, &UtcClock),
            Classified::Usable {
                kind: SourceKind::WhdloadHardfile
            }
        );
        let m = measure(&path, &SourceKind::WhdloadHardfile, &UtcClock, &NoProgress).unwrap();
        assert_eq!(m.top_level, ["Lotus3HD", "Lotus3HD.info"]);
        assert_eq!(m.content.files, 3, "slave, Disk.1 and the icon");
        assert_eq!(m.content.directories, 1);
        assert_eq!(m.left_behind, ["C", "Devs", "Disk.info", "s"]);

        let mut expected = ContentMeasure::default();
        expected.add_directory("Lotus3HD");
        expected.add_file("Lotus3.slave", slave.len() as u64);
        expected.add_file("Disk.1", 1500);
        expected.add_file("Lotus3HD.info", 4);
        assert_eq!(m.content, expected);
    }

    #[test]
    fn classify_says_what_each_source_is_or_why_not() {
        let (_guard, dir) = scratch("classify");
        let clock = UtcClock;

        let folder = dir.join("folder");
        std::fs::create_dir_all(&folder).unwrap();
        assert_eq!(
            classify(&folder, &clock),
            Classified::Usable {
                kind: SourceKind::Folder
            }
        );

        let zip_path = dir.join("tool.zip");
        {
            let out = std::fs::File::create(&zip_path).unwrap();
            let mut writer = zip::ZipWriter::new(out);
            writer
                .start_file("Tool/Tool", zip::write::SimpleFileOptions::default())
                .unwrap();
            std::io::Write::write_all(&mut writer, b"tool").unwrap();
            writer.finish().unwrap();
        }
        assert_eq!(
            classify(&zip_path, &clock),
            Classified::Usable {
                kind: SourceKind::Archive {
                    format: "zip".into()
                }
            }
        );

        let lzx_path = dir.join("tool.lzx");
        std::fs::write(&lzx_path, make_lzx_with(&[("Tool/Tool", b"tool")])).unwrap();
        assert_eq!(
            classify(&lzx_path, &clock),
            Classified::Usable {
                kind: SourceKind::Archive {
                    format: "lzx".into()
                }
            }
        );

        let broken_zip = dir.join("broken.zip");
        let mut bytes = b"PK\x03\x04".to_vec();
        bytes.extend_from_slice(&[0u8; 60]);
        std::fs::write(&broken_zip, bytes).unwrap();
        assert!(
            matches!(
                classify(&broken_zip, &clock),
                Classified::NotUsable {
                    why: Unusable::ArchiveUnreadable { .. }
                }
            ),
            "{:?}",
            classify(&broken_zip, &clock)
        );

        let adf = dir.join("x.adf");
        std::fs::write(&adf, vec![0u8; 901_120]).unwrap();
        assert_eq!(
            classify(&adf, &clock),
            Classified::Usable {
                kind: SourceKind::Adf
            }
        );

        let no_slave = dir.join("empty.hdf");
        build_hdf(&no_slave, &["C"], &[("C/Dir", b"dir")]);
        let got = classify(&no_slave, &clock);
        assert!(
            matches!(
                &got,
                Classified::NotUsable {
                    why: Unusable::HardfileNotWhdload { detail }
                } if detail.contains("no .slave")
            ),
            "{got:?}"
        );

        // A slave at the volume root: the game cannot be told from the boot
        // scaffold, so there is no drawer to copy.
        let root_slave = dir.join("root.hdf");
        let slave = build_slave("Lotus 3", "1992 Gremlin", 16);
        build_hdf(
            &root_slave,
            &["C"],
            &[("Lotus3.slave", &slave), ("C/Dir", b"d")],
        );
        let got = classify(&root_slave, &clock);
        assert!(
            matches!(
                &got,
                Classified::NotUsable {
                    why: Unusable::HardfileNotWhdload { detail }
                } if detail.contains("volume root")
            ),
            "{got:?}"
        );

        assert_eq!(
            classify(&dir.join("missing.lha"), &clock),
            Classified::NotUsable {
                why: Unusable::Missing
            }
        );

        let text = dir.join("readme.txt");
        std::fs::write(&text, b"just text").unwrap();
        assert!(
            matches!(
                classify(&text, &clock),
                Classified::NotUsable {
                    why: Unusable::NotAnAmigaSource { .. }
                }
            ),
            "{:?}",
            classify(&text, &clock)
        );
    }

    #[test]
    fn two_sources_putting_the_same_name_at_the_top_are_refused_naming_both() {
        let a = SourceMeasure {
            top_level: vec!["Demos".into()],
            ..Default::default()
        };
        let b = SourceMeasure {
            top_level: vec!["DEMOS".into()],
            ..Default::default()
        };
        let err = check_partition(&[
            (PathBuf::from(r"E:\one.lha"), a),
            (PathBuf::from(r"E:\two.lha"), b),
        ])
        .unwrap_err();
        assert!(matches!(err, CoreError::SourceNamesCollide { .. }));
        let text = err.to_string();
        assert!(
            text.contains("one.lha") && text.contains("two.lha") && text.contains("DEMOS"),
            "{text}"
        );
    }

    #[test]
    fn non_ascii_and_escaped_names_in_one_partition_are_refused_naming_both() {
        let a = SourceMeasure {
            non_ascii: vec!["français".into()],
            top_level: vec!["x".into()],
            ..Default::default()
        };
        let b = SourceMeasure {
            escaped: vec!["AUX".into()],
            top_level: vec!["y".into()],
            ..Default::default()
        };
        let err = check_partition(&[(PathBuf::from("a"), a), (PathBuf::from("b"), b)]).unwrap_err();
        assert!(matches!(err, CoreError::NamesNoWriterCanCopy { .. }));
        let text = err.to_string();
        assert!(text.contains("français") && text.contains("AUX"), "{text}");
    }

    #[test]
    fn an_over_long_name_is_refused_with_no_fallback() {
        let a = SourceMeasure {
            too_long: vec!["x".repeat(107)],
            ..Default::default()
        };
        assert!(matches!(
            check_partition(&[(PathBuf::from("a"), a)]),
            Err(CoreError::Pfs3NamesTooLong { max_bytes: 106, .. })
        ));
    }

    /// Negative control: two sources with different names, ASCII only, pass —
    /// one of them holding an escaped name, which on its own is not a refusal
    /// (only escaped *and* non-ASCII together are).
    #[test]
    fn distinct_plain_sources_pass_the_partition_check() {
        let a = SourceMeasure {
            top_level: vec!["A".into(), "AUX".into()],
            escaped: vec!["AUX".into()],
            ..Default::default()
        };
        let b = SourceMeasure {
            top_level: vec!["B".into()],
            ..Default::default()
        };
        assert!(check_partition(&[(PathBuf::from("a"), a), (PathBuf::from("b"), b)]).is_ok());
    }
}
