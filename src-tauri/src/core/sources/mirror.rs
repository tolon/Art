//! Mirrors and the fetch trait (§41.5.3).
//!
//! `core/` may not touch the network, so it declares what it needs instead:
//! [`MirrorClient`] is implemented by the shell over a real HTTP client, and by
//! a test double in memory. Every rule about *where* ART is allowed to fetch
//! from lives here, in core, where it is unit-testable and cannot be relaxed by
//! whoever writes the transport.
//!
//! ## The URL rule
//!
//! §41.5.7 puts a hard limit on the AI layer: `sources_fetch` "can only reach
//! configured mirrors, never arbitrary URLs". That guarantee is worth nothing
//! if it lives in the AI layer, so it lives here: a request is always
//! *constructed* from a configured [`Mirror`] plus a validated repository path.
//! There is no function anywhere in this module that fetches a caller-supplied
//! URL.
//!
//! ## Index files only
//!
//! ART fetches index files and packages, never HTML pages (§41.5.3). Nothing
//! here scrapes, so a mirror redesigning its website cannot break parsing.

use std::io::Write;

use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;

/// The longest URL ART will construct.
const MAX_URL_BYTES: usize = 1024;

/// One configured repository mirror.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mirror {
    /// Shown to the user: "Aminet (Germany)".
    pub name: String,
    /// Base URL, always ending in `/`.
    base_url: String,
}

impl Mirror {
    /// Configure a mirror, rejecting anything that is not a plain HTTP(S) base.
    ///
    /// A query string or fragment in a *base* means the caller is trying to
    /// build something other than `<base><path>`, which is the only shape this
    /// module supports.
    pub fn new(name: impl Into<String>, base_url: &str) -> CoreResult<Self> {
        let base = base_url.trim();

        if !(base.starts_with("http://") || base.starts_with("https://")) {
            return Err(CoreError::InvalidInput(format!(
                "mirror '{base}' must start with http:// or https://"
            )));
        }
        if base.len() > MAX_URL_BYTES {
            return Err(CoreError::InvalidInput("mirror URL is too long".into()));
        }
        if base.contains('?') || base.contains('#') {
            return Err(CoreError::InvalidInput(
                "a mirror base URL cannot contain a query or fragment".into(),
            ));
        }
        if !base.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(CoreError::InvalidInput(
                "mirror URL contains spaces or non-printable characters".into(),
            ));
        }
        // A host with no path at all still needs the trailing slash so joining
        // is pure concatenation.
        let base_url = if base.ends_with('/') {
            base.to_string()
        } else {
            format!("{base}/")
        };

        Ok(Self {
            name: name.into(),
            base_url,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Build the URL for a repository-relative path.
    ///
    /// The path is re-validated here even though the catalog already validated
    /// it: this is the last point before bytes leave the machine, and a check
    /// that only runs at parse time protects nothing against a path that
    /// arrived some other way (a saved plan, a config file, the AI layer).
    pub fn url_for(&self, repo_path: &str) -> CoreResult<String> {
        validate_fetch_path(repo_path)?;

        let url = format!("{}{repo_path}", self.base_url);
        if url.len() > MAX_URL_BYTES {
            return Err(CoreError::InvalidInput(
                "the resulting URL is too long".into(),
            ));
        }
        Ok(url)
    }
}

/// Reject any repository path that could address something other than a file
/// under the mirror's base.
fn validate_fetch_path(repo_path: &str) -> CoreResult<()> {
    let reject = |why: &str| {
        Err(CoreError::InvalidInput(format!(
            "'{repo_path}' is not a repository path: {why}"
        )))
    };

    if repo_path.is_empty() {
        return reject("it is empty");
    }
    if repo_path.len() > MAX_URL_BYTES {
        return reject("it is too long");
    }
    if !repo_path.bytes().all(|b| b.is_ascii_graphic()) {
        return reject("it contains spaces or non-printable characters");
    }
    if repo_path.starts_with('/') {
        return reject("it is absolute");
    }
    if repo_path.contains('\\') {
        return reject("it contains a backslash");
    }
    if repo_path.contains('?') || repo_path.contains('#') {
        return reject("it contains a query or fragment");
    }
    // `//` after a scheme-like prefix would re-point at another host entirely.
    if repo_path.contains("//") || repo_path.contains(':') {
        return reject("it could address another host");
    }
    for segment in repo_path.split('/') {
        if segment == "." || segment == ".." {
            return reject("it contains a relative component");
        }
    }

    Ok(())
}

/// What one fetch attempt achieved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FetchStats {
    /// Bytes written to `out` by this call.
    pub written: u64,
    /// Whether the server honoured the range request and continued an existing
    /// partial file. `false` means the caller must discard what it had.
    pub resumed: bool,
    /// The total size the server declared, when it declared one.
    pub declared_total: Option<u64>,
}

/// Somewhere ART can fetch bytes from.
///
/// Implementations live outside `core/`. A conforming implementation must:
///
/// - write only the bytes it received, in order, to `out`;
/// - set [`FetchStats::resumed`] to `true` **only** when the server actually
///   honoured `from` — a server that ignores the range and restarts from zero
///   is common, and reporting it as resumed would corrupt the partial file;
/// - stop early and return [`CoreError::Cancelled`] when
///   [`ProgressSink::is_cancelled`] becomes true. Stopping mid-transfer is safe
///   here because the destination is a `.part` file, never user data;
/// - never follow a redirect to a host outside the configured mirror.
pub trait MirrorClient: Send + Sync {
    fn fetch(
        &self,
        url: &str,
        from: u64,
        out: &mut dyn Write,
        sink: &dyn ProgressSink,
    ) -> CoreResult<FetchStats>;
}

/// Somewhere a fetch attempt writes, which can be wound back when the attempt
/// fails.
///
/// A plain `Write` is not enough. A mirror can fail *after* sending part of
/// the body — a dropped connection halfway through a 7 MB index is ordinary —
/// and if those bytes stay in the destination, the next mirror's response is
/// appended to them. The result is one file made of two mirrors, which can
/// still parse, still hash, and still be cached. Winding the destination back
/// before each attempt is what makes failover safe rather than corrupting.
pub trait FetchTarget: Write {
    /// Discard everything written so far and start from the beginning.
    fn restart(&mut self) -> CoreResult<()>;
}

impl FetchTarget for Vec<u8> {
    fn restart(&mut self) -> CoreResult<()> {
        self.clear();
        Ok(())
    }
}

impl FetchTarget for std::fs::File {
    fn restart(&mut self) -> CoreResult<()> {
        use std::io::Seek;
        self.set_len(0)?;
        self.seek(std::io::SeekFrom::Start(0))?;
        Ok(())
    }
}

impl<T: FetchTarget + ?Sized> FetchTarget for &mut T {
    fn restart(&mut self) -> CoreResult<()> {
        (**self).restart()
    }
}

/// One mirror's failure, kept so the error can name every attempt rather than
/// saying "download failed".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorAttempt {
    pub mirror: String,
    pub reason: String,
}

/// The mirror that answered, and what it delivered.
#[derive(Debug, Clone)]
pub struct FetchAttemptOutcome {
    pub mirror: String,
    pub stats: FetchStats,
    /// Mirrors that failed before this one succeeded.
    pub failures: Vec<MirrorAttempt>,
}

/// Try each mirror in configured order until one delivers.
///
/// The destination is wound back before every attempt, so a mirror that dies
/// mid-body cannot leave its bytes in front of the next mirror's response.
///
/// A cancellation stops the whole thing rather than moving to the next mirror:
/// the user asked to stop, not to try harder.
pub fn fetch_with_failover(
    mirrors: &[Mirror],
    client: &dyn MirrorClient,
    repo_path: &str,
    from: u64,
    out: &mut dyn FetchTarget,
    sink: &dyn ProgressSink,
) -> CoreResult<FetchAttemptOutcome> {
    if mirrors.is_empty() {
        return Err(CoreError::MirrorUnreachable(
            "no mirrors are configured".into(),
        ));
    }

    let mut failures: Vec<MirrorAttempt> = Vec::new();

    for mirror in mirrors {
        if sink.is_cancelled() {
            return Err(CoreError::Cancelled);
        }

        // Before, not after: the caller may also hand over a destination that
        // already has something in it.
        out.restart()?;

        // A path a mirror cannot express is a fault in the request, not in the
        // mirror: no other mirror will do better, so this propagates rather
        // than joining the failure list.
        let url = mirror.url_for(repo_path)?;

        match client.fetch(&url, from, out, sink) {
            Ok(stats) => {
                return Ok(FetchAttemptOutcome {
                    mirror: mirror.name.clone(),
                    stats,
                    failures,
                })
            }
            Err(CoreError::Cancelled) => return Err(CoreError::Cancelled),
            Err(err) => failures.push(MirrorAttempt {
                mirror: mirror.name.clone(),
                reason: err.to_string(),
            }),
        }
    }

    let detail = failures
        .iter()
        .map(|f| format!("{} ({})", f.mirror, f.reason))
        .collect::<Vec<_>>()
        .join("; ");

    Err(CoreError::MirrorUnreachable(detail))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::core::jobs::{CancelToken, NoProgress};
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// An in-memory mirror. This is what keeps CI offline (§41.5.9): the whole
    /// fetch pipeline — failover, resume, short and long bodies — is driven
    /// without a socket.
    #[derive(Default)]
    pub(crate) struct MockMirror {
        /// URL → body.
        files: HashMap<String, Vec<u8>>,
        /// URLs that always fail, and why.
        failures: HashMap<String, String>,
        /// URLs that write this many bytes and *then* fail — a connection
        /// dropped mid-body, which is what a real mirror does under load.
        dies_after: HashMap<String, usize>,
        /// When true, range requests are ignored and the whole body is sent —
        /// the behaviour of several real mirrors.
        pub ignores_range: bool,
        /// Truncate every response to this many bytes.
        pub truncate_to: Option<usize>,
        /// Append this many junk bytes to every response.
        pub pad_with: usize,
        /// Cancel the job once this many bytes have been served.
        pub cancel_after: Option<(CancelToken, u64)>,
        pub requests: Mutex<Vec<(String, u64)>>,
    }

    impl MockMirror {
        pub(crate) fn new() -> Self {
            Self::default()
        }

        pub(crate) fn with_file(mut self, url: &str, body: &[u8]) -> Self {
            self.files.insert(url.to_string(), body.to_vec());
            self
        }

        pub(crate) fn failing(mut self, url: &str, reason: &str) -> Self {
            self.failures.insert(url.to_string(), reason.to_string());
            self
        }

        /// Serve `bytes` of the body, then drop the connection.
        pub(crate) fn dying_after(mut self, url: &str, body: &[u8], bytes: usize) -> Self {
            self.files.insert(url.to_string(), body.to_vec());
            self.dies_after.insert(url.to_string(), bytes);
            self
        }

        pub(crate) fn request_count(&self) -> usize {
            self.requests.lock().unwrap().len()
        }
    }

    impl MirrorClient for MockMirror {
        fn fetch(
            &self,
            url: &str,
            from: u64,
            out: &mut dyn Write,
            sink: &dyn ProgressSink,
        ) -> CoreResult<FetchStats> {
            self.requests.lock().unwrap().push((url.to_string(), from));

            if let Some(reason) = self.failures.get(url) {
                return Err(CoreError::Io(std::io::Error::other(reason.clone())));
            }
            let body = self
                .files
                .get(url)
                .ok_or_else(|| CoreError::Io(std::io::Error::other("404")))?;

            if let Some(bytes) = self.dies_after.get(url) {
                let sent = body.len().min(*bytes);
                out.write_all(&body[..sent])?;
                return Err(CoreError::Io(std::io::Error::other(
                    "the connection ended prematurely",
                )));
            }

            let declared_total = Some(body.len() as u64);
            let resumed = from > 0 && !self.ignores_range;
            let start = if resumed { from as usize } else { 0 };
            let start = start.min(body.len());

            let mut payload = body[start..].to_vec();
            if let Some(limit) = self.truncate_to {
                payload.truncate(limit);
            }
            payload.extend(std::iter::repeat_n(b'\xff', self.pad_with));

            if let Some((token, after)) = &self.cancel_after {
                if payload.len() as u64 >= *after {
                    token.cancel();
                }
            }
            if sink.is_cancelled() {
                return Err(CoreError::Cancelled);
            }

            out.write_all(&payload)?;
            Ok(FetchStats {
                written: payload.len() as u64,
                resumed,
                declared_total,
            })
        }
    }

    fn aminet() -> Mirror {
        Mirror::new("Aminet", "https://aminet.invalid").unwrap()
    }

    #[test]
    fn a_base_url_always_gains_its_trailing_slash() {
        assert_eq!(aminet().base_url(), "https://aminet.invalid/");
        assert_eq!(
            Mirror::new("x", "https://aminet.invalid/pub/")
                .unwrap()
                .base_url(),
            "https://aminet.invalid/pub/"
        );
    }

    #[test]
    fn only_http_bases_are_accepted() {
        for bad in [
            "ftp://aminet.invalid",
            "file:///C:/",
            "aminet.invalid",
            "javascript:alert(1)",
            "https://aminet.invalid/?a=b",
            "https://aminet.invalid/#x",
            "https://amin et.invalid",
        ] {
            assert!(Mirror::new("x", bad).is_err(), "accepted {bad}");
        }
    }

    #[test]
    fn a_repository_path_joins_onto_the_base() {
        assert_eq!(
            aminet().url_for("util/libs/AmiSSL-5.5.lha").unwrap(),
            "https://aminet.invalid/util/libs/AmiSSL-5.5.lha"
        );
    }

    /// §41.5.7: fetching must be confined to configured mirrors. The check is
    /// here, not in the caller, so no caller can skip it.
    #[test]
    fn a_path_can_never_redirect_to_another_host() {
        for hostile in [
            "https://evil.invalid/x.lha",
            "//evil.invalid/x.lha",
            "/etc/passwd",
            "../../../etc/passwd",
            "util/../../etc/passwd",
            "util/libs/x.lha?callback=evil",
            "util/libs/x.lha#frag",
            "util\\libs\\x.lha",
            "util/libs/ x.lha",
            "",
        ] {
            assert!(
                aminet().url_for(hostile).is_err(),
                "accepted hostile path: {hostile:?}"
            );
        }
    }

    #[test]
    fn failover_moves_on_and_reports_which_mirror_answered() {
        let first = Mirror::new("Broken", "https://broken.invalid").unwrap();
        let second = Mirror::new("Good", "https://good.invalid").unwrap();

        let client = MockMirror::new()
            .failing("https://broken.invalid/a/b.lha", "connection refused")
            .with_file("https://good.invalid/a/b.lha", b"payload");

        let mut out = Vec::new();
        let outcome = fetch_with_failover(
            &[first, second],
            &client,
            "a/b.lha",
            0,
            &mut out,
            &NoProgress,
        )
        .unwrap();

        assert_eq!(out, b"payload");
        assert_eq!(outcome.mirror, "Good");
        assert_eq!(outcome.failures.len(), 1);
        assert_eq!(outcome.failures[0].mirror, "Broken");
    }

    /// ART-030. A mirror that dies mid-body has already written bytes into the
    /// destination. Without winding it back, the next mirror's response lands
    /// *after* them and the caller gets one file made of two mirrors — which
    /// parses, hashes and caches perfectly well while being wrong.
    ///
    /// Found by syncing against real Aminet mirrors: `aminet.net` dropped the
    /// connection 519 416 bytes into `INDEX`, and the catalog came back with
    /// 91 413 entries instead of the 85 438 the file actually contains.
    #[test]
    fn a_mirror_that_dies_mid_body_does_not_pollute_the_next_one() {
        let body = b"THE ONE TRUE BODY".to_vec();
        let mirrors = [
            Mirror::new("Dies", "https://dies.invalid").unwrap(),
            Mirror::new("Good", "https://good.invalid").unwrap(),
        ];
        let client = MockMirror::new()
            .dying_after("https://dies.invalid/a/b.lha", b"GARBAGE-PREFIX", 14)
            .with_file("https://good.invalid/a/b.lha", &body);

        let mut out = Vec::new();
        let outcome =
            fetch_with_failover(&mirrors, &client, "a/b.lha", 0, &mut out, &NoProgress).unwrap();

        assert_eq!(outcome.mirror, "Good");
        assert_eq!(
            out, body,
            "the dead mirror's bytes survived into the result"
        );
    }

    /// The same wind-back must reach a file destination, not just a buffer.
    #[test]
    fn a_file_destination_is_wound_back_too() {
        let dir = std::env::temp_dir().join(format!("art-restart-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("download.part");

        let body = b"THE ONE TRUE BODY".to_vec();
        let mirrors = [
            Mirror::new("Dies", "https://dies.invalid").unwrap(),
            Mirror::new("Good", "https://good.invalid").unwrap(),
        ];
        let client = MockMirror::new()
            .dying_after("https://dies.invalid/a/b.lha", b"GARBAGE-PREFIX", 14)
            .with_file("https://good.invalid/a/b.lha", &body);

        {
            let mut file = std::fs::File::create(&path).unwrap();
            fetch_with_failover(&mirrors, &client, "a/b.lha", 0, &mut file, &NoProgress).unwrap();
        }

        assert_eq!(std::fs::read(&path).unwrap(), body);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A destination the caller handed over dirty is still wound back — the
    /// guarantee is about what the function leaves behind, not about how
    /// careful its callers are.
    #[test]
    fn a_dirty_destination_is_wound_back_before_the_first_attempt() {
        let mirrors = [Mirror::new("Good", "https://good.invalid").unwrap()];
        let client = MockMirror::new().with_file("https://good.invalid/a/b.lha", b"payload");

        let mut out = b"LEFTOVER".to_vec();
        fetch_with_failover(&mirrors, &client, "a/b.lha", 0, &mut out, &NoProgress).unwrap();

        assert_eq!(out, b"payload");
    }

    /// "Download failed" tells a user nothing. Every attempt is named.
    #[test]
    fn all_mirrors_failing_names_all_of_them() {
        let mirrors = [
            Mirror::new("One", "https://one.invalid").unwrap(),
            Mirror::new("Two", "https://two.invalid").unwrap(),
        ];
        let client = MockMirror::new()
            .failing("https://one.invalid/a/b.lha", "timed out")
            .failing("https://two.invalid/a/b.lha", "connection refused");

        let mut out = Vec::new();
        let err = fetch_with_failover(&mirrors, &client, "a/b.lha", 0, &mut out, &NoProgress)
            .unwrap_err();

        assert_eq!(err.code(), "ART-MIRROR-UNREACHABLE");
        let text = err.to_string();
        assert!(text.contains("One"), "{text}");
        assert!(text.contains("Two"), "{text}");
        assert!(text.contains("timed out"), "{text}");
    }

    #[test]
    fn no_configured_mirrors_is_an_honest_error() {
        let mut out = Vec::new();
        let err = fetch_with_failover(&[], &MockMirror::new(), "a/b.lha", 0, &mut out, &NoProgress)
            .unwrap_err();
        assert_eq!(err.code(), "ART-MIRROR-UNREACHABLE");
    }

    /// A bad path is the request's fault. Trying four more mirrors with the
    /// same bad path would only waste the user's time.
    #[test]
    fn a_hostile_path_fails_immediately_without_trying_every_mirror() {
        let mirrors = [
            Mirror::new("One", "https://one.invalid").unwrap(),
            Mirror::new("Two", "https://two.invalid").unwrap(),
        ];
        let client = MockMirror::new();

        let mut out = Vec::new();
        let err = fetch_with_failover(
            &mirrors,
            &client,
            "../../etc/passwd",
            0,
            &mut out,
            &NoProgress,
        )
        .unwrap_err();

        assert_eq!(err.code(), "ART-INPUT-INVALID");
        assert_eq!(client.request_count(), 0, "no mirror should be contacted");
    }

    #[test]
    fn cancelling_stops_rather_than_trying_the_next_mirror() {
        let mirrors = [
            Mirror::new("One", "https://one.invalid").unwrap(),
            Mirror::new("Two", "https://two.invalid").unwrap(),
        ];
        let client = MockMirror::new().failing("https://one.invalid/a/b.lha", "boom");

        struct Cancelled;
        impl ProgressSink for Cancelled {
            fn report(&self, _: u64, _: Option<u64>, _: &str) {}
            fn is_cancelled(&self) -> bool {
                true
            }
        }

        let mut out = Vec::new();
        let err =
            fetch_with_failover(&mirrors, &client, "a/b.lha", 0, &mut out, &Cancelled).unwrap_err();

        assert_eq!(err.code(), "ART-CANCELLED");
        assert_eq!(client.request_count(), 0);
    }

    /// A mirror that ignores `Range:` must say so, or the caller would append
    /// a whole body onto a partial file and corrupt it.
    #[test]
    fn a_mirror_that_ignores_range_reports_that_it_did_not_resume() {
        let mut client = MockMirror::new().with_file("https://m.invalid/a/b.lha", b"0123456789");
        client.ignores_range = true;
        let mirrors = [Mirror::new("M", "https://m.invalid").unwrap()];

        let mut out = Vec::new();
        let outcome =
            fetch_with_failover(&mirrors, &client, "a/b.lha", 4, &mut out, &NoProgress).unwrap();

        assert!(!outcome.stats.resumed);
        assert_eq!(out, b"0123456789");
    }

    #[test]
    fn a_mirror_that_honours_range_resumes() {
        let client = MockMirror::new().with_file("https://m.invalid/a/b.lha", b"0123456789");
        let mirrors = [Mirror::new("M", "https://m.invalid").unwrap()];

        let mut out = Vec::new();
        let outcome =
            fetch_with_failover(&mirrors, &client, "a/b.lha", 4, &mut out, &NoProgress).unwrap();

        assert!(outcome.stats.resumed);
        assert_eq!(out, b"456789");
    }
}
