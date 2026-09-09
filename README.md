# Amiga Retro Toolkit (ART)

[![Latest release](https://img.shields.io/github/v/release/tolon/Art?label=download&color=success)](https://github.com/tolon/Art/releases/latest)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
![Platform: Windows 10/11 x64](https://img.shields.io/badge/platform-Windows%2010%2F11%20x64-informational)
![Rust 1.93+](https://img.shields.io/badge/rust-1.93%2B-orange)

**The Swiss Army Knife for Amiga Files**

<https://github.com/tolon/Art>

A professional Windows desktop toolkit for Commodore Amiga users. ART combines
ADF, HDF, LHA, ROM, Gotek, WinUAE and collection management into one coherent,
drag-and-drop-driven application.

> **DROP IT INTO ART.**

**0.9.1 is out, and it is asking for testers.**
[Download it](https://github.com/tolon/Art/releases/latest), try it on your own
Amiga files, and tell it what it got wrong — the thirteen things that still need
someone other than the author are listed under
[What still needs testing](#what-still-needs-testing), each with what to run
and what a good or a bad result looks like. **A result that went well is worth
reporting too.** ART is written for real Amigas rather than for emulation, and
the one thing nobody has done yet is flash a card it built and start an A500
with it.

![The two-pane file manager: a folder of ADF images on the left, a Windows drive on the right](docs/assets/files.png)

*A Windows folder of 29 `.adf` images on the left, the root of a Windows drive on
the right — either side could just as easily be an Amiga volume, a disc or an
archive. Name, extension, size, date and attributes in both panes, the F3–F9
function bar and a command line under them. Everything starts by dropping
something on ART: it works out what a file is from its bytes, not its name, and
offers what can be done with it.*

**It looks like a Windows 11 application now, in both themes.** The colours are
the Fluent tokens Windows itself draws with — the Mica background, the layer and
card fills, the system accent — controls carry Fluent's darker bottom edge, text
boxes show focus with an accent underline, and the sidebar is a WinUI navigation
pane with drawn icons instead of emoji. The Files screen keeps its Total
Commander look on purpose, because that is what that screen is. Application Size
is unchanged and so is everything you had set. Where a Fluent value did not meet
the contrast rule it was moved one step along Fluent's own ramp and the measured
ratio recorded beside it: every colour pair in both themes is still checked
against WCAG in CI, **105 of them**, counted 2026-09-09.

## What it looks like

**Both panes, and anything can be a pane.** A Total Commander-style dual pane in
which Amiga volumes, CD images and archives open beside your Windows drives, and
you copy either way — the picture at the top of this page is that screen.

**The cursor keys work now.** Up/Down, Home/End and Page Up/Page Down move the
pane's cursor, holding Shift while moving marks every row it passes over, and
Ctrl+Space focuses the command line ([ART-275](docs/ISSUES.md#fixed)). No
version of ART before 0.9.1 handled any of them, so a mouse-free session was
Insert and letters only.

**Your library, with covers.** Point ART at the folders your games live in. It
reads each title from whatever *states* it — a WHDLoad slave's own header, an
`.rp9` manifest, the filename last — and marks anything it had to guess. Cover
art is fetched from sources you choose, and nothing reaches the network until
you ask it to.

![The Collection: 2815 titles across three folders, shown as a grid with cover art](docs/assets/collection.png)

*The two `1869` AGA cards in the top row are the point, and they are a real pair
rather than a staged one. Both are AGA, both ask for Kickstart `40068.a1200`,
and both say so because the WHDLoad slave inside them says so. But one of the
two had its **name** taken off its filename — somebody had renamed the file —
so ART marks it `~guessed` while its neighbour, named by the slave itself,
carries no mark. A guess and a statement look identical once they are on a
screen, and ART's answer is to never let them.*

**Play.** The Collection's detail panel has a Play button that hands a
catalogued title straight to WinUAE. ART picks a Kickstart from your ROM
folder, shows the machine, the ROM and the memory on a confirmation screen
before anything starts, and **refuses rather than launches** when no ROM in
your folder actually suits the title. Which machine depends on the shape. A
floppy or a plain hardfile is planned from the title's own chipset. **A
WHDLoad title is not**: it runs on one known-good profile — **A1200, AGA,
68EC020, 2 MB Chip, 8 MB Fast, Kickstart 3.x** — because a WHDLoad title does
not boot the game, it boots AmigaDOS, which starts WHDLoad, which patches the
game. Sizing that machine from a 1988 game's chipset means as many launch
configurations as there are titles, each of which has to be right on its own.
This was a reasoned decision when it landed and it has since been measured:
`1000 Miglia` (Simulmondo, 1992), an OCS-era self-booting WHDLoad hardfile
this project had already played on the old A500/OCS path, was played again
from the Collection afterwards and ran — on the default setting, where
Automatic resolves to the A1200 profile, so nobody hand-picked the machine.
One OCS-era title running is the claim, not every OCS title you own. Your own
per-title machine choice still outranks the profile, and the Fast RAM
figure is a default rather than a floor — set it lower, including to zero, and
that is what the generated configuration says. A floppy set
— plain `.adf` or `.rp9` — mounts and boots directly; a self-booting WHDLoad
hardfile (most WHDLoad titles in a real collection) mounts the same way, with
an off-by-default switch to make it writable so saves survive; a WHDLoad
*drawer* that needs a separate system image is handed that system read-only,
or, when the system supports it, boots straight into the game in one step.
Two shapes have run for real, from this project's own collection: a
self-booting WHDLoad hardfile (`1000 Miglia`, one of 1697 catalogued the same
way) reached the game, and an `.rp9`-packaged floppy title (`3D Demo`)
launched with its two floppies extracted and mounted in order. That is two
proven shapes, not all of them — see
[What still needs testing](#what-still-needs-testing) for exactly what is
still unverified and how to try it.

*(A screenshot of the Play confirmation screen — machine, Kickstart, memory,
shown before anything starts — would fit here; none exists yet.)*

**A PiStorm card, described in the words its own documentation uses.** Every
control writes a documented Emu68 option or a Raspberry Pi firmware setting —
and tells you which one. Both files are merged into what is already on the card,
never rewritten over the top.

![The PiStorm screen: hardware, an empty card folder, Kickstart and ready-made settings](docs/assets/pistorm.png)

*An A500 with a PiStorm and a Pi 3A, which is what fixes the Emu68 build
(`Emu68-pistorm.zip`), the storage driver (`brcm-sdhc.device`) and the Pi's
memory. No card folder has been chosen here, so this is the screen as you meet
it. Each ready-made profile lists the Emu68 options it would write, in full.*

**A Gotek, including what its little screen will say.** The OLED preview is
live: it renders the same text the hardware will, before you write anything to
the stick.

![The Gotek screen: an OLED simulator showing DF0: with no disk loaded, beside the FlashFloppy settings](docs/assets/gotek.png)

*The simulated OLED reads `DF0:` and `(No disk loaded)`, because nothing has
been assigned yet — the quickslot rack under it is empty and says so. The
`FF.CFG` settings on the right are the ones this preview is drawn from.*

**And when you need to see the actual bytes.** A hex and sector inspector that
knows where a volume's boot block, root block and bitmap are, and will jump to
any of them.

![The hex inspector before a file has been chosen](docs/assets/tools.png)

*Nothing opens until you choose one, so this is the whole screen on arrival: one
button, and the sentence saying what it will take.*

## Which machines

**The whole classic line, not one model.** ART's file work is machine-
independent to begin with — an ADF is an ADF whether it came off an A500 or an
A4000, and FFS/OFS, RDB/HDF, LHA and ISO9660 are formats rather than machines.
Where the machine *does* matter, it is data ART carries rather than a code path
it hard-codes: built-in machine profiles ship for **A1000, A500, A500+, A600,
A2000, A3000, A1200, A4000, CDTV and CD32**. Kickstart identification,
WinUAE configuration and the compatibility check all read those profiles.
They are presets: profiles you define yourself are spec §33 and are not built
yet.

Commodore's 8-bit side is in scope too, and built: **C64 disk and tape
images** (`.d64`, `.d71`, `.d81`, `.t64`) open in the same commander and copy
out to a folder, read-only. `.tap`, `.prg` and `.crt` are identified and
described rather than browsed — a TAP is a sampled tape signal with no
directory in it.

## Status

**0.9.1**, measured on the release branch on **2026-09-09**: **3181** Rust tests and
**1386** frontend tests passing (the Rust suite run twice), **2169** interface strings in
each language, **9** open defects — ART-062, 117, 118, 250, 278, 279, 281, 283 and 285.
Every one of those numbers, and the command that produced it, is in
[docs/STATUS.md](docs/STATUS.md) — count them there rather than trusting this paragraph.

The application builds and runs on Windows 10/11 x64. Working today: DD/HD
floppy images and hard-disk (RDB/HDF) partitions — read, write, create and
validate through one volume driver, including boot code that starts a real
Amiga (verified by booting a disk on an actual A500/A500+ — see below) — with
a Total Commander-style dual pane (browse, multi-select,
batch copy in/out/delete, sort, filter by filename mask, rename, mkdir,
attributes) over FFS/OFS volumes; **CD images (ISO9660 with Joliet, including
raw 2352-byte tracks in Mode 1 and Mode 2/XA)** and **archives (LHA, ZIP, 7z)**
opened as panes of the same manager, walked into and copied out of — to a
folder or straight into an Amiga volume; LHA WHDLoad detection with several
archives installed to a disk at once; Kickstart ROM identification; machine
profiles for the whole classic line; Gotek/FlashFloppy; PiStorm/Emu68; WinUAE
launching; **AmigaOS installed from your own media** into a distribution tree,
with the AmigaOS 3.9 update chain — BoingBag 1 and BoingBag 2 included — placed
from Windows; **Aminet browsed, downloaded and installed** from a
catalogue held locally; a background job queue with progress/cancel;
an operation log; Beginner/Power User modes; and the drag-and-drop Workflow
Engine behind "what can I do with this?".

Every screen scales with Ctrl +/-/0 and every colour pair in both themes is
**measured against WCAG in CI** rather than judged by eye — most of the people
this is for are over fifty, and contrast is not decoration.

**A collection you can keep.** ART indexes your folders once and remembers:
a library that takes minutes to read is there the moment the screen opens, and
an Update re-reads only what changed. Each title's facts come from whatever
*states* them — a WHDLoad slave's own header, an `.rp9` manifest, a filename
last — and the screen marks anything guessed rather than read. Cover art is
fetched from sources listed in Settings, which you can switch off or point
elsewhere; **nothing reaches the network until you ask it to.** Where a name
could only be taken off a filename, ART proposes a tidier one and you accept it
— it will not rename anything by itself, and where the evidence runs out it
says nothing and leaves you the edit box. Measured against a real 2815-title
library across three folders.

**Software from Aminet, without a browser.** ART syncs Aminet's own index —
**85 472 packages**, measured 2026-08-24 — and keeps it locally, so search and
browse work with no connection at all. A package downloads to a folder you
choose, is checked against the size the index claimed and its own SHA-256, and
can be installed straight into a floppy image or a hard-disk partition. An
update view compares what you have downloaded against the catalogue as it
stands now. There is no address bar: every request is built from a **configured
mirror** plus a validated repository path, so there is no code path anywhere in
ART that fetches a URL somebody typed. The mirror list is yours to edit and
reorder, and it is remembered. Nothing leaves the machine until you ask it to.


**PiStorm cards, both directions.** ART opens a real one — an MBR with a FAT32
boot partition and one to three Amiga disks inside it, each carrying its own
partition table at a byte offset inside the card — and shows it as the list of
disks the m68k side actually sees, verified against CaffeineOS and MultibootOS.
It can also **build one**: the partition table, a FAT32 boot partition carrying
your own Emu68 release and Kickstart, and a partition table at the start of
every Amiga disk. **PFS3 and FFS volumes are formatted and filled by ART
itself** — no external tool required, though one can be configured as a
fallback for two named gaps.

The card can carry **more than one complete AmigaOS** — 3.1 for compatibility
beside 3.2 for daily use, say. ART writes no boot menu, because AmigaOS already
has one: hold both mouse buttons while the machine starts and its own Early
Startup screen lists every bootable partition. What ART sets is which one starts
when nobody holds anything, and if two of them claim that equally it **says so
rather than choosing between your systems**. It also refuses to build an FFS
partition larger than the 4 GB a Kickstart before v46 can address — that one is
a partition which corrupts the drive, not an inconvenience. And it can put your
WiFi details on the card while you are setting it up, so the Amiga is on the
network the first time it boots; the passphrase is the one thing ART
deliberately does not remember.

One limit, said here rather than discovered later:
**no card ART built has been flashed or booted.**

**AmigaOS, built from your own install media.** Point ART at your own AmigaOS
floppies or CD and it produces a **distribution tree** — a Windows folder that
is the finished system volume file for file, with an Amiga-metadata `.uaem`
sidecar beside each one and a `distribution.json` recording which component and
which disc every file came from. It does not copy disks; a component is a named
set of paths, because `ModulesA1200_3.2.adf` holds fourteen commands and
thirteen of them are *older* than the ones `Workbench3.2` already carries.
Releases are data, not code: AmigaOS 3.2 and 3.9 each ship as a JSON recipe.
Both have been run against the owner's own media — the 36-disk 3.2 set and a
469 MiB `AmigaOS39.iso` — and both trees boot to a clean Workbench under WinUAE
with a licensed ROM. The 3.2 tree also boots off a PFS3 volume ART formatted
and filled itself.

**You say where your files are once.** One list of folders, however many you
keep your material in. ART then resolves every artefact the release needs into a
named slot and tells you, artefact by artefact, whether it is there and **how it
knows** — identified by its bytes against a table that now carries the 3.9 CD
and every update archive; identified by the medium's own name; or only taken
from a filename you pointed at, which is said in those words and is never
counted as a match. Where the file came from, and whether that piece is already
in your tree, sit on the same row. Required and optional are counted apart, so a
set short of nothing but an optional file still reads as ready to build.

![The OS Builder's install-media step: three material folders and a ten-row readout](docs/assets/material.png)

*AmigaOS 3.9 over three material folders, with the set line above the rows —
"10 of 10 found · everything required is here". `Locale3_9.lha` is "identified
by its bytes: they are the ones ART's table records for AmigaOS 3.9 Locale
update", while `BoingBag39-1 (1).lha` is "the file you chose for BoingBag 3.9-1.
ART has not checked that it is one" — two sentences because they are two
different amounts of evidence, and only one of them is a match. Three rows are
already in this tree and say so; `Kickstart 40` is required and is not a file
for these folders at all, so its row names the one chosen in ART instead. The
buttons at the bottom write a plain-text guide to what belongs in each folder,
into that folder, when you ask for one.*

**The AmigaOS 3.9 updates are one screen, in the order the material goes on.**
Nine links — the CD, BoingBag 1, BoingBag 2, the Locale 3.9 update and its
Turkish slice, the Turkish catalogs from BoingBag 2, the Contribution drawer,
Euro-Update, and the community BoingBags 3&4 — each carrying the one sentence
that is true of it: it is already in this tree; it is ready, and here is the
file; it is waiting for a named row; its file is not in the folders you named;
you do not need it because something else already contains it; or ART will not
run it, and here is why. Those never collapse into "not done", because they need
different next steps.

![The AmigaOS 3.9 update chain: nine rows, each with its own sentence and its own badge](docs/assets/chain.png)

*"AmigaOS 3.9 updates — 3 of 9 applied", over a real tree. Row 2 reads "ready:
BoingBag39-1 (1).lha"; row 3 reads "BoingBag 3.9-2 — BoingBag 3.9-1 has to go on
first", which is the chain refusal saying itself before anything is copied.
Three rows are "already in this tree", which is where the 3 comes from. Row 7,
Euro-Update, says in the row why ART will not place it — its own installer
rebuilds the `.font` index afterwards and ART cannot, so placing it would quietly
drop any font size the package does not itself ship — and row 8, BoingBags 3&4,
says nobody has measured whether its Installer script finishes without somebody
at the window. Six rows carry a "placed from Windows" badge and one "runs on the
Amiga". The two rows numbered 4 are two links at the same depth of the chain,
not a numbering slip.*

**BoingBag 1 and BoingBag 2 are placed from Windows, in seconds.** An AmigaOS
3.9 update package carries its files in a password-encrypted archive, and the
password is published in the MIT source of the two projects that already do this
job — Emu68 Hatcher and Emu68-Imager. ART uses it: it opens the payload, copies
the same set of paths those installers copy, and then does the fix-ups the
package's own `Updater` leaves undone — ten protection bits, the
`AmigaOS ROM Update.BB39-2` promotion without which BoingBag 2's ROM update is
silently inert, and the `.BB39-2` renames — plus BoingBag 2's XAD update when
your tree's `xadmaster.library` is older than 10, measured 9.1 → 10.0 on the
owner's own material. No emulator, no Kickstart for that step, nothing to watch.

**How that was checked.** The tree ART places was compared file for file against
the tree two real emulator `Updater` runs produced on the owner's own material:
**4 030 files expected, 4 031 produced — 2 added, 1 missing, 2 differing**, and
**every path the `Updater` wrote hashes identically**, including the five it
renames by case, which were checked by name on disk and not only by hash. All
five differences are accounted for, and four of them are one deliberate step:
ART renames BoingBag 2's own `Devs/NSDPatch.cfg.BB39-2` into place — which
Emu68 Hatcher does and the `Updater` does not — keeping your file beside it as
`NSDPatch.cfg.old`, with its `.uaem` sidecar travelling along. The fifth is
ART's own `distribution.json`, the record of the build. That comparison is a
test rather than a story about one afternoon, and it is what found the round's
one real defect: both recipes claimed to override only half of the Workbench the
tree's manifest records, so the placement was **refused** rather than writing
over 122 files it had not declared it may replace.

**The Amiga-side route is still there, and nothing ART ships drives it today.**
The engine that boots a copy of your tree under WinUAE and lets a package
install itself — three volumes, one generated `Startup-Sequence`, a deadline,
four distinct endings, and the copy promoted over your tree only on success — is
unchanged, and it is what installed both BoingBags before this. But those two no
longer need it, and the only recipe that still declares an Amiga-side installer,
BoingBags 3&4, is marked as never having been run: ART refuses it before it
composes anything rather than starting an emulator on a script nobody has
watched finish. The engine is present, tested and idle, and
[docs/FEATURES.md](docs/FEATURES.md) marks that row amber rather than green.

**The chain refusal stays, and it is the whole protection.** Clean 3.9, then
BoingBag 1, then BoingBag 2. A run whose prerequisite is missing is refused
**before anything is copied**: measured at 17.6 ms on the emulator route's own
control run, with nothing copied and no emulator started.

**What the measurements said.** Two arms of one controlled experiment, run on
the owner's own material, and they are worth more of a reader's trust than any
feature sentence above them.

A BoingBag with **one byte flipped** in its payload does not fail. The `Updater`
hangs on it: two runs, both stopped on the same payload entry, both ended by
ART's own 30-minute deadline, and each had written around 170 MB of garbage into
the staged copy by then. The copy was discarded and the original tree was never
touched, which is what the copy is for — but ART cannot tell "hung on a damaged
archive" from "waiting for an answer nobody gave it", so the ending it shows
names one cause for a state that has two. Both halves of that are filed rather
than papered over: [ART-278](docs/ISSUES.md#open), because nothing bounds what a
hung installer writes into the copy, and [ART-279](docs/ISSUES.md#open), because
the timeout's next step is the wrong advice for a corrupt archive.

BoingBag 2 applied to a tree BoingBag 1 never touched **reports success** — and
moves the version string to `45.3`, the right answer — while **57 files are
missing** and 51 more hold the wrong bytes. So neither the installer's own word
nor the version string is evidence, and ART treats neither as evidence. The
chain refusal above is what actually protects you, and that is now a measurement
rather than a piece of reasoning about what the package would presumably do.

One boundary, said plainly: **the panel for these screens has never been driven
by a person.** Every run quoted here — the emulator installs and the host
placement's oracle alike — went through a test hook rather than the screen.

**And the Amiga can finish its own setup, the first time it starts.** Turn the
first-boot step on and the tree ART builds carries a small AmigaDOS program that
runs itself on that machine's first boot: a dispatcher that works out what it is
running on — a real PiStorm board or an emulator, and which Kickstart — then runs
a short, fixed list of steps (`10-hardware`, `20-aux`, `30-datatypes`), skips the
ones that do not apply to this machine rather than failing them, writes what it
did to `S/FirstBoot.log`, and removes itself when it is done. ART reads that
report back **off the card** afterwards — from the Amiga volume's own copy where
there is one, and the FAT partition's secondary copy otherwise, across FAT32, FFS
and PFS3 — so the card-preparation screen can say whether a card has been booted
yet and what happened when it was.

It was proved by **rehearsal under WinUAE against the owner's own AmigaOS 3.2
tree**: every step ran, the boot finished in 18 s, the step directory was gone
afterwards, and the refusal path was measured separately. That one rehearsal
found two real defects that seven tasks of green tests had not — a bare `$name`
does not expand in an AmigaDOS script ([ART-272](docs/ISSUES.md#fixed)), and a
script cannot delete itself while AmigaDOS is executing it
([ART-273](docs/ISSUES.md#fixed)).

**What is not proven, said here rather than discovered later:** this is phases 1
and 2 of four. The **Pi3/Pi4 branch of `10-hardware`** — `fat95`, `SD0:`,
`EMU68BOOT:` — has never run, because under an emulator it writes `skipped uae`
and measuring it needs a real card. The reboot itself and the package steps
(phases 3 and 4) are not built. And the FFS and PFS3 read-back paths have so far
only read volumes ART itself wrote, which proves the code runs, not that the
bytes match what a real Amiga's own dispatcher writes.

**Content-first detection**: what a file *is* comes from its bytes, not its
name, so an `.img` holding a floppy is a floppy and a `.dat` holding an LHA
still opens.

**Application Size** (Ctrl +/-/0, or Settings) scales the whole interface from
70 % to 250 % and is remembered — as is every other choice ART offers. A
right-hand-edge complaint at 130 % ([ART-099](docs/ISSUES.md#fixed)) did not
reproduce when the running application was measured across seven screens and
three sizes; what was real — content being clipped with no way to scroll to it
— is fixed.

### Verified how, exactly

ART's disk writer has been checked four ways, and the last one is the one that
matters:

1. `cargo test` — ART agrees with itself.
2. `amitools` and 7-Zip — ART agrees with implementations that share no code
   with it, in both directions.
3. **Two disks ART wrote, opened under licensed Kickstart and Workbench in
   WinUAE / Amiga Forever** — one mounted and read back, one booted to a CLI
   prompt.
4. **A real Amiga.** On **2026-08-12**, `test/art-bootable-test.adf` cold-booted
   an **A500 / A500+** running **Kickstart 3.9** — served from a **Gotek** as
   `DF0:` — straight to an AmigaDOS `1>` prompt.

Rung four is what rung three could not be: the boot code is ART's own,
assembled from the published LVO table, and running it on a real **68000**
(the emulated passes were an A1200's 68020 and an A500+ *configuration*) was
an assumption until then.

**What is still not claimed:** a Gotek is not a mechanical drive. Nothing ART
has written has been through a real floppy head onto physical magnetic media.
That rung is listed by name in [`test/README.md`](test/README.md) and is not
being quietly folded into the one above it — claiming hardware ART has not
been tried on is the one thing [docs/FEATURES.md](docs/FEATURES.md) exists to
prevent.

### What still needs testing

The owner has decided the remaining verification happens by the community,
not alone. This list is kept honest in both directions, so start with what has
since stopped being a gap:

**Proven since this section was written.** The shipped release build has been
driven by a person rather than only by `pnpm tauri dev`, and it immediately
found two defects nothing in 2260 tests had ([ART-195](docs/ISSUES.md#fixed),
[ART-196](docs/ISSUES.md#fixed)) — which is the argument for this whole section.
Titles have been played from the Collection panel by hand: an `.rp9` floppy
title (`3D Demo`), and — on 2026-08-21, through the new A1200 WHDLoad profile —
`Akira` (AGA) and `1000 Miglia` (OCS-era), both of which ran. `1000 Miglia` is
the one that settles something, because this project had already played it on
the *old* A500/OCS path: same file, same ROM folder, only the machine profile
changed, and it was left on Automatic rather than hand-picked. **One OCS-era
title on the A1200 profile is the claim** — not every OCS title in a
collection. And a **Turkish** sentence has finally
been read on a running screen by someone who speaks it: the new WHDLoad
Kickstart refusal, judged clear, and the launch then worked
([ART-062](docs/ISSUES.md#open)). That is one sentence out of 2169 (counted
2026-09-09), so the language as a whole is still unseen — but it is no longer
zero.

### How to report what you find

**[Open an issue](https://github.com/tolon/Art/issues/new)** — a result that
went *well* is as useful as one that did not, because the gaps below are
unverified in both directions.

Four things make a report actionable, and the third is the one people leave
out:

1. **What you did, what you expected, what happened instead.** One sentence
   each is enough.
2. **Your ART version** — Settings → About shows it, taken from the build
   itself so it cannot disagree with the installer — and your Windows version.
3. **The sentence ART showed you, copied rather than described.** ART's
   refusals carry an `ART-NNN` id on purpose; that id says exactly which check
   fired, and *"it said something about a Kickstart"* does not.
4. **`operations.jsonl`**, if a file was involved — it is in
   `%LOCALAPPDATA%\com.amiga-retro-toolkit.desktop\logs\` and it records every
   operation ART performed, whether it succeeded, and what it backed up. Paste
   the last few lines. `art.log` beside it has the technical detail.

**Nothing in either file is sent anywhere by ART** — you attach them or you
do not. And if something went wrong with your own Amiga files, say so first:
ART backs up before it changes anything, so the backup path is in that log and
recovering it comes before diagnosing anything.

**Still open, and each one is a concrete gap rather than a vague "try it and
see"** — what to run, and what a good or bad result looks like:

1. **A bare `.adf` floppy set (no `.rp9` wrapper).** Only an `.rp9`-packaged
   floppy set has actually launched. Catalogue a folder holding a plain
   multi-disk `.adf` title, open its detail panel and press Play. **Good:**
   WinUAE starts with the disks mounted in the order the panel shows, and
   boots. **Bad:** the wrong disk order, a missing disk, or a refusal that
   should not have happened.
2. **An `.rp9`-packaged *hardfile* title.** 102 of these are in this user's
   collection and none has run. This is the shape [ART-141](docs/ISSUES.md)
   was found and fixed in review, but the fix has never been exercised
   against a real launch. Play one. **Good:** ART extracts the hardfile entry
   named inside the package (never the `.rp9` zip itself) and mounts it.
   **Bad:** WinUAE opens the wrong file, or reports it cannot be mounted.
3. **The WHDLoad *drawer*-plus-system Y1/Y2 path.** Nothing in this
   collection produces this shape — an unpacked WHDLoad pack paired with a
   separate bootable system image — so only its own unit tests have ever
   exercised it. Configure a bootable system image in Settings → Play, then
   Play a drawer-based (not hardfile) WHDLoad title. **Good:** Y1 mounts your
   system read-only and hands control over, or Y2 (where supported) boots
   straight to the game. **Bad:** a bare AmigaDOS CLI prompt instead of the
   game — this exact failure is how [ART-145](docs/ISSUES.md),
   [ART-147](docs/ISSUES.md), [ART-148](docs/ISSUES.md) and
   [ART-149](docs/ISSUES.md) were each found — or your original system image
   being modified, which it must never be.
4. **Any VHD- or RDB-container system image.** [ART-146](docs/ISSUES.md)
   stopped ART forcing bare-image geometry onto every hardfile, VHD and RDB
   containers included; the fix reads the file's own bytes (`conectix`,
   `RDSK`) instead of assuming one shape for all of them, but has **not**
   been retried against a real emulator run — the one real run since
   (`1000 Miglia`) is a bare `DOS\1` image, the branch ART-146 left
   unchanged. **Half of this closed on 2026-08-24** and the half that is
   left is the half that needs a person: ART now reads the real 1.2 GB
   `AmiKit.hdf` itself rather than a fixture built from its first eight
   bytes, and two independent readers agree about it - it is a *dynamic*
   VHD carrying 3.9 GB of disk, its footer checksum matches, and the line
   ART writes for it has the empty device name and zeroed geometry the fix
   was about. What that measures is the configuration ART writes, **not
   what WinUAE does with it.** So: launch a title or a WHDLoad system (via Y1)
   backed by a VHD container (e.g. an AmiKit-style `.hdf`) or a plain RDB
   hardfile. **Good:** WinUAE mounts and reads it normally. **Bad:** "Not a
   DOS disk in unit 0" — the exact error ART-146 was filed against.
5. **Whether a WHDLoad save survives with the writable switch on.** Measured,
   not proven: two titles were played with *allow writes* on, the emulator
   window was closed rather than quit through WHDLoad's own key, and the
   images were read back afterward — same byte counts and timestamps as
   before, so the mount really is read-write (the generated configuration
   says `rw`) but nothing wrote in that session. The likely explanation is
   that a WHDLoad game writes its save or high-score table on a clean exit,
   not while running, and this session never gave it one. Play a title with
   *allow writes* on, exit through WHDLoad's own quit key rather than closing
   the window, then reopen and check for the save. **Good:** the save is
   there. **Bad:** still nothing after a clean exit, which would mean the
   switch does not do what it claims.

6. **The Amiga-side install engine, driven by a person.** This item has gone
   backwards on purpose, and the honest state is the point of writing it down.
   The engine that runs a package's own installer on the Amiga installed both
   BoingBags, and every one of those runs went through its own test hook rather
   than the panel. Those two packages are now placed from Windows and their
   Amiga-side declarations went with the route, so **no package ART ships today
   reaches that engine at all.** The one recipe that still declares an installer
   is BoingBags 3&4, which says nobody has measured whether its Installer script
   finishes unattended, and which ART refuses before composing anything rather
   than starting an emulator on it. There is nothing here to run yet: the item
   stays open and un-ticked until BoingBags 3&4 is measured.
7. **The Aminet studio, driven by a person.** The whole chain has been run
   against the real Aminet and it works — each shipped mirror asked
   separately, the index synced and reloaded, and a real package downloaded,
   checked and unpacked (`cargo test live_aminet -- --ignored`). What has never
   happened is somebody pressing the buttons in the running application. Sync,
   search for something you want, download it, and install it into a floppy
   image. **Good:** the catalogue fills, the download lands where you chose it
   to, and the install names what it put where. **Bad:** a progress bar with no
   total behind it, or a mirror failure that does not say which mirror.
8. **A card with two AmigaOS environments on it.** ART will build one, and the
   Amiga's own Early Startup screen is what chooses between them — ART writes
   no menu. Build a card with a second system, then hold both mouse buttons at
   power-on. **Good:** both systems are listed, and the one ART gave the higher
   priority is what starts when you hold nothing. **Bad:** only one appears, or
   the wrong one starts — and if ART warned you the two were tied, that warning
   is the thing to report on.
9. **A PiStorm card ART built, flashed and booted.** Everything on the code
   side is finished, tested, and cross-checked against 7-Zip and `hst-imager`;
   what is missing is a microSD card and a Pi. Build a card image, write it
   with whatever card writer you already use (ART deliberately does not write
   to raw devices), fit it and power on with HDMI attached. **Good:** Emu68
   starts and the Amiga volumes ask to be formatted or mount. **Bad:**
   anything else — and the image health check's report is the first thing to
   send. **This is the project's 1.0 bar**, not a bigger version number.
10. **First boot on a real machine, and the report read back off the card.**
    The first-boot program has only ever run under WinUAE, where the whole
    Pi3/Pi4 branch of its hardware step writes `skipped uae` and is therefore
    unmeasured. Build a tree with the first-boot step turned on, put it on a
    card, boot the Amiga, then bring the card back to ART and open the
    card-preparation screen. **Good:** the machine sets itself up unattended,
    the screen shows the same report the Amiga wrote — with the hardware step
    reporting a real board rather than `skipped uae` — and the step directory is
    gone from the tree. **Bad:** a step that refuses on real hardware where it
    passed under emulation, a report ART cannot read back off FFS or PFS3, or a
    first boot that leaves its own machinery behind on the volume.
11. **The material readout, over your own folders.** Open the OS Builder, choose
    AmigaOS 3.9 or 3.2, and add the folders your install media actually lives
    in. **Good:** everything you own is found; each row says *how* — by its
    bytes, by the medium's own name, or as a file you chose and ART has not
    checked; and the set line counts required and optional apart, so a missing
    optional does not read as a broken build. **Bad:** a file you know is there
    reported as missing; a row claiming *identified by its bytes* for something
    ART only matched by name; or one unreadable folder costing you the readout
    for the folders beside it, which should be named on their own.
12. **BoingBag 1 and then BoingBag 2, placed from Windows.** The oracle proves
    the bytes against a real `Updater`'s own tree; what nobody has done is tick
    the two BoingBags on the packages step and boot the result. Build a 3.9
    tree, add BoingBag 1, then BoingBag 2, then boot it and **ask** it —
    `version full`. **Good:** `Workbench 45.3`, seconds rather than minutes per
    package, and your own `Devs/NSDPatch.cfg` still beside the new one as
    `NSDPatch.cfg.old`. **Bad:** a run reported as done over a tree that will
    not boot — or BoingBag 2 accepted on a tree BoingBag 1 never touched, which
    is the one refusal that has to fire every time.
13. **The guide text, written into your own folder.** Under the readout there is
    a button for each folder that writes a plain-text list of what that folder
    can hold. Press one. **Good:** the file appears in that folder, in the
    language ART is showing, naming each piece, whether it is required, what it
    depends on, and carrying no links. **Bad:** a guide written over one that
    was already there — ART must leave an existing note exactly as it is, byte
    for byte, and tell you to delete it yourself.

Found something? File it the way every other defect in this project is
filed — see [docs/ISSUES.md](docs/ISSUES.md) for the format, and
[CONTRIBUTING.md](CONTRIBUTING.md) for how to open one.

Data safety is enforced in `core/safety`: every write is atomic, and files are
backed up to `.art-backup/` before being replaced (or, for images too large to
hold in memory, journaled block-by-block). Hand-tuned configuration files are
edited in place, never regenerated.

The interface ships in English and Turkish. The language is chosen in
Settings and remembered across restarts. Error messages coming from the Rust
core are still English regardless of the chosen language.

Not yet built: SFS (partitions using it are listed but their contents are not
readable), DMS/ADZ conversion, recovery tools, and writing *into* a CD or an
archive (both are read-only, deliberately and permanently).

Three fields in the Collection — chipset, genre and rating — are usually empty,
and that is a shortage of sources rather than of code. Lemon Amiga refuses
automated requests outright; Hall of Light publishes only web pages, and ART
fetches index files, never pages; OpenRetro has exactly the right data and
documents no way in. ART leaves them blank rather than guessing.

| | |
|---|---|
| Where the project is, what is next | [docs/STATUS.md](docs/STATUS.md) |
| Feature-by-feature state | [docs/FEATURES.md](docs/FEATURES.md) |
| Known defects | [docs/ISSUES.md](docs/ISSUES.md) |
| Phase definitions | [docs/roadmap.md](docs/roadmap.md) |
| Released changes | [CHANGELOG.md](CHANGELOG.md) |

## Install

**[Download the latest release](https://github.com/tolon/Art/releases/latest)** —
Windows 10/11, 64-bit. Two installers, and either is fine:

| File | What it is |
|---|---|
| `Amiga.Retro.Toolkit_<version>_x64_en-US.msi` | the Windows Installer package — use this one if your machine is managed, or if you want to deploy it |
| `Amiga.Retro.Toolkit_<version>_x64-setup.exe` | the NSIS installer — a smaller download and the usual choice |

Nothing else is needed: the WebView2 runtime ART draws its interface with is
already on Windows 10 and 11, and ART bundles no Amiga content, so there is
nothing to license or download alongside it.

**The installers are not code-signed**, so SmartScreen will say "Windows
protected your PC" the first time. That is the absence of a certificate, not a
verdict on the file — *More info* → *Run anyway*. If you would rather not take
anybody's word for it, the release is built by
[a public workflow](.github/workflows/release.yml) from the tagged commit, and
you can build the same installers yourself with the steps below.

**Where ART puts things** — worth knowing before you report anything, because
two of these are what a bug report needs:

| What | Where |
|---|---|
| Settings, the catalogue, the download cache | `%APPDATA%\com.amiga-retro-toolkit.desktop\` |
| `art.log` and `operations.jsonl` — every operation ART performed, and what happened | `%LOCALAPPDATA%\com.amiga-retro-toolkit.desktop\logs\` |
| Working files (staging, previews, unpacked packages) | wherever you point the scratch folder — ART asks once, up front, and pressing Next keeps the default |

ART does not write to any of your own disks unless you ask it to, and every
operation that changes a file backs it up first and tells you where the backup
went.

## Requirements

*For building from source. To just use ART, see [Install](#install) above.*

| Tool | Version | Notes |
|------|---------|-------|
| **Rust** | 1.93+ (stable) | MSVC toolchain (`x86_64-pc-windows-msvc`) |
| **MSVC Build Tools** | VS 2022 | "Desktop development with C++" workload |
| **Node.js** | 20+ | for the frontend |
| **pnpm** | 9+ | package manager |
| **WebView2 Runtime** | any | preinstalled on Windows 10/11 |

## Setup

### 1. Rust toolchain (MSVC)

```powershell
# Install via https://rustup.rs, then ensure the MSVC target is default:
rustup default stable-x86_64-pc-windows-msvc
rustc -vV   # should show: host: x86_64-pc-windows-msvc
```

### 2. MSVC C++ Build Tools

Install **Visual Studio 2022** (Community is fine) or **Build Tools for Visual
Studio 2022** with the **"Desktop development with C++"** workload selected.
This provides `cl.exe`/`link.exe` that the MSVC Rust target requires.

### 3. Frontend dependencies

```bash
pnpm install
```

## Development

```bash
# Run the app in dev mode (hot-reload frontend + Rust rebuild)
pnpm tauri dev

# Type-check the frontend
pnpm lint

# Run the frontend unit tests
pnpm test

# Run Rust unit tests
cd src-tauri && cargo test

# Production build (produces .msi + .exe installers)
pnpm tauri build
```

Build output:

```
src-tauri/target/release/bundle/
├── msi/   Amiga Retro Toolkit_0.9.1_x64_en-US.msi
└── nsis/  Amiga Retro Toolkit_0.9.1_x64-setup.exe
```

## Architecture

```
UI (React + TypeScript)
        ↓  Tauri commands
Application Services / Commands
        ↓
Workflow Engine  ←  Detection
        ↓
Amiga Core (Rust, platform-independent)
├── volume (the filesystem driver + writer) · adf · hdf · rdb · mbr · fat32
├── card (an SD card as the list of disks the m68k side sees)
├── osinstall (a distribution tree, built from your own install media)
├── preload (putting that tree onto a card) · gameindex · artwork
├── amigainstall (running a package's own installer, on the Amiga)
├── distro (which distributions, as data) · amiganet · layout · launch
├── archive (lha · zip · 7z, read-only) · iso · rom · cbm · whdload · vhd
├── analysis · compatibility · conversion · validation · hashing
├── security (hostile input) · safety (your data against ART itself) · jobs
├── sources (the mirrors, and every rule about where ART may fetch from)
        ↓
Platform Services → Windows
```

The Amiga core is **platform-independent Rust** — no Tauri types, no Windows
APIs, and no network. Where it needs something platform-specific it declares a
trait and the implementation lives outside — `MirrorClient` (the network),
`VolumeFormatter` (launching an external imager), `HostRecycler` (the Windows
Recycle Bin), `EmulatorLauncher` (starting WinUAE) and `VolumeSession` (the
command layer's own volume sessions). `src-tauri/src/net/` is the only place in ART that
opens a connection. This keeps the core unit-testable and leaves a future CLI shell
open.

- [docs/architecture.md](docs/architecture.md) — the layers, the traits, the
  volume writer's two strategies, the card and OS-install models, and the rules
  each of them is built on.
- [docs/security-model.md](docs/security-model.md) — hostile input, the
  data-safety pipeline, and where ART is allowed to fetch from.
- [docs/lessons.md](docs/lessons.md) — the incidents those rules were bought
  with, dated, each ending with the rule it produced.
- [docs/testing.md](docs/testing.md) — the test strategy, the external oracles,
  and what a test has to survive before it counts as a guard.

## License

Copyright (C) 2026 tolon.

**GNU General Public License v3.0 or later** (`GPL-3.0-or-later`). See
[LICENSE](LICENSE).

ART's dependencies are permissively licensed (MIT / Apache-2.0 / Zlib / CDLA)
with **one deliberate exception**: `libpfs3`, the PFS3 implementation ART writes
real PiStorm cards with, is **LGPL-3.0-or-later**. Weak copyleft, compatible
with ART's own licence, and taken in preference to writing a second filesystem
writer from scratch. All of them are listed in
[THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md) and checked on every push by
`cargo deny`. ART itself distributes **no** Amiga ROMs, no AmigaOS files and no
copyrighted software — see [docs/licenses.md](docs/licenses.md).
