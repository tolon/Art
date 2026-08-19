//! What a package recipe says: which files an official (or unofficial)
//! update archive contributes, and where each one goes.
//!
//! A package is the same shape a release recipe already uses — [`Component`]
//! — because it goes through the same placer once a future task applies it.
//! What differs is the wrapper: a package is chosen by the user, never
//! switched on by a ROM version (`Component.required` is always `false` and
//! `.condition` always `None`, see [`parse`]), it may depend on another
//! package having been applied first (`requires`), and its media may not be
//! the archive a person downloaded at all but a payload archive nested
//! inside it (`member`, read via
//! [`source_archive::ArchiveSource::open_nested`](super::source_archive::ArchiveSource::open_nested)).
//!
//! ## The mechanism, copied from `recipe.rs`
//!
//! `include_str!` for the same three reasons: reviewable in a diff, shipped
//! without a network, unable to grow a code path. [`packages`] is the one
//! list a fourth package JSON has to join to be reachable at all — from
//! there, [`by_id`] resolving it, `recipe.rs`'s widened invariant tests
//! reaching it, and [`order`] sequencing it all fall out for free, the same
//! way `recipe.rs`'s doc comment on `shipped_recipes` explains for releases.
//!
//! ## Why every shipped package's `overrides` is empty
//!
//! BoingBag 3.9-1 and 3.9-2 both write into drawers the base AmigaOS 3.9
//! recipe's `workbench-base` component already places, and 3.9-2 writes into
//! every drawer 3.9-1 does. Declaring that in `overrides` would read
//! naturally, but two things make it either meaningless or unresolvable
//! today: every rule in all three shipped packages is `subtree`, and
//! `no_two_components_claim_one_destination_without_declaring_it` — the test
//! `overrides` exists to satisfy — only polices `file` rules, treating a
//! Subtree destination as a merge point rather than a claim (see that test's
//! own comment in `recipe.rs`). And `Component.overrides` is resolved
//! against **one recipe's own component list** — `recipe.rs`'s
//! `every_override_names_a_component_that_exists` checks
//! `recipe.component(over)`, where `recipe` is whichever single package's
//! own one-component pseudo-recipe `shipped_recipes()` built for it. A
//! package's `overrides` can therefore only ever name another *package*
//! (`boingbag-39-2` could legitimately declare `["boingbag-39-1"]`), never a
//! release recipe's component — `workbench-base` lives in a different
//! recipe's list and would never resolve there. Since nothing here needs it
//! resolved (Subtree rules again), every shipped package leaves it empty
//! rather than declaring something that cannot be checked. Which file wins
//! when two packages — or a package and the release it sits on top of — both
//! place the same *named file* inside a shared drawer is a question for the
//! engine that actually applies a package onto a built tree, which this task
//! does not build.
//!
//! ## `requires` is a dependency, not a suggestion
//!
//! BoingBag 3.9-2 assumes BoingBag 3.9-1 is already on the volume (spec §8:
//! applying them the other way round is a wrong system, not a warning). So
//! [`order`] topologically sorts a chosen set by `requires` — the order the
//! user ticked boxes in is not the order that gets applied — and refuses,
//! by name, either a requirement that was not itself chosen or a cycle in
//! the shipped data (which would otherwise hang whatever applies them).

use std::collections::{HashMap, HashSet, VecDeque};

use serde::Deserialize;

use super::recipe::validate_component;
use super::{Component, PathRule};
use crate::core::error::{CoreError, CoreResult};

const BOINGBAG_39_1_JSON: &str = include_str!("recipes/packages/boingbag-39-1.json");
const BOINGBAG_39_2_JSON: &str = include_str!("recipes/packages/boingbag-39-2.json");
const LOCALE_TURKISH_JSON: &str = include_str!("recipes/packages/locale-turkish.json");

/// Every package JSON this project ships. The one list a new package JSON
/// has to join — see the module doc comment.
const SHIPPED_JSON: &[&str] = &[BOINGBAG_39_1_JSON, BOINGBAG_39_2_JSON, LOCALE_TURKISH_JSON];

/// An update package on top of an installed AmigaOS tree — an official
/// BoingBag or an unofficial pack like the Turkish catalogs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub id: String,
    /// Shown on screen. Not the id, and not translated — a package's name is
    /// its own, the way a volume name is (ART-060).
    pub name: String,
    /// The archive's single top-level directory, read from inside it.
    pub media: String,
    /// The member holding the payload, for a package whose files sit inside
    /// a second archive. `None` for loose files at direct paths.
    pub member: Option<String>,
    pub requires: Vec<String>,
    /// The rules and `overrides`, in the shape the placer already takes.
    pub component: Component,
}

/// The flat shape a person actually edits. `id`/`media`/`rules`/`overrides`
/// fold into [`Package::component`] at parse time — the file on disk should
/// not have to know the engine's own struct layout.
#[derive(Debug, Deserialize)]
struct RawPackage {
    id: String,
    name: String,
    media: String,
    #[serde(default)]
    member: Option<String>,
    #[serde(default)]
    requires: Vec<String>,
    #[serde(default)]
    overrides: Vec<String>,
    rules: Vec<PathRule>,
}

impl RawPackage {
    /// Fold the flat JSON shape into a [`Package`], without validating it —
    /// shared by [`parse`], which validates, and
    /// [`raw_packages`](self::raw_packages), which deliberately does not
    /// (see that function's own doc comment).
    fn into_package(self) -> Package {
        let component = Component {
            id: self.id.clone(),
            media: self.media.clone(),
            rules: self.rules,
            // A package is something the user chooses, never something a
            // ROM version switches on (Task 4's brief, verbatim).
            required: false,
            condition: None,
            overrides: self.overrides,
            user_startup: Vec::new(),
            exclusive_group: None,
            available: true,
        };
        Package {
            id: self.id,
            name: self.name,
            media: self.media,
            member: self.member,
            requires: self.requires,
            component,
        }
    }
}

/// Parse and validate one package's JSON — `recipe::parse`'s own validation
/// shape, applied to the single component a package wraps.
fn parse(json: &str) -> CoreResult<Package> {
    let raw: RawPackage = serde_json::from_str(json).map_err(|e| CoreError::Malformed {
        format: "package".into(),
        detail: e.to_string(),
    })?;
    let package = raw.into_package();
    validate_component(&package.component)?;
    Ok(package)
}

/// Every package ART ships, parsed and validated. Order here is not
/// application order — see [`order`] for that.
pub fn packages() -> CoreResult<Vec<Package>> {
    SHIPPED_JSON.iter().map(|json| parse(json)).collect()
}

/// The shipped JSON, deserialised **without** going through
/// [`validate_component`] — `recipe.rs`'s own `raw_recipe`'s reason for
/// existing, applied here: a test built on [`packages`] alone would already
/// have failed via `?` on any bad name, so a test that means to police the
/// *shipped data* independently of whether `validate_component` itself has a
/// bug has to deserialise directly. Test-only, like its `recipe.rs`
/// counterpart, but `pub(crate)` rather than module-private: `recipe.rs`'s
/// own test module needs it too, to widen `raw_shipped_recipes()` (Task 4,
/// Step 3).
#[cfg(test)]
pub(crate) fn raw_packages() -> Vec<Package> {
    SHIPPED_JSON
        .iter()
        .map(|json| {
            let raw: RawPackage = serde_json::from_str(json)
                .unwrap_or_else(|e| panic!("a shipped package must at least deserialise: {e}"));
            raw.into_package()
        })
        .collect()
}

/// One shipped package by id.
///
/// An unknown id is refused rather than defaulted — the same rule
/// `recipe::by_release` follows and for the same reason: picking a different
/// package than the one asked for would be wrong data reaching the volume,
/// not a warning.
pub fn by_id(id: &str) -> CoreResult<Package> {
    packages()?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| CoreError::InvalidInput(format!("ART ships no package '{id}'")))
}

/// [`order`]'s own machinery, parameterised over the package list — so a
/// test can hand it a small, hand-built [`Package`] set that contains a
/// cycle, rather than only ever exercising it through the three shipped,
/// cycle-free packages. [`order`] is the thin wrapper that calls this with
/// [`packages`].
///
/// A plain Kahn's-algorithm topological sort over `chosen`'s own `requires`
/// edges. Every requirement is checked against `chosen` itself first — a
/// requirement that exists as a package but was not itself chosen is refused
/// by name rather than silently pulled in, because adding a whole package
/// the user did not ask for is a bigger surprise than a refusal. What is
/// left unsorted after the sort — some id whose in-degree never reached zero
/// — is exactly a cycle, refused by name rather than left to hang whatever
/// applies the result.
fn order_over(chosen: &[String], all: &[Package]) -> CoreResult<Vec<String>> {
    let index: HashMap<&str, &Package> = all.iter().map(|p| (p.id.as_str(), p)).collect();
    let chosen_set: HashSet<&str> = chosen.iter().map(|s| s.as_str()).collect();

    for id in chosen {
        let package = index
            .get(id.as_str())
            .ok_or_else(|| CoreError::InvalidInput(format!("ART ships no package '{id}'")))?;
        for need in &package.requires {
            if !chosen_set.contains(need.as_str()) {
                return Err(CoreError::InvalidInput(format!(
                    "'{id}' requires '{need}', which was not chosen"
                )));
            }
        }
    }

    let mut in_degree: HashMap<&str, usize> = chosen.iter().map(|id| (id.as_str(), 0)).collect();
    let mut dependents: HashMap<&str, Vec<&str>> =
        chosen.iter().map(|id| (id.as_str(), Vec::new())).collect();

    for id in chosen {
        let package = index[id.as_str()];
        for need in &package.requires {
            dependents.get_mut(need.as_str()).unwrap().push(id.as_str());
            *in_degree.get_mut(id.as_str()).unwrap() += 1;
        }
    }

    // Ties broken by `chosen`'s own order, never a `HashMap`'s iteration
    // order — nothing here should depend on that.
    let mut queue: VecDeque<&str> = chosen
        .iter()
        .map(|s| s.as_str())
        .filter(|id| in_degree[id] == 0)
        .collect();

    let mut result: Vec<String> = Vec::with_capacity(chosen.len());
    while let Some(id) = queue.pop_front() {
        result.push(id.to_string());
        for &next in &dependents[id] {
            let degree = in_degree.get_mut(next).unwrap();
            *degree -= 1;
            if *degree == 0 {
                queue.push_back(next);
            }
        }
    }

    if result.len() != chosen.len() {
        return Err(CoreError::InvalidInput(
            "the chosen packages contain a dependency cycle".to_string(),
        ));
    }

    Ok(result)
}

/// `chosen`, reordered so every package comes after everything it
/// `requires` — the order an applier should actually use, independent of
/// the order the user happened to tick the boxes in.
pub fn order(chosen: &[String]) -> CoreResult<Vec<String>> {
    order_over(chosen, &packages()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::osinstall::RuleKind;

    /// Every shipped package parses and validates. This is the test that
    /// fails when a JSON file is added and its rules are wrong.
    #[test]
    fn every_shipped_package_parses() {
        for p in super::packages().expect("the shipped packages must parse") {
            assert!(!p.id.is_empty());
            assert!(!p.media.is_empty());
            assert!(!p.component.rules.is_empty(), "{} has no rules", p.id);
        }
    }

    /// `requires` names a package that exists. A dependency on something ART
    /// does not ship is a recipe that can never be satisfied.
    #[test]
    fn every_requirement_names_a_shipped_package() {
        let all = super::packages().unwrap();
        let ids: Vec<&str> = all.iter().map(|p| p.id.as_str()).collect();
        for p in &all {
            for need in &p.requires {
                assert!(
                    ids.contains(&need.as_str()),
                    "{} requires unknown {need}",
                    p.id
                );
            }
        }
    }

    /// BoingBag 2 assumes BoingBag 1. Applying them the other way round is a
    /// wrong system, not a warning (spec §8).
    #[test]
    fn boingbag_two_requires_boingbag_one() {
        let two = super::by_id("boingbag-39-2").unwrap();
        assert!(two.requires.contains(&"boingbag-39-1".to_string()));
    }

    /// An unknown id is refused by name, never defaulted to some other
    /// package — the same rule `recipe::by_release` follows, for the same
    /// reason.
    #[test]
    fn an_unknown_package_is_refused_by_name() {
        let err = super::by_id("boingbag-39-9").unwrap_err().to_string();
        assert!(err.contains("boingbag-39-9"), "got {err}");
    }

    /// Ordering is derived from `requires`, not from the order the user
    /// happened to tick the boxes in.
    #[test]
    fn selection_order_does_not_decide_application_order() {
        let a = super::order(&["boingbag-39-2".into(), "boingbag-39-1".into()]).unwrap();
        let b = super::order(&["boingbag-39-1".into(), "boingbag-39-2".into()]).unwrap();
        assert_eq!(a, b);
        assert_eq!(
            a,
            vec!["boingbag-39-1".to_string(), "boingbag-39-2".to_string()]
        );
    }

    /// Choosing a package without what it requires is refused, saying what
    /// is missing — not silently added, because adding a whole package the
    /// user did not ask for is a bigger surprise than a refusal.
    #[test]
    fn a_requirement_that_was_not_chosen_is_refused_by_name() {
        let err = super::order(&["boingbag-39-2".into()])
            .unwrap_err()
            .to_string();
        assert!(err.contains("boingbag-39-1"), "got {err}");
    }

    /// A minimal, valid `Package` for the synthetic tests below — real
    /// enough to pass `validate_component` (one file rule), not meant to
    /// resemble any shipped package.
    fn synthetic(id: &str, requires: &[&str]) -> Package {
        Package {
            id: id.to_string(),
            name: id.to_string(),
            media: "SyntheticMedia".to_string(),
            member: None,
            requires: requires.iter().map(|s| s.to_string()).collect(),
            component: Component {
                id: id.to_string(),
                media: "SyntheticMedia".to_string(),
                rules: vec![PathRule {
                    from: "X".to_string(),
                    to: "X".to_string(),
                    kind: RuleKind::File,
                }],
                required: false,
                condition: None,
                overrides: Vec::new(),
                user_startup: Vec::new(),
                exclusive_group: None,
                available: true,
            },
        }
    }

    /// `order` only ever reads the shipped list ([`packages`]), which has no
    /// cycle in it — a hand-built one is the only way to exercise the cycle
    /// branch at all, which is why this drives [`order_over`] directly
    /// rather than [`order`] itself.
    #[test]
    fn a_dependency_cycle_in_hand_built_data_is_refused_rather_than_hung() {
        let x = synthetic("x", &["y"]);
        let y = synthetic("y", &["x"]);
        let err = super::order_over(&["x".to_string(), "y".to_string()], &[x, y])
            .unwrap_err()
            .to_string();
        assert!(err.contains("cycle"), "got {err}");
    }

    /// A three-package chain, out of order and with the middle package
    /// requiring the last rather than the first — the shipped two-package
    /// case can't tell a stable sort from a lucky one.
    #[test]
    fn a_longer_chain_still_sorts_correctly_regardless_of_input_order() {
        let a = synthetic("a", &[]);
        let b = synthetic("b", &["a"]);
        let c = synthetic("c", &["b"]);
        let chosen = vec!["c".to_string(), "a".to_string(), "b".to_string()];
        let ordered = super::order_over(&chosen, &[a, b, c]).unwrap();
        assert_eq!(
            ordered,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }
}
