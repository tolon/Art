//! The shipped recipes, as data.
//!
//! `include_str!` for the same three reasons `core/distro` uses it: reviewable
//! in a diff, shipped without a network, and unable to grow a code path.

use std::collections::HashMap;

use super::{Component, Recipe, RuleKind};
use crate::core::error::{CoreError, CoreResult};

const AMIGAOS_32_JSON: &str = include_str!("recipes/amigaos-3.2.json");
const AMIGAOS_39_JSON: &str = include_str!("recipes/amigaos-3.9.json");
const AMIGAOS_322_JSON: &str = include_str!("recipes/amigaos-3.2.2.json");

/// Every key a recipe may carry, by the level it sits at.
///
/// **A misspelled key used to be dropped in silence** (ART-183). Writing
/// `"userStartup"` for `"user_startup"` parsed cleanly to a component with no
/// startup lines, and the symptom was a tree that quietly lacked something
/// the recipe plainly asked for. `package.rs` closed the same hole for
/// packages; this is the half that was left, deliberately, until somebody
/// measured the other file's own data.
///
/// `deny_unknown_fields` is **not** the fix and was tried: every shipped
/// recipe carries `_why_…` documentation blocks — the measurements, with
/// their dates, that make a recipe reviewable in a diff — and there is no way
/// to enumerate those. So a key beginning `_` is a note to a human and
/// anything else is a person telling ART something ART did not hear.
///
/// Walked over the JSON rather than caught with `#[serde(flatten)]`, because
/// `Component` and `PathRule` derive `Eq` and a `serde_json::Value` is not
/// `Eq` — putting a catch-all on the real types would change types the whole
/// crate compares, to check a file.
const RECIPE_KEYS: &[&str] = &["release", "components", "layers", "base"];
const COMPONENT_KEYS: &[&str] = &[
    "id",
    "media",
    "rules",
    "required",
    "condition",
    "overrides",
    "user_startup",
    "activate",
    "exclusive_group",
    "available",
    "label_key",
    "layer",
    "removes",
];
const RULE_KEYS: &[&str] = &["from", "to", "kind"];
const CONDITION_KEYS: &[&str] = &["condition", "major", "resident", "minor"];
const ACTIVATION_KEYS: &[&str] = &["kind", "name"];
const LAYER_KEYS: &[&str] = &["id", "label_key"];

/// Refuse the first key that is neither known at its level nor a `_…` note.
///
/// The message names the key, where it is, and what a note looks like,
/// because the two mistakes this catches want different fixes: `userStartup`
/// is a misspelling of a real field, `why_this_is_here` is a note whose
/// author forgot the underscore.
fn check_keys(value: &serde_json::Value, allowed: &[&str], where_: &str) -> CoreResult<()> {
    let Some(object) = value.as_object() else {
        return Ok(());
    };
    // Sorted. `serde_json`'s own map is already ordered, so this is a no-op
    // today and a mutation removing it survives — said here rather than left
    // as a guard that guards nothing. It stays because `preserve_order` is a
    // real `serde_json` feature any dependency can turn on, and the day one
    // does, a file with two bad keys would start refusing by whichever key
    // its author happened to type first.
    let mut unknown: Vec<&str> = object
        .keys()
        .map(String::as_str)
        .filter(|key| !key.starts_with('_') && !allowed.contains(key))
        .collect();
    unknown.sort_unstable();
    if let Some(key) = unknown.first() {
        return Err(CoreError::Malformed {
            format: "recipe".into(),
            detail: format!(
                "{where_}: '{key}' is not a recipe key (keys are snake_case, and a note to a \
                 human must begin with '_')"
            ),
        });
    }
    Ok(())
}

/// Every level of one recipe's JSON, checked before it is trusted.
fn check_unknown_keys(json: &str) -> CoreResult<()> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| CoreError::Malformed {
            format: "recipe".into(),
            detail: e.to_string(),
        })?;

    check_keys(&value, RECIPE_KEYS, "the recipe")?;

    if let Some(layers) = value.get("layers").and_then(|l| l.as_array()) {
        for layer in layers {
            let id = layer
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("a layer with no id");
            check_keys(layer, LAYER_KEYS, &format!("layer '{id}'"))?;
        }
    }

    let Some(components) = value.get("components").and_then(|c| c.as_array()) else {
        return Ok(());
    };
    for component in components {
        // Named by its own id where it has one, so a refusal points at the
        // component a person can find rather than at an index.
        let id = component
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("a component with no id");
        check_keys(component, COMPONENT_KEYS, &format!("component '{id}'"))?;

        if let Some(condition) = component.get("condition") {
            check_keys(
                condition,
                CONDITION_KEYS,
                &format!("'{id}'\u{2019}s condition"),
            )?;
        }
        if let Some(rules) = component.get("rules").and_then(|r| r.as_array()) {
            for rule in rules {
                check_keys(rule, RULE_KEYS, &format!("a rule of '{id}'"))?;
            }
        }
        if let Some(activations) = component.get("activate").and_then(|a| a.as_array()) {
            for activation in activations {
                check_keys(
                    activation,
                    ACTIVATION_KEYS,
                    &format!("an activation of '{id}'"),
                )?;
            }
        }
    }
    Ok(())
}

/// Parse and validate a recipe, exactly as its file says — no `base` is
/// resolved, so a recipe that names one still carries only its own
/// components afterwards.
///
/// Used directly by tests that police the file's own data (`raw_recipe`,
/// `raw_shipped_recipes`) and by [`resolve_base`], which needs the base
/// recipe's components before anything is merged onto them. [`parse`] is
/// what every other caller wants.
pub fn parse_unresolved(json: &str) -> CoreResult<Recipe> {
    // Before deserialising, so a misspelling is named rather than dropped
    // (ART-183) — a component missing the thing it plainly asked for is the
    // confident-and-wrong shape, not a parse error anybody would notice.
    check_unknown_keys(json)?;
    let recipe: Recipe = serde_json::from_str(json).map_err(|e| CoreError::Malformed {
        format: "recipe".into(),
        detail: e.to_string(),
    })?;
    validate(&recipe)?;
    Ok(recipe)
}

/// Parse, validate, and resolve `base` if the recipe declares one.
///
/// This is the parse every caller outside this module wants: a resolved
/// recipe behaves as if its base's components had been typed into the file
/// by hand, so nothing downstream — `plan()`, the install screen — has to
/// know inheritance exists.
pub fn parse(json: &str) -> CoreResult<Recipe> {
    resolve_base(parse_unresolved(json)?)
}

/// Replace `recipe.base` with its components, stamped onto the **first**
/// declared layer, in front of this recipe's own.
///
/// Depth is one on purpose. Nothing ART ships needs a chain, and a chain
/// would need a cycle guard whose failure mode is a stack overflow in a
/// binary built with `panic = "abort"`. A base that itself declares a `base`
/// is refused by name.
fn resolve_base(recipe: Recipe) -> CoreResult<Recipe> {
    let Some(base_release) = recipe.base.clone() else {
        return Ok(recipe);
    };
    let base = by_release_unresolved(&base_release)?;
    merge_base(base, recipe)
}

/// [`resolve_base`]'s merge, with the base recipe already in hand — split
/// out so a test can exercise the collision refusal below against two
/// inline recipes (`merge_for_test`) rather than needing a shipped release
/// to look up. `resolve_base` is the only production caller.
fn merge_base(base: Recipe, recipe: Recipe) -> CoreResult<Recipe> {
    if base.base.is_some() {
        return Err(CoreError::Malformed {
            format: "recipe".into(),
            detail: format!(
                "'{}' is based on '{}', which is itself based on another recipe; \
                 ART resolves one level only",
                recipe.release, base.release
            ),
        });
    }
    let Some(first) = recipe.layers.first().map(|l| l.id.clone()) else {
        return Err(CoreError::Malformed {
            format: "recipe".into(),
            detail: format!(
                "'{}' is based on '{}' but declares no layers, \
                 so there is nowhere to put the inherited components",
                recipe.release, base.release
            ),
        });
    };
    let mut components: Vec<Component> = base
        .components
        .into_iter()
        .map(|mut c| {
            c.layer = Some(first.clone());
            c
        })
        .collect();
    for own in &recipe.components {
        if components.iter().any(|c| c.id == own.id) {
            return Err(CoreError::Malformed {
                format: "recipe".into(),
                detail: format!(
                    "'{}' declares a component '{}' that '{}' already declares; \
                     an update amends a component through `overrides`, never by reusing its id",
                    recipe.release, own.id, base.release
                ),
            });
        }
    }
    components.extend(recipe.components.iter().cloned());
    let resolved = Recipe {
        components,
        ..recipe
    };
    validate(&resolved)?;
    // `validate()` skips `validate_removals` whenever `base` is set, because
    // an *unresolved* based recipe's own `removes` may legitimately name a
    // destination the base places (AmigaOS 3.2.2's `update-322-system`
    // removes a file `extras`, a base-layer component, places) — a check
    // `parse_unresolved`'s standalone `validate()` call cannot answer, since
    // the base's own components are not there yet. `resolved.base` is still
    // `Some(_)` (carried through by `..recipe`), so that same skip would
    // apply here too if left to `validate()` alone — which would mean no
    // based recipe's removals were ever checked at all. Run it explicitly,
    // once, against the component list that actually has everything.
    validate_removals(&resolved)?;
    Ok(resolved)
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
///
/// `pub(super)` for the same reason [`validate_component`] is: `package.rs`'s
/// `amiga_installer.program` is a `/`-separated AmigaDOS path inside a
/// package, held to exactly these rules, and a second copy of them would be
/// a second copy that drifts.
///
/// `format` is the caller's, not this function's (fix round 1, review m3).
/// Sharing the checks is right; announcing them all as `format: "recipe"` was
/// not, because a person who mistyped a path in a *package* JSON was then
/// told the fault lay in a recipe — a different file, which they would have
/// gone and read.
pub(super) fn validate_path(
    format: &str,
    component_id: &str,
    field: &str,
    path: &str,
    allow_empty: bool,
) -> CoreResult<()> {
    if path.is_empty() {
        return if allow_empty {
            Ok(())
        } else {
            Err(CoreError::Malformed {
                format: format.into(),
                detail: format!("'{component_id}': {field} is empty"),
            })
        };
    }
    for segment in path.split('/') {
        crate::core::volume::write::dir::check_name(segment)?;
    }
    if path.starts_with('/') || path.split('/').any(|s| s == "..") {
        return Err(CoreError::Malformed {
            format: format.into(),
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
///
/// `format` names the file the caller was reading — `"recipe"` here,
/// `"package"` from `package.rs` — for the reason [`validate_path`] spells
/// out (fix round 1, review m3).
pub(super) fn validate_component(format: &str, component: &Component) -> CoreResult<()> {
    if component.media.trim().is_empty() {
        return Err(CoreError::Malformed {
            format: format.into(),
            detail: format!("'{}' names no media", component.id),
        });
    }
    for rule in &component.rules {
        validate_path(format, &component.id, "from", &rule.from, true)?;
        validate_path(format, &component.id, "to", &rule.to, false)?;
    }
    // A removal names a destination in the tree, exactly like a rule's own
    // `to` — the same AmigaDOS name rules apply, and `removes: [""]` is
    // exactly as meaningless as a `to` of `""` would be.
    for removed in &component.removes {
        validate_path(format, &component.id, "removes", removed, false)?;
    }
    Ok(())
}

/// Everything a recipe must get right before ART trusts it: no two layers
/// sharing an id, no two components sharing an id, every component naming a
/// layer exactly when the recipe is layered and one it actually declares,
/// plus [`validate_component`] for each one.
///
/// **[`validate_removals`] is skipped whenever `recipe.base` is set.** A
/// based recipe's own `removes` may legitimately name a destination the
/// *base* places — AmigaOS 3.2.2's `update-322-system` removes a file
/// `extras`, a base-layer component, places — and this function is called on
/// the recipe standalone, before [`merge_base`] has brought the base's
/// components in. Checking removals here would refuse every based recipe
/// that removes anything the base placed, on the grounds that nothing in
/// *this file alone* places it. [`merge_base`] calls [`validate_removals`]
/// itself, explicitly, once the component list actually has everything.
fn validate(recipe: &Recipe) -> CoreResult<()> {
    let mut seen_layers = std::collections::HashSet::new();
    for layer in &recipe.layers {
        if !seen_layers.insert(layer.id.as_str()) {
            return Err(CoreError::Malformed {
                format: "recipe".into(),
                detail: format!("two layers share the id '{}'", layer.id),
            });
        }
    }

    let mut seen_ids = std::collections::HashSet::new();
    for component in &recipe.components {
        if !seen_ids.insert(component.id.as_str()) {
            return Err(CoreError::Malformed {
                format: "recipe".into(),
                detail: format!("two components share the id '{}'", component.id),
            });
        }

        match (recipe.is_layered(), component.layer.as_deref()) {
            (true, None) => {
                return Err(CoreError::Malformed {
                    format: "recipe".into(),
                    detail: format!(
                        "component '{}' names no layer, and this recipe declares {}",
                        component.id,
                        recipe.layer_ids().join(", ")
                    ),
                })
            }
            (true, Some(named)) if !seen_layers.contains(named) => {
                return Err(CoreError::Malformed {
                    format: "recipe".into(),
                    detail: format!(
                        "component '{}' names the layer '{named}', which this recipe does not \
                         declare",
                        component.id
                    ),
                })
            }
            (false, Some(named)) => {
                return Err(CoreError::Malformed {
                    format: "recipe".into(),
                    detail: format!(
                        "component '{}' names the layer '{named}', but this recipe declares none",
                        component.id
                    ),
                })
            }
            _ => {}
        }

        validate_component("recipe", component)?;
    }
    if recipe.base.is_none() {
        validate_removals(recipe)?;
    }
    Ok(())
}

/// Every `Component::removes` entry names a destination some **other**
/// component in this recipe places with a **`RuleKind::File`** rule, and
/// that component's id is one this component declares an `overrides` over.
///
/// **Why this and not `validate_component`.** A single component's own data
/// cannot answer any of this — "does anything place this path", "does it
/// place it as a file or as a whole drawer" and "is the placer named in my
/// own `overrides`" all need the *rest* of the recipe, which is exactly why
/// this runs once over the whole component list rather than per component
/// like [`validate_component`]'s path checks.
///
/// **Why refuse rather than skip.** A `removes` entry naming nobody's
/// destination is either a typo (the recipe author meant a path that is
/// spelled differently) or a claim about a tree this recipe cannot see —
/// there is no other component in ART's own binary to have placed it, since
/// recipes are `include_str!`-ed and closed. And a `removes` entry naming a
/// real destination without declaring the override is the undeclared-claim
/// shape `no_two_components_claim_one_destination_without_declaring_it`
/// already refuses for a `from`/`to` rule — a component that can make a file
/// disappear without saying whose file it is taking is a stronger claim than
/// one that merely overwrites it, not a weaker one.
///
/// **A `Subtree` placer is refused, never accepted as "close enough"
/// (fix round 1).** The first version of this check matched a rule's `to`
/// regardless of `kind`, on the reasoning that a `Subtree` rule's own
/// destination is a merge point rather than a claim (true for collisions,
/// `recipe.rs`'s own module doc comment) — but a *removal* of that path is
/// not a merge question, it is "delete this drawer", and ART removes files,
/// never drawers: `apply::perform_removal` cannot honestly report how many
/// nested files a drawer removal took with it, which is exactly the
/// "don't claim support that isn't implemented and tested" shape (spec
/// §89) in its quietest form — a JSON key whose validator accepts a shape
/// the engine can only approximate. So this is checked and refused *here*,
/// at the point the recipe format could otherwise say something ART cannot
/// honestly do, rather than left for `apply()` to discover.
fn validate_removals(recipe: &Recipe) -> CoreResult<()> {
    for component in &recipe.components {
        for removed in &component.removes {
            let mut file_placers: Vec<&str> = Vec::new();
            let mut subtree_placers: Vec<&str> = Vec::new();
            for other in recipe.components.iter().filter(|o| o.id != component.id) {
                for rule in &other.rules {
                    if rule.to != *removed {
                        continue;
                    }
                    match rule.kind {
                        // An icon-tooltypes rule amends one file at `to`,
                        // exactly like a `File` rule places one — see
                        // `RuleKind::IconTooltypes`'s own doc comment on why
                        // it participates in every file-level check a `File`
                        // rule does.
                        RuleKind::File | RuleKind::IconTooltypes => {
                            file_placers.push(other.id.as_str())
                        }
                        RuleKind::Subtree => subtree_placers.push(other.id.as_str()),
                    }
                }
            }

            if file_placers.is_empty() && subtree_placers.is_empty() {
                return Err(CoreError::Malformed {
                    format: "recipe".into(),
                    detail: format!(
                        "'{}' removes '{removed}', which no component in this recipe places",
                        component.id
                    ),
                });
            }

            if file_placers.is_empty() {
                // Only `Subtree` placers exist — see this function's own doc
                // comment on why that is refused rather than accepted.
                return Err(CoreError::Malformed {
                    format: "recipe".into(),
                    detail: format!(
                        "'{}' removes '{removed}', which '{}' places as a whole drawer (a \
                         Subtree rule), not a file — ART removes files, never drawers, because \
                         it cannot honestly report how many nested files a drawer removal took \
                         with it",
                        component.id, subtree_placers[0]
                    ),
                });
            }

            let declared = file_placers
                .iter()
                .any(|placer| component.overrides.iter().any(|o| o == placer));
            if !declared {
                return Err(CoreError::Malformed {
                    format: "recipe".into(),
                    detail: format!(
                        "'{}' removes '{removed}', which '{}' places, but '{}' does not declare \
                         an override over '{}'",
                        component.id, file_placers[0], component.id, file_placers[0]
                    ),
                });
            }
        }
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

/// The shipped AmigaOS 3.2.2 recipe — a `base` of `"AmigaOS 3.2"` plus one
/// `update-3.2.2` layer, per AmigaOS 3.2.2's own `HowToInstall` ("a successful
/// installation of AmigaOS 3.2 or 3.2.1"). See the recipe file's own
/// `_why_this_recipe_exists` note for what was measured and where.
pub fn amigaos_322() -> CoreResult<Recipe> {
    parse(AMIGAOS_322_JSON)
}

/// Every release ART ships a recipe for, in the order a picker should list
/// them.
///
/// The strings are the recipes' own `release` values, and
/// `every_offered_release_resolves_to_a_recipe` pins that: this list and the
/// recipe files are two halves of one fact, and a release in only one of them
/// is a release the user either cannot reach or cannot install.
pub fn releases() -> &'static [&'static str] {
    &["AmigaOS 3.2", "AmigaOS 3.2.2", "AmigaOS 3.9"]
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
        "AmigaOS 3.2.2" => amigaos_322(),
        "AmigaOS 3.9" => amigaos_39(),
        other => Err(CoreError::InvalidInput(format!(
            "ART ships no install recipe for {other}"
        ))),
    }
}

/// [`by_release`]'s own match arm set, parsed with [`parse_unresolved`]
/// instead of [`parse`] — what [`resolve_base`] needs when a recipe names
/// this release as its `base`, so that a base's own `base` (refused at depth
/// one) is seen before anything is merged, and so that resolving 3.2.2 does
/// not first resolve 3.2 against itself.
fn by_release_unresolved(release: &str) -> CoreResult<Recipe> {
    match release {
        "AmigaOS 3.2" => parse_unresolved(AMIGAOS_32_JSON),
        "AmigaOS 3.2.2" => parse_unresolved(AMIGAOS_322_JSON),
        "AmigaOS 3.9" => parse_unresolved(AMIGAOS_39_JSON),
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
/// `Ok(None)` means no shipped recipe of either kind declares this id. The
/// caller decides what that is: `declared_override` still refuses by name,
/// because an id that resolves to nothing is an inconsistency in whatever
/// built the item, not a fact about the file.
///
/// # It returns an error and does not panic (fix round 1, F10)
///
/// The first version reached for `panic!`/`expect` on a shipped recipe that
/// would not parse, reasoning that such a recipe is a bug in ART's own data
/// rather than a user situation. That reasoning does not survive the release
/// profile: `panic = "abort"` means an abort here takes the whole
/// application down, and this runs **per collision row** — the exact shape
/// CLAUDE.md's bounds-checking rule names. Shipped data being wrong is still
/// a bug; it is now a bug that produces a refusal a user can read and a
/// process that is still running.
///
/// # Parsed once, not once per row (fix round 1, F6)
///
/// `by_release` re-parses the JSON on every call, and this was called for
/// every row of every preview — benchmarked at 110 µs against the 16.5 µs
/// `package::by_id` alone used to cost, ~6.7x per row and unflagged. The
/// answer is a flat `id -> overrides` map built once per process. The shipped
/// JSON is `include_str!`-ed into the binary and cannot change under a
/// running process, so caching it can never go stale; the parse **result**
/// is what is cached, so a broken recipe is reported identically on the
/// first call and the thousandth.
///
/// `CoreError` is not `Clone`, so the cache holds the failure as its own
/// text and rebuilds a `CoreError::Malformed` from it. The text is the
/// original error's, unchanged.
pub fn shipped_component_overrides(id: &str) -> CoreResult<Option<Vec<String>>> {
    static OVERRIDES: std::sync::OnceLock<Result<HashMap<String, Vec<String>>, String>> =
        std::sync::OnceLock::new();

    let built = OVERRIDES.get_or_init(build_shipped_override_map);
    match built {
        Ok(map) => Ok(map.get(id).cloned()),
        Err(detail) => Err(CoreError::Malformed {
            format: "shipped recipe".into(),
            detail: detail.clone(),
        }),
    }
}

/// Every shipped component id mapped to its own `overrides`, releases first
/// then packages — see [`shipped_component_overrides`], which is the only
/// caller and which caches this.
fn build_shipped_override_map() -> Result<HashMap<String, Vec<String>>, String> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for release in releases() {
        let recipe = by_release(release)
            .map_err(|e| format!("the shipped {release} recipe does not parse: {e}"))?;
        for component in recipe.components {
            // Releases are searched first, so an id a release already claims
            // is not overwritten by a package's. `no_id_is_claimed_by_both_a_release_and_a_package`
            // is what keeps that from ever mattering.
            map.entry(component.id).or_insert(component.overrides);
        }
    }
    let packages = super::package::packages()
        .map_err(|e| format!("the shipped packages do not parse: {e}"))?;
    for package in packages {
        map.entry(package.id).or_insert(package.component.overrides);
    }
    Ok(map)
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
                    base: None,
                    layers: vec![],
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
                "AmigaOS 3.2.2" => AMIGAOS_322_JSON,
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
                    base: None,
                    layers: vec![],
                    components: vec![package.component],
                },
            ));
        }
        all
    }

    // ---- ART-183: a misspelled key is named, not dropped ----

    /// The recipe this is all about: `user_startup` written the wrong way.
    /// It used to parse to a component with no startup lines, and the symptom
    /// was a tree quietly missing what the recipe plainly asked for.
    #[test]
    fn a_misspelled_component_key_is_refused_by_name() {
        let json = r#"{
            "release": "Test",
            "components": [
                { "id": "a", "media": "A", "rules": [], "userStartup": ["assign X: SYS:"] }
            ]
        }"#;
        let err = parse(json).expect_err("a misspelled key must be refused");
        let text = err.to_string();
        assert!(text.contains("userStartup"), "name the key: {text}");
        assert!(text.contains("'a'"), "and where it is: {text}");
        assert!(
            text.contains("must begin with '_'"),
            "and what a note looks like, because the other mistake wants the other fix: {text}"
        );
    }

    /// **The reason `deny_unknown_fields` is not the answer.** Every shipped
    /// recipe carries these, and rejecting them would refuse ART's own data.
    #[test]
    fn a_note_to_a_human_is_kept() {
        let json = r#"{
            "_why_this_release": "measured off the owner's own disc, 2026-08-22",
            "release": "Test",
            "components": [
                { "_why_": "the base", "id": "a", "media": "A", "rules": [] }
            ]
        }"#;
        assert!(parse(json).is_ok(), "{:?}", parse(json));
    }

    /// Every level, not just the component: a rule and an activation are
    /// where the other misspellings would land.
    #[test]
    fn a_misspelled_key_is_refused_at_every_level() {
        for (json, needle) in [
            (
                r#"{"release":"T","components":[{"id":"a","media":"A","rules":[{"from":"C","to":"C","kind":"subtree","recursive":true}]}]}"#,
                "recursive",
            ),
            (
                r#"{"release":"T","components":[{"id":"a","media":"A","rules":[],"activate":[{"kind":"monitor","named":"NTSC"}]}]}"#,
                "named",
            ),
            (
                r#"{"release":"T","components":[{"id":"a","media":"A","rules":[],"condition":{"condition":"rom-older-than","majorVersion":40}}]}"#,
                "majorVersion",
            ),
            (r#"{"release":"T","recipes":[],"components":[]}"#, "recipes"),
        ] {
            let err = parse(json).expect_err(needle);
            assert!(err.to_string().contains(needle), "{needle}: {err}");
        }
    }

    /// Two bad keys refuse the same way twice, and the same way every time.
    ///
    /// **Disclosed:** removing the `sort_unstable` does not fail this, because
    /// `serde_json`'s map is already ordered. What this pins is the
    /// *determinism*, which is the property that matters and which holds by
    /// two mechanisms rather than one — see `check_keys`'s own comment for
    /// why the redundant one stays.
    #[test]
    fn two_bad_keys_refuse_the_same_way_every_time() {
        let json = r#"{
            "release": "T",
            "components": [{ "id": "a", "media": "A", "rules": [], "zzz": 1, "aaa": 2 }]
        }"#;
        let first = parse(json).unwrap_err().to_string();
        for _ in 0..8 {
            assert_eq!(parse(json).unwrap_err().to_string(), first);
        }
        assert!(
            first.contains("aaa"),
            "sorted, so the first is the first: {first}"
        );
    }

    /// And the shipped recipes still parse, which is the check that this did
    /// not close the hole by refusing ART's own data.
    #[test]
    fn every_shipped_recipe_still_parses() {
        assert!(amigaos_32().is_ok());
        assert!(amigaos_39().is_ok());
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
    /// **This sees `RuleKind::File` and `RuleKind::IconTooltypes` rules
    /// only — the two kinds that claim one file rather than merge into a
    /// drawer — and every rule in the shipped AmigaOS 3.9 recipe is a
    /// `Subtree`** — so for that recipe it
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
                for rule in component
                    .rules
                    .iter()
                    .filter(|r| matches!(r.kind, RuleKind::File | RuleKind::IconTooltypes))
                {
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
            vec![
                "workbench-base",
                "locale-base",
                "workbench-39",
                "keymaps",
                "special-locale-turkish",
                "locale-euro"
            ],
            "recipe order is what decides which layer writes last (see this test's own doc comment)"
        );
        // **What "last" turned out to mean** (ART-159, 2026-08-23). This test
        // used to assert `workbench-39` was the final element of the array,
        // and read that as the fix for ART-169. It is not: the fix is that the
        // overlay is declared *after the two layers it overrides*, which is
        // what `apply`'s last-writer-wins actually depends on. Two components
        // that collide with neither of those layers now sit after it, and the
        // ART-169 property is untouched — so the assertion says the property
        // rather than the position. `every_override_is_declared_after_what_it_overrides`
        // holds the same rule for every component of every shipped recipe.
        let at = |id: &str| ids.iter().position(|c| *c == id).unwrap();
        assert!(
            at("workbench-39") > at("workbench-base") && at("workbench-39") > at("locale-base"),
            "the 3.9 overlay must be declared after both layers it overrides"
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
            "the overlay collides with both layers for real — 12 of its 29 C commands replace a 3.5 command, and 32 of its Locale/Countries files are ones locale-base also places"
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
            "the overlay's rules must match the disc's own Workbench3.9 top level, drawer for drawer — a rule missing here is a drawer of the 3.9 layer that stops being installed"
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
            super::shipped_component_overrides("workbench-39").unwrap(),
            Some(vec![
                "workbench-base".to_string(),
                "locale-base".to_string()
            ]),
            "a release recipe's own component"
        );
        assert_eq!(
            super::shipped_component_overrides("locale-turkish").unwrap(),
            Some(vec!["locale-base".to_string()]),
            "a package, which is all this used to be able to answer for"
        );
        assert_eq!(
            super::shipped_component_overrides("workbench-base").unwrap(),
            Some(Vec::new()),
            "a real component that declares none is not the same as no component"
        );
        assert_eq!(
            super::shipped_component_overrides("not-shipped-at-all").unwrap(),
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

    /// `available: false` is §96's "Coming Later" box: a component that is
    /// registered but not yet built, kept visible rather than hidden.
    ///
    /// Used to be checked against the shipped `update-3.2.1` placeholder —
    /// `available: false` with no rules — until Task 8 removed it: AmigaOS
    /// 3.2.2's own `HowToInstall` wants "3.2 **or** 3.2.1", so installing
    /// cumulatively needs one recipe (3.2.2, layered on 3.2), not a second
    /// placeholder recipe for the update in between. Constructs its own
    /// example now, so this test does not depend on any one shipped
    /// component staying unbuilt forever.
    #[test]
    fn a_component_declared_unavailable_says_so() {
        let json = r#"{
            "release": "X",
            "components": [
                { "id": "a", "media": "M", "rules": [], "available": false }
            ]
        }"#;
        let recipe = parse(json).expect("an unavailable component is still a valid recipe");
        assert!(!recipe.component("a").unwrap().available);
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

    /// The four languages whose disks carry a `Support` drawer, and whose
    /// alphabets need it.
    ///
    /// **Read out of the release's own Installer script on 2026-08-24**, not
    /// recalled: `Install3.2.adf`'s `Install/Install` guards the block with
    ///
    /// ```text
    /// ;begin copy for language that need special support as greek, polski, russian and turkish
    /// (if (OR (OR (= n 6) (= n 10)) (OR (= n 12) (= n 14)))
    /// ```
    ///
    /// where `n` indexes the script's own `choices` list — 6 Greek, 10 Polski,
    /// 12 Russian, **14 Türkçe**. Confirmed against the media rather than
    /// against the script alone: those four disks carry `Support`, and
    /// `Locale-DE`, `-EN`, `-FR` and `-IT` have no such drawer at all.
    const SPECIAL_SUPPORT: [&str; 4] = ["GR", "PL", "RU", "TR"];

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
            let expected = if SPECIAL_SUPPORT.contains(&code) {
                4
            } else {
                2
            };
            assert_eq!(component.rules.len(), expected, "{id}");
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

            // **The alphabet, and where the release puts it.** `Support/Fonts`
            // goes to `SYS:Fonts` — not `Locale/`, not `Storage/` — because
            // that is what the Installer's own `(tackon target "Fonts")`
            // says. Turkish catalogs on a system with no ISO-8859-9 glyphs is
            // [ART-159](../../../../docs/ISSUES.md#fixed) in the other recipe.
            let has_support = component.rules.iter().any(|r| {
                r.from == "Support/Fonts" && r.to == "Fonts" && r.kind == RuleKind::Subtree
            }) && component
                .rules
                .iter()
                .any(|r| r.from == "Support/Prefs" && r.to == "Prefs");
            assert_eq!(
                has_support,
                SPECIAL_SUPPORT.contains(&code),
                "{id}: only the four languages whose disks carry Support may claim it"
            );
        }
    }

    /// The one whose absence would be invisible: a locale disk writing into a
    /// drawer the base owns has to say so, and these four write
    /// `Prefs/Presets/Font-XX.prefs`.
    ///
    /// Measured on the owner's own disks, 2026-08-24: `Locale-GR`, `-PL` and
    /// `-RU` each carry one `Font-XX.prefs` under `Support/Prefs/Presets`.
    /// **`Locale-TR`'s `Presets` is empty** — the rule is there anyway, so a
    /// disk revision that fills it is carried rather than silently dropped,
    /// and because a `from` that does not resolve is a refusal rather than a
    /// skip, the empty drawer has to exist for that to be safe. It does.
    #[test]
    fn the_four_special_locales_declare_what_they_write_into() {
        let recipe = recipe();
        for code in SPECIAL_SUPPORT {
            let id = format!("locale-{}", code.to_lowercase());
            let component = recipe.component(&id).unwrap();
            assert!(
                component.overrides.iter().any(|o| o == "workbench-base"),
                "{id} writes into Prefs, which workbench-base owns"
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

    /// **ART-169's positional half, generalised** — filed out of ART-159,
    /// which is what made it necessary.
    ///
    /// `plan()` emits items in recipe-declaration order and `apply`'s writer
    /// lets the last writer win, so a component that declares `overrides`
    /// only wins if it is declared *after* what it overrides. Declared above
    /// it, the override is still accepted by `detect_collisions` — the plan
    /// does not refuse — and the older layer then writes last. Nothing
    /// reports it: the tree builds, the manifest is consistent, and the files
    /// are the wrong ones. That is this project's confident-and-wrong shape,
    /// and until now the only thing holding it was one hardcoded id list in
    /// `the_39_overlay_is_declared_last_required_and_over_both_layers` — a
    /// list ART-159's two new components had to change, which is exactly when
    /// a positional assertion stops meaning what it used to.
    ///
    /// Release recipes only. A package's `overrides` names a *release's*
    /// component, and a package is applied after the whole release tree
    /// exists, so "declared after" has no meaning across that boundary.
    #[test]
    fn every_override_is_declared_after_what_it_overrides() {
        for release in super::releases() {
            let recipe = super::by_release(release)
                .unwrap_or_else(|e| panic!("the shipped {release} recipe must parse: {e}"));
            let order: Vec<&str> = recipe.components.iter().map(|c| c.id.as_str()).collect();
            for (index, component) in recipe.components.iter().enumerate() {
                for over in &component.overrides {
                    let Some(earlier) = order.iter().position(|id| id == over) else {
                        // A cross-recipe override is somebody else's test —
                        // `every_override_names_a_component_that_exists`.
                        continue;
                    };
                    assert!(
                        index > earlier,
                        "{release}: '{}' declares an override over '{over}' but is declared \
                         before it, so '{over}' writes last and the override silently does \
                         nothing",
                        component.id
                    );
                }
            }
        }
    }

    /// **ART-224.** The install screen labels a component's row with its
    /// `media` — the volume it comes from — and for AmigaOS 3.2 that is a
    /// different ADF per component and exactly what a person needs to read.
    /// AmigaOS 3.9 is five components off **one disc**, so every row said
    /// `AmigaOS3.9`: three identical labels before ART-159, two of them
    /// tick-boxes the user is asked to decide about, and no way to tell which
    /// was which. Nothing failed and nothing looked broken — which is why it
    /// sat there through a driven session.
    ///
    /// **Refined 2026-08-24, by the next thing that used it.** The rule read
    /// *"a recipe whose components do not each name their own medium must
    /// label every row"*, and the AmigaOS 3.2 recipe's new `keymaps`
    /// component — which comes off `Storage3.2`, the same disk as `storage` —
    /// would have demanded a label on all twenty-nine of that recipe's rows.
    /// Twenty-seven of them already say `Extras3.2`, `Locale-TR`, `MMULibs`:
    /// better than any sentence ART would write for them. A rule that forces
    /// twenty-seven redundant keys is obeyed by writing twenty-seven worse
    /// labels, which is the outcome the rule was written to avoid. So it is
    /// narrowed to the components that actually collide: **a component
    /// sharing its medium with another must name its own row**, and one whose
    /// medium is its own keeps saying so.
    #[test]
    fn a_component_sharing_its_medium_labels_its_own_row() {
        for release in super::releases() {
            let recipe = super::by_release(release)
                .unwrap_or_else(|e| panic!("the shipped {release} recipe must parse: {e}"));

            for component in &recipe.components {
                let shared = recipe
                    .components
                    .iter()
                    .any(|other| other.id != component.id && other.media == component.media);
                if !shared {
                    continue;
                }
                let key = component.label_key.as_deref().unwrap_or_else(|| {
                    panic!(
                        "{release}: '{}' shares the medium '{}'; both rows read alike, so it needs a label_key",
                        component.id, component.media
                    )
                });
                assert!(
                    key.starts_with("osinstall.components.name."),
                    "{release}: '{}' labels itself '{key}', outside this screen's own namespace",
                    component.id
                );
            }
        }
    }

    /// **A font descriptor is spelled `.font`, and ART-225 is why this is a
    /// test rather than a convention.**
    ///
    /// `diskfont.library` matches the `.font` suffix **case-sensitively**. A
    /// descriptor placed as `TOPAZ-ISO9.FONT` is not merely spelled oddly —
    /// it does not exist as far as `AvailFonts`, every font requester and the
    /// whole system are concerned. Measured on the owner's own machine, in
    /// the shape this project asks for: one of thirteen descriptors had its
    /// suffix lowered on the host while the emulator was running, the
    /// requester was reopened, and **that one appeared while the other twelve
    /// did not** — control and treatment in the same list, one variable, and
    /// the base name left uppercase on purpose so the suffix was the only
    /// thing that changed.
    ///
    /// Nothing else could have caught it. The tree built, verified clean,
    /// planned exactly 63 items, and the round's own real-media test asserted
    /// "thirteen family drawers each with a non-empty descriptor" — which was
    /// true, and the fonts were invisible. This is the cheapest guard that
    /// makes the class impossible in CI: no media, no emulator, one rule.
    #[test]
    fn every_font_descriptor_a_recipe_places_is_spelled_dot_font_art_225() {
        let packages = super::super::package::packages().expect("the shipped packages must parse");
        let mut checked = 0usize;
        for release in super::releases() {
            let recipe = super::by_release(release)
                .unwrap_or_else(|e| panic!("the shipped {release} recipe must parse: {e}"));
            let components = recipe
                .components
                .iter()
                .chain(packages.iter().map(|p| &p.component));
            for component in components {
                for rule in &component.rules {
                    let leaf = rule.to.rsplit('/').next().unwrap_or(&rule.to);
                    if !leaf.to_ascii_lowercase().ends_with(".font") {
                        continue;
                    }
                    checked += 1;
                    assert!(
                        leaf.ends_with(".font"),
                        "{release}: '{}' places '{}', and diskfont.library will not see it — \
                         a font descriptor's suffix is matched case-sensitively, so it has to \
                         be spelled '.font' exactly, the way the medium itself spells it",
                        component.id,
                        rule.to
                    );
                }
            }
        }
        assert!(
            checked >= 13,
            "this guard saw only {checked} font descriptors; the Turkish set alone is thirteen, \
             so it is looking in the wrong place"
        );
    }

    /// **ART-226, the AmigaOS 3.2 half — and it is the same defect solved the
    /// opposite way, on purpose.**
    ///
    /// A 3.2 tree ART built carries twenty-two entries in `Devs/Keymaps` and
    /// every one of them is a `.info`: icons pointing at nothing, and the
    /// only usable keymap the one in ROM. The keymaps are on `Storage3.2`,
    /// the shelf, and `tr` is among them — so AmigaOS 3.2 does ship a Turkish
    /// keymap and ART was not installing it.
    ///
    /// **Twenty-two `File` rules, where the 3.9 component uses one
    /// `Subtree`.** Measured, not stylistic: the 3.2 shelf carries a `.info`
    /// beside every keymap and `Devs/Keymaps` already holds icons — different
    /// ones (`Workbench3.2`'s `d.info` is 1 256 bytes, `Storage3.2`'s is 450),
    /// so a subtree rule would replace twenty-two good icons with shelf ones.
    /// On the 3.9 disc the drawer arrives empty, so there the shelf's icons
    /// are the only icons and taking the drawer whole is right. Each recipe
    /// follows its own medium.
    ///
    /// The names are the shelf's own, and being ASCII they can be listed
    /// without reintroducing ART-225's risk — which is exactly why the 3.9
    /// component, whose shelf holds `türkçe`, may not be written this way.
    #[test]
    fn the_32_keymaps_component_names_the_shelf_and_not_its_icons_art_226() {
        let recipe = parse(AMIGAOS_32_JSON).unwrap();
        let keymaps = recipe
            .component("keymaps")
            .expect("ART-226: the 3.2 recipe must offer the keymaps");

        assert!(!keymaps.required, "a tick-box, as on the 3.9 side");
        assert_eq!(keymaps.media, "Storage3.2");
        assert!(
            keymaps.overrides.is_empty(),
            "naming the keymaps alone replaces nothing; an override here would be a licence \
             to overwrite the icons this component deliberately leaves alone"
        );

        const SHELF: [&str; 22] = [
            "cdn", "ch1", "ch2", "d", "dk", "e", "f", "gb", "greek", "i", "la", "n", "po",
            "polska", "rusd", "rusgb", "rusuae", "rusus", "ruswin", "s", "tr", "usa2",
        ];
        let expected: Vec<PathRule> = SHELF
            .iter()
            .map(|name| PathRule {
                from: format!("Keymaps/{name}"),
                to: format!("Devs/Keymaps/{name}"),
                kind: RuleKind::File,
            })
            .collect();
        assert_eq!(
            keymaps.rules, expected,
            "the shelf's twenty-two keymaps, and not the twenty-two icons beside them"
        );
        assert!(
            keymaps.rules.iter().any(|r| r.to.ends_with("/tr")),
            "the Turkish keymap is on this shelf, and it is why this component exists"
        );
        assert!(
            !keymaps.rules.iter().any(|r| r.from.ends_with(".info")),
            "an icon named here would replace one Workbench3.2 already placed"
        );
    }

    /// **ART-226.** A system whose keyboard cannot be selected is not a matter
    /// of taste, and until this component existed every tree ART built was
    /// one: `Devs/Keymaps` came off the disc empty, the twenty-two keymaps sat
    /// on the `Storage/Keymaps` shelf where nothing loads them, and the only
    /// keymap available was the one in ROM — `usa`, whatever language the user
    /// had chosen.
    ///
    /// Three things pinned here that nothing else can see:
    ///
    /// - **The source is the 3.9 overlay's shelf, not the 3.5 layer's.**
    ///   `Workbench3.5/Storage` carries no Keymaps drawer; the shelf arrived
    ///   with `Workbench3.9`. A rule pointed at the older path resolves to
    ///   nothing and refuses at plan time, which is loud — but only against
    ///   real media, and no unit test opens a disc.
    /// - **`Subtree`, deliberately.** A `File` rule's `to` is whatever the
    ///   author typed, and ART-225 is what that costs: thirteen descriptors
    ///   retyped off a listing, spelled `.FONT`, invisible to the running
    ///   system. A subtree rule carries the medium's own spelling for every
    ///   child — including this drawer's `türkçe`, the one name here nobody
    ///   could retype safely.
    /// - **It declares no override**, and must not. `Devs/Keymaps` arrives
    ///   from `workbench-base` as an empty drawer and `Workbench3.9/Devs` has
    ///   no Keymaps at all, so nothing of anyone's is replaced. An override
    ///   here would be a licence to overwrite a keymap set somebody else
    ///   placed, bought for nothing.
    #[test]
    fn the_keymaps_component_takes_the_whole_shelf_art_226() {
        let recipe = parse(AMIGAOS_39_JSON).unwrap();
        let keymaps = recipe
            .component("keymaps")
            .expect("ART-226: the 3.9 recipe must offer the keymaps");

        assert!(
            !keymaps.required,
            "the owner's call: a tick-box, because a few people will not want it"
        );
        assert_eq!(keymaps.media, "AmigaOS3.9");
        assert!(
            keymaps.overrides.is_empty(),
            "the drawer this fills arrives empty; there is nothing here to override"
        );
        assert_eq!(
            keymaps.rules,
            vec![PathRule {
                from: "OS-VERSION3.9/WORKBENCH3.9/STORAGE/KEYMAPS".into(),
                to: "Devs/Keymaps".into(),
                kind: RuleKind::Subtree,
            }],
            "the shelf, whole, into the drawer AmigaDOS actually loads from"
        );

        // Declared after the two layers that touch `Devs`, so that whatever
        // order `apply` writes in, this drawer's contents land on top of an
        // empty drawer rather than under a later copy of it.
        let ids: Vec<&str> = recipe.components.iter().map(|c| c.id.as_str()).collect();
        let at = |id: &str| ids.iter().position(|c| *c == id).unwrap();
        assert!(
            at("keymaps") > at("workbench-base") && at("keymaps") > at("workbench-39"),
            "the keymaps go in after the layers that create Devs"
        );
    }

    /// **ART-159, hazard 2 — the owner's own language.** The disc's
    /// `Special-Locale/TÜRKÇE` branch is fonts and nothing else: 13 families,
    /// 13 `.font` descriptors, 37 size files, and no keymap, printer driver
    /// or install script (measured 2026-08-23 off the owner's `AmigaOS39.iso`
    /// by reading its Primary directory records — the recipe file's own
    /// `_why_this_component_exists` carries the numbers). They are the
    /// ISO-8859-9 set, so without them a Turkish system has the catalogs and
    /// not the glyphs for ş, ğ, ı and İ.
    ///
    /// Three things this pins that nothing else can:
    ///
    /// - **A family and its descriptor travel together.** AmigaOS finds a
    ///   font through the descriptor; a directory placed without one is a
    ///   font `diskfont.library` cannot see, and the tree would build and
    ///   verify clean around it.
    /// - **Each destination is the disc's own spelling, copied not retyped**
    ///   (ART-225). The pairs below are the disc's, inconsistencies included:
    ///   `FuturaB-ISO9.font` beside `courier-iso9.font`, `XHelvetica-iso9.font`
    ///   beside `XCourier-ISO9.font`, and every descriptor's case differing
    ///   from its own family drawer's. Tidying any of that would be ART
    ///   renaming the user's file, and lowering a suffix that the medium
    ///   spells lower is the difference between a font the system has and one
    ///   it does not.
    /// - **No override, on purpose.** Zero of the thirteen names collide with
    ///   the base `Fonts` drawer, so declaring one would say this layer
    ///   replaces the system's fonts when it does not — and would license a
    ///   future rule to do so unmeasured.
    #[test]
    fn the_turkish_font_component_pairs_every_family_with_its_descriptor_art_159() {
        let recipe = parse(AMIGAOS_39_JSON).unwrap();
        let turkish = recipe
            .component("special-locale-turkish")
            .expect("ART-159: the 3.9 recipe must offer the Turkish ISO-8859-9 fonts");

        assert!(
            !turkish.required,
            "a tree without the Turkish fonts still boots — this is a tick-box, not a layer"
        );
        assert_eq!(turkish.media, "AmigaOS3.9");
        assert!(
            turkish.overrides.is_empty(),
            "the thirteen families collide with nothing in the base Fonts drawer, and an \
             override that replaces nothing is a licence to replace something later"
        );

        // The disc's own thirteen, drawer and descriptor, in recipe order.
        // Read off the owner's AmigaOS39.iso and cross-checked: 7-Zip and
        // ART's own `distribution.json` agree byte for byte on all thirteen
        // *base* font names from the same disc, so these are the names
        // `CdSource` returns and not one tool's opinion of them.
        const DISC: [(&str, &str); 13] = [
            ("courier-iso9", "courier-iso9.font"),
            ("diamond-iso9", "diamond-iso9.font"),
            ("emerald-iso9", "emerald-iso9.font"),
            ("futurab-iso9", "FuturaB-ISO9.font"),
            ("garnet-iso9", "garnet-iso9.font"),
            ("personal-iso9", "Personal-ISO9.font"),
            ("times-iso9", "Times-ISO9.font"),
            ("topaz-iso9", "Topaz-ISO9.font"),
            ("topazt", "topazt.font"),
            ("xcourier-iso9", "XCourier-ISO9.font"),
            ("xen-iso9", "Xen-ISO9.font"),
            ("xen-wide-iso9", "Xen-Wide-ISO9.font"),
            ("xhelvetica-iso9", "XHelvetica-iso9.font"),
        ];

        const DRAWER: &str = "OS-VERSION3.9/SPECIAL-LOCALE/TÜRKÇE/FONTS";
        let mut expected: Vec<(String, String, RuleKind)> = Vec::new();
        for (drawer, descriptor) in DISC {
            // `from` stays in the Primary tree's all-caps spelling: it
            // resolves (the real run refuses nothing), lookup is
            // case-insensitive, and the drawer above these is not spelled the
            // same in its Rock Ridge entry as in its Primary one.
            let caps = drawer.to_ascii_uppercase();
            expected.push((
                format!("{DRAWER}/{caps}"),
                format!("Fonts/{drawer}"),
                RuleKind::Subtree,
            ));
            expected.push((
                format!("{DRAWER}/{caps}.FONT"),
                format!("Fonts/{descriptor}"),
                RuleKind::File,
            ));
        }

        let actual: Vec<(String, String, RuleKind)> = turkish
            .rules
            .iter()
            .map(|r| (r.from.clone(), r.to.clone(), r.kind))
            .collect();
        assert_eq!(
            actual, expected,
            "each family arrives with its own descriptor, in the disc's own spelling — a \
             missing descriptor is a font AmigaOS cannot see, and a retyped one is a font \
             AmigaOS cannot see either (ART-225)"
        );
    }

    /// **ART-159, hazard 2 — the euro country files.** Nine `.country` files
    /// that are byte-for-byte different from the base release's namesakes
    /// while being exactly the same length as them (586/588/590/592 bytes,
    /// 9 of 9 different by SHA-256, measured 2026-08-23 off the owner's own
    /// disc). Same size is why this is worth a component at all: it looks
    /// like a duplicate and is not.
    ///
    /// The override list is the load-bearing part. It really does replace
    /// both layers' copies, and `plan::detect_collisions` refuses an
    /// undeclared claim at plan time — but only against real media, which no
    /// unit test resolves, so dropping an entry here would turn every real
    /// install with this box ticked into a refusal and nothing in CI would
    /// notice.
    #[test]
    fn the_euro_country_component_replaces_both_layers_it_names_art_159() {
        let recipe = parse(AMIGAOS_39_JSON).unwrap();
        let euro = recipe
            .component("locale-euro")
            .expect("ART-159: the 3.9 recipe must offer the euro country files");

        assert!(
            !euro.required,
            "euro currency is a choice, not a system need"
        );
        assert_eq!(euro.media, "AmigaOS3.9");
        assert_eq!(
            euro.rules,
            vec![PathRule {
                from: "OS-VERSION3.9/LOCALE.EURO/COUNTRIES".into(),
                to: "Locale/Countries".into(),
                kind: RuleKind::Subtree,
            }],
            "the nine files go where locale.library reads them — not onto the Storage shelf, \
             where the disc's other, older euro set already lands"
        );

        let mut overrides: Vec<&str> = euro.overrides.iter().map(String::as_str).collect();
        overrides.sort_unstable();
        assert_eq!(
            overrides,
            vec!["locale-base", "workbench-39"],
            "both layers place Locale/Countries and both are really overwritten; \
             workbench-base is not named because its Storage rule is not in the way"
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

    // ---- Layered media (Task 1): a component says which layer it lives in ----

    #[test]
    fn a_recipe_with_no_layers_is_unlayered_and_components_need_no_layer() {
        let recipe = parse(r#"{"release":"X","components":[{"id":"a","media":"M","rules":[]}]}"#)
            .expect("an unlayered recipe still parses");
        assert!(!recipe.is_layered());
        assert!(recipe.layers.is_empty());
        assert_eq!(recipe.component("a").unwrap().layer, None);
    }

    #[test]
    fn a_component_in_a_layered_recipe_must_name_its_layer() {
        let err = parse(
            r#"{"release":"X",
                "layers":[{"id":"base"},{"id":"update"}],
                "components":[{"id":"a","media":"M","rules":[]}]}"#,
        )
        .expect_err("a component with no layer is a recipe error");
        let text = err.to_string();
        assert!(
            text.contains("'a'") && text.contains("layer"),
            "the refusal has to name the component and the missing field, got: {text}"
        );
    }

    #[test]
    fn a_component_may_not_name_a_layer_the_recipe_does_not_declare() {
        let err = parse(
            r#"{"release":"X",
                "layers":[{"id":"base"}],
                "components":[{"id":"a","media":"M","layer":"update","rules":[]}]}"#,
        )
        .expect_err("an undeclared layer is a recipe error");
        let text = err.to_string();
        assert!(
            text.contains("'a'") && text.contains("update"),
            "the refusal has to name the component and the layer it asked for, got: {text}"
        );
    }

    #[test]
    fn two_layers_may_not_share_an_id() {
        let err = parse(
            r#"{"release":"X",
                "layers":[{"id":"base"},{"id":"base"}],
                "components":[]}"#,
        )
        .expect_err("duplicate layer ids are a recipe error");
        assert!(err.to_string().contains("base"));
    }

    #[test]
    fn a_layer_carries_its_label_key() {
        let recipe = parse(
            r#"{"release":"X",
                "layers":[{"id":"base","label_key":"osinstall.layer.base32"}],
                "components":[{"id":"a","media":"M","layer":"base","rules":[]}]}"#,
        )
        .expect("a labelled layer parses");
        assert_eq!(
            recipe.layers[0].label_key.as_deref(),
            Some("osinstall.layer.base32")
        );
    }

    // ---- base: a recipe may inherit another release's components ----

    /// The raw JSON text behind one of ART's own shipped releases —
    /// [`an_unlayered_recipe_is_byte_for_byte_what_its_file_says`] compares
    /// the resolved recipe against this rather than against `parse`'s own
    /// output, so a merge step that quietly changed something would have to
    /// change it away from the file itself, not just away from a second call
    /// to the function under test.
    fn json_for(release: &str) -> &'static str {
        match release {
            "AmigaOS 3.2" => AMIGAOS_32_JSON,
            "AmigaOS 3.9" => AMIGAOS_39_JSON,
            other => {
                panic!("'{other}' is offered by releases() but json_for() has no raw JSON for it")
            }
        }
    }

    /// [`resolve_base`] minus its shipped-release lookup: parses `base_json`
    /// and `derived_json` and merges the second onto the first exactly as
    /// `resolve_base` would once it had found the base recipe by name. Lets
    /// the collision refusal be tested against an inline base rather than
    /// one of the two releases ART actually ships, so the test does not have
    /// to wait on a real update recipe existing.
    fn merge_for_test(base_json: &str, derived_json: &str) -> CoreResult<Recipe> {
        let base = parse_unresolved(base_json)?;
        let derived = parse_unresolved(derived_json)?;
        merge_base(base, derived)
    }

    #[test]
    fn a_based_recipe_inherits_its_bases_components_on_the_first_layer() {
        let recipe = by_release("AmigaOS 3.2.2").expect("the 3.2.2 recipe resolves");
        let inherited = recipe
            .component("workbench-base")
            .expect("the base recipe's components are inherited");
        assert_eq!(inherited.layer.as_deref(), Some("base"));
        assert_eq!(
            recipe
                .component("update-322-system")
                .unwrap()
                .layer
                .as_deref(),
            Some("update-3.2.2"),
            "the recipe's own components keep the layer they declared"
        );
    }

    #[test]
    fn an_unlayered_recipe_is_byte_for_byte_what_its_file_says() {
        // The 3.2 and 3.9 recipes declare no `base` and no `layers`, so
        // resolution must be a no-op for them. Compared field by field rather
        // than by count, because a merge that dropped a rule would keep the
        // count if it also added one.
        for release in ["AmigaOS 3.2", "AmigaOS 3.9"] {
            let resolved = by_release(release).unwrap();
            let raw = parse_unresolved(json_for(release)).unwrap();
            assert_eq!(resolved, raw, "{release} must not change under resolution");
        }
    }

    #[test]
    fn a_base_that_names_an_unknown_release_is_refused() {
        let err = resolve_base(
            parse_unresolved(
                r#"{"release":"X","base":"AmigaOS 9.9",
                    "layers":[{"id":"base"}],
                    "components":[]}"#,
            )
            .unwrap(),
        )
        .expect_err("an unknown base is a recipe error");
        assert!(err.to_string().contains("AmigaOS 9.9"));
    }

    #[test]
    fn a_based_recipe_may_not_redeclare_one_of_its_bases_component_ids() {
        let err = merge_for_test(
            /* base   */
            r#"{"release":"B","components":[{"id":"a","media":"M","rules":[]}]}"#,
            /* derived*/
            r#"{"release":"D","base":"B",
                "layers":[{"id":"base"},{"id":"up"}],
                "components":[{"id":"a","media":"M2","layer":"up","rules":[]}]}"#,
        )
        .expect_err("a redeclared id is a recipe error, not a silent replacement");
        assert!(
            err.to_string().contains("'a'"),
            "the refusal names the id that collides"
        );
    }

    /// **Depth is one on purpose.** A base that itself declares a `base`
    /// would need a cycle guard whose failure mode, in a binary built with
    /// `panic = "abort"`, is the whole application dying rather than a
    /// caught error. Refusing by name at depth one avoids needing that
    /// guard at all. Not one of the brief's four written tests — the fifth
    /// mutation row asks for a two-level fixture instead of a named test —
    /// but recorded here rather than only in the mutation table, so the
    /// property survives a future edit to this file even if nobody re-reads
    /// the plan that produced it.
    #[test]
    fn a_base_that_is_itself_based_on_something_is_refused() {
        let err = merge_for_test(
            /* base   */
            r#"{"release":"B","base":"Grandparent","layers":[{"id":"base"}],
                "components":[{"id":"a","media":"M","layer":"base","rules":[]}]}"#,
            /* derived*/
            r#"{"release":"D","base":"B","layers":[{"id":"base"}],"components":[]}"#,
        )
        .expect_err("a base that is itself based on something is refused rather than chained");
        let text = err.to_string();
        assert!(
            text.contains("'D'") && text.contains("'B'"),
            "the refusal names both the recipe and the base it points at: {text}"
        );
    }

    /// **A standing guard for the direction the ignored 3.2.2 test cannot
    /// cover until Task 8.** `merge_base` stamps every inherited component
    /// with the derived recipe's *first* declared layer — this fixture
    /// declares two, `alpha` and `beta`, specifically so that stamping the
    /// wrong one is a different, nameable string rather than a coincidence.
    /// The assertion checks that string, not merely `Some(_)`: a mutation
    /// that stamped `beta` (the last layer) must produce a message showing
    /// `Some("beta")`, not a green test that never looked.
    #[test]
    fn a_based_recipes_inherited_components_are_stamped_with_the_first_layer_not_any_other() {
        let merged = merge_for_test(
            /* base   */
            r#"{"release":"B","components":[{"id":"inherited","media":"M","rules":[]}]}"#,
            /* derived*/
            r#"{"release":"D","base":"B",
                "layers":[{"id":"alpha"},{"id":"beta"}],
                "components":[{"id":"own","media":"M2","layer":"beta","rules":[]}]}"#,
        )
        .expect("a two-layer derived recipe resolves against its base");
        assert_eq!(
            merged.component("inherited").unwrap().layer.as_deref(),
            Some("alpha"),
            "the inherited component must be stamped with the recipe's first declared layer \
             ('alpha'), not 'beta' or any other"
        );
    }

    /// **Fix round 1, Finding 2.** `validate()` skips `validate_removals`
    /// whenever `recipe.base` is set (so a based recipe's own `removes` can
    /// legitimately name a destination the *base* places), and `merge_base`
    /// is what is supposed to run that check once, explicitly, against the
    /// fully merged component list. Nothing else calls `validate_removals`
    /// on a based recipe — so if that one line in `merge_base` is ever
    /// deleted, a based recipe's `removes` stops being checked at all, and
    /// this is the test that would notice: `a` (the base's own component)
    /// places `Tools/X` as a `File`, `b` (the derived recipe's own, on the
    /// `up` layer) removes it without declaring an `overrides` over `a` —
    /// the same undeclared-claim shape
    /// `a_removal_may_only_name_a_path_an_overridden_component_places`
    /// already refuses for an unbased recipe, asserted here across the
    /// `base` boundary instead.
    #[test]
    fn a_based_recipes_removal_is_still_checked_against_the_merged_component_list() {
        let err = merge_for_test(
            /* base   */
            r#"{"release":"B","components":[
                  {"id":"a","media":"M","rules":[{"from":"P","to":"Tools/X","kind":"file"}]}
                ]}"#,
            /* derived*/
            r#"{"release":"D","base":"B",
                "layers":[{"id":"base"},{"id":"up"}],
                "components":[
                  {"id":"b","media":"N","layer":"up","rules":[],"removes":["Tools/X"]}
                ]}"#,
        )
        .expect_err("b removes a's file across the base boundary without declaring it overrides a");
        let text = err.to_string();
        assert!(
            text.contains("'b'") && text.contains("Tools/X") && text.contains("'a'"),
            "names the remover, the path and the placer, even though the placer came from the \
             base rather than from this recipe's own file: {text}"
        );
    }

    // ---- Task 4: a component may remove a path an overridden one placed ----

    /// The brief's own written test: `b` removes `a`'s file without
    /// declaring an override over `a`.
    #[test]
    fn a_removal_may_only_name_a_path_an_overridden_component_places() {
        let err = parse(
            r#"{"release":"X",
                "components":[
                  {"id":"a","media":"M","rules":[{"from":"P","to":"Tools/X","kind":"file"}]},
                  {"id":"b","media":"N","rules":[],"removes":["Tools/X"]}
                ]}"#,
        )
        .expect_err("b removes a's file without declaring it overrides a");
        let text = err.to_string();
        assert!(text.contains("'b'") && text.contains("Tools/X") && text.contains("'a'"));
    }

    /// The other half of the same rule: declaring the override makes the
    /// identical recipe parse.
    #[test]
    fn a_removal_of_a_path_an_overridden_component_places_is_allowed() {
        let recipe = parse(
            r#"{"release":"X",
                "components":[
                  {"id":"a","media":"M","rules":[{"from":"P","to":"Tools/X","kind":"file"}]},
                  {"id":"b","media":"N","rules":[],"overrides":["a"],"removes":["Tools/X"]}
                ]}"#,
        )
        .expect("declaring the override is exactly what makes the removal legitimate");
        assert_eq!(recipe.component("b").unwrap().removes, vec!["Tools/X"]);
    }

    /// A `removes` entry naming a path **nobody** in the recipe places is a
    /// typo or a claim about a tree this recipe cannot see — either way, not
    /// something to build silently.
    #[test]
    fn a_removal_naming_a_path_nobody_places_is_refused() {
        let err = parse(
            r#"{"release":"X",
                "components":[
                  {"id":"a","media":"M","rules":[],"removes":["Tools/Nowhere"]}
                ]}"#,
        )
        .expect_err("nothing in this recipe places 'Tools/Nowhere'");
        let text = err.to_string();
        assert!(
            text.contains("'a'") && text.contains("Tools/Nowhere"),
            "{text}"
        );
    }

    /// `removes` is a `to`-shaped path like any other — an empty entry is
    /// exactly as meaningless as an empty `to`, and [`validate_path`] already
    /// refuses that for rules; `removes` must get the identical check.
    #[test]
    fn an_empty_removal_path_is_refused() {
        let err = parse(
            r#"{"release":"X",
                "components":[
                  {"id":"a","media":"M","rules":[],"removes":[""]}
                ]}"#,
        )
        .expect_err("an empty removal path is as meaningless as an empty destination");
        assert!(err.to_string().contains("'a'"));
    }

    /// **Fix round 1, Finding 1.** A `removes` entry naming a destination
    /// only a `Subtree` rule places is refused, even when the override is
    /// properly declared — `b` overrides `a` here, so the *only* thing
    /// wrong is that `a` places `Tools` as a whole drawer rather than as a
    /// file. `apply::perform_removal` cannot honestly report how many
    /// nested files a drawer removal took with it, so the format must not
    /// be able to say it at all.
    #[test]
    fn a_removal_of_a_path_a_subtree_rule_places_is_refused() {
        let err = parse(
            r#"{"release":"X",
                "components":[
                  {"id":"a","media":"M","rules":[{"from":"X","to":"Tools","kind":"subtree"}]},
                  {"id":"b","media":"N","rules":[],"overrides":["a"],"removes":["Tools"]}
                ]}"#,
        )
        .expect_err("a's placer for 'Tools' is a Subtree rule, not a File rule");
        let text = err.to_string();
        assert!(
            text.contains("'b'")
                && text.contains("Tools")
                && text.contains("'a'")
                && text.contains("drawer"),
            "names the remover, the path, the placer, and why: {text}"
        );
    }

    // ---- Task 8: the AmigaOS 3.2.2 recipe ----

    #[test]
    fn the_322_recipe_resolves_and_declares_two_layers() {
        let r = by_release("AmigaOS 3.2.2").unwrap();
        assert_eq!(r.layer_ids(), vec!["base", "update-3.2.2"]);
        assert_eq!(r.base.as_deref(), Some("AmigaOS 3.2"));
    }

    #[test]
    fn the_two_diskdoctors_are_two_components_in_two_layers() {
        let r = by_release("AmigaOS 3.2.2").unwrap();
        assert_eq!(
            r.component("diskdoctor").unwrap().layer.as_deref(),
            Some("base")
        );
        let update = r.component("update-322-diskdoctor").unwrap();
        assert_eq!(update.layer.as_deref(), Some("update-3.2.2"));
        assert_eq!(update.media, "DiskDoctor");
        assert!(update.overrides.iter().any(|o| o == "diskdoctor"));
    }

    /// **Fix round 1, Finding 1.** `update-322-diskdoctor`'s rules were
    /// copied from the base AmigaOS 3.2 recipe's own `diskdoctor` component
    /// rather than measured, and they were wrong in both directions: it
    /// placed `C/FixROMLibs`, which `Update3.2.2.adf`'s own installer
    /// (`Install/Install`) never copies, and it omitted
    /// `Devs/trackfile.device`, which the installer does copy. Both files
    /// are really on the `DiskDoctor` disk, so nothing refused and nothing
    /// failed — exactly the confident-wrong shape this project pays most
    /// for. Nothing read this component's rules before this test; now
    /// something does.
    #[test]
    fn update_322_diskdoctor_places_exactly_the_three_files_the_installer_does() {
        let r = by_release("AmigaOS 3.2.2").unwrap();
        let update = r.component("update-322-diskdoctor").unwrap();
        let pairs: Vec<(&str, &str)> = update
            .rules
            .iter()
            .map(|rule| (rule.from.as_str(), rule.to.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("C/DAControl", "C/DAControl"),
                ("C/DiskDoctor", "C/DiskDoctor"),
                ("Devs/trackfile.device", "Devs/trackfile.device"),
            ],
            "exactly what Update3.2.2.adf's own Install/Install script copies from DiskDoctor \
             — never C/FixROMLibs, which is the base 3.2 diskdoctor's own rule for a different \
             disk"
        );
        assert!(
            update.rules.iter().all(|rule| rule.kind == RuleKind::File),
            "all three are single files, never a Subtree"
        );
    }

    /// **Fix round 1, coordinator ruling.** `Update3.2.2.adf:Install/Install`
    /// copies both `Update3.2.2` and `Classes3.2.2` unconditionally — no
    /// `askbool`, no condition in front of either step — so both must be
    /// `required: true`: an optional Classes update would let a user decline
    /// it and still get a tree ART stamps `Release 3.2.2`, missing 31 files
    /// the release always places. The same shape as Finding 1's
    /// `Devs/trackfile.device` defect, one level up — and, like that one,
    /// invisible without a test that actually reads the field. A future edit
    /// flipping either one back to optional now has something to fail.
    #[test]
    fn update_322_system_and_update_322_classes_are_both_required() {
        let r = by_release("AmigaOS 3.2.2").unwrap();
        assert!(
            r.component("update-322-system").unwrap().required,
            "the release copies Update3.2.2 unconditionally"
        );
        assert!(
            r.component("update-322-classes").unwrap().required,
            "the release copies Classes3.2.2 unconditionally, in the same main flow, with no \
             askbool in front of it"
        );
    }

    #[test]
    fn every_locale_component_names_only_drawers_its_own_disk_carries() {
        // -EN carries Help alone; only -CZ, -RS and -RU carry Languages.
        let r = by_release("AmigaOS 3.2.2").unwrap();
        let en = r.component("update-322-locale-en").unwrap();
        assert!(en.rules.iter().all(|rule| rule.from.starts_with("Help")));
        for id in [
            "update-322-locale-cz",
            "update-322-locale-rs",
            "update-322-locale-ru",
        ] {
            assert!(
                r.component(id)
                    .unwrap()
                    .rules
                    .iter()
                    .any(|rule| rule.from == "Languages"),
                "{id} carries Languages"
            );
        }
        for id in ["update-322-locale-tr", "update-322-locale-de"] {
            assert!(
                !r.component(id)
                    .unwrap()
                    .rules
                    .iter()
                    .any(|rule| rule.from == "Languages"),
                // Fix round 1, Finding 4: this used to say "a rule for it
                // would refuse MediaMissing", which overstates it — every
                // locale disk carries a `Languages` drawer, empty on
                // fourteen of them (measured: `-TR` and `-DE` both have it,
                // with zero files inside), so a rule here would resolve and
                // place nothing, not refuse. Still wrong to write, because
                // it is not what the release's own `UPDATELOCALE` procedure
                // does for these two languages.
                "{id} does not carry a non-empty Languages drawer, and a rule for it would \
                 place an empty subtree rather than match what the release's own installer \
                 does"
            );
        }
    }

    #[test]
    fn nothing_places_the_drawers_the_release_leaves_alone() {
        let r = by_release("AmigaOS 3.2.2").unwrap();
        for component in &r.components {
            for rule in &component.rules {
                assert!(
                    !rule.from.starts_with("Other") && rule.from != "ReadMe",
                    "'{}' places '{}', which the release's own installer does not",
                    component.id,
                    rule.from
                );
            }
        }
    }

    #[test]
    fn the_ten_c_tools_the_release_does_not_place_are_not_placed() {
        let r = by_release("AmigaOS 3.2.2").unwrap();
        let system = r.component("update-322-system").unwrap();
        for tool in ["C/AmigaModel", "C/CopyTooltypes", "C/GuessBootDev"] {
            assert!(
                !system.rules.iter().any(|rule| rule.from == tool),
                "{tool} is an install-time helper, not a file the release places"
            );
        }
        assert_eq!(
            system
                .rules
                .iter()
                .filter(|r| r.from.starts_with("C/"))
                .count(),
            10
        );
    }

    #[test]
    fn the_update_removes_the_file_the_release_removes() {
        let r = by_release("AmigaOS 3.2.2").unwrap();
        let system = r.component("update-322-system").unwrap();
        assert!(system
            .removes
            .iter()
            .any(|p| p == "Tools/TextEditFileTypes/Default4Types"));
        assert!(system.overrides.iter().any(|o| o == "extras"));
    }

    #[test]
    fn the_empty_3_2_1_placeholder_is_gone() {
        assert!(amigaos_32().unwrap().component("update-3.2.1").is_none());
    }
}
