# ROM pairing — does this Kickstart suit this volume? (SD-2 · G9)

**Date:** 2026-08-17
**Status:** approved (2026-08-17)
**Scope:** `core/osinstall` (the manifest), `core/card/manifest.rs` (read, not
written), a new pure comparison, and the preload screen's confirmation step
**Gap:** [sd-appliance-gap-analysis.md](../../sd-appliance-gap-analysis.md) G9

---

## What this document is

The design for the last unbuilt half of G9. The other half has been built
since SD-1: `CardBuilder` puts a Kickstart on the FAT32 partition under the
name `config.txt`'s `initramfs` line points at, so the Pi hands the m68k side
a ROM. What is missing is the question nobody asks: **is that the ROM this
system volume was built for?**

It is not a plan. The implementation plan follows.

## The failure it exists to prevent, observed

On 2026-08-16 a distribution tree built by `core/osinstall` was put on a PFS3
volume and booted under WinUAE with a licensed Kickstart 3.1 (V40). AmigaOS
3.2 started, read its own `Startup-Sequence`, and stopped:

```
This disk must be booted from Kickstart ROM 3.2 (V47)
or from the 'Modules' disk that matches your hardware.
```

Nothing was broken. The tree was correct, the volume was correct, the ROM was
the wrong one — and **every layer of ART had the information needed to say so
in advance and none of them said anything.** `core/osinstall` had read that
ROM to decide the plan; the card knew which ROM it carried; the two facts
never met.

That is the whole of this design: make them meet, before the destructive step.

## The shape: a check, not an object

The gap analysis calls this "ROM profiles" and points at the multiboot
document's `rom_profile`. **This design deliberately builds no such object.**
A named, stored, reusable pairing (ROM + volume + Emu68 config) is a real
idea, but its customer is G16 — multiboot *is* "several ROMs and several
volumes", and inventing the concept here would mean designing it twice.

What ships instead: each side records what it already knows, and one pure
function compares them at the one moment they meet.

```
core/osinstall  →  distribution.json   "planned for this ROM, needs V47"
core/card       →  card.manifest.json  "the boot partition carries this ROM"
                                ↓
                    core/rom::pairing (pure)
                                ↓
              the preload screen, before formatting
```

## 1. What the tree records

`DistributionManifest` gains one field:

```rust
pub struct PairedRom {
    /// Identity, as `core::rom::identify_rom` answers it — including for a
    /// licensed Amiga Forever ROM, which is decoded first (ART-128).
    pub name: String,
    pub sha256: String,
    /// The value the ROM stores 24 bytes before its end: what distinguishes
    /// `40.68 (A1200)` from `40.68 (A4000)` (ART-104). `None` for a dump too
    /// short to hold one.
    pub stored_checksum: Option<u32>,
    /// What the ROM states about itself. `None` for pre-2.0 ROMs, which state
    /// nothing.
    pub stated_major: Option<u16>,
    pub compatible_models: Vec<String>,
    /// **The load-bearing field.** Whether the plan switched on a ROM-update
    /// component (`modules-a1200` and its siblings) — that is, whether this
    /// tree carries the modules that let an older ROM run it.
    pub carries_rom_modules: bool,
}
```

Everything but the last field is a copy of what `identify_rom` already
returns. The last field is what makes the check possible **without
re-planning**: it is the difference between *"this tree needs a V47 ROM"* and
*"this tree brings its own"*.

`plan()` already resolves it — `components_on` contains `modules-a1200` or it
does not — so this records a decision already made rather than making a new
one.

## 2. What the card offers

From `<card>.manifest.json`, which SD-1's G7 already writes beside every card
ART builds: the `boot_files` entry whose name is the firmware's
`kickstart_file`.

Two properties of that source decided it over the alternatives:

* **It describes the card, not the build.** `boot_files` entries are hashed
  from the bytes actually placed in the boot partition. `source.kickstart_sha256`
  in the same manifest is the *source file's* hash — provenance, and since
  ART-128 not the same thing at all for a licensed Amiga Forever ROM, whose
  file on disk is encrypted and whose bytes on the card are not.
* **ART cannot read FAT32.** It writes that filesystem with `fatfs` and has no
  reader for it, which is why G8 answers the boot files from the manifest and
  reports them as `not-checked` rather than ticking them. This design inherits
  that honestly: **no manifest beside the card means the pairing cannot be
  checked, and ART says so.** A missing answer is never rendered as a pass.

## 3. The comparison

A pure function in `core/rom`, taking the two records and returning a verdict.
It does **not** ask "is this the same ROM". It asks the tree's own question
again, against the card's ROM:

| Case | Verdict |
|---|---|
| Same `sha256` | `Paired` — nothing to say |
| Different ROM, tree carries its own modules | `Suitable { rom }` — any Kickstart the modules support will run it |
| Different ROM, tree needs V47, card's ROM states ≥ 47 | `Suitable { rom }` |
| Different ROM, tree needs V47, card's ROM states < 47 | `Unsuitable { needs, found, consequence }` |
| Card's ROM states nothing (pre-2.0), tree needs V47 | `Unsuitable` — a ROM that cannot state 47 is not 47 |
| No card manifest, or no ROM in it | `NotChecked { why }` |

`Unsuitable` carries what the machine will do, not an adjective: the
`consequence` is the sentence AmigaOS itself prints, which this project has
seen on a screen and can therefore quote rather than paraphrase.

**The threshold is not invented here.** `Condition::RomOlderThan { major: 47 }`
is already in the shipped recipe and is what `plan()` evaluates; the comparison
reads the same condition rather than hard-coding 47 a second time. A recipe
that changes its threshold changes this check with it.

### A floor nobody has measured

`Condition::RomOlderThan { major: 47 }` has a ceiling and no floor: it says a
pre-V47 ROM needs the modules, and nothing about how old a ROM may be before
the modules stop rescuing it. A 3.2 tree carrying `modules-a1200` almost
certainly will not run on a Kickstart 1.3, but **ART has not measured where
that boundary is**, and the recipe does not claim to know either.

So the verdict for a modules-carrying tree is `Suitable` against any ROM, and
this document records that as *the recipe's claim, repeated* rather than as
ART's own finding. Inventing a floor here would be the exact mistake ART-104
was — a threshold nobody checked, presented as a fact. If somebody measures
one, it belongs in the recipe as a second condition, and this comparison will
read it without changing.

### Machines are reported, never enforced

`compatible_models` now says which Amiga a ROM is for (ART-104), and it is
tempting to refuse an A4000 ROM on an A500 build. This design does not: the
user's own answer to that question is *"people boot odd combinations on
purpose"*, which is why `rom_suits` has always been a note. The verdict may
mention the machine; it never decides on it.

## 4. What the user sees, and when

At the preload screen's confirmation step — the one place where a tree and a
card meet, and the last moment before a partition is erased.

* `Paired` renders nothing. Silence is the correct report for "as expected".
* `Suitable` renders one muted line naming the ROM.
* `Unsuitable` renders a warning above the confirmation checkbox, with what
  the Amiga will do. **It does not block.** The user decided this: the machine
  does boot, it prints a message, and the ROM can be changed afterwards
  without rebuilding anything — so this is a warning, not ART-128's refusal,
  and the difference between the two is whether the card can serve *any*
  purpose as written.
* `NotChecked` renders one muted line saying what was missing.

Strings are `Phrase` values, translated in both catalogues, per `src/lib`'s
rule. The verdict itself is a typed value with no English in it (ART-060).

## 5. What this does not do

Named against the temptation to grow it:

* **No ROM profile object, no persistence, no picker.** See "a check, not an
  object" above; G16 owns that.
* **No writing.** Nothing in this design changes a card, a ROM or a tree. It
  reads two manifests and forms an opinion.
* **No re-planning.** The check never re-runs `osinstall::plan`; that would
  need the original media, which by then may be nowhere.
* **No enforcement on machine mismatch.** Reported only, as above.

## 6. How it is proved

Unit tests on the pure comparison — every row of the table above, including
the two `NotChecked` reasons, since a missing answer rendering as a pass is
the failure mode G8 exists to prevent.

Then against real material, which is on this machine today:

* `dist-3.2b` — the tree built for the user's **V47** ROM (`carries_rom_modules:
  false`), against a card manifest naming their **V40** ROM → `Unsuitable`,
  quoting what AmigaOS prints. This is the pairing that actually failed on
  2026-08-16, so the test reproduces a defect that has been observed rather
  than imagined.
* The same tree against a manifest naming the V47 ROM → `Paired`.
* `dist-3.2-v40` — the tree built for the **V40** ROM, whose plan switched
  `modules-a1200` on (`carries_rom_modules: true`) → `Suitable` against either
  ROM.

The third case is the one worth having: it is the only evidence that the check
reads the tree's own capability rather than comparing version numbers.

## Open questions

None blocking. One noted for whoever builds G16: if a card is ever to carry
several ROMs, `boot_files` already lists them all, and this comparison takes
one ROM record — so the plural case needs a caller that picks, not a different
verdict type.
