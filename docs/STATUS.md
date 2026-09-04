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
| **Last updated** | 2026-09-04 — a documentation sweep: every number in this table re-measured against the tree, and the decayed copies removed. **The last change to what ART does was 2026-08-25** — the AGS export assessed and deliberately not built. This cell used to carry a *"Before that: …"* chain of every earlier round; that chain is the [session log](session-log.md)'s job, it had already drifted a day behind it, and it is deleted rather than duplicated |
| **Version** | **0.9.0** (2026-09-04) — the first version cut as a GitHub release with installers attached, built by `.github/workflows/release.yml` from the tagged commit. 0.8.5 was the first published for other people to use. Deliberately not 1.0: of 233 marked rows in [FEATURES.md](FEATURES.md), 162 are green — **32 amber, 33 not started, 6 stubs** (counted 2026-09-04, after G14's network half was found marked "not started" three rounds after it shipped; the previous figure, "30 amber", predates several rounds), and as the row above says, what is left in SD-1 is not code: a card flashed and an A500 booted. That is the bar for 1.0, not a bigger number |
| **Current stage** | **SD-0, SD-1, SD-2 and SD-4 built; SD-3 mostly, SD-5 part-built.** The per-gap detail is the stage table further down this file, which is maintained gap by gap — this cell no longer retells it. Three sentences that have not changed and are the ones to read first: **what is left in SD-1 is not code — a card flashed and an A500 booted**; what emulation has settled is the **filesystem** side (a PFS3 volume ART formatted, and an AmigaOS 3.2 tree ART built, each boot a licensed Kickstart to a clean Workbench — so `expansion.library` and the real `pfs3aio` binary have accepted ART's disk, which is not provisional), and it does **not** touch the card path, where an MBR, an Amiga disk starting 1.1 GB in and Emu68's `brcm-sdhc.device` are the untested rung ([ART-095](ISSUES.md)); and **no card ART built has been flashed or booted** |
| **Build** | PASS |
| **Tests** | **2626 Rust passed, 0 failed, 42 ignored; 1004 frontend passed across 79 files** (re-measured 2026-09-04; the previous figures, 2624/41, were 2026-08-25). Both run twice, per the standing rule. **Quote the `test result:` line, never the exit code** — on 2026-09-04 an antivirus interfering with `rustup.exe` killed the harness after about seventy tests and the shell still saw exit 0, four runs in a row, with no summary line and no failure (recorded in CLAUDE.md, "Before you commit"). The ignored ones are the real-material hooks, env-gated and run by hand - real install media, a real card, `hst.imager.exe`, Microsoft's `Get-VHD`, the owner's own 1.2 GB `AmiKit.hdf` ([ART-146](ISSUES.md#fixed)), and since 2026-08-24 the three that leave the machine for the real Aminet (`net/live_aminet.rs`). Outside the two suites: **113 contrast pairs** (`scripts/contrast-check.py`, blocking in CI, and since [ART-232](ISSUES.md#fixed) it covers the colours three screens hard-code outside the token system as well as `theme.css`). **The scratch-directory class is closed rather than reduced**: [ART-164](ISSUES.md) fixed `core::iso`, [ART-173](ISSUES.md) fixed `core::cbm` and `core::detect` (4 failures in 40 runs, two different tests, one failing with the *other* test's 1000-byte fixture), and a sweep then took every remaining test scratch helper in the crate - 70 keyed on the process id, then 26 more keyed on `as_nanos()` **alone**, which is the worse shape since two threads can share a nanosecond but not a pid. The first sweep script reported a clean zero while blind to those 26; the widened one (`scripts/scratch-counter-sweep.py`) is the reason that number means anything. As of 2026-09-04 it reports **one** site "needing a counter" and that one is a **false positive**: the helper in `commands/osinstall.rs::staging_is_removed_however_the_preview_ends` keys on the thread id rather than a counter ([ART-182](ISSUES.md#fixed)'s own fix), which is unique within the process and which the sweep does not recognise ([ART-235](ISSUES.md)) |
| **Clippy** | clean at `-D warnings` |
| **TypeScript** | clean |
| **Kickstart table** | 154 dumps, generated from amitools' Remus split database and verified against it on every CI run (`scripts/rom-table-check.py`, ART-104). 24 of the user's own collection now named *with their machine*, including the two 40.68 builds told apart; ART's previous ten hand-listed hashes matched none of them. **Licensed Amiga Forever ROMs are first-class input** (ART-128): decoded with the `rom.key` beside them and then identified like any dump, and refused for a card build when the key is absent — they used to reach the boot partition still encrypted, which no Amiga could start |
| **amitools oracle** | 53 checks, both directions — now including a filesystem driver ART embedded in an RDB and `rdbtool` extracted back out byte-for-byte |
| **7-Zip FAT32 oracle** | the card's boot partition, written by ART and read by 7-Zip: filesystem type, geometry, label, names and every file's bytes (`scripts/fat-oracle-check.py`) |
| **7-Zip disc oracle** | 4 fixtures — Joliet, ISO9660-only, raw Mode 1, raw Mode 2/XA — names, sizes and every file's SHA-256 |
| **hst-imager PFS3 oracle** | both directions, local only (`scripts/pfs3-oracle-check.py`, no `hst.imager.exe` in CI): ART writes a volume through `NativeFormatter` and `hst-imager fs dir -r` reads it back — names, sizes, and every protection-bit string, `hsparwed` cased as `hst-imager` spells it; `hst-imager` formats and fills a volume and ART reads it back through `libpfs3`, SHA-256 per file rather than a length (ART-079's exact shape) plus the same protection strings |
| **cargo-deny** | advisories, bans, licences, sources — all ok |
| **MSRV** | 1.93 (raised from 1.77 on 2026-08-12, for a maintained 7z decoder) |
| **i18n** | `en.json` and `tr.json`, **1916** leaf keys each (counted 2026-09-04; this row said 1834 while the files had grown past it — count them, do not quote this), parity enforced by `pnpm test` |
| **Release bundle** | rebuilt 2026-09-04 for 0.9.0 — `Amiga Retro Toolkit_0.9.0_x64_en-US.msi` and `_x64-setup.exe`, both produced by `pnpm tauri build` in 5m 47s. **Not code-signed**, which the README now says rather than leaving to SmartScreen |
| **Published** | <https://github.com/tolon/Art> — public, `main`, **GPL-3.0-or-later**. **[v0.9.0](https://github.com/tolon/Art/releases/tag/v0.9.0) is released, 2026-09-04**, with both installers attached (NSIS 5.1 MB, MSI 6.3 MB) — built by `release.yml` from the tagged commit, after CI went green on that same commit. The built `.exe` was launched and answered before the draft was published. Work lands on `sd-1` and merges to `main` at the phase's
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
python scripts/catalogue-check.py                      # every shipped bundle path vs Aminet itself (not in CI, it leaves the machine)
python scripts/zoom-check.py                           # the shell's widths, in a real browser (needs `pnpm dev`)
python scripts/osbuilder-strip-check.py                # the OS Builder's step strip: per kind, both languages,
                                                       #   three Application Sizes (needs `pnpm dev`)
python scripts/contrast-check.py                       # every colour pair in both themes, against WCAG (in CI)
python scripts/control-byte-sweep.py                   # no stray BEL/BS/VT/FF/ESC in tracked text
                                                       #   (the heredoc trap; ART-216, in CI)
python scripts/scratch-root-sweep.py                   # every production staging site goes through the chosen
                                                       #   scratch root, not %TEMP% (ART-196; in CI)
python scripts/scratch-counter-sweep.py                # test scratch names unique *within one process*
                                                       #   (ART-059/164/173; not in CI — one known false
                                                       #   positive, ART-235)
python scripts/rom-table-check.py                      # the Kickstart table vs amitools' Remus data
                                                       #   (ART-104; in CI)
python scripts/vhd-oracle-check.py                     # the dynamic VHD writer vs Microsoft's Get-VHD
                                                       #   (needs the Hyper-V PowerShell module; not in CI)

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
# ART-159's two language components, against the disc they were read off.
# `#[ignore]`d; needs the owner's own AmigaOS39.iso and an empty destination.
cd src-tauri && ART_159_ISO="E:\amiga\Amigatolon\iso\AmigaOS39.iso" \
  ART_159_DEST="E:\amiga\ProjeART\art159-tree" \
  cargo test --release build_the_real_39_language_components_when_asked -- --nocapture --ignored

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
chosen language ([ART-060](ISSUES.md#fixed)) — `core/`'s independence rule means
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

**Owed, recorded not fixed:** [ART-078](ISSUES.md#fixed) — Rock Ridge and the
Amiga `AS` System Use entry are not read, so an AmigaOS CD's protection bits
and file comments are lost on the way out. *(Fixed 2026-08-20 on
`debt-wave-b2`; the line is left as it was written to keep this session's own
record honest.)*

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
  half-built: out of a host folder ([ART-080](ISSUES.md#fixed) — ART owns no
  host-side delete), several entries between two images (ART-064), and a
  single *file* between two images ([ART-081](ISSUES.md#fixed) —
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

**Filed rather than built** (the phase's own rule): [ART-080](ISSUES.md#fixed),
[ART-081](ISSUES.md#fixed) (move's missing primitives),
[ART-087](ISSUES.md#fixed) (`Space` does not count a directory) and
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

### 🟡 PiStorm Image Builder — SD-0 … SD-5 (SD-0, SD-1, SD-2 and SD-4 built; SD-3 and SD-5 part-built)

**This was the project's largest unbuilt feature and, per the user, its point.** Read the table below with its own correction in mind: four of the six phases are built, SD-4 among them, and what SD-1 has left is a card flashed rather than a line of code. The remaining two are **part-built** rather than planned as of 2026-08-24 - SD-3's G14 network half and the whole of G16, SD-5's safety half - and what is left of each is named in its own row rather than implied by a tick.

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
| **SD-0** | ✅ **Done 2026-08-12** — prior-art teardown, written up as [sd0-prior-art.md](sd0-prior-art.md). Its one owed exit test (drive `hst-imager` end to end) has since been paid several times over and is now a standing check: `scripts/pfs3-oracle-check.py` runs it **both** directions, and `tools/hst_imager.rs` is the named fallback the product itself reaches for (corrected 2026-09-04 — this row still called it owed) |
| **SD-1** | The image has a shape: MBR + FAT32 boot partition (**G2 — done 2026-08-13/14, engine and screen**), RDB filesystem embedding FSHD/LSEG (**G4 — done**, also closed ART-084), build manifest (**G7 — done 2026-08-15**, with 7-Zip answering the half ART cannot check), image validation (**G8 — done 2026-08-15**), a build as a drop target (**G15 — done 2026-08-15**). **Every gap in SD-1 is built; what is left is a card flashed and booted** |
| **SD-2** | Content, preloaded: PFS3 via `hst-imager` (**G3 route E — done 2026-08-15, engine *and* screen**; route D dropped: E was already proven and needs no Kickstart), OS install engine (**G5 — done 2026-08-16, engine *and* screen**), ROM pairing (**G9 — done 2026-08-17, engine *and* screen**: the preload screen says whether a card's Kickstart suits the volume about to be written, and warns without blocking), launcher metadata export (G10), layout policy (**G11 — done 2026-08-15, engine *and* screen**: a pile of dropped files becomes a staging tree, not yet driven against real material and no staging tree has reached a card) |
| **SD-3** | 🟡 **Mostly built, 2026-08-24.** *It is mine*: **G14's network half is done** - `core/amiganet/` writes `DEVS:tolunnet.config` (merged, because the stack's own GUI writes it too) and `ENVARC:Sys/Wireless.prefs` (replaced, and said so first, with the count of what is already there), asked for on the volumes step while the card is being set up, and the passphrase deliberately **not** remembered ([ART-231](ISSUES.md#fixed) was the drawer it first wrote to). **G16 is done, engine and screen** - `core/card/multiboot.rs` plus a second Amiga disk the card builder can ask for; there is no menu to write, because AmigaOS has one, so what ART decides is `de_BootPri` and it **names a tie rather than resolving it**. Left: the wallpaper (**the owner's own ruling, 2026-08-24: "duvar kağıdını boşver kullanıcı onu kendisi yapar"** - out of scope, not deferred), the rest of prefs (assessed and not worth building - they are binary `IFF PREF` files whose editors are the Amiga's own), and the named ROM+volume pairing G9 deferred into G16 |
| **SD-4** | ✅ **Built, and this row said otherwise until 2026-08-24.** It read *"the flagship: native PFS3 write in ART (G3 route B) — its own brief"*, which stopped being true when `libpfs3` arrived: `core/preload/native.rs` implements `VolumeFormatter` over it and over ART's own FFS writer, it is the **default** with `hst-imager` a named fallback for two typed gaps ([ART-120](ISSUES.md#fixed)), it is checked in both directions by an independent `hst-imager` oracle, and a PFS3 volume ART formatted booted a licensed Kickstart. Route D's harness never became its oracle because route D was dropped and the `hst-imager` one serves. What is left of SD-4 is nothing |
| **SD-5** | 🟡 **Half built, 2026-08-24, and its own note called the whole thing *"comfort"* - half of it was not.** `core/card/capacity.rs` refuses to let ART build an FFS partition past the 4 GB a pre-v46 Kickstart can address, which is a partition that corrupts the drive rather than an inconvenience. Left: the planner proper - the distro registry's entries are all `available: false`, so nothing yet plans a card from a named distribution |

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

*One block, kept current. Every round used to add its own "Start here" and
leave the previous one below it; on 2026-09-04 four such blocks were collapsed
into this one, because the oldest of them still contradicted the newest.
**Update this block — do not stack another on top of it.** A round's own
narrative is one line in the [session log](session-log.md), its reasoning is in
`docs/superpowers/`, and the defect register is [ISSUES.md](ISSUES.md).*

### Where the work stands (verified 2026-09-04)

- **`main` is clean and there is no live phase branch.** Last merge:
  *"Merge docs-ags-assessed: the AGS export, assessed"*.
- **Five open entries**, and none of them is unstarted code:
  [ART-166](ISSUES.md) and [ART-117](ISSUES.md) are standing decisions
  (re-checked 2026-08-25 and 2026-08-16 respectively),
  [ART-118](ISSUES.md) and [ART-062](ISSUES.md) need a person at a screen, and
  [ART-235](ISSUES.md) is a guard reporting a false positive.
  **[ART-226](ISSUES.md#fixed) closed on 2026-09-04** — not by new work but by
  checking it against the tree: all three of its questions had been answered
  by rounds on 2026-08-24 and 08-25, and two of its own sentences had gone
  stale in the meantime.
- **The list to walk, top to bottom**, is
  [superpowers/specs/2026-09-04-work-list.md](superpowers/specs/2026-09-04-work-list.md)
  — seven items, ordered by importance rather than by size, and **the first
  two are the two below**: they used to sit outside the sequence because they
  run in parallel, which was true and made them invisible. Items 3, 5 and 7
  are the build queue in that order; 1, 2, 4 and 6 are the owner's.
- Per-gap state is the stage table above; per-feature state is
  [FEATURES.md](FEATURES.md). Neither is retold here.

### Blocked on a person, not on code

1. **A card flashed and an A500 booted.** Nothing SD-1 or SD-2 has built has
   reached real hardware. The software side is complete; what is missing is
   physical — a microSD card, a USB reader and an HDMI cable, the last plugged
   in **before** power or the VPU never configures the port and there is no
   RTG that session.
2. **[ART-118](ISSUES.md) — the OS Builder's install screen in a real
   window.** Deeper interaction than a smoke test (filling the fields, ticking
   a component, running Plan or Verify) crashes the renderer reproducibly with
   an access violation in every headless Chrome and Edge combination tried.
   This one needs `pnpm tauri dev` and a human, not a substitute.

Neither needs a design decision or more code first.

### The machine, and where things live

- `E:\amiga\Amigatolon` — the owner's own material. **Read from, never
  written to.** Inventoried 2026-08-13 and nothing copyrighted is missing:
  A1200 Kickstarts in both families (3.1 rev 40.68, 3.2's `kicka1200.rom`,
  3.2.1's `A1200.47.102.rom`), the full AmigaOS 3.2 ADF set with its CD and the
  3.2.1/3.2.2 updates, the 3.9 ISO with both BoingBags, every release from 1.0
  to 3.1 as ADFs, and both real PiStorm distributions (CaffeineOS 9317,
  MultibootOS 2.2) as images. WinUAE, 7-Zip and amitools are installed.
- `E:\amiga\ProjeART` — where trial output goes. **Not C:, not D:** (the
  owner's rule, 2026-08-14), and not F: either, which is a 499 MB drive a
  300 MB oracle image will not fit on. `ART_SCRATCH` overrides it in the
  scripts. It holds `card.img` (a card ART built from the real release),
  `dist-3.2\` (the real AmigaOS 3.2 tree `core/osinstall` built from the
  owner's own ADFs — read from, not rebuilt idly, since rebuilding costs the
  same real-media run), and `screen-test.img` + `preload-tree\`, staged for a
  screen run. That card's one Amiga partition is `SDH0`, FFS, 0.90 GiB, in MBR
  slot 2 — so a correct preload plan has **two** steps and no driver-embed
  step, because Kickstart carries FFS. If the plan shows three, something is
  wrong before anything is formatted.
- The test suite's own scratch goes to `D:/tmp/art-tests`, forced by
  `src-tauri/.cargo/config.toml` (ART-184) — machine-local relief, not a fix.
- Rebuilding the card is the fastest way to see the whole engine work at once:

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

### What the owner brings, and what each thing settles

Each of these changed a decision rather than merely being nice to know.

- **The target board is settled by hardware, not preference**: an A500 with a
  classic PiStorm on a Raspberry Pi 3A+, plus a Gotek — which is
  `PistormHardware::default()` already. Consequences that are facts rather
  than parameters: the kernel archive is `Emu68-pistorm.zip` on the stable
  line (**not** the same file as on the 1.1 alpha — ART-091), the SD driver is
  `brcm-sdhc.device`, and `genet.device` is irrelevant (the 3A+ has no
  ethernet; its WiFi is `wifipi.device`).
- **More than one real Amiga.** The rule is unchanged — a claim records
  *which* machine proved it — but there is more than one machine to prove
  things on.
- **Licensed Amiga Forever, desktop and mobile.** There is no Kickstart
  licence problem to design around, and the Cloanto-headered dumps are
  available — which `core/rom` already strips (`AMIROMTYPE1`). It is also why
  [ART-104](ISSUES.md) mattered: a licensed collection carries dumps whose
  checksums are not the ones a table copied from anywhere else will hold.
- **The owner wrote two Amiga-side programs and both are meant to ship in
  ART-built distributions**: **tolunnet**, a TCP/IP stack in Roadshow's place,
  and **tolunwifi**. Source at `D:\Projeler\tolunnet`. A stack the owner
  wrote is theirs to distribute, so ART Baseline can carry one outright rather
  than as a fetch task — which is what SD-0 concluded for anything not clearly
  redistributable. G14's network half is **built** against that program's own
  source (`core/amiganet/`), so nothing was reverse-engineered and Roadshow is
  not assumed as the default stack.

## Session log

Moved to [session-log.md](session-log.md) on 2026-09-04. It was 279 KB of this
file's 353 KB and it describes the past, where the rest of this file describes
the present — and a reader looking for "where is the project" had to scroll
past every round ART has ever had to reach the end.

**Add a row there, at the top, when work lands.** Nothing is duplicated back
here on purpose: two copies of a log is how the Snapshot's own "Last updated"
cell ended up a day behind it.
