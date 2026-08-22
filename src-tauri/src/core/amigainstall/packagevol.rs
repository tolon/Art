//! The package's own files, unpacked to a host directory ART can mount.
//!
//! **This module exists because of ART-185.** [`run::media_for`] mounted the
//! distribution tree and ART's own work volume, and nothing anywhere put the
//! package's `Updater` where the generated script could reach it. A BoingBag's
//! wrapper cannot simply be placed into the tree beforehand — *not being
//! placeable on the host is the whole reason this round exists* (ART-166) — so
//! the wrapper is unpacked to a directory of ART's own and that directory
//! becomes the third mount.
//!
//! [`run::media_for`]: super::run::media_for
//!
//! ## Nothing here decrypts anything
//!
//! The wrapper is **plain LHA** and ART has always read it. What is encrypted
//! is the payload archive *inside* it — `AmigaOS-Update`, ZipCrypto, every one
//! of its 233 entries (measured in the content-layer round, three independent
//! ways) — and that stays encrypted: it is copied out as the opaque blob it
//! is, and the package's own `Updater` decrypts it on the Amiga, which is this
//! design's arrangement from the start and the owner's recorded decision. No
//! protection is bypassed and none is examined.
//!
//! ## What was measured, and what the fixtures therefore look like
//!
//! Read with 7-Zip 26.02 on 2026-08-21, against the owner's own archives in
//! `E:\amiga\Amigatolon\paketler`:
//!
//! ```text
//! BoingBag39-1.lha        1112 entries   1111 under `BoingBag3.9-1\`
//!                                          1 `BoingBag3.9-1.info`
//! BoingBag39-2.lha         112 entries    111 under `BoingBag3.9-2\`
//!                                          1 `BoingBag3.9-2.info`
//! Euro-Update.lha          106 entries    105 under `Euro-Update\`
//!                                          1 `Euro-Update.info`
//! ```
//!
//! Three facts fall out of that listing and every fixture in this file carries
//! all three, because a fixture tidier than the real thing is a test that
//! passes against the defect it was written for:
//!
//! 1. **The top level is not one directory.** It is a drawer *and its icon* —
//!    a plain file sitting beside it. So "the archive's single top-level
//!    entry" is not a thing that can be read, and nothing here tries to.
//! 2. **There are no directory entries at all.** Every row is a file with a
//!    `/`-bearing name; the drawers exist only implicitly. The extraction gate
//!    creates the parents, and [`unpack`] therefore asks the *filesystem*
//!    whether the drawer arrived rather than asking the archive's index.
//! 3. **Entry names are not ASCII.** `C\Catalogs\türkçe\Updater.catalog` and
//!    `português-brasil` are real rows. Nothing here may slice a `&str` by
//!    byte offset.
//!
//! ## What is *not* read from the archive
//!
//! The drawer's name and the installer's path inside it are **not** taken from
//! the archive. They are the shipped recipe's (`media` and
//! `amiga_installer.program` — see [`crate::core::osinstall::package`]), or a
//! caller's override which the command layer has already put through its four
//! gates. The design's rule is that nothing ART generates is assembled from a
//! string ART did not author, and the drawer name reaches a generated
//! AmigaDOS script.
//!
//! What the archive *is* asked is whether those names are true of it — after
//! the extraction, of the files that actually arrived. That is this project's
//! "ask the artefact what it is; never infer it" rule, and it is what turns
//! ART-185's silent shape into a refusal: an archive that carries no such
//! drawer, or a drawer that carries no such installer, is refused **before**
//! the emulator starts rather than discovered as a `CD` that failed and an
//! answer of "the installer said no" about a program that never ran.
//!
//! ## A second medium, and a version the program has to state (ART-186)
//!
//! The same sentence has a third way of arriving, and this one was found by
//! reading the owner's archives rather than the code. `BoingBag39-1.lha`'s
//! own `C/Updater` states `$VER: Updater 45.13 (3.4.2001)`, and 45.13 cannot
//! install a BoingBag under an emulator — which is exactly what this round
//! does. The fix shipped separately, as `BoingBag39-1-UAE.lha`, whose readme
//! says so plainly: *"This archive contains a file, Updater 45.15, that fixes
//! the following problem: You can install the BoingBag on UAE now."*
//!
//! So [`unpack`] takes a **list** of archives, copies any declared overlay
//! subtree over the package's drawer, and then asks the installer what it is.
//! A program older than the recipe's declared minimum is refused here, with
//! the archive to supply named — never launched to fail and be reported as
//! the package refusing (§89).
//!
//! **The version, never the size.** 25 588 bytes is consistent with any build
//! that happens to be that long; `$VER:` is the file's own statement about
//! itself. Reading a coincidence as proof is how this project once shipped an
//! AmigaOS 3.5 tree under the name 3.9.

use std::path::{Path, PathBuf};

use crate::core::amigaver::{self, AmigaVersion};
use crate::core::archive::extract::{extract_with_backend, OverwritePolicy};
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::security::safe_join;
use crate::core::sources::install::Scratch;

/// How many names a refusal lists when it says what the archive actually
/// carried. Enough to recognise a wrong archive, bounded so that a hostile one
/// with a hundred thousand entries cannot write its whole index into an error
/// message the UI then renders.
const NAMES_IN_A_REFUSAL: usize = 8;

/// How much of the installer this module reads looking for its `$VER:`
/// marker.
///
/// The same bound `core::osinstall::collide` uses for the same question, and
/// for the same reason: a marker sits within the first few hundred bytes of
/// every real Amiga program (measured — 505 bytes into the owner's
/// `BoingBag39-1.lha` `Updater`, 537 into the other two), and never reading
/// further is what keeps this safe against a file that is not one.
const VERSION_SEARCH_BOUND: u64 = 1024 * 1024;

/// One overlay medium: which subtree of its own archive is copied, and where
/// under the package's drawer it lands.
///
/// **This module's own record, not `core::osinstall::package::
/// InstallerOverlay`.** `core/amigainstall` knows nothing about recipes and
/// must not start to — CLAUDE.md's rule for a lower-level module that needs
/// a higher-level one's data is that it declares what it reads and the
/// command layer maps between the two, which is what
/// `commands/amigainstall.rs` does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlay {
    /// `/`-separated, from the overlay archive's own root — including that
    /// archive's own top-level drawer, because a real overlay is not shaped
    /// like the package it patches. See [`unpack`].
    pub from: String,
    /// `/`-separated, inside the package's drawer. Empty means the drawer
    /// itself.
    pub to: String,
}

/// Which of a package's archives this one is, judged by the single top-level
/// drawer it carries.
///
/// **ART-200/ART-201.** ART tells a user to go and fetch a package's *update*
/// archive by name (`BoingBag39-1-UAE.lha` — its own `Updater` is 45.13, which
/// cannot install under an emulator), and the user brings it back and supplies
/// it as the package. Before this existed the answer was a generic drawer
/// mismatch that listed what the archive held and never said the one thing
/// that ends the problem: *this is the update archive, it goes in the other
/// field*. ART had the fact in hand the whole time — the recipe declares the
/// overlay's own drawer.
///
/// Judged from the archive's top-level name alone, so it can be asked
/// **before** anything is unpacked. That is what lets the preview answer it
/// (ART-201): `amiga_install_preview` used to describe a run — package, tree,
/// emulator, disc, machine — that `unpack` would refuse a moment later, which
/// is a confident wrong sentence in the shape of a summary card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveIs {
    /// Its top-level drawer is the one the installer lives in.
    ThePackage,
    /// Its top-level drawer is the first segment of one of this package's own
    /// declared overlays — so it is the update archive, in the wrong field.
    TheUpdateArchive,
    /// Neither. It may be another package's archive, or not a package at all.
    Neither,
}

/// Compare two Amiga drawer names.
///
/// Case-insensitively, because AmigaDOS is. `to_lowercase` rather than
/// `eq_ignore_ascii_case` so a non-ASCII drawer name in some future recipe is
/// folded rather than silently mismatched — a mismatch here would refuse a
/// *valid* archive, which is worse than the message this function exists to
/// improve.
///
/// **Not the authoritative check.** `unpack` still resolves the drawer on the
/// real filesystem and asks `is_dir`, which is the same question the mount
/// will put to the emulator. This one only decides which sentence to say.
fn drawer_names_equal(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// The drawer an overlay is copied *from* — its archive's own top level.
fn overlay_drawer(overlay: &Overlay) -> &str {
    overlay.from.split('/').next().unwrap_or("")
}

/// See [`ArchiveIs`].
pub fn archive_is(media: &str, overlays: &[Overlay], top_level: &str) -> ArchiveIs {
    if drawer_names_equal(media, top_level) {
        return ArchiveIs::ThePackage;
    }
    if overlays
        .iter()
        .any(|overlay| drawer_names_equal(overlay_drawer(overlay), top_level))
    {
        return ArchiveIs::TheUpdateArchive;
    }
    ArchiveIs::Neither
}

/// What to say about an archive supplied as the package's own when it is not.
///
/// English, like every other `CoreError` sentence (ART-060) — the screen adds
/// the half it can say in the user's language. **Actionable**, which is
/// CLAUDE.md's rule and the whole of ART-200: a mistake the user can undo by
/// moving one file between two fields must not read like one they cannot fix.
pub fn wrong_archive_sentence(
    archive: &std::path::Path,
    media: &str,
    role: &ArchiveIs,
    holds: &str,
) -> String {
    match role {
        ArchiveIs::TheUpdateArchive => format!(
            "'{}' is this package's update archive, not the package itself — it carries '{holds}'.              Supply it in the update-archive field instead, and put the archive carrying '{media}'              in the package's own field.",
            archive.display()
        ),
        _ => format!(
            "'{}' carries no '{media}' drawer, so it is not the archive this package's installer              lives in; it holds {holds}",
            archive.display()
        ),
    }
}

/// What the package is expected to look like once it is unpacked, and what
/// must be true of it before the emulator is started.
///
/// A struct rather than five positional arguments: `drawer` and `installer`
/// are both `&str`-shaped strings whose order nothing but their names
/// distinguishes, and swapping them silently would produce exactly the
/// "refused about the wrong thing" message this module exists to avoid.
#[derive(Debug, Clone)]
pub struct Layout<'a> {
    /// The package's own drawer inside the wrapper (`BoingBag3.9-1`),
    /// `/`-separated, or `None` for an archive whose files are at its root.
    pub drawer: Option<&'a str>,
    /// The program's path **inside that drawer** (`C/Updater`).
    pub installer: &'a str,
    /// Overlay media, applied in this order, after the wrapper and before the
    /// installer is looked for.
    pub overlays: &'a [Overlay],
    /// The lowest `version.revision` the installer's own `$VER:` marker may
    /// state, or `None` when no build of it is known to be unfit.
    pub minimum_installer_version: Option<(u32, u32)>,
}

impl<'a> Layout<'a> {
    /// The ordinary case: one archive, no overlay, no version requirement.
    pub fn new(drawer: Option<&'a str>, installer: &'a str) -> Self {
        Self {
            drawer,
            installer,
            overlays: &[],
            minimum_installer_version: None,
        }
    }
}

/// What arrived, for the report and the log.
///
/// `refused` carries the entries the extraction gate would not write — a
/// traversal name, an over-sized claim, a declared size that was a lie. They
/// are **reported and not fatal**, exactly as every other extraction in ART
/// treats them: the guarantee that matters is that none of them was written,
/// and that guarantee is absolute whether or not this module reacts. What
/// would be dishonest is unpacking an archive that tried and saying nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unpacked {
    /// The directory the package now sits in — the host side of the mount.
    pub root: PathBuf,
    pub files: usize,
    pub bytes: u64,
    pub refused: Vec<String>,
    /// Each overlay medium actually applied, by the `from` path that
    /// identified it — reported and logged, because a run that only worked
    /// because a second archive patched the first should say so rather than
    /// look like a run of the first alone (ART-186).
    pub overlaid: Vec<String>,
    /// What the installer says about *itself* once every overlay has landed
    /// — `Updater 45.15`, or `None` for a program carrying no `$VER:` marker.
    /// The measurement, kept so the report can state it rather than assert
    /// that it was checked.
    pub installer_version: Option<String>,
}

/// Unpack `archives` into `into`, apply any overlay medium, and prove the
/// package is really in there and fit to run.
///
/// `archives[0]` is the package's **own wrapper**; every archive after it is
/// an overlay medium the user supplied, matched against `layout.overlays` by
/// what it actually carries rather than by its position, so a user who picks
/// two files in either order gets the same result.
///
/// `layout.installer` is the program's path **inside the package's drawer**,
/// and `layout.drawer` the drawer inside the wrapper. Both are the recipe's
/// or the command layer's, never the archive's — see the module
/// documentation.
///
/// `into` must be absent or **empty**. That is not tidiness: this writes a
/// whole archive, and a mistyped path pointing at something of the user's
/// would scatter a BoingBag through it. The same rule and the same reasoning
/// as [`super::workvol::build`].
///
/// ## The order of the four checks is the whole point (ART-186)
///
/// The wrapper is unpacked, **then** every overlay lands, **then** the
/// installer is looked for, **then** it is asked what version it is. Each of
/// the last three depends on the one before it, and moving any of them
/// earlier makes it answer about a package that is not the one the emulator
/// will see: the installer check would refuse an overlay-supplied program as
/// missing, and the version check would read the very build the overlay
/// exists to replace.
///
/// ## An overlay is copied, not extracted over
///
/// The obvious implementation — extract the second archive into `into` with
/// [`OverwritePolicy::Overwrite`] — was written down, then measured against
/// the owner's real `BoingBag39-1-UAE.lha`, and it does not work. That
/// archive's `Updater` is at `BoingBag3.9-1-UAE/BoingBag3.9-1/C/Updater`, one
/// drawer deeper than the `BoingBag3.9-1/C/Updater` it replaces, so an
/// overwrite pass would have written a second, parallel drawer and left the
/// old build exactly where it was: a run that looks patched, launches 45.13,
/// and reports that the installer said no.
///
/// So each overlay is extracted into a scratch directory of its own — through
/// the same one gate, so its entry names are bounded and contained before
/// anything of it is read — and the subtree the declaration names is then
/// copied over the package's drawer, **replacing** what is there. Replacing,
/// not skipping: [`OverwritePolicy::Skip`] applied to this would let the
/// *older* file win, which is the defect wearing the fix's clothes.
pub fn unpack(
    archives: &[PathBuf],
    into: &Path,
    layout: &Layout<'_>,
    scratch_root: &Path,
    sink: &dyn ProgressSink,
) -> CoreResult<Unpacked> {
    let Some((archive, extra)) = archives.split_first() else {
        return Err(CoreError::InvalidInput(
            "running a package's own installer needs the package's own archive, and none was \
             given"
                .to_string(),
        ));
    };
    // Every archive, before anything is written: a run refused halfway through
    // unpacking because the *second* file the user picked is not there would
    // leave a scratch directory full of a package nobody asked for, and would
    // say so one step later than it could have.
    for archive in archives {
        if !archive.is_file() {
            return Err(CoreError::InvalidInput(format!(
                "running a package's own installer needs the package's own archive; '{}' is not \
                 a file",
                archive.display()
            )));
        }
    }

    if into.exists() {
        let mut entries = std::fs::read_dir(into)?;
        if entries.next().is_some() {
            return Err(CoreError::SafetyRefused(format!(
                "'{}' already has contents; a package is unpacked into an empty directory",
                into.display()
            )));
        }
    }

    let outcome = extract_whole(archive, into, sink)?;

    let refused: Vec<String> = outcome
        .extracted
        .iter()
        .filter(|e| e.skipped)
        .map(|e| match &e.reason {
            Some(reason) => format!("{}: {reason}", e.source_path),
            None => e.source_path.clone(),
        })
        .collect();

    // Ask what arrived, rather than trusting either the recipe or the archive
    // index. `is_dir` follows the real filesystem, which is also the thing the
    // mount will show the Amiga — so this is the same question the emulator
    // would ask, asked before the emulator is started.
    let drawer_dir = match layout.drawer {
        Some(drawer) => resolve(into, drawer)?,
        None => into.to_path_buf(),
    };
    if !drawer_dir.is_dir() {
        return Err(CoreError::InvalidInput(format!(
            "'{}' carries no '{}' drawer, so it is not the archive this package's installer \
             lives in; it holds {}",
            archive.display(),
            layout.drawer.unwrap_or(""),
            what_it_holds(into)
        )));
    }

    let mut files = outcome.total_files;
    let mut bytes = outcome.total_bytes;
    let mut overlaid = Vec::new();
    for medium in extra {
        let (from, (added_files, added_bytes)) =
            apply_overlay(medium, &drawer_dir, layout, scratch_root, sink)?;
        files += added_files;
        bytes += added_bytes;
        overlaid.push(from);
    }

    // And the installer itself, **after** the overlays: the whole reason a
    // second medium exists is that it supplies a fit build of exactly this
    // program. Without this check the run reaches the emulator, the shell
    // fails to find the program, `If Warn` writes `failed`, and ART tells the
    // user the installer said no about a program that never started —
    // ART-185's own sentence, arriving from one directory deeper.
    let program = resolve(&drawer_dir, layout.installer)?;
    if !program.is_file() {
        return Err(CoreError::InvalidInput(format!(
            "'{}' carries no '{}' inside its '{}' drawer, so there is nothing for the Amiga to \
             run",
            archive.display(),
            layout.installer,
            layout.drawer.unwrap_or("")
        )));
    }

    // Last: ask the program what it is. Never its size — see
    // `core::osinstall::package::AmigaInstaller::minimum_version`.
    let stated = read_installer_version(&program)?;
    if let Some((least_version, least_revision)) = layout.minimum_installer_version {
        let wanted = AmigaVersion {
            name: String::new(),
            version: least_version,
            revision: least_revision,
        };
        match &stated {
            Some(found) if found.compare_version(&wanted) != std::cmp::Ordering::Less => {}
            Some(found) => {
                return Err(CoreError::SafetyRefused(format!(
                    "'{}' carries {} {}.{}, and this package's installer has to be at least \
                     {least_version}.{least_revision} to run inside an emulator. Supply the \
                     package's update archive as well — the one carrying {} — and ART will copy \
                     it over. Running the older build would fail and look like the package \
                     refusing.",
                    archive.display(),
                    found.name,
                    found.version,
                    found.revision,
                    expected_overlays(layout)
                )));
            }
            None => {
                return Err(CoreError::SafetyRefused(format!(
                    "'{}' inside '{}' states no $VER: version, so ART cannot tell whether it is \
                     a build that works inside an emulator ({least_version}.{least_revision} or \
                     newer); it will not launch a program it cannot identify",
                    layout.installer,
                    archive.display()
                )));
            }
        }
    }

    Ok(Unpacked {
        root: into.to_path_buf(),
        files,
        bytes,
        refused,
        overlaid,
        installer_version: stated.map(|v| format!("{} {}.{}", v.name, v.version, v.revision)),
    })
}

/// Extract one archive whole, or refuse.
///
/// One gate, and it is not this module's. `core::archive::extract` owns
/// `safe_join`, the total and per-entry output caps, the entry cap, the
/// declared-size check and the overwrite policy, for every format ART reads.
/// A second extractor here would be a second copy of five defences, and the
/// copy is where the hole would be (`core::archive`'s own module
/// documentation says exactly this).
///
/// `Skip` rather than `Overwrite`: `dest` is always proved empty before this
/// is called, so nothing can legitimately be in the way, and an entry that
/// collides with one already written is an archive naming the same path twice
/// — which is reported rather than resolved by letting the later one win.
///
/// A partial unpack is the ART-185 shape wearing a different hat: the run
/// would start, the drawer might even be there, and whichever file the cap
/// cut off would be missing at the moment the Amiga reached for it. There is
/// no honest way to report that afterwards, so it is refused here.
fn extract_whole(
    archive: &Path,
    dest: &Path,
    sink: &dyn ProgressSink,
) -> CoreResult<crate::core::archive::extract::ExtractOutcome> {
    let mut backend = crate::core::archive::open(archive)?;
    let outcome = extract_with_backend(backend.as_mut(), dest, OverwritePolicy::Skip, sink)?;
    if outcome.aborted {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' was not unpacked whole and a partly-unpacked package must not be run: {}",
            archive.display(),
            outcome
                .abort_reason
                .unwrap_or_else(|| "the extraction stopped".to_string())
        )));
    }
    Ok(outcome)
}

/// Unpack one overlay medium and copy the subtree it was declared for over
/// the package's drawer. Returns the `from` that identified it, and how much
/// was copied.
fn apply_overlay(
    medium: &Path,
    drawer_dir: &Path,
    layout: &Layout<'_>,
    scratch_root: &Path,
    sink: &dyn ProgressSink,
) -> CoreResult<(String, (usize, u64))> {
    if layout.overlays.is_empty() {
        return Err(CoreError::InvalidInput(format!(
            "'{}' was given as a second archive, but this package declares no overlay medium — \
             ART runs what its own recipe names and nothing else",
            medium.display()
        )));
    }

    // Its own scratch directory, which removes itself on `Drop`: an overlay
    // archive's own files are not the package's, and unpacking them into the
    // mount would show the Amiga drawers nobody declared.
    let staging = Scratch::in_dir(scratch_root)?;
    extract_whole(medium, staging.path(), sink)?;

    // Which declared overlay is this? Asked of what the archive actually
    // carries, not of the order the user picked their files in.
    let mut matched = None;
    for overlay in layout.overlays {
        let candidate = resolve(staging.path(), &overlay.from)?;
        if candidate.is_dir() {
            matched = Some((overlay, candidate));
            break;
        }
    }
    let Some((overlay, source)) = matched else {
        return Err(CoreError::InvalidInput(format!(
            "'{}' is not this package's update archive: it carries none of {}; it holds {}",
            medium.display(),
            expected_overlays(layout),
            what_it_holds(staging.path())
        )));
    };

    let destination = if overlay.to.trim().is_empty() {
        drawer_dir.to_path_buf()
    } else {
        resolve(drawer_dir, &overlay.to)?
    };
    let copied = copy_over(&source, &destination, sink)?;
    Ok((overlay.from.clone(), copied))
}

/// The `from` paths a package's declared overlays are recognised by, for a
/// refusal that says what the user should have picked.
fn expected_overlays(layout: &Layout<'_>) -> String {
    layout
        .overlays
        .iter()
        .map(|o| format!("'{}'", o.from))
        .collect::<Vec<String>>()
        .join(" or ")
}

/// Copy every file under `from` into `to`, replacing what is already there.
///
/// An explicit stack rather than recursion: the depth comes from a directory
/// tree an archive produced, and a deep one must not go on the call stack.
///
/// Every destination goes through [`resolve`], so a name that arrived from an
/// archive cannot address anything outside `to` even though the extraction
/// gate has already contained it once — the containment is true *here* rather
/// than true elsewhere. Every write goes through `core::safety`, so a
/// half-written `Updater` is not a possible outcome.
///
/// **Those two `resolve` calls are defence in depth, and no test covers
/// them.** Measured in this round's mutation run: swapping either for
/// `Path::join` leaves the whole suite green, and it should — `relative` is
/// built from `std::fs::read_dir` entry names, and no filesystem returns
/// `..`, an absolute path, or a name containing a separator, so there is
/// nothing for the gate to catch here. It stays because CLAUDE.md's rule is
/// that `safe_join` is the only route from an archive entry name to a path
/// and these names did come from an archive; a future caller handing this a
/// differently-built `relative` is exactly what it is for. The two calls a
/// test **does** pin are the ones taking a *declared* path —
/// `apply_overlay`'s `from` and `to`, which a recipe or a caller supplies —
/// and both were caught.
fn copy_over(from: &Path, to: &Path, sink: &dyn ProgressSink) -> CoreResult<(usize, u64)> {
    let mut files = 0usize;
    let mut bytes = 0u64;
    // (directory to walk, its path relative to `from`, `/`-separated)
    let mut stack = vec![(from.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        // Between whole directories, never inside a write (CLAUDE.md).
        if sink.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let kind = entry.file_type()?;
            if kind.is_dir() {
                std::fs::create_dir_all(resolve(to, &relative)?)?;
                stack.push((entry.path(), relative));
            } else if kind.is_file() {
                // Bounded by the gate that wrote it: this file came out of
                // `extract_whole`, which caps both one entry's output and the
                // archive's total.
                let content = std::fs::read(entry.path())?;
                let destination = resolve(to, &relative)?;
                if let Some(parent) = destination.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                crate::core::safety::atomic::atomic_write(&destination, &content)?;
                files += 1;
                bytes += content.len() as u64;
            }
            // Anything that is neither is not something the extraction gate
            // creates, and copying one would be ART inventing an entry.
        }
    }
    Ok((files, bytes))
}

/// What the installer says about itself, read from a bounded window.
fn read_installer_version(program: &Path) -> CoreResult<Option<AmigaVersion>> {
    use std::io::Read as _;

    let file = std::fs::File::open(program)?;
    let mut window = Vec::new();
    file.take(VERSION_SEARCH_BOUND).read_to_end(&mut window)?;
    Ok(amigaver::read(&window))
}

/// A package-relative path, resolved under `root` through the same gate an
/// archive entry name goes through.
///
/// `safe_join` and not `Path::join`, even though these strings are ART's own:
/// a caller-supplied `package_dir` reaches here, and "it was validated
/// upstream" is the sentence in front of every traversal this project has
/// fixed. It costs one call and it makes the containment true here rather than
/// true elsewhere.
fn resolve(root: &Path, relative: &str) -> CoreResult<PathBuf> {
    safe_join(root, relative).map_err(|err| {
        CoreError::SafetyRefused(format!(
            "'{relative}' is not a path inside the package: {err}"
        ))
    })
}

/// The first few names directly inside `dir`, for a refusal that tells the
/// user which archive they actually pointed at.
fn what_it_holds(dir: &Path) -> String {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return "nothing that could be read".to_string();
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    if names.is_empty() {
        return "nothing".to_string();
    }
    names.sort();
    let more = names.len().saturating_sub(NAMES_IN_A_REFUSAL);
    names.truncate(NAMES_IN_A_REFUSAL);
    let listed = names.join(", ");
    if more > 0 {
        format!("{listed} and {more} more")
    } else {
        listed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------
    // ART-200 / ART-201: which archive is this, judged before anything is
    // unpacked.
    // -----------------------------------------------------------------

    #[test]
    fn the_packages_own_archive_is_recognised_by_its_drawer() {
        assert_eq!(
            archive_is("BoingBag3.9-1", &uae_overlay(), "BoingBag3.9-1"),
            ArchiveIs::ThePackage
        );
    }

    #[test]
    fn the_update_archive_is_recognised_as_the_update_archive() {
        // The whole of ART-200: ART names this archive to the user, the user
        // fetches it, and ART must not then refuse it as an unknown stranger.
        assert_eq!(
            archive_is("BoingBag3.9-1", &uae_overlay(), "BoingBag3.9-1-UAE"),
            ArchiveIs::TheUpdateArchive
        );
    }

    #[test]
    fn a_prefix_is_not_a_match_in_either_direction() {
        // `BoingBag3.9-1` is a character prefix of `BoingBag3.9-1-UAE`. A
        // comparison that did not require the whole name would call the
        // update archive the package, and the refusal would never fire.
        assert_ne!(
            archive_is("BoingBag3.9-1", &[], "BoingBag3.9-1-UAE"),
            ArchiveIs::ThePackage
        );
        assert_ne!(
            archive_is("BoingBag3.9-1-UAE", &[], "BoingBag3.9-1"),
            ArchiveIs::ThePackage
        );
    }

    #[test]
    fn another_packages_archive_is_neither() {
        assert_eq!(
            archive_is("BoingBag3.9-1", &uae_overlay(), "BoingBag3.9-2"),
            ArchiveIs::Neither
        );
    }

    #[test]
    fn drawer_names_are_compared_the_way_amigados_compares_them() {
        // AmigaDOS is case-insensitive, so an archive spelling its drawer
        // differently is still the package's own — refusing it would be a
        // false refusal, which is worse than the generic message this whole
        // change replaces.
        assert_eq!(
            archive_is("BoingBag3.9-1", &[], "boingbag3.9-1"),
            ArchiveIs::ThePackage
        );
    }

    #[test]
    fn the_update_archive_refusal_names_the_field_to_move_it_to() {
        // CLAUDE.md: a refusal must be actionable. This one is fixable by
        // moving one file between two fields, so the sentence has to say so.
        let said = wrong_archive_sentence(
            std::path::Path::new("E:\\dl\\BoingBag39-1-UAE.lha"),
            "BoingBag3.9-1",
            &ArchiveIs::TheUpdateArchive,
            "BoingBag3.9-1-UAE, BoingBag3.9-1-UAE.info",
        );
        assert!(
            said.contains("update archive"),
            "must name what the file is: {said}"
        );
        assert!(
            said.contains("update-archive field"),
            "must name the field it belongs in: {said}"
        );
        assert!(
            said.contains("BoingBag3.9-1"),
            "must name the drawer the right archive carries: {said}"
        );
    }

    #[test]
    fn an_unrecognised_archive_still_lists_what_it_held() {
        // The old sentence was not wrong, only incomplete — it stays for the
        // case where ART genuinely cannot say what the file is.
        let said = wrong_archive_sentence(
            std::path::Path::new("E:\\dl\\Euro-Update.lha"),
            "BoingBag3.9-1",
            &ArchiveIs::Neither,
            "Euro-Update, Euro-Update.info",
        );
        assert!(said.contains("carries no 'BoingBag3.9-1' drawer"), "{said}");
        assert!(said.contains("Euro-Update.info"), "{said}");
    }

    use crate::core::jobs::{CancelToken, NoProgress, ProgressSink};
    use crate::core::lha::tests::{make_lha_with, make_lha_with_raw_names};
    use crate::core::ScratchDir;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// ART-184: the directory removes itself on `Drop`, so a panicking test
    /// cleans up too.
    fn scratch(tag: &str) -> ScratchDir {
        ScratchDir::new("art-amigainstall-pkg", tag)
    }

    /// The one-archive, no-overlay call every test written before ART-186
    /// made. A helper rather than thirteen rewritten call sites: those tests
    /// are about the wrapper, the drawer, the traversal gate and the caps,
    /// and none of them changed.
    fn unpack_one(
        archive: &Path,
        into: &Path,
        drawer: Option<&str>,
        installer: &str,
        sink: &dyn ProgressSink,
    ) -> CoreResult<Unpacked> {
        unpack(
            std::slice::from_ref(&archive.to_path_buf()),
            into,
            &Layout::new(drawer, installer),
            &std::env::temp_dir(),
            sink,
        )
    }

    /// The **real** BoingBag layout, not a tidier one.
    ///
    /// Measured from the owner's `BoingBag39-1.lha` with 7-Zip 26.02 on
    /// 2026-08-21 and reproduced feature for feature: an icon file sitting
    /// beside the drawer at the top level, **no directory entries at all**, a
    /// non-ASCII catalog path, and the opaque encrypted payload blob next to
    /// the `Updater` that decrypts it.
    ///
    /// A fixture of `[("Pkg/C/Updater", …)]` alone would pass every test below
    /// while proving nothing about a real archive: the two things that would
    /// actually break — a sibling file at the top level, and drawers that
    /// exist only because a file name implies them — would both be absent.
    fn boingbag_shaped(drawer: &str) -> Vec<u8> {
        let icon = format!("{drawer}.info");
        let payload = format!("{drawer}/AmigaOS-Update");
        let updater = format!("{drawer}/C/Updater");
        let getlocale = format!("{drawer}/C/GetLocale");
        let install = format!("{drawer}/Install");
        let mut raw: Vec<(&[u8], &[u8])> = vec![
            (icon.as_bytes(), b"icon bytes"),
            // Stored, opaque, and never looked inside: the real one is a
            // ZipCrypto ZIP the Amiga-side Updater decrypts.
            (payload.as_bytes(), b"PK\x03\x04 encrypted payload"),
            (updater.as_bytes(), b"the updater program"),
            (getlocale.as_bytes(), b"getlocale"),
            (install.as_bytes(), b"; the package's own Installer script"),
        ];
        // `BoingBag3.9-1\C\Catalogs\türkçe\Updater.catalog`, in the Latin-1
        // the real archive stores. ART-168 was exactly this byte range.
        let mut catalog = Vec::new();
        catalog.extend_from_slice(drawer.as_bytes());
        catalog.extend_from_slice(b"/C/Catalogs/t\xFCrk\xE7e/Updater.catalog");
        raw.push((catalog.as_slice(), b"catalog"));
        make_lha_with_raw_names(&raw)
    }

    fn write_boingbag(at: &Path, drawer: &str) -> PathBuf {
        let archive = at.join(format!("{drawer}.lha"));
        std::fs::write(&archive, boingbag_shaped(drawer)).unwrap();
        archive
    }

    /// The whole point of the module: after this, the drawer and the program
    /// the script names are really on the host, so the mount has something to
    /// show the Amiga.
    #[test]
    fn a_real_shaped_wrapper_unpacks_and_its_updater_is_there() {
        let dir = scratch("happy");
        let archive = write_boingbag(dir.path(), "BoingBag3.9-1");
        let into = dir.join("pkg");

        let unpacked = unpack_one(
            &archive,
            &into,
            Some("BoingBag3.9-1"),
            "C/Updater",
            &NoProgress,
        )
        .unwrap();

        assert_eq!(unpacked.root, into);
        assert!(
            into.join("BoingBag3.9-1/C/Updater").is_file(),
            "the installer must be on the host, at the path the script will name"
        );
        assert!(
            into.join("BoingBag3.9-1/AmigaOS-Update").is_file(),
            "and its payload beside it, still encrypted"
        );
        assert_eq!(
            std::fs::read(into.join("BoingBag3.9-1/AmigaOS-Update")).unwrap(),
            b"PK\x03\x04 encrypted payload",
            "the payload is copied, never opened"
        );
        assert!(
            into.join("BoingBag3.9-1.info").is_file(),
            "the icon sits beside the drawer, as it does in the real archive"
        );
        assert_eq!(unpacked.files, 6);
        assert!(unpacked.refused.is_empty(), "{:?}", unpacked.refused);
    }

    /// The non-ASCII path survives. ART-168 was a name whose high-bit bytes
    /// became U+FFFD, and it survived every test in the suite because no
    /// fixture had one.
    #[test]
    fn a_latin_one_catalog_path_arrives_intact() {
        let dir = scratch("latin1");
        let archive = write_boingbag(dir.path(), "BoingBag3.9-1");
        let into = dir.join("pkg");

        unpack_one(
            &archive,
            &into,
            Some("BoingBag3.9-1"),
            "C/Updater",
            &NoProgress,
        )
        .unwrap();

        let catalogs: Vec<String> = std::fs::read_dir(into.join("BoingBag3.9-1/C/Catalogs"))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(catalogs, vec!["türkçe".to_string()], "got {catalogs:?}");
    }

    /// The wrong archive is refused, and the refusal **lists what was really
    /// in it** so the user can see which one they picked.
    ///
    /// The listing is the assertion, and deliberately so. Deleting the
    /// drawer check leaves the installer check to refuse this anyway — the
    /// program cannot be a file under a directory that is not there — and a
    /// test that only asserted `is_err()`, or that the message mentioned the
    /// two names, **passed against that deletion** when it was measured. What
    /// the drawer check is actually for is the diagnosis: `Euro-Update` and
    /// `Euro-Update.info` are what the archive holds, and only this branch
    /// says so. A user told "there is no `C/Updater` in `BoingBag3.9-1`" about
    /// an archive that contains neither has been told the wrong thing about
    /// the wrong drawer.
    #[test]
    fn an_archive_without_the_packages_drawer_is_refused_and_lists_what_it_held() {
        let dir = scratch("wrong-archive");
        let archive = write_boingbag(dir.path(), "Euro-Update");
        let into = dir.join("pkg");

        let err = unpack_one(
            &archive,
            &into,
            Some("BoingBag3.9-1"),
            "C/Updater",
            &NoProgress,
        )
        .unwrap_err();

        let text = err.to_string();
        assert!(text.contains("BoingBag3.9-1"), "got {text}");
        assert!(
            text.contains("Euro-Update.info") && text.contains("it holds"),
            "it must list the archive's own top level, which is the whole point of \
             diagnosing the drawer separately: {text}"
        );
    }

    /// The drawer being right is not enough. This is ART-185's own sentence
    /// one directory deeper: without the check the run starts, the shell
    /// cannot find the program, and ART reports that the installer said no.
    #[test]
    fn a_drawer_without_the_installer_is_refused_before_anything_is_launched() {
        let dir = scratch("no-updater");
        let archive = dir.join("BoingBag3.9-1.lha");
        std::fs::write(
            &archive,
            make_lha_with(&[
                ("BoingBag3.9-1.info", b"icon"),
                ("BoingBag3.9-1/AmigaOS-Update", b"payload"),
                ("BoingBag3.9-1/Install", b"script"),
            ]),
        )
        .unwrap();
        let into = dir.join("pkg");

        let err = unpack_one(
            &archive,
            &into,
            Some("BoingBag3.9-1"),
            "C/Updater",
            &NoProgress,
        )
        .unwrap_err();

        assert!(err.to_string().contains("C/Updater"), "got {err}");
    }

    /// A hostile entry name never leaves the unpack directory, and the
    /// refusal is reported rather than swallowed.
    ///
    /// The traversal entries are given the **shape of the drawer's own
    /// files**, so a gate that refused only obviously-foreign names would
    /// still have to refuse these.
    #[test]
    fn a_traversal_entry_is_refused_and_writes_nothing_outside() {
        let dir = scratch("traversal");
        let outside = dir.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("keep.txt"), b"the user's own file").unwrap();

        let archive = dir.join("hostile.lha");
        std::fs::write(
            &archive,
            make_lha_with(&[
                ("BoingBag3.9-1.info", b"icon"),
                ("BoingBag3.9-1/C/Updater", b"the updater"),
                ("../outside/keep.txt", b"overwritten"),
                ("BoingBag3.9-1/../../outside/planted", b"planted"),
                ("C:/Windows/System32/art.dll", b"planted"),
                ("/etc/passwd", b"planted"),
            ]),
        )
        .unwrap();
        let into = dir.join("pkg");

        let unpacked = unpack_one(
            &archive,
            &into,
            Some("BoingBag3.9-1"),
            "C/Updater",
            &NoProgress,
        )
        .unwrap();

        assert_eq!(
            std::fs::read(outside.join("keep.txt")).unwrap(),
            b"the user's own file",
            "nothing outside the unpack directory may be touched"
        );
        assert!(!outside.join("planted").exists());
        assert!(
            unpacked.refused.len() >= 3,
            "and the refusals must be reported, not swallowed: {:?}",
            unpacked.refused
        );
    }

    /// A hostile **drawer name** cannot reach out of the unpack directory.
    ///
    /// **The escape target really exists**, and that is what makes this
    /// mutation-proof. A test that only asserted `is_err()` passed with
    /// `safe_join` swapped for `Path::join` — `into/../../Windows` is
    /// nowhere, so the `is_dir` check refused it anyway and the traversal
    /// guard could have been deleted unnoticed. Here `../outside` is a real
    /// directory carrying a real `C/Updater`, so `Path::join` would resolve
    /// it, find the program, and hand the run a mount **outside** the
    /// directory ART unpacked into. Only the gate refuses it.
    #[test]
    fn a_drawer_that_leaves_the_unpack_directory_is_refused_even_when_it_exists() {
        let dir = scratch("hostile-drawer");
        let archive = write_boingbag(dir.path(), "BoingBag3.9-1");

        // The place a traversal would land, fully furnished.
        std::fs::create_dir_all(dir.join("outside").join("C")).unwrap();
        std::fs::write(dir.join("outside").join("C").join("Updater"), b"not ours").unwrap();

        for (n, hostile) in [
            "../outside",
            "BoingBag3.9-1/../../outside",
            "../../Windows",
            "C:/Windows",
            "/Windows",
            "   ",
        ]
        .into_iter()
        .enumerate()
        {
            let into = dir.join(format!("pkg-{n}"));
            let err = unpack_one(&archive, &into, Some(hostile), "C/Updater", &NoProgress);
            assert!(
                matches!(
                    err,
                    Err(CoreError::SafetyRefused(_)) | Err(CoreError::InvalidInput(_))
                ),
                "'{hostile}' must be refused, got {err:?}"
            );
        }
    }

    /// The same for the installer's own path, and the same reason for the
    /// furnished escape target: a recipe or an override that climbed out of
    /// the drawer would be naming a program the package does not carry, and
    /// with `Path::join` it would find one.
    #[test]
    fn an_installer_path_that_leaves_the_drawer_is_refused_even_when_it_exists() {
        let dir = scratch("hostile-installer");
        let archive = write_boingbag(dir.path(), "BoingBag3.9-1");
        std::fs::write(dir.join("Updater"), b"not ours").unwrap();

        for (n, hostile) in [
            // `into/BoingBag3.9-1/../../Updater` → `dir/Updater`, which is
            // really there.
            "../../Updater",
            "C:/Windows/System32/cmd.exe",
            "/bin/sh",
            "   ",
        ]
        .into_iter()
        .enumerate()
        {
            let into = dir.join(format!("pkg-{n}"));
            let err = unpack_one(&archive, &into, Some("BoingBag3.9-1"), hostile, &NoProgress);
            assert!(
                matches!(
                    err,
                    Err(CoreError::SafetyRefused(_)) | Err(CoreError::InvalidInput(_))
                ),
                "'{hostile}' must be refused, got {err:?}"
            );
        }
    }

    /// A directory with anything in it is never written into — the same rule
    /// as `workvol::build`, and for the same reason: a mistyped path pointing
    /// at the user's own tree would scatter a BoingBag through it.
    #[test]
    fn a_directory_with_contents_is_refused_rather_than_unpacked_into() {
        let dir = scratch("not-empty");
        let archive = write_boingbag(dir.path(), "BoingBag3.9-1");
        let into = dir.join("theirs");
        std::fs::create_dir_all(&into).unwrap();
        std::fs::write(into.join("Startup-Sequence"), b"the user's own").unwrap();

        let err = unpack_one(
            &archive,
            &into,
            Some("BoingBag3.9-1"),
            "C/Updater",
            &NoProgress,
        )
        .unwrap_err();

        assert!(matches!(err, CoreError::SafetyRefused(_)), "got {err:?}");
        assert_eq!(
            std::fs::read(into.join("Startup-Sequence")).unwrap(),
            b"the user's own",
            "and it is left exactly as it was"
        );
        assert_eq!(
            std::fs::read_dir(&into).unwrap().count(),
            1,
            "nothing was unpacked into it"
        );
    }

    /// An archive too big for the gate to unpack whole is refused, not run
    /// half-unpacked.
    ///
    /// A partially unpacked package is ART-185 wearing a different hat: the
    /// drawer might well be there, and whichever file the cap cut off would be
    /// missing at the moment the Amiga reached for it — reported afterwards as
    /// the installer having said no. `MAX_ENTRIES` is the cheapest of the
    /// gate's caps to reach honestly, and it aborts before anything is
    /// written at all, which is also asserted.
    #[test]
    fn an_archive_the_gate_will_not_unpack_whole_is_refused_rather_than_run() {
        use crate::core::archive::extract::MAX_ENTRIES;

        let dir = scratch("too-many");
        let names: Vec<String> = (0..=MAX_ENTRIES)
            .map(|n| format!("BoingBag3.9-1/f{n}"))
            .collect();
        let entries: Vec<(&str, &[u8])> =
            names.iter().map(|n| (n.as_str(), b"x" as &[u8])).collect();
        let archive = dir.join("huge.lha");
        std::fs::write(&archive, make_lha_with(&entries)).unwrap();
        let into = dir.join("pkg");

        let err = unpack_one(
            &archive,
            &into,
            Some("BoingBag3.9-1"),
            "C/Updater",
            &NoProgress,
        )
        .unwrap_err();

        assert!(matches!(err, CoreError::SafetyRefused(_)), "got {err:?}");
        assert!(
            err.to_string().contains("whole"),
            "the refusal must say why: {err}"
        );
        assert_eq!(
            std::fs::read_dir(&into).unwrap().count(),
            0,
            "and nothing may have been written"
        );
    }

    /// A missing archive is a sentence, not an unpack of nothing that then
    /// fails somewhere less obvious.
    #[test]
    fn a_missing_archive_is_refused_by_name() {
        let dir = scratch("missing");
        let err = unpack_one(
            &dir.join("nowhere.lha"),
            &dir.join("pkg"),
            Some("BoingBag3.9-1"),
            "C/Updater",
            &NoProgress,
        )
        .unwrap_err();
        assert!(err.to_string().contains("nowhere.lha"), "got {err}");
    }

    /// Cancellation reaches through: the extraction gate checks between whole
    /// entries and this must not swallow the answer.
    #[test]
    fn a_cancelled_unpack_stops_and_says_so() {
        struct StopAfter {
            seen: AtomicU32,
            after: u32,
            token: CancelToken,
        }
        impl ProgressSink for StopAfter {
            fn report(&self, _d: u64, _t: Option<u64>, _m: &str) {}
            fn is_cancelled(&self) -> bool {
                if self.seen.fetch_add(1, Ordering::Relaxed) + 1 > self.after {
                    self.token.cancel();
                }
                self.token.is_cancelled()
            }
        }

        let dir = scratch("cancel");
        let archive = write_boingbag(dir.path(), "BoingBag3.9-1");
        let into = dir.join("pkg");

        let sink = StopAfter {
            seen: AtomicU32::new(0),
            after: 2,
            token: CancelToken::new(),
        };
        let err =
            unpack_one(&archive, &into, Some("BoingBag3.9-1"), "C/Updater", &sink).unwrap_err();

        assert!(matches!(err, CoreError::Cancelled), "got {err:?}");
    }

    /// A package whose files are at the wrapper's root is expressible, and the
    /// installer check still applies to it.
    #[test]
    fn a_package_with_no_drawer_uses_the_unpack_root() {
        let dir = scratch("no-drawer");
        let archive = dir.join("flat.lha");
        std::fs::write(
            &archive,
            make_lha_with(&[("C/Updater", b"updater"), ("Readme", b"about")]),
        )
        .unwrap();
        let into = dir.join("pkg");

        let unpacked = unpack_one(&archive, &into, None, "C/Updater", &NoProgress).unwrap();
        assert_eq!(unpacked.root, into);
        assert!(into.join("C/Updater").is_file());

        let other = dir.join("pkg2");
        assert!(
            unpack_one(&archive, &other, None, "C/Installer", &NoProgress).is_err(),
            "the installer must still have to be there"
        );
    }

    // -----------------------------------------------------------------
    // ART-186: the overlay, and the version the installer has to state
    // -----------------------------------------------------------------

    /// The overlay declaration the shipped BoingBag 3.9-1 recipe carries,
    /// measured off the owner's own `BoingBag39-1-UAE.lha`.
    fn uae_overlay() -> Vec<Overlay> {
        vec![Overlay {
            from: "BoingBag3.9-1-UAE/BoingBag3.9-1".to_string(),
            to: String::new(),
        }]
    }

    /// An `Updater` that states its own version the way a real one does — the
    /// marker some way into the file, never at offset zero (505 bytes into the
    /// owner's `BoingBag39-1.lha` build, 537 into the other two).
    ///
    /// `tail` makes two builds of the same version distinguishable, so a test
    /// can tell "the overlay's file" from "the package's file" by more than
    /// the version number it is also asserting on.
    fn updater(version: u32, revision: u32, tail: &str) -> Vec<u8> {
        let mut bytes = vec![0u8; 480];
        bytes.extend_from_slice(b"\x00\x00\x03\xf3");
        bytes.extend_from_slice(
            format!("$VER: Updater {version}.{revision} (17.4.2001) {tail}").as_bytes(),
        );
        bytes.push(0);
        bytes
    }

    /// The owner's real wrapper, whose `Updater` is the stock 45.13 build.
    fn stock_wrapper(at: &Path) -> PathBuf {
        let archive = at.join("BoingBag39-1.lha");
        let updater = updater(45, 13, "stock");
        std::fs::write(
            &archive,
            make_lha_with(&[
                ("BoingBag3.9-1.info", b"icon"),
                (
                    "BoingBag3.9-1/AmigaOS-Update",
                    b"PK\x03\x04 encrypted payload",
                ),
                ("BoingBag3.9-1/C/Updater", updater.as_slice()),
                ("BoingBag3.9-1/C/GetLocale", b"only the wrapper has this"),
                ("BoingBag3.9-1/Install", b"; the package's own script"),
            ]),
        )
        .unwrap();
        archive
    }

    /// `BoingBag39-1-UAE.lha`, shaped the way the real one is — measured with
    /// 7-Zip 26.02 on 2026-08-21: seven entries, all under a
    /// `BoingBag3.9-1-UAE` top level, and the `Updater` **one drawer deeper**
    /// than in the package it patches, at
    /// `BoingBag3.9-1-UAE\BoingBag3.9-1\C\Updater`.
    fn uae_wrapper(at: &Path) -> PathBuf {
        let archive = at.join("BoingBag39-1-UAE.lha");
        let updater = updater(45, 15, "uae");
        std::fs::write(
            &archive,
            make_lha_with(&[
                ("BoingBag3.9-1-UAE.info", b"icon"),
                ("BoingBag3.9-1-UAE/BoingBag3.9-1.info", b"drawer icon"),
                (
                    "BoingBag3.9-1-UAE/BoingBag3.9-1/C/Updater",
                    updater.as_slice(),
                ),
                ("BoingBag3.9-1-UAE/Readme", b"Updater 45.15 fixes UAE"),
            ]),
        )
        .unwrap();
        archive
    }

    fn with_overlay<'a>(overlays: &'a [Overlay]) -> Layout<'a> {
        Layout {
            drawer: Some("BoingBag3.9-1"),
            installer: "C/Updater",
            overlays,
            minimum_installer_version: Some((45, 15)),
        }
    }

    /// The whole of ART-186's second half: the newer `Updater` replaces the
    /// older one, and ART can say so.
    ///
    /// **Every assertion here is one the defect would fail.** The two archives
    /// differ in the `Updater`'s own bytes *and* in its stated version, and
    /// both are asserted — an overlay test whose two archives differ only in
    /// a file the assertion never looks at is a test that passes against a
    /// copy that never happened, which is the trap this round was warned
    /// about. The wrapper's `C/GetLocale`, which the overlay does not carry,
    /// is asserted to survive: an overlay adds to the drawer, it does not
    /// replace it.
    #[test]
    fn an_overlay_replaces_the_packages_own_updater_and_the_older_build_loses() {
        let dir = scratch("overlay");
        let stock = stock_wrapper(dir.path());
        let uae = uae_wrapper(dir.path());
        let into = dir.join("pkg");
        let overlays = uae_overlay();

        let unpacked = unpack(
            &[stock, uae],
            &into,
            &with_overlay(&overlays),
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap();

        let landed = std::fs::read(into.join("BoingBag3.9-1/C/Updater")).unwrap();
        assert_eq!(
            landed,
            updater(45, 15, "uae"),
            "the overlay's own bytes, byte for byte"
        );
        assert_ne!(
            landed,
            updater(45, 13, "stock"),
            "and not the wrapper's — Skip rather than replace would leave this one here"
        );
        assert_eq!(
            unpacked.installer_version.as_deref(),
            Some("Updater 45.15"),
            "and ART reports what the program it will launch says it is"
        );
        assert_eq!(unpacked.overlaid, vec!["BoingBag3.9-1-UAE/BoingBag3.9-1"]);
        assert_eq!(
            std::fs::read(into.join("BoingBag3.9-1/C/GetLocale")).unwrap(),
            b"only the wrapper has this",
            "an overlay lands *over* the drawer; it does not replace it"
        );
        assert_eq!(
            std::fs::read(into.join("BoingBag3.9-1/AmigaOS-Update")).unwrap(),
            b"PK\x03\x04 encrypted payload",
            "and the payload is still the opaque blob nobody opened"
        );
    }

    /// The measurement that killed the obvious implementation.
    ///
    /// Extracting the overlay archive into the mount with
    /// `OverwritePolicy::Overwrite` — the design this round was handed — would
    /// have written a **parallel** `BoingBag3.9-1-UAE` drawer, because the
    /// real archive is one level deeper than the package it patches, and left
    /// 45.13 exactly where it was. Both halves are asserted: the parallel
    /// drawer is not on the mount, and the file that had to change did.
    #[test]
    fn the_overlays_own_drawer_never_reaches_the_mount() {
        let dir = scratch("overlay-shape");
        let stock = stock_wrapper(dir.path());
        let uae = uae_wrapper(dir.path());
        let into = dir.join("pkg");
        let overlays = uae_overlay();

        unpack(
            &[stock, uae],
            &into,
            &with_overlay(&overlays),
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap();

        assert!(
            !into.join("BoingBag3.9-1-UAE").exists(),
            "the overlay archive's own top level is ART's business, not the Amiga's"
        );
        assert!(
            !into.join("BoingBag3.9-1/BoingBag3.9-1").exists(),
            "and it is not nested inside the package either"
        );
        assert_eq!(
            std::fs::read(into.join("BoingBag3.9-1/C/Updater")).unwrap(),
            updater(45, 15, "uae")
        );
    }

    /// The refusal ART-186 asks for, in its own words: the owner's own copy of
    /// BoingBag 3.9-1 alone, with no second archive.
    ///
    /// Refused *before* anything is launched, and the message carries all
    /// three things a person needs — what they have, what is needed, and what
    /// to go and find.
    #[test]
    fn the_stock_updater_alone_is_refused_and_the_message_says_what_to_supply() {
        let dir = scratch("stock-alone");
        let stock = stock_wrapper(dir.path());
        let into = dir.join("pkg");
        let overlays = uae_overlay();

        let err = unpack(
            std::slice::from_ref(&stock),
            &into,
            &with_overlay(&overlays),
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap_err();

        assert!(matches!(err, CoreError::SafetyRefused(_)), "got {err:?}");
        let text = err.to_string();
        assert!(text.contains("45.13"), "what they have: {text}");
        assert!(text.contains("45.15"), "what is needed: {text}");
        assert!(
            text.contains("BoingBag3.9-1-UAE/BoingBag3.9-1"),
            "and what to go and find: {text}"
        );
    }

    /// A program that says nothing about itself is not launched either. "Ask
    /// the artefact what it is; never infer it" cuts both ways: a file that
    /// will not answer has not answered, and guessing from its size is the
    /// mistake this whole check exists instead of.
    #[test]
    fn an_installer_that_states_no_version_is_refused_when_a_minimum_is_declared() {
        let dir = scratch("silent-updater");
        let archive = dir.join("BoingBag39-1.lha");
        std::fs::write(
            &archive,
            make_lha_with(&[
                ("BoingBag3.9-1.info", b"icon"),
                // Exactly 25 588 bytes would be the *stock* size and 25 732 the
                // fixed one; neither is asked, and this file is neither.
                ("BoingBag3.9-1/C/Updater", b"a program that says nothing"),
            ]),
        )
        .unwrap();
        let into = dir.join("pkg");
        let overlays = uae_overlay();

        let err = unpack(
            std::slice::from_ref(&archive),
            &into,
            &with_overlay(&overlays),
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap_err();

        assert!(matches!(err, CoreError::SafetyRefused(_)), "got {err:?}");
        assert!(err.to_string().contains("$VER:"), "got {err}");
    }

    /// A build newer than the minimum is fine — the check is a floor, not an
    /// equality. BoingBag 3.9-2's own `Updater` is 45.19.
    #[test]
    fn a_newer_installer_than_the_minimum_is_accepted() {
        let dir = scratch("newer");
        let archive = dir.join("BoingBag39-1.lha");
        let newer = updater(45, 19, "later");
        std::fs::write(
            &archive,
            make_lha_with(&[
                ("BoingBag3.9-1.info", b"icon"),
                ("BoingBag3.9-1/C/Updater", newer.as_slice()),
            ]),
        )
        .unwrap();
        let into = dir.join("pkg");
        let overlays = uae_overlay();

        let unpacked = unpack(
            std::slice::from_ref(&archive),
            &into,
            &with_overlay(&overlays),
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap();
        assert_eq!(unpacked.installer_version.as_deref(), Some("Updater 45.19"));
        assert!(unpacked.overlaid.is_empty(), "and no overlay was needed");
    }

    /// The order of the checks, proved by a package whose own wrapper carries
    /// **no installer at all** and whose overlay supplies it. Looking for the
    /// program before the overlay landed would refuse this as missing.
    #[test]
    fn the_installer_is_looked_for_after_the_overlay_not_before() {
        let dir = scratch("order");
        let stock = dir.join("BoingBag39-1.lha");
        std::fs::write(
            &stock,
            make_lha_with(&[
                ("BoingBag3.9-1.info", b"icon"),
                ("BoingBag3.9-1/Install", b"; the package's own script"),
            ]),
        )
        .unwrap();
        let uae = uae_wrapper(dir.path());
        let into = dir.join("pkg");
        let overlays = uae_overlay();

        let unpacked = unpack(
            &[stock, uae],
            &into,
            &with_overlay(&overlays),
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap();

        assert!(into.join("BoingBag3.9-1/C/Updater").is_file());
        assert_eq!(unpacked.installer_version.as_deref(), Some("Updater 45.15"));
    }

    /// A second archive that is not the declared overlay is refused **by
    /// name**, listing what it actually held — never applied as though it
    /// were, and never silently ignored.
    #[test]
    fn a_second_archive_that_is_not_the_declared_overlay_is_refused() {
        let dir = scratch("wrong-overlay");
        let stock = stock_wrapper(dir.path());
        let other = dir.join("Euro-Update.lha");
        std::fs::write(
            &other,
            make_lha_with(&[
                ("Euro-Update.info", b"icon"),
                ("Euro-Update/C/Updater", b"a different package's updater"),
            ]),
        )
        .unwrap();
        let into = dir.join("pkg");
        let overlays = uae_overlay();

        let err = unpack(
            &[stock, other],
            &into,
            &with_overlay(&overlays),
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap_err();

        let text = err.to_string();
        assert!(
            text.contains("BoingBag3.9-1-UAE/BoingBag3.9-1"),
            "it must name what was expected: {text}"
        );
        assert!(
            text.contains("Euro-Update.info"),
            "and list what the archive really held: {text}"
        );
        assert_eq!(
            std::fs::read(into.join("BoingBag3.9-1/C/Updater")).unwrap(),
            updater(45, 13, "stock"),
            "and nothing of it may have been copied over the package"
        );
    }

    /// A package that declares no overlay does not get one. ART runs what its
    /// own recipe names; a second archive it cannot place is a refusal, not a
    /// silent extra mount.
    #[test]
    fn a_second_archive_for_a_package_that_declares_no_overlay_is_refused() {
        let dir = scratch("undeclared-overlay");
        let stock = stock_wrapper(dir.path());
        let uae = uae_wrapper(dir.path());
        let into = dir.join("pkg");

        let err = unpack(
            &[stock, uae],
            &into,
            &Layout::new(Some("BoingBag3.9-1"), "C/Updater"),
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap_err();

        assert!(err.to_string().contains("no overlay medium"), "got {err}");
    }

    /// An overlay whose declared destination is a traversal cannot reach out
    /// of the drawer it is copied into.
    ///
    /// **The assertions here were rewritten after a mutation run.** The first
    /// version asserted only `is_err()` and that a file outside was unchanged
    /// — and it **passed** with `resolve` swapped for `Path::join`, because
    /// `into/BoingBag3.9-1/../../outside` is a real directory, the copy landed
    /// there beside the file being asserted on, the drawer therefore still
    /// held 45.13, and the *version gate* refused the run. A test that agreed
    /// with the defect it was written for, exactly. So it now asserts which
    /// refusal fired, and that nothing at all was written outside.
    #[test]
    fn an_overlay_destination_that_leaves_the_drawer_is_refused() {
        let dir = scratch("overlay-traversal");
        let stock = stock_wrapper(dir.path());
        let uae = uae_wrapper(dir.path());
        // The place a traversal would land, fully furnished, so `Path::join`
        // would resolve it rather than fail on a path that is nowhere.
        std::fs::create_dir_all(dir.join("outside")).unwrap();
        std::fs::write(dir.join("outside").join("keep.txt"), b"the user's own").unwrap();
        let into = dir.join("pkg");

        let overlays = vec![Overlay {
            from: "BoingBag3.9-1-UAE/BoingBag3.9-1".to_string(),
            to: "../../outside".to_string(),
        }];
        let err = unpack(
            &[stock, uae],
            &into,
            &with_overlay(&overlays),
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("is not a path inside the package"),
            "the *traversal* must be what refused this, not the version gate \
             refusing the un-overlaid drawer a moment later: {err}"
        );
        let outside: Vec<String> = std::fs::read_dir(dir.join("outside"))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            outside,
            vec!["keep.txt".to_string()],
            "not one byte may have been written outside the package's own drawer"
        );
        assert_eq!(
            std::fs::read(dir.join("outside").join("keep.txt")).unwrap(),
            b"the user's own"
        );
    }

    /// And an overlay whose declared **source** leaves its own archive.
    ///
    /// An absolute path is the escape that matters here, because
    /// `Path::join(absolute)` discards the base entirely: a declaration of
    /// `C:/Windows` would resolve to `C:/Windows`, find it a real directory,
    /// and copy the host's own files over the package the emulator is about
    /// to run. The target below is a real, furnished directory for exactly
    /// that reason — a made-up one would be refused by the `is_dir` check
    /// whether or not the gate was there.
    #[test]
    fn an_overlay_source_that_leaves_its_own_archive_is_refused() {
        let dir = scratch("overlay-source-traversal");
        let stock = stock_wrapper(dir.path());
        let uae = uae_wrapper(dir.path());
        let planted = dir.join("elsewhere");
        std::fs::create_dir_all(planted.join("C")).unwrap();
        std::fs::write(planted.join("C").join("Updater"), b"not from any archive").unwrap();
        let into = dir.join("pkg");

        for hostile in [
            planted.to_string_lossy().replace('\\', "/"),
            "../elsewhere".to_string(),
            "/elsewhere".to_string(),
        ] {
            let overlays = vec![Overlay {
                from: hostile.clone(),
                to: String::new(),
            }];
            let into = into.join(hostile.replace([':', '/', '\\', '.'], "_"));
            let err = unpack(
                std::slice::from_ref(&stock),
                &into,
                &Layout {
                    drawer: Some("BoingBag3.9-1"),
                    installer: "C/Updater",
                    overlays: &overlays,
                    minimum_installer_version: None,
                },
                &std::env::temp_dir(),
                &NoProgress,
            );
            // With no overlay archive given, nothing is applied and the run is
            // fine — the declaration only bites when a medium is supplied.
            assert!(err.is_ok(), "{hostile}: {err:?}");

            let into = into.join("with-overlay");
            let err = unpack(
                &[stock.clone(), uae.clone()],
                &into,
                &Layout {
                    drawer: Some("BoingBag3.9-1"),
                    installer: "C/Updater",
                    overlays: &overlays,
                    minimum_installer_version: None,
                },
                &std::env::temp_dir(),
                &NoProgress,
            )
            .unwrap_err();
            assert!(
                err.to_string().contains("is not a path inside the package"),
                "'{hostile}' must be refused by the gate: {err}"
            );
        }
    }

    /// An archive list with nothing in it is a sentence, not a panic on
    /// `[0]`.
    #[test]
    fn no_archive_at_all_is_refused_by_name() {
        let dir = scratch("no-archives");
        let err = unpack(
            &[],
            &dir.join("pkg"),
            &Layout::new(Some("BoingBag3.9-1"), "C/Updater"),
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap_err();
        assert!(err.to_string().contains("none was given"), "got {err}");
    }

    /// A missing *second* archive is refused before the first is unpacked —
    /// so a run the user's own file list cannot satisfy writes nothing at all.
    #[test]
    fn a_missing_overlay_archive_is_refused_before_anything_is_unpacked() {
        let dir = scratch("missing-overlay");
        let stock = stock_wrapper(dir.path());
        let into = dir.join("pkg");
        let overlays = uae_overlay();

        let err = unpack(
            &[stock, dir.join("nowhere.lha")],
            &into,
            &with_overlay(&overlays),
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap_err();

        assert!(err.to_string().contains("nowhere.lha"), "got {err}");
        assert!(
            !into.exists() || std::fs::read_dir(&into).unwrap().count() == 0,
            "and nothing was unpacked"
        );
    }

    /// **The measurement, re-runnable.** Reads the owner's own three archives
    /// through the same [`unpack`] the run uses and asserts what each one's
    /// `C/Updater` states about itself.
    ///
    /// Recorded here rather than only in a report, because the recipe's
    /// `minimum_version: "45.15"` is worth exactly as much as the reading it
    /// came from, and the AmigaOS 3.9 release recipe once shipped fourteen
    /// paths nobody had read out of the medium.
    ///
    /// Measured 2026-08-21 against `E:\amiga\Amigatolon\os39`:
    ///
    /// ```text
    /// BoingBag39-1.lha      C/Updater  25 588  2001-04-03  $VER: Updater 45.13 (3.4.2001)
    /// BoingBag39-1-UAE.lha  C/Updater  25 732  2001-04-17  $VER: Updater 45.15 (17.4.2001)
    /// BoingBag39-2.lha      C/Updater  42 676  2001-11-09  $VER: Updater 45.19 (9.11.2001)
    /// ```
    #[test]
    #[ignore = "reads the owner's real BoingBag archives: set ART_OS39_FOLDER and run with --ignored --nocapture"]
    fn the_owners_real_updaters_state_the_versions_this_recipe_relies_on() {
        // **A skipped run must not read as a passed one.** With no folder this
        // test prints `ok` exactly like one that read all three archives, so
        // it says which it was.
        let Ok(folder) = std::env::var("ART_OS39_FOLDER") else {
            eprintln!("SKIPPED: ART_OS39_FOLDER is not set, so nothing was read");
            return;
        };
        let folder = std::path::PathBuf::from(folder);
        let dir = scratch("real-archives");

        for (n, (file, drawer, installer, expected)) in [
            (
                "BoingBag39-1.lha",
                "BoingBag3.9-1",
                "C/Updater",
                "Updater 45.13",
            ),
            (
                "BoingBag39-1-UAE.lha",
                "BoingBag3.9-1-UAE/BoingBag3.9-1",
                "C/Updater",
                "Updater 45.15",
            ),
            (
                "BoingBag39-2.lha",
                "BoingBag3.9-2",
                "C/Updater",
                "Updater 45.19",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let archive = folder.join(file);
            assert!(archive.is_file(), "{} is not in {}", file, folder.display());
            let into = dir.join(format!("real-{n}"));
            let unpacked = unpack(
                std::slice::from_ref(&archive),
                &into,
                &Layout::new(Some(drawer), installer),
                &std::env::temp_dir(),
                &NoProgress,
            )
            .unwrap_or_else(|err| panic!("{file}: {err}"));
            assert_eq!(
                unpacked.installer_version.as_deref(),
                Some(expected),
                "{file} must state {expected}"
            );
            eprintln!("{file}: {}", unpacked.installer_version.unwrap());
        }

        // And the two halves of ART-186 against the real material: the owner's
        // own BoingBag 3.9-1 alone is refused, and the pair is not.
        let stock = folder.join("BoingBag39-1.lha");
        let uae = folder.join("BoingBag39-1-UAE.lha");
        let overlays = uae_overlay();

        let err = unpack(
            std::slice::from_ref(&stock),
            &dir.join("real-alone"),
            &with_overlay(&overlays),
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap_err();
        eprintln!("stock alone: {err}");
        assert!(err.to_string().contains("45.13"), "got {err}");

        let paired = unpack(
            &[stock, uae],
            &dir.join("real-pair"),
            &with_overlay(&overlays),
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap();
        assert_eq!(paired.installer_version.as_deref(), Some("Updater 45.15"));
        eprintln!("with the UAE overlay: {:?}", paired.installer_version);
    }
}
