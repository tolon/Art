//! Deciding what first boot will run — every refusal here, before a byte.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::scripts::fixed_files;
use crate::core::error::{CoreError, CoreResult};

#[derive(Debug, Clone)]
pub struct FirstBootRequest {
    pub tree: PathBuf,
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
}

pub fn plan(request: &FirstBootRequest) -> CoreResult<FirstBootPlan> {
    let tree = &request.tree;
    if !tree.join("distribution.json").is_file() {
        return Err(CoreError::FirstBootNotATree { tree: tree.clone() });
    }
    let sequence = tree.join("S").join("Startup-Sequence");
    if !calls_user_startup(&sequence)? {
        return Err(CoreError::FirstBootHookUnreachable { file: sequence });
    }

    let fat_mount = if tree.join("L").join("fat95").is_file() {
        FatMount::Available
    } else {
        FatMount::Unavailable { needs: "fat95" }
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
    bytes_added += b"TRUE\n".len() as u64;

    Ok(FirstBootPlan {
        tree: tree.clone(),
        steps,
        fat_mount,
        user_startup_exists: tree.join("S").join("User-Startup").is_file(),
        already_written: tree.join(super::DISPATCHER_PATH).is_file(),
        bytes_added,
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

#[cfg(test)]
mod tests {
    use super::*;
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
        d
    }

    #[test]
    fn a_plain_tree_plans_three_fixed_steps_and_no_fat_mount() {
        let d = tree("plain");
        let p = plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
        })
        .unwrap();
        let names: Vec<&str> = p.steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["10-hardware", "20-aux", "30-datatypes"]);
        assert_eq!(p.fat_mount, FatMount::Unavailable { needs: "fat95" });
        assert!(!p.user_startup_exists);
        assert!(!p.already_written);
        assert!(p.bytes_added > 0);
    }

    #[test]
    fn fat95_in_l_makes_the_fat_mount_available() {
        let d = tree("fat95");
        fs::write(d.join("L/fat95"), b"x").unwrap();
        let p = plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
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
            tree: d.path().to_path_buf()
        })
        .is_ok());
    }

    #[test]
    fn a_folder_without_distribution_json_is_not_a_tree() {
        let d = ScratchDir::new("art-firstboot-plan", "notree");
        let err = plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
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
        })
        .unwrap();
        assert!(p.already_written);
    }
}
