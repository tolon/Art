//! Formatting a card's Amiga volumes and filling them (SD-2 · G3, route E).
//!
//! Three commands and one rule: **nothing is formatted until the user has seen
//! what would be** (§92). `preload_plan` writes nothing and answers what would
//! run; `preload_run` does it as a job.
//!
//! Formatting is `Destructive` — a partition that had something in it does not
//! afterwards. There is no backup step and there deliberately is not one: the
//! volumes this is aimed at are tens of gigabytes and ART's own build made
//! them empty a moment ago. The guard is the preview and the confirmation, and
//! the plan says how many partitions it would erase.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::core::preload::VolumeFormatter;
use crate::core::preload::{plan, run, PreloadOutcome, PreloadPlan, PreloadRequest, ToolVersion};
use crate::error::AppResult;
use crate::tools::hst_imager::{HstImager, TESTED_VERSION};

use super::jobs::{spawn_job, JobRegistry};
use super::oplog::{user_operation, write_to_path};

/// What the configured tool is, and whether it is the one ART was written
/// against.
#[derive(Debug, Clone, Serialize)]
pub struct FormatterReport {
    pub version: ToolVersion,
    /// False when the version is not [`TESTED_VERSION`]. **Not a refusal** —
    /// ART's command set came from that version's own scripts, and saying so
    /// beats turning away a tool that would have worked.
    pub is_tested_version: bool,
    pub tested_version: String,
}

/// Ask the configured formatter what it is. Runs it with `--version` and
/// nothing else.
#[tauri::command]
pub fn preload_probe(tool_path: String) -> AppResult<FormatterReport> {
    let version = HstImager::at(tool_path.trim()).probe()?;
    Ok(FormatterReport {
        is_tested_version: version.raw.contains(TESTED_VERSION),
        version,
        tested_version: TESTED_VERSION.to_string(),
    })
}

/// The request a screen sends, with where the tool is.
#[derive(Debug, Clone, Deserialize)]
pub struct PreloadCommand {
    #[serde(flatten)]
    pub request: PreloadRequest,
    /// Where `hst.imager` is. ART does not ship it — 137 MB beside a Tauri
    /// app is not a trade worth making, and SD-0 said as much.
    pub tool_path: String,
}

/// What would be run. Writes nothing (§92's PREVIEW).
#[tauri::command]
pub fn preload_plan(command: PreloadCommand) -> AppResult<PreloadPlan> {
    Ok(plan(&command.request)?)
}

/// The event a finished preload arrives on.
pub const PRELOAD_EVENT: &str = "preload-result";

#[derive(Debug, Clone, Serialize)]
pub struct PreloadResult {
    pub job_id: u64,
    pub image: String,
    pub outcome: PreloadOutcome,
}

/// Format the partitions and copy the content in. Returns a job id (§54).
///
/// The plan is recomputed here rather than taken from the caller: a screen
/// that previewed one thing must not be able to run another, and the card may
/// have changed since.
#[tauri::command]
pub fn preload_run(
    command: PreloadCommand,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<u64> {
    let image = command.request.image.display().to_string();
    let formatter = HstImager::at(command.tool_path.trim());

    // Refuse here, on the command thread, rather than inside the job: a bad
    // partition number is not something to discover after the first format.
    let made = plan(&command.request)?;

    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!("Preparing {} volume(s) on {image}", made.formats());
    let for_log = image.clone();

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let outcome = run(&made, &formatter, progress);

        // §53. A format destroys what was there, so what was formatted and
        // with which tool are the two things the log has to carry.
        let record = user_operation("Format and fill Amiga volumes")
            .source(&for_log)
            .destination(&for_log)
            .detail("Partitions formatted", made.formats().to_string());
        let record = match &outcome {
            Ok(done) => record
                .detail("Volumes", done.formatted.join(", "))
                .detail("Files copied", done.copied.files.to_string())
                .detail(
                    "Tool",
                    done.tool
                        .as_ref()
                        .map(|t| t.raw.clone())
                        .unwrap_or_else(|| "unknown".into()),
                )
                // **Not verified, and it says so.** ART has no PFS3 reader, so
                // the files inside the volume cannot be read back. Claiming
                // verification here would be the one thing §89 forbids.
                .outcome(OperationOutcome::verified(false)),
            Err(err) => record.failure(err.code(), err.to_string()),
        };
        write_to_path(&log_path, &record);

        let outcome = outcome?;
        let _ = emit_app.emit(
            PRELOAD_EVENT,
            PreloadResult {
                job_id,
                image: for_log,
                outcome,
            },
        );
        Ok(())
    });

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::preload::{PreloadPartition, PreloadStep};

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("art-preload-cmd-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// **The wire, written down.** `src/lib/preload.ts` builds this object by
    /// hand — the request's own fields spread flat, with `tool_path` beside
    /// them — and `#[serde(flatten)]` is the only thing making the two agree.
    /// Nothing else in the build checks it: a renamed field would compile on
    /// both sides and fail at the moment the user pressed Preview, which is
    /// the worst place to find out.
    #[test]
    fn the_payload_the_frontend_sends_deserialises() {
        let command: PreloadCommand = serde_json::from_str(
            r#"{"image":"E:\\amiga\\ProjeART\\card.img",
                "driver":null,
                "partitions":[{"area":1,"index":1,"volume_name":"Work","content":null}],
                "tool_path":"E:\\amiga\\hstimager\\hst.imager.exe"}"#,
        )
        .expect("the shape src/lib/preload.ts sends");

        assert_eq!(command.tool_path, "E:\\amiga\\hstimager\\hst.imager.exe");
        assert_eq!(command.request.driver, None);
        assert_eq!(command.request.partitions.len(), 1);
        assert_eq!(command.request.partitions[0].area, 1);
        assert_eq!(command.request.partitions[0].volume_name, "Work");
        assert_eq!(command.request.partitions[0].content, None);

        // And the two the screen fills in when the user does: a driver to
        // embed, and a folder whose tree goes in.
        let filled: PreloadCommand = serde_json::from_str(
            r#"{"image":"card.img",
                "driver":"E:\\amiga\\pfs3aio.lha",
                "partitions":[{"area":2,"index":3,"volume_name":"Games","content":"E:\\tree"}],
                "tool_path":"hst.imager.exe"}"#,
        )
        .expect("the same shape with every optional filled");

        assert_eq!(
            filled.request.driver,
            Some(std::path::PathBuf::from("E:\\amiga\\pfs3aio.lha"))
        );
        assert_eq!(filled.request.partitions[0].index, 3);
        assert_eq!(
            filled.request.partitions[0].content,
            Some(std::path::PathBuf::from("E:\\tree"))
        );
    }

    /// The adapter is thin, and this is what it must not get wrong: the plan
    /// the screen is shown comes from the card, through the same `plan` the
    /// run recomputes.
    #[test]
    fn the_plan_command_answers_from_the_card() {
        use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

        let dir = scratch("plan");
        let image = dir.join("card.hdf");
        crate::core::hdf::create_hdf(
            &image,
            32 * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::FfsStandard,
                size_mb: 10,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[],
        )
        .unwrap();

        let made = preload_plan(PreloadCommand {
            request: PreloadRequest {
                image: image.clone(),
                driver: None,
                partitions: vec![PreloadPartition {
                    area: 1,
                    index: 1,
                    volume_name: "Work".into(),
                    content: None,
                }],
            },
            tool_path: "hst.imager".into(),
        })
        .unwrap();

        assert_eq!(
            made.steps,
            vec![PreloadStep::FormatPartition {
                slot: None,
                index: 1,
                drive_name: "DH0".into(),
                volume_name: "Work".into(),
            }]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The real tool, on a real card, when asked.
    ///
    /// ```text
    /// ART_HST=E:\amiga\Amigatolon\hstimager\hst.imager.exe \
    ///   cargo test preload_a_real_card_when_asked -- --nocapture
    /// ```
    ///
    /// `ART_PFS3` may name a `pfs3aio.lha` to embed first; without it the card
    /// is built with FFS, which Kickstart carries.
    #[test]
    fn preload_a_real_card_when_asked() {
        use crate::core::card::build::{build_card, AreaSpec, CardSpec};
        use crate::core::jobs::NoProgress;
        use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

        let Ok(tool) = std::env::var("ART_HST") else {
            return;
        };
        let dir = std::path::PathBuf::from(
            std::env::var("ART_SCRATCH")
                .unwrap_or_else(|_| std::env::temp_dir().display().to_string()),
        )
        .join("art-preload-real");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let image = dir.join("preload.img");

        let driver = std::env::var("ART_PFS3").ok().map(std::path::PathBuf::from);
        let fs = match driver {
            Some(_) => AmigaHardDiskFs::Pfs3DirectScsi,
            None => AmigaHardDiskFs::FfsStandard,
        };

        build_card(
            &image,
            &CardSpec {
                total_bytes: 2 * 1024 * 1024 * 1024,
                boot_bytes: 0,
                label: "ART CARD".into(),
                boot_files: Vec::new(),
                areas: vec![AreaSpec {
                    size_bytes: 0,
                    partitions: vec![PartitionSpec {
                        drive_name: "DH0".into(),
                        fs_type: fs,
                        size_mb: 512,
                        bootable: true,
                        boot_priority: 0,
                        num_buffers: 0,
                    }],
                    file_systems: Vec::new(),
                }],
            },
            &NoProgress,
        )
        .unwrap();

        // Something to copy in, so the whole chain is exercised.
        let tree = dir.join("tree");
        std::fs::create_dir_all(tree.join("S")).unwrap();
        std::fs::write(tree.join("S").join("Startup-Sequence"), b"echo hello\n").unwrap();
        std::fs::write(tree.join("Readme"), b"from ART\n").unwrap();

        let request = PreloadRequest {
            image: image.clone(),
            driver: std::env::var("ART_PFS3").ok().map(std::path::PathBuf::from),
            partitions: vec![PreloadPartition {
                area: 1,
                index: 1,
                volume_name: "Work".into(),
                content: Some(tree),
            }],
        };

        let made = plan(&request).unwrap();
        for step in &made.steps {
            println!("  step: {step:?}");
        }

        let formatter = HstImager::at(&tool);
        println!("tool: {:?}", formatter.probe());

        match run(&made, &formatter, &NoProgress) {
            Ok(outcome) => println!("outcome: {outcome:?}"),
            Err(err) => panic!("preload failed: {err}"),
        }
    }
}
