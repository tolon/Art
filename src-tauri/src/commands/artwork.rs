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
use crate::core::artwork::local::{adopt_local, LocalOutcome, LocalPreview, MAX_PREVIEW_BYTES};
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

/// What attaching a picture did, including where the previous version of the
/// catalogue's user layer went (ART-144 #2).
///
/// **The backup path is not decoration.** `set_override` writes through
/// `core/safety`'s `guarded_write`, which returns where it preserved the
/// previous `overrides.json` — and CLAUDE.md's rule is that commands surface
/// that to the UI so the user is always told. Both of these commands used to
/// drop it on the floor, which made them the only override writers in the
/// collection that did: `catalogue_set_override` has always returned it.
/// Attaching a picture rewrites a file holding every correction the user has
/// ever made to their catalogue, so "where did the previous one go" is a fair
/// question to be able to answer.
///
/// A `String`, not a `PathBuf`: `serde`'s `PathBuf` implementation errors on
/// a non-UTF-8 path, so an outcome built to reassure the user could itself
/// fail to cross the command boundary on Windows — the same reasoning
/// `RefusalReason::MediaAmbiguous::paths` already carries.
#[derive(Debug, Clone, Serialize)]
pub struct AttachOutcome {
    pub art: ArtRef,
    pub backup: Option<String>,
}

/// What detaching did. Same reasoning as [`AttachOutcome`]; there is no
/// `ArtRef` because there is no longer a picture to point at.
#[derive(Debug, Clone, Serialize)]
pub struct DetachOutcome {
    pub backup: Option<String>,
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

/// Every picture the cache holds for one title, in `ArtKind::ALL` order.
///
/// That is the preference order, so the first element is the one the grid is
/// already showing and the detail panel's default switch position needs no
/// second rule. Normalised the same way `artwork_known` normalises: the cache
/// keys itself on the normalised title, and every reader folds it first.
#[tauri::command]
pub fn artwork_for_title(title: String, app: AppHandle) -> AppResult<Vec<ArtRef>> {
    let cache = Cache::open(&artwork_dir_for(&app))?;
    let key = normalise(&title);
    Ok(ArtKind::ALL
        .into_iter()
        .filter_map(|kind| cache.get(&key, kind).cloned())
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

/// The logic behind [`artwork_attach`], `AppHandle`-free so it can be tested
/// directly against a tempdir rather than a real Tauri app.
///
/// **Two writes, one rollback.** The cache write (bytes plus index row) and
/// the override write (the choice that makes it stick) are two different
/// files on two different rules — `core/safety`'s guarded write for the
/// override, a plain atomic write for the derived cache — so one can succeed
/// while the other fails. If the override write fails, the cache entry this
/// call just created is removed before the error is returned: otherwise a
/// picture would sit live in the cache under source `manual` with nothing in
/// the override layer backing it, and the user would see an error for an
/// attach that half happened. The rollback's own failure is swallowed — there
/// is nothing further to do about it — but it must never replace the real
/// error with one about cleanup.
fn attach_picture(
    dir: &Path,
    catalogue: &Path,
    title: &str,
    id: &str,
    file: String,
) -> AppResult<AttachOutcome> {
    let source = PathBuf::from(&file);
    let ext = picture_extension(&source)
        .ok_or("ART can show PNG and JPEG pictures. This file is neither.")?;
    // Every other picture path in this wave carries a ceiling
    // (`MAX_PREVIEW_BYTES`) — a user's chosen file is untrusted the same way
    // an archive entry is, and reading it whole with no bound is the one gap
    // this path had left.
    let size = std::fs::metadata(&source)?.len();
    if size > MAX_PREVIEW_BYTES {
        return Err(format!(
            "'{}' is {size} bytes, larger than ART accepts for a picture ({MAX_PREVIEW_BYTES} bytes)",
            source.display()
        )
        .into());
    }
    let bytes = std::fs::read(&source)?;

    let mut cache = Cache::open(dir)?;
    // The normalised key, not the raw title: every other write to this cache
    // (`enrich`) keys itself the same way, and `artwork_known`'s lookup
    // normalises the title it is asked about. A raw key here would make the
    // picture invisible the moment casing or a leading article differs.
    let key = normalise(title);
    let art = cache.store(&key, ArtKind::Boxart, "manual", ext, &bytes)?;
    cache.save()?;

    let write_override: AppResult<Option<PathBuf>> = (|| {
        let mut edit = read_overrides(catalogue)?
            .edits
            .get(id)
            .cloned()
            .unwrap_or_default();
        edit.art = Some(ArtBinding {
            chosen: file,
            cached: art.file.clone(),
        });
        Ok(set_override(catalogue, id, edit)?)
    })();

    let backup = match write_override {
        Ok(backup) => backup,
        Err(err) => {
            let _ = cache.remove(&key, ArtKind::Boxart);
            let _ = cache.save();
            return Err(err);
        }
    };

    Ok(AttachOutcome {
        art,
        backup: backup.map(|p| p.display().to_string()),
    })
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
) -> AppResult<AttachOutcome> {
    attach_picture(
        &artwork_dir_for(&app),
        &catalogue_dir(&app),
        &title,
        &id,
        file,
    )
}

/// Undo [`artwork_attach`].
///
/// Removes the cached picture and clears the override's `art` field;
/// `set_override`'s existing empty-override rule deletes the record entirely
/// once nothing else is left in it.
#[tauri::command]
pub fn artwork_detach(title: String, id: String, app: AppHandle) -> AppResult<DetachOutcome> {
    let dir = artwork_dir_for(&app);
    let mut cache = Cache::open(&dir)?;
    cache.remove(&normalise(&title), ArtKind::Boxart)?;
    cache.save()?;

    let catalogue = catalogue_dir(&app);
    let mut backup = None;
    if let Some(mut edit) = read_overrides(&catalogue)?.edits.get(&id).cloned() {
        edit.art = None;
        backup = set_override(&catalogue, &id, edit)?;
    }

    Ok(DetachOutcome {
        backup: backup.map(|p| p.display().to_string()),
    })
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

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-artwork-cmd-{}-{name}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The defect this exists for: the cache write (bytes plus index row)
    /// succeeds and the override write that is supposed to record the choice
    /// fails. Without the rollback, the picture would sit in the cache under
    /// source `manual` — indistinguishable from a successfully attached one —
    /// while the catalogue has no idea it exists.
    #[test]
    fn a_failed_override_write_rolls_back_the_cache_entry() {
        let root = tempdir("rollback");
        let cache_dir = root.join("artwork");
        let catalogue_dir = root.join("catalogue");
        std::fs::create_dir_all(&catalogue_dir).unwrap();
        // Malformed JSON makes `read_overrides` fail deterministically — the
        // same failure shape a locked or corrupt overrides file would produce
        // for real, without reaching for filesystem permissions.
        std::fs::write(catalogue_dir.join("overrides.json"), b"{ not json").unwrap();

        let picture = root.join("cover.png");
        std::fs::write(&picture, b"PNGDATA").unwrap();

        let result = attach_picture(
            &cache_dir,
            &catalogue_dir,
            "Turrican II",
            "some-id",
            picture.to_string_lossy().to_string(),
        );

        assert!(
            result.is_err(),
            "the override write must have failed for this test to prove anything"
        );

        // The cache write that happened before the failure must have been
        // undone, not left behind as an orphaned "manual" entry.
        let cache = Cache::open(&cache_dir).unwrap();
        assert!(
            cache.get("turrican ii", ArtKind::Boxart).is_none(),
            "a failed attach left a picture live in the cache"
        );
    }

    /// The ordinary path still works once the rollback exists: a successful
    /// attach's picture is findable afterwards, normalised key and all.
    #[test]
    fn a_successful_attach_leaves_the_picture_in_the_cache() {
        let root = tempdir("success");
        let cache_dir = root.join("artwork");
        let catalogue_dir = root.join("catalogue");
        std::fs::create_dir_all(&catalogue_dir).unwrap();

        let picture = root.join("cover.png");
        std::fs::write(&picture, b"PNGDATA").unwrap();

        let first = attach_picture(
            &cache_dir,
            &catalogue_dir,
            "Turrican II",
            "some-id",
            picture.to_string_lossy().to_string(),
        )
        .unwrap();

        assert_eq!(first.art.source, "manual");
        let cache = Cache::open(&cache_dir).unwrap();
        assert!(cache.get("turrican ii", ArtKind::Boxart).is_some());

        // ART-144 (#2). The first attach writes an `overrides.json` that did
        // not exist, so `guarded_write` had nothing to preserve and says so.
        assert_eq!(first.backup, None, "there was no previous version to keep");

        // The second one overwrites a real file, and that is when the user
        // needs to be told where the previous one went. Discarding this — as
        // both commands used to — left the collection's two artwork writers
        // as the only override writers in ART that did not.
        let second = attach_picture(
            &cache_dir,
            &catalogue_dir,
            "Turrican II",
            "some-id",
            picture.to_string_lossy().to_string(),
        )
        .unwrap();
        let backup = second
            .backup
            .expect("overwriting an existing overrides.json must report its backup");
        assert!(
            std::path::Path::new(&backup).is_file(),
            "the reported backup must actually be there: {backup}"
        );
    }

    /// A picture the user points at is untrusted the same way an archive
    /// entry is — every other picture path in this wave carries a ceiling
    /// (`MAX_PREVIEW_BYTES`); this is the one that did not, until now.
    #[test]
    fn a_picture_larger_than_the_ceiling_is_refused() {
        let root = tempdir("oversized");
        let cache_dir = root.join("artwork");
        let catalogue_dir = root.join("catalogue");
        std::fs::create_dir_all(&catalogue_dir).unwrap();

        let picture = root.join("huge.png");
        // One byte past the ceiling is enough to prove the refusal fires on
        // the boundary rather than only on something absurdly large.
        std::fs::write(&picture, vec![0u8; (MAX_PREVIEW_BYTES + 1) as usize]).unwrap();

        let result = attach_picture(
            &cache_dir,
            &catalogue_dir,
            "Turrican II",
            "some-id",
            picture.to_string_lossy().to_string(),
        );

        assert!(result.is_err(), "an oversized picture must be refused");
        let cache = Cache::open(&cache_dir).unwrap();
        assert!(
            cache.get("turrican ii", ArtKind::Boxart).is_none(),
            "a refused attach must not leave a cache entry behind"
        );
    }
}
