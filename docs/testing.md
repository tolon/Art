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

Three rules about that scratch, each of them paid for once:

- **Take a `core::ScratchDir`; it removes itself on `Drop`.** A trailing
  `remove_dir_all` is skipped exactly when a test panics, which is when a red
  suite leaks most — 169 291 directories and ~987 GB into `%TEMP%` in one
  session, filling a 2 TB system drive ([ART-184](ISSUES.md#fixed)).
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
python scripts/contrast-check.py --quiet    # every colour pair, both themes, against WCAG
cargo deny check                       # licence + advisory audit
pnpm tauri build                       # full production build
```

Clippy runs with `-D warnings` and is **blocking on purpose**: it previously
ran with `continue-on-error`, and that hid a real correctness bug for months
([ART-019](ISSUES.md#fixed)/[ART-020](ISSUES.md#fixed)). The oracle step is
blocking too — ART's own tests cannot catch a format mistake its reader and
writer both share, which is exactly what caught ART-032…035.

Three oracles run **outside** CI because they need a tool the runner does not
have: `scripts/iso-oracle-check.py` and `scripts/fat-oracle-check.py` need
7-Zip, and `scripts/pfs3-oracle-check.py` needs `hst.imager.exe`. Run them
locally when `core/iso`, `core/fat32` or `core/preload` moves.
