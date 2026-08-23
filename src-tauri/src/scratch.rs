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

/// The prefix every scratch directory ART creates carries.
///
/// Production and tests alike: `art-osinstall-collisions-…`,
/// `art-preload-…`, `art-launch-…`, `art-amigainstall-…`. It is the only
/// thing that distinguishes ART's leavings from whatever else lives in the
/// folder, which is why [`sweep_crash_leftovers`] removes nothing without it.
const SCRATCH_PREFIX: &str = "art-";

/// How stale a leftover has to be before the sweep will touch it.
///
/// **A day, not the hour `sweep_stale_preview_scratch_dirs` uses.** That one
/// guards a single narrow prefix belonging to a preview that finishes in
/// seconds. This one runs across everything ART stages, and ART stages some
/// genuinely long work: a card write, or an install placing the owner's 1915
/// files, is hours. An hour here would eventually reap a directory a live job
/// was still filling — and a job that loses its staging mid-write is a worse
/// outcome than a leftover that survives one more day.
///
/// Directory mtime is also the wrong clock for this on Windows: it moves when
/// entries are added to *that* directory, not when a file three levels down
/// is written. The margin is what covers the gap.
const LEFTOVER_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// What one sweep did, so it can be logged rather than done in silence.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Swept {
    /// Directories removed.
    pub removed: usize,
    /// Directories that matched the prefix and were **not** old enough.
    pub too_new: usize,
    /// Directories that matched and were old enough, but could not be
    /// removed — in use, or a permission ART does not have.
    pub failed: usize,
}

/// Whether this process can be sure it is the only ART running.
///
/// Fails **closed**: no lock means no sweep. A second instance's live staging
/// directory is indistinguishable from a dead one's leftovers by age alone —
/// mtime does not move while a job writes deep inside — so the only safe
/// answer when ART cannot tell is to remove nothing and let the next run do
/// it. A leftover costs disk; deleting a running job's staging costs the job.
///
/// The lock file is held for as long as the returned handle lives, which is
/// the duration of the sweep. It is a dotfile, so it never matches
/// [`SCRATCH_PREFIX`] and the sweep cannot reap its own lock.
#[cfg(windows)]
fn take_sweep_lock(root: &Path) -> Option<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    /// `dwShareMode = 0`: nobody else may open it at all while this handle is
    /// alive, which is precisely the question being asked.
    const EXCLUSIVE: u32 = 0;
    std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .share_mode(EXCLUSIVE)
        .open(root.join(".art-sweep.lock"))
        .ok()
}

/// The same question where the Win32 sharing model does not exist.
///
/// ART ships on Windows; this arm exists so `core`-adjacent code still builds
/// and tests run elsewhere. It does not lock, so it answers "yes, sweep" —
/// stated plainly rather than left to look like a lock that works.
#[cfg(not(windows))]
fn take_sweep_lock(root: &Path) -> Option<std::fs::File> {
    std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join(".art-sweep.lock"))
        .ok()
}

/// Remove what a crash left in `root` (ART-184's other half).
///
/// [`root`] made where ART stages a choice; it does not tidy up after an ART
/// that died mid-stage. A killed job, a panic, a machine that lost power:
/// each leaves a staging directory nothing will ever come back for, and the
/// measured worst case for that class was 169,291 directories and ~987 GB.
///
/// **Four conditions, all of them, before anything is removed.** A sweep is a
/// delete ART performs without being asked, in a folder the user chose, so
/// every one of these is a refusal rather than a filter:
///
/// 1. **This process is the only ART** — see [`take_sweep_lock`].
/// 2. **Directly inside the root.** Never a descent: ART's leftovers are
///    top-level, and a recursive hunt through a folder the user pointed at
///    would be ART walking somewhere it was not invited.
/// 3. **Named [`SCRATCH_PREFIX`]**, and a directory. ART cannot prove it made
///    a given folder, and this is the closest it gets; a user who points the
///    root at a folder holding their own `art-…` directories is told so on
///    the Settings screen, because the alternative is a silent policy.
/// 4. **Older than [`LEFTOVER_MAX_AGE`].**
///
/// Best-effort throughout, like the sweep it is modelled on: a directory this
/// pass cannot read or remove is counted and left, never escalated into a
/// failure of whatever the caller was actually doing.
pub fn sweep_crash_leftovers(root: &Path) -> Swept {
    let mut swept = Swept::default();
    let Some(_lock) = take_sweep_lock(root) else {
        return swept;
    };
    let Ok(entries) = std::fs::read_dir(root) else {
        return swept;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(SCRATCH_PREFIX)
        {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_dir() {
            continue;
        }
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        // `duration_since` fails on a directory stamped in the future — a
        // clock change, or a file copied from a machine ahead of this one.
        // Not old, then; leave it.
        let Ok(age) = now.duration_since(modified) else {
            swept.too_new += 1;
            continue;
        };
        if age <= LEFTOVER_MAX_AGE {
            swept.too_new += 1;
            continue;
        }
        match std::fs::remove_dir_all(entry.path()) {
            Ok(()) => swept.removed += 1,
            Err(_) => swept.failed += 1,
        }
    }
    swept
}

/// Sweep `root` at most once per root per run, on a thread of its own.
///
/// Called wherever the effective root becomes known, which is more than once
/// — the start-up query and every Settings change both resolve one. Removing
/// a thousand directories is seconds of I/O and belongs nowhere near a
/// command's own latency, and the result is logged because a delete ART
/// performed unasked is not something to do in silence.
///
/// Returns whether this call is the one that scheduled the sweep, so the
/// "once per root, and a changed root is a new root" rule is a property a
/// test can assert rather than a comment.
pub fn sweep_once(root: &Path) -> bool {
    use std::collections::HashSet;
    static DONE: RwLock<Option<HashSet<PathBuf>>> = RwLock::new(None);

    let root = root.to_path_buf();
    {
        let mut done = DONE
            .write()
            .expect("the sweep ledger lock is never held across a panic");
        if !done.get_or_insert_with(HashSet::new).insert(root.clone()) {
            return false;
        }
    }

    // **Scheduled, not performed, under `cargo test`.** The whole suite runs
    // in one process against one real `%TEMP%`, and a background thread
    // deleting directories there as a side effect of asking a command a
    // question is not something a test run should do — least of all in the
    // folder ART-184 filled. The removal itself is tested by calling
    // [`sweep_crash_leftovers`] against a directory the test owns.
    #[cfg(not(test))]
    std::thread::spawn(move || {
        let swept = sweep_crash_leftovers(&root);
        if swept.removed > 0 || swept.failed > 0 {
            log::info!(
                "Scratch sweep in {}: removed {}, still in use {}, too new to touch {}",
                root.display(),
                swept.removed,
                swept.failed,
                swept.too_new
            );
        }
    });

    true
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

    // ---- ART-184: what a crash leaves behind ----

    /// Backdate `path`'s mtime by `age`, so a test can have an old directory
    /// without waiting a day for one.
    ///
    /// **The alternative was a test that sleeps**, and this project has a
    /// rule about those: anything timing-dependent gets an invariant rather
    /// than a wait (ART-182). The clock is what the sweep reads, so the clock
    /// is what the test sets.
    fn backdate(path: &Path, age: std::time::Duration) {
        stamp(path, std::time::SystemTime::now() - age);
    }

    /// Set one path's mtime, directory or file.
    ///
    /// **A directory needs `FILE_FLAG_BACKUP_SEMANTICS` on Windows** —
    /// `File::open` on one is `PermissionDenied` without it, which is how the
    /// first version of these tests failed. `FILE_WRITE_ATTRIBUTES` is the
    /// access being asked for; neither read nor write data is wanted, and
    /// asking for write on a directory fails on its own.
    #[cfg(windows)]
    fn stamp(path: &Path, when: std::time::SystemTime) {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        std::fs::OpenOptions::new()
            .access_mode(FILE_WRITE_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
            .expect("open the path to stamp it")
            .set_modified(when)
            .expect("stamp it");
    }

    #[cfg(not(windows))]
    fn stamp(path: &Path, when: std::time::SystemTime) {
        std::fs::File::open(path)
            .expect("open the path to stamp it")
            .set_modified(when)
            .expect("stamp it");
    }

    fn old_dir(root: &Path, name: &str) -> PathBuf {
        let path = root.join(name);
        std::fs::create_dir_all(path.join("staged")).expect("make a leftover");
        std::fs::write(path.join("staged").join("file"), b"bytes").expect("fill it");
        backdate(&path, LEFTOVER_MAX_AGE + std::time::Duration::from_secs(60));
        path
    }

    /// The case the issue is about: ART died mid-stage, and the directory it
    /// was filling is never coming back for.
    #[test]
    fn a_days_old_leftover_of_arts_own_is_removed_with_what_is_inside_it() {
        let dir = ScratchDir::new("art-scratch-root", "sweep-old");
        let stale = old_dir(dir.path(), "art-osinstall-collisions-deadbeef");

        let swept = sweep_crash_leftovers(dir.path());

        assert!(!stale.exists(), "a day-old leftover is what this is for");
        assert_eq!(swept.removed, 1);
        assert_eq!(swept.failed, 0);
    }

    /// **The one that must never fire.** A job still writing is the reason
    /// the age is a day and not an hour, and a sweep that reaps live staging
    /// costs more than every leftover it has ever removed.
    #[test]
    fn a_directory_younger_than_a_day_is_left_alone() {
        let dir = ScratchDir::new("art-scratch-root", "sweep-young");
        let live = dir.path().join("art-preload-in-flight");
        std::fs::create_dir_all(&live).expect("make a live staging dir");

        let swept = sweep_crash_leftovers(dir.path());

        assert!(live.exists(), "a job may still be filling this");
        assert_eq!(swept.removed, 0);
        assert_eq!(swept.too_new, 1);
    }

    /// ART removes what ART named. The scratch root is a folder **the user
    /// chose**, and it may be one they keep other things in.
    #[test]
    fn nothing_without_arts_own_prefix_is_touched_however_old_it_is() {
        let dir = ScratchDir::new("art-scratch-root", "sweep-theirs");
        let theirs = old_dir(dir.path(), "Workbench3.2-backup");
        let also_theirs = dir.path().join("notes.txt");
        std::fs::write(&also_theirs, b"mine").expect("write a file of the user's");
        backdate(
            &also_theirs,
            LEFTOVER_MAX_AGE + std::time::Duration::from_secs(60),
        );

        // **A file carrying the prefix**, which is the case the `is_dir`
        // check is the only thing standing in front of: `remove_dir_all` on a
        // file fails, so without it this would be counted as a leftover ART
        // could not remove and logged as "still in use" — a wrong sentence
        // about a file nothing was ever going to delete.
        let log = dir.path().join("art-preload-run.log");
        std::fs::write(&log, b"a log somebody kept").expect("write it");
        backdate(&log, LEFTOVER_MAX_AGE + std::time::Duration::from_secs(60));

        let swept = sweep_crash_leftovers(dir.path());

        assert!(theirs.exists(), "not ART's, whatever its age");
        assert!(also_theirs.exists(), "a file is not a staging directory");
        assert!(log.exists(), "nor is one that happens to be named like one");
        assert_eq!(swept, Swept::default(), "and nothing was even considered");
    }

    /// **Top-level only.** A recursive hunt through a folder the user pointed
    /// at would be ART walking somewhere it was not invited, and an
    /// `art-…`-named directory *inside* the user's own tree is theirs.
    #[test]
    fn the_sweep_does_not_descend_into_what_it_is_not_removing() {
        let dir = ScratchDir::new("art-scratch-root", "sweep-nested");
        let theirs = dir.path().join("my-amiga-stuff");
        std::fs::create_dir_all(&theirs).expect("make the user's folder");
        let nested = old_dir(&theirs, "art-launch-old");

        let swept = sweep_crash_leftovers(dir.path());

        assert!(nested.exists(), "one level down is not ART's to sweep");
        assert_eq!(swept.removed, 0);
    }

    /// A directory stamped in the future — a clock change, or a copy from a
    /// machine ahead of this one — is not old. `duration_since` fails on it,
    /// and failing to age something must not read as "old enough".
    #[test]
    fn a_directory_stamped_in_the_future_is_not_treated_as_ancient() {
        let dir = ScratchDir::new("art-scratch-root", "sweep-future");
        let ahead = dir.path().join("art-tomorrow");
        std::fs::create_dir_all(&ahead).expect("make it");
        stamp(
            &ahead,
            std::time::SystemTime::now() + std::time::Duration::from_secs(48 * 60 * 60),
        );

        let swept = sweep_crash_leftovers(dir.path());

        assert!(ahead.exists());
        assert_eq!(swept.removed, 0);
        assert_eq!(swept.too_new, 1);
    }

    /// **The screen says "more than a day old" in two languages.** A sweep is
    /// a delete ART performs without being asked, so the sentence that
    /// describes it is part of the feature and not documentation of it — this
    /// pins the two together, because changing the constant and leaving the
    /// sentence would put a confident, wrong claim about deleting the user's
    /// files on screen.
    ///
    /// Widening the policy means rewriting `settings.scratchRootHint` and
    /// `scratch.ask.note` in `en.json` **and** `tr.json`, and this test is
    /// where that is remembered.
    #[test]
    fn the_age_the_screen_promises_is_the_age_the_sweep_keeps() {
        assert_eq!(
            LEFTOVER_MAX_AGE,
            std::time::Duration::from_secs(24 * 60 * 60),
            "the two catalogues promise a day"
        );
    }

    /// **The fail-closed branch, and it is the one that matters most.** If
    /// ART cannot establish that it is the only instance, it removes nothing
    /// — because a second instance's live staging looks exactly like a dead
    /// one's leftovers from the outside.
    ///
    /// Testable in one process because Win32 sharing is per *handle*, not per
    /// process: this test holding the lock exclusively is indistinguishable
    /// from another ART holding it, which is the whole mechanism.
    #[cfg(windows)]
    #[test]
    fn while_another_art_may_be_running_nothing_is_swept() {
        let dir = ScratchDir::new("art-scratch-root", "sweep-locked");
        let stale = old_dir(dir.path(), "art-osinstall-collisions-locked");

        let held = take_sweep_lock(dir.path()).expect("this test takes the lock first");
        let swept = sweep_crash_leftovers(dir.path());
        assert!(stale.exists(), "a second instance may be filling this");
        assert_eq!(swept, Swept::default(), "and the sweep did not even look");

        // Released, and the same call now does the work — so the test is
        // measuring the lock and not some other reason nothing happened.
        drop(held);
        let swept = sweep_crash_leftovers(dir.path());
        assert!(!stale.exists());
        assert_eq!(swept.removed, 1);
    }

    /// Once per root per run, and a **changed** root is a new root — the
    /// Settings screen's whole point is that the answer can change.
    #[test]
    fn a_root_is_swept_once_a_run_and_a_new_root_is_swept_too() {
        let first = ScratchDir::new("art-scratch-root", "sweep-once-a");
        let second = ScratchDir::new("art-scratch-root", "sweep-once-b");

        assert!(sweep_once(first.path()), "the first ask schedules it");
        assert!(!sweep_once(first.path()), "the second does not");
        assert!(!sweep_once(first.path()));
        assert!(
            sweep_once(second.path()),
            "a different root is a different sweep"
        );
    }

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
