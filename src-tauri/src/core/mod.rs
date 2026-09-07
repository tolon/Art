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
}

#[cfg(test)]
impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
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

    /// The `(start, end)` line ranges (0-based, inclusive) a `#[cfg(test)]`
    /// covers — the attribute line itself through to the `}` that closes the
    /// item it is attached to, or through the item's own line when it has no
    /// block body (`#[cfg(test)] mod x;`).
    ///
    /// Matching on **indent** rather than counting braces is deliberate: a
    /// WinUAE `.uae` line or an AmigaDOS script assembled with `format!`
    /// carries plenty of literal `{`/`}` inside string data (see
    /// `tools::winuae_launcher`'s own `real_version_hook`), and a brace
    /// counter would misread those. `cargo fmt --check` is blocking in CI,
    /// so a block's closing brace is always aligned with the line that opened
    /// it — indent is the reliable signal this codebase's own formatting
    /// already guarantees.
    fn test_regions(lines: &[&str]) -> Vec<(usize, usize)> {
        let mut regions = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            if lines[i].trim() == "#[cfg(test)]" {
                let indent = indent_of(lines[i]);
                let start = i;
                let mut j = i + 1;
                // Stacked attributes (`#[cfg(test)]` then `#[test]`, say) sit
                // at the same indent as the item they both apply to.
                while j < lines.len()
                    && indent_of(lines[j]) == indent
                    && lines[j].trim_start().starts_with('#')
                {
                    j += 1;
                }
                let opens_block = lines.get(j).is_some_and(|l| l.trim_end().ends_with('{'));
                let end = if opens_block {
                    let mut k = j + 1;
                    while k < lines.len()
                        && !(indent_of(lines[k]) == indent && lines[k].trim() == "}")
                    {
                        k += 1;
                    }
                    k.min(lines.len().saturating_sub(1))
                } else {
                    j.min(lines.len().saturating_sub(1))
                };
                regions.push((start, end));
                i = end + 1;
            } else {
                i += 1;
            }
        }
        regions
    }

    fn is_test_line(regions: &[(usize, usize)], line_no: usize) -> bool {
        regions.iter().any(|(s, e)| line_no >= *s && line_no <= *e)
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
            let regions = test_regions(&lines);
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
}
