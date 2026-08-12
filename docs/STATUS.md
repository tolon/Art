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
| **Last updated** | 2026-08-12 |
| **Version** | 0.1.0 (unreleased) |
| **Current stage** | **SD-1 in progress** — G4 (RDB filesystem embedding) done both ways; G2, G7, G15, G8 owed |
| **Build** | PASS |
| **Tests** | 1057 Rust passed, 0 failed; 401 frontend passed, 0 failed |
| **Clippy** | clean at `-D warnings` |
| **TypeScript** | clean |
| **amitools oracle** | 53 checks, both directions — now including a filesystem driver ART embedded in an RDB and `rdbtool` extracted back out byte-for-byte |
| **7-Zip disc oracle** | 4 fixtures — Joliet, ISO9660-only, raw Mode 1, raw Mode 2/XA — names, sizes and every file's SHA-256 |
| **cargo-deny** | advisories, bans, licences, sources — all ok |
| **MSRV** | 1.93 (raised from 1.77 on 2026-08-12, for a maintained 7z decoder) |
| **i18n** | `en.json` and `tr.json`, 1119 leaf keys each, parity enforced by `pnpm test` |
| **Release bundle** | rebuilt 2026-08-12 — MSI and NSIS, and the application was launched and answered |
| **Published** | <https://github.com/tolon/Art> — public, `main`, **GPL-3.0-or-later** (was MIT until 2026-08-12) |
| **Real hardware** | **Bare metal, reached 2026-08-12** — `test/art-bootable-test.adf` booted a real **A500/A500+** (Kickstart 3.9) from a **Gotek**, to an AmigaDOS CLI. Photographed. Two ADFs had already mounted/booted under licensed Kickstart in WinUAE (Phase 1a). Still untouched: physical magnetic media — a Gotek is not a mechanical drive |
| **Seen on a screen** | **The Files screen, at last** — opened, driven and screenshotted on 2026-08-12, along with the ADF, hard-disk, PiStorm, WHDLoad and Settings screens. It cost immediately: [ART-082](ISSUES.md#open) (the listings were capped at 420 px inside panes that filled the window) had shipped green behind 178 tests. Four more findings came out of the same session — ART-083 … ART-086. `ART-062` narrows to the screens still unopened (Aminet, Collection, Gotek, ROM, WinUAE, Tools) |

Reproduce the numbers above:

```bash
pnpm lint                                              # TypeScript
pnpm test                                              # frontend unit tests (i18n parity, phrase keys)
cd src-tauri && cargo fmt --check                      # formatting
cd src-tauri && cargo clippy --all-targets -- -D warnings
cd src-tauri && cargo test                             # unit + integration (twice — ART-059)
pip install amitools && python scripts/oracle-check.py # independent cross-check
python scripts/iso-oracle-check.py                     # the disc reader vs 7-Zip (needs 7z; not in CI)
cd src-tauri && cargo deny check                       # licences and advisories
pnpm tauri build                                       # full bundle (slow)
```

A claim in this file is only valid if the command that proves it was actually
run. Do not carry a PASS forward on faith.

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
([ART-061](ISSUES.md#open)). And no language has been checked on a running
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
  `…	estrt-bootable-test.adf`, volume row `Work FFS 877 k of 880 k free` —
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
[ART-088](ISSUES.md#open) (the writer ignores the `d` protection bit; the file
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
  which is the same shape as [ART-043](ISSUES.md#open). One coherent fix, with
  tests at both offsets.
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
the card — not a code path. Two things are still the user's to settle before
SD-1 designs a layout: which PiStorm board(s) to target first, and which
machine the first card gets verified on (recorded with the result, not
generalised from it).

| Phase | Contents |
|---|---|
| **SD-0** | ✅ **Done 2026-08-12** — prior-art teardown, written up as [sd0-prior-art.md](sd0-prior-art.md). One exit test still owed (drive `hst-imager` end to end on a scratch image) |
| **SD-1** | The image has a shape: MBR + FAT32 boot partition (G2), RDB filesystem embedding FSHD/LSEG (G4 — also closes ART-084), build manifest (G7), a build as a drop target (G15), image validation (G8) |
| **SD-2** | Content, preloaded: PFS3 via a scripted WinUAE session (G3 route D), OS install engine (G5), ROM pairing (G9), launcher metadata export (G10), layout policy (G11) |
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

`core/sources` is built and tested (115 tests, all driven by an in-memory
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

1. Run the verification block above and confirm the snapshot still holds.
   The oracle is part of it: it is the only check that can catch ART's reader
   and writer being wrong in the same way.
2. Read this file, then [ISSUES.md](ISSUES.md#open) — open defects are
   `ART-043` (the whole-file strategy ignores a partition's offset),
   `ART-046` … `ART-050` (recorded findings, none fixed yet), `ART-060` …
   `ART-062` (Phase 0b's own: Rust error strings still English, `formatAge`'s
   English pluralisation, no language checked on screen), and `ART-064` …
   `ART-068` (Phase 1a's own: volume→volume and volume→local multi-select
   don't share the other two directions' atomicity, `archives_plan_install`
   blocks the command thread, a batch install's Stop is unresponsive
   mid-archive, the filter's empty-vs-no-match message is inferred rather
   than carried), and `ART-080`/`ART-081` (Phase 2b task 3's own: there is no
   host-side delete, so nothing can be moved *off* a folder, and no primitive
   that moves a single file between two images); the next new defect starts at
   `ART-082`.
3. **The hardware rung is climbed to the bottom, and it is a photograph now.**
   `test/art-bootable-test.adf` booted a real **A500/A500+** (Kickstart 3.9)
   off a **Gotek** to an AmigaDOS `1>` prompt on 2026-08-12 — real silicon, not
   an emulator. `test/task-10-boot-test.adf` mounted, listed and read back
   correctly under licensed Kickstart/Workbench (Amiga Forever / WinUAE) in
   Phase 1a. What is left: **physical magnetic media** — a Gotek is not a
   mechanical drive, and nothing ART wrote has been through a real floppy head.
   All of that is a different claim from "the running app has been looked at":
   apart from the Dashboard, no screen ART shipped in phases 1a, 2a or 2b has
   actually been opened by a person — still `ART-062`.
4. **Stage R and Stage W are both done.** Aminet Stage A is complete too,
   including the update view and install to HDF. **Phase 1a is done too** —
   the two-pane manager has real focus, multi-select, batch copy/delete,
   sorting, a filename mask, and now boot code that works; see "Phase 1a"
   above for what is not (volume→volume batching, ART-064/065).
5. **Start here tomorrow: SD-1**, the PiStorm image builder's first slice —
   MBR + FAT32 in an image file (G2), the RDB at a byte offset with FSHD/LSEG
   embedded (G4, which also closes ART-084), the build manifest (G7), a build
   as a drag & drop target (G15) and image validation (G8). SD-0 is done and
   its findings are in [sd0-prior-art.md](sd0-prior-art.md) — read the
   supersession table at its end before designing, and settle its one exit
   test (drive `hst-imager` end to end on a scratch image) first.

   **Phase 2b is complete and merged.** Two of its twelve acceptance points
   were not verified — the light theme was never opened, and session restore
   has never been seen working — and both are worth ten minutes with the
   application before they are forgotten.
6. **Phase 2a is complete** and merged to `main`
   — see its section above for what it changed and what it left owed, and its
   plan file's Amendments section for what changed after the plan was written
   (the real-Amiga-CD step is cancelled; a 7-Zip oracle replaced it).
6. **The SD Card Appliance Builder is planned but not started** —
   [sd-appliance-gap-analysis.md](sd-appliance-gap-analysis.md), phases
   SD-0 … SD-5. It is sequenced after Phase 2a; its order relative to the AI
   layer is still open.
7. The named work left in the briefs:
   - **§45.5 AI Workflow Layer.** Designed
     ([design-ai-layer.md](design-ai-layer.md)), not built. Its prerequisites
     (operation log with `origin: ai-plan`, a tested workflow catalogue) are
     all in place.
   - **Dircache write support**, if it is wanted. Deliberately refused today;
     doing it properly means maintaining the cache blocks in the same
     journalled operation as the directory, which is real work rather than a
     flag.

Things deliberately left undone, so they are not mistaken for oversights:

- **No preview step** before destructive operations (§92) *except* copying into
  a volume, which now plans first and shows the cost, the name problems and the
  collisions before writing anything. Delete and overwrite still tell the user
  what happened rather than what is about to.
- **`pnpm tauri build`** succeeds; MSI and NSIS bundles were produced on
  2026-08-09. The bundles have not been installed and run from a clean machine.
- **i18n covers the interface, not the core.** Every screen, shared component
  and `src/lib` helper goes through `t()` in English and Turkish. `CoreError`
  and `WhdloadRefusal` sentences, written in Rust, still reach the UI in
  English only regardless of the chosen language ([ART-060](ISSUES.md#open)) —
  not a gap left to fill screen by screen, a design question about whether
  `core/` gets its own catalogue or the frontend keys off `CoreError::code()`.
- `docs/roadmap.md` phase marks remain the original Phase 0 plan by design —
  this file is the live position.

## Session log

Newest first. One line per session that changed what works.

| Date | Change | Tests |
|---|---|---|
| 2026-08-13 | **ART can read a real PiStorm card — the thing that blocked SD-2a.** Two real distributions arrived (CaffeineOS 9317, MultibootOS 2.2) and reading them found that ART could not open either: `find_rdb_location` looked in the first 16 blocks of the file, and on every card those are the MBR and the FAT32 partition, with the Amiga's RDB about 1.1 GB in ([ART-095](ISSUES.md#open)). `core/mbr.rs` + `core/card.rs` fix it, and a card is a **list** of Amiga disks: MultibootOS has two, with different geometries, and its second RDB carries no PFS3 while all fifteen of its partitions are PFS3 — so drivers are the card's, not the area's, or ART would name fifteen working partitions as broken ([ART-097](ISSUES.md#open)). Both verified against the real files. Also filed: [ART-096](ISSUES.md#open), ART writes `MaxTransfer` and `Mask` as zero where every partition on both cards uses `0x1FE00` / `0x7FFFFFFE`. The layout is written up in [sd2-card-layout.md](sd2-card-layout.md) | 1057 Rust / 401 frontend |
| 2026-08-13 | **The two things left owed from the previous rounds.** ART-094: the `w` bit is checked before an overwrite, at all three paths — and it caught a side effect of the fix that created it, since ART-088 had made every overwrite path ask the *deletion* question by going through `delete`. Two bits, two guards, `a_delete_protected_file_may_still_be_overwritten` keeping them apart. ART-092: a named firmware set can be deleted, backed up first so it stays recoverable, and never the one the card is currently running. Still owed and named: the copy dialog's own write-protection question, and [ART-093](ISSUES.md#open) (fetching a kernel) | 1038 Rust / 401 frontend |
| 2026-08-13 | **Four from the open list, while the card is being prepared.** ART-088: the *writer* honours the delete-protection bit now, not just a dialog — `delete` refuses and names the entry, `delete_with(.., Override)` is the way past it, and the Files screen sends the answer only where it has shown the question. Move asks before the copy half rather than after, so a refusal cannot leave a duplicate behind. ART-072: `Docs` and `docs` are one drawer on an Amiga, so both collision checks compare without case — the clean refusal fires where it used to become a pile of unexplained skips. ART-071: a selection of nothing but shortcuts said it had copied everything; the report carries the declined roots now. ART-061: "1 weeks ago" — fixed with `_one`/`_other`, and with `count`, which is the half that makes i18next actually pluralise. The `w`-bit question is split out as ART-094 rather than left inside ART-088 | 1031 Rust / 401 frontend |
| 2026-08-13 | **The OS Builder knows the distributions.** `core/distro/` is a registry of real AmigaOS distros as data — CaffeineOS, CoffinOS, AmiKit, two ART Baseline entries — with the licence model each one obliges the user to, the Kickstart family its base wants, and the card it needs. The `/os-builder` screen leads with the licence, checks the ROM family and the card size, and says plainly that ART cannot write a card yet: the adapter is blocked on reading a real distribution's layout by hand (research §8.2) rather than guessing at it. Two open questions closed with evidence in [sd2-distro-decisions.md](sd2-distro-decisions.md) — the **free Aminet Picasso96 is enough** (so ART Baseline is reproducible without a paid component), and the HstWB package format, whose `Install` turns out to be 26 KB of Amiga script only an Amiga can run | 1024 Rust / 400 frontend |
| 2026-08-13 | **PiStorm fix round.** The Kickstart goes through ROM Manager now — every ROM on a card identified by checksum and labelled with its version and machines, one pickable from anywhere on the PC and copied on under a confirmed name; unrecognised stays a label, never a refusal. The kernel states its version, read from the `$VER:` string Emu68's own build compiles in. Named firmware sets can be created, duplicated, renamed and activated, each through preview to backup to write. **[ART-091](ISSUES.md#open) found in review**: ART named `Emu68-pistorm16.zip`, which no Emu68 release has ever shipped — and the name that does exist, `Emu68-pistorm.zip`, means the *classic* board in 1.0.x and the PiStorm32-lite/PiStorm16 in 1.1 alpha. The release line is now a field and the answer a type with `Absent` and `Unstated` cases. Owed and named: [ART-093](ISSUES.md#open) (fetching a kernel update) and [ART-092](ISSUES.md#open) (deleting a set) | 1013 Rust / 379 frontend |
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
