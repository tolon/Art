# One button, one card: a complete PiStorm card image from the OS Builder

**Status:** approved section by section by the owner, 2026-09-11 (brainstorming, architectural path).
**Research:** `docs/superpowers/notes/2026-09-11-card-layout-research.md` (measured images, the
established projects' code, real card capacities, PFS3) and
`docs/superpowers/notes/2026-09-11-pistorm-kickstart-and-4gb.md` (Kickstart, the 4 GB line).
**Fixes on the way:** ART-308 (card sized in GiB) and ART-309 (last partition past its area).

## 1. What the owner asked for

> *"Bu kart üzerinde amikit gibi zengin dağıtım oluşturabilmeliyiz. … whdload arşivim var onu 2. disk
> olarak direkt amigaos'a tanıtılmış biçimde karta ekleyebilmeliyim ya da programlarımı otomatik
> kurabilmeliyim kartın içine imaj almadan."* — and: *"sistem arayüzü karışık olmadan oldukça basit
> biçimde … Arayüzü en sade hale getirmeliyiz."*

Four parts were named and ordered by the owner: **A** the operating system on the card in one step,
**B** a WHDLoad collection on its own partition, **C** programs installed onto the card, **D** one
plain screen. **This spec is A + D, with the parts of B the owner pulled into it** — named content
partitions (System, Work, Games, Stuff …) filled by dropping folders, archives and WHDLoad HDFs on
them, and WHDLoad working on the result. **C (program installation) is a later round.**

Why now: on 2026-09-11 the owner believed the OS was on their card. It was not — both Amiga
partitions of `1card.img` read all zeros at their first block; the image and its manifest share one
modification time; the manifest says `"os": []`. Putting the tree onto a card takes three screens
today (OS Builder `install`, then `boot-card`, then `prepare-volumes`). That is the defect this design
removes.

## 2. Decisions (the owner's, in order)

1. A + D first; B and C after. *("Önerini uygula")*
2. The result is an **`.img` file only** — ART does not write a physical card in this round.
3. The flow lives **inside the OS Builder's four tabs**: the Machine tab's destination becomes *card
   image*, the Build tab's one button builds the tree and the card.
4. Partitions **System, Work, and any the user adds** (Games, Stuff, Demos …), each filled by
   **drag and drop**; ART recognises each source.
5. Sizes are **automatic**; the user picks only the card size. *"Hatasız yapmak için gereken
   araştırmayı her zaman yap, hafızana güvenme … örnek projeleri incele."* — hence the research notes.
6. Content handling **approach 1**: transform only what needs it (archives, WHDLoad HDFs) into a
   short-lived staging folder; copy plain folders straight in.
7. A **WHDLoad phase** is part of this round, so games on the card run.

## 3. The screen — Machine tab (section 1, approved)

The destination gains two choices, **Folder** (today) and **Card image**; the choice is remembered.
With *Card image*:

```
Kickstart     [amiga-os-310-a1200.rom ▾]              (as today)
Keymap        [Turkish ▾]                             (as today)
Destination   (•) Card image   ( ) Folder

Card size     [64 GB ▾]   16 · 32 · 64 · 128 — as printed on the card
Image file    [E:\amiga\Kartlar\amiga39.img]          [Choose…]
Emu68         [Emu68-pistorm-classic.zip]             [Choose…]
PFS3 driver   pfs3aio 19.2 — from paketler\pfs3aio.lha ✓

Partitions
  System   the operating system                 1.0 GiB
  Games    112 games · 3 sources                4.9 GB    [Add…] [×]
  Stuff    2 folders                            1.2 GB    [Add…] [×]
  Work     the rest                            52.1 GB
  [+ Add partition]
  Total 60.8 GB — 95 % of a 64 GB card ✓

▸ Advanced   (folded: override a partition's size, Emu68 release line)
```

- **System** and **Work** always exist; *Add partition* offers Games, Stuff, Demos or a typed name
  (a valid AmigaDOS volume name, `check_name`'s rules). Drive names are ART's (SDH0, SDH1, …) in
  partition order.
- A row takes **drops** and **Add…**. Drops go through the **one global drop listener**
  (`Layout.tsx` via `lib/dnd.ts`), which learns a new target — never a second listener.
- Each source is recognised and said in one phrase: folder · archive (`.lha`, `.lzx`, `.zip`, `.7z`)
  · WHDLoad HDF · ADF · not usable (with the reason). A row expands to list its sources with a
  remove button each.
- Each row's size follows its sources; while a size is being measured the row says so, never a
  number it does not have. The total line is always visible; an overflow turns it red and names the
  partition and the GB.
- The PFS3 driver is found in the material folders (`pfs3aio.lha` or a loose `pfs3aio`), its version
  read from its own `$VER:` (`rdb.rs::version_from_ver_string`); missing, the line says what to add.
- Card size, image path, Emu68 archive, partitions and their sources are remembered per release
  (`remembered.ts`); nothing changes unless the user changes it.
- `CardBuilder` and `VolumePreload` stay as advanced screens; removing them is a separate decision.

## 4. Automatic sizing (approved rule)

All figures are sourced in the research note; the table is the rule.

| Part | Rule | Source |
|---|---|---|
| Image total | **95 % of the card's decimal gigabytes** (16 → 15.2 × 10⁹, 32 → 30.4, 64 → 60.8, 128 → 121.6) | emu68hatcher `partition_helpers.py:34-36` (MIT, attributed); real same-label cards up to ~2 % apart (62.53 / 63.86 × 10⁹ for 64 GB); MultibootOS's last partition ends at 121.60 × 10⁹ |
| FAT32 boot | 1.10 GiB (2 299 904 sectors at LBA 2048), unchanged | `core/mbr.rs:214`; CaffeineOS, MultibootOS, both of the owner's cards |
| System | **800 MiB** — `core::card::propose::MEASURED_SYSTEM_MB`, not a second constant | measured off both real cards and deliberately not scaled (CaffeineOS 801 MiB, MultibootOS 800 MiB, 45 % used); the owner's 3.9 tree is 23.5 MB. *Corrected while planning:* the approved table said 1 GiB (Emu68-Imager's default); ART already carries a measured answer to the same question, and two constants for one fact is how two answers start |
| Content partition | `estimate_pfs3(content) × 1.25`, rounded up to whole cylinders, never below PFS3's own 10 MiB minimum (Emu68-Imager `Get-MinimumPartitionSizes.ps1`), at most 101 GiB | MultibootOS game partitions 76–80 % full; pfs3aio normal-mode limit 109 067 239 424 bytes (`blocks.h:106-120`); both imagers cap at 101 GiB |
| Work | the rest; over 101 GiB it is split into Work, Work_1 … | Emu68-Imager, emu68hatcher |
| Area end | the last partition ends inside its `0x76` area | ART-309 |

**`estimate_pfs3`** — for a set of host files: every file rounded up to whole 512-byte blocks (an empty
file still one), plus directory blocks and anodes **as `libpfs3` 0.1.3's writer actually spends them**
(corrected in round 1's task review, 2026-09-11: an entry is `18 + name + 1 + 2` bytes padded to even,
never split across a 1 KB directory block, and each extra directory block costs an anode —
`writer.rs:1115, 1166-1189`; the "17 bytes + name" first written here was pfs3aio's struct, not what
ART's writer lays down, and a reviewer's measurement showed 16 000 files in 10 directories refused at the
estimate), plus — below `MAXSMALLDISK` (10 241 440 blocks, ~4.88 GiB) — `libpfs3`'s small-mode ceiling
of one anode index block (~21 252 anodes, `writer.rs:1014-1024`), past which the estimate sizes the
partition up into SUPERINDEX mode rather than certify content the writer refuses; plus the format's reserved
area (`CalcNumReserved`, 0.75–2.5 %), plus pfs3aio's **always-free 5 %** (`format.c:466`). The formula
comes from pfs3aio's code and **is not trusted until it is measured** against ART's own `libpfs3`
(§8.1).

**The 5 % stays in the estimate even though ART's writer does not enforce it** (corrected while
planning, 2026-09-11). `libpfs3` 0.1.3 writes `alwaysfree = data_blocks / 20` into the rootblock
(`format.rs:181`) but its writer never reads the field (`writer.rs` has no reference to it), so ART
could fill a partition to the last block. The handler that runs the card afterwards is the Amiga's
own pfs3aio, which holds that twentieth back on every write (`allocation.c:158`): a partition ART
filled past 95 % would mount and read, and refuse every new file. The estimate follows the handler
that will run the card, not the writer that built it.

A partition's content that does not fit is refused before anything is written, naming the partition
and the bytes. An override in *Advanced* is a floor, never a way past the card's total.

## 5. The run — Build tab (section 2, approved; order refined while writing)

With *Card image* the Build tab keeps its one button and its sequenced run. Phases, in order:

| # | Phase | What it does | Built from |
|---|---|---|---|
| 1 | System tree | the tree, into a scratch folder (`scratch::root()`) | today's tree phase, destination changed |
| 2 | Updates | the ticked BoingBags/packages onto it | today's package phase |
| 3 | First boot | first-boot files into it | today's first-boot phase |
| 4 | Content | archives unpacked and WHDLoad drawers taken out of HDFs into staging; every partition measured; the layout fixed and checked against the card | archive gate, `volume::write::copy::extract_from_volume`, `core/card/sizing` (new) |
| 5 | WHDLoad | only if a partition holds a WHDLoad drawer: `C/WHDLoad` and `S/WHDLoad.prefs` into the **system tree**, and the Kickstarts the titles ask for into its `Devs/Kickstarts` | `core/rom/offer.rs` + `place.rs` (ART-130); WHDLoad from the user's material |
| 6 | Card image | FAT32 boot (Emu68, Kickstart, `config.txt`, `cmdline.txt`) and the RDB with `pfs3aio` and every partition, written as `<image>.partial` | `core/card/build.rs::build_card`, ART-308/309 fixed |
| 7 | Partitions | each PFS3 partition formatted and filled: System from the tree, the others from their sources (plain folders directly, transformed ones from staging) | `core/preload` native writer, a partition fed by several sources (new) |
| 8 | Check | `card_check_image`'s health checks and the manifest — now naming the partitions' contents, so `"os"` is no longer empty; then `<image>.partial` is renamed to `<image>` | `core/card/health.rs`, `core/card/manifest.rs` |

**The order changed from section 2 as approved:** WHDLoad moved from after the partitions to before
the card image, because ART-130's placement writes into a host tree (`place.rs:56`, `<tree>/Devs/
Kickstarts`) — the WHDLoad files reach the card as part of System's tree. **Added while writing:** the
image is written under a `.partial` name and gets its real name only after the check passes, so a
half-built image never carries the name of a finished one.

Each phase row states its own ending — succeeded · refused · failed · stopped · not attempted — as
today (`buildRun.ts`). Progress is a real count ("Games: 4 812 of 9 216 files"); phase 4 knows every
total before phase 7 starts. Stop takes effect between files. The system tree and staging are removed
after the run in every ending (`ScratchDir`); a folder that cannot be removed is named. For a tree the
user wants to keep, the destination *Folder* builds it as today.

**WHDLoad's source is stated, never guessed:** ART looks for `C/WHDLoad` in the material — a WHDLoad
user archive, or the `C/WHDLoad` inside a WHDLoad HDF — and takes the highest version by the file's own
`$VER:`, naming the file it came from. Kickstarts come from the ROM folder, matched on the CRC-16 a
slave declares (`RomInfo::whdload_crc16`), never on a filename. A Kickstart not found is named in the
phase's ending, which stays *succeeded*: *"kick34005.A500 not found — you can put it in Devs/Kickstarts
yourself."*

## 6. Endings and refusals (section 3, approved)

**Refused before anything is written**, in the Build button's place, each with the next step:
no `pfs3aio` · no Emu68 archive · content does not fit (partition, bytes, card total) · the image file
exists (`SAFE_CREATE`) · not enough free space on the image's drive (image + staging + tree, both
numbers) · a source unreadable (its name and why).

**Names the native PFS3 writer cannot write** (ART-113, non-ASCII): found in phase 4. With
`hstImagerPath` set, that partition goes through hst-imager (`run_with_fallback`, made reachable) and
the phase says which writer ran; without it the build is refused listing the first 20 names.

**A stop or a failure** removes `<image>.partial` and says so, naming the file removed and, for a
failure, the phase, the file and the reason. A half-built image is not kept: it could be written to a
card by mistake.

Every run is written to the operation log (§53): what was written, what was removed, the ending.

## 7. Components

Core (platform-independent, no Tauri, no process spawn):

- **`core/card/sizing.rs`** (new): card capacity from the label (decimal × 0.95); the layout from the
  partition list and the content estimates; `estimate_pfs3`; the 101 GiB split; typed refusals
  (`Overflow { partition, needed, available }`). Pure functions, unit-tested.
- **`core/card/content.rs`** (new): classify a source; measure it (bytes, files, directories,
  non-ASCII names); prepare it into a staging directory the caller gives (archives through the one
  archive gate; a WHDLoad HDF's drawer and its `.info` through `extract_from_volume`, the boot scaffold
  left behind). Refuses what it cannot place, by name.
- **`core/rdb.rs`**: cylinder count rounded down (ART-309).
- **`core/preload`**: a partition's content is a list of sources (folders), copied in order; the
  existing single-folder form is one element of it.
- **`core/whdload/system.rs`** (new, small): choose WHDLoad from the material by `$VER:`, write it into
  a tree; collect the Kickstarts the drawers' slaves want and hand them to `rom::offer`/`rom::place`.

Commands (thin adapters, jobs with progress and cancellation):

- `card_os_prepare` (phases 4–5) and `card_os_build` (phases 6–8), each a job ending on its own
  event, as `card-build-result` does; free-space query for the refusal lives here (a host question).
- `run_with_fallback` reachable from `card_os_build`.

Frontend:

- `buildSession.ts`: the destination kind and a `CardTarget { sizeGb, image, emu68Archive,
  partitions: [{ name, sources }], overrides }`; remembered keys per release.
- `MachineTab.tsx`: the card destination (section 3); a drop target registered with `lib/dnd.ts`.
- `buildRun.ts` / `useBuildRun.ts`: phase kinds `content`, `whdload`, `card`, `partitions`, `check`,
  with phrase keys for every (kind, ending) pair, in both catalogues.
- `CardBuilder.tsx`, `OsBuilder.tsx` (`distroCheckCard`): card sizes decimal (ART-308).

## 8. Testing (section 4, approved)

1. **Sizing** — the 95 % rule for 16/32/64/128; the last partition inside an area whose size is not a
   whole number of cylinders (ART-309, seen red first); "64" is no longer 64 GiB (ART-308); **the
   estimate calibrated against real `libpfs3` formats** of several sizes and two file profiles (many
   tiny files, as the owner's tree with its 521-byte median; few large ones): never below the real use,
   and the margin above it stated as a number; the 5 % reserve measured; overflow refused by name.
2. **Content** — a synthetic WHDLoad HDF written with ART's own FFS writer (boot scaffold + a drawer
   with a `.slave`): only the drawer and its `.info` reach the partition; an archive unpacked with the
   drawer's icon beside it; a folder copied as is; an ADF placed as a file; a non-ASCII name found in
   advance.
3. **End to end** — a small card: System tree, a Games partition with a synthetic WHDLoad title,
   a Stuff partition; read back by ART's RDB and PFS3 readers **and by hst-imager**
   (`scripts/pfs3-oracle-check.py` extended): image size, partition positions, files and sizes, the
   WHDLoad phase's files, the manifest's contents; the `.partial` name gone only on success.
4. **Owner's material** (`#[ignore]`, env-gated): `sonuclar`'s tree, a few HDFs from Enzo's `[A]`,
   a subset of `WHDLoadDemos100.lha` → a small card, listed by hst-imager.
5. **Frontend** — the Machine tab's card mode (rows, total, overflow, remembering), the Build tab's
   phases and five endings each in both languages, the drop target on the one pipeline.
6. **Mutation** — the 95 % rule, the area end, the overflow refusal, the drawer extraction, the
   non-ASCII refusal, the `.partial` removal; survivors disclosed. Full `cargo test --lib` twice, lint,
   clippy, the sweeps.
7. **Owed by the owner** — the image written to a card, booted on the PiStorm, `version full`, and one
   game run.

## 9. Not in this round

Writing a physical card (the owner's decision 2); installing programs (C); a boot menu across
several distributions (MultibootOS's model); removing `CardBuilder` / `VolumePreload`; SFS; FFS content
partitions (native FFS is capped at 2 GiB, `volume/mod.rs:43`); a `.vhd` card destination (preload
cannot fill a dynamic VHD, the survey's finding 1).
