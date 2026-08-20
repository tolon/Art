//! The shipped recipes, as data.
//!
//! `include_str!` for the same three reasons `core/distro` uses it: reviewable
//! in a diff, shipped without a network, and unable to grow a code path.

use super::{Component, Recipe};
use crate::core::error::{CoreError, CoreResult};

const AMIGAOS_32_JSON: &str = include_str!("recipes/amigaos-3.2.json");
const AMIGAOS_39_JSON: &str = include_str!("recipes/amigaos-3.9.json");

/// Parse and validate a recipe.
pub fn parse(json: &str) -> CoreResult<Recipe> {
    let recipe: Recipe = serde_json::from_str(json).map_err(|e| CoreError::Malformed {
        format: "recipe".into(),
        detail: e.to_string(),
    })?;
    validate(&recipe)?;
    Ok(recipe)
}

/// Check one `/`-separated path field (`from` or `to`) against the same
/// rules AmigaDOS itself would apply: every segment has to be a name it can
/// store, and nothing may climb out of the tree it's rooted in.
///
/// `from` is human-typed data too, exactly like `to` — measured off real
/// media, but still typed by a person into this JSON file — and `from: ""`
/// is meaningful (the media's own root, for `fonts` and `backdrops`), so
/// `allow_empty` lets a caller say which fields may be empty and which must
/// not be.
fn validate_path(component_id: &str, field: &str, path: &str, allow_empty: bool) -> CoreResult<()> {
    if path.is_empty() {
        return if allow_empty {
            Ok(())
        } else {
            Err(CoreError::Malformed {
                format: "recipe".into(),
                detail: format!("'{component_id}': {field} is empty"),
            })
        };
    }
    for segment in path.split('/') {
        crate::core::volume::write::dir::check_name(segment)?;
    }
    if path.starts_with('/') || path.split('/').any(|s| s == "..") {
        return Err(CoreError::Malformed {
            format: "recipe".into(),
            detail: format!("'{component_id}': {field} '{path}' leaves the tree"),
        });
    }
    Ok(())
}

/// One component's own share of [`validate`]: real media, and every path —
/// media-side `from` as well as tree-side `to` — a name AmigaDOS can
/// actually store, inside the tree it's rooted in.
///
/// `pub(super)` rather than private: `package.rs` needs the identical gate
/// for the single component a package wraps — a package's rules are AmigaDOS
/// paths exactly like a release recipe's, and `parse`'s validation shape is
/// what a package's own parser is required to follow (Task 4). Splitting
/// this out of [`validate`] is what makes that possible without a second,
/// drifting copy of the same checks.
pub(super) fn validate_component(component: &Component) -> CoreResult<()> {
    if component.media.trim().is_empty() {
        return Err(CoreError::Malformed {
            format: "recipe".into(),
            detail: format!("'{}' names no media", component.id),
        });
    }
    for rule in &component.rules {
        validate_path(&component.id, "from", &rule.from, true)?;
        validate_path(&component.id, "to", &rule.to, false)?;
    }
    Ok(())
}

/// Everything a recipe must get right before ART trusts it: no two
/// components sharing an id, plus [`validate_component`] for each one.
fn validate(recipe: &Recipe) -> CoreResult<()> {
    let mut seen_ids = std::collections::HashSet::new();
    for component in &recipe.components {
        if !seen_ids.insert(component.id.as_str()) {
            return Err(CoreError::Malformed {
                format: "recipe".into(),
                detail: format!("two components share the id '{}'", component.id),
            });
        }
        validate_component(component)?;
    }
    Ok(())
}

/// The shipped AmigaOS 3.2 recipe.
pub fn amigaos_32() -> CoreResult<Recipe> {
    parse(AMIGAOS_32_JSON)
}

/// The shipped AmigaOS 3.9 recipe — **three components, and the boot that
/// this comment used to be waiting for has happened** (2026-08-19, Task 8).
///
/// `workbench-base` places the disc's `OS-VERSION3.9/WORKBENCH3.5` tree,
/// `locale-base` its `OS-VERSION3.9/LOCALE` tree (ART-162), and
/// `workbench-39` — declared last, `required`, overriding both — lays the
/// disc's `OS-VERSION3.9/WORKBENCH3.9` **overlay** on top. That third one is
/// exactly what the boot said was missing: without it the tree reached a
/// clean Workbench but reported `Workbench 44.5 (18-Aug-00)` and its
/// Startup-Sequence failed on its first command, `C:LoadMonDrvs`. With it,
/// the same tree under the same licensed V40 ROM reports
/// `Kickstart 40.68, Workbench 45.1 (13-Nov-00)`. See **ART-169** and
/// `the_39_overlay_is_declared_last_required_and_over_both_layers`, the test
/// that keeps all four of those facts pinned.
///
/// What still waits for a run rather than a judgement: `WBStartup` from the
/// 3.5 layer, and `T` (see the recipe file's own `_why_no_T`). CLAUDE.md's
/// "don't claim support that isn't implemented and tested" rule (spec §89)
/// is what keeps them out until a boot asks for them.
///
/// **All-caps `from` paths, on purpose (Task 4's real run against the
/// owner's own disc — see `apply.rs`'s `build_the_real_39_tree_when_asked`).**
/// The synthetic fixtures every earlier task tested against always build a
/// Joliet tree, so the recipe was originally written in the mixed case a
/// Joliet name carries. The owner's real `AmigaOS39.iso` carries no Joliet
/// supplementary descriptor at all — only a Primary tree, whose names
/// ISO9660 keeps uppercase (`CdSource`/`IsoImage::open` docs this: "Joliet
/// wins when it is there"). Every `from` here was refused as
/// `MediaPathMissing` under the mixed-case spelling before this fix; `to`
/// stays mixed-case, matching AmigaDOS convention, since it names a
/// destination in the tree ART writes, never a path resolved against the
/// disc.
pub fn amigaos_39() -> CoreResult<Recipe> {
    parse(AMIGAOS_39_JSON)
}

/// Every release ART ships a recipe for, in the order a picker should list
/// them.
///
/// The strings are the recipes' own `release` values, and
/// `every_offered_release_resolves_to_a_recipe` pins that: this list and the
/// recipe files are two halves of one fact, and a release in only one of them
/// is a release the user either cannot reach or cannot install.
pub fn releases() -> &'static [&'static str] {
    &["AmigaOS 3.2", "AmigaOS 3.9"]
}

/// The shipped recipe for `release`.
///
/// An unrecognised name is refused rather than defaulted. A default here
/// would mean a caller asking for one operating system and getting another
/// written onto their volume — the failure this project's §92 pipeline
/// exists to prevent.
pub fn by_release(release: &str) -> CoreResult<Recipe> {
    match release {
        "AmigaOS 3.2" => amigaos_32(),
        "AmigaOS 3.9" => amigaos_39(),
        other => Err(CoreError::InvalidInput(format!(
            "ART ships no install recipe for {other}"
        ))),
    }
}

/// The `overrides` any shipped component declares, whichever kind of recipe
/// ships it — a release's own component or a package (ART-170).
///
/// **Two catalogues, one question.** `overrides` names components across the
/// boundary in both directions: `locale-turkish` (a package) declares
/// `locale-base` (a release component), and `workbench-39` (a release
/// component) declares `workbench-base` (another). Anything that reads the
/// field therefore has to look in both places, which
/// `every_override_names_a_component_that_exists` already learnt for the
/// *validation* side — this is the same union, made available to the code
/// that reads the field at runtime instead of only to the test that checks
/// it. `collide::declared_override` used `package::by_id` alone and so could
/// answer for a package and refuse, by name, for a recipe component: a
/// release's own layering (`workbench-39` over `workbench-base`, the change
/// that makes a tree AmigaOS 3.9) could not be previewed at all.
///
/// Releases are searched before packages, and the two id spaces are asserted
/// disjoint by `no_id_is_claimed_by_both_a_release_and_a_package` — so the
/// order is a statement about determinism rather than a precedence rule
/// anything relies on.
///
/// `None` means no shipped recipe of either kind declares this id. The
/// caller decides what that is: `declared_override` still refuses by name,
/// because an id that resolves to nothing is an inconsistency in whatever
/// built the item, not a fact about the file.
pub fn shipped_component_overrides(id: &str) -> Option<Vec<String>> {
    for release in releases() {
        // A shipped recipe that does not parse is a bug in shipped data, not
        // a reason to answer "no such component" — every other reader of
        // these files treats it that way, and swallowing it here would turn
        // a broken recipe into a silent `declared: false` on every row.
        let recipe = by_release(release).unwrap_or_else(|e| {
            panic!("the shipped {release} recipe must parse and validate: {e}")
        });
        if let Some(component) = recipe.component(id) {
            return Some(component.overrides.clone());
        }
    }
    let packages =
        super::package::packages().expect("the shipped packages must parse and validate");
    packages
        .into_iter()
        .find(|package| package.id == id)
        .map(|package| package.component.overrides)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::osinstall::{Component, Condition, PathRule, RuleKind};
    use std::collections::HashMap;

    fn recipe() -> Recipe {
        parse(AMIGAOS_32_JSON).expect("the shipped 3.2 recipe must parse and validate")
    }

    /// The shipped JSON, deserialised **without** going through `validate`.
    /// Used only by tests that exist to police the data independently of
    /// whatever `validate` happens to check — `recipe()` calls `parse`,
    /// which runs `validate` first and panics via `expect` on any failure,
    /// so a test built on `recipe()` alone can never actually observe a bad
    /// name or an escaping path: `validate` would already have turned it
    /// into a panicked `expect` before the test's own assertion ever ran.
    fn raw_recipe() -> Recipe {
        serde_json::from_str(AMIGAOS_32_JSON)
            .expect("the shipped 3.2 recipe must at least deserialise")
    }

    /// Every recipe this project ships, parsed and validated, paired with a
    /// label for assertion messages.
    ///
    /// **Driven from [`super::releases`] and [`super::by_release`], not
    /// hand-listed.** The earlier version of this function was a literal
    /// `vec!["AmigaOS 3.2", "AmigaOS 3.9"]` guarded only by a doc comment
    /// saying a new recipe file had to be added here too — nothing enforced
    /// that, so 3.9 shipped with one component and the collision test below
    /// stayed green by construction, dormant rather than passing, right up
    /// to the day a second component gave it something to actually check.
    /// Now the only way to make a recipe reachable from the UI at all is to
    /// add it to `releases()` (`by_release` refuses anything not named
    /// there), and that same edit is what puts it in this list — there is no
    /// second place to remember.
    ///
    /// **Widened for packages (Task 4).** Every shipped package's own
    /// component is appended too, each as its own single-component pseudo
    /// `Recipe` labelled by the package's id, driven directly from
    /// [`super::super::package::packages`] rather than a second hand-typed
    /// list — the same "one list, no second place to remember" reasoning
    /// this function's doc comment already gives for releases, so a fourth
    /// package JSON is inside these four invariant tests the moment it is
    /// added to `package.rs`'s own shipped list. A package's `overrides`
    /// therefore only resolves against its *own* component here — never a
    /// release recipe's — which is why every shipped package ships an empty
    /// `overrides` today; see `package.rs` module doc for why that is
    /// correct rather than a gap.
    fn shipped_recipes() -> Vec<(String, Recipe)> {
        let mut all: Vec<(String, Recipe)> = super::releases()
            .iter()
            .map(|&release| {
                (
                    release.to_string(),
                    super::by_release(release).unwrap_or_else(|e| {
                        panic!("the shipped {release} recipe must parse and validate: {e}")
                    }),
                )
            })
            .collect();

        for package in
            super::super::package::packages().expect("the shipped packages must parse and validate")
        {
            all.push((
                package.id.clone(),
                Recipe {
                    release: package.id.clone(),
                    components: vec![package.component],
                },
            ));
        }
        all
    }

    /// [`shipped_recipes`]'s own raw counterpart — see [`raw_recipe`]'s doc
    /// comment for why a test that means to police `validate` itself has to
    /// deserialise directly rather than go through `parse`.
    ///
    /// Also driven from [`super::releases`], for the same reason
    /// `shipped_recipes` is: every name `releases()` offers has to resolve
    /// here too. Deserialising raw needs the literal JSON text, which
    /// `by_release` does not hand back (it returns a validated `Recipe`), so
    /// `raw_json` below is the one place that still pairs a release name
    /// with its constant by hand — kept to a single small match, right next
    /// to the list it must stay in step with, so a release added to
    /// `releases()` and forgotten here fails loudly (a panic from every test
    /// that calls this) rather than silently going unchecked the way 3.9
    /// did before any of this existed.
    ///
    /// Widened for packages the same way [`shipped_recipes`] is, and for the
    /// same reason: [`super::super::package::raw_packages`] deserialises
    /// without calling [`validate_component`], so a test built on this
    /// function actually exercises `validate_component` rather than trusting
    /// it already ran.
    fn raw_shipped_recipes() -> Vec<(String, Recipe)> {
        fn raw_json(release: &str) -> &'static str {
            match release {
                "AmigaOS 3.2" => AMIGAOS_32_JSON,
                "AmigaOS 3.9" => AMIGAOS_39_JSON,
                other => panic!(
                    "'{other}' is offered by releases() but raw_shipped_recipes() has no raw \
                     JSON for it — add it beside by_release's own match arm"
                ),
            }
        }

        let mut all: Vec<(String, Recipe)> = super::releases()
            .iter()
            .map(|&release| {
                (
                    release.to_string(),
                    serde_json::from_str(raw_json(release)).unwrap_or_else(|e| {
                        panic!("the shipped {release} recipe must at least deserialise: {e}")
                    }),
                )
            })
            .collect();

        for package in super::super::package::raw_packages() {
            all.push((
                package.id.clone(),
                Recipe {
                    release: package.id.clone(),
                    components: vec![package.component],
                },
            ));
        }
        all
    }

    #[test]
    fn the_shipped_recipe_parses() {
        let recipe = recipe();
        assert_eq!(recipe.release, "AmigaOS 3.2");
        assert!(recipe.component("workbench-base").is_some());
    }

    #[test]
    fn workbench_base_is_required() {
        assert!(recipe().component("workbench-base").unwrap().required);
    }

    /// `Workbench3.2.adf` has no `L/` **at all** — its own Startup-Sequence
    /// says `IF NOT EXISTS SYS:L / Assign L: Extras3.2:L DEFER`. So the tree's
    /// `L` has to come from Extras, and a recipe that forgot it would produce
    /// a system with no handlers.
    #[test]
    fn l_comes_from_extras_and_not_from_workbench() {
        let recipe = recipe();
        let extras = recipe.component("extras").unwrap();
        assert!(
            extras.rules.iter().any(|r| r.to == "L"),
            "extras must carry L"
        );

        let base = recipe.component("workbench-base").unwrap();
        assert!(!base.rules.iter().any(|r| r.to == "L"));
    }

    /// The measurement this whole design rests on.
    #[test]
    fn the_modules_component_takes_loadmodule_and_not_the_rest_of_c() {
        let modules = recipe().component("modules-a1200").unwrap().clone();
        let from_c: Vec<&str> = modules
            .rules
            .iter()
            .filter(|r| r.from.starts_with("C/") || r.from == "C")
            .map(|r| r.from.as_str())
            .collect();
        assert_eq!(
            from_c,
            vec!["C/LoadModule"],
            "thirteen of ModulesA1200's fourteen C/ commands are older copies \
             of Workbench3.2's own; taking the drawer downgrades them"
        );
    }

    /// `Locale.adf` is the base (Catalogs, Countries, Support — no Languages);
    /// `Locale-XX.adf` are the per-language disks. Two components, not one.
    #[test]
    fn the_locale_base_is_separate_from_the_language_disks() {
        let recipe = recipe();
        let base = recipe.component("locale-base").unwrap();
        assert_eq!(base.media, "Locale");
        assert!(base.rules.iter().any(|r| r.to == "Locale/Countries"));

        let english = recipe.component("locale-en").unwrap();
        assert_eq!(english.media, "Locale-EN");
        assert!(english
            .rules
            .iter()
            .any(|r| r.to.starts_with("Locale/Languages")));
    }

    /// Deliberately built on `raw_recipe()`, not `recipe()`: `recipe()` goes
    /// through `parse`, which runs `validate` first and would already have
    /// panicked the `expect` on any name this test exists to catch —
    /// checking the *shipped data* directly, independent of whether
    /// `validate` itself has a bug, is the point.
    #[test]
    fn every_destination_is_a_name_amigados_can_store() {
        for (release, recipe) in raw_shipped_recipes() {
            for component in &recipe.components {
                for rule in &component.rules {
                    for segment in rule.to.split('/') {
                        crate::core::volume::write::dir::check_name(segment).unwrap_or_else(|e| {
                            panic!(
                                "{release} '{}': destination '{}' segment '{segment}': {e}",
                                component.id, rule.to
                            )
                        });
                    }
                }
            }
        }
    }

    /// Only `File` rules are checked here. A `Subtree` destination is a
    /// merge point, not a claim: Workbench, Extras and Classes all
    /// legitimately contribute to `Devs/`, and every language disk
    /// contributes a different `.language` file to `Locale/Languages` —
    /// nothing is being overwritten, so nothing needs an `overrides`
    /// declaration. What this test actually guards against is two
    /// components writing the same **file**, which is exactly what taking
    /// `ModulesA1200`'s whole `C/` would have done. The expanded, file-level
    /// check over the media a real install folder actually has belongs to
    /// `plan()` (a later task) — this one only proves the shipped recipe is
    /// internally consistent about the files it names.
    ///
    /// Checked symmetrically, over **every** claimant of a path at once,
    /// rather than by walking the array and comparing each component only to
    /// whichever one happened to claim the path most recently: that
    /// alternative makes the check depend on the components' array
    /// position, and reordering a *set* — which carries no meaning of its
    /// own — must never be able to flip a test from green to red.
    /// **This sees `RuleKind::File` rules only, and every rule in the
    /// shipped AmigaOS 3.9 recipe is a `Subtree`** — so for that recipe it
    /// passes *vacuously*, and would pass just as happily with every
    /// `overrides` array emptied. That is deliberate rather than an
    /// oversight (a `Subtree` destination is a merge point, not a claim, and
    /// two components legitimately merge into `C/`), but it means this test
    /// is not what protects `workbench-39`'s `overrides` from being dropped.
    /// `the_39_overlay_is_declared_last_required_and_over_both_layers`
    /// below is; a guard that passes vacuously is worse than no guard,
    /// because it reads like protection (Task 8 fix round 2, F1).
    #[test]
    fn no_two_components_claim_one_destination_without_declaring_it() {
        for (release, recipe) in shipped_recipes() {
            let mut claimants: HashMap<&str, Vec<&Component>> = HashMap::new();
            for component in &recipe.components {
                for rule in component.rules.iter().filter(|r| r.kind == RuleKind::File) {
                    claimants
                        .entry(rule.to.as_str())
                        .or_default()
                        .push(component);
                }
            }

            for (path, claiming) in &claimants {
                if claiming.len() < 2 {
                    continue;
                }
                let resolved = claiming.iter().any(|winner| {
                    claiming
                        .iter()
                        .filter(|other| other.id != winner.id)
                        .all(|other| winner.overrides.iter().any(|o| o == &other.id))
                });
                assert!(
                    resolved,
                    "{release}: '{path}' is claimed by {:?} and none of them declares an \
                     override over all the rest",
                    claiming.iter().map(|c| c.id.as_str()).collect::<Vec<_>>()
                );
            }
        }
    }

    /// **ART-169's fix, pinned.** The AmigaOS 3.9 recipe is two layers off
    /// one disc: `workbench-base` places `OS-Version3.9/Workbench3.5` and
    /// `workbench-39` places the `Workbench3.9` overlay on top of it. Four
    /// separate things have to stay true for that to keep working, and
    /// nothing else in this file or in `plan.rs` can see any of them:
    ///
    /// - **`workbench-39` exists.** Deleting it takes the tree back to
    ///   Workbench 44.5 and a Startup-Sequence that fails on its first
    ///   command (ART-169's own evidence).
    /// - **It is declared last.** `plan()` emits items in recipe-declaration
    ///   order and `apply`'s writer lets the last writer win, so moving this
    ///   component up the array silently reverts the fix for all 40 files the
    ///   two layers really do collide on.
    /// - **It is `required`.** An overlay a user can switch off is a way to
    ///   build a tree that calls itself 3.9 and is not.
    /// - **It overrides both** `workbench-base` *and* `locale-base`.
    ///   `plan::detect_collisions` refuses an undeclared claim at plan time,
    ///   so dropping either one turns every real install into a refusal —
    ///   but only for a plan that actually resolves media, which no unit
    ///   test does.
    ///
    /// The rule set is pinned to the disc's own top level (measured with
    /// `7z l -slt` against the owner's `AmigaOS39.iso`, 2026-08-19: thirteen
    /// drawers, and *not* the sixteen `Workbench3.5` carries — no `L`, no
    /// `Expansion`, no `Rexxc`, no `T`, and a `Locale` that 3.5 has not).
    /// A rule vanishing here is a drawer of the 3.9 layer that stops being
    /// installed, which nothing downstream would report.
    ///
    /// Only a boot notices any of this otherwise, and a boot is not
    /// something CI does.
    #[test]
    fn the_39_overlay_is_declared_last_required_and_over_both_layers() {
        let recipe = super::amigaos_39().expect("the shipped 3.9 recipe must parse and validate");

        let ids: Vec<&str> = recipe.components.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["workbench-base", "locale-base", "workbench-39"],
            "the 3.9 overlay must be the last component declared — recipe order is what decides              which layer writes last (see this test's own doc comment)"
        );

        let overlay = recipe
            .component("workbench-39")
            .expect("named in the assertion above");
        assert!(
            overlay.required,
            "the 3.9 overlay is not optional: a tree without it is AmigaOS 3.5 calling itself 3.9"
        );

        let mut overrides: Vec<&str> = overlay.overrides.iter().map(String::as_str).collect();
        overrides.sort_unstable();
        assert_eq!(
            overrides,
            vec!["locale-base", "workbench-base"],
            "the overlay collides with both layers for real — 12 of its 29 C commands replace a              3.5 command, and 32 of its Locale/Countries files are ones locale-base also places"
        );

        // The disc's own `OS-Version3.9/Workbench3.9` top level, in the
        // spelling a Joliet-less Primary tree forces on `from` and the
        // AmigaDOS convention `to` keeps.
        let expected: Vec<(&str, &str)> = vec![
            ("OS-VERSION3.9/WORKBENCH3.9/C", "C"),
            ("OS-VERSION3.9/WORKBENCH3.9/CLASSES", "Classes"),
            ("OS-VERSION3.9/WORKBENCH3.9/DEVS", "Devs"),
            ("OS-VERSION3.9/WORKBENCH3.9/FONTS", "Fonts"),
            ("OS-VERSION3.9/WORKBENCH3.9/LIBS", "Libs"),
            ("OS-VERSION3.9/WORKBENCH3.9/LOCALE", "Locale"),
            ("OS-VERSION3.9/WORKBENCH3.9/PREFS", "Prefs"),
            ("OS-VERSION3.9/WORKBENCH3.9/S", "S"),
            ("OS-VERSION3.9/WORKBENCH3.9/STORAGE", "Storage"),
            ("OS-VERSION3.9/WORKBENCH3.9/SYSTEM", "System"),
            ("OS-VERSION3.9/WORKBENCH3.9/TOOLS", "Tools"),
            ("OS-VERSION3.9/WORKBENCH3.9/UTILITIES", "Utilities"),
            ("OS-VERSION3.9/WORKBENCH3.9/WBSTARTUP", "WBStartup"),
        ];
        let actual: Vec<(&str, &str)> = overlay
            .rules
            .iter()
            .map(|rule| (rule.from.as_str(), rule.to.as_str()))
            .collect();
        assert_eq!(
            actual, expected,
            "the overlay's rules must match the disc's own Workbench3.9 top level, drawer for              drawer — a rule missing here is a drawer of the 3.9 layer that stops being installed"
        );
        assert!(
            overlay
                .rules
                .iter()
                .all(|rule| rule.kind == RuleKind::Subtree),
            "every overlay rule takes a whole drawer"
        );
    }

    /// The ModulesA1200 lesson, generalised. Four disks in the 3.2 set
    /// repeat `workbench-base`'s own `C/` almost entirely, and a component
    /// that lazily takes the whole drawer passes the collision test above
    /// (which sees `File` rules only) while downgrading the user's
    /// commands.
    ///
    /// **Widened from a hardcoded list of five 3.2 component ids (m1 of the
    /// final whole-branch review).** The original never saw `workbench-39`
    /// — thirteen `Subtree` rules over drawers `workbench-base` owns, which
    /// is exactly and literally the shape it describes — nor either
    /// BoingBag, whose every rule is the same. The round shipped the
    /// described shape three times and the "generalised" guard saw none of
    /// them, which is the "reads like protection" pattern Task 8's F1 was
    /// filed for.
    ///
    /// So the set is derived, not written down: **every** component of
    /// **every** shipped release recipe, plus **every** shipped package's
    /// own component (a package legitimately overlays a release's drawers,
    /// so it has to be asked the same question), checked against the
    /// drawers that recipe's own base layer takes — the first `required`
    /// component with `Subtree` rules, read from the data rather than named.
    ///
    /// **What the exemption is, and what it is not.** Taking a whole drawer
    /// the base owns is allowed exactly when the component *declares* an
    /// override over the base: that is what separates a deliberate layer
    /// (`workbench-39` over 3.9's base, a BoingBag over both) or a
    /// supplementary disk that really does merge (`extras`, `classes`,
    /// `glowicons`) from a toolkit disk that took the easy route. It does
    /// **not** mean a declaring component cannot still downgrade something:
    /// nothing in a recipe can know whose copy of a file is newer. That is
    /// measured rather than declared — `plan::detect_collisions` and
    /// `collide::preview`'s downgrade rows are what catch it, against real
    /// media, and `layer_the_real_39_overlay_when_asked` is where the 3.9
    /// overlay's 19 upgrades and 0 downgrades were actually counted. This
    /// test only guarantees that no component takes a base drawer *without
    /// saying so*.
    #[test]
    fn no_toolkit_disk_takes_a_whole_drawer_the_base_already_owns() {
        let packages = super::super::package::packages().expect("the shipped packages must parse");

        for release in super::releases() {
            let recipe = super::by_release(release)
                .unwrap_or_else(|e| panic!("the shipped {release} recipe must parse: {e}"));

            // The base layer, from the data: the first `required` component
            // that actually takes drawers. Named nowhere, so a recipe whose
            // base is called something other than `workbench-base` is
            // policed just the same.
            let base = recipe
                .components
                .iter()
                .find(|c| c.required && c.rules.iter().any(|r| r.kind == RuleKind::Subtree))
                .unwrap_or_else(|| {
                    panic!("{release}: a release recipe must declare a required base layer")
                });
            let owned: std::collections::HashSet<&str> = base
                .rules
                .iter()
                .filter(|r| r.kind == RuleKind::Subtree)
                .map(|r| r.to.as_str())
                .collect();

            let claimants = recipe
                .components
                .iter()
                .chain(packages.iter().map(|p| &p.component));

            for component in claimants {
                if component.id == base.id {
                    continue;
                }
                let declared = component.overrides.iter().any(|over| over == &base.id);
                for rule in &component.rules {
                    if rule.kind != RuleKind::Subtree || !owned.contains(rule.to.as_str()) {
                        continue;
                    }
                    assert!(
                        declared,
                        "{release}: {} takes the whole '{}' drawer, which {} owns, without \
                         declaring an override over it — take the files that are actually \
                         new instead, or say outright that this layer replaces them",
                        component.id, rule.to, base.id
                    );
                }
            }
        }
    }

    /// Every component id anywhere ART ships — every release recipe's own
    /// components, plus every package's (each a standalone id, since a
    /// package wraps exactly one component under its own id). Used by
    /// [`every_override_names_a_component_that_exists`] to resolve an
    /// `overrides` entry that crosses out of its own recipe — a package
    /// legitimately overriding a release's component, `boingbag-39-1`
    /// overriding `workbench-base` being the real case a fix round found
    /// (review, Task 4): [`shipped_recipes`] wraps each package in its own
    /// one-component pseudo-`Recipe`, so `recipe.component(over)` alone can
    /// never see across that boundary, however correct the override itself
    /// is.
    fn all_shipped_component_ids() -> std::collections::HashSet<String> {
        let mut ids = std::collections::HashSet::new();
        for release in super::releases() {
            let recipe = super::by_release(release).unwrap_or_else(|e| {
                panic!("the shipped {release} recipe must parse and validate: {e}")
            });
            ids.extend(recipe.components.into_iter().map(|c| c.id));
        }
        for package in
            super::super::package::packages().expect("the shipped packages must parse and validate")
        {
            ids.insert(package.id);
        }
        ids
    }

    /// Every component id a **release recipe** ships — packages
    /// deliberately excluded, unlike [`all_shipped_component_ids`].
    /// `Package::requires_components` says "this recipe component has to be
    /// switched on", and a package is not something `plan()` can switch on:
    /// resolving one against a package id would let `requires_components:
    /// ["boingbag-39-1"]` pass this test and then refuse forever at plan
    /// time, since no `components_on` can ever contain it.
    fn release_component_ids() -> std::collections::HashSet<String> {
        let mut ids = std::collections::HashSet::new();
        for release in super::releases() {
            let recipe = super::by_release(release).unwrap_or_else(|e| {
                panic!("the shipped {release} recipe must parse and validate: {e}")
            });
            ids.extend(recipe.components.into_iter().map(|c| c.id));
        }
        ids
    }

    #[test]
    fn every_required_component_a_package_names_is_a_real_recipe_component() {
        let ids = release_component_ids();
        for package in
            super::super::package::packages().expect("the shipped packages must parse and validate")
        {
            for need in &package.requires_components {
                assert!(
                    ids.contains(need.as_str()),
                    "{}: no release recipe ships a component '{need}'",
                    package.id
                );
            }
        }
    }

    /// **ART-170's precondition.** `shipped_component_overrides` searches
    /// releases and then packages, and answers with the first id it finds.
    /// That is only safe while no id is claimed by both — otherwise one
    /// component's `overrides` would silently answer for another's, and the
    /// `declared` column would be right about the wrong component.
    ///
    /// A data invariant rather than a guard on the fix: it holds today and
    /// must keep holding, which is exactly the kind of thing nothing else
    /// would notice breaking.
    #[test]
    fn no_id_is_claimed_by_both_a_release_and_a_package() {
        let release_ids = release_component_ids();
        let packages = super::super::package::packages()
            .expect("the shipped packages must parse and validate");
        // Not vacuous: there are ids on both sides to collide.
        assert!(!release_ids.is_empty() && !packages.is_empty());
        for package in packages {
            assert!(
                !release_ids.contains(&package.id),
                "'{}' is both a package and a release recipe's component",
                package.id
            );
        }
    }

    /// The resolver itself, over both catalogues and over an id in neither.
    #[test]
    fn shipped_component_overrides_reads_releases_and_packages_alike() {
        assert_eq!(
            super::shipped_component_overrides("workbench-39"),
            Some(vec![
                "workbench-base".to_string(),
                "locale-base".to_string()
            ]),
            "a release recipe's own component"
        );
        assert_eq!(
            super::shipped_component_overrides("locale-turkish"),
            Some(vec!["locale-base".to_string()]),
            "a package, which is all this used to be able to answer for"
        );
        assert_eq!(
            super::shipped_component_overrides("workbench-base"),
            Some(Vec::new()),
            "a real component that declares none is not the same as no component"
        );
        assert_eq!(
            super::shipped_component_overrides("not-shipped-at-all"),
            None
        );
    }

    #[test]
    fn every_override_names_a_component_that_exists() {
        let ids = all_shipped_component_ids();
        for (release, recipe) in shipped_recipes() {
            for component in &recipe.components {
                for over in &component.overrides {
                    assert!(
                        ids.contains(over.as_str()),
                        "{release}: {}: no such component '{over}'",
                        component.id
                    );
                }
            }
        }
    }

    /// Also built on `raw_recipe()` — see the comment on
    /// `every_destination_is_a_name_amigados_can_store` above.
    #[test]
    fn no_rule_escapes_the_tree() {
        for (release, recipe) in raw_shipped_recipes() {
            for component in &recipe.components {
                for rule in &component.rules {
                    assert!(
                        !rule.to.starts_with('/'),
                        "{release}: {}: '{}' is absolute",
                        component.id,
                        rule.to
                    );
                    assert!(
                        !rule.to.split('/').any(|s| s == ".."),
                        "{release}: {}: '{}' climbs",
                        component.id,
                        rule.to
                    );
                }
            }
        }
    }

    // ---- validate(): negative coverage ----
    //
    // The shipped recipe never trips any of these four branches, so without
    // tests that construct bad data directly, deleting any one of them would
    // leave all ten (now more) tests above green. Small inline JSON strings,
    // one per branch.

    #[test]
    fn two_components_sharing_an_id_are_refused() {
        let json = r#"{
            "release": "X",
            "components": [
                { "id": "a", "media": "M1", "rules": [] },
                { "id": "a", "media": "M2", "rules": [] }
            ]
        }"#;
        let err = parse(json).unwrap_err();
        assert!(err.to_string().contains("share the id"), "{err}");
    }

    #[test]
    fn a_component_naming_no_media_is_refused() {
        let json = r#"{
            "release": "X",
            "components": [
                { "id": "a", "media": "   ", "rules": [] }
            ]
        }"#;
        let err = parse(json).unwrap_err();
        assert!(err.to_string().contains("names no media"), "{err}");
    }

    #[test]
    fn a_destination_with_a_character_amigados_cannot_store_is_refused() {
        let json = r#"{
            "release": "X",
            "components": [
                { "id": "a", "media": "M", "rules": [
                    { "from": "X", "to": "Bad:Name", "kind": "file" }
                ] }
            ]
        }"#;
        let err = parse(json).unwrap_err();
        assert!(
            err.to_string().contains("Bad:Name") || err.to_string().contains(':'),
            "{err}"
        );
    }

    #[test]
    fn a_destination_that_climbs_out_of_the_tree_is_refused() {
        let json = r#"{
            "release": "X",
            "components": [
                { "id": "a", "media": "M", "rules": [
                    { "from": "X", "to": "../evil", "kind": "file" }
                ] }
            ]
        }"#;
        let err = parse(json).unwrap_err();
        assert!(err.to_string().contains("leaves the tree"), "{err}");
    }

    /// `from` is human-typed data too, and `validate` didn't used to look at
    /// it at all — `from: ""` has to stay legal (it means the media's own
    /// root, for `fonts` and `backdrops`), but `"../.."` must not sail
    /// through just because nobody ever checked the media-side half of a
    /// rule.
    #[test]
    fn a_source_path_that_climbs_out_of_the_media_is_refused() {
        let json = r#"{
            "release": "X",
            "components": [
                { "id": "a", "media": "M", "rules": [
                    { "from": "../..", "to": "Somewhere", "kind": "subtree" }
                ] }
            ]
        }"#;
        let err = parse(json).unwrap_err();
        assert!(err.to_string().contains("leaves the tree"), "{err}");
    }

    #[test]
    fn an_empty_from_is_legal_it_means_the_medias_own_root() {
        let json = r#"{
            "release": "X",
            "components": [
                { "id": "a", "media": "M", "rules": [
                    { "from": "", "to": "Fonts", "kind": "subtree" }
                ] }
            ]
        }"#;
        assert!(parse(json).is_ok());
    }

    // ---- three reasoned data decisions, asserted rather than assumed ----

    /// A typo in the major would otherwise be silent — the recipe would
    /// still parse and validate, and only misbehave for the one machine
    /// class this condition exists to gate.
    #[test]
    fn modules_a1200_only_applies_below_kickstart_major_47() {
        let modules = recipe().component("modules-a1200").unwrap().clone();
        assert_eq!(
            modules.condition,
            Some(Condition::RomOlderThan { major: 47 })
        );
    }

    /// `backdrops` stays off on purpose — the brief is explicit that where
    /// the real installer places these wallpapers has not been established,
    /// and this project does not guess at destinations. A flipped
    /// `available` would ship that guess silently.
    #[test]
    fn backdrops_and_update_3_2_1_are_not_yet_available() {
        let recipe = recipe();
        assert!(!recipe.component("update-3.2.1").unwrap().available);
    }

    /// **The destination was measured, so `backdrops` is on (ART-127).**
    ///
    /// It shipped `available: false` with a comment saying it would stay off
    /// "until somebody measures where the real installer places wallpapers" —
    /// which was the right call while the answer was a guess. Booting the
    /// tree measured it: AmigaOS 3.2's own Preferences, running on a V47
    /// A1200 ROM, asked for `Sys:Prefs/Presets/Backdrops/default_pal.iff` by
    /// name. That is the OS naming its own path, not ART choosing one, and
    /// `Backdrops3.2` carries `default_pal.iff` at its root.
    #[test]
    fn backdrops_go_where_the_running_system_asked_for_them() {
        let recipe = recipe();
        let component = recipe.component("backdrops").unwrap();
        assert!(component.available);
        assert_eq!(
            component.rules,
            vec![PathRule {
                from: String::new(),
                to: "Prefs/Presets/Backdrops".into(),
                kind: RuleKind::Subtree,
            }],
            "the whole disk, at the path Preferences asked for"
        );
    }

    /// **ART-127 — the two libraries without which Workbench does not start.**
    ///
    /// AmigaOS 3.2's A1200 ROM carries neither `icon.library` nor
    /// `workbench.library`; the system volume has to. Neither is on
    /// `Workbench3.2` — whose `Libs` drawer holds 23 libraries and not these
    /// two — so the `Libs` subtree rule that looks like it would cover
    /// everything does not cover them. They are on `Install3.2`, which the
    /// recipe named no component for at all: the disk had been written off as
    /// "the OS's own boot floppy, not component media".
    ///
    /// Found by booting what G5 built, one requester at a time: with a V47
    /// ROM the tree reached Workbench startup and stopped on *"Please insert
    /// a volume containing LIBS/icon.library in any drive"*, and once that
    /// was supplied, on `LIBS/workbench.library`. No fixture could have found
    /// either, and neither could any amount of reading the recipe — every
    /// rule in it was correct about the disk it names.
    ///
    /// The other three libraries in that drawer (`iffparse`, `locale`,
    /// `version`) are deliberately **not** taken: `Workbench3.2` ships those
    /// itself, and a second component writing the same destination is the
    /// collision ART-112 was.
    #[test]
    fn the_libraries_workbench_does_not_carry_come_off_the_install_disk() {
        let recipe = recipe();
        let component = recipe
            .component("install-libs")
            .expect("a component supplying the LIBS: the ROM does not carry");

        assert_eq!(component.media, "Install3.2");
        assert!(
            component.required,
            "Workbench does not start without these, so a tree built without \
             them is not a working system and ART should say so rather than \
             build one"
        );
        assert_eq!(
            component.rules,
            vec![
                PathRule {
                    from: "Libs/icon.library".into(),
                    to: "Libs/icon.library".into(),
                    kind: RuleKind::File,
                },
                PathRule {
                    from: "Libs/workbench.library".into(),
                    to: "Libs/workbench.library".into(),
                    kind: RuleKind::File,
                },
            ]
        );

        // And the reason it has to be its own component: the disk that looks
        // like it should carry them does not.
        let workbench = recipe.component("workbench-base").unwrap();
        for name in ["Libs/icon.library", "Libs/workbench.library"] {
            assert!(
                workbench.rules.iter().all(|rule| rule.from != name),
                "if Workbench3.2 ever does carry {name}, this component is the \
                 one to remove — not a second copy to add"
            );
        }
    }

    #[test]
    fn modules_a1200_is_in_its_own_exclusive_group() {
        assert_eq!(
            recipe()
                .component("modules-a1200")
                .unwrap()
                .exclusive_group
                .as_deref(),
            Some("modules")
        );
    }

    /// One loop over all fifteen, so a mistyped volume name is caught here
    /// rather than surfacing only as a `MediaMissing` refusal at install
    /// time, on whichever machine happens to have that particular disk.
    #[test]
    fn every_locale_disk_has_the_right_media_and_rule_shape() {
        let recipe = recipe();
        for code in [
            "DE", "DK", "EN", "ES", "FR", "GR", "IT", "NL", "NO", "PL", "PT", "RU", "SE", "TR",
            "UK",
        ] {
            let id = format!("locale-{}", code.to_lowercase());
            let component = recipe
                .component(&id)
                .unwrap_or_else(|| panic!("missing locale component '{id}'"));
            assert_eq!(component.media, format!("Locale-{code}"));
            assert_eq!(component.rules.len(), 2, "{id}");
            assert!(
                component.rules.iter().any(|r| r.from == "Languages"
                    && r.to == "Locale/Languages"
                    && r.kind == RuleKind::Subtree),
                "{id} must carry Languages -> Locale/Languages"
            );
            assert!(
                component.rules.iter().any(|r| r.from == "Help"
                    && r.to == "Locale/Help"
                    && r.kind == RuleKind::Subtree),
                "{id} must carry Help -> Locale/Help"
            );
        }
    }

    // ---- AmigaOS 3.9 — first component (base tree) ----

    /// The shipped recipe parses and validates — the same bar the 3.2 recipe
    /// meets, and the reason a malformed one cannot ship.
    #[test]
    fn the_39_recipe_is_valid() {
        let recipe = parse(AMIGAOS_39_JSON).expect("the shipped 3.9 recipe validates");
        assert!(
            recipe.components.iter().any(|c| c.id == "workbench-base"),
            "the base component is what makes a tree at all"
        );
    }

    /// A CD's paths are deeper than a floppy's, and every segment still has to
    /// be a name AmigaDOS could store — `validate_path` is the gate and this
    /// pins that the deeper shape passes it.
    #[test]
    fn the_39_recipes_deep_media_paths_pass_the_name_gate() {
        let recipe = parse(AMIGAOS_39_JSON).unwrap();
        let base = recipe
            .components
            .iter()
            .find(|c| c.id == "workbench-base")
            .unwrap();
        assert!(
            base.rules
                .iter()
                .any(|r| r.from == "OS-VERSION3.9/WORKBENCH3.5/C"),
            "the rules are written against the CD's own layout — all-caps, \
             because the owner's real disc (Task 4) carries no Joliet \
             descriptor at all, so `CdSource` reads its Primary tree, whose \
             names ISO9660 keeps uppercase"
        );
    }

    /// Every component names media the recipe itself declares — the rule
    /// `validate` already enforces, asserted for this recipe specifically.
    #[test]
    fn the_39_recipe_names_one_medium() {
        let recipe = parse(AMIGAOS_39_JSON).unwrap();
        for component in &recipe.components {
            assert_eq!(component.media, "AmigaOS3.9", "{}", component.id);
        }
    }

    /// ART-162 (Task 4 review, fix round 1). The original 3.9 recipe placed
    /// nothing from `OS-Version3.9/Locale` at all, which made the
    /// `locale-turkish` package (shipped the same task) inert: no
    /// `.language`/`.country` file, so `locale.library` could never select
    /// any non-English locale, Turkish included. Measured against the
    /// owner's own `AmigaOS39.iso`: `OS-Version3.9/Locale`'s six top-level
    /// drawers are `Catalogs`, `Countries`, `Flags`, `Help`, `Languages`,
    /// `Providers`; `locale-base` takes the four `locale.library` selection
    /// actually reads and leaves the two cosmetic/unrelated ones out
    /// (`_why_locale_base_ART_162` in `amigaos-3.9.json` has the full
    /// reasoning). `required: false` matches the shipped 3.2 recipe's own
    /// `locale-base` exactly — a tree with no Locale drawer still boots and
    /// runs, in English.
    #[test]
    fn the_39_recipe_places_a_locale_component_art_162() {
        let recipe = parse(AMIGAOS_39_JSON).unwrap();
        let locale = recipe
            .component("locale-base")
            .expect("ART-162: the 3.9 recipe must place a Locale component");
        assert!(
            !locale.required,
            "a tree with no Locale drawer still boots and runs, in English — \
             matching the shipped 3.2 recipe's own locale-base"
        );
        assert_eq!(locale.media, "AmigaOS3.9");
        assert_eq!(
            locale.rules,
            vec![
                PathRule {
                    from: "OS-VERSION3.9/LOCALE/CATALOGS".into(),
                    to: "Locale/Catalogs".into(),
                    kind: RuleKind::Subtree,
                },
                PathRule {
                    from: "OS-VERSION3.9/LOCALE/COUNTRIES".into(),
                    to: "Locale/Countries".into(),
                    kind: RuleKind::Subtree,
                },
                PathRule {
                    from: "OS-VERSION3.9/LOCALE/LANGUAGES".into(),
                    to: "Locale/Languages".into(),
                    kind: RuleKind::Subtree,
                },
                PathRule {
                    from: "OS-VERSION3.9/LOCALE/HELP".into(),
                    to: "Locale/Help".into(),
                    kind: RuleKind::Subtree,
                },
            ]
        );
    }

    // ---- Task 6 — choosing which release to install ----

    /// Every release the picker can offer must actually resolve. This is the
    /// test that fails when a recipe is added to one list and not the other.
    #[test]
    fn every_offered_release_resolves_to_a_recipe() {
        for release in super::releases() {
            let recipe = super::by_release(release)
                .unwrap_or_else(|e| panic!("{release} does not resolve: {e}"));
            assert_eq!(
                &recipe.release, release,
                "the recipe answering to {release} names itself differently"
            );
        }
    }

    /// A release nobody ships is a refusal with the name in it, not a
    /// fallback to whichever recipe happens to be first — installing 3.2
    /// because the caller asked for something unknown would write the wrong
    /// operating system onto the user's volume.
    #[test]
    fn an_unknown_release_is_refused_by_name() {
        let err = super::by_release("AmigaOS 4.1").unwrap_err();
        let text = err.to_string();
        assert!(text.contains("AmigaOS 4.1"), "got {text}");
    }

    /// Both shipped recipes are offered. A recipe that exists but is not in
    /// `releases()` is unreachable from the UI, which is how 3.9 arrived.
    #[test]
    fn both_shipped_recipes_are_offered() {
        let offered = super::releases();
        assert!(offered.contains(&"AmigaOS 3.2"), "got {offered:?}");
        assert!(offered.contains(&"AmigaOS 3.9"), "got {offered:?}");
    }
}
