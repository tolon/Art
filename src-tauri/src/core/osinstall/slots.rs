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
//! ## Rank 1 falling silent has more than one cause, and the screen says which
//!
//! **The most common reason no row answers is that nobody has read the bytes**
//! (fix round 1, F1). `osinstall_slots` asks the scan cache and hashes
//! nothing — a 490 MB ISO belongs on the job that already exists, not on a
//! command thread — so until `osinstall_identify_media` has run over a folder,
//! every file in it lands at rank 2 with nothing known about its bytes at all.
//! *"Its bytes are in no table ART has"* would then be a statement about a
//! lookup nobody made, which is exactly what
//! [`mediahash::remembered_media_in`]'s own doc comment forbids its callers to
//! say. So [`Found::bytes_read`] and [`Candidate::bytes_read`] carry the
//! difference to the screen, and the readout has three sentences where it used
//! to have one: **nobody looked**, **looked and no row claims these bytes**,
//! and **looked, and a row claims them for a different artefact** — the last
//! being a relabelled disk, where "in no table ART has" is simply false. See
//! [`BytesRead`].
//!
//! ## What rank 3 can see, and what it cannot (F8, parked by ruling)
//!
//! [`Facts`] carries what three *readers* accepted — disks `find_media`
//! opened, archives `find_packages` opened, files somebody hashed — and not a
//! raw directory listing. So the guess rank only ever fires for a file at
//! least one reader took, which in practice means **a readable archive or disk
//! whose bytes the table does not know**. A file that no reader would accept
//! at all (a truncated download, an archive of a shape `ArchiveSource`
//! refuses) is not a candidate here and the slot reads as not found.
//!
//! That is narrower than the design's § 3.2 wording, and it is parked rather
//! than fixed: closing it means the caller passing the plain per-folder
//! listing as a fourth fact, which is a fact about a folder rather than about
//! an artefact and belongs with the readout that would show it. Recorded here
//! so the gap is a known one rather than a surprise.
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
    /// Names ART has seen this artefact ship under. **ART's own claim, not
    /// the adopted table's** — Hatcher's rows carry no such field, so this is
    /// the only place an adopted-only artefact can get one (fix round 1, F4).
    /// A hint, never a requirement.
    #[serde(default)]
    pub filenames: Vec<String>,
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
        let artefact = artefact_for_package(rows, pkg);
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

/// Which artefact the rows say `identity` is — a medium's or an overlay's
/// name for itself, and a package's `media` when no row carries the package's
/// own id.
///
/// **Only ART's own rows are asked, and that is a rule rather than an
/// optimisation** (fix round 1, F12). `identity` here is an AmigaDOS name — a
/// volume label off a root block, an archive's single top-level directory —
/// and [`MediaRow::volume`] is **not** one for an adopted row: it is Hatcher's
/// own identifier for the disk, and that field's own doc comment carries the
/// measurement (0 of the owner's 12 AmigaOS 3.2 disks matched it). Comparing
/// across the two namespaces is harmless only for as long as no adopted row
/// happens to spell an identity the way AmigaDOS does; it is a false positive
/// waiting for the map to grow, so the comparison is not made at all.
/// [`MediaRow::table_origin`] is what separates them.
///
/// **Only when the answer is unambiguous.** Two artefacts claiming one
/// identity (two archives sharing a top-level directory, which the owner's
/// own folder has eight of) answers `None`: naming one of them would be the
/// arbitrary winner this module refuses everywhere else.
fn artefact_for_identity(rows: &[MediaRow], identity: &str) -> Option<String> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for row in rows {
        if row.table_origin != mediahash::Origin::Own {
            continue;
        }
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

/// The artefact a package slot's rows are looked up by.
///
/// Its own id when a row names it — the ordinary case, and the one that keeps
/// two packages reading two different archives apart. Otherwise the artefact
/// of whatever row names the archive the package reads, because
/// **`MediaRow::artefact` names bytes and several packages may read one
/// archive** (fix round 1, F3). `locale-39` and `locale-39-turkish` both
/// declare `"media": "Locale3.9"` and there is one row for that file: without
/// this fallback the second slot had an artefact no row named, so rank 1
/// could never fire for it and its not-found row could not say which file to
/// obtain.
///
/// `None` when neither answers, which is honest: ART has no rows for these
/// bytes at all.
fn artefact_for_package(rows: &[MediaRow], package: &Package) -> Option<String> {
    if rows_for_artefact(rows, &package.id).next().is_some() {
        return Some(package.id.clone());
    }
    artefact_for_identity(rows, &package.media)
}

/// Every filename ART has recorded for `artefact`, deduplicated — the rows'
/// own first, then the adopted map's.
///
/// The map is consulted as well as the rows because the adopted table carries
/// `filenames` on none of its 186 rows and never will (fix round 1, F4): an
/// artefact whose only row is adopted — AmigaOS 3.9's BoingBag 2 — would
/// otherwise have a not-found sentence with its actionable half missing,
/// naming no file for the user to go and get. Putting ART's own claim in the
/// map rather than in a duplicate row is the same rule the artefact id itself
/// follows.
fn filenames_for(rows: &[MediaRow], artefact: Option<&str>) -> Vec<String> {
    let Some(artefact) = artefact else {
        return Vec::new();
    };
    let mut names: Vec<String> = Vec::new();
    let push = |name: &String, names: &mut Vec<String>| {
        if !names.iter().any(|seen| seen.eq_ignore_ascii_case(name)) {
            names.push(name.clone());
        }
    };
    for row in rows_for_artefact(rows, artefact) {
        for name in &row.filenames {
            push(name, &mut names);
        }
    }
    if let Ok(mapped) = adopted_artefacts() {
        for entry in mapped.iter().filter(|entry| entry.artefact == artefact) {
            for name in &entry.filenames {
                push(name, &mut names);
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
    /// What the table lookup for these bytes actually came back with — see
    /// [`BytesRead`].
    pub bytes_read: BytesRead,
}

/// What is known about one file's **bytes**, as three distinct answers.
///
/// **Three, because two was a lie in the middle** (fix round 1's F1, then the
/// re-review's F13). Rank 1 not firing has more than one cause, and a single
/// boolean collapsed the first two of these:
///
/// 1. [`BytesRead::NotRead`] — nobody has hashed this file. `osinstall_slots`
///    asks the scan cache and hashes nothing (a 490 MB ISO belongs on the job
///    that already exists), so until `osinstall_identify_media` has run over a
///    folder this is the state of every file in it. *"Its bytes are in no
///    table ART has"* would be a statement about a lookup nobody made — the
///    exact sentence [`mediahash::remembered_media_in`]'s own doc comment
///    forbids its callers from producing.
/// 2. [`BytesRead::ReadNoRow`] — hashed, and no row in either table claims
///    those bytes. This is the one that *may* say "in no table ART has".
/// 3. [`BytesRead::ReadRow`] — hashed, and a row **does** claim them. At
///    rank 1 that row is this slot's own artefact and the readout says so by
///    its own sentence. At ranks 2 and 3 it is by construction a *different*
///    artefact: rank 1 filters `facts.hashes` by `artefact_of(row) ==
///    slot.artefact`, and a single hit there returns before rank 2 is
///    reached — so a file arriving here with a row is a relabelled or
///    mislabelled disk, and telling its owner the bytes are "in no table" is
///    both false and unhelpful. The row's own `artefact` and human-readable
///    `name` travel with it so the readout can say which artefact the bytes
///    actually are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum BytesRead {
    /// Nobody has hashed this file.
    NotRead,
    /// Hashed; no row in either table claims these bytes.
    ReadNoRow,
    /// Hashed, and a row claims these bytes.
    ReadRow {
        /// The artefact id that row's bytes are of, when ART has one for it —
        /// an adopted row nothing maps answers `None` rather than inventing
        /// an id (see [`artefact_of`]).
        artefact: Option<String>,
        /// The row's own human-readable label, as its table states it.
        name: String,
    },
}

/// One file that might fill a slot, and what is known about its bytes.
///
/// A struct rather than a bare path for [`Found::bytes_read`]'s reason: a
/// filename guess that nobody has hashed, one that was hashed and matched no
/// row, and one whose bytes are some *other* catalogued artefact are three
/// different situations, and the row that offers the user a next step has to
/// know which it is looking at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub path: PathBuf,
    pub bytes_read: BytesRead,
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
    pub candidates: Vec<Candidate>,
    pub installed: Installed,
    /// The path the user chose for this slot when **that file is not there**
    /// (fix round 1, F5).
    ///
    /// Its own field rather than a `candidate` or a `Found`, because it is
    /// its own ending: a remembered ROM path on a drive that is not plugged
    /// in must not read as *chosen* (a sentence about a file that is not
    /// there) and must not read as *not found* either (which says nothing
    /// about the choice the user already made and would have them make it
    /// again). `found` stays `None`, so [`summarize`] does not count it.
    pub chosen_missing: Option<PathBuf>,
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

/// A Kickstart the user named by hand, and what the caller found when it
/// looked for the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChosenRom<'a> {
    /// The caller looked and the file is there.
    OnDisk(&'a Path),
    /// The caller looked and it is not — a remembered path on a drive nobody
    /// plugged in, which is exactly what the design's Amiga Forever
    /// suggestion will start producing.
    Absent(&'a Path),
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
    /// The Kickstart the user chose by hand, **and whether the caller found
    /// it on disk**.
    ///
    /// The existence check is the caller's — this module opens nothing — and
    /// it is expressed as a state rather than as two fields so that "chose
    /// one, it is there", "chose one, it has gone" and "chose none" cannot be
    /// muddled by a caller setting both (fix round 1, F5).
    pub rom: Option<ChosenRom<'a>>,
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
    let state = |found, candidates: Vec<PathBuf>, chosen_missing| SlotState {
        slot: slot.clone(),
        found,
        candidates: candidates
            .into_iter()
            .map(|path| Candidate {
                bytes_read: bytes_read(&path, facts),
                path,
            })
            .collect(),
        installed: installed.clone(),
        chosen_missing,
        blocked_by: Vec::new(),
        not_needed: not_needed.clone(),
    };

    if slot.kind == SlotKind::Rom {
        // Never `Filename`: the user handed ART this path, which is a
        // different thing from ART recognising a name, and collapsing the two
        // would let a hand-picked ROM read as a guess.
        return match facts.rom {
            Some(ChosenRom::OnDisk(path)) => state(
                Some(Found {
                    path: path.to_path_buf(),
                    matched_by: MatchedBy::Chosen,
                    row: None,
                    confirmed: None,
                    bytes_read: bytes_read(path, facts),
                }),
                Vec::new(),
                None,
            ),
            // Its own ending, not a `Found` and not a plain absence — see
            // `SlotState::chosen_missing`.
            Some(ChosenRom::Absent(path)) => state(None, Vec::new(), Some(path.to_path_buf())),
            None => state(None, Vec::new(), None),
        };
    }

    // --- rank 1: the bytes -------------------------------------------------
    let mut by_hash: Vec<PathBuf> = Vec::new();
    if let Some(artefact) = &slot.artefact {
        for entry in facts.hashes {
            let Some(row) = &entry.row else { continue };
            if artefact_of(row).as_deref() == Some(artefact.as_str()) {
                by_hash.push(entry.path.clone());
            }
        }
    }
    // `collapse_identical`, not a `dedup_by` over a path-sorted list (fix
    // round 1, F6): `Vec::dedup_by` removes only *adjacent* equals, so two
    // byte-identical copies separated by a third file of the same artefact —
    // a second accepted mastering, and the 3.9 CD has five rows mapping to
    // one artefact — survived as two candidates and the slot read as
    // ambiguous with itself. One collapse rule for all three ranks, so there
    // is one answer to "is this one file or two".
    let by_hash = collapse_identical(sorted(by_hash), facts.hashes);
    if by_hash.len() == 1 {
        let entry = facts
            .hashes
            .iter()
            .find(|entry| entry.path == by_hash[0])
            .expect("every rank-1 path came out of facts.hashes");
        return state(
            Some(Found {
                path: entry.path.clone(),
                matched_by: MatchedBy::Hash,
                row: entry.row.clone(),
                confirmed: entry.confirmed.clone(),
                // The same lookup as every other rank, not a hardcoded
                // `true`: at rank 1 it answers `ReadRow` naming *this
                // slot's* artefact, which is what the row already proves.
                // One function, so there is one answer to "what is known
                // about these bytes".
                bytes_read: bytes_read(&entry.path, facts),
            }),
            Vec::new(),
            None,
        );
    }
    if by_hash.len() > 1 {
        return state(None, by_hash, None);
    }

    // --- rank 2: what the artefact says it is ------------------------------
    let (by_identity, matched_by) = match slot.kind {
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
    let by_identity = collapse_identical(sorted(by_identity), facts.hashes);
    if by_identity.len() == 1 {
        return state(
            Some(Found {
                path: by_identity[0].clone(),
                matched_by,
                row: None,
                confirmed: None,
                bytes_read: bytes_read(&by_identity[0], facts),
            }),
            Vec::new(),
            None,
        );
    }
    if by_identity.len() > 1 {
        return state(None, by_identity, None);
    }

    // --- rank 3: the name somebody gave the file — a guess, never a find ---
    let by_name: Vec<PathBuf> = every_path(facts)
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
    state(
        None,
        collapse_identical(sorted(by_name), facts.hashes),
        None,
    )
}

fn sorted(mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.sort();
    paths.dedup();
    paths
}

/// What the table lookup for this file's bytes came back with — the fact
/// [`Found::bytes_read`] carries.
///
/// **Presence in `facts.hashes` was not enough** (F13). An entry there means
/// somebody hashed the file; whether its `row` is `None` or names another
/// artefact entirely is a second question, and the readout's sentence turns
/// on it. A relabelled disk — one that answers rank 2 by volume name while
/// its bytes belong to a different catalogued artefact — used to render
/// *"its bytes are in no table ART has"*, which is false about a file the
/// table knows perfectly well.
fn bytes_read(path: &Path, facts: &Facts<'_>) -> BytesRead {
    let Some(entry) = facts.hashes.iter().find(|entry| entry.path == path) else {
        return BytesRead::NotRead;
    };
    match &entry.row {
        None => BytesRead::ReadNoRow,
        Some(row) => BytesRead::ReadRow {
            artefact: artefact_of(row),
            name: row.name.clone(),
        },
    }
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
///
/// **The one collapse rule, for all three ranks** (fix round 1, F6). It is
/// order-independent, unlike `Vec::dedup_by`, which is what rank 1 used to
/// use over a path-sorted list and which therefore only ever collapsed
/// duplicates that happened to land next to each other.
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
    use crate::core::osinstall::{Component, PathRule, RuleKind};

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
        rom_on_disk: bool,
        program_versions: Vec<(String, String)>,
    }

    impl Gathered {
        fn empty() -> Self {
            Gathered {
                media: Vec::new(),
                packages: Vec::new(),
                hashes: Vec::new(),
                rom: None,
                rom_on_disk: true,
                program_versions: Vec::new(),
            }
        }

        fn facts<'a>(&'a self, manifest: Option<&'a DistributionManifest>) -> Facts<'a> {
            Facts {
                media: &self.media,
                packages: &self.packages,
                hashes: &self.hashes,
                manifest,
                rom: self.rom.as_deref().map(|path| match self.rom_on_disk {
                    true => ChosenRom::OnDisk(path),
                    false => ChosenRom::Absent(path),
                }),
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

    fn state_of_slot<'a>(slots: &'a [Slot], id: &str) -> &'a Slot {
        slots
            .iter()
            .find(|slot| slot.id == id)
            .unwrap_or_else(|| panic!("no slot {id}"))
    }

    fn candidate_paths(state: &SlotState) -> Vec<PathBuf> {
        state
            .candidates
            .iter()
            .map(|candidate| candidate.path.clone())
            .collect()
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
            candidate_paths(overlay),
            vec![PathBuf::from("D:/a/BoingBag39-1-UAE.lha")]
        );
        // Nobody hashed it, and the readout has to be able to say so rather
        // than claiming the table does not know these bytes (F1).
        assert_eq!(overlay.candidates[0].bytes_read, BytesRead::NotRead);
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
            candidate_paths(bb1),
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

    // -----------------------------------------------------------------
    // Fix round 1
    // -----------------------------------------------------------------

    /// F1 — the two causes of rank 1 falling silent, kept apart.
    #[test]
    fn a_rank_two_match_says_whether_anybody_has_read_the_bytes() {
        let slots = slots_for("AmigaOS 3.9").unwrap();

        // Nobody has hashed anything: the readout must be able to say ART did
        // not look, not that the table does not know these bytes.
        let mut unread = Gathered::empty();
        unread
            .packages
            .push(archive("D:/a/renamed.lha", "BoingBag3.9-1"));
        let states = resolve(&slots, &unread.facts(None));
        let found = state_of(&states, "package:boingbag-39-1")
            .found
            .as_ref()
            .unwrap();
        assert_eq!(found.matched_by, MatchedBy::TopLevelDirectory);
        assert_eq!(
            found.bytes_read,
            BytesRead::NotRead,
            "the scan cache had no answer for it"
        );

        // Hashed, and the hash matched no row this slot's artefact names —
        // the genuinely stronger statement.
        let mut hashed_unmatched = Gathered::empty();
        hashed_unmatched
            .packages
            .push(archive("D:/a/renamed.lha", "BoingBag3.9-1"));
        hashed_unmatched
            .hashes
            .push(hashed("D:/a/renamed.lha", &"1".repeat(32)));
        let states = resolve(&slots, &hashed_unmatched.facts(None));
        let found = state_of(&states, "package:boingbag-39-1")
            .found
            .as_ref()
            .unwrap();
        assert_eq!(found.matched_by, MatchedBy::TopLevelDirectory);
        assert_eq!(
            found.bytes_read,
            BytesRead::ReadNoRow,
            "somebody hashed it and no row claimed it"
        );

        // Rank 1 is only ever reached through a hash, so it always has them —
        // and the row it matched is this slot's own artefact.
        let mut by_bytes = Gathered::empty();
        by_bytes.hashes.push(hashed("D:/a/bb1.lha", BB1_45_15_MD5));
        let states = resolve(&slots, &by_bytes.facts(None));
        assert!(matches!(
            state_of(&states, "package:boingbag-39-1")
                .found
                .as_ref()
                .unwrap()
                .bytes_read,
            BytesRead::ReadRow { artefact: Some(ref id), .. } if id == "boingbag-39-1"
        ));
    }

    /// F13 — "nobody hashed it", "hashed and in no table" and "hashed, and
    /// the table says these bytes are something **else**" are three answers,
    /// and the third used to render as the second.
    ///
    /// The case is a real one and it is exactly what this module exists to
    /// sort out: a disc relabelled `AmigaOS3.9` whose bytes are BoingBag 1's.
    /// Rank 1 excludes it from the CD slot precisely *because* the row names
    /// another artefact; rank 2 then matches it on the volume name, and the
    /// old boolean said only "somebody hashed this", which the readout
    /// rendered as *"its bytes are in no table ART has"* — false about a file
    /// the table knows perfectly well.
    #[test]
    fn a_row_for_a_different_artefact_is_not_the_same_as_no_row_at_all() {
        let slots = slots_for("AmigaOS 3.9").unwrap();

        let mut relabelled = Gathered::empty();
        // The volume name says the CD; the bytes say BoingBag 1.
        relabelled
            .media
            .push(medium("D:/a/relabelled.iso", "AmigaOS3.9"));
        relabelled
            .hashes
            .push(hashed("D:/a/relabelled.iso", BB1_45_15_MD5));

        let states = resolve(&slots, &relabelled.facts(None));
        let cd = state_of(&states, "medium:AmigaOS3.9");
        let found = cd.found.as_ref().expect("rank 2 matched the volume name");
        assert_eq!(found.matched_by, MatchedBy::VolumeName);
        match &found.bytes_read {
            BytesRead::ReadRow { artefact, name } => {
                assert_eq!(artefact.as_deref(), Some("boingbag-39-1"));
                assert!(
                    !name.is_empty(),
                    "the readout names the artefact the bytes actually are"
                );
            }
            other => panic!("the table knows these bytes: {other:?}"),
        }
    }

    /// F3 — a package slot whose own id no row names falls back to the
    /// artefact of the archive it reads, so two packages over one archive
    /// both resolve.
    #[test]
    fn two_packages_reading_one_archive_share_its_artefact() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        // `locale-39` has its own row; `locale-39-turkish` declares the same
        // `Locale3.9` archive and has none.
        assert_eq!(
            state_of_slot(&slots, "package:locale-39")
                .artefact
                .as_deref(),
            Some("locale-39")
        );
        assert_eq!(
            state_of_slot(&slots, "package:locale-39-turkish")
                .artefact
                .as_deref(),
            Some("locale-39"),
            "a row names bytes, and two packages may read one archive"
        );
        // And the Turkish catalog pack now has a row of its own, so it does
        // not fall back at all.
        assert_eq!(
            state_of_slot(&slots, "package:locale-turkish")
                .artefact
                .as_deref(),
            Some("locale-turkish")
        );
    }

    /// F3/F4 — every shipped 3.9 package slot can tell the user what file to
    /// obtain. An artefact no row names produces a not-found row with its
    /// actionable half missing, which is what this pins against a fourth
    /// instance.
    #[test]
    fn every_shipped_package_slot_for_3_9_names_at_least_one_expected_filename() {
        let empty: Vec<String> = slots_for("AmigaOS 3.9")
            .unwrap()
            .into_iter()
            .filter(|slot| slot.kind == SlotKind::Package && slot.filenames.is_empty())
            .map(|slot| slot.id)
            .collect();
        assert!(
            empty.is_empty(),
            "package slots that cannot say what file to get: {empty:?}"
        );
    }

    /// F4 — the adopted map's own `filenames` reach a slot whose only row is
    /// an adopted one.
    #[test]
    fn an_adopted_only_artefact_still_names_the_file_to_obtain() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let bb2 = state_of_slot(&slots, "package:boingbag-39-2");
        assert_eq!(bb2.artefact.as_deref(), Some("boingbag-39-2"));
        assert!(
            bb2.filenames.iter().any(|n| n == "BoingBag39-2.lha"),
            "its only row is the adopted fd45d24b…, which carries no filenames: {:?}",
            bb2.filenames
        );
        // The name is ART's own claim; the provenance is still the table's,
        // stated as the table states it.
        assert_eq!(bb2.provenance.as_deref(), Some("Haage and Partners (3.9)"));
    }

    /// F6 — two different masterings of one artefact present at once. Rank 1
    /// is ambiguous, and it is not resolved by list order.
    #[test]
    fn two_masterings_of_one_artefact_leave_rank_one_ambiguous() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let mut gathered = Gathered::empty();
        gathered.hashes.push(hashed("D:/b/second.iso", CD_OWN_MD5));
        gathered
            .hashes
            .push(hashed("D:/a/first.iso", CD_ADOPTED_MD5));
        let states = resolve(&slots, &gathered.facts(None));
        let cd = state_of(&states, "medium:AmigaOS3.9");
        assert!(cd.found.is_none(), "two masterings, no arbitrary winner");
        assert_eq!(
            candidate_paths(cd),
            vec![
                PathBuf::from("D:/a/first.iso"),
                PathBuf::from("D:/b/second.iso")
            ]
        );
        assert!(cd
            .candidates
            .iter()
            .all(|c| matches!(c.bytes_read, BytesRead::ReadRow { .. })));
    }

    /// F6 — `collapse_identical` is the one rule, and rank 2 goes through it:
    /// the same archive kept in two folders is one candidate even though
    /// neither copy's hash matches any row.
    #[test]
    fn one_archive_kept_twice_is_one_candidate_at_rank_two_as_well() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let mut gathered = Gathered::empty();
        let md5 = "2".repeat(32);
        gathered
            .packages
            .push(archive("D:/a/BoingBag39-1.lha", "BoingBag3.9-1"));
        gathered
            .packages
            .push(archive("D:/b/backup.lha", "BoingBag3.9-1"));
        gathered.hashes.push(hashed("D:/a/BoingBag39-1.lha", &md5));
        gathered.hashes.push(hashed("D:/b/backup.lha", &md5));

        let states = resolve(&slots, &gathered.facts(None));
        let bb1 = state_of(&states, "package:boingbag-39-1");
        let found = bb1
            .found
            .as_ref()
            .expect("one archive kept twice is one archive");
        assert_eq!(found.matched_by, MatchedBy::TopLevelDirectory);
        assert_eq!(found.path, PathBuf::from("D:/a/BoingBag39-1.lha"));
    }

    /// F12 — an adopted row's `volume` is Hatcher's identifier, not an
    /// AmigaDOS name, and this module never compares one against the other.
    #[test]
    fn an_adopted_rows_volume_never_answers_an_identity_question() {
        let rows = mediahash::rows().unwrap();
        // `AmigaOS3_9BB1` is an adopted identifier that maps to a real
        // artefact — so if adopted rows were consulted, asking for it would
        // answer `boingbag-39-1`. It must answer nothing.
        assert_eq!(artefact_for_identity(rows, "AmigaOS3_9BB1"), None);
        // While ART's own rows, whose `volume` really is what the archive
        // calls itself, still answer.
        assert_eq!(
            artefact_for_identity(rows, "BoingBag3.9-1"),
            Some("boingbag-39-1".to_string())
        );
    }

    // --- F11: synthetic recipes, which no test used to state ---------------

    fn component(id: &str, media: &str, required: bool, condition: Option<Condition>) -> Component {
        Component {
            id: id.to_string(),
            media: media.to_string(),
            rules: vec![PathRule {
                from: "C".to_string(),
                to: "C".to_string(),
                kind: RuleKind::Subtree,
            }],
            required,
            condition,
            overrides: Vec::new(),
            user_startup: Vec::new(),
            activate: Vec::new(),
            exclusive_group: None,
            label_key: None,
            available: true,
            layer: None,
            removes: Vec::new(),
        }
    }

    fn recipe_of(components: Vec<Component>) -> Recipe {
        Recipe {
            release: "Test OS".to_string(),
            base: None,
            layers: Vec::new(),
            components,
        }
    }

    /// A release off **several** disks — the shape every shipped release but
    /// 3.9 has, and one no test stated until this round. `slots_over`'s own
    /// doc comment promised it.
    #[test]
    fn a_release_off_several_disks_gets_one_medium_slot_each_in_recipe_order() {
        let slots = slots_over(
            &recipe_of(vec![
                component("base", "Workbench3.2", true, None),
                component("extras", "Extras3.2", false, None),
                // A second component off a disk already named adds no second
                // slot — a slot is an artefact, not a component.
                component("fonts", "workbench3.2", false, None),
            ]),
            &[],
        )
        .unwrap();
        assert_eq!(
            slots.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            vec!["medium:Workbench3.2", "medium:Extras3.2"],
            "no ROM condition anywhere, so no ROM slot either"
        );
        assert!(slots[0].required, "a required component reads Workbench3.2");
        assert!(!slots[1].required, "nothing required reads Extras3.2");
        assert_eq!(slots[1].position, 1);
    }

    /// A release that asks about the ROM without stating a floor — AmigaOS
    /// 3.2's shape. The slot exists (something has to be paired) and is not
    /// required, and its identity is empty because the recipe states no
    /// number.
    #[test]
    fn a_rom_condition_with_no_floor_gives_an_optional_rom_slot_with_no_number() {
        let slots = slots_over(
            &recipe_of(vec![
                component("base", "Workbench3.2", true, None),
                component(
                    "modules-a1200",
                    "Modules3.2",
                    false,
                    Some(Condition::RomOlderThan { major: 47 }),
                ),
            ]),
            &[],
        )
        .unwrap();
        let rom = slots.last().unwrap();
        assert_eq!(rom.id, "rom");
        assert!(
            !rom.required,
            "rom-older-than switches a fallback on; it states no floor"
        );
        assert_eq!(rom.identity, "", "the recipe states no number to show");
    }

    /// And a recipe that asks nothing about the ROM has no ROM slot at all —
    /// the arm the shipped 3.9 recipe cannot exercise.
    #[test]
    fn a_recipe_that_never_asks_about_the_rom_has_no_rom_slot() {
        let slots =
            slots_over(&recipe_of(vec![component("base", "Only", true, None)]), &[]).unwrap();
        assert!(slots.iter().all(|slot| slot.kind != SlotKind::Rom));
    }
}
