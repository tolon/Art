# Testing Strategy

ART is built as production software, not a demo. Every feature gets tests.

## Test layers

| Layer | What | Where | Command |
|-------|------|-------|---------|
| Unit | Core logic (detection, hashing, workflow registry) | `src-tauri/src/**/*.rs` (`#[cfg(test)]`) | `cargo test` |
| Integration | Cross-module flows (ADF → browser, LHA → extract) | `src-tauri/tests/` | `cargo test` |
| Security | Path traversal, malformed input, oversized allocations | `src-tauri/tests/` | `cargo test` |
| Frontend type-check | TS correctness | `src/**` | `pnpm lint` |
| UI workflow | Drag-and-drop, navigation | (manual / future e2e) | manual |

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

## What must be tested (per phase)

### Phase 1 — ADF + LHA
- Valid ADF, invalid ADF, bootable ADF, full ADF, nearly-full ADF.
- OFS vs FFS detection.
- File insertion, extraction, capacity checks.
- Valid LHA, invalid LHA, extraction, **path traversal protection**.
- WHDLoad detection (true positive + true negative).

### Phase 4 — HDF
- Valid HDF, malformed HDF, multi-partition HDF.
- RDB parsing, filesystem detection.
- Resize (expand safe; shrink creates a verified copy).

### Phase 7 — ROM + Binary
- ROM checksum, size, identification.
- Hunk detection, executable vs data.

### Security (always)
- Malicious paths (`../`, absolute, mixed separators).
- Malformed headers (truncated, wild lengths).
- Oversized allocations.

## Test corpus

Tests use **synthetic, legally-clean fixtures** generated in `tempdir()`
during the test run. ART never distributes copyrighted commercial content.

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
pnpm lint                        # TS type-check
cargo fmt --check                # Rust formatting
cargo clippy --all-targets       # Rust lints
cargo test                       # all unit + integration tests
python scripts/oracle-check.py   # amitools oracle, both directions
cargo deny check                 # licence + advisory audit
pnpm tauri build                 # full production build
```
