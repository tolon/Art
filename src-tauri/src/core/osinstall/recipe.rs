//! The shipped recipes, as data.
//!
//! `include_str!` for the same three reasons `core/distro` uses it: reviewable
//! in a diff, shipped without a network, and unable to grow a code path.

use super::Recipe;
use crate::core::error::{CoreError, CoreResult};

const AMIGAOS_32_JSON: &str = include_str!("recipes/amigaos-3.2.json");

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

/// Everything a recipe must get right before ART trusts it: no two
/// components sharing an id, every component naming real media, and every
/// path — media-side `from` as well as tree-side `to` — a name AmigaDOS can
/// actually store, inside the tree it's rooted in.
fn validate(recipe: &Recipe) -> CoreResult<()> {
    let mut seen_ids = std::collections::HashSet::new();
    for component in &recipe.components {
        if !seen_ids.insert(component.id.as_str()) {
            return Err(CoreError::Malformed {
                format: "recipe".into(),
                detail: format!("two components share the id '{}'", component.id),
            });
        }
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
    }
    Ok(())
}

/// The shipped AmigaOS 3.2 recipe.
pub fn amigaos_32() -> CoreResult<Recipe> {
    parse(AMIGAOS_32_JSON)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::osinstall::{Component, Condition, RuleKind};
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
        for component in &raw_recipe().components {
            for rule in &component.rules {
                for segment in rule.to.split('/') {
                    crate::core::volume::write::dir::check_name(segment).unwrap_or_else(|e| {
                        panic!(
                            "{}: destination '{}' segment '{segment}': {e}",
                            component.id, rule.to
                        )
                    });
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
    #[test]
    fn no_two_components_claim_one_destination_without_declaring_it() {
        let recipe = recipe();
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
                "'{path}' is claimed by {:?} and none of them declares an override over all the rest",
                claiming.iter().map(|c| c.id.as_str()).collect::<Vec<_>>()
            );
        }
    }

    /// The ModulesA1200 lesson, generalised. Four disks in this set repeat
    /// `workbench-base`'s own `C/` almost entirely; a component that declares
    /// an override and then lazily takes the whole drawer would pass the
    /// collision test above and still downgrade the user's commands.
    #[test]
    fn no_toolkit_disk_takes_a_whole_drawer_the_base_already_owns() {
        let recipe = recipe();
        let base: std::collections::HashSet<&str> = recipe
            .component("workbench-base")
            .unwrap()
            .rules
            .iter()
            .map(|r| r.to.as_str())
            .collect();

        for id in [
            "modules-a1200",
            "diskdoctor",
            "mmulibs",
            "hdtools",
            "storage",
        ] {
            let component = recipe.component(id).unwrap();
            for rule in &component.rules {
                assert!(
                    !(base.contains(rule.to.as_str()) && rule.kind == RuleKind::Subtree),
                    "{id} takes the whole '{}' drawer, which workbench-base owns — \
                     take the files that are actually new instead",
                    rule.to
                );
            }
        }
    }

    #[test]
    fn every_override_names_a_component_that_exists() {
        let recipe = recipe();
        for component in &recipe.components {
            for over in &component.overrides {
                assert!(
                    recipe.component(over).is_some(),
                    "{}: no such component '{over}'",
                    component.id
                );
            }
        }
    }

    /// Also built on `raw_recipe()` — see the comment on
    /// `every_destination_is_a_name_amigados_can_store` above.
    #[test]
    fn no_rule_escapes_the_tree() {
        for component in &raw_recipe().components {
            for rule in &component.rules {
                assert!(
                    !rule.to.starts_with('/'),
                    "{}: '{}' is absolute",
                    component.id,
                    rule.to
                );
                assert!(
                    !rule.to.split('/').any(|s| s == ".."),
                    "{}: '{}' climbs",
                    component.id,
                    rule.to
                );
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
        assert!(
            !recipe.component("backdrops").unwrap().available,
            "Backdrops3.2 stays off until somebody measures where the real \
             installer places wallpapers"
        );
        assert!(!recipe.component("update-3.2.1").unwrap().available);
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
}
