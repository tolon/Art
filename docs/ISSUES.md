# Known Issues & Technical Debt

Every defect found in ART gets an ID here — fixed or not. The ID is stable and
never reused, so a commit message, a code comment or an error message can point
at one and stay meaningful.

Spec §68 requires user-facing errors to carry an identifier rather than an
opaque code. These IDs are the registry those identifiers come from.

**Format:** `ART-NNN` · severity · one-line claim · where · how it fails ·
what fixed it (with the test that proves it).

**Severity:**

| | Meaning |
|---|---|
| 🔴 **Critical** | Can destroy or corrupt user data, or crash the application. |
| 🟠 **High** | Wrong results, or a spec rule broken in a way users will notice. |
| 🟡 **Medium** | Incorrect behaviour with limited blast radius. |
| 🔵 **Low** | Hygiene, dead code, developer-facing friction. |

---

## Open

_(ART-075 was open here; it is fixed — see [Phase 2a](#phase-2a) below.)_

**ART-094** 🟡 **Overwriting a write-protected file is not checked either**
`core/volume/write/mod.rs` · `commands/volume_write.rs::replace_file` · The
delete half of [ART-088](#open) is fixed: the writer honours the `d` bit and
refuses unless the user has been asked. The **overwrite** half is not.

AmigaDOS governs replacing a file's contents with the `w` bit, and a file with
it withheld is one the Amiga will not let you write to. ART's `replace_file`
deletes and re-adds — an implementation detail — and passes
`DeleteProtection::Override` precisely because that delete is not a deletion
the user asked for. Which is right, and leaves the real question unasked:
nothing anywhere checks `w` before overwriting.

The same shape as the fix that just landed: honour the bit by default, and let
a caller that has shown the question say so. The Files screen's copy dialog is
where that question already belongs — it asks about collisions there, and
"this one is write-protected" is the same conversation.

Split out of ART-088 on 2026-08-13 rather than left as a sentence inside it.

**ART-093** 🟡 **ART cannot fetch an Emu68 kernel update; it can only tell you which one you need**
`core/pistorm/` · `net/` · The fix round's F4 asked for two things. The reading
half is built: the card's `Emu68.img` is identified from the `$VER:` string its
own build compiles in, and the hardware matrix names the archive that belongs on
it. The **fetching** half is not.

Not built rather than half-built, and the screen offers no button for it — the
same rule the WiFi panel follows. Three things make it more than an afternoon,
and each is a decision worth making deliberately:

- **The host policy.** `net/http_mirror.rs` refuses cross-host redirects on
  purpose (§41.5.7): a followed redirect is a fetch the user never configured.
  A GitHub release asset redirects to `objects.githubusercontent.com`, so this
  needs its own client with its own stated policy, not a relaxation of that one.
- **Which release.** The archive name depends on the release line, and one name
  means a different board in each ([ART-091](#open)). A fetch that resolves
  "latest" without the line would be the same defect with a network connection.
- **Writing it.** Unpacking an archive onto a card that boots somebody's machine
  is a multi-file write and wants the same preview → backup → verify every other
  write in ART has, per file.

Until then the screen tells the user exactly which archive to download and from
which release line, which is the part they cannot work out for themselves.

Recorded 2026-08-13 as owed work, not a defect.

**ART-092** 🔵 **A named PiStorm firmware set cannot be deleted from ART**
`core/pistorm/mod.rs` · `src/pages/PistormStudio.tsx` · Named sets can be
created, duplicated, renamed and activated. Deleting one is deliberately absent
from that list: removing a user's configuration is destructive, and destructive
actions in ART carry their own confirmation design (§92) rather than a bare
button. The screen says so rather than leaving a gap the user has to discover.

Not urgent — a set is a file on a card the user can already delete in Files, or
in Explorer. Worth doing properly when the confirm shape for "delete a thing
the user made" is settled, which is the same question a future *delete a ROM
from the card* will ask.

Recorded 2026-08-13 as a deferral, not a defect.

**ART-091** 🟠 **ART named an Emu68 archive that has never existed, and the name that does exist means a different board in each release line** — *fixed 2026-08-13*
`core/pistorm/hardware.rs` · `PistormVariant::kernel_archive()` returned
`"Emu68-pistorm16.zip"` for the PiStorm16. **No Emu68 release has ever shipped a
file by that name.** It was invented to fill a cell in the table — in a module
whose own doc comment says "named rather than guessed" — and it survived the
ART-090 rebuild and its review.

Verified 2026-08-13 against `api.github.com/repos/michalsc/Emu68/releases` and
`pistorm.github.io/tutorials/sd_setup/`, the answer is worse than a wrong
filename:

| | 1.0.7 (latest stable) | 1.1.0-alpha.1 (prerelease) |
|---|---|---|
| PiStorm (classic) | `Emu68-pistorm.zip` | `Emu68-pistorm-classic.zip` |
| PiStorm600 | `Emu68-pistorm.zip` | not stated |
| PiStorm32-lite | `Emu68-pistorm32lite.zip` | `Emu68-pistorm.zip` |
| PiStorm16 | **no asset at all** | `Emu68-pistorm.zip` |

Two things follow, and neither can be said by a single string:

- **PiStorm16 has no stable release.** `v1.1.0-alpha.1` is the first Emu68 to
  support it — a GitHub prerelease. "Which zip" has no honest stable answer.
- **`Emu68-pistorm.zip` changes meaning between the lines.** In 1.0.x it is the
  classic board's firmware; in 1.1 alpha it is the PiStorm32-lite's and
  PiStorm16's. A user told "download Emu68-pistorm.zip", who then lands on the
  latest *stable* release — the obvious thing to do — flashes firmware for
  another board entirely.

Fixed by making the release line a field of its own and the answer a type
rather than a string: `KernelArchive::{Named, Absent, Unstated}`. `Absent` and
`Unstated` are both real cells in that table, and a plausible filename in
either is the slip this type exists to prevent. Three new notes carry it to the
screen — no stable release, the name means another board in the other line, and
the notes do not say. `no_archive_name_is_one_no_release_has_ever_contained`
pins the whole table against the three assets those releases actually ship.

Found in review of the ART-090 work, 2026-08-13.

**ART-090** 🟠 **The PiStorm screen offered controls Emu68 does not have, and wrote tokens it does not read** — *fixed 2026-08-12*
`core/pistorm.rs` · `src/pages/PistormStudio.tsx` · `src/i18n/*.json` · Four
things on that screen were not merely wrong, they were invented:

- **"Enable JIT Dynamic Recompiler."** Emu68 *is* a JIT engine. The official
  FAQ's words are "Emu68 is exclusively a Just-In-Time (JIT) engine" — it
  cannot be turned off short of powering the machine down. The real, adjacent
  option is `enable_cache` (JIT cache active from startup).
- **"Enable 68040 MMU Emulation — required for WHDLoad MuForce."** Emu68
  documents no MMU emulation at all; the MMU is famously not emulated and
  WHDLoad runs in NOMMU mode. This one actively misled: a user chasing a
  WHDLoad problem would have turned it on and believed something had changed.
- **A Fast RAM slider**, 1 GB by default. Emu68 maps RAM automatically. There
  is no size to set. The real memory knobs are `limit_2g`, `z2_ram_size`, the
  `enable_c*_slow` family and `move_slow_to_chip`.
- **`emu68-sd.device`**, named in the UI as the "high-speed MicroSD" driver.
  The real driver is `brcm-sdhc.device` on the Pi 3 family and
  `brcm-emmc.device` on the Pi 4 / CM4. The `emu68-` prefix belongs to the RTG
  card (`emu68-vc4.card`). This name reaches a user's mountlist, where a wrong
  one mounts nothing.

What was written to the card followed: `emu68.jit`, `emu68.mmu` and
`buptest.fastram_size` — three tokens Emu68 has never read — plus `sd.unit0=0`,
when that option takes `off`, `ro` or `rw`. Also `hdmi_cvt` and three
`framebuffer_*` keys in `config.txt`, for an "RTG resolution" Emu68 does not
take from there.

The profile cards claimed "99 % WHDLoad compatibility", "~800+ MIPS", "512 MB
Fast RAM" and "20+ MB/s" — none measured, none reproducible.

Spec §10 and §89 in the one place a user is most likely to trust ART, and worse
than the ADF equivalents: this screen's output goes onto a card that boots
somebody's real machine.

**Fixed 2026-08-12.** `core/pistorm/` is three modules with 58 tests:

- `hardware.rs` — the matrix the screen models. A setup is **three** choices,
  not one: Amiga → PiStorm board → Raspberry Pi, each filtering the next.
  Everything downstream derives from it — the Emu68 release archive, the
  storage device name in every generated hint, which tokens are meaningful, and
  the notes (CM4 eMMC, 3B/3B+ physical fit, power supply, a Pi that is reported
  working rather than guaranteed).
- `options.rs` — one field per documented `cmdline.txt` token and nothing else.
  `every_profile_is_made_of_real_tokens_only` is the test that keeps it that
  way. Slow-RAM tokens are **dropped**, not hidden, on an A600/A1200: the Emu68
  FAQ's own answer to "my A1200 reports the wrong RAM" is to remove them.
- `firmware.rs` — `config.txt`: kernel, `initramfs`, display presets with their
  real `hdmi_group`/`hdmi_mode` numbers shown on screen, and an overclock that
  is opt-in only and never part of a profile.

Both files are still merged, never regenerated, and
`saving_never_loses_the_parameter_the_pi_boots_by` pins the one that matters.
A card written by an older ART has the three fictional tokens removed on its
next save.

The screen prints the token beside every control and the whole `cmdline.txt`
line beneath them, plus the user's own boot parameters read-only — which is the
only way somebody can see for themselves that a control does what it says.

Source: `ART-brief-pistorm-studio-v2.md` (Emu68 Options and SD_Preparation
docs, pistorm.github.io hardware page and Emu68 FAQ, wiki.amiga.org
Pistorm-500 / Pistorm32-Lite, MultibootOS 2.2). Found by the user, 2026-08-12.

**Closed 2026-08-13 after the fix round** (`ART-prompt-pistorm-studio-fixes.md`),
which reviewed the above and closed what it left:

- **The Kickstart goes through ROM Manager.** Every ROM-shaped file on the card
  is identified by checksum and labelled with its version and the machines it is
  for; a ROM can be picked from anywhere on the PC, identified, and copied onto
  the card under a confirmed name. Unrecognised stays a label, never a refusal —
  a custom or byte-swapped image copies like any other. The picker is shown to
  *everyone*; Power Mode adds free typing rather than replacing it, which is the
  way round the first version had it.
- **The kernel states its version.** `$VER: Emu68 <version> <date> git:<hash>`,
  which Emu68's own build assembles in `cmake/verstring.cmake` and compiles in.
  A card whose image says nothing reports "unknown" rather than a guess.
- **Named firmware sets are managed, not just listed**: create, duplicate,
  rename and activate, each through preview → backup → write. Deleting one is
  [ART-092](#open), deliberately.
- ART-091 above, found in the same review.

Still owed and named as such: fetching a kernel update from GitHub is not built
(F4's second half), and the WiFi panel stays declared rather than offered until
G14 can edit the volumes it would write into.

**Not yet verified on real hardware.** No card built by this screen has been
booted. The tokens are what the documentation says; whether a given machine
likes a given set of them is a separate claim ART is not making.

**ART-089** 🟠 **Session restore could not work, and destroyed the session it was meant to restore** — *fixed 2026-08-12*
`src/pages/FileManager.tsx` · `src/App.tsx` · `App` starts the settings store
loading and deliberately does not await it — "non-blocking async
initializations", which is right for a theme and a language. The Files screen
mounts in parallel and reads `filesSession` off the store's **defaults**, which
is `null`.

So the cold start found no session, opened the first enumerated drive, and then
the persistence effect — doing exactly its job — wrote *that* over the real
session. Two tabs on `D:\…\test` became one tab on `C:\`, on disk, before the
user had touched anything.

Both halves of that are worth naming: the feature could not work at all, and it
was **destructive** — the failure erased the thing it failed to read. A
restore that merely did nothing would have been a bug; this lost data.

Found by reading `%APPDATA%\com.amiga-retro-toolkit.desktop\settings.json`
after the application had been reloaded, and noticing the saved session had
shrunk. No test could have caught it: the round-trip is correct
(`paneSession.test.ts` proves it), the tab model is correct, and the defect
lives entirely in *when* the screen asked.

Fixed by keying the cold start on the store's own `loaded` flag rather than on
mount, with `sessionRestored` keeping it once-only. **This is the second time
in one day that a defect survived a green suite and was found only by running
the application** — see [ART-082](#open).

**ART-088** 🟡 **The volume writer deletes a delete-protected entry without noticing the bit** — *fixed 2026-08-13*
`src-tauri/src/core/volume/write/mod.rs::delete` · AmigaDOS refuses to delete a
file whose `d` protection bit is clear — that is what the bit is *for*, and
WHDLoad slaves and system files routinely have it set that way. ART's `delete`
checks that a directory is empty and nothing else; the protection field is read
for the attributes dialog and for `.uaem` sidecars, and ignored here.

So a `Delete` in the file manager removes an entry that the Amiga itself would
have protected, and the user's own `[Confirmation]` settings — which keep
"overwrite read-only" on — have nothing to attach to.

**Half-fixed 2026-08-12** (phase 2b task 7): the file manager now reads the bit
off `PanelEntry.attrs` and asks a third time before deleting a protected entry,
naming it (`isDeleteProtected` in `@/lib/protection`, 6 tests). That is a
confirmation, not a guard — anything that calls `delete` without going through
that screen still deletes silently.

The real fix belongs in the writer: refuse unless an explicit override is
passed, the same shape `SAFE_CREATE` already has for "creating never replaces".
Worth doing alongside the same question for **overwrite** — `volume_put_file`
does not check the `w` bit either.

**Fixed 2026-08-13.** `VolumeWriter::delete` honours the bit and refuses,
naming the entry; `delete_with(.., DeleteProtection::Override)` is the way past
it, so a caller has to *ask* rather than get there by not thinking about it.
The default is `Honour`, so anything that has not been taught the question gets
the safe answer.

The file manager's three confirmations now reach the writer: `volume_delete`
and `volume_delete_many` take the answer as an argument, and the Files screen
sends it only where it has actually shown the question. Move asks **before**
the copy half rather than after — a move is a copy and then a delete, so a
refusal at the end would have left the user with both a duplicate and an error.
An icon deleted alongside the object it belongs to inherits the answer, because
asking again about `Turrican.info` after the user has agreed to delete
`Turrican` is a question with no new information in it.

`replace_file` passes `Override` deliberately: it deletes only so the same name
can carry new bytes, and AmigaDOS governs overwriting with the `w` bit rather
than `d`. **ART does not check `w` yet either** — that half is now
[ART-094](#open) rather than a sentence at the bottom of this one.

Tests: `a_delete_protected_entry_is_refused_by_name` (and the entry is still
there afterwards), `a_delete_protected_entry_goes_when_the_user_has_been_asked`,
`an_ordinary_file_still_deletes_without_being_asked_twice`, and
`the_delete_bit_is_read_the_way_amigados_stores_it` — RWED are stored inverted,
and getting that backwards would refuse every ordinary delete and allow every
protected one.

Found while auditing the brief's §3.4 confirmations against what ART actually
does.

**ART-087** 🔵 **Space marks a row but does not compute a directory's size**
`src/pages/FileManager.tsx` · `src/lib/selection.ts::spaceToggle` · The brief
(§3.2) asks for Total Commander's `CountSpace=1`: Space on a **directory**
marks it *and* walks it, replacing the `<DIR>` in the Size column with the real
total. ART marks; it does not count.

The reason is the phase's own rule. There is no primitive to count with:
`panel_list_local` lists one level, `scan_collection_directory` looks for Amiga
files rather than totalling bytes, and `volume_plan_copy` computes a size only
against a destination volume. A recursive walk is new engine capability — small
and read-only, but new — and phase 2b's plan says a gap found on the way is
filed rather than smuggled in.

Fixing it means one command per side of the fence: a depth-limited local walk
(the same guards `scan_collection_directory` already has — bounded depth, no
symlink following) and a volume-side directory total, both as jobs, because a
directory of forty thousand files must not block the command thread and must
be stoppable. The Size column then needs a third state — not just a number or
`<DIR>`, but "counting…".

Found while building phase 2b task 5.

**ART-086** 🔵 **Every path in Settings had to be typed by hand** — *fixed 2026-08-12*
`src/pages/Settings.tsx` · "WinUAE Path" and "Collection Directory" were plain
`<input>`s with a placeholder. ART already opens native pickers everywhere else
(`@tauri-apps/plugin-dialog`'s `open`, used by the Files screen, both studios
and the WHDLoad installer); these were simply the fields that never got it
wired.

Fixed with a `PathField` — the box, a **Browse…** button that opens the right
kind of picker (file for an executable, folder for a directory), and a clear
button. The box stays editable, because a path can be pasted and somebody who
knows where they are going types faster than they click. Empty is stored as
`null` rather than `""`, so "cleared" and "never set" stay the same thing,
which is what every caller's `?? fallback` already assumed.

The same component now serves the two **default folders** the Files screen
opens in, which is what made the fix worth doing today rather than eventually:
a setting the user is expected to fill in is a setting that needs a picker.

Found by the user in the running application, 2026-08-12.

**ART-085** 🟡 **A studio forgets the image it had open the moment you leave the screen**
`src/pages/AdfBrowser.tsx`, `HardDiskStudio.tsx`, and every other studio ·
Each holds its open file in a local `useState`. Navigating away unmounts the
component and the state goes with it, so coming back gives the empty
"open an .adf to begin" page again — while the Dashboard's Recent list, which
*is* persisted (SQLite `recent_files`), still shows the file that was open a
second ago. That contrast is what makes it read as a fault rather than as a
design.

What is missing is a notion of **the object ART currently has open**, shared
across screens: the Files panes, the studios and the workflow engine all
address the same kinds of thing and none of them can tell the others what it
is looking at. Phase 2b task 6 already has to persist per-pane paths for
session restore, and this is the same question asked once for the whole
application rather than once per screen — worth designing together rather than
bolting a `useRef` onto each studio.

Found by the user in the running application, 2026-08-12.

**ART-084** 🟠 **An HDF created as PFS3 or SFS is a DosType with no filesystem behind it, and an Amiga cannot mount it** — *fixed 2026-08-12*
`core/hdf.rs::create_hdf` · `core/rdb.rs::create_rdb_layout` ·
`src/pages/HardDiskStudio.tsx` · The New HDF wizard's third step is "Choose
Amiga Filesystem", and it offers **PFS3-AIO as the default, badged
"⭐ Recommended (Fast & Safe)"**. What `create_hdf` actually writes is the RDSK
block, the PART blocks, and nothing else:

- **No filesystem is created**, for any choice. The partition has no root
  block and no bitmap. For DOS\1 / DOS\3 that is correct and normal — a real
  disk is partitioned on one machine and `Format`ted on the Amiga — but the
  wizard's wording does not say so.
- **No driver is embedded in the RDB.** PFS3 and SFS are not in Kickstart;
  they are loaded from FSHD + LSEG blocks inside the RDB, which ART
  deliberately does not write ([ART-025](#fixed), and G4 of the
  [SD gap analysis](sd-appliance-gap-analysis.md)). `IDNAME_FSHD` and
  `IDNAME_LSEG` exist in `core/rdb.rs` as constants and are referenced by
  nothing.

So the recommended, default option produces an image whose partition an Amiga
does not see at all. That is exactly the "don't claim support that isn't
implemented" rule (spec §10, §89) broken in the one place a user is most
likely to trust ART.

**Half-fixed 2026-08-12:** the wizard now shows the limitation in the dialog,
in both languages, before the image is made
(`hardDisk.modal.warnNoDriver`, `hdfSizeWarning` in `@/lib/hdfSize`, tested).
That turns a false claim into a stated one; it is not the fix.

**And confirmed from outside, the same day.** `hst-imager` — the tool both
existing PiStorm imagers stand on — **refuses** to do what ART's wizard does:

```text
[ERR] File system with DOS type 'PDS3' not found in Rigid Disk Block
```

It will not add a `PDS` partition until the driver is in the RDB. ART is not
being conservative in calling this a defect; it is being late.

**Reading half now built (SD-1, 2026-08-12).** `core/rdb.rs` reads the
FSHD/LSEG chain, and `partitionsMissingDriver` (`@/lib/rdbDrivers`, 6 tests)
answers the question this issue is about: the Hard Disk studio now *names the
partition* that will not mount, instead of warning in general. Verified against
an image `hst-imager` built, agreeing with `rdbtool` on version, size and
seg-list block. The writing half — putting a driver *into* an RDB ART creates —
is still owed. The fix is G4 —
segment-splitting a user-supplied `pfs3aio` binary into an LSEG chain,
checksumming it and wiring `DosType → SegListBlocks` — which is scheduled as
SD-1 and verifiable against `rdbtool`, an oracle that already exists.

**Fixed 2026-08-12 — the writing half (G4 complete).** `create_rdb_layout` now
takes `&[FileSystemSpec]` and writes, per driver, an FSHD block
(`DosType`, `Version`, `PatchFlags = 0x10`, `dn_SegListBlock`,
`dn_GlobalVec = -1`) followed by its own LSEG chain, 492 bytes a block, with
`SummedLongs` declaring how much of the last block is real — the same field the
reader already relied on from the other side. `create_hdf` and the `hdf_create`
command thread it through; the wizard's step 4 asks for the driver when the
chosen filesystem is one Kickstart does not have (`@/lib/fsDriver`, 8 tests).

Rust tests: `a_written_driver_reads_back_with_its_version_and_exact_size`,
`the_drivers_bytes_survive_the_trip_verbatim`, `two_drivers_are_chained_and_both_come_back`,
`a_disk_with_no_drivers_says_so_rather_than_pointing_at_block_zero`,
`a_driver_too_big_for_the_reserved_area_is_refused_with_the_numbers`.

**Verified from outside, both directions.** `rdbtool` reads an image ART built
carrying the real `pfs3aio`, reporting `PDS3/0x50445303 version=19.2
size=59120` — the same three numbers `hst-imager`'s own image reports — and
`rdbtool fsget` extracts the driver back out **SHA-256-identical** to the file
that went in. `hst.imager info` reads the same image and lays out the RDB and
the `PDS\3` partition correctly. ART's reader and writer agreeing with each
other would have proved nothing; this is the half that answers to somebody else.

The version is no longer asked for: `version_from_ver_string` reads it out of
the driver's own `$VER:` string, and refuses rather than defaulting when the
binary says nothing. 0.0 is not a harmless default — AmigaOS keeps the higher
of the version in the RDB and the one already loaded, so a driver claiming 0.0
is one that never runs.

`hardDisk.modal.warnNoDriver` is gone with the limitation it described.
`hdfSizeWarning` no longer answers the driver question at all — it is a size
function, and the answer now depends on what the user picked.

Found by the user in the running application, 2026-08-12.

**ART-083** 🟡 **The New HDF wizard capped disk size at 8 GB, and nothing in the engine asked it to** — *fixed 2026-08-12*
`src/pages/HardDiskStudio.tsx` · The five size buttons (500 MB … 8 GB) were
the whole of what could be chosen. `create_rdb_layout` refuses below 10 MB and
then only when the cylinder count will not fit a `u32` — at 516,096 bytes per
cylinder, a limit measured in petabytes. The 8 GB ceiling was a list of five
numbers in a component.

Fixed by a "Custom…" size that is typed, parsed and validated in
`@/lib/hdfSize` (13 tests): a comma decimal separator (the user's own locale),
the engine's own 10 MB floor as the only lower bound, no upper bound, and a
fraction of a megabyte **refused rather than rounded away** — this is the one
number in the dialog that cannot be changed after the image exists. A
partition past 4 GB now carries a warning about TD64/NSD rather than a
refusal: the image may be for an emulator, or for a machine that has both.

Found by the user in the running application, 2026-08-12.

**ART-082** 🟠 **The Files panes filled the window; their listings did not** — *fixed 2026-08-12*
`src/pages/FileManager.css` · Phase 2b task 1 made the panes fill the window
by construction and was checked by tests, which cannot see a pane. A
`max-height: 420px` on `.tc-row-list` — left from when the commander was a
widget on a page — survived it, so the *pane* grew to the window and the
*listing inside it* stayed 420 CSS pixels tall. On a maximized 4K window that
is twenty rows above six hundred pixels of dead pane, with the status line
stranded in the middle of it: acceptance point 1 of the phase brief
("panes always filling; no dead space") failing in the most visible way
possible.

Fixed by deleting the `max-height`; the flex rule further down the file was
always the thing that should have decided the height.

**Worth more than the fix:** this is the first defect [ART-062](#open) has
actually cost. Two tasks' worth of layout work went in green on 178 frontend
tests, and the first human look at the running screen found this in seconds.
Nothing in a jsdom test has a viewport height.

**ART-081** 🟡 **A single file cannot be moved between two images, because the primitive underneath addresses a directory**
`src-tauri/src/commands/volume_write.rs::volume_copy_between` ·
`src/lib/movePlan.ts` · The command takes a *directory* block at both ends and
copies that directory's tree. F5 on a lone file between two images already
passes the pane's own `dirBlock` and therefore copies the whole folder the file
happens to be in — noisy, but harmless, because F5 deletes nothing. F6 (Move)
cannot use that: it would copy twenty files, delete the one that was marked, and
report a move.

So `planMove` refuses it by name (`files.move.refuseFileBetweenImages`), and
the test `refuses a lone file between two images` pins the refusal. Folders
move between images; files move out to a host folder; a file between two images
is copied with F5 for now.

Fixing it means a command that stages one *entry* — extract to a scratch path,
then `put_file` into the destination volume, inside one write session at each
end so the backup and journal guarantees hold — which is the same missing
primitive ART-064 needs for batching, and it should be built once for both.
The F5 whole-folder surprise above is worth fixing in the same pass.

Found while building F6 in phase 2b task 3; recorded rather than smuggled into
a UI task, which is what that task's plan requires.

**ART-080** 🔵 **ART cannot delete a file on the user's own disk, so nothing can be moved *off* a host folder**
`src/lib/movePlan.ts` · `src-tauri/src/commands/panel.rs` · Every delete ART
owns goes *into* a disk image through `core/volume/write`; there is no command
that removes a file from the user's own filesystem, and that is deliberate —
Explorer does that job and a two-pane commander that can silently delete host
files is a much larger safety surface than one that cannot.

The consequence, once F6 became Move: the most obvious move of all — drag a
game off `D:\downloads` and onto a floppy image — is a copy, and the original
stays. F6 says so (`files.move.refuseLocalSource`) instead of being disabled
with no explanation, and points at F5.

Fixing it is not a line of code but a decision: a host-side delete needs its own
`Safety` class, its own confirmation, its own oplog entry, and a policy on
recycle bin versus unlink. Worth doing deliberately, if at all.

Found while building F6 in phase 2b task 3.

**ART-078** 🟡 **An AmigaOS CD's protection bits and file comments are lost, because Rock Ridge and the Amiga `AS` entry are not read**
`core/iso/` · ART reads ISO9660 and prefers Joliet when a disc carries it.
Neither carries what an Amiga CD actually says about its files: protection bits
(`HSPARWED`) and the file comment live in the **Amiga `AS` System Use entry**, a
Rock Ridge-style extension, and ART reads no System Use area at all. Two
consequences, both quiet:

- **A WHDLoad-era disc loses its slave's `S` and `P` bits on the way out.** A
  game copied off a CD onto an HDF can arrive with the right bytes and the
  wrong protection, which is a game that starts and then does not work — the
  same class of failure §7.2 records for archives, where ART *does* carry the
  bits through `.uaem` sidecars.
- **A Unix-mastered disc with no Joliet descriptor falls back to uppercase
  8.3 names.** Rock Ridge is where its real names are, so `MyGame.info`
  becomes `MYGAME.INF` and the icon stops matching the drawer.

Neither is a regression: nothing ever claimed to read them, `FEATURES.md` and
`format-support-matrix.md` both say so, and the disc still copies. Fixing it
means reading the System Use area after each directory record, handling the
`SP`/`CE`/`NM` continuation entries, and mapping `AS` onto the same
`Protection` type `core/volume/write` already has — at which point the
existing `.uaem` writer carries the bits out to a folder for free.

Found while closing Phase 2a; recorded rather than fixed because it is a
format layer of its own, not an omission in the one that landed.

**ART-073** 🟡 **`delete_many`'s all-or-nothing guarantee only holds for the whole-file strategy**
`src-tauri/src/commands/volume_write.rs::delete_many` (line ~505) · The
pre-check (`check_batch_deletable`) runs once, against a read-only listing,
before the writer session opens — for a floppy-sized image (the whole-file
strategy) that is enough: nothing is written until the whole in-memory
result validates, so a batch that cannot fully succeed leaves the file
untouched, and every test in this module exercises that path. On a
block-journal image (a large HDF) each `writer.delete(...)` inside the
session's loop is its own committed, journalled operation, already durable
in the file the instant it returns — there is no whole-image commit step to
refuse at. An error partway through the loop after the pre-check passed
(a name resolving differently a moment later, say) leaves the earlier
deletes in the batch standing rather than none of them, breaking the
all-or-nothing promise the doc comment now qualifies. Reachable in
principle whenever a batch delete runs against an HDF rather than an ADF.
Not reachable through the case-different-name path any more —
`dedupe_case_insensitive` (added in the same pass that found this) closes
that specific trigger — but the underlying strategy gap is still open. Fix
would need the block-journal strategy to buffer its own generation of
deletes behind one commit point the way the whole-file strategy already
does, which is a real design change, not a one-line fix.

**ART-072** 🟡 **Selection collision checks compare names case-sensitively, so `Docs` and `docs` are not caught** — *fixed 2026-08-13*
`src-tauri/src/core/volume/write/copy.rs::HostSelection::check_for_name_collisions`
(line ~489) and `src-tauri/src/commands/archives.rs::prepare_archives`
(line ~350) both keep a `BTreeMap<String, _>` keyed on the name exactly as
given, so two roots (or two archives) that would land under the same drawer
name only collide when they are byte-for-byte identical. AmigaDOS is
case-preserving but case-*insensitive* (the same rule `hash::name_hash`
respects, ART-009/ART-010), so `Docs` and `docs` are two different keys
here but the one directory entry there. In `copy.rs` this means the clean,
named refusal `check_for_name_collisions` exists to give never fires for a
case-different pair; the second root is instead silently skipped later,
along with its whole subtree, replacing one clear "rename one of these"
message with a pile of unexplained "skipped" lines. In `archives.rs` the
same gap means two archives that both unpack to a `Docs`/`docs` drawer are
not refused up front either — `std::fs::rename(&item.content_root,
&destination)` is what fails instead, surfacing a raw OS rename error
rather than the friendly named-collision sentence the exact-match case
already gets. Fix is to key both maps on `name.to_lowercase()` instead of
the name itself, the same change `dedupe_case_insensitive`
(`commands/volume_write.rs`) just made for batch deletes.

**Fixed 2026-08-13**, both maps, keeping the name as *typed* for the message
so the sentence still says `Docs` rather than `docs`. Test:
`two_roots_differing_only_in_case_are_refused_too`.

**ART-071** 🟡 **A selection of only symlinks copies nothing and reports success** — *fixed 2026-08-13*
`src-tauri/src/core/volume/write/copy.rs::HostSelection::entries_capped`
(line ~527) · A root that is a symlink is skipped with a bare `continue` —
correct on its own (a link out of the pick would copy in something the user
did not select, the same rule `HostFolder`'s own `walk` applies mid-tree at
line ~414), but nothing records that it happened. If every root in a
selection is a symlink, `entries()` returns an empty `Vec` with no entry in
`CopyReport.skipped`, so `copy_into_volume` runs its loop zero times and
returns a report where `cancelled` is false and `skipped` is empty —
`CopyReport::is_complete()` reads that as a clean success. The user picked
one or more things, ART copied none of them, and every signal available to
the UI says the copy worked. Fix is for `entries_capped` to push a skipped
entry (as `walk`'s sibling check already could, but currently does not
either) rather than silently dropping the root.

**Fixed 2026-08-13** by a `CopySource::skipped_sources` the engine merges into
the report *before* the loop — which is the point, since a source that declined
everything runs the loop zero times. Default-empty on the trait, so no other
source is affected. Tests:
`a_selection_of_nothing_but_shortcuts_says_so_rather_than_reporting_success`
and `a_selection_of_ordinary_files_declines_nothing`.

**ART-070** 🔵 **`refresh(side)` moves keyboard focus to the pane it refreshed**
`src/pages/FileManager.tsx` — `openLocal`, `openAdf`, `openHdf` and
`openVolume` (lines 552, 585, 600, 638) each end with `resetSelection(side);
setFocused(side);`, and `refresh(side)` (line ~723) calls whichever of them
matches the pane's kind. F5's copy-in path calls `refresh(to)` on the
*destination* pane once the job result arrives, so after a copy, keyboard
focus silently jumps from the source pane (where the user was working) to
the destination — Total Commander leaves focus on the source. Cosmetic, not
a safety issue: nothing is acted on incorrectly, the next F-key press just
lands on the pane the user was not looking at. Fix would need `refresh` to
take an explicit "keep focus here" flag, or for its callers to restore
`focused` afterward rather than trusting the open-pane functions' own
default.

**ART-069** 🔵 **No frontend test renders `FileManager.tsx`**
`src/pages/FileManager.tsx` · It calls Tauri commands (`onVolumeWriteResult`,
`onJobProgress`, panel listing, …) on mount, which is why every phase-1a
frontend test extracts a pure function or hook instead of rendering the
page — `@/lib/selection`, `@/lib/functionKeyPlan` (added closing finding 4
of the phase-1a whole-branch review), `usePaneTab`/`isShortcutBlocked` in
`FunctionKeys.tsx`, and so on. Each extraction is real, tested logic, but
none of them proves the page actually *wires* the extracted piece
correctly — that an F-key's `run` reads the same `target` its `enabled`
was computed from, that a click handler calls the selection function it
looks like it calls, that the two `useEffect` result listeners registered
at mount really are registered before any button can start a job. Closing
this needs either a mock of the Tauri IPC surface (`@tauri-apps/api/core`'s
`invoke`, `@tauri-apps/api/event`'s `listen`) sufficient to render the page
in a test, or splitting `FileManager.tsx` into smaller components each
small enough to mock individually — a real task, not a quick fix.

**ART-068** 🔵 **The filter box tells "empty" from "no match" by comparing entry counts, not a dedicated flag**
`src/pages/FileManager.tsx` (~line 2260) · The "a mask matching nothing says so"
message picks between `files.pane.filterNoMatch` and `files.pane.empty` with
`filter.trim() !== "" && state.entries.length > 0` — a mask is active *and*
the pane's unfiltered listing was non-empty. That reads correctly today
because `filterEntries` (`src/lib/mask.ts`) never changes the unfiltered
count and the mask resets on navigation, but the distinction the UI actually
wants — "did the mask remove everything?" — is being inferred from two
numbers matching a shape, not read off a value that says so directly. A
future change to either side (a mask that also hid something for a different
reason, a pane whose unfiltered count is not `state.entries` any more) could
silently start showing "this folder is empty" for a folder that only looks
empty because of the filter, which reads as ART having failed to open the
disk. No test exercises the two counts diverging from what the boolean they
stand in for would say. Fix is mechanical: have `filterEntries` (or a sibling)
return whether it removed anything, and key the message off that instead of
re-deriving it at the call site.

**ART-067** 🔵 **A batch archive install can't be stopped mid-archive**
`commands/archives.rs::prepare_archives` (line ~316) · `unpack_for_install(archive, &NoProgress)`
is called with `&NoProgress` regardless of which caller is running — including
`install_archives`, which is on a real job with a real `ProgressSink` one
call up the stack. `is_cancelled()` is checked once per archive, at the top
of the loop (line ~310), so Stop is honoured *between* archives but not
during one — a batch of five archives where the third is large leaves Stop
unresponsive for however long that one extraction takes. Not a data-safety
issue (§54's "never mid-write" is still honoured: nothing is written to the
volume until every archive is unpacked and staged), just a slower response
to Stop than the rest of the job queue gives. Fix is to thread the real
`progress` sink into `unpack_for_install` instead of a fixed `NoProgress`.

**ART-066** 🟡 **`archives_plan_install` unpacks the whole batch on the Tauri command thread**
`commands/archives.rs::archives_plan_install` (line ~104) · Every other
multi-step operation in this module runs through [`spawn_job`](../src-tauri/src/commands/jobs.rs)
so it can report progress and be cancelled (§54, §55) — `archives_install`
does. `archives_plan_install` is a plain `#[tauri::command]`: it calls
`build_plan` → `prepare_archives(archives, staging.path(), &NoProgress)`
straight in the command handler, which extracts every archive in the
selection before returning. A plan over several large archives blocks the
Tauri command thread for the whole unpack, with no progress and no way to
stop it, where the read-only plan step for every other batched operation in
this file manager returns as soon as the (much cheaper) cost is computed.
Not data-unsafe — nothing is written — just unresponsive. Needs the same
`spawn_job` treatment `archives_install` already has, returning a job id the
UI awaits the way it awaits every other plan today would be a larger change
than this note; recorded here rather than fixed under Task 8's scope.

**ART-065** 🟡 **Volume→local multi-select is several concurrent operations, not one**
`src/pages/FileManager.tsx::copySelectionTo` (line ~1090) · When the source
pane is a volume and more than one entry is selected for extraction to a
local folder, each entry becomes its own concurrent operation inside a
single `Promise.all` — a subdirectory goes through its own `volumeCopyOut`
job (awaited individually inside the map callback), a plain file through its
own direct `volumeExtractTo` call — rather than the one atomic, one-job
operation `volumeCopyInMany` (local→volume) and `volumeCopyBetween`
(volume→volume, staged) both give their directions. Each individual
extraction is still safe on its own — every write is the same
backup-and-validate pipeline as ever — but the *batch* has none of the
all-or-nothing guarantee the other two directions do: a selection of ten
entries where the seventh fails to extract leaves the first six on disk and
the last three silently never attempted, with no report tying the partial
result back to "this was one selection." Needs a batched extract primitive
(`volume_extract_many`, mirroring `volume_copy_in_many`'s shape) rather than
fixing the concurrency at the call site.

**ART-064** 🟡 **Volume→volume multi-select refuses rather than batching**
`src/pages/FileManager.tsx::copySelectionTo` (line ~1124) · "Two volumes and
more than one entry: not supported yet" — `setError(t("files.err.batchBetweenVolumes"))`
("Copying several entries between two images at once is not supported yet —
copy them one at a time."). Not a defect in the sense of wrong behaviour: the
refusal is explicit, immediate, and names the reason, which is exactly what
§89 asks for when a case is not handled. It is recorded here because it is
the one direction of the four (local→volume, volume→local, volume→volume
single-entry, volume→volume batch) Task 8's roadmap self-review calls out by
name as deliberately not built: there is no `volume_copy_between_many`
primitive to build a batch on top of — `volume_copy_between` (the command
`e3035cf` added end-to-end coverage for) stages exactly one directory tree
through a temp folder per call, and doing several would mean either several
separate stage-and-insert round trips (no shared atomicity, the same
weakness as ART-065) or a `HostSelection`-shaped staging step that does not
exist yet on the extract side. Needs its own task, not a quick fix — see
`ART-065` for the sibling gap it would need to close at the same time.

**ART-062** 🔵 **No language has been checked on screen**
`src/i18n/tr.json`, `src/i18n/en.json` · Every Turkish string landed this phase
was verified by `pnpm test`'s key-parity check and by reading the JSON — never
by opening the running application and looking at a screen. Several Turkish
strings are substantially longer than their English originals and sit in tight
controls, so the check that remains is visual, not automatable:

| Key | English | Turkish | Growth |
|---|---|---|---|
| `pistorm.saveSync` | "Save & Sync PiStorm SD" | "PiStorm SD'yi Kaydet ve Eşitle" | +36% |
| `hardDisk.bootablePri` | "Bootable (Pri {{n}})" | "Önyüklenebilir (Öncelik {{n}})" | +50% |
| `pistorm.profile.classic.badge` | "Cycle-Exact & Demos" | "Çevrim Hassasiyetli ve Demolar" | +58% |
| FileManager function-key label | "View" | "Görüntüle" | 4 → 9 characters |
| FileManager function-key label | "Grid" | "Izgara" | +50% |
| job status | "Done" | "Tamamlandı" | +150% |
| job status | "Failed" | "Başarısız" | +50% |

The function-key bar (`src/components/files/FunctionKeys.tsx`) was inspected
in source rather than run: its container carries `flexWrap: "wrap"` and each
button `flex: "1 1 90px"`, and `.btn` in `src/styles/global.css` carries no
`text-overflow` rule (only `.file-row-name` does), so the bar should wrap
rather than clip. That makes it the most likely of the rows above to look
merely cramped rather than the most likely to break outright — but nobody has
looked at it in Turkish. Needs an actual run of `pnpm tauri dev` with the
language switched to Turkish, working through PiStorm, the hard disk screen,
and the Files function-key bar at a few window widths.

**ART-061** 🟡 **`formatAge` is always plural in English** — *fixed 2026-08-13*
`src/lib/sources.ts::formatAge` · Returns `{ key: "aminet.age.weeksAgo", params:
{ n } }` (and the `monthsAgo` sibling) for any `n`, and `aminet.age.weeksAgo`'s
English text is the fixed template `"{{n}} weeks ago"` — so a package uploaded
exactly one week ago reads "1 weeks ago", and one month old reads "1 months
ago". Predates Phase 0b: task 7b's job was to move this string to the `Phrase`
layer faithfully, not to redesign it, and reproducing a known wart rather than
quietly fixing it mid-refactor was the right call — but the wart is real and
now has a name. The Turkish side is unaffected: `"{{n}} hafta önce"` is correct
at any `n`, because Turkish does not inflect the noun after a number. Fixing it
means either a plural-aware key pair (`weekAgo` / `weeksAgo`, chosen by `n
=== 1`) or i18next's own plural key suffix (`_one` / `_other`), English-only —
Turkish needs no equivalent change.

**Fixed 2026-08-13** with `_one`/`_other`, and the part that is easy to miss:
i18next picks the plural form from **`count`** and from nothing else, so
`formatAge` returns `{ count }` rather than `{ n }` now. A phrase that kept `n`
would have rendered the `_other` form at every number — the same bug with more
keys. `yearsAgo` is left alone deliberately: it carries a decimal, and "1.0
year ago" is worse English than what it replaces. Both catalogues carry the
pair; the two Turkish forms are identical to each other, which is correct.

**ART-060** 🔵 **Rust-side error sentences do not translate**
`core/error.rs::CoreError`, `commands/whdload.rs::WhdloadRefusal` · Every
`CoreError` variant's `Display` implementation, and `WhdloadRefusal { reason,
suggestion }`'s two fields, are English sentences written in `core/` and
`commands/`, reaching the UI verbatim regardless of the language chosen in
Settings. `CoreError::user_message()` appends the stable `ART-*` id from
`code()` to the English sentence for exactly this reason (§68) — the id was
always meant to be the stable, quotable part — but nothing today keys off it
to show a translated sentence instead. This is a design question worth
recording, not answering here: `core/` may not depend on the frontend's
`react-i18next` catalogue (CLAUDE.md's core-independence rule), so there are at
least two ways forward — move the sentences into the frontend catalogue, keyed
by `CoreError::code()` / a `WhdloadRefusal` reason code, and have the UI look
them up instead of rendering the string Rust sent; or give `core/` its own
minimal, dependency-free catalogue and have `Display` consult it. The first
keeps `core/` exactly as independent as it is today but means every error path
has to carry a stable key instead of (or alongside) a sentence, and duplicates
some phrasing decisions between Rust and the JSON catalogues. The second keeps
the sentence and its translation next to each other but adds a resource-lookup
concept to a crate that currently has none. Neither is decided here.

**ART-058** 🔵 **A cancelled block-journal copy doesn't tell the user files already landed**
`commands/volume_write.rs::run_copy_in_folder_with` (`WriteStrategy::BlockJournal` branch,
also reached through `run_install`'s `with_volume` closure in `commands/whdload.rs`) · Above
the whole-file limit (16 MiB) each file a copy or install writes is its own committed,
journalled operation, already durable on disk before the next one starts. Cancelling there is
honest about that on purpose — `device.sync()?` runs before the cancellation check, and the
files that landed are correctly left in place rather than rolled back, unlike the whole-file
strategy where cancelling leaves nothing at all. What the user is told is only
`CoreError::Cancelled`'s message, `"operation cancelled"` (`core::error::CoreError`,
`ART-CANCELLED`) — nothing distinguishes that from the whole-file case, so someone who cancels
a large HDF install partway through has no way to learn from the UI that some files are already
on the volume. Not a data-safety defect — nothing here is wrong or at risk — just a message
that undersells what happened. Needs the block-journal branch to carry how much landed (files
copied so far) into a distinct message or a `Cancelled` variant that names it, and a UI string
for that case.

**ART-043** 🟠 **A partition inside a small image is written at the wrong offset**
`commands/volume_write.rs` · The whole-file strategy is chosen by the *file's*
size, but it then builds its `VecDevice` from the whole file and opens the
writer at offset `0`, while the geometry it was given describes a **partition**
that may start megabytes into that file. For any RDB image of 16 MiB or less
this reads and writes volume-relative block numbers as if they were
file-absolute ones: the root block lands in the middle of the partition's data.
In practice the first read then fails with something unhelpful ("block N is not
a directory") rather than corrupting anything, and — since ART-042 — a result
that did somehow get written is refused by the whole-image gate before it
reaches the file. So the user's data is not at risk today; the strategy choice
is simply wrong. The fix is to pick the block-journal strategy whenever the
volume does not start at byte 0 and cover the whole file, or to give the
whole-file branch the partition's slice and write it back at its offset. Needs
its own task and its own fixture (a small RDB image with a formatted
partition) — no test covers it today, which is why it survived this long.

**ART-050** 🟡 **The §92 pre-flight gate does not check bitmap consistency or hash-chain integrity**
`core/adf/validate.rs::validate_image` · `commit_whole_file` (ART-042) refuses
a write when `validate_image` finds a `Problem`, but `validate_image` only
covers the bootblock signature, its checksum, the block count and the root
block's type. It does not walk the bitmap against what is actually allocated
and does not walk a hash chain for consistency, so an operation that leaves
two files owning the same block, or an entry linked into the wrong bucket,
still passes the gate and commits. `CHANGELOG.md`'s entry for ART-042 was
corrected in this pass to stop implying broader coverage than this. Deepening
`validate_image` to catch these is real work, not a flag to flip.

**ART-049** 🟡 **`create.rs`'s oracle hook and `VolumeWriter::open` agree by hand, not by a check**
`core/adf/create.rs` (`oracle_export`), `core/volume/write/mod.rs`
(`VolumeWriter::open`) · The oracle export hook hardcodes `DOS\x01` geometry
while formatting with `FileSystemType::Ffs` — the two are only consistent
because the hook's author chose them to match. `VolumeWriter::open` does not
cross-check the geometry it is handed against the dostype the image's own
bootblock declares, so a caller that passed a geometry for one filesystem
against an image formatted as another would not be refused at the boundary
that is supposed to catch exactly that. No test exercises the disagreement
because nothing in the suite constructs one. Needs a guard in
`VolumeWriter::open` and a fixture that deliberately mismatches geometry and
bootblock.

Missing features are not defects — see [FEATURES.md](FEATURES.md) for what is
not built yet, and [STATUS.md](STATUS.md) for what is scheduled.

Every module with working logic has now been audited. The remaining `core`
modules are stubs that only return `NotImplemented` (`recovery.rs`,
`conversion.rs`, `binary.rs`, `validation.rs`) or hold types with no logic
(`compatibility.rs`) — see [FEATURES.md](FEATURES.md) for their planned state.

Two areas were reviewed and found sound, and are recorded here so nobody
re-audits them without reason:

- `core/analysis.rs` — the hex reader clamps both offset and length, and the
  signature scan guards its window.
- `core/profile.rs` — preset data only, no parsing of untrusted input.

---

## Fixed

### Phase 2a

**ART-079** 🔴 **A 7z archive from any real tool gave one entry another entry's bytes**
`core/archive/sevenz.rs` · Two defects in one read path, both invisible to
ART's own tests and both producing **wrong data with no error**:

1. **Index drift.** `sevenz-rust2`'s `for_each_entries` walks the archive's
   compressed *blocks* first — the entries that carry data, in block order —
   and the streamless ones (directories, empty files) after them. ART counted
   the yields as though they matched `archive().files`, so the moment an
   archive held a directory every index after it pointed at the wrong entry.
   Every archive a real tool writes holds directories; none of ART's fixtures
   did.
2. **A skipped entry was not drained.** A 7z block is one compressed stream
   holding several files end to end, so a file's data begins where the
   previous one's ended. Returning from an unwanted entry without consuming
   its bytes left the block reader short and decoded the *next* wanted file
   from the wrong place — right length, wrong contents. A partial selection is
   the normal case: the gate skips entries that already exist and refuses
   hostile names.

The two together are why `ReadMe.txt` came back holding `Notes.txt`'s text.

→ Entries are matched to indices **by the name the archive stores**, which is
stable under both orderings, and every entry the pass skips has its stream
drained. Verified against an archive the 7-Zip application wrote: ART's
SHA-256 for every entry now equals `7z e -so`'s, through both read paths.

**How it was found, because that is the lesson.** ART's 7z fixtures are built
by ART's own writer, which produces one block per file and no directories —
so they exercised neither defect and passed throughout. Pointing the reader at
a file *another tool* wrote found both in one run. That is the same failure
mode as ART-032…035 and ART-075, for the third time in this project:
`read_foreign_archive_for_oracle_when_asked` and
`read_foreign_c64_for_oracle_when_asked` exist now so it is one command rather
than an idea.

**ART-077** 🟠 **The file manager ignored the object a workflow sent it, so "Open in the file manager" opened nothing**
`src/pages/FileManager.tsx` · Every `Navigate` workflow hands its object over
the same way — a route plus `{ state: { path } }` — and every other studio
reads it on mount (`AdfBrowser`, `CollectionStudio`, `HardDiskStudio`, …).
This screen never did. `iso.browse` (Task 3) pointed at `/files` precisely
because the commander is where a disc belongs, and choosing it left the user
on the file manager with whatever panes they already had and the disc they
dropped nowhere in sight. Nothing failed and nothing said so, which is why it
survived Task 3's review: the route existed, the test asserting
`every_workflow_route_is_a_real_app_route` passed, and the action did nothing.

→ The screen now reads `location.state.path` and opens it in the left pane,
choosing the pane kind from `analyze_paths` — the same detection that offered
the action — rather than from the extension. Found while adding
`archive.browse`, the second workflow to depend on it.

**ART-076** 🟠 **Content-first detection never actually recognised an LHA, and its test could not tell**
`core/detect.rs` · An LHA header carries its compression-method field
(`-lh5-`, `-lhd-`, …) at **offset 2**, after the header length and its
checksum. Detection matched `-l` at offset **0**, which no LHA tool has ever
written — so a real archive fell through to the extension fallback, and one
renamed to `.dat` came back `Unknown`. The whole point of Phase 2a Task 1 was
that a file's name stops deciding what it is; for the format ART was built
around, it still did.

The test that was supposed to cover this, `detects_lha_by_signature`, wrote
five bytes — `-lh5-` — into a file and asserted they were detected. That is
not an archive; it is the method field with nothing around it. Fixture and
code agreed with each other and with no LHA in the world, which is
ART-032…035's shape once again, and the reason the fix is filed rather than
quietly applied.

→ `is_lha_header` checks the field where it belongs: dash, family (`lh`,
`lz`, `pm`), level, dash. The test now builds a real archive with the same
`make_lha_with` the LHA tests use and gives it a `.dat` extension, so it can
only pass on the strength of the signature. `a_method_field_at_offset_zero_is_not_an_lha`
pins the old fixture as *not* an archive. Found while adding ZIP and 7z
signatures next to it (Task 4).

**ART-075** 🟡 **A raw CD image in Mode 2 Form 1 would be misread, and two layers would be wrong together**
`core/detect.rs`, `core/iso/` · ART found a raw 2352-byte-sector track by
probing `CD001` at `0x9311`, which assumes **Mode 1**: 12 bytes of sync, a
4-byte header, then 2048 bytes of user data. A **Mode 2/XA Form 1** sector
carries an 8-byte subheader as well, so its user data starts at 24 and the
signature sits at `0x9319`. ART did not recognise such an image at all — and
worse, the reader took its data offset from the same assumption detection
made, so had one ever been recognised, both layers would have been wrong
*together*. That is the shape behind ART-032…035, and it is the one thing a
green test suite cannot show you. CD32 and mixed-mode discs are written this
way.

→ `SectorLayout::Raw2352Xa` carries the data offset (24) the way the existing
variants carry 0 and 16, so Mode 1 and Mode 2 Form 1 differ in one number
rather than in a branch at every read. Detection probes `0x9319` and reports
`iso9660-raw-xa`; `from_format_hint` turns it back into the layout, pinned by
`detections_format_hints_open_the_right_layout`. **Mode 2 Form 2** — 2324
bytes of audio or video and no filesystem — is refused by reading the submode
byte rather than misread as Form 1 (`refuse_form2`).

Pinned by `a_raw_mode2_xa_track_is_detected_at_0x9319`,
`a_mode1_raw_track_is_not_reported_as_xa`,
`an_xa_form1_disc_reads_exactly_as_a_mode1_one_does` (listing *and* bytes: an
eight-byte slip gives a file that is almost right, which is worse than one
that fails) and `a_mode2_form2_track_is_refused_rather_than_misread`.

The issue also recorded *why* ART's own tests could not close this: no host
mounts a raw track dump, so the 2352 path rested entirely on ART agreeing with
itself. That half is closed too, and separately —
`scripts/iso-oracle-check.py` strips both raw layouts down to 2048-byte
sectors from the *layout's* documented offsets (16 and 24, written in the
script, never read from `core::iso`) and diffs ART's listing and every file's
SHA-256 against 7-Zip's. Both raw fixtures now have an independent
implementation agreeing with them.

### Phase 1a

**ART-074** 🟠 **An accented filename came back corrupted**
`core/adf/bcpl.rs` · AmigaDOS stores strings as Latin-1, one byte per
character. `core/volume/write/dir.rs::put_name` knew that and encoded
correctly — its doc comment even said `write_bcpl_string` "would store `ü` as
two characters and the name would come back wrong on a real Amiga" — but it
fixed the problem locally instead of at the source, and left the read path
alone. `read_bcpl_string` used `String::from_utf8_lossy`, so `Grüße` (bytes
`47 72 FC DF 65`) read back with two replacement characters: `FC` and `DF` are
not valid UTF-8 lead bytes. `dir.rs::name_of` sits three lines from
`put_name` and disagreed with it about the encoding.

Two more callers were wrong in the other direction: `create.rs`'s volume name
and `rdb.rs`'s drive name went through `write_bcpl_string`, which used
`s.as_bytes()` — UTF-8 — so a non-ASCII volume name was written as byte pairs
a real Amiga renders as mojibake. Those two were *self*-consistent with the
old reader, which is why nothing failed: the same shape as ART-032 through
ART-035, where the reader and the writer share a mistake and the suite stays
green. Every name in every test was ASCII. ART-041 had already established
that Latin-1 names are supported, with `Grüße vom Süden` as its own example.

→ Both directions of `bcpl.rs` are now Latin-1, a plain cast each way because
Latin-1 is exactly the first 256 Unicode code points. A character above
`U+00FF` has no byte and becomes `?` rather than a pair an Amiga would show as
two wrong characters. `write_bcpl_string` also encodes before truncating, so a
field limit can no longer cut a character in half. Pinned by
`an_accented_name_survives_a_round_trip`,
`a_name_written_as_latin1_elsewhere_reads_back_intact` and
`truncation_counts_characters_and_never_splits_one`. One existing test,
`bcpl_string_lossy_on_non_utf8`, asserted only `chars().count() == 3` and so
passed under either encoding while claiming to prove replacement characters —
it now asserts the actual string. Found by the whole-branch reviewer chasing a
case-folding question in `delete_many`.


### Phase 0b

**ART-047** 🔵 **Dead code that clippy cannot see**
`core/adf/blocks.rs:81-99` · `block_slice`, `block_slice_mut`, `read_u32_at`
and `write_u32_at` had no callers left outside their own tests —
`core/adf/mutate.rs` was the last production caller, and task 10 (`fbd35ef`)
deleted it. `lib.rs`'s `#![allow(dead_code)]` (CLAUDE.md's one permitted
blanket allow) meant clippy did not flag them. Either give them a real caller
or remove them; audited bounds-checking code that production no longer reaches
is exactly the kind of gap ART-020 exists to stop CI from hiding.
→ Split the decision by what each helper is for. `core/adf/validate.rs::validate`
(~line 191) used to bounds-check `root_off` by hand and then index
`image[root_off..]` directly — the exact pattern this module exists to
replace — so it now calls `blocks::read_u32_at(image, root_block, 0)`, which
bounds-checks through `block_slice` internally. That gives both a real,
reachable production caller: `commands/volume_write.rs`'s §92 pre-flight gate
calls `validate_image` → `validate` before every whole-file commit.
`block_slice_mut` and `write_u32_at`, the write-side pair, are deleted rather
than kept for a caller that does not exist: `core/adf/mutate.rs` was their
only one, and the writer that replaced it, `core/volume/write`'s `BlockSet`
(`layout.rs`), stages each touched block in a `BTreeMap<u32, Vec<u8>>` for the
journal instead of slicing a whole-image buffer — a genuinely different
shape, not an oversight, and no surviving write path indexes a whole image by
block number. No bounds check was loosened to make anything fit; the deleted
functions' own bounds-checking arithmetic (`checked_mul`, the containment
check) is unchanged in the two that remain. `CLAUDE.md`'s bounds-checking
paragraph still names `block_slice_mut` and `write_u32_at` and needs a reword
to drop them. Pinned by `block_access_rejects_out_of_range_numbers` and
`block_word_access_is_bounds_checked` (`core/adf/blocks.rs`, the latter now
read-only), plus `validate.rs`'s existing `wrong_root_type_is_problem` and
`tiny_image_is_problem`, which exercise the new call path.

**ART-048** 🔵 **A source comment still described a module that no longer exists**
`commands/adf.rs:369` (a test's doc comment) · Written for task 8's routing
tests, before `core/adf/mutate` was deleted in task 10 (`fbd35ef`): "a later
task deleting `core/adf/mutate` would be unsafe" described something already
done. `docs/architecture.md`'s reference to `mutate_disk_file` carried the
same staleness and was corrected in an earlier pass — the one exception to
"intent docs are not rewritten" that CLAUDE.md names, because the module it
pointed at is gone.
→ Reworded to state what the test actually guards now: that all four
commands land on `core/volume`, ART's only filesystem writer, now that
`core/adf/mutate` is gone. A tree-wide search
(`grep -rn "mutate_disk_file\|adf/mutate" src src-tauri/src`) turned up no
other production code depending on the retired module — every remaining hit
is a past-tense comment recalling what it replaced.

**ART-051** 🟡 **`FEATURES.md` carries raw control bytes and git treats it as binary**
`docs/FEATURES.md` (the DosType write-matrix table, ~line 140) · Eight bytes
in the range `0x00`–`0x07` were embedded in the table as literal control
characters where the escaped text `\x00` … `\x07` was intended — the same
escaped style the rest of the file uses. They rendered as empty backticks, and
git classified the file as binary, so every change to it showed as
`Bin N -> M bytes` with no reviewable diff.
→ Each of the eight raw bytes replaced with its escaped text
(`python -c "d=open('docs/FEATURES.md','rb').read(); ..."` found the exact
offsets). A byte-for-byte alignment of the old and new file confirms those
eight positions are the *only* difference — everything else, including the
CRLF line endings, is untouched. `git diff --stat` for *this* commit still
prints `Bin 14510 -> 14261 bytes`: git classifies either side of a diff pair
as binary if it contains one raw NUL byte, regardless of what the other side
looks like, and the pre-fix blob has exactly one (the `DOS\x00` cell) —
confirmed by reproducing the same `Bin` notation on a two-line synthetic
before/after pair with `git diff --no-index`, including with `-a`/`--text`
forced. That is a property of diffing *against the old, binary-flagged blob*,
not of the fixed file: every diff of `docs/FEATURES.md` from this commit
onward, once neither side has a raw NUL, renders as ordinary text.

**ART-063** 🟠 **ART could not write a disk an Amiga would boot from**
`core/adf/create.rs`, `core/adf/bootcode.rs` · The `bootable` flag wrote
`0x4E 0x75` — a bare `RTS` — at offset 12 and nothing else. Kickstart's `strap`
validates the boot block's `DOS` signature and checksum, jumps to offset 12,
and requires the code there to return `D0 = 0` with `A0` holding an address to
jump to; **a non-zero `D0` raises a system alert and reboots**. An `RTS`
returned with whatever `D0` already held, so nothing was ever loaded. Invisible
to every test ART had, and to the amitools oracle, because both only ask
whether the boot block is *well-formed* — `xdftool` reported `bootable: True`
for the RTS stub too. Only Kickstart can answer whether the code runs.
→ `core/adf/bootcode.rs` assembles what the contract asks for: read `ExecBase`
from absolute address 4 (guaranteed, unlike `A6` on entry, which is convention),
`FindResident("dos.library")` at exec LVO −96, take `rt_Init` at offset 22 of
the returned `struct Resident`, return `D0 = 0` with `A0` pointing at it. ART's
own implementation, written from the documented contract and the published LVO
table — Commodore's boot block is copyrighted and ART ships no Amiga content,
ever. The two relative displacements are computed from the layout rather than
hand-counted, because miscounting either produces a disk that hangs a real
machine while every test still passes; seven tests in `bootcode::tests` pin
each landmark and both displacements independently. **Verified by booting
`test/art-bootable-test.adf` on 2026-08-11.** Note the scope: a disk that
boots is not a disk that boots to Workbench — DOS then wants
`S/Startup-Sequence` and the `c/`, `libs/` and `l/` contents behind it, which
are AmigaOS content ART cannot supply. Reaching a CLI prompt is the whole
claim, and `info.bootable` now means what it says.

### Phase 0a

**ART-059** 🔵 **A flaky test could fail CI at random**
`net/http_mirror.rs` (test helpers) · `a_plain_download_reports_what_it_wrote`
and `a_206_is_a_resume_and_carries_the_whole_size` read
`requests.lock().unwrap()[0]` immediately after `fetch` returned, but the test
server thread records the request *after* `handle` has written the response.
The client only needs those bytes, so it can return first, leaving the vector
empty and the index panicking. It lost roughly one full-suite run in five —
found by running the suite five times before merging Phase 0a rather than
once. A test that fails at random in a blocking CI trains people to re-run
until green, which is how a real failure gets waved through.
→ `wait_served(&served, n)` blocks on the `served` counter, which the thread
stores with `SeqCst` *after* the push, so it is a real happens-before edge
rather than a sleep. Applied at all three sites that read the recorded
requests. Verified by five consecutive clean full-suite runs.

**ART-037** 🔴 **ADF Studio could not open any bootable ADF**
`core/adf/bootblock.rs`, `core/adf/mod.rs` · The parser read bytes 8..11 of the
boot block as a "root block pointer". An AmigaDOS boot block has no such field:
0..3 is the DOS type, 4..7 the checksum, and 8 onwards is boot code. ART
therefore read 68000 machine code as a block number and refused every bootable
disk with `root block has type <nonsense>`. Invisible to ART's own tests
because every fixture ART builds has zeros where a real disk has boot code —
and invisible in day-to-day use because the file manager, which runs on
`core/volume`, opened the same disks perfectly.
→ The root block is computed, `total_blocks / 2` via `root_block_of()`, the
way `VolumeGeometry::root_block_for` already did. Pinned by
`a_bootable_image_opens_because_the_root_block_is_computed` and by an oracle
check that has `xdftool` write a bootable floppy — its own boot code,
checksum-legal — for ART to open.

**ART-038** 🟡 **HD ADFs reported half their capacity**
`core/adf/mod.rs` · `info()` computed capacity and parsed the bitmap with
`DD_TOTAL_BLOCKS`, so every number it reported for a 1.76 MB image was a
floppy-shaped guess. Same root cause as ART-037: `core/adf` predated the
Stage R geometry and never adopted it.
→ Both come from the image's own length, via `total_blocks_of()`. Pinned by
`an_hd_image_reports_its_real_capacity`.

**ART-039** 🟡 **Disabled controls were indistinguishable from active ones**
`src/styles/global.css` · The word `disabled` appeared in no stylesheet, so a
disabled primary button kept its accent fill. On the WHDLoad screen a
correctly-refused install showed a solid blue "Install to the disk" button
that did nothing when clicked.
→ `:disabled` and `:focus-visible` styles for buttons and form controls.

**ART-040** 🟡 **Content was clipped instead of scrolled, and nothing scaled**
`src/components/layout/layout.css` · `.app-main` is a grid item with the
default `min-height: auto`, so it grew to fit its content and `.app-content`'s
`overflow: auto` never engaged; the shell's `overflow: hidden` then clipped the
excess. `min-height: 0` appeared nowhere in the codebase. Separately, the
application had zero `@media` queries and eleven pages carried hand-written
`maxWidth` values.
→ The `min-height: 0` chain, breakpoints, and one central content width.

**ART-044** 🟠 **WHDLoad and Aminet installs could land a package partially with no warning**
`commands/sources.rs` · `sources_install_adf` and `sources_install_volume`
both wrote through `run_copy_in_folder`, which lands what fits and reports the
rest as `skipped` — the right contract for the general-purpose F5 copy, wrong
for an install. Neither install command ran a pre-flight fit check before
writing, so a package that did not fit was discovered mid-loop: a WHDLoad pack
whose `.slave` ended up in `skipped` is a broken, non-bootable result the user
was never warned about (§92: explain before modify).
→ `plan_copy_in_folder` (`commands/volume_write.rs`) mirrors
`volume_plan_copy`'s own pre-flight against an already-built `HostFolder` and
refuses atomically, with the real block numbers, before either install command
writes anything. Pinned by
`installing_a_package_that_does_not_fit_is_refused_without_touching_the_image`
and `installing_a_package_that_fits_installs_completely`.

**ART-045** 🟠 **A transient re-read after a successful write could report it as failed**
`commands/adf.rs` · `MutationOutcome::from_write` re-opened the image with
`AdfImage::open()` after `with_volume` had already committed the write and
closed its handle. That second, independent open could race an external lock
(antivirus scan-on-close, a search indexer) taken the moment the first handle
released, turning an already-durable, successful write into a reported
failure — and losing the backup path along with it, contradicting the
guarantee that the user is always told where the previous version went.
→ `VolumeWriter::all_bytes()` (`core/volume/write/mod.rs`) reads the volume
block by block through the session's own still-open device; `info_from_session`
builds `AdfInfo` from that, called from *inside* every `with_volume` closure,
so a failure there aborts before any byte reaches disk rather than after.
`MutationOutcome::from_write` is deleted, not disarmed. Pinned by
`a_failure_after_the_mutation_never_reaches_the_disk` plus per-command routing
tests (`creating_a_directory_goes_through_the_volume_writer`,
`deleting_an_entry_goes_through_the_volume_writer`,
`renaming_an_entry_goes_through_the_volume_writer`).

**ART-052** 🟠 **A cancelled install committed half a package and reported success**
`commands/sources.rs`, `commands/volume_write.rs` · `copy_into_volume` stops
between files and returns `Ok(report)` with `cancelled: true` — the right
contract for the general-purpose F5 copy. `run_copy_in_folder` then went
straight on to `commit_whole_file`, and neither install command ever read
`report.cancelled`: they read `files_copied`, `bytes_copied` and `skipped`, and
the files that were never reached appear in none of those, because the loop
`break`s. Cancelling an install therefore wrote half the package to the disk,
finished the job as **Completed**, and recorded a successful install in the
operation log — with the `.slave` quite possibly among the files that never
landed. The retired `install_archive_into_adf` had checked `is_cancelled()`
inside its `mutate_disk_file` closure and returned `CoreError::Cancelled`, so
nothing was written at all; that guarantee was lost in the move to the volume
writer.
→ `run_copy_in_folder_with(.., OnCancel::Abandon, ..)` returns
`CoreError::Cancelled` *before* `commit_whole_file` is reached, so the user's
image never leaves the state it was in. F5 keeps `OnCancel::KeepWhatLanded`
deliberately. Pinned by
`a_cancelled_install_writes_nothing_and_does_not_report_success`, which
compares the image's bytes before and after.

**ART-053** 🟠 **`VolumeWriter::all_bytes()` allocated from an unchecked block count**
`core/volume/write/mod.rs` · Introduced with ART-045's fix and documented as
"only sensible for a volume small enough to hold in memory", with nothing
enforcing it — while `commands/adf.rs::info_from_session` calls it inside
every `with_volume` closure and every ADF command takes an arbitrary image
path from the frontend. Pointed at an image over 16 MiB, `with_volume` takes
the block-journal branch: the write lands *in the file*, the journal is
deleted, and only then does `all_bytes()` try to read `total_blocks ×
block_size` into memory — a multi-gigabyte allocation in a profile that sets
`panic = "abort"`, or an `Err` returned after the write is already durable and
its journal gone.
→ `all_bytes()` computes the size with `checked_mul` and refuses anything over
`WHOLE_FILE_LIMIT_BYTES` — the same constant `WriteStrategy::for_image` splits
on, so "small enough to read whole" means one thing in ART. Pinned by
`all_bytes_refuses_a_volume_too_large_to_hold_in_memory`.

**ART-046** 🔵 **A doc comment claims a guarantee the public API does not give**
`core/volume/write/mod.rs` · The comment above the test-only `commit_blocks`
said "every public operation here deliberately cannot get into that state" — a
volume whose touched blocks are each well-formed but whose structure is wrong.
`add_file` on an image whose bitmap is flagged valid but marks the root block
free reaches exactly that state through the public API: `bitmap.rs`'s allocator
hands out anything free in `reserved..total_blocks` and the root block is not
inside `reserved` (2, on a floppy), so a plain `add_file` allocates block 880
and overwrites the root, with every touched block internally well-formed. The
claim was the argument for deleting ART-042's whole-image gate as redundant.
→ The comment now says what is true — no public operation can reach that state
on a *well-formed* volume, the flagged-valid-but-wrong bitmap can, and that is
what the gate above catches. Narrowed rather than "proved", because the public
API really can reach it.

**ART-054** 🟡 **The WHDLoad refusal panel contradicted itself**
`src/pages/WhdloadInstall.tsx`, `commands/whdload.rs` · On the "not a WHDLoad
pack" path Rust deliberately blanks the layout and zeroes the cost, and the UI
rendered them anyway: "Create the drawer `[blank]` and write 0 files, 0 B ·
0 blocks needed · 0 free" printed directly above the amber refusal, an empty
pack name badged *"no icon — it will not show up on Workbench"*, and an install
button reading "Install to " because `?? "the disk"` catches `null` but not the
empty string Rust sends. The panel's suggestion was a hardcoded "You can still
copy it by hand from the Files screen", which is wrong or duplicated for five
of the six refusals — a full disk, a name already taken and unsupported names
all fail the same way by hand, `needs_installer` needs WinUAE, and the
low-confidence reason already ends with that same sentence.
→ `hasPack()` (`src/lib/whdload.ts`) gates the cost paragraph and the
pack-name/icon block; the button falls back with `||`. The remedy is now data:
`WhdloadRefusal { reason, suggestion }` carries it from `refuse()`, so the hand
copy is suggested for exactly the one refusal it answers. Refusals still arrive
as `plan.refusal` inside an `Ok` and exceptions still reach the red banner —
`a_missing_pack_is_a_refusal_and_a_broken_archive_is_still_an_error` pins both,
and now also pins which suggestion travels with which reason.

**ART-055** 🔵 **The install pre-flight guard had no test that would fail if it were deleted**
`commands/sources.rs` · ART-044's fix put `if !plan.fits() { return
Err(SafetyRefused) }` inline inside `sources_install_adf`'s and
`sources_install_volume`'s `spawn_job` closures, which no test can call. The
test named after it called `plan_copy_in_folder` directly and asserted the
image was unchanged — trivially true, since only a read-only planner had run.
Deleting either guard left the suite green.
→ Both command bodies are factored into `install_archive_into_volume`, the way
task 8 factored the ADF commands, and the test drives that. Verified by
mutation: reverting the guard fails the test.

**ART-056** 🟡 **The sidebar clipped the page on any window shorter than its own nav list**
`src/components/layout/layout.css` · ART-040 applied `min-height: 0` to
`.app-main` and not to `.sidebar`, the other item in the same implicit grid
row. With fourteen nav entries the sidebar's min-content height is about
665px, so on a shorter viewport the auto row grew past `100vh`, `.app-main`
stretched with it and the shell's `overflow: hidden` clipped the bottom of
`.app-content` — including the bottom of its own scrollbar. A 1366×768 laptop
at 125% scaling is about 584 CSS px, so this was the ordinary case.
→ `.sidebar { min-height: 0; overflow-y: auto; }`.

**ART-057** 🟡 **Two more controls looked enabled while disabled**
`src/styles/global.css` · ART-039's fix keyed on `.btn`, and
`.breadcrumb-item` and `.file-row-dir-btn` (both `disabled` while an operation
runs, `src/pages/AdfBrowser.tsx`) carry neither — so both kept `cursor:
pointer` and their live colour. Also found in the same file: the
`:focus-visible` block ART-039 added was byte-identical to a universal
`:focus-visible` rule already 60 lines below it, and `CHANGELOG.md` claimed a
focus ring was "visible again" when it had never been missing.
→ The `.btn:disabled` treatment applied to both controls (the last breadcrumb
keeps full contrast — it is disabled because it is where you already are), the
duplicate rule removed, and the CHANGELOG line corrected.

### Stage W

**ART-042** 🔴 **A write that produced an invalid volume was committed anyway**
`commands/volume_write.rs` · The retired `core/adf/mutate.rs::mutate_disk_file`
validated the finished image in memory before writing it. `with_volume`, which
replaced it, ran the operation and went straight to `guarded_write` — under a
comment that said the opposite. The only surviving check was
`core/volume/write::validate_touched`, which sees just the blocks the operation
touched and, of those, only their checksums. A write that left every touched
block well-formed and the volume structurally wrong — a root block that is no
longer a header block, an image that stopped being an AmigaDOS volume —
replaced the user's file silently, and the backup it took was of the last good
version, so the damage was recoverable only if the user noticed.
→ `commit_whole_file` validates the whole in-memory image at all three
whole-file commit points and refuses before `guarded_write` is reached. Only
`Problem` findings refuse; a warning describes an image that was already like
that. Pinned by `a_write_that_would_not_validate_never_reaches_the_file`
(refused, and the file byte-for-byte unchanged) and
`a_valid_write_still_commits_through_the_gate`.

**ART-041** 🟠 **Validation measured every image against a DD floppy**
`core/adf/validate.rs` · `validate` compared `image.len()` against
`DD_TOTAL_BLOCKS * BLOCK_SIZE` and warned on anything else, so every HD floppy
and every hard-disk image was reported as suspect. Harmless while validation
was only a report; a wall the moment it became the gate in front of every write
(ART-042), which would have refused writes to every image that is not a DD ADF.
→ The block count comes from the image's own length and the root block from
that count (`VolumeGeometry::root_block_for`). Pinned by
`an_hd_floppy_image_is_healthy_at_its_own_geometry`,
`a_hard_disk_sized_image_is_healthy_at_its_own_geometry` and, end to end,
`an_hd_floppy_is_written_through_the_gate` /
`a_hard_disk_image_is_written_through_the_gate`.

**ART-036** 🟡 **Names with accented characters were refused as too long**
`core/volume/write/dir.rs` · `check_name` compared `str::len()` against the
30-character AmigaDOS limit. That counts **UTF-8 bytes**, not characters, so
every Latin-1 character above `~` counted double: `Grüße vom Süden` is 15
characters and 18 bytes, and a name of thirty accented characters was rejected
as sixty. AmigaDOS stores one byte per Latin-1 character, so all thirty fit.
The more accents a name had, the more wrong the answer — which is why it went
unnoticed in ASCII-only testing.
→ The check counts characters, and `plan.rs` shortens by characters too:
byte-slicing a Latin-1 stem would cut a character in half and panic. Pinned by
`the_length_limit_counts_characters_not_utf_8_bytes` and
`shortening_a_latin_1_name_does_not_split_a_character`.

### Amiga format compatibility (Stage R oracle)

All four were found the same way: by pointing **amitools** — an independent
implementation — at images ART had written. Every one of them was invisible to
ART's own test suite, because ART's reader and writer agreed with each other.
That is the project's oldest failure pattern, and this is the first time an
outside implementation has been asked.

**ART-033** 🔴 **Every ADF ART wrote had invalid block checksums**
`core/adf/checksum.rs` · AmigaDOS uses **two** checksum algorithms: the boot
block sums with end-around carry and stores the bitwise NOT, while root, header,
OFS data and bitmap blocks use a plain wrapping sum stored as its two's
complement. ART applied the boot block algorithm to everything. `core/rdb.rs`
had the correct one all along, which is why RDB blocks always validated
elsewhere and ADFs never did.
Every disk ART created or modified was rejected by AmigaOS, WinUAE and every
other Amiga tool — while ART reported it healthy, because its verifier used the
same wrong algorithm. Confirmed: `xdftool` refused an ART-made blank ADF with
"Invalid Root Block(2)".
→ `block_checksum` is now the plain sum; the boot block keeps its own
implementation in `bootblock.rs`. Verified: `xdftool` now reads an ART-made ADF
and lists its volume.
Tests: `a_checksummed_block_sums_to_zero`,
`the_algorithm_matches_the_reference_implementation`,
`checksum_all_zero_block` (which had been pinning the bug).

**ART-034** 🔴 **Files ART added to a disk were zero bytes on a real Amiga**
`core/adf/blocks.rs`, `core/adf/mutate.rs` · The file header's `byte_size` was
read and written at longword 79, and the comment at longword 80. The real
layout is `byte_size` at longword 81 (offset 324) and the comment at longword 82
(offset 328) — 80 is the protection bits. Both halves of ART used the same wrong
offsets, so a file round-tripped through ART perfectly and appeared as an empty
file to AmigaOS. The root block's bitmap-extension pointer was one longword
early too (412 instead of 416), which would matter on any volume large enough to
need extension blocks — that is, exactly the HDF partitions Stage R adds.
→ Offsets corrected against `amitools` (`EntryBlock._read_nac_modts`,
`FileHeaderBlock._read`). Verified: `xdftool` lists a file ART wrote with the
right size and `xdftool type` returns its contents.

**ART-035** 🔴 **The free-space bitmap was laid out the wrong way round**
`core/adf/blocks.rs`, `core/adf/create.rs`, `core/adf/mutate.rs` · ART recorded
a block's bit at longword `block / 32`, counting from the **most** significant
bit. AmigaDOS records it at `(block - reserved) / 32`, counting from the
**least** significant bit — the bitmap does not describe the boot blocks at all.
ART was wrong on both axes, so every block it marked used landed on some other
block's bit. AmigaOS would have seen ART's occupied blocks as free and handed
them out to the next file written, destroying data on a real machine.
Invisible from inside ART: the allocator, the free-space count and the parser
all shared the same arithmetic, and a permutation of bit positions leaves the
*count* of free blocks unchanged, so even the totals looked right.
→ One `bit_position()` helper now owns the arithmetic and all four call sites
go through it. Confirmed against ADFlib (`adf_bitm.c`: `bitMask[i] == 1 << i`,
`sectOfMap = nSect - 2`) and amitools (`ADFSBitmap.get_bit`). Verified by
decoding an ART-written disk with the reference mapping: the blocks marked used
are exactly the root, the bitmap and the file that was added.
Tests: `bit_positions_match_the_reference_implementations`,
`bitmap_marks_used_and_free` (which had been pinning the bug),
`a_large_volume_spills_into_more_bitmap_blocks`.

**ART-032** 🟠 **RDB partitions described the wrong DosType and boot priority**
`core/rdb.rs` · `BootPri` was read and written at longword 46 and `DosType` at
47. They belong at 47 and 48; longword 46 is `Mask`. An ART-created hard disk
image therefore told AmigaOS its partitions had DosType 0 — no filesystem —
while ART read its own images back correctly. Found by `rdbtool`, which showed
`pri=1146049281` (the ASCII for `DOS`) sitting in the priority field.
→ Offsets corrected and pinned.
Tests: `dosenv_offsets_match_the_amiga_layout`,
`a_partition_reads_back_the_filesystem_it_was_written_with`.

### Software Sources (Aminet, §41.5)

Both found the same way: by running the finished engine against real Aminet
mirrors instead of only against its own fixtures. Neither was reachable from
the test suite as written, which is the lesson worth keeping.

**ART-030** 🔴 **Mirror failover concatenated a dead mirror's bytes onto the next one's**
`core/sources/mirror.rs` · `fetch_with_failover` passed the same `Write` to
every attempt. A mirror that failed *after* sending part of the body left those
bytes in the destination, and the next mirror's response was appended to them.
The result was one file assembled from two mirrors — which parses, hashes and
caches perfectly happily while being wrong.
Observed live: `aminet.net` dropped the connection 519 416 bytes into `INDEX`,
and the catalog came back with 91 413 entries instead of the 85 435 the file
actually holds. No error, no skipped line, no warning.
→ The destination is now a `FetchTarget`, wound back before every attempt.
`HttpMirrorClient` also refuses a body shorter than an announced
`Content-Length`, so a silently truncated response cannot pass as complete.
Tests: `a_mirror_that_dies_mid_body_does_not_pollute_the_next_one`,
`a_file_destination_is_wound_back_too`,
`a_dirty_destination_is_wound_back_before_the_first_attempt`,
`a_body_shorter_than_the_announced_length_is_never_a_success`.

**ART-031** 🟠 **LHA archives with level 2 or 3 headers could not be opened at all**
`core/lha/mod.rs`, `core/lha/safe_extract.rs` · both read `header.filename`
directly. That field is only populated for level 0 and 1 headers; levels 2 and
3 — what modern tools write, and what Aminet hosts — leave it empty and carry
the name in an extended header. Every such archive failed with
`ART-FORMAT-MALFORMED: empty entry name`, so LHA Studio could not list or
extract a typical Aminet download.
→ `entry_path()` falls back to `parse_pathname_to_str()` when the raw field is
empty. Levels 0 and 1 deliberately keep using the raw field: `delharc`
percent-encodes non-ASCII bytes, and switching those levels over would rename
the Latin-1 Amiga filenames that extract correctly today. `safe_join` remains
the choke point for both.
Tests: `a_level_two_header_is_read_from_its_extended_header`,
`a_level_zero_name_still_comes_from_the_raw_field`.

All fixed 2026-08-09 unless noted. Test names are in `src-tauri/`.

### Hard disk images (Stage 3 audit)

**ART-021** 🔴 **Creating or opening an HDF allocated the whole image in memory**
`core/hdf.rs`, `core/rdb.rs` · `create_rdb_image` built the entire image as a
`Vec<u8>` before writing it, and `open_hdf` read the whole file back to report
its geometry. A 4 GB hard disk image therefore needed 4 GB of RAM — on a machine
that could not spare it, ART died. Violates §56 (never allocate from an
unchecked length).
→ `create_rdb_layout` returns only the structured leading blocks; the file is
created sparsely with `set_len`. `open_hdf` reads a 1 MB header window and takes
the size from the file's metadata. Tests: `large_images_are_created_sparsely`,
`create_and_parse_rdb_image_with_pfs3_and_ffs`.

**ART-022** 🔴 **Creating an image silently destroyed an existing one**
`core/hdf.rs`, `core/adf/create.rs` · Both creation paths called
`std::fs::write`, which replaces whatever is already there. Typing the name of
an existing `Workbench.hdf` destroyed it — gigabytes, with no backup and no
prompt. Creation is a `SAFE_CREATE` operation and may only ever write something
new (§57).
→ Both refuse when the target exists; the HDF path uses `create_new` and removes
a partially written file if anything fails. Tests:
`create_refuses_to_replace_an_existing_image`,
`save_refuses_to_replace_an_existing_file`.

**ART-023** 🟠 **A tiny requested size aborted the application**
`core/hdf.rs` · A plain image smaller than four bytes was indexed at `[0..4]` to
write the bootblock signature, panicking — and with `panic = "abort"` that ends
the process.
→ A minimum size is enforced and reported as an error.
Test: `absurdly_small_sizes_error_rather_than_panic`.

**ART-024** 🔴 **The RDSK block described a zero-capacity disk to AmigaOS**
`core/rdb.rs` · The logical-drive fields were written at the wrong longwords.
`HiCylinder` (LW 35) and `CylBlocks` (LW 36) were never written at all, so they
stayed zero; `RDBBlocksHi` got the value meant for another field, and
`cylinders - 1` landed in a reserved longword. ART read its own disks back
correctly because the parser only reads geometry from LW 16–18 — the same shape
of defect as ART-009: self-consistent, and wrong for every other Amiga tool.
Verified against `amitools`' `RDBlock.py` field indices.
→ Fields written at their documented offsets.
Test: `rdsk_logical_drive_fields_are_written`.

**ART-025** 🟠 **Empty RDB block lists pointed at block 0**
`core/rdb.rs` · `BadBlockList`, `FileSysHeaderList` and `DriveInit` were left at
zero. Zero is a valid block number, so AmigaOS would follow them into block 0 —
the RDSK block itself. `BlockBytes` was also zero rather than 512.
→ Absent lists are written as `-1` (`NO_BLOCK`) and `BlockBytes` is set.
Test: `rdsk_absent_lists_use_the_no_block_sentinel`.

**ART-026** 🟠 **Oversized partitions were silently truncated**
`core/rdb.rs` · Each partition's end was clamped with `.min(cylinders - 1)`. Ask
for two 500 MB partitions on a 100 MB disk and the first was quietly shrunk to
whatever fit, the rest were skipped mid-loop — leaving the previous partition's
`next` pointer aimed at a block that was never written. The user was told
nothing. Violates §89.
→ The total is checked up front and an impossible layout is refused with the
numbers involved. Tests: `oversized_partitions_are_refused_not_truncated`,
`partitions_do_not_overlap_and_stay_inside_the_disk`,
`too_many_partitions_are_refused`.

**ART-027** 🟡 **RDB checksums ignored the block's own `SummedLongs`**
`core/rdb.rs` · The checksum summed a fixed 128 longwords instead of the count
the block declares (LW 1, conventionally 64). ART's own images happened to pass
because their later longwords are zero, but a real Amiga disk carrying vendor
strings there would be reported as corrupt.
→ Both compute and verify honour `SummedLongs`, with a guard against a
nonsensical declared value. Tests: `checksum_honours_summed_longs`,
`checksum_survives_a_nonsense_summed_longs_field`.

### Collection & ROM (Stage 3 audit)

**ART-028** 🟠 **A folder scan could be sent into unbounded recursion**
`core/collection.rs` · `collect_files_recursive` followed every subdirectory
with no depth limit and no symlink check. A Windows junction pointing back up
its own tree recursed until the stack overflowed, which aborts the process.
Dropping such a folder onto ART closed the application.
→ Depth is capped and symlinks are not followed.
Tests: `scanning_stops_at_the_depth_limit`, `scanning_finds_images_within_the_limit`.

**ART-029** 🟡 **Any file could be read whole as a "ROM"**
`core/rom.rs` · `identify_rom` read the entire file before checking anything.
Kickstarts are at most 1 MB, but the user picks the file — a mistaken pick of a
large image pulled all of it into memory.
→ Size is checked against a 4 MB ceiling before reading.

### Data safety

**ART-001** 🔴 **ADF edits overwrote the original with no backup and no atomicity**
`core/adf/mod.rs` · `mutate_disk_file` called `std::fs::write` straight onto the
user's file. An interrupted write left a truncated — that is, destroyed — disk
image, and there was no previous version to fall back on. Violates §57, §93.
→ Pipeline is now `read → mutate → validate → backup → atomic commit` via
`core/safety`. Tests: `mutation_backs_up_the_previous_version`,
`failed_mutation_leaves_the_original_untouched`,
`mutation_that_corrupts_the_image_is_not_committed`.

**ART-002** 🔴 **LHA extraction silently overwrote existing files**
`core/lha/safe_extract.rs` · `File::create` replaced whatever was already at the
destination. Extracting an archive into a working folder destroyed the user's
files without a word. Violates §89.
→ `OverwritePolicy::{Skip,Overwrite,Rename}`, defaulting to `Skip`.
Test: `existing_files_are_skipped_by_default`.

**ART-003** 🔴 **FlashFloppy `FF.CFG` was regenerated from scratch**
`core/gotek.rs` · Saving rewrote the whole file from ART's own fields, deleting
every setting it does not model — `pin02`, `head-settle-ms`, `display-order` and
dozens more that Gotek owners tune by hand. Directly violates §39
("Unknown settings must be preserved").
→ The file is now edited in place; unmanaged keys and comments pass through
verbatim. Test: `round_trip_preserves_unknown_settings`.

**ART-004** 🔴 **PiStorm `cmdline.txt` was regenerated, leaving the SD card unbootable**
`core/pistorm.rs` · Same defect, worse consequence: regenerating the Raspberry Pi
kernel command line dropped `root=` and `console=`, so the Pi would not boot at
all after ART "saved" the configuration. Violates §40.
→ `config.txt` and `cmdline.txt` are merged, not rewritten.
Test: `cmdline_round_trip_preserves_boot_parameters`.

**ART-005** 🟠 **Config files were replaced without a backup**
`core/gotek.rs`, `core/pistorm.rs` · No previous version was kept.
→ Both go through `guarded_write` with `BackupPolicy::CONFIG` (5 generations).
Tests: `saving_backs_up_the_previous_config`,
`saving_backs_up_the_previous_sd_configuration`.

**ART-006** 🟡 **Aborted extractions left truncated files behind**
`core/lha/safe_extract.rs` · A file that failed to decompress, or was cut short
by the bomb guard, stayed on disk looking like a successful extraction.
→ Partial output is removed and the entry is reported as skipped with a reason.

### Crashes

**ART-007** 🔴 **An invalid block number from the UI killed the whole application**
`core/adf/blocks.rs`, `core/volume/write/mod.rs` · Block numbers arrive from
the frontend (`dirBlock`, `headerBlock`) and were used to index the image
directly. Out of range meant a panic, and the release profile sets
`panic = "abort"` — so the entire app died.
→ All access goes through `blocks::block_slice`/`block_slice_mut`/`read_u32_at`/
`write_u32_at`, which return a `CoreError`. *(Location updated: the fix and its
tests originally lived in `core/adf/mutate.rs`, deleted when task 10 retired it
in favour of the single volume writer — fixed, not reopened; see ART-047,
where `block_slice_mut`/`write_u32_at` were later deleted for good — no write
path indexes a whole image by block number any more — and `block_slice`/
`read_u32_at` gained a new caller in `core/adf/validate.rs`.)* Tests:
`block_access_rejects_out_of_range_numbers` (`core/adf/blocks.rs`),
`out_of_range_directory_block_is_an_error` (`core/volume/write/mod.rs`).

**ART-008** 🟠 **Malformed images could hang the UI forever**
`core/volume/write/dir.rs`, `core/volume/write/file.rs` · Hash-bucket and
file-extension chain walks had no step limit; a chain pointing back at itself
looped indefinitely.
→ Both walks are bounded and report a malformed image rather than looping.
*(Location updated: originally `core/adf/mutate.rs`, deleted when task 10
retired it — fixed, not reopened.)* Tests:
`a_hash_chain_that_loops_is_an_error_not_a_hang` (`dir.rs`),
`an_extension_chain_that_loops_is_an_error_not_a_hang` (`file.rs`).

### Filesystem correctness

**ART-009** 🔴 **The ADF hash function was not AmigaDOS-compatible**
`core/adf/hash.rs` · `name_hash` omitted the `& 0x7ff` mask that the reference
algorithm applies after *every character*. Entries went into the wrong hash
buckets. ART could read its own images back — it hashed consistently — but
AmigaDOS, WinUAE and every other Amiga tool could not find the files. Any image
ART wrote before this fix is suspect.
→ Matches `adfGetHashValue`, with reference values pinned.
Tests: `matches_the_amigados_reference_values`,
`the_mask_is_applied_every_iteration`.

**ART-010** 🟠 **International volumes hashed names with the wrong case folding**
`core/adf/hash.rs` · `name_hash` took no `international` flag, so INTL volumes
(which fold the Latin-1 224–254 range) used plain ASCII rules.
→ The flag now comes from the volume's own bootblock.
Test: `international_folding_differs_for_accented_names`.

**ART-011** 🔴 **`rename_entry` corrupted directories when the chain was inconsistent**
`core/volume/write/dir.rs` · If the entry was not found in its parent's hash
chain the function carried on silently and linked it into the new bucket
anyway, leaving it referenced from two places. `delete_entry` had the check;
rename did not.
→ Missing entries now error out via `predecessor_of`, which ends its walk with
`Malformed { "block N is not in the bucket its name hashes to" }` rather than
returning "no predecessor". *(Location updated: originally `core/adf/mutate.rs`,
deleted when task 10 retired it — fixed, not reopened; the moved test also now
asserts the image is byte-for-byte unchanged after the refusal, which the
in-memory original could not check.)*
Test: `rename_of_an_unlinked_entry_reports_an_error` (`core/volume/write/mod.rs`).

**ART-012** 🟠 **Duplicate names could be created in one directory**
`core/volume/write/dir.rs` · AmigaDOS compares names case-insensitively and
cannot hold two entries with the same name; nothing checked.
→ Name collisions are rejected, case-insensitively, including a directory
shadowing an existing file's name. *(Location updated: originally
`core/adf/mutate.rs`, deleted when task 10 retired it — fixed, not reopened.)*
Tests: `a_name_that_differs_only_in_case_is_already_taken` (`dir.rs`),
`a_name_already_taken_is_refused_and_nothing_is_written`,
`a_directory_cannot_shadow_an_existing_file_name`,
`rename_onto_an_existing_name_is_rejected` (`core/volume/write/mod.rs`).

**ART-013** 🟠 **A file header could be used as a directory**
`core/volume/write/mod.rs` · Passing a file's header block as the insert
target wrote hash-table entries into it, corrupting the file.
→ `VolumeWriter::resolve_directory` verifies the block is `ST_ROOT` or
`ST_USERDIR` via `dir::is_directory`. *(Location updated: originally
`core/adf/mutate.rs`, deleted when task 10 retired it — fixed, not reopened.)*
Test: `writing_into_a_file_header_instead_of_a_directory_is_refused`.

### Security

**ART-014** 🟠 **The zip-bomb guard could be bypassed by integer overflow**
`core/lha/safe_extract.rs` · `total_written + header.original_size` was unchecked,
so an archive declaring a huge size could wrap the total past the limit.
→ `checked_add`; a size that cannot be added aborts the extraction.

**ART-015** 🟡 **Media paths could inject arbitrary WinUAE directives**
`core/winuae.rs` · `.uae` files are line-oriented and paths were written into
them unescaped, so a newline in a path became further configuration.
→ `checked_config_value` rejects line breaks.
Test: `line_breaks_in_paths_are_rejected`.

### Emulator integration

**ART-016** 🟠 **Only the first of several HDFs was reachable**
`core/winuae.rs` · Multiple hard drives produced repeated bare `hardfile=` lines
with no device names, and an invalid `hardfile_type{i}` key.
→ One `hardfile2=rw,DH<n>:…` line per image, per WinUAE `cfgfile.cpp`.
Test: `each_hardfile_gets_its_own_device`.

**ART-017** 🟡 **WinUAE detection used hard-coded drive letters**
`core/winuae.rs` · The candidate list included `D:\WinUAE\…`, left over from a
developer's machine, and assumed Windows lives on `C:`.
→ Paths come from the `ProgramFiles*` environment variables.

**ART-018** 🔵 **Concurrent launches clobbered each other's configuration**
`core/winuae.rs` · Every launch wrote the same `art_launch.uae` temp file.
→ The filename now carries a nanosecond stamp.

### Analysis

**ART-019** 🟠 **HD floppy geometry was never reported**
`core/analysis.rs` · The check read `size == 901_120 || size == 880 * 1024` —
the same number twice — so the HD branch was unreachable. Clippy's `eq_op` lint
caught this, but CI ran clippy with `continue-on-error`, so nobody saw it.
→ Explicit `DD_IMAGE_SIZE` / `HD_IMAGE_SIZE` constants, 11 vs 22 sectors.

### Build hygiene

**ART-020** 🟠 **CI hid real correctness errors**
`.github/workflows/ci.yml` · Clippy ran with `continue-on-error: true`, and
`lib.rs` carried a blanket `allow(dead_code, unused_imports, unused_variables,
unused_assignments)`. Between them, ART-019 stayed invisible for months.
→ Clippy is blocking; the blanket allow is narrowed to `dead_code`.

---

## Adding an entry

1. Take the next free `ART-NNN`.
2. State the defect as a claim, not a symptom — what is wrong, not what looked odd.
3. Name the file, and say **how it fails for a user**. If you cannot describe a
   way it hurts someone, it is a preference, not a defect.
4. Cite the spec section when a rule is broken.
5. When fixing it, add a regression test and name it here. A fix without a test
   is not fixed — it is untested.
