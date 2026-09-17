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
//! **Endings stay distinct.** A build succeeds, is refused, fails, or is
//! stopped, each in a named phase — [`CardOsEnding`]. Everything a build can
//! refuse is asked before it writes anything (the preflight), so a refusal's
//! next step — choose another name, put a file in place — is never mistaken
//! for a failure's (keep the log, build again). A stop is never reported as a
//! failure, and every ending says what became of `<image>.partial`
//! ([`PartialRemoval`]): ART removes only the file this run created (P3).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::commands::card::{card_image_inputs, CardBuildRequest, CardImageInputs};
use crate::commands::hdf::FileSystemInput;
use crate::commands::preload::{run_with_fallback, StepReport};
use crate::core::card::build::{build_card_reporting, ImageLeft};
use crate::core::card::health::{check_image, HealthReport};
use crate::core::card::manifest::{
    describe_card, manifest_path_for, render_manifest, PartitionContent,
};
use crate::core::card::read_card;
use crate::core::cardos::kickstarts::{check_agreed, place_agreed, PlacedKickstart};
use crate::core::cardos::partial::{
    finish_partial, partial_path_for, refuse_partial_destination, remove_partial, PartialRemoval,
};
use crate::core::cardos::prepare::{
    check_free_space, measure_card, space_needs, stage_card, HstImagerState, PartitionInput,
    PartitionWriter, PreparedCard,
};
use crate::core::cardos::readback::{count_pfs3_partition, PartitionCount};
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
use crate::core::rom::place::PlaceOutcome;
use crate::core::safety::{atomic_create_new, Created};
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
    /// The hst-imager the preparation asked, when one was given — the build
    /// uses this one, never a second path of its own (card round 3, I1).
    hst_imager: Option<PathBuf>,
    busy: bool,
}

/// What a finished preparation leaves in its session.
pub(crate) struct PreparedSession {
    pub prepared: PreparedCard,
    pub image: PathBuf,
    pub hst_imager: Option<PathBuf>,
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
    pub hst_imager: Option<PathBuf>,
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
                hst_imager: None,
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
    pub(crate) fn end_prepare(&self, id: u64, result: Option<PreparedSession>) {
        let mut sessions = self.lock();
        if let Some(session) = sessions.get_mut(&id) {
            session.busy = false;
            match result {
                Some(done) => {
                    session.prepared = Some(done.prepared);
                    session.image = Some(done.image);
                    session.hst_imager = done.hst_imager;
                }
                None => {
                    session.prepared = None;
                    session.image = None;
                    session.hst_imager = None;
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
            hst_imager: session.hst_imager.take(),
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
/// `hst_probe` asks the hst-imager the request names whether it runs (I1):
/// a partition is given to hst-imager only when it answered.
#[allow(clippy::too_many_arguments)]
fn prepare_card(
    request: &CardOsPrepareRequest,
    image: &Path,
    session_root: &Path,
    clock: &dyn AmigaClock,
    available: impl Fn(&Path) -> std::io::Result<u64>,
    hst_probe: impl Fn(&Path) -> Result<(), String>,
    progress: &dyn ProgressSink,
) -> CoreResult<PreparedCard> {
    let hst_imager = match given_path(Some(&request.hst_imager_path)) {
        None => HstImagerState::NotConfigured,
        Some(tool) => match hst_probe(&tool) {
            Ok(()) => HstImagerState::Usable,
            Err(why) => HstImagerState::Unusable {
                path: tool.display().to_string(),
                why,
            },
        },
    };
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
        &hst_imager,
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
    let image = image_path_from(&request.image)?;
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
            |tool: &Path| {
                HstImager::at(tool)
                    .probe()
                    .map(|_| ())
                    .map_err(|err| err.to_string())
            },
            progress,
        );
        write_to_path(&log_path, &prepare_record(&request, &image, &outcome));

        match outcome {
            Ok(prepared) => {
                // I1: the build gets this hst-imager — the one that answered
                // here — exactly when a partition was given to it.
                let hst_imager = (!volumes_needing_hst_imager(&prepared).is_empty())
                    .then(|| given_path(Some(&request.hst_imager_path)))
                    .flatten();
                sessions.end_prepare(
                    request.session,
                    Some(PreparedSession {
                        prepared: prepared.clone(),
                        image,
                        hst_imager,
                    }),
                );
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
    // No hst-imager path: the build uses the one its preparation asked
    // (card round 3, I1), so the two can never disagree.
}

/// The event a build's every ending arrives on.
pub const CARD_OS_BUILD_EVENT: &str = "card-os-build-result";

/// How a build ended. Four sentences, never collapsed into "not succeeded".
#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "ending",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum CardOsEnding {
    Succeeded,
    /// Refused before anything of the image was written: its next step is
    /// the user's (a name, a file, a setting), not "build again" (I3).
    Refused {
        phase: BuildPhase,
        code: String,
        message: String,
    },
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
#[derive(Debug, Clone, PartialEq, Eq)]
enum Written {
    /// Nothing of this run's is on disk (P3: a `.partial` found there is not ours).
    Nothing,
    /// The card writer created `<image>.partial`, failed, and said what it
    /// did with the file — the ending reports exactly that (I2).
    Reported(PartialRemoval),
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

/// The request `card_image_inputs` takes, from the prepared card (R1).
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

/// The most of the tree's `distribution.json` ART reads to name the release.
/// It lists every file of the tree, so it is far larger than a config file —
/// but it is bounded (card round 3, M13).
const DISTRIBUTION_MANIFEST_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// The tree's own release, from `distribution.json`. Absent, unreadable or
/// larger than [`DISTRIBUTION_MANIFEST_MAX_BYTES`] is empty — never guessed.
fn os_of(tree: &Path) -> Vec<String> {
    let path = tree.join(MANIFEST_FILE_NAME);
    let small_enough = std::fs::metadata(&path)
        .map(|m| m.len() <= DISTRIBUTION_MANIFEST_MAX_BYTES)
        .unwrap_or(false);
    if !small_enough {
        return Vec::new();
    }
    std::fs::File::open(&path)
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

/// What is in each partition, for the manifest: the sources' names from the
/// preparation, the writer from the step that filled it (or formatted it, for
/// Work), and the files and bytes **counted off the finished card** (I4) —
/// `counts` is one per planned partition, in order.
fn partition_contents(
    prepared: &PreparedCard,
    steps: &[StepReport],
    counts: &[PartitionCount],
) -> Vec<PartitionContent> {
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
            .map(|report| writer_name(&report.tool))
            .unwrap_or_default()
    };
    let measured = &prepared.measured;
    measured
        .plan
        .partitions
        .iter()
        .enumerate()
        .map(|(i, planned)| {
            let sources = if i == 0 {
                vec![SYSTEM_TREE_SOURCE.to_string()]
            } else if let Some(partition) = measured.partitions.get(i - 1) {
                partition
                    .sources
                    .iter()
                    .map(|s| file_name_of(&s.path))
                    .collect()
            } else {
                Vec::new()
            };
            let count = counts
                .get(i)
                .copied()
                .unwrap_or(PartitionCount { files: 0, bytes: 0 });
            PartitionContent {
                area: 0,
                drive_name: planned.drive_name.clone(),
                volume_name: planned.volume_name.clone(),
                sources,
                files: count.files,
                bytes: count.bytes,
                writer: tool_for(&planned.drive_name),
            }
        })
        .collect()
}

/// The writer the manifest records for a step's tool: `native`, or the
/// fallback's probed version **named as hst-imager's** — a bare
/// `1.6.616+a91fa4ca` does not say which tool wrote the partition (the
/// ledger's T13). The fallback of a card build is only ever hst-imager.
fn writer_name(tool: &str) -> String {
    if tool == "native" || tool.starts_with("hst-imager") {
        tool.to_string()
    } else {
        format!("hst-imager {tool}")
    }
}

/// A card image path from the screen: refused when blank, or when relative —
/// a relative path would be read against wherever the process happens to be
/// running (card round 3, M7).
fn image_path_from(given: &str) -> CoreResult<PathBuf> {
    let path = PathBuf::from(given.trim());
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return Err(CoreError::InvalidInput(format!(
            "'{}' is not where a card image can go — choose a full path for it, such as \
             E:\\cards\\amiga.img",
            given.trim()
        )));
    }
    Ok(path)
}

/// What became of `<image>.partial` when writing the card failed — the card
/// writer's own account of the file it created (I2), never a guess.
fn partial_after_card_failure(image: &Path, left: &ImageLeft) -> PartialRemoval {
    let path = partial_path_for(image).display().to_string();
    match left {
        ImageLeft::NotCreated => PartialRemoval::NotCreated,
        ImageLeft::Removed => PartialRemoval::Removed { path },
        ImageLeft::NotRemoved { why } => PartialRemoval::NotRemoved {
            path,
            why: why.clone(),
        },
    }
}

/// The volumes the preparation gave to hst-imager.
fn volumes_needing_hst_imager(prepared: &PreparedCard) -> Vec<String> {
    std::iter::once(&prepared.measured.system)
        .chain(prepared.measured.partitions.iter())
        .filter(|partition| partition.writer == PartitionWriter::HstImager)
        .map(|partition| partition.volume_name.clone())
        .collect()
}

/// The hst-imager a build falls back to, with the path the preparation
/// probed — so a refusal names the tool that was given rather than saying
/// none was (card round 3, N1).
#[derive(Clone, Copy)]
pub(crate) struct HstFallback<'a> {
    pub path: &'a Path,
    pub tool: &'a dyn VolumeFormatter,
}

/// I1: a card prepared with partitions for hst-imager is built only with an
/// hst-imager that answers — asked here, before the image exists, rather
/// than discovered at that partition's format with the card half written.
fn refuse_without_hst_imager(
    prepared: &PreparedCard,
    fallback: Option<HstFallback<'_>>,
) -> CoreResult<()> {
    let partitions = volumes_needing_hst_imager(prepared);
    if partitions.is_empty() {
        return Ok(());
    }
    match fallback {
        None => Err(CoreError::HstImagerUnusable {
            partitions,
            path: String::new(),
            why: String::new(),
        }),
        Some(fallback) => {
            fallback
                .tool
                .probe()
                .map(|_| ())
                .map_err(|err| CoreError::HstImagerUnusable {
                    partitions,
                    path: fallback.path.display().to_string(),
                    why: err.to_string(),
                })
        }
    }
}

/// The Pi as the screen names it (`pi3-a-plus`), for the health report.
fn pi_name(pi: PiModel) -> String {
    serde_json::to_value(pi)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Everything the build can refuse, asked before it writes anything — the
/// tree included (I3, M1). Every error here is a refusal: nothing of this
/// run's exists yet. Returns what the card image is built from.
fn preflight(
    prepared: &PreparedCard,
    image: &Path,
    request: &CardOsBuildRequest,
    fallback: Option<HstFallback<'_>>,
    progress: &dyn ProgressSink,
) -> PhaseResult<CardImageInputs> {
    use BuildPhase::*;

    // Whdload: every agreed name is one the proposal supplies.
    gate(Whdload, progress)?;
    check_agreed(&prepared.kickstarts, &request.agreed_kickstarts).map_err(at(Whdload))?;

    // Card: the names are free, and the archive, the Kickstart and the driver
    // are read and checked.
    gate(Card, progress)?;
    refuse_partial_destination(image).map_err(at(Card))?;
    let inputs = card_image_inputs(&card_request_for(
        prepared,
        request,
        &partial_path_for(image),
    ))
    .map_err(at(Card))?;

    // Partitions: the tool the preparation chose is here and answers.
    gate(Partitions, progress)?;
    refuse_without_hst_imager(prepared, fallback).map_err(at(Partitions))?;
    Ok(inputs)
}

/// The sentence for a finish that failed. `finish_partial`'s own sentence
/// says the image is still at its partial name; the ending removes it, so
/// that sentence would be wrong — a name taken meanwhile is said as such.
fn finish_error(image: &Path, err: CoreError) -> CoreError {
    // N6: both names left are ART's own card, said by `finish_partial` itself.
    if matches!(err, CoreError::CardFinishLeftBothNames { .. }) {
        return err;
    }
    if image.exists() {
        CoreError::SafetyRefused(format!(
            "'{}' appeared while ART was building it — ART left that file as it is and did \
             not give its own image that name",
            image.display()
        ))
    } else {
        err
    }
}

/// The phases, each error tagged with the phase it ended.
#[allow(clippy::too_many_arguments)]
fn run_phases(
    prepared: &PreparedCard,
    tree: &Path,
    image: &Path,
    request: &CardOsBuildRequest,
    inputs: CardImageInputs,
    native: &dyn VolumeFormatter,
    fallback: Option<HstFallback<'_>>,
    progress: &dyn ProgressSink,
    built: &mut BuiltCardOs,
    written: &mut Written,
) -> PhaseResult<()> {
    use BuildPhase::*;

    // 1. WHDLoad and the agreed Kickstarts, into the tree.
    gate(Whdload, progress)?;
    built.kickstarts = place_agreed(&prepared.kickstarts, &request.agreed_kickstarts, tree)
        .map_err(at(Whdload))?;
    if let Some(choice) = &prepared.whdload {
        built.whdload = Some(install_whdload(choice, tree).map_err(at(Whdload))?);
    }

    // 2. The card's shape, under `<image>.partial`. The destination is asked
    //    again: the Whdload phase took time, and a file that appeared meanwhile
    //    is still a refusal, not a failure.
    gate(Card, progress)?;
    refuse_partial_destination(image).map_err(at(Card))?;
    let partial = partial_path_for(image);
    let CardImageInputs {
        spec,
        facts,
        boot_files,
    } = inputs;
    match build_card_reporting(&partial, &spec, progress) {
        Ok(_card) => *written = Written::Partial,
        Err(failure) => {
            // I2: the card writer says whether it created the file and
            // whether it removed it again — not `NotCreated` by default.
            if failure.image != ImageLeft::NotCreated {
                *written = Written::Reported(partial_after_card_failure(image, &failure.image));
            }
            return Err((Card, failure.error));
        }
    }

    // 3. Every partition formatted, and filled from its roots.
    gate(Partitions, progress)?;
    let made = plan(&preload_request_for(prepared, tree, &partial)).map_err(at(Partitions))?;
    match run_with_fallback(&made, native, fallback.map(|f| f.tool), progress) {
        Ok((_outcome, steps)) => built.steps = steps,
        Err(stopped) => {
            let stopped = *stopped;
            built.steps = stopped.steps;
            return Err((Partitions, stopped.error));
        }
    }

    // 4. Counted, described, checked, named — and only then its manifest.
    gate(Check, progress)?;
    let card = read_card(&partial).map_err(at(Check))?;
    let counts = (0..prepared.measured.plan.partitions.len())
        .map(|index| count_pfs3_partition(&partial, &card, 0, index))
        .collect::<CoreResult<Vec<_>>>()
        .map_err(at(Check))?;
    let manifest = describe_card(
        &partial,
        facts,
        boot_files,
        request.built_at.clone(),
        os_of(tree),
        partition_contents(prepared, &built.steps, &counts),
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
        return Err((Check, finish_error(image, err)));
    }
    *written = Written::Finished;

    // M17: created, never replaced — a manifest that appeared since the
    // preflight is someone's, and the image goes with the refusal.
    let manifest_path = manifest_path_for(image);
    let text = render_manifest(&manifest).map_err(at(Check))?;
    match atomic_create_new(&manifest_path, text.as_bytes()).map_err(at(Check))? {
        Created::Yes => {}
        Created::AlreadyThere => {
            return Err((
                Check,
                CoreError::SafetyRefused(format!(
                    "'{}' appeared while ART was building the card — ART left it as it is and \
                     did not finish the card",
                    manifest_path.display()
                )),
            ))
        }
    }
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
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => PartialRemoval::AlreadyGone {
            path: image.display().to_string(),
        },
        Err(err) => PartialRemoval::NotRemoved {
            path: image.display().to_string(),
            why: err.to_string(),
        },
    }
}

/// A build's ending for an error: stopped for a stop, refused when the error
/// is a refusal and nothing of the image was written, failed otherwise.
fn ending_for(phase: BuildPhase, error: &CoreError, refused: bool) -> CardOsEnding {
    match error {
        CoreError::Cancelled => CardOsEnding::Stopped { phase },
        other if refused => CardOsEnding::Refused {
            phase,
            code: other.code().to_string(),
            message: other.to_string(),
        },
        other => CardOsEnding::Failed {
            phase,
            code: other.code().to_string(),
            message: other.to_string(),
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
    fallback: Option<HstFallback<'_>>,
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

    let inputs = match preflight(prepared, image, request, fallback, progress) {
        Ok(inputs) => inputs,
        Err((phase, error)) => {
            built.ending = ending_for(phase, &error, true);
            built.error = Some(error);
            return built;
        }
    };

    let mut written = Written::Nothing;
    let outcome = run_phases(
        prepared,
        tree,
        image,
        request,
        inputs,
        native,
        fallback,
        progress,
        &mut built,
        &mut written,
    );
    if let Err((phase, error)) = outcome {
        // After the preflight, only a name taken meanwhile is a refusal — and
        // only while nothing of the image exists.
        let refused = written == Written::Nothing
            && matches!(
                error,
                CoreError::SafetyRefused(_) | CoreError::PartialImageExists { .. }
            );
        built.ending = ending_for(phase, &error, refused);
        built.partial = match written {
            Written::Nothing => remove_partial(image, false),
            Written::Reported(partial) => partial,
            Written::Partial => remove_partial(image, true),
            Written::Finished => remove_finished(image),
        };
        built.error = Some(error);
    }
    built
}

/// The operation log's line for the Kickstarts: each with what happened to
/// its image and its `.RTB`, and — on any ending but success — that they went
/// only into the session tree, which the ending removes (M1).
fn kickstarts_detail(built: &BuiltCardOs) -> String {
    if built.kickstarts.is_empty() {
        return "none".to_string();
    }
    let outcome = |placed: &PlaceOutcome| match placed {
        PlaceOutcome::Placed { .. } => "placed",
        PlaceOutcome::AlreadyThere { .. } => "already there",
        PlaceOutcome::Occupied { .. } => "a different file was already there and was kept",
    };
    let list = built
        .kickstarts
        .iter()
        .map(|k| {
            format!(
                "{} (image: {}; .RTB: {})",
                k.name,
                outcome(&k.image),
                outcome(&k.rtb)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    match built.ending {
        CardOsEnding::Succeeded => list,
        _ => format!(
            "{list} — into the session tree only, which is removed with the session folder; \
             none of them reached a card"
        ),
    }
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
        CardOsEnding::Refused { phase, .. } => ("refused", *phase),
        CardOsEnding::Failed { phase, .. } => ("failed", *phase),
        CardOsEnding::Stopped { phase } => ("stopped", *phase),
    };
    let partial = match &built.partial {
        PartialRemoval::NotCreated => "not created".to_string(),
        PartialRemoval::Removed { path } => format!("removed: {path}"),
        PartialRemoval::AlreadyGone { path } => format!("already gone: {path}"),
        PartialRemoval::NotRemoved { path, why } => format!("NOT removed: {path} ({why})"),
    };
    let record = user_operation("Build a card image with its partitions")
        .source(request.archive.clone())
        .destination(image.to_string())
        .detail("Phase reached", phase.name())
        .detail("Ending", ending)
        .detail("Partial image", partial)
        .detail("Kickstarts", kickstarts_detail(built))
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
/// arrives on [`CARD_OS_BUILD_EVENT`], and a refused, failed or stopped build
/// also ends its job with its error so the job bar does not say done.
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
        // I1: the hst-imager the preparation asked, never a second path.
        let hst = ticket
            .hst_imager
            .as_ref()
            .map(|path| (path, HstImager::at(path)));
        let built = build_card_os(
            &ticket.prepared,
            &ticket.tree,
            &ticket.image,
            &request,
            &native,
            hst.as_ref().map(|(path, tool)| HstFallback {
                path: path.as_path(),
                tool: tool as &dyn VolumeFormatter,
            }),
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

    /// System's tree: `C/Dir`, `S/Startup-Sequence` and a `distribution.json`
    /// stating its release.
    pub(crate) fn small_tree(dir: &Path) -> PathBuf {
        folder_with(
            dir,
            "tree",
            &[
                ("C/Dir", &[0u8; 3000][..]),
                ("S/Startup-Sequence", b"Echo hello\n"),
                (
                    MANIFEST_FILE_NAME,
                    br#"{"release":"AmigaOS 3.2.2","builtFrom":[],"files":[]}"#,
                ),
            ],
        )
    }

    /// The material a card round needs beside the driver: `WHDLoad_usr.lha`
    /// (WHDLoad 20.0 and its prefs) and `skick346.lha` (the 1.3 `.RTB`).
    fn material_with_whdload_and_skick(dir: &Path) -> PathBuf {
        let material = material_with_driver(dir);
        let lha = crate::core::lha::tests::make_lha_with;
        std::fs::write(
            material.join("WHDLoad_usr.lha"),
            lha(&[
                ("WHDLoad/C/WHDLoad", &whdload_bytes("20.0")),
                ("WHDLoad/S/WHDLoad.prefs", b";prefs\n"),
            ]),
        )
        .unwrap();
        std::fs::write(
            material.join("skick346.lha"),
            lha(&[("Kickstarts/kick34005.A500.RTB", &[1u8; 4000][..])]),
        )
        .unwrap();
        material
    }

    /// The smallest whole card: System (the tree); `Games`, one WHDLoad
    /// hardfile whose `Turrican` slave names `kick34005.A500`; `Stuff`, a
    /// folder; Work — with the driver, WHDLoad and skick in the material and
    /// the 1.3 ROM in a Kickstart folder. Measured and staged for real, on the
    /// hand-built 2 GiB plan (R2).
    pub(crate) fn small_prepared_card(dir: &Path) -> (PreparedCard, PathBuf) {
        small_prepared_card_with(dir, "Notes")
    }

    /// [`small_prepared_card`], with Stuff's one drawer named `stuff_drawer`
    /// (a non-ASCII name sends Stuff to hst-imager).
    pub(crate) fn small_prepared_card_with(
        dir: &Path,
        stuff_drawer: &str,
    ) -> (PreparedCard, PathBuf) {
        use crate::core::gameindex::readers::slave::tests_support::slave_needing;

        let tree = small_tree(dir);
        let rom = rom_13();
        let roms = dir.join("roms");
        std::fs::create_dir_all(&roms).unwrap();
        std::fs::write(roms.join("my-13.rom"), &rom).unwrap();
        let slave = slave_needing(
            "kick34005.A500",
            crate::core::hashing::crc16_arc(&rom),
            rom.len() as u32,
        );
        let hdf = dir.join("Turrican.hdf");
        crate::core::cardos::content::test_support::build_hdf(
            &hdf,
            &["C", "S", "Turrican"],
            &[
                ("C/WHDLoad", &whdload_bytes("18.0")),
                ("S/Startup-Sequence", b"WHDLoad Turrican.slave\n"),
                ("Turrican/Turrican.slave", &slave),
                ("Turrican.info", b"icon"),
            ],
        );
        let stuff = folder_with(
            dir,
            "Stuff",
            &[(&format!("{stuff_drawer}/readme"), b"stuff\n")],
        );
        let material = [material_with_whdload_and_skick(dir)];
        let parts = [
            PartitionInput {
                volume_name: "Games".into(),
                sources: vec![hdf],
                floor_bytes: 0,
            },
            PartitionInput {
                volume_name: "Stuff".into(),
                sources: vec![stuff],
                floor_bytes: 0,
            },
        ];
        let mut measured = measure_card(
            16,
            &tree,
            &parts,
            &material,
            None,
            &dir.join("driver"),
            &crate::core::cardos::prepare::HstImagerState::Usable,
            &UtcClock,
            &NoProgress,
        )
        .unwrap();
        let plan = two_gib_plan(&["System", "Games", "Stuff", "Work"]);
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
            std::slice::from_ref(&roms),
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

    /// WHDLoad's binary as far as ART reads it: bytes that state a version.
    pub(crate) fn whdload_bytes(version: &str) -> Vec<u8> {
        let mut bytes = vec![0u8; 286];
        bytes.extend_from_slice(
            format!("$VER: WHDLoad {version} [build 7051] (27.03.2026)\0").as_bytes(),
        );
        bytes
    }

    /// The fixture's "Kickstart 1.3": arbitrary bytes of a 256 KB ROM's size,
    /// whose CRC-16/ARC the slave names.
    pub(crate) fn rom_13() -> Vec<u8> {
        vec![0x11u8; 262_144]
    }

    /// Every file on PFS3 partition `index` of the card's first Amiga area,
    /// by its `/`-separated path, read through libpfs3 at the partition's
    /// file-absolute offset. Directories are listed with an empty value under
    /// `<path>/`.
    pub(crate) fn read_pfs3_files(
        image: &Path,
        card: &crate::core::card::CardImage,
        index: usize,
    ) -> std::collections::BTreeMap<String, Vec<u8>> {
        let area = &card.areas[0];
        let part = &area.rdb.partitions[index];
        let (offset, _, _) = crate::core::preload::native::partition_region(area, part).unwrap();
        let mut vol = libpfs3::volume::Volume::open(image, offset).unwrap();
        let mut out = std::collections::BTreeMap::new();
        let mut stack = vec![String::new()];
        let mut nodes = 0usize;
        while let Some(dir) = stack.pop() {
            for entry in vol.list_dir(&dir).unwrap() {
                nodes += 1;
                assert!(nodes < 100_000, "a bounded walk");
                let path = if dir.is_empty() {
                    entry.name.clone()
                } else {
                    format!("{dir}/{}", entry.name)
                };
                if entry.is_dir() {
                    out.insert(format!("{path}/"), Vec::new());
                    stack.push(path);
                } else {
                    let bytes = vol.read_file_data(entry.anode, entry.file_size()).unwrap();
                    out.insert(path, bytes);
                }
            }
        }
        out
    }

    pub(crate) fn request_agreeing(dir: &Path, agreed: &[&str]) -> CardOsBuildRequest {
        let mut request = request_for(dir);
        request.agreed_kickstarts = agreed.iter().map(|name| name.to_string()).collect();
        request
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

        // I3: a refusal is its own ending, never a failure.
        match &built.ending {
            CardOsEnding::Refused { phase, code, .. } => {
                assert_eq!(*phase, BuildPhase::Whdload);
                assert_eq!(code, "ART-KICKSTART-NOT-PROPOSED");
            }
            other => panic!("expected a Whdload refusal, got {other:?}"),
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
        let mut request = request_agreeing(&dir, &["kick34005.A500"]);
        request.archive = dir.join("not-there.zip").display().to_string();
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

        // M11: the archive's own refusal, not any Card-phase ending.
        match &built.ending {
            CardOsEnding::Refused {
                phase: BuildPhase::Card,
                code,
                message,
            } => {
                assert_eq!(code, built.error.as_ref().unwrap().code());
                assert!(message.contains("not-there.zip"), "{message}");
            }
            other => panic!("expected a Card refusal, got {other:?}"),
        }
        assert_eq!(built.partial, PartialRemoval::NotCreated);
        assert!(!partial_path_for(&image).exists() && !image.exists());
        // M1: refused before the Whdload phase wrote anything — no Kickstart
        // is reported placed, and the tree is as it was.
        assert!(built.kickstarts.is_empty(), "{:?}", built.kickstarts);
        assert!(built.whdload.is_none());
        assert_eq!(listing(&tree), before, "the tree is unchanged");
    }

    /// I1: a card whose Stuff was prepared for hst-imager is refused before
    /// the image exists when the build has no hst-imager, or one that does
    /// not run — never failed at Stuff's format with the card half written.
    #[test]
    fn a_card_prepared_for_hst_imager_is_refused_before_the_image_without_a_usable_one() {
        /// A tool that does not run.
        struct Broken;
        impl VolumeFormatter for Broken {
            fn probe(&self) -> CoreResult<ToolVersion> {
                Err(CoreError::InvalidInput(
                    "could not run 'E:\\tools\\hst.imager.exe': not found".into(),
                ))
            }
            fn format_partition(
                &self,
                _: &Path,
                _: Option<usize>,
                _: usize,
                _: &str,
                _: &dyn ProgressSink,
            ) -> CoreResult<()> {
                panic!("a tool that does not run was asked to format")
            }
            fn can_copy_in(&self, _: &Path, _: Option<usize>, _: &str, _: &Path) -> CoreResult<()> {
                Ok(())
            }
            fn copy_in(
                &self,
                _: &Path,
                _: Option<usize>,
                _: &str,
                _: &Path,
                _: &dyn ProgressSink,
            ) -> CoreResult<CopySummary> {
                panic!("a tool that does not run was asked to copy")
            }
        }

        let (_guard, dir) = scratch("hst-gate");
        let (prepared, tree) = small_prepared_card_with(&dir, "Café");
        assert_eq!(
            prepared.measured.partitions[1].writer,
            crate::core::cardos::prepare::PartitionWriter::HstImager,
            "the fixture must send Stuff to hst-imager, or this proves nothing"
        );
        let before = listing(&tree);

        let given = dir.join("tools").join("hst.imager.exe");
        for (arm, fallback) in [
            ("none", None),
            (
                "broken",
                Some(HstFallback {
                    path: &given,
                    tool: &Broken as &dyn VolumeFormatter,
                }),
            ),
        ] {
            let image = dir.join(format!("card-{arm}.img"));
            let built = build_card_os(
                &prepared,
                &tree,
                &image,
                &request_agreeing(&dir, &["kick34005.A500"]),
                &NativeFormatter::UTC,
                fallback,
                &NoProgress,
            );
            match &built.ending {
                CardOsEnding::Refused {
                    phase: BuildPhase::Partitions,
                    code,
                    message,
                } => {
                    assert_eq!(code, "ART-HST-IMAGER-UNUSABLE", "{arm}");
                    assert!(message.contains("Stuff"), "{arm}: {message}");
                    if arm == "broken" {
                        // N1: a tool was given — the sentence says which one
                        // and carries the tool's own error, never "none given".
                        assert!(
                            !message.contains("no hst.imager.exe was given"),
                            "{arm}: {message}"
                        );
                        assert!(
                            message
                                .contains("could not run 'E:\\tools\\hst.imager.exe': not found"),
                            "{arm}: the tool's own error: {message}"
                        );
                        assert!(
                            message.contains(&format!("'{}'", given.display())),
                            "{arm}: the path the preparation probed: {message}"
                        );
                    }
                }
                other => panic!("{arm}: expected a Partitions refusal, got {other:?}"),
            }
            assert_eq!(built.partial, PartialRemoval::NotCreated, "{arm}");
            assert!(
                !partial_path_for(&image).exists() && !image.exists(),
                "{arm}"
            );
            assert_eq!(listing(&tree), before, "{arm}: the tree is unchanged");
        }
    }

    /// ART-343's ROM half, through the build: the agreed Kickstart's file
    /// changed after the card was prepared, and the build is refused at
    /// Whdload by name — before the tree or the image is touched.
    #[test]
    fn a_kickstart_changed_since_prepare_is_refused_before_the_tree_or_the_image() {
        let (_guard, dir) = scratch("rom-changed");
        let (prepared, tree) = small_prepared_card(&dir);
        let before = listing(&tree);
        let rom = dir.join("roms").join("my-13.rom");
        let mut bytes = std::fs::read(&rom).unwrap();
        bytes[100] ^= 0xFF;
        std::fs::write(&rom, &bytes).unwrap();
        let image = dir.join("card.img");

        let built = build_card_os(
            &prepared,
            &tree,
            &image,
            &request_agreeing(&dir, &["kick34005.A500"]),
            &NativeFormatter::UTC,
            None,
            &NoProgress,
        );

        match &built.ending {
            CardOsEnding::Refused {
                phase: BuildPhase::Whdload,
                code,
                message,
            } => {
                assert_eq!(code, "ART-KICKSTART-SOURCE-CHANGED", "{message}");
                assert!(message.contains(&rom.display().to_string()), "{message}");
                assert!(message.contains("prepare the card again"), "{message}");
            }
            other => panic!("expected a Whdload refusal, got {other:?}"),
        }
        assert_eq!(built.partial, PartialRemoval::NotCreated);
        assert!(!partial_path_for(&image).exists() && !image.exists());
        assert!(built.kickstarts.is_empty(), "{:?}", built.kickstarts);
        assert_eq!(listing(&tree), before, "the tree is unchanged");
    }

    /// N5 (the survivor I2b): the card writer fails after it created the
    /// `.partial` — here a stop asked at its first check once the file exists
    /// (`lay_out`) — and the ending, through `build_card_os`, reports that
    /// file removed, and it is gone. Never `NotCreated` for a file that was made.
    #[test]
    fn a_card_writer_stopped_after_creating_the_partial_reports_it_removed() {
        /// Asks to stop from the first check after `.partial` exists.
        struct StopOnceCreated {
            partial: PathBuf,
            saw_it: AtomicBool,
        }
        impl ProgressSink for StopOnceCreated {
            fn report(&self, _: u64, _: Option<u64>, _: &str) {}
            fn is_cancelled(&self) -> bool {
                if self.partial.exists() {
                    self.saw_it.store(true, Ordering::SeqCst);
                    true
                } else {
                    false
                }
            }
        }

        let (_guard, dir) = scratch("stop-after-create");
        let (prepared, tree) = small_prepared_card(&dir);
        let image = dir.join("card.img");
        let partial = partial_path_for(&image);
        let sink = StopOnceCreated {
            partial: partial.clone(),
            saw_it: AtomicBool::new(false),
        };

        let built = build_card_os(
            &prepared,
            &tree,
            &image,
            &request_agreeing(&dir, &["kick34005.A500"]),
            &NativeFormatter::UTC,
            None,
            &sink,
        );

        assert!(
            sink.saw_it.load(Ordering::SeqCst),
            "the stop must come after the file was created, or this proves nothing"
        );
        assert!(
            matches!(
                built.ending,
                CardOsEnding::Stopped {
                    phase: BuildPhase::Card
                }
            ),
            "{:?} {:?}",
            built.ending,
            built.error
        );
        assert_eq!(
            built.partial,
            PartialRemoval::Removed {
                path: partial.display().to_string()
            }
        );
        assert!(!partial.exists(), "the .partial is gone");
        assert!(!image.exists());
    }

    /// N6: a finish that left both names is said as ART's own card, never
    /// rewritten into "appeared while ART was building it" because the image
    /// name exists — it exists because ART linked it. The control: a name
    /// that did appear is still said as such.
    #[test]
    fn a_finish_that_left_both_names_is_not_called_somebody_else_s_file() {
        let (_guard, dir) = scratch("finish-error");
        let image = dir.join("card.img");
        std::fs::write(&image, b"card").unwrap();

        let both = CoreError::CardFinishLeftBothNames {
            image: image.display().to_string(),
            partial: partial_path_for(&image).display().to_string(),
            why: "in use; and the new name: in use".into(),
        };
        let said = finish_error(&image, both).to_string();
        assert!(said.contains("ART's own half-built card"), "{said}");
        assert!(!said.contains("appeared"), "{said}");

        let raced = finish_error(&image, CoreError::SafetyRefused("x".into())).to_string();
        assert!(
            raced.contains("appeared while ART was building it"),
            "{raced}"
        );
    }

    /// M17: a manifest already beside the image name is someone's record, and
    /// the build is refused before the image rather than writing over it.
    #[test]
    fn a_manifest_already_beside_the_image_is_refused_before_the_image() {
        let (_guard, dir) = scratch("manifest-there");
        let (prepared, tree) = small_prepared_card(&dir);
        let image = dir.join("card.img");
        std::fs::write(manifest_path_for(&image), b"{\"theirs\":true}").unwrap();

        let built = build_card_os(
            &prepared,
            &tree,
            &image,
            &request_for(&dir),
            &NativeFormatter::UTC,
            None,
            &NoProgress,
        );

        match &built.ending {
            CardOsEnding::Refused {
                phase: BuildPhase::Card,
                message,
                ..
            } => assert!(message.contains("card.img.manifest.json"), "{message}"),
            other => panic!("expected a Card refusal, got {other:?}"),
        }
        assert!(!partial_path_for(&image).exists() && !image.exists());
        assert_eq!(
            std::fs::read(manifest_path_for(&image)).unwrap(),
            b"{\"theirs\":true}"
        );
    }

    /// I2: what the card writer did with the file it created is what the
    /// ending says — never `NotCreated` for a file that was made.
    #[test]
    fn a_card_that_failed_after_creation_reports_its_partial_as_it_was_left() {
        let image = Path::new(r"E:\cards\card.img");
        let partial = partial_path_for(image).display().to_string();
        assert_eq!(
            partial_after_card_failure(image, &ImageLeft::NotCreated),
            PartialRemoval::NotCreated
        );
        assert_eq!(
            partial_after_card_failure(image, &ImageLeft::Removed),
            PartialRemoval::Removed {
                path: partial.clone()
            }
        );
        assert_eq!(
            partial_after_card_failure(
                image,
                &ImageLeft::NotRemoved {
                    why: "in use".into()
                }
            ),
            PartialRemoval::NotRemoved {
                path: partial,
                why: "in use".into()
            }
        );
    }

    /// T12 (the ledger's cheap test): the operation log's record for a build
    /// says its ending, what became of the partial, and each Kickstart with
    /// what happened to it — pure, so no Tauri harness is needed.
    #[test]
    fn the_build_record_says_the_ending_the_partial_and_each_kickstart_s_outcome() {
        use crate::core::rom::place::PlaceOutcome;

        let detail = |record: &OperationRecord, key: &str| {
            record
                .details
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| panic!("no '{key}' in {:?}", record.details))
        };
        let (_guard, dir) = scratch("record");
        let request = request_for(&dir);
        let built = |ending, partial| BuiltCardOs {
            ending,
            whdload: None,
            kickstarts: vec![PlacedKickstart {
                name: "kick34005.A500".into(),
                image: PlaceOutcome::Occupied {
                    to: "tree/Devs/Kickstarts/kick34005.A500".into(),
                },
                rtb: PlaceOutcome::Placed {
                    to: "tree/Devs/Kickstarts/kick34005.A500.RTB".into(),
                    bytes: 4000,
                },
            }],
            steps: Vec::new(),
            manifest_path: None,
            health: None,
            partial,
            error: Some(CoreError::Cancelled),
        };

        let stopped = build_record(
            &request,
            "E:\\cards\\card.img",
            &built(
                CardOsEnding::Stopped {
                    phase: BuildPhase::Partitions,
                },
                PartialRemoval::NotRemoved {
                    path: "E:\\cards\\card.img.partial".into(),
                    why: "in use".into(),
                },
            ),
            None,
        );
        assert_eq!(detail(&stopped, "Ending"), "stopped");
        assert_eq!(detail(&stopped, "Phase reached"), "partitions");
        assert_eq!(
            detail(&stopped, "Partial image"),
            "NOT removed: E:\\cards\\card.img.partial (in use)"
        );
        let kickstarts = detail(&stopped, "Kickstarts");
        assert!(
            kickstarts.contains("kick34005.A500")
                && kickstarts.contains("a different file was already there")
                && kickstarts.contains("removed with the session folder"),
            "{kickstarts}"
        );

        let refused = build_record(
            &request,
            "E:\\cards\\card.img",
            &built(
                CardOsEnding::Refused {
                    phase: BuildPhase::Card,
                    code: "ART-X".into(),
                    message: "x".into(),
                },
                PartialRemoval::NotCreated,
            ),
            None,
        );
        assert_eq!(detail(&refused, "Ending"), "refused");
        assert_eq!(detail(&refused, "Partial image"), "not created");
    }

    /// T13 (deferred minor): the manifest names the tool that wrote a
    /// partition, not a bare version string.
    #[test]
    fn a_partition_s_writer_is_named_with_its_tool() {
        assert_eq!(writer_name("native"), "native");
        assert_eq!(
            writer_name("1.6.616+a91fa4ca"),
            "hst-imager 1.6.616+a91fa4ca"
        );
    }

    /// M7: an image path the screen did not fill in, or one relative to
    /// wherever the process happens to run, is refused with its next step.
    #[test]
    fn a_blank_or_relative_image_path_is_refused() {
        for given in ["", "   ", "card.img", r"cards\card.img"] {
            let err = image_path_from(given).unwrap_err();
            assert_eq!(err.code(), "ART-INPUT-INVALID", "{given:?}");
            assert!(err.to_string().contains("full path"), "{given:?}: {err}");
        }
        assert_eq!(
            image_path_from(r"  E:\cards\card.img ").unwrap(),
            PathBuf::from(r"E:\cards\card.img")
        );
    }

    /// I1: the preparation asks the hst-imager it was given, and a tool that
    /// does not run is refused by name for the partition that needs it.
    #[test]
    fn a_preparation_asks_the_hst_imager_it_was_given() {
        let (_guard, dir) = scratch("prepare-probe");
        small_tree(&dir);
        let material = material_with_driver(&dir);
        let stuff = folder_with(&dir, "Stuff", &[("Café/readme", b"x")]);
        let tool = r"E:\tools\hst.imager.exe";
        let request = CardOsPrepareRequest {
            session: 1,
            card_gb: 16,
            image: dir.join("card.img").display().to_string(),
            partitions: vec![PartitionInput {
                volume_name: "Stuff".into(),
                sources: vec![stuff],
                floor_bytes: 0,
            }],
            material: vec![material.display().to_string()],
            pfs3_driver: None,
            kickstart: None,
            hst_imager_path: tool.into(),
        };
        let asked = std::sync::Mutex::new(Vec::new());

        let err = prepare_card(
            &request,
            &dir.join("card.img"),
            &dir,
            &crate::core::clock::UtcClock,
            |_| Ok(u64::MAX),
            |path: &Path| {
                asked.lock().unwrap().push(path.to_path_buf());
                Err("the system cannot find the file specified".to_string())
            },
            &NoProgress,
        )
        .unwrap_err();

        assert_eq!(err.code(), "ART-HST-IMAGER-UNUSABLE", "{err}");
        assert!(
            err.to_string().contains("Stuff") && err.to_string().contains(tool),
            "{err}"
        );
        assert_eq!(*asked.lock().unwrap(), vec![PathBuf::from(tool)]);
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
        let tool = PathBuf::from(r"E:\tools\hst.imager.exe");
        sessions.end_prepare(
            id,
            Some(PreparedSession {
                prepared,
                image: dir.join("card.img"),
                hst_imager: Some(tool.clone()),
            }),
        );

        let ticket = sessions.take_for_build(id).unwrap();
        assert_eq!(ticket.image, dir.join("card.img"));
        // I1: the build uses the hst-imager the preparation asked.
        assert_eq!(ticket.hst_imager, Some(tool));
        let err = match sessions.take_for_build(id) {
            Ok(_) => panic!("a second build of one session was allowed"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("busy"), "{err}");
        // And a busy session is not closed from under its build.
        assert!(sessions.close(id).unwrap_err().to_string().contains("busy"));

        assert_eq!(sessions.end_build(id), None);
    }

    // -----------------------------------------------------------------------
    // Task 13: the end-to-end card
    // -----------------------------------------------------------------------

    /// The whole card, built by the real path from the plan on, and read back
    /// by ART's own readers: the RDB, System and Games through libpfs3, the
    /// manifest from its file beside the image.
    #[test]
    fn a_small_card_is_built_whole_and_reads_back_as_it_was_asked() {
        use crate::core::card::health::ManualStep;
        use crate::core::card::manifest::read_manifest;

        let (_guard, dir) = scratch("e2e");
        let (prepared, tree) = small_prepared_card(&dir);
        let image = dir.join("card.img");

        let built = build_card_os(
            &prepared,
            &tree,
            &image,
            &request_agreeing(&dir, &["kick34005.A500"]),
            &NativeFormatter::UTC,
            None,
            &NoProgress,
        );
        assert!(
            matches!(built.ending, CardOsEnding::Succeeded),
            "{:?} {:?}",
            built.ending,
            built.error
        );
        assert!(image.exists() && !partial_path_for(&image).exists());

        let card = crate::core::card::read_card(&image).unwrap();
        let parts: Vec<_> = card.areas[0]
            .rdb
            .partitions
            .iter()
            .map(|p| p.drive_name.clone())
            .collect();
        assert_eq!(parts, ["SDH0", "SDH1", "SDH2", "SDH3"]);

        let system = read_pfs3_files(&image, &card, 0);
        assert_eq!(
            system.get("C/WHDLoad"),
            Some(&whdload_bytes("20.0")),
            "{:?}",
            system.keys().collect::<Vec<_>>()
        );
        assert_eq!(
            system.get("S/WHDLoad.prefs").map(Vec::as_slice),
            Some(&b";prefs\n"[..])
        );
        assert_eq!(
            system.get("Devs/Kickstarts/kick34005.A500"),
            Some(&rom_13())
        );
        assert_eq!(
            system.get("Devs/Kickstarts/kick34005.A500.RTB"),
            Some(&vec![1u8; 4000])
        );
        assert_eq!(system.get("C/Dir"), Some(&vec![0u8; 3000]));

        let games = read_pfs3_files(&image, &card, 1);
        assert!(
            games.keys().any(|k| k == "Turrican/Turrican.slave"),
            "{:?}",
            games.keys().collect::<Vec<_>>()
        );
        let stuff = read_pfs3_files(&image, &card, 2);
        assert_eq!(
            stuff.get("Notes/readme").map(Vec::as_slice),
            Some(&b"stuff\n"[..])
        );
        assert!(
            read_pfs3_files(&image, &card, 3).is_empty(),
            "Work is empty"
        );

        let manifest = read_manifest(&manifest_path_for(&image)).unwrap();
        assert_eq!(manifest.os, vec!["AmigaOS 3.2.2".to_string()]);
        let volumes: Vec<_> = manifest
            .partitions
            .iter()
            .map(|p| p.volume_name.as_str())
            .collect();
        assert_eq!(volumes, ["System", "Games", "Stuff", "Work"], "R3");
        // I4: the manifest records what is on the card — System's files
        // include WHDLoad, its prefs, the Kickstart and its .RTB, which the
        // build wrote after the tree was measured — counted and summed from
        // the card, for every partition.
        for (index, recorded) in manifest.partitions.iter().enumerate() {
            let on_card = read_pfs3_files(&image, &card, index);
            let files: Vec<&Vec<u8>> = on_card
                .iter()
                .filter(|(path, _)| !path.ends_with('/'))
                .map(|(_, bytes)| bytes)
                .collect();
            assert_eq!(
                (recorded.files, recorded.bytes),
                (
                    files.len() as u64,
                    files.iter().map(|b| b.len() as u64).sum::<u64>()
                ),
                "{}",
                recorded.volume_name
            );
            assert_eq!(recorded.writer, "native", "{}", recorded.volume_name);
        }
        assert_eq!(
            manifest.partitions[0].files, 7,
            "System, as hst-imager counted it"
        );
        let health = built.health.expect("a health report");
        assert!(
            !health
                .by_hand
                .iter()
                .any(|s| matches!(s, ManualStep::VolumesNeedFormatting { .. })),
            "{:?}",
            health.by_hand
        );
    }

    /// Decision 1, the other direction: a Kickstart the proposal supplies but
    /// the user did not agree to never reaches the card.
    #[test]
    fn only_agreed_kickstarts_reach_the_tree() {
        let (_guard, dir) = scratch("only-agreed");
        let (prepared, tree) = small_prepared_card(&dir);
        assert!(
            prepared
                .kickstarts
                .items
                .iter()
                .any(|item| item.name == "kick34005.A500"
                    && matches!(item.offer, crate::core::rom::offer::Offer::Supplied { .. })),
            "the fixture must offer the image, or this proves nothing: {:?}",
            prepared.kickstarts.items
        );
        let image = dir.join("card.img");

        let built = build_card_os(
            &prepared,
            &tree,
            &image,
            &request_agreeing(&dir, &[]),
            &NativeFormatter::UTC,
            None,
            &NoProgress,
        );
        assert!(
            matches!(built.ending, CardOsEnding::Succeeded),
            "{:?} {:?}",
            built.ending,
            built.error
        );
        assert!(built.kickstarts.is_empty(), "{:?}", built.kickstarts);

        let card = crate::core::card::read_card(&image).unwrap();
        let system = read_pfs3_files(&image, &card, 0);
        assert!(
            !system.keys().any(|k| k.starts_with("Devs/Kickstarts/")),
            "{:?}",
            system.keys().collect::<Vec<_>>()
        );
        assert!(!tree.join("Devs").join("Kickstarts").exists());
        // The rest of the card is still there: this is not a card that failed.
        assert_eq!(system.get("C/WHDLoad"), Some(&whdload_bytes("20.0")));
    }

    /// Writes a card for `scripts/pfs3-oracle-check.py --card`, and ART's own
    /// listing of every partition beside it. With `ART_HST` naming
    /// `hst.imager.exe`, Stuff holds a non-ASCII name, so Stuff is formatted
    /// and filled by hst-imager — under the `.partial` name (P2's write half).
    #[test]
    #[ignore = "writes a card for scripts/pfs3-oracle-check.py --card; set ART_CARD_OS_OUT to an existing folder on E: (and ART_HST for the hst-imager run)"]
    fn writes_a_small_card_for_the_hst_imager_oracle() {
        use sha2::Digest as _;

        let out = PathBuf::from(std::env::var("ART_CARD_OS_OUT").expect("ART_CARD_OS_OUT"));
        assert!(out.is_dir(), "ART_CARD_OS_OUT must be an existing folder");
        let hst = std::env::var("ART_HST")
            .ok()
            .filter(|p| !p.trim().is_empty());
        let (stem, stuff_drawer) = match hst {
            Some(_) => ("card-os-e2e-hst", "Café"),
            None => ("card-os-e2e", "Notes"),
        };
        let image = out.join(format!("{stem}.img"));
        let listing_path = out.join(format!("{stem}.art.json"));
        assert!(
            !image.exists() && !partial_path_for(&image).exists(),
            "'{}' (or its .partial) already exists — ART does not replace it",
            image.display()
        );

        let (_guard, dir) = scratch("oracle-hook");
        let (prepared, tree) = small_prepared_card_with(&dir, stuff_drawer);
        let fallback = hst
            .as_ref()
            .map(|path| (PathBuf::from(path), HstImager::at(path)));
        let started = std::time::Instant::now();
        let built = build_card_os(
            &prepared,
            &tree,
            &image,
            &request_agreeing(&dir, &["kick34005.A500"]),
            &NativeFormatter::UTC,
            fallback.as_ref().map(|(path, tool)| HstFallback {
                path: path.as_path(),
                tool: tool as &dyn VolumeFormatter,
            }),
            &NoProgress,
        );
        assert!(
            matches!(built.ending, CardOsEnding::Succeeded),
            "{:?} {:?}",
            built.ending,
            built.error
        );
        println!("built in {:.1} s", started.elapsed().as_secs_f64());
        for step in &built.steps {
            println!("step {:?} by {}", step.step, step.tool);
        }

        let card = crate::core::card::read_card(&image).unwrap();
        let area = &card.areas[0];
        let mut partitions = Vec::new();
        for (index, part) in area.rdb.partitions.iter().enumerate() {
            let files = read_pfs3_files(&image, &card, index);
            let entries: Vec<serde_json::Value> = files
                .iter()
                .map(|(path, bytes)| match path.strip_suffix('/') {
                    Some(dir) => serde_json::json!({ "path": dir, "dir": true }),
                    None => serde_json::json!({
                        "path": path,
                        "size": bytes.len(),
                        "sha256": format!("{:x}", sha2::Sha256::digest(bytes)),
                    }),
                })
                .collect();
            println!(
                "{}: {} files, {} dirs",
                part.drive_name,
                files.keys().filter(|k| !k.ends_with('/')).count(),
                files.keys().filter(|k| k.ends_with('/')).count()
            );
            partitions.push(serde_json::json!({
                "slot": area.mbr_slot,
                "drive": part.drive_name,
                "entries": entries,
            }));
        }
        let listing =
            serde_json::json!({ "image": image.display().to_string(), "partitions": partitions });
        std::fs::write(
            &listing_path,
            serde_json::to_string_pretty(&listing).unwrap(),
        )
        .unwrap();
        println!("wrote {} and {}", image.display(), listing_path.display());
    }
}
