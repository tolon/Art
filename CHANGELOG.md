# Changelog

All notable changes to Amiga Retro Toolkit are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Summarised on 2026-09-17 to the user-visible changes, one line each. The full
wording of every entry, with what was measured, is in git history
(`git log -p CHANGELOG.md`).

## [Unreleased]

### Added

- **First boot asks, on the Amiga, only the preferences ART did not set** — language and country, keyboard unless you chose a keymap, screen mode unless you applied a depth; a tick on *What to install*, on by default. Rehearsed under WinUAE only.
- **A first-boot step can restart the Amiga**; on AmigaOS 3.9, which has no restart command, first boot logs that and carries on.
- **ART reads Amiga LZX archives** with its own reader, behind the same safety checks as LHA, ZIP and 7z (checked against unar).
- **Groundwork for the one-button card's content step** (no screen yet): folders, archives, WHDLoad hardfiles and ADFs are checked and measured before a card partition is written, and one partition can take several folders.
- **More groundwork for the one-button card, still no screen**: the PFS3 driver, WHDLoad and the Kickstarts a card's titles need are found — Kickstarts are only ever placed on the names you agree to — and the card is built under a temporary name, checked, and only then given its own.

### Changed

- **When ART falls back to hst-imager to fill a volume, the `.uaem` files beside your files are applied** — protection bits, dates and comments now reach the volume.
- **Copying a folder out of an Amiga volume writes a `.uaem` beside each drawer** that has protection bits, a comment or a date.
- **The card content step's rules:** two sources with the same top-level drawer are refused; Amiga attributes are taken from Amiga-made ZIPs and `.lha` files; an archive's own `.uaem` wins; an entry named like a Windows device is staged as `_AUX` and renamed back on the Amiga.
- The first-boot report says when a step asked for a restart and what happened, and a timed-out rehearsal names the preferences window it was waiting in.
- The sentence for a restart the Amiga could not carry out says what may not take effect, instead of telling you to restart "to finish".
- Applying a screen depth twice no longer keeps a backup of ART's own four-byte marker file.
- **A note owed since 0.9.2:** an RDB partition table ART writes (a hard-disk image with partitions, or a card's Amiga area) counts whole cylinders only, rounding down. Where the size asked for is not a whole number of cylinders (16 × 63 sectors, 516 096 bytes), the table — and a new partitioned HDF's file — is one cylinder smaller than 0.9.1 made it: at or under the size asked for, never over. The old rounding let the last partition end past the disk ([ART-309](docs/ISSUES.md)).

### Fixed

- **A file's date and comment in its `.uaem` now reach a PFS3 partition ART fills** (ART-116, ART-335); a comment with a character the Amiga cannot store is refused by name.
- **A comment in a `.uaem` now reaches an FFS volume ART fills** (ART-337), and a card partition's size estimate counts comments.

## [0.9.4] - 2026-09-15

### Added

- **ART can put a filesystem driver into a card's existing partition table, or update it, without hst-imager** — previewed, backed up to a file you choose, written in four undoable steps and read back; a different or unidentifiable driver is left alone and the preview says why.
- **PFS3 partitions ART formats carry PFS3's deleted-files area (deldir)**, and a file deleted through ART's writer can be undeleted through it.

### Fixed

- **A file of 4 GiB or more is refused by name before anything is copied onto a PFS3 partition.**
- **A damaged drawer on a PFS3 partition is reported as damaged, not as a missing file** or a card that has not booted.
- **ART reads a PFS3 file's size correctly when the Amiga stored extra fields** (hard links, rollover files, owner and group bits).
- The Files screen's empty-pane text no longer clips in Turkish at 1024x768.
- **The job bar names each job in the language you chose.**
- The OS Builder no longer re-reads two identical install discs whole on every start; a disc's fingerprint is remembered for 30 days.
- Adding an older update over a newer one says which is newer and in what order to add them.
- **A PFS3 partition ART fills holds as many files and drawers as PFS3 allows**, no longer about 21 500 (ART-311).
- **A name the Amiga could not open is refused by name**: PFS3 is formatted for 107-byte names and a longer name is refused before the copy.
- PFS3 drawers record their parent drawer the way the Amiga's handler expects; a partition formatted before this is worth formatting again.
- **Dates on the Amiga show your local time**, in the offset in force on that date, instead of UTC; known install discs are read again once.

## [0.9.3] - 2026-09-13

### Fixed

- **Filling a PFS3 partition no longer gives a file another block's contents** (ART-312, listed under Known issues in 0.9.2); a partition filled before this is worth filling again.

## [0.9.2] - 2026-09-13

### Added

- **The Files screen's columns can be resized, hidden and reset**, shared by both panes and remembered.

### Changed

- **OS Builder: the install lane is four tabs** (Amiga files · What to install · Kickstart and destination · Build) and old step URLs redirect.
- **OS Builder: building is one press** — the Build tab runs the tree, each ticked update in order, then first boot, stops at the first step that does not succeed, and reports each step as succeeded, refused, failed, stopped or not attempted.
- OS Builder: Build over a folder holding a tree ART built updates it instead of refusing it; BoingBag 1 and 2, the locale updates and the Turkish catalogs are placed from Windows.
- OS Builder: the source step shows what ART found first; what to install is one list; the bar's button goes to the next tab.
- **The Amiga-side installer run and the first-boot rehearsal moved to the WinUAE studio.**
- **While a build runs, the sidebar refuses to navigate and says how to stop the build.**
- OS Builder, Build tab: first boot says where `S/User-Startup` was backed up, and the button reads **Build again** after a success.
- **Card builder: any Emu68 archive can go on the card**; only `Emu68-raspi.zip` is still refused.
- **PiStorm: the A1200's Kickstart is accepted for every Amiga.**

### Known issues

- **Filling a PFS3 partition can, rarely, give a file another block's contents, with no error** ([ART-312](docs/ISSUES.md)); fixed in 0.9.3.
- **A PFS3 partition ART fills holds at most about 21 500 files and drawers** ([ART-311](docs/ISSUES.md)).

### Fixed

- **A PFS3 partition over about 4.9 GB that ART formatted could not be mounted, and a first new drawer could fail with "disk full"** (ART-310); reformat such partitions.
- BoingBag 3.9-2 Contribution goes on after BoingBag 3.9-2, though both carry the same top-level name (ART-304).
- Build waits and names the update when a ticked update has more than one candidate file (ART-303).
- **The OS Builder no longer freezes the window** re-reading repeated install discs (ART-297).
- Locale 3.9's Turkish catalogs and fonts go on before BoingBag 3.9-2's, and the list says so (ART-298).
- The Build tab no longer says "no update ticked" while it is still checking (ART-299).
- Removing a folder from the OS Builder's list also forgets it as your archives folder (ART-291).
- Pressing Back or Forward while a build runs no longer abandons it (ART-292).
- Previewing a BoingBag update no longer writes to your system drive (ART-296).
- A first-boot rehearsal ends when its copy grows out of bounds, keeping the copy (ART-294).
- The OS Builder's folder list counts only install disks (ART-285).
- Dropping onto the dashboard no longer rewrites a remembered Files tab (ART-283).
- **A damaged update package can no longer fill your disk from inside the emulator** — a run whose copy grows out of bounds ends at once (ART-278).
- The "nobody answered" ending states why ART believes that (ART-279).
- OS Builder: the tree banners, the files tab's update panel and the Build tab's preview all judge the tree and file you chose ([ART-289](docs/ISSUES.md)).
- OS Builder: a release's ticked parts are kept in one place with the build's other choices ([ART-290](docs/ISSUES.md)).
- OS Builder: browsing for an update archive adds its folder to the one folder list instead of a separate setting.
- **Card images fit the card they are named for** — sized in decimal gigabytes with a safety margin ([ART-308](docs/ISSUES.md)).
- The last partition of a card no longer ends past the card ([ART-309](docs/ISSUES.md)).

## [0.9.1] - 2026-09-08

### Added

- **BoingBag 1 and BoingBag 2 install from Windows, in seconds, with no emulator** — checked file by file against a tree two real emulator runs produced; `Devs/NSDPatch.cfg` is updated with your old one kept as `.old`, and a BoingBag build ART does not know is refused before anything is written.
- **BoingBag 3.9-2's XAD update is applied with it** when your tree's `xadmaster.library` is older than 10, and the report says whether it was applied or not needed.
- **Identifying install media no longer reads a folder of games end to end** — a disc is hashed only if its own name is one a recipe installs from; the rest are listed as left alone.
- **The AmigaOS 3.9 updates are one ordered chain**, and every link is in one of seven states that never collapse into "not done".
- **Three more update packages:** BoingBag 3.9-2's Contribution is placed from Windows; Euro-Update and BoingBags 3&4 are listed with the reason ART will not place them.
- **ART can write a "what goes here" note into your material folders**, in the language ART shows, never over an existing note.
- **The commander has cursor keys** — Up/Down, Home/End, Page Up/Down, Shift-marking and Ctrl+Space for the command line (ART-275).
- **A real Amiga can finish its own first boot**: an optional first-boot step detects the machine, runs fixed setup steps and writes a report that the card-preparation screen reads back.
- **ART can say what an install disk is by its content**, against a 186-row table adopted from Emu68 Hatcher (MIT), with five distinct answers; it runs as a cancellable job.
- **The OS Builder shows what media was found when it cannot complete a build** — this release's, another release's, or media that does not settle the release.
- **Drawers with no icon position can be arranged into a grid** ("Arrange icons"); not yet checked on a real Amiga.
- **A built distribution can carry your own wallpaper** (PNG or JPEG converted to ILBM), and **the screen depth and shell defaults are yours to set**, edited in the release's own prefs files.
- **Verify checks every backdrop path a prefs file names** in the built distribution tree.
- **A WHDLoad collection kept as drawers is catalogued**, including drawers inside an `.lha`, and **iGame can be told what ART knows** through `igame.data`, backed up and reported per title on your own collection.
- **What a WHDLoad drawer wants at launch is read from its icon's ToolTypes.**
- **AmigaOS 3.2.2 is an installable release** — a base plus an update, asked for as two labelled folders; the finished tree's own release marker is read back and reported; a component can remove a file; icons are amended rather than replaced; ART reads a Kickstart's module table to decide on softkicked modules.
- `scripts/icon-oracle-check.py` round-trips every `.info` on real install media through ART's reader (not in CI).

### Changed

- **The Amiga-side step is the whole AmigaOS 3.9 update chain, with one Run button**, and host-placed rows are placed from the same screen.
- **The OS Builder asks for your files once** — one folder list replaces the separate folders, and a readout says for every piece of material whether it is here and how ART knows; the Amiga-side step's Browse fields are filled from the same answer, and a file you choose by hand stays yours.
- The readout no longer reports the Kickstart as missing from your folders, and the archives folder is remembered per release.
- **ART looks like a Windows 11 application in both themes** — Fluent colours and controls, a WinUI navigation pane, six Amiga tiles on the Dashboard; the File Manager keeps its Total Commander look.

### Removed

- **The emulator route for BoingBag 1 and BoingBag 2**, with the second archive field, the CD-ROM field on the Amiga-side step and the separate "second update" report line; your earlier choices stay in your settings, unread.

### Fixed

- A BoingBag installs even when your folder holds two builds of it, using the one you picked or the one ART recognises by content.
- "Checking what this would replace…" no longer sits above a check that failed, or for ever on a row already in your tree.
- The updates step is headed *AmigaOS 3.9 updates* and says what it does.
- The updates chain and the source step no longer disagree about which file you chose.
- Ticking a Turkish locale update before BoingBag 3.9-2 says what to install first instead of a raw error (ART-282).
- Two packages can share one archive when the recipe declares it (ART-276).
- The Amiga-side install panel remembers an archive per package and names the package a wrong archive belongs to (ART-277).
- **Applying a wallpaper, screen depth or icon arrangement no longer freezes the window and can be stopped** (ART-248).
- The media step's hints and outcomes are announced to screen readers with their field (ART-241).
- **A built AmigaOS 3.2 tree carries `Utilities` and `WBStartup`** (ART-251).
- A WHDLoad rescan clears a title removed from its archive (ART-243), and an unchanged archive is not reopened on every refresh (ART-244).
- Stopping the install-media check says you stopped it; one unreadable folder no longer discards the others' results; two layers on one folder no longer list every disk twice.
- The OS Builder's "what is in this folder" line: reads both folders of a part-built 3.2.2 (ART-257), claims only what was checked (ART-258), counts every folder you added (ART-256), no longer says disks carry no version (ART-255), survives a switch to the detected release (ART-254), no longer calls the right folder wrong (ART-253), and no longer denies your release's media when shared disks are present.
- **A built distribution's own drawers carry their icons**, so Workbench no longer shows them missing.
- A WHDLoad archive with one unreadable drawer keeps the rest, and scanning a large WHDLoad archive is about 80× faster.
- A 3.2 + 3.2.2 media pair is no longer refused over one shared floppy name.

## [0.9.0] - 2026-09-04

0.9.0 builds an AmigaOS 3.9 distribution from your own CD as well as 3.2 from floppies, runs a package's own installer inside an emulator when the host cannot place its files, writes a PiStorm card that can carry two AmigaOS environments, and puts your WiFi on the card before its first boot. **Still not claimed:** no card ART built has been flashed and booted on real hardware.

### Added

- **The official 3.9 Locale update installs**, both its wholesale half (`locale-39`, 49 keymaps) and its Turkish half (`locale-39-turkish`, catalogs and fonts placed where `diskfont.library` looks).
- **A card can carry a second complete AmigaOS**, each on its own Amiga disk; AmigaOS's own Early Startup menu chooses, ART sets which starts by default and names a tie rather than choosing.
- **Your WiFi details can go on the card while you set it up** — the stack's configuration is edited, the network list replaced after telling you how many networks it holds, and the passphrase is never remembered.
- **Aminet: a package can be installed straight into a hard-disk image**, a downloaded folder handed to the Collection, and the mirror list edited and remembered.
- **The Kickstart a WHDLoad title asks for can be placed where WHDLoad looks for it**, matched by content, one title at a time.
- **`igame.data` is written the way iGame's own source reads it.**
- **An install can read its disks from more than one folder.**
- **Switching firmware sets says which Kickstart and kernel will be used and whether they are on the card.**
- **ART can propose a card's volume table**, sized from two real PiStorm cards, splitting FFS work volumes on older Kickstarts.
- **AmigaOS 3.9: the Turkish ISO-8859-9 font set, every keyboard layout and the euro country files** as optional components; **AmigaOS 3.2: the compressed help system is expanded** (3 263 files) and every keyboard layout is a tick-box.
- **ART clears up staging folders left by a crash** — only its own, only over a day old, never while another ART might be running.
- **Recipes can switch on drivers the install disks leave on the shelf** (`Devs/DOSDrivers`, `Devs/Monitors`, `WBStartup`), and a recipe naming something the disks lack is refused before building; nothing shipped switches anything on.
- **Two of ART's refusals appear in your own language.**
- **Cards get a second partition**: `SDH0` for the system and `SDH1` for your files.
- **The card builder can write PFS3 partitions** with your own `pfs3aio`, the same driver the volume step uses.
- **A scratch folder you choose**, asked on first run and changeable in Settings.
- **Package bundles**: 14 named sets of Amiga software, 62 packages, downloaded in order, with unfetchable entries named before you tick, a per-package report with six distinct endings, permission notes above the tick, and remembered choices.
- **ART remembers what was on an install disc** so it does not read it again, with **Scan again** and a **Reuse the last scan** option.
- **An update package can be added onto an existing distribution tree**, with every overwrite classed as upgrade, downgrade, same version, or unversioned before you confirm; update archives are read from their own folder.
- **Running a package's own installer on the Amiga**: a panel in the OS Builder copies the tree, unpacks the package as a third disk, mounts your own CD image when the package asks for one, opens an emulator after warning you, and ends one of four ways — succeeded, refused, nobody answered, window closed — each with its own next step. The copy replaces your tree only on success; out-of-order packages and BoingBag 1's too-old `Updater` are refused before the run.
- **Space on a folder counts its size** in the file manager, cancellable, shown as "at least" when stopped early.
- **AmigaOS 3.9 can be built from your own install CD**, with a release picker, and a dropped disc offers the OS Builder.

### Changed

- **The OS Builder is a sequence of steps** with a strip along the top and one step on screen at a time; the tree you just built is the one the next steps work on.
- **Every WHDLoad launch runs on one known-good A1200** (68020, AGA, 2 MB Chip, 8 MB Fast, Kickstart 3.x); floppies and plain hardfiles keep their own machine, and the per-title picker still overrules. The WHDLoad Fast RAM setting now applies exactly, including 0.
- An unresponsive Amiga-side install is given 30 minutes.
- The destination field's hint says a new or empty folder is fine.

### Fixed

- The same disk in two folders is one disk when it is byte-identical; differing disks of one name are still refused.
- Ten fixed-size chunk reads use `as_chunks` for the current Rust toolchain; no behaviour changed.
- **Greek, Polish, Russian and Turkish 3.2 systems get their fonts.**
- **ART refuses to build an FFS partition past what the card's Kickstart can address.**
- The selected partition in the Hard Disk studio stands out on every colour.
- **A Turkish system can type Turkish** — the chosen keymap is written into the boot.
- The WiFi settings go where `ENVARC:` points, not into a folder no Amiga opens.
- **A BoingBag install finishes properly** — BoingBag 2's ROM update is put in place and BoingBag 1's protection bits are set.
- **The Turkish fonts are spelled `.font`**, so AmigaOS sees them.
- The AmigaOS 3.9 component rows are named; GlowIcons no longer loses sixteen icons to the Storage disk.
- A cancelled Amiga-side install says what became of the copy.
- Error messages show ART's own wording without a doubled "Error:" or an empty Error ID line; twelve sentences lost a stray run of spaces; seventeen unused sentences were removed.
- A misspelled key in a recipe or package file is refused by name, and a package-file error no longer says "recipe".
- **You are asked for your Kickstart once, not three times.**
- **ART stops when an install disk changed between preview and build.**
- **A card keeps its storage settings across Emu68 versions and Pi boards**, with the read-only default kept.
- **Everything ART stages follows the scratch-folder choice**; an unreachable folder stops the job and changing it moves nothing.
- **The install plan predicts the finished tree** — files and bytes match what is written (ART-205).
- Cancelling a bundle download ends as cancelled, and a cache hit over someone else's file is checked (ART-213).
- **Choosing an AmigaOS release changes the whole screen**, and each release remembers its own folders.
- A folder holding another release's disks says so once, with a one-click switch.
- **You can choose a new or empty folder to build into.**
- **A card keeps the boot settings for every PiStorm board** in `config.txt`.
- A fetched archive supplied in the wrong box is recognised; the preview no longer describes a run that cannot happen; refusals appear at the button, once.
- **The install preview no longer restarts on its own**, and a new preview cancels the previous one.
- A WHDLoad refusal for a non-A1200 Kickstart 3.1 names the Kickstart to add.
- **A package's own installer now installs**: both BoingBags ran to completion (Workbench 45.3), after five environment faults were fixed (`SetPatch`, ReAction classes, `ENV:`, a lost refusal, a restart mistaken for a finished run) (ART-185).
- **Archive names**: Latin-1 names read correctly (the Turkish catalogs land in `türkçe`), drawers kept in extension headers preserved, comments no longer glued to names, names Windows cannot store escaped and colliding names refused.
- A CD build's size prediction no longer counts folders as bytes; a disc bigger than ART reads reports ART's limit, not damage; the staging screen shows what it did not look at; the sidebar collapses correctly under Application Size.
- **A BoingBag no longer offers a tick it cannot honour** (its payload is password-protected), and suspicious entry names in an update archive are listed on its row.
- **The AmigaOS 3.9 tree ART built was AmigaOS 3.5; it is now 3.9** — the `Workbench3.9` overlay is laid, and the booted system answers `Workbench 45.1`.
- The frontend test suite no longer reports a pass while exiting with an error.
- The component list belongs to the release you picked, remembered per release; a mixed-case disc builds in the right place; a file where a drawer was expected is refused.
- The install says when it will not run and shows its progress; the "Verify against a card" fields explain themselves.

## [0.8.5] - 2026-08-18

Everything from the Phase 0 foundation (2026-08-08) to 2026-08-18.

### Added

- **Play gives a WHDLoad launch Fast RAM headroom** (a setting, 0–8 MB), and the confirmation screen states the memory a launch will use; a self-booting WHDLoad hardfile from the owner's collection (`1000 Miglia`) reached the game.
- **The Collection shows the screenshots inside your `.rp9` files**, lets you attach your own picture to a title and choose which picture shows, and opens a title's detail panel with its disks, slave, Kickstart and provenance.
- **Play** hands a title to WinUAE: floppies boot directly, hardfiles mount read-only (with an opt-in writable switch), and a WHDLoad drawer mounts with your own system image or boots straight into the game.
- **Titles can be edited on the spot**, with one-button title fixes and file renames where ART can show a reason; multi-disk games are recognised by their neighbours.
- **Cover art, screenshots and icons** from libretro-thumbnails and whdload.de, with configurable sources, nothing fetched until you ask, and results remembered per title.
- **The Collection keeps its catalogue between runs**: it opens instantly, Update reads only what changed, several folders make one library, moved files are followed, missing ones are kept and marked, and your corrections are stored apart.
- **Titles come from what states them** — a WHDLoad slave's own header or an `.rp9` manifest — with guesses marked, a title's declared Kickstart shown, and bootable hardfiles read from the inside.
- **ART warns, per folder and named by drive, before preparing a card whose Kickstart is older than the system going onto it needs.**
- **The OS Builder builds an AmigaOS 3.2 distribution tree from your own floppies**, switching on the A1200 Modules disk for older Kickstarts, with a manifest of every file and a volume check that tells "not checked" from "checked".
- **The `layout` screen turns a dropped pile of files into an organised staging tree** with an editable preview; WHDLoad drawers travel whole with their icons, and ROMs and Commodore disks are refused with a reason.
- **Prepare a card's Amiga volumes from the OS Builder** — tick partitions, name volumes, give each a folder, preview; the ticks are deliberately not remembered, and `hst.imager`'s path is a setting.
- **Drop the Emu68 archive and your Kickstart on the card builder** and the form fills itself; everything else dropped is told where it belongs.
- **One card health check** covering the partition table, Amiga disks, filesystems and the build manifest, with *not checked* never shown as a tick, plus the steps only you can take.
- **Every card comes with a build manifest** (`card.img.manifest.json`) that can be checked against the card any time; `scripts/fat-oracle-check.py` checks the boot files with 7-Zip.
- **The OS Builder can build a boot-only PiStorm card image** from your Emu68 release and Kickstart, previewed first, with an existing destination refused before the button.
- **The Hard Disk screen opens a PiStorm card** — its MBR slots, each Amiga disk and its partitions, with drivers counted across the whole card; read-only.
- **An About panel in Settings** with the version, author, source, licence and the GPL notice.
- **A named firmware set can be deleted** (backed up; the active set is refused).
- **OS Builder: a registry of AmigaOS distributions** (CaffeineOS, CoffinOS, AmiKit, ART Baseline), each leading with its licence and checking the Kickstart family and card size; ART downloads no distribution.
- **The PiStorm screen identifies every Kickstart on the card**, can copy one on under a confirmed name, reads the kernel's Emu68 version, and manages named firmware sets.
- **The PiStorm screen asks for your hardware as three answers** (Amiga, PiStorm board, Raspberry Pi), shows the token beside every option and the whole `cmdline.txt` line, and previews the change before saving.
- **Application Size scales the whole program 70–250 %**, and **every choice you make is remembered**.
- **The New HDF wizard embeds the PFS3/SFS driver inside the disk**, reading its version from `$VER:`; the Hard Disk studio names partitions whose filesystem is missing (ART-084); the Aminet download folder is in Settings.
- **Enter opens a disk image, archive or C64 disk in the pane** and Backspace comes back out; tabs per pane restored on reopen; full keyboard coverage; file colour rules; command-line history.
- **F6 moves** (copy, verify, then remove), each pane header is a source box, path and filter, and the command line navigates and filters.
- **A file is identified by its content, not its name**; CD images (ISO9660, Joliet, raw Mode 1 and Mode 2/XA), LHA, ZIP and 7z archives, and C64 `.d64`/`.d71`/`.d81`/`.t64` open read-only in the file manager.
- **The Files screen became a commander**: multi-select, batch copy and delete as one operation, several `.lha` archives dropped at once, one sort order with per-pane sorting, a filename filter, a Total Commander look.
- **ART writes a disk a real Amiga boots from** — real 68000 boot code, booted under WinUAE and then cold-booted on a real A500/A500+ from a Gotek.
- **Turkish**, on every screen, with a build-time parity check between the two language files.
- **Install WHDLoad**: a `.lha` onto a hard disk image in one step, checked before writing and refused with a reason rather than guessed.
- **The Files screen writes into any volume** — copy, rename, move, delete, folders, attributes, F3–F9, copy between images, a cost report before a folder copy, edit a file in an image (F4), editable protection bits and comments preserved through `.uaem`, icons travelling with their drawers, Aminet packages into a hard disk partition.
- **Copy a whole folder into an ADF**, show a downloaded Aminet package in the Collection, and edit the mirror list.
- **Hard disk images open in the Files screen** with their partitions browsable.
- **Files**, a two-pane manager (local, ADF, HDF) with drag and drop out to Explorer; Aminet sorting, filters, a chosen download folder and install into an ADF.
- **Aminet Studio** (§41.5): sync the catalogue and search it offline, downloads through a trust pipeline, background jobs, mirror failover, provenance on package facts, `ART-MIRROR-UNREACHABLE` and `ART-INTEGRITY-MISMATCH`.
- **Job Queue** (§54, §55) with a global job bar and Stop, and **Beginner / Power User mode** (§47, §48); `ART-CANCELLED`.
- **Operation Log** (§53) and stable `ART-*` error IDs (§68).
- **`core/safety`**: atomic writes, generational backups, `OverwritePolicy` for LHA extraction, backup paths reported to the UI, read-only HDF mounting for WinUAE.
- **Phase 0 foundation**: the Tauri 2 + React 19 shell, the platform-independent Rust core, content detection, SHA-256, the Workflow Engine, one global drag-and-drop listener, SQLite, logging, settings, i18n, dark/light theme, Dashboard, Settings, Windows CI, `cargo-deny`, MSI and NSIS installers.

### Changed

- **Preparing a card's Amiga volumes no longer needs `hst-imager`** — ART's own writer by default, `hst-imager` a named per-step fallback, and the preview and result name which ran each step.
- A title whose chipset nothing declares reads **Unknown** instead of OCS/ECS.
- The manifest check is part of the card health report.
- The Files screen shows one status line at the bottom, asks about name collisions in the copy dialog, keeps its function keys on one row, and merges the selection count into each pane's status line.
- Machine profiles cover A1000 through CD32; building ART needs Rust 1.93.
- Rust core error messages were still English at this point (ART-060).
- ADF Studio and the file manager go through one AmigaDOS writer.
- A large image is written in place with a journal instead of being copied for backup, and an interrupted write is offered back on the next open.
- `collection_scan` and LHA extraction run as jobs; command errors end with `Error ID: ART-…`; `create_rdb_image` became `create_rdb_layout`; CI runs clippy blocking and `lib.rs` allows only `dead_code`.

### Removed

- Nine unreachable code paths (an old ADF-extraction command, a superseded folder-copy planner and block writer, an uncalled extraction command, a placeholder screen).

### Fixed

- **A self-booting WHDLoad hardfile no longer asks for a system it does not need**, and an older catalogue reads as stale instead of breaking the Collection.
- A WHDLoad title with no stated Kickstart gets the newest suitable ROM instead of Kickstart 1.3.
- The plain-hardfile geometry change was wrong and reverted to 32 sectors / 1 surface (ART-149).
- **Every status badge, secondary text, the primary button, input borders and the crash screen meet WCAG contrast in both themes**, checked by `scripts/contrast-check.py` on every build.
- **Accelerator and other non-Kickstart ROMs are no longer called `CRC ERR`** or named as Kickstarts; a blank compatible-models field says why; Aminet's inputs follow the theme.
- An interrupted artwork fetch keeps its pictures, and fetching is about forty times faster.
- **Confirmations appear** — thirteen had silently returned "yes", four of them before a deletion.
- **Hard disk images whose filesystem is smaller than the file open** (1 456 of 1 697 real images); no second scan; the progress bar no longer rounds to 100 % early.
- Card-preparation badges and the result panel describe the native writer honestly, and a count string gained plural forms.
- **A licensed Amiga Forever ROM is decoded with its `rom.key`**, a missing key refuses the card build, and the ROM screen no longer ticks a ROM it cannot read.
- **ART recognises real Kickstarts by their stored checksum**, from a table generated from an open-source ROM database, and a file's size no longer names a machine.
- The build report counts the finished tree, `distribution.json` records each file once, and an unknown byte count is left out rather than shown as zero.
- **Hard disks ART creates can be mounted by an Amiga** (the RDB driver's `PatchFlags` were wrong), and **AmigaOS 3.2 built by ART boots to Workbench** under WinUAE.
- **The whole 3.2 tree reaches a volume in one run** — one writer formats and fills each volume, nothing is erased for a copy that cannot finish, and a crashed tool's message is shown.
- **A small hard disk image can be written into** (ART-043).
- Content too wide for the window under Application Size can be scrolled to.
- Planning a batch of archives no longer freezes the window; a cancelled copy into a large image says how many files landed; focus stays after F5; Stop works inside an archive; a filter matching nothing says so.
- What you have open in a studio stays open when you leave the screen.
- A partition ART creates carries `MaxTransfer`, `Mask` and the engine's 600 buffers.
- The PiStorm screen says why it is grey; the installer's Publisher reads `tolon`; the licence inventory no longer lists ART as MIT.
- **ART can open a real PiStorm card** (the RDB is about 1.1 GB in), reading every Amiga disk on it.
- A write-protected file is not replaced without asking; a delete-protected file is not deleted without asking; `Docs` and `docs` are one drawer; a selection of only shortcuts says what it declined; "1 weeks ago".
- **ART no longer names an Emu68 download that never existed** — the release line is asked; an unnamed HDMI mode is kept; the Kickstart picker shows for Power Users.
- **The PiStorm screen stopped offering controls Emu68 does not have** (JIT, MMU, Fast RAM, `emu68-sd.device`), and profiles list the options they set.
- A setting changed right after launch is no longer undone by the settings file arriving late.
- LHA archives are recognised by content; "Open in the file manager" opens the file; Mode 2/XA CD tracks are read; copying out of an image honours your overwrite choice.
- **Cancelling a WHDLoad or Aminet install leaves the disk untouched**; the WHDLoad refusal no longer contradicts itself; the sidebar no longer clips the page.
- **A write that would break a disk image is refused** after checking the whole result, and HD floppies and hard disk images are measured at their own geometry.
- **ADF Studio opens bootable disks**, 1.76 MB disks work, long pages scroll, the window scales, disabled buttons look disabled, a refusal no longer looks like a crash, installs refuse up front when they will not fit, and a write is no longer reported failed after it succeeded.
- Names with accented characters are no longer refused as too long.
- The Aminet download folder and custom mirror order are remembered.
- **Four compatibility bugs found with amitools**: invalid ADF checksums (ART-033), zero-byte files (ART-034), a reversed bitmap (ART-035) and RDB partitions with no filesystem (ART-032) — disks made earlier should be re-created.
- The Aminet screen no longer waits forever after a failed sync or download.
- **LHA archives with level 2 or 3 headers open** (ART-031), and mirror failover no longer splices two responses (ART-030).
- **Hard disk images are created and opened without loading them whole**, creation never overwrites an existing image, RDSK fields and empty block lists are written correctly, impossible layouts are refused, RDB checksums honour `SummedLongs`, folder scans are depth-limited, and tiny image requests error instead of aborting.
- **ADF names hash the way AmigaDOS does**, ADF edits are validated and backed up before commit, invalid block numbers no longer crash the app, directory chains are bounded and consistent, `FF.CFG` and `cmdline.txt`/`config.txt` are edited in place, HD floppy geometry is reported, multiple HDFs get device names, WinUAE is found in Program Files, `.uae` injection and a zip-bomb overflow are closed, and concurrent launches no longer clobber each other.
