# The content layer: updates, drivers and programs on top of a booted OS

**Date:** 2026-08-19
**Status:** approved (2026-08-19)
**Scope:** packages as a fourth and fifth `MediaSource`, two entry points into
one placer, and a preview that classifies every overwrite before one happens
**Follows:** the AmigaOS 3.9 round, whose design deferred exactly this —
*"first get 3.9 building, content after"* — and which ended with a 3.9 tree
booting to a clean Workbench under WinUAE

---

## What this document is

The owner's goal, unchanged since the OS Builder became the focus: **a
current, quality AmigaOS 3.9 and 3.2 distribution that runs on PiStorm.** A
base install is not that. What makes it current is what goes on top —
official updates, drivers, and the programs a real distribution carries.

The previous round produced the base and proved it boots. This one puts
things on it.

## What was measured, before anything was designed

The owner's package folder, `E:\amiga\Amigatolon\paketler`, holds 58 items.
Three of them were opened, and they turned out to have **three different
shapes**. That is the finding this whole design answers to.

**Shape A — an archive inside an archive.** `BoingBag39-1.lha` (5.2 MB) does
not carry loose files. It carries an `Updater` program, its catalogs in 17
languages, and one member stored **uncompressed**:

```
1699456  1699456  BoingBag3.9-1\AmigaOS-Update
```

Its first four bytes are `50 4b 03 04` — a **ZIP**. Inside are 234 entries
whose top-level names are a Workbench volume, exactly:

```
C  Classes  Devs  Fonts  L  Libs  Prefs  S  Storage  System  Tools
Utilities  WBStartup
```

So a BoingBag's payload maps straight onto a system volume, which is the
shape the recipe engine already speaks. `BoingBag39-2.lha` has the same
structure (113 entries). The `Updater` and its catalogs are the Amiga-side
installer; ART places files itself and does not need them.

**Shape B — loose files at direct paths.** `BoingBag39-2-turkce.lha` (41
entries) is `LocaleUpdate/locale/catalogs/türkçe/*.catalog` — no nesting, no
blob. The simplest case, and the owner's own language.

**Shape C — an Amiga Installer script.** `Euro-Update.lha` (107 entries)
carries `Euro-Install` and `C/Installer`. This is the Commodore Installer,
and refusing to run it is the premise `core/osinstall` was built on.

**How much version information really exists.** The preview this design
turns on rests on AmigaOS `$VER:` strings, so they were counted rather than
assumed. Across the 588 files of the real 3.9 base tree:

```
files scanned : 588
carry $VER:   : 181  (31%)
```

Examples read straight out of the tree: `assign 37.4 (25.4.91)`,
`adddatatypes 44.4 (4.8.99)`, `ConClip 44.3 (22.9.99)`. Thirty-one per cent
is enough to be worth showing and nowhere near enough to depend on, and the
design says so rather than pretending otherwise.

## 1. A package is a medium

`core/osinstall/source.rs` declares `MediaSource`; `AdfSource` and
`CdSource` implement it. Packages add two more implementations, not a second
engine:

- **`ArchiveSource`** — paths inside an LHA, ZIP or 7z, read through
  `core/archive`'s single security gate, with `safe_join` between an entry
  name and any destination. Shape B is this and nothing more.
- **A nested medium** — Shape A: an `ArchiveSource` over a member of another
  archive. The recipe names the member (`AmigaOS-Update`) and the paths
  inside it; ART opens the outer archive, reads that one member, and opens
  the inner one over its bytes.

The 3.9 round already raised this and set it aside: *"whether a recipe
should ever read an ADF out of an ISO (a nested medium) — both are possible
from this disc; neither is needed to make 3.9 boot."* A BoingBag needs it, so
it lands here.

**Bounded, as everywhere else.** An archive member's declared length is
untrusted; nothing allocates from it without checking, and the inner archive
is opened over bytes already bounded by the outer gate — the same shape
`rp9-manifest.xml` is read with.

## 2. Two ways in, one placer

Both were asked for, and each answers a different need:

- **Produce** — base plus chosen packages, in one pass, into a new tree. The
  product is reproducible: the same selection gives the same tree.
- **Add** — an existing tree plus one package. The owner already has a 3.9
  tree that boots; adding a BoingBag to it should not mean rebuilding.

They share the placer, and the test that keeps them honest is the owner's
own:

```
produce(base + A + B)  ==  add(produce(base + A), B)
```

byte for byte. If the two disagree, one of them is wrong, and no amount of
reading either path would have told us which.

## 3. Nothing is overwritten silently

The rule the owner chose, and the one that shapes the screen. Before any
package is applied, ART reads what it would land on and sorts every file
into one of four classes:

| What ART finds | What it says |
|---|---|
| The bytes are identical | Not an overwrite. Not listed, not asked about |
| Both carry `$VER:`, incoming is newer | `assign 37.4 → 45.9` |
| Both carry `$VER:`, incoming is **older** | **A downgrade, marked as one** |
| Both carry `$VER:`, **the same version, different bytes** | `assign 44.1 — same version, different bytes` |
| No `$VER:` on one or both (69% of files) | `different, 12 KB → 14 KB` — never an invented version |

**The fourth row was added after the third was implemented**, and the reason
is worth keeping. An implementation reading only the first three rows put
equal versions into the downgrade class, so re-applying a package rendered
`44.1 → 44.1` in the colour reserved for going backwards. Equal is not older,
and scaring a user away from a legitimate update is as much a §89 failure as
reassuring them about a bad one. The other available answer — treating it as
unversioned and showing only sizes — is wrong in the opposite direction: both
sides *do* say what they are, and hiding that they agree throws away the most
useful thing on the row.

**The comparison is between two files claiming to be the same program.** A
`$VER:` marker is found by scanning for it, and the first one in a file is
not always the file's own: a binary can embed another program's version
string ahead of its own. Skipping that check is how an *older* file renders
as an upgrade, which was found by construction during this round rather than
reasoned about: existing `assign 45.9` against an incoming file whose first
marker read `dos.library 47.0` produced `Upgrade(45.9 → 47.0)` for a file
that was in fact `assign 37.4`.

Comparing the two files' marker **names to each other** is not enough, and
that too was found by construction: if both files carry the *same* foreign
marker ahead of their own, the names agree and the numbers being compared are
still the wrong ones. **The marker must name the file it was found in.** A
marker whose name is anything else means ART has not measured a version for
that file, and the row falls to the unversioned class — a mismatch is never a
verdict, only an absence.

"Names the file" is matched case-insensitively (AmigaDOS is), against the
file's name, its name without extension, or its parent drawer's name joined
to either. That last form is not a special case for one file: a font is
`Fonts/courier/11` and calls itself `courier11`, which is genuinely its
identity.

Measured on the real 3.9 tree, which is what settled the rule rather than
taste:

```
markers found                  : 180
marker names its own file      : 173  (96%)
dropped                        :   7
```

and among the seven is `L/Queue-Handler`, whose first marker reads `Version`
— the bypass occurring in real material, caught. The cost of the rule is that
seven files fall from a version label to a size comparison; the benefit is
that no file can be labelled with another program's numbers.

**"Newer" needs defining, and the definition is AmigaOS's own.** A `$VER:`
string is `name version.revision (date)` — `assign 37.4 (25.4.91)`. Compare
version first, then revision, both as integers; the date is not part of the
comparison, because a rebuilt binary can carry a later date and the same
version and is not an update. A string ART cannot parse into two integers is
treated as *no version string at all* and falls to the fourth row — guessing
at a malformed one would be the invention the fourth row exists to prevent.

The third row is not hypothetical. The 3.9 engine exists because of it:
`ModulesA1200_3.2.adf` holds fourteen commands and **thirteen are older than
the ones `Workbench3.2` already carries**, so copying the disk wholesale
would downgrade thirteen commands to install one. A package that does the
same must be visible doing it.

**Confirmation is per package, not per file.** A BoingBag exists to replace
things; asking fifty-seven times would train the user to click through. One
decision, with the list in front of them.

A curated recipe's `overrides` declaration does **not** skip the preview. It
only marks the collision as expected — an undeclared collision is shown as
undeclared, which is the existing project-wide rule (*no two components may
claim the same destination without one declaring an `overrides`
relationship*) reaching the content layer intact.

## 4. Two ways a package is understood, in two rounds

Both are wanted, and they are not the same size:

- **A curated recipe**, exactly like `amigaos-3.9.json`: data, not code. Used
  where order and overwriting are unforgiving — the BoingBags are official,
  versioned and layered, and getting 39-2 applied before 39-1 is a wrong
  system, not a warning. **This round.**
- **Inspect and propose.** For a package ART has never seen, it reads the
  archive, works out how its paths map onto a system volume, and proposes
  that mapping for the user to accept. Fifty of the owner's fifty-eight
  files will never have a hand-written recipe, and without this they are
  simply unsupported. **The next round.**

The split is the owner's call, and the reasoning is the shape this project
keeps returning to: everything else here extends an engine that already
exists, while inference is a new subsystem with its own logic, its own
screen and its own ways of being wrong. The 3.9 round worked because it made
one thing work end to end before making it wide, and it ended with a tree
that boots rather than a half-finished surface.

When it lands, the proposal is a proposal: ART does not apply a mapping it
inferred without being told to — the same rule the collection's name
suggestions already follow (*ART must not guess at a name: it proposes, and
the user accepts*). Nothing in this round should make that harder to add:
the placer takes a resolved set of rules, and where those rules came from —
a shipped recipe or an accepted proposal — is not its business.

## 5. What ART refuses, and says

A package whose installation is an Amiga Installer script (Shape C) is
**refused with its reason**. Not half-copied, not silently skipped. ART does
not run the Amiga Installer — that is what `core/osinstall` was built
instead of — and a package that has no other way in is a package ART cannot
place today.

This is a permanent boundary, not a gap to be closed later, and the refusal
should say what the package would need in order to be placeable (a recipe
naming its paths) so the answer is actionable rather than a dead end.

## 6. Deliberately not in this round

- **Removing a package from a tree.** The Produce flow rebuilds in about six
  seconds, measured on a release build; keeping every overwritten file aside
  to make removal possible buys little and costs a second, shadow copy of
  the tree. Rebuild is the answer to "undo".
- **Running anything on the Amiga side.** §5 above.
- **Fetching packages.** The owner's folder is the source. Aminet is its own
  module and its own decision.
- **Inspect and propose** (§4). Next round, deliberately, which means this
  round supports exactly the packages it ships recipes for and must say so on
  screen rather than appearing to accept any archive.

## 7. What ships first, and how it is verified

First round: **BoingBag 39-1, BoingBag 39-2, and the Turkish language pack**,
with curated recipes — the three packages that between them exercise both
archive shapes ART can read (a nested ZIP inside an LHA, and loose files at
direct paths), an ordering dependency, and the owner's own language. A
package with no recipe is refused with its reason, exactly as a Shape C
package is: this round's honest boundary is "the packages ART knows", and
§89 requires saying that rather than implying more.

Verification is the bar the previous round set and met, applied again:

- The engine's own tests are synthetic, so they cannot catch a recipe-data
  mistake. An `#[ignore]`d, environment-gated hook runs the real engine
  against the owner's own packages, as
  `build_the_real_39_tree_when_asked` does for the disc.
- The equivalence test of §2, which no reading of the code substitutes for.
- **The tree must still boot, and show its update.** A 3.9 tree with
  BoingBag 2 applied reports a different Workbench version than a base one;
  booting it under WinUAE with the owner's licensed ROM — the same way the
  base tree was proved on 2026-08-19 — is what closes this round. A tree
  that builds and does not boot is not a distribution.

## 8. What could make this harder than it looks

Stated now rather than discovered later:

- **BoingBag order is not a preference.** 39-2 assumes 39-1. A recipe must
  express that dependency, and the engine must refuse an order that violates
  it rather than producing a subtly broken system.
- **The `Updater` may not be inert.** ART reads `AmigaOS-Update` and ignores
  the `Updater` program, which assumes the payload needs no processing
  beyond placement. The 234 paths look like plain files; that they *are*
  plain files is an assumption to verify against a booted system, not from
  the listing.
- **`WBStartup` and `Devs` arrive for the first time.** The base recipe
  places neither `T` nor `WBStartup` (both empty on the disc, verified). A
  BoingBag carries five `WBStartup` entries and three `Devs`, so the tree
  gains drawers the base never had — and what a program in `WBStartup` does
  at boot is exactly the kind of thing only a running system reveals.
- **Language packs may collide with the Locale the base already placed.**
  Three locale trees were flagged as a hazard in the 3.9 design and never
  exercised. A Turkish catalog pack is the first thing to touch them.
