# AmigaOS 3.9 from the user's own CD (SD-2 · G5, second release)

**Date:** 2026-08-19
**Status:** approved (2026-08-19)
**Scope:** a second `MediaSource` implementation (an ISO), a second recipe
(`amigaos-3.9.json`), and the tree it produces booting under WinUAE
**Follows:** G5's AmigaOS 3.2 engine, whose own design said this would happen:
*"a future release (3.9, CaffeineOS) adds a JSON file, not a code path"*

---

## What this document is

The owner's decision, in their words: **focus on designing and producing a
current, quality AmigaOS 3.9 and 3.2 distribution that runs on PiStorm.** Games
are explicitly out — *"the community has already made good distributions"* —
and so is the launcher menu that would have served them.

This document covers the first half of that: making ART able to build a 3.9
tree at all. What goes *on top* of a clean 3.9 — current programs, drivers,
BoingBags — is deliberately a separate round, on the owner's own call: **first
get 3.9 building, content after.**

## What was measured, before anything was designed

Everything below was read off the owner's own material rather than assumed.

**The medium exists and is complete.** `E:\amiga\Amigatolon\iso\AmigaOS39.iso`
— 469 MiB, volume `AmigaOS3.9`, **7609 files in 975 folders**, holding:

```
OS-Version3.9/     the OS itself
Emergency-Boot/    a bootable rescue system
Contribution/      third-party: AHI5, CyberGraphX3, datatypes, InstallerNG…
Manuals/  Audio/  Videos/  WhoDidIt/
```

**3.9 installs onto an empty volume — it is not merely an update delta.**
`OS-Version3.9/Workbench3.5/` carries a complete AmigaOS volume layout:

```
C  Classes  Devs  Expansion  Fonts  L  Libs  Prefs
Rexxc  S  Storage  System  T  Tools  Utilities
```

(The directory is named `Workbench3.5` because 3.9 is built on the 3.5 release.
The name is the CD's, not a mistake.) This is the finding that decides the
whole shape: forum answers were equivocal about whether 3.9 needs an existing
3.1 on disk, and the medium settles it.

**The payload is plain files.** Searching the whole ISO returns **zero `.lha`
archives** in the OS payload, so there is nothing for ART to unpack before it
can place files — the same "copy these paths there" model 3.2 uses applies
directly.

**The engine is already written against a trait, not against ADFs.**
`core/osinstall/source.rs` declares `MediaSource`; `AdfSource` is one
implementation of it (`impl MediaSource for AdfSource`). A second medium is a
second implementation, which is exactly the extension point G5 left.

**And the recipe format already expresses what 3.9 needs.** The shipped 3.2
recipe is 29 components of this shape:

```json
{
  "id": "workbench-base",
  "media": "Workbench3.2",
  "required": true,
  "rules": [ { "from": "C", "to": "C", "kind": "subtree" }, … ]
}
```

`media` names a **volume**, `from` is a path **inside** it. For an ISO that is
`media: "AmigaOS3.9"` with `from: "OS-Version3.9/Workbench3.5/C"`. The model
does not change; the paths get deeper.

## 0. The medium is not a 3.9-only investment

Pointed at `E:\amiga\Amigatolon\iso` by the owner after this design was first
written, and worth recording because it changes what `IsoSource` is worth.
That folder also holds **`AmigaOS3.2CD(ZaP).iso`** (volume `AmigaOS3.2CD`,
4753 files in 736 folders), and it carries three things at once:

```
C  Devs  L  Libs  Prefs  S  System  Tools  Utilities  WBStartup  Labels
                      ← a full Workbench tree at the CD's own top level
ADF/                  ← every install disk: Install3.2, Extras3.2, Classes3.2,
                        Fonts, GlowIcons3.2, HDSetup3.2, Backdrops3.2, DiskDoctor
ROM/  NDK3.2/  GlowIconsCollection/  FAQs/
```

So the second `MediaSource` serves **both** releases, and 3.2 gains a second
route: build from one CD rather than from 36 loose ADFs. It also raises a
question this round does not have to answer — whether a recipe should ever read
an ADF *out of* an ISO (a nested medium) rather than reading the CD's own tree.
Both are possible from this disc; neither is needed to make 3.9 boot, so
neither is in this round.

The Amiga Developer CDs (v1.1, v2.1) and a `kick.rom` sit in the same folder.
They are not part of this design, but they are the kind of material the
content round will meet.

## 1. `IsoSource` — the second medium

A new `core/osinstall/source_iso.rs` implementing `MediaSource` over ART's
existing ISO reader (`core/iso`, whose correctness is checked against 7-Zip by
`scripts/iso-oracle-check.py` on four fixtures — Joliet, ISO9660-only, raw Mode
1, raw Mode 2/XA).

Three properties it must have, each mirroring what `AdfSource` already does:

- **It answers by volume name, not by filename.** `AdfSource::open` reads the
  volume name out of the root block precisely so a renamed `Workbench3.2.adf`
  still identifies itself; an ISO carries its volume label the same way, and a
  recipe naming `AmigaOS3.9` must match on that, not on `AmigaOS39.iso`.
- **It reads bounded, never whole.** A 469 MiB ISO is not to be pulled into
  memory; the reader already works a window at a time and this must not
  regress it.
- **It reports what it cannot do.** A path a recipe names and the medium does
  not hold is the engine's existing `MediaMissing`/refusal shape, not a panic
  and not a silently shorter plan.

Whether Joliet or the plain ISO9660 names win matters here: the CD carries
directory names with spaces (`AmigaOS 3.9 Manual`) which ISO9660 alone would
mangle. The recipe must be written against whichever set `core/iso` actually
exposes, and the plan step that establishes this comes before any recipe data
is authored.

## 2. `amigaos-3.9.json` — where the real work is

The code above is a day or two. **The recipe is the work**, and this document
will not pretend otherwise.

3.2's recipe was written from its media and then run against the owner's real
36-ADF set, and that run found two defects nothing else could have
([ART-111](../../ISSUES.md), [ART-112](../../ISSUES.md)) — which files a
component actually owns is knowledge that lives in the medium, not in a
specification. 3.9 is larger: 7609 files, a `Contribution/` tree, language
variants, a PowerPC section, an `Emergency-Boot` system.

So the recipe is authored **incrementally against the real ISO**, smallest
bootable set first:

1. `workbench-base` — the `Workbench3.5` tree, which is the whole volume layout
   above.
2. Whatever the base needs to boot that is not in that tree — established by
   the boot test in §4, not by reading a manual.
3. Everything else — Locale, Internet, PowerPC, `Contribution/` — as separate
   components, each `required: false` unless the boot proves otherwise.

The same rule G5 already enforces applies unchanged: **no two components may
claim the same destination** without one declaring an `overrides` relationship,
and a test asserts it over the shipped recipe.

## 3. What is deliberately not in this round

Named so they read as decisions:

- **Programs, drivers and BoingBags.** The owner's goal is a *current* 3.9 and
  `BoingBag39-1.lha` / `BoingBag39-2-*.lha` sit in their material already — but
  they said plainly: first get 3.9 building, content after. BoingBags are the
  obvious first content round precisely because they are official, versioned
  and layered, which is what makes them a good test of the content model.
- **Launcher menus (iGame / AGS).** Dropped. A probe read a real AGS entry out
  of the owner's own card image and measured the cost: `.run` scripts depend on
  a distribution's shared `Scripts:` framework and `$Expert`/`$HW` variables,
  and the artwork is genuine ILBM (320×256, 6–7 bitplanes, ByteRun1) which
  would need an encoder ART does not have. The owner's ruling makes it moot —
  *"AGS already exists; we can install it into the OS anyway"*.
- **Games.** Out, by the same ruling.
- **The one-flow plumbing** (media → tree → card image without hand-carrying
  paths) remains wanted and is its own round; a second release is worth more
  once there is a second release to carry.

## 4. How this is verified

The engine's own tests are synthetic — ART ships no Amiga content — so they
cannot catch a *recipe-data* mistake. Two things stand in for that, both
already precedent in this repo:

- **An `#[ignore]`d, environment-gated hook** that runs the real engine against
  the owner's own ISO, the way
  `run_the_real_engine_against_the_users_own_media_when_asked` does for 3.2.
  It reports components on, files and directories written, and refusals.
- **The tree must boot.** 3.2's tree booted AmigaOS to a clean Workbench under
  WinUAE with the owner's licensed ROM, and that is what proved the recipe
  rather than any assertion. **The same bar applies here and this round is not
  done until a 3.9 tree boots.** It needs no card and no hardware, which is
  what makes it the right target while the microSD, reader and cable are still
  unaccounted for.

A 3.9 tree that boots is also the first evidence for the owner's actual goal —
a distribution that runs on a PiStorm — since the card path's remaining rung is
the card, not the volume.

## 5. What could make this harder than it looks

Stated now rather than discovered later:

- **Kickstart.** 3.9 expects a 3.1 ROM (V40). ART already records what a tree
  requires and compares it against a card's ROM (G9's pairing check), so the
  recipe must state 3.9's requirement in that same field rather than leaving it
  unsaid.
- **`SetPatch` and the boot sequence.** 3.9's startup differs from 3.2's; the
  `First-Install` tree on the CD carries its own `c/SetPatch`, `loadwb`,
  `iprefs` and `mount`, which suggests the boot path is part of what must be
  placed, not an afterthought.
- **Language variants.** `Locale`, `Locale.Euro` and `Special-Locale` are three
  trees, and 3.2's real run already showed that placing the wrong locale files
  is the kind of thing only a running system reveals.
