# AmigaOS install engine — media to a system volume (SD-2 · G5)

**Date:** 2026-08-15
**Status:** approved
**Scope:** `core/osinstall/`, `core/preload/native.rs`, `core/preload/pfs3dev.rs`,
and the OS Builder screen over them
**Gap:** [sd-appliance-gap-analysis.md](../../sd-appliance-gap-analysis.md) G5

---

## What this document is

The design for turning user-provided AmigaOS media into a populated,
bootable system volume — **without running the Amiga Installer**, which is
177 KB of Amiga script that only an Amiga can run.

It is not a plan. The implementation plan follows.

## What changed underneath this design, on the day it was written

The gap analysis, the G11 spec and STATUS all rest on one sentence: **"ART
cannot write PFS3."** That sentence stopped being true on 2026-08-15 and this
design is the first that does not have to work around it.

[`libpfs3`](https://github.com/metaneutrons/pfs3) v0.1.3 (crates.io,
LGPL-3.0-or-later) is a pure-Rust PFS3 implementation that reads, writes and
formats. Four things about it were **measured, not read**:

| Question | Measurement |
|---|---|
| Windows MSVC build | Builds. Whole dependency tree is `byteorder` + `thiserror` |
| Reads what another implementation wrote | `hst-imager` wrote a PFS3 volume; libpfs3 read every name, size, protection bit, date, comment and byte |
| Writes what another implementation reads | libpfs3 wrote `Libs/`, `Libs/fake.library`, `WrittenByRust`; `hst-imager` read all three |
| Reads a **real** card | CaffeineOS Storm 9317, 59.5 GB, PFS3 system volume at byte offset 1 181 745 152 inside an MBR: `volume "CaffeineOS", 1 640 448 blocks, 847 408 free`, full tree, `h-p-rwed` bits intact |
| Sets protection bits on write | `PureCmd → --p-rwed`, `ScriptFile → -s--rw-d`; `hst-imager` reads back `--P-RWED` and `-S--RW-D` |

The README's "Windows not supported" refers to its FUSE driver, not the
library — the library declares no OS dependency and the build proves it.

That last row is why the finding matters to *this* gap specifically. AmigaOS
3.2's `Startup-Sequence` runs `Resident >NIL: C:Assign PURE`; without the pure
bit the boot fails. A PFS3 writer that could not set protection bits would be
useless for an OS install however well it wrote data.

**What was not measured, and is therefore not claimed:** writing at install
scale (thousands of files, ~200 MB), file comments on write, deletion and
re-run, and whether any Amiga boots a volume libpfs3 wrote. Each is a rung in
the plan, not an assumption in this design.

### What this costs, said plainly

- `libpfs3` is **v0.1.3** and its own README calls read-write mode
  experimental, recommending it be used "only on copies of disk images, never
  on originals". ART's answer is not to disregard that but to make it moot:
  every write goes through `core/safety` and the journal, which is what ART
  does with its own writer too.
- **PFS3 has no journalling.** That is the on-disk format, not the library —
  the original AmigaOS driver has none either. It is the reason the
  `BlockDevice` adapter below is not optional.
- `deny.toml` must gain `LGPL-3.0-or-later`, and `THIRD_PARTY_LICENSES.md` a
  row, in the same commit that adds the crate. LGPL-3.0 is compatible with
  ART's GPL-3.0-or-later. It does sit against the note already in `deny.toml`
  that ART prefers permissive dependencies **so its own modules stay
  reusable** — a future standalone `core` crate would carry a weak-copyleft
  dependency. That is a real cost, accepted deliberately, not overlooked.

## Where it sits

```
AmigaOS media (36 ADFs)          the user's own, read-only
        │
        │  core/osinstall
        ▼
a distribution tree on the PC    the product: files + .uaem + distribution.json
        │
        │  core/preload  (VolumeFormatter)
        ▼
a formatted, filled system volume on a card image or an HDF
```

Only the middle box is new engine. The bottom one exists (G3 route E) and has
been run for real; what this design adds to it is a second implementation of
the trait it already has.

## Decision 1 — the product is a tree, not a volume

**G5 produces a *distribution tree*:** a host folder that is the finished
system volume file for file, each file's Amiga metadata in a `.uaem` sidecar
beside it, and at the root a `distribution.json` recording which component
each file came from, which media it was read out of, and its SHA-256.

Three reasons, in the order they matter:

1. **It is what the user asked for.** "Build our own distributions" means a
   thing you keep, re-apply, and add to. A card is a *copy* of that thing. One
   tree, twenty cards.
2. **It is the only thing that makes component removal possible.** Removing a
   component cleanly requires knowing what it added. That record has to be
   born at install time; it cannot be reconstructed afterwards. This is the
   hinge on which the *next* piece of work turns.
3. **It is testable with no volume at all.** The whole engine runs in a
   tempdir — no card, no driver, no external binary.

And one that costs nothing: G11 already produces a staging tree, so games and
the OS reach a card through one mechanism rather than two.

**What the tree deliberately does not carry is a volume name.** Which
partition it lands on, and what that volume is called, are questions the
preload screen already asks and answers. Restating them here would put the
same question in two places, where the two can disagree — the reasoning G11's
spec used to keep volumes out of the layout module, applied again.

## Decision 2 — a component is a set of paths, not a disk

This is the design's central claim and it came from measuring the media, not
from reading about it.

`ModulesA1200_3.2.adf` has 14 commands in `C/`. **Thirteen are boot-floppy
copies of commands Workbench3.2 already carries**; exactly one, `LoadModule`,
is new. Copying that disk onto `SYS:` downgrades thirteen commands. The same
shape holds for `HDSetup3.2` (22), `DiskDoctor` (39) and `Storage3.2` (9).

So:

```rust
Component {
    id: String,                    // "modules-a1200"
    media: String,                 // volume name to look for: "ModulesA1200_3.2"
    rules: Vec<PathRule>,
    required: bool,
    condition: Option<Condition>,  // RomOlderThan(47)
    overrides: Vec<String>,        // component ids this one may write over
    user_startup: Vec<String>,     // lines for S:User-Startup
}

PathRule { from: String, to: String, kind: File | Subtree }
```

Recipes are **data**, in `core/osinstall/recipes/amigaos-3.2.json`, following
the pattern `core/distro/registry.json` set. Adding AmigaOS 3.9 or CaffeineOS
later adds a file, not a code path.

## Decision 3 — media is identified by content, and checksums are recorded, not enforced

SD-0 settled that install media is identified by content rather than filename.
Here, "content" means the **volume name inside the image** — `Workbench3.2`,
`Extras3.2`, `ModulesA1200_3.2` — which ART reads with the reader it already
has. A renamed ADF still works; a folder missing a disk is named, not guessed
around.

The SHA-256 of each ADF actually used is **written into
`distribution.json` and not used as a gate**. ART has no verified checksum
table for the 3.2 set, and refusing against a table copied from somewhere else
is precisely [ART-104](../../ISSUES.md): the user's own licensed A1200 ROM
hashed to a dump `KNOWN_ROMS` did not carry, and every card built with it was
warned about a ROM that was very probably right.

## Decision 4 — a collision inside a plan is a defect, not a policy

Components are path sets, so two of them should never claim one destination.
When they do, the preview names the path and both components **in red, and
Apply is held**. A silent "later wins" is exactly how the ModulesA1200
downgrade would arrive.

The one exception is declared: a component carrying
`overrides: ["workbench-base"]` is *meant* to replace files, and the preview
shows that as an intentional override rather than a collision. Nothing
overrides by accident.

### What the preview lets you change, and what it does not

**Components, and nothing below them.** Ticks, the language selection, and the
Modules override — those are the edit. The resulting file list is shown in
full and is **read-only**.

That is a narrower answer than G11's, where retargeting an individual row *is*
the feature, and the difference is not arbitrary: there, ART genuinely cannot
tell a demo from a game, so the user's judgement is the only source of truth.
Here every destination comes from a recipe that the media itself is checked
against, and a hand-moved file would make `distribution.json` describe
something that is not a release — which breaks the component removal the
manifest exists for. Moving files around inside an OS install is the next
piece of work (arbitrary packages), where it will be recorded as such rather
than smuggled in as an untracked edit.

## Decision 5 — conditions are derived from the ROM, never asked

`Workbench3.2.adf:S/Startup-sequence` opens with:

```
Version exec.library version 47 >NIL:
If Warn
  Echo " This disk must be booted from Kickstart ROM 3.2 (V47)"
  Echo " or from the 'Modules' disk that matches your hardware."
  Quit
EndIf
```

So a 3.2 system on a Kickstart 3.1 ROM without `LIBS:Modules` does not boot at
all. ART already identifies the paired ROM by checksum (ROM Manager, G9), so
the Modules component's tick arrives **already decided, with its reason on
screen**: *"on, because the paired Kickstart is 3.1 (40.68) and this
release's Startup-Sequence quits on anything below V47."*

The user may still turn it off. That is a **confirmed warning, not a
refusal** — it is their machine, and ART says exactly what will happen rather
than deciding for them.

This is where G5 and G9 meet, and it is the whole of the "which machine" question
the gap analysis worried about: the machine is a fact ART already holds, not a
question to add to a form.

## Decision 6 — ART does not write the Startup-Sequence

The release carries one that already handles both cases
(`IF NOT EXISTS SYS:L`, `IF NOT EXISTS SYS:Fonts`). It is a `PathRule` in
`workbench-base` like any other file. ART placing its own would be ART
guessing at something the release states — the rule ART-103 was filed for.

What ART *does* write is `S:User-Startup`, and it **edits in place** (§39/§40).
Each component's `user_startup` lines go between

```
;BEGIN <component-id>
...
;END <component-id>
```

which is the convention real Amiga installers already use — the release's own
idiom, not ART's invention. Removing a component means finding and deleting
its own block, leaving everything around it untouched.

## The recipe, v1

Measured against the user's own 3.2 set (36 ADFs).

| Component | Media | Notes |
|---|---|---|
| `workbench-base` | `Workbench3.2` | required |
| `extras` | `Extras3.2` | **carries `L/`, which `Workbench3.2` has none of at all** — the startup-sequence's `IF NOT EXISTS SYS:L` branch is the evidence it is expected from here |
| `fonts` | `Fonts` | |
| `classes` | `Classes3.2` | |
| `storage` | `Storage3.2` | |
| `locale-base` | `Locale` | required with any language: this disk is `Catalogs`, **`Countries`** and `Support`, and carries no `Languages` at all |
| `locale-<XX>` | `Locale-<XX>` | multi-select; these carry `Catalogs`, `Help` and `Languages`. 15 present, `Locale-TR` among them |
| `glowicons` | `GlowIcons3.2` | |
| `backdrops` | `Backdrops3.2` | |
| `diskdoctor` | `DiskDoctor` | |
| `mmulibs` | `MMULibs` | |
| `hdtools` | `HDSetup3.2` | |
| `modules-<model>` | `ModulesA1200_3.2` … | conditional, mutually exclusive; rules take `C/LoadModule`, `LIBS/Modules`, `DEVS/<model>`, `LIBS/<model>` and **nothing else** |

Registered `available: false`, so they render as "Coming Later" rather than
vanishing (CLAUDE.md, §96): the 3.2.1 and 3.2.2 updates, which are a different
media shape and want their own measurement round.

## Writing: no new trait

`core/preload::VolumeFormatter` is already the boundary —
`probe`, `import_filesystem`, `format_partition`, `copy_in`. It lives outside
`core/` for one reason: **its only implementation launches a program.**

A second implementation, `core/preload/native.rs`, launches nothing, so it
lives *inside* `core/`:

| Method | Native implementation |
|---|---|
| `import_filesystem` | ART already does this — G4's `create_rdb_layout` |
| `format_partition` | `libpfs3::format::format_with_size` for PDS3; ART's own for `DOS\1`…`DOS\7` |
| `copy_in` | reads the host tree and its `.uaem` sidecars; writes through libpfs3 (PFS3) or `core/volume/write` (FFS) |

Consequences worth stating: **preload becomes testable in CI with no binary
present**, and `hst-imager` stays registered — as a fallback if v0.1.3
disappoints, and more importantly as the oracle below.

### The BlockDevice adapter, and why it is not optional

The two traits are close enough that the adapter is small:

| | ART `core/volume` | `libpfs3::io` |
|---|---|---|
| bounds | `Send + Sync` | `Send + Sync` |
| read | `read_block(&self, u32, buf)` | `read_block(&self, u64, buf)` |
| write | `write_block(&mut self, …)` | `write_block(&self, …)` — interior mutability |
| extra | — | `read_blocks`, `write_blocks`, `flush` |

`core/preload/pfs3dev.rs` wraps an ART `BlockDeviceMut` in a `Mutex` — which is
what libpfs3's own `FileBlockDevice` does — widens the block number and maps
the error type.

**The point of the adapter is not tidiness.** PFS3 has no journalling of its
own, so `core/volume/journal.rs` is the only crash safety a PFS3 write can
have. Driving libpfs3 through ART's device puts every PFS3 block write inside
that journal: a block that has not been saved cannot be written, and a rollback
restores the image byte for byte. Using libpfs3's own `FileBlockDevice` would
leave an interrupted install as an unknown volume.

Limit, written down rather than discovered: ART's `total_blocks()` is `u32`, so
2 TB at 512-byte blocks. Far beyond any card.

## Verification, and the trap it avoids

ART writing with libpfs3 and then verifying with libpfs3 is a reader and a
writer that agree with each other and with nothing else. That is
ART-032 … ART-035, ART-075 and ART-079 — the same shape, four times. So:

- **In-app REPORT** reads the volume back and checks it against
  `distribution.json`: every file present, right size, right protection bits.
  What could not be checked is a separate list and **never renders as a
  tick** — G8's three states (`pass` / `fail` / `not-checked`) apply here
  unchanged (§89).
- **`scripts/pfs3-oracle-check.py`** — a new oracle with `hst-imager` as the
  independent implementation, **both directions**: ART writes and hst-imager
  reads, hst-imager writes and ART reads. It is a **local** oracle, not a CI
  one, exactly like `fat-oracle-check.py` and `iso-oracle-check.py`, because
  the binary is not on the runner. Said here so nobody later reads "there is
  an oracle" as "CI runs it".
- **The rungs, in order of strength**: ART reads it back (weakest — same code
  family) → `hst-imager` reads it (independent) → WinUAE boots it (the
  agreed criterion) → a real Amiga boots it (last, and open).

## Refusals

Typed variants, not sentences — `core/` is English and ART-060 is open, so a
refusal the UI must translate has to arrive as a value. G11's `RefusalReason`
is the pattern.

| Refusal | Why it stops rather than continues |
|---|---|
| `MediaMissing { component, volume_name }` | No ADF in the folder has volume `Extras3.2` and `extras` needs it. Which one, not "something is missing" |
| `MediaPathMissing { component, path, media }` | The recipe expects `LIBS/Modules` on `ModulesA1200_3.2` and it is not there — **the recipe is wrong about this media**. A silently skipped path is a system missing a library |
| `RomUnknown` | The ROM was not identified, so the Modules condition cannot be decided. Guessing wastes 800 KB or produces a system that quits at boot |
| `DoesNotFit { needed, available }` | Real block numbers, before anything is touched |
| `DestinationCollision { path, components }` | Two components, one path, no declared override — a recipe defect |
| `NameNotStorable { name }` | Via the existing `check_name`, asked before the write rather than discovered during it |

One case is deliberately **not** a refusal: turning Modules off on a pre-V47
ROM is a confirmed warning. It is the user's machine.

## Safety

§92's pipeline in full:

| Stage | Here |
|---|---|
| SOURCE | the media folder, opened read-only, never written |
| ANALYZE | scan the folder, identify each image by its volume name |
| VALIDATE | every path every selected component names exists in its media |
| RECOMMEND | components ticked with their reasons |
| PREVIEW | the editable component list, the resulting file list, the totals |
| BACKUP | the tree is `SAFE_CREATE` — an existing distribution folder is refused, never written into; a volume write goes through the journal |
| APPLY | a job: `ProgressSink`, cancel checked between whole files |
| VERIFY | read back against `distribution.json` |
| REPORT | an oplog entry carrying what was verified and what could not be |

Every destination goes through `safe_join`. Recipe paths are data a human
typed; G11 proved the point by measurement — removing `safe_join` there
genuinely wrote a file outside the staging root before being reverted.

## Module layout

```
core/osinstall/
  mod.rs        types: Component, Recipe, InstallPlan, Refusal
  recipe.rs     load and validate the JSON recipes
  recipes/amigaos-3.2.json
  source.rs     MediaSource trait; AdfSource, IsoSource, FolderSource
  scan.rs       identify media in a folder by volume name
  plan.rs       components + media + ROM -> InstallPlan (pure, no I/O beyond reads)
  apply.rs      plan -> distribution tree + distribution.json (job, cancellable)
core/preload/
  native.rs     VolumeFormatter over libpfs3 + core/volume/write
  pfs3dev.rs    the BlockDevice adapter
commands/osinstall.rs      thin adapter
src/lib/osinstall.ts       typed wrapper
```

`IsoSource` exists in the trait and has no recipe in v1. It is the seam AmigaOS
3.9 arrives through, and it costs one file when it does.

### The screen

**Inside the OS Builder**, as its second kind, not a new route.

G11 became its own `/layout` page and that was right — but layout is not OS
building. The OS Builder today leads with a boot-only card and lists
distributions as Coming Later; G5 is exactly what makes those real. Putting it
anywhere else would leave the screen named "OS Builder" unable to build an OS.

## Testing

- **Fixtures are synthetic and built at runtime in a tempdir.** ART ships no
  copyrighted Amiga content, ever. Tests write tiny ADFs with ART's own writer
  holding the paths a test recipe names.
- **The recipe JSON has its own tests**: every `to` is a legal AmigaDOS path,
  no two components collide without a declared override, every condition is
  decidable, every `media` is referenced by at least one rule.
- **Security**: `safe_join` on every destination, with a test that fails
  without it.
- **Data safety**: a failed or cancelled apply leaves the media byte-for-byte
  unchanged, proved by a test; the tree is `SAFE_CREATE`.
- **Mutation-checked**, because these are the load-bearing rules: the Modules
  condition, the collision refusal, `MediaPathMissing`, `safe_join`, and the
  protection-bit transfer.
- **The wire is pinned in Rust** — a test deserialising the literal object
  `src/lib/osinstall.ts` builds, the precedent the preload screen set.
- **Real material, twice**: the whole engine against the user's actual 3.2 ADF
  set, and `pfs3-oracle-check.py` both ways.

## Out of scope for v1, registered rather than hidden

- AmigaOS 3.9 / ISO as a source — the trait has the slot, the recipe does not exist
- The 3.2.1 and 3.2.2 updates
- Arbitrary package add and remove — the next piece, and what `distribution.json` exists for
- Editing an existing distribution (CaffeineOS, AmiKit) — the piece after that,
  now unblocked by libpfs3 but not designed
- Multiboot as several complete environments — G16, SD-3

## The decomposition this belongs to

Recorded because the scope was set by splitting a larger request, and the order
is not arbitrary:

1. **Install from media** — this document.
2. **Add and remove components** — builds on `distribution.json`, which is why
   the manifest is born here rather than added later.
3. **Edit an existing distribution** — borrows 2's model. Its wall (no PFS3
   access) came down on 2026-08-15; its remaining honesty problem did not: ART
   did not build CaffeineOS, so it has no manifest for it and can remove only
   what it itself added.
