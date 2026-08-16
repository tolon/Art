<!--
Provenance: written 2026-08-11 as `ART-gap-analysis-sd-builder.md` at the
repository's parent folder, alongside the master spec and the stage briefs.
Copied here on 2026-08-12 and **this copy is the one that is maintained** —
it is versioned with the code it plans, and STATUS.md schedules from it. The
original is kept as the record of what was handed over, not as a live document.
-->

# ART Gap Analysis — PiStorm Image Builder
## What the PiStorm Multiboot vision needs that ART does not have yet
### Source: PISTORM_AMIGA_MULTIBOOT_MASTER_ARCHITECTURE.md vs ART master spec + addenda + implementation as of 2026-08-11

> **Scope decision, 2026-08-12: ART builds the image. It does not write the
> card.**
>
> This document was written around "insert the SD card and ART builds it on
> the card". That is no longer the target. **ART produces a `.img` file**, and
> the user flashes it with whichever of the hundred existing imagers they
> already have — Raspberry Pi Imager, balenaEtcher, Rufus, Win32DiskImager.
> Writing raw sectors to a removable device is a solved, commoditised problem
> that ART would be reimplementing for no gain, at the cost of its single
> largest safety surface.
>
> What this removes, and it is the biggest thing in this document:
>
> - **G1 is out of scope entirely** — `\\.\PhysicalDriveN`, device enumeration
>   and identification, dismount/lock, raw block write, verify-back, the
>   typed-confirmation UI, and the whole "is this really the SD card and not
>   your backup disk" problem. It was 🟥 blocker number one and it is now
>   somebody else's, deliberately.
> - **G8 becomes image validation, not card validation** — no device to read
>   back from, and no need for one: the artifact ART hands over is a file it
>   just built and can check in place.
> - **G12 (card backup/restore) largely goes with G1.** Backing up a card
>   means reading a device.
> - **§56's raw-device guard stops being a thing ART has to solve.** It stays
>   in the spec as the reason ART does not do this.
>
> What it does *not* change: everything that makes the image an Amiga —
> partitioning, filesystems, the OS, the content, the multiboot layout. Those
> were always the hard and interesting parts, and every one of them is file
> work, which is what ART's core is built for and what keeps it inside the
> core-independence rule with no Windows device code at all.
>
> The image is built **sparsely**, the same way `create_hdf` already builds a
> multi-gigabyte HDF without a multi-gigabyte allocation (spec §56), so a
> 32 GB target does not need 32 GB of RAM or a 32 GB write of zeroes.

The target user story, restated to match:

> ART builds a complete PiStorm multiboot **image file** on Windows —
> AmigaOS 3.2.x + 3.9, WHDLoad games, demos, ADF/HDF archives, launcher,
> recovery — verified against its own manifest. The user flashes it to an SD
> card with the imager of their choice, puts it in the Amiga, and it boots.

**The machine is a parameter, not an assumption** (decision, 2026-08-12). The
original wording said "the A500", and that is one of the machines PiStorm goes
into, not the only one — ART's machine profiles already span A1000 through
A4000, CDTV and CD32, and this build must not quietly narrow that. What
actually varies by machine is **data the build already has to carry**: which
Kickstart ROM goes on the FAT32 partition (G9's ROM profiles), what the Emu68
config says about the board, which OS release is installed (G5), and what
partition geometry is sensible for the card. None of that is a code path, and
none of it should be hard-coded to one model.

Two things follow, and both are for the user to settle before SD-1 designs a
layout:

- **Which PiStorm variant(s) to target first.** They are different boards for
  different sockets, and the FAT32 boot payload is per-board. ART should read
  the target from a hardware profile rather than assume one; which board the
  developer can actually test on decides what SD-1's milestone is *verified*
  against.
- **Which machine the first image is verified on.** The milestone below names
  "an Amiga" deliberately: the proof is one real machine booting, and which
  one it is gets recorded rather than generalised. That rung is no longer
  hypothetical — a disk ART wrote booted a real A500/A500+ off a Gotek on
  2026-08-12 (`test/README.md`), which is the same shape of proof this build
  will need and the same honesty about what it does and does not show.

This document lists ONLY what is missing on the ART side. What ART already
covers (WHDLoad install, ADF/LHA import + validation, RDB creation, Gotek
prep, FF.CFG / PiStorm cmdline.txt round-trip, checksums, journalled writes,
Aminet, oplog, job queue) is not repeated — the multiboot project must
consume those, never reimplement them.

Legend: 🟥 blocker (the build story does not exist without it) ·
🟧 major (story works but half-manual) · 🟨 nice-to-have for v1.

---

## G0 — Prior art: existing PiStorm imagers ✅ **done, 2026-08-12**

> **The teardown is written: [sd0-prior-art.md](sd0-prior-art.md).** It is SD-0,
> and where it disagrees with this document **it wins** — its final section
> lists exactly what it supersedes here, gap by gap, so the corrections can be
> made deliberately when SD-1 designs from them rather than in a hurry now.
>
> The three findings that change what gets built:
>
> - **The card's shape is now exact.** MBR, FAT32 primary #1, then 1–3 `0x76`
>   primaries — and **the RDB lives at a byte offset inside one of those**, not
>   at offset 0. G4 is therefore bigger than "write FSHD/LSEG": ART's RDB
>   writer has to work at an offset, which is the same shape as ART-043. One
>   coherent fix.
> - **PC-side PFS3 write is already solved and MIT-licensed** — `hst-amiga` /
>   `hst-imager` read, write *and format* PFS3 and FFS, and both existing
>   imagers stand on them. That adds a Route E to G3 that is proven rather
>   than speculative, and gives Route B a PC-side oracle that needs no
>   emulator.
> - **Multiboot mechanism B ships in the field today** on stock Emu68:
>   per-distro `config_{distro}.txt` and an Amiga-side selector that rewrites
>   `CONFIG.TXT` and reboots. ART can generate the whole static side at build
>   time; the selector is later work and must be ART's own code.
>
> One blocker-grade unknown is carried forward: SD-0's own exit test — drive
> `hst-imager` on a scratch image end to end and verify the result with ART's
> readers and WinUAE.

ART is NOT the first mover here. Several active projects already build Emu68
SD cards or ship a finished multiboot distribution, and every one must be
studied before any SD work starts — for lessons, for format decisions, and to
define what ART does *better* rather than *again*:

| Project | What it is | Learn from it |
|---|---|---|
| **MultibootOS** (multibootos.com) | **The closest thing that exists to what ART is being asked to build.** A multi-boot *distribution* for PiStorm Amigas: several complete Amiga environments on one microSD, chosen at boot, so no card swapping and no extender. v2.2 (April 2026). Targets A500 / A600 / A1200 / A2000 with PiStorm, PiStorm16 or PiStorm32-lite, and also runs under WinUAE / FS-UAE. Free to use; **v2.2 requires a free, manually assigned UserID per installation**, which is what unlocks its integrated online update service | What a finished multiboot layout actually looks like, and what its boot menu offers — this is G16's reference implementation. **Also the sharpest positioning question in this document**: MultibootOS is a distribution somebody else assembles and you install; ART is a *builder* that makes the user's own. Those are complements, not competitors, and ART must never repackage or redistribute it |
| **Emu68-Imager** (mja65) | PowerShell/WPF, Windows-only, MIT. Preconfigured AmigaOS 3.1 / 3.2 / 3.2.2.1 / 3.9 installs from user media, writes physical SD, mature docs, admin-rights model | OS-media handling (which ADFs/files it expects and how it validates them), Kickstart requirements per OS, its FAT32/RDB layout choices |
| **emu68hatcher** (rootrootde) | Python, cross-platform, early-stage. Builds **sparse .img first, flashes after** (same architecture G1 recommends), custom partition layouts with **PFS3 + FFS**, Workbench install from original ADFs, optional packages (MUI, WHDLoad, networking, RTG), JSON build configs | Its package model ≈ G7's manifest; its PFS3 provisioning method (contains ARexx scripts — inspect how it drives filesystem creation); its layout JSON ≈ G13 profiles |
| **hdf2emu68** (PiStorm org) | Converts an existing HDF into an Emu68 SD | The minimal FAT32+Emu68 boot layout, distilled |
| **Emu68 official SD docs** (michalsc) | The authoritative reference for what the boot partition must contain | G2's ground truth |

> **Teardown still owed.** `multibootos.com` returns HTTP 403 to an automated
> fetch, so the notes above come from the project's own summaries and from
> amiga-news.de / GenerationAmiga coverage, not from reading the thing. SD-0
> means opening it properly — layout, boot menu mechanics, which OS releases,
> which filesystems, and above all **its terms**: a UserID-gated update service
> is a licensing shape ART has to understand before going anywhere near it.
> The user is assembling a reading list for SD-0; this is the first entry.


**Positioning:** these tools prove the demand and the feasibility. ART's
differentiators are (a) the safety pipeline — journal, backup, verify, oplog —
none of them have it; (b) one integrated toolkit instead of a one-shot imager:
the same app that builds the image also browses it, repairs it, updates games
on it, and validates it against its manifest; (c) **native AmigaOS filesystem
engines** (G3, landed 2026-08-15 — `libpfs3` behind `core/preload/native.rs`)
where the others shell out or pre-bake — this is the "fark yaratır" line: an
imager images once; ART owns the image's whole lifecycle.

Note that ART is now *less* than these tools in exactly one respect, on
purpose: they write the card and ART does not. That is the commodity half.
The half ART keeps is the half that is hard to get right and dangerous to get
wrong, and it is the half that stays useful long after the card is written —
a build ART can re-open, inspect, fix and rebuild is worth more than one it
merely flashed once.

Licence note: Emu68-Imager is MIT — its *approaches* may be studied freely;
its OS-install recipes are still worth reimplementing cleanly against ART's
own engine rather than porting PowerShell.

---

## ~~G1 🟥 Raw physical device access (the SD card itself)~~ — **out of scope, 2026-08-12**

**ART does not write SD cards.** It builds a `.img` and the user flashes it
with an existing imager. See the scope decision at the top of this document.

Kept here rather than deleted because the ID is stable and because the
*reasoning* is worth not relitigating: everything below assumed a "build the
image, then flash it" split, and the decision is simply that ART stops at the
first half. Nothing else in this analysis depended on the second half.

What this ID used to cover, and what ART is therefore **not** building:
`\\.\PhysicalDriveN` access, removable-device enumeration and identification,
dismount/lock of mounted volumes, raw block write, verify-after-write, the
device picker showing model/serial/size/removable, and the typed-confirmation
flow §56 would have demanded around all of it.

The half that survives is the half that was always the point: **build
everything into a sparse image file through the existing, tested paths.** That
was this gap's own recommendation; it is now the whole of it.

## G2 ✅ Hybrid SD layout: MBR + FAT32 boot partition + Amiga RDB

> **Done in the engine, 2026-08-13/14.** ART writes a partition table
> (`core/mbr.rs::plan_card`/`write_mbr`), creates and fills the FAT32 boot
> partition (`core/fat32.rs`, `fatfs`), chooses the payload from the user's own
> Emu68 archive (`core/card/payload.rs`) and lays an RDB at the start of every
> Amiga area (`core/card/build.rs`). Verified against the two real cards, an
> independent FAT reader (7-Zip) and the real `Emu68-pistorm.zip`. What is left
> is a command and a screen — and, the part no test answers, flashing a card and
> booting it. Three things below turned out differently from what this section
> assumed; they are marked where they appear.

A PiStorm/Emu68 card is not a pure Amiga disk. The Pi boots from a **FAT32
partition** (Emu68 kernel, Kickstart ROM images, `config.txt`/`cmdline.txt`),
and the Amiga side lives in the remaining space as RDB. ART had:

- no MBR partition table writer,
- no FAT32 formatter/file writer,
- no notion of a disk that is two worlds at once.

This is the single most PiStorm-specific gap. Scope it honestly: **v1 targets
Emu68 only** (bare-metal, the dominant setup). The classic Linux/Musashi
route (SD carries a Linux rootfs) is a completely different build and should
be declared out of scope for v1 in writing — exactly the "do not hard-code a
fork" adapter boundary the multiboot doc demands. FAT32 write can come from a
maintained MIT/Apache crate (e.g. `fatfs`) rather than hand-rolling; licence
check + THIRD_PARTY_LICENSES entry as usual.

## G3 ✅ PFS3 write support — the critical path — **done: Route E 2026-08-13/14, native (Route B) 2026-08-15**

> **Both landed, and Route D — the recommendation this section originally
> made — was never built; it turned out unnecessary.** Route E
> (`hst-imager`, PC-side, MIT) arrived first — SD-0's own teardown found it
> already solved and proven — and needs no Kickstart, so the
> WinUAE-assisted Route D below was dropped rather than built: E already
> delivered the user story D existed to prove. Then, on 2026-08-15,
> **Route B itself landed**: [`libpfs3`](https://github.com/metaneutrons/pfs3)
> v0.1.3 (LGPL-3.0-or-later) is a pure-Rust PFS3 implementation that reads,
> writes and sets AmigaDOS protection bits correctly, measured against a real
> CaffeineOS card rather than only a synthetic fixture (see
> `docs/superpowers/specs/2026-08-15-os-install-design.md`). `core/preload/native.rs`
> wraps it behind the same `VolumeFormatter` trait `hst-imager` already
> implements, launches nothing, and is now the **default** writer —
> `hst-imager` is a named fallback for two specific, typed gaps
> ([ART-113](ISSUES.md)'s non-ASCII AmigaDOS names, [ART-117](ISSUES.md)'s
> foreign-RDB driver embed), not the primary path any more. This is the
> differentiator G0's own positioning section named as the long-term goal:
> ART owns native PFS3 write, not just an imager wrapper.
>
> **What is proven and what is not.** Writing at install scale is proven —
> G5's real 3.2 install put 3061 files onto a native PFS3 volume and
> `hst-imager` read every one back byte-identical. What is *not* proven is
> anything past that: no Amiga has booted a volume `libpfs3` wrote, and no
> card carrying one has been flashed. See G5.
>
> The routes table below is kept as the record of the decision, including the
> one path (C) still true as a fallback and the one (A) nobody built.

The multiboot layout wants 16–32 GB content volumes. Classic FFS is
effectively capped around 2 GiB in ART today (Stage R deliberately refuses
larger mounts), so **large volumes require PFS3 — including writing it**,
which at the time this gap was written ART did not have in any form (read
was planned via `metaneutrons/pfs3`; write was not yet scheduled). Both now
exist — see the status note above.

Four routes — the project must pick consciously:

| Route | What ART does | Trade-off |
|---|---|---|
| **A. Format-on-first-boot** | ART writes RDB + FSHD/LSEG with the user-supplied `pfs3aio` driver, leaves big volumes unformatted; a first-boot script on the Amiga formats them | Ships soonest; but "insert and everything is preloaded" is NOT delivered |
| **B. PFS3 write in ART (native)** | Full preload from Windows, pure Rust — **the long-term differentiator** (user decision 2026-08-11: native AmigaOS filesystems in ART are the goal; this is what sets ART apart from one-shot imagers) | Serious engineering: a second filesystem writer, same rigour as Stage W (journal, oracle, property tests) |
| **C. Many small FFS volumes** | GAMES1:…GAMESn: under 2 GiB each, all FFS | Works today; ugly; a fallback, not a plan |
| **D. Emulator-assisted build (WinUAE headless)** | ART attaches the sparse build image as a hardfile to a **scripted, unattended WinUAE session** (ART already detects, configures and launches WinUAE — §35, built). A generated `startup-sequence` runs the REAL `pfs3aio`'s format and copies content in from a directory hardfile mapped to a staging folder on the PC, then quits the emulator. ART verifies afterwards by reading the volumes back (its own PFS3 read, or hash manifests) | **The authentic filesystem code does the writing — zero reimplementation risk, PFS3 bytes are correct by construction.** Costs: needs user-provided Kickstart (AROS ROM as fallback where pfs3aio tolerates it — verify, don't assume), slower than native, Windows+WinUAE dependency, and the session must be treated as an untrusted step: timeout, log capture, and a verify pass are mandatory. emu68hatcher's ARexx scripts suggest the same family of approach — study them (G0) |

**What actually happened, revised again after both routes shipped:** the
original recommendation was v1 preload = Route D, with A as the no-Kickstart
fallback, and B as its own later flagship phase. Route D was never built —
Route E (`hst-imager`) proved to be the working no-Kickstart path SD-0 found
already solved, so D's WinUAE harness was skipped entirely, and E became both
the shipped fallback tool and, briefly, the reference implementation Route
B's fixtures were checked against. Route B then landed for real
(`libpfs3`, 2026-08-15), sooner than "own flagship phase" implied, because
the crate arrived built rather than needing to be written from scratch. C
remains what it always was: the emergency exit, never exercised because B
and E both worked. A was never built and is no longer needed — B format-on-
build is strictly better than format-on-first-boot once B exists.

## G4 ✅ RDB filesystem embedding (FSHD + LSEG blocks) — *done 2026-08-12*

For AmigaOS to mount PFS3 volumes at boot, the PFS3 driver must live **inside
the RDB** as FileSysHeader + LoadSeg blocks. ART's RDB writer creates RDSK +
PART and deliberately writes the FileSysHeaderList as "absent" (ART-025). The
gap: segment-splitting a user-supplied filesystem binary into LSEG chains,
checksumming them, and wiring `DosType → SegListBlocks` correctly. Needed for
route A and B alike, and verifiable with amitools (`rdbtool` reads FSHD/LSEG)
— the oracle already exists.

**Both halves built.** Reading: `parse_file_systems` walks the FSHD/LSEG chain
and reports each driver's DosType, version and true size, with the bound and
loop-limit rules the rest of `core/rdb.rs` follows;
`partitionsMissingDriver` (`@/lib/rdbDrivers`) turns that into the sentence the
Hard Disk studio shows. Writing: `create_rdb_layout(total, partitions,
file_systems)` lays out FSHD + LSEG per driver inside the reserved area, and
refuses — with the block numbers — a driver that will not fit there rather than
producing an image the first partition would overwrite.
`version_from_ver_string` reads the version out of the binary's own `$VER:`
string, so nobody has to type it.

**Verified against the oracle in both directions**, which is the point: ART
reads `hst-imager`'s RDB and agrees with `rdbtool` on every field, and
`rdbtool` reads an RDB ART built (`PDS3 version=19.2 size=59120`) and extracts
the driver back out SHA-256-identical to the file that went in. Closes
[ART-084](ISSUES.md).

## G5 🟧 OS installation engine (3.2.x / 3.9 system volumes) — **engine built, 2026-08-15/16; not yet booted**

> **Built and run against the user's own real 3.2 media, not a fixture.**
> `core/osinstall/` (`recipe` → `source`/`scan` → `plan` → `apply` → `startup`
> → `verify`, design in
> `docs/superpowers/specs/2026-08-15-os-install-design.md`) turns AmigaOS
> install media into a populated system volume without running the Amiga
> Installer. It does this **without copying whole disks**: a component is a
> named set of paths, not a disk — measured, not assumed —
> `ModulesA1200_3.2.adf` carries 14 commands in `C/` and 13 are older copies
> of commands `Workbench3.2` already has; copying the disk wholesale would
> downgrade 13 commands to install the one new one, `LoadModule`. Recipes are
> data (`core/osinstall/recipes/amigaos-3.2.json`), so a future release adds
> a JSON file, not a code path. The engine's product is a **distribution
> tree** — a host folder plus `.uaem` sidecars plus a `distribution.json`
> manifest — not a volume; putting that tree onto a card goes through
> `core/preload` (G3), which is what makes the manifest also the basis for
> future component add/remove.
>
> Run for real: 36 ADFs, 26 components switched on (the `modules-a1200`
> condition correctly firing off the user's own ROM's stated version), 4030
> files / 330 directories built to a tree, and — for AmigaDOS-ASCII names —
> put onto a native PFS3 volume and read back independently by `hst-imager`
> byte-for-byte. That run also found a real, permanent limitation:
> [ART-113](ISSUES.md) — `libpfs3` 0.1.3 cannot round-trip a non-ASCII
> AmigaDOS name, which excluded about a quarter of that particular tree
> (real Locale content: `español`, `türkçe`, `österreich`) from the *volume*
> only — the distribution tree itself carries every file. ART's own refusal
> ships (✅ ART-113: refused by name, before anything is written) and
> `commands/preload.rs` now falls back to `hst-imager` for exactly that case
> (ART-120), so a non-ASCII name no longer stops a preload; it costs a
> slower write, named as such. Only libpfs3 itself can close the gap for
> real.
>
> **What is left, precisely — not "the gap", three separate things:**
> - **No Amiga has booted anything G5 built.** The tree, the PFS3 volume and
>   the independent `hst-imager` read-back are all real and proven; the
>   WinUAE and real-hardware rungs are untouched by this work.
> - **The OS Builder's install screen is unverified past its own headings**
>   ([ART-118](ISSUES.md)) — the engine below it has real coverage, the
>   screen driving it does not yet.
> - **AmigaOS 3.9 has no recipe** — the `IsoSource` trait has the slot, and
>   it costs one JSON file when it is written; the 3.2.1/3.2.2 updates are
>   the same shape of un-started work.
>
> **G9 (ROM/Kickstart profile pairing) and G10 (launcher metadata export)
> are separate gaps, not sub-items of G5, and remain unbuilt** — see their
> own entries below.

## G6 🟧 Multiboot & recovery configuration

- Set bootable flags and **boot priority** per partition on an existing RDB
  (creation supports flags; editing an existing card's boot order does not
  exist yet — the spec's Partition Manager §18 is still ⏳).
- v1 boot selection = Kickstart's native early-startup menu (both mouse
  buttons) + boot priorities. A custom boot manager is explicitly out of
  scope for v1 — record that decision.
- A RECOVERY: volume built from a template: minimal C: tools, a known-good
  startup, and copies of the manifests. Content is a curated file set —
  no new engine needed, but the template and its tests don't exist yet.

## G7 ✅ Build manifest + reproducible rebuild — **done 2026-08-15**

Built as `core/card/manifest.rs`. Two decisions carry it: the manifest is
**read off the finished card** rather than remembered from the build, so a
card ART wrote wrongly cannot be described rightly; and it lives **beside**
the image, because a manifest carrying the boot partition's checksums inside
that partition cannot be right about itself. `card_verify_manifest` reports
what it did **not** look at — ART writes FAT32 and does not read one — and
`scripts/fat-oracle-check.py` closes that half with 7-Zip.

The multiboot doc's manifest idea (§31) has no ART counterpart. Gap: a
`manifest.json` written at build time — schema version, hardware profile,
volume table with sizes/filesystems/checksums, OS releases, ROM profiles,
installed package list — plus a `Validate card against manifest` command, and
ideally `Rebuild from manifest` (point ART at the manifest + your content
folders → identical card). This is also the natural **shared-schema
contract** between ART and any future Amiga-side tooling: define it once, in
one repo, versioned.

## G8 ✅ Whole-**image** validation ("appliance health check") — **done 2026-08-15**

Built as `core/card/health.rs`, fourteen checks. The design decision is the
**three states**: `pass`, `fail` and `not-checked` are kept apart and the
third never renders as a tick, because a green mark meaning "ART did not
look" is the claim §89 forbids. The verdict says it out loud.

*Was "whole-card". With G1 gone there is no card to read back from, and no
need for one: the artifact is a file ART just built and can check in place —
which is strictly easier and strictly more reliable than reading a device.*

ART validates single images. The built image needs a composite check: MBR
sane, FAT32 boot files present and matching the chosen Emu68/Kickstart
profile, RDB consistent, every partition's filesystem mounts, SYSTEM volumes
contain a bootable startup, manifest checksums match. Output = the multiboot
doc's First Boot Acceptance checklist (§50), generated as a report the user
can walk through at the real machine. Builds almost entirely on existing
validators — the gap is the orchestration and the report.

**This is now the last thing ART does before handing the file over**, so it
carries more weight than it did when a flash-and-verify step came after it: it
is the only check between the build and somebody else's imager.

## G9 🟧 ROM/Kickstart profile pairing

ART identifies ROMs (§32, built). The gap is **profiles**: pairing a ROM with
an OS volume and an Emu68 config (`rom_profile` in the multiboot doc §19–21),
placing the ROM file into the FAT32 partition, and writing the mapping into
Emu68's config. Pure bookkeeping + existing pistorm.rs-style config editing;
no new hard tech.

## G10 🟧 Launcher metadata export (Game Center, without writing one)

Do NOT write an Amiga-side launcher in v1. Gap on the ART side: export the
Collection's game metadata in the formats existing Amiga launchers already
eat — **iGame** gameslist + screenshots in the expected layout, and/or
**AGS** menu structure — generated onto the GAMES: volume at build time.
`metadata.json` per game (multiboot doc §13) is written alongside as the
neutral source of truth (schema lives with G7's contract). Box art /
screenshots: optional online fetch, off by default (§60 offline-first).

## G11 ✅ Content layout policy ("what goes where") — **done 2026-08-15, engine and screen**

The classifier existed (detect + WHDLoad analysis); what was missing was the
**policy layer** mapping classified content to the card layout (Games/ vs
Demos/ vs ADF/Coverdisks/…, the multiboot doc §15/§24), with the usual
preview-before-apply plan. Small, but it is what makes "drop 400 files,
get an organised card" real.

Built as `core/layout/` — `scan.rs` walks a drop and stops at a WHDLoad drawer
rather than descending into it, `policy.rs` carries the drawer table as data
and returns `Result<&str, RefusalReason>` so the drawer and the refusal are one
exhaustive decision, and `apply.rs` builds the staging tree without ever
overwriting or touching the source. Its screen is `/layout`, **a peer of the OS
Builder rather than a page inside it**.

The design decision worth carrying forward: **ART cannot tell a demo from a
game** — nothing derivable from the bytes separates them — so the policy
proposes only what it can justify and the preview is editable, rather than the
rule engine growing cleverer. See [ISSUES.md](ISSUES.md) ART-105…ART-110 for
what its own review found and did not fix.

Still true of it: no staging tree G11 builds has been carried onto a card, and
it has not been driven against real material in `pnpm tauri dev`.

## ~~G12 🟨 Card-level backup/restore~~ — **mostly out of scope with G1**

Backing a card up means reading a device, which ART no longer does. What is
left of this gap is small and file-shaped: the build manifest (G7) already
describes how to **rebuild** an image from its inputs, which is a better
answer than a 32 GB blob anyway — reproducible, diffable, and a hundredth of
the size.

If a user wants a byte-for-byte copy of a card, the same imager that wrote it
reads it back. Snapshot Manager (§49) remains unbuilt and remains about
*images*, where it always belonged.

## G13 🟨 Capacity planner / build profiles

The guided wizard: pick a profile (Power Retro / Gamer / Preservationist…),
see the proposed volume table scaled to the actual card size, adjust, go.
Pure UI over G1–G7; last piece to build, first piece the user sees.

**Reframed 2026-08-13 by `ART-research-distro-profiles.md`.** A "profile" is no
longer a made-up name for a volume layout — it is a **real AmigaOS
distribution**: CaffeineOS, CoffinOS, AmiKit, or ART's own Baseline built from
the user's licences. That research is the design; this row is its home in the
ladder.

**Half built, 2026-08-13.** The registry exists (`core/distro/`, JSON data with
11 tests over it) and the OS Builder screen renders it: which distribution,
what it requires of you, whether the ROM you picked is from the family its base
wants, whether your card is big enough. Every entry is `available: false` and
says so — because *preparing* the card is blocked on §8.2 of that research:
the layout has to be read off a real download by hand before an adapter is
written. Guessing at it is how a tool comes to write a card that quietly does
not boot.

Two of the research's open questions are closed with evidence in
[sd2-distro-decisions.md](sd2-distro-decisions.md): the **free Aminet
Picasso96 is enough** (the commercial P96 is newer, not required — which is
what makes ART Baseline reproducible at all), and the **HstWB package format**,
whose manifest and content ART can use today but whose `Install` is 26 KB of
Amiga script that only an Amiga can run.

---

# Added 2026-08-12, from the user

Three gaps this analysis did not name, added after the first human session
with the running application. They are what turns "a card that boots" into
"**my** distribution" — the difference the user was pointing at, and none of
them is reachable from G0–G13 as written.

## G14 🟧 Build inputs the user defines, not just content ART copies

G5 installs an OS and G11 decides what goes where. Neither covers the settings
that make a distribution *someone's*, all of which are ordinary files on the
built volumes and none of which ART can currently set:

| Input | Where it lands | Why it is not G5 |
|---|---|---|
| **Wallpaper / Workbench backdrop** | `Prefs/Env-Archive/sys/wbpattern.prefs` + the picture itself | An IFF/PNG the user supplies, plus a prefs file ART has to write in place — the same "never regenerate a hand-tuned config" rule §39/§40 already impose |
| **WiFi credentials** | the Emu68/Linux side of the FAT32 partition (`wpa_supplicant.conf`), or an AmigaOS TCP stack's own config | Secret material. §45.5's `write_pistorm_wifi` already exists **as a design**, with `@form.wifi_psk` and the rule that a literal secret is a validator *rejection*; the plain, non-AI path to the same thing does not |
| Hostname, timezone, keymap, screen mode, overscan | `Prefs/Env-Archive/sys/*`, `devs/keymaps/` | Same shape: small files on a volume ART is already building |
| Startup-Sequence / user-startup additions | `S/` | Must be **edited in place**, never regenerated — this is exactly the FF.CFG / `cmdline.txt` rule (§39, §40) applied to AmigaOS |

The engineering here is small; the discipline is not. Every one of these is a
config file, which means §39/§40's in-place editing rule applies to all of
them, and the WiFi key means `core/security` has a secret to keep out of the
oplog, out of the build manifest (G7) and out of any AI prompt (§45.5).

**Wallpaper is new scope**: it appears in no existing document, including the
master spec. WiFi is not new — it is §45.5's, reachable only through an AI
layer that is not built.

## G15 ✅ Drag and drop as the way a build is fed — **done 2026-08-15**

Built as `core/card/intake.rs`. The existing drop pipeline answers *what can
I do with this file*; this answers the other question, *what does it become
in this card* — and for everything Amiga the honest answer is usually that it
**needs a volume this card has not got**, which is the answer SD-1 owes most
often and the one worth not swallowing.

ART already has exactly one drag & drop pipeline, and it is architectural
rather than a convenience: `analyze_paths` → `WorkflowEngine::plan` → "what
can I do?" (see CLAUDE.md). The distribution builder should be its largest
consumer — drop a folder of WHDLoad archives, a Kickstart ROM, an OS 3.2 ISO,
a wallpaper, a pile of `.lha`s, and have each one detected, placed and
reported against the build it is being added to.

What is missing is not the pipeline but a **drop target that is a build**:
today `plan()` answers "what can I do with this file", and the builder needs
"what does this file become in *this* card". Detection, the workflow
catalogue and the job queue all already exist to hang it on.

## G16 🟧 Multiboot as a first-class build, not a boot-priority field

G6 covers boot priority and a recovery volume. What the user is asking for is
larger: **several complete AmigaOS environments on one card**, chosen at boot
— e.g. 3.1 for compatibility, 3.2 for daily use, a games-only volume, a
recovery volume — each with its own system partition, its own Startup-Sequence
and its own place in the boot menu.

That is G5 (OS install) run more than once, G6's boot priorities used as a
menu rather than a tiebreak, G11's layout policy made per-environment, and
G7's manifest describing all of it so the card can be rebuilt. It changes no
gap below it; it changes the shape of the thing being built, so it belongs
here rather than inside G6.

---

## Suggested phasing (after Phase 2a + user's screen/hardware checks)

**ART never touches physical media.** Every phase below produces or checks a
*file*; the user flashes it with the imager they already have. There is no
"the card exists" phase any more, because a card is not something ART makes.

```text
SD-0  Prior art study         : G0 — Emu68-Imager + emu68hatcher teardown;
                                document their layouts, OS-media handling and
                                PFS3 provisioning before designing ours.
                                Their *flashing* code is now irrelevant to us;
                                their layout and OS-install decisions are not
SD-1  The image has a shape   : G2 (MBR + FAT32 boot partition, in a file)
                                + G4 ✅ (FSHD/LSEG — closed ART-084)
                                + G7 (build manifest)
                                + G15 (a build as a drop target — the drag &
                                  drop pipeline exists; this is what it drops
                                  *into*)
                                + G8 (image validation)
      → milestone: an .img whose MBR, FAT32 payload and RDB all check out,
                   flashed by any imager, boots a real Amiga into a CLI
                   (which machine and which PiStorm board: recorded, not assumed)
SD-2  Content, preloaded      : G3 ✅ (PFS3 write — route E 2026-08-13/14,
                                  native/route B 2026-08-15; route D was
                                  dropped, never built, turned out unneeded)
                                + G5 🟧 (OS install — engine built and run
                                  against real media; no Amiga has booted its
                                  output) + G9 (ROM pairing, owed)
                                + G10 (launcher export, owed) + G11 (layout
                                  policy)
      → milestone: AmigaOS and games already on the volumes, from Windows,
                   with nothing left to do on the Amiga but boot it — not yet
                   reached: G9, G10, and the boot itself
SD-3  It is *mine*            : G14 (wallpaper, WiFi, prefs, Startup-Sequence
                                  — every one edited in place, never
                                  regenerated: §39/§40's rule, applied to
                                  AmigaOS)
                                + G16 (multiboot: several complete environments
                                  and a boot menu, not a priority field)
      → milestone: two OS environments, a recovery volume, the user's own
                   wallpaper and a working network, all chosen in ART
SD-4  The flagship            : native PFS3 write landed in SD-2 instead of
                                  here (G3's own entry) — this slot's next
                                  candidate is unnamed; the boot rungs (an
                                  Amiga, real or emulated, booting anything
                                  SD-2 built) are the honest gate before any
                                  of SD-4 is worth planning
SD-5  Comfort                 : G13 (capacity planner / build profiles)
```

**What moved, and why.** The old SD-2 was "the card exists: G1 flash+verify".
With G1 gone it had nothing left in it, so validation (G8) moved up into SD-1
where it belongs — it is now the last thing that happens before the file is
handed over — and everything after shifted up a number. G12 left the plan with
G1 (see its entry). The old SD-3 split in two: preloading content is a
different problem from configuring a distribution, and the second one is what
the user actually asked for by name.


Rule carried over from the review: **when a job can be done on either side,
it is done on the PC.** The Amiga-side of the multiboot doc consumes this
work; it never duplicates it.

The one sentence that keeps this honest: the first milestone is not
"128 GB of preloaded everything" — it is **one SD card, built entirely from
Windows, that boots a real Amiga into AmigaOS 3.2 with a recovery volume
beside it.** Everything else stacks on top of that proof. Which machine and
which PiStorm board that card was proved on is written down with the result,
because "it boots" is a claim about the hardware it was tried on and nothing
else.
