//! The shipped recipes, as data.
//!
//! `include_str!` for the same three reasons `core/distro` uses it: reviewable
//! in a diff, shipped without a network, and unable to grow a code path.

use super::Recipe;
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

/// The shipped AmigaOS 3.9 recipe — one component today (`workbench-base`,
/// the base tree out of `OS-VERSION3.9/WORKBENCH3.5`). Everything else waits
/// for a real boot to say whether the base needs it; see the recipe file's
/// own comment and CLAUDE.md's "don't claim support that isn't implemented
/// and tested" rule (spec §89).
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
    /// **A new recipe file (3.9's second component, 3.1, CaffeineOS, …) must
    /// be added here too.** Nothing makes that automatic — `include_str!`
    /// gives each recipe its own named constant, and there is no build step
    /// that walks `recipes/` and discovers new files — so a property this
    /// project states as holding "over the shipped recipe" (singular in the
    /// old wording, plural in truth: CLAUDE.md's own destination-collision
    /// rule and `no_two_components_claim_one_destination_without_declaring_it`'s
    /// own doc comment both say "the shipped recipe" while only 3.2 existed)
    /// only actually holds for whatever is listed here. The fix-round finding
    /// this function exists to close was exactly that: a second recipe
    /// (3.9) shipped with one component, so the collision test above stayed
    /// green by construction — dormant, not passing — right up to the day a
    /// second component gives it something to actually check.
    fn shipped_recipes() -> Vec<(&'static str, Recipe)> {
        vec![
            ("AmigaOS 3.2", recipe()),
            (
                "AmigaOS 3.9",
                parse(AMIGAOS_39_JSON).expect("the shipped 3.9 recipe must parse and validate"),
            ),
        ]
    }

    /// [`shipped_recipes`]'s own raw counterpart — see [`raw_recipe`]'s doc
    /// comment for why a test that means to police `validate` itself has to
    /// deserialise directly rather than go through `parse`.
    fn raw_shipped_recipes() -> Vec<(&'static str, Recipe)> {
        vec![
            ("AmigaOS 3.2", raw_recipe()),
            (
                "AmigaOS 3.9",
                serde_json::from_str(AMIGAOS_39_JSON)
                    .expect("the shipped 3.9 recipe must at least deserialise"),
            ),
        ]
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
        for (release, recipe) in shipped_recipes() {
            for component in &recipe.components {
                for over in &component.overrides {
                    assert!(
                        recipe.component(over).is_some(),
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
}
