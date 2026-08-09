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
  See `mutation_that_corrupts_the_image_is_not_committed` for the pattern.

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

## CI

GitHub Actions runs on every push (Windows x64):

```bash
pnpm lint          # TS type-check
cargo fmt --check  # Rust formatting
cargo clippy       # Rust lints
cargo test         # all unit + integration tests
pnpm tauri build   # full production build
```
