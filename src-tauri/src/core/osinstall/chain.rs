//! What a tree has already had applied to it, and what a package needs first.
//!
//! **This module exists because of ART-186.** The packages are a chain: a
//! clean AmigaOS 3.9, then BoingBag 1, then BoingBag 2, and only then the
//! optional community BoingBag 3 and 4 — whose own release states its
//! requirement as "AmigaOS 3.9+BB2". Practitioners agree from experience:
//! one on forum.amiga.org reports installing "BB 1-4, one right after the
//! other, all in a row".
//!
//! Nothing enforced it. A run of BoingBag 2 against a tree BoingBag 1 had
//! never touched was accepted, and the result **boots and is quietly wrong**
//! — which is the same failure this project already produced once, when a
//! tree that booted cleanly turned out to be AmigaOS 3.5 rather than 3.9. A
//! wrong system that starts is worse than one that refuses, because nothing
//! tells the user which one they have.
//!
//! ## The tree already carries the answer, and it carries half of it
//!
//! [`DistributionManifest`] records, file by file, which component and which
//! medium every byte came from, so the components a tree was built from can
//! simply be read back. That is the half that was already there.
//!
//! The other half is new, and leaving it out would have made the refusal
//! *worse* than no refusal at all. A BoingBag's payload is ZipCrypto-
//! encrypted and ART cannot place one from the host by any route (ART-166) —
//! that is the whole reason the Amiga-side round exists — so BoingBag 1
//! never appears among a tree's file records, and a refusal reading only
//! those would have refused BoingBag 2 for ever, on a tree that really did
//! have BoingBag 1 installed. So a successful Amiga-side run records itself:
//! [`record_amiga_install`], read back by [`applied`].
//!
//! **What it records is only what ART can vouch for.** An Amiga Installer is
//! a program ART did not write and cannot supervise per file; it does not
//! know which files were written or what they displaced. Writing invented
//! [`FileRecord`](super::apply::FileRecord)s would make the tree claim a
//! provenance nobody measured, which is precisely the failure the manifest
//! exists to prevent. So the record says: this package's own installer ran,
//! this is the line it ran as, and it reported success.
//!
//! ## Two refusals, not one, because they need different answers
//!
//! - A tree with **no `distribution.json`** is not a tree ART built, so ART
//!   cannot say what is in it. The user's fix is to point at a distribution
//!   tree.
//! - A tree whose manifest is readable and **does not name the prerequisite**
//!   is missing a package. The user's fix is to install that package first,
//!   and the message names which, in the order they go on.
//!
//! Collapsing the two into one sentence would send half the readers to the
//! wrong fix — §3's rule that ART says *which* of the possible reasons
//! applies.
//!
//! ## Both refusals apply to every package, including one that requires
//! nothing — fix round 1
//!
//! The first version read the tree **only** when the package declared a
//! requirement: BoingBag 1 requires nothing, so a hand-made tree with no
//! manifest was an explicitly permitted run. That was wrong, and wrong in
//! this round's signature way.
//!
//! [`record_amiga_install`] cannot record into a tree that has no manifest.
//! So the permitted run reached the emulator, the installer **worked**, the
//! recording failed, `perform`'s closure returned `Err`, the copy was never
//! promoted — and the user was told the install failed *after it had
//! succeeded*. That is the third time in two days this round produced a true
//! outcome reported as its opposite: ART-185 would have said "the installer
//! ran and refused" about a program that never started, the stock `Updater`
//! would have said the same about one that could not work, and this said "it
//! failed" about one that did the job. §89 forbids all three.
//!
//! The defect was not in either half. It was in the gap between them, so the
//! fix closes the gap rather than patching one side: [`applied`] is now read
//! **unconditionally**, which makes "ART can account for this tree" and "ART
//! can record a success into this tree" the same question, asked once.
//!
//! The alternatives were considered and are worse:
//!
//! - **Create a minimal manifest when there is none.** It would carry a
//!   `release` ART does not know and an empty `files[]`, and put both in
//!   front of `verify`, `collide` and
//!   [`apply`](super::apply)'s own `classify_incoming`, which today refuses
//!   outright when a tree has no manifest because it cannot say what adding
//!   a component would replace. A synthesised empty one turns that honest
//!   refusal into "nothing was there", which is a lie — the same
//!   corrupt-a-different-consumer argument that ruled out synthesising
//!   [`FileRecord`](super::apply::FileRecord)s.
//! - **Record nothing and say so.** The next run's chain check would then
//!   refuse BoingBag 2 on a tree that really does have BoingBag 1 — exactly
//!   the "worse than no refusal" this module exists to avoid.
//!
//! §89 is still satisfied, because the two refusals stay two sentences: a
//! manifest-less tree is told ART cannot say what is in it, and is never told
//! that some package is missing, which ART would not know.

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::apply::{AmigaInstallRecord, DistributionManifest, MANIFEST_FILE_NAME};
use super::package::{self, NotYetRunnable, Package};
use super::recipe;
use super::slots::{Installed, SlotKind, SlotState};
use super::Component;
use crate::core::error::{CoreError, CoreResult};

/// Read a distribution tree's own `distribution.json`.
///
/// `pub` rather than private since the slot resolver arrived: a tree's
/// manifest is the **only** thing allowed to say a slot is installed
/// (`core::osinstall::slots::Installed` — a file being present is never
/// "installed"), and `commands::osinstall::osinstall_slots` has to hand the
/// resolver the whole manifest rather than [`applied`]'s id set. One reader,
/// so the refusal a manifest-less folder gets is the same sentence wherever
/// it is asked from.
pub fn read_manifest(tree: &Path) -> CoreResult<DistributionManifest> {
    let path = tree.join(MANIFEST_FILE_NAME);
    if !path.is_file() {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' holds no {MANIFEST_FILE_NAME}, so ART cannot say which packages it already \
             has; point at a distribution tree ART built",
            tree.display()
        )));
    }
    let text = std::fs::read_to_string(&path)?;
    serde_json::from_str(&text).map_err(|err| CoreError::Malformed {
        format: "distribution manifest".into(),
        detail: format!("'{}': {err}", path.display()),
    })
}

/// What a folder is, asked of the folder itself.
///
/// **ART-199.** A step that only knew whether *a path had been chosen* looked
/// ready on any folder at all, and the user learned otherwise from a refusal
/// on the button — the owner pointed the Amiga-side step at their own
/// `os39` folder and got `ART-SAFETY-REFUSED` for their trouble. The refusal
/// was right; it arrived in the wrong place and at the wrong time. This is the
/// question the field can ask the moment a folder is picked.
///
/// **It never fails for a folder that is not a tree.** A missing or malformed
/// `distribution.json` is an answer — `is_tree: false` with a `problem`
/// saying which — not an error. An error here would put the burden back on
/// the caller to tell "you picked the wrong folder" apart from "the disk went
/// away", and those are different sentences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeSummary {
    /// Whether this folder carries a distribution ART can reason about.
    pub is_tree: bool,
    /// The release it was built from, when it is a tree.
    pub release: Option<String>,
    /// How many files the manifest accounts for.
    pub files: usize,
    /// Which components built it, sorted and without repeats.
    ///
    /// **This is what decides whether a package can go on it**, so the picker
    /// shows it: the owner learned by trial which of nine trees carried
    /// `locale-base`, when every manifest says so.
    pub components: Vec<String>,
    /// Packages whose own installer has already run on the Amiga against it.
    pub amiga_installed: Vec<String>,
    /// Why it is not a tree, when it is not. English, like every other
    /// `CoreError` sentence (ART-060) — the screen adds its own.
    pub problem: Option<String>,
}

/// See [`TreeSummary`].
pub fn describe_tree(tree: &Path) -> TreeSummary {
    let empty = |problem: String| TreeSummary {
        is_tree: false,
        release: None,
        files: 0,
        components: Vec::new(),
        amiga_installed: Vec::new(),
        problem: Some(problem),
    };

    if !tree.is_dir() {
        return empty(format!("'{}' is not a folder", tree.display()));
    }
    let manifest = match read_manifest(tree) {
        Ok(manifest) => manifest,
        Err(err) => return empty(err.to_string()),
    };

    let mut components: Vec<String> = manifest
        .files
        .iter()
        .map(|file| file.component.clone())
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect();
    components.sort();

    TreeSummary {
        is_tree: true,
        release: Some(manifest.release),
        files: manifest.files.len(),
        components,
        amiga_installed: manifest
            .amiga_installed
            .iter()
            .map(|record| record.package.clone())
            .collect(),
        problem: None,
    }
}

/// One tree found inside a folder, and what it carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundTree {
    /// The tree's own folder, absolute.
    pub path: PathBuf,
    /// Its folder name — what a picker shows, so the screen never has to
    /// split a path itself.
    pub name: String,
    /// [`describe_tree`]'s answer for it. Always `is_tree: true`; anything
    /// else is not returned at all.
    pub summary: TreeSummary,
}

/// Every distribution tree directly inside `folder`, newest name last.
///
/// **ART-197's first remaining row, and the doc comment on
/// [`TreeSummary::components`] already asked for it**: *"the owner learned by
/// trial which of nine trees carried `locale-base`, when every manifest says
/// so."* Nine folders whose names differ by a suffix, and the only way to tell
/// them apart was to run something and see what happened.
///
/// **One directory level, no recursion.** A tree is a system volume — it has a
/// `C`, a `Devs`, a `Libs` and several thousand files under them, and
/// descending into one looking for another would walk the whole distribution
/// to find nothing. The folder the user keeps their builds in is the folder
/// they point at.
///
/// **The folder itself is not considered.** A caller that has just been handed
/// a path asks [`describe_tree`] about it first; this answers the different
/// question *"what is inside here?"*, and folding both into one function would
/// mean a tree could be returned as its own child.
///
/// Unreadable entries are skipped rather than raised, for the reason
/// [`describe_tree`] never fails: a folder holding one broken build and eight
/// good ones should offer the eight. Only the top-level `read_dir` can fail,
/// which is the caller's own bad path.
pub fn trees_in(folder: &Path) -> CoreResult<Vec<FoundTree>> {
    let mut found: Vec<FoundTree> = Vec::new();
    for entry in std::fs::read_dir(folder)? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let summary = describe_tree(&path);
        if !summary.is_tree {
            continue;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        found.push(FoundTree {
            path,
            name,
            summary,
        });
    }
    // `read_dir` order is the filesystem's, which on NTFS is neither creation
    // order nor anything a person would predict. Sorted by name so the list
    // is the same list twice running — a picker whose rows move between two
    // openings is one nobody can learn.
    found.sort_by_key(|found| found.name.to_lowercase());
    Ok(found)
}

/// Every component and package id this tree already carries — the components
/// its files came from, and the packages whose own installers ran on the
/// Amiga against it.
///
/// The two are unioned rather than kept apart because callers ask one
/// question: *is this thing in there?* A package id and a component id are
/// the same string by construction — `RawPackage::into_package` builds the
/// component from the package's own `id` — so one set answers it.
pub fn applied(tree: &Path) -> CoreResult<BTreeSet<String>> {
    Ok(applied_in(&read_manifest(tree)?))
}

/// [`applied`] over a manifest the caller already holds.
///
/// The same answer from the same rule, for the callers that have read the
/// tree once and must not read it a second time to ask a second question —
/// `osinstall_chain` resolves slots and rows off one manifest, and
/// `resolve_packages_for_add` derives both `components_on` and this from the
/// manifest it already opened. A second inline union is how the two would
/// come to disagree about what "already there" means.
pub fn applied_in(manifest: &DistributionManifest) -> BTreeSet<String> {
    let mut ids: BTreeSet<String> = manifest
        .files
        .iter()
        .map(|file| file.component.clone())
        .collect();
    ids.extend(manifest.amiga_installed.iter().map(|r| r.package.clone()));
    ids
}

/// Every package `package` needs before it, transitively, in the order they
/// must be installed.
///
/// Transitive on purpose: BoingBag 3 and 4 declare BoingBag 2, which
/// declares BoingBag 1, and a user starting from a clean tree needs to be
/// told both — naming only the immediate one would send them round the same
/// refusal twice.
fn prerequisite_chain(package: &Package) -> CoreResult<Vec<String>> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut queue: VecDeque<String> = package.requires.iter().cloned().collect();
    while let Some(id) = queue.pop_front() {
        if !seen.insert(id.clone()) {
            continue;
        }
        for need in package::by_id(&id)?.requires {
            queue.push_back(need);
        }
    }
    // `order` is the one place that decides application order, and it is
    // given a transitively closed set so it can never refuse for the "not
    // chosen" reason. A second sort here would be a second answer to a
    // question that already has one.
    package::order(&seen.into_iter().collect::<Vec<String>>())
}

/// Which of `package`'s prerequisites this tree does not have, in the order
/// they must be installed. Empty when there is nothing to do.
///
/// **[`applied`] is read before the chain is looked at, and unconditionally**
/// — including for a package that requires nothing, which used to skip the
/// read entirely (fix round 1). That single line is what keeps this in step
/// with [`record_amiga_install`]: a tree this function accepts is a tree a
/// successful run can be recorded into, because both go through the same
/// read. See the module documentation for the ending that disagreement
/// produced.
pub fn missing_prerequisites(package: &Package, tree: &Path) -> CoreResult<Vec<String>> {
    unmet_prerequisites(package, &applied(tree)?)
}

/// [`missing_prerequisites`] over an already-read account of the tree.
///
/// **The one implementation of the BoingBag-1-before-BoingBag-2 rule.**
/// [`refuse_unless_installable`] reaches it through
/// [`missing_prerequisites`], which reads the tree; [`rows_for`] reaches it
/// directly, because the chain screen has already read the manifest once and
/// asks about every package rather than one. Two entry points, one rule — a
/// second implementation is how a refusal and the row above it would come to
/// disagree about the same package.
fn unmet_prerequisites(package: &Package, have: &BTreeSet<String>) -> CoreResult<Vec<String>> {
    Ok(prerequisite_chain(package)?
        .into_iter()
        .filter(|id| !have.contains(id))
        .collect::<Vec<String>>())
}

/// Refuse a run this tree cannot honestly carry.
///
/// Two reasons, and the caller does not choose between them because a reader
/// of the message needs to know which applies: the tree is not one ART can
/// account for at all, or it is missing a package `package` has to go on
/// after. The first comes out of [`applied`], the second out of
/// [`missing_prerequisites`].
///
/// **Named for the question rather than for one of its halves** (fix round
/// 1). It was `refuse_unless_prerequisites_met`, and under that name it was
/// natural to read a manifest-less tree as having no prerequisites to fail —
/// which is how the two halves came to disagree.
///
/// Called **before anything is copied** — the copy, the work volume and the
/// package unpack all happen after this, so a refused run has changed
/// nothing at all.
///
/// ## This is the only thing that stops it — measured 2026-09-08
///
/// Until round 3 the sentence below rested on reasoning: BoingBag 3.9-2's own
/// `Install` script reads `version.library` off the target and wants revision
/// 2, so the package would presumably decline. **ART does not run that script;
/// it runs `C/Updater` directly**, and the experiment in round 3, task 3 asked
/// the program itself. On a tree that had never had BoingBag 3.9-1 — reachable
/// only by doctoring the copy's own `distribution.json`, because this function
/// refuses it in 17.6 ms with nothing copied and no emulator started — the
/// `Updater` ran to completion and wrote **`ok`**, twice, `Succeeded` and
/// `Promoted` in 142.2 s and 140.7 s, producing two byte-identical trees.
///
/// Those trees are the "boots and is quietly wrong" case, and the numbers are
/// worth carrying: against a correctly chained tree they are **missing 57
/// files** (60 paths differ, three of which — `C/Exe2Arc`, `C/WBInfo`,
/// `Utilities/More` — exist in both under a different case, because BoingBag 1
/// re-cases them rather than adding them), carry **51 at older bytes**, and
/// leave `Libs/xadmaster.library` at `9.0` where the chained tree has `9.1`.
///
/// **What that refutes, stated narrowly.** `Libs/version.library` reads
/// `version 45.3 (7.12.2001)` on **both** — the same 352 bytes and the same
/// sha256 — so *the version string the package updates* is not evidence that
/// the right thing happened. It does **not** follow that no artefact could
/// separate them: `xadmaster.library` is an artefact and it does, 9.0 against
/// 9.1. What the round refused to build was a `leaves_version` check **on
/// `version.library`**, which would have passed the broken tree.
///
/// So an after-the-fact artefact check is *possible*; it is simply not what is
/// protecting anyone here. This refusal is — and the bound on that claim
/// belongs beside it: it is a **bookkeeping** guard, not an artefact guard.
/// [`missing_prerequisites`] reads `distribution.json`, which only a
/// successful ART run writes, so the wrong-target state is unreachable through
/// ART at all. That is why it is a refusal before anything is copied rather
/// than a warning after, and why the package's own judgement — measured to be
/// `ok` — is not something to lean on.
pub fn refuse_unless_installable(package: &Package, tree: &Path) -> CoreResult<()> {
    let missing = missing_prerequisites(package, tree)?;
    let Some(first) = missing.first() else {
        return Ok(());
    };
    // Names, not ids, where ART has one: `boingbag-39-1` is ART's own
    // bookkeeping and "BoingBag 3.9-1" is what is written on the thing the
    // user downloaded.
    let named: Vec<String> = missing
        .iter()
        .map(|id| match package::by_id(id) {
            Ok(found) => found.name,
            Err(_) => id.clone(),
        })
        .collect();
    Err(CoreError::SafetyRefused(format!(
        "'{}' has to go on after {}, and '{}' does not have {} yet — install {} first, in that \
         order. Running it now would produce a system that boots and is quietly wrong.",
        package.name,
        named.join(", then "),
        tree.display(),
        if missing.len() == 1 { "it" } else { "them" },
        named.first().map(String::as_str).unwrap_or(first)
    )))
}

/// Record that `package_id`'s own installer ran on the Amiga against this
/// tree and reported success.
///
/// Written into the tree's existing `distribution.json`, preserving every
/// other field: this is an addition to the tree's account of itself, never a
/// rewrite of it. A package already recorded is not recorded twice — a
/// re-run of the same package is a legitimate thing to do and does not make
/// the tree carry it twice.
///
/// Goes through `core::safety::atomic`, like every other write to this file:
/// a half-written `distribution.json` is a tree that can no longer say what
/// it is.
pub fn record_amiga_install(tree: &Path, package_id: &str, command: &str) -> CoreResult<()> {
    let mut manifest = read_manifest(tree)?;
    if !manifest
        .amiga_installed
        .iter()
        .any(|record| record.package == package_id)
    {
        manifest.amiga_installed.push(AmigaInstallRecord {
            package: package_id.to_string(),
            command: command.to_string(),
        });
    }
    let text = serde_json::to_string_pretty(&manifest).map_err(|err| CoreError::Malformed {
        format: "distribution manifest".into(),
        detail: err.to_string(),
    })?;
    crate::core::safety::atomic::atomic_write(&tree.join(MANIFEST_FILE_NAME), text.as_bytes())
}

// ---------------------------------------------------------------------------
// The whole chain, as rows
// ---------------------------------------------------------------------------

/// What one link of the chain is, and it is exactly the design's own table
/// (`2026-09-08-os-builder-chain-design.md` § 2).
///
/// **Seven states, and they never collapse.** *Installed*, *ready*, *blocked
/// by something else*, *the file is not here*, *the material itself makes it
/// redundant*, *ART will not do it and here is why*, and *nobody has
/// measured this yet* are seven different next steps. A screen that folded
/// any two of them would tell somebody to go and find a file they already
/// have, or to wait for something that has already happened — this project's
/// named defect, in the one place it is most likely to be committed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum ChainState {
    /// The tree's own `distribution.json` records it — `amiga_installed` for
    /// a run, `files`/`built_from` for a placement. **Never a file being
    /// present**, which is the whole reason this comes from the manifest.
    ///
    /// `when` is always `None` today and is on the wire anyway: the manifest
    /// records no date against a component, a medium or a run (round 2
    /// whole-branch review, I14, which corrected the design's own sketch for
    /// the same reason). The field exists so the shape does not change under
    /// the screen the day a record carries one, and the sentence must render
    /// nothing rather than a guess when it is absent.
    Installed { when: Option<String> },
    /// Its own artefact is in hand, everything it goes on after is
    /// installed, and the disc its installer verifies is in hand too.
    Ready,
    /// Something earlier in the chain has to happen first, named by the
    /// **rows'** own names and in chain order — never by slot id, and never
    /// as a bare count.
    BlockedBy { names: Vec<String> },
    /// The tree was not built with a **component** this package needs
    /// ([`Package::requires_components`], ART-162).
    ///
    /// **Its own variant and not [`BlockedBy`](Self::BlockedBy), because the
    /// next step is somewhere else entirely** (round 3 whole-branch review,
    /// M4). Every name in `BlockedBy` is another row on this same screen and
    /// the advice is *do that one first*; a component is not a row here at
    /// all — it is a tick-box on the Packages/components step, and a tree
    /// already built without it has to be rebuilt or have the component
    /// added. Folding the two together would send somebody looking down a
    /// list of nine rows for something that is not in it.
    ///
    /// Before this existed the row read **`Ready`**, the one Run button armed
    /// on it — taking the button from a later row that really was ready — and
    /// `resolve_packages_for_add` then refused with `PackageComponentMissing`.
    /// That is the screen out-claiming the core, which is the defect this
    /// whole file is written against.
    BlockedByComponent { components: Vec<BlockedComponent> },
    /// Its artefact is not in the folders the user named. `expected` is the
    /// file names ART has recorded for it, which may be empty: *"Expected ."*
    /// is a sentence nobody can act on, so the screen picks a different one.
    Missing { expected: Vec<String> },
    /// The material's own rule makes it redundant — `superseded_by` names a
    /// package the tree already has. The **name** of that package, not a
    /// sentence: the words belong in the catalogue (ART-060).
    NotNeeded { superseded_by: String },
    /// A package that comes after this one and writes over it — it names
    /// this package in its own `overrides` — is **already in the tree**.
    /// `names` are those packages' own names, in chain order.
    ///
    /// **Not [`NotNeeded`](Self::NotNeeded)**, because it is not redundant:
    /// the owner's case, measured 2026-09-10, is Locale 3.9's Turkish slice
    /// under BoingBag 3.9-2's Turkish catalogs — 32 of its 33 catalogs are in
    /// the newer package, the same version or older, but it also carries
    /// `sys/ahi.catalog` and two font families nothing else does. *"Already
    /// contains it"* would be a claim the material does not support. And
    /// **not [`Ready`](Self::Ready)**: adding it now puts older files over
    /// newer ones, which `add_package` refuses as undeclared overwrites — the
    /// owner's run ticked it and was refused over 82 files. The row says so
    /// before anybody ticks it.
    OvertakenBy { names: Vec<String> },
    /// ART will not do this row, and says which of the reasons applies.
    Refused { reason: RefusedBecause },
    /// The recipe declares what installs this package and **nobody has run
    /// it**. Registered rather than hidden (§10), carrying the recipe's own
    /// typed reason so the screen translates it (fix round 1, m6 — it used
    /// to carry free English prose and put a Turkish frame around it).
    NotYetRunnable { reason: NotYetRunnable },
}

/// A component a package needs and the tree does not have — its id, and the
/// **i18n key** the components screen labels it by.
///
/// A key and not a rendered name, for the reason [`Component::label_key`]
/// itself is a key (ART-224): the recipe is data in the Rust tree and the
/// words belong in the catalogue (ART-060). `label_key` is `None` for a
/// component that labels itself by its medium, and the screen then shows the
/// id — which is what the components screen shows for it too, so the two
/// cannot disagree about what the thing is called.
///
/// [`Component::label_key`]: super::Component::label_key
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockedComponent {
    pub id: String,
    pub label_key: Option<String>,
}

/// Why a row is refused. A value, never a sentence, for the two causes ART
/// can state as data; the screen translates it (ART-060).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "because", rename_all = "kebab-case")]
pub enum RefusedBecause {
    /// Several files in the folders could be this row's artefact and ART
    /// will not choose between them. The candidates travel with it, because
    /// *"pick the one you mean"* without the list is not a next step.
    Ambiguous { candidates: Vec<String> },
    /// ART cannot place this package's files from the host at all and it has
    /// no Amiga-side installer either — so there is no route, and the block
    /// says which. See [`HostPlacementBlock`](super::HostPlacementBlock).
    NotPlaceable { block: super::HostPlacementBlock },
}

/// The facts a row's sentence needs beside its state, and nothing more.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SentenceFacts {
    /// The file filling this row's slot, by name alone. `None` when nothing
    /// fills it — including for a row that is installed and whose archive
    /// has since left the folder, which is an ordinary state.
    pub file: Option<String>,
    /// Where this row happens: `Some(true)` on the Amiga, through the
    /// package's own installer; `Some(false)` placed from Windows by ART;
    /// `None` for a row that is neither — the CD, and a package ART can
    /// neither place nor run (Euro-Update).
    ///
    /// Three states rather than a boolean, because "ART places it" and "ART
    /// can do neither" are not the same claim and a boolean would have to
    /// tell one of them as the other.
    pub runs_on_amiga: Option<bool>,
}

/// One row of the chain screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainRow {
    /// The rank the material gives this link — the CD is 1, and two rows may
    /// share a rank where the material states no order between them
    /// (`Package::chain_position`). Rows come sorted by `(position, id)`, so
    /// the list is the same list twice running.
    pub position: u32,
    /// The package's own id, or `None` for the CD row, which is a medium.
    pub package_id: Option<String>,
    /// The slot this row is fed by, when the release has one for it.
    pub slot_id: Option<String>,
    /// What to call it: the package's own name, or the medium's (ART-060).
    pub name: String,
    pub state: ChainState,
    /// The facts the row's sentence needs beside its state. Named as the
    /// brief named it (fix round 1, m5): it shipped as `facts`, which is
    /// shorter and says less about what it is for.
    pub sentence_facts: SentenceFacts,
}

/// How much of the chain is done — the design's `● 3 of 8 applied` line.
///
/// **A row the material makes redundant is counted apart, never as done and
/// never as outstanding.** Counting Euro-Update as outstanding under an
/// installed BoingBags 3&4 would send somebody looking for a file whose
/// contents they already have; counting it as applied would claim ART did
/// something it did not. `summarize` keeps the same rule one field over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainSummary {
    pub release: String,
    pub total: u32,
    pub installed: u32,
    pub not_needed: u32,
}

/// The whole chain for `release`, resolved against the tree's own manifest
/// and round 2's slot states.
///
/// **The order of the checks is the design's, and each one is why the next
/// cannot fire.** A row the manifest records is not waiting for anything; a
/// row the material makes redundant is not *missing*; a row nobody has run
/// cannot be *ready* whatever the folders hold. The two that come last
/// are the pair a reader is most likely to want the other way round, and the
/// choice is deliberate: **an artefact ART cannot find comes before an order
/// it cannot yet keep**, because "go and get this file" is something a
/// person can act on now and "install 4 first" is advice about a run they
/// could not start anyway.
///
/// The **CD row is synthesised** from the release's own medium slot and the
/// manifest's `built_from` — never from a package, because it is not one.
/// Where the release has no medium slot at all there is no CD row, rather
/// than an invented one.
pub fn rows_for(
    release: &str,
    manifest: Option<&DistributionManifest>,
    slots: &[SlotState],
) -> CoreResult<Vec<ChainRow>> {
    let packages = package::packages_for(release)?;
    let have: BTreeSet<String> = manifest.map(applied_in).unwrap_or_default();
    // ART-162 / M4: what the release's own components are called, so a row
    // blocked on one can name it the way the components step names it. An
    // unreadable recipe answers *no components* rather than refusing the
    // whole chain: the worst it costs is a component named by its id, and a
    // screen that renders nothing is worse than one that renders an id.
    let components: Vec<Component> = recipe::by_release(release)
        .map(|recipe| recipe.components)
        .unwrap_or_default();
    let state_of = |id: &str| slots.iter().find(|state| state.slot.id == id);

    // --- the packages that are chain rows, in the material's own order -----
    let mut chain: Vec<&Package> = packages
        .iter()
        .filter(|package| package.chain_position.is_some())
        .collect();

    // **A release with no chain is an empty list, not a lone CD row** (fix
    // round 1, m3). AmigaOS 3.2 ships no update package at all, so taking
    // the first medium slot regardless produced one row built from whichever
    // floppy happened to sort first, labelled position 1, under a summary
    // reading *"AmigaOS 3.2 updates — 0 of 1 applied"*. The CD row exists
    // because it is the **first link of a chain**; with no chain there is no
    // first link, and a screen showing one would be inventing a step.
    if chain.is_empty() {
        return Ok(Vec::new());
    }

    let mut rows: Vec<ChainRow> = Vec::new();

    // --- 1: the medium, which is a disc and not a package ------------------
    if let Some(medium) = slots
        .iter()
        .find(|state| state.slot.kind == SlotKind::Medium)
    {
        rows.push(ChainRow {
            position: 1,
            package_id: None,
            slot_id: Some(medium.slot.id.clone()),
            name: medium.slot.name.clone(),
            state: medium_state(medium),
            sentence_facts: SentenceFacts {
                file: file_name_of(medium),
                runs_on_amiga: None,
            },
        });
    }

    chain.sort_by(|a, b| {
        a.chain_position
            .cmp(&b.chain_position)
            .then_with(|| a.id.cmp(&b.id))
    });

    for package in chain {
        let slot_id = format!("package:{}", package.id);
        let state = state_of(&slot_id);
        rows.push(ChainRow {
            position: package.chain_position.unwrap_or_default(),
            package_id: Some(package.id.clone()),
            slot_id: state.map(|state| state.slot.id.clone()),
            name: package.name.clone(),
            state: package_state(package, state, &have, slots, &packages, &components)?,
            sentence_facts: SentenceFacts {
                file: state.and_then(file_name_of),
                // **The block decides, and the installer only breaks the
                // tie** (2026-09-08, the owner's BoingBag reversal).
                //
                // The precedence used to be the other way round — a package
                // that declared an `amiga_installer` ran on the Amiga,
                // whatever else was true — and that was right while the only
                // packages declaring one were also the ones ART could not
                // place. Both BoingBags now carry a `payload_password` and
                // **no** `host_placement_block`, and they still carry their
                // `amiga_installer`: the emulator route is not withdrawn,
                // and it is no longer the route this row takes. Under the
                // old reading the chain would have gone on offering a ~140 s
                // emulator run, needing a ROM and a licence, for work the
                // host does in seconds — the screen out-claiming what ART
                // has to do.
                //
                // So: no block means ART places it (`Some(false)`, the
                // `useHostPlacement` route); a block with an installer means
                // the Amiga (`Some(true)`); a block with no installer means
                // neither (`None`, Euro-Update).
                runs_on_amiga: match (
                    package.host_placement_block.is_some(),
                    package.amiga_installer.is_some(),
                ) {
                    (false, _) => Some(false),
                    (true, true) => Some(true),
                    (true, false) => None,
                },
            },
        });
    }

    Ok(rows)
}

/// [`ChainSummary`] over what [`rows_for`] answered.
pub fn summarize_chain(release: &str, rows: &[ChainRow]) -> ChainSummary {
    ChainSummary {
        release: release.to_string(),
        total: rows.len() as u32,
        installed: rows
            .iter()
            .filter(|row| matches!(row.state, ChainState::Installed { .. }))
            .count() as u32,
        not_needed: rows
            .iter()
            .filter(|row| matches!(row.state, ChainState::NotNeeded { .. }))
            .count() as u32,
    }
}

/// The CD row's state. **Two answers, and `Ready` is deliberately not one of
/// them** (round 3 whole-branch review, M3).
///
/// The design says it plainly — *"the CD row is never run here"* — and this
/// function used to contradict it: a tree whose manifest names no
/// `AmigaOS3.9` in `built_from`, with the ISO sitting in a named folder,
/// answered `Ready`. `chainLines` marks the first ready row runnable, so the
/// single Run button landed on a disc, `runLabel` read *"Next: AmigaOS3.9"*
/// over a button that could never fire (the request needs a package id), and
/// the BoingBag below it that really was ready was never offered.
///
/// The two answers are the only two this screen can act on:
///
/// - **`Installed`** — the manifest's `built_from` names this volume. That is
///   the whole of what the row is *for*: the first link of the chain is "this
///   tree came off that disc".
/// - **`Missing`** — it does not. Finding the ISO in a folder does not change
///   that and must not read as though it did: **this screen cannot build a
///   tree from a disc**, and the row's action is the `kaynak` link the panel
///   renders beside it. The screen substitutes its own sentence for this one
///   (`chain.mediumNotBuiltFrom`) rather than *"not in the folders you
///   named"*, which would be false with the file right there.
///
/// The old `Ready` and `Ambiguous` arms went with it. Ambiguity is a question
/// about *which file to use*, and this row never uses one; the source step's
/// own `slotLines` reports it where it can be acted on.
fn medium_state(state: &SlotState) -> ChainState {
    if state.installed != Installed::No {
        return ChainState::Installed { when: None };
    }
    ChainState::Missing {
        expected: state.slot.filenames.clone(),
    }
}

fn package_state(
    package: &Package,
    state: Option<&SlotState>,
    have: &BTreeSet<String>,
    slots: &[SlotState],
    all: &[Package],
    components: &[Component],
) -> CoreResult<ChainState> {
    // 1 — what actually happened, and it outranks every other check.
    //
    // **The brief put `not_yet_runnable` first and running the test moved
    // it.** *"ART has never driven this one"* about a row the tree already
    // records is the screen out-claiming the core: it tells somebody to wait
    // for something that has already happened. The manifest is the only
    // thing that knows what was done, so it is asked first — and only the
    // manifest, never a file being present.
    if have.contains(&package.id) {
        return Ok(ChainState::Installed { when: None });
    }

    // 2 — the material itself says it is redundant. Above *missing* on
    // purpose: telling somebody to go and find Euro-Update on a tree that
    // has BoingBags 3&4 is sending them after a file whose contents they
    // already have. Below *installed*, for the reason above.
    for superseder in &package.superseded_by {
        if have.contains(superseder) {
            return Ok(ChainState::NotNeeded {
                superseded_by: all
                    .iter()
                    .find(|other| &other.id == superseder)
                    .map(|other| other.name.clone())
                    .unwrap_or_else(|| superseder.clone()),
            });
        }
    }

    // 2b — a later package that writes over this one is already in the
    // tree. Below *not needed*, which is the stronger answer when both hold;
    // above everything still to come, because none of those can be acted on
    // for a row that cannot be added without putting older files over newer
    // ones. Read from the newer package's own `overrides` — the declaration
    // `add_package` checks — so the row and the refusal cannot disagree.
    let overtaken: Vec<String> = all
        .iter()
        .filter(|other| {
            have.contains(&other.id)
                && other
                    .component
                    .overrides
                    .iter()
                    .any(|over| over == &package.id)
        })
        .map(|other| other.name.clone())
        .collect();
    if !overtaken.is_empty() {
        return Ok(ChainState::OvertakenBy { names: overtaken });
    }

    // 3 — declared and never run. Above everything that is still to come,
    // because whatever the folders hold, ART cannot do this row (§10:
    // registered, not hidden), and `commands::amigainstall::compose` refuses
    // it too. A row here with its archive sitting in the folder would
    // otherwise read *ready* and offer a Run button for a program ART has
    // never seen finish.
    if let Some(why) = package
        .amiga_installer
        .as_ref()
        .and_then(|installer| installer.not_yet_runnable.as_ref())
    {
        return Ok(ChainState::NotYetRunnable { reason: *why });
    }

    // 4 — no route at all: ART cannot place it from the host and there is no
    // installer to run either. Before the folder is consulted, because it is
    // a property of the package and true however many copies of the archive
    // the user holds (the same order `detect_package_refusals` uses).
    if let Some(block) = package.host_placement_block {
        if package.amiga_installer.is_none() {
            return Ok(ChainState::Refused {
                reason: RefusedBecause::NotPlaceable { block },
            });
        }
    }

    let Some(state) = state else {
        // A package this release ships no slot for. Not reachable through
        // `slots_for`, which derives its list from the same `packages_for`;
        // answered rather than panicked because a caller may pass a slot
        // list it built itself.
        return Ok(ChainState::Missing {
            expected: Vec::new(),
        });
    };

    // 5 — several files could be it, and ART never picks one.
    if state.found.is_none() && state.candidates.len() > 1 {
        return Ok(ChainState::Refused {
            reason: RefusedBecause::Ambiguous {
                candidates: candidate_paths(state),
            },
        });
    }

    // 6 — the artefact is not in hand. See this function's own doc comment
    // for why this comes before the order check rather than after it.
    if state.found.is_none() {
        return Ok(ChainState::Missing {
            expected: state.slot.filenames.clone(),
        });
    }

    // 7 — everything it goes on after, plus the disc its installer verifies.
    // The package half is `unmet_prerequisites` — the one implementation of
    // ART-186's rule, shared with `refuse_unless_installable`; the medium
    // half comes off the slot, which is where a `required_medium` became a
    // `requires` entry in the first place.
    //
    // **In position order** (fix round 1, m2, and the design says so:
    // *"blocked_by names rows, in position order"*). The two halves arrive
    // in two different orders — the packages in `package::order`'s
    // topological one, the media after them — so BoingBag 3.9-2 blocked on
    // both answered `["BoingBag 3.9-1", "AmigaOS3.9"]`: rank 2 before rank
    // 1. A reader told to do things in an order that is not the order is
    // being given the one thing this screen exists to get right.
    //
    // The medium is rank 0 here because the CD is row 1 and every package
    // starts at 2; a package's own rank is its `chain_position`.
    let mut blocked: Vec<(u32, String)> = Vec::new();
    for id in unmet_prerequisites(package, have)? {
        let other = all.iter().find(|other| other.id == id);
        blocked.push((
            other
                .and_then(|other| other.chain_position)
                .unwrap_or(u32::MAX),
            other.map(|other| other.name.clone()).unwrap_or(id),
        ));
    }
    for need in &state.blocked_by {
        if let Some(required) = slots.iter().find(|other| &other.slot.id == need) {
            if required.slot.kind == SlotKind::Medium {
                blocked.push((0, required.slot.name.clone()));
            }
        }
    }
    if !blocked.is_empty() {
        blocked.sort();
        return Ok(ChainState::BlockedBy {
            names: blocked.into_iter().map(|(_, name)| name).collect(),
        });
    }

    // 8 — the tree has to have been *built* with something (ART-162).
    //
    // **Below step 7 on purpose.** Both are "something else first", and they
    // differ in where that something is: step 7 names rows on this screen,
    // which a reader can act on here and now; a component is a tick-box on
    // the components step and a tree already built without it needs the
    // component added or the tree rebuilt. Naming the reachable one first is
    // the same rule that puts *missing* above *blocked*.
    //
    // `have` already carries component ids — `applied_in` unions the
    // manifest's `files[].component` with its `amiga_installed` — so this
    // asks the manifest and never the filesystem, exactly as step 1 does.
    //
    // Without it the three `locale-*` packages read **`Ready`** on any tree
    // built without `locale-base`, which is `required: false` in
    // `amigaos-3.9.json` and so an ordinary tree rather than an exotic one:
    // the row said ready, the one Run button armed on it, and
    // `resolve_packages_for_add` refused with `PackageComponentMissing`
    // (round 3 whole-branch review, M4).
    let missing_components: Vec<BlockedComponent> = package
        .requires_components
        .iter()
        .filter(|id| !have.contains(*id))
        .map(|id| BlockedComponent {
            id: id.clone(),
            label_key: components
                .iter()
                .find(|component| &component.id == id)
                .and_then(|component| component.label_key.clone()),
        })
        .collect();
    if !missing_components.is_empty() {
        return Ok(ChainState::BlockedByComponent {
            components: missing_components,
        });
    }

    Ok(ChainState::Ready)
}

/// The file filling a slot, by name alone — the whole path is the readout's
/// business, and a chain row has one line.
fn file_name_of(state: &SlotState) -> Option<String> {
    state.found.as_ref().and_then(|found| {
        found
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    })
}

/// Whole paths, because the commonest ambiguity is two copies under the same
/// name and the folder is then the only information there is (round 2's own
/// finding, F3).
fn candidate_paths(state: &SlotState) -> Vec<String> {
    state
        .candidates
        .iter()
        .map(|candidate| candidate.path.display().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::osinstall::apply::FileRecord;
    use crate::core::ScratchDir;

    /// ART-184: removes itself on `Drop`, so a panicking test cleans up too.
    fn scratch(tag: &str) -> ScratchDir {
        ScratchDir::new("art-osinstall-chain", tag)
    }

    // -----------------------------------------------------------------
    // ART-199: what a folder is, asked of the folder.
    // -----------------------------------------------------------------

    #[test]
    fn a_tree_describes_itself_by_release_files_and_components() {
        let dir = scratch("describe-tree");
        let tree = tree_with(
            dir.path(),
            &["workbench-base", "locale-base", "workbench-base"],
        );

        let said = describe_tree(&tree);
        assert!(said.is_tree);
        assert_eq!(said.release.as_deref(), Some("amigaos-3.9"));
        assert_eq!(said.files, 3);
        // Sorted and without repeats: the picker renders this, and what a tree
        // carries is what decides whether a package can go on it.
        assert_eq!(said.components, vec!["locale-base", "workbench-base"]);
        assert!(said.problem.is_none());
    }

    #[test]
    fn a_folder_with_no_manifest_is_not_a_tree_and_says_why() {
        // The owner's own case: their `os39` folder is an AmigaOS folder, not
        // a tree ART built, and the step showed it as ready.
        let dir = scratch("describe-not-a-tree");
        let folder = dir.join("os39");
        std::fs::create_dir_all(&folder).unwrap();

        let said = describe_tree(&folder);
        assert!(!said.is_tree);
        assert!(said.components.is_empty());
        assert!(
            said.problem.is_some(),
            "a folder that is not a tree must say why, not merely answer no"
        );
    }

    #[test]
    fn a_folder_that_is_not_there_is_answered_rather_than_erroring() {
        let dir = scratch("describe-absent");
        let said = describe_tree(&dir.join("nothing-here"));
        assert!(!said.is_tree);
        assert!(said.problem.unwrap().contains("not a folder"));
    }

    #[test]
    fn a_malformed_manifest_is_an_answer_not_a_panic() {
        let dir = scratch("describe-malformed");
        let tree = dir.join("dist");
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::write(tree.join(MANIFEST_FILE_NAME), b"{ not json").unwrap();

        let said = describe_tree(&tree);
        assert!(!said.is_tree);
        assert!(said.problem.is_some());
    }

    #[test]
    fn a_tree_reports_what_has_been_installed_on_the_amiga() {
        let dir = scratch("describe-amiga-installed");
        let tree = tree_with(dir.path(), &["workbench-base"]);
        record_amiga_install(&tree, "boingbag-39-1", "C:Updater AmigaOS-Update SYS:").unwrap();

        let said = describe_tree(&tree);
        assert_eq!(said.amiga_installed, vec!["boingbag-39-1"]);
    }

    fn file_from(component: &str) -> FileRecord {
        FileRecord {
            path: format!("C/{component}"),
            component: component.to_string(),
            media: "Workbench3.9".into(),
            sha256: String::new(),
            bytes: 1,
            protection: None,
            overwrote: None,
            host_path: None,
        }
    }

    /// A tree with a real `distribution.json` naming `components`.
    ///
    /// **The manifest is always written**, even for the "nothing applied"
    /// case, and that is the point: the trap this module's own tests were
    /// warned about is a fixture with no manifest at all, where the refusal
    /// fires because ART cannot read the tree rather than because the
    /// prerequisite is missing. Those two are different errors here, and
    /// `a_tree_with_no_manifest_is_a_different_refusal` pins the other one.
    fn tree_with(at: &std::path::Path, components: &[&str]) -> std::path::PathBuf {
        let tree = at.join("Workbench3.9");
        std::fs::create_dir_all(&tree).unwrap();
        let manifest = DistributionManifest {
            release: "amigaos-3.9".into(),
            built_from: Vec::new(),
            files: components.iter().map(|c| file_from(c)).collect(),
            paired_rom: None,
            amiga_installed: Vec::new(),
            layers: Vec::new(),
        };
        std::fs::write(
            tree.join(MANIFEST_FILE_NAME),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        tree
    }

    /// A tree under a name of the caller's choosing, so a folder can hold
    /// several and they can be told apart.
    fn named_tree(at: &std::path::Path, name: &str, components: &[&str]) -> std::path::PathBuf {
        let tree = at.join(name);
        std::fs::create_dir_all(&tree).unwrap();
        let manifest = DistributionManifest {
            release: "amigaos-3.9".into(),
            built_from: Vec::new(),
            files: components.iter().map(|c| file_from(c)).collect(),
            paired_rom: None,
            amiga_installed: Vec::new(),
            layers: Vec::new(),
        };
        std::fs::write(
            tree.join(MANIFEST_FILE_NAME),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        tree
    }

    // -----------------------------------------------------------------
    // ART-197 wave 2, row 1: the artefact picker's own question.
    // -----------------------------------------------------------------

    /// The case the picker exists for: several builds side by side, told
    /// apart by what each one carries rather than by trying them.
    #[test]
    fn a_folder_of_builds_lists_each_one_and_what_it_carries() {
        let dir = scratch("trees-in");
        named_tree(dir.path(), "dist-3.9-plain", &["workbench-base"]);
        named_tree(
            dir.path(),
            "dist-3.9-turkish",
            &["workbench-base", "locale-base"],
        );

        let found = trees_in(dir.path()).unwrap();
        assert_eq!(found.len(), 2);

        let turkish = found
            .iter()
            .find(|t| t.name == "dist-3.9-turkish")
            .expect("the tree with the locale in it");
        assert!(turkish.summary.is_tree);
        assert_eq!(turkish.summary.release.as_deref(), Some("amigaos-3.9"));
        assert!(
            turkish
                .summary
                .components
                .contains(&"locale-base".to_string()),
            "which component a tree carries is the whole reason for the list"
        );

        let plain = found.iter().find(|t| t.name == "dist-3.9-plain").unwrap();
        assert!(!plain
            .summary
            .components
            .contains(&"locale-base".to_string()));
    }

    /// One broken build must not cost the user the eight good ones beside it.
    #[test]
    fn a_folder_that_is_not_a_tree_is_skipped_not_raised() {
        let dir = scratch("trees-in-mixed");
        named_tree(dir.path(), "a-real-build", &["workbench-base"]);
        std::fs::create_dir_all(dir.path().join("just-a-folder")).unwrap();
        // A folder that *has* a manifest ART cannot read is the harder case:
        // `describe_tree` answers rather than failing, and this has to skip
        // on the answer, not on the absence of a file.
        let broken = dir.path().join("half-written");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(broken.join(MANIFEST_FILE_NAME), b"{ not json").unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"hello").unwrap();

        let found = trees_in(dir.path()).unwrap();
        assert_eq!(
            found.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            vec!["a-real-build"]
        );
    }

    /// The order is the same order twice running, and it is the order a
    /// person reads.
    ///
    /// **One mutation survives here and it is disclosed rather than worked
    /// around**: deleting `sort_by` altogether does not fail this test on
    /// Windows, because NTFS keeps its directory index in a *case-insensitive*
    /// order already — the same order the sort produces. Replacing the fold
    /// with a byte-wise `cmp` **does** fail it (`Zulu` would come before
    /// `beta`), which is what says the comparison is load-bearing wherever the
    /// two differ. The sort stays for the filesystems whose `read_dir` is not
    /// ordered at all; no test on this machine can prove it, and claiming one
    /// could would be worse than saying so.
    #[test]
    fn the_list_is_sorted_by_name_and_stable() {
        let dir = scratch("trees-in-order");
        // Deliberately mixed case: `["zulu", "Alpha", "mike"]` sorts the same
        // whether or not the comparison folds case, so it would have pinned
        // nothing. These three separate the two.
        for name in ["beta", "Alpha", "Zulu"] {
            named_tree(dir.path(), name, &["workbench-base"]);
        }
        let names: Vec<String> = trees_in(dir.path())
            .unwrap()
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert_eq!(
            names,
            vec!["Alpha", "beta", "Zulu"],
            "a byte-wise sort puts Zulu before beta; a person does not"
        );
        let again: Vec<String> = trees_in(dir.path())
            .unwrap()
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert_eq!(names, again);
    }

    /// A tree is a system volume with thousands of files under `C`, `Devs`
    /// and `Libs`. Descending into one to look for another would walk the
    /// whole distribution to find nothing — and would return a tree as its
    /// own child.
    #[test]
    fn it_does_not_descend_into_a_tree_it_has_already_found() {
        let dir = scratch("trees-in-nested");
        let outer = named_tree(dir.path(), "outer", &["workbench-base"]);
        named_tree(&outer, "inner", &["workbench-base"]);

        let found = trees_in(dir.path()).unwrap();
        assert_eq!(
            found.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            vec!["outer"]
        );
    }

    /// The folder the caller points at is a different question, asked of
    /// `describe_tree`. Folding both in here would make a tree its own child.
    #[test]
    fn the_folder_itself_is_never_in_its_own_list() {
        let dir = scratch("trees-in-self");
        let tree = named_tree(dir.path(), "only-build", &["workbench-base"]);
        assert!(trees_in(&tree).unwrap().is_empty());
        assert!(
            describe_tree(&tree).is_tree,
            "asked the other way, it is one"
        );
    }

    #[test]
    fn a_folder_that_cannot_be_read_is_the_callers_own_bad_path() {
        let dir = scratch("trees-in-missing");
        assert!(trees_in(&dir.path().join("nowhere")).is_err());
    }

    #[test]
    fn an_empty_folder_lists_nothing_without_complaining() {
        let dir = scratch("trees-in-empty");
        assert!(trees_in(dir.path()).unwrap().is_empty());
    }

    /// The defect ART-186 names, in one line: BoingBag 2 on a tree BoingBag 1
    /// never touched.
    #[test]
    fn boingbag_two_is_refused_on_a_tree_that_never_had_boingbag_one() {
        let dir = scratch("bb2-without-bb1");
        let tree = tree_with(dir.path(), &["workbench-base"]);
        let two = package::by_id("boingbag-39-2").unwrap();

        let err = refuse_unless_installable(&two, &tree).unwrap_err();
        let message = err.to_string();

        assert!(
            message.contains("BoingBag 3.9-1"),
            "the refusal must name what is missing: {message}"
        );
        assert!(
            !message.contains(MANIFEST_FILE_NAME),
            "and it must not be the 'ART cannot read this tree' refusal: {message}"
        );
        assert_eq!(
            missing_prerequisites(&two, &tree).unwrap(),
            vec!["boingbag-39-1".to_string()]
        );
    }

    /// And the same package on a tree that *does* have it goes through — a
    /// refusal that fires either way would be no check at all.
    #[test]
    fn boingbag_two_is_allowed_once_boingbag_one_is_recorded() {
        let dir = scratch("bb2-with-bb1");
        let tree = tree_with(dir.path(), &["workbench-base"]);
        let two = package::by_id("boingbag-39-2").unwrap();

        record_amiga_install(&tree, "boingbag-39-1", "ARTPkg:BoingBag3.9-1/C/Updater").unwrap();

        assert_eq!(
            missing_prerequisites(&two, &tree).unwrap(),
            Vec::<String>::new()
        );
        refuse_unless_installable(&two, &tree).unwrap();
    }

    /// The half that made the refusal usable at all. A BoingBag cannot be
    /// placed from the host, so if the Amiga-side run left no trace, this
    /// tree would look exactly like one that never had BoingBag 1 — and
    /// BoingBag 2 would be refused for ever.
    #[test]
    fn an_amiga_side_install_is_recorded_and_read_back() {
        let dir = scratch("record");
        let tree = tree_with(dir.path(), &["workbench-base"]);

        assert!(!applied(&tree).unwrap().contains("boingbag-39-1"));
        record_amiga_install(
            &tree,
            "boingbag-39-1",
            "ARTPkg:BoingBag3.9-1/C/Updater DH0:",
        )
        .unwrap();
        assert!(applied(&tree).unwrap().contains("boingbag-39-1"));

        let manifest = read_manifest(&tree).unwrap();
        assert_eq!(manifest.amiga_installed.len(), 1);
        assert_eq!(manifest.amiga_installed[0].package, "boingbag-39-1");
        assert_eq!(
            manifest.amiga_installed[0].command,
            "ARTPkg:BoingBag3.9-1/C/Updater DH0:"
        );
        assert_eq!(
            manifest.files.len(),
            1,
            "the rest of the tree's own account of itself survives"
        );
        assert_eq!(manifest.release, "amigaos-3.9");
    }

    /// ART never *creates* a `distribution.json` for a tree it did not build.
    ///
    /// The other way to make the two halves agree — considered in fix round 1
    /// and rejected — is to let recording invent a manifest when there is
    /// none. It would carry a `release` ART does not know and an empty
    /// `files[]`, and put both in front of `verify`, `collide` and
    /// `apply`'s own `classify_incoming`, which today refuses outright on a
    /// manifest-less tree because it cannot say what adding a component would
    /// replace; a synthesised empty one turns that honest refusal into
    /// "nothing was there".
    ///
    /// **The choice is not observable from the outcome** — both ways make the
    /// two halves agree, so `every_run_that_is_allowed_can_also_record_that_
    /// it_worked` passes under either. Measured in fix round 1's mutation run:
    /// swapping one for the other left the whole suite green. So it is
    /// asserted directly, here, or the next reader could change the decision
    /// without anything noticing.
    #[test]
    fn recording_never_creates_a_manifest_for_a_tree_art_did_not_build() {
        let dir = scratch("no-invention");
        let tree = dir.join("Workbench3.9");
        std::fs::create_dir_all(&tree).unwrap();

        assert!(record_amiga_install(&tree, "boingbag-39-1", "line").is_err());
        assert!(
            !tree.join(MANIFEST_FILE_NAME).exists(),
            "ART must not write a distribution.json into a tree it did not build"
        );
    }

    /// Twice is once. Re-running a package is legitimate; a tree claiming it
    /// twice is not.
    #[test]
    fn recording_the_same_package_twice_adds_one_row() {
        let dir = scratch("record-twice");
        let tree = tree_with(dir.path(), &["workbench-base"]);

        record_amiga_install(&tree, "boingbag-39-1", "first").unwrap();
        record_amiga_install(&tree, "boingbag-39-1", "second").unwrap();

        let manifest = read_manifest(&tree).unwrap();
        assert_eq!(manifest.amiga_installed.len(), 1);
        assert_eq!(
            manifest.amiga_installed[0].command, "first",
            "the first run's own line stays; a re-run does not rewrite history"
        );
    }

    /// The trap this module was warned about. A tree with no manifest is
    /// refused too, but for its own reason and with its own sentence — a
    /// test that only asserted "it refused" would pass against a prerequisite
    /// check that never ran.
    #[test]
    fn a_tree_with_no_manifest_is_a_different_refusal() {
        let dir = scratch("no-manifest");
        let tree = dir.join("Workbench3.9");
        std::fs::create_dir_all(&tree).unwrap();
        let two = package::by_id("boingbag-39-2").unwrap();

        let message = refuse_unless_installable(&two, &tree)
            .unwrap_err()
            .to_string();
        assert!(
            message.contains(MANIFEST_FILE_NAME),
            "it must say ART cannot read this tree: {message}"
        );
        assert!(
            !message.contains("BoingBag 3.9-1"),
            "and it must not claim a package is missing, which it cannot know: {message}"
        );
    }

    /// A package that requires nothing **still** needs a tree ART can account
    /// for — the fix round 1 Major, from this side.
    ///
    /// This test asserted the opposite until 2026-08-21: BoingBag 1 requires
    /// nothing, so the tree was never read and a hand-made folder was a
    /// permitted run. On such a folder the installer would then work and
    /// [`record_amiga_install`] would fail, and the user would be told the
    /// install failed after it had succeeded.
    ///
    /// The refusal is the **manifest** one, never "a package is missing" —
    /// asserted here, because a refusal naming BoingBag 3.9-1 about a tree
    /// nobody can read would claim something ART does not know (§89).
    #[test]
    fn a_package_that_requires_nothing_still_needs_a_tree_art_can_account_for() {
        let dir = scratch("no-requires");
        let tree = dir.join("nothing-here");
        std::fs::create_dir_all(&tree).unwrap();
        let one = package::by_id("boingbag-39-1").unwrap();

        assert!(one.requires.is_empty());
        let message = refuse_unless_installable(&one, &tree)
            .unwrap_err()
            .to_string();
        assert!(
            message.contains(MANIFEST_FILE_NAME),
            "it must say ART cannot read this tree: {message}"
        );
        assert!(
            !message.contains("BoingBag"),
            "and must not claim a package is missing, which it cannot know: {message}"
        );

        // And on a tree ART *can* account for it goes through — a refusal
        // that fired either way would be no check at all.
        let real = tree_with(dir.path(), &["workbench-base"]);
        refuse_unless_installable(&one, &real).unwrap();
    }

    /// **The two halves have to agree, and this is the assertion that makes
    /// them.**
    ///
    /// Fix round 1's Major was in neither half. `record_amiga_install`
    /// refused a manifest-less tree while `missing_prerequisites` permitted a
    /// run against one, and the gap between them turned a successful install
    /// into a reported failure. So the property is asserted over every tree
    /// shape rather than by one example: **whatever this module lets a run
    /// start against, it must be able to record a success into.**
    ///
    /// A fresh tree per pair, because recording mutates the tree it is given.
    #[allow(clippy::type_complexity)]
    #[test]
    fn every_run_that_is_allowed_can_also_record_that_it_worked() {
        type MakeTree = Box<dyn Fn(&std::path::Path) -> std::path::PathBuf>;
        let shapes: Vec<(&str, MakeTree)> = vec![
            (
                "no-manifest",
                Box::new(|at: &std::path::Path| {
                    let tree = at.join("Workbench3.9");
                    std::fs::create_dir_all(&tree).unwrap();
                    tree
                }),
            ),
            (
                "unreadable-manifest",
                Box::new(|at: &std::path::Path| {
                    let tree = at.join("Workbench3.9");
                    std::fs::create_dir_all(&tree).unwrap();
                    std::fs::write(tree.join(MANIFEST_FILE_NAME), b"{ not json").unwrap();
                    tree
                }),
            ),
            (
                "empty-manifest",
                Box::new(|at: &std::path::Path| tree_with(at, &[])),
            ),
            (
                "base-only",
                Box::new(|at: &std::path::Path| tree_with(at, &["workbench-base"])),
            ),
            (
                "base-and-bb1",
                Box::new(|at: &std::path::Path| {
                    tree_with(at, &["workbench-base", "boingbag-39-1"])
                }),
            ),
        ];

        let mut allowed_any = 0usize;
        for (shape, make) in &shapes {
            for id in ["boingbag-39-1", "boingbag-39-2"] {
                let dir = scratch(&format!("agree-{shape}-{id}"));
                let tree = make(dir.path());
                let package = package::by_id(id).unwrap();

                let allowed = refuse_unless_installable(&package, &tree).is_ok();
                let recordable = record_amiga_install(&tree, id, "line").is_ok();
                assert!(
                    !allowed || recordable,
                    "{shape}/{id}: the run is allowed but its success could not be recorded \
                     — the installer would work and ART would report that it failed"
                );
                allowed_any += usize::from(allowed);
            }
        }
        assert!(
            allowed_any >= 2,
            "a check that allowed nothing at all would satisfy this vacuously"
        );
    }

    /// A component the tree was *built* from counts as applied, without any
    /// Amiga-side record — the manifest's original half still answers.
    #[test]
    fn a_component_in_the_manifests_files_counts_as_applied() {
        let dir = scratch("host-placed");
        let tree = tree_with(dir.path(), &["workbench-base", "boingbag-39-1"]);
        let two = package::by_id("boingbag-39-2").unwrap();

        assert_eq!(
            missing_prerequisites(&two, &tree).unwrap(),
            Vec::<String>::new()
        );
        refuse_unless_installable(&two, &tree).unwrap();
    }

    /// Transitive, and in install order. Built from a fabricated chain rather
    /// than the shipped one, because today's shipped chain is only two deep
    /// and a one-step-only implementation would pass against it.
    #[test]
    fn a_chain_two_deep_is_reported_whole_and_in_order() {
        // `prerequisite_chain` resolves through `package::by_id`, so this
        // exercises the real shipped graph: BoingBag 2 -> BoingBag 1.
        let dir = scratch("chain");
        let tree = tree_with(dir.path(), &["workbench-base"]);
        let two = package::by_id("boingbag-39-2").unwrap();
        assert_eq!(prerequisite_chain(&two).unwrap(), vec!["boingbag-39-1"]);

        // And the ordering itself, over a chain that is genuinely two deep,
        // through the same `package::order` this module defers to.
        let ordered =
            package::order(&["boingbag-39-2".to_string(), "boingbag-39-1".to_string()]).unwrap();
        assert_eq!(ordered, vec!["boingbag-39-1", "boingbag-39-2"]);

        assert_eq!(
            missing_prerequisites(&two, &tree).unwrap(),
            vec!["boingbag-39-1".to_string()]
        );
    }

    // -----------------------------------------------------------------
    // Round 3: the whole chain, as rows
    // -----------------------------------------------------------------

    /// A manifest built inline, so a test can say exactly what a tree
    /// records without writing one to disk. `rows_for` takes the manifest
    /// rather than a path for precisely this reason: the chain screen has
    /// already read it once.
    fn manifest(built_from: &[&str], components: &[&str], ran: &[&str]) -> DistributionManifest {
        DistributionManifest {
            release: "AmigaOS 3.9".into(),
            built_from: built_from
                .iter()
                .map(|volume| super::super::apply::MediaRecord {
                    volume_name: (*volume).to_string(),
                    sha256: "0".repeat(64),
                    distinguished_by: None,
                })
                .collect(),
            files: components.iter().map(|c| file_from(c)).collect(),
            paired_rom: None,
            amiga_installed: ran
                .iter()
                .map(|package| AmigaInstallRecord {
                    package: (*package).to_string(),
                    command: "PKG:C/Updater AmigaOS-Update SYS:".into(),
                })
                .collect(),
            layers: Vec::new(),
        }
    }

    /// Every fact empty — nothing in any folder, no ROM, no chosen file.
    /// `slots::resolve` opens nothing, so this is the whole of it.
    fn resolved(
        manifest: Option<&DistributionManifest>,
        packages: &[super::super::scan::FoundPackage],
        media: &[super::super::scan::FoundMedia],
    ) -> Vec<SlotState> {
        let slots = super::super::slots::slots_for("AmigaOS 3.9").unwrap();
        let facts = super::super::slots::Facts {
            media,
            packages,
            hashes: &[],
            manifest,
            rom: None,
            overrides: &[],
            disc_roots: &[],
        };
        super::super::slots::resolve(&slots, &facts)
    }

    /// An archive in a folder, identified by the top-level directory it
    /// states — rank 2, which is all a test needs to make a slot `found`.
    fn archive(path: &str, top_level: &str) -> super::super::scan::FoundPackage {
        super::super::scan::FoundPackage {
            path: std::path::PathBuf::from(path),
            media: top_level.to_string(),
            refused_names: Vec::new(),
        }
    }

    fn row<'a>(rows: &'a [ChainRow], id: &str) -> &'a ChainRow {
        rows.iter()
            .find(|row| row.package_id.as_deref() == Some(id))
            .unwrap_or_else(|| panic!("no row for {id} in {rows:#?}"))
    }

    /// **The chain is nine rows in the material's own order**, and the order
    /// is read out of the recipes rather than out of a list here.
    ///
    /// Asserted as `(position, id)` pairs rather than as a count: a count
    /// passes while the list holds the wrong things, and the *order* is the
    /// whole information this screen carries. Two rows share rank 4 because
    /// the material states no order between them.
    #[test]
    fn the_chain_is_the_materials_own_order_with_the_cd_first() {
        let rows = rows_for("AmigaOS 3.9", None, &resolved(None, &[], &[])).unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| (row.position, row.package_id.clone()))
                .collect::<Vec<_>>(),
            vec![
                (1, None),
                (2, Some("boingbag-39-1".to_string())),
                (3, Some("boingbag-39-2".to_string())),
                (4, Some("locale-39".to_string())),
                (4, Some("locale-39-turkish".to_string())),
                (5, Some("locale-turkish".to_string())),
                (6, Some("boingbag-39-2-contribution".to_string())),
                (7, Some("euro-update".to_string())),
                (8, Some("boingbags-39-3-4".to_string())),
            ]
        );
        // Row 1 is the medium, synthesised from the slot and never from a
        // package — it has no `package_id` and it is fed by the CD's slot.
        assert_eq!(rows[0].slot_id.as_deref(), Some("medium:AmigaOS3.9"));
        assert_eq!(rows[0].sentence_facts.runs_on_amiga, None);
    }

    /// **The CD row never takes the Run button, in the one state where it
    /// used to** (round 3 whole-branch review, M3).
    ///
    /// The state is ordinary, not exotic: a 3.9 tree whose manifest records
    /// no `AmigaOS3.9` in `built_from` — an imported tree, or one built
    /// before ART recorded media — with the ISO sitting in a folder the user
    /// named. `medium_state` answered `Ready`, `chainLines` marks the first
    /// ready row runnable, and the single Run button landed on a disc it can
    /// never run while the BoingBag below it was never offered.
    ///
    /// Asserted at both ends, because either alone would pass a defect: the
    /// medium row is not `Ready` **and** the first ready row is a package.
    #[test]
    fn the_cd_row_is_never_ready_so_the_run_button_reaches_the_first_ready_package() {
        let media = [super::super::scan::FoundMedia {
            path: std::path::PathBuf::from("D:/a/AmigaOS39.iso"),
            kind: super::super::scan::MediaKind::Disc,
            volume_name: "AmigaOS3.9".to_string(),
            layer: None,
        }];
        let packages = [archive("D:/a/BoingBag39-1.lha", "BoingBag3.9-1")];
        // Built from nothing ART recorded, and carrying every component, so
        // the BoingBag below is genuinely ready and the comparison is real.
        let tree = manifest(&[], &["workbench-base", "locale-base"], &[]);
        let rows = rows_for(
            "AmigaOS 3.9",
            Some(&tree),
            &resolved(Some(&tree), &packages, &media),
        )
        .unwrap();

        assert_eq!(rows[0].package_id, None, "the premise: row 1 is the medium");
        assert!(
            !matches!(rows[0].state, ChainState::Ready),
            "the CD row may never be ready — the Run button would land on a disc: {:?}",
            rows[0].state
        );
        assert!(
            matches!(rows[0].state, ChainState::Missing { .. }),
            "and it says what is actually true of the tree: {:?}",
            rows[0].state
        );

        // The control, and the half that says the button has somewhere to
        // go: the first row that *is* ready is a package, and it is the one
        // whose archive is in the folder.
        let first_ready = rows
            .iter()
            .find(|row| matches!(row.state, ChainState::Ready))
            .expect("a ready row, or this proves nothing");
        assert_eq!(first_ready.package_id.as_deref(), Some("boingbag-39-1"));
    }

    /// **A tree built without a component a package needs reads
    /// `BlockedByComponent`, not `Ready`** (round 3 whole-branch review, M4).
    ///
    /// `locale-base` is `required: false` in `amigaos-3.9.json`, so a tree
    /// without it is ordinary. Before this the three `locale-*` rows read
    /// *ready*, the one Run button armed on the first of them — taking it
    /// from a later row that really was ready — and
    /// `resolve_packages_for_add` then refused with
    /// `PackageComponentMissing`. The screen out-claiming the core.
    ///
    /// The control is the same tree with the component, because a check that
    /// blocked every row would pass the first assertion and prove nothing.
    #[test]
    fn a_package_whose_component_the_tree_lacks_is_blocked_by_that_component() {
        let packages = [archive("D:/a/Locale3_9.lha", "Locale3.9")];
        let media = [super::super::scan::FoundMedia {
            path: std::path::PathBuf::from("D:/a/AmigaOS39.iso"),
            kind: super::super::scan::MediaKind::Disc,
            volume_name: "AmigaOS3.9".to_string(),
            layer: None,
        }];

        let without = manifest(&["AmigaOS3.9"], &["workbench-base"], &[]);
        let rows = rows_for(
            "AmigaOS 3.9",
            Some(&without),
            &resolved(Some(&without), &packages, &media),
        )
        .unwrap();
        match &row(&rows, "locale-39").state {
            ChainState::BlockedByComponent { components } => {
                assert_eq!(components.len(), 1, "one component, named: {components:?}");
                assert_eq!(components[0].id, "locale-base");
                // The key, not a rendered name: the words are the
                // catalogue's, and this is the key the components screen
                // labels the same component by.
                assert_eq!(
                    components[0].label_key.as_deref(),
                    Some("osinstall.components.name.os39.locale")
                );
            }
            other => panic!("locale-39 must be blocked on its component, got {other:?}"),
        }

        // The control: add the component and the same row reads ready.
        let with = manifest(&["AmigaOS3.9"], &["workbench-base", "locale-base"], &[]);
        let rows = rows_for(
            "AmigaOS 3.9",
            Some(&with),
            &resolved(Some(&with), &packages, &media),
        )
        .unwrap();
        assert_eq!(
            row(&rows, "locale-39").state,
            ChainState::Ready,
            "with the component, the same row is ready"
        );
    }

    /// **A release with no chain is an empty list, not a lone CD row** (fix
    /// round 1, m3).
    ///
    /// AmigaOS 3.2 ships no update package at all, and taking the first
    /// medium slot regardless produced one row built from whichever floppy
    /// happened to sort first — position 1, and a summary reading *"AmigaOS
    /// 3.2 updates — 0 of 1 applied"*. The control is beside it, because a
    /// `rows_for` that answered empty for everything would pass the first
    /// assertion and prove nothing.
    #[test]
    fn a_release_with_no_chain_rows_has_no_chain_at_all() {
        let slots = super::super::slots::slots_for("AmigaOS 3.2").unwrap();
        assert!(
            slots.iter().any(|slot| slot.kind == SlotKind::Medium),
            "the premise: 3.2 has medium slots, so a first one exists to be taken by mistake"
        );
        let facts = super::super::slots::Facts {
            media: &[],
            packages: &[],
            hashes: &[],
            manifest: None,
            rom: None,
            overrides: &[],
            disc_roots: &[],
        };
        let states = super::super::slots::resolve(&slots, &facts);
        let rows = rows_for("AmigaOS 3.2", None, &states).unwrap();
        assert!(rows.is_empty(), "3.2 has no update package: {rows:#?}");
        assert_eq!(summarize_chain("AmigaOS 3.2", &rows).total, 0);

        // The control: the release that does have a chain still has one.
        assert_eq!(
            rows_for("AmigaOS 3.9", None, &resolved(None, &[], &[]))
                .unwrap()
                .len(),
            9
        );
    }

    /// **What a row waits for is named in the order it goes on** (fix round
    /// 1, m2, and the design says so: *"blocked_by names rows, in position
    /// order"*).
    ///
    /// **Only one half survives, since 2026-09-08.** The two halves used to
    /// arrive in two different orders — the packages in `package::order`'s
    /// topological one, the media appended after them — so BoingBag 3.9-2
    /// blocked on both answered `["BoingBag 3.9-1", "AmigaOS3.9"]`: rank 2
    /// before rank 1. The medium half came from
    /// `amiga_installer.required_medium` and went with the emulator route; the
    /// sort in `package_state` is kept, because the two-source shape it
    /// answers is still there in `state.blocked_by` and the day a second
    /// source returns this is what keeps the order right.
    #[test]
    fn a_blocked_row_names_what_it_waits_for_in_the_order_it_goes_on() {
        let packages = [archive("D:/a/BoingBag39-2.lha", "BoingBag3.9-2")];
        let rows = rows_for("AmigaOS 3.9", None, &resolved(None, &packages, &[])).unwrap();
        assert_eq!(
            row(&rows, "boingbag-39-2").state,
            ChainState::BlockedBy {
                names: vec!["BoingBag 3.9-1".to_string()]
            },
            "BoingBag 3.9-1 is row 2 and this is row 3"
        );
    }

    /// **`installed` comes from the manifest and from nothing else.** Two
    /// arms over one set of facts: the same folders, the same slots, and
    /// only the manifest differing.
    #[test]
    fn installed_is_the_manifests_word_for_it_and_a_run_and_a_placement_both_count() {
        let bare = rows_for("AmigaOS 3.9", None, &resolved(None, &[], &[])).unwrap();
        assert!(
            !matches!(
                row(&bare, "boingbag-39-1").state,
                ChainState::Installed { .. }
            ),
            "no manifest says nothing about what is installed"
        );

        let done = manifest(&["AmigaOS3.9"], &["locale-turkish"], &["boingbag-39-1"]);
        let states = resolved(Some(&done), &[], &[]);
        let rows = rows_for("AmigaOS 3.9", Some(&done), &states).unwrap();

        // A run — `amiga_installed`.
        assert_eq!(
            row(&rows, "boingbag-39-1").state,
            ChainState::Installed { when: None }
        );
        // A placement — a `files[]` record naming the component.
        assert_eq!(
            row(&rows, "locale-turkish").state,
            ChainState::Installed { when: None }
        );
        // And the CD, from `built_from`.
        assert_eq!(rows[0].state, ChainState::Installed { when: None });
        assert_eq!(
            summarize_chain("AmigaOS 3.9", &rows).installed,
            3,
            "three rows the manifest records, and no fourth"
        );
    }

    /// **A row is ready when its own artefact is in hand and everything
    /// before it is done** — and the control beside it: the same archive,
    /// the same folders, a tree that has not had BoingBag 3.9-1 run on it,
    /// and the row is blocked by name (ART-186, through the one
    /// implementation `refuse_unless_installable` also uses).
    #[test]
    fn a_row_is_ready_only_when_what_goes_before_it_is_installed() {
        let media = [super::super::scan::FoundMedia {
            path: std::path::PathBuf::from("D:/a/AmigaOS39.iso"),
            kind: super::super::scan::MediaKind::Disc,
            volume_name: "AmigaOS3.9".to_string(),
            layer: None,
        }];
        let packages = [archive("D:/a/BoingBag39-2.lha", "BoingBag3.9-2")];

        let without = manifest(&["AmigaOS3.9"], &[], &[]);
        let rows = rows_for(
            "AmigaOS 3.9",
            Some(&without),
            &resolved(Some(&without), &packages, &media),
        )
        .unwrap();
        assert_eq!(
            row(&rows, "boingbag-39-2").state,
            ChainState::BlockedBy {
                names: vec!["BoingBag 3.9-1".to_string()]
            },
            "the row names the package, never the slot id"
        );

        let with = manifest(&["AmigaOS3.9"], &[], &["boingbag-39-1"]);
        let rows = rows_for(
            "AmigaOS 3.9",
            Some(&with),
            &resolved(Some(&with), &packages, &media),
        )
        .unwrap();
        assert_eq!(row(&rows, "boingbag-39-2").state, ChainState::Ready);
        assert_eq!(
            row(&rows, "boingbag-39-2").sentence_facts.file.as_deref(),
            Some("BoingBag39-2.lha"),
            "the file travels with the row whatever its state"
        );
        assert_eq!(
            row(&rows, "boingbag-39-2").sentence_facts.runs_on_amiga,
            Some(false),
            "since 2026-09-08 this row is placed from Windows — see the precedence test below"
        );
    }

    /// **Which of the two routes a row takes, and what decides it**
    /// (2026-09-08, ART-166's reversal).
    ///
    /// `runs_on_amiga` is what `AmigaInstallPanel` switches on: `Some(true)`
    /// composes an emulator run, `Some(false)` goes through
    /// `useHostPlacement`, `None` is neither. The precedence used to be
    /// "an `amiga_installer` wins", and under that reading both BoingBags —
    /// which still declare one — would have gone on offering a ~140 s
    /// emulator run needing a ROM and a licence for work the host now does
    /// in seconds.
    ///
    /// All three arms, from the shipped recipes rather than from fixtures,
    /// because each is a different package's real shape and the table is
    /// only a guard if the middle one is genuinely occupied.
    #[test]
    fn the_block_decides_which_route_a_row_takes_and_the_installer_only_breaks_the_tie() {
        let media = [super::super::scan::FoundMedia {
            path: std::path::PathBuf::from("D:/a/AmigaOS39.iso"),
            kind: super::super::scan::MediaKind::Disc,
            volume_name: "AmigaOS3.9".to_string(),
            layer: None,
        }];
        let manifest = manifest(&["AmigaOS3.9"], &[], &[]);
        let rows = rows_for(
            "AmigaOS 3.9",
            Some(&manifest),
            &resolved(Some(&manifest), &[], &media),
        )
        .unwrap();

        for id in ["boingbag-39-1", "boingbag-39-2"] {
            let package = super::super::package::by_id(id).unwrap();
            assert_eq!(package.host_placement_block, None, "the premise for {id}");
            assert_eq!(
                row(&rows, id).sentence_facts.runs_on_amiga,
                Some(false),
                "{id} is placed from Windows"
            );
        }

        // A block *and* an installer: the emulator, as before.
        assert_eq!(
            row(&rows, "boingbags-39-3-4").sentence_facts.runs_on_amiga,
            Some(true),
            "needs-installer-script, and it declares an installer"
        );
        // A block and no installer: neither route, which is not the same as
        // either of the other two and must not be reported as one.
        assert_eq!(
            row(&rows, "euro-update").sentence_facts.runs_on_amiga,
            None,
            "needs-fixfonts, and nothing to run on the Amiga either"
        );
        // And the ordinary host package, so `Some(false)` above is not
        // simply what every row answers.
        assert_eq!(
            row(&rows, "locale-turkish").sentence_facts.runs_on_amiga,
            Some(false)
        );
    }

    /// **A package placed from Windows waits for no disc** (review F6,
    /// 2026-09-08).
    ///
    /// This test used to assert the opposite, with the comment *"BoingBag
    /// 3.9-1's own Updater checks for it before it does anything"* — true of
    /// the `Updater`, and the row no longer runs it. The requirement came from
    /// `amiga_installer.required_medium`, a fact about a program, and it gated
    /// a row that is now placed from Windows where nothing reads the disc. On
    /// a tree whose manifest does not record the disc, the row read
    /// `BlockedBy ["AmigaOS3.9"]` and withheld a host placement that needs no
    /// disc at all.
    ///
    /// **The disc is still required — by the tree.** `medium:AmigaOS3.9`
    /// carries `required: true` because the release's own required components
    /// read from it, and the CD is row 1 of the chain in its own right. Both
    /// arms here, because "no disc anywhere" is exactly the arrangement that
    /// used to block.
    #[test]
    fn a_package_placed_from_windows_does_not_wait_for_the_disc() {
        let packages = [archive("D:/a/BoingBag39-1.lha", "BoingBag3.9-1")];

        let rows = rows_for("AmigaOS 3.9", None, &resolved(None, &packages, &[])).unwrap();
        assert_eq!(
            row(&rows, "boingbag-39-1").state,
            ChainState::Ready,
            "no disc in any folder, and the row is placed from Windows"
        );

        // And with the ISO there, unchanged — so the arm above is not simply
        // every row answering `Ready`.
        let media = [super::super::scan::FoundMedia {
            path: std::path::PathBuf::from("D:/a/AmigaOS39.iso"),
            kind: super::super::scan::MediaKind::Disc,
            volume_name: "AmigaOS3.9".to_string(),
            layer: None,
        }];
        let rows = rows_for("AmigaOS 3.9", None, &resolved(None, &packages, &media)).unwrap();
        assert_eq!(row(&rows, "boingbag-39-1").state, ChainState::Ready);

        // The control, and it is what makes the medium slot's own
        // `required` the right place for the disc: the CD row itself is
        // `Missing` until the tree's manifest says it was built from it.
        let cd = rows
            .iter()
            .find(|row| row.package_id.is_none())
            .expect("the CD is row 1 of the chain in its own right");
        assert_eq!(cd.name, "AmigaOS3.9");
        assert!(
            matches!(cd.state, ChainState::Missing { .. }),
            "got {:?}",
            cd.state
        );
    }

    /// **Superseded and installed reads *not needed*, and the row names the
    /// package that made it so.** The control is the same chain with
    /// BoingBags 3&4 absent from the manifest, where Euro-Update is an
    /// ordinary row.
    #[test]
    fn a_superseded_row_whose_superseder_is_installed_is_not_needed() {
        let without = manifest(&[], &[], &[]);
        let rows = rows_for(
            "AmigaOS 3.9",
            Some(&without),
            &resolved(Some(&without), &[], &[]),
        )
        .unwrap();
        assert!(
            !matches!(
                row(&rows, "euro-update").state,
                ChainState::NotNeeded { .. }
            ),
            "nothing has superseded it yet: {:?}",
            row(&rows, "euro-update").state
        );

        let with = manifest(&[], &["boingbags-39-3-4"], &[]);
        let rows = rows_for("AmigaOS 3.9", Some(&with), &resolved(Some(&with), &[], &[])).unwrap();
        assert_eq!(
            row(&rows, "euro-update").state,
            ChainState::NotNeeded {
                superseded_by: "BoingBags 3&4 for AmigaOS 3.9".to_string()
            },
            "the name, not the id — the words belong in the catalogue"
        );
        assert_eq!(summarize_chain("AmigaOS 3.9", &rows).not_needed, 1);
    }

    /// **The owner's finding of 2026-09-10.** Locale 3.9's Turkish slice was
    /// ticked on a tree that already had BoingBag 3.9-2's Turkish catalogs,
    /// and the run refused it — 82 files it would write over. Measured on the
    /// owner's own archives the same day: 32 of its 33 catalogs are in the
    /// newer package too, every one of them the same version or older. So
    /// the row is not *not needed* (it carries `sys/ahi.catalog` and two font
    /// families nothing else does) and it is not *ready*: a newer update that
    /// writes over it, by its own `overrides`, is already in the tree, and
    /// adding it now would put older files over newer ones. The control is
    /// the same chain without that update, where it is an ordinary row.
    #[test]
    fn a_row_an_installed_later_package_writes_over_is_overtaken() {
        let without = manifest(&[], &[], &[]);
        let rows = rows_for(
            "AmigaOS 3.9",
            Some(&without),
            &resolved(Some(&without), &[], &[]),
        )
        .unwrap();
        assert!(
            !matches!(
                row(&rows, "locale-39-turkish").state,
                ChainState::OvertakenBy { .. }
            ),
            "nothing newer is in the tree yet: {:?}",
            row(&rows, "locale-39-turkish").state
        );

        let with = manifest(&[], &["locale-turkish"], &[]);
        let rows = rows_for("AmigaOS 3.9", Some(&with), &resolved(Some(&with), &[], &[])).unwrap();
        assert_eq!(
            row(&rows, "locale-39-turkish").state,
            ChainState::OvertakenBy {
                names: vec![package::by_id("locale-turkish").unwrap().name]
            },
            "the newer package's name, not its id"
        );
    }

    /// **An artefact ART cannot find names the files to go and get**, and a
    /// row two files could be is refused with both of them rather than
    /// resolved by picking one.
    #[test]
    fn a_missing_row_names_the_file_and_an_ambiguous_one_names_the_candidates() {
        let rows = rows_for("AmigaOS 3.9", None, &resolved(None, &[], &[])).unwrap();
        assert_eq!(
            row(&rows, "boingbag-39-1").state,
            ChainState::Missing {
                expected: vec!["BoingBag39-1.lha".to_string()]
            }
        );

        let two = [
            archive("D:/a/BoingBag39-1.lha", "BoingBag3.9-1"),
            archive("D:/b/BoingBag39-1.lha", "BoingBag3.9-1"),
        ];
        let rows = rows_for("AmigaOS 3.9", None, &resolved(None, &two, &[])).unwrap();
        assert_eq!(
            row(&rows, "boingbag-39-1").state,
            ChainState::Refused {
                reason: RefusedBecause::Ambiguous {
                    candidates: vec![
                        "D:/a/BoingBag39-1.lha".to_string(),
                        "D:/b/BoingBag39-1.lha".to_string(),
                    ]
                }
            },
            "whole paths: two copies under one name is the commonest shape"
        );
    }

    /// **A package with no route at all is refused and says which**, and the
    /// two refusals are different values rather than one shrug. Euro-Update
    /// cannot be placed (`needs-fixfonts`) and has no Amiga-side installer;
    /// BoingBag 3.9-1 cannot be placed either and *does*, so it is never
    /// refused for the same reason.
    #[test]
    fn a_package_with_no_route_is_refused_by_the_block_that_is_true_of_it() {
        let rows = rows_for("AmigaOS 3.9", None, &resolved(None, &[], &[])).unwrap();
        assert_eq!(
            row(&rows, "euro-update").state,
            ChainState::Refused {
                reason: RefusedBecause::NotPlaceable {
                    block: super::super::HostPlacementBlock::NeedsFixfonts
                }
            }
        );
        assert_eq!(row(&rows, "euro-update").sentence_facts.runs_on_amiga, None);

        assert!(
            !matches!(
                row(&rows, "boingbag-39-1").state,
                ChainState::Refused { .. }
            ),
            "a package ART can run on the Amiga is not refused for being unplaceable"
        );
        // And the one host-placeable row of the chain says so.
        assert_eq!(
            row(&rows, "boingbag-39-2-contribution")
                .sentence_facts
                .runs_on_amiga,
            Some(false)
        );
    }

    /// **A declaration nobody has run outranks everything else about the
    /// row.** BoingBags 3&4's archive is in the folder and BoingBag 3.9-2 is
    /// installed, so every other check would have said *ready*; what the
    /// user has to know is that ART has never driven the thing.
    #[test]
    fn a_row_nobody_has_run_says_so_however_ready_the_rest_of_it_looks() {
        let done = manifest(&["AmigaOS3.9"], &[], &["boingbag-39-1", "boingbag-39-2"]);
        let packages = [archive("D:/a/BoingBags3&4.lha", "BoingBag3.9-3&4")];
        let rows = rows_for(
            "AmigaOS 3.9",
            Some(&done),
            &resolved(Some(&done), &packages, &[]),
        )
        .unwrap();
        assert_eq!(
            row(&rows, "boingbags-39-3-4").state,
            ChainState::NotYetRunnable {
                reason: NotYetRunnable::InstallerNotMeasured
            }
        );
        assert_eq!(
            row(&rows, "boingbags-39-3-4")
                .sentence_facts
                .file
                .as_deref(),
            Some("BoingBags3&4.lha"),
            "the archive is there, and the row still says what has not been measured"
        );

        // **But the manifest outranks it.** A tree that records the package
        // has had it done, and saying "ART has never driven this one" over
        // that is the screen out-claiming the core — the check ordering that
        // running this test corrected.
        let already = manifest(&["AmigaOS3.9"], &["boingbags-39-3-4"], &[]);
        let rows = rows_for(
            "AmigaOS 3.9",
            Some(&already),
            &resolved(Some(&already), &packages, &[]),
        )
        .unwrap();
        assert_eq!(
            row(&rows, "boingbags-39-3-4").state,
            ChainState::Installed { when: None }
        );
    }

    /// The summary counts what the rows say and nothing else — and a
    /// *not needed* row is in neither the installed count nor the
    /// outstanding one.
    #[test]
    fn the_summary_counts_the_rows_the_manifest_accounts_for() {
        let done = manifest(&["AmigaOS3.9"], &["boingbags-39-3-4"], &["boingbag-39-1"]);
        let rows = rows_for("AmigaOS 3.9", Some(&done), &resolved(Some(&done), &[], &[])).unwrap();
        let summary = summarize_chain("AmigaOS 3.9", &rows);
        assert_eq!(summary.release, "AmigaOS 3.9");
        assert_eq!(summary.total, 9);
        // The CD, BoingBag 3.9-1 and BoingBags 3&4 — three rows the manifest
        // records; Euro-Update is not needed and is counted apart.
        assert_eq!((summary.installed, summary.not_needed), (3, 1));
    }

    /// A manifest that is there but is not JSON is not silently treated as an
    /// empty tree — that would turn an unreadable tree into "nothing is
    /// installed" and let the very run this module refuses go ahead.
    #[test]
    fn an_unreadable_manifest_refuses_rather_than_reading_as_empty() {
        let dir = scratch("bad-manifest");
        let tree = dir.join("Workbench3.9");
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::write(tree.join(MANIFEST_FILE_NAME), b"{ not json").unwrap();
        let two = package::by_id("boingbag-39-2").unwrap();

        assert!(refuse_unless_installable(&two, &tree).is_err());
    }
}
