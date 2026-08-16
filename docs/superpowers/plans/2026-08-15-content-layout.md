# Content Layout Policy Implementation Plan (SD-2 · G11)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn a pile of dropped files into an organised staging tree on the PC that the preload screen copies onto a card's Amiga volumes.

**Architecture:** A new `core/layout/` module in three parts — a walk that finds things (`scan.rs`), a policy that proposes where each goes (`policy.rs`), and an applier that materialises the tree as a job (`apply.rs`) — with `mod.rs` holding the types, the classifier and `plan()`. Thin Tauri adapters over it, then a screen whose preview table is **editable**, because ART cannot tell a demo from a game and must not pretend to.

**Tech Stack:** Rust (`std` + `serde` + `thiserror` only in `core/`), Tauri commands, React + TypeScript + react-i18next.

**Spec:** [`docs/superpowers/specs/2026-08-15-content-layout-design.md`](../specs/2026-08-15-content-layout-design.md)

## Global Constraints

- **`core/` stays platform-independent.** No `use tauri`, no Windows APIs, no network. `core/layout` may use `std`, `serde`, and other `core/` modules only.
- **Every write goes through `core/safety`.** `atomic_write` for a file ART composes; a plain copy of a user file uses `std::fs::copy` into a path that does not exist yet, never over one that does.
- **Nothing overwrites.** A destination that already holds that name is a `Collision` reported by the plan; the applier refuses.
- **The source is never modified.** Every apply test asserts it byte for byte.
- **Cancellation is checked between whole items, never inside one** (§54). Return `CoreError::Cancelled`.
- **Fixtures are synthetic and built in a tempdir.** ART ships no copyrighted Amiga content, ever.
- **i18n:** every new key goes into **both** `src/i18n/en.json` and `src/i18n/tr.json` in the same commit. `pnpm test` fails the build if the key sets differ.
- **`src/lib/*` returns `Phrase { key, params? }`**, never a rendered string; the component calls `t()`. Every new mapper variant gets enumerated in `src/i18n/phrase-keys.test.ts`.
- **New commands** go in *both* `invoke_handler![]` in `src-tauri/src/lib.rs` and a typed wrapper in `src/lib/`; components never call `invoke` directly.
- Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (twice — ART-059), `pnpm lint` and `pnpm test` before each commit.

## File Structure

| File | Responsibility |
|---|---|
| `src-tauri/src/core/layout/mod.rs` (create) | The types, `classify()`, `plan()` |
| `src-tauri/src/core/layout/scan.rs` (create) | The walk: what is on disk, with the two safety rules |
| `src-tauri/src/core/layout/policy.rs` (create) | The drawer table and `drawer_for()` |
| `src-tauri/src/core/layout/apply.rs` (create) | Materialising the staging tree, as a job |
| `src-tauri/src/core/mod.rs` (modify) | `pub mod layout;` |
| `src-tauri/src/commands/layout.rs` (create) | `layout_plan`, `layout_apply` |
| `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs` (modify) | Register both |
| `src/lib/layout.ts` (create) | Typed wrappers, the pure UI rules, `Phrase` mappers |
| `src/lib/layout.test.ts` (create) | Those pure rules |
| `src/pages/ContentLayout.tsx` (create) | The editable preview screen |
| `src/App.tsx`, `src/components/layout/Sidebar.tsx` (modify) | The route and its sidebar entry |
| `src/i18n/{en,tr}.json` (modify) | Keys, both catalogues |

### Why `core/layout` gets its own walk

There are already two walks: `core/collection.rs::collect_files_at_depth` (depth-limited, skips symlinks, **filters to five extensions** and produces paths for a game collection) and `commands/whdload.rs::walk` (produces `whdload::Entry`, no size, walks a temp dir ART just unpacked so it does not need the symlink rule).

Neither output fits — `core/layout` needs a size per entry and must stop at a WHDLoad drawer rather than descend into it. What the three share is two rules totalling about six lines: a depth cap, and `symlink_metadata` so a Windows junction cannot make a cycle. **Do not lift a shared walk.** Three short correct walks beat one walk with four flags, and the shared part is smaller than the parameterisation would be. Copy the two rules and cite them.

---

### Task 1: The walk — what is on disk

**Files:**
- Create: `src-tauri/src/core/layout/scan.rs`
- Create: `src-tauri/src/core/layout/mod.rs` (module declarations only, this task)
- Modify: `src-tauri/src/core/mod.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub struct Found { pub path: PathBuf, pub bytes: u64, pub is_dir: bool }`
  - `pub const MAX_SCAN_DEPTH: usize = 32;`
  - `pub fn gather(paths: &[PathBuf]) -> CoreResult<Vec<Found>>`
  - `pub fn is_whdload_drawer(dir: &Path) -> bool`

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/core/layout/scan.rs` with only the test module and the
signatures it calls (empty bodies that `todo!()`), so the test compiles and
fails rather than failing to build.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("art-layout-scan-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A folder is walked and every file in it becomes a `Found`, at any depth.
    #[test]
    fn a_folder_is_walked_and_its_files_are_found() {
        let dir = scratch("walk");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.adf"), vec![0u8; 10]).unwrap();
        std::fs::write(dir.join("sub").join("b.lha"), vec![0u8; 20]).unwrap();

        let mut found = gather(&[dir.clone()]).unwrap();
        found.sort_by(|a, b| a.path.cmp(&b.path));

        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(found[0].path, dir.join("a.adf"));
        assert_eq!(found[0].bytes, 10);
        assert!(!found[0].is_dir);
        assert_eq!(found[1].bytes, 20);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The one non-obvious rule.** A folder that directly holds a `.slave` is
    /// the game, and is returned whole rather than descended into — otherwise
    /// dropping a folder of 400 files scatters the insides of every game.
    #[test]
    fn a_whdload_drawer_is_returned_whole_and_never_walked_into() {
        let dir = scratch("drawer");
        let game = dir.join("TurricanII");
        std::fs::create_dir_all(game.join("data")).unwrap();
        std::fs::write(game.join("TurricanII.slave"), vec![0u8; 4]).unwrap();
        std::fs::write(game.join("data").join("level1"), vec![0u8; 6]).unwrap();

        let found = gather(&[dir.clone()]).unwrap();

        assert_eq!(found.len(), 1, "the drawer is one thing, not three: {found:?}");
        assert_eq!(found[0].path, game);
        assert!(found[0].is_dir);
        assert_eq!(found[0].bytes, 10, "a drawer measures its whole tree");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A folder of games gives one entry per game, not one entry for the folder.
    #[test]
    fn a_folder_of_drawers_gives_one_entry_per_drawer() {
        let dir = scratch("many");
        for name in ["TurricanII", "Zool"] {
            let game = dir.join(name);
            std::fs::create_dir_all(&game).unwrap();
            std::fs::write(game.join(format!("{name}.slave")), vec![0u8; 4]).unwrap();
        }

        let found = gather(&[dir.clone()]).unwrap();
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found.iter().all(|f| f.is_dir));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file given directly is found as itself, folder or not.
    #[test]
    fn a_file_named_directly_is_found_as_itself() {
        let dir = scratch("direct");
        let file = dir.join("one.adf");
        std::fs::write(&file, vec![0u8; 7]).unwrap();

        let found = gather(&[file.clone()]).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, file);
        assert_eq!(found[0].bytes, 7);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `is_whdload_drawer` looks one level down and no further: a folder whose
    /// *child* holds a slave is a folder of games, not a game.
    #[test]
    fn only_a_slave_directly_inside_makes_a_drawer() {
        let dir = scratch("depth");
        let outer = dir.join("Games");
        let inner = outer.join("Zool");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join("Zool.slave"), b"x").unwrap();

        assert!(!is_whdload_drawer(&outer));
        assert!(is_whdload_drawer(&inner));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd src-tauri && cargo test layout::scan:: 2>&1 | tail -20
```

Expected: FAIL — `not yet implemented` from the `todo!()` bodies.

- [ ] **Step 3: Write the implementation**

`src-tauri/src/core/layout/mod.rs`, for now:

```rust
//! What goes where (SD-2 · G11).
//!
//! Between the classifier and the card: a pile of files in, a staging tree on
//! the PC out, which the preload screen then copies onto a volume. The staging
//! seam is not a preference — a real PiStorm card is PFS3 and ART cannot write
//! PFS3, so writing straight into the volume works only on FFS, which is not
//! what a finished card uses.

pub mod apply;
pub mod policy;
pub mod scan;
```

`src-tauri/src/core/layout/scan.rs`:

```rust
//! Finding the things a layout is made of.
//!
//! Its own walk rather than `core/collection`'s: that one filters to five
//! extensions and carries no size, and this one has to stop at a WHDLoad
//! drawer instead of descending into it. What is shared is two rules — a depth
//! cap, and `symlink_metadata` so a Windows junction cannot make a cycle — and
//! six lines of rule is smaller than the parameterisation sharing them would
//! cost.

use std::path::{Path, PathBuf};

use crate::core::error::{CoreError, CoreResult};

/// How deep a scan will descend.
///
/// The same cap and the same reason as `core/collection`: a symlink cycle plus
/// unbounded recursion overflows the stack, and with `panic = "abort"` that
/// takes the whole application down rather than reporting an error.
pub const MAX_SCAN_DEPTH: usize = 32;

/// One thing the layout will place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    /// For a file, its size. For a drawer, its whole tree.
    pub bytes: u64,
    /// True only for a WHDLoad drawer — every other directory is walked
    /// through rather than returned.
    pub is_dir: bool,
}

/// Whether `dir` **is** a WHDLoad drawer: it directly holds a `.slave`.
///
/// One level, deliberately. `core/whdload::analyse` is not the right question
/// here: it reads an unpacked *archive's* entry list, where exactly one drawer
/// sits beside its own `.info`, and a folder holding fifty games is not that
/// shape — `pick_slave` would choose one and call the whole folder a game.
pub fn is_whdload_drawer(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry
            .path()
            .extension()
            .map(|ext| ext.eq_ignore_ascii_case("slave"))
            .unwrap_or(false)
    })
}

/// Everything under `paths`, with WHDLoad drawers kept whole.
pub fn gather(paths: &[PathBuf]) -> CoreResult<Vec<Found>> {
    let mut out = Vec::new();
    for path in paths {
        if path.is_dir() {
            if is_whdload_drawer(path) {
                out.push(drawer(path)?);
            } else {
                walk(path, 0, &mut out)?;
            }
        } else if path.is_file() {
            out.push(file(path)?);
        } else {
            return Err(CoreError::InvalidInput(format!(
                "'{}' is neither a file nor a folder",
                path.display()
            )));
        }
    }
    Ok(out)
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<Found>) -> CoreResult<()> {
    if depth >= MAX_SCAN_DEPTH {
        return Ok(());
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // `symlink_metadata` does not follow links, so a directory symlink
        // pointing back up the tree is skipped instead of followed.
        let is_symlink = std::fs::symlink_metadata(&path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        if is_symlink {
            continue;
        }
        if path.is_dir() {
            if is_whdload_drawer(&path) {
                out.push(drawer(&path)?);
            } else {
                walk(&path, depth + 1, out)?;
            }
        } else if path.is_file() {
            out.push(file(&path)?);
        }
    }
    Ok(())
}

fn file(path: &Path) -> CoreResult<Found> {
    Ok(Found {
        path: path.to_path_buf(),
        bytes: std::fs::metadata(path)?.len(),
        is_dir: false,
    })
}

fn drawer(path: &Path) -> CoreResult<Found> {
    Ok(Found {
        path: path.to_path_buf(),
        bytes: tree_bytes(path, 0),
        is_dir: true,
    })
}

fn tree_bytes(dir: &Path, depth: usize) -> u64 {
    if depth >= MAX_SCAN_DEPTH {
        return 0;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut total = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if std::fs::symlink_metadata(&path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            continue;
        }
        if path.is_dir() {
            total += tree_bytes(&path, depth + 1);
        } else if let Ok(meta) = std::fs::metadata(&path) {
            total += meta.len();
        }
    }
    total
}
```

Add to `src-tauri/src/core/mod.rs`, in alphabetical position among the other
`pub mod` lines:

```rust
pub mod layout;
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd src-tauri && cargo test layout::scan:: 2>&1 | tail -10
```

Expected: PASS, 5 tests.

Note: `apply.rs` and `policy.rs` are declared in `mod.rs` but do not exist yet.
Create both as empty files with only a `//!` line this task so the crate
builds; Tasks 2 and 4 fill them.

- [ ] **Step 5: Check formatting and lints, then commit**

```bash
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings
git add src-tauri/src/core/layout/ src-tauri/src/core/mod.rs
git commit -m "SD-2 G11: the walk a layout is made of"
```

---

### Task 2: The policy — which drawer, and what ART may not claim

**Files:**
- Modify: `src-tauri/src/core/layout/policy.rs`

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces:
  - `pub enum WhdloadPlacement { Unpack, AsArchive }`
  - `pub struct Policy { pub whdload: WhdloadPlacement, pub games: String, pub floppies: String, pub hard_disks: String, pub discs: String, pub unsorted: String }` with `Default`
  - `pub fn drawer_for(kind: &ItemKind, policy: &Policy) -> Option<&str>` — `None` means refused, not "put it at the root"

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::layout::ItemKind;

    /// The shipped defaults, pinned. A card built from them is what the user
    /// gets without touching anything.
    #[test]
    fn the_default_policy_puts_each_kind_where_the_spec_says() {
        let policy = Policy::default();
        let cases = [
            (ItemKind::WhdloadArchive { name: "Turrican".into() }, Some("Games")),
            (ItemKind::WhdloadDrawer { name: "Zool".into() }, Some("Games")),
            (ItemKind::FloppyImage, Some("Floppies")),
            (ItemKind::HardDiskImage, Some("HardDisks")),
            (ItemKind::OpticalImage, Some("CDs")),
            (ItemKind::Archive, Some("Unsorted")),
            (ItemKind::Unknown, Some("Unsorted")),
        ];
        for (kind, expected) in cases {
            assert_eq!(drawer_for(&kind, &policy), expected, "{kind:?}");
        }
    }

    /// **Two kinds are refused rather than placed**, and refusing is not the
    /// same as dropping: a ROM belongs on the FAT32 partition and a 1541 disk
    /// has no business on an Amiga volume at all. `core/card/intake.rs` gives
    /// both the same answer for a card.
    #[test]
    fn a_rom_and_a_commodore_disk_get_no_drawer() {
        let policy = Policy::default();
        assert_eq!(drawer_for(&ItemKind::Rom, &policy), None);
        assert_eq!(drawer_for(&ItemKind::Commodore8Bit, &policy), None);
    }

    /// A renamed drawer is used everywhere that kind lands.
    #[test]
    fn a_drawer_the_user_renamed_is_the_one_used() {
        let policy = Policy {
            games: "Oyunlar".into(),
            ..Policy::default()
        };
        assert_eq!(
            drawer_for(&ItemKind::WhdloadDrawer { name: "Zool".into() }, &policy),
            Some("Oyunlar")
        );
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd src-tauri && cargo test layout::policy:: 2>&1 | tail -20
```

Expected: FAIL to compile — `Policy` and `drawer_for` do not exist. That is a
failing test; do not "fix" it by writing the test differently.

- [ ] **Step 3: Write the implementation**

```rust
//! Which drawer a thing goes in — the rules, as data.
//!
//! **A named field per drawer rather than a rule list**, and the reason is the
//! compiler: `drawer_for` matches every [`ItemKind`], so a kind added later
//! cannot quietly fall through to "somewhere". A `Vec<Rule>` keyed by kind
//! would need a runtime lookup and a default, and the default is exactly the
//! silent answer this design is trying not to give.

use serde::{Deserialize, Serialize};

use crate::core::layout::ItemKind;

/// What happens to an archive holding a WHDLoad pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WhdloadPlacement {
    /// Unpacked into a drawer with its icon beside it, so the card arrives
    /// ready and the game is visible on Workbench. The default, because that
    /// is the point of the feature.
    #[default]
    Unpack,
    /// Copied in as the `.lha` it is; unpacking is the user's job on the Amiga.
    AsArchive,
}

/// Where each kind of thing goes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    pub whdload: WhdloadPlacement,
    pub games: String,
    pub floppies: String,
    pub hard_disks: String,
    pub discs: String,
    pub unsorted: String,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            whdload: WhdloadPlacement::default(),
            games: "Games".into(),
            floppies: "Floppies".into(),
            hard_disks: "HardDisks".into(),
            discs: "CDs".into(),
            unsorted: "Unsorted".into(),
        }
    }
}

/// The drawer `kind` belongs in, or `None` when it belongs on no Amiga volume.
///
/// `None` is a **refusal with a reason**, not a shrug: the caller turns it
/// into a `Refusal` the preview shows. There is deliberately no fallback
/// drawer for it — a ROM quietly landing in `Unsorted/` is a file on the wrong
/// partition that nobody was told about.
pub fn drawer_for<'a>(kind: &ItemKind, policy: &'a Policy) -> Option<&'a str> {
    match kind {
        ItemKind::WhdloadArchive { .. } | ItemKind::WhdloadDrawer { .. } => Some(&policy.games),
        ItemKind::FloppyImage => Some(&policy.floppies),
        ItemKind::HardDiskImage => Some(&policy.hard_disks),
        ItemKind::OpticalImage => Some(&policy.discs),
        ItemKind::Archive | ItemKind::Unknown => Some(&policy.unsorted),
        ItemKind::Rom | ItemKind::Commodore8Bit => None,
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

This task's tests need `ItemKind`, which Task 3 defines. Add `ItemKind` to
`mod.rs` now — its definition is in Task 3 Step 3 and is reproduced there;
copy it in here so this task compiles, and Task 3 will not redefine it.

```bash
cd src-tauri && cargo test layout::policy:: 2>&1 | tail -10
```

Expected: PASS, 3 tests.

- [ ] **Step 5: Commit**

```bash
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings
git add src-tauri/src/core/layout/
git commit -m "SD-2 G11: the drawer policy, and the two kinds it refuses"
```

---

### Task 3: Classify and plan

**Files:**
- Modify: `src-tauri/src/core/layout/mod.rs`

**Interfaces:**
- Consumes: `scan::{gather, Found}`, `policy::{drawer_for, Policy, WhdloadPlacement}`
- Produces:
  - `pub enum ItemKind { WhdloadArchive { name: String }, WhdloadDrawer { name: String }, FloppyImage, HardDiskImage, OpticalImage, Archive, Unknown, Rom, Commodore8Bit }`
  - `pub enum Placement { CopyFile, CopyTree, UnpackWhdload }`
  - `pub struct LayoutItem { pub source: PathBuf, pub kind: ItemKind, pub destination: String, pub placement: Placement, pub bytes: u64 }`
  - `pub enum RefusalReason { BelongsOnBootPartition, NoPlaceOnAnAmigaVolume }`
  - `pub struct Refusal { pub source: PathBuf, pub reason: RefusalReason }`
  - `pub struct Collision { pub destination: String, pub sources: Vec<PathBuf> }`
  - `pub struct LayoutPlan { pub root: PathBuf, pub items: Vec<LayoutItem>, pub refused: Vec<Refusal>, pub collisions: Vec<Collision>, pub bytes: u64 }`
  - `pub fn classify(found: &Found) -> CoreResult<ItemKind>`
  - `pub fn plan(root: &Path, paths: &[PathBuf], policy: &Policy) -> CoreResult<LayoutPlan>`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("art-layout-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A 901 120-byte file whose first bytes say `DOS\0` is an ADF, and
    /// `core/detect` says so from the bytes rather than the name.
    fn adf(path: &Path) {
        let mut bytes = vec![0u8; 901_120];
        bytes[0..4].copy_from_slice(b"DOS\0");
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn a_floppy_image_is_proposed_for_the_floppies_drawer() {
        let dir = scratch("adf");
        let root = dir.join("staging");
        adf(&dir.join("Workbench.adf"));

        let made = plan(&root, &[dir.join("Workbench.adf")], &Policy::default()).unwrap();

        assert_eq!(made.items.len(), 1, "{made:?}");
        assert_eq!(made.items[0].kind, ItemKind::FloppyImage);
        assert_eq!(made.items[0].destination, "Floppies/Workbench.adf");
        assert_eq!(made.items[0].placement, Placement::CopyFile);
        assert!(made.refused.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A folder that is a drawer keeps its own name under `Games/` and is
    /// copied as a tree.
    #[test]
    fn a_whdload_drawer_is_proposed_whole_under_games() {
        let dir = scratch("drawer");
        let root = dir.join("staging");
        let game = dir.join("TurricanII");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("TurricanII.slave"), b"slave").unwrap();

        let made = plan(&root, &[game.clone()], &Policy::default()).unwrap();

        assert_eq!(
            made.items[0].kind,
            ItemKind::WhdloadDrawer { name: "TurricanII".into() }
        );
        assert_eq!(made.items[0].destination, "Games/TurricanII");
        assert_eq!(made.items[0].placement, Placement::CopyTree);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A ROM is refused, not placed.** It belongs on the FAT32 partition,
    /// and saying so beats a Kickstart quietly landing in `Unsorted/`.
    #[test]
    fn a_rom_is_refused_with_a_reason_and_never_reaches_items() {
        let dir = scratch("rom");
        let root = dir.join("staging");
        let rom = dir.join("kick.rom");
        // 512 KB starting with the Kickstart magic `$11114EF9`.
        let mut bytes = vec![0u8; 512 * 1024];
        bytes[0..4].copy_from_slice(&[0x11, 0x11, 0x4E, 0xF9]);
        std::fs::write(&rom, bytes).unwrap();

        let made = plan(&root, &[rom.clone()], &Policy::default()).unwrap();

        assert!(made.items.is_empty(), "{:?}", made.items);
        assert_eq!(made.refused.len(), 1);
        assert_eq!(made.refused[0].source, rom);
        assert_eq!(made.refused[0].reason, RefusalReason::BelongsOnBootPartition);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Two sources proposing the same destination are a collision, reported
    /// before the button rather than discovered during the copy.
    #[test]
    fn two_things_wanting_one_name_are_a_collision() {
        let dir = scratch("collide");
        let root = dir.join("staging");
        std::fs::create_dir_all(dir.join("one")).unwrap();
        std::fs::create_dir_all(dir.join("two")).unwrap();
        adf(&dir.join("one").join("Disk.adf"));
        adf(&dir.join("two").join("Disk.adf"));

        let made = plan(&root, &[dir.join("one"), dir.join("two")], &Policy::default()).unwrap();

        assert_eq!(made.collisions.len(), 1, "{:?}", made.collisions);
        assert_eq!(made.collisions[0].destination, "Floppies/Disk.adf");
        assert_eq!(made.collisions[0].sources.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A destination that already exists on disk is a collision too — the
    /// applier never overwrites, so the plan has to say it first.
    #[test]
    fn a_name_already_in_the_staging_tree_is_a_collision() {
        let dir = scratch("exists");
        let root = dir.join("staging");
        std::fs::create_dir_all(root.join("Floppies")).unwrap();
        std::fs::write(root.join("Floppies").join("Disk.adf"), b"already here").unwrap();
        adf(&dir.join("Disk.adf"));

        let made = plan(&root, &[dir.join("Disk.adf")], &Policy::default()).unwrap();

        assert_eq!(made.collisions.len(), 1, "{:?}", made.collisions);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The plan totals what the staging tree will need, so the number is on
    /// screen before the button.
    #[test]
    fn the_plan_totals_the_bytes_the_tree_will_need() {
        let dir = scratch("bytes");
        let root = dir.join("staging");
        adf(&dir.join("A.adf"));
        adf(&dir.join("B.adf"));

        let made = plan(&root, &[dir.clone()], &Policy::default()).unwrap();

        assert_eq!(made.bytes, 901_120 * 2);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd src-tauri && cargo test layout:: 2>&1 | tail -20
```

Expected: FAIL — `plan` does not exist.

- [ ] **Step 3: Write the implementation**

Append to `src-tauri/src/core/layout/mod.rs`:

```rust
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::detect::{detect, FormatCategory};
use crate::core::error::CoreResult;
use crate::core::layout::policy::{drawer_for, Policy, WhdloadPlacement};
use crate::core::layout::scan::{gather, Found};

/// What ART can justify saying about one thing on disk.
///
/// **There is no `Demo` and there will not be one.** `Detection` carries a
/// category, a format hint and a confidence, and nothing derivable from the
/// bytes separates a demo from a game. The preview is editable instead; §14
/// and §34 say an uncertain classification is offered, never acted on as fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ItemKind {
    /// An archive holding a WHDLoad pack. `name` is what the drawer will be
    /// called, from `core/whdload::analyse`.
    WhdloadArchive { name: String },
    /// A folder that *is* a drawer — it directly holds a `.slave`.
    WhdloadDrawer { name: String },
    FloppyImage,
    HardDiskImage,
    OpticalImage,
    /// An archive that is not a WHDLoad pack. Could be anything, so it goes to
    /// `Unsorted/` and the user moves it.
    Archive,
    Unknown,
    /// Belongs on the FAT32 boot partition. Refused here.
    Rom,
    /// No business on an Amiga volume at all. Refused here.
    Commodore8Bit,
}

/// How an item reaches the staging tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Placement {
    CopyFile,
    CopyTree,
    UnpackWhdload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutItem {
    pub source: PathBuf,
    pub kind: ItemKind,
    /// Relative to the staging root, `/` separated. Proposed by the policy;
    /// the user may change it before the plan is applied.
    pub destination: String,
    pub placement: Placement,
    /// What this will occupy once placed. For an unpacked archive this is the
    /// archive's own declared uncompressed total — a claim, and named as one.
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RefusalReason {
    /// A Kickstart: the Pi firmware reads it off FAT32, not off a volume.
    BelongsOnBootPartition,
    /// A 1541 disk, a tape, a cartridge.
    NoPlaceOnAnAmigaVolume,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refusal {
    pub source: PathBuf,
    pub reason: RefusalReason,
}

/// Two or more things wanting one name, or one thing wanting a name the
/// staging tree already holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Collision {
    pub destination: String,
    /// Empty of a second entry when the clash is with a file already on disk.
    pub sources: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutPlan {
    pub root: PathBuf,
    pub items: Vec<LayoutItem>,
    pub refused: Vec<Refusal>,
    pub collisions: Vec<Collision>,
    pub bytes: u64,
}

/// What one found thing is.
///
/// A directory reaching here is always a WHDLoad drawer — `scan::gather` walks
/// through every other kind rather than returning it.
pub fn classify(found: &Found) -> CoreResult<ItemKind> {
    if found.is_dir {
        return Ok(ItemKind::WhdloadDrawer {
            name: name_of(&found.path),
        });
    }

    let detection = detect(&found.path)?;
    Ok(match detection.category {
        FormatCategory::FloppyImage => ItemKind::FloppyImage,
        FormatCategory::HardDiskImage => ItemKind::HardDiskImage,
        FormatCategory::OpticalImage => ItemKind::OpticalImage,
        FormatCategory::Rom => ItemKind::Rom,
        FormatCategory::Commodore8Bit => ItemKind::Commodore8Bit,
        FormatCategory::Archive => match whdload_name(&found.path) {
            Some(name) => ItemKind::WhdloadArchive { name },
            None => ItemKind::Archive,
        },
        FormatCategory::Directory | FormatCategory::Unknown => ItemKind::Unknown,
    })
}

/// The drawer name an archive would unpack to, if it holds a WHDLoad pack.
///
/// Reads the archive's **entry list only** — no decompression — so asking the
/// question of four hundred files costs four hundred directory reads rather
/// than four hundred unpacks. `analyse` is the right function here and the
/// wrong one for a folder: this is precisely the shape it was written for, one
/// drawer beside its own `.info`.
fn whdload_name(path: &Path) -> Option<String> {
    let mut backend = crate::core::archive::open(path).ok()?;
    let entries = backend.entries().ok()?;
    let listed: Vec<crate::core::whdload::Entry> = entries
        .iter()
        .map(|entry| crate::core::whdload::Entry {
            relative: entry.name.clone(),
            is_dir: entry.is_dir,
        })
        .collect();
    crate::core::whdload::analyse(&listed)
        .ok()
        .map(|layout| layout.name)
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// What laying these paths out under `root` would do. Writes nothing.
pub fn plan(root: &Path, paths: &[PathBuf], policy: &Policy) -> CoreResult<LayoutPlan> {
    let mut items = Vec::new();
    let mut refused = Vec::new();

    for found in gather(paths)? {
        let kind = classify(&found)?;
        let Some(drawer) = drawer_for(&kind, policy) else {
            refused.push(Refusal {
                source: found.path.clone(),
                reason: match kind {
                    ItemKind::Rom => RefusalReason::BelongsOnBootPartition,
                    _ => RefusalReason::NoPlaceOnAnAmigaVolume,
                },
            });
            continue;
        };

        let (placement, leaf, bytes) = match (&kind, policy.whdload) {
            (ItemKind::WhdloadArchive { name }, WhdloadPlacement::Unpack) => (
                Placement::UnpackWhdload,
                name.clone(),
                declared_bytes(&found.path).unwrap_or(found.bytes),
            ),
            (ItemKind::WhdloadDrawer { name }, _) => {
                (Placement::CopyTree, name.clone(), found.bytes)
            }
            _ => (Placement::CopyFile, name_of(&found.path), found.bytes),
        };

        items.push(LayoutItem {
            source: found.path,
            kind,
            destination: format!("{drawer}/{leaf}"),
            placement,
            bytes,
        });
    }

    let collisions = collisions_in(root, &items);
    let bytes = items.iter().map(|item| item.bytes).sum();

    Ok(LayoutPlan {
        root: root.to_path_buf(),
        items,
        refused,
        collisions,
        bytes,
    })
}

/// What an archive says it decompresses to. A claim, used only to show the
/// user a number; the gate measures what actually arrives.
fn declared_bytes(path: &Path) -> Option<u64> {
    let mut backend = crate::core::archive::open(path).ok()?;
    Some(
        backend
            .entries()
            .ok()?
            .iter()
            .map(|entry| entry.declared_bytes)
            .sum(),
    )
}

/// Every destination two items want, and every one the tree already holds.
fn collisions_in(root: &Path, items: &[LayoutItem]) -> Vec<Collision> {
    let mut by_destination: BTreeMap<&str, Vec<PathBuf>> = BTreeMap::new();
    for item in items {
        by_destination
            .entry(item.destination.as_str())
            .or_default()
            .push(item.source.clone());
    }

    by_destination
        .into_iter()
        .filter_map(|(destination, sources)| {
            let on_disk = root.join(destination.replace('/', std::path::MAIN_SEPARATOR_STR));
            if sources.len() > 1 || on_disk.exists() {
                Some(Collision {
                    destination: destination.to_string(),
                    sources,
                })
            } else {
                None
            }
        })
        .collect()
}
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd src-tauri && cargo test layout:: 2>&1 | tail -10
```

Expected: PASS — 5 scan + 3 policy + 6 plan tests.

- [ ] **Step 5: Commit**

```bash
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test layout::
git add src-tauri/src/core/layout/
git commit -m "SD-2 G11: classify a pile of files and propose where each goes"
```

---

### Task 4: Apply — copying a file and copying a tree

**Files:**
- Modify: `src-tauri/src/core/layout/apply.rs`

**Interfaces:**
- Consumes: `LayoutPlan`, `LayoutItem`, `Placement`
- Produces:
  - `pub struct ApplyOutcome { pub placed: usize, pub bytes: u64 }`
  - `pub fn apply(plan: &LayoutPlan, sink: &dyn ProgressSink) -> CoreResult<ApplyOutcome>`

`Placement::UnpackWhdload` returns `CoreError::InvalidInput` in this task and
is implemented in Task 5.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jobs::NoProgress;
    use crate::core::layout::{ItemKind, LayoutItem, LayoutPlan, Placement};

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("art-layout-apply-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn plan_of(root: &Path, items: Vec<LayoutItem>) -> LayoutPlan {
        LayoutPlan {
            root: root.to_path_buf(),
            bytes: items.iter().map(|i| i.bytes).sum(),
            items,
            refused: Vec::new(),
            collisions: Vec::new(),
        }
    }

    #[test]
    fn a_file_lands_at_its_destination_and_the_source_is_untouched() {
        let dir = scratch("file");
        let root = dir.join("staging");
        let source = dir.join("Disk.adf");
        std::fs::write(&source, b"disk bytes").unwrap();

        let plan = plan_of(
            &root,
            vec![LayoutItem {
                source: source.clone(),
                kind: ItemKind::FloppyImage,
                destination: "Floppies/Disk.adf".into(),
                placement: Placement::CopyFile,
                bytes: 10,
            }],
        );

        let outcome = apply(&plan, &NoProgress).unwrap();

        assert_eq!(outcome.placed, 1);
        assert_eq!(outcome.bytes, 10);
        assert_eq!(
            std::fs::read(root.join("Floppies").join("Disk.adf")).unwrap(),
            b"disk bytes"
        );
        assert_eq!(
            std::fs::read(&source).unwrap(),
            b"disk bytes",
            "the source is never modified"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_drawer_lands_with_its_whole_tree() {
        let dir = scratch("tree");
        let root = dir.join("staging");
        let game = dir.join("Zool");
        std::fs::create_dir_all(game.join("data")).unwrap();
        std::fs::write(game.join("Zool.slave"), b"slave").unwrap();
        std::fs::write(game.join("data").join("level1"), b"level").unwrap();

        let plan = plan_of(
            &root,
            vec![LayoutItem {
                source: game.clone(),
                kind: ItemKind::WhdloadDrawer { name: "Zool".into() },
                destination: "Games/Zool".into(),
                placement: Placement::CopyTree,
                bytes: 10,
            }],
        );

        apply(&plan, &NoProgress).unwrap();

        assert_eq!(
            std::fs::read(root.join("Games").join("Zool").join("Zool.slave")).unwrap(),
            b"slave"
        );
        assert_eq!(
            std::fs::read(root.join("Games").join("Zool").join("data").join("level1")).unwrap(),
            b"level"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **Nothing overwrites.** The plan reports collisions; if one slips
    /// through — the tree changed between preview and apply — the applier
    /// refuses rather than replacing the user's file.
    #[test]
    fn an_existing_destination_is_refused_and_left_alone() {
        let dir = scratch("exists");
        let root = dir.join("staging");
        std::fs::create_dir_all(root.join("Floppies")).unwrap();
        std::fs::write(root.join("Floppies").join("Disk.adf"), b"already here").unwrap();
        let source = dir.join("Disk.adf");
        std::fs::write(&source, b"new bytes").unwrap();

        let plan = plan_of(
            &root,
            vec![LayoutItem {
                source,
                kind: ItemKind::FloppyImage,
                destination: "Floppies/Disk.adf".into(),
                placement: Placement::CopyFile,
                bytes: 9,
            }],
        );

        let err = apply(&plan, &NoProgress).unwrap_err();
        assert!(err.to_string().contains("already"), "{err}");
        assert_eq!(
            std::fs::read(root.join("Floppies").join("Disk.adf")).unwrap(),
            b"already here",
            "the file that was there is still there"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A destination that climbs out of the staging root is refused. The
    /// destination is user-editable text, so it is untrusted input like an
    /// archive entry name, and goes through the same gate.
    #[test]
    fn a_destination_that_escapes_the_root_is_refused() {
        let dir = scratch("escape");
        let root = dir.join("staging");
        let source = dir.join("Disk.adf");
        std::fs::write(&source, b"x").unwrap();

        for bad in ["../outside/Disk.adf", "C:/Windows/Disk.adf"] {
            let plan = plan_of(
                &root,
                vec![LayoutItem {
                    source: source.clone(),
                    kind: ItemKind::FloppyImage,
                    destination: bad.into(),
                    placement: Placement::CopyFile,
                    bytes: 1,
                }],
            );
            assert!(apply(&plan, &NoProgress).is_err(), "{bad} was allowed");
        }
        assert!(!dir.join("outside").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A sink that says stop once `after` items have been reported.
    struct StopAfter {
        after: std::cell::Cell<u64>,
    }

    impl ProgressSink for StopAfter {
        fn report(&self, done: u64, _total: Option<u64>, _label: &str) {
            self.after.set(self.after.get().min(done));
        }
        fn is_cancelled(&self) -> bool {
            self.after.get() == 0
        }
    }

    /// **Cancelling leaves whole files, never half of one** (§54). The check
    /// sits between items, so the first is complete and the second was never
    /// begun.
    #[test]
    fn stopping_leaves_the_finished_item_whole_and_the_next_one_absent() {
        let dir = scratch("cancel");
        let root = dir.join("staging");
        std::fs::write(dir.join("A.adf"), b"first").unwrap();
        std::fs::write(dir.join("B.adf"), b"second").unwrap();

        let plan = plan_of(
            &root,
            vec![
                LayoutItem {
                    source: dir.join("A.adf"),
                    kind: ItemKind::FloppyImage,
                    destination: "Floppies/A.adf".into(),
                    placement: Placement::CopyFile,
                    bytes: 5,
                },
                LayoutItem {
                    source: dir.join("B.adf"),
                    kind: ItemKind::FloppyImage,
                    destination: "Floppies/B.adf".into(),
                    placement: Placement::CopyFile,
                    bytes: 6,
                },
            ],
        );

        // Not cancelled for item 0, cancelled by the time item 1 is reached.
        let sink = StopAfter {
            after: std::cell::Cell::new(1),
        };
        let err = apply(&plan, &sink).unwrap_err();

        assert_eq!(err.code(), "ART-CANCELLED", "{err}");
        assert_eq!(
            std::fs::read(root.join("Floppies").join("A.adf")).unwrap(),
            b"first",
            "the item that was begun is finished"
        );
        assert!(
            !root.join("Floppies").join("B.adf").exists(),
            "the next item was never begun"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

`"ART-CANCELLED"` is what `CoreError::Cancelled::code()` returns
(`src-tauri/src/core/error.rs:89`), checked while writing this plan rather
than assumed.

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd src-tauri && cargo test layout::apply:: 2>&1 | tail -20
```

Expected: FAIL — `apply` does not exist.

- [ ] **Step 3: Write the implementation**

```rust
//! Materialising a staging tree.
//!
//! Three rules, and each has a test that fails without it: the source is never
//! modified, nothing overwrites, and a destination is untrusted text that goes
//! through `safe_join` like an archive entry name — the user types it, and a
//! `../` in a text box is the same hole a `../` in a zip is.
//!
//! Cancellation is checked **between items and never inside one** (§54), so
//! stopping leaves whole files behind and never half of one.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::layout::{LayoutItem, LayoutPlan, Placement};
use crate::core::security::path::safe_join;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyOutcome {
    pub placed: usize,
    pub bytes: u64,
}

/// Build the staging tree the plan describes.
pub fn apply(plan: &LayoutPlan, sink: &dyn ProgressSink) -> CoreResult<ApplyOutcome> {
    let total = plan.items.len() as u64;
    let mut outcome = ApplyOutcome::default();

    for (done, item) in plan.items.iter().enumerate() {
        if sink.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        sink.report(done as u64, Some(total), &item.destination);

        let target = safe_join(&plan.root, &item.destination).map_err(|err| {
            CoreError::SafetyRefused(format!(
                "'{}' does not stay inside the staging folder: {err}",
                item.destination
            ))
        })?;
        if target.exists() {
            return Err(CoreError::InvalidInput(format!(
                "'{}' is already there; nothing is overwritten",
                item.destination
            )));
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }

        outcome.bytes += match item.placement {
            Placement::CopyFile => std::fs::copy(&item.source, &target)?,
            Placement::CopyTree => copy_tree(&item.source, &target)?,
            Placement::UnpackWhdload => {
                return Err(CoreError::InvalidInput(
                    "unpacking a WHDLoad archive is not implemented yet".into(),
                ))
            }
        };
        outcome.placed += 1;
    }

    sink.report(total, Some(total), "done");
    Ok(outcome)
}

/// Copy `from` to `to` recursively, creating nothing that is already there.
fn copy_tree(from: &Path, to: &Path) -> CoreResult<u64> {
    std::fs::create_dir_all(to)?;
    let mut bytes = 0;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        if std::fs::symlink_metadata(&source)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            continue;
        }
        let target: PathBuf = to.join(entry.file_name());
        if source.is_dir() {
            bytes += copy_tree(&source, &target)?;
        } else {
            bytes += std::fs::copy(&source, &target)?;
        }
    }
    Ok(bytes)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd src-tauri && cargo test layout::apply:: 2>&1 | tail -10
```

Expected: PASS, 5 tests.

- [ ] **Step 5: Commit**

```bash
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings
git add src-tauri/src/core/layout/apply.rs
git commit -m "SD-2 G11: build the staging tree, overwriting nothing"
```

---

### Task 5: Apply — unpacking a WHDLoad archive

**Files:**
- Modify: `src-tauri/src/core/layout/apply.rs`

**Interfaces:**
- Consumes: `core::archive::{open, extract_with_backend, OverwritePolicy}`, `core::whdload::{analyse, Entry, PackLayout}`
- Produces: no new public names; `Placement::UnpackWhdload` now works.

The shape is the one `commands/whdload.rs::build_plan` already uses: unpack
into a scratch directory, walk it, `analyse` the entry list, then place the
drawer — **and the icon beside the drawer, not inside it** (§82). Inside, the
game is on the disk and invisible on Workbench, which is indistinguishable
from a failed install.

- [ ] **Step 1: Write the failing test**

Add to `apply.rs`'s test module. The fixture is a ZIP built at runtime, so no
archive is checked in.

```rust
    /// Build a zip holding `Turrican/Turrican.slave`, `Turrican/data/level1`
    /// and `Turrican.info` beside the drawer — the shape a real WHDLoad
    /// archive has.
    fn whdload_zip(path: &Path) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, body) in [
            ("Turrican/Turrican.slave", &b"slave"[..]),
            ("Turrican/data/level1", &b"level"[..]),
            ("Turrican.info", &b"icon"[..]),
        ] {
            zip.start_file(name, options).unwrap();
            std::io::Write::write_all(&mut zip, body).unwrap();
        }
        zip.finish().unwrap();
    }

    /// **§82, in the staging tree.** The drawer lands under `Games/`, and its
    /// icon lands *beside* the drawer — not inside it, which would put the
    /// game on the disk and leave it invisible on Workbench.
    #[test]
    fn a_whdload_archive_unpacks_to_a_drawer_with_its_icon_beside_it() {
        let dir = scratch("unpack");
        let root = dir.join("staging");
        let archive = dir.join("Turrican.zip");
        whdload_zip(&archive);

        let plan = plan_of(
            &root,
            vec![LayoutItem {
                source: archive.clone(),
                kind: ItemKind::WhdloadArchive { name: "Turrican".into() },
                destination: "Games/Turrican".into(),
                placement: Placement::UnpackWhdload,
                bytes: 14,
            }],
        );

        apply(&plan, &NoProgress).unwrap();

        let games = root.join("Games");
        assert_eq!(
            std::fs::read(games.join("Turrican").join("Turrican.slave")).unwrap(),
            b"slave"
        );
        assert_eq!(
            std::fs::read(games.join("Turrican").join("data").join("level1")).unwrap(),
            b"level"
        );
        assert_eq!(
            std::fs::read(games.join("Turrican.info")).unwrap(),
            b"icon",
            "the icon sits beside the drawer, never inside it (§82)"
        );
        assert!(
            !games.join("Turrican").join("Turrican.info").exists(),
            "an icon inside the drawer is a game Workbench cannot see"
        );

        assert!(archive.exists(), "the archive is never consumed");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An archive holding no pack is an answer about the archive, not a fault
    /// in ART — but it cannot be placed as a drawer, so it is refused by name.
    #[test]
    fn an_archive_with_no_slave_is_refused_rather_than_half_placed() {
        let dir = scratch("nopack");
        let root = dir.join("staging");
        let archive = dir.join("Plain.zip");
        {
            let file = std::fs::File::create(&archive).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file("readme.txt", options).unwrap();
            std::io::Write::write_all(&mut zip, b"hello").unwrap();
            zip.finish().unwrap();
        }

        let plan = plan_of(
            &root,
            vec![LayoutItem {
                source: archive,
                kind: ItemKind::WhdloadArchive { name: "Plain".into() },
                destination: "Games/Plain".into(),
                placement: Placement::UnpackWhdload,
                bytes: 5,
            }],
        );

        assert!(apply(&plan, &NoProgress).is_err());
        assert!(!root.join("Games").join("Plain").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd src-tauri && cargo test layout::apply:: 2>&1 | tail -20
```

Expected: FAIL — "unpacking a WHDLoad archive is not implemented yet".

- [ ] **Step 3: Write the implementation**

Replace the `Placement::UnpackWhdload` arm in `apply` with
`unpack_whdload(&item.source, &target)?`, and add:

```rust
/// Unpack an archive holding a WHDLoad pack into `target`, which is the
/// drawer's own path — so the icon goes to `target`'s **parent**.
///
/// Everything decompressed goes through `core/archive`'s one gate first, into
/// a scratch directory, and only then is the pack's own shape worked out. The
/// two steps are separate because the gate's question is "is this archive
/// hostile" and `analyse`'s is "where is the game", and neither should be
/// asked in the other's terms.
fn unpack_whdload(archive: &Path, target: &Path) -> CoreResult<u64> {
    use crate::core::archive::{extract_with_backend, open, OverwritePolicy};
    use crate::core::jobs::NoProgress;
    use crate::core::whdload::{analyse, Entry};

    let scratch = target.with_extension("art-unpack");
    if scratch.exists() {
        return Err(CoreError::InvalidInput(format!(
            "'{}' is in the way of unpacking",
            scratch.display()
        )));
    }
    std::fs::create_dir_all(&scratch)?;

    let unpacked = (|| -> CoreResult<u64> {
        let mut backend = open(archive)?;
        extract_with_backend(&mut *backend, &scratch, OverwritePolicy::Skip, &NoProgress)?;

        let entries = walk_entries(&scratch, "", 0)?;
        let layout = analyse(&entries)?;

        // The drawer. `layout.root` is empty when the archive's own root is
        // the pack, in which case the scratch directory itself is the drawer.
        let drawer = if layout.root.is_empty() {
            scratch.clone()
        } else {
            safe_join(&scratch, &layout.root)?
        };
        let mut bytes = copy_tree(&drawer, target)?;

        // §82: beside the drawer, never inside it.
        if let Some(icon) = &layout.icon {
            let from = safe_join(&scratch, icon)?;
            if let Some(parent) = target.parent() {
                let to = parent.join(layout.icon_name());
                if !to.exists() {
                    bytes += std::fs::copy(&from, &to)?;
                }
            }
        }
        Ok(bytes)
    })();

    let _ = std::fs::remove_dir_all(&scratch);
    unpacked
}

/// The unpacked tree as the list of names `analyse` reads.
fn walk_entries(base: &Path, relative: &str, depth: usize) -> CoreResult<Vec<Entry>> {
    use crate::core::layout::scan::MAX_SCAN_DEPTH;

    let mut out = Vec::new();
    if depth >= MAX_SCAN_DEPTH {
        return Ok(out);
    }
    let here = if relative.is_empty() {
        base.to_path_buf()
    } else {
        safe_join(base, relative)?
    };
    for entry in std::fs::read_dir(&here)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let child = if relative.is_empty() {
            name
        } else {
            format!("{relative}/{name}")
        };
        if entry.file_type()?.is_dir() {
            out.push(Entry::dir(&child));
            out.extend(walk_entries(base, &child, depth + 1)?);
        } else {
            out.push(Entry::file(&child));
        }
    }
    Ok(out)
}
```

Add `use crate::core::whdload::Entry;` to the imports at the top of the file if
clippy asks for it, and remove the now-unused error arm.

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd src-tauri && cargo test layout:: 2>&1 | tail -10
```

Expected: PASS, all layout tests.

- [ ] **Step 5: Add the hostile-archive case**

Every archive path in ART gets one. Add to the test module:

```rust
    /// The gate every archive goes through, exercised on this path too: an
    /// entry naming its way out of the destination is refused, and nothing
    /// lands outside the staging tree.
    #[test]
    fn a_traversing_entry_never_escapes_the_staging_tree() {
        let dir = scratch("hostile");
        let root = dir.join("staging");
        let archive = dir.join("Evil.zip");
        {
            let file = std::fs::File::create(&archive).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            for name in ["Evil/Evil.slave", "../../escaped.txt"] {
                zip.start_file(name, options).unwrap();
                std::io::Write::write_all(&mut zip, b"x").unwrap();
            }
            zip.finish().unwrap();
        }

        let plan = plan_of(
            &root,
            vec![LayoutItem {
                source: archive,
                kind: ItemKind::WhdloadArchive { name: "Evil".into() },
                destination: "Games/Evil".into(),
                placement: Placement::UnpackWhdload,
                bytes: 2,
            }],
        );

        // Whether the pack places or the archive is refused, one thing must
        // hold: nothing is written outside the staging tree.
        let _ = apply(&plan, &NoProgress);
        assert!(!dir.join("escaped.txt").exists());
        assert!(!std::env::temp_dir().join("escaped.txt").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
```

Run `cd src-tauri && cargo test layout::apply::tests::a_traversing_entry -v` —
expected PASS, because `core/archive/extract.rs` already refuses the entry.

- [ ] **Step 6: Commit**

```bash
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test layout::
git add src-tauri/src/core/layout/apply.rs
git commit -m "SD-2 G11: unpack a WHDLoad archive, icon beside the drawer"
```

---

### Task 6: The commands, and the typed wrapper

**Files:**
- Create: `src-tauri/src/commands/layout.rs`
- Modify: `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs`
- Create: `src/lib/layout.ts`, `src/lib/layout.test.ts`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json`, `src/i18n/phrase-keys.test.ts`

**Interfaces:**
- Consumes: `core::layout::{apply, plan, LayoutPlan, Policy}`
- Produces:
  - Rust: `layout_plan(request: LayoutRequest) -> AppResult<LayoutPlan>`, `layout_apply(plan: LayoutPlan) -> AppResult<u64>`, `LAYOUT_EVENT = "layout-result"`
  - TS: `layoutPlan(request)`, `layoutApply(plan)`, `onLayoutResult(handler)`, `retarget(plan, indices, drawer)`, `layoutBlocker(input)`, `refusalPhrase(reason)`, `kindPhrase(kind)`

- [ ] **Step 1: Write the failing Rust test**

`layout_apply` takes the **edited** plan, because the whole point is that the
user changed it — unlike `preload_run`, which recomputes. Write the wire test
first, the way `commands/preload.rs` now has one:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// **The wire, written down.** `src/lib/layout.ts` builds this object by
    /// hand; nothing else in either build checks that the two agree.
    #[test]
    fn the_payload_the_frontend_sends_deserialises() {
        let request: LayoutRequest = serde_json::from_str(
            r#"{"root":"E:\\amiga\\ProjeART\\staging",
                "paths":["E:\\amiga\\Games"],
                "policy":{"whdload":"unpack","games":"Games","floppies":"Floppies",
                          "hard_disks":"HardDisks","discs":"CDs","unsorted":"Unsorted"}}"#,
        )
        .expect("the shape src/lib/layout.ts sends");

        assert_eq!(request.paths.len(), 1);
        assert_eq!(request.policy.games, "Games");
        assert_eq!(
            request.policy.whdload,
            crate::core::layout::policy::WhdloadPlacement::Unpack
        );
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cd src-tauri && cargo test commands::layout:: 2>&1 | tail -20
```

Expected: FAIL — the module does not exist.

- [ ] **Step 3: Write the commands**

```rust
//! Laying content out into a staging tree (SD-2 · G11).
//!
//! Two commands and one difference from `commands/preload.rs` worth stating:
//! **`layout_apply` takes the plan it is given rather than recomputing it.**
//! Preload recomputes because a screen must not be able to preview one card
//! and format another. Here the user's edits *are* the plan — retargeting rows
//! is the feature — so recomputing would throw away exactly what they came to
//! do. What protects the tree instead is the applier: `safe_join` on every
//! destination, and a refusal on anything already there.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::core::layout::{apply, plan, LayoutPlan};
use crate::core::layout::policy::Policy;
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::error::AppResult;

use super::jobs::{spawn_job, JobRegistry};
use super::oplog::{user_operation, write_to_path};

#[derive(Debug, Clone, Deserialize)]
pub struct LayoutRequest {
    pub root: PathBuf,
    pub paths: Vec<PathBuf>,
    pub policy: Policy,
}

/// What laying these out would do. Writes nothing (§92's PREVIEW).
#[tauri::command]
pub fn layout_plan(request: LayoutRequest) -> AppResult<LayoutPlan> {
    Ok(plan(&request.root, &request.paths, &request.policy)?)
}

pub const LAYOUT_EVENT: &str = "layout-result";

#[derive(Debug, Clone, Serialize)]
pub struct LayoutResult {
    pub job_id: u64,
    pub root: String,
    pub outcome: crate::core::layout::apply::ApplyOutcome,
}

/// Build the staging tree. Returns a job id (§54).
#[tauri::command]
pub fn layout_apply(
    plan: LayoutPlan,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<u64> {
    let root = plan.root.display().to_string();
    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!("Laying {} item(s) out in {root}", plan.items.len());
    let for_log = root.clone();

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let outcome = apply(&plan, progress);

        let record = user_operation("Lay content out into a staging folder")
            .destination(&for_log)
            .detail("Items", plan.items.len().to_string());
        let record = match &outcome {
            Ok(done) => record
                .detail("Placed", done.placed.to_string())
                .detail("Bytes", done.bytes.to_string())
                // Every file is read back by nothing: the tree is on the PC
                // and the user can open it. Verification belongs to whatever
                // puts it on the card.
                .outcome(OperationOutcome::verified(false)),
            Err(err) => record.failure(err.code(), err.to_string()),
        };
        write_to_path(&log_path, &record);

        let outcome = outcome?;
        let _ = emit_app.emit(
            LAYOUT_EVENT,
            LayoutResult {
                job_id,
                root: for_log,
                outcome,
            },
        );
        Ok(())
    });

    Ok(id)
}
```

`LayoutPlan` must gain `Deserialize` (it has it from Task 3). Add
`pub mod layout;` to `src-tauri/src/commands/mod.rs` and both commands to
`invoke_handler![]` in `src-tauri/src/lib.rs`.

- [ ] **Step 4: Run the Rust test**

```bash
cd src-tauri && cargo test commands::layout:: 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 5: Write the failing frontend test**

`src/lib/layout.test.ts`:

```ts
import { describe, expect, it } from "vitest";

import {
  layoutBlocker,
  retarget,
  type LayoutPlan,
} from "@/lib/layout";

const PLAN: LayoutPlan = {
  root: "E:\\staging",
  items: [
    {
      source: "E:\\a\\Turrican.lha",
      kind: { kind: "whdload-archive", name: "Turrican" },
      destination: "Games/Turrican",
      placement: "unpack-whdload",
      bytes: 100,
    },
    {
      source: "E:\\a\\Mega.lha",
      kind: { kind: "archive" },
      destination: "Unsorted/Mega.lha",
      placement: "copy-file",
      bytes: 50,
    },
  ],
  refused: [],
  collisions: [],
  bytes: 150,
};

describe("retarget", () => {
  it("moves the chosen rows to another drawer, keeping each leaf name", () => {
    const next = retarget(PLAN, [1], "Demos");
    expect(next.items[1].destination).toBe("Demos/Mega.lha");
    expect(next.items[0].destination).toBe("Games/Turrican", );
  });

  it("recomputes the collisions, because a retarget can make one", () => {
    const plan: LayoutPlan = {
      ...PLAN,
      items: [
        { ...PLAN.items[0], destination: "Games/Same" },
        { ...PLAN.items[1], destination: "Unsorted/Same" },
      ],
    };
    const next = retarget(plan, [1], "Games");
    expect(next.collisions.map((c) => c.destination)).toEqual(["Games/Same"]);
  });

  it("leaves the plan alone when no row was chosen", () => {
    expect(retarget(PLAN, [], "Demos")).toEqual(PLAN);
  });
});

describe("layoutBlocker", () => {
  const ready = { root: "E:\\staging", paths: ["E:\\a"], plan: PLAN };

  it("is clear when a root, some paths and a plan are in hand", () => {
    expect(layoutBlocker(ready)).toBeNull();
  });

  it("asks for the staging folder first", () => {
    expect(layoutBlocker({ ...ready, root: null })?.key).toBe("layout.blocked.noRoot");
  });

  it("asks for something to lay out", () => {
    expect(layoutBlocker({ ...ready, paths: [] })?.key).toBe("layout.blocked.nothingToPlace");
  });

  it("asks for a preview before writing anything", () => {
    expect(layoutBlocker({ ...ready, plan: null })?.key).toBe("layout.blocked.notPlanned");
  });

  it("will not apply a plan with a collision in it", () => {
    const clashing: LayoutPlan = {
      ...PLAN,
      collisions: [{ destination: "Games/Turrican", sources: ["a", "b"] }],
    };
    expect(layoutBlocker({ ...ready, plan: clashing })?.key).toBe("layout.blocked.collisions");
  });

  it("will not apply a plan that would place nothing", () => {
    expect(layoutBlocker({ ...ready, plan: { ...PLAN, items: [] } })?.key).toBe(
      "layout.blocked.nothingToPlace"
    );
  });
});
```

- [ ] **Step 6: Run it to verify it fails**

```bash
npx vitest run src/lib/layout.test.ts
```

Expected: FAIL — the module does not exist.

- [ ] **Step 7: Write `src/lib/layout.ts`**

Mirror the Rust types exactly (`kind` tagged, kebab-case), then:

```ts
/** Move the chosen rows into `drawer`, keeping each one's own leaf name. */
export function retarget(plan: LayoutPlan, indices: number[], drawer: string): LayoutPlan {
  if (indices.length === 0) return plan;
  const chosen = new Set(indices);
  const items = plan.items.map((item, index) => {
    if (!chosen.has(index)) return item;
    const leaf = item.destination.split("/").pop() ?? item.destination;
    return { ...item, destination: `${drawer}/${leaf}` };
  });
  return { ...plan, items, collisions: collisionsIn(items) };
}

/**
 * Destinations two rows want.
 *
 * **Only the ones inside this plan.** A destination the staging tree already
 * holds is a fact about the disk, and only the engine has looked at the disk —
 * so those survive from the last `layout_plan` and are recomputed when the
 * user previews again.
 */
function collisionsIn(items: LayoutItem[]): Collision[] {
  const by = new Map<string, string[]>();
  for (const item of items) {
    by.set(item.destination, [...(by.get(item.destination) ?? []), item.source]);
  }
  return [...by.entries()]
    .filter(([, sources]) => sources.length > 1)
    .map(([destination, sources]) => ({ destination, sources }));
}
```

plus `layoutBlocker` returning the five keys the test names, and
`refusalPhrase` / `kindPhrase` mapping every Rust variant to a key.

- [ ] **Step 8: Add the keys to both catalogues and the phrase test**

Add a `"layout"` block to `src/i18n/en.json` **and** `src/i18n/tr.json` with
matching key sets, and enumerate `refusalPhrase`, `kindPhrase` and every
`layoutBlocker` reason in `src/i18n/phrase-keys.test.ts` the way
`stepPhrase`/`preloadBlocker` are.

- [ ] **Step 9: Run everything**

```bash
npx pnpm lint && npx vitest run
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Expected: all green.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/commands/ src-tauri/src/lib.rs src/lib/layout.ts src/lib/layout.test.ts src/i18n/
git commit -m "SD-2 G11: the layout commands, and the wire to them"
```

---

### Task 7: The screen

**Files:**
- Create: `src/pages/ContentLayout.tsx`
- Modify: `src/App.tsx`, `src/components/layout/Sidebar.tsx`, `src/i18n/{en,tr}.json`, `src/i18n/literal-keys.test.ts`

**Interfaces:**
- Consumes: everything Task 6 produced.
- Produces: the route `/layout`.

- [ ] **Step 1: Build the screen**

Follow `VolumePreload.tsx` for the shape — remembered paths through
`useRemembered`, a plan invalidated by a request fingerprint, a job-backed
apply whose result arrives on an event.

What is different here, and is the point of the feature:

- The preview is a **table with a checkbox per row**, and a drawer dropdown
  above it that retargets every checked row at once through `retarget`.
  The dropdown lists the policy's five drawers plus a free-text box.
- Refusals are their own list under the table, each with its reason. They are
  not rows to retarget: a Kickstart has no drawer, and offering one would
  undo the refusal.
- Collisions are shown in red and **block Apply**, because the applier refuses
  and finding that out halfway through is worse than not starting.
- The total is `bytes` from the plan, said before the button.
- After a successful apply: what landed, where the staging folder is, and one
  line saying to point the preload screen at it. No automatic chaining.

- [ ] **Step 2: Route and sidebar**

Add `<Route path="/layout" element={<ContentLayout />} />` to `src/App.tsx` and
a sidebar entry with the `nav.layout` key in both catalogues.

- [ ] **Step 3: Update the dynamic-call count**

`src/i18n/literal-keys.test.ts` counts `t()` calls whose key is not a literal.
Run `npx vitest run src/i18n/literal-keys.test.ts`, take the number it reports,
and update `expect(dynamicCalls).toBe(N)` **with a comment saying which calls
were added and why**, the way every previous entry in that list does.

- [ ] **Step 4: Run everything**

```bash
npx pnpm lint && npx vitest run
```

- [ ] **Step 5: Drive the screen in a real browser**

The engine passing its tests is not the screen working. Use the probe shape
from the preload session: a throwaway HTML page served by Vite that mounts the
app, navigates to `#/layout`, and dumps `.app-content`'s `innerText`. Check
for raw catalogue keys (`layout.…` on screen) and for literal `{{variable}}`.

```bash
npx vite --port 5199 --strictPort   # in the background
```

- [ ] **Step 6: Commit**

```bash
git add src/pages/ContentLayout.tsx src/App.tsx src/components/layout/Sidebar.tsx src/i18n/
git commit -m "SD-2 G11: the layout screen, with a preview you can change"
```

---

### Task 8: Documentation

**Files:**
- Modify: `docs/STATUS.md`, `docs/FEATURES.md`, `CHANGELOG.md`

- [ ] **Step 1: Update the three**

- `docs/FEATURES.md` — a row for G11 under the SD-2 rows, `✅` only if the
  tests exist, naming `core/layout/` and the screen.
- `docs/STATUS.md` — the snapshot's Rust and frontend counts (run the suites
  and copy the real numbers, do not estimate), the i18n leaf count, the SD-2
  row in the phase table, and a session-log line at the top of the table.
- `CHANGELOG.md` — a user-visible entry under `## [Unreleased]`, in the voice
  the entries above it use: what you can now do, and what ART does not claim.

State plainly whichever of these is true: whether the screen was driven
against real material, and whether a staging tree it built was actually
carried onto a card. If not, say so — that is the rung this project does not
let a green suite cover for.

- [ ] **Step 2: Commit**

```bash
git add docs/ CHANGELOG.md
git commit -m "SD-2 G11 lands: what goes where"
```

---

## Self-review

**Spec coverage.** Staging-tree output → Tasks 4/5. No volume concept →
destinations are relative strings throughout. The drawer table → Task 2. ROM
and C64 refused with a reason → Task 2 + Task 3's test. WHDLoad unpack with
the icon beside the drawer → Task 5. `AsArchive` switch → Task 2's
`WhdloadPlacement`, used in Task 3's `plan`. Folder walked, drawer whole →
Task 1. `bytes` reported → Task 3. Editable preview → Tasks 6 (`retarget`) and
7. Cancellation between items → Task 4. Source unchanged → Tasks 4 and 5.
Hostile archive → Task 5 Step 5. Out-of-scope items appear nowhere, as
intended.

**Two problems found and fixed inline.** The spec requires cancellation
between items and no task tested it — Task 4's test module now carries
`stopping_leaves_the_finished_item_whole_and_the_next_one_absent`, with the
sink that makes it happen. And Task 4's implementation carried an
`item_source` helper that did nothing, followed by a step telling the
implementer to delete it; both are gone. A plan that ships code to be removed
is a plan asking to be half-followed.

**Type consistency.** `ItemKind`, `Placement`, `LayoutItem`, `Refusal`,
`Collision`, `LayoutPlan`, `Policy`, `WhdloadPlacement`, `ApplyOutcome` are
defined once and used with the same field names throughout. `ItemKind` is
introduced in Task 3 but needed by Task 2's tests — Task 2 Step 4 says so
explicitly and tells the implementer to add it there.
