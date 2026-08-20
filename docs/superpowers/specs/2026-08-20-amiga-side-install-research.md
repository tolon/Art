# Research note: what an Amiga-side install step would actually need

**Date:** 2026-08-20
**Status:** research only — no design, no decisions
**Written before any spec, deliberately**, under `CLAUDE.md`'s "Research
before design" rule. That rule exists because the content-layer round was
designed around a payload that turned out to be encrypted, and the AmigaOS
3.9 round shipped the wrong operating system — both from assertions made
without looking.

---

## Why this is being considered at all

ART places files from the host. That works for anything it can read, and it
has a ceiling that the content-layer round found three times in one day:

- **BoingBag 39-1 and 39-2** carry ZipCrypto-encrypted payloads; the password
  lives in the package's own Amiga-side `Updater` (ART-166).
- **`Euro-Update` and packages like it** install through an Amiga Installer
  script, which `core/osinstall` was built instead of running.
- **`türkçe` and names like it** are a known problem for *directory-based*
  distributions — Cloanto's own knowledge base says so, and AmiKit's answer is
  to carry those files in an LhA and unpack them **on the Amiga**.

## How the established projects do it — measured from their own words

**HstWB Installer** (`README.md`): *"HstWB Installer uses WinUAE or FS-UAE
emulator to run the installation process"*, with the logic *"written in
AmigaDOS scripts and uses binaries for 68000 CPU"*. It supports AmigaOS
3.1/3.1.4/3.2/3.9 **and** `BoingBag39-1.lha` / `BoingBag39-2.lha` — not by
decrypting anything, but by running the package's own installer where the
password already is.

Its mechanism, from the self-install tutorial:

- **Host folders are mounted as Amiga volumes with standardised labels** —
  `WORKBENCHDIR`, `KICKSTARTDIR`, `OS39DIR`, `USERPACKAGESDIR`. The scripts
  find their media by **label**, not by path.
- **Media is recognised by filename** inside those mounts (`AmigaOS3.9.iso`,
  `BoingBag39-1.lha`), case-insensitively.
- A Kickstart 3.1 or newer ROM is a prerequisite; the user supplies it.
- After configuration, the run proceeds unattended from a reset.

**AmiKit / AmigaSYS / ClassicWB** are the same family: none ships the OS, all
copy it from the user's own source during a one-time setup phase on the Amiga
side (RetroPlatform KB 19-106).

## What ART already has — verified today, not assumed

| Piece | State |
|---|---|
| Mount a host folder as an Amiga volume | `core::winuae::DirMount` → `filesystem2=rw,DH0:Workbench:<path>,0`. **Used today**; the 3.9 tree booted from one. |
| Generate the config | `core::winuae::generate_uae_config`. **Used today**; the boot ran from ART's own output, not a hand-written `.uae`. |
| Launch and get a pid | `core::winuae::launch_winuae`. **Used today.** |
| A licensed Kickstart | The owner's `amiga-os-310-a1200.rom` (V40). **Used today.** |
| The Amiga writing to a mounted host folder | The mount is `rw` and the 3.9 tree's own `Startup-Sequence` wrote into `RAM:`; a package installer would write into the mount. **Not yet measured from the host side.** |

So the loop is assembled almost entirely from parts that ran today:

1. Mount the tree as the system volume and the package folder as a second
   volume, both by label.
2. Put a small AmigaDOS script in the tree's `S:` that runs the package's own
   installer and then writes a result file.
3. Boot with the licensed ROM.
4. Read the result file from the host, then terminate the process ART started.

## The one unknown — measured 2026-08-20, and the answer is the good one

**Does a file the Amiga writes into a `filesystem2=` directory mount appear on
the host promptly, or only when the emulator exits?** Step 4 rested entirely on
this, and nothing before today had touched it: the 3.9 tree was read *after*
WinUAE closed, which is consistent with either answer.

**Measured: the write is visible live.** A copy of the real 3.9 tree was given
a `Startup-Sequence` with `Echo >SYS:probe-early.txt "early marker"` inserted
at its second line, mounted through the config ART itself generates
(`filesystem2=rw,DH0:Workbench:…,0`) and booted with the owner's licensed
Kickstart 3.1. The host watched the folder while it ran: the file appeared
with its 13 bytes, and **WinUAE was confirmed still running at that moment**
(pid alive), so this is not a post-exit flush. The probe tree and its config
were removed afterwards.

So the host can **poll** rather than wait for exit, and step 4 keeps its shape:
the Amiga writes a result file, the host reads it while the machine is still
up, and ART terminates the process it started. Nothing here needs the Amiga to
be able to quit the emulator.

**One thing the probe also showed, worth carrying into the design:** a line
appended *after* the Startup-Sequence's own end never ran — the sequence
finishes with `LoadWB`/`EndCLI`. So a script that must report back has to run
**before** the sequence hands over to Workbench, or be started as its own
thing. Where the result is written from is a design decision, not a free
choice.

## A second mechanism worth knowing, not yet needed

**`uae-configuration`** is a real Amiga-side program shipped in WinUAE's own
"Amiga programs" folder: it changes WinUAE settings **from inside the
emulation**, and is how WHDLoad's `ExecuteStartup` / `ExecuteCleanup` adjust
CPU speed and cache. So the Amiga side can talk back to the emulator. Whether
it can make WinUAE *quit* is unconfirmed — and ART may not need it, since ART
launches the process and can terminate it.

## What is deliberately not decided here

Whether ART grows this step at all, what it would run, how a failure is
reported, and whether the host-side placer keeps its place beside it. Those
belong to a design conversation with the owner, after the measurement above.

## Sources

- HstWB Installer — <https://github.com/henrikstengaard/hstwb-installer>
  (`README.md`, and the wiki's *Prepare self install tutorial*)
- RetroPlatform KB 19-106, *Self-Installing Packages* —
  <https://www.retroplatform.com/kb/19-106>
- `uae-configuration` usage — <http://guide.abime.net/wb3.1/miscwhd.htm>
- amitools `xdftool`, the host-side family ART belongs to —
  <https://amitools.readthedocs.io/en/latest/tools/xdftool.html>
