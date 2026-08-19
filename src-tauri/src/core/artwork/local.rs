//! Pictures ART already has, and never asked anybody for (wave C).
//!
//! Every `.rp9` in the user's collection carries an embedded `screen-running`
//! PNG — 242 of 242 in the folder this was written against — and G10's reader
//! already records its **name inside the zip** as `GameRecord::preview`.
//! Rendering it is therefore an extraction, not a download.
//!
//! **Why this is not a source in the wave B sense.** `enrich()` takes a
//! `MirrorClient` because every source it knows fetches; this one must be
//! callable with none at all. Nothing leaves the machine, so nothing here asks
//! the user's permission, consults a configured mirror or can fail because a
//! host is down. It shares wave B's `Cache`, so every screen renders the
//! result through the path it already uses.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::archive::open;
use crate::core::artwork::cache::Cache;
use crate::core::artwork::key::normalise;
use crate::core::artwork::ArtKind;
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;

/// The source id these pictures are cached under.
pub const SOURCE_ID: &str = "rp9";

/// A preview is a screenshot, not a disk image. Four megabytes is far above
/// any real one and far below anything that could exhaust memory — the same
/// reasoning `MAX_MANIFEST_BYTES` uses one module over.
pub const MAX_PREVIEW_BYTES: u64 = 4 * 1024 * 1024;

/// One picture to look for: which title it belongs to, which package holds it,
/// and what it is called in there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalPreview {
    pub title: String,
    pub package: PathBuf,
    pub entry: String,
}

/// What a pass managed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalOutcome {
    pub written: u32,
    /// Already on disk from an earlier pass.
    pub adopted: u32,
    /// No package, no such entry, or a name that would have escaped the cache.
    pub missed: u32,
}

/// Pull each preview out of its package and into the cache.
///
/// Never fails for one bad package: a catalogue outlives the files it
/// describes, and one missing `.rp9` must not cost the other 241 their
/// pictures. The only error it returns is [`CoreError::Cancelled`].
pub fn adopt_local(
    cache_dir: &Path,
    previews: &[LocalPreview],
    sink: &dyn ProgressSink,
) -> CoreResult<LocalOutcome> {
    let mut cache = Cache::open(cache_dir)?;
    let mut outcome = LocalOutcome::default();
    let total = previews.len() as u64;

    for (done, want) in previews.iter().enumerate() {
        // Between whole units of work, never mid-write.
        if sink.is_cancelled() {
            let _ = cache.save();
            return Err(CoreError::Cancelled);
        }
        sink.report(done as u64, Some(total), &want.title);

        // The Collection screen looks up `cache.best(&normalise(title))`
        // (`commands/artwork.rs::artwork_known`), and `enrich()` stores under
        // the same folded key. A raw `want.title` here would land the entry
        // where nothing ever reads it back.
        let key = normalise(&want.title);
        if cache.adopt(&key, ArtKind::Snap, SOURCE_ID, "png").is_some() {
            outcome.adopted += 1;
            continue;
        }
        match extract(want) {
            Some(bytes) => match cache.store(&key, ArtKind::Snap, SOURCE_ID, "png", &bytes) {
                Ok(_) => outcome.written += 1,
                Err(_) => outcome.missed += 1,
            },
            None => outcome.missed += 1,
        }
    }

    sink.report(total, Some(total), "");
    cache.save()?;
    Ok(outcome)
}

/// The named entry's bytes, or `None` for every ordinary reason it might not
/// be there. `Cache::store` is what turns the title into a path, and it goes
/// through `safe_join`, so a hostile entry name is refused there rather than
/// sanitised here.
fn extract(want: &LocalPreview) -> Option<Vec<u8>> {
    if want.entry.contains("..") {
        return None;
    }
    let mut archive = open(&want.package).ok()?;
    let entries = archive.entries().ok()?;
    let index = entries.iter().position(|entry| {
        !entry.is_dir && entry.name.replace('\\', "/") == want.entry.replace('\\', "/")
    })?;
    archive.read(index, MAX_PREVIEW_BYTES).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jobs::NoProgress;

    /// A `.rp9` is a zip. Only two entries matter here: the picture and
    /// something else to prove the right one is picked.
    fn package(dir: &Path, name: &str, entries: &[(&str, &[u8])]) -> PathBuf {
        use std::io::Write;
        let path = dir.join(name);
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (entry, bytes) in entries {
            zip.start_file(*entry, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-local-{tag}-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The Collection screen never asks the cache for the raw title — it asks
    /// for `normalise(title)`, exactly like `artwork_known` and `enrich()` do.
    /// Storing under anything else extracts and writes the picture and then
    /// never shows it, so the lookup here must go through the same fold the
    /// reader uses rather than the raw title `adopt_local` was given.
    #[test]
    fn a_preview_inside_a_package_becomes_a_cached_picture() {
        let dir = scratch("adopt");
        let cache_dir = dir.join("cache");
        let pkg = package(
            &dir,
            "TheChaosEngine.rp9",
            &[
                ("rp9-manifest.xml", b"<application/>"),
                ("rp9-preview.png", b"PNGDATA"),
            ],
        );
        let title = "The Chaos Engine";

        let outcome = adopt_local(
            &cache_dir,
            &[LocalPreview {
                title: title.into(),
                package: pkg,
                entry: "rp9-preview.png".into(),
            }],
            &NoProgress,
        )
        .unwrap();

        assert_eq!(outcome.written, 1);
        let cache = Cache::open(&cache_dir).unwrap();
        let art = cache
            .best(&normalise(title))
            .expect("found the way artwork_known looks it up: cache.best(&normalise(title))");
        assert_eq!(art.source, "rp9");
        assert_eq!(art.kind, ArtKind::Snap);
        let bytes = std::fs::read(cache_dir.join(&art.file)).unwrap();
        assert_eq!(
            bytes, b"PNGDATA",
            "the picture's own bytes, not a placeholder"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The second run must not rewrite 242 files to reach the same place —
    /// and can only find them again if `adopt` folds the title the same way
    /// `store` did on the first pass.
    #[test]
    fn a_second_pass_adopts_rather_than_rewrites() {
        let dir = scratch("second");
        let cache_dir = dir.join("cache");
        let pkg = package(&dir, "Agony.rp9", &[("rp9-preview.png", b"PNGDATA")]);
        let ask = || LocalPreview {
            title: "Agony".into(),
            package: pkg.clone(),
            entry: "rp9-preview.png".into(),
        };

        assert_eq!(
            adopt_local(&cache_dir, &[ask()], &NoProgress)
                .unwrap()
                .written,
            1
        );
        let second = adopt_local(&cache_dir, &[ask()], &NoProgress).unwrap();

        assert_eq!(second.written, 0, "nothing was written the second time");
        assert_eq!(
            second.adopted, 1,
            "and the picture was found rather than lost"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The entry name comes out of a file somebody else made.
    #[test]
    fn an_entry_that_escapes_the_cache_is_refused() {
        let dir = scratch("traversal");
        let cache_dir = dir.join("cache");
        let pkg = package(
            &dir,
            "Evil.rp9",
            &[
                ("../../evil.png", b"PNGDATA"),
                ("rp9-manifest.xml", b"<x/>"),
            ],
        );

        let outcome = adopt_local(
            &cache_dir,
            &[LocalPreview {
                title: "Evil".into(),
                package: pkg,
                entry: "../../evil.png".into(),
            }],
            &NoProgress,
        )
        .unwrap();

        assert_eq!(outcome.written, 0);
        assert_eq!(outcome.missed, 1, "refused, and counted as a miss");
        assert!(
            !dir.join("evil.png").exists() && !cache_dir.join("../../evil.png").exists(),
            "nothing was written outside the cache"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A catalogue can outlive the file it describes.
    #[test]
    fn a_package_that_is_no_longer_there_is_a_miss_and_not_an_error() {
        let dir = scratch("missing");
        let outcome = adopt_local(
            &dir.join("cache"),
            &[LocalPreview {
                title: "Gone".into(),
                package: dir.join("nothing-here.rp9"),
                entry: "rp9-preview.png".into(),
            }],
            &NoProgress,
        )
        .unwrap();

        assert_eq!(outcome.missed, 1);
        assert_eq!(outcome.written, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancelling_stops_between_packages_and_says_so() {
        struct StopAtOnce;
        impl ProgressSink for StopAtOnce {
            fn report(&self, _done: u64, _total: Option<u64>, _message: &str) {}
            fn is_cancelled(&self) -> bool {
                true
            }
        }

        let dir = scratch("cancel");
        let pkg = package(&dir, "One.rp9", &[("rp9-preview.png", b"PNGDATA")]);
        let err = adopt_local(
            &dir.join("cache"),
            &[LocalPreview {
                title: "One".into(),
                package: pkg,
                entry: "rp9-preview.png".into(),
            }],
            &StopAtOnce,
        )
        .unwrap_err();

        assert!(matches!(err, CoreError::Cancelled), "{err:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
