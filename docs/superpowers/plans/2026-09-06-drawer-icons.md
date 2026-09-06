# Drawer Icons Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A tree ART builds carries its drawers' own icons, and the icons that arrive with no position get laid out instead of being scattered by Workbench.

**Architecture:** One fix in `core/osinstall/plan.rs` so a `Subtree` rule takes the sibling `.info` it has always dropped; `core/amigaicon` grows the reads it deliberately skipped and the writes it never had; a pure `core/icongrid` does the arithmetic; and `core/appearance::apply_appearance` — round 1's applier, already reachable from the OS Builder — gains the flag that calls them.

**Tech Stack:** Rust (`std` + existing core deps only — no new crates), React + `react-i18next`, Vitest, and the existing `scripts/icon-oracle-check.py`.

**Spec:** [docs/superpowers/specs/2026-09-06-drawer-icons-design.md](../specs/2026-09-06-drawer-icons-design.md)

## Global Constraints

- **`core/` stays platform-independent.** No `use tauri`, no Windows API, no network. **No new dependencies** — everything here is `std` plus what `core/` already carries.
- **MSRV 1.93.** `cargo clippy --all-targets -- -D warnings` is blocking; `lib.rs` allows only `dead_code`.
- **Bounds before indexing.** `panic = "abort"` is set in release, so an out-of-range index kills the whole application. Every offset is computed with `checked_add`/`checked_mul` and validated against the real buffer before slicing. This module tree parses files ART did not write; a file that does not parse is refused with `CoreError`, never read past its bounds and never rewritten best-effort.
- **Preserve everything not named.** Every write rebuilds the file with every byte it was not asked to change carried through — appended ColorIcon and NewIcon blobs included. Round 1 paid three fix rounds to learn this on `PTRN` flags, `wbp_Revision` and `SCRM`'s reserved bytes; here it is the premise.
- **Every write goes through `core/safety`** — `guarded_write` with a `BackupPolicy`, never `std::fs::write` on a file in the tree.
- **ART ships no copyrighted Amiga content.** Unit fixtures are synthetic and built at runtime; the real-material checks are the `#[ignore]`d, env-gated oracle hooks.
- **Test scratch is `core::ScratchDir`**, self-removing on `Drop`. Never write a trailing `remove_dir_all` — it is skipped exactly when a test panics.
- `CoreError` messages stay English (ART-060). New UI strings go in **both** `src/i18n/en.json` and `tr.json` in the same commit.
- **Measured constants only.** Every offset below was read out of a real file (spec §1.3). Do not "correct" one from memory or from another project's source.

## The three numbers that decide this round

State them in the code where they bite, because a plausible guess gets each one wrong:

1. **`0x80000000`, not `0`, is "no position"** (`do_CurrentX`). 361 of the owner's 798 real icons carry it. Treating `0` as unset moves icons the release deliberately placed.
2. **The `Gadget` size is not the rendered size** in 613 of 798 — 77 %. The extreme is a NewIcon whose `Gadget` says `3x3` and whose `IM1=` says `46x46`.
3. **A `Subtree` rule's `from` names a drawer whose icon is its *sibling*.** Nested icons already come across; only the drawer's own is lost.

---

## File structure

| File | Responsibility |
|---|---|
| `src-tauri/src/core/osinstall/plan.rs` | **modify** — `expand_rules` also emits the drawer's own `.info` |
| `src-tauri/src/core/amigaicon/mod.rs` | **modify** — new reads and writes; keep the existing API intact |
| `src-tauri/src/core/amigaicon/render.rs` | rendered size: ColorIcon `FACE`, NewIcons `IM1=`, fallback |
| `src-tauri/src/core/icongrid/mod.rs` | the arithmetic — pure, no I/O, no filesystem |
| `src-tauri/src/core/appearance/mod.rs` | **modify** — `arrange_icons`, the first real caller |
| `src-tauri/src/commands/appearance.rs` | **modify** — carry the flag |
| `src/lib/appearance.ts`, `src/components/osbuilder/AppearancePanel.tsx` | **modify** — one checkbox |
| `scripts/icon-oracle-check.py` | **modify** — prove writes preserve, over the real corpus |

**Reachability.** Tasks 2-5 build primitives. **Task 6 owns their first real caller**, Task 8 owns the frontend flag. When Task 8 is done, trace the chain from the panel's checkbox to `core::icongrid::arrange` hop by hop and record the hops in the commit message. This project has twice shipped a feature no caller could reach.

---

### Task 1: A `Subtree` rule takes the drawer's own icon

**Files:**
- Modify: `src-tauri/src/core/osinstall/plan.rs` (`expand_rules`, around line 1145)

**Interfaces:**
- Consumes: the existing `PlanItem { component, media, from, to, is_dir, bytes, decompress, merge_icon }` and whatever `expand_rules` already uses to list a medium.
- Produces: no new public API — one more `PlanItem` per `Subtree` rule whose medium holds the sibling icon.

**Read first:** `expand_rules` itself, and the module doc at the top of `plan.rs` — it explains why collisions are detected over the **expanded** item list and why a coinciding `Subtree` destination is a legal merge point rather than a claim. Your new item is an ordinary file item and inherits both behaviours; do not add a parallel path.

- [ ] **Step 1: Write the failing tests**

Build a synthetic medium (follow whatever helper the existing `plan.rs` tests use for one — read them first; do not invent a second fixture style).

```rust
#[test]
fn a_subtree_rule_also_places_the_drawers_own_icon() {
    // A medium holding `Prefs/` and, beside it, `Prefs.info` - which is what
    // Workbench3.2.adf's root actually looks like (design doc 1.1).
    // A Subtree rule from "Prefs" to "Prefs" must produce an item for
    // `Prefs.info` -> `Prefs.info` as well as the subtree's own files.
}

#[test]
fn the_icon_item_is_attributed_to_the_same_component_and_medium() {
    // Provenance: distribution.json must be able to say where it came from.
}

#[test]
fn a_subtree_rule_whose_medium_has_no_sibling_icon_places_none() {
    // No icon on the medium, no item, no refusal - this is the ordinary case
    // for a drawer the release ships without an icon.
}

#[test]
fn a_from_empty_subtree_rule_takes_no_icon() {
    // The `fonts` and `backdrops` rules copy a whole medium. Its root icon is
    // `Disk.info`, a VOLUME icon (do_Type 1), not a drawer icon (do_Type 2).
    // Handing a drawer a disk icon is not "the icon it should have".
}

#[test]
fn the_icon_item_is_a_file_not_a_directory() {
    // is_dir false, and `bytes` is the icon's real size on the medium - so the
    // preview's total is right.
}

#[test]
fn two_components_placing_the_same_drawer_icon_is_a_collision_like_any_other() {
    // The existing detect_collisions runs over the expanded list; this item
    // must be visible to it rather than bypassing it.
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test osinstall::plan`
Expected: FAIL — the icon items do not exist.

- [ ] **Step 3: Write the implementation**

In `expand_rules`, for a rule of kind `Subtree` whose `from` is **not empty**: after expanding the subtree, ask the medium for `<from>.info`. If it holds one, emit one more `PlanItem` with `from: "<from>.info"`, `to: "<to>.info"`, `is_dir: false`, `bytes` from the medium, `decompress` decided the same way every other file item decides it, and `merge_icon: false`.

Document, at the emission site, **why `from: ""` is excluded** — measured, spec §2.1 — so nobody later "completes" it.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test osinstall::` — the whole module, because this changes what every recipe produces.
Expected: PASS. **If an existing test's expected item count changes, do not simply bump it** — say so explicitly in your report and justify each change. Editing a test until it goes green is the most dangerous move in a diff, and this round has already caught one such fixture that was wrong all along.

- [ ] **Step 5: Measure it against real material**

Extend or add an `#[ignore]`d hook that runs the real 3.2 build and **counts the root-level `.info` files in the result**. The design measured 19 root-level icons across five disks and 0 in the built tree; report what the number is now. A count is the claim; "it works" is not.

- [ ] **Step 6: Mutate the guards**

| Mutation | Must fail |
|---|---|
| emit the icon item for `from: ""` rules too | `a_from_empty_subtree_rule_takes_no_icon` |
| emit it with `is_dir: true` | `the_icon_item_is_a_file_not_a_directory` |
| attribute it to a fixed component name | `the_icon_item_is_attributed_to_the_same_component_and_medium` |
| skip the medium check and always emit | `a_subtree_rule_whose_medium_has_no_sibling_icon_places_none` |

Restore with `shutil.copyfile` or `touch`, never `shutil.move` — it loses the mtime and cargo then compiles the mutation.

- [ ] **Step 7: Commit** — only the files you changed, by explicit path; `git commit -F <a message file>`.

---

### Task 2: `core/amigaicon` reads what it deliberately skipped

**Files:**
- Modify: `src-tauri/src/core/amigaicon/mod.rs`

**Interfaces:**
- Produces:
  - `pub enum IconType { Disk, Drawer, Tool, Project, Garbage, Other(u8) }` — from `do_Type` at 48; measured values 1, 2, 3, 4, 5
  - `pub const NO_POSITION: i32 = i32::MIN;` — `0x80000000`
  - `pub fn icon_type(bytes: &[u8]) -> CoreResult<IconType>`
  - `pub fn position(bytes: &[u8]) -> CoreResult<Option<(i32, i32)>>` — `None` when either coordinate is `NO_POSITION`
  - `pub struct DrawerWindow { pub left: i16, pub top: i16, pub width: i16, pub height: i16 }`
  - `pub fn drawer_window(bytes: &[u8]) -> CoreResult<Option<DrawerWindow>>` — `None` when `do_DrawerData` at 66 is zero
  - `pub fn has_drawer_data2(bytes: &[u8]) -> CoreResult<bool>` — the revision bit, `UserData` at 44, bit 0

**Read first:** the module doc at the top of `mod.rs`. It states which offsets were measured and which blocks it skips without reading; you are converting some of the second list into the first, so extend that table rather than starting a new one.

- [ ] **Step 1: Write the failing tests**

Use the existing `tests_support::synthetic_icon` builder where it fits; extend it rather than writing a second builder.

```rust
#[test]
fn the_no_position_sentinel_is_0x80000000_not_zero() {
    // Measured: art1/Fonts/Disk.info carries i32::MIN in do_CurrentX.
    // A position of (0, 0) is a REAL position - the window's top-left corner -
    // and must not be read as "unset".
    let placed = synthetic_icon_at(0, 0);
    assert_eq!(position(&placed).unwrap(), Some((0, 0)), "(0,0) is a real position");

    let unplaced = synthetic_icon_at(NO_POSITION, NO_POSITION);
    assert_eq!(position(&unplaced).unwrap(), None);
}

#[test]
fn a_real_position_reads_back_exactly() {
    // Measured: art1/Devs/DataTypes.info is at 13,4.
    assert_eq!(position(&synthetic_icon_at(13, 4)).unwrap(), Some((13, 4)));
}

#[test]
fn the_five_measured_icon_types_decode() {
    // Measured across 798 real icons: 1 disk (37), 2 drawer (56),
    // 3 tool (196), 4 project (506), 5 garbage (3).
    for (byte, want) in [
        (1u8, IconType::Disk),
        (2, IconType::Drawer),
        (3, IconType::Tool),
        (4, IconType::Project),
        (5, IconType::Garbage),
    ] {
        assert_eq!(icon_type(&synthetic_icon_of_type(byte)).unwrap(), want);
    }
    assert_eq!(icon_type(&synthetic_icon_of_type(9)).unwrap(), IconType::Other(9));
}

#[test]
fn a_drawer_window_reads_only_when_drawer_data_is_present() {
    // Measured: art1/Devs/DataTypes.info -> 393,126 342x163; the project icon
    // beside it has do_DrawerData = 0 and no window at all.
    assert_eq!(
        drawer_window(&synthetic_drawer_icon(393, 126, 342, 163)).unwrap(),
        Some(DrawerWindow { left: 393, top: 126, width: 342, height: 163 })
    );
    assert_eq!(drawer_window(&synthetic_icon(&[], 4096, &[])).unwrap(), None);
}

#[test]
fn every_real_icon_measured_is_revision_one() {
    // 798 of 798. So DrawerData2 is the normal case, not a special one.
    assert!(has_drawer_data2(&synthetic_drawer_icon(0, 0, 100, 100)).unwrap());
}

#[test]
fn a_truncated_icon_is_refused_rather_than_read_past_its_end() {
    // Truncate a VALID icon. A zero-filled buffer would be refused for its
    // missing 0xE310 magic rather than for its length, so such a test passes
    // with every bounds check deleted - a state with more than one cause,
    // which is the defect this project names.
    let whole = synthetic_icon_at(13, 4);
    assert!(icon_type(&whole).is_ok(), "the fixture must be valid to start with");
    for len in [12usize, 45, 48, 57, 62, 65] {
        let cut = &whole[..len];
        assert!(
            icon_type(cut).is_err() || len > 48,
            "icon_type read do_Type out of {len} bytes"
        );
        assert!(
            position(cut).is_err() || len > 65,
            "position read a coordinate out of {len} bytes"
        );
    }
}
```

- [ ] **Step 2: Run to verify they fail** — `cd src-tauri && cargo test amigaicon::`

- [ ] **Step 3: Implement.** Offsets, all measured (spec §1.3): `UserData` 44, `do_Type` 48, `do_CurrentX/Y` 58/62 as `i32`, `do_DrawerData` 66, `DrawerData.NewWindow` 78 as `>hhhh`. Bound-check before every read.

- [ ] **Step 4: Run to verify they pass** — expect the new tests plus the module's existing ones, unchanged.

- [ ] **Step 5: Mutate**

| Mutation | Must fail |
|---|---|
| treat `0` as "no position" | `the_no_position_sentinel_is_0x80000000_not_zero` |
| read the position at 56 instead of 58 | `a_real_position_reads_back_exactly` |
| return a window regardless of `do_DrawerData` | `a_drawer_window_reads_only_when_drawer_data_is_present` |
| drop a length check | `a_truncated_icon_is_refused_rather_than_read_past_its_end` |

- [ ] **Step 6: Commit**

---

### Task 3: The rendered size — the number a layout actually needs

**Files:**
- Create: `src-tauri/src/core/amigaicon/render.rs`
- Modify: `src-tauri/src/core/amigaicon/mod.rs` — `pub mod render;`

**Interfaces:**
- Produces:
  - `pub struct Rendered { pub width: u16, pub height: u16, pub framed: bool }`
  - `pub fn rendered_size(bytes: &[u8]) -> CoreResult<Rendered>`

**The rule, measured (spec §1.3), in this order:**
1. **ColorIcon** — find `FORM` … `ICON`, then its `FACE` chunk: width `data[face+8] + 1`, height `data[face+9] + 1`, frameless when `data[face+10] & 1`. **Only trust `FACE` when an `IMAG` chunk backs it** — MagicWB-era icons (MUI 3.8, MagicMenu) end in a degenerate `FACE` claiming 256×256 with no image data.
2. **NewIcons** — the first `IM1=` tool type: width `data[im1+5] - 0x21`, height `data[im1+6] - 0x21`.
3. **Otherwise** the `Gadget` width and height at 12/14.

In every case the result is `max(gadget, found)`, never smaller than the gadget.

- [ ] **Step 1: Write the failing tests**

Rust has no named arguments; the calls below are written with positional
parameters in the order `(gadget_w, gadget_h, face_w, face_h, with_imag)` and
`(gadget_w, gadget_h, im1_w, im1_h)`. Write the builders with those signatures.

```rust
#[test]
fn a_colour_icon_uses_its_face_chunk_not_its_gadget_size() {
    // Measured: art1/Devs/DataTypes.info says 44x44 in the Gadget and 46x46
    // in FACE - which is why a layout cannot use the Gadget fields.
    let icon = synthetic_colour_icon(44, 44, 46, 46, true);
    let r = rendered_size(&icon).unwrap();
    assert_eq!((r.width, r.height), (46, 46));
}

#[test]
fn a_degenerate_face_with_no_imag_is_not_trusted() {
    // MagicWB-era icons claim 256x256 in FACE with no image behind it.
    // Believing that makes one icon eat a whole window.
    let icon = synthetic_colour_icon(32, 32, 256, 256, false);
    let r = rendered_size(&icon).unwrap();
    assert_eq!((r.width, r.height), (32, 32), "fall back to the planar size");
}

#[test]
fn a_newicon_uses_its_im1_tooltype_not_its_stub_gadget() {
    // Measured: art1/Prefs/Presets/Animated GIFs/Amiga.gif.info has a 3x3
    // Gadget and a 46x46 IM1= - fifteen times larger. This single case is why
    // the round exists.
    let icon = synthetic_newicon(3, 3, 46, 46);
    let r = rendered_size(&icon).unwrap();
    assert_eq!((r.width, r.height), (46, 46));
}

#[test]
fn a_plain_planar_icon_uses_its_gadget_size() {
    // Assert the EXACT size the fixture was built with. "greater than zero"
    // would pass for any wrong answer, including a hard-coded one.
    let icon = synthetic_icon_sized(35, 18);
    let r = rendered_size(&icon).unwrap();
    assert_eq!((r.width, r.height), (35, 18));
}

#[test]
fn the_result_is_never_smaller_than_the_gadget() {
    let icon = synthetic_colour_icon(64, 64, 16, 16, true);
    let r = rendered_size(&icon).unwrap();
    assert_eq!((r.width, r.height), (64, 64));
}

#[test]
fn a_truncated_face_or_im1_is_refused_not_read_past() {
    // A FACE header claiming to be there with fewer than three bytes behind it.
}
```

- [ ] **Step 2: Run to verify they fail** — `cd src-tauri && cargo test amigaicon::render`

- [ ] **Step 3: Implement.** Search for the markers with bounds checks; never index a found offset without confirming the bytes behind it exist.

- [ ] **Step 4: Run to verify they pass**

- [ ] **Step 5: Mutate**

| Mutation | Must fail |
|---|---|
| drop the `IMAG` requirement | `a_degenerate_face_with_no_imag_is_not_trusted` |
| ignore `IM1=` | `a_newicon_uses_its_im1_tooltype_not_its_stub_gadget` |
| forget the `+ 1` on `FACE`'s width | `a_colour_icon_uses_its_face_chunk_not_its_gadget_size` |
| return the found size without `max` | `the_result_is_never_smaller_than_the_gadget` |

- [ ] **Step 6: Commit**

---

### Task 4: The writers

**Files:**
- Modify: `src-tauri/src/core/amigaicon/mod.rs`

**Interfaces:**
- Produces:
  - `pub fn set_tooltypes(bytes: &[u8], tooltypes: &[String]) -> CoreResult<Vec<u8>>`
  - `pub fn set_position(bytes: &[u8], at: Option<(i32, i32)>) -> CoreResult<Vec<u8>>` — `None` writes `NO_POSITION` into both
  - `pub fn set_window(bytes: &[u8], window: DrawerWindow) -> CoreResult<Vec<u8>>` — refuses when `do_DrawerData` is zero, because there is no window to set
  - `pub fn set_show_all_files(bytes: &[u8], show_all: bool) -> CoreResult<Vec<u8>>`

**The rule that governs all four:** every byte the call was not asked to change comes out identical — the appended ColorIcon or NewIcon blob, the images, the `DefaultTool`, the stack size, everything. `merge_tooltypes` already demonstrates the technique in this module; follow it.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn setting_a_position_changes_eight_bytes_and_nothing_else() {
    let before = synthetic_icon(&["A=1", "B=2"], 8192, b"FORM....ICONtrailing");
    let after = set_position(&before, Some((37, 11))).unwrap();
    assert_eq!(position(&after).unwrap(), Some((37, 11)));
    assert_eq!(after.len(), before.len(), "no length change");
    for i in (0..before.len()).filter(|i| !(58..66).contains(i)) {
        assert_eq!(after[i], before[i], "byte {i} changed and should not have");
    }
}

#[test]
fn clearing_a_position_writes_the_sentinel_in_both_coordinates() {
    let before = synthetic_icon_at(13, 4);
    let after = set_position(&before, None).unwrap();
    assert_eq!(position(&after).unwrap(), None);
    assert_eq!(i32::from_be_bytes([after[58], after[59], after[60], after[61]]), NO_POSITION);
    assert_eq!(i32::from_be_bytes([after[62], after[63], after[64], after[65]]), NO_POSITION);
}

#[test]
fn replacing_tooltypes_keeps_the_appended_blob_byte_for_byte() {
    let trailing = b"FORM\x00\x00\x00\x08ICONabcd";
    let before = synthetic_icon(&["OLD=1"], 4096, trailing);
    let after = set_tooltypes(&before, &["NEW=2".to_string(), "MORE=3".to_string()]).unwrap();
    assert_eq!(tooltypes(&after).unwrap(), vec!["NEW=2", "MORE=3"]);
    assert!(after.ends_with(trailing), "the appended ColorIcon must survive");
    assert_eq!(stack_size(&after).unwrap(), 4096, "the stack size is not ours to change");
}

#[test]
fn setting_no_tooltypes_clears_the_flag_and_still_keeps_the_blob() { /* ... */ }

#[test]
fn setting_a_window_on_an_icon_with_no_drawer_data_is_refused_by_name() {
    let err = set_window(&synthetic_icon(&[], 4096, &[]), DrawerWindow { left: 0, top: 0, width: 100, height: 100 }).unwrap_err();
    assert!(format!("{err}").contains("DrawerData"), "the refusal must say why: {err}");
}

#[test]
fn setting_a_window_changes_only_the_eight_bytes_at_78() { /* same shape as the position test */ }

#[test]
fn show_all_files_round_trips_and_leaves_the_rest_alone() { /* ... */ }

#[test]
fn every_write_leaves_a_refused_icon_untouched() {
    // A malformed icon must come back as an Err, never as a best-effort rewrite.
}
```

- [ ] **Step 2: Run to verify they fail** — `cd src-tauri && cargo test amigaicon::`

- [ ] **Step 3: Implement**, reusing `layout()` to find the regions rather than re-deriving offsets.

- [ ] **Step 4: Run to verify they pass**

- [ ] **Step 5: Mutate**

| Mutation | Must fail |
|---|---|
| rebuild the file from parsed fields instead of patching in place | `replacing_tooltypes_keeps_the_appended_blob_byte_for_byte`, `setting_a_position_changes_eight_bytes_and_nothing_else` |
| write only `do_CurrentX` when clearing | `clearing_a_position_writes_the_sentinel_in_both_coordinates` |
| let `set_window` write when `do_DrawerData` is zero | `setting_a_window_on_an_icon_with_no_drawer_data_is_refused_by_name` |
| reset the stack size while replacing tooltypes | `replacing_tooltypes_keeps_the_appended_blob_byte_for_byte` |

- [ ] **Step 6: Commit**

---

### Task 5: The grid arithmetic

**Files:**
- Create: `src-tauri/src/core/icongrid/mod.rs`
- Modify: `src-tauri/src/core/mod.rs` — `pub mod icongrid;` in the alphabetical list

**Interfaces:**
- Consumes: nothing — this module takes sizes and names and returns positions. **No filesystem, no `amigaicon`, no I/O.** That is what makes it testable with three numbers.
- Produces:
  - `pub struct Cell { pub label: String, pub width: u16, pub height: u16, pub is_container: bool }`
  - `pub struct Placement { pub index: usize, pub x: i32, pub y: i32 }`
  - `pub struct GridResult { pub placements: Vec<Placement>, pub window: (i16, i16) }`
  - `pub fn arrange(cells: &[Cell], inner_width: u16) -> GridResult`
  - `pub const ROOT_INNER_WIDTH: u16 = 420;` and `pub const DRAWER_INNER_WIDTH: u16 = 520;`

**The constants are adopted from Emu68 Hatcher, not measured by ART**, and the module doc must say so field by field — the same honesty `core/ilbm`'s `BMHD` doc applies to its header. Topaz character width 8; emboss 3; margins 10 across and 4 down; column gap 8; row pad 18 for the label line; target aspect 2.0 (from iTidy). **`ROOT_INNER_WIDTH` is 420 rather than 520 because the `SYS:` window geometry lives in the volume's `disk.info`, which ART does not write.**

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_cell_is_as_wide_as_its_label_when_the_label_is_wider_than_the_image() {
    // Topaz is 8px per character, so "AReallyLongName" (15 chars) is 120px -
    // wider than a 46px icon. A grid using the image width overlaps labels.
    let cells = vec![
        Cell { label: "AReallyLongName".into(), width: 46, height: 46, is_container: false },
        Cell { label: "X".into(), width: 46, height: 46, is_container: false },
    ];
    let g = arrange(&cells, DRAWER_INNER_WIDTH);
    assert!(g.placements[1].x >= 120, "the second column clears the first label");
}

#[test]
fn containers_come_before_everything_else_and_each_group_is_alphabetical() { /* ... */ }

#[test]
fn nothing_is_placed_outside_the_inner_width() {
    // Every x plus its cell width stays within the budget, for 1..40 cells.
}

#[test]
fn rows_are_bottom_aligned_so_labels_share_a_baseline() {
    // Two cells of different heights in one row must end at the same y + height.
}

#[test]
fn a_single_cell_lands_at_the_margin_and_the_window_contains_it() {
    // "sane" cannot fail, so name the property: one placement, at the margin
    // origin, and a window at least as large as the cell plus its margins.
    let cells = vec![Cell { label: "One".into(), width: 46, height: 46, is_container: false }];
    let g = arrange(&cells, DRAWER_INNER_WIDTH);
    assert_eq!(g.placements.len(), 1);
    assert_eq!((g.placements[0].x, g.placements[0].y), (10, 4));
    assert!(g.window.0 as i32 >= 10 + 46 + 10, "the window contains the cell");
}

#[test]
fn the_column_count_shrinks_until_the_row_fits() {
    // Cells wide enough that the aspect-ratio choice would overflow.
}

#[test]
fn arranging_no_cells_returns_no_placements_and_does_not_panic() { /* ... */ }
```

- [ ] **Step 2: Run to verify they fail** — `cd src-tauri && cargo test icongrid::`

- [ ] **Step 3: Implement.** Choose the column count whose grid shape lands closest to the target aspect, with a floor of two columns (a 1×N tower is never the best answer), then shrink it until the widest-column sum plus gaps fits `inner_width`. Column width is the widest cell in that column; row height is the tallest in that row.

- [ ] **Step 4: Run to verify they pass**

- [ ] **Step 5: Mutate**

| Mutation | Must fail |
|---|---|
| size a cell by its image only, ignoring the label | `a_cell_is_as_wide_as_its_label_when_the_label_is_wider_than_the_image` |
| skip the shrink loop | `nothing_is_placed_outside_the_inner_width`, `the_column_count_shrinks_until_the_row_fits` |
| top-align rows | `rows_are_bottom_aligned_so_labels_share_a_baseline` |
| sort without the container-first key | `containers_come_before_everything_else_and_each_group_is_alphabetical` |

- [ ] **Step 6: Commit**

---

### Task 6: The applier — first real caller of Tasks 2-5

**Files:**
- Modify: `src-tauri/src/core/appearance/mod.rs`

**Interfaces:**
- Consumes: `core::amigaicon::{position, set_position, icon_type, rendered_size, IconType, NO_POSITION}`, `core::icongrid::{arrange, Cell, ROOT_INNER_WIDTH, DRAWER_INNER_WIDTH}`, `core::safety::{guarded_write, BackupPolicy}`
- Produces:
  - `AppearanceRequest` gains `pub arrange_icons: bool`
  - `AppearanceOutcome` gains `pub icons_placed: usize` and `pub drawers_arranged: usize`

**The scoping rule, and it is the owner's decision (spec §2.4): only icons whose position is `NO_POSITION` are placed.** 361 of the owner's 798 carry it; the other 437 were positioned by the release and ART does not touch them. An icon that already has a position keeps it, and it still occupies its cell so the new ones are placed around it.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn an_icon_the_release_positioned_is_left_exactly_where_it_was() {
    // The central guard. A drawer holding one placed icon (13,4) and one
    // unplaced: after arranging, the first is still byte-identical and the
    // second has a position.
}

#[test]
fn an_unplaced_icon_gets_a_position_and_nothing_else_changes() {
    // Compare the whole file outside bytes 58..66.
}

#[test]
fn a_drawer_where_every_icon_is_already_placed_is_not_rewritten_at_all() {
    // No write, no backup, and the outcome counts zero.
}

#[test]
fn the_root_uses_the_narrower_budget() {
    // ROOT_INNER_WIDTH, because the SYS: window size lives in disk.info.
}

#[test]
fn a_malformed_icon_is_skipped_and_named_rather_than_failing_the_whole_run() {
    // One bad .info must not cost the other forty their layout - and the
    // outcome must say which one was skipped. Never claim what you did not do.
}

#[test]
fn a_failed_arrange_leaves_every_icon_byte_for_byte_unchanged() { /* ... */ }

#[test]
fn the_outcome_counts_what_was_actually_placed() { /* ... */ }
```

- [ ] **Step 2: Run to verify they fail** — `cd src-tauri && cargo test appearance::`

- [ ] **Step 3: Implement.** Walk the tree; for each directory, read every `.info` beside its entries; build `Cell`s from `rendered_size` and the file stem; call `arrange`; write back **only** the icons whose position was `NO_POSITION`, through `guarded_write`. Everything that can fail happens before the first write.

- [ ] **Step 4: Run to verify they pass**

- [ ] **Step 5: Mutate**

| Mutation | Must fail |
|---|---|
| place every icon, not only the unplaced | `an_icon_the_release_positioned_is_left_exactly_where_it_was` |
| use `DRAWER_INNER_WIDTH` at the root | `the_root_uses_the_narrower_budget` |
| abort the run on one malformed icon | `a_malformed_icon_is_skipped_and_named_rather_than_failing_the_whole_run` |
| write before arranging | `a_failed_arrange_leaves_every_icon_byte_for_byte_unchanged` |

- [ ] **Step 6: Commit**

---

### Task 7: The oracle proves the writes preserve

**Files:**
- Modify: `scripts/icon-oracle-check.py`

It already round-trips real `.info` files through `core/amigaicon` — **485 of the owner's own icons, 0 failures.** Read it first and extend it in its own shape.

For every icon in the corpus, apply each write and require that **everything the write did not name is byte-identical** and that the named field reads back as asked:

- `set_position(Some((37, 11)))` → bytes 58..66 differ, everything else identical.
- `set_position(None)` → both coordinates are `NO_POSITION`.
- `set_tooltypes(existing)` → the file is byte-identical to what it was (setting what is already there changes nothing).
- `set_show_all_files(true)` then `(false)` → back to the original bytes.
- `rendered_size` → never smaller than the `Gadget` size, and never zero.

- [ ] **Step 1: Extend the script**

- [ ] **Step 2: Prove it can fail.** Break a writer deliberately — have `set_position` also zero the stack size — run the oracle, confirm it reports the icon and the differing byte range. Then restore. **A script that has never failed has proved nothing.**

- [ ] **Step 3: Run it against the real corpus**

```bash
python scripts/icon-oracle-check.py "E:\amiga\Amigatolon\os39"
```

Report the count. "N icons, 0 failures" is the claim.

- [ ] **Step 4: Commit**

---

### Task 8: The flag reaches the screen

**Files:**
- Modify: `src-tauri/src/commands/appearance.rs`, `src/lib/appearance.ts`, `src/components/osbuilder/AppearancePanel.tsx` and its test, `src/i18n/en.json`, `src/i18n/tr.json`

Round 1 built this chain and it is already reachable; you are adding one flag to it, not a second path. `commands/*.rs` stays a **thin adapter** — no decision lives there.

- [ ] **Step 1: Write the failing tests**

```tsx
it("sends arrangeIcons when the box is ticked", async () => {
  // Assert the request object carries arrangeIcons true AND that the other
  // capabilities are absent - otherwise a request that always sends
  // everything would pass.
});

it("says how many icons were placed after a successful run", async () => {
  // The specific number on screen, not "it worked".
});

it("remembers the choice across a remount", async () => { /* isFlag guard */ });

it("renders the new strings in Turkish when the language is tr", async () => { /* ... */ });
```

- [ ] **Step 2: Run to verify they fail** — `pnpm vitest run src/components/osbuilder/AppearancePanel.test.tsx`

- [ ] **Step 3: Implement.** Both i18n files in this commit. The choice goes through `src/lib/remembered.ts` with `isFlag`; it is consumed by the step that asks for it, so it is a remembered key, not a `buildSession` field.

- [ ] **Step 4: Run to verify they pass**, then `pnpm lint` and `pnpm test`

- [ ] **Step 5: Trace the chain and record it** — from the checkbox through `src/lib/appearance.ts` → `commands/appearance.rs` → `core::appearance` → `core::icongrid::arrange`, hop by hop, in the commit message.

- [ ] **Step 6: Commit**

---

### Task 9: Land it

- [ ] **Step 1: The suites, twice** (ART-059). `cargo test` — and quote the **lib target's** summary line: this crate has three test targets and the last two are empty, so `grep "test result" | tail -1` reads the wrong one. Then `pnpm test`, `pnpm lint`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`.
- [ ] **Step 2: The sweeps.** `control-byte-sweep.py`, `scratch-root-sweep.py`, `scratch-counter-sweep.py`, `contrast-check.py --quiet`.
- [ ] **Step 3: The real-material runs.** The icon oracle, and Task 1's root-icon count against the real 3.2 build. Quote both.
- [ ] **Step 4: The documents.** `docs/session-log.md` (a row at the top), `docs/STATUS.md` (numbers, and the "Picking up next session" block **updated in place**), `docs/FEATURES.md` (only where a test exists), `docs/ISSUES.md`, `CHANGELOG.md`. Record the shipped defect Task 1 fixes as an `ART-NNN` in Fixed, with its test name — it was a real defect, not a new feature.
- [ ] **Step 5: Stop.** Do **not** merge or push; that is the owner's call.

---

## Self-review against the spec

| Spec section | Task |
|---|---|
| §2.1 a `Subtree` rule takes the drawer's icon, `from: ""` excluded, `Disk.info` deliberate | 1 |
| §2.2 the reads `amigaicon` skipped | 2 |
| §1.3 the rendered-size rule | 3 |
| §2.3 the writes, preserving everything else | 4 |
| §2.4 the grid arithmetic and its adopted constants | 5 |
| §2.4 only unpositioned icons are placed | 6 |
| §3 the oracle over the real corpus, and mutation | 7, plus every task's Step 5 |
| §4 what is not in this round | nothing implements it, by design |
| §5 known risks | Task 9 does not claim Workbench draws the grid as predicted, nor that the constants are ART's own measurements |
