//! The one-button card (ART-340): one session, `.partial`, every ending said.
//!
//! Four commands the card screen runs in order (design § 6, P1):
//!
//! 1. [`card_os_open`] — a session folder under the scratch root, with the
//!    `tree/` the OS Builder fills, `staging/` for unpacked sources and
//!    `driver/` for a PFS3 driver out of an archive.
//! 2. [`card_os_prepare`] — a job: the card measured, free space checked,
//!    sources staged, WHDLoad chosen and Kickstarts **proposed**
//!    (`core::cardos::prepare`). Nothing is written outside the session.
//! 3. [`card_os_build`] — a job: [`build_card_os`] writes the card under
//!    `<image>.partial`, formats and fills every partition, checks the
//!    result, and only then gives it its name and writes its manifest.
//! 4. [`card_os_close`] — the session removed, when the screen gives up
//!    before building.
//!
//! **No single stack frame spans these four**, so the session folder is an
//! [`OwnedScratch`] a registry in Tauri `State` holds: every ending of a
//! build removes it, `card_os_close` removes it, and what could not be
//! removed is named rather than claimed gone.
//!
//! **Endings stay distinct.** A build succeeds, fails in a named phase, or is
//! stopped in a named phase — [`CardOsEnding`]. A stop is never reported as a
//! failure, and every ending says what became of `<image>.partial`
//! ([`PartialRemoval`]): ART removes only the file this run created (P3).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::commands::card::{write_card_image, CardBuildRequest};
use crate::commands::hdf::FileSystemInput;
use crate::commands::preload::{run_with_fallback, StepReport};
use crate::core::card::health::{check_image, HealthReport};
use crate::core::card::manifest::{
    describe_card, manifest_path_for, render_manifest, PartitionContent,
};
use crate::core::cardos::kickstarts::{place_agreed, PlacedKickstart};
use crate::core::cardos::partial::{
    finish_partial, partial_path_for, refuse_partial_destination, remove_partial, PartialRemoval,
};
use crate::core::cardos::prepare::{
    check_free_space, measure_card, space_needs, stage_card, PartitionInput, PreparedCard,
};
use crate::core::cardos::whdload::{install_whdload, WhdloadInstalled};
use crate::core::clock::AmigaClock;
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::{JobId, JobTitle, ProgressSink};
use crate::core::oplog::{JsonlOperationLog, OperationOutcome, OperationRecord};
use crate::core::osinstall::apply::{DistributionManifest, MANIFEST_FILE_NAME};
use crate::core::pistorm::firmware::FirmwareConfig;
use crate::core::pistorm::hardware::{Emu68Line, PiModel, PistormHardware};
use crate::core::pistorm::options::Emu68Options;
use crate::core::preload::native::NativeFormatter;
use crate::core::preload::{plan, PreloadPartition, PreloadRequest, PreloadStep, VolumeFormatter};
use crate::core::safety::atomic::atomic_write;
use crate::core::scratch_guard::{LeftBehind, OwnedScratch};
use crate::error::AppResult;
use crate::tools::hst_imager::HstImager;

use super::jobs::{spawn_job, JobRegistry};
use super::oplog::{user_operation, write_to_path};

/// The session's folders, inside its [`OwnedScratch`].
const TREE: &str = "tree";
const STAGING: &str = "staging";
const DRIVER: &str = "driver";

/// What the manifest names System's one source as — never a host path.
const SYSTEM_TREE_SOURCE: &str = "system tree";

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

/// One card being put together: its folder, what `card_os_prepare` found,
/// and whether a job is using it right now.
pub struct CardOsSession {
    scratch: OwnedScratch,
    prepared: Option<PreparedCard>,
    /// The image the preparation was asked for — its free space was checked
    /// for this path, so the build writes exactly here.
    image: Option<PathBuf>,
    busy: bool,
}

/// Every open session, by id.
#[derive(Default)]
pub struct CardOsSessions(Mutex<HashMap<u64, CardOsSession>>);

/// Process-wide, so two sessions never share an id.
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

/// What a build takes out of its session.
pub(crate) struct BuildTicket {
    pub prepared: PreparedCard,
    pub image: PathBuf,
    pub tree: PathBuf,
}

fn no_session(id: u64) -> CoreError {
    CoreError::InvalidInput(format!("no card session {id} is open"))
}

fn session_busy(id: u64) -> CoreError {
    CoreError::InvalidInput(format!(
        "card session {id} is busy — a preparation or a build is still running; wait for it to \
         end or stop it first"
    ))
}

impl CardOsSessions {
    fn lock(&self) -> MutexGuard<'_, HashMap<u64, CardOsSession>> {
        // A panic while the lock was held leaves the map itself intact.
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// A new session folder under `root`, with `tree/`, `staging/` and
    /// `driver/` inside it. Returns the id and the tree.
    pub(crate) fn open_in(&self, root: &Path) -> CoreResult<(u64, PathBuf)> {
        let scratch = OwnedScratch::create_in(root, "card-os")?;
        for folder in [TREE, STAGING, DRIVER] {
            // On an error `scratch` drops here and removes what was made.
            std::fs::create_dir_all(scratch.path().join(folder))?;
        }
        let tree = scratch.path().join(TREE);
        let id = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
        self.lock().insert(
            id,
            CardOsSession {
                scratch,
                prepared: None,
                image: None,
                busy: false,
            },
        );
        Ok((id, tree))
    }

    /// Mark the session busy for a preparation; its folder.
    pub(crate) fn begin_prepare(&self, id: u64) -> CoreResult<PathBuf> {
        let mut sessions = self.lock();
        let session = sessions.get_mut(&id).ok_or_else(|| no_session(id))?;
        if session.busy {
            return Err(session_busy(id));
        }
        session.busy = true;
        Ok(session.scratch.path().to_path_buf())
    }

    /// A preparation ended. Its staging folder was emptied when it began, so
    /// a failed one leaves nothing an earlier preparation could still use:
    /// `None` clears what was there.
    pub(crate) fn end_prepare(&self, id: u64, result: Option<(PreparedCard, PathBuf)>) {
        let mut sessions = self.lock();
        if let Some(session) = sessions.get_mut(&id) {
            session.busy = false;
            match result {
                Some((prepared, image)) => {
                    session.prepared = Some(prepared);
                    session.image = Some(image);
                }
                None => {
                    session.prepared = None;
                    session.image = None;
                }
            }
        }
    }

    /// Take the prepared card out for a build, and mark the session busy
    /// until [`end_build`](Self::end_build).
    pub(crate) fn take_for_build(&self, id: u64) -> CoreResult<BuildTicket> {
        let mut sessions = self.lock();
        let session = sessions.get_mut(&id).ok_or_else(|| no_session(id))?;
        if session.busy {
            return Err(session_busy(id));
        }
        let (Some(prepared), Some(image)) = (session.prepared.take(), session.image.take()) else {
            return Err(CoreError::InvalidInput(format!(
                "card session {id} has not been prepared — prepare the card before building it"
            )));
        };
        session.busy = true;
        Ok(BuildTicket {
            prepared,
            image,
            tree: session.scratch.path().join(TREE),
        })
    }

    /// A build ended, however it ended: the session is removed, and its
    /// folder with it. `Some` names the folder when it could not be removed.
    pub(crate) fn end_build(&self, id: u64) -> Option<LeftBehind> {
        let removed = self.lock().remove(&id);
        removed.and_then(|session| session.scratch.finish().err())
    }

    /// Close a session the screen gave up on. Refused while a job uses it.
    pub(crate) fn close(&self, id: u64) -> CoreResult<Option<LeftBehind>> {
        let session = {
            let mut sessions = self.lock();
            match sessions.get(&id) {
                None => return Err(no_session(id)),
                Some(session) if session.busy => return Err(session_busy(id)),
                Some(_) => sessions.remove(&id),
            }
        };
        // Removed outside the lock: a staging folder can be large.
        Ok(session.and_then(|session| session.scratch.finish().err()))
    }
}

/// A new card session.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardOsOpened {
    pub session: u64,
    /// The folder the OS Builder builds System's tree into.
    pub tree: String,
}

/// Open a card session under the scratch root.
#[tauri::command]
pub fn card_os_open(sessions: State<'_, Arc<CardOsSessions>>) -> AppResult<CardOsOpened> {
    let root = crate::scratch::root()?;
    let (session, tree) = sessions.open_in(&root)?;
    Ok(CardOsOpened {
        session,
        tree: tree.display().to_string(),
    })
}

// ---------------------------------------------------------------------------
// Prepare
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardOsPrepareRequest {
    pub session: u64,
    pub card_gb: u32,
    pub image: String,
    pub partitions: Vec<PartitionInput>,
    pub material: Vec<String>,
    #[serde(default)]
    pub pfs3_driver: Option<String>,
    /// The card's boot Kickstart; its folder joins the Kickstart collection (P4).
    #[serde(default)]
    pub kickstart: Option<String>,
    #[serde(default)]
    pub hst_imager_path: String,
}

/// The event a finished preparation arrives on.
pub const CARD_OS_PREPARE_EVENT: &str = "card-os-prepare-result";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardOsPrepareResult {
    pub job_id: JobId,
    pub session: u64,
    pub prepared: PreparedCard,
}

/// Empty one of the session's own folders: its contents are ART's, from this
/// session.
fn empty_session_folder(folder: &Path) -> CoreResult<()> {
    if folder.exists() {
        std::fs::remove_dir_all(folder)?;
    }
    std::fs::create_dir_all(folder)?;
    Ok(())
}

/// A path the screen sent, trimmed; `None` when blank.
fn given_path(given: Option<&str>) -> Option<PathBuf> {
    given
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

/// The preparation, without Tauri: measure, check free space, stage.
fn prepare_card(
    request: &CardOsPrepareRequest,
    image: &Path,
    session_root: &Path,
    clock: &dyn AmigaClock,
    available: impl Fn(&Path) -> std::io::Result<u64>,
    progress: &dyn ProgressSink,
) -> CoreResult<PreparedCard> {
    let tree = session_root.join(TREE);
    let staging = session_root.join(STAGING);
    let driver_stage = session_root.join(DRIVER);
    // A second preparation starts from empty folders: what an earlier one
    // unpacked there is this session's own, and would otherwise be mixed in.
    empty_session_folder(&staging)?;
    empty_session_folder(&driver_stage)?;

    let material: Vec<PathBuf> = request
        .material
        .iter()
        .filter_map(|m| given_path(Some(m)))
        .collect();
    let explicit_driver = given_path(request.pfs3_driver.as_deref());

    let card = measure_card(
        request.card_gb,
        &tree,
        &request.partitions,
        &material,
        explicit_driver.as_deref(),
        &driver_stage,
        !request.hst_imager_path.trim().is_empty(),
        clock,
        progress,
    )?;
    check_free_space(&space_needs(&card, image, session_root), available)?;

    // P4: every material folder, and the folder of the card's own Kickstart.
    let mut collection = material.clone();
    if let Some(parent) = given_path(request.kickstart.as_deref())
        .as_deref()
        .and_then(Path::parent)
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        collection.push(parent.to_path_buf());
    }

    stage_card(
        card,
        &tree,
        &staging,
        &material,
        &collection,
        clock,
        progress,
    )
}

/// The operation log's record for a preparation (§53).
fn prepare_record(
    request: &CardOsPrepareRequest,
    image: &Path,
    outcome: &CoreResult<PreparedCard>,
) -> OperationRecord {
    let record = user_operation("Prepare a card image")
        .destination(image.display().to_string())
        .detail("Card size", format!("{} GB", request.card_gb));
    match outcome {
        Ok(prepared) => {
            let mut record = record;
            for partition in &prepared.measured.partitions {
                record = record.detail(
                    format!("{} sources", partition.volume_name),
                    partition.sources.len().to_string(),
                );
            }
            record
                .detail(
                    "Kickstarts proposed",
                    prepared.kickstarts.items.len().to_string(),
                )
                .outcome(OperationOutcome::success())
        }
        Err(err) => record.failed(err),
    }
}

/// Prepare the session's card. Returns a job id; the result arrives on
/// [`CARD_OS_PREPARE_EVENT`], and a refusal ends the job with its code.
#[tauri::command]
pub fn card_os_prepare(
    request: CardOsPrepareRequest,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    sessions: State<'_, Arc<CardOsSessions>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<JobId> {
    let image = PathBuf::from(request.image.trim());
    refuse_partial_destination(&image)?;
    let session_root = sessions.begin_prepare(request.session)?;

    let sessions = Arc::clone(&sessions);
    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let target = image.display().to_string();
    let title = JobTitle::new("components.jobBar.title.prepareCardOs").text("target", &target);

    let id = spawn_job(&app, registry, title, move |job_id, progress| {
        let outcome = prepare_card(
            &request,
            &image,
            &session_root,
            &crate::tools::local_time::LOCAL_TIME,
            crate::tools::free_space::available_bytes,
            progress,
        );
        write_to_path(&log_path, &prepare_record(&request, &image, &outcome));

        match outcome {
            Ok(prepared) => {
                sessions.end_prepare(request.session, Some((prepared.clone(), image)));
                let _ = emit_app.emit(
                    CARD_OS_PREPARE_EVENT,
                    CardOsPrepareResult {
                        job_id,
                        session: request.session,
                        prepared,
                    },
                );
                Ok(())
            }
            Err(err) => {
                sessions.end_prepare(request.session, None);
                Err(err)
            }
        }
    });

    Ok(id)
}

// ---------------------------------------------------------------------------
// Build
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardOsBuildRequest {
    pub session: u64,
    /// The Kickstart names the user agreed to (decision 1). Never paths.
    pub agreed_kickstarts: Vec<String>,
    /// Everything `card_build` takes except `dest`, `partitions`, `file_systems`, `first_disk_bytes`,
    /// `extra_disks`, `total_bytes`, `card_gb` — those come from the prepared card.
    pub archive: String,
    pub kickstart: Option<String>,
    pub label: String,
    pub hardware: PistormHardware,
    pub line: Emu68Line,
    #[serde(default)]
    pub firmware: FirmwareConfig,
    #[serde(default)]
    pub options: Emu68Options,
    #[serde(default)]
    pub built_at: Option<String>,
    #[serde(default)]
    pub hst_imager_path: String,
}

/// The event a build's every ending arrives on.
pub const CARD_OS_BUILD_EVENT: &str = "card-os-build-result";

/// How a build ended. Three sentences, never collapsed into "not succeeded".
#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "ending",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum CardOsEnding {
    Succeeded,
    Failed {
        phase: BuildPhase,
        code: String,
        message: String,
    },
    Stopped {
        phase: BuildPhase,
    },
}

/// The build's phases, in order.
#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BuildPhase {
    Whdload,
    Card,
    Partitions,
    Check,
}

impl BuildPhase {
    fn name(self) -> &'static str {
        match self {
            Self::Whdload => "whdload",
            Self::Card => "card",
            Self::Partitions => "partitions",
            Self::Check => "check",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardOsBuildResult {
    pub job_id: JobId,
    pub session: u64,
    pub image: String,
    pub ending: CardOsEnding,
    pub whdload: Option<WhdloadInstalled>,
    pub kickstarts: Vec<PlacedKickstart>,
    pub steps: Vec<StepReport>,
    pub manifest_path: Option<String>,
    pub health: Option<HealthReport>,
    /// Always said: what happened to `<image>.partial`.
    pub partial: PartialRemoval,
    /// The session folder, when it could not be removed.
    pub scratch_left: Option<LeftBehind>,
}

/// What [`build_card_os`] did, whatever the ending.
#[derive(Debug)]
pub(crate) struct BuiltCardOs {
    pub ending: CardOsEnding,
    pub whdload: Option<WhdloadInstalled>,
    pub kickstarts: Vec<PlacedKickstart>,
    pub steps: Vec<StepReport>,
    pub manifest_path: Option<String>,
    pub health: Option<HealthReport>,
    pub partial: PartialRemoval,
    /// The error a failed or stopped build ends its job with.
    pub error: Option<CoreError>,
}

/// How far the build got in writing the image — which decides what a
/// non-success ending removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Written {
    /// Nothing of this run's is on disk (P3: a `.partial` found there is not ours).
    Nothing,
    /// `<image>.partial` exists and is this run's.
    Partial,
    /// Renamed to `<image>`, and the build still did not succeed.
    Finished,
}

type PhaseResult<T> = Result<T, (BuildPhase, CoreError)>;

fn at(phase: BuildPhase) -> impl Fn(CoreError) -> (BuildPhase, CoreError) {
    move |err| (phase, err)
}

/// Between whole phases, a stop is honoured.
fn gate(phase: BuildPhase, progress: &dyn ProgressSink) -> PhaseResult<()> {
    if progress.is_cancelled() {
        Err((phase, CoreError::Cancelled))
    } else {
        Ok(())
    }
}

/// The request `write_card_image` takes, from the prepared card (R1).
fn card_request_for(
    prepared: &PreparedCard,
    request: &CardOsBuildRequest,
    partial: &Path,
) -> CardBuildRequest {
    let plan = &prepared.measured.plan;
    CardBuildRequest {
        archive: request.archive.clone(),
        kickstart: request.kickstart.clone(),
        dest: partial.display().to_string(),
        total_bytes: plan.image_bytes,
        card_gb: None,
        boot_bytes: 0,
        label: request.label.clone(),
        hardware: request.hardware,
        line: request.line,
        firmware: request.firmware.clone(),
        options: request.options.clone(),
        built_at: request.built_at.clone(),
        file_systems: vec![FileSystemInput {
            path: prepared.measured.driver.path.display().to_string(),
            dos_type: "PDS3".into(),
            version: None,
            revision: None,
        }],
        partitions: plan.partitions.iter().map(|p| p.spec.clone()).collect(),
        first_disk_bytes: 0,
        extra_disks: Vec::new(),
    }
}

/// Every partition of the plan, in order: System from the tree, each of the
/// user's from its roots, and Work formatted empty (R3).
fn preload_request_for(prepared: &PreparedCard, tree: &Path, partial: &Path) -> PreloadRequest {
    PreloadRequest {
        image: partial.to_path_buf(),
        driver: None,
        rdb_backup: None,
        partitions: prepared
            .measured
            .plan
            .partitions
            .iter()
            .enumerate()
            .map(|(i, planned)| PreloadPartition {
                area: 1,
                index: i + 1,
                volume_name: planned.volume_name.clone(),
                content: if i == 0 {
                    vec![tree.to_path_buf()]
                } else {
                    prepared.roots.get(i - 1).cloned().unwrap_or_default()
                },
            })
            .collect(),
    }
}

/// The tree's own release, from `distribution.json`. Absent or unreadable is
/// empty — never guessed.
fn os_of(tree: &Path) -> Vec<String> {
    std::fs::File::open(tree.join(MANIFEST_FILE_NAME))
        .ok()
        .and_then(|file| {
            serde_json::from_reader::<_, DistributionManifest>(std::io::BufReader::new(file)).ok()
        })
        .map(|manifest| vec![manifest.release])
        .unwrap_or_default()
}

fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// What went into each partition, for the manifest: names from the measures,
/// the writer from the step that filled it (or formatted it, for Work).
fn partition_contents(prepared: &PreparedCard, steps: &[StepReport]) -> Vec<PartitionContent> {
    let tool_for = |drive: &str| -> String {
        let copy = steps.iter().find(|report| {
            matches!(&report.step, PreloadStep::CopyIn { drive_name, .. } if drive_name == drive)
        });
        let format = || {
            steps.iter().find(|report| {
                matches!(
                    &report.step,
                    PreloadStep::FormatPartition { drive_name, .. } if drive_name == drive
                )
            })
        };
        copy.or_else(format)
            .map(|report| report.tool.clone())
            .unwrap_or_default()
    };
    let measured = &prepared.measured;
    measured
        .plan
        .partitions
        .iter()
        .enumerate()
        .map(|(i, planned)| {
            let (sources, files, bytes) = if i == 0 {
                (
                    vec![SYSTEM_TREE_SOURCE.to_string()],
                    measured.system.sources.iter().map(|s| s.files).sum(),
                    measured.system.sources.iter().map(|s| s.bytes).sum(),
                )
            } else if let Some(partition) = measured.partitions.get(i - 1) {
                (
                    partition
                        .sources
                        .iter()
                        .map(|s| file_name_of(&s.path))
                        .collect(),
                    partition.sources.iter().map(|s| s.files).sum(),
                    partition.sources.iter().map(|s| s.bytes).sum(),
                )
            } else {
                (Vec::new(), 0, 0)
            };
            PartitionContent {
                drive_name: planned.drive_name.clone(),
                volume_name: planned.volume_name.clone(),
                sources,
                files,
                bytes,
                writer: tool_for(&planned.drive_name),
            }
        })
        .collect()
}

/// The Pi as the screen names it (`pi3-a-plus`), for the health report.
fn pi_name(pi: PiModel) -> String {
    serde_json::to_value(pi)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// The phases, each error tagged with the phase it ended.
#[allow(clippy::too_many_arguments)]
fn run_phases(
    prepared: &PreparedCard,
    tree: &Path,
    image: &Path,
    request: &CardOsBuildRequest,
    native: &dyn VolumeFormatter,
    fallback: Option<&dyn VolumeFormatter>,
    progress: &dyn ProgressSink,
    built: &mut BuiltCardOs,
    written: &mut Written,
) -> PhaseResult<()> {
    use BuildPhase::*;

    // 1. WHDLoad and the agreed Kickstarts, into the tree. The agreement is
    //    checked first: `place_agreed` refuses the whole batch before it
    //    writes, so a refusal leaves the tree as it was.
    gate(Whdload, progress)?;
    built.kickstarts = place_agreed(&prepared.kickstarts, &request.agreed_kickstarts, tree)
        .map_err(at(Whdload))?;
    if let Some(choice) = &prepared.whdload {
        built.whdload = Some(install_whdload(choice, tree).map_err(at(Whdload))?);
    }

    // 2. The card's shape, under `<image>.partial`.
    gate(Card, progress)?;
    refuse_partial_destination(image).map_err(at(Card))?;
    let partial = partial_path_for(image);
    let (_card, facts, boot_files) = write_card_image(
        &card_request_for(prepared, request, &partial),
        &partial,
        progress,
    )
    .map_err(at(Card))?;
    *written = Written::Partial;

    // 3. Every partition formatted, and filled from its roots.
    gate(Partitions, progress)?;
    let made = plan(&preload_request_for(prepared, tree, &partial)).map_err(at(Partitions))?;
    match run_with_fallback(&made, native, fallback, progress) {
        Ok((_outcome, steps)) => built.steps = steps,
        Err(stopped) => {
            let stopped = *stopped;
            built.steps = stopped.steps;
            return Err((Partitions, stopped.error));
        }
    }

    // 4. Described, checked, named — and only then its manifest.
    gate(Check, progress)?;
    let manifest = describe_card(
        &partial,
        facts,
        boot_files,
        request.built_at.clone(),
        os_of(tree),
        partition_contents(prepared, &built.steps),
    )
    .map_err(at(Check))?;
    let health =
        check_image(&partial, Some(&manifest), &pi_name(request.hardware.pi)).map_err(at(Check))?;
    let failed: Vec<String> = health
        .items
        .iter()
        .filter(|item| item.state == crate::core::card::health::CheckState::Fail)
        .map(|item| {
            serde_json::to_value(&item.check)
                .ok()
                .and_then(|value| value.get("kind").and_then(|k| k.as_str()).map(String::from))
                .unwrap_or_default()
        })
        .collect();
    let failures = health.failures();
    built.health = Some(health);
    if failures > 0 {
        return Err((
            Check,
            CoreError::CardCheckFailed {
                failures,
                checks: failed.join(", "),
            },
        ));
    }

    if let Err(err) = finish_partial(image) {
        // `finish_partial`'s own sentence says the image is still at its
        // partial name; the ending removes it, so that sentence would be wrong.
        let err = if image.exists() {
            CoreError::SafetyRefused(format!(
                "'{}' appeared while ART was building it — ART left that file as it is and did \
                 not give its own image that name",
                image.display()
            ))
        } else {
            err
        };
        return Err((Check, err));
    }
    *written = Written::Finished;

    let manifest_path = manifest_path_for(image);
    let text = render_manifest(&manifest).map_err(at(Check))?;
    atomic_write(&manifest_path, text.as_bytes()).map_err(at(Check))?;
    built.manifest_path = Some(manifest_path.display().to_string());
    Ok(())
}

/// Remove the image this run renamed into place, on an ending that is still
/// not a success (its manifest could not be written).
fn remove_finished(image: &Path) -> PartialRemoval {
    match std::fs::remove_file(image) {
        Ok(()) => PartialRemoval::Removed {
            path: image.display().to_string(),
        },
        Err(err) => PartialRemoval::NotRemoved {
            path: image.display().to_string(),
            why: err.to_string(),
        },
    }
}

/// The build, without Tauri: what `card_os_build` runs on its job thread and
/// Task 13 runs in a test.
pub(crate) fn build_card_os(
    prepared: &PreparedCard,
    tree: &Path,
    image: &Path,
    request: &CardOsBuildRequest,
    native: &dyn VolumeFormatter,
    fallback: Option<&dyn VolumeFormatter>,
    progress: &dyn ProgressSink,
) -> BuiltCardOs {
    let mut built = BuiltCardOs {
        ending: CardOsEnding::Succeeded,
        whdload: None,
        kickstarts: Vec::new(),
        steps: Vec::new(),
        manifest_path: None,
        health: None,
        partial: PartialRemoval::NotCreated,
        error: None,
    };
    let mut written = Written::Nothing;
    let outcome = run_phases(
        prepared,
        tree,
        image,
        request,
        native,
        fallback,
        progress,
        &mut built,
        &mut written,
    );
    if let Err((phase, error)) = outcome {
        built.ending = match &error {
            CoreError::Cancelled => CardOsEnding::Stopped { phase },
            other => CardOsEnding::Failed {
                phase,
                code: other.code().to_string(),
                message: other.to_string(),
            },
        };
        built.partial = match written {
            Written::Nothing => remove_partial(image, false),
            Written::Partial => remove_partial(image, true),
            Written::Finished => remove_finished(image),
        };
        built.error = Some(error);
    }
    built
}

/// The operation log's record for a build's every ending (§53).
fn build_record(
    request: &CardOsBuildRequest,
    image: &str,
    built: &BuiltCardOs,
    scratch_left: Option<&LeftBehind>,
) -> OperationRecord {
    let (ending, phase) = match &built.ending {
        CardOsEnding::Succeeded => ("succeeded", BuildPhase::Check),
        CardOsEnding::Failed { phase, .. } => ("failed", *phase),
        CardOsEnding::Stopped { phase } => ("stopped", *phase),
    };
    let partial = match &built.partial {
        PartialRemoval::NotCreated => "not created".to_string(),
        PartialRemoval::Removed { path } => format!("removed: {path}"),
        PartialRemoval::NotRemoved { path, why } => format!("NOT removed: {path} ({why})"),
    };
    let record = user_operation("Build a card image with its partitions")
        .source(request.archive.clone())
        .destination(image.to_string())
        .detail("Phase reached", phase.name())
        .detail("Ending", ending)
        .detail("Partial image", partial)
        .detail(
            "Kickstarts placed",
            built
                .kickstarts
                .iter()
                .map(|k| k.name.clone())
                .collect::<Vec<_>>()
                .join(", "),
        )
        .detail(
            "Session folder",
            match scratch_left {
                None => "removed".to_string(),
                Some(left) => format!("NOT removed: {} ({})", left.path, left.why),
            },
        );
    match (&built.error, &built.health) {
        (Some(err), _) => record.failed(err),
        (None, Some(health)) => record.outcome(OperationOutcome::verified(health.ok())),
        (None, None) => record.outcome(OperationOutcome::success()),
    }
}

/// Build the session's prepared card. Returns a job id; **every** ending
/// arrives on [`CARD_OS_BUILD_EVENT`], and a failed or stopped build also
/// ends its job with its error so the job bar does not say done.
#[tauri::command]
pub fn card_os_build(
    request: CardOsBuildRequest,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    sessions: State<'_, Arc<CardOsSessions>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<JobId> {
    let ticket = sessions.take_for_build(request.session)?;

    let sessions = Arc::clone(&sessions);
    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let image = ticket.image.display().to_string();
    let title = JobTitle::new("components.jobBar.title.buildCardOs").text("target", &image);

    let id = spawn_job(&app, registry, title, move |job_id, progress| {
        let native = NativeFormatter::new(&crate::tools::local_time::LOCAL_TIME);
        let tool_path = request.hst_imager_path.trim().to_string();
        let hst = (!tool_path.is_empty()).then(|| HstImager::at(tool_path));
        let built = build_card_os(
            &ticket.prepared,
            &ticket.tree,
            &ticket.image,
            &request,
            &native,
            hst.as_ref().map(|h| h as &dyn VolumeFormatter),
            progress,
        );

        // After every ending, the session and its folder go.
        let scratch_left = sessions.end_build(request.session);
        write_to_path(
            &log_path,
            &build_record(&request, &image, &built, scratch_left.as_ref()),
        );

        let BuiltCardOs {
            ending,
            whdload,
            kickstarts,
            steps,
            manifest_path,
            health,
            partial,
            error,
        } = built;
        let _ = emit_app.emit(
            CARD_OS_BUILD_EVENT,
            CardOsBuildResult {
                job_id,
                session: request.session,
                image,
                ending,
                whdload,
                kickstarts,
                steps,
                manifest_path,
                health,
                partial,
                scratch_left,
            },
        );
        match error {
            Some(err) => Err(err),
            None => Ok(()),
        }
    });

    Ok(id)
}

// ---------------------------------------------------------------------------
// Close
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardOsClosed {
    pub scratch_left: Option<LeftBehind>,
}

/// Close a session without building; its folder is removed.
#[tauri::command]
pub fn card_os_close(
    session: u64,
    sessions: State<'_, Arc<CardOsSessions>>,
) -> AppResult<CardOsClosed> {
    Ok(CardOsClosed {
        scratch_left: sessions.close(session)?,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// A small prepared card and a build request, for this module's tests and
/// Task 13's end-to-end card.
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::core::card::propose::{MEASURED_BOOT_BYTES, MEASURED_BUFFERS};
    use crate::core::card::sizing::{CardImagePlan, PlannedPartition, BYTES_PER_CYLINDER};
    use crate::core::clock::UtcClock;
    use crate::core::jobs::NoProgress;
    use crate::core::pistorm::hardware::{AmigaTarget, PistormVariant};
    use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

    pub(crate) const MIB: u64 = 1024 * 1024;
    pub(crate) const GIB: u64 = 1024 * MIB;

    /// A folder `name` under `dir` holding `files` (paths `/`-separated).
    pub(crate) fn folder_with(dir: &Path, name: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let root = dir.join(name);
        std::fs::create_dir_all(&root).unwrap();
        for (path, bytes) in files {
            let host = path
                .split('/')
                .fold(root.clone(), |acc, part| acc.join(part));
            std::fs::create_dir_all(host.parent().unwrap()).unwrap();
            std::fs::write(host, bytes).unwrap();
        }
        root
    }

    /// A material folder with a loose `pfs3aio` 19.2 — bytes that state a
    /// version, not a driver; the card's RDB carries them and nothing runs them.
    pub(crate) fn material_with_driver(dir: &Path) -> PathBuf {
        let material = dir.join("material");
        std::fs::create_dir_all(&material).unwrap();
        let mut driver = vec![0u8; 64];
        driver.extend_from_slice(b"$VER: pfs3aio 19.2 (01.01.2026)\0");
        std::fs::write(material.join("pfs3aio"), driver).unwrap();
        material
    }

    /// A PFS3 partition of `size_mb` (0: the rest of the disk).
    fn planned(drive: &str, volume: &str, size_mb: u32, bootable: bool) -> PlannedPartition {
        PlannedPartition {
            drive_name: drive.into(),
            volume_name: volume.into(),
            bytes: (u64::from(size_mb) * MIB).div_ceil(BYTES_PER_CYLINDER) * BYTES_PER_CYLINDER,
            spec: PartitionSpec {
                drive_name: drive.into(),
                fs_type: AmigaHardDiskFs::Pfs3DirectScsi,
                size_mb,
                bootable,
                boot_priority: 0,
                num_buffers: MEASURED_BUFFERS,
            },
        }
    }

    /// R2: a 2 GiB card laid out by hand — `plan_card_image` would make a
    /// label's size, and the smallest label is far larger than a test needs.
    /// `volumes` are System, the user's partitions, then Work (the rest).
    pub(crate) fn two_gib_plan(volumes: &[&str]) -> CardImagePlan {
        let image_bytes = 2 * GIB;
        let last = volumes.len() - 1;
        CardImagePlan {
            image_bytes,
            area_bytes: image_bytes - MEASURED_BOOT_BYTES,
            partitions: volumes
                .iter()
                .enumerate()
                .map(|(i, volume)| {
                    let size_mb = match i {
                        0 => 64,
                        i if i == last => 0,
                        _ => 16,
                    };
                    planned(&format!("SDH{i}"), volume, size_mb, i == 0)
                })
                .collect(),
        }
    }

    /// System's tree: `C/Dir` and `S/Startup-Sequence`.
    pub(crate) fn small_tree(dir: &Path) -> PathBuf {
        folder_with(
            dir,
            "tree",
            &[
                ("C/Dir", &[0u8; 3000][..]),
                ("S/Startup-Sequence", b"Echo hello\n"),
            ],
        )
    }

    /// The smallest prepared card: System, `Games` (one folder, no titles),
    /// Work — measured and staged for real, on the hand-built 2 GiB plan.
    pub(crate) fn small_prepared_card(dir: &Path) -> (PreparedCard, PathBuf) {
        let tree = small_tree(dir);
        let games = folder_with(dir, "Games", &[("Readme", b"hello\n")]);
        let material = [material_with_driver(dir)];
        let parts = [PartitionInput {
            volume_name: "Games".into(),
            sources: vec![games],
            floor_bytes: 0,
        }];
        let mut measured = measure_card(
            16,
            &tree,
            &parts,
            &material,
            None,
            &dir.join("driver"),
            true,
            &UtcClock,
            &NoProgress,
        )
        .unwrap();
        let plan = two_gib_plan(&["System", "Games", "Work"]);
        let names = |plan: &CardImagePlan| {
            plan.partitions
                .iter()
                .map(|p| (p.drive_name.clone(), p.volume_name.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(&measured.plan),
            names(&plan),
            "R2: same drives, same order"
        );
        measured.plan = plan;

        let prepared = stage_card(
            measured,
            &tree,
            &dir.join("staging"),
            &material,
            &[],
            &UtcClock,
            &NoProgress,
        )
        .unwrap();
        (prepared, tree)
    }

    /// A zip shaped like the Emu68 release: files at the root, a folder, and
    /// `config.txt` naming the kernel.
    pub(crate) fn emu68_zip(dir: &Path) -> PathBuf {
        use std::io::Write as _;
        let path = dir.join("Emu68-pistorm.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (entry, contents) in [
            ("Emu68-pistorm.gz", &b"kernel"[..]),
            ("start.elf", b"firmware"),
            ("overlays/emu68.dtbo", b"overlay"),
            ("config.txt", b"kernel=Emu68-pistorm.gz\narm_64bit=1\n"),
        ] {
            zip.start_file(entry, options).unwrap();
            zip.write_all(contents).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    pub(crate) fn request_for(dir: &Path) -> CardOsBuildRequest {
        CardOsBuildRequest {
            session: 0,
            agreed_kickstarts: Vec::new(),
            archive: emu68_zip(dir).display().to_string(),
            kickstart: None,
            label: "ART CARD".into(),
            hardware: PistormHardware {
                amiga: AmigaTarget::A500,
                variant: PistormVariant::Classic,
                pi: PiModel::Pi3APlus,
            },
            line: Emu68Line::Stable,
            firmware: FirmwareConfig::default(),
            options: Emu68Options::default(),
            built_at: None,
            hst_imager_path: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;
    use crate::core::jobs::NoProgress;
    use crate::core::preload::{CopySummary, ToolVersion};
    use std::sync::atomic::AtomicBool;

    fn scratch(tag: &str) -> (crate::core::ScratchDir, PathBuf) {
        crate::core::ScratchDir::pair("art-cardos-cmd", tag)
    }

    /// `NativeFormatter`, except that formatting drive `on` fails.
    struct FailingFormatter {
        on: &'static str,
    }

    impl VolumeFormatter for FailingFormatter {
        fn probe(&self) -> CoreResult<ToolVersion> {
            NativeFormatter::UTC.probe()
        }

        fn format_partition(
            &self,
            image: &Path,
            slot: Option<usize>,
            index: usize,
            volume: &str,
            sink: &dyn ProgressSink,
        ) -> CoreResult<()> {
            if format!("SDH{}", index.saturating_sub(1)) == self.on {
                return Err(CoreError::Io(std::io::Error::other(format!(
                    "formatting {} was made to fail",
                    self.on
                ))));
            }
            NativeFormatter::UTC.format_partition(image, slot, index, volume, sink)
        }

        fn can_copy_in(
            &self,
            image: &Path,
            slot: Option<usize>,
            drive: &str,
            source: &Path,
        ) -> CoreResult<()> {
            NativeFormatter::UTC.can_copy_in(image, slot, drive, source)
        }

        fn copy_in(
            &self,
            image: &Path,
            slot: Option<usize>,
            drive: &str,
            source: &Path,
            sink: &dyn ProgressSink,
        ) -> CoreResult<CopySummary> {
            NativeFormatter::UTC.copy_in(image, slot, drive, source, sink)
        }

        fn can_copy_in_sources(
            &self,
            image: &Path,
            slot: Option<usize>,
            drive: &str,
            sources: &[PathBuf],
        ) -> CoreResult<()> {
            NativeFormatter::UTC.can_copy_in_sources(image, slot, drive, sources)
        }
    }

    /// Asks to stop once the first partition's format has been reported —
    /// so the stop lands between whole steps of the Partitions phase.
    #[derive(Default)]
    struct StopAfterFirstFormat {
        seen: AtomicBool,
    }

    impl ProgressSink for StopAfterFirstFormat {
        fn report(&self, _done: u64, _total: Option<u64>, message: &str) {
            if message.starts_with("Formatting ") {
                self.seen.store(true, Ordering::SeqCst);
            }
        }

        fn is_cancelled(&self) -> bool {
            self.seen.load(Ordering::SeqCst)
        }
    }

    /// Every file under `root`, with its size — a tree "unchanged" is this
    /// list unchanged.
    fn listing(root: &Path) -> Vec<(PathBuf, u64)> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let len = std::fs::metadata(&path).unwrap().len();
                    out.push((path.strip_prefix(root).unwrap().to_path_buf(), len));
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn a_build_that_fails_after_the_image_exists_removes_the_partial_and_says_so() {
        let (_guard, dir) = scratch("fail-after-image");
        let (prepared, tree) = small_prepared_card(&dir);
        let image = dir.join("card.img");
        let failing = FailingFormatter { on: "SDH1" };

        let built = build_card_os(
            &prepared,
            &tree,
            &image,
            &request_for(&dir),
            &failing,
            None,
            &NoProgress,
        );

        assert!(
            matches!(
                built.ending,
                CardOsEnding::Failed {
                    phase: BuildPhase::Partitions,
                    ..
                }
            ),
            "{:?}",
            built.ending
        );
        assert!(
            matches!(built.partial, PartialRemoval::Removed { .. }),
            "{:?}",
            built.partial
        );
        assert!(!partial_path_for(&image).exists() && !image.exists());
        // System was formatted and filled before SDH1 failed, and says so.
        assert_eq!(built.steps.len(), 2, "{:?}", built.steps);
        assert!(built.error.is_some());
    }

    #[test]
    fn a_stop_between_partitions_is_stopped_not_failed_and_removes_the_partial() {
        let (_guard, dir) = scratch("stop-between");
        let (prepared, tree) = small_prepared_card(&dir);
        let image = dir.join("card.img");
        let sink = StopAfterFirstFormat::default();

        let built = build_card_os(
            &prepared,
            &tree,
            &image,
            &request_for(&dir),
            &NativeFormatter::UTC,
            None,
            &sink,
        );

        assert!(
            matches!(
                built.ending,
                CardOsEnding::Stopped {
                    phase: BuildPhase::Partitions
                }
            ),
            "{:?}",
            built.ending
        );
        assert!(
            matches!(built.partial, PartialRemoval::Removed { .. }),
            "{:?}",
            built.partial
        );
        assert!(!partial_path_for(&image).exists() && !image.exists());
        assert!(matches!(built.error, Some(CoreError::Cancelled)));
    }

    #[test]
    fn a_refusal_before_the_image_leaves_no_partial_and_reports_not_created() {
        let (_guard, dir) = scratch("refusal");
        let (prepared, tree) = small_prepared_card(&dir);
        let image = dir.join("card.img");
        let mut request = request_for(&dir);
        request.agreed_kickstarts = vec!["kick40068.A1200".into()];
        let before = listing(&tree);

        let built = build_card_os(
            &prepared,
            &tree,
            &image,
            &request,
            &NativeFormatter::UTC,
            None,
            &NoProgress,
        );

        match &built.ending {
            CardOsEnding::Failed { phase, code, .. } => {
                assert_eq!(*phase, BuildPhase::Whdload);
                assert_eq!(code, "ART-KICKSTART-NOT-PROPOSED");
            }
            other => panic!("expected a Whdload failure, got {other:?}"),
        }
        assert_eq!(built.partial, PartialRemoval::NotCreated);
        assert!(!partial_path_for(&image).exists() && !image.exists());
        assert_eq!(listing(&tree), before, "the tree is unchanged");
    }

    #[test]
    fn a_missing_emu68_archive_is_refused_before_the_image() {
        let (_guard, dir) = scratch("no-archive");
        let (prepared, tree) = small_prepared_card(&dir);
        let image = dir.join("card.img");
        let mut request = request_for(&dir);
        request.archive = dir.join("not-there.zip").display().to_string();

        let built = build_card_os(
            &prepared,
            &tree,
            &image,
            &request,
            &NativeFormatter::UTC,
            None,
            &NoProgress,
        );

        assert!(
            matches!(
                built.ending,
                CardOsEnding::Failed {
                    phase: BuildPhase::Card,
                    ..
                }
            ),
            "{:?}",
            built.ending
        );
        assert_eq!(built.partial, PartialRemoval::NotCreated);
        assert!(!partial_path_for(&image).exists() && !image.exists());
    }

    #[test]
    fn closing_a_session_removes_its_folder_and_a_second_close_is_a_refusal() {
        let (_guard, dir) = scratch("close");
        let sessions = CardOsSessions::default();
        let (id, tree) = sessions.open_in(&dir).unwrap();
        assert!(tree.is_dir());
        let folder = tree.parent().unwrap().to_path_buf();
        assert!(folder.join(STAGING).is_dir() && folder.join(DRIVER).is_dir());

        assert_eq!(sessions.close(id).unwrap(), None);
        assert!(!folder.exists(), "the session folder is gone");

        let err = sessions.close(id).unwrap_err();
        assert!(
            err.to_string()
                .contains(&format!("no card session {id} is open")),
            "{err}"
        );
    }

    #[test]
    fn a_session_cannot_be_built_twice_at_once() {
        let (_guard, dir) = scratch("twice");
        let (prepared, _tree) = small_prepared_card(&dir);
        let sessions = CardOsSessions::default();
        let (id, _) = sessions.open_in(&dir).unwrap();
        sessions.begin_prepare(id).unwrap();
        sessions.end_prepare(id, Some((prepared, dir.join("card.img"))));

        let ticket = sessions.take_for_build(id).unwrap();
        assert_eq!(ticket.image, dir.join("card.img"));
        let err = match sessions.take_for_build(id) {
            Ok(_) => panic!("a second build of one session was allowed"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("busy"), "{err}");
        // And a busy session is not closed from under its build.
        assert!(sessions.close(id).unwrap_err().to_string().contains("busy"));

        assert_eq!(sessions.end_build(id), None);
    }
}
