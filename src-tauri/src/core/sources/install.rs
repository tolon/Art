//! Installing a downloaded package into a floppy image (§41.5.6).
//!
//! This is the last step of "find it, fetch it, use it", and it is deliberately
//! the *thinnest* part of the module. Everything dangerous already exists and
//! is already tested:
//!
//! - unpacking is `core/lha::extract_archive_with` — traversal defence, bomb
//!   caps and overwrite policy included;
//! - writing is `core/adf::mutate_disk_file`, which reads, mutates in memory,
//!   **validates the result before anything reaches the disk**, takes a
//!   generational backup and commits atomically (§57).
//!
//! So this file only decides *what* to write and *where*, and refuses early
//! when it will not fit.
//!
//! ## ADF only, on purpose
//!
//! §41.5.10's Stage B wants "one-click install to HDF" as well. ART cannot do
//! that yet — writing into an HDF means writing into a partition's filesystem,
//! and PFS3/SFS are not implemented while FFS is bound to the floppy layout.
//! The workflow catalogue already carries `install_hdf` as "Coming Later"
//! (§96); pretending otherwise here would be the exact claim §10 and §89
//! forbid.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::core::adf::blocks::{EntryKind, BLOCK_SIZE};
use crate::core::adf::bootblock::DEFAULT_ROOT_BLOCK;
use crate::core::adf::mutate::{add_file, create_directory, free_block_count};
use crate::core::adf::mutate_disk_file;
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::lha::safe_extract::extract_archive_with;
use crate::core::lha::OverwritePolicy;

/// How deep the extracted tree is walked.
const MAX_TREE_DEPTH: usize = 12;

/// The most files installed in one go.
const MAX_FILES: usize = 4096;

/// What an install would do, worked out before anything is written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstallPlan {
    pub files: usize,
    pub directories: usize,
    pub bytes: u64,
    /// Blocks the image has left.
    pub free_blocks: usize,
    /// Blocks the install needs, header blocks included.
    pub needed_blocks: usize,
    pub fits: bool,
}

/// What an install actually did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstallOutcome {
    pub files: usize,
    pub directories: usize,
    pub bytes: u64,
    /// Where inside the image it went; empty for the root.
    pub into: String,
    /// Where the previous version of the image was kept.
    pub backup: Option<String>,
    /// Entries the archive held that ART did not write, with the reason.
    pub skipped: Vec<String>,
}

/// One file to be written into the image.
struct Planned {
    /// Path on disk, in the extracted tree.
    source: PathBuf,
    /// Directory components inside the image, relative to `into`.
    directories: Vec<String>,
    name: String,
    bytes: u64,
}

/// Unpack `archive` and write its contents into `adf`.
///
/// `into` is a directory inside the image; empty means the root. The image is
/// only touched once every file has been read and the whole result validated.
pub fn install_archive_into_adf(
    archive: &Path,
    adf: &Path,
    into: &str,
    sink: &dyn ProgressSink,
) -> CoreResult<InstallOutcome> {
    if sink.is_cancelled() {
        return Err(CoreError::Cancelled);
    }

    let scratch = Scratch::new()?;
    sink.report(0, None, "Unpacking");

    // The existing extractor owns traversal and bomb defence. Overwriting
    // inside a fresh scratch directory is safe by construction — nothing of the
    // user's is in there.
    let extracted =
        extract_archive_with(archive, scratch.path(), OverwritePolicy::Overwrite, sink)?;
    if extracted.aborted {
        return Err(CoreError::SafetyRefused(
            extracted
                .abort_reason
                .unwrap_or_else(|| "the archive was refused".into()),
        ));
    }

    let mut planned = Vec::new();
    let mut skipped: Vec<String> = extracted
        .extracted
        .iter()
        .filter(|entry| entry.skipped && !entry.is_dir)
        .map(|entry| {
            format!(
                "{} ({})",
                entry.source_path,
                entry.reason.clone().unwrap_or_else(|| "skipped".into())
            )
        })
        .collect();

    collect_files(
        scratch.path(),
        scratch.path(),
        0,
        &mut planned,
        &mut skipped,
    )?;

    if planned.is_empty() {
        return Err(CoreError::InvalidInput(
            "the archive holds no files to install".into(),
        ));
    }

    let plan = plan_from(&planned, adf)?;
    if !plan.fits {
        return Err(CoreError::SafetyRefused(format!(
            "this needs about {} KB but the disk has {} KB free; nothing was changed",
            plan.needed_blocks * BLOCK_SIZE / 1024,
            plan.free_blocks * BLOCK_SIZE / 1024
        )));
    }

    if sink.is_cancelled() {
        return Err(CoreError::Cancelled);
    }

    let total = planned.len() as u64;
    let into = into.trim().trim_matches('/').to_string();
    let mut written_files = 0usize;
    let mut written_dirs = 0usize;
    let mut written_bytes = 0u64;

    // One closure, one backup, one validation, one commit — even for a
    // hundred-file package. A per-file transaction would leave a half-installed
    // image behind the moment one of them failed.
    let outcome = mutate_disk_file(adf, |img, fs_type| {
        // The root block explicitly, not 0. `add_file` translates 0 for
        // itself, but `list_directory` does not — and this walk has to be able
        // to look inside a folder it has just created.
        let base = if into.is_empty() {
            DEFAULT_ROOT_BLOCK
        } else {
            ensure_directory_path(
                img,
                DEFAULT_ROOT_BLOCK,
                &split_path(&into),
                &mut written_dirs,
            )?
        };

        for (index, file) in planned.iter().enumerate() {
            if sink.is_cancelled() {
                return Err(CoreError::Cancelled);
            }
            sink.report(index as u64, Some(total), &file.name);

            let parent = ensure_directory_path(img, base, &file.directories, &mut written_dirs)?;
            let data = std::fs::read(&file.source)?;
            add_file(img, parent, &file.name, &data, fs_type)?;

            written_files += 1;
            written_bytes += data.len() as u64;
        }
        Ok(())
    })?;

    Ok(InstallOutcome {
        files: written_files,
        directories: written_dirs,
        bytes: written_bytes,
        into,
        backup: outcome.backup_path,
        skipped,
    })
}

/// Unpack an archive into a scratch directory, ready to be copied into a
/// volume.
///
/// Split out of [`install_archive_into_adf`] so installing into a hard disk
/// partition reuses the same unpack — traversal defence, decompression-bomb
/// limits and all — rather than growing a second one (§41.5.3).
///
/// The caller owns the scratch directory and it removes itself when dropped.
pub fn unpack_for_install(
    archive: &Path,
    sink: &dyn ProgressSink,
) -> CoreResult<(Scratch, Vec<String>)> {
    if sink.is_cancelled() {
        return Err(CoreError::Cancelled);
    }

    let scratch = Scratch::new()?;
    sink.report(0, None, "Unpacking");

    let extracted =
        extract_archive_with(archive, scratch.path(), OverwritePolicy::Overwrite, sink)?;
    if extracted.aborted {
        return Err(CoreError::SafetyRefused(
            extracted
                .abort_reason
                .unwrap_or_else(|| "the archive was refused".into()),
        ));
    }

    let skipped = extracted
        .extracted
        .iter()
        .filter(|entry| entry.skipped && !entry.is_dir)
        .map(|entry| {
            format!(
                "{} ({})",
                entry.source_path,
                entry.reason.clone().unwrap_or_else(|| "skipped".into())
            )
        })
        .collect();

    Ok((scratch, skipped))
}

/// Work out whether an archive would fit, without writing anything.
pub fn plan_install(archive: &Path, adf: &Path) -> CoreResult<InstallPlan> {
    let scratch = Scratch::new()?;
    let extracted = extract_archive_with(
        archive,
        scratch.path(),
        OverwritePolicy::Overwrite,
        &crate::core::jobs::NoProgress,
    )?;
    if extracted.aborted {
        return Err(CoreError::SafetyRefused(
            extracted
                .abort_reason
                .unwrap_or_else(|| "the archive was refused".into()),
        ));
    }

    let mut planned = Vec::new();
    let mut skipped = Vec::new();
    collect_files(
        scratch.path(),
        scratch.path(),
        0,
        &mut planned,
        &mut skipped,
    )?;
    plan_from(&planned, adf)
}

fn plan_from(planned: &[Planned], adf: &Path) -> CoreResult<InstallPlan> {
    let bytes: u64 = planned.iter().map(|f| f.bytes).sum();

    let mut directories: Vec<&[String]> =
        planned.iter().map(|f| f.directories.as_slice()).collect();
    directories.sort_unstable();
    directories.dedup();
    let directory_count = directories.iter().filter(|d| !d.is_empty()).count();

    // Each file costs its data blocks plus a header block; each directory costs
    // one block. Rounded up per file, because a block is the unit of
    // allocation — summing bytes first would under-count every partial block.
    let data_blocks: usize = planned
        .iter()
        .map(|f| (f.bytes as usize).div_ceil(BLOCK_SIZE).max(1) + 1)
        .sum();
    let needed_blocks = data_blocks + directory_count;

    // Only the bitmap is read, but the file has to be opened to reach it. An
    // ADF is under a megabyte, so this is not the "never read a whole file"
    // case that HDFs are.
    let image = std::fs::read(adf)?;
    let free_blocks = free_block_count(&image)?;

    Ok(InstallPlan {
        files: planned.len(),
        directories: directory_count,
        bytes,
        free_blocks,
        needed_blocks,
        fits: needed_blocks <= free_blocks,
    })
}

/// Create every directory in `path`, returning the block of the last one.
fn ensure_directory_path(
    img: &mut [u8],
    base: u32,
    path: &[String],
    created: &mut usize,
) -> CoreResult<u32> {
    let mut parent = base;
    for name in path {
        parent = match create_directory(img, parent, name) {
            Ok(block) => {
                *created += 1;
                block
            }
            // Already there is the normal case for the second file in a folder.
            Err(_) => find_directory(img, parent, name)?,
        };
    }
    Ok(parent)
}

/// The block of an existing subdirectory.
fn find_directory(img: &[u8], parent: u32, name: &str) -> CoreResult<u32> {
    let entries = crate::core::adf::fs::list_directory(img, parent)?;
    entries
        .iter()
        .find(|entry| entry.kind == EntryKind::Directory && entry.name.eq_ignore_ascii_case(name))
        .map(|entry| entry.header_block)
        .ok_or_else(|| {
            CoreError::InvalidInput(format!("could not create or find the folder '{name}'"))
        })
}

fn split_path(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|part| !part.is_empty())
        .map(|part| part.to_string())
        .collect()
}

/// Walk the extracted tree, depth- and count-limited.
fn collect_files(
    root: &Path,
    dir: &Path,
    depth: usize,
    out: &mut Vec<Planned>,
    skipped: &mut Vec<String>,
) -> CoreResult<()> {
    if depth >= MAX_TREE_DEPTH {
        skipped.push(format!("{} (nested too deeply)", dir.display()));
        return Ok(());
    }

    let mut entries: Vec<_> = std::fs::read_dir(dir)?.flatten().collect();
    // Sorted so an install writes the same order every time, which makes a
    // failure reproducible.
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        if out.len() >= MAX_FILES {
            skipped.push("the rest of the archive (too many files)".into());
            return Ok(());
        }
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };

        if meta.is_dir() {
            collect_files(root, &path, depth + 1, out, skipped)?;
            continue;
        }
        if !meta.is_file() {
            // A symlink in an extracted archive is not something to follow.
            skipped.push(format!("{} (not a regular file)", path.display()));
            continue;
        }

        let relative = path.strip_prefix(root).unwrap_or(&path);
        let mut components: Vec<String> = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();
        let Some(name) = components.pop() else {
            continue;
        };

        out.push(Planned {
            source: path.clone(),
            directories: components,
            name,
            bytes: meta.len(),
        });
    }

    Ok(())
}

/// A temporary directory that removes itself.
/// A scratch directory that removes itself.
///
/// `pub(crate)` so the volume install path can unpack into one and hand it to
/// the Stage W copy engine: an unpacked archive is a folder, and copying a
/// folder into a volume is already a tested operation.
pub(crate) struct Scratch(PathBuf);

impl Scratch {
    pub(crate) fn new() -> CoreResult<Self> {
        // The process id plus a counter is enough: two installs in the same
        // process must not share a directory, and two processes cannot.
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);

        let path = std::env::temp_dir().join(format!(
            "art-install-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::adf::create::create_blank_adf;
    use crate::core::adf::fs::list_directory;
    use crate::core::jobs::NoProgress;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("art-install-t-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn blank_adf(path: &Path) {
        let image = create_blank_adf("Test", crate::core::adf::FileSystemType::Ffs, false).unwrap();
        std::fs::write(path, image).unwrap();
    }

    fn archive_with(files: &[(&str, &[u8])]) -> Vec<u8> {
        crate::core::lha::tests::make_lha_with(files)
    }

    #[test]
    fn a_package_lands_in_the_image() {
        let dir = scratch("basic");
        let adf = dir.join("disk.adf");
        blank_adf(&adf);
        let archive = dir.join("pkg.lha");
        std::fs::write(&archive, archive_with(&[("hello.txt", b"hi")])).unwrap();

        let outcome = install_archive_into_adf(&archive, &adf, "", &NoProgress).unwrap();

        assert_eq!(outcome.files, 1);
        assert_eq!(outcome.bytes, 2);
        assert!(outcome.backup.is_some(), "§57: the previous image is kept");

        let image = std::fs::read(&adf).unwrap();
        let entries = list_directory(&image, 880).unwrap();
        assert!(entries.iter().any(|e| e.name == "hello.txt"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_package_can_be_installed_into_a_folder() {
        let dir = scratch("into");
        let adf = dir.join("disk.adf");
        blank_adf(&adf);
        let archive = dir.join("pkg.lha");
        std::fs::write(&archive, archive_with(&[("hello.txt", b"hi")])).unwrap();

        let outcome = install_archive_into_adf(&archive, &adf, "Tools", &NoProgress).unwrap();
        assert_eq!(outcome.into, "Tools");

        let image = std::fs::read(&adf).unwrap();
        let root = list_directory(&image, 880).unwrap();
        let tools = root
            .iter()
            .find(|e| e.name == "Tools" && e.kind == EntryKind::Directory)
            .expect("the folder was created");
        let inside = list_directory(&image, tools.header_block).unwrap();
        assert!(inside.iter().any(|e| e.name == "hello.txt"));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// §57: an install that cannot fit must change nothing, and must say so
    /// before it starts rather than failing part-way.
    #[test]
    fn a_package_too_big_for_the_disk_is_refused_without_touching_it() {
        let dir = scratch("toobig");
        let adf = dir.join("disk.adf");
        blank_adf(&adf);
        let before = std::fs::read(&adf).unwrap();

        let big = vec![b'x'; 900 * 1024];
        let archive = dir.join("pkg.lha");
        std::fs::write(&archive, archive_with(&[("big.bin", &big)])).unwrap();

        let err = install_archive_into_adf(&archive, &adf, "", &NoProgress).unwrap_err();

        assert_eq!(err.code(), "ART-SAFETY-REFUSED");
        assert!(err.to_string().contains("free"), "{err}");
        assert_eq!(
            std::fs::read(&adf).unwrap(),
            before,
            "the image must be byte-for-byte unchanged"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_plan_reports_the_fit_without_writing() {
        let dir = scratch("plan");
        let adf = dir.join("disk.adf");
        blank_adf(&adf);
        let before = std::fs::read(&adf).unwrap();
        let archive = dir.join("pkg.lha");
        std::fs::write(&archive, archive_with(&[("hello.txt", b"hi")])).unwrap();

        let plan = plan_install(&archive, &adf).unwrap();

        assert_eq!(plan.files, 1);
        assert!(plan.fits);
        assert!(plan.free_blocks > 0);
        assert_eq!(std::fs::read(&adf).unwrap(), before);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// An archive with nothing in it is rejected by the LHA reader before the
    /// installer ever sees it, which is the right place for it to fail.
    #[test]
    fn an_archive_with_no_files_is_an_honest_error() {
        let dir = scratch("empty");
        let adf = dir.join("disk.adf");
        blank_adf(&adf);
        let archive = dir.join("pkg.lha");
        std::fs::write(&archive, archive_with(&[])).unwrap();

        let err = install_archive_into_adf(&archive, &adf, "", &NoProgress).unwrap_err();
        assert_eq!(err.code(), "ART-FORMAT-MALFORMED");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn nested_folders_in_the_archive_are_recreated() {
        let dir = scratch("nested");
        let adf = dir.join("disk.adf");
        blank_adf(&adf);
        let archive = dir.join("pkg.lha");
        std::fs::write(
            &archive,
            archive_with(&[("Docs/readme.txt", b"a"), ("Docs/notes.txt", b"b")]),
        )
        .unwrap();

        let outcome = install_archive_into_adf(&archive, &adf, "", &NoProgress).unwrap();
        assert_eq!(outcome.files, 2);
        assert_eq!(
            outcome.directories, 1,
            "the folder is created once, not twice"
        );

        let image = std::fs::read(&adf).unwrap();
        let root = list_directory(&image, 880).unwrap();
        let docs = root
            .iter()
            .find(|e| e.name == "Docs" && e.kind == EntryKind::Directory)
            .unwrap();
        assert_eq!(list_directory(&image, docs.header_block).unwrap().len(), 2);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cancelling_leaves_the_image_alone() {
        struct Cancelled;
        impl ProgressSink for Cancelled {
            fn report(&self, _: u64, _: Option<u64>, _: &str) {}
            fn is_cancelled(&self) -> bool {
                true
            }
        }

        let dir = scratch("cancel");
        let adf = dir.join("disk.adf");
        blank_adf(&adf);
        let before = std::fs::read(&adf).unwrap();
        let archive = dir.join("pkg.lha");
        std::fs::write(&archive, archive_with(&[("hello.txt", b"hi")])).unwrap();

        let err = install_archive_into_adf(&archive, &adf, "", &Cancelled).unwrap_err();

        assert_eq!(err.code(), "ART-CANCELLED");
        assert_eq!(std::fs::read(&adf).unwrap(), before);

        std::fs::remove_dir_all(&dir).ok();
    }
}
