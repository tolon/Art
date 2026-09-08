//! What one release's build actually needs, as named slots — and what the
//! folders a user pointed at turn out to fill.
//!
//! ## Why this module exists
//!
//! Until now the OS Builder asked for the same material in four places (the
//! `kaynak` step's media folder, its extra folders, the `paketler` step's
//! package folder, and the Amiga-side panel's three browse buttons), and each
//! place resolved what it was given by a different rule. A person holding the
//! files had to know which sentence each field wanted. This module is the one
//! answer: **every artefact a release can use is a slot, and every slot is
//! resolved once.**
//!
//! Nothing here is new identification. [`super::scan::find_media`] already
//! reads a disk's volume name off its root block, [`super::scan::find_packages`]
//! already reads an archive's single top-level directory from inside it, and
//! [`super::mediahash`] already hashes a candidate and looks the hash up in
//! ART's two tables. This module **joins** those three answers to the slots a
//! recipe implies. It is pure: no file is opened here, no folder is read, and
//! nothing is written — the caller gathers the facts and hands them over in
//! [`Facts`].
//!
//! ## The slot list is derived, never written down
//!
//! [`slots_for`] reads the release's own recipe and the packages that declare
//! that release, and derives every slot from them. A fifth package, or a
//! sixth component off a new disc, joins the readout by being data — which is
//! the same rule `recipe.rs` and `package.rs` already keep, carried one level
//! up. A list of slots in code would be a fifth place for the material to be
//! described, and it would drift from the four that already exist.
//!
//! ## How a slot is filled, and why the rank is what it is
//!
//! Four ranks, strongest first, and they are not interchangeable:
//!
//! 1. [`MatchedBy::Hash`] — the file's bytes are in a row whose artefact is
//!    this slot's. A fact about the bytes, and the only rank that carries the
//!    row and its confirmation.
//! 2. [`MatchedBy::VolumeName`] / [`MatchedBy::TopLevelDirectory`] — the
//!    medium says it *is* this thing. What the artefact claims about itself,
//!    which is weaker than what its bytes prove but far stronger than its
//!    file name: a renamed disk still answers correctly.
//! 3. [`MatchedBy::Filename`] — **a guess, and it never fills
//!    [`SlotState::found`].** A file called `BoingBag39-2.lha` is evidence
//!    about whoever named it, not about what is inside; a `filenames` entry
//!    is a hint recorded for a drop-folder guide, never a requirement. So a
//!    filename match goes into [`SlotState::candidates`] with `found = None`,
//!    which is what lets the readout say the two-fact sentence — *"a file by
//!    that name is here and ART could not confirm it"* — instead of the
//!    one-fact lie *"found"*.
//! 4. [`MatchedBy::Chosen`] — the user named this file by hand. Not a rank
//!    ART earned at all; it is recorded distinctly so the readout can say
//!    *chosen by you* rather than claiming an identification nobody made. The
//!    ROM arrives this way today, and Task 4's per-artefact override will.
//!
//! Two candidates at one rank are never resolved by list order:
//! [`super::scan::MediaMatch::Ambiguous`]'s own rule applies here unchanged —
//! both go into `candidates`, in sorted path order, and the user picks. Only
//! **byte-identical** duplicates collapse, exactly as
//! `scan::find_media_across` already folds a disk somebody keeps two copies
//! of.
//!
//! ## Installed is read from the tree's own account of itself, never inferred
//!
//! A file being in a folder says nothing about whether it was ever applied.
//! [`Installed`] comes from `distribution.json` alone — `amiga_installed` for
//! a package whose own installer ran on the Amiga, `built_from` for a medium
//! the tree was built out of, `files[].component` for a package ART placed
//! itself. A slot with a file beside it and nothing in the manifest is
//! [`Installed::No`], which is the truth.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::apply::DistributionManifest;
use super::mediahash::{self, Confirmation, MediaRow};
use super::package::{self, Package};
use super::recipe;
use super::scan::{FoundMedia, FoundPackage};
use super::{amiga_names_equal, Condition, Recipe};
use crate::core::error::{CoreError, CoreResult};

// ---------------------------------------------------------------------------
// The adopted table's artefact ids, which live outside Hatcher's data
// ---------------------------------------------------------------------------

const MEDIA_ARTEFACTS_ADOPTED_JSON: &str = include_str!("media_artefacts_adopted.json");

/// One adopted row's join to an ART artefact id — see
/// `media_artefacts_adopted.json`'s own `$comment` for why this is a separate
/// file rather than a field on the row.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AdoptedArtefact {
    /// As the adopted row states it, never re-derived.
    pub version: String,
    /// **Hatcher's own identifier** for the disk (`AmigaOS3_9BB1`) — not an
    /// AmigaDOS volume name, and never compared with one. See
    /// [`MediaRow::volume`].
    pub volume: String,
    /// ART's artefact id — the same string a slot is named by.
    pub artefact: String,
}

#[derive(Debug, Deserialize)]
struct AdoptedArtefactTable {
    artefacts: Vec<AdoptedArtefact>,
}

/// The parsed mapping, once per process.
///
/// A `Result` in the cell rather than a panic, for the reason `mediahash`'s
/// own tables are cached the same way: a shipped file this module cannot
/// parse must refuse the one question that needs it, not take the whole
/// application down at startup.
fn adopted_artefacts() -> CoreResult<&'static [AdoptedArtefact]> {
    static CELL: OnceLock<Result<Vec<AdoptedArtefact>, String>> = OnceLock::new();
    match CELL.get_or_init(|| {
        serde_json::from_str::<AdoptedArtefactTable>(MEDIA_ARTEFACTS_ADOPTED_JSON)
            .map(|table| table.artefacts)
            .map_err(|e| e.to_string())
    }) {
        Ok(rows) => Ok(rows),
        Err(detail) => Err(CoreError::Malformed {
            format: "adopted artefact map".into(),
            detail: detail.clone(),
        }),
    }
}

/// Which artefact this row's bytes are of, whichever table it came from.
///
/// An own-table row states it. An adopted row does not and never will — so
/// its id is looked up by the two fields it *does* state about itself,
/// `version` and `volume`. An adopted row nothing maps answers `None`, which
/// is the honest answer: ART has no name for those bytes.
pub fn artefact_of(row: &MediaRow) -> Option<String> {
    if let Some(id) = &row.artefact {
        return Some(id.clone());
    }
    adopted_artefacts()
        .ok()?
        .iter()
        .find(|entry| entry.version == row.version && entry.volume == row.volume)
        .map(|entry| entry.artefact.clone())
}

// ---------------------------------------------------------------------------
// Slot
// ---------------------------------------------------------------------------

/// What kind of artefact a slot wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SlotKind {
    /// A disk or disc a component reads from.
    Medium,
    /// An update package's own archive.
    Package,
    /// A second archive that patches a package before its installer runs —
    /// BoingBag 1's UAE fix.
    Overlay,
    /// The Kickstart the release states it needs.
    Rom,
}

/// One artefact a release's build can use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Slot {
    /// `medium:AmigaOS3.9`, `package:boingbag-39-1`,
    /// `overlay:boingbag-39-1:BoingBag3.9-1-UAE`, `rom`. Stable, and what
    /// [`Slot::requires`] names.
    pub id: String,
    pub kind: SlotKind,
    /// What the recipe calls this thing — a package's own name (ART-060: it
    /// is the package's, not ART's sentence about it), a medium's volume
    /// name, an overlay's own drawer. Never translated, and never composed
    /// into a sentence here.
    pub name: String,
    /// The name the artefact gives for **itself** and that rank 2 compares
    /// against: a volume name off a root block, an archive's single top-level
    /// directory.
    ///
    /// A [`SlotKind::Rom`] slot has no such name — a ROM image states a
    /// version, not an identity — so it carries the major the recipe states
    /// it needs (`"40"`), or the empty string when the recipe states no
    /// floor. That is the only thing the recipe says about the artefact, and
    /// it is what the readout has to render.
    pub identity: String,
    /// The artefact id this slot's rows are looked up by. `None` when neither
    /// table names these bytes yet, which simply means rank 1 cannot fire —
    /// never that the artefact is unknown or unwanted.
    pub artefact: Option<String>,
    /// A medium a **required** component reads from, or a ROM the release
    /// states a floor for. Everything else is optional, which is the truth:
    /// a package is a thing the user chooses.
    pub required: bool,
    /// Names ART has actually seen this artefact ship under, from the media
    /// rows for its artefact. A hint for the drop-folder guide and for rank
    /// 3's *guess*, never a requirement — see the module doc.
    pub filenames: Vec<String>,
    /// The row's own `source`, as the table states it. `None` when no row
    /// names this artefact.
    pub provenance: Option<String>,
    /// Chain order: media first, then the packages in
    /// [`package::order`](super::package::order)'s requires-respecting order
    /// with each package's overlays immediately after it, and the ROM last.
    pub position: u32,
    /// Slot ids that must be installed before this one — a package's own
    /// `requires`, and the CD its installer verifies before it will work.
    pub requires: Vec<String>,
    /// Reserved for round 3 (a newer artefact that makes this one
    /// unnecessary). Always empty today, and stated rather than omitted so
    /// the wire shape does not change under the screen when it fills.
    pub superseded_by: Vec<String>,
}

/// Every slot `release`'s build can use, in chain order.
pub fn slots_for(release: &str) -> CoreResult<Vec<Slot>> {
    let recipe = recipe::by_release(release)?;
    let packages = package::packages_for(release)?;
    slots_over(&recipe, &packages)
}

/// [`slots_for`]'s own derivation, parameterised over the recipe and the
/// package list — so a test can state a small recipe outright instead of
/// only ever exercising this through the three shipped releases. The same
/// split `package::order_over` and `recipe::merge_base` already make, and for
/// the same reason.
fn slots_over(recipe: &Recipe, packages: &[Package]) -> CoreResult<Vec<Slot>> {
    let rows = mediahash::rows()?;
    let mut slots: Vec<Slot> = Vec::new();
    let mut position: u32 = 0;

    // --- media, in the order the recipe first names them -------------------
    let mut media_names: Vec<&str> = Vec::new();
    for component in &recipe.components {
        if !media_names
            .iter()
            .any(|seen| amiga_names_equal(seen, &component.media))
        {
            media_names.push(&component.media);
        }
    }
    for media in media_names {
        let required = recipe
            .components
            .iter()
            .any(|c| c.required && amiga_names_equal(&c.media, media));
        let artefact = artefact_for_identity(rows, media);
        slots.push(Slot {
            id: format!("medium:{media}"),
            kind: SlotKind::Medium,
            name: media.to_string(),
            identity: media.to_string(),
            required,
            filenames: filenames_for(rows, artefact.as_deref()),
            provenance: provenance_for(rows, artefact.as_deref()),
            artefact,
            position: take(&mut position),
            requires: Vec::new(),
            superseded_by: Vec::new(),
        });
    }

    // --- packages, in the order they must be installed ---------------------
    //
    // `order_over` over the whole release's package list, not over a user's
    // selection: this is the chain the readout is sorted by, and it has to be
    // the same order `package::order` would give — a second sort here would
    // be a second answer to a question that already has one (`chain.rs`'s own
    // rule).
    let ids: Vec<String> = packages.iter().map(|p| p.id.clone()).collect();
    for id in package::order_over(&ids, packages)? {
        let Some(pkg) = packages.iter().find(|p| p.id == id) else {
            continue;
        };
        let mut requires: Vec<String> = pkg
            .requires
            .iter()
            .map(|need| format!("package:{need}"))
            .collect();
        if let Some(installer) = &pkg.amiga_installer {
            if let Some(medium) = &installer.required_medium {
                // Resolved against the medium slots already built rather than
                // formatted from the recipe's own spelling: the two strings
                // are AmigaDOS names from two different files, and a slot id
                // that names nothing would read as a permanent blocker.
                requires.push(medium_slot_id(&slots, &medium.volume));
            }
        }
        let artefact = Some(pkg.id.clone());
        slots.push(Slot {
            id: format!("package:{}", pkg.id),
            kind: SlotKind::Package,
            name: pkg.name.clone(),
            identity: pkg.media.clone(),
            required: false,
            filenames: filenames_for(rows, artefact.as_deref()),
            provenance: provenance_for(rows, artefact.as_deref()),
            artefact,
            position: take(&mut position),
            requires,
            superseded_by: Vec::new(),
        });

        // --- and its overlays, immediately after it ------------------------
        //
        // No `requires` on an overlay, deliberately: it patches the package
        // *before* the run, so "wait until the package is installed" would be
        // exactly backwards.
        let Some(installer) = &pkg.amiga_installer else {
            continue;
        };
        for overlay in &installer.overlays {
            let identity = overlay.from.split('/').next().unwrap_or("").to_string();
            let artefact = artefact_for_identity(rows, &identity);
            slots.push(Slot {
                id: format!("overlay:{}:{identity}", pkg.id),
                kind: SlotKind::Overlay,
                name: identity.clone(),
                identity,
                required: false,
                filenames: filenames_for(rows, artefact.as_deref()),
                provenance: provenance_for(rows, artefact.as_deref()),
                artefact,
                position: take(&mut position),
                requires: Vec::new(),
                superseded_by: Vec::new(),
            });
        }
    }

    // --- the ROM, last ----------------------------------------------------
    //
    // A slot exists when any component's own `Condition` asks about the
    // paired Kickstart at all; it is *required* only when a component that is
    // on regardless states a floor, which is AmigaOS 3.9's V40 (ART-157). A
    // `RomOlderThan` condition states no floor for the tree — it switches a
    // fallback component on — so it makes the ROM askable, not mandatory.
    let asks_about_rom = recipe.components.iter().any(|c| {
        matches!(c.condition, Some(Condition::RomOlderThan { .. }))
            || matches!(c.condition, Some(Condition::RomAtLeast { .. }))
    });
    if asks_about_rom {
        let floor = recipe
            .components
            .iter()
            .filter_map(|c| match &c.condition {
                Some(Condition::RomAtLeast { major }) if c.required => Some(*major),
                _ => None,
            })
            .max();
        slots.push(Slot {
            id: "rom".to_string(),
            kind: SlotKind::Rom,
            name: "Kickstart".to_string(),
            identity: floor.map(|major| major.to_string()).unwrap_or_default(),
            artefact: None,
            required: floor.is_some(),
            filenames: Vec::new(),
            provenance: None,
            position: take(&mut position),
            requires: Vec::new(),
            superseded_by: Vec::new(),
        });
    }

    Ok(slots)
}

fn take(position: &mut u32) -> u32 {
    let now = *position;
    *position += 1;
    now
}

/// The id of the medium slot that carries `volume`, or the plain form when no
/// slot does.
fn medium_slot_id(slots: &[Slot], volume: &str) -> String {
    slots
        .iter()
        .find(|slot| slot.kind == SlotKind::Medium && amiga_names_equal(&slot.identity, volume))
        .map(|slot| slot.id.clone())
        .unwrap_or_else(|| format!("medium:{volume}"))
}

/// Which artefact the rows say `identity` is — used for a medium and for an
/// overlay, whose identities are names a row's `volume` really does state.
///
/// **Only when the answer is unambiguous.** Two artefacts claiming one
/// identity (two archives sharing a top-level directory, which the owner's
/// own folder has eight of) answers `None`: naming one of them would be the
/// arbitrary winner this module refuses everywhere else. A package slot never
/// asks this — its artefact is its own id, which `package.rs` already
/// guarantees is unique.
fn artefact_for_identity(rows: &[MediaRow], identity: &str) -> Option<String> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for row in rows {
        if !amiga_names_equal(&row.volume, identity) {
            continue;
        }
        if let Some(artefact) = artefact_of(row) {
            seen.insert(artefact);
        }
    }
    match seen.len() {
        1 => seen.into_iter().next(),
        _ => None,
    }
}

/// Every filename the rows for `artefact` record, in table order, deduplicated.
fn filenames_for(rows: &[MediaRow], artefact: Option<&str>) -> Vec<String> {
    let Some(artefact) = artefact else {
        return Vec::new();
    };
    let mut names: Vec<String> = Vec::new();
    for row in rows_for_artefact(rows, artefact) {
        for name in &row.filenames {
            if !names.iter().any(|seen| seen.eq_ignore_ascii_case(name)) {
                names.push(name.clone());
            }
        }
    }
    names
}

/// The first row for `artefact`, as the tables are ordered (adopted, then
/// ART's own), and its `source` exactly as that table states it — never
/// re-worded here.
fn provenance_for(rows: &[MediaRow], artefact: Option<&str>) -> Option<String> {
    let artefact = artefact?;
    rows_for_artefact(rows, artefact)
        .next()
        .map(|row| row.source.clone())
}

fn rows_for_artefact<'a>(
    rows: &'a [MediaRow],
    artefact: &'a str,
) -> impl Iterator<Item = &'a MediaRow> {
    rows.iter()
        .filter(move |row| artefact_of(row).as_deref() == Some(artefact))
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

/// How ART came to believe a file fills a slot. Ranked — see the module doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MatchedBy {
    /// The bytes are in a row whose artefact is this slot's.
    Hash,
    /// A disk's own volume name, off its root block.
    VolumeName,
    /// An archive's own single top-level directory.
    TopLevelDirectory,
    /// The file's name alone — **a guess**, and never enough to fill
    /// [`SlotState::found`]. Kept as a variant because the panel's override
    /// and the guide text both need to name the rank that did *not* apply.
    Filename,
    /// The user named this file by hand. Not an identification ART made.
    Chosen,
}

/// A media row carried on a resolved slot.
///
/// An owned copy rather than a borrow: a [`SlotState`] crosses the wire, and
/// the readout needs the row's own `name`, `version` and `source` to say what
/// the match was against without looking it up a second time.
pub type MediaRowRef = MediaRow;

/// The confirmation of that row, when one has been checked against a real
/// disk here. `None` beside a `Some(row)` is "matched a row nobody here has
/// checked", which is a weaker sentence than a confirmed match and a
/// different one from "not in the table" — see [`mediahash::MediaMatch`].
pub type ConfirmationRef = Confirmation;

/// The file that fills a slot, and how ART knows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub path: PathBuf,
    pub matched_by: MatchedBy,
    /// The table row, and only for [`MatchedBy::Hash`]: a row is the evidence
    /// rank 1 *is*. A file matched by the name it gives for itself may well
    /// be in the table under some other artefact, and carrying that row here
    /// would present one artefact's provenance as another's.
    pub row: Option<MediaRowRef>,
    pub confirmed: Option<ConfirmationRef>,
}

/// Whether this slot's artefact is already part of the tree, **as the tree's
/// own `distribution.json` states it**.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum Installed {
    /// ART placed it from the host. `at` names one file it wrote, when the
    /// manifest records files for it — a medium contributes no single path,
    /// so it answers `None`.
    Placed { at: Option<String> },
    /// The package's **own** installer ran on the Amiga and reported success,
    /// with this command line. Deliberately distinct from `Placed`: ART did
    /// not write those files and cannot account for them per file.
    Ran { command: String },
    /// The manifest says nothing about it. **A file being present is never
    /// "installed"** — that is the whole reason this comes from the manifest.
    No,
}

/// One slot, resolved against the facts the caller gathered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotState {
    pub slot: Slot,
    /// The one file ART is prepared to say fills this slot. `None` whenever
    /// ART could not decide — nothing matched, several did, or the only
    /// evidence was a file name.
    pub found: Option<Found>,
    /// Everything that might fill it, in sorted path order: the several
    /// claimants of an ambiguous identity, or the filename guesses. Empty
    /// when `found` is `Some` — a decided slot has nothing left to choose
    /// between.
    pub candidates: Vec<PathBuf>,
    pub installed: Installed,
    /// Every [`Slot::requires`] entry that is not installed yet, in the order
    /// the slot states them. Empty is "nothing is in the way".
    pub blocked_by: Vec<String>,
    /// Why this slot does not need filling, when something ART measured says
    /// so — the artefact's own statement about itself, e.g. `Updater 45.15`
    /// for a BoingBag 1 copy that already carries the fixed program, so the
    /// UAE overlay is unnecessary.
    ///
    /// **A measurement, not a sentence.** The words around it belong in the
    /// catalogue; core states what it read.
    pub not_needed: Option<String>,
}

/// Everything the caller learned about the user's folders, gathered once.
///
/// Borrowed rather than owned because the caller already holds all of it:
/// nothing here is re-read, re-hashed or re-opened by [`resolve`].
pub struct Facts<'a> {
    /// [`super::scan::find_media`]'s answers across every material folder.
    pub media: &'a [FoundMedia],
    /// [`super::scan::find_packages`]'s answers across the same folders.
    pub packages: &'a [FoundPackage],
    /// What [`mediahash`] already knows about those folders' candidates.
    pub hashes: &'a [mediahash::MediaMatch],
    /// The chosen distribution tree's own account of itself, when one is
    /// chosen. The **only** source of [`Installed`].
    pub manifest: Option<&'a DistributionManifest>,
    /// The Kickstart the user chose by hand.
    pub rom: Option<&'a Path>,
    /// What each Amiga-installable package's own wrapper archive says its
    /// installer program is, as `(package id, "45.15")` — read by the caller
    /// from the archive itself (`packagevol::stated_version`), because a
    /// version is a fact about a file and this module opens none.
    pub program_versions: &'a [(String, String)],
}

/// Resolve every slot against `facts`.
///
/// Two passes, and the order is load-bearing: [`SlotState::blocked_by`] is
/// about *other* slots' [`Installed`] state, so nothing can be said about it
/// until every slot has one.
pub fn resolve(slots: &[Slot], facts: &Facts<'_>) -> Vec<SlotState> {
    let mut states: Vec<SlotState> = slots.iter().map(|slot| resolve_one(slot, facts)).collect();

    let installed: Vec<(String, bool)> = states
        .iter()
        .map(|state| (state.slot.id.clone(), state.installed != Installed::No))
        .collect();
    for state in &mut states {
        state.blocked_by = state
            .slot
            .requires
            .iter()
            .filter(|need| {
                // A requirement naming a slot this release has none of is
                // treated as not installed rather than ignored: silently
                // dropping it would turn a data mistake into a run that
                // looks ready.
                !installed.iter().any(|(id, done)| id == *need && *done)
            })
            .cloned()
            .collect();
    }
    states
}

fn resolve_one(slot: &Slot, facts: &Facts<'_>) -> SlotState {
    let installed = installed_state(slot, facts.manifest);
    let not_needed = not_needed_for(slot, facts);
    let state = |found, candidates| SlotState {
        slot: slot.clone(),
        found,
        candidates,
        installed: installed.clone(),
        blocked_by: Vec::new(),
        not_needed: not_needed.clone(),
    };

    if slot.kind == SlotKind::Rom {
        // Never `Filename`: the user handed ART this path, which is a
        // different thing from ART recognising a name, and collapsing the two
        // would let a hand-picked ROM read as a guess.
        let found = facts.rom.map(|path| Found {
            path: path.to_path_buf(),
            matched_by: MatchedBy::Chosen,
            row: None,
            confirmed: None,
        });
        return state(found, Vec::new());
    }

    // --- rank 1: the bytes -------------------------------------------------
    let mut by_hash: Vec<&mediahash::MediaMatch> = Vec::new();
    if let Some(artefact) = &slot.artefact {
        for entry in facts.hashes {
            let Some(row) = &entry.row else { continue };
            if artefact_of(row).as_deref() == Some(artefact.as_str()) {
                by_hash.push(entry);
            }
        }
    }
    by_hash.sort_by(|a, b| a.path.cmp(&b.path));
    by_hash.dedup_by(|a, b| a.md5 == b.md5);
    if by_hash.len() == 1 {
        let entry = by_hash[0];
        return state(
            Some(Found {
                path: entry.path.clone(),
                matched_by: MatchedBy::Hash,
                row: entry.row.clone(),
                confirmed: entry.confirmed.clone(),
            }),
            Vec::new(),
        );
    }
    if by_hash.len() > 1 {
        return state(None, by_hash.iter().map(|e| e.path.clone()).collect());
    }

    // --- rank 2: what the artefact says it is ------------------------------
    let (mut by_identity, matched_by) = match slot.kind {
        SlotKind::Medium => (
            facts
                .media
                .iter()
                .filter(|found| amiga_names_equal(&found.volume_name, &slot.identity))
                .map(|found| found.path.clone())
                .collect::<Vec<PathBuf>>(),
            MatchedBy::VolumeName,
        ),
        SlotKind::Package | SlotKind::Overlay => (
            facts
                .packages
                .iter()
                .filter(|found| amiga_names_equal(&found.media, &slot.identity))
                .map(|found| found.path.clone())
                .collect::<Vec<PathBuf>>(),
            MatchedBy::TopLevelDirectory,
        ),
        SlotKind::Rom => (Vec::new(), MatchedBy::Chosen),
    };
    by_identity.sort();
    by_identity.dedup();
    let by_identity = collapse_identical(by_identity, facts.hashes);
    if by_identity.len() == 1 {
        return state(
            Some(Found {
                path: by_identity[0].clone(),
                matched_by,
                row: None,
                confirmed: None,
            }),
            Vec::new(),
        );
    }
    if by_identity.len() > 1 {
        return state(None, by_identity);
    }

    // --- rank 3: the name somebody gave the file — a guess, never a find ---
    let mut by_name: Vec<PathBuf> = every_path(facts)
        .into_iter()
        .filter(|path| {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                return false;
            };
            slot.filenames
                .iter()
                .any(|wanted| wanted.eq_ignore_ascii_case(name))
        })
        .collect();
    by_name.sort();
    by_name.dedup();
    state(None, collapse_identical(by_name, facts.hashes))
}

/// Every path the caller looked at, however it was looked at.
fn every_path(facts: &Facts<'_>) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = facts.media.iter().map(|f| f.path.clone()).collect();
    paths.extend(facts.packages.iter().map(|f| f.path.clone()));
    paths.extend(facts.hashes.iter().map(|f| f.path.clone()));
    paths.sort();
    paths.dedup();
    paths
}

/// Drop a path whose bytes another kept path already is.
///
/// `scan::find_media_across`'s rule, reached from the other side: a user who
/// keeps the same disk in two folders has one disk, and telling them it is
/// ambiguous with itself is a refusal with no decision behind it. Only files
/// the caller already has a hash for can collapse — this module reads
/// nothing, so an unhashed pair stays two candidates, which is the honest
/// answer for two files nobody has compared.
fn collapse_identical(paths: Vec<PathBuf>, hashes: &[mediahash::MediaMatch]) -> Vec<PathBuf> {
    let md5_of = |path: &Path| {
        hashes
            .iter()
            .find(|entry| entry.path == path)
            .map(|entry| entry.md5.clone())
    };
    let mut kept: Vec<PathBuf> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for path in paths {
        match md5_of(&path) {
            Some(md5) if seen.contains(&md5) => continue,
            Some(md5) => seen.push(md5),
            None => {}
        }
        kept.push(path);
    }
    kept
}

fn installed_state(slot: &Slot, manifest: Option<&DistributionManifest>) -> Installed {
    let Some(manifest) = manifest else {
        return Installed::No;
    };
    match slot.kind {
        SlotKind::Package => {
            let id = slot.id.strip_prefix("package:").unwrap_or(&slot.id);
            if let Some(record) = manifest
                .amiga_installed
                .iter()
                .find(|record| record.package == id)
            {
                return Installed::Ran {
                    command: record.command.clone(),
                };
            }
            match manifest.files.iter().find(|file| file.component == id) {
                Some(file) => Installed::Placed {
                    at: Some(file.path.clone()),
                },
                None => Installed::No,
            }
        }
        SlotKind::Medium => {
            if manifest
                .built_from
                .iter()
                .any(|record| amiga_names_equal(&record.volume_name, &slot.identity))
            {
                Installed::Placed { at: None }
            } else {
                Installed::No
            }
        }
        // An overlay leaves no record of its own — `unpack` reports it and
        // the package's `Ran` record is what survives — and a ROM is not part
        // of the tree at all. Saying `Placed` for either would be a claim
        // nothing backs.
        SlotKind::Overlay | SlotKind::Rom => Installed::No,
    }
}

/// Whether an overlay slot is unnecessary because the package's own copy
/// already carries a program at or above the version its recipe demands.
///
/// The measurement, not the sentence: `Some("Updater 45.15")`. ART-186's
/// whole point is that the archive's own `$VER:` decides this, never the
/// user and never a file size — so `None` here covers both "the copy is
/// older" and "nobody has read it yet", and the readout must not render the
/// second as the first.
fn not_needed_for(slot: &Slot, facts: &Facts<'_>) -> Option<String> {
    if slot.kind != SlotKind::Overlay {
        return None;
    }
    let package_id = slot.id.strip_prefix("overlay:")?.split_once(':')?.0;
    let package = package::by_id(package_id).ok()?;
    let installer = package.amiga_installer.as_ref()?;
    let minimum = package::parse_version_pair(installer.minimum_version.as_deref()?)?;
    let (_, stated) = facts
        .program_versions
        .iter()
        .find(|(id, _)| id == package_id)?;
    if package::parse_version_pair(stated)? < minimum {
        return None;
    }
    let program = installer
        .program
        .rsplit('/')
        .next()
        .unwrap_or(&installer.program);
    Some(format!("{program} {stated}"))
}

// ---------------------------------------------------------------------------
// Summary
// ---------------------------------------------------------------------------

/// The one line above the readout: how much of this set is here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSummary {
    pub release: String,
    pub required_total: u32,
    pub required_found: u32,
    pub optional_total: u32,
    pub optional_found: u32,
}

/// Count `states`, required and optional apart.
///
/// **Apart, the way HstWB counts them**, and not as one fraction: a set
/// missing only optional files is ready to build, and one fraction cannot say
/// that. "Found" is `found.is_some() || installed != No` — a slot ART already
/// placed is not missing merely because the archive it came from has since
/// been moved.
///
/// A slot ART has measured as **not needed** is left out of both totals
/// entirely. It is neither found nor missing; counting it as missing would
/// put "1 optional missing" on screen about an artefact ART has just proved
/// nobody has to obtain, which is this project's own named failure — a
/// confident, wrong sentence that passes every test.
///
/// `release` is a parameter rather than a field read off the states: a
/// [`Slot`] does not carry the release it came from (its id is the same
/// whichever release asked for it), so deriving it here would mean inventing
/// it. The caller has it in hand.
pub fn summarize(release: &str, states: &[SlotState]) -> SetSummary {
    let mut summary = SetSummary {
        release: release.to_string(),
        required_total: 0,
        required_found: 0,
        optional_total: 0,
        optional_found: 0,
    };
    for state in states {
        if state.not_needed.is_some() {
            continue;
        }
        let found = state.found.is_some() || state.installed != Installed::No;
        if state.slot.required {
            summary.required_total += 1;
            summary.required_found += u32::from(found);
        } else {
            summary.optional_total += 1;
            summary.optional_found += u32::from(found);
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::osinstall::apply::{AmigaInstallRecord, FileRecord, MediaRecord};
    use crate::core::osinstall::scan::MediaKind;

    // -----------------------------------------------------------------
    // Deriving the list
    // -----------------------------------------------------------------

    #[test]
    fn the_shipped_release_yields_the_cd_every_package_the_overlay_and_the_rom_in_order() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        // The ids, not a count: a count passes while the list holds the
        // wrong things, and the *order* is the chain this readout is sorted
        // by (media, then packages requires-first with each overlay right
        // after its package, then the ROM).
        assert_eq!(
            slots.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            vec![
                "medium:AmigaOS3.9",
                "package:boingbag-39-1",
                "overlay:boingbag-39-1:BoingBag3.9-1-UAE",
                "package:locale-turkish",
                "package:locale-39",
                "package:locale-39-turkish",
                "package:boingbag-39-2",
                "rom",
            ]
        );
        assert_eq!(
            slots.iter().map(|s| s.position).collect::<Vec<_>>(),
            (0..8).collect::<Vec<u32>>()
        );
        // BoingBag 2 after BoingBag 1, which is the whole reason `order_over`
        // is asked rather than the shipped list being taken as written.
        let bb1 = slots.iter().position(|s| s.id == "package:boingbag-39-1");
        let bb2 = slots.iter().position(|s| s.id == "package:boingbag-39-2");
        assert!(bb1 < bb2, "BoingBag 1 must come before BoingBag 2");
    }

    #[test]
    fn a_medium_a_required_component_reads_from_is_required_and_the_packages_are_not() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let cd = slots.iter().find(|s| s.id == "medium:AmigaOS3.9").unwrap();
        assert!(
            cd.required,
            "3.9's workbench-base is required and reads the CD"
        );
        assert_eq!(cd.kind, SlotKind::Medium);
        assert_eq!(cd.identity, "AmigaOS3.9");
        for slot in slots.iter().filter(|s| s.kind == SlotKind::Package) {
            assert!(!slot.required, "{} is a choice, not a requirement", slot.id);
        }
    }

    #[test]
    fn the_rom_slot_carries_the_floor_the_recipe_states() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let rom = slots.iter().find(|s| s.id == "rom").unwrap();
        assert!(rom.required, "3.9's own required component states V40");
        assert_eq!(rom.identity, "40");
    }

    #[test]
    fn a_package_slot_names_the_cd_its_installer_verifies_as_a_requirement() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let bb1 = slots
            .iter()
            .find(|s| s.id == "package:boingbag-39-1")
            .unwrap();
        assert!(
            bb1.requires.contains(&"medium:AmigaOS3.9".to_string()),
            "BoingBag 1's Updater checks the CD before it does anything: {:?}",
            bb1.requires
        );
        let bb2 = slots
            .iter()
            .find(|s| s.id == "package:boingbag-39-2")
            .unwrap();
        assert!(bb2.requires.contains(&"package:boingbag-39-1".to_string()));
        // An overlay patches its package before the run, so it must not wait
        // for the package to be installed.
        let overlay = slots.iter().find(|s| s.kind == SlotKind::Overlay).unwrap();
        assert!(overlay.requires.is_empty(), "{:?}", overlay.requires);
    }

    #[test]
    fn a_slots_filenames_and_provenance_come_from_the_rows_for_its_artefact() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let cd = slots.iter().find(|s| s.id == "medium:AmigaOS3.9").unwrap();
        assert_eq!(cd.artefact.as_deref(), Some("amigaos-39-cd"));
        assert!(
            cd.filenames.iter().any(|n| n == "AmigaOS39.iso"),
            "{:?}",
            cd.filenames
        );
        // As the table states it, never re-worded here.
        assert_eq!(cd.provenance.as_deref(), Some("Haage and Partners (3.9)"));
        let overlay = slots
            .iter()
            .find(|s| s.id == "overlay:boingbag-39-1:BoingBag3.9-1-UAE")
            .unwrap();
        assert_eq!(overlay.artefact.as_deref(), Some("boingbag-39-1-uae-fix"));
        assert_eq!(overlay.filenames, vec!["BoingBag39-1-UAE.lha".to_string()]);
    }

    #[test]
    fn every_adopted_3_9_row_maps_to_an_artefact() {
        // The mapping is data beside the table, so it can go stale the moment
        // the adopted table gains a row. Asked of the real file rather than of
        // a list here — a test that reads its own copy is a copy, and copies
        // drift.
        let unmapped: Vec<&MediaRow> = mediahash::rows()
            .unwrap()
            .iter()
            .filter(|row| row.version == "3.9" && row.artefact.is_none())
            .filter(|row| artefact_of(row).is_none())
            .collect();
        assert!(
            unmapped.is_empty(),
            "adopted 3.9 rows with no artefact id: {:?}",
            unmapped
                .iter()
                .map(|r| (&r.volume, &r.md5))
                .collect::<Vec<_>>()
        );
    }

    // -----------------------------------------------------------------
    // Resolution
    // -----------------------------------------------------------------

    fn medium(path: &str, volume: &str) -> FoundMedia {
        FoundMedia {
            path: PathBuf::from(path),
            volume_name: volume.to_string(),
            kind: MediaKind::Disc,
            layer: None,
        }
    }

    fn archive(path: &str, top_level: &str) -> FoundPackage {
        FoundPackage {
            path: PathBuf::from(path),
            media: top_level.to_string(),
            refused_names: Vec::new(),
        }
    }

    /// A hash answer for `path`, carrying whichever real table row `md5`
    /// names. The md5s below are the shipped tables' own, so a row that moves
    /// tables (or loses its artefact) fails these tests rather than being
    /// silently re-stated here.
    fn hashed(path: &str, md5: &str) -> mediahash::MediaMatch {
        mediahash::MediaMatch {
            path: PathBuf::from(path),
            volume_name: None,
            row: mediahash::row_for(md5).unwrap().cloned(),
            md5: md5.to_string(),
            confirmed: mediahash::confirmation_for(md5).unwrap().cloned(),
        }
    }

    const CD_OWN_MD5: &str = "e32a107e68edfc9b28a2fe075e32e5f6";
    const CD_ADOPTED_MD5: &str = "3cb96e77d922a4f8eb696e525a240448";
    const BB1_45_15_MD5: &str = "ef67ce2f786044dae1bce8fafb439d5e";
    const UAE_FIX_MD5: &str = "e00a91cd6800d6a34821a156c44b1c2f";

    struct Gathered {
        media: Vec<FoundMedia>,
        packages: Vec<FoundPackage>,
        hashes: Vec<mediahash::MediaMatch>,
        rom: Option<PathBuf>,
        program_versions: Vec<(String, String)>,
    }

    impl Gathered {
        fn empty() -> Self {
            Gathered {
                media: Vec::new(),
                packages: Vec::new(),
                hashes: Vec::new(),
                rom: None,
                program_versions: Vec::new(),
            }
        }

        fn facts<'a>(&'a self, manifest: Option<&'a DistributionManifest>) -> Facts<'a> {
            Facts {
                media: &self.media,
                packages: &self.packages,
                hashes: &self.hashes,
                manifest,
                rom: self.rom.as_deref(),
                program_versions: &self.program_versions,
            }
        }
    }

    fn state_of<'a>(states: &'a [SlotState], id: &str) -> &'a SlotState {
        states
            .iter()
            .find(|state| state.slot.id == id)
            .unwrap_or_else(|| panic!("no slot {id}"))
    }

    #[test]
    fn a_hash_row_wins_over_the_name_the_medium_gives_itself() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let mut gathered = Gathered::empty();
        // Two different files: one whose bytes are the CD's, one that merely
        // calls itself `AmigaOS3.9`. The bytes win, and the loser is not left
        // behind as a candidate either — the slot is decided.
        gathered.media.push(medium("D:/a/right.iso", "AmigaOS3.9"));
        gathered.media.push(medium("D:/a/other.iso", "AmigaOS3.9"));
        gathered.hashes.push(hashed("D:/a/right.iso", CD_OWN_MD5));

        let states = resolve(&slots, &gathered.facts(None));
        let cd = state_of(&states, "medium:AmigaOS3.9");
        let found = cd.found.as_ref().expect("the bytes decided it");
        assert_eq!(found.matched_by, MatchedBy::Hash);
        assert_eq!(found.path, PathBuf::from("D:/a/right.iso"));
        assert!(found.row.is_some(), "rank 1 carries its row");
        assert!(cd.candidates.is_empty());
    }

    #[test]
    fn an_adopted_rows_hash_fills_the_same_slot_as_arts_own() {
        // The four adopted AmigaOS 3.9 masterings carry no `artefact` of
        // their own; `media_artefacts_adopted.json` is what joins them, and
        // without it this file would resolve to nothing at all.
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let mut gathered = Gathered::empty();
        gathered
            .hashes
            .push(hashed("D:/a/AmigaOS39.iso", CD_ADOPTED_MD5));

        let states = resolve(&slots, &gathered.facts(None));
        let found = state_of(&states, "medium:AmigaOS3.9")
            .found
            .as_ref()
            .expect("an adopted row names this artefact through the map");
        assert_eq!(found.matched_by, MatchedBy::Hash);
    }

    #[test]
    fn an_archive_that_calls_itself_the_package_fills_the_slot_when_no_hash_does() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let mut gathered = Gathered::empty();
        gathered
            .packages
            .push(archive("D:/a/renamed.lha", "BoingBag3.9-1"));

        let states = resolve(&slots, &gathered.facts(None));
        let found = state_of(&states, "package:boingbag-39-1")
            .found
            .as_ref()
            .expect("the archive's own top-level directory is enough");
        assert_eq!(found.matched_by, MatchedBy::TopLevelDirectory);
        assert_eq!(found.path, PathBuf::from("D:/a/renamed.lha"));
        // Rank 2 carries no row: the bytes were never the evidence, and
        // attaching a row here would present one artefact's provenance as
        // another's.
        assert!(found.row.is_none());
    }

    #[test]
    fn a_filename_match_alone_never_fills_found() {
        // The mutation this test exists for: make rank 3 fill `found` and
        // this fails. A file called `BoingBag39-1-UAE.lha` is evidence about
        // whoever named it, and the readout has to be able to say ART could
        // not confirm it.
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let mut gathered = Gathered::empty();
        gathered
            .packages
            .push(archive("D:/a/BoingBag39-1-UAE.lha", "SomethingElse"));

        let states = resolve(&slots, &gathered.facts(None));
        let overlay = state_of(&states, "overlay:boingbag-39-1:BoingBag3.9-1-UAE");
        assert!(
            overlay.found.is_none(),
            "a name is a guess: {:?}",
            overlay.found
        );
        assert_eq!(
            overlay.candidates,
            vec![PathBuf::from("D:/a/BoingBag39-1-UAE.lha")]
        );
    }

    #[test]
    fn two_claimants_of_one_identity_leave_found_empty_and_list_both_in_path_order() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let mut gathered = Gathered::empty();
        // Deliberately added in the wrong order: the answer is sorted, never
        // the order the folders happened to be scanned in.
        gathered
            .packages
            .push(archive("D:/b/second.lha", "BoingBag3.9-1"));
        gathered
            .packages
            .push(archive("D:/a/first.lha", "BoingBag3.9-1"));

        let states = resolve(&slots, &gathered.facts(None));
        let bb1 = state_of(&states, "package:boingbag-39-1");
        assert!(bb1.found.is_none(), "ART does not pick between two");
        assert_eq!(
            bb1.candidates,
            vec![
                PathBuf::from("D:/a/first.lha"),
                PathBuf::from("D:/b/second.lha")
            ]
        );
    }

    #[test]
    fn two_byte_identical_copies_of_one_archive_are_one_candidate() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let mut gathered = Gathered::empty();
        gathered
            .packages
            .push(archive("D:/a/BoingBag39-1.lha", "BoingBag3.9-1"));
        gathered
            .packages
            .push(archive("D:/b/backup.lha", "BoingBag3.9-1"));
        // The same bytes in two folders — one archive, not an ambiguity.
        gathered
            .hashes
            .push(hashed("D:/a/BoingBag39-1.lha", BB1_45_15_MD5));
        gathered
            .hashes
            .push(hashed("D:/b/backup.lha", BB1_45_15_MD5));

        let states = resolve(&slots, &gathered.facts(None));
        let bb1 = state_of(&states, "package:boingbag-39-1");
        let found = bb1.found.as_ref().expect("one archive, kept twice");
        assert_eq!(found.matched_by, MatchedBy::Hash);
        assert_eq!(found.path, PathBuf::from("D:/a/BoingBag39-1.lha"));
    }

    fn manifest_with(
        built_from: Vec<&str>,
        components: Vec<&str>,
        ran: Vec<&str>,
    ) -> DistributionManifest {
        DistributionManifest {
            release: "AmigaOS 3.9".to_string(),
            built_from: built_from
                .into_iter()
                .map(|volume| MediaRecord {
                    volume_name: volume.to_string(),
                    sha256: "0".repeat(64),
                })
                .collect(),
            files: components
                .into_iter()
                .map(|component| FileRecord {
                    path: format!("C/{component}"),
                    component: component.to_string(),
                    media: String::new(),
                    sha256: "0".repeat(64),
                    bytes: 1,
                    protection: None,
                    overwrote: None,
                    host_path: None,
                })
                .collect(),
            paired_rom: None,
            amiga_installed: ran
                .into_iter()
                .map(|package| AmigaInstallRecord {
                    package: package.to_string(),
                    command: format!("PKG:C/Updater AmigaOS-Update {package}"),
                })
                .collect(),
            layers: Vec::new(),
        }
    }

    #[test]
    fn installed_is_read_from_the_manifest_and_never_from_a_file_being_present() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let mut gathered = Gathered::empty();
        // The archive is right here, and nothing has been installed.
        gathered
            .packages
            .push(archive("D:/a/BoingBag39-1.lha", "BoingBag3.9-1"));

        let none = manifest_with(vec![], vec![], vec![]);
        let states = resolve(&slots, &gathered.facts(Some(&none)));
        assert_eq!(
            state_of(&states, "package:boingbag-39-1").installed,
            Installed::No,
            "a file in a folder is not an install"
        );

        let done = manifest_with(vec!["AmigaOS3.9"], vec!["locale-39"], vec!["boingbag-39-1"]);
        let states = resolve(&slots, &gathered.facts(Some(&done)));
        assert_eq!(
            state_of(&states, "package:boingbag-39-1").installed,
            Installed::Ran {
                command: "PKG:C/Updater AmigaOS-Update boingbag-39-1".to_string()
            }
        );
        assert_eq!(
            state_of(&states, "package:locale-39").installed,
            Installed::Placed {
                at: Some("C/locale-39".to_string())
            }
        );
        assert_eq!(
            state_of(&states, "medium:AmigaOS3.9").installed,
            Installed::Placed { at: None }
        );
    }

    #[test]
    fn blocked_by_names_every_requirement_that_is_not_installed_yet() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let gathered = Gathered::empty();

        let nothing = manifest_with(vec![], vec![], vec![]);
        let states = resolve(&slots, &gathered.facts(Some(&nothing)));
        assert_eq!(
            state_of(&states, "package:boingbag-39-2").blocked_by,
            vec![
                "package:boingbag-39-1".to_string(),
                "medium:AmigaOS3.9".to_string()
            ]
        );

        let one_done = manifest_with(vec!["AmigaOS3.9"], vec![], vec!["boingbag-39-1"]);
        let states = resolve(&slots, &gathered.facts(Some(&one_done)));
        assert!(
            state_of(&states, "package:boingbag-39-2")
                .blocked_by
                .is_empty(),
            "{:?}",
            state_of(&states, "package:boingbag-39-2").blocked_by
        );
    }

    #[test]
    fn an_overlay_is_not_needed_when_the_package_already_carries_a_new_enough_program() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let mut gathered = Gathered::empty();
        gathered
            .program_versions
            .push(("boingbag-39-1".to_string(), "45.15".to_string()));
        let states = resolve(&slots, &gathered.facts(None));
        assert_eq!(
            state_of(&states, "overlay:boingbag-39-1:BoingBag3.9-1-UAE").not_needed,
            Some("Updater 45.15".to_string())
        );

        // The pre-fix build: the overlay is exactly as needed as it ever was,
        // and "nobody has read it yet" answers the same way as "it is older",
        // which is why neither may be rendered as the other.
        let mut older = Gathered::empty();
        older
            .program_versions
            .push(("boingbag-39-1".to_string(), "45.13".to_string()));
        let states = resolve(&slots, &older.facts(None));
        assert_eq!(
            state_of(&states, "overlay:boingbag-39-1:BoingBag3.9-1-UAE").not_needed,
            None
        );
        let states = resolve(&slots, &Gathered::empty().facts(None));
        assert_eq!(
            state_of(&states, "overlay:boingbag-39-1:BoingBag3.9-1-UAE").not_needed,
            None
        );
    }

    #[test]
    fn a_rom_the_user_chose_is_matched_by_chosen_and_never_by_filename() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let mut gathered = Gathered::empty();
        gathered.rom = Some(PathBuf::from("D:/roms/kick40068.A1200.rom"));
        let states = resolve(&slots, &gathered.facts(None));
        let found = state_of(&states, "rom").found.as_ref().unwrap();
        assert_eq!(found.matched_by, MatchedBy::Chosen);
        assert_eq!(found.path, PathBuf::from("D:/roms/kick40068.A1200.rom"));

        let states = resolve(&slots, &Gathered::empty().facts(None));
        assert!(state_of(&states, "rom").found.is_none());
    }

    #[test]
    fn the_summary_counts_required_and_optional_apart_and_skips_what_is_not_needed() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let mut gathered = Gathered::empty();
        gathered.hashes.push(hashed("D:/a/cd.iso", CD_OWN_MD5));
        gathered.hashes.push(hashed("D:/a/bb1.lha", BB1_45_15_MD5));
        gathered.hashes.push(hashed("D:/a/uae.lha", UAE_FIX_MD5));
        gathered
            .program_versions
            .push(("boingbag-39-1".to_string(), "45.15".to_string()));

        let states = resolve(&slots, &gathered.facts(None));
        let summary = summarize("AmigaOS 3.9", &states);
        assert_eq!(summary.release, "AmigaOS 3.9");
        // The CD is found; the ROM is required and nobody chose one.
        assert_eq!((summary.required_total, summary.required_found), (2, 1));
        // Five packages, one of them found — and the UAE overlay, which is
        // present but measured as unnecessary, is in neither total.
        assert_eq!((summary.optional_total, summary.optional_found), (5, 1));
    }

    #[test]
    fn a_slot_already_installed_counts_as_found_even_with_the_archive_gone() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let gathered = Gathered::empty();
        let done = manifest_with(vec!["AmigaOS3.9"], vec![], vec![]);
        let states = resolve(&slots, &gathered.facts(Some(&done)));
        let summary = summarize("AmigaOS 3.9", &states);
        assert_eq!(summary.required_found, 1);
    }
}
