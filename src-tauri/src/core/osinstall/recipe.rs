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

/// Everything a recipe must get right before ART trusts it: no two
/// components sharing an id, every component naming real media, and every
/// destination a name AmigaDOS can actually store, inside the tree.
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
            for segment in rule.to.split('/') {
                crate::core::volume::write::dir::check_name(segment)?;
            }
            if rule.to.starts_with('/') || rule.to.split('/').any(|s| s == "..") {
                return Err(CoreError::Malformed {
                    format: "recipe".into(),
                    detail: format!(
                        "'{}': destination '{}' leaves the tree",
                        component.id, rule.to
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::osinstall::RuleKind;
    use std::collections::HashMap;

    fn recipe() -> Recipe {
        parse(AMIGAOS_32_JSON).expect("the shipped 3.2 recipe must parse and validate")
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

    #[test]
    fn every_destination_is_a_name_amigados_can_store() {
        for component in &recipe().components {
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

    #[test]
    fn no_two_components_claim_one_destination_without_declaring_it() {
        let recipe = recipe();
        let mut owner: HashMap<&str, &str> = HashMap::new();
        for component in &recipe.components {
            for rule in &component.rules {
                if let Some(first) = owner.insert(rule.to.as_str(), component.id.as_str()) {
                    assert!(
                        component.overrides.iter().any(|o| o == first),
                        "'{}' and '{}' both write '{}' and neither declared an override",
                        first,
                        component.id,
                        rule.to
                    );
                }
            }
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

    #[test]
    fn no_rule_escapes_the_tree() {
        for component in &recipe().components {
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
}
