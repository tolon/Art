# Should ART build PiStorm cards at all?

**Date:** 2026-08-22
**Status:** research. **The owner answered the question while this was being
written; § 5 records the answer and the options are kept only as the reasoning
that led to it.**
**Why it was asked:** *"bu şekilde karmaşık ve başarısız bir installer ile
ilerlemeyelim. Hali hazırda mevcut çalışan bir proje zaten var."*

**The owner's answer, in their own words:** *"bizimkisi de çalışmalı, o yüzden
söyledim"* and *"ART'ın amacı bir suit yapmak Amiga retro meraklılarına."*

So the complaint was never *stop building cards*. It was **ours has to work
too** — the incumbent is the proof that this is achievable, and the standard to
be measured against. And ART is a **suite**: the card is one tool among many,
which is exactly why it may not be dropped and also why it must not consume the
project.

The question is fair and it is asked at the right time: the OS Builder round
has just produced five defects in one sitting, and the thing it is building
already exists, works, and is in wide use. This document sets out what the
incumbent does, what ART does, where they overlap, and what is left of ART if
the overlap is removed. It argues for a narrowing. It does not make it.

Sources are in
[2026-08-22-emu68-imager-research.md](2026-08-22-emu68-imager-research.md) —
everything below was read on 2026-08-22 from the tool's own documentation and
source, not recalled.

---

## 1. The uncomfortable fact, stated first

**No card ART has ever built has been flashed or booted.** That is
`docs/STATUS.md`'s own words, not a summary of it. The Emu68 Imager writes
cards that thousands of people boot.

Everything that follows should be read against that. ART's card path is a
reimplementation whose output has never been shown to work, of a tool whose
output demonstrably does.

---

## 2. What the Emu68 Imager already does

From its own documentation:

- **Five AmigaOS releases**: 3.1, 3.2, 3.2.2.1, 3.2.3, 3.9 (3.9 from the
  installation CD plus BoingBag 1 and 2 `.lha` files).
- **Simple and Advanced modes** — advanced gives *"full control over disk
  partitioning"*.
- **Partitions and formats the card**, writes Emu68, installs Workbench,
  installs **WHDLoad**, installs *"many useful default tools"*.
- **Copies a folder of your own** onto the Workbench partition.
- **WiFi through the Pi's own chip, and a browser** — internet on the Amiga.
- **Picasso96 (RTG)**, Roadshow (networking), iBrowse, **PFS3**.
- **Per-Pi finalisation on first boot**: PAL/NTSC, HDToolBox and the FAT32
  partition driver *"according to your RaspberryPi (rPi3/4)"*, deletion of
  unneeded libraries, icon tidying, locale preferences.
- **Buptest** on first boot — verifies Amiga↔PiStorm communication, then
  disables itself.
- **An add-on package mechanism**: drop files in the FAT32 `Install` folder,
  boot, double-click *Install Packages*. A WHDLoad Demos pack is published for
  it.
- **TransferKick**, for getting Kickstart images into `Devs:Kickstarts`.

It is built on **HST.Imager** and **HST.Amiga** by Henrik Nørfjand Stengaard,
plus **HDF2Emu68**, `unlzx` and 7-Zip. Most of it is downloaded at run time
*"to keep the package small in size as well as free and legal to distribute"*.

**This is ART's SD-1 and most of SD-2, finished, and with several things ART
has never attempted** (RTG, networking, a browser, per-Pi driver selection,
first-boot hardware verification).

---

## 3. What ART does that it does *not*

Four things, and they are real. They are also **smaller than the overlap**.

1. **Encrypted Cloanto ROMs.** The imager's requirements say the Kickstart must
   be *"single file and NON-ENCRYPTED"*. ART decodes an Amiga Forever ROM with
   the `rom.key` beside it and identifies it like any dump
   ([ART-128](../../ISSUES.md)) — and refuses a card build when the key is
   absent, because such a ROM used to reach the boot partition still encrypted.
   The owner has a licensed Amiga Forever, so this is not hypothetical for
   them.
2. **It works with no internet.** The imager *requires* an active connection
   and downloads its components each run. ART's rule is the opposite by design
   (§2: ART never downloads a distribution) and everything it needs is the
   user's own material on their own disk.
3. **It reads cards, not only writes them.** `core/card` opens a real card,
   unions the filesystem drivers across every RDB
   ([ART-097](../../ISSUES.md)), and reports which partitions lack a driver;
   there is an image health check (G8) and an independent 7-Zip/amitools
   oracle. The imager creates; it does not diagnose.
4. **A manifest with provenance.** `distribution.json` records, file by file,
   which component and which medium every byte came from — which is what makes
   adding or removing a component later possible, and what
   `core/osinstall/chain` reads to refuse a BoingBag out of order. Nothing in
   the imager corresponds to it.

And two that are about the product rather than the format:

5. **Turkish.** The imager is English only. ART is bilingual by construction,
   and the owner's own stated reason — most of the people using this are over
   fifty — is not served by an English-only tool.
6. **The safety pipeline**: preview → confirm → backup → verify, an operation
   log, and refusals that name what is missing. The imager warns once, in
   capitals, that the card's contents will be destroyed.

---

## 4. Where that leaves the overlap

| Job | Imager | ART | Honest verdict |
|---|---|---|---|
| Partition + format an SD card | ✅ shipping, in use | 🟡 built, **never booted** | **Duplicate.** ART is behind and unproven |
| Write Emu68 boot partition | ✅ | 🟡 [ART-103](../../ISSUES.md), [ART-091](../../ISSUES.md), and the three-kernel `config.txt` question still open | **Duplicate**, and ART is the one with open questions |
| Install AmigaOS from the user's own media | ✅ 5 releases | 🟡 2 recipes, 3.2.2.1 not expressible | **Duplicate.** ART is behind |
| WHDLoad, RTG, networking, browser on the card | ✅ | ❌ | **Imager only** |
| First-boot hardware verification (Buptest) | ✅ | ❌ | **Imager only** |
| Encrypted Cloanto ROM | ❌ refuses | ✅ | **ART only** |
| Offline | ❌ requires internet | ✅ | **ART only** |
| Read/diagnose an existing card | ❌ | ✅ | **ART only** |
| Per-file provenance, add/remove a component later | ❌ | ✅ | **ART only** |
| ADF/Gotek/floppy work, Collection (2787 titles), file manager, ROM manager (154 dumps), Aminet, hex tools | ❌ | ✅ | **ART only — and it is most of ART** |
| Turkish | ❌ | ✅ | **ART only** |

**The pattern is clear enough to say out loud.** Everything ART uniquely offers
is either *around* the card (identify a ROM, organise a collection, read a
disk, drive an emulator, speak Turkish) or *about* the tree (provenance,
chaining, refusals). Everything in the middle — cutting partitions, laying an
RDB, writing a boot partition — is a mature incumbent's job that ART is
redoing, more slowly, without having booted the result.

---

## 5. The three options, and which the owner chose

### A. Stop at the tree. Hand the card to the imager.

ART's product becomes the **distribution tree** — built from the user's own
media, with a manifest, with encrypted ROMs handled, offline, in Turkish — plus
everything it already does around it. The card is written by Emu68 Imager,
whose *Transfer folder* feature exists precisely to take a folder of content,
and which ART could name and explain on screen.

- **Cost**: G2, G3, G4, G7 and the card half of G5 stop being developed. Some
  of that code stays useful for *reading* cards; the writing half becomes dead
  or a diagnostic.
- **Gain**: the whole unproven half of the risk disappears. What is left is
  what ART is actually good at, and what nothing else does.
- **Honest risk**: the imager needs internet and refuses encrypted ROMs, so a
  user in exactly the owner's position hits both walls at the last step. That
  is the one thing that argues against A, and it is not small.

### B. Keep card writing, but only for what the imager cannot do.

ART writes a card *when* the ROM is an encrypted Cloanto one, or when there is
no internet, and otherwise says plainly: *use Emu68 Imager, here is why, here
is your tree*. The card writer stays, but stops being the main path and stops
needing feature parity.

- **Cost**: two paths to explain and keep honest — this project's own worst
  failure mode is a screen that says a confident wrong thing about which path
  ran, and [ART-120](../../ISSUES.md) already shows how carefully that has to
  be done.
- **Gain**: the owner's own case still works end to end.

### C. Carry on as now.

Reach parity with a tool that has RTG, networking, a browser, five releases,
per-Pi drivers and first-boot verification — while ART's card has never booted.

**The owner chose neither A nor B: ART keeps building cards, and the standard
is that it works.** Which is a fourth option this document had not framed, and
it is the right one for a *suite* — a toolkit whose card builder was a dead end
would be a toolkit with a hole in it, and pointing users at another program for
the one step that produces the artefact is not what a suite does.

What changes is therefore **not the scope but the method**: the incumbent stops
being a competitor to reason about and becomes the **reference implementation
to measure against**. Concretely:

- The seven checks in
  [the research](2026-08-22-emu68-imager-research.md) § "What this changes for
  ART" are no longer optional curiosities. Each is a place ART may be quietly
  wrong about a card nobody has booted.
- The imager's **`Check` button** is the pattern
  [ART-199](../../ISSUES.md) just adopted, arrived at independently. Its
  **Simple / Advanced split** is the same shape as the flow design's wizard.
  Both are evidence those decisions were right.
- Its **first-boot verification (Buptest)** has no ART equivalent, and ART's own
  rule — *a card is verified by something that is not ART* — argues for one.
- Its **per-Pi driver selection** (HDToolBox and the FAT32 driver chosen for
  rPi3 vs rPi4) is a fact about real cards ART's writer does not know.

And what stays true from § 4 is the *ordering* advice, not a cut: the parts of
ART nothing else does — the ROM identification, the collection, the disks, the
tree with its provenance, Turkish — are what make the suite worth having, and
they should not be starved to chase parity on RTG or a bundled browser. **Card
writing has to work. It does not have to win.**

---

## 6. What I would check before deciding — and have not

Listed because deciding on the strength of this document alone would repeat the
mistake it is written to avoid.

1. **Does the imager's *Transfer folder* actually accept a tree of ART's
   shape** — an Amiga directory tree with `.uaem` sidecars — or does it flatten
   the metadata? If it drops protection bits, option A loses something real.
   *Not tested.*
2. **Can the imager use a card ART built, or a tree ART built, at all?** Never
   tried in either direction.
3. **How much of `core/card`, `core/preload` and `core/mbr` is reading versus
   writing?** Option A keeps the readers. That ratio decides how much is
   actually discarded, and it is one afternoon's measurement.
4. **What does `docs/product-vision.md` claim ART is for?** If it says "build
   the card", this is a change to the product, not to a plan.
5. **Does the owner want a tool that needs no internet?** That single
   requirement is most of the argument for B.

---

## 7. One thing that is not in doubt

The research that produced this was worth doing on its own account, whatever is
decided: it confirmed ART's card *model* is right (MBR + `0x76` areas, one RDB
each), found that the reference tool drives the same `hst-imager` ART keeps as
a fallback, and turned up seven concrete checks against ART's own writer. If
option A is chosen, that knowledge still pays — reading a card correctly needs
exactly the same facts as writing one.
