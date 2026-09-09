//! Running a package's own installer on the Amiga — the command layer.
//!
//! Everything this module joins was built without it: the work volume and its
//! generated script ([`crate::core::amigainstall::workvol`]), the declaration
//! of what a package runs
//! ([`crate::core::osinstall::package::AmigaInstaller`]), the run itself
//! behind two injected seams ([`crate::core::amigainstall::run`]), and the
//! copy the install happens against
//! ([`crate::core::amigainstall::stage`]). **Nothing here decrypts anything
//! and no protection is bypassed**: the package's own program runs on the
//! machine it was written for, which is the owner's recorded decision.
//!
//! Two commands, the shape §92 gives every data-changing operation:
//!
//! - [`amiga_install_preview`] — **read-only**. What would run, on which
//!   tree, with which package, and whether the two things ART cannot supply
//!   (the user's Kickstart, an emulator) are there. It launches nothing and
//!   writes nothing, and a test proves the directory it was asked about is
//!   untouched afterwards.
//! - [`amiga_install_run`] — a job (§54/§55), returning a job id at once.
//!
//! ## What this module actually decides
//!
//! `PlannedRun::program`'s own documentation hands one job here and names it:
//! a recipe declares a path **inside the package** (`C/Updater`) and refuses
//! to name a volume at all, so the whole AmigaDOS path — and the proof that
//! it stayed inside the volume ART mounted — is composed here, in
//! [`compose`], and pinned by tests here. Three things are composed and
//! nothing else:
//!
//! 1. **Where the package is.** `ARTPkg:{package_dir}` — the volume ART mounts
//!    the package's own **unpacked wrapper** under
//!    ([`crate::core::amigainstall::PACKAGE_VOLUME`]), plus the drawer inside
//!    that wrapper. **It used to be the system volume, and that was ART-185**:
//!    a BoingBag cannot be placed into the tree at all — not being placeable
//!    on the host is the whole reason this round exists — so a path rooted in
//!    the tree named a program that was never there. The drawer defaults to
//!    the package's own recipe `media`, the archive's top-level directory and
//!    shipped data; a caller may override it for a repack. Every segment is
//!    checked: no `:` (a recipe or a caller may not decide which volume the
//!    run reaches), no `..` or `.`, no empty segment (AmigaDOS reads a leading
//!    or doubled `/` as the parent directory), no `\`.
//! 2. **The installer's whole path**, that location joined to the recipe's
//!    declared program.
//! 3. **The target argument.** `boingbag-39-1.json`'s own reading of the
//!    package's `Install` script — `C/Updater AmigaOS-Update "<target>"` —
//!    records that the last argument is the volume being installed into, and
//!    that it is deliberately *not* in the recipe because it is a fact about
//!    the run rather than about the package. So `{volume}:` is appended here.
//!
//! **And the wrapper is unpacked**, into a scratch directory of ART's own,
//! through [`crate::core::amigainstall::packagevol`]. That is the other half
//! of ART-185, and the reason a third mount was necessary but not sufficient:
//! nothing anywhere put the package's files on the host in the first place.
//! The unpack proves the drawer and the installer really arrived, so an
//! archive that is not this package's is refused **by name**, before the
//! emulator starts.
//!
//! Everything else the recipe declares passes through **exactly as written**.
//! ART cannot tell a path argument from a keyword like `QUIET` in a program it
//! did not write, so it rewrites none of them; what it does instead is run the
//! installer from the package's own drawer
//! ([`PlannedRun::working_directory`]), which is where the package's own
//! script runs it from and what makes a relative argument resolve.
//!
//! ## One token, because the generated line cannot quote
//!
//! `refuse_shell_metacharacters` refuses `"` — deliberately, since a quote
//! changes where a string ends — and the generated script joins the program
//! and its arguments with spaces. So a value carrying a space would arrive at
//! the Amiga as two arguments and there is no way to say otherwise. Every
//! value **this module composes** is therefore refused if it carries
//! whitespace ([`one_token`]), rather than quietly generating a line that
//! means something else. AmigaDOS names legitimately contain spaces, which is
//! why this is a refusal with a sentence and not a silent rewrite.
//!
//! ## The four endings, and what happens to the copy
//!
//! [`RunOutcome`] has four variants and only `Succeeded` promotes the copy
//! over the user's tree. The other three leave the original untouched **and
//! the copy in place**, and [`SettlementReport`] carries both paths so the
//! report can say both halves: your system at *X* is exactly as it was, what
//! the installer did is at *Y*.
//!
//! A **cancellation is not a fourth ending** — the run produced no answer —
//! and it is the one path where the copy does not survive:
//! [`perform`] calls `discard()` on it. `Staged` has no `Drop` on purpose
//! (a discarding one would destroy the evidence a failed run exists to keep),
//! so that decision is made explicitly on every path out of this module.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use super::jobs::{spawn_job, JobRegistry};
use super::oplog::{user_operation, write_to_path};
use crate::core::amigainstall::run::{run_with, RealClock, RunLimits, RunRequest};
use crate::core::amigainstall::stage::{settle, stage_with, Settlement};
use crate::core::amigainstall::{
    packagevol, workvol, PlannedRun, RunOutcome, PACKAGE_VOLUME, RESULT_FILE, WORK_VOLUME,
};
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::{JobId, ProgressSink};
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::core::osinstall::source::MediaSource;
use crate::core::osinstall::source_archive::ArchiveSource;
use crate::core::osinstall::{chain, package};
use crate::core::profile::AmigaProfile;
use crate::core::sources::install::Scratch;
use crate::core::winuae::detect_winuae;
use crate::error::AppResult;
use crate::tools::winuae_launcher::WinUaeLauncher;

/// The volume a distribution tree is mounted as when the caller says nothing.
///
/// `DH0` is what `core::amigainstall::run`'s own tests and the WHDLoad launch
/// path already use for a directory mounted as a system volume.
pub const DEFAULT_SYSTEM_VOLUME: &str = "DH0";

/// The machine an Amiga-side install runs on when the caller says nothing.
///
/// AmigaOS 3.9 — the release both BoingBags update — needs a 68020 or better,
/// so an A500 preset would refuse to boot the very tree this exists to
/// install into. A named default rather than a silent one: an id ART does not
/// ship is refused by name instead of falling back to something.
pub const DEFAULT_PROFILE_ID: &str = "a1200-aga";

// ---------------------------------------------------------------------------
// The wire
// ---------------------------------------------------------------------------

/// What the screen asks for, for both the preview and the run.
///
/// One type for the two commands on purpose: a preview that could describe a
/// run the following command would not perform is worse than no preview.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmigaInstallRequest {
    /// The distribution tree. Never written to — the install runs against a
    /// copy, and the copy replaces this only on success (§92).
    pub tree: PathBuf,
    /// A package ART ships a recipe for. Anything else is refused: this round
    /// does not make ART able to run whatever a user points at.
    pub package_id: String,
    /// The Amiga volume the tree is mounted as. `None` means
    /// [`DEFAULT_SYSTEM_VOLUME`]. A name, never a name with a colon — ART
    /// adds that itself.
    #[serde(default)]
    pub system_volume: Option<String>,
    /// The package's **own** archive — the wrapper the user downloaded,
    /// `BoingBags3&4.lha`; ART unpacks it to a directory of its own and mounts
    /// that as a third volume.
    ///
    /// **Required, and added by ART-185.** Nothing else can supply the
    /// installer: the program the run executes is in no volume ART mounts
    /// unless it comes from here.
    ///
    /// **One, not a list, since 2026-09-08.** It used to be a list whose
    /// second and later entries were overlay media (ART-186's UAE fix for
    /// BoingBag 3.9-1's 45.13 `Updater`). Both BoingBags are placed from
    /// Windows now, no shipped recipe declares an overlay, and a second slot
    /// nothing can fill is a field that can only be filled wrongly.
    pub package_archive: PathBuf,
    /// Where the package's **own** files sit inside that unpacked wrapper,
    /// `/`-separated — `BoingBag3.9-1`, which is the drawer every one of the
    /// owner's real wrappers carries at its top level beside its icon.
    ///
    /// `None` takes the package's recipe `media`, which is that same drawer as
    /// shipped data; an explicit value is for a repack whose drawer somebody
    /// renamed. An empty string means the wrapper's own root.
    #[serde(default)]
    pub package_dir: Option<String>,
    /// The user's own licensed Kickstart. ART ships none and never will.
    pub kickstart: PathBuf,
    /// A machine preset id (`AmigaProfile::all_presets`). `None` means
    /// [`DEFAULT_PROFILE_ID`].
    #[serde(default)]
    pub profile: Option<String>,
}

/// What would run, on which tree, with which package — and what is missing.
///
/// Read-only (§92's PREVIEW). Every field is either recipe data, something
/// this module composed, or the answer to an `is_file` question.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AmigaInstallPreview {
    pub package_id: String,
    /// The package's own name, untranslated — a package's name is its own,
    /// the way a volume's is (ART-060).
    pub package_name: String,
    pub tree: PathBuf,
    pub system_volume: String,
    /// The drawer the installer is run from, as AmigaDOS will see it.
    pub working_directory: Option<String>,
    /// The installer's whole AmigaDOS path.
    pub program: String,
    /// Its arguments, each its own token, in the order they will be passed.
    pub args: Vec<String>,
    /// ART's own volume, mounted alongside the tree at the highest boot
    /// priority — the screen should say a second volume exists, because the
    /// user will see it on the Workbench.
    pub work_volume: String,
    /// The volume the package's own unpacked wrapper is mounted as — the
    /// **third**, and the one ART-185 was missing. Named here for the same
    /// reason as `work_volume`: the user will see it on the Workbench.
    pub package_volume: String,
    /// The package's own archive, as the user chose it.
    pub package_archive: PathBuf,
    /// Whether it is actually there. A preview that did not ask would be
    /// describing a run with nothing to run.
    pub package_archive_present: bool,
    /// The drawer inside that archive the installer is expected in, or `None`
    /// for the archive's own root.
    pub package_dir: Option<String>,
    /// The file the Amiga writes and the host polls.
    pub result_file: String,
    /// How long the run may go without an answer before ART ends the
    /// emulator it started.
    pub deadline_seconds: u64,
    pub kickstart: PathBuf,
    /// Whether that Kickstart is actually there. The run refuses without one
    /// rather than falling back to AROS, so a preview that did not ask this
    /// would be describing a run that cannot start.
    pub kickstart_present: bool,
    /// The emulator ART would start, or `None` when it found none. **A person
    /// should not be surprised by a machine window** (design §4), so the
    /// screen has to be able to name it before anything is confirmed.
    pub emulator: Option<String>,
    pub profile_id: String,
    pub profile_name: String,
}

/// What [`settle`] did, on the wire.
///
/// A command-layer type rather than `Settlement` serialized: turning one
/// module's representation into another's is this layer's job, and `core/` is
/// meant to stay promotable without the frontend's naming conventions in it.
///
/// **The inner `rename_all` is load-bearing.** `#[serde(rename_all)]` on an
/// enum renames the *variants*, not the fields of a struct variant — that was
/// a real wire bug in this project this week — so `left_behind` would go out
/// as `left_behind` without the second attribute. `settlement_is_camel_case_
/// on_the_wire_including_inside_a_variant` pins it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SettlementReport {
    /// The run succeeded and the copy is now the tree.
    #[serde(rename_all = "camelCase")]
    Promoted {
        tree: PathBuf,
        /// The previous tree, when it could not be removed after the swap —
        /// something held it open. Not an error: the promotion happened. The
        /// screen names it so the user can delete it.
        left_behind: Option<PathBuf>,
    },
    /// The run did not succeed. Both paths, because the sentence the user
    /// needs has two halves.
    Kept { copy: PathBuf, original: PathBuf },
}

impl From<Settlement> for SettlementReport {
    fn from(settlement: Settlement) -> Self {
        match settlement {
            Settlement::Promoted(committed) => Self::Promoted {
                tree: committed.tree,
                left_behind: committed.left_behind,
            },
            Settlement::Kept { copy, original } => Self::Kept { copy, original },
        }
    }
}

/// The event a finished run's own answer arrives on.
pub const AMIGA_INSTALL_EVENT: &str = "amiga-install-result";

// Deliberately not camelCased — `job_id` matches `OsInstallResult` and every
// other job result in ART, and `src/lib/amigainstall.ts` declares `job_id` to
// match.
#[derive(Debug, Clone, Serialize)]
pub struct AmigaInstallResult {
    pub job_id: u64,
    /// Which of the four endings it was. Mirrored exactly in TypeScript.
    pub outcome: RunOutcome,
    pub settlement: SettlementReport,
}

// ---------------------------------------------------------------------------
// Composing the run
// ---------------------------------------------------------------------------

/// A validated run plus the one thing about the package a report needs that
/// the plan does not carry.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Composed {
    plan: PlannedRun,
    package_name: String,
    /// The drawer inside the unpacked wrapper, `/`-separated, or `None` for
    /// the wrapper's own root. The AmigaDOS side of it is already in
    /// `plan.working_directory`; this is the host side, and
    /// `packagevol::unpack` needs it to prove the drawer really arrived.
    package_dir: Option<String>,
    /// The installer's path **inside** that drawer, as the recipe declares it
    /// (`C/Updater`) — likewise for the unpack's proof, and likewise not the
    /// same string as `plan.program`, which is the whole AmigaDOS path.
    installer_in_package: String,
    /// Every *other* package `overlay_mismatch_sentence` may name in a
    /// wrong-archive refusal — scoped to the releases the selected package's
    /// own recipe declares, and with the selected package's own id already
    /// removed (ART-277 re-review, L3/L4: this used to be *every* shipped
    /// package regardless of release, which could name a package a
    /// different-release build can never reach). Built once, here, so
    /// `install` does not recompute it and cannot drift from what `compose`
    /// already decided was reachable.
    catalogue: Vec<packagevol::KnownPackage>,
}

/// Refuse a value that cannot survive being written into the generated line.
///
/// See the module documentation: the line joins its parts with spaces and
/// cannot quote, so anything this module composes has to be one token.
fn one_token(label: &str, value: &str) -> CoreResult<()> {
    if value.chars().any(char::is_whitespace) {
        return Err(CoreError::InvalidInput(format!(
            "'{value}' cannot be used as {label}: ART's generated AmigaDOS line separates its \
             arguments with spaces and cannot quote one, so every part of it must be a single \
             word"
        )));
    }
    Ok(())
}

/// The volume name the tree is mounted as.
///
/// A bare name: the colon is ART's to add, and a caller that could write one
/// could write a second path component after it.
fn system_volume(raw: Option<&str>) -> CoreResult<String> {
    let name = raw
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_SYSTEM_VOLUME);
    if name.contains(':') {
        return Err(CoreError::InvalidInput(format!(
            "'{name}' is a volume name, not a path: write it without a colon"
        )));
    }
    one_token("a volume name", name)?;
    Ok(name.to_string())
}

/// `{volume}:` or `{volume}:{dir}` — where the package's own files are, as
/// AmigaDOS will see them.
///
/// **This is the containment proof `PlannedRun::program` says it cannot make
/// itself.** The result begins with the volume ART mounted the tree under, and
/// nothing in `dir` may take it back out of that volume: no `:` (which would
/// name a different one), no empty segment (AmigaDOS reads a leading or
/// doubled `/` as the parent directory), no `..` or `.`, no `\`.
fn package_location(volume: &str, dir: Option<&str>) -> CoreResult<String> {
    let Some(dir) = dir.map(str::trim).filter(|d| !d.is_empty()) else {
        return Ok(format!("{volume}:"));
    };
    for segment in dir.split('/') {
        if segment.is_empty() {
            return Err(CoreError::InvalidInput(format!(
                "'{dir}' is not a path inside the tree: a leading, doubled or trailing '/' is \
                 AmigaDOS's own parent-directory notation"
            )));
        }
        if segment == ".." || segment == "." {
            return Err(CoreError::InvalidInput(format!(
                "'{dir}' is not a path inside the tree: '{segment}' leaves it"
            )));
        }
        if segment.contains(':') || segment.contains('\\') {
            return Err(CoreError::InvalidInput(format!(
                "'{dir}' is not a path inside the tree: the volume it is reached under is ART's \
                 to decide"
            )));
        }
        one_token("a package directory", segment)?;
    }
    Ok(format!("{volume}:{dir}"))
}

/// Join an AmigaDOS location to a path below it. A location ending in `:` is
/// a volume root and takes no separator.
fn join_amigados(location: &str, tail: &str) -> String {
    if location.ends_with(':') {
        format!("{location}{tail}")
    } else {
        format!("{location}/{tail}")
    }
}

/// Turn a request into the run it describes, or say why there is none.
///
/// Ends by generating the script through
/// [`workvol::startup_sequence`] and throwing the text away. That is not a
/// second copy of its guards — it *is* those guards, run early: a preview
/// that answered where the run would refuse would be a preview of something
/// that cannot happen, and a shell metacharacter or a value naming ART's own
/// work volume must be refused before a screen offers a confirm button, not
/// after.
fn compose(request: &AmigaInstallRequest) -> CoreResult<Composed> {
    compose_over(request, package::by_id(request.package_id.trim())?)
}

/// [`compose`]'s own derivation, over a package already resolved.
///
/// The same split `package::order_over`, `slots::slots_over` and
/// `known_packages_from` make in this project, and for the same reason: since
/// 2026-09-08 the only shipped recipe declaring an `amiga_installer` is
/// `boingbags-39-3-4`, whose declaration is `not_yet_runnable` and so refused
/// two lines below. Without this split every guard in this file — the volume
/// containment, the metacharacter gate, the wrong-archive refusal, the
/// prerequisite check — would have no shipped package it could be exercised
/// against at all.
fn compose_over(request: &AmigaInstallRequest, package: package::Package) -> CoreResult<Composed> {
    let Some(installer) = package.amiga_installer.clone() else {
        return Err(CoreError::InvalidInput(format!(
            "ART ships no Amiga-side installer for '{}'; this is not a package it can run on \
             the Amiga",
            package.id
        )));
    };

    // **Declared, and never run.** A recipe may say what installs a package
    // before anybody has measured that ART can drive it — §10's "register an
    // unready action rather than hiding it", from the side that has to be
    // guarded. The panel already renders such a row disabled with this
    // sentence; this is the guard that means a request built any other way
    // is refused too, rather than reaching the emulator on the strength of a
    // greyed-out checkbox.
    //
    // Its own refusal, deliberately not folded into the one above: "ART ships
    // no installer for this" and "this one has not been run yet" are
    // different sentences with different next steps, and the second names
    // what would have to happen for it to become the first.
    if let Some(why) = installer.not_yet_runnable {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' is not a package ART can run yet: {}",
            package.name,
            crate::commands::osinstall::describe_not_yet_runnable(why)
        )));
    }

    // ART-186, and it is **before** anything is copied, unpacked or written:
    // `perform` makes the tree copy, `install` builds two scratch volumes, and
    // all of it happens after this line. A BoingBag 2 run against a tree
    // BoingBag 1 never touched produces a system that boots and is quietly
    // wrong, which is the failure this project already shipped once.
    //
    // Here rather than in the job, because the *preview* has to refuse it too:
    // a screen that offered a confirm button for a run that cannot happen
    // would be a preview of nothing.
    //
    // It refuses a tree ART cannot account for at all as well, and that half
    // is load-bearing rather than tidy (fix round 1): `record_if_succeeded`
    // below cannot write into a tree with no `distribution.json`, so a run
    // allowed against one would have reached the emulator, **worked**, and
    // then failed at the recording — leaving the copy unpromoted and the user
    // told the install failed after it had succeeded. Both halves go through
    // one read in `chain`, so they cannot say different things again.
    chain::refuse_unless_installable(&package, &request.tree)?;

    let volume = system_volume(request.system_volume.as_deref())?;

    // The drawer inside the **wrapper**, not inside the tree (ART-185). The
    // recipe's `media` is that drawer as shipped data — the archive's
    // top-level directory, which is what `scan::package_for` already requires
    // an archive to carry to be this package's at all — so a caller who says
    // nothing gets the right answer instead of the volume's root, which was
    // right for no real package.
    let package_dir = request
        .package_dir
        .as_deref()
        .unwrap_or(package.media.as_str())
        .trim();
    let package_dir = (!package_dir.is_empty()).then(|| package_dir.to_string());

    // `PACKAGE_VOLUME` and not the system volume: the wrapper is mounted as
    // its own volume, because a BoingBag cannot be placed into the tree.
    let location = package_location(PACKAGE_VOLUME, package_dir.as_deref())?;

    let installer_in_package = installer.program.trim().to_string();
    let program = join_amigados(&location, &installer_in_package);
    one_token("an installer path", &program)?;

    let mut args = installer.args.clone();
    for arg in &args {
        one_token("an installer argument", arg)?;
    }
    // The target volume. See the module documentation: the package's own
    // `Install` script passes it, and the recipe deliberately does not carry
    // it because it is a fact about the run.
    args.push(format!("{volume}:"));

    let plan = PlannedRun {
        package_id: package.id.clone(),
        system_volume: volume,
        program,
        args,
        working_directory: Some(location),
    };

    workvol::startup_sequence(&plan)?;

    // ART-277 re-review, L3/L4: release-scoped and self-excluding, built
    // once here (before the refusal below, which now also reads it — I11)
    // rather than recomputed (and re-drifting) in `install`.
    let catalogue = known_packages_for(&package);

    // ART-200/ART-201: is the file in the package's own field actually the
    // package? Asked here, so the preview refuses it too and the answer names
    // that package when ART can tell (I11).
    refuse_wrong_package_archive(&package.media, &request.package_archive, &catalogue)?;

    Ok(Composed {
        plan,
        package_name: package.name,
        package_dir,
        installer_in_package,
        catalogue,
    })
}

/// Every *other* package ART's catalogue ships an Amiga-side installer for
/// and that shares a release with `selected`, as the small record
/// `core/amigainstall` is allowed to read (ART-277) — never the whole
/// recipe. An unreadable catalogue answers empty rather than refusing: this
/// is only ever used to make a refusal *friendlier*, never to decide whether
/// a run may happen, so losing it costs a nicety and not a correctness
/// guarantee.
///
/// **Release-scoped and self-excluding (ART-277 re-review, L3/L4).** The
/// first round built this from every shipped package regardless of release
/// and did not remove `selected` — harmless only because `overlay_
/// mismatch_sentence`'s own `layout.drawer` check runs first, but a doc
/// comment claiming this discipline while nothing here kept it was found by
/// reading the two, not by a test. `classify_top_level`
/// (`amigainstall_classify_archive`) already answers over exactly this
/// scope — `package::packages_for(&release)`, minus the selected id — so
/// this gives the core refusal the same one.
fn known_packages_for(selected: &package::Package) -> Vec<packagevol::KnownPackage> {
    known_packages_from(selected, &package::packages().unwrap_or_default())
}

/// [`known_packages_for`]'s own filter, parameterised over the candidate
/// list so a test can hand it a package from a release the shipped catalogue
/// does not otherwise have one for (ART-277 re-review, L3: a test that a
/// package of another release is never named) without inventing shipped
/// JSON to do it.
fn known_packages_from(
    selected: &package::Package,
    all: &[package::Package],
) -> Vec<packagevol::KnownPackage> {
    all.iter()
        .filter(|p| p.id != selected.id)
        .filter(|p| p.amiga_installer.is_some())
        .filter(|p| p.releases.iter().any(|r| selected.releases.contains(r)))
        .cloned()
        .map(known_package)
        .collect()
}

/// One `package::Package`, translated into `core/amigainstall`'s own record
/// — the single place that does it, so [`known_packages_for`] (the core
/// refusal's catalogue) and `classify_top_level`'s release-scoped list
/// (ART-277 review, Major 2) build the same shape.
fn known_package(p: package::Package) -> packagevol::KnownPackage {
    packagevol::KnownPackage {
        id: p.id,
        name: p.name,
        media: p.media,
    }
}

/// What a chosen archive is, judged **before** it goes into a request —
/// ART-277's second cause. `AmigaInstallPanel` calls this the moment a file
/// picker returns, so a BoingBag 2 archive supplied while BoingBag 1 is still
/// selected is named on screen immediately, rather than discovered as a
/// refusal after the round trip through [`compose`].
///
/// **Scoped to `release`** (ART-277 review, Major 2) — the exact list the
/// radio offers (`osinstall_packages`), never every shipped recipe: a hint
/// naming a package a different release's build cannot even reach would be
/// unactionable in a new way.
///
/// Read-only and lenient: nothing is unpacked (the archive's listing alone is
/// read, once — [`packagevol::archive_listing`] never opens the encrypted
/// payload a second archive might carry, and never opens the archive twice
/// for the two questions this asks of it), and an archive this cannot make
/// sense of — missing, unreadable, not carrying a single top-level directory
/// — answers `"unknown"` rather than refusing. A query the panel asks on
/// every file pick must not turn "I could not tell" into a hard error the
/// user cannot get past.
#[tauri::command]
pub fn amigainstall_classify_archive(
    path: PathBuf,
    package_id: String,
    release: String,
) -> AppResult<ArchiveClassification> {
    let (top_level, identity) = packagevol::archive_listing(&path).unwrap_or_default();

    // Parsed once (review finding 7): `selected` is looked up in the same
    // release-scoped list the ambiguity/other-package scan below reads,
    // rather than `by_id` (every release) plus `packages()` (every release,
    // again) each re-parsing the shipped JSON.
    let release_packages = package::packages_for(&release).unwrap_or_default();
    let selected = release_packages.iter().find(|p| p.id == package_id.trim());

    let expected_media = selected.map(|p| p.media.clone());

    let Classified { kind, shared_by } = match identity {
        None => Classified {
            kind: "unknown".to_string(),
            shared_by: Vec::new(),
        },
        Some(top) => classify_top_level(&top, selected, &release_packages, Some(&path)),
    };

    Ok(ArchiveClassification {
        kind,
        top_level,
        expected_media,
        shared_by,
    })
}

/// [`classify_top_level`]'s answer: a `kind` string plus the one thing it
/// cannot embed as an id list and stay a plain string — every package id
/// that shares an identity, for the `shared-artefact` shape.
struct Classified {
    kind: String,
    /// Every release package that reads this exact identity, when `kind` is
    /// `` `shared-artefact:<media>` `` — empty for every other kind. The
    /// panel resolves these to display names to say *both* packages'
    /// (ART-277 re-review, L5).
    shared_by: Vec<String>,
}

impl Classified {
    fn plain(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            shared_by: Vec::new(),
        }
    }
}

/// [`amigainstall_classify_archive`]'s own decision, parameterised so it can
/// be tested without a real archive on disk. `release_packages` is the exact
/// list the radio offers' recipes come from (`package::packages_for`), not
/// every shipped package — see the command's own doc comment.
///
/// **ART-277 review, Major 2 and Medium 1.** The first round scanned every
/// shipped package regardless of release, compared `media` only, and picked
/// whichever matching package happened to come first when more than one
/// recipe declared the same `media` — `locale-39` and `locale-39-turkish`
/// both declare `"Locale3.9"` on purpose (`recipes/packages/locale-39-turkish.json`'s
/// own comment). That produced a hint naming a package **not on this
/// screen's radio at all** (neither Locale package is Amiga-installable) and
/// then hard-disabled Run over an instruction the user cannot act on — the
/// exact shared-medium ambiguity ART-276 was filed for the night before,
/// arriving through a different door.
///
/// So: every match against this archive's identity — this package's own
/// drawer, or (Medium 1) one of its own declared overlays' drawer — is
/// collected first, and **more than one match is never resolved by picking
/// one**.
///
/// **ART-277 re-review, L5: two different causes, two different kinds.**
/// The first round folded "two or more packages share this identity" and
/// "the one package that does is not Amiga-installable" into the same
/// `other-artefact` kind and the same sentence — *"which the Packages step
/// places from Windows"*, which is only true of the second cause. Today
/// both causes happen to be the same two non-installable `Locale3.9`
/// packages, so the sentence read true by accident of the shipped data; an
/// Amiga-installable package sharing a `media` with another would have made
/// it a confident, wrong claim about which step handles a file. Now:
/// `` `shared-artefact:<top>` `` for the ambiguous case (names both packages,
/// says nothing about which step runs them — the panel does not know that
/// yet either), `` `other-artefact:<top>` `` only for a single match this
/// screen's radio does not offer at all (naming it by id would produce
/// "select `<pkg>`" for a package that is not selectable here).
fn classify_top_level(
    top: &str,
    selected: Option<&package::Package>,
    release_packages: &[package::Package],
    archive: Option<&std::path::Path>,
) -> Classified {
    if let Some(selected) = selected {
        match packagevol::archive_is(&selected.media, top) {
            packagevol::ArchiveIs::ThePackage => return Classified::plain("the-package"),
            packagevol::ArchiveIs::Neither => {}
        }
    }

    // ART-277 re-review, I9: one implementation of "which archive is this",
    // not two — `archive_is` already answers `ThePackage`/`Neither` for the
    // *selected* package above, and the same question is asked of every other
    // catalogue candidate rather than re-derived by hand.
    let matches: Vec<&package::Package> = release_packages
        .iter()
        .filter(|pkg| Some(pkg.id.as_str()) != selected.map(|s| s.id.as_str()))
        .filter(|pkg| packagevol::archive_is(&pkg.media, top) == packagevol::ArchiveIs::ThePackage)
        .collect();

    // **Narrowed by what is inside the archive before it is called
    // ambiguous** (round 3). Two of the shipped 3.9 packages now share the
    // top level `BoingBag3.9-2` — `BoingBag39-2.lha` and
    // `BoingBag39-2-Contribution.lha` — which is exactly the collision
    // `Package::distinguished_by` was measured for. Stopping at the top
    // level here would tell somebody holding the plain BoingBag 2 archive
    // that ART cannot say which package it is, about a file that carries
    // `AmigaOS-Update` and says so itself.
    //
    // Only when a path is in hand (the command's case), and only to *drop*
    // candidates whose declared path this archive does not carry: a package
    // that declares no distinguisher survives, and narrowing never picks a
    // winner — a still-ambiguous list is still ambiguous.
    let matches: Vec<&package::Package> = match archive {
        None => matches,
        Some(path) => {
            let narrowed: Vec<&package::Package> = matches
                .iter()
                .filter(|pkg| match pkg.distinguished_by.as_deref() {
                    Some(inner) => crate::core::osinstall::scan::archive_carries(path, inner),
                    None => true,
                })
                .copied()
                .collect();
            // Narrowing to nothing is never an improvement: an unreadable
            // archive makes `archive_carries` answer `false` for every
            // candidate, and answering "unknown" about a file two packages
            // do claim would be worse than saying they both do.
            match narrowed.is_empty() {
                true => matches,
                false => narrowed,
            }
        }
    };

    // More than one release package claims this exact top level: never pick
    // one arbitrarily (ART-276's own trap, arriving here too) — named as
    // what it is, and named to the user as *both* packages (L5).
    if matches.len() > 1 {
        return Classified {
            kind: format!("shared-artefact:{top}"),
            shared_by: matches.into_iter().map(|pkg| pkg.id.clone()).collect(),
        };
    }

    match matches.into_iter().next() {
        Some(pkg) if pkg.amiga_installer.is_some() => {
            Classified::plain(format!("another-package:{}", pkg.id))
        }
        // A real, single match — just not one this screen's radio offers at
        // all (every Locale package, today). Naming it by id would produce
        // "select <pkg>" for a package that is not selectable here.
        Some(_) => Classified::plain(format!("other-artefact:{top}")),
        None => Classified::plain("unknown"),
    }
}

/// [`amigainstall_classify_archive`]'s answer: what the archive is, and what
/// it actually carries at its top level — the latter so a refusal or a hint
/// can say what it held even when ART recognises none of it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveClassification {
    /// `"the-package"`, `` "another-package:<id>" ``,
    /// `` "shared-artefact:<top-level-name>" `` (ART-277 re-review, L5 — two
    /// or more release packages declare this exact identity; `shared_by`
    /// names which), `` "other-artefact:<top-level-name>" `` (a single, real
    /// match, but not a package this screen can select — not
    /// Amiga-installable), or `"unknown"`.
    ///
    /// The two update-archive kinds went with the overlay machinery on
    /// 2026-09-08: there is one archive field now, so there is no second field
    /// for an archive to belong in instead.
    pub kind: String,
    pub top_level: Vec<String>,
    /// The **selected** package's own `media` — what the package field
    /// itself expects — so a panel that cannot read a recipe's `media`
    /// directly (`PackageSummary` never carries it) can still say what this
    /// field wants when the archive turns out to be something else's.
    /// `None` when the selected id is not a package this release ships.
    pub expected_media: Option<String>,
    /// Every release package that reads this exact identity, when `kind` is
    /// `` `shared-artefact:<top>` `` (ART-277 re-review, L5) — empty for
    /// every other kind. Ids, not names: `PackageSummary`'s own list is
    /// where the panel resolves a display name, the same way it already
    /// does for `another-package`'s id.
    pub shared_by: Vec<String>,
}

/// Ask the supplied archive what it carries, **before anything is unpacked**.
///
/// **ART-201.** `amiga_install_preview` starts no process and unpacks nothing,
/// so with the wrong file in the package's own field it used to render a
/// confident summary — package, tree, emulator, disc, machine — for a run
/// `packagevol::unpack` would refuse the moment the job started. The owner's
/// operation log carried **seven** identical failed runs. A preview that
/// describes a run that cannot happen is §92's PREVIEW step not covering the
/// input the run uses, and it is this project's own named defect class: a
/// confident, wrong sentence.
///
/// This sits in `compose`, which both the preview and the run go through, so
/// the two cannot disagree about what is acceptable.
///
/// **It only refuses what it is sure of.** A path that is not there is left to
/// the panel's own missing-archive blocker, and an archive `ArchiveSource`
/// cannot open at all is left to `unpack` — refusing here on a file this
/// reader merely does not understand would turn a working run into a false
/// refusal, which is worse than the message this exists to improve.
///
/// **One slot** (2026-09-08). It used to walk a list — the first entry the
/// package's own archive, every one after it an overlay medium that must
/// *not* be — because ART-186's UAE fix needed a second archive. There is one
/// field now, and the only thing it can be wrong about is not being the
/// package.
fn refuse_wrong_package_archive(
    media: &str,
    archive: &Path,
    catalogue: &[packagevol::KnownPackage],
) -> CoreResult<()> {
    if !archive.is_file() {
        return Ok(());
    }
    let Ok(source) = ArchiveSource::open(archive) else {
        return Ok(());
    };
    let holds = MediaSource::volume_name(&source).to_string();
    if packagevol::archive_is(media, &holds) == packagevol::ArchiveIs::ThePackage {
        return Ok(());
    }
    Err(CoreError::InvalidInput(packagevol::wrong_archive_sentence(
        archive, media, &holds, catalogue,
    )))
}

/// The machine the installer runs on.
pub(crate) fn profile_for(id: Option<&str>) -> CoreResult<AmigaProfile> {
    let wanted = id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_PROFILE_ID);
    AmigaProfile::all_presets()
        .into_iter()
        .find(|p| p.id == wanted)
        .ok_or_else(|| {
            CoreError::InvalidInput(format!("ART has no machine profile called '{wanted}'"))
        })
}

// ---------------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------------

/// Copy the tree, run something against the copy, and decide what happens to
/// it — the whole of the §92 pipeline this command owns.
///
/// `run` is a parameter for the same reason `run_with` takes a launcher and a
/// clock: **no test in this file may open an emulator window on the owner's
/// desktop**, and every ending — the four outcomes, a cancellation, and an
/// error part way — has to be reachable without one.
///
/// Cancellation is checked once here, *before* the copy is made, which is the
/// cheapest place to stop: nothing has been written, so there is nothing to
/// undo. `stage_with` checks again between whole files, and `run` between
/// whole polls. None of the three is inside a write.
fn perform(
    tree: &Path,
    sink: &dyn ProgressSink,
    run: impl FnOnce(&Path, &dyn ProgressSink) -> CoreResult<RunOutcome>,
) -> CoreResult<(RunOutcome, SettlementReport)> {
    if sink.is_cancelled() {
        return Err(CoreError::Cancelled);
    }

    let staged = stage_with(tree, sink)?;

    match run(staged.copy_path(), sink) {
        Ok(outcome) => {
            let settlement = settle(staged, &outcome)?;
            Ok((outcome, SettlementReport::from(settlement)))
        }
        // The one path where the copy does not survive (design §4). `Staged`
        // has no `Drop`, so nothing else would remove it — and a cancelled
        // run that left a half-installed tree beside the user's own would be
        // ART leaving litter nobody asked for.
        //
        // A discard that itself fails must not turn a cancellation into a
        // failure: the user asked for this, and the job bar must not go red
        // for it. It is reported instead, so the copy is never left on disk
        // with nobody saying so.
        Err(CoreError::Cancelled) => {
            // **Both endings are reported** (ART-187). Only the failure was,
            // so a discard that worked left the panel showing whatever phase
            // the run last announced -- "Unpacking...", "Copying..." -- under
            // a badge that correctly claimed nothing about the copy. Stale
            // rather than false, which is why it shipped; but a screen that
            // goes on saying something after it stopped being so is the
            // family this whole round was about.
            //
            // The core says it because the core is what knows. The screen
            // asserting a discard over the top of ART's own "could not be
            // removed" is the sibling defect, and it was worse.
            let original = staged.original_path().display().to_string();
            match staged.discard() {
                Ok(()) => sink.report(
                    0,
                    None,
                    &format!("The cancelled run's copy was removed; '{original}' was not touched"),
                ),
                Err(err) => sink.report(
                    0,
                    None,
                    &format!("The cancelled run's copy could not be removed: {err}"),
                ),
            }
            Err(CoreError::Cancelled)
        }
        // Not one of the four endings and not a cancellation: something went
        // wrong while the run was under way. The copy stays — the emulator
        // may have changed it, and that is evidence — and the original was
        // never opened for writing at all. The error keeps its own code; what
        // is added is where to look.
        Err(err) => {
            sink.report(
                0,
                None,
                &format!(
                    "'{}' was not touched; the copy ART installed into is at '{}'",
                    staged.original_path().display(),
                    staged.copy_path().display()
                ),
            );
            Err(err)
        }
    }
}

/// Build ART's work volume, unpack the package, then [`perform`] the run
/// against a copy.
///
/// **Both scratch volumes are built before the copy**, so nothing that goes
/// wrong with either can leave a half-installed tree behind — and the unpack
/// is the step most likely to refuse, because it is where the user's own
/// choice of archive is checked against the package they ticked. Each lives in
/// a scratch directory that removes itself: one holds a generated script plus
/// the Amiga's one-word answer, already read by the time this returns; the
/// other holds a copy of the package's own files, which the user still has.
#[allow(clippy::too_many_arguments)]
fn install(
    composed: &Composed,
    tree: &Path,
    package_archive: &Path,
    profile: &AmigaProfile,
    kickstart: &Path,
    emulator: &Path,
    scratch_root: &Path,
    sink: &dyn ProgressSink,
) -> CoreResult<(RunOutcome, SettlementReport)> {
    let plan = &composed.plan;
    let work = Scratch::in_dir(scratch_root)?;
    workvol::build(work.path(), plan)?;

    // ART-185. Without this the installer the whole round exists to run is on
    // no volume the emulator can see.
    sink.report(0, None, "Unpacking the package's own files");
    let package = Scratch::in_dir(scratch_root)?;
    // ART-277: named so a wrong-package refusal can say whose archive it
    // really is, not only what the selected package expected. `compose`
    // already built this release-scoped and self-excluding (L3/L4) — used
    // as `compose` left it, not recomputed here.
    let unpacked = packagevol::unpack(
        package_archive,
        package.path(),
        &packagevol::Layout {
            drawer: composed.package_dir.as_deref(),
            installer: &composed.installer_in_package,
            package_name: &composed.package_name,
            catalogue: &composed.catalogue,
        },
        sink,
    )?;
    for refusal in &unpacked.refused {
        sink.report(
            0,
            None,
            &format!("The package's archive carries an entry ART would not write — {refusal}"),
        );
    }
    if let Some(version) = &unpacked.installer_version {
        sink.report(0, None, &format!("The installer states {version}"));
    }

    // The real launcher is built here, not in `core/`: constructing one is a
    // process-spawning decision, and `core/amigainstall::run` may not make it
    // (ART-274) — it takes an `EmulatorLauncher` directly instead.
    let launcher = WinUaeLauncher::new(emulator, scratch_root);
    let (outcome, settlement) = perform(tree, sink, |copy, sink| {
        let request = RunRequest {
            plan,
            work_volume_dir: work.path(),
            tree_dir: copy,
            package_volume_dir: unpacked.root.as_path(),
            scratch_root,
            profile,
            kickstart_path: kickstart,
            cd_image: None,
            limits: RunLimits::default(),
        };
        let outcome = run_with(&request, &launcher, &RealClock::new(), sink)?;
        record_if_succeeded(copy, plan, &outcome)?;
        Ok(outcome)
    })?;

    Ok((outcome, settlement))
}

/// ART-186's other half: a run that says it worked writes that into the
/// copy's own `distribution.json`.
///
/// Without it the prerequisite refusal `compose` now makes could never be
/// satisfied — a BoingBag cannot be placed from the host at all, so if a
/// successful Amiga-side run left no trace, BoingBag 2 would be refused for
/// ever on a tree that really did have BoingBag 1.
///
/// Written into the **copy**, before [`settle`] decides whether the copy
/// becomes the tree: that makes the record and the promotion one decision
/// rather than two, and a record that could not be written takes
/// [`perform`]'s existing failure path — the copy is kept, the original was
/// never opened.
///
/// **A named function rather than three lines inside the closure**, because
/// `install` cannot be driven to a successful outcome in a test without
/// opening an emulator window on the owner's desktop, and a test that
/// re-wrote this condition in its own closure would be testing its own copy
/// of it. This is the real one, and the tests call it for all four endings.
fn record_if_succeeded(copy: &Path, plan: &PlannedRun, outcome: &RunOutcome) -> CoreResult<()> {
    if matches!(outcome, RunOutcome::Succeeded) {
        chain::record_amiga_install(copy, &plan.package_id, &command_line_of(plan))?;
    }
    Ok(())
}

/// The AmigaDOS line a plan runs as, program and arguments joined by spaces —
/// what the generated script carried, recorded in the tree's own manifest and
/// in the operation log.
fn command_line_of(plan: &PlannedRun) -> String {
    std::iter::once(plan.program.clone())
        .chain(plan.args.iter().cloned())
        .collect::<Vec<String>>()
        .join(" ")
}

/// The word the log records for an ending. English, like every other
/// `CoreError` message (ART-060) — the user's sentence is the screen's.
fn ending_of(outcome: &RunOutcome) -> &'static str {
    match outcome {
        RunOutcome::Succeeded => "succeeded",
        RunOutcome::Failed => "failed",
        RunOutcome::TimedOut { .. } => "timed out",
        RunOutcome::EmulatorClosed { .. } => "the emulator was closed",
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// What running this package's own installer would do — §92's PREVIEW.
///
/// Reads recipe data and asks three `is_file` questions. It starts no
/// process, unpacks nothing, copies nothing, and writes nothing.
#[tauri::command]
pub fn amiga_install_preview(
    request: AmigaInstallRequest,
    winuae_path: Option<String>,
) -> AppResult<AmigaInstallPreview> {
    let composed = compose(&request)?;
    Ok(preview_over(request, composed, winuae_path.as_deref())?)
}

/// [`amiga_install_preview`]'s own derivation, over a run already composed —
/// the same split [`compose_over`] makes, and for the same reason.
fn preview_over(
    request: AmigaInstallRequest,
    composed: Composed,
    winuae_path: Option<&str>,
) -> CoreResult<AmigaInstallPreview> {
    let profile = profile_for(request.profile.as_deref())?;

    Ok(AmigaInstallPreview {
        package_id: composed.plan.package_id,
        package_name: composed.package_name,
        system_volume: composed.plan.system_volume,
        working_directory: composed.plan.working_directory,
        program: composed.plan.program,
        args: composed.plan.args,
        work_volume: WORK_VOLUME.to_string(),
        package_volume: PACKAGE_VOLUME.to_string(),
        package_archive_present: request.package_archive.is_file(),
        package_archive: request.package_archive,
        package_dir: composed.package_dir,
        result_file: RESULT_FILE.to_string(),
        deadline_seconds: RunLimits::default().deadline.as_secs(),
        kickstart_present: request.kickstart.is_file(),
        kickstart: request.kickstart,
        emulator: detect_winuae(winuae_path).executable_path,
        profile_id: profile.id,
        profile_name: profile.name,
        tree: request.tree,
    })
}

/// Run the package's own installer inside an emulator, against a copy of the
/// tree. Returns a job id (§54); the answer arrives on
/// [`AMIGA_INSTALL_EVENT`].
///
/// Everything that can be refused is refused **here**, before the job starts,
/// so a bad package id or a missing emulator is a sentence on the screen
/// rather than a job that goes red a moment later.
#[tauri::command]
pub fn amiga_install_run(
    request: AmigaInstallRequest,
    winuae_path: Option<String>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<JobId> {
    let composed = compose(&request)?;
    let profile = profile_for(request.profile.as_deref())?;
    let emulator = detect_winuae(winuae_path.as_deref())
        .executable_path
        .ok_or_else(|| {
            CoreError::InvalidInput(
                "WinUAE was not found in a standard install location — set its path in Settings"
                    .to_string(),
            )
        })?;

    let plan = composed.plan.clone();
    let tree = request.tree.clone();
    let package_archive = request.package_archive.clone();
    let kickstart = request.kickstart.clone();
    let emulator = PathBuf::from(emulator);
    let log_path = oplog.path().to_path_buf();
    let emit_app = app.clone();
    let title = format!("Installing {} on the Amiga", composed.package_name);

    let for_log = tree.display().to_string();
    let command_line = command_line_of(&plan);
    let package_id = plan.package_id.clone();

    // Resolved here rather than inside the job: a scratch root that has
    // gone away is the user's to fix, and they should hear it from the
    // button they pressed (ART-196).
    let scratch_root = crate::scratch::root()?;

    let id = spawn_job(
        &app,
        Arc::clone(&registry),
        &title,
        move |job_id, progress| {
            let result = install(
                &composed,
                &tree,
                &package_archive,
                &profile,
                &kickstart,
                &emulator,
                &scratch_root,
                progress,
            );

            // §53. Best-effort, and never able to fail the operation it
            // describes.
            let record = user_operation("Run a package's own installer on the Amiga")
                .source(package_id)
                .destination(&for_log)
                .detail("Command", command_line)
                .detail("Machine", profile.id.clone());
            let record = match &result {
                Ok((outcome, settlement)) => {
                    let record = record.detail("Ending", ending_of(outcome));
                    let record = match settlement {
                        SettlementReport::Promoted { left_behind, .. } => match left_behind {
                            Some(path) => record.detail("Left behind", path.display().to_string()),
                            None => record,
                        },
                        SettlementReport::Kept { copy, .. } => {
                            record.detail("Copy kept at", copy.display().to_string())
                        }
                    };
                    // The result file is the Amiga's own report and the only
                    // check there is: `verified(true)` when it said the
                    // install worked, `verified(false)` for the three endings
                    // where it did not — never `success()`, which would read
                    // as "ART looked and found nothing wrong" about a run ART
                    // cannot inspect (§89).
                    record.outcome(OperationOutcome::verified(matches!(
                        outcome,
                        RunOutcome::Succeeded
                    )))
                }
                Err(err) => record.failed(err),
            };
            write_to_path(&log_path, &record);

            let (outcome, settlement) = result?;
            let _ = emit_app.emit(
                AMIGA_INSTALL_EVENT,
                AmigaInstallResult {
                    job_id,
                    outcome,
                    settlement,
                },
            );
            Ok(())
        },
    );

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;

    use crate::core::amigainstall::stage::STAGED_SUFFIX;
    use crate::core::ScratchDir;

    /// A sink that can be cancelled from the start and counts what it was
    /// told, so a test can assert *that a message naming the copy was sent*
    /// rather than only that a path exists.
    #[derive(Default)]
    struct Sink {
        cancelled: AtomicBool,
        messages: std::sync::Mutex<Vec<String>>,
    }

    impl Sink {
        fn cancelled() -> Self {
            Self {
                cancelled: AtomicBool::new(true),
                messages: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn said(&self, needle: &str) -> bool {
            self.messages
                .lock()
                .unwrap()
                .iter()
                .any(|m| m.contains(needle))
        }
    }

    impl ProgressSink for Sink {
        fn report(&self, _done: u64, _total: Option<u64>, message: &str) {
            self.messages.lock().unwrap().push(message.to_string());
        }
        fn is_cancelled(&self) -> bool {
            self.cancelled.load(Ordering::Relaxed)
        }
    }

    /// [`compose`] with the selected package given the Amiga-side installer
    /// the two BoingBags used to declare, when its own recipe declares none.
    ///
    /// **Why a test needs one at all.** The emulator route for BoingBag 3.9-1
    /// and 3.9-2 was removed on 2026-09-08, so the only shipped recipe that
    /// still declares an `amiga_installer` is `boingbags-39-3-4` — and that
    /// one is `not_yet_runnable`, which `compose` refuses before it composes
    /// anything. Every guard in this file would then be unreachable.
    ///
    /// It supplies a *declaration*, not a code path: `compose_over` is the
    /// production function, and the two lines below are exactly what a recipe
    /// writes. A package that already declares one is passed through
    /// untouched, so the `not_yet_runnable` refusal is still reached by the
    /// test that is about it.
    fn compose_runnable(request: &AmigaInstallRequest) -> CoreResult<Composed> {
        let mut package = package::by_id(request.package_id.trim())?;
        if package.amiga_installer.is_none() {
            package.amiga_installer = Some(package::AmigaInstaller {
                program: "C/Updater".to_string(),
                args: vec!["AmigaOS-Update".to_string()],
                not_yet_runnable: None,
            });
        }
        compose_over(request, package)
    }

    /// [`amiga_install_preview`] over [`compose_runnable`]'s composition — the
    /// production `preview_over` with the same one declaration supplied. See
    /// [`compose_runnable`] for why a test needs it.
    fn preview_runnable(
        request: AmigaInstallRequest,
        winuae_path: Option<&str>,
    ) -> CoreResult<AmigaInstallPreview> {
        let composed = compose_runnable(&request)?;
        preview_over(request, composed, winuae_path)
    }

    fn request(tree: &Path) -> AmigaInstallRequest {
        AmigaInstallRequest {
            tree: tree.to_path_buf(),
            package_id: "boingbag-39-1".to_string(),
            system_volume: None,
            package_archive: PathBuf::from("BoingBag39-1.lha"),
            package_dir: None,
            kickstart: PathBuf::from("kick.rom"),
            profile: None,
        }
    }

    /// [`request`] for a package other than BoingBag 3.9-1.
    fn request_for(tree: &Path, package_id: &str) -> AmigaInstallRequest {
        let mut req = request(tree);
        req.package_id = package_id.to_string();
        req
    }

    /// A wrapper shaped like the owner's own `BoingBag39-1.lha`, measured with
    /// 7-Zip 26.02 on 2026-08-21: **an icon file beside the drawer at the top
    /// level, no directory entries at all**, the `Updater` under `C/`, and the
    /// still-encrypted payload blob beside it.
    ///
    /// `core::amigainstall::packagevol` has the same fixture and the same
    /// reason for it; this one exists because the command layer is where the
    /// recipe's `media` is turned into that drawer name, and a fixture that
    /// did not carry the real drawer would let a wrong `media` pass.
    fn boingbag_lha() -> Vec<u8> {
        crate::core::lha::tests::make_lha_with(&[
            ("BoingBag3.9-1.info", b"icon"),
            ("BoingBag3.9-1/AmigaOS-Update", b"PK encrypted"),
            ("BoingBag3.9-1/C/Updater", updater(45, 15)),
            ("BoingBag3.9-1/Install", b"; the package's own script"),
        ])
    }

    /// An `Updater` that states its own version the way the real one does.
    ///
    /// ART-186: BoingBag 3.9-1's recipe declares a minimum of 45.15, so a
    /// fixture whose `Updater` is the bytes `b"the updater"` is not a program
    /// ART would launch at all — it says nothing about itself, and ART does
    /// not launch what it cannot identify. The marker is placed after some
    /// leading bytes because a real one is: 505 bytes into the owner's own
    /// `BoingBag39-1.lha` `Updater`, 537 into the other two.
    fn updater(version: u32, revision: u32) -> &'static [u8] {
        // `make_lha_with` takes `&'static [u8]`, and the two builds this file
        // needs are known at compile time.
        match (version, revision) {
            (45, 13) => b"\x00\x00\x03\xf3 hunk header \x00$VER: Updater 45.13 (3.4.2001)\x00",
            (45, 15) => b"\x00\x00\x03\xf3 hunk header \x00$VER: Updater 45.15 (17.4.2001)\x00",
            _ => unreachable!("only the two builds ART-186 measured"),
        }
    }

    /// A tree with something in it, so the copy is a real copy.
    fn tree_in(scratch: &ScratchDir) -> PathBuf {
        tree_with_manifest(scratch, &["workbench-base"])
    }

    /// A folder with the same files and **no** `distribution.json` — not a
    /// distribution tree, and since fix round 1 not something an Amiga-side
    /// install may run against, because a success against it could not be
    /// recorded. Two tests use it and both are about that refusal.
    fn tree_without_manifest(scratch: &ScratchDir) -> PathBuf {
        let tree = scratch.join("Workbench3.9");
        std::fs::create_dir_all(tree.join("Libs")).unwrap();
        std::fs::write(tree.join("Libs/version.library"), b"the original").unwrap();
        tree
    }

    /// A distribution tree whose `distribution.json` names `components`.
    ///
    /// **The manifest is always written.** A fixture without one refuses for
    /// its own, different reason — "ART cannot say what this tree has" — and a
    /// prerequisite test built on it would pass with the prerequisite check
    /// deleted. `a_tree_with_no_manifest_is_a_different_refusal` in
    /// `core::osinstall::chain` pins that other sentence.
    fn tree_with_manifest(scratch: &ScratchDir, components: &[&str]) -> PathBuf {
        use crate::core::osinstall::apply::{DistributionManifest, FileRecord};

        let tree = tree_without_manifest(scratch);
        let manifest = DistributionManifest {
            release: "amigaos-3.9".into(),
            built_from: Vec::new(),
            files: components
                .iter()
                .map(|c| FileRecord {
                    path: "Libs/version.library".into(),
                    component: (*c).to_string(),
                    media: "Workbench3.9".into(),
                    sha256: String::new(),
                    bytes: 12,
                    protection: None,
                    overwrote: None,
                    host_path: None,
                })
                .collect(),
            paired_rom: None,
            amiga_installed: Vec::new(),
            layers: Vec::new(),
        };
        std::fs::write(
            tree.join("distribution.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        tree
    }

    /// Everything beside the tree whose name marks it as ART's copy.
    fn copies_beside(tree: &Path) -> Vec<PathBuf> {
        let parent = tree.parent().expect("a parent");
        let mut found: Vec<PathBuf> = std::fs::read_dir(parent)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.contains(STAGED_SUFFIX))
            })
            .collect();
        found.sort();
        found
    }

    // -- composing -------------------------------------------------------

    /// The whole composition, pinned against the shipped recipe.
    ///
    /// `boingbag-39-1.json` declares `C/Updater` and one argument,
    /// `AmigaOS-Update`, and refuses to name a volume at all. Everything that
    /// turns that into a runnable line happens here, so this is where it is
    /// asserted: the drawer, the whole path, the target argument the
    /// package's own `Install` script passes, and the directory the installer
    /// runs from.
    ///
    /// **The program and the target come from two different volumes, and that
    /// is ART-185.** The installer is on `ARTPkg:`, where ART unpacked the
    /// wrapper; the volume being installed *into* is `DH0:`, the tree.
    /// Composing both from the tree named a program that was never there —
    /// see `the_installer_is_reached_through_the_package_volume_and_not_the_tree`.
    #[test]
    fn a_recipe_declaration_becomes_a_whole_amigados_command() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "compose-line");
        let tree = tree_in(&scratch);
        let composed = compose_runnable(&request(&tree)).unwrap();

        assert_eq!(composed.plan.system_volume, "DH0");
        assert_eq!(composed.plan.program, "ARTPkg:BoingBag3.9-1/C/Updater");
        assert_eq!(composed.plan.args, vec!["AmigaOS-Update", "DH0:"]);
        assert_eq!(composed.package_dir.as_deref(), Some("BoingBag3.9-1"));
        assert_eq!(composed.installer_in_package, "C/Updater");
        assert_eq!(
            composed.plan.working_directory.as_deref(),
            Some("ARTPkg:BoingBag3.9-1"),
            "the installer runs from the package's own drawer, because its arguments are \
             relative to it"
        );
        assert_eq!(composed.package_name, "BoingBag 3.9-1");
    }

    /// ART-185, as one assertion: the installer is reached through the volume
    /// the **package** was mounted under, and the tree's volume appears only
    /// as the thing being installed into.
    ///
    /// Put the defect back — compose the location from `volume` rather than
    /// `PACKAGE_VOLUME` — and the second assertion fails, because a BoingBag's
    /// `Updater` is not in the tree and cannot be put there (ART-166).
    #[test]
    fn the_installer_is_reached_through_the_package_volume_and_not_the_tree() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "compose-volume");
        let tree = tree_in(&scratch);
        let composed = compose_runnable(&request(&tree)).unwrap();

        assert!(
            composed
                .plan
                .program
                .starts_with(&format!("{PACKAGE_VOLUME}:")),
            "the installer lives on the package volume: {}",
            composed.plan.program
        );
        assert!(
            !composed
                .plan
                .program
                .starts_with(&format!("{}:", composed.plan.system_volume)),
            "and never on the tree, which cannot carry it: {}",
            composed.plan.program
        );
        assert_eq!(
            composed.plan.args.last().map(String::as_str),
            Some("DH0:"),
            "the tree is what is installed *into*, and only that"
        );
    }

    /// A caller who says nothing gets the package's own recipe `media` — the
    /// archive's top-level drawer, shipped data — rather than the wrapper's
    /// root, which is right for none of the owner's real archives.
    #[test]
    fn no_drawer_named_takes_the_recipe_media_rather_than_the_root() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "compose-media");
        let tree = tree_in(&scratch);
        let mut req = request(&tree);
        req.package_dir = None;

        let composed = compose_runnable(&req).unwrap();

        assert_eq!(
            composed.plan.working_directory.as_deref(),
            Some("ARTPkg:BoingBag3.9-1"),
            "and 'BoingBag3.9-1' is what boingbag-39-1.json declares as its media"
        );
    }

    /// A package whose files really are at the wrapper's root is expressible,
    /// and the path carries no stray separator.
    #[test]
    fn a_package_at_the_volume_root_composes_without_a_separator() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "compose-root");
        let tree = tree_in(&scratch);
        let mut req = request(&tree);
        req.package_dir = Some("  ".to_string());
        req.system_volume = Some("  DH3  ".to_string());

        let composed = compose_runnable(&req).unwrap();

        assert_eq!(composed.plan.program, "ARTPkg:C/Updater");
        assert_eq!(composed.plan.working_directory.as_deref(), Some("ARTPkg:"));
        assert_eq!(composed.plan.args, vec!["AmigaOS-Update", "DH3:"]);
        assert_eq!(composed.package_dir, None);
    }

    /// The tree may not be mounted under the package's device name either: it
    /// would shadow the package, and a shadowed package is ART-185 arriving
    /// through a name instead of through a missing mount.
    ///
    /// Lower case on purpose in the second case — AmigaDOS device names are
    /// case-insensitive, so a comparison that is not would let it through.
    #[test]
    fn a_tree_mounted_under_the_package_volumes_name_is_refused_at_composition() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "compose-shadow");
        let tree = tree_in(&scratch);
        for hostile in [PACKAGE_VOLUME, "artpkg"] {
            let mut req = request(&tree);
            req.system_volume = Some(hostile.to_string());

            let err = compose_runnable(&req).unwrap_err();

            assert!(
                err.to_string().contains(PACKAGE_VOLUME),
                "the refusal must name it: {err}"
            );
        }
    }

    /// Nothing a caller writes may take the run out of the volume ART
    /// mounted, which is the containment `PlannedRun::program` says its own
    /// module cannot check.
    #[test]
    fn a_package_directory_that_leaves_the_volume_is_refused() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "compose-escape");
        let tree = tree_in(&scratch);
        for hostile in [
            "../../Windows",
            "Pkg/../../..",
            "DH1:Pkg",
            "/Pkg",
            "Pkg//C",
            "Pkg/",
            "Pkg\\C",
            ".",
            "Boing Bag",
        ] {
            let mut req = request(&tree);
            req.package_dir = Some(hostile.to_string());
            let composed = compose_runnable(&req);
            assert!(
                composed.is_err(),
                "'{hostile}' must not compose, got {composed:?}"
            );
        }
    }

    /// A volume name is a name. One carrying a colon would be composing the
    /// path from both ends, and one carrying a space cannot survive the
    /// generated line.
    #[test]
    fn a_volume_that_is_not_a_bare_name_is_refused() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "compose-volname");
        let tree = tree_in(&scratch);
        for hostile in ["DH0:", "DH0:Pkg", "My Volume"] {
            let mut req = request(&tree);
            req.system_volume = Some(hostile.to_string());
            // The message has to name the value, not merely be an error. The
            // tree is a real distribution tree for the same reason (fix round
            // 1): against a folder with no manifest this test would have
            // passed on the *chain* refusal instead, whatever the volume said.
            let err = compose_runnable(&req).unwrap_err().to_string();
            assert!(
                err.contains(hostile),
                "'{hostile}' must be refused by name: {err}"
            );
        }
    }

    /// The composition is validated through the same generator the run uses,
    /// so a preview cannot answer where the run would refuse. `ARTWork` is
    /// the case that proves it: nothing in this file refuses it, and it must
    /// still be refused, because ART's own volume carries the running script
    /// and the result file the host is waiting on.
    #[test]
    fn a_run_that_would_reach_into_arts_own_volume_is_refused_at_composition() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "compose-artwork");
        let tree = tree_in(&scratch);
        let mut req = request(&tree);
        req.system_volume = Some(WORK_VOLUME.to_string());

        let err = compose_runnable(&req).unwrap_err();

        assert!(
            err.to_string().contains(WORK_VOLUME),
            "the refusal must name it: {err}"
        );
    }

    /// This round runs packages ART ships a recipe for and nothing else — the
    /// boundary the content-layer round drew, unchanged (design §3).
    #[test]
    fn a_package_art_cannot_run_on_the_amiga_is_refused_by_name() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "compose-no-installer");
        let tree = tree_in(&scratch);
        let mut req = request(&tree);
        req.package_id = "locale-turkish".to_string();
        let err = compose(&req).unwrap_err();
        assert!(err.to_string().contains("locale-turkish"), "got {err}");

        req.package_id = "no-such-package".to_string();
        assert!(compose(&req).is_err());
    }

    /// An unknown machine is refused by name rather than quietly becoming the
    /// default — an installer that failed under hardware the user did not
    /// choose would be a failure ART invented.
    #[test]
    fn an_unknown_machine_profile_is_refused_rather_than_defaulted() {
        assert_eq!(profile_for(None).unwrap().id, DEFAULT_PROFILE_ID);
        assert_eq!(profile_for(Some("  ")).unwrap().id, DEFAULT_PROFILE_ID);
        assert_eq!(profile_for(Some("a500-ocs")).unwrap().id, "a500-ocs");

        let err = profile_for(Some("a5000-turbo")).unwrap_err();
        assert!(err.to_string().contains("a5000-turbo"), "got {err}");
    }

    // -- the pipeline ----------------------------------------------------

    /// Cancelling **before** the copy is made stops there: nothing is staged
    /// and the run is never reached.
    ///
    /// **The tree is empty on purpose, and that is the whole point of the
    /// test.** `stage_with` checks for cancellation between whole files, so a
    /// tree with anything in it is stopped by *its* guard and this one could
    /// be deleted without a test noticing — measured, by removing the check in
    /// `perform` and watching an earlier version of this test still pass. An
    /// empty tree copies with no entries to check between, so the only thing
    /// that can stop the emulator being launched after the user pressed Stop
    /// is the check in `perform`.
    #[test]
    fn a_run_cancelled_before_it_starts_copies_nothing() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "pre-cancel");
        let tree = scratch.join("Workbench3.9");
        std::fs::create_dir_all(&tree).unwrap();
        let sink = Sink::cancelled();
        let ran = AtomicUsize::new(0);

        let result = perform(&tree, &sink, |_, _| {
            ran.fetch_add(1, Ordering::Relaxed);
            Ok(RunOutcome::Succeeded)
        });

        assert!(matches!(result, Err(CoreError::Cancelled)), "{result:?}");
        assert_eq!(ran.load(Ordering::Relaxed), 0, "nothing may be launched");
        assert!(
            copies_beside(&tree).is_empty(),
            "and nothing may be copied either"
        );
    }

    /// A cancellation **during** the run discards the copy — the one path
    /// where it does not survive (design §4).
    ///
    /// `Staged` has no `Drop`, so if `perform` forgets to discard, the copy
    /// is still on disk when this looks. That is what makes the assertion
    /// load-bearing rather than one that would hold anyway.
    #[test]
    fn a_cancelled_run_discards_the_copy_and_leaves_the_original() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "cancel");
        let tree = tree_in(&scratch);
        let sink = Sink::default();
        let staged_at = std::sync::Mutex::new(PathBuf::new());

        let result = perform(&tree, &sink, |copy, _| {
            *staged_at.lock().unwrap() = copy.to_path_buf();
            // The copy exists at this moment, which is what makes its
            // absence afterwards mean something.
            assert!(copy.join("Libs/version.library").is_file());
            Err(CoreError::Cancelled)
        });

        assert!(matches!(result, Err(CoreError::Cancelled)), "{result:?}");
        assert!(
            !staged_at.lock().unwrap().exists(),
            "a cancelled run leaves no half-installed copy behind"
        );
        assert!(copies_beside(&tree).is_empty());
        assert_eq!(
            std::fs::read(tree.join("Libs/version.library")).unwrap(),
            b"the original",
            "and the original is exactly as it was"
        );
    }

    /// **ART-187.** A discard that *worked* used to report nothing, so the
    /// panel kept showing whatever phase the run last announced under a badge
    /// that correctly claimed nothing about the copy. Stale, not false — and
    /// a screen that goes on saying something after it stopped being so is
    /// the family this round was about.
    ///
    /// Both endings are reported now, and the sentence names the original,
    /// because "your tree was not touched" is the half the user actually
    /// needs.
    #[test]
    fn a_cancelled_run_says_what_became_of_the_copy() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "cancel-says");
        let tree = tree_in(&scratch);
        let sink = Sink::default();

        let _ = perform(&tree, &sink, |_, _| Err(CoreError::Cancelled));

        assert!(
            sink.said("was removed"),
            "a successful discard has to say so: {:?}",
            sink.messages.lock().unwrap()
        );
        assert!(
            sink.said(&tree.display().to_string()),
            "and name the tree it did not touch: {:?}",
            sink.messages.lock().unwrap()
        );
        // The screen may not out-claim the core: nothing here may read as a
        // failure to remove when the removal worked.
        assert!(
            !sink.said("could not be removed"),
            "{:?}",
            sink.messages.lock().unwrap()
        );
    }

    /// The three endings that are not a success: the original is untouched,
    /// the copy stays, and the report names **both** — a user told "it
    /// failed" and not told where the evidence went has been given nothing.
    #[test]
    fn a_run_that_did_not_succeed_keeps_the_copy_and_names_both_paths() {
        for outcome in [
            RunOutcome::Failed,
            RunOutcome::TimedOut {
                waited: Duration::from_secs(1200),
            },
            RunOutcome::EmulatorClosed {
                waited: Duration::from_secs(31),
            },
        ] {
            let scratch = ScratchDir::new("art-amigainstall-cmd", "kept");
            let tree = tree_in(&scratch);
            let sink = Sink::default();
            let wanted = outcome.clone();

            let (ending, settlement) = perform(&tree, &sink, move |copy, _| {
                std::fs::write(copy.join("Libs/version.library"), b"installed").unwrap();
                Ok(wanted)
            })
            .unwrap();

            assert_eq!(ending, outcome);
            match settlement {
                SettlementReport::Kept { copy, original } => {
                    assert_eq!(original, tree, "the report must name the untouched tree");
                    assert_eq!(
                        std::fs::read(copy.join("Libs/version.library")).unwrap(),
                        b"installed",
                        "and the copy must still hold what the installer did"
                    );
                }
                other => panic!("{outcome:?} must not promote: {other:?}"),
            }
            assert_eq!(
                std::fs::read(tree.join("Libs/version.library")).unwrap(),
                b"the original",
                "the original is untouched after {outcome:?}"
            );
        }
    }

    /// Only a success promotes, and the tree ends up holding what the
    /// installer wrote.
    #[test]
    fn a_successful_run_promotes_the_copy_over_the_tree() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "ok");
        let tree = tree_in(&scratch);
        let sink = Sink::default();

        let (outcome, settlement) = perform(&tree, &sink, |copy, _| {
            std::fs::write(copy.join("Libs/version.library"), b"installed").unwrap();
            Ok(RunOutcome::Succeeded)
        })
        .unwrap();

        assert_eq!(outcome, RunOutcome::Succeeded);
        match &settlement {
            SettlementReport::Promoted {
                tree: promoted,
                left_behind,
            } => {
                assert_eq!(promoted, &tree, "the tree keeps its own path");
                assert_eq!(left_behind, &None, "and the previous one is gone");
            }
            other => panic!("a success must promote: {other:?}"),
        }
        assert_eq!(
            std::fs::read(tree.join("Libs/version.library")).unwrap(),
            b"installed"
        );
        assert!(
            copies_beside(&tree).is_empty(),
            "and nothing is left beside"
        );
    }

    /// An error part way through is not one of the four endings and not a
    /// cancellation. The copy stays — the emulator may have changed it — and
    /// the user is told where it is and that their own tree was not touched.
    #[test]
    fn an_error_mid_run_keeps_the_copy_and_says_where_it_is() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "error");
        let tree = tree_in(&scratch);
        let sink = Sink::default();

        let result = perform(&tree, &sink, |_, _| {
            Err(CoreError::InvalidInput("the mount went away".into()))
        });

        assert!(result.is_err());
        let copies = copies_beside(&tree);
        assert_eq!(copies.len(), 1, "the copy is evidence and must survive");
        assert!(
            sink.said(&copies[0].display().to_string()),
            "and the user must be told where it is: {:?}",
            sink.messages.lock().unwrap()
        );
        assert!(
            sink.said(&tree.display().to_string()),
            "and that their own tree was not touched"
        );
        assert_eq!(
            std::fs::read(tree.join("Libs/version.library")).unwrap(),
            b"the original"
        );
    }

    // -- the package actually reaches the run (ART-185) ------------------

    /// The wrapper is unpacked **before** the tree is copied, and a wrong
    /// archive is refused there — so nothing is staged at all.
    ///
    /// Two things at once, and both are load-bearing. Move the unpack after
    /// `perform` and the second assertion fails: a user who pointed at the
    /// wrong `.lha` would be left with a copy of their whole tree beside it
    /// for nothing. Delete the drawer check in `packagevol::unpack` and the
    /// first fails, because the run would then proceed to an emulator with a
    /// `Euro-Update` drawer where the script expects `BoingBag3.9-1` — and
    /// come back saying the installer said no.
    ///
    /// **No emulator is opened by this test**: it never gets past the unpack.
    #[test]
    fn a_wrong_archive_is_refused_before_the_tree_is_copied() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "wrong-archive");
        let tree = tree_in(&scratch);
        let archive = scratch.join("Euro-Update.lha");
        std::fs::write(
            &archive,
            crate::core::lha::tests::make_lha_with(&[
                ("Euro-Update.info", b"icon"),
                ("Euro-Update/C/Updater", b"a different package's updater"),
            ]),
        )
        .unwrap();

        let composed = compose_runnable(&request(&tree)).unwrap();
        let err = install(
            &composed,
            &tree,
            &archive,
            &AmigaProfile::a1200_aga(),
            Path::new("no-such.rom"),
            Path::new("no-such.exe"),
            &std::env::temp_dir(),
            &Sink::default(),
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("BoingBag3.9-1"),
            "the refusal must name the drawer the package needs: {err}"
        );
        assert!(
            copies_beside(&tree).is_empty(),
            "and nothing may have been staged: the unpack comes first"
        );
    }

    /// With the right archive the run is reached — proved by the *next* thing
    /// that refuses being the Kickstart, which `media_for` asks for after the
    /// three mounts are built.
    ///
    /// This is how far the pipeline can be driven without opening an emulator
    /// on the owner's desktop, and it is far enough to prove the unpack
    /// succeeded: with the package missing, the error would be about the
    /// package instead. Remove the unpack call from `install` and this fails
    /// with a message naming the *package volume* rather than the Kickstart,
    /// which is `media_for`'s ART-185 guard doing its job one step earlier.
    #[test]
    fn the_right_archive_unpacks_and_the_run_gets_as_far_as_the_kickstart() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "unpacks");
        let tree = tree_in(&scratch);
        let archive = scratch.join("BoingBag39-1.lha");
        std::fs::write(&archive, boingbag_lha()).unwrap();

        let composed = compose_runnable(&request(&tree)).unwrap();
        let sink = Sink::default();
        let err = install(
            &composed,
            &tree,
            &archive,
            &AmigaProfile::a1200_aga(),
            Path::new("no-such.rom"),
            Path::new("no-such.exe"),
            &std::env::temp_dir(),
            &sink,
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("Kickstart"),
            "the unpack and all three mounts must have been fine; got {err}"
        );
        assert!(
            !err.to_string().contains("unpacked"),
            "and it must not be the package that was missing: {err}"
        );
        // The copy is evidence and stays; the original is untouched. Same
        // rule as `an_error_mid_run_keeps_the_copy_and_says_where_it_is`.
        assert_eq!(
            std::fs::read(tree.join("Libs/version.library")).unwrap(),
            b"the original"
        );
    }

    /// A package archive with a hostile entry name is unpacked without
    /// anything escaping, and the run is told about the refusals.
    ///
    /// The archive is a real user's file and ART cannot vouch for it. The
    /// guarantee is `core::archive`'s gate and it is absolute; what this adds
    /// is that ART **says** an entry was refused rather than unpacking a
    /// hostile archive in silence.
    #[test]
    fn a_hostile_entry_in_the_package_archive_is_reported_and_never_written() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "hostile-entry");
        let tree = tree_in(&scratch);
        let archive = scratch.join("BoingBag39-1.lha");
        std::fs::write(
            &archive,
            crate::core::lha::tests::make_lha_with(&[
                ("BoingBag3.9-1.info", b"icon"),
                // A fit build: this test is about the traversal gate, and an
                // `Updater` that said nothing about itself would be refused by
                // ART-186's version check first, so the refusals it is
                // asserting on would never be reported at all.
                ("BoingBag3.9-1/C/Updater", updater(45, 15)),
                ("../../Workbench3.9/Libs/version.library", b"planted"),
                ("C:/Windows/System32/art.dll", b"planted"),
            ]),
        )
        .unwrap();

        let composed = compose_runnable(&request(&tree)).unwrap();
        let sink = Sink::default();
        let _ = install(
            &composed,
            &tree,
            &archive,
            &AmigaProfile::a1200_aga(),
            Path::new("no-such.rom"),
            Path::new("no-such.exe"),
            &std::env::temp_dir(),
            &sink,
        );

        assert_eq!(
            std::fs::read(tree.join("Libs/version.library")).unwrap(),
            b"the original",
            "an archive entry may never reach the user's own tree"
        );
        assert!(
            sink.said("would not write"),
            "and the user must be told an entry was refused: {:?}",
            sink.messages.lock().unwrap()
        );
    }

    // -- the wire --------------------------------------------------------

    /// The four endings, exactly as the frontend will receive them.
    ///
    /// `src/lib/amigainstall.ts` declares the same four `kind`s and
    /// `src/lib/amigainstall.test.ts` checks the two lists against each
    /// other; this pins the JSON itself, including that a struct variant's
    /// own field does **not** inherit the enum's `rename_all`.
    #[test]
    fn every_run_outcome_has_the_shape_the_frontend_reads() {
        let cases = [
            (RunOutcome::Succeeded, r#"{"kind":"succeeded"}"#),
            (RunOutcome::Failed, r#"{"kind":"failed"}"#),
            (
                RunOutcome::TimedOut {
                    waited: Duration::from_secs(1200),
                },
                r#"{"kind":"timed-out","waited":{"secs":1200,"nanos":0}}"#,
            ),
            (
                RunOutcome::EmulatorClosed {
                    waited: Duration::from_secs(31),
                },
                r#"{"kind":"emulator-closed","waited":{"secs":31,"nanos":0}}"#,
            ),
        ];
        for (outcome, expected) in cases {
            assert_eq!(serde_json::to_string(&outcome).unwrap(), expected);
        }
    }

    /// `#[serde(rename_all)]` on an enum renames its **variants**, not the
    /// fields inside a struct variant — a real wire bug in this project this
    /// week. `left_behind` reaches the frontend as `leftBehind` because the
    /// variant carries its own attribute, and this is what says so.
    #[test]
    fn settlement_is_camel_case_on_the_wire_including_inside_a_variant() {
        let promoted = SettlementReport::Promoted {
            tree: PathBuf::from("T"),
            left_behind: Some(PathBuf::from("P")),
        };
        assert_eq!(
            serde_json::to_string(&promoted).unwrap(),
            r#"{"kind":"promoted","tree":"T","leftBehind":"P"}"#
        );

        let kept = SettlementReport::Kept {
            copy: PathBuf::from("C"),
            original: PathBuf::from("O"),
        };
        assert_eq!(
            serde_json::to_string(&kept).unwrap(),
            r#"{"kind":"kept","copy":"C","original":"O"}"#
        );
    }

    /// The preview writes nothing and starts nothing — §92's PREVIEW, and the
    /// property `run_workflow`'s `Safety::ReadOnly` rule exists to protect.
    #[test]
    fn the_preview_touches_nothing() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "preview");
        let tree = tree_in(&scratch);
        // The request — and so the disc fixture it carries — is built first,
        // because what this test is about is that *the preview* creates
        // nothing, not that a fixture does.
        let request = request(&tree);
        let before = std::fs::read_dir(scratch.path()).unwrap().count();

        let preview = preview_runnable(request, Some("no-such.exe")).unwrap();

        assert_eq!(preview.program, "ARTPkg:BoingBag3.9-1/C/Updater");
        assert_eq!(preview.work_volume, WORK_VOLUME);
        assert_eq!(
            preview.package_volume, PACKAGE_VOLUME,
            "the user will see a third volume on the Workbench; say so"
        );
        assert!(
            !preview.package_archive_present,
            "and it says the package's own archive is missing too"
        );
        assert_eq!(preview.result_file, RESULT_FILE);
        assert!(preview.deadline_seconds > 0, "a deadline is not optional");
        assert!(!preview.kickstart_present, "and it says what is missing");
        assert_eq!(preview.profile_id, DEFAULT_PROFILE_ID);
        assert_eq!(
            std::fs::read_dir(scratch.path()).unwrap().count(),
            before,
            "a preview creates nothing"
        );
        assert!(copies_beside(&tree).is_empty(), "and copies nothing");
        assert_eq!(
            std::fs::read(tree.join("Libs/version.library")).unwrap(),
            b"the original"
        );
    }

    // -----------------------------------------------------------------
    // ART-186: the chain is mandatory, and the refusal is in `compose`
    // -----------------------------------------------------------------

    /// The defect, in one line: BoingBag 2 on a tree BoingBag 1 never touched
    /// used to compose cleanly. It is refused in `compose`, which is what
    /// makes the **preview** refuse it too — a screen that offered a confirm
    /// button here would be offering a run that cannot happen.
    #[test]
    fn boingbag_two_is_refused_on_a_tree_without_boingbag_one() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "chain");
        let tree = tree_with_manifest(&scratch, &["workbench-base"]);

        let mut req = request(&tree);
        req.package_id = "boingbag-39-2".to_string();

        let err = compose_runnable(&req).unwrap_err();
        assert!(
            err.to_string().contains("BoingBag 3.9-1"),
            "the refusal must name what is missing: {err}"
        );

        // And the preview, which is the screen the user is actually looking
        // at, refuses the same way rather than describing the run.
        let err = preview_runnable(request_for(&tree, "boingbag-39-2"), None).unwrap_err();
        assert!(err.to_string().contains("BoingBag 3.9-1"), "got {err}");
    }

    /// **A declaration nobody has run is refused here, not only greyed out
    /// on a screen** (round 3, §10/§89).
    ///
    /// `boingbags-39-3-4` declares an `amiga_installer` so the row exists at
    /// all — hiding it would be ART claiming it ships nothing for a package
    /// it ships a whole recipe for — and declares `not_yet_runnable` because
    /// its `Install` is an Installer script nobody has driven unattended. The
    /// panel renders that row disabled; this is the guard that means a
    /// request built any other way is refused too.
    ///
    /// The refusal is checked to be *this* one and not the chain's: the
    /// tree carries BoingBag 3.9-2, so prerequisites are met and the only
    /// thing left standing in the way is the declaration itself.
    #[test]
    fn a_package_whose_installer_nobody_has_run_is_refused_and_says_so() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "not-yet-runnable");
        let tree = tree_with_manifest(
            &scratch,
            &["workbench-base", "boingbag-39-1", "boingbag-39-2"],
        );

        let err = compose_runnable(&request_for(&tree, "boingbags-39-3-4")).unwrap_err();
        let text = err.to_string();
        assert!(
            text.contains("nobody has measured whether that script finishes"),
            "the refusal must say what has not been measured: {text}"
        );
        assert!(
            !text.contains("ART ships no Amiga-side installer"),
            "that is a different sentence about a different package: {text}"
        );
        // The screen the user is looking at refuses identically.
        let err = amiga_install_preview(request_for(&tree, "boingbags-39-3-4"), None).unwrap_err();
        assert!(
            err.to_string()
                .contains("nobody has measured whether that script finishes"),
            "got {err}"
        );
    }

    /// The same package on a tree that has BoingBag 1 composes. A refusal that
    /// fired either way would be no check at all.
    #[test]
    fn boingbag_two_composes_once_the_tree_has_boingbag_one() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "chain-ok");
        let tree = tree_with_manifest(&scratch, &["workbench-base", "boingbag-39-1"]);

        let composed = compose_runnable(&request_for(&tree, "boingbag-39-2")).unwrap();
        assert_eq!(composed.plan.package_id, "boingbag-39-2");
    }

    /// **Refused before anything is copied.** The whole tree copy, both
    /// scratch volumes and the unpack all happen after `compose`, so a refused
    /// run leaves the user's folder exactly as it was — no `.art-staged`
    /// directory, no partial anything.
    #[test]
    fn a_refused_chain_copies_nothing_at_all() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "chain-nothing");
        let tree = tree_with_manifest(&scratch, &["workbench-base"]);
        let request = request_for(&tree, "boingbag-39-2");
        let before = std::fs::read_dir(scratch.path()).unwrap().count();

        assert!(compose_runnable(&request).is_err());

        assert!(
            copies_beside(&tree).is_empty(),
            "no copy may have been made"
        );
        assert_eq!(
            std::fs::read_dir(scratch.path()).unwrap().count(),
            before,
            "and nothing at all was created"
        );
        assert_eq!(
            std::fs::read(tree.join("Libs/version.library")).unwrap(),
            b"the original"
        );
    }

    /// BoingBag 1 requires nothing and is **still** refused against a tree
    /// with no `distribution.json` — fix round 1's Major, at the seam.
    ///
    /// This test asserted the opposite until 2026-08-21. The run was allowed,
    /// and `record_if_succeeded` would then have failed on that same tree
    /// *after the installer had worked*: the copy is kept, nothing is
    /// promoted, and ART reports a failure about a success. The refusal is
    /// the manifest one and names no package, because which packages such a
    /// tree has is precisely what ART cannot say.
    #[test]
    fn boingbag_one_is_refused_on_a_tree_with_no_manifest() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "chain-none");
        let tree = tree_without_manifest(&scratch);
        assert!(!tree.join("distribution.json").exists());

        let err = compose_runnable(&request(&tree)).unwrap_err().to_string();
        assert!(err.contains("distribution.json"), "got {err}");
        assert!(
            !err.contains("BoingBag"),
            "it must not claim a package is missing: {err}"
        );
    }

    /// **The ending that had no test: a run that worked, reported as failed.**
    ///
    /// `record_if_succeeded` is the last thing between a successful installer
    /// and the promotion, and on a tree `compose` used to allow it was the
    /// thing that failed. Asserted at the seam rather than inside one module:
    /// for every tree, what `compose` accepts must reach a **promotion** when
    /// the installer succeeds, and what it rejects must be rejected before a
    /// single byte is copied.
    #[test]
    fn a_run_compose_accepts_always_reaches_a_promotion_when_the_installer_succeeds() {
        for (tag, with_manifest) in [("accepted", true), ("rejected", false)] {
            let scratch = ScratchDir::new("art-amigainstall-cmd", tag);
            let tree = if with_manifest {
                tree_with_manifest(&scratch, &["workbench-base"])
            } else {
                tree_without_manifest(&scratch)
            };

            let Ok(composed) = compose_runnable(&request(&tree)) else {
                assert!(!with_manifest, "a tree with a manifest must be accepted");
                assert!(
                    copies_beside(&tree).is_empty(),
                    "and a rejected run copies nothing"
                );
                continue;
            };
            assert!(
                with_manifest,
                "a tree with no manifest must not be accepted"
            );

            let plan = composed.plan.clone();
            let (outcome, settlement) = perform(&tree, &Sink::default(), |copy, _sink| {
                let outcome = RunOutcome::Succeeded;
                record_if_succeeded(copy, &plan, &outcome)?;
                Ok(outcome)
            })
            .unwrap();

            assert_eq!(outcome, RunOutcome::Succeeded);
            assert!(
                matches!(settlement, SettlementReport::Promoted { .. }),
                "a successful installer must end in a promotion, never in an error the \
                 user reads as the install having failed"
            );
            assert!(chain::applied(&tree).unwrap().contains("boingbag-39-1"));
        }
    }

    /// A run that says it succeeded records itself in the promoted tree's own
    /// `distribution.json` — the half without which the refusal above could
    /// never be satisfied, because a BoingBag cannot be placed from the host
    /// at all.
    ///
    /// Driven through [`perform`] and [`record_if_succeeded`], the same two
    /// functions the real run uses, with the emulator replaced by a closure:
    /// no window opens on the owner's desktop.
    #[test]
    fn a_successful_run_records_itself_in_the_promoted_trees_manifest() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "record");
        let tree = tree_with_manifest(&scratch, &["workbench-base"]);
        let composed = compose_runnable(&request(&tree)).unwrap();
        let plan = composed.plan.clone();

        let (outcome, settlement) = perform(&tree, &Sink::default(), |copy, _sink| {
            let outcome = RunOutcome::Succeeded;
            record_if_succeeded(copy, &plan, &outcome)?;
            Ok(outcome)
        })
        .unwrap();

        assert_eq!(outcome, RunOutcome::Succeeded);
        assert!(matches!(settlement, SettlementReport::Promoted { .. }));

        let applied = chain::applied(&tree).unwrap();
        assert!(
            applied.contains("boingbag-39-1"),
            "the tree must now say it has BoingBag 1: {applied:?}"
        );

        // And that is exactly what unblocks the next link in the chain.
        compose_runnable(&request_for(&tree, "boingbag-39-2")).unwrap();
    }

    /// **Only** a successful run records anything. The other three endings
    /// leave the tree saying what it said before, so a BoingBag 1 that failed,
    /// timed out or was interrupted does not let BoingBag 2 through.
    ///
    /// All three are exercised through [`record_if_succeeded`] itself rather
    /// than through a condition the test writes out again — a test carrying
    /// its own copy of the rule passes however the real one is mutated.
    #[test]
    fn only_a_successful_run_records_anything() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "record-endings");
        let tree = tree_with_manifest(&scratch, &["workbench-base"]);
        let composed = compose_runnable(&request(&tree)).unwrap();
        let plan = &composed.plan;

        for ending in [
            RunOutcome::Failed,
            RunOutcome::TimedOut {
                waited: std::time::Duration::from_secs(1200),
            },
            RunOutcome::EmulatorClosed {
                waited: std::time::Duration::from_secs(3),
            },
        ] {
            record_if_succeeded(&tree, plan, &ending).unwrap();
            assert!(
                !chain::applied(&tree).unwrap().contains("boingbag-39-1"),
                "{ending:?} must record nothing"
            );
            assert!(
                compose_runnable(&request_for(&tree, "boingbag-39-2")).is_err(),
                "{ending:?} must leave the chain shut"
            );
        }

        // The one that does, against the same tree, so the difference is the
        // ending and nothing else.
        record_if_succeeded(&tree, plan, &RunOutcome::Succeeded).unwrap();
        assert!(chain::applied(&tree).unwrap().contains("boingbag-39-1"));
        compose_runnable(&request_for(&tree, "boingbag-39-2")).unwrap();
    }

    /// The package's own archive still passes — a check that refused
    /// everything would pass the two tests above and break the product.
    #[test]
    fn the_packages_own_archive_is_still_accepted() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "preview-right-archive");
        let tree = tree_in(&scratch);
        let archive = scratch.join("BoingBag39-1.lha");
        std::fs::write(&archive, boingbag_lha()).unwrap();

        let mut req = request(&tree);
        req.package_archive = archive;

        assert!(
            preview_runnable(req, None).is_ok(),
            "the real archive must still preview"
        );
    }

    /// An archive that is not there is left to the panel's own missing-file
    /// blocker, not turned into a drawer refusal — the two say different
    /// things and the user needs the one that is true.
    #[test]
    fn a_missing_archive_is_not_refused_as_the_wrong_one() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "preview-absent-archive");
        let tree = tree_in(&scratch);

        let mut req = request(&tree);
        req.package_archive = scratch.join("nothing-here.lha");

        let preview = preview_runnable(req, None).expect("no refusal for an absent file");
        assert!(!preview.package_archive_present);
    }

    // -----------------------------------------------------------------
    // ART-277: classifying an archive at the moment it is picked, before
    // it ever reaches a request.
    // -----------------------------------------------------------------

    /// A wrapper shaped like the owner's own `BoingBag39-2.lha` — the archive
    /// this round's defect was about: chosen while BoingBag 3.9-1 was still
    /// selected.
    fn boingbag2_lha() -> Vec<u8> {
        crate::core::lha::tests::make_lha_with(&[
            ("BoingBag3.9-2.info", b"icon"),
            ("BoingBag3.9-2/AmigaOS-Update", b"PK encrypted"),
            ("BoingBag3.9-2/C/Updater", b"boingbag 2's own updater"),
        ])
    }

    const RELEASE: &str = "AmigaOS 3.9";

    fn release_packages() -> Vec<package::Package> {
        package::packages_for(RELEASE).unwrap()
    }

    /// The one package this screen's radio offers, since 2026-09-08: the
    /// only shipped recipe still declaring an `amiga_installer`. Every
    /// classification below is asked *as that package*, because the whole
    /// point of the `another-package:<id>` kind is that the id it names is
    /// selectable here.
    fn selected_package() -> package::Package {
        package::by_id("boingbags-39-3-4").unwrap()
    }

    // -----------------------------------------------------------------
    // ART-277 re-review, L3/L4: the core refusal's own catalogue is
    // release-scoped and excludes the selected package, the same discipline
    // `classify_top_level` already keeps.
    // -----------------------------------------------------------------

    #[test]
    fn known_packages_from_excludes_a_package_from_a_different_release() {
        let selected = selected_package();
        // A package sharing BoingBag 3.9-2's own shape but declared for a
        // release this build is not — the shipped catalogue has only one
        // release today, so this is built rather than found, per the
        // review's own note that it is otherwise untestable.
        let installable = package::by_id("boingbags-39-3-4").unwrap();
        let other_release = package::Package {
            id: "other-release-pkg".to_string(),
            releases: vec!["AmigaOS 3.2".to_string()],
            ..installable.clone()
        };
        let same_release = package::Package {
            id: "same-release-pkg".to_string(),
            releases: vec!["AmigaOS 3.9".to_string()],
            media: "SameRelease".to_string(),
            ..installable
        };
        let all = vec![selected.clone(), other_release, same_release];

        let names: Vec<String> = known_packages_from(&selected, &all)
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(
            names,
            vec!["same-release-pkg".to_string()],
            "must exclude the selected package itself and any package of a different release"
        );
    }

    #[test]
    fn classify_top_level_recognises_the_selected_packages_own_archive() {
        let selected = selected_package();
        let catalogue = release_packages();
        assert_eq!(
            classify_top_level("BoingBag3.9-3&4", Some(&selected), &catalogue, None).kind,
            "the-package"
        );
    }

    /// The whole of ART-277: an archive that is a *different* catalogued
    /// package's own, offered while something else is selected, is named by
    /// id rather than folded into "unknown".
    ///
    /// **`another-package:<id>` is only ever said of a package this screen's
    /// radio offers**, because the sentence it produces is "select that one
    /// instead" — so it is asked with the one shipped Amiga-installable
    /// package as the *other*, and something else selected. Since 2026-09-08
    /// that is `boingbags-39-3-4`, the only recipe still declaring an
    /// installer.
    #[test]
    fn classify_top_level_names_another_catalogued_package() {
        let selected = package::by_id("boingbag-39-1").unwrap();
        let catalogue = release_packages();
        assert_eq!(
            classify_top_level("BoingBag3.9-3&4", Some(&selected), &catalogue, None).kind,
            "another-package:boingbags-39-3-4"
        );
    }

    #[test]
    fn classify_top_level_answers_unknown_for_an_archive_nothing_recognises() {
        let selected = selected_package();
        let catalogue = release_packages();
        // `NDK39` rather than `Euro-Update`: round 3 ships a recipe whose
        // own media *is* `Euro-Update`, and the NDK installs nothing and is
        // outside the chain (research section 2), so it is an archive ART
        // genuinely does not recognise.
        assert_eq!(
            classify_top_level("NDK39", Some(&selected), &catalogue, None).kind,
            "unknown"
        );
    }

    /// **ART-277 re-review, L5 (was Major 2's `other-artefact`, split in
    /// two).** `locale-39` and `locale-39-turkish` both declare `"media":
    /// "Locale3.9"` on purpose (`recipes/packages/locale-39-turkish.json`'s
    /// own comment): two components sharing one archive is ART-276's own
    /// shape. Naming either one arbitrarily here would be that same trap
    /// wearing this round's clothes — the answer has to say what the
    /// archive *is* and name **both** packages, not guess which one it
    /// means or claim a single cause ("the Packages step places it") that
    /// is only true of the *other* kind (`other-artefact`, below).
    #[test]
    fn classify_top_level_answers_shared_artefact_when_two_packages_share_the_media() {
        let selected = selected_package();
        let catalogue = release_packages();
        let classified = classify_top_level("Locale3.9", Some(&selected), &catalogue, None);
        assert_eq!(classified.kind, "shared-artefact:Locale3.9");
        let mut shared_by = classified.shared_by;
        shared_by.sort();
        assert_eq!(
            shared_by,
            vec!["locale-39".to_string(), "locale-39-turkish".to_string()]
        );
    }

    /// **ART-277 review, Major 2 — the other half; re-review L5 keeps this
    /// one `other-artefact`.** `locale-turkish` declares `"media":
    /// "LocaleUpdate"` uniquely — a single, unambiguous match — but it is
    /// not `amiga_installable` and so is not on this screen's radio at all.
    /// "select locale-turkish" would be an instruction the user cannot
    /// follow here; this answers with what the archive is instead of a
    /// package id nobody can act on, and carries no `shared_by` (there is
    /// only the one match).
    #[test]
    fn classify_top_level_answers_other_artefact_for_a_match_the_radio_does_not_offer() {
        let selected = selected_package();
        let catalogue = release_packages();
        let classified = classify_top_level("LocaleUpdate", Some(&selected), &catalogue, None);
        assert_eq!(classified.kind, "other-artefact:LocaleUpdate");
        assert!(classified.shared_by.is_empty());
    }

    /// No package selected yet (a fresh panel, or an archive picked by hand):
    /// still names another catalogued package rather than only ever
    /// answering `"unknown"` — a query with no selected package is not the
    /// same question as one that already knows something is wrong.
    #[test]
    fn classify_top_level_without_a_selected_package_still_names_a_catalogued_one() {
        let catalogue = release_packages();
        assert_eq!(
            classify_top_level("BoingBag3.9-3&4", None, &catalogue, None).kind,
            "another-package:boingbags-39-3-4"
        );
    }

    /// **The top level alone cannot separate BoingBag 3.9-2 from its own
    /// Contribution archive, and the archive's contents can** (round 3).
    ///
    /// Both arms, on the same top level, because the whole point is that the
    /// second fact changes the answer: without a file to read, two packages
    /// claim `BoingBag3.9-2` and saying which would be a guess; with the
    /// file, `AmigaOS-Update` is in it and the Contribution's declared
    /// `Contribution/ClassAction/ClassAction` is not.
    #[test]
    fn an_archive_that_can_be_read_is_narrowed_by_what_is_inside_it() {
        let catalogue = release_packages();
        assert_eq!(
            classify_top_level("BoingBag3.9-2", None, &catalogue, None).kind,
            "shared-artefact:BoingBag3.9-2",
            "two packages claim this top level and nothing here can tell them apart"
        );

        let scratch = ScratchDir::new("art-amigainstall-cmd", "classify-narrow");
        let archive = scratch.join("BoingBag39-2.lha");
        std::fs::write(&archive, boingbag2_lha()).unwrap();
        assert_eq!(
            classify_top_level("BoingBag3.9-2", None, &catalogue, Some(&archive)).kind,
            "other-artefact:BoingBag3.9-2",
            "one package claims it now — and not one this screen's radio offers, since \
             BoingBag 3.9-2 is placed from Windows"
        );
    }

    /// End to end, through the command itself: a real file on disk, read by
    /// its listing alone. This is the one test in the group that proves
    /// `amigainstall_classify_archive` actually opens the archive rather than
    /// merely wiring `classify_top_level` correctly.
    #[test]
    fn the_command_classifies_a_real_archive_on_disk() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "classify-archive");
        let archive = scratch.join("BoingBag39-2.lha");
        std::fs::write(&archive, boingbag2_lha()).unwrap();

        let answer = amigainstall_classify_archive(
            archive,
            "boingbags-39-3-4".to_string(),
            RELEASE.to_string(),
        )
        .unwrap();
        // One package claims `BoingBag3.9-2` once the archive is read, and it
        // is not one this screen can select — so it is named as an artefact,
        // never as "select boingbag-39-2", which is not a thing the radio
        // offers.
        assert_eq!(answer.kind, "other-artefact:BoingBag3.9-2");
        assert!(
            answer.top_level.iter().any(|n| n == "BoingBag3.9-2"),
            "got {:?}",
            answer.top_level
        );
        // The selected package's own expectation travels with the answer.
        assert_eq!(answer.expected_media.as_deref(), Some("BoingBag3.9-3&4"));
    }

    /// A file that is not an archive at all — or is not there — answers
    /// `"unknown"` rather than an error: this is asked on every file pick,
    /// and a query must not turn "I could not tell" into something the user
    /// cannot get past.
    #[test]
    fn the_command_is_lenient_about_a_file_it_cannot_read() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "classify-unreadable");
        let answer = amigainstall_classify_archive(
            scratch.join("nothing-here.lha"),
            "boingbag-39-1".to_string(),
            RELEASE.to_string(),
        )
        .unwrap();
        assert_eq!(answer.kind, "unknown");
        assert!(answer.top_level.is_empty());
    }
}

#[cfg(test)]
mod real_install_hook {
    //! Run a real package's own installer against the owner's own tree — the
    //! one thing every other test in this round cannot reach.
    //!
    //! Everything above this module is synthetic by design: a fixture LHA ART
    //! itself wrote, a fake launcher, an injected clock. That is what makes
    //! them fast and deterministic, and it is also their ceiling — a fixture
    //! cannot tell anyone whether a twenty-five-year-old `Updater` finds the
    //! volume it is looking for, how long it takes, or whether it asks a
    //! question nobody is there to answer.
    //!
    //! So this hook takes the same shape as
    //! [`crate::core::osinstall::apply`]'s `build_the_real_39_tree_when_asked`
    //! and `core::winuae`'s `boot_a_distribution_tree_when_asked`: `#[ignore]`d,
    //! gated on environment variables that only exist on the owner's machine,
    //! and a silent `return` when they do not — so CI is green without it and
    //! nothing here is ever written into the repository.
    //!
    //! **It opens an emulator window**, deliberately and one at a time, and
    //! terminates it. Run it explicitly:
    //!
    //! ```text
    //!   ART_AMIGA_TREE=E:\amiga\ProjeART\bb-run\p2 ^
    //!   ART_AMIGA_ROM="E:\...\Kickstart v3.1 rev 40.68 (1993)(Commodore)(A1200).rom" ^
    //!   ART_WINUAE="C:\Program Files\WinUAE\winuae64.exe" ^
    //!   ART_AMIGA_PACKAGES="E:\...\BoingBag39-1 (1).lha" ^
    //!   ART_AMIGA_PACKAGE_ID=boingbag-39-1 ^
    //!   cargo test install_a_real_package_when_asked -- --ignored --nocapture
    //! ```
    //!
    //! `ART_AMIGA_PACKAGES` is `;`-separated, because ART-186's second archive
    //! is the whole point of one of the three paths this exists to walk.

    use super::*;
    use std::time::Instant;

    /// A sink that prints every phase with the seconds since the run began.
    ///
    /// The elapsed time is the measurement this hook exists for as much as the
    /// outcome is: design §6 says the deadline must be *"a multiple of"* a real
    /// installer's running time on this machine, *"recorded with what it was
    /// measured from"*, and a number nobody timed is the thing that rule
    /// forbids.
    struct Loud(Instant);

    impl ProgressSink for Loud {
        fn report(&self, done: u64, total: Option<u64>, message: &str) {
            println!(
                "[{:>7.1}s] {message}{}",
                self.0.elapsed().as_secs_f64(),
                match total {
                    Some(total) => format!(" ({done}/{total})"),
                    None => String::new(),
                }
            );
        }
        fn is_cancelled(&self) -> bool {
            false
        }
    }

    /// Files and total bytes under `root`, so the report can say what the
    /// installer actually changed rather than that it said it worked.
    fn measure(root: &Path) -> (usize, u64) {
        fn walk(dir: &Path, files: &mut usize, bytes: &mut u64) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, files, bytes);
                } else if let Ok(meta) = entry.metadata() {
                    *files += 1;
                    *bytes += meta.len();
                }
            }
        }
        let (mut files, mut bytes) = (0usize, 0u64);
        walk(root, &mut files, &mut bytes);
        (files, bytes)
    }

    #[test]
    #[ignore = "opens WinUAE against the owner's own tree, ROM and packages; run explicitly"]
    fn install_a_real_package_when_asked() {
        let (Ok(tree), Ok(rom), Ok(winuae), Ok(packages), Ok(package_id)) = (
            std::env::var("ART_AMIGA_TREE"),
            std::env::var("ART_AMIGA_ROM"),
            std::env::var("ART_WINUAE"),
            std::env::var("ART_AMIGA_PACKAGES"),
            std::env::var("ART_AMIGA_PACKAGE_ID"),
        ) else {
            return;
        };

        let tree = PathBuf::from(tree);
        let archive = PathBuf::from(packages.trim());

        let request = AmigaInstallRequest {
            tree: tree.clone(),
            package_id: package_id.clone(),
            system_volume: None,
            package_archive: archive.clone(),
            package_dir: None,
            kickstart: PathBuf::from(&rom),
            profile: None,
        };

        let before = measure(&tree);
        println!("tree before: {} files, {} bytes", before.0, before.1);
        println!("archive: {}", archive.display());

        let started = Instant::now();
        let sink = Loud(started);

        let composed = match compose(&request) {
            Ok(composed) => composed,
            Err(err) => {
                println!("REFUSED at compose after {:?}: {err}", started.elapsed());
                println!("tree after: {:?}", measure(&tree));
                return;
            }
        };
        println!(
            "command: {} (from {:?})",
            command_line_of(&composed.plan),
            composed.plan.working_directory
        );
        let profile = profile_for(request.profile.as_deref()).unwrap();

        let result = install(
            &composed,
            &tree,
            &archive,
            &profile,
            &PathBuf::from(&rom),
            &PathBuf::from(&winuae),
            &std::env::temp_dir(),
            &sink,
        );
        let elapsed = started.elapsed();

        match result {
            Ok((outcome, settlement)) => {
                println!("outcome: {outcome:?}");
                println!("settlement: {settlement:?}");
            }
            Err(err) => println!("ERROR after {elapsed:?}: {err}"),
        }
        println!("elapsed: {:.1}s", elapsed.as_secs_f64());
        let after = measure(&tree);
        println!("tree after: {} files, {} bytes", after.0, after.1);
    }
}
