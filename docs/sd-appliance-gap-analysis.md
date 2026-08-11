<!--
Provenance: written 2026-08-11 as `ART-gap-analysis-sd-builder.md` at the
repository's parent folder, alongside the master spec and the stage briefs.
Copied here on 2026-08-12 and **this copy is the one that is maintained** —
it is versioned with the code it plans, and STATUS.md schedules from it. The
original is kept as the record of what was handed over, not as a live document.
-->

# ART Gap Analysis — SD Card Appliance Builder
## What the PiStorm Multiboot vision needs that ART does not have yet
### Source: PISTORM_AMIGA_MULTIBOOT_MASTER_ARCHITECTURE.md vs ART master spec + addenda + implementation as of 2026-08-11

The target user story:

> Insert a 128 GB SD card into the Windows PC. ART builds the complete
> PiStorm multiboot environment on it — AmigaOS 3.2.x + 3.9, WHDLoad games,
> demos, ADF/HDF archives, launcher, recovery — verified. Insert into the
> A500. It boots.

This document lists ONLY what is missing on the ART side. What ART already
covers (WHDLoad install, ADF/LHA import + validation, RDB creation, Gotek
prep, FF.CFG / PiStorm cmdline.txt round-trip, checksums, journalled writes,
Aminet, oplog, job queue) is not repeated — the multiboot project must
consume those, never reimplement them.

Legend: 🟥 blocker (the build story does not exist without it) ·
🟧 major (story works but half-manual) · 🟨 nice-to-have for v1.

---

## G0 — Prior art: existing PiStorm imagers (study before building)

ART is NOT the first mover here. Two active projects already build Emu68 SD
cards, and both must be studied before any SD work starts — for lessons, for
format decisions, and to define what ART does *better* rather than *again*:

| Project | What it is | Learn from it |
|---|---|---|
| **Emu68-Imager** (mja65) | PowerShell/WPF, Windows-only, MIT. Preconfigured AmigaOS 3.1 / 3.2 / 3.2.2.1 / 3.9 installs from user media, writes physical SD, mature docs, admin-rights model | OS-media handling (which ADFs/files it expects and how it validates them), Kickstart requirements per OS, its FAT32/RDB layout choices |
| **emu68hatcher** (rootrootde) | Python, cross-platform, early-stage. Builds **sparse .img first, flashes after** (same architecture G1 recommends), custom partition layouts with **PFS3 + FFS**, Workbench install from original ADFs, optional packages (MUI, WHDLoad, networking, RTG), JSON build configs | Its package model ≈ G7's manifest; its PFS3 provisioning method (contains ARexx scripts — inspect how it drives filesystem creation); its layout JSON ≈ G13 profiles |
| **hdf2emu68** (PiStorm org) | Converts an existing HDF into an Emu68 SD | The minimal FAT32+Emu68 boot layout, distilled |
| **Emu68 official SD docs** (michalsc) | The authoritative reference for what the boot partition must contain | G2's ground truth |

**Positioning:** these tools prove the demand and the feasibility. ART's
differentiators are (a) the safety pipeline — journal, backup, verify, oplog —
none of them have it; (b) one integrated toolkit instead of a one-shot imager:
the same app that builds the card also browses it, repairs it, updates games
on it, and validates it against its manifest; (c) longer term, **native
AmigaOS filesystem engines** (G3 Route B) where the others shell out or
pre-bake — this is the "fark yaratır" line: an imager images once; ART owns
the card's whole lifecycle.

Licence note: Emu68-Imager is MIT — its *approaches* may be studied freely;
its OS-install recipes are still worth reimplementing cleanly against ART's
own engine rather than porting PowerShell.

---

## G1 🟥 Raw physical device access (the SD card itself)

ART today operates on image FILES. Building an SD card means talking to
`\\.\PhysicalDriveN`: enumerating removable devices, identifying them safely
(size, bus type, "is this really the SD and not your backup disk"),
dismounting/locking mounted volumes, raw block read/write, and verify-after-
write. The master spec (§56) explicitly guards against accidental raw-device
writes, and FEATURES.md lists raw device writes as "deliberately absent until
double-confirm UI exists" — that UI and that engine are this gap.

Safest architecture: **build everything into a sparse image file first**
(existing, tested code paths), then a separate, dumb, heavily-guarded
"flash + verify" step streams it to the device. Never build directly on the
device. The flash step needs: device picker with hard evidence shown
(model, serial, size, removable flag), typed confirmation, hash verify-back,
and resume-on-error. Windows-specific → lives outside `core/` behind a trait
(matches CLAUDE.md's core-independence rule).

## G2 🟥 Hybrid SD layout: MBR + FAT32 boot partition + Amiga RDB

A PiStorm/Emu68 card is not a pure Amiga disk. The Pi boots from a **FAT32
partition** (Emu68 kernel, Kickstart ROM images, `config.txt`/`cmdline.txt`),
and the Amiga side lives in the remaining space as RDB. ART has:

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

## G3 🟥 PFS3 write support — the critical path

The multiboot layout wants 16–32 GB content volumes. Classic FFS is
effectively capped around 2 GiB in ART today (Stage R deliberately refuses
larger mounts), so **large volumes require PFS3 — including writing it**,
which ART does not have in any form (read was planned via
`metaneutrons/pfs3`; write was never scheduled).

Four routes — the project must pick consciously:

| Route | What ART does | Trade-off |
|---|---|---|
| **A. Format-on-first-boot** | ART writes RDB + FSHD/LSEG with the user-supplied `pfs3aio` driver, leaves big volumes unformatted; a first-boot script on the Amiga formats them | Ships soonest; but "insert and everything is preloaded" is NOT delivered |
| **B. PFS3 write in ART (native)** | Full preload from Windows, pure Rust — **the long-term differentiator** (user decision 2026-08-11: native AmigaOS filesystems in ART are the goal; this is what sets ART apart from one-shot imagers) | Serious engineering: a second filesystem writer, same rigour as Stage W (journal, oracle, property tests) |
| **C. Many small FFS volumes** | GAMES1:…GAMESn: under 2 GiB each, all FFS | Works today; ugly; a fallback, not a plan |
| **D. Emulator-assisted build (WinUAE headless)** | ART attaches the sparse build image as a hardfile to a **scripted, unattended WinUAE session** (ART already detects, configures and launches WinUAE — §35, built). A generated `startup-sequence` runs the REAL `pfs3aio`'s format and copies content in from a directory hardfile mapped to a staging folder on the PC, then quits the emulator. ART verifies afterwards by reading the volumes back (its own PFS3 read, or hash manifests) | **The authentic filesystem code does the writing — zero reimplementation risk, PFS3 bytes are correct by construction.** Costs: needs user-provided Kickstart (AROS ROM as fallback where pfs3aio tolerates it — verify, don't assume), slower than native, Windows+WinUAE dependency, and the session must be treated as an untrusted step: timeout, log capture, and a verify pass are mandatory. emu68hatcher's ARexx scripts suggest the same family of approach — study them (G0) |

**Recommendation (revised after prior-art review):** v1 preload = **Route D**,
with A as the no-Kickstart fallback. Route B stays on the roadmap as its own
flagship phase — D delivers the user story now, B replaces D's emulator
dependency later and becomes the headline feature no other imager has. C is
the emergency exit only. Bonus: once B exists, **D becomes its perfect
oracle** — the reference implementation writing the fixtures that the native
writer must match. Build D's harness with that future in mind.

## G4 🟥 RDB filesystem embedding (FSHD + LSEG blocks)

For AmigaOS to mount PFS3 volumes at boot, the PFS3 driver must live **inside
the RDB** as FileSysHeader + LoadSeg blocks. ART's RDB writer creates RDSK +
PART and deliberately writes the FileSysHeaderList as "absent" (ART-025). The
gap: segment-splitting a user-supplied filesystem binary into LSEG chains,
checksumming them, and wiring `DosType → SegListBlocks` correctly. Needed for
route A and B alike, and verifiable with amitools (`rdbtool` reads FSHD/LSEG)
— the oracle already exists.

## G5 🟧 OS installation engine (3.2.x / 3.9 system volumes)

Turning user-provided AmigaOS media (3.2 CD/ADFs, 3.9 CD — Phase 2a's ISO
reader arrives exactly on time) into a populated SYSTEM32:/SYSTEM39: volume.
Gap pieces: extracting from install media rather than running the Amiga
Installer, laying out C:/DEVS:/L:/LIBS:/S:/Classes:, generating a clean
`startup-sequence` from templates, per-OS isolation (the multiboot doc's
§3.2), and recording exactly which release was installed into the manifest.
Never bundle OS files; always user-provided media (same rule as ROMs).

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

## G7 🟧 Build manifest + reproducible rebuild

The multiboot doc's manifest idea (§31) has no ART counterpart. Gap: a
`manifest.json` written at build time — schema version, hardware profile,
volume table with sizes/filesystems/checksums, OS releases, ROM profiles,
installed package list — plus a `Validate card against manifest` command, and
ideally `Rebuild from manifest` (point ART at the manifest + your content
folders → identical card). This is also the natural **shared-schema
contract** between ART and any future Amiga-side tooling: define it once, in
one repo, versioned.

## G8 🟧 Whole-card validation ("appliance health check")

ART validates single images. The card needs a composite check: MBR sane,
FAT32 boot files present and matching the chosen Emu68/Kickstart profile,
RDB consistent, every partition's filesystem mounts, SYSTEM volumes contain a
bootable startup, manifest checksums match. Output = the multiboot doc's
First Boot Acceptance checklist (§50), generated as a report the user can
walk through at the real machine. Builds almost entirely on existing
validators — the gap is the orchestration and the report.

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

## G11 🟨 Content layout policy ("what goes where")

The classifier exists (detect + WHDLoad analysis); what's missing is the
**policy layer** mapping classified content to the card layout (Games/ vs
Demos/ vs ADF/Coverdisks/…, the multiboot doc §15/§24), with the usual
preview-before-apply plan. Small, but it is what makes "drop 400 files,
get an organised card" real.

## G12 🟨 Card-level backup/restore

Full-image backup of a 128 GB card needs streaming + compression + verify
(existing hashing/jobs help; the streaming compressor is new), and
config-only restore needs G7's manifest. Snapshot Manager (§49) is still
unbuilt and would slot here. Reasonable to defer behind the first bootable
card.

## G13 🟨 Capacity planner / build profiles

The guided wizard: pick a profile (Power Retro / Gamer / Preservationist…),
see the proposed volume table scaled to the actual card size, adjust, go.
Pure UI over G1–G7; last piece to build, first piece the user sees.

---

## Suggested phasing (after Phase 2a + user's screen/hardware checks)

```text
SD-0  Prior art study         : G0 — Emu68-Imager + emu68hatcher teardown;
                                document their layouts, OS-media handling and
                                PFS3 provisioning before designing ours
SD-1  Image-first foundations : G2 (MBR+FAT32) + G4 (FSHD/LSEG) + G7 (manifest)
SD-2  The card exists         : G1 (flash+verify) + G6 (bootpri/recovery) + G8 (validate)
      → milestone: card built from an image boots the real A500 into 3.2
SD-3  Content, preloaded      : G3 Route D (WinUAE-assisted PFS3 format+fill)
                                + G5 (OS install) + G10 (launcher export)
                                + G11 (layout policy)
      → milestone: games preloaded onto PFS3 volumes from Windows
SD-4  The flagship            : G3 Route B — native PFS3 write, own brief;
                                Route D's harness becomes its oracle
SD-5  Comfort                 : G12 (backup) + G13 (wizard)
```

Rule carried over from the review: **when a job can be done on either side,
it is done on the PC.** The Amiga-side of the multiboot doc consumes this
work; it never duplicates it.

The one sentence that keeps this honest: the first milestone is not
"128 GB of preloaded everything" — it is **one SD card, built entirely from
Windows, that boots a real A500 into AmigaOS 3.2 with a recovery volume
beside it.** Everything else stacks on top of that proof.
