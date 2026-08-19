//! One enrichment run: match, fetch, cache, report.
//!
//! The rules this honours, all from the design:
//!
//! - **Politeness is a constraint, not a setting — and it belongs to the host.**
//!   Requests go sequentially, at a rate each source states for itself
//!   ([`ArtSource::requests_per_second`]) under a ceiling here. One number for
//!   every source was wrong in both directions: whdload.de is run by volunteers
//!   on a small server, while libretro's pictures come off GitHub's CDN, and
//!   holding the CDN to the volunteer's pace turned a one-minute job into a
//!   forty-minute one. A user is not asked to choose politely on someone else's
//!   behalf, so neither is a setting.
//! - **Only what the caller will render is fetched.** libretro publishes four
//!   kinds per title; fetching all four costs four times as long for three
//!   pictures no screen shows yet.
//! - **A source that fails is not fatal.** The run continues with the others
//!   and reports which one could not be reached.
//! - **Cancellation is checked between whole titles**, never mid-write, so a
//!   cancelled run leaves work undone but never a truncated file.
//! - **Nothing is asked twice.** A cached hit is not re-fetched and a recorded
//!   miss is not re-asked; that is what makes the second run cost nothing.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::core::artwork::cache::Cache;
use crate::core::artwork::config::{image_mirror, index_mirror, source_for, ConfiguredSource};
use crate::core::artwork::key::normalise;
use crate::core::artwork::sources::{ArtSource, SourceIndex};
use crate::core::artwork::ArtKind;
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::sources::mirror::{fetch_with_failover, Mirror, MirrorClient};

/// A ceiling no source may exceed, whatever it asks for.
///
/// A source states its own rate — politeness belongs to the host — but a
/// mistyped constant should not become a flood aimed at somebody else.
const MAX_REQUESTS_PER_SECOND: u32 = 32;

/// Never go longer than this without writing the index.
///
/// The index is the *record* of a run, and a run over 1700 titles takes the
/// better part of an hour. Saving only at the end means an interruption throws
/// the whole record away — which is not theory: the first run against a real
/// collection wrote 790 pictures, was stopped, and left nothing that knew they
/// existed, so the next run began downloading all of them again.
///
/// **Measured in time rather than titles**, because the screen reads this file
/// to show pictures as they arrive: a count would save every 30 seconds when
/// titles match and every few seconds when they do not, so a picture could sit
/// on disk unseen for half a minute for no reason the user could observe.
const SAVE_INTERVAL: Duration = Duration::from_secs(3);

/// One picture may not exceed this. Never allocate from an unchecked length.
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;

/// What to enrich, and where to keep it.
pub struct EnrichRequest<'a> {
    /// Titles as the catalogue holds them; normalisation happens here.
    pub titles: &'a [String],
    pub sources: &'a [ConfiguredSource],
    pub cache_dir: &'a Path,
    /// Which kinds to fetch.
    ///
    /// Not every kind a source *can* supply: libretro publishes four, and
    /// fetching all four takes four times as long for three pictures no screen
    /// shows yet. The caller asks for what it will render, and wave C widens
    /// this rather than the engine guessing.
    pub wanted: &'a [ArtKind],
    /// Titles whose picture the user chose by hand.
    ///
    /// No source may overwrite that choice, so a pinned title is skipped
    /// **before any source is asked** about it — not recorded as a miss,
    /// since nobody looked, and a recorded miss is never asked again.
    pub pinned: &'a [String],
}

/// How one source did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceOutcome {
    pub id: String,
    /// Pictures written this run.
    pub written: u32,
    /// Pictures found already on disk from a run that never saved its index.
    pub adopted: u32,
    /// Titles this source had something for.
    pub matched: u32,
    /// Titles this source had nothing for.
    pub missed: u32,
    /// False when its index could not be fetched at all.
    pub reachable: bool,
    /// Why it was unreachable, or why it was skipped.
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichOutcome {
    pub per_source: Vec<SourceOutcome>,
    /// Pictures already in the cache when the run started.
    pub cached_before: u32,
}

/// Holds the request rate down, per host.
///
/// The gap is set per source rather than globally: see
/// [`ArtSource::requests_per_second`] for why one number for every host was
/// wrong in both directions.
struct Pace {
    last: HashMap<String, Instant>,
    gap: Duration,
}

impl Pace {
    fn new(requests_per_second: u32) -> Self {
        let rate = requests_per_second.clamp(1, MAX_REQUESTS_PER_SECOND);
        Self {
            last: HashMap::new(),
            gap: Duration::from_millis(1000 / u64::from(rate)),
        }
    }

    /// Wait, if the last request to this host was too recent.
    fn wait_for(&mut self, host: &str) {
        if let Some(previous) = self.last.get(host) {
            let elapsed = previous.elapsed();
            if elapsed < self.gap {
                std::thread::sleep(self.gap - elapsed);
            }
        }
        self.last.insert(host.to_string(), Instant::now());
    }
}

/// Fetch one document into memory, bounded.
fn fetch_bounded(
    mirror: &Mirror,
    client: &dyn MirrorClient,
    repo_path: &str,
    sink: &dyn ProgressSink,
    pace: &mut Pace,
) -> CoreResult<Vec<u8>> {
    pace.wait_for(mirror.base_url());
    let mut body: Vec<u8> = Vec::new();
    let mirrors = [mirror.clone()];
    fetch_with_failover(&mirrors, client, repo_path, 0, &mut body, sink)?;
    if body.len() > MAX_IMAGE_BYTES {
        return Err(CoreError::InvalidInput(format!(
            "'{repo_path}' is larger than the allowed bound"
        )));
    }
    Ok(body)
}

/// The extension a repository path ends in, or `png` when it says nothing.
fn extension_of(repo_path: &str) -> &str {
    repo_path
        .rsplit_once('.')
        .map(|(_, ext)| ext)
        .filter(|ext| {
            !ext.is_empty() && ext.len() <= 4 && ext.chars().all(|c| c.is_ascii_alphanumeric())
        })
        .unwrap_or("png")
}

/// Build one source's index, in the two rounds the trait describes.
fn build_index(
    source: &dyn ArtSource,
    mirror: &Mirror,
    client: &dyn MirrorClient,
    sink: &dyn ProgressSink,
    pace: &mut Pace,
) -> CoreResult<SourceIndex> {
    let mut manifests: Vec<Vec<u8>> = Vec::new();
    for path in source.manifest_paths() {
        manifests.push(fetch_bounded(mirror, client, &path, sink, pace)?);
    }

    let mut index = SourceIndex::default();
    for (kind, path) in source.index_paths(&manifests)? {
        let bytes = fetch_bounded(mirror, client, &path, sink, pace)?;
        source.absorb_index(kind, &bytes, &mut index)?;
    }
    Ok(index)
}

/// Run the enrichment.
///
/// Returns `Err(CoreError::Cancelled)` when the user stopped it. **The index is
/// written whatever happens** — see [`enrich`]'s body: the record of what was
/// fetched is most valuable exactly when the run did not finish.
pub fn enrich(
    request: EnrichRequest<'_>,
    client: &dyn MirrorClient,
    sink: &dyn ProgressSink,
) -> CoreResult<EnrichOutcome> {
    let mut cache = Cache::open(request.cache_dir)?;
    let outcome = run(&mut cache, request, client, sink);

    // Unconditional, and deliberately not `?`. Every early return above this
    // line — cancelled, a write that failed, a source that behaved unexpectedly
    // — is a moment when losing the record costs the most, and a failure to
    // save must not replace the real outcome with a second, less useful error.
    let _ = cache.save();

    outcome
}

fn run(
    cache: &mut Cache,
    request: EnrichRequest<'_>,
    client: &dyn MirrorClient,
    sink: &dyn ProgressSink,
) -> CoreResult<EnrichOutcome> {
    let mut last_save = Instant::now();

    let keys: Vec<String> = request.titles.iter().map(|t| normalise(t)).collect();

    // Normalised once, the same way `keys` is: a pinned title is matched by
    // the same key a source would have been asked about.
    let pinned: std::collections::HashSet<String> =
        request.pinned.iter().map(|t| normalise(t)).collect();

    let cached_before = keys
        .iter()
        .flat_map(|key| ArtKind::ALL.iter().map(move |kind| (key, *kind)))
        .filter(|(key, kind)| cache.get(key, *kind).is_some())
        .count() as u32;

    let enabled: Vec<&ConfiguredSource> = request
        .sources
        .iter()
        .filter(|source| source.enabled)
        .collect();

    let total = (keys.len() * enabled.len().max(1)) as u64;
    let mut done: u64 = 0;
    let mut per_source: Vec<SourceOutcome> = Vec::new();

    for configured in enabled {
        let mut outcome = SourceOutcome {
            id: configured.id.clone(),
            written: 0,
            adopted: 0,
            matched: 0,
            missed: 0,
            reachable: true,
            note: None,
        };

        let Some(source) = source_for(&configured.id) else {
            outcome.reachable = false;
            outcome.note = Some("no source of that name is built into this ART".into());
            per_source.push(outcome);
            continue;
        };

        // Per source, because the rate belongs to the host being asked.
        let mut pace = Pace::new(source.requests_per_second());

        // A configuration error and an unreachable host are the same thing to
        // the run: this source contributes nothing and the others carry on.
        let index = match (index_mirror(configured), image_mirror(configured)) {
            (Ok(index_at), Ok(images_at)) => {
                match build_index(source.as_ref(), &index_at, client, sink, &mut pace) {
                    Ok(index) => Some((index, images_at)),
                    Err(CoreError::Cancelled) => return Err(CoreError::Cancelled),
                    Err(err) => {
                        outcome.reachable = false;
                        outcome.note = Some(err.to_string());
                        None
                    }
                }
            }
            (Err(err), _) | (_, Err(err)) => {
                outcome.reachable = false;
                outcome.note = Some(err.to_string());
                None
            }
        };

        let Some((index, images_at)) = index else {
            done += keys.len() as u64;
            sink.report(done, Some(total), &outcome.id);
            per_source.push(outcome);
            continue;
        };

        // Only kinds this source actually has an index for. A source that
        // needs no index (whdload.de builds its path from the title) offers
        // all of its kinds.
        //
        // The distinction matters: recording a miss for a kind whose index was
        // never built would say "this title has no snap" when the truth is
        // "nobody looked", and a recorded miss is never asked again — so a
        // directory the repository adds later would stay invisible forever.
        let usable: Vec<ArtKind> = source
            .kinds()
            .iter()
            .copied()
            .filter(|kind| request.wanted.contains(kind))
            .filter(|kind| index.by_kind.is_empty() || index.by_kind.contains_key(kind))
            .collect();

        for (key, title) in keys.iter().zip(request.titles) {
            // Between whole titles, never mid-write.
            if sink.is_cancelled() {
                return Err(CoreError::Cancelled);
            }

            if pinned.contains(key) {
                done += 1;
                sink.report(done, Some(total), title);
                continue;
            }

            for kind in &usable {
                if cache.get(key, *kind).is_some() || cache.is_missing(key, *kind, source.id()) {
                    continue;
                }
                let Some(repo_path) = source.locate(&index, title, *kind) else {
                    cache.record_miss(key, *kind, source.id());
                    outcome.missed += 1;
                    continue;
                };

                // The picture may already be here from a run that never got to
                // save. Asking the disk costs one `is_file`; not asking costs
                // the download again.
                let ext = extension_of(&repo_path);
                if cache.adopt(key, *kind, source.id(), ext).is_some() {
                    outcome.adopted += 1;
                    continue;
                }

                outcome.matched += 1;

                match fetch_bounded(&images_at, client, &repo_path, sink, &mut pace) {
                    Ok(bytes) => {
                        cache.store(key, *kind, source.id(), ext, &bytes)?;
                        outcome.written += 1;
                    }
                    Err(CoreError::Cancelled) => return Err(CoreError::Cancelled),
                    // An index can name a file the server no longer serves.
                    // That is a miss, not a failed run.
                    Err(_) => {
                        cache.record_miss(key, *kind, source.id());
                        outcome.missed += 1;
                    }
                }
            }

            done += 1;
            sink.report(done, Some(total), title);

            if last_save.elapsed() >= SAVE_INTERVAL {
                last_save = Instant::now();
                // Best-effort mid-run: a save that fails here is not worth
                // ending a run over, and the unconditional one in `enrich`
                // will try again on the way out.
                let _ = cache.save();
            }
        }

        per_source.push(outcome);
    }

    Ok(EnrichOutcome {
        per_source,
        cached_before,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    use crate::core::sources::mirror::FetchStats;

    const ROOT_TREE: &[u8] = br#"{"tree":[
        {"path":"Named_Boxarts","type":"tree","sha":"7a1b0e"}
    ],"truncated":false}"#;

    /// A root tree naming two directories, so a test can prove one of them is
    /// left alone.
    const ROOT_TREE_ALL: &[u8] = br#"{"tree":[
        {"path":"Named_Boxarts","type":"tree","sha":"7a1b0e"},
        {"path":"Named_Snaps","type":"tree","sha":"9f65ac"}
    ],"truncated":false}"#;

    const BOXART_TREE: &[u8] = br#"{"tree":[
        {"path":"Turrican II.png","type":"blob"}
    ],"truncated":false}"#;

    /// The fake is what keeps the suite off the network. It answers only what
    /// it was told about; anything else is a 404, which is what a real mirror
    /// does with a path nobody published.
    #[derive(Default)]
    struct FakeClient {
        bodies: BTreeMap<String, Vec<u8>>,
        asked: Mutex<Vec<String>>,
    }

    impl FakeClient {
        fn with(pairs: &[(&str, &[u8])]) -> Self {
            Self {
                bodies: pairs
                    .iter()
                    .map(|(url, body)| ((*url).to_string(), body.to_vec()))
                    .collect(),
                asked: Mutex::new(Vec::new()),
            }
        }

        fn asked(&self) -> Vec<String> {
            self.asked.lock().unwrap().clone()
        }
    }

    impl MirrorClient for FakeClient {
        fn fetch(
            &self,
            url: &str,
            _from: u64,
            out: &mut dyn std::io::Write,
            _sink: &dyn ProgressSink,
        ) -> CoreResult<FetchStats> {
            self.asked.lock().unwrap().push(url.to_string());
            match self.bodies.get(url) {
                Some(body) => {
                    out.write_all(body)?;
                    Ok(FetchStats {
                        written: body.len() as u64,
                        ..Default::default()
                    })
                }
                None => Err(CoreError::MirrorUnreachable(format!("404 {url}"))),
            }
        }
    }

    #[derive(Default)]
    struct CancelAfter {
        limit: usize,
        seen: Mutex<usize>,
        tripped: AtomicBool,
    }

    impl ProgressSink for CancelAfter {
        fn report(&self, _done: u64, _total: Option<u64>, _message: &str) {
            let mut seen = self.seen.lock().unwrap();
            *seen += 1;
            if *seen >= self.limit {
                self.tripped.store(true, Ordering::Relaxed);
            }
        }
        fn is_cancelled(&self) -> bool {
            self.tripped.load(Ordering::Relaxed)
        }
    }

    fn tempdir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-enrich-{}-{name}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn libretro_only() -> Vec<ConfiguredSource> {
        vec![ConfiguredSource {
            id: "libretro".into(),
            enabled: true,
            mirror_base: "https://index.test/".into(),
            image_base: "https://images.test/".into(),
        }]
    }

    fn libretro_client() -> FakeClient {
        FakeClient::with(&[
            (
                "https://index.test/repos/libretro-thumbnails/Commodore_-_Amiga/git/trees/master",
                ROOT_TREE,
            ),
            (
                "https://index.test/repos/libretro-thumbnails/Commodore_-_Amiga/git/trees/7a1b0e",
                BOXART_TREE,
            ),
            (
                "https://images.test/Named_Boxarts/Turrican%20II.png",
                b"PNGDATA",
            ),
        ])
    }

    /// A source configured exactly as `libretro_only()` provides it, named for
    /// what the pinned-title test needs to say about it: it is there, and it
    /// has an answer, so a written count of zero can only mean the pin worked.
    fn source_that_always_answers() -> Vec<ConfiguredSource> {
        libretro_only()
    }

    /// The same fake as every other test here, holding the reply that would
    /// have matched "Turrican" if it had been asked.
    fn client_that_would_answer() -> FakeClient {
        libretro_client()
    }

    #[test]
    fn a_matched_title_is_written_to_the_cache() {
        let dir = tempdir("matched");
        let sources = libretro_only();
        let titles = vec!["Turrican II".to_string()];
        let client = libretro_client();

        let outcome = enrich(
            EnrichRequest {
                titles: &titles,
                sources: &sources,
                cache_dir: &dir,
                wanted: &ArtKind::ALL,
                pinned: &[],
            },
            &client,
            &crate::core::jobs::NoProgress,
        )
        .unwrap();

        assert_eq!(outcome.per_source[0].written, 1);
        assert!(outcome.per_source[0].reachable);
        assert!(dir.join("boxart").exists());
    }

    /// The whole point of recording misses: the second run asks for no images.
    #[test]
    fn an_unmatched_title_is_recorded_as_a_miss_and_not_retried() {
        let dir = tempdir("miss-not-retried");
        let sources = libretro_only();
        let titles = vec!["No Such Game".to_string()];

        for _ in 0..2 {
            let client = libretro_client();
            enrich(
                EnrichRequest {
                    titles: &titles,
                    sources: &sources,
                    cache_dir: &dir,
                    wanted: &ArtKind::ALL,
                    pinned: &[],
                },
                &client,
                &crate::core::jobs::NoProgress,
            )
            .unwrap();

            assert!(
                !client.asked().iter().any(|url| url.contains("images.test")),
                "an image was fetched for a title with no match"
            );
        }
    }

    #[test]
    fn a_cached_title_is_not_fetched_again() {
        let dir = tempdir("cached");
        let sources = libretro_only();
        let titles = vec!["Turrican II".to_string()];

        let first = libretro_client();
        enrich(
            EnrichRequest {
                titles: &titles,
                sources: &sources,
                cache_dir: &dir,
                wanted: &ArtKind::ALL,
                pinned: &[],
            },
            &first,
            &crate::core::jobs::NoProgress,
        )
        .unwrap();

        let second = libretro_client();
        let outcome = enrich(
            EnrichRequest {
                titles: &titles,
                sources: &sources,
                cache_dir: &dir,
                wanted: &ArtKind::ALL,
                pinned: &[],
            },
            &second,
            &crate::core::jobs::NoProgress,
        )
        .unwrap();

        assert_eq!(outcome.per_source[0].written, 0);
        assert_eq!(outcome.cached_before, 1);
        assert!(!second.asked().iter().any(|url| url.contains("images.test")));
    }

    #[test]
    fn an_unreachable_source_does_not_stop_the_other() {
        let dir = tempdir("unreachable");
        let sources = vec![
            ConfiguredSource {
                id: "libretro".into(),
                enabled: true,
                mirror_base: "https://index.test/".into(),
                image_base: "https://images.test/".into(),
            },
            ConfiguredSource {
                id: "whdload-de".into(),
                enabled: true,
                mirror_base: "https://whd.test/".into(),
                image_base: String::new(),
            },
        ];
        let titles = vec!["Moonstone".to_string()];

        // libretro's root tree is absent; whdload.de's icon is there.
        let client = FakeClient::with(&[("https://whd.test/games/ico/Moonstone.png", b"ICON")]);

        let outcome = enrich(
            EnrichRequest {
                titles: &titles,
                sources: &sources,
                cache_dir: &dir,
                wanted: &ArtKind::ALL,
                pinned: &[],
            },
            &client,
            &crate::core::jobs::NoProgress,
        )
        .unwrap();

        let libretro = outcome
            .per_source
            .iter()
            .find(|s| s.id == "libretro")
            .unwrap();
        let whdload = outcome
            .per_source
            .iter()
            .find(|s| s.id == "whdload-de")
            .unwrap();

        assert!(!libretro.reachable);
        assert!(libretro.note.is_some());
        assert!(whdload.reachable);
        assert_eq!(whdload.written, 1);
    }

    #[test]
    fn cancellation_returns_cancelled_and_leaves_a_whole_cache() {
        let dir = tempdir("cancelled");
        let sources = libretro_only();
        let titles = vec!["Turrican II".to_string(), "Turrican II".to_string()];
        let client = libretro_client();

        let sink = CancelAfter {
            limit: 1,
            ..Default::default()
        };
        let result = enrich(
            EnrichRequest {
                titles: &titles,
                sources: &sources,
                cache_dir: &dir,
                wanted: &ArtKind::ALL,
                pinned: &[],
            },
            &client,
            &sink,
        );

        assert!(matches!(result, Err(CoreError::Cancelled)));

        // What was fetched before the stop is complete and readable.
        let cache = Cache::open(&dir).unwrap();
        let art = cache.get("turrican ii", ArtKind::Boxart).unwrap();
        assert_eq!(std::fs::read(dir.join(&art.file)).unwrap(), b"PNGDATA");
    }

    #[test]
    fn a_disabled_source_is_never_asked() {
        let dir = tempdir("disabled");
        let sources = vec![ConfiguredSource {
            id: "libretro".into(),
            enabled: false,
            mirror_base: "https://index.test/".into(),
            image_base: "https://images.test/".into(),
        }];
        let titles = vec!["Turrican II".to_string()];
        let client = libretro_client();

        let outcome = enrich(
            EnrichRequest {
                titles: &titles,
                sources: &sources,
                cache_dir: &dir,
                wanted: &ArtKind::ALL,
                pinned: &[],
            },
            &client,
            &crate::core::jobs::NoProgress,
        )
        .unwrap();

        assert!(outcome.per_source.is_empty());
        assert!(client.asked().is_empty());
    }

    /// An index can name a file the server no longer serves. That is one
    /// title's miss, not a failed run.
    #[test]
    fn an_image_the_server_does_not_have_is_a_miss_not_an_error() {
        let dir = tempdir("gone");
        let sources = libretro_only();
        let titles = vec!["Turrican II".to_string()];

        let client = FakeClient::with(&[
            (
                "https://index.test/repos/libretro-thumbnails/Commodore_-_Amiga/git/trees/master",
                ROOT_TREE,
            ),
            (
                "https://index.test/repos/libretro-thumbnails/Commodore_-_Amiga/git/trees/7a1b0e",
                BOXART_TREE,
            ),
        ]);

        let outcome = enrich(
            EnrichRequest {
                titles: &titles,
                sources: &sources,
                cache_dir: &dir,
                wanted: &ArtKind::ALL,
                pinned: &[],
            },
            &client,
            &crate::core::jobs::NoProgress,
        )
        .unwrap();

        assert!(outcome.per_source[0].reachable);
        assert_eq!(outcome.per_source[0].written, 0);
        assert_eq!(outcome.per_source[0].missed, 1);
    }

    /// The root tree here names only Named_Boxarts. The other three kinds have
    /// no index, so nobody looked — and "nobody looked" must not be written
    /// down as "there is nothing", because a recorded miss is never re-asked
    /// and a directory the repository adds later would stay invisible.
    #[test]
    fn a_kind_with_no_index_is_skipped_rather_than_recorded_as_missing() {
        let dir = tempdir("no-index-for-kind");
        let sources = libretro_only();
        let titles = vec!["Turrican II".to_string()];
        let client = libretro_client();

        let outcome = enrich(
            EnrichRequest {
                titles: &titles,
                sources: &sources,
                cache_dir: &dir,
                wanted: &ArtKind::ALL,
                pinned: &[],
            },
            &client,
            &crate::core::jobs::NoProgress,
        )
        .unwrap();

        assert_eq!(outcome.per_source[0].written, 1);
        assert_eq!(
            outcome.per_source[0].missed, 0,
            "snap, title and logo have no index and must not be called missing"
        );

        let cache = Cache::open(&dir).unwrap();
        for kind in [ArtKind::Snap, ArtKind::Title, ArtKind::Logo] {
            assert!(
                !cache.is_missing("turrican ii", kind, "libretro"),
                "{kind:?} was recorded as missing without ever being looked for"
            );
        }
    }

    /// The defect this was written for, in the shape it actually appeared in.
    ///
    /// A cancelled run used to leave its pictures on disk with nothing that
    /// knew they existed: the index was written only after the last title, so
    /// stopping at title 200 of 1700 threw away the record of all 200. The
    /// screen showed nothing and the next run downloaded every one again.
    ///
    /// Two things are asserted, and both matter. The index survives the
    /// cancellation, and the run that follows fetches **no image at all** —
    /// the second is what makes an interrupted run cheap rather than merely
    /// recoverable.
    #[test]
    fn a_cancelled_run_keeps_its_record_and_the_next_run_refetches_nothing() {
        let dir = tempdir("cancel-keeps-record");
        let sources = libretro_only();
        let titles = vec!["Turrican II".to_string(), "Turrican II".to_string()];

        let first = libretro_client();
        let sink = CancelAfter {
            limit: 1,
            ..Default::default()
        };
        let stopped = enrich(
            EnrichRequest {
                titles: &titles,
                sources: &sources,
                cache_dir: &dir,
                wanted: &ArtKind::ALL,
                pinned: &[],
            },
            &first,
            &sink,
        );
        assert!(matches!(stopped, Err(CoreError::Cancelled)));

        // The record survived the stop.
        let reloaded = Cache::open(&dir).unwrap();
        assert!(
            reloaded.get("turrican ii", ArtKind::Boxart).is_some(),
            "the index was not written, so the fetched picture is orphaned"
        );

        // And the next run pays nothing for it.
        let second = libretro_client();
        let outcome = enrich(
            EnrichRequest {
                titles: &titles,
                sources: &sources,
                cache_dir: &dir,
                wanted: &ArtKind::ALL,
                pinned: &[],
            },
            &second,
            &crate::core::jobs::NoProgress,
        )
        .unwrap();

        assert_eq!(outcome.per_source[0].written, 0);
        assert!(
            !second.asked().iter().any(|url| url.contains("images.test")),
            "an image was fetched again after a cancelled run"
        );
    }

    /// Belt and braces for the same failure: even with the index deleted
    /// outright, the pictures on disk are adopted rather than downloaded. This
    /// is what recovers the 790 files the first real run left behind.
    #[test]
    fn pictures_on_disk_without_an_index_are_adopted_not_refetched() {
        let dir = tempdir("adopt-orphans");
        let sources = libretro_only();
        let titles = vec!["Turrican II".to_string()];

        let first = libretro_client();
        enrich(
            EnrichRequest {
                titles: &titles,
                sources: &sources,
                cache_dir: &dir,
                wanted: &ArtKind::ALL,
                pinned: &[],
            },
            &first,
            &crate::core::jobs::NoProgress,
        )
        .unwrap();

        // The interruption this simulates is cruder than any real one.
        std::fs::remove_file(dir.join("index.json")).unwrap();

        let second = libretro_client();
        let outcome = enrich(
            EnrichRequest {
                titles: &titles,
                sources: &sources,
                cache_dir: &dir,
                wanted: &ArtKind::ALL,
                pinned: &[],
            },
            &second,
            &crate::core::jobs::NoProgress,
        )
        .unwrap();

        assert_eq!(outcome.per_source[0].adopted, 1);
        assert_eq!(outcome.per_source[0].written, 0);
        assert!(!second.asked().iter().any(|url| url.contains("images.test")));
        assert!(Cache::open(&dir)
            .unwrap()
            .get("turrican ii", ArtKind::Boxart)
            .is_some());
    }

    /// libretro publishes four kinds, and fetching all four costs four times as
    /// long for three pictures nothing renders. The caller says what it will
    /// show; the engine fetches that and no more.
    #[test]
    fn only_the_wanted_kinds_are_fetched() {
        let dir = tempdir("wanted");
        let sources = libretro_only();
        let titles = vec!["Turrican II".to_string()];
        let client = FakeClient::with(&[
            (
                "https://index.test/repos/libretro-thumbnails/Commodore_-_Amiga/git/trees/master",
                ROOT_TREE_ALL,
            ),
            (
                "https://index.test/repos/libretro-thumbnails/Commodore_-_Amiga/git/trees/7a1b0e",
                BOXART_TREE,
            ),
            (
                "https://index.test/repos/libretro-thumbnails/Commodore_-_Amiga/git/trees/9f65ac",
                BOXART_TREE,
            ),
            (
                "https://images.test/Named_Boxarts/Turrican%20II.png",
                b"BOX",
            ),
            ("https://images.test/Named_Snaps/Turrican%20II.png", b"SNAP"),
        ]);

        let outcome = enrich(
            EnrichRequest {
                titles: &titles,
                sources: &sources,
                cache_dir: &dir,
                wanted: &[ArtKind::Boxart],
                pinned: &[],
            },
            &client,
            &crate::core::jobs::NoProgress,
        )
        .unwrap();

        assert_eq!(outcome.per_source[0].written, 1);
        assert!(
            !client
                .asked()
                .iter()
                .any(|url| url.contains("Named_Snaps/Turrican")),
            "a kind nobody asked for was fetched"
        );

        let cache = Cache::open(&dir).unwrap();
        assert!(cache.get("turrican ii", ArtKind::Boxart).is_some());
        assert!(cache.get("turrican ii", ArtKind::Snap).is_none());
    }

    /// A source may not overwrite a picture the user chose by hand.
    ///
    /// The title and fixture are the same ones `a_matched_title_is_written_to_
    /// the_cache` uses to prove a *match* happens — so a written count of zero
    /// here can only be the pin, never a fixture that had nothing to offer.
    #[test]
    fn a_pinned_title_is_left_alone() {
        let dir = tempdir("pinned");
        let sources = source_that_always_answers();
        let titles = vec!["Turrican II".to_string()];
        let pinned = vec!["Turrican II".to_string()];
        let client = client_that_would_answer();

        let outcome = enrich(
            EnrichRequest {
                titles: &titles,
                sources: &sources,
                cache_dir: &dir,
                wanted: &[ArtKind::Boxart],
                pinned: &pinned,
            },
            &client,
            &crate::core::jobs::NoProgress,
        )
        .unwrap();

        assert_eq!(
            outcome.per_source.iter().map(|s| s.written).sum::<u32>(),
            0,
            "the user's own picture is not something a source gets to replace"
        );
        assert!(
            Cache::open(&dir)
                .unwrap()
                .get("turrican ii", ArtKind::Boxart)
                .is_none(),
            "nothing should have been written to the cache for a pinned title"
        );
    }

    /// The rate belongs to the host. One number for every source held GitHub's
    /// CDN to a volunteer server's pace.
    #[test]
    fn each_source_states_its_own_rate_and_none_exceeds_the_ceiling() {
        use crate::core::artwork::sources::{libretro::Libretro, whdload_de::WhdloadDe, ArtSource};

        assert!(
            Libretro.requests_per_second() > WhdloadDe.requests_per_second(),
            "a CDN and a volunteer's server must not be asked at the same pace"
        );
        for rate in [
            Libretro.requests_per_second(),
            WhdloadDe.requests_per_second(),
        ] {
            assert!((1..=MAX_REQUESTS_PER_SECOND).contains(&rate));
        }
    }

    #[test]
    fn the_extension_comes_from_the_path_and_falls_back_to_png() {
        assert_eq!(extension_of("Named_Boxarts/A.png"), "png");
        assert_eq!(extension_of("games/ico/B.jpeg"), "jpeg");
        assert_eq!(extension_of("no-extension-here"), "png");
        assert_eq!(extension_of("a.verylongextension"), "png");
    }
}
