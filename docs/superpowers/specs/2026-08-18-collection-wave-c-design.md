# The richer screen — pictures already on disk, a panel, and Play (SD-2 · G10 wave C)

**Date:** 2026-08-18
**Status:** approved (2026-08-18)
**Scope:** a new offline artwork pass, a manual art binding in the user layer,
a detail panel on the Collection screen, and launching a title in WinUAE —
including WHDLoad drawers, which need a bootable system ART does not own
**Follows:** [2026-08-17-catalogue-persistence-design.md](2026-08-17-catalogue-persistence-design.md)
(wave A, the saved catalogue) and
[2026-08-17-collection-artwork-design.md](2026-08-17-collection-artwork-design.md)
(wave B, artwork from configured sources)

---

## What this document is

The design for the last of the three rounds the user asked for after G10's
index landed. A and B are done; this is C, and the user's own words for it have
always been *LaunchBox-shaped*: an art grid, a detail panel, a play button.

It is not a plan. The implementation plan follows this document.

**One wave, not two.** The four parts below were offered split — the screen
first, Play second — and the user chose to take them together. The order of
work inside the wave (§7) is what keeps that honest: something is visible on
screen after each part, rather than at the end.

## The problem, measured

Four separate gaps, and only one of them is about the network.

1. **242 pictures ART already holds are rendered nowhere.** Every `.rp9` in the
   user's `E:\amiga\Titles` folder carries an embedded `screen-running` PNG.
   The reader parses it — `preview` is populated for 242 of 242 — and the
   Collection has never drawn one. Amiga Forever's own grid is built out of
   exactly these images.
2. **Artwork from the network matches 3% of that folder.** Wave B's two sources
   match 60% against the WHDLoad folder and 3% against this one, because the
   titles are hand-named and the sources are keyed by canonical names. No
   online source is going to fix that; the user attaching a picture will.
3. **A card cannot be opened.** Everything the index knows about a title —
   the disk order, the slave's name, the declared Kickstart, the path, the
   rating and genre the `.rp9` manifest carries — either fits on a card or is
   not shown at all. Most of it is not shown at all.
4. **Nothing launches.** ART can generate a WinUAE configuration and start
   WinUAE (`core/winuae.rs`, used by the WinUAE screen), and a catalogue of
   more than two thousand titles has no way to reach it.

## Where each part lives

The core-independence rule decides most of this before taste does.

| Part | Home | Why there |
|---|---|---|
| Extracting a `.rp9` preview | `core/artwork/sources/rp9.rs` | A third source beside `libretro.rs` and `whdload_de.rs` — but it takes no `MirrorClient`, because it touches no network |
| The offline pass itself | `core/artwork/local.rs` | `enrich()` requires a client; this one must be callable with none. Separate entry point, same `Cache` |
| A hand-attached picture | binding in `core/gameindex`'s user layer, bytes in the artwork cache | §2 — the two halves have different owners on purpose |
| Which profile, which ROM, which media | `core/launch/` (new) | Platform-independent: it produces a `LaunchPlan`, it starts nothing |
| Starting WinUAE | `core::winuae::launch_winuae` via `commands/winuae.rs` | Already outside `core/`, and stays there — launching a Windows binary is the trait rule's other half |
| The panel | `src/pages/CollectionStudio.tsx`, pure parts in `src/lib/` | A `src/lib` helper returns a `Phrase`; the component calls `t()` |

## 1. The offline pass: 242 pictures, no network

`record.preview` holds **the entry's name inside the zip** (`rp9-preview.png`),
not its bytes. Rendering it is therefore an extraction, and extraction of an
untrusted archive entry has exactly one legal route in this codebase:
`core/archive`'s gate, which bounds the read and refuses a name that escapes.

```rust
// core/artwork/local.rs
pub struct LocalPreview {
    pub title: String,       // the catalogue's title, for the cache key
    pub package: PathBuf,    // the .rp9 on disk
    pub entry: String,       // record.preview, verbatim
}

pub fn adopt_local(
    cache_dir: &Path,
    previews: &[LocalPreview],
    sink: &dyn ProgressSink,
) -> CoreResult<LocalOutcome>;
```

Each one becomes `Cache::store(title_key, ArtKind::Snap, "rp9", "png", bytes)`,
which is the same call wave B's sources make, so the grid and the panel need no
new rendering path at all: they already read the cache.

Three properties this must have:

- **It asks nothing.** No source configuration, no mirror, no consent question,
  because nothing leaves the machine. The Collection can offer it as *use the
  pictures already in your files* and simply do it.
- **It is skipped when already done.** `Cache::adopt` exists for precisely this
  — a picture on disk from an earlier run is picked up rather than rewritten.
- **A hostile entry name is refused, not sanitised.** `../../evil.png` inside a
  `.rp9` is a test, not a footnote.

`ArtKind::Snap` rather than a new kind: a `screen-running` capture *is* a
snap, and `ART_KINDS`' preference order already puts boxart above it, so a
title that later gains real box art from libretro shows the box art and keeps
the snap.

`EnrichRequest.wanted` widens here as its own comment anticipated — the panel
renders more than one kind, so more than one is worth fetching.

## 2. A hand-attached picture, and which layer owns it

The artwork cache is **derived data**. Its own module says so, and `save()` is
atomic but unbacked: it can be deleted and rebuilt from the sources. A picture
the user chose by hand is not derived data and must not live where a rebuild
can lose it.

So the binding splits:

- **The bytes** are copied into the artwork cache under source `manual`
  (`Cache::store`), so every screen renders it through the path it already
  uses.
- **The fact that the user chose it** goes in `core/gameindex`'s user layer,
  beside `title`, `year`, `publisher`, `genre` and `chipset` — the layer
  described in wave A as the one no refresh touches. `RecordOverride` gains
  `art: Option<ArtBinding>` carrying the source path the user picked and the
  cache file it produced.

That placement buys three things at once: a catalogue refresh cannot lose it, a
later online fetch cannot overwrite it (a title whose override names a manual
picture is skipped by the enrich pass), and deleting the cache re-materialises
it rather than losing it.

Accepted formats are **PNG and JPEG** — what the webview draws without help.
IFF/ILBM is not promised; ART has no ILBM decoder on this path and a format
that half-works on screen is worse than one that is honestly absent.

Removing a binding empties it from the user layer, and `RecordOverride`'s
existing rule applies unchanged: an override that says nothing is deleted
rather than stored, so *I changed my mind* leaves no trace.

## 3. The detail panel

The grid stays as it is. Selecting a card opens a panel — beside the grid on a
wide window, beneath it on a narrow one — carrying what a card cannot:

- the picture, large, with a switch between the kinds this title has;
- title, publisher, year, genre, rating, each with its existing `Guessed` mark
  where the value was inferred rather than read;
- **the media, spelled out**: the disk order for a floppy set, the slave's name
  for a WHDLoad drawer, the image for a hardfile;
- the declared Kickstart (G10 reads it; ART-130 is the half that will one day
  offer to supply it);
- the file's path and whether it is still there;
- and the actions: **Play**, attach a picture, edit the title, rename the file.

House rules it inherits rather than reinvents: Beginner mode hides the path and
the block-level detail and **hides only** — no action is disabled by mode;
Application Size scales it like every other screen, so no fixed pixel heights;
and whether the panel is open, and which title is selected, are **remembered**
through `src/lib/remembered.ts`, because a choice the user made may not reset
itself between runs.

## 4. Play

### 4.1 What each medium needs

| `Media` | What ART does | New work |
|---|---|---|
| `Floppies` from bare `.adf` | `floppy0..3` in the order the catalogue holds | none |
| `Floppies` inside a `.rp9` | extract the ordered images to ART's own data directory, then as above | extraction |
| `Hardfile` | one hardfile — **write-protected** if it is the user's own `.hdf`, **writable** if it is ART's own copy unpacked from a `.rp9` (so the game's saves survive) | none |
| `WhdloadDrawer` | §4.3 | directory mounts, a boot directory |

WinUAE takes four floppy drives. A set with more disks is launched with the
first four and **ART says so** — the emulator's own disk swapper handles the
rest, and claiming otherwise would be the kind of quiet half-truth §89 exists
to forbid.

Extraction goes to `<ART data dir>\launch\<record id>\`, never beside the
user's file.

### 4.2 Which machine, which ROM

The profile comes from the chipset the catalogue recorded: `aga` → `a1200-aga`,
`ocs`/`ecs` → `a500-ocs`, unknown → the user's default. The Kickstart comes
from their ROM folder, identified by the table `core/rom` already carries and
filtered by `rom_suits`.

**Unknown is the common case, not the edge.** 1536 of the WHDLoad titles carry
no chipset signal from any local source — that gap is what wave B's round 2 was
about and it is still open. So the default profile is what most titles will
launch with, which makes it a first-class setting rather than a fallback: it is
chosen in Settings, shown on the confirmation, and remembered per title the
moment the user changes it for one.

**When no ROM suits, ART refuses.** It does not launch a machine that will show
a black screen or an insert-disk hand and leave the user to work out why. This
is the same shape as G9's pairing verdict, and the wording lives beside it.

The proposal is shown before anything starts, it can be changed, and the
change is **remembered per title** — the settings rule applies to a launch
choice as much as to a pane's sort order.

### 4.3 WHDLoad: Y1 as the floor, Y2 on top

A WHDLoad slave is not a program WinUAE can run. It needs an Amiga that has
booted, with WHDLoad installed. ART does not own such a system and will not
pretend to build one for this wave — the user has `E:\amiga\amikit\AmiKit.hdf`,
which is one.

**Y1 — mount and hand over.** The user's system image is mounted as the boot
hardfile, **write-protected**, and the game's drawer is mounted as a directory
volume beside it. WinUAE boots to Workbench and the user starts the game. No
assumption about the system's layout, nothing written anywhere, works with any
system image, and it is the floor Y2 falls back to.

**Y2 — one click.** ART mounts a boot directory **of its own** at the highest
boot priority, holding a five-line `S/Startup-Sequence` that assigns from the
mounted system volume, enters the game's drawer and runs `WHDLoad <slave>`.
The only thing written is inside ART's own data directory. Y2 depends on the
user's system exposing what the startup-sequence assigns, which is why Y1 is
never removed from the panel — *open it and leave it to me* stays one click
away, and that pairing is the same shape `run_with_fallback` already uses in
`commands/preload.rs`: the good path first, a named alternative behind it,
never a silent one.

`LaunchMedia` grows the field that makes both possible:

```rust
pub struct DirMount {
    pub host_path: String,
    pub volume: String,     // DH1
    pub label: String,      // Games
    pub boot_priority: i8,
    pub read_only: bool,
}
```

emitted as WinUAE's `filesystem2=` lines, tested the way that module is already
tested — by asserting on the generated configuration text.

### 4.4 What is written, and what is not

- The user's system image: **read-only** (`write_protect_hardfiles`, which
  exists and is spec §93's rule).
- The game's drawer: **writable**, deliberately. WHDLoad keeps save games
  beside the game, and a launcher that silently discards a saved position is
  not a launcher. It is stated on the confirmation screen rather than assumed.
- ART's boot directory and the extraction directory: ART's own, under its data
  directory.
- Every launch goes through the operation log. It starts an external process
  against the user's files, which is exactly what §53 is for.

## 5. Testing

Rust, all synthetic and in a tempdir:

- a `.rp9` fixture's preview is extracted and cached (the fixture builder
  already exists in `readers/rp9.rs`);
- an entry named `../../evil.png` is refused;
- a second pass adopts rather than rewrites;
- a manual binding survives a catalogue refresh, and an enrich pass does not
  overwrite it;
- a launch plan for each `Media` variant, including the four-drive statement;
- the generated configuration contains the directory mount, the read-only
  system image and the boot priorities;
- no suitable ROM produces a refusal, not a launch.

Frontend: the panel's pure logic in `src/lib` with its `Phrase` keys enumerated
in `phrase-keys.test.ts`, i18n parity in both catalogues, and the dynamic-call
count in `literal-keys.test.ts` updated with its reason.

**And three things only the user can verify**, in the project's own tradition
that real material beats tests:

1. 242 previews actually appear, from their own folder;
2. one floppy title actually boots in WinUAE;
3. one WHDLoad title actually starts against `AmiKit.hdf` — both Y2 and, if Y2
   cannot reach it, Y1.

## 6. What this wave does not do

Named so they read as decisions rather than oversights:

- **It does not build a system volume.** The OS install engine (G5) can, and
  pointing it at this problem is a wave of its own. The user has a working
  AmiKit.
- **It does not ship or fetch WHDLoad.** ART distributes nothing; the system
  image the user points at has it or it does not.
- **It does not decode IFF/ILBM** for hand-attached art.
- **It does not offer to supply a missing Kickstart image** — that is
  [ART-130](../../ISSUES.md), filed and still open.
- **It does not add an online source.** Wave B's two stay as they are; this
  wave's new source is the user's own disk.

## 7. Order of work

Each step ends with something on screen, which is the point of the order:

1. **The offline pass** — 242 pictures appear in the grid. Smallest change,
   largest visible result, no new UI.
2. **The detail panel** — the place the next two parts live.
3. **Attaching a picture by hand** — needs the panel.
4. **Play** — the largest part, and the one whose verification needs the user
   and a real emulator.
