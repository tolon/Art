//! Game index Tauri commands (SD-2 · G10).
//!
//! A thin adapter, as CLAUDE.md requires: deserialize, call core, serialize
//! back. The scanning, the four readers and every decision about which source
//! wins live in `core/gameindex`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use super::jobs::{spawn_job, JobRegistry};
use super::oplog::{user_operation, write_result};
use crate::core::error::CoreError;
use crate::core::gameindex::cleanup;
use crate::core::gameindex::scan::{scan_titles_with, CatalogueEntry};
use crate::core::gameindex::store;
use crate::core::jobs::JobId;
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::error::{AppError, AppResult};

/// Which Kickstart images a title asks for, and which of them ART found in a
/// ROM folder (ART-130).
///
/// **Read-only, and it places nothing.** The owner's decision, 2026-08-21: ART
/// closes this loop *"always as a proposal"* — *"this title asks for
/// `kick34005.A500`; ART recognises it in your collection — place it?"*, never
/// a silent copy. Putting somebody's ROM onto their card touches ROM Manager,
/// the licensed Amiga Forever decode path and the card's layout, and those are
/// their decisions rather than a side effect of a scan.
///
/// **The mapping from `KickstartNeed` to `WantedImage` is here on purpose.**
/// `core::rom::offer` declares its own record and does not import
/// `core::gameindex` — the layering rule CLAUDE.md states with `core/rom` as
/// its own example. Translating one module's representation into another's is a
/// command-layer job, the way `commands/preload.rs::rom_pairing_for` already
/// does it for `core/rom/pairing`.
///
/// An empty `wanted` list is an empty answer, not an error: a great many titles
/// declare no Kickstart at all, and that is not a problem with the title.
#[tauri::command]
pub fn kickstart_offers_for(
    need: crate::core::gameindex::record::KickstartNeed,
    rom_folder: String,
) -> AppResult<Vec<crate::core::rom::offer::Offer>> {
    let collection = crate::core::rom::scan_rom_directory(Path::new(&rom_folder))?;
    Ok(crate::core::rom::offer::offer_for(
        &wanted_images(&need),
        &collection,
    ))
}

/// Put one Kickstart where WHDLoad will look for it (ART-130).
///
/// **One agreed placement, never a title's worth.** The owner's decision was
/// that ART offers and the user chooses; a command that took a title and did
/// the right thing would be the silent copy that decision rules out. So this
/// takes the single image the user pressed a button about.
///
/// Logged like every other write (§53), including its refusals — an occupied
/// name and a name a slave should not have declared are both things somebody
/// may want to find again afterwards.
#[tauri::command]
pub fn place_kickstart(
    from: String,
    as_name: String,
    tree: String,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<crate::core::rom::place::PlaceOutcome> {
    use crate::core::rom::place::{place, PlaceOutcome, Placement};

    let placement = Placement {
        from: PathBuf::from(&from),
        as_name: as_name.clone(),
        tree: PathBuf::from(&tree),
    };
    let result = place(&placement).map_err(AppError::from);

    write_result(
        &oplog,
        user_operation("Place a Kickstart a title asks for")
            .source(&from)
            .destination(&tree)
            .detail("Name the title asks for", &as_name),
        &result,
        |record, done: &PlaceOutcome| {
            // **The three endings stay apart in the log too.** A refusal
            // recorded as a plain success is the operation log agreeing with a
            // screen that out-claims the core, which is the shape this project
            // names as its own worst.
            let record = match done {
                PlaceOutcome::Placed { to, bytes } => record
                    .detail("Written to", to.clone())
                    .detail("Bytes", bytes.to_string()),
                PlaceOutcome::AlreadyThere { to } => {
                    record.detail("Already there, unchanged", to.clone())
                }
                PlaceOutcome::Occupied { to } => {
                    record.detail("Refused: a different ROM is already there", to.clone())
                }
            };
            record.outcome(match done {
                PlaceOutcome::Placed { .. } => OperationOutcome::verified(true),
                // Nothing was written. `verified(false)` is not a failure and
                // not a claim that anything was checked.
                PlaceOutcome::AlreadyThere { .. } | PlaceOutcome::Occupied { .. } => {
                    OperationOutcome::verified(false)
                }
            })
        },
    );

    result
}

/// A slave's declared need, as the images it will accept.
///
/// **The list wins when there is one.** `KickstartNeed::image` is the first of
/// `alternatives` when that is non-empty, so reading both would ask for the
/// same image twice — and `crc16` is `None` in exactly that case, because the
/// `$ffff` sentinel is how a slave says "the name field is a list" rather than
/// a checksum ([ART-137](../../../docs/ISSUES.md)).
fn wanted_images(
    need: &crate::core::gameindex::record::KickstartNeed,
) -> Vec<crate::core::rom::offer::WantedImage> {
    use crate::core::rom::offer::WantedImage;

    if !need.alternatives.is_empty() {
        return need
            .alternatives
            .iter()
            .map(|alt| WantedImage {
                name: alt.image.clone(),
                crc16: Some(alt.crc16),
                size: need.size,
            })
            .collect();
    }
    match &need.image {
        Some(image) => vec![WantedImage {
            name: image.clone(),
            crc16: need.crc16,
            size: need.size,
        }],
        None => Vec::new(),
    }
}

/// Where the catalogue lives.
///
/// **Resolved here and nowhere else.** `core/gameindex/store` takes the
/// directory as an argument because `core/` is platform-independent and
/// `%APPDATA%` is not. The temp-directory fallback is the same one `lib.rs`
/// uses for the software catalog, and for the same reason: a catalogue ART
/// cannot place is recoverable with one rescan, refusing to start is not.
///
/// `pub(crate)` rather than a second copy: `commands/artwork.rs` needs this
/// same path to reach the overrides layer when attaching or detaching a
/// picture, and a second implementation of the same lookup is how the two
/// would drift.
pub(crate) fn catalogue_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("catalogue")
}

/// A timestamp for `scanned_at`.
///
/// `core` has no clock, so the command layer supplies one — the same split
/// `CardManifest::built_at` uses. Seconds since the epoch rather than a
/// formatted date: ART has no date library and the screen can format it.
fn now_stamp() -> Option<String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some(secs.to_string())
}

/// Emitted when an index job finishes with its catalogue.
pub const INDEX_RESULT_EVENT: &str = "gameindex-result";

/// Emitted when a root has been refreshed. The screen reloads on it.
pub const REFRESHED_EVENT: &str = "catalogue-refreshed";

/// Which root a finished refresh was for.
#[derive(Debug, Clone, Serialize)]
pub struct RefreshedRoot {
    pub job_id: JobId,
    pub root: String,
}

/// The saved catalogue, with the user's own corrections applied.
///
/// **Starts nothing.** Opening the Collection screen calls this and no more;
/// scanning is an explicit ask.
#[tauri::command]
pub fn catalogue_load(app: AppHandle) -> AppResult<Vec<store::RootView>> {
    Ok(store::load(&catalogue_dir(&app))?)
}

#[tauri::command]
pub fn catalogue_add_root(root: String, app: AppHandle) -> AppResult<()> {
    Ok(store::add_root(&catalogue_dir(&app), Path::new(&root))?)
}

#[tauri::command]
pub fn catalogue_remove_root(root: String, app: AppHandle) -> AppResult<()> {
    Ok(store::remove_root(&catalogue_dir(&app), Path::new(&root))?)
}

/// Refresh one root on a job.
///
/// `mode` is `"update"` or `"rescan"`; anything else is refused rather than
/// guessed at — a third word arriving from the screen would otherwise silently
/// become whichever branch the `match` fell through to.
#[tauri::command]
pub fn catalogue_refresh(
    root: String,
    mode: String,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
) -> AppResult<JobId> {
    let refresh = match mode.as_str() {
        "update" => store::Refresh::Update,
        "rescan" => store::Refresh::Rescan,
        other => {
            return Err(crate::core::error::CoreError::InvalidInput(format!(
                "'{other}' is not a refresh mode"
            ))
            .into())
        }
    };

    let dir = catalogue_dir(&app);
    let root_path = PathBuf::from(&root);
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let stamped = now_stamp();

    let id = spawn_job(
        &app,
        registry,
        "Refreshing the catalogue",
        move |job_id, progress| {
            store::refresh_root(&dir, &root_path, refresh, stamped, progress)?;
            // The screen reloads the whole catalogue rather than patching one
            // root: the user layer and availability both apply across roots, and
            // one reload is cheaper than keeping two views in step.
            let _ = emit_app.emit(REFRESHED_EVENT, RefreshedRoot { job_id, root });
            Ok(())
        },
    );

    Ok(id)
}

/// Record — or clear — one title's hand corrections.
///
/// Returns where the previous overrides were backed up, which the screen
/// surfaces the way every mutating command in ART does.
#[tauri::command]
pub fn catalogue_set_override(
    id: String,
    edit: store::RecordOverride,
    app: AppHandle,
) -> AppResult<Option<String>> {
    let backup = store::set_override(&catalogue_dir(&app), &id, edit)?;
    Ok(backup.map(|path| path.to_string_lossy().to_string()))
}

/// What ART would propose for one entry's name, if anything.
///
/// Both fields are `None` far more often than not, and that is the point: the
/// screen shows a button only where there is something to propose. A tool that
/// offers to fix a name already right teaches people to click without reading.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameSuggestion {
    /// A cleaner catalogue title. Applying it is a user override.
    pub title: Option<String>,
    /// A cleaner filename, extension included. Applying it renames a real file.
    pub file_name: Option<String>,
}

/// What ART would propose for these paths, one answer each, in order.
///
/// Read-only and free: nothing is applied, nothing is written, and the screen
/// can ask about a whole library without a confirmation in sight.
/// **Every title is asked about at once, and that is the point.** `dune2-2` is
/// Dune II's second disk, but nothing in that name says so — `dune2` itself
/// ends in a digit. What settles it is `dune2-1` lying beside it, so the whole
/// list decides and no single name is guessed at.
#[tauri::command]
pub fn name_suggestions(paths: Vec<String>, titles: Vec<String>) -> Vec<NameSuggestion> {
    let sets = cleanup::disk_sets(&titles);

    paths
        .iter()
        .zip(titles.iter())
        .map(|(path, title)| {
            let path = Path::new(path);
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let extension = path
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default();

            NameSuggestion {
                title: cleanup::suggest_in_set(title, &sets),
                file_name: cleanup::suggest_stem(&stem).map(|s| format!("{s}{extension}")),
            }
        })
        .collect()
}

/// Rename a title's file on disk.
///
/// This is the half of the tool that changes the user's data, so it behaves
/// like every other mutating command in ART:
///
/// - the new name is a **file name**, never a path — a caller cannot move a
///   file into another folder, or out of one, by renaming it;
/// - an existing target is **refused rather than replaced** (`SAFE_CREATE`);
/// - the operation is logged, success or failure.
///
/// The catalogue is not rewritten here. An entry's id is derived from its
/// content, so the next refresh recognises the file at its new path as the same
/// title rather than losing one and finding another — which is the behaviour a
/// rename on screen already proved.
#[tauri::command]
pub fn rename_title_file(
    path: String,
    new_name: String,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<String> {
    let result = rename_on_disk(&path, &new_name).map_err(AppError::from);

    write_result(
        &oplog,
        user_operation("Rename a title's file")
            .source(&path)
            .detail("New name", &new_name),
        &result,
        |record, renamed: &String| {
            record
                .destination(renamed)
                .outcome(OperationOutcome::verified(true))
        },
    );

    result
}

fn rename_on_disk(path: &str, new_name: &str) -> Result<String, CoreError> {
    let from = Path::new(path);
    let trimmed = new_name.trim();

    if trimmed.is_empty() {
        return Err(CoreError::InvalidInput("the new name is empty".into()));
    }
    // A name, not a path. Anything that could climb out of the folder is
    // refused here rather than sanitised into something the user did not ask
    // for.
    if trimmed
        != Path::new(trimmed)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
    {
        return Err(CoreError::InvalidInput(format!(
            "'{trimmed}' is a path, not a file name"
        )));
    }

    let parent = from.parent().ok_or_else(|| {
        CoreError::InvalidInput(format!("'{path}' has no folder to rename inside"))
    })?;
    let to = parent.join(trimmed);

    if !from.is_file() {
        return Err(CoreError::InvalidInput(format!("'{path}' is not a file")));
    }
    // SAFE_CREATE: refuse when the target exists rather than replacing it.
    // Renaming one title over another would destroy the second, and the user
    // asked to tidy a name, not to lose a disk.
    if to.exists() && to != from {
        return Err(CoreError::InvalidInput(format!(
            "'{trimmed}' already exists in that folder"
        )));
    }

    std::fs::rename(from, &to)?;
    Ok(to.to_string_lossy().to_string())
}

#[cfg(test)]
mod rename_tests {
    use super::rename_on_disk;

    fn tempdir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-rename-{}-{name}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_file_is_renamed_and_its_bytes_are_untouched() {
        let dir = tempdir("ok");
        let from = dir.join("ADPro_D1.adf");
        std::fs::write(&from, b"DISKDATA").unwrap();

        let to = rename_on_disk(from.to_str().unwrap(), "ADPro (Disk 1).adf").unwrap();

        assert!(!from.exists());
        assert_eq!(std::fs::read(&to).unwrap(), b"DISKDATA");
        assert_eq!(
            std::path::Path::new(&to).parent(),
            Some(dir.as_path()),
            "the file must stay in its folder"
        );
    }

    /// SAFE_CREATE. Renaming one title over another would destroy the second,
    /// and the user asked to tidy a name, not to lose a disk.
    #[test]
    fn an_existing_target_is_refused_and_both_files_survive() {
        let dir = tempdir("exists");
        let from = dir.join("A-Train Disk 1.adf");
        let occupied = dir.join("A-Train (Disk 1).adf");
        std::fs::write(&from, b"ONE").unwrap();
        std::fs::write(&occupied, b"TWO").unwrap();

        let refused = rename_on_disk(from.to_str().unwrap(), "A-Train (Disk 1).adf");

        assert!(refused.is_err());
        assert_eq!(std::fs::read(&from).unwrap(), b"ONE");
        assert_eq!(std::fs::read(&occupied).unwrap(), b"TWO");
    }

    /// A name, not a path. A rename must not be able to move a file somewhere
    /// else — least of all somewhere above its own folder.
    #[test]
    fn a_name_that_is_really_a_path_is_refused() {
        let dir = tempdir("traversal");
        let from = dir.join("game.adf");
        std::fs::write(&from, b"X").unwrap();

        for attempt in [
            "../escaped.adf",
            "..\\escaped.adf",
            "sub/escaped.adf",
            "C:/Windows/escaped.adf",
        ] {
            let refused = rename_on_disk(from.to_str().unwrap(), attempt);
            assert!(refused.is_err(), "{attempt:?} was accepted");
        }
        assert!(from.is_file(), "the original must be where it was");
    }

    #[test]
    fn an_empty_name_is_refused() {
        let dir = tempdir("empty");
        let from = dir.join("game.adf");
        std::fs::write(&from, b"X").unwrap();
        assert!(rename_on_disk(from.to_str().unwrap(), "   ").is_err());
        assert!(from.is_file());
    }

    #[test]
    fn a_missing_file_is_refused_rather_than_creating_one() {
        let dir = tempdir("missing");
        let from = dir.join("not-here.adf");
        assert!(rename_on_disk(from.to_str().unwrap(), "renamed.adf").is_err());
        assert!(!dir.join("renamed.adf").exists());
    }
}

/// What a finished index delivers to the UI.
#[derive(Debug, Clone, Serialize)]
pub struct IndexResult {
    pub job_id: JobId,
    pub dir_path: String,
    pub entries: Vec<CatalogueEntry>,
}

/// Start indexing a folder of Amiga titles.
///
/// Returns a job id immediately. Indexing reads inside every hardfile and
/// hashes every file, which for the collection this was built against is 1699
/// files and 3.74 GB — firmly §54/§55 territory, and the reason the user needs
/// a Stop rather than a spinner.
#[tauri::command]
pub fn gameindex_scan(
    dir_path: String,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
) -> AppResult<JobId> {
    let dir = PathBuf::from(&dir_path);
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();

    let id = spawn_job(
        &app,
        registry,
        "Indexing titles",
        move |job_id, progress| {
            let entries = scan_titles_with(&dir, progress)?;
            let _ = emit_app.emit(
                INDEX_RESULT_EVENT,
                IndexResult {
                    job_id,
                    dir_path,
                    entries,
                },
            );
            Ok(())
        },
    );

    Ok(id)
}

#[cfg(test)]
mod kickstart_offer_tests {
    use super::wanted_images;
    use crate::core::gameindex::record::{KickstartAlternative, KickstartNeed};

    fn need() -> KickstartNeed {
        KickstartNeed {
            image: None,
            size: None,
            crc16: None,
            rom_version: None,
            alternatives: Vec::new(),
        }
    }

    /// **The list wins when there is one**, and reading both fields would ask
    /// for the same image twice: `KickstartNeed::image` is documented as the
    /// first of `alternatives` when those exist.
    #[test]
    fn a_slave_naming_three_images_asks_for_three() {
        let asked = wanted_images(&KickstartNeed {
            image: Some("kick40063.A600".into()),
            size: Some(524_288),
            crc16: None,
            alternatives: vec![
                KickstartAlternative {
                    image: "kick40063.A600".into(),
                    crc16: 0x0001,
                },
                KickstartAlternative {
                    image: "kick40068.A1200".into(),
                    crc16: 0x0002,
                },
                KickstartAlternative {
                    image: "kick40068.A4000".into(),
                    crc16: 0x0003,
                },
            ],
            ..need()
        });
        assert_eq!(
            asked.len(),
            3,
            "three, not four - `image` is the first of them"
        );
        assert_eq!(asked[0].name, "kick40063.A600");
        assert_eq!(asked[0].crc16, Some(0x0001));
        assert_eq!(asked[2].crc16, Some(0x0003));
        assert!(asked.iter().all(|w| w.size == Some(524_288)));
    }

    #[test]
    fn a_slave_naming_one_image_asks_for_one() {
        let asked = wanted_images(&KickstartNeed {
            image: Some("kick34005.A500".into()),
            crc16: Some(0xABCD),
            size: Some(262_144),
            ..need()
        });
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0].name, "kick34005.A500");
        assert_eq!(asked[0].crc16, Some(0xABCD));
    }

    /// A great many titles declare no Kickstart at all. That is an empty
    /// answer, not a problem with the title.
    #[test]
    fn a_slave_naming_nothing_asks_for_nothing() {
        assert!(wanted_images(&need()).is_empty());
    }
}
