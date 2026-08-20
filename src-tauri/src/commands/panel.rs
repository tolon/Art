//! The two-pane file manager's host-side half.
//!
//! One pane can show a local folder, an ADF image or an HDF image, so the
//! frontend needs a *uniform* listing regardless of which. That shape is
//! [`PanelEntry`]: name, whether it is a folder, a size, and the one identifier
//! needed to open or read it — a path for local files, a header block for ADF
//! entries.
//!
//! ADF listing, extraction and writing already have commands of their own
//! (`commands/adf.rs`); this module only adds what a file manager needs on top:
//! listing the host filesystem, and copying an ADF entry straight out to disk
//! without routing megabytes through the webview.
//!
//! ## HDF shows partitions, not files
//!
//! Deliberately. Writing — or even listing — inside an HDF partition means
//! implementing its filesystem, and PFS3/SFS are not implemented while FFS is
//! bound to the floppy layout. The pane reports what ART can actually read: the
//! partition table. Showing an empty file list instead would imply the disk is
//! empty (§89).

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::core::error::{CoreError, CoreResult};
use crate::error::AppResult;

/// The most entries returned for one folder.
///
/// A directory with a hundred thousand files is a real thing on a host disk,
/// and rendering all of it helps nobody.
const MAX_ENTRIES: usize = 5000;

/// One row in a pane, whatever the pane is showing.
#[derive(Debug, Clone, Serialize)]
pub struct PanelEntry {
    pub name: String,
    pub is_dir: bool,
    pub bytes: u64,
    /// Full path, for a local entry.
    pub path: Option<String>,
    /// Header block, for an ADF entry.
    pub header_block: Option<u32>,
    /// Starting logical block, for an ISO entry. Deliberately its own field
    /// rather than reusing `header_block`: a number that means two different
    /// things depending on the pane kind is how the wrong block gets read.
    /// Unlike `header_block`, this alone does not address a *directory* — a
    /// listing needs the entry's own `bytes` alongside it for that (an
    /// ISO9660 directory's length), which the field already carries for
    /// every other purpose.
    pub iso_extent: Option<u32>,
    /// True when the entry is a symlink or junction. Reported, never followed —
    /// that is the ART-028 lesson.
    pub is_link: bool,
    /// Last-modified time, Unix seconds. `None` when the source could not
    /// report one — a sort by date must know the difference between "no
    /// date" and "epoch", so this stays optional rather than defaulting to 0.
    pub date: Option<i64>,
    /// The Attr column: `rahs`-shape for a local file (Windows attributes),
    /// `hsparwed`-shape for an ADF/HDF entry (Amiga protection bits, already
    /// formatted by `core::volume::write::uaem::format_bits` — never a second
    /// formatter). `None` only when the source has nothing to report, which
    /// today is just a non-Windows build listing a local folder.
    pub attrs: Option<String>,
}

/// A local folder's contents plus where it sits.
#[derive(Debug, Clone, Serialize)]
pub struct LocalListing {
    pub path: String,
    /// The parent folder, or `None` at a drive root.
    pub parent: Option<String>,
    pub entries: Vec<PanelEntry>,
    /// True when the folder held more than ART will list.
    pub truncated: bool,
}

/// List a local folder for a pane.
#[tauri::command]
pub fn panel_list_local(path: String) -> AppResult<LocalListing> {
    let dir = PathBuf::from(path.trim());
    if !dir.is_dir() {
        return Err(CoreError::InvalidInput(format!("'{}' is not a folder", dir.display())).into());
    }

    Ok(list_local(&dir)?)
}

fn list_local(dir: &Path) -> CoreResult<LocalListing> {
    let mut entries = Vec::new();
    let mut truncated = false;

    for entry in std::fs::read_dir(dir)?.flatten() {
        if entries.len() >= MAX_ENTRIES {
            truncated = true;
            break;
        }
        // `symlink_metadata` does not follow the link, so a junction pointing
        // back up its own tree is listed rather than walked into.
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        let is_link = meta.file_type().is_symlink();
        let is_dir = if is_link {
            // A link's target may be a folder; asking costs one stat and makes
            // the icon right without following it for listing.
            std::fs::metadata(entry.path())
                .map(|m| m.is_dir())
                .unwrap_or(false)
        } else {
            meta.is_dir()
        };

        entries.push(PanelEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            is_dir,
            bytes: if is_dir { 0 } else { meta.len() },
            path: Some(entry.path().to_string_lossy().to_string()),
            header_block: None,
            iso_extent: None,
            is_link,
            date: mtime_unix(&meta),
            attrs: windows_attrs(&meta),
        });
    }

    // Folders first, then by name — the order every file manager uses, and the
    // one that makes navigating with the keyboard predictable.
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(LocalListing {
        path: dir.to_string_lossy().to_string(),
        parent: dir.parent().map(|p| p.to_string_lossy().to_string()),
        entries,
        truncated,
    })
}

/// A file's last-modified time as Unix seconds, or `None` when the platform
/// could not report one (some filesystems have no modified time at all).
fn mtime_unix(meta: &std::fs::Metadata) -> Option<i64> {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

// The four Windows file-attribute bits the Attr column shows, in the order
// Total Commander shows them: Read-only, Archive, Hidden, System. Values from
// winnt.h (`FILE_ATTRIBUTE_*`); hardcoded rather than pulled from a `windows`
// crate dependency, which `core/` (and this module stays close to) has no
// reason to add for four constants that never change.
const FILE_ATTRIBUTE_READONLY: u32 = 0x1;
const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
const FILE_ATTRIBUTE_ARCHIVE: u32 = 0x20;

/// Format a raw Windows attribute bitmask as the Attr column's `rahs` shape:
/// a letter when the bit is set, `-` when it is not, always four characters.
///
/// Pure and platform-independent on purpose — this is the part that gets
/// tested without a real Windows file — even though only [`windows_attrs`]
/// ever calls it with a real value.
fn format_windows_attrs(attrs: u32) -> String {
    let bit = |mask: u32, letter: char| if attrs & mask != 0 { letter } else { '-' };
    [
        bit(FILE_ATTRIBUTE_READONLY, 'r'),
        bit(FILE_ATTRIBUTE_ARCHIVE, 'a'),
        bit(FILE_ATTRIBUTE_HIDDEN, 'h'),
        bit(FILE_ATTRIBUTE_SYSTEM, 's'),
    ]
    .iter()
    .collect()
}

/// A local entry's Attr column, or `None` on a platform with no such thing.
///
/// This is the one platform-specific read in this module: `core/` must never
/// call a Windows API, so it lives here in `commands/`, behind `cfg(windows)`
/// with a `None`-returning fallback rather than failing to build elsewhere.
#[cfg(windows)]
fn windows_attrs(meta: &std::fs::Metadata) -> Option<String> {
    use std::os::windows::fs::MetadataExt;
    Some(format_windows_attrs(meta.file_attributes()))
}

#[cfg(not(windows))]
fn windows_attrs(_meta: &std::fs::Metadata) -> Option<String> {
    None
}

/// The places a pane can start from: drives, and the usual folders.
#[tauri::command]
pub fn panel_local_roots() -> AppResult<Vec<String>> {
    let mut roots = Vec::new();

    // Windows has no single filesystem root, so the drive letters are the
    // starting points. Probing is the only portable way to find them.
    #[cfg(windows)]
    for letter in b'A'..=b'Z' {
        let drive = format!("{}:\\", letter as char);
        if Path::new(&drive).is_dir() {
            roots.push(drive);
        }
    }

    #[cfg(not(windows))]
    roots.push("/".to_string());

    Ok(roots)
}

/// The event a finished directory-size job puts its answer on.
///
/// A job returns an id, not a result (§54), so the answer has to arrive
/// separately — the same shape `LAYOUT_EVENT` already uses. `jobId` ties it
/// back to the job the pane started, and `key` is the row it was started for,
/// so an answer that arrives after the user has moved on can be dropped rather
/// than written into whatever row is under the cursor now.
pub const DIR_SIZE_EVENT: &str = "dir-size-result";

// No `rename_all`: `job_id` travels snake_case, the same as `LayoutResult`
// and every other job payload, because `src/lib/jobs.ts::awaitJobResult` is
// bound on `TPayload extends { job_id: number }` and that helper is what
// closes the subscribe-after-invoke race (F4).
#[derive(Debug, Clone, serde::Serialize)]
pub struct DirSizeResult {
    pub job_id: u64,
    /// Whatever the caller asked under — a path for a local row, a header
    /// block written as a string for a volume row. Opaque here on purpose:
    /// this module has no business knowing how a pane keys its rows.
    pub key: String,
    pub total: crate::core::dirsize::DirTotal,
}

/// Count what a local folder holds (ART-087, brief §3.2 `CountSpace=1`).
///
/// A job, not a plain command: a drawer of forty thousand files must not block
/// the command thread and must be stoppable (§54, §55). Returns the job id;
/// the answer arrives on [`DIR_SIZE_EVENT`].
///
/// Read-only, so nothing is logged to the operation log — that records what
/// changes user data, and this changes nothing.
#[tauri::command]
pub fn panel_directory_size(
    path: String,
    app: tauri::AppHandle,
    registry: tauri::State<'_, std::sync::Arc<crate::commands::jobs::JobRegistry>>,
) -> AppResult<u64> {
    let dir = PathBuf::from(path.trim());
    if !dir.is_dir() {
        return Err(CoreError::InvalidInput(format!("'{}' is not a folder", dir.display())).into());
    }

    let key = dir.to_string_lossy().to_string();
    let registry = std::sync::Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!("Counting {key}");

    let id = super::jobs::spawn_job(&app, registry, &title, move |job_id, progress| {
        let total = crate::core::dirsize::host_total(&dir, progress)?;
        let _ = tauri::Emitter::emit(
            &emit_app,
            DIR_SIZE_EVENT,
            DirSizeResult { job_id, key, total },
        );
        Ok(())
    });

    Ok(id)
}

/// Send named entries of a **host folder** to the Recycle Bin (ART-080).
///
/// The first command in ART that removes a file from the user's own disk, and
/// every part of its shape is a consequence of that:
///
/// - **A directory plus names, never paths.** `core::hostfs::recycle_many`
///   resolves each through `safe_join`, so nothing the frontend sends — however
///   it was assembled — can name a file outside the folder the user is looking
///   at. A name that escapes, or one that is not there, refuses the **whole**
///   pass before a single file is touched.
/// - **Previewed before it happens.** The screen confirms, naming what goes and
///   where it goes; this command is the APPLY step and asks nothing.
/// - **Logged.** Through `commands/oplog.rs` like every other write, recording
///   the folder, the count, and how many actually went — because a partial
///   result is the one a log most needs to carry.
/// - **Per entry in its report, not all-or-nothing.** A host filesystem has no
///   journal (see `core::hostfs`'s own module doc): claiming a guarantee ART
///   cannot keep would be worse than the honest answer, which is every name and
///   what became of it.
///
/// A job (§54): a selection can be large, each entry is a shell round trip, and
/// the user must be able to stop. Cancelling is safe in the only sense
/// available here — it stops between whole entries, and what has already gone
/// is reported rather than thrown away.
#[tauri::command]
pub fn panel_delete_many(
    folder: String,
    names: Vec<String>,
    app: tauri::AppHandle,
    registry: tauri::State<'_, std::sync::Arc<crate::commands::jobs::JobRegistry>>,
    oplog: tauri::State<'_, crate::core::oplog::JsonlOperationLog>,
) -> AppResult<u64> {
    let dir = PathBuf::from(folder.trim());
    if !dir.is_dir() {
        return Err(CoreError::InvalidInput(format!("'{}' is not a folder", dir.display())).into());
    }
    // **Refused here, on the command thread, before a job even starts**
    // (review F5). `recycle_many` refuses it too — that is where the rule
    // lives — but a caller that reaches this command without the screen
    // deserves the answer immediately rather than as a failed job, and the
    // two together mean there is no arrangement of callers that gets past it.
    crate::core::hostfs::refuse_drive_root(&dir)?;
    if names.is_empty() {
        return Err(CoreError::InvalidInput("nothing was selected".to_string()).into());
    }

    // The log's *path*, not its `State`: a job runs on its own thread and
    // cannot carry one across (see `commands::oplog::write_to_path`).
    let log_path = oplog.path().to_path_buf();
    let registry = std::sync::Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!("Deleting {} item(s) from {}", names.len(), dir.display());
    let source = format!("{}:{}", dir.display(), names.join(", "));
    let asked = names.len();

    let id = super::jobs::spawn_job(&app, registry, &title, move |job_id, progress| {
        let result = crate::core::hostfs::recycle_many(
            &crate::tools::recycle_bin::RecycleBin,
            &dir,
            &names,
            progress,
        );

        // Logged whichever way it went, and the *partial* case is the one this
        // record exists for: "asked for 12, removed 11" is the sentence a user
        // comes back to the log for.
        let record = match &result {
            Ok(outcome) => super::oplog::user_operation("Delete from host folder")
                .source(source.clone())
                .detail("Asked", asked.to_string())
                .detail("Removed", outcome.removed().to_string())
                .detail("Failed", outcome.failed().to_string())
                // Never attempted, because the user stopped it. Distinct from
                // "failed", which is a name ART tried and could not remove
                // (review F1) — a log that folded the two together would say
                // a cancelled pass had failures it never had.
                .detail("Untouched", outcome.untouched().to_string())
                .detail("Cancelled", outcome.cancelled.to_string())
                // Asked of the outcome rather than written as a literal
                // (review F10): a second recycler would otherwise have the
                // first one's destination logged against it. Absent when
                // nothing was removed, which is the one case with nowhere to
                // name.
                .detail(
                    "Destination",
                    outcome
                        .target
                        .map(|target| target.log_label().to_string())
                        .unwrap_or_else(|| "-".to_string()),
                )
                // `verified(true)` only for a pass that removed **every name
                // it was asked for** (review F1). It used to be
                // `failed() == 0`, which is true of a twelve-name request
                // cancelled after three — the log then recorded an
                // unqualified success for a delete that mostly did not
                // happen.
                .outcome(crate::core::oplog::OperationOutcome::verified(
                    outcome.complete(),
                )),
            Err(err) => super::oplog::user_operation("Delete from host folder")
                .source(source.clone())
                .detail("Asked", asked.to_string())
                .failure(err.code(), err.to_string()),
        };
        super::oplog::write_to_path(&log_path, &record);

        let outcome = result?;
        let _ = tauri::Emitter::emit(
            &emit_app,
            HOST_DELETE_EVENT,
            HostDeleteResult { job_id, outcome },
        );
        Ok(())
    });

    Ok(id)
}

/// The event a finished host delete arrives on.
pub const HOST_DELETE_EVENT: &str = "panel-host-delete-result";

// `job_id`, not `jobId` — the spelling every other job result in this codebase
// uses, and the one `src/lib/panel.ts` declares.
#[derive(Debug, Clone, Serialize)]
pub struct HostDeleteResult {
    pub job_id: u64,
    #[serde(flatten)]
    pub outcome: crate::core::hostfs::HostDeleteOutcome,
}

/// Where an ADF entry was written on the host.
#[derive(Debug, Clone, Serialize)]
pub struct ExtractedTo {
    pub path: String,
    pub bytes: u64,
    /// True when a file was already there and was left alone.
    pub skipped_existing: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_attributes_set_is_four_dashes() {
        assert_eq!(format_windows_attrs(0), "----");
    }

    #[test]
    fn archive_only() {
        assert_eq!(format_windows_attrs(FILE_ATTRIBUTE_ARCHIVE), "-a--");
    }

    #[test]
    fn hidden_and_system() {
        assert_eq!(
            format_windows_attrs(FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM),
            "--hs"
        );
    }

    #[test]
    fn read_only() {
        assert_eq!(format_windows_attrs(FILE_ATTRIBUTE_READONLY), "r---");
    }

    #[test]
    fn all_four_at_once() {
        let all = FILE_ATTRIBUTE_READONLY
            | FILE_ATTRIBUTE_ARCHIVE
            | FILE_ATTRIBUTE_HIDDEN
            | FILE_ATTRIBUTE_SYSTEM;
        assert_eq!(format_windows_attrs(all), "rahs");
    }

    /// Bits this module does not know about (e.g. `FILE_ATTRIBUTE_NORMAL`,
    /// `FILE_ATTRIBUTE_DIRECTORY`) must not corrupt the four it does.
    #[test]
    fn unrelated_bits_are_ignored() {
        let known = FILE_ATTRIBUTE_READONLY
            | FILE_ATTRIBUTE_ARCHIVE
            | FILE_ATTRIBUTE_HIDDEN
            | FILE_ATTRIBUTE_SYSTEM;
        assert_eq!(format_windows_attrs(!known), "----");
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-panel-{name}-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn folders_come_first_then_names_in_order() {
        let dir = scratch("order");
        std::fs::write(dir.join("zebra.txt"), b"z").unwrap();
        std::fs::write(dir.join("Apple.txt"), b"a").unwrap();
        std::fs::create_dir(dir.join("Tools")).unwrap();
        std::fs::create_dir(dir.join("assets")).unwrap();

        let listing = list_local(&dir).unwrap();
        let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();

        assert_eq!(names, vec!["assets", "Tools", "Apple.txt", "zebra.txt"]);
        assert!(listing.entries[0].is_dir);
        assert!(!listing.entries.last().unwrap().is_dir);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_listing_knows_its_parent() {
        let dir = scratch("parent");
        let child = dir.join("inside");
        std::fs::create_dir(&child).unwrap();

        let listing = list_local(&child).unwrap();
        assert_eq!(listing.parent.as_deref(), Some(dir.to_str().unwrap()));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A sort by date has to know a real date from a missing one — see the
    /// `Option<i64>` comment on `PanelEntry::date` — so this proves the local
    /// source actually reports one rather than always coming back `None`.
    #[test]
    fn a_freshly_written_file_has_a_recent_date() {
        let dir = scratch("date");
        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        std::fs::write(dir.join("new.txt"), b"x").unwrap();

        let listing = list_local(&dir).unwrap();
        let file = listing
            .entries
            .iter()
            .find(|e| e.name == "new.txt")
            .unwrap();
        let date = file
            .date
            .expect("a freshly written file has a modified time");

        assert!(
            date >= before - 2,
            "date {date} looks stale next to {before}"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn file_sizes_are_reported_and_folders_are_zero() {
        let dir = scratch("sizes");
        std::fs::write(dir.join("data.bin"), vec![0u8; 1234]).unwrap();
        std::fs::create_dir(dir.join("sub")).unwrap();

        let listing = list_local(&dir).unwrap();
        let file = listing
            .entries
            .iter()
            .find(|e| e.name == "data.bin")
            .unwrap();
        let folder = listing.entries.iter().find(|e| e.name == "sub").unwrap();

        assert_eq!(file.bytes, 1234);
        assert_eq!(folder.bytes, 0);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A folder with more files than ART will list must say so, rather than
    /// silently showing a prefix as if it were everything.
    #[test]
    fn an_enormous_folder_is_truncated_and_says_so() {
        let dir = scratch("many");
        for i in 0..(MAX_ENTRIES + 10) {
            std::fs::write(dir.join(format!("f{i}")), b"x").unwrap();
        }

        let listing = list_local(&dir).unwrap();
        assert!(listing.truncated);
        assert_eq!(listing.entries.len(), MAX_ENTRIES);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn listing_something_that_is_not_a_folder_is_refused() {
        let dir = scratch("notdir");
        let file = dir.join("a.txt");
        std::fs::write(&file, b"x").unwrap();

        assert!(panel_list_local(file.to_string_lossy().to_string()).is_err());
        assert!(panel_list_local("".into()).is_err());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn there_is_always_somewhere_to_start() {
        let roots = panel_local_roots().unwrap();
        assert!(!roots.is_empty(), "a pane needs a starting point");
    }
}
