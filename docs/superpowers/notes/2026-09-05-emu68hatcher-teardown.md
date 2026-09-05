# Emu68 Hatcher — a teardown, and what ART should take from it

*Written 2026-09-05. **This is history**: it describes that project and this
one on the day it was written. Re-run the commands below rather than
re-trusting the numbers.*

**Why this note exists.** The owner found
<https://github.com/rootrootde/emu68hatcher> and ruled that ART will take
what is worth taking from it, because ART is a master tool suite with real
design behind it and "if we are doing this, let us do it well." This is
therefore not a competitive assessment. It is the *research-before-design*
step that CLAUDE.md puts first, applied to the one project that builds the
same artefact ART's SD-1/SD-2/SD-5 build.

## What was actually checked

Not a summary of a summary. The repository was cloned and read:

```bash
git clone --depth 1 https://github.com/rootrootde/emu68hatcher.git
gh api repos/rootrootde/emu68hatcher            # metadata
gh api repos/rootrootde/emu68hatcher/contents/LICENSE
```

Read in full or in part: `README.md`, `docs/index.md`, `LICENSE`,
`builder/stage_registry.py`, `builder/workflow.py` (structure),
`builder/pipeline/create_image.py`, `builder/pipeline/adf_mapping.py`,
`builder/host/hst_commands.py`, `builder/staging/boingbag.py`,
`builder/staging/icons.py`, `builder/staging/icon_grid.py`,
`builder/staging/scripts/injector.py`, `data/package_schema.py`,
`data/install_media.py`, `data/rom_detection.py`,
`data/reference/adf_rules.yaml`, `data/reference/install_media_hashes.yaml`,
`data/packages/*.yaml` (listing), `data/local_packages/System/S/Hatcher-FirstBoot`.

**Not checked, and it matters:** the application was never run, no card was
built, and nothing here was tested against real hardware. Every claim below
about their behaviour is read off their source, not observed.

## The project, in facts

| | |
|---|---|
| Purpose | *"Build ready-to-run SD cards with pre-configured Workbench installation (+batteries included) for PiStorm-accelerated Amigas."* |
| Stack | Python 3.10+, GUI, macOS + Linux + Windows, signed/notarised installers |
| Licence | **MIT**; portions derived from mja65's **Emu68-Imager** (also MIT) |
| Age | created 2026-04-12, last push 2026-09-05 — actively developed |
| Size | **20 056 lines of Python**, ~170 package definitions, 7.4 MB |
| Reach | 14 stars, 1 fork, own Discord and MkDocs site |
| Disk tooling | `hst-imager` + `hst-amiga` (Henrik Stengaard, MIT), 7-Zip |
| Content | ships no copyrighted Amiga OS; the user supplies ADFs or the 3.9 CD |

For scale, on the same day: ART is **161 545 lines of Rust** (inline
`#[cfg(test)]` tests included) plus **59 316 lines of TypeScript**, over a much
wider scope — ADF studio, Gotek, ISO, VHD, C64, Aminet, the title catalogue.
Line counts measure size, not quality, and the two projects are not trying to
cover the same ground. Where the ground *is* the same — a PiStorm card built
from the user's own media — they are peers.

**Licence direction matters and only points one way.** MIT is compatible with
ART's GPL-3.0-or-later: ART may use their approach, and their data, **with
attribution**. The reverse is not true. Nothing in this note proposes copying
code; what is worth taking is design, and in one case a table that ART should
generate for itself anyway.

## The two pipelines, side by side

Theirs (`builder/stage_registry.py`, verbatim order):

```
VALIDATE → SETUP_WORKSPACE → DOWNLOAD → EXTRACT → CREATE_IMAGE
        → INSTALL_WORKBENCH → INSTALL_PACKAGES → CONFIGURE
        → INSTALL_EXTRAS → FINALIZE → FLASH
```

ART's, for the same job: `core/osinstall` builds a **distribution tree** (a
host folder plus `.uaem` sidecars and a `distribution.json` provenance record),
and `core/preload` then puts that tree onto a card that `core/card/build.rs`
laid out. ART's split — build the tree, then place it — is the better one and
should not be given up: it is what lets the whole OS-install engine run in a
tempdir with no volume, no driver and no external binary, which is why it is
testable at all. Theirs goes straight to the image and needs `hst-imager` for
every step.

Two places where they agree with ART, independently, and that is worth
recording as confirmation rather than as a finding:

- **MBR + `0x76` areas + per-area RDB, FAT32 first at sector 2048.** Same card
  model as `core/mbr.rs`. One difference: they write FAT32 type `0x0B`
  "to match the reference imager"; ART writes `0x0C` because that is what both
  of the owner's real cards carry. ART reads either (`core/mbr.rs:60`). Nothing
  to change — but note ART's byte is measured off real hardware and theirs is
  inherited.
- **Marker-wrapped blocks appended to `S/User-Startup`** rather than
  regenerating it (`package_schema.py::ScriptModification`). The same
  convention `core/osinstall/startup.rs` follows, arrived at separately.

## What ART should take

Ordered by what each is worth, with a recommendation on each. Nothing here is
built yet; this note is the input to a design round, not a plan.

### 1. Identify install media by content hash, not by volume name — TAKE

`data/reference/install_media_hashes.yaml`: **186 MD5 hashes**, each tagged
with the release *and the publisher it came from*:

```
Commodore 25 · Escom 20 · Cloanto 12 · Haage and Partners (3.9) 7
Hyperion (3.2 base) 35 · Hyperion (3.2.2 Update) 28
Hyperion (3.2.2.1 Hotfix Pack) 29 · Hyperion (3.2.3) 30
```

`data/install_media.py::scan_install_media_by_hash` walks the folders the user
points at, hashes every `.adf`/`.iso`/`.lha` under a file cap, and identifies
each one. Filename matching exists only as a fallback for media not in the
table (`adf_mapping.py`, the locale disks).

**This dissolves a problem ART paid for in full.** The layered-release round
found that `extra_media_folders` is a set whose refusal fires on one shared
*volume name*: 61 disks across the owner's 3.2 and 3.2.1 folders carry 60
distinct names, and `DiskDoctor` is the one Hyperion never version-named. The
whole layer mechanism — recipes naming which layer a component's media sits in
— exists to answer "which `DiskDoctor` is this?". **A content hash answers it
without asking the user anything.**

ART already does exactly this one layer over: the Kickstart table is 154 dumps
generated from amitools' Remus database and re-verified against it on every CI
run (`scripts/rom-table-check.py`, ART-104). The same shape applied to install
media is a known, proven pattern in this codebase.

**Take the idea; generate the table.** Copying their YAML would import 186
numbers ART cannot re-derive, which is the opposite of ART-104's discipline.
The table should be produced from the owner's own media and from any published
source that can be re-checked, with a `scripts/media-table-check.py` beside it.

Note this does **not** retire the layer mechanism: layers still say which
*release* a component belongs to and in what order updates apply. It retires
the part where the user has to keep media in separate labelled folders so ART
can tell two disks apart.

### 2. A first-boot wizard on the Amiga side — TAKE

`data/local_packages/System/S/Hatcher-FirstBoot` is an AmigaDOS script
dispatched from `S:Startup-Sequence` after `BindDrivers`. It runs one-shot
scripts out of `S:FirstBoot/`, deletes each as it succeeds, writes
`S:FirstBoot.done`, clears its own `FIRSTTIMEBOOT` env flag early "so a partial
wizard run does not repeat", and reboots when a step asks for one. It knows
where it is running:

```
IF $System EQ "PiStorm"
   C:Echo "Detected: PiStorm on $RpiType"
ELSE
   C:Echo "Detected: UAE emulator"
```

**This is the missing half of ART's own ruling.** The owner's standing decision
is: do not force what cannot be installed host-side — run the package's own
installer under an emulator. A first-boot script is the same idea aimed at a
better target: the machine the card is actually for, with its real Kickstart,
its real chipset and its real hardware, at the moment it first starts.

It is also the honest answer to several things ART currently cannot do
host-side: choosing between a Pi3 and a Pi4 `HDToolBox`, activating an `AUX`
device, patching datatypes, and — this is the one that matters — running an
update's **own** installer, which is precisely what ART-166's BoingBag
`Updater` is.

Design constraints ART would have to keep, all of them from rules already
written down: the script is generated, not regenerated over a user's own
(§39/§40); every step reports its own ending distinctly rather than collapsing
into "did not succeed"; and a step that cannot run says what is missing rather
than failing silently.

### 3. A richer package model — ADAPT

`data/package_schema.py` is a Pydantic model, and several fields are things
ART's recipes cannot currently express:

| Field | What it does | ART today |
|---|---|---|
| `requires` / `recommends` / `conflicts` / `provides` | a real dependency graph with **virtual capability tokens** (`provides: mui`) | `overrides` and exclusive groups only |
| `bundle` | one GUI checkbox toggling a set of packages | nothing |
| `relocate` | move a file the OS install already placed (`Tools/Commodities/ClickToFront` → `WBStartup/`) | `removes` and `File`/`Subtree` copies, no move |
| `stack` on an install rule | patch a `.info`'s `do_StackSize` | **ART has this** — `core/amigaicon`, built 2026-09-05 |
| `xor` on an install rule | XOR-decode a payload byte on install | nothing |
| `menu_entry` | add a Workbench menu launcher (ToolsDaemon) | nothing |
| `scripts` | marker-wrapped block into `S/User-Startup` | **ART has this** — `core/osinstall/startup.rs` |
| `source: aminet \| github \| web \| local` | four fetch kinds, MD5 per download | Aminet mirrors only |
| `emu68_versions` | gate a package on the chosen Emu68 release | nothing |

**Adapt rather than adopt.** `requires`/`provides`/`conflicts` is the piece
worth real design work — it is what turns a package list into a catalogue that
can refuse an impossible selection before anything is written, and ART's
exclusive groups are already half of it. `relocate` is small and obviously
useful. `menu_entry` is a product feature, not plumbing.

**One of these must not be adopted as written.** `source: web` takes a URL out
of a YAML file. ART's `core/sources/mirror.rs` deliberately has no function
anywhere that fetches a caller-supplied URL — a request is always *constructed*
from a configured mirror plus a validated path — and §41.5.7's guarantee is
worthless if a data file can name a host. If ART grows a `github` source it
must be a **configured source kind** with the repository as a validated field,
never a URL.

### 4. Icons that make the result look like a real Workbench — ADAPT

Two things ART has the primitives for and does not do:

- `staging/icons.py::ensure_dirs_for_orphan_drawer_icons` — a folder with no
  `.info` is invisible on Workbench, so every user-visible drawer gets one from
  a bundled template. **ART already knows this fact**: `core/whdload`'s module
  doc records that a drawer copied without its icon "is indistinguishable from
  the install having failed". ART knows it about WHDLoad packs and does not
  apply it to the tree it builds.
- `staging/icon_grid.py` — alphabetical icon grids **byte-patched into the
  `.info` files**, with real numbers behind them: Topaz character width 8, a
  3-pixel emboss for the frame IControl draws, a 520-pixel inner width budget,
  a 420-pixel budget for the root because the SYS: window size lives in the
  volume's `disk.info`, and a 2.0 target aspect ratio taken from iTidy.

ART has `core/amigaicon` and can read and write a `DiskObject` losslessly
(485 real icons round-tripped, 0 failures). Positioning is arithmetic on top of
that. This is polish, not plumbing — but it is the difference between a tree
that boots and a tree that looks like somebody made it.

### 5. Newer releases exist — VERIFY FIRST

Their hash table carries **AmigaOS 3.2.2.1 (Hotfix Pack)** and **3.2.3**, and
their ROM table carries **Kickstart 47.111** as `3.2.2.1`. ART's newest recipe
is 3.2.2, closed the same day this note was written.

**Do not act on this from their table.** ART's own rule is to ask the artefact:
a release is confirmed by the owner's own media and by the tree's own
`Prefs/Env-Archive/Versions/Release`, not by another project's YAML. What this
finding earns is a question for the owner — *do you have 3.2.2.1 or 3.2.3?* —
and, if yes, a round that adds a recipe the same way 3.2.2 was added.

### 6. Cross-platform, signing, and direct flashing — NOT NOW

They ship notarised macOS `.dmg`s, Debian `.deb`s and a Windows installer, with
a signed update manifest, and they flash a card directly with an elevated
helper per platform (`builder/host/_disk_{linux,macos,windows}.py`,
`_elevated_worker_src.py`). ART is Windows-only and unsigned, and has never
flashed a card.

This is a real gap but it is not a feature to copy — it is a decision about
what ART is. The nearer thing is that **ART has never flashed a card**, which is
already work-list item 1 and already the bar for 1.0.

## What ART has and they do not

Recorded so a round that borrows from them does not accidentally give
something up.

- **A native Rust filesystem writer.** ART formats PFS3 through `libpfs3` and
  FFS through its own `core/volume/write`, with `hst-imager` as a named
  fallback for two typed capability gaps. Every disk operation of theirs
  shells out to `hst-imager`.
- **Independent oracles.** amitools both directions, 7-Zip for FAT32 and for
  discs, hst-imager for PFS3 both directions, Microsoft's `Get-VHD`, and the
  Remus database for the ROM table — several of them blocking in CI. Nothing
  equivalent was found in their repository.
- **A Kickstart table of 154 dumps** against their ~19, generated rather than
  hand-listed, verified on every CI run, with Amiga Forever `rom.key` decoding.
  Their docs state plainly that manual ROM selection does not exist yet.
- **The whole collection side.** A title catalogue, `.rp9`, WHDLoad drawers and
  hardfiles, `igame.data`, artwork, launching into WinUAE. They build a card;
  they do not catalogue anything.
- **Everything outside the card**: the ADF studio, Gotek/FlashFloppy, ISO, VHD,
  Commodore 8-bit, the Aminet client.
- **A Turkish user interface.** They ship a Turkish *locale package* for the
  built Amiga; the application itself is English only.

## The BoingBag passwords — the owner's call, and no default

`builder/staging/boingbag.py` carries the ZipCrypto passwords for both AmigaOS
3.9 BoingBag payloads in plain source, attributed to the Emu68-Imager's own
install list, and applies the curated per-item file list from each.

[ART-166](../../ISSUES.md) is ART's oldest open entry and is stuck on exactly
this archive. It was left open deliberately, with the reason recorded:
"circumventing the encryption is not ART's business." The owner's standing
ruling is the same and is stronger — no password-bypassing is written.

**Nothing was taken and nothing is recommended.** That a password is published
in an MIT-licensed repository changes what is *available*, not what ART's
owner decided. This paragraph exists so the option is on the record with its
provenance, and so nobody rediscovers it in six months and quietly acts on it.
If the ruling ever changes, it changes deliberately and in writing.

The alternative that *is* consistent with the existing ruling is finding 2
above: place the wrapper's loose files and let the BoingBag's own `Updater`
run at first boot, on the Amiga, which is the machine it was written for.

## Suggested order, if this becomes work

1. **Media identification by hash** — closes a defect class ART has already
   paid for, and the pattern is proven here by ART-104.
2. **First-boot mechanism** — unlocks ART-166 and a category of things that
   cannot be done host-side at all.
3. **Package model: `requires`/`provides`/`conflicts`, then `relocate`** —
   design work, and the exclusive-group machinery is already half of it.
4. **Drawer icons and grids** — visible quality, low risk, primitives already
   built.
5. **3.2.2.1 / 3.2.3** — only after the owner confirms the media exists.

Each of those is a round with its own spec, and each should re-check this note
against the tree before building against it: this file describes 2026-09-05 and
starts decaying the moment it is committed.
