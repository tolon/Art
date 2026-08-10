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
