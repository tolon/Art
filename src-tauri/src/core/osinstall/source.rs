//! Reading install media, behind a trait — an ADF today, something else later
//! without `apply` (a future task) knowing which.
//!
//! ## Read-only on purpose
//!
//! `core::volume::write::VolumeWriter` would have handed this module `list`,
//! `find`, `read_file` and `attributes` in one type, but opening one means
//! opening the underlying file **for write** (`FileRegionMut`) even though
//! nothing here ever calls a mutating method. A user's install floppy image
//! is exactly the kind of file that gets archived read-only, and refusing to
//! even *look* at it because Windows will not hand out a write lock would be
//! a self-inflicted wound. So `AdfSource` reads through the plain, read-only
//! `FileRegion` `core::volume::mount::mount` returns, and calls the same
//! free functions (`write::dir`, `write::file`, `write::layout`) `VolumeWriter`
//! itself is built on — they are generic over `BlockDevice`, not
//! `BlockDeviceMut`, so nothing about them requires write access.
//!
//! ## Identified by what is inside, never by a filename
//!
//! [`AdfSource::open`] reads the volume name out of the root block. A
//! component names its media by that label (`Component::media`); a disk that
//! reached the user's media folder renamed `disk07.dat` still has to resolve.

use std::collections::HashSet;
use std::path::Path;

use crate::core::adf::bcpl::{read_bcpl_string, AmigaDate};
use crate::core::adf::blocks::RootBlock;
use crate::core::error::{CoreError, CoreResult};
use crate::core::layout::scan::MAX_SCAN_DEPTH;
use crate::core::volume::device::FileRegion;
use crate::core::volume::mount::{mount, scan_image, ImageLayout};
use crate::core::volume::write::layout::{
    self, BlockSet, BYTE_SIZE_OFFSET, COMMENT_OFFSET, DAYS_OFFSET, MINS_OFFSET, PROTECT_OFFSET,
    TICKS_OFFSET,
};
use crate::core::volume::write::{dir, file};
use crate::core::volume::{read_block_vec, VolumeGeometry};

/// One thing found on install media: a file or a drawer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaEntry {
    /// `/`-separated, relative to the media's own root. Never a leading
    /// slash, so it matches a [`super::PathRule::from`] value byte for byte.
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    /// `HSPARWED` exactly as AmigaDOS stores it, `RWED` inverted — ART's own
    /// convention throughout `core::volume`, not narrowed here. Render with
    /// [`crate::core::volume::write::uaem::format_bits`].
    pub protection: u32,
    pub date: AmigaDate,
    pub comment: String,
}

/// Something ART can pull install files out of: an image today, conceivably
/// a network mirror later — `apply` (a future task) is written against this
/// trait, not against `AdfSource`.
pub trait MediaSource {
    /// The volume name recorded **inside** the media — never the filename.
    fn volume_name(&self) -> &str;
    /// One entry, or `None` when `path` is not on this media. Absence is not
    /// an error: the caller decides whether a missing path is a refusal.
    fn entry(&mut self, path: &str) -> CoreResult<Option<MediaEntry>>;
    /// Every entry at or under `path`, `/`-separated and relative to the
    /// media's own root — not to `path`. `""` means the whole media, which
    /// is what a flat disk like `Fonts.adf` needs (its rule's `from` is
    /// `""`, because the whole image *is* the drawer).
    ///
    /// A `path` that is not on this media returns an empty `Vec` — the same
    /// answer a real, empty drawer would give. `walk` does not distinguish
    /// "missing" from "empty"; a caller that needs to tell them apart calls
    /// `entry` first.
    ///
    /// A `path` that names a **file**, though, is a different thing
    /// entirely and is refused with [`CoreError::InvalidInput`]: the caller
    /// asked for the contents of something that has no contents, and
    /// answering `Ok(vec![])` would make that mistake indistinguishable
    /// from an empty drawer and from a missing path — a silently short
    /// plan instead of a refusal (§89). This was left unstated once and two
    /// implementations promptly disagreed about it; `source_contract.rs`
    /// now asks every implementation the same questions.
    ///
    /// The **casing** of the returned paths is the media's own, not the
    /// caller's: a disc may hold two names differing only in case, and
    /// `entry` and `read` have to be able to reach both. Lookup is
    /// case-insensitive (AmigaDOS semantics) while these answers are not
    /// case-normalised, so a caller stripping a prefix off one of them must
    /// strip case-insensitively — `plan::relative_to` does, and a Joliet
    /// disc built the tree nested under itself for as long as it did not.
    /// The rule applies **whole**: the drawer's own segments as much as the
    /// names under them, and to [`entry`](Self::entry) as much as to this
    /// (M1 — `AdfSource` answered with the caller's spelling for both until
    /// the contract asked).
    ///
    /// **Containment is decided without regard to case, too.** What `path`
    /// names is an AmigaDOS drawer, and a medium that can hold `Libs/x`
    /// beside `libs/y` — an archive's flat name list, a Joliet tree — holds
    /// one drawer as far as AmigaDOS is concerned, so `walk` yields both.
    /// This was M2, and the two implementations that have a flat list to
    /// filter had argued it in opposite directions in their own files. The
    /// deciding argument is that the failure modes are not symmetrical:
    /// over-including is loud (the extra entries are in the plan, and a real
    /// clash raises `DestinationCollision`, which folds case itself), while
    /// under-including is silent — a short walk is a short plan is a file
    /// missing from the volume with nothing said about it (§89).
    /// [`starts_with_ignoring_case`] is the one test both use.
    fn walk(&mut self, path: &str) -> CoreResult<Vec<MediaEntry>>;
    /// A file's bytes. A `path` that is missing, or that names a drawer
    /// (including `""`, every media's own root), is refused with
    /// [`CoreError::InvalidInput`] rather than answered with bytes.
    fn read(&mut self, path: &str) -> CoreResult<Vec<u8>>;
}

/// Whether `path` sits under `prefix` (which always ends in `/`), compared
/// without regard to case.
///
/// **The one prefix test every `MediaSource` implementation that has a flat
/// list of paths to filter uses** — `CdSource::walk` and
/// `ArchiveSource::walk`, which each had their own and, until M2 of the
/// final whole-branch review, disagreed about the answer. `AdfSource` needs
/// none: it walks the volume's real directory tree, where containment is
/// structural rather than textual.
///
/// The rule and its reason are in [`MediaSource::walk`]'s own doc comment.
///
/// `get`, not a slice: `prefix.len()` may land mid-character in a multi-byte
/// path, which indexing would panic on and which cannot be a
/// case-insensitive ASCII match anyway. The same care `plan::relative_to`
/// takes for the same reason.
pub(super) fn starts_with_ignoring_case(path: &str, prefix: &str) -> bool {
    path.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// [`MediaSource`] for a floppy image — any bare AmigaDOS volume
/// `scan_image` finds, not assumed to be exactly 880 KB (ART-037, ART-038:
/// hard-coding DD geometry has cost this project twice already).
#[derive(Debug)]
pub struct AdfSource {
    device: FileRegion,
    geometry: VolumeGeometry,
    volume_name: String,
}

impl AdfSource {
    /// Open `image` and read the one volume inside it.
    ///
    /// Refuses anything `scan_image` did not identify as a single bare
    /// volume. Install media is a floppy or bare hardfile image — never a
    /// partitioned disk — and `volumes.first()` would otherwise silently
    /// treat an RDB's partition 0 as "the" volume, a quiet wrong answer
    /// rather than the loud one this project has filed issues over before.
    pub fn open(image: &Path) -> CoreResult<Self> {
        let scanned = scan_image(image)?;
        if scanned.layout != ImageLayout::BareVolume {
            let reason = match scanned.layout {
                ImageLayout::Rdb => {
                    "carries a partition table (RDB) — install media must be a single \
                     bare AmigaDOS volume, never a partitioned disk"
                }
                _ => "does not start with a recognisable AmigaDOS signature",
            };
            return Err(CoreError::UnsupportedFormat(format!(
                "'{}' {reason}",
                image.display()
            )));
        }
        let entry = scanned
            .volumes
            .first()
            .ok_or_else(|| CoreError::Malformed {
                format: "adf".into(),
                detail: "no volume found in this image".into(),
            })?;
        let (device, geometry) = mount(image, entry)?;

        // The label, read from the root block itself — not from `entry.name`,
        // which for a bare volume is `scan_image`'s fallback of the file's own
        // stem, exactly the filename this type must not trust.
        let root = read_block_vec(&device, geometry.root_block)?;
        let volume_name = RootBlock::parse(&root)?.volume_name;

        Ok(Self {
            device,
            geometry,
            volume_name,
        })
    }

    /// `path`, split into its non-empty `/`-separated segments — dropping a
    /// leading, trailing or doubled slash rather than treating one as a
    /// path component.
    fn segments(path: &str) -> impl Iterator<Item = &str> {
        path.split('/').filter(|s| !s.is_empty())
    }

    /// Walk `path` one segment at a time from the root, stopping the moment
    /// a segment is not found. `""` resolves to the root itself.
    ///
    /// Answers the block **and the volume's own spelling** of the path that
    /// reached it — every segment as `dir::find_entry` found it stored, not
    /// as the caller asked for it.
    ///
    /// That second half is M1 of the final whole-branch review, and it was
    /// the fourth `MediaSource` divergence: the trait doc has always said
    /// "the casing of the returned paths is the media's own, not the
    /// caller's", `CdSource` and `ArchiveSource` both did that, and this
    /// type answered `entry("c/loadmodule")` with `path: "c/loadmodule"`
    /// while the other two answered `"C/LoadModule"`. `walk` had the same
    /// shape one level down — its descendants carried the media's names but
    /// were prefixed with the caller's spelling of the drawer. Nothing in
    /// production reads `MediaEntry::path` off an `entry()` result today,
    /// which is exactly what made it survive; the contract now asks.
    ///
    /// `find_entry` already lowercases both sides to match (AmigaDOS is
    /// case-insensitive, ART-012), so the lookup is unchanged — only what
    /// is reported back is.
    fn resolve(&self, path: &str) -> CoreResult<Option<(u32, String)>> {
        let set = BlockSet::new(self.geometry.block_size);
        let mut current = self.geometry.root_block;
        let mut spelled: Vec<String> = Vec::new();
        for segment in Self::segments(path) {
            match dir::find_entry(&self.device, &set, &self.geometry, current, segment)? {
                Some(found) => {
                    current = found.block;
                    spelled.push(found.name);
                }
                None => return Ok(None),
            }
        }
        Ok(Some((current, spelled.join("/"))))
    }

    /// Whether `block` is a directory header.
    ///
    /// `walk` and `read` both need this before doing anything with a block
    /// `resolve` handed back: a `kind: "file"` rule aimed at a drawer, or a
    /// `Subtree` rule aimed at a file, is a wrong recipe or a wrong media
    /// revision, and either way must be refused by name rather than quietly
    /// reading the wrong shape of block (a file's data-block pointer list
    /// read as if it were a directory's hash table, or a directory's unused
    /// size field read as if it were a file's).
    fn is_directory_block(&self, block: u32) -> CoreResult<bool> {
        if block >= self.geometry.total_blocks {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!("block {block} is outside this volume"),
            });
        }
        let header = read_block_vec(&self.device, block)?;
        dir::is_directory(&header)
    }

    /// The media's own root, as a [`MediaEntry`] — never read off the root
    /// block itself.
    ///
    /// The root block reuses `PROTECT_OFFSET`/`BYTE_SIZE_OFFSET`/
    /// `COMMENT_OFFSET` for its bitmap-page table: those bytes are real
    /// block pointers, not protection bits or a comment, on every root block
    /// that has ever been formatted. Reading them anyway is worse than an
    /// error, because the result is bounds-safe and plausible-looking — a
    /// `MediaEntry` an `apply` step could carry straight into a `.uaem`
    /// sidecar and never notice it was never AmigaDOS metadata at all. So the
    /// root is its own case, not a block read: `protection: 0` (`----rwed`)
    /// and an empty `date`/`comment` are **declared defaults**, not a
    /// reading — the only field `RootBlock::parse` actually reads correctly
    /// off the root (its `rDate`, at the same `DAYS_OFFSET`/`MINS_OFFSET`/
    /// `TICKS_OFFSET` this type otherwise reads for a file or directory) is
    /// deliberately left unused here too, so a `MediaEntry` never mixes one
    /// genuinely-read field with two fabricated ones.
    ///
    /// `path` is `""`, matching what [`Self::normalized`] already collapses
    /// an all-slashes path to, and matching a `Subtree` rule's own
    /// `from: ""` — the only rule shape that ever names the root.
    fn root_entry(&self) -> MediaEntry {
        MediaEntry {
            path: String::new(),
            is_dir: true,
            size: 0,
            protection: 0,
            date: AmigaDate {
                days: 0,
                mins: 0,
                ticks: 0,
            },
            comment: String::new(),
        }
    }

    /// Build a [`MediaEntry`] for `block`, already known to sit at `path`.
    ///
    /// Reads the protection, comment and date fields straight off the header
    /// block rather than through `VolumeWriter::attributes` — the same
    /// fields, at the same offsets, without needing a `BlockDeviceMut`. Only
    /// ever called with a real file or directory header: `walk`'s recursion
    /// reaches this through `dir::entries_in`, which yields a directory's
    /// *children*, never the directory being asked about, and `entry`
    /// special-cases the root itself ([`Self::root_entry`]) before this is
    /// reached at all. Never call this with `self.geometry.root_block`.
    fn entry_at(&self, path: &str, block: u32) -> CoreResult<MediaEntry> {
        if block >= self.geometry.total_blocks {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!("block {block} is outside this volume"),
            });
        }
        let header = read_block_vec(&self.device, block)?;
        let is_dir = dir::is_directory(&header)?;
        let size = if is_dir {
            0
        } else {
            layout::get_u32(&header, BYTE_SIZE_OFFSET)? as u64
        };

        Ok(MediaEntry {
            path: path.to_string(),
            is_dir,
            size,
            protection: layout::get_u32(&header, PROTECT_OFFSET)?,
            date: AmigaDate {
                days: layout::get_u32(&header, DAYS_OFFSET)?,
                mins: layout::get_u32(&header, MINS_OFFSET)?,
                ticks: layout::get_u32(&header, TICKS_OFFSET)?,
            },
            comment: read_bcpl_string(&header, COMMENT_OFFSET).unwrap_or_default(),
        })
    }

    /// `walk`'s recursion, one directory level at a time.
    ///
    /// Bounded three separate ways, because depth alone does not bound the
    /// work a malformed image can force: a directory that is its own
    /// ancestor with branching factor `b` would push up to `b`^`MAX_SCAN_DEPTH`
    /// entries into `out` before a depth cap ever fired — an OOM under
    /// `panic = "abort"`, the exact crash a cap is supposed to prevent. This
    /// is settled project policy, not a judgement call made fresh here:
    /// `core/cbm/d64.rs`'s module doc states it outright ("every chain has a
    /// step limit *and* a visited set... a step limit alone lets a cycle run
    /// to the limit on every file; a visited set alone lets a long enough
    /// chain run for a long time. Both, or neither."), and
    /// `core/adf/fs.rs::walk_and_count_on` keeps exactly this shape of
    /// `HashSet<u32>` over exactly this shape of walk.
    ///
    /// - `depth`, capped at [`MAX_SCAN_DEPTH`] with the same `>=` `core::layout::scan`
    ///   uses — the release profile's `panic = "abort"` turns unbounded
    ///   recursion into the whole application going down, not a slow leak.
    /// - `visited`, a `HashSet` of directory blocks already walked in *this*
    ///   call — the moment a directory would be entered twice, this media is
    ///   not a tree, and reporting that beats looping (or blowing up) trying
    ///   to prove it harder.
    /// - a total-entry cap: no volume can legitimately hold more entries than
    ///   it has blocks, since every entry needs at least one block of its
    ///   own. This is the guard that actually stops the branching-factor
    ///   case the visited set does not — a hostile image can alias the same
    ///   *file* block into many distinct, otherwise-unvisited directories
    ///   without ever repeating a directory block.
    ///
    /// The hash chain each `entries_in` call walks is bounded on its own
    /// terms too (`dir::MAX_CHAIN_STEPS`); this bounds the tree shape on top
    /// of that.
    fn walk_dir(
        &self,
        dir_block: u32,
        prefix: &str,
        depth: usize,
        visited: &mut HashSet<u32>,
        out: &mut Vec<MediaEntry>,
    ) -> CoreResult<()> {
        if depth >= MAX_SCAN_DEPTH {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!("this media is nested deeper than {MAX_SCAN_DEPTH} levels"),
            });
        }
        if !visited.insert(dir_block) {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!("directory block {dir_block} is its own ancestor"),
            });
        }

        let set = BlockSet::new(self.geometry.block_size);
        for found in dir::entries_in(&self.device, &set, &self.geometry, dir_block)? {
            if out.len() >= self.geometry.total_blocks as usize {
                return Err(CoreError::Malformed {
                    format: "volume".into(),
                    detail: format!(
                        "this media lists more entries than it has blocks ({})",
                        self.geometry.total_blocks
                    ),
                });
            }
            let path = if prefix.is_empty() {
                found.name.clone()
            } else {
                format!("{prefix}/{}", found.name)
            };
            let entry = self.entry_at(&path, found.block)?;
            let is_dir = entry.is_dir;
            out.push(entry);
            if is_dir {
                self.walk_dir(found.block, &path, depth + 1, visited, out)?;
            }
        }
        Ok(())
    }

    /// `path`, normalised the way [`Self::segments`] sees it — no leading,
    /// trailing or doubled slash — so a path built by joining two rules
    /// never carries one into a result.
    fn normalized(path: &str) -> String {
        Self::segments(path).collect::<Vec<_>>().join("/")
    }
}

impl MediaSource for AdfSource {
    fn volume_name(&self) -> &str {
        &self.volume_name
    }

    fn entry(&mut self, path: &str) -> CoreResult<Option<MediaEntry>> {
        if Self::normalized(path).is_empty() {
            return Ok(Some(self.root_entry()));
        }
        // `spelled`, never the caller's `normalized` — see `resolve`.
        let Some((block, spelled)) = self.resolve(path)? else {
            return Ok(None);
        };
        Ok(Some(self.entry_at(&spelled, block)?))
    }

    fn walk(&mut self, path: &str) -> CoreResult<Vec<MediaEntry>> {
        let Some((block, spelled)) = self.resolve(path)? else {
            return Ok(Vec::new());
        };
        if !self.is_directory_block(block)? {
            return Err(CoreError::InvalidInput(format!(
                "'{path}' is a file on this media, not a drawer"
            )));
        }
        let mut out = Vec::new();
        let mut visited = HashSet::new();
        // The prefix is the volume's own spelling of the drawer, so every
        // path in the answer is the media's from end to end — the
        // descendants' names always were (`walk_dir` reads `found.name`),
        // and now the part in front of them is too (M1).
        self.walk_dir(block, &spelled, 0, &mut visited, &mut out)?;
        Ok(out)
    }

    fn read(&mut self, path: &str) -> CoreResult<Vec<u8>> {
        let Some((block, _)) = self.resolve(path)? else {
            return Err(CoreError::InvalidInput(format!(
                "'{path}' is not on this media"
            )));
        };
        if self.is_directory_block(block)? {
            return Err(CoreError::InvalidInput(format!(
                "'{path}' is a drawer on this media, not a file"
            )));
        }
        let set = BlockSet::new(self.geometry.block_size);
        file::read_file(&self.device, &set, &self.geometry, block)
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use crate::core::hdf::create_hdf;
    use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};
    use crate::core::volume::device::FileRegionMut;
    use crate::core::volume::write::{FileMeta, VolumeWriter};
    use crate::core::volume::DosType;

    /// The blank-ADF geometry every hand-spliced test below needs to compute
    /// a byte offset into the file — the same geometry `fixtures::media`
    /// itself formats with.
    fn dd_geometry() -> VolumeGeometry {
        VolumeGeometry::floppy_dd(DosType::new(*b"DOS\x01"))
    }

    #[test]
    fn a_source_reports_the_volume_name_from_inside_the_image() {
        let dir = super::super::fixtures::scratch("source-volume-name");
        // Deliberately a filename that says nothing, and a volume that is
        // not `Workbench3.2` — `fixtures::workbench` bakes that name in, and
        // this is the one test that needs to choose its own.
        let image = super::super::fixtures::media(
            &dir,
            "ModulesA1200_3.2",
            "ModulesA1200_3.2.adf",
            &[("C/LoadModule", b"cmd", 0x20)],
        );
        let renamed = dir.join("disk07.dat");
        std::fs::rename(&image, &renamed).unwrap();

        let source = AdfSource::open(&renamed).unwrap();
        assert_eq!(source.volume_name(), "ModulesA1200_3.2");
    }

    #[test]
    fn an_entry_carries_the_protection_bits_the_media_holds() {
        let dir = super::super::fixtures::scratch("source-protection");
        let mut source = AdfSource::open(&super::super::fixtures::workbench(&dir)).unwrap();

        let load_module = source.entry("C/LoadModule").unwrap().unwrap();
        assert!(!load_module.is_dir);
        assert_eq!(
            crate::core::volume::write::uaem::format_bits(load_module.protection),
            "--p-rwed",
            "the pure bit is load-bearing: 3.2's Startup-Sequence runs \
             `Resident C:Assign PURE` and fails without it"
        );

        // `LoadModule`'s `0x20` grants all of RWED, so on its own it cannot
        // tell an inverted bit from a non-inverted one — flip every bit and
        // it would still read as granted. `Startup-sequence`'s `0x42` is the
        // fixture's only case where a *denied* bit (`w`) has to survive, and
        // RWED-inversion is the single easiest thing in this format to get
        // backwards.
        let startup = source.entry("S/Startup-sequence").unwrap().unwrap();
        assert_eq!(
            crate::core::volume::write::uaem::format_bits(startup.protection),
            "-s--rw-d",
            "0x42 is the fixture's only denied-bit case; getting RWED's \
             inversion backwards would still pass the LoadModule assertion"
        );
    }

    #[test]
    fn a_missing_path_is_none_rather_than_an_error() {
        let dir = super::super::fixtures::scratch("source-missing");
        let mut source = AdfSource::open(&super::super::fixtures::workbench(&dir)).unwrap();
        assert!(source.entry("LIBS/Modules").unwrap().is_none());
    }

    #[test]
    fn walk_returns_a_subtree_with_paths_relative_to_the_media_root() {
        let dir = super::super::fixtures::scratch("source-walk");
        let mut source = AdfSource::open(&super::super::fixtures::workbench(&dir)).unwrap();
        let found = source.walk("C").unwrap();
        assert!(found.iter().any(|e| e.path == "C/LoadModule"));
    }

    #[test]
    fn read_returns_the_bytes() {
        let dir = super::super::fixtures::scratch("source-read");
        let mut source = AdfSource::open(&super::super::fixtures::workbench(&dir)).unwrap();
        assert_eq!(source.read("S/Startup-sequence").unwrap(), b"; test\n");
    }

    /// `entry_at` reads `size`, `date` and `comment` off the header block
    /// too, and none of the brief's own tests (or the ones above, which only
    /// pin `protection`) touch any of the three. `size` in particular has a
    /// track record — ART-034 is `BYTE_SIZE_OFFSET` being 324, not 316, one
    /// longword off. Built directly with `VolumeWriter` rather than
    /// `fixtures::media`, because `FileMeta` carries no comment field at
    /// all; `set_attributes` is the real, independently-tested path
    /// AmigaDOS metadata goes through, so this proves `entry_at` reads back
    /// what that path actually wrote, not a hand-spliced approximation of
    /// it.
    #[test]
    fn an_entry_carries_its_size_date_and_comment() {
        let dir = super::super::fixtures::scratch("source-size-date-comment");
        let image = super::super::fixtures::media(&dir, "Meta", "meta.adf", &[]);
        let geometry = dd_geometry();

        let stamped = AmigaDate {
            days: 17_752, // 2026-08-09 — the reference date `uaem.rs`'s own tests use
            mins: 9 * 60 + 5,
            ticks: 0,
        };
        {
            let mut device =
                FileRegionMut::open(&image, 0, geometry.total_bytes(), geometry.block_size)
                    .unwrap();
            let mut writer = VolumeWriter::open(&mut device, geometry, &image, 0).unwrap();
            let added = writer
                .add_file(0, "Payload.bin", b"abc", FileMeta::default())
                .unwrap();
            writer
                .set_attributes(
                    added.block.unwrap(),
                    None,
                    Some("a real AmigaDOS comment"),
                    Some(stamped),
                )
                .unwrap();
        }

        let mut source = AdfSource::open(&image).unwrap();
        let entry = source.entry("Payload.bin").unwrap().unwrap();

        assert_eq!(
            entry.size, 3,
            "b\"abc\" is 3 bytes — ART-034 got this exact offset wrong once already"
        );
        assert_eq!(entry.date, stamped);
        assert_eq!(entry.comment, "a real AmigaDOS comment");
    }

    // ---- `from: ""`: the media's own root (decision 3) ----

    /// `Fonts.adf` and `Backdrops3.2.adf` are flat disks — every rule that
    /// names them uses `from: ""`, meaning "walk the whole media", and that
    /// has no coverage in the brief's own five tests. A flat disk has no `C`
    /// or `S` drawer to walk instead, so this fixture is its own, matching
    /// what those two components actually look like: files sitting directly
    /// in the root.
    #[test]
    fn walking_the_empty_path_returns_the_whole_media() {
        let dir = super::super::fixtures::scratch("source-root-walk");
        let image = super::super::fixtures::media(
            &dir,
            "Fonts",
            "fonts.adf",
            &[
                ("topaz.font", b"font data", 0x00),
                ("topaz/8", b"glyphs", 0x00),
            ],
        );
        let mut source = AdfSource::open(&image).unwrap();

        let found = source.walk("").unwrap();
        let paths: Vec<&str> = found.iter().map(|e| e.path.as_str()).collect();

        assert!(paths.contains(&"topaz.font"), "{paths:?}");
        assert!(paths.contains(&"topaz"), "{paths:?}");
        assert!(paths.contains(&"topaz/8"), "{paths:?}");
        // No entry is rooted with a leading slash — `""` is not itself a
        // path component.
        assert!(paths.iter().all(|p| !p.starts_with('/')), "{paths:?}");
    }

    /// `entry("")` is the same question about a single thing rather than a
    /// subtree: it must resolve to the root and say it is a directory,
    /// without erroring on the root block's different layout.
    #[test]
    fn an_empty_path_entry_resolves_to_the_root_directory() {
        let dir = super::super::fixtures::scratch("source-root-entry");
        let mut source = AdfSource::open(&super::super::fixtures::workbench(&dir)).unwrap();

        let entry = source.entry("").unwrap().unwrap();
        assert!(entry.is_dir);
        assert_eq!(entry.path, "");
    }

    /// The defect the coordinator caught in review: `PROTECT_OFFSET`,
    /// `COMMENT_OFFSET` and the date fields are real offsets into the root
    /// block, but on a root block they hold its bitmap-page table and its
    /// own `rDate`, not the protection/comment/date of an entry. Before
    /// [`AdfSource::root_entry`] existed, `entry("")` read straight through
    /// to whatever was there — bounds-safe, and completely invented for two
    /// of the three, genuine-but-mismatched for the third. This splices
    /// something a bug would read back verbatim into all three fields, and
    /// pins that none of it comes back: it fails against the block-read
    /// version of `entry_at` this test used to go through.
    #[test]
    fn an_empty_path_entry_does_not_read_the_root_blocks_own_fields_as_metadata() {
        let dir = super::super::fixtures::scratch("source-root-no-fabrication");
        let image = super::super::fixtures::workbench(&dir);
        let geometry = dd_geometry();
        let root_offset = geometry.root_block as usize * geometry.block_size;

        let mut raw = std::fs::read(&image).unwrap();
        // `PROTECT_OFFSET`: a bitmap-page pointer that would read back as a
        // very real-looking, very wrong, protection value if it were ever
        // treated as one.
        let protect_slot = root_offset + layout::PROTECT_OFFSET;
        raw[protect_slot..protect_slot + 4].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
        // `COMMENT_OFFSET`: a BSTR claiming 20 bytes of text that is not a
        // comment — it is more of the same bitmap table.
        let comment_slot = root_offset + layout::COMMENT_OFFSET;
        raw[comment_slot] = 20;
        raw[comment_slot + 1..comment_slot + 21].copy_from_slice(b"not really a comment");
        // `DAYS_OFFSET`/`MINS_OFFSET`/`TICKS_OFFSET`: this one *is* really
        // the root's own alteration date (`rDate`), not garbage — and
        // `root_entry` deliberately does not read it either, so a genuine
        // field is not mixed into a struct with two fabricated ones.
        for (offset, value) in [
            (layout::DAYS_OFFSET, 12_345u32),
            (layout::MINS_OFFSET, 678u32),
            (layout::TICKS_OFFSET, 90u32),
        ] {
            let slot = root_offset + offset;
            raw[slot..slot + 4].copy_from_slice(&value.to_be_bytes());
        }
        std::fs::write(&image, &raw).unwrap();

        let mut source = AdfSource::open(&image).unwrap();
        let entry = source.entry("").unwrap().unwrap();

        assert!(entry.is_dir);
        assert_eq!(
            entry.protection, 0,
            "the root's bitmap-page table must never be read as protection bits"
        );
        assert_eq!(
            entry.comment, "",
            "the root's bitmap-page table must never be read as a comment"
        );
        assert_eq!(
            entry.date,
            AmigaDate {
                days: 0,
                mins: 0,
                ticks: 0
            },
            "the root's own alteration date must not leak into a declared default"
        );
    }

    // ---- decision 4: a chain walk needs a step limit — and a visited set ----

    /// The Critical finding from review: a depth cap alone does not bound
    /// the work a malformed image can force. A directory that is its own
    /// ancestor with branching factor `b` pushes up to `b^MAX_SCAN_DEPTH`
    /// entries into `out` before a depth cap ever fires — an OOM under
    /// `panic = "abort"`, the exact crash the cap exists to prevent. This
    /// splices **two** buckets of the root's own hash table to point at the
    /// root block, not one — the shape that would have blown up
    /// combinatorially without a visited set (the single-bucket version
    /// merely walked a straight line down to `MAX_SCAN_DEPTH`, which even an
    /// unbounded recursion would have survived long enough to look like it
    /// passed). With the visited set in place, both branches are moot: the
    /// very first recursive step re-enters the root and is refused
    /// immediately, regardless of how many children it has.
    #[test]
    fn a_directory_that_branches_into_itself_is_refused_not_exploded() {
        let dir = super::super::fixtures::scratch("source-branch-cycle");
        let image = super::super::fixtures::media(&dir, "Loop", "loop.adf", &[]);
        let geometry = dd_geometry();
        let root_offset = geometry.root_block as usize * geometry.block_size;

        let mut raw = std::fs::read(&image).unwrap();
        for bucket in 0..2usize {
            let slot = root_offset + layout::TABLE_OFFSET + bucket * 4;
            raw[slot..slot + 4].copy_from_slice(&geometry.root_block.to_be_bytes());
        }
        std::fs::write(&image, &raw).unwrap();

        let mut source = AdfSource::open(&image).unwrap();
        let err = source.walk("").unwrap_err();
        assert!(err.to_string().contains("its own ancestor"), "{err}");
    }

    /// A single bucket's own chain, made to loop by pointing one already-
    /// written file block's `NEXT_HASH_OFFSET` at itself — the route the
    /// coordinator's second review round identified as untouched by the
    /// visited set (which is keyed on *directory* blocks; this loop never
    /// recurses, since every entry in it is a file) and by the depth cap
    /// (which never advances, for the same reason).
    ///
    /// Measured, not assumed: this refuses, but **not** through
    /// `walk_dir`'s total-entry cap. `dir::entries_in` walks a bucket's
    /// chain with its own pre-existing bound, `MAX_CHAIN_STEPS` (65,536,
    /// `core/volume/write/dir.rs`) — a limit older than this task and far
    /// above any floppy's own block count (1,760 on a DD disk), so it always
    /// fires first for a same-*bucket* loop, on any volume, and returns its
    /// own `"hash bucket {n} does not end"` before `entries_in` ever hands a
    /// `Vec` back to `walk_dir` for the entry cap to look at. The assertion
    /// below is that exact message, verified by running this splice and
    /// reading back what actually came out, not assumed to be the entry cap
    /// because the request that prompted this test named it. The entry
    /// cap's own genuine, uncontested job — the one nothing else in this
    /// file touches — is the *next* test: the same reused block aliased
    /// across many distinct, never-repeated *directories*, where no single
    /// bucket ever loops and no directory is ever revisited.
    #[test]
    fn a_bucket_that_loops_on_itself_is_refused_by_the_chain_step_limit() {
        let dir = super::super::fixtures::scratch("source-bucket-loop");
        let image = super::super::fixtures::media(
            &dir,
            "Bucket",
            "bucket.adf",
            &[("Payload.bin", b"x", 0x00)],
        );
        let geometry = dd_geometry();

        let payload_block = {
            let source = AdfSource::open(&image).unwrap();
            source.resolve("Payload.bin").unwrap().unwrap().0
        };

        let mut raw = std::fs::read(&image).unwrap();
        let next_hash_slot =
            payload_block as usize * geometry.block_size + layout::NEXT_HASH_OFFSET;
        raw[next_hash_slot..next_hash_slot + 4].copy_from_slice(&payload_block.to_be_bytes());
        std::fs::write(&image, &raw).unwrap();

        let mut source = AdfSource::open(&image).unwrap();
        let err = source.walk("").unwrap_err();
        assert!(
            err.to_string().contains("does not end"),
            "expected dir::entries_in's own chain-step limit, got: {err}"
        );
    }

    /// The total-entry cap's own, uncontested route: the same file block
    /// aliased into `total_blocks`-many distinct directories, none of them
    /// ever revisited (so the visited set never fires) and no single
    /// directory's own hash-table bucket ever chained past one hop (so
    /// `dir::entries_in`'s chain-step limit never fires either). That is a
    /// real fixture — dozens of genuine directories, each one legitimate on
    /// its own — larger than either of the other two guards' fixtures
    /// needed to be, for a property that is otherwise obvious once stated:
    /// no volume can legitimately hold more entries than it has blocks.
    ///
    /// So this proves the guard the cheap way instead: `walk_dir` is a
    /// private method of this same module, reachable directly from here.
    /// Seeding `out` at exactly `total_blocks` entries before calling it on
    /// a real, ordinary directory reproduces the one fact that matters —
    /// the cap refuses the very next legitimate entry rather than growing
    /// `out` past what the volume could ever really hold — without needing
    /// the large fixture that would be the only way to reach the same
    /// check from `MediaSource::walk`.
    #[test]
    fn the_total_entry_cap_refuses_one_more_entry_once_out_is_already_full() {
        let dir = super::super::fixtures::scratch("source-total-entry-cap");
        let image = super::super::fixtures::workbench(&dir);
        let source = AdfSource::open(&image).unwrap();

        let mut out = vec![source.root_entry(); source.geometry.total_blocks as usize];
        let mut visited = HashSet::new();
        let err = source
            .walk_dir(source.geometry.root_block, "", 0, &mut visited, &mut out)
            .unwrap_err();
        assert!(
            err.to_string().contains("more entries than it has blocks"),
            "{err}"
        );
    }

    /// The depth cap on its own terms, independent of the cycle guard above:
    /// a real, non-cyclic chain of distinct directories nested past
    /// `MAX_SCAN_DEPTH` must still be refused. The visited set does not fire
    /// here — every directory in the chain is visited exactly once — so this
    /// is the test that would fail if the depth cap were ever quietly
    /// dropped in favour of "the visited set catches everything now".
    #[test]
    fn a_real_directory_chain_deeper_than_the_cap_is_refused() {
        let dir = super::super::fixtures::scratch("source-depth-cap-real");
        let image = super::super::fixtures::media(&dir, "Deep", "deep.adf", &[]);
        let geometry = dd_geometry();
        {
            let mut device =
                FileRegionMut::open(&image, 0, geometry.total_bytes(), geometry.block_size)
                    .unwrap();
            let mut writer = VolumeWriter::open(&mut device, geometry, &image, 0).unwrap();
            let mut parent = 0u32;
            for _ in 0..MAX_SCAN_DEPTH + 2 {
                parent = writer.make_dir(parent, "D").unwrap().block.unwrap();
            }
        }

        let mut source = AdfSource::open(&image).unwrap();
        let err = source.walk("").unwrap_err();
        assert!(err.to_string().contains("deeper than"), "{err}");
    }

    // ---- decision 4b: `walk` and `read` refuse the wrong kind of block ----

    /// `walk("C/LoadModule")` would otherwise run `entries_in` over a *file*
    /// header, reading its data-block pointer list as if it were a
    /// directory's hash table — bounds-checked, so not a crash, but wrong: a
    /// `Subtree` rule aimed at a file by mistake must be refused by name,
    /// not silently return whatever garbage-shaped "entries" fall out of a
    /// pointer list.
    #[test]
    fn walk_refuses_a_path_that_names_a_file() {
        let dir = super::super::fixtures::scratch("source-walk-wrong-kind");
        let mut source = AdfSource::open(&super::super::fixtures::workbench(&dir)).unwrap();

        let err = source.walk("C/LoadModule").unwrap_err();
        assert!(err.to_string().contains("not a drawer"), "{err}");
    }

    /// `read("C")` would otherwise read `BYTE_SIZE_OFFSET` off a userdir
    /// header — always `0`, since that field is unused there — and hand back
    /// `Ok(vec![])`: a silently "successful" zero-byte read of a directory.
    /// A `kind: "file"` rule aimed at a drawer by mistake must be refused,
    /// not produce a file nobody asked for.
    #[test]
    fn read_refuses_a_path_that_names_a_drawer() {
        let dir = super::super::fixtures::scratch("source-read-wrong-kind");
        let mut source = AdfSource::open(&super::super::fixtures::workbench(&dir)).unwrap();

        let err = source.read("C").unwrap_err();
        assert!(err.to_string().contains("not a file"), "{err}");
    }

    // ---- promoted minor: `open` refuses a partitioned image by name ----

    /// `open` used to take `scanned.volumes.first()` unconditionally, which
    /// for an RDB image is partition 0 — a quiet wrong answer for install
    /// media, which is always a single bare volume. `AdfSource::open` is
    /// public API, so this is refused by name rather than silently treated
    /// as "the" volume.
    #[test]
    fn open_refuses_a_partitioned_image() {
        let dir = super::super::fixtures::scratch("source-refuses-rdb");
        let path = dir.join("disk.hdf");
        create_hdf(
            &path,
            32 * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::FfsStandard,
                size_mb: 10,
                bootable: true,
                boot_priority: 0,
                num_buffers: 100,
            }],
            &[],
        )
        .unwrap();

        let err = AdfSource::open(&path).unwrap_err();
        assert!(err.to_string().contains("partition table"), "{err}");
    }
}
