# Content layout policy — what goes where (SD-2 · G11)

**Date:** 2026-08-15
**Status:** approved
**Scope:** `core/layout/`, and the screen over it
**Gap:** [sd-appliance-gap-analysis.md](../../sd-appliance-gap-analysis.md) G11

---

## What this document is

The design for the piece that makes *"drop 400 files, get an organised card"*
real. The classifier exists (`core/detect`, `core/whdload`); what is missing is
the policy layer between it and the card.

It is not a plan. The implementation plan follows.

## Where it sits

SD-2's preload screen (G3 route E) asks for **a folder of content** per
partition and copies its tree into the volume. Nothing in ART builds that
folder. This does.

```
a pile of files  →  core/layout  →  a staging tree on the PC  →  preload  →  the card
```

That seam was chosen over the two alternatives on one fact: **a real PiStorm
card is PFS3 and ART cannot write PFS3.** Writing straight into the volume
through `core/volume/write` would work only on FFS, which is not what a
finished card uses; it becomes possible when route B (SD-4) lands, and then
this module's output is what it writes. A plan with no copy — one external
process invocation per file — was rejected for 400 invocations of
`hst.imager`.

The staging tree has a second, unpaid-for virtue: you can open it and look
before any of it reaches a card.

## The one concept

**Every item gets a destination path, relative to the staging root.** There is
no "volume" concept in this module, and deliberately so — the top-level
drawers already behave as volume names (`staging/Games`), and *which real
partition* is a question the preload screen asks and answers. Adding a volume
type here would restate that question in a second place, where the two could
disagree.

A card with one partition points it at the staging root and gets `Games/`,
`Floppies/` as drawers. A card with three points each at a top-level folder.
Same tree, no extra concept.

## What ART may and may not say

The gap analysis names `Games/` and `Demos/` as sibling drawers. **ART cannot
tell a demo from a game.** `Detection` carries a category, a format hint and a
confidence; nothing in it, and nothing derivable from the bytes, separates
those two. `core/whdload::analyse` can say "this archive is a WHDLoad pack",
which is a real answer, and that is the extent of it.

So the policy proposes only what it can justify, and the preview is
**editable**: the user re-targets a row, or many rows at once. This is §14/§34
applied to layout — an uncertain classification is offered as a
recommendation, never acted on as fact — and it is why the design has an
editable table rather than a cleverer rule engine.

Filename-pattern rules (`*demo*` → `Demos/`, the shape `colourRules.ts`
already has) are a plausible later addition and are **out of scope for v1**: a
second rule-editor UI, for a job the editable preview already does.

## `core/layout/policy.rs` — the rules, as data

```rust
pub enum Placement { CopyFile, CopyTree, UnpackWhdload }

pub enum WhdloadPlacement { Unpack, AsArchive }

pub struct Policy {
    pub whdload: WhdloadPlacement,
    pub drawers: Vec<Rule>,
}
```

Shipped default:

| What ART can justify | Drawer |
|---|---|
| WHDLoad pack (`core/whdload::analyse`) | `Games` |
| Floppy image (ADF, ADZ, DMS) | `Floppies` |
| Hard disk image (HDF) | `HardDisks` |
| Optical image (ISO, raw track) | `CDs` |
| Archive that is not a WHDLoad pack | `Unsorted` |
| Unknown | `Unsorted` |
| **ROM** | **not placed** — it is the FAT32 partition's business |
| **Commodore 8-bit** | **not placed** — no business on an Amiga volume |

The last two are refusals *with a reason*, carried on the item and rendered in
the preview. `core/card/intake.rs` already answers both that way for a card;
saying it again here rather than silently dropping the file is the same rule.

`WhdloadPlacement::AsArchive` copies the `.lha` into `Games/` untouched. Both
branches ship because the user asked for both; the default is `Unpack`,
because a card that arrives ready is the point.

## `core/layout/mod.rs::plan()` — pure

```rust
pub struct LayoutItem {
    pub source: PathBuf,
    pub kind: ItemKind,
    /// Relative to the staging root. Proposed by the policy, changed by
    /// the user.
    pub destination: String,
    pub placement: Placement,
    pub bytes: u64,
}

pub struct LayoutPlan {
    pub root: PathBuf,
    pub items: Vec<LayoutItem>,
    pub refused: Vec<Refusal>,
    pub collisions: Vec<Collision>,
    /// What the staging tree will need on disk.
    pub bytes: u64,
}
```

Inputs are paths — files or folders. A folder is walked; each file found
becomes an item.

The walk needs `core/collection`'s **rules** — depth-limited, and
`symlink_metadata` so a Windows junction cannot make a cycle — but not its
function: `scan_collection_directory` returns `CollectionItem`, which carries
a title, a year and a publisher and is about a game collection rather than
about a tree. `core/layout` gets its own walk holding the same two rules, and
the plan's first task is to decide whether the shared part is worth lifting
into one place or whether two short walks that agree is the smaller thing.

**The one non-obvious rule:** a folder that *is* a WHDLoad drawer is placed
whole rather than walked. Without it, dropping a folder of 400 files scatters
the insides of every game across `Unsorted/`.

Whether a folder is a drawer is decided **from its own contents** — it holds a
`.slave` — and not by handing it to `core/whdload::analyse`, whose question is
a different one: that function reads an unpacked *archive's* entry list, where
exactly one drawer sits beside its own `.info`, and a folder holding fifty
games is not that shape. If the parent listing has a `<drawer>.info`, it
travels with the drawer, beside it. Both halves have their own test.

`bytes` is reported because an unpacked WHDLoad archive is larger than the
archive, sometimes much larger, and saying the number before the button is
what this project does everywhere else.

## `core/layout/apply.rs` — the job

Runs under `ProgressSink`, cancellable **between items and never inside one**
(§54). Three rules:

- **The source is never touched.** A test asserts it byte for byte.
- **Nothing overwrites.** A destination that already holds that name is a
  `Collision` on the plan, reported before the button; the applier refuses
  rather than resolving.
- **Unpacking goes through the one gate.** `core/archive/extract.rs` for the
  security pass, then `core/whdload::analyse` over the unpacked entry list for
  the pack layout — which is exactly the shape that function was written for
  (§82) — and the drawer's `.info` is placed **beside** the drawer, not inside
  it. Put it inside and the game is on the disk and invisible on Workbench,
  which is indistinguishable from a failed install.

Errors are `CoreError`. A refusal (a ROM, a C64 disk) is an item state, not an
error — the same distinction Phase 0a drew for WHDLoad.

## The screen

A second slice, after the engine, the way G2 and G3 both went.

An editable preview table: source · what ART thinks it is · destination
(a dropdown of the drawers in the policy, plus free text) · size. Multi-select
retargets many rows at once. Under it: the total, the refusals with their
reasons, the collisions. Then Apply, as a job.

The staging folder is then pointed at by hand from the preload screen. **No
automatic chaining in v1** — the two screens each do one thing, and a wire
between them is a decision neither has earned yet.

## Testing

- The planner is pure and tested against synthetic fixtures built in a
  tempdir — ART ships no copyrighted Amiga content, ever.
- The WHDLoad-drawer-placed-whole rule, its own test.
- The applier materialises into a tempdir and is checked file by file.
- One hostile archive through the gate (traversal), as every backend gets.
- **Source unchanged, byte for byte**, after a successful apply and after a
  cancelled one.
- A refused ROM and a refused C64 disk reach `refused`, not `items`.

## Out of scope, in writing

- Filename-pattern rules and a rule editor (the editable preview does the job).
- Aminet category as a source of truth (`core/sources` knows the directory a
  package came from — real, but only for files ART downloaded, and it would
  make the policy's answer depend on the file's history rather than on the
  file).
- Chaining the staging folder into the preload screen automatically.
- Writing into the volume directly. That is route B, SD-4.
