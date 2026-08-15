# AmigaOS install engine (SD-2 · G5) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the user's own AmigaOS 3.2 media into a *distribution tree* on the PC — files, `.uaem` sidecars, and a `distribution.json` recording where each file came from — and put that tree onto a real FFS or PFS3 volume with ART's own code.

**Architecture:** A new pure module `core/osinstall/` reads install media through a `MediaSource` trait and lays out a host tree from a JSON **recipe**, where a component is a named set of paths rather than a whole disk. A second implementation of the existing `core/preload::VolumeFormatter` trait — `core/preload/native.rs`, backed by `libpfs3` and `core/volume/write` — writes that tree onto a volume without launching any program, with every PFS3 block write passing through ART's own journal.

**Tech Stack:** Rust (MSRV 1.93), `libpfs3` 0.1.3 (new), existing `core/volume`, `core/adf`, `core/rom`, `core/jobs`, `core/safety`, `core/security`; React + TypeScript for the OS Builder screen; Python for the new local oracle.

**Spec:** [`docs/superpowers/specs/2026-08-15-os-install-design.md`](../specs/2026-08-15-os-install-design.md)

## Global Constraints

Every task's requirements implicitly include this section.

- **ART ships no copyrighted Amiga content, ever.** Every fixture is synthetic and built at runtime in a tempdir, via `core::adf::create::create_blank_adf` plus `VolumeWriter`.
- **`core/` never uses `tauri`, never calls a Windows API, never opens a network connection.** `libpfs3` is acceptable inside `core/` precisely because it launches nothing and has no OS dependency.
- **Refusals are typed values, not sentences.** `core/` is English and [ART-060](../../ISSUES.md) is open, so anything the UI must translate arrives as an enum variant. Follow `core/layout::RefusalReason`.
- **`safe_join` on every destination path.** Recipe paths are data a human typed.
- **Creating a file is `SAFE_CREATE`:** refuse when the target exists rather than replacing it.
- **Cancellation is checked only between whole units of work**, never mid-write. Return `CoreError::Cancelled`.
- **Bounds:** never index a block buffer directly; use `blocks::block_slice` / `read_u32_at` on the reading side.
- **Every i18n key goes into `src/i18n/en.json` *and* `src/i18n/tr.json` in the same commit.**
- **New commands go into both `invoke_handler![]` in `lib.rs` and a typed wrapper in `src/lib/*.ts`.** Components never call `invoke` directly.
- **Run the Rust suite twice before declaring a task green** ([ART-059](../../ISSUES.md)).
- Verification commands, from `amiga-retro-toolkit/`:
  ```bash
  pnpm lint && pnpm test
  cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
  ```

## Phase split

**Phase A — the tree (Tasks 1–7).** Produces a distribution tree from media. No volume, no card, no new dependency. Independently valuable and fully testable in a tempdir.

**Phase B — the volume (Tasks 8–11).** Adds `libpfs3`, the adapter, the native formatter, the read-back check and the oracle. **Independent of Phase A** — the two can be built in either order or in parallel.

**Phase C — the surface (Tasks 12–14).** Commands, wrapper, screen, and the run against real material.

---

## Shared test fixtures

Written once here because five tasks use them; the skill's DRY rule applies to
plans too. Put them in a `#[cfg(test)] mod fixtures` inside
`core/osinstall/mod.rs` and `use super::fixtures::*` from each module's tests.

```rust
#[cfg(test)]
pub(crate) mod fixtures {
    use std::path::{Path, PathBuf};

    use crate::core::adf::create::create_blank_adf;
    use crate::core::adf::FileSystemType;
    use crate::core::jobs::ProgressSink;

    /// A synthetic install disk. **ART ships no Amiga content** — this builds
    /// one, now, in a tempdir.
    ///
    /// `entries` is `(path, bytes, protection)`. Protection is `HSPARWED` with
    /// `RWED` inverted, so `0x20` is `--p-rwed` and `0x42` is `-s--rw-d`.
    pub fn media(dir: &Path, volume: &str, filename: &str,
                 entries: &[(&str, &[u8], u32)]) -> PathBuf {
        let path = dir.join(filename);
        std::fs::write(&path, create_blank_adf(volume, FileSystemType::Ffs, false).unwrap())
            .unwrap();
        // Open the image with VolumeWriter and create each path's parents,
        // then write the file with its protection through FileMeta.
        write_entries(&path, entries);
        path
    }

    /// `Workbench3.2` with the two files every test in this plan leans on.
    pub fn workbench(dir: &Path) -> PathBuf {
        media(dir, "Workbench3.2", "wb.adf", &[
            ("C/LoadModule", b"cmd", 0x20),          // --p-rwed
            ("S/Startup-sequence", b"; test\n", 0x42), // -s--rw-d
        ])
    }

    /// Stops the job after `n` units, so a cancel path can be tested without
    /// timing.
    pub struct CancelAfter {
        limit: u64,
        seen: std::sync::atomic::AtomicU64,
    }

    impl CancelAfter {
        pub fn new(limit: u64) -> Self {
            Self { limit, seen: std::sync::atomic::AtomicU64::new(0) }
        }
    }

    impl ProgressSink for CancelAfter {
        fn report(&self, _done: u64, _total: Option<u64>, _message: &str) {
            self.seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        fn is_cancelled(&self) -> bool {
            self.seen.load(std::sync::atomic::Ordering::SeqCst) >= self.limit
        }
    }

    /// One hash over a whole folder, so "unchanged" is a single assertion.
    /// Sorted, so it does not depend on directory order.
    pub fn digest_of_folder(root: &Path) -> String {
        use sha2::{Digest, Sha256};
        let mut names: Vec<PathBuf> = walkdir_sorted(root);
        names.sort();
        let mut hasher = Sha256::new();
        for path in names {
            hasher.update(path.to_string_lossy().as_bytes());
            if path.is_file() {
                hasher.update(std::fs::read(&path).unwrap());
            }
        }
        format!("{:x}", hasher.finalize())
    }
}
```

**For the block-device tests (Task 8), do not invent a device.**
`core::volume::device::VecDevice` already exists and implements both
`BlockDevice` and `BlockDeviceMut`:
`VecDevice::new(vec![0u8; 512 * 64], 512)`.

The plan-level and volume-level helpers, used by Tasks 5, 6, 9 and 10:

```rust
#[cfg(test)]
pub(crate) mod fixtures {
    // …continued…

    /// A media folder plus a plan over it, so a test states only what it
    /// varies. `present` lists the volume names to create.
    pub fn planned_with(chosen: &[&str], present: &[&str], rom_major: Option<u16>)
        -> (crate::core::osinstall::plan::InstallPlan, PathBuf)
    {
        let dir = fixtures::scratch("<tag>");   // returns PathBuf, not a TempDir
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        for volume in present {
            media(&folder, volume, &format!("{volume}.adf"), default_entries_for(volume));
        }
        let rom = rom_major.map(|major| fake_rom(&dir, major));
        let request = InstallRequest {
            media_folder: folder,
            rom,
            chosen: chosen.iter().map(|s| s.to_string()).collect(),
            destination: dir.join("dist"),
        };
        (plan(&request, &recipe::amigaos_32().unwrap()).unwrap(), dir)
    }

    /// A ROM file that states `major` in its own header — which is what
    /// `core::rom::stated_version` reads, and what the Modules condition asks.
    /// Deliberately *not* a real dump: ART ships none, and the condition does
    /// not consult `KNOWN_ROMS` anyway (ART-104).
    pub fn fake_rom(dir: &Path, major: u16) -> PathBuf {
        let path = dir.join(format!("kick-{major}.rom"));
        let mut bytes = vec![0u8; 512 * 1024];
        bytes[12..14].copy_from_slice(&major.to_be_bytes());
        bytes[14..16].copy_from_slice(&68u16.to_be_bytes());
        std::fs::write(&path, &bytes).unwrap();
        path
    }

    /// A small RDB image with one partition of the given DosType, for the
    /// formatter tests. Built with ART's own `core/card/build.rs`, so these
    /// tests need no external tool and no card.
    pub fn rdb_image(dir: &Path, dostype: &[u8; 4], megabytes: u64) -> PathBuf { /* … */ }

    /// The byte offset of partition 1 inside `image`, read back through
    /// `core::card::read_card` rather than recomputed — the same number two
    /// ways is how ART-095 was found.
    pub fn partition_offset(image: &Path) -> u64 { /* … */ }
}
```

---

### Task 1: The recipe, as data with its own tests

**Files:**
- Create: `src-tauri/src/core/osinstall/mod.rs`
- Create: `src-tauri/src/core/osinstall/recipe.rs`
- Create: `src-tauri/src/core/osinstall/recipes/amigaos-3.2.json`
- Modify: `src-tauri/src/core/mod.rs` (add `pub mod osinstall;`)

**Interfaces:**
- Consumes: `crate::core::error::{CoreError, CoreResult}`, `crate::core::volume::write::dir::check_name`
- Produces: `Recipe`, `Component`, `PathRule`, `RuleKind`, `Condition`, `RefusalReason`, `recipe::amigaos_32() -> &'static Recipe`

- [ ] **Step 1: Write the types**

`core/osinstall/mod.rs`:

```rust
//! Installing AmigaOS from the user's own media (SD-2 · G5).
//!
//! ## A component is a set of paths, not a disk
//!
//! Measured, not assumed: `ModulesA1200_3.2.adf` holds 14 commands in `C/`,
//! and **thirteen are boot-floppy copies of commands `Workbench3.2` already
//! carries**. Exactly one, `LoadModule`, is new. Copying that disk onto `SYS:`
//! downgrades thirteen commands. `HDSetup3.2` (22), `DiskDoctor` (39) and
//! `Storage3.2` (9) have the same shape.
//!
//! So the unit of choice is a named set of [`PathRule`]s, and the recipe says
//! which paths on which media are actually wanted.

pub mod apply;
pub mod plan;
pub mod recipe;
pub mod scan;
pub mod source;
pub mod startup;

use serde::{Deserialize, Serialize};

/// Whether a rule takes one file or a whole subtree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuleKind {
    File,
    Subtree,
}

/// One path taken out of a component's media.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathRule {
    /// Where it lives on the media, `/`-separated: `LIBS/Modules`.
    pub from: String,
    /// Where it goes in the tree, `/`-separated: `Libs/Modules`.
    pub to: String,
    pub kind: RuleKind,
}

/// When a component applies without the user being asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "condition", rename_all = "kebab-case")]
pub enum Condition {
    /// On when the paired Kickstart's own stated major is below this.
    ///
    /// The ROM's **own header** answers it (`core::rom::stated_version`), not a
    /// checksum table — which is what keeps [ART-104](../../ISSUES.md) from
    /// repeating: the user's licensed A1200 dump is not in `KNOWN_ROMS`, and a
    /// condition resting on that table would misfire on a ROM that is right.
    RomOlderThan { major: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Component {
    pub id: String,
    /// The **volume name inside the image**, not a filename: `Workbench3.2`.
    pub media: String,
    pub rules: Vec<PathRule>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub condition: Option<Condition>,
    /// Component ids this one may legitimately write over.
    #[serde(default)]
    pub overrides: Vec<String>,
    /// Lines for `S:User-Startup`, written inside this component's own block.
    #[serde(default)]
    pub user_startup: Vec<String>,
    /// Only one of a group may be chosen — `"modules"` for the Modules disks.
    #[serde(default)]
    pub exclusive_group: Option<String>,
    /// Registered but not built (CLAUDE.md, §96): shown as Coming Later.
    #[serde(default = "yes")]
    pub available: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recipe {
    /// `"AmigaOS 3.2"`.
    pub release: String,
    pub components: Vec<Component>,
}

impl Recipe {
    pub fn component(&self, id: &str) -> Option<&Component> {
        self.components.iter().find(|c| c.id == id)
    }
}

/// Why an install cannot proceed. A value, never a sentence — the UI
/// translates it (ART-060).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "refusal", rename_all = "kebab-case")]
pub enum RefusalReason {
    /// No image in the folder carries this volume name.
    MediaMissing {
        component: String,
        volume_name: String,
    },
    /// The media is here and the path the recipe expects is not — so the
    /// recipe is wrong about *this* media, probably a different revision.
    /// Skipping it silently would give a system missing a library.
    MediaPathMissing {
        component: String,
        media: String,
        path: String,
    },
    /// The ROM was not identified, so a `Condition` cannot be decided.
    RomUnknown,
    /// Two components claim one destination and neither declared an override.
    DestinationCollision {
        path: String,
        components: Vec<String>,
    },
}
```

> **Two refusals the spec originally listed are deliberately absent**, and the
> reason is worth carrying: `DoesNotFit` cannot be answered by a module that
> produces a *tree* — the tree carries no volume name, so only the write step
> can ask it, and Task 9 does. `NameNotStorable` cannot arise: destinations are
> checked against `check_name` when the recipe loads, and source names came off
> an Amiga volume where they were already storable. A variant nothing
> constructs is dead API.

Note the module list at the top of this file: `verify` (Task 10) belongs in it
too.

```rust
pub mod verify;
```

- [ ] **Step 2: Write the failing recipe tests**

Append to `core/osinstall/recipe.rs`:

```rust
//! The shipped recipes, as data.
//!
//! `include_str!` for the same three reasons `core/distro` uses it: reviewable
//! in a diff, shipped without a network, and unable to grow a code path.

use super::{Component, PathRule, Recipe, RuleKind};
use crate::core::error::{CoreError, CoreResult};

const AMIGAOS_32_JSON: &str = include_str!("recipes/amigaos-3.2.json");

/// Parse and validate a recipe.
pub fn parse(json: &str) -> CoreResult<Recipe> {
    let recipe: Recipe = serde_json::from_str(json)
        .map_err(|e| CoreError::Malformed { format: "recipe".into(), detail: e.to_string() })?;
    validate(&recipe)?;
    Ok(recipe)
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert!(extras.rules.iter().any(|r| r.to == "L"), "extras must carry L");

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
        assert!(english.rules.iter().any(|r| r.to.starts_with("Locale/Languages")));
    }

    #[test]
    fn every_destination_is_a_name_amigados_can_store() {
        for component in &recipe().components {
            for rule in &component.rules {
                for segment in rule.to.split('/') {
                    crate::core::volume::write::dir::check_name(segment).unwrap_or_else(|e| {
                        panic!("{}: destination '{}' segment '{segment}': {e}", component.id, rule.to)
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

        for id in ["modules-a1200", "diskdoctor", "mmulibs", "hdtools", "storage"] {
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
                assert!(recipe.component(over).is_some(), "{}: no such component '{over}'", component.id);
            }
        }
    }

    #[test]
    fn no_rule_escapes_the_tree() {
        for component in &recipe().components {
            for rule in &component.rules {
                assert!(!rule.to.starts_with('/'), "{}: '{}' is absolute", component.id, rule.to);
                assert!(!rule.to.split('/').any(|s| s == ".."), "{}: '{}' climbs", component.id, rule.to);
            }
        }
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test osinstall::recipe`
Expected: FAIL — `recipes/amigaos-3.2.json` does not exist, so `include_str!` will not compile.

- [ ] **Step 4: Write the recipe JSON and the validator**

`core/osinstall/recipes/amigaos-3.2.json` — the components measured off the real media. Abbreviated here to the shapes that matter; the implementer fills each drawer list from the ADFs (`xdftool <adf> list`), and the tests above police the result:

```json
{
  "release": "AmigaOS 3.2",
  "components": [
    {
      "id": "workbench-base",
      "media": "Workbench3.2",
      "required": true,
      "rules": [
        { "from": "C", "to": "C", "kind": "subtree" },
        { "from": "Classes", "to": "Classes", "kind": "subtree" },
        { "from": "Devs", "to": "Devs", "kind": "subtree" },
        { "from": "Expansion", "to": "Expansion", "kind": "subtree" },
        { "from": "Libs", "to": "Libs", "kind": "subtree" },
        { "from": "Prefs", "to": "Prefs", "kind": "subtree" },
        { "from": "Rexxc", "to": "Rexxc", "kind": "subtree" },
        { "from": "S", "to": "S", "kind": "subtree" },
        { "from": "System", "to": "System", "kind": "subtree" }
      ]
    },
    {
      "id": "extras",
      "media": "Extras3.2",
      "rules": [
        { "from": "L", "to": "L", "kind": "subtree" },
        { "from": "Prefs", "to": "Prefs", "kind": "subtree" },
        { "from": "S", "to": "S", "kind": "subtree" },
        { "from": "System", "to": "System", "kind": "subtree" },
        { "from": "Tools", "to": "Tools", "kind": "subtree" }
      ],
      "overrides": ["workbench-base"]
    },
    {
      "id": "locale-base",
      "media": "Locale",
      "rules": [
        { "from": "Catalogs", "to": "Locale/Catalogs", "kind": "subtree" },
        { "from": "Countries", "to": "Locale/Countries", "kind": "subtree" },
        { "from": "Support", "to": "Locale/Support", "kind": "subtree" }
      ]
    },
    {
      "id": "locale-en",
      "media": "Locale-EN",
      "rules": [
        { "from": "Languages", "to": "Locale/Languages", "kind": "subtree" },
        { "from": "Help", "to": "Locale/Help", "kind": "subtree" }
      ],
      "overrides": ["locale-base"]
    },
    {
      "id": "modules-a1200",
      "media": "ModulesA1200_3.2",
      "condition": { "condition": "rom-older-than", "major": 47 },
      "exclusive_group": "modules",
      "rules": [
        { "from": "C/LoadModule", "to": "C/LoadModule", "kind": "file" },
        { "from": "LIBS/Modules", "to": "Libs/Modules", "kind": "subtree" },
        { "from": "DEVS/A1200", "to": "Devs/A1200", "kind": "subtree" },
        { "from": "LIBS/A1200", "to": "Libs/A1200", "kind": "subtree" }
      ]
    },
    {
      "id": "update-3.2.1",
      "media": "Update3.2.1",
      "available": false,
      "rules": []
    }
  ]
}
```

**The remaining components, with their real contents.** These were measured off
the user's media (`xdftool <adf> list`), not guessed, and four of them are not
the shape "copy the disk" at all — the ModulesA1200 lesson repeats across the
whole set:

```json
    { "id": "fonts", "media": "Fonts",
      "rules": [ { "from": "", "to": "Fonts", "kind": "subtree" } ] },

    { "id": "classes", "media": "Classes3.2",
      "rules": [ { "from": "Classes", "to": "Classes", "kind": "subtree" },
                 { "from": "Devs",    "to": "Devs",    "kind": "subtree" } ],
      "overrides": ["workbench-base"] },

    { "id": "glowicons", "media": "GlowIcons3.2",
      "rules": [ { "from": "Devs", "to": "Devs", "kind": "subtree" },
                 { "from": "Prefs", "to": "Prefs", "kind": "subtree" },
                 { "from": "Storage", "to": "Storage", "kind": "subtree" },
                 { "from": "System", "to": "System", "kind": "subtree" },
                 { "from": "Tools", "to": "Tools", "kind": "subtree" } ],
      "overrides": ["workbench-base", "extras", "storage"] },

    { "id": "backdrops", "media": "Backdrops3.2", "available": false,
      "rules": [ { "from": "", "to": "Backdrops", "kind": "subtree" } ] }
```

`from: ""` means the media's own root. Two disks need it: `Fonts.adf` and
`Backdrops3.2.adf` are flat — the whole disk *is* the drawer.

`backdrops` ships **`available: false` on purpose**. Where the real installer
places these wallpapers has not been established, and this project does not
guess at destinations. It turns on when somebody reads it off the Installer
script or a real installed 3.2 — and that is a task in its own right, not a
line to fill in here.

**The four toolkit disks are file-level, derived by diffing against
`workbench-base`.** Measured differences in `C/`:

| Component | Media | New in `C/` beyond `workbench-base` | Plus |
|---|---|---|---|
| `diskdoctor` | `DiskDoctor` | `DAControl`, `DiskDoctor`, `FixROMLibs` | — |
| `mmulibs` | `MMULibs` | `FPU`, `MuFastChip`, `MuFastRom`, `MuFastZero`, `MuLockLib`, `MuMapRom`, `MuProtectModules`, `MuRedox`, `MuScan`, `MuSetCacheMode`, `OmniScsiPatch` | `Libs/68020…68060.library`, `Libs/680x0.library`, `Libs/mmu.library`, `Libs/memory.library`, `Libs/disassembler.library` |
| `hdtools` | `HDSetup3.2` | `Check2090`, `CopyTooltypes`, `ExtractKickstart`, `FindResident`, `IconPos`, `NSDPatch`, `Prod_Prep`, `UpdateWBFiles` | the `HDTools/` drawer |
| `storage` | `Storage3.2` | `DefIcons`, `Edit`, `Group`, `LoadModule`, `LoadMonDrvs`, `MD5Sum`, `MountInfo`, `Owner`, `Reboot` | `DOSDrivers/`, `Keymaps/`, `Monitors/`, `Presets/`, `DefIcons/`, `Env-Archive/` under `Storage/` |

Each of those becomes one `"kind": "file"` rule per command. Verbose, and
correct — which is the trade this whole design makes.

`locale-XX` exists once per language ADF on the media — `Locale-DE`, `Locale-DK`,
`Locale-EN`, `Locale-ES`, `Locale-FR`, `Locale-GR`, `Locale-IT`, `Locale-NL`,
`Locale-NO`, `Locale-PL`, `Locale-PT`, `Locale-RU`, `Locale-SE`, `Locale-TR`,
`Locale-UK`. Fifteen entries, identical but for `id` and `media`, each with the
`locale-en` rules and `"overrides": ["locale-base"]` (they all carry a `Support`
drawer the base also has).

And the validator in `recipe.rs`:

```rust
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
                    detail: format!("'{}': destination '{}' leaves the tree", component.id, rule.to),
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
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test osinstall::recipe`
Expected: PASS, all nine.

- [ ] **Step 6: Mutation-check the load-bearing test**

Temporarily add `{ "from": "C", "to": "C", "kind": "subtree" }` to `modules-a1200`.
Run: `cd src-tauri && cargo test the_modules_component_takes_loadmodule -- --exact`
Expected: FAIL. **Revert the mutation.**

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/core/osinstall src-tauri/src/core/mod.rs
git commit -m "SD-2 G5: the recipe, and what a component actually is"
```

---

### Task 2: `MediaSource`, and an ADF behind it

**Files:**
- Create: `src-tauri/src/core/osinstall/source.rs`

**Interfaces:**
- Consumes: `core::volume::mount::{scan_image, mount, VolumeEntry}`, `core::volume::write::dir::{entries_in, find_entry, DirEntry}`, `core::volume::write::VolumeWriter::attributes`, `core::volume::write::file::read_file`, `core::adf::bcpl::AmigaDate`
- Produces:
  ```rust
  pub struct MediaEntry { pub path: String, pub is_dir: bool, pub size: u64,
                          pub protection: u32, pub date: AmigaDate, pub comment: String }
  pub trait MediaSource {
      fn volume_name(&self) -> &str;
      fn entry(&mut self, path: &str) -> CoreResult<Option<MediaEntry>>;
      fn walk(&mut self, path: &str) -> CoreResult<Vec<MediaEntry>>;
      fn read(&mut self, path: &str) -> CoreResult<Vec<u8>>;
  }
  pub struct AdfSource { /* … */ }
  impl AdfSource { pub fn open(image: &Path) -> CoreResult<Self> }
  ```

- [ ] **Step 1: Write the failing test**

In `source.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::adf::create::create_blank_adf;
    use crate::core::adf::FileSystemType;

    /// Build a synthetic ADF holding `C/LoadModule` and `S/Startup-sequence`.
    /// ART ships no Amiga content; every fixture is made here, now.
    /// `fixtures::media` from the Shared test fixtures section above.
    fn fixture(dir: &Path, volume: &str) -> PathBuf {
        super::super::fixtures::media(dir, volume, &format!("{volume}.adf"), &[
            ("C/LoadModule", b"cmd", 0x20),            // --p-rwed
            ("S/Startup-sequence", b"; test\n", 0x42), // -s--rw-d
        ])
    }

    #[test]
    fn a_source_reports_the_volume_name_from_inside_the_image() {
        let dir = fixtures::scratch("<tag>");   // returns PathBuf, not a TempDir
        // Deliberately a filename that says nothing.
        let image = fixture(&dir, "ModulesA1200_3.2");
        let renamed = dir.join("disk07.dat");
        std::fs::rename(&image, &renamed).unwrap();

        let source = AdfSource::open(&renamed).unwrap();
        assert_eq!(source.volume_name(), "ModulesA1200_3.2");
    }

    #[test]
    fn an_entry_carries_the_protection_bits_the_media_holds() {
        let dir = fixtures::scratch("<tag>");   // returns PathBuf, not a TempDir
        let mut source = AdfSource::open(&fixture(&dir, "Workbench3.2")).unwrap();

        let entry = source.entry("C/LoadModule").unwrap().unwrap();
        assert!(!entry.is_dir);
        assert_eq!(
            crate::core::volume::write::uaem::format_bits(entry.protection),
            "--p-rwed",
            "the pure bit is load-bearing: 3.2's Startup-Sequence runs \
             `Resident C:Assign PURE` and fails without it"
        );
    }

    #[test]
    fn a_missing_path_is_none_rather_than_an_error() {
        let dir = fixtures::scratch("<tag>");   // returns PathBuf, not a TempDir
        let mut source = AdfSource::open(&fixture(&dir, "Workbench3.2")).unwrap();
        assert!(source.entry("LIBS/Modules").unwrap().is_none());
    }

    #[test]
    fn walk_returns_a_subtree_with_paths_relative_to_the_media_root() {
        let dir = fixtures::scratch("<tag>");   // returns PathBuf, not a TempDir
        let mut source = AdfSource::open(&fixture(&dir, "Workbench3.2")).unwrap();
        let found = source.walk("C").unwrap();
        assert!(found.iter().any(|e| e.path == "C/LoadModule"));
    }

    #[test]
    fn read_returns_the_bytes() {
        let dir = fixtures::scratch("<tag>");   // returns PathBuf, not a TempDir
        let mut source = AdfSource::open(&fixture(&dir, "Workbench3.2")).unwrap();
        assert_eq!(source.read("S/Startup-sequence").unwrap(), b"; test\n");
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd src-tauri && cargo test osinstall::source`
Expected: FAIL — `AdfSource` not defined.

- [ ] **Step 3: Implement `AdfSource`**

Open the image with `scan_image`, take its single bare volume, `mount` it for a `FileRegion` + `VolumeGeometry`, and resolve `/`-separated paths by walking `find_entry` one segment at a time from `geometry.root_block`. `walk` recurses with `entries_in` under a depth cap of `MAX_SCAN_DEPTH`; **a chain walk needs a step limit** — a malformed image can loop forever. `read` is `file::read_file`. Attributes come from `VolumeWriter::attributes(entry_block)`.

```rust
impl MediaSource for AdfSource {
    fn volume_name(&self) -> &str {
        &self.volume_name
    }

    fn entry(&mut self, path: &str) -> CoreResult<Option<MediaEntry>> {
        let Some(block) = self.resolve(path)? else { return Ok(None) };
        Ok(Some(self.entry_at(path, block)?))
    }
    // …
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd src-tauri && cargo test osinstall::source`
Expected: PASS, all five.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/core/osinstall/source.rs
git commit -m "SD-2 G5: read install media behind a trait, ADF first"
```

---

### Task 3: Find the media in a folder, by what is inside it

**Files:**
- Create: `src-tauri/src/core/osinstall/scan.rs`

**Interfaces:**
- Consumes: `source::AdfSource`
- Produces:
  ```rust
  pub struct FoundMedia { pub path: PathBuf, pub volume_name: String }
  pub fn find_media(folder: &Path) -> CoreResult<Vec<FoundMedia>>;
  pub fn media_for<'a>(found: &'a [FoundMedia], volume_name: &str) -> Option<&'a FoundMedia>;
  ```

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_is_found_by_its_volume_name_not_its_filename() {
        let dir = fixtures::scratch("<tag>");   // returns PathBuf, not a TempDir
        make_adf(&dir, "Workbench3.2", "wb.adf");
        make_adf(&dir, "Extras3.2", "totally-unrelated-name.bin");

        let found = find_media(&dir).unwrap();
        assert!(media_for(&found, "Extras3.2").is_some());
    }

    #[test]
    fn a_file_that_is_not_an_amiga_image_is_skipped_not_an_error() {
        let dir = fixtures::scratch("<tag>");   // returns PathBuf, not a TempDir
        std::fs::write(dir.join("readme.txt"), b"hello").unwrap();
        make_adf(&dir, "Workbench3.2", "wb.adf");

        let found = find_media(&dir).unwrap();
        assert_eq!(found.len(), 1);
    }

    /// The user's own 3.2 folder holds 36 ADFs; a scan must not read 36 whole
    /// images into memory to learn 36 names.
    #[test]
    fn the_scan_is_not_recursive_and_does_not_follow_symlinks() {
        let dir = fixtures::scratch("<tag>");   // returns PathBuf, not a TempDir
        let nested = dir.join("sub");
        std::fs::create_dir(&nested).unwrap();
        make_adf(&nested, "Workbench3.2", "wb.adf");

        assert!(find_media(&dir).unwrap().is_empty());
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd src-tauri && cargo test osinstall::scan`
Expected: FAIL — `find_media` not defined.

- [ ] **Step 3: Implement**

One directory level, no symlinks, each candidate opened only far enough to read its volume name; anything that does not open as an Amiga volume is skipped rather than reported.

- [ ] **Step 4: Run to verify it passes**

Run: `cd src-tauri && cargo test osinstall::scan`
Expected: PASS, all three.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/core/osinstall/scan.rs
git commit -m "SD-2 G5: identify install media by what is inside it"
```

---

### Task 4: Decide the Modules component from the ROM itself

**Files:**
- Create: `src-tauri/src/core/osinstall/plan.rs` (the condition half only)

**Interfaces:**
- Consumes: `core::rom::stated_version`, `Condition`, `RefusalReason`
- Produces:
  ```rust
  pub struct RomFacts { pub major: u16 }
  pub fn rom_facts(rom: &Path) -> CoreResult<RomFacts>;
  pub fn condition_holds(condition: &Condition, rom: Option<&RomFacts>) -> Result<bool, RefusalReason>;
  ```

- [ ] **Step 1: Write the failing test**

```rust
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
    #[test]
    fn the_major_comes_from_the_roms_own_header() {
        let dir = fixtures::scratch("<tag>");   // returns PathBuf, not a TempDir
        let path = dir.join("fake.rom");
        let mut bytes = vec![0u8; 512 * 1024];
        bytes[12..14].copy_from_slice(&40u16.to_be_bytes());
        bytes[14..16].copy_from_slice(&68u16.to_be_bytes());
        std::fs::write(&path, &bytes).unwrap();

        assert_eq!(rom_facts(&path).unwrap().major, 40);
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd src-tauri && cargo test osinstall::plan::condition_tests`
Expected: FAIL — `condition_holds` not defined.

- [ ] **Step 3: Implement**

```rust
pub fn condition_holds(condition: &Condition, rom: Option<&RomFacts>) -> Result<bool, RefusalReason> {
    let rom = rom.ok_or(RefusalReason::RomUnknown)?;
    match condition {
        Condition::RomOlderThan { major } => Ok(rom.major < *major),
    }
}

pub fn rom_facts(rom: &Path) -> CoreResult<RomFacts> {
    let bytes = crate::core::rom::strip_cloanto_header(&std::fs::read(rom)?);
    let (major, _minor) = crate::core::rom::stated_version(&bytes)
        .ok_or_else(|| CoreError::InvalidInput("this file does not state a Kickstart version".into()))?;
    Ok(RomFacts { major })
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd src-tauri && cargo test osinstall::plan::condition_tests`
Expected: PASS, all four.

- [ ] **Step 5: Mutation-check**

Change `Ok(rom.major < *major)` to `Ok(true)`.
Run: `cd src-tauri && cargo test a_v47_rom_leaves_it_off -- --exact` → FAIL. **Revert.**

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/core/osinstall/plan.rs
git commit -m "SD-2 G5: the Modules question is answered by the ROM, not the user"
```

---

### Task 5: `plan()` — components, media and ROM to an install plan

**Files:**
- Modify: `src-tauri/src/core/osinstall/plan.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct PlanItem { pub component: String, pub media: String,
                        pub from: String, pub to: String, pub is_dir: bool, pub bytes: u64 }
  pub struct InstallPlan { pub release: String, pub items: Vec<PlanItem>,
                           pub refusals: Vec<RefusalReason>, pub total_bytes: u64,
                           pub components_on: Vec<String>,
                           /// Volume name -> the image it was found in.
                           /// Resolved here so `apply` can reopen the media
                           /// without re-scanning the folder — and so the plan
                           /// that was previewed is the plan that runs, even if
                           /// the folder changed underneath it.
                           pub media_paths: BTreeMap<String, PathBuf> }
  pub struct InstallRequest { pub media_folder: PathBuf, pub rom: Option<PathBuf>,
                              pub chosen: Vec<String>, pub destination: PathBuf }
  pub fn plan(request: &InstallRequest, recipe: &Recipe) -> CoreResult<InstallPlan>;
  ```

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod plan_tests {
    use super::*;

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
        assert!(plan.items.is_empty(), "nothing is planned once the media is wrong");
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
        let plan = plan_with(&["workbench-base", "extras"], &["Workbench3.2", "Extras3.2"]);
        assert!(!plan.refusals.iter().any(|r| matches!(r, RefusalReason::DestinationCollision { .. })));
    }

    #[test]
    fn a_conditional_component_is_on_without_being_chosen() {
        let plan = plan_with_rom(&["workbench-base"], 40);
        assert!(plan.components_on.iter().any(|c| c == "modules-a1200"));
    }

    #[test]
    fn the_total_is_the_sum_of_what_will_actually_be_written() {
        let plan = plan_with(&["workbench-base"], &["Workbench3.2"]);
        assert_eq!(plan.total_bytes, plan.items.iter().map(|i| i.bytes).sum::<u64>());
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd src-tauri && cargo test osinstall::plan::plan_tests`
Expected: FAIL — `plan` not defined.

- [ ] **Step 3: Implement `plan()`**

Order: resolve the chosen set (chosen + required + conditions), find each component's media by volume name, resolve every rule against that media, expand `Subtree` rules with `walk`, collect collisions across the whole item list, and sum. **Refusals do not stop the walk** — collect them all so the screen shows every problem at once rather than one per attempt — but any refusal leaves `items` empty, because a half-planned install is not something to preview.

- [ ] **Step 4: Run to verify they pass**

Run: `cd src-tauri && cargo test osinstall::plan`
Expected: PASS, all ten (four condition + six plan).

- [ ] **Step 5: Mutation-check the collision guard**

Make the collision check consider only the first two items.
Run: `cd src-tauri && cargo test two_components_wanting_one_path -- --exact` → FAIL. **Revert.**

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/core/osinstall/plan.rs
git commit -m "SD-2 G5: plan an install, and refuse what would install wrongly"
```

---

### Task 6: `apply()` — build the tree and its manifest

**Files:**
- Create: `src-tauri/src/core/osinstall/apply.rs`

**Interfaces:**
- Consumes: `InstallPlan`, `core::security::path::safe_join`, `core::volume::write::uaem::{render, sidecar_path, Sidecar}`, `core::jobs::ProgressSink`
- Produces:
  ```rust
  pub struct DistributionManifest { pub release: String, pub built_from: Vec<MediaRecord>,
                                    pub files: Vec<FileRecord> }
  pub struct MediaRecord { pub volume_name: String, pub sha256: String }
  pub struct FileRecord { pub path: String, pub component: String,
                          pub media: String, pub sha256: String, pub bytes: u64 }
  pub struct ApplyOutcome { pub root: PathBuf, pub files: u64, pub directories: u64, pub bytes: u64 }
  pub fn apply(plan: &InstallPlan, root: &Path, sink: &dyn ProgressSink) -> CoreResult<ApplyOutcome>;
  ```

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tree_carries_a_uaem_sidecar_for_every_file_with_something_to_say() {
        let (plan, dir) = planned();
        let root = dir.join("dist");
        apply(&plan, &root, &NoProgress).unwrap();

        let sidecar = root.join("C").join("LoadModule.uaem");
        assert!(sidecar.exists());
        assert!(std::fs::read_to_string(&sidecar).unwrap().starts_with("--p-rwed "));
    }

    #[test]
    fn the_manifest_says_which_component_and_which_media_each_file_came_from() {
        let (plan, dir) = planned();
        let root = dir.join("dist");
        apply(&plan, &root, &NoProgress).unwrap();

        let manifest: DistributionManifest =
            serde_json::from_str(&std::fs::read_to_string(root.join("distribution.json")).unwrap()).unwrap();
        let record = manifest.files.iter().find(|f| f.path == "C/LoadModule").unwrap();
        assert_eq!(record.component, "modules-a1200");
        assert_eq!(record.media, "ModulesA1200_3.2");
        assert_eq!(record.sha256.len(), 64);
    }

    /// SAFE_CREATE. A distribution folder already there is somebody's work.
    #[test]
    fn an_existing_destination_is_refused_never_written_into() {
        let (plan, dir) = planned();
        let root = dir.join("dist");
        std::fs::create_dir_all(&root).unwrap();
        assert!(apply(&plan, &root, &NoProgress).is_err());
    }

    /// The rule G11 proved by measurement: removing `safe_join` genuinely
    /// wrote outside the staging root.
    #[test]
    fn a_destination_that_climbs_out_of_the_root_is_refused() {
        let (mut plan, dir) = planned();
        plan.items[0].to = "../escaped".into();
        let root = dir.join("dist");
        assert!(apply(&plan, &root, &NoProgress).is_err());
        assert!(!dir.join("escaped").exists());
    }

    #[test]
    fn the_media_is_byte_for_byte_unchanged_afterwards() {
        let (plan, dir) = planned();
        let before = digest_of_folder(&media_folder(&dir));
        apply(&plan, &dir.join("dist"), &NoProgress).unwrap();
        assert_eq!(digest_of_folder(&media_folder(&dir)), before);
    }

    #[test]
    fn a_cancelled_apply_stops_between_files_and_says_how_many_landed() {
        let (plan, dir) = planned();
        let sink = CancelAfter::new(1);
        let err = apply(&plan, &dir.join("dist"), &sink).unwrap_err();
        assert!(matches!(err, CoreError::Cancelled));
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd src-tauri && cargo test osinstall::apply`
Expected: FAIL — `apply` not defined.

- [ ] **Step 3: Implement**

`SAFE_CREATE` first. Then, per item: `safe_join(root, &item.to)`, create parents, read bytes through the item's `MediaSource`, write the file, write a `.uaem` beside it when `sidecar_for` says there is something to record, hash as it goes. Check `sink.is_cancelled()` **between whole files**. Write `distribution.json` last, so a cancelled run leaves no manifest claiming a complete tree.

- [ ] **Step 4: Run to verify they pass**

Run: `cd src-tauri && cargo test osinstall::apply`
Expected: PASS, all six.

- [ ] **Step 5: Mutation-check `safe_join`**

Replace `safe_join(root, &item.to)?` with `root.join(&item.to)`.
Run: `cd src-tauri && cargo test a_destination_that_climbs_out -- --exact` → FAIL. **Revert.**

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/core/osinstall/apply.rs
git commit -m "SD-2 G5: build the distribution tree, and record what built it"
```

---

### Task 7: `S:User-Startup`, edited in place

**Files:**
- Create: `src-tauri/src/core/osinstall/startup.rs`

**Interfaces:**
- Produces: `pub fn merge_user_startup(existing: Option<&str>, component: &str, lines: &[String]) -> String`
- Consumed by: `apply.rs`, after the file items

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_is_added_with_the_convention_real_installers_use() {
        let out = merge_user_startup(None, "amissl", &["Assign AmiSSL: SYS:Libs/AmiSSL".into()]);
        assert_eq!(
            out,
            ";BEGIN amissl\nAssign AmiSSL: SYS:Libs/AmiSSL\n;END amissl\n"
        );
    }

    /// §39/§40: a hand-tuned file is edited, never regenerated.
    #[test]
    fn everything_the_user_wrote_survives_verbatim() {
        let existing = "; my own line\nAssign WORK: DH1:\n";
        let out = merge_user_startup(Some(existing), "amissl", &["Assign AmiSSL: SYS:Libs/AmiSSL".into()]);
        assert!(out.starts_with(existing), "the user's own lines come first and unchanged");
    }

    #[test]
    fn re_running_replaces_only_this_components_own_block() {
        let first = merge_user_startup(None, "amissl", &["one".into()]);
        let with_user = format!("{first}; something the user added later\n");
        let second = merge_user_startup(Some(&with_user), "amissl", &["two".into()]);

        assert!(second.contains("two"));
        assert!(!second.contains("one"));
        assert!(second.contains("; something the user added later"));
    }

    #[test]
    fn another_components_block_is_left_alone() {
        let a = merge_user_startup(None, "alpha", &["a".into()]);
        let both = merge_user_startup(Some(&a), "beta", &["b".into()]);
        let again = merge_user_startup(Some(&both), "beta", &["b2".into()]);

        assert!(again.contains(";BEGIN alpha\na\n;END alpha"));
        assert!(again.contains("b2"));
    }

    #[test]
    fn an_unterminated_block_is_left_alone_rather_than_swallowed() {
        let broken = ";BEGIN alpha\na\n; the END line is missing\n";
        let out = merge_user_startup(Some(broken), "beta", &["b".into()]);
        assert!(out.starts_with(broken), "ART does not repair a file it did not break");
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd src-tauri && cargo test osinstall::startup`
Expected: FAIL — `merge_user_startup` not defined.

- [ ] **Step 3: Implement**

Scan for `;BEGIN <component>` … `;END <component>`. Replace between them if both are found; append a new block otherwise. An opening marker with no matching close is **not** treated as a block — leave the text untouched and append, because guessing where somebody's unterminated block ends is how a file gets eaten.

- [ ] **Step 4: Run to verify they pass**

Run: `cd src-tauri && cargo test osinstall::startup`
Expected: PASS, all five.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/core/osinstall/startup.rs
git commit -m "SD-2 G5: User-Startup gets a block per component, edited in place"
```

---

### Task 8: `libpfs3`, and ART's journal underneath it

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `deny.toml`
- Modify: `THIRD_PARTY_LICENSES.md`
- Modify: `CLAUDE.md` (the core-dependency list)
- Create: `src-tauri/src/core/preload/pfs3dev.rs`

**Interfaces:**
- Produces: `pub struct ArtBlockDevice<D>(Mutex<D>)` implementing `libpfs3::io::BlockDevice` over any ART `BlockDeviceMut`

- [ ] **Step 1: Add the dependency and its paperwork, in one commit**

```toml
# Cargo.toml — PFS3, so ART can write the filesystem a real PiStorm card uses.
# Pure Rust: its whole dependency tree is byteorder + thiserror, it launches
# nothing and declares no OS dependency, which is what lets it live inside
# `core/`. LGPL-3.0-or-later, compatible with ART's GPL-3.0-or-later.
libpfs3 = "0.1.3"
```

`deny.toml`, in the `allow` list, with its reason beside the others:

```toml
  # `libpfs3` — the PFS3 implementation ART writes real PiStorm cards with.
  # LGPL-3.0 is compatible with ART's GPL-3.0-or-later. Noted against the
  # preference above: this is a weak-copyleft dependency inside `core/`, so a
  # future standalone `core` crate would carry it. Accepted deliberately —
  # the alternative was a second filesystem writer of ART's own (SD-4).
  "LGPL-3.0-or-later",
```

Add the row to `THIRD_PARTY_LICENSES.md` and the crate to CLAUDE.md's core-dependency sentence **in this same commit** — CLAUDE.md requires it.

Run: `cd src-tauri && cargo deny check`
Expected: ok.

- [ ] **Step 2: Write the failing adapter test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use libpfs3::io::BlockDevice as Pfs3Device;

    #[test]
    fn the_adapter_reads_and_writes_through_arts_own_device() {
        let mut backing = VecDevice::new(vec![0u8; 512 * 64], 512).unwrap();
        let device = ArtBlockDevice::new(&mut backing);

        device.write_block(3, &[0xAB; 512]).unwrap();
        let mut buf = [0u8; 512];
        device.read_block(3, &mut buf).unwrap();
        assert_eq!(buf, [0xAB; 512]);
    }

    #[test]
    fn a_block_past_the_end_is_an_error_not_a_short_write() {
        let mut backing = VecDevice::new(vec![0u8; 512 * 4], 512).unwrap();
        let device = ArtBlockDevice::new(&mut backing);
        assert!(device.write_block(9, &[0; 512]).is_err());
    }

    /// PFS3 has no journalling of its own — the format has none, and neither
    /// does the original AmigaOS driver. ART's journal is therefore the only
    /// crash safety a PFS3 write can have, which is the whole reason this
    /// adapter exists instead of libpfs3's own FileBlockDevice.
    #[test]
    fn a_write_through_a_journalled_device_is_journalled() {
        let mut journalled = journalled_device();
        {
            let device = ArtBlockDevice::new(&mut journalled);
            device.write_block(2, &[0x11; 512]).unwrap();
        }
        assert!(journalled.journal_holds(2), "block 2 was saved before being written");
    }

    #[test]
    fn a_block_number_beyond_u32_is_refused_rather_than_truncated() {
        let mut backing = VecDevice::new(vec![0u8; 512 * 4], 512).unwrap();
        let device = ArtBlockDevice::new(&mut backing);
        assert!(device.read_block(u64::from(u32::MAX) + 1, &mut [0u8; 512]).is_err());
    }
}
```

- [ ] **Step 3: Run to verify they fail**

Run: `cd src-tauri && cargo test preload::pfs3dev`
Expected: FAIL — `ArtBlockDevice` not defined.

- [ ] **Step 4: Implement the adapter**

```rust
//! Driving `libpfs3` through ART's own block device.
//!
//! The two traits are close: both `Send + Sync`, both block-addressed. Two
//! differences to bridge — libpfs3 writes through `&self` (its own
//! `FileBlockDevice` uses a `Mutex<File>`, and so do we), and it addresses in
//! `u64` where ART uses `u32`.
//!
//! **The point is not tidiness.** PFS3 has no journalling; using libpfs3's own
//! device would leave an interrupted install as an unknown volume. Through
//! ART's device, every PFS3 block write goes into `core/volume/journal.rs`:
//! a block that was not saved cannot be written, and a rollback restores the
//! image byte for byte.
//!
//! Limit, written down rather than discovered: ART's `total_blocks()` is
//! `u32`, so 2 TB at 512-byte blocks — far beyond any card.

pub struct ArtBlockDevice<'a, D: ?Sized> {
    inner: Mutex<&'a mut D>,
}
```

`write_blocks` / `read_blocks` loop over the single-block methods so the journal sees every block. `flush` maps to ART's own sync. Errors map to `libpfs3::error::Error::Io`.

- [ ] **Step 5: Run to verify they pass**

Run: `cd src-tauri && cargo test preload::pfs3dev`
Expected: PASS, all four.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock deny.toml THIRD_PARTY_LICENSES.md CLAUDE.md src-tauri/src/core/preload/pfs3dev.rs
git commit -m "SD-2 G5: PFS3 through ART's own block device, so the journal covers it"
```

---

### Task 9: A `VolumeFormatter` that launches nothing

**Files:**
- Create: `src-tauri/src/core/preload/native.rs`
- Modify: `src-tauri/src/core/preload/mod.rs` (`pub mod native; pub mod pfs3dev;`)

**Interfaces:**
- Consumes: `VolumeFormatter`, `ArtBlockDevice`, `core::card::build::create_rdb_layout`, `core::volume::write`
- Produces: `pub struct NativeFormatter;` implementing `VolumeFormatter`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_formats_a_pfs3_partition_and_reads_the_volume_name_back() {
        let image = rdb_image_with_one_pds3_partition();
        NativeFormatter
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();

        let vol = libpfs3::volume::Volume::open(&image, partition_offset(&image)).unwrap();
        assert_eq!(vol.name(), "Work");
    }

    #[test]
    fn it_formats_an_ffs_partition_with_arts_own_writer() {
        let image = rdb_image_with_one_dos3_partition();
        NativeFormatter
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        // ART reads FFS, so ART's own reader is the check here.
        let volumes = crate::core::volume::mount::scan_image(&image).unwrap();
        assert_eq!(volumes.volumes[0].name, "DH0");
    }

    /// The bit that makes an install boot. Written here, read back here — the
    /// *independent* check is `scripts/pfs3-oracle-check.py` in Task 11.
    #[test]
    fn copy_in_carries_the_protection_bits_out_of_the_uaem_sidecars() {
        let image = formatted_pds3_image();
        let tree = fixtures::scratch("<tag>");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C/Assign"), b"x").unwrap();
        std::fs::write(tree.join("C/Assign.uaem"), "--p-rwed 2021-04-13 02:43:13.68 \n").unwrap();

        NativeFormatter.copy_in(&image, None, "DH0", &tree, &NoProgress).unwrap();

        let mut vol = libpfs3::volume::Volume::open(&image, partition_offset(&image)).unwrap();
        let entry = vol.list_dir("C").unwrap().into_iter().find(|e| e.name == "Assign").unwrap();
        assert_eq!(libpfs3::util::amiga_protection_string(entry.protection), "--p-rwed");
    }

    #[test]
    fn a_sidecar_is_applied_and_never_copied_as_a_file_of_its_own() {
        // …as above, then:
        assert!(vol.list_dir("C").unwrap().iter().all(|e| !e.name.ends_with(".uaem")));
    }

    #[test]
    fn copy_in_reports_what_it_moved() {
        let summary = NativeFormatter.copy_in(&image, None, "DH0", &tree, &NoProgress).unwrap();
        assert_eq!(summary.files, 1);
        assert_eq!(summary.directories, 1);
    }

    #[test]
    fn it_stops_between_files_when_cancelled() {
        let err = NativeFormatter.copy_in(&image, None, "DH0", &tree, &CancelAfter::new(1)).unwrap_err();
        assert!(matches!(err, CoreError::Cancelled));
    }

    /// The fit question lives here, not in the plan: the tree carries no
    /// volume name (Decision 1), so only the write step knows how much room
    /// there is. Asked **before the first byte**, with real numbers.
    #[test]
    fn a_tree_too_big_for_the_volume_is_refused_before_anything_is_written() {
        let image = formatted_pds3_image_of(4); // MB
        let tree = tree_of_bytes(8 * 1024 * 1024);
        let before = std::fs::read(&image).unwrap();

        let err = NativeFormatter.copy_in(&image, None, "DH0", &tree, &NoProgress).unwrap_err();
        assert!(format!("{err}").contains("8"), "the refusal carries the real numbers");
        assert_eq!(std::fs::read(&image).unwrap(), before, "nothing was written");
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd src-tauri && cargo test preload::native`
Expected: FAIL — `NativeFormatter` not defined.

- [ ] **Step 3: Implement**

- `probe` reports `libpfs3`'s crate version as the `ToolVersion.raw`, so the report says which implementation did the work.
- `import_filesystem` delegates to the existing `create_rdb_layout`.
- `format_partition` branches on the partition's DosType: `PDS\3` → `libpfs3::format::format_with_size`; `DOS\0`…`DOS\7` → ART's own. **Validate the volume name with `check_name` first** — the preload screen already does, and the engine must not depend on a screen having asked.
- `copy_in` walks the host tree, applies each `.uaem` through `uaem::parse` (never copying a sidecar as a file), writes, then sets protection with `update_dir_entry_protection`. Cancel between whole files.

- [ ] **Step 4: Run to verify they pass**

Run: `cd src-tauri && cargo test preload::native`
Expected: PASS, all six.

- [ ] **Step 5: Mutation-check the bit transfer**

Skip the `update_dir_entry_protection` call.
Run: `cd src-tauri && cargo test copy_in_carries_the_protection_bits -- --exact` → FAIL. **Revert.**

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/core/preload/native.rs src-tauri/src/core/preload/mod.rs
git commit -m "SD-2 G5: a VolumeFormatter that launches nothing"
```

---

### Task 10: Read the volume back and check it against the manifest

**Files:**
- Create: `src-tauri/src/core/osinstall/verify.rs`

**Interfaces:**
- Consumes: `DistributionManifest`, `libpfs3::volume::Volume`, `core::volume::mount`
- Produces:
  ```rust
  pub enum CheckState { Pass, Fail, NotChecked }
  pub struct FileVerdict { pub path: String, pub state: CheckState, pub detail: Option<String> }
  pub struct VerifyReport { pub files: Vec<FileVerdict>,
                            pub passed: usize, pub failed: usize, pub not_checked: usize }
  pub fn verify_volume(image: &Path, slot: Option<usize>, index: usize,
                       manifest: &DistributionManifest) -> CoreResult<VerifyReport>;
  ```

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_in_the_manifest_is_found_with_its_size_and_its_bits() {
        let (image, manifest) = written_volume();
        let report = verify_volume(&image, None, 1, &manifest).unwrap();
        assert_eq!(report.failed, 0);
        assert_eq!(report.passed, manifest.files.len());
    }

    #[test]
    fn a_missing_file_is_a_fail_and_says_which_one() {
        let (image, mut manifest) = written_volume();
        manifest.files.push(FileRecord {
            path: "C/NeverWritten".into(), component: "workbench-base".into(),
            media: "Workbench3.2".into(), sha256: "0".repeat(64), bytes: 4,
        });
        let report = verify_volume(&image, None, 1, &manifest).unwrap();
        assert_eq!(report.failed, 1);
        assert!(report.files.iter().any(|f| f.path == "C/NeverWritten"
                                        && f.state == CheckState::Fail));
    }

    /// The pure bit is why this check exists at all.
    #[test]
    fn a_file_whose_protection_bits_are_wrong_is_a_fail_not_a_pass() {
        let (image, manifest) = written_volume_with_the_pure_bit_dropped();
        let report = verify_volume(&image, None, 1, &manifest).unwrap();
        assert!(report.files.iter().any(|f| f.path == "C/LoadModule"
                                        && f.state == CheckState::Fail));
    }

    /// §89 and G8's three states. ART reads the volume, not the file's bytes
    /// against their recorded hash on a volume it cannot fully re-read — and
    /// what it did not look at must never render as a tick.
    #[test]
    fn what_was_not_checked_is_its_own_state_and_never_a_pass() {
        let (image, manifest) = written_volume();
        let report = verify_volume(&image, None, 1, &manifest).unwrap();
        assert_eq!(report.passed + report.failed + report.not_checked,
                   report.files.len(),
                   "every file lands in exactly one of the three states");
        assert!(report.files.iter().all(|f| f.state != CheckState::NotChecked
                                        || f.detail.is_some()),
                "a not-checked verdict has to say why");
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd src-tauri && cargo test osinstall::verify`
Expected: FAIL — `verify_volume` not defined.

- [ ] **Step 3: Implement**

Open the volume by DosType — `libpfs3` for `PDS\3`, ART's own reader for FFS. For each manifest record, look the path up, compare size and protection. **Three states, kept apart**: a file whose bytes could not be re-read (because the reader offers no content for it) is `NotChecked` with a reason, never `Pass`. `VerifyReport` is what `commands/osinstall.rs` puts in the oplog entry's `verified` field.

- [ ] **Step 4: Run to verify they pass**

Run: `cd src-tauri && cargo test osinstall::verify`
Expected: PASS, all four.

- [ ] **Step 5: Mutation-check the three states**

Make `NotChecked` count towards `passed`.
Run: `cd src-tauri && cargo test what_was_not_checked_is_its_own_state -- --exact` → FAIL. **Revert.**

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/core/osinstall/verify.rs
git commit -m "SD-2 G5: read the volume back, and keep 'did not look' apart from 'fine'"
```

---

### Task 11: The PFS3 oracle, both directions

**Files:**
- Create: `scripts/pfs3-oracle-check.py`
- Modify: `docs/STATUS.md` (the reproduce-the-numbers block)

**Interfaces:** none — a standalone script, like `fat-oracle-check.py`.

- [ ] **Step 1: Write the script**

Two directions, because a reader and a writer that agree only with each other is the shape of ART-032 … ART-035, ART-075 and ART-079:

1. **ART writes, `hst-imager` reads.** A `cargo test build_pfs3_volume_for_oracle_when_asked -- --nocapture` run behind an env var builds a volume with `NativeFormatter`; the script runs `hst.imager fs dir <image>\rdb\dh0 -r` and compares every name, size and attribute string against a JSON the test emitted.
2. **`hst-imager` writes, ART reads.** The script formats and fills a volume with `hst.imager`, then `cargo test read_foreign_pfs3_for_oracle_when_asked` prints a **SHA-256 per entry** — a hash, not a length, because ART-079's defect gave every file exactly the right length.

Environment: `ART_HST_IMAGER` for the binary, `ART_SCRATCH` for the working directory (defaulting to `E:\amiga\ProjeART`).

- [ ] **Step 2: Run it against real material**

Run: `python scripts/pfs3-oracle-check.py`
Expected: both directions clean, including the protection bits.

- [ ] **Step 3: Record it honestly in STATUS**

Add it to the reproduce block **marked local, not CI** — the runner has no `hst.imager.exe`, exactly like `fat-oracle-check.py` and `iso-oracle-check.py`. Nobody should read "there is an oracle" as "CI runs it".

- [ ] **Step 4: Commit**

```bash
git add scripts/pfs3-oracle-check.py docs/STATUS.md
git commit -m "SD-2 G5: an independent witness for the PFS3 writer, both ways"
```

---

### Task 12: Commands, the typed wrapper, and the wire between them

**Files:**
- Create: `src-tauri/src/commands/osinstall.rs`
- Create: `src/lib/osinstall.ts`
- Modify: `src-tauri/src/lib.rs` (`invoke_handler![]`)

**Interfaces:**
- Produces: `osinstall_scan_media`, `osinstall_plan`, `osinstall_apply` (a job)

- [ ] **Step 1: Write the commands as thin adapters**

Deserialize, call core, serialize back — nothing else. `osinstall_apply` goes through `spawn_job` and through `write_result` in `commands/oplog.rs`, following `commands/adf.rs`'s shape. **`osinstall_apply` takes the plan it is given** rather than recomputing it, the way `layout_apply` does and `preload_run` does not — because the user's component choices *are* the plan.

A fourth command, `osinstall_verify`, wraps Task 10's `verify_volume` and returns the `VerifyReport`. The oplog entry's `verified` field is `report.failed == 0 && report.not_checked == 0` — **not** `report.failed == 0`, because "ART did not look" is not "ART found nothing wrong" (§89). The screen shows all three counts.

- [ ] **Step 2: Write the typed wrapper**

`src/lib/osinstall.ts` mirrors the Rust types. No component ever calls `invoke` directly.

- [ ] **Step 3: Pin the wire in Rust**

The one thing a thin adapter can get wrong — a renamed field compiles on both sides and fails under the user's finger:

```rust
#[test]
fn the_payload_the_frontend_sends_deserialises() {
    let json = r#"{
        "mediaFolder": "E:\\media",
        "rom": "E:\\kick.rom",
        "chosen": ["workbench-base", "extras"],
        "destination": "E:\\dist"
    }"#;
    let request: InstallRequest = serde_json::from_str(json).unwrap();
    assert_eq!(request.chosen.len(), 2);
}
```

- [ ] **Step 4: Run everything**

Run: `pnpm lint && pnpm test && cd src-tauri && cargo test`
Expected: PASS.

- [ ] **Step 5: Mutation-check the wire**

Rename `chosen` to `components` in the Rust struct only.
Run: `cd src-tauri && cargo test the_payload_the_frontend_sends_deserialises -- --exact` → FAIL. **Revert.**

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/osinstall.rs src-tauri/src/lib.rs src/lib/osinstall.ts
git commit -m "SD-2 G5: the commands, and the wire pinned in Rust"
```

---

### Task 13: The screen, inside the OS Builder

**Files:**
- Create: `src/components/osbuilder/OsInstall.tsx`
- Modify: `src/pages/OsBuilder.tsx`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json`

- [ ] **Step 1: Build the screen**

The OS Builder's second kind, beside the boot-only card. Flow: media folder → ROM → the component list → preview → confirm → job → report.

Three things it must get right:

- **Every conditional tick states its reason on screen.** "on, because the paired Kickstart is 3.1 (40.68) and this release's Startup-Sequence quits below V47." A tick ART decided and did not explain is a tick the user cannot argue with.
- **Turning Modules off on a pre-V47 ROM is a confirmation, not a refusal** — and the confirmation says what will happen, which is that the machine will not boot.
- **The file list is shown in full and is read-only.** Components are the edit. Unlike G11, where retargeting a row *is* the feature, a hand-moved file here would make `distribution.json` describe something that is not a release.

Remembered between runs (`@/lib/remembered`, through a guard): the media folder, the ROM, the destination, and the component selection. **Nothing is remembered that arms a destructive action** — this screen writes a new tree and refuses an existing one, so there is nothing here of the shape the preload screen's ticks had.

- [ ] **Step 2: Add every key to both catalogues**

Run: `pnpm test`
Expected: PASS — the parity test fails the build if a key, a value or an interpolation variable is missing from either.

- [ ] **Step 3: Mount it in a real browser against the real bundle**

Every string resolves, no raw key and no `{{variable}}` on screen. This is the pass that caught G11's drawer dropdown leaking a policy field, which no test could.

- [ ] **Step 4: Run everything twice**

Run: `pnpm lint && pnpm test && cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo test`

- [ ] **Step 5: Commit**

```bash
git add src/components/osbuilder/OsInstall.tsx src/pages/OsBuilder.tsx src/i18n/en.json src/i18n/tr.json
git commit -m "SD-2 G5: the OS Builder can install an OS"
```

---

### Task 14: Drive it against the real 3.2 set, and write down what happened

**Files:**
- Modify: `docs/STATUS.md`, `docs/FEATURES.md`, `docs/ISSUES.md`, `CHANGELOG.md`

- [ ] **Step 1: Run the whole engine against the user's own media**

`E:\amiga\Amigatolon\paketler\3.2\AmigaOs 3.2\ADF\` (36 ADFs), with
`E:\amiga\Amigatolon\kickstart\Kickstart v3.1 rev 40.68 (1993)(Commodore)(A1200).rom`,
into `E:\amiga\ProjeART\dist-3.2\`.

Expect the Modules component **on** without being chosen, and its reason stated. This is the run that finds what the fixtures could not — every previous session that skipped it paid for it.

- [ ] **Step 2: Put the tree on a volume and read it back with something that is not ART**

Onto a copy of the card, never the original. Then `hst.imager fs dir` as the independent witness, and `python scripts/pfs3-oracle-check.py`.

- [ ] **Step 3: Update the four documents, claiming only what was run**

- `FEATURES.md`: flip the row **only if a test exists**.
- `STATUS.md`: the session line and the snapshot. The honest sentence for this one is *"a distribution tree was built from the user's own 3.2 media and put onto a volume; no Amiga has booted it"* — the WinUAE and hardware rungs stay open until they are actually climbed.
- `ISSUES.md`: an `ART-NNN` for anything found and not fixed. Expect some; the real media has surprised this project every single time.
- `CHANGELOG.md`: the user-visible change.

- [ ] **Step 4: Commit**

```bash
git add docs CHANGELOG.md
git commit -m "SD-2 G5 lands: AmigaOS installed from the user's own media"
```

---

## What this plan does not build

Named so they are not mistaken for oversights. Each is registered `available: false` or filed, never hidden:

- **AmigaOS 3.9 / an ISO source.** `MediaSource` has the slot; the recipe does not exist.
- **The 3.2.1 and 3.2.2 updates.** Registered, not implemented.
- **Arbitrary package add and remove.** The next piece of work, and what `distribution.json` exists to make possible.
- **Editing an existing distribution** (CaffeineOS, AmiKit). Its wall came down with `libpfs3`, but its honesty problem did not: ART did not build those cards, so it has no manifest for them and can remove only what it itself added.
- **Multiboot as several complete environments.** G16, SD-3.
