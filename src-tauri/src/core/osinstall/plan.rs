//! Turning a [`Recipe`], a media folder and (optionally) a ROM into an
//! [`InstallPlan`] — or into every reason it cannot proceed.
//!
//! ## Collect every refusal, never stop at the first
//!
//! The screen shows every problem at once; a user fixing them one dispatch
//! at a time is a bad afternoon. So [`plan`] never returns early on a
//! recoverable problem — a missing disk, a path the recipe expects that the
//! media does not have, an unresolved collision — it keeps walking and
//! collects every [`RefusalReason`] it finds. But any refusal at all empties
//! `items` and `media_paths`: a half-planned install, with some of the
//! files it would write and none of the ones it could not resolve, is not
//! something to preview.
//!
//! ## A green recipe says nothing about file-level collisions
//!
//! `recipe.rs`'s own collision test only checks `File`-kind rules against
//! each other, because a coinciding `Subtree` destination is a legal merge
//! point (fifteen locale disks all contribute to `Locale/Languages`) rather
//! than a claim. The check here is the same rule, applied one level lower —
//! over the *expanded* item list, after every `Subtree` rule has been
//! walked out into the real files the media actually holds. That is where a
//! genuine collision (two components writing the same destination *file*)
//! can actually be seen, and it is what the spec means by "a collision
//! inside a plan is a defect".
//!
//! ## `MediaMatch` is matched exhaustively, never `if let`
//!
//! `scan::MediaMatch` is `#[must_use]` and says so in its own doc: an
//! `if let MediaMatch::Found(..)` silently reads `Ambiguous` as `Missing`,
//! which is the arbitrary-winner failure the enum exists to rule out. This
//! module is the one place that resolves a component's media, so it is the
//! one place that has to get this right.
//!
//! ## The ROM's own header, never `KNOWN_ROMS`
//!
//! `Condition::RomOlderThan` exists because `Workbench3.2.adf:S/Startup-sequence`
//! opens with `Version exec.library version 47` / `If Warn` / … / `Quit` — a
//! 3.2 system installed on an older Kickstart, without `LIBS:Modules`, does
//! not boot at all. So the condition has to be decided, not skipped.
//!
//! It is decided from `core::rom::stated_version`, which reads the major and
//! minor a Kickstart states about itself at offset 12 in its own header —
//! never from `KNOWN_ROMS`, the curated table of dump checksums. ART-104 is
//! why: the user's own licensed A1200 Kickstart hashes to a dump that table
//! does not carry, so it comes back unidentified even though it is a
//! perfectly good 3.1 ROM. A condition resting on that table would misfire
//! on a ROM that is right; asking the ROM what it is costs nothing extra and
//! cannot be wrong about a dump nobody has catalogued yet.
//!
//! ## Refuse, never guess
//!
//! An unidentified ROM makes [`condition_holds`] return
//! `Err(RefusalReason::RomUnknown)` rather than picking a default. Guessing
//! "off" on a pre-V47 ROM produces a system that quits at boot; guessing
//! "on" on a V47 ROM wastes 800 KB installing modules nothing loads. Neither
//! is ART's to choose for the user, so neither is chosen.
//!
//! ## Two functions, kept apart on purpose
//!
//! `condition_holds` is pure — it takes the facts already read, never a
//! `Path` — so Task 5 can call it once per conditional component in a recipe
//! without re-reading the ROM file each time. `rom_facts` is the one place
//! that touches disk, called once per install plan.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::scan::{find_media, media_for, MediaMatch};
use super::source::{AdfSource, MediaSource};
use super::{Condition, Recipe, RefusalReason, RuleKind};
use crate::core::error::{CoreError, CoreResult};

/// What a planning decision needs to know about the paired Kickstart.
///
/// Only the major: every `Condition` variant so far (`RomOlderThan`) tests
/// the major alone, and `stated_version`'s minor is trivially available
/// later — by widening this struct — should a future condition ever need
/// finer granularity than "3.1 vs 3.2". Carrying it now, unread by anything,
/// would be a guess about a need that does not exist yet, which is exactly
/// what this module's own `RomOlderThan` rule is built to avoid making
/// about the ROM itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RomFacts {
    pub major: u16,
}

/// Read the paired Kickstart's own stated major.
///
/// Strips a Cloanto header first (`core::rom::strip_cloanto_header`) — the
/// user has Amiga Forever, and those dumps carry an 11-byte `AMIROMTYPE1`
/// prefix that is not part of the ROM proper. A Kickstart is 512 KB, small
/// enough that reading it whole here does not need the windowed-read
/// treatment `open_hdf` gives a multi-gigabyte HDF.
pub fn rom_facts(rom: &Path) -> CoreResult<RomFacts> {
    let bytes = crate::core::rom::strip_cloanto_header(&std::fs::read(rom)?);
    let (major, _minor) = crate::core::rom::stated_version(&bytes).ok_or_else(|| {
        CoreError::InvalidInput("this file does not state a Kickstart version".into())
    })?;
    Ok(RomFacts { major })
}

/// Whether a conditional component switches on, given the facts already
/// read about the paired ROM — `None` when the ROM could not be identified
/// at all, which refuses rather than guessing (see the module doc comment).
pub fn condition_holds(
    condition: &Condition,
    rom: Option<&RomFacts>,
) -> Result<bool, RefusalReason> {
    let rom = rom.ok_or(RefusalReason::RomUnknown)?;
    match condition {
        Condition::RomOlderThan { major } => Ok(rom.major < *major),
    }
}

/// One file or directory `apply` (a later task) will place in the
/// distribution tree.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub component: String,
    /// The volume name the bytes came from — not the image's own filename.
    pub media: String,
    /// Where it lives on the media, `/`-separated, relative to the media's
    /// own root.
    pub from: String,
    /// Where it goes in the tree, `/`-separated.
    pub to: String,
    pub is_dir: bool,
    pub bytes: u64,
}

/// What [`plan`] produces: either a full description of what would be
/// written, or every reason it cannot proceed — never both. Any refusal at
/// all empties `items` and `media_paths` (see the module doc comment); the
/// UI can always tell the two cases apart by checking `refusals.is_empty()`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPlan {
    pub release: String,
    pub items: Vec<PlanItem>,
    pub refusals: Vec<RefusalReason>,
    /// The sum of `items[].bytes` — `0` whenever `items` is empty, which is
    /// always true together, never computed separately.
    pub total_bytes: u64,
    /// Every component id that is switched on — required, explicitly
    /// chosen, or turned on by its own [`Condition`] — regardless of
    /// whether its media could actually be found. Populated even when the
    /// plan as a whole refuses, so the UI can explain *why* a refusal names
    /// a component the user never picked.
    pub components_on: Vec<String>,
    /// Volume name -> the image it was found in. Resolved here so `apply`
    /// (a later task) can reopen the media without re-scanning the folder —
    /// and so the plan that was previewed is the plan that runs, even if
    /// the folder changed underneath it.
    pub media_paths: BTreeMap<String, PathBuf>,
}

/// What the user asked for.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRequest {
    pub media_folder: PathBuf,
    /// The paired Kickstart, if the user supplied one. `None` refuses any
    /// component whose [`Condition`] needs it to be decided — see
    /// [`condition_holds`].
    pub rom: Option<PathBuf>,
    /// Component ids the user picked. `required` components and
    /// condition-satisfied ones are added on top of this, not instead of
    /// it — see [`InstallPlan::components_on`].
    pub chosen: Vec<String>,
    pub destination: PathBuf,
}

/// `entry_path`, relative to `from` rather than to the media root. `from`
/// itself maps to `""`, matching what a [`RuleKind::Subtree`] rule's own
/// root entry resolves to — so a rule's own directory lands at `to` and
/// everything under it lands at `to/…`.
fn relative_to(entry_path: &str, from: &str) -> String {
    if from.is_empty() {
        return entry_path.to_string();
    }
    match entry_path.strip_prefix(from) {
        Some(rest) => rest.strip_prefix('/').unwrap_or(rest).to_string(),
        None => entry_path.to_string(),
    }
}

/// Where `relative` (already relative to a rule's `from`) lands under the
/// rule's `to`.
fn destination_for(to: &str, relative: &str) -> String {
    if relative.is_empty() {
        to.to_string()
    } else if to.is_empty() {
        relative.to_string()
    } else {
        format!("{to}/{relative}")
    }
}

/// Which components are switched on: `required`, explicitly `chosen`, or
/// turned on by a satisfied [`Condition`] — the user is never asked about
/// the last kind (see the doc comment on [`Condition`] itself). Every
/// conditional component in the recipe is decided, not only the ones the
/// user happened to pick, which is the point of a condition existing at
/// all. An unsatisfiable condition (`Err(RefusalReason::RomUnknown)`) is
/// reported at most once, however many conditional components share the
/// same unreadable ROM, so the refusal list names one problem instead of
/// repeating it.
fn resolve_components_on(
    recipe: &Recipe,
    chosen: &[String],
    rom_facts: Option<&RomFacts>,
    refusals: &mut Vec<RefusalReason>,
) -> Vec<String> {
    let chosen: HashSet<&str> = chosen.iter().map(String::as_str).collect();
    let mut rom_unknown_reported = refusals.contains(&RefusalReason::RomUnknown);
    let mut on = Vec::new();

    for component in &recipe.components {
        let mut is_on = component.required || chosen.contains(component.id.as_str());
        if let Some(condition) = &component.condition {
            match condition_holds(condition, rom_facts) {
                Ok(true) => is_on = true,
                Ok(false) => {}
                Err(reason) => {
                    if !rom_unknown_reported {
                        refusals.push(reason);
                        rom_unknown_reported = true;
                    }
                }
            }
        }
        if is_on {
            on.push(component.id.clone());
        }
    }
    on
}

/// Every destination that two or more components write a **file** to
/// without one declaring an `overrides` entry that covers all the others.
/// `Subtree` destinations coinciding is not checked — see the module doc
/// comment on why that is a merge point, not a claim; this is the same rule
/// `recipe.rs`'s own
/// `no_two_components_claim_one_destination_without_declaring_it` applies
/// to the rules themselves, applied here to the walked-out file list
/// `plan` actually produces.
fn detect_collisions(items: &[PlanItem], recipe: &Recipe) -> Vec<RefusalReason> {
    let mut claimants: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for item in items.iter().filter(|item| !item.is_dir) {
        let claiming = claimants.entry(item.to.clone()).or_default();
        if !claiming.contains(&item.component) {
            claiming.push(item.component.clone());
        }
    }

    let mut refusals = Vec::new();
    for (path, claiming) in &claimants {
        if claiming.len() < 2 {
            continue;
        }

        let resolved = claiming.iter().any(|winner| {
            let Some(winner_component) = recipe.component(winner) else {
                return false;
            };
            claiming
                .iter()
                .filter(|other| *other != winner)
                .all(|other| winner_component.overrides.contains(other))
        });

        if !resolved {
            let mut components = claiming.clone();
            components.sort();
            refusals.push(RefusalReason::DestinationCollision {
                path: path.clone(),
                components,
            });
        }
    }
    refusals
}

/// Turn a recipe, a media folder and (optionally) a ROM into a description
/// of what would be written — or into every reason it cannot proceed.
///
/// Order: read the ROM once, decide which components are on, resolve each
/// on component's media by volume name, resolve every one of its rules
/// against that media (expanding `Subtree` rules with
/// [`MediaSource::walk`]), check the whole walked-out item list for
/// file-level collisions, and sum. See the module doc comment for why
/// refusals never stop the walk and why any refusal empties `items`.
pub fn plan(request: &InstallRequest, recipe: &Recipe) -> CoreResult<InstallPlan> {
    let mut refusals: Vec<RefusalReason> = Vec::new();

    let rom_facts = match &request.rom {
        Some(path) => match rom_facts(path) {
            Ok(facts) => Some(facts),
            Err(_) => {
                // A ROM path was given and it could not be identified — a
                // typed refusal, never the `CoreError` sentence `rom_facts`
                // itself raises (ART-060; the join Task 4's review carried
                // into this task by name).
                refusals.push(RefusalReason::RomUnknown);
                None
            }
        },
        None => None,
    };

    let components_on =
        resolve_components_on(recipe, &request.chosen, rom_facts.as_ref(), &mut refusals);

    let found = find_media(&request.media_folder)?;
    let mut media_paths: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut items: Vec<PlanItem> = Vec::new();

    for component_id in &components_on {
        let component = recipe.component(component_id).expect(
            "components_on only ever names ids resolve_components_on read from this same recipe",
        );

        // Never `if let MediaMatch::Found(..)` — see the module doc comment.
        let media_path = match media_for(&found, &component.media) {
            MediaMatch::Missing => {
                refusals.push(RefusalReason::MediaMissing {
                    component: component.id.clone(),
                    volume_name: component.media.clone(),
                });
                continue;
            }
            MediaMatch::Ambiguous(matches) => {
                refusals.push(RefusalReason::MediaAmbiguous {
                    component: component.id.clone(),
                    volume_name: component.media.clone(),
                    paths: matches
                        .iter()
                        .map(|m| m.path.display().to_string())
                        .collect(),
                });
                continue;
            }
            MediaMatch::Found(found_media) => found_media.path.clone(),
        };
        media_paths.insert(component.media.clone(), media_path.clone());

        let mut source = AdfSource::open(&media_path)?;

        for rule in &component.rules {
            let Some(entry) = source.entry(&rule.from)? else {
                // The media is here and the path the recipe expects is not
                // — a refusal, not a skip (see the module doc comment and
                // the `RefusalReason::MediaPathMissing` doc comment).
                refusals.push(RefusalReason::MediaPathMissing {
                    component: component.id.clone(),
                    media: component.media.clone(),
                    path: rule.from.clone(),
                });
                continue;
            };

            match rule.kind {
                RuleKind::File => {
                    items.push(PlanItem {
                        component: component.id.clone(),
                        media: component.media.clone(),
                        from: rule.from.clone(),
                        to: rule.to.clone(),
                        is_dir: entry.is_dir,
                        bytes: entry.size,
                    });
                }
                RuleKind::Subtree => {
                    // The subtree's own root, so an empty drawer still gets
                    // created — `walk` yields only what is *inside* `from`,
                    // never `from` itself.
                    items.push(PlanItem {
                        component: component.id.clone(),
                        media: component.media.clone(),
                        from: rule.from.clone(),
                        to: rule.to.clone(),
                        is_dir: entry.is_dir,
                        bytes: 0,
                    });
                    for walked in source.walk(&rule.from)? {
                        let relative = relative_to(&walked.path, &rule.from);
                        items.push(PlanItem {
                            component: component.id.clone(),
                            media: component.media.clone(),
                            from: walked.path.clone(),
                            to: destination_for(&rule.to, &relative),
                            is_dir: walked.is_dir,
                            bytes: walked.size,
                        });
                    }
                }
            }
        }
    }

    refusals.extend(detect_collisions(&items, recipe));

    let (items, media_paths) = if refusals.is_empty() {
        (items, media_paths)
    } else {
        (Vec::new(), BTreeMap::new())
    };
    let total_bytes = items.iter().map(|item| item.bytes).sum();

    Ok(InstallPlan {
        release: recipe.release.clone(),
        items,
        refusals,
        total_bytes,
        components_on,
        media_paths,
    })
}

#[cfg(test)]
mod condition_tests {
    use super::*;

    /// `Workbench3.2.adf:S/Startup-sequence` opens with
    /// `Version exec.library version 47 … If Warn … Quit`. So a 3.2 system on a
    /// 3.1 ROM without `LIBS:Modules` does not boot at all.
    #[test]
    fn a_pre_v47_rom_turns_the_modules_component_on() {
        let holds = condition_holds(
            &Condition::RomOlderThan { major: 47 },
            Some(&RomFacts { major: 40 }),
        );
        assert_eq!(holds, Ok(true));
    }

    #[test]
    fn a_v47_rom_leaves_it_off() {
        let holds = condition_holds(
            &Condition::RomOlderThan { major: 47 },
            Some(&RomFacts { major: 47 }),
        );
        assert_eq!(holds, Ok(false));
    }

    /// Guessing costs 800 KB, or a system that quits at boot. Neither is ART's
    /// to choose.
    #[test]
    fn an_unidentified_rom_refuses_rather_than_guessing() {
        let holds = condition_holds(&Condition::RomOlderThan { major: 47 }, None);
        assert_eq!(holds, Err(RefusalReason::RomUnknown));
    }

    /// The ROM's own header, not `KNOWN_ROMS` — the user's licensed A1200 dump
    /// is not in that table (ART-104) and is still a perfectly good 3.1 ROM.
    ///
    /// The brief's own version of this test used `tempfile::tempdir()`, but
    /// this project deliberately does not depend on `tempfile` (see
    /// `fixtures::scratch`'s doc comment) — `scratch` is the repository's
    /// existing way to get a private directory for one test.
    #[test]
    fn the_major_comes_from_the_roms_own_header() {
        let dir = super::super::fixtures::scratch("plan-rom-header");
        let path = dir.join("fake.rom");
        let mut bytes = vec![0u8; 512 * 1024];
        bytes[12..14].copy_from_slice(&40u16.to_be_bytes());
        bytes[14..16].copy_from_slice(&68u16.to_be_bytes());
        std::fs::write(&path, &bytes).unwrap();

        assert_eq!(rom_facts(&path).unwrap().major, 40);
    }

    /// The user has licensed Amiga Forever (desktop and mobile — see
    /// `docs/STATUS.md`), so a Cloanto-headered dump is ordinary input on
    /// this machine, not an edge case. Without the strip, `rom_facts` would
    /// read bytes 12..16 eleven bytes early, land outside the plausible
    /// major range, and refuse a perfectly good ROM — ART-104's exact shape,
    /// surfacing at the user's Amiga instead of in CI. `fake_rom` alone
    /// cannot express this: it never carries the `AMIROMTYPE1` prefix, so
    /// this test builds one by hand, the one byte-for-byte thing `fake_rom`
    /// does not do.
    #[test]
    fn a_cloanto_headered_dump_still_reads_its_stated_major() {
        let dir = super::super::fixtures::scratch("plan-rom-cloanto");
        let path = dir.join("cloanto.rom");

        let mut bytes = b"AMIROMTYPE1".to_vec();
        let mut body = vec![0u8; 512 * 1024];
        body[12..14].copy_from_slice(&40u16.to_be_bytes());
        body[14..16].copy_from_slice(&68u16.to_be_bytes());
        bytes.extend_from_slice(&body);
        std::fs::write(&path, &bytes).unwrap();

        assert_eq!(rom_facts(&path).unwrap().major, 40);
    }

    /// A file that exists and reads fine but is not a ROM at all — plain
    /// text, far too short to carry a version field. This is the case
    /// `an_unreadable_rom_is_a_core_error_not_a_panic` (a *missing* file)
    /// could not pin: that test's `is_err()` would pass for an I/O failure
    /// just as readily as for a content problem, so it never proved
    /// `rom_facts` actually rejects bad content rather than merely
    /// propagating `std::fs::read`'s own error. This one names the exact
    /// variant.
    #[test]
    fn content_that_is_not_a_rom_is_refused_as_invalid_input() {
        let dir = super::super::fixtures::scratch("plan-rom-not-a-rom");
        let path = dir.join("readme.txt");
        std::fs::write(&path, b"this is not a Kickstart image").unwrap();

        assert!(matches!(rom_facts(&path), Err(CoreError::InvalidInput(_))));
    }
}

#[cfg(test)]
mod plan_tests {
    use super::*;
    use crate::core::osinstall::{Component, PathRule};

    /// The common case: media that matches the shipped recipe exactly, so a
    /// test only has to state which components are chosen and which disks
    /// are in the folder. Rom major `47` — V47 or later — keeps
    /// `modules-a1200`'s own condition off by default, so a test that is
    /// not about the ROM does not have to think about it.
    fn plan_with(chosen: &[&str], present: &[&str]) -> InstallPlan {
        crate::core::osinstall::fixtures::planned_with(chosen, present, Some(47)).0
    }

    /// The variant that *is* about the ROM. `Workbench3.2` alone is enough
    /// media for `workbench-base` (required) to resolve without noise; the
    /// point of this helper is `components_on`, not whether
    /// `modules-a1200`'s own media happens to be present too.
    fn plan_with_rom(chosen: &[&str], rom_major: u16) -> InstallPlan {
        crate::core::osinstall::fixtures::planned_with(chosen, &["Workbench3.2"], Some(rom_major)).0
    }

    /// `Workbench3.2` built completely valid — every one of `workbench-base`'s
    /// rules resolves, exactly as [`plan_with`] would build it. `Extras3.2`
    /// starts from the same recipe-derived content and then has its `L`
    /// entry removed — the one path this test means to break, and the only
    /// difference from an ordinary, fully-satisfied `Extras3.2`.
    fn plan_where_extras_has_no_l() -> InstallPlan {
        let dir = crate::core::osinstall::fixtures::scratch("plan-extras-no-l");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        let recipe = crate::core::osinstall::recipe::amigaos_32().unwrap();

        let wb = crate::core::osinstall::fixtures::entries_for(&recipe, "Workbench3.2");
        let wb_refs: Vec<(&str, &[u8], u32)> = wb
            .iter()
            .map(|(path, bytes, protection)| (path.as_str(), bytes.as_slice(), *protection))
            .collect();
        crate::core::osinstall::fixtures::media(&folder, "Workbench3.2", "wb.adf", &wb_refs);

        let extras: Vec<(String, Vec<u8>, u32)> =
            crate::core::osinstall::fixtures::entries_for(&recipe, "Extras3.2")
                .into_iter()
                .filter(|(path, _, _)| path != "L/placeholder")
                .collect();
        let extras_refs: Vec<(&str, &[u8], u32)> = extras
            .iter()
            .map(|(path, bytes, protection)| (path.as_str(), bytes.as_slice(), *protection))
            .collect();
        crate::core::osinstall::fixtures::media(&folder, "Extras3.2", "extras.adf", &extras_refs);

        let request = InstallRequest {
            media_folder: folder,
            rom: Some(crate::core::osinstall::fixtures::fake_rom(&dir, 47)),
            chosen: vec!["extras".to_string()],
            destination: dir.join("dist"),
        };
        plan(&request, &recipe).unwrap()
    }

    /// A recipe built by hand, not the shipped one: two components with no
    /// declared `overrides` both write `C/Assign`. The shipped recipe's own
    /// `no_two_components_claim_one_destination_without_declaring_it` test
    /// proves this shape never occurs in it, so the collision guard needs a
    /// fixture that manufactures the shape on purpose.
    fn plan_with_colliding_recipe() -> InstallPlan {
        let dir = crate::core::osinstall::fixtures::scratch("plan-collision");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        crate::core::osinstall::fixtures::media(&folder, "A", "a.adf", &[("C/Assign", b"one", 0)]);
        crate::core::osinstall::fixtures::media(&folder, "B", "b.adf", &[("C/Assign", b"two", 0)]);

        let make = |id: &str, media: &str| Component {
            id: id.to_string(),
            media: media.to_string(),
            rules: vec![PathRule {
                from: "C/Assign".to_string(),
                to: "C/Assign".to_string(),
                kind: RuleKind::File,
            }],
            required: false,
            condition: None,
            overrides: vec![],
            user_startup: vec![],
            exclusive_group: None,
            available: true,
        };
        let recipe = Recipe {
            release: "Test".to_string(),
            components: vec![make("a", "A"), make("b", "B")],
        };

        let request = InstallRequest {
            media_folder: folder,
            rom: None,
            chosen: vec!["a".to_string(), "b".to_string()],
            destination: dir.join("dist"),
        };
        plan(&request, &recipe).unwrap()
    }

    #[test]
    fn a_component_whose_media_is_absent_names_the_component_and_the_disk() {
        let plan = plan_with(&["extras"], /* media present: */ &["Workbench3.2"]);
        assert!(plan.refusals.contains(&RefusalReason::MediaMissing {
            component: "extras".into(),
            volume_name: "Extras3.2".into(),
        }));
    }

    /// The media is here and the path is not — the recipe is wrong about this
    /// media. Skipping it silently gives a system missing a library.
    #[test]
    fn a_path_the_recipe_expects_and_the_media_lacks_is_a_refusal_not_a_skip() {
        let plan = plan_where_extras_has_no_l();
        assert!(matches!(
            plan.refusals.as_slice(),
            [RefusalReason::MediaPathMissing { component, path, .. }]
                if component == "extras" && path == "L"
        ));
        assert!(
            plan.items.is_empty(),
            "nothing is planned once the media is wrong"
        );
    }

    #[test]
    fn two_components_wanting_one_path_without_an_override_is_a_collision() {
        let plan = plan_with_colliding_recipe();
        assert!(matches!(
            plan.refusals.as_slice(),
            [RefusalReason::DestinationCollision { path, components }]
                if path == "C/Assign" && components.len() == 2
        ));
    }

    #[test]
    fn a_declared_override_is_not_a_collision() {
        let plan = plan_with(
            &["workbench-base", "extras"],
            &["Workbench3.2", "Extras3.2"],
        );
        assert!(!plan
            .refusals
            .iter()
            .any(|r| matches!(r, RefusalReason::DestinationCollision { .. })));
    }

    #[test]
    fn a_conditional_component_is_on_without_being_chosen() {
        let plan = plan_with_rom(&["workbench-base"], 40);
        assert!(plan.components_on.iter().any(|c| c == "modules-a1200"));
    }

    #[test]
    fn the_total_is_the_sum_of_what_will_actually_be_written() {
        let plan = plan_with(&["workbench-base"], &["Workbench3.2"]);
        assert_eq!(
            plan.total_bytes,
            plan.items.iter().map(|i| i.bytes).sum::<u64>()
        );
    }

    // ---- coverage beyond the brief's own six, closing gaps a
    // falsification pass found ----

    /// The brief's own total-bytes test is satisfied by construction: both
    /// sides of the assertion compute the identical formula, so a `plan()`
    /// that quietly returned `total_bytes: 0` while still filling `items`
    /// correctly would pass it just as well as a correct one, as long as it
    /// stayed self-consistent. This pins the number against a real, known
    /// item list instead, so a broken sum cannot hide behind agreeing with
    /// itself.
    #[test]
    fn the_total_is_a_real_positive_number_not_just_self_consistent() {
        let plan = plan_with(&["workbench-base"], &["Workbench3.2"]);
        assert!(
            plan.total_bytes > 0,
            "workbench-base's media carries real bytes"
        );
        assert!(!plan.items.is_empty());
    }

    /// `condition_holds(_, None)` already refuses rather than guessing
    /// (Task 4's own `an_unidentified_rom_refuses_rather_than_guessing`).
    /// This is the join Task 4's review carried into this task by name: a
    /// ROM file `plan()` cannot read as a Kickstart at all must still reach
    /// the caller as `RefusalReason::RomUnknown`, not as the bare
    /// `CoreError` sentence `rom_facts` itself raises.
    #[test]
    fn a_rom_that_does_not_state_a_version_is_romunknown_not_a_core_error() {
        let dir = crate::core::osinstall::fixtures::scratch("plan-bad-rom");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        crate::core::osinstall::fixtures::workbench(&folder);
        let bad_rom = dir.join("not-a-rom.bin");
        std::fs::write(&bad_rom, b"nope").unwrap();

        let recipe = crate::core::osinstall::recipe::amigaos_32().unwrap();
        let request = InstallRequest {
            media_folder: folder,
            rom: Some(bad_rom),
            chosen: vec!["workbench-base".to_string()],
            destination: dir.join("dist"),
        };

        let plan = plan(&request, &recipe).unwrap();
        assert!(plan.refusals.contains(&RefusalReason::RomUnknown));
        assert!(plan.items.is_empty());
    }

    /// Carried concern #2 from Task 3's review: `MediaAmbiguous` existed
    /// with nothing to raise it. Two files in the folder both carry the
    /// volume name `workbench-base` wants — `scan::find_media` already
    /// keeps both rather than guessing at one; this is the proof that
    /// `plan()` actually turns that into the refusal naming the one
    /// component actually affected, not a silently arbitrary pick of
    /// either file.
    #[test]
    fn two_files_claiming_one_volume_name_is_media_ambiguous_for_the_component_that_needs_it() {
        let dir = crate::core::osinstall::fixtures::scratch("plan-ambiguous");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        let recipe = crate::core::osinstall::recipe::amigaos_32().unwrap();

        let wb = crate::core::osinstall::fixtures::entries_for(&recipe, "Workbench3.2");
        let wb_refs: Vec<(&str, &[u8], u32)> = wb
            .iter()
            .map(|(path, bytes, protection)| (path.as_str(), bytes.as_slice(), *protection))
            .collect();
        crate::core::osinstall::fixtures::media(&folder, "Workbench3.2", "wb-copy-1.adf", &wb_refs);
        crate::core::osinstall::fixtures::media(&folder, "Workbench3.2", "wb-copy-2.adf", &wb_refs);

        let request = InstallRequest {
            media_folder: folder,
            rom: Some(crate::core::osinstall::fixtures::fake_rom(&dir, 47)),
            chosen: vec!["workbench-base".to_string()],
            destination: dir.join("dist"),
        };
        let plan = plan(&request, &recipe).unwrap();

        assert!(plan.refusals.iter().any(|r| matches!(
            r,
            RefusalReason::MediaAmbiguous { component, volume_name, paths }
                if component == "workbench-base" && volume_name == "Workbench3.2" && paths.len() == 2
        )));
        assert!(plan.items.is_empty());
    }
}
