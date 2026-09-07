//! Task 9 of the prefs-and-wallpaper round: the adapter layer over
//! `core::appearance`, the wallpaper/screen-mode/shell-defaults engine Task 7
//! built and Task 8 verified against a real distribution tree.
//!
//! Two commands, both thin: deserialise, call core, serialise back
//! (CLAUDE.md: "commands/*.rs are thin adapters only"). Neither opens media,
//! neither decides what to change — that is `core::appearance::apply_appearance`'s
//! own job, already built and tested. This file's only real content is the
//! wire shapes: `core::amigaprefs::wbpattern::{Which, Placement}` and
//! `core::appearance::{WallpaperSource, AppearanceRequest, AppearanceOutcome}`
//! carry no `serde` derives of their own (by design — see `core/appearance/mod.rs`'s
//! module doc: `core/` stays promotable, and its own doc comment never asks for
//! a wire shape), so this module holds the translation, the same way
//! `commands/osinstall.rs::ComponentSummary` translates `core::osinstall::Component`
//! rather than the core type growing `Serialize` for one caller.
//!
//! # Logging: yes, and here is why
//!
//! `appearance_apply` writes real files — `WBPattern.prefs`, `ScreenMode.prefs`,
//! the five `Prefs/Env-Archive` shell defaults, and (for a host picture) a new
//! backdrop under `Prefs/Presets/Backdrops` — inside a distribution tree the
//! user chose the destination for. That the tree was built by `osinstall_apply`
//! rather than being the user's original media is not a reason to stay silent:
//! `osinstall_apply` itself logs its own writes into that same kind of tree
//! (`commands/osinstall.rs::osinstall_apply`), and this operation carries the
//! same fact this whole codebase logs everything else for — every
//! `guarded_write` inside `apply_appearance` takes a backup, and a user told
//! "done" without being told where the previous version went has been given
//! nothing (CLAUDE.md, "the failure that does not crash"). So this goes
//! through `oplog::write_result`, synchronously, the same shape
//! `commands/adf.rs::adf_add_file` uses — not a background job, because even
//! the largest real case (arranging icons across a 3.9-scale tree, hundreds
//! of small `.info` writes) is nowhere near the hundreds of megabytes a real
//! media copy moves. **Corrected 2026-09-06** (final whole-branch review of
//! the drawer-icons round, C5): this used to say "at most eight small text
//! files and one image", true before `arrange_icons` existed and false
//! since — 361 of the owner's own 798 real icons are unplaced, so a single
//! call can commit that many files. The oplog's own `Written`/`Backups`
//! details are capped the same way — see `some_of` at the call site below.
//!
//! `appearance_backdrops` reads a directory listing and writes nothing, so it
//! is not logged — the same rule `osinstall_components`/`osinstall_packages`
//! (also read-only, also unlogged) already follow.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::core::amigaprefs::wbpattern;
use crate::core::osinstall::apply::some_of;

#[cfg(test)]
use crate::core::appearance::apply_appearance;
use crate::core::appearance::{
    apply_appearance_with, backdrops_in_tree, AppearanceOutcome, AppearanceRequest, WallpaperSource,
};
use crate::core::jobs::JobId;
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::error::AppResult;

use super::jobs::{spawn_job, JobRegistry};
use super::oplog::{user_operation, write_to_path};

// ---------------------------------------------------------------------------
// Wire shapes — request
// ---------------------------------------------------------------------------

/// [`wbpattern::Which`] on the wire.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WireWhich {
    Root,
    Drawer,
    Screen,
}

impl From<WireWhich> for wbpattern::Which {
    fn from(value: WireWhich) -> Self {
        match value {
            WireWhich::Root => wbpattern::Which::Root,
            WireWhich::Drawer => wbpattern::Which::Drawer,
            WireWhich::Screen => wbpattern::Which::Screen,
        }
    }
}

/// [`wbpattern::Placement`] on the wire.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WirePlacement {
    Tile,
    Center,
    Scale,
    ScaleGood,
}

impl From<WirePlacement> for wbpattern::Placement {
    fn from(value: WirePlacement) -> Self {
        match value {
            WirePlacement::Tile => wbpattern::Placement::Tile,
            WirePlacement::Center => wbpattern::Placement::Center,
            WirePlacement::Scale => wbpattern::Placement::Scale,
            WirePlacement::ScaleGood => wbpattern::Placement::ScaleGood,
        }
    }
}

/// [`WallpaperSource`] on the wire — an internally tagged enum so the
/// frontend sends `{ kind: "already-in-tree", amigaPath }` or
/// `{ kind: "host-picture", path, colours }` rather than two separate
/// optional fields the command would have to reconcile itself.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum WireWallpaperSource {
    #[serde(rename_all = "camelCase")]
    AlreadyInTree {
        amiga_path: String,
    },
    HostPicture {
        path: PathBuf,
        colours: usize,
    },
}

impl From<WireWallpaperSource> for WallpaperSource {
    fn from(value: WireWallpaperSource) -> Self {
        match value {
            WireWallpaperSource::AlreadyInTree { amiga_path } => {
                WallpaperSource::AlreadyInTree { amiga_path }
            }
            WireWallpaperSource::HostPicture { path, colours } => {
                WallpaperSource::HostPicture { path, colours }
            }
        }
    }
}

/// A wallpaper assignment on the wire — [`AppearanceRequest::wallpaper`]'s
/// tuple, given field names.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireWallpaperAssignment {
    pub which: WireWhich,
    pub source: WireWallpaperSource,
    pub placement: WirePlacement,
}

/// [`AppearanceRequest`] on the wire — what `appearance_apply` deserialises.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceApplyRequest {
    pub wallpaper: Option<WireWallpaperAssignment>,
    pub screen_depth: Option<u16>,
    pub shell_defaults: bool,
    /// [`AppearanceRequest::arrange_icons`] on the wire. `#[serde(default)]`
    /// so an older frontend build that has never heard of this field still
    /// deserialises — the same "nothing changes unless the user changes it"
    /// rule CLAUDE.md states for settings applies to a request shape too: a
    /// caller that never asked for icon arranging must not start getting it
    /// for free just because this field exists now.
    #[serde(default)]
    pub arrange_icons: bool,
}

impl From<AppearanceApplyRequest> for AppearanceRequest {
    fn from(value: AppearanceApplyRequest) -> Self {
        AppearanceRequest {
            wallpaper: value
                .wallpaper
                .map(|w| (w.which.into(), w.source.into(), w.placement.into())),
            screen_depth: value.screen_depth,
            shell_defaults: value.shell_defaults,
            arrange_icons: value.arrange_icons,
        }
    }
}

// ---------------------------------------------------------------------------
// Wire shapes — outcome
// ---------------------------------------------------------------------------

/// [`AppearanceOutcome`] on the wire. Paths become `String` (`Path::display`),
/// the same rendering `core::adf::MutationOutcome::backup_path` already uses
/// for the identical reason — a raw `PathBuf` reaching `serde_json` is fine on
/// this codebase's own Windows CI, but a `String` is what every other outcome
/// type in this codebase already sends across the wire, and there is no
/// reason for this one to be the first exception.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceOutcomeWire {
    pub written: Vec<String>,
    pub backups: Vec<String>,
    pub picture_placed: Option<String>,
    pub amiga_path: Option<String>,
    /// [`AppearanceOutcome::icons_placed`] on the wire.
    pub icons_placed: usize,
    /// [`AppearanceOutcome::drawers_arranged`] on the wire.
    pub drawers_arranged: usize,
    /// [`AppearanceOutcome::icons_skipped`] on the wire — named, not merely
    /// counted, so a malformed icon is not silently dropped (CLAUDE.md: "never
    /// claim what you did not do").
    pub icons_skipped: Vec<String>,
}

impl From<AppearanceOutcome> for AppearanceOutcomeWire {
    fn from(value: AppearanceOutcome) -> Self {
        Self {
            written: value
                .written
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            backups: value
                .backups
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            picture_placed: value.picture_placed.map(|p| p.display().to_string()),
            amiga_path: value.amiga_path,
            icons_placed: value.icons_placed,
            drawers_arranged: value.drawers_arranged,
            icons_skipped: value
                .icons_skipped
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Every backdrop `tree` already carries under
/// `Prefs/Presets/Backdrops` — read-only, writes nothing, not logged (the same
/// rule `osinstall_components`/`osinstall_packages` follow for their own
/// read-only listings).
#[tauri::command]
pub fn appearance_backdrops(tree: PathBuf) -> AppResult<Vec<String>> {
    Ok(backdrops_in_tree(&tree)?)
}

/// The whole fallible half of `appearance_apply`, kept synchronous and
/// unit-tested here — the same shape `commands/osinstall.rs::preview_collisions`
/// already uses for the identical reason, and still the fastest way to pin
/// that a core refusal's sentence reaches the caller unrewritten (ART-060 /
/// CLAUDE.md). Not what the `appearance_apply` command itself calls any more
/// (ART-248): the command runs the sink-taking `apply_appearance_with` on a
/// job thread instead, so this stays on the thin `apply_appearance`/`NoProgress`
/// wrapper on purpose, the same `scan_titles`/`scan_titles_with` split
/// `core::gameindex::scan` already uses.
///
/// `core::appearance::apply_appearance` does the actual work — plans every
/// requested part first, refuses the whole call before writing anything if any
/// part cannot be done, and only then commits through `guarded_write`
/// (`BackupPolicy::CONFIG`) for each file. This function only converts the
/// wire request into `AppearanceRequest`, calls it, and converts the outcome
/// back — **never rewrites, wraps or prettifies the `Err` case** (ART-060 /
/// CLAUDE.md: the frontend must receive the sentence core wrote, because this
/// round's error messages name specific files and backup paths a rewrite
/// would destroy). `apply_appearance`'s `CoreError` reaches `AppError` through
/// its own `#[from]` conversion (`?`), not a hand-written `map_err`.
#[cfg(test)]
fn apply_appearance_request(
    tree: &std::path::Path,
    request: AppearanceApplyRequest,
) -> AppResult<AppearanceOutcomeWire> {
    let core_request: AppearanceRequest = request.into();
    let outcome = apply_appearance(tree, &core_request)?;
    Ok(outcome.into())
}

/// A finished `appearance_apply` job's own answer. `job_id` stays snake_case
/// to match every other job result in ART (`RehearsalResult`,
/// `AmigaInstallResult`); the outcome's own fields are flattened in beside it
/// rather than nested, so the frontend keeps reading `AppearanceOutcomeWire`'s
/// familiar camelCase shape.
#[derive(Debug, Clone, Serialize)]
pub struct AppearanceApplyResult {
    pub job_id: JobId,
    #[serde(flatten)]
    pub outcome: AppearanceOutcomeWire,
}

/// The event a finished `appearance_apply` job's own answer arrives on.
pub const APPEARANCE_APPLY_EVENT: &str = "appearance-apply-result";

/// Apply a wallpaper, a screen depth, and/or the shell defaults to `tree`.
///
/// **ART-248: a job, not a command-thread call.** The wallpaper path alone
/// can mean a median-cut quantise pass over a couple of million pixels
/// (`core::picture::quantise`, O(colours × pixels)) and arranging icons
/// across a 3.9-scale tree can commit hundreds of small files — §54/§55
/// already made this call for `commands/layout.rs`, `commands/archives.rs`
/// and `commands/card.rs`, and this command was the one added without that
/// wrapper. Returns a `JobId` immediately; progress arrives on the ordinary
/// `job-progress` event (`core::appearance::apply_appearance_with` reports a
/// real "N of M files" count once planning has finished, never a fixed-width
/// bar for the indefinite planning phase before it — CLAUDE.md), and the
/// finished outcome on [`APPEARANCE_APPLY_EVENT`]. A cancelled run never
/// reaches that event at all — the job bar's own `Cancelled` state is the
/// only place it is reported, the same split `firstboot_rehearse` already
/// uses.
///
/// The oplog write happens on the job thread through `write_to_path`, same as
/// `firstboot_rehearse` — a background job cannot carry a Tauri `State`
/// across the thread boundary. See the module doc for why this is logged at
/// all, and never rewrites, wraps or prettifies a core refusal's own sentence
/// (ART-060).
#[tauri::command]
pub fn appearance_apply(
    tree: PathBuf,
    request: AppearanceApplyRequest,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<JobId> {
    let log_path = oplog.path().to_path_buf();
    let emit_app = app.clone();
    let for_log = tree.display().to_string();

    let id = spawn_job(
        &app,
        Arc::clone(&registry),
        "Applying appearance to distribution tree",
        move |job_id, progress| {
            let core_request: AppearanceRequest = request.into();
            let result = apply_appearance_with(&tree, &core_request, progress)
                .map(AppearanceOutcomeWire::from);

            // §53. Best-effort, and never able to fail the operation it
            // describes.
            let record =
                user_operation("Apply appearance to distribution tree").destination(for_log);
            let record = match &result {
                Ok(outcome) => {
                    // C5 (final whole-branch review): capped the same way a
                    // package refusal naming up to 211 real files already is
                    // (`core::osinstall::apply::some_of`) — arranging icons
                    // across a 3.9-scale tree can commit hundreds of `.info`
                    // files in one call, and joining every one of them into a
                    // single log line is as unusable on disk as it is on
                    // screen.
                    let record = record.detail("Written", some_of(&outcome.written));
                    let record = if outcome.backups.is_empty() {
                        record
                    } else {
                        record
                            .backup(outcome.backups.first().cloned())
                            .detail("Backups", some_of(&outcome.backups))
                    };
                    record.outcome(OperationOutcome::verified(true))
                }
                // Covers a genuine failure and a cancellation alike — the
                // same shape `firstboot_rehearse`'s own `perform` uses, and
                // for the same reason: a cancellation partway is not
                // different in kind from a commit-phase I/O failure, and
                // both already leave real files behind that the record
                // should not pretend never happened.
                Err(err) => record.failed(err),
            };
            write_to_path(&log_path, &record);

            let outcome = result?;
            let _ = emit_app.emit(
                APPEARANCE_APPLY_EVENT,
                AppearanceApplyResult { job_id, outcome },
            );
            Ok(())
        },
    );

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::amigaprefs::iff::tests_support::synthetic_prefs;
    use crate::core::ScratchDir;

    // -----------------------------------------------------------------
    // Both commands are actually reachable — the specific failure this
    // round's own plan was written to avoid (CLAUDE.md: "a feature that
    // compiles, tests green, and is unreachable... this project has shipped
    // twice"). Asserted against `lib.rs`'s own source text rather than a
    // macro trick, since nothing in this codebase already has a
    // `every_command_is_registered`-style test to follow (checked first).
    // -----------------------------------------------------------------
    #[test]
    fn both_commands_are_registered_in_invoke_handler() {
        let lib_rs = include_str!("../lib.rs");
        assert!(
            lib_rs.contains("commands::appearance::appearance_backdrops"),
            "appearance_backdrops must be registered in invoke_handler![]"
        );
        assert!(
            lib_rs.contains("commands::appearance::appearance_apply"),
            "appearance_apply must be registered in invoke_handler![]"
        );
    }

    // -----------------------------------------------------------------
    // A core error's own sentence reaches the caller verbatim — nothing in
    // this file rewrites, wraps or prettifies it (ART-060 / CLAUDE.md: "the
    // frontend must receive the sentence core wrote").
    // -----------------------------------------------------------------
    #[test]
    fn a_missing_wbpattern_prefs_is_refused_with_cores_own_sentence_unchanged() {
        let scratch = ScratchDir::new("art-appearance-cmd", "missing-wbpattern");
        let tree = scratch.path().to_path_buf();
        std::fs::create_dir_all(tree.join("Prefs")).unwrap();

        let request = AppearanceApplyRequest {
            wallpaper: Some(WireWallpaperAssignment {
                which: WireWhich::Root,
                source: WireWallpaperSource::AlreadyInTree {
                    amiga_path: "Sys:Prefs/Presets/Backdrops/default_pal.iff".to_string(),
                },
                placement: WirePlacement::Scale,
            }),
            screen_depth: None,
            shell_defaults: false,
            arrange_icons: false,
        };

        // The ground truth: `core::appearance::apply_appearance`'s own
        // `CoreError::Display`, called directly — never through this file.
        let core_request: AppearanceRequest = request.clone().into();
        let core_err = apply_appearance(&tree, &core_request).unwrap_err();
        let core_text = format!("{core_err}");
        // Pinned as a substring too, not merely "some error occurred" — so a
        // change to what core itself says is visible here rather than only
        // in `core::appearance`'s own tests.
        assert!(
            core_text.contains(
                "'Prefs/Env-Archive/Sys/WBPattern.prefs' was not found under this \
                 distribution tree"
            ),
            "core's own sentence must name the missing file: {core_text}"
        );

        // Through `apply_appearance_request` — the actual command-layer
        // function `appearance_apply` calls.
        let app_err = apply_appearance_request(&tree, request).unwrap_err();
        let rendered = format!("{app_err}");

        // Exact equality, not `.contains()` — a command-layer rewrite that
        // *wraps* the sentence (`format!("could not apply appearance: {e}")`,
        // say) would still contain this substring and slip past a `.contains`
        // check; only byte-for-byte equality with core's own text catches
        // that (CLAUDE.md's own warning: "a refusal test that searched for
        // two substrings separately where a weaker sentence contained both").
        assert_eq!(
            rendered, core_text,
            "AppError::Display must carry core's sentence unchanged, verbatim, with nothing \
             prepended or appended"
        );
    }

    // -----------------------------------------------------------------
    // The outcome's backup path reaches the wire — a user told "done"
    // without being told where the previous version went has been given
    // nothing.
    // -----------------------------------------------------------------
    fn wbpattern_bytes(amiga_path: &str) -> Vec<u8> {
        let backdrop = wbpattern::Backdrop {
            reserved: [0u8; 16],
            which: wbpattern::Which::Root,
            placement: wbpattern::Placement::Scale,
            precision: wbpattern::Precision::Image,
            dither: wbpattern::Dither::Good,
            no_remap: false,
            other_flags: 0,
            revision: 0,
            content: wbpattern::Content::Picture(amiga_path.to_string()),
        };
        synthetic_prefs(&[(*b"PTRN", wbpattern::write_backdrop(&backdrop).unwrap())])
    }

    #[test]
    fn applying_a_screen_depth_change_reports_a_backup_on_the_wire() {
        let scratch = ScratchDir::new("art-appearance-cmd", "screen-depth-wire");
        let tree = scratch.path().to_path_buf();
        let sys_dir = tree.join("Prefs").join("Env-Archive").join("Sys");
        std::fs::create_dir_all(&sys_dir).unwrap();
        std::fs::write(
            sys_dir.join("WBPattern.prefs"),
            wbpattern_bytes("Sys:Prefs/Presets/Backdrops/default_pal.iff"),
        )
        .unwrap();
        let mode = crate::core::amigaprefs::screenmode::ScreenMode {
            reserved: [0u8; 16],
            display_id: 0x0002_9000,
            width: crate::core::amigaprefs::screenmode::USE_MODE_DEFAULT,
            height: crate::core::amigaprefs::screenmode::USE_MODE_DEFAULT,
            depth: 4,
            control: 1,
        };
        std::fs::write(
            sys_dir.join("ScreenMode.prefs"),
            synthetic_prefs(&[(
                *b"SCRM",
                crate::core::amigaprefs::screenmode::write_screen_mode(&mode).unwrap(),
            )]),
        )
        .unwrap();

        let request = AppearanceApplyRequest {
            wallpaper: None,
            screen_depth: Some(8),
            shell_defaults: false,
            arrange_icons: false,
        };
        let outcome = apply_appearance_request(&tree, request).unwrap();

        assert_eq!(outcome.written.len(), 1);
        assert_eq!(
            outcome.backups.len(),
            1,
            "the previous ScreenMode.prefs must be backed up and named on the wire"
        );
        assert!(std::path::Path::new(&outcome.backups[0]).is_file());
    }

    // -----------------------------------------------------------------
    // Wire shapes, pinned the way `commands/osinstall.rs::wire_shapes` pins
    // its own — `src/lib/appearance.ts` is maintained by hand, so this test
    // only pins the Rust side.
    // -----------------------------------------------------------------
    #[test]
    fn outcome_serializes_with_the_keys_the_wrapper_reads() {
        let outcome = AppearanceOutcomeWire {
            written: vec!["a".to_string()],
            backups: vec!["b".to_string()],
            picture_placed: Some("c".to_string()),
            amiga_path: Some("Sys:Prefs/Presets/Backdrops/x.iff".to_string()),
            icons_placed: 3,
            drawers_arranged: 2,
            icons_skipped: vec!["Sys:Broken/broken.info".to_string()],
        };
        let value = serde_json::to_value(&outcome).unwrap();
        let keys: std::collections::BTreeSet<String> =
            value.as_object().unwrap().keys().cloned().collect();
        let expected: std::collections::BTreeSet<String> = [
            "written",
            "backups",
            "picturePlaced",
            "amigaPath",
            "iconsPlaced",
            "drawersArranged",
            "iconsSkipped",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        assert_eq!(keys, expected);
    }

    #[test]
    fn request_deserializes_from_the_shape_the_wrapper_sends() {
        let json = serde_json::json!({
            "wallpaper": {
                "which": "root",
                "source": {
                    "kind": "host-picture",
                    "path": "C:\\wallpaper.png",
                    "colours": 16
                },
                "placement": "center"
            },
            "screenDepth": 8,
            "shellDefaults": true
        });
        let request: AppearanceApplyRequest = serde_json::from_value(json).unwrap();
        assert_eq!(request.screen_depth, Some(8));
        assert!(request.shell_defaults);
        let wallpaper = request.wallpaper.expect("wallpaper must deserialize");
        matches!(wallpaper.which, WireWhich::Root);
        matches!(wallpaper.placement, WirePlacement::Center);
        match wallpaper.source {
            WireWallpaperSource::HostPicture { path, colours } => {
                assert_eq!(path, PathBuf::from("C:\\wallpaper.png"));
                assert_eq!(colours, 16);
            }
            other => panic!("expected HostPicture, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------
    // ART-248: the job result's own wire shape. `job_id` stays snake_case —
    // `jobs.ts` matches on it, same as `RehearsalResult` — while the
    // outcome's own fields sit flattened in beside it rather than nested, so
    // `src/lib/appearance.ts` keeps reading the same `AppearanceOutcome`
    // shape it always has.
    // -----------------------------------------------------------------
    #[test]
    fn the_apply_result_crosses_the_wire_with_a_snake_case_job_id_and_a_flattened_outcome() {
        let result = AppearanceApplyResult {
            job_id: 3,
            outcome: AppearanceOutcomeWire {
                written: vec!["a".to_string()],
                backups: vec![],
                picture_placed: None,
                amiga_path: Some("Sys:Prefs/Presets/Backdrops/x.iff".to_string()),
                icons_placed: 0,
                drawers_arranged: 0,
                icons_skipped: vec![],
            },
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["job_id"], 3, "jobs.ts matches on job_id");
        assert!(json.get("jobId").is_none(), "and never on jobId");
        assert_eq!(json["written"][0], "a");
        assert_eq!(json["amigaPath"], "Sys:Prefs/Presets/Backdrops/x.iff");
        assert!(
            json.get("outcome").is_none(),
            "the outcome is flattened in, not nested under its own key"
        );
    }
}
