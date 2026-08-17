//! One enrichment run: match, fetch, cache, report.
//!
//! The rules this honours, all from the design:
//!
//! - **Politeness is a constraint, not a setting.** Requests go sequentially,
//!   at most [`REQUESTS_PER_SECOND`] per host. whdload.de is run by volunteers
//!   on a small server; opening dozens of parallel connections would finish
//!   ART's job sooner at their expense. A user cannot be asked to choose
//!   politely on someone else's behalf, so this is a constant.
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

/// At most this many requests per second, per host.
const REQUESTS_PER_SECOND: u32 = 4;

/// One picture may not exceed this. Never allocate from an unchecked length.
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;

/// What to enrich, and where to keep it.
pub struct EnrichRequest<'a> {
    /// Titles as the catalogue holds them; normalisation happens here.
    pub titles: &'a [String],
    pub sources: &'a [ConfiguredSource],
    pub cache_dir: &'a Path,
}

/// How one source did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceOutcome {
    pub id: String,
    /// Pictures written this run.
    pub written: u32,
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
struct Pace {
    last: HashMap<String, Instant>,
    gap: Duration,
}

impl Pace {
    fn new() -> Self {
        Self {
            last: HashMap::new(),
            gap: Duration::from_millis(1000 / u64::from(REQUESTS_PER_SECOND)),
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
/// Returns `Err(CoreError::Cancelled)` when the user stopped it — the cache is
/// saved first, so nothing already fetched is thrown away.
pub fn enrich(
    request: EnrichRequest<'_>,
    client: &dyn MirrorClient,
    sink: &dyn ProgressSink,
) -> CoreResult<EnrichOutcome> {
    let mut cache = Cache::open(request.cache_dir)?;
    let mut pace = Pace::new();

    let keys: Vec<String> = request.titles.iter().map(|t| normalise(t)).collect();

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

        // A configuration error and an unreachable host are the same thing to
        // the run: this source contributes nothing and the others carry on.
        let index = match (index_mirror(configured), image_mirror(configured)) {
            (Ok(index_at), Ok(images_at)) => {
                match build_index(source.as_ref(), &index_at, client, sink, &mut pace) {
                    Ok(index) => Some((index, images_at)),
                    Err(CoreError::Cancelled) => {
                        cache.save()?;
                        return Err(CoreError::Cancelled);
                    }
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
        let usable: Vec<ArtKind> = if index.by_kind.is_empty() {
            source.kinds().to_vec()
        } else {
            source
                .kinds()
                .iter()
                .copied()
                .filter(|kind| index.by_kind.contains_key(kind))
                .collect()
        };

        for (key, title) in keys.iter().zip(request.titles) {
            // Between whole titles, never mid-write.
            if sink.is_cancelled() {
                cache.save()?;
                return Err(CoreError::Cancelled);
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
                outcome.matched += 1;

                match fetch_bounded(&images_at, client, &repo_path, sink, &mut pace) {
                    Ok(bytes) => {
                        cache.store(key, *kind, source.id(), extension_of(&repo_path), &bytes)?;
                        outcome.written += 1;
                    }
                    Err(CoreError::Cancelled) => {
                        cache.save()?;
                        return Err(CoreError::Cancelled);
                    }
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
        }

        per_source.push(outcome);
    }

    cache.save()?;
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
        let dir = std::env::temp_dir().join(format!("art-enrich-{}-{name}", std::process::id()));
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

    #[test]
    fn the_extension_comes_from_the_path_and_falls_back_to_png() {
        assert_eq!(extension_of("Named_Boxarts/A.png"), "png");
        assert_eq!(extension_of("games/ico/B.jpeg"), "jpeg");
        assert_eq!(extension_of("no-extension-here"), "png");
        assert_eq!(extension_of("a.verylongextension"), "png");
    }
}
