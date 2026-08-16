# ROM pairing (SD-2 · G9) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tell the user, before a partition is erased, whether the Kickstart on
the card suits the system volume about to be written to it.

**Architecture:** Each side records what it already knows — the distribution
tree records the ROM it was planned for and whether it carries its own ROM
modules; the card manifest records which boot file is its Kickstart — and one
pure function in `core/rom` compares them. Nothing is written, nothing is
refused: the preload screen shows the verdict above its confirmation checkbox.

**Tech Stack:** Rust (`core/osinstall`, `core/card`, `core/rom`,
`commands/`), TypeScript + React (`src/lib/preload.ts`,
`components/osbuilder/VolumePreload.tsx`), i18next (`src/i18n/en.json`,
`tr.json`).

**Spec:** [../specs/2026-08-17-rom-pairing-design.md](../specs/2026-08-17-rom-pairing-design.md)

## Global Constraints

- `core/` is platform-independent: `std` + `serde` + the crates CLAUDE.md
  lists. **No `use tauri`, no network, no launching programs.** The comparison
  in Task 3 is pure and takes values, never paths.
- Every string the user reads is a `Phrase { key, params? }` from `src/lib`,
  rendered by the component through `t()`. Rust returns typed values, never
  English sentences (ART-060).
- A key added to `src/i18n/en.json` is added to `tr.json` **in the same
  commit**; `pnpm test` fails the build otherwise.
- Every new `Phrase`-returning mapper is enumerated in
  `src/i18n/phrase-keys.test.ts`, and any new non-literal `t(…)` call site
  moves the count in `src/i18n/literal-keys.test.ts` **with a comment saying
  why**.
- A missing answer is never rendered as a pass (§89, G8's `not-checked`
  precedent).
- Serde: every new field on an existing serialised struct carries
  `#[serde(default)]`, so a manifest written by an older ART still
  deserialises.
- Run `cargo fmt` before every commit; `cargo clippy --all-targets -- -D
  warnings` must stay silent.

---

### Task 1: The tree records the ROM it was planned for

**Files:**
- Modify: `src-tauri/src/core/osinstall/mod.rs` (add `PairedRom` beside
  `Condition`)
- Modify: `src-tauri/src/core/osinstall/plan.rs` (`rom_facts`, `InstallPlan`,
  `plan()`)
- Modify: `src-tauri/src/core/osinstall/apply.rs` (`DistributionManifest`)
- Test: the same three files' own `mod tests`

**Interfaces:**
- Consumes: `crate::core::rom::identify_rom`, `RomInfo` (fields `name`,
  `sha256`, `compatible_models`, `version`, `revision`), and
  `crate::core::rom::stated_version`.
- Produces:
  - `core::osinstall::PairedRom { name: String, sha256: String, stated_major: Option<u16>, compatible_models: Vec<String>, requires_major: Option<u16> }`
  - `InstallPlan.paired_rom: Option<PairedRom>`
  - `DistributionManifest.paired_rom: Option<PairedRom>`

- [ ] **Step 1: Write the failing test for the record itself**

In `src-tauri/src/core/osinstall/plan.rs`, inside `mod plan_tests`:

```rust
    /// **G9.** A tree is planned against one Kickstart, and which one decides
    /// what is in it — `modules-a1200` switches on for a pre-V47 ROM and not
    /// otherwise. The plan records that pairing so the check at card time
    /// needs no re-planning and no media.
    #[test]
    fn the_plan_records_the_rom_it_was_planned_against() {
        let (plan, dir) = crate::core::osinstall::fixtures::planned_with(
            &["workbench-base"],
            &["Workbench3.2"],
            Some(47),
        );
        let paired = plan.paired_rom.expect("a plan with a ROM records it");

        assert_eq!(paired.stated_major, Some(47));
        assert_eq!(
            paired.requires_major, None,
            "a V47 ROM satisfies the recipe's condition, so the tree carries \
             no ROM modules and requires nothing of a future ROM"
        );
        assert!(!paired.sha256.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The other half, and the load-bearing one: a tree built for an older
    /// ROM carries the modules that let an older ROM run it, so it requires
    /// nothing — `requires_major` is `None` for the opposite reason.
    #[test]
    fn a_tree_built_for_a_pre_v47_rom_requires_nothing_of_the_card() {
        let (plan, dir) = crate::core::osinstall::fixtures::planned_with(
            &["workbench-base"],
            &["Workbench3.2", "ModulesA1200_3.2"],
            Some(40),
        );
        assert!(plan.components_on.iter().any(|id| id == "modules-a1200"));
        let paired = plan.paired_rom.expect("a plan with a ROM records it");

        assert_eq!(paired.stated_major, Some(40));
        assert_eq!(paired.requires_major, None, "it brings its own modules");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// And the case the check exists for: a V47-planned tree whose modules
    /// component is *not* on states what a future ROM has to be.
    #[test]
    fn a_tree_planned_on_v47_without_modules_requires_v47() {
        let (plan, dir) = crate::core::osinstall::fixtures::planned_with(
            &["workbench-base"],
            &["Workbench3.2", "ModulesA1200_3.2"],
            Some(47),
        );
        assert!(!plan.components_on.iter().any(|id| id == "modules-a1200"));
        let paired = plan.paired_rom.unwrap();
        assert_eq!(paired.requires_major, Some(47));

        let _ = std::fs::remove_dir_all(&dir);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd src-tauri && cargo test --lib the_plan_records_the_rom_it_was_planned_against
```

Expected: FAIL — `no field 'paired_rom' on type 'InstallPlan'`.

- [ ] **Step 3: Add the record type**

In `src-tauri/src/core/osinstall/mod.rs`, after the `Condition` enum:

```rust
/// The Kickstart a distribution tree was planned against, and what it needs
/// of a future one (SD-2 · G9).
///
/// **Recorded rather than recomputed.** Which components a plan switches on
/// depends on the ROM it was given — `modules-a1200` is on for a pre-V47 ROM
/// and off otherwise — so a tree carries a fact about a file that may be
/// nowhere by the time somebody puts the tree on a card. Re-planning to
/// recover it would need the original media, which is exactly what is gone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedRom {
    /// As `core::rom::identify_rom` names it: `Kickstart 40.68 (A1200)`.
    pub name: String,
    /// Of the ROM image, decoded — so a licensed Amiga Forever dump and a
    /// bare dump of the same ROM hash alike (ART-128).
    pub sha256: String,
    /// What the ROM states about itself. `None` for pre-2.0 ROMs, which
    /// state nothing at all.
    pub stated_major: Option<u16>,
    pub compatible_models: Vec<String>,
    /// **The load-bearing field.** `Some(47)` when this tree needs a ROM of
    /// at least that major to start; `None` when it needs nothing, either
    /// because it carries its own ROM modules or because no component in it
    /// ever depended on the ROM.
    ///
    /// Taken from the recipe's own `Condition::RomOlderThan`, so the
    /// threshold is not written down a second time.
    pub requires_major: Option<u16>,
}
```

- [ ] **Step 4: Fill it in when planning**

In `src-tauri/src/core/osinstall/plan.rs`, replace `rom_facts` with a version
that also answers identity, and add the field:

```rust
/// Read the paired Kickstart's own stated major.
///
/// Goes through `core::rom::identify_rom`, which decodes a licensed Amiga
/// Forever ROM with the `rom.key` beside it (ART-128) instead of describing
/// its ciphertext — the previous version stripped the header and read the
/// encrypted bytes, so a licensed ROM refused the whole plan.
pub fn rom_facts(rom: &Path) -> CoreResult<RomFacts> {
    let info = crate::core::rom::identify_rom(rom)?;
    let bytes = crate::core::rom::decoded_image(rom)?;
    let (major, _minor) = crate::core::rom::stated_version(&bytes).ok_or_else(|| {
        CoreError::InvalidInput("this file does not state a Kickstart version".into())
    })?;
    Ok(RomFacts { major, info })
}
```

and widen `RomFacts`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RomFacts {
    pub major: u16,
    /// Kept whole so `plan()` can record the pairing without reading the file
    /// twice — and so a future condition can ask about the machine.
    pub info: crate::core::rom::RomInfo,
}
```

Add to `InstallPlan`, after `components_on`:

```rust
    /// The Kickstart this plan was made against, and what the resulting tree
    /// needs of a future one (G9). `None` when no ROM was supplied.
    pub paired_rom: Option<PairedRom>,
```

And in `plan()`, where the `InstallPlan` is constructed, build it from the
facts already in hand:

```rust
    let paired_rom = rom_facts.map(|facts| PairedRom {
        name: facts.info.name.clone(),
        sha256: facts.info.sha256.clone(),
        stated_major: Some(facts.major),
        compatible_models: facts.info.compatible_models.clone(),
        requires_major: rom_requirement(recipe, &components_on),
    });
```

with, beside it in the same file:

```rust
/// What a tree with these components needs of a future ROM.
///
/// A component with a `RomOlderThan` condition that is **off** is one whose
/// modules are absent, so the tree needs a ROM the condition would not have
/// fired for: at least `major`. A component that is *on* brought its modules
/// with it and needs nothing.
fn rom_requirement(recipe: &Recipe, components_on: &[String]) -> Option<u16> {
    recipe
        .components
        .iter()
        .filter_map(|component| match component.condition {
            Some(Condition::RomOlderThan { major }) => Some((component.id.as_str(), major)),
            None => None,
        })
        .filter(|(id, _)| !components_on.iter().any(|on| on == id))
        .map(|(_, major)| major)
        .max()
}
```

- [ ] **Step 5: Add `decoded_image` to `core/rom`**

In `src-tauri/src/core/rom/mod.rs`, beside `key_beside`:

```rust
/// A ROM's bytes as an Amiga would read them: the header gone and the image
/// decoded when it is a licensed Amiga Forever dump with its key beside it
/// (ART-128), and the file as-is otherwise.
///
/// The one place that answers "what is actually in this ROM", so nothing has
/// to repeat the header-and-key dance to read a version out of one.
pub fn decoded_image(path: &Path) -> CoreResult<Vec<u8>> {
    let raw = std::fs::read(path)?;
    if !raw.starts_with(CLOANTO_HEADER) {
        return Ok(raw);
    }
    match key_beside(path) {
        Some(key) => Ok(decode_cloanto(&strip_cloanto_header(&raw), &key)),
        None => Err(CoreError::InvalidInput(format!(
            "'{}' is an encrypted Amiga Forever ROM and its 'rom.key' is not beside it",
            path.display()
        ))),
    }
}
```

- [ ] **Step 6: Carry it into the tree's own manifest**

In `src-tauri/src/core/osinstall/apply.rs`, add to `DistributionManifest`:

```rust
    /// The Kickstart this tree was built for (G9). `#[serde(default)]` so a
    /// tree written by an older ART still reads back.
    #[serde(default)]
    pub paired_rom: Option<PairedRom>,
```

and where the manifest is constructed, `paired_rom: plan.paired_rom.clone(),`.

- [ ] **Step 7: Run the tests**

```bash
cd src-tauri && cargo test --lib osinstall
```

Expected: PASS, including the three new tests. Fix any other `InstallPlan`
literal the compiler names by adding `paired_rom: None`.

- [ ] **Step 8: Prove the Cloanto half of `rom_facts`**

In `plan.rs`'s tests:

```rust
    /// **ART-128, from the planning side.** `rom_facts` used to strip the
    /// header and read the ciphertext, so a licensed Amiga Forever ROM
    /// refused the whole install with "does not state a Kickstart version".
    #[test]
    fn a_licensed_rom_with_its_key_beside_it_states_its_version() {
        let dir = crate::core::osinstall::fixtures::scratch("rom-facts-cloanto");
        let key = b"a key".to_vec();
        std::fs::write(dir.join("rom.key"), &key).unwrap();

        let mut plain = vec![0u8; 524_288];
        plain[0..2].copy_from_slice(&0x1114u16.to_be_bytes());
        plain[12..14].copy_from_slice(&47u16.to_be_bytes());
        plain[14..16].copy_from_slice(&102u16.to_be_bytes());

        let mut encoded = b"AMIROMTYPE1".to_vec();
        encoded.extend(
            plain
                .iter()
                .enumerate()
                .map(|(at, byte)| byte ^ key[at % key.len()]),
        );
        let path = dir.join("amiga-os-321-a1200.rom");
        std::fs::write(&path, &encoded).unwrap();

        assert_eq!(rom_facts(&path).unwrap().major, 47);

        let _ = std::fs::remove_dir_all(&dir);
    }
```

- [ ] **Step 9: Run it**

```bash
cd src-tauri && cargo test --lib a_licensed_rom_with_its_key_beside_it_states_its_version
```

Expected: PASS.

- [ ] **Step 10: Commit**

```bash
cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test
git add src-tauri/src/core/osinstall src-tauri/src/core/rom
git commit -m "The distribution tree records the Kickstart it was planned for (G9)"
```

---

### Task 2: The card manifest says which boot file is its Kickstart

**Files:**
- Modify: `src-tauri/src/core/card/manifest.rs` (`SourceFacts`)
- Modify: `src-tauri/src/commands/card.rs` (`source_facts`)
- Test: both files' own `mod tests`

**Interfaces:**
- Consumes: `CardBuildRequest.firmware` (a `FirmwareConfig`, whose
  `kickstart_file: String` is the name written on the card).
- Produces: `SourceFacts.kickstart_file: Option<String>` — the **on-card**
  file name, which is what lets a reader find the ROM among `boot_files`.

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/core/card/manifest.rs`'s tests:

```rust
    /// **G9.** A manifest lists the boot files and, until now, gave no way to
    /// tell which of them is the Kickstart: `kickstart_name` is the *source*
    /// file's name on the user's PC, and after ART-128 not even the same
    /// bytes. The on-card name is what a reader needs.
    #[test]
    fn the_manifest_says_which_boot_file_is_the_kickstart() {
        let mut facts = source();
        facts.kickstart_file = Some("kick.rom".into());
        let rendered = render_manifest(&CardManifest {
            source: facts,
            ..manifest()
        })
        .unwrap();
        let read: CardManifest = serde_json::from_str(&rendered).unwrap();

        assert_eq!(read.source.kickstart_file.as_deref(), Some("kick.rom"));
    }

    /// A manifest written before this field existed still reads.
    #[test]
    fn an_older_manifest_without_the_field_still_reads() {
        let mut value = serde_json::to_value(manifest()).unwrap();
        value["source"]
            .as_object_mut()
            .unwrap()
            .remove("kickstart_file");
        let read: CardManifest = serde_json::from_value(value).unwrap();

        assert_eq!(read.source.kickstart_file, None);
    }
```

If `mod tests` has no `manifest()` helper, add one beside `source()`:

```rust
    fn manifest() -> CardManifest {
        CardManifest {
            art_version: env!("CARGO_PKG_VERSION").into(),
            built_at: None,
            total_bytes: 2 * 1024 * 1024 * 1024,
            mbr_sha256: "c".repeat(64),
            slots: Vec::new(),
            source: source(),
            boot_files: vec![ManifestFile {
                name: "kick.rom".into(),
                bytes: 524_288,
                sha256: "d".repeat(64),
            }],
        }
    }
```

If `CardManifest` has fields this literal does not name, the compiler will say
so — add them from the struct rather than guessing.

- [ ] **Step 2: Run it**

```bash
cd src-tauri && cargo test --lib the_manifest_says_which_boot_file_is_the_kickstart
```

Expected: FAIL — `no field 'kickstart_file' on type 'SourceFacts'`.

- [ ] **Step 3: Add the field**

In `SourceFacts`, after `kickstart_sha256`:

```rust
    /// The name the Kickstart is written under **on the card** — what
    /// `config.txt`'s `initramfs` line points at. `kickstart_name` above is
    /// the source file's own name and `kickstart_sha256` the source file's
    /// hash; since ART-128 neither describes the bytes on the card, because a
    /// licensed Amiga Forever ROM is decoded on the way there. This field is
    /// how a reader finds the ROM among `boot_files`, whose hashes *are* of
    /// the bytes placed.
    ///
    /// `None` on a manifest written before this existed.
    #[serde(default)]
    pub kickstart_file: Option<String>,
    /// The major version that Kickstart states about itself, read once at
    /// build time.
    ///
    /// **Recorded because it cannot be recovered.** ART writes FAT32 and has
    /// no reader for it, so once the card exists the ROM's own bytes are out
    /// of reach — the same reason G8 answers the boot files from the manifest
    /// rather than by looking. `None` when the ROM states none (pre-2.0) or
    /// when the manifest predates this field.
    #[serde(default)]
    pub kickstart_stated_major: Option<u16>,
```

- [ ] **Step 4: Fill both in**

In `src-tauri/src/commands/card.rs::source_facts`, add:

```rust
        kickstart_file: kickstart
            .as_ref()
            .map(|_| request.firmware.kickstart_file.clone()),
        kickstart_stated_major: match &kickstart {
            Some(path) => crate::core::rom::stated_version(
                &crate::core::rom::decoded_image(path)?,
            )
            .map(|(major, _minor)| major),
            None => None,
        },
```

`decoded_image` comes from Task 1, Step 5 — a licensed Amiga Forever ROM
states its version only once decoded.

- [ ] **Step 5: Run the tests**

```bash
cd src-tauri && cargo test --lib card
```

Expected: PASS. Add `kickstart_file: None` to any other `SourceFacts` literal
the compiler names.

- [ ] **Step 6: Commit**

```bash
cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings
git add src-tauri/src/core/card/manifest.rs src-tauri/src/commands/card.rs
git commit -m "A card manifest names its own Kickstart file (G9)"
```

---

### Task 3: The comparison

**Files:**
- Create: `src-tauri/src/core/rom/pairing.rs`
- Modify: `src-tauri/src/core/rom/mod.rs` (`pub mod pairing;`)
- Test: `src-tauri/src/core/rom/pairing.rs`'s own `mod tests`

**Interfaces:**
- Consumes: `core::osinstall::PairedRom` (Task 1).
- Produces:
  - `core::rom::pairing::CardRom { name: String, sha256: String, stated_major: Option<u16> }`
  - `core::rom::pairing::Pairing` (serialisable, `#[serde(tag = "verdict", rename_all = "kebab-case")]`)
  - `core::rom::pairing::compare(tree: Option<&PairedRom>, card: Option<&CardRom>) -> Pairing`

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/core/rom/pairing.rs` with the tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn tree(sha: &str, requires: Option<u16>) -> PairedRom {
        PairedRom {
            name: "Kickstart 47.102 (A1200)".into(),
            sha256: sha.into(),
            stated_major: Some(47),
            compatible_models: vec!["A1200".into()],
            requires_major: requires,
        }
    }

    fn card(sha: &str, major: Option<u16>) -> CardRom {
        CardRom {
            name: "kick.rom".into(),
            sha256: sha.into(),
            stated_major: major,
        }
    }

    #[test]
    fn the_same_rom_is_paired_and_says_nothing() {
        let verdict = compare(Some(&tree("aa", Some(47))), Some(&card("aa", Some(47))));
        assert!(matches!(verdict, Pairing::Paired));
    }

    #[test]
    fn a_tree_that_carries_its_modules_suits_any_rom() {
        let verdict = compare(Some(&tree("aa", None)), Some(&card("bb", Some(40))));
        assert!(matches!(verdict, Pairing::Suitable { .. }), "{verdict:?}");
    }

    #[test]
    fn a_newer_rom_than_required_suits() {
        let verdict = compare(Some(&tree("aa", Some(47))), Some(&card("bb", Some(47))));
        assert!(matches!(verdict, Pairing::Suitable { .. }), "{verdict:?}");
    }

    /// The pairing that failed on 2026-08-16: a V47 tree, a V40 card.
    #[test]
    fn an_older_rom_than_required_is_unsuitable_and_says_both_numbers() {
        let verdict = compare(Some(&tree("aa", Some(47))), Some(&card("bb", Some(40))));
        match verdict {
            Pairing::Unsuitable { needs, found, .. } => {
                assert_eq!(needs, 47);
                assert_eq!(found, Some(40));
            }
            other => panic!("{other:?}"),
        }
    }

    /// A ROM that states nothing cannot be the V47 the tree needs.
    #[test]
    fn a_rom_that_states_no_version_cannot_satisfy_a_requirement() {
        let verdict = compare(Some(&tree("aa", Some(47))), Some(&card("bb", None)));
        assert!(matches!(verdict, Pairing::Unsuitable { found: None, .. }));
    }

    /// A missing answer is a missing answer, never a pass (§89).
    #[test]
    fn a_missing_side_is_not_checked_rather_than_paired() {
        assert!(matches!(
            compare(None, Some(&card("aa", Some(47)))),
            Pairing::NotChecked { .. }
        ));
        assert!(matches!(
            compare(Some(&tree("aa", Some(47))), None),
            Pairing::NotChecked { .. }
        ));
    }
}
```

- [ ] **Step 2: Run them**

```bash
cd src-tauri && cargo test --lib pairing
```

Expected: FAIL — the module does not compile; `compare` does not exist.

- [ ] **Step 3: Write the implementation above the tests**

```rust
//! Does the Kickstart on this card suit the volume about to be written to it?
//! (SD-2 · G9)
//!
//! **A check, not an object.** Both sides already record what they know — the
//! tree its planning ROM (`core::osinstall::PairedRom`), the card its boot
//! files and which of them is the Kickstart — and this compares them. It reads
//! no files, launches nothing and decides nothing: the caller renders the
//! verdict beside a confirmation, and the user chooses.
//!
//! It does **not** ask "is this the same ROM". A different Kickstart is
//! perfectly ordinary; the question is whether the tree's own requirement —
//! recorded at plan time from the recipe's `Condition::RomOlderThan` — still
//! holds against this one.

use serde::{Deserialize, Serialize};

use crate::core::osinstall::PairedRom;

/// The Kickstart a card carries, as its manifest describes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardRom {
    /// The on-card file name, from `SourceFacts::kickstart_file`.
    pub name: String,
    /// Of the bytes placed in the boot partition, from the manifest's
    /// `boot_files` entry — not of the source file (ART-128).
    pub sha256: String,
    /// What that ROM states about itself, when the caller could read it.
    pub stated_major: Option<u16>,
}

/// What ART can say about a tree and a card put together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "kebab-case")]
pub enum Pairing {
    /// The same ROM the tree was planned against. Nothing to report.
    Paired,
    /// A different ROM, and the tree's requirement holds against it.
    Suitable { rom: String },
    /// The tree needs a newer Kickstart than the card carries.
    Unsuitable {
        needs: u16,
        /// `None` when the card's ROM states no version at all.
        found: Option<u16>,
        rom: String,
    },
    /// One of the two sides did not answer. **Never rendered as a pass.**
    NotChecked { why: NotCheckedReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NotCheckedReason {
    /// No `distribution.json`, or one written before ART recorded this.
    TreeRecordsNoRom,
    /// No manifest beside the card, no Kickstart in it, or a manifest written
    /// before it named its own Kickstart file.
    CardRecordsNoRom,
}

/// Compare what the tree was planned for against what the card carries.
pub fn compare(tree: Option<&PairedRom>, card: Option<&CardRom>) -> Pairing {
    let Some(tree) = tree else {
        return Pairing::NotChecked {
            why: NotCheckedReason::TreeRecordsNoRom,
        };
    };
    let Some(card) = card else {
        return Pairing::NotChecked {
            why: NotCheckedReason::CardRecordsNoRom,
        };
    };

    if !tree.sha256.is_empty() && tree.sha256.eq_ignore_ascii_case(&card.sha256) {
        return Pairing::Paired;
    }

    match tree.requires_major {
        // The tree carries its own ROM modules, or never depended on the ROM
        // at all. See the design's "a floor nobody has measured": the recipe
        // states no lower bound, so neither does this.
        None => Pairing::Suitable {
            rom: card.name.clone(),
        },
        Some(needs) => match card.stated_major {
            Some(found) if found >= needs => Pairing::Suitable {
                rom: card.name.clone(),
            },
            found => Pairing::Unsuitable {
                needs,
                found,
                rom: card.name.clone(),
            },
        },
    }
}
```

Add `pub mod pairing;` to `src-tauri/src/core/rom/mod.rs`, beside
`pub mod remus;`.

- [ ] **Step 4: Run the tests**

```bash
cd src-tauri && cargo test --lib pairing
```

Expected: PASS, all six.

- [ ] **Step 5: Mutation-check the load-bearing branch**

Temporarily change `Some(found) if found >= needs` to `Some(_)` and re-run:
`an_older_rom_than_required_is_unsuitable_and_says_both_numbers` must fail.
Revert.

- [ ] **Step 6: Commit**

```bash
cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings
git add src-tauri/src/core/rom
git commit -m "Compare a tree's Kickstart requirement against a card's ROM (G9)"
```

---

### Task 4: Ask it at the moment the two meet

**Files:**
- Modify: `src-tauri/src/commands/preload.rs` (new command)
- Modify: `src-tauri/src/lib.rs` (`invoke_handler![]`)
- Modify: `src/lib/preload.ts` (typed wrapper, types, `pairingPhrase`)
- Modify: `src/components/osbuilder/VolumePreload.tsx`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json`
- Test: `src-tauri/src/commands/preload.rs`, `src/lib/preload.test.ts`,
  `src/i18n/phrase-keys.test.ts`, `src/i18n/literal-keys.test.ts`

**Interfaces:**
- Consumes: `core::rom::pairing::{compare, CardRom, Pairing}`,
  `core::card::manifest::{manifest_path_for, read_manifest}`,
  `core::osinstall::apply::{DistributionManifest, MANIFEST_FILE_NAME}`.
- Produces: Tauri command `preload_rom_pairing(image: String, content: String) -> AppResult<Pairing>`; TS `preloadRomPairing(image, content): Promise<Pairing>` and `pairingPhrase(p: Pairing): Phrase | null`.

- [ ] **Step 1: Write the failing Rust test**

In `src-tauri/src/commands/preload.rs`'s tests:

```rust
    /// **G9.** The command reads both records off disk — the tree's
    /// `distribution.json` and the card's own manifest — and answers with the
    /// comparison. Nothing else in ART reads those two files together.
    #[test]
    fn the_pairing_command_reads_both_manifests() {
        use crate::core::rom::pairing::Pairing;

        let dir = scratch("pairing-command");
        let tree = dir.join("dist");
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::write(
            tree.join(crate::core::osinstall::apply::MANIFEST_FILE_NAME),
            r#"{"release":"AmigaOS 3.2","builtFrom":[],"files":[],
                "pairedRom":{"name":"Kickstart 47.102 (A1200)","sha256":"aa",
                "statedMajor":47,"compatibleModels":["A1200"],"requiresMajor":47}}"#,
        )
        .unwrap();

        let image = dir.join("card.img");
        std::fs::write(&image, b"not a card; only its manifest is read").unwrap();

        // A card carrying a V40 ROM under the name config.txt points at.
        let mut card = crate::core::card::manifest::tests_support::sample_manifest();
        card.source.kickstart_file = Some("kick.rom".into());
        card.source.kickstart_stated_major = Some(40);
        card.boot_files = vec![crate::core::card::manifest::ManifestFile {
            name: "kick.rom".into(),
            bytes: 524_288,
            sha256: "bb".into(),
        }];
        std::fs::write(
            crate::core::card::manifest::manifest_path_for(&image),
            crate::core::card::manifest::render_manifest(&card).unwrap(),
        )
        .unwrap();

        let verdict = rom_pairing_for(&image, &tree).unwrap();

        match verdict {
            Pairing::Unsuitable { needs, found, .. } => {
                assert_eq!((needs, found), (47, Some(40)));
            }
            other => panic!("{other:?}"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
```

`sample_manifest()` does not exist yet: add it in Task 2 as
`pub(crate) mod tests_support` inside `core/card/manifest.rs`, holding the
same `manifest()` helper the tests there use, so two test modules share one
fixture instead of two drifting copies. Mark it `#[cfg(test)]`.

- [ ] **Step 2: Run it**

```bash
cd src-tauri && cargo test --lib the_pairing_command_reads_both_manifests
```

Expected: FAIL — `rom_pairing_for` does not exist.

- [ ] **Step 3: Implement the reader and the command**

In `src-tauri/src/commands/preload.rs`:

```rust
/// Read both records and compare them (G9). Split from the command so the
/// test can drive it without Tauri.
///
/// **Everything missing is `NotChecked`, never an error and never a pass.** A
/// user pointing at a folder that is not a distribution tree, or a card ART
/// did not build, has done nothing wrong — there is simply nothing to check.
fn rom_pairing_for(image: &Path, content: &Path) -> CoreResult<Pairing> {
    let tree = std::fs::read_to_string(content.join(MANIFEST_FILE_NAME))
        .ok()
        .and_then(|text| serde_json::from_str::<DistributionManifest>(&text).ok())
        .and_then(|manifest| manifest.paired_rom);

    let card = read_manifest(&manifest_path_for(image)).ok().and_then(|card| {
        let name = card.source.kickstart_file.clone()?;
        let entry = card.boot_files.iter().find(|file| file.name == name)?;
        Some(CardRom {
            name,
            sha256: entry.sha256.clone(),
            stated_major: card.source.kickstart_stated_major,
        })
    });

    Ok(compare(tree.as_ref(), card.as_ref()))
}

/// What ART can say about the ROM on this card and the tree about to go onto
/// it. Reads two manifests; writes nothing (§92's PREVIEW).
#[tauri::command]
pub fn preload_rom_pairing(image: String, content: String) -> AppResult<Pairing> {
    Ok(rom_pairing_for(
        Path::new(image.trim()),
        Path::new(content.trim()),
    )?)
}
```

`card.source.kickstart_stated_major` comes from Task 2.

Register `preload_rom_pairing` in `src-tauri/src/lib.rs`'s `invoke_handler![]`.

- [ ] **Step 4: Run the Rust tests**

```bash
cd src-tauri && cargo test --lib preload
```

Expected: PASS.

- [ ] **Step 5: Write the failing TypeScript test**

In `src/lib/preload.test.ts`:

```ts
describe("pairingPhrase", () => {
  it("says nothing when the card carries the very ROM the tree was built for", () => {
    expect(pairingPhrase({ verdict: "paired" })).toBeNull();
  });

  it("names the ROM when it is a different but sufficient one", () => {
    expect(pairingPhrase({ verdict: "suitable", rom: "kick.rom" })).toEqual({
      key: "preload.pairing.suitable",
      params: { rom: "kick.rom" },
    });
  });

  it("gives both versions when the card's ROM is too old", () => {
    expect(
      pairingPhrase({ verdict: "unsuitable", needs: 47, found: 40, rom: "kick.rom" })
    ).toEqual({
      key: "preload.pairing.unsuitable",
      params: { needs: 47, found: 40, rom: "kick.rom" },
    });
  });

  it("has a sentence for a ROM that states no version at all", () => {
    expect(
      pairingPhrase({ verdict: "unsuitable", needs: 47, found: null, rom: "kick.rom" })
    ).toEqual({
      key: "preload.pairing.unsuitableUnknown",
      params: { needs: 47, rom: "kick.rom" },
    });
  });

  it("says which side did not answer, and never passes", () => {
    expect(pairingPhrase({ verdict: "not-checked", why: "tree-records-no-rom" })).toEqual({
      key: "preload.pairing.notChecked.tree",
    });
    expect(pairingPhrase({ verdict: "not-checked", why: "card-records-no-rom" })).toEqual({
      key: "preload.pairing.notChecked.card",
    });
  });
});
```

- [ ] **Step 6: Run it**

```bash
npx vitest run src/lib/preload.test.ts
```

Expected: FAIL — `pairingPhrase` is not exported.

- [ ] **Step 7: Add the types, the wrapper and the mapper**

In `src/lib/preload.ts`:

```ts
/** What ART can say about the ROM on a card and the tree going onto it (G9). */
export type Pairing =
  | { verdict: "paired" }
  | { verdict: "suitable"; rom: string }
  | { verdict: "unsuitable"; needs: number; found: number | null; rom: string }
  | { verdict: "not-checked"; why: "tree-records-no-rom" | "card-records-no-rom" };

/** Ask whether the card's Kickstart suits the tree. Reads two manifests. */
export async function preloadRomPairing(image: string, content: string): Promise<Pairing> {
  return invoke<Pairing>("preload_rom_pairing", { image, content });
}

/**
 * The sentence for a pairing, or `null` when there is nothing to say.
 *
 * `paired` renders nothing on purpose: silence is the right report for "the
 * ROM you built this for is the ROM on the card", and a tick that means
 * "checked and fine" invites the reader to trust the *absence* of one.
 */
export function pairingPhrase(pairing: Pairing): Phrase | null {
  switch (pairing.verdict) {
    case "paired":
      return null;
    case "suitable":
      return { key: "preload.pairing.suitable", params: { rom: pairing.rom } };
    case "unsuitable":
      return pairing.found === null
        ? {
            key: "preload.pairing.unsuitableUnknown",
            params: { needs: pairing.needs, rom: pairing.rom },
          }
        : {
            key: "preload.pairing.unsuitable",
            params: { needs: pairing.needs, found: pairing.found, rom: pairing.rom },
          };
    case "not-checked":
      return pairing.why === "tree-records-no-rom"
        ? { key: "preload.pairing.notChecked.tree" }
        : { key: "preload.pairing.notChecked.card" };
  }
}
```

- [ ] **Step 8: Add the strings to both catalogues**

Under `preload` in `src/i18n/en.json`:

```json
      "pairing": {
        "suitable": "The card carries {{rom}}, which is not the Kickstart this system was built against — but it is new enough to run it.",
        "unsuitable": "This system was built for a Kickstart V{{needs}} or newer, and the card carries {{rom}}, which states V{{found}}. The Amiga will start and stop with \"This disk must be booted from Kickstart ROM 3.2 (V47) or from the 'Modules' disk that matches your hardware.\" Preparing the card anyway is fine — the ROM can be replaced afterwards without rebuilding anything.",
        "unsuitableUnknown": "This system was built for a Kickstart V{{needs}} or newer, and {{rom}} on the card states no version at all, so ART cannot tell whether it will run. The Amiga may stop asking for a newer ROM.",
        "notChecked": {
          "tree": "ART cannot check whether the card's Kickstart suits this folder: the folder carries no record of the ROM it was built against.",
          "card": "ART cannot check whether the card's Kickstart suits this system: the card has no manifest beside it, or its manifest does not say which file is the Kickstart."
        }
      },
```

and the same keys with Turkish values in `src/i18n/tr.json`.

- [ ] **Step 9: Render it above the confirmation**

In `src/components/osbuilder/VolumePreload.tsx`, beside the other state:

```tsx
  const [pairing, setPairing] = useState<Pairing | null>(null);
```

and beside the effect that already clears the plan when `fingerprint` changes:

```tsx
  // G9: the ROM question is about the card and the folder going onto it, so
  // it is asked whenever either changes — and forgotten with the plan, since
  // a stale verdict beside a fresh plan is worse than none.
  useEffect(() => {
    const filled = picks.find((pick) => pick.chosen && pick.content);
    if (!imagePath || !filled?.content) {
      setPairing(null);
      return;
    }
    let cancelled = false;
    preloadRomPairing(imagePath, filled.content)
      .then((verdict) => {
        if (!cancelled) setPairing(verdict);
      })
      .catch(() => {
        // Not an error the user needs: the command answers `not-checked`
        // for everything it cannot read, so a rejection here means the
        // command itself failed, and the preview below is unaffected.
        if (!cancelled) setPairing(null);
      });
    return () => {
      cancelled = true;
    };
  }, [fingerprint, imagePath]);
```

Then render above the confirmation checkbox:

```tsx
{pairing &&
  (() => {
    const phrase = pairingPhrase(pairing);
    if (!phrase) return null;
    return (
      <p
        className={pairing.verdict === "unsuitable" ? "badge badge-warn" : "muted"}
        style={{ fontSize: 12, margin: "0 0 8px" }}
      >
        {t(phrase.key, phrase.params)}
      </p>
    );
  })()}
```

- [ ] **Step 10: Enumerate the new phrase and move the dynamic count**

Add `pairingPhrase` to `src/i18n/phrase-keys.test.ts` (all five variants
resolve), and raise the expected number in `src/i18n/literal-keys.test.ts` by
one **with a comment saying it is G9's pairing line**.

- [ ] **Step 11: Run everything**

```bash
pnpm lint && pnpm test
cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test
```

Expected: all green.

- [ ] **Step 12: Commit**

```bash
git add src-tauri/src src/lib src/components src/i18n
git commit -m "Say whether the card's Kickstart suits the tree, before formatting (G9)"
```

---

### Task 5: Prove it against the real material

**Files:**
- Modify: `src-tauri/src/commands/preload.rs` (an `#[ignore]`d hook)
- Modify: `docs/STATUS.md`, `docs/ISSUES.md`, `docs/FEATURES.md`,
  `CHANGELOG.md`

**Interfaces:**
- Consumes: `rom_pairing_for` (Task 4).
- Produces: nothing further; this task closes G9.

- [ ] **Step 1: Write the hook**

```rust
    /// **The pairing that actually failed, as a test** (G9).
    ///
    /// ```text
    /// cd src-tauri
    /// ART_TREE_V47="E:\amiga\ProjeART\dist-3.2b" \
    /// ART_TREE_V40="E:\amiga\ProjeART\dist-3.2-v40" \
    /// ART_CARD="E:\amiga\ProjeART\card.img" \
    ///   cargo test the_real_trees_against_a_real_card_when_asked -- --nocapture --ignored
    /// ```
    ///
    /// `ART_CARD` is a card ART built, so its manifest is beside it. The V47
    /// tree against a V40-carrying card is the 2026-08-16 failure; the V40
    /// tree carries its own modules and suits either.
    #[test]
    #[ignore = "reads the user's own trees and card; run explicitly"]
    fn the_real_trees_against_a_real_card_when_asked() {
        let (Ok(v47), Ok(v40), Ok(card)) = (
            std::env::var("ART_TREE_V47"),
            std::env::var("ART_TREE_V40"),
            std::env::var("ART_CARD"),
        ) else {
            return;
        };
        let card = Path::new(&card);

        let needs_newer = rom_pairing_for(card, Path::new(&v47)).unwrap();
        println!("V47 tree: {needs_newer:?}");
        let brings_its_own = rom_pairing_for(card, Path::new(&v40)).unwrap();
        println!("V40 tree: {brings_its_own:?}");

        assert!(
            !matches!(brings_its_own, Pairing::Unsuitable { .. }),
            "a tree carrying its own ROM modules is never unsuitable: \
             {brings_its_own:?}"
        );
    }
```

- [ ] **Step 2: Run it against the real material**

Rebuild the two trees first if they are not on disk (their commands are in
`docs/STATUS.md`'s reproduce block), then run the hook and **read the output**.
The V47 tree against a card carrying the V40 ROM must print `Unsuitable {
needs: 47, found: Some(40), .. }`.

- [ ] **Step 3: Record what it printed**

`docs/ISSUES.md`: no new defect unless the run finds one. `docs/FEATURES.md`:
flip G9's row to ✅ with the verdicts the real run produced.
`docs/STATUS.md`: the snapshot's SD-2 line (G9 no longer owed) and a session
log line carrying the same numbers. `CHANGELOG.md`: one user-facing entry
saying ART now warns before preparing a card whose Kickstart is older than the
system needs.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/preload.rs docs CHANGELOG.md
git commit -m "G9 closes: the real V47 tree against a real V40 card warns (G9)"
```

---

## Where this plan departs from the spec

Two, both found while writing the tasks. The spec is the approved design;
these are named rather than absorbed.

1. **`PairedRom` carries no `stored_checksum`.** The spec listed one. It is
   redundant here: `sha256` (of the *decoded* image) is what the comparison
   uses for equality, and the stored checksum's job — telling two same-revision
   builds apart — was already done by `identify_rom` before the name was
   recorded. Adding it would mean making `core::rom`'s private
   `stored_checksum` public for a field nothing reads.
2. **The card's ROM major is recorded at build time**, in
   `SourceFacts::kickstart_stated_major`, rather than read when the comparison
   runs. The spec assumed the comparison could ask the ROM; it cannot — ART
   writes FAT32 and has no reader for it, which is the spec's own §2
   constraint followed to its conclusion.

Both belong in the spec if it is ever revised; neither changes what the user
sees.

## Notes for whoever executes this

- **Task 1 and Task 2 are independent.** Task 3 needs Task 1's type; Task 4
  needs all three. Task 5 needs Task 4.
- The two real trees named in Task 5 (`dist-3.2b`, `dist-3.2-v40`) exist on
  the machine as of 2026-08-17 and were built by
  `run_the_real_engine_against_the_users_own_media_when_asked` with the V47 and
  V40 ROMs respectively.
- If a step's test passes without the implementation, stop: that means the
  test is not testing what it claims. Every "run it to see it fail" step is
  there because a test that has never failed has never been checked.
