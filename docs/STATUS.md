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
| **Last updated** | 2026-08-16 |
| **Version** | 0.1.0 (unreleased) |
| **Current stage** | **SD-1 complete** — every gap it names is built: the card's shape (G2), RDB filesystem embedding (G4), build manifest (G7), image health check (G8), a build as a drop target (G15), engine and screen throughout. **What is left in SD-1 is not code: a card flashed and an A500 booted.** **SD-2 in progress** — PFS3 preload via `hst-imager` (G3 route E) and the layout policy (G11) have both landed, engine and screen; OS install (G5), ROM pairing (G9) and launcher metadata export (G10) are owed. Reading a real card has a screen ([ART-095](ISSUES.md), [ART-097](ISSUES.md)); **no card ART built has been flashed or booted** |
| **Build** | PASS |
| **Tests** | 1357 Rust passed, 0 failed; 490 frontend passed, 0 failed |
| **Clippy** | clean at `-D warnings` |
| **TypeScript** | clean |
| **amitools oracle** | 53 checks, both directions — now including a filesystem driver ART embedded in an RDB and `rdbtool` extracted back out byte-for-byte |
| **7-Zip FAT32 oracle** | the card's boot partition, written by ART and read by 7-Zip: filesystem type, geometry, label, names and every file's bytes (`scripts/fat-oracle-check.py`) |
| **7-Zip disc oracle** | 4 fixtures — Joliet, ISO9660-only, raw Mode 1, raw Mode 2/XA — names, sizes and every file's SHA-256 |
| **hst-imager PFS3 oracle** | both directions, local only (`scripts/pfs3-oracle-check.py`, no `hst.imager.exe` in CI): ART writes a volume through `NativeFormatter` and `hst-imager fs dir -r` reads it back — names, sizes, and every protection-bit string, `hsparwed` cased as `hst-imager` spells it; `hst-imager` formats and fills a volume and ART reads it back through `libpfs3`, SHA-256 per file rather than a length (ART-079's exact shape) plus the same protection strings |
| **cargo-deny** | advisories, bans, licences, sources — all ok |
| **MSRV** | 1.93 (raised from 1.77 on 2026-08-12, for a maintained 7z decoder) |
| **i18n** | `en.json` and `tr.json`, 1374 leaf keys each, parity enforced by `pnpm test` |
| **Release bundle** | rebuilt 2026-08-12 — MSI and NSIS, and the application was launched and answered |
| **Published** | <https://github.com/tolon/Art> — public, `main`, **GPL-3.0-or-later**. Work lands on `sd-1` and merges to `main` at the phase's
end; the licence *inventory* still said MIT until 2026-08-13, months after the
licence itself changed |
| **Real hardware** | **Bare metal, reached 2026-08-12** — `test/art-bootable-test.adf` booted a real **A500/A500+** (Kickstart 3.9) from a **Gotek**, to an AmigaDOS CLI. Photographed. Two ADFs had already mounted/booted under licensed Kickstart in WinUAE (Phase 1a). Still untouched: physical magnetic media — a Gotek is not a mechanical drive |
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
| **SD-2** | Content, preloaded: PFS3 via `hst-imager` (**G3 route E — done 2026-08-15, engine *and* screen**; route D dropped: E was already proven and needs no Kickstart), OS install engine (G5), ROM pairing (G9), launcher metadata export (G10), layout policy (**G11 — done 2026-08-15, engine *and* screen**: a pile of dropped files becomes a staging tree, not yet driven against real material and no staging tree has reached a card) |
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

*Last session: 2026-08-15. Nothing is half-done on disk and the working tree is
clean; `sd-1` carries **21 commits ahead of `origin/sd-1`** — the preload screen
and the whole of G11. Not pushed.*

**The tree is green**: 1203 Rust (run twice), 490 frontend across 43 files,
clippy clean at `-D warnings`, TypeScript clean, amitools oracle 53 both ways,
the 7-Zip FAT32 oracle clean on a real card's 21 boot files.

### Where the phases stand

**SD-1 is complete.** Every gap it names is built — G2 (the card's shape,
engine and screen), G4 (RDB filesystem embedding), G7 (build manifest), G8
(image health check), G15 (a build as a drop target). A 64 GB card was built
from the running screen on 2026-08-14 with the user's own material, and 7-Zip
read it back independently. **What is left in SD-1 is not code: flash a card
and boot an A500.** The materials are all here bar a microSD card, a USB reader
and an HDMI cable.

**SD-2 is half built.** Two of its five gaps are done, engine and screen:

- **G3 route E** — `core/preload/` plans and runs a preload, `tools/hst_imager.rs`
  drives `hst-imager`, `core/` still launches nothing (the boundary is a trait),
  and the OS Builder's third kind asks for it. Run for real on 2026-08-15: `DH0`
  formatted as `Work` on a card ART built, a tree copied in, and the tool listed
  it back as 1 directory, 2 files, 20 B with `----RWED` on each.
- **G11** — `core/layout/` and its own `/layout` screen, a peer of the OS Builder
  rather than a page inside it. Design: [content-layout](superpowers/specs/2026-08-15-content-layout-design.md),
  plan: [content-layout](superpowers/plans/2026-08-15-content-layout.md).

**G5, G9 and G10 are owed**, and G5 is the big one.

**What the two screens have and have not been through.** Both were mounted and
driven in a real browser against the real bundle — every string resolved, no raw
key and no `{{variable}}`, and for G11 that browser pass caught a bug the tests
could not (the drawer dropdown leaking a policy field's value). **Neither has
been driven in `pnpm tauri dev` against real material**, and **no staging tree
G11 builds has been carried onto a card**. The preload *engine* has been through
the real tool; the screen in front of it has not. That is the same rung
`test/README.md` keeps for everything else.

### What to pick up

Two of these are the same shape — a built screen that no human has driven — and
doing them together is one session with the application open.

- **Drive both new screens in `pnpm tauri dev` against real material.** For
  preload: `E:\amiga\ProjeART\screen-test.img` (a copy of the built card, made
  for exactly this) with `E:\amiga\Amigatolon\hstimager\hst.imager.exe`, and the
  content tree at `E:\amiga\ProjeART\preload-tree\`. Expect two plan steps —
  format `SDH0` as `Work`, then copy — and **no** driver-embed step, since the
  partition is FFS and Kickstart carries it. For layout: drop a folder of real
  Amiga files on `/layout`, retarget a row, and check the staging tree it
  builds. Afterwards the tool's own `fs dir` is the independent read-back, and
  Settings → Operation Log should carry both runs with `verified: false`.
- **G5 (OS install)** — the largest thing left in SD-2, and now unblocked twice
  over: there is a formatted volume to install onto and a staging tree to fill
  it from. Wants its own design round; extracting AmigaOS 3.2/3.9 media without
  running the Amiga Installer is not a patch.
- **G9 (ROM pairing)** — smaller than it looks. Half of it is already done by
  `CardBuilder` (the ROM is placed on FAT32 under the name the config points
  at); the half that remains is pairing a ROM with an *OS volume*, which wants
  G5 first.
- **G10 (launcher metadata export)** — iGame/AGS metadata onto a GAMES: volume.
  Pairs naturally with G11, which is what puts games there.
- **tolunnet / tolunwifi**: decided as ART Baseline's default network stack and
  WiFi suite, and **not yet coded anywhere**. They belong beside SD-2's package
  manifest, which sits next to G5.
- **Six issues G11 opened and did not fix**: [ART-105 … ART-110](ISSUES.md).
  ART-106 is the one worth reading first — the plan never considers where a
  WHDLoad icon lands, so §82 can fail from the other side.

### Four things this project keeps re-learning

Every one of them cost a wrong turn on 2026-08-15 alone, and every one would
have shipped behind green tests:

1. **Running it against real material beats reading about it.** Identifying a
   ROM by revision looked right until 55 real ROMs showed three different files
   all stating `40.68`. `hst-imager`'s command set looked right until it met a
   card whose RDB is not at byte zero. A log parser looked right until the
   command turned out to print no summary at all.
2. **A screenshot is not a measurement** (ART-099, closed). Ask the thing to
   report its own numbers, and check the instrument before believing the
   picture — a DPI-unaware capture returned two thirds of the window and said
   nothing about it.
3. **A doc comment that promises the opposite of the code is a trap with a
   fuse.** `MbrPartition::index` claimed to be the number everybody writes down
   and was zero-based; it cost a failed run before `slot_number()` existed.
4. **A guarantee that holds is not the same as a goal that works.** G11's
   layout screen set a `stale` flag on every retarget so an unverified collision
   list could never gate Apply — and it held, on every path. What nobody asked
   was whether the user could then *apply*: the only thing that cleared the flag
   was a fresh preview, which rebuilds the plan from policy and throws the edit
   away. The editable preview, which is the entire answer to "ART cannot tell a
   demo from a game", could not be used at all, and `FEATURES.md` already claimed
   it. Seven per-task reviews passed it; the whole-branch review caught it. When
   a rule is added to protect something, the next question is what the rule costs
   the thing it protects.

### Open issues

Twenty entries remain open ([ISSUES.md](ISSUES.md)); three are waiting on a
decision rather than on work. ART-099 and ART-104 closed on 2026-08-15, and
ART-105 … ART-110 opened the same day out of G11's whole-branch review — all six
in code that landed that day, none in anything older.

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
  `card.img`.
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

| 2026-08-16 | **Task 11: an independent witness for the PFS3 writer, both ways.** `libpfs3` is both the crate ART writes PFS3 with and the crate ART reads it back with, so ART's own suite cannot catch a mistake the two would share — the exact shape of ART-032 … ART-035, ART-075 and ART-079. `scripts/pfs3-oracle-check.py` closes that gap with `hst-imager` (C#, no shared code) in both directions: `build_pfs3_volume_for_oracle_when_asked` builds a volume through `NativeFormatter` — the same `format_partition`/`copy_in` G5 uses — and prints a JSON claim of every entry, which `hst-imager fs dir -r` is then checked against, protection bits included (`--P-RWED`, `-S--RWED` — the Pure and Script bits, the reason this phase exists: 3.2's `Startup-Sequence` runs `Resident C:Assign PURE`); `read_foreign_pfs3_for_oracle_when_asked` reads back a volume `hst-imager` itself formatted and filled, printing a **SHA-256 per file** rather than a length, because a length is exactly what ART-079 got right while handing over the wrong bytes. Run for real against `hst-imager 1.6.616`: both directions clean. Deliberately broken twice to confirm the oracle actually discriminates — a protection bit forced to zero was caught (`FAIL C/Assign carries --P-RWED (hst-imager says ----RWED)`), and a single byte flipped in a file ART read back was caught by the hash while the length still matched (`ok Readme is 34 bytes` / `FAIL Readme hashes to what hst-imager was given`), which is the property the second direction exists to prove. Local only, like `fat-oracle-check.py` and `iso-oracle-check.py`: the CI runner has no `hst.imager.exe` | 1357 Rust / 490 frontend |

| Date | Change | Tests |
|---|---|---|
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
