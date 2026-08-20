//! Reading ISO9660 optical discs.
//!
//! Enough of the format to open an AmigaOS install CD and walk it: the volume
//! descriptor set, the directory tree, and file contents. Nothing is written —
//! this module has no path that modifies a file.
//!
//! # An ISO is not read into memory
//!
//! A CD image is 700 MB and a DVD image 4.7 GB. [`IsoImage`] holds the path,
//! the file's length and the sector layout; every call seeks to the sectors it
//! needs and reads only those, the same shape as `core::adf::open_hdf` reading
//! a 1 MB window of a multi-gigabyte HDF.
//!
//! # Every number here came from a file ART did not write
//!
//! Extents, lengths and record sizes are all attacker-controlled in the
//! general case, and the release profile aborts on panic. So: the descriptor
//! scan is capped, the directory walk is capped in entries and in depth, a
//! zero-length record advances to the next sector instead of looping, and no
//! allocation is ever made from a length field before that length has been
//! checked against the real size of the file on disk.

pub mod descriptor;
pub mod directory;
pub mod susp;

use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::volume::write::copy::{
    host_target, sidecar_for, CopySource, ExtractReport, HostTarget, OverwritePolicy,
};
use crate::core::volume::write::file::default_protection;
use crate::core::volume::write::layout::amiga_from_unix;
use crate::core::volume::write::plan::SourceEntry;
use crate::core::volume::write::uaem::Sidecar;

pub use descriptor::{SectorLayout, LOGICAL_SECTOR_SIZE};
pub use directory::IsoEntry;
pub use susp::SystemUse;

use descriptor::{
    descriptor_kind, parse_volume_descriptor, DescriptorKind, VolumeInfo, FIRST_DESCRIPTOR_LBA,
    MAX_DESCRIPTORS,
};
use directory::{parse_directory_extent_with_susp, susp_skip_from_root};

/// Most bytes ART will read for a single directory extent.
///
/// A directory of 4 MB holds tens of thousands of records — far past
/// [`directory::MAX_ENTRIES_PER_DIRECTORY`] — so this only ever bites a
/// corrupt or hostile length field.
pub const MAX_DIRECTORY_BYTES: u64 = 4 * 1024 * 1024;

/// Most bytes [`IsoImage::read_file`] will return in one call.
///
/// Reading a whole file into a `Vec` is fine for the documents, icons and
/// LHA archives on an Amiga CD. It is not fine for a 4 GB ISO nested inside
/// another one, so past this the caller is told to stream instead — which is
/// a `NotImplemented` conversation, not a silent 4 GB allocation.
pub const MAX_FILE_READ_BYTES: u64 = 256 * 1024 * 1024;

/// How deep [`IsoImage::walk`] descends.
///
/// ISO9660 itself allows 8 levels; Joliet and Rock Ridge discs exceed that
/// routinely, so ART allows more. It cannot allow unlimited: a directory
/// whose record points back at an ancestor is a legal-looking file and an
/// infinite tree.
pub const MAX_WALK_DEPTH: usize = 16;

/// Most entries [`IsoImage::walk`] collects across the whole disc.
pub const MAX_WALK_ENTRIES: usize = 100_000;

/// How many `CE` continuations ART follows for a single directory record.
///
/// A System Use Area may continue in another block, and that block's area may
/// continue again. Real discs do not chain at all — of the four measured by
/// `scripts/iso-susp-census.py` exactly one carried a single `CE`, on the
/// root's own record — but a chain that points back at itself is a legal
/// pair of numbers and an endless read.
pub const MAX_CONTINUATIONS: usize = 8;

/// Most bytes ART will read for one `CE` continuation area.
///
/// A continuation is the tail of one directory record's metadata. Anything
/// past a few sectors is a length field being used as an allocation request.
pub const MAX_CONTINUATION_BYTES: u64 = 8 * LOGICAL_SECTOR_SIZE as u64;

/// An open ISO9660 image.
///
/// Holds the path and the sector layout — never the disc's bytes.
#[derive(Debug, Clone)]
pub struct IsoImage {
    path: PathBuf,
    file_len: u64,
    layout: SectorLayout,
    volume_name: String,
    root_extent: u32,
    root_length: u32,
    /// True when the tree being read is a Joliet one, so its identifiers are
    /// UCS-2 big-endian. Decided once, at open, from the descriptor the root
    /// was taken from — never re-guessed per directory.
    joliet: bool,
    /// `Some(skip)` when the disc's root directory declared SUSP with an `SP`
    /// entry, so System Use Areas are read; `None` when it did not, so they
    /// are not looked at anywhere on the disc.
    ///
    /// Decided once, at open, for the same reason `joliet` is: a per-record
    /// guess would make one directory's record padding look like Rock Ridge
    /// and the next one's not.
    susp_skip: Option<usize>,
}

impl IsoImage {
    /// Open an image, working out its sector layout from where `CD001` sits.
    ///
    /// The two probe offsets are the ones `core::detect` already uses, and a
    /// test pins them against each other, so this agrees with detection by
    /// construction rather than by coincidence.
    pub fn open(path: &Path) -> CoreResult<Self> {
        let mut file = File::open(path)?;
        let file_len = file.metadata()?.len();
        let layout = probe_layout(&mut file, file_len)?;
        Self::open_with_layout_inner(path, file, file_len, layout)
    }

    /// Open an image whose layout is already known — from `core::detect`'s
    /// `format_hint`, for instance, which has done this work once already.
    pub fn open_with_layout(path: &Path, layout: SectorLayout) -> CoreResult<Self> {
        let file = File::open(path)?;
        let file_len = file.metadata()?.len();
        Self::open_with_layout_inner(path, file, file_len, layout)
    }

    fn open_with_layout_inner(
        path: &Path,
        mut file: File,
        file_len: u64,
        layout: SectorLayout,
    ) -> CoreResult<Self> {
        refuse_form2(&mut file, file_len, layout)?;
        let volume = scan_descriptors(&mut file, file_len, layout)?;

        // The root has to be somewhere inside the image. Checking it here
        // means every later call starts from a block that exists.
        let root_start = layout.data_offset_of(volume.root_extent)?;
        if root_start >= file_len {
            return Err(malformed(format!(
                "the root directory is at block {}, which is past the end of this image",
                volume.root_extent
            )));
        }

        let mut image = Self {
            path: path.to_path_buf(),
            file_len,
            layout,
            volume_name: volume.volume_name,
            root_extent: volume.root_extent,
            root_length: volume.root_length,
            joliet: volume.joliet,
            susp_skip: None,
        };
        image.susp_skip = image.detect_susp();
        Ok(image)
    }

    /// Ask the root directory whether this disc carries System Use data.
    ///
    /// Reads one sector and looks at one record — the root's `.` — because
    /// that is the only place SUSP allows the `SP` entry to be. A disc that
    /// cannot be read here is not an error: it is a disc with no Rock Ridge,
    /// which every plain ISO9660 disc is, and it still opens and copies.
    ///
    /// **This asks the tree ART is actually reading.** When a disc carries a
    /// Joliet descriptor ART reads the Joliet tree, whose records normally
    /// carry no System Use Area at all, and this correctly answers `None`
    /// there. Every Amiga disc measured for ART-078 — AmigaOS 3.9, the two
    /// Developer CDs — has *no* Joliet descriptor, which is exactly why its
    /// real names and protection bits are in the primary tree's System Use
    /// Areas and nowhere else.
    fn detect_susp(&self) -> Option<usize> {
        let buf = self
            .read_payload(self.root_extent, LOGICAL_SECTOR_SIZE as u64)
            .ok()?;
        susp_skip_from_root(&buf)
    }

    /// The disc's volume identifier.
    pub fn volume_name(&self) -> &str {
        &self.volume_name
    }

    /// The sector layout this image was opened with.
    pub fn layout(&self) -> SectorLayout {
        self.layout
    }

    /// True when the names being read come from a Joliet tree.
    pub fn is_joliet(&self) -> bool {
        self.joliet
    }

    /// The root directory's `(extent, length)`, ready to hand to [`list`].
    ///
    /// [`list`]: IsoImage::list
    pub fn root(&self) -> (u32, u32) {
        (self.root_extent, self.root_length)
    }

    /// List one directory.
    ///
    /// `extent` and `length` come from [`root`] or from an [`IsoEntry`] with
    /// `is_dir` set. `.` and `..` are not returned.
    ///
    /// [`root`]: IsoImage::root
    pub fn list(&self, extent: u32, length: u32) -> CoreResult<Vec<IsoEntry>> {
        if length == 0 {
            return Err(malformed(
                "a directory on this disc declares no contents at all".to_string(),
            ));
        }
        if length as u64 > MAX_DIRECTORY_BYTES {
            return Err(malformed(format!(
                "a directory on this disc claims to be {length} bytes; ART reads at most {MAX_DIRECTORY_BYTES}"
            )));
        }
        // Records live in whole sectors, so read whole sectors: the parser
        // then knows exactly where each sector begins, which is what makes a
        // zero-length record a step to the next sector rather than a stall.
        let sectors = sectors_for(length as u64)?;
        let padded = sectors * LOGICAL_SECTOR_SIZE as u64;
        let buf = self.read_payload(extent, padded)?;
        let parsed = parse_directory_extent_with_susp(&buf, self.joliet, self.susp_skip)?;
        parsed
            .into_iter()
            .map(|(mut entry, mut state)| {
                if self.susp_skip.is_some() && state.continuation.is_some() {
                    self.resolve_continuations(&mut entry, &mut state)?;
                }
                Ok(entry)
            })
            .collect()
    }

    /// Follow a record's `CE` chain and merge what it holds into the entry.
    ///
    /// Done here rather than in `core::iso::directory` because a continuation
    /// lives in another block and that module has no I/O — the split is what
    /// keeps the record parser a pure function over one buffer.
    ///
    /// The chain is bounded three ways, because all three numbers came off
    /// the disc: [`MAX_CONTINUATIONS`] links, [`MAX_CONTINUATION_BYTES`] per
    /// link, and a set of blocks already visited so a continuation pointing
    /// at itself is followed once rather than forever. A continuation that
    /// cannot be read is dropped, not raised: it costs the entry its comment,
    /// and refusing to list the directory over it would cost the user the
    /// whole disc.
    fn resolve_continuations(
        &self,
        entry: &mut IsoEntry,
        system_use: &mut SystemUse,
    ) -> CoreResult<()> {
        let mut seen: HashSet<u32> = HashSet::new();
        let mut followed = 0usize;

        while let Some(area) = system_use.continuation.take() {
            if followed >= MAX_CONTINUATIONS || !seen.insert(area.block) {
                break;
            }
            followed += 1;

            let length = area.length as u64;
            let start = area.offset as u64;
            // An offset is a position *within* a block, so it cannot reach a
            // sector; a length past the cap is a length field being used as
            // an allocation request. Both are checked before the read rather
            // than after, and `checked_add` because both came off the disc.
            if length == 0 || start >= LOGICAL_SECTOR_SIZE as u64 {
                break;
            }
            let Some(end) = start
                .checked_add(length)
                .filter(|&e| e <= MAX_CONTINUATION_BYTES)
            else {
                break;
            };
            let Ok(buf) = self.read_payload(area.block, end) else {
                break;
            };
            let Some(slice) = buf.get(start as usize..end as usize) else {
                break;
            };
            // Skip zero, not `self.susp_skip`: the `SP` count is what a
            // *directory record's* System Use field begins with, and a
            // continuation area is not one — it is the tail of a field that
            // has already been skipped once. Applying it again would eat the
            // first entry of every continuation.
            susp::parse_into(slice, 0, system_use)?;
        }

        if let Some(name) = system_use.name.clone().filter(|n| !n.is_empty()) {
            entry.name = name;
        }
        entry.protection = system_use.protection;
        entry.comment = system_use.comment.clone().filter(|c| !c.is_empty());
        Ok(())
    }

    /// Read a file's contents.
    ///
    /// `extent` and `bytes` come from an [`IsoEntry`]. Both are checked
    /// against the real length of the image before a single byte is
    /// allocated — a record claiming 3 GB inside a 600 KB file is an error,
    /// not an allocation followed by a short read.
    pub fn read_file(&self, extent: u32, bytes: u64) -> CoreResult<Vec<u8>> {
        if bytes > MAX_FILE_READ_BYTES {
            return Err(CoreError::InvalidInput(format!(
                "this file is {bytes} bytes; ART reads at most {MAX_FILE_READ_BYTES} bytes from a disc in one go"
            )));
        }
        self.read_payload(extent, bytes)
    }

    /// Walk the whole tree, depth first, bounded.
    ///
    /// Returns what it found together with whether a bound stopped it, so a
    /// caller can say "this disc is deeper than ART walks" instead of
    /// presenting a truncated listing as if it were complete (§10, §89).
    pub fn walk(&self) -> CoreResult<IsoWalk> {
        self.walk_subtree(self.root_extent, self.root_length)
    }

    /// Walk one subtree, depth first, bounded — the same walk [`walk`] does,
    /// starting anywhere rather than at the disc's root.
    ///
    /// `extent`/`length` name a directory the same way [`list`] does. Paths
    /// in the result are relative to *this* directory, with no leading
    /// component for it — the same shape a [`CopySource`] gives for a picked
    /// folder, which is what [`IsoSource`] builds this from.
    ///
    /// [`walk`]: IsoImage::walk
    /// [`list`]: IsoImage::list
    pub fn walk_subtree(&self, extent: u32, length: u32) -> CoreResult<IsoWalk> {
        let mut result = IsoWalk::default();
        // Extents already descended into. A directory record that points at
        // an ancestor is a legal-looking record and an endless tree; without
        // this, only the depth cap stands between ART and 16 levels of the
        // same directory.
        let mut visited: HashSet<u32> = HashSet::new();
        visited.insert(extent);

        let mut stack: Vec<(u32, u32, usize, String)> = vec![(extent, length, 0, String::new())];

        while let Some((extent, length, depth, prefix)) = stack.pop() {
            for entry in self.list(extent, length)? {
                if result.entries.len() >= MAX_WALK_ENTRIES {
                    result.truncated = true;
                    return Ok(result);
                }
                let path = if prefix.is_empty() {
                    entry.name.clone()
                } else {
                    format!("{prefix}/{}", entry.name)
                };

                if entry.is_dir {
                    if depth + 1 >= MAX_WALK_DEPTH {
                        result.depth_limited = true;
                    } else if visited.insert(entry.extent) {
                        stack.push((
                            entry.extent,
                            // A directory's length is a u32 on disc; a
                            // directory claiming more than u32 cannot exist.
                            entry.bytes.min(u32::MAX as u64) as u32,
                            depth + 1,
                            path.clone(),
                        ));
                    }
                }

                result.entries.push(IsoWalkEntry { path, entry });
            }
        }

        Ok(result)
    }

    /// Copy a subtree of this disc out to a folder on the host.
    ///
    /// `extent`/`length` name a directory the same way [`list`] does; the
    /// directory's *contents* land inside `dest`, not a folder named after
    /// the directory itself — the same shape
    /// `core::volume::write::copy::extract_from_volume` gives for an Amiga
    /// volume's `dir_block`. A disc is read-only, so this is the whole of
    /// "F5 out of a disc" for a local destination; the Amiga-volume
    /// direction goes through [`IsoSource`] and the existing copy engine
    /// instead, deliberately, rather than a second one here.
    ///
    /// `policy` is the user's collision setting, the same one an ADF copied
    /// out obeys — this used to be hardcoded to "skip" here, so the setting
    /// meant one thing for a floppy and nothing at all for a disc.
    ///
    /// [`list`]: IsoImage::list
    pub fn extract_tree(
        &self,
        extent: u32,
        length: u32,
        dest: &Path,
        policy: OverwritePolicy,
        sink: &dyn ProgressSink,
    ) -> CoreResult<ExtractReport> {
        let mut report = ExtractReport::default();
        std::fs::create_dir_all(dest)?;
        // The same set `walk_subtree` keeps, and for the same reason: a
        // directory record pointing back at an ancestor is legal-looking and
        // describes a tree with no bottom. `MAX_WALK_DEPTH` alone does not
        // save this path — a root holding eight directories that all point at
        // the root is 8^16 `create_dir_all` calls before the depth cap bites.
        let mut visited: HashSet<u32> = HashSet::new();
        visited.insert(extent);
        self.extract_dir(
            extent,
            length,
            dest,
            0,
            policy,
            &mut visited,
            sink,
            &mut report,
        )?;
        Ok(report)
    }

    #[allow(clippy::too_many_arguments)]
    fn extract_dir(
        &self,
        extent: u32,
        length: u32,
        dest: &Path,
        depth: usize,
        policy: OverwritePolicy,
        visited: &mut HashSet<u32>,
        sink: &dyn ProgressSink,
        report: &mut ExtractReport,
    ) -> CoreResult<()> {
        if depth >= MAX_WALK_DEPTH {
            report.skipped.push(format!(
                "{} — nested deeper than ART follows",
                dest.display()
            ));
            return Ok(());
        }

        for entry in self.list(extent, length)? {
            if sink.is_cancelled() {
                report.cancelled = true;
                return Ok(());
            }
            sink.report(report.files_written as u64, None, &entry.name);

            // Before anything is created for it: a directory already written
            // once on this run is a cycle, not a second copy of the tree.
            if entry.is_dir && visited.contains(&entry.extent) {
                report.skipped.push(format!(
                    "{} — this directory points back at one ART has already written",
                    entry.name
                ));
                continue;
            }

            // The NTFS-safe name, the containment check and the collision
            // policy, decided in the one place `extract_from_volume` decides
            // them too — this was a second copy of that logic, and it had
            // already drifted.
            let target = match host_target(dest, &entry.name, entry.is_dir, policy, report)? {
                HostTarget::Skip => continue,
                HostTarget::Descend(target) => {
                    visited.insert(entry.extent);
                    // A directory's length is a u32 on disc; one claiming more
                    // than u32::MAX cannot exist, same clamp `walk_subtree` uses.
                    let sub_length = entry.bytes.min(u32::MAX as u64) as u32;
                    self.extract_dir(
                        entry.extent,
                        sub_length,
                        &target,
                        depth + 1,
                        policy,
                        visited,
                        sink,
                        report,
                    )?;
                    continue;
                }
                HostTarget::Write(target) => target,
            };

            let data = match self.read_file(entry.extent, entry.bytes) {
                Ok(data) => data,
                Err(err) => {
                    report.skipped.push(format!("{} — {err}", entry.name));
                    continue;
                }
            };

            // Through `core/safety`: a truncated file on the way out is
            // still a file the user will believe is a good copy.
            crate::core::safety::atomic::atomic_write(&target, &data)?;
            report.files_written += 1;
            report.bytes_written += data.len() as u64;

            // The `AS` entry's bits and comment, in the one format that can
            // hold them on NTFS — the same `.uaem` sidecar an ADF extraction
            // writes, through the same `sidecar_for`, which declines when the
            // file has nothing worth recording. Unconditional here rather
            // than behind a flag: `extract_from_volume` takes one because a
            // *volume* extraction has a caller that can sensibly say no; a
            // disc has no writer to round-trip back to, so losing the bits
            // here loses them for good, which is ART-078 itself.
            //
            // Only for a record that actually carried an `AS` entry. A
            // recording date alone is not Amiga metadata, and writing a
            // sidecar for it would put a second file beside every file on
            // every plain ISO9660 disc ART has ever extracted — a visible
            // change to something ART-078 does not ask about.
            let carries_amiga_metadata = entry.protection.is_some() || entry.comment.is_some();
            if let Some(sidecar) = carries_amiga_metadata
                .then(|| {
                    sidecar_for(
                        entry.protection.unwrap_or_else(default_protection),
                        entry.date.map(amiga_from_unix).unwrap_or_default(),
                        entry.comment.as_deref().unwrap_or_default(),
                    )
                })
                .flatten()
            {
                crate::core::safety::atomic::atomic_write(
                    &crate::core::volume::write::uaem::sidecar_path(&target),
                    crate::core::volume::write::uaem::render(&sidecar).as_bytes(),
                )?;
                report.sidecars_written += 1;
            }
        }

        Ok(())
    }

    /// Read `bytes` of user data starting at logical block `extent`.
    ///
    /// The single choke point for reading disc contents. The order matters:
    /// the range is proved to exist inside the file *before* the buffer is
    /// allocated, so no length field on the disc can make ART reserve memory
    /// for data that is not there.
    fn read_payload(&self, extent: u32, bytes: u64) -> CoreResult<Vec<u8>> {
        if bytes == 0 {
            return Ok(Vec::new());
        }
        if bytes > MAX_FILE_READ_BYTES {
            return Err(malformed(format!(
                "an extent on this disc claims {bytes} bytes, more than ART reads at once"
            )));
        }

        let sectors = sectors_for(bytes)?;
        let last_lba = (extent as u64)
            .checked_add(sectors - 1)
            .filter(|&l| l <= u32::MAX as u64)
            .ok_or_else(|| {
                malformed(format!(
                    "an extent starting at block {extent} runs past the end of the address space"
                ))
            })? as u32;

        // Bytes wanted from the final sector; the ones before it are full.
        let tail = bytes - (sectors - 1) * LOGICAL_SECTOR_SIZE as u64;
        let last_end = self
            .layout
            .data_offset_of(last_lba)?
            .checked_add(tail)
            .ok_or_else(|| malformed("an extent's length overflows the image".to_string()))?;
        if last_end > self.file_len {
            return Err(malformed(format!(
                "this disc points at {bytes} bytes from block {extent}, which is past the end of the image ({} bytes)",
                self.file_len
            )));
        }

        // Only now, with the bytes proved present, is anything allocated.
        let mut out = vec![0u8; bytes as usize];
        let mut file = File::open(&self.path)?;

        match self.layout {
            // Cooked sectors are contiguous, so this is one seek and one read.
            SectorLayout::Cooked => {
                file.seek(SeekFrom::Start(self.layout.data_offset_of(extent)?))?;
                file.read_exact(&mut out)?;
            }
            // Raw sectors interleave sync, header, (for XA) a subheader and
            // ECC that are not part of the file, so each has to be lifted
            // separately. `data_offset_of` carries where the data starts, so
            // Mode 1 and Mode 2 Form 1 differ only in that one number.
            SectorLayout::Raw2352 | SectorLayout::Raw2352Xa => {
                for i in 0..sectors {
                    let lba = extent as u64 + i;
                    let start = (i * LOGICAL_SECTOR_SIZE as u64) as usize;
                    let take = ((bytes as usize) - start).min(LOGICAL_SECTOR_SIZE);
                    file.seek(SeekFrom::Start(self.layout.data_offset_of(lba as u32)?))?;
                    file.read_exact(&mut out[start..start + take])?;
                }
            }
        }

        Ok(out)
    }
}

/// One entry from [`IsoImage::walk`], with the path that reached it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsoWalkEntry {
    /// Slash-separated path from the root, not including a leading slash.
    pub path: String,
    pub entry: IsoEntry,
}

/// The result of walking a disc, and whether a bound cut it short.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IsoWalk {
    pub entries: Vec<IsoWalkEntry>,
    /// A directory was not descended into because it sat at
    /// [`MAX_WALK_DEPTH`].
    pub depth_limited: bool,
    /// The walk stopped early at [`MAX_WALK_ENTRIES`].
    pub truncated: bool,
}

/// A subtree of an optical disc, as a [`CopySource`] for
/// `core::volume::write::copy::copy_into_volume`.
///
/// The same shape [`HostSelection`](crate::core::volume::write::copy::HostSelection)
/// gives a picked set of Windows folders: a disc has no copy engine of its
/// own, it only has to answer the three questions [`CopySource`] asks. This
/// is what makes "F5 out of a disc, into an Amiga volume" reuse the one
/// tested copy engine rather than needing a second.
///
/// The subtree is walked once, eagerly, at construction — every later
/// `read`/`metadata` call is a lookup into that snapshot, not a fresh walk of
/// the disc, and a disc's directory tree never changes under ART's feet the
/// way a live host folder theoretically could.
pub struct IsoSource {
    image: IsoImage,
    entries: Vec<IsoWalkEntry>,
}

impl IsoSource {
    /// `extent`/`length` name a directory the same way [`IsoImage::list`]
    /// does.
    pub fn new(image: IsoImage, extent: u32, length: u32) -> CoreResult<Self> {
        let walk = image.walk_subtree(extent, length)?;
        Ok(Self {
            image,
            entries: walk.entries,
        })
    }

    /// One file of a disc, as a copy source of exactly one entry.
    ///
    /// The scope F5 on a single selected file has to have. [`new`] walks a
    /// *subtree*, so a caller with only a file to copy used to hand it the
    /// whole directory the file sat in — on an install CD that is hundreds of
    /// megabytes copied while the status line names one file. `CopySource`
    /// needs nothing more than this to answer for a single entry:
    /// `relative` is the file's own name, so it lands directly in the
    /// destination directory.
    ///
    /// `name`, `extent` and `bytes` come from the listing that found it
    /// ([`IsoImage::list`]), the same three fields
    /// `commands::iso::iso_extract_file` takes for the local-folder direction.
    ///
    /// `parent` is that listing's own `(extent, length)`. It is what makes a
    /// single copied file keep its Amiga `AS` protection bits and comment:
    /// those live in the *directory record*, not in the file, so the only way
    /// to have them is to read the record again — and reading it here rather
    /// than accepting them as arguments is deliberate, because a protection
    /// byte that arrived from the frontend is a protection byte ART did not
    /// verify. `None` copies the file with default bits, which is what a disc
    /// with no System Use Area gives anyway.
    ///
    /// A parent that cannot be listed, or that does not hold this record, is
    /// not an error: the file still copies, without its bits. Refusing a copy
    /// over metadata would trade the thing the user asked for against the
    /// thing they did not.
    ///
    /// [`new`]: IsoSource::new
    pub fn single_file(
        image: IsoImage,
        name: &str,
        extent: u32,
        bytes: u64,
        date: Option<i64>,
        parent: Option<(u32, u32)>,
    ) -> Self {
        let found = parent.and_then(|(dir_extent, dir_length)| {
            image
                .list(dir_extent, dir_length)
                .ok()?
                .into_iter()
                .find(|e| !e.is_dir && e.extent == extent && e.name == name)
        });
        Self {
            image,
            entries: vec![IsoWalkEntry {
                path: name.to_string(),
                entry: IsoEntry {
                    name: name.to_string(),
                    is_dir: false,
                    bytes,
                    extent,
                    date,
                    protection: found.as_ref().and_then(|e| e.protection),
                    comment: found.and_then(|e| e.comment),
                },
            }],
        }
    }
}

impl CopySource for IsoSource {
    fn entries(&self) -> CoreResult<Vec<SourceEntry>> {
        Ok(self
            .entries
            .iter()
            .map(|e| SourceEntry {
                relative: e.path.clone(),
                is_dir: e.entry.is_dir,
                bytes: e.entry.bytes,
            })
            .collect())
    }

    fn read(&self, relative: &str) -> CoreResult<Vec<u8>> {
        let found = self
            .entries
            .iter()
            .find(|e| e.path == relative && !e.entry.is_dir)
            .ok_or_else(|| {
                CoreError::InvalidInput(format!("'{relative}' is not part of this disc"))
            })?;
        self.image.read_file(found.entry.extent, found.entry.bytes)
    }

    /// What AmigaDOS metadata this disc has for one entry.
    ///
    /// # Why a disc-sourced file now carries protection bits (ART-078)
    ///
    /// It used to be true that "a disc carries a recording date and nothing
    /// else AmigaDOS would call protection bits or a comment". It is not:
    /// an Amiga-mastered disc records both in the `AS` System Use entry, and
    /// two of the four discs measured for ART-078 carry one on every file —
    /// 44 796 entries between them, including 145 with the `p` (pure) bit and
    /// 20 with the `s` (script) bit set.
    ///
    /// Those two bits are the reason this matters rather than a nicety.
    /// `core::volume::write::uaem`'s own module doc records why: a WHDLoad
    /// slave copied without its `S` bit is a game that starts and does not
    /// work, and the AmigaOS 3.9 tree's `Resident C:Assign PURE` needs `p`.
    /// Copying a game off a CD onto an HDF used to lose exactly those.
    ///
    /// So the answer is: **yes, and through the path that already existed.**
    /// A `Sidecar` is what `CopySource` speaks, `copy.rs` turns it into the
    /// `FileMeta` the volume writer stores and into a `.uaem` file when the
    /// destination is a Windows folder, and `copy.rs::sidecar_for` already
    /// declines to write one for a file whose bits are the default. Nothing
    /// new was needed downstream — the bits simply had to stop being thrown
    /// away here.
    ///
    /// A file with no `AS` entry still gets [`default_protection`], which is
    /// the same answer a host file with no `.uaem` beside it gets. That is
    /// deliberate: absent is not the same as `----rwed`, but AmigaDOS has no
    /// third state, and inventing restrictive bits for a disc that recorded
    /// none would break more than it protects.
    fn metadata(&self, relative: &str) -> CoreResult<Option<Sidecar>> {
        let Some(found) = self.entries.iter().find(|e| e.path == relative) else {
            return Ok(None);
        };
        // Nothing at all to say about this entry: no date, no bits, no
        // comment. `None` keeps the writer's own defaults rather than
        // stamping an epoch date over them.
        if found.entry.date.is_none()
            && found.entry.protection.is_none()
            && found.entry.comment.is_none()
        {
            return Ok(None);
        }
        Ok(Some(Sidecar {
            protection: found.entry.protection.unwrap_or_else(default_protection),
            date: found.entry.date.map(amiga_from_unix).unwrap_or_default(),
            comment: found.entry.comment.clone().unwrap_or_default(),
        }))
    }
}

/// Sectors needed to hold `bytes` of user data.
fn sectors_for(bytes: u64) -> CoreResult<u64> {
    let sector = LOGICAL_SECTOR_SIZE as u64;
    let rounded = bytes
        .checked_add(sector - 1)
        .ok_or_else(|| malformed("a length on this disc overflows".to_string()))?;
    Ok(rounded / sector)
}

/// Work out whether sectors are 2048 or 2352 bytes by looking for `CD001`.
///
/// Both offsets are exactly the ones `core::detect` probes, so an image
/// detection called `iso9660` opens as [`SectorLayout::Cooked`] here.
fn probe_layout(file: &mut File, file_len: u64) -> CoreResult<SectorLayout> {
    for layout in [
        SectorLayout::Cooked,
        SectorLayout::Raw2352,
        SectorLayout::Raw2352Xa,
    ] {
        let at = layout.data_offset_of(FIRST_DESCRIPTOR_LBA)? + 1;
        if at + descriptor::ISO_MAGIC.len() as u64 > file_len {
            continue;
        }
        file.seek(SeekFrom::Start(at))?;
        let mut magic = [0u8; 5];
        if file.read_exact(&mut magic).is_ok() && &magic == descriptor::ISO_MAGIC {
            return Ok(layout);
        }
    }
    Err(CoreError::UnsupportedFormat(
        "this file does not carry an ISO9660 volume descriptor at sector 16".to_string(),
    ))
}

/// Refuse a Mode 2 **Form 2** track rather than reading it as Form 1.
///
/// A Form 2 sector carries 2324 bytes of user data and no error correction —
/// it is how audio and video are stored, and it holds no ISO9660 filesystem.
/// Reading 2048 bytes out of one and calling the result a directory would
/// produce confident nonsense, which is the failure mode this whole module is
/// written against. The submode byte says which form the sector is, so ART
/// asks rather than assumes.
///
/// Only raw layouts have a subheader to read; a cooked image is user data
/// alone and passes straight through. A file too short to hold the header is
/// not judged here — [`scan_descriptors`] reports what is actually wrong with
/// it a moment later.
fn refuse_form2(file: &mut File, file_len: u64, layout: SectorLayout) -> CoreResult<()> {
    if !layout.is_raw() {
        return Ok(());
    }
    let sector_start = (FIRST_DESCRIPTOR_LBA as u64)
        .checked_mul(layout.sector_size())
        .ok_or_else(|| malformed("this image's sector arithmetic overflows".to_string()))?;
    let header_len = descriptor::XA_DATA_OFFSET as usize;
    if sector_start + header_len as u64 > file_len {
        return Ok(());
    }

    file.seek(SeekFrom::Start(sector_start))?;
    let mut header = vec![0u8; header_len];
    file.read_exact(&mut header)?;

    let mode2 = header[descriptor::RAW_MODE_OFFSET] == 2;
    let form2 = header[descriptor::XA_SUBMODE_OFFSET] & descriptor::XA_SUBMODE_FORM2 != 0;
    if mode2 && form2 {
        return Err(CoreError::UnsupportedFormat(
            "this track's sectors are Mode 2 Form 2, which carries audio or video rather than a \
             filesystem. ART reads Mode 1 and Mode 2 Form 1 data tracks."
                .to_string(),
        ));
    }
    Ok(())
}

/// Walk the volume descriptor set and pick the tree to read.
///
/// Joliet wins when it is there: it is where the real filenames live, and the
/// Primary descriptor's copy of them has been folded to uppercase 8.3.
fn scan_descriptors(
    file: &mut File,
    file_len: u64,
    layout: SectorLayout,
) -> CoreResult<VolumeInfo> {
    let mut primary: Option<VolumeInfo> = None;
    let mut joliet: Option<VolumeInfo> = None;
    let mut terminated = false;

    let mut sector = vec![0u8; LOGICAL_SECTOR_SIZE];
    for i in 0..MAX_DESCRIPTORS as u32 {
        let lba = FIRST_DESCRIPTOR_LBA + i;
        let at = layout.data_offset_of(lba)?;
        if at + LOGICAL_SECTOR_SIZE as u64 > file_len {
            return Err(malformed(
                "the volume descriptors run past the end of this image".to_string(),
            ));
        }
        file.seek(SeekFrom::Start(at))?;
        file.read_exact(&mut sector)?;

        match descriptor_kind(&sector)? {
            DescriptorKind::Terminator => {
                terminated = true;
                break;
            }
            DescriptorKind::Primary => {
                if primary.is_none() {
                    primary = Some(parse_volume_descriptor(&sector, DescriptorKind::Primary)?);
                }
            }
            DescriptorKind::Joliet => {
                if joliet.is_none() {
                    joliet = Some(parse_volume_descriptor(&sector, DescriptorKind::Joliet)?);
                }
            }
            DescriptorKind::Other => {}
        }
    }

    if !terminated {
        return Err(malformed(format!(
            "this disc lists more than {MAX_DESCRIPTORS} volume descriptors without a terminator"
        )));
    }

    joliet
        .or(primary)
        .ok_or_else(|| malformed("this disc has no primary volume descriptor".to_string()))
}

fn malformed(detail: String) -> CoreError {
    CoreError::Malformed {
        format: "iso9660".to_string(),
        detail,
    }
}

/// Building synthetic ISO images, for tests only.
///
/// ART ships no copyrighted Amiga content, so every fixture in this module is
/// assembled here byte by byte. That creates the risk this whole task turns
/// on: the builder below and the reader above were written from the same
/// offsets, so they can agree with each other and both be wrong — which is
/// precisely what ART-032 … ART-035 were.
///
/// The mitigation is [`export_iso_for_oracle_when_asked`]: it writes a
/// fixture to wherever `ART_ISO_OUT` points so an implementation that shares
/// no code with this one — the host operating system's own ISO driver — can
/// be asked whether these bytes are a disc.
///
/// [`export_iso_for_oracle_when_asked`]: tests::export_iso_for_oracle_when_asked
#[cfg(test)]
pub(crate) mod fixture {
    use super::descriptor::{LOGICAL_SECTOR_SIZE, RAW_SECTOR_SIZE};
    use super::SectorLayout;
    use std::collections::VecDeque;

    /// The System Use data to write into one record, when the builder is
    /// making a Rock Ridge disc.
    ///
    /// # This copies a real disc's shape, not a convenient one
    ///
    /// Measured by `scripts/iso-susp-census.py` over the owner's four discs
    /// and reproduced here so a passing test means something about real
    /// material:
    ///
    /// - **`RR` first, then `PX`, then `NM`, then `AS`** — the order the
    ///   AmigaOS 3.9 CD and Amiga Developer CD v2.1 both write, and the
    ///   reason `PX` is here at all: it is 36 bytes of a signature ART does
    ///   not read, sitting between the `SP` and the `AS`. A parser that
    ///   stopped at the first unknown entry would pass every test written
    ///   without it.
    /// - **`SP` in the root's `.` record only**, with skip 0 — both discs.
    /// - **`AS` flags `0x01`** (protection, no comment) on 44 795 of the
    ///   44 796 real entries; `0x03` (protection and comment) on the rest.
    /// - **Directories carry `NM` and `AS` too**, not just files.
    #[derive(Debug, Clone, Default)]
    pub struct RockRidge {
        /// The mixed-case name for the `NM` entry. Empty writes no `NM`.
        pub name: String,
        /// The AmigaDOS protection long for the `AS` entry, or `None` for a
        /// record with no `AS` at all.
        pub protection: Option<u32>,
        /// The `AS` comment. Empty writes no comment fragment.
        pub comment: String,
        /// Split the `NM` and the comment across a `CE` continuation area.
        /// No measured disc does this; it is here because the standard
        /// allows it and an unexercised continuation reader is an untested
        /// one.
        pub continue_in_ce: bool,
    }

    /// A file or directory to put on the synthetic disc.
    #[derive(Debug, Clone)]
    pub enum Node {
        File {
            /// The 8.3 uppercase name in the Primary tree, without `;1`.
            iso: String,
            /// The name in the Joliet tree, if one is being built.
            joliet: String,
            data: Vec<u8>,
            rock: Option<RockRidge>,
        },
        Dir {
            iso: String,
            joliet: String,
            children: Vec<Node>,
            rock: Option<RockRidge>,
        },
    }

    pub fn file(iso: &str, joliet: &str, data: &[u8]) -> Node {
        Node::File {
            iso: iso.to_string(),
            joliet: joliet.to_string(),
            data: data.to_vec(),
            rock: None,
        }
    }

    pub fn dir(iso: &str, joliet: &str, children: Vec<Node>) -> Node {
        Node::Dir {
            iso: iso.to_string(),
            joliet: joliet.to_string(),
            children,
            rock: None,
        }
    }

    /// The same file, with a System Use Area on its record.
    pub fn rock_file(iso: &str, data: &[u8], rock: RockRidge) -> Node {
        Node::File {
            iso: iso.to_string(),
            joliet: String::new(),
            data: data.to_vec(),
            rock: Some(rock),
        }
    }

    /// The same directory, with a System Use Area on its record.
    pub fn rock_dir(iso: &str, children: Vec<Node>, rock: RockRidge) -> Node {
        Node::Dir {
            iso: iso.to_string(),
            joliet: String::new(),
            children,
            rock: Some(rock),
        }
    }

    /// A synthetic ISO9660 image.
    #[derive(Debug, Clone)]
    pub struct IsoBuilder {
        pub volume: String,
        pub joliet_volume: String,
        pub joliet: bool,
        pub layout: SectorLayout,
        pub children: Vec<Node>,
        /// Give the root directory two sectors and start its entries after
        /// the first one's padding, so the reader has to treat a zero-length
        /// record as "next sector" rather than "stop" or "loop".
        pub split_root: bool,
        /// Declare SUSP: write an `SP` entry into the root's `.` record, so
        /// System Use Areas on this disc are read at all. A disc with `AS`
        /// entries and no `SP` is a disc ART deliberately ignores, and a
        /// fixture that set one without the other would test the wrong thing.
        pub rock_ridge: bool,
        /// Bytes of padding before the first SUSP entry of every System Use
        /// Area *except* the root's `.`, declared through `SP`'s skip field.
        /// Both measured discs use 0; a non-zero value is what proves the
        /// skip is applied rather than assumed.
        pub susp_skip: u8,
    }

    impl Default for IsoBuilder {
        fn default() -> Self {
            Self {
                volume: "ART_TEST".to_string(),
                joliet_volume: "ART Test".to_string(),
                joliet: false,
                layout: SectorLayout::Cooked,
                children: Vec::new(),
                split_root: false,
                rock_ridge: false,
                susp_skip: 0,
            }
        }
    }

    struct FlatDir {
        iso: String,
        joliet: String,
        parent: usize,
        subdirs: Vec<usize>,
        files: Vec<usize>,
        rock: Option<RockRidge>,
    }

    struct FlatFile {
        iso: String,
        joliet: String,
        data: Vec<u8>,
        extent: u32,
        rock: Option<RockRidge>,
    }

    /// A fixed recording date: 1994-05-17 08:30:00 UTC.
    const FIXTURE_DATE: [u8; 7] = [94, 5, 17, 8, 30, 0, 0];

    impl IsoBuilder {
        pub fn build(self) -> Vec<u8> {
            let (dirs, mut files) = self.flatten();

            // --- sector allocation ---------------------------------------
            let mut next = 16u32;
            let pvd_lba = next;
            next += 1;
            let svd_lba = if self.joliet {
                let l = next;
                next += 1;
                l
            } else {
                0
            };
            let term_lba = next;
            next += 1;

            let l_path_lba = next;
            next += 1;
            let m_path_lba = next;
            next += 1;
            let (jl_path_lba, jm_path_lba) = if self.joliet {
                let a = next;
                next += 1;
                let b = next;
                next += 1;
                (a, b)
            } else {
                (0, 0)
            };

            // One sector for every `CE` continuation area on the disc,
            // allocated whether or not it is used — a fixture that moved
            // every later extent depending on a flag would make two builds
            // hard to compare.
            let ce_lba = if self.rock_ridge {
                let l = next;
                next += 1;
                l
            } else {
                0
            };

            let root_sectors: u32 = if self.split_root { 2 } else { 1 };
            let mut iso_dir_lba = Vec::with_capacity(dirs.len());
            for i in 0..dirs.len() {
                iso_dir_lba.push(next);
                next += if i == 0 { root_sectors } else { 1 };
            }
            let mut joliet_dir_lba = Vec::with_capacity(dirs.len());
            if self.joliet {
                for i in 0..dirs.len() {
                    joliet_dir_lba.push(next);
                    next += if i == 0 { root_sectors } else { 1 };
                }
            }

            for f in files.iter_mut() {
                f.extent = next;
                next += sectors_for(f.data.len()).max(1);
            }
            let total_sectors = next;

            // --- path tables ---------------------------------------------
            let iso_l = path_table(&dirs, &iso_dir_lba, false, false);
            let iso_m = path_table(&dirs, &iso_dir_lba, true, false);
            assert_eq!(iso_l.len(), iso_m.len());
            let (jol_l, jol_m) = if self.joliet {
                (
                    path_table(&dirs, &joliet_dir_lba, false, true),
                    path_table(&dirs, &joliet_dir_lba, true, true),
                )
            } else {
                (Vec::new(), Vec::new())
            };

            // --- assemble -------------------------------------------------
            let mut image = vec![0u8; total_sectors as usize * LOGICAL_SECTOR_SIZE];

            let iso_root_len = root_sectors * LOGICAL_SECTOR_SIZE as u32;
            put(
                &mut image,
                pvd_lba,
                &volume_descriptor(
                    1,
                    &self.volume,
                    false,
                    total_sectors,
                    iso_l.len() as u32,
                    l_path_lba,
                    m_path_lba,
                    iso_dir_lba[0],
                    iso_root_len,
                ),
            );
            if self.joliet {
                put(
                    &mut image,
                    svd_lba,
                    &volume_descriptor(
                        2,
                        &self.joliet_volume,
                        true,
                        total_sectors,
                        jol_l.len() as u32,
                        jl_path_lba,
                        jm_path_lba,
                        joliet_dir_lba[0],
                        iso_root_len,
                    ),
                );
            }
            put(&mut image, term_lba, &terminator());
            put(&mut image, l_path_lba, &iso_l);
            put(&mut image, m_path_lba, &iso_m);
            if self.joliet {
                put(&mut image, jl_path_lba, &jol_l);
                put(&mut image, jm_path_lba, &jol_m);
            }

            let mut arena = CeArena {
                lba: ce_lba,
                buf: Vec::new(),
            };
            for (i, d) in dirs.iter().enumerate() {
                let sectors = if i == 0 { root_sectors } else { 1 };
                let rock = if self.rock_ridge {
                    Some((self.susp_skip, &mut arena))
                } else {
                    None
                };
                let extent = directory_extent(
                    d,
                    &dirs,
                    &files,
                    &iso_dir_lba,
                    i,
                    false,
                    sectors,
                    self.split_root && i == 0,
                    rock,
                );
                put(&mut image, iso_dir_lba[i], &extent);
                if self.joliet {
                    let extent = directory_extent(
                        d,
                        &dirs,
                        &files,
                        &joliet_dir_lba,
                        i,
                        true,
                        sectors,
                        self.split_root && i == 0,
                        None,
                    );
                    put(&mut image, joliet_dir_lba[i], &extent);
                }
            }
            if self.rock_ridge && !arena.buf.is_empty() {
                put(&mut image, ce_lba, &arena.buf);
            }

            for f in &files {
                put(&mut image, f.extent, &f.data);
            }

            match self.layout {
                SectorLayout::Cooked => image,
                SectorLayout::Raw2352 | SectorLayout::Raw2352Xa => to_raw(&image, self.layout),
            }
        }

        fn flatten(&self) -> (Vec<FlatDir>, Vec<FlatFile>) {
            let mut dirs = vec![FlatDir {
                iso: String::new(),
                joliet: String::new(),
                parent: 0,
                subdirs: Vec::new(),
                files: Vec::new(),
                rock: None,
            }];
            let mut files: Vec<FlatFile> = Vec::new();

            // Breadth first, so a directory's index is always greater than
            // its parent's — which is what a path table requires.
            let mut queue: VecDeque<(usize, &[Node])> = VecDeque::new();
            queue.push_back((0, &self.children));
            while let Some((parent, children)) = queue.pop_front() {
                for child in children {
                    match child {
                        Node::File {
                            iso,
                            joliet,
                            data,
                            rock,
                        } => {
                            let index = files.len();
                            files.push(FlatFile {
                                iso: iso.clone(),
                                joliet: joliet.clone(),
                                data: data.clone(),
                                extent: 0,
                                rock: rock.clone(),
                            });
                            dirs[parent].files.push(index);
                        }
                        Node::Dir {
                            iso,
                            joliet,
                            children,
                            rock,
                        } => {
                            let index = dirs.len();
                            dirs.push(FlatDir {
                                iso: iso.clone(),
                                joliet: joliet.clone(),
                                parent,
                                subdirs: Vec::new(),
                                files: Vec::new(),
                                rock: rock.clone(),
                            });
                            dirs[parent].subdirs.push(index);
                            queue.push_back((index, children));
                        }
                    }
                }
            }
            (dirs, files)
        }
    }

    fn sectors_for(bytes: usize) -> u32 {
        bytes.div_ceil(LOGICAL_SECTOR_SIZE) as u32
    }

    fn put(image: &mut [u8], lba: u32, data: &[u8]) {
        let at = lba as usize * LOGICAL_SECTOR_SIZE;
        image[at..at + data.len()].copy_from_slice(data);
    }

    fn both32(v: u32) -> [u8; 8] {
        let mut out = [0u8; 8];
        out[..4].copy_from_slice(&v.to_le_bytes());
        out[4..].copy_from_slice(&v.to_be_bytes());
        out
    }

    fn both16(v: u16) -> [u8; 4] {
        let mut out = [0u8; 4];
        out[..2].copy_from_slice(&v.to_le_bytes());
        out[2..].copy_from_slice(&v.to_be_bytes());
        out
    }

    /// Fill a field with spaces then write an ASCII string over it.
    fn pad_ascii(dst: &mut [u8], s: &str) {
        for b in dst.iter_mut() {
            *b = b' ';
        }
        for (slot, ch) in dst.iter_mut().zip(s.bytes()) {
            *slot = ch;
        }
    }

    /// Fill a field with UCS-2 big-endian spaces then write a string over it.
    fn pad_ucs2(dst: &mut [u8], s: &str) {
        for (i, b) in dst.iter_mut().enumerate() {
            *b = if i % 2 == 0 { 0x00 } else { 0x20 };
        }
        let units: Vec<u16> = s.encode_utf16().collect();
        for (i, u) in units.iter().enumerate() {
            let at = i * 2;
            if at + 2 > dst.len() {
                break;
            }
            dst[at..at + 2].copy_from_slice(&u.to_be_bytes());
        }
    }

    /// A directory record. `id` is already encoded for its tree.
    fn dir_record(id: &[u8], extent: u32, length: u32, is_dir: bool, system_use: &[u8]) -> Vec<u8> {
        let mut len = 33 + id.len();
        if len % 2 == 1 {
            len += 1;
        }
        let mut r = vec![0u8; len + system_use.len()];
        r[0] = r.len() as u8;
        r[2..10].copy_from_slice(&both32(extent));
        r[10..18].copy_from_slice(&both32(length));
        r[18..25].copy_from_slice(&FIXTURE_DATE);
        r[25] = if is_dir { 0x02 } else { 0x00 };
        r[28..32].copy_from_slice(&both16(1));
        r[32] = id.len() as u8;
        r[33..33 + id.len()].copy_from_slice(id);
        // The System Use Area starts after the identifier and after the pad
        // byte an even-length identifier is followed by — which is exactly
        // what `len` above already rounded up to.
        r[len..].copy_from_slice(system_use);
        r
    }

    /// The SUSP entries for one record, laid out the way the measured discs
    /// lay them out: `RR`, then `PX`, then `NM`, then `AS`.
    ///
    /// `PX` is written with its real 36-byte shape and never read by ART —
    /// its whole job here is to sit between the entries ART *does* read, so
    /// a parser that stopped at an unknown signature would fail.
    ///
    /// When `continue_in_ce` is set, the `NM` and the comment are cut in half
    /// and the tail goes into `arena`, reached through a `CE` entry.
    fn system_use_for(rock: &RockRidge, skip: usize, arena: &mut CeArena) -> Vec<u8> {
        let mut out = vec![0u8; skip];
        out.extend_from_slice(&[b'R', b'R', 5, 1, 0x89]);
        let mut px = vec![b'P', b'X', 36, 1];
        px.extend(std::iter::repeat_n(0u8, 32));
        out.extend_from_slice(&px);

        let split = rock.continue_in_ce;
        let mut tail: Vec<u8> = Vec::new();

        if !rock.name.is_empty() {
            let bytes = rock.name.as_bytes();
            let cut = if split { bytes.len() / 2 } else { bytes.len() };
            let (head, rest) = bytes.split_at(cut);
            out.push(b'N');
            out.push(b'M');
            out.push((5 + head.len()) as u8);
            out.push(1);
            out.push(if rest.is_empty() { 0x00 } else { 0x01 });
            out.extend_from_slice(head);
            if !rest.is_empty() {
                tail.extend_from_slice(&[b'N', b'M', (5 + rest.len()) as u8, 1, 0x00]);
                tail.extend_from_slice(rest);
            }
        }

        if let Some(protection) = rock.protection {
            let comment = rock.comment.as_bytes();
            let cut = if split {
                comment.len() / 2
            } else {
                comment.len()
            };
            let (head, rest) = comment.split_at(cut);
            let mut flags = 0x01u8;
            if !comment.is_empty() {
                flags |= 0x02;
            }
            if !rest.is_empty() {
                flags |= 0x04;
            }
            let mut entry = vec![b'A', b'S', 0, 1, flags];
            entry.extend_from_slice(&protection.to_be_bytes());
            if !comment.is_empty() {
                entry.push((head.len() + 1) as u8);
                entry.extend_from_slice(head);
            }
            entry[2] = entry.len() as u8;
            out.extend_from_slice(&entry);
            if !rest.is_empty() {
                let mut cont = vec![b'A', b'S', 0, 1, 0x02, (rest.len() + 1) as u8];
                cont.extend_from_slice(rest);
                cont[2] = cont.len() as u8;
                tail.extend_from_slice(&cont);
            }
        }

        if !tail.is_empty() {
            let (block, offset, length) = arena.put(&tail);
            let mut ce = vec![b'C', b'E', 28, 1];
            ce.extend_from_slice(&both32(block));
            ce.extend_from_slice(&both32(offset));
            ce.extend_from_slice(&both32(length));
            out.extend_from_slice(&ce);
        }

        if out.len() % 2 == 1 {
            out.push(0);
        }
        out
    }

    /// One sector holding every `CE` continuation area the fixture needs.
    pub struct CeArena {
        pub lba: u32,
        pub buf: Vec<u8>,
    }

    impl CeArena {
        fn put(&mut self, bytes: &[u8]) -> (u32, u32, u32) {
            let offset = self.buf.len() as u32;
            self.buf.extend_from_slice(bytes);
            assert!(
                self.buf.len() <= LOGICAL_SECTOR_SIZE,
                "fixture continuation areas do not fit one sector"
            );
            (self.lba, offset, bytes.len() as u32)
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn directory_extent(
        d: &FlatDir,
        dirs: &[FlatDir],
        files: &[FlatFile],
        dir_lba: &[u32],
        index: usize,
        joliet: bool,
        sectors: u32,
        split: bool,
        rock: Option<(u8, &mut CeArena)>,
    ) -> Vec<u8> {
        let self_lba = dir_lba[index];
        let parent_lba = dir_lba[d.parent];
        let self_len = sectors * LOGICAL_SECTOR_SIZE as u32;
        let parent_len = if d.parent == 0 {
            self_len.max(LOGICAL_SECTOR_SIZE as u32)
        } else {
            LOGICAL_SECTOR_SIZE as u32
        };

        // Joliet records never carry System Use data, on the measured discs
        // or in the standard's intent: Rock Ridge lives on the primary tree.
        let (skip, mut arena) = match rock {
            Some((skip, arena)) if !joliet => (skip as usize, Some(arena)),
            _ => (0, None),
        };
        let mut area_for = |node: Option<&RockRidge>| -> Vec<u8> {
            match (node, arena.as_deref_mut()) {
                (Some(rock), Some(arena)) => system_use_for(rock, skip, arena),
                _ => Vec::new(),
            }
        };

        let mut records: Vec<(Vec<u8>, Vec<u8>)> = Vec::new(); // (sort key, bytes)
        for &sub in &d.subdirs {
            let name = if joliet {
                &dirs[sub].joliet
            } else {
                &dirs[sub].iso
            };
            let id = encode_id(name, joliet);
            let su = area_for(dirs[sub].rock.as_ref());
            records.push((
                id.clone(),
                dir_record(&id, dir_lba[sub], LOGICAL_SECTOR_SIZE as u32, true, &su),
            ));
        }
        for &fi in &d.files {
            let f = &files[fi];
            let name = if joliet { &f.joliet } else { &f.iso };
            let id = encode_id(&format!("{name};1"), joliet);
            let su = area_for(f.rock.as_ref());
            records.push((
                id.clone(),
                dir_record(&id, f.extent, f.data.len() as u32, false, &su),
            ));
        }
        records.sort_by(|a, b| a.0.cmp(&b.0));

        // `SP` goes in the `.` record of the *root* only, and unskipped: it
        // is the entry that declares the skip, so it cannot sit behind it.
        let dot_area: Vec<u8> = if arena.is_some() && index == 0 {
            vec![b'S', b'P', 7, 1, 0xBE, 0xEF, skip as u8]
        } else {
            Vec::new()
        };

        let mut buf = vec![0u8; sectors as usize * LOGICAL_SECTOR_SIZE];
        let mut pos = 0usize;
        for r in [
            dir_record(&[0x00], self_lba, self_len, true, &dot_area),
            dir_record(&[0x01], parent_lba, parent_len, true, &[]),
        ] {
            buf[pos..pos + r.len()].copy_from_slice(&r);
            pos += r.len();
        }

        for (i, (_, bytes)) in records.iter().enumerate() {
            // With `split` set, everything after the first entry moves to the
            // second sector, leaving the first one padded with zeros.
            if split && i == 1 {
                pos = LOGICAL_SECTOR_SIZE;
            }
            let sector_end = (pos / LOGICAL_SECTOR_SIZE + 1) * LOGICAL_SECTOR_SIZE;
            assert!(
                pos + bytes.len() <= sector_end && pos + bytes.len() <= buf.len(),
                "fixture directory does not fit its sectors"
            );
            buf[pos..pos + bytes.len()].copy_from_slice(bytes);
            pos += bytes.len();
        }
        buf
    }

    fn encode_id(name: &str, joliet: bool) -> Vec<u8> {
        if joliet {
            name.encode_utf16().flat_map(|u| u.to_be_bytes()).collect()
        } else {
            name.bytes().collect()
        }
    }

    fn path_table(dirs: &[FlatDir], dir_lba: &[u32], big_endian: bool, joliet: bool) -> Vec<u8> {
        let mut out = Vec::new();
        for (i, d) in dirs.iter().enumerate() {
            let id: Vec<u8> = if i == 0 {
                vec![0x00]
            } else {
                encode_id(if joliet { &d.joliet } else { &d.iso }, joliet)
            };
            let parent = (d.parent + 1) as u16;
            out.push(id.len() as u8);
            out.push(0);
            if big_endian {
                out.extend_from_slice(&dir_lba[i].to_be_bytes());
                out.extend_from_slice(&parent.to_be_bytes());
            } else {
                out.extend_from_slice(&dir_lba[i].to_le_bytes());
                out.extend_from_slice(&parent.to_le_bytes());
            }
            out.extend_from_slice(&id);
            if id.len() % 2 == 1 {
                out.push(0);
            }
        }
        out
    }

    fn terminator() -> Vec<u8> {
        let mut s = vec![0u8; LOGICAL_SECTOR_SIZE];
        s[0] = 255;
        s[1..6].copy_from_slice(b"CD001");
        s[6] = 1;
        s
    }

    #[allow(clippy::too_many_arguments)]
    fn volume_descriptor(
        kind: u8,
        volume: &str,
        joliet: bool,
        total_sectors: u32,
        path_table_size: u32,
        l_path_lba: u32,
        m_path_lba: u32,
        root_lba: u32,
        root_len: u32,
    ) -> Vec<u8> {
        let mut s = vec![0u8; LOGICAL_SECTOR_SIZE];
        s[0] = kind;
        s[1..6].copy_from_slice(b"CD001");
        s[6] = 1;
        pad_ascii(&mut s[8..40], "");
        if joliet {
            pad_ucs2(&mut s[8..40], "");
            pad_ucs2(&mut s[40..72], volume);
            // UCS-2 level 3.
            s[88..91].copy_from_slice(b"%/E");
        } else {
            pad_ascii(&mut s[40..72], volume);
        }
        s[80..88].copy_from_slice(&both32(total_sectors));
        s[120..124].copy_from_slice(&both16(1));
        s[124..128].copy_from_slice(&both16(1));
        s[128..132].copy_from_slice(&both16(LOGICAL_SECTOR_SIZE as u16));
        s[132..140].copy_from_slice(&both32(path_table_size));
        s[140..144].copy_from_slice(&l_path_lba.to_le_bytes());
        s[148..152].copy_from_slice(&m_path_lba.to_be_bytes());
        let root = dir_record(&[0x00], root_lba, root_len, true, &[]);
        s[156..156 + root.len()].copy_from_slice(&root);

        let filler: &[(usize, usize)] = &[
            (190, 128), // volume set identifier
            (318, 128), // publisher
            (446, 128), // data preparer
            (574, 128), // application
            (702, 37),  // copyright file
            (739, 37),  // abstract file
            (776, 37),  // bibliographic file
        ];
        for &(at, len) in filler {
            if joliet {
                pad_ucs2(&mut s[at..at + len], "");
            } else {
                pad_ascii(&mut s[at..at + len], "");
            }
        }

        let stamp = b"1994051708300000";
        for at in [813usize, 830, 847, 864] {
            s[at..at + 16].copy_from_slice(stamp);
            s[at + 16] = 0;
        }
        s[881] = 1; // file structure version
        s
    }

    /// Wrap 2048-byte sectors in raw 2352-byte ones: 12 bytes of sync, a
    /// 4-byte header, the data, then the rest where EDC/ECC would be.
    ///
    /// `layout` decides which raw shape:
    ///
    /// - [`SectorLayout::Raw2352`] — Mode 1, data at offset 16.
    /// - [`SectorLayout::Raw2352Xa`] — Mode 2 Form 1: an 8-byte subheader
    ///   (file, channel, submode, coding, written twice) after the header, so
    ///   the data starts at 24. The submode says "data, Form 1"; a fixture
    ///   that wants Form 2 flips [`XA_SUBMODE_FORM2`] into the byte at
    ///   [`XA_SUBMODE_OFFSET`] itself, since ART must refuse such a track.
    ///
    /// The parity bytes are left zero. Nothing ART reads looks at them, and a
    /// raw image is a track dump rather than something a host mounts.
    ///
    /// [`XA_SUBMODE_FORM2`]: super::descriptor::XA_SUBMODE_FORM2
    /// [`XA_SUBMODE_OFFSET`]: super::descriptor::XA_SUBMODE_OFFSET
    fn to_raw(cooked: &[u8], layout: SectorLayout) -> Vec<u8> {
        let xa = layout == SectorLayout::Raw2352Xa;
        let data_at = if xa { 24 } else { 16 };
        let sectors = cooked.len() / LOGICAL_SECTOR_SIZE;
        let mut out = vec![0u8; sectors * RAW_SECTOR_SIZE];
        for i in 0..sectors {
            let base = i * RAW_SECTOR_SIZE;
            out[base] = 0x00;
            for b in out[base + 1..base + 11].iter_mut() {
                *b = 0xFF;
            }
            out[base + 11] = 0x00;
            // MSF address, BCD, with the 150-sector lead-in.
            let abs = i + 150;
            let bcd = |v: usize| ((v / 10) << 4 | (v % 10)) as u8;
            out[base + 12] = bcd(abs / (60 * 75));
            out[base + 13] = bcd((abs / 75) % 60);
            out[base + 14] = bcd(abs % 75);
            out[base + 15] = if xa { 0x02 } else { 0x01 };
            if xa {
                // file, channel, submode (0x08 = data), coding — twice.
                out[base + 18] = 0x08;
                out[base + 22] = 0x08;
            }
            out[base + data_at..base + data_at + LOGICAL_SECTOR_SIZE]
                .copy_from_slice(&cooked[i * LOGICAL_SECTOR_SIZE..(i + 1) * LOGICAL_SECTOR_SIZE]);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::{dir, file, rock_dir, rock_file, IsoBuilder, RockRidge};
    use super::*;
    use std::fs;

    /// A scratch directory no other test can be handed.
    ///
    /// The process id and a nanosecond stamp are **not** enough: two threads
    /// entering here close enough together get the same name, and then one
    /// test reads another's disc. That is measured, not hypothetical — 5
    /// failures across 4 different tests in 40 runs, one of them comparing
    /// an accented volume name against a different fixture's, which is two
    /// discs meeting in one directory (ART-164). The counter is what makes
    /// the name unique; the stamp only makes it readable.
    fn tmp() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let d = std::env::temp_dir().join(format!(
            "art-iso-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Write an image to a temp file and return (dir, path). The dir is kept
    /// alive by the caller so the file survives until the test ends.
    fn write_image(bytes: &[u8]) -> (PathBuf, PathBuf) {
        let d = tmp();
        let p = d.join("disc.iso");
        fs::write(&p, bytes).unwrap();
        (d, p)
    }

    /// A small disc: two files at the root and one subdirectory holding one.
    fn sample_builder(layout: SectorLayout, joliet: bool) -> IsoBuilder {
        IsoBuilder {
            volume: "AMIGA_TEST".to_string(),
            joliet_volume: "Amiga Tëst".to_string(),
            joliet,
            layout,
            split_root: false,
            rock_ridge: false,
            susp_skip: 0,
            children: vec![
                file("README.TXT", "ReadMe.txt", b"Hello from the disc.\n"),
                file("STARTUP", "Startup-Sequence", b"echo hello\n"),
                dir(
                    "TOOLS",
                    "Tools",
                    vec![file("SHELL.LHA", "Shëll.lha", b"not really an archive")],
                ),
            ],
        }
    }

    /// A disc shaped like the owner's own AmigaOS 3.9 CD, as
    /// `scripts/iso-susp-census.py` measured it: no Joliet descriptor, an
    /// `SP` entry in the root, and `RR`/`PX`/`NM`/`AS` on every record.
    ///
    /// The uppercase 8.3 identifiers and the mixed-case Rock Ridge names are
    /// deliberately different, because that difference *is* ART-078's second
    /// consequence — a fixture whose two names matched would pass whether or
    /// not `NM` was read at all.
    fn amiga_rock_ridge_builder() -> IsoBuilder {
        IsoBuilder {
            volume: "AMIGAOS39".to_string(),
            joliet: false,
            rock_ridge: true,
            children: vec![
                rock_file(
                    "MYGAME.INF",
                    b"icon",
                    RockRidge {
                        name: "MyGame.info".to_string(),
                        // `0x02` — the commonest value on both AS-carrying
                        // discs: `e` set, so not executable.
                        protection: Some(0x0000_0002),
                        ..Default::default()
                    },
                ),
                rock_file(
                    "STARTUP.SEQ",
                    b"Resident C:Assign PURE\n",
                    RockRidge {
                        name: "Startup-Sequence".to_string(),
                        // `s` (script), and `e` clear in the same byte.
                        protection: Some(0x0000_0040),
                        comment: "the boot script".to_string(),
                        ..Default::default()
                    },
                ),
                rock_file(
                    "ASSIGN",
                    b"binary",
                    RockRidge {
                        name: "Assign".to_string(),
                        // `p` (pure): what `Resident C:Assign PURE` needs.
                        protection: Some(0x0000_0020),
                        ..Default::default()
                    },
                ),
                rock_dir(
                    "GAMES",
                    vec![rock_file(
                        "SLAVE.SLV",
                        b"slave",
                        RockRidge {
                            name: "Game.slave".to_string(),
                            protection: Some(0x0000_0060),
                            comment: "a comment that is split in two".to_string(),
                            // Through a `CE` continuation area, which no
                            // measured disc uses but the standard allows.
                            continue_in_ce: true,
                        },
                    )],
                    RockRidge {
                        name: "Games".to_string(),
                        protection: Some(0),
                        ..Default::default()
                    },
                ),
            ],
            ..sample_builder(SectorLayout::Cooked, false)
        }
    }

    #[test]
    fn a_rock_ridge_disc_reads_its_real_names_not_its_8_3_ones() {
        let (d, p) = write_image(&amiga_rock_ridge_builder().build());
        let iso = IsoImage::open(&p).unwrap();
        let (extent, length) = iso.root();
        let names: Vec<String> = iso
            .list(extent, length)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert!(names.contains(&"MyGame.info".to_string()), "{names:?}");
        assert!(names.contains(&"Startup-Sequence".to_string()), "{names:?}");
        assert!(names.contains(&"Games".to_string()), "{names:?}");
        // And the 8.3 spelling is gone, not merely joined by the real one.
        assert!(!names.contains(&"MYGAME.INF".to_string()), "{names:?}");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_rock_ridge_disc_carries_its_amiga_protection_bits_and_comment() {
        use crate::core::volume::write::uaem::format_bits;
        let (d, p) = write_image(&amiga_rock_ridge_builder().build());
        let iso = IsoImage::open(&p).unwrap();
        let (extent, length) = iso.root();
        let entries = iso.list(extent, length).unwrap();

        let by = |name: &str| entries.iter().find(|e| e.name == name).unwrap().clone();
        assert_eq!(
            format_bits(by("MyGame.info").protection.unwrap()),
            "----rw-d"
        );
        let startup = by("Startup-Sequence");
        assert_eq!(format_bits(startup.protection.unwrap()), "-s--rwed");
        assert_eq!(startup.comment.as_deref(), Some("the boot script"));
        assert_eq!(format_bits(by("Assign").protection.unwrap()), "--p-rwed");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_name_and_comment_split_across_a_ce_continuation_are_joined() {
        let (d, p) = write_image(&amiga_rock_ridge_builder().build());
        let iso = IsoImage::open(&p).unwrap();
        let (extent, length) = iso.root();
        let games = iso
            .list(extent, length)
            .unwrap()
            .into_iter()
            .find(|e| e.name == "Games")
            .unwrap();
        let inside = iso.list(games.extent, LOGICAL_SECTOR_SIZE as u32).unwrap();
        assert_eq!(inside.len(), 1, "{inside:?}");
        // Both halves live in a continuation area in another block; reading
        // only the inline half would give "Game." and "a comment that is".
        assert_eq!(inside[0].name, "Game.slave");
        assert_eq!(
            inside[0].comment.as_deref(),
            Some("a comment that is split in two")
        );
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_disc_with_no_sp_entry_is_not_parsed_as_rock_ridge() {
        // The same tree with the `SP` entry withheld, which is what a disc
        // that never declared SUSP looks like. ART must read the 8.3 names
        // and no protection bits.
        let mut builder = amiga_rock_ridge_builder();
        builder.rock_ridge = false;
        let (d, p) = write_image(&builder.build());
        let iso = IsoImage::open(&p).unwrap();
        let (extent, length) = iso.root();
        let entries = iso.list(extent, length).unwrap();
        assert!(
            entries.iter().all(|e| e.protection.is_none()),
            "{entries:?}"
        );
        assert!(
            entries.iter().any(|e| e.name == "MYGAME.INF"),
            "{entries:?}"
        );
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_sp_skip_is_honoured_rather_than_assumed_to_be_zero() {
        // Every measured disc declares skip 0, so a reader that ignored the
        // field entirely would pass every test written from real material.
        // This disc declares 4, which is legal and which nothing else here
        // would catch.
        let mut builder = amiga_rock_ridge_builder();
        builder.susp_skip = 4;
        let (d, p) = write_image(&builder.build());
        let iso = IsoImage::open(&p).unwrap();
        let (extent, length) = iso.root();
        let entries = iso.list(extent, length).unwrap();
        let assign = entries.iter().find(|e| e.name == "Assign").unwrap();
        assert_eq!(assign.protection, Some(0x20));
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_disc_sourced_file_carries_its_bits_into_a_uaem_sidecar() {
        use crate::core::jobs::NoProgress;
        let (d, p) = write_image(&amiga_rock_ridge_builder().build());
        let iso = IsoImage::open(&p).unwrap();
        let (extent, length) = iso.root();
        let out = d.join("out");
        let report = iso
            .extract_tree(extent, length, &out, OverwritePolicy::Skip, &NoProgress)
            .unwrap();

        // Every file here carries either non-default bits, a comment, or a
        // date; `sidecar_for` is what declines to write one for a file with
        // none of the three.
        assert_eq!(report.sidecars_written, report.files_written, "{report:?}");
        let sidecar = fs::read_to_string(out.join("Startup-Sequence.uaem")).unwrap();
        assert!(sidecar.starts_with("-s--rwed "), "{sidecar}");
        assert!(sidecar.trim_end().ends_with("the boot script"), "{sidecar}");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_disc_sourced_file_carries_its_bits_into_an_amiga_volume() {
        use crate::core::volume::write::copy::CopySource;
        let (d, p) = write_image(&amiga_rock_ridge_builder().build());
        let iso = IsoImage::open(&p).unwrap();
        let (extent, length) = iso.root();
        let source = IsoSource::new(iso, extent, length).unwrap();

        // This is the path a WHDLoad slave takes off a CD and onto an HDF:
        // `copy.rs` turns this `Sidecar` into the `FileMeta` the volume
        // writer stores, so `s` and `p` surviving here is `s` and `p`
        // surviving the copy (ART-078).
        let startup = source.metadata("Startup-Sequence").unwrap().unwrap();
        assert_eq!(startup.protection, 0x40);
        assert_eq!(startup.comment, "the boot script");
        let assign = source.metadata("Assign").unwrap().unwrap();
        assert_eq!(assign.protection, 0x20);
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_disc_with_no_amiga_metadata_still_writes_no_sidecars() {
        // The guard the sidecar test needs to not be vacuous: an ordinary
        // ISO9660 disc must produce the same files and no `.uaem` at all.
        // The fixture gives every record a date, so this also pins that a
        // date alone is not treated as Amiga metadata worth a sidecar.
        use crate::core::jobs::NoProgress;
        let (d, p) = write_image(&sample_builder(SectorLayout::Cooked, false).build());
        let iso = IsoImage::open(&p).unwrap();
        let (extent, length) = iso.root();
        let out = d.join("out");
        let report = iso
            .extract_tree(extent, length, &out, OverwritePolicy::Skip, &NoProgress)
            .unwrap();
        assert!(report.files_written > 0);
        assert_eq!(report.sidecars_written, 0, "{report:?}");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_minimal_iso_reports_its_volume_name_and_root() {
        let (d, p) = write_image(&sample_builder(SectorLayout::Cooked, false).build());
        let iso = IsoImage::open(&p).unwrap();
        assert_eq!(iso.volume_name(), "AMIGA_TEST");
        assert_eq!(iso.layout(), SectorLayout::Cooked);
        assert!(!iso.is_joliet());
        let (extent, length) = iso.root();
        assert!(extent >= 16, "root must be past the descriptors");
        assert_eq!(length, LOGICAL_SECTOR_SIZE as u32);
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_directory_lists_its_entries_without_dot_and_dotdot() {
        let (d, p) = write_image(&sample_builder(SectorLayout::Cooked, false).build());
        let iso = IsoImage::open(&p).unwrap();
        let (extent, length) = iso.root();
        let entries = iso.list(extent, length).unwrap();

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["README.TXT", "STARTUP", "TOOLS"]);
        assert!(!names.iter().any(|n| *n == "." || *n == ".."));

        let tools = entries.iter().find(|e| e.name == "TOOLS").unwrap();
        assert!(tools.is_dir);
        let readme = entries.iter().find(|e| e.name == "README.TXT").unwrap();
        assert!(!readme.is_dir);
        assert_eq!(readme.bytes, 21);
        // 1994-05-17 08:30:00 UTC, the fixture's recording date.
        assert_eq!(readme.date, Some(769_163_400));

        let inner = iso.list(tools.extent, tools.bytes as u32).unwrap();
        assert_eq!(inner.len(), 1);
        assert_eq!(inner[0].name, "SHELL.LHA");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_file_reads_back_byte_for_byte() {
        // Deliberately more than one sector, and not a whole number of them,
        // so the tail of the last sector is excluded.
        let payload: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        let builder = IsoBuilder {
            children: vec![
                file("BIG.DAT", "Big.dat", &payload),
                file("EMPTY.DAT", "Empty.dat", b""),
            ],
            ..Default::default()
        };
        let (d, p) = write_image(&builder.build());
        let iso = IsoImage::open(&p).unwrap();
        let (extent, length) = iso.root();
        let entries = iso.list(extent, length).unwrap();

        let big = entries.iter().find(|e| e.name == "BIG.DAT").unwrap();
        assert_eq!(big.bytes, 5000);
        assert_eq!(iso.read_file(big.extent, big.bytes).unwrap(), payload);

        let empty = entries.iter().find(|e| e.name == "EMPTY.DAT").unwrap();
        assert!(iso.read_file(empty.extent, empty.bytes).unwrap().is_empty());
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_joliet_disc_prefers_its_unicode_names() {
        let (d, p) = write_image(&sample_builder(SectorLayout::Cooked, true).build());
        let iso = IsoImage::open(&p).unwrap();
        assert!(iso.is_joliet());
        // The volume name comes from the Joliet descriptor too, and the
        // accented character survives — which it would not if the UCS-2 were
        // read as UTF-8 or as little-endian.
        assert_eq!(iso.volume_name(), "Amiga Tëst");

        let (extent, length) = iso.root();
        let names: Vec<String> = iso
            .list(extent, length)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert!(
            names.contains(&"Startup-Sequence".to_string()),
            "expected the Joliet names, got {names:?}"
        );
        assert!(names.contains(&"ReadMe.txt".to_string()), "{names:?}");
        assert!(
            !names.contains(&"README.TXT".to_string()),
            "the uppercase 8.3 names are the Primary tree's; Joliet must win"
        );

        // And the nested Joliet name, so this is not just the root.
        let tools = iso
            .list(extent, length)
            .unwrap()
            .into_iter()
            .find(|e| e.name == "Tools")
            .unwrap();
        let inner = iso.list(tools.extent, tools.bytes as u32).unwrap();
        assert_eq!(inner[0].name, "Shëll.lha");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_raw_2352_byte_image_reads_the_same_as_a_2048_byte_one() {
        let (d1, cooked_path) = write_image(&sample_builder(SectorLayout::Cooked, true).build());
        let (d2, raw_path) = write_image(&sample_builder(SectorLayout::Raw2352, true).build());

        let cooked = IsoImage::open(&cooked_path).unwrap();
        let raw = IsoImage::open(&raw_path).unwrap();
        assert_eq!(cooked.layout(), SectorLayout::Cooked);
        assert_eq!(raw.layout(), SectorLayout::Raw2352);
        // The raw file is bigger for the same contents — proof the wrapper
        // bytes are really there and really being skipped.
        assert!(fs::metadata(&raw_path).unwrap().len() > fs::metadata(&cooked_path).unwrap().len());

        assert_eq!(cooked.volume_name(), raw.volume_name());
        assert_eq!(cooked.root(), raw.root());

        let (e, l) = cooked.root();
        let a = cooked.list(e, l).unwrap();
        let b = raw.list(e, l).unwrap();
        assert_eq!(a, b);

        let f = a.iter().find(|x| x.name == "ReadMe.txt").unwrap();
        assert_eq!(
            cooked.read_file(f.extent, f.bytes).unwrap(),
            raw.read_file(f.extent, f.bytes).unwrap()
        );
        fs::remove_dir_all(&d1).ok();
        fs::remove_dir_all(&d2).ok();
    }

    #[test]
    fn a_record_length_of_zero_moves_to_the_next_sector_rather_than_looping() {
        // `split_root` leaves the rest of the first sector zero-filled after
        // one entry and puts the others in the second. A reader that treated
        // the zero length as a step of zero would hang here; one that treated
        // it as "stop" would return a single entry.
        let builder = IsoBuilder {
            split_root: true,
            ..sample_builder(SectorLayout::Cooked, false)
        };
        let (d, p) = write_image(&builder.build());
        let iso = IsoImage::open(&p).unwrap();
        let (extent, length) = iso.root();
        assert_eq!(length, 2 * LOGICAL_SECTOR_SIZE as u32);

        let names: Vec<String> = iso
            .list(extent, length)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, ["README.TXT", "STARTUP", "TOOLS"]);
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_directory_claiming_a_length_past_the_end_of_the_file_is_an_error() {
        let (d, p) = write_image(&sample_builder(SectorLayout::Cooked, false).build());
        let image_len = fs::metadata(&p).unwrap().len();
        let iso = IsoImage::open(&p).unwrap();
        let (extent, _) = iso.root();

        // Well under the directory cap, well past the end of a ~100 KB file.
        let claimed = 1024 * 1024u32;
        assert!(claimed as u64 > image_len);
        let err = iso.list(extent, claimed).unwrap_err();
        assert!(
            err.to_string().contains("past the end of the image"),
            "{err}"
        );
        assert_eq!(err.code(), "ART-FORMAT-MALFORMED");

        // And the same for a file: the length is refused before anything is
        // allocated for it.
        let err = iso.read_file(extent, 900_000_000).unwrap_err();
        assert_eq!(err.code(), "ART-INPUT-INVALID");
        let err = iso.read_file(extent, image_len + 1).unwrap_err();
        assert!(
            err.to_string().contains("past the end of the image"),
            "{err}"
        );
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_deeply_nested_disc_stops_at_the_depth_limit() {
        // Forty levels, far past MAX_WALK_DEPTH.
        let mut node = dir("L40", "L40", vec![file("LEAF.TXT", "Leaf.txt", b"deep")]);
        for level in (1..40).rev() {
            node = dir(&format!("L{level}"), &format!("L{level}"), vec![node]);
        }
        let builder = IsoBuilder {
            children: vec![node],
            ..Default::default()
        };
        let (d, p) = write_image(&builder.build());
        let iso = IsoImage::open(&p).unwrap();

        let walk = iso.walk().unwrap();
        assert!(walk.depth_limited, "the depth cap should have been reached");
        assert!(!walk.truncated);
        let deepest = walk
            .entries
            .iter()
            .map(|e| e.path.split('/').count())
            .max()
            .unwrap();
        assert_eq!(
            deepest, MAX_WALK_DEPTH,
            "the walk went deeper than MAX_WALK_DEPTH"
        );
        // The leaf is 40 levels down, so it must not appear.
        assert!(!walk.entries.iter().any(|e| e.entry.name == "LEAF.TXT"));
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_walk_finds_every_nested_entry_of_a_shallow_disc() {
        let (d, p) = write_image(&sample_builder(SectorLayout::Cooked, false).build());
        let iso = IsoImage::open(&p).unwrap();
        let walk = iso.walk().unwrap();
        assert!(!walk.depth_limited);
        assert!(!walk.truncated);
        let mut paths: Vec<&str> = walk.entries.iter().map(|e| e.path.as_str()).collect();
        paths.sort_unstable();
        assert_eq!(paths, ["README.TXT", "STARTUP", "TOOLS", "TOOLS/SHELL.LHA"]);
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn extract_tree_writes_the_disc_out_to_a_host_folder() {
        use crate::core::jobs::NoProgress;

        let (d, p) = write_image(&sample_builder(SectorLayout::Cooked, false).build());
        let iso = IsoImage::open(&p).unwrap();
        let dest = tmp();
        let (extent, length) = iso.root();

        let report = iso
            .extract_tree(extent, length, &dest, OverwritePolicy::Skip, &NoProgress)
            .unwrap();
        assert_eq!(report.files_written, 3, "{report:?}");
        assert_eq!(report.directories_created, 1, "TOOLS");
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);

        assert_eq!(
            fs::read(dest.join("README.TXT")).unwrap(),
            b"Hello from the disc.\n"
        );
        assert_eq!(
            fs::read(dest.join("TOOLS").join("SHELL.LHA")).unwrap(),
            b"not really an archive"
        );
        fs::remove_dir_all(&d).ok();
        fs::remove_dir_all(&dest).ok();
    }

    #[test]
    fn extract_tree_reports_a_bad_extent_instead_of_panicking() {
        use crate::core::jobs::NoProgress;

        let (d, p) = write_image(&sample_builder(SectorLayout::Cooked, false).build());
        let iso = IsoImage::open(&p).unwrap();
        let dest = tmp();

        let err = iso
            .extract_tree(
                900_000,
                LOGICAL_SECTOR_SIZE as u32,
                &dest,
                OverwritePolicy::Skip,
                &NoProgress,
            )
            .unwrap_err();
        assert_eq!(err.code(), "ART-FORMAT-MALFORMED");
        fs::remove_dir_all(&d).ok();
        fs::remove_dir_all(&dest).ok();
    }

    /// The divergence finding 3 of the Task 3 review names: copying out of an
    /// ADF honoured the user's collision policy and copying out of a disc did
    /// not, because the disc had a second copy of the same loop with `Skip`
    /// written into it. Both now go through `host_target`, so this is the
    /// same assertion `extract_from_volume`'s overwrite test makes.
    #[test]
    fn extract_tree_honours_the_users_overwrite_policy() {
        use crate::core::jobs::NoProgress;

        let (d, p) = write_image(&sample_builder(SectorLayout::Cooked, false).build());
        let iso = IsoImage::open(&p).unwrap();
        let dest = tmp();
        let (extent, length) = iso.root();

        // Something already standing where README.TXT wants to go.
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("README.TXT"), b"an older copy").unwrap();

        let skipped = iso
            .extract_tree(extent, length, &dest, OverwritePolicy::Skip, &NoProgress)
            .unwrap();
        assert_eq!(
            fs::read(dest.join("README.TXT")).unwrap(),
            b"an older copy",
            "Skip must leave what is already there"
        );
        assert!(
            skipped.skipped.iter().any(|s| s.contains("README.TXT")),
            "{:?}",
            skipped.skipped
        );

        let overwritten = iso
            .extract_tree(
                extent,
                length,
                &dest,
                OverwritePolicy::Overwrite,
                &NoProgress,
            )
            .unwrap();
        assert_eq!(
            fs::read(dest.join("README.TXT")).unwrap(),
            b"Hello from the disc.\n",
            "Overwrite must replace it — the setting a disc used to ignore"
        );
        assert!(
            !overwritten.skipped.iter().any(|s| s.contains("README.TXT")),
            "{:?}",
            overwritten.skipped
        );

        fs::remove_dir_all(&d).ok();
        fs::remove_dir_all(&dest).ok();
    }

    /// `walk_subtree` has kept a `visited` set since it was written, because a
    /// record pointing at an ancestor is "a legal-looking record and an
    /// endless tree". `extract_tree` had only `MAX_WALK_DEPTH`, which does not
    /// bound anything: a root whose directories point back at it branches at
    /// every level, so sixteen levels is exponential, not linear. This is the
    /// same fixture `a_directory_that_points_back_at_its_parent_does_not_walk_forever`
    /// uses, pointed at the extraction path instead of the walk.
    #[test]
    fn extract_tree_does_not_follow_a_directory_that_points_back_at_the_root() {
        use crate::core::jobs::NoProgress;

        let builder = IsoBuilder {
            children: vec![dir("SUB", "Sub", vec![file("A.TXT", "a.txt", b"x")])],
            ..Default::default()
        };
        let mut bytes = builder.build();

        let pvd = 16 * LOGICAL_SECTOR_SIZE;
        let root_lba = u32::from_le_bytes([
            bytes[pvd + 158],
            bytes[pvd + 159],
            bytes[pvd + 160],
            bytes[pvd + 161],
        ]);
        let root_at = root_lba as usize * LOGICAL_SECTOR_SIZE;
        let sub = find_record(&bytes[root_at..root_at + LOGICAL_SECTOR_SIZE], b"SUB")
            .expect("the fixture should contain a SUB record");
        bytes[root_at + sub + 2..root_at + sub + 6].copy_from_slice(&root_lba.to_le_bytes());

        let (d, p) = write_image(&bytes);
        let iso = IsoImage::open(&p).unwrap();
        let dest = tmp();
        let (extent, length) = iso.root();

        let report = iso
            .extract_tree(extent, length, &dest, OverwritePolicy::Skip, &NoProgress)
            .unwrap();

        // SUB is created once and not descended into: without the guard the
        // same directory is re-listed at every level down to the depth cap.
        assert!(
            report.directories_created <= 1,
            "the cycle was followed: {report:?}"
        );
        assert!(
            report
                .skipped
                .iter()
                .any(|s| s.contains("points back at one ART has already written")),
            "the user is told why, not left with a silently short copy: {:?}",
            report.skipped
        );
        assert!(
            !dest.join("SUB").join("SUB").exists(),
            "a second level of the cycle reached the disk"
        );

        fs::remove_dir_all(&d).ok();
        fs::remove_dir_all(&dest).ok();
    }

    /// One selected *file* copied disc → volume must copy that file, not the
    /// directory it happened to be sitting in (finding 2 of the Task 3
    /// review: an install CD's root is hundreds of megabytes, and the status
    /// line named a single file while all of it went across).
    #[test]
    fn a_single_file_source_carries_exactly_that_one_file() {
        let (d, p) = write_image(&sample_builder(SectorLayout::Cooked, false).build());
        let iso = IsoImage::open(&p).unwrap();
        let (extent, length) = iso.root();
        let readme = iso
            .list(extent, length)
            .unwrap()
            .into_iter()
            .find(|e| e.name == "README.TXT")
            .unwrap();

        let source = IsoSource::single_file(
            iso,
            &readme.name,
            readme.extent,
            readme.bytes,
            readme.date,
            None,
        );
        let entries = source.entries().unwrap();
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].relative, "README.TXT");
        assert!(!entries[0].is_dir);
        assert_eq!(entries[0].bytes, readme.bytes);
        assert_eq!(
            source.read("README.TXT").unwrap(),
            b"Hello from the disc.\n"
        );
        // And the recording date still reaches the volume.
        assert!(source.metadata("README.TXT").unwrap().is_some());

        fs::remove_dir_all(&d).ok();
    }

    /// The claim Task 3 exists to prove: an `IsoSource` needs no copy engine
    /// of its own. It answers `CopySource`'s three questions and the disc's
    /// contents land on an Amiga volume through the one tested
    /// `copy_into_volume`, unchanged.
    #[test]
    fn iso_source_copies_a_disc_into_an_amiga_volume_through_the_shared_copy_engine() {
        use crate::core::jobs::NoProgress;
        use crate::core::lha::OverwritePolicy;
        use crate::core::volume::device::FileRegionMut;
        use crate::core::volume::fixture::ffs_volume;
        use crate::core::volume::write::copy::copy_into_volume;
        use crate::core::volume::write::VolumeWriter;
        use crate::core::volume::DosType;

        let (d, p) = write_image(&sample_builder(SectorLayout::Cooked, false).build());
        let iso = IsoImage::open(&p).unwrap();
        let (extent, length) = iso.root();
        let source = IsoSource::new(iso, extent, length).unwrap();

        let vol_dir = tmp();
        let image_path = vol_dir.join("disk.adf");
        let (bytes, geometry) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        fs::write(&image_path, &bytes).unwrap();

        let mut device = FileRegionMut::open(&image_path, 0, geometry.total_bytes(), 512).unwrap();
        let report = {
            let mut writer = VolumeWriter::open(&mut device, geometry, &image_path, 0).unwrap();
            copy_into_volume(&mut writer, 0, &source, OverwritePolicy::Skip, &NoProgress).unwrap()
        };
        drop(device);

        assert!(report.is_complete(), "{report:?}");
        assert_eq!(report.files_copied, 3, "README.TXT, STARTUP, SHELL.LHA");
        assert_eq!(report.files_verified, 3);
        assert_eq!(report.directories_created, 1, "TOOLS");

        fs::remove_dir_all(&d).ok();
        fs::remove_dir_all(&vol_dir).ok();
    }

    #[test]
    fn a_directory_that_points_back_at_its_parent_does_not_walk_forever() {
        let builder = IsoBuilder {
            children: vec![dir("SUB", "Sub", vec![file("A.TXT", "a.txt", b"x")])],
            ..Default::default()
        };
        let mut bytes = builder.build();

        // Rewrite SUB's extent so it points at the root: a record that looks
        // entirely legal and describes a tree with no bottom.
        let pvd = 16 * LOGICAL_SECTOR_SIZE;
        let root_lba = u32::from_le_bytes([
            bytes[pvd + 158],
            bytes[pvd + 159],
            bytes[pvd + 160],
            bytes[pvd + 161],
        ]);
        let root_at = root_lba as usize * LOGICAL_SECTOR_SIZE;
        let sub = find_record(&bytes[root_at..root_at + LOGICAL_SECTOR_SIZE], b"SUB")
            .expect("the fixture should contain a SUB record");
        bytes[root_at + sub + 2..root_at + sub + 6].copy_from_slice(&root_lba.to_le_bytes());

        let (d, p) = write_image(&bytes);
        let iso = IsoImage::open(&p).unwrap();
        let walk = iso.walk().unwrap();
        // The cycle is cut by the visited set, so the walk returns rather
        // than spending sixteen levels re-listing the root.
        assert!(walk.entries.iter().any(|e| e.entry.name == "SUB"));
        assert!(walk.entries.len() < 10, "{:?}", walk.entries);
        fs::remove_dir_all(&d).ok();
    }

    /// Offset of the first directory record in `sector` whose identifier
    /// starts with `name`.
    fn find_record(sector: &[u8], name: &[u8]) -> Option<usize> {
        let mut pos = 0usize;
        while pos < sector.len() {
            let len = sector[pos] as usize;
            if len == 0 {
                return None;
            }
            let id_len = sector[pos + 32] as usize;
            if sector[pos + 33..pos + 33 + id_len].starts_with(name) {
                return Some(pos);
            }
            pos += len;
        }
        None
    }

    #[test]
    fn an_extent_outside_the_image_is_an_error_not_a_read_of_garbage() {
        let (d, p) = write_image(&sample_builder(SectorLayout::Cooked, false).build());
        let iso = IsoImage::open(&p).unwrap();
        let err = iso.list(900_000, LOGICAL_SECTOR_SIZE as u32).unwrap_err();
        assert!(
            err.to_string().contains("past the end of the image"),
            "{err}"
        );
        let err = iso.read_file(u32::MAX, 2048).unwrap_err();
        assert_eq!(err.code(), "ART-FORMAT-MALFORMED");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_directory_larger_than_art_reads_is_refused() {
        let (d, p) = write_image(&sample_builder(SectorLayout::Cooked, false).build());
        let iso = IsoImage::open(&p).unwrap();
        let err = iso.list(20, u32::MAX).unwrap_err();
        assert!(err.to_string().contains("ART reads at most"), "{err}");
        // Zero is not a directory either.
        assert!(iso.list(20, 0).is_err());
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_file_that_is_not_a_disc_is_rejected_with_a_sentence() {
        let d = tmp();
        let p = d.join("notadisc.iso");
        fs::write(&p, vec![0u8; 200_000]).unwrap();
        let err = IsoImage::open(&p).unwrap_err();
        assert_eq!(err.code(), "ART-FORMAT-UNSUPPORTED");
        assert!(err.to_string().contains("ISO9660"), "{err}");

        // A file too short to hold sector 16 at all.
        let tiny = d.join("tiny.iso");
        fs::write(&tiny, b"CD001").unwrap();
        assert!(IsoImage::open(&tiny).is_err());
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_descriptor_set_with_no_terminator_is_an_error() {
        let mut bytes = sample_builder(SectorLayout::Cooked, false).build();
        // Turn the terminator into another primary descriptor, and every
        // sector after it too, so the scan runs to its cap.
        for lba in 17..17 + MAX_DESCRIPTORS as u32 {
            let at = lba as usize * LOGICAL_SECTOR_SIZE;
            if at + LOGICAL_SECTOR_SIZE > bytes.len() {
                bytes.resize(at + LOGICAL_SECTOR_SIZE, 0);
            }
            bytes[at] = 0;
            bytes[at + 1..at + 6].copy_from_slice(b"CD001");
        }
        let (d, p) = write_image(&bytes);
        let err = IsoImage::open(&p).unwrap_err();
        assert!(err.to_string().contains("without a terminator"), "{err}");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_descriptor_that_loses_its_identifier_is_an_error() {
        let mut bytes = sample_builder(SectorLayout::Cooked, false).build();
        // Corrupt the terminator's magic — the descriptor run now ends in
        // something that is not a descriptor at all.
        let at = 17 * LOGICAL_SECTOR_SIZE;
        bytes[at + 1..at + 6].copy_from_slice(b"XXXXX");
        let (d, p) = write_image(&bytes);
        let err = IsoImage::open(&p).unwrap_err();
        assert!(err.to_string().contains("CD001"), "{err}");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_disc_with_no_joliet_falls_back_to_the_primary_names() {
        let (d, p) = write_image(&sample_builder(SectorLayout::Cooked, false).build());
        let iso = IsoImage::open(&p).unwrap();
        assert!(!iso.is_joliet());
        assert_eq!(iso.volume_name(), "AMIGA_TEST");
        fs::remove_dir_all(&d).ok();
    }

    /// ART-075. A Mode 2/XA Form 1 disc puts its user data eight bytes later
    /// than a Mode 1 one, and the reader takes that offset from the layout —
    /// so before this existed, detection and the reader were wrong *together*
    /// on such a disc, which is the one failure a green test suite cannot
    /// show you. CD32 and mixed-mode discs are written this way.
    #[test]
    fn an_xa_form1_disc_reads_exactly_as_a_mode1_one_does() {
        let (d1, mode1) = write_image(&sample_builder(SectorLayout::Raw2352, true).build());
        let (d2, xa) = write_image(&sample_builder(SectorLayout::Raw2352Xa, true).build());

        let a = IsoImage::open(&mode1).unwrap();
        let b = IsoImage::open(&xa).unwrap();
        assert_eq!(a.layout(), SectorLayout::Raw2352);
        assert_eq!(b.layout(), SectorLayout::Raw2352Xa, "the subheader is read");
        assert_eq!(a.volume_name(), b.volume_name());
        assert_eq!(a.root(), b.root());

        let walked_a = a.walk().unwrap().entries;
        let walked_b = b.walk().unwrap().entries;
        assert_eq!(walked_a, walked_b, "the same tree, eight bytes further in");

        // And the bytes, not just the listing: an eight-byte slip reads a
        // file that is almost right, which is worse than one that fails.
        for item in &walked_b {
            if !item.entry.is_dir {
                assert_eq!(
                    b.read_file(item.entry.extent, item.entry.bytes).unwrap(),
                    a.read_file(item.entry.extent, item.entry.bytes).unwrap(),
                    "{}",
                    item.path
                );
            }
        }

        fs::remove_dir_all(&d1).ok();
        fs::remove_dir_all(&d2).ok();
    }

    /// Mode 2 **Form 2** holds 2324 bytes of audio or video and no filesystem
    /// at all. Reading 2048 of them as a volume descriptor would produce
    /// confident nonsense, so the submode byte is asked rather than assumed.
    #[test]
    fn a_mode2_form2_track_is_refused_rather_than_misread() {
        let mut bytes = sample_builder(SectorLayout::Raw2352Xa, true).build();
        let submode = 16 * 2352 + descriptor::XA_SUBMODE_OFFSET;
        bytes[submode] |= descriptor::XA_SUBMODE_FORM2;

        let (d, p) = write_image(&bytes);
        let err = IsoImage::open(&p).unwrap_err();
        assert_eq!(err.code(), "ART-FORMAT-UNSUPPORTED", "{err}");
        assert!(err.to_string().contains("Form 2"), "{err}");

        // Told the layout directly rather than probing for it: the same
        // refusal, because the check belongs to opening, not to detection.
        let told = IsoImage::open_with_layout(&p, SectorLayout::Raw2352Xa).unwrap_err();
        assert_eq!(told.code(), "ART-FORMAT-UNSUPPORTED", "{told}");

        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn opening_with_a_layout_from_detection_matches_probing_for_one() {
        for layout in [
            SectorLayout::Cooked,
            SectorLayout::Raw2352,
            SectorLayout::Raw2352Xa,
        ] {
            let (d, p) = write_image(&sample_builder(layout, false).build());
            let probed = IsoImage::open(&p).unwrap();
            let told = IsoImage::open_with_layout(&p, layout).unwrap();
            assert_eq!(probed.layout(), told.layout());
            assert_eq!(probed.root(), told.root());
            assert_eq!(probed.volume_name(), told.volume_name());
            fs::remove_dir_all(&d).ok();
        }
    }

    #[test]
    fn detections_format_hints_open_the_right_layout() {
        // The contract with Task 1: what `detect` reports for an image is
        // what `SectorLayout::from_format_hint` turns back into a layout.
        for (layout, hint) in [
            (SectorLayout::Cooked, "iso9660"),
            (SectorLayout::Raw2352, "iso9660-raw"),
            (SectorLayout::Raw2352Xa, "iso9660-raw-xa"),
        ] {
            let (d, p) = write_image(&sample_builder(layout, false).build());
            let detection = crate::core::detect::detect(&p).unwrap();
            assert_eq!(detection.format_hint, hint);
            let from_hint = SectorLayout::from_format_hint(&detection.format_hint).unwrap();
            assert_eq!(from_hint, layout);
            assert_eq!(IsoImage::open(&p).unwrap().layout(), layout);
            fs::remove_dir_all(&d).ok();
        }
    }

    /// Write a synthetic disc out for an external check.
    ///
    /// Not an assertion about ART: a hook, in the shape `core/volume/mount.rs`
    /// already uses for the amitools oracle. ART's reader and its fixture
    /// builder were written from the same offsets, so they can agree with
    /// each other and both be wrong — the shape of ART-032 … ART-035. Setting
    /// `ART_ISO_OUT` writes a disc that an implementation sharing no code
    /// with this one can be pointed at:
    ///
    /// ```text
    /// ART_ISO_OUT=C:/temp/art.iso cargo test iso:: -- --nocapture
    /// powershell Mount-DiskImage -ImagePath C:\temp\art.iso
    /// ```
    ///
    /// `ART_ISO_PLAIN_OUT` writes the same disc without a Joliet descriptor,
    /// because a host that has one will read it and never look at the
    /// Primary tree's ISO 646 names — so both have to be offered separately.
    #[test]
    fn export_iso_for_oracle_when_asked() {
        fn disc(joliet: bool) -> Vec<u8> {
            IsoBuilder {
                volume: "AMIGA_TEST".to_string(),
                joliet_volume: "Amiga Tëst".to_string(),
                joliet,
                layout: SectorLayout::Cooked,
                split_root: false,
                rock_ridge: false,
                susp_skip: 0,
                children: vec![
                    file("README.TXT", "ReadMe.txt", b"Hello from the disc.\n"),
                    file("STARTUP", "Startup-Sequence", b"echo hello\n"),
                    dir(
                        "TOOLS",
                        "Tools",
                        vec![file("SHELL.LHA", "Shell.lha", b"not really an archive")],
                    ),
                ],
            }
            .build()
        }

        if let Ok(dest) = std::env::var("ART_ISO_OUT") {
            fs::write(&dest, disc(true)).unwrap();
            println!("wrote synthetic Joliet ISO to {dest}");
        }
        if let Ok(dest) = std::env::var("ART_ISO_PLAIN_OUT") {
            fs::write(&dest, disc(false)).unwrap();
            println!("wrote synthetic ISO9660-only disc to {dest}");
        }
        // The raw layout, for `scripts/iso-oracle-check.py`: no host mounts a
        // 2352-byte track dump and 7-Zip will not read one either, so the
        // script strips it back to 2048-byte sectors itself — from the
        // layout's documented offsets, never from ART's code — and checks the
        // stripped image. Without this fixture the raw path has no
        // independent check at all, which is what ART-075 records.
        if let Ok(dest) = std::env::var("ART_ISO_RAW_OUT") {
            fs::write(&dest, sample_builder(SectorLayout::Raw2352, true).build()).unwrap();
            println!("wrote synthetic raw 2352-byte Joliet disc to {dest}");
        }
        // A Rock Ridge disc, for the same script. This one matters more than
        // it looks: ART's `NM` reader and ART's own fixture builder were
        // written from the same reading of SUSP, so they can agree with each
        // other and both be wrong — exactly the gap ART-032..035 fell
        // through. 7-Zip reads Rock Ridge with an implementation sharing no
        // code with either, so if it sees `MyGame.info` where ART sees it,
        // the System Use Areas ART writes here are really Rock Ridge and the
        // ones it reads on the owner's AmigaOS 3.9 CD are being read the same
        // way (ART-078).
        if let Ok(dest) = std::env::var("ART_ISO_ROCK_OUT") {
            // Without the `CE` continuation, and that exclusion was
            // *measured*, not assumed. Handed the disc with one, 7-Zip lists
            // `Games/Game` where ART lists `Games/Game.slave`: it reads the
            // inline `NM` fragment `Game.` and stops, dropping the trailing
            // dot as an empty extension. Its own source says why —
            // `CPP/7zip/Archive/Iso/IsoItem.h`, `FindSuspRecord` returns the
            // *first* matching entry in the inline area and `CE` is not a
            // signature it looks for at all, so it neither joins `NM`
            // fragments nor follows a continuation.
            //
            // So a `CE` fixture here would report a disagreement that is
            // 7-Zip's limitation and not ART's bug, and this script's own
            // rule is that only fixtures the oracle can actually judge are
            // handed to it. The `CE` case is checked instead by a *third*
            // implementation that does follow continuations:
            // `scripts/iso-susp-census.py` reads the same synthetic disc and
            // joins `Game.` + `slave` and the two comment halves the same
            // way ART does, and `a_name_and_comment_split_across_a_ce_
            // continuation_are_joined` pins it inside `cargo test`.
            let mut builder = amiga_rock_ridge_builder();
            for child in builder.children.iter_mut() {
                if let super::fixture::Node::Dir { children, .. } = child {
                    for grandchild in children.iter_mut() {
                        if let super::fixture::Node::File { rock: Some(r), .. } = grandchild {
                            r.continue_in_ce = false;
                        }
                    }
                }
            }
            fs::write(&dest, builder.build()).unwrap();
            println!("wrote synthetic Rock Ridge disc to {dest}");
        }
        // Mode 2/XA Form 1 — the layout ART-075 was about. Its data sits at
        // offset 24 rather than 16, so the script strips it with a different
        // constant and the same code path is checked against 7-Zip twice.
        if let Ok(dest) = std::env::var("ART_ISO_RAW_XA_OUT") {
            fs::write(&dest, sample_builder(SectorLayout::Raw2352Xa, true).build()).unwrap();
            println!("wrote synthetic raw Mode 2/XA Form 1 disc to {dest}");
        }
    }

    /// Read a disc and print what ART made of it, for
    /// `scripts/iso-oracle-check.py` to diff against 7-Zip's own listing.
    ///
    /// The same shape as `read_foreign_volume_for_oracle_when_asked` for a
    /// volume: one line per entry, so a mismatch names the entry rather than
    /// dumping two blobs. Sizes and a SHA-256 per file, because a listing that
    /// agrees on names and disagrees on bytes is the interesting failure —
    /// and the one a reader and its own fixtures can share.
    ///
    /// ```text
    /// ART_ISO_READ_IN=C:/temp/art.iso cargo test read_iso_for_oracle_when_asked -- --nocapture
    /// ```
    #[test]
    fn read_iso_for_oracle_when_asked() {
        let Ok(source) = std::env::var("ART_ISO_READ_IN") else {
            return;
        };
        let iso = IsoImage::open(std::path::Path::new(&source)).unwrap();
        println!("volume={}", iso.volume_name());
        println!("joliet={}", iso.is_joliet());
        println!("layout={:?}", iso.layout());

        let walk = iso.walk().unwrap();
        assert!(!walk.truncated, "the oracle fixtures are small");
        assert!(!walk.depth_limited, "the oracle fixtures are shallow");
        for item in &walk.entries {
            if item.entry.is_dir {
                println!("dir={}", item.path);
            } else {
                let data = iso.read_file(item.entry.extent, item.entry.bytes).unwrap();
                let digest = crate::core::hashing::sha256_bytes(&data);
                println!("file={}|{}|{digest}", item.path, item.entry.bytes);
            }
            // The Amiga `AS` metadata, on its own line so the oracle script
            // — which knows only `volume=`, `dir=` and `file=` — ignores it.
            // 7-Zip cannot judge these (it reads no `AS` entry), so they are
            // printed for a human to compare against
            // `scripts/iso-susp-census.py`'s decoding of the same disc, not
            // diffed automatically.
            if item.entry.protection.is_some() || item.entry.comment.is_some() {
                println!(
                    "meta={}|{}|{}",
                    item.path,
                    item.entry
                        .protection
                        .map(crate::core::volume::write::uaem::format_bits)
                        .unwrap_or_default(),
                    item.entry.comment.as_deref().unwrap_or_default()
                );
            }
        }
    }
}
