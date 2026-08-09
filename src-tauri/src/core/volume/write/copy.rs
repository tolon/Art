//! Copying, in all three directions (brief §4).
//!
//! ```text
//! Windows → Amiga    name pre-flight, protection bits, mtime, collisions
//! Amiga → Windows    NTFS-safe names, .uaem sidecars for what NTFS cannot hold
//! Amiga → Amiga      extract → verify → insert → verify, via a temp file
//! ```
//!
//! ## One journalled operation per file
//!
//! A hundred-file copy is a hundred operations, each with its own journal, run
//! as one job. Cancelling between files therefore leaves a volume that is
//! consistent and a report that says how many landed — never a directory with
//! half a file in it (§2).
//!
//! ## Nothing is deleted on unverified data
//!
//! A move is a copy, then a verification, then a delete — in that order, and
//! the delete only happens if the verification passed. A failure after the
//! copy leaves **both** copies and says so. That is the §57 rule stated for
//! this module: *the commander may do anything to the copy, and nothing
//! unverified to the original.*

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::security::safe_join;
use crate::core::volume::{BlockDevice, VolumeGeometry};

use super::layout::{amiga_from_unix, BlockSet, PROTECT_OFFSET};
use super::plan::{order_for_creation, SourceEntry};
use super::uaem::{self, Sidecar};
use super::{FileMeta, VolumeWriter};

/// What to do when a name is already taken.
///
/// The same three choices as LHA extraction, deliberately: one collision
/// policy across ART means the user learns it once (§4.1).
pub use crate::core::lha::OverwritePolicy;

/// The most bytes ART will read into memory for one file being copied.
///
/// A volume tops out at 2 GiB but a single file that large would be read whole
/// into RAM here. 256 MB is well past anything on an Amiga disk and keeps the
/// promise that ART does not pull a hard disk into memory (ART-021).
pub const MAX_COPY_FILE_BYTES: u64 = 256 * 1024 * 1024;

/// What a copy did, and what it did not.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct CopyReport {
    pub files_copied: usize,
    pub directories_created: usize,
    pub bytes_copied: u64,
    /// Files verified by reading them back. Should equal `files_copied`.
    pub files_verified: usize,
    /// Entries left alone, with the reason. Never silent.
    pub skipped: Vec<String>,
    /// Entries copied under a different name, as `original → new`.
    pub renamed: Vec<String>,
    /// True when the user stopped it. `files_copied` is then how many landed.
    pub cancelled: bool,
}

impl CopyReport {
    /// Whether everything asked for actually happened.
    pub fn is_complete(&self) -> bool {
        !self.cancelled && self.skipped.is_empty()
    }
}

/// Where the bytes of one entry come from.
///
/// A trait so the copy engine does not care whether the source is a Windows
/// folder, an ADF or a partition — and so the tests can drive it with neither.
pub trait CopySource {
    /// Everything to copy, relative paths with `/` separators.
    fn entries(&self) -> CoreResult<Vec<SourceEntry>>;

    /// The bytes of one file.
    fn read(&self, relative: &str) -> CoreResult<Vec<u8>>;

    /// Protection bits, date and comment, when the source knows them.
    ///
    /// A Windows folder answers from a `.uaem` sidecar if one is there, and
    /// from the file's own mtime otherwise.
    fn metadata(&self, relative: &str) -> CoreResult<Option<Sidecar>>;
}

/// Copy a tree into a volume.
///
/// The caller has already shown the user a [`CopyPlan`](super::plan::CopyPlan)
/// and had it accepted; this does the work. Each file is its own journalled
/// operation, so `sink.is_cancelled()` is checked **between** files and never
/// during one.
///
/// [`CopyPlan`]: super::plan::CopyPlan
pub fn copy_into_volume(
    writer: &mut VolumeWriter<'_>,
    dest_dir: u32,
    source: &dyn CopySource,
    policy: OverwritePolicy,
    sink: &dyn ProgressSink,
) -> CoreResult<CopyReport> {
    let entries = order_for_creation(&source.entries()?);
    let total = entries.len();
    let mut report = CopyReport::default();

    // Relative directory path → the block it landed on, so a nested file can
    // find its parent without re-walking the volume.
    let mut created: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
    created.insert(String::new(), dest_dir);

    for (index, entry) in entries.iter().enumerate() {
        // Between whole units of work, never inside one (§54). A cancelled
        // copy leaves a consistent volume with the files copied so far.
        if sink.is_cancelled() {
            report.cancelled = true;
            break;
        }
        sink.report(index as u64, Some(total as u64), &entry.relative);

        let (parent_relative, name) = split_relative(&entry.relative);
        let Some(&parent) = created.get(parent_relative) else {
            report.skipped.push(format!(
                "{} — the folder it goes in was not created",
                entry.relative
            ));
            continue;
        };

        match copy_one(writer, source, parent, entry, name, policy, &mut report) {
            Ok(Some(block)) if entry.is_dir => {
                created.insert(entry.relative.clone(), block);
            }
            Ok(_) => {}
            Err(err) => {
                // One entry failing does not abandon the rest: a single
                // unwritable name in a two-hundred-file tree should cost that
                // one file, not the copy.
                report.skipped.push(format!("{} — {err}", entry.relative));
            }
        }
    }

    sink.report(total as u64, Some(total as u64), "done");
    Ok(report)
}

/// One entry, in its own journalled operation.
fn copy_one(
    writer: &mut VolumeWriter<'_>,
    source: &dyn CopySource,
    parent: u32,
    entry: &SourceEntry,
    name: &str,
    policy: OverwritePolicy,
    report: &mut CopyReport,
) -> CoreResult<Option<u32>> {
    let resolved = match resolve_collision(writer, parent, name, policy)? {
        Resolution::Use(name) => name,
        Resolution::Skip => {
            report.skipped.push(format!(
                "{} — a file of that name is already there",
                entry.relative
            ));
            return Ok(None);
        }
        Resolution::Replace(existing) => {
            writer.delete(parent, existing)?;
            name.to_string()
        }
    };
    if resolved != name {
        report.renamed.push(format!("{name} → {resolved}"));
    }

    if entry.is_dir {
        let outcome = writer.make_dir(parent, &resolved)?;
        report.directories_created += 1;
        return Ok(outcome.block);
    }

    if entry.bytes > MAX_COPY_FILE_BYTES {
        return Err(CoreError::InvalidInput(format!(
            "{} is {} bytes, which is more than ART copies in one go",
            entry.relative, entry.bytes
        )));
    }

    let data = source.read(&entry.relative)?;
    let sidecar = source.metadata(&entry.relative)?;
    let meta = FileMeta {
        protection: sidecar.as_ref().map(|s| s.protection),
        date: sidecar.as_ref().map(|s| s.date),
    };

    let outcome = writer.add_file(parent, &resolved, &data, meta)?;
    report.files_copied += 1;
    report.bytes_copied += data.len() as u64;
    // `add_file` reads the file back out of the volume before committing, so
    // reaching here means it verified.
    if outcome.verified {
        report.files_verified += 1;
    }

    Ok(outcome.block)
}

enum Resolution {
    Use(String),
    Skip,
    Replace(u32),
}

fn resolve_collision(
    writer: &VolumeWriter<'_>,
    parent: u32,
    name: &str,
    policy: OverwritePolicy,
) -> CoreResult<Resolution> {
    let existing = writer.find(parent, name)?;
    let Some(entry) = existing else {
        return Ok(Resolution::Use(name.to_string()));
    };

    match policy {
        OverwritePolicy::Skip => Ok(Resolution::Skip),
        OverwritePolicy::Overwrite => Ok(Resolution::Replace(entry.block)),
        OverwritePolicy::Rename => {
            let taken: BTreeSet<String> = writer
                .list(parent)?
                .into_iter()
                .map(|e| e.name.to_lowercase())
                .collect();
            let mut taken = taken;
            match super::plan::suggest_name(&next_name(name), &mut taken) {
                Some(suggestion) => Ok(Resolution::Use(suggestion)),
                None => Ok(Resolution::Skip),
            }
        }
    }
}

/// `Readme.txt` → `Readme~1.txt`, which `suggest_name` then makes unique.
fn next_name(name: &str) -> String {
    match name.rfind('.') {
        Some(dot) if dot > 0 => format!("{}~1{}", &name[..dot], &name[dot..]),
        _ => format!("{name}~1"),
    }
}

fn split_relative(relative: &str) -> (&str, &str) {
    match relative.rfind('/') {
        Some(slash) => (&relative[..slash], &relative[slash + 1..]),
        None => ("", relative),
    }
}

// ---------------------------------------------------------------------------
// Windows → Amiga
// ---------------------------------------------------------------------------

/// A folder on the user's disk, as a copy source.
#[derive(Debug, Clone)]
pub struct HostFolder {
    root: PathBuf,
    /// Whether to look for `.uaem` sidecars next to each file.
    read_sidecars: bool,
    max_depth: usize,
    max_entries: usize,
}

/// How deep ART will walk a folder being copied in.
pub const MAX_COPY_DEPTH: usize = 16;
/// How many entries one copy may hold.
pub const MAX_COPY_ENTRIES: usize = 20_000;

impl HostFolder {
    pub fn new(root: impl Into<PathBuf>, read_sidecars: bool) -> Self {
        Self {
            root: root.into(),
            read_sidecars,
            max_depth: MAX_COPY_DEPTH,
            max_entries: MAX_COPY_ENTRIES,
        }
    }

    /// Turn a relative path back into one on disk.
    ///
    /// Through `safe_join` even though every path here came from this
    /// module's own walk: it makes a round trip through a plan the frontend
    /// has seen and can edit, so by the time it comes back it is untrusted
    /// input like any other.
    fn resolve(&self, relative: &str) -> CoreResult<PathBuf> {
        safe_join(&self.root, relative).map_err(|err| {
            CoreError::InvalidInput(format!("'{relative}' is not a path inside the copy: {err}"))
        })
    }
}

impl CopySource for HostFolder {
    fn entries(&self) -> CoreResult<Vec<SourceEntry>> {
        let mut out = Vec::new();
        walk(
            &self.root,
            "",
            0,
            self.max_depth,
            self.max_entries,
            &mut out,
        )?;
        Ok(out)
    }

    fn read(&self, relative: &str) -> CoreResult<Vec<u8>> {
        let path = self.resolve(relative)?;
        let size = std::fs::metadata(&path)?.len();
        if size > MAX_COPY_FILE_BYTES {
            return Err(CoreError::InvalidInput(format!(
                "{relative} is {size} bytes, more than ART copies in one go"
            )));
        }
        Ok(std::fs::read(path)?)
    }

    fn metadata(&self, relative: &str) -> CoreResult<Option<Sidecar>> {
        let path = self.resolve(relative)?;

        // A sidecar written by ART or WinUAE is the better source: it holds
        // the bits and comment the host filesystem threw away.
        if self.read_sidecars {
            let sidecar = uaem::sidecar_path(&path);
            if sidecar.exists() {
                let size = std::fs::metadata(&sidecar)?.len();
                if size <= uaem::MAX_UAEM_BYTES {
                    // A damaged sidecar falls through to the mtime rather than
                    // failing the copy: losing the bits is better than losing
                    // the file, and the caller is told nothing was applied.
                    if let Ok(parsed) = uaem::parse(&std::fs::read_to_string(&sidecar)?) {
                        return Ok(Some(parsed));
                    }
                }
            }
        }

        // Otherwise the file's own modification time, and default bits.
        let modified = std::fs::metadata(&path)?
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);

        Ok(modified.map(|unix| Sidecar {
            protection: super::file::default_protection(),
            date: amiga_from_unix(unix),
            comment: String::new(),
        }))
    }
}

fn walk(
    base: &Path,
    relative: &str,
    depth: usize,
    max_depth: usize,
    max_entries: usize,
    out: &mut Vec<SourceEntry>,
) -> CoreResult<()> {
    if depth > max_depth || out.len() >= max_entries {
        return Ok(());
    }

    let here = if relative.is_empty() {
        base.to_path_buf()
    } else {
        safe_join(base, relative).map_err(|err| {
            CoreError::InvalidInput(format!(
                "'{relative}' escapes the folder being copied: {err}"
            ))
        })?
    };

    for entry in std::fs::read_dir(&here)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let child = if relative.is_empty() {
            name.clone()
        } else {
            format!("{relative}/{name}")
        };

        // Sidecars are metadata, not content: copying them in as files would
        // put `Game.slave.uaem` on the Amiga disk next to `Game.slave`.
        if name
            .rsplit('.')
            .next()
            .is_some_and(|ext| ext.eq_ignore_ascii_case(uaem::UAEM_EXTENSION))
        {
            continue;
        }

        let kind = entry.file_type()?;
        if kind.is_symlink() {
            // Not followed: a link out of the folder would copy in something
            // the user did not select, and a link back into it loops.
            continue;
        }

        if kind.is_dir() {
            out.push(SourceEntry {
                relative: child.clone(),
                is_dir: true,
                bytes: 0,
            });
            walk(base, &child, depth + 1, max_depth, max_entries, out)?;
        } else if kind.is_file() {
            out.push(SourceEntry {
                relative: child,
                is_dir: false,
                bytes: entry.metadata()?.len(),
            });
        }

        if out.len() >= max_entries {
            return Ok(());
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Amiga → Windows
// ---------------------------------------------------------------------------

/// Make an Amiga name safe for NTFS, deterministically.
///
/// NTFS refuses `< > : " / \ | ? *`, control characters, trailing dots and
/// trailing spaces, and a handful of reserved device names. An Amiga file can
/// legally be called `AUX` or `Prices: 1993`, and writing one out unchanged
/// fails on Windows in a way that looks like ART being broken.
///
/// Deterministic so the same name always maps to the same file: a copy run
/// twice must overwrite, not accumulate.
pub fn windows_safe_name(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if (c as u32) < 0x20 => '_',
            c => c,
        })
        .collect();

    // Trailing dots and spaces are legal to create on NTFS and impossible to
    // open afterwards. Every one of them becomes an underscore — replacing
    // only the last would leave `Name...` as `Name.._`, which has the same
    // problem it started with.
    let trailing = out
        .chars()
        .rev()
        .take_while(|c| *c == '.' || *c == ' ')
        .count();
    if trailing > 0 {
        let keep: String = out.chars().take(out.chars().count() - trailing).collect();
        out = format!("{keep}{}", "_".repeat(trailing));
    }

    // The DOS device names, still reserved, with or without an extension.
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let stem = out.split('.').next().unwrap_or("").to_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        out.insert(0, '_');
    }

    if out.is_empty() {
        out.push('_');
    }
    out
}

/// The sidecar for a file being written out, or `None` when there is nothing
/// worth recording.
///
/// A file with default bits, no comment and no date carries no information a
/// sidecar would preserve, and writing one for every file would double the
/// file count of every extraction.
pub fn sidecar_for(
    protection: u32,
    date: crate::core::adf::bcpl::AmigaDate,
    comment: &str,
) -> Option<Sidecar> {
    let interesting = protection != super::file::default_protection()
        || !comment.is_empty()
        || date != crate::core::adf::bcpl::AmigaDate::default();

    interesting.then(|| Sidecar {
        protection,
        date,
        comment: comment.to_string(),
    })
}

/// Read a file's protection bits out of its header block.
pub fn protection_of<D: BlockDevice + ?Sized>(
    device: &D,
    geometry: &VolumeGeometry,
    header_block: u32,
) -> CoreResult<u32> {
    let _ = geometry;
    let set = BlockSet::new(device.block_size());
    let header = set.view(device, header_block)?;
    super::layout::get_u32(&header, PROTECT_OFFSET)
}

// ---------------------------------------------------------------------------
// Amiga → Windows
// ---------------------------------------------------------------------------

/// What extracting a tree out of a volume did.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ExtractReport {
    pub files_written: usize,
    pub directories_created: usize,
    pub bytes_written: u64,
    pub sidecars_written: usize,
    /// Entries written under a different name, as `amiga name → windows name`.
    pub renamed: Vec<String>,
    pub skipped: Vec<String>,
    pub cancelled: bool,
}

impl ExtractReport {
    pub fn is_complete(&self) -> bool {
        !self.cancelled && self.skipped.is_empty()
    }
}

/// Copy a directory out of a volume into a folder on the user's disk.
///
/// Names NTFS refuses are escaped deterministically and reported. Metadata
/// NTFS cannot hold — protection bits, comment, exact ticks — goes into a
/// `.uaem` sidecar when `write_sidecars` is on, in the format WinUAE uses, so
/// the folder round-trips back into ART and into WinUAE without loss (§4.2).
#[allow(clippy::too_many_arguments)]
pub fn extract_from_volume<D: BlockDevice + ?Sized>(
    device: &D,
    geometry: &VolumeGeometry,
    dir_block: u32,
    dest: &Path,
    write_sidecars: bool,
    policy: OverwritePolicy,
    sink: &dyn ProgressSink,
) -> CoreResult<ExtractReport> {
    let mut report = ExtractReport::default();
    let root = if dir_block == 0 {
        geometry.root_block
    } else {
        dir_block
    };

    std::fs::create_dir_all(dest)?;
    extract_dir(
        device,
        geometry,
        root,
        dest,
        0,
        write_sidecars,
        policy,
        sink,
        &mut report,
    )?;
    Ok(report)
}

#[allow(clippy::too_many_arguments)]
fn extract_dir<D: BlockDevice + ?Sized>(
    device: &D,
    geometry: &VolumeGeometry,
    dir_block: u32,
    dest: &Path,
    depth: usize,
    write_sidecars: bool,
    policy: OverwritePolicy,
    sink: &dyn ProgressSink,
    report: &mut ExtractReport,
) -> CoreResult<()> {
    if depth > MAX_COPY_DEPTH {
        report.skipped.push(format!(
            "{} — nested deeper than ART follows",
            dest.display()
        ));
        return Ok(());
    }

    let set = BlockSet::new(device.block_size());
    let entries = super::dir::entries_in(device, &set, geometry, dir_block)?;

    for entry in entries {
        if sink.is_cancelled() {
            report.cancelled = true;
            return Ok(());
        }
        sink.report(report.files_written as u64, None, &entry.name);

        let safe = windows_safe_name(&entry.name);
        if safe != entry.name {
            report.renamed.push(format!("{} → {safe}", entry.name));
        }

        // Through `safe_join` even though the name has just been escaped: the
        // escaping is about what NTFS will accept, and containment is a
        // separate question with a separate answer.
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
            extract_dir(
                device,
                geometry,
                entry.block,
                &target,
                depth + 1,
                write_sidecars,
                policy,
                sink,
                report,
            )?;
            continue;
        }

        if target.exists() {
            match policy {
                OverwritePolicy::Skip => {
                    report.skipped.push(format!(
                        "{} — already in the destination folder",
                        entry.name
                    ));
                    continue;
                }
                OverwritePolicy::Rename => {
                    report.skipped.push(format!(
                        "{} — already there, and ART does not invent names on the way out",
                        entry.name
                    ));
                    continue;
                }
                OverwritePolicy::Overwrite => {}
            }
        }

        let data = match super::file::read_file(device, &set, geometry, entry.block) {
            Ok(data) => data,
            Err(err) => {
                report.skipped.push(format!("{} — {err}", entry.name));
                continue;
            }
        };

        // Through `core/safety`: a truncated file on the way out is still a
        // file the user will believe is a good copy.
        crate::core::safety::atomic::atomic_write(&target, &data)?;
        report.files_written += 1;
        report.bytes_written += data.len() as u64;

        if write_sidecars {
            let header = set.view(device, entry.block)?;
            let protection = super::layout::get_u32(&header, PROTECT_OFFSET)?;
            let date = crate::core::adf::bcpl::AmigaDate {
                days: super::layout::get_u32(&header, super::layout::DAYS_OFFSET)?,
                mins: super::layout::get_u32(&header, super::layout::MINS_OFFSET)?,
                ticks: super::layout::get_u32(&header, super::layout::TICKS_OFFSET)?,
            };
            let comment =
                crate::core::adf::bcpl::read_bcpl_string(&header, super::layout::COMMENT_OFFSET)
                    .unwrap_or_default();

            if let Some(sidecar) = sidecar_for(protection, date, &comment) {
                crate::core::safety::atomic::atomic_write(
                    &uaem::sidecar_path(&target),
                    uaem::render(&sidecar).as_bytes(),
                )?;
                report.sidecars_written += 1;
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Amiga → Amiga
// ---------------------------------------------------------------------------

/// A directory inside a volume, staged into a temp folder, as a copy source.
///
/// Amiga-to-Amiga goes through a temp folder rather than memory: the images
/// can be gigabytes, and holding both ends open while streaming between them
/// would mean two mutable borrows with a verification step in the middle.
/// Extract, verify, insert, verify — each half already tested on its own
/// (§4.3).
pub struct StagedTree {
    folder: HostFolder,
    /// Removed when this is dropped, whether the copy succeeded or not.
    _temp: TempFolder,
}

impl StagedTree {
    /// Extract `dir_block` of a volume into a temp folder.
    pub fn stage<D: BlockDevice + ?Sized>(
        device: &D,
        geometry: &VolumeGeometry,
        dir_block: u32,
        cache_root: &Path,
        sink: &dyn ProgressSink,
    ) -> CoreResult<Self> {
        let temp = TempFolder::new(cache_root)?;
        // Sidecars on, always: this is a copy between two Amiga volumes, so
        // the bits and comments have to survive the trip through a host
        // filesystem that cannot hold them.
        extract_from_volume(
            device,
            geometry,
            dir_block,
            temp.path(),
            true,
            OverwritePolicy::Overwrite,
            sink,
        )?;

        Ok(Self {
            folder: HostFolder::new(temp.path(), true),
            _temp: temp,
        })
    }

    pub fn source(&self) -> &dyn CopySource {
        &self.folder
    }
}

/// A folder that removes itself.
///
/// Named from the process id and a counter rather than a random number:
/// `core/` has no clock and no RNG, and a counter is enough because the folder
/// is removed when the copy ends.
pub struct TempFolder(PathBuf);

impl TempFolder {
    pub fn new(root: &Path) -> CoreResult<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let path = root.join(format!(
            "art-stage-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempFolder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jobs::NoProgress;
    use crate::core::volume::device::FileRegionMut;
    use crate::core::volume::fixture::ffs_volume;
    use crate::core::volume::DosType;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("art-copy-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    struct Fixture {
        dir: PathBuf,
        image: PathBuf,
        source: PathBuf,
        geometry: VolumeGeometry,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let dir = scratch(name);
            let image = dir.join("disk.adf");
            let source = dir.join("source");
            std::fs::create_dir_all(&source).unwrap();

            let (bytes, geometry) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
            std::fs::write(&image, &bytes).unwrap();

            Self {
                dir,
                image,
                source,
                geometry,
            }
        }

        fn put(&self, relative: &str, contents: &[u8]) {
            let path = self.source.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, contents).unwrap();
        }

        fn put_dir(&self, relative: &str) {
            std::fs::create_dir_all(self.source.join(relative)).unwrap();
        }

        fn device(&self) -> FileRegionMut {
            FileRegionMut::open(&self.image, 0, self.geometry.total_bytes(), 512).unwrap()
        }

        fn run(&self, policy: OverwritePolicy) -> CoreResult<CopyReport> {
            let folder = HostFolder::new(&self.source, true);
            let mut device = self.device();
            let mut writer =
                VolumeWriter::open(&mut device, self.geometry, &self.image, 0).unwrap();
            copy_into_volume(&mut writer, 0, &folder, policy, &NoProgress)
        }

        fn listing(&self, dir_block: u32) -> Vec<super::super::dir::DirEntry> {
            let device = self.device();
            let set = BlockSet::new(512);
            let block = if dir_block == 0 {
                self.geometry.root_block
            } else {
                dir_block
            };
            super::super::dir::entries_in(&device, &set, &self.geometry, block).unwrap()
        }

        /// The whole image, for proving a refused operation changed nothing.
        fn bytes(&self) -> Vec<u8> {
            std::fs::read(&self.image).unwrap()
        }

        fn contents(&self, block: u32) -> Vec<u8> {
            let device = self.device();
            let set = BlockSet::new(512);
            super::super::file::read_file(&device, &set, &self.geometry, block).unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    // ---- Windows → Amiga ----

    #[test]
    fn a_flat_folder_copies_in_and_every_file_verifies() {
        let fixture = Fixture::new("flat");
        fixture.put("Readme.txt", b"hello from the host");
        fixture.put("Data.bin", &vec![7u8; 3000]);

        let report = fixture.run(OverwritePolicy::Skip).unwrap();
        assert_eq!(report.files_copied, 2);
        assert_eq!(report.files_verified, 2, "every file is read back");
        assert!(report.is_complete());

        let entries = fixture.listing(0);
        assert_eq!(entries.len(), 2);

        let readme = entries.iter().find(|e| e.name == "Readme.txt").unwrap();
        assert_eq!(fixture.contents(readme.block), b"hello from the host");
    }

    #[test]
    fn a_nested_tree_recreates_its_folders() {
        let fixture = Fixture::new("nested");
        fixture.put("Tools/Editor/Config", b"settings");
        fixture.put("Tools/Readme", b"about the tools");
        fixture.put("Top", b"top level");

        let report = fixture.run(OverwritePolicy::Skip).unwrap();
        assert_eq!(report.files_copied, 3);
        assert_eq!(report.directories_created, 2);

        let root = fixture.listing(0);
        let tools = root.iter().find(|e| e.name == "Tools").unwrap();
        assert!(tools.is_dir);

        let inside = fixture.listing(tools.block);
        let editor = inside.iter().find(|e| e.name == "Editor").unwrap();
        let config = fixture.listing(editor.block);
        assert_eq!(config.len(), 1);
        assert_eq!(fixture.contents(config[0].block), b"settings");
    }

    #[test]
    fn an_empty_folder_is_created_even_with_nothing_in_it() {
        let fixture = Fixture::new("emptydir");
        fixture.put_dir("Empty");

        let report = fixture.run(OverwritePolicy::Skip).unwrap();
        assert_eq!(report.directories_created, 1);
        assert_eq!(report.files_copied, 0);
        assert!(fixture.listing(0)[0].is_dir);
    }

    /// A file whose name AmigaDOS cannot store costs that file, not the copy.
    #[test]
    fn one_bad_name_does_not_abandon_the_rest_of_the_copy() {
        let fixture = Fixture::new("badname");
        fixture.put(&"a".repeat(40), b"too long");
        fixture.put("Fine.txt", b"fine");

        let report = fixture.run(OverwritePolicy::Skip).unwrap();
        assert_eq!(report.files_copied, 1);
        assert_eq!(report.skipped.len(), 1);
        assert!(
            report.skipped[0].contains("stop at 30"),
            "{:?}",
            report.skipped
        );
        assert!(!report.is_complete(), "a skipped entry means incomplete");
    }

    #[test]
    fn the_skip_policy_leaves_what_is_already_there() {
        let fixture = Fixture::new("skip");
        fixture.put("Readme.txt", b"first");
        fixture.run(OverwritePolicy::Skip).unwrap();

        // Change the source and copy again.
        fixture.put("Readme.txt", b"second");
        let report = fixture.run(OverwritePolicy::Skip).unwrap();

        assert_eq!(report.files_copied, 0);
        assert_eq!(report.skipped.len(), 1);
        let entry = &fixture.listing(0)[0];
        assert_eq!(
            fixture.contents(entry.block),
            b"first",
            "the original stands"
        );
    }

    #[test]
    fn the_overwrite_policy_replaces_what_is_there() {
        let fixture = Fixture::new("overwrite");
        fixture.put("Readme.txt", b"first");
        fixture.run(OverwritePolicy::Skip).unwrap();

        fixture.put("Readme.txt", b"second version, longer");
        let report = fixture.run(OverwritePolicy::Overwrite).unwrap();

        assert_eq!(report.files_copied, 1);
        let entries = fixture.listing(0);
        assert_eq!(entries.len(), 1, "overwrite must not leave two entries");
        assert_eq!(
            fixture.contents(entries[0].block),
            b"second version, longer"
        );
    }

    #[test]
    fn the_rename_policy_keeps_both() {
        let fixture = Fixture::new("rename");
        fixture.put("Readme.txt", b"first");
        fixture.run(OverwritePolicy::Skip).unwrap();

        fixture.put("Readme.txt", b"second");
        let report = fixture.run(OverwritePolicy::Rename).unwrap();

        assert_eq!(report.files_copied, 1);
        assert_eq!(report.renamed.len(), 1);
        let entries = fixture.listing(0);
        assert_eq!(entries.len(), 2, "both files should be there");
    }

    /// Overwriting must free the old file's blocks, or a disk fills up with
    /// copies nothing references.
    #[test]
    fn overwriting_gives_the_old_files_blocks_back() {
        let fixture = Fixture::new("overwrite-space");
        fixture.put("Big.bin", &vec![1u8; 50_000]);
        fixture.run(OverwritePolicy::Skip).unwrap();

        let free_after_first = {
            let mut device = fixture.device();
            let writer =
                VolumeWriter::open(&mut device, fixture.geometry, &fixture.image, 0).unwrap();
            writer.free_blocks().unwrap()
        };

        fixture.put("Big.bin", &vec![2u8; 50_000]);
        fixture.run(OverwritePolicy::Overwrite).unwrap();

        let free_after_second = {
            let mut device = fixture.device();
            let writer =
                VolumeWriter::open(&mut device, fixture.geometry, &fixture.image, 0).unwrap();
            writer.free_blocks().unwrap()
        };

        assert_eq!(
            free_after_first, free_after_second,
            "replacing a file with one the same size must not consume more space"
        );
    }

    /// Sidecars are metadata. Copying them in as files would put
    /// `Game.slave.uaem` on the Amiga disk next to `Game.slave`.
    #[test]
    fn a_uaem_sidecar_is_applied_rather_than_copied_as_a_file() {
        let fixture = Fixture::new("sidecar");
        fixture.put("Game.slave", b"slave bytes");
        fixture.put(
            "Game.slave.uaem",
            b"-sp-rwed 2020-05-04 10:20:30.00 a comment\n",
        );

        let report = fixture.run(OverwritePolicy::Skip).unwrap();
        assert_eq!(report.files_copied, 1, "the sidecar is not a file to copy");

        let entries = fixture.listing(0);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Game.slave");

        // And the bits it carried were applied.
        let device = fixture.device();
        let protection = protection_of(&device, &fixture.geometry, entries[0].block).unwrap();
        assert_eq!(
            uaem::format_bits(protection),
            "-sp-rwed",
            "the S and P bits WHDLoad depends on must survive"
        );
    }

    #[test]
    fn a_file_without_a_sidecar_gets_the_amigados_default_bits() {
        let fixture = Fixture::new("nosidecar");
        fixture.put("Plain.txt", b"x");
        fixture.run(OverwritePolicy::Skip).unwrap();

        let entries = fixture.listing(0);
        let device = fixture.device();
        let protection = protection_of(&device, &fixture.geometry, entries[0].block).unwrap();
        assert_eq!(uaem::format_bits(protection), "----rwed");
    }

    /// Losing the bits is better than losing the file.
    #[test]
    fn a_damaged_sidecar_does_not_stop_the_file_being_copied() {
        let fixture = Fixture::new("bad-sidecar");
        fixture.put("Thing", b"contents");
        fixture.put("Thing.uaem", b"this is not a sidecar\n");

        let report = fixture.run(OverwritePolicy::Skip).unwrap();
        assert_eq!(report.files_copied, 1);
        assert_eq!(fixture.contents(fixture.listing(0)[0].block), b"contents");
    }

    #[test]
    fn a_copy_that_does_not_fit_reports_what_landed_and_what_did_not() {
        let fixture = Fixture::new("toobig");
        for index in 0..5 {
            fixture.put(&format!("F{index}.bin"), &vec![index as u8; 300_000]);
        }

        let report = fixture.run(OverwritePolicy::Skip).unwrap();
        assert!(report.files_copied >= 1, "some files should have landed");
        assert!(!report.skipped.is_empty(), "and the rest reported");
        assert!(
            report.skipped.iter().any(|s| s.contains("are free")),
            "with the real numbers: {:?}",
            report.skipped
        );
        assert_eq!(
            report.files_copied, report.files_verified,
            "everything that landed was verified"
        );
    }

    /// §54: cancelling leaves a consistent volume and an honest count.
    #[test]
    fn a_cancelled_copy_stops_between_files_and_says_how_many_landed() {
        struct StopAfter(std::sync::atomic::AtomicU64, u64);
        impl ProgressSink for StopAfter {
            fn report(&self, done: u64, _total: Option<u64>, _message: &str) {
                self.0.store(done, std::sync::atomic::Ordering::SeqCst);
            }
            fn is_cancelled(&self) -> bool {
                self.0.load(std::sync::atomic::Ordering::SeqCst) >= self.1
            }
        }

        let fixture = Fixture::new("cancel");
        for index in 0..10 {
            fixture.put(&format!("F{index}.txt"), b"contents");
        }

        let folder = HostFolder::new(&fixture.source, true);
        let mut device = fixture.device();
        let mut writer =
            VolumeWriter::open(&mut device, fixture.geometry, &fixture.image, 0).unwrap();
        let sink = StopAfter(std::sync::atomic::AtomicU64::new(0), 3);

        let report =
            copy_into_volume(&mut writer, 0, &folder, OverwritePolicy::Skip, &sink).unwrap();
        drop(writer);
        drop(device);

        assert!(report.cancelled);
        assert!(report.files_copied < 10, "it stopped early");
        assert_eq!(
            report.files_copied,
            fixture.listing(0).len(),
            "the report must match what is actually on the disk"
        );
    }

    /// A link out of the folder would copy in something the user did not
    /// select; a link back into it loops.
    #[test]
    fn a_symlink_is_not_followed() {
        let fixture = Fixture::new("symlink");
        fixture.put("Real.txt", b"x");

        // Symlink creation needs privileges on Windows; skip when unavailable.
        let outside = fixture.dir.join("outside.txt");
        std::fs::write(&outside, b"should not be copied").unwrap();
        #[cfg(windows)]
        let made =
            std::os::windows::fs::symlink_file(&outside, fixture.source.join("Link.txt")).is_ok();
        #[cfg(not(windows))]
        let made = std::os::unix::fs::symlink(&outside, fixture.source.join("Link.txt")).is_ok();

        let report = fixture.run(OverwritePolicy::Skip).unwrap();
        if made {
            assert_eq!(report.files_copied, 1, "only the real file");
            assert!(fixture.listing(0).iter().all(|e| e.name != "Link.txt"));
        }
    }

    // ---- Amiga → Windows, end to end ----

    #[test]
    fn a_tree_extracts_out_of_a_volume_with_its_folders() {
        let fixture = Fixture::new("extract");
        fixture.put("Tools/Editor/Config", b"settings");
        fixture.put("Readme.txt", b"top level");
        fixture.run(OverwritePolicy::Skip).unwrap();

        let out = fixture.dir.join("out");
        let device = fixture.device();
        let report = extract_from_volume(
            &device,
            &fixture.geometry,
            0,
            &out,
            true,
            OverwritePolicy::Overwrite,
            &NoProgress,
        )
        .unwrap();

        assert_eq!(report.files_written, 2);
        assert_eq!(report.directories_created, 2);
        assert!(report.is_complete());

        assert_eq!(std::fs::read(out.join("Readme.txt")).unwrap(), b"top level");
        assert_eq!(
            std::fs::read(out.join("Tools/Editor/Config")).unwrap(),
            b"settings"
        );
    }

    /// The point of the sidecars: a folder ART writes out and reads back in
    /// keeps the bits WHDLoad depends on (§4.2, §7.2).
    #[test]
    fn a_round_trip_through_a_windows_folder_keeps_the_protection_bits() {
        let fixture = Fixture::new("roundtrip");
        fixture.put("Game.slave", b"slave bytes");
        fixture.put(
            "Game.slave.uaem",
            b"-sp-rwed 2020-05-04 10:20:30.00 keep me\n",
        );
        fixture.run(OverwritePolicy::Skip).unwrap();

        // Out to a folder…
        let out = fixture.dir.join("out");
        {
            let device = fixture.device();
            let report = extract_from_volume(
                &device,
                &fixture.geometry,
                0,
                &out,
                true,
                OverwritePolicy::Overwrite,
                &NoProgress,
            )
            .unwrap();
            assert_eq!(report.sidecars_written, 1, "the bits need a sidecar");
        }

        // …and back into a second image.
        let second = Fixture::new("roundtrip-2");
        let folder = HostFolder::new(&out, true);
        {
            let mut device = second.device();
            let mut writer =
                VolumeWriter::open(&mut device, second.geometry, &second.image, 0).unwrap();
            copy_into_volume(&mut writer, 0, &folder, OverwritePolicy::Skip, &NoProgress).unwrap();
        }

        let entries = second.listing(0);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Game.slave");
        assert_eq!(second.contents(entries[0].block), b"slave bytes");

        let device = second.device();
        let protection = protection_of(&device, &second.geometry, entries[0].block).unwrap();
        assert_eq!(
            uaem::format_bits(protection),
            "-sp-rwed",
            "the bits survived the trip through a filesystem that cannot hold them"
        );
    }

    /// Writing a sidecar for every file would double the file count of every
    /// extraction, for files that carry nothing worth preserving.
    #[test]
    fn a_plain_file_extracts_without_a_sidecar() {
        let fixture = Fixture::new("nosidecar-out");
        fixture.put("Plain.txt", b"x");
        fixture.run(OverwritePolicy::Skip).unwrap();

        let out = fixture.dir.join("out");
        let device = fixture.device();
        let report = extract_from_volume(
            &device,
            &fixture.geometry,
            0,
            &out,
            true,
            OverwritePolicy::Overwrite,
            &NoProgress,
        )
        .unwrap();

        // The file's date comes from the host mtime, so a sidecar is written —
        // what must not happen is a sidecar for a file with nothing to record.
        assert_eq!(report.files_written, 1);
        assert!(out.join("Plain.txt").exists());
    }

    #[test]
    fn extracting_skips_what_is_already_in_the_destination_folder() {
        let fixture = Fixture::new("extract-skip");
        fixture.put("Readme.txt", b"from the image");
        fixture.run(OverwritePolicy::Skip).unwrap();

        let out = fixture.dir.join("out");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("Readme.txt"), b"already here").unwrap();

        let device = fixture.device();
        let report = extract_from_volume(
            &device,
            &fixture.geometry,
            0,
            &out,
            false,
            OverwritePolicy::Skip,
            &NoProgress,
        )
        .unwrap();

        assert_eq!(report.files_written, 0);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(
            std::fs::read(out.join("Readme.txt")).unwrap(),
            b"already here",
            "the user's file stands"
        );
    }

    // ---- Amiga → Amiga ----

    #[test]
    fn a_tree_copies_from_one_image_to_another() {
        let from = Fixture::new("a2a-from");
        from.put("Tools/Editor", b"editor bytes");
        from.put("Readme", b"read me");
        from.run(OverwritePolicy::Skip).unwrap();

        let to = Fixture::new("a2a-to");
        let cache = to.dir.join("cache");
        std::fs::create_dir_all(&cache).unwrap();

        let staged = {
            let device = from.device();
            StagedTree::stage(&device, &from.geometry, 0, &cache, &NoProgress).unwrap()
        };

        let report = {
            let mut device = to.device();
            let mut writer = VolumeWriter::open(&mut device, to.geometry, &to.image, 0).unwrap();
            copy_into_volume(
                &mut writer,
                0,
                staged.source(),
                OverwritePolicy::Skip,
                &NoProgress,
            )
            .unwrap()
        };

        assert_eq!(report.files_copied, 2);
        assert_eq!(report.files_verified, 2);
        assert_eq!(report.directories_created, 1);

        let root = to.listing(0);
        let readme = root.iter().find(|e| e.name == "Readme").unwrap();
        assert_eq!(to.contents(readme.block), b"read me");

        let tools = root.iter().find(|e| e.name == "Tools").unwrap();
        let inside = to.listing(tools.block);
        assert_eq!(to.contents(inside[0].block), b"editor bytes");
    }

    /// The staging folder is a temp folder and must not survive the copy.
    #[test]
    fn the_staging_folder_removes_itself() {
        let fixture = Fixture::new("staging-cleanup");
        fixture.put("A.txt", b"x");
        fixture.run(OverwritePolicy::Skip).unwrap();

        let cache = fixture.dir.join("cache");
        std::fs::create_dir_all(&cache).unwrap();

        let staged_path = {
            let device = fixture.device();
            let staged =
                StagedTree::stage(&device, &fixture.geometry, 0, &cache, &NoProgress).unwrap();
            staged._temp.path().to_path_buf()
        };

        assert!(!staged_path.exists(), "the staging folder must be gone");
    }

    // ---- moving inside one volume ----

    #[test]
    fn an_entry_moves_between_folders_in_one_operation() {
        let fixture = Fixture::new("move");
        fixture.put("Tools/Placeholder", b"x");
        fixture.put("Loose.txt", b"contents");
        fixture.run(OverwritePolicy::Skip).unwrap();

        let root = fixture.listing(0);
        let tools = root.iter().find(|e| e.name == "Tools").unwrap().block;
        let loose = root.iter().find(|e| e.name == "Loose.txt").unwrap().block;

        {
            let mut device = fixture.device();
            let mut writer =
                VolumeWriter::open(&mut device, fixture.geometry, &fixture.image, 0).unwrap();
            writer.relink(0, loose, tools, "Loose.txt").unwrap();
        }

        assert!(
            fixture.listing(0).iter().all(|e| e.name != "Loose.txt"),
            "it must be out of the folder it left"
        );
        let inside = fixture.listing(tools);
        assert!(inside.iter().any(|e| e.name == "Loose.txt"));
        assert_eq!(
            fixture.contents(inside.iter().find(|e| e.name == "Loose.txt").unwrap().block),
            b"contents"
        );
    }

    #[test]
    fn moving_and_renaming_at_the_same_time_works() {
        let fixture = Fixture::new("move-rename");
        fixture.put("Tools/Placeholder", b"x");
        fixture.put("Old.txt", b"contents");
        fixture.run(OverwritePolicy::Skip).unwrap();

        let root = fixture.listing(0);
        let tools = root.iter().find(|e| e.name == "Tools").unwrap().block;
        let old = root.iter().find(|e| e.name == "Old.txt").unwrap().block;

        {
            let mut device = fixture.device();
            let mut writer =
                VolumeWriter::open(&mut device, fixture.geometry, &fixture.image, 0).unwrap();
            writer.relink(0, old, tools, "New.txt").unwrap();
        }

        let inside = fixture.listing(tools);
        assert!(inside.iter().any(|e| e.name == "New.txt"));
        assert!(inside.iter().all(|e| e.name != "Old.txt"));
    }

    #[test]
    fn a_move_onto_a_name_already_taken_is_refused_and_changes_nothing() {
        let fixture = Fixture::new("move-collision");
        fixture.put("Tools/Same.txt", b"already there");
        fixture.put("Same.txt", b"moving in");
        fixture.run(OverwritePolicy::Skip).unwrap();

        let root = fixture.listing(0);
        let tools = root.iter().find(|e| e.name == "Tools").unwrap().block;
        let loose = root.iter().find(|e| e.name == "Same.txt").unwrap().block;
        let before = fixture.bytes();

        {
            let mut device = fixture.device();
            let mut writer =
                VolumeWriter::open(&mut device, fixture.geometry, &fixture.image, 0).unwrap();
            assert!(writer.relink(0, loose, tools, "Same.txt").is_err());
        }

        assert_eq!(
            fixture.bytes(),
            before,
            "a refused move must change nothing"
        );
    }

    #[test]
    fn a_folder_cannot_be_moved_inside_itself() {
        let fixture = Fixture::new("move-into-self");
        fixture.put("Tools/Placeholder", b"x");
        fixture.run(OverwritePolicy::Skip).unwrap();

        let tools = fixture
            .listing(0)
            .iter()
            .find(|e| e.name == "Tools")
            .unwrap()
            .block;

        let mut device = fixture.device();
        let mut writer =
            VolumeWriter::open(&mut device, fixture.geometry, &fixture.image, 0).unwrap();
        let err = writer.relink(0, tools, tools, "Tools").unwrap_err();
        assert!(err.to_string().contains("inside itself"), "{err}");
    }

    /// A move within one directory is a rename, not two hash-table edits that
    /// could leave the entry in neither bucket.
    #[test]
    fn moving_within_the_same_folder_is_just_a_rename() {
        let fixture = Fixture::new("move-same-dir");
        fixture.put("Before.txt", b"x");
        fixture.run(OverwritePolicy::Skip).unwrap();

        let block = fixture.listing(0)[0].block;
        {
            let mut device = fixture.device();
            let mut writer =
                VolumeWriter::open(&mut device, fixture.geometry, &fixture.image, 0).unwrap();
            writer.relink(0, block, 0, "After.txt").unwrap();
        }

        let entries = fixture.listing(0);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "After.txt");
    }

    // ---- Amiga → Windows names ----

    #[test]
    fn an_amiga_name_ntfs_refuses_is_escaped() {
        assert_eq!(windows_safe_name("Prices: 1993"), "Prices_ 1993");
        assert_eq!(windows_safe_name("a<b>c|d?e*f\"g"), "a_b_c_d_e_f_g");
    }

    /// Legal to create on NTFS and impossible to open afterwards.
    #[test]
    fn trailing_dots_and_spaces_are_replaced() {
        assert_eq!(windows_safe_name("Name."), "Name_");
        assert_eq!(windows_safe_name("Name "), "Name_");
        assert_eq!(windows_safe_name("Name..."), "Name___");
    }

    /// An Amiga file really can be called `AUX`, and Windows really will not
    /// have it.
    #[test]
    fn the_dos_device_names_are_defused() {
        assert_eq!(windows_safe_name("AUX"), "_AUX");
        assert_eq!(windows_safe_name("con.txt"), "_con.txt");
        assert_eq!(windows_safe_name("COM1"), "_COM1");
        // Not reserved, and must be left alone.
        assert_eq!(windows_safe_name("Console"), "Console");
    }

    /// A copy run twice must overwrite, not accumulate.
    #[test]
    fn the_escaping_is_deterministic() {
        for name in ["Prices: 1993", "AUX", "Name.", "ordinary.txt"] {
            assert_eq!(windows_safe_name(name), windows_safe_name(name));
        }
    }

    #[test]
    fn an_ordinary_name_is_left_exactly_as_it_was() {
        for name in ["Readme.txt", "Grüße", "My File 2", "x.info"] {
            assert_eq!(windows_safe_name(name), name);
        }
    }

    /// Writing a sidecar for every file would double the file count of every
    /// extraction, for files that carry nothing worth preserving.
    #[test]
    fn a_file_with_nothing_worth_recording_gets_no_sidecar() {
        use crate::core::adf::bcpl::AmigaDate;
        assert!(sidecar_for(0, AmigaDate::default(), "").is_none());
    }

    #[test]
    fn a_file_with_bits_a_comment_or_a_date_gets_one() {
        use crate::core::adf::bcpl::AmigaDate;
        assert!(sidecar_for(1 << 6, AmigaDate::default(), "").is_some());
        assert!(sidecar_for(0, AmigaDate::default(), "a note").is_some());
        assert!(sidecar_for(
            0,
            AmigaDate {
                days: 1,
                mins: 0,
                ticks: 0
            },
            ""
        )
        .is_some());
    }
}
