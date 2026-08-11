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

use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::security::safe_join;
use crate::core::volume::write::copy::{windows_safe_name, CopySource, ExtractReport};
use crate::core::volume::write::file::default_protection;
use crate::core::volume::write::layout::amiga_from_unix;
use crate::core::volume::write::plan::SourceEntry;
use crate::core::volume::write::uaem::Sidecar;

pub use descriptor::{SectorLayout, LOGICAL_SECTOR_SIZE};
pub use directory::IsoEntry;

use descriptor::{
    descriptor_kind, parse_volume_descriptor, DescriptorKind, VolumeInfo, FIRST_DESCRIPTOR_LBA,
    MAX_DESCRIPTORS,
};
use directory::parse_directory_extent;

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

        Ok(Self {
            path: path.to_path_buf(),
            file_len,
            layout,
            volume_name: volume.volume_name,
            root_extent: volume.root_extent,
            root_length: volume.root_length,
            joliet: volume.joliet,
        })
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
        parse_directory_extent(&buf, self.joliet)
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
    /// [`list`]: IsoImage::list
    pub fn extract_tree(
        &self,
        extent: u32,
        length: u32,
        dest: &Path,
        sink: &dyn ProgressSink,
    ) -> CoreResult<ExtractReport> {
        let mut report = ExtractReport::default();
        std::fs::create_dir_all(dest)?;
        self.extract_dir(extent, length, dest, 0, sink, &mut report)?;
        Ok(report)
    }

    fn extract_dir(
        &self,
        extent: u32,
        length: u32,
        dest: &Path,
        depth: usize,
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

            let safe = windows_safe_name(&entry.name);
            if safe != entry.name {
                report.renamed.push(format!("{} → {safe}", entry.name));
            }

            // Through `safe_join` even though the name has just been made
            // NTFS-safe: escaping is about what the host filesystem will
            // accept, containment is a separate question with a separate
            // answer (the same split `extract_from_volume` makes).
            let target = match safe_join(dest, &safe) {
                Ok(path) => path,
                Err(err) => {
                    report.skipped.push(format!("{} — {err}", entry.name));
                    continue;
                }
            };

            if entry.is_dir {
                std::fs::create_dir_all(&target)?;
                report.directories_created += 1;
                // A directory's length is a u32 on disc; one claiming more
                // than u32::MAX cannot exist, same clamp `walk_subtree` uses.
                let sub_length = entry.bytes.min(u32::MAX as u64) as u32;
                self.extract_dir(entry.extent, sub_length, &target, depth + 1, sink, report)?;
                continue;
            }

            // A disc never changes, so a name already at the destination
            // means a previous run put it there — SAFE_CREATE, not a silent
            // overwrite of something the user may not want replaced.
            if target.exists() {
                report.skipped.push(format!(
                    "{} — already in the destination folder",
                    entry.name
                ));
                continue;
            }

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
            // Raw sectors interleave 304 bytes of sync, header and ECC that
            // are not part of the file, so each has to be lifted separately.
            SectorLayout::Raw2352 => {
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

    fn metadata(&self, relative: &str) -> CoreResult<Option<Sidecar>> {
        // A disc carries a recording date and nothing else AmigaDOS would
        // call protection bits or a comment — every file gets the AmigaDOS
        // default bits, the same as a host file with no `.uaem` beside it.
        let found = self.entries.iter().find(|e| e.path == relative);
        Ok(found.and_then(|e| e.entry.date).map(|unix| Sidecar {
            protection: default_protection(),
            date: amiga_from_unix(unix),
            comment: String::new(),
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
    for layout in [SectorLayout::Cooked, SectorLayout::Raw2352] {
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

    /// A file or directory to put on the synthetic disc.
    #[derive(Debug, Clone)]
    pub enum Node {
        File {
            /// The 8.3 uppercase name in the Primary tree, without `;1`.
            iso: String,
            /// The name in the Joliet tree, if one is being built.
            joliet: String,
            data: Vec<u8>,
        },
        Dir {
            iso: String,
            joliet: String,
            children: Vec<Node>,
        },
    }

    pub fn file(iso: &str, joliet: &str, data: &[u8]) -> Node {
        Node::File {
            iso: iso.to_string(),
            joliet: joliet.to_string(),
            data: data.to_vec(),
        }
    }

    pub fn dir(iso: &str, joliet: &str, children: Vec<Node>) -> Node {
        Node::Dir {
            iso: iso.to_string(),
            joliet: joliet.to_string(),
            children,
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
            }
        }
    }

    struct FlatDir {
        iso: String,
        joliet: String,
        parent: usize,
        subdirs: Vec<usize>,
        files: Vec<usize>,
    }

    struct FlatFile {
        iso: String,
        joliet: String,
        data: Vec<u8>,
        extent: u32,
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

            for (i, d) in dirs.iter().enumerate() {
                let sectors = if i == 0 { root_sectors } else { 1 };
                let extent = directory_extent(
                    d,
                    &dirs,
                    &files,
                    &iso_dir_lba,
                    i,
                    false,
                    sectors,
                    self.split_root && i == 0,
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
                    );
                    put(&mut image, joliet_dir_lba[i], &extent);
                }
            }

            for f in &files {
                put(&mut image, f.extent, &f.data);
            }

            match self.layout {
                SectorLayout::Cooked => image,
                SectorLayout::Raw2352 => to_raw(&image),
            }
        }

        fn flatten(&self) -> (Vec<FlatDir>, Vec<FlatFile>) {
            let mut dirs = vec![FlatDir {
                iso: String::new(),
                joliet: String::new(),
                parent: 0,
                subdirs: Vec::new(),
                files: Vec::new(),
            }];
            let mut files: Vec<FlatFile> = Vec::new();

            // Breadth first, so a directory's index is always greater than
            // its parent's — which is what a path table requires.
            let mut queue: VecDeque<(usize, &[Node])> = VecDeque::new();
            queue.push_back((0, &self.children));
            while let Some((parent, children)) = queue.pop_front() {
                for child in children {
                    match child {
                        Node::File { iso, joliet, data } => {
                            let index = files.len();
                            files.push(FlatFile {
                                iso: iso.clone(),
                                joliet: joliet.clone(),
                                data: data.clone(),
                                extent: 0,
                            });
                            dirs[parent].files.push(index);
                        }
                        Node::Dir {
                            iso,
                            joliet,
                            children,
                        } => {
                            let index = dirs.len();
                            dirs.push(FlatDir {
                                iso: iso.clone(),
                                joliet: joliet.clone(),
                                parent,
                                subdirs: Vec::new(),
                                files: Vec::new(),
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
    fn dir_record(id: &[u8], extent: u32, length: u32, is_dir: bool) -> Vec<u8> {
        let mut len = 33 + id.len();
        if len % 2 == 1 {
            len += 1;
        }
        let mut r = vec![0u8; len];
        r[0] = len as u8;
        r[2..10].copy_from_slice(&both32(extent));
        r[10..18].copy_from_slice(&both32(length));
        r[18..25].copy_from_slice(&FIXTURE_DATE);
        r[25] = if is_dir { 0x02 } else { 0x00 };
        r[28..32].copy_from_slice(&both16(1));
        r[32] = id.len() as u8;
        r[33..33 + id.len()].copy_from_slice(id);
        r
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
    ) -> Vec<u8> {
        let self_lba = dir_lba[index];
        let parent_lba = dir_lba[d.parent];
        let self_len = sectors * LOGICAL_SECTOR_SIZE as u32;
        let parent_len = if d.parent == 0 {
            self_len.max(LOGICAL_SECTOR_SIZE as u32)
        } else {
            LOGICAL_SECTOR_SIZE as u32
        };

        let mut records: Vec<(Vec<u8>, Vec<u8>)> = Vec::new(); // (sort key, bytes)
        for &sub in &d.subdirs {
            let name = if joliet {
                &dirs[sub].joliet
            } else {
                &dirs[sub].iso
            };
            let id = encode_id(name, joliet);
            records.push((
                id.clone(),
                dir_record(&id, dir_lba[sub], LOGICAL_SECTOR_SIZE as u32, true),
            ));
        }
        for &fi in &d.files {
            let f = &files[fi];
            let name = if joliet { &f.joliet } else { &f.iso };
            let id = encode_id(&format!("{name};1"), joliet);
            records.push((
                id.clone(),
                dir_record(&id, f.extent, f.data.len() as u32, false),
            ));
        }
        records.sort_by(|a, b| a.0.cmp(&b.0));

        let mut buf = vec![0u8; sectors as usize * LOGICAL_SECTOR_SIZE];
        let mut pos = 0usize;
        for r in [
            dir_record(&[0x00], self_lba, self_len, true),
            dir_record(&[0x01], parent_lba, parent_len, true),
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
        let root = dir_record(&[0x00], root_lba, root_len, true);
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

    /// Wrap 2048-byte sectors in Mode-1 raw sectors: 12 bytes of sync, a
    /// 4-byte header, the data, then 288 bytes where EDC/ECC would be.
    ///
    /// The parity bytes are left zero. Nothing ART reads looks at them, and a
    /// raw image is a track dump rather than something a host mounts.
    fn to_raw(cooked: &[u8]) -> Vec<u8> {
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
            out[base + 15] = 0x01; // Mode 1
            out[base + 16..base + 16 + LOGICAL_SECTOR_SIZE]
                .copy_from_slice(&cooked[i * LOGICAL_SECTOR_SIZE..(i + 1) * LOGICAL_SECTOR_SIZE]);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::{dir, file, IsoBuilder};
    use super::*;
    use std::fs;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "art-iso-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
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
            .extract_tree(extent, length, &dest, &NoProgress)
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
            .extract_tree(900_000, LOGICAL_SECTOR_SIZE as u32, &dest, &NoProgress)
            .unwrap_err();
        assert_eq!(err.code(), "ART-FORMAT-MALFORMED");
        fs::remove_dir_all(&d).ok();
        fs::remove_dir_all(&dest).ok();
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

    #[test]
    fn opening_with_a_layout_from_detection_matches_probing_for_one() {
        for layout in [SectorLayout::Cooked, SectorLayout::Raw2352] {
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
    }
}
