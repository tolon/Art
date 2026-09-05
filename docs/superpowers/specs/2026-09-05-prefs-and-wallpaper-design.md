# Prefs and wallpaper — a tree that looks like the owner's own machine

*Written 2026-09-05. **This is history**: it describes the tree and the outside
world on the day it was written. Re-run the commands below rather than
re-trusting the numbers.*

**Round 1 of the Emu68 Hatcher intake.** The owner ruled that ART takes what is
worth taking from `rootrootde/emu68hatcher` (MIT), and the intake was ordered
into six rounds; this is the first. It closes what is left of **SD-3 G14** and
work-list item 7, which the 2026-09-04 work list correctly recorded as *"new
scope and not designed"*.

The teardown note that opened the intake is
[2026-09-05-emu68hatcher-teardown.md](../notes/2026-09-05-emu68hatcher-teardown.md).
**That note is incomplete**: it read their `builder/` and missed the
`configure_*` family, `staging/prefs.py`, and the IFF PTCH decoder entirely.
The findings below come from reading their tree again, and — for every byte
layout — from measuring real files rather than from their source.

---

## 1. What was measured, and how to measure it again

Nothing in section 3 rests on recalled documentation or on their code. Every
offset below was read out of a file on this machine.

### 1.1 `WBPattern.prefs` — the wallpaper

Three real files were dumped: the one in ART's own built AmigaOS 3.2 tree
(`E:\amiga\ProjeART\dist-3.2`), the one in the AmigaOS 3.9 tree
(`E:\amiga\Amigatolon\os39\art3`), and the `Christmas` preset that ships with
3.9. The 3.2 and 3.9 files are byte-identical at 462 bytes.

```bash
od -A d -t x1z "E:/amiga/ProjeART/dist-3.2/Prefs/Env-Archive/Sys/WBPattern.prefs"
od -A d -t x1z "E:/amiga/Amigatolon/os39/art1/Prefs/Presets/Christmas/wbpattern.prefs"
```

The container is a standard `FORM` … `PREF`, a 6-byte `PRHD` chunk, then **one
`PTRN` chunk per backdrop**. The `PTRN` body is a 24-byte header followed by
`wbp_DataLength` bytes:

| Offset | Field | Type | Measured |
|---|---|---|---|
| 0 | `wbp_Reserved[4]` | 16 bytes | always zero in all six chunks read |
| 16 | `wbp_Which` | `u16` | `0` root, `1` drawer, `2` screen |
| 18 | `wbp_Flags` | `u16` | see below |
| 20 | `wbp_Revision` | `i8` | `0` in every chunk |
| 21 | `wbp_Depth` | `i8` | `0` for a picture, `3` for a pattern |
| 22 | `wbp_DataLength` | `u16` | length of what follows |
| 24 | data | `DataLength` bytes | a NUL-terminated path, **or** bitplanes |

The field names are `prefs/wbpattern.h` from the NDK; the values are this
machine's files. The flag constants are `WBPF_PATTERN` `0x0001`, `WBPF_NOREMAP`
`0x0010`, dither mask `0x0300` (`BAD` `0x0100`, `GOOD` `0x0200`, `BEST`
`0x0300`), precision mask `0x0C00` (`ICON` `0x0400`, `IMAGE` `0x0800`, `EXACT`
`0x0C00`), placement mask `0x3000` (`TILE` `0x0000`, `CENTER` `0x1000`, `SCALE`
`0x2000`, `SCALEGOOD` `0x3000`).

**The discriminator is `WBPF_PATTERN`.** Clear means the data is a path;
set means the data is raw bitplanes. Both halves were confirmed:

- **Picture.** 3.2/3.9 root chunk: `Which=0`, `Flags=0x2A00`, `Depth=0`,
  `DataLength=43`, data `Sys:Prefs/Presets/Backdrops/default_pal.iff\0` — 42
  characters plus the NUL, exactly 43. `0x2A00` decodes as
  `PLACEMENT_SCALE | PRECISION_IMAGE | DITHER_GOOD`, and `WBPF_PATTERN` is
  clear.
- **Pattern.** The `Christmas` preset's screen chunk: `Which=2`,
  `Flags=0x0001` (`WBPF_PATTERN`), `Depth=3`, `DataLength=0x60`=96. A pattern
  is `PAT_WIDTH`×`PAT_HEIGHT` = 16×16 bits = 32 bytes per plane, and
  32 × 3 planes = 96. The arithmetic closes exactly.

`0x2A00` is **the release's own choice**, so it is ART's default for a picture
rather than something invented here.

The 3.2 drawer chunk is `Which=1`, `Flags=0x0500`
(`PRECISION_ICON | DITHER_BAD`, placement `TILE`), pointing at
`Sys:Prefs/Presets/Backdrops/pattern.iff`.

**Their implementation is wrong and is not copied.**
`builder/staging/prefs.py::generate_wbpattern_prefs` packs `>BB HH` — six
bytes — where the structure's header is twenty-four, and it omits
`wbp_Reserved` entirely. Every real file measured has the 16 reserved bytes.

### 1.2 `ScreenMode.prefs`

```bash
od -A d -t x1z "E:/amiga/Amigatolon/os39/art3/Prefs/Env-Archive/Sys/ScreenMode.prefs"
```

62 bytes: `FORM`/`PREF`, `PRHD` 6, then one `SCRM` chunk whose body is 28 bytes:

| Offset | Field | Type | Measured |
|---|---|---|---|
| 0 | reserved | 16 bytes | zero |
| 16 | `DisplayID` | `u32` | `0x00029000` |
| 20 | `Width` | `u16` | `0xFFFF` — "use the mode's own" |
| 22 | `Height` | `u16` | `0xFFFF` |
| 24 | `Depth` | `u16` | `4` — sixteen colours |
| 26 | `Control` | `u16` | `1` |

16 + 4 + 2 + 2 + 2 + 2 = 28, which is the chunk size the file states. Their
code writes the depth as a single byte at `body + 25`; that is the **low half**
of this `u16` and works for every depth, but ART writes the whole field.

### 1.3 ILBM, from a real Amiga picture

```bash
od -A d -t x1z "E:/amiga/Amigatolon/os39/art1/Prefs/Presets/WBClock/Backgrounds/UhrenskinAMIGA.iff"
```

`FORM` … `ILBM`, then `BMHD` (20 bytes): width `0x00D8` = 216, height `0x00D5`
= 213, x = 0, y = 0, `nPlanes` = 8, masking = 2, **compression = 1**
(ByteRun1), transparent colour 0, aspect 22:22, page 640×480. Then `CMAP` at
`0x300` = 768 bytes = 256 entries × 3, which is what 8 planes requires.

### 1.4 The oracle — proven, not listed

ART verifies every format it writes with something that is not ART. For ILBM
that thing had to be found, and four candidates were **run** rather than
assumed:

| Candidate | Result |
|---|---|
| **ffmpeg 9.0.1** (`Gyan.FFmpeg`, winget) | **Works.** `iff_ilbm` decoder present; decoded all five real `.iff` files on this machine. `UhrenskinAMIGA.iff` came out **216×213, `pal8`** — identical to the `BMHD` read by hand in §1.3. |
| Pillow 12.3.0 | **Refuted.** `PIL.UnidentifiedImageError: cannot identify image file`. |
| ImageMagick | Not installed on this machine. |
| `sirxyzzy/ilbm` | A **Rust** crate, not Python, decoder-only, last commit 2020-10-12. Rejected as an oracle: an abandoned single-author decoder is weaker evidence than ffmpeg's. |

**ffmpeg has no ILBM *encoder*** — `ffmpeg -encoders` lists none. That is the
right shape rather than a limitation: ART writes and an independent
implementation reads, which is exactly how `scripts/oracle-check.py` uses
amitools. It does mean the oracle cannot check an ART *reader*, and ART does
not build one this round.

### 1.5 A finding that came free

`E:\amiga\ProjeART\dist-3.2`, the AmigaOS 3.2 tree ART built from the owner's
own ADFs, ships a `WBPattern.prefs` naming
`Sys:Prefs/Presets/Backdrops/default_pal.iff` and
`Sys:Prefs/Presets/Backdrops/pattern.iff`. Its `Prefs/Presets/` holds
**`Backdrops.info` and no `Backdrops` drawer at all** — an orphan icon, and two
prefs paths pointing at nothing.

**This is not filed as a defect.** That tree was built in August 2026 and the
3.2 recipe has carried a `Backdrops3.2` → `Prefs/Presets/Backdrops` rule since
(`core/osinstall/recipes/amigaos-3.2.json:471`), so the current engine may well
place it. What the finding earns is §3.5's verify check, which would have said
so on the day it happened instead of being noticed a month later by someone
reading a hex dump for another reason.

---

## 2. What this round is for

G14 is *"it is **mine**"*: wallpaper, WiFi, prefs and `Startup-Sequence`, each
edited in place. Two thirds shipped earlier — `core/osinstall/startup.rs` for
`S:User-Startup` and `core/amiganet/` for the network half. **Wallpaper and
prefs are the third**, and until this document they had no scope.

The owner made two scope decisions when this design was presented:

1. **The user's own picture is in**, not only the backdrops a release ships.
   That requires a real ILBM encoder — bitplane conversion, quantisation to at
   most eight planes, `CMAP`, and ByteRun1 — because a stock AmigaOS 3.2 tree
   has `ilbm.datatype` and nothing else. Placing a PNG and hoping datatypes are
   installed is the project's "confident and wrong" failure class: the screen
   would say the wallpaper was set and the Amiga would show an empty backdrop.
2. **The distribution tree first.** These settings are written into the tree the
   OS Builder builds, where nothing of the user's own is touched and the whole
   thing runs in a tempdir. Applying them to an existing volume, HDF or card is
   a later wave and needs the full mutation pipeline.

---

## 3. The design

### 3.1 `core/amigaprefs/` — the IFF PREF container and its chunks

Four files, no network, no platform calls, `CoreError` on every refusal.

- **`iff.rs`** — parse a `FORM` … `PREF` into a chunk index (`id`, byte range),
  replace one chunk's body, and serialise. It never rewrites a chunk it was not
  asked about, and it never reorders. Bounds are computed with `checked_add` /
  `checked_mul` and validated against the buffer before anything is sliced —
  the same discipline `core/amigaicon` documents, for the same reason: the
  release profile sets `panic = "abort"`, so an out-of-range index kills the
  application.
- **`wbpattern.rs`** — the `PTRN` structure of §1.1, both the picture and the
  pattern form, as a typed `Backdrop { which, placement, precision, dither,
  remap, content }` where `content` is `Picture(AmigaPath)` or
  `Pattern { depth, planes }`.
- **`screenmode.rs`** — the `SCRM` structure of §1.2.
- **`env.rs`** — `Prefs/Env-Archive/<name>`, plain files, **ISO-8859-1 and bare
  LF**, edited in place.

**Edit in place, never regenerate** (§39/§40, the rule `FF.CFG`, `config.txt`
and `cmdline.txt` already follow). A release's `WBPattern.prefs` carries three
`PTRN` chunks; ART rewrites only the ones the user set. The other chunks, the
`PRHD`, and any chunk this module does not recognise pass through
**byte-for-byte**. A user who sets only the root backdrop keeps the release's
drawer and screen settings exactly as they were.

### 3.2 `core/ilbm/` — the encoder

Input is **RGB8 pixels plus width and height**. This module knows nothing about
PNG, JPEG, or files: it is testable with a synthetic three-pixel image and has
no dependencies of its own.

Output is `FORM` … `ILBM` with `BMHD`, `CMAP`, and a **ByteRun1-compressed**
`BODY`, interleaved by row across planes as §1.3 measured. `nPlanes` is the **plane**
count, 1..=8, so the palette it implies holds 2..=256 colours; masking is `0`
(`mskNone`), because a backdrop has no transparency. Compression is on because the file measured
in §1.3 uses it and because a 640×480×8 backdrop is 300 KB uncompressed on a
volume where that matters.

### 3.3 `core/picture/` — decode and quantise

Reads a PNG or JPEG into RGB8 and reduces it to a palette of at most 256
entries by median cut, then hands `core/ilbm` the indexed pixels and the
palette. Scaling to the target screen size happens here too.

**A dependency decision, made deliberately.** This adds `png` and
`jpeg-decoder` to `core/`. Both are pure Rust, read-only, permissively
licensed, and make no platform call — the same category as `delharc`, `zip` and
`sevenz-rust2`, which `core/` already carries. The core-independence rule is
about **platform** independence, not about a frozen dependency list, and
nothing here makes `core/` less promotable to a standalone crate. Both go into
`THIRD_PARTY_LICENSES.md` in the same commit that adds them to `Cargo.toml`,
as `CLAUDE.md` requires.

### 3.4 Where the picture lands

`Sys:Prefs/Presets/Backdrops/`, because that is where the release puts its own
and where the release's own `WBPattern.prefs` points (§1.1). A user's picture
is encoded to ILBM and written there under a name derived from its own, and the
`PTRN` chunk names it with that Amiga path. A release backdrop already in the
tree is named directly and nothing is written.

`SAFE_CREATE` applies: ART refuses to overwrite a backdrop that is already
there rather than replacing it.

### 3.5 A verify check: every prefs path resolves

New in `core/osinstall/verify.rs`. Every path a `PTRN` chunk names must exist
in the tree, matched **case-insensitively** — AmigaDOS is case-insensitive and
the host filesystem's answer is not the Amiga's. A prefs file pointing at a
file the tree does not have is reported by naming **both** the prefs file and
the missing path, so the message is actionable rather than "verification
failed".

This is what §1.5 found by accident, turned into something that runs.

### 3.6 Writing, backup and metadata

Every write goes through `core/safety`. A prefs file inside a distribution tree
is a config file: `guarded_write` with `BackupPolicy::CONFIG` (five
generations), so a hand-edited one is recoverable and the backup path reaches
the UI. The `.uaem` sidecar beside a rewritten file is kept in step — a
distribution tree's Amiga metadata is part of the file, not decoration.

### 3.7 The screen

A **Görünüm** panel on the OS Builder's volumes step, beside `NetworkPanel`,
which is where the other "make it mine" settings already live. It offers the
backdrops the built tree actually contains plus "choose my own picture", the
placement (tile / centre / scale), and the Workbench screen depth. Both
languages, both themes, `Phrase` for anything `src/lib` builds.

Every value the user sets is remembered (`src/lib/remembered.ts`), and it goes
through `buildSession` only if it can drift from what ART wrote — by the
question `CLAUDE.md` states: *can the value ART wrote and the value the next
step operates on drift apart?* A backdrop choice is consumed by the same step
that asks for it, so it is a remembered key, not a facade field.

---

## 4. Verification

| What | How |
|---|---|
| The ILBM encoder | `scripts/ilbm-oracle-check.py` — ART writes, **ffmpeg reads**, pixels compared. Not in CI (ffmpeg is not on the runner). |
| The prefs container | An `#[ignore]`d hook, `ART_PREFS_DIR`: every `.prefs` under a real tree parsed and re-serialised, **byte equality required**. The 485-icon oracle's shape. |
| The chunk layouts | Unit tests pinned to the **measured** bytes of §1.1–§1.3, not to constructed ones. |
| Quantisation | Synthetic images with known answers; a 2-colour image must produce 1 plane, not 8. |
| Bounds | Truncated chunks, a `DataLength` past the end, a `FORM` size larger than the file — each refused, none read past its buffer. |
| The verify check | A tree whose prefs name a missing file must fail **and name both**. |

Every guard that matters gets its defect put back and is watched to fail —
`CLAUDE.md`'s mutation rule. Survivors are reported rather than hidden, and
each one is asked the 2026-08-24 question first: is the guard weak, or was the
mutation the wrong one for it?

---

## 5. Not in this round, and why

- **Applying to an existing volume, HDF or card.** The owner's decision (§2).
  It needs preview / backup / verify / per-entry reporting, which roughly
  doubles the round.
- **RTG screen modes.** Their eight VideoCore mode IDs (`0x50061303` and
  friends) are Picasso96 board IDs ART cannot re-derive and has no RTG driver
  in the tree to use. `screenmode.rs` handles standard Amiga display IDs only.
  This is the same judgement that made ART drop a 186-row hash table it could
  not verify.
- **An ILBM *reader*.** Nothing this round needs one, and ffmpeg's lack of an
  encoder means the oracle could not check it in that direction anyway.
- **The `.info` tooltype writer**, and everything else in the icon round. It
  belongs to intake round 2, where it has many consumers; building it here
  would repeat the WHDLoad round's lesson that green tests do not prove a
  feature is reachable.
- **HAM and EHB.** A backdrop does not need them.

---

## 6. Known risks

- **Quantisation quality is a judgement, not a test.** A median-cut result can
  be correct and ugly. The oracle proves the file decodes to the pixels ART
  meant; it cannot prove those pixels look good on a 16-colour Workbench. The
  honest mitigation is that the owner looks at one on a real screen, and that
  the round says so rather than claiming otherwise.
- **`ilbm.datatype` on a stock 3.2 tree is assumed, not measured.** It ships
  with the OS and every backdrop the release itself uses is an `.iff`, which is
  strong — but no ART-written ILBM has been opened by an Amiga yet. The first
  task that can settle it should.
- **ffmpeg is not on the CI runner**, so the ILBM oracle runs by hand like the
  hst-imager and 7-Zip ones. Same trade-off, recorded the same way.
