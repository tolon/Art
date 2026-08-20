# Installing on the Amiga side

**Date:** 2026-08-20
**Status:** approved (2026-08-20)
**Scope:** run a package's own installer inside the emulator, on a copy of the
tree, and read the result back
**Follows:** the content-layer round, which built the host-side placer and then
measured its ceiling three times in one day
**Research:** [2026-08-20-amiga-side-install-research.md](2026-08-20-amiga-side-install-research.md)

---

## Why this exists

ART places files from the host. That works for everything it can read, and the
content-layer round found the edge of it against the owner's own material:

- **BoingBag 39-1 and 39-2** carry ZipCrypto-encrypted payloads. The password
  lives in the package's own Amiga-side `Updater` ([ART-166](../../ISSUES.md)).
- **`Euro-Update` and packages like it** install through an Amiga Installer
  script — the thing `core/osinstall` was built instead of running.
- **AmigaOS 3.9's `First-Install`/`SetPatch` tree** is unplaced
  ([ART-159](../../ISSUES.md)), and two hazards the content-layer spec predicted
  were never exercised because no package file landed correctly
  ([ART-171](../../ISSUES.md), [ART-172](../../ISSUES.md)).

All four are the same shape: **work that belongs on the Amiga.** Every
established distribution builder already does it there — HstWB Installer's own
README says it *"uses WinUAE or FS-UAE emulator to run the installation
process"*, with the logic in AmigaDOS scripts, and that is precisely why it can
install BoingBags at all.

**No protection is bypassed.** The `Updater` that already holds the password
runs where it belongs. That was the owner's decision and it is not revisited
here.

## What is already built, and what is missing

Measured rather than assumed — three of the four parts ran this week:

| Part | State |
|---|---|
| Mount a host folder as an Amiga volume | `core::winuae::DirMount` → `filesystem2=rw,DH0:…`. The 3.9 tree booted from one. |
| Generate the config | `core::winuae::generate_uae_config`. The boot ran from ART's own output, not a hand-written `.uae`. |
| Launch and hold the process | `core::winuae::launch_winuae`, which returns a pid. |
| **Read the result back** | **Measured 2026-08-20 and it works live** — see below. |

**The measurement that shaped this design.** A copy of the real 3.9 tree was
given `Echo >SYS:probe-early.txt "early marker"` at the second line of its
`Startup-Sequence`, mounted through ART's own config and booted with the
owner's licensed Kickstart 3.1. The file appeared on the host **while WinUAE
was still running** — pid confirmed alive at that moment, so it is not a
post-exit flush.

Two consequences, and the second is why the design has the shape it has:

1. The host **polls**. It does not wait for the emulator to exit, and nothing
   needs the Amiga to be able to quit it.
2. A line appended *after* the `Startup-Sequence` **never runs** — the sequence
   ends with `LoadWB`/`EndCLI`. So whatever reports back must run **before**
   the hand-over to Workbench, or not be part of that sequence at all.

## 1. ART does not touch the user's Startup-Sequence

It would be fragile and, given (2) above, it would also be wrong: appending is
the one place that does not execute.

Instead the run gets a **second volume — ART's own work volume** — mounted
alongside the tree with the **highest boot priority**, so AmigaDOS boots it
first. It carries the script, runs the package's installer, writes a result,
and stops.

That mechanism is not new either. It is the same shape as the boot directory
behind "one click starts the game" (the collection round's Y2), where ART
already mounts a directory it wrote and gives it the highest `bootpri`
precisely so it is the device AmigaDOS boots from.

**The user's tree is mounted as data, not as the boot device.** Nothing ART
generates is written into it.

## 2. The original is never the thing being changed

The install runs against a **copy** of the distribution tree. The copy replaces
the original only when the result file says the run succeeded.

This is §92's rule — *never destroy the original before successful
validation* — applied at the granularity the operation actually has. A package
installer is an Amiga program ART did not write and cannot supervise per file,
so per-file backup is not available; a whole-tree copy is, and it is cheap: the
3.9 tree is 19 MB and rebuilds from media in about ten seconds anyway.

A run that fails leaves the original untouched and the copy in place for the
user to look at.

## 3. What runs, and what ART refuses to run

**What runs:** the installer the package's own recipe names. **The recipe
format gains one declaration for this** — the path, inside the package, of the
thing to run, and how to run it. For a BoingBag that is its `Updater`; for an
Installer-script package it is `Installer` with the script the package ships.
It is data, like every other thing a recipe says, so a fourth package is a
JSON file rather than a code path — and a package with no such declaration is
simply not one this round can run.

**A run always ends, and says which ending it was.** An Amiga Installer is
interactive by nature, so a run that stops on a requester would otherwise wait
for ever. Every run therefore carries a deadline, and when it expires ART
terminates the emulator it started and reports **timed out** — not failed, and
explicitly not succeeded. The distinction matters to a user: a failure means
the installer said no, a timeout means nobody was there to answer it, and the
second is fixed by watching the window rather than by changing anything. The
original is untouched either way.

**Where the package's own files come from — amended 2026-08-21.** The first
version of this section said what to run and never said where it runs *from*,
and the omission survived four tasks because each one was locally correct. The
run mounts the tree copy and ART's work volume; a BoingBag's `Updater` is in
neither, because being unable to place it on the host is the whole reason this
round exists.

So the run mounts a **third** volume: a host directory holding the package's
own wrapper, unpacked. The wrapper is plain LHA and ART already reads it; only
the payload inside is encrypted, and that stays encrypted — the `Updater`
decrypts it on the Amiga, which is the arrangement this design chose from the
start.

This is stated as its own rule because deferring it does not fail loudly. The
composed command would run, `CD` would fail, the shell would not find the
program, the script's `If Warn` would write `failed`, and ART would tell the
user **"the installer ran and said no"** about a program that never started.
A wrong sentence delivered confidently is worse than an error, and §89 forbids
exactly this.

**What ART refuses:**

- A package it ships no recipe for. This round does not make ART able to run
  anything a user points at — the boundary the content-layer round drew stays
  where it is, and the panel says so.
- Anything assembled from a string ART did not author. The command line comes
  from the recipe, which is shipped data, never from an archive's contents.
- A run with no licensed Kickstart. ART ships none and never will.

## 4. What the user sees

The same shape as every other data-changing operation in ART (§92): **preview →
confirm → job → report.**

- The **preview** says what will run, on which tree, and that it will run
  inside an emulator — a person should not be surprised by a machine window.
- The **job** reports progress. The emulator's own window is the honest
  progress indicator for the Amiga's part; ART's job reports the phases it
  controls.
- The **report** says what the result file said, what the tree looks like now,
  and — when it failed — that the original is untouched and where the copy is.

**Cancellation** stops between whole units: before the launch, or by
terminating the emulator ART started. A cancelled run discards the copy and
leaves the original alone.

## 5. How this is verified

The bar is the one the content-layer round set for itself and did not meet:
**a BoingBag'd tree must boot and show its update.** Not `Workbench 45.1` —
that is the 3.9 overlay, already proved — but the version BoingBag 2 produces.

And the method that found the last round's biggest defect applies here too:
**ask the running system, do not infer.** The boot check reads the version off
the mount rather than interrupting `Startup-Sequence` to reach a shell — a
healthy tree resists interruption by design, which is itself a thing this
project learned by measuring.

Closing this round closes [ART-166](../../ISSUES.md),
[ART-159](../../ISSUES.md), [ART-171](../../ISSUES.md) and
[ART-172](../../ISSUES.md) together, because all four were open for the same
reason.

## 6. What could make this harder than it looks

Stated now rather than discovered later:

- **An installer that asks a question.** §3's deadline is the answer, but not
  a complete one: the host sees only the absence of a result file, so it cannot
  tell a requester from slow work. Picking the deadline is therefore a
  judgement about real installers on a real machine, and it should be measured
  against the owner's own packages rather than chosen — a BoingBag's `Updater`
  on this hardware has a running time, and the deadline should be a multiple of
  it, recorded with what it was measured from.
- **The result file is written by a script ART generated but did not run.**
  Whatever writes it has to run even when the installer fails, or a failure and
  a hang look identical.
- **A package that reboots the Amiga.** Some installers do. The work volume has
  to survive a reboot and not re-run the installation on the second pass.
- **The emulator is a window on the owner's desktop.** This round runs it
  deliberately and should say so before it does. The last round opened it
  repeatedly without warning and that was a real annoyance.
- **`uae-configuration`** exists — WinUAE ships an Amiga-side program that
  changes emulator settings from inside the emulation, which is how WHDLoad's
  `ExecuteStartup` adjusts CPU speed. It is not needed here (ART owns the
  process and can terminate it), but it is the escape hatch if the Amiga side
  ever needs to talk back for something other than a file.

## 7. Deliberately not in this round

- **Running anything the user points at.** Only packages with recipes.
- **Building the distribution *inside* the emulator.** The host-side placer
  stays what it is: fast, inspectable, and correct for everything it can read.
  This is the fallback for what it cannot, chosen per package, the way
  `hst-imager` is a named fallback for two typed gaps in `core/preload`.
- **Writing a card.** Unchanged.
