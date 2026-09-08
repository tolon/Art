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
pub fn drawer_names_equal(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// One package ART's catalogue ships a recipe for, carrying only what a
/// refusal or a classification needs to name it — never
/// `core::osinstall::package::Package`.
///
/// **This module's own record, never `core::osinstall::package::Package`.**
/// CLAUDE.md's
/// inward-dependency rule: `core/amigainstall` must not read recipes, so
/// `commands/amigainstall.rs` maps its catalogue into this before calling
/// in — ART-277, where the wrong-archive refusal had to say *"this is
/// BoingBag 3.9-2's own archive"* while BoingBag 3.9-1 was still selected,
/// which needs every other shipped package's name and media, not only the
/// one this run is composing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownPackage {
    pub id: String,
    pub name: String,
    pub media: String,
}

/// The archive's own top-level names, and its single identity drawer when it
/// has exactly one — read from **one** listing, so a caller wanting both
/// answers (`amigainstall_classify_archive` wants exactly this pair) opens
/// the archive once rather than twice (review finding 7). The wrapper is
/// plain LHA and this never extracts a byte, so the encrypted payload an
/// update archive might carry is never opened.
///
/// The identity is the same question [`archive_is`] answers, asked of the
/// raw listing rather than of one name already picked out: a root sibling
/// (an `.info` icon) does not count on its own, the same rule
/// `ArchiveSource::open` in `core::osinstall` uses for the same reason — it
/// is not what the archive is *named after*. `None` for an archive with none
/// or with more than one, which [`archive_is`] can only ever answer
/// `Neither` about anyway.
pub fn archive_listing(archive: &Path) -> CoreResult<(Vec<String>, Option<String>)> {
    let mut backend = crate::core::archive::open(archive)?;
    let entries = backend.entries()?;
    listing_from_entries(archive, &entries)
}

/// [`archive_listing`]'s own logic, over an already-fetched entry list —
/// split out so a test can hand it more entries than any real archive on
/// disk needs building to prove the bound (ART-277 re-review, I10), and so
/// the bound is checked exactly once regardless of caller.
fn listing_from_entries(
    archive: &Path,
    entries: &[crate::core::archive::ArchiveEntry],
) -> CoreResult<(Vec<String>, Option<String>)> {
    // `entries()` alone has no cap of its own — `MAX_ENTRIES` is enforced in
    // `extract_selection`, downstream of every *other* caller, but
    // `amigainstall_classify_archive` reaches a listing directly, from a raw
    // user-picked path, with no extraction behind it to cap it first.
    // Bounded reads is a `core/security` rule; the caller
    // (`amigainstall_classify_archive`) already turns any `Err` here into
    // `"unknown"`, so this is the one place that needs to refuse rather than
    // read.
    if entries.len() > crate::core::archive::extract::MAX_ENTRIES {
        return Err(CoreError::InvalidInput(format!(
            "'{}' declares {} entries — too many entries for ART to list (at most {})",
            archive.display(),
            entries.len(),
            crate::core::archive::extract::MAX_ENTRIES
        )));
    }
    let mut names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut identity: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for entry in entries {
        let mut parts = entry.name.split(['/', '\\']).filter(|s| !s.is_empty());
        let Some(first) = parts.next() else { continue };
        names.insert(first.to_string());
        if entry.is_dir || parts.next().is_some() {
            identity.insert(first.to_string());
        }
    }
    let identity = (identity.len() == 1).then(|| identity.into_iter().next().unwrap());
    Ok((names.into_iter().collect(), identity))
}

/// The archive's own top-level names alone. A thin wrapper over
/// [`archive_listing`], kept as its own pub function because it is unit
/// tested on its own; production code that wants both answers calls
/// [`archive_listing`] directly rather than through this and
/// [`archive_identity`] both, which would open the archive twice.
pub fn archive_top_level(archive: &Path) -> CoreResult<Vec<String>> {
    Ok(archive_listing(archive)?.0)
}

/// The archive's single identity drawer alone — see [`archive_listing`].
pub fn archive_identity(archive: &Path) -> CoreResult<Option<String>> {
    Ok(archive_listing(archive)?.1)
}

/// See [`ArchiveIs`].
pub fn archive_is(media: &str, top_level: &str) -> ArchiveIs {
    if drawer_names_equal(media, top_level) {
        return ArchiveIs::ThePackage;
    }
    ArchiveIs::Neither
}

/// What to say about an archive supplied as the package's own when it is not.
///
/// English, like every other `CoreError` sentence (ART-060) — the screen adds
/// the half it can say in the user's language. **Actionable**, which is
/// CLAUDE.md's rule and the whole of ART-200: a mistake the user can undo by
/// moving one file between two fields must not read like one they cannot fix.
///
/// `catalogue` is the command layer's release-scoped, self-excluding record
/// (ART-277 re-review, I11): the package field's own refusal used to say only
/// what the archive was *not*, with no catalogue lookup at all. `&[]` when the
/// caller has none to offer; the sentence then falls back to the shape it
/// always had.
///
/// **One shape, since 2026-09-08.** It used to `match` on [`ArchiveIs`] and
/// carry two more sentences — "this is the update archive, put it in the other
/// field" and its mirror — for the second archive slot the UAE-fix overlay
/// needed. That slot is gone with the overlay machinery, so the only way to
/// hand this function a wrong archive is to hand it one that is not the
/// package.
pub fn wrong_archive_sentence(
    archive: &std::path::Path,
    media: &str,
    holds: &str,
    catalogue: &[KnownPackage],
) -> String {
    let owner = catalogue
        .iter()
        .find(|pkg| drawer_names_equal(&pkg.media, holds));
    match owner {
        Some(pkg) => format!(
            "'{}' carries no '{media}' drawer, so it is not the archive this package's \
             installer lives in; it holds {holds}, which is {}'s own archive",
            archive.display(),
            pkg.name
        ),
        None => format!(
            "'{}' carries no '{media}' drawer, so it is not the archive this package's \
             installer lives in; it holds {holds}",
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
    /// The selected package's own display name — `"BoingBag 3.9-1"` — used
    /// only to make a wrong-overlay refusal name what is actually selected
    /// (ART-277). `""` for a caller that has not supplied one, which every
    /// call site before ART-277 is; a refusal falls back to the shape it has
    /// always had rather than printing an empty name.
    pub package_name: &'a str,
    /// Every *other* package the command layer considers reachable from
    /// here, as `(id, name, media, overlay_drawers)` only. **The lower
    /// module's own record** — see [`KnownPackage`]. `&[]` when the caller
    /// has none to offer; a wrong-overlay refusal then names only what the
    /// archive held, exactly as it did before ART-277.
    ///
    /// **What "reachable" means is the caller's contract, not this module's
    /// — stated exactly, not aspirationally (ART-277 re-review, L4).** The
    /// one production caller, `commands::amigainstall::compose`, builds this
    /// from every shipped package that shares a release with the one
    /// selected, with the selected package's own id already removed. Nothing
    /// here enforces either property — the drawer check in
    /// [`overlay_mismatch_sentence`] finds the selected package unconditionally
    /// regardless of whether it is in this list, and the catalogue scan
    /// refuses to guess when more than one entry matches — so a caller that
    /// does not keep this discipline gets a less specific sentence, never a
    /// wrong one.
    pub catalogue: &'a [KnownPackage],
}

impl<'a> Layout<'a> {
    /// The ordinary case: one archive, no overlay, no version requirement,
    /// no catalogue to name a mismatch against.
    pub fn new(drawer: Option<&'a str>, installer: &'a str) -> Self {
        Self {
            drawer,
            installer,
            package_name: "",
            catalogue: &[],
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
    /// What the installer says about *itself* — `Updater 45.15`, or `None`
    /// for a program carrying no `$VER:` marker.
    /// The measurement, kept so the report can state it rather than assert
    /// that it was checked.
    pub installer_version: Option<String>,
}

/// Unpack `archive` into `into` and prove the package is really in there.
///
/// `layout.installer` is the program's path **inside the package's drawer**,
/// and `layout.drawer` the drawer inside the wrapper. Both are the recipe's
/// or the command layer's, never the archive's -- see the module
/// documentation.
///
/// `into` must be absent or **empty**. That is not tidiness: this writes a
/// whole archive, and a mistyped path pointing at something of the user's
/// would scatter a BoingBag through it. The same rule and the same reasoning
/// as [`super::workvol::build`].
///
/// ## The order of the checks is the whole point
///
/// The wrapper is unpacked, **then** the drawer is looked for, **then** the
/// installer inside it, **then** it is asked what version it states. Each
/// depends on the one before it. Without the installer check the run reaches
/// the emulator, the shell fails to find the program, `If Warn` writes
/// `failed`, and ART tells the user the installer said no about a program that
/// never started.
///
/// **One archive, since 2026-09-08.** This used to take a list: the first the
/// package's own wrapper, every one after it an overlay medium copied over the
/// package's drawer before the installer was looked for (ART-186's UAE fix for
/// BoingBag 3.9-1's 45.13 `Updater`). Both BoingBags are placed from Windows
/// now, no shipped recipe declares an overlay, and the machinery went with the
/// route that used it.
pub fn unpack(
    archive: &Path,
    into: &Path,
    layout: &Layout<'_>,
    sink: &dyn ProgressSink,
) -> CoreResult<Unpacked> {
    if !archive.is_file() {
        return Err(CoreError::InvalidInput(format!(
            "running a package's own installer needs the package's own archive; '{}' is not \
             a file",
            archive.display()
        )));
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

    let files = outcome.total_files;
    let bytes = outcome.total_bytes;

    // The installer itself. Without this check the run reaches the emulator,
    // the shell fails to find the program, `If Warn` writes `failed`, and ART
    // tells the user the installer said no about a program that never started
    // -- ART-185's own sentence, arriving from one directory deeper.
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

    // Last: ask the program what it is, and record the answer. Never its size
    // -- a size is consistent with any build that happens to be that long.
    let stated = read_installer_version(&program)?;

    Ok(Unpacked {
        root: into.to_path_buf(),
        files,
        bytes,
        refused,
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

/// What a program says about **itself**, read from a bounded window of its
/// own bytes.
///
/// The one place that decides how much of a program is read and what counts
/// as a statement of version, so the two callers that ask the same question
/// of the same file cannot drift apart: [`unpack`], which has the program on
/// disk after an archive was extracted, and
/// `commands::osinstall::osinstall_slots`, which has the same program's
/// bytes straight out of the wrapper archive and never writes them anywhere.
/// A second `amigaver::read` with a second bound is how one of them would
/// start answering `None` where the other answers `Updater 45.15`.
///
/// **Never a size, never a date** — see
/// [`crate::core::osinstall::package::AmigaInstaller::minimum_version`] for
/// why the `$VER:` marker is the only acceptable evidence here.
pub fn stated_version(bytes: &[u8]) -> Option<AmigaVersion> {
    let bound = VERSION_SEARCH_BOUND as usize;
    amigaver::read(&bytes[..bytes.len().min(bound)])
}

/// What the installer says about itself, read from a bounded window.
fn read_installer_version(program: &Path) -> CoreResult<Option<AmigaVersion>> {
    use std::io::Read as _;

    let file = std::fs::File::open(program)?;
    let mut window = Vec::new();
    file.take(VERSION_SEARCH_BOUND).read_to_end(&mut window)?;
    Ok(stated_version(&window))
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
    match top_level_dir_names(dir) {
        Some(names) => format_holds(names),
        None => "nothing that could be read".to_string(),
    }
}

/// The names directly inside `dir` — `what_it_holds`'s own listing, split
/// out so a caller that also needs to *match* one of them (ART-277's
/// wrong-package refusal) does not read the directory twice. `None` only
/// when `dir` itself could not be read; an empty, readable directory is
/// `Some(vec![])`, which `format_holds` already renders as `"nothing"`.
fn top_level_dir_names(dir: &Path) -> Option<Vec<String>> {
    let entries = std::fs::read_dir(dir).ok()?;
    Some(
        entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect(),
    )
}

/// The first few of `names`, sorted, for a refusal that says what an archive
/// actually held without printing a hostile archive's entire index.
fn format_holds(mut names: Vec<String>) -> String {
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
            archive_is("BoingBag3.9-1", "BoingBag3.9-1"),
            ArchiveIs::ThePackage
        );
    }

    #[test]
    fn a_prefix_is_not_a_match_in_either_direction() {
        // `BoingBag3.9-1` is a character prefix of `BoingBag3.9-1-UAE`. A
        // comparison that did not require the whole name would call a
        // neighbouring archive the package, and the refusal would never fire.
        assert_ne!(
            archive_is("BoingBag3.9-1", "BoingBag3.9-1-UAE"),
            ArchiveIs::ThePackage
        );
        assert_ne!(
            archive_is("BoingBag3.9-1-UAE", "BoingBag3.9-1"),
            ArchiveIs::ThePackage
        );
    }

    #[test]
    fn another_packages_archive_is_neither() {
        assert_eq!(
            archive_is("BoingBag3.9-1", "BoingBag3.9-2"),
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
            archive_is("BoingBag3.9-1", "boingbag3.9-1"),
            ArchiveIs::ThePackage
        );
    }

    // -----------------------------------------------------------------
    // ART-277: classifying an archive from its listing alone, before
    // anything is unpacked.
    // -----------------------------------------------------------------

    #[test]
    fn archive_top_level_lists_the_drawer_and_its_sibling_icon() {
        let dir = scratch("top-level");
        let archive = write_boingbag(dir.path(), "BoingBag3.9-2");
        let names = archive_top_level(&archive).unwrap();
        assert_eq!(
            names,
            vec![
                "BoingBag3.9-2".to_string(),
                "BoingBag3.9-2.info".to_string()
            ],
            "got {names:?}"
        );
    }

    #[test]
    fn archive_identity_is_the_drawer_not_the_sibling_icon() {
        let dir = scratch("identity");
        let archive = write_boingbag(dir.path(), "BoingBag3.9-2");
        assert_eq!(
            archive_identity(&archive).unwrap().as_deref(),
            Some("BoingBag3.9-2")
        );
    }

    #[test]
    fn archive_identity_is_none_for_an_archive_with_no_single_top_level_directory() {
        let dir = scratch("no-identity");
        let archive = dir.join("loose.lha");
        std::fs::write(
            &archive,
            make_lha_with(&[("Readme", b"a file with nothing nested under it")]),
        )
        .unwrap();
        assert_eq!(archive_identity(&archive).unwrap(), None);
    }

    /// **ART-277 re-review, I10.** `entries()` has no cap of its own — only
    /// `extract_selection` enforces `MAX_ENTRIES`, and `archive_listing`
    /// reaches a listing with no extraction downstream of it to cap first.
    /// Built as a synthetic entry list rather than a real 100,001-entry
    /// archive on disk (`listing_from_entries` is exactly the split that
    /// makes this cheap to prove).
    #[test]
    fn a_listing_over_the_entry_cap_is_refused_rather_than_read() {
        let entries: Vec<crate::core::archive::ArchiveEntry> = (0
            ..=crate::core::archive::extract::MAX_ENTRIES)
            .map(|i| crate::core::archive::ArchiveEntry {
                name: format!("Pkg/f{i}.txt"),
                is_dir: false,
                declared_bytes: 1,
            })
            .collect();
        let err = listing_from_entries(Path::new("E:\\dl\\huge.lha"), &entries).unwrap_err();
        assert!(err.to_string().contains("too many entries"), "got {err}");
    }

    #[test]
    fn an_unrecognised_archive_still_lists_what_it_held() {
        // The old sentence was not wrong, only incomplete — it stays for the
        // case where ART genuinely cannot say what the file is.
        let said = wrong_archive_sentence(
            std::path::Path::new("E:\\dl\\Euro-Update.lha"),
            "BoingBag3.9-1",
            "Euro-Update, Euro-Update.info",
            &[],
        );
        assert!(said.contains("carries no 'BoingBag3.9-1' drawer"), "{said}");
        assert!(said.contains("Euro-Update.info"), "{said}");
    }

    /// **ART-277 re-review, I11.** The package field's own refusal used to
    /// say only what the archive was *not*, with no catalogue lookup at all
    /// — the asymmetry the review named against `apply_overlay`'s equivalent
    /// sentence for the second field, which already named a recognised
    /// archive's real owner. Given a catalogue, this one now does too.
    #[test]
    fn a_wrong_archive_in_the_package_field_names_its_real_owner_when_the_catalogue_knows_it() {
        let catalogue = [KnownPackage {
            id: "boingbag-39-2".to_string(),
            name: "BoingBag 3.9-2".to_string(),
            media: "BoingBag3.9-2".to_string(),
        }];
        // `holds` here is the single identity `MediaSource::volume_name`
        // states — the shape `refuse_wrong_package_archive` actually passes
        // in production, not the comma-joined display list the *other* two
        // tests in this group use to pin `Neither`'s generic wording.
        let said = wrong_archive_sentence(
            std::path::Path::new("E:\\dl\\BoingBag39-2.lha"),
            "BoingBag3.9-1",
            "BoingBag3.9-2",
            &catalogue,
        );
        assert!(said.contains("carries no 'BoingBag3.9-1' drawer"), "{said}");
        assert!(
            said.contains("which is BoingBag 3.9-2's own archive"),
            "must name the real owner: {said}"
        );
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
        unpack(archive, into, &Layout::new(drawer, installer), sink)
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

    /// **The measurement, re-runnable.** Reads the owner's own three archives
    /// through the same [`unpack`] the run uses and asserts what each one's
    /// `C/Updater` states about itself.
    ///
    /// Recorded here rather than only in a report, because the AmigaOS 3.9
    /// release recipe once shipped fourteen paths nobody had read out of the
    /// medium.
    ///
    /// Measured 2026-08-21 against `E:\amiga\Amigatolon\os39`:
    ///
    /// ```text
    /// BoingBag39-1.lha      C/Updater  25 588  2001-04-03  $VER: Updater 45.13 (3.4.2001)
    /// BoingBag39-1-UAE.lha  C/Updater  25 732  2001-04-17  $VER: Updater 45.15 (17.4.2001)
    /// BoingBag39-2.lha      C/Updater  42 676  2001-11-09  $VER: Updater 45.19 (9.11.2001)
    /// ```
    ///
    /// The UAE archive is still read, and it is no longer an *overlay*: ART
    /// stopped applying one on 2026-09-08 with the emulator route for the two
    /// BoingBags. It is read here as an archive in its own right, because the
    /// three readings are what the version reader is checked against.
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
                &archive,
                &into,
                &Layout::new(Some(drawer), installer),
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
    }
}
