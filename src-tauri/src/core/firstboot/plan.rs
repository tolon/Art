//! Deciding what first boot will run — every refusal here, before a byte.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::scripts::{fixed_files, WIZARD_FILE};
use super::{WizardWindow, WIZARD_PATH, WIZARD_STEP};
use crate::core::amigaprefs::env;
use crate::core::error::{CoreError, CoreResult};
use crate::core::osinstall::plan::KEYMAP_SELECTION;
use crate::core::osinstall::startup::has_block;

#[derive(Debug, Clone)]
pub struct FirstBootRequest {
    pub tree: PathBuf,
    /// Write `S:FirstBoot/90-prefs`, the wizard (phase 3 design §2 decision 5).
    pub ask_prefs: bool,
}

/// Whether `10-hardware` can mount the FAT boot partition.
///
/// Not a refusal: a build that never needed `EMU68BOOT:` must not be blocked
/// by a handler it does not have. The step skips, and the preview names the
/// package that would change that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FatMount {
    Available,
    Unavailable { needs: &'static str },
}

/// The catalogue id of Aminet `util/boot/reboot`
/// (`core/sources/bundle/catalogue/acilis.json`).
pub const REBOOT_PACKAGE: &str = "reboot";

/// Whether the step wrapper can carry out a reboot request. Not a refusal —
/// the `FatMount` rule: first boot is still written, the Amiga logs `reboot
/// unavailable` and carries on, and the preview names the package (design §2
/// decision 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RebootCommand {
    Available,
    Unavailable { needs: &'static str },
}

/// What `90-prefs` will do with one window, read from the tree at preview
/// time. The Amiga decides on the day; Appearance applied after this preview
/// can still turn an `Ask` into a skipped window (design §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WindowState {
    Ask,
    SetByArt,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WizardRow {
    pub window: WizardWindow,
    pub state: WindowState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WizardPlan {
    /// Locale, Input, ScreenMode — the order the script asks them.
    pub rows: Vec<WizardRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedStep {
    /// `10-hardware` — the file name in `S:FirstBoot/`, which is also the
    /// name the report uses.
    pub name: String,
    pub tree_path: String,
    /// `true` for text compiled into ART; generated steps arrive in phase 3.
    pub fixed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstBootPlan {
    pub tree: PathBuf,
    pub steps: Vec<PlannedStep>,
    pub fat_mount: FatMount,
    /// `S/User-Startup` is there already — the merge will edit it, and the
    /// command layer backs it up first.
    pub user_startup_exists: bool,
    /// `S/ART-FirstBoot` is already in the tree: writing again is a re-install.
    pub already_written: bool,
    /// What the fixed files add to the tree, for the card step's arithmetic.
    pub bytes_added: u64,
    pub reboot: RebootCommand,
    /// `None` when the wizard was not asked for.
    pub wizard: Option<WizardPlan>,
    /// `S/User-Startup` carries a well-formed `keymap-selection` block, so
    /// the write puts `ART_Set_Input` down (design §4.3) — decided here, from
    /// the block, so the preview's Input row and the marker cannot disagree.
    pub input_set_by_art: bool,
}

/// Disk commands (not shell-internal ones) the fixed scripts run. `List`,
/// `Sort`, `Delete`, `Copy`, `Rename`, `Version`, `Mount` and `Assign` are
/// files in `C/` on every release; a recipe that dropped one would break the
/// boot. `Rename` is what `20-aux` and `30-datatypes` use to swap a staged
/// `.new`/`_AUX` file into place (M1, final review — missing from this list
/// even though both scripts already ran it; a completeness gap in the guard,
/// not a live bug, since every real release ships `C:Rename` too).
/// `Wait` joined in phase 3: the step wrapper runs `C:Wait 3` before `C:Reboot` (design §3.4). `Reboot` is not here — it is an availability, not a requirement.
pub const NEEDED_COMMANDS: [&str; 9] = [
    "List", "Sort", "Delete", "Copy", "Rename", "Version", "Mount", "Assign", "Wait",
];

pub fn plan(request: &FirstBootRequest) -> CoreResult<FirstBootPlan> {
    let tree = &request.tree;
    if !tree.join("distribution.json").is_file() {
        return Err(CoreError::FirstBootNotATree { tree: tree.clone() });
    }
    let sequence = tree.join("S").join("Startup-Sequence");
    if !calls_user_startup(&sequence)? {
        return Err(CoreError::FirstBootHookUnreachable { file: sequence });
    }
    // The dispatcher sorts its run list with C:Sort, because List answers
    // in the directory's own order (measured reversed under WinUAE,
    // 2026-09-07). A tree without it would run steps in no order at all.
    for command in NEEDED_COMMANDS {
        if !tree.join("C").join(command).is_file() {
            return Err(CoreError::FirstBootNeedsCommand {
                command: command.to_string(),
            });
        }
    }

    let fat_mount = if tree.join("L").join("fat95").is_file() {
        FatMount::Available
    } else {
        FatMount::Unavailable { needs: "fat95" }
    };
    let reboot = if tree.join("C").join("Reboot").is_file() {
        RebootCommand::Available
    } else {
        RebootCommand::Unavailable {
            needs: REBOOT_PACKAGE,
        }
    };
    let input_set_by_art = keymap_block_present(tree)?;
    let wizard = if request.ask_prefs {
        Some(wizard_rows(tree, input_set_by_art))
    } else {
        None
    };

    let mut steps = Vec::new();
    let mut bytes_added = 0u64;
    for file in fixed_files() {
        bytes_added += file.text.len() as u64;
        if let Some(name) = file.tree_path.strip_prefix("S/FirstBoot/") {
            steps.push(PlannedStep {
                name: name.to_string(),
                tree_path: file.tree_path.to_string(),
                fixed: true,
            });
        }
    }
    if wizard.is_some() {
        bytes_added += WIZARD_FILE.text.len() as u64;
        steps.push(PlannedStep {
            name: WIZARD_STEP.to_string(),
            tree_path: WIZARD_PATH.to_string(),
            fixed: true,
        });
    }
    bytes_added += b"TRUE\n".len() as u64;
    if input_set_by_art {
        bytes_added += env::MARKER_VALUE.len() as u64;
    }

    Ok(FirstBootPlan {
        tree: tree.clone(),
        steps,
        fat_mount,
        user_startup_exists: tree.join("S").join("User-Startup").is_file(),
        already_written: tree.join(super::DISPATCHER_PATH).is_file(),
        bytes_added,
        reboot,
        wizard,
        input_set_by_art,
    })
}

/// AmigaDOS is case-insensitive and so is this check. A missing file is a
/// tree with no sequence at all, which is the same refusal.
fn calls_user_startup(sequence: &Path) -> CoreResult<bool> {
    let bytes = match std::fs::read(sequence) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err.into()),
    };
    let text = bytes.to_ascii_lowercase();
    Ok(text
        .windows(b"execute s:user-startup".len())
        .any(|w| w == b"execute s:user-startup"))
}

/// Whether `S/User-Startup` carries a well-formed `keymap-selection` block —
/// through `core::osinstall::startup`'s own pairing, never a substring search,
/// so a stray `;BEGIN` is no keymap (design §4.3). Read Latin-1, the way
/// `write` reads the same file.
fn keymap_block_present(tree: &Path) -> CoreResult<bool> {
    let bytes = match std::fs::read(tree.join("S").join("User-Startup")) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err.into()),
    };
    let text: String = bytes.iter().map(|&b| b as char).collect();
    Ok(has_block(&text, KEYMAP_SELECTION))
}

/// The rows `90-prefs` will act on, decided in the script's own order: a
/// window ART set is not asked whether or not its editor exists; otherwise an
/// editor that is there is asked, and one that is not is missing.
fn wizard_rows(tree: &Path, input_set_by_art: bool) -> WizardPlan {
    let rows = WizardWindow::ALL
        .iter()
        .map(|&window| {
            let set_by_art = match window {
                WizardWindow::Locale => false,
                WizardWindow::Input => input_set_by_art,
                WizardWindow::ScreenMode => tree.join(env::ART_SET_SCREENMODE_PATH).is_file(),
            };
            let state = if set_by_art {
                WindowState::SetByArt
            } else if tree.join("Prefs").join(window.amiga_name()).is_file() {
                WindowState::Ask
            } else {
                WindowState::Missing
            };
            WizardRow { window, state }
        })
        .collect();
    WizardPlan { rows }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::amigaprefs::env;
    use crate::core::firstboot::WizardWindow;
    use crate::core::ScratchDir;
    use std::fs;

    /// The smallest thing `plan` accepts: a distribution.json and a
    /// Startup-Sequence that calls User-Startup, the way both real ones do.
    fn tree(tag: &str) -> ScratchDir {
        let d = ScratchDir::new("art-firstboot-plan", tag);
        fs::create_dir_all(d.join("S")).unwrap();
        fs::create_dir_all(d.join("L")).unwrap();
        fs::write(d.join("distribution.json"), b"{}").unwrap();
        fs::write(
            d.join("S/Startup-Sequence"),
            b"C:SetPatch QUIET\nBindDrivers\nIF EXISTS S:User-Startup\n  Execute S:User-Startup\nENDIF\nC:LoadWB\nEndCLI >NIL:\n",
        )
        .unwrap();
        fs::create_dir_all(d.join("C")).unwrap();
        for command in NEEDED_COMMANDS {
            fs::write(d.join("C").join(command), b"\x00\x00\x03\xf3").unwrap();
        }
        d
    }

    /// The refusal names the command, so one copied file fixes it.
    #[test]
    fn a_tree_without_c_sort_is_refused_by_name() {
        let d = tree("nosort");
        fs::remove_file(d.join("C/Sort")).unwrap();
        let err = plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
            ask_prefs: false,
        })
        .unwrap_err();
        match err {
            CoreError::FirstBootNeedsCommand { command } => assert_eq!(command, "Sort"),
            other => panic!("wrong refusal: {other:?}"),
        }
    }

    /// **M1, final review.** `20-aux` and `30-datatypes` both run `Rename`
    /// (swapping a staged `.new`/`_AUX` file into place); a tree missing it
    /// must be refused by name the same way a missing `Sort` is, not left to
    /// fail partway through a real boot.
    #[test]
    fn a_tree_without_c_rename_is_refused_by_name() {
        let d = tree("norename");
        fs::remove_file(d.join("C/Rename")).unwrap();
        let err = plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
            ask_prefs: false,
        })
        .unwrap_err();
        match err {
            CoreError::FirstBootNeedsCommand { command } => assert_eq!(command, "Rename"),
            other => panic!("wrong refusal: {other:?}"),
        }
    }

    /// Phase 3: the step wrapper waits with `C:Wait 3` before `C:Reboot`.
    #[test]
    fn a_tree_without_c_wait_is_refused_by_name() {
        let d = tree("nowait");
        fs::remove_file(d.join("C/Wait")).unwrap();
        let err = plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
            ask_prefs: false,
        })
        .unwrap_err();
        match err {
            CoreError::FirstBootNeedsCommand { command } => assert_eq!(command, "Wait"),
            other => panic!("wrong refusal: {other:?}"),
        }
    }

    #[test]
    fn a_plain_tree_plans_three_fixed_steps_and_no_fat_mount() {
        let d = tree("plain");
        let p = plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
            ask_prefs: false,
        })
        .unwrap();
        let names: Vec<&str> = p.steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["10-hardware", "20-aux", "30-datatypes"]);
        assert_eq!(p.fat_mount, FatMount::Unavailable { needs: "fat95" });
        assert!(!p.user_startup_exists);
        assert!(!p.already_written);
        assert!(p.bytes_added > 0);
        assert!(p.wizard.is_none(), "not asked, no wizard");
        assert_eq!(p.reboot, RebootCommand::Unavailable { needs: "reboot" });
        assert!(!p.input_set_by_art);
    }

    #[test]
    fn fat95_in_l_makes_the_fat_mount_available() {
        let d = tree("fat95");
        fs::write(d.join("L/fat95"), b"x").unwrap();
        let p = plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
            ask_prefs: false,
        })
        .unwrap();
        assert_eq!(p.fat_mount, FatMount::Available);
    }

    #[test]
    fn a_startup_sequence_that_never_calls_user_startup_is_refused_by_name() {
        let d = tree("nohook");
        fs::write(
            d.join("S/Startup-Sequence"),
            b"C:SetPatch QUIET\nC:LoadWB\nEndCLI >NIL:\n",
        )
        .unwrap();
        let err = plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
            ask_prefs: false,
        })
        .unwrap_err();
        match err {
            CoreError::FirstBootHookUnreachable { file } => {
                assert!(file.ends_with("Startup-Sequence"));
            }
            other => panic!("wrong refusal: {other:?}"),
        }
    }

    #[test]
    fn the_hook_check_is_case_insensitive_like_amigados() {
        let d = tree("case");
        fs::write(
            d.join("S/Startup-Sequence"),
            b"if exists s:user-startup\n  execute s:user-startup\nendif\n",
        )
        .unwrap();
        assert!(plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
            ask_prefs: false,
        })
        .is_ok());
    }

    #[test]
    fn a_folder_without_distribution_json_is_not_a_tree() {
        let d = ScratchDir::new("art-firstboot-plan", "notree");
        let err = plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
            ask_prefs: false,
        })
        .unwrap_err();
        assert!(matches!(err, CoreError::FirstBootNotATree { .. }));
    }

    #[test]
    fn an_existing_dispatcher_marks_the_plan_as_already_written() {
        let d = tree("again");
        fs::write(d.join("S/ART-FirstBoot"), b"x").unwrap();
        let p = plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
            ask_prefs: false,
        })
        .unwrap();
        assert!(p.already_written);
    }

    fn asked(d: &ScratchDir) -> FirstBootPlan {
        plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
            ask_prefs: true,
        })
        .unwrap()
    }

    fn editors(d: &ScratchDir, names: &[&str]) {
        fs::create_dir_all(d.join("Prefs")).unwrap();
        for name in names {
            fs::write(d.join("Prefs").join(name), b"\x00\x00\x03\xf3").unwrap();
        }
    }

    fn rows(p: &FirstBootPlan) -> Vec<(WizardWindow, WindowState)> {
        p.wizard
            .as_ref()
            .expect("asked, so planned")
            .rows
            .iter()
            .map(|r| (r.window, r.state))
            .collect()
    }

    #[test]
    fn asking_plans_the_wizard_as_the_last_step_and_counts_its_bytes() {
        let d = tree("ask");
        editors(&d, &["Locale", "Input", "ScreenMode"]);
        let off = plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
            ask_prefs: false,
        })
        .unwrap();
        let on = asked(&d);
        let names: Vec<&str> = on.steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["10-hardware", "20-aux", "30-datatypes", "90-prefs"]);
        assert_eq!(on.steps[3].tree_path, "S/FirstBoot/90-prefs");
        assert_eq!(
            on.bytes_added - off.bytes_added,
            super::super::scripts::STEP_90_PREFS.len() as u64
        );
    }

    #[test]
    fn every_editor_present_and_nothing_set_asks_all_three() {
        let d = tree("all-asked");
        editors(&d, &["Locale", "Input", "ScreenMode"]);
        assert_eq!(
            rows(&asked(&d)),
            [
                (WizardWindow::Locale, WindowState::Ask),
                (WizardWindow::Input, WindowState::Ask),
                (WizardWindow::ScreenMode, WindowState::Ask),
            ]
        );
    }

    /// Design §4.3's own case: a tree ART built before phase 3 carries the
    /// block and no marker. The preview must say what the write is about to
    /// make true, not what the drawer holds this second.
    #[test]
    fn a_keymap_block_is_input_set_by_art_before_any_marker_exists() {
        let d = tree("keymap");
        editors(&d, &["Locale", "Input", "ScreenMode"]);
        fs::write(
            d.join("S/User-Startup"),
            b"; mine\n;BEGIN keymap-selection\nSetKeyboard usa\n;END keymap-selection\n",
        )
        .unwrap();
        let p = asked(&d);
        assert!(p.input_set_by_art);
        assert!(!d.join(env::ART_SET_INPUT_PATH).exists());
        assert_eq!(rows(&p)[1], (WizardWindow::Input, WindowState::SetByArt));

        // The marker's four bytes are counted only while the block is there.
        let with_block = p.bytes_added;
        fs::write(d.join("S/User-Startup"), b"; mine\n").unwrap();
        let without_block = asked(&d).bytes_added;
        assert_eq!(with_block - without_block, env::MARKER_VALUE.len() as u64);
    }

    #[test]
    fn a_stray_keymap_opener_is_not_a_keymap_art_set() {
        let d = tree("stray");
        editors(&d, &["Locale", "Input", "ScreenMode"]);
        fs::write(
            d.join("S/User-Startup"),
            b";BEGIN keymap-selection\nSetKeyboard usa\n",
        )
        .unwrap();
        let p = asked(&d);
        assert!(!p.input_set_by_art);
        assert_eq!(rows(&p)[1], (WizardWindow::Input, WindowState::Ask));
    }

    /// The script asks the marker before the editor; so does the plan.
    #[test]
    fn a_set_window_is_set_whether_or_not_its_editor_exists_and_a_missing_one_is_missing() {
        let d = tree("mixed");
        editors(&d, &["Locale"]);
        fs::create_dir_all(d.join("Prefs/Env-Archive")).unwrap();
        fs::write(d.join(env::ART_SET_SCREENMODE_PATH), b"TRUE").unwrap();
        assert_eq!(
            rows(&asked(&d)),
            [
                (WizardWindow::Locale, WindowState::Ask),
                (WizardWindow::Input, WindowState::Missing),
                (WizardWindow::ScreenMode, WindowState::SetByArt),
            ]
        );
    }

    #[test]
    fn c_reboot_in_the_tree_makes_the_reboot_available() {
        let d = tree("reboot");
        fs::write(d.join("C/Reboot"), b"\x00\x00\x03\xf3").unwrap();
        assert_eq!(asked(&d).reboot, RebootCommand::Available);
    }

    /// The package the preview names is the catalogue's own id, read from the
    /// file rather than trusted from a copy (CLAUDE.md, "a test that reads a
    /// table instead of the file is a copy").
    #[test]
    fn the_reboot_package_is_the_catalogues_aminet_reboot() {
        let json: serde_json::Value =
            serde_json::from_str(include_str!("../sources/bundle/catalogue/acilis.json")).unwrap();
        let entry = json["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == REBOOT_PACKAGE)
            .expect("the catalogue carries the reboot package");
        assert_eq!(entry["source"]["aminet"]["path"], "util/boot/reboot");
    }
}
