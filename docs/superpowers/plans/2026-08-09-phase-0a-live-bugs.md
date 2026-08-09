# Phase 0a — the two live bugs: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the application scroll and scale, make disabled controls look disabled, and make ADF Studio able to open a real bootable ADF — then retire the duplicate filesystem writer that bug came from.

**Architecture:** Two independent halves. The first is CSS and one small React component: the shell's flex/grid chain gains the `min-height: 0` that lets `overflow: auto` engage, breakpoints replace hand-written page widths, and a shared `Refusal` component separates "the answer is no" from "something broke". The second is Rust: `core/adf` stops reading a root-block pointer out of 68000 boot code and computes it the way `core/volume` already does, which fixes both bootable and HD images; then the mutating commands move onto `core/volume::write`, and `core/adf/mutate.rs` — a second, floppy-only filesystem writer — is retired with its regression tests moved rather than deleted.

**Tech Stack:** Tauri 2, React 18 + TypeScript, Rust (MSRV 1.77), `amitools` as an independent oracle.

## Global Constraints

- **MSRV is 1.77.** APIs stabilised later fail CI even though they compile locally. No trait-object upcasting (`&dyn Sub` → `&dyn Super`), which is 1.86.
- **`core/` is platform-independent:** `std` + `serde` + `sha2` + `thiserror` + `delharc` only. No `tauri`, no Windows APIs, no network.
- **Release builds use `panic = "abort"`.** Never index a block buffer directly; go through the bounds-checked helpers (`block_slice`, `read_u32_at`, …). A bad index must produce a `CoreError`, not a panic.
- **Every feature gets tests.** Fixtures are synthetic and generated at runtime in a tempdir. ART ships no copyrighted Amiga content.
- **Never destroy the original before successful validation** (§57).
- **Gates, all of which must pass before a task is done:**
  ```
  pnpm lint
  cd src-tauri && cargo fmt --check
  cd src-tauri && cargo clippy --all-targets -- -D warnings
  cd src-tauri && cargo test
  python scripts/oracle-check.py
  ```
- **This repository is not under git** (no `.git`). Where a plan would normally say "commit", the checkpoint is running the gate set above. If the repository is later initialised, those checkpoints become commits unchanged.
- **There is no frontend test runner.** `pnpm lint` is `tsc --noEmit` only. Tasks 1–4 therefore carry exact manual verification procedures instead of automated tests. Tasks 5–10 are Rust and use real TDD.

---

## File Structure

**Created**

| File | Responsibility |
|---|---|
| `src/components/Refusal.tsx` | One component for "ART will not do this, and here is why" — distinct from an error banner. Used wherever a plan can refuse. |

**Modified**

| File | Change |
|---|---|
| `src/components/layout/layout.css` | The scroll chain, breakpoints, a flexible sidebar |
| `src/styles/global.css` | `:disabled` and `:focus-visible` for `.btn` and form controls |
| `src/pages/*.tsx` (11 files) | Hand-written `maxWidth` removed in favour of a shell-level content width |
| `src/pages/WhdloadInstall.tsx` | Uses `Refusal` for a refused plan rather than the error banner |
| `src-tauri/src/core/adf/bootblock.rs` | `root_block` field removed — a boot block has no such field |
| `src-tauri/src/core/adf/mod.rs` | `AdfImage` computes its geometry from the image; `info()` stops assuming DD |
| `src-tauri/src/commands/adf.rs` | Mutations routed through `core/volume::write` |
| `src-tauri/src/core/sources/install.rs` | ADF install routed through the volume path |
| `scripts/oracle-check.py` | A bootable ADF written by amitools must open in ART |

**Deleted**

| File | Why |
|---|---|
| `src-tauri/src/core/adf/mutate.rs` | Superseded by `core/volume/write/`. Its ART-009…013 regression tests move to `core/volume/write/mod.rs` first. |

`core/adf/create.rs` **stays** — formatting a blank image has no equivalent in `core/volume` yet.

---

## Task 1: The shell scrolls

**Files:**
- Modify: `src/components/layout/layout.css:14-24`

**Interfaces:**
- Consumes: nothing
- Produces: a `.app-content` that actually scrolls; every later UI task depends on this being true

**Why this is the fix:** `.app-shell` is `height: 100vh; overflow: hidden`. `.app-main` is a grid item whose default `min-height: auto` makes it grow to fit its content instead of being held to the row height. `.app-content`'s `overflow: auto` therefore never engages — there is nothing to scroll — and the excess is clipped by the shell. `min-height: 0` appears nowhere in the codebase.

- [ ] **Step 1: Reproduce the bug**

Run `pnpm tauri dev`, open **Aminet**, and sync the catalogue so the results list is long. Resize the window short.

Expected right now: the list is cut off at the bottom edge, and there is **no scrollbar**.

- [ ] **Step 2: Add the missing link in the chain**

In `src/components/layout/layout.css`, replace the `.app-main` and `.app-content` rules:

```css
.app-main {
  display: flex;
  flex-direction: column;
  min-width: 0;
  /* A grid item defaults to `min-height: auto`, which means "as tall as my
     content". That makes the flex child below unable to shrink, so its
     `overflow: auto` never engages and the shell clips the excess instead of
     scrolling it. */
  min-height: 0;
}
.app-content {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 24px 28px;
}
```

- [ ] **Step 3: Verify the fix**

Run `pnpm tauri dev`, repeat Step 1.

Expected: a scrollbar appears on the right of the content area; the sidebar and topbar stay put; nothing is cut off. Repeat on **Files** and **ADF Studio**.

- [ ] **Step 4: Checkpoint**

```bash
pnpm lint
```

---

## Task 2: The shell scales

**Files:**
- Modify: `src/components/layout/layout.css` (sidebar, quick actions)
- Modify: `src/pages/AdfBrowser.tsx:295`, `AminetStudio.tsx:562`, `CollectionStudio.tsx:125`, `GotekStudio.tsx:114`, `HardDiskStudio.tsx:180`, `HexTools.tsx`, `PistormStudio.tsx`, `RomStudio.tsx`, `WinuaeStudio.tsx`, `WhdloadInstall.tsx`, `LhaBrowser.tsx`

**Interfaces:**
- Consumes: Task 1's scroll chain
- Produces: `.app-content > *` is width-constrained centrally; pages stop setting their own `maxWidth`

**Why:** there are zero `@media` queries in the application. The sidebar is a fixed 224px, quick actions are `repeat(4, 1fr)`, and each page carries its own `maxWidth` (980 / 1040 / 1100). Narrow windows crush the two-pane manager; wide windows waste both margins.

- [ ] **Step 1: Reproduce**

Run `pnpm tauri dev`. Drag the window narrow (~900px). Open **Files**.

Expected right now: the two panes crush together and the sidebar still takes 224px.

- [ ] **Step 2: Give the content one width, centrally**

Add to `src/components/layout/layout.css`, after the `.app-content` rule:

```css
/* One place decides how wide content gets, so a page never has to. Pages that
   genuinely want the full width (the two-pane manager) opt out with
   `.app-content-wide` on their own root. */
.app-content > * {
  max-width: 1180px;
  margin-inline: auto;
}
.app-content > .app-content-wide {
  max-width: none;
}
```

- [ ] **Step 3: Add breakpoints**

Append to `src/components/layout/layout.css`:

```css
/* Below this the sidebar's labels cost more than they give. */
@media (max-width: 1000px) {
  .app-shell {
    grid-template-columns: 60px 1fr;
  }
  .sidebar-brand,
  .sidebar-link span:not(.sidebar-icon) {
    display: none;
  }
  .sidebar-link {
    justify-content: center;
  }
  .app-content {
    padding: 16px 14px;
  }
}

@media (max-width: 760px) {
  .quick-actions {
    grid-template-columns: repeat(2, 1fr);
  }
}
```

- [ ] **Step 4: Remove the per-page widths**

In each of the eleven pages listed above, delete the `maxWidth` from the page's root `<div style={{ … }}>`. For example in `src/pages/AminetStudio.tsx`:

```tsx
// before
<div style={{ maxWidth: 1100 }}>
// after
<div>
```

Leave `maxWidth: "90vw"` on modal cards — those are dialogs, not page content.

- [ ] **Step 5: Let the file manager use the full width**

In `src/pages/FileManager.tsx`, give the page root the opt-out class:

```tsx
<div className="app-content-wide">
```

- [ ] **Step 6: Verify**

Run `pnpm tauri dev`. At ~900px wide: the sidebar collapses to icons, the panes stay usable. At ~1900px: content is centred rather than left-hugging, and **Files** uses the whole width.

- [ ] **Step 7: Checkpoint**

```bash
pnpm lint
```

---

## Task 3: Disabled and focused controls look it

**Files:**
- Modify: `src/styles/global.css:48-77`

**Interfaces:**
- Consumes: nothing
- Produces: `:disabled` and `:focus-visible` styling that every button in the app inherits

**Why:** the string `disabled` appears in no stylesheet. A disabled primary button stays solid accent-coloured and looks entirely clickable — as seen on the WHDLoad screen, where a correctly-disabled "Install to the disk" button read as ready.

- [ ] **Step 1: Reproduce**

Run `pnpm tauri dev`, open **Install WHDLoad**, choose a `.lha` with no `.slave` in it (any ordinary archive).

Expected right now: the plan is refused, and the blue "Install to the disk" button looks active. Click it — nothing happens, with no explanation.

- [ ] **Step 2: Style the states**

In `src/styles/global.css`, after the `.btn-primary:hover` rule:

```css
/* A control that cannot be used has to look like it. Without this a disabled
   primary button keeps its accent fill and reads as ready. */
.btn:disabled,
.btn[aria-disabled="true"] {
  opacity: 0.45;
  cursor: not-allowed;
  filter: grayscale(0.6);
}
.btn:disabled:hover,
.btn[aria-disabled="true"]:hover {
  background: var(--bg-elevated);
}
.btn-primary:disabled:hover {
  background: var(--accent);
}

/* Keyboard users need to see where they are. `:focus-visible` rather than
   `:focus` so a mouse click does not leave a ring behind. */
.btn:focus-visible,
input:focus-visible,
select:focus-visible,
textarea:focus-visible,
a:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

input:disabled,
select:disabled,
textarea:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
```

- [ ] **Step 3: Verify**

Repeat Step 1. The button is now visibly greyed and shows a "not allowed" cursor. Tab through the WHDLoad screen — every focusable control shows a ring.

- [ ] **Step 4: Checkpoint**

```bash
pnpm lint
```

---

## Task 4: A refusal is not an error

**Files:**
- Create: `src/components/Refusal.tsx`
- Modify: `src/pages/WhdloadInstall.tsx` (the error banner and `WhatHappens`)

**Interfaces:**
- Consumes: Task 3's disabled styling
- Produces: `<Refusal title={string} reason={string} suggestion?={ReactNode} />` — used by any screen whose plan can say no

**Why:** "this is not a WHDLoad package" currently shares its red banner and its `ART-*` identifier with a genuine runtime failure. The identifier is right for a failure (§68) and wrong for an answer. A refusal is the normal outcome of asking, and it belongs where the question was asked.

- [ ] **Step 1: Write the component**

Create `src/components/Refusal.tsx`:

```tsx
// "ART will not do this, and here is why."
//
// Deliberately not the error banner. An error means something broke and
// carries an ART-* identifier so it can be looked up (§68). A refusal is the
// normal answer to a question the user asked — it broke nothing, it needs no
// identifier, and presenting it in red with a code teaches the user to read
// ART's real failures as noise.

export function Refusal({
  title,
  reason,
  suggestion,
}: {
  /** What ART will not do, in the user's words. */
  title: string;
  /** Why not. Complete sentences; this is the whole explanation. */
  reason: string;
  /** What they can do instead, when there is something. */
  suggestion?: React.ReactNode;
}) {
  return (
    <div
      role="status"
      className="card"
      style={{
        borderColor: "var(--warn)",
        background: "color-mix(in srgb, var(--warn) 8%, var(--bg-panel))",
        marginTop: 8,
      }}
    >
      <strong style={{ fontSize: 13 }}>{title}</strong>
      <div style={{ fontSize: 13, marginTop: 4 }}>{reason}</div>
      {suggestion && (
        <div className="faint" style={{ fontSize: 12, marginTop: 6 }}>
          {suggestion}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Use it for a refused plan**

In `src/pages/WhdloadInstall.tsx`, import it:

```tsx
import { Refusal } from "@/components/Refusal";
```

and in `WhatHappens`, replace the refusal branch:

```tsx
      {plan.refusal ? (
        <Refusal
          title="ART will not install this package"
          reason={plan.refusal}
          suggestion="You can still copy it by hand from the Files screen."
        />
      ) : (
```

- [ ] **Step 3: Stop the plan failure reaching the error banner**

Still in `src/pages/WhdloadInstall.tsx`, `refreshPlan` currently puts a thrown plan error into `setError`. A plan that throws because the archive is not a WHDLoad package is a refusal, not a failure. Replace the `catch` in `refreshPlan`:

```tsx
    } catch (e) {
      // A plan that cannot even be built is still an answer about the archive,
      // not a fault in ART. It is shown where the package was chosen.
      setPlan(null);
      setPlanRefusal(String(e).replace(/^invalid input: /, ""));
    } finally {
```

and add the state beside the others:

```tsx
  const [planRefusal, setPlanRefusal] = useState<string | null>(null);
```

clearing it at the start of `refreshPlan` alongside `setError(null)`:

```tsx
    setError(null);
    setPlanRefusal(null);
```

Then render it in the package section, after `{plan && <Detection … />}`:

```tsx
        {planRefusal && (
          <Refusal
            title="This is not a package ART can install"
            reason={planRefusal}
            suggestion="Open it in Archive Tools to see what is inside."
          />
        )}
```

- [ ] **Step 4: Verify**

Run `pnpm tauri dev`, open **Install WHDLoad**, choose an ordinary `.lha`.

Expected: an amber panel under the package saying it is not an installable package, with no `ART-*` code and no red banner. The install button is visibly disabled. Choosing a real WHDLoad archive clears it.

- [ ] **Step 5: Checkpoint**

```bash
pnpm lint
pnpm build
```

---

## Task 5: The root block is computed, not read from boot code

**Files:**
- Modify: `src-tauri/src/core/adf/bootblock.rs:57` (field), `:97-109` (derivation), `:178-215` (tests)
- Modify: `src-tauri/src/core/adf/mod.rs:66-88` (`AdfImage::from_bytes`)

**Interfaces:**
- Consumes: `crate::core::volume::VolumeGeometry::root_block_for(total_blocks: u32) -> u32`
- Produces: `BootBlock` no longer has a `root_block` field; `AdfImage::from_bytes` derives `root_block_num` from the image length

**Why:** `BootBlock::parse` reads bytes 8..11 as a "root block pointer". An AmigaDOS boot block has no such field — 0..3 is the DOS type, 4..7 the checksum, and **8 onwards is boot code**. On any bootable disk ART reads 68000 machine code as a block number, which is the reported `malformed adf: root block has type 1963519789`. The two existing tests pass only because their fixtures are all zeros.

The correct value is computed: `total_blocks / 2`. `VolumeGeometry::root_block_for` already does this and was pinned during Stage R against ADFlib's `adfVolCalcRootBlk` and amitools' `BootBlock.calc_root_blk`.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src-tauri/src/core/adf/mod.rs`:

```rust
    /// A bootable disk carries 68000 boot code from byte 8 onwards. ART used
    /// to read those bytes as a root-block pointer, which is why every
    /// bootable ADF failed to open with a nonsense block type.
    #[test]
    fn a_bootable_image_opens_because_the_root_block_is_computed() {
        use crate::core::adf::create::create_blank_adf;

        let mut image = create_blank_adf("Boot", FileSystemType::Ffs, false).unwrap();
        // Real boot code, not zeros: `bra.s` plus arbitrary following bytes.
        image[8..12].copy_from_slice(&[0x60, 0x0E, 0x75, 0x0B]);

        let opened = AdfImage::from_bytes(image).expect("a bootable ADF must open");
        assert_eq!(
            opened.info().unwrap().root_block,
            880,
            "a DD image's root block is 1760/2, whatever the boot code says"
        );
    }

    /// The same omission is why HD images never worked: the old path assumed
    /// the DD block count as well as the DD root.
    #[test]
    fn an_hd_image_finds_its_root_at_1760() {
        let mut image = vec![0u8; 3520 * blocks::BLOCK_SIZE];
        image[0..4].copy_from_slice(b"DOS\x01");

        let root = 1760 * blocks::BLOCK_SIZE;
        // A minimal root block: type T_HEADER, subtype ST_ROOT, hash size.
        image[root..root + 4].copy_from_slice(&2i32.to_be_bytes());
        image[root + 12..root + 16].copy_from_slice(&72u32.to_be_bytes());
        image[root + 508..root + 512].copy_from_slice(&1i32.to_be_bytes());

        let opened = AdfImage::from_bytes(image).expect("an HD ADF must open");
        assert_eq!(opened.info().unwrap().root_block, 1760);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd src-tauri && cargo test --quiet a_bootable_image_opens_because_the_root_block_is_computed an_hd_image_finds_its_root_at_1760
```

Expected: both FAIL — the first with `root block has type` and a large number, the second with a malformed or out-of-range error.

- [ ] **Step 3: Remove the field that does not exist on disk**

In `src-tauri/src/core/adf/bootblock.rs`, delete the `root_block` field from the struct, delete the derivation at lines 97–109, and delete it from the constructor at line 121. Update the doc comment at line 6:

```rust
//! - offset 0..4:   DOS type
//! - offset 4..8:   checksum
//! - offset 8..:    boot code
//!
//! There is **no root-block pointer in a boot block.** ART used to read bytes
//! 8..11 as one, which meant reading 68000 boot code as a block number and
//! failing on every bootable disk. The root block is computed from the
//! volume's size — see `VolumeGeometry::root_block_for`.
```

Delete `DEFAULT_ROOT_BLOCK` if nothing else uses it:

```bash
cd src-tauri && grep -rn "DEFAULT_ROOT_BLOCK" src/
```

Keep it only where a caller genuinely wants the DD constant; otherwise remove it. Delete the now-meaningless test `root_block_defaults_when_zero`, and the `assert_eq!(bb.root_block, DEFAULT_ROOT_BLOCK)` line from `parses_ofs_bootblock`.

- [ ] **Step 4: Compute it in `AdfImage`**

In `src-tauri/src/core/adf/mod.rs`, in `from_bytes`, replace the line that took the root from the boot block:

```rust
        // The root block is derived from the volume's size, not read from the
        // boot block — a boot block has no such field. Verified against
        // ADFlib (`adfVolCalcRootBlk`) and amitools (`calc_root_blk`).
        let total_blocks = (image.len() / blocks::BLOCK_SIZE) as u32;
        let root_block_num = crate::core::volume::VolumeGeometry::root_block_for(total_blocks);
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cd src-tauri && cargo test --quiet adf::
```

Expected: PASS, including the two new tests.

- [ ] **Step 6: Checkpoint**

```bash
cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test
```

---

## Task 6: `AdfImage` reports the geometry it actually has

**Files:**
- Modify: `src-tauri/src/core/adf/mod.rs:90-110` (`info`)

**Interfaces:**
- Consumes: Task 5's `root_block_num`
- Produces: `AdfInfo` whose `capacity_bytes`, `free_bytes` and `used_bytes` are right for HD as well as DD

**Why:** `info()` computes capacity from `DD_TOTAL_BLOCKS` and parses the bitmap with the same constant. On an HD image every number it reports is half.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src-tauri/src/core/adf/mod.rs`:

```rust
    #[test]
    fn an_hd_image_reports_its_real_capacity() {
        let mut image = vec![0u8; 3520 * blocks::BLOCK_SIZE];
        image[0..4].copy_from_slice(b"DOS\x01");

        let root = 1760 * blocks::BLOCK_SIZE;
        image[root..root + 4].copy_from_slice(&2i32.to_be_bytes());
        image[root + 12..root + 16].copy_from_slice(&72u32.to_be_bytes());
        // bm_pages[0] → block 1761, and bm_flag valid.
        image[root + 312..root + 316].copy_from_slice(&(-1i32).to_be_bytes());
        image[root + 316..root + 320].copy_from_slice(&1761u32.to_be_bytes());
        image[root + 508..root + 512].copy_from_slice(&1i32.to_be_bytes());

        let info = AdfImage::from_bytes(image).unwrap().info().unwrap();
        assert_eq!(
            info.capacity_bytes,
            3520 * blocks::BLOCK_SIZE as u64,
            "an HD image is 1.76 MB, not 880 KB"
        );
    }
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cd src-tauri && cargo test --quiet an_hd_image_reports_its_real_capacity
```

Expected: FAIL — `capacity_bytes` is the DD value.

- [ ] **Step 3: Use the image's own size**

In `src-tauri/src/core/adf/mod.rs`, in `info()`, replace the two uses of `DD_TOTAL_BLOCKS`:

```rust
        // The image's own size, not a floppy-shaped assumption. An HD ADF has
        // twice the blocks and its bitmap describes twice as many.
        let total_blocks = self.image.len() / blocks::BLOCK_SIZE;
        let bm = blocks::Bitmap::parse(bm_block, total_blocks)?;
        let capacity_bytes = (total_blocks * blocks::BLOCK_SIZE) as u64;
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd src-tauri && cargo test --quiet adf::
```

Expected: PASS.

- [ ] **Step 5: Checkpoint**

```bash
cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test
```

---

## Task 7: The oracle proves it on a disk ART did not write

**Files:**
- Modify: `scripts/oracle-check.py` (new check + registration in `main`)
- Modify: `src-tauri/src/core/adf/mod.rs` (a `#[test]` export hook)

**Interfaces:**
- Consumes: Tasks 5 and 6
- Produces: a blocking CI check that a bootable ADF made by amitools opens in ART

**Why:** Tasks 5 and 6 are proved by ART's own fixtures. The bug they fix was invisible to ART's tests for exactly that reason — every fixture ART builds has zeros where a real disk has boot code. Only a disk ART did not write settles it.

- [ ] **Step 1: Add the read-back hook**

Add to the `tests` module in `src-tauri/src/core/adf/mod.rs`:

```rust
    /// Open an image some other tool wrote and print what ART made of it.
    ///
    /// `scripts/oracle-check.py` has `xdftool` build a *bootable* floppy — the
    /// case ART used to fail on, because a bootable disk has 68000 code where
    /// ART looked for a block number.
    #[test]
    fn open_foreign_adf_for_oracle_when_asked() {
        let Ok(source) = std::env::var("ART_ADF_READ_IN") else {
            return;
        };
        let image = AdfImage::open(std::path::Path::new(&source)).unwrap();
        let info = image.info().unwrap();
        println!("volume={}", info.volume_name);
        println!("root={}", info.root_block);
        println!("capacity={}", info.capacity_bytes);
    }
```

- [ ] **Step 2: Add the oracle check**

In `scripts/oracle-check.py`, add before `def main()`:

```python
def check_bootable_adf_opens(work: Path) -> None:
    """A *bootable* floppy amitools wrote, opened by ART.

    ART used to read bytes 8..11 of the boot block as a root-block pointer.
    On a bootable disk those bytes are 68000 boot code, so every real bootable
    ADF failed to open. ART's own fixtures never caught it: they are zeros
    there. Only a disk ART did not write can prove the fix.
    """
    print("Bootable ADF written by amitools, opened by ART:")
    image = work / "bootable.adf"

    oracle(["xdftool", str(image), "create", "+", "format", "Boot", "ffs", "+", "boot", "install"])
    if not image.exists():
        print("  FAIL amitools could not create a bootable image")
        failures.append("amitools could not create a bootable image")
        return

    # Prove the fixture is actually bootable — otherwise the test proves nothing.
    head = image.read_bytes()[8:12]
    if head == b"\0\0\0\0":
        print("  FAIL the fixture has no boot code, so it does not exercise the bug")
        failures.append("the bootable fixture was not bootable")
        return
    print("  ok   the fixture really carries boot code")

    out = run(
        [
            "cargo",
            "test",
            "--quiet",
            "open_foreign_adf_for_oracle_when_asked",
            "--",
            "--nocapture",
        ],
        {"ART_ADF_READ_IN": str(image)},
    )
    expect("ART opens a bootable ADF", out, "volume=Boot")
    expect("and puts its root block at 880", out, "root=880")
    expect("and reports a DD capacity", out, "capacity=901120")
```

and register it in `main()` beside the others:

```python
        check_bootable_adf_opens(work)
```

- [ ] **Step 3: Run it**

```bash
python scripts/oracle-check.py
```

Expected: the new block prints five `ok` lines and the script ends with "ART and an independent implementation agree, both ways round."

- [ ] **Step 4: Checkpoint**

```bash
cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test
python scripts/oracle-check.py
```

---

## Task 8: ADF mutations run on the volume writer

**Files:**
- Modify: `src-tauri/src/commands/adf.rs:8-10` (imports), `:100`, `:133`, `:164`, `:196` (the four mutating commands)

**Interfaces:**
- Consumes: `crate::commands::volume_write::with_volume(image: &Path, volume_index: usize, run: F) -> CoreResult<(T, WriteStrategy, Option<String>)>`, and on the writer: `add_file(parent: u32, name: &str, data: &[u8], meta: FileMeta) -> CoreResult<WriteOutcome>`, `make_dir(parent: u32, name: &str)`, `delete(parent: u32, entry_block: u32)`, `rename(parent: u32, entry_block: u32, new_name: &str)`
- Produces: `commands/adf.rs` no longer references `core::adf::mutate`; `MutationOutcome` is built from a `WriteOutcome`

**Why:** an ADF is a bare volume at index 0 — `volume_scan` already reports it that way, and `/files` already writes to ADFs through this path. Two writers for one filesystem is how the two screens came to disagree about the same disk.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src-tauri/src/commands/adf.rs` (create the module if there is none):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::adf::create::create_blank_adf;
    use crate::core::adf::FileSystemType;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("art-cmd-adf-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The same operation the file manager performs, through the ADF commands.
    /// Both must land on `core/volume` so the two screens cannot disagree.
    #[test]
    fn adding_a_file_goes_through_the_volume_writer() {
        let dir = scratch("add");
        let path = dir.join("disk.adf");
        std::fs::write(
            &path,
            create_blank_adf("Work", FileSystemType::Ffs, false).unwrap(),
        )
        .unwrap();

        let source = dir.join("Readme");
        std::fs::write(&source, b"hello from ART").unwrap();

        let outcome = add_file_at(&path, None, &source, Some("Readme".into())).unwrap();
        assert!(outcome.verified, "the volume writer verifies what it wrote");

        // Read it back through the volume path, which is now the only path.
        let entry = crate::commands::volume_write::pick_volume(&path, 0).unwrap();
        let (device, geometry) = crate::core::volume::mount::mount(&path, &entry).unwrap();
        let set = crate::core::volume::write::layout::BlockSet::new(geometry.block_size);
        let found = crate::core::volume::write::dir::find_entry(
            &device,
            &set,
            &geometry,
            geometry.root_block,
            "Readme",
        )
        .unwrap()
        .expect("the file must be on the disk");
        assert_eq!(
            crate::core::volume::write::file::read_file(&device, &set, &geometry, found.block)
                .unwrap(),
            b"hello from ART"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cd src-tauri && cargo test --quiet adding_a_file_goes_through_the_volume_writer
```

Expected: FAIL to compile — `add_file_at` does not exist.

- [ ] **Step 3: Add the shared helper and route the four commands**

In `src-tauri/src/commands/adf.rs`, replace the `core::adf` mutation imports with the volume ones and add the helper:

```rust
use crate::commands::volume_write::{with_volume, MutationResult};
use crate::core::volume::write::FileMeta;

/// One place where an ADF command becomes a volume write.
///
/// An ADF is a bare volume at index 0. Routing every mutation through the
/// Stage W writer is what stops ADF Studio and the file manager holding two
/// different ideas of the same disk.
fn add_file_at(
    image: &std::path::Path,
    dir_block: Option<u32>,
    source: &std::path::Path,
    name: Option<String>,
) -> CoreResult<MutationResult> {
    let data = std::fs::read(source)?;
    let chosen = name.unwrap_or_else(|| {
        source
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    });

    crate::commands::volume_write::write_bytes_into(
        image,
        0,
        dir_block.unwrap_or(0),
        &chosen,
        &data,
        false,
    )
}
```

Then change each of the four `mutate_disk_file` call sites to its volume equivalent. For `adf_add_file` at line 100:

```rust
    let result = add_file_at(
        &PathBuf::from(&path),
        dir_block,
        &PathBuf::from(&source_path),
        file_name,
    )
    .map_err(AppError::from);
```

For `adf_create_directory` at line 133:

```rust
    let result = with_volume(&PathBuf::from(&path), 0, |writer| {
        writer.make_dir(dir_block.unwrap_or(0), name.trim())
    })
    .map(|(outcome, _, backup)| MutationOutcome::from_write(outcome, backup))
    .map_err(AppError::from);
```

For `adf_delete_entry` at line 164:

```rust
    let result = with_volume(&PathBuf::from(&path), 0, |writer| {
        writer.delete(dir_block.unwrap_or(0), header_block)
    })
    .map(|(outcome, _, backup)| MutationOutcome::from_write(outcome, backup))
    .map_err(AppError::from);
```

For `adf_rename_entry` at line 196:

```rust
    let result = with_volume(&PathBuf::from(&path), 0, |writer| {
        writer.rename(dir_block.unwrap_or(0), header_block, new_name.trim())
    })
    .map(|(outcome, _, backup)| MutationOutcome::from_write(outcome, backup))
    .map_err(AppError::from);
```

- [ ] **Step 4: Add the two bridges these need**

In `src-tauri/src/commands/volume_write.rs`, make `write_bytes_into` a reusable function — `volume_write_bytes` becomes a thin command over it:

```rust
/// Write bytes into a volume, replacing an existing entry when asked.
///
/// Shared by the `volume_write_bytes` command, the checkout checkin path and
/// the ADF commands, so all three take the same route.
pub fn write_bytes_into(
    image: &Path,
    volume_index: usize,
    dir_block: u32,
    name: &str,
    contents: &[u8],
    replace: bool,
) -> CoreResult<MutationResult> {
    let (outcome, strategy, backup) = with_writer(image, volume_index, |writer| {
        if let Some(existing) = writer.find(dir_block, name)? {
            if !replace {
                return Err(CoreError::InvalidInput(format!(
                    "'{name}' is already there"
                )));
            }
            writer.delete(dir_block, existing.block)?;
        }
        writer.add_file(dir_block, name, contents, Default::default())
    })?;

    Ok(result_of(
        outcome,
        strategy,
        backup,
        outcome_block_size(image, volume_index),
    ))
}
```

In `src-tauri/src/core/adf/mod.rs`, give `MutationOutcome` a constructor from a `WriteOutcome` so the command's return type does not change:

```rust
impl MutationOutcome {
    /// Build the ADF commands' outcome from the volume writer's.
    ///
    /// The shapes differ because they were written two stages apart; this is
    /// the one place that reconciles them, so the frontend contract is
    /// unchanged by the migration.
    pub fn from_write(
        outcome: crate::core::volume::write::WriteOutcome,
        backup: Option<String>,
    ) -> Self {
        Self {
            backup_path: backup,
            verified: outcome.verified,
            blocks_touched: outcome.blocks_touched,
        }
    }
}
```

Adjust the field names to whatever `MutationOutcome` already declares — run `grep -n "pub struct MutationOutcome" -A10 src-tauri/src/core/adf/mod.rs` first and match them exactly.

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cd src-tauri && cargo test --quiet adf
```

Expected: PASS, including the new test and every existing ADF command test.

- [ ] **Step 6: Verify in the application**

Run `pnpm tauri dev`, open **Disk Tools**, open a real ADF, add a file, make a folder, rename it, delete it. Then open the same image in **Files** and confirm it shows the same contents.

- [ ] **Step 7: Checkpoint**

```bash
cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test
python scripts/oracle-check.py
pnpm lint
```

---

## Task 9: The ADF install runs on the volume writer too

**Files:**
- Modify: `src-tauri/src/core/sources/install.rs:31-32` (imports), `install_archive_into_adf`
- Modify: `src-tauri/src/commands/sources.rs` (`sources_install_adf`)

**Interfaces:**
- Consumes: `crate::commands::volume_write::run_copy_in_folder(image, volume_index, parent, folder, policy, sink) -> CoreResult<(CopyReport, Option<String>)>` and `crate::core::sources::install::unpack_for_install`
- Produces: `core/sources/install.rs` no longer imports `core::adf::mutate`; `sources_install_adf` becomes a thin wrapper over the same path `sources_install_volume` uses

**Why:** `install_archive_into_adf` is the last caller of `core/adf/mutate`. `sources_install_volume` already does the same job through the volume writer — one install path with two destinations, as §41.5.3 asks.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src-tauri/src/commands/sources.rs`:

```rust
    /// Installing into an ADF and installing into a partition must produce the
    /// same disk contents, because they are the same operation.
    #[test]
    fn installing_into_an_adf_uses_the_volume_writer() {
        use crate::core::jobs::NoProgress;
        use crate::core::lha::tests::make_lha_with;
        use crate::core::volume::fixture::ffs_volume;
        use crate::core::volume::DosType;

        let dir = temp_root("install-adf");
        let archive = dir.join("Pack.lha");
        std::fs::write(
            &archive,
            make_lha_with(&[("Tools/Editor", b"editor bytes"), ("Readme", b"read me")]),
        )
        .unwrap();

        let image = dir.join("disk.adf");
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();

        let (scratch, _) =
            crate::core::sources::install::unpack_for_install(&archive, &NoProgress).unwrap();
        let folder = crate::core::volume::write::copy::HostFolder::new(scratch.path(), true);
        let (report, backup) = crate::commands::volume_write::run_copy_in_folder(
            &image,
            0,
            0,
            &folder,
            crate::core::lha::OverwritePolicy::Skip,
            &NoProgress,
        )
        .unwrap();

        assert_eq!(report.files_copied, 2);
        assert_eq!(report.files_verified, 2, "the volume writer verifies");
        assert!(backup.is_some(), "a floppy is backed up before replacement");

        let _ = std::fs::remove_dir_all(&dir);
    }
```

- [ ] **Step 2: Run it to verify it fails or passes**

```bash
cd src-tauri && cargo test --quiet installing_into_an_adf_uses_the_volume_writer
```

Expected: PASS — the volume path already works. This test is the safety net for Step 3, which changes the command to use it. If it fails, stop: the migration in Task 8 is incomplete.

- [ ] **Step 3: Route the command**

In `src-tauri/src/commands/sources.rs`, change `sources_install_adf`'s job body to call the same helper `sources_install_volume` does, with `volume_index: 0`:

```rust
        let outcome = (|| -> CoreResult<InstallOutcome> {
            let (scratch, skipped) =
                crate::core::sources::install::unpack_for_install(&archive_path, progress)?;

            let folder = crate::core::volume::write::copy::HostFolder::new(scratch.path(), true);
            // An ADF is a bare volume at index 0 — the same install, a
            // different destination (§41.5.3).
            let (report, backup) = crate::commands::volume_write::run_copy_in_folder(
                &adf_path,
                0,
                parent,
                &folder,
                policy,
                progress,
            )?;

            let mut left_behind = skipped;
            left_behind.extend(report.skipped.iter().cloned());

            Ok(InstallOutcome {
                files: report.files_copied,
                directories: report.directories_created,
                bytes: report.bytes_copied,
                into: String::new(),
                backup,
                skipped: left_behind,
            })
        })();
```

Add the `parent` and `policy` bindings above the job, mirroring `sources_install_volume`.

- [ ] **Step 4: Delete the superseded function**

Remove `install_archive_into_adf` and `plan_install` from `src-tauri/src/core/sources/install.rs`, along with the now-unused imports on lines 31–32 and the helpers only they used (`collect_files`, `plan_from`, `ensure_directory_path`, `split_path`, `InstallPlan`). Run `cargo clippy` after each removal — it names what has become dead.

Keep `unpack_for_install` and `Scratch`.

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cd src-tauri && cargo test --quiet sources
```

Expected: PASS. Any test that exercised `install_archive_into_adf` directly should now exercise the volume path instead — rewrite it rather than deleting it.

- [ ] **Step 6: Verify in the application**

Run `pnpm tauri dev`, open **Aminet**, download a small package, and use **Install to ADF…** against a real floppy image. Then open that image in **Files** and confirm the contents.

- [ ] **Step 7: Checkpoint**

```bash
cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test
python scripts/oracle-check.py
```

---

## Task 10: Retire the second filesystem writer

**Files:**
- Modify: `src-tauri/src/core/volume/write/mod.rs` (receives the moved regression tests)
- Modify: `src-tauri/src/core/adf/mod.rs:21` (re-exports), `:184` (`mutate_disk_file`)
- Delete: `src-tauri/src/core/adf/mutate.rs`

**Interfaces:**
- Consumes: Tasks 8 and 9 — nothing outside `core/adf` calls `mutate` any more
- Produces: one filesystem writer in the codebase

**Why:** `core/adf/mutate.rs` is 858 lines of a second AmigaDOS writer that only understands floppies. It is the code that hardcodes block 881 as *the* bitmap block and 1760 as the size. Its regression tests, though, are the evidence for behaviour the surviving writer must keep — ART-009 … ART-013 were real bugs and their tests must not disappear with the file.

- [ ] **Step 1: Find the tests worth keeping**

```bash
cd src-tauri && grep -n "#\[test\]" -A2 src/core/adf/mutate.rs
```

For each test, decide: does `core/volume/write/` already cover this behaviour? Run the equivalent to check:

```bash
cd src-tauri && cargo test --quiet volume::write
```

The ART-009 … ART-013 cases — hash-chain unlink from the middle, case-insensitive collision, INTL folding from the volume's own dostype — are covered by `dir.rs`'s tests. Any test in `mutate.rs` with **no** counterpart in `core/volume/write/` moves in Step 2.

- [ ] **Step 2: Move the uncovered tests**

For each uncovered test, add an equivalent to the `tests` module in `src-tauri/src/core/volume/write/mod.rs`, rewritten against `VolumeWriter`. For example, a `mutate.rs` test of the form:

```rust
    #[test]
    fn deleting_frees_the_extension_blocks_too() {
        let mut img = create_blank_adf("T", FileSystemType::Ffs, false).unwrap();
        let block = add_file(&mut img, 880, "Big", &vec![0u8; 100 * 512], FileSystemType::Ffs).unwrap();
        let before = free_block_count(&img).unwrap();
        delete_entry(&mut img, 880, block).unwrap();
        assert!(free_block_count(&img).unwrap() > before);
    }
```

becomes:

```rust
    /// Moved from `core/adf/mutate.rs` when that second writer was retired.
    /// A file long enough to need an extension chain must give every one of
    /// those blocks back, not just its data.
    #[test]
    fn deleting_frees_the_extension_blocks_too() {
        let disk = floppy("delete-extensions");
        let data = vec![9u8; 100 * 512];

        let before = with_writer(&disk, |w| w.free_blocks()).unwrap();
        let added =
            with_writer(&disk, |w| w.add_file(0, "Big.bin", &data, FileMeta::default())).unwrap();
        with_writer(&disk, |w| w.delete(0, added.block.unwrap())).unwrap();

        assert_eq!(
            with_writer(&disk, |w| w.free_blocks()).unwrap(),
            before,
            "every block a deleted file held must come back, extensions included"
        );
    }
```

- [ ] **Step 3: Run the moved tests to verify they pass**

```bash
cd src-tauri && cargo test --quiet volume::write
```

Expected: PASS. If one fails, the surviving writer has a real gap — fix the writer, do not weaken the test.

- [ ] **Step 4: Delete the file and its re-exports**

```bash
cd src-tauri && rm src/core/adf/mutate.rs
```

In `src-tauri/src/core/adf/mod.rs`, remove the module declaration and line 21's re-export:

```rust
pub use mutate::{add_file, create_directory, delete_entry, rename_entry};
```

Remove `mutate_disk_file` at line 184 and the test at line 305 that used it. `create.rs:307` also calls it — replace that test's body with the volume writer, or delete the test if `create.rs` has an equivalent.

- [ ] **Step 5: Let the compiler find the rest**

```bash
cd src-tauri && cargo build
```

Fix each error by routing the caller through `core/volume/write`. Repeat until it builds. Then:

```bash
cd src-tauri && cargo clippy --all-targets -- -D warnings
```

Clippy names anything that became dead — `blocks.rs` helpers only `mutate.rs` used, for instance. Remove them.

- [ ] **Step 6: Run the whole suite**

```bash
cd src-tauri && cargo test
```

Expected: PASS, with a test count **lower** than before by the number of duplicated tests removed, and no lower than that.

- [ ] **Step 7: Verify nothing regressed on a real disk**

```bash
python scripts/oracle-check.py
```

Expected: all checks pass, both directions.

Then run `pnpm tauri dev` and, in **Disk Tools**, create a blank ADF, add a file to it, and boot-test it in WinUAE. Finally write the same image to a Gotek and confirm a real Amiga reads it — the only rung that settles whether ART's disks are Amiga disks.

- [ ] **Step 8: Checkpoint**

```bash
pnpm lint
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
python scripts/oracle-check.py
pnpm tauri build
```

---

## Task 11: Update the status documents

**Files:**
- Modify: `docs/ISSUES.md` (ART-037 … ART-040)
- Modify: `docs/FEATURES.md` (ADF HD row, filesystem rows)
- Modify: `docs/STATUS.md` (snapshot, session log)
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: Tasks 1–10
- Produces: documents that describe the code as it now is

**Why:** the project's rule is that STATUS/ISSUES/FEATURES describe reality and win when they disagree with anything else. Four defects were found and fixed; none is recorded.

- [ ] **Step 1: Record the defects**

Add to the Fixed section of `docs/ISSUES.md`, above the Stage W entries:

```markdown
### Phase 0a

**ART-037** 🔴 **ADF Studio could not open any bootable ADF**
`core/adf/bootblock.rs` · The parser read bytes 8..11 of the boot block as a
"root block pointer". An AmigaDOS boot block has no such field: 0..3 is the
DOS type, 4..7 the checksum, and 8 onwards is boot code. ART therefore read
68000 machine code as a block number and refused every bootable disk with
`root block has type <nonsense>`. Invisible to ART's own tests because every
fixture ART builds has zeros where a real disk has boot code — and invisible in
day-to-day use because the file manager, which runs on `core/volume`, opened
the same disks perfectly.
→ The root block is computed, `total_blocks / 2`, the way
`VolumeGeometry::root_block_for` already did. Pinned by
`a_bootable_image_opens_because_the_root_block_is_computed` and by a new
oracle check that has `xdftool` write a bootable floppy for ART to open.

**ART-038** 🟡 **HD ADFs reported half their capacity**
`core/adf/mod.rs` · `info()` computed capacity and parsed the bitmap with
`DD_TOTAL_BLOCKS`, so every number it reported for a 1.76 MB image was a
floppy-shaped guess. Same root cause as ART-037: `core/adf` predated the
Stage R geometry and never adopted it.
→ Both come from the image's own length. Pinned by
`an_hd_image_reports_its_real_capacity`.

**ART-039** 🟡 **Disabled controls were indistinguishable from active ones**
`src/styles/global.css` · The word `disabled` appeared in no stylesheet, so a
disabled primary button kept its accent fill. On the WHDLoad screen a
correctly-refused install showed a solid blue "Install to the disk" button
that did nothing when clicked.
→ `:disabled` and `:focus-visible` styles for buttons and form controls.

**ART-040** 🟡 **Content was clipped instead of scrolled, and nothing scaled**
`src/components/layout/layout.css` · `.app-main` is a grid item with the
default `min-height: auto`, so it grew to fit its content and `.app-content`'s
`overflow: auto` never engaged; the shell's `overflow: hidden` then clipped the
excess. `min-height: 0` appeared nowhere in the codebase. Separately, the
application had zero `@media` queries and eleven pages carried hand-written
`maxWidth` values.
→ The `min-height: 0` chain, breakpoints, and one central content width.
```

- [ ] **Step 2: Correct the feature table**

In `docs/FEATURES.md`:

```markdown
| **ADF** (HD) | ✅ | ✅ | ⏳ | ✅ | ✅ | ⏳ |
```

and replace the note beneath it:

```markdown
- **ADF (HD)** — reads, writes and validates. The DD-only assumption that made
  these fail was ART-037/038; geometry now comes from the image.
```

- [ ] **Step 3: Update the snapshot and log**

In `docs/STATUS.md`, set the current stage to `Phase 0a — live bugs fixed, one filesystem writer`, update the test count to whatever `cargo test` reports, and add a session-log row:

```markdown
| 2026-08-09 | Phase 0a: ADF Studio's root-block bug (ART-037/038), shell scroll and scale (ART-039/040), `core/adf/mutate.rs` retired | <count> |
```

Then update `docs/STATUS.md`'s known-limitations list: the preview note stays, the i18n note stays, and any claim that the shell scrolls or scales is now true rather than absent.

- [ ] **Step 4: Write the changelog entry**

Add to `CHANGELOG.md` under `## [Unreleased]`:

```markdown
### The window scrolls, and ADF Studio opens real disks (2026-08-09)

#### Fixed
- **ADF Studio could not open a bootable disk.** It looked for the root block
  in a place the Amiga does not keep one — in the boot code itself — so any
  disk that could actually start a machine was refused as damaged. It now
  works out where the root block is, the way every other Amiga tool does.
- **1.76 MB disks now work**, and report their real size rather than half of it.
- **Long pages scroll.** The window was cutting content off at the bottom with
  no scrollbar at all.
- **The window scales.** Narrow it and the sidebar collapses to icons instead
  of squeezing the panes; widen it and content stays centred instead of
  clinging to the left.
- **Buttons that cannot be used now look it.** A greyed button is a button you
  know not to click.
- **"ART will not do this" no longer looks like a crash.** Being told an
  archive is not a WHDLoad package is an answer, not a fault, and it no longer
  arrives in red with an error code.
```

- [ ] **Step 5: Checkpoint**

```bash
pnpm lint
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
python scripts/oracle-check.py
pnpm tauri build
```

---

## Self-review

**Spec coverage.** Phase 0a covers spec slices 0.1 and 0.2 in full:

| Spec item | Task |
|---|---|
| 0.1 `min-height: 0` scroll chain | 1 |
| 0.1 `@media` breakpoints, page `maxWidth` removed | 2 |
| 0.1 `:disabled` / `:focus-visible` | 3 |
| 0.1 shared refusal-vs-error component | 4 |
| 0.2 root block from geometry (D1) | 5, 7 |
| 0.2 HD ADFs work | 5, 6 |
| 0.2 retire `mutate.rs` and the `&[u8]` wrappers | 8, 9, 10 |
| 0.2 `create.rs` stays | 10, Step 4 |
| 0.2 ART-009…013 tests move rather than disappear | 10, Steps 1–3 |
| Verification ladder incl. real Amiga | 10, Step 7 |

Slices 0.3 (dead code) and 0.4 (Turkish) are **deliberately not here** — they are Plan B, because 0.4 is easier after Tasks 1–4 restructure components and part of 0.3's dead-code list is retired by Tasks 8–10.

Two spec items are partly served and finish in Plan B: `FEATURES.md` truthfulness (Task 11 corrects the ADF rows; the rest is 0.3) and the `&[u8]` wrapper removal in `fs.rs` / `extract.rs` / `validate.rs` (Task 10 Step 5 removes whatever clippy finds dead; any survivor is 0.3).

**Placeholder scan.** No TBDs. Every code step carries real code. Task 8 Step 4 asks the implementer to `grep` for `MutationOutcome`'s actual field names before writing the constructor — that is a deliberate instruction to check, not a placeholder, because the struct's fields were not read while planning.

**Type consistency.** `with_volume` (Task 8) is the function added during §82 and returns `(T, WriteStrategy, Option<String>)`. `run_copy_in_folder` (Task 9) is the §82 rename of `run_copy_in`. `write_bytes_into` (Task 8 Step 4) is new and used by Task 8 Step 3. `MutationOutcome::from_write` is new in Task 8 Step 4 and used by Task 8 Step 3. `pick_volume` (Task 8 Step 1) is the public helper added for the checkout commands. `VolumeGeometry::root_block_for` (Task 5) exists from Stage R.

**Known risk.** Task 10 deletes 858 lines of audited code. Its safety depends entirely on Steps 1–3 identifying every behaviour the surviving writer must keep. If Step 3 fails, the instruction is explicit: fix the writer, never weaken the test.
