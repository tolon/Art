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
pub mod cardos;
pub mod cbm;
pub mod clock;
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
pub mod rdbedit;
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
        //
        // `writeln!` and not `eprintln!`, whose macro panics if the write to
        // stderr fails — which would be exactly the double-abort the line
        // above rejects. A closed stderr is theoretical; a `Drop` that can
        // panic is not worth keeping for a shorter line.
        if let Err(err) = std::fs::remove_dir_all(&self.0) {
            if self.0.exists() {
                use std::io::Write as _;
                let _ = writeln!(
                    std::io::stderr(),
                    "ScratchDir: {} not removed: {err}",
                    self.0.display()
                );
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

    /// ART-317: the UTC offset is a platform question, answered by
    /// `tools/local_time.rs` through `core::clock::AmigaClock`. A time-zone
    /// crate named inside `core/` would put the answer back where it cannot
    /// be tested with a fixed clock.
    ///
    /// Mutate by adding `use chrono::Local;` to `core/clock.rs`; this fails.
    #[test]
    fn core_never_names_a_time_zone_crate() {
        // Built with `concat!` so this file's own source does not match.
        let needles = [
            concat!("chro", "no::"),
            concat!("use chro", "no"),
            concat!("iana_time", "_zone"),
        ];
        let mut offenders = Vec::new();
        for path in core_files() {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("reading {}: {err}", path.display()));
            for (n, line) in text.lines().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                if needles.iter().any(|needle| line.contains(needle)) {
                    offenders.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "core/ named a time-zone crate — the offset belongs in tools/local_time.rs (ART-317):\n{}",
            offenders.join("\n")
        );
    }

    /// ART-317: an Amiga date is made from an instant only through
    /// `core::clock::AmigaClock`. Production code naming the epoch constant is
    /// doing that arithmetic by hand, which is how every writer came to stamp
    /// UTC. `bcpl.rs` defines it and `clock.rs` is the one converter.
    ///
    /// Mutate by pasting create.rs's old `get_current_amiga_date` above its
    /// `#[cfg(test)]`; this fails.
    #[test]
    fn core_makes_amiga_dates_only_through_the_clock() {
        let needle = concat!("AMIGA_EPOCH", "_UNIX");
        let allowed = ["adf/bcpl.rs", "clock.rs"];
        let mut offenders = Vec::new();
        for path in core_files() {
            let shown = path.display().to_string().replace('\\', "/");
            if allowed.iter().any(|ok| shown.ends_with(ok)) {
                continue;
            }
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("reading {}: {err}", path.display()));
            let lines: Vec<&str> = text.lines().collect();
            let regions = test_regions(&path.display().to_string(), &lines);
            for (n, line) in lines.iter().enumerate() {
                if line.trim_start().starts_with("//") || is_test_line(&regions, n) {
                    continue;
                }
                if line.contains(needle) {
                    offenders.push(format!("{shown}:{}: {}", n + 1, line.trim()));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "an Amiga date made outside core::clock (ART-317):\n{}",
            offenders.join("\n")
        );
    }

    /// Every `.rs` file under `commands/` and `tools/`, recursively — the
    /// wiring layer [`commands_and_tools_never_name_utc_clock_outside_a_test`]
    /// reads. Separate from [`core_files`] because `UtcClock` is legitimate
    /// inside `core/` itself (see that test's own doc comment).
    fn command_and_tool_files() -> Vec<PathBuf> {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        for sub in ["commands", "tools"] {
            let mut stack = vec![base.join(sub)];
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
        }
        files
    }

    /// Every `.rs` file under `src/`, recursively — wider than [`core_files`]
    /// (`core/` only) because [`only_clock_and_local_time_name_amiga_from_wall_or_system_now_unix`]
    /// must also see `tools/local_time.rs`, the one legitimate caller outside
    /// `core/clock.rs` itself.
    fn all_src_files() -> Vec<PathBuf> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
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

    /// ART-317's third debt round, survivor (b) (docs/ISSUES.md). Both
    /// `amiga_from_wall` and `system_now_unix` are `pub`, and `clock.rs` is on
    /// [`core_makes_amiga_dates_only_through_the_clock`]'s own allow-list — so
    /// nothing stopped a product site from calling
    /// `amiga_from_wall(system_now_unix())` directly, which makes a UTC Amiga
    /// date with no offset at all, bypassing `AmigaClock::offset_at`
    /// entirely, invisibly to every guard ART-317 shipped. Only
    /// `core/clock.rs` (where both are declared, and where
    /// `AmigaClock::amiga_from_unix`'s default body legitimately calls
    /// `amiga_from_wall`) and `tools/local_time.rs` (the product clock,
    /// which calls `system_now_unix()` inside `now_unix()`) may name either
    /// outside a test. This walks all of `src/`, not just `core/`, since
    /// `tools/local_time.rs` must be checked too.
    ///
    /// Mutate by adding a line naming `amiga_from_wall` or `system_now_unix`
    /// to a real product file outside the two allowed ones, above its
    /// `#[cfg(test)]`; this fails.
    #[test]
    fn only_clock_and_local_time_name_amiga_from_wall_or_system_now_unix() {
        let needles = [
            concat!("amiga_from", "_wall"),
            concat!("system_now", "_unix"),
        ];
        let allowed = ["core/clock.rs", "tools/local_time.rs"];
        let mut offenders = Vec::new();
        for path in all_src_files() {
            let shown = path.display().to_string().replace('\\', "/");
            if allowed.iter().any(|ok| shown.ends_with(ok)) {
                continue;
            }
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("reading {}: {err}", path.display()));
            let lines: Vec<&str> = text.lines().collect();
            let regions = test_regions(&path.display().to_string(), &lines);
            for (n, line) in lines.iter().enumerate() {
                if line.trim_start().starts_with("//") || is_test_line(&regions, n) {
                    continue;
                }
                if needles.iter().any(|needle| line.contains(needle)) {
                    offenders.push(format!("{shown}:{}: {}", n + 1, line.trim()));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "amiga_from_wall or system_now_unix named outside core::clock and \
             tools::local_time — a date made this way carries no UTC offset at all \
             (ART-317, third debt round survivor (b)):\n{}",
            offenders.join("\n")
        );
    }

    /// `line` with every run of whitespace dropped, except one space kept
    /// between two identifier characters — `libpfs3 :: writer :: { Writer as
    /// W }` becomes `libpfs3::writer::{Writer as W}` — so a `use` tree and a
    /// call read the same however they are spaced.
    fn compact_rust(line: &str) -> String {
        let ident = |c: char| c.is_alphanumeric() || c == '_';
        let mut out = String::new();
        let mut pending_space = false;
        for c in line.chars() {
            if c.is_whitespace() {
                pending_space = true;
                continue;
            }
            if pending_space && out.chars().last().is_some_and(ident) && ident(c) {
                out.push(' ');
            }
            pending_space = false;
            out.push(c);
        }
        out
    }

    /// Every leaf of a compacted `use` tree as `(full path, the name it
    /// binds)`: `libpfs3::{writer::{self as w, Writer}}` gives
    /// `("libpfs3::writer", "w")` and `("libpfs3::writer::Writer", "Writer")`;
    /// a glob binds `*`.
    fn use_tree_leaves(prefix: &str, tree: &str, out: &mut Vec<(String, String)>) {
        if let Some(open) = tree.find('{') {
            let base = format!("{prefix}{}", &tree[..open]);
            let close = tree.rfind('}').unwrap_or(tree.len()).max(open + 1);
            let inner = &tree[open + 1..close];
            let (mut depth, mut start) = (0usize, 0usize);
            for (i, c) in inner.char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => depth = depth.saturating_sub(1),
                    ',' if depth == 0 => {
                        use_tree_leaves(&base, &inner[start..i], out);
                        start = i + 1;
                    }
                    _ => {}
                }
            }
            use_tree_leaves(&base, &inner[start..], out);
            return;
        }
        if tree.is_empty() {
            return;
        }
        let (path, alias) = match tree.split_once(" as ") {
            Some((path, alias)) => (path, Some(alias)),
            None => (tree, None),
        };
        let mut full = format!("{prefix}{path}");
        if let Some(parent) = full.strip_suffix("::self") {
            full = parent.to_string();
        }
        let last = full.rsplit("::").next().unwrap_or(&full).to_string();
        out.push((full, alias.map(str::to_string).unwrap_or(last)));
    }

    /// Final review fix wave, I2. The lines of `text` outside a test and a
    /// comment that call libpfs3's `Writer::open`, **however `Writer` is
    /// named there**: the full path; a `use` of `libpfs3`, of
    /// `libpfs3::writer` or of `libpfs3::writer::Writer` itself — plain,
    /// `as`-aliased, grouped in braces or a glob, `pub` or not, wrapped
    /// across lines; and a `type` alias of any of those. A `use` or `type`
    /// inside a test region binds nothing here, since a call there is
    /// skipped anyway. Returned as `(0-based line, trimmed line)`.
    ///
    /// Scoped re-review item 2: also a glob of either (pinned in the fixture
    /// now, not only resolved); the qualified-path form
    /// `<libpfs3::writer::Writer>::open(` and `<Alias>::open(`; and an
    /// `extern crate libpfs3 as x;` alias, used as a path or as the root of a
    /// `use`. It reads code only: a block comment (nested or not), a string,
    /// a raw string or a char literal is blanked first by
    /// `strip_string_literals`, which keeps every newline where it was, so
    /// neither holds a call nor binds a name.
    ///
    /// Follow-up 4 (2026-09-15): a `use` tree whose root is a brace group —
    /// `use {libpfs3::writer::Writer};`, nested groups, and through an
    /// `extern crate` alias — is resolved leaf by leaf (pinned in the
    /// fixture).
    ///
    /// **What it does not see:** a `Writer` reached through a re-export in
    /// another module (`pub use libpfs3::writer::Writer;` in one file, then
    /// `crate::that::Writer::open(` in another), or through an `extern crate`
    /// alias made in another file; a call built by a macro; a call split
    /// across lines inside `Writer::open(`; a trait-qualified
    /// `<libpfs3::writer::Writer as T>::open(`, which only a trait of ART's
    /// own with an `open` method could make compile; and a `use` that does
    /// not start its line (`let x = 1; use libpfs3::writer::Writer;`) or whose
    /// keyword stands on a line of its own — both seen unflagged by a
    /// temporary fixture on 2026-09-15 (follow-up 4, E4).
    fn libpfs3_writer_open_sites(label: &str, text: &str) -> Vec<(usize, String)> {
        let lines: Vec<&str> = text.lines().collect();
        let regions = test_regions(label, &lines);
        let stripped = strip_string_literals(text);
        let code: Vec<&str> = stripped.lines().collect();
        assert_eq!(
            code.len(),
            lines.len(),
            "{label}: stripping comments and strings moved a line"
        );
        let live = |n: usize| !is_test_line(&regions, n);
        let root = concat!("lib", "pfs3");
        let writer_mod = format!("{root}::writer");
        let writer_type = format!("{writer_mod}::Writer");
        let mut needles = vec![format!("{writer_type}::open(")];
        // Paths that name the `Writer` type itself, for `type` aliases.
        let mut type_names = vec![writer_type.clone()];
        // `extern crate libpfs3 as x;` makes `x` another name for the crate.
        let mut roots = vec![root.to_string()];
        let extern_alias = format!("extern crate {root} as ");
        for (n, line) in code.iter().enumerate() {
            if !live(n) {
                continue;
            }
            let compact = compact_rust(line);
            let unpub = compact.strip_prefix("pub ").unwrap_or(&compact);
            if let Some(alias) = unpub.strip_prefix(extern_alias.as_str()) {
                let alias = alias.trim_end_matches(';');
                if !alias.is_empty() && alias != "_" {
                    needles.push(format!("{alias}::writer::Writer::open("));
                    type_names.push(format!("{alias}::writer::Writer"));
                    roots.push(alias.to_string());
                }
            }
        }
        let mut n = 0;
        while n < code.len() {
            let start = n;
            n += 1;
            if !live(start) {
                continue;
            }
            let head = compact_rust(code[start]);
            // Follow-up 4 (2026-09-15): `compact_rust` keeps no space before a
            // `{` or a `::`, so `use {libpfs3::…}` reads `use{libpfs3::…}` and
            // `use ::{…}` reads `use::{…}`; all three spellings start a `use`.
            let Some(at) = ["use ", "use{", "use::"]
                .iter()
                .filter_map(|keyword| head.find(keyword))
                .min()
            else {
                continue;
            };
            let before = &head[..at];
            if !(before.is_empty() || before.starts_with("pub")) {
                continue;
            }
            let mut stmt = head[at + 3..].trim_start().to_string();
            while !stmt.contains(';') && n < code.len() {
                stmt.push_str(&compact_rust(code[n]));
                n += 1;
            }
            let stmt = stmt.trim_start_matches("::");
            let stmt = stmt.split(';').next().unwrap_or("");
            let mut leaves = Vec::new();
            use_tree_leaves("", stmt, &mut leaves);
            for (path, name) in leaves {
                // Each leaf rooted at the crate or at an `extern crate` alias
                // of it, and spelled from the crate's own name either way.
                // Follow-up 4 (2026-09-15): per leaf, not per statement, so a
                // tree whose root is a brace group — `use {libpfs3::…};`,
                // nested or not — is resolved too. It used to require the
                // statement itself to start with the crate's name.
                let path = path.trim_start_matches("::");
                let Some(path) = roots.iter().find_map(|r| {
                    let rest = path.strip_prefix(r.as_str())?;
                    (rest.is_empty() || rest.starts_with("::")).then(|| format!("{root}{rest}"))
                }) else {
                    continue;
                };
                if name == "_" {
                    continue;
                }
                if path == writer_type {
                    needles.push(format!("{name}::open("));
                    type_names.push(name);
                } else if path == writer_mod {
                    needles.push(format!("{name}::Writer::open("));
                    type_names.push(format!("{name}::Writer"));
                } else if path == format!("{writer_mod}::*") {
                    needles.push("Writer::open(".to_string());
                    type_names.push("Writer".to_string());
                } else if path == root {
                    needles.push(format!("{name}::writer::Writer::open("));
                    type_names.push(format!("{name}::writer::Writer"));
                } else if path == format!("{root}::*") {
                    needles.push("writer::Writer::open(".to_string());
                    type_names.push("writer::Writer".to_string());
                }
            }
        }
        let mut aliases = Vec::new();
        for (n, line) in code.iter().enumerate() {
            if !live(n) {
                continue;
            }
            let compact = compact_rust(line);
            let unpub = compact.strip_prefix("pub ").unwrap_or(&compact);
            let Some(rest) = unpub.strip_prefix("type ") else {
                continue;
            };
            if let Some((alias, rhs)) = rest.split_once('=') {
                let rhs = rhs.trim_end_matches(';').trim_start_matches("::");
                if type_names.iter().any(|t| rhs == t) {
                    aliases.push(alias.to_string());
                }
            }
        }
        // `<T>::open(` names the type as `T::open(` does.
        for t in type_names.iter().chain(&aliases) {
            needles.push(format!("<{t}>::open("));
        }
        needles.extend(aliases.iter().map(|alias| format!("{alias}::open(")));
        let ident = |c: char| c.is_alphanumeric() || c == '_';
        let mut sites = Vec::new();
        for (n, line) in code.iter().enumerate() {
            if !live(n) {
                continue;
            }
            let compact = compact_rust(line).replace("<::", "<");
            let hit = needles.iter().any(|needle| {
                compact
                    .match_indices(needle.as_str())
                    .any(|(at, _)| !compact[..at].chars().last().is_some_and(ident))
            });
            if hit {
                sites.push((n, lines[n].trim().to_string()));
            }
        }
        sites
    }

    /// The fixture for [`libpfs3_writer_open_sites`]: every naming the guard
    /// claims to see is flagged, and the controls — a different type's
    /// `open`, a comment, a test, and a `Writer` that is not libpfs3's — are
    /// not. Re-runnable, where the guard below was first proven only by
    /// one-off injections.
    #[test]
    fn libpfs3_writer_open_sites_sees_every_naming_of_the_writer() {
        let src = "\
    use libpfs3::writer::Writer;
    pub use libpfs3::writer::Writer as PfsWriter;
    use libpfs3::{
        volume::Volume,
        writer::{self as w, Writer as Grouped},
    };
    use libpfs3 as pfs;
    type Alias = libpfs3::writer::Writer;

    fn product(v: Volume) {
        let _ = Writer::open(v);
        let _ = PfsWriter :: open(v);
        let _ = w::Writer::open(v);
        let _ = Grouped::open(v);
        let _ = pfs::writer::Writer::open(v);
        let _ = Alias::open(v);
        let _ = libpfs3::writer::Writer::open(v);
        let _ = VolumeWriter::open(v);
        // Writer::open(v) in a comment
    }

    #[cfg(test)]
    mod tests {
        fn t(v: Volume) {
            let _ = Writer::open(v);
        }
    }
";
        let flagged: Vec<String> = libpfs3_writer_open_sites("naming fixture", src)
            .into_iter()
            .map(|(_, line)| line)
            .collect();
        assert_eq!(
            flagged,
            [
                "let _ = Writer::open(v);",
                "let _ = PfsWriter :: open(v);",
                "let _ = w::Writer::open(v);",
                "let _ = Grouped::open(v);",
                "let _ = pfs::writer::Writer::open(v);",
                "let _ = Alias::open(v);",
                "let _ = libpfs3::writer::Writer::open(v);",
            ]
        );
        let unrelated = "\
    use crate::core::volume::write::Writer;

    fn product() {
        let _ = Writer::open();
    }
";
        assert!(libpfs3_writer_open_sites("unrelated fixture", unrelated).is_empty());

        // Scoped re-review item 2: the glob forms, the qualified-path form,
        // an `extern crate` alias, and a call that only looks like one because
        // it sits in a block comment or a string. Each case is its own source.
        let cases: [(&str, &str, &[&str]); 13] = [
            // Follow-up 4 (2026-09-15): a `use` tree whose root is a brace
            // group, plain, nested, and through an `extern crate` alias.
            (
                "a brace-rooted use",
                "\
    use {libpfs3::writer::Writer};
    fn f(v: V) {
        let _ = Writer::open(v);
    }
",
                &["let _ = Writer::open(v);"],
            ),
            (
                "a nested brace-rooted use",
                "\
    use {{libpfs3::{writer::{Writer as W}}}, std::io};
    fn f(v: V) {
        let _ = W::open(v);
    }
",
                &["let _ = W::open(v);"],
            ),
            (
                "a brace-rooted use through an extern crate alias",
                "\
    extern crate libpfs3 as x;
    pub use ::{x::{writer}};
    fn f(v: V) {
        let _ = writer::Writer::open(v);
    }
",
                &["let _ = writer::Writer::open(v);"],
            ),
            (
                "a glob of libpfs3::writer",
                "\
    use libpfs3::writer::*;
    fn f(v: V) {
        let _ = Writer::open(v);
    }
",
                &["let _ = Writer::open(v);"],
            ),
            (
                "a glob of libpfs3",
                "\
    use libpfs3::*;
    fn f(v: V) {
        let _ = writer::Writer::open(v);
    }
",
                &["let _ = writer::Writer::open(v);"],
            ),
            (
                "a qualified path",
                "\
    fn f(v: V) {
        let _ = <libpfs3::writer::Writer>::open(v);
    }
",
                &["let _ = <libpfs3::writer::Writer>::open(v);"],
            ),
            (
                "a qualified path through a use alias",
                "\
    use libpfs3::writer::Writer as W;
    fn f(v: V) {
        let _ = <W>::open(v);
    }
",
                &["let _ = <W>::open(v);"],
            ),
            (
                "an extern crate alias",
                "\
    extern crate libpfs3 as x;
    fn f(v: V) {
        let _ = x::writer::Writer::open(v);
    }
",
                &["let _ = x::writer::Writer::open(v);"],
            ),
            (
                "a use through an extern crate alias",
                "\
    extern crate libpfs3 as x;
    use x::writer::Writer;
    fn f(v: V) {
        let _ = Writer::open(v);
    }
",
                &["let _ = Writer::open(v);"],
            ),
            (
                "block comments",
                "\
    use libpfs3::writer::Writer;
    fn f(v: V) {
        /* let _ = Writer::open(v); */
        /*
         let _ = Writer::open(v);
         /* nested */ let _ = Writer::open(v);
        */
    }
",
                &[],
            ),
            (
                "string literals",
                "\
    use libpfs3::writer::Writer;
    fn f(v: V) {
        let _ = \"Writer::open(v)\";
        let _ = r#\"Writer::open(v)\"#;
    }
",
                &[],
            ),
            (
                "a call after a string holding a comment opener",
                "\
    use libpfs3::writer::Writer;
    fn f(v: V) {
        let s = \"/*\"; let _ = Writer::open(v);
    }
",
                &["let s = \"/*\"; let _ = Writer::open(v);"],
            ),
            (
                "a use inside a block comment",
                "\
    /* use libpfs3::writer::Writer; */
    fn f(v: V) {
        let _ = Writer::open(v);
    }
",
                &[],
            ),
        ];
        let mut wrong = Vec::new();
        for (what, src, expected) in cases {
            let flagged: Vec<String> = libpfs3_writer_open_sites(what, src)
                .into_iter()
                .map(|(_, line)| line)
                .collect();
            if flagged != expected {
                wrong.push(format!(
                    "{what}: flagged {flagged:?}, expected {expected:?}"
                ));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    /// `(first, last)` 0-based lines of the top-level item whose opening line
    /// contains `signature`, through its closing `}` at column 0.
    fn top_level_item_lines(text: &str, signature: &str) -> Option<(usize, usize)> {
        let lines: Vec<&str> = text.lines().collect();
        let first = lines
            .iter()
            .position(|l| !l.starts_with(' ') && l.contains(signature))?;
        let last = (first..lines.len()).find(|&n| lines[n].trim_end() == "}")?;
        Some((first, last))
    }

    /// ART-317's third debt round, survivor (c) (docs/ISSUES.md). A libpfs3
    /// `Writer` opened without calling `set_entry_date` falls back to
    /// `current_amiga_datestamp` (`vendor/libpfs3/src/writer.rs`'s
    /// `entry_datestamp`), which stamps UTC — the same defect ART-317 closed
    /// everywhere else, reopened by any fresh `Writer::open` call outside the
    /// one place that stamps it. `core::preload::native::open_pfs3_writer` is
    /// that place: it opens the writer and calls `set_entry_date` with the
    /// clock's own date before handing the writer back.
    ///
    /// **Final review fix wave, I2.** This walks all of `src/`, and a call is
    /// found however `Writer` is named ([`libpfs3_writer_open_sites`] says
    /// which namings it resolves and which it does not). The allow-list is
    /// the one call inside `open_pfs3_writer`'s own body, not the file it
    /// lives in, and that body must hold exactly one — so a second call beside
    /// the helper fails, and so does renaming the helper. It used to look for
    /// the fully qualified path only, in `core/` only, allowing all of
    /// `native.rs` and a `core/card/sizing.rs` whose one call was already
    /// inside `#[cfg(test)]`; `use libpfs3::writer::Writer;` followed by
    /// `Writer::open(` passed it.
    ///
    /// Mutate by adding `use libpfs3::writer::Writer;` and a function calling
    /// `Writer::open(vol)` above a product file's `#[cfg(test)]`, or the same
    /// with an `as` alias, or a second call in `native.rs` outside the
    /// helper; each fails.
    #[test]
    fn libpfs3_writer_open_is_named_only_by_the_pfs3_writer_helper() {
        let mut offenders = Vec::new();
        let mut helper_calls = 0;
        for path in all_src_files() {
            let shown = path.display().to_string().replace('\\', "/");
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("reading {}: {err}", path.display()));
            let helper = if shown.ends_with("core/preload/native.rs") {
                top_level_item_lines(&text, "fn open_pfs3_writer(")
            } else {
                None
            };
            for (n, line) in libpfs3_writer_open_sites(&shown, &text) {
                if helper.is_some_and(|(first, last)| (first..=last).contains(&n)) {
                    helper_calls += 1;
                } else {
                    offenders.push(format!("{shown}:{}: {line}", n + 1));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "libpfs3's Writer::open named outside \
             core::preload::native::open_pfs3_writer, which is the only place that stamps \
             the clock's date on a fresh writer (ART-317, third debt round survivor (c)):\n{}",
            offenders.join("\n")
        );
        assert_eq!(
            helper_calls, 1,
            "core::preload::native::open_pfs3_writer must hold exactly one libpfs3 \
             Writer::open call — the allow-list is that call, not the file"
        );
    }

    /// ART-317, final review I1's survivor (a). Nothing stopped a
    /// `commands/` or `tools/` site from passing `&crate::core::clock::UtcClock`
    /// in place of `&crate::tools::local_time::LOCAL_TIME` — that makes a UTC
    /// Amiga date on every machine, not only a UTC CI runner, and no test,
    /// clippy lint or guard caught it. This walks `commands/` and `tools/`
    /// rather than `core/`: `UtcClock` is `pub` and legitimately named inside
    /// `core/` at three sites that read only a directory listing's names or
    /// counts and never a date shown to the user —
    /// `core/adf/fs.rs::list_files` (`list_directory_on`, line ~130),
    /// `core/dirsize.rs` (`list_directory_on`, line ~175) and
    /// `core/gameindex/readers/whdhdf.rs` (icon/drawer lookups, lines ~196
    /// and ~285). None of those three files are under `commands/` or
    /// `tools/`, so they need no allow-list entry here; grepping
    /// `commands/**` and `tools/**` for `UtcClock` (2026-09-14) found no
    /// site at all, product or test.
    ///
    /// Mutate by passing `&crate::core::clock::UtcClock` instead of
    /// `&crate::tools::local_time::LOCAL_TIME` at a real `commands/` site
    /// (for example `commands/adf.rs`'s `list_root`/`list_dir` calls); this
    /// fails.
    #[test]
    fn commands_and_tools_never_name_utc_clock_outside_a_test() {
        let needle = concat!("Utc", "Clock");
        let mut offenders = Vec::new();
        for path in command_and_tool_files() {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("reading {}: {err}", path.display()));
            let lines: Vec<&str> = text.lines().collect();
            let regions = test_regions(&path.display().to_string(), &lines);
            for (n, line) in lines.iter().enumerate() {
                if line.trim_start().starts_with("//") || is_test_line(&regions, n) {
                    continue;
                }
                if line.contains(needle) {
                    offenders.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "commands/ or tools/ named UtcClock outside a test — a date reaching a \
             command must go through tools::local_time::LOCAL_TIME, never a fixed \
             UTC offset (ART-317, final review I1):\n{}",
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

    /// ART-339: `core/card` must not import `core/preload` — the two used to
    /// import each other, against CLAUDE.md's inward-layering rule — and now
    /// that `core/cardos` sits above both (`core/cardos/mod.rs`'s own module
    /// doc), nothing below `core/cardos` may import it either, or the new
    /// module becomes exactly the layering problem it was created to close.
    ///
    /// Mutate by adding a line naming `crate::core::preload` to a product file
    /// under `core/card/`, or a line naming `crate::core::cardos` to a product
    /// file outside `core/cardos/`; each fails.
    #[test]
    fn core_card_does_not_import_preload_and_nothing_below_imports_cardos() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/core");
        let mut offenders = Vec::new();
        for path in core_files() {
            let rel = path
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("reading {}: {err}", path.display()));
            let lines: Vec<&str> = text.lines().collect();
            let regions = test_regions(&rel, &lines);
            for (n, line) in lines.iter().enumerate() {
                if line.trim_start().starts_with("//") || is_test_line(&regions, n) {
                    continue;
                }
                let card_imports_preload =
                    rel.starts_with("card/") && line.contains("crate::core::preload");
                let below_imports_cardos =
                    !rel.starts_with("cardos/") && line.contains("crate::core::cardos");
                if card_imports_preload || below_imports_cardos {
                    offenders.push(format!("{rel}:{}: {}", n + 1, line.trim()));
                }
            }
        }
        assert!(offenders.is_empty(), "layering (ART-339): {offenders:#?}");
    }
}
