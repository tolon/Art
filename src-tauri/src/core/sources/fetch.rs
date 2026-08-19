//! The download trust pipeline (§41.5.3).
//!
//! ```text
//! FETCH (mirror, resumable)  →  SIZE CHECK  →  SHA256  →  ARCHIVE VALIDATION
//!                                                               ↓
//!                                                     CACHE (content-addressed)
//! ```
//!
//! A download is untrusted input (§56) and gets the same treatment as anything
//! else that arrives from outside. Three properties are load-bearing:
//!
//! - **Nothing enters the cache until every gate has passed.** The download
//!   lives in `partial/` until it has been sized, hashed and — if it is an
//!   archive — structurally validated. A package that fails any gate is
//!   discarded, so an install can never be offered a file ART has not checked.
//! - **No new extraction code.** Archive validation calls the existing LHA
//!   engine. This module knows how to find and fetch; installing is somebody
//!   else's tested job (§41.5.3).
//! - **Failure and cancellation leave images untouched** (§57), because this
//!   pipeline never opens one. The worst case is an orphaned `.part` file,
//!   which the next attempt resumes from.

use std::io::Write;
use std::path::{Path, PathBuf};

use super::cache::CacheLayout;
use super::mirror::{fetch_with_failover, Mirror, MirrorClient};
use super::PackageMeta;
use crate::core::error::{CoreError, CoreResult};
use crate::core::hashing::sha256_file;
use crate::core::jobs::ProgressSink;
use crate::core::safety::atomic::atomic_write;
use crate::core::security::path::safe_join;

/// The most bytes ART will download for one package when the catalog claims no
/// usable size. A backstop against an endless response, not a policy.
const MAX_DOWNLOAD_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// The smallest size tolerance, in bytes.
const MIN_SIZE_TOLERANCE: u64 = 1024;

/// What a completed fetch produced.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FetchOutcome {
    /// The SHA-256 of the file, recorded so the Collection can spot a package
    /// changing under the same name.
    pub sha256: String,
    /// Where it now lives in the cache.
    pub path: PathBuf,
    pub bytes: u64,
    /// The mirror that answered, or `"cache"` when nothing was downloaded.
    pub mirror: String,
    pub from_cache: bool,
}

/// Fetch a package into the cache, refusing anything that fails a gate.
///
/// Reaching the cache is the *only* effect. Installing is a separate,
/// user-initiated step through the existing workflows.
pub fn fetch_package(
    meta: &PackageMeta,
    mirrors: &[Mirror],
    client: &dyn MirrorClient,
    cache: &CacheLayout,
    sink: &dyn ProgressSink,
) -> CoreResult<FetchOutcome> {
    if sink.is_cancelled() {
        return Err(CoreError::Cancelled);
    }

    let repo_path = meta.reference.path.as_str();
    cache.ensure_dirs()?;

    // Content-addressed: the same bytes are never fetched twice (§41.5.3).
    if let Some((sha256, path)) = cache.cached_object(repo_path) {
        let bytes = std::fs::metadata(&path)?.len();
        sink.report(bytes, Some(bytes), &meta.name);
        return Ok(FetchOutcome {
            sha256,
            path,
            bytes,
            mirror: "cache".into(),
            from_cache: true,
        });
    }

    let partial = cache.partial_path(repo_path);
    let downloaded = download_to_partial(meta, mirrors, client, &partial, sink)?;

    if sink.is_cancelled() {
        // The partial file stays: it is not user data, and the next attempt
        // resumes from it.
        return Err(CoreError::Cancelled);
    }

    // ---- gates ----
    if let Err(err) = check_size(meta, downloaded) {
        discard(&partial);
        return Err(err);
    }

    let sha256 = match sha256_file(&partial) {
        Ok(sha) => sha,
        Err(err) => {
            discard(&partial);
            return Err(err);
        }
    };

    if let Err(err) = validate_archive(&partial, &meta.name) {
        discard(&partial);
        return Err(err);
    }

    // ---- promotion ----
    let object = cache.object_path(&sha256)?;
    if let Some(parent) = object.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if object.is_file() {
        // Another path already produced these exact bytes.
        discard(&partial);
    } else {
        std::fs::rename(&partial, &object)?;
    }

    atomic_write(&cache.pointer_path(repo_path), sha256.as_bytes())?;

    Ok(FetchOutcome {
        sha256,
        path: object,
        bytes: downloaded,
        mirror: meta.reference.provider.clone(),
        from_cache: false,
    })
}

/// Download into the partial file, resuming when the mirror allows it.
///
/// Returns the partial file's total length afterwards.
fn download_to_partial(
    meta: &PackageMeta,
    mirrors: &[Mirror],
    client: &dyn MirrorClient,
    partial: &Path,
    sink: &dyn ProgressSink,
) -> CoreResult<u64> {
    let already = std::fs::metadata(partial).map(|m| m.len()).unwrap_or(0);
    let ceiling = download_ceiling(meta.size_bytes);

    if already >= ceiling && ceiling > 0 {
        // A partial file already larger than the package can be is not a
        // resume point, it is rubbish.
        discard(partial);
        return download_from_scratch(meta, mirrors, client, partial, ceiling, sink);
    }

    if already == 0 {
        return download_from_scratch(meta, mirrors, client, partial, ceiling, sink);
    }

    // Resume into a scratch file rather than appending straight onto the
    // partial. A mirror that ignores `Range:` replies with the *whole* body,
    // and appending that to a partial file would produce a corrupt archive
    // that still hashes and still gets cached.
    let scratch = partial.with_extension("resume");
    let outcome = {
        let file = std::fs::File::create(&scratch)?;
        let mut writer = LimitedWriter::new(file, ceiling.saturating_sub(already));
        fetch_with_failover(
            mirrors,
            client,
            &meta.reference.path,
            already,
            &mut writer,
            sink,
        )
    };

    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(err) => {
            discard(&scratch);
            return Err(err);
        }
    };

    if outcome.stats.resumed {
        let mut existing = std::fs::OpenOptions::new().append(true).open(partial)?;
        let mut scratch_file = std::fs::File::open(&scratch)?;
        std::io::copy(&mut scratch_file, &mut existing)?;
        existing.sync_all()?;
        drop(scratch_file);
        discard(&scratch);
    } else {
        // Not a resume after all — the scratch file *is* the whole download.
        std::fs::rename(&scratch, partial)?;
    }

    let total = std::fs::metadata(partial)?.len();
    sink.report(total, Some(meta.size_bytes), &meta.name);
    Ok(total)
}

fn download_from_scratch(
    meta: &PackageMeta,
    mirrors: &[Mirror],
    client: &dyn MirrorClient,
    partial: &Path,
    ceiling: u64,
    sink: &dyn ProgressSink,
) -> CoreResult<u64> {
    let result = {
        let file = std::fs::File::create(partial)?;
        let mut writer = LimitedWriter::new(file, ceiling);
        fetch_with_failover(mirrors, client, &meta.reference.path, 0, &mut writer, sink)
    };

    if let Err(err) = result {
        discard(partial);
        return Err(err);
    }

    let total = std::fs::metadata(partial)?.len();
    sink.report(total, Some(meta.size_bytes), &meta.name);
    Ok(total)
}

/// The most bytes worth accepting for a package of this claimed size.
fn download_ceiling(claimed: u64) -> u64 {
    if claimed == 0 {
        return MAX_DOWNLOAD_BYTES;
    }
    claimed
        .saturating_add(size_tolerance(claimed))
        .min(MAX_DOWNLOAD_BYTES)
}

/// How far from the claimed size a real download may land.
///
/// The index rounds — `318K`, `1.2M` — so an exact match is impossible and a
/// tight bound would reject every package. This gate exists to catch a mirror
/// serving an error page or an endless stream, not to prove integrity; the
/// SHA-256 and the archive check do that.
fn size_tolerance(claimed: u64) -> u64 {
    (claimed / 8).max(MIN_SIZE_TOLERANCE)
}

fn check_size(meta: &PackageMeta, actual: u64) -> CoreResult<()> {
    if actual == 0 {
        return Err(CoreError::IntegrityMismatch(format!(
            "'{}' downloaded as an empty file",
            meta.name
        )));
    }
    if meta.size_bytes == 0 {
        // The catalog claimed nothing usable; there is nothing to compare to.
        return Ok(());
    }

    let tolerance = size_tolerance(meta.size_bytes);
    let low = meta.size_bytes.saturating_sub(tolerance);
    let high = meta.size_bytes.saturating_add(tolerance);

    if actual < low || actual > high {
        return Err(CoreError::IntegrityMismatch(format!(
            "'{}' is {actual} bytes but the catalog lists about {} bytes",
            meta.name, meta.size_bytes
        )));
    }
    Ok(())
}

/// Structural validation for the archive formats ART understands.
///
/// Reuses the existing LHA engine — no new archive code (§41.5.3) — and adds
/// the traversal check, so an archive whose entries would escape their
/// destination is rejected before it can be offered for install.
fn validate_archive(path: &Path, name: &str) -> CoreResult<()> {
    let lower = name.to_ascii_lowercase();
    if !(lower.ends_with(".lha") || lower.ends_with(".lzh")) {
        return Ok(());
    }

    let info = crate::core::lha::open_archive(path)?;

    let root = Path::new("art-download-validation");
    for entry in &info.entries {
        safe_join(root, &entry.path).map_err(|err| {
            CoreError::SafetyRefused(format!(
                "'{name}' contains an entry that would escape its destination: {err}"
            ))
        })?;
    }

    Ok(())
}

/// Remove a file, ignoring the failure. Used on paths that are already being
/// abandoned — reporting a failed cleanup over the real error would bury it.
fn discard(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// A writer that refuses to accept more than it was promised.
///
/// The size gate runs *after* a download; this is what stops an endless
/// response from filling the disk before the gate is ever reached.
pub struct LimitedWriter<W: Write> {
    inner: W,
    limit: u64,
    remaining: u64,
}

impl<W: Write> LimitedWriter<W> {
    pub fn new(inner: W, limit: u64) -> Self {
        Self {
            inner,
            limit,
            remaining: limit,
        }
    }
}

/// Winding back the destination also restores the allowance — otherwise a
/// failed first attempt would spend the budget the retry needs.
impl<W: super::mirror::FetchTarget> super::mirror::FetchTarget for LimitedWriter<W> {
    fn restart(&mut self) -> CoreResult<()> {
        self.inner.restart()?;
        self.remaining = self.limit;
        Ok(())
    }
}

impl<W: Write> Write for LimitedWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.len() as u64 > self.remaining {
            return Err(std::io::Error::other(
                "the mirror sent more data than the catalog describes",
            ));
        }
        let written = self.inner.write(buf)?;
        self.remaining -= written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jobs::{CancelToken, NoProgress, ProgressSink};
    use crate::core::sources::mirror::tests::MockMirror;
    use crate::core::sources::{index, PROVIDER_AMINET};

    struct Fixture {
        dir: PathBuf,
        cache: CacheLayout,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "art-fetch-{name}-{}",
                crate::core::test_scratch_id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self {
                cache: CacheLayout::new(dir.join("cache")),
                dir,
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn mirrors() -> Vec<Mirror> {
        vec![Mirror::new("Test", "https://m.invalid").unwrap()]
    }

    fn url(path: &str) -> String {
        format!("https://m.invalid/{path}")
    }

    /// A package whose catalog size matches `body`. `path` is the composed
    /// repository path; the index line it is built from splits it back into
    /// the name and directory columns the real format uses.
    fn package(path: &str, body: &[u8]) -> PackageMeta {
        let (directory, name) = path.rsplit_once('/').expect("a directory and a name");
        let (entries, report) = index::parse_index(
            &format!("{name} {directory} {} 0 A test package\n", body.len()),
            PROVIDER_AMINET,
        );
        assert_eq!(report.skipped, 0);
        entries.into_iter().next().unwrap()
    }

    fn archive() -> Vec<u8> {
        crate::core::lha::tests::make_minimal_lha()
    }

    #[test]
    fn a_clean_download_lands_in_the_cache_addressed_by_its_hash() {
        let fixture = Fixture::new("clean");
        let body = archive();
        let meta = package("util/libs/Thing.lha", &body);
        let client = MockMirror::new().with_file(&url("util/libs/Thing.lha"), &body);

        let outcome =
            fetch_package(&meta, &mirrors(), &client, &fixture.cache, &NoProgress).unwrap();

        assert!(!outcome.from_cache);
        assert_eq!(outcome.bytes, body.len() as u64);
        assert_eq!(
            outcome.path,
            fixture.cache.object_path(&outcome.sha256).unwrap()
        );
        assert_eq!(std::fs::read(&outcome.path).unwrap(), body);
    }

    #[test]
    fn a_second_fetch_is_served_from_the_cache_without_a_download() {
        let fixture = Fixture::new("cachehit");
        let body = archive();
        let meta = package("util/libs/Thing.lha", &body);
        let client = MockMirror::new().with_file(&url("util/libs/Thing.lha"), &body);

        let first = fetch_package(&meta, &mirrors(), &client, &fixture.cache, &NoProgress).unwrap();
        let second =
            fetch_package(&meta, &mirrors(), &client, &fixture.cache, &NoProgress).unwrap();

        assert!(!first.from_cache);
        assert!(second.from_cache);
        assert_eq!(first.sha256, second.sha256);
        assert_eq!(
            client.request_count(),
            1,
            "the second fetch hit the network"
        );
    }

    // ---- gates ----

    #[test]
    fn a_short_body_never_reaches_the_cache() {
        let fixture = Fixture::new("short");
        let body = vec![b'x'; 100_000];
        let meta = package("util/libs/Thing.lha", &body);

        let mut client = MockMirror::new().with_file(&url("util/libs/Thing.lha"), &body);
        client.truncate_to = Some(10);

        let err =
            fetch_package(&meta, &mirrors(), &client, &fixture.cache, &NoProgress).unwrap_err();

        assert_eq!(err.code(), "ART-INTEGRITY-MISMATCH");
        assert!(!fixture.cache.partial_path("util/libs/Thing.lha").exists());
        assert!(fixture.cache.cached_object("util/libs/Thing.lha").is_none());
    }

    /// An endless response must be cut off while it is arriving, not measured
    /// afterwards.
    #[test]
    fn an_oversized_body_is_cut_off_during_the_download() {
        let fixture = Fixture::new("long");
        let body = vec![b'x'; 8192];
        let meta = package("util/libs/Thing.lha", &body);

        let mut client = MockMirror::new().with_file(&url("util/libs/Thing.lha"), &body);
        client.pad_with = 1024 * 1024;

        let err =
            fetch_package(&meta, &mirrors(), &client, &fixture.cache, &NoProgress).unwrap_err();

        // The writer refuses the body, which reads as a mirror failure.
        assert_eq!(err.code(), "ART-MIRROR-UNREACHABLE");
        assert!(fixture.cache.cached_object("util/libs/Thing.lha").is_none());
    }

    /// §41.5.3: archive validation runs before anything is cached, and it uses
    /// the existing LHA engine rather than new parsing code.
    #[test]
    fn a_body_that_is_not_a_valid_archive_never_reaches_the_cache() {
        let fixture = Fixture::new("badarchive");
        let body = vec![b'X'; 4096];
        let meta = package("util/libs/Broken.lha", &body);
        let client = MockMirror::new().with_file(&url("util/libs/Broken.lha"), &body);

        let err =
            fetch_package(&meta, &mirrors(), &client, &fixture.cache, &NoProgress).unwrap_err();

        assert_eq!(err.code(), "ART-FORMAT-MALFORMED");
        assert!(fixture
            .cache
            .cached_object("util/libs/Broken.lha")
            .is_none());
        assert!(!fixture.cache.partial_path("util/libs/Broken.lha").exists());
    }

    /// A non-archive — a readme, a text file — is cached without being parsed
    /// as one.
    #[test]
    fn a_non_archive_download_skips_archive_validation() {
        let fixture = Fixture::new("readme");
        let body = b"Short: something\nVersion: 1.0\n".to_vec();
        let meta = package("util/libs/Thing.readme", &body);
        let client = MockMirror::new().with_file(&url("util/libs/Thing.readme"), &body);

        let outcome =
            fetch_package(&meta, &mirrors(), &client, &fixture.cache, &NoProgress).unwrap();
        assert_eq!(std::fs::read(&outcome.path).unwrap(), body);
    }

    // ---- resume ----

    /// The dangerous case: a mirror that ignores `Range:` sends the whole body.
    /// Appending it to a partial file would produce a corrupt archive that
    /// still hashes cleanly and still gets cached.
    #[test]
    fn a_mirror_that_ignores_range_replaces_the_partial_instead_of_appending() {
        let fixture = Fixture::new("norange");
        let body = archive();
        let meta = package("util/libs/Thing.lha", &body);

        fixture.cache.ensure_dirs().unwrap();
        let partial = fixture.cache.partial_path("util/libs/Thing.lha");
        std::fs::write(&partial, &body[..4]).unwrap();

        let mut client = MockMirror::new().with_file(&url("util/libs/Thing.lha"), &body);
        client.ignores_range = true;

        let outcome =
            fetch_package(&meta, &mirrors(), &client, &fixture.cache, &NoProgress).unwrap();

        assert_eq!(outcome.bytes, body.len() as u64);
        assert_eq!(std::fs::read(&outcome.path).unwrap(), body);
    }

    #[test]
    fn a_mirror_that_honours_range_continues_the_partial_file() {
        let fixture = Fixture::new("resume");
        let body = archive();
        let meta = package("util/libs/Thing.lha", &body);

        fixture.cache.ensure_dirs().unwrap();
        let partial = fixture.cache.partial_path("util/libs/Thing.lha");
        std::fs::write(&partial, &body[..4]).unwrap();

        let client = MockMirror::new().with_file(&url("util/libs/Thing.lha"), &body);
        let outcome =
            fetch_package(&meta, &mirrors(), &client, &fixture.cache, &NoProgress).unwrap();

        assert_eq!(std::fs::read(&outcome.path).unwrap(), body);
        assert_eq!(
            client.requests.lock().unwrap()[0].1,
            4,
            "the resume offset was not sent"
        );
    }

    #[test]
    fn a_partial_larger_than_the_package_is_thrown_away_not_resumed() {
        let fixture = Fixture::new("bigpartial");
        let body = archive();
        let meta = package("util/libs/Thing.lha", &body);

        fixture.cache.ensure_dirs().unwrap();
        std::fs::write(
            fixture.cache.partial_path("util/libs/Thing.lha"),
            vec![b'z'; 1024 * 1024],
        )
        .unwrap();

        let client = MockMirror::new().with_file(&url("util/libs/Thing.lha"), &body);
        let outcome =
            fetch_package(&meta, &mirrors(), &client, &fixture.cache, &NoProgress).unwrap();

        assert_eq!(std::fs::read(&outcome.path).unwrap(), body);
        assert_eq!(
            client.requests.lock().unwrap()[0].1,
            0,
            "a rubbish partial must not become a resume offset"
        );
    }

    // ---- cancellation and failure ----

    /// §57: a failed or cancelled download leaves no image touched, and the
    /// cache holds nothing unverified.
    #[test]
    fn cancelling_leaves_nothing_in_the_cache() {
        let fixture = Fixture::new("cancel");
        let body = archive();
        let meta = package("util/libs/Thing.lha", &body);

        let token = CancelToken::new();
        let mut client = MockMirror::new().with_file(&url("util/libs/Thing.lha"), &body);
        client.cancel_after = Some((token.clone(), 1));

        struct TokenSink(CancelToken);
        impl ProgressSink for TokenSink {
            fn report(&self, _: u64, _: Option<u64>, _: &str) {}
            fn is_cancelled(&self) -> bool {
                self.0.is_cancelled()
            }
        }

        let err = fetch_package(
            &meta,
            &mirrors(),
            &client,
            &fixture.cache,
            &TokenSink(token),
        )
        .unwrap_err();

        assert_eq!(err.code(), "ART-CANCELLED");
        assert!(fixture.cache.cached_object("util/libs/Thing.lha").is_none());
    }

    #[test]
    fn every_mirror_failing_leaves_the_cache_empty() {
        let fixture = Fixture::new("allfail");
        let meta = package("util/libs/Thing.lha", &archive());
        let client = MockMirror::new().failing(&url("util/libs/Thing.lha"), "timed out");

        let err =
            fetch_package(&meta, &mirrors(), &client, &fixture.cache, &NoProgress).unwrap_err();

        assert_eq!(err.code(), "ART-MIRROR-UNREACHABLE");
        assert!(fixture.cache.cached_object("util/libs/Thing.lha").is_none());
        assert!(!fixture.cache.partial_path("util/libs/Thing.lha").exists());
    }

    #[test]
    fn a_hostile_repository_path_is_refused_before_any_request() {
        let fixture = Fixture::new("hostilepath");
        let mut meta = package("util/libs/Thing.lha", &archive());
        meta.reference.path = "../../../etc/passwd".into();

        let client = MockMirror::new();
        let err =
            fetch_package(&meta, &mirrors(), &client, &fixture.cache, &NoProgress).unwrap_err();

        assert_eq!(err.code(), "ART-INPUT-INVALID");
        assert_eq!(client.request_count(), 0);
    }

    // ---- size arithmetic ----

    #[test]
    fn the_size_gate_tolerates_the_index_rounding_but_not_a_wrong_file() {
        let meta = package("a/b/x.lha", &vec![0u8; 325_632]);

        // 318K rounds; a few hundred bytes either way is normal.
        assert!(check_size(&meta, 325_000).is_ok());
        assert!(check_size(&meta, 326_000).is_ok());

        assert!(check_size(&meta, 512).is_err());
        assert!(check_size(&meta, 10_000_000).is_err());
        assert!(check_size(&meta, 0).is_err());
    }

    #[test]
    fn a_catalog_with_no_usable_size_still_bounds_the_download() {
        let mut meta = package("a/b/x.lha", b"x");
        meta.size_bytes = 0;

        assert!(check_size(&meta, 12_345).is_ok(), "nothing to compare to");
        assert_eq!(download_ceiling(0), MAX_DOWNLOAD_BYTES);
    }

    #[test]
    fn size_arithmetic_never_overflows() {
        assert_eq!(download_ceiling(u64::MAX), MAX_DOWNLOAD_BYTES);
        let mut meta = package("a/b/x.lha", b"x");
        meta.size_bytes = u64::MAX;
        assert!(check_size(&meta, 1).is_err());
    }
}
