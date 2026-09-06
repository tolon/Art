//! First boot: thin adapters over `core::firstboot`.

use std::path::PathBuf;

use tauri::State;

use super::oplog::{user_operation, write_result};
use crate::core::firstboot::plan::{plan, FirstBootPlan, FirstBootRequest};
use crate::core::firstboot::write::{write, Written};
use crate::core::oplog::JsonlOperationLog;
use crate::error::{AppError, AppResult};

/// §92 PREVIEW: what a first boot would run. Writes nothing.
#[tauri::command]
pub fn firstboot_preview(tree: String) -> AppResult<FirstBootPlan> {
    Ok(plan(&FirstBootRequest {
        tree: PathBuf::from(tree.trim()),
    })?)
}

/// §92 APPLY: put the files into the tree. `Safe` — nothing of the user's is
/// overwritten except `S/User-Startup`, which is merged and backed up first.
#[tauri::command]
pub fn firstboot_write(tree: String, oplog: State<'_, JsonlOperationLog>) -> AppResult<Written> {
    let request = FirstBootRequest {
        tree: PathBuf::from(tree.trim()),
    };
    let result = plan(&request)
        .and_then(|p| write(&p))
        .map_err(AppError::from);
    write_result(
        &oplog,
        user_operation("Write the first-boot files into a tree").destination(&tree),
        &result,
        |record, done: &Written| {
            let record = record.detail("Files written", done.files.join(", "));
            match &done.user_startup_backup {
                Some(backup) => record.detail("User-Startup backup", backup.display().to_string()),
                None => record,
            }
        },
    );
    result
}

#[cfg(test)]
mod tests {
    use crate::core::firstboot::plan::{FatMount, FirstBootPlan, PlannedStep};

    #[test]
    fn the_plan_crosses_the_wire_in_camel_case_with_kebab_tags() {
        let p = FirstBootPlan {
            tree: "x".into(),
            steps: vec![PlannedStep {
                name: "10-hardware".into(),
                tree_path: "S/FirstBoot/10-hardware".into(),
                fixed: true,
            }],
            fat_mount: FatMount::Unavailable { needs: "fat95" },
            user_startup_exists: false,
            already_written: false,
            bytes_added: 1,
        };
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["fatMount"]["kind"], "unavailable");
        assert_eq!(json["fatMount"]["needs"], "fat95");
        assert_eq!(json["userStartupExists"], false);
        assert_eq!(json["alreadyWritten"], false);
        assert_eq!(json["bytesAdded"], 1);
        assert_eq!(json["steps"][0]["treePath"], "S/FirstBoot/10-hardware");
    }
}
