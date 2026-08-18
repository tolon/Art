# Amiga Retro Toolkit (ART)

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
![Platform: Windows 10/11 x64](https://img.shields.io/badge/platform-Windows%2010%2F11%20x64-informational)
![Rust 1.93+](https://img.shields.io/badge/rust-1.93%2B-orange)

**The Swiss Army Knife for Amiga Files**

<https://github.com/tolon/Art>

A professional Windows desktop toolkit for Commodore Amiga users. ART combines
ADF, HDF, LHA, ROM, Gotek, WinUAE and collection management into one coherent,
drag-and-drop-driven application.

> **DROP IT INTO ART.**

![A real Workbench 1.3 Extras disk open beside a Windows drive](docs/assets/files.png)

*An Amiga disk from 1988 on the left — OFS, its own `----rwed` protection bits,
its own dates — and a Windows drive on the right. Copy either way. Everything
starts by dropping something on ART: it works out what a file is from its bytes,
not its name, and offers what can be done with it.*

## What it looks like

**Your library, with covers.** Point ART at the folders your games live in. It
reads each title from whatever *states* it — a WHDLoad slave's own header, an
`.rp9` manifest, the filename last — and marks anything it had to guess. Cover
art is fetched from sources you choose, and nothing reaches the network until
you ask it to.

![The Collection: 2787 titles across two folders, with cover art](docs/assets/collection.png)

*The two `1869` cards in the top row are the point, and they are a real pair
rather than a staged one. Both are AGA, both ask for Kickstart `40068.a1200`,
and both say so because the WHDLoad slave inside them says so. But the third
card's **name** was taken off its filename — somebody had renamed the file —
so ART marks it `~guessed` while its neighbour, named by the slave itself,
carries no mark. A guess and a statement look identical once they are on a
screen, and ART's answer is to never let them.*

**Play.** The Collection's detail panel has a Play button that hands a
catalogued title straight to WinUAE. ART picks the machine from the title's
own chipset, picks a Kickstart from your ROM folder, works out how much extra
memory a WHDLoad title needs on top of the stock machine, shows all of that
on a confirmation screen before anything starts, and **refuses rather than
launches** when no ROM in your folder actually suits the title. A floppy set
— plain `.adf` or `.rp9` — mounts and boots directly; a self-booting WHDLoad
hardfile (most WHDLoad titles in a real collection) mounts the same way, with
an off-by-default switch to make it writable so saves survive; a WHDLoad
*drawer* that needs a separate system image is handed that system read-only,
or, when the system supports it, boots straight into the game in one step.
Two shapes have run for real, from this project's own collection: a
self-booting WHDLoad hardfile (`1000 Miglia`, one of 1697 catalogued the same
way) reached the game, and an `.rp9`-packaged floppy title (`3D Demo`)
launched with its two floppies extracted and mounted in order. That is two
proven shapes, not all of them — see
[What still needs testing](#what-still-needs-testing) for exactly what is
still unverified and how to try it.

*(A screenshot of the Play confirmation screen — machine, Kickstart, memory,
shown before anything starts — would fit here; none exists yet.)*

**A PiStorm card, described in the words its own documentation uses.** Every
control writes a documented Emu68 option or a Raspberry Pi firmware setting —
and tells you which one. Both files are merged into what is already on the card,
never rewritten over the top.

![The PiStorm screen: hardware, card, Kickstart and ready-made settings](docs/assets/pistorm.png)

**A Gotek, including what its little screen will say.** The OLED preview is
live: it renders the same text the hardware will, before you write anything to
the stick.

![The Gotek screen with an OLED simulator and FlashFloppy settings](docs/assets/gotek.png)

**And when you need to see the actual bytes.** A hex and sector inspector that
knows where a volume's boot block, root block and bitmap are, and will jump to
any of them.

![The hex inspector, showing a ROM's first sector](docs/assets/tools.png)

## Which machines

**The whole classic line, not one model.** ART's file work is machine-
independent to begin with — an ADF is an ADF whether it came off an A500 or an
A4000, and FFS/OFS, RDB/HDF, LHA and ISO9660 are formats rather than machines.
Where the machine *does* matter, it is data ART carries rather than a code path
it hard-codes: built-in machine profiles ship for **A1000, A500, A500+, A600,
A2000, A3000, A1200, A4000, CDTV and CD32**, and users add their own (spec
§33). Kickstart identification, WinUAE configuration and the compatibility
check all read those profiles.

Commodore's 8-bit side is in scope too, and built: **C64 disk and tape
images** (`.d64`, `.d71`, `.d81`, `.t64`) open in the same commander and copy
out to a folder, read-only. `.tap`, `.prg` and `.crt` are identified and
described rather than browsed — a TAP is a sampled tape signal with no
directory in it.

## Status

The application builds and runs on Windows 10/11 x64. Working today: DD/HD
floppy images and hard-disk (RDB/HDF) partitions — read, write, create and
validate through one volume driver, including boot code that starts a real
Amiga (verified by booting a disk on an actual A500/A500+ — see below) — with
a Total Commander-style dual pane (browse, multi-select,
batch copy in/out/delete, sort, filter by filename mask, rename, mkdir,
attributes) over FFS/OFS volumes; **CD images (ISO9660 with Joliet, including
raw 2352-byte tracks in Mode 1 and Mode 2/XA)** and **archives (LHA, ZIP, 7z)**
opened as panes of the same manager, walked into and copied out of — to a
folder or straight into an Amiga volume; LHA WHDLoad detection with several
archives installed to a disk at once; Kickstart ROM identification; machine
profiles for the whole classic line; Gotek/FlashFloppy; PiStorm/Emu68; WinUAE
launching; a background job queue with progress/cancel;
an operation log; Beginner/Power User modes; and the drag-and-drop Workflow
Engine behind "what can I do with this?".

**A collection you can keep.** ART indexes your folders once and remembers:
a library that takes minutes to read is there the moment the screen opens, and
an Update re-reads only what changed. Each title's facts come from whatever
*states* them — a WHDLoad slave's own header, an `.rp9` manifest, a filename
last — and the screen marks anything guessed rather than read. Cover art is
fetched from sources listed in Settings, which you can switch off or point
elsewhere; **nothing reaches the network until you ask it to.** Where a name
could only be taken off a filename, ART proposes a tidier one and you accept it
— it will not rename anything by itself, and where the evidence runs out it
says nothing and leaves you the edit box. Measured against a real 2787-title
library across two folders.

**PiStorm cards, both directions.** ART opens a real one — an MBR with a FAT32
boot partition and one to three Amiga disks inside it, each carrying its own
partition table at a byte offset inside the card — and shows it as the list of
disks the m68k side actually sees, verified against CaffeineOS and MultibootOS.
It can also **build one**: the partition table, a FAT32 boot partition carrying
your own Emu68 release and Kickstart, and a partition table at the start of
every Amiga disk. **PFS3 and FFS volumes are formatted and filled by ART
itself** — no external tool required, though one can be configured as a
fallback for two named gaps. One limit, said here rather than discovered later:
**no card ART built has been flashed or booted.**

**Content-first detection**: what a file *is* comes from its bytes, not its
name, so an `.img` holding a floppy is a floppy and a `.dat` holding an LHA
still opens.

Two directions a multi-selection cannot yet move in: image-to-image (copy
one at a time instead) and, when copying a selection out of an image, as one
atomic operation rather than several running together — see
[docs/ISSUES.md](docs/ISSUES.md) (ART-064, ART-065).

**Application Size** (Ctrl +/-/0, or Settings) scales the whole interface from
70 % to 250 % and is remembered — as is every other choice ART offers. A
right-hand-edge complaint at 130 % ([ART-099](docs/ISSUES.md#open)) did not
reproduce when the running application was measured across seven screens and
three sizes; what was real — content being clipped with no way to scroll to it
— is fixed.

### Verified how, exactly

ART's disk writer has been checked four ways, and the last one is the one that
matters:

1. `cargo test` — ART agrees with itself.
2. `amitools` and 7-Zip — ART agrees with implementations that share no code
   with it, in both directions.
3. **Two disks ART wrote, opened under licensed Kickstart and Workbench in
   WinUAE / Amiga Forever** — one mounted and read back, one booted to a CLI
   prompt.
4. **A real Amiga.** On **2026-08-12**, `test/art-bootable-test.adf` cold-booted
   an **A500 / A500+** running **Kickstart 3.9** — served from a **Gotek** as
   `DF0:` — straight to an AmigaDOS `1>` prompt.

Rung four is what rung three could not be: the boot code is ART's own,
assembled from the published LVO table, and running it on a real **68000**
(the emulated passes were an A1200's 68020 and an A500+ *configuration*) was
an assumption until then.

**What is still not claimed:** a Gotek is not a mechanical drive. Nothing ART
has written has been through a real floppy head onto physical magnetic media.
That rung is listed by name in [`test/README.md`](test/README.md) and is not
being quietly folded into the one above it — claiming hardware ART has not
been tried on is the one thing [docs/FEATURES.md](docs/FEATURES.md) exists to
prevent.

### What still needs testing

The owner has decided the remaining verification of Play happens by the
community, not alone. One WHDLoad hardfile and one `.rp9` floppy title have
run for real (above); the rest of what the Collection catalogues has not.
Each of these is a concrete gap, not a vague "try it and see" — what to run
and what a good or bad result looks like:

1. **A bare `.adf` floppy set (no `.rp9` wrapper).** Only an `.rp9`-packaged
   floppy set has actually launched. Catalogue a folder holding a plain
   multi-disk `.adf` title, open its detail panel and press Play. **Good:**
   WinUAE starts with the disks mounted in the order the panel shows, and
   boots. **Bad:** the wrong disk order, a missing disk, or a refusal that
   should not have happened.
2. **An `.rp9`-packaged *hardfile* title.** 102 of these are in this user's
   collection and none has run. This is the shape [ART-141](docs/ISSUES.md)
   was found and fixed in review, but the fix has never been exercised
   against a real launch. Play one. **Good:** ART extracts the hardfile entry
   named inside the package (never the `.rp9` zip itself) and mounts it.
   **Bad:** WinUAE opens the wrong file, or reports it cannot be mounted.
3. **The WHDLoad *drawer*-plus-system Y1/Y2 path.** Nothing in this
   collection produces this shape — an unpacked WHDLoad pack paired with a
   separate bootable system image — so only its own unit tests have ever
   exercised it. Configure a bootable system image in Settings → Play, then
   Play a drawer-based (not hardfile) WHDLoad title. **Good:** Y1 mounts your
   system read-only and hands control over, or Y2 (where supported) boots
   straight to the game. **Bad:** a bare AmigaDOS CLI prompt instead of the
   game — this exact failure is how [ART-145](docs/ISSUES.md),
   [ART-147](docs/ISSUES.md), [ART-148](docs/ISSUES.md) and
   [ART-149](docs/ISSUES.md) were each found — or your original system image
   being modified, which it must never be.
4. **Any VHD- or RDB-container system image.** [ART-146](docs/ISSUES.md)
   stopped ART forcing bare-image geometry onto every hardfile, VHD and RDB
   containers included; the fix reads the file's own bytes (`conectix`,
   `RDSK`) instead of assuming one shape for all of them, but has **not**
   been retried against a real emulator run — the one real run since
   (`1000 Miglia`) is a bare `DOS\1` image, the branch ART-146 left
   unchanged. Launch a title or a WHDLoad system (via Y1)
   backed by a VHD container (e.g. an AmiKit-style `.hdf`) or a plain RDB
   hardfile. **Good:** WinUAE mounts and reads it normally. **Bad:** "Not a
   DOS disk in unit 0" — the exact error ART-146 was filed against.
5. **Whether a WHDLoad save survives with the writable switch on.** Measured,
   not proven: two titles were played with *allow writes* on, the emulator
   window was closed rather than quit through WHDLoad's own key, and the
   images were read back afterward — same byte counts and timestamps as
   before, so the mount really is read-write (the generated configuration
   says `rw`) but nothing wrote in that session. The likely explanation is
   that a WHDLoad game writes its save or high-score table on a clean exit,
   not while running, and this session never gave it one. Play a title with
   *allow writes* on, exit through WHDLoad's own quit key rather than closing
   the window, then reopen and check for the save. **Good:** the save is
   there. **Bad:** still nothing after a clean exit, which would mean the
   switch does not do what it claims.

Found something? File it the way every other defect in this project is
filed — see [docs/ISSUES.md](docs/ISSUES.md) for the format, and
[CONTRIBUTING.md](CONTRIBUTING.md) for how to open one.

Data safety is enforced in `core/safety`: every write is atomic, and files are
backed up to `.art-backup/` before being replaced (or, for images too large to
hold in memory, journaled block-by-block). Hand-tuned configuration files are
edited in place, never regenerated.

The interface ships in English and Turkish. The language is chosen in
Settings and remembered across restarts. Error messages coming from the Rust
core are still English regardless of the chosen language.

Not yet built: SFS (partitions using it are listed but their contents are not
readable), DMS/ADZ conversion, recovery tools, and writing *into* a CD or an
archive (both are read-only, deliberately and permanently).

Three fields in the Collection — chipset, genre and rating — are usually empty,
and that is a shortage of sources rather than of code. Lemon Amiga refuses
automated requests outright; Hall of Light publishes only web pages, and ART
fetches index files, never pages; OpenRetro has exactly the right data and
documents no way in. ART leaves them blank rather than guessing.

| | |
|---|---|
| Where the project is, what is next | [docs/STATUS.md](docs/STATUS.md) |
| Feature-by-feature state | [docs/FEATURES.md](docs/FEATURES.md) |
| Known defects | [docs/ISSUES.md](docs/ISSUES.md) |
| Phase definitions | [docs/roadmap.md](docs/roadmap.md) |
| Released changes | [CHANGELOG.md](CHANGELOG.md) |

## Requirements

| Tool | Version | Notes |
|------|---------|-------|
| **Rust** | 1.93+ (stable) | MSVC toolchain (`x86_64-pc-windows-msvc`) |
| **MSVC Build Tools** | VS 2022 | "Desktop development with C++" workload |
| **Node.js** | 20+ | for the frontend |
| **pnpm** | 9+ | package manager |
| **WebView2 Runtime** | any | preinstalled on Windows 10/11 |

## Setup

### 1. Rust toolchain (MSVC)

```powershell
# Install via https://rustup.rs, then ensure the MSVC target is default:
rustup default stable-x86_64-pc-windows-msvc
rustc -vV   # should show: host: x86_64-pc-windows-msvc
```

### 2. MSVC C++ Build Tools

Install **Visual Studio 2022** (Community is fine) or **Build Tools for Visual
Studio 2022** with the **"Desktop development with C++"** workload selected.
This provides `cl.exe`/`link.exe` that the MSVC Rust target requires.

### 3. Frontend dependencies

```bash
pnpm install
```

## Development

```bash
# Run the app in dev mode (hot-reload frontend + Rust rebuild)
pnpm tauri dev

# Type-check the frontend
pnpm lint

# Run the frontend unit tests
pnpm test

# Run Rust unit tests
cd src-tauri && cargo test

# Production build (produces .msi + .exe installers)
pnpm tauri build
```

Build output:

```
src-tauri/target/release/bundle/
├── msi/   Amiga Retro Toolkit_0.8.5_x64_en-US.msi
└── nsis/  Amiga Retro Toolkit_0.8.5_x64-setup.exe
```

## Architecture

```
UI (React + TypeScript)
        ↓  Tauri commands
Application Services / Commands
        ↓
Workflow Engine  ←  Detection
        ↓
Amiga Core (Rust, platform-independent)
├── volume (the filesystem driver + writer) · adf · hdf · rdb · mbr · fat32
├── card (an SD card as the list of disks the m68k side sees)
├── osinstall (a distribution tree, built from your own install media)
├── preload (putting that tree onto a card) · gameindex · artwork
├── archive (lha · zip · 7z, read-only) · rom · cbm · binary
├── analysis · compatibility · conversion · validation · hashing
├── security (hostile input) · safety (your data against ART itself) · jobs
        ↓
Platform Services → Windows
```

The Amiga core is **platform-independent Rust** — no Tauri types, no Windows
APIs, and no network. Where it needs something platform-specific it declares a
trait and the implementation lives outside: `MirrorClient` is the one real
instance, and `src-tauri/src/net/` is the only place in ART that opens a
connection. This keeps the core unit-testable and leaves a future CLI shell
open. See [docs/architecture.md](docs/architecture.md).

## License

Copyright (C) 2026 tolon.

**GNU General Public License v3.0 or later** (`GPL-3.0-or-later`). See
[LICENSE](LICENSE).

ART's dependencies are permissively licensed (MIT / Apache-2.0 / Zlib / CDLA)
with **one deliberate exception**: `libpfs3`, the PFS3 implementation ART writes
real PiStorm cards with, is **LGPL-3.0-or-later**. Weak copyleft, compatible
with ART's own licence, and taken in preference to writing a second filesystem
writer from scratch. All of them are listed in
[THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md) and checked on every push by
`cargo deny`. ART itself distributes **no** Amiga ROMs, no AmigaOS files and no
copyrighted software — see [docs/licenses.md](docs/licenses.md).
