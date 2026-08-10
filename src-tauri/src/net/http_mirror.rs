//! `MirrorClient` over HTTP(S).
//!
//! The trait's contract is not a formality — three of its clauses exist because
//! breaking them corrupts a download in a way that still hashes cleanly and
//! still gets cached:
//!
//! - **`resumed` is only true on `206`.** A server that ignores `Range:` and
//!   replies `200` sends the *whole* body. Reporting that as a resume would
//!   have the caller append it to a partial file.
//! - **No transparent decompression.** `gzip` is off in `Cargo.toml` and every
//!   request asks for `identity`, so the bytes written are the bytes the server
//!   counted. Otherwise the resume offset and the size gate both measure the
//!   wrong thing.
//! - **Redirects never leave the host.** `sources_fetch` is confined to
//!   configured mirrors (§41.5.7); a followed redirect is a fetch the user
//!   never configured. Same-host redirects are followed, and an HTTPS → HTTP
//!   downgrade is refused even then.
//!
//! Cancellation is checked between chunks. That is safe here because the
//! destination is always a `.part` file — never user data (see
//! `core/sources/fetch.rs`).

use std::io::{Read, Write};
use std::time::Duration;

use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::sources::mirror::{FetchStats, MirrorClient};

/// How much is read from the socket at a time. Also how often cancellation is
/// noticed, so it is a latency knob as much as a buffer size.
const CHUNK_BYTES: usize = 64 * 1024;

/// How long to wait for a connection, including the TLS handshake.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// How long to wait for response headers. Deliberately not a whole-call
/// timeout: a 7 MB index on a slow line is not a failure.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

/// How many same-host redirects to follow before giving up.
const MAX_REDIRECTS: usize = 5;

/// Fetches from repository mirrors over HTTP(S).
pub struct HttpMirrorClient {
    agent: ureq::Agent,
}

impl HttpMirrorClient {
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_recv_response(Some(RESPONSE_TIMEOUT))
            // Redirects are followed by this module, not by ureq, so the
            // host check cannot be skipped.
            .max_redirects(0)
            .max_redirects_will_error(false)
            .build();

        Self {
            agent: config.into(),
        }
    }
}

impl Default for HttpMirrorClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MirrorClient for HttpMirrorClient {
    fn fetch(
        &self,
        url: &str,
        from: u64,
        out: &mut dyn Write,
        sink: &dyn ProgressSink,
    ) -> CoreResult<FetchStats> {
        if sink.is_cancelled() {
            return Err(CoreError::Cancelled);
        }

        let mut target = url.to_string();

        for _hop in 0..=MAX_REDIRECTS {
            let mut request = self
                .agent
                .get(&target)
                // Belt and braces with the disabled `gzip` feature: ART wants
                // the bytes the server counted, not a decompressed stand-in.
                .header("Accept-Encoding", "identity");

            if from > 0 {
                request = request.header("Range", &format!("bytes={from}-"));
            }

            let mut response = request.call().map_err(transport_error)?;
            let status = response.status().as_u16();

            if (300..400).contains(&status) {
                let location = response
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| {
                        transport_message(format!("HTTP {status} with no Location header"))
                    })?
                    .to_string();

                target = redirect_target(&target, &location)?;
                continue;
            }

            // A mirror that answers 200 to a range request has not resumed —
            // it has sent the whole file, and saying otherwise corrupts the
            // caller's partial file.
            let resumed = status == 206;
            let declared_total = declared_total(&response, from, resumed);
            let body_length = header_u64(&response, "content-length");
            let label = file_label(&target);

            let base = if resumed { from } else { 0 };
            let written = stream_body(
                &mut response.body_mut().as_reader(),
                out,
                sink,
                base,
                declared_total,
                &label,
            )?;

            // A server that announces a length and then stops early has given
            // ART a truncated file that would otherwise look perfectly valid —
            // a short index parses without a single skipped line, it is just
            // missing packages. Real mirrors do this: `aminet.net` dropped the
            // connection 519 416 bytes into a 7 MB index on 2026-08-09.
            //
            // Chunked responses carry no length, so the check simply does not
            // apply to them; this is a second line of defence, not the only one.
            if let Some(expected) = body_length {
                if written != expected {
                    return Err(transport_message(format!(
                        "the mirror announced {expected} bytes but sent {written}"
                    )));
                }
            }

            return Ok(FetchStats {
                written,
                resumed,
                declared_total,
            });
        }

        Err(transport_message(format!(
            "gave up after {MAX_REDIRECTS} redirects"
        )))
    }
}

/// Copy the body through, reporting progress and stopping when asked.
fn stream_body(
    reader: &mut dyn Read,
    out: &mut dyn Write,
    sink: &dyn ProgressSink,
    base: u64,
    total: Option<u64>,
    label: &str,
) -> CoreResult<u64> {
    let mut buffer = vec![0u8; CHUNK_BYTES];
    let mut written: u64 = 0;

    loop {
        if sink.is_cancelled() {
            return Err(CoreError::Cancelled);
        }

        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }

        out.write_all(&buffer[..read])?;
        written = written.saturating_add(read as u64);
        sink.report(base.saturating_add(written), total, label);
    }

    out.flush()?;
    Ok(written)
}

/// The total size of the resource, as opposed to the length of this response.
///
/// On a `206` the interesting number is the one after the slash in
/// `Content-Range: bytes 100-999/1000` — `Content-Length` there is only the
/// slice being sent, and reporting it as the total would show a progress bar
/// that finishes at the wrong place.
fn declared_total(
    response: &ureq::http::Response<ureq::Body>,
    from: u64,
    resumed: bool,
) -> Option<u64> {
    if resumed {
        if let Some(total) = response
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            .and_then(parse_content_range_total)
        {
            return Some(total);
        }
    }

    let length = header_u64(response, "content-length")?;

    if resumed {
        length.checked_add(from)
    } else {
        Some(length)
    }
}

fn header_u64(response: &ureq::http::Response<ureq::Body>, name: &str) -> Option<u64> {
    response
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
}

/// The total from `bytes 100-999/1000`. A `*` total is unknown, not zero.
fn parse_content_range_total(value: &str) -> Option<u64> {
    let total = value.rsplit('/').next()?.trim();
    total.parse::<u64>().ok()
}

/// Resolve a `Location` against the current URL, refusing to leave the host.
///
/// §41.5.7 confines fetching to configured mirrors. A redirect that changes
/// host is a fetch nobody configured, so it is an error rather than something
/// to follow — even when the new host looks respectable.
fn redirect_target(current: &str, location: &str) -> CoreResult<String> {
    let location = location.trim();
    if location.is_empty() {
        return Err(transport_message("redirect with an empty Location"));
    }

    let (scheme, host, path) =
        split_url(current).ok_or_else(|| transport_message("cannot parse the current URL"))?;

    // Protocol-relative: "//other.host/x" changes host without a scheme.
    if let Some(rest) = location.strip_prefix("//") {
        let target = format!("{scheme}://{rest}");
        return check_same_host(scheme, host, &target);
    }

    if location.starts_with('/') {
        return Ok(format!("{scheme}://{host}{location}"));
    }

    if location.contains("://") {
        return check_same_host(scheme, host, location);
    }

    // A bare relative path, resolved against the current directory.
    let directory = match path.rfind('/') {
        Some(index) => &path[..=index],
        None => "/",
    };
    Ok(format!("{scheme}://{host}{directory}{location}"))
}

fn check_same_host(scheme: &str, host: &str, target: &str) -> CoreResult<String> {
    let (target_scheme, target_host, _) =
        split_url(target).ok_or_else(|| transport_message("cannot parse the redirect target"))?;

    if !target_host.eq_ignore_ascii_case(host) {
        return Err(transport_message(format!(
            "refused a redirect from {host} to {target_host}: only configured mirrors may be fetched"
        )));
    }
    if scheme.eq_ignore_ascii_case("https") && !target_scheme.eq_ignore_ascii_case("https") {
        return Err(transport_message(
            "refused a redirect that downgrades HTTPS to plain HTTP",
        ));
    }

    Ok(target.to_string())
}

/// Split a URL into scheme, host (with port) and path.
///
/// Credentials in the authority are refused rather than parsed: `https://
/// aminet.net@evil.invalid/` reads as the mirror to a human and as another host
/// to a client, which is exactly the confusion the host check exists to stop.
fn split_url(url: &str) -> Option<(&str, &str, &str)> {
    let (scheme, rest) = url.split_once("://")?;
    if scheme.is_empty() || rest.is_empty() {
        return None;
    }

    let (host, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, "/"),
    };

    if host.is_empty() || host.contains('@') {
        return None;
    }

    Some((scheme, host, path))
}

/// The last path segment, for progress messages.
fn file_label(url: &str) -> String {
    url.rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or(url)
        .to_string()
}

fn transport_error(error: ureq::Error) -> CoreError {
    match error {
        ureq::Error::StatusCode(code) => transport_message(format!("server returned HTTP {code}")),
        other => transport_message(other.to_string()),
    }
}

/// Network failures are `Io` so a single mirror's problem reads as one failed
/// attempt; `fetch_with_failover` turns "all of them failed" into
/// `ART-MIRROR-UNREACHABLE` with every reason named.
fn transport_message(message: impl Into<String>) -> CoreError {
    CoreError::Io(std::io::Error::other(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jobs::{CancelToken, NoProgress};
    use std::io::{BufRead, BufReader};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // ---- URL handling (pure, no socket) ----

    #[test]
    fn a_url_splits_into_scheme_host_and_path() {
        assert_eq!(
            split_url("https://aminet.net/util/libs/A.lha"),
            Some(("https", "aminet.net", "/util/libs/A.lha"))
        );
        assert_eq!(
            split_url("http://host:8080"),
            Some(("http", "host:8080", "/"))
        );
    }

    /// `https://aminet.net@evil.invalid/` reads as the mirror to a human and
    /// as another host to a client.
    #[test]
    fn credentials_in_the_authority_are_refused() {
        assert_eq!(split_url("https://aminet.net@evil.invalid/x"), None);
        assert_eq!(split_url("not a url"), None);
        assert_eq!(split_url("https://"), None);
    }

    #[test]
    fn a_same_host_redirect_is_followed() {
        let from = "https://aminet.net/INDEX";

        assert_eq!(
            redirect_target(from, "/pub/INDEX").unwrap(),
            "https://aminet.net/pub/INDEX"
        );
        assert_eq!(
            redirect_target(from, "https://aminet.net/other/INDEX").unwrap(),
            "https://aminet.net/other/INDEX"
        );
        assert_eq!(
            redirect_target("https://aminet.net/pub/INDEX", "INDEX.new").unwrap(),
            "https://aminet.net/pub/INDEX.new"
        );
        // Host comparison is case-insensitive; ports are part of the host.
        assert_eq!(
            redirect_target(from, "https://AMINET.NET/x").unwrap(),
            "https://AMINET.NET/x"
        );
    }

    /// §41.5.7: fetching is confined to configured mirrors. A redirect that
    /// leaves the host is a fetch nobody configured.
    #[test]
    fn a_redirect_off_the_host_is_refused() {
        let from = "https://aminet.net/INDEX";

        for hostile in [
            "https://evil.invalid/INDEX",
            "//evil.invalid/INDEX",
            "http://aminet.net.evil.invalid/INDEX",
            "https://aminet.net:8443/INDEX",
            "",
        ] {
            assert!(
                redirect_target(from, hostile).is_err(),
                "followed {hostile:?}"
            );
        }
    }

    #[test]
    fn a_redirect_that_downgrades_to_plain_http_is_refused() {
        let err = redirect_target("https://aminet.net/INDEX", "http://aminet.net/INDEX")
            .expect_err("a downgrade must be refused");
        assert!(err.to_string().contains("downgrade"), "{err}");
    }

    #[test]
    fn a_content_range_total_is_read_from_after_the_slash() {
        assert_eq!(parse_content_range_total("bytes 100-999/1000"), Some(1000));
        assert_eq!(parse_content_range_total("bytes 0-0/*"), None);
        assert_eq!(parse_content_range_total("nonsense"), None);
    }

    #[test]
    fn the_progress_label_is_the_file_name() {
        assert_eq!(file_label("https://aminet.net/util/libs/A.lha"), "A.lha");
        assert_eq!(file_label("https://aminet.net/INDEX"), "INDEX");
    }

    // ---- against a real socket, on localhost, offline ----

    /// What the scripted server should answer with.
    #[derive(Clone)]
    struct Scripted {
        status: &'static str,
        headers: Vec<String>,
        body: Vec<u8>,
        /// Announce this length instead of the body's real one, to imitate a
        /// mirror that drops the connection partway.
        announced_length: Option<usize>,
    }

    /// Serve one request per connection from a background thread.
    ///
    /// A real socket rather than a mock, because the properties under test —
    /// that a `200` is not reported as a resume, that a redirect off-host is
    /// refused — live in the HTTP handling itself, and a mock of ureq would
    /// only test the mock.
    fn serve(
        responses: Vec<Scripted>,
    ) -> (String, Arc<AtomicUsize>, Arc<std::sync::Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a local port");
        let port = listener.local_addr().unwrap().port();
        let served = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(std::sync::Mutex::new(Vec::new()));

        let served_thread = Arc::clone(&served);
        let requests_thread = Arc::clone(&requests);

        std::thread::spawn(move || {
            for (index, response) in responses.into_iter().enumerate() {
                let Ok((stream, _)) = listener.accept() else {
                    return;
                };
                let recorded = handle(stream, &response);
                requests_thread.lock().unwrap().push(recorded);
                served_thread.store(index + 1, Ordering::SeqCst);
            }
        });

        (format!("http://127.0.0.1:{port}"), served, requests)
    }

    /// Read the request, write the scripted response, return the request text.
    fn handle(mut stream: TcpStream, response: &Scripted) -> String {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut request = String::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            if line == "\r\n" || line == "\n" {
                break;
            }
            request.push_str(&line);
        }

        let mut head = format!("HTTP/1.1 {}\r\n", response.status);
        for header in &response.headers {
            head.push_str(header);
            head.push_str("\r\n");
        }
        let announced = response.announced_length.unwrap_or(response.body.len());
        head.push_str(&format!("Content-Length: {announced}\r\n"));
        head.push_str("Connection: close\r\n\r\n");

        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(&response.body);
        let _ = stream.flush();

        request
    }

    fn ok(body: &[u8]) -> Scripted {
        Scripted {
            status: "200 OK",
            headers: Vec::new(),
            body: body.to_vec(),
            announced_length: None,
        }
    }

    /// Block until the server thread has finished recording `count` requests.
    ///
    /// `fetch` can return before that thread runs its `push`: the client only
    /// needs the response bytes, which `handle` writes before it returns the
    /// request text to be recorded. Reading `requests` straight after `fetch`
    /// is therefore a race, and it lost about one run in five — indexing `[0]`
    /// on a vector the server had not filled yet.
    ///
    /// `served` is the synchronisation point rather than `requests` itself:
    /// the thread pushes first and stores the counter afterwards with
    /// `SeqCst`, so a counter of `count` guarantees `count` recorded requests.
    fn wait_served(served: &Arc<AtomicUsize>, count: usize) {
        for _ in 0..2000 {
            if served.load(Ordering::SeqCst) >= count {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("the test server never recorded {count} request(s)");
    }

    #[test]
    fn a_plain_download_reports_what_it_wrote() {
        let (base, served, requests) = serve(vec![ok(b"0123456789")]);

        let mut out = Vec::new();
        let stats = HttpMirrorClient::new()
            .fetch(&format!("{base}/INDEX"), 0, &mut out, &NoProgress)
            .unwrap();

        assert_eq!(out, b"0123456789");
        assert_eq!(stats.written, 10);
        assert!(!stats.resumed);
        assert_eq!(stats.declared_total, Some(10));

        // Header names go out lowercased, so match case-insensitively.
        wait_served(&served, 1);
        let sent = requests.lock().unwrap()[0].to_lowercase();
        assert!(
            !sent.contains("range:"),
            "a fresh fetch must not ask to resume"
        );
        assert!(sent.contains("identity"), "compression must be refused");
    }

    #[test]
    fn a_206_is_a_resume_and_carries_the_whole_size() {
        let (base, served, requests) = serve(vec![Scripted {
            status: "206 Partial Content",
            headers: vec!["Content-Range: bytes 4-9/10".into()],
            body: b"456789".to_vec(),
            announced_length: None,
        }]);

        let mut out = Vec::new();
        let stats = HttpMirrorClient::new()
            .fetch(&format!("{base}/INDEX"), 4, &mut out, &NoProgress)
            .unwrap();

        assert_eq!(out, b"456789");
        assert_eq!(stats.written, 6);
        assert!(stats.resumed);
        assert_eq!(
            stats.declared_total,
            Some(10),
            "the total is the resource, not the slice"
        );

        wait_served(&served, 1);
        let sent = requests.lock().unwrap()[0].to_lowercase();
        assert!(sent.contains("range: bytes=4-"), "sent:\n{sent}");
    }

    /// The clause that matters most: a server that ignores `Range:` sends the
    /// whole body, and calling that a resume would corrupt the partial file.
    #[test]
    fn a_200_answer_to_a_range_request_is_not_a_resume() {
        let (base, _, _) = serve(vec![ok(b"0123456789")]);

        let mut out = Vec::new();
        let stats = HttpMirrorClient::new()
            .fetch(&format!("{base}/INDEX"), 4, &mut out, &NoProgress)
            .unwrap();

        assert!(!stats.resumed, "a 200 is never a resume");
        assert_eq!(out, b"0123456789");
        assert_eq!(stats.declared_total, Some(10));
    }

    #[test]
    fn a_same_host_redirect_is_followed_over_the_wire() {
        let (base, served, _) = serve(vec![
            Scripted {
                status: "301 Moved Permanently",
                headers: vec!["Location: /moved/INDEX".into()],
                body: Vec::new(),
                announced_length: None,
            },
            ok(b"after the redirect"),
        ]);

        let mut out = Vec::new();
        HttpMirrorClient::new()
            .fetch(&format!("{base}/INDEX"), 0, &mut out, &NoProgress)
            .unwrap();

        assert_eq!(out, b"after the redirect");
        wait_served(&served, 2);
        assert_eq!(served.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_redirect_to_another_host_fails_instead_of_being_followed() {
        let (base, _, _) = serve(vec![Scripted {
            status: "302 Found",
            headers: vec!["Location: http://evil.invalid/INDEX".into()],
            body: Vec::new(),
            announced_length: None,
        }]);

        let mut out = Vec::new();
        let err = HttpMirrorClient::new()
            .fetch(&format!("{base}/INDEX"), 0, &mut out, &NoProgress)
            .unwrap_err();

        assert!(err.to_string().contains("evil.invalid"), "{err}");
        assert!(out.is_empty());
    }

    #[test]
    fn an_error_status_is_a_failed_attempt_not_a_body() {
        let (base, _, _) = serve(vec![Scripted {
            status: "404 Not Found",
            headers: Vec::new(),
            body: b"<html>not here</html>".to_vec(),
            announced_length: None,
        }]);

        let mut out = Vec::new();
        let err = HttpMirrorClient::new()
            .fetch(&format!("{base}/INDEX"), 0, &mut out, &NoProgress)
            .unwrap_err();

        assert!(err.to_string().contains("404"), "{err}");
        assert!(out.is_empty(), "an error page must never become the file");
    }

    /// The other half of ART-030: a mirror that announces a length and then
    /// stops early. A truncated index parses without a single skipped line —
    /// it is just missing packages — so this must never come back as success.
    ///
    /// In practice ureq's own framing check fires first and reports the
    /// disconnect; the explicit length comparison in `fetch` covers a server
    /// that closes tidily on a short body. Either way the fetch fails, which is
    /// what the caller depends on.
    #[test]
    fn a_body_shorter_than_the_announced_length_is_never_a_success() {
        let (base, _, _) = serve(vec![Scripted {
            status: "200 OK",
            headers: Vec::new(),
            body: b"only the beginning".to_vec(),
            announced_length: Some(10_000),
        }]);

        let mut out = Vec::new();
        let result =
            HttpMirrorClient::new().fetch(&format!("{base}/INDEX"), 0, &mut out, &NoProgress);

        assert!(
            result.is_err(),
            "a truncated body was accepted as complete: {} bytes",
            out.len()
        );
    }

    #[test]
    fn a_cancelled_job_stops_before_asking_for_anything() {
        struct Cancelled;
        impl ProgressSink for Cancelled {
            fn report(&self, _: u64, _: Option<u64>, _: &str) {}
            fn is_cancelled(&self) -> bool {
                true
            }
        }

        let (base, served, _) = serve(vec![ok(b"never read")]);

        let mut out = Vec::new();
        let err = HttpMirrorClient::new()
            .fetch(&format!("{base}/INDEX"), 0, &mut out, &Cancelled)
            .unwrap_err();

        assert_eq!(err.code(), "ART-CANCELLED");
        assert_eq!(served.load(Ordering::SeqCst), 0);
    }

    /// Progress is reported as it goes, not once at the end — a 7 MB index on
    /// a slow line would otherwise look frozen.
    #[test]
    fn progress_is_reported_while_the_body_arrives() {
        struct Counting {
            reports: std::sync::Mutex<Vec<u64>>,
            cancel: CancelToken,
        }
        impl ProgressSink for Counting {
            fn report(&self, done: u64, _: Option<u64>, _: &str) {
                self.reports.lock().unwrap().push(done);
            }
            fn is_cancelled(&self) -> bool {
                self.cancel.is_cancelled()
            }
        }

        let body = vec![b'x'; CHUNK_BYTES * 3];
        let (base, _, _) = serve(vec![ok(&body)]);

        let sink = Counting {
            reports: std::sync::Mutex::new(Vec::new()),
            cancel: CancelToken::new(),
        };
        let mut out = Vec::new();
        HttpMirrorClient::new()
            .fetch(&format!("{base}/INDEX"), 0, &mut out, &sink)
            .unwrap();

        let reports = sink.reports.lock().unwrap();
        assert!(reports.len() > 1, "only {} report(s)", reports.len());
        assert_eq!(*reports.last().unwrap(), body.len() as u64);
        assert_eq!(out.len(), body.len());
    }
}
