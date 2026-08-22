//! Where ART stages work it is going to throw away — one root, chosen once,
//! and every staging site in the product goes through it (ART-196).
//!
//! # Why this module exists at all
//!
//! Everything ART staged used to go through [`std::env::temp_dir`], which on
//! Windows is `%TEMP%` on the **system drive**. Preview extractions, install
//! staging roots, unpacked packages, the emulator's own launch configuration:
//! all of it landed on `C:` whatever the user would have preferred, and the
//! product offered no way to say otherwise.
//!
//! That is not hypothetical here. ART's *test* scratch once put 169,291
//! directories and roughly 987 GB into `%TEMP%` and filled a 2 TB system
//! drive (ART-184). The tests were redirected off the system drive with a
//! machine-local `.cargo/config.toml`; the shipped product could not be.
//!
//! # The rules this module keeps
//!
//! - **The default is [`std::env::temp_dir`]**, so nothing changes for
//!   somebody who never opens the setting. The owner's own ruling: the
//!   question is asked once, up front, and pressing Next gives today's
//!   behaviour.
//! - **A chosen root that is not usable is a refusal, never a fallback.**
//!   Once a user has said "not `C:`", silently staging on `C:` because their
//!   drive is not plugged in would be exactly the class of defect this
//!   project is most expensive for: a confident, wrong action nobody is told
//!   about. [`root`] returns [`AppError::ScratchUnavailable`] instead, which
//!   names the folder and what to do about it.
//! - **Nothing is moved and nothing is deleted.** Repointing the root leaves
//!   whatever is in the old one exactly where it is; the screen says where
//!   that is. Moving gigabytes silently while a user waits on a Settings
//!   screen is the wrong shape, and ART does not delete what it did not put
//!   there this run.
//!
//! # Why it is not in `core/`
//!
//! Where a platform keeps scratch files is not a question `core/` gets to
//! answer — the same rule `core::osinstall::plan_with_cache` already states
//! for its own cache directory, and `core::artwork::cache` for its own. A
//! `core/` function that needs somewhere to stage **takes the directory**;
//! this module is what the command layer hands it.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::error::{AppError, AppResult};

/// The root the user chose, or `None` while they have not chosen one.
///
/// A `RwLock` rather than a `OnceLock`: the setting is changeable at any time
/// afterwards, which is the other half of the owner's ruling — somebody who
/// pressed Next without reading, or who later adds a disk, must not have to
/// reinstall ART to move its scratch.
static CHOSEN: RwLock<Option<PathBuf>> = RwLock::new(None);

/// The folder ART will stage into, or a refusal naming the one it cannot use.
///
/// `Ok(std::env::temp_dir())` whenever no root has been chosen — that path is
/// the platform's own answer and is not probed, because refusing to work
/// because `%TEMP%` is odd would help nobody and is not what this issue is
/// about.
pub fn root() -> AppResult<PathBuf> {
    let chosen = CHOSEN
        .read()
        .expect("the scratch root lock is never held across a panic")
        .clone();
    match chosen {
        None => Ok(std::env::temp_dir()),
        Some(path) => match usable(&path) {
            Ok(()) => Ok(path),
            Err(why) => Err(AppError::ScratchUnavailable { root: path, why }),
        },
    }
}

/// The root as the user set it, or `None` when they are on the default.
///
/// Kept separate from [`root`] so a screen can tell "the user accepted the
/// default" from "the user chose this folder and it happens to be `%TEMP%`" —
/// two different sentences, and only the first should offer to explain the
/// default.
pub fn chosen() -> Option<PathBuf> {
    CHOSEN
        .read()
        .expect("the scratch root lock is never held across a panic")
        .clone()
}

/// Take `path` as the scratch root for this process, or refuse it.
///
/// `None` returns to the default. A folder that cannot be written to is
/// refused **here**, on the Settings screen where the user can still do
/// something about it, rather than hours later inside a job — and the
/// previous root stays in force, so a bad choice never leaves ART with
/// nowhere to stage.
pub fn set(path: Option<&Path>) -> AppResult<PathBuf> {
    match path {
        None => {
            *CHOSEN
                .write()
                .expect("the scratch root lock is never held across a panic") = None;
            Ok(std::env::temp_dir())
        }
        Some(path) => {
            usable(path).map_err(|why| AppError::ScratchUnavailable {
                root: path.to_path_buf(),
                why,
            })?;
            let path = path.to_path_buf();
            *CHOSEN
                .write()
                .expect("the scratch root lock is never held across a panic") = Some(path.clone());
            Ok(path)
        }
    }
}

/// Can ART actually stage here — asked by writing, not by looking.
///
/// `is_dir()` is not the question. A folder on a full disk, a read-only
/// share, or a path the user has no permission for all answer `true` to it
/// and then fail on the first real byte, which is the wrong moment and the
/// wrong screen. The probe file is created exclusively and removed
/// immediately; a leftover means the removal failed, not that the check did.
fn usable(path: &Path) -> Result<(), String> {
    if !path.is_dir() {
        return Err(if path.exists() {
            "it is a file, not a folder".to_string()
        } else {
            "the folder is not there".to_string()
        });
    }

    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let probe = path.join(format!(
        ".art-scratch-probe-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            Ok(())
        }
        Err(err) => Err(format!("ART cannot write there ({err})")),
    }
}

/// Reset to the default. Tests only — the product changes the root through
/// [`set`], and a test that left a chosen root behind would decide where the
/// *next* test stages.
#[cfg(test)]
fn reset_for_test() {
    *CHOSEN
        .write()
        .expect("the scratch root lock is never held across a panic") = None;
}

/// Run `body` with the process-wide root to itself, reset on both sides.
///
/// **One lock for the whole crate**, not one per test module. `CHOSEN` is a
/// single static; `cargo test` runs the whole binary in one process on many
/// threads, and a second `Mutex` in a second module guards nothing that the
/// first one is guarding — which is exactly how three tests here failed
/// against each other the first time this was written. Same class as
/// ART-182: a test that passes or fails on thread scheduling is not a test.
#[cfg(test)]
pub(crate) fn serially<T>(body: impl FnOnce() -> T) -> T {
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
    // `into_inner` on a poisoned lock: an earlier test panicking inside the
    // guard must not turn every later one into a second failure with a worse
    // message than the first.
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    reset_for_test();
    let out = body();
    reset_for_test();
    drop(guard);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ScratchDir;

    #[test]
    fn with_nothing_chosen_the_root_is_the_platforms_own_temp_dir() {
        serially(|| {
            assert_eq!(root().unwrap(), std::env::temp_dir());
            assert_eq!(chosen(), None, "the default is not a choice the user made");
        });
    }

    #[test]
    fn a_chosen_folder_is_the_root_and_is_remembered_as_chosen() {
        serially(|| {
            let dir = ScratchDir::new("art-scratch-root", "chosen");
            let picked = dir.path().to_path_buf();
            assert_eq!(set(Some(&picked)).unwrap(), picked);
            assert_eq!(root().unwrap(), picked);
            assert_eq!(chosen(), Some(picked));
        });
    }

    #[test]
    fn clearing_the_choice_returns_to_the_default() {
        serially(|| {
            let dir = ScratchDir::new("art-scratch-root", "cleared");
            set(Some(dir.path())).unwrap();
            assert_eq!(set(None).unwrap(), std::env::temp_dir());
            assert_eq!(root().unwrap(), std::env::temp_dir());
            assert_eq!(chosen(), None);
        });
    }

    /// **The rule this module exists for.** A root that has gone away is a
    /// refusal, not a quiet return to `C:`.
    #[test]
    fn a_chosen_root_that_disappears_is_refused_rather_than_falling_back() {
        serially(|| {
            let dir = ScratchDir::new("art-scratch-root", "vanishes");
            let picked = dir.path().to_path_buf();
            set(Some(&picked)).unwrap();
            std::fs::remove_dir_all(&picked).unwrap();

            // The refusal itself *is* the property: an `Ok` here would mean
            // ART had quietly gone back to the system drive. Deliberately
            // not asserted by comparing the message with `temp_dir()` — a
            // test scratch directory lives under `temp_dir()`, so that
            // comparison passes and fails for reasons that have nothing to
            // do with the rule.
            let err = root().expect_err("a missing chosen root must refuse, never fall back");
            let text = err.user_message();
            assert!(
                text.contains(&picked.display().to_string()),
                "the refusal must name the folder: {text}"
            );
            assert!(
                text.contains("has been moved or removed"),
                "and must say ART touched nothing: {text}"
            );
            assert_eq!(err.code(), "ART-SCRATCH-UNAVAILABLE");
            assert_eq!(
                chosen(),
                Some(picked),
                "a refusal is not a reset: the user's choice stands until they change it"
            );
        });
    }

    /// A bad choice is refused where the user can still act on it, and the
    /// root they had stays in force — never "now ART has nowhere to stage".
    #[test]
    fn a_folder_that_is_not_there_is_refused_and_the_previous_root_stands() {
        serially(|| {
            let dir = ScratchDir::new("art-scratch-root", "keeps-previous");
            let good = dir.path().to_path_buf();
            set(Some(&good)).unwrap();

            let missing = good.join("no-such-folder");
            let err = set(Some(&missing)).expect_err("a folder that is not there must be refused");
            assert!(err.user_message().contains(&missing.display().to_string()));
            assert_eq!(
                root().unwrap(),
                good,
                "the refused choice must not have displaced the working one"
            );
        });
    }

    /// The probe writes, and leaves nothing behind.
    ///
    /// Two things in one, because they fail as one: `is_dir()` alone would
    /// accept a folder on a full disk, a read-only share or one the user has
    /// no permission for, and then fail on the first real byte — the wrong
    /// moment and the wrong screen. So the check writes. And because it
    /// writes, it has to clean up: a probe file appearing in the user's own
    /// folder every time they open Settings is litter ART put there.
    ///
    /// **Disclosed survivor:** this covers the probe *running*, not the
    /// branch where it *fails*. Making a directory that exists and cannot be
    /// written to needs an ACL change on Windows, which is a bench step and
    /// not a unit test; the failing branch is one `match` arm and is
    /// currently taken on trust. Said plainly rather than left to be
    /// discovered — a disclosed gap is worth more than a clean-looking table.
    #[test]
    fn the_check_actually_writes_and_removes_what_it_wrote() {
        serially(|| {
            let dir = ScratchDir::new("art-scratch-root", "probe");
            assert_eq!(
                std::fs::read_dir(dir.path()).unwrap().count(),
                0,
                "sanity: a fresh scratch directory is empty"
            );

            set(Some(dir.path())).unwrap();
            root().unwrap();
            set(Some(dir.path())).unwrap();

            let left: Vec<String> = std::fs::read_dir(dir.path())
                .unwrap()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            assert!(
                left.is_empty(),
                "the write probe left litter in the user's own folder: {left:?}"
            );
        });
    }

    /// A file is not a folder, and the sentence says which — "the folder is
    /// not there" would send somebody looking for a folder that is right
    /// where they left it.
    #[test]
    fn a_file_where_a_folder_should_be_says_so_in_its_own_words() {
        serially(|| {
            let dir = ScratchDir::new("art-scratch-root", "is-a-file");
            let file = dir.join("not-a-folder");
            std::fs::write(&file, b"x").unwrap();
            let err = set(Some(&file)).expect_err("a file must be refused");
            let text = err.user_message();
            assert!(
                text.contains("a file, not a folder"),
                "the refusal must say what is actually wrong: {text}"
            );
        });
    }
}
