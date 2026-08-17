# Launcher metadata — what a game is, said by something that knows (SD-2 · G10)

**Date:** 2026-08-17
**Status:** approved (2026-08-17)
**Scope:** a new `core/gameindex/`, one new field-reading path in `core/layout`,
the Collection screen, and two files written into the staging tree
**Gap:** [sd-appliance-gap-analysis.md](../../sd-appliance-gap-analysis.md) G10
— the last gap SD-2 owes

---

## What this document is

The design for G10. It is not a plan; the implementation plan follows.

The gap text asks for "iGame gameslist + screenshots in the expected layout,
and/or AGS menu structure, generated onto the GAMES: volume at build time,
with `metadata.json` per game alongside as the neutral source of truth".

**Three of those four things changed once the real material and the real
formats were read.** This document records what was measured, what it forced,
and what ships instead. The measurements are the argument; they are kept here
rather than summarised away, because every one of them contradicted something
the gap text assumed.

## 1. What the user's collection actually is

Measured 2026-08-17 on this machine. The gap text was written against a guess
at this; the guess was wrong in a way that changes the work.

| Material | Count | Shape |
|---|---:|---|
| `E:\amiga\Amigatolon\WHDload\HDF_Games_WHDLoad_by_Enzo_[#..Z]` | **1697** | `.hdf`, 3.74 GB, avg 2.26 MB |
| `E:\amiga\Amigatolon\WHDload\*.lha` | 2 | classic WHDLoad archives |
| `E:\amiga\Titles\**\*.rp9` | 207 | Cloanto RetroPlatform packages |
| `E:\amiga\Titles\**\*.adf` | 847 | loose floppy images |

The 1697 are **not** `.lha` WHDLoad archives and **not** bare WHDLoad drawers.
Each is a bootable single-game hardfile:

```
A Prehistoric Tale v1.1.hdf   943 616 bytes, 1843 blocks
  signature 44 4F 53 01  =  DOS\1 (FFS), no RDB — a bare single-volume hardfile
  S/startup-sequence  →  "WHDLoad PrehistoricTale.Slave Preload"
  PrehistoricTale.slave · PrehistoricTale.info · Disk.info · ReadMe
```

Three consequences:

1. **ART can already open every one of them.** `core/hdf.rs`'s non-RDB path
   ("plain raw single-filesystem container") is exactly this shape. No new
   format work.
2. **G11 would file them wrongly.** `detect()` calls them `harddisk-image`,
   and `policy::drawer_for` sends that to `HardDisks/`, not `Games/`. There is
   no `ItemKind` for "a hardfile that *is* a game".
3. **A launcher cannot see into them.** iGame scans for `.slave` files on a
   volume; 1697 slaves each sealed inside their own hardfile are invisible to
   it, and mounting 1697 hardfiles on a real Amiga is not a thing anyone does.
   Getting the drawers *out* is therefore a prerequisite the gap text does not
   mention.

## 2. The reading that replaced the guessing

### 2.1 The WHDLoad slave states title, year, publisher and chipset

Source: [WHDLoad autodoc](https://whdload.de/docs/autodoc.html), section
`WHDLoad.Slave/--Overview--`, plus `WHDLoad/Include/whdload.i` v16.8 from
Aminet `dev/misc/WHDLoad_dev.lha` — the flag bit numbers appear only in the
include file, the current structure tail only in the autodoc. **Neither file is
copied into ART**; the format is read and implemented independently, the same
way ART's boot code was written from the published LVO table.

```
 0  ws_Security   STRUCT 4   moveq #-1,d0 / rts   = 70 FF 4E 75  (SLAVE_HEADER macro)
 4  ws_ID         STRUCT 8   "WHDLOADS"
12  ws_Version    UWORD      gates every field below
14  ws_Flags      UWORD      bit 4 = Req68020, bit 5 = ReqAGA
16  ws_BaseMemSize / 20 ws_ExecInstall / 24 ws_GameLoader / 26 ws_CurrentDir / 28 ws_DontCache
30  ws_keydebug · 31 ws_keyexit                      ← v4+
32  ws_ExpMem                                        ← v8+
36  ws_name · 38 ws_copy · 40 ws_info                ← v10+
42  ws_kickname · 44 ws_kicksize · 48 ws_kickcrc     ← v16+
50  ws_config                                        ← v17+
52  ws_MemConfig                                     ← v20+
```

`RPTR` is documented as "a relative (to the start of the structure) 16-bit
pointer". Verified against both real archives:

```
Lotus3.slave     Security 70 FF 4E 75 ✓  v13  Flags 0x1007 = Disk|NoError|EmulTrap|ClearMem
                 ws_name "Lotus 3"   ws_copy "1992 Gremlin"
                 ws_kickname: FIELD ABSENT (v13 < 16)
Moonstone.Slave  Security 70 FF 4E 75 ✓  v16  Flags 0x0002 = NoError
                 ws_name "Moonstone" ws_copy "1991 Mindscape"
                 ws_kickname: offset 0 = none
```

Five rules follow, and four of them are invisible to anyone who finds the
header by counting bytes:

- **The structure base comes from the hunk header, not from searching for
  `WHDLOADS`.** The doc: a slave is a standard AmigaDOS executable of exactly
  one hunk, 100% PC-relative, and "may contain debug/symbol hunks which are
  ignored". A string search can land in a debug hunk or in the game's own data.
  `ws_Security`'s fixed `70 FF 4E 75` is a second, cheap confirmation once the
  base is found.
- **Every field is version-gated.** Reading `ws_kickname` from Lotus3 (v13)
  reads the game's code as an offset. The reader checks `ws_Version` first and
  reports a field as absent rather than reading it.
- **`ws_copy` has a documented shape**: "should start with the year followed by
  the companies holding the copyright. Multiple years or companies should be
  separated with `', '`" — e.g. `1983 Schega, 1989 Bad Dreams`. So year and
  publisher come from a documented convention, not a guess. It says *should*,
  so a string that does not match yields no year rather than a wrong one.
- **`ws_info` is not plain text.** It may contain `$0a` line feeds, and the
  byte `-1` (`0xFF`) means "line feed plus half a font height". Rendering it as
  Latin-1 puts `ÿ` on screen.
- **`ws_Flags` bit 5 is a stated AGA requirement**, bit 4 a stated 68020 one.
  `core/collection.rs` currently *infers* chipset by looking for the substring
  "AGA" in a filename. The flag word is a UWORD with all 16 bits assigned, so a
  16-name table is complete, not a prefix.

### 2.2 `.rp9` states what the bytes cannot

An `.rp9` is a zip holding the media, `rp9-manifest.xml` and `rp9-preview.png`.

```xml
<type>game</type>            <title>Aerial Racers</title>
<year>1996</year>            <entity type="publisher">Insane Software</entity>
<rating>4</rating>           <genre>driving-simulation</genre>
<systemrom>310</systemrom>   <system>a-1200</system>
<floppy priority="1">aerialracers1.adf</floppy>   <!-- disk order -->
<image type="screen-running">rp9-preview.png</image>
```

`core/layout/mod.rs` says "there is no `Demo` and there will not be one —
nothing derivable from the bytes separates a demo from a game". That was right,
and it stays right: `.rp9`'s `<type>demo</type>` is not derived from bytes, it
is **declared by the packager**. §14/§34 forbid acting on an uncertain
classification as fact; they do not forbid recording a statement as a
statement. (Measured: the Demoscene `.rp9`s carry an `af-application.hdf`
rather than ADFs, so `<type>` is the only thing that separates them.)

Reading the XML needs a parser `core/` does not have. **Decision: add
`quick-xml`** — 0.41.0, MIT, not yanked (checked against crates.io on
2026-08-17) — read-only, behind `core/archive`'s existing security gate,
alongside `delharc`/`zip`/`sevenz-rust2`. Hand-parsing namespaced XML from
untrusted input is the failure class `core/security` exists for.
`THIRD_PARTY_LICENSES.md`, `deny.toml` and CLAUDE.md's core-dependency list are
updated in the same commit that adds it to `Cargo.toml`.

### 2.3 The neutral index already exists, and it is called `igame.data`

Source: iGame 2.6.1, `iGame.guide` node `IGDF` and `src/fsfuncs.c:693`
(`getIGameDataInfo`) — GPLv3, so reading the format and implementing against it
is compatible with ART's GPL-3.0-or-later.

The guide's own justification for the file:

> "If `igame.data` files are included in community-maintained collections of
> games and demos the users will be benefited a lot… **Other game launchers
> will be able to use that information as well.**"

That is the neutral per-game index this design set out to invent. It is a plain
`key=value` text file beside the slave:

```
title=Menace      year=1988      by=Psyclapse    chipset=OCS
genre=Shoot 'em up               players=1
exe=              arguments=     lemon=735       hol=2448      pouet=
```

**ART writes this rather than a `metadata.json` of its own.** Inventing a
second neutral format when a documented, community-consumed one exists is the
mistake, not the caution.

The consumer's source gives six rules the guide does not:

| Rule | Evidence | Why it matters |
|---|---|---|
| Every line ≤ 63 bytes | `int lineSize = 64; FGets(fp, line, lineSize)` | A longer line is split; the first fragment is parsed with a truncated value. Some Enzo titles are long. |
| `exe` is ignored if it contains `.slave` | `!strcasestr(value, ".slave")` | `exe` is for non-WHDLoad items only. |
| `year`, `players` must be numeric | `isNumeric(value)` | Otherwise silently dropped. |
| An empty value is skipped | `strlen(value) > 0` | ART may leave a field it cannot know as empty, harmlessly. |
| The whole file is ignored unless the user enables it | `funcs.c:910`, `useIgameDataTitle` | The screen must say so, or ART "wrote it and nothing happened". |
| Field caps 128 / 32 / 16 / 256 | `iGameExtern.h:72-79` | But the 64-byte line read caps a title at ~56 first. The reader wins. |

Two further facts from the same source:

- **`gameslist.csv` is iGame's runtime state**, not an export target: user
  favourites, times played, titles the user edited. ART writing it would be
  overwriting user data, which "config files are user data" already forbids.
  iGame builds it itself by scanning repositories and reading `igame.data`.
  **So there is no iGame exporter to write.**
- **`genres` is a fixed 21-entry list** shipped with the release, quoted in
  full because it is the vocabulary the mapping targets: `Action`, `Adult`,
  `Adventure`, `Bat and ball`, `Beat 'em up`, `Board`, `Cards`, `Demo`,
  `Gambling`, `Maze`, `Misc`, `Pinball`, `Platform`, `Puzzle`, `Quiz`,
  `Racing`, `RPG`, `Shoot 'em up`, `Simulation`, `Sports`, `Strategy`.
  `.rp9`'s vocabulary is different
  (`driving-simulation`, `intro-40k`), so a mapping table is needed, and an
  unmapped genre is written as `Unknown` — iGame's own default — rather than
  invented.

A third, corroborating: iGame's own scanner reads the game's name **from the
slave file**, with "use the parent folder name instead" as a switchable
alternative. `Lotus3HD` and `Moonstone Install` are why that alternative is the
worse one.

## 3. What ships

### 3.1 The module

```
core/gameindex/
  record.rs         GameRecord + schema constant     (the G7 manifest pattern)
  readers/
    slave.rs        a WHDLoad slave binary → stated title/year/publisher/chipset
    rp9.rs          .rp9 → rp9-manifest.xml + preview   (quick-xml)
    tosec.rs        a filename                        ← moved from core/collection.rs
    whdhdf.rs       a bootable WHDLoad hardfile's insides
  scan.rs           a folder → a catalogue, on a ProgressSink
  write.rs          a record → igame.data
  export/ags.rs     AGS menu structure
```

Layering holds: `core/gameindex` calls `core::whdload`, `core::volume`,
`core::archive`, `core::hashing` — all lower — and nothing above. `core/layout`
(G11) and `commands/` call *it*. `core/collection.rs`'s TOSEC parser moves in;
its scanner retires onto `scan.rs`, the way `core/adf/mutate.rs` retired onto
`core/volume/write`. No second scanner is left behind.

### 3.2 The record: a statement is not a guess

```rust
pub enum Provenance { Rp9Manifest, WhdloadSlave, TosecName, DrawerName }
pub struct Fact<T> { pub value: T, pub from: Provenance }

pub struct GameRecord {
    pub schema: u32,
    pub id: String,                     // slug(title) + "-" + sha256(primary)[..8]
    pub title: Fact<String>,
    pub kind: Option<Fact<TitleKind>>,  // Game | Demo — declared only, never inferred
    pub year: Option<Fact<u16>>,
    pub publisher: Option<Fact<String>>,
    pub genre: Option<Fact<String>>,
    pub rating: Option<Fact<u8>>,
    pub kickstart: Option<Fact<KickstartNeed>>,
    pub chipset: Option<Fact<ChipsetRequirement>>,
    pub media: Media,                   // Floppies{ordered} | Hardfile | WhdloadDrawer{slave}
    pub preview: Option<String>,
    pub source: SourceRef,              // file NAME + sha256 — never a path
}
```

The wrapper is not decoration. The same field arrives from two tiers and they
**will** disagree — a filename containing "AGA" against a slave whose `ReqAGA`
bit is clear is a real case in this collection. Which one won has to be
recoverable.

`source` carries no path, for `SourceFacts`' reason: a record travels with the
card, and where someone keeps their downloads is not part of what the card is.
The catalogue holds `CatalogueEntry { path, record }`; the written record does
not.

`id` is content-derived so it survives the file moving. Today's
`core/collection.rs` uses `md5(path)`, which does not.

### 3.3 Two files, two jobs

```
staging/Games/
  Lotus3HD.info
  Lotus3HD/
    Lotus3.slave
    igame.data          ← community format, only its defined fields, line limit respected
  Moonstone Install/
    Moonstone.Slave
    igame.data
  games.json            ← ART's own record at the root: provenance, hashes,
                          required Kickstart, disk order, preview
```

`games.json` at the root follows G5's `distribution.json` precedent. It carries
what `igame.data` has no field for and ART needs: provenance, source hashes,
the Kickstart a title requires, disk order, preview paths.

`gameslist.csv` is not written. See §2.3.

### 3.4 The reading path into a bootable hardfile

`core/layout` gains `ItemKind::WhdloadHardfile { name, slave }` and
`Placement::ExtractWhdload`; `policy::drawer_for` sends it to `policy.games`.
The extraction reads the FFS volume through `core/volume`, applies
`core::whdload::analyse`'s existing rules to the entries it finds, and places
the drawer with **its icon beside it, not inside** — §82's rule.

This walks straight into [ART-106](../../ISSUES.md), which is open and is
narrower than "the icon is forgotten": `collisions_in` walks `item.destination`
only, while applying the item *also* writes `<parent>/<name>.info` beside the
drawer. So the preview reports no collision for a staging tree that already
holds `Games/Turrican.info`, and the apply then silently no-ops the icon
(`if !to.exists()`) — a drawer placed with no icon, which §82 calls
indistinguishable from a failed install. A new placement that writes an icon
the same way inherits the same hole, so **ART-106 is fixed as part of this
wave** rather than reproduced by it: `collisions_in` must be taught to ask
about the icon's destination for every placement that writes one.

## 4. Testing

Fixtures are synthetic and generated in a tempdir, as always: `core/volume/write`
builds an FFS hardfile carrying a fabricated slave with a valid `WHDLOADS`
header, so the whole reader is exercised without shipping a byte of anyone's
game.

The hostile cases are derived from the documentation rather than imagined:

- `ws_name` requested from a v9 slave (the field does not exist)
- an RPTR pointing past end-of-file, and one pointing at 0 (= absent)
- a string with no NUL terminator before EOF
- `ws_info` containing `0x0A` and `0xFF`
- a file whose hunk header declares more than one hunk
- an `igame.data` line exceeding 63 bytes
- an `.rp9` whose XML is malformed, deeply nested, or declares a huge entity

Real-material hooks, `#[ignore]`d and env-gated in the established style: the
1697 hardfiles, the 207 `.rp9`s, the 2 `.lha`s.

## 5. Waves

1. **The index.** `GameRecord`, the four readers, the scan job, the Collection
   screen. Runs against 2753 real titles on day one. `core/collection.rs`
   retires.
2. **Extraction and writing.** `ItemKind::WhdloadHardfile` and its placement
   (closing ART-106), then `igame.data` beside each slave and `games.json` at
   the root.
3. **AGS.** The format read off the user's own MultibootOS card —
   `AGS0`…`AGS10`, eleven PFS3 partitions of a real AGS library, which ART can
   now read through `libpfs3`. No format is written before it has been read
   from real material.

## 6. Out of scope, recorded rather than dropped

**`ws_kickname` belongs to G9.** A slave may declare the Kickstart image it
needs, by name (`kick34005.A500`, loaded from `DEVS:Kickstarts/`), size and
CRC16. ART holds a 154-dump Kickstart table verified against amitools' Remus
database on every CI run. A card whose `DEVS:Kickstarts/` lacks the named file
gives a game that will not start — the same class of silent failure G9 exists
to prevent, arriving from the games side. Neither sample here needs one
(Moonstone's offset is 0, Lotus3's field does not exist at v13), which is
exactly the pair of cases a reader must handle. Filed as an issue; not built
here.

**`ws_config`, `ws_MemConfig`** (v17+/v20+) are read past, not read. They
describe splash-window gadgets, which no exporter needs.

**Box art fetched online** is not built. The gap text budgets "an optional
online fetch, off by default" for fields `.rp9` already carries offline and the
slave header states. §60's offline-first rule is satisfied more strongly by not
needing the fetch than by defaulting it off.
