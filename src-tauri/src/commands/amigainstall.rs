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
use crate::core::amigainstall::run::{run, RunLimits, RunRequest};
use crate::core::amigainstall::stage::{settle, stage_with, Settlement};
use crate::core::amigainstall::{
    packagevol, workvol, PlannedRun, RunOutcome, PACKAGE_VOLUME, RESULT_FILE, WORK_VOLUME,
};
use crate::core::error::{CoreError, CoreResult};
use crate::core::iso::IsoImage;
use crate::core::jobs::{JobId, ProgressSink};
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::core::osinstall::package::RequiredMedium;
use crate::core::osinstall::{chain, package};
use crate::core::profile::AmigaProfile;
use crate::core::sources::install::Scratch;
use crate::core::winuae::detect_winuae;
use crate::error::AppResult;

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
    /// The package's **own** archives. The first is the wrapper the user
    /// downloaded, `BoingBag39-1.lha`; ART unpacks it to a directory of its
    /// own and mounts that as a third volume.
    ///
    /// **Required, and added by ART-185.** Nothing else can supply the
    /// installer: a BoingBag's payload cannot be placed into the tree from the
    /// host at all, which is why this round exists (ART-166), so the program
    /// the run executes is in no volume ART mounts unless it comes from here.
    ///
    /// **A list, and that is ART-186.** BoingBag 3.9-1's own `Updater` is
    /// 45.13, which cannot install a BoingBag under an emulator; the fix
    /// shipped as a second archive, and its own readme's remedy is to copy
    /// that archive's `BoingBag3.9-1` drawer over the package's. So every
    /// archive after the first is an **overlay medium**, matched against what
    /// the recipe declares by what it actually carries rather than by the
    /// order the user picked their files in.
    pub package_archives: Vec<PathBuf>,
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
    /// The user's **own** copy of the medium the package's installer verifies
    /// — an image of the original disc. `None` for a package that requires
    /// none, and a refusal for one that does (ART-193).
    ///
    /// **On the request rather than in the recipe, and the split is the
    /// point.** The recipe declares *which volume* the installer looks for —
    /// a fact about the package, readable in the package's own binary, and
    /// shipped data like every other fact a recipe carries
    /// ([`RequiredMedium`](crate::core::osinstall::package::RequiredMedium)).
    /// *Which file on this machine* is that disc is not a fact about the
    /// package at all: ART ships no Amiga media and never will, and a path in
    /// a recipe would be one that is true on exactly one computer. It is the
    /// same division as [`kickstart`](Self::kickstart) and
    /// [`package_archives`](Self::package_archives) — ART knows what is
    /// needed, the user supplies what they own.
    ///
    /// The two halves are checked against each other before anything is
    /// copied: [`compose`] opens the image and asks it its own volume name,
    /// and a disc that does not state the name the recipe declares is refused
    /// naming both. That is "ask the artefact what it is; never infer it"
    /// applied to a medium — a filename is consistent with several answers,
    /// and this project has shipped the wrong tree once for reading one.
    #[serde(default)]
    pub medium: Option<PathBuf>,
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
    /// The package's own archives, as the user chose them — the wrapper
    /// first, then any overlay medium.
    pub package_archives: Vec<PathBuf>,
    /// Whether **every** one of them is actually there. A preview that did not
    /// ask would be describing a run with nothing to run; asking only about
    /// the first would describe a run ART would refuse a moment later.
    pub package_archives_present: bool,
    /// The overlay media this package declares, by the path inside such an
    /// archive that identifies one — so the screen can say what a second file
    /// would have to be before the user goes looking for it (ART-186).
    pub declared_overlays: Vec<String>,
    /// The medium the run will mount, as the user chose it — `None` when the
    /// package requires none. A person should not be surprised by a disc
    /// appearing in the emulated machine any more than by the machine itself
    /// (design §4).
    pub medium: Option<PathBuf>,
    /// The volume that image **states it has** — read from the image, never
    /// from its filename or from the recipe. It is the whole point of the
    /// check `compose` makes, so the screen shows the answer rather than the
    /// question.
    pub medium_volume: Option<String>,
    /// What the package's own installer requires — *"the original AmigaOS 3.9
    /// CD-ROM"* — when it requires one. `None` both for a package that
    /// verifies no medium and for a disc the user supplied unasked, which are
    /// two different things the screen never has to tell apart: `compose`
    /// refuses a required medium that is missing outright, so a preview
    /// exists only when whatever is required is already there.
    pub required_medium: Option<String>,
    /// The lowest version the package's installer may state, `"45.15"`, or
    /// `None` when no build of it is known to be unfit. Named on the screen
    /// because it is why a second archive may be needed at all.
    pub minimum_installer_version: Option<String>,
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
    /// The recipe's overlay declarations, translated into the record
    /// `core::amigainstall` declares for itself.
    ///
    /// **The translation is this layer's job on purpose.** `core/amigainstall`
    /// knows nothing about recipes and must not learn: CLAUDE.md's rule is
    /// that a lower-level `core/` module declares its own record carrying only
    /// what it reads, and `commands/` maps between the two — the shape
    /// `core/rom/pairing.rs` and `commands/preload.rs::rom_pairing_for`
    /// already set.
    overlays: Vec<packagevol::Overlay>,
    /// The recipe's `minimum_version`, parsed. `None` when the recipe declares
    /// none, which is every package but BoingBag 3.9-1.
    minimum_installer_version: Option<(u32, u32)>,
    /// The same thing as the recipe wrote it, for the preview.
    minimum_installer_version_text: Option<String>,
    /// The medium the run mounts, once `compose` has checked that the file
    /// the user supplied really is the disc the recipe named. `None` when the
    /// package requires none *and* the user supplied none.
    medium: Option<ComposedMedium>,
}

/// A disc `compose` has opened and vouched for (ART-193).
#[derive(Debug, Clone, PartialEq, Eq)]
struct ComposedMedium {
    /// The image on the host, as the user chose it.
    path: PathBuf,
    /// The volume name **the image itself states** — read with
    /// [`IsoImage::volume_name`], never taken from the recipe or from the
    /// filename. This is what the screen shows and what the recipe's
    /// declaration was checked against.
    volume: String,
    /// What the recipe calls this disc in a sentence, when it declared one.
    declared_as: Option<String>,
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
    let package = package::by_id(request.package_id.trim())?;
    let Some(installer) = package.amiga_installer.clone() else {
        return Err(CoreError::InvalidInput(format!(
            "ART ships no Amiga-side installer for '{}'; this is not a package it can run on \
             the Amiga",
            package.id
        )));
    };

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

    // The recipe's own declarations, translated — never carried across as the
    // recipe's own type. See `Composed::overlays`.
    let overlays: Vec<packagevol::Overlay> = installer
        .overlays
        .iter()
        .map(|overlay| packagevol::Overlay {
            from: overlay.from.clone(),
            to: overlay.to.clone(),
        })
        .collect();
    // `validate_installer` already refused a `minimum_version` that is not two
    // integers, so a shipped recipe always parses. A `None` here can therefore
    // only mean the recipe declared none.
    let minimum_installer_version = installer
        .minimum_version
        .as_deref()
        .and_then(package::parse_version_pair);

    let medium = compose_medium(
        &package.id,
        installer.required_medium.as_ref(),
        &request.medium,
    )?;

    Ok(Composed {
        plan,
        medium,
        package_name: package.name,
        package_dir,
        installer_in_package,
        overlays,
        minimum_installer_version,
        minimum_installer_version_text: installer.minimum_version.clone(),
    })
}

/// Match what the package's installer requires against what the user
/// supplied, and ask the supplied image what it actually is (ART-193).
///
/// **Three outcomes, and each is a different sentence.**
///
/// - The recipe declares a medium and nothing was supplied → refused, naming
///   the disc and the volume, *before* anything is copied or unpacked. The
///   alternative is what was measured on 2026-08-21: the installer starts,
///   finds no volume, opens its own screen and never answers, and ART reports
///   a timeout — *"nobody answered a question it asked"* — about a program
///   that was never going to get as far as asking one. §89 forbids that
///   sentence, so the run does not start.
/// - Something was supplied → the image is **opened and asked its own volume
///   name**, and when the recipe declared one they must agree. A filename is
///   not an identity; this project shipped an AmigaOS 3.5 tree under the name
///   3.9 for reading one artefact's appearance as proof of what it was.
/// - Neither → `None`, and the run mounts no CD at all, exactly as before.
///
/// Supplying a disc an installer asks for is **meeting** its check, not
/// bypassing one. ART deliberately does not satisfy such a check by
/// extracting the handful of files the program happens to name.
fn compose_medium(
    package_id: &str,
    required: Option<&RequiredMedium>,
    supplied: &Option<PathBuf>,
) -> CoreResult<Option<ComposedMedium>> {
    let Some(path) = supplied.as_ref().filter(|p| !p.as_os_str().is_empty()) else {
        return match required {
            Some(medium) => Err(CoreError::SafetyRefused(format!(
                "'{package_id}''s installer verifies {} before it will do anything — it checks \
                 named files on a volume called '{}:'. Supply an image of your own copy of that \
                 disc. Without it the installer opens its window and never finishes, and ART \
                 would have nothing to report but a timeout.",
                medium.name, medium.volume
            ))),
            None => Ok(None),
        };
    };

    if !path.is_file() {
        return Err(CoreError::InvalidInput(format!(
            "the medium supplied for '{package_id}' is not a file: '{}'",
            path.display()
        )));
    }

    // The disc's own statement about itself. `IsoImage::open` reads a handful
    // of sectors — a 468 MB disc is not read to answer this.
    let image = IsoImage::open(path)?;
    let volume = image.volume_name().trim().to_string();

    if let Some(medium) = required {
        // AmigaDOS volume names are case-insensitive, so the comparison is
        // too — the same rule `core::amigainstall::claims_volume` follows.
        if !volume.eq_ignore_ascii_case(medium.volume.trim()) {
            return Err(CoreError::SafetyRefused(format!(
                "'{package_id}''s installer verifies {}, whose volume is '{}'. The image \
                 supplied — '{}' — states its volume as '{volume}', so it is not that disc and \
                 the installer would not find what it looks for.",
                medium.name,
                medium.volume,
                path.display()
            )));
        }
    }

    Ok(Some(ComposedMedium {
        path: path.clone(),
        volume,
        declared_as: required.map(|medium| medium.name.clone()),
    }))
}

/// The machine the installer runs on.
fn profile_for(id: Option<&str>) -> CoreResult<AmigaProfile> {
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
            if let Err(err) = staged.discard() {
                sink.report(
                    0,
                    None,
                    &format!("The cancelled run's copy could not be removed: {err}"),
                );
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
fn install(
    composed: &Composed,
    tree: &Path,
    package_archives: &[PathBuf],
    profile: &AmigaProfile,
    kickstart: &Path,
    emulator: &Path,
    sink: &dyn ProgressSink,
) -> CoreResult<(RunOutcome, SettlementReport)> {
    let plan = &composed.plan;
    let work = Scratch::new()?;
    workvol::build(work.path(), plan)?;

    // ART-185. Without this the installer the whole round exists to run is on
    // no volume the emulator can see.
    sink.report(0, None, "Unpacking the package's own files");
    let package = Scratch::new()?;
    let unpacked = packagevol::unpack(
        package_archives,
        package.path(),
        &packagevol::Layout {
            drawer: composed.package_dir.as_deref(),
            installer: &composed.installer_in_package,
            overlays: &composed.overlays,
            minimum_installer_version: composed.minimum_installer_version,
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
    // Said before the run, not only in the report afterwards: a run that
    // worked because a second archive patched the first is not the same run
    // as one of the first alone, and the user should be told which they got
    // (ART-186).
    for from in &unpacked.overlaid {
        sink.report(
            0,
            None,
            &format!("A second archive supplied '{from}' over the package's own files"),
        );
    }
    if let Some(version) = &unpacked.installer_version {
        sink.report(0, None, &format!("The installer states {version}"));
    }

    // Said before the run for the same reason the overlay line above is: a
    // run that worked because the user supplied the disc the installer asks
    // for is not the same run as one without it, and the volume named here is
    // the image's own, read from the image (ART-193).
    if let Some(medium) = &composed.medium {
        sink.report(
            0,
            None,
            &format!(
                "Mounting '{}' as the CD the installer checks for — it states its volume as '{}'",
                medium.path.display(),
                medium.volume
            ),
        );
    }

    perform(tree, sink, |copy, sink| {
        let request = RunRequest {
            plan,
            work_volume_dir: work.path(),
            tree_dir: copy,
            package_volume_dir: unpacked.root.as_path(),
            profile,
            kickstart_path: kickstart,
            winuae_path: emulator,
            cd_image: composed.medium.as_ref().map(|m| m.path.as_path()),
            limits: RunLimits::default(),
        };
        let outcome = run(&request, sink)?;
        record_if_succeeded(copy, plan, &outcome)?;
        Ok(outcome)
    })
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
        package_archives_present: !request.package_archives.is_empty()
            && request.package_archives.iter().all(|a| a.is_file()),
        declared_overlays: composed
            .overlays
            .iter()
            .map(|overlay| overlay.from.clone())
            .collect(),
        minimum_installer_version: composed.minimum_installer_version_text,
        medium: composed.medium.as_ref().map(|m| m.path.clone()),
        medium_volume: composed.medium.as_ref().map(|m| m.volume.clone()),
        required_medium: composed.medium.as_ref().and_then(|m| m.declared_as.clone()),
        package_archives: request.package_archives,
        package_dir: composed.package_dir,
        result_file: RESULT_FILE.to_string(),
        deadline_seconds: RunLimits::default().deadline.as_secs(),
        kickstart_present: request.kickstart.is_file(),
        kickstart: request.kickstart,
        emulator: detect_winuae(winuae_path.as_deref()).executable_path,
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
    let package_archives = request.package_archives.clone();
    let kickstart = request.kickstart.clone();
    let emulator = PathBuf::from(emulator);
    let log_path = oplog.path().to_path_buf();
    let emit_app = app.clone();
    let title = format!("Installing {} on the Amiga", composed.package_name);

    let for_log = tree.display().to_string();
    let command_line = command_line_of(&plan);
    let package_id = plan.package_id.clone();

    let id = spawn_job(
        &app,
        Arc::clone(&registry),
        &title,
        move |job_id, progress| {
            let result = install(
                &composed,
                &tree,
                &package_archives,
                &profile,
                &kickstart,
                &emulator,
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

    /// A synthetic disc stating the volume name a BoingBag's `Updater`
    /// verifies (ART-193), written beside `tree` — which is inside the
    /// caller's own scratch directory, so it goes away with it.
    ///
    /// **Assembled byte by byte, like every fixture in this project.** The
    /// disc the real run needs is the owner's own 468 MB AmigaOS 3.9 CD, and
    /// a test may not depend on one: ART ships no Amiga content, ever. What
    /// this proves is the half that is ART's — that a disc's *own* volume
    /// name is what gets compared — and the other half is the `#[ignore]`d
    /// hook, against the owner's real disc.
    fn disc_beside(tree: &Path, volume: &str) -> PathBuf {
        use crate::core::iso::fixture::{file as iso_file, IsoBuilder};

        let path = tree.parent().unwrap().join(format!("{volume}.iso"));
        let bytes = IsoBuilder {
            volume: volume.to_string(),
            children: vec![iso_file("ANGELS.AVI", "Angels.avi", b"synthetic")],
            ..Default::default()
        }
        .build();
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn request(tree: &Path) -> AmigaInstallRequest {
        AmigaInstallRequest {
            tree: tree.to_path_buf(),
            package_id: "boingbag-39-1".to_string(),
            system_volume: None,
            package_archives: vec![PathBuf::from("BoingBag39-1.lha")],
            package_dir: None,
            kickstart: PathBuf::from("kick.rom"),
            // Both BoingBags declare a `required_medium`, so every request
            // shaped like a real one carries the disc — a fixture that did
            // not would be testing ART's refusal instead of the thing the
            // test is named for.
            medium: Some(disc_beside(tree, "AmigaOS3.9")),
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
        let composed = compose(&request(&tree)).unwrap();

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
        let composed = compose(&request(&tree)).unwrap();

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

        let composed = compose(&req).unwrap();

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

        let composed = compose(&req).unwrap();

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

            let err = compose(&req).unwrap_err();

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
            let composed = compose(&req);
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
            let err = compose(&req).unwrap_err().to_string();
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

        let err = compose(&req).unwrap_err();

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

        let composed = compose(&request(&tree)).unwrap();
        let err = install(
            &composed,
            &tree,
            std::slice::from_ref(&archive),
            &AmigaProfile::a1200_aga(),
            Path::new("no-such.rom"),
            Path::new("no-such.exe"),
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

        let composed = compose(&request(&tree)).unwrap();
        let sink = Sink::default();
        let err = install(
            &composed,
            &tree,
            std::slice::from_ref(&archive),
            &AmigaProfile::a1200_aga(),
            Path::new("no-such.rom"),
            Path::new("no-such.exe"),
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

        let composed = compose(&request(&tree)).unwrap();
        let sink = Sink::default();
        let _ = install(
            &composed,
            &tree,
            std::slice::from_ref(&archive),
            &AmigaProfile::a1200_aga(),
            Path::new("no-such.rom"),
            Path::new("no-such.exe"),
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

        let preview = amiga_install_preview(request, Some("no-such.exe".into())).unwrap();

        assert_eq!(preview.program, "ARTPkg:BoingBag3.9-1/C/Updater");
        assert_eq!(preview.work_volume, WORK_VOLUME);
        assert_eq!(
            preview.package_volume, PACKAGE_VOLUME,
            "the user will see a third volume on the Workbench; say so"
        );
        assert!(
            !preview.package_archives_present,
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

        let err = compose(&req).unwrap_err();
        assert!(
            err.to_string().contains("BoingBag 3.9-1"),
            "the refusal must name what is missing: {err}"
        );

        // And the preview, which is the screen the user is actually looking
        // at, refuses the same way rather than describing the run.
        let err = amiga_install_preview(request_for(&tree, "boingbag-39-2"), None).unwrap_err();
        assert!(err.to_string().contains("BoingBag 3.9-1"), "got {err}");
    }

    /// The same package on a tree that has BoingBag 1 composes. A refusal that
    /// fired either way would be no check at all.
    #[test]
    fn boingbag_two_composes_once_the_tree_has_boingbag_one() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "chain-ok");
        let tree = tree_with_manifest(&scratch, &["workbench-base", "boingbag-39-1"]);

        let composed = compose(&request_for(&tree, "boingbag-39-2")).unwrap();
        assert_eq!(composed.plan.package_id, "boingbag-39-2");
    }

    /// ART-193, the refusal half. A BoingBag's `Updater` verifies the
    /// original AmigaOS 3.9 CD-ROM before it does anything, and without one
    /// it opens its own screen and never answers — measured three times
    /// against the owner's real tree, up to 1 200 s, with not one of 3 795
    /// files written. ART would then have had nothing to report but a
    /// timeout, which says *"nobody answered a question it asked"* about a
    /// program that never got as far as asking. So the run is refused before
    /// it starts, and the sentence names the disc and the volume.
    #[test]
    fn a_package_whose_installer_verifies_a_disc_is_refused_without_one() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "no-disc");
        let tree = tree_in(&scratch);
        let mut request = request(&tree);
        request.medium = None;

        let err = compose(&request).unwrap_err();
        assert!(matches!(err, CoreError::SafetyRefused(_)), "{err:?}");
        let text = err.to_string();
        assert!(text.contains("AmigaOS3.9"), "{text}");
        assert!(
            text.contains("AmigaOS 3.9 CD-ROM"),
            "the sentence names the disc, not just the volume: {text}"
        );
    }

    /// **The disc is asked what it is.** A filename is not an identity — this
    /// project shipped an AmigaOS 3.5 tree under the name 3.9 for reading one
    /// artefact's appearance as proof of what it was — so `compose` opens the
    /// image and compares the volume it *states* against the one the recipe
    /// declares.
    #[test]
    fn a_disc_that_states_another_volume_is_refused_and_the_message_names_both() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "wrong-disc");
        let tree = tree_in(&scratch);
        let mut request = request(&tree);
        request.medium = Some(disc_beside(&tree, "AmigaOS3.5"));

        let err = compose(&request).unwrap_err();
        assert!(matches!(err, CoreError::SafetyRefused(_)), "{err:?}");
        let text = err.to_string();
        assert!(text.contains("AmigaOS3.5"), "what was supplied: {text}");
        assert!(text.contains("AmigaOS3.9"), "and what is needed: {text}");
    }

    /// The other half: the right disc composes, and the volume that reaches
    /// the preview is the **image's own**, never the recipe's declaration
    /// echoed back.
    #[test]
    fn the_right_disc_composes_and_the_preview_names_the_volume_the_image_states() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "right-disc");
        let tree = tree_in(&scratch);

        let preview = amiga_install_preview(request(&tree), Some("no-such.exe".into())).unwrap();
        assert_eq!(preview.medium_volume.as_deref(), Some("AmigaOS3.9"));
        assert_eq!(
            preview.required_medium.as_deref(),
            Some("the original AmigaOS 3.9 CD-ROM")
        );
        assert!(preview.medium.is_some(), "and the screen says which file");
    }

    /// A disc reaches the emulator as a CD, and by the path the user chose.
    /// The three volumes ART mounts are directories; this is the fourth
    /// thing, and it is what the installer runs *against* rather than *from*.
    #[test]
    fn the_disc_reaches_the_generated_configuration_as_a_cd() {
        use crate::core::amigainstall::run::media_for;
        use crate::core::winuae::generate_uae_config;

        let scratch = ScratchDir::new("art-amigainstall-cmd", "cd-config");
        let tree = tree_in(&scratch);
        let request = request(&tree);
        let composed = compose(&request).unwrap();

        let work = scratch.join("work");
        workvol::build(&work, &composed.plan).unwrap();
        let package = scratch.join("pkg");
        std::fs::create_dir_all(&package).unwrap();
        let kickstart = scratch.join("kick.rom");
        std::fs::write(&kickstart, b"rom").unwrap();
        let profile = profile_for(None).unwrap();
        let disc = composed.medium.as_ref().unwrap().path.clone();

        let media = media_for(&RunRequest {
            plan: &composed.plan,
            work_volume_dir: &work,
            tree_dir: &tree,
            package_volume_dir: &package,
            profile: &profile,
            kickstart_path: &kickstart,
            winuae_path: Path::new("winuae64.exe"),
            cd_image: Some(&disc),
            limits: RunLimits::default(),
        })
        .unwrap();

        assert_eq!(
            media.cd_image_path.as_deref(),
            Some(disc.to_string_lossy().as_ref())
        );
        let config = generate_uae_config(&profile, &media).unwrap();
        assert!(config.contains("cdimage0="), "{config}");
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

        assert!(compose(&request).is_err());

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

        let err = compose(&request(&tree)).unwrap_err().to_string();
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

            let Ok(composed) = compose(&request(&tree)) else {
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

    /// The recipe's overlay declaration reaches the unpack, translated into
    /// `core::amigainstall`'s own record rather than carried across as the
    /// recipe's type.
    #[test]
    fn the_recipes_overlay_and_minimum_version_reach_the_composed_run() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "overlay-decl");
        let tree = tree_in(&scratch);

        let composed = compose(&request(&tree)).unwrap();

        assert_eq!(composed.minimum_installer_version, Some((45, 15)));
        assert_eq!(
            composed.overlays,
            vec![packagevol::Overlay {
                from: "BoingBag3.9-1-UAE/BoingBag3.9-1".to_string(),
                to: String::new(),
            }]
        );

        let preview = amiga_install_preview(request(&tree), None).unwrap();
        assert_eq!(
            preview.declared_overlays,
            vec!["BoingBag3.9-1-UAE/BoingBag3.9-1".to_string()],
            "the screen can say what a second file would have to be"
        );
        assert_eq!(preview.minimum_installer_version.as_deref(), Some("45.15"));
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
        let composed = compose(&request(&tree)).unwrap();
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
        compose(&request_for(&tree, "boingbag-39-2")).unwrap();
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
        let composed = compose(&request(&tree)).unwrap();
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
                compose(&request_for(&tree, "boingbag-39-2")).is_err(),
                "{ending:?} must leave the chain shut"
            );
        }

        // The one that does, against the same tree, so the difference is the
        // ending and nothing else.
        record_if_succeeded(&tree, plan, &RunOutcome::Succeeded).unwrap();
        assert!(chain::applied(&tree).unwrap().contains("boingbag-39-1"));
        compose(&request_for(&tree, "boingbag-39-2")).unwrap();
    }

    /// A run whose *second* archive is missing is a run with nothing to run.
    ///
    /// The preview asks about **every** archive: with one present and one
    /// absent, an `any`-shaped check would report the run as ready and the
    /// user would meet the refusal after confirming.
    #[test]
    fn the_preview_asks_about_every_archive_not_just_the_first() {
        let scratch = ScratchDir::new("art-amigainstall-cmd", "preview-archives");
        let tree = tree_in(&scratch);
        let present = scratch.join("BoingBag39-1.lha");
        std::fs::write(&present, boingbag_lha()).unwrap();

        let mut req = request(&tree);
        req.package_archives = vec![present.clone()];
        assert!(
            amiga_install_preview(req, None)
                .unwrap()
                .package_archives_present,
            "one archive, and it is there"
        );

        let mut req = request(&tree);
        req.package_archives = vec![present, scratch.join("BoingBag39-1-UAE.lha")];
        assert!(
            !amiga_install_preview(req, None)
                .unwrap()
                .package_archives_present,
            "the second one is not, and the screen has to say so before the confirm button"
        );
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
        let archives: Vec<PathBuf> = packages
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .collect();

        let request = AmigaInstallRequest {
            tree: tree.clone(),
            package_id: package_id.clone(),
            system_volume: None,
            package_archives: archives.clone(),
            package_dir: None,
            kickstart: PathBuf::from(&rom),
            // The disc the package's own installer verifies (ART-193).
            // `ART_AMIGA_CD` is optional here rather than required: a package
            // that declares no `required_medium` needs none, and one that does
            // is refused by name when it is absent, which is itself a path
            // this hook exists to walk.
            medium: std::env::var("ART_AMIGA_CD").ok().map(PathBuf::from),
            profile: None,
        };

        let before = measure(&tree);
        println!("tree before: {} files, {} bytes", before.0, before.1);
        for archive in &archives {
            println!("archive: {}", archive.display());
        }

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
            &archives,
            &profile,
            &PathBuf::from(&rom),
            &PathBuf::from(&winuae),
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
