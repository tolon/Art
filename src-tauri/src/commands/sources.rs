//! Aminet Studio commands — the shell over `core/sources` (§41.5).
//!
//! Thin adapters, as everywhere else: deserialize, call core, serialize back.
//! Two things are decided here rather than in core, because they are shell
//! concerns:
//!
//! - **Anything that touches the network is a job** (§54, §55). Not just the
//!   7 MB index sync and package downloads — readmes too. A readme is a few
//!   kilobytes, but a mirror that has stopped answering does not know that, and
//!   a command thread blocked on a connect timeout is a frozen window.
//! - **The catalog and cache live in the app data directory**, resolved once at
//!   startup and held in [`SourcesState`].
//!
//! Every background result arrives on one `sources-result` event, tagged by
//! kind, so the frontend has a single listener rather than three.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use super::jobs::{spawn_job, JobRegistry};
use super::oplog::user_operation;
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::JobId;
use crate::core::lha::OverwritePolicy;
use crate::core::oplog::{JsonlOperationLog, OperationOutcome, OperationRecord};
use crate::core::sources::cache::CacheLayout;
use crate::core::sources::catalog::jsonl::JsonlCatalogStore;
use crate::core::sources::catalog::{
    resolve, CatalogStats, CatalogStore, Resolution, SearchFilters, SortOrder, SourceQuery,
};
use crate::core::sources::fetch::{fetch_package, FetchOutcome};
use crate::core::sources::install::InstallOutcome;
use crate::core::sources::installed::{
    check_updates, DownloadRecord, DownloadRecordStore, JsonlDownloadRecords, PackageUpdate,
};
use crate::core::sources::library::{Library, Placement};
use crate::core::sources::mirror::Mirror;
use crate::core::sources::readme::{parse_readme_bytes, ReadmeFields};
use crate::core::sources::sync::{aminet_defaults, sync_catalog, ProviderConfig, SyncOutcome};
use crate::core::sources::PackageMeta;
use crate::error::AppResult;
use crate::net::http_mirror::HttpMirrorClient;

/// The event every background sources result arrives on.
pub const SOURCES_EVENT: &str = "sources-result";

/// The most bytes ART will read of a readme.
const MAX_README_BYTES: u64 = 1024 * 1024;

/// Everything the sources commands need, resolved once at startup.
pub struct SourcesState {
    catalog: JsonlCatalogStore,
    cache: CacheLayout,
    client: HttpMirrorClient,
    /// Mirrors are user-configurable (§41.5.6), so this can change at runtime.
    provider: Mutex<ProviderConfig>,
    /// Where downloads are kept for the user. Also user-chosen, also runtime.
    library: Mutex<Library>,
    /// What has been downloaded and what the catalogue said at the time, so a
    /// later sync can be compared against it (§41.5.6).
    records: JsonlDownloadRecords,
}

impl SourcesState {
    /// Load the catalog from `root`, or start with an empty one.
    ///
    /// A failure to read an existing catalog is *not* fatal: ART works with no
    /// catalog at all, and refusing to start over a damaged cache file would
    /// break the offline-first rule (§60/§94) for no gain.
    pub fn new(root: PathBuf, default_library_root: PathBuf) -> Self {
        let catalog_path = root.join("catalog.jsonl");
        let catalog = match JsonlCatalogStore::load(&catalog_path) {
            Ok((store, report)) => {
                if report.damaged > 0 {
                    log::warn!(
                        "catalog: {} entries loaded, {} damaged lines skipped",
                        report.loaded,
                        report.damaged
                    );
                }
                store
            }
            Err(err) => {
                log::warn!("catalog could not be read ({err}); starting empty");
                JsonlCatalogStore::empty_at(catalog_path)
            }
        };

        let provider = aminet_defaults().unwrap_or_else(|err| {
            // The defaults are asserted valid by a test; this branch exists so
            // a typo in a future edit degrades to "no mirrors" rather than
            // taking the whole application down at startup.
            log::error!("the built-in Aminet mirrors are invalid: {err}");
            ProviderConfig {
                id: crate::core::sources::PROVIDER_AMINET.into(),
                index_path: "INDEX".into(),
                mirrors: Vec::new(),
            }
        });

        // Same reasoning as the catalog above: unreadable records cost the
        // update view, not the application.
        let records_path = root.join("downloads.jsonl");
        let records = JsonlDownloadRecords::load(&records_path).unwrap_or_else(|err| {
            log::warn!("download records could not be read ({err}); starting empty");
            JsonlDownloadRecords::empty_at(records_path)
        });

        Self {
            catalog,
            cache: CacheLayout::new(root.join("cache")),
            client: HttpMirrorClient::new(),
            provider: Mutex::new(provider),
            library: Mutex::new(Library::new(default_library_root)),
            records,
        }
    }

    fn provider(&self) -> ProviderConfig {
        self.provider
            .lock()
            .map(|p| p.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
    }

    fn library(&self) -> Library {
        self.library
            .lock()
            .map(|l| l.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
    }
}

// ---------------------------------------------------------------------------
// Read-only: served entirely from the local catalog, no network (§41.5.2)
// ---------------------------------------------------------------------------

/// How much the local catalog holds.
#[tauri::command]
pub fn sources_stats(state: State<'_, SourcesState>) -> AppResult<CatalogStats> {
    Ok(state.catalog.stats()?)
}

/// Everything the search screen can narrow or reorder by.
///
/// One struct rather than a growing argument list, so adding a filter is a
/// change in two places (here and `src/lib/sources.ts`) instead of five.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SearchOptions {
    pub directory: Option<String>,
    pub limit: Option<usize>,
    pub sort: SortOrder,
    pub filters: SearchFilters,
}

/// Search the local catalog. Works with nothing connected.
#[tauri::command]
pub fn sources_search(
    query: String,
    options: Option<SearchOptions>,
    state: State<'_, SourcesState>,
) -> AppResult<Vec<PackageMeta>> {
    let options = options.unwrap_or_default();

    let mut search = SourceQuery::text(query)
        .sorted_by(options.sort)
        .filtered(options.filters);
    if let Some(directory) = options.directory {
        search = search.in_directory(directory);
    }
    if let Some(limit) = options.limit {
        search = search.limit(limit);
    }

    Ok(state.catalog.search(&search)?)
}

/// Which release of a package to offer, with any version disagreement intact.
#[tauri::command]
pub fn sources_resolve(
    directory: String,
    stem: String,
    provider: Option<String>,
    state: State<'_, SourcesState>,
) -> AppResult<Option<Resolution>> {
    let provider = provider.unwrap_or_else(|| state.provider().id);
    let candidates = state.catalog.in_directory(&provider, &directory)?;
    Ok(resolve(&candidates, &stem))
}

/// A mirror as the UI sees it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorInfo {
    pub name: String,
    pub base_url: String,
}

/// The configured provider: id, index path and mirror order.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderInfo {
    pub id: String,
    pub index_path: String,
    pub mirrors: Vec<MirrorInfo>,
    /// Where downloads are cached. Power User Mode shows it; Beginner does not.
    pub cache_path: String,
}

fn describe(provider: &ProviderConfig, cache: &CacheLayout) -> ProviderInfo {
    ProviderInfo {
        id: provider.id.clone(),
        index_path: provider.index_path.clone(),
        mirrors: provider
            .mirrors
            .iter()
            .map(|m| MirrorInfo {
                name: m.name.clone(),
                base_url: m.base_url().to_string(),
            })
            .collect(),
        cache_path: cache.root().display().to_string(),
    }
}

#[tauri::command]
pub fn sources_provider(state: State<'_, SourcesState>) -> AppResult<ProviderInfo> {
    Ok(describe(&state.provider(), &state.cache))
}

/// The download folder and the subfolders already in it.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryInfo {
    pub root: String,
    /// Existing subfolders, for the destination picker. Depth- and
    /// count-limited; the user can always type a new one.
    pub subfolders: Vec<String>,
}

fn describe_library(library: &Library) -> LibraryInfo {
    LibraryInfo {
        root: library.root().display().to_string(),
        // A folder that cannot be listed is shown as empty rather than as an
        // error: the user can still type a destination, and refusing to show
        // the screen over an unreadable folder helps nobody.
        subfolders: library.subfolders().unwrap_or_default(),
    }
}

/// Where downloads are being kept, and what is already there.
#[tauri::command]
pub fn sources_library(state: State<'_, SourcesState>) -> AppResult<LibraryInfo> {
    Ok(describe_library(&state.library()))
}

/// Choose the download folder.
///
/// Deliberately not validated against a whitelist of "sensible" locations —
/// it is the user's disk. The folder is created if it does not exist, and
/// everything written into it afterwards is confined to it by
/// [`Library`](crate::core::sources::library::Library).
#[tauri::command]
pub fn sources_set_library_root(
    path: String,
    state: State<'_, SourcesState>,
) -> AppResult<LibraryInfo> {
    let root = PathBuf::from(path.trim());
    if root.as_os_str().is_empty() {
        return Err(CoreError::InvalidInput("choose a download folder".into()).into());
    }
    if root.exists() && !root.is_dir() {
        return Err(CoreError::InvalidInput(format!(
            "'{}' is a file, not a folder",
            root.display()
        ))
        .into());
    }
    std::fs::create_dir_all(&root).map_err(CoreError::from)?;

    let library = Library::new(root);
    let info = describe_library(&library);
    *state
        .library
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = library;

    Ok(info)
}

/// Move a file already in the download folder to another subfolder.
///
/// The mirror image of choosing a destination at download time: §41.5.6's
/// "organise your downloads" is only real if it can be changed afterwards.
#[tauri::command]
pub fn sources_relocate(
    path: String,
    subfolder: String,
    overwrite: Option<OverwritePolicy>,
    state: State<'_, SourcesState>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<Placement> {
    let library = state.library();
    let source = PathBuf::from(&path);

    let result = library.relocate(&source, &subfolder, overwrite.unwrap_or_default());

    let record = user_operation("Move download").source(&path);
    let record = match &result {
        Ok(placement) => record
            .destination(placement.path.display().to_string())
            .detail("Folder", placement.subfolder.clone())
            .outcome(OperationOutcome::verified(!placement.skipped_existing)),
        Err(err) => record.failed(err),
    };
    super::oplog::write_to_path(oplog.path(), &record);

    Ok(result?)
}

/// Replace the mirror list (Power User Mode, §41.5.6).
///
/// Every entry goes through [`Mirror::new`], so a mirror that could address
/// something other than a plain HTTP(S) base is refused here rather than at
/// download time.
#[tauri::command]
pub fn sources_set_mirrors(
    mirrors: Vec<MirrorInfo>,
    state: State<'_, SourcesState>,
) -> AppResult<ProviderInfo> {
    let parsed = parse_mirrors(&mirrors)?;

    let mut held = state
        .provider
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    held.mirrors = parsed;

    Ok(describe(&held, &state.cache))
}

/// What ART has downloaded, and whether Aminet has moved on (§41.5.6).
///
/// Reads only the local catalogue and the local record file, so it answers
/// instantly and works offline — the answer is "as of your last sync", which
/// is exactly what the screen says. Nothing is fetched, replaced or deleted:
/// this produces a list, and the user decides.
#[tauri::command]
pub fn sources_check_updates(state: State<'_, SourcesState>) -> AppResult<Vec<PackageUpdate>> {
    Ok(check_updates(&state.records, &state.catalog, &|path| {
        path.exists()
    })?)
}

/// Drop one package from the update view.
///
/// For a download the user has since dealt with by hand. Deliberately does not
/// touch the file — forgetting a record and deleting someone's download are
/// very different things, and only one of them is reversible.
#[tauri::command]
pub fn sources_forget_download(
    path: String,
    provider: Option<String>,
    state: State<'_, SourcesState>,
) -> AppResult<()> {
    let provider = provider.unwrap_or_else(|| state.provider().id.clone());
    state
        .records
        .forget(&crate::core::sources::PackageRef::new(provider, path))?;
    Ok(())
}

/// Put the shipped mirror list back.
///
/// Separate from [`sources_set_mirrors`] rather than "pass an empty list":
/// an empty list there is an error the user needs to see, and overloading it
/// to mean "reset" would make a mistyped edit that deleted every row look like
/// a deliberate reset.
#[tauri::command]
pub fn sources_reset_mirrors(state: State<'_, SourcesState>) -> AppResult<ProviderInfo> {
    let mut held = state
        .provider
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    held.mirrors = aminet_defaults()?.mirrors;

    Ok(describe(&held, &state.cache))
}

/// Validate a mirror list from the UI.
///
/// An empty list is refused rather than accepted as "offline mode": it would
/// turn every later sync into `ART-MIRROR-UNREACHABLE` with no explanation of
/// why. Each entry then goes through [`Mirror::new`], so anything that could
/// address something other than a plain HTTP(S) base fails here, while the
/// user is looking at the field they typed it into.
fn parse_mirrors(mirrors: &[MirrorInfo]) -> CoreResult<Vec<Mirror>> {
    if mirrors.is_empty() {
        return Err(CoreError::InvalidInput(
            "at least one mirror is needed to sync or download".into(),
        ));
    }

    mirrors
        .iter()
        .map(|m| Mirror::new(m.name.clone(), &m.base_url))
        .collect()
}

// ---------------------------------------------------------------------------
// Background work
// ---------------------------------------------------------------------------

/// What a finished sources job delivers to the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourcesResult {
    Sync {
        job_id: JobId,
        outcome: SyncOutcome,
    },
    Fetch {
        job_id: JobId,
        path: String,
        outcome: FetchOutcome,
        /// Where the file was put in the user's download folder. The cache
        /// keeps the verified copy under its hash; this is the one with a name.
        placement: Placement,
    },
    Install {
        job_id: JobId,
        /// The image that was written to — a floppy or a hard disk, since the
        /// same install path now reaches both.
        image: String,
        outcome: InstallOutcome,
    },
    Readme {
        job_id: JobId,
        path: String,
        /// The readme verbatim. ART never strips a package's own licence
        /// text (§41.5.5), so this is shown as it arrived.
        text: String,
        fields: ReadmeFields,
    },
}

/// Sync the catalog from the configured mirrors.
///
/// Returns a job id; the outcome arrives on `sources-result`. A sync that comes
/// back too damaged to trust leaves the previous catalog in place and says so
/// in [`SyncOutcome::applied`].
#[tauri::command]
pub fn sources_sync(
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    state: State<'_, SourcesState>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<JobId> {
    let provider = state.provider();
    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();

    let id = spawn_job(
        &app,
        registry,
        "Syncing the Aminet catalog",
        move |job_id, progress| {
            // The state is managed by Tauri and outlives every job, so the
            // worker reaches it through the handle rather than capturing a
            // borrow across the thread boundary.
            let state = emit_app.state::<SourcesState>();
            let result = sync_catalog(&provider, &state.client, &state.catalog, progress);

            let record = user_operation("Sync software catalog")
                .source(provider_label(&provider))
                .destination(state.catalog.path().display().to_string());
            record_sync(&log_path, record, &result);

            let outcome = result?;
            let _ = emit_app.emit(SOURCES_EVENT, SourcesResult::Sync { job_id, outcome });
            Ok(())
        },
    );

    Ok(id)
}

/// Download a package through the §41.5.3 trust pipeline, then put a copy in
/// the user's download folder.
///
/// Two destinations on purpose. The cache is content-addressed and verified —
/// it is what makes a second download unnecessary and a corrupted file obvious.
/// The library copy is the one a person can find, name and move. Nothing is
/// installed and no image is opened either way.
/// Where a download should go and what to do if something is already there.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DownloadOptions {
    /// Relative to the download folder. Empty means the folder itself.
    pub subfolder: String,
    pub overwrite: OverwritePolicy,
    pub provider: Option<String>,
}

#[tauri::command]
pub fn sources_fetch(
    path: String,
    options: Option<DownloadOptions>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    state: State<'_, SourcesState>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<JobId> {
    let options = options.unwrap_or_default();
    let config = state.provider();
    let provider_id = options
        .provider
        .clone()
        .unwrap_or_else(|| config.id.clone());
    let reference = crate::core::sources::PackageRef::new(provider_id, &path);

    let meta = state
        .catalog
        .get(&reference)?
        .ok_or_else(|| CoreError::InvalidInput(format!("'{path}' is not in the catalog")))?;

    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!("Downloading {}", meta.name);
    let subfolder = options.subfolder;
    let policy = options.overwrite;

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let state = emit_app.state::<SourcesState>();
        let result = fetch_package(
            &meta,
            &config.mirrors,
            &state.client,
            &state.cache,
            progress,
        );

        // Placement only happens once the download has passed every gate, so a
        // package that failed verification never appears in the user's folder.
        let placed = result.as_ref().ok().map(|outcome| {
            state
                .library()
                .place(&outcome.path, &subfolder, &meta.name, policy)
        });

        let record = user_operation("Download package")
            .source(format!(
                "{}:{}",
                meta.reference.provider, meta.reference.path
            ))
            .detail("Catalog size", meta.size_bytes.to_string());
        record_fetch(&log_path, record, &result, placed.as_ref());

        let outcome = result?;
        let placement = placed.expect("a successful fetch always attempts placement")?;

        // Remember what the catalogue said *now*, so a later sync can be
        // compared against it (§41.5.6). Best-effort, like the operation log:
        // failing to remember a download must never fail the download.
        let remembered = state.records.record(DownloadRecord {
            reference: meta.reference.clone(),
            file_path: placement.path.display().to_string(),
            sha256: outcome.sha256.clone(),
            size_bytes: outcome.bytes,
            age_weeks: meta.age_weeks,
            version: meta.version.as_ref().map(|claim| claim.value.clone()),
        });
        if let Err(err) = remembered {
            log::warn!("could not record the download for the update view: {err}");
        }

        let _ = emit_app.emit(
            SOURCES_EVENT,
            SourcesResult::Fetch {
                job_id,
                path: meta.reference.path.clone(),
                outcome,
                placement,
            },
        );
        Ok(())
    });

    Ok(id)
}

/// The body of both install commands, factored out so it is callable — and
/// therefore testable — without a live Tauri `State` or a job runner. Same
/// shape task 8 gave the ADF commands (`add_file_at`, `create_directory_at`).
///
/// Everything that makes an install different from the general-purpose F5 copy
/// lives here, and both of those differences are guarantees a test has to be
/// able to reach:
///
/// - **It fits, or nothing happens.** An install that will not fit is refused
///   atomically with real block numbers, before a single block is written
///   (§92) — landing what fits and reporting the rest is right for a copy and
///   wrong for an install: a WHDLoad pack missing its `.slave` because it did
///   not fit is not a partial success, it is a broken result nobody was
///   warned about.
/// - **Cancelling installs nothing.** A cancelled batch never reaches the
///   user's file and never reports success (§54, §57).
///
/// Both guarantees come from
/// [`volume_write::install_into_folder`](crate::commands::volume_write::install_into_folder),
/// which this only wraps with the one thing specific to a single archive:
/// unpacking it and handing the result over as a [`HostFolder`](crate::core::volume::write::copy::HostFolder).
/// Task 5's batch install (`commands::archives`) is the other caller — several
/// archives, each renamed to its own drawer, staged into one
/// [`HostSelection`](crate::core::volume::write::copy::HostSelection) and
/// handed to the same primitive, so a cancelled batch is exactly as atomic as
/// a cancelled single install.
///
/// `pub(crate)` rather than private so `commands::archives` (and any future
/// caller in this crate) can reach it without a second copy of this wrapper.
pub(crate) fn install_archive_into_volume(
    archive: &std::path::Path,
    image: &std::path::Path,
    volume_index: usize,
    parent: u32,
    policy: OverwritePolicy,
    progress: &dyn crate::core::jobs::ProgressSink,
) -> CoreResult<InstallOutcome> {
    let (scratch, skipped) = crate::core::sources::install::unpack_for_install(archive, progress)?;

    // Sidecars on: an archive may carry `.uaem` files, and a WHDLoad slave's
    // S and P bits are the difference between a game that starts and one that
    // does not (§7.2).
    let folder = crate::core::volume::write::copy::HostFolder::new(scratch.path(), true);

    let (report, committed) = crate::commands::volume_write::install_into_folder(
        image,
        volume_index,
        parent,
        &folder,
        policy,
        progress,
    )?;

    let mut left_behind = skipped;
    left_behind.extend(report.skipped.iter().cloned());

    Ok(InstallOutcome {
        files: report.files_copied,
        directories: report.directories_created,
        bytes: report.bytes_copied,
        into: String::new(),
        backup: committed.backup,
        skipped: left_behind,
    })
}

/// Install a downloaded package into a floppy image.
///
/// The archive has to be a file already on disk — normally the one a download
/// put in the library. Nothing is fetched here: installing something the user
/// has not seen arrive would hide which of the two steps failed.
///
/// HDF is not a target. `install_hdf` stays registered as "Coming Later" in the
/// workflow catalogue (§96) until ART can write into a partition's filesystem.
#[tauri::command]
pub fn sources_install_adf(
    archive: String,
    adf: String,
    into: Option<String>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<JobId> {
    let archive_path = PathBuf::from(&archive);
    let adf_path = PathBuf::from(&adf);
    if !archive_path.is_file() {
        return Err(CoreError::InvalidInput(format!("'{archive}' is not a file")).into());
    }
    if !adf_path.is_file() {
        return Err(CoreError::InvalidInput(format!("'{adf}' is not a disk image")).into());
    }

    // The volume writer navigates by block, not by path, so a subfolder
    // string is not something this command can honour — and silently
    // dropping it would let a caller send "Tools" and never learn why the
    // package landed in the root instead. Refuse loudly rather than that.
    let into = into.unwrap_or_default();
    let into = into.trim().trim_matches('/');
    if !into.is_empty() {
        return Err(CoreError::InvalidInput(format!(
            "installing into a subfolder ('{into}') is not supported by this command; \
             use sources_install_volume with a directory block instead"
        ))
        .into());
    }
    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!(
        "Installing {}",
        archive_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| archive.clone())
    );

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        // An ADF is a bare volume at index 0 — the same install, a different
        // destination (§41.5.3).
        let result = install_archive_into_volume(
            &archive_path,
            &adf_path,
            0,
            0,
            OverwritePolicy::default(),
            progress,
        );

        let record = user_operation("Install package to disk")
            .source(&archive)
            .destination(&adf);
        let record = match &result {
            Ok(outcome) => record
                .detail("Files", outcome.files.to_string())
                .detail("Bytes", outcome.bytes.to_string())
                .detail(
                    "Folder on disk",
                    if outcome.into.is_empty() {
                        "(root)".to_string()
                    } else {
                        outcome.into.clone()
                    },
                )
                .backup(outcome.backup.clone())
                .outcome(OperationOutcome::verified(true)),
            Err(err) => record.failed(err),
        };
        super::oplog::write_to_path(&log_path, &record);

        let outcome = result?;
        let _ = emit_app.emit(
            SOURCES_EVENT,
            SourcesResult::Install {
                job_id,
                image: adf.clone(),
                outcome,
            },
        );
        Ok(())
    });

    Ok(id)
}

/// Fetch and show a package's readme.
///
/// A readme is small, but it still comes off the network, so it runs as a job
/// like everything else that does.
#[tauri::command]
pub fn sources_readme(
    path: String,
    provider: Option<String>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    state: State<'_, SourcesState>,
) -> AppResult<JobId> {
    let config = state.provider();
    let provider_id = provider.unwrap_or_else(|| config.id.clone());
    let reference = crate::core::sources::PackageRef::new(provider_id, &path);

    let meta = state
        .catalog
        .get(&reference)?
        .ok_or_else(|| CoreError::InvalidInput(format!("'{path}' is not in the catalog")))?;

    let readme_path = meta.readme_path();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!("Reading {}", meta.name);

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let state = emit_app.state::<SourcesState>();

        let mut buffer: Vec<u8> = Vec::new();
        {
            let mut limited =
                crate::core::sources::fetch::LimitedWriter::new(&mut buffer, MAX_README_BYTES);
            crate::core::sources::mirror::fetch_with_failover(
                &config.mirrors,
                &state.client,
                &readme_path,
                0,
                &mut limited,
                progress,
            )?;
        }

        let fields = parse_readme_bytes(&buffer);
        // Readmes predate UTF-8; a mangled character costs that character, not
        // the whole document.
        let text = String::from_utf8_lossy(&buffer).to_string();

        let _ = emit_app.emit(
            SOURCES_EVENT,
            SourcesResult::Readme {
                job_id,
                path: readme_path,
                text,
                fields,
            },
        );
        Ok(())
    });

    Ok(id)
}

// ---------------------------------------------------------------------------
// Operation log helpers
// ---------------------------------------------------------------------------

fn provider_label(provider: &ProviderConfig) -> String {
    match provider.mirrors.first() {
        Some(mirror) => format!("{} ({})", provider.id, mirror.base_url()),
        None => provider.id.clone(),
    }
}

fn record_sync(
    log_path: &std::path::Path,
    record: OperationRecord,
    result: &CoreResult<SyncOutcome>,
) {
    let record = match result {
        Ok(outcome) => record
            .detail("Mirror", outcome.mirror.clone())
            .detail("Index bytes", outcome.index_bytes.to_string())
            .detail("Packages", outcome.report.parsed.to_string())
            .detail("Unreadable lines", outcome.report.skipped.to_string())
            // A sync that was not applied is a success that changed nothing —
            // the previous catalog survived on purpose, and the log has to be
            // able to say that later.
            .detail(
                "Catalog replaced",
                if outcome.applied { "yes" } else { "no" },
            )
            .outcome(OperationOutcome::verified(outcome.applied)),
        Err(err) => record.failed(err),
    };
    super::oplog::write_to_path(log_path, &record);
}

fn record_fetch(
    log_path: &std::path::Path,
    record: OperationRecord,
    result: &CoreResult<FetchOutcome>,
    placed: Option<&CoreResult<Placement>>,
) {
    let Ok(outcome) = result else {
        // `result` is an `Err` here, so this unwrap-by-match cannot panic.
        if let Err(err) = result {
            super::oplog::write_to_path(log_path, &record.failed(err));
        }
        return;
    };

    let record = record
        // The destination a user cares about is the file with a name, not the
        // cache object named after its hash.
        .destination(match placed {
            Some(Ok(placement)) => placement.path.display().to_string(),
            _ => outcome.path.display().to_string(),
        })
        .detail("SHA-256", outcome.sha256.clone())
        .detail("Bytes", outcome.bytes.to_string())
        .detail("Mirror", outcome.mirror.clone())
        .detail(
            "Already cached",
            if outcome.from_cache { "yes" } else { "no" },
        );

    // A refused placement is not a failed download — the verified file is in
    // the cache either way — but the log has to be able to say which of the two
    // happened, so it is recorded as a success that did not fully verify.
    let record = match placed {
        Some(Err(err)) => record
            .detail("Kept in cache only", err.to_string())
            .outcome(OperationOutcome::verified(false)),
        Some(Ok(placement)) if placement.skipped_existing => record
            .detail("A file was already there", "kept, nothing written")
            .outcome(OperationOutcome::verified(false)),
        Some(Ok(placement)) => record
            .detail("Folder", placement.subfolder.clone())
            .outcome(OperationOutcome::verified(true)),
        None => record.outcome(OperationOutcome::verified(true)),
    };

    super::oplog::write_to_path(log_path, &record);
}

/// Install a downloaded package into a hard disk partition.
///
/// Unpacks the archive with the same extractor the ADF path uses — traversal
/// defence, decompression-bomb limits and all — and then copies the unpacked
/// folder in with the Stage W writer. An unpacked archive is a folder, and
/// copying a folder into a volume is already the tested operation (§41.5.3):
/// there is no second install path here, only a second destination.
#[tauri::command]
// A Tauri command's parameters are its wire protocol: these names are the keys
// the frontend sends.
#[allow(clippy::too_many_arguments)]
pub fn sources_install_volume(
    archive: String,
    image: String,
    volume_index: usize,
    dir_block: Option<u32>,
    overwrite: Option<OverwritePolicy>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<JobId> {
    let archive_path = PathBuf::from(archive.trim());
    let image_path = PathBuf::from(image.trim());
    let policy = overwrite.unwrap_or_default();
    let parent = dir_block.unwrap_or(0);

    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!(
        "Installing {} into {}",
        archive_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        image_path.display()
    );

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let outcome = install_archive_into_volume(
            &archive_path,
            &image_path,
            volume_index,
            parent,
            policy,
            progress,
        );

        let record = user_operation("Install package into volume")
            .source(archive_path.display().to_string())
            .destination(format!("{}:{volume_index}", image_path.display()));
        let record = match &outcome {
            Ok(installed) => record
                .detail("Files", installed.files.to_string())
                .detail("Folders", installed.directories.to_string())
                .detail("Skipped", installed.skipped.len().to_string())
                // Whole-file destinations are backed up before replacement;
                // the log has to be able to say where the previous image went.
                .backup(installed.backup.clone())
                .outcome(OperationOutcome::verified(installed.files > 0)),
            Err(err) => record.failed(err),
        };
        super::oplog::write_to_path(&log_path, &record);

        let installed = outcome?;
        let _ = emit_app.emit(
            SOURCES_EVENT,
            SourcesResult::Install {
                job_id,
                image: image_path.display().to_string(),
                outcome: installed,
            },
        );
        Ok(())
    });

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-sources-{name}-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// This runs inside Tauri's `setup`, so anything that can panic here takes
    /// the whole application down before a window appears.
    #[test]
    fn starting_with_no_catalogue_is_the_normal_case() {
        let root = temp_root("fresh");
        let state = SourcesState::new(root.join("sources"), root.join("downloads"));

        assert_eq!(state.catalog.stats().unwrap().total, 0);
        assert_eq!(state.provider().id, crate::core::sources::PROVIDER_AMINET);
        assert_eq!(state.provider().mirrors.len(), 3);

        std::fs::remove_dir_all(&root).unwrap();
    }

    /// A catalogue ART cannot read must not stop it starting (§60/§94), and it
    /// must keep the same path so the next sync repairs it rather than writing
    /// somewhere else forever.
    #[test]
    fn an_unreadable_catalogue_still_starts_and_keeps_its_path() {
        let root = temp_root("unreadable");
        let sources = root.join("sources");
        std::fs::create_dir_all(&sources).unwrap();

        // Larger than the store will read.
        let path = sources.join("catalog.jsonl");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(512 * 1024 * 1024).unwrap();
        drop(file);

        let state = SourcesState::new(sources, root.join("downloads"));
        assert_eq!(state.catalog.stats().unwrap().total, 0);
        assert_eq!(state.catalog.path(), path);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_provider_description_reaches_the_ui_intact() {
        let root = temp_root("describe");
        let state = SourcesState::new(root.join("sources"), root.join("downloads"));

        let info = describe(&state.provider(), &state.cache);
        assert_eq!(info.index_path, "INDEX");
        assert_eq!(info.mirrors[0].base_url, "https://aminet.net/");
        assert!(info.cache_path.ends_with("cache"));

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_hostile_mirror_is_refused_while_the_user_is_still_looking_at_it() {
        for bad in [
            "ftp://aminet.net/",
            "file:///C:/",
            "https://aminet.net/?redirect=evil",
            "aminet.net",
        ] {
            let result = parse_mirrors(&[MirrorInfo {
                name: "Typed by hand".into(),
                base_url: bad.into(),
            }]);
            assert!(result.is_err(), "accepted {bad}");
        }
    }

    /// An empty list would turn every later sync into an unexplained
    /// "no mirror could be reached".
    #[test]
    fn an_empty_mirror_list_is_refused() {
        let err = parse_mirrors(&[]).unwrap_err();
        assert_eq!(err.code(), "ART-INPUT-INVALID");
    }

    /// Installing into an ADF and installing into a partition must produce the
    /// same disk contents, because they are the same operation.
    #[test]
    fn installing_into_an_adf_uses_the_volume_writer() {
        use crate::core::jobs::NoProgress;
        use crate::core::lha::tests::make_lha_with;
        use crate::core::volume::fixture::ffs_volume;
        use crate::core::volume::DosType;

        let dir = temp_root("install-adf");
        let archive = dir.join("Pack.lha");
        std::fs::write(
            &archive,
            make_lha_with(&[("Tools/Editor", b"editor bytes"), ("Readme", b"read me")]),
        )
        .unwrap();

        let image = dir.join("disk.adf");
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();

        let (scratch, _) =
            crate::core::sources::install::unpack_for_install(&archive, &NoProgress).unwrap();
        let folder = crate::core::volume::write::copy::HostFolder::new(scratch.path(), true);
        let (report, backup) = crate::commands::volume_write::run_copy_in_folder(
            &image,
            0,
            0,
            &folder,
            crate::core::lha::OverwritePolicy::Skip,
            &NoProgress,
        )
        .unwrap();

        assert_eq!(report.files_copied, 2);
        assert_eq!(report.files_verified, 2, "the volume writer verifies");
        assert!(
            backup.backup.is_some(),
            "a floppy is backed up before replacement"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Nested folders in the archive are recreated once, not once per file —
    /// carried over from the retired `core::adf::mutate` install path, now
    /// proved against the volume writer instead.
    #[test]
    fn installing_recreates_nested_folders_once() {
        use crate::core::jobs::NoProgress;
        use crate::core::lha::tests::make_lha_with;
        use crate::core::volume::fixture::ffs_volume;
        use crate::core::volume::DosType;

        let dir = temp_root("install-nested");
        let archive = dir.join("Pack.lha");
        std::fs::write(
            &archive,
            make_lha_with(&[("Docs/readme.txt", b"a"), ("Docs/notes.txt", b"b")]),
        )
        .unwrap();

        let image = dir.join("disk.adf");
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();

        let (scratch, _) =
            crate::core::sources::install::unpack_for_install(&archive, &NoProgress).unwrap();
        let folder = crate::core::volume::write::copy::HostFolder::new(scratch.path(), true);
        let (report, _) = crate::commands::volume_write::run_copy_in_folder(
            &image,
            0,
            0,
            &folder,
            crate::core::lha::OverwritePolicy::Skip,
            &NoProgress,
        )
        .unwrap();

        assert_eq!(report.files_copied, 2);
        assert_eq!(
            report.directories_created, 1,
            "the folder is created once, not twice"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// §92 explain before modify, and §57's "unchanged on failure" guarantee:
    /// an install that will not fit must be refused atomically, with real
    /// numbers, before a single block is written — never landed partially. A
    /// WHDLoad pack missing its `.slave` because it did not fit is not a
    /// partial success; it is a broken, non-bootable result the user was
    /// never warned about.
    ///
    /// This drives [`install_archive_into_volume`] — the real body of both
    /// install commands — rather than calling the read-only planner itself, so
    /// deleting the `plan.fits()` guard makes this test fail rather than
    /// leaving it trivially green.
    #[test]
    fn installing_a_package_that_does_not_fit_is_refused_without_touching_the_image() {
        use crate::core::jobs::NoProgress;
        use crate::core::lha::tests::make_lha_with;
        use crate::core::volume::fixture::ffs_volume;
        use crate::core::volume::DosType;

        let dir = temp_root("install-toobig");
        let archive = dir.join("Pack.lha");
        let small = b"fits easily".to_vec();
        let big = vec![b'x'; 900 * 1024];
        std::fs::write(
            &archive,
            make_lha_with(&[("Small.txt", &small), ("Big.bin", &big)]),
        )
        .unwrap();

        let image = dir.join("disk.adf");
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();
        let before = std::fs::read(&image).unwrap();

        let err = install_archive_into_volume(
            &archive,
            &image,
            0,
            0,
            crate::core::lha::OverwritePolicy::Skip,
            &NoProgress,
        )
        .expect_err("a package larger than the floppy must be refused");

        assert_eq!(err.code(), "ART-SAFETY-REFUSED");
        let message = err.to_string();
        assert!(message.contains("blocks"), "{message}");
        assert!(message.contains("free"), "{message}");

        // Never reaching the writer is what the refusal buys: the image is
        // exactly what it was before the install was attempted.
        assert_eq!(
            std::fs::read(&image).unwrap(),
            before,
            "a refused install must leave the image byte-for-byte unchanged"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// §54 and §57: cancelling an install must install *nothing*.
    ///
    /// `copy_into_volume` stops between files and reports how many landed,
    /// which is right for a general-purpose copy. An install is not that: half
    /// a WHDLoad pack is a game that will not start, and reporting it as a
    /// finished install — with an operation-log line saying so — is worse than
    /// any error. The bytes are captured before and compared after, because an
    /// `Err` on its own would not prove the half-written package never reached
    /// the file.
    #[test]
    fn a_cancelled_install_writes_nothing_and_does_not_report_success() {
        use crate::core::jobs::ProgressSink;
        use crate::core::lha::tests::make_lha_with;
        use crate::core::volume::fixture::ffs_volume;
        use crate::core::volume::DosType;
        use std::sync::atomic::{AtomicBool, Ordering};

        /// Cancels once the copy into the volume is a couple of files in —
        /// the user hitting Stop halfway through, not before it started.
        ///
        /// The two stages are told apart by their `total`: unpacking reports a
        /// running count with no total, while `copy_into_volume` reports
        /// "index of total". Keying on that rather than on a call count keeps
        /// the test aimed at the stage it is about.
        struct StopDuringCopy(AtomicBool);
        impl ProgressSink for StopDuringCopy {
            fn report(&self, done: u64, total: Option<u64>, _message: &str) {
                if total.is_some() && done >= 2 {
                    self.0.store(true, Ordering::SeqCst);
                }
            }
            fn is_cancelled(&self) -> bool {
                self.0.load(Ordering::SeqCst)
            }
        }

        let dir = temp_root("install-cancel");
        let archive = dir.join("Pack.lha");
        let files: Vec<(String, Vec<u8>)> = (0..10)
            .map(|index| (format!("File{index}.txt"), vec![b'a' + index as u8; 64]))
            .collect();
        let borrowed: Vec<(&str, &[u8])> = files
            .iter()
            .map(|(name, data)| (name.as_str(), data.as_slice()))
            .collect();
        std::fs::write(&archive, make_lha_with(&borrowed)).unwrap();

        let image = dir.join("disk.adf");
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();
        let before = std::fs::read(&image).unwrap();

        let sink = StopDuringCopy(AtomicBool::new(false));
        let err = install_archive_into_volume(
            &archive,
            &image,
            0,
            0,
            crate::core::lha::OverwritePolicy::Skip,
            &sink,
        )
        .expect_err("a cancelled install must not come back as a successful one");

        assert_eq!(
            err.code(),
            "ART-CANCELLED",
            "the job must end Cancelled, not Completed: {err}"
        );
        assert_eq!(
            std::fs::read(&image).unwrap(),
            before,
            "a cancelled install must leave the image byte-for-byte unchanged"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The refusal above must not be satisfiable by refusing everything: a
    /// package that genuinely fits still has to install completely.
    #[test]
    fn installing_a_package_that_fits_installs_completely() {
        use crate::core::jobs::NoProgress;
        use crate::core::lha::tests::make_lha_with;
        use crate::core::volume::fixture::ffs_volume;
        use crate::core::volume::DosType;

        let dir = temp_root("install-fits");
        let archive = dir.join("Pack.lha");
        std::fs::write(
            &archive,
            make_lha_with(&[("Small.txt", b"fits easily"), ("Readme", b"read me")]),
        )
        .unwrap();

        let image = dir.join("disk.adf");
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();

        let (scratch, _) =
            crate::core::sources::install::unpack_for_install(&archive, &NoProgress).unwrap();
        let folder = crate::core::volume::write::copy::HostFolder::new(scratch.path(), true);

        let plan =
            crate::commands::volume_write::plan_copy_in_folder(&image, 0, 0, &folder).unwrap();
        assert!(plan.fits(), "two tiny files must fit on a floppy");

        let (report, backup) = crate::commands::volume_write::run_copy_in_folder(
            &image,
            0,
            0,
            &folder,
            crate::core::lha::OverwritePolicy::Skip,
            &NoProgress,
        )
        .unwrap();

        assert_eq!(report.files_copied, 2, "nothing was refused for no reason");
        assert_eq!(report.files_copied, report.files_verified);
        assert!(report.skipped.is_empty());
        assert!(backup.backup.is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_valid_mirror_list_is_accepted_in_order() {
        let parsed = parse_mirrors(&[
            MirrorInfo {
                name: "First".into(),
                base_url: "https://one.invalid".into(),
            },
            MirrorInfo {
                name: "Second".into(),
                base_url: "https://two.invalid/pub/".into(),
            },
        ])
        .unwrap();

        assert_eq!(parsed[0].name, "First");
        // A base URL always gains its trailing slash so joining is pure
        // concatenation.
        assert_eq!(parsed[0].base_url(), "https://one.invalid/");
        assert_eq!(parsed[1].base_url(), "https://two.invalid/pub/");
    }

    /// The reset path has to lead somewhere usable. If `aminet_defaults()`
    /// ever ships an empty or unparseable list, "Back to defaults" would put
    /// the user in the exact state `parse_mirrors` refuses to let them type.
    #[test]
    fn the_shipped_mirrors_are_a_list_the_editor_would_accept() {
        let shipped = aminet_defaults()
            .expect("the shipped defaults must parse")
            .mirrors;
        assert!(!shipped.is_empty(), "reset would leave no mirror to try");

        let round_tripped: Vec<MirrorInfo> = shipped
            .iter()
            .map(|mirror| MirrorInfo {
                name: mirror.name.clone(),
                base_url: mirror.base_url().to_string(),
            })
            .collect();

        let parsed = parse_mirrors(&round_tripped)
            .expect("the shipped mirrors must survive the UI's own validation");
        assert_eq!(parsed.len(), shipped.len());
        // Order is the failover order, so it must not be reshuffled on the way
        // out to the editor and back.
        for (before, after) in shipped.iter().zip(parsed.iter()) {
            assert_eq!(before.base_url(), after.base_url());
        }
    }
}
