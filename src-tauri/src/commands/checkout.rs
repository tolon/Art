//! F4 — checkout and checkin (brief §6).
//!
//! ART does not implement an editor; it implements the safe round trip. The
//! file comes out to a temp folder, the user's own editor opens it, and the
//! bytes go back in through the same journalled write path as any other copy.
//!
//! Two shell decisions live here rather than in core:
//!
//! - **Launching the editor.** `core/` may not start processes. The command
//!   layer does it, with structured argv — never a shell string built from a
//!   file name that came off an Amiga disk.
//! - **Where the temp files go.** The app cache directory, one folder per
//!   image, so a restart finds the work and the OS knows it is scratch.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::State;

use super::oplog::{user_operation, write_result};
use crate::core::error::{CoreError, CoreResult};
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::core::volume::checkout::{
    checkout_id, has_crlf, hash_of, icon_for, looks_binary, state_of, temp_path_for,
    to_amiga_line_endings, Checkout, CheckoutState, CheckoutStore, JsonlCheckouts,
    MAX_CHECKOUT_BYTES,
};
use crate::core::volume::mount::mount;
use crate::error::{AppError, AppResult};

/// Everything the checkout commands need, resolved once at startup.
pub struct CheckoutState_ {
    root: PathBuf,
    store: Mutex<JsonlCheckouts>,
}

impl CheckoutState_ {
    pub fn new(root: PathBuf) -> Self {
        let manifest = root.join("checkouts.jsonl");
        let store = JsonlCheckouts::load(&manifest).unwrap_or_else(|err| {
            log::warn!("the checkout manifest could not be read ({err}); starting empty");
            JsonlCheckouts::empty_at(manifest)
        });
        Self {
            root,
            store: Mutex::new(store),
        }
    }

    fn with_store<T>(&self, run: impl FnOnce(&JsonlCheckouts) -> T) -> T {
        let held = self
            .store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        run(&held)
    }
}

/// A checkout and what its temp file says now.
#[derive(Debug, Clone, Serialize)]
pub struct CheckoutRow {
    pub id: String,
    pub image: String,
    pub volume_index: usize,
    pub dir_block: u32,
    pub entry_block: u32,
    pub name: String,
    pub temp_path: String,
    pub bytes: u64,
    pub state: CheckoutState,
}

fn row_of(checkout: Checkout) -> CheckoutRow {
    let state = state_of(&checkout);
    CheckoutRow {
        id: checkout.id,
        image: checkout.image,
        volume_index: checkout.volume_index,
        dir_block: checkout.dir_block,
        entry_block: checkout.entry_block,
        name: checkout.name,
        temp_path: checkout.temp_path,
        bytes: checkout.bytes,
        state,
    }
}

/// Check a file out of a volume for editing.
///
/// A second checkout of the same file **reopens the existing copy** rather
/// than overwriting it: the first one may hold an edit the user has not saved
/// back yet, and replacing it would throw that away without asking.
#[tauri::command]
pub fn checkout_open(
    path: String,
    volume_index: usize,
    dir_block: Option<u32>,
    entry_block: u32,
    state: State<'_, CheckoutState_>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<CheckoutRow> {
    let image = PathBuf::from(path.trim());
    let id = checkout_id(&image, volume_index, entry_block);

    // Already out? Hand back what is there.
    if let Some(existing) = state.with_store(|store| store.get(&id))? {
        if PathBuf::from(&existing.temp_path).exists() {
            return Ok(row_of(existing));
        }
        // The manifest outlived its temp file — fall through and make it again.
    }

    let result = (|| -> CoreResult<Checkout> {
        let entry = super::volume_write::pick_volume(&image, volume_index)?;
        let (device, geometry) = mount(&image, &entry)?;

        let set = crate::core::volume::write::layout::BlockSet::new(geometry.block_size);
        let header = crate::core::volume::read_block_vec(&device, entry_block)?;
        let name = crate::core::volume::write::dir::name_of(&header);
        if crate::core::volume::write::dir::is_directory(&header)? {
            return Err(CoreError::InvalidInput(
                "a folder cannot be edited — open a file inside it".into(),
            ));
        }

        let data =
            crate::core::volume::write::file::read_file(&device, &set, &geometry, entry_block)?;
        if data.len() as u64 > MAX_CHECKOUT_BYTES {
            return Err(CoreError::InvalidInput(format!(
                "'{name}' is {} bytes, more than ART checks out for editing",
                data.len()
            )));
        }

        let temp = temp_path_for(&state.root, &image, &id, &name)?;
        if let Some(parent) = temp.parent() {
            std::fs::create_dir_all(parent)?;
        }
        crate::core::safety::atomic::atomic_write(&temp, &data)?;

        Ok(Checkout {
            id: id.clone(),
            image: image.display().to_string(),
            volume_index,
            dir_block: dir_block.unwrap_or(geometry.root_block),
            entry_block,
            name,
            temp_path: temp.display().to_string(),
            sha256: hash_of(&data),
            bytes: data.len() as u64,
            was_lf_only: !has_crlf(&data),
            is_binary: looks_binary(&data),
        })
    })()
    .and_then(|checkout| {
        state.with_store(|store| store.put(checkout.clone()))?;
        Ok(checkout)
    })
    .map(row_of)
    .map_err(AppError::from);

    write_result(
        &oplog,
        user_operation("Check a file out for editing").source(format!("{path}:{entry_block}")),
        &result,
        |record, row: &CheckoutRow| {
            record
                .destination(row.temp_path.clone())
                .detail("Bytes", row.bytes.to_string())
                .outcome(OperationOutcome::verified(true))
        },
    );

    result
}

/// Every file currently checked out, with its state.
#[tauri::command]
pub fn checkout_list(state: State<'_, CheckoutState_>) -> AppResult<Vec<CheckoutRow>> {
    Ok(state
        .with_store(|store| store.all())?
        .into_iter()
        .map(row_of)
        .collect())
}

/// Open a checked-out file in the user's editor.
///
/// Structured argv, never a shell string: the file name came off an Amiga disk
/// and a name containing `&` or `"` must not be able to become a second
/// command (`core/security`'s rule, applied outside core).
#[tauri::command]
pub fn checkout_edit(
    id: String,
    editor: Option<String>,
    state: State<'_, CheckoutState_>,
) -> AppResult<()> {
    let checkout = state
        .with_store(|store| store.get(&id))?
        .ok_or_else(|| CoreError::InvalidInput(format!("no file is checked out as {id}")))?;

    let temp = PathBuf::from(&checkout.temp_path);
    if !temp.exists() {
        return Err(CoreError::InvalidInput(format!(
            "the working copy of '{}' is no longer there",
            checkout.name
        ))
        .into());
    }

    let spawned = match editor.as_deref().map(str::trim).filter(|e| !e.is_empty()) {
        Some(program) => std::process::Command::new(program).arg(&temp).spawn(),
        // No editor configured: hand it to whatever the OS associates with it.
        None => open_with_system_default(&temp),
    };

    spawned.map_err(|err| {
        CoreError::InvalidInput(format!(
            "the editor could not be started: {err}. Set one in Settings, or open \
             '{}' yourself.",
            temp.display()
        ))
    })?;
    Ok(())
}

#[cfg(windows)]
fn open_with_system_default(path: &std::path::Path) -> std::io::Result<std::process::Child> {
    // `cmd /C start` needs an empty title argument first, or a quoted path is
    // taken as the window title. The path itself is still a separate argv
    // entry, so nothing in it is parsed as a command.
    std::process::Command::new("cmd")
        .args(["/C", "start", ""])
        .arg(path)
        .spawn()
}

#[cfg(not(windows))]
fn open_with_system_default(path: &std::path::Path) -> std::io::Result<std::process::Child> {
    std::process::Command::new("xdg-open").arg(path).spawn()
}

/// Write an edited file back into its image.
///
/// Refuses when nothing changed, so an editor that opened and closed a file
/// cannot cause a write. The size changing is normal and goes through the full
/// allocation path — the old blocks come back and new ones are taken, under
/// one journal.
#[tauri::command]
pub fn checkout_checkin(
    id: String,
    convert_line_endings: Option<bool>,
    state: State<'_, CheckoutState_>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<super::volume_write::MutationResult> {
    let checkout = state
        .with_store(|store| store.get(&id))?
        .ok_or_else(|| CoreError::InvalidInput(format!("no file is checked out as {id}")))?;

    let result = (|| -> CoreResult<super::volume_write::MutationResult> {
        let temp = PathBuf::from(&checkout.temp_path);
        let mut data = std::fs::read(&temp)?;

        if hash_of(&data) == checkout.sha256 {
            return Err(CoreError::InvalidInput(format!(
                "'{}' has not been changed, so there is nothing to write back",
                checkout.name
            )));
        }
        if data.len() as u64 > MAX_CHECKOUT_BYTES {
            return Err(CoreError::InvalidInput(format!(
                "'{}' has grown to {} bytes, more than ART writes back",
                checkout.name,
                data.len()
            )));
        }

        if convert_line_endings.unwrap_or(false) {
            data = to_amiga_line_endings(&data);
        }

        super::volume_write::replace_file(
            &PathBuf::from(&checkout.image),
            checkout.volume_index,
            checkout.dir_block,
            checkout.entry_block,
            &checkout.name,
            &data,
        )
    })()
    .map_err(AppError::from);

    // The manifest entry and the temp file only go once the write succeeded.
    // A failed checkin must leave the edit exactly where the user left it
    // (§6) — losing it would be far worse than the failure itself.
    if result.is_ok() {
        let _ = state.with_store(|store| store.remove(&id));
        let _ = std::fs::remove_file(&checkout.temp_path);
    }

    write_result(
        &oplog,
        user_operation("Write an edited file back")
            .source(checkout.temp_path.clone())
            .destination(format!("{}:{}", checkout.image, checkout.name)),
        &result,
        |record, made: &super::volume_write::MutationResult| {
            record
                .detail("Strategy", made.strategy.clone())
                .outcome(OperationOutcome::verified(made.verified))
        },
    );

    result
}

/// Throw a checkout away without writing it back.
///
/// The temp file goes with it, because keeping an orphan the user cannot see
/// listed anywhere is how a scratch directory fills up over a year.
#[tauri::command]
pub fn checkout_discard(id: String, state: State<'_, CheckoutState_>) -> AppResult<()> {
    if let Some(checkout) = state.with_store(|store| store.get(&id))? {
        let _ = std::fs::remove_file(&checkout.temp_path);
    }
    state.with_store(|store| store.remove(&id))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// `.info` pairing (§7.1)
// ---------------------------------------------------------------------------

/// The icon that belongs to an entry, when the volume actually holds one.
///
/// Renaming `Game` without renaming `Game.info` leaves a game that is
/// invisible on Workbench, so the UI offers to do both — but only when there
/// is an icon to pair with, which is why this asks the volume rather than
/// guessing from the name.
#[derive(Debug, Clone, Serialize)]
pub struct IconPair {
    pub icon_name: String,
    pub icon_block: u32,
}

#[tauri::command]
pub fn volume_icon_for(
    path: String,
    volume_index: usize,
    dir_block: Option<u32>,
    name: String,
) -> AppResult<Option<IconPair>> {
    let Some(icon_name) = icon_for(&name) else {
        return Ok(None);
    };

    let image = PathBuf::from(path.trim());
    let entry = super::volume_write::pick_volume(&image, volume_index)?;
    let (device, geometry) = mount(&image, &entry)?;

    let set = crate::core::volume::write::layout::BlockSet::new(geometry.block_size);
    let dir = dir_block.unwrap_or(geometry.root_block);
    let found =
        crate::core::volume::write::dir::find_entry(&device, &set, &geometry, dir, &icon_name)?;

    Ok(found.map(|entry| IconPair {
        icon_name: entry.name,
        icon_block: entry.block,
    }))
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-cmd-co-{name}-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The manifest has to outlive the process: a checkout is a file the user
    /// is editing right now, and losing the mapping leaves an orphan in the
    /// temp directory and an edit with nowhere to go back to.
    #[test]
    fn the_state_reloads_its_manifest_from_disk() {
        let dir = scratch("reload");

        let first = CheckoutState_::new(dir.clone());
        first
            .with_store(|store| {
                store.put(Checkout {
                    id: "abc".into(),
                    image: "D:/Work.adf".into(),
                    volume_index: 0,
                    dir_block: 880,
                    entry_block: 900,
                    name: "Startup-Sequence".into(),
                    temp_path: dir.join("Startup-Sequence").display().to_string(),
                    sha256: hash_of(b"x"),
                    bytes: 1,
                    was_lf_only: true,
                    is_binary: false,
                })
            })
            .unwrap();

        let second = CheckoutState_::new(dir.clone());
        let all = second.with_store(|store| store.all()).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "Startup-Sequence");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The temp file is gone, so the row says so rather than the UI offering
    /// a checkin of nothing.
    #[test]
    fn a_row_reports_a_missing_working_copy() {
        let dir = scratch("row-missing");
        let row = row_of(Checkout {
            id: "abc".into(),
            image: "D:/Work.adf".into(),
            volume_index: 0,
            dir_block: 880,
            entry_block: 900,
            name: "Gone".into(),
            temp_path: dir.join("Gone").display().to_string(),
            sha256: hash_of(b"x"),
            bytes: 1,
            was_lf_only: true,
            is_binary: false,
        });

        assert_eq!(row.state, CheckoutState::Missing);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
