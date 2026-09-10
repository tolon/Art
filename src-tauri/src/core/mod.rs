//! Amiga Core Engine
//!
//! Platform-independent Rust modules for Amiga file-format handling.
//! This crate MUST NOT depend on `tauri` — it is pure Rust (`std` + `serde`)
//! so it stays unit-testable and reusable by a future CLI or other shells.
//!
//! See `docs/architecture.md` for the layered design.

pub mod adf;
pub mod amigaicon;
pub mod amigainstall;
pub mod amiganet;
pub mod amigaprefs;
pub mod amigaver;
pub mod analysis;
pub mod appearance;
pub mod archive;
pub mod artwork;
pub mod binary;
pub mod card;
pub mod cbm;
pub mod compatibility;
pub mod conversion;
pub mod detect;
pub mod dirsize;
pub mod distro;
pub mod error;
pub mod fat32;
pub mod firstboot;
pub mod gameindex;
pub mod gotek;
pub mod hashing;
pub mod hdf;
pub mod hostfs;
pub mod icongrid;
pub mod ilbm;
pub mod iso;
pub mod jobs;
pub mod launch;
pub mod layout;
pub mod lha;
pub mod mbr;
pub mod oplog;
pub mod osinstall;
pub mod picture;
pub mod pistorm;
pub mod preload;
pub mod profile;
pub mod rdb;
pub mod recovery;
pub mod rom;
pub mod safety;
pub mod security;
pub mod sources;
pub mod validation;
pub mod vhd;
pub mod volume;
pub mod whdload;
pub mod winuae;
pub mod workflow;

#[allow(unused_imports)]
pub use error::{CoreError, CoreResult};

/// A component that makes a test's scratch directory name unique — process
/// id plus a counter that never repeats within the process.
///
/// **Why a counter and not a timestamp** (ART-164, ART-115, ART-173). Cargo
/// runs tests in parallel threads of **one** process, so the pid is shared by
/// every one of them, and `SystemTime::now()` does not advance between two
/// calls that land in the same clock tick — coarse on Windows. Two tests then
/// build the same directory name, and whichever writes second hands the other
/// its fixture. That has been diagnosed three times in this codebase now:
/// `core::iso` (5 failures in 40 runs, four *different* tests losing the
/// race), `core::cbm` (4 in 40), and `net`'s own test server before them.
///
/// It is a `String` rather than a number so a helper that already formats
/// `"{tag}-{}"` with `std::process::id()` can swap one call for another and
/// gain the counter without its format string changing at all — which is how
/// the sweep across every scratch helper in the crate stayed mechanical and
/// reviewable.
///
/// Test-only: nothing ART ships names a file this way.
#[cfg(test)]
pub fn test_scratch_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// A test scratch directory that removes itself when it goes out of scope.
///
/// **Why this exists rather than a bare `PathBuf` and a trailing
/// `remove_dir_all`** (ART-184). A trailing statement is skipped whenever the
/// test panics, and a scratch name is unique per run, so nothing ever removes
/// a previous run's directory. Those two together put 169,291 directories —
/// about 987 GB — into `%TEMP%` in a single session and filled a 2 TB system
/// drive, after which hundreds of tests failed with `StorageFull` and every
/// measurement taken that evening was worthless.
///
/// A red suite is exactly when leaking hurts most, and a trailing cleanup is
/// exactly what a red suite skips. `Drop` runs on the panicking path too.
#[cfg(test)]
pub struct ScratchDir(std::path::PathBuf);

#[cfg(test)]
impl ScratchDir {
    pub fn new(prefix: &str, tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("{prefix}-{tag}-{}", test_scratch_id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    pub fn path(&self) -> &std::path::Path {
        &self.0
    }

    /// Deliberately a method rather than a `Deref<Target = Path>`. `Deref`
    /// would make `ScratchDir::new(..).join("x")` compile, and that temporary
    /// drops — deleting the directory — before the joined path is ever used.
    /// An explicit method keeps the guard's lifetime visible at the call site.
    pub fn join(&self, tail: impl AsRef<std::path::Path>) -> std::path::PathBuf {
        self.0.join(tail)
    }

    /// The guard and the path it guards, for the one-line local helpers every
    /// test module carries (ART-281): `let (_guard, dir) = scratch("x")` keeps
    /// the directory until the test's scope ends and leaves `dir` the plain
    /// `PathBuf` the body already used. `_guard` — never `_`, which drops at
    /// once and is the leak back in one character.
    pub fn pair(prefix: &str, tag: &str) -> (Self, std::path::PathBuf) {
        let guard = Self::new(prefix, tag);
        let dir = guard.0.clone();
        (guard, dir)
    }
}

#[cfg(test)]
impl Drop for ScratchDir {
    fn drop(&mut self) {
        // Said, not swallowed (ART-281): on Windows `remove_dir_all` fails
        // while any handle in the tree is still open, and a scratch that
        // survives silently is how 763 GB accumulated. A panic here would
        // abort a panicking test twice; stderr is the honest middle.
        if let Err(err) = std::fs::remove_dir_all(&self.0) {
            if self.0.exists() {
                eprintln!("ScratchDir: {} not removed: {err}", self.0.display());
            }
        }
    }
}

/// ART-281: the guard hands out its path, and the path stops existing when the
/// guard does. Both halves are asserted — a `Drop` that never ran and a `Drop`
/// that ran too early are different defects, and only the pair of tests tells
/// them apart.
#[cfg(test)]
#[test]
fn scratch_pair_removes_its_directory_when_the_guard_drops() {
    let (guard, dir) = ScratchDir::pair("art-core", "pair");
    assert!(dir.is_dir(), "pair must create the directory it hands back");
    drop(guard);
    assert!(
        !dir.exists(),
        "the directory must be gone once the guard is dropped: {}",
        dir.display()
    );
}

#[cfg(test)]
#[test]
fn scratch_pair_keeps_the_directory_while_the_guard_lives() {
    let (_guard, dir) = ScratchDir::pair("art-core", "pair-live");
    let file = dir.join("kept.txt");
    std::fs::write(&file, b"kept").unwrap();
    assert_eq!(
        std::fs::read(&file).unwrap(),
        b"kept",
        "the scratch must still hold what the test wrote while `_guard` is in scope"
    );
    assert!(dir.is_dir(), "the directory must outlive the write");
}

/// ART-274: `core/` may declare a trait for something platform-specific, but
/// must never spawn a process itself — that is `tools/`'s job (CLAUDE.md,
/// "The core independence rule"). `VolumeFormatter` and `HostRecycler` are
/// the shape to copy; `EmulatorLauncher` (`core::amigainstall::run`) is the
/// third, closed by moving `WinUaeLauncher`'s `Command::new` out to
/// `tools::winuae_launcher`.
///
/// This walks the real, on-disk `core/` tree rather than a fixture, for the
/// same reason `osinstall::package`'s
/// `every_package_json_file_on_disk_is_wired_into_shipped_json` does: what
/// matters is whether the source tree itself still holds the promise, not a
/// copy of it that can drift out from under the check.
#[cfg(test)]
mod independence {
    use std::path::{Path, PathBuf};

    /// Every `.rs` file under `core/`, recursively.
    fn core_files() -> Vec<PathBuf> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/core");
        let mut files = Vec::new();
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir)
                .unwrap_or_else(|err| panic!("reading {}: {err}", dir.display()))
            {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    files.push(path);
                }
            }
        }
        files
    }

    fn indent_of(line: &str) -> usize {
        line.len() - line.trim_start_matches(' ').len()
    }

    /// Replace every byte inside a string literal — plain `"…"` or raw
    /// `r"…"` / `r#"…"#` / `r##"…"##`… of any hash count — a `//` line
    /// comment, a nested `/* … */` block comment, or a char literal (`'x'`,
    /// `'\''`, `'"'`, `'\n'`, `'\u{2603}'`) — with a space, so
    /// [`test_regions`]'s brace matching below can never mistake a `}` that
    /// is really just data, or commentary, for the end of a real Rust block.
    /// A multi-line span is handled the same as a single-line one: only the
    /// newline bytes inside a stripped span are kept as-is, so line numbers
    /// and every other line's own leading indentation stay exactly what they
    /// were — [`test_regions`] can keep indexing by line number afterwards
    /// without knowing anything changed.
    ///
    /// **Follow-up to ART-274.** A fixture string exercising this very guard
    /// (see `test_regions_covers_a_signature_that_wraps_across_lines` below)
    /// used to have to be indented four spaces past its own content, purely
    /// so a flush-left `}` inside it would not land at the same indent as
    /// the real `#[cfg(test)] mod independence {` this file opens with —
    /// which would end that real region early and turn every line after the
    /// fixture into a false offender. Stripping string content first removes
    /// the need for that workaround rather than merely working around it
    /// again.
    ///
    /// **I1 (2026-09-07 final review).** This module's own doc used to claim
    /// "only a string literal has ever actually tripped this guard" and
    /// deliberately left comments and char literals untracked on that
    /// premise. Measured against the real tree, that premise was false: an
    /// unpaired `"` inside a `//` comment (719 such comment lines in
    /// `core/` at the time) or a `'"'` char literal opens exactly the same
    /// kind of phantom span a bare string quote does — `amigainstall/
    /// finish.rs`'s own module doc has one at line 28 (`` `refuse_shell_
    /// metacharacters` refuses `"` *because a quote `` — the backtick-quoted
    /// `"` has no partner on that line), and it alone was enough to blank
    /// this file's `#[cfg(test)]` attribute and leave `test_regions` with
    /// zero regions for a file that plainly has a test module. Comments and
    /// char literals are stripped for exactly the reason strings already
    /// were: so `test_regions`'s indent-based brace matching is asked of the
    /// *code*, never of prose or data that merely looks like it.
    fn strip_string_literals(text: &str) -> String {
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];

            // A raw string: `r` or `br`, then zero or more `#`, then `"`.
            if c == 'r' || (c == 'b' && chars.get(i + 1) == Some(&'r')) {
                let prefix_start = i;
                let mut j = if c == 'b' { i + 2 } else { i + 1 };
                let mut hashes = 0usize;
                while chars.get(j) == Some(&'#') {
                    hashes += 1;
                    j += 1;
                }
                if chars.get(j) == Some(&'"') {
                    for &ch in &chars[prefix_start..=j] {
                        out.push(if ch == '\n' { '\n' } else { ' ' });
                    }
                    i = j + 1;
                    loop {
                        match chars.get(i) {
                            None => break,
                            Some('"') => {
                                let close_start = i;
                                let mut k = i + 1;
                                let mut seen = 0usize;
                                while seen < hashes && chars.get(k) == Some(&'#') {
                                    seen += 1;
                                    k += 1;
                                }
                                if seen == hashes {
                                    for &ch in &chars[close_start..k] {
                                        out.push(if ch == '\n' { '\n' } else { ' ' });
                                    }
                                    i = k;
                                    break;
                                }
                                out.push(' ');
                                i += 1;
                            }
                            Some('\n') => {
                                out.push('\n');
                                i += 1;
                            }
                            Some(_) => {
                                out.push(' ');
                                i += 1;
                            }
                        }
                    }
                    continue;
                }
                // Not actually a raw string (e.g. a bare `r` or `br`
                // identifier fragment) — fall through and push `c` as
                // ordinary text below.
            }

            if c == '"' {
                out.push(' ');
                i += 1;
                loop {
                    match chars.get(i) {
                        None => break,
                        // An escape consumes the backslash and the one
                        // character after it together, so an escaped quote
                        // (`\"`) can never be misread as the closing quote.
                        Some('\\') => {
                            out.push(' ');
                            i += 1;
                            if let Some(&next) = chars.get(i) {
                                out.push(if next == '\n' { '\n' } else { ' ' });
                                i += 1;
                            }
                        }
                        Some('"') => {
                            out.push(' ');
                            i += 1;
                            break;
                        }
                        Some('\n') => {
                            out.push('\n');
                            i += 1;
                        }
                        Some(_) => {
                            out.push(' ');
                            i += 1;
                        }
                    }
                }
                continue;
            }

            // A `//` line comment runs to the end of the line — never past
            // it, so a `#[cfg(test)]` attribute on the next line is
            // untouched.
            if c == '/' && chars.get(i + 1) == Some(&'/') {
                out.push(' ');
                out.push(' ');
                i += 2;
                while let Some(&ch) = chars.get(i) {
                    if ch == '\n' {
                        break;
                    }
                    out.push(' ');
                    i += 1;
                }
                continue;
            }

            // A `/* … */` block comment, nested as Rust itself allows —
            // `depth` tracks how many unclosed openers are in scope so an
            // inner `/* */` pair does not end the outer one early.
            if c == '/' && chars.get(i + 1) == Some(&'*') {
                out.push(' ');
                out.push(' ');
                i += 2;
                let mut depth = 1usize;
                while depth > 0 {
                    match chars.get(i) {
                        None => break,
                        Some('*') if chars.get(i + 1) == Some(&'/') => {
                            out.push(' ');
                            out.push(' ');
                            i += 2;
                            depth -= 1;
                        }
                        Some('/') if chars.get(i + 1) == Some(&'*') => {
                            out.push(' ');
                            out.push(' ');
                            i += 2;
                            depth += 1;
                        }
                        Some('\n') => {
                            out.push('\n');
                            i += 1;
                        }
                        Some(_) => {
                            out.push(' ');
                            i += 1;
                        }
                    }
                }
                continue;
            }

            // A char literal: `'x'`, an escape (`'\''`, `'"'`, `'\n'`,
            // `'\xNN'`, `'\u{…}'`), or neither — a lifetime (`'a`,
            // `'static`), which this deliberately leaves alone by falling
            // through and re-scanning the quote as ordinary text one
            // character at a time, since a lifetime is never followed by an
            // immediate closing `'`.
            if c == '\'' {
                if chars.get(i + 1) == Some(&'\\') {
                    let mut j = i + 2;
                    match chars.get(j) {
                        Some('u') => {
                            j += 1;
                            if chars.get(j) == Some(&'{') {
                                j += 1;
                                while matches!(chars.get(j), Some(ch) if *ch != '}') {
                                    j += 1;
                                }
                                if chars.get(j) == Some(&'}') {
                                    j += 1;
                                }
                            }
                        }
                        Some('x') => {
                            j += 1;
                            for _ in 0..2 {
                                if chars.get(j).is_some_and(|ch| ch.is_ascii_hexdigit()) {
                                    j += 1;
                                }
                            }
                        }
                        Some(_) => {
                            j += 1;
                        }
                        None => {}
                    }
                    if chars.get(j) == Some(&'\'') {
                        for &ch in &chars[i..=j] {
                            out.push(if ch == '\n' { '\n' } else { ' ' });
                        }
                        i = j + 1;
                        continue;
                    }
                    // Not a valid escape after all — fall through and treat
                    // the opening quote as ordinary text.
                } else if let Some(&next) = chars.get(i + 1) {
                    if next != '\'' && chars.get(i + 2) == Some(&'\'') {
                        for &ch in &chars[i..=i + 2] {
                            out.push(if ch == '\n' { '\n' } else { ' ' });
                        }
                        i += 3;
                        continue;
                    }
                }
                out.push(c);
                i += 1;
                continue;
            }

            out.push(c);
            i += 1;
        }
        out
    }

    /// The `(start, end)` line ranges (0-based, inclusive) a `#[cfg(test)]`
    /// covers — the attribute line itself through to the `}` that closes the
    /// item it is attached to, or through the item's own declaration when it
    /// has no block body (`#[cfg(test)] mod x;`).
    ///
    /// Matching on **indent** rather than counting braces is deliberate: a
    /// WinUAE `.uae` line or an AmigaDOS script assembled with `format!`
    /// carries plenty of literal `{`/`}` inside string data (see
    /// `tools::winuae_launcher`'s own `real_version_hook`), and a brace
    /// counter would misread those. `cargo fmt --check` is blocking in CI,
    /// so a block's closing brace is always aligned with the line that opened
    /// it — indent is the reliable signal this codebase's own formatting
    /// already guarantees. [`strip_string_literals`] runs first so that
    /// "indent" and "trim() == \"}\"" are asked of the *code*, not of
    /// whatever a string literal happens to contain — a `}` inside string
    /// data can no longer be mistaken for indentation-matched code at all,
    /// which is the follow-up half of the same defect: matching by indent
    /// alone still could not tell a flush-left `}` *inside a string* from a
    /// real one at the same indent.
    ///
    /// **The item's own declaration can span several lines** — a
    /// rustfmt-wrapped function signature, most often — so the line that
    /// opens the block is found by scanning forward, not by checking only
    /// the line right after the attribute(s). A round of review found the
    /// single-line check missing exactly this: `core/volume/write/mod.rs`'s
    /// `#[cfg(test)] pub(crate) fn commit_blocks(` wraps its three
    /// parameters across their own lines before `{`, and the old check
    /// covered only the attribute plus that first signature line — the body,
    /// where a stray `Command::new(` would actually sit, was left outside
    /// the region. The scan is bounded: a line ending in `;` (a bodyless
    /// item, `#[cfg(test)] mod x;`) or a blank line means there is no block
    /// to find, and it stops there instead of reading past the item.
    ///
    /// `label` identifies the source for the panic message below only — the
    /// real file's own path from [`core_never_spawns_a_process_outside_a_test`],
    /// or a short description from a fixture-built unit test.
    fn test_regions(label: &str, lines: &[&str]) -> Vec<(usize, usize)> {
        let stripped = strip_string_literals(&lines.join("\n"));
        let scan: Vec<&str> = stripped.lines().collect();
        // Fix round 1 (batch-4 review, Minor): `strip_string_literals` keeps
        // every newline byte exactly where it was, by construction, so this
        // must always hold. A silent fallback to the unstripped lines on
        // mismatch used to fail open — reopening the exact
        // flush-left-brace-inside-a-string hazard this guard exists to
        // close, with nothing on screen saying it had happened. A stripper
        // bug belongs in `strip_string_literals`'s own test coverage, not in
        // a quiet revert here.
        assert_eq!(
            scan.len(),
            lines.len(),
            "{label}: strip_string_literals changed the line count ({} raw vs {} \
             stripped) — every newline byte it sees must be preserved exactly; this is a \
             bug in the stripper itself, not something test_regions may silently work \
             around by falling back to the unstripped lines",
            lines.len(),
            scan.len()
        );
        let scan: &[&str] = &scan;

        let mut regions = Vec::new();
        let mut i = 0;
        while i < scan.len() {
            if scan[i].trim() == "#[cfg(test)]" {
                let indent = indent_of(scan[i]);
                let start = i;
                let mut j = i + 1;
                // Stacked attributes (`#[cfg(test)]` then `#[test]`, say) sit
                // at the same indent as the item they both apply to.
                while j < scan.len()
                    && indent_of(scan[j]) == indent
                    && scan[j].trim_start().starts_with('#')
                {
                    j += 1;
                }
                // Scan the item's own declaration, however many lines it
                // wraps across, for the line that actually opens the block.
                let mut sig_end = j;
                let opens_block = loop {
                    match scan.get(sig_end) {
                        None => break false,
                        Some(line) => {
                            let trimmed = line.trim_end();
                            if trimmed.ends_with('{') {
                                break true;
                            }
                            if trimmed.ends_with(';') || trimmed.trim().is_empty() {
                                break false;
                            }
                            sig_end += 1;
                        }
                    }
                };
                let end = if opens_block {
                    let mut k = sig_end + 1;
                    while k < scan.len() && !(indent_of(scan[k]) == indent && scan[k].trim() == "}")
                    {
                        k += 1;
                    }
                    k.min(scan.len().saturating_sub(1))
                } else {
                    sig_end.min(scan.len().saturating_sub(1))
                };
                regions.push((start, end));
                i = end + 1;
            } else {
                i += 1;
            }
        }
        regions
    }

    /// A round of review simulated this exact shape against
    /// `core/volume/write/mod.rs:995-1000` and found 2 of 9 lines covered —
    /// the attribute plus the first signature line, with the wrapped
    /// parameters and the body both left outside the region. This is the
    /// regression test for the fix above: (a) a single-line-signature test
    /// item, (b) a wrapped, multi-line-signature test item whose body holds
    /// a `Command::new(`, and (c) ordinary code after both. (a) and (b) must
    /// be fully covered; (c) must not be swept in.
    #[test]
    fn test_regions_covers_a_signature_that_wraps_across_lines() {
        // Indented by 4 spaces relative to its own content on purpose: this
        // literal becomes part of *this file's own bytes*, and the guard
        // being tested scans its own source. A fake snippet flush at column
        // 0 would put a `}` at indent 0 right where `core/mod.rs`'s real
        // `#[cfg(test)] mod independence {` (also indent 0) is scanning for
        // *its* closing brace — ending that real region early and turning
        // every line after this test into a false offender. Discovered by
        // running this exact test the first time it was written.
        let src = "\
    mod scratch {
        #[cfg(test)]
        fn single_line_test() {
            let _ = 1;
        }

        #[cfg(test)]
        pub(crate) fn wrapped_signature_test(
            a: u32,
            b: u32,
        ) -> u32 {
            let _ = std::process::Command::new(\"x\");
            a + b
        }

        fn production_after(a: u32) -> u32 {
            a
        }
    }
";
        let lines: Vec<&str> = src.lines().collect();
        let regions = test_regions("wrapped-signature fixture", &lines);

        let single_line_body = lines.iter().position(|l| l.contains("let _ = 1;")).unwrap();
        assert!(
            is_test_line(&regions, single_line_body),
            "a single-line-signature #[cfg(test)] item's body must be covered"
        );

        let wrapped_signature_line = lines
            .iter()
            .position(|l| l.contains("pub(crate) fn wrapped_signature_test("))
            .unwrap();
        let wrapped_body_line = lines
            .iter()
            .position(|l| l.contains("Command::new(\"x\")"))
            .unwrap();
        assert!(
            is_test_line(&regions, wrapped_signature_line),
            "the wrapped item's own signature line must be covered"
        );
        assert!(
            is_test_line(&regions, wrapped_body_line),
            "a #[cfg(test)] item's body must be covered even when its \
             signature wraps across several lines before the opening brace"
        );

        let production_line = lines
            .iter()
            .position(|l| l.contains("fn production_after"))
            .unwrap();
        assert!(
            !is_test_line(&regions, production_line),
            "ordinary code after a #[cfg(test)] item must not be swept into its region"
        );
    }

    fn is_test_line(regions: &[(usize, usize)], line_no: usize) -> bool {
        regions.iter().any(|(s, e)| line_no >= *s && line_no <= *e)
    }

    /// Follow-up to ART-274. A `}` sitting inside a string literal must
    /// never be mistaken for the end of a real Rust block —
    /// `strip_string_literals` exists to prevent exactly that. Built
    /// directly, unlike the test above, so the fixture can place the
    /// flush-left `}` exactly where it used to do damage: at indent 0, the
    /// same indent the enclosing `#[cfg(test)] fn` itself sits at, so an
    /// unfixed guard ends the region on that line instead of the fn's own
    /// closing brace three lines later.
    #[test]
    fn a_flush_left_brace_inside_a_string_literal_does_not_end_the_region_early() {
        let src = "\
#[cfg(test)]
fn holds_a_flush_left_brace_in_a_string() {
    let evidence = \"before
}
after\";
    assert!(!evidence.is_empty());
}

fn production_after() -> u32 {
    0
}
";
        let lines: Vec<&str> = src.lines().collect();
        let regions = test_regions("flush-left-brace-in-string fixture", &lines);

        let assert_line = lines
            .iter()
            .position(|l| l.contains("assert!(!evidence.is_empty());"))
            .unwrap();
        assert!(
            is_test_line(&regions, assert_line),
            "the flush-left `}}` inside the string literal must not have ended \
             the region before the fn's own body finished"
        );

        let production_line = lines
            .iter()
            .position(|l| l.contains("fn production_after"))
            .unwrap();
        assert!(
            !is_test_line(&regions, production_line),
            "ordinary code after the #[cfg(test)] item must still not be swept in"
        );
    }

    /// The same shape, for a raw string (`r"…"` / `r#"…"#`) rather than a
    /// plain one — the other span [`strip_string_literals`] is asked to
    /// skip, and the one a WinUAE `.uae` template or an AmigaDOS script
    /// assembled with `format!` is more likely to actually use, since
    /// neither wants to escape every backslash in a Windows path or an
    /// AmigaDOS `;` comment.
    #[test]
    fn a_flush_left_brace_inside_a_raw_string_literal_does_not_end_the_region_early() {
        let src = "\
#[cfg(test)]
fn holds_a_flush_left_brace_in_a_raw_string() {
    let evidence = r#\"before
}
after\"#;
    assert!(!evidence.is_empty());
}

fn production_after() -> u32 {
    0
}
";
        let lines: Vec<&str> = src.lines().collect();
        let regions = test_regions("flush-left-brace-in-raw-string fixture", &lines);

        let assert_line = lines
            .iter()
            .position(|l| l.contains("assert!(!evidence.is_empty());"))
            .unwrap();
        assert!(
            is_test_line(&regions, assert_line),
            "the flush-left `}}` inside the raw string literal must not have \
             ended the region before the fn's own body finished"
        );

        let production_line = lines
            .iter()
            .position(|l| l.contains("fn production_after"))
            .unwrap();
        assert!(
            !is_test_line(&regions, production_line),
            "ordinary code after the #[cfg(test)] item must still not be swept in"
        );
    }

    /// Fix round 1 (batch-4 review, Important): the two tests above cover a
    /// plain and a raw string, but neither exercises the escape branch —
    /// `strip_string_literals`'s `Some('\\') => { .. }` arm, which must
    /// consume the backslash *and* the character right after it together so
    /// an escaped quote (`\"`) is never misread as the string's own closing
    /// quote. A string with an escaped `\"` followed by a flush-left `}`
    /// before its *real* closing quote is exactly the case that would catch
    /// a broken escape branch: if the escape were mishandled, the scanner
    /// would treat the `"` right after the `\` as the closer, leaving
    /// everything from there on — the flush-left `}` included — as ordinary
    /// unstripped text again, ending the region early the same way the two
    /// tests above prove a naive, un-stripped scan does.
    #[test]
    fn an_escaped_quote_before_a_flush_left_brace_does_not_end_the_region_early() {
        let src = "\
#[cfg(test)]
fn holds_an_escaped_quote_before_a_flush_left_brace() {
    let evidence = \"before \\\" middle
}
after\";
    assert!(!evidence.is_empty());
}

fn production_after() -> u32 {
    0
}
";
        let lines: Vec<&str> = src.lines().collect();
        // The fixture's own escaped quote must actually be there, or this
        // test could pass with no escape in play at all — the same
        // discipline `test_regions_covers_a_signature_that_wraps_across_lines`
        // exercises for its own fixture with the fixture-must-actually-
        // distinguish check that pattern already uses elsewhere in this
        // crate.
        assert!(
            lines.iter().any(|l| l.contains("before \\\" middle")),
            "the fixture must actually contain an escaped quote, or this test proves nothing"
        );

        let regions = test_regions("escaped-quote fixture", &lines);

        let assert_line = lines
            .iter()
            .position(|l| l.contains("assert!(!evidence.is_empty());"))
            .unwrap();
        assert!(
            is_test_line(&regions, assert_line),
            "the flush-left `}}` after an escaped quote must not have ended \
             the region before the fn's own body finished"
        );

        let production_line = lines
            .iter()
            .position(|l| l.contains("fn production_after"))
            .unwrap();
        assert!(
            !is_test_line(&regions, production_line),
            "ordinary code after the #[cfg(test)] item must still not be swept in"
        );
    }

    /// ART-274's own guard: production `core/` never spawns a process.
    ///
    /// Mutate by putting `std::process::Command::new(...)` back in
    /// `core/winuae.rs`, above its `#[cfg(test)] mod tests {`, and this
    /// should fail; restore it and this should pass again.
    #[test]
    fn core_never_spawns_a_process_outside_a_test() {
        let mut offenders = Vec::new();
        for path in core_files() {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("reading {}: {err}", path.display()));
            let lines: Vec<&str> = text.lines().collect();
            let regions = test_regions(&path.display().to_string(), &lines);
            for (n, line) in lines.iter().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                if line.contains("Command::new(") && !is_test_line(&regions, n) {
                    offenders.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "core/ spawned a process outside a #[cfg(test)] block — the trait rule \
             (CLAUDE.md, \"The core independence rule\"; ART-274) says the spawn belongs \
             in tools/, not here:\n{}",
            offenders.join("\n")
        );
    }

    /// I1 (2026-09-07 final review). `strip_string_literals` used to track
    /// only string literals, so a stray, unpaired `"` inside a `//` comment
    /// or a `'"'` char literal opened a phantom span that could blank a
    /// file's own `#[cfg(test)]` attribute line — leaving `test_regions`
    /// with zero regions for a file that plainly has a test module. That is
    /// the safe direction (the whole file then reads as production, so
    /// nothing escapes as a false negative) but it means
    /// [`core_never_spawns_a_process_outside_a_test`] cannot actually read
    /// two of the 204 files it claims to check, and the mirror mistake — a
    /// blanked closing `}` extending a region *over* real production code —
    /// is the genuine fail-open this test exists to close off.
    ///
    /// Mutate by reverting `strip_string_literals` to track only plain and
    /// raw string literals (drop the `//`, `/* */` and char-literal
    /// branches): this fails, naming
    /// `src/core/amigainstall/finish.rs` and `src/core/gameindex/cleanup.rs`
    /// — both have a `#[cfg(test)]` attribute and, with the old stripper, a
    /// module-doc `"` (`finish.rs` line 28: `` `refuse_shell_metacharacters`
    /// refuses `"` *because a quote ``) or a char literal that opens an
    /// unclosed span reaching past it. Restore the comment/char-literal
    /// handling and this passes again.
    #[test]
    fn every_file_with_a_cfg_test_attribute_yields_at_least_one_region() {
        let mut blind = Vec::new();
        for path in core_files() {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("reading {}: {err}", path.display()));
            if !text.lines().any(|l| l.trim() == "#[cfg(test)]") {
                continue;
            }
            let lines: Vec<&str> = text.lines().collect();
            let regions = test_regions(&path.display().to_string(), &lines);
            if regions.is_empty() {
                blind.push(path.display().to_string());
            }
        }
        assert!(
            blind.is_empty(),
            "these core/ files carry a #[cfg(test)] attribute but strip_string_literals \
             found zero regions in them — a stray quote inside a comment or a char \
             literal opened a span that blanked the attribute itself, so the guard \
             cannot read these files at all: {}",
            blind.join(", ")
        );
    }
}
