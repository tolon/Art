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
    /// Directories this artefact's disc carries at its root, for design
    /// § 3.6's structural check. **ART's own claim**, like `filenames`:
    /// Hatcher's rows say nothing about a disc's contents. Empty means ART
    /// has no expectation and never reports such a disc as incomplete.
    #[serde(default)]
    pub directories: Vec<String>,
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
    /// Where the **artefact** comes from, for somebody who has to go and get
    /// it: an adopted row's own `source`, verbatim (`"Haage and Partners
    /// (3.9)"`). `None` when no adopted row names this artefact — including
    /// when one of ART's own rows does, because that field records where
    /// *this machine's dump* came from and not where the artefact comes from.
    /// See [`provenance_for`].
    pub provenance: Option<String>,
    /// Chain order: media first, then the packages in
    /// [`package::order`](super::package::order)'s requires-respecting order
    /// with each package's overlays immediately after it, and the ROM last.
    pub position: u32,
    /// Slot ids that must be installed before this one — a package's own
    /// `requires`, and the CD its installer verifies before it will work.
    pub requires: Vec<String>,
    /// A newer artefact that makes this one unnecessary.
    ///
    /// **Still empty after round 3, and now for a reason rather than because
    /// nothing had been built.** `Package::superseded_by` is where that fact
    /// lives, and [`super::chain::rows_for`] — which already has to read the
    /// packages for their `chain_position`, their name and their
    /// `not_yet_runnable` — reads it straight from the recipe. Copying it
    /// onto the slot as well would be a second copy of one fact, and copies
    /// drift; a resolved slot would then be able to disagree with the chain
    /// row above it about the same artefact, which is the two-screens-one-
    /// artefact defect the overrides field was added to close.
    ///
    /// Kept on the wire rather than deleted so the shape does not change
    /// under the screen the day a *slot-level* supersession exists — one an
    /// artefact's own bytes state, rather than one a package's recipe does.
    pub superseded_by: Vec<String>,
    /// Directories a disc filling this slot must carry at its root — design
    /// § 3.6's structural check, and **data on the artefact map, never a list
    /// in code**: which six directories an AmigaOS 3.9 CD has is a fact about
    /// that disc, and a second release's disc would need its own.
    ///
    /// Empty for everything but a medium ART records expectations for.
    pub expects_directories: Vec<String>,
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
        // Read before the move into the literal below; `artefact` is consumed
        // there and this is the one slot kind that has directories at all.
        let artefact_ref = artefact.clone();
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
            expects_directories: directories_for(artefact_ref.as_deref()),
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
            expects_directories: Vec::new(),
        });

        // --- and its overlays, immediately after it ------------------------
        //
        // **An overlay requires the package whose drawer it patches** (round
        // 2 whole-branch review, I15). It shipped with `requires: []` and the
        // reasoning written beside it was that an overlay patches the package
        // *before* the run, so waiting for the package to be installed would
        // be backwards. The first half is right and the conclusion did not
        // follow: what the overlay waits for is not the package being
        // *installed*, it is the package's own **archive being here** — a
        // BoingBag 3.9-1 UAE fix with no BoingBag 3.9-1 beside it patches
        // nothing. The empty list made the drop-folder guide print *"Nothing
        // has to be in place before it"* about exactly that file, which is
        // false.
        //
        // `resolve` is what keeps the direction right: an overlay's
        // requirement is satisfied by the package slot being **found**, not
        // by its being installed — see `requirement_satisfied`.
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
                requires: vec![format!("package:{}", pkg.id)],
                superseded_by: Vec::new(),
                expects_directories: Vec::new(),
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
            expects_directories: Vec::new(),
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

/// Where the **artefact** comes from — and only when a row is entitled to say
/// so.
///
/// **`MediaRow::source` is not one field with one meaning** (fix round 1, M3).
/// Its own doc says what it is: *"where Hatcher's table says this dump came
/// from"*. On an **adopted** row that is a statement about the artefact —
/// `"Haage and Partners (3.9)"` — and it is exactly what somebody trying to
/// obtain the file needs. On one of **ART's own** rows it is a statement about
/// *this machine*: `"the owner's copy, 2026-09-08"`. Rendering that behind the
/// words *"where it comes from"* — in a field hint, and worse, in a text file
/// ART writes into a stranger's folder — is a date-stamped claim about
/// somebody else's disk and no help in obtaining anything. It is the
/// confident, wrong sentence this project is most expensive at, and it was
/// already pinned by a test before anybody read it as prose.
///
/// So an own-table row answers `None`, and the caller says *"ART has no note
/// of where this one comes from"*, which is true. The rows are searched in
/// table order (adopted first), so an artefact with rows in both tables still
/// gets the adopted row's sentence.
///
/// Closing this properly means a real *where to obtain* note per artefact,
/// which belongs on the adopted-artefact map beside `filenames` — the place
/// the brief's `homepage` was heading for. Until somebody writes those notes,
/// silence is the honest answer.
fn provenance_for(rows: &[MediaRow], artefact: Option<&str>) -> Option<String> {
    let artefact = artefact?;
    rows_for_artefact(rows, artefact)
        .find(|row| row.table_origin == mediahash::Origin::Adopted)
        .map(|row| row.source.clone())
}

/// The directories a disc of `artefact` carries at its root, from the
/// artefact map — design § 3.6's structural check, as **data**.
///
/// On the map beside `filenames` rather than in code, for the same reason
/// that field is there: which directories an AmigaOS 3.9 CD has is a fact
/// about that disc, ART's own claim rather than Hatcher's, and a second
/// release's disc needs its own list, not an `if` here.
fn directories_for(artefact: Option<&str>) -> Vec<String> {
    let Some(artefact) = artefact else {
        return Vec::new();
    };
    adopted_artefacts()
        .ok()
        .and_then(|mapped| mapped.iter().find(|entry| entry.artefact == artefact))
        .map(|entry| entry.directories.clone())
        .unwrap_or_default()
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
    /// The first directory [`Slot::expects_directories`] names that the disc
    /// filling this slot does **not** carry — design § 3.6's structural check
    /// beside the hash.
    ///
    /// *Matched but incomplete* is a different sentence from *not found*, and
    /// that is the whole of it: a disc whose bytes ART recognises but whose
    /// root is missing `Emergency-Boot` is a disc somebody re-mastered or a
    /// partial copy, and telling them it is absent would send them looking
    /// for a file that is sitting right there.
    ///
    /// `None` means either that ART has nothing to check (no expectations, or
    /// nobody listed the disc's root) or that everything expected is there.
    /// Those two are deliberately one value here because the row says nothing
    /// in both cases; the fact that separates them is
    /// [`Facts::disc_roots`]'s own membership.
    pub incomplete: Option<String>,
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
    /// **Files the user picked by hand, per slot** (design § 3.4).
    ///
    /// The generalisation of [`Facts::rom`], which was the first of these and
    /// had to be a field of its own because it arrives from a different
    /// screen. An override **outranks every rank**: it is not an
    /// identification ART made, so it cannot be compared with one — the user
    /// said *this file*, and ART's job is to use it and say who chose it.
    ///
    /// The round-2 review found the readout and the Amiga-side panel saying
    /// opposite things about one artefact because only the panel knew about
    /// these: the readout printed *"BoingBag 3.9-1 is not in the folders you
    /// named"* about a file ART was holding a path for and would use in the
    /// run.
    pub overrides: &'a [Override<'a>],
    /// The root directory names of each disc the caller could read, as
    /// `(path, names)` — the fact behind [`SlotState::incomplete`].
    ///
    /// The caller's, because this module opens nothing. A disc missing from
    /// this list is a disc nobody listed, which is **not** the same as a disc
    /// whose root is empty: the first says nothing and leaves `incomplete`
    /// `None`, the second is a real answer.
    pub disc_roots: &'a [(PathBuf, Vec<String>)],
}

/// One file the user chose by hand for a named slot.
///
/// `on_disk` is the caller's check, exactly as [`ChosenRom`]'s two variants
/// are and for the same reason (fix round 1, F5): this module opens nothing,
/// and "chose one, it has gone" is its own ending rather than a plain
/// absence.
///
/// **`slot` may name an overlay's package rather than a particular drawer.**
/// An overlay slot's id is `overlay:<package>:<drawer>`, and the screen that
/// holds these overrides has one *"the package's update archive"* field per
/// package, not one per drawer — so `overlay:boingbag-39-1` matches every
/// overlay of that package. An exact id always matches only itself. Today
/// every shipped package declares at most one overlay, so the two readings
/// coincide; the prefix rule is what keeps a second overlay from silently
/// dropping the user's choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Override<'a> {
    pub slot: &'a str,
    pub path: &'a Path,
    pub on_disk: bool,
}

impl Override<'_> {
    /// Whether this override is about `slot`.
    fn names(&self, slot: &Slot) -> bool {
        if slot.id == self.slot {
            return true;
        }
        slot.kind == SlotKind::Overlay && slot.id.starts_with(&format!("{}:", self.slot))
    }
}

/// Resolve every slot against `facts`.
///
/// Two passes, and the order is load-bearing: [`SlotState::blocked_by`] is
/// about *other* slots' [`Installed`] state, so nothing can be said about it
/// until every slot has one.
pub fn resolve(slots: &[Slot], facts: &Facts<'_>) -> Vec<SlotState> {
    let mut states: Vec<SlotState> = slots.iter().map(|slot| resolve_one(slot, facts)).collect();

    let known: Vec<(String, SlotKind, bool, bool)> = states
        .iter()
        .map(|state| {
            (
                state.slot.id.clone(),
                state.slot.kind,
                state.installed != Installed::No,
                state.found.is_some(),
            )
        })
        .collect();
    for state in &mut states {
        let kind = state.slot.kind;
        state.blocked_by = state
            .slot
            .requires
            .iter()
            .filter(|need| {
                // A requirement naming a slot this release has none of is
                // treated as not installed rather than ignored: silently
                // dropping it would turn a data mistake into a run that
                // looks ready.
                !known.iter().any(|(id, required, installed, found)| {
                    id == *need && requirement_met(kind, *required, *installed, *found)
                })
            })
            .cloned()
            .collect();
    }
    states
}

/// Whether a requirement is met, given what is known about the slot it names.
///
/// **An overlay asks a different question from everything else, and asking
/// the same one produced a false sentence** (round 2 whole-branch review,
/// I15). A package waits for the package before it to be *installed*: running
/// BoingBag 3.9-2 against a tree BoingBag 3.9-1 never touched is a system
/// that boots and is quietly wrong (ART-186). An overlay waits for nothing of
/// the kind — it is copied **over its package's own drawer, before that
/// package's installer runs**, so "BoingBag 3.9-1 must be installed first"
/// describes the opposite of what happens. What it genuinely needs is the
/// package's own archive to be in hand, which is `found`.
///
/// `installed` still satisfies an overlay: a package already on the tree is
/// not one the overlay is waiting for either, and answering *blocked* there
/// would be a row waiting for something that has already happened.
///
/// **A required *medium* is met by having it, not by installing it** (round
/// 3). A `required_medium` becomes a `requires` entry naming the medium slot
/// — BoingBag 3.9-1's own `Updater` checks for the AmigaOS 3.9 CD-ROM before
/// it does anything (ART-193) — and what that check wants is the disc in a
/// drive, which is exactly `found`. Reading it as *installed* said "needs
/// AmigaOS3.9 first" about a build whose ISO was sitting in the folder the
/// user had just named, and told them to install a disc they were not
/// installing. The design says it in as many words: ready is *"every
/// `requires` installed and `required_medium` **found**"*.
fn requirement_met(requiring: SlotKind, required: SlotKind, installed: bool, found: bool) -> bool {
    if installed {
        return true;
    }
    match required {
        SlotKind::Medium => found,
        _ => requiring == SlotKind::Overlay && found,
    }
}

fn resolve_one(slot: &Slot, facts: &Facts<'_>) -> SlotState {
    let installed = installed_state(slot, facts.manifest);
    let not_needed = not_needed_for(slot, facts);
    let state = |found: Option<Found>, candidates: Vec<PathBuf>, chosen_missing| SlotState {
        incomplete: found
            .as_ref()
            .and_then(|one| missing_directory(slot, &one.path, facts)),
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

    // --- rank 0: the user said so ------------------------------------------
    //
    // **Above every rank, because it is not one.** The others are ART
    // deciding what a file is; this is the user telling it. A hash find that
    // outranked a hand-picked path would be ART overruling a decision, which
    // is the one thing `remembered.ts`'s rule forbids — and the readout would
    // then say something different from the panel about the file the run is
    // actually going to use.
    if let Some(chosen) = facts.overrides.iter().find(|one| one.names(slot)) {
        return match chosen.on_disk {
            true => state(
                Some(Found {
                    path: chosen.path.to_path_buf(),
                    matched_by: MatchedBy::Chosen,
                    row: None,
                    confirmed: None,
                    bytes_read: bytes_read(chosen.path, facts),
                }),
                Vec::new(),
                None,
            ),
            false => state(None, Vec::new(), Some(chosen.path.to_path_buf())),
        };
    }

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

/// The first directory `slot` expects that the disc at `path` does not carry
/// — design § 3.6's structural check beside the hash.
///
/// **Silent unless ART has both halves.** A slot with no expectations, or a
/// disc nobody listed the root of, answers `None` — because *"ART did not
/// look"* is not *"ART looked and it is not there"*, which is the same rule
/// `BytesRead` exists for one field over. Names are compared the way
/// AmigaDOS compares them (case-insensitively over Latin-1), since that is
/// what the disc's own directory is.
fn missing_directory(slot: &Slot, path: &Path, facts: &Facts<'_>) -> Option<String> {
    if slot.expects_directories.is_empty() {
        return None;
    }
    let names = facts
        .disc_roots
        .iter()
        .find(|(disc, _)| disc == path)
        .map(|(_, names)| names)?;
    slot.expects_directories
        .iter()
        .find(|wanted| !names.iter().any(|name| amiga_names_equal(name, wanted)))
        .cloned()
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

// ---------------------------------------------------------------------------
// The drop-folder guide (design § 3.7)
// ---------------------------------------------------------------------------

const GUIDE_EN_JSON: &str = include_str!("recipes/guide.en.json");
const GUIDE_TR_JSON: &str = include_str!("recipes/guide.tr.json");

/// The guide's own words, in one language.
///
/// **Data beside the recipes, not an i18n key.** A Rust module never renders
/// a key it does not have (CLAUDE.md), and `src/i18n` is the *screen's*
/// catalogue — reachable only from React, which is not what writes this file.
/// So the two languages live here, in the same folder as the recipes the
/// guide is composed from, and `guide_parity_holds_between_the_two_languages`
/// keeps them saying the same things the way the frontend's own parity test
/// does for `en.json`/`tr.json`.
///
/// Every field is required. `deny_unknown_fields` plus named fields is what
/// makes "this language is missing a line" a parse failure rather than a
/// silently empty paragraph in somebody's folder — the same reason
/// `recipe.rs` deserializes into a struct rather than a map.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GuideStrings {
    /// What the file is called. **The name is part of the guide's own data**:
    /// the command takes a language and no filename, and the SAFE_CREATE
    /// check has to be made against the real name before anything is opened.
    pub filename: String,
    pub heading: String,
    /// `{release}`.
    pub release: String,
    pub intro: String,
    pub no_downloads: String,
    pub required: String,
    pub optional: String,
    /// `{filenames}`.
    pub expected: String,
    pub filenames_unknown: String,
    /// `{provenance}`.
    pub provenance: String,
    pub provenance_unknown: String,
    /// The ROM's own line, in place of the expected/provenance pair —
    /// `{major}` (fix round 1, M1).
    ///
    /// **A ROM slot is never resolved from a folder at all**: `resolve_one`'s
    /// rank 2 answers `MatchedBy::Chosen` for it and rank 3 is keyed on
    /// `filenames`, which a ROM slot has none of. It is filled from
    /// [`Facts::rom`] — the Kickstart the user chose in ART — and from
    /// nothing else. So the generic lines told a reader to put a Kickstart in
    /// this folder and promised ART would identify it by its contents; both
    /// claims were false, and this is the file ART leaves on somebody's disk.
    pub rom_not_in_this_folder: String,
    /// The same, for a release that states no floor — `rom-older-than` alone
    /// gives an optional ROM slot with no number, and interpolating an empty
    /// `{major}` would read "Kickstart  or newer".
    pub rom_not_in_this_folder_no_floor: String,
    /// `{needs}`.
    pub needs_first: String,
    pub needs_nothing: String,
    pub without_required: String,
    /// `{dependents}` — a slot another slot's `requires` names (fix round 1,
    /// M2).
    ///
    /// Every package slot is `required: false` by construction (a package is
    /// a thing the user chooses), so BoingBag 3.9-1 used to be told it could
    /// be skipped with "nothing else fails" four lines above the guide's own
    /// statement that BoingBag 3.9-2 goes on after it. The file contradicted
    /// itself and the half a reader acts on was the wrong half.
    pub without_needed_by: String,
    pub without_optional: String,
    pub footer: String,
}

/// The strings for `language`, or English for anything else.
///
/// **A fallback rather than a refusal**, and the direction is deliberate: the
/// only caller is the UI, which sends its own two language codes, so an
/// unknown one means ART has grown a third language and nobody has written
/// this file for it yet. Refusing would take a working button away; falling
/// back writes a guide the user can read even if it is not in their language,
/// which is what the guide is for.
pub fn guide_strings(language: &str) -> CoreResult<GuideStrings> {
    let json = match language {
        "tr" => GUIDE_TR_JSON,
        _ => GUIDE_EN_JSON,
    };
    serde_json::from_str(json).map_err(|e| CoreError::Malformed {
        format: format!("guide.{language}.json"),
        detail: e.to_string(),
    })
}

/// One `{placeholder}` filled in. Deliberately not a template engine: three
/// substitutions, each with exactly one placeholder, and a missing one leaves
/// the sentence as its author wrote it rather than half-rendered.
fn fill(template: &str, placeholder: &str, value: &str) -> String {
    template.replace(&format!("{{{placeholder}}}"), value)
}

/// The text of *"what goes in this folder"*, composed from the slots.
///
/// **Generated from the slot list, so it cannot drift from what the code
/// accepts** (design § 3.7). Every line is something the recipes state: the
/// names ART expects, the source note the media rows carry, and the order
/// `requires` already encodes. Nothing here is a URL — ART downloads none of
/// this and the file says so — and nothing is a guess: a slot with no
/// recorded file name says that, rather than printing "Expected ." at
/// somebody.
///
/// Takes [`Slot`]s and not [`SlotState`]s on purpose. The guide answers *what
/// this release needs*, which is true of the release wherever it is written;
/// a state is about one folder set at one moment, and a file dropped into a
/// folder tomorrow would make a guide composed from states stale in a way the
/// reader could not see.
///
/// A prerequisite or a dependent is named the way the reader will see it in
/// this same file, never by slot id.
///
/// **The line endings are CRLF** (fix round 1, L8). This is a `.txt` a Windows
/// user opens in whatever they have; every other file ART writes for a machine
/// to read keeps its own convention, and this one is written for a person on
/// this platform.
pub fn guide_text(slots: &[Slot], release: &str, language: &str) -> CoreResult<String> {
    let words = guide_strings(language)?;
    let names: Vec<(&str, String)> = slots
        .iter()
        .map(|slot| (slot.id.as_str(), guide_name(slot)))
        .collect();
    let name_of = |id: &str| -> String {
        names
            .iter()
            .find(|(other, _)| *other == id)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| id.to_string())
    };

    let mut out = String::new();
    out.push_str(&words.heading);
    out.push('\n');
    out.push_str(&"=".repeat(words.heading.chars().count()));
    out.push_str("\n\n");
    out.push_str(&fill(&words.release, "release", release));
    out.push_str("\n\n");
    out.push_str(&words.intro);
    out.push_str("\n\n");
    out.push_str(&words.no_downloads);
    out.push_str("\n\n");

    // Position order — the order the material actually goes on, which is the
    // order somebody filling a folder wants to read it in.
    let mut ordered: Vec<&Slot> = slots.iter().collect();
    ordered.sort_by_key(|slot| slot.position);

    // **One entry per artefact** (round 2 review, L11). `locale-39` and
    // `locale-39-turkish` genuinely share one archive — both declare
    // `"media": "Locale3.9"` and the second's recipe says so — so the guide
    // listed `Locale3_9.lha` twice under two OPTIONAL headings and a person
    // filling a folder went looking for two files. Grouped by artefact id,
    // the group's names are joined into one heading and everything else is
    // the union: one file, both names, one place to read it.
    //
    // A slot with no artefact (`None`) is never grouped with another — `None`
    // means the tables name no artefact for it, which is a gap in ART's
    // knowledge and not a statement that two slots are the same thing.
    let mut written: Vec<&str> = Vec::new();
    for slot in &ordered {
        if let Some(artefact) = &slot.artefact {
            if written.contains(&artefact.as_str()) {
                continue;
            }
            written.push(artefact.as_str());
        }
        let group: Vec<&&Slot> = match &slot.artefact {
            Some(artefact) => ordered
                .iter()
                .filter(|other| other.artefact.as_deref() == Some(artefact.as_str()))
                .collect(),
            None => vec![slot],
        };

        // Any one of them required makes the entry required: the reader has
        // to obtain the file either way.
        let required = group.iter().any(|one| one.required);
        let tag = match required {
            true => &words.required,
            false => &words.optional,
        };
        out.push_str(&format!(
            "{tag}  {}\n",
            group
                .iter()
                .map(|one| guide_name(one))
                .collect::<Vec<_>>()
                .join(" / ")
        ));

        // **The ROM is not a file for this folder** (M1). Its own line
        // replaces the expected/provenance pair, because both of those
        // answers would be about looking in a folder ART never looks in for
        // it.
        if slot.kind == SlotKind::Rom {
            out.push_str(&format!(
                "  {}\n",
                match slot.identity.is_empty() {
                    true => words.rom_not_in_this_folder_no_floor.clone(),
                    false => fill(&words.rom_not_in_this_folder, "major", &slot.identity),
                }
            ));
        } else {
            // The group's union, deduplicated: two slots reading one archive
            // expect one set of file names, and printing them twice was the
            // defect this grouping exists for.
            let mut filenames: Vec<String> = Vec::new();
            for one in &group {
                for name in &one.filenames {
                    if !filenames.iter().any(|seen| seen.eq_ignore_ascii_case(name)) {
                        filenames.push(name.clone());
                    }
                }
            }
            out.push_str(&format!(
                "  {}\n",
                match filenames.is_empty() {
                    true => words.filenames_unknown.clone(),
                    false => fill(&words.expected, "filenames", &filenames.join(", ")),
                }
            ));
            out.push_str(&format!(
                "  {}\n",
                match group.iter().find_map(|one| one.provenance.as_ref()) {
                    Some(source) => fill(&words.provenance, "provenance", source),
                    None => words.provenance_unknown.clone(),
                }
            ));
        }

        // Everything the group needs first, and everything that needs the
        // group — the union, and never a slot of the group itself: a package
        // does not go on after the archive it *is*.
        let mut requires: Vec<String> = Vec::new();
        for one in &group {
            for id in &one.requires {
                if group.iter().any(|other| &other.id == id) {
                    continue;
                }
                let name = name_of(id);
                if !requires.contains(&name) {
                    requires.push(name);
                }
            }
        }
        out.push_str(&format!(
            "  {}\n",
            match requires.is_empty() {
                true => words.needs_nothing.clone(),
                false => fill(&words.needs_first, "needs", &requires.join(", ")),
            }
        ));

        // Who else stops working. **Not derivable from `required` alone**
        // (M2): every package slot is optional by construction, so BoingBag
        // 3.9-1 was told "nothing else fails" four lines above this same
        // file's own statement that BoingBag 3.9-2 goes on after it.
        let mut dependents: Vec<String> = Vec::new();
        for other in slots {
            if group.iter().any(|one| one.id == other.id) {
                continue;
            }
            if other
                .requires
                .iter()
                .any(|id| group.iter().any(|one| &one.id == id))
            {
                let name = guide_name(other);
                if !dependents.contains(&name) {
                    dependents.push(name);
                }
            }
        }
        out.push_str(&format!(
            "  {}\n\n",
            match (required, dependents.is_empty()) {
                // A required slot's own sentence outranks it: "the build
                // cannot be made at all" already covers everything that would
                // have gone on after it.
                (true, _) => words.without_required.clone(),
                (false, false) => fill(
                    &words.without_needed_by,
                    "dependents",
                    &dependents.join(", ")
                ),
                (false, true) => words.without_optional.clone(),
            }
        ));
    }

    out.push_str(&words.footer);
    out.push('\n');
    // Composed with `\n` and converted once, rather than threading `\r\n`
    // through every `push_str`: there is exactly one place to be wrong, and
    // nothing above ever writes a `\r`.
    Ok(out.replace('\n', "\r\n"))
}

/// What the guide calls a slot — **the same name the screen shows**.
///
/// `slots.ts::displayName`'s rule, and it exists for the ROM: the recipe names
/// that slot only *"Kickstart"* and states the major it needs in `identity`,
/// so the guide printing `slot.name` raw dropped the one actionable fact the
/// slot holds (M1). Two data values placed side by side, not a sentence.
fn guide_name(slot: &Slot) -> String {
    match slot.kind == SlotKind::Rom && !slot.identity.is_empty() {
        true => format!("{} {}", slot.name, slot.identity),
        false => slot.name.clone(),
    }
}

#[cfg(test)]
mod tests {
    // -----------------------------------------------------------------------
    // The drop-folder guide (design § 3.7)
    // -----------------------------------------------------------------------

    /// The frontend's own `parity.test.ts`, one folder over.
    ///
    /// A missing line in one language is a paragraph a Turkish reader simply
    /// does not get, and nothing about it fails to compile. `deny_unknown_fields`
    /// plus named fields makes both directions a parse error; this asserts
    /// the two remaining things a struct cannot: that nothing is blank, and
    /// that a sentence carrying a placeholder in one language still carries
    /// it in the other.
    #[test]
    fn guide_parity_holds_between_the_two_languages() {
        let en = guide_strings("en").unwrap();
        let tr = guide_strings("tr").unwrap();

        let pairs: Vec<(&str, &String, &String)> = vec![
            ("filename", &en.filename, &tr.filename),
            ("heading", &en.heading, &tr.heading),
            ("release", &en.release, &tr.release),
            ("intro", &en.intro, &tr.intro),
            ("noDownloads", &en.no_downloads, &tr.no_downloads),
            ("required", &en.required, &tr.required),
            ("optional", &en.optional, &tr.optional),
            ("expected", &en.expected, &tr.expected),
            (
                "filenamesUnknown",
                &en.filenames_unknown,
                &tr.filenames_unknown,
            ),
            ("provenance", &en.provenance, &tr.provenance),
            (
                "provenanceUnknown",
                &en.provenance_unknown,
                &tr.provenance_unknown,
            ),
            ("needsFirst", &en.needs_first, &tr.needs_first),
            ("needsNothing", &en.needs_nothing, &tr.needs_nothing),
            (
                "withoutRequired",
                &en.without_required,
                &tr.without_required,
            ),
            (
                "withoutOptional",
                &en.without_optional,
                &tr.without_optional,
            ),
            ("footer", &en.footer, &tr.footer),
        ];

        for (name, english, turkish) in &pairs {
            assert!(!english.trim().is_empty(), "{name} is blank in English");
            assert!(!turkish.trim().is_empty(), "{name} is blank in Turkish");
            for placeholder in ["{release}", "{filenames}", "{provenance}", "{needs}"] {
                assert_eq!(
                    english.contains(placeholder),
                    turkish.contains(placeholder),
                    "{name}: {placeholder} is in one language and not the other"
                );
            }
        }

        // Two files, two names. One name for both would put the Turkish text
        // under an English name — and, worse, make the SAFE_CREATE check
        // answer about the wrong file after a language switch.
        assert_ne!(en.filename, tr.filename);
        assert!(en.required != tr.required, "REQUIRED is not a Turkish word");
    }

    /// An unrecognised language answers rather than refusing: the button
    /// keeps working the day ART grows a third language and nobody has
    /// written this file for it yet.
    #[test]
    fn an_unknown_language_falls_back_to_english_rather_than_refusing() {
        assert_eq!(guide_strings("de").unwrap(), guide_strings("en").unwrap());
    }

    /// Every slot gets its own entry, and each entry says the five things
    /// somebody filling a folder needs: whether they have to have it, what it
    /// is called, where it comes from, what has to come first, and what
    /// happens without it.
    #[test]
    fn every_slot_gets_a_required_or_optional_entry_in_both_languages() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        assert!(slots.len() > 3, "3.9 has more than three slots");

        for language in ["en", "tr"] {
            let words = guide_strings(language).unwrap();
            let text = guide_text(&slots, "AmigaOS 3.9", language).unwrap();

            assert!(
                text.contains("AmigaOS 3.9"),
                "{language}: the release is not named"
            );
            for slot in &slots {
                // Every slot's own name appears — but not necessarily as a
                // heading of its own: slots sharing an artefact share one
                // entry (L11) and their names are joined into its heading.
                assert!(
                    text.contains(&guide_name(slot)),
                    "{language}: no entry names {}",
                    slot.id
                );
            }
            // Both tags are really used by 3.9's own slot list — a guide that
            // said REQUIRED about everything would satisfy the loop above.
            assert!(
                text.contains(&words.required),
                "{language}: nothing required"
            );
            assert!(
                text.contains(&words.optional),
                "{language}: nothing optional"
            );
            assert!(text.contains(&words.without_required));
            assert!(text.contains(&words.without_optional));
            // No URL, ever: ART downloads none of this (design § 4).
            assert!(!text.contains("http"), "{language}: the guide names a URL");
        }
    }

    /// The order line names a prerequisite the way the reader will see it in
    /// this same file — never by slot id, which is ART's own bookkeeping.
    /// This is fix round 1's F7 one document over.
    #[test]
    fn the_order_line_names_what_comes_first_by_name_not_by_slot_id() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let blocked = slots
            .iter()
            .find(|slot| !slot.requires.is_empty())
            .expect("3.9 has a package that requires another");
        let prerequisite = slots
            .iter()
            .find(|slot| slot.id == blocked.requires[0])
            .expect("a requires entry names a slot in the same list");
        let words = guide_strings("en").unwrap();
        let text = guide_text(&slots, "AmigaOS 3.9", "en").unwrap();

        assert!(
            text.contains(&fill(&words.needs_first, "needs", &prerequisite.name)),
            "the order line does not name {} by name:
{text}",
            prerequisite.name
        );
        assert!(
            !text.contains(&blocked.requires[0]),
            "a slot id reached the guide: {}",
            blocked.requires[0]
        );
        // And a slot with no prerequisite says so rather than nothing at all.
        assert!(text.contains(&words.needs_nothing));
    }

    /// The entry for one slot, as a reader sees it: its heading line and the
    /// indented lines under it, up to the blank line. Written for the fix
    /// round's three findings, all of which are *"the guide says a true-looking
    /// thing about the wrong slot"* and none of which a `contains` over the
    /// whole file could have caught.
    fn guide_entry(text: &str, heading_ends_with: &str) -> String {
        let mut out = String::new();
        let mut inside = false;
        for line in text.lines() {
            if inside {
                if line.trim().is_empty() {
                    break;
                }
                out.push_str(line);
                out.push('\n');
                continue;
            }
            if !line.starts_with(' ') && line.trim_end().ends_with(heading_ends_with) {
                inside = true;
                out.push_str(line);
                out.push('\n');
            }
        }
        assert!(
            !out.is_empty(),
            "no entry ending '{heading_ends_with}' in:\n{text}"
        );
        out
    }

    /// **M1.** A ROM slot is filled from the Kickstart the user chose in ART
    /// and from nothing else — `resolve_one` answers `MatchedBy::Chosen` for
    /// it at rank 2 and it carries no `filenames` for rank 3. The guide used
    /// to print the generic lines for it, so a file ART writes into somebody's
    /// folder, headed *"What goes in this folder"*, told them to put a
    /// Kickstart there and promised it would be "identified by its contents".
    /// Two claims, both false, on a person's own disk.
    #[test]
    fn the_rom_entry_says_it_is_not_a_file_for_this_folder_and_names_the_floor() {
        for language in ["en", "tr"] {
            let words = guide_strings(language).unwrap();
            let text =
                guide_text(&slots_for("AmigaOS 3.9").unwrap(), "AmigaOS 3.9", language).unwrap();
            // The heading carries the floor, exactly as the screen's own
            // `displayName` joins it — the recipe names the slot "Kickstart"
            // and states the major separately.
            let entry = guide_entry(&text, "Kickstart 40");

            assert!(
                entry.contains(&fill(&words.rom_not_in_this_folder, "major", "40")),
                "{language}: the ROM entry does not say where the Kickstart is chosen:\n{entry}"
            );
            // And it makes neither of the two false claims any more.
            assert!(
                !entry.contains(&words.filenames_unknown),
                "{language}: the ROM entry still promises identification by contents:\n{entry}"
            );
            assert!(
                !entry.contains(&words.provenance_unknown),
                "{language}: the ROM entry still answers a where-from question:\n{entry}"
            );
        }
    }

    /// A release stating no Kickstart floor gets the same instruction without
    /// a number — `"Kickstart  or newer"` is what interpolating an empty
    /// identity would have produced.
    #[test]
    fn a_rom_slot_with_no_floor_still_says_it_is_chosen_in_art() {
        let rom = Slot {
            id: "rom".into(),
            kind: SlotKind::Rom,
            name: "Kickstart".into(),
            identity: String::new(),
            artefact: None,
            required: false,
            filenames: Vec::new(),
            provenance: None,
            position: 0,
            requires: Vec::new(),
            superseded_by: Vec::new(),
            expects_directories: Vec::new(),
        };
        let words = guide_strings("en").unwrap();
        let text = guide_text(&[rom], "AmigaOS 3.9", "en").unwrap();

        assert!(
            text.contains(&words.rom_not_in_this_folder_no_floor),
            "{text}"
        );
        assert!(!text.contains("Kickstart  or newer"), "{text}");
    }

    /// **I15's own sentence, asserted** (fix round 1, m8).
    ///
    /// The finding was not that the data was empty — it was that the
    /// drop-folder guide printed *"Nothing has to be in place before it"*
    /// about a file whose whole purpose is to patch BoingBag 3.9-1. The
    /// data-level fix is pinned elsewhere; this pins the sentence the
    /// finding was about, in both languages, and asserts the false one is
    /// gone rather than merely that a true one appeared.
    #[test]
    fn the_uae_fix_says_what_it_goes_on_after_and_no_longer_says_nothing_does() {
        for language in ["en", "tr"] {
            let words = guide_strings(language).unwrap();
            let text =
                guide_text(&slots_for("AmigaOS 3.9").unwrap(), "AmigaOS 3.9", language).unwrap();
            let entry = guide_entry(&text, "BoingBag3.9-1-UAE");

            assert!(
                entry.contains(&fill(&words.needs_first, "needs", "BoingBag 3.9-1")),
                "{language}: the UAE fix must name the package it patches:\n{entry}"
            );
            assert!(
                !entry.contains(&words.needs_nothing),
                "{language}: the UAE fix still says nothing has to be in place first:\n{entry}"
            );
        }
    }

    /// **M2.** Every package slot is optional by construction — a package is a
    /// thing the user chooses — so BoingBag 3.9-1 was told "nothing else
    /// fails" four entries above this same file's own statement that BoingBag
    /// 3.9-2 goes on after it. The file contradicted itself, and the half a
    /// reader acts on ("I can skip this one") was the wrong half.
    #[test]
    fn a_slot_another_slot_needs_says_who_stops_working_without_it() {
        for language in ["en", "tr"] {
            let words = guide_strings(language).unwrap();
            let text =
                guide_text(&slots_for("AmigaOS 3.9").unwrap(), "AmigaOS 3.9", language).unwrap();
            let entry = guide_entry(&text, "BoingBag 3.9-1");

            assert!(
                entry.contains(&fill(
                    &words.without_needed_by,
                    "dependents",
                    "BoingBag3.9-1-UAE, BoingBag 3.9-2"
                )),
                "{language}: BoingBag 3.9-1 does not name what needs it:\n{entry}"
            );
            assert!(
                !entry.contains(&words.without_optional),
                "{language}: BoingBag 3.9-1 still says nothing else fails:\n{entry}"
            );
        }
        // The other arm, so this is not a rule that fires for everything: a
        // package nothing else requires still gets the plain optional line.
        let words = guide_strings("en").unwrap();
        let text = guide_text(&slots_for("AmigaOS 3.9").unwrap(), "AmigaOS 3.9", "en").unwrap();
        assert!(
            text.contains(&words.without_optional),
            "no slot got the plain optional sentence at all:\n{text}"
        );
    }

    /// **M3.** `MediaRow::source` is where the *dump* came from. On an adopted
    /// row that is a statement about the artefact; on one of ART's own rows it
    /// is `"the owner's copy, 2026-09-08"` — a date-stamped claim about this
    /// machine, rendered behind the words "where it comes from" in a file ART
    /// writes into a stranger's folder.
    #[test]
    fn the_guide_never_puts_this_machines_own_dump_note_behind_where_it_comes_from() {
        for language in ["en", "tr"] {
            let text =
                guide_text(&slots_for("AmigaOS 3.9").unwrap(), "AmigaOS 3.9", language).unwrap();
            assert!(
                !text.contains("the owner's copy"),
                "{language}: the owner's own dump note reached the guide:\n{text}"
            );
        }

        // Both arms, on the real shipped tables. An artefact an adopted row
        // names keeps its provenance; one only ART's own table names answers
        // "no note", because that is what is true.
        let rows = mediahash::rows().unwrap();
        assert_eq!(
            provenance_for(rows, Some("amigaos-39-cd")).as_deref(),
            Some("Haage and Partners (3.9)"),
            "an adopted row still says where the artefact comes from"
        );
        assert_eq!(
            provenance_for(rows, Some("boingbag-39-1-uae-fix")),
            None,
            "an own-table row is a note about this machine, not about the artefact"
        );
    }

    /// **L8.** A `.txt` a Windows user opens in whatever they have.
    #[test]
    fn the_guide_is_written_with_windows_line_endings() {
        let text = guide_text(&slots_for("AmigaOS 3.9").unwrap(), "AmigaOS 3.9", "en").unwrap();
        assert!(text.contains("\r\n"));
        assert_eq!(
            text.matches('\n').count(),
            text.matches("\r\n").count(),
            "a bare newline got through"
        );
    }

    /// **L11.** `locale-39` and `locale-39-turkish` genuinely share one
    /// archive — both declare `"media": "Locale3.9"` — so the guide listed
    /// `Locale3_9.lha` under two OPTIONAL headings and a person filling a
    /// folder went looking for two files.
    #[test]
    fn two_slots_reading_one_archive_get_one_guide_entry_naming_both() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let text = guide_text(&slots, "AmigaOS 3.9", "en").unwrap();

        assert_eq!(
            text.matches("Locale3_9.lha").count(),
            1,
            "one archive, one expected-names line:\n{text}"
        );
        // And neither package lost its name: one entry, both headings.
        let entry = guide_entry(&text, "Türkçe catalogs and fonts (Locale 3.9)");
        assert!(
            entry.contains("AmigaOS 3.9 Locale update"),
            "the other package sharing the archive is not named:\n{entry}"
        );

        // The other arm: a package with an artefact of its own still gets its
        // own entry, so this is a grouping and not a collapse.
        assert_eq!(text.matches("BoingBag39-1.lha").count(), 1, "{text}");
        // `\r\n`: the guide is CRLF (fix round 1, L8), so a heading asserted
        // with a bare newline would never match.
        assert!(text.contains("OPTIONAL  BoingBag 3.9-1\r\n"), "{text}");
    }

    // -----------------------------------------------------------------------
    // Overrides — the files the user picked by hand (round 2 review, M5)
    // -----------------------------------------------------------------------

    /// **M5.** An override is not a rank: it is the user telling ART what a
    /// file is. A hash find that beat it would be ART overruling a decision,
    /// and the readout would then say something different from the panel
    /// about the file the run is actually going to use.
    #[test]
    fn a_file_the_user_chose_wins_over_a_hash_find() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let mine = PathBuf::from("D:\\pkg\\my-own-copy.lha");
        let mut gathered = Gathered::empty();
        // A real rank-1 find for the same slot, so the override is beating
        // the strongest evidence ART has rather than an empty answer.
        gathered
            .hashes
            .push(hashed("E:\\material\\BoingBag39-1.lha", BB1_45_15_MD5));

        let without = resolve(&slots, &gathered.facts(None));
        let found = state_of(&without, "package:boingbag-39-1")
            .found
            .as_ref()
            .expect("the hash fills it");
        assert_eq!(found.matched_by, MatchedBy::Hash);
        assert_eq!(found.path, PathBuf::from("E:\\material\\BoingBag39-1.lha"));

        let chosen = [Override {
            slot: "package:boingbag-39-1",
            path: mine.as_path(),
            on_disk: true,
        }];
        let with = resolve(&slots, &gathered.facts_with(None, &chosen));
        let found = state_of(&with, "package:boingbag-39-1")
            .found
            .as_ref()
            .expect("the override fills it");
        assert_eq!(found.matched_by, MatchedBy::Chosen, "the user chose it");
        assert_eq!(found.path, mine);
        assert!(
            found.row.is_none(),
            "a chosen file carries no table row: ART identified nothing"
        );
    }

    /// A chosen file that has gone is its own ending here too — the same
    /// rule `ChosenRom::Absent` already keeps (F5), generalised.
    #[test]
    fn a_chosen_file_that_is_not_there_is_chosen_missing_and_not_found() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let gone = PathBuf::from("E:\\unplugged\\BoingBag39-1.lha");
        let chosen = [Override {
            slot: "package:boingbag-39-1",
            path: gone.as_path(),
            on_disk: false,
        }];
        let gathered = Gathered::empty();
        let states = resolve(&slots, &gathered.facts_with(None, &chosen));
        let state = state_of(&states, "package:boingbag-39-1");

        assert!(state.found.is_none());
        assert_eq!(state.chosen_missing.as_deref(), Some(gone.as_path()));
        assert!(state.candidates.is_empty());
    }

    /// An overlay override may name the package rather than the drawer — the
    /// screen that holds these has one *"update archive"* field per package,
    /// not one per overlay.
    #[test]
    fn an_overlay_override_may_name_the_package_it_belongs_to() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let overlay_id = slots
            .iter()
            .find(|slot| slot.kind == SlotKind::Overlay)
            .expect("3.9 declares one")
            .id
            .clone();
        assert!(overlay_id.starts_with("overlay:boingbag-39-1:"));

        let mine = PathBuf::from("D:\\pkg\\the-uae-fix.lha");
        let chosen = [Override {
            slot: "overlay:boingbag-39-1",
            path: mine.as_path(),
            on_disk: true,
        }];
        let gathered = Gathered::empty();
        let states = resolve(&slots, &gathered.facts_with(None, &chosen));
        assert_eq!(
            state_of(&states, &overlay_id)
                .found
                .as_ref()
                .map(|f| f.path.clone()),
            Some(mine)
        );
        // And it does not reach across to the package's own slot: a prefix
        // that matched anything starting with the package id would put the
        // update archive into the package field.
        assert!(state_of(&states, "package:boingbag-39-1").found.is_none());
    }

    // -----------------------------------------------------------------------
    // The structural check beside the hash (design § 3.6, review L6)
    // -----------------------------------------------------------------------

    /// **L6.** A disc whose bytes ART recognises but whose root is missing
    /// one of the directories the artefact map records is a re-master or a
    /// partial copy — *matched but incomplete*, which is a different next
    /// step from *not found*.
    #[test]
    fn a_disc_missing_a_directory_the_artefact_expects_is_incomplete_not_absent() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let medium = state_of_slot(&slots, "medium:AmigaOS3.9");
        assert!(
            !medium.expects_directories.is_empty(),
            "the artefact map records directories for the 3.9 CD"
        );

        let disc = PathBuf::from("E:\\material\\AmigaOS39.iso");
        let mut gathered = Gathered::empty();
        gathered
            .hashes
            .push(hashed("E:\\material\\AmigaOS39.iso", CD_ADOPTED_MD5));

        // Every expected directory but one.
        let mut roots: Vec<String> = medium.expects_directories.clone();
        let missing = roots.pop().expect("at least one");
        gathered.disc_roots.push((disc, roots));

        let states = resolve(&slots, &gathered.facts(None));
        let state = state_of(&states, "medium:AmigaOS3.9");
        assert!(
            state.found.is_some(),
            "the disc is still found — this is not a not-found row"
        );
        assert_eq!(state.incomplete.as_deref(), Some(missing.as_str()));
    }

    /// Both other arms, because *"ART did not look"* is not *"ART looked and
    /// it is all there"*: a complete disc and a disc nobody listed both
    /// answer `None`, and only the first of those is a claim.
    #[test]
    fn a_complete_disc_and_an_unlisted_one_both_report_nothing_missing() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let expected = state_of_slot(&slots, "medium:AmigaOS3.9")
            .expects_directories
            .clone();
        let disc = PathBuf::from("E:\\material\\AmigaOS39.iso");

        let mut complete = Gathered::empty();
        complete
            .hashes
            .push(hashed("E:\\material\\AmigaOS39.iso", CD_ADOPTED_MD5));
        complete.disc_roots.push((disc, expected));
        assert_eq!(
            state_of(&resolve(&slots, &complete.facts(None)), "medium:AmigaOS3.9").incomplete,
            None
        );

        let mut unlisted = Gathered::empty();
        unlisted
            .hashes
            .push(hashed("E:\\material\\AmigaOS39.iso", CD_ADOPTED_MD5));
        assert_eq!(
            state_of(&resolve(&slots, &unlisted.facts(None)), "medium:AmigaOS3.9").incomplete,
            None,
            "a disc nobody listed the root of says nothing, never 'incomplete'"
        );
    }

    /// A slot ART has no recorded file name for says that, instead of
    /// printing "Expected ." at somebody — the sentence a reader cannot act
    /// on, which is the same choice `slotLines` makes on screen.
    #[test]
    fn a_slot_with_no_recorded_names_says_so_rather_than_expecting_nothing() {
        let bare = Slot {
            id: "package:nameless".into(),
            kind: SlotKind::Package,
            name: "Nameless".into(),
            identity: "Nameless".into(),
            artefact: None,
            required: false,
            filenames: Vec::new(),
            provenance: None,
            position: 0,
            requires: Vec::new(),
            superseded_by: Vec::new(),
            expects_directories: Vec::new(),
        };
        let words = guide_strings("en").unwrap();
        let text = guide_text(&[bare], "AmigaOS 3.9", "en").unwrap();

        assert!(text.contains(&words.filenames_unknown), "{text}");
        assert!(text.contains(&words.provenance_unknown), "{text}");
        assert!(
            !text.contains(
                "Expected file names: 
"
            ),
            "{text}"
        );
    }

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
                "package:locale-39",
                "package:locale-39-turkish",
                "package:euro-update",
                "package:boingbag-39-2",
                "package:locale-turkish",
                "package:boingbag-39-2-contribution",
                "package:boingbags-39-3-4",
                "rom",
            ]
        );
        assert_eq!(
            slots.iter().map(|s| s.position).collect::<Vec<_>>(),
            (0..11).collect::<Vec<u32>>()
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
        // **An overlay names the package whose drawer it patches** (round 2
        // whole-branch review, I15). It shipped with an empty list, and the
        // drop-folder guide therefore said "Nothing has to be in place
        // before it" about a file that patches BoingBag 3.9-1 and is useless
        // without it. What keeps the direction right is not an empty list
        // but `requirement_met`: the requirement is satisfied by the
        // package's archive being **found**, never by its being installed —
        // see `an_overlay_waits_for_its_packages_archive_not_for_its_install`.
        let overlay = slots.iter().find(|s| s.kind == SlotKind::Overlay).unwrap();
        assert_eq!(
            overlay.requires,
            vec!["package:boingbag-39-1".to_string()],
            "the UAE fix patches BoingBag 3.9-1's own drawer"
        );
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
        disc_roots: Vec<(PathBuf, Vec<String>)>,
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
                disc_roots: Vec::new(),
            }
        }

        fn facts<'a>(&'a self, manifest: Option<&'a DistributionManifest>) -> Facts<'a> {
            self.facts_with(manifest, &[])
        }

        /// The same, plus the files the user picked by hand.
        ///
        /// A separate method rather than a field, because an `Override`
        /// borrows its slot id and path: owning them on `Gathered` and
        /// handing out a `Vec` built inside `facts()` would be a reference
        /// into a temporary. The caller owns the array; the borrow is theirs.
        fn facts_with<'a>(
            &'a self,
            manifest: Option<&'a DistributionManifest>,
            overrides: &'a [Override<'a>],
        ) -> Facts<'a> {
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
                overrides,
                disc_roots: &self.disc_roots,
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
        // Eight packages, one of them found — and the UAE overlay, which is
        // present but measured as unnecessary, is in neither total.
        assert_eq!((summary.optional_total, summary.optional_found), (8, 1));
    }

    /// **An overlay waits for its package's *archive*, never for its
    /// install** (round 2 whole-branch review, I15).
    ///
    /// Three arms, because the rule has three answers and only the middle
    /// one is new. The overlay slot is given no requirement satisfaction of
    /// its own in any of them; what changes is BoingBag 3.9-1's state.
    #[test]
    fn an_overlay_waits_for_its_packages_archive_not_for_its_install() {
        let slots = slots_for("AmigaOS 3.9").unwrap();
        let overlay_id = "overlay:boingbag-39-1:BoingBag3.9-1-UAE";
        let blocked = |states: &[SlotState]| {
            states
                .iter()
                .find(|state| state.slot.id == overlay_id)
                .expect("the UAE overlay is one of 3.9's slots")
                .blocked_by
                .clone()
        };

        // 1 — nothing in the folders and no tree: the overlay waits, and it
        // names what it waits for rather than reading ready.
        let nothing = Gathered::empty();
        assert_eq!(
            blocked(&resolve(&slots, &nothing.facts(None))),
            vec!["package:boingbag-39-1".to_string()],
            "with no BoingBag 3.9-1 anywhere, the fix patches nothing"
        );

        // 2 — the archive is in the folder and nothing is installed. This is
        // the arm the old empty `requires` could not tell from arm 1 and the
        // new rule could get backwards: the fix is used *during* BoingBag
        // 3.9-1's run, so an archive in hand is all it is waiting for.
        //
        // Matched at rank 2 by the archive's own top-level directory, so the
        // package slot is `found` while the manifest still says nothing.
        let mut in_folder = Gathered::empty();
        in_folder
            .packages
            .push(archive("D:/a/BoingBag39-1.lha", "BoingBag3.9-1"));
        assert!(
            blocked(&resolve(&slots, &in_folder.facts(None))).is_empty(),
            "an overlay must not wait for its package to be installed — that is backwards"
        );

        // 3 — installed and the archive gone. Still not waiting: a package
        // already on the tree is not one anything is waiting for.
        let done = manifest_with(vec![], vec![], vec!["boingbag-39-1"]);
        assert!(
            blocked(&resolve(&slots, &nothing.facts(Some(&done)))).is_empty(),
            "a requirement that has already happened cannot still be blocking"
        );

        // And the control: a **package** requirement is not softened by the
        // same fact. BoingBag 3.9-2 with BoingBag 3.9-1's archive merely
        // sitting in a folder is exactly the run ART-186 refuses.
        let bb2 = resolve(&slots, &in_folder.facts(None))
            .into_iter()
            .find(|state| state.slot.id == "package:boingbag-39-2")
            .expect("BoingBag 3.9-2 is one of 3.9's slots");
        assert!(
            bb2.blocked_by
                .contains(&"package:boingbag-39-1".to_string()),
            "a package waits for the package before it to be installed: {:?}",
            bb2.blocked_by
        );
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
