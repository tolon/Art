//! Running a package's own installer on the Amiga, because the host cannot.
//!
//! ART builds a distribution tree by placing files from the host, and that
//! has a measured ceiling: the two AmigaOS BoingBags carry ZipCrypto-encrypted
//! payloads whose password lives in the package's own Amiga-side `Updater`
//! (ART-166), and packages like `Euro-Update` install through an Amiga
//! Installer script. Both are the same shape — **work that belongs on the
//! Amiga** — and every established distribution builder (HstWB Installer,
//! AmiKit, ClassicWB) already does it there.
//!
//! **Nothing here decrypts anything and no protection is bypassed.** The
//! program that already holds the password is run where it belongs, by the
//! machine it was written for.
//!
//! ## The shape of a run
//!
//! ART mounts the distribution tree as **data** and its own work volume as the
//! **boot device**, at the highest boot priority — the mechanism
//! [`crate::core::winuae::DirMount`]'s `boot_priority` documents and that "one
//! click starts the game" already uses. The work volume carries one generated
//! AmigaDOS script ([`workvol`]); the script runs the installer the package's
//! recipe names and writes [`RESULT_FILE`]. The host polls that file — a write
//! into a `filesystem2=rw` directory mount was measured on 2026-08-20 to be
//! visible on the host *while the emulator is still running*, with the pid
//! confirmed alive — and terminates the emulator it started.
//!
//! **The user's `Startup-Sequence` is never touched.** Appending to it would
//! be the one place that does not execute: the same measurement showed a line
//! after a tree's own sequence never runs, because the sequence ends with
//! `LoadWB`/`EndCLI`.
//!
//! The install runs against a **copy** of the tree, and the copy replaces the
//! original only when the result says it succeeded (§92).

pub mod workvol;

use std::time::Duration;

use serde::Serialize;

/// The file the Amiga writes and the host reads, in the root of ART's own
/// work volume.
///
/// It lives on ART's volume rather than in the tree because the tree is the
/// thing being installed into: a result file written there would be one more
/// difference between the copy and the original, and would have to be cleaned
/// up before the copy could replace it.
pub const RESULT_FILE: &str = "art-result.txt";

/// The Amiga volume label ART's own work volume is mounted under.
///
/// A constant rather than a field because ART owns *both* ends of it — the
/// mount and the script that writes to it — so a mismatch between the two is
/// not a thing that can happen.
///
/// It is deliberately not `Work`, which is the name of an ordinary partition
/// on a great many real Amiga setups: a distribution tree that carries a
/// `Work:` assign of its own would shadow ART's volume, and the result file
/// would then be written somewhere ART is not looking.
pub const WORK_VOLUME: &str = "ARTWork";

/// Written before the installer is invoked. See [`workvol::startup_sequence`]
/// for why it exists — on its own it is **not** an outcome.
pub const MARK_STARTED: &str = "started";

/// Written when the installer returned a non-warning, non-failing code.
pub const MARK_OK: &str = "ok";

/// Written when the installer returned `WARN` or worse — it ran, and it said
/// no.
pub const MARK_FAILED: &str = "failed";

/// What ART will run on the Amiga, and where.
///
/// Every string in here is **shipped recipe data or a name ART chose itself**.
/// Nothing in it may come from an archive's contents: a generated AmigaDOS
/// script is a command interpreter's input, and the design's rule is that
/// nothing ART generates is assembled from a string ART did not author. The
/// guard that enforces it is
/// [`crate::core::security::refuse_shell_metacharacters`], applied to every
/// field before any of them is formatted into a line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedRun {
    /// The recipe id of the package being installed — the only name a report
    /// can honestly give, since ART did not write the installer and cannot
    /// describe what it does.
    pub package_id: String,
    /// The Amiga volume the distribution tree copy is mounted as, e.g. `DH0`.
    /// The script assigns `SYS:`, `C:` and the rest from it, because ART's own
    /// volume is the one that booted and carries none of them (ART-118).
    pub system_volume: String,
    /// The installer's AmigaDOS path as it will be reachable at run time, e.g.
    /// `PKG:C/Updater`. Composed at the command layer from the recipe's
    /// declaration plus the volume ART mounted the package under — translating
    /// one module's representation into another's is a command-layer job.
    pub program: String,
    /// Arguments, **each a separate string** — never one line to be split, so
    /// nothing can be reinterpreted as a second command.
    pub args: Vec<String>,
}

/// What a run ended as. **Three outcomes, not two.**
///
/// An Amiga Installer is interactive by nature, so a run that stops on a
/// requester would otherwise wait for ever. A timeout is therefore its own
/// answer: a failure means the installer said no, a timeout means nobody was
/// there to answer its question, and the second is fixed by watching the
/// window rather than by changing anything. Reporting a timeout as either of
/// the other two would be claiming something that was not observed (§89).
///
/// The original tree is untouched in all three cases; only [`Succeeded`] lets
/// the copy replace it.
///
/// [`Succeeded`]: Self::Succeeded
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RunOutcome {
    Succeeded {
        message: String,
    },
    /// The installer ran and said no.
    Failed {
        message: String,
    },
    /// Nobody answered its question. Not a failure, and explicitly not a
    /// success — the two are fixed by different things.
    TimedOut {
        waited: Duration,
    },
}
