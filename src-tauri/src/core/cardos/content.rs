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
//! This module lives in `core/cardos`, above both `core/card` and
//! `core/preload`, so it may use the copy's own name rules — the ART-113
//! non-ASCII test, the PFS3 name limit, the case fold — without making those
//! two modules import each other (ART-339, fixed 2026-09-17).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::core::adf::bcpl::{AmigaDate, TICKS_PER_SEC};
use crate::core::adf::blocks::EntryKind;
use crate::core::adf::extract::extract_file_on;
use crate::core::adf::fs::{list_directory_on, read_header_on, FileEntry};
use crate::core::archive::extract::{extract_selection, too_many_entries, OverwritePolicy, Wanted};
use crate::core::archive::{self, ArchiveEntry, EntryDate};
use crate::core::card::sizing::ContentMeasure;
use crate::core::clock::AmigaClock;
use crate::core::detect::{detect, FormatCategory};
use crate::core::error::{CoreError, CoreResult};
use crate::core::gameindex::readers::lhadrawer::slave_drawers;
use crate::core::gameindex::readers::whdhdf::{fs_type_of, read_whdload_hardfile, HardfileGame};
use crate::core::jobs::{cancelled_error, ProgressSink};
use crate::core::preload::amiga_names::{write_record, AMIGA_NAMES_RECORD};
use crate::core::preload::native::{
    collect_entries, latin1_comment, needs_latin1, pfs3_name_limit, read_sidecar,
    MAX_NAMED_NON_ASCII,
};
use crate::core::preload::{amiga_fold, first_collision};
use crate::core::safety::atomic::atomic_write;
use crate::core::security::path::safe_join;
use crate::core::volume::mount::{mount, scan_image};
use crate::core::volume::write::copy::{
    escaped_name_collision, extract_from_volume, header_sidecar, sidecar_for, windows_safe_name,
    SelectedEntry, MAX_COPY_DEPTH,
};
use crate::core::volume::write::file::default_protection;
use crate::core::volume::write::layout::BlockSet;
use crate::core::volume::write::uaem::{self, days_from_civil, MAX_COMMENT_LEN};
use crate::core::volume::{BlockDevice, VolumeGeometry};
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
/// level-0 LhA header's, R3 § 1), and every empty and `.` segment dropped, so
/// `./x`, `/x`, `x/` and `x//y` are `x` and `x/y` (final review I1: a
/// `./.art-amiga-names.json` must meet R7's check as the record it is, and
/// `/Demos` must be the same top-level name as `Demos`). `..` is kept so the
/// extraction gate still refuses it.
fn normalise(name: &str) -> String {
    name.replace('\\', "/")
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect::<Vec<_>>()
        .join("/")
}

/// Refuse a name no AmigaDOS node can have (final review I3): `/` and `:`
/// separate paths on the Amiga, so a node called `a/b` would be copied as `b`
/// inside a drawer `a` — possibly a sibling's. An archive or a hardfile can
/// store any byte in a name; the user hears which one before anything is
/// staged.
fn refuse_unholdable_name(source: &Path, amiga_path: &str, name: &str) -> CoreResult<()> {
    match name.chars().find(|c| *c == '/' || *c == ':') {
        None => Ok(()),
        Some(bad) => Err(CoreError::InvalidInput(format!(
            "'{amiga_path}' in '{}' holds '{bad}', which separates paths on the Amiga, so \
             AmigaDOS cannot hold it as one name and it could not be copied as itself. Rename \
             it on the Amiga side first.",
            source.display()
        ))),
    }
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
                // Final review M3: a comment an Amiga cannot store is refused
                // here, by name, not mid-copy after the format.
                let comment = match read_sidecar(&entry.host_path)? {
                    Some(sidecar) => {
                        latin1_comment(&sidecar.comment, &entry.relative)?;
                        comment_bytes(&sidecar.comment)
                    }
                    None => 0,
                };
                tally.add(&entry.relative, entry.is_dir, entry.size, comment);
            }
            Vec::new()
        }
        SourceKind::Archive { .. } => {
            let mut backend = archive::open(path)?;
            let entries = backend.entries()?;
            // Final review M9: the gate's own entry cap, asked before the
            // source is reported measurable.
            if let Some(reason) = too_many_entries(backend.format(), entries.len()) {
                return Err(CoreError::LimitExceeded {
                    subject: "archive entries".into(),
                    detail: format!("'{}': {reason}", path.display()),
                });
            }
            let placement = place_archive(&entries)?;
            // Final review M5: an archive's own sidecar beside the entry it
            // describes is consumed by the copy, never copied as a file.
            let placed_paths: HashSet<&str> = placement
                .placed
                .iter()
                .map(|placed| placed.amiga_path.as_str())
                .collect();
            for placed in &placement.placed {
                if progress.is_cancelled() {
                    return Err(cancelled_error());
                }
                let amiga_path = placed.amiga_path.as_str();
                if is_uaem_name(amiga_path)
                    && amiga_path
                        .get(..amiga_path.len() - uaem::UAEM_EXTENSION.len() - 1)
                        .is_some_and(|base| placed_paths.contains(base))
                {
                    continue;
                }
                for segment in amiga_path.split('/') {
                    refuse_unholdable_name(path, amiga_path, segment)?;
                }
                let entry = &entries[placed.index];
                if let Some(comment) = &entry.amiga.comment {
                    latin1_comment(comment, amiga_path)?;
                }
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
    geometry: VolumeGeometry,
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
                            geometry,
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

/// Visit every directory under a hardfile's game drawer, the drawer first,
/// with its Amiga path (from the drawer's own name) and its listing. Bounded:
/// more than `MAX_HARDFILE_NODES` entries, or drawers nested deeper than
/// `MAX_COPY_DEPTH`, is refused naming the image.
fn walk_drawer<D: BlockDevice>(
    path: &Path,
    found: &HardfileDrawer<D>,
    clock: &dyn AmigaClock,
    progress: &dyn ProgressSink,
    mut visit: impl FnMut(&str, &[FileEntry]) -> CoreResult<()>,
) -> CoreResult<()> {
    let mut stack = vec![(found.block, found.game.drawer.clone(), 1usize)];
    let mut nodes = 0usize;
    while let Some((block, amiga_path, depth)) = stack.pop() {
        if progress.is_cancelled() {
            return Err(cancelled_error());
        }
        let listing = list_directory_on(&found.device, block, clock)?;
        nodes = nodes.saturating_add(listing.len());
        if nodes > MAX_HARDFILE_NODES {
            return Err(hardfile_limit(path));
        }
        visit(&amiga_path, &listing)?;
        for entry in &listing {
            if entry.kind == EntryKind::Directory {
                if depth >= MAX_COPY_DEPTH {
                    return Err(hardfile_limit(path));
                }
                stack.push((
                    entry.header_block,
                    format!("{amiga_path}/{}", entry.name),
                    depth + 1,
                ));
            }
        }
    }
    Ok(())
}

/// What a hardfile holds besides the game drawer and its icon: the boot
/// scaffold at the volume root and, when the drawer is nested (`Games/Foo`),
/// every sibling along the way (`Games/Bar`) — left behind, and said so
/// (final review M6: nothing is dropped silently).
fn hardfile_left_behind<D: BlockDevice>(
    found: &HardfileDrawer<D>,
    clock: &dyn AmigaClock,
) -> CoreResult<Vec<String>> {
    let parts: Vec<&str> = found.path.split('/').collect();
    let mut left_behind = Vec::new();
    let mut block = found.root_block;
    for depth in 0..parts.len() {
        let is_parent = depth + 1 == parts.len();
        let prefix = parts[..depth].join("/");
        let mut next = None;
        for entry in list_directory_on(&found.device, block, clock)? {
            let on_the_way = if is_parent {
                entry.header_block == found.block
            } else {
                entry.kind == EntryKind::Directory && entry.name == parts[depth]
            };
            if on_the_way {
                next = Some(entry.header_block);
                continue;
            }
            if is_parent && found.icon.as_ref().is_some_and(|i| i.name == entry.name) {
                continue;
            }
            left_behind.push(if prefix.is_empty() {
                entry.name
            } else {
                format!("{prefix}/{}", entry.name)
            });
        }
        block = next.ok_or_else(|| CoreError::Malformed {
            format: "whdload-hardfile".into(),
            detail: format!("'{}' is no longer on the volume", parts[..=depth].join("/")),
        })?;
    }
    left_behind.sort();
    Ok(left_behind)
}

/// The two names a hardfile's game puts at the partition's top — the drawer
/// and its icon — held to [`refuse_unholdable_name`]; the walk asks the rest.
fn refuse_hardfile_names<D: BlockDevice>(path: &Path, found: &HardfileDrawer<D>) -> CoreResult<()> {
    refuse_unholdable_name(path, &found.path, &found.game.drawer)?;
    if let Some(icon) = &found.icon {
        refuse_unholdable_name(path, &icon.name, &icon.name)?;
    }
    Ok(())
}

fn measure_hardfile(
    path: &Path,
    clock: &dyn AmigaClock,
    progress: &dyn ProgressSink,
    tally: &mut Tally,
) -> CoreResult<Vec<String>> {
    let found = open_hardfile_game(path, clock)?;
    refuse_hardfile_names(path, &found)?;
    let leaf = found.game.drawer.clone();
    let header = read_header_on(&found.device, found.block)?;
    tally.add(&leaf, true, 0, comment_bytes(&header.comment));

    walk_drawer(path, &found, clock, progress, |amiga_path, listing| {
        for entry in listing {
            let node = format!("{amiga_path}/{}", entry.name);
            refuse_unholdable_name(path, &node, &entry.name)?;
            tally.add(
                &node,
                entry.kind == EntryKind::Directory,
                entry.byte_size,
                comment_bytes(&entry.comment),
            );
        }
        Ok(())
    })?;

    if let Some(icon) = &found.icon {
        tally.add(
            &format!("{leaf}.info"),
            false,
            icon.byte_size,
            comment_bytes(&icon.comment),
        );
    }
    hardfile_left_behind(&found, clock)
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

    // Final review M8: what each list does not name is counted, so the
    // refusal never reads as complete when it is not.
    let (mut non_ascii, mut non_ascii_more) = (Vec::new(), 0usize);
    let (mut escaped, mut escaped_more) = (Vec::new(), 0usize);
    for (_, measured) in sources {
        for path in &measured.non_ascii {
            bounded(&mut non_ascii, &mut non_ascii_more, path);
        }
        non_ascii_more += measured.non_ascii_more;
        for path in &measured.escaped {
            bounded(&mut escaped, &mut escaped_more, path);
        }
        escaped_more += measured.escaped_more;
    }
    if !non_ascii.is_empty() && !escaped.is_empty() {
        return Err(CoreError::NamesNoWriterCanCopy {
            non_ascii,
            non_ascii_more,
            escaped,
            escaped_more,
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Preparing a source
// ---------------------------------------------------------------------------

/// What [`prepare`] did with one source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Prepared {
    /// The folder the partition copy takes: the source itself for a folder, `staging` otherwise.
    pub root: PathBuf,
    pub staged: bool,
    pub sidecars_written: usize,
    pub escaped: usize,
    pub left_behind: Vec<String>,
}

/// The largest floppy image staged as one file. An HD floppy is 1 760 KiB; a
/// file four times that is not a floppy image.
const MAX_ADF_BYTES: u64 = 4 * 1024 * 1024;

/// The Amiga date for an MS-DOS date (high word) and time (low word). No zone is
/// involved: the pair is what a wall clock showed, which is exactly what an
/// Amiga date is too, so the fields become days, minutes and ticks directly and
/// never pass through an instant (`core::clock` owns every instant-to-Amiga
/// conversion, ART-317). An MS-DOS year starts at 1980, after the Amiga epoch,
/// so nothing needs clamping. `None` for a field out of range.
fn dos_amiga_date(bits: u32) -> Option<AmigaDate> {
    let date = bits >> 16;
    let time = bits & 0xFFFF;
    let year = 1980 + i64::from(date >> 9);
    let month = (date >> 5) & 0xF;
    let day = date & 0x1F;
    let hour = time >> 11;
    let minute = (time >> 5) & 0x3F;
    let second = (time & 0x1F) * 2;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let days =
        days_from_civil(year, i64::from(month), i64::from(day)) - days_from_civil(1978, 1, 1);
    Some(AmigaDate {
        days: u32::try_from(days).ok()?,
        mins: hour * 60 + minute,
        ticks: second * TICKS_PER_SEC as u32,
    })
}

/// Put `path`'s contents (already classified as `kind`) where a partition
/// copy can take them: `staging` for an archive, a hardfile or a floppy image,
/// the folder itself for a folder.
///
/// `staging` is the caller's — the scratch root decides where it is — and must
/// exist and be empty: ART stages into nothing it did not create. What a host
/// folder cannot hold travels beside the files: escaped names in
/// [`AMIGA_NAMES_RECORD`], protection bits, comments and dates in `.uaem`
/// sidecars. Stops between whole entries when `progress` is cancelled; a
/// refusal after unpacking began leaves `staging` for the caller to remove.
pub fn prepare(
    path: &Path,
    kind: &SourceKind,
    staging: &Path,
    clock: &dyn AmigaClock,
    progress: &dyn ProgressSink,
) -> CoreResult<Prepared> {
    let staged = match kind {
        SourceKind::Folder => {
            return Ok(Prepared {
                root: path.to_path_buf(),
                staged: false,
                sidecars_written: 0,
                escaped: 0,
                left_behind: Vec::new(),
            })
        }
        SourceKind::Adf => {
            require_empty_staging(staging)?;
            prepare_adf(path, staging)?
        }
        SourceKind::Archive { .. } => {
            require_empty_staging(staging)?;
            prepare_archive(path, staging, clock, progress)?
        }
        SourceKind::WhdloadHardfile => {
            require_empty_staging(staging)?;
            prepare_hardfile(path, staging, clock, progress)?
        }
    };
    Ok(Prepared {
        root: staging.to_path_buf(),
        ..staged
    })
}

fn require_empty_staging(staging: &Path) -> CoreResult<()> {
    let is_empty_folder = staging.is_dir() && std::fs::read_dir(staging)?.next().is_none();
    if is_empty_folder {
        Ok(())
    } else {
        Err(CoreError::SafetyRefused(format!(
            "'{}' is not an empty folder; ART stages a source only into an empty one",
            staging.display()
        )))
    }
}

/// What staging a source did, before `prepare` fills in `root` and `staged`.
fn staged(
    sidecars_written: usize,
    names: &BTreeMap<String, String>,
    left_behind: Vec<String>,
) -> Prepared {
    Prepared {
        root: PathBuf::new(),
        staged: true,
        sidecars_written,
        escaped: names.len(),
        left_behind,
    }
}

/// Write the names record when anything was escaped.
fn record_names(staging: &Path, names: &BTreeMap<String, String>) -> CoreResult<()> {
    if names.is_empty() {
        Ok(())
    } else {
        write_record(staging, names)
    }
}

fn source_name(path: &Path) -> CoreResult<&str> {
    path.file_name()
        .and_then(|name| name.to_str())
        .ok_or(CoreError::NonUtf8Path)
}

fn prepare_adf(path: &Path, staging: &Path) -> CoreResult<Prepared> {
    let name = source_name(path)?;
    let size = std::fs::metadata(path)?.len();
    if size > MAX_ADF_BYTES {
        return Err(CoreError::InvalidInput(format!(
            "'{}' is {size} bytes, larger than any floppy image ({MAX_ADF_BYTES} bytes at most); \
             if it is a hardfile, rename it to .hdf",
            path.display()
        )));
    }
    let safe = windows_safe_name(name);
    atomic_write(&staging.join(&safe), &std::fs::read(path)?)?;
    let mut names = BTreeMap::new();
    if safe != name {
        names.insert(safe, name.to_string());
    }
    record_names(staging, &names)?;
    Ok(staged(0, &names, Vec::new()))
}

fn could_not_unpack(path: &Path, reason: &str) -> CoreError {
    CoreError::InvalidInput(format!(
        "'{}' could not be unpacked for the card: {reason}. Nothing from it will be copied.",
        path.display()
    ))
}

/// An Amiga path's host path: every segment escaped on its own. `.` and `..`
/// are left as they are so the archive gate refuses them as it always has.
fn host_segment(segment: &str) -> String {
    if segment == "." || segment == ".." {
        segment.to_string()
    } else {
        windows_safe_name(segment)
    }
}

fn is_uaem_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case(uaem::UAEM_EXTENSION))
}

fn prepare_archive(
    path: &Path,
    staging: &Path,
    clock: &dyn AmigaClock,
    progress: &dyn ProgressSink,
) -> CoreResult<Prepared> {
    let mut backend = archive::open(path)?;
    let entries = backend.entries()?;
    let placement = place_archive(&entries)?;

    // Escape every segment, record every one that changed, and refuse two
    // Amiga paths that would share one host path (NTFS ignores case).
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    let mut wanted = Vec::with_capacity(placement.placed.len());
    // Keyed by where the gate will write each entry: its report names a file
    // by the archive's own spelling, not by the host name it was given.
    let mut amiga_of: BTreeMap<PathBuf, (usize, String)> = BTreeMap::new();
    // Folded host path → (the host spelling Windows keeps, the Amiga path).
    let mut hosts: BTreeMap<String, (String, String)> = BTreeMap::new();
    for placed in &placement.placed {
        let mut host = String::new();
        let mut amiga = String::new();
        for segment in placed.amiga_path.split('/').filter(|s| !s.is_empty()) {
            refuse_unholdable_name(path, &placed.amiga_path, segment)?;
            if !host.is_empty() {
                host.push('/');
                amiga.push('/');
            }
            let safe = host_segment(segment);
            host.push_str(&safe);
            amiga.push_str(segment);
            match hosts.get(&host.to_lowercase()) {
                None => {
                    hosts.insert(host.to_lowercase(), (host.clone(), amiga.clone()));
                }
                Some((_, first)) if amiga_fold(first) != amiga_fold(&amiga) => {
                    return Err(CoreError::EscapedNamesCollide {
                        source_path: path.display().to_string().into(),
                        first: first.as_str().into(),
                        second: amiga.as_str().into(),
                        host: host.as_str().into(),
                    });
                }
                // Final review M4: `con/PRN` after `CON/AUX` lands in the
                // folder NTFS already made, `_CON`, so the record is keyed by
                // that spelling — the one the copy will read back.
                Some((first_host, _)) => host = first_host.clone(),
            }
            if safe != segment {
                names
                    .entry(host.clone())
                    .or_insert_with(|| segment.to_string());
            }
        }
        if let Ok(target) = safe_join(staging, &host) {
            amiga_of.insert(target, (placed.index, amiga));
        }
        wanted.push(Wanted {
            index: placed.index,
            name: host,
        });
    }

    let outcome = extract_selection(
        backend.as_mut(),
        &entries,
        &wanted,
        staging,
        OverwritePolicy::Skip,
        progress,
    )?;
    if outcome.aborted {
        let reason = outcome
            .abort_reason
            .as_deref()
            .unwrap_or("the unpacking stopped");
        return Err(could_not_unpack(path, reason));
    }
    if let Some(first) = outcome.errors.first() {
        return Err(could_not_unpack(path, first));
    }
    if let Some(skipped) = outcome.extracted.iter().find(|e| e.skipped) {
        let reason = format!(
            "'{}' {}",
            skipped.source_path,
            skipped.reason.as_deref().unwrap_or("was not written")
        );
        return Err(could_not_unpack(path, &reason));
    }

    // An archive's own sidecars first: one ART cannot read would be copied as
    // a file and its attributes silently lost.
    for done in outcome.extracted.iter().filter(|e| !e.is_dir) {
        let target = Path::new(&done.destination);
        let amiga = amiga_of
            .get(target)
            .map_or(done.source_path.as_str(), |(_, amiga)| amiga.as_str());
        if !is_uaem_name(amiga) {
            continue;
        }
        let parsed = if std::fs::metadata(target)?.len() > uaem::MAX_UAEM_BYTES {
            Err(CoreError::InvalidInput("it is too large".into()))
        } else {
            std::fs::read_to_string(target)
                .map_err(CoreError::from)
                .and_then(|text| uaem::parse(&text))
        };
        if let Err(err) = parsed {
            return Err(CoreError::InvalidInput(format!(
                "'{amiga}' in '{}' is not a .uaem sidecar ART can read ({err}). Nothing from it \
                 will be copied.",
                path.display()
            )));
        }
    }

    let mut sidecars = 0usize;
    for done in &outcome.extracted {
        let Some((index, amiga)) = amiga_of.get(Path::new(&done.destination)) else {
            continue;
        };
        if is_uaem_name(amiga) {
            continue;
        }
        let Some(entry) = entries.get(*index) else {
            continue;
        };
        let sidecar_path = uaem::sidecar_path(Path::new(&done.destination));
        if sidecar_path.exists() {
            // The archive's own sidecar wins.
            continue;
        }
        let date = match entry.amiga.date {
            Some(EntryDate::MsDos(bits)) => dos_amiga_date(bits),
            Some(EntryDate::Unix(seconds)) => Some(clock.amiga_from_unix(seconds)),
            None => None,
        };
        // Final review I2: whether a sidecar is needed is decided without a
        // date the archive never stated; one written for bits or a comment
        // alone is dated the clock's now — what every writer gives an entry
        // with no sidecar — and never the 1978 epoch.
        let sidecar = sidecar_for(
            entry.amiga.protection.unwrap_or(default_protection()),
            date.unwrap_or_default(),
            entry.amiga.comment.as_deref().unwrap_or(""),
        )
        .map(|mut sidecar| {
            if date.is_none() {
                sidecar.date = clock.amiga_now();
            }
            sidecar
        });
        if let Some(sidecar) = sidecar {
            atomic_write(&sidecar_path, uaem::render(&sidecar).as_bytes())?;
            sidecars += 1;
        }
    }

    record_names(staging, &names)?;
    Ok(staged(sidecars, &names, placement.left_behind))
}

/// Refuse a hardfile's drawer holding a name AmigaDOS cannot hold, or two
/// names Windows would stage as one.
fn refuse_hardfile_collisions<D: BlockDevice>(
    path: &Path,
    found: &HardfileDrawer<D>,
    clock: &dyn AmigaClock,
    progress: &dyn ProgressSink,
) -> CoreResult<()> {
    refuse_hardfile_names(path, found)?;
    walk_drawer(path, found, clock, progress, |amiga_dir, listing| {
        for entry in listing {
            refuse_unholdable_name(path, &format!("{amiga_dir}/{}", entry.name), &entry.name)?;
        }
        let selected: Vec<SelectedEntry> = listing
            .iter()
            .map(|entry| SelectedEntry {
                header_block: entry.header_block,
                name: entry.name.clone(),
                is_dir: entry.kind == EntryKind::Directory,
            })
            .collect();
        match escaped_name_collision(&selected) {
            None => Ok(()),
            Some((second, first)) => {
                let host_dir: Vec<String> = amiga_dir.split('/').map(host_segment).collect();
                Err(CoreError::EscapedNamesCollide {
                    source_path: path.display().to_string().into(),
                    first: format!("{amiga_dir}/{first}").into(),
                    second: format!("{amiga_dir}/{second}").into(),
                    host: format!("{}/{}", host_dir.join("/"), windows_safe_name(&second)).into(),
                })
            }
        }
    })
}

fn prepare_hardfile(
    path: &Path,
    staging: &Path,
    clock: &dyn AmigaClock,
    progress: &dyn ProgressSink,
) -> CoreResult<Prepared> {
    let found = open_hardfile_game(path, clock)?;
    refuse_hardfile_collisions(path, &found, clock, progress)?;

    let leaf = found.game.drawer.clone();
    let safe_leaf = windows_safe_name(&leaf);
    let drawer_target = staging.join(&safe_leaf);
    let report = extract_from_volume(
        &found.device,
        &found.geometry,
        found.block,
        &drawer_target,
        true,
        OverwritePolicy::Skip,
        progress,
    )?;
    if report.cancelled {
        return Err(CoreError::Cancelled);
    }
    if let Some(first) = report.skipped.first() {
        return Err(could_not_unpack(path, first));
    }

    let set = BlockSet::new(found.device.block_size());
    let mut sidecars = report.sidecars_written;
    if let Some(sidecar) = header_sidecar(&found.device, &set, found.block)? {
        atomic_write(
            &uaem::sidecar_path(&drawer_target),
            uaem::render(&sidecar).as_bytes(),
        )?;
        sidecars += 1;
    }

    let mut names = BTreeMap::new();
    if safe_leaf != leaf {
        names.insert(safe_leaf, leaf.clone());
    }
    if let Some(icon) = &found.icon {
        let header = read_header_on(&found.device, icon.header_block)?;
        let bytes = extract_file_on(&found.device, &header, fs_type_of(&found.geometry))?;
        let safe_icon = windows_safe_name(&icon.name);
        let icon_target = staging.join(&safe_icon);
        atomic_write(&icon_target, &bytes)?;
        if let Some(sidecar) = header_sidecar(&found.device, &set, icon.header_block)? {
            atomic_write(
                &uaem::sidecar_path(&icon_target),
                uaem::render(&sidecar).as_bytes(),
            )?;
            sidecars += 1;
        }
        if safe_icon != icon.name {
            names.insert(safe_icon, icon.name.clone());
        }
    }
    for escaped in &report.escaped {
        let relative =
            escaped
                .host_path
                .strip_prefix(staging)
                .map_err(|_| CoreError::Malformed {
                    format: "whdload-hardfile".into(),
                    detail: format!(
                        "'{}' was written outside the staging folder",
                        escaped.host_path.display()
                    ),
                })?;
        let key: Vec<String> = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        names.insert(key.join("/"), escaped.amiga_name.clone());
    }

    let left_behind = hardfile_left_behind(&found, clock)?;
    record_names(staging, &names)?;
    Ok(staged(sidecars, &names, left_behind))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::archive::lzx::tests::{archive as lzx_archive, make_lzx_with, Rec};
    use crate::core::clock::UtcClock;
    use crate::core::gameindex::readers::slave::tests_support::build_slave;
    use crate::core::jobs::NoProgress;
    use crate::core::volume::device::FileRegionMut;
    use crate::core::volume::fixture::make_ffs_volume;
    use crate::core::volume::write::{FileMeta, VolumeWriter};
    use crate::core::volume::DosType;
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
    pub(crate) fn build_hdf(path: &Path, dirs: &[&str], files: &[(&str, &[u8])]) {
        build_hdf_with(path, dirs, files, &[]);
    }

    /// `(path, protection, comment)` applied after everything is written.
    type Attrs<'a> = (&'a str, Option<u32>, Option<&'a str>);

    /// [`build_hdf`], with every node dated the Amiga epoch (R2) so that only
    /// the nodes `attrs` names carry anything a sidecar would record.
    fn build_hdf_with(path: &Path, dirs: &[&str], files: &[(&str, &[u8])], attrs: &[Attrs]) {
        let epoch = crate::core::adf::bcpl::AmigaDate::default();
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
                .add_file(
                    parent,
                    leaf,
                    bytes,
                    FileMeta {
                        protection: None,
                        date: Some(epoch),
                    },
                )
                .unwrap();
        }
        // Drawers last: adding a file may restamp the drawer it goes into.
        for dir in dirs {
            let block = block_of(&writer, dir);
            writer
                .set_attributes(block, None, None, Some(epoch))
                .unwrap();
        }
        for (node, protection, comment) in attrs {
            let block = block_of(&writer, node);
            writer
                .set_attributes(block, *protection, *comment, None)
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

    // -----------------------------------------------------------------------
    // prepare
    // -----------------------------------------------------------------------

    /// A scratch folder with an empty `staging` inside it.
    fn staging_in(dir: &Path) -> PathBuf {
        let staging = dir.join("staging");
        std::fs::create_dir_all(&staging).unwrap();
        staging
    }

    /// The names directly inside `dir`, sorted.
    fn listed(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn lzx_kind() -> SourceKind {
        SourceKind::Archive {
            format: "lzx".into(),
        }
    }

    #[test]
    fn a_dos_date_is_wall_clock_seconds() {
        // 2025-01-01 00:00:00 is 1 735 689 600 wall seconds, day 17 167 of the Amiga epoch.
        let midnight = dos_amiga_date(((45 << 9) | (1 << 5) | 1) << 16).expect("a date");
        assert_eq!(
            midnight,
            AmigaDate {
                days: 17_167,
                mins: 0,
                ticks: 0
            }
        );
        assert_eq!(midnight.to_wall_seconds(), 1_735_689_600);
        // 2025-01-01 13:14:10 — hour, minute and the two-second field.
        let afternoon =
            dos_amiga_date((((45 << 9) | (1 << 5) | 1) << 16) | (13 << 11) | (14 << 5) | 5)
                .expect("a date");
        assert_eq!(
            afternoon,
            AmigaDate {
                days: 17_167,
                mins: 13 * 60 + 14,
                ticks: 10 * 50
            }
        );
        assert_eq!(
            afternoon.to_wall_seconds(),
            1_735_689_600 + 13 * 3600 + 14 * 60 + 10
        );
        assert_eq!(dos_amiga_date(0), None, "month 0 is not a date");
        assert_eq!(
            dos_amiga_date((((45 << 9) | (1 << 5) | 1) << 16) | (24 << 11)),
            None,
            "hour 24 is not a time"
        );
    }

    /// The LZX archive the name tests stage: `<drawer>/AUX` (attrs `0x4F`,
    /// comment "note") and `<drawer>/Plain`, in one stored group.
    fn aux_lzx(path: &Path, drawer: &str) {
        let aux_name = format!("{drawer}/AUX");
        let plain_name = format!("{drawer}/Plain");
        std::fs::write(
            path,
            lzx_archive(&[
                Rec {
                    name: &aux_name,
                    comment: "note",
                    attrs: 0x4F,
                    unpacked: 3,
                    method: 0,
                    data_crc: crate::core::hashing::crc32_ieee(b"aux"),
                    packed: b"",
                },
                Rec {
                    name: &plain_name,
                    comment: "",
                    attrs: 0x0F,
                    unpacked: 5,
                    method: 0,
                    data_crc: crate::core::hashing::crc32_ieee(b"plain"),
                    packed: b"auxplain",
                },
            ]),
        )
        .unwrap();
    }

    /// The end of R2's name problem, through the real copy: an archive entry
    /// called `AUX` is staged as `_AUX`, the record says `AUX`, and the native
    /// copy's entry list carries `AUX`.
    #[test]
    fn an_archive_entry_windows_refuses_reaches_the_copy_under_its_amiga_name() {
        use crate::core::preload::amiga_names::AmigaNames;
        let (_guard, dir) = scratch("prepare-aux");
        let lzx = dir.join("aux.lzx");
        aux_lzx(&lzx, "Drawer");
        let staging = staging_in(&dir);
        let clock = crate::core::clock::FixedClock {
            now: 1_800_000_000,
            offset: 0,
        };

        let prepared = prepare(&lzx, &lzx_kind(), &staging, &clock, &NoProgress).unwrap();
        assert_eq!(prepared.root, staging);
        assert!(prepared.staged);
        assert_eq!(prepared.escaped, 1);
        assert_eq!(prepared.sidecars_written, 1);
        assert!(staging.join("Drawer/_AUX").is_file());
        assert_eq!(std::fs::read(staging.join("Drawer/_AUX")).unwrap(), b"aux");
        assert_eq!(
            AmigaNames::read(&staging).name_for("Drawer/_AUX"),
            Some("AUX")
        );
        let entries = collect_entries(&staging).unwrap();
        assert!(
            entries.iter().any(|e| e.relative == "Drawer/AUX"),
            "{:?}",
            entries.iter().map(|e| &e.relative).collect::<Vec<_>>()
        );
        assert!(entries
            .iter()
            .all(|e| !e.relative.contains(".art-amiga-names")));
        let sidecar =
            uaem::parse(&std::fs::read_to_string(staging.join("Drawer/_AUX.uaem")).unwrap())
                .unwrap();
        assert_eq!(sidecar.protection, 0x40);
        assert_eq!(sidecar.comment, "note");
        // Final review I2: an LZX member states no date, so the sidecar its
        // bits need carries the clock's now — never the 1978 epoch.
        assert_eq!(sidecar.date, clock.amiga_now());
        assert!(
            !staging.join("Drawer/Plain.uaem").exists(),
            "default bits, no comment, no date: no sidecar"
        );

        // Every segment is escaped, not only the leaf: a drawer called `CON`
        // is staged as `_CON` and recorded as `CON`.
        let (_guard2, dir2) = scratch("prepare-con");
        let lzx2 = dir2.join("con.lzx");
        aux_lzx(&lzx2, "CON");
        let staging2 = staging_in(&dir2);
        let prepared2 = prepare(&lzx2, &lzx_kind(), &staging2, &UtcClock, &NoProgress).unwrap();
        assert_eq!(prepared2.escaped, 2);
        assert!(staging2.join("_CON/_AUX").is_file());
        assert!(staging2.join("_CON/Plain").is_file());
        let names = AmigaNames::read(&staging2);
        assert_eq!(names.name_for("_CON"), Some("CON"));
        assert_eq!(names.name_for("_CON/_AUX"), Some("AUX"));
        // Sorted: the walk goes in host-name order, where `_AUX` follows `Plain`.
        let mut relatives: Vec<String> = collect_entries(&staging2)
            .unwrap()
            .into_iter()
            .map(|e| e.relative)
            .collect();
        relatives.sort();
        assert_eq!(relatives, ["CON", "CON/AUX", "CON/Plain"]);
    }

    /// The first of R3's claims measured end to end: bits, comment and date
    /// from an archive land on a PFS3 volume. Read back through libpfs3.
    ///
    /// Staged with a clock three hours east of UTC: an MS-DOS date is wall
    /// time already, so the offset must not move it.
    #[test]
    fn archive_attributes_land_on_a_pfs3_partition() {
        use crate::core::clock::{amiga_from_wall, FixedClock};
        use crate::core::preload::native::test_support::{formatted_pds3_image, partition_offset};
        use crate::core::preload::native::{pfs3_datestamp, NativeFormatter};
        use crate::core::preload::VolumeFormatter;

        let (_guard, dir) = scratch("prepare-pfs3");
        let lha = dir.join("assign.lha");
        std::fs::write(
            &lha,
            crate::core::lha::tests::make_level1_lha(b"C", b"Assign", b"x"),
        )
        .unwrap();
        let staging = staging_in(&dir);
        let clock = FixedClock {
            now: 1_800_000_000,
            offset: 3 * 3600,
        };
        let kind = SourceKind::Archive {
            format: "lha".into(),
        };
        let prepared = prepare(&lha, &kind, &staging, &clock, &NoProgress).unwrap();
        assert_eq!(prepared.sidecars_written, 1);
        assert_eq!(prepared.escaped, 0);

        let (_guard2, image) = formatted_pds3_image();
        NativeFormatter::UTC
            .copy_in(&image, None, "DH0", &staging, &NoProgress)
            .unwrap();
        let mut vol = libpfs3::volume::Volume::open(&image, partition_offset(&image)).unwrap();
        let entry = vol
            .list_dir("C")
            .unwrap()
            .into_iter()
            .find(|e| e.name == "Assign")
            .expect("C/Assign on the partition");
        assert_eq!(
            libpfs3::util::amiga_protection_string(entry.protection),
            "--p-rwed"
        );
        assert_eq!(
            (
                entry.creation_day,
                entry.creation_minute,
                entry.creation_tick
            ),
            pfs3_datestamp(amiga_from_wall(1_735_689_600))
        );
    }

    #[test]
    fn a_whdload_hardfile_stages_only_its_drawer_and_icon() {
        let (_guard, dir) = scratch("prepare-hdf");
        let path = dir.join("Lotus3.hdf");
        let slave = build_slave("Lotus 3", "1992 Gremlin", 16);
        build_hdf_with(
            &path,
            &["C", "Devs", "Devs/Kickstarts", "s", "Lotus3HD"],
            &[
                ("C/WHDLoad", b"whdload"),
                ("Devs/Kickstarts/kick34005.A500", b"kick"),
                ("s/startup-sequence", b"WHDLoad Lotus3.slave"),
                ("Disk.info", b"disk icon"),
                ("Lotus3HD/Lotus3.slave", &slave),
                ("Lotus3HD/Disk.1", &[7u8; 1500]),
                ("Lotus3HD/ReadMe.info", b"readme icon"),
                ("Lotus3HD.info", b"icon"),
            ],
            &[
                ("Lotus3HD/ReadMe.info", Some(0x80), None),
                ("Lotus3HD", None, Some("drawer")),
            ],
        );
        let staging = staging_in(&dir);

        let prepared = prepare(
            &path,
            &SourceKind::WhdloadHardfile,
            &staging,
            &UtcClock,
            &NoProgress,
        )
        .unwrap();

        // Every node is dated the epoch with default bits (R2), so the only
        // sidecars are the two nodes given attributes: the drawer's own
        // (its comment) and ReadMe.info's (its `h` bit). The icon carries
        // nothing, so `Lotus3HD.info.uaem` is rightly absent.
        assert_eq!(
            listed(&staging),
            ["Lotus3HD", "Lotus3HD.info", "Lotus3HD.uaem"]
        );
        assert_eq!(
            listed(&staging.join("Lotus3HD")),
            ["Disk.1", "Lotus3.slave", "ReadMe.info", "ReadMe.info.uaem"]
        );
        assert_eq!(
            std::fs::read(staging.join("Lotus3HD.info")).unwrap(),
            b"icon"
        );
        let readme = uaem::parse(
            &std::fs::read_to_string(staging.join("Lotus3HD/ReadMe.info.uaem")).unwrap(),
        )
        .unwrap();
        assert_eq!(readme.protection, 0x80);
        let drawer =
            uaem::parse(&std::fs::read_to_string(staging.join("Lotus3HD.uaem")).unwrap()).unwrap();
        assert_eq!(drawer.comment, "drawer");
        assert!(!staging.join("Devs").exists() && !staging.join("C").exists());
        assert_eq!(prepared.left_behind, ["C", "Devs", "Disk.info", "s"]);
        assert_eq!(prepared.sidecars_written, 2);
        assert_eq!(prepared.escaped, 0);
        assert!(prepared.staged);
        assert_eq!(prepared.root, staging);
    }

    #[test]
    fn a_hardfile_whose_names_collide_once_escaped_is_refused_naming_both() {
        let (_guard, dir) = scratch("prepare-hdf-collide");
        let path = dir.join("Game.hdf");
        let slave = build_slave("Game", "1990 Somebody", 16);
        build_hdf(
            &path,
            &["Game"],
            &[
                ("Game/Game.slave", &slave),
                ("Game/A?B", b"one"),
                ("Game/A_B", b"two"),
            ],
        );
        let staging = staging_in(&dir);

        let err = prepare(
            &path,
            &SourceKind::WhdloadHardfile,
            &staging,
            &UtcClock,
            &NoProgress,
        )
        .unwrap_err();
        assert!(
            matches!(err, CoreError::EscapedNamesCollide { .. }),
            "{err:?}"
        );
        let text = err.to_string();
        assert!(
            text.contains("'Game/A?B'") && text.contains("'Game/A_B'") && text.contains("Game.hdf"),
            "{text}"
        );
        assert!(listed(&staging).is_empty(), "nothing is staged");
    }

    fn zip_with(path: &Path, files: &[(&str, &[u8])]) {
        let out = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(out);
        for (name, bytes) in files {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            std::io::Write::write_all(&mut writer, bytes).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn an_archive_s_own_sidecar_is_kept_and_a_broken_one_is_refused() {
        let zip_kind = SourceKind::Archive {
            format: "zip".into(),
        };
        let (_guard, dir) = scratch("prepare-zip-sidecar");
        let own = "--p-rwed 2021-04-13 02:43:13.68 x\n";
        let good = dir.join("good.zip");
        // A PC-made ZIP: every entry has a date, so ART would write a sidecar
        // for `Game.slave` if the archive did not bring its own.
        zip_with(
            &good,
            &[
                ("Game/Game.slave", b"slave"),
                ("Game/Game.slave.uaem", own.as_bytes()),
            ],
        );
        let staging = staging_in(&dir);
        let prepared = prepare(&good, &zip_kind, &staging, &UtcClock, &NoProgress).unwrap();
        assert_eq!(
            std::fs::read_to_string(staging.join("Game/Game.slave.uaem")).unwrap(),
            own,
            "the archive's own sidecar is not replaced"
        );
        assert!(
            !staging.join("Game/Game.slave.uaem.uaem").exists(),
            "a sidecar gets no sidecar"
        );
        assert_eq!(prepared.sidecars_written, 0);

        let (_guard2, dir2) = scratch("prepare-zip-bad-sidecar");
        let bad = dir2.join("bad.zip");
        zip_with(&bad, &[("Tool", b"tool"), ("Bad.uaem", b"not a sidecar")]);
        let staging2 = staging_in(&dir2);
        let err = prepare(&bad, &zip_kind, &staging2, &UtcClock, &NoProgress).unwrap_err();
        let text = err.to_string();
        assert!(
            text.contains("'Bad.uaem'") && text.contains("bad.zip"),
            "{text}"
        );
    }

    // -----------------------------------------------------------------------
    // Final review fix wave
    // -----------------------------------------------------------------------

    fn zip_kind() -> SourceKind {
        SourceKind::Archive {
            format: "zip".into(),
        }
    }

    /// Final review I1: a `./` or `/` prefix does not hide ART's own records
    /// from R7. Both land in `left_behind`, neither is staged, and the prefix
    /// does not reach `top_level` as a name of its own.
    #[test]
    fn a_dot_or_slash_prefixed_art_record_is_left_behind_and_never_staged() {
        let entries = [
            file("./.art-amiga-names.json", 10),
            file("/distribution.json", 10),
            file("./Tools/Tool", 10),
            file("/Demos//x", 10),
        ];
        let placed = place_archive(&entries).unwrap();
        assert_eq!(paths(&placed), ["Tools/Tool", "Demos/x"]);
        assert_eq!(
            placed.left_behind,
            [".art-amiga-names.json", "distribution.json"]
        );

        let (_guard, dir) = scratch("prepare-dot-record");
        let path = dir.join("hostile.zip");
        let record = br#"{"version":1,"names":[{"host":"Tool","amiga":"Other"}]}"#;
        std::fs::write(
            &path,
            crate::core::archive::zip::tests::make_zip_with(&[
                ("./.art-amiga-names.json", record),
                ("/distribution.json", b"{}"),
                ("./Tool", b"tool"),
            ]),
        )
        .unwrap();
        let measured = measure(&path, &zip_kind(), &UtcClock, &NoProgress).unwrap();
        assert_eq!(measured.top_level, ["Tool"]);
        assert_eq!(
            measured.left_behind,
            [".art-amiga-names.json", "distribution.json"]
        );

        let staging = staging_in(&dir);
        let prepared = prepare(&path, &zip_kind(), &staging, &UtcClock, &NoProgress).unwrap();
        assert_eq!(prepared.left_behind, measured.left_behind);
        assert!(!staging.join(AMIGA_NAMES_RECORD).exists());
        assert!(!staging.join("distribution.json").exists());
        let relatives: Vec<String> = collect_entries(&staging)
            .unwrap()
            .into_iter()
            .map(|e| e.relative)
            .collect();
        assert_eq!(relatives, ["Tool"]);
    }

    /// Patch a node's name in a bare FFS hardfile past `check_name`, the way a
    /// hostile or damaged image can store any byte, and fix the checksum.
    fn patch_hdf_name(path: &Path, node: &str, new_name: &str) {
        let block = {
            let geometry =
                VolumeGeometry::new(512, HDF_BLOCKS, 2, DosType::new(*b"DOS\x01")).unwrap();
            let mut device = FileRegionMut::open(path, 0, HDF_BLOCKS as u64 * 512, 512).unwrap();
            let writer = VolumeWriter::open(&mut device, geometry, path, 0).unwrap();
            let mut block = writer.geometry().root_block;
            for part in node.split('/') {
                block = writer.find(block, part).unwrap().unwrap().block;
            }
            block
        };
        let mut image = std::fs::read(path).unwrap();
        let header = &mut image[block as usize * 512..(block as usize + 1) * 512];
        header[432..432 + 31].fill(0);
        header[432] = new_name.len() as u8;
        header[433..433 + new_name.len()].copy_from_slice(new_name.as_bytes());
        header[20..24].fill(0);
        let sum = header.chunks(4).fold(0u32, |sum, long| {
            sum.wrapping_add(u32::from_be_bytes(long.try_into().unwrap()))
        });
        header[20..24].copy_from_slice(&0u32.wrapping_sub(sum).to_be_bytes());
        std::fs::write(path, image).unwrap();
    }

    /// Final review I3: a hardfile name holding `/` would become a path on the
    /// card. Refused by name when measured and when prepared, before anything
    /// is staged.
    #[test]
    fn a_hardfile_name_amigados_cannot_hold_is_refused_by_name() {
        let (_guard, dir) = scratch("prepare-hdf-slash");
        let path = dir.join("Game.hdf");
        let slave = build_slave("Game", "1990 Somebody", 16);
        build_hdf(
            &path,
            &["Game", "Game/a"],
            &[("Game/Game.slave", &slave), ("Game/a_b", b"one")],
        );
        patch_hdf_name(&path, "Game/a_b", "a/b");

        let err = measure(&path, &SourceKind::WhdloadHardfile, &UtcClock, &NoProgress)
            .expect_err("measured");
        let text = err.to_string();
        assert!(
            text.contains("'Game/a/b'") && text.contains("Game.hdf") && text.contains("AmigaDOS"),
            "{text}"
        );

        let staging = staging_in(&dir);
        let err = prepare(
            &path,
            &SourceKind::WhdloadHardfile,
            &staging,
            &UtcClock,
            &NoProgress,
        )
        .expect_err("prepared");
        let text = err.to_string();
        assert!(
            text.contains("'Game/a/b'") && text.contains("Game.hdf") && text.contains("AmigaDOS"),
            "{text}"
        );
        assert!(listed(&staging).is_empty(), "nothing is staged");
    }

    /// Final review I3, the archive half: a `:` in an entry name is refused by
    /// name before the archive is unpacked.
    #[test]
    fn an_archive_name_holding_a_colon_is_refused_by_name() {
        let (_guard, dir) = scratch("prepare-zip-colon");
        let path = dir.join("colon.zip");
        std::fs::write(
            &path,
            crate::core::archive::zip::tests::make_zip_with(&[
                ("Tool/ok", b"ok"),
                ("Tool/a:b", b"x"),
            ]),
        )
        .unwrap();
        let err = measure(&path, &zip_kind(), &UtcClock, &NoProgress).expect_err("measured");
        let text = err.to_string();
        assert!(
            text.contains("'Tool/a:b'") && text.contains("colon.zip"),
            "{text}"
        );
        let staging = staging_in(&dir);
        let err =
            prepare(&path, &zip_kind(), &staging, &UtcClock, &NoProgress).expect_err("prepared");
        let text = err.to_string();
        assert!(
            text.contains("'Tool/a:b'") && text.contains("colon.zip"),
            "{text}"
        );
        assert!(listed(&staging).is_empty(), "nothing is staged");
    }

    /// Final review M3: a comment outside Latin-1 is refused by name when the
    /// source is measured, not mid-copy after the format.
    #[test]
    fn a_comment_an_amiga_cannot_store_is_refused_when_measured() {
        let (_guard, dir) = scratch("measure-comment-not-latin1");
        let src = dir.join("src");
        std::fs::create_dir_all(src.join("C")).unwrap();
        std::fs::write(src.join("C/Assign"), b"x").unwrap();
        std::fs::write(
            src.join("C/Assign.uaem"),
            "--p-rwed 2021-04-13 02:43:13.68 日本\n",
        )
        .unwrap();
        let err = measure(&src, &SourceKind::Folder, &UtcClock, &NoProgress).unwrap_err();
        let text = err.to_string();
        assert!(
            text.contains("C/Assign") && text.contains("an Amiga cannot store"),
            "{text}"
        );
    }

    /// Final review M4: `CON/AUX` and `con/PRN` share one NTFS folder, `_CON`;
    /// the record is keyed by the spelling Windows kept, so the copy finds it.
    #[test]
    fn names_sharing_one_ntfs_folder_are_recorded_under_the_folder_windows_kept() {
        use crate::core::preload::amiga_names::AmigaNames;
        let (_guard, dir) = scratch("prepare-con-case");
        let path = dir.join("con.zip");
        std::fs::write(
            &path,
            crate::core::archive::zip::tests::make_zip_with(&[
                ("CON/AUX", b"aux"),
                ("con/PRN", b"prn"),
            ]),
        )
        .unwrap();
        let staging = staging_in(&dir);
        prepare(&path, &zip_kind(), &staging, &UtcClock, &NoProgress).unwrap();
        assert_eq!(listed(&staging), [AMIGA_NAMES_RECORD, "_CON"]);
        let names = AmigaNames::read(&staging);
        assert_eq!(names.name_for("_CON/_PRN"), Some("PRN"));
        let mut relatives: Vec<String> = collect_entries(&staging)
            .unwrap()
            .into_iter()
            .map(|e| e.relative)
            .collect();
        relatives.sort();
        assert_eq!(relatives, ["CON", "CON/AUX", "CON/PRN"]);
    }

    /// Final review M5: an archive's own `.uaem` beside the entry it describes
    /// is consumed by the copy, so it is not measured as a card file.
    #[test]
    fn an_archive_s_own_sidecars_are_not_measured_as_card_files() {
        let (_guard, dir) = scratch("measure-zip-sidecar");
        let path = dir.join("tool.zip");
        std::fs::write(
            &path,
            crate::core::archive::zip::tests::make_zip_with(&[
                ("Tool", b"tool"),
                ("Tool.uaem", b"--p-rwed 2021-04-13 02:43:13.68 x\n"),
            ]),
        )
        .unwrap();
        let m = measure(&path, &zip_kind(), &UtcClock, &NoProgress).unwrap();
        assert_eq!(m.content.files, 1);
        assert_eq!(m.top_level, ["Tool"]);
    }

    /// Final review M6: a game drawer nested in `Games` leaves its siblings
    /// behind on purpose, and says so.
    #[test]
    fn a_nested_game_drawer_s_siblings_are_listed_as_left_behind() {
        let (_guard, dir) = scratch("prepare-hdf-nested");
        let path = dir.join("Foo.hdf");
        let slave = build_slave("Foo", "1990 Somebody", 16);
        build_hdf(
            &path,
            &["Games", "Games/Foo", "Games/Bar"],
            &[
                ("Disk.info", b"disk"),
                ("Games/ReadMe", b"readme"),
                ("Games/Foo.info", b"icon"),
                ("Games/Foo/Foo.slave", &slave),
            ],
        );
        let expected = ["Disk.info", "Games/Bar", "Games/ReadMe"];
        let m = measure(&path, &SourceKind::WhdloadHardfile, &UtcClock, &NoProgress).unwrap();
        assert_eq!(m.top_level, ["Foo", "Foo.info"]);
        assert_eq!(m.left_behind, expected);
        let staging = staging_in(&dir);
        let prepared = prepare(
            &path,
            &SourceKind::WhdloadHardfile,
            &staging,
            &UtcClock,
            &NoProgress,
        )
        .unwrap();
        assert_eq!(prepared.left_behind, expected);
    }

    /// Final review M8: the names refusal says how many names it did not list.
    #[test]
    fn a_names_refusal_says_how_many_it_did_not_name() {
        let a = SourceMeasure {
            non_ascii: vec!["français".into()],
            non_ascii_more: 3,
            top_level: vec!["x".into()],
            ..Default::default()
        };
        let b = SourceMeasure {
            escaped: vec!["AUX".into()],
            escaped_more: 7,
            top_level: vec!["y".into()],
            ..Default::default()
        };
        let err = check_partition(&[(PathBuf::from("a"), a), (PathBuf::from("b"), b)]).unwrap_err();
        let text = err.to_string();
        assert!(
            text.contains("français, and 3 more") && text.contains("AUX, and 7 more"),
            "{text}"
        );
    }

    /// Final review M9: an archive past the extraction gate's entry cap is
    /// refused when measured, not reported usable and refused at `prepare`.
    #[test]
    fn an_archive_past_the_entry_cap_is_refused_when_measured() {
        use crate::core::archive::extract::MAX_ENTRIES;
        let (_guard, dir) = scratch("measure-lzx-cap");
        let path = dir.join("many.lzx");
        let names: Vec<String> = (0..=MAX_ENTRIES).map(|i| format!("F{i}")).collect();
        let records: Vec<Rec> = names
            .iter()
            .map(|name| Rec {
                name,
                comment: "",
                attrs: 0x0F,
                unpacked: 0,
                method: 0,
                data_crc: 0,
                packed: b"",
            })
            .collect();
        std::fs::write(&path, lzx_archive(&records)).unwrap();
        let err = measure(&path, &lzx_kind(), &UtcClock, &NoProgress).unwrap_err();
        let text = err.to_string();
        assert!(
            text.contains("many.lzx") && text.contains(&format!("at most {MAX_ENTRIES}")),
            "{text}"
        );
    }

    #[test]
    fn staging_that_is_not_empty_is_refused() {
        let (_guard, dir) = scratch("prepare-not-empty");
        let lzx = dir.join("aux.lzx");
        aux_lzx(&lzx, "Drawer");
        let staging = staging_in(&dir);
        std::fs::write(staging.join("mine.txt"), b"keep me").unwrap();

        let err = prepare(&lzx, &lzx_kind(), &staging, &UtcClock, &NoProgress).unwrap_err();
        assert!(matches!(err, CoreError::SafetyRefused(_)), "{err:?}");
        assert!(
            err.to_string()
                .contains("is not an empty folder; ART stages a source only into an empty one"),
            "{err}"
        );
        assert_eq!(listed(&staging), ["mine.txt"]);
        assert_eq!(std::fs::read(staging.join("mine.txt")).unwrap(), b"keep me");

        // A staging folder that does not exist is not an empty one either.
        let missing = dir.join("missing");
        let err = prepare(&lzx, &lzx_kind(), &missing, &UtcClock, &NoProgress).unwrap_err();
        assert!(matches!(err, CoreError::SafetyRefused(_)), "{err:?}");
        assert!(!missing.exists());
    }

    #[test]
    fn a_folder_is_used_where_it_is() {
        let (_guard, dir) = scratch("prepare-folder");
        let folder = dir.join("Tools");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("Tool"), b"tool").unwrap();
        let staging = staging_in(&dir);

        let prepared = prepare(
            &folder,
            &SourceKind::Folder,
            &staging,
            &UtcClock,
            &NoProgress,
        )
        .unwrap();
        assert_eq!(
            prepared,
            Prepared {
                root: folder.clone(),
                staged: false,
                sidecars_written: 0,
                escaped: 0,
                left_behind: Vec::new(),
            }
        );
        assert!(listed(&staging).is_empty());
    }

    #[test]
    fn an_adf_is_staged_as_one_file_and_an_oversized_one_is_refused() {
        let (_guard, dir) = scratch("prepare-adf");
        let adf = dir.join("Workbench.adf");
        std::fs::write(&adf, vec![0x5Au8; 901_120]).unwrap();
        let staging = staging_in(&dir);
        let prepared = prepare(&adf, &SourceKind::Adf, &staging, &UtcClock, &NoProgress).unwrap();
        assert_eq!(listed(&staging), ["Workbench.adf"]);
        assert_eq!(
            std::fs::read(staging.join("Workbench.adf")).unwrap(),
            vec![0x5Au8; 901_120]
        );
        assert_eq!(prepared.escaped, 0);

        let (_guard2, dir2) = scratch("prepare-adf-big");
        let big = dir2.join("Huge.adf");
        std::fs::write(&big, vec![0u8; 4 * 1024 * 1024 + 1]).unwrap();
        let staging2 = staging_in(&dir2);
        let err = prepare(&big, &SourceKind::Adf, &staging2, &UtcClock, &NoProgress).unwrap_err();
        assert!(err.to_string().contains("Huge.adf"), "{err}");
        assert!(listed(&staging2).is_empty());
    }

    /// Every path under `root`, `/`-separated and relative to it.
    fn walked(root: &Path) -> Vec<String> {
        let mut found = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                let relative = path.strip_prefix(root).unwrap();
                found.push(relative.to_string_lossy().replace('\\', "/"));
                if path.is_dir() {
                    stack.push(path);
                }
            }
        }
        found.sort();
        found
    }

    /// The owner's own card material through classify, measure and prepare.
    /// `#[ignore]`d and env-gated: ART ships no copyrighted content. Any
    /// variable left unset skips its part.
    ///
    /// ```text
    /// cd src-tauri && \
    ///   ART_CARD_HDF_A="E:\amiga\Amigatolon\WHDload\HDF_Games_WHDLoad_by_Enzo_[A]\A Prehistoric Tale v1.1.hdf" \
    ///   ART_CARD_HDF_B="E:\amiga\Amigatolon\WHDload\HDF_Games_WHDLoad_by_Enzo_[B]\B-17 Flying Fortress v1.0.hdf" \
    ///   ART_CARD_COLLECTION="E:\amiga\Amigatolon\paketler\WHDLoadDemos100.lha" \
    ///   ART_CARD_LHA="E:\amiga\Amigatolon\paketler\BoingBag39-1.lha" \
    ///   cargo test --lib prepare_the_owners_card_material_when_asked -- --ignored --nocapture
    /// ```
    ///
    /// The hardfiles are copied into scratch first and prepared from the copy.
    /// B-17's scaffold carries a Kickstart ROM: the staging must hold neither
    /// its `Devs` nor any `kick*` file (R1 § 6). The collection is measured
    /// and never prepared — unpacked it is 917 MB (R1-10).
    #[test]
    #[ignore]
    fn prepare_the_owners_card_material_when_asked() {
        let var = |name: &str| std::env::var(name).ok().map(PathBuf::from);
        let (hdf_a, hdf_b, collection, lha) = (
            var("ART_CARD_HDF_A"),
            var("ART_CARD_HDF_B"),
            var("ART_CARD_COLLECTION"),
            var("ART_CARD_LHA"),
        );
        if hdf_a.is_none() && hdf_b.is_none() && collection.is_none() && lha.is_none() {
            eprintln!(
                "ART_CARD_HDF_A, ART_CARD_HDF_B, ART_CARD_COLLECTION and ART_CARD_LHA unset — skipping"
            );
            return;
        }

        for (label, source) in [("HDF_A", &hdf_a), ("HDF_B", &hdf_b)] {
            let Some(source) = source else {
                eprintln!("ART_CARD_{label} unset — skipping it");
                continue;
            };
            let (_guard, dir) = scratch(&format!("owners-{label}"));
            let copy = dir.join(source.file_name().unwrap());
            std::fs::copy(source, &copy).unwrap();
            let kind = match classify(&copy, &UtcClock) {
                Classified::Usable { kind } => kind,
                other => panic!("{}: {other:?}", source.display()),
            };
            assert_eq!(kind, SourceKind::WhdloadHardfile, "{}", source.display());
            let measured = measure(&copy, &kind, &UtcClock, &NoProgress).unwrap();
            let staging = staging_in(&dir);
            let prepared = prepare(&copy, &kind, &staging, &UtcClock, &NoProgress).unwrap();
            let tree = walked(&staging);
            println!(
                "{label} {}: top_level {:?} left_behind {:?} sidecars_written {} escaped {} staged {}",
                source.file_name().unwrap().to_string_lossy(),
                measured.top_level,
                prepared.left_behind,
                prepared.sidecars_written,
                prepared.escaped,
                tree.len()
            );
            assert_eq!(measured.left_behind, prepared.left_behind);
            assert_eq!(
                listed(&staging)
                    .iter()
                    .filter(|n| !n.ends_with(".uaem"))
                    .count(),
                measured.top_level.len()
            );
            if label == "HDF_B" {
                let leaf = |p: &String| p.rsplit('/').next().unwrap().to_ascii_lowercase();
                assert!(
                    !tree.iter().any(|p| leaf(p) == "devs"),
                    "B-17's staging holds a Devs: {tree:?}"
                );
                assert!(
                    !tree.iter().any(|p| leaf(p).starts_with("kick")),
                    "B-17's staging holds a kick* file: {tree:?}"
                );
                assert!(
                    prepared.left_behind.iter().any(|n| n == "Devs"),
                    "B-17's Devs should be named as left behind: {:?}",
                    prepared.left_behind
                );
            }
        }

        if let Some(collection) = &collection {
            let kind = match classify(collection, &UtcClock) {
                Classified::Usable { kind } => kind,
                other => panic!("{}: {other:?}", collection.display()),
            };
            let entries = crate::core::archive::open(collection)
                .unwrap()
                .entries()
                .unwrap();
            let placement = place_archive(&entries).unwrap();
            let ArchiveShape::Collection { drawers } = placement.shape else {
                panic!("{}: placed as {:?}", collection.display(), placement.shape);
            };
            let measured = measure(collection, &kind, &UtcClock, &NoProgress).unwrap();
            println!(
                "COLLECTION {}: entries {} drawers {drawers} top_level {:?} left_behind {:?}",
                collection.file_name().unwrap().to_string_lossy(),
                entries.len(),
                measured.top_level,
                measured.left_behind
            );
            assert!(drawers > 1, "a collection holds more than one drawer");
        }

        if let Some(lha) = &lha {
            let kind = match classify(lha, &UtcClock) {
                Classified::Usable { kind } => kind,
                other => panic!("{}: {other:?}", lha.display()),
            };
            let entries = crate::core::archive::open(lha).unwrap().entries().unwrap();
            let with_comment = entries.iter().filter(|e| e.amiga.comment.is_some()).count();
            let measured = measure(lha, &kind, &UtcClock, &NoProgress).unwrap();
            println!(
                "LHA {}: entries {} top_level {:?} comment-carrying entries {with_comment} \
                 left_behind {:?}",
                lha.file_name().unwrap().to_string_lossy(),
                entries.len(),
                measured.top_level,
                measured.left_behind
            );
            assert!(!measured.top_level.is_empty(), "{}", lha.display());
        }
    }
}

/// The bare-FFS-hardfile fixture builder [`tests::build_hdf`] already had, for
/// the other `core/cardos/` modules Tasks 6–10 add — the same fixture, not a
/// second copy of it that can drift out of step with what this module's own
/// tests already build.
///
/// A real wrapper `fn`, not a `pub(crate) use` re-export: nothing in this
/// tree calls it yet (Tasks 8, 10 and 13 are the callers), and a re-export
/// with no user is `unused_imports`, which `-D warnings` makes a build
/// failure — `lib.rs` allows only `dead_code`, and an unused wrapper `fn` is
/// exactly that, not an unused import.
#[cfg(test)]
pub(crate) mod test_support {
    use std::path::Path;

    pub(crate) fn build_hdf(path: &Path, dirs: &[&str], files: &[(&str, &[u8])]) {
        super::tests::build_hdf(path, dirs, files);
    }
}
