<!--
Provenance: handed over 2026-08-12 as `ART-research-sd0-prior-art.md` at the
repository's parent folder, alongside the master spec and the stage briefs.
Copied here and **this copy is the one that is maintained**, for the same
reason the SD gap analysis and the commander brief are: a document that lives
outside the repository it plans drifts from it.

This report **is SD-0**. The phase's charter was "study the prior art before
designing ours", and this answers it — including one find (§5, the Stengaard
stack) that changes SD-2's route table rather than merely informing it. What
it does not do is close SD-0's own exit test, which is named in §5 and carried
in §8 as the last blocker-grade unknown.

Where it disagrees with `sd-appliance-gap-analysis.md`, **this file wins** and
the gap analysis is to be corrected — see the "What this supersedes" section
appended at the end.
-->

# SD-0 Research Report — Prior Art Teardown for the SD Appliance Builder
## Layout & OS-install decisions only (flash mechanics out of scope, per SD-0 charter)
## v2 (deepened) · researched 2026-08-12 · feeds SD-1 (image shape) and SD-2 (content)

---

# 1. GROUND TRUTH — how an Emu68 SD card is actually shaped

From Emu68's official SD preparation docs (michalsc):

- **MBR partition table, GPT unsupported.** Layout:
  - Primary #1: **FAT32 boot partition (~200 MB)** — Emu68 kernel files
    (`Emu68-pistorm.zip` contents for classic PiStorm,
    `Emu68-pistorm32lite.zip` for PiStorm32-lite; NOT the raspi build),
    `config.txt` with an `initramfs kick.rom` line mapping the Kickstart
    into fast Pi RAM, plus `cmdline.txt` (see §2).
  - Primary #2..4: **type `0x76` partitions** — each appears to the m68k
    OS as a SEPARATE hard drive unit.
- **The RDB lives INSIDE a `0x76` partition**, not raw on the card.
  AmigaOS reaches it via Emu68's SD driver (`brcm-sdhc.device` on
  Pi3/Zero2, `brcm-emmc.device` on Pi4/CM4); HDToolBox sees each 0x76
  partition as an unknown SCSI unit and writes a normal RDB into it.
- **Unit at address 0 = the whole SD card including FAT32.** Never
  partition/format unit 0. ART must make generating such a layout
  impossible, and the validator (G8) must flag any RDB found at raw
  offset 0 of a card as a foreign/legacy layout, not ART's.

**Design consequences for SD-1 (G2):**
- ART builds: MBR → FAT32 (Emu68 + kick + configs) → 1–3 × 0x76
  partitions, each carrying its own RDB + FSHD/LSEG + volumes.
- Hard prerequisite: ART's RDB writer must support **RDB at a byte
  offset inside a larger image** — the same shape as open issue
  ART-043/084. Fix once, correctly, with tests at both offsets.
- **MBR's 4-primary limit is the real-estate budget**: FAT32 + up to 3
  independent Amiga "disks" per card.

# 2. EMU68 CONFIGURATION SURFACE (feeds G14 + PiStorm Studio)

`cmdline.txt` is a **one-line** file on FAT32, passed into the m68k
device tree. Options ART's config editor should model (from the official
Options page — full list there):

| Group | Options | ART relevance |
|---|---|---|
| SD | `sd.unit0=off|ro|rw`, `sd.verbose`, `sd.low_speed`, `sd.clock` (+ `emmc.*` twins) | **`sd.unit0=ro` is a safety lever**: the whole-card unit exposed read-only to AmigaOS prevents an Amiga-side tool from nuking the MBR/FAT32. ART default: `ro` (or `off`) on built cards, documented |
| Memory | `limit_2g`, `z2_ram_size`, `enable_{c0,c8,d0}_slow`, `move_slow_to_chip` | G13 profile knobs |
| Graphics | `vc4.mem` (VC4 memory size reported to P96) | RTG profile knob |
| ROM | `copy_rom` (256–2048 KB to fast mem) | ROM profile pairing (G9) |
| Floppy | `swap_df0_with_df[1-3]` | Gotek-alongside-internal-drive scenarios |
| Debug | `debug`, `disassemble`, `async_log`, `buptest` | diagnostics preset in Power mode |

**cmdline is ONE line** — ART's existing merge-don't-rewrite discipline
(the ART-004 lesson from PiStorm cmdline.txt) applies verbatim: parse
tokens, edit known keys, preserve unknown tokens and order.

**No native boot menu / config selector exists in Emu68's documented
options.** Consequence for G16: any menu beyond mechanism A (below) is
custom work — nothing to inherit.

# 3. THE MULTIBOOT MECHANISM (G16) — **VERIFIED from the official
MultibootOS 2.2 readme** (user's legally obtained copy, March 2026)

## 3.1 Verified card layout (OSFMount partition table, 128 GB card)

| # | Type | Start sector | Size |
|---|---|---|---|
| 0 | WIN95 FAT32 (LBA) | 2048 | **1.10 GB** |
| 1 | Unknown (`0x76`) | 2 301 952 | **46.0 GB** |
| 2 | Unknown (`0x76`) | 98 770 944 | **66.15 GB** |

Exactly the §1 model: FAT32 + two 0x76 Amiga units. Note the FAT32 is
**1.1 GB, not 200 MB** — because it doubles as the user drop-zone and
per-distro asset store (below). ART should size FAT32 generously too.

## 3.2 Verified boot-select mechanism = per-distro config switching

The FAT32 partition carries **one `CONFIG.TXT` plus a
`config_{distro}.txt` per distribution** (verified file list:
`config_caffeine.txt`, `config_ags2.txt`, `config_amigaos.32.txt`,
`config_amigaos.323.txt`, `config_amikit.txt`, `config_kick13.txt`,
`cmdline.txt`). An **Amiga-side "Boot Selector"** (CRT/RGB output,
up/down + RETURN, 5-second countdown to last choice) rewrites
`CONFIG.TXT` with the chosen distro's config — which maps that distro's
Kickstart via `initramfs` (verified ROMs on FAT32 `ROMS/`:
`A1200.47.111.rom`, `A1200.47.115.rom`, `caffeine_jingle.rom`,
`ks31a1200.rom`) — and the Pi reboots into it. The readme explicitly
warns users to edit `config_{distro}.txt`, "not CONFIG.TXT as this gets
overwritten when you select a new option from the Boot Selector."

**Consequences:** mechanism B is not hypothetical — it ships today on
stock Emu68. It requires the Amiga side to WRITE the FAT32 (unit-0
access + a FAT handler; CaffeineOS mounts it as `EMU68:`). So §2's
`sd.unit0=ro` default is wrong for a multiboot card — B needs `rw`;
the mitigation is G8 validation + FAT32 backups in the manifest, not
lockdown.

## 3.3 Verified OS-install & content patterns worth copying

- **AmigaOS 3.2 install = FAT32 drop-folder + on-Amiga script**: user
  copies the ADFs from their own 3.2 CD into `+AmigaOS32/` on FAT32
  (+ `hotfix3.2.2.1.lha` or `AmigaOS-3.2.3.lha`), picks "AmigaOS 3.2"
  in the selector, and an installer script installs to `ADH1:`,
  configures RTG, wires the ROM, reboots. The selector then auto-offers
  the installed OS. **This is the Imager `Install/` pattern elevated to
  whole-OS level — the strongest possible validation of ART writing
  FAT32 payloads + first-boot scripts (G15/G5).**
- **AmiKit = pre-provisioned EMPTY partitions** (`AK0:`, `AK1:`): the
  card ships the slots, the user pours their licensed content in. Adopt
  for G13 profiles: "licensed-distro slot" = empty formatted partition
  + selector entry + instructions.
- **Custom-image escape hatch**: users may overwrite `ADH1:` with their
  own 3.2 install; keep the provided `startup-sequence` and edit rather
  than replace. Same philosophy as ART's merge-don't-rewrite.
- **Network**: Roadshow (demo) + wifipi/genet; CLI `online`/`offline`,
  `configSMB`/`mountSMB`; WiFi SSID+password prompted on the Amiga.
  Online activation (5-char UserID) + online update (UpdateCheckerGUI +
  selector menu item). ART needs none of the activation machinery —
  that's their distribution control, not a technical requirement.
- FAT32 also carries `USER/WinUAE/` — the card is directly usable
  under emulation (WinUAE opens the physical card / OSFMount'ed img,
  adds the two [PART] disks + FAT32 as removable). ART's G8 validator
  should support exactly this emulation path as its test harness.

## 3.4 Mechanism decision for ART (updated)

| Mechanism | Status |
|---|---|
| **A. Multi-0x76 + early-startup menu** | Still the zero-code day-one option; max 3 distros |
| **B. Config switching via Amiga-side selector** | **PROVEN in the field** (§3.2). ART can generate the whole static side today (config_{distro}.txt set, ROMs, drop-folders); the selector itself must be ART's own implementation (MultibootOS's selector is their software — pattern is free, code is not) |
| **C. Hybrid** | B's selector + A as fallback when selector absent |

**SD-1 verdict (revised):** the card FORMAT is designed for B from day
one (per-distro config files, ROMS/, drop-folders — all written by ART
at build time); the SELECTOR ships in a later slice (own code, simple
AmigaDOS/menu program), with mechanism A as the interim boot chooser.

# 4. EMU68-IMAGER TEARDOWN (mja65) — the OS-install & content recipe

**OS install pipeline:**
- AmigaOS **3.1 / 3.2 / 3.2.2.1 / 3.2.3 from user-provided ADF sets**;
  **3.9 from ISO + Boing Bag LHA archives** — ART's Phase 2a ISO reader
  + LHA engine cover both input types already.
- **Media identified by CONTENT CHECKSUM, not filename.** Adopt
  verbatim: ART ships a hash DB of known OS install media; any filename
  accepted. (The user's own `e:\temp\` archive history is the argument.)
- **Kickstart policy: A1200 ROMs only**, validated against a checksum DB,
  multiple accepted ROM versions per OS release. Adopt: ART's ROM
  identifier (built) + per-OS accepted-kickstart table (G9).

**Bundled content set — the de facto community standard ART's SD-2
should match** (what a PiStorm user expects on a ready card):

| Category | Packages |
|---|---|
| Network | **Roadshow (demo)** TCP/IP ("the only still-maintained Amiga stack"), **wifipi.device** (WiFi), **genet.device** (wired, Pi4/CM4) |
| Graphics | **Picasso96** with preconfigured resolution presets; PeterK's icon.library |
| Desktop | Directory Opus 4, MUI, PeterK icons, language packs |
| Tools | SnoopDOS, HippoPlayer, IBrowse (demo), Jano editor |
| Disk | PFS3 for large Work partitions |

Licence care for ART: Roadshow/IBrowse are DEMO versions and DOpus4's
distribution terms must be verified per package — SD-2's package
manifest needs a licence column, and anything not clearly
redistributable ships as "fetch task" (Aminet engine!) rather than
bundled bytes. **ART's Aminet engine (§41.5, built) is a structural
advantage here: packages can be pulled+verified at build time instead of
redistributed.**

**The `InstallPackages` pattern (adopt for G15/G11):** the Imager lets
users drop extra `.lha` files into an `Install/` folder on FAT32; a
script installs them on first boot. That is a beautiful, dumb, robust
delivery channel — ART should both WRITE that folder at build time
(drag-and-drop → Install/) and generate the first-boot script.

**Post-install personalisation (validates G14 scope):** screen
resolution (HDMI auto/manual), Workbench screenmode RTG vs native,
**WiFi SSID/password pre-seeding**, icon sets, languages. Market
expectation confirmed: wallpaper/WiFi/prefs are baked at build time.
(Exact WiFi credential file path comes from the wifipi.device docs in
Emu68-tools — pin it during SD-2 implementation, don't guess.)

# 5. THE DECISIVE FIND — the Stengaard stack (hst-imager + hst-amiga)

**hst-amiga** (.NET library, MIT): *"Read, write and FORMAT PFS3
partitions"* + *"read, write and format FFS partitions"* + RDB partition
table support. **hst-imager** (console/GUI on top, MIT, 625+ commits,
Win/macOS/Linux): RDB init/modify, MBR support, physical drive
read/write, file-level copy into Amiga filesystems — no emulator.

And the clincher discovered this round: **BOTH existing imagers stand on
it.** Emu68-Imager drives hst-imager for disk work; emu68hatcher's
README credits `hst-imager` + `hst-amiga` for "disk image + RDB tooling"
and PFS3/FFS formatting. hatcher's REXX scripts are NOT filesystem
provisioning — they are on-Amiga helpers in the Emu68-tools family
(EmuControl etc.). So: **PC-side PFS3 write is solved, MIT-licensed, and
battle-tested by two independent shipping tools.**

Consequences for G3 (rewrites the route table's economics):

- **Route E (shell out to hst-imager) is de-risked to "proven"**: two
  projects already ship on it. Rules if adopted: external process,
  structured argv (never a shell string), lives in `commands/` not
  `core/`, output verified by ART's own readers afterwards. .NET
  runtime dependency must be weighed (self-contained hst-imager builds
  exist — verify per-platform size).
- **Route B (native Rust PFS3 write) gains a second reference
  implementation** (hst-amiga's C# alongside pfs3aio's C) AND a PC-side
  oracle: ART-written PFS3 volumes can be cross-verified by hst-imager
  and vice versa, no emulator in the loop. Route D (WinUAE) then drops
  to third-oracle/ceremony status.
- **Recommended revision:** SD-2 preload = Route E (hst-imager child
  process) with Route D as fallback; SD-4 flagship = Route B with
  hst-amiga + pfs3aio as dual oracles. This likely pulls SD-2 in by
  weeks.
- **SD-0 exit test (acceptance for this report):** run hst-imager on a
  scratch image — init RDB, add PFS3 partition, format, `fs copy` a
  tree in → mount in WinUAE → files verified. Document exact command
  set and versions in the SD-1 design doc.
- Community note: hst-imager already has a thread on commodore.gen.tr
  (topic 20969) — familiar ground for the beta crew.

# 6. EMU68HATCHER + HDF2EMU68 (updated)

- **emu68hatcher** (Python, cross-platform, early): **sparse .img first,
  flash after** — independently validates ART's G1 architecture. Custom
  partition layouts (PFS3 + FFS via the Stengaard stack), Workbench
  install from stock ADFs (3.1→3.2.3), packages: MUI, WHDLoad +
  WHDLoadWrapper, Roadshow, wifipi/genet, Picasso96, IBrowse,
  HippoPlayer; per-model bootable Emu68 (pistorm/pistorm16/
  pistorm32-lite); JSON build configs = reproducible builds (≈ G7
  manifest + G13 profiles). Study its JSON schema before writing G7's.
- **hdf2emu68** (PiStorm org): the minimal correct answer to "what must
  FAT32 contain to boot an existing HDF" — smallest reference for G2's
  FAT32 payload.

# 7. DECISIONS THIS RESEARCH SETTLES (into SD-1/SD-2 design)

1. Card shape = MBR + FAT32(~200 MB: Emu68 kernel per PiStorm model +
   `kick.rom` via `initramfs` + `config.txt` + one-line `cmdline.txt`) +
   1–3 × 0x76 partitions each with own RDB. Unit-0 untouchable;
   built cards default `sd.unit0=ro`.
2. RDB-at-offset support in ART's writer is a hard prerequisite
   (ART-043/084 — one coherent fix with G4's FSHD/LSEG).
3. Multiboot day one = mechanism A (multi-0x76 + bootable/priority +
   early-startup menu). B's unit0 tension recorded; B/C deferred.
4. OS install inputs = ADF sets (3.1/3.2.x) + ISO+LHA (3.9); media by
   checksum DB; A1200-ROM-only with per-OS accepted-ROM table.
5. PFS3 provisioning: **Route E (hst-imager) for SD-2**, Route D as
   fallback, Route B as SD-4 flagship with dual oracles (hst-amiga +
   pfs3aio). Subject to the §5 exit test.
6. SD-2 package set mirrors the community standard table in §4, with a
   licence column; non-redistributable items resolved via ART's Aminet
   engine at build time instead of bundling.
7. G14 personalisation scope confirmed: WiFi, screenmode, resolution,
   language/icons, wallpaper — applied by ART's journalled in-place
   volume editing at build time. Plus the FAT32 `Install/` drop-folder +
   first-boot script pattern adopted for extra packages (G15's simplest
   win).
8. `cmdline.txt` modelling enters PiStorm Studio's config editor with
   the §2 option table; merge-don't-rewrite, one-line format.

# 8. OPEN QUESTIONS carried into SD-1 (final)

- §5 exit test result (hst-imager PFS3 end-to-end on this machine) —
  the only blocker-grade unknown left.
- hst-imager self-contained binary size/licensing of .NET runtime per
  platform (Route E packaging).
- 0x76 partition size limits + brcm driver behaviour on 128 GB cards
  (TD64) — the MultibootOS card runs 46 GB + 66 GB units in the field,
  which is strong empirical evidence large units work; still verify
  driver specifics before G13 fixes volume sizes.
- **Optional confirmation pass on the user's MultibootOS .img**
  (read-only, interoperability research only, nothing copied): scan the
  two 0x76 units' first 16 blocks for `RDSK`, note PART DosTypes
  (PFS3/FFS?), FSHD/LSEG presence, bootable flags/priorities — the one
  layer the readme doesn't document. Nice-to-have, no longer blocking.
- wifipi.device credential file path/format — pin from Emu68-tools when
  G14 lands (the readme confirms SSID/password are prompted on-Amiga;
  where they persist is the remaining detail).

Sources: Emu68 SD preparation + Options (michalsc.github.io/Emu68),
Emu68-Imager instructions/FAQ/Amiga utilities (mja65.github.io/
Emu68-Imager), emu68hatcher (github.com/rootrootde/emu68hatcher),
hdf2emu68 (github.com/PiStorm), multibootos.com, hst-imager + hst-amiga
(github.com/henrikstengaard), commodore.gen.tr topic 20969.

---

# What this supersedes in the gap analysis

Recorded here rather than by rewriting `sd-appliance-gap-analysis.md` in the
same breath: that document is the *plan*, this is the *research*, and the plan
gets corrected when SD-1 designs from it — deliberately, item by item, not in a
hurry. Until then this list is the diff.

| Gap | What the analysis said | What this report establishes |
|---|---|---|
| **G2** (MBR + FAT32) | "ART has no MBR writer, no FAT32 writer, no notion of a disk that is two worlds at once" — correct, but shapeless | The shape is now exact: MBR (no GPT), FAT32 primary #1, then **1–3 `0x76` primaries**, each of which the m68k side sees as a *separate hard drive unit*. §1 |
| **G4** (FSHD/LSEG) | An RDB gap | Bigger than that: **the RDB lives at a byte offset inside a `0x76` partition**, not at offset 0 of the image. ART's RDB writer must work at an offset — the same shape as ART-043. One coherent fix, tests at both offsets. §1 |
| **G3** (PFS3 write) | Four routes, D (WinUAE) recommended for preload, B (native) as flagship | **hst-amiga / hst-imager (MIT, .NET) read, write and *format* PFS3 and FFS from the PC**, and *both* existing imagers already stand on them. A new **Route E** (child process) is de-risked to "proven"; Route B gains a PC-side oracle and Route D drops to third. §5 |
| **G16** (multiboot) | Named as new scope, mechanism unknown | Mechanism **B is shipping in the field**: per-distro `config_{distro}.txt` on FAT32, an Amiga-side selector rewriting `CONFIG.TXT` and rebooting. ART can generate the entire static side at build time; the selector itself is later, and must be ART's own code. §3 |
| **G14** (build inputs) | Proposed from the user's ask | Confirmed as market expectation, with the mechanism: FAT32 drop-folders + first-boot scripts, and WiFi/screenmode/resolution/icons/wallpaper all baked at build time. §4 |
| **G7** (manifest) | "reproducible rebuild" | emu68hatcher's JSON build configs are the shape to study first. §6 |
| **G9** (ROM pairing) | ROM profiles | Sharpened: **A1200 ROMs only**, validated against a checksum DB, several accepted versions per OS release. §4 |
| — (new) | — | **Media identified by content checksum, not filename.** Adopt verbatim: any filename accepted, a hash DB decides what it is. That is ART's own content-first detection rule (phase 2a) applied to install media. §4 |
| — (new) | — | **`sd.unit0`**: unit 0 is the whole card, FAT32 included. Never partition it, and G8 must flag any RDB found at raw offset 0 as a foreign layout. `ro` is the safe default — but mechanism B needs `rw`, and that tension is resolved by validation and backups, not by lockdown. §1, §3.2 |

**Licence discipline this adds to SD-2**: the community-standard package set is
full of demo and conditionally-redistributable software (Roadshow, IBrowse,
DOpus4). The package manifest needs a **licence column**, and anything not
clearly redistributable ships as a *fetch task* through ART's existing Aminet
engine rather than as bundled bytes. That engine (§41.5, built) turns a
licensing problem into a build step.

---

# The exit test, run — 2026-08-12

§5 named one blocker-grade unknown and one acceptance condition for this whole
report: drive `hst-imager` end to end on a scratch image and see whether the
PFS3 route is real. **It is.** Run on Windows, on `F:`, against
**hst-imager 1.6.616** (2026-05-26) and **amitools** as the independent
witness.

## The command set, verbatim

Discovered from the tool's own `scripts/create_1gb_vhd_rdb_pfs3.txt`, not
guessed:

```text
hst.imager blank      F:\art-sd0\sd0-test.img 64mb
hst.imager rdb init   F:\art-sd0\sd0-test.img
hst.imager rdb fs import F:\art-sd0\sd0-test.img https://aminet.net/disk/misc/pfs3aio.lha --dos-type PDS3 --name pfs3aio
hst.imager rdb part add  F:\art-sd0\sd0-test.img DH0 PDS3 * --bootable
hst.imager rdb part format F:\art-sd0\sd0-test.img 1 Work
hst.imager fs copy F:\art-sd0\tree "F:\art-sd0\sd0-test.img\rdb\dh0" --recursive --makedir
```

Every step succeeded. The copy reported `1 directory, 2 files, 36 B`.

## Four findings, in the order they matter

### 1. The filesystem must be in the RDB *before* a PFS3 partition can exist

`rdb part add … PDS3` **refused** until `rdb fs import` had run:

```text
[ERR] File system with DOS type 'PDS3' not found in Rigid Disk Block
```

This is the single most useful thing the test produced, because it is an
independent, shipping implementation refusing to do **exactly what ART's New
HDF wizard does today** ([ART-084](../ISSUES.md)): write a PDS3 partition
with no FSHD/LSEG behind it. ART is not being conservative in calling that a
defect; it is being late.

It also **forces the order of the work**. G4 is not "also needed" alongside
G3 — it is the precondition for any PFS3 partition existing at all, and SD-1
has it before SD-2 for a reason that is now demonstrated rather than assumed.

### 2. `pfs3aio` comes from Aminet, and ART already has the engine for that

The tool's own canonical script fetches it from `https://aminet.net/` at build
time rather than bundling it. That is precisely the pattern this report
recommended for every non-redistributable package (§4), and ART's Aminet engine
(§41.5, built and tested) does it **better** than a bare URL fetch: mirror
failover, size check, SHA-256, a content-addressed cache, and an oplog entry.

The driver is user-supplied content either way — `rdb fs add` takes a *path*,
and nothing is bundled with the tool.

### 3. ART reads such an RDB correctly — and is blind to its file systems

A new oracle hook (`read_foreign_rdb_for_oracle_when_asked`, the same idiom
`read_foreign_archive_*` and `read_foreign_c64_*` already use) was pointed at
the image hst-imager built:

```text
rdb_at=Some(0)   checksum_valid=true
geometry cyls=130 heads=16 sectors=63 block_size=512
partitions=1
  DH0 dostype=PDS3 (0x50445303) fs=Pfs3DirectScsi bootable=true cyls=2..129 bytes=66060288
```

Which agrees with `rdbtool` exactly. But `rdbtool` also reports:

```text
FileSystem #0 PDS3/0x50445303 version=19.2 size=59120 seg_list_blk=0x3
```

and **ART has nothing to print there**, because `ParsedRdb` has no notion of a
FileSysHeader at all. So ART today cannot answer "does this RDB carry a
filesystem driver?" — which means G8 cannot validate a built image, and a user
cannot be told why their PFS3 partition will not mount.

**G4 therefore has a reading half as well as a writing one**, and the reading
half is the cheaper of the two and useful on its own: it turns ART-084's
warning from a guess into a fact ART can check.

### 4. Packaging: no .NET runtime needed

The Windows x64 console build is a **single self-contained 137 MB
`hst.imager.exe`**. That closes §8's packaging question for Route E: no runtime
prerequisite, one file, MIT.

## What this settles for SD-1

- **Route E is viable and proven**, with the caveat that 137 MB is a large
  thing to ship beside a Tauri app; a "point ART at your own hst-imager" option
  is worth designing alongside bundling.
- **G4 comes first**, and starts with reading FSHD/LSEG rather than writing it.
- **The Aminet engine is a build-time dependency of the PiStorm builder**, not
  merely a feature that happens to exist.
- What is still **not** verified: none of this has been near an Amiga. The
  image mounts as far as `rdbtool` and ART's reader are concerned; whether
  AmigaOS mounts it is SD-1's milestone and needs the real machine that booted
  `art-bootable-test.adf`.
