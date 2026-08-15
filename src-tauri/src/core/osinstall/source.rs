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

use std::path::Path;

use crate::core::adf::bcpl::{read_bcpl_string, AmigaDate};
use crate::core::adf::blocks::RootBlock;
use crate::core::error::{CoreError, CoreResult};
use crate::core::layout::scan::MAX_SCAN_DEPTH;
use crate::core::volume::device::FileRegion;
use crate::core::volume::mount::{mount, scan_image};
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
    fn walk(&mut self, path: &str) -> CoreResult<Vec<MediaEntry>>;
    /// A file's bytes.
    fn read(&mut self, path: &str) -> CoreResult<Vec<u8>>;
}

/// [`MediaSource`] for a floppy image — any bare AmigaDOS volume
/// `scan_image` finds, not assumed to be exactly 880 KB (ART-037, ART-038:
/// hard-coding DD geometry has cost this project twice already).
pub struct AdfSource {
    device: FileRegion,
    geometry: VolumeGeometry,
    volume_name: String,
}

impl AdfSource {
    /// Open `image` and read the one volume inside it.
    pub fn open(image: &Path) -> CoreResult<Self> {
        let scanned = scan_image(image)?;
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
    fn resolve(&self, path: &str) -> CoreResult<Option<u32>> {
        let set = BlockSet::new(self.geometry.block_size);
        let mut current = self.geometry.root_block;
        for segment in Self::segments(path) {
            match dir::find_entry(&self.device, &set, &self.geometry, current, segment)? {
                Some(found) => current = found.block,
                None => return Ok(None),
            }
        }
        Ok(Some(current))
    }

    /// Build a [`MediaEntry`] for `block`, already known to sit at `path`.
    ///
    /// Reads the protection, comment and date fields straight off the header
    /// block rather than through `VolumeWriter::attributes` — the same
    /// fields, at the same offsets, without needing a `BlockDeviceMut`. Only
    /// meaningful for a real file or directory header; `walk`'s recursion
    /// never calls this for the root itself (root entries come from
    /// `dir::entries_in`, which never yields the directory it was asked
    /// about), so the root's differently-shaped block never reaches it.
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
    /// `depth` is capped at [`MAX_SCAN_DEPTH`] — the same limit
    /// `core::layout::scan` uses, for the same reason: a malformed image can
    /// make a directory its own descendant, and the release profile's
    /// `panic = "abort"` turns an unbounded recursion into the whole
    /// application going down rather than an error coming back. The hash
    /// chain each `entries_in` call walks is bounded on its own terms
    /// (`dir::MAX_CHAIN_STEPS`); this bounds the tree shape on top of that.
    fn walk_dir(
        &self,
        dir_block: u32,
        prefix: &str,
        depth: usize,
        out: &mut Vec<MediaEntry>,
    ) -> CoreResult<()> {
        if depth > MAX_SCAN_DEPTH {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!("this media is nested deeper than {MAX_SCAN_DEPTH} levels"),
            });
        }

        let set = BlockSet::new(self.geometry.block_size);
        for found in dir::entries_in(&self.device, &set, &self.geometry, dir_block)? {
            let path = if prefix.is_empty() {
                found.name.clone()
            } else {
                format!("{prefix}/{}", found.name)
            };
            let entry = self.entry_at(&path, found.block)?;
            let is_dir = entry.is_dir;
            out.push(entry);
            if is_dir {
                self.walk_dir(found.block, &path, depth + 1, out)?;
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
        let Some(block) = self.resolve(path)? else {
            return Ok(None);
        };
        Ok(Some(self.entry_at(&Self::normalized(path), block)?))
    }

    fn walk(&mut self, path: &str) -> CoreResult<Vec<MediaEntry>> {
        let Some(block) = self.resolve(path)? else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        self.walk_dir(block, &Self::normalized(path), 0, &mut out)?;
        Ok(out)
    }

    fn read(&mut self, path: &str) -> CoreResult<Vec<u8>> {
        let Some(block) = self.resolve(path)? else {
            return Err(CoreError::InvalidInput(format!(
                "'{path}' is not on this media"
            )));
        };
        let set = BlockSet::new(self.geometry.block_size);
        file::read_file(&self.device, &set, &self.geometry, block)
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    /// Build a synthetic ADF holding `C/LoadModule` and `S/Startup-sequence`.
    /// ART ships no Amiga content; every fixture is made here, now.
    fn fixture(dir: &Path, volume: &str) -> PathBuf {
        super::super::fixtures::media(
            dir,
            volume,
            &format!("{volume}.adf"),
            &[
                ("C/LoadModule", b"cmd", 0x20),            // --p-rwed
                ("S/Startup-sequence", b"; test\n", 0x42), // -s--rw-d
            ],
        )
    }

    #[test]
    fn a_source_reports_the_volume_name_from_inside_the_image() {
        let dir = super::super::fixtures::scratch("source-volume-name");
        // Deliberately a filename that says nothing.
        let image = fixture(&dir, "ModulesA1200_3.2");
        let renamed = dir.join("disk07.dat");
        std::fs::rename(&image, &renamed).unwrap();

        let source = AdfSource::open(&renamed).unwrap();
        assert_eq!(source.volume_name(), "ModulesA1200_3.2");
    }

    #[test]
    fn an_entry_carries_the_protection_bits_the_media_holds() {
        let dir = super::super::fixtures::scratch("source-protection");
        let mut source = AdfSource::open(&fixture(&dir, "Workbench3.2")).unwrap();

        let entry = source.entry("C/LoadModule").unwrap().unwrap();
        assert!(!entry.is_dir);
        assert_eq!(
            crate::core::volume::write::uaem::format_bits(entry.protection),
            "--p-rwed",
            "the pure bit is load-bearing: 3.2's Startup-Sequence runs \
             `Resident C:Assign PURE` and fails without it"
        );
    }

    #[test]
    fn a_missing_path_is_none_rather_than_an_error() {
        let dir = super::super::fixtures::scratch("source-missing");
        let mut source = AdfSource::open(&fixture(&dir, "Workbench3.2")).unwrap();
        assert!(source.entry("LIBS/Modules").unwrap().is_none());
    }

    #[test]
    fn walk_returns_a_subtree_with_paths_relative_to_the_media_root() {
        let dir = super::super::fixtures::scratch("source-walk");
        let mut source = AdfSource::open(&fixture(&dir, "Workbench3.2")).unwrap();
        let found = source.walk("C").unwrap();
        assert!(found.iter().any(|e| e.path == "C/LoadModule"));
    }

    #[test]
    fn read_returns_the_bytes() {
        let dir = super::super::fixtures::scratch("source-read");
        let mut source = AdfSource::open(&fixture(&dir, "Workbench3.2")).unwrap();
        assert_eq!(source.read("S/Startup-sequence").unwrap(), b"; test\n");
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
        let image = fixture(&dir, "Workbench3.2");
        let mut source = AdfSource::open(&image).unwrap();

        let entry = source.entry("").unwrap().unwrap();
        assert!(entry.is_dir);
        assert_eq!(entry.path, "");
    }

    // ---- decision 4: a chain walk needs a step limit ----

    /// A directory that is its own ancestor must not hang the whole
    /// application — `panic = "abort"` in the release profile means an
    /// unbounded recursion here is not a slow leak, it is a crash.
    ///
    /// Splices the **root's own** hash table to point at the root block
    /// itself: `dir::is_directory` counts `ST_ROOT` as a directory, so the
    /// root becomes its own child with no hash-chain loop anywhere — the
    /// case `dir.rs`'s own loop test does not reach, since that one is a
    /// single `entries_in` call walking a chain that points at itself, not a
    /// tree of otherwise-well-formed directories that never bottoms out.
    /// Only `walk_dir`'s own depth cap catches this one.
    #[test]
    fn a_directory_tree_deeper_than_the_cap_is_an_error_not_a_hang() {
        let dir = super::super::fixtures::scratch("source-depth-cap");
        let image = super::super::fixtures::media(&dir, "Loop", "loop.adf", &[]);

        let geometry = crate::core::volume::VolumeGeometry::floppy_dd(
            crate::core::volume::DosType::new(*b"DOS\x01"),
        );
        let root_offset = geometry.root_block as usize * geometry.block_size;

        let mut raw = std::fs::read(&image).unwrap();
        let table_slot = root_offset + layout::TABLE_OFFSET;
        raw[table_slot..table_slot + 4].copy_from_slice(&geometry.root_block.to_be_bytes());
        std::fs::write(&image, &raw).unwrap();

        let mut source = AdfSource::open(&image).unwrap();
        let err = source.walk("").unwrap_err();
        assert!(err.to_string().contains("deeper than"), "{err}");
    }
}
