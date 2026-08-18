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

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use super::gameindex::catalogue_dir;
use super::jobs::{spawn_job, JobRegistry};
use crate::core::artwork::cache::Cache;
use crate::core::artwork::config::{self, ConfiguredSource};
use crate::core::artwork::enrich::{enrich, EnrichOutcome, EnrichRequest};
use crate::core::artwork::key::normalise;
use crate::core::artwork::local::{adopt_local, LocalOutcome, LocalPreview};
use crate::core::artwork::{ArtKind, ArtRef};
use crate::core::gameindex::store::{read_overrides, set_override, ArtBinding};
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
///
/// `pinned` names the titles whose picture the user chose by hand — the
/// screen builds it from the rows whose cached picture's `source` is
/// `"manual"`, because the override layer that actually records the choice is
/// keyed by record id, not by title, and carries no title to send back.
#[tauri::command]
pub fn artwork_enrich(
    titles: Vec<String>,
    sources: Vec<ConfiguredSource>,
    pinned: Vec<String>,
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
                    pinned: &pinned,
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

/// The cache extension for a picture the screen can actually draw.
fn picture_extension(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => Some("png"),
        Some("jpg" | "jpeg") => Some("jpg"),
        _ => None,
    }
}

/// Attach a picture the user picked to a title.
///
/// The bytes are copied into the cache under source `manual`; the choice is
/// written into the catalogue's user layer, which is what makes it survive a
/// refresh and stops a later fetch from replacing it. `ArtKind::Boxart` on
/// purpose: `ART_KINDS` prefers it first, so a hand-attached picture outranks
/// the `.rp9` snap from Task 1 — which is what the user meant by attaching
/// one.
#[tauri::command]
pub fn artwork_attach(
    title: String,
    id: String,
    file: String,
    app: AppHandle,
) -> AppResult<ArtRef> {
    let source = PathBuf::from(&file);
    let ext = picture_extension(&source)
        .ok_or("ART can show PNG and JPEG pictures. This file is neither.")?;
    let bytes = std::fs::read(&source)?;

    let dir = artwork_dir_for(&app);
    let mut cache = Cache::open(&dir)?;
    // The normalised key, not the raw title: every other write to this cache
    // (`enrich`) keys itself the same way, and `artwork_known`'s lookup
    // normalises the title it is asked about. A raw key here would make the
    // picture invisible the moment casing or a leading article differs.
    let key = normalise(&title);
    let art = cache.store(&key, ArtKind::Boxart, "manual", ext, &bytes)?;
    cache.save()?;

    let catalogue = catalogue_dir(&app);
    let mut edit = read_overrides(&catalogue)?
        .edits
        .get(&id)
        .cloned()
        .unwrap_or_default();
    edit.art = Some(ArtBinding {
        chosen: file,
        cached: art.file.clone(),
    });
    set_override(&catalogue, &id, edit)?;

    Ok(art)
}

/// Undo [`artwork_attach`].
///
/// Removes the cached picture and clears the override's `art` field;
/// `set_override`'s existing empty-override rule deletes the record entirely
/// once nothing else is left in it.
#[tauri::command]
pub fn artwork_detach(title: String, id: String, app: AppHandle) -> AppResult<()> {
    let dir = artwork_dir_for(&app);
    let mut cache = Cache::open(&dir)?;
    cache.remove(&normalise(&title), ArtKind::Boxart)?;
    cache.save()?;

    let catalogue = catalogue_dir(&app);
    if let Some(mut edit) = read_overrides(&catalogue)?.edits.get(&id).cloned() {
        edit.art = None;
        set_override(&catalogue, &id, edit)?;
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// PNG and JPEG are what the webview draws. An IFF would be accepted here
    /// and then not render, which is worse than an honest refusal.
    #[test]
    fn only_the_formats_the_screen_can_draw_are_accepted() {
        assert_eq!(picture_extension(Path::new("cover.png")), Some("png"));
        assert_eq!(picture_extension(Path::new("cover.PNG")), Some("png"));
        assert_eq!(picture_extension(Path::new("cover.jpg")), Some("jpg"));
        assert_eq!(picture_extension(Path::new("cover.jpeg")), Some("jpg"));
        assert_eq!(picture_extension(Path::new("cover.iff")), None);
        assert_eq!(picture_extension(Path::new("cover")), None);
    }
}
