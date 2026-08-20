//! Putting a hand-attached picture back after the artwork cache has gone
//! (ART-143).
//!
//! **Why this can exist at all.** A hand-attached picture is deliberately two
//! halves with two owners (`core::gameindex::store::ArtBinding`): the bytes go
//! into the artwork cache, which is *derived* data and can be rebuilt, and the
//! **choice** goes into the catalogue's user layer, which no refresh touches.
//! `ArtBinding::chosen` is the path the user actually picked, and its own doc
//! comment has always said it is kept "so the binding can be re-materialised
//! if the cache is ever cleared". Nothing read it back. Deleting the artwork
//! cache — a documented, sanctioned thing to do, since it is a sibling of the
//! catalogue directory precisely so a user can reclaim 1.6 GB without losing
//! the index — left every hand-attached picture rendering nothing, with the
//! exact file it came from still named in the override right beside it.
//!
//! **The key is normalised here, and that is the whole of the second rule.**
//! The cache does **not** normalise internally: `Cache::store` and `Cache::get`
//! take whatever key they are handed. Every reader folds the title through
//! [`normalise`] first (`commands::artwork::artwork_known`,
//! `artwork_for_title`, `attach_picture`), and a pass that skipped the fold
//! once wrote 242 pictures under keys nothing ever read (the defect
//! `local::adopt_local`'s own comment records). So the title arrives here as
//! the screen holds it and is folded on the way in, exactly once.
//!
//! **Nothing is guessed and nothing is fetched.** A binding whose `chosen`
//! file has gone, or is no longer a picture ART can draw, or has grown past
//! the ceiling, is reported as unrestorable and left alone — the override is
//! not deleted, because the file may be on a drive that is merely not plugged
//! in today, and quietly discarding the user's choice is the one outcome
//! "nothing changes unless the user changes it" forbids.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::artwork::cache::Cache;
use crate::core::artwork::key::normalise;
use crate::core::artwork::local::MAX_PREVIEW_BYTES;
use crate::core::artwork::{ArtKind, ArtRef};
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;

/// The cache source a hand-attached picture is stored under.
///
/// The same literal `commands::artwork::attach_picture` writes, and the one
/// the Collection screen tests against to decide whether a row's picture is
/// the user's own. A constant so the two cannot drift.
pub const SOURCE_ID: &str = "manual";

/// The kind a hand-attached picture is stored as.
///
/// `Boxart` on purpose: `ArtKind::ALL` prefers it first, so a picture the user
/// attached outranks the `.rp9` snap ART found by itself — which is what the
/// user meant by attaching one. Restoring it under any other kind would put it
/// back where it no longer wins.
pub const KIND: ArtKind = ArtKind::Boxart;

/// One title's hand-attached picture, as the caller knows it.
///
/// `title` is the title **the screen shows** — the applied one, after any
/// override — because that is what `attach_picture` was given and therefore
/// what the cache key was folded from. `id` never keys the cache; it is
/// carried so the caller can write an updated binding back against the right
/// record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub id: String,
    pub title: String,
    /// `ArtBinding::chosen` — the file the user originally picked.
    pub chosen: PathBuf,
    /// `ArtBinding::cached` — the cache-relative name recorded at the time.
    pub cached: String,
}

/// Why one binding could not be put back.
///
/// A value, never a sentence (ART-060); the screen translates it. Closed on
/// purpose, so a fifth reason is a compile error at every place that has to
/// say something about it rather than a row with no explanation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RebindProblem {
    /// The file the user picked is not there any more — moved, renamed, or on
    /// a drive that is not plugged in.
    SourceGone,
    /// It is there and is not a picture ART can draw.
    NotAPicture,
    /// It is there and has grown past [`MAX_PREVIEW_BYTES`] since it was
    /// attached.
    TooLarge,
    /// It is there and could not be read or written back into the cache.
    Unreadable,
}

/// One binding that could not be put back, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebindMiss {
    pub id: String,
    pub title: String,
    /// The file that was looked for, so the user's next question — "which
    /// one?" — is answered without them having to open the overrides file.
    pub chosen: String,
    pub problem: RebindProblem,
}

/// One binding that was put back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rebound {
    pub id: String,
    pub title: String,
    /// The cache-relative name it now lives under. Compared against the
    /// binding's recorded `cached` by the caller: when it differs, the
    /// override needs rewriting so the two halves agree again.
    pub cached: String,
}

/// What a pass managed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebindOutcome {
    /// Bindings whose cache entry was already there, with its file. The
    /// ordinary case, and the reason a pass costs nothing when there is
    /// nothing to do.
    pub intact: u32,
    pub restored: Vec<Rebound>,
    pub missed: Vec<RebindMiss>,
}

impl RebindOutcome {
    /// Whether anything actually changed — the caller's cue to re-read the
    /// cache, and to say anything at all on screen.
    pub fn changed_anything(&self) -> bool {
        !self.restored.is_empty()
    }
}

/// Whether ART can draw this file, and under what extension the cache should
/// store it.
///
/// Deliberately identical to `commands::artwork::picture_extension`: the
/// re-materialised copy must land under the same cache-relative name the
/// original attach produced, and that name is derived from this extension.
/// `jpeg` folds to `jpg` for exactly that reason.
fn picture_extension(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())?
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => Some("png"),
        "jpg" | "jpeg" => Some("jpg"),
        _ => None,
    }
}

/// Put back every hand-attached picture whose cache entry has gone.
///
/// Never fails for one bad binding: a catalogue outlives the files it
/// describes, and one picture the user has since deleted must not cost the
/// others theirs. The only error returned is [`CoreError::Cancelled`], and
/// the cache index is saved before it is — a cancelled pass leaves every
/// picture it did restore genuinely restored, rather than on disk with no
/// index row pointing at it.
///
/// Cancellation is checked **between whole bindings**, never mid-write:
/// `Cache::store` goes through `atomic_write`, and stopping inside one would
/// be the one thing `core/safety` exists to prevent.
pub fn rebind_manual_art(
    cache_dir: &Path,
    bindings: &[Binding],
    sink: &dyn ProgressSink,
) -> CoreResult<RebindOutcome> {
    let mut cache = Cache::open(cache_dir)?;
    let mut outcome = RebindOutcome::default();
    let total = bindings.len() as u64;

    for (done, binding) in bindings.iter().enumerate() {
        // Between whole units of work, never mid-write.
        if sink.is_cancelled() {
            let _ = cache.save();
            return Err(CoreError::Cancelled);
        }
        sink.report(done as u64, Some(total), &binding.title);

        let key = normalise(&binding.title);

        // **Both halves, not just the index.** A user who deleted the whole
        // artwork directory takes the index with it, so a missing row is the
        // commonest signal — but deleting the *pictures* and leaving
        // `index.json` behind is just as easy with a file manager, and that
        // leaves a row pointing at nothing. Asking the filesystem is what
        // makes the second case recoverable too.
        if let Some(art) = cache.get(&key, KIND) {
            if cache.dir().join(&art.file).is_file() {
                outcome.intact += 1;
                continue;
            }
        }

        match restore(&mut cache, &key, binding) {
            Ok(art) => outcome.restored.push(Rebound {
                id: binding.id.clone(),
                title: binding.title.clone(),
                cached: art.file,
            }),
            Err(problem) => outcome.missed.push(RebindMiss {
                id: binding.id.clone(),
                title: binding.title.clone(),
                chosen: binding.chosen.display().to_string(),
                problem,
            }),
        }
    }

    sink.report(total, Some(total), "");
    cache.save()?;
    Ok(outcome)
}

/// Read one chosen file and put it back into the cache, or say why not.
///
/// The size is taken from the metadata and checked **before** the read, not
/// after: the file is the user's own but it is still untrusted input, and
/// reading a multi-gigabyte file to then reject it is the gap the attach path
/// itself had until ART-144's round.
fn restore(cache: &mut Cache, key: &str, binding: &Binding) -> Result<ArtRef, RebindProblem> {
    let ext = picture_extension(&binding.chosen).ok_or(RebindProblem::NotAPicture)?;
    let meta = std::fs::metadata(&binding.chosen).map_err(|_| RebindProblem::SourceGone)?;
    if !meta.is_file() {
        return Err(RebindProblem::SourceGone);
    }
    if meta.len() > MAX_PREVIEW_BYTES {
        return Err(RebindProblem::TooLarge);
    }
    let bytes = std::fs::read(&binding.chosen).map_err(|_| RebindProblem::Unreadable)?;
    cache
        .store(key, KIND, SOURCE_ID, ext, &bytes)
        .map_err(|_| RebindProblem::Unreadable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jobs::NoProgress;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-rebind-{tag}-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn binding(dir: &Path, id: &str, title: &str, file: &str, bytes: &[u8]) -> Binding {
        let chosen = dir.join(file);
        std::fs::write(&chosen, bytes).unwrap();
        Binding {
            id: id.to_string(),
            title: title.to_string(),
            chosen,
            cached: String::new(),
        }
    }

    /// The whole point of the issue: the cache is deleted and the pictures
    /// come back from the files the overrides still name.
    #[test]
    fn a_deleted_cache_is_rebuilt_from_the_files_the_user_chose() {
        let root = scratch("deleted-cache");
        let cache_dir = root.join("artwork");
        let want = binding(&root, "id-1", "Turrican II", "cover.png", b"PNGDATA");

        // Attach it the way the command does, then delete the cache the way
        // a user reclaiming disk space does.
        {
            let mut cache = Cache::open(&cache_dir).unwrap();
            cache
                .store(&normalise(&want.title), KIND, SOURCE_ID, "png", b"PNGDATA")
                .unwrap();
            cache.save().unwrap();
        }
        std::fs::remove_dir_all(&cache_dir).unwrap();

        let outcome =
            rebind_manual_art(&cache_dir, std::slice::from_ref(&want), &NoProgress).unwrap();

        assert_eq!(outcome.missed, vec![]);
        assert_eq!(outcome.intact, 0);
        assert_eq!(outcome.restored.len(), 1);
        assert!(outcome.changed_anything());

        // And it is readable through the path every screen already uses —
        // `cache.best(normalise(title))`, which is what `artwork_known` calls.
        let cache = Cache::open(&cache_dir).unwrap();
        let art = cache
            .best(&normalise("Turrican II"))
            .expect("the picture is back where the screen looks for it");
        assert_eq!(art.source, SOURCE_ID);
        assert_eq!(
            std::fs::read(cache.dir().join(&art.file)).unwrap(),
            b"PNGDATA"
        );
    }

    /// **The key is folded, and the cache does not fold it for us.** This is
    /// the mistake that once wrote 242 pictures to keys nothing read: store
    /// under the raw title and every reader — which folds — misses it. Asked
    /// with a title whose raw and folded forms differ in three separate ways
    /// (leading article, case, doubled spaces).
    #[test]
    fn the_picture_lands_under_the_key_the_screen_reads_by() {
        let root = scratch("normalised-key");
        let cache_dir = root.join("artwork");
        let raw = "The  Settlers ";
        let want = binding(&root, "id-1", raw, "cover.png", b"PNGDATA");

        rebind_manual_art(&cache_dir, std::slice::from_ref(&want), &NoProgress).unwrap();

        let cache = Cache::open(&cache_dir).unwrap();
        assert_eq!(
            normalise(raw),
            "settlers",
            "sanity: the fold really differs"
        );
        assert!(
            cache.get("settlers", KIND).is_some(),
            "stored under the folded key, which is what every reader asks for"
        );
        assert!(
            cache.get(raw, KIND).is_none(),
            "and not under the raw title, which nothing ever asks for"
        );
    }

    /// A pass over an intact cache costs one metadata call per binding and
    /// changes nothing — which is what makes running it on every catalogue
    /// load acceptable.
    #[test]
    fn an_intact_binding_is_left_exactly_as_it_was() {
        let root = scratch("intact");
        let cache_dir = root.join("artwork");
        let want = binding(&root, "id-1", "Turrican II", "cover.png", b"ORIGINAL");

        let first =
            rebind_manual_art(&cache_dir, std::slice::from_ref(&want), &NoProgress).unwrap();
        assert_eq!(first.restored.len(), 1);

        // Change the source file. A second pass must not pick it up: the
        // cache entry is intact, and silently replacing a picture the user is
        // looking at because a file elsewhere changed is not this function's
        // job.
        std::fs::write(&want.chosen, b"CHANGED").unwrap();

        let second =
            rebind_manual_art(&cache_dir, std::slice::from_ref(&want), &NoProgress).unwrap();
        assert_eq!(second.intact, 1);
        assert_eq!(second.restored, vec![]);
        assert!(!second.changed_anything());

        let cache = Cache::open(&cache_dir).unwrap();
        let art = cache.best(&normalise(&want.title)).unwrap();
        assert_eq!(
            std::fs::read(cache.dir().join(&art.file)).unwrap(),
            b"ORIGINAL"
        );
    }

    /// An index row pointing at a file that is no longer there is just as
    /// broken as no row at all — and far easier to produce, since deleting
    /// the pictures and leaving `index.json` is one selection in a file
    /// manager.
    #[test]
    fn a_row_whose_file_has_gone_is_rebuilt_too() {
        let root = scratch("row-without-file");
        let cache_dir = root.join("artwork");
        let want = binding(&root, "id-1", "Turrican II", "cover.png", b"PNGDATA");

        rebind_manual_art(&cache_dir, std::slice::from_ref(&want), &NoProgress).unwrap();

        let picture = {
            let cache = Cache::open(&cache_dir).unwrap();
            cache
                .dir()
                .join(&cache.best(&normalise(&want.title)).unwrap().file)
        };
        std::fs::remove_file(&picture).unwrap();

        let outcome =
            rebind_manual_art(&cache_dir, std::slice::from_ref(&want), &NoProgress).unwrap();
        assert_eq!(outcome.intact, 0, "a row with no file is not intact");
        assert_eq!(outcome.restored.len(), 1);
        assert!(picture.is_file());
    }

    /// One picture the user has since deleted must not cost the others
    /// theirs, and the override is not touched — the drive may simply not be
    /// plugged in.
    #[test]
    fn one_missing_source_does_not_stop_the_others() {
        let root = scratch("missing-source");
        let cache_dir = root.join("artwork");
        let good = binding(&root, "id-1", "Turrican II", "cover.png", b"PNGDATA");
        let gone = Binding {
            id: "id-2".to_string(),
            title: "Lotus".to_string(),
            chosen: root.join("nowhere").join("lotus.png"),
            cached: String::new(),
        };

        let outcome = rebind_manual_art(&cache_dir, &[gone, good], &NoProgress).unwrap();

        assert_eq!(outcome.restored.len(), 1);
        assert_eq!(outcome.restored[0].id, "id-1");
        assert_eq!(outcome.missed.len(), 1);
        assert_eq!(outcome.missed[0].id, "id-2");
        assert_eq!(outcome.missed[0].problem, RebindProblem::SourceGone);
        // Named, so the user can go and find it.
        assert!(outcome.missed[0].chosen.ends_with("lotus.png"));
    }

    /// A file that has grown past the ceiling since it was attached is
    /// refused on its **metadata**, not after being read into memory.
    #[test]
    fn an_oversized_source_is_refused_by_its_size() {
        let root = scratch("oversized");
        let cache_dir = root.join("artwork");
        let want = binding(
            &root,
            "id-1",
            "Turrican II",
            "cover.png",
            &vec![0u8; (MAX_PREVIEW_BYTES + 1) as usize],
        );

        let outcome =
            rebind_manual_art(&cache_dir, std::slice::from_ref(&want), &NoProgress).unwrap();

        assert_eq!(outcome.restored, vec![]);
        assert_eq!(outcome.missed.len(), 1);
        assert_eq!(outcome.missed[0].problem, RebindProblem::TooLarge);
        assert!(
            Cache::open(&cache_dir)
                .unwrap()
                .best("turrican ii")
                .is_none(),
            "a refused restore leaves no cache entry behind"
        );
    }

    /// A binding whose file is no longer something the webview can draw —
    /// the user replaced their `cover.png` with a `cover.iff`, or pointed at
    /// something that never was a picture. Refused by name rather than stored
    /// and then failing to render.
    #[test]
    fn a_source_that_is_not_a_drawable_picture_is_refused() {
        let root = scratch("not-a-picture");
        let cache_dir = root.join("artwork");
        let want = binding(&root, "id-1", "Turrican II", "cover.iff", b"FORM....ILBM");

        let outcome =
            rebind_manual_art(&cache_dir, std::slice::from_ref(&want), &NoProgress).unwrap();

        assert_eq!(outcome.missed.len(), 1);
        assert_eq!(outcome.missed[0].problem, RebindProblem::NotAPicture);
    }

    /// Cancelling leaves what it did finish genuinely finished — index rows
    /// included — rather than pictures on disk nothing points at.
    #[test]
    fn cancelling_keeps_what_it_already_restored() {
        use crate::core::jobs::ProgressSink;
        use std::sync::atomic::{AtomicBool, Ordering};

        /// Cancels the moment the first binding is reported.
        ///
        /// The check sits *before* the report, so this lets the first binding
        /// through whole and stops the loop at the top of the second — which
        /// is the boundary the guarantee is about.
        struct AfterFirst {
            cancelled: AtomicBool,
        }
        impl ProgressSink for AfterFirst {
            fn report(&self, _done: u64, _total: Option<u64>, _label: &str) {
                self.cancelled.store(true, Ordering::SeqCst);
            }
            fn is_cancelled(&self) -> bool {
                self.cancelled.load(Ordering::SeqCst)
            }
        }

        let root = scratch("cancel");
        let cache_dir = root.join("artwork");
        let first = binding(&root, "id-1", "Turrican II", "a.png", b"FIRST");
        let second = binding(&root, "id-2", "Lotus", "b.png", b"SECOND");

        let sink = AfterFirst {
            cancelled: AtomicBool::new(false),
        };
        let err = rebind_manual_art(&cache_dir, &[first, second], &sink).unwrap_err();
        assert!(matches!(err, CoreError::Cancelled), "{err:?}");

        // The first one is on disk *and* in the index — the save before the
        // `Cancelled` return is what makes that true.
        let cache = Cache::open(&cache_dir).unwrap();
        let art = cache
            .best(&normalise("Turrican II"))
            .expect("what finished before the stop is kept");
        assert_eq!(
            std::fs::read(cache.dir().join(&art.file)).unwrap(),
            b"FIRST"
        );
        assert!(cache.best(&normalise("Lotus")).is_none());
    }

    /// The extension decides the cache-relative name, so it has to fold the
    /// same way `attach_picture`'s own gate folds — otherwise a restored
    /// `.jpeg` would land beside the original rather than on top of it.
    #[test]
    fn jpeg_and_jpg_are_one_name_here_as_they_are_on_the_attach_path() {
        assert_eq!(picture_extension(Path::new("c.png")), Some("png"));
        assert_eq!(picture_extension(Path::new("c.PNG")), Some("png"));
        assert_eq!(picture_extension(Path::new("c.jpeg")), Some("jpg"));
        assert_eq!(picture_extension(Path::new("c.JPG")), Some("jpg"));
        assert_eq!(picture_extension(Path::new("c.iff")), None);
        assert_eq!(picture_extension(Path::new("c")), None);
    }
}
