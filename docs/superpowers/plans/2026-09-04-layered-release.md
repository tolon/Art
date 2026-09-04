# Layered release implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let ART express an AmigaOS release that arrives as a base plus an update, so the owner's own 3.2 + 3.2.2 media builds a tree that says `Release 3.2.2`.

**Architecture:** A recipe declares ordered **media layers** and each component says which layer its `media` lives in, so nothing is resolved by the order the user clicked in. A recipe may `base` itself on another and inherit its components. Three smaller capabilities the 3.2.2 update actually needs come with it: a component that `removes` a path, an `.info` tooltype-and-stack merger, and a reader for a Kickstart's resident table so a condition can ask what the release's own installer asks.

**Tech Stack:** Rust (`src-tauri/src/core/osinstall`, `core/amigaicon`, `core/rom`), recipe JSON compiled in with `include_str!`, React + `react-i18next` for the media step, Python for the oracle script.

**Spec:** [`docs/superpowers/specs/2026-09-04-layered-release-design.md`](../specs/2026-09-04-layered-release-design.md), which argues from [`docs/superpowers/specs/2026-09-04-layered-release-research.md`](../specs/2026-09-04-layered-release-research.md). Read both; the research note is where every number in the recipe comes from.

## Global Constraints

- **`core/` stays platform-independent.** `std` + `serde` + `serde_json` + `sha2` + `log` + `thiserror` + `delharc` + `zip` + `sevenz-rust2` + `quick-xml` + `fatfs` + `libpfs3` only. `core/amigaicon` adds **no dependency at all**.
- **MSRV 1.93.** Clippy is blocking at `-D warnings`; `lib.rs` allows only `dead_code`.
- **Fixtures are synthetic and generated at runtime in a tempdir.** ART ships no copyrighted Amiga content, ever. In `core/osinstall` the convention is that module's own `fixtures::scratch(tag)` / `fixtures::media(dir, volume, filename, entries)`; `scratch` already appends `core::test_scratch_id()`, the **process-wide atomic counter** (never `as_nanos()` alone). Elsewhere in the crate the RAII `core::ScratchDir` is the convention — follow whichever the file being edited already uses rather than introducing the other.
- **i18n: every key lands in `src/i18n/en.json` *and* `src/i18n/tr.json` in the same commit.** `pnpm test` fails the build if the key sets differ.
- **A test is not a guard until the defect has been put back and seen to fail it.** Every task with a `Mutations` block runs them and reports what fell. A survivor is reported as a survivor, after asking which of the two things it is: a weak guard, or the wrong mutation for it.
- **Never pass a commit message as a double-quoted shell string.** Write it to a file and `git commit -F`.
- **Branch:** `art-layered-release`, already created. Merge to `main` with `--no-ff` when the plan is done.
- **Bounds:** every length computed from file bytes uses `checked_add`/`checked_mul` against the buffer's real length. The release profile sets `panic = "abort"`; an out-of-range index kills the application.

## Verification commands

```bash
cd src-tauri && cargo test osinstall::          # the module under change
cd src-tauri && cargo test amigaicon::          # task 5's module
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings
cd src-tauri && cargo test                      # twice, per the standing rule
pnpm lint && pnpm test
python scripts/control-byte-sweep.py
python scripts/scratch-counter-sweep.py
```

**Quote the `test result:` line, never the exit code.** A killed harness and a
green suite look identical from the shell.

---

## File structure

| File | Responsibility | Task |
|---|---|---|
| `src-tauri/src/core/osinstall/mod.rs` | `MediaLayer`, `Component.layer`, `Component.removes`, `RuleKind::IconTooltypes`, `Condition::ResidentOlderThan`, `RefusalReason` additions | 1, 4, 6, 7 |
| `src-tauri/src/core/osinstall/recipe.rs` | key lists, `validate`, `base` resolution, `releases()`, `by_release` | 1, 2, 8 |
| `src-tauri/src/core/osinstall/recipes/amigaos-3.2.2.json` | the new recipe (created) | 8 |
| `src-tauri/src/core/osinstall/recipes/amigaos-3.2.json` | the empty `update-3.2.1` placeholder goes | 8 |
| `src-tauri/src/core/osinstall/scan.rs` | `FoundMedia.layer`, per-layer scan and dedupe, same-folder refusal | 3 |
| `src-tauri/src/core/osinstall/plan.rs` | `InstallRequest.media_folders`, per-layer `media_for`, removal items, the new rule kind | 3, 4, 6 |
| `src-tauri/src/core/osinstall/apply.rs` | applying removals and the icon merge, `DistributionManifest.layers` | 4, 6, 9 |
| `src-tauri/src/core/osinstall/identify.rs` | `release_of_tree`, per-layer media identification | 9 |
| `src-tauri/src/core/amigaicon/mod.rs` | `.info` layout, tooltypes, stack, merge (created) | 5 |
| `src-tauri/src/commands/osinstall.rs` | the folder-per-layer adapter, the release sentence, `osinstall_layers` | 3, 9, 10 |
| `src-tauri/src/lib.rs` | `invoke_handler![]` gains `osinstall_layers` | 10 |
| `src-tauri/src/core/rom/mod.rs` | the Kickstart resident-table reader | 7 |
| `src/lib/osinstall.ts` | the typed request wrapper | 3 |
| `src/components/osbuilder/OsInstall.tsx` | one labelled folder field per layer | 10 |
| `src/i18n/{en,tr}.json` | layer labels, the removal and icon verdicts, the release sentence | 4, 6, 8, 9, 10 |
| `scripts/icon-oracle-check.py` | every `.info` on the owner's media round-trips (created) | 11 |

---

## Task 1: Layers in the recipe format

**Files:**
- Modify: `src-tauri/src/core/osinstall/mod.rs` (near `Component`, line ~248, and `Recipe`, line ~305)
- Modify: `src-tauri/src/core/osinstall/recipe.rs:33-49` (key lists), `:225-237` (`validate`)
- Test: inline `#[cfg(test)]` in `recipe.rs`

**Interfaces:**
- Consumes: nothing
- Produces: `MediaLayer { id: String, label_key: Option<String> }`; `Recipe.layers: Vec<MediaLayer>`; `Component.layer: Option<String>`; `Recipe::layer_ids() -> Vec<&str>`; `Recipe::is_layered() -> bool`

- [ ] **Step 1: Write the failing tests**

In `recipe.rs`'s `mod tests`:

```rust
#[test]
fn a_recipe_with_no_layers_is_unlayered_and_components_need_no_layer() {
    let recipe = parse(
        r#"{"release":"X","components":[{"id":"a","media":"M","rules":[]}]}"#,
    )
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
    assert_eq!(recipe.layers[0].label_key.as_deref(), Some("osinstall.layer.base32"));
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cd src-tauri && cargo test osinstall::recipe:: -- layer`
Expected: FAIL — `Recipe` has no field `layers`, `Component` has no field `layer`.

- [ ] **Step 3: Add the types**

In `mod.rs`, beside `Recipe`:

```rust
/// One set of install media a recipe reads from, named so a component can
/// say which set its `media` lives in.
///
/// **A layer is stated by the recipe, never inferred from the order folders
/// were added.** Two AmigaOS releases can ship a disk under one volume name —
/// the owner's 3.2 and 3.2.2 sets each carry a `DiskDoctor`, 901 120 bytes
/// apiece with different SHA-256s — and which of them a component wants is a
/// fact about the release, not about which folder somebody picked first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaLayer {
    pub id: String,
    /// An **i18n key** for this layer's own folder field on the media step,
    /// for the reason `Component::label_key` is a key: the recipe is data in
    /// the Rust tree with no compiler between it and the screen.
    #[serde(default)]
    pub label_key: Option<String>,
}
```

On `Recipe`:

```rust
    /// The media sets this recipe reads from, in the order a release states.
    /// Empty means one implicit layer, which is every recipe that shipped
    /// before this existed.
    #[serde(default)]
    pub layers: Vec<MediaLayer>,
```

On `Component`, beside `media`:

```rust
    /// Which [`MediaLayer`] this component's `media` lives in. `None` is the
    /// only legal value in an unlayered recipe and is refused in a layered
    /// one — a component that searched every layer would be resolving by
    /// order, which is the thing this design exists to avoid.
    #[serde(default)]
    pub layer: Option<String>,
```

And on `impl Recipe`:

```rust
    pub fn is_layered(&self) -> bool {
        !self.layers.is_empty()
    }

    pub fn layer_ids(&self) -> Vec<&str> {
        self.layers.iter().map(|l| l.id.as_str()).collect()
    }
```

- [ ] **Step 4: Teach the key checker and `validate` about them**

`recipe.rs:33`:

```rust
const RECIPE_KEYS: &[&str] = &["release", "components", "layers"];
const LAYER_KEYS: &[&str] = &["id", "label_key"];
```

Add `"layer"` to `COMPONENT_KEYS`. In `check_unknown_keys`, after the recipe-level check:

```rust
    if let Some(layers) = value.get("layers").and_then(|l| l.as_array()) {
        for layer in layers {
            let id = layer
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("a layer with no id");
            check_keys(layer, LAYER_KEYS, &format!("layer '{id}'"))?;
        }
    }
```

In `validate`, before the component loop:

```rust
    let mut seen_layers = std::collections::HashSet::new();
    for layer in &recipe.layers {
        if !seen_layers.insert(layer.id.as_str()) {
            return Err(CoreError::Malformed {
                format: "recipe".into(),
                detail: format!("two layers share the id '{}'", layer.id),
            });
        }
    }
```

and inside it, per component:

```rust
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
                        "component '{}' names the layer '{named}', which this recipe does not declare",
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
```

- [ ] **Step 5: Run the tests**

Run: `cd src-tauri && cargo test osinstall::recipe::`
Expected: PASS, and the two shipped recipes still parse (`amigaos_32`, `amigaos_39` are already covered by existing tests).

- [ ] **Step 6: Mutations — put each defect back and watch it fall**

| Mutation | Test that must fail |
|---|---|
| delete the `(true, None)` arm | `a_component_in_a_layered_recipe_must_name_its_layer` |
| delete the `(true, Some(named)) if !contains` arm | `a_component_may_not_name_a_layer_the_recipe_does_not_declare` |
| delete the duplicate-layer check | `two_layers_may_not_share_an_id` |
| drop `"layers"` from `RECIPE_KEYS` | every layered-recipe test — the key is refused as unknown |

Restore each file with `shutil.copyfile` (**not** `move`) and `touch` it afterwards, or cargo compiles the mutation on the next run.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/core/osinstall/mod.rs src-tauri/src/core/osinstall/recipe.rs
git commit -F <message file>
```

Message: `feat(osinstall): a recipe may declare ordered media layers`

---

## Task 2: A recipe may be based on another

**Files:**
- Modify: `src-tauri/src/core/osinstall/recipe.rs` (`RECIPE_KEYS`, `parse`, a new `resolve_base`)
- Modify: `src-tauri/src/core/osinstall/mod.rs` (`Recipe.base`)
- Test: inline in `recipe.rs`

**Interfaces:**
- Consumes: Task 1's `MediaLayer`, `Recipe.layers`, `Component.layer`
- Produces: `Recipe.base: Option<String>`; `recipe::by_release` returns a **merged** recipe; `recipe::parse_unresolved(json) -> CoreResult<Recipe>` for tests that want the file as written

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_based_recipe_inherits_its_bases_components_on_the_first_layer() {
    let recipe = by_release("AmigaOS 3.2.2").expect("the 3.2.2 recipe resolves");
    let inherited = recipe
        .component("workbench-base")
        .expect("the base recipe's components are inherited");
    assert_eq!(inherited.layer.as_deref(), Some("base"));
    assert_eq!(
        recipe.component("update-322-system").unwrap().layer.as_deref(),
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
    let err = resolve_base(parse_unresolved(
        r#"{"release":"X","base":"AmigaOS 9.9",
            "layers":[{"id":"base"}],
            "components":[]}"#,
    ).unwrap())
    .expect_err("an unknown base is a recipe error");
    assert!(err.to_string().contains("AmigaOS 9.9"));
}

#[test]
fn a_based_recipe_may_not_redeclare_one_of_its_bases_component_ids() {
    let err = merge_for_test(
        /* base   */ r#"{"release":"B","components":[{"id":"a","media":"M","rules":[]}]}"#,
        /* derived*/ r#"{"release":"D","base":"B",
                        "layers":[{"id":"base"},{"id":"up"}],
                        "components":[{"id":"a","media":"M2","layer":"up","rules":[]}]}"#,
    )
    .expect_err("a redeclared id is a recipe error, not a silent replacement");
    assert!(
        err.to_string().contains("'a'"),
        "the refusal names the id that collides"
    );
}
```

`json_for` and `merge_for_test` are small test helpers in the same `mod tests`; write them there rather than exposing anything new.

- [ ] **Step 2: Run them and watch them fail**

Run: `cd src-tauri && cargo test osinstall::recipe:: -- base`
Expected: FAIL — no `base` field, no `AmigaOS 3.2.2`.

- [ ] **Step 3: Add the field and the resolver**

`mod.rs`, on `Recipe`:

```rust
    /// Another recipe's `release` string, whose components this one inherits.
    ///
    /// **A release update is layered, and the release says so itself**:
    /// AmigaOS 3.2.2's own `HowToInstall` requires "a successful installation
    /// of AmigaOS 3.2 or 3.2.1". Expressing that as `base` keeps the base's
    /// thirty-odd components in one file instead of two copies that drift.
    #[serde(default)]
    pub base: Option<String>,
```

`recipe.rs`: add `"base"` to `RECIPE_KEYS`, rename today's `parse` to `parse_unresolved`, and make `parse` resolve:

```rust
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
    if base.base.is_some() {
        return Err(CoreError::Malformed {
            format: "recipe".into(),
            detail: format!(
                "'{}' is based on '{base_release}', which is itself based on another recipe; \
                 ART resolves one level only",
                recipe.release
            ),
        });
    }
    let Some(first) = recipe.layers.first().map(|l| l.id.clone()) else {
        return Err(CoreError::Malformed {
            format: "recipe".into(),
            detail: format!(
                "'{}' is based on '{base_release}' but declares no layers, \
                 so there is nowhere to put the inherited components",
                recipe.release
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
                    "'{}' declares a component '{}' that '{base_release}' already declares; \
                     an update amends a component through `overrides`, never by reusing its id",
                    recipe.release, own.id
                ),
            });
        }
    }
    components.extend(recipe.components.iter().cloned());
    let resolved = Recipe { components, ..recipe };
    validate(&resolved)?;
    Ok(resolved)
}
```

`by_release_unresolved` is `by_release`'s match arm set calling `parse_unresolved`; `by_release` keeps calling `parse`.

- [ ] **Step 4: Run the tests**

Run: `cd src-tauri && cargo test osinstall::recipe::`
Expected: the `AmigaOS 3.2.2` tests still fail (Task 8 writes that file); the rest PASS. **Mark the two 3.2.2 tests `#[ignore]` with the reason `waiting for Task 8's recipe`, and remove the `#[ignore]` in Task 8.** Do not delete them — an ignored test with a named reason is a promise; a deleted one is nothing.

- [ ] **Step 5: Mutations**

| Mutation | Test that must fail |
|---|---|
| stamp the inherited components with the *last* layer | `a_based_recipe_inherits_its_bases_components_on_the_first_layer` |
| let a redeclared id silently replace the base's | `a_based_recipe_may_not_redeclare_one_of_its_bases_component_ids` |
| make `resolve_base` return the recipe untouched | the 3.2.2 inheritance test |
| drop the `base.base.is_some()` guard | write a two-level fixture and watch it resolve instead of refuse |

- [ ] **Step 6: Commit**

Message: `feat(osinstall): a recipe may inherit another release's components`

---

## Task 3: Media resolution per layer

**Files:**
- Modify: `src-tauri/src/core/osinstall/scan.rs:107-116` (`FoundMedia`), `:304-318` (`find_media_across`), `:320-…` (`dedupe_identical_disks`), `:547` (`media_for`)
- Modify: `src-tauri/src/core/osinstall/plan.rs:483` (`InstallRequest`), `:1199-1240` (resolution)
- Modify: `src-tauri/src/commands/osinstall.rs:272`
- Modify: `src/lib/osinstall.ts:76`
- Test: inline in `scan.rs` and `plan.rs`

**Interfaces:**
- Consumes: Task 1's `Component.layer`, `Recipe::layer_ids`
- Produces: `FoundMedia.layer: Option<String>`; `scan::find_media_in_layers(&[(String, PathBuf)]) -> CoreResult<Vec<FoundMedia>>`; `scan::media_for_layer(&[FoundMedia], layer: Option<&str>, volume_name: &str) -> MediaMatch<'_>`; `InstallRequest.media_folders: BTreeMap<String, PathBuf>`; `RefusalReason::LayersShareFolder { layers: Vec<String>, folder: String }`

- [ ] **Step 1: Write the failing tests**

In `scan.rs`'s `mod tests`, using the module's own fixture helpers —
`crate::core::osinstall::fixtures::{scratch, media}`, already imported at the
top of that `mod tests`. `scratch(tag)` returns a `PathBuf` and appends
`core::test_scratch_id()`, the process-wide counter; `media(dir, volume,
filename, entries)` writes a synthetic ADF whose `entries` are
`(path, bytes, protection)`.

```rust
#[test]
fn one_volume_name_in_two_layers_resolves_per_layer() {
    let dir = scratch("scan-layers");
    let base = dir.join("base");
    let update = dir.join("update");
    for folder in [&base, &update] {
        std::fs::create_dir_all(folder).unwrap();
    }
    media(&base, "DiskDoctor", "dd.adf", &[("C/DiskDoctor", b"base", 0)]);
    media(&update, "DiskDoctor", "dd.adf", &[("C/DiskDoctor", b"update", 0)]);

    let found = find_media_in_layers(&[
        ("base".to_string(), base.clone()),
        ("update".to_string(), update.clone()),
    ])
    .unwrap();

    let MediaMatch::Found(from_update) =
        media_for_layer(&found, Some("update"), "DiskDoctor")
    else {
        panic!("the update layer has exactly one DiskDoctor");
    };
    assert!(from_update.path.starts_with(&update));

    let MediaMatch::Found(from_base) = media_for_layer(&found, Some("base"), "DiskDoctor")
    else {
        panic!("the base layer has exactly one DiskDoctor");
    };
    assert!(from_base.path.starts_with(&base));
}

#[test]
fn two_of_one_name_inside_one_layer_are_still_ambiguous() {
    let dir = scratch("scan-layer-ambiguous");
    let base = dir.join("base");
    std::fs::create_dir_all(&base).unwrap();
    media(&base, "DiskDoctor", "a.adf", &[("C/DiskDoctor", b"one", 0)]);
    media(&base, "DiskDoctor", "b.adf", &[("C/DiskDoctor", b"two", 0)]);

    let found = find_media_in_layers(&[("base".to_string(), base)]).unwrap();
    let MediaMatch::Ambiguous(both) = media_for_layer(&found, Some("base"), "DiskDoctor")
    else {
        panic!("a layer holding two disks of one name is ambiguous, as it always was");
    };
    assert_eq!(both.len(), 2);
}

#[test]
fn a_byte_identical_disk_in_two_layers_survives_in_both() {
    // The defect this guards: `dedupe_identical_disks` run across layers
    // drops the update folder's copy and leaves that layer unable to
    // resolve a component that names it.
    let dir = scratch("scan-layer-dedupe");
    let base = dir.join("base");
    let update = dir.join("update");
    for folder in [&base, &update] {
        std::fs::create_dir_all(folder).unwrap();
    }
    let one = media(&base, "Workbench3.2", "wb.adf", &[("C/Assign", b"same", 0)]);
    let other = update.join("wb.adf");
    std::fs::copy(&one, &other).unwrap();   // byte for byte, on purpose

    let found = find_media_in_layers(&[
        ("base".to_string(), base),
        ("update".to_string(), update),
    ])
    .unwrap();

    assert!(
        matches!(media_for_layer(&found, Some("base"), "Workbench3.2"), MediaMatch::Found(_)),
        "the base layer keeps its copy"
    );
    assert!(
        matches!(media_for_layer(&found, Some("update"), "Workbench3.2"), MediaMatch::Found(_)),
        "and so does the update layer — same bytes in two roles are two answers"
    );
}

#[test]
fn a_byte_identical_disk_twice_inside_one_layer_is_still_one_disk() {
    let dir = scratch("scan-layer-dedupe-within");
    let base = dir.join("base");
    std::fs::create_dir_all(&base).unwrap();
    let one = media(&base, "Workbench3.2", "wb.adf", &[("C/Assign", b"same", 0)]);
    std::fs::copy(&one, base.join("wb-copy.adf")).unwrap();

    let found = find_media_in_layers(&[("base".to_string(), base)]).unwrap();
    assert!(matches!(
        media_for_layer(&found, Some("base"), "Workbench3.2"),
        MediaMatch::Found(_)
    ));
}
```

In `plan.rs`'s `mod tests`:

**This test builds its own two-layer recipe rather than reaching for
`AmigaOS 3.2.2`** — that file does not exist until Task 8, and the behaviour
under test is the layer mechanism, not the shipped recipe:

```rust
/// A minimal layered recipe: one component per layer, both naming a volume
/// the fixture writes into both folders.
fn two_layer_recipe() -> Recipe {
    recipe::parse(
        r#"{"release":"T","layers":[{"id":"base"},{"id":"up"}],
            "components":[
              {"id":"a","media":"DiskDoctor","layer":"base","required":true,
               "rules":[{"from":"C/DiskDoctor","to":"C/DiskDoctor","kind":"file"}]},
              {"id":"b","media":"DiskDoctor","layer":"up","required":true,
               "overrides":["a"],
               "rules":[{"from":"C/DiskDoctor","to":"C/DiskDoctor","kind":"file"}]}
            ]}"#,
    )
    .unwrap()
}

#[test]
fn two_layers_on_one_folder_refuse_by_naming_the_fields() {
    let dir = scratch("plan-same-folder");
    let one = dir.join("everything");
    std::fs::create_dir_all(&one).unwrap();
    media(&one, "DiskDoctor", "dd.adf", &[("C/DiskDoctor", b"x", 0)]);

    let request = InstallRequest {
        media_folders: BTreeMap::from([
            ("base".to_string(), one.clone()),
            ("up".to_string(), one.clone()),
        ]),
        ..request_for_scratch(&dir)
    };
    let plan = plan(&two_layer_recipe(), &request).unwrap();

    let same_folder = plan
        .refusals
        .iter()
        .find(|r| matches!(r, RefusalReason::LayersShareFolder { .. }))
        .expect("the refusal names the fields, not the disks");
    let RefusalReason::LayersShareFolder { layers, .. } = same_folder else {
        unreachable!()
    };
    assert_eq!(layers.len(), 2);
}
```

- [ ] **Step 2: Run and watch them fail**

Run: `cd src-tauri && cargo test osinstall::scan:: -- layer`
Expected: FAIL — `find_media_in_layers` does not exist.

- [ ] **Step 3: Carry the layer through the scan**

`FoundMedia` gains:

```rust
    /// Which [`super::MediaLayer`] this disk was found in. `None` for an
    /// unlayered scan, which is every caller that existed before layers did.
    pub layer: Option<String>,
```

`find_media` sets `layer: None`; a new function stamps it:

```rust
/// Every install disk in each named layer, as one list that remembers which
/// layer each disk came from.
///
/// **Deduplication is per layer, and that is the point rather than an
/// implementation detail.** Identity of content answers "is this one disk or
/// two?" *within* a layer — a user keeping a spare copy of `Workbench3.2`
/// beside the original has one disk. Across layers the same bytes in two
/// roles are two answers, and folding them would leave whichever layer lost
/// the coin toss unable to resolve a component that names the disk.
pub fn find_media_in_layers(layers: &[(String, PathBuf)]) -> CoreResult<Vec<FoundMedia>> {
    let mut all = Vec::new();
    for (layer, folder) in layers {
        let mut found = find_media(folder)?;
        for entry in &mut found {
            entry.layer = Some(layer.clone());
        }
        all.extend(dedupe_identical_disks(found));
    }
    Ok(all)
}

/// [`media_for`], asked inside one layer.
///
/// `layer: None` asks across everything, which is what an unlayered recipe
/// means and what every caller before layers was doing.
///
/// Written as one filter rather than as a wrapper that narrows a `Vec` first:
/// `MediaMatch<'a>` borrows from `found`, so a filtered copy would have to be
/// cloned and the borrows would not survive it.
pub fn media_for_layer<'a>(
    found: &'a [FoundMedia],
    layer: Option<&str>,
    volume_name: &str,
) -> MediaMatch<'a> {
    let matches: Vec<&FoundMedia> = found
        .iter()
        .filter(|f| layer.is_none() || f.layer.as_deref() == layer)
        .filter(|f| same_identity(&f.volume_name, volume_name))
        .collect();
    match matches.len() {
        0 => MediaMatch::Missing,
        1 => MediaMatch::Found(matches[0]),
        _ => MediaMatch::Ambiguous(matches),
    }
}
```

and today's `media_for(found, name)` becomes
`media_for_layer(found, None, name)`, so there is one comparison in this
module and not two that can drift.

- [ ] **Step 4: Wire the request**

`InstallRequest` gains, beside `extra_media_folders`:

```rust
    /// One media folder per layer the recipe declares, keyed by
    /// [`MediaLayer::id`].
    ///
    /// `media_folder` and `extra_media_folders` above stay for the reason
    /// they were given a `#[serde(default)]` in the first place: a request
    /// serialised before this field existed must still deserialise. When this
    /// map is empty they are read exactly as before, onto the single layer.
    #[serde(default)]
    pub media_folders: BTreeMap<String, PathBuf>,
```

**36 struct literals construct `InstallRequest` in tests** — `rg -n "extra_media_folders:" src-tauri/src` lists them (5 in `commands/osinstall.rs`, 9 in `apply.rs`, 1 in `mod.rs`, 20 in `plan.rs`, 1 in `scan_cache.rs`). Each gets one line added beneath: `media_folders: BTreeMap::new(),`. Mechanical; do it in this step so the tree compiles.

In `plan()`, replace the flat folder list:

```rust
    let layers: Vec<(String, PathBuf)> = if recipe.is_layered() {
        recipe
            .layers
            .iter()
            .filter_map(|l| request.media_folders.get(&l.id).map(|f| (l.id.clone(), f.clone())))
            .collect()
    } else {
        let mut folders = vec![request.media_folder.clone()];
        folders.extend(request.extra_media_folders.iter().cloned());
        folders.into_iter().map(|f| (String::new(), f)).collect()
    };
    refusals.extend(layers_sharing_a_folder(&layers));
    let found = if recipe.is_layered() {
        find_media_in_layers(&layers)?
    } else {
        find_media_across(&layers.iter().map(|(_, f)| f.clone()).collect::<Vec<_>>())?
    };
```

and the per-component lookup becomes
`media_for_layer(&found, component.layer.as_deref(), &component.media)`.

`layers_sharing_a_folder` compares canonical paths and yields one
`RefusalReason::LayersShareFolder` per colliding group.

- [ ] **Step 5: Add the refusal and its sentence**

`RefusalReason::LayersShareFolder { layers: Vec<String>, folder: String }` in `mod.rs`, rendered in the same place its siblings are, plus its i18n key in **both** catalogues.

- [ ] **Step 6: Update the command adapter and the TS type**

`commands/osinstall.rs:272`'s `once(&request.media_folder).chain(...)` iterates `request.media_folders.values()` when it is non-empty and the old pair otherwise. `src/lib/osinstall.ts` gains `mediaFolders?: Record<string, string>;`.

- [ ] **Step 7: Run everything**

Run: `cd src-tauri && cargo test osinstall::` then `pnpm lint`
Expected: PASS.

- [ ] **Step 8: Mutations**

| Mutation | Test that must fail |
|---|---|
| dedupe across layers instead of within | `a_byte_identical_disk_in_two_layers_survives_in_both` |
| stop deduping altogether | `a_byte_identical_disk_twice_inside_one_layer_is_still_one_disk` |
| ignore `layer` in `media_for_layer` | `one_volume_name_in_two_layers_resolves_per_layer` |
| drop `layers_sharing_a_folder` | `two_layers_on_one_folder_refuse_by_naming_the_fields` |
| collapse `Ambiguous` into `Missing` inside a layer | `two_of_one_name_inside_one_layer_are_still_ambiguous` |

- [ ] **Step 9: Commit**

Message: `feat(osinstall): resolve a component's media inside its own layer`

---

## Task 4: A component may remove a path

**Files:**
- Modify: `src-tauri/src/core/osinstall/mod.rs` (`Component.removes`), `recipe.rs` (`COMPONENT_KEYS`, `validate_component`)
- Modify: `src-tauri/src/core/osinstall/plan.rs` (a `removals` list on `InstallPlan`), `apply.rs` (perform them, report per entry)
- Modify: `src/i18n/{en,tr}.json`
- Test: inline in `recipe.rs`, `plan.rs`, `apply.rs`

**Interfaces:**
- Consumes: Task 1's validation shape
- Produces: `Component.removes: Vec<String>`; `InstallPlan.removals: Vec<PlanRemoval { component: String, to: String }>`; `ApplyReport.removed: Vec<RemovalVerdict { to: String, state: RemovalState }>` with `RemovalState::{Removed, NotPresent, Failed(String)}`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_component_removes_a_path_an_overridden_component_placed() {
    let dir = scratch("apply-removes");
    let tree = dir.join("tree");
    let report = apply_with(
        &recipe_with_removal(),   // base places Tools/X, update removes it
        &request_for_scratch(&dir, &tree),
    )
    .unwrap();

    assert!(!tree.join("Tools/X").exists(), "the removal actually removed it");
    let verdict = report
        .removed
        .iter()
        .find(|r| r.to == "Tools/X")
        .expect("the removal is reported by name");
    assert!(matches!(verdict.state, RemovalState::Removed));
}

#[test]
fn a_removal_of_something_that_is_not_there_is_an_outcome_not_a_failure() {
    // The base component is off, so nothing placed Tools/X. That is a
    // legitimate build, not a failed one.
    let report = apply_with(&recipe_with_removal(), &request_without_the_base_component());
    let report = report.unwrap();
    let verdict = report.removed.iter().find(|r| r.to == "Tools/X").unwrap();
    assert!(
        matches!(verdict.state, RemovalState::NotPresent),
        "not present is its own verdict, distinct from Removed and from Failed"
    );
}

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
```

- [ ] **Step 2: Run and watch them fail**

Run: `cd src-tauri && cargo test osinstall:: -- remov`
Expected: FAIL — no `removes` field.

- [ ] **Step 3: Add the field, the validation and the plan items**

`Component`:

```rust
    /// Destinations this component **deletes from the distribution tree**.
    ///
    /// AmigaOS 3.2.2's Installer deletes `Tools/TextEditFileTypes/Default4Types`
    /// as unsupported, and that file reaches an ART tree from `Extras3.2`. A
    /// `from`/`to` rule cannot say it.
    ///
    /// **Only inside the tree ART is building.** This never names a path on
    /// the user's own disks; `core/hostfs`'s recycler is a different thing for
    /// a different threat and this does not become it.
    #[serde(default)]
    pub removes: Vec<String>,
```

`validate_component` checks each entry with the same path rules `to` gets
(`allow_empty: false`), and `validate` checks the override relationship:
a removal must name a destination some component in the recipe places, and
that component's id must appear in this component's `overrides`.

`plan()` collects `InstallPlan.removals` from the components that are on,
after the item loop. `apply()` performs them **after every placement**, in the
merged recipe's component order, and records one verdict per entry through the
existing oplog path.

- [ ] **Step 4: Run the tests**

Run: `cd src-tauri && cargo test osinstall:: -- remov`
Expected: PASS.

- [ ] **Step 5: Mutations**

| Mutation | Test that must fail |
|---|---|
| perform removals *before* placement | `a_component_removes_a_path_an_overridden_component_placed` (the base places it back) |
| report a missing path as `Failed` | `a_removal_of_something_that_is_not_there_is_an_outcome_not_a_failure` |
| drop the override-relationship check | `a_removal_may_only_name_a_path_an_overridden_component_places` |
| make the removal silent (no verdict) | both apply tests |

- [ ] **Step 6: Commit**

Message: `feat(osinstall): a component may remove a path an overridden one placed`

---

## Task 5: `core/amigaicon` — read an `.info`, merge tooltypes and stack

**Files:**
- Create: `src-tauri/src/core/amigaicon/mod.rs`
- Modify: `src-tauri/src/core/mod.rs` (declare the module)
- Test: inline `#[cfg(test)]` in `amigaicon/mod.rs`

**Interfaces:**
- Consumes: nothing
- Produces: `IconLayout { tooltypes: Option<Range<usize>>, trailing: Range<usize> }`; `layout(&[u8]) -> CoreResult<IconLayout>`; `tooltypes(&[u8]) -> CoreResult<Vec<String>>`; `stack_size(&[u8]) -> CoreResult<u32>`; `merge_tooltypes(dest: &[u8], source: &[u8]) -> CoreResult<Vec<u8>>`

**The format, measured rather than recalled** (research note §8): `do_Magic`
`0xE310` at 0, a 78-byte `DiskObject`, then the optional blocks in this order —
`DrawerData` (56 bytes, when `do_DrawerData` at 66 is non-zero), `GadgetRender`
(when the gadget's field at 22 is non-zero), `SelectRender` (at 26),
`DefaultTool` (a `u32` length then that many bytes, at 50), `ToolTypes` (a
`u32` **size** which is `(count + 1) * 4`, then `count` length-prefixed
strings, at 54), `ToolWindow` (at 70). An `Image` is 20 bytes plus
`((width + 15) / 16) * 2 * height * depth`. `do_StackSize` is the `u32` at 74.
Anything after the last block is the appended ColorIcon/NewIcon data.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_hand_built_icon_parses_to_its_own_length() {
    let icon = synthetic_icon(&["A=1", "B=2"], 4096, /* trailing */ b"");
    let l = layout(&icon).unwrap();
    assert_eq!(l.trailing.start, icon.len(), "nothing is left over");
    assert_eq!(tooltypes(&icon).unwrap(), vec!["A=1".to_string(), "B=2".to_string()]);
    assert_eq!(stack_size(&icon).unwrap(), 4096);
}

#[test]
fn merging_carries_the_trailing_block_through_byte_for_byte() {
    let trailing = b"FORM....ICONFACE....pretend colour icon";
    let dest = synthetic_icon(&["A=1"], 4096, trailing);
    let source = synthetic_icon(&["A=1", "(PUBSCREEN=<name>)"], 8192, b"");

    let merged = merge_tooltypes(&dest, &source).unwrap();

    let l = layout(&merged).unwrap();
    assert_eq!(&merged[l.trailing.clone()], trailing, "the ColorIcon survives");
    assert_eq!(tooltypes(&merged).unwrap(), tooltypes(&source).unwrap());
    assert_eq!(stack_size(&merged).unwrap(), 8192, "the stack comes from the source");
}

#[test]
fn merging_an_icon_with_itself_returns_it_unchanged() {
    let icon = synthetic_icon(&["A=1", "B=2"], 4096, b"FORM....trailing");
    assert_eq!(merge_tooltypes(&icon, &icon).unwrap(), icon);
}

#[test]
fn a_length_that_runs_past_the_buffer_is_refused_not_read() {
    let mut icon = synthetic_icon(&["A=1"], 4096, b"");
    // Overwrite the first tooltype's length with something enormous.
    let l = layout(&icon).unwrap();
    let at = l.tooltypes.clone().unwrap().start + 4;
    icon[at..at + 4].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(tooltypes(&icon).is_err(), "a lying length is a refusal, never a read");
}

#[test]
fn an_image_whose_dimensions_multiply_past_the_buffer_is_refused() {
    // A 65535 x 65535 x 8 image claims about 34 GB of plane data behind a
    // 200-byte file. Refused on the arithmetic, before anything is indexed.
    let icon = synthetic_icon_with_image(0xFFFF, 0xFFFF, 8);
    assert!(layout(&icon).is_err());
}

#[test]
fn something_that_is_not_an_icon_is_refused_by_magic() {
    assert!(layout(b"not an icon at all").is_err());
    assert!(layout(&[]).is_err());
}
```

`synthetic_icon` and `synthetic_icon_with_image` are test helpers in the same
module that build a `DiskObject` by hand. Generated at runtime; nothing
copyrighted is shipped.

- [ ] **Step 2: Run and watch them fail**

Run: `cd src-tauri && cargo test amigaicon::`
Expected: FAIL — the module does not exist.

- [ ] **Step 3: Write the module**

Every read goes through helpers that bound-check first:

```rust
fn be_u32(bytes: &[u8], at: usize) -> CoreResult<u32> {
    let end = at.checked_add(4).ok_or_else(|| malformed("offset overflow"))?;
    let slice = bytes.get(at..end).ok_or_else(|| malformed("truncated icon"))?;
    Ok(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}
```

and the image size is computed the same way:

```rust
let words = (width as usize).checked_add(15).ok_or(..)? / 16;
let plane = words.checked_mul(2).and_then(|w| w.checked_mul(height as usize)).ok_or(..)?;
let total = plane.checked_mul(depth as usize).and_then(|t| t.checked_add(20)).ok_or(..)?;
```

`merge_tooltypes` splices: everything before the destination's tooltype range,
then the source's tooltype block verbatim, then everything after — with
`do_StackSize` (offset 74) taken from the source. **The trailing region is
copied and never interpreted;** ART does not need to understand a ColorIcon to
preserve one.

- [ ] **Step 4: Run the tests**

Run: `cd src-tauri && cargo test amigaicon::`
Expected: PASS.

- [ ] **Step 5: Mutations**

| Mutation | Test that must fail |
|---|---|
| truncate at the tooltype array's end instead of splicing | `merging_carries_the_trailing_block_through_byte_for_byte` |
| keep the destination's stack | that same test's last assertion |
| read a length without bounding it | `a_length_that_runs_past_the_buffer_is_refused_not_read` |
| multiply the image dimensions without `checked_mul` | `an_image_whose_dimensions_multiply_past_the_buffer_is_refused` |
| skip the magic check | `something_that_is_not_an_icon_is_refused_by_magic` |

- [ ] **Step 6: Commit**

Message: `feat(core): read an Amiga .info and merge its tooltypes and stack`

---

## Task 6: The `icon-tooltypes` rule kind

**Files:**
- Modify: `src-tauri/src/core/osinstall/mod.rs` (`RuleKind`), `plan.rs` (the match at ~905-985), `apply.rs`
- Modify: `src/i18n/{en,tr}.json`
- Test: inline in `plan.rs` and `apply.rs`

**Interfaces:**
- Consumes: Task 5's `merge_tooltypes`
- Produces: `RuleKind::IconTooltypes` (`"icon-tooltypes"` in JSON); `PlanItem.merge_icon: bool`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn an_icon_rule_merges_into_the_icon_already_in_the_tree() {
    let dir = scratch("apply-icon-rule");
    let tree = dir.join("tree");
    // The base component places an icon with a 4096 stack and a trailing
    // block; the update's media carries one with 8192 and one more tooltype.
    apply_with(&recipe_with_icon_rule(), &request_for_scratch(&dir, &tree)).unwrap();

    let merged = std::fs::read(tree.join("Tools/IconEdit.info")).unwrap();
    assert_eq!(crate::core::amigaicon::stack_size(&merged).unwrap(), 8192);
    let l = crate::core::amigaicon::layout(&merged).unwrap();
    assert_eq!(&merged[l.trailing], TRAILING, "the tree's own ColorIcon survived");
}

#[test]
fn an_icon_rule_whose_destination_is_absent_is_skipped_and_says_so() {
    let report = apply_with(
        &recipe_with_icon_rule(),
        &request_without_the_component_that_places_the_icon(),
    )
    .unwrap();
    let verdict = report
        .icons
        .iter()
        .find(|v| v.to == "Tools/IconEdit.info")
        .expect("the skip is reported by name");
    assert!(
        matches!(verdict.state, IconMergeState::DestinationAbsent),
        "the release's own `if exists` guard — skipped, never failed"
    );
    assert_eq!(report.failed, 0);
}
```

- [ ] **Step 2: Run and watch them fail**

Run: `cd src-tauri && cargo test osinstall:: -- icon`
Expected: FAIL — `RuleKind` has two variants.

- [ ] **Step 3: Add the variant and wire it**

`RuleKind` gains `IconTooltypes` (kebab-case `icon-tooltypes`). In `plan`'s
match, an `IconTooltypes` rule against a media **file** produces a `PlanItem`
with `merge_icon: true`; against a directory it is a `RuleKindMismatch`, like
its siblings. In `apply`, such an item reads the destination from the tree,
returns `DestinationAbsent` when it is not there, and otherwise writes
`merge_tooltypes(dest, source)` through `core/safety`'s `atomic_write`.

The rule participates in the destination-collision check exactly like a
`File` rule, so the component declares what it amends through `overrides`.

- [ ] **Step 4: Run the tests**

Run: `cd src-tauri && cargo test osinstall::`
Expected: PASS.

- [ ] **Step 5: Mutations**

| Mutation | Test that must fail |
|---|---|
| turn `DestinationAbsent` into a failure | `an_icon_rule_whose_destination_is_absent_is_skipped_and_says_so` |
| write the source icon over the destination instead of merging | `an_icon_rule_merges_into_the_icon_already_in_the_tree` (the trailing block) |
| exempt the kind from the collision check | the recipe test in Task 8 must report an undeclared collision |

- [ ] **Step 6: Commit**

Message: `feat(osinstall): an icon-tooltypes rule amends an icon already in the tree`

---

## Task 7: Read a ROM's resident table, and condition on it

**Files:**
- Modify: `src-tauri/src/core/rom/mod.rs` (a resident reader)
- Modify: `src-tauri/src/core/osinstall/mod.rs:101-135` (`Condition`), `recipe.rs:48` (`CONDITION_KEYS`)
- Modify: `src-tauri/src/core/osinstall/plan.rs` (`condition_holds`, the ROM facts it reads)
- Test: inline in `rom/mod.rs` and `plan.rs`

**Interfaces:**
- Consumes: nothing from earlier tasks
- Produces: `rom::RomResident { name: String, version: u8, id: String }`;
  `rom::residents(&[u8]) -> CoreResult<Vec<RomResident>>`;
  `rom::resident_version(&[u8], name: &str) -> Option<(u16, u16)>`;
  `Condition::ResidentOlderThan { resident: String, major: u16, minor: Option<u16> }`

**Why this is not the `minor` field an earlier draft of this plan called for.**
The design's §5 records the measurement that killed that one: the release's
Modules test asks a running machine for `exec.library`'s revision and for
`strap`'s version, and the ROM **header** tracks neither. Read out of the three
A1200 Kickstarts the owner holds — 47.96 carries `exec 47.7` and `strap 45.1`;
47.102 carries `exec 47.8` and `strap 47.2`; 47.111 carries `exec 47.10` and
`strap 47.2` — a header proxy collapses two different outcomes into one, and
would place `Shell-Seg` and three library modules onto a 47.102 machine that
the release deliberately withholds them from.

**The format.** A `Resident` is 26 bytes: `rt_MatchWord` (`0x4AFC`),
`rt_MatchTag` (a pointer **to the struct itself** — this is what makes the scan
reliable), `rt_EndSkip`, `rt_Flags`, `rt_Version`, `rt_Type`, `rt_Pri`,
`rt_Name`, `rt_IdString`, `rt_Init`. A 512 KiB image maps at `0xF80000`, a
256 KiB one at `0xFC0000`. The revision lives only in the ID string
(`exec 47.10 (21.01.2023)`); `rt_Version` carries the major alone.

- [ ] **Step 1: Write the failing tests**

In `core/rom/mod.rs`'s `mod tests`:

```rust
/// A 512 KiB image with one hand-built `Resident` at a known offset.
fn rom_with_resident(offset: usize, name: &str, version: u8, id: &str) -> Vec<u8> {
    const BASE: u32 = 0xF8_0000;
    let mut rom = vec![0u8; 512 * 1024];
    let name_at = offset + 64;
    let id_at = offset + 128;
    rom[offset..offset + 2].copy_from_slice(&0x4AFCu16.to_be_bytes());
    rom[offset + 2..offset + 6].copy_from_slice(&(BASE + offset as u32).to_be_bytes());
    rom[offset + 11] = version;
    rom[offset + 14..offset + 18].copy_from_slice(&(BASE + name_at as u32).to_be_bytes());
    rom[offset + 18..offset + 22].copy_from_slice(&(BASE + id_at as u32).to_be_bytes());
    rom[name_at..name_at + name.len()].copy_from_slice(name.as_bytes());
    rom[id_at..id_at + id.len()].copy_from_slice(id.as_bytes());
    rom
}

#[test]
fn a_resident_is_found_by_its_own_self_pointer() {
    let rom = rom_with_resident(0x400, "exec.library", 47, "exec 47.10 (21.01.2023)");
    let found = residents(&rom).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "exec.library");
    assert_eq!(found[0].version, 47);
    assert_eq!(found[0].id, "exec 47.10 (21.01.2023)");
}

#[test]
fn a_match_word_whose_tag_points_elsewhere_is_not_a_resident() {
    // 0x4AFC is the m68k ILLEGAL instruction and occurs in ordinary code.
    // Only the self-pointer separates a real Resident from a coincidence.
    let mut rom = rom_with_resident(0x400, "exec.library", 47, "exec 47.10 (x)");
    rom[0x800..0x802].copy_from_slice(&0x4AFCu16.to_be_bytes());
    rom[0x802..0x806].copy_from_slice(&0xF8_0000u32.to_be_bytes()); // points at 0, not itself
    assert_eq!(residents(&rom).unwrap().len(), 1);
}

#[test]
fn resident_version_reads_the_revision_out_of_the_id_string() {
    let rom = rom_with_resident(0x400, "exec.library", 47, "exec 47.10 (21.01.2023)");
    assert_eq!(resident_version(&rom, "exec"), Some((47, 10)));
    assert_eq!(resident_version(&rom, "strap"), None);
}

#[test]
fn a_name_pointer_outside_the_image_is_refused_not_read() {
    let mut rom = rom_with_resident(0x400, "exec.library", 47, "exec 47.10 (x)");
    rom[0x400 + 14..0x400 + 18].copy_from_slice(&0xFFFF_FFFEu32.to_be_bytes());
    assert!(residents(&rom).is_err(), "a pointer outside the image is a refusal");
}

#[test]
fn an_image_of_an_unexpected_size_has_no_base_and_is_refused() {
    assert!(residents(&vec![0u8; 1234]).is_err());
}
```

In `plan.rs`'s `mod tests`, against the three real Kickstarts as **numbers** —
no ROM file is shipped, needed, or read:

```rust
#[test]
fn the_modules_condition_answers_what_the_release_answers_for_all_three_roms() {
    let exec_older = Condition::ResidentOlderThan {
        resident: "exec".into(), major: 47, minor: Some(10),
    };
    let strap_older = Condition::ResidentOlderThan {
        resident: "strap".into(), major: 47, minor: None,
    };
    // (exec, strap), measured out of the owner's own A1200 Kickstarts.
    let kick_32  = residents_of((47, 7),  (45, 1));
    let kick_321 = residents_of((47, 8),  (47, 2));
    let kick_322 = residents_of((47, 10), (47, 2));

    assert!(resident_condition_holds(&exec_older, &kick_32));
    assert!(
        resident_condition_holds(&strap_older, &kick_32),
        "3.2's ROM gets the larger file set"
    );

    assert!(resident_condition_holds(&exec_older, &kick_321));
    assert!(
        !resident_condition_holds(&strap_older, &kick_321),
        "3.2.1's ROM gets the smaller set - Shell-Seg and the three libraries are \
         withheld, which is exactly what the header proxy got wrong"
    );

    assert!(!resident_condition_holds(&exec_older, &kick_322));
    assert!(
        !resident_condition_holds(&strap_older, &kick_322),
        "3.2.2's own ROM needs no softkicked modules at all"
    );
}

#[test]
fn a_condition_naming_a_resident_the_rom_does_not_carry_does_not_hold() {
    let c = Condition::ResidentOlderThan {
        resident: "nosuchthing".into(), major: 47, minor: None,
    };
    assert!(
        !resident_condition_holds(&c, &residents_of((47, 7), (45, 1))),
        "an absent resident switches nothing on - never a default of `older`"
    );
}
```

- [ ] **Step 2: Run and watch them fail**

Run: `cd src-tauri && cargo test rom:: -- resident` then
`cargo test osinstall::plan:: -- modules_condition`
Expected: FAIL — `residents` does not exist.

- [ ] **Step 3: Write the resident reader**

In `core/rom/mod.rs`. Every pointer becomes an offset through one bounded
helper, and a pointer outside the image is a `CoreError`, never a clamp:

```rust
const RESIDENT_MATCH_WORD: u16 = 0x4AFC;
const RESIDENT_SIZE: usize = 26;

/// Where a Kickstart image of this size maps in the Amiga's address space.
fn rom_base(len: usize) -> Option<u32> {
    match len {
        0x8_0000 => Some(0xF8_0000), // 512 KiB
        0x4_0000 => Some(0xFC_0000), // 256 KiB
        _ => None,
    }
}

fn string_at(bytes: &[u8], base: u32, pointer: u32) -> CoreResult<String> {
    let offset = pointer
        .checked_sub(base)
        .ok_or_else(|| malformed("a resident points below the ROM base"))?
        as usize;
    let tail = bytes
        .get(offset..)
        .ok_or_else(|| malformed("a resident points past the end of the ROM"))?;
    let end = tail.iter().position(|b| *b == 0).unwrap_or(tail.len());
    Ok(String::from_utf8_lossy(&tail[..end]).into_owned())
}
```

`residents` walks two bytes at a time and accepts a candidate **only** when
`rt_MatchTag == base + offset`.

`resident_version(bytes, "exec")` finds the resident whose ID string's first
word is `name`, then parses `major.minor` out of the second word. A malformed
ID string yields `None` rather than a partial number.

- [ ] **Step 4: Add the condition**

```rust
    /// The named resident **inside the paired Kickstart** is older than this.
    ///
    /// `exec` and `strap` are the two AmigaOS 3.2.2's Modules step asks
    /// about. Deliberately distinct from [`Condition::RomOlderThan`], which
    /// asks the ROM's own stated version - a different number that tracks
    /// neither of these, measured in the design's section 5.
    ResidentOlderThan {
        resident: String,
        major: u16,
        #[serde(default)]
        minor: Option<u16>,
    },
```

Add `"resident"` and `"minor"` to `CONDITION_KEYS`. `plan`'s ROM facts gain the
residents read from the paired Kickstart, and the comparison is lexicographic
on `(major, minor)` — the major alone when `minor` is `None`. **A resident the
ROM does not carry never satisfies the condition**: absent is not "older".

- [ ] **Step 5: Run the tests**

Run: `cd src-tauri && cargo test rom:: && cargo test osinstall::`
Expected: PASS — and the 3.2 recipe's `modules-a1200`, still on
`rom-older-than major 47`, behaves exactly as it did.

- [ ] **Step 6: Add the real-ROM hook**

`#[ignore]`d, env-gated, read-only against the owner's own Kickstarts. The
three numbers above are a measurement, and this is what keeps them one:

```rust
#[test]
#[ignore = "needs the owner's own 3.2-family Kickstarts"]
fn read_the_real_roms_residents_when_asked() {
    let Ok(path) = std::env::var("ART_ROM") else { return };
    let bytes = std::fs::read(path).unwrap();
    println!(
        "ART_ROM_RESULT header={:?} exec={:?} strap={:?}",
        stated_version(&bytes),
        resident_version(&bytes, "exec"),
        resident_version(&bytes, "strap"),
    );
    assert!(resident_version(&bytes, "exec").is_some());
}
```

Run it against all three and check the printed numbers against the design's §5
table. A mismatch is a finding, not a reason to adjust the table.

- [ ] **Step 7: Mutations**

| Mutation | Test that must fail |
|---|---|
| accept a `0x4AFC` without checking the self-pointer | `a_match_word_whose_tag_points_elsewhere_is_not_a_resident` |
| clamp an out-of-range pointer instead of refusing | `a_name_pointer_outside_the_image_is_refused_not_read` |
| take the revision from `rt_Version` instead of the ID string | `resident_version_reads_the_revision_out_of_the_id_string` |
| compare the major only | the 3.2.1 rows of `the_modules_condition_answers_what_the_release_answers_for_all_three_roms` |
| treat an absent resident as older | `a_condition_naming_a_resident_the_rom_does_not_carry_does_not_hold` |
| default `rom_base` to `0xF80000` whatever the size | `an_image_of_an_unexpected_size_has_no_base_and_is_refused` |

- [ ] **Step 8: Commit**

Message: `feat(rom): read a Kickstart's resident table, and condition on it`

---

## Task 8: The AmigaOS 3.2.2 recipe

**Files:**
- Create: `src-tauri/src/core/osinstall/recipes/amigaos-3.2.2.json`
- Modify: `src-tauri/src/core/osinstall/recipe.rs` (`include_str!`, `amigaos_322`, `releases`, `by_release`, `by_release_unresolved`)
- Modify: `src-tauri/src/core/osinstall/recipes/amigaos-3.2.json` (drop `update-3.2.1`)
- Modify: `src-tauri/src/core/osinstall/recipe.rs:1238` and `src/lib/osinstall.test.ts:258-266` (their unavailable-component example)
- Modify: `src/i18n/{en,tr}.json` (layer labels, component labels)
- Test: inline in `recipe.rs`

**Interfaces:**
- Consumes: Tasks 1, 2, 4, 6, 7
- Produces: `recipe::amigaos_322() -> CoreResult<Recipe>`; `"AmigaOS 3.2.2"` in `releases()`

**Every path below is from the research note's census; do not retype them from
memory.** The counts to expect: `update-322-system` 39 + 2, `update-322-classes`
31, `update-322-diskdoctor` 3, `update-322-locale-tr` 39.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn the_322_recipe_resolves_and_declares_two_layers() {
    let r = by_release("AmigaOS 3.2.2").unwrap();
    assert_eq!(r.layer_ids(), vec!["base", "update-3.2.2"]);
    assert_eq!(r.base.as_deref(), Some("AmigaOS 3.2"));
}

#[test]
fn the_two_diskdoctors_are_two_components_in_two_layers() {
    let r = by_release("AmigaOS 3.2.2").unwrap();
    assert_eq!(r.component("diskdoctor").unwrap().layer.as_deref(), Some("base"));
    let update = r.component("update-322-diskdoctor").unwrap();
    assert_eq!(update.layer.as_deref(), Some("update-3.2.2"));
    assert_eq!(update.media, "DiskDoctor");
    assert!(update.overrides.iter().any(|o| o == "diskdoctor"));
}

#[test]
fn every_locale_component_names_only_drawers_its_own_disk_carries() {
    // -EN carries Help alone; only -CZ, -RS and -RU carry Languages.
    let r = by_release("AmigaOS 3.2.2").unwrap();
    let en = r.component("update-322-locale-en").unwrap();
    assert!(en.rules.iter().all(|rule| rule.from.starts_with("Help")));
    for id in ["update-322-locale-cz", "update-322-locale-rs", "update-322-locale-ru"] {
        assert!(
            r.component(id).unwrap().rules.iter().any(|rule| rule.from == "Languages"),
            "{id} carries Languages"
        );
    }
    for id in ["update-322-locale-tr", "update-322-locale-de"] {
        assert!(
            !r.component(id).unwrap().rules.iter().any(|rule| rule.from == "Languages"),
            "{id} does not, and a rule for it would refuse MediaMissing"
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
        system.rules.iter().filter(|r| r.from.starts_with("C/")).count(),
        10
    );
}

#[test]
fn the_update_removes_the_file_the_release_removes() {
    let r = by_release("AmigaOS 3.2.2").unwrap();
    let system = r.component("update-322-system").unwrap();
    assert!(system.removes.iter().any(|p| p == "Tools/TextEditFileTypes/Default4Types"));
    assert!(system.overrides.iter().any(|o| o == "extras"));
}

#[test]
fn the_empty_3_2_1_placeholder_is_gone() {
    assert!(amigaos_32().unwrap().component("update-3.2.1").is_none());
}
```

Then remove the `#[ignore]` from Task 2's two 3.2.2 tests.

- [ ] **Step 2: Run and watch them fail**

Run: `cd src-tauri && cargo test osinstall::recipe:: -- 322`
Expected: FAIL — `ART ships no install recipe for AmigaOS 3.2.2`.

- [ ] **Step 3: Write the recipe file**

Skeleton; fill every rule from the census:

```jsonc
{
  "release": "AmigaOS 3.2.2",
  "base": "AmigaOS 3.2",
  "layers": [
    { "id": "base", "label_key": "osinstall.layer.base32" },
    { "id": "update-3.2.2", "label_key": "osinstall.layer.update322" }
  ],
  "components": [
    {
      "id": "update-322-system",
      "media": "Update3.2.2",
      "layer": "update-3.2.2",
      "required": true,
      "label_key": "osinstall.component.update322System",
      "overrides": ["workbench-base", "install-libs", "extras", "storage", "glowicons", "locale-base", "classes"],
      "removes": ["Tools/TextEditFileTypes/Default4Types"],
      "rules": [
        { "from": "C/AssignWedge.Z", "to": "C/AssignWedge", "kind": "file" },
        { "from": "C/ConClip.Z",     "to": "C/ConClip",     "kind": "file" },
        { "from": "C/DefIcons.Z",    "to": "C/DefIcons",    "kind": "file" },
        { "from": "C/Eval.Z",        "to": "C/Eval",        "kind": "file" },
        { "from": "C/Execute.Z",     "to": "C/Execute",     "kind": "file" },
        { "from": "C/IPrefs.Z",      "to": "C/IPrefs",      "kind": "file" },
        { "from": "C/MD5Sum.Z",      "to": "C/MD5Sum",      "kind": "file" },
        { "from": "C/SetPatch.Z",    "to": "C/SetPatch",    "kind": "file" },
        { "from": "C/Type.Z",        "to": "C/Type",        "kind": "file" },
        { "from": "C/WBInfo.Z",      "to": "C/WBInfo",      "kind": "file" },
        { "from": "DEVS",            "to": "Devs",             "kind": "subtree" },
        { "from": "L",               "to": "L",                "kind": "subtree" },
        { "from": "LIBS",            "to": "Libs",             "kind": "subtree" },
        { "from": "Locale/Countries","to": "Locale/Countries", "kind": "subtree" },
        { "from": "Prefs",           "to": "Prefs",            "kind": "subtree" },
        { "from": "System",          "to": "System",           "kind": "subtree" },
        { "from": "Tools",           "to": "Tools",            "kind": "subtree" },
        { "from": "Utilities",       "to": "Utilities",        "kind": "subtree" },
        { "from": "WBStartup",       "to": "WBStartup",        "kind": "subtree" },
        { "from": "Update/Release",  "to": "Prefs/Env-Archive/Versions/Release", "kind": "file" },
        { "from": "Update/Startup-HardDrive", "to": "S/Startup-Sequence", "kind": "file" },
        { "from": "Update/IconEdit.info", "to": "Tools/IconEdit.info", "kind": "icon-tooltypes" }
      ]
    }
    // … update-322-classes, update-322-diskdoctor, the seventeen locales,
    //   update-322-modules-a1200, update-322-modules-a1200-strap
  ]
}
```

**The two Modules components**, and the design's §5 is why there are two and
not one — the release's inner branch picks a different file set for a 3.2 ROM
than for a 3.2.1 one, which a single component cannot say:

```jsonc
{
  "id": "update-322-modules-a1200",
  "media": "ModulesA1200_3.2.2",
  "layer": "update-3.2.2",
  "exclusive_group": "modules",
  "condition": { "condition": "resident-older-than", "resident": "exec", "major": 47, "minor": 10 },
  "overrides": ["storage", "workbench-base"],
  "rules": [
    { "from": "LIBS/A1200",            "to": "Libs/A1200",            "kind": "subtree" },
    { "from": "LIBS/intuition.library","to": "Libs/intuition.library","kind": "file" },
    { "from": "L/Ram-Handler",         "to": "L/Ram-Handler",         "kind": "file" },
    { "from": "L/System-startup",      "to": "L/System-startup",      "kind": "file" }
  ]
},
{
  "id": "update-322-modules-a1200-strap",
  "media": "ModulesA1200_3.2.2",
  "layer": "update-3.2.2",
  "condition": { "condition": "resident-older-than", "resident": "strap", "major": 47 },
  "overrides": ["update-322-modules-a1200", "workbench-base"],
  "rules": [
    { "from": "L/Shell-Seg",           "to": "L/Shell-Seg",           "kind": "file" },
    { "from": "L/Ram-Handler",         "to": "L/Ram-Handler",         "kind": "file" },
    { "from": "L/System-startup",      "to": "L/System-startup",      "kind": "file" },
    { "from": "LIBS/dos.library",      "to": "Libs/dos.library",      "kind": "file" },
    { "from": "LIBS/gadtools.library", "to": "Libs/gadtools.library", "kind": "file" },
    { "from": "LIBS/graphics.library", "to": "Libs/graphics.library", "kind": "file" }
  ]
}
```

**Those rules are measured, not inferred.** `xdftool ModulesA1200_3.2.2.adf
list` was run: the disk carries exactly one machine drawer,
`LIBS/A1200/exec.library`, which is what the release's `(A500|A600|…|CD32)`
pattern over `LIBS` matches — so a single `subtree LIBS/A1200` says the same
thing without a wildcard the recipe format does not have. `LIBS/intuition.library`,
`L/{Ram-Handler,Shell-Seg,System-startup}` and
`LIBS/{dos,gadtools,graphics}.library` are all present.

**Two things on that disk deliberately stay behind.** `DEVS/A1200/scsi.device`
is there and the 3.2.2 script never copies `DEVS` — only the base 3.2 recipe's
own `modules-a1200` does, from a different disk under a different installer.
`L/FastFileSystem`, `LIBS/Modules/` and `LIBS/Resources/` are likewise on the
disk and untouched by this update. A rule for any of them would be ART
installing something the release does not.

- [ ] **Step 4: Register it**

```rust
const AMIGAOS_322_JSON: &str = include_str!("recipes/amigaos-3.2.2.json");

pub fn amigaos_322() -> CoreResult<Recipe> {
    parse(AMIGAOS_322_JSON)
}

pub fn releases() -> &'static [&'static str] {
    &["AmigaOS 3.2", "AmigaOS 3.2.2", "AmigaOS 3.9"]
}
```

and both `by_release` and `by_release_unresolved` gain the arm.

- [ ] **Step 5: Let the collision test name the `overrides`**

Run: `cd src-tauri && cargo test osinstall::recipe:: -- collision`

The existing "no two components claim one destination without an `overrides`
relationship" test **reports every collision by name**. Add exactly the ids it
reports to each update component's `overrides` — do not guess them and do not
add ids it did not report. Re-run until it passes.

- [ ] **Step 6: Move the two unavailable-component examples**

`recipe.rs:1238` and `src/lib/osinstall.test.ts:258-266` use `update-3.2.1` as
their example. Both construct their own unavailable component instead — the
Rust one from a JSON literal, the TS one from a fixture object.

- [ ] **Step 7: Add the i18n keys to both catalogues**

`osinstall.layer.base32`, `osinstall.layer.update322`, and one
`osinstall.component.*` per new component, in **both** `en.json` and `tr.json`.

- [ ] **Step 8: Run everything**

Run: `cd src-tauri && cargo test osinstall::` then `pnpm test`
Expected: PASS, including `every_offered_release_resolves_to_a_recipe` and the
i18n parity tests.

- [ ] **Step 9: Mutations**

| Mutation | Test that must fail |
|---|---|
| give `update-322-locale-tr` a `Languages` rule | `every_locale_component_names_only_drawers_its_own_disk_carries` |
| add `Other` to a locale component | `nothing_places_the_drawers_the_release_leaves_alone` |
| add `C/AmigaModel` to `update-322-system` | `the_ten_c_tools_the_release_does_not_place_are_not_placed` |
| point `update-322-diskdoctor` at the `base` layer | `the_two_diskdoctors_are_two_components_in_two_layers` |
| drop one `overrides` entry | the collision test |

- [ ] **Step 10: Commit**

Message: `feat(osinstall): ship the AmigaOS 3.2.2 recipe`

---

## Task 9: The tree says what it is

**Files:**
- Modify: `src-tauri/src/core/osinstall/identify.rs` (add `release_of_tree`)
- Modify: `src-tauri/src/core/osinstall/apply.rs:322` (`DistributionManifest.layers`)
- Modify: `src-tauri/src/commands/osinstall.rs` (carry the sentence out)
- Modify: `src/i18n/{en,tr}.json`
- Test: inline in `identify.rs` and `apply.rs`

**Interfaces:**
- Consumes: Task 3's layer folders
- Produces: `identify::release_of_tree(root: &Path) -> CoreResult<Option<String>>`; `DistributionManifest.layers: Vec<LayerRecord { id: String, folder: PathBuf }>`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_tree_states_its_own_release_from_the_file_the_release_wrote() {
    let dir = scratch("release-marker");
    let root = dir.join("tree");
    std::fs::create_dir_all(root.join("Prefs/Env-Archive/Versions")).unwrap();
    std::fs::write(root.join("Prefs/Env-Archive/Versions/Release"), b"Release 3.2.2\n").unwrap();
    assert_eq!(release_of_tree(&root).unwrap().as_deref(), Some("Release 3.2.2"));
}

#[test]
fn a_tree_with_no_marker_says_so_rather_than_guessing() {
    let dir = scratch("release-marker-absent");
    let root = dir.join("tree");
    std::fs::create_dir_all(&root).unwrap();
    assert_eq!(release_of_tree(&root).unwrap(), None);
}

#[test]
fn the_manifest_records_which_folder_each_layer_came_from() {
    let manifest = apply_a_layered_build();
    let ids: Vec<&str> = manifest.layers.iter().map(|l| l.id.as_str()).collect();
    assert_eq!(ids, vec!["base", "update-3.2.2"]);
    assert!(manifest.layers.iter().all(|l| l.folder.is_absolute()));
}

#[test]
fn an_older_manifest_with_no_layers_still_reads_back() {
    let json = r#"{"release":"AmigaOS 3.2","builtFrom":[],"files":[]}"#;
    let m: DistributionManifest = serde_json::from_str(json).unwrap();
    assert!(m.layers.is_empty());
}
```

- [ ] **Step 2: Run and watch them fail**

Run: `cd src-tauri && cargo test osinstall:: -- release_of_tree`
Expected: FAIL — no such function.

- [ ] **Step 3: Implement**

`release_of_tree` reads `Prefs/Env-Archive/Versions/Release` with a **bounded**
read (the file is 11 or 14 bytes; cap at 256 and refuse anything longer rather
than loading whatever is there), trims trailing whitespace, and returns `None`
when the file is absent. `DistributionManifest` gains
`#[serde(default)] pub layers: Vec<LayerRecord>`.

- [ ] **Step 4: Give it its own sentence**

The OS Builder's result panel reports the marker as **its own line**, with
three distinct i18n keys — the tree says `Release 3.2.2`; the tree says
something else, naming both; the tree states no release. Never folded into a
pass or a fail: they are three different next steps.

- [ ] **Step 5: Run the tests**

Run: `cd src-tauri && cargo test osinstall::` then `pnpm test`
Expected: PASS.

- [ ] **Step 6: Mutations**

| Mutation | Test that must fail |
|---|---|
| return the recipe's `release` when the file is absent | `a_tree_with_no_marker_says_so_rather_than_guessing` |
| drop `#[serde(default)]` from `layers` | `an_older_manifest_with_no_layers_still_reads_back` |
| collapse the three sentences into one | the frontend test asserting the specific key for a mismatch |

- [ ] **Step 7: Commit**

Message: `feat(osinstall): report the release the built tree states about itself`

---

## Task 10: One folder field per layer

**Files:**
- Modify: `src-tauri/src/commands/osinstall.rs` (a new `osinstall_layers` command)
- Modify: `src-tauri/src/lib.rs` (`invoke_handler![]`)
- Modify: `src/lib/osinstall.ts` (the typed wrapper)
- Modify: `src/components/osbuilder/OsInstall.tsx:283-284` (the remembered key), `:1118-1155` (the fields)
- Modify: `src/lib/buildSession.ts` (the remembered value becomes per-layer)
- Modify: `src/i18n/{en,tr}.json`
- Test: `src/components/osbuilder/OsInstall.test.tsx`, inline in `commands/osinstall.rs`

**Interfaces:**
- Consumes: Task 3's `mediaFolders`, Task 8's `label_key`s
- Produces: `osinstall_layers(release) -> Vec<MediaLayer>`; `layersFor(release): Promise<InstallLayer[]>` in `src/lib/osinstall.ts`

**The gap this task closes, found by the pre-flight scan.** Nothing carries a
release's layers to the frontend. `INSTALL_RELEASES` in `src/lib/osinstall.ts`
is a hand-maintained list pinned to `recipe::releases()` by a test, but layers
carry an order and a `label_key` as well as an id — three facts, not one, and
duplicating them by hand is how the recipe and the screen drift apart. So a
command, following `osinstall_release_for_media`'s shape exactly:

```rust
/// The media layers the named release's recipe declares, in its own order.
#[tauri::command]
pub async fn osinstall_layers(release: String) -> Result<Vec<MediaLayer>, AppError> {
    Ok(recipe::by_release(&release)?.layers)
}
```

registered in `lib.rs`'s `invoke_handler![]` (a new command must be in **both**
that list and a typed wrapper — the frontend never calls `invoke` from a
component), with:

```ts
/** One media folder the chosen release asks for, in the recipe's own order. */
export interface InstallLayer {
  id: string;
  labelKey: string | null;
}

export async function layersFor(release: InstallRelease): Promise<InstallLayer[]> {
  return invoke<InstallLayer[]>("osinstall_layers", { release });
}
```

An unlayered release answers with an empty array, which is what makes the
"unchanged for AmigaOS 3.2" test below meaningful rather than accidental.

- [ ] **Step 1: Write the failing tests**

```tsx
it("shows one labelled folder field per layer the release declares", async () => {
  renderOsInstall({ release: "AmigaOS 3.2.2" });
  expect(await screen.findByLabelText(/AmigaOS 3.2 base media/i)).toBeInTheDocument();
  expect(await screen.findByLabelText(/AmigaOS 3.2.2 update media/i)).toBeInTheDocument();
});

it("sends one folder per layer", async () => {
  const sent = await planWithFolders({
    base: "E:\\media\\3.2",
    "update-3.2.2": "E:\\media\\Update3.2.2",
  });
  expect(sent.mediaFolders).toEqual({
    base: "E:\\media\\3.2",
    "update-3.2.2": "E:\\media\\Update3.2.2",
  });
});

it("keeps the single folder field for an unlayered release", async () => {
  renderOsInstall({ release: "AmigaOS 3.2" });
  expect(await screen.findByLabelText(/media/i)).toBeInTheDocument();
  expect(screen.queryByTestId("extra-media-folder")).not.toBeInTheDocument();
});

it("remembers each layer's folder separately across a remount", async () => {
  // ART's standing rule: nothing changes unless the user changes it.
  await setFolders({ base: "E:\\a", "update-3.2.2": "E:\\b" });
  const remounted = await remountOsInstall();
  expect(remounted.folderFor("base")).toBe("E:\\a");
  expect(remounted.folderFor("update-3.2.2")).toBe("E:\\b");
});
```

- [ ] **Step 2: Run and watch them fail**

Run: `pnpm vitest run src/components/osbuilder/OsInstall.test.tsx`
Expected: FAIL — no per-layer fields.

- [ ] **Step 3: Implement**

The media step calls `layersFor(release)` when the release changes and renders
one `Field` per layer, in the returned order, labelled with `t(labelKey)`. An
**empty** array means unlayered, and the step renders exactly what it renders
today — the single `Field` plus the existing add-folder list, untouched.

The remembered key is `osinstall.mediaFolder.<release>.<layerId>`, read through
`recallInto` with the existing guard, so a stale settings file falls back to
the default rather than putting a bad value on screen. Per-layer keys are the
point: the standing rule is that nothing changes unless the user changes it,
and a single key shared across layers would hand the update field the base
folder the first time a user switched releases.

- [ ] **Step 4: Run the tests**

Run: `pnpm test && pnpm lint`
Expected: PASS.

- [ ] **Step 5: Mutations**

| Mutation | Test that must fail |
|---|---|
| key the remembered folder by release only | `remembers each layer's folder separately` |
| render the fields in declaration-reverse order | the label-order assertion in the first test |
| send the folders as an array | `sends one folder per layer` |

- [ ] **Step 6: Commit**

Message: `feat(osbuilder): a folder field per media layer, labelled by the recipe`

---

## Task 11: The oracle and the real-material hook

**Files:**
- Create: `scripts/icon-oracle-check.py`
- Modify: `src-tauri/src/core/osinstall/apply.rs` (a new `#[ignore]`d test)
- Modify: `README.md` / `CLAUDE.md`'s command list, `docs/STATUS.md`'s reproduce block
- Test: the script is the test

**Interfaces:**
- Consumes: Tasks 5 and 8
- Produces: nothing downstream

- [ ] **Step 1: Add the Rust half the script drives**

The existing oracle scripts drive the Rust side through
`subprocess.run(["cargo", "test", "--quiet", <test>, "--", "--nocapture"])`
with the material named in environment variables
(`scripts/pfs3-oracle-check.py:152`). Follow that exactly rather than
inventing a second mechanism. In `core/amigaicon/mod.rs`:

```rust
#[test]
#[ignore = "needs a folder of real .info files"]
fn round_trip_every_icon_in_a_folder_when_asked() {
    let Ok(folder) = std::env::var("ART_ICON_DIR") else { return };
    let mut checked = 0usize;
    let mut failed: Vec<String> = Vec::new();
    for entry in walk(&folder) {
        let bytes = std::fs::read(&entry).unwrap();
        checked += 1;
        match (layout(&bytes), merge_tooltypes(&bytes, &bytes)) {
            (Ok(l), Ok(same)) if same == bytes => {
                // The layout has to land on the file's end, or on the start
                // of an appended ColorIcon block — never mid-file.
                assert!(l.trailing.end == bytes.len());
            }
            _ => failed.push(entry.display().to_string()),
        }
    }
    println!("ART_ICON_RESULT checked={checked} failed={}", failed.len());
    for f in &failed {
        println!("ART_ICON_FAIL {f}");
    }
    assert!(failed.is_empty(), "{} icons did not round-trip", failed.len());
}
```

- [ ] **Step 1b: Write the script**

`scripts/icon-oracle-check.py` takes one or more media folders, extracts every
`.info` from every ADF in them with `xdftool` into a scratch directory, runs
the test above with `ART_ICON_DIR` pointing at it, and reports the
`ART_ICON_RESULT` count and every `ART_ICON_FAIL` line by name. **Not in CI** —
it needs the owner's media. Head the file with what it proves and why, the way
`pfs3-oracle-check.py` does: ART's own parser and its own writer agreeing is
not evidence of anything; agreeing with files Commodore and Hyperion wrote is.

- [ ] **Step 2: Run it against the owner's own material**

```bash
python scripts/icon-oracle-check.py "E:\amiga\Amigatolon\paketler\3.2\AmigaOs 3.2\ADF"
```

Expected: every `.info` round-trips. Record the count in the commit message and
in `docs/STATUS.md`.

- [ ] **Step 3: Add the real-material install hook**

In `apply.rs`, beside the existing `#[ignore]`d hooks:

```rust
#[test]
#[ignore = "needs the owner's own 3.2 and 3.2.2 media"]
fn build_the_real_322_tree_when_asked() {
    let Ok(base) = std::env::var("ART_322_BASE") else { return };
    let Ok(update) = std::env::var("ART_322_UPDATE") else { return };
    let Ok(dest) = std::env::var("ART_322_DEST") else { return };
    // … plan and apply with the two layers, then:
    let stated = release_of_tree(Path::new(&dest)).unwrap();
    assert_eq!(
        stated.as_deref(),
        Some("Release 3.2.2"),
        "the tree has to say what it is; a passing test is a claim about the code, \
         this file is the claim about the tree"
    );
}
```

- [ ] **Step 4: Run it for real**

```bash
cd src-tauri && ART_322_BASE="E:\amiga\Amigatolon\paketler\3.2\AmigaOs 3.2\ADF" \
  ART_322_UPDATE="E:\amiga\ProjeART\layer-research\Update3.2.2\ADFs" \
  ART_322_DEST="E:\amiga\ProjeART\dist-3.2.2" \
  cargo test build_the_real_322_tree_when_asked -- --nocapture --ignored
```

Expected: the tree is built and says `Release 3.2.2`. **If it does not, that is
the finding** — file it as an `ART-NNN` and do not soften the assertion.

- [ ] **Step 5: Full verification**

```bash
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings
cd src-tauri && cargo test          # twice; quote the `test result:` line
pnpm lint && pnpm test
python scripts/control-byte-sweep.py
python scripts/scratch-counter-sweep.py
python scripts/contrast-check.py --quiet
```

- [ ] **Step 6: Update the four documents CLAUDE.md names**

`docs/session-log.md` (a row at the top), `docs/STATUS.md` (the snapshot
numbers and the "Picking up next session" block, **updated in place**),
`docs/ISSUES.md` (the two new `ART-NNN`s from the design's §8 and anything the
real run found), `docs/FEATURES.md` (flip the rows, but only where a test
exists), `CHANGELOG.md` (the user-visible change).

Also correct work-list item 3 in
`docs/superpowers/specs/2026-09-04-work-list.md` in place, per that file's own
instruction.

- [ ] **Step 7: Commit and merge**

```bash
git add -- <the files above>
git commit -F <message file>
git checkout main && git merge --no-ff art-layered-release
```

Check `git branch --show-current` before committing, and never pipe the merge
into `tail` — the pipeline's exit code is `tail`'s.

---

## Self-review notes

- **Spec coverage.** §1 → Tasks 1, 3, 10. §2 → Task 8. §3 → Task 4. §4 →
  Tasks 5, 6. §5 → Task 7, whose scope grew when a measurement refuted the header proxy: a Kickstart resident-table reader and two conditions, not one `minor` field. §6 → Task 9. §7 is the "not built" list and needs
  no task; its `WBStartup` no-op claim is asserted by Task 8's collision test
  plus a dedicated assertion added there. §8's two `ART-NNN`s → Task 11 step 6.
  §9's mutation table is distributed across every task's `Mutations` block.
  §10's three risks are carried into Task 11's real run.
- **The one thing this plan cannot promise.** Task 8's rule lists are written
  from a census of one copy of the media. Task 11 step 4 is what tests that,
  and its failure is a finding rather than a reason to weaken the assertion.
