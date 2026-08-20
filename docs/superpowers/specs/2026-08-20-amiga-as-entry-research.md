# Research note: the Amiga `AS` System Use entry (ART-078)

**Date:** 2026-08-20
**Status:** research only — no design, no decisions
**For:** ART-078, *an AmigaOS CD's protection bits and file comments are lost*

Written before the fix, under `CLAUDE.md`'s "Research before design" rule.

---

## What the entry is, and where it came from

The Amiga ISO 9660 extensions were introduced by **Angela Schmidt**, with help
from **Andrew Young — the primary author of RRIP and SUSP themselves**. That
provenance matters: the `AS` entry is a SUSP application designed alongside
Rock Ridge rather than bolted beside it, and **Amiga Rock Ridge and POSIX RRIP
may both be used on the same volume**. So reading `AS` does not mean giving up
anything ART already reads.

What it carries, in the terms ART already uses:

- the AmigaDOS **protection flags** — the `HSPARWED` set `core/volume/write/uaem.rs`
  already renders and inverts
- an **optional comment**
- explicitly including the **`P` (pure)** bit — a re-entrant command, which is
  what `Resident C:Assign PURE` in a real Startup-Sequence depends on — and the
  **`S` (script)** bit

Those two bits are not decoration. ART already knows why: `core/volume/write/uaem.rs`'s
own module doc says WHDLoad packs depend on `S` and `P` surviving a copy, and
the AmigaOS 3.9 tree's boot needs `p` on `C:Assign`.

## Where a working implementation can be read

Two, and both are better than a prose description:

- **`ODFileSystem`** — Stefan Reinauer, <https://github.com/reinauer/ODFileSystem>,
  version 0.7 as of July 2026. An open-source AmigaDOS handler replacing the
  Amiga's own CDFileSystem, supporting ISO 9660, Rock Ridge, Joliet, UDF, HFS,
  HFS+ and CDDA. Its release notes state it **parses the Amiga-specific Rock
  Ridge `AS` SUSP entry on top of normal RRIP handling** — so it is a current,
  maintained implementation of exactly the thing ART does not read.
- **`AmiFUSE`** — same author, <https://github.com/reinauer/amifuse> — native
  Amiga filesystems on macOS/Linux/Windows through FUSE. Host-side rather than
  Amiga-side, which may make it the closer model for ART.

Historically, **MakeCD** (Angela Schmidt with Patrick Ohly) was the first
software to master a CD with these extensions, so a disc that carries them was
very likely written by it.

## What ART does today, and what is actually missing

`core/iso` reads ISO 9660 and prefers Joliet when a disc has it. **It reads no
System Use area at all** — so neither the POSIX RRIP fields nor the Amiga `AS`
entry are looked at. That is the whole of ART-078.

Note what this round already learned about the owner's own 3.9 disc: it has
**no Joliet descriptor**, and 7-Zip reads its mixed-case names through **Rock
Ridge**. So that disc carries a System Use area which ART is not reading —
which means the fix is testable against real material already on this machine,
not only against a synthetic fixture.

## What must be measured before designing

1. **Does the owner's `AmigaOS39.iso` actually carry `AS` entries**, or only
   POSIX Rock Ridge? Read the System Use area of a few directory records and
   say which signatures are present (`RR`, `NM`, `PX`, `AS`, …). If there is no
   `AS` on it, the fix needs a different disc to prove itself and that changes
   the plan.
2. **The entry's byte layout**, taken from an implementation rather than from
   prose — the flags byte, where the comment sits, and how a continuation is
   handled.
3. **What ART would do with a protection bit it reads.** `.uaem` already
   carries `HSPARWED` for files copied out of a volume; the question is whether
   a disc-sourced file should write one too, and what `distribution.json`
   records. That is a design question, not a research one, and it waits.

## Sources

- OSDev Wiki, *ISO 9660* — SUSP applications, listing Amiga `AS` entries —
  <https://wiki.osdev.org/ISO_9660>
- `ODFileSystem` — <https://github.com/reinauer/ODFileSystem>
- `AmiFUSE` — <https://github.com/reinauer/amifuse>
- amiga-news.de, ODFileSystem 0.6 and 0.7 release notes (the `AS` parsing claim)
