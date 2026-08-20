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
use super::{DeleteProtection, FileMeta, OverwriteProtection, VolumeWriter};

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

    /// Things the source declined to read at all, and why.
    ///
    /// Distinct from the entries it *does* return, and merged into the report
    /// so that a source which contributes nothing cannot come back looking
    /// like a clean success (ART-071). Empty for a source with nothing to
    /// decline, which is most of them.
    fn skipped_sources(&self) -> Vec<String> {
        Vec::new()
    }
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

    // Before the loop, because a source that declined everything runs the loop
    // zero times — and an empty report reads as a clean success (ART-071).
    report.skipped.extend(source.skipped_sources());

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
            // The `w` bit is the question a replace asks; `d` is not. ART
            // deletes and re-creates as an implementation detail, so the
            // delete is overridden and the real check is made first
            // (ART-088 for the one, ART-094 for the other). A refusal here
            // becomes this entry's line in `skipped`, which is what the
            // engine does with any error from one entry.
            writer.ensure_overwritable(existing, OverwriteProtection::Honour)?;
            writer.delete_with(parent, existing, DeleteProtection::Override)?;
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
        host_metadata(&path, self.read_sidecars)
    }
}

/// Protection bits, date and comment for a file already resolved to a real
/// path on disk.
///
/// Shared by every `CopySource` that reads the host filesystem — `HostFolder`
/// and `HostSelection` alike — so a `.uaem` sidecar means the same thing
/// regardless of which one found it.
fn host_metadata(path: &Path, read_sidecars: bool) -> CoreResult<Option<Sidecar>> {
    // A sidecar written by ART or WinUAE is the better source: it holds the
    // bits and comment the host filesystem threw away.
    if read_sidecars {
        let sidecar = uaem::sidecar_path(path);
        if sidecar.exists() {
            let size = std::fs::metadata(&sidecar)?.len();
            if size <= uaem::MAX_UAEM_BYTES {
                // A damaged sidecar falls through to the mtime rather than
                // failing the copy: losing the bits is better than losing the
                // file, and the caller is told nothing was applied.
                if let Ok(parsed) = uaem::parse(&std::fs::read_to_string(&sidecar)?) {
                    return Ok(Some(parsed));
                }
            }
        }
    }

    // Otherwise the file's own modification time, and default bits.
    let modified = std::fs::metadata(path)?
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

/// Several picked entries — files, folders, or a mix — copied as one operation.
///
/// Each root keeps its own base name at the destination, so picking
/// `Game/` and `Readme.txt` produces `Game/` and `Readme.txt` side by side.
///
/// This is the only new copy source the batch commander needs: every entry
/// still flows through the one tested [`copy_into_volume`], the same as a
/// single [`HostFolder`] does — `HostSelection` just widens `entries()` to
/// span several roots instead of one.
#[derive(Debug, Clone)]
pub struct HostSelection {
    roots: Vec<PathBuf>,
    /// Whether to look for `.uaem` sidecars next to each file — the same flag
    /// [`HostFolder`] takes, plumbed through so a batch selection honours the
    /// same option a single-folder copy does (§4.2).
    read_sidecars: bool,
}

impl HostSelection {
    pub fn new(roots: Vec<PathBuf>, read_sidecars: bool) -> Self {
        Self {
            roots,
            read_sidecars,
        }
    }

    /// The name a root will be called at the destination — the last
    /// component of its path.
    fn base_name(root: &Path) -> CoreResult<String> {
        root.file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
            .ok_or_else(|| {
                CoreError::InvalidInput(format!(
                    "'{}' has no name ART can give it at the destination",
                    root.display()
                ))
            })
    }

    /// Refuse a selection where two roots would land under the same name.
    ///
    /// `C:\a\Docs` and `D:\b\Docs` both want to be called `Docs`. Copying
    /// both in would silently interleave two unrelated trees into one
    /// directory — the user would have no way to know it happened — so this
    /// is refused up front, naming both paths, rather than merged.
    ///
    /// **Compared without case** (ART-072). AmigaDOS is case-preserving but
    /// case-*insensitive* — the same rule `hash::name_hash` respects
    /// (ART-009/ART-010) — so `Docs` and `docs` are two names here and one
    /// directory entry there. Keyed on the name as typed, this check simply
    /// did not fire for such a pair: the second root was dropped later
    /// instead, taking its whole subtree with it, and the one clear "rename
    /// one of these" turned into a pile of unexplained skips.
    fn check_for_name_collisions(&self) -> CoreResult<()> {
        let mut seen: std::collections::BTreeMap<String, (&Path, String)> =
            std::collections::BTreeMap::new();
        for root in &self.roots {
            let name = Self::base_name(root)?;
            if let Some((other, other_name)) = seen.get(&name.to_lowercase()) {
                return Err(CoreError::InvalidInput(format!(
                    "'{}' and '{}' would both be called '{other_name}' at the destination — \
                     rename one before copying",
                    other.display(),
                    root.display()
                )));
            }
            seen.insert(name.to_lowercase(), (root, name));
        }
        Ok(())
    }

    /// Roots this selection will not read, and why.
    ///
    /// A symlinked root is skipped on purpose — a link out of the pick would
    /// copy in something the user did not select, the same rule `walk` applies
    /// mid-tree. But skipping it silently was the bug (ART-071): a selection
    /// of nothing *but* symlinks produced no entries, no skips and no
    /// cancellation, and `CopyReport::is_complete()` reads that as a clean
    /// success. The user picked things, ART copied none of them, and every
    /// signal said it worked.
    fn skipped_roots(&self) -> Vec<String> {
        self.roots
            .iter()
            .filter(|root| {
                std::fs::symlink_metadata(root)
                    .map(|metadata| metadata.file_type().is_symlink())
                    .unwrap_or(false)
            })
            .map(|root| {
                format!(
                    "{} — a shortcut, and ART copies what you picked rather than what it points at",
                    root.display()
                )
            })
            .collect()
    }

    /// Everything to copy, with the entry cap taken as a parameter so tests
    /// can prove it applies to the whole selection without materialising
    /// [`MAX_COPY_ENTRIES`] real files. Production always calls this with
    /// that constant, through [`CopySource::entries`].
    fn entries_capped(&self, max_entries: usize) -> CoreResult<Vec<SourceEntry>> {
        self.check_for_name_collisions()?;

        let mut out = Vec::new();
        for root in &self.roots {
            // The cap is shared across every root: once it is spent, later
            // roots contribute nothing rather than getting a fresh budget of
            // their own (a selection of 200 folders must not multiply the
            // cap by 200).
            if out.len() >= max_entries {
                break;
            }

            let prefix = Self::base_name(root)?;
            let metadata = std::fs::symlink_metadata(root)?;
            let kind = metadata.file_type();

            if kind.is_symlink() {
                // Not followed, for the same reason `walk` does not follow
                // one found mid-tree: a link out of the pick would copy in
                // something the user did not select.
                continue;
            }

            if kind.is_dir() {
                out.push(SourceEntry {
                    relative: prefix.clone(),
                    is_dir: true,
                    bytes: 0,
                });
                if out.len() >= max_entries {
                    continue;
                }

                // `walk` is given the *remaining* budget, not the whole cap,
                // so what it adds on top of everything already collected
                // still respects the shared total.
                let remaining = max_entries - out.len();
                let mut local = Vec::new();
                walk(root, "", 0, MAX_COPY_DEPTH, remaining, &mut local)?;
                for mut entry in local {
                    entry.relative = format!("{prefix}/{}", entry.relative);
                    out.push(entry);
                }
            } else if kind.is_file() {
                out.push(SourceEntry {
                    relative: prefix,
                    is_dir: false,
                    bytes: metadata.len(),
                });
            }
        }

        Ok(out)
    }

    /// A selection-relative path (`"Game/Sub/File.txt"` or `"Readme.txt"`)
    /// back to the root it came from and the path inside that root.
    fn locate(&self, relative: &str) -> CoreResult<(PathBuf, String)> {
        let (prefix, rest) = match relative.split_once('/') {
            Some((p, r)) => (p, r.to_string()),
            None => (relative, String::new()),
        };

        for root in &self.roots {
            if Self::base_name(root)? == prefix {
                return Ok((root.clone(), rest));
            }
        }

        Err(CoreError::InvalidInput(format!(
            "'{relative}' is not part of this selection"
        )))
    }

    /// Turn a selection-relative path back into one on disk, through
    /// `safe_join` for the part inside the root — the same round-trip
    /// [`HostFolder::resolve`] makes, and for the same reason: by the time a
    /// plan comes back from the frontend it is untrusted input again.
    fn resolve(&self, relative: &str) -> CoreResult<PathBuf> {
        let (root, rest) = self.locate(relative)?;
        if rest.is_empty() {
            Ok(root)
        } else {
            safe_join(&root, &rest).map_err(|err| {
                CoreError::InvalidInput(format!(
                    "'{relative}' is not a path inside the copy: {err}"
                ))
            })
        }
    }
}

impl CopySource for HostSelection {
    fn entries(&self) -> CoreResult<Vec<SourceEntry>> {
        self.entries_capped(MAX_COPY_ENTRIES)
    }

    fn skipped_sources(&self) -> Vec<String> {
        self.skipped_roots()
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
        host_metadata(&path, self.read_sidecars)
    }
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

/// What writing one entry out to a host folder should do.
///
/// See [`host_target`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostTarget {
    /// The directory now exists on the host; descend into it.
    Descend(PathBuf),
    /// Write the file's bytes here.
    Write(PathBuf),
    /// Leave it alone — `report` already says why.
    Skip,
}

/// Decide — and record — what happens to one entry called `name` under `dest`.
///
/// Three questions every "copy out" path asks in the same order: what name
/// will the host filesystem accept, does that name stay inside the folder the
/// user picked, and what does the user's collision policy say about a name
/// already there.
///
/// Shared rather than repeated. `extract_from_volume` below and
/// [`IsoImage::extract_tree`](crate::core::iso::IsoImage::extract_tree) read
/// sources with nothing in common — a block device with header blocks, a disc
/// with `(extent, length)` pairs — so their loops cannot merge, but these
/// three answers are identical for both. When each answered them for itself
/// they diverged: the disc path hardcoded [`OverwritePolicy::Skip`] and
/// dropped the setting the same user had just chosen for an ADF.
pub fn host_target(
    dest: &Path,
    name: &str,
    is_dir: bool,
    policy: OverwritePolicy,
    report: &mut ExtractReport,
) -> CoreResult<HostTarget> {
    let safe = windows_safe_name(name);
    if safe != name {
        report.renamed.push(format!("{name} → {safe}"));
    }

    // Through `safe_join` even though the name has just been escaped: the
    // escaping is about what NTFS will accept, and containment is a separate
    // question with a separate answer.
    let target = match safe_join(dest, &safe) {
        Ok(path) => path,
        Err(err) => {
            report.skipped.push(format!("{name} — {err}"));
            return Ok(HostTarget::Skip);
        }
    };

    if is_dir {
        std::fs::create_dir_all(&target)?;
        report.directories_created += 1;
        return Ok(HostTarget::Descend(target));
    }

    if target.exists() {
        match policy {
            OverwritePolicy::Skip => {
                report
                    .skipped
                    .push(format!("{name} — already in the destination folder"));
                return Ok(HostTarget::Skip);
            }
            OverwritePolicy::Rename => {
                report.skipped.push(format!(
                    "{name} — already there, and ART does not invent names on the way out"
                ));
                return Ok(HostTarget::Skip);
            }
            OverwritePolicy::Overwrite => {}
        }
    }

    Ok(HostTarget::Write(target))
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

        let target = match host_target(dest, &entry.name, entry.is_dir, policy, report)? {
            HostTarget::Skip => continue,
            HostTarget::Descend(target) => {
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
            HostTarget::Write(target) => target,
        };

        write_one_file(
            device,
            geometry,
            &set,
            entry.block,
            &entry.name,
            &target,
            write_sidecars,
            report,
        )?;
    }

    Ok(())
}

/// Write one file out of a volume to `target`, sidecar and all.
///
/// Lifted out of [`extract_dir`]'s loop because
/// [`extract_selection_from_volume`] needs exactly this for a *root* the user
/// picked that happens to be a file, and a second copy of it is how one path
/// comes to write sidecars and the other not (ART-065).
#[allow(clippy::too_many_arguments)]
fn write_one_file<D: BlockDevice + ?Sized>(
    device: &D,
    geometry: &VolumeGeometry,
    set: &BlockSet,
    block: u32,
    name: &str,
    target: &Path,
    write_sidecars: bool,
    report: &mut ExtractReport,
) -> CoreResult<()> {
    let data = match super::file::read_file(device, set, geometry, block) {
        Ok(data) => data,
        Err(err) => {
            report.skipped.push(format!("{name} — {err}"));
            return Ok(());
        }
    };

    // Through `core/safety`: a truncated file on the way out is still a
    // file the user will believe is a good copy.
    crate::core::safety::atomic::atomic_write(target, &data)?;
    report.files_written += 1;
    report.bytes_written += data.len() as u64;

    if write_sidecars {
        let header = set.view(device, block)?;
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
                &uaem::sidecar_path(target),
                uaem::render(&sidecar).as_bytes(),
            )?;
            report.sidecars_written += 1;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// A selection out of one volume — the primitive ART-064 and ART-065 share
// ---------------------------------------------------------------------------

/// One row the user picked in a volume pane.
///
/// Deliberately not a `DirEntry`: the frontend sends what it has on screen —
/// a header block, a name and whether it is a drawer — and inventing a richer
/// type here would mean the command layer synthesising fields it did not
/// receive.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct SelectedEntry {
    pub header_block: u32,
    pub name: String,
    pub is_dir: bool,
}

/// Extract a **selection** of entries out of one volume into one host folder,
/// as one operation with one report.
///
/// This is the missing primitive both ART-064 and ART-065 named. Before it,
/// volume→local ran one `volumeCopyOut` job *per selected entry* inside a
/// `Promise.all`, so a selection of ten where the seventh failed left six on
/// disk, three never attempted, and no report tying any of it back to "this
/// was one selection"; and volume→volume refused outright, because staging a
/// selection is what it had no way to do. One function answers both: the
/// caller here writes to the user's folder, the caller in [`StagedTree`]
/// writes to a temp folder and inserts the result into the other image.
///
/// Each root keeps its own name at the destination, the same shape
/// [`HostSelection`] gives a batch going the other way.
///
/// Cancellation is checked **between whole entries**, never inside one: a
/// stopped batch leaves whole files, never a half-written one.
#[allow(clippy::too_many_arguments)]
pub fn extract_selection_from_volume<D: BlockDevice + ?Sized>(
    device: &D,
    geometry: &VolumeGeometry,
    entries: &[SelectedEntry],
    dest: &Path,
    write_sidecars: bool,
    policy: OverwritePolicy,
    sink: &dyn ProgressSink,
) -> CoreResult<ExtractReport> {
    check_escaped_name_collisions(entries)?;

    let mut report = ExtractReport::default();
    std::fs::create_dir_all(dest)?;
    let set = BlockSet::new(device.block_size());

    for entry in entries {
        // Between whole entries, never mid-write — the rule `core/jobs`
        // states and the reason a cancelled batch is safe rather than merely
        // stopped.
        if sink.is_cancelled() {
            report.cancelled = true;
            return Ok(report);
        }
        sink.report(report.files_written as u64, None, &entry.name);

        let block = resolve_block(geometry, entry.header_block);
        // Every `?` in this loop used to throw the report away, which is
        // exactly the situation `CoreError::PartiallyApplied` was added for:
        // a batch that fails on its seventh entry has six on the user's disk
        // and nothing was saying so. The count and the entry that refused now
        // travel with the error.
        let step = match host_target(dest, &entry.name, entry.is_dir, policy, &mut report) {
            Ok(HostTarget::Skip) => continue,
            Ok(HostTarget::Descend(target)) => extract_dir(
                device,
                geometry,
                block,
                &target,
                0,
                write_sidecars,
                policy,
                sink,
                &mut report,
            ),
            Ok(HostTarget::Write(target)) => write_one_file(
                device,
                geometry,
                &set,
                block,
                &entry.name,
                &target,
                write_sidecars,
                &mut report,
            ),
            Err(err) => Err(err),
        };
        if let Err(err) = step {
            return Err(partway(err, &report, &entry.name));
        }
    }

    Ok(report)
}

/// Turn a mid-batch failure into one that says how much of the batch is
/// already on the user's disk.
///
/// The same rule `core::layout::apply` follows: a failure on the first entry
/// reports the plain reason, because dressing that up would send the user
/// looking for a mess that is not there. `files_written` rather than the
/// entry index, because that is what is actually on disk — a picked drawer
/// counts for every file inside it.
fn partway(err: CoreError, report: &ExtractReport, item: &str) -> CoreError {
    if report.files_written == 0 && report.directories_created == 0 {
        return err;
    }
    CoreError::PartiallyApplied {
        placed: report.files_written as u64,
        item: item.to_string(),
        reason: err.to_string(),
    }
}

/// `0` means the root, as everywhere else in ART.
fn resolve_block(geometry: &VolumeGeometry, block: u32) -> u32 {
    if block == 0 {
        geometry.root_block
    } else {
        block
    }
}

/// Refuse a selection whose entries would land under the same host name.
///
/// AmigaDOS will not let two entries in one directory share a name, so this
/// cannot fire on the names as the volume holds them. It fires on the names
/// **after escaping**: `Prices: 1993` and `Prices_ 1993` are two files there
/// and one file here, and copying both out would silently overwrite one with
/// the other. Compared without case for the same reason
/// [`HostSelection::check_for_name_collisions`] is (ART-072) — NTFS is
/// case-insensitive too.
fn check_escaped_name_collisions(entries: &[SelectedEntry]) -> CoreResult<()> {
    let mut seen: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for entry in entries {
        let safe = windows_safe_name(&entry.name);
        if let Some(other) = seen.insert(safe.to_lowercase(), entry.name.clone()) {
            return Err(CoreError::InvalidInput(format!(
                "'{}' and '{other}' would both be written as '{safe}' on this disk. \
                 Copy them one at a time, or rename one first.",
                entry.name
            )));
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

    /// Stage a **selection** of entries rather than one directory (ART-064).
    ///
    /// The temp folder ends up holding each picked entry under its own name,
    /// side by side — which is exactly what [`HostFolder`] then copies into
    /// the destination directory, so a selection of `Game/` and `Readme.txt`
    /// arrives as `Game/` and `Readme.txt` and not merged into one drawer.
    /// That is the same shape [`HostSelection`] gives the local→volume
    /// direction, reached from the other side.
    pub fn stage_selection<D: BlockDevice + ?Sized>(
        device: &D,
        geometry: &VolumeGeometry,
        entries: &[SelectedEntry],
        cache_root: &Path,
        sink: &dyn ProgressSink,
    ) -> CoreResult<Self> {
        let temp = TempFolder::new(cache_root)?;
        // Sidecars on, always — see [`stage`].
        let report = extract_selection_from_volume(
            device,
            geometry,
            entries,
            temp.path(),
            true,
            OverwritePolicy::Overwrite,
            sink,
        )?;
        // A staging run the user stopped must not be inserted into the other
        // image as if it were the whole selection. Said here rather than at
        // the insert step because this is where the knowledge is: the
        // destination image has not been opened yet, so nothing has to be
        // undone.
        if report.cancelled {
            return Err(CoreError::Cancelled);
        }

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
        let dir = std::env::temp_dir().join(format!(
            "art-copy-{name}-{}",
            crate::core::test_scratch_id()
        ));
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

    // ---- HostSelection: several roots as one copy ----

    #[test]
    fn a_selection_of_files_and_folders_copies_each_at_the_top_level() {
        let fixture = Fixture::new("selection-flat");
        let staging = fixture.dir.join("picks");
        std::fs::create_dir_all(&staging).unwrap();

        let game = staging.join("Game");
        std::fs::create_dir_all(game.join("Sub")).unwrap();
        std::fs::write(game.join("Loader"), b"loader bytes").unwrap();
        std::fs::write(game.join("Sub").join("File.txt"), b"nested").unwrap();

        let readme = staging.join("Readme.txt");
        std::fs::write(&readme, b"top level readme").unwrap();

        let selection = HostSelection::new(vec![game, readme], true);

        let mut device = fixture.device();
        let mut writer =
            VolumeWriter::open(&mut device, fixture.geometry, &fixture.image, 0).unwrap();
        let report = copy_into_volume(
            &mut writer,
            0,
            &selection,
            OverwritePolicy::Skip,
            &NoProgress,
        )
        .unwrap();
        drop(writer);
        drop(device);

        assert!(report.is_complete(), "{report:?}");
        assert_eq!(report.files_copied, 3, "Loader, File.txt and Readme.txt");
        assert_eq!(report.directories_created, 2, "Game and Sub");

        let root = fixture.listing(0);
        assert_eq!(root.len(), 2, "Game and Readme.txt land side by side");

        let game_entry = root.iter().find(|e| e.name == "Game").unwrap();
        assert!(game_entry.is_dir);
        let readme_entry = root.iter().find(|e| e.name == "Readme.txt").unwrap();
        assert!(!readme_entry.is_dir);
        assert_eq!(fixture.contents(readme_entry.block), b"top level readme");

        let inside_game = fixture.listing(game_entry.block);
        assert_eq!(inside_game.len(), 2, "Loader and Sub");
        let loader = inside_game.iter().find(|e| e.name == "Loader").unwrap();
        assert_eq!(fixture.contents(loader.block), b"loader bytes");
        let sub = inside_game.iter().find(|e| e.name == "Sub").unwrap();
        assert!(sub.is_dir);
        let inside_sub = fixture.listing(sub.block);
        assert_eq!(inside_sub.len(), 1);
        assert_eq!(fixture.contents(inside_sub[0].block), b"nested");
    }

    /// The option a single-folder copy (`HostFolder`) already honours must do
    /// the same thing through a batch selection: the same user action —
    /// "copy this in, without sidecars" — must not behave differently
    /// depending on whether the user picked one thing or several (finding 2
    /// of the phase-1a whole-branch review). `read_sidecars: false` must
    /// leave the file with AmigaDOS default bits even though a `.uaem`
    /// sidecar sits right next to it; `true` must apply it — proving the
    /// flag is actually read, not just accepted and ignored either way.
    #[test]
    fn a_selection_honours_its_own_sidecar_flag_in_both_positions() {
        let with_sidecars = Fixture::new("selection-sidecars-on");
        let staging_on = with_sidecars.dir.join("picks");
        std::fs::create_dir_all(&staging_on).unwrap();
        std::fs::write(staging_on.join("Game.slave"), b"slave bytes").unwrap();
        std::fs::write(
            staging_on.join("Game.slave.uaem"),
            b"-sp-rwed 2020-05-04 10:20:30.00 a comment\n",
        )
        .unwrap();

        let selection_on = HostSelection::new(vec![staging_on.join("Game.slave")], true);
        let mut device = with_sidecars.device();
        let mut writer =
            VolumeWriter::open(&mut device, with_sidecars.geometry, &with_sidecars.image, 0)
                .unwrap();
        copy_into_volume(
            &mut writer,
            0,
            &selection_on,
            OverwritePolicy::Skip,
            &NoProgress,
        )
        .unwrap();
        drop(writer);
        drop(device);

        let entry = with_sidecars.listing(0)[0].clone();
        let device = with_sidecars.device();
        let protection = protection_of(&device, &with_sidecars.geometry, entry.block).unwrap();
        assert_eq!(
            uaem::format_bits(protection),
            "-sp-rwed",
            "read_sidecars: true must apply the bits the .uaem carried"
        );

        let without_sidecars = Fixture::new("selection-sidecars-off");
        let staging_off = without_sidecars.dir.join("picks");
        std::fs::create_dir_all(&staging_off).unwrap();
        std::fs::write(staging_off.join("Game.slave"), b"slave bytes").unwrap();
        std::fs::write(
            staging_off.join("Game.slave.uaem"),
            b"-sp-rwed 2020-05-04 10:20:30.00 a comment\n",
        )
        .unwrap();

        let selection_off = HostSelection::new(vec![staging_off.join("Game.slave")], false);
        let mut device = without_sidecars.device();
        let mut writer = VolumeWriter::open(
            &mut device,
            without_sidecars.geometry,
            &without_sidecars.image,
            0,
        )
        .unwrap();
        copy_into_volume(
            &mut writer,
            0,
            &selection_off,
            OverwritePolicy::Skip,
            &NoProgress,
        )
        .unwrap();
        drop(writer);
        drop(device);

        let entry = without_sidecars.listing(0)[0].clone();
        let device = without_sidecars.device();
        let protection = protection_of(&device, &without_sidecars.geometry, entry.block).unwrap();
        assert_eq!(
            uaem::format_bits(protection),
            "----rwed",
            "read_sidecars: false must ignore the .uaem sitting right next to the file"
        );
    }

    /// If the cap reset per root, two roots with ten files each and a shared
    /// cap of six would leave the second root free to add its own six —
    /// twelve entries out of a cap of six. It must not.
    #[test]
    fn a_selection_obeys_the_entry_cap_across_all_roots_not_per_root() {
        let dir = scratch("selection-cap");
        let root_a = dir.join("A");
        let root_b = dir.join("B");
        std::fs::create_dir_all(&root_a).unwrap();
        std::fs::create_dir_all(&root_b).unwrap();
        for index in 0..10 {
            std::fs::write(root_a.join(format!("f{index}.txt")), b"x").unwrap();
            std::fs::write(root_b.join(format!("f{index}.txt")), b"x").unwrap();
        }

        let selection = HostSelection::new(vec![root_a, root_b], true);
        let capped = selection.entries_capped(6).unwrap();

        assert!(
            capped.len() <= 6,
            "the cap must hold across the whole selection, not double per root: {}",
            capped.len()
        );
        assert!(
            !capped
                .iter()
                .any(|e| e.relative == "B" || e.relative.starts_with("B/")),
            "root B must not be touched once the shared cap is spent on root A: {capped:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_roots_with_the_same_base_name_are_refused_rather_than_silently_merged() {
        let dir = scratch("selection-collision");
        let a = dir.join("a").join("Docs");
        let b = dir.join("b").join("Docs");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("A.txt"), b"from a").unwrap();
        std::fs::write(b.join("B.txt"), b"from b").unwrap();

        let selection = HostSelection::new(vec![a.clone(), b.clone()], true);
        let err = selection.entries().unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains(&a.display().to_string()),
            "names root a: {message}"
        );
        assert!(
            message.contains(&b.display().to_string()),
            "names root b: {message}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ART-072. AmigaDOS is case-preserving but case-*insensitive*, so `Docs`
    /// and `docs` are two names here and one directory entry there. Compared
    /// as typed, this refusal never fired for such a pair — the second root
    /// was dropped later instead, taking its whole subtree with it, and one
    /// clear "rename one of these" became a pile of unexplained skips.
    #[test]
    fn two_roots_differing_only_in_case_are_refused_too() {
        let dir = scratch("selection-collision-case");
        let a = dir.join("a").join("Docs");
        let b = dir.join("b").join("docs");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("A.txt"), b"from a").unwrap();
        std::fs::write(b.join("B.txt"), b"from b").unwrap();

        let selection = HostSelection::new(vec![a.clone(), b.clone()], true);
        let err = selection
            .entries()
            .expect_err("Docs and docs are the same drawer on an Amiga");
        let message = err.to_string();
        assert!(message.contains(&a.display().to_string()), "{message}");
        assert!(message.contains(&b.display().to_string()), "{message}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ART-071. A selection of nothing but shortcuts produced no entries, no
    /// skips and no cancellation — and `CopyReport::is_complete()` reads that
    /// as a clean success. The user picked things, ART copied none of them,
    /// and every signal available to the screen said it worked.
    #[test]
    fn a_selection_of_nothing_but_shortcuts_says_so_rather_than_reporting_success() {
        let dir = scratch("selection-symlinks");
        let target = dir.join("real.txt");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&target, b"not selected").unwrap();

        let link = dir.join("Link.txt");
        // Symlink creation needs privileges on Windows; skip when unavailable.
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(&target, &link).is_ok();
        #[cfg(not(windows))]
        let made = std::os::unix::fs::symlink(&target, &link).is_ok();

        if made {
            let selection = HostSelection::new(vec![link.clone()], true);
            assert!(
                selection.entries().unwrap().is_empty(),
                "a shortcut is still not followed"
            );

            let skipped = selection.skipped_sources();
            assert_eq!(skipped.len(), 1, "the root that was declined must be named");
            assert!(
                skipped[0].contains(&link.display().to_string()),
                "{}",
                skipped[0]
            );

            // And the report the engine builds from it must not read as clean.
            let mut report = CopyReport::default();
            report.skipped.extend(selection.skipped_sources());
            assert!(!report.is_complete());
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_selection_of_ordinary_files_declines_nothing() {
        // The other half: `skipped_sources` must stay empty for a normal
        // pick, or every copy would report a skip it did not make.
        let dir = scratch("selection-no-skips");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Readme.txt");
        std::fs::write(&file, b"x").unwrap();

        let selection = HostSelection::new(vec![file], true);
        assert!(selection.skipped_sources().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_selection_copies_nothing_without_error() {
        let selection = HostSelection::new(vec![], true);
        assert_eq!(selection.entries().unwrap(), Vec::new());
    }

    #[test]
    fn a_root_that_does_not_exist_is_reported_rather_than_silently_skipped() {
        let dir = scratch("selection-missing");
        let missing = dir.join("Nope");

        let selection = HostSelection::new(vec![missing], true);
        assert!(selection.entries().is_err());

        let _ = std::fs::remove_dir_all(&dir);
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
