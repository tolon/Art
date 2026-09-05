# What a layered AmigaOS release actually is — measured, not recalled

*2026-09-04. Research for [work-list item 3](2026-09-04-work-list.md#3--a-release-update-is-layered-and-art-cannot-say-so).
No design decision is taken here; this file is the measurement the decision has
to survive.*

Everything below was **run** against the owner's own material in
`E:\amiga\Amigatolon\paketler\3.2`, read-only. The scratch copies and the two
scripts live in `E:\amiga\ProjeART\layer-research` and are not committed —
what is committed is what they found. Tools: `xdftool` (amitools), 7-Zip, and
two throwaway Python scripts (`volnames.py`, `census.py`/`ops.py`).

---

## 1. The refusal has one cause, not sixty

`find_media_across` keeps two disks of one volume name and `media_for` answers
`Ambiguous`, which is why a 3.2 + 3.2.1 folder pair is refused today. The work
list said "63 volumes across their 3.2.1 and 3.2.2 folders" and left the
impression of a wide clash. It is not wide.

Measured over `AmigaOs 3.2/ADF` (35 ADFs) and `Update3.2.1/ADFs` (26 ADFs) —
**61 disks, 60 distinct volume names**, so exactly **one** name is claimed
twice:

| Volume name | Where | Size | SHA-256 (first 16) |
|---|---|---|---|
| `DiskDoctor` | `AmigaOs 3.2/ADF/DiskDoctor.adf` | 901 120 | `12fc60124042d8c3` |
| `DiskDoctor` | `Update3.2.1/ADFs/DiskDoctor.adf` | 901 120 | `ab6b5f195f4efa07` |

Every other disk in the update carries its version **in the volume name** —
`Classes3.2.1`, `Locale3.2.1-TR`, `ModulesA1200_3.2.1`, `Update3.2.1`.
`Update3.2.2` (28 ADFs, extracted from `Update3.2.2.lha`) has the same shape
and the same single exception: one more `DiskDoctor`, again 901 120 bytes,
again a different hash.

**So the ambiguity to resolve is one disk per update, and it is the one disk
Hyperion never version-named.** A design that solves the general case is still
right; a design justified by "sixty ambiguous volumes" would be justified by a
number that is not there.

## 2. The order is stated by the release, and the chain is shorter than assumed

Read out of the packages' own `HowToInstall` (revision 0.23, 28.11.2021 for
3.2.1; revision 0.27, 27.02.2023 for 3.2.2):

- 3.2.1 requires *"an Amiga … containing a successful installation of
  **AmigaOS 3.2**"*.
- 3.2.2 requires *"… a successful installation of **AmigaOS 3.2 or 3.2.1**"*.

**3.2.2 is cumulative.** The chain for the owner's material is *base 3.2 → one
update*, not *base → 3.2.1 → 3.2.2*. A design that models an arbitrarily deep
stack is modelling something this release family does not do; two layers is
what the media states.

Each update also ships its own Kickstarts — 47.102 with 3.2.1, **47.111** with
3.2.2 (5 ROMs) — and both `HowToInstall` files *recommend softkicking the
modules rather than replacing the ROM*.

## 3. The update's payload is compressed — and ART already opens it

Files on the update disks are named `IPrefs.Z`, `SetPatch.Z`,
`workbench.library.Z`, `icon.library.Z`. Their first bytes are `1f 9d 8e` —
**Unix `compress`, LZW**, block mode. The Amiga Installer decompresses them
itself: `Install/Install`'s `UNCOMPRESS` procedure (line 1159) drops the `.Z`
and calls `copyfiles … (compression)`.

> **This section said the opposite for about an hour, and the correction is
> left in rather than tidied away.** It claimed ART had no decompressor and
> that a layered install would therefore place `C/IPrefs.Z` on the volume — a
> tree that builds, verifies and cannot boot. That was written from the media
> without asking the tree, which is precisely the failure CLAUDE.md's work-list
> rule describes. **`core/archive/compress.rs` has existed since ART-228**: a
> pure-Rust `.Z` reader written rather than depended on, with the code width
> checked against the header's own maximum, an undefined code refused rather
> than read, and the output capped. It was written for exactly this reason one
> release earlier — AmigaOS 3.2's own Locale content is `.Z` too, 3 263 files
> in a tree ART built.
>
> It is **wired into the install path**, not merely present:
> `plan::expand_rules` marks an entry compressed with
> `compress::is_compressed_name` and strips the suffix from the destination
> with `compress::name_without_suffix` (`plan.rs:921`, and again at `:968` for
> every file walked inside a `Subtree` rule), and `apply` calls
> `compress::decompress` as it writes (`apply.rs:700`).
>
> So `.Z` is not new capability for this round. What the measurement below is
> still worth is the count: it says how much of the update is `.Z` and
> therefore how much of it rides on a path that is already built and tested.

Verified in both directions rather than asserted:

| File | On the disk | Decompressed | Proof it is genuine |
|---|---|---|---|
| `C/IPrefs.Z` | 18 854 | 24 820 | `$VER: IPrefs 47.29 (22.10.2022)` |
| `C/SetPatch.Z` | 17 634 | 22 896 | hunk magic `0000 03f3` |
| `LIBS/workbench.library.Z` | 141 722 | 185 880 | hunk magic `0000 03f3` |

## 4. What a 3.2.2 install actually does — every operation, from the script

`Update3.2.2.adf`'s `Install/Install`, 1 979 lines, read in full. The
operations, in order, with the counts measured off the disks:

**Media → tree (a recipe could express these):**

1. `COPYDISKCONTENT Update3.2.2` — 17 entries, the last of them the disk root,
   walked **non-recursively**, pattern `(#?.Z)` only, `.Z` dropped:
   **39 files** (C 10, Devs 1, L 1,
   Libs 6, Locale/Countries 4, Prefs 2, System 2, Tools 3, Tools/Commodities 3,
   Tools/TextEditFileTypes 4, Utilities 1, WBStartup 2).
2. `Update/Release` → `Prefs/Env-Archive/Versions/Release` — 14 bytes reading
   `Release 3.2.2`. See §6.
3. `Update/Startup-HardDrive` → `S/Startup-Sequence` (renamed).
4. `COPYDISKCONTENT Classes3.2.2` — **31 files** (Classes 3, DataTypes 7,
   Gadgets 17, Images 4).
5. From `DiskDoctor`, **three files only**: `c/DAControl` and `c/DiskDoctor`
   → `C`, `Devs/trackfile.device` → `Devs`. Not the disk.
6. `UPDATELOCALE`, per language the user ticked: `Languages/<lang>.language`
   → `Locale/Languages`; `Catalogs/<lang>/Sys` → `Locale/Catalogs/<lang>/Sys`;
   `Help/<lang>/Sys` with its subdirectories and `.Images` →
   `Locale/Help/<lang>/Sys`; `Support/Fonts` → `Fonts`;
   `Support/Prefs/Presets` → `Prefs/Presets`. For Turkish that is **39 files**
   placed off a disk listing 40 (the 40th is `Disk.info`), including a
   `helvetica_iso-8859-9` font family and `Font-TR.prefs` — the one placed file
   on that disk that is *not* `.Z`.

   **The 17 locale disks do not have one shape, and a rule list written from
   one of them would be wrong about the rest.** Counted:

   | Disk | Placed | Top-level drawers |
   |---|---|---|
   | `-EN` | 23 | `Help` only — English needs no catalogs |
   | `-DE -DK -ES -FR -GR -IT -NL -NO -PT -SE -UK` | 28–43 | `Catalogs`, `Help` |
   | `-PL -TR` | 45, 39 | `Catalogs`, `Help`, `Support` |
   | `-RS` | 239 | `Catalogs`, `Help`, `Languages` |
   | `-CZ` | 281 | `Catalogs`, `Help`, `Languages`, `Support`, **`Other`** |
   | `-RU` | 35 | `Catalogs`, `Help`, `Languages`, `Support`, **`Other`**, `ReadMe` |

   Only **three** disks carry `Languages` at all. And `Other`
   (`Other/Keymaps/cz_ISO-8859-2` on CZ, `Other/ENVARC/Sys/topaz.font` on RU)
   together with RU's root `ReadMe` is material the release ships and **its own
   installer never copies** — `UPDATELOCALE` handles `Languages`, `Catalogs`,
   `Help`, `Support/Fonts` and `Support/Prefs/Presets`, and nothing else. ART
   must not place it either; a drawer the release leaves for the user to
   install by hand is not ART's to install for them.
7. Modules disk, **conditional** (see below): `LIBS/(A500|A600|A1000|A1200|
   A2000A|A2000B|A3000|A4000D|A4000T|CD32)` and `LIBS/intuition.library`;
   plus, if the ROM is older still, `L/(Ram-Handler|Shell-Seg|System-startup)`
   and `LIBS/(dos|gadtools|graphics).library`.

**Operations that read the target tree, not the media (a `PathRule` cannot
express these):**

8. `UNPROTECT` on four `Libs/*.library` before overwriting them.
9. Delete `Tools/TextEditFileTypes/Default4Types` if present.
10. `copytooltypes Update/IconEdit.info → Tools/IconEdit.info`, **only if the
    target already has that icon** — it merges tooltypes and stack into the
    user's existing icon rather than replacing it.
11. Stash the existing `S/Startup-Sequence` into a backup drawer (asks first).
12. `UPDATE` ×4 — for every file in `WBStartup`, if a file of the same name
    now exists in `Utilities`, `System`, `Tools` or `Tools/Commodities`, delete
    the `WBStartup` copy and **move** the new one into `WBStartup`. This keeps
    a user's own WBStartup choices working; on a tree ART built from scratch it
    is computable, because ART knows what it just placed.
13. `LoadModule REMOVE`, then reboot.

**Conditions, and where they are read from:**

| Condition | Read from | ART's equivalent today |
|---|---|---|
| which Modules disk | `C/AmigaModel`, a program run on the Amiga | the build already targets a machine |
| whether Modules are needed at all | `exec.library` version **and revision** (`< 47.10`), and `version res strap` (`< 47`) | `Condition::RomOlderThan { major }` — **major only**; a `47.10` test cannot be expressed |
| which extra L/ and LIBS/ files | `strapver < 47`, else `execrev < 10` | same gap |
| old install's version | `Libs/version.library` on the target | — |

**Unused by the script:** `Patch/graphics.library.pch` (+ its readme) and
`T/ed-backup` are on the disk and never copied. `C/AmigaModel`,
`C/CopyTooltypes`, `C/GuessBootDev` and `Installer` are tools the install runs,
not files it places.

## 5. The shape generalises; the lists do not

`Update3.2.1.adf`'s own `Install/Install` (1 905 lines) was read and compared.
Same procedures, same skeleton — and **every list differs**:

- its `COPYDISKCONTENT` directory list has 14 entries, not 17 (no
  `Locale/Countries`, no `WBStartup`);
- it takes **one** file from `DiskDoctor` (`c/DiskDoctor`), not three;
- its Modules step fires only on `strapver < 47`, and copies neither the
  per-machine `LIBS/(A500|…)` files nor `intuition.library`;
- it has no `copytooltypes` and no `UPDATE`-into-`WBStartup` pass.

**So an update is data, exactly like a release** — the same conclusion
`core/osinstall/recipes/*.json` already rests on. A code path per update would
be the wrong shape; a recipe per update is the shape ART already has.

## 6. The tree states its own release, and ART does not have to guess

`Prefs/Env-Archive/Versions/Release` is a vendor-written marker file:

| Source | Bytes | Content |
|---|---|---|
| `Workbench3.2.adf` (base) | 11 | `Release 3.2` |
| `Update3.2.1.adf` → `Update/Release` | 14 | `Release 3.2.1` |
| `Update3.2.2.adf` → `Update/Release` | 14 | `Release 3.2.2` |

ART's `workbench-base` component already copies `Prefs` as a subtree, so a
tree ART builds today **already says `Release 3.2`**. This is the artefact to
ask, and it settles the naming question without ART inventing an answer:
a tree is 3.2.2 when the file says so *because the update placed it*, and
`verify` can read it back. Nothing here is a directory name or a copyright
line — the two things this project has already been burned by.

## 7. Why the Amiga-side route cannot be unattended

`core/amigainstall` runs `Installer SCRIPT …` with no one watching, and treats
a requester as a timeout — deliberately, and documented as such. The 3.2.2
script forces four questions at `(user 2)`, which is above any user level and
therefore cannot be defaulted away: the introduction message, the target
directory (when `GuessBootDev` finds nothing), `SELECTLANG`'s `askoptions`, and
the closing message — plus `askchoice` for the Amiga model whenever the Modules
step fires, and `askbool` for stashing `S/Startup-Sequence`.

**So handing the update to its own installer means a person at the emulator
window.** That is not a defect in `core/amigainstall`; it is what this
particular installer is. It stays a legitimate route — with a human in it —
but it is not the unattended one, and any design that claims otherwise is
claiming something measured to be false.

The mount side, by contrast, works: the script finds its own disks either from
an extracted folder (`sourcePath`) or by mounting the ADFs itself with
`DAControl`, so ART would not have to swap floppies for it.

## 8. The three target-reading operations, measured against ART's own tree

§4's items 9–12 are the ones a `from`/`to` rule cannot state. Whether they
matter was measured rather than argued, by reading the base recipe's rules and
the base media together.

**`Tools/TextEditFileTypes/Default4Types` — real.** It is on `Extras3.2` and
reaches the tree through the `extras` component's `subtree Tools → Tools`. The
update deletes it. ART has no way to say "this component removes that path".

**`Tools/IconEdit.info` — real, and its cost is one line.** The three icons
were extracted and parsed:

| Icon | Bytes | Structure | Tooltypes |
|---|---|---|---|
| `Extras3.2:Tools/IconEdit.info` | 1 116 | `GadgetRender` 372 @78, `ToolTypes` 666 @450, nothing after | 18 |
| `Update3.2.2:Update/IconEdit.info` | 1 156 | `GadgetRender` 372 @78, `ToolTypes` 706 @450, nothing after | 19 |
| `GlowIcons3.2:Tools/IconEdit.info` | 2 266 | `GadgetRender` 36 @78, `ToolTypes` 666 @114, **1 486 bytes of IFF `FORM` after** | 18 |

The update's icon differs from the base's by exactly one tooltype entry —
`(PUBSCREEN=<name of public screen>)` — and **every entry in all three is
parenthesised**, which is the AmigaDOS convention for a tooltype that is inert
documentation rather than a live setting.

**On the tooltypes alone this would be cosmetic. It is not, and the reason is
the other half of what `copytooltypes` copies:**

| Icon | `do_StackSize` | `do_CurrentX/Y` |
|---|---|---|
| `Extras3.2:Tools/IconEdit.info` | 4 096 | 111, 45 |
| `Update3.2.2:Update/IconEdit.info` | **8 192** | 111, 45 |
| `GlowIcons3.2:Tools/IconEdit.info` | 4 096 | 198, 4 |

The update **doubles IconEdit's stack**, and the update also replaces the
`IconEdit` binary itself (`Tools/IconEdit.Z`, §4 item 1). Skipping the merge
therefore leaves a new program running on half the stack its own release
allocates for it — a functional difference, not a documentation one. An
earlier draft of this section called the whole merge "one disabled tooltype
line"; that read the tooltype array and stopped, and it was wrong.

Two more things follow. First, replacing the icon outright instead of merging
would be **actively wrong**: the icon actually in an ART-built tree is the
GlowIcons one (`glowicons` overrides `extras`), so the user would gain the
right stack but lose 1 486 bytes of ColorIcon artwork *and* have the icon jump
from (198, 4) to (111, 45). Second, a merger must splice the tooltype array
**in place** and carry everything after it through byte for byte — the
appended IFF block is exactly what a naive rewrite loses.

The `DiskObject` layout was confirmed empirically, not recalled: a parser
following `do_Magic` `0xE310`, a 78-byte header, then the optional
`DrawerData`/`GadgetRender`/`SelectRender`/`DefaultTool`/`ToolTypes`/
`ToolWindow` blocks in that order lands **exactly** on end-of-file for the two
classic icons and exactly on the start of the IFF block for the ColorIcon.

**The `WBStartup` reorganisation pass — a no-op by construction.** Not
because the pass is harmless, but because **no component in the 3.2 recipe
targets `WBStartup` at all**: `workbench-base` takes C, Classes, Devs,
Expansion, Libs, Prefs, Rexxc, S, System; `glowicons` takes Devs, Prefs,
Storage, System, Tools; `storage` takes DOSDrivers, Keymaps, Monitors,
Presets, DefIcons, Env-Archive. There is nothing in the tree's `WBStartup` for
the pass to match against.

**That is itself a finding, and it is about the base rather than the update.**
`GlowIcons3.2` carries `WBStartup/{AssignWedge,AsyncWB,AutoArrangeIcons,
DefIcons,MenuTools,RAWBInfo}.info` and `Workbench3.2` carries
`WBStartup/AssignWedge.info`; ART places none of them. AmigaOS starts what the
`WBStartup` icon says, so **an ART-built 3.2 tree starts nothing from
`WBStartup`** — and the 3.2.2 update would add `AsyncWB` and `RAWBInfo` as
programs with no icons beside them. Worth its own `ART-NNN`, independent of
this round.

## 9. The Modules condition, measured out of the ROMs themselves

§4's table left the Modules step's real test as a gap: the release asks the
**running machine** for `exec.library`'s version and for `version res strap`,
and ART has a ROM file. The design's first answer was a proxy — "older than the
47.111 this update ships" — read off the ROM header. **The proxy is wrong, and
only a measurement could show it.**

A Kickstart's own `Resident` structures carry both numbers. Scanning the three
A1200 ROMs the owner holds (a `Resident` is found by its `rt_MatchWord`
`0x4AFC` followed by an `rt_MatchTag` pointing at itself; a 512 KiB image maps
at `0xF80000`), 45 residents each:

| Kickstart | ROM header | `exec.library` | `strap` |
|---|---|---|---|
| `AmigaOs 3.2/ROM/kicka1200.rom` | 47.96 | `exec 47.7 (12.11.2020)` | `strap 45.1 (11.5.2018)` |
| `Update3.2.1/ROMs/A1200.47.102.rom` | 47.102 | `exec 47.8 (27.10.2021)` | `strap 47.2 (30.5.2021)` |
| `Update3.2.2/ROMs/A1200.47.111.rom` | 47.111 | `exec 47.10 (21.01.2023)` | `strap 47.2 (30.5.2021)` |

Run the script's own conditions against those numbers:

| Paired ROM | `exec_rev < 10 \|\| strap < 47` | Which extra files |
|---|---|---|
| 3.2 (47.96) | **on** — exec 47.7 | `strap < 47`, so `L/(Ram-Handler\|Shell-Seg\|System-startup)` **and** `LIBS/(dos\|gadtools\|graphics).library` |
| 3.2.1 (47.102) | **on** — exec 47.8 | strap is 47, so only `L/(Ram-Handler\|System-startup)` |
| 3.2.2 (47.111) | **off** | none |

**Three outcomes, where the header proxy sees two.** "Older than 47.111" puts
the 3.2 ROM's larger file set onto a machine with a 47.102 ROM — copying
`Shell-Seg` and three library modules that the release deliberately does not
copy for that ROM. Not a difference anybody could have argued their way to.

**And no new logic operator is needed to express it**, which the numbers also
settle: the smaller set is a strict subset of the larger, so declaring the two
conditions as two independent components gives the right files in all three
cases — 3.2 switches both on and their union is the larger set; 3.2.1 switches
only the smaller on; 3.2.2 switches neither. The two components place two of
the same paths, so one declares `overrides` over the other, exactly as any
other pair would.

What this costs: `Condition` needs to name the two residents rather than the
ROM header, and `core/rom` needs to read a ROM's resident table — a scan over
bytes ART did not write, so `checked_add`/`checked_mul` and a bounded
pointer-to-offset conversion throughout, and a refusal rather than a guess when
a pointer lands outside the image.

---

## What this measurement changes

1. **Item 3's "done when" is not sufficient.** "Installs without a refusal" is
   a statement about ART's own error list, not about the tree. The bar has to
   include the tree being *right*, and §6 gives a cheap, vendor-written way to
   check that claim rather than assert it.
2. **The ambiguity is one disk per update**, so resolving it is a small part
   of the work and must not be mistaken for the item.
3. **`.Z` is already built** (`core/archive/compress.rs`, ART-228, wired
   through `plan` and `apply`) — see the correction in §3. A Turkish 3.2.2
   install places about **120** files (39 + 2 system, 31 classes, 3 DiskDoctor,
   39 Turkish locale, a handful of modules) and **108 of them are `.Z`**, so
   the largest single piece of this work is done and tested.
4. **`Condition` cannot express `47.10`.** The Modules step's real test is a
   revision comparison and ART's condition carries a major only.
5. **Three operations read the target tree** (§4, items 9–12). A recipe of
   `from`/`to` rules cannot state them; whether they matter for a tree ART
   built from scratch is a design question, not a measurement — but item 10,
   `IconEdit.info`, is the one where doing nothing silently loses the user's
   own icon settings.

## Sources

- `E:\amiga\Amigatolon\paketler\3.2\AmigaOs 3.2\ADF` — 35 ADFs, read-only.
- `E:\amiga\Amigatolon\paketler\3.2\Update3.2.1` — 26 ADFs plus
  `HowToInstall`, `ChangeSummary`, `ReleaseNotes`.
- `E:\amiga\Amigatolon\paketler\3.2\Update3.2.2.lha` — 17 365 882 bytes,
  extracted to scratch: 28 ADFs, 5 ROMs, `HowToInstall`.
- `Install/Install` from `Update3.2.2.adf` (1 979 lines) and
  `Update3.2.1.adf` (1 905 lines), read in full.
- `xdftool` from amitools; 7-Zip 7z.exe for the `.lha` and for `.Z`.
