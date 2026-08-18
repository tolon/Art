//! Artwork Tauri commands (Collection · wave B).
//!
//! A thin adapter, as CLAUDE.md requires: deserialize, call core, serialize
//! back. Matching, the cache, the rate limit and every decision about which
//! source is asked live in `core/artwork`.
//!
//! **The source list is not stored here.** It is a setting, and settings live
//! in the frontend's store like every other one (CLAUDE.md, "State &
//! persistence"). Rust supplies the shipped defaults so no URL is hard-coded in
//! TypeScript, validates a base before the screen saves it, and receives the
//! list back as an argument when the job runs. A second persistence mechanism
//! for one list would be a second place for a setting to disagree with itself.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use super::jobs::{spawn_job, JobRegistry};
use crate::core::artwork::cache::Cache;
use crate::core::artwork::config::{self, ConfiguredSource};
use crate::core::artwork::enrich::{enrich, EnrichOutcome, EnrichRequest};
use crate::core::artwork::key::normalise;
use crate::core::artwork::local::{adopt_local, LocalOutcome, LocalPreview};
use crate::core::artwork::{ArtKind, ArtRef};
use crate::core::jobs::JobId;
use crate::error::AppResult;
use crate::net::http_mirror::HttpMirrorClient;

/// Where cached pictures live.
///
/// A **sibling** of the catalogue directory, not a child. A user deleting
/// 1.6 GB of artwork to reclaim disk must not lose the index that took minutes
/// to build. The path is mirrored by the `assetProtocol` scope in
/// `tauri.conf.json` — change one and the other stops matching.
fn artwork_dir_for(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("artwork")
}

/// Emitted when an enrichment job finishes, with what each source managed.
pub const ARTWORK_RESULT_EVENT: &str = "artwork-result";

#[derive(Debug, Clone, Serialize)]
pub struct ArtworkResult {
    pub job_id: JobId,
    #[serde(flatten)]
    pub outcome: EnrichOutcome,
}

/// The sources ART ships with, enabled, with their default mirrors.
///
/// The screen asks for these on first run rather than carrying its own copy of
/// the URLs: two lists of mirrors would drift.
#[tauri::command]
pub fn artwork_defaults() -> Vec<ConfiguredSource> {
    config::shipped_defaults()
}

/// Check a source the user has edited, before the screen saves it.
///
/// Refused at the door rather than at fetch time: a base that cannot become a
/// `Mirror` would otherwise sit in the settings file looking fine until an
/// enrichment run failed on it.
#[tauri::command]
pub fn artwork_check_source(source: ConfiguredSource) -> AppResult<()> {
    if config::source_for(&source.id).is_none() {
        return Err(crate::core::error::CoreError::InvalidInput(format!(
            "'{}' is not a source this ART knows",
            source.id
        ))
        .into());
    }
    config::index_mirror(&source)?;
    config::image_mirror(&source)?;
    Ok(())
}

/// Where the screen loads cached pictures from.
///
/// Returned rather than assumed, because the fallback to a temp directory when
/// `%APPDATA%` is unavailable would otherwise leave the screen pointing at a
/// path that does not exist.
#[tauri::command]
pub fn artwork_dir(app: AppHandle) -> String {
    artwork_dir_for(&app).to_string_lossy().to_string()
}

/// Which of these titles already have a picture, in the order asked.
///
/// The screen sends its titles as the catalogue holds them and gets back one
/// slot each. Normalising here rather than in TypeScript is the point: the two
/// matching rules live in `core/artwork/key.rs`, and a second implementation in
/// the frontend would drift from them the first time one changed.
#[tauri::command]
pub fn artwork_known(titles: Vec<String>, app: AppHandle) -> AppResult<Vec<Option<ArtRef>>> {
    let cache = Cache::open(&artwork_dir_for(&app))?;
    Ok(titles
        .iter()
        .map(|title| cache.best(&normalise(title)).cloned())
        .collect())
}

/// What the Collection actually renders today: one picture per row.
///
/// libretro publishes four kinds and fetching all four takes four times as long
/// for three pictures nothing shows — the difference, measured against a real
/// 1700-title library, between about a minute and about forty. Wave C's richer
/// screen widens this list when it has somewhere to put them.
const DISPLAYED_KINDS: [ArtKind; 2] = [ArtKind::Boxart, ArtKind::Icon];

/// Fetch artwork for a list of titles, on a job.
///
/// Returns a job id immediately. A collection of 1700 titles is firmly §54/§55
/// territory, and the reason the user gets a Stop rather than a spinner.
///
/// **Nothing calls this on its own.** The Collection screen opens without
/// touching the network; this runs when the user asks it to.
#[tauri::command]
pub fn artwork_enrich(
    titles: Vec<String>,
    sources: Vec<ConfiguredSource>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
) -> AppResult<JobId> {
    let dir = artwork_dir_for(&app);
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();

    let id = spawn_job(
        &app,
        registry,
        "Fetching artwork",
        move |job_id, progress| {
            let client = HttpMirrorClient::default();
            let outcome = enrich(
                EnrichRequest {
                    titles: &titles,
                    sources: &sources,
                    cache_dir: &dir,
                    wanted: &DISPLAYED_KINDS,
                    // Task 6 is where the real pinned list arrives from the
                    // catalogue's overrides; until then this command pins
                    // nothing, which is exactly what it does today.
                    pinned: &[],
                },
                &client,
                progress,
            )?;
            let _ = emit_app.emit(ARTWORK_RESULT_EVENT, ArtworkResult { job_id, outcome });
            Ok(())
        },
    );

    Ok(id)
}

/// The argument shape: a path is a string on the wire, a `PathBuf` in `core`.
#[derive(Debug, Clone, Deserialize)]
pub struct LocalPreviewArg {
    pub title: String,
    pub package: String,
    pub entry: String,
}

/// Emitted when the offline pass finishes.
pub const LOCAL_RESULT_EVENT: &str = "artwork-local-result";

#[derive(Debug, Clone, Serialize)]
pub struct LocalResult {
    pub job_id: JobId,
    pub outcome: LocalOutcome,
}

/// Take the pictures the user's own packages already carry.
///
/// A job rather than a plain command because it opens one archive per title
/// and there are 242 of them (§54).
#[tauri::command]
pub fn artwork_adopt_local(
    previews: Vec<LocalPreviewArg>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
) -> AppResult<JobId> {
    let dir = artwork_dir_for(&app);
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let previews: Vec<LocalPreview> = previews
        .into_iter()
        .map(|arg| LocalPreview {
            title: arg.title,
            package: PathBuf::from(arg.package),
            entry: arg.entry,
        })
        .collect();

    let id = spawn_job(
        &app,
        registry,
        "Reading pictures from your files",
        move |job_id, progress| {
            let outcome = adopt_local(&dir, &previews, progress)?;
            let _ = emit_app.emit(LOCAL_RESULT_EVENT, LocalResult { job_id, outcome });
            Ok(())
        },
    );

    Ok(id)
}
