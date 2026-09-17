//! The Kickstarts WHDLoad titles want: what every `.slave` under a prepared
//! partition asks for, a proposal naming each image's `.RTB` source, and
//! placing **only** the names the user agreed to (design § 5 phase 6, P4).
//!
//! # Always a proposal, never a silent copy
//!
//! `core::rom::offer` and `core::rom::place` already carry the owner's
//! 2026-08-21 rule for a *single* image, matched by a title's slave against
//! a folder the user points at. This module is the card build's own use of
//! that rule, scaled up: every title under every prepared partition, matched
//! against the whole Kickstart collection, and a `.RTB` found alongside each
//! one — but still only *offered*. [`place_agreed`] is the one function here
//! that writes anything, and it takes the exact names the caller agreed to,
//! never a title and "do the right thing".
//!
//! # Three steps
//!
//! 1. [`find_title_needs`] walks every prepared partition root for `.slave`
//!    files and reads what each one asks for (`core::gameindex::readers::slave`).
//! 2. [`propose_kickstarts`] turns that into one item per distinct wanted
//!    name — which titles ask for it, whether the collection already
//!    supplies it ([`core::rom::offer`]), and where its `.RTB` would come
//!    from.
//! 3. [`place_agreed`] writes exactly the agreed names: the decoded image
//!    and its `.RTB`, both under `Devs/Kickstarts/` ([`core::rom::place`]).
//!    A name outside the proposal — never offered, offered but not
//!    `Supplied`, or `Supplied` with no `.RTB` — is refused before anything
//!    is written, for the whole batch, not just the offending name.
//!
//! # The `.RTB`, and Aminet's `skick346`
//!
//! A WHDLoad Kickstart image is written **decoded** (`core::rom::place`), and
//! WHDLoad also wants a `.RTB` beside it — the resource file `Show Kickstart`
//! and friends ship inside `skick346.lha` (Aminet `util/boot/skick346`,
//! already in ART's own package catalogue). ART never downloads it during a
//! build (owner decision 2): the proposal looks for it in the material the
//! user already has and says which Aminet package to fetch when none is
//! there ([`KickstartProposal::rtb_missing`]).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::core::archive;
use crate::core::error::{CoreError, CoreResult};
use crate::core::gameindex::readers::slave::read_slave;
use crate::core::gameindex::record::KickstartNeed;
use crate::core::jobs::{cancelled_error, ProgressSink};
use crate::core::rom::offer::{offer_for, Offer, WantedImage};
use crate::core::rom::place::{place, PlaceOutcome, Placement, KICKSTART_DRAWER};
use crate::core::rom::scan_rom_directory;
use crate::core::safety::atomic_write;
use crate::core::security::safe_join;
use crate::core::whdload::has_extension;

/// The most of a `.slave` file `find_title_needs` will read into memory. A
/// real one is a few KB of 68000 code and a short header; a hostile file
/// under that extension is not let past this.
pub const SLAVE_MAX_BYTES: u64 = 1024 * 1024;

/// The most of a candidate `.RTB` file `propose_kickstarts`/`place_agreed`
/// will read — a short WHDLoad resource file, never anywhere near this large
/// in practice.
pub const RTB_MAX_BYTES: u64 = 64 * 1024;

/// How many files and folders [`find_title_needs`] will walk under one root
/// before calling it a loop. A real system tree's `Games`/`Stuff` partitions
/// hold at most a few thousand.
pub const MAX_WALK_NODES: usize = 200_000;

/// A slave's declared need, as the images it will accept.
///
/// **The list wins when there is one.** `KickstartNeed::image` is the first of
/// `alternatives` when that is non-empty, so reading both would ask for the
/// same image twice — and `crc16` is `None` in exactly that case, because the
/// `$ffff` sentinel is how a slave says "the name field is a list" rather than
/// a checksum ([ART-137](../../../../docs/ISSUES.md)).
///
/// Moved here from `commands/gameindex.rs` (ART round 3, task 9): matching a
/// title's declared need against the ROM collection is card-build machinery,
/// not a thin command adapter, and `find_title_needs` needs it directly
/// rather than through the command layer.
pub fn wanted_images(need: &KickstartNeed) -> Vec<WantedImage> {
    if !need.alternatives.is_empty() {
        return need
            .alternatives
            .iter()
            .map(|alt| WantedImage {
                name: alt.image.clone(),
                crc16: Some(alt.crc16),
                size: need.size,
            })
            .collect();
    }
    match &need.image {
        Some(image) => vec![WantedImage {
            name: image.clone(),
            crc16: need.crc16,
            size: need.size,
        }],
        None => Vec::new(),
    }
}

/// Every title's Kickstart need, read from the `.slave` files under a set of
/// prepared partition roots.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TitleNeeds {
    /// Slave path (relative to its root, `/`-separated) → the images it will
    /// accept, in the slave's own order. A slave that asks for nothing is
    /// not recorded here at all.
    pub titles: Vec<(String, Vec<WantedImage>)>,
    /// Slaves ART could not read, named — never dropped silently.
    pub unreadable: Vec<String>,
    /// Every `.slave` file found under the roots, readable or not, whether it
    /// asks for a Kickstart or not. Each one is a title that needs WHDLoad —
    /// most titles declare no Kickstart at all, so `titles` alone cannot say
    /// whether the card needs WHDLoad (P6).
    pub slaves: usize,
}

/// Walk each prepared partition root on the host for `.slave` files
/// (bounded), and read what each one asks for.
///
/// A root that is not a directory is skipped (nothing to walk); each root is
/// walked iteratively, with its own node counter, so a symlink loop or a
/// pathological tree hits [`CoreError::LimitExceeded`] naming the root rather
/// than recursing forever. `.uaem` sidecars — WHDLoad slave metadata, never a
/// slave themselves — are skipped explicitly rather than relied on to miss
/// `has_extension`'s check by chance.
pub fn find_title_needs(roots: &[PathBuf], progress: &dyn ProgressSink) -> CoreResult<TitleNeeds> {
    progress.report(0, None, "Looking for WHDLoad titles…");

    let mut candidates: Vec<(PathBuf, String)> = Vec::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        let mut stack: Vec<PathBuf> = vec![root.clone()];
        let mut nodes: usize = 0;
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                nodes += 1;
                if nodes > MAX_WALK_NODES {
                    return Err(CoreError::LimitExceeded {
                        subject: "kickstart title walk".into(),
                        detail: format!(
                            "'{}' holds more than {MAX_WALK_NODES} files and folders; ART \
                             stopped looking there for .slave files",
                            root.display()
                        ),
                    });
                }
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if name.to_ascii_lowercase().ends_with(".uaem") {
                    continue;
                }
                if !has_extension(name, "slave") {
                    continue;
                }
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(path.as_path())
                    .to_string_lossy()
                    .replace('\\', "/");
                candidates.push((path, relative));
            }
        }
    }

    let total = candidates.len() as u64;
    let slaves = candidates.len();
    let mut titles = Vec::new();
    let mut unreadable = Vec::new();
    let mut done: u64 = 0;

    for (path, relative) in candidates {
        if progress.is_cancelled() {
            return Err(cancelled_error());
        }
        done += 1;
        progress.report(done, Some(total), &relative);

        match read_bounded(&path, SLAVE_MAX_BYTES, "WHDLoad slave") {
            Ok(bytes) => match read_slave(&bytes) {
                Ok(facts) => {
                    let wanted = wanted_images(&facts.kickstart);
                    if !wanted.is_empty() {
                        titles.push((relative, wanted));
                    }
                }
                Err(_) => unreadable.push(relative),
            },
            Err(_) => unreadable.push(relative),
        }
    }

    Ok(TitleNeeds {
        titles,
        unreadable,
        slaves,
    })
}

/// Where a Kickstart's `.RTB` would come from, once agreed to.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum RtbSource {
    /// A loose `<name>.RTB` at a material folder's top level, in its
    /// `Kickstarts/`, or in its `Devs/Kickstarts/`.
    Loose { path: PathBuf },
    /// A member of an archive whose name starts `skick` (Aminet's own
    /// `skick346.lha`), found by name rather than a fixed member list.
    InArchive { archive: PathBuf, member: String },
    /// Not found anywhere ART looked. `get_from` names the package that
    /// carries this `.RTB` (owner decision 2), and
    /// `KickstartProposal::rtb_missing` says that some item is missing one.
    Missing { get_from: RtbPackage },
}

/// Where a missing `.RTB` can be had — not every one is in `skick346`
/// (card round 3, M16; research-whdload.md § 1.3: `skick346.lha` carries no
/// `kick31034.A1000.RTB`, which ships in whdload.de's `7CitiesOfGold.lha`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RtbPackage {
    /// Aminet `util/boot/skick346`.
    AminetSkick346,
    /// whdload.de's WHDLoad install `7CitiesOfGold.lha`.
    WhdloadSevenCitiesOfGold,
}

/// The package that carries `name`'s `.RTB`: the A1000's 1.1 image from
/// whdload.de, every other from `skick346` — whose listing (7-Zip, round 3's
/// research) holds the 1.2, 1.3, 3.1 A600/A1200/A4000 `.RTB` files and 14
/// more for beta, 2.x and 3.0 Kickstarts.
pub fn rtb_package_for(name: &str) -> RtbPackage {
    if name.eq_ignore_ascii_case("kick31034.A1000") {
        RtbPackage::WhdloadSevenCitiesOfGold
    } else {
        RtbPackage::AminetSkick346
    }
}

/// One Kickstart name at least one title asks for, and what ART can say
/// about it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposedKickstart {
    /// The name WHDLoad looks for, `kick40068.A1200` — the key the build's
    /// agreement uses.
    pub name: String,
    /// The titles that name it (at most 20), in first-seen order.
    pub titles: Vec<String>,
    /// How many more titles name it, past the 20 listed.
    pub titles_more: usize,
    pub offer: Offer,
    pub rtb: RtbSource,
}

/// Every Kickstart at least one title on the card asks for, proposed —
/// never placed.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KickstartProposal {
    pub items: Vec<ProposedKickstart>,
    /// `TitleNeeds::unreadable`, carried through so one screen shows both
    /// halves of what ART could not fully account for.
    pub unreadable_slaves: Vec<String>,
    /// True when some item's `.RTB` is [`RtbSource::Missing`]: the screen
    /// names Aminet `util/boot/skick346` (owner decision 2).
    pub rtb_missing: bool,
}

/// One item per distinct wanted name, in first-seen order. `collection_dirs`
/// are scanned with [`scan_rom_directory`]; a folder that is not a directory
/// is skipped.
pub fn propose_kickstarts(
    needs: &TitleNeeds,
    collection_dirs: &[PathBuf],
    material: &[PathBuf],
) -> CoreResult<KickstartProposal> {
    let mut collection = Vec::new();
    for dir in collection_dirs {
        if dir.is_dir() {
            collection.extend(scan_rom_directory(dir)?);
        }
    }

    let mut order: Vec<String> = Vec::new();
    let mut wanted_by_name: HashMap<String, WantedImage> = HashMap::new();
    let mut titles_by_name: HashMap<String, Vec<String>> = HashMap::new();

    for (slave_path, images) in &needs.titles {
        for image in images {
            wanted_by_name.entry(image.name.clone()).or_insert_with(|| {
                order.push(image.name.clone());
                image.clone()
            });
            titles_by_name
                .entry(image.name.clone())
                .or_default()
                .push(slave_path.clone());
        }
    }

    let mut items = Vec::with_capacity(order.len());
    let mut rtb_missing = false;
    for name in order {
        let wanted = wanted_by_name
            .remove(&name)
            .expect("every name in `order` was just inserted alongside its image");
        let offer = offer_for(std::slice::from_ref(&wanted), &collection)
            .into_iter()
            .next()
            .expect("one wanted image always produces exactly one offer");
        let rtb = find_rtb(&name, material);
        if matches!(rtb, RtbSource::Missing { .. }) {
            rtb_missing = true;
        }
        let mut titles = titles_by_name.remove(&name).unwrap_or_default();
        let titles_more = titles.len().saturating_sub(20);
        titles.truncate(20);
        items.push(ProposedKickstart {
            name,
            titles,
            titles_more,
            offer,
            rtb,
        });
    }

    Ok(KickstartProposal {
        items,
        unreadable_slaves: needs.unreadable.clone(),
        rtb_missing,
    })
}

/// Where `name`'s `.RTB` would come from, searched across `material` in
/// order: a loose file first (top level, `Kickstarts/`, `Devs/Kickstarts/`,
/// case-insensitive listing, each material folder in turn), then a
/// `skick*` archive's member ending `/<name>.rtb` (case-insensitive).
fn find_rtb(name: &str, material: &[PathBuf]) -> RtbSource {
    let rtb_name = format!("{name}.RTB");
    for folder in material {
        for candidate_dir in [
            folder.clone(),
            folder.join("Kickstarts"),
            folder.join("Devs").join("Kickstarts"),
        ] {
            if let Some(path) = find_named_file(&candidate_dir, &rtb_name) {
                return RtbSource::Loose { path };
            }
        }
    }

    let suffix = format!("/{}.rtb", name.to_ascii_lowercase());
    let bare = format!("{}.rtb", name.to_ascii_lowercase());
    for folder in material {
        let Ok(entries) = std::fs::read_dir(folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(fname) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !fname.to_ascii_lowercase().starts_with("skick") {
                continue;
            }
            let Ok(mut backend) = archive::open(&path) else {
                continue;
            };
            let Ok(archive_entries) = backend.entries() else {
                continue;
            };
            if let Some(hit) = archive_entries.iter().find(|e| {
                let normalised = e.name.replace('\\', "/").to_ascii_lowercase();
                normalised.ends_with(&suffix) || normalised == bare
            }) {
                return RtbSource::InArchive {
                    archive: path,
                    member: hit.name.replace('\\', "/"),
                };
            }
        }
    }

    RtbSource::Missing {
        get_from: rtb_package_for(name),
    }
}

/// What [`place_agreed`] did with one agreed Kickstart.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacedKickstart {
    pub name: String,
    pub image: PlaceOutcome,
    pub rtb: PlaceOutcome,
}

/// Place exactly the `agreed` names, with their `.RTB`, under
/// `Devs/Kickstarts/` in `tree`.
///
/// **Every agreed name is validated first**, and the whole batch is refused
/// before anything is written if any one of them is not something the
/// proposal actually offers: a name `proposal.items` does not carry, an item
/// whose [`Offer`] is not [`Offer::Supplied`], or a `Supplied` item whose
/// `.RTB` is [`RtbSource::Missing`]. This is the owner's rule made
/// mechanical — a caller cannot place a name the user never saw offered, and
/// cannot place half a batch while the other half silently fails partway.
pub fn place_agreed(
    proposal: &KickstartProposal,
    agreed: &[String],
    tree: &Path,
) -> CoreResult<Vec<PlacedKickstart>> {
    let validated = check_agreed(proposal, agreed)?;

    let mut placed = Vec::with_capacity(validated.len());
    for (item, by) in validated {
        let image = place(&Placement {
            from: PathBuf::from(&by.path),
            as_name: item.name.clone(),
            tree: tree.to_path_buf(),
        })?;
        let rtb_bytes = read_rtb_bytes(&item.rtb)?;
        let rtb = place_rtb(tree, &item.name, &rtb_bytes)?;
        placed.push(PlacedKickstart {
            name: item.name.clone(),
            image,
            rtb,
        });
    }
    Ok(placed)
}

/// [`place_agreed`]'s refusal, on its own and writing nothing: every agreed
/// name must be one the proposal supplies with its `.RTB`. The card build
/// asks this before anything of the build is written, so a refusal is a
/// refusal and not a failure halfway (card round 3, I3).
pub fn check_agreed<'a>(
    proposal: &'a KickstartProposal,
    agreed: &[String],
) -> CoreResult<
    Vec<(
        &'a ProposedKickstart,
        &'a crate::core::rom::offer::SuppliedBy,
    )>,
> {
    let mut validated = Vec::with_capacity(agreed.len());
    for name in agreed {
        let item = proposal
            .items
            .iter()
            .find(|item| &item.name == name)
            .ok_or_else(|| CoreError::KickstartNotProposed {
                name: name.clone(),
                why: "ART's proposal does not name it".into(),
            })?;
        let Offer::Supplied { by, .. } = &item.offer else {
            return Err(CoreError::KickstartNotProposed {
                name: name.clone(),
                why: "ART did not offer it — nothing in the collection matches it".into(),
            });
        };
        recheck_source(name, item.offer.wanted(), by)?;
        if let RtbSource::Missing { get_from } = item.rtb {
            let package = match get_from {
                RtbPackage::AminetSkick346 => "Aminet util/boot/skick346",
                RtbPackage::WhdloadSevenCitiesOfGold => "whdload.de's 7CitiesOfGold.lha",
            };
            return Err(CoreError::KickstartNotProposed {
                name: name.clone(),
                why: format!(
                    "its .RTB was not found in any material folder — put {package} in one"
                ),
            });
        }
        validated.push((item, by));
    }
    Ok(validated)
}

/// The agreed file is still the offered one (card round 3, the ROM half of
/// ART-343): read again, and its WHDLoad CRC-16 — and its size, when the
/// offer stated one — compared with what the proposal offered. Read the way
/// the proposal read it (`identify_rom`: bounded, an Amiga Forever ROM
/// decoded with its key), so the same file gives the same answer.
fn recheck_source(
    name: &str,
    wanted: &WantedImage,
    by: &crate::core::rom::offer::SuppliedBy,
) -> CoreResult<()> {
    let path = Path::new(&by.path);
    let changed = |why: String| CoreError::KickstartSourceChanged {
        name: name.to_string(),
        path: by.path.clone(),
        why,
    };
    let info = match crate::core::rom::identify_rom(path) {
        Ok(info) => info,
        Err(_) if !path.exists() => return Err(changed("is no longer there".into())),
        Err(err) => return Err(changed(format!("can no longer be read ({err})"))),
    };
    if let Some(offered) = wanted.crc16 {
        match info.whdload_crc16 {
            Some(now) if now == offered => {}
            Some(now) => {
                return Err(changed(format!(
                    "has changed: its WHDLoad checksum is ${now:04X}, and the proposal offered \
                     ${offered:04X}"
                )))
            }
            None => {
                return Err(changed(format!(
                    "can no longer be checked: ART cannot compute its WHDLoad checksum (an \
                     encrypted Amiga Forever ROM without its rom.key beside it), and the proposal \
                     offered ${offered:04X}"
                )))
            }
        }
    }
    // The size the offer stated: the file's own when it differed from the
    // slave's, the slave's otherwise.
    if let Some(stated) = wanted.size {
        let offered = u64::from(by.size_disagrees.unwrap_or(stated));
        let now = info.size_bytes as u64;
        if now != offered {
            return Err(changed(format!(
                "has changed: it is {now} bytes, and the proposal offered {offered}"
            )));
        }
    }
    Ok(())
}

/// The `.RTB` bytes a validated [`RtbSource`] names. Called only after
/// [`place_agreed`]'s own validation has ruled out [`RtbSource::Missing`].
fn read_rtb_bytes(source: &RtbSource) -> CoreResult<Vec<u8>> {
    match source {
        RtbSource::Loose { path } => read_bounded(path, RTB_MAX_BYTES, "Kickstart .RTB"),
        RtbSource::InArchive {
            archive: path,
            member,
        } => {
            let mut backend = archive::open(path)?;
            let entries = backend.entries()?;
            let index = entries
                .iter()
                .position(|e| e.name.replace('\\', "/") == *member)
                .ok_or_else(|| CoreError::Malformed {
                    format: "kickstart .RTB".into(),
                    detail: format!("'{member}' is no longer in '{}'", path.display()),
                })?;
            backend.read(index, RTB_MAX_BYTES)
        }
        RtbSource::Missing { .. } => {
            unreachable!("place_agreed refuses a Missing .RTB before this is ever called")
        }
    }
}

/// Write `bytes` as `<name>.RTB` in `tree`'s `Devs/Kickstarts/` — the same
/// drawer and the same three endings [`core::rom::place::place`] uses for
/// the image itself, but over the `.RTB`'s own raw bytes rather than a
/// decoded ROM image, so it is its own small function rather than a call
/// through `place` with a `from` that names nothing.
fn place_rtb(tree: &Path, name: &str, bytes: &[u8]) -> CoreResult<PlaceOutcome> {
    let as_name = format!("{name}.RTB");
    let drawer = KICKSTART_DRAWER
        .iter()
        .fold(tree.to_path_buf(), |at, part| at.join(part));
    let to = safe_join(&drawer, &as_name).map_err(|err| {
        CoreError::SafetyRefused(format!(
            "'{as_name}' is not a name a Kickstart's .RTB can be written under: {err}"
        ))
    })?;

    if to.exists() {
        // By length first, and read only when the lengths agree — a bounded
        // read of a file already in the tree (card round 3, M13).
        let same = std::fs::metadata(&to)?.len() == bytes.len() as u64
            && read_bounded(&to, bytes.len() as u64, "Kickstart .RTB")? == bytes;
        if same {
            return Ok(PlaceOutcome::AlreadyThere {
                to: to.display().to_string(),
            });
        }
        return Ok(PlaceOutcome::Occupied {
            to: to.display().to_string(),
        });
    }

    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write(&to, bytes)?;
    Ok(PlaceOutcome::Placed {
        to: to.display().to_string(),
        bytes: bytes.len(),
    })
}

/// A file whose name matches `name` case-insensitively, directly under
/// `dir` — `None` for a `dir` that does not exist or holds no such file,
/// never an error: most folders simply do not carry one.
fn find_named_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    entries.flatten().map(|entry| entry.path()).find(|path| {
        path.is_file()
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|found| found.eq_ignore_ascii_case(name))
    })
}

fn read_bounded(path: &Path, cap: u64, subject: &str) -> CoreResult<Vec<u8>> {
    let len = std::fs::metadata(path)?.len();
    if len > cap {
        return Err(CoreError::LimitExceeded {
            subject: subject.into(),
            detail: format!(
                "'{}' is {len} bytes, more than the {cap}-byte cap ART reads for it",
                path.display()
            ),
        });
    }
    Ok(std::fs::read(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gameindex::readers::slave::tests_support::slave_needing;
    use crate::core::gameindex::record::KickstartAlternative;
    use crate::core::rom::offer::SuppliedBy;

    fn scratch(tag: &str) -> (crate::core::ScratchDir, PathBuf) {
        crate::core::ScratchDir::pair("art-cardos-kickstarts", tag)
    }

    fn write_lha(path: &Path, files: &[(&str, &[u8])]) {
        std::fs::write(path, crate::core::lha::tests::make_lha_with(files)).unwrap();
    }

    // -- wanted_images (moved from commands/gameindex.rs) --------------------

    fn need() -> KickstartNeed {
        KickstartNeed {
            image: None,
            size: None,
            crc16: None,
            rom_version: None,
            alternatives: Vec::new(),
        }
    }

    /// **The list wins when there is one**, and reading both fields would ask
    /// for the same image twice: `KickstartNeed::image` is documented as the
    /// first of `alternatives` when those exist — the ART-137 case.
    #[test]
    fn the_moved_mapping_reads_a_list_before_the_single_image() {
        let asked = wanted_images(&KickstartNeed {
            image: Some("kick40063.A600".into()),
            size: Some(524_288),
            crc16: None,
            alternatives: vec![
                KickstartAlternative {
                    image: "kick40068.A1200".into(),
                    crc16: 0x9ff5,
                },
                KickstartAlternative {
                    image: "kick40068.A4000".into(),
                    crc16: 0x75d3,
                },
            ],
            ..need()
        });
        assert_eq!(
            asked.iter().map(|w| w.name.as_str()).collect::<Vec<_>>(),
            vec!["kick40068.A1200", "kick40068.A4000"],
            "the list wins, not the single-name field"
        );
        assert!(asked.iter().all(|w| w.size == Some(524_288)));
    }

    #[test]
    fn a_slave_naming_one_image_asks_for_one() {
        let asked = wanted_images(&KickstartNeed {
            image: Some("kick34005.A500".into()),
            crc16: Some(0xABCD),
            size: Some(262_144),
            ..need()
        });
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0].name, "kick34005.A500");
        assert_eq!(asked[0].crc16, Some(0xABCD));
    }

    /// A great many titles declare no Kickstart at all. That is an empty
    /// answer, not a problem with the title.
    #[test]
    fn a_slave_naming_nothing_asks_for_nothing() {
        assert!(wanted_images(&need()).is_empty());
    }

    // -- find_title_needs ------------------------------------------------

    #[test]
    fn every_slave_under_the_roots_is_read_and_an_unreadable_one_is_named() {
        let (_guard, dir) = scratch("needs");
        let games = dir.join("Games");
        std::fs::create_dir_all(games.join("Turrican")).unwrap();
        std::fs::write(
            games.join("Turrican").join("Turrican.slave"),
            slave_needing("kick34005.A500", 0xf20b, 262_144),
        )
        .unwrap();
        std::fs::create_dir_all(games.join("Broken")).unwrap();
        std::fs::write(games.join("Broken").join("Broken.Slave"), b"not a slave").unwrap();

        let needs =
            find_title_needs(std::slice::from_ref(&dir), &crate::core::jobs::NoProgress).unwrap();

        assert_eq!(needs.titles.len(), 1);
        assert_eq!(needs.titles[0].0, "Games/Turrican/Turrican.slave");
        assert_eq!(needs.titles[0].1[0].name, "kick34005.A500");
        assert_eq!(
            needs.unreadable,
            vec!["Games/Broken/Broken.Slave".to_string()]
        );
        assert_eq!(needs.slaves, 2, "the unreadable slave is a title too");
    }

    /// A slave that asks for nothing is not recorded — an empty need is not
    /// a title ART failed to read.
    #[test]
    fn a_slave_naming_no_kickstart_is_not_recorded() {
        let (_guard, dir) = scratch("no-need");
        let games = dir.join("Games");
        std::fs::create_dir_all(&games).unwrap();
        std::fs::write(
            games.join("Silent.slave"),
            crate::core::gameindex::readers::slave::tests_support::build_slave(
                "Silent", "2026 ART", 13,
            ),
        )
        .unwrap();

        let needs =
            find_title_needs(std::slice::from_ref(&dir), &crate::core::jobs::NoProgress).unwrap();
        assert!(needs.titles.is_empty(), "{:?}", needs.titles);
        assert!(needs.unreadable.is_empty());
        assert_eq!(
            needs.slaves, 1,
            "a title asking for no Kickstart still needs WHDLoad"
        );
    }

    // -- propose_kickstarts ------------------------------------------------

    #[test]
    fn the_proposal_matches_by_crc_and_finds_the_rtb_in_skick346() {
        let (_guard, dir) = scratch("propose");
        let roms = dir.join("roms");
        std::fs::create_dir_all(&roms).unwrap();
        let rom = vec![0x11u8; 262_144];
        std::fs::write(roms.join("my-13.rom"), &rom).unwrap();
        let crc = crate::core::hashing::crc16_arc(&rom);

        let material = dir.join("material");
        std::fs::create_dir_all(&material).unwrap();
        write_lha(
            &material.join("skick346.lha"),
            &[("Kickstarts/kick34005.A500.RTB", &[1u8; 4000][..])],
        );

        let needs = TitleNeeds {
            titles: vec![(
                "Games/T/T.slave".into(),
                vec![WantedImage {
                    name: "kick34005.A500".into(),
                    crc16: Some(crc),
                    size: Some(262_144),
                }],
            )],
            unreadable: vec![],
            slaves: 1,
        };

        let proposal = propose_kickstarts(
            &needs,
            std::slice::from_ref(&roms),
            std::slice::from_ref(&material),
        )
        .unwrap();

        let item = &proposal.items[0];
        assert!(
            matches!(item.offer, Offer::Supplied { .. }),
            "{:?}",
            item.offer
        );
        assert!(
            matches!(&item.rtb, RtbSource::InArchive { member, .. } if member == "Kickstarts/kick34005.A500.RTB"),
            "{:?}",
            item.rtb
        );
        assert!(!proposal.rtb_missing);
    }

    /// Nothing supplies the image and nothing carries its `.RTB` either: both
    /// halves of "ART cannot say yes" are reported, never silently dropped.
    #[test]
    fn nothing_found_is_notheremissing_and_the_rtb_flag_is_set() {
        let (_guard, dir) = scratch("nothing-found");
        let needs = TitleNeeds {
            titles: vec![(
                "Games/T/T.slave".into(),
                vec![WantedImage {
                    name: "kick34005.A500".into(),
                    crc16: Some(0xABCD),
                    size: None,
                }],
            )],
            unreadable: vec![],
            slaves: 1,
        };

        let proposal = propose_kickstarts(&needs, &[], std::slice::from_ref(&dir)).unwrap();

        assert!(matches!(proposal.items[0].offer, Offer::NotHere { .. }));
        assert_eq!(
            proposal.items[0].rtb,
            RtbSource::Missing {
                get_from: RtbPackage::AminetSkick346
            }
        );
        assert!(proposal.rtb_missing);
    }

    /// M16: the A1000's `.RTB` is not in `skick346`, and the proposal does
    /// not send the user there for it.
    #[test]
    fn a_missing_rtb_names_the_package_that_carries_it() {
        assert_eq!(
            rtb_package_for("kick31034.A1000"),
            RtbPackage::WhdloadSevenCitiesOfGold
        );
        for name in [
            "kick33180.A500",
            "kick34005.A500",
            "kick40063.A600",
            "kick40068.A1200",
            "kick40068.A4000",
        ] {
            assert_eq!(rtb_package_for(name), RtbPackage::AminetSkick346, "{name}");
        }
    }

    // -- place_agreed --------------------------------------------------------

    /// Offered as the file is now: its WHDLoad checksum read off the file,
    /// the way `propose_kickstarts` read it (the build checks it again).
    fn supplied(name: &str, from: &Path) -> Offer {
        let crc = crate::core::hashing::crc16_arc(&std::fs::read(from).unwrap());
        Offer::Supplied {
            wanted: WantedImage {
                name: name.to_string(),
                crc16: Some(crc),
                size: None,
            },
            by: SuppliedBy {
                path: from.display().to_string(),
                name: "Kickstart".into(),
                size_disagrees: None,
            },
        }
    }

    #[test]
    fn a_missing_rtb_is_said_and_that_image_cannot_be_agreed() {
        let (_guard, dir) = scratch("missing-rtb");
        let rom = dir.join("a.rom");
        std::fs::write(&rom, vec![0xAAu8; 4096]).unwrap();

        let proposal = KickstartProposal {
            items: vec![ProposedKickstart {
                name: "kick34005.A500".into(),
                titles: vec!["Games/A/A.slave".into()],
                titles_more: 0,
                offer: supplied("kick34005.A500", &rom),
                rtb: RtbSource::Missing {
                    get_from: RtbPackage::AminetSkick346,
                },
            }],
            unreadable_slaves: vec![],
            rtb_missing: true,
        };
        let tree = dir.join("dist");
        std::fs::create_dir_all(&tree).unwrap();

        let err = place_agreed(&proposal, &["kick34005.A500".to_string()], &tree).unwrap_err();
        assert_eq!(err.code(), "ART-KICKSTART-NOT-PROPOSED");
        assert!(!tree.join("Devs").exists(), "nothing was written");
    }

    #[test]
    fn only_the_agreed_names_are_placed_with_their_rtb_under_devs_kickstarts() {
        let (_guard, dir) = scratch("agree");
        let rom_a = dir.join("a.rom");
        std::fs::write(&rom_a, vec![0xAAu8; 4096]).unwrap();
        let rtb_a = dir.join("a.RTB");
        std::fs::write(&rtb_a, vec![0xBBu8; 512]).unwrap();
        let rom_b = dir.join("b.rom");
        std::fs::write(&rom_b, vec![0xCCu8; 4096]).unwrap();
        let rtb_b = dir.join("b.RTB");
        std::fs::write(&rtb_b, vec![0xDDu8; 512]).unwrap();

        let proposal = KickstartProposal {
            items: vec![
                ProposedKickstart {
                    name: "kick34005.A500".into(),
                    titles: vec!["Games/A/A.slave".into()],
                    titles_more: 0,
                    offer: supplied("kick34005.A500", &rom_a),
                    rtb: RtbSource::Loose {
                        path: rtb_a.clone(),
                    },
                },
                ProposedKickstart {
                    name: "kick40068.A1200".into(),
                    titles: vec!["Games/B/B.slave".into()],
                    titles_more: 0,
                    offer: supplied("kick40068.A1200", &rom_b),
                    rtb: RtbSource::Loose {
                        path: rtb_b.clone(),
                    },
                },
            ],
            unreadable_slaves: vec![],
            rtb_missing: false,
        };

        let tree = dir.join("dist");
        std::fs::create_dir_all(&tree).unwrap();
        let placed = place_agreed(&proposal, &["kick34005.A500".to_string()], &tree).unwrap();

        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].name, "kick34005.A500");
        assert!(matches!(placed[0].image, PlaceOutcome::Placed { .. }));
        assert!(matches!(placed[0].rtb, PlaceOutcome::Placed { .. }));
        assert_eq!(
            std::fs::read(tree.join("Devs").join("Kickstarts").join("kick34005.A500")).unwrap(),
            vec![0xAAu8; 4096]
        );
        assert_eq!(
            std::fs::read(
                tree.join("Devs")
                    .join("Kickstarts")
                    .join("kick34005.A500.RTB")
            )
            .unwrap(),
            vec![0xBBu8; 512]
        );
        assert!(
            !tree
                .join("Devs")
                .join("Kickstarts")
                .join("kick40068.A1200")
                .exists(),
            "only the agreed name is placed"
        );
    }

    /// ART-343's ROM half: the file the user agreed to is the file placed.
    /// Each agreed Kickstart's source is read again when the build checks the
    /// agreement, and one that changed or went since the proposal is refused
    /// by name — its path, what the proposal offered and what is there now —
    /// before anything is written. The unchanged file is the control.
    #[test]
    fn an_agreed_kickstart_whose_file_changed_since_the_proposal_is_refused_by_name() {
        let (_guard, dir) = scratch("rom-changed");
        let rom = dir.join("a.rom");
        let rtb = dir.join("a.RTB");
        std::fs::write(&rtb, vec![0xBBu8; 512]).unwrap();
        let proposal_for = |offer: Offer| KickstartProposal {
            items: vec![ProposedKickstart {
                name: "kick34005.A500".into(),
                titles: vec!["Games/A/A.slave".into()],
                titles_more: 0,
                offer,
                rtb: RtbSource::Loose { path: rtb.clone() },
            }],
            unreadable_slaves: vec![],
            rtb_missing: false,
        };
        let agreed = ["kick34005.A500".to_string()];
        let tree = dir.join("dist");
        std::fs::create_dir_all(&tree).unwrap();

        // Control: the file as it was offered passes.
        std::fs::write(&rom, vec![0xAAu8; 4096]).unwrap();
        let unchanged = proposal_for(supplied("kick34005.A500", &rom));
        assert_eq!(check_agreed(&unchanged, &agreed).unwrap().len(), 1);

        // Its bytes changed: another checksum.
        std::fs::write(&rom, vec![0xACu8; 4096]).unwrap();
        let err = check_agreed(&unchanged, &agreed).unwrap_err();
        assert_eq!(err.code(), "ART-KICKSTART-SOURCE-CHANGED", "{err}");
        let message = err.to_string();
        assert!(message.contains("kick34005.A500"), "{message}");
        assert!(message.contains(&rom.display().to_string()), "{message}");
        let offered = crate::core::hashing::crc16_arc(&vec![0xAAu8; 4096]);
        let now = crate::core::hashing::crc16_arc(&vec![0xACu8; 4096]);
        assert!(message.contains(&format!("${offered:04X}")), "{message}");
        assert!(message.contains(&format!("${now:04X}")), "{message}");
        assert!(message.contains("prepare the card again"), "{message}");
        let err = place_agreed(&unchanged, &agreed, &tree).unwrap_err();
        assert_eq!(err.code(), "ART-KICKSTART-SOURCE-CHANGED");
        assert!(!tree.join("Devs").exists(), "nothing was written");

        // The same checksum, and a size other than the offer stated.
        std::fs::write(&rom, vec![0xAAu8; 4096]).unwrap();
        let sized = |size: u32, disagrees: Option<u32>| match supplied("kick34005.A500", &rom) {
            Offer::Supplied { mut wanted, mut by } => {
                wanted.size = Some(size);
                by.size_disagrees = disagrees;
                proposal_for(Offer::Supplied { wanted, by })
            }
            _ => unreachable!(),
        };
        assert_eq!(check_agreed(&sized(4096, None), &agreed).unwrap().len(), 1);
        assert_eq!(
            check_agreed(&sized(8192, Some(4096)), &agreed)
                .unwrap()
                .len(),
            1,
            "the offer stated the file's own size beside the slave's"
        );
        let err = check_agreed(&sized(8192, None), &agreed).unwrap_err();
        assert_eq!(err.code(), "ART-KICKSTART-SOURCE-CHANGED", "{err}");
        assert!(err.to_string().contains("8192"), "{err}");

        // It is gone.
        std::fs::remove_file(&rom).unwrap();
        let err = check_agreed(&unchanged, &agreed).unwrap_err();
        assert_eq!(err.code(), "ART-KICKSTART-SOURCE-CHANGED", "{err}");
        let message = err.to_string();
        assert!(message.contains(&rom.display().to_string()), "{message}");
        assert!(message.contains("no longer there"), "{message}");
        assert!(!tree.join("Devs").exists(), "nothing was written");
    }

    #[test]
    fn a_name_the_proposal_does_not_carry_is_refused_before_anything_is_written() {
        let (_guard, dir) = scratch("not-carried");
        let proposal = KickstartProposal::default();
        let tree = dir.join("dist");
        std::fs::create_dir_all(&tree).unwrap();

        let err = place_agreed(&proposal, &["kick99999.A9".to_string()], &tree).unwrap_err();
        assert_eq!(err.code(), "ART-KICKSTART-NOT-PROPOSED");
        assert!(!tree.join("Devs").exists());
    }
}
