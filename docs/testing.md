# Testing Strategy

ART is built as production software, not a demo. Every feature gets tests.

## Test layers

| Layer | What | Where | Command |
|-------|------|-------|---------|
| Unit | Core logic (detection, hashing, workflow registry) | `src-tauri/src/**/*.rs` (`#[cfg(test)]`) | `cargo test` |
| Integration | Cross-module flows (ADF → browser, LHA → extract) | `src-tauri/src/commands/*.rs` (`#[cfg(test)]`) | `cargo test` |
| Security | Path traversal, malformed input, oversized allocations | alongside the module under test (`#[cfg(test)]`) | `cargo test` |
| Frontend unit | Pure `src/lib` logic, i18n parity, `Phrase` keys | `src/**/*.test.ts` | `pnpm test` |
| Frontend component | Real components in jsdom | `src/**/*.test.tsx` | `pnpm test` |
| Frontend type-check | TS correctness | `src/**` | `pnpm lint` |
| UI workflow | Drag-and-drop, navigation | (manual / future e2e) | manual |

There is **no `src-tauri/tests/` directory.** Every Rust test in ART is an
inline `#[cfg(test)]` module beside the code it tests, integration and security
cases included — which is why `cargo test` runs the whole suite in one process
and why a test that stages scratch has to name its own thread ([ART-182](ISSUES.md#fixed)).

Frontend tests sit **next to the source** (`src/lib/mask.test.ts`), never in a
`__tests__` directory. Vitest's environment is `node` by default and jsdom
applies **only** to `src/**/*.test.tsx` — a component test written as `.ts`
gets no DOM, which is a confusing failure rather than an obvious one.

## Current coverage

The live test count and build status are recorded in [STATUS.md](STATUS.md);
this file describes the strategy, not the score.

Two categories are mandatory for any change that touches user data:

- **Security**: path traversal, malformed headers, oversized allocations,
  integer overflow in size accounting.
- **Data safety**: a failed or rejected operation must leave the original file
  byte-for-byte unchanged, and there must be a test that asserts exactly that.
  See `a_refused_operation_leaves_the_image_byte_for_byte_unchanged`
  (`commands/volume_write.rs`) for the pattern.

Regression tests for fixed defects are named in [ISSUES.md](ISSUES.md); a fix
without a named test is not considered fixed.

**Run the Rust suite more than once before merging.** `net/`'s test server had a
race that lost roughly one full-suite run in five
([ART-059](ISSUES.md#fixed)); a blocking CI that fails at random trains people
to re-run until green, which is how a real failure gets waved through.

## A test is not a guard until the defect has been put back

A test that has never been seen to fail is a claim, not a guard. **Seventeen
tests in one round passed against the very defect they were written for.** Real
examples from that round:

- a test named for refusing symlinks that was green because the machine cannot
  *create* one;
- a boot-priority test comparing a constant with itself;
- three that asserted only `is_err()` and so passed on a different refusal than
  the one they were named for;
- a refusal test that searched for two substrings separately, where a weaker
  sentence contained both.

**Mutate every guard that matters and report what fell.** Say plainly when one
survives — a disclosed survivor is worth more than a clean table.

### A survivor is one of two things, and they need opposite answers

Either the guard is weak, or the *mutation* was the wrong one for it. Both
happened on **2026-08-24**:

- Extracting an archive into a subdirectory left "unpacking produced no files"
  green — the mutation changed *where*, not *whether*, so the right one (extract
  nothing) was run instead, and it fell. **The mutation was wrong.**
- "Build is disabled when the split is refused" survived removing the refusal
  entirely, because the state was true anyway: no plan existed yet. **The guard
  was wrong**, and it was fixed rather than written down as a survivor.

Ask which before recording it.

### Never assert a state that has more than one cause

"The button is disabled" was true in two separate rounds for a *second* reason —
nothing had been previewed yet — so removing the thing under test left every test
green. Twice in two days: the refused two-system split, and a failed
volume-table proposal silently falling back to the two-field pair, which would
have built a different card than the screen showed.

**Assert the specific sentence.** Where the state itself is the point, first put
the screen in a condition where only the thing under test can produce it.

### A test that reads a table instead of the file is a copy, and copies drift

`scripts/contrast-check.py` measured the two-ring selection indicator out of its
own table. Reverting the source to a single ring changed neither the set of
colours in the file nor the table, so the script went on measuring a ring that
was no longer drawn.

Where the *arrangement* matters and cannot be parsed, **require the literal in
the source**. Same family as `scripts/rom-table-check.py`, which re-derives the
Kickstart table from amitools' Remus data rather than trusting the copy.

### Anything timing-dependent gets an invariant, not a wait

[ART-182](ISSUES.md#fixed) failed three runs in six on one machine and none in
six on another, so "it passes" proved nothing.

Make the clock or the launcher injectable and assert the property the race
violates: `temp_path_at` freezes the clock, and the emulator tests do zero real
waiting.

## Mutating a file safely

A mutation run edits a file that is not yours to lose. Two traps, both paid for:

- **Back the file up by absolute path, and never restore with
  `git checkout -- <file>`.** On 2026-09-07 a mutation on `src/lib/firstboot.ts`
  was "restored" that way while the file carried 123 uncommitted lines, and the
  checkout put it back to `HEAD`. The backup `cp` had already failed silently
  because a `cd` from an earlier command had stuck to the shell. Start from the
  repository root with an absolute `cd`, copy to the scratchpad by absolute
  path, and restore from that copy —
  [lessons.md § Shell traps](lessons.md#shell-traps).
- **Restore with `shutil.copyfile`, not `shutil.move`.** `move` carries the
  backup's *older* modification time, so cargo sees nothing newer than what it
  already built and the next `cargo test` runs the **mutated** binary against
  unmutated source. On 2026-08-24 that showed up as a guard failing on an
  assertion its own source plainly satisfies — a confusing five minutes rather
  than a wrong result, since each mutation was itself compiled from a freshly
  written file. `touch` the file after restoring it, or use `copyfile`.

## What must be tested, per format

The phase numbers this section used to carry ("Phase 1 — ADF + LHA") are the
dead plan [roadmap.md](roadmap.md) says not to resurrect; the questions
themselves still hold, so they are kept by format instead. A ⏳ marks a
question about work that is **not built** — see [FEATURES.md](FEATURES.md) for
what is.

### ADF + LHA
- Valid ADF, invalid ADF, bootable ADF, full ADF, nearly-full ADF.
- OFS vs FFS detection.
- File insertion, extraction, capacity checks.
- Valid LHA, invalid LHA, extraction, **path traversal protection**.
- WHDLoad detection (true positive + true negative).

### HDF + RDB
- Valid HDF, malformed HDF, multi-partition HDF.
- RDB parsing, filesystem detection.
- An embedded filesystem driver read back byte-for-byte, and the `PatchFlags`
  value AmigaOS actually reads ([ART-126](ISSUES.md#fixed)).
- ⏳ Resize (expand safe; shrink creates a verified copy) — not built.

### ROM + Binary
- ROM checksum, size, identification, and `NotChecked` kept apart from
  `Invalid` ([ART-138](ISSUES.md#fixed)).
- Hunk detection, executable vs data.

### Security (always)
- Malicious paths (`../`, absolute, mixed separators).
- Malformed headers (truncated, wild lengths).
- Oversized allocations.

## Test corpus

Tests use **synthetic, legally-clean fixtures** generated in a scratch
directory during the test run. ART never distributes copyrighted commercial
content.

Six rules about that scratch, each of them paid for once:

- **Take a `core::ScratchDir`; it removes itself on `Drop`.** A trailing
  `remove_dir_all` is skipped exactly when a test panics, which is when a red
  suite leaks most — 169 291 directories and ~987 GB into `%TEMP%` in one
  session, filling a 2 TB system drive ([ART-184](ISSUES.md#fixed)).
- **The shape is `ScratchDir::pair`.** A module's local helper is one line —
  `fn scratch(tag: &str) -> (crate::core::ScratchDir, PathBuf) {
  crate::core::ScratchDir::pair("art-<module>", tag) }` — and a call site is
  `let (_guard, dir) = scratch("x")`, which leaves `dir` the plain `PathBuf`
  the body already used. `_guard`, **never `_`**: a bare `_` drops at once and
  is the leak back in one character. Product code that takes a scratch root
  gets that `dir`, never `&std::env::temp_dir()` — its staging lands under
  whatever root it is given, and no sweeper runs in a test. A trailing
  `let _ = remove_dir_all(&dir)` next to a guard is now **redundant**; it goes
  when the file is next touched, not in a churn pass of its own
  ([ART-281](ISSUES.md#fixed)).
- **A guard must outlive every handle inside its directory, and the two
  containers order the opposite way.** Bindings drop right-to-left, so a guard
  returned in a **tuple goes first** (`(ScratchDir, FileRegion)` — otherwise
  the directory is removed while the region is still open). Struct fields drop
  in **declaration order**, so a `_guard: ScratchDir` **field goes last**.
  Both were caught in review during ART-281, one each way round.
- **`scripts/scratch-guard-sweep.py` keeps all three shapes from coming back**
  — a `fn scratch(` returning a bare path, a `scratch(` call bound without its
  guard (`let (_, ` included), and a test that builds a scratch path by hand
  (`temp_dir().join(…)`) or hands `&std::env::temp_dir()` to product code as
  its root. It is blocking in CI beside `scratch-root-sweep.py`.
- **The name must be unique within the process, not just per run.** Cargo runs
  the whole suite in one process, so the pid is shared and `as_nanos()` alone
  can repeat; use a process-wide counter, or the thread id where the test is
  counting directories rather than making one
  ([ART-059](ISSUES.md#fixed)/[ART-164](ISSUES.md#fixed)/[ART-173](ISSUES.md#fixed),
  and [ART-182](ISSUES.md#fixed) for the thread-id case). The sweep is
  `scripts/scratch-counter-sweep.py`.
- **`TMP` is forced off the system drive** by `src-tauri/.cargo/config.toml`.
  That is machine-local relief for the leak, not a fix for it, and it is why a
  fresh checkout on another machine should set the same thing before a long
  run.

Fixture plan (built up per phase):
- valid ADF (blank, formatted)
- bootable ADF (synthetic bootblock)
- full ADF
- OFS / FFS examples
- valid / malformed HDF
- multi-partition HDF
- valid / malformed LHA
- path-traversal archive (rejected on extraction)
- synthetic Amiga executable (Hunk header only)
- WHDLoad structure (slave + exe + data dir)

**The checked-in `test/` directory is the deliberate exception** to the tempdir
rule: synthetic images ART itself wrote, kept because they were carried to real
hardware — `test/art-bootable-test.adf` booted an A500 from a Gotek. Outputs the
user is meant to try go there, not into a git-ignored scratch folder.

## External oracle

ART's own test suite cannot catch a format mistake that its reader and
writer share — a wrong checksum algorithm, a field one longword out of
place, a bitmap laid out backwards. Every one of those round-trips through
ART perfectly and is rejected by real AmigaOS tools; four shipped
(ART-032…035) before anyone checked against an outside implementation.

`scripts/oracle-check.py` checks ART against `amitools` — a separate
implementation with no shared code — in **both directions**:

- ART writes an image → amitools reads it (proves ART's writer).
- amitools writes an image → ART reads it (proves ART's reader).

The fixtures are synthetic, built through `#[test]` hooks that do nothing
unless their environment variable is set. `amitools` is a Python package
(`pip install amitools`), invoked only as an external subprocess by this
script — a dev/CI dependency, never linked into or shipped with ART (see
[licenses.md](licenses.md)). This is a blocking CI step, not optional
tooling.

### The disc reader has its own oracle

`scripts/iso-oracle-check.py` does the same job for ISO9660 that
`oracle-check.py` does for AmigaDOS, against **7-Zip**. The reason is the same
and so is the risk: `core/iso`'s reader and the synthetic ISO builder its
tests run on were written from the same offsets, so they can agree and both be
wrong.

```bash
python scripts/iso-oracle-check.py
```

It builds ART's own fixtures (through the `ART_ISO_*_OUT` hooks), has 7-Zip
list and extract them, and compares names, sizes and the SHA-256 of every
file's bytes.

Two things about it are deliberate:

- **Raw 2352-byte images are checked too**, and that is the half that matters
  most. No host mounts a track dump and 7-Zip will not open one, so the script
  strips the image back to 2048-byte sectors itself, from the layout's
  documented offsets — never from `core::iso`, or it would inherit exactly
  what it is meant to catch ([ART-075](ISSUES.md)).
- **A missing `7z` fails the script.** An oracle that quietly skips is a green
  tick nobody earned.

Only well-formed fixtures reach 7-Zip. The malformed ones — records that loop,
lengths past the end of the file, depth bombs — stay inside `cargo test`
against ART's own reader, where the assertion is about ART refusing them.

This one runs **outside CI**, unlike the amitools oracle: 7-Zip is not on the
runner, and installing it there is a change to make deliberately rather than
in passing. Run it locally when anything under `core/iso` moves.

An earlier plan had a third rung — a real AmigaOS CD read by a real Amiga.
It was cancelled (2026-08-11) because it assumed licensed media reliably to
hand. A volunteer with a real CD32 or AmigaOS disc is still welcome; nothing
claims it has happened.

### The other oracle scripts

None of these is in CI, and none ships a fixture. Run the one whose module you
moved:

| Script | What it checks against | Needs |
|---|---|---|
| `iso-oracle-check.py` | the disc reader vs 7-Zip | `7z` |
| `fat-oracle-check.py` | the card's FAT32 boot partition vs 7-Zip | `7z` |
| `pfs3-oracle-check.py` | PFS3, both directions, vs hst-imager | `hst.imager.exe` |
| `vhd-oracle-check.py` | the dynamic VHD writer vs Microsoft's `Get-VHD` | Hyper-V PowerShell |
| `icon-oracle-check.py DIR` | every `.info` on real install media, round-tripped through `core/amigaicon` | `xdftool` |
| `ilbm-oracle-check.py` | the ByteRun1 encoder vs ffmpeg, and real `.prefs` containers | `ffmpeg` |
| `media-table-check.py DIR` | the install-media hash table vs the owner's own ADFs | real media |
| `catalogue-check.py` | every shipped bundle path vs Aminet itself | the network |
| `zoom-check.py`, `osbuilder-strip-check.py` | the shell's real widths, and the OS Builder's step strip | `pnpm dev` |

Alongside them are the **censuses** — `iso-susp-census.py`,
`lha-header-census.py`, `lha-package-identity.py`, `make-c64-fixture.py` — kept
as scripts so an answer about real material can be re-run rather than
re-trusted.

## Real material and the ignored hooks

**`#[ignore]`d Rust tests are the other half of the outside checks**, and they
are not optional extras: they are where ART meets material nobody here wrote.
`cargo test -- --ignored` lists them. Most take an environment variable naming
real material — `ART_OSINSTALL_DEST`, `ART_CARD_OUT`, `ART_REAL_HARDFILE` — and
run read-only against the owner's own disks. Every command line is in
[STATUS.md](STATUS.md)'s reproduce block.

Three are worth knowing by name, because each proves something ART's own
assertions cannot:

- **`build_the_real_322_tree_when_asked`** (`ART_322_BASE`, `ART_322_UPDATE`,
  `ART_322_ROM`, `ART_322_DEST`) builds AmigaOS 3.2.2 from the owner's own base
  and update media and then **asks the tree what it is** rather than asserting
  it: `Prefs/Env-Archive/Versions/Release` is written by the release itself, so
  the claim comes from a file Hyperion wrote and not from ART's own dropdown.
- **`round_trip_every_icon_in_a_folder_when_asked`** (`ART_ICON_DIR`) is the
  Rust half of the icon oracle; `scripts/icon-oracle-check.py` drives it.
- **`rehearse_the_real_tree_when_asked`** (`ART_FIRSTBOOT_TREE`,
  `ART_FIRSTBOOT_ROM`, `ART_WINUAE`, in `commands/firstboot.rs`) is the one test
  in ART that opens WinUAE against the owner's own material. It copies the tree,
  writes the first boot into the copy, boots it, and asks the Amiga's own
  `S/FirstBoot.log` for the answer — which is how ART-272 and ART-273 were
  found. It never touches the tree it is pointed at, and a failure keeps the
  copy and prints where it is.

Being gated is the point: they need licensed material, so they cannot run in CI
and must never be made to pass by shipping a fixture of somebody's ROM.

## CI

GitHub Actions runs on every push (Windows x64):

```bash
pnpm lint                              # TS type-check (app + test tsconfig)
pnpm test                              # frontend unit tests (Vitest)
cargo fmt --check                      # Rust formatting
cargo clippy --all-targets -- -D warnings   # Rust lints — blocking on purpose
cargo test                             # all Rust tests
python scripts/oracle-check.py         # amitools oracle, both directions
python scripts/rom-table-check.py      # the Kickstart table against amitools' Remus data
python scripts/control-byte-sweep.py   # no stray control bytes in tracked text (ART-216)
python scripts/scratch-root-sweep.py   # every staging site goes through the scratch root (ART-196)
python scripts/scratch-guard-sweep.py  # every test scratch hands out its guard (ART-281)
python scripts/contrast-check.py --quiet    # every colour pair, both themes, against WCAG
cargo deny check                       # licence + advisory audit
pnpm tauri build                       # full production build
```

### Quoting a test run

**A test run is finished when it says `test result:`, not when it exits 0.**

On **2026-09-04** `cargo test` printed *"running 2668 tests"*, then about
seventy `... ok` lines, then stopped — **no summary line, no failure, exit code
0** — four times in a row, and identically whether the output went to a pipe or
straight to a file. The cause was outside the repository: the machine's
antivirus was interfering with `rustup.exe`, so the harness was being killed
mid-run and the shell saw a clean exit. Run again with the interference gone and
the suite reported **2626 passed, 0 failed, 42 ignored** in 32.65 s.

For most of 2026-09 the owner's machine could not run the full suite at all:
`cargo test --lib -- --skip artwork` was the only honest Rust number until the
antivirus exclusions landed on 2026-09-07 and the full suite printed its
summary twice (3062 passed; [ART-261](ISSUES.md#fixed)). The lesson outlived
the defect: a suite that stops without a `test result:` line has not run.

**Quote the `test result:` line, never the exit code.** An exit code cannot tell
a finished suite from a killed one, and "green" is exactly what a truncated run
looks like. The current numbers, and the command that produced them, are in
[STATUS.md](STATUS.md)'s Snapshot.

Clippy runs with `-D warnings` and is **blocking on purpose**: it previously
ran with `continue-on-error`, and that hid a real correctness bug for months
([ART-019](ISSUES.md#fixed)/[ART-020](ISSUES.md#fixed)). The oracle step is
blocking too — ART's own tests cannot catch a format mistake its reader and
writer both share, which is exactly what caught ART-032…035.

Three oracles run **outside** CI because they need a tool the runner does not
have: `scripts/iso-oracle-check.py` and `scripts/fat-oracle-check.py` need
7-Zip, and `scripts/pfs3-oracle-check.py` needs `hst.imager.exe`. Run them
locally when `core/iso`, `core/fat32` or `core/preload` moves.
