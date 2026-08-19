# Project Status

**The single source of truth for where ART actually is.**

Every other document describes intent (the spec, `roadmap.md`) or mechanics
(`architecture.md`, `CLAUDE.md`). This one describes reality. When they
disagree, this file wins — and if this file disagrees with the code, fix this
file.

Update it at the end of any session that changes what works.

- Known defects and technical debt: [ISSUES.md](ISSUES.md)
- Feature-by-feature implementation state: [FEATURES.md](FEATURES.md)

---

## Snapshot

| | |
|---|---|
| **Last updated** | 2026-08-19 (AmigaOS 3.9: a second `MediaSource` reads a real install disc image, and the real 469 MiB disc builds the recipe's **one** component — `workbench-base`, 588 files, ~6 MB — end to end. Not a complete 3.9 system, but it **boots to a clean Workbench** under WinUAE with a licensed V40 ROM; `FEATURES.md`'s row is the precise statement. **And it is now really 3.9**: the recipe used to place only the disc's `Workbench3.5` half, so the booted tree answered `Workbench 44.5` and failed its Startup-Sequence's first command; a second component, `workbench-39`, now lays the disc's 854-row `Workbench3.9` overlay on top (+622 files, +8.8 MB, 19 upgrades, 0 downgrades) and the booted system answers **`Kickstart 40.68, Workbench 45.1 (13-Nov-00)`** — [ART-169](ISSUES.md), fixed. **The update packages on top of it are worse off**: none of the three shipped ones reaches the tree correctly — [ART-166](ISSUES.md), [ART-167](ISSUES.md), [ART-168](ISSUES.md). **The content layer that carries them is built and tested**: a package archive is a `MediaSource`, a payload archive can be read out of its wrapper, packages are discovered in their own folder, three curated recipes ship as data, every overwrite is classified into five classes with downgrades marked, Produce and Add give byte-identical trees, and a panel makes the set selectable with one confirmation. **The reason none of that reaches the owner's own packages is a ceiling on the approach itself, not a missing feature** — see the section below) |
| **Version** | 0.8.5 — the first version published for other people to use. Deliberately not 1.0: 32 features are still amber ([FEATURES.md](FEATURES.md)), and as the row above says, what is left in SD-1 is not code: a card flashed and an A500 booted. That is the bar for 1.0, not a bigger number |
| **Current stage** | **SD-1 complete** — every gap it names is built: the card's shape (G2), RDB filesystem embedding (G4), build manifest (G7), image health check (G8), a build as a drop target (G15), engine and screen throughout. **What is left in SD-1 is not code: a card flashed and an A500 booted.** **SD-2 in progress** — PFS3/FFS preload (G3/G5 route E and native, [ART-120](ISSUES.md): `NativeFormatter` writes both by default, `hst-imager` a named fallback for two known gaps), the layout policy (G11), OS install (G5) and ROM pairing (G9) have all landed, engine and screen; launcher metadata export (G10) is owed. **G9 closes against the pairing that actually failed under WinUAE with a licensed ROM (real hardware untouched)**: the real AmigaOS 3.2 tree that needs Kickstart 47 against a card carrying the user's real Kickstart 40 comes back `Unsuitable { needs: 47, found: Some(40), rom: "kick.rom" }`; the same recipe's own V40 build, which carries its compatibility modules, comes back `Paired` against the identical card. **G5's product boots.** AmigaOS 3.2 was built from the user's own media, carried onto a PFS3 volume through the native/fallback path in one unattended run ([ART-122](ISSUES.md) had to be fixed first: a partition is formatted and filled by one tool), and **booted to a clean Workbench** under WinUAE with their licensed V47 A1200 ROM — wallpaper and all, no requesters. Getting there found two defects nothing but a Kickstart could have found: [ART-126](ISSUES.md) (every RDB filesystem ART ever embedded was advertised with the wrong `PatchFlags`, so AmigaOS ignored the driver — the guru a user reported) and [ART-127](ISSUES.md) (the tree lacked `icon.library` and `workbench.library`, and its wallpapers were switched off on an assumption the running system has now corrected). **What that proves, stated precisely**: the code that accepted the disk is Kickstart's own `expansion.library` and the real `pfs3aio` 68k binary, executing — so the filesystem side is settled, not provisional. What it does **not** touch is the card path: WinUAE hands the volume over as a plain `.hdf` through `uaehf.device`, where a PiStorm has an MBR, an Amiga disk starting 1.1 GB in, and Emu68's `brcm-sdhc.device` in the way ([ART-095](ISSUES.md)'s shape). That, the FAT32 boot partition and Emu68's own start-up are the untested rung — not the mounting. The install screen now has jsdom coverage past its own headings, but a real browser session has still never driven it — the access violation that crashed headless Chrome/Edge is unresolved ([ART-118](ISSUES.md)). Reading a real card has a screen ([ART-095](ISSUES.md), [ART-097](ISSUES.md)); **no card ART built has been flashed or booted**. **A second AmigaOS release joined 3.2, and stopped a step short of the same bar.** `CdSource` (`core/osinstall/source_cd.rs`) reads an install CD as a second `MediaSource` beside the ADF one; `core/osinstall/recipes/amigaos-3.9.json` ships a first, single-component 3.9 recipe; a release picker (`recipe::by_release`) makes it reachable from the OS Builder, and a disc dropped on the panel now offers the OS Builder too. Run for real against the owner's own 469 MiB `AmigaOS39.iso`, the tree now **builds end to end** — 588 files, 75 directories, all 663 planned items — after three defects the real run found and fixed ([ART-153](ISSUES.md), [ART-154](ISSUES.md), [ART-155](ISSUES.md) — the last one mis-diagnosed first, corrected by measurement) and one it found and left open ([ART-156](ISSUES.md)). **The tree boots.** Mounted as a `filesystem2=` directory volume under WinUAE with the owner's licensed Kickstart 3.1 (`amiga-os-310-a1200.rom`, V40) on an A1200/AGA profile, it reaches a clean Workbench with no requester; the screen's copyright line is the 3.5/3.9 string (*1985-2000 Amiga International*), not the ROM's own 1985-1993, so what is on screen came from the tree. The configuration is the one ART writes itself (`core::winuae::generate_uae_config`), through `core::winuae::real_boot_hook::boot_a_distribution_tree_when_asked` — `#[ignore]`d, env-gated. It boots **without** the `First-Install`/`SetPatch` tree [ART-159](ISSUES.md) says is unplaced, so that hazard is real but not fatal to a base install. Measured on a release build the install takes **6.2 s**, not the 20 s first recorded under a debug build — and the general sluggishness reported while driving it was the debug build too: `pnpm tauri dev` burned 64% of a core while idle where the release binary burns 0.0% at 65 MB. **Driven by hand at last**, which immediately found two things 619 frontend tests had not: a refusal that protected data and said nothing on screen (three `ART-SAFETY-REFUSED` entries in the operation log over an existing destination — a failed job never emits `osinstall-result`, the only event the screen listened to), and a verify field labelled "Amiga volume image" with nothing to explain it. Both fixed, plus a progress bar the install never had |
| **Build** | PASS |
| **Tests** | 1885 Rust passed, 0 failed, 20 ignored (real-media hooks, env-gated — see below) — and now on every run: [ART-164](ISSUES.md) had `core::iso`'s tests sharing a scratch directory, so about one full-suite run in eleven failed on an arbitrary one of them (measured 2026-08-19: the module on its own failed 5 times in 40, on four *different* tests, one of them comparing an accented volume name against another fixture's — two discs meeting in one directory). Fixed the same day with an atomic counter and verified the way it was found: **40 consecutive module runs, zero failures**, then the full suite three times. Worth keeping in view, because every “the suite is green” said during that round was a claim about the runs someone happened to make; 642 frontend passed, 0 failed, **exit code 0** — which it had not been: `pnpm test` printed `Tests 619 passed` and exited 1, because ten jsdom tests left unhandled promise rejections behind, and CI runs that command as a blocking step ([ART-163](ISSUES.md), fixed, with the underlying product defect filed and fixed separately as [ART-165](ISSUES.md)) |
| **Clippy** | clean at `-D warnings` |
| **TypeScript** | clean |
| **Kickstart table** | 154 dumps, generated from amitools' Remus split database and verified against it on every CI run (`scripts/rom-table-check.py`, ART-104). 24 of the user's own collection now named *with their machine*, including the two 40.68 builds told apart; ART's previous ten hand-listed hashes matched none of them. **Licensed Amiga Forever ROMs are first-class input** (ART-128): decoded with the `rom.key` beside them and then identified like any dump, and refused for a card build when the key is absent — they used to reach the boot partition still encrypted, which no Amiga could start |
| **amitools oracle** | 53 checks, both directions — now including a filesystem driver ART embedded in an RDB and `rdbtool` extracted back out byte-for-byte |
| **7-Zip FAT32 oracle** | the card's boot partition, written by ART and read by 7-Zip: filesystem type, geometry, label, names and every file's bytes (`scripts/fat-oracle-check.py`) |
| **7-Zip disc oracle** | 4 fixtures — Joliet, ISO9660-only, raw Mode 1, raw Mode 2/XA — names, sizes and every file's SHA-256 |
| **hst-imager PFS3 oracle** | both directions, local only (`scripts/pfs3-oracle-check.py`, no `hst.imager.exe` in CI): ART writes a volume through `NativeFormatter` and `hst-imager fs dir -r` reads it back — names, sizes, and every protection-bit string, `hsparwed` cased as `hst-imager` spells it; `hst-imager` formats and fills a volume and ART reads it back through `libpfs3`, SHA-256 per file rather than a length (ART-079's exact shape) plus the same protection strings |
| **cargo-deny** | advisories, bans, licences, sources — all ok |
| **MSRV** | 1.93 (raised from 1.77 on 2026-08-12, for a maintained 7z decoder) |
| **i18n** | `en.json` and `tr.json`, 1655 leaf keys each, parity enforced by `pnpm test` |
| **Release bundle** | rebuilt 2026-08-12 — MSI and NSIS, and the application was launched and answered |
| **Published** | <https://github.com/tolon/Art> — public, `main`, **GPL-3.0-or-later**. Work lands on `sd-1` and merges to `main` at the phase's
end; the licence *inventory* still said MIT until 2026-08-13, months after the
licence itself changed |
| **Real hardware** | **Bare metal, reached 2026-08-12** — `test/art-bootable-test.adf` booted a real **A500/A500+** (Kickstart 3.9) from a **Gotek**, to an AmigaDOS CLI. Photographed. Two ADFs had already mounted/booted under licensed Kickstart in WinUAE (Phase 1a). Still untouched: physical magnetic media — a Gotek is not a mechanical drive. **A hard disk joined the ADFs on 2026-08-16, in emulation**: a PFS3 volume ART formatted and filled booted a licensed Kickstart 3.1 to `hello from ART` at the `1>` prompt, and the full AmigaOS 3.2 tree booted a licensed V47 A1200 ROM to a clean Workbench. No card or hard disk ART built has reached real hardware |
| **Seen on a screen** | **The Files screen, at last** — opened, driven and screenshotted on 2026-08-12, along with the ADF, hard-disk, PiStorm, WHDLoad and Settings screens. It cost immediately: [ART-082](ISSUES.md) (the listings were capped at 420 px inside panes that filled the window) had shipped green behind 178 tests. Four more findings came out of the same session — ART-083 … ART-086. `ART-062` narrows to the screens still unopened (Aminet, Collection, Gotek, ROM, WinUAE, Tools) |

Reproduce the numbers above:

```bash
pnpm lint                                              # TypeScript
pnpm test                                              # frontend unit tests (i18n parity, phrase keys)
cd src-tauri && cargo fmt --check                      # formatting
cd src-tauri && cargo clippy --all-targets -- -D warnings
cd src-tauri && cargo test                             # unit + integration (twice — ART-059)
pip install amitools && python scripts/oracle-check.py # independent cross-check
python scripts/iso-oracle-check.py                     # the disc reader vs 7-Zip (needs 7z; not in CI)
python scripts/fat-oracle-check.py                     # the card's boot partition vs 7-Zip (needs 7z; not in CI)
python scripts/pfs3-oracle-check.py                     # PFS3, both directions, vs hst-imager (needs hst.imager.exe; not in CI)
python scripts/zoom-check.py                           # the shell's widths, in a real browser (needs `pnpm dev`)
python scripts/contrast-check.py                       # every colour pair in both themes, against WCAG (in CI)

# Task 14's real run — the user's own media, not a fixture. `#[ignore]`d
# (unlike the two pfs3-oracle hooks above them in the same file, which are
# only env-gated); skipped cleanly with the vars unset. Needs
# E:\amiga\Amigatolon (the user's own material) and E:\amiga\ProjeART
# (scratch) to exist.
cd src-tauri && ART_OSINSTALL_MEDIA="E:\amiga\Amigatolon\paketler\3.2\AmigaOs 3.2\ADF" \
  ART_OSINSTALL_ROM="E:\amiga\Amigatolon\kickstart\Kickstart v3.1 rev 40.68 (1993)(Commodore)(A1200).rom" \
  ART_OSINSTALL_DEST="E:\amiga\ProjeART\dist-3.2" \
  cargo test run_the_real_engine_against_the_users_own_media_when_asked -- --nocapture --ignored

# The second hook only puts a tree onto a card — it does not build one. Point
# it at dist-3.2 above and it reproduces the ART-113 failure exactly (see
# that entry). The reduced "witness" tree it built successfully against
# (dist-3.2 with every non-ASCII-named entry's subtree removed by hand) was
# a one-off Python pass, not a committed script, so the 969/106-file
# exclusion count and the 3059/3061 hash-match figure in ART-113/ART-114 are
# this session's own measurement, not something this block can regenerate.
cd src-tauri && ART_OSINSTALL_DEST="E:\amiga\ProjeART\dist-3.2-witness" \
  ART_CARD_OUT="E:\amiga\ProjeART\dist-3.2-witness-card.hdf" \
  cargo test build_the_real_dist_tree_onto_a_card_when_asked -- --nocapture --ignored

# The third hook is the one that carries the **whole** tree through the path
# the product actually runs — `run_with_fallback`, native first — rather than
# calling one formatter directly like the two above. It is what re-measured
# ART-113's gap on 2026-08-16 and what found ART-122; it prints the source
# tree's own counts beside the copy's, so the two can be compared without a
# separate pass. `ART_HST` is optional: without it the run measures the
# native path alone and ends on the refusal a user with no hst-imager would
# see. Delete the output image first — SAFE_CREATE refuses an existing one.
cd src-tauri && ART_OSINSTALL_DEST="E:\amiga\ProjeART\dist-3.2" \
  ART_CARD_OUT="E:\amiga\ProjeART\dist-3.2-fallback.hdf" \
  ART_PFS3="E:\amiga\Amigatolon\hstimager\pfs3aio" \
  ART_HST="E:\amiga\Amigatolon\hstimager\hst.imager.exe" \
  cargo test carry_the_real_dist_tree_through_the_fallback_path_when_asked -- --nocapture --ignored
cd src-tauri && cargo deny check                       # licences and advisories
pnpm tauri build                                       # full bundle (slow)
```

A claim in this file is only valid if the command that proves it was actually
run. Do not carry a PASS forward on faith.

---

## Where ART sits, and the ceiling that comes with it (2026-08-19)

**This is the most useful thing a future session could know before planning
more content work**, and it came from external research the owner asked for
after the content-layer round met his own packages and could not place two of
the three.

**Every established Amiga distribution builder runs the install inside an
emulator.** HstWB Installer, AmiKit, AmigaSYS and ClassicWB all do this, using
the user's own OS files; HstWB's own README says so outright — *"HstWB
Installer uses WinUAE or FS-UAE emulator to run the installation process"* —
with its logic in AmigaDOS scripts and 68000 binaries. That is *how* they
install BoingBags. They do not decrypt anything: they run the package's own
`Updater` where the password already lives.

**ART is not in that family.** `amitools`'s `xdftool` does what ART does —
host-side placement plus a metadata sidecar (`.xdfmeta`, ART's `.uaem`) — so
ART sits in the **tooling** family, and the distributions sit in another. The
walls this round kept hitting are that family's ceiling, not defects in the
round's own work:

- **[ART-166](ISSUES.md)** — a BoingBag's payload is password-encrypted and
  the password lives in an Amiga executable. A host-side placer cannot read
  it. A builder that boots an Amiga does not need to.
- **[ART-168](ISSUES.md)** — Cloanto's own knowledge base (RetroPlatform KB
  19-106) names this by description: for *directory-based* distributions,
  filenames with characters above 128 "as used in Amiga locale names" are a
  known problem, and AmiKit's solution is to carry those files in an LhA and
  extract them **on the Amiga**.

**The owner's decision, taken the same day and recorded so it is not
re-litigated by accident:** do **not** force the host side, and **no bypass of
the BoingBag password will be written.** The content layer's host-side work
stands — archive media, nested media, the collision preview, the two entry
points, the screen. An **Amiga-side install step becomes its own round**, and
it is cheaper than it sounds: `core/winuae::launch_winuae` already exists, and
what is missing is running it unattended and reading the result back. Nothing
about it is scheduled for next week; it is a round, not a patch.

**The round's own closing bar was not met, and was substituted — said here
because being silent about it is the part that would be wrong.** The content
layer's design spec §7 set the bar as *"a 3.9 tree with BoingBag 2 applied
reports a different Workbench version than a base one; booting it under
WinUAE … is what closes this round. A tree that builds and does not boot is
not a distribution."* No BoingBag file ever reached a tree (ART-166), so what
actually closed the round was the **3.9 overlay** boot — `Kickstart 40.68,
Workbench 45.1 (13-Nov-00)`, measured from the running system. That is a
different result, reached by the same method, and a good one. Nothing in the
record ever claimed a BoingBag booted; what was missing until the final
whole-branch review (m3) was the sentence saying the bar moved, and where it
moved to: the Amiga-side round above.

Two of the spec's §8 hazards went unexercised for the same reason and are now
**filed rather than merely absent** — [ART-171](ISSUES.md) (`WBStartup` and
`Devs` arriving on a tree for the first time) and [ART-172](ISSUES.md) (a
language pack colliding with the base `Locale`). The second is the one worth
remembering: the Türkçe run reported **0 rows of every collision class**,
which reads like "no collisions" and was in fact ART-168 comparing against a
drawer name nothing had ever written. The cleanest number the round produced
was measuring the wrong thing.

**And the lesson that cost the most to learn, which points the same way.**
`Libs/WORKBENCH.LIBRARY` carries no `$VER:` marker, so the 3.9 overlay's
replacement of it (193,400 → 199,852 bytes) landed in the `Unversioned`
bucket — while being the single change that turns `Workbench 44.5` into
`Workbench 45.1`. **The boot proved the decisive change and the collision
classifier could not.** Generalised: if the library carries no readable
marker, other decisive files will not either, so **any future "did the update
take?" check built on collision classes alone will be confidently wrong about
the one file that matters.** Ask the running system its version; do not infer
it from a copyright line, which is exactly the mistake that let a 3.5 tree be
recorded as 3.9 for most of a day ([ART-169](ISSUES.md)). The method for the
next boot check follows from it: let the tree boot normally and have it write
`Version >SYS:version.txt FULL`, rather than interrupting `Startup-Sequence`
to reach a shell — a healthy tree resists that by design, which is itself how
the fix was confirmed.

---

## Stage plan

This supersedes the phase numbering in `roadmap.md` for scheduling purposes.
`roadmap.md` still defines *what each phase contains*; this defines *what order
the remaining work happens in and why*.

### ✅ Stage 1 — Data safety (complete, 2026-08-09)

Made it impossible for ART to quietly destroy user data.

- `core/safety`: atomic writes, generational backups
- ADF mutation pipeline: `read → mutate → validate → backup → commit`
- Bounds-checked block access (a bad block number no longer aborts the app)
- AmigaDOS hash compatibility fix
- FF.CFG / PiStorm config round-trip (hand-tuned settings survive)

Defects fixed: [ART-001 … ART-020](ISSUES.md#fixed).

### ✅ Stage 2 — Workflow Engine (complete, 2026-08-09)

Made the spec's central interaction real: *drop something → what can I do?*

- `core/workflow/builtin.rs`: 22 registered workflows across ADF/LHA/HDF/ROM/folder
- `Navigate` vs `Execute` split; `run_workflow` command refuses non-read-only actions
- Dashboard renders the engine's plan instead of hard-coded buttons

### ✅ Stage 3 — Systematic audit (complete, 2026-08-09)

Audited every module with working logic. HDF/RDB held the worst of it: nine
defects ([ART-021 … ART-029](ISSUES.md#fixed)), including three critical ones.

- Hard disk images are now created **sparsely** — a 4 GB HDF no longer needs
  4 GB of RAM, and `open_hdf` reads a header window instead of the whole file
- Creating an image never replaces an existing one (HDF and ADF alike)
- RDSK logical-drive fields written at their documented offsets, verified
  against `amitools` — disks ART creates now describe their real capacity
- Partition layouts that do not fit are refused instead of silently truncated
- Folder scans are depth-limited and do not follow symlinks

`core/analysis.rs` and `core/profile.rs` were reviewed and found sound. The
remaining modules are stubs — building them is feature work, not audit work.

### ✅ Stage 4 — Shared infrastructure (complete, 2026-08-09)

Not polish — these were hard prerequisites for Stage 5, which both addenda name
directly.

| Item | Spec | Needed by |
|---|---|---|
| Operation Log + error IDs | §53, §68 | AI plan audit trail, `origin: ai-plan` (§45.5.8) |
| Job Queue (background work, responsive UI) | §54, §55 | Aminet download/install queue (§41.5.6) |
| Beginner / Power User mode | §47, §48 | Aminet mirror & cache UI (§41.5.6) |

**Operation Log** — `core/oplog` records every operation that touched user data:
what was done, where the backup went, whether verification passed, and on
failure the error ID. Append-only JSON Lines, readable in Settings and
exportable as text. `OperationOrigin::AiPlan` is already in the model so the AI
layer has somewhere to declare itself from day one. Errors carry stable `ART-*`
identifiers (§68).

**Job Queue** — `core/jobs` gives platform-independent operations a
`ProgressSink` to report through and a cancel flag to check; `commands/jobs.rs`
runs them on background threads and forwards progress as throttled
`job-progress` events. Collection scanning and LHA extraction are jobs now, with
a global `JobBar` in the app shell so work started in one studio stays visible
and stoppable from anywhere. Cancellation is checked only between whole units of
work, so stopping never leaves a half-written file.

**Beginner / Power User mode** — `usePowerMode()` drives what the UI shows. In
Beginner mode the raw-data studios (Hex Tools, PiStorm), the Advanced action
group and block-level numbers are hidden; nothing is disabled, and Settings
explains what the switch changes.

### ✅ Stage W — Writing into volumes (complete, 2026-08-09)

The commander became a real file manager. `core/volume/write/` writes OFS, FFS
and INTL at any geometry — multi-block bitmaps through `bm_pages[25]` and the
extension chain, OFS data-block headers, extension chains for files past 72
blocks, hash insert/remove/rename per volume.

The decision the stage turns on: **a 2 GB image is not a big floppy.** An image
of 16 MiB or less keeps the audited whole-file pipeline exactly as it was;
anything larger is written in place under an undo journal
(`core/volume/journal.rs`) that saves every block before touching it and is
fsynced before the first image write. `Journalled::write_block` refuses any
block it has not saved, which turns "remember to journal first" from a rule
into a compile-time-shaped one. A journal found at mount blocks every write
until the user decides, and rolling it back restores the image byte for byte —
proved by a test that really kills the process at three points mid-write.

Also landed: `.uaem` sidecars in WinUAE's format so protection bits and
comments survive a trip through a filesystem that cannot hold them; F3–F9 on
the keyboard and on a bar; checkout/checkin gated on SHA-256 so an editor that
opens and closes a file cannot cause a write; `.info` pairing on rename and
delete; and a pre-flight copy plan that reports blocks, name problems and
collisions before anything is written.

The oracle now runs **both ways**: ART writes and amitools reads, amitools
writes and ART reads. Both halves are needed — a reader and a writer that agree
with each other and with nothing else is exactly what ART-032 … ART-035 were.

Still deliberately read-only: dircache volumes (a directory is stored twice),
long-filename volumes, PFS3 and SFS, and any volume whose bitmap is marked
invalid. Each is listed by name with the reason, never hidden.

### ✅ §82 — One-click WHDLoad install (complete, 2026-08-09)

The spec's first MVP success scenario, end to end:

```text
Game.lha → DROP → WHDLoad detected → Install to HDF → Backup → Apply → Verify
```

Every arrow but one already existed. The missing piece was working out **what
inside the archive is the game**: a WHDLoad archive wraps the pack in a drawer
and puts the drawer's `.info` *beside* it, not inside. Copy only the drawer and
the icon is left behind — the game is then on the disk and **invisible on
Workbench**, which is indistinguishable from a failed install.

`core/whdload` is that analysis, and it is pure: it takes a list of names and
returns where the pack is, what it is called, which file is the slave, where
the icon is, and what in the archive is not part of the game (usually a
readme, left behind and said so).

The install refuses rather than guesses. Each refusal is a case where going
ahead produces something that looks installed and does not work:

| Refusal | Why |
|---|---|
| Low or unknown WHDLoad confidence | §14/§34: an uncertain detection is never acted on as fact |
| The archive holds an `Install` script | The game is not installed yet, and running the script needs an Amiga |
| A drawer of that name is already there | Writing a game over one that is already there is not a one-click decision |
| It does not fit | With the real block numbers, before the disk is touched |
| Names AmigaDOS cannot store | A slave looks for its files by name; renaming them gives a game that starts and finds nothing |
| The archive did not unpack completely | Half a game is not a game |

Backup, journalling and per-file verification all come from Stage W — the whole
install runs in **one** volume session, so a floppy-sized image is backed up
once rather than three times.

### ✅ Phase 0a — Live bugs fixed, one filesystem writer (complete, 2026-08-10)

Found live in the running app, not by audit: ADF Studio could not open a
single real bootable disk, the shell clipped instead of scrolling, and a
disabled button looked exactly like a live one.

- **The shell scrolls and scales.** `min-height: 0` on `.app-main` and
  `.app-content` so `overflow` actually engages, `@media` breakpoints that
  collapse the sidebar and drop quick actions to two columns below a width,
  and one central content width instead of eleven hand-written `maxWidth`
  values (ART-039, ART-040).
- **`:disabled` and `:focus-visible` styles exist.** A disabled primary button
  used to keep its solid accent fill (ART-039).
- **ADF Studio opens bootable disks.** The root block was being read out of
  68000 boot code instead of computed from the volume's size — `core/adf`
  predated the geometry Stage R gave `core/volume` and never adopted it. The
  same root cause gave HD ADFs half their reported capacity (ART-037,
  ART-038).
- **A refusal is not an error.** "This is not a WHDLoad package" used to throw
  the same red, `ART-*`-coded banner as a real failure; it now renders as an
  amber `Refusal`, and a review pass on the same work confirmed real failures
  (a corrupt archive, a permission error) still throw.
- **Both install commands refuse atomically when a package will not fit**,
  instead of discovering it mid-copy and reporting the rest as `skipped` — a
  WHDLoad pack missing its `.slave` used to be a broken game with no warning
  (ART-044).
- **`MutationOutcome` is built from the write session's own still-open
  device**, not a second file open after the fact — closing a race where an
  external lock (antivirus scan-on-close, a search indexer) could turn a
  durable, successful write into a reported failure and lose the backup path
  with it (ART-045).
- **`core/adf/mutate.rs` — the second AmigaDOS writer — is retired.** 858
  lines that hardcoded DD floppy geometry throughout and could never write a
  hard disk. ADF Studio and the file manager now share exactly one writer
  (`core/volume/write`). Its five previously-fixed defects (ART-007, ART-008,
  ART-011 … ART-013) moved with their tests rather than disappearing — none
  reopened, all still pinned.
- **Validation now measures every image against its own geometry**, not a DD
  floppy, and the whole in-memory result is checked before a write commits,
  not only the blocks the operation touched (ART-041, ART-042).

Defects fixed: ART-037 … ART-040, ART-044, ART-045 (this phase's own),
ART-041, ART-042 (found restoring the write pipeline's validation step). Left
open: ART-043 (a partition inside a small image), and five findings recorded
but not fixed — ART-046 … ART-050.

**Left undone on purpose: the hardware verification rung.** A boot-test ADF
(`.superpowers/sdd/2026-08-09-phase-0a-live-bugs/artifacts/task-10-boot-test.adf`,
gitignored) was built by the changed write path and cross-checked with
`xdftool` — `xdftool … list` reports the volume and the file it holds. No
human has mounted it in WinUAE or on a Gotek yet. A pass is `DIR DF0:` listing
`Readme`, `TYPE DF0:Readme` printing `hello from ART`, and `INFO DF0:`
reporting volume `Work` with no errors. This image is **not bootable** — ART
installs no boot code, only an RTS stub when the `bootable` flag is set — the
claim under test is that a real Amiga mounts the volume and reads the file
back, not that it boots.

### ✅ Phase 0b — Dead code removed, interface speaks Turkish (complete, 2026-08-10)

Two slices, eleven commits.

**Dead code.** Nine code paths that were registered, typed and reached by
nothing were deleted: Rust commands `adf_extract_to`, `panel_plan_folder_copy`,
`volume_write_bytes`, `lha_extract_job`, `sources_get`, the helper
`write_bytes_into`; TypeScript wrappers `adfExtractTo`, `panelPlanFolderCopy`,
`volumeWriteBytes`, `sourcesGet`; and the `ComingLater` page (`common
.comingLater`'s key now feeds the "Coming Later" badge that Dashboard used to
hardcode). The plan named six; three more (`write_bytes_into`, `sources_get`,
the `comingLater` badge) turned out dead once their last callers were gone and
were removed in the same pass. Rust test count dropped from 687 to 683 as the
four tests pinning the deleted paths went with them. `ART-047`, `ART-048` and
`ART-051` — hygiene issues left over from Phase 0a — were closed alongside.

**Turkish.** `tr.json` ships beside `en.json`, both at 814 leaf keys. All
fourteen screens, every shared component, and the eight `src/lib` modules that
used to build English sentences by hand (`sources.ts`, `whdload.ts`, …) now
return a `Phrase { key, params? }` that the caller renders through `t()`. The
language switcher shows each language in its own name (`English` / `Türkçe`).
A parity test (`src/i18n/parity.test.ts`) fails the build if the two
catalogues' key sets diverge, a value is empty, or an interpolation variable is
dropped — this is also where `pnpm test` and vitest entered the project;
before this phase there were no frontend tests at all. 10 frontend tests now
pass alongside the 683 Rust ones.

**The boundary, recorded rather than fixed:** `CoreError` and
`WhdloadRefusal { reason, suggestion }` are English sentences written in
`core/` and `commands/`, and they reach the UI unchanged regardless of the
chosen language ([ART-060](ISSUES.md#open)) — `core/`'s independence rule means
this needs a real design decision, not a quick fix. `formatAge`
(`src/lib/sources.ts`) still renders "1 weeks ago" in English, a pre-existing
defect task 7b reproduced faithfully rather than fixing mid-refactor
([ART-061](ISSUES.md)). And no language has been checked on a running
screen — every string was verified by the parity test and by reading, never by
opening the app ([ART-062](ISSUES.md#open)); several Turkish strings are
substantially longer than their English originals and sit in tight controls,
listed in that issue so the check is possible.

### ✅ Phase 1a — The Commander (complete, 2026-08-11)

Planned as eight tasks, ran to 35 commits. The Files screen stopped being a
plain two-pane copier and became the Norton/Total Commander the spec always
called it: real selection, batch operations, sorting, a filter, and — outside
the plan entirely — the first fix ART has ever needed to make a disk actually
*boot*.

**Selection and focus (Tasks 1–2).** Pane focus is now a real value
(`focused: Side`), not derived from which pane happens to hold a selection,
and Tab moves it. Selection became `Set<string>` per pane — Shift-click a
range, Ctrl-click to toggle, Insert to mark-and-advance, Ctrl+A to select
all — behind a pure, tested reducer (`src/lib/selection.ts`). This is also
where frontend testing entered the project for real: Task 1 wrote the first
`.test.tsx` (Vitest + jsdom, scoped to that extension) *before* touching a
1600-line component with no prior coverage, specifically because focus and
selection together touch nearly every handler in it.

**Batch operations (Tasks 3–5).** `HostSelection` — a `CopySource` spanning
several host roots, each keeping its own name at the destination — let batch
copy and delete reuse the existing tested copy engine rather than growing a
second one. `volume_plan_copy_many` / `volume_copy_in_many` /
`volume_delete_many` give local→volume and delete their all-or-nothing
guarantee: a cancelled batch copy commits nothing, a batch delete that can't
fully succeed (a name that's gone, a directory that isn't empty) deletes
nothing. Several `.lha` archives dropped on a disk at once each get their own
drawer, staged into one write, so a cancelled multi-archive install can't
leave two games half-installed. **Volume→volume multi-select and
volume→local multi-select did not get the same treatment** — see ART-064 and
ART-065 below; the roadmap named only local→volume and single-file
volume→volume as in scope, and Task 8 confirmed no primitive exists to build
the other two batched directions on top of.

**One listing order, sorting, the filter, the restyle (Tasks 6–7, plus 6b and
Attr, added).** There had been three different listing orders — local
folders-first, ADF name-only, HDF not sorted at all — now one floor
(folders first, case-insensitive name) everywhere, with a client-side
per-pane column sort on top and `PanelEntry.date` carried end to end so date
is sortable at all. A Total Commander-style filename mask (`*`/`?`, whole-name,
case-insensitive) narrows what a pane shows and clears the pane's selection
when it changes, so a selection never silently keeps entries the mask just
hid. Outside the original eight tasks: the Files screen was restyled as
Total Commander from two reference screenshots, with row icons, file-type
colour and an Attr column backed by the protection-bit reader that already
existed for the attributes dialog.

**ART-063, out of plan: ART could not write a disk an Amiga would boot.**
Found while building the hardware-verification artifact this phase set out
to produce anyway. The `bootable` flag wrote a bare `RTS` at offset 12 —
every test, and the amitools oracle, only ever asked whether the boot block
was *well-formed*, and an `RTS` is. Only Kickstart can answer whether the
code runs. `core/adf/bootcode.rs` now assembles real boot code from the
documented contract (read `ExecBase`, `FindResident("dos.library")`, return
`rt_Init` in `A0` with `D0 = 0`) — ART's own implementation from the published
LVO table, not Commodore's copyrighted boot block. **Fixed and verified by
actually booting `test/art-bootable-test.adf`** — see below.

**The headline: real hardware, reached for the first time.** Two rungs
existed before this phase — `cargo test` (ART agrees with itself) and the
amitools oracle (ART agrees with an independent implementation) — and both
can be wrong together, which is exactly how ART-032 … ART-035 shipped behind
a green suite. This phase reached the third: `test/task-10-boot-test.adf` was
mounted, its volume `Work` listed, and `Readme` inside it read back
`hello from ART`; `test/art-bootable-test.adf` — the same disk with real boot
code — booted to a CLI prompt. Both under **licensed Kickstart and Workbench,
Amiga Forever under WinUAE, in A1200 and A500+ configurations — not bare
metal at the time**, and `test/README.md` said so plainly rather than letting
the A1200/A500+ machine names imply real hardware. (**That caveat has since
been lifted** — the bootable image booted a real A500/A500+ on 2026-08-12; see
the entry under Phase 2b below.) A `DIR DF0:` on the non-bootable
image also read AmigaDOS's own free-space count off ART's bitmap for the
first time (878 KB free of 880 for a 14-byte file, correct) — nothing set out
to test that; it came free with the screenshot.

**Debt opened this phase, not fixed:** ART-064 (volume→volume multi-select
refuses), ART-065 (volume→local multi-select is concurrent per-entry jobs,
not one operation), ART-066 (`archives_plan_install` unpacks synchronously on
the command thread instead of a job), ART-067 (a batch archive install can't
be stopped mid-archive), ART-068 (the filter's empty-vs-no-match message is
inferred from two counts rather than carried as a flag). None are data-unsafe;
all are named in [ISSUES.md](ISSUES.md#open) with what fixing each would take.

**What Task 8 added: the one test the phase owed.** Volume-to-volume copy
(`volume_copy_between`) shipped in Stage W and was wired to F5, but no test
ever exercised the command's own pipeline — only the bare staging primitives
in `core/volume/write/copy.rs`. `commands/volume_write.rs` now has
`a_tree_copies_between_two_images_through_the_command_pipeline`, which stages
a source volume's tree into a temp folder and runs it through
`run_copy_in_staged` — the same journal-check, whole-image-validate,
guarded-backup pipeline every write command in that file goes through — and
asserts the destination has the tree while the source is byte-for-byte
unchanged. Rust tests: 729 → 731.

**Nothing in this phase has been checked on a running screen.** Not the
restyle, not the Attr column, not the filter box, not multi-select — all of
it was written and tested (Rust + Vitest), never opened in `pnpm tauri dev`.
ART-062 (no language checked on screen) predates this phase and is still
open; it now also covers a phase's worth of new UI nobody has looked at.

### ✅ Phase 2a — Content-first detection and containers (complete, 2026-08-12)

Plan: [2026-08-11-phase-2a-content-detection-and-optical.md](superpowers/plans/2026-08-11-phase-2a-content-detection-and-optical.md),
amended 2026-08-11 (A1–A7 in that file's Amendments section).

The commander stops asking what a file is *called* and starts asking what it
*is*, and grows container kinds behind the pane model it already has: a CD, two
archive formats, and Commodore 64 disk and tape images.

Done: content-first `detect()`; an ISO9660 + Joliet reader; a disc pane with F5
out of it in both directions; **an independent 7-Zip oracle** for the disc
reader, raw layouts included; Mode 2/XA Form 1 (which closed ART-075); and one
shared archive security gate with the format behind a backend trait.

Then ZIP and 7z behind that gate, and the archive pane with the virtual tree
that gives a flat list of names folders to walk into (Task 4). Then the
Commodore side (Task 5): D64/D71/D81 and T64 read and browsable, TAP/PRG/CRT
identified and described — with the 40-track variants and the "a T64's header
is not to be trusted" rules amendments A3 and A4 added.

**What the phase actually changed, in one line each:**

- **A file is what its bytes say**, not what its name says. `.img` holding a
  floppy is a floppy; an LHA renamed `.dat` still opens. Fixing that found
  [ART-076](ISSUES.md#fixed) — LHA's own signature had been matched at the
  wrong offset all along, so content-first detection had never worked for the
  format ART is built around.
- **Four new container kinds behind the one pane model**: a CD, ZIP, 7z, and
  Commodore disks and tapes. Not four new screens.
- **One security gate for every archive** (`core/archive/extract.rs`), written
  *before* the second format arrived, with one hostile-archive test each
  backend is run through.
- **An independent oracle for the disc reader** (7-Zip), which is what
  replaced the cancelled real-Amiga-CD rung and what closed
  [ART-075](ISSUES.md#fixed)'s "two layers wrong together" for raw tracks.
- **The MSRV moved to 1.93**, deliberately, so 7z uses a maintained decoder
  rather than one fourteen versions behind.

**Defects found and fixed on the way:** ART-075 (Mode 2/XA), ART-076 (LHA
detection), ART-077 (the file manager ignored the object a workflow sent it —
so "Open in the file manager" had been doing nothing since Task 3), and
**ART-079** — a 7z from any real tool gave one entry another entry's bytes,
found by pointing the reader at a file 7-Zip wrote while every fixture ART
builds for itself passed.

**The third time that shape has bitten this project** (ART-032…035, ART-075,
now ART-079), so it is now one command rather than an idea:
`read_foreign_archive_for_oracle_when_asked` and
`read_foreign_c64_for_oracle_when_asked` read files ART did not write, and
print a hash per entry rather than a length — the defect gave every file
exactly the right length.

**Owed, recorded not fixed:** [ART-078](ISSUES.md#open) — Rock Ridge and the
Amiga `AS` System Use entry are not read, so an AmigaOS CD's protection bits
and file comments are lost on the way out.

**Nothing in this phase has been seen on a screen.** Not the disc pane, not
the archive pane with its virtual tree, not the C64 pane. Every claim above
rests on tests and on two oracles (ART-062).

**The real-Amiga-CD verification step was cancelled** (amendment A1) — it
assumed licensed AmigaOS media reliably to hand. `scripts/iso-oracle-check.py`
covers the risk it existed for, with an implementation that shares no code with
ART's.

### ✅ Phase 2b — The Files screen, done right (complete, 2026-08-12)

Plan: [2026-08-12-phase-2b-commander-ui.md](superpowers/plans/2026-08-12-phase-2b-commander-ui.md).
Brief: [brief-files-commander-ui.md](brief-files-commander-ui.md) — written from
the **first human look at the running screen** plus the user's own twenty-year-old
Total Commander `wincmd.ini`, and it supersedes the earlier restyle briefs.

The verdict it starts from: the first restyle got Total Commander's *components*
right and its *gestalt* wrong. TC is not a widget on a page; it is the window.

Done (3 of 8 tasks):

- **Task 1 — the panes fill the window.** Page title and intro gone, full-bleed
  grid, and panes that are equal **by construction** (`minmax(0, 1fr)`, not a
  content-sized flex — that is what made the right pane come up short). Rows
  lost their `→`/`←`/`X` buttons; TC puts nothing on rows. `Ctrl+B` collapses
  the sidebar and the state survives a restart.
- **Task 2 — one visual world, in the user's own colours.** Decoded from his
  `[Colors]`: pane `#1C1F24`, cursor bar pure yellow with black text
  (`InverseCursor=1` settles the question the first restyle guessed wrong),
  selected names `#FF3C3C`. Chrome is one step lighter than the rows instead of
  Windows-95 grey, in dark and light alike, and focus moved to the path row.
- **Task 3 — minimal chrome, and F6 means Move.** The pane header is
  `[source ▾] [path] [filter]`; the button strip behind it is a Settings
  toggle, off by default, because his `[Layout]` is `ButtonBar=0, DriveBar1=0,
  DriveCombo=1`. The combo lists **enumerated** mounts plus the six things ART
  opens — no hardcoded letters — and a folder under no listed mount says so
  rather than claiming one. The command line stops being decoration: it
  navigates and filters, and refuses anything else *by name* (§56 — it is not
  a shell, and a prompt-shaped box that swallows a keystroke is worse than
  none). The F-key row is one row that cannot become two, with labels giving
  way to keycaps below 1000px. Everything that used to push the panes down
  from the top of the page — error, message, busy — is one status strip inside
  the dock, and the permanent collision footer moved into the copy dialog,
  where Total Commander asks that question, with its default now a Setting.

  **F6 is Move** (Shift+F6 renames), and it is the one function key that can
  destroy the original, so it runs §92's pipeline in full: validate → offer
  the icons (§7.1) → confirm → copy → **re-list the destination and look for
  every moved name** → delete. Nothing is deleted unless the destination's own
  listing has it, so the worst a stopped move can leave is a duplicate. A
  collision is refused up front rather than handed to the overwrite policy —
  "leave it alone" would skip the copy and then delete the source. Three
  directions have no primitive underneath and are refused by name rather than
  half-built: out of a host folder ([ART-080](ISSUES.md#open) — ART owns no
  host-side delete), several entries between two images (ART-064), and a
  single *file* between two images ([ART-081](ISSUES.md#open) —
  `volume_copy_between` addresses a directory). Task 5's `F2`/`Ctrl+R` refresh
  came forward into this task, because hiding the button strip left Refresh
  with no other way to be reached.

**Out of plan, and the bigger news of the day: a real Amiga booted a disk ART
wrote.** `test/art-bootable-test.adf` was put on a **Gotek** and cold-booted a
real **A500 / A500+** running **Kickstart 3.9** (the screen's copyright line
reads `1985-2002`) straight to an AmigaDOS `1>` prompt. Photographed.

This is the rung the project has been careful not to claim since Phase 1a. The
boot code is ART's own, assembled from the published LVO table, and until this
photograph "it runs on a real 68000" was an assumption: the emulated passes
were an A1200 (68020) and an A500+ *configuration*, both under WinUAE with
licensed ROMs. A Gotek also presents the image the way FlashFloppy reads an
ADF — sector by sector — rather than as a file an emulator maps into memory,
so the disk had to behave like a disk.

**One caveat survives and is not being quietly dropped:** a Gotek is not a
mechanical drive. Nothing ART has written has been through a real floppy head
on physical magnetic media. `test/README.md` carries that as its own rung.

- **Task 4 — Enter opens the container, in the same pane.** The phase's
  headline, and the one thing here that was *seen working on a real disk*:
  Enter on `art-bootable-test.adf` turned the pane into the floppy — breadcrumb
  `test\art-bootable-test.adf`, volume row `Work FFS 877 k of 880 k free` —
  and Backspace came back out with the cursor sitting on the ADF it had
  entered. What a row opens into comes from `analyze_paths`, never the
  extension. `PaneState.host` is the mechanism, threaded through all seven
  `openX` functions as a *required* parameter so the compiler names every call
  site. Per-pane history (Alt+Left/Right) treats a container as a place.
- **Task 5 — the keyboard covers everything.** Space marks where you stand,
  Insert marks and advances, the numpad marks by mask and inverts,
  type-to-search moves the cursor (and only the cursor — a search must never
  throw away a selection), Alt+F1/F2 open the source combos.
- **Task 6 — tabs and session restore.** A tab is a place plus how you were
  looking at it, built on task 4's `PaneLocation`, so a tab can live inside an
  image. Persistence falls out because the tab is *derived* from the pane
  rather than remembered alongside it.
- **Task 7 — colour rules, histories, confirmations.** Three shipped colour
  rules that answer the question a directory of Amiga files poses — walk into
  it, unpack it, identify it — sitting in front of the built-in classification
  rather than replacing it. A dropdown history on the command line. Right-button
  marking as a setting.

**Filed rather than built** (the phase's own rule): [ART-080](ISSUES.md#open),
[ART-081](ISSUES.md#open) (move's missing primitives),
[ART-087](ISSUES.md#open) (`Space` does not count a directory) and
[ART-088](ISSUES.md) (the writer ignores the `d` protection bit; the file
manager asks, the engine does not refuse).

**What was and was not seen.** The acceptance walk is in the plan file, point
by point: six of the twelve were verified on the running screen, four on tests
alone, and two were not looked at. The two that matter:

- **The light theme was never opened.** Its tokens derive from the dark ones by
  role; nobody has looked at it.
- **Session restore is half verified.** Two tabs made in the running pane do
  reach `settings.json` as two tabs, with their paths, the focused side and the
  command history — so the *write* half is real. The **read-back half has still
  not been seen**, because nothing has closed and reopened ART since. The
  round trip itself now has its own tests (`src/lib/paneSession.ts`), added
  because the acceptance walk said it had none.

### ⏳ PiStorm Image Builder — SD-0 … SD-5 (planned, not started)

**This is the project's largest unbuilt feature and, per the user, its point.**

Gap analysis: [sd-appliance-gap-analysis.md](sd-appliance-gap-analysis.md)
(2026-08-11, from the PiStorm multiboot architecture; rescoped 2026-08-12). It
lists **only** what ART lacks; everything ART already has — WHDLoad install,
ADF/LHA import, RDB creation, Gotek prep, config round-trip, journalled
writes, Aminet, oplog, the job queue — the build consumes rather than
reimplements.

The story: build a complete PiStorm/Emu68 **image file** on Windows, verified,
that boots a real Amiga once the user has flashed it.

**Scope decision, 2026-08-12: ART builds the image; it never touches physical
media.** The user flashes the `.img` with whichever imager they already have.
This deletes **G1** — raw `\\.\PhysicalDriveN` access, device enumeration and
identification, dismount/lock, verify-back and the typed-confirmation UI
around all of it — which was blocker number one and ART's single largest
safety surface. G8 becomes image validation rather than card validation (and
moves up, since it is now the last thing before the file is handed over), and
G12 (card backup) goes with G1: the build manifest already describes how to
*rebuild* an image, which is better than a 32 GB blob. Everything that makes
the image an Amiga — partitioning, filesystems, the OS, the content, the
multiboot layout — is untouched by the decision and is all file work.

**SD-0 is complete** ([sd0-prior-art.md](sd0-prior-art.md), from the user's own
research). Three of its findings change what gets built, not merely how:

- **The card's shape is exact now**: MBR, a FAT32 primary, then 1–3 `0x76`
  primaries, each of which the m68k side sees as a separate hard drive — and
  **the RDB sits at a byte offset inside one of those**, not at offset 0. So G4
  is bigger than "write FSHD/LSEG": ART's RDB writer has to work at an offset,
  which is the same shape as [ART-043](ISSUES.md). **Both halves are done**
  (2026-08-13/14) — writing *into* a volume that starts at an offset was
  ART-043, and laying an RDB *at* one is `core/card/build.rs` — each with tests
  at both offsets.
- **PC-side PFS3 write is already solved and MIT-licensed.** `hst-amiga` /
  `hst-imager` read, write *and format* PFS3 and FFS with no emulator, and both
  existing imagers stand on them. G3 gains a Route E that is proven rather than
  speculative, and Route B (native PFS3 in ART, the flagship) gains a PC-side
  oracle — which likely pulls SD-2 in by weeks.
- **Multiboot mechanism B ships in the field today** on stock Emu68: one
  `config_{distro}.txt` per distribution on FAT32, and an Amiga-side selector
  that rewrites `CONFIG.TXT` and reboots. ART can generate the entire static
  side at build time; the selector itself is later work and must be ART's own
  code — the pattern is free, the implementation is not.

Also settled: install media is identified **by content checksum, not by
filename** (ART's own content-first rule, applied to OS media); Kickstarts are
A1200-only against a checksum DB; and the community-standard package set is
full of demo and conditionally-redistributable software, so SD-2's manifest
needs a **licence column** and anything not clearly redistributable ships as a
fetch task through the Aminet engine instead of as bundled bytes.

One blocker-grade unknown is carried into SD-1: SD-0's own exit test — drive
`hst-imager` on a scratch image end to end (init RDB, add a PFS3 partition,
format, copy a tree in) and verify the result with ART's readers and WinUAE.

**Three gaps the analysis did not name, added 2026-08-12 from the user:**

- **G14** — the build inputs that make a distribution *theirs*: wallpaper,
  WiFi credentials, prefs, Startup-Sequence additions. Every one is a config
  file, so §39/§40's "edit in place, never regenerate" rule applies to all of
  them, and the WiFi key is secret material that must stay out of the oplog,
  the manifest and any AI prompt. **Wallpaper is new scope** — it is in no
  existing document. WiFi is not: §45.5 designed `write_pistorm_wifi` with
  `@form.wifi_psk`, reachable only through an AI layer that is not built.
- **G15** — a **build** as a drag & drop target. ART already has exactly one
  drop pipeline (`analyze_paths` → `WorkflowEngine::plan`); what is missing is
  not the pipeline but the question "what does this file become in *this*
  image", as against today's "what can I do with this file".
- **G16** — multiboot as several complete AmigaOS environments and a boot
  menu, not a boot-priority field.

**Which Amiga is a parameter, not an assumption** (decision, 2026-08-12). The
gap analysis was written around "the A500"; that is one of the machines
PiStorm goes into. What varies by machine is data the build already carries —
which Kickstart ROM lands on the FAT32 partition, what the Emu68 config says
about the board, which OS release is installed, what partition geometry suits
the card — not a code path.

**Settled 2026-08-13: the classic PiStorm on a Pi 3A+, first verified on an
A500** (an A500+ follows around 2026-08-28). That is the hardware in the room,
which is the only kind of answer this question has. It is recorded *with* the
result rather than generalised from it — a card that boots an A500 has proved
one machine, not the line.

| Phase | Contents |
|---|---|
| **SD-0** | ✅ **Done 2026-08-12** — prior-art teardown, written up as [sd0-prior-art.md](sd0-prior-art.md). One exit test still owed (drive `hst-imager` end to end on a scratch image) |
| **SD-1** | The image has a shape: MBR + FAT32 boot partition (**G2 — done 2026-08-13/14, engine and screen**), RDB filesystem embedding FSHD/LSEG (**G4 — done**, also closed ART-084), build manifest (**G7 — done 2026-08-15**, with 7-Zip answering the half ART cannot check), image validation (**G8 — done 2026-08-15**), a build as a drop target (**G15 — done 2026-08-15**). **Every gap in SD-1 is built; what is left is a card flashed and booted** |
| **SD-2** | Content, preloaded: PFS3 via `hst-imager` (**G3 route E — done 2026-08-15, engine *and* screen**; route D dropped: E was already proven and needs no Kickstart), OS install engine (**G5 — done 2026-08-16, engine *and* screen**), ROM pairing (**G9 — done 2026-08-17, engine *and* screen**: the preload screen says whether a card's Kickstart suits the volume about to be written, and warns without blocking), launcher metadata export (G10), layout policy (**G11 — done 2026-08-15, engine *and* screen**: a pile of dropped files becomes a staging tree, not yet driven against real material and no staging tree has reached a card) |
| **SD-3** | It is *mine*: wallpaper, WiFi, prefs and Startup-Sequence, each edited in place (G14); multiboot as several complete environments and a boot menu (G16) |
| **SD-4** | The flagship: native PFS3 write in ART (G3 route B) — its own brief; route D's harness becomes its oracle |
| **SD-5** | Comfort: capacity planner and build profiles (G13) |

*The old SD-2 ("the card exists") is gone with G1, and everything after it
moved up a number; validation came forward into SD-1, where it is now the last
thing that happens before the file is handed over.*

Four decisions in it are worth knowing before reading the code they will
produce:

- **ART never touches physical media.** Everything is built into a sparse
  image file through the existing tested paths, and there the job ends: the
  user flashes it with the imager they already have. §56's raw-device guard
  stays in the spec as the reason ART does not do this, rather than as a
  problem ART has to solve.
- **v1 targets Emu68 only.** The classic Linux/Musashi route is a different
  build and is out of scope in writing, not by omission.
- **PFS3 preload starts borrowed and ends native.** Route D has the real
  `pfs3aio` do the writing inside a scripted WinUAE session — correct by
  construction, zero reimplementation risk — while route B (a native PFS3
  writer, the thing no other imager has) stays the flagship. Once B exists, D
  is its oracle.
- **A distribution is configured in ART, not afterwards on the Amiga.**
  Wallpaper, WiFi, prefs and Startup-Sequence are build inputs (G14), and
  every one of them is edited in place rather than regenerated — the same rule
  §39/§40 already impose on `FF.CFG` and `cmdline.txt`, applied to AmigaOS.

The milestone that matters first is not a preloaded 128 GB image: it is **one
image, built entirely from Windows, that boots a real Amiga into AmigaOS 3.2
with a recovery volume beside it** — with the machine and the board it was
proved on written down beside the claim. The bootable-ADF pass on a real
A500/A500+ (2026-08-12) is what that proof looks like, and what its honesty
standard is.

Sequenced after Phase 2a. Whether it goes before or after Stage 5's AI layer
is open.

### ⏳ Stage 5 — Spec addenda

From `ART-SPEC-ADDENDA-COMPLETE.md` in the project root. **Stage 4 has landed, so
both are now unblocked** — every prerequisite they name by hand exists:

| Addendum requires | Provided by |
|---|---|
| §41.5.6 download/install queue on the standard Job Queue | `core/jobs`, `commands/jobs.rs` |
| §41.5.6 Beginner hides mirrors/cache, Power exposes them | `lib/uxmode.ts` |
| §41.5.6 per-package actions follow §46 | workflow catalogue |
| §45.5.8 operation log entry with `origin: ai-plan` | `core/oplog`, `OperationOrigin::AiPlan` |
| §45.5.1 plans made of existing, tested workflows | `core/workflow/builtin.rs` |

- **§41.5 Software Sources Engine (Aminet)** — Stage A: catalog sync, search,
  download, readme, Collection. Stage B: one-click install to HDF, update view.
- **§45.5 AI Workflow Layer** — Stage A: read-only assistant. Stage B: plan
  generation with Plan Cards. Stage C: full multi-step scenarios.

**Both are now designed, neither is built.** The designs are:

- [design-software-sources.md](design-software-sources.md)
- [design-ai-layer.md](design-ai-layer.md)

Design decisions worth carrying forward, because they are not obvious from the
addenda:

- Aminet's local catalog is a **core trait (`CatalogStore`) with a file-backed
  implementation outside core**, not SQLite. The spec says SQLite; SQLite in
  `core/` would break the independence rule and putting it in `art.db` would put
  engine logic in React. A `SqliteCatalogStore` can replace it later without
  touching `core/`.
- Network access is a core trait (`MirrorClient`) implemented outside core with
  **`ureq`** — blocking, so it fits the existing job threads and drags in no
  async runtime. Both traits get in-memory test doubles, so CI never opens a
  socket.
- The AI layer's `DangerClass` is **derived from the existing `Safety` enum** by
  a total mapping, not defined as a second parallel enum. `Experimental`
  workflows are excluded from the tool whitelist in v1.
- Secrets can only reach a plan as `@form.*` placeholders. A literal supplied
  for a `Secret` parameter is a validator *rejection*, not something ART
  sanitises.

Build order: Aminet Stage A first — it is self-contained, fully offline-testable,
and it leaves the AI layer more engine to describe when its turn comes.

#### Aminet Stage A — where it stands

`core/sources` is built and tested (167 tests, all driven by an in-memory
mirror, no socket opened):

| Module | What it does |
|---|---|
| `index.rs` | `INDEX` parsing, structural rather than fixed-column; malformed lines counted, never guessed |
| `readme.rs` | readme field extraction, bounded and sanitised |
| `text.rs` | the bounding/control-character rules every repository string goes through |
| `catalog/` | `CatalogStore` trait, search and ranking, version resolution, `MemoryCatalogStore` + `JsonlCatalogStore` |
| `mirror.rs` | `Mirror` URL construction, `MirrorClient` trait, failover |
| `cache.rs` | content-addressed cache layout |
| `fetch.rs` | the §41.5.3 trust pipeline |
| `sync.rs` | `sync_catalog`, including the refusal to replace a good catalog with a bad parse |

Two new error IDs, stable from here: `ART-MIRROR-UNREACHABLE`,
`ART-INTEGRITY-MISMATCH`.

Verified against live Aminet mirrors on 2026-08-09, which **corrected the
format**: the index is five columns (name · directory · size · age ·
description) with a `|` header, not the path-first layout the design assumed.
The parser was rewritten and now reads 3 026 of 3 026 data lines from a real
256 KB `INDEX` sample with zero skips. Shipped defaults —
`https://aminet.net/`, `https://se.aminet.net/`, `https://ftp.fau.de/aminet/`,
index path `INDEX` — all served a byte-identical 7 229 355-byte index and
honoured range requests.

The whole stack is built: `core/sources`, `net/http_mirror.rs` (a `ureq`-backed
`MirrorClient` that follows only same-host redirects, refuses an HTTPS→HTTP
downgrade, never reports a `200` as a resume, and rejects a body shorter than an
announced `Content-Length`), `commands/sources.rs` on the Job Queue,
`src/lib/sources.ts`, and Aminet Studio at `/aminet` in the sidebar. Transport
tests run against a real socket on localhost, so CI stays offline.

Running the finished engine against real mirrors found two defects the fixture
suite could not — [ART-030](ISSUES.md#software-sources-aminet-415) (failover
concatenated a dead mirror's bytes onto the next one's) and
[ART-031](ISSUES.md#software-sources-aminet-415) (LHA level 2/3 headers could
not be opened at all, which also broke LHA Studio for typical Aminet
downloads). Both are fixed with regression tests.

Live end-to-end, after the fixes: 7 229 355 bytes of index, 85 435 packages,
zero skipped lines, and a real AmiSSL release fetched, size-checked, hashed,
validated and cached.

**Not done in Stage A**, listed so it is not mistaken for finished:
"Add to Collection" (§41.5.6's secondary action — downloads reach the cache and
stop there), editing the mirror list from the UI (`sources_set_mirrors` exists
and is tested; the UI only displays the order), and **actually launching the
app** — everything is covered by tests and both bundles build, but nobody has
clicked the button yet.

There is deliberately no workflow-catalogue entry: nothing you can drop leads to
Aminet in Stage A, so a `route::AMINET` no workflow points at would be dead code.
Stage B's update view is the first thing that plausibly needs one.

---

## Phase completion criteria

Carried over from `roadmap.md`; a stage is not done until all of these hold.

- Build: PASS
- Tests: PASS
- No critical errors
- No obvious data-loss risk
- UI remains responsive
- Documentation updated (this file, [ISSUES.md](ISSUES.md), [FEATURES.md](FEATURES.md))
- CHANGELOG updated

---

## Picking up next session

*Last touched: 2026-08-18. **The Collection is finished as a thing you keep,
and it merged** — two pull requests, the repository's first: PR #1 (41 commits:
the game index, the saved catalogue, artwork, the name tool) and PR #2 (the
README and five screenshots). `main` carries both, plus the ART-137 fix on
`fix-kickname`.*

***Seven defects, and the user found six of them by driving the screen.*** Not
one was reachable by the 1614 tests, and the reason is the same every time: the
suite runs synthetic fixtures, a handful at a time, to completion. Real material
is 2787 titles across two folders, hand-named, half of it interrupted half way.
[ART-131](ISSUES.md) (1448 of 1697 hardfiles refused), [ART-132](ISSUES.md),
[ART-133](ISSUES.md) (thirteen confirmations that never fired),
[ART-134](ISSUES.md) (an interrupted artwork run orphaned its 790 pictures),
[ART-135](ISSUES.md) (forty minutes where one would do),
[ART-136](ISSUES.md) (the ADF path assumed a filename convention **no real file
used** — zero of 847), [ART-137](ISSUES.md) (99 of 758 records reported a
Kickstart whose name was 68000 machine code).

***The user corrected the design twice, and was right both times.*** On chipset:
a shortcut reading a clear `ReqAGA` bit as "runs on OCS/ECS" was tested against
the catalogue and killed by 83 counterexamples, which left the field
unsourceable — and the answer was theirs from the start, *ask online*. On disk
sets: a first cut refused bare trailing numbers because `4D Driving 1` and
`Turrican 2` are indistinguishable, and their correction — **multi-disk is the
Amiga norm, `dune2-2` should base on `dune2`** — turned out to be possible by
reading every name at once rather than one at a time. 551 of 847 files belong to
a numbered set; accepting one only when it begins at disk one groups the real
ones and leaves eighteen disk-magazine issues alone.

**What is worth carrying forward:** *the tests keep being right about the code
and wrong about the world.* Every one of the seven was a correct implementation
of a wrong assumption — a naming convention, a politeness rate, a field's type,
a run that finishes. Driving it against somebody's actual disk is not a
verification step at the end; it is where these are found, and nothing cheaper
has worked yet.

*Before that, on 2026-08-17: **G9 landed** — the volume-preparation screen now
says whether a card's Kickstart suits the OS volume about to go onto it, from
two facts each side already recorded, and it warns rather than blocks. A final
review of the whole branch found ten things and all ten were fixed in one wave
([ART-129](ISSUES.md)); two were blockers and both were the same failure —
**the check that exists to warn said nothing.** Identity was allowed to answer
a question it was never asked, and the screen asked about only the first of
several partitions. The design's third proof case — a V40 tree that carries
its own compatibility modules coming back `Suitable` against a V47 card —
reached real material for the first time, which is the only evidence that the
check reads the tree's capability rather than comparing version numbers.*

*Before that, on 2026-08-16, three sessions in one day: first re-measuring the
real AmigaOS 3.2 tree through the native/fallback preload path (which closed
[ART-122](ISSUES.md) — a partition is formatted and filled by one tool — and
filed [ART-124](ISSUES.md)/[ART-125](ISSUES.md)), then chasing a guru the user
reported when they booted the result. That chase found
[ART-126](ISSUES.md): every RDB filesystem ART has ever embedded was
advertised with the wrong `PatchFlags`, so AmigaOS ignored the driver and the
volume never mounted — [ART-084](ISSUES.md)'s closing claim had been verified
only by tools that do not read that field. With it fixed, ART's own PFS3
volume booted a licensed Kickstart, and the full 3.2 tree booted to a clean
Workbench once [ART-127](ISSUES.md)'s three missing pieces were supplied.
Then the day's own two debts were closed: [ART-124](ISSUES.md) (the report and
the manifest describe the tree now, not the run — the manifest had been
crediting two components for 94 of the same files) and
[ART-125](ISSUES.md) (a byte total ART does not have is left unsaid rather
than printed as zero). Work is on `main`.*

### What to pick up first

Four things are open and none of them blocks another. In the order I would take
them:

1. **[ART-138](ISSUES.md) and [ART-139](ISSUES.md)** — both found by
   photographing the screens and both small. ROM Manager labels expansion-board
   ROMs (`A4091.rom`, `Blizzard_1230-IV.rom`) `CRC ERR`, which claims a damage
   ART has no way to know about; the right label is "not a Kickstart". Aminet
   renders its text inputs white in the dark theme.
2. **The light theme.** Its palette was widened once while looking at it —
   seventeen steps between page and panel is a range in arithmetic and not in
   perception — but it has not had a proper pass, and it is not in the README
   pictures because of that. Contrast is not decoration here: most of the people
   this is for are over fifty.
3. **The Collection's wave C** — the LaunchBox-shaped screen, and attaching
   artwork by hand. The 242 `.rp9` packages already carry a screenshot offline
   in `record.preview` that nothing renders yet, which is free material for it.
4. **G10 waves 2 and 3** — writing `igame.data`, drawer extraction, AGS export.

**The tree is green, measured fresh**: 1614 Rust passed / 0 failed / 11 ignored
(the real-media hooks, env-gated), 569 frontend, 1528 i18n leaf keys in
`en.json` and `tr.json` alike, `pnpm lint` clean, clippy clean at `-D warnings`,
`cargo deny` clean, the amitools oracle and the ROM table check clean.
**CI on GitHub was confirmed green two sessions ago** (run `31939802790`, commit
`544282c`) — but it took three pushes to get there, and the reason is worth
carrying forward rather than filing away: the merge to `main` was the first
time any runner had ever compiled the card builder (those commits sat
unpushed on `sd-1` for days), and it failed twice on `clippy::question_mark`
at `commands/card.rs:275` — a lint invisible on this machine, because CI runs
`stable` (clippy 1.97.0) and this machine is pinned at 0.1.95 (rustc 1.95.0).
A green local `cargo clippy` proved nothing about CI's; the fix was written by
reading the lint, not by reproducing it. CLAUDE.md already names this effect
from the other direction — raising the MSRV once turned on lints that had
been silent — and it is the same mismatch arriving from a newer toolchain
instead of an older one. The next session's local clippy will be green too,
and that is exactly the condition that hid this one.

### Where the phases stand

**SD-1 is complete.** Every gap it names is built — G2, G4, G7, G8, G15,
engine and screen throughout. **What is left in SD-1 is not code: a card
flashed and an A500 booted**, and that rung has not moved since 2026-08-14 —
the last materials inventory still names a microSD card, a USB reader and an
HDMI cable as the only unaccounted-for items, and nothing since suggests they
have arrived.

**SD-2 is most of the way built.** Four of its five gaps are done, engine and
(bar one) screen:

- **G3/G5 route E and native** — `core/preload/` plans and runs a preload.
  `core/preload/native.rs`'s `NativeFormatter` (`libpfs3` for PFS3, ART's own
  writer for FFS, launching nothing) existed with its own tests and its own
  `hst-imager` oracle since the previous wave, but shipped **unreachable from
  the product** for six commits — `commands/preload.rs` constructed
  `HstImager::at(...)` unconditionally regardless. [ART-120](ISSUES.md) fixed
  that: native runs first for every step, `hst-imager` remains a named
  fallback for exactly two typed, known gaps — [ART-113](ISSUES.md) (a
  non-ASCII AmigaDOS name defeats `libpfs3` 0.1.3's own UTF-8/Latin-1
  mismatch; refused by name, not fixable from ART's side) and
  [ART-117](ISSUES.md) (embedding a driver into a *foreign* card's existing
  RDB — one ART did not build itself). A review pass then found the wave had
  shipped the engineering but not the record — two warning strings still told
  the user ART cannot write PFS3 itself — and closed that as
  [ART-121](ISSUES.md).
- **G11** — `core/layout/` and its own `/layout` screen turn a dropped pile of
  files into a staging tree. Driven in headless Chrome against the real
  bundle; **not yet driven in `pnpm tauri dev` against real material, and no
  staging tree it built has been carried onto a card.**
- **G5 — the OS install engine, the gap SD-2 named as its largest, landed this
  wave.** `core/osinstall/{scan,plan,apply,verify}.rs` was run for real
  against the user's own 36-ADF AmigaOS 3.2 set and their Kickstart v3.1
  rev 40.68: 26 components switched on (`modules-a1200` among them, by its
  own condition against the real ROM, unchosen), 4030 files / 330 directories
  / 11.9 MB written to `E:\amiga\ProjeART\dist-3.2`, zero refusals — after two
  real recipe defects the run itself found and fixed
  ([ART-111](ISSUES.md), [ART-112](ISSUES.md)). **Re-measured 2026-08-16**,
  which replaced that wave's own figures: the tree holds **3933 files / 280
  directories / 12.2 MB** — `apply()`'s 4030/330 counts plan items, not
  entries ([ART-124](ISSUES.md)) — and carried through `run_with_fallback`,
  **all of it lands**, `hst-imager`'s own listing counting the same 280/3933
  with `españa.country`, `österreich.country` and `türkiye.country` there by
  name. So [ART-113](ISSUES.md)'s non-ASCII quarter is copied by the fallback
  rather than lost, and the earlier "3061 of 4030 / 969 excluded" numbers are
  superseded. **What the run cannot do is finish unattended**: the first
  `hst-imager` write into a volume `NativeFormatter` has just formatted dies
  `ERROR_DISK_FULL` ([ART-122](ISSUES.md), fixed the same day by making the
  tool a per-*partition* choice rather than a per-step one) — the full-tree
  figures above are from one unattended run through the fixed path.

- **G9 (ROM pairing) — done 2026-08-17.** Half was already done by
  `CardBuilder` (the ROM lands on FAT32 under the name the config points at).
  The remaining half — pairing a ROM with an *OS volume* — built as a check
  rather than an object: `core/rom/pairing.rs::compare` is pure, takes the
  tree's own planning record and the card's manifest entry, and asks only
  whether the tree's recipe requirement still holds. No stored profile, no
  picker; a named, reusable ROM+volume pairing belongs to G16 (multiboot),
  which is where "several ROMs and several volumes" actually lives, and
  inventing it here would have meant designing it twice.

**G10 is the last gap SD-2 owes, and its first wave landed 2026-08-17.**

- **G10 wave 1 — the game index — is built.** `core/gameindex/` reads a
  title's facts from whatever states them and carries the source beside each
  value; the Collection screen renders it and marks anything guessed. Design:
  [2026-08-17-g10-launcher-metadata-design.md](superpowers/specs/2026-08-17-g10-launcher-metadata-design.md).
  Plan: [2026-08-17-g10-game-index.md](superpowers/plans/2026-08-17-g10-game-index.md).
  Measured against the real library: 1698 titles, 1679 named by their own
  slave, 758 declaring a Kickstart image.
- **Wave 2 (`igame.data` beside each slave, drawer extraction out of a
  hardfile, `games.json`) and wave 3 (AGS export) are designed but not
  planned.** Wave 3's format can now be read: the MultibootOS card was
  extracted to `E:\amiga\multiboot\extracted\` and its second Amiga area holds
  `AGS0`…`AGS10` as PFS3, which `libpfs3` opens. The AGS menu format itself is
  already known from MrV2K's own distribution — `.ags` directories are menus,
  `.run` is an AmigaDOS launch script, `.txt` the description, `.iff` the
  artwork, `.rub` a hidden entry — but no *game* entry has been read yet, only
  the distribution's own scripts.
- **The user has asked for a larger Collection, in three rounds. A is done.**
  Design: [2026-08-17-catalogue-persistence-design.md](superpowers/specs/2026-08-17-catalogue-persistence-design.md).
  Plan: [2026-08-17-catalogue-persistence.md](superpowers/plans/2026-08-17-catalogue-persistence.md).
  What is left, in their order:
  1. ~~**The catalogue is not saved.**~~ **Done 2026-08-17 (wave A)** — one
     JSON per root in ART's own data directory, refreshed only when asked,
     with the user's corrections in a layer no refresh touches. Several
     folders came with it, since the storage shape had to know about them from
     the start. Watched on a real screen: a second Update reads nothing.
  2. **Missing facts fetched online, from sources configured in Settings.**
     The user's own framing, and it is also the right architecture: `core/`'s
     rule is that a request is *constructed* from a configured source, never
     from a caller-supplied URL, so ART would ship no data and no built-in
     endpoint. What is missing is real: 1536 of 1697 WHDLoad titles have no
     chipset signal from any local source.
  3. **Artwork.** Amiga Forever's grid shows the `screen-running` PNG embedded
     in each `.rp9` — which ART already reads into `record.preview` and does
     not render. LaunchBox shows box art from its own database. Candidate
     sources named by the user: [LaunchBox
     Games DB](https://gamesdb.launchbox-app.com/) (a daily `Metadata.zip`
     exists, **terms are not published anywhere and images are separate** —
     their forums say there is no public image access) and
     [libretro-thumbnails](https://thumbnails.libretro.com/), whose
     `/<System>/Named_Boxarts/<Game>.png` shape fits the mirror rule exactly
     and which has a `Commodore - Amiga` system. Neither is verified yet.
  4. ~~**More than one folder** in one catalogue.~~ **Done with A** — it fell
     out of one file per root, which is why the two were designed together.
  5. **A richer screen**, LaunchBox-shaped: art grid, detail panel, play
     button. Worth knowing before designing it: LaunchBox scanned this user's
     Amiga folder and listed `Asl`, `AskAssign` and `ASCIITable` — AmigaDOS
     commands — as games, with most covers blank. ART's index does not have
     that failure mode, because a file only becomes a title if it holds a
     valid `WHDLOADS` structure. **The gap is artwork, not facts.**

### What is blocked on the user, not on code

Three things, named as such rather than left to look like unstarted work:

1. **Driving the preload/G3 screen and the OS install screen in
   `pnpm tauri dev` against real material.** Both engines have been run for
   real, end to end, through Rust; neither screen has been watched rendering
   past a browser smoke test — and the OS install screen's own smoke test
   already found a wall a browser cannot get past: deeper interaction (filling
   the fields, ticking a component, running Plan or Verify) crashes the
   renderer reproducibly with an access violation in every headless Chrome and
   Edge combination tried ([ART-118](ISSUES.md)). This one needs the real
   window, not a substitute for it.
2. **A WinUAE boot of `dist-3.2`** (or the reduced PFS3 `.hdf` built from it).
   Nothing G5 built has been opened by an emulator yet, licensed or otherwise.
3. **A real A500 boot.** Nothing SD-1 or SD-2 has built has reached a card,
   and that rung waits on hardware still on a shopping list — a microSD card,
   a USB reader, an HDMI cable, the last plugged in *before* power or the VPU
   never configures the port.

None of the three needs a design decision or more code first; each needs
someone at the machine (or the Amiga) to run what already exists.

### What to pick up, once at the machine

- **G10 — the last of SD-2's five gaps, and the only one not started.** A
  design round opened on 2026-08-17, gathered context and stopped at its first
  question; **that context is written up in
  [sd-appliance-gap-analysis.md](sd-appliance-gap-analysis.md)'s own G10
  entry** rather than left to be rediscovered. The two measurements it made,
  because both change the gap's shape:

  1. **The user's collection holds no WHDLoad at all** — `E:\amiga\Titles` is 847
     `.adf` + 207 `.rp9` and not one `.slave`. **iGame is a WHDLoad launcher**,
     so a gameslist over this material would do nothing on a real Amiga; the
     gap text's own wording targets content this user does not yet own.
     **Superseded the same evening:** the user has a WHDLoad collection and is
     placing it under a folder for it. True of `Titles/` still; no longer true
     of what this project can reach. Measure the new folder before the design
     round resumes rather than carrying the count above forward — and note
     that it removes the strongest argument against reading the gap text
     literally, since iGame becomes testable against real material.
  2. **`.rp9` already carries curated metadata *and* a screenshot, offline** —
     title, publisher, year, genre, rating, required Kickstart, target machine,
     disk order and `rp9-preview.png`, in a `rp9-manifest.xml` inside each of
     207 packages. The gap text budgets an "optional online fetch, off by
     default" for exactly these fields; here no fetch is needed at all. ART
     does not read `.rp9` today — `core/layout::classify` sees a zip and sends
     it to `Unsorted/`.

  **The open question, undecided:** whether G10's first deliverable is a
  neutral per-game index that launcher exporters sit on, the literal
  iGame/WHDLoad export, or something aimed at the collection's actual shape.
  It is the user's call, and the round stopped there deliberately.
- **A card, on the PiStorm.** This is the rung that is genuinely untested,
  and it is worth naming precisely now that the filesystem side is settled.
  WinUAE ran Kickstart's own `expansion.library` and the real `pfs3aio`
  binary against a volume ART wrote, so *mounting* is proved by the same code
  a real Amiga runs — what is not proved is everything around it on a card:
  the MBR, an Amiga disk starting 1.1 GB in reached through Emu68's
  `brcm-sdhc.device` rather than a plain hardfile ([ART-095](ISSUES.md)'s
  shape from the other side), the FAT32 boot partition, and Emu68 starting at
  all. None of that is something an emulator can answer. Still missing: a
  microSD card, a USB reader and an HDMI cable (plugged in *before* power, or
  the VPU never configures the port).
- **The other screens, in `pnpm tauri dev`.** The engines are proven; the
  screens above them still have not been driven against real material
  ([ART-118](ISSUES.md)), and this session is the argument for doing it:
  three defects sat behind green suites until something outside ART looked at
  the result.
- **ART-115** — a `core::iso` test flake seen three times across the G5
  session, never diagnosed, and not reproduced by nine deliberate attempts
  since. Still open; the next sighting should save the panic output before
  re-running, which nobody has done yet.
- **Six issues G11 opened and did not all fix**: [ART-105 … ART-110](ISSUES.md).
  ART-106 is the one worth reading first — the plan never considers where a
  WHDLoad icon lands, so §82 can fail from the other side of the rule that
  exists to prevent it.
- **tolunnet / tolunwifi** — decided as ART Baseline's default network stack
  and WiFi suite, and **still not coded anywhere**. Belongs beside SD-2's
  package manifest, next to G5.

### Open issues

See [ISSUES.md](ISSUES.md) for the current list and severities rather than a
count copied here, which would go stale the moment either file is next
touched. Worth knowing without opening it: **[ART-118](ISSUES.md) is the
biggest one left** — the OS Builder's install screen has never been seen
rendering past its own headings, and this day's three defects are the argument
for looking. ([ART-104](ISSUES.md), which used to be named here, closed on
2026-08-16: the Kickstart table is generated from an independent database now,
and the machine check works against the user's own collection for the first
time.)

**Both of SD-1's open decisions are answered** (2026-08-13), and by hardware
rather than by preference — the user has an **A500 with a classic PiStorm on a
Raspberry Pi 3A+**, plus a Gotek. So:

- **Target board: the classic PiStorm, Pi 3A+.** Which is `PistormHardware::default()`
  already, and the Pi 3A+ is on the project's own supported list for that board
  rather than the reported-working one.
- **First image verified on: the A500.** An **A500+ arrives around 2026-08-28**,
  which makes a second witness possible — the same value the bare-metal ADF pass
  had, twice over.

Consequences that are now facts instead of parameters: the kernel archive is
`Emu68-pistorm.zip` on the stable line (**not** the same file as on 1.1 alpha —
ART-091), the SD driver is `brcm-sdhc.device`, and `genet.device` is irrelevant
(the 3A+ has no ethernet; its WiFi is `wifipi.device`).

**Materials: the software side is complete.** The collection at
`E:\amiga\Amigatolon` was inventoried on 2026-08-13 and **nothing copyrighted
is missing** — A1200 Kickstarts in both families (3.1 rev 40.68, 3.2's
`kicka1200.rom`, 3.2.1's `A1200.47.102.rom`), the full AmigaOS 3.2 ADF set plus
its CD and the 3.2.1/3.2.2 updates, the 3.9 ISO with both BoingBags, every
release from 1.0 to 3.1 as ADFs, and both real PiStorm distributions
(CaffeineOS 9317, MultibootOS 2.2) as images. WinUAE, 7-Zip and amitools are
installed.

**Everything on the software side arrived by 2026-08-14**, and the card was
built from it: `Emu68-pistorm.zip` (with the 1.1 alpha and `-raspi` ones, which
are *not* the card's), `VideoCore.card` — the name that needed checking, and it
is capitalised — `pfs3aio.lha`, Picasso96, `Emu68-WiFi.zip`, and
`hst.imager.exe` with its scripts. The full list, with what each one unblocks,
is `../eksik malzemeler.md` (Turkish, kept outside the repository since it is
the user's own shopping list).

**Only physical items are unaccounted for**: a microSD card, a USB reader and
an HDMI cable — and the card must be started with HDMI *already plugged in*, or
the VPU never configures the port and there is no RTG that session.

### What the user brings, and what it settles

Recorded 2026-08-15, because each of these changes a decision rather than
merely being nice to know.

- **Licensed Amiga Forever, desktop and mobile.** There is no Kickstart licence
  problem to design around, and the Cloanto-headered dumps are available —
  which `core/rom` already strips (`AMIROMTYPE1`). It is also why
  [ART-104](ISSUES.md) mattered: a licensed collection carries dumps whose
  checksums are not the ones a table copied from anywhere else will hold.
- **Four real Amigas.** Hardware verification is not limited to one witness.
  The rule stands unchanged — a claim records *which* machine proved it — but
  there is more than one machine to prove things on.
- **The user has written two Amiga-side programs, and both are meant to ship
  in ART-built distributions**: **tolunnet**, a TCP/IP stack to take Roadshow's
  place, and **tolunwifi**, a WiFi suite. Source at `D:\Projeler\tolunnet`.

  This last one is a real change to SD-2 and SD-3. SD-0 found that the
  community-standard package set is full of demo and conditionally
  redistributable software, and concluded that anything not clearly
  redistributable has to ship as a *fetch task* through the Aminet engine
  rather than as bundled bytes. A networking stack the user wrote themselves is
  theirs to distribute, so ART Baseline can carry one outright. And G14's WiFi
  pre-seeding changes shape: the configuration format belongs to their own
  program, so ART can write it directly instead of reverse-engineering
  somebody else's.

  **Not yet designed.** When SD-2 reaches its package manifest, the licence
  column has an easy row — and Roadshow should not be assumed as the default
  stack without asking.

### Where things are, for a fresh session

- `E:\amiga\Amigatolon` — the user's material. **Read from, never written to.**
- `E:\amiga\ProjeART` — where trial output goes. **Not C:, not D:** (the user
  said so on 2026-08-14), and not F: either, which turned out to be a 499 MB
  drive a 300 MB oracle image would not fit on. `ART_SCRATCH` overrides it in
  the scripts. A card ART built from the real release is sitting there as
  `card.img`. As of Task 14 (2026-08-16), `dist-3.2\` also holds the real
  4030-file/330-directory AmigaOS 3.2 tree `core/osinstall` built from the
  user's own ADFs — read from for the re-run named above, not rebuilt idly,
  since building it again costs the same real-media run.
- **Staged on 2026-08-15 for the screen run that has not happened yet**, both in
  `E:\amiga\ProjeART`: `screen-test.img`, a byte copy of `card.img` so the
  original survives being formatted, and `preload-tree\` holding `Readme` and
  `S\Startup-Sequence` for the copy step. That card's one Amiga partition is
  `SDH0`, FFS, 0.90 GiB, in MBR slot 2 — so a correct preload plan has **two**
  steps and no driver-embed step, because Kickstart carries FFS. If the plan
  shows three, something is wrong before anything is formatted.
- Rebuilding that card is the fastest way to see the whole engine work at once:

  ```bash
  cd src-tauri
  ART_CARD_ZIP="E:\amiga\Amigatolon\Emu68\Emu68-pistorm.zip" \
  ART_CARD_ROM="E:\amiga\Amigatolon\kickstart\Kickstart v3.1 rev 40.68 (1993)(Commodore)(A1200).rom" \
  ART_CARD_OUT="E:\amiga\ProjeART\card.img" \
    cargo test build_real_card_when_asked -- --nocapture
  ```

  It prints the payload it chose, where each partition landed, and reads the
  finished card back. Delete `card.img` first — `build_card` refuses to write
  over one that is already there.

## Session log

Newest first. One line per session that changed what works.

| Date | Change | Tests |
|---|---|---|
| 2026-08-19 | **The final whole-branch review's findings, fixed on `content-layer` before the merge.** The one a user meets first: ART offered both BoingBags as placeable and answered a confirmed selection with the ZIP reader's own raw English *"Password required to decrypt file"* — ART-166's record said the screen refused them and it did not. Both recipes now declare `host_placement_block: "encrypted-payload"` as **data**; the checklist refuses the tick, in both languages, naming what the package needs (*its own Amiga-side `Updater`*) rather than that ART failed; and `plan()` refuses by type before the folder is even scanned. **Every archive fixture on the branch was a ZIP while two of the three shipped recipes name a `.lha`** — which is exactly how [ART-168](ISSUES.md) passed 1875 tests and was found by a real run. `source_contract.rs` now asks its twelve questions of **five** sources (ADF, ISO, ZIP, LHA, 7z), `make_lha_with` learnt directory entries and raw name bytes, and ART-168's shape is pinned by a test that asserts the *wrong* answer on purpose and fails the day it is fixed. Two more `MediaSource` divergences, both latent, both found by a human reading files side by side — `entry()`'s returned casing and `walk`'s prefix filter — are settled (the media's own casing; folded containment) and are now contract questions, so the sixth fails a test instead. The "ModulesA1200 lesson, generalised" guard was hardcoded to five 3.2 component ids and so never saw `workbench-39` or either BoingBag — the exact shape it polices, shipped three times; it is now derived from every shipped recipe and package, and was falsified both ways before being trusted. And the record caught up with the code: §7's closing bar (a BoingBag'd tree booting) was **substituted** by the 3.9-overlay boot and now says so; §8's two unexercised hazards are filed as [ART-171](ISSUES.md)/[ART-172](ISSUES.md) — the second because *0 rows of every collision class* read like "no collisions" and was ART-168 comparing against a name nothing had written | 1885 Rust, 642 frontend, both green twice |
| 2026-08-19 | **The content layer closed, and the documents say what it does and what it will never do.** Nine tasks: `core/amigaver.rs` reads a file's own `$VER:` marker (**181 of the real tree's 588 files carry one, 31%**; **173 of 180 markers name their own file, 96%** — the rule that stops one program's numbers labelling another's file, adopted after an *older* file was made to render as an upgrade twice, by construction); `ArchiveSource` makes a package archive a third `MediaSource` and `open_nested` reads a payload out of its wrapper; `find_packages` discovers archives in their own folder (**27 of the owner's 58 items in 0.30 s**, a 171 MB `.rar` and a 248 MB `.7z` among them, nothing decompressed); three curated recipes ship as data, measured off the real archives; `collide.rs` sorts every overwrite into **five** classes with downgrades marked; Produce and Add give **byte-identical** trees; and a panel makes the set selectable with one confirmation for the whole set. **Two corrections are the round's real product.** The 3.9 tree recorded as green that morning was **AmigaOS 3.5** — the recipe placed the disc's `Workbench3.5` and never its `Workbench3.9` overlay, and the copyright line read as proof only proved the screen came from the tree; asking the running system its version instead gave `Workbench 44.5 (18-Aug-00)`, and after the fix `Kickstart 40.68, Workbench 45.1 (13-Nov-00)` ([ART-169](ISSUES.md)). And **external research found the ceiling**: every established distribution builder installs by running the package's own `Updater` inside an emulator, which is why they can install a BoingBag and ART cannot ([ART-166](ISSUES.md)) — the owner's decision is that **no password bypass will be written** and an Amiga-side install step becomes its own round. Boundaries stated rather than implied: two BoingBags unplaceable, the Turkish pack's catalogs unreachable ([ART-168](ISSUES.md)), eight archives claiming one identity ([ART-167](ISSUES.md)), Amiga Installer scripts refused permanently, and **inspect-and-propose deliberately not in this round** — three packages out of 58 items, said on screen. `FEATURES.md` carries the corrected 3.9 row and two new ones; `CHANGELOG.md` the user-visible half | 1875 Rust (twice), 637 frontend, exit 0 |
| 2026-08-19 | **Task 8 review round: the fix that made the 3.9 tree 3.9 could have been reverted silently, and now cannot.** `no_two_components_claim_one_destination_without_declaring_it` filters to `RuleKind::File` and every 3.9 rule is a `Subtree`, so it passed **vacuously** — it would have passed with every `overrides` array emptied, and reordering the components array would have reverted ART-169 with nothing catching it but a boot. Fixed two ways: that test now says in its own comment that it cannot see `Subtree` rules and which test does the protecting, and `the_39_overlay_is_declared_last_required_and_over_both_layers` pins that `workbench-39` exists, is **last**, is `required`, overrides **both** layers, and carries exactly the disc's thirteen `Workbench3.9` drawers — mutation-checked: dropping an override, reordering the array and deleting a rule each fail it, and the restored file passes. **The most interesting finding is F5**: of the layer's 21 unversioned overwrites, now all named by the hook rather than counted, one is `Libs/WORKBENCH.LIBRARY 193,400→199,852` — the very file `Version FULL` reads, the single change that turns `Workbench 44.5` into `45.1`, and one ART's classifier can say nothing about because the library carries no `$VER:`. **The boot proved the decisive change and the classifier could not**, which is this round's clearest argument for why a real run is not optional. Also: **ART-170** filed (`collide::preview` can only be asked about a *package* — `declared_override` resolves through `package::by_id` — so a release's own layering cannot be previewed); the `Flags`/`Providers` note corrected from both sides, with shipping them **accepted** as part of 3.9 rather than offered for narrowing; and `amigaos_39()`'s "one component today … waits for a real boot" comment retired, because the boot happened. `CHANGELOG.md` deliberately left to Task 9. Next boot check should read `Version >SYS:version.txt FULL` off the mount rather than interrupting `Startup-Sequence` — a healthy tree resists the interrupt by design | 1875 Rust passed, 0 failed, 20 ignored (run twice) |
| 2026-08-19 | **ART-169 fixed: the AmigaOS 3.9 tree is AmigaOS 3.9 at last.** The disc carries two sibling trees under `OS-Version3.9` and neither is a complete volume — `Workbench3.5` (673 rows, 16 drawers) and `Workbench3.9` (854 rows, 13 drawers, **no `L`, no `Expansion`, no `Rexxc`, no `T`, no `Startup-Sequence`, no `C/Version`**, and a `workbench.library` of 199,852 bytes against 3.5's 193,400). `Workbench3.9` is an **overlay**, which is why a 3.9 CD carries a `Workbench3.5` drawer at all, and `workbench-base` was never installing the wrong tree — it stopped after the first layer. Fixed by a second component, `workbench-39`, declared **last** (recipe order decides who writes last), `required: true`, `overrides: ["workbench-base", "locale-base"]`, with thirteen rules read off the disc's own listing rather than copied from `workbench-base`'s. **Measured** (`layer_the_real_39_overlay_when_asked`, release build): the tree goes from **1257 files / 156 drawers / 10,003,017 bytes** to **1879 / 181 / 18,813,726** — +622 files, +8.8 MB — and what the layer does to what was already there is **19 upgrades, 0 downgrades, 0 same-version, 21 unversioned, 130 identical (excluded by design) and 622 files landing where nothing existed**. The upgrades are the decisive ones: `C/IPrefs 44.23→45.9`, `Prefs/IControl 44.11→45.2`, `Prefs/Input`, `Prefs/Overscan`, `Prefs/Reaction` all 44.x→45.x, read out of the files' own `$VER:` strings by ART's own classifier. **The Locale question, measured rather than reasoned**: `Workbench3.9/Locale` and `OS-Version3.9/Locale` are *overlapping and complementary* — the overlay's `Catalogs` and `Languages` drawers are **empty** (locale-base is the only source of any catalog or `.language`), `Countries` shares 32 of 33, `Help` is disjoint (only `ViNCEd.guide`), and `Flags`/`Providers` are the overlay's alone — so neither component is redundant and `overrides` names both. **And the booted system says so**: under WinUAE with the owner's licensed Kickstart 3.1, `version full` now answers **`Kickstart 40.68, Workbench 45.1 (13-Nov-00)`** against `Workbench 44.5 (18-Aug-00)` before, `C:LoadMonDrvs: Unknown command` is gone from the boot console, and Workbench reaches a clean screen wearing the real 3.9 icons. ART-166, ART-167 and ART-168 deliberately untouched; the package run re-measured on the layered base is byte-for-byte identical. Full write-up: `.superpowers/sdd/2026-08-19-content-layer/task-8-report.md` | 1874 Rust passed, 0 failed, 20 ignored (run twice) |
| 2026-08-19 | **The content layer met the owner's own packages, and none of the three reached the tree.** A new env-gated hook (`core::osinstall::apply::tests::apply_the_real_packages_when_asked`) builds the base 3.9 tree with `locale-base` on — **1257 files, 156 drawers, 10,003,017 bytes, 10.10 s** on a release build — then previews and adds BoingBag 3.9-1, the Türkçe catalogs and BoingBag 3.9-2 in `order()`'s own sequence, against the owner's real 58-item package folder (`find_packages` identified 27 in 0.30 s, the 171 MB `.rar` and 248 MB `.7z` among them, nothing fatal). Four defects, all measured, none fixed here. **[ART-166](ISSUES.md) 🔴: both BoingBag payloads are password-encrypted ZIPs** — 233/233 and 147/147 entries ZipCrypto, confirmed by ART's reader, by 7-Zip 26.02 and by the raw local header's `0x0003` flag word; the password belongs to the BoingBag's own Amiga `Updater`, so neither recipe can place a single file and Task 4's counts were right only because ZipCrypto leaves the *listing* in clear. **[ART-167](ISSUES.md) 🟠:** eight archives claim `LocaleUpdate` and two claim `BoingBag3.9-2`, so `package_for` correctly refuses two of three packages and nothing in the product can pick between the candidates. **[ART-168](ISSUES.md) 🔴:** `core/lha`'s `entry_path` decodes a level-0/1 name with `from_utf8_lossy`, so `türkçe` became `t\u{FFFD}rk\u{FFFD}e` — the Türkçe pack's 36 catalogs landed *beside* the disc's `TÜRKÇE` and the preview reported **0 rows of every class**, a clean install by every number; the booted Amiga's `dir SYS:Locale/Catalogs` lists 20 drawers where the host holds 21, so all 36 are invisible to AmigaDOS. ART-155 again, in the other reader. **[ART-169](ISSUES.md) 🔴, found by the boot:** `workbench-base` places only the disc's `Workbench3.5` (673 entries) and never its `Workbench3.9` sibling (**854 entries**), so the tree ART calls 3.9 prints `C:LoadMonDrvs: Unknown command` before Workbench opens and `version full` answers `Kickstart 40.68, Workbench 44.5 (18-Aug-00)`. **The tree still boots clean** — two icons, no requester, `Amiga Workbench 2,013,904 graphics mem 7,907,696 other mem` — with the owner's licensed V40 ROM, through the config ART writes itself. **No recipe path was wrong**: every rule in all three package recipes resolved against the real archives, which is exactly why zero JSON changed and two packages are still unusable. Full write-up: `.superpowers/sdd/2026-08-19-content-layer/task-8-report.md` | 1874 Rust passed, 0 failed, 19 ignored (run twice) |
| 2026-08-19 | **The whole-branch review's three Majors closed, and the worst of them was on the screen rather than in the engine.** `OsInstall.tsx` rendered `AMIGAOS_32_COMPONENTS`, a hand-written copy of the AmigaOS 3.2 catalogue, **whatever release the new picker said** — so choosing "AmigaOS 3.9" planned from the 3.9 recipe while showing 26 components for a recipe that holds one, and labelled 3.9's `workbench-base` "Workbench3.2" (both recipes use that id; the label resolved against the wrong recipe and named a floppy volume with nothing to do with the disc). Nothing was ever written wrongly — the engine ignores an id its recipe does not hold — but showing one operating system's parts while installing another's is §89 on the screen itself. The hardcoded list is gone: `osinstall_components(release)` projects the chosen release's own `Recipe` (a fifth command, read-only, opens no media), and every helper that used to reach for a module constant now takes the loaded list, because a component id means nothing without knowing which release is being installed. **Component picks are remembered per release** (`rememberedComponentKey`) — switching to 3.9 and back finds the 3.2 selection untouched, and "AmigaOS 3.2" keeps its unsuffixed key so nobody's existing selection is forgotten by the upgrade. A test written for that caught a subtler ART-089: for one render after the picker changes, `release` is already the new one (so `chosen` reads the new key) while the loaded catalogue is still the old release's, and sanitizing one against the other wrote an empty list over a selection the user never touched — the catalogue now carries the release it describes and nothing sanitizes until the two agree. **The other two Majors were one function and one method.** `CdSource::walk` on a path naming a file answered `Ok(vec![])` where `AdfSource::walk` refuses — the **third** divergence between two implementations of one trait found on this branch, after `entry("")` and case folding — and `plan.rs`'s comment stated the refusing behaviour as fact. And `plan::relative_to` stripped a rule's `from` case-sensitively while resolution is case-insensitive, so on a **Joliet-pressed** disc (whose names are mixed-case where the recipe's are upper) the strip failed silently and the whole system tree landed nested under itself — `C/OS-Version3.9/Workbench3.5/C/List` — with no refusal. Before the case-insensitive resolution fix that disc refused loudly; after it, it built a wrong tree. Both fixed, both tested, and because three of these have now been found one at a time by a human reading two files side by side, `core/osinstall/source_contract.rs` asks **every** `MediaSource` the same ten questions over independently built media — a fourth divergence fails a test instead of waiting for a reviewer. Minors: the CHANGELOG's "a real, physical AmigaOS 3.9 CD" (it was an ISO file), "an AmigaOS 3.9 distribution" over a one-component ~6 MB base in CHANGELOG and STATUS alike (FEATURES.md said it honestly; these now match it), a workflow test comparing two `Option<usize>` so `None < Some(n)` passed while asserting nothing, a doc comment promising a refusal its body never performed, and the 3.9 recipe's unexplained missing `T` — recorded now with the reason **measured** rather than assumed: the disc's own Startup-Sequence runs `MakeDir RAM:T` / `Assign T: RAM:T`, and the disc's `T` drawer is empty. Three things tracked nowhere durable became issues: [ART-159](ISSUES.md) (spec §5's SetPatch/boot-sequence and language-variant hazards, predicted before the work and untouched after it — the likeliest reason a 3.9 tree fails to boot), [ART-160](ISSUES.md) (`apply()` still bypasses `windows_safe_name`, measured harmless on one Windows build and nowhere else), [ART-161](ISSUES.md) (a 469 MiB disc walked three to four times per install). **Still not met, unchanged: no 3.9 tree has booted** | 1758 Rust / 619 frontend |
| 2026-08-19 | **A second AmigaOS release, built from a real CD — and stopped one step short of the bar 3.2 met.** `core/osinstall/source_cd.rs::CdSource` is a second `MediaSource`, reading an ISO9660 disc through `core/iso` rather than an ADF; `scan::find_media`/`scan::identify` find one in a media folder alongside floppy images, planning and applying from it exactly as they do a floppy. `core/osinstall/recipes/amigaos-3.9.json` ships a first AmigaOS 3.9 recipe, one component (`workbench-base`), and a release picker (`recipe::by_release`, `InstallRequest.release`) makes it reachable from the OS Builder — an unknown release name is refused rather than defaulted, and the remembered default stays "AmigaOS 3.2" so nothing already set changed meaning the day a second recipe arrived. A disc dropped on the panel now offers the OS Builder too (`os.install-from-disc`, priority 20 behind `iso.browse`'s 10), handed the dropped file's parent folder (`src/lib/hostPath.ts::hostParentDir`) since that folder is where the sibling discs and ADFs live. **Run for real against the owner's own 469 MiB `AmigaOS39.iso`.** The recipe's 14 `from` paths were written in the mixed case every synthetic Joliet fixture uses; the real disc carries no Joliet descriptor at all, only a Primary tree, which ISO9660 keeps upper-case — corrected, and `plan()` resolves clean (0 refusals, 663 items, 6,108,319 planned bytes). `apply()` then failed twice on real engine code no synthetic test had reached: it opened every medium through `AdfSource::open` unconditionally, so it could not read a CD at all ([ART-153](ISSUES.md), fixed by extracting the floppy-or-disc identification `find_media` already did into a named `scan::identify`, used by both); and it hashed the whole 469 MB medium into memory just to record its SHA-256, against this project's own no-whole-file-read rule ([ART-154](ISSUES.md), fixed — `core/hashing.rs::sha256_file` streams). The re-run got measurably further — 1,020 files, 71 directories actually written — then failed on real disc content `apply()` could not write as literal Windows path segments. **Filed as ART-155 with a diagnosis that was half wrong, and the correction is on the record rather than smoothed over**: it first named two causes, a reserved DOS device name (`Storage/DOSDrivers/AUX`) and three accented `.country` filenames ART's own ISO9660 reader was rendering as `?`. Measured directly on this machine (Windows 11 Pro 26200): a file literally named `AUX` writes and reads back fine, by a plain path and a `\\?\` path alike — the reserved name was never what failed. The real cause was the `?`: `core/iso/descriptor.rs::decode_iso646` mapped every byte with the high bit set to `?`, and `?` is one of the characters Windows refuses in a path. The three real bytes (`0xD6`, `0xCB`, `0xD1`) measured off the disc are `Ö`, `Ë`, `Ñ` in ISO-8859-1 — AmigaDOS's own native charset, cross-checked against 7-Zip decoding the same disc's Rock Ridge names to the same bytes lower-cased. `decode_iso646` now decodes Latin-1 instead of substituting `?` ([ART-155](ISSUES.md) fixed). **The real tree now builds end to end**: 588 files, 75 directories, all 663 planned items, from the real disc, in about 20 seconds. One defect that same completed run found is filed, not fixed: `plan()`'s predicted `total_bytes` (6,108,319) overstates what `apply()` actually writes (6,054,225) by exactly 54,094 bytes — a CD-sourced directory's own ISO9660 extent length counted as if it were file content, invisible until ART-155 stopped hiding it ([ART-156](ISSUES.md), open). Two more findings from earlier in the run are recorded here rather than fixed, both design gaps rather than defects in what shipped: the recipe format can express a Kickstart *maximum* but not the *minimum* (V40+) AmigaOS 3.9 actually needs, so G9 has nothing to enforce for this release yet ([ART-157](ISSUES.md), open); and `CoreError::Malformed` now covers two different failure classes for a disc — genuinely corrupt, and merely larger than `CdSource`'s own walk limits — with no cost today, since nothing measured comes close to either limit ([ART-158](ISSUES.md), open). **The bar this did not meet, stated plainly: the tree has not booted.** 3.2's tree was proved by booting AmigaOS to a clean Workbench under WinUAE with the owner's own licensed ROM — the same bar the spec sets for 3.9 — and nothing in this run could meet it, because booting is a person driving an emulator, not a task in a plan. Also still owed: the release picker and the drop-panel path are covered by component tests only (`OsInstall.test.tsx`, the workflow catalogue tests, `hostPath.test.ts`), never driven by hand in `pnpm tauri dev` | 1742 Rust / 611 frontend |
| 2026-08-19 | **The OS install screen got its first automated coverage — `ART-118` narrowed, not closed.** `src/components/osbuilder/OsInstall.test.tsx` renders the real `OsInstall.tsx` in jsdom (not a proxy harness), mocked at the `@/lib/osinstall` / `@/lib/pistorm` / `@/lib/settings` boundary the rest of this suite already mocks at. Five tests: the screen mounts past its headings with the real media/ROM/destination fields, the 26-entry checklist and the Build/Verify actions all present — the thing no headless-Chrome session ever got past; the whole rendered tree carries no raw i18next key and no literal `{{…}}`, in **both English and Turkish** — the first language check against a running instance of this screen (`ART-062`); ticking a checklist component reaches the request `osinstallPlan` is asked to plan and changes what the plan section shows; and a refusal renders as the real translated sentence, not a blank card. Writing it surfaced a pre-existing gap in `src/i18n/literal-keys.test.ts`: its file scanner excluded `*.test.ts` but not `*.test.tsx`, so a `.tsx` test file with a real `t()` call under `src/pages` or `src/components` silently counted toward the "exact number of dynamic call sites" guard — fixed to exclude both. What this does **not** touch: jsdom does no layout, so the renderer's own access violation and whether long Turkish strings overflow are both still unreproduced — a real `pnpm tauri dev` pass remains owed | 1716 Rust / 599 frontend |
| 2026-08-18 | **The retry landed: a WHDLoad title from the user's own collection reached the game.** `1000 Miglia` (Simulmondo, one of 1697 self-booting WHDLoad hardfiles catalogued in `E:\amiga\Amigatolon\WHDload\HDF_Games_WHDLoad_by_Enzo_[#]\`) was launched from the Collection screen, and WinUAE showed Simulmondo's own title logo — this project's first WHDLoad game actually running, not just planned. The failing and the succeeding configuration, read off disk, differ in exactly one line: `fastmem_size=0` → `fastmem_size=8`, which is ART-151 proven directly. Three more of today's fixes are proven the same way, by what the working configuration itself says: `kickstart_rom_file=...Kickstart 3.1.rom` is ART-148's "highest major wins" rule choosing 3.1 over the folder's older 1.3; `hardfile2=rw,DH0:...,32,1,2,512,0,,uae` is ART-149's revert, restored exactly; and reaching WHDLoad at all with no system-volume prompt is ART-147's reclassification (`Media::WhdloadHardfile`) doing its job. **Two of the chain's fixes stayed honestly unexercised.** ART-146 (VHD/RDB geometry) never fired — `1000 Miglia` is a bare `DOS\1` hardfile, not a VHD, so it took the unchanged `Bare` branch. ART-145 (the Y2 startup-sequence's `Assign C:` fix) never fired either — a self-booting hardfile takes `RequestKind::Hardfile`, mounting and booting directly with none of ART's own generated `S/Startup-Sequence` in play; only the drawer-plus-system `RequestKind::Whdload` path ever reaches that script, and no title here took it. **What one title proves and what it does not.** 1697 records in this collection share this exact shape — a strong signal the rest boot too, not proof of each one. Still unrun: a bare `.adf` floppy set, an `.rp9`-packaged hardfile (102 titles), the Y1 mount-and-hand-over path, any VHD/RDB system image, and whether a save survives with `allow_write` turned on. `docs/ISSUES.md` and `docs/FEATURES.md` updated to match | 1716 Rust / 593 frontend |
| 2026-08-18 | **ART-149 corrected: the bare-hardfile geometry change (`32,1` → `1,1`) was itself wrong, and has been reverted.** WinUAE's `hardfile.cpp::getchs2` does floor `filesize / blocksize / (sectors × surfaces)` with integer division, so the old `32,1,2,512` geometry does round a non-cylinder-aligned file's presented block count down — that mechanism was real and stayed the basis of the earlier fix. What was wrong was the inference drawn from it: that the truncated blocks were lost data a different geometry could recover. They are not. This user's self-booting WHDLoad hardfiles were themselves **built** at 32-sectors/1-surface geometry, and the filesystem inside each is sized to the truncated whole-cylinder block count — an FFS root block sits at half the volume's block count, so its actual position (read out of the image) tells you what count the filesystem was built for. Measured across six real images from `E:\amiga\Amigatolon\WHDload\HDF_Games_WHDLoad_by_Enzo_[A]\`: `2 × root_block` equalled the file's block count truncated to a multiple of 32, six for six, never the raw count. Presenting the raw count instead (`sectors=1`) made AmigaDOS compute `1000 Miglia`'s root block at 1167 where the real one sits at 1152, so the volume stopped validating and WinUAE again reported "not a DOS disk" — reproducing the very symptom the change was meant to fix, on titles that had mounted fine before it. `sectors=32` is restored; the regression test (`bare_geometry_truncates_to_the_built_cylinder_count`, replacing the one that pinned `sectors=1`) and the doc comment above the fix both now carry the six-image table so the wrong inference is not rediscovered from the same, still-true, `getchs2` reading. **ART-148 (the WHDLoad Kickstart floor) is unaffected and remains correct** — it is a genuinely separate cause of the same "not a DOS disk" message on the same file, nothing about it changed. Still unretried: whether `1000 Miglia` and the rest of this collection now actually boot in WinUAE | 1709 Rust / 587 frontend |
| 2026-08-18 | **ART-147: the catalogue called 1698 of this user's self-booting WHDLoad hardfiles an unpacked drawer, and fixing it briefly took the whole Collection screen down.** `Media::WhdloadDrawer { slave }` said every one of these files needed a separate bootable system — the shape ART-146 left `1000 Miglia` unretried against — when they are `DOS\1` hardfiles that boot themselves, `S/startup-sequence` and all; the user picked a bare Workbench 3.0 as "the system" and landed at a CLI, reasonably concluding WHDLoad needed installing. Renamed to `Media::WhdloadHardfile { file, slave }` (the sole producer of the old variant, checked and confirmed), routed onto the same plain-hardfile launch path — mount and boot, no system, no boot directory, no Y1/Y2 — with a new off-by-default `allow_write` switch for the one real conflict this surfaced: WHDLoad writes saves back into the exact image spec §93 says must default to read-only. `GAMEINDEX_SCHEMA` 2 → 3. **Landing that rename broke every catalogue already on disk**: `store::read_root`'s parse of the old shape failed outright, and `store::load` turned that into a hard error the Collection screen showed as `ART-FORMAT-MALFORMED` instead of any title — found immediately by running the app against the fix. Fixed on the distinction that a root catalogue file is derived (rebuildable by one Update) while the overrides file is user data (not): `read_root` now returns `Absent`/`Unreadable`/`Found` instead of erroring on a parse failure, folding `Unreadable` into the existing `stale` signal so the screen's own "an update would improve this" badge already covers it with no frontend change; `refresh_root` treats `Unreadable` like `Absent` so pressing Update actually rebuilds the file rather than failing on it; `read_overrides` was deliberately left exactly as strict. Neither half has been retried against the real application yet | 1702 Rust / 587 frontend |
| 2026-08-18 | **Play's first real run — WinUAE, a real title (`1000 Miglia`), Y2 — found a defect and fixed it; whether the game itself now starts is still open.** ART generated `Assign C: DH0:C` as the first line of the boot directory's startup-sequence, but that boot directory has no `C` drawer: AmigaOS auto-assigns `C:` only when one exists on the boot volume, and `Assign` is itself a command that lives in `C:`. The script could not run its own first line, and the user landed at an AmigaDOS CLI instead of the game ([ART-145](ISSUES.md), found and fixed same day). Fixed by invoking that first `Assign` through an explicit path on the mounted system volume (`DH0:C/Assign C: DH0:C`), and — reasoned rather than confirmed by the run — adding `SYS:` and `S:` beside the existing `LIBS:`/`DEVS:` assigns, since WHDLoad reads `S:WHDLoad.prefs` and ART's own `S` drawer would otherwise have shadowed it. `core/launch/whdload_boot.rs`'s two script tests now pin the complete corrected text. **Not yet re-run**: the fixed script has not gone back through WinUAE against the real AmiKit image, so whether `1000 Miglia` now starts is unverified — Play is not being claimed as working end to end. | 1682 Rust / 587 frontend |
| 2026-08-18 | **Collection wave C lands — pictures already on disk, a detail panel, hand-attached art and a picture-kind switch, and Play — built and unit-tested, not one byte of it run against real material.** `core/artwork/local.rs` extracts an `.rp9`'s own embedded `screen-running` PNG straight from the package with no network and no consent question, adopting rather than re-extracting on a second pass and refusing a hostile `../../evil.png` entry — and, after a Task 6 review caught it, storing under the same normalised key `artwork_known` reads rather than the raw catalogue title, which had silently sent every one of the 242 real pictures to a key nothing ever looked up. A hand-attached picture (PNG/JPEG) binds in `RecordOverride`'s user layer (`ArtBinding { chosen, cached }`) so a refresh cannot lose it and a later online fetch skips a title that already has one — though deleting the cache does not yet rebuild it from `chosen` ([ART-143](ISSUES.md)). The detail panel (`CollectionStudio.tsx`, `TitleDetail.tsx`) opens from either the grid or the table and shows the disk order, the slave, the declared Kickstart, the path, and a switch between the kinds of picture a title has. `core/launch/` plans a launch — a machine profile from the catalogue's chipset, a Kickstart from the ROM folder, refusing outright when none suits — unpacks an `.rp9`'s floppies or hardfile into ART's own data directory, and builds the Y2 one-click boot directory for WHDLoad, never removing Y1 (mount the user's own system read-only, hand over) from the panel; `core/winuae.rs` grew `DirMount` so a host folder mounts as an Amiga volume. Two Critical findings surfaced and were fixed in review before any of it landed: an `.rp9`-packaged HARDFILE title was mounting the zip archive itself, writable ([ART-141](ISSUES.md), fixed same day), and a WHDLoad slave name was refused for quoting and redirection characters but not `;`, `` ` `` or `$`, all of which the AmigaShell command-line parser accepts — refused now, and the doc comment cites the source rather than hedging: the AmigaOS Manual's *AmigaDOS Using Scripts* chapter states that `$` "introduces an environment variable (which also works outside of a script)" and that "[b]ack apostrophes are used to execute commands from within a string", so the refusal is evidenced rather than precautionary — with the one reservation that survives the check written down beside it, that the source describes backtick substitution *inside a string* while ART passes the slave unquoted, which that source does not settle. **Two other claims this wave had been carrying on inference were taken to their primary sources at close-out.** `filesystem2=<access>,<device>:<volume>:<path>,<bootpri>` and `hardfile2=<access>,<device>:<path>,<sectors>,<surfaces>,<reserved>,<blocksize>,<bootpri>,<handler>` are exactly the field orders ART emits (e-uae's `docs/configuration.txt`, the syntax WinUAE inherits) — documented-correct, no longer inferred. What is **still** inference, and is recorded as such rather than promoted: that a *higher* `bootpri` boots first. The syntax documentation does not state the direction and WinUAE's own help page does not mention `bootpri` at all; it rests on the Amiga RDB `BootPri` convention `core/rdb.rs` already implements, and the AmiKit run is what settles it. Whether a comma in a mounted path could be *quoted* instead of refused is unsettled the same way, so [ART-142](ISSUES.md)'s refusal stands as the evidenced-conservative choice rather than a guess. **Per this project's own rule, no `FEATURES.md` row is flipped to done on the strength of unit tests alone**: Play is unit-tested and has never reached a real emulator; the offline picture pass is unit-tested and has never been pointed at the user's real 242 `.rp9` folder. Two new issues filed rather than fixed — [ART-142](ISSUES.md) (a comma in a mounted folder's path shifts every field after it in the generated WinUAE configuration; pre-existing in the `hardfile2=` loop, now far likelier to fire with directory mounts) and [ART-143](ISSUES.md) above — plus [ART-144](ISSUES.md) folding five deferred review minors (Tasks 3, 6, 6b, 8) into one entry. **A process error, recorded rather than smoothed over**: this session ran Task 8's implementer while Task 7's fix round was still editing `core/launch/mod.rs` in the same checkout — the skill this project follows forbids parallel implementation dispatches for exactly this reason, and the user's own memory records the identical hazard. Commit `020c332` (Task 8) swept Task 7's two uncommitted tests in without saying so in its message; nothing was lost and both pieces of work were reviewed separately, but the message does not say what it carries, and the fix is not to rewrite history to hide that — it is to say so here. Real-material verification is now a checklist for the user, not a claim in this file: the 242 `.rp9` pictures, a floppy title, and a WHDLoad title against `E:\amiga\amikit\AmiKit.hdf` (`CHANGELOG.md` carries the exact steps) | 1682 Rust / 587 frontend |
| 2026-08-18 | **The palette stopped being a preference and became a measurement.** [ART-140](ISSUES.md): the light theme rendered its own success badge at **2.20:1** — `.badge-ok` sets `color: var(--ok)` on a background that is 22% of `var(--ok)`, so the text and its ground were the same colour 22% apart. Measured against WCAG's 4.5:1, the light theme failed in 23 places and the dark theme in 9: every status badge in both, `--text-faint` (which carries file paths) at 2.85 and 2.58, white on the primary button at 3.94, and `--border-strong` — the edge that says an input is an input — at 1.95. No single value fixes a badge, because darkening the colour takes its own tint down with it; solving both at once yields `#614710` for a warning, which is mud. **Each meaning now has two tokens**: `--ok`/`--warn`/`--err`/`--accent` remain the mark (fills, borders, `accent-color`, the tint), and `--ok-text`/`--warn-text`/`--err-text`/`--accent-text` are the same hue moved until they clear 4.5:1 against all four surfaces *and* their own tint on each. The dark theme's identity colours are unchanged. Two hard-coded-colour bugs fell out of the same sweep: `ErrorBoundary` carried the dark theme's hexes, so the **crash screen** was 2.3:1 on a light page, and the Hex viewer put `var(--text-muted)` inside a hard-coded `#0d1117` panel. **And the judgement is now a script**: `scripts/contrast-check.py` reads `theme.css`, computes all 90 pairs ART renders and exits non-zero below threshold — pure Python, no browser, so it **runs in CI** where `zoom-check.py` cannot. All 90 pass; both themes screenshotted in a real browser. | 1618 Rust · 570 frontend · 90 contrast pairs |
| 2026-08-18 | **Two labels that claimed more than ART knew, and a theme that stopped at the screen's edge.** [ART-138](ISSUES.md): the ROM library called 46 of the 76 files in the user's own ROM folder `CRC ERR` — accelerator, SCSI and PPC ROMs, every one of them intact. The field behind the badge was a `bool`, so "this Kickstart's checksum does not verify" and "there is no Kickstart checksum here" were the same answer. It is `RomChecksum::{Valid, Invalid, NotChecked}` now, and telling a Kickstart image from a file that merely sits in the same folder is done on **two structural marks Commodore's build leaves** — the opening `$11xx 4EF9` and the tail `00 1C 00 1D 00 1E 00 1F` — chosen by measuring ~150 real files (this project's 76, the AmigaOS 3.2/3.2.1/3.2.2 releases, an Amiga Forever export; Kickstart 0.7 through 47.111) and finding the two marks and a verifying checksum agree **exactly**. Neither mark is touched by damage to the code between them, so a Kickstart with a flipped bit still reports `CRC ERR` — the one case where that badge is a fact. Re-measured through `identify_rom`: `valid=30 invalid=0 not-checked=46`. The size-based name was the same claim from the other side (a 256 KB accelerator ROM was *Generic Amiga 256KB ROM (Kickstart 1.x)*) and now reads `Not a Kickstart image (256 KB)`; the blank `Compatible Amiga Models:` under the CDTV Extended 2.30 — correct data, the Remus database names no machine for it — says so in words. [ART-139](ISSUES.md): Aminet's inputs were bare elements taking the browser's white, fixed as a theme rule for `input`/`select`/`textarea` in `global.css` at zero specificity (`:where()`), so every screen that already styled its own controls is untouched. Both themes checked in a real browser. **Neither screen has been driven by the user yet — the ROM badges in particular are a claim about a screen, and [this project's own rule](ISSUES.md) is that those get confirmed by looking.** | 1618 Rust · 570 frontend |
| 2026-08-18 | **ART-137: 99 of 758 records were asking for a Kickstart whose name was 68000 machine code, and the cause was a field that is sometimes a list.** Photographing the Collection for the README showed two cards reading `Needs Kickstart ÔöÇÔûêÔûêÔûêÔûê` beside neighbours reading `34005.a500`. The bytes were not a mangled string but code, repeating across every affected title with three bytes changing. **`ws_kickname` is not always a string**: when `ws_kickcrc` is `$ffff` — a marker, not a checksum — it points at `(crc16, rptr-to-name)` entries ended by a zero word, with the names laid out immediately after. ART read the list as a string and reported what it collected. The layout was **decoded from the bytes rather than looked up**, because whdload.de could not be fetched that session, and it is trusted for three independent reasons: two real slaves carry the same three CRCs; every entry's pointer lands exactly on a name; each name ends one byte before the next entry's pointer. It then yields names that exist — `40068.a1200`, `40068.a4000`, `40063.a600`, which is precisely what a game running on an A1200, an A4000 or an A600 asks for. Verified on the real catalogue after an Update: **99 records carry a list, 659 a single name, zero carry an unprintable one, and zero still record `$ffff` as a checksum** — and the lists themselves are coherent, all 99 accepting both the A1200 and A4000 ROMs while 85 also accept the A600's older 3.0. `crc16` is left `None` rather than storing the sentinel, because a marker kept as a checksum is the same class of lie as the garbage name it arrived with. `GAMEINDEX_SCHEMA` moved 1 → 2, which is exactly what that number was separated from the file format's version for: the Update re-read 1698 records without the user having to know why. Not cosmetic — [ART-130](ISSUES.md) is meant to take a declared Kickstart, find it in the 154-dump table and offer to place it, and this is that work's input. Two more defects were filed from the same photography session and deliberately kept out of the README: [ART-138](ISSUES.md) (ROM Manager labels accelerator and SCSI ROMs `CRC ERR`, claiming a damage it cannot know about) and [ART-139](ISSUES.md) (Aminet's inputs render white in the dark theme) | 1614 Rust / 569 frontend |
| 2026-08-18 | **The README stops describing a product two months behind itself, and the photography finds three more defects.** Five screenshots of ART's own window — dark, English, on the real 2787-title library — captured with `PrintWindow` so the window draws itself rather than being photographed off the screen, which is what keeps a desktop out of a public repository. The hero is a Workbench 1.3 Extras disk beside a Windows drive: 1988 dates, OFS, `----rwed` protection bits, one frame saying what the paragraph under it did not. **Three claims had gone stale and one of them was a licence claim** — "PFS3 is not written yet" (it has been since `libpfs3`), "not yet built: PFS3/SFS" (only SFS now), and "ART's dependencies are permissively licensed", which stopped being true the moment `libpfs3` arrived as LGPL-3.0-or-later. The architecture tree had never heard of `card`, `osinstall`, `preload`, `gameindex`, `artwork`, `security` or `safety`. Three screens are deliberately absent and `docs/assets/README.md` names each so the absence does not read as taste. **The light theme was widened while looking at it**: `#e9ecf1` to `#fafbfc` is seventeen steps, the same distance the dark theme uses to obvious effect, but contrast is judged against the surrounding luminance and seventeen steps near white reads as nothing — the file manager arrived as one flat sheet. Widened, not finished; it wants a proper pass and is not in the pictures | 1609 Rust / 569 frontend |
| 2026-08-18 | **A second real folder made a hidden assumption visible: ART's ADF path parses TOSEC filenames, and none of the 847 real ADFs are TOSEC** ([ART-136](ISSUES.md)). The user added `E:\amiga\Titles` — 847 `.adf` + 242 `.rp9`, no WHDLoad at all — and the shape was nothing like the folder beside it. The `.rp9`s came out *richer* than anything WHDLoad can state (`rating` 242 of 242, `preview` 242, `genre` 207 — all three are 0 % in the WHDLoad catalogue), while the ADFs came out poorer than expected and for a reason nobody had looked for: the files are hand-named (`A-Train Disk 1.adf`, `ADPro_D3.adf`, `dune2-2.adf`) and **zero of 847 match the TOSEC convention the reader assumes**. So one game becomes five entries, artwork matches 3 % against the WHDLoad folder's 60 %, and the parser does not merely fail but mangles — `(c) 1990 Svein Berge.adf` arrived as `1990 Svein Berge`, a parenthesised group stripped as though it were a TOSEC field, with provenance then claiming `tosec-name`. **The fix is a tool, not a guess, by the owner's explicit call**: `core/gameindex/cleanup.rs` proposes and the user accepts. A first version refused bare trailing numbers outright — `4D Driving 1` is a disk and `Turrican 2` is a sequel and one name cannot tell you which — and the user's correction was the right one: multi-disk is the Amiga norm and `dune2-2` should base on `dune2`. It can, but only by reading **every name at once**: `disk_sets` accepts a group when it begins at disk one, which 163 of the 174 numbered groups do. That rule carries both directions — it groups `apoc1/2/3`, and it leaves `Turrican 2` beside `Turrican 3` alone *and* `LSD_042 … LSD_064`, eighteen issues of a disk magazine that share a base and are numbered and would have collapsed into one title. What no rule can settle (`brian the lion 2`, no disk one anywhere) is typed by hand — the override editing UI FEATURES had been deferring to wave C landed with it. Renaming a real file is a separate button: confirms with both names in full, refuses an existing target rather than replacing it, logged either way, five tests including a mutation-checked path-traversal guard. **847 files now resolve to 523 titles, 606 with a suggestion**; verified on the user's own disk, rename included, with the catalogue following the moved file by its content id. One test wrote itself wrong on the way and is recorded as such: it demanded the magazine issues get *no* suggestion when the property that matters is that eighteen issues stay eighteen titles | 1609 Rust / 569 frontend |
| 2026-08-17 | **Wave B on a real screen: two defects in the first run, both mine, both about a run that does not finish.** [ART-134](ISSUES.md) — the user said the downloaded artwork was not showing. It had downloaded: **790 pictures were on disk**. What was missing was `index.json`, because `cache.save()` ran once, after the last of 1700 titles. The run was interrupted, so nothing knew those files existed; the screen reads the index and saw an empty cache, and the next run started downloading all of them again. Fixed twice over — the index is now written unconditionally on the way out of `enrich` (cancelled, failed or finished) and every three seconds during the run, **timed rather than counted** because the screen reads the same file to show pictures as they land; and `Cache::adopt` takes a picture already on disk into the index without fetching it, which is what rescued the 790 rather than re-downloading them. Every existing test ran a handful of titles **to completion**, which is why none of them caught it. Both fixes mutation-checked, and the limit stated rather than glossed: the real failure was the **process ending**, which no in-process test reproduces — the cancellation path already saved, so only the adoption test covers what actually happened. Then [ART-135](ISSUES.md), from the user asking whether waiting an hour for cover art was right. It was not. `REQUESTS_PER_SECOND` was one constant chosen for **whdload.de**, which volunteers run on a small server, and applied unchanged to **libretro's pictures, which come off GitHub's CDN** — holding a CDN to a volunteer's pace is not politeness, it is a mistake with a courteous name. Worse, libretro publishes four kinds per title and the run fetched all four while the Collection renders one, so three-quarters of the wait bought pictures nothing displays. The rate is stated per source under a ceiling (whdload.de 4, libretro 16) and `EnrichRequest::wanted` carries what the caller will render. **About forty minutes to about one, measured against the user's own 1700 titles**, confirmed on screen. Also on screen: pictures now appear as they arrive rather than at the end, and the filter bar is stuck to the top — a filter you must scroll back up to reach in a 1700-row list is a filter nobody uses. Match rate against the real library, measured mid-run: **230 of 383 titles, ~60 %** | 1585 Rust / 569 frontend |
| 2026-08-17 | **Wave B: measuring the catalogue before designing it turned an "artwork and chipset" round into an artwork round, and killed an attractive shortcut with 83 counterexamples.** The user asked for online enrichment — chipset badges and cover art. Measuring the real 1700-title catalogue first showed the metadata people assume needs fetching is already there and arrived with no network at all: `title` 100 %, `publisher` 98.8 %, `year` 92.5 %, all out of the WHDLoad slave's own header. What is genuinely empty is `preview` (0 %), `genre` (0 %), `rating` (0 %) and `chipset` (9.6 %). **The chipset shortcut was tested rather than argued.** `ws_Flags` bit 5 is read for every slave, so a *clear* bit read as "runs on OCS/ECS" would have taken it to ~99 % offline — and `chipset_of`'s existing comment said that was wrong. It is: 83 records have a slave that left the flag clear while the title says AGA or CD32 (`Ace Ball v1.0 Pl AGA.hdf`, `Akira v1.3 CD32.hdf`), and those are only the ones whose *filename* gives them away. 9.6 % is the correct answer, not a gap. **The source hunt closed the metadata half for a reason that is not ART's**: Lemon Amiga returns 403 to every non-browser request including `robots.txt` — reaching it needs a forged user agent to defeat an access control, so it is out; Hall of Light is HTML only and ART does not scrape (§41.5.3); OpenRetro holds exactly the chipset data and its `/about` expects third-party apps, but documents no endpoint. The user's position is recorded as project policy: **an absent licence is not a blocker for forty-year-old game and demo material, an absent endpoint is.** So `core/artwork/` fetches pictures: libretro-thumbnails through its git-tree index, whdload.de's icons at a path built from the package name ART already read from the slave — an exact key, no matching at all. **ART's own path validator shaped the design twice**: it rejects `:` (a colon could re-point a request at another host), so neither `?recursive=1` nor `trees/master:Named_Boxarts` is expressible and the index takes two colon-free calls (root tree → sha → subtree, verified live: 3324 boxarts, untruncated); and it rejects spaces, which every libretro filename has, so segments are percent-encoded in `core/artwork` rather than the validator being weakened. Matching is two written rules and no similarity measure — whole title, then the part before a ` - ` — because online data is provenance rank 2 and a silently accepted wrong guess would make that ranking meaningless. **Writing the tests found a defect the design had not anticipated**: a source offers four kinds, and when only one had an index the other three were being recorded as *misses* — but a miss means "no picture exists" and is never re-asked, while the truth was "nobody looked", so a directory the repository added later would have stayed invisible forever. Sources ship enabled, per the user's call; **enabled is not automatic** — opening the Collection reads the cache and touches no network, and the run is capped at four requests a second because whdload.de is run by volunteers | 1579 Rust / 569 frontend |
| 2026-08-17 | **The Collection stops re-reading 3.74 GB, and driving it found a third defect older than the work.** Wave A of the user's larger Collection: `core/gameindex/store.rs` keeps a catalogue between runs — one JSON per scanned root, a roots list, and the user's own corrections in a file of their own. An Update reuses a cached entry when its path, size and mtime match **and** its record was read by the current reader; a Rescan trusts nothing still on disk; neither deletes an entry whose file has gone. The two carrying tests were mutation-checked in both directions: a planted `SENTINEL` record proves the file is never opened on a cache hit, and lowering a cached record's schema by one proves a reader fix like [ART-131](ISSUES.md) lands without the user knowing to ask. `Provenance` gained `UserEdit` and a `rank()` — a bool cannot order four tiers — with **tier 2 left empty for wave B**, so a third-party database adds a variant rather than renumbering. **The ten-thousand-game question the user asked is answered by a test, not an argument**: in a *debug* build, 10 000 entries are 5.8 MB on disk, written in 253 ms and loaded in 326 ms including 10 000 `metadata()` calls — so SQLite would be complexity without payoff, and `store`'s interface names no format if that ever changes. **Driven in `pnpm tauri dev` against the real collection**, which is where the claims were actually checked: opening scans nothing and the folder survived closing and reopening the application (persistence end to end, which no test here could show — they all run in one process in a tempdir), a second Update finishes instantly, and Remove asks. That last one is [ART-133](ISSUES.md), found in the first minute: **`window.confirm` returns without asking in this application**, so thirteen confirmations never fired — four of them in front of a delete, including `deleteEntry`'s double confirmation whose own comment explains why two were needed. All thirteen now use the dialog plugin's real `confirm`; the four `window.prompt` sites stay open on purpose, because the evidence is one observation and those screens have been driven before. Then renaming a file on screen found the last one: two entries sharing a content-derived id, and the screen showing the missing one — **a file whose id turns up at a new path moved, it did not vanish**, so the old path is dropped while a title that moved nowhere keeps its entry | 1519 Rust / 567 frontend |
| 2026-08-17 | **G10's wave 1: the Collection asks the game what it is called, and two older defects fell out of doing it.** `core/gameindex/` reads a title from whatever *states* it — a WHDLoad slave's own `ws_name`/`ws_copy`/`ws_Flags` header (format from whdload.de's autodoc plus `whdload.i`'s flag bits, implemented independently, neither file copied), a Cloanto `.rp9` manifest through `quick-xml`, a bootable hardfile's insides, and a TOSEC filename last. Every value carries `Fact<T>`'s provenance, and **the screen marks anything guessed** — because a slave declaring `ReqAGA` and a filename merely containing the letters "AGA" are not the same claim, and `Agassi Tennis` is the case that proves it. Run against the real library: **1698 titles, 1679 named by their own slave**, 18 by a drawer name (slaves older than v10 have no `ws_name`), 2 by filename; 1678 publishers and 1570 years out of `ws_copy`'s documented "year then holder" shape; **758 declaring a Kickstart image by name**, which is [ART-130](ISSUES.md)'s material sitting in the index already. The real material changed the design twice, both times the same mistake — treating a statement as silence: `TitleKind` had only `Game` and `Demo` while the 242 real `.rp9`s are 111 demo / 96 game / 15 system / 10 gallery / 10 video, and `.rp9`'s `<genre>` turns out to depend on `<type>`, so `history` (only on gallery/video) and `original`/`enhanced`/`prototype`/`third-party` (only on system) stay unmapped **on purpose**, with a test saying so. **Driving the screen for the first time was worth what it cost:** [ART-131](ISSUES.md) — a bare hardfile records its extent nowhere, so `mount()` placed the root block from the file's length; partitions are whole cylinders and files are not, so **1456 of 1697 images would not open, in the Collection, ADF Studio or the file manager alike** (241 vs 1697 measured across the whole collection) — and [ART-132](ISSUES.md), three things wrong in the first minute on screen: a 29 GB card image hashed before anything asked whether it was a title, two scans running side by side, and a bar rounding 99.6% to 100%. `core/collection.rs` and its command retired onto this, with its depth-limit test moved rather than deleted | 1489 Rust / 560 frontend |
| 2026-08-17 | **G9 merges to `main`, CI green on the first push, and a docs sweep clears out what the day made stale.** The `g9-rom-pairing` branch went up and all nineteen CI steps passed at the first attempt — worth noting because the G5 merge two days earlier went red twice on `clippy::question_mark`, a lint invisible on this machine (CI `stable`/clippy 1.97.0, this machine pinned at 0.1.95). Seven commits that had been sitting unpushed on `main` (ART-104, ART-128 and G9's own spec and plan) were already ancestors of the branch, so that CI run compiled them too; the merge is `--no-ff` so the phase keeps a node, and `git diff main g9-rom-pairing` is empty — what is on `main` is byte-for-byte what CI validated. **The sweep then deleted claims that had rotted during the day rather than over weeks**: STATUS.md's Snapshot said 1460 i18n leaf keys against FEATURES.md's 1469 (1469 is right — two rows of one file disagreeing is worse than either being old); its "Picking up next session" section still listed G9 as owed and unstarted, told the next session it "wants a design pass", and quoted the previous session's 1411/533 as fresh. The gap analysis still had G5 as 🟧 *"not yet booted"* — it booted to a clean Workbench the day before — and G9 as 🟧 with its original `rom_profile` framing, which the design round had deliberately rejected in favour of a check with no stored object (that idea belongs to G16). One FEATURES.md sentence had run two clauses together at the comma and no longer parsed. A test's doc comment stated a false premise and corrected it two paragraphs later, so a partial read came away with the wrong idea. **G10's design round opened and stopped at its first question, by the user's call**, but not before measuring two things now written into the gap analysis: the user's 954-title collection holds no WHDLoad at all (847 `.adf` + 207 `.rp9`, zero `.slave`) while **iGame is a WHDLoad launcher**, and `.rp9` already carries curated metadata *and* a screenshot offline — title, publisher, year, genre, rating, required Kickstart, disk order, `rp9-preview.png` — which is the field set the gap text budgets an optional online fetch for | 1439 Rust / 554 frontend |
| 2026-08-17 | **G9's final review, all ten findings fixed in one wave — two of them blockers, both the same failure ([ART-129](ISSUES.md)): the check that exists to warn said nothing.** `compare` returned `Paired` on a matching hash *before* the tree's own requirement was evaluated, and a tree can carry both at once — plan AmigaOS 3.2 against a real Kickstart 40.68 with `modules-a1200` excluded (supported, with a shipped test of its own) and `plan()` records `{ statedMajor: 40, requiresMajor: 47 }`; build the card with that same ROM and identity answered a question it was never asked. The requirement is asked first now. Separately, the screen asked about the **first** filled partition only, while the plan emits a `copy-in` per partition — a paired DH0 swallowed DH1's real `Unsuitable` entirely, and a staging folder on DH0 made ART talk about the one folder not at risk. It asks about every chosen folder now and renders one line each, named by drive. Both tests were written first and watched to fail. Three more silences closed with them: a verdict in flight and a rejecting command both rendered as the reassuring nothing (`check-failed` and a muted "checking…" line, checkbox never disabled — this warns, it never blocks), the `unsuitable` sentence hard-coded V47 beside its own interpolated `{{needs}}` (split, so the message observed on a real screen is quoted only where it is true), and `notChecked.card` asserted a cause it had not established. Structurally: `core/rom` no longer imports `core::osinstall::PairedRom` — a local `TreeRom` carries the two facts the comparison reads and `commands/preload.rs` maps between them, so the lower-level module can be read, and extracted, on its own; and `distribution.json` is deserialised through a narrow struct rather than 3950 `FileRecord`s (1,052,629 bytes on the real `dist-3.2b`) to reach its last field, with the effect's deps narrowed to the card and the folders so typing a volume name no longer re-reads it. **The design's third proof case reaches real material at last**: the hook builds a *second* card from the user's real Kickstart 47 and asks the V40 tree about it — `card A stated_major=Some(40)` / `card B stated_major=Some(47)`; `V47 tree vs V40 card: Unsuitable { needs: 47, found: Some(40), rom: "kick.rom" }`, `V40 tree vs V40 card: Paired`, `V40 tree vs V47 card: Suitable { rom: "kick.rom" }` — the last being the only evidence on real material that the check reads the tree's capability rather than comparing numbers, since 40 is not ≥ 47. Both cards deleted on the way out. One correction found while fixing: dropping `#[serde(default)]` from an `Option<T>` field cannot break a manifest read, because serde's derive already treats a missing `Option` as `None` — the attributes are documentation, and the test now says so rather than implying a guard that is not there | 1439 Rust / 554 frontend |
| 2026-08-17 | **G9 closes: the volume-preparation screen now says whether a card's Kickstart suits the OS about to go onto it, and the check was proved against the pairing that actually failed under WinUAE with a licensed ROM (real hardware untouched).** `core/rom/pairing.rs::compare` reads two already-recorded facts — the tree's own planning ROM (G5's `distribution.json`) and the card's manifest (G7's `SourceFacts::kickstart_stated_major`, recorded at build time because ART writes FAT32 and has no reader for it) — and asks only whether the tree's own recipe requirement still holds, never "is this the same ROM"; `NotChecked` is never rendered as a pass (§89). Landed across four prior commits this session (the comparison, the `preload_rom_pairing` command, the screen, a review fix for a stale verdict surviving a re-fetch); this task's own job was to prove it against real material rather than fixtures. The brief assumed a manifest would already sit beside a real card — false on this machine, since every card here so far came from a test hook that calls `build_card` directly and never wrote one — so the proof hook builds its own card first, following `commands/card.rs::build_requested_card`'s own three calls (`build_card`, `describe_card`, `render_manifest`) against the real `Emu68-pistorm.zip` and the user's real Kickstart 40.68, then deletes both the card and its manifest when it is done. Run for real: the AmigaOS 3.2 tree that needs Kickstart 47 (`E:\amiga\ProjeART\dist-3.2b`) against that V40-carrying card printed `V47 tree: Unsuitable { needs: 47, found: Some(40), rom: "kick.rom" }` — the 2026-08-16 failure, reproduced on demand — and the same recipe's own V40 build (`dist-3.2-v40`, which carries its compatibility modules and needed no ROM) printed `V40 tree: Paired` against the identical card, since it is the very Kickstart file both the card and that tree's own plan hashed. `commands::preload::tests::the_real_trees_against_a_real_card_when_asked`, `#[ignore]`d and env-gated. No defect found by the run | 1434 Rust / 542 frontend |
| 2026-08-17 | **Cloanto ROMs become first-class input, and a card that could never have booted stops being built ([ART-128](ISSUES.md)).** The user mentioned owning licensed Amiga Forever ROMs as well as bare dumps — their community is Amiga owners with their own ROMs, using ART for real machines rather than emulators — and measuring what ART did with one found three things. `payload_for` read the ROM with a plain `std::fs::read`, so an `AMIROMTYPE1` header and half a megabyte of ciphertext went onto the boot partition as the Kickstart; the Amiga would not start and the only note was the same `RomUnrecognised` any uncatalogued dump gets. `identify_rom` stripped the header and then described the ciphertext, calling a licensed ROM *Generic Amiga 512KB ROM*. And the ROM screen showed a green **✓ Cloanto Encrypted Header Stripped** for every such file — true, and useless. Now: `decode_cloanto` undoes the repeating XOR with the `rom.key` **beside the ROM** (where Amiga Forever puts it and where `amitools`' loader looks — the algorithm read from that implementation, not remembered), after which the image goes through ordinary identification and a licensed A1200 dump is named and placed like a bare one. Without the key ART names the file for what it is, claims nothing, and **refuses** the card build rather than warning: a certainty, not a risk (ART-103's precedent). Proved with a synthetic ROM carrying a catalogued stored checksum, encrypted with a synthetic key — recovering `Kickstart 40.68 (A1200)` shows the decode produced the original bytes rather than merely different ones. **Then verified against genuine material, and the search took a turn**: the user's Amiga Forever ROMs are on this machine after all (`E:\amiga\shared
om`) and are **not encrypted** — plain 256K/512K images, as are the 39 on the original Amiga Forever DVD. So their licensed ROMs already work through the ordinary path, and the new table proves it: **25 of the 41 files named with their machine**, every one agreeing with Cloanto's own filename, while the boot ROMs and keyboard MCU beside them claim nothing — a **second, independent collection** confirming the Remus data. The DVD does carry a real `rom.key`, so the decode was tested with it rather than a synthetic one: the user's own plain A1200 ROM encrypted with their own key, back out as `Kickstart 40.68 (A1200)`. Key and ROM copy deleted afterwards; neither is in the repository | 1419 Rust / 533 frontend |
| 2026-08-16 | **ART's ROM database matched none of the user's 29 Kickstarts, and now names 24 of them with their machine ([ART-104](ISSUES.md)).** The entry described one A1200 dump; measuring first showed the ten hand-listed SHA-256s matched **zero** of the collection — so `compatible_models` was empty every time, `rom_suits` returned `None` every time, and the wrong-machine check had never fired for real material at all. The hashes also had no recorded provenance. **Fixed by identifying a dump the way the ecosystem does**: every Kickstart stores a checksum 24 bytes before its end, unique per *build*, which is what tells `40.68 (A1200)` from `40.68 (A4000)` — a distinction a shared revision can never make. `identify_rom` asks three questions in order (stored checksum, the old SHA table, then what the ROM states), and only the first can name a machine. The table is **generated, not hand-listed**: `scripts/rom-table-check.py` reads the Remus split database `amitools` ships (GPL-2.0-or-later, compatible; recorded in `THIRD_PARTY_LICENSES.md`) and emits 154 entries, with CI running the same script in verify mode so the committed file cannot drift from its source. Machine lists come from an explicit map of the database's own parentheticals rather than a tokeniser — the obvious split read `A500/2000` as A500 alone and `A1200_R2` as nothing, and a partial list would make ART warn 'wrong machine' about a ROM that suits it. An unmapped string stops the script; one already did. **Its mirror went with it**: the size-based fallback used to name machines from a file's *length* (a 256 KB image was "A500, A2000" — what ART told the user about their CDTV ROM), and unrecognised files were given the model `"Unknown"`. The size still names the shape; it no longer names a machine | 1415 Rust / 533 frontend |
| 2026-08-16 | **The session's own two debts, closed — and the smaller one turned out to be about the record rather than the count.** [ART-124](ISSUES.md): `apply()` counted plan items, and an `overrides` relationship writes one destination twice on purpose, so a real 3.2 install announced 4047 files where 3950 existed. Worse, and found while fixing it: every item pushed a `FileRecord`, so `distribution.json` — the only surviving record of where each file came from — held 94 paths **twice**, each duplicate crediting a different component, one of the two always false. Destinations are now tracked as they are written (keyed the way `plan::detect_collisions` pairs claimants, so the two cannot disagree), an override replaces its predecessor's record rather than adding one, and `bytes` follows the surviving file. Directories are deduped the same way, and ancestors no rule names (`Prefs/Presets` on the way to `Prefs/Presets/Backdrops`) are counted at last — a second undercount that had been hiding behind the first. Both real trees now match a filesystem walk exactly: 3950/278 for the V47 tree, 3954/281 for the V40 one, no duplicate path in either manifest. The test counts the tree on disk rather than deriving a number from `plan.items`, since that derivation is what was wrong; mutation-checked twice. [ART-125](ISSUES.md): a fallback copy reported `0 bytes` for twelve megabytes, because `hst-imager` answers in rounded units (`12.2 MB`) and the parser left the field at its zero default. `CopySummary::bytes` is an `Option` now — `None` is *not answered*, `Default` is a **known** zero so accumulators can start there, and `absorb` makes one unanswered step unanswer the run's total. The screen picks a sentence without the byte clause rather than printing a number nothing measured; a real zero still prints, because that is a different answer | 1411 Rust / 533 frontend |
| 2026-08-16 | **A user reported a guru, and it turned out every RDB filesystem ART has ever written was ignored by AmigaOS — fixing it made AmigaOS 3.2 boot to Workbench from a volume ART prepared.** The report was `Software Failure 8000 0008` (a privilege violation) booting `dist-3.2-fallback.hdf`. Reproduced in WinUAE against the user's own licensed Kickstart, then isolated by mounting the volume beside a boot floppy instead: **no volume icon at all**, so nothing was booting because nothing had mounted. The RDB said otherwise — FSHD present, DosType `PDS3`, the whole 121-block LSEG chain there, `rdbtool` extracting `pfs3aio` back byte-for-byte. The field that decides whether any of it is *used* was wrong: `PatchFlags` was `0x10`, which is `StackSize` (value zero), where `SegListBlock`+`GlobalVec` is `0x180`. ART's own comment named bit 4 as the seg list; it is bit 7. **Both of the user's real booting cards were read for the right value** — CaffeineOS writes `0x180`, MultibootOS `0x190` — and [ART-126](ISSUES.md) is that one-word fix plus the test that pins the two claimed bits *and* the two unclaimed ones. This is [ART-084](ISSUES.md)'s "a PFS3 disk ART makes now mounts" turning out to have been verified only by tools that never read the field: the project's own recurring shape, one layer up. **Then the tree spoke for itself.** With the driver loading, ART's own PFS3 format booted Kickstart 3.1 to `hello from ART` at the `1>` prompt — the first hard disk ART has made that an Amiga booted, and the settling of [ART-122](ISSUES.md)'s open half, since the real driver accepts the reserved-area layout `pfs3aio`'s own algorithm produces. The full 3.2 tree on a V47 ROM then asked for three things in turn, each a real recipe gap ([ART-127](ISSUES.md)): `LIBS/icon.library`, `LIBS/workbench.library` — both on `Install3.2`, the one disk the recipe named no component for — and `Sys:Prefs/Presets/Backdrops/default_pal.iff`, the wallpaper path the `backdrops` component had been left switched off waiting for somebody to measure. All three fixed from what the machine said, not from a guess. **AmigaOS 3.2 now boots to a clean Workbench, wallpaper and all, from a PFS3 volume ART prepared** — WinUAE and a licensed ROM; real hardware untouched. Two hooks were hardened on the way: the real-media run had one ROM's answers hard-coded and failed against the user's other real ROM (it now asserts the *rule* the condition encodes, with counts pinned per ROM), and `fixtures::required_media` exists because eight fixtures broke at once when a second required component appeared | 1408 Rust / 529 frontend |
| 2026-08-16 | **The real AmigaOS 3.2 tree, measured through the path that ships — the fallback carries all of it, and cannot finish a run.** The figures the project has been quoting for the real install (*"3061 of 4030 files, 969 excluded"*) came from a one-off manual pass against code that changed underneath them, so nobody knew whether [ART-120](ISSUES.md)'s fallback closes [ART-113](ISSUES.md)'s gap or merely reaches it more cleanly. `carry_the_real_dist_tree_through_the_fallback_path_when_asked` is the committed hook that answers it: it embeds the real `pfs3aio` at create time (so the run measures the copy, not [ART-117](ISSUES.md)), plans through `plan()` and runs through `run_with_fallback` — native first, exactly as the product does — and prints the source tree's own counts beside the copy's. **Answer: the fallback carries the whole tree.** `hst-imager`'s own listing counts *280 directories, 3933 files, 12.2 MB* against a source of 3933/280, `Locale/Countries` holding `españa.country`, `österreich.country` and `türkiye.country` by name — so the non-ASCII quarter is copied, not lost. **But not unattended, and that is the session's real finding**: `hst-imager`'s *first* write into a volume `NativeFormatter` has just formatted dies `System.IO.IOException: ERROR_DISK_FULL`, and the identical command succeeds on a second attempt — reproduced down to two files and 15 bytes into a 400 MB partition, outside any ART process. Byte-diffing before and after shows the failed attempt *repairing* the reserved bitmap ART wrote (120 bytes of `FF` zeroed), and `hst-imager`'s own format writes those bytes as zero from the start: the two PFS3 implementations disagree about how much reserved area exists — ART's bitmap marks 14 684 blocks free where `hst-imager`'s marks 11 188. **Then settled, and it is ART's**: `pfs3aio`'s own `CalcNumReserved`, worked through by hand for this geometry, gives 14 784 — ART's number to the block, and `libpfs3`'s port of it is faithful. So the fix was not to change ART's arithmetic to please the fallback tool but to stop mixing the two: [ART-122](ISSUES.md) makes the tool a per-**partition** choice, asked before the destructive step through a new `VolumeFormatter::can_copy_in`, so a volume is formatted and filled by one tool or by neither — and with no fallback configured the run refuses *before* the partition is erased instead of after. The real tree then went through in **one unattended run**: 3933 files / 280 directories, counted back by `hst-imager`'s own listing. Two mutations, both caught. It was invisible for two runs because ART reported the *last* line of the tool's output, which for an unhandled .NET exception is a stack frame — `at Hst.Imager.ConsoleApp.CommandHandler.Execute(CommandBase command)` — rather than the message twelve lines above it; fixed and pinned by three tests ([ART-123](ISSUES.md)). Counting the tree also caught [ART-124](ISSUES.md): `apply()` reports plan items, not entries, so the real install's headline 4030/330 is 98 files and 50 directories more than the tree it wrote — an override writes one destination twice, a shared directory is counted twice. and [ART-125](ISSUES.md): a fallback copy reports zero bytes, because the tool's own summary rounds to `12.2 MB` and a rounded string is not a byte count worth inventing. `FEATURES.md`'s two OS-install rows and this file's own numbers now say 3933/280 | 1405 Rust / 529 frontend |
| 2026-08-16 | **CI turns green on `main` for the first time since SD-2 G5 merged, and a docs sweep catches STATUS.md up to a session that had already finished.** The merge ("Merge sd-1: SD-2 G5, the AmigaOS install engine") was the first time any GitHub runner had ever compiled the card builder — those commits had sat unpushed on `sd-1` for days — and it failed on `clippy::question_mark` at `commands/card.rs:275`, twice (once on the merge itself, once again after the ART-121 fix wave landed on top of it). The lint is invisible on this machine: CI runs `stable` (clippy 1.97.0), this machine is pinned at 0.1.95 (rustc 1.95.0), so a green local `cargo clippy` proved nothing about CI's — the fix in `544282c` was written by reading the lint, not by reproducing it. Confirmed green on the third push: run `31939802790`, every step including `pnpm tauri build` and the amitools oracle. Measured fresh rather than carried forward: 1399 Rust (run twice, no flake), 526 frontend across 44 files, 1457 i18n leaf keys in `en.json` and `tr.json` alike — all matching what the previous wave's commits already claimed, so nothing had drifted since. **STATUS.md's "Picking up next session" section was rewritten in full** — it still described the session that ended before G5 merged (`sd-1` unpushed, G5/G9/G10 all "owed", a "Four things this project keeps re-learning" list tied to a day that had already passed) and was actively misleading about where SD-2 stands now: G3/native, G11 and G5 done, only G9 and G10 left, three specific things blocked on the user (driving both new screens in `pnpm tauri dev`, a WinUAE boot, a real A500 boot) rather than on code. `FEATURES.md`'s i18n row still read "1129 keys each" from Phase 0b; corrected to 1457. Everything else in both files checked against the code and the current `ISSUES.md` held up as written | 1399 Rust / 526 frontend |
| 2026-08-16 | **hst-imager is no longer required to prepare a card — [ART-120](ISSUES.md) makes `NativeFormatter` reachable, native by default with `hst-imager` a named fallback, then a review round closes four findings against the string and the record.** `core/preload/native.rs`'s `NativeFormatter` (`libpfs3` for PFS3, ART's own writer for FFS, launching nothing — G5) had existed since the previous session with its own tests and its own oracle and was reachable from **nowhere in the product**: `commands/preload.rs::preload_run` constructed `HstImager::at(...)` unconditionally. `run_with_fallback` fixes that — native runs first for every step of a plan, and only reaches the configured `hst-imager` for the two known, typed capability gaps ([ART-113](ISSUES.md)'s non-ASCII PFS3 names, [ART-117](ISSUES.md)'s new `ForeignRdbEmbedNotSupported`), both refused *before* anything is written so retrying on the other tool is safe. Chosen **per step, not per run** — `import-filesystem` always needs the fallback, `format-partition` and almost every `copy-in` never do — and never silent: `StepReport { step, tool, fallback_reason }` travels on every result and is logged. **Review then found the six-commit wave had shipped the engineering but not the record**: `preload.scope` and `layout.scope` still told the user ART cannot write PFS3 itself, which stopped being true the moment this landed; `preload.result.notVerified` said "the tool's word, not ART's", which is backwards for a native run and, read against `preload_run`'s own job closure, turns out to be true for the *same* reason on both paths — neither writer's output is read back and checked within this operation, so the string now says that instead of picking a side. The preview never named which writer would run a step at all, for a `Destructive` operation whose default writer had just changed under the user without a Settings toggle (deliberately not added — the decision was "native by default", not a choice to expose); `src/lib/preload.ts::plannedToolPhrase` now labels each step before the confirmation checkbox, from the plan alone. `run_with_fallback` also set `outcome.tool = native.probe().ok()` **before the loop ran**, so a run where every step fell back still printed "By libpfs3 … (native)" against its own per-step list — fixed to reflect what actually ran, mutation-checked by forcing every step through the fallback and asserting the summary follows it. And `preload.fallback.nonAsciiPfs3Names` passed `{{count}}` with no `_one`/`_other` forms, unlike every other counted key on the same screen — split in both catalogues. Filed as [ART-121](ISSUES.md), folding all four findings and this file's own gap into one entry. **Not done here**: the real 4030-file `dist-3.2` tree was not re-run through the new fallback path end to end, so the 969/106 non-ASCII figures in `FEATURES.md` remain the prior, separate measurement, not a claim the fallback closes that gap | 1399 Rust / 526 frontend |
| 2026-08-16 | **SD-2 G5 lands: a distribution tree was built from the user's own 3.2 media and put onto a volume; no Amiga has booted it.** Task 14 of the OS-install plan — the last task — runs the whole engine (Tasks 1–13) against the real thing for the first time: 36 ADFs at `E:\amiga\Amigatolon\paketler\3.2\AmigaOs 3.2\ADF` and the user's real Kickstart v3.1 rev 40.68. `find_media` found 35 of the 36 (`Install3.2.adf` is the OS's own boot/installer floppy, not component media the recipe names — expected, not a defect); `plan()` switched on 26 components, `modules-a1200` among them **by its own condition, unchosen**, because the real ROM's stated major (40) is below 47 — exactly the case the confirmation UI exists for, now proven against real hardware's own ROM rather than a fixture. `apply()` wrote 4030 files / 330 directories / 11.9 MB to `E:\amiga\ProjeART\dist-3.2`, manifest included — **after the run found two real recipe defects no synthetic fixture ever could**: `storage`'s six rules named a `Storage/` drawer the real disk does not have ([ART-111](ISSUES.md)), and `glowicons` was missing `classes` from its `overrides`, so the real `Devs/DataTypes/*.info` icons both disks ship collided ([ART-112](ISSUES.md)). Both fixed; the full 26-component plan then built with zero refusals. **Putting it on a volume found a third, open defect**: `libpfs3` 0.1.3 writes an entry's name as UTF-8 and reads it back as Latin-1, so any non-ASCII AmigaDOS name — real content on `Locale-ES/-FR/-PT/-TR` and the base `Locale` disk's own country files (`español`, `türkçe`, `österreich`, 24 *directories* named) — fails `copy_in`'s own sanity check outright ([ART-113](ISSUES.md), open, external pinned crate). Excluding those 24 directories excludes their whole subtrees: **969 of 4030 files and 106 of 330 directories — about a quarter of the tree — never reached a volume**, not "24 files/directories". A reduced tree (that quarter excluded, one-off Python pass, method recorded in ART-113 rather than a committed script) went onto a fresh 128 MB PFS3 `.hdf` instead: 3061 files / 224 directories / 10.3 MB, and **`hst.imager`'s own count matched ART's exactly**; every one of 3059 extracted files hashed byte-for-byte against the source, bar two (`Storage/DOSDrivers/AUX`, `AUX.info` — a real AmigaDOS DOSDriver name colliding with a Windows-reserved device name, `hst-imager`'s own extraction blind spot, filed as [ART-114](ISSUES.md) and not an ART defect). **All of the 969/106 and 3061/3059 figures above are a one-off measurement**, not reproducible from a committed script (see ART-113). `scripts/pfs3-oracle-check.py` reran clean, both directions. Also filed at this task, as required by the plan: the `core::iso` test flake seen three times this session and never diagnosed ([ART-115](ISSUES.md)), the PFS3 writer's silent `.uaem` comment/date drop ([ART-116](ISSUES.md)), `import_filesystem`'s refusal of a foreign card's existing RDB ([ART-117](ISSUES.md)), the OS Builder install screen's unverified depth ([ART-118](ISSUES.md)), and five deferred Task 13 minors folded into one entry ([ART-119](ISSUES.md)). **The WinUAE and hardware rungs are untouched by this task and stay open** — nothing built by G5 has been opened by WinUAE or booted on any Amiga, real or emulated | 1382 Rust / 518 frontend |
| 2026-08-16 | **Task 11: an independent witness for the PFS3 writer, both ways.** `libpfs3` is both the crate ART writes PFS3 with and the crate ART reads it back with, so ART's own suite cannot catch a mistake the two would share — the exact shape of ART-032 … ART-035, ART-075 and ART-079. `scripts/pfs3-oracle-check.py` closes that gap with `hst-imager` (C#, no shared code) in both directions: `build_pfs3_volume_for_oracle_when_asked` builds a volume through `NativeFormatter` — the same `format_partition`/`copy_in` G5 uses — and prints a JSON claim of every entry, which `hst-imager fs dir -r` is then checked against, protection bits included (`--P-RWED`, `-S--RWED` — the Pure and Script bits, the reason this phase exists: 3.2's `Startup-Sequence` runs `Resident C:Assign PURE`); `read_foreign_pfs3_for_oracle_when_asked` reads back a volume `hst-imager` itself formatted and filled, printing a **SHA-256 per file** rather than a length, because a length is exactly what ART-079 got right while handing over the wrong bytes. Run for real against `hst-imager 1.6.616`: both directions clean. Deliberately broken twice to confirm the oracle actually discriminates — a protection bit forced to zero was caught (`FAIL C/Assign carries --P-RWED (hst-imager says ----RWED)`), and a single byte flipped in a file ART read back was caught by the hash while the length still matched (`ok Readme is 34 bytes` / `FAIL Readme hashes to what hst-imager was given`), which is the property the second direction exists to prove. Local only, like `fat-oracle-check.py` and `iso-oracle-check.py`: the CI runner has no `hst.imager.exe` | 1357 Rust / 490 frontend |
| 2026-08-15 | **SD-2 G11 lands: what goes where.** `core/layout/` turns a dropped pile of files into an organised staging tree on the PC, which the preload screen then copies onto a card — the piece SD-2's design doc named as missing between the classifier and the card. `scan.rs` walks a drop and stops at a WHDLoad drawer instead of descending into it, depth-capped and skipping symlinks. `policy.rs` carries the drawer table as data — `Games`, `Floppies`, `HardDisks`, `CDs`, `Unsorted` — and the two kinds it refuses **with a reason**: a ROM is the FAT32 boot partition's business, a Commodore 8-bit disk has none on an Amiga volume. `policy.rs::drawer_for` returns `Result<&str, RefusalReason>` so the drawer and the refusal are one exhaustive decision instead of two matches in two files that have to agree. `apply.rs` builds the tree: never overwrites, never touches the source (proved byte for byte), `safe_join` on every destination because it is user-typed text, cancellable between items with `CancelledPartway` reporting how many landed. Unpacking a WHDLoad archive goes through the one archive gate and places the drawer's `.info` **beside** the drawer, never inside it — inside, the game is on the disk and invisible on Workbench (§82). `commands/layout.rs` and `layout_apply` take the plan they are given rather than recomputing it, unlike `preload_run`, because the user's edits to the preview *are* the plan. **The design decision worth keeping:** ART cannot tell a demo from a game — nothing derivable from the bytes separates them — so the policy proposes only what it can justify and its own **layout** screen (`/layout`, `src/pages/ContentLayout.tsx`) — a peer of the OS Builder in the sidebar, not a screen inside it — makes the preview editable rather than building a cleverer rule engine. Every rule is tested and the load-bearing ones mutation-checked: the refusals, the depth caps, the no-overwrite guard, `safe_join` (removing it genuinely wrote a file outside the staging root before being reverted), the §82 icon rule, the scratch-directory cleanup. The screen was mounted and driven in headless Chrome against the real bundle with `invoke` mocked, and it caught a real bug tests could not — the drawer dropdown was leaking a policy field's value into the list; every string resolved, no raw key and no `{{variable}}` reached the screen. **Not yet driven in `pnpm tauri dev` against real material, and no staging tree this builds has been carried onto a card** — nothing built here has been near an Amiga  A whole-branch review then found the feature's own centre unusable and it was fixed before landing: retargeting a row set a `stale` flag that blocked Apply, and the only thing that cleared it was a fresh preview — which rebuilds from policy and throws the edit away. The editable preview, which is the whole answer to "ART cannot tell a demo from a game", could not be used at all. `layout_recheck` answers the question the flag was avoiding instead of blocking on it: it recomputes collisions against the disk without replanning, so edits survive and the red list is always true — and when that check itself fails the screen says so and holds Apply, rather than reading an unverified list as an all-clear (§89). The same review found `unpack_whdload` discarding the extraction gate's verdict — `extract_selection` reports a truncated unpack through its **return value**, not an `Err`, so a drawer cut short by the output cap was placed and counted as a success — and an archive holding an `Install` script placed as a game, which `commands/whdload.rs` refuses outright. Both refuse now. Six things it found and G11 did not fix are filed as [ART-105 … ART-110](ISSUES.md) rather than left in a scratch file | 1203 Rust / 490 frontend |
| 2026-08-15 | **The preload engine gets a screen — the OS Builder's third kind.** `src/lib/preload.ts` and `components/osbuilder/VolumePreload.tsx`: read the card, tick the partitions to prepare, name each volume, optionally give it a folder, preview, confirm the count, run as a job, report. **No Rust changed at all** — the three commands had been registered since the morning and nothing above them existed. Three decisions carry it. **The ticks are not remembered**, alone among this screen's choices: the card and the paths come back the way the user left them, but a screen returning already armed to format two partitions is ART's own remembering rule turned into a hazard, and it is written down as that rather than left to look like an oversight. **The volume-name rules are `check_name`'s** — no `:` or `/`, thirty characters counted as characters — asked before the format instead of discovered during it; the engine never checked them and `hst-imager`'s answer would have arrived after the first partition was gone. And **the result panel says ART cannot read the volume back**: there is no PFS3 reader here, so what is inside is the tool's word, said as the tool's word (§89). `hst.imager`'s path became a setting beside `winuaePath`, since ART does not ship it. Four mutations, four caught. Driven in a real browser against the real bundle: mounts, every string resolves, no raw key and no `{{variable}}` on screen — **not** yet driven against a real card through Tauri, which is the rung it still owes. And the one thing a thin adapter can get wrong is now pinned in Rust: `the_payload_the_frontend_sends_deserialises` deserialises the literal object `src/lib/preload.ts` builds, because `#[serde(flatten)]` is all that makes the two agree and a renamed field would compile on both sides and fail under the user's finger. Mutation-checked — nest the request and it fails | 1167 Rust / 474 frontend |
| 2026-08-15 | **SD-2 opens: a card's Amiga volume is formatted and filled from Windows.** `core/preload/` plans it and runs it, `tools/hst_imager.rs` does the work, and `core/` still launches nothing — the boundary is a trait, the way `MirrorClient` is for the network, so route B replaces one file when it comes. **Route E over route D**: SD-0 had already run E end to end, it needs no Kickstart and no unattended emulator session, and the tool was already on this machine. **Running it corrected the design twice.** First: `hst-imager` answered *"Rigid Disk Block not found"* on a card ART built — of course it did, byte zero of a card is a partition table and the Amiga disk begins 1.1 GB in. That is ART-095 from the other side, and the Emu68 Imager's own command set had the answer all along (`<image>\mbr\<slot>`), which also meant a partition is two numbers — which disk, then which partition in *its* RDB — not one flattened index. Second: `fs copy` prints no summary at all, so the parser that read one was reporting numbers it had matched in a different sentence; the count comes from asking the volume afterwards, which makes it the tool's reading rather than ART's guess. Found on the way: `MbrPartition::index` is zero-based while its own comment promised the number everybody else writes down — `slot_number()` now exists and the comment tells the truth. Real run: RDB embedded, `DH0` formatted as `Work`, a tree copied, and the tool listed it back as 1 directory, 2 files, 20 B with `----RWED` on each | 1166 Rust / 454 frontend |
| 2026-08-15 | **SD-1 G15, and with it every gap the phase names.** The drop pipeline has been architectural since Stage 2 and answers one question — *what can I do with this file*. `core/card/intake.rs` asks the other one: *what does it become in this card*. An Emu68 archive and a Kickstart fill their fields; a `config_<name>.txt` is recognised and declared not-yet-used (G16); everything Amiga — a game, an ADF, an OS CD, a folder — is told it **needs a volume this card has not got**, which is the answer SD-1 owes most often and the one worth not swallowing. Two decisions: the archive is matched **by name and deliberately so** — its bytes say "zip", and which board and which release line it is for lives only in the name, which is what ART-091 was — so the answer carries *every* reading rather than picking one, and `emu68_payload` still refuses if it disagrees with the user's setting; and `intakeFills` is pure, so "the last archive dropped is the one chosen" is a rule with a test rather than whatever the loop happened to do. Four mutations, four caught | 1140 Rust / 454 frontend |
| 2026-08-15 | **SD-1 G8: the gate before the file is handed over.** `core/card/health.rs` answers "is this a card that will boot" in fourteen checks — the boot partition first (SD-0's unit-0 rule, now a check rather than only an impossibility) and at sector 2048, areas 4 MiB aligned, nothing overlapping or past the end, every RDB read and checksummed, no partition naming a filesystem the card lacks (ART-084 as a gate), the manifest still agreeing, and the four files the firmware needs present. **The design decision is the three states.** `pass`, `fail` and `not-checked` are kept apart and the third never renders as a tick: ART writes FAT32 and cannot read one, so the boot files are answered *from the manifest* and a card with no manifest answers nothing — a green mark meaning "ART did not look" is the claim §89 forbids. The verdict says it out loud: *"nothing is wrong with what ART could check — and N questions it could not answer at all"*. What ART cannot check at all is a separate list, for the machine (§50). G7's manifest button became a section of this one report rather than a second door. Four mutations, four caught. Real 8 GB card: fourteen passes, and 7-Zip still clean on all 21 boot files | 1131 Rust / 450 frontend |
| 2026-08-15 | **SD-1 G7: a card now says what it was built from, and something that is not ART checks half of it.** `core/card/manifest.rs` writes `<card>.manifest.json` beside the image — the archive's and the ROM's SHA-256, the partition table's, every boot file's, and a 256 KiB window at each RDB. Two decisions carry it: the manifest is **read off the finished card** rather than remembered from the build, so a card ART wrote wrongly cannot be described rightly; and it lives **beside** the image, because a manifest carrying the boot partition's checksums *inside* the boot partition cannot be right about itself. `card_verify_manifest` checks the table, the areas and the RDBs — and **reports what it did not look at**: ART writes FAT32 and does not read one, so the boot files are recorded and left unverified rather than passed over. `scripts/fat-oracle-check.py <card.img>` closes that half with 7-Zip. Driven against the real `Emu68-pistorm.zip` on an 8 GB card: ART's own check clean, 7-Zip clean on 21 of 21 files. Mutation-checked in three places, and the third mutation **found a missing test** — nothing exercised the RDB window, so a byte written into the reserved area (which leaves every parsed field identical) would have passed | 1124 Rust / 449 frontend |
| 2026-08-14 | **A card was built from the screen, and looking at the screen closed one issue and reopened my own mistake.** The OS Builder's boot-only card was driven in the running application with the user's own material and produced a 64 GB `denemecard.img`; 7-Zip reads its table back independently. On the way there I screenshotted the window, read text as clipped at the right edge, and **committed a wrong diagnosis onto [ART-099](ISSUES.md)** — the capture was made by a DPI-unaware process on a 150 %-scaled 3840-wide display and returned the left *two thirds* of the window. An overlay that printed the page's own rects settled it in one shot: `scroll == client` at every level, `.app-shell` exactly one window wide, the left strip is `max-width: 1180px; margin-inline: auto` doing its job. **ART-099 closes with the shell exonerated**, and with a second half to its lesson: a screenshot is not a measurement, and check the instrument before believing the picture. Also read the Emu68 Imager at source level ([sd0-prior-art.md §4.1](sd0-prior-art.md)) — it settles ART-104 in data (several MD5s per Kickstart revision, and the 524 299-byte Amiga Forever header ART rejects outright), hands SD-2 the `hst-imager` command surface verbatim, and confirms ART's no-physical-media scope as the same tool minus an Administrator requirement | 1114 Rust / 443 frontend |
| 2026-08-14 | **The application can ask for a card.** `card_plan_build` and `card_build` are the adapter the engine had been waiting for, and the OS Builder leads with a **boot-only card** — the one thing on that screen that produces a file, sitting above the distributions that are all still Coming Later. Four questions (archive, ROM, size, destination) and an `Advanced` panel for everything with a defaulted answer; the hardware half of it writes the *same* `settings.json` keys the PiStorm studio uses, so an answer means one thing in both places. Three decisions worth keeping: **one request type and one `card_spec` for the plan and the build**, so a screen cannot show one card and write another; **`SAFE_CREATE` is answered by the plan**, before the button rather than by a job that fails; and **warnings are a typed enum, not sentences** — `CoreError`'s strings still reach the UI in English (ART-060) and there was no reason to add four more. Two things came out of using real material rather than fixtures: `emu68_payload` had no *total* ceiling — twenty entries under the per-file 64 MB is a gigabyte and a quarter held in memory, so there is a 256 MB budget on the running total now — and **[ART-104](ISSUES.md)**, the user's own A1200 3.1 ROM hashing to a dump `KNOWN_ROMS` does not carry, so every card built with it is warned about a ROM that is very probably right. Filed rather than patched: the fix is to identify a ROM by its own header and treat the checksum as confirmation, which is a change to how ROMs are identified. Driven against the real `Emu68-pistorm.zip`: 21 files, booting `Emu68-pistorm.gz` | 1114 Rust / 443 frontend |
| 2026-08-14 | **Docs swept and the branch pushed.** Every claim about *now* checked against what is now true: the snapshot still said G2 was owed and that reading a card had no screen; FEATURES still described three fixed defects as live, including Application Size cutting the right edge — which measurement had disproved; README carried two paragraphs that contradicted each other and each other's dates. History kept its own wording; only the present tense was corrected. `sd-1` pushed to origin, fourteen commits, level as of this line | 1106 Rust / 432 frontend |
| 2026-08-14 | **The payload chooses itself, and doing it against the real archive found a card that would not have booted.** `core/card/payload.rs` takes the user's `Emu68-pistorm.zip` and their Kickstart and produces everything the boot partition needs: the archive is checked against their board *and release line* before a byte of it is read (ART-091's lesson — the same file name means a different board in the two lines), `Emu68-raspi.zip` is refused for what it is rather than as "wrong", the Pi's own `config.txt` is merged rather than regenerated (§39/§40), `cmdline.txt` is written from nothing because the release has none, and the ROM goes on under the name the config points at. Then **[ART-103](ISSUES.md)**: `merge_config_txt` managed the `kernel=` key and wrote `Emu68.img` over the release's own `kernel=Emu68-pistorm.gz` — a name no release has ever shipped, pointing at a file the card does not carry. The card would have failed on the Amiga, where nobody could see why. The kernel's name is a field now, taken from the archive's own config and **verified to be among the files being placed**, so an archive that names a kernel it does not carry is refused before a card is written. Found the same way ART-090 and ART-091 were: by reading what the real thing says instead of what ART believed | 1106 Rust / 432 frontend |
| 2026-08-14 | **ART built a PiStorm card, from the real Emu68 release.** `core/card/build.rs` puts the four pieces in one file in the right order: a sparse image, the partition table, the formatted boot partition with its payload, and **an RDB at the start of every Amiga area** — each with block numbers relative to its own offset, which is the mirror of ART-043 on the writing side. It then reads the file back with the same reader that opens CaffeineOS and MultibootOS, because a build that cannot be read is not a build. Driven against real material through `build_real_card_when_asked`: the complete `Emu68-pistorm.zip` (20 files, `overlays/` included) plus a Kickstart, laid on a 2 GiB card whose first Amiga disk lands at 1 178 599 424 — the same offset both real cards use. Two findings from reading the real payload rather than the specification: **the boot partition is not flat** (the Emu68 release has an `overlays/` folder and CaffeineOS's card has eighteen folders, so `create_boot_partition` creates directories now), and **`fatfs` writes two things wrong in every directory it makes** ([ART-102](ISSUES.md)) — long-filename entries on `.` and `..`, which the format forbids and 7-Zip reports, and `..` pointing at the root's own cluster instead of 0. Both repaired, both pinned by a test that reads the bytes, and the oracle now writes a folder so it cannot pass while the fault is there. `SAFE_CREATE`: an existing card image is refused, never built over | 1097 Rust / 432 frontend |
| 2026-08-13 | **The card's boot partition, and the first filesystem ART writes that is not an Amiga one.** `core/fat32.rs` creates the FAT32 the Raspberry Pi boots from and puts files in it — `fatfs` rather than a formatter of ART's own, which is what the gap analysis asked for and the right call for a filesystem whose entire job is to be read by somebody else's firmware. Three decisions worth keeping: **FAT32 is forced**, because `fatfs` picks a width from the size and a small partition would silently come out FAT16 and not boot; **`chrono` is off**, so files carry no timestamps and a build produces the same bytes twice, which is what a manifest can describe (G7); and **every write is bounded to the partition** by `Region`, because the Amiga's first RDB begins where this partition ends — a formatter that ran past its end would take it, so a write past the end is refused rather than shortened, and a test watches the bytes on both sides. Cross-checked against **7-Zip** (`scripts/fat-oracle-check.py`): `File System = FAT32`, 512-byte sectors, 4 KiB clusters, the label, and every file's bytes back byte-for-byte, long names included. `cargo deny` clean with the new crate; `THIRD_PARTY_LICENSES.md` and CLAUDE.md's core-dependency list both updated in the same commit | 1089 Rust / 432 frontend |
| 2026-08-13 | **G2's first half: ART can decide a card's shape and write its partition table.** `plan_card` + `write_mbr` in `core/mbr.rs`, built on the two real cards rather than on the specification — and reading their tables byte by byte paid twice. First, the front of both cards is identical to the sector (LBA 2048, 2 299 904 sectors of FAT32, first Amiga area at LBA 2 301 952), so the defaults are measured: **1.10 GiB of boot partition, not the "~200 MB" the research estimated**. Second, the two cards *disagree* about the CHS fields and the boot flag and both boot — which is a far better licence for a design decision than a specification is, and is why ART writes the LBA sentinel in every CHS field. Asked for MultibootOS's shape the planner reproduces MultibootOS's layout to the sector. **An Amiga disk at byte zero is not expressible**: the boot partition is not optional and it is first, which is how SD-0's unit-0 rule is enforced — by having no way to say the dangerous thing. Ten tests. Still owed for G2: a filesystem inside that partition, and the Emu68 payload in it | 1080 Rust / 432 frontend |
| 2026-08-13 | **ART-043 closed — a partition inside a small image is written where it lives.** The whole-file strategy handed the writer the whole file at offset zero while the geometry described a partition megabytes in, so for any RDB image of 16 MiB or less volume-relative block numbers were used as file-absolute. Nothing was ever at risk, and that is now *measured*: the gate ran `validate_image` over the whole file, which stops at the signature — `RDSK`, not `DOS` — so a small RDB image could not be committed at all. `WholeFileVolume` replaces the three copies of that branch with one session that gives the writer the volume's own bytes, opens it at the volume's offset, validates the volume, and splices it back into the file for one atomic write; everything around the partition survives byte-for-byte and a bare ADF is untouched by the change. The fixture the entry said nothing constructed now exists — a 12 MB image with a formatted 4 MB partition — and the test asserts every byte *before* the partition is unchanged, which is where the RDB is. Mutation-checked. The other half of "writing at an offset", writing an **RDB** at one, belongs to G2 and has no caller yet | 1070 Rust / 432 frontend |
| 2026-08-13 | **ART-099, measured instead of looked at — and the diagnosis was wrong.** `scripts/zoom-check.py` drives the running application in headless Chrome and reads the widths out of the page; seven screens, three sizes, in a window the size of the user's own. `.app-shell` renders **exactly one window wide at every size**, so it was never drawn `z` times too wide and both reverted fixes were corrections to something that was not happening. Nothing on any screen overflows its column at 130 % either. What *was* real: `.app-content` carried `overflow-x: hidden`, and since zoom spends width (2299 CSS px at 100 %, 1717 at 130 %, 1038 at 200 %) anything that did exceed the column would be clipped with no way to reach it — the box could still be scrolled by script while offering the user no scrollbar at all. One character. The entry stays open for the one check a dev server cannot make: the real WebView2 window with real data. Filed on the way: **ART-101**, the sidebar's `max-width: 1000px` collapse is asked of the real viewport and so never fires under zoom — 448 real px of a 1258 px window at 200 % | 1068 Rust / 432 frontend |
| 2026-08-13 | **A card has a screen.** `core/card.rs` could read a real PiStorm card since the morning and nothing could show one; `card_open` and the Hard Disk studio's card view close that. The studio now asks the **card reader for every file** and branches on whether a partition table was found — an HDF comes back as one area at offset zero, so a card is never recognised by its extension and `hdf_open`, which cannot open a card at all, is never asked to. The card view is a list of *disks*: the four MBR slots as the card's own documentation numbers them, one section per Amiga disk with its offset and its partitions, and the drivers **unioned across the whole card** with the unmountable question asked against that union (ART-097, computed in Rust so the UI cannot get it wrong). Read-only, and it says so on the screen. Verified against both real cards: MultibootOS 2.2 — 128 GB, 2 Amiga disks, 17 partitions, 2 drivers, none unmountable; CaffeineOS 9317 — 64 GB, 1 disk, 2 partitions, 1 driver. Also: **every missing material arrived**, `VideoCore.card` among them (the name that needed checking), so G2 is unblocked | 1068 Rust / 432 frontend |
| 2026-08-13 | **Two more off the open list.** ART-066: planning a batch of archives has to unpack every one of them, and it did that on the command thread — the window froze with no progress and no Stop, in the step that exists so the user can change their mind. It is a job now, with the plan arriving on its own event; `busy` is cleared by the plan *or* by the job ending, which is the cancelled case the old code could not have at all. ART-058: cancelling a copy into a large image left the files that had landed in place, correctly, and said only "cancelled" — the same word the whole-file strategy uses when it leaves nothing. `CancelledPartway { files }` carries the count as a number to `JobState::Cancelled { files_landed }`, still a cancellation and never a failure, and the sentence is the UI's in the user's language. The large-image test asserts the count against the volume's own listing | 1066 Rust / 425 frontend |
| 2026-08-13 | **Four off the open list, the four that needed no decision first.** ART-070: refreshing a pane moved the keyboard into it, so after F5 the next key acted on the pane the user was not looking at — `refresh` puts focus back, and the harness test proves the old behaviour too by running with it. ART-067: Stop was heard between archives but not inside one, because the unpack was handed `NoProgress`; a `BatchStep` sink forwards the cancel flag while keeping the batch's own counts, since forwarding it raw would make the progress bar leap and fall back at every archive. ART-068: the pane inferred "your mask matches nothing" from two counts matching a shape; `filterEntriesReporting` answers it where the removing happens. ART-049: `VolumeWriter::open` now refuses a geometry that contradicts the bootblock's dostype — no existing caller was refused, which is the evidence they all already agreed | 1063 Rust / 419 frontend |
| 2026-08-13 | **ART-085 closed: what ART has open now outlives the screen.** Six studios each held their open file in a `useState`, so leaving the screen threw it away while the Dashboard's Recent list still named it a second later. `useOpenObject(kind)` — one small store, nine slots — is a drop-in for all six. **Session-scoped by the user's decision**: it never reaches `settings.json`, because a path that outlives the run can name a file since deleted or unplugged, and that is a bigger design than this asked for. Only the path is held; a studio re-reads its file on the way back, so nothing comes back stale. Router state still wins. Caught on the way past: ADF Studio's `loadDisk` reset the hex panel on every run, which on a reopen would have switched off a remembered choice — a setting changing without the user changing it. Mutation-checked: the harness back on `useState` fails two of five cases | 1060 Rust / 410 frontend |
| 2026-08-13 | **ART-096 closed.** The RDB half had landed; what was left was the half that decides what a *user* gets. `HardDiskStudio.tsx` hard-coded `num_buffers: 100` in three places, so the core's measured 600 reached nothing anybody created through the UI — a literal in a component quietly outvoting the engine. The three are gone and the field is now `#[serde(default)]` / optional in `hdf.ts`: absent and zero both mean "the core decides", because a screen that never asks for a buffer count has no business stating one. Three tests, mutation-checked in both directions — restoring either old value fails them. Docs: CLAUDE.md gained the network layer, the two-catalogue string rule and the Vitest jsdom trap; CONTRIBUTING's "branch from `master`" is gone, published months ago | 1060 Rust / 401 frontend |
| 2026-08-13 | ART-096 half done (RDB `MaxTransfer`, `Mask`, 600 buffers written and read back); ART-100 (the PiStorm screen said "choose a card first" only by being grey); ART-099 reopened after two bad fixes were reverted; docs swept — seventeen fixed entries moved out of ISSUES' Open section, fifteen stale `#open` anchors corrected, CLAUDE.md's dead branch name and missing CI step fixed | 1057 Rust / 401 frontend |
| 2026-08-13 | **GitHub caught up, and CI turned out to be broken.** 28 commits of `main`, 15 of `phase-2b` and the whole `sd-1` branch had never reached the remote. Pushing them showed the last three runs on `main` red — and the reason was never in the code: the licence gate used a **container** action on a **Windows** runner, which cannot run, so it failed on every push since it was added and took `Build application` and the MSI artifact down with it ([ART-098](ISSUES.md)). `docs/licenses.md` had been claiming that check ran the whole time. The frontend tests were never in CI at all — four hundred of them, including the i18n parity check. Both fixed | 1057 Rust / 401 frontend |
| 2026-08-13 | **ART can read a real PiStorm card — the thing that blocked SD-2a.** Two real distributions arrived (CaffeineOS 9317, MultibootOS 2.2) and reading them found that ART could not open either: `find_rdb_location` looked in the first 16 blocks of the file, and on every card those are the MBR and the FAT32 partition, with the Amiga's RDB about 1.1 GB in ([ART-095](ISSUES.md)). `core/mbr.rs` + `core/card.rs` fix it, and a card is a **list** of Amiga disks: MultibootOS has two, with different geometries, and its second RDB carries no PFS3 while all fifteen of its partitions are PFS3 — so drivers are the card's, not the area's, or ART would name fifteen working partitions as broken ([ART-097](ISSUES.md)). Both verified against the real files. Also filed: [ART-096](ISSUES.md), ART writes `MaxTransfer` and `Mask` as zero where every partition on both cards uses `0x1FE00` / `0x7FFFFFFE`. The layout is written up in [sd2-card-layout.md](sd2-card-layout.md) | 1057 Rust / 401 frontend |
| 2026-08-13 | **The two things left owed from the previous rounds.** ART-094: the `w` bit is checked before an overwrite, at all three paths — and it caught a side effect of the fix that created it, since ART-088 had made every overwrite path ask the *deletion* question by going through `delete`. Two bits, two guards, `a_delete_protected_file_may_still_be_overwritten` keeping them apart. ART-092: a named firmware set can be deleted, backed up first so it stays recoverable, and never the one the card is currently running. Still owed and named: the copy dialog's own write-protection question, and [ART-093](ISSUES.md#open) (fetching a kernel) | 1038 Rust / 401 frontend |
| 2026-08-13 | **Four from the open list, while the card is being prepared.** ART-088: the *writer* honours the delete-protection bit now, not just a dialog — `delete` refuses and names the entry, `delete_with(.., Override)` is the way past it, and the Files screen sends the answer only where it has shown the question. Move asks before the copy half rather than after, so a refusal cannot leave a duplicate behind. ART-072: `Docs` and `docs` are one drawer on an Amiga, so both collision checks compare without case — the clean refusal fires where it used to become a pile of unexplained skips. ART-071: a selection of nothing but shortcuts said it had copied everything; the report carries the declined roots now. ART-061: "1 weeks ago" — fixed with `_one`/`_other`, and with `count`, which is the half that makes i18next actually pluralise. The `w`-bit question is split out as ART-094 rather than left inside ART-088 | 1031 Rust / 401 frontend |
| 2026-08-13 | **The OS Builder knows the distributions.** `core/distro/` is a registry of real AmigaOS distros as data — CaffeineOS, CoffinOS, AmiKit, two ART Baseline entries — with the licence model each one obliges the user to, the Kickstart family its base wants, and the card it needs. The `/os-builder` screen leads with the licence, checks the ROM family and the card size, and says plainly that ART cannot write a card yet: the adapter is blocked on reading a real distribution's layout by hand (research §8.2) rather than guessing at it. Two open questions closed with evidence in [sd2-distro-decisions.md](sd2-distro-decisions.md) — the **free Aminet Picasso96 is enough** (so ART Baseline is reproducible without a paid component), and the HstWB package format, whose `Install` turns out to be 26 KB of Amiga script only an Amiga can run | 1024 Rust / 400 frontend |
| 2026-08-13 | **PiStorm fix round.** The Kickstart goes through ROM Manager now — every ROM on a card identified by checksum and labelled with its version and machines, one pickable from anywhere on the PC and copied on under a confirmed name; unrecognised stays a label, never a refusal. The kernel states its version, read from the `$VER:` string Emu68's own build compiles in. Named firmware sets can be created, duplicated, renamed and activated, each through preview to backup to write. **[ART-091](ISSUES.md) found in review**: ART named `Emu68-pistorm16.zip`, which no Emu68 release has ever shipped — and the name that does exist, `Emu68-pistorm.zip`, means the *classic* board in 1.0.x and the PiStorm32-lite/PiStorm16 in 1.1 alpha. The release line is now a field and the answer a type with `Absent` and `Unstated` cases. Owed and named: [ART-093](ISSUES.md#open) (fetching a kernel update) and [ART-092](ISSUES.md) (deleting a set) | 1013 Rust / 379 frontend |
| 2026-08-12 | **The PiStorm screen stopped inventing things (ART-090).** It offered a JIT switch (Emu68 *is* a JIT), an MMU switch (it emulates none), a Fast RAM slider (it maps RAM itself) and `emu68-sd.device` (the real driver is `brcm-sdhc`/`brcm-emmc`), and wrote three tokens Emu68 has never read. Rebuilt from the user's research brief as `core/pistorm/{hardware,options,firmware}.rs`, 58 tests: a three-field hardware matrix everything derives from, one field per documented token, four profiles whose claims are the tokens they write. The screen prints the token beside every control and the whole line beneath them. **Not verified on real hardware** — no card built by it has been booted | 985 Rust / 367 frontend |
| 2026-08-12 | **Every choice is remembered now, and the whole program can be made bigger.** `@/lib/remembered` gives every screen's view modes, filters, folders and configs a checked home in `settings.json`; Application Size scales the entire UI 70–250 % with Ctrl+wheel, Ctrl+±, Ctrl+0 and a Settings slider. Both from the user: *nothing changes unless the user changes it*, and *fonts, search fields and icons on every screen* | 985 Rust / 367 frontend |
| 2026-08-12 | **SD-1 G4: a PFS3 disk ART makes now mounts.** Both halves of RDB filesystem embedding — `parse_file_systems` reads the FSHD/LSEG chain and the studio names any partition nothing will mount; `create_rdb_layout` writes a driver into an RDB it builds, refusing with block numbers one that will not fit the reserved area. Version comes from the binary's own `$VER:`, and ART asks rather than defaulting to 0.0 when it is silent. **Verified from outside, both directions**: `rdbtool` reads ART's image (`PDS3 version=19.2 size=59120`) and extracts the driver back SHA-256-identical; `hst-imager` reads it too. Added to `oracle-check.py` with a synthetic driver (48 → 53 checks, CI-blocking) and mutation-checked: a fixed `SummedLongs` is caught. Closes ART-084. Also: the Aminet download folder is now set from Settings, validated by the engine before it is remembered | 931 Rust / 316 frontend |
| 2026-08-12 | **Phase 2b complete.** Enter opens a container in the same pane and Backspace comes back out with the cursor on it — verified on a real disk. Full keyboard coverage, tabs with session restore, per-filetype colour rules, and the confirmations his config keeps on. Four gaps filed rather than smuggled in (ART-080/081/087/088). Two acceptance points unverified and said so: the light theme and session restore | 912 Rust / 279 frontend |
| 2026-08-12 | **First human session with the running application.** ART-082 found and fixed in seconds — the Files panes filled the window and their listings did not, capped at 420 px, behind 178 green tests. ART-083 fixed (the HDF wizard's 8 GB ceiling was five numbers in a component, not an engine limit; there is a Custom size now). ART-084 recorded and labelled in the dialog: a PFS3/SFS HDF is a DosType with no filesystem and no RDB driver, so an Amiga cannot mount it. ART-085/086 recorded. **Scope decision: ART builds the PiStorm `.img`, never the card** — G1 deleted, and G14/G15/G16 (wallpaper + WiFi + prefs, a build as a drop target, real multiboot) added from the user | 912 Rust / 191 frontend |
| 2026-08-12 | **Bare metal.** `test/art-bootable-test.adf` cold-booted a real A500/A500+ (Kickstart 3.9) from a Gotek to an AmigaDOS `1>` prompt — ART's own boot code running on a real 68000, photographed. Every earlier pass was WinUAE with licensed ROMs. Left standing: physical magnetic media, which a Gotek is not | — |
| 2026-08-12 | Phase 2b task 3: the pane header is a source combo, a path and a filter, with the button strip behind a Settings toggle; the command line navigates and filters and refuses everything else by name; one non-wrapping F-key row; one status strip inside the dock instead of three banners above the panes; the collision question moved into the copy dialog. **F6 is Move** — verified against the destination's own listing before anything is deleted — with three directions refused by name for want of a primitive (ART-080, ART-081) | 912 Rust / 178 frontend |
| 2026-08-12 | **Published at github.com/tolon/Art** (public). Licence changed MIT → GPL-3.0-or-later to match the repository, in all seven places that claimed one. Phase 2a merged to `main`; phase 2b started on its own branch so half-finished UI did not travel with it | 912 Rust / 137 frontend |
| 2026-08-12 | Phase 2b tasks 1–2: the commander fills the window with panes equal by construction, rows lost their buttons, `Ctrl+B` collapses the sidebar; the palette is now the user's own and the chrome follows the theme | 912 Rust / 137 frontend |
| 2026-08-12 | Phase 2a closed. Also this session: the built-in machine profiles now cover the classic line end to end (A1000 … CD32), and Commodore 8-bit files became scope in writing (spec addendum §10.5) | 908 Rust / 137 frontend |
| 2026-08-12 | Task 5: `core/cbm` — D64/D71/D81 (35 and 40 track) and T64 read and browsable, TAP/PRG/CRT identified with a real answer rather than a shrug. Amendments A3 (40-track sizes, refusal carries the size) and A4 (a T64's header is not trusted) both in. New `Commodore8Bit` category so a 1541 image is never offered ADF Studio or Gotek | 908 Rust / 137 frontend |
| 2026-08-12 | Task 4: one archive security gate with the format behind a backend trait, then ZIP and 7z through it, then the archive pane and its virtual tree. MSRV 1.77 → 1.93 for a maintained 7z decoder. ART-076 (LHA matched at the wrong offset) and ART-077 (the file manager ignored a workflow's object) found and fixed; cargo-deny was broken three ways and now runs | 857 Rust / 134 frontend |
| 2026-08-12 | Tasks 3a/3b: `scripts/iso-oracle-check.py` checks the disc reader against 7-Zip (raw layouts included, by stripping sectors); Mode 2/XA Form 1 read and Form 2 refused, closing ART-075 | 819 Rust / 129 frontend |
| 2026-08-11 | Tasks 1–3: content-first `detect()`, an ISO9660 + Joliet reader, and a disc pane with F5 out of it both ways. Task 3's review fixes landed after a mid-session reboot: one copy-out boundary joined in Rust, the user's overwrite policy reaching a disc, and one selected file copying as that file | 814 Rust / 129 frontend |
| 2026-08-11 | Task 8 closes Phase 1a: `volume_copy_between` covered end to end through its own command pipeline, not just the staging primitives beneath it (source proved byte-for-byte unchanged). Docs updated for the whole phase; five debt items opened (ART-064 … ART-068), none data-unsafe | 731 Rust / 107 frontend |
| 2026-08-11 | Phase 1a: real pane focus + Tab, `Set<string>` multi-select (Shift/Ctrl/Insert/Ctrl+A), batch copy-in/delete/multi-archive-install as one job each, one listing order + per-pane sort + filename mask, Files restyled as Total Commander with an Attr column. Outside the plan: ART-063 fixed — ART writes real boot code, verified by booting a real image; first real-hardware verification of any kind, under WinUAE/Amiga Forever (A1200, A500+), not bare metal. First frontend test infrastructure (Vitest+jsdom) landed in Task 1 | 729 Rust / 107 frontend |
| 2026-08-10 | Phase 0b: nine dead code paths removed (ART-047/048/051 closed); `tr.json` ships beside `en.json` at 814 keys each, every screen/component/`src/lib` helper translated, parity enforced by a new frontend test suite (vitest, 0 → 10 tests). Rust error strings and `formatAge`'s English pluralisation remain untranslated, and no language has been checked on screen — recorded as ART-060/061/062 | 683 Rust / 10 frontend |
| 2026-08-10 | Phase 0a review fixes: a cancelled install no longer commits half a package or reports success (ART-052); `all_bytes()` refuses a volume too large to hold in memory (ART-053); the install pre-flight guard is now covered by a test that fails without it (ART-055); the WHDLoad refusal panel stopped contradicting itself and its remedy comes from Rust (ART-054); sidebar clipping (ART-056) and two more disabled-looking controls (ART-057) | 687 |
| 2026-08-10 | Phase 0a: ADF Studio's root-block bug (ART-037/038) and shell scroll/scale/disabled-controls (ART-039/040) fixed live; install pre-flight refusal restored (ART-044); a re-read race in `MutationOutcome` closed (ART-045); `core/adf/mutate.rs` retired onto `core/volume/write`; validation measures each image at its own geometry and gates every write (ART-041/042) | 684 |
| 2026-08-09 | §82: one-click WHDLoad install to HDF — pack layout, plan, refusals, oracle-checked end to end | 675 |
| 2026-08-09 | Stage W: writing into any volume — journal, F3–F9, checkout/checkin, `.uaem`, install to HDF; oracle now runs both ways | 642 |
| 2026-08-09 | Aminet update view (§41.5.6); `check_name` counted UTF-8 bytes instead of characters | 617 |
| 2026-08-09 | Folder copy into ADF, Aminet → Collection, editable mirrors, download folder and mirrors now persist | 425 |
| 2026-08-09 | Stage R: `BlockDevice`/`VolumeGeometry`, HDF partitions mount, amitools oracle — ART-032/033/034/035 found and fixed | 415 |
| 2026-08-09 | Stage R: HDF partitions browse in the file manager; `commands/volume.rs`, partition picker | 420 |
| 2026-08-09 | Two-pane file manager at `/files` (local ↔ ADF, HDF partitions), install-to-ADF, drag & drop in and out | 368 |
| 2026-08-09 | Aminet search: sorting, filters, user-chosen download folder with subfolders | 355 |
| 2026-08-09 | Aminet §41.5 Stage A: commands on the Job Queue, `src/lib/sources.ts`, Aminet Studio at `/aminet` | 326 |
| 2026-08-09 | Aminet §41.5 Stage A: `net/http_mirror.rs` (ureq); ART-030 failover corruption and ART-031 level-2 LHA headers fixed | 320 |
| 2026-08-09 | Aminet §41.5 Stage A: format verified against live mirrors; parser corrected to the real five-column layout, defaults pinned | 299 |
| 2026-08-09 | Aminet §41.5 Stage A: `core/sources` engine — parsers, catalog, mirrors, trust pipeline, sync (no shell, no UI) | 295 |
| 2026-08-09 | Stage 5 design: Aminet (§41.5) and AI layer (§45.5) designed; no code yet | 180 |
| 2026-08-09 | Stage 4b/c: Job Queue (§54/55) with global job bar, Beginner/Power mode (§47/48) | 180 |
| 2026-08-09 | Stage 4a: Operation Log (§53) + error IDs (§68), wired into every mutating command | 169 |
| 2026-08-09 | Stage 3: audited HDF/RDB/collection/rom; 9 defects fixed (sparse images, RDSK offsets, bounded scans) | 157 |
| 2026-08-09 | Docs restructured: STATUS / ISSUES / FEATURES split out of CLAUDE.md | 143 |
| 2026-08-09 | Stage 2: Workflow Engine catalogue, `run_workflow`, plan-driven Dashboard | 143 |
| 2026-08-09 | Stage 1: `core/safety`, ADF hardening, config round-trip, 20 defects fixed | 133 |
| 2026-08-08 | Phase 0 foundation delivered (per CHANGELOG) | 90 |
