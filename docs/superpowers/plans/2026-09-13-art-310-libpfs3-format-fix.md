# ART-310 — `libpfs3`'s format fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ART formats PFS3 the way pfs3aio does. In SUPERINDEX mode the format writes the super index
(`SB`) level, and at every size it marks anodes 0–4 reserved. It does this through a vendored, patched
`libpfs3` (`0.1.3+art.1`), proved by ART's own tests and by the hst-imager oracle at both sizes.

**Architecture:**
- `libpfs3` 0.1.3's `src/` goes into `src-tauri/vendor/libpfs3` unchanged. `[patch.crates-io]` points the
  build at it, and ART's version constant, `probe()` and the pin test move to `0.1.3+art.1` (Task 1).
- Only `src/format.rs` is patched. Three new ART tests read the image's own blocks and are seen red on
  0.1.3 first (Task 2).
- `scripts/pfs3-oracle-check.py` gains a large-mode direction and an anode check (Task 3).
- The documents close ART-310 and file ART-311 (Task 4).
- The same patch is prepared for upstream outside the repository, for the owner to open (Task 5).

**Tech Stack:**
- Rust: ART's MSRV is 1.93. This machine has rustc 1.95.0 through rustup.
- `libpfs3` 0.1.3 (edition 2024)
- Python 3 (the oracle)
- hst-imager 1.6.616 (linux-x64 console)
- cargo-deny 0.20.2

**Spec:** `docs/superpowers/specs/2026-09-13-art-310-libpfs3-format-fix-design.md`.
**Research:** `docs/superpowers/notes/2026-09-11-libpfs3-format-fix-research.md`.

## Global Constraints

**The repository and the machine**
- The repository is `/home/tolon/Belgeler/Projeler/Art`, on branch `art-310-libpfs3-format` (base
  `4b0950b`). Nothing is pushed or merged: both are the owner's word.
- The machine is **CachyOS Linux** (Arch). `sudo` needs a password, and it is never used by an agent. A
  package that needs root is handed to the owner as a line to type:
  `! sudo pacman -S <package>`.
- **ART builds for `x86_64-pc-windows-msvc` only.** On Linux, `cargo test --lib` does not compile,
  because two test-only helpers use `std::os::windows`:
  - `commands/osinstall.rs`, the test `sweep_stale_preview_scratch_dirs_removes_only_old_directories_under_its_own_prefix`
  - `core/appearance/mod.rs`, the test `a_failed_arrange_leaves_every_icon_byte_for_byte_unchanged`

  Every Rust test, clippy and oracle command on this machine therefore goes through
  **`~/Belgeler/Projeler/art-experiments/art-linux-run.sh`** (written in Task 1, Step 1). It gates those
  two tests with `#[cfg(windows)]` for the run and restores both files afterwards. **Never stage or commit
  either file.**
- **The full `cargo test --lib` of record runs on the owner's Windows machine**, twice, with `TMP` and
  `TEMP` on `E:\amiga\ProjeART\build\tmp`.

**The vendored crate**
- Vendored path: `src-tauri/vendor/libpfs3`. Version `0.1.3+art.1`.
- It carries only `src/`, `Cargo.toml`, `README.md`, `LICENSE`, `COPYING.LESSER` and `ART-PATCH.md`.
  **Never `tests/`.**
- Original: `https://static.crates.io/crates/libpfs3/libpfs3-0.1.3.crate`, SHA-256
  `02f457ef99a09ddebf56e454c6a25dc3a6860a602c878489f132a4ca3eed4317`.
- Upstream commit: `metaneutrons/pfs3` `33e9ff6ba8462cc4e434dfb6e2783d91b7dd5b14`, path `crates/libpfs3`.
- `src-tauri/Cargo.toml` keeps `libpfs3 = "=0.1.3"` unchanged, and adds:
  `[patch.crates-io]` / `libpfs3 = { path = "vendor/libpfs3" }`.

**The patch** (`src/format.rs` only)
- **SB**, in SUPERINDEX mode only:
  - allocated after the bitmap index blocks and before the anode index block;
  - `id = SBLKID (0x5342)`, `datestamp = 1`, `seqnr = 0`, `index[0]` = the anode index block;
  - `rext.superindex[0]` names it.
- **Anodes 0–4** of the first anode block are `clustersize = 0, blocknr = 0xFFFF_FFFF, next = 0`.
- **Nothing else changes:** not reserved sizing, not option flags, not datestamps, not `enable_deldir`.

**Numbers and tools**
- MAXSMALLDISK = **10 241 440** blocks. The large-mode test partition is **5 100 MiB** on a **5 200 MiB**
  disk: 10 362 cylinders × 1 008 = **10 444 896** blocks.
- Tools:
  - hst-imager: `~/.local/share/art-tools/hst-imager-1.6.616/hst.imager`
  - pfs3aio 3.1: `~/.local/share/art-tools/pfs3aio-3.1/pfs3aio`, SHA-256 `185fae785d0f6e15a1936ecb44e4cb18a21b6f701094c57edbd331e5f9a16047`
  - `cargo deny`: installed at user level
- Scratch for Linux runs: `/home/tolon/Belgeler/Projeler/art-experiments/tmp`, outside the repository.

**How work is done**
- `src-tauri/src/core/` stays platform-independent. No new dependency.
- **Every new test is seen red** against 0.1.3's format before the patch makes it green. **Every guard is
  mutated** and seen failing. The red messages and each mutation's result go into ART-310's Fixed entry.
- Never pipe a command whose exit status matters. A test run is finished only when it prints
  `test result:`.
- Commit messages go through `git commit -F -` from a quoted heredoc. Stage files by name, never
  `git add -A`. End every ART commit message with:
  `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`.
- **Upstream (Task 5):** no AI attribution trailers in commits or the PR body (`metaneutrons/pfs3`
  CONTRIBUTING).

## File structure

| File | Responsibility |
|---|---|
| `src-tauri/vendor/libpfs3/**` (create) | the vendored crate; `src/format.rs` is the only file that differs from 0.1.3 |
| `src-tauri/vendor/libpfs3/ART-PATCH.md` (create) | provenance, what was carried and not, the diff against 0.1.3, upstream status |
| `src-tauri/Cargo.toml` (modify) | `[patch.crates-io]`; the comment above the `libpfs3` pin |
| `src-tauri/Cargo.lock` (modify) | `libpfs3 0.1.3+art.1`, a path source |
| `src-tauri/src/core/preload/native.rs` (modify) | `LIBPFS3_VERSION`; the pin and probe tests; the raw-block helpers and three ART-310 tests; the oracle hooks; the module doc |
| `scripts/pfs3-oracle-check.py` (modify) | the large-mode write direction, the anode check, a path separator for Linux |
| `src-tauri/src/core/card/sizing.rs` (modify) | two doc comments that describe 0.1.3's broken format as current |
| `docs/ISSUES.md`, `CHANGELOG.md`, `docs/STATUS.md`, `docs/session-log.md`, `docs/licenses.md`, `THIRD_PARTY_LICENSES.md`, `deny.toml` (modify) | closing ART-310, filing ART-311, the vendored licence |
| `~/Belgeler/Projeler/art-experiments/art-linux-run.sh` (create, outside the repo) | the Linux gate |
| `~/Belgeler/Projeler/art-experiments/pfs3-upstream/` (create, outside the repo) | the upstream branch for the owner's PR |

---

### Task 1: Vendor `libpfs3` 0.1.3 unchanged as `0.1.3+art.1`

The deliverable: the build uses the vendored copy, nothing on disk changes yet, and the version ART reports
names the copy.

**Files:**
- Create: `~/Belgeler/Projeler/art-experiments/art-linux-run.sh`
- Create: `src-tauri/vendor/libpfs3/{Cargo.toml,README.md,LICENSE,COPYING.LESSER,ART-PATCH.md}`, `src-tauri/vendor/libpfs3/src/**`
- Modify: `src-tauri/Cargo.toml:157-172` (the comment above `libpfs3 = "=0.1.3"`), and a new section before `[dev-dependencies]` (`:211`)
- Modify: `src-tauri/Cargo.lock`
- Modify: `src-tauri/src/core/preload/native.rs:113-121` (`LIBPFS3_VERSION`), `:1747-1762` (the pin test), `:1978-1985` (`probe_names_libpfs3`)

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces:
  - `const LIBPFS3_VERSION: &str = "0.1.3+art.1"` in `core::preload::native`;
  - the Linux gate script, used by every later task as `bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh <command…>`, which runs `<command…>` from the repository root with the gate applied and `TMP`/`TMPDIR` set;
  - a pristine copy of 0.1.3 at `~/Belgeler/Projeler/art-experiments/libpfs3-0.1.3-pristine/libpfs3-0.1.3/`, used by Task 2 for the diff.

- [ ] **Step 1: Write the Linux gate script**

Create `/home/tolon/Belgeler/Projeler/art-experiments/art-linux-run.sh`:

```bash
#!/usr/bin/env bash
# Runs a command in ART's repository on Linux. ART builds for Windows only, and
# two test-only helpers (std::os::windows) stop `cargo test --lib` compiling
# here; they are gated with #[cfg(windows)] for the run and restored afterwards.
# The gate is NEVER committed. Usage: art-linux-run.sh <command...>
set -u
REPO=/home/tolon/Belgeler/Projeler/Art
SCRATCH=/home/tolon/Belgeler/Projeler/art-experiments/tmp
GATED=(src-tauri/src/commands/osinstall.rs src-tauri/src/core/appearance/mod.rs)
cd "$REPO" || exit 2
for f in "${GATED[@]}"; do
  if ! git diff --quiet -- "$f"; then
    echo "refusing: $f already has uncommitted changes" >&2
    exit 2
  fi
done
python3 - <<'EOF'
for path, fn in [
    ("src-tauri/src/commands/osinstall.rs",
     "fn sweep_stale_preview_scratch_dirs_removes_only_old_directories_under_its_own_prefix"),
    ("src-tauri/src/core/appearance/mod.rs",
     "fn a_failed_arrange_leaves_every_icon_byte_for_byte_unchanged"),
]:
    lines = open(path, encoding="utf-8").read().split("\n")
    i = next(k for k, l in enumerate(lines) if fn in l)
    j = max(k for k in range(i) if lines[k].strip() == "#[test]")
    lines.insert(j, "    #[cfg(windows)] // ART LINUX RUN ONLY - never committed")
    open(path, "w", encoding="utf-8").write("\n".join(lines))
EOF
mkdir -p "$SCRATCH"
TMP="$SCRATCH" TMPDIR="$SCRATCH" "$@"
status=$?
git checkout -- "${GATED[@]}"
exit $status
```

Run: `chmod +x ~/Belgeler/Projeler/art-experiments/art-linux-run.sh && bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh bash -c 'cd src-tauri && cargo test --lib core::preload::native'`
Expected: `test result: ok. 37 passed; 0 failed; 1 ignored`. Afterwards `git status --short` shows neither gated file.

- [ ] **Step 2: Write the failing tests — the pin names the vendored copy, `probe()` says `+art.1`**

In `src-tauri/src/core/preload/native.rs`, replace the whole test from the comment line
`// ---- fix round 1, item 2: the version pin cannot silently drift ----` through the end of
`fn the_pinned_version_constant_matches_cargo_toml` with:

```rust
    // ---- fix round 1, item 2: the version pin cannot silently drift ----
    // ---- ART-310: and it names the vendored, patched copy ----

    #[test]
    fn the_pinned_version_constant_matches_cargo_toml() {
        let cargo_toml = include_str!("../../../Cargo.toml");
        assert!(
            cargo_toml.contains("libpfs3 = \"=0.1.3\""),
            "Cargo.toml must still pin the crates.io release the vendored copy was taken from"
        );
        // Two separate checks, not one string with a newline in it: a Windows
        // checkout may carry CRLF.
        assert!(
            cargo_toml.contains("[patch.crates-io]")
                && cargo_toml.contains("libpfs3 = { path = \"vendor/libpfs3\" }"),
            "Cargo.toml must patch libpfs3 to the vendored copy (ART-310) — without it the \
             build silently goes back to 0.1.3's broken format"
        );
        let vendored = include_str!("../../../vendor/libpfs3/Cargo.toml");
        let expected = format!("version = \"{LIBPFS3_VERSION}\"");
        assert!(
            vendored.contains(&expected),
            "the vendored libpfs3's version no longer matches LIBPFS3_VERSION \
             ({LIBPFS3_VERSION}) — update the constant (and what probe() claims) together \
             with the vendored copy"
        );
    }
```

Replace the body of `fn probe_names_libpfs3` with:

```rust
    #[test]
    fn probe_names_libpfs3() {
        let probed = NativeFormatter.probe().unwrap();
        assert_eq!(probed.raw, "libpfs3 0.1.3+art.1 (native, no external tool)");
    }
```

- [ ] **Step 3: Run the tests to see them fail**

Run: `bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh bash -c 'cd src-tauri && cargo test --lib core::preload::native'`
Expected: a compile error `couldn't read` `…/src-tauri/src/core/preload/../../../vendor/libpfs3/Cargo.toml`. Record the line.

- [ ] **Step 4: Fetch, verify and copy 0.1.3**

```bash
W=/home/tolon/Belgeler/Projeler/art-experiments/libpfs3-0.1.3-pristine
V=/home/tolon/Belgeler/Projeler/Art/src-tauri/vendor/libpfs3
mkdir -p "$W" && cd "$W"
curl -sSfL -o libpfs3-0.1.3.crate https://static.crates.io/crates/libpfs3/libpfs3-0.1.3.crate
echo "02f457ef99a09ddebf56e454c6a25dc3a6860a602c878489f132a4ca3eed4317  libpfs3-0.1.3.crate" > crate.sha256
sha256sum -c crate.sha256
tar xzf libpfs3-0.1.3.crate
mkdir -p "$V"
cp -r libpfs3-0.1.3/src libpfs3-0.1.3/README.md "$V"/
gh api 'repos/metaneutrons/pfs3/contents/LICENSE?ref=33e9ff6ba8462cc4e434dfb6e2783d91b7dd5b14' --jq '.content' > license.b64
base64 -d license.b64 > "$V/LICENSE"
curl -sSfL -o "$V/COPYING.LESSER" https://www.gnu.org/licenses/lgpl-3.0.txt
```

Expected:
- `sha256sum -c` prints `libpfs3-0.1.3.crate: OK`.
- `head -5 "$V/LICENSE"` shows `This project is licensed under the GNU Lesser General Public License v3.0 or later.`
- `head -3 "$V/COPYING.LESSER"` shows `GNU LESSER GENERAL PUBLIC LICENSE` and `Version 3, 29 June 2007`.
- `diff -r libpfs3-0.1.3/src "$V/src"` prints nothing.

- [ ] **Step 5: Write the vendored manifest and `ART-PATCH.md`**

Create `src-tauri/vendor/libpfs3/Cargo.toml` (from 0.1.3's `Cargo.toml.orig`, the version changed and
`[dev-dependencies]` removed):

```toml
# Vendored by ART from libpfs3 0.1.3 (crates.io) for ART-310 — see ART-PATCH.md.
# The build metadata in the version (+art.1) is what Cargo.lock and ART's
# probe() report, so nothing claims the unpatched release was built.
[package]
name = "libpfs3"
version = "0.1.3+art.1"
edition = "2024"
license = "LGPL-3.0-or-later"
description = "Pure Rust PFS3 (Amiga) filesystem library — read, write, format, and check"
readme = "README.md"
repository = "https://github.com/metaneutrons/pfs3"
keywords = ["amiga", "pfs3", "filesystem", "retro", "fuse"]
categories = ["filesystem", "parser-implementations"]

[dependencies]
byteorder = "1"
thiserror = "2"
```

Create `src-tauri/vendor/libpfs3/ART-PATCH.md`:

```markdown
# `libpfs3` 0.1.3+art.1 — ART's vendored copy

This directory is `libpfs3` 0.1.3 as published on crates.io, vendored into ART for
[ART-310](../../../docs/ISSUES.md). ART's build uses it through `[patch.crates-io]` in
`src-tauri/Cargo.toml`; the `libpfs3 = "=0.1.3"` pin there names the release it was taken from.

| | |
|---|---|
| Original | `https://static.crates.io/crates/libpfs3/libpfs3-0.1.3.crate`, SHA-256 `02f457ef99a09ddebf56e454c6a25dc3a6860a602c878489f132a4ca3eed4317` |
| Upstream source | `metaneutrons/pfs3` commit `33e9ff6ba8462cc4e434dfb6e2783d91b7dd5b14`, `crates/libpfs3` (the crate's `.cargo_vcs_info.json`) |
| Licence | LGPL-3.0-or-later. `LICENSE` is upstream's own file at that commit, unchanged; the full LGPL-3.0 text is `COPYING.LESSER`; the GPL-3.0 text it builds on is ART's `LICENSE` |
| Carried | `src/`, `README.md`, `Cargo.toml` (from `Cargo.toml.orig`: version `0.1.3+art.1`, `[dev-dependencies]` removed), `LICENSE`, `COPYING.LESSER` |
| Not carried | `tests/`: `GPL-3.0-only` headers, 9.3 MB of fixtures, and a dev-dependency (`sevenz-rust` 0.6) with RUSTSEC-2026-0245 and RUSTSEC-2026-0246. ART's own tests prove the patch (`src-tauri/src/core/preload/native.rs`) |

## Changes against 0.1.3

None in this revision: `src/` is byte-for-byte 0.1.3's.
```

- [ ] **Step 6: Patch the build to the vendored copy**

In `src-tauri/Cargo.toml`, replace this comment paragraph above `libpfs3 = "=0.1.3"`:

```toml
# Pinned exactly, the same way `ureq` above is: `core/preload/native.rs`'s
# `NativeFormatter::probe()` reports this version number as which
# implementation did the work, and a `cargo update` silently drifting that
# past what `probe()` claims would make the report lie about itself. A test
# in `native.rs` checks this line's string literally, so drift fails the
# build rather than the user's trust in the report.
```

with:

```toml
# Pinned exactly, the same way `ureq` above is. Since ART-310 this pin names
# the crates.io release ART's vendored copy was taken from; the build itself
# uses that copy, `vendor/libpfs3` (0.1.3+art.1), through `[patch.crates-io]`
# below. `core/preload/native.rs`'s `NativeFormatter::probe()` reports the
# vendored version as which implementation did the work, and a test there
# checks this line, the patch and the vendored manifest together, so drift
# fails the build rather than the user's trust in the report.
```

Insert immediately before the line `[dev-dependencies]`:

```toml
# ART-310: libpfs3 0.1.3's format writes two structures wrong (no super index
# level in SUPERINDEX mode; anodes 0-4 left unreserved). The patched copy in
# vendor/libpfs3 replaces the crates.io release everywhere in the build — see
# its ART-PATCH.md for what changed, and docs/licenses.md for the licence.
[patch.crates-io]
libpfs3 = { path = "vendor/libpfs3" }

```

In `src-tauri/src/core/preload/native.rs`, replace the doc comment and constant at `:113-121` (from
`/// The version pinned in `Cargo.toml`.` through `const LIBPFS3_VERSION: &str = "0.1.3";`) with:

```rust
/// The version of the `libpfs3` ART builds: the vendored copy in
/// `src-tauri/vendor/libpfs3` (ART-310) — crates.io's 0.1.3 with ART's patch,
/// `+art.1`. There is no `CARGO_PKG_VERSION`-style macro for a *dependency's*
/// version, so this is kept in sync by hand, the same trade-off ART already
/// accepts for `ureq`'s exact `=3.2.1` pin (CLAUDE.md). `probe()` reports this
/// constant as which implementation did the work, and
/// `the_pinned_version_constant_matches_cargo_toml` (below) reads the pin, the
/// `[patch.crates-io]` line and the vendored manifest, so the constant cannot
/// drift from what was actually built.
const LIBPFS3_VERSION: &str = "0.1.3+art.1";
```

- [ ] **Step 7: Run the tests to see them pass**

Run: `bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh bash -c 'cd src-tauri && cargo test --lib core::preload::native'`
Expected: `test result: ok. 37 passed; 0 failed; 1 ignored`.
Then: `grep -A2 'name = "libpfs3"' src-tauri/Cargo.lock`
Expected: `version = "0.1.3+art.1"` and **no** `source =` or `checksum =` line.

- [ ] **Step 8: The sizing suite, formatting, clippy, deny**

Run each separately and record the output:
- `bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh bash -c 'cd src-tauri && cargo test --lib core::card::sizing'`
  Expected: `test result: ok.`, with the same passed and ignored counts as on the base commit. Record `finished in`.
- `cd /home/tolon/Belgeler/Projeler/Art/src-tauri && cargo fmt --check`
  Expected: exit 0.
- `git -C /home/tolon/Belgeler/Projeler/Art status --short src-tauri/vendor`
  Expected: only `??` for the new directory. `cargo fmt` did not rewrite the vendored source.
- `bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh bash -c 'cd src-tauri && cargo clippy --all-targets -- -D warnings'`
  Expected: exit 0. **If it fails**, list every error's path. An error under `vendor/libpfs3` or in
  `core/preload/native.rs` blocks the task. An error elsewhere that is Linux-only (dead code behind the
  gate, a `cfg(windows)` item) is recorded for the owner's Windows clippy run, not fixed here.
- `cd /home/tolon/Belgeler/Projeler/Art/src-tauri && cargo deny check`
  Expected: `advisories ok, bans ok, licenses ok, sources ok`.

- [ ] **Step 9: Commit**

```bash
cd /home/tolon/Belgeler/Projeler/Art
git add src-tauri/vendor/libpfs3 src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/core/preload/native.rs
git status --short
git commit -q -F - <<'EOF'
ART-310 task 1: vendor libpfs3 0.1.3 unchanged as 0.1.3+art.1

- src-tauri/vendor/libpfs3: 0.1.3's src/ and README byte for byte (the .crate's SHA-256
  checked), upstream's LICENSE at 33e9ff6, the full LGPL-3.0 text, a manifest from
  Cargo.toml.orig with version 0.1.3+art.1 and no dev-dependencies, ART-PATCH.md. No tests/.
- src-tauri/Cargo.toml: [patch.crates-io] to the vendored copy; the =0.1.3 pin stays.
- native.rs: LIBPFS3_VERSION and probe() say 0.1.3+art.1; the pin test reads the pin, the
  patch and the vendored manifest (red first: the vendored manifest did not exist).

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
```

Expected: `git status --short` before the commit lists no gated file.

---

### Task 2: The format patch, proved by ART's own tests

The deliverable: `format_with_size` writes the `SB` level and reserves anodes 0–4. Three tests read the
image's blocks and show it. Each test is seen red on 0.1.3 and fails under a mutation.

**Files:**
- Modify: `src-tauri/src/core/preload/native.rs` (tests module: helpers after `fn pfs3_free_bytes` at `:1269-1272`; the three tests after `fn copy_in_reports_what_it_moved` at `:1402-1413`)
- Modify: `src-tauri/vendor/libpfs3/src/format.rs`
- Modify: `src-tauri/vendor/libpfs3/ART-PATCH.md`

**Interfaces:**
- Consumes:
  - `LIBPFS3_VERSION` from Task 1, and the gate script;
  - the existing test helpers `partition_offset(image: &Path) -> u64`, `formatted_pds3_image() -> (ScratchDir, PathBuf)` (8 MB, small mode) and `scratch(tag) -> (ScratchDir, PathBuf)`;
  - `fixtures::scratch(tag)`, `NativeFormatter::copy_in(image, None, "DH0", tree, &NoProgress) -> CoreResult<CopySummary>`.
- Produces:
  - `fn formatted_large_pds3_image() -> (crate::core::ScratchDir, PathBuf)`;
  - `fn anode_chain(image: &Path) -> AnodeChain`, with `struct AnodeChain { supermode: bool, ids: Vec<[u8; 2]>, anodes: Vec<(u32, u32, u32)> }`.

  Task 3 uses `partition_offset`, not these.

- [ ] **Step 1: Write the helpers**

In `native.rs`'s tests module, directly after `fn pfs3_free_bytes`:

```rust
    /// A PFS3 partition past MAXSMALLDISK (10 241 440 blocks), the size at which
    /// a format selects SUPERINDEX mode: 5 100 MiB is 10 362 cylinders,
    /// 10 444 896 blocks. The image is extended with `set_len` (`create_hdf`),
    /// and the format writes only the reserved area near the partition's start.
    fn formatted_large_pds3_image() -> (crate::core::ScratchDir, PathBuf) {
        let (_guard, dir) = scratch("pds3-large");
        let path = dir.join("card.hdf");
        crate::core::hdf::create_hdf(
            &path,
            5_200 * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::Pfs3DirectScsi,
                size_mb: 5_100,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[],
        )
        .unwrap();
        NativeFormatter
            .format_partition(&path, None, 1, "Work", &NoProgress)
            .unwrap();
        (_guard, path)
    }

    /// **ART-310.** What a PFS3 format left on disk, read straight off the
    /// image — not through `libpfs3`'s reader, which shares a crate with the
    /// writer under test. Follows the rootblock to the first anode block:
    /// `rootblock.indexblocks[0]` in small mode, `rext.superindex[0]` in
    /// SUPERINDEX mode.
    struct AnodeChain {
        /// `MODE_SUPERINDEX` (0x80) is set in the rootblock's options.
        supermode: bool,
        /// The two-byte id of every block from that pointer down to the first
        /// `AB`, in order: `IB AB` in small mode, `SB IB AB` in SUPERINDEX mode.
        ids: Vec<[u8; 2]>,
        /// Anodes 0–5 of the first anode block, as `(clustersize, blocknr, next)`.
        anodes: Vec<(u32, u32, u32)>,
    }

    fn anode_chain(image: &Path) -> AnodeChain {
        use std::io::{Read, Seek, SeekFrom};
        const MODE_SUPERINDEX: u32 = 0x80;
        let offset = partition_offset(image);
        let mut file = std::fs::File::open(image).unwrap();
        let mut sectors = |sector: u32, len: usize| -> Vec<u8> {
            file.seek(SeekFrom::Start(offset + u64::from(sector) * 512)).unwrap();
            let mut buf = vec![0u8; len];
            file.read_exact(&mut buf).unwrap();
            buf
        };
        let be16 = |b: &[u8], at: usize| u16::from_be_bytes([b[at], b[at + 1]]);
        let be32 = |b: &[u8], at: usize| u32::from_be_bytes(b[at..at + 4].try_into().unwrap());

        // Rootblock at sector 2: options 0x04, reserved_blksize 0x40,
        // extension 0x58, and the index union from 0x60 (small mode:
        // bitmapindex[0..=4], then indexblocks[0..]).
        let root = sectors(2, 512);
        let supermode = be32(&root, 0x04) & MODE_SUPERINDEX != 0;
        let resblk = usize::from(be16(&root, 0x40));
        let mut next = if supermode {
            let ext = sectors(be32(&root, 0x58), resblk);
            be32(&ext, 0x40) // rext.superindex[0]
        } else {
            be32(&root, 0x60 + 5 * 4) // rootblock.indexblocks[0]
        };
        let mut ids = Vec::new();
        loop {
            let block = sectors(next, resblk);
            let id = [block[0], block[1]];
            ids.push(id);
            if &id == b"AB" {
                // Anode block: a 16-byte header, then 12-byte anodes.
                let anodes = (0..6)
                    .map(|k| {
                        let at = 16 + 12 * k;
                        (be32(&block, at), be32(&block, at + 4), be32(&block, at + 8))
                    })
                    .collect();
                return AnodeChain { supermode, ids, anodes };
            }
            assert!(
                (&id == b"SB" || &id == b"IB") && ids.len() < 3,
                "the anode pointer chain reached {:?} after {:?}",
                String::from_utf8_lossy(&id),
                ids.iter().map(|i| String::from_utf8_lossy(i).into_owned()).collect::<Vec<_>>()
            );
            next = be32(&block, 12); // index[0], after a 12-byte header
        }
    }
```

- [ ] **Step 2: Write the three failing tests**

Directly after `fn copy_in_reports_what_it_moved`:

```rust
    // ---- ART-310: the format writes what pfs3aio writes ----

    /// **ART-310, every size.** pfs3aio's format reserves anodes 0–4 by
    /// allocating them (`AllocAnode` leaves `clustersize 0, blocknr 0xffffffff,
    /// next 0`); `libpfs3` 0.1.3 left them `(0, 0, 0)`, which every allocator
    /// ported from pfs3aio reads as free — hst-imager's first new directory then
    /// failed `ERROR_DISK_FULL`, and a later one was given reserved anode 1.
    #[test]
    fn a_small_pfs3_format_reserves_anodes_zero_to_four() {
        let (_guard, image) = formatted_pds3_image();
        let chain = anode_chain(&image);
        assert!(!chain.supermode, "an 8 MB partition must be small mode");
        assert_eq!(chain.ids, vec![*b"IB", *b"AB"]);
        for (nr, anode) in chain.anodes.iter().take(5).enumerate() {
            assert_eq!(
                *anode,
                (0, 0xFFFF_FFFF, 0),
                "anode {nr} must be reserved the way pfs3aio's AllocAnode leaves it"
            );
        }
        assert_eq!(chain.anodes[5].0, 1, "anode 5 is the root directory, one block");
    }

    /// **ART-310, SUPERINDEX mode.** Every reader — pfs3aio's `GetSuperBlock`,
    /// hst-imager's port, `libpfs3`'s own `resolve_anode_block` — walks
    /// `superindex[0] -> SB -> IB -> AB`. `libpfs3` 0.1.3 pointed
    /// `superindex[0]` at the index block itself: hst-imager could not mount
    /// the volume and `libpfs3` could not read its own root directory.
    #[test]
    fn a_large_pfs3_format_writes_the_superblock_level() {
        let (_guard, image) = formatted_large_pds3_image();
        let chain = anode_chain(&image);
        assert!(chain.supermode, "a 5 100 MiB partition must be SUPERINDEX mode");
        assert_eq!(
            chain.ids,
            vec![*b"SB", *b"IB", *b"AB"],
            "superindex[0] must name a super index block, not the anode index block"
        );
        for (nr, anode) in chain.anodes.iter().take(5).enumerate() {
            assert_eq!(
                *anode,
                (0, 0xFFFF_FFFF, 0),
                "anode {nr} must be reserved in SUPERINDEX mode too"
            );
        }
    }

    /// **ART-310, the consequence.** A volume past MAXSMALLDISK that
    /// `NativeFormatter` formatted takes `copy_in`'s writes and gives back the
    /// same bytes. On 0.1.3 the first write failed `anode 5 not found`.
    #[test]
    fn a_large_pfs3_volume_takes_its_own_writes() {
        let (_guard, image) = formatted_large_pds3_image();
        let (_guard, tree) = fixtures::scratch("large-pfs3-writes");
        std::fs::create_dir_all(tree.join("C")).unwrap();
        std::fs::write(tree.join("C/Assign"), b"assign\n").unwrap();
        std::fs::write(tree.join("Readme"), b"hello from ART\n").unwrap();

        let summary = NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();
        assert_eq!((summary.files, summary.directories), (2, 1));

        let mut vol = libpfs3::volume::Volume::open(&image, partition_offset(&image)).unwrap();
        assert_eq!(vol.read_file("C/Assign").unwrap(), b"assign\n");
        assert_eq!(vol.read_file("Readme").unwrap(), b"hello from ART\n");
    }
```

- [ ] **Step 3: Run the tests to see them fail on 0.1.3's format**

Run: `bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh bash -c 'cd src-tauri && cargo test --lib -- a_small_pfs3_format_reserves_anodes_zero_to_four a_large_pfs3_format_writes_the_superblock_level a_large_pfs3_volume_takes_its_own_writes'`

Expected: `3 failed`, each for its own reason:
- `a_small_…`: `anode 0 must be reserved…`, `left: (0, 0, 0)`
- `a_large_pfs3_format_…`: `superindex[0] must name a super index block…`, `left: [[73, 66], [65, 66]]` (`IB AB`)
- `a_large_pfs3_volume_…`: a panic whose message contains `anode 5 not found`

Copy all three panic messages verbatim into
`~/Belgeler/Projeler/art-experiments/2026-09-13-art-310/plan-run.md` under `## Task 2 red`, with the
`finished in` time. If a test fails for a **different** reason, stop: the guard is wrong, not the format.

- [ ] **Step 4: Patch `format.rs`**

Save as `/home/tolon/Belgeler/Projeler/art-experiments/art-310-format-patch.py` (Task 5 uses it too):

```python
#!/usr/bin/env python3
"""ART-310's change to libpfs3's src/format.rs. Usage: art-310-format-patch.py <format.rs>

Applies to 0.1.3 and to 0.1.6/main alike: every anchor below is identical in
both. Each anchor must occur exactly once, or nothing is written.
"""
import sys

path = sys.argv[1]
s = open(path, encoding="utf-8").read()


def sub(old: str, new: str) -> None:
    global s
    n = s.count(old)
    assert n == 1, f"expected exactly one {old!r}, found {n}"
    s = s.replace(old, new)


sub(
    "//! 6. Allocate and write anode index + anode block (with ANODE_ROOTDIR)\n",
    "//! 6. Allocate and write the super index block (SUPERINDEX mode only), the\n"
    "//!    anode index block and the anode block (anodes 0-4 reserved, ANODE_ROOTDIR)\n",
)
sub(
    "    // 4. Allocate anode index + anode block\n"
    "    let anidx_blk = firstreserved + alloc.alloc()? * rescluster;\n",
    "    // 4. Allocate the anode index path. In SUPERINDEX mode pfs3aio's first\n"
    "    //    AllocAnode goes through NewSuperBlock before NewIndexBlock, so the\n"
    "    //    super index block is allocated first.\n"
    "    let sb_blk = if supermode {\n"
    "        Some(firstreserved + alloc.alloc()? * rescluster)\n"
    "    } else {\n"
    "        None\n"
    "    };\n"
    "    let anidx_blk = firstreserved + alloc.alloc()? * rescluster;\n",
)
sub(
    "    if supermode {\n"
    "        put_u32(&mut rext, 0x40, anidx_blk); // superindex[0]\n"
    "    }\n",
    "    if let Some(sb_blk) = sb_blk {\n"
    "        // superindex[0] names the super index block, never the anode index\n"
    "        // block: every reader walks SB -> IB -> AB.\n"
    "        put_u32(&mut rext, 0x40, sb_blk); // superindex[0]\n"
    "    }\n",
)
sub(
    "    // Write anode index block\n",
    "    // Write the super index block, as pfs3aio's NewSuperBlock leaves it:\n"
    "    // id SB, seqnr 0, index[0] = the anode index block.\n"
    "    if let Some(sb_blk) = sb_blk {\n"
    "        let mut sb = vec![0u8; resblocksize as usize];\n"
    "        put_u16(&mut sb, 0, SBLKID);\n"
    "        put_u32(&mut sb, 4, 1); // datestamp\n"
    "        put_u32(&mut sb, 8, 0); // seqnr\n"
    "        put_u32(&mut sb, 12, anidx_blk); // index[0] = anode index block\n"
    "        write_reserved_blocks(dev, sb_blk as u64, &sb, rescluster, bs)?;\n"
    "    }\n"
    "\n"
    "    // Write anode index block\n",
)
sub(
    "    put_u32(&mut an, 8, 0); // seqnr\n",
    "    put_u32(&mut an, 8, 0); // seqnr\n"
    "    // Anodes 0..ANODE_ROOTDIR-1 are reserved, as pfs3aio's format leaves them\n"
    "    // (AllocAnode: clustersize 0, blocknr 0xffffffff, next 0). An allocator\n"
    "    // ported from pfs3aio hands out any anode that reads (0, 0, 0).\n"
    "    for nr in 0..ANODE_ROOTDIR as usize {\n"
    "        let off = ANODE_BLOCK_HEADER_SIZE + nr * ANODE_SIZE;\n"
    "        put_u32(&mut an, off + 4, 0xFFFF_FFFF);\n"
    "    }\n",
)
open(path, "w", encoding="utf-8").write(s)
print("patched", path)
```

Run: `python3 ~/Belgeler/Projeler/art-experiments/art-310-format-patch.py /home/tolon/Belgeler/Projeler/Art/src-tauri/vendor/libpfs3/src/format.rs`
Expected: `patched …/format.rs`. (0.1.3's `use crate::ondisk::*;` already brings `SBLKID` into scope.)

- [ ] **Step 5: Run the tests to see them pass**

Run: `bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh bash -c 'cd src-tauri && cargo test --lib core::preload::native'`
Expected: `test result: ok. 40 passed; 0 failed; 1 ignored`. Record `finished in` under `## Task 2 green`
in `plan-run.md`. **If the two large tests together take more than 60 s, stop and report.** That is the
spec's § 5 fallback question, and it is the owner's to rule on.

Then: `bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh bash -c 'cd src-tauri && cargo test --lib core::card::sizing'`
Expected: `test result: ok.`, the same counts as in Task 1, Step 8.

- [ ] **Step 6: Record the diff in `ART-PATCH.md`**

Replace the section `## Changes against 0.1.3` (to the end of the file) with the following, then append
the diff:

````markdown
## Changes against 0.1.3

**`src/format.rs` only** ([ART-310](../../../docs/ISSUES.md); research
`docs/superpowers/notes/2026-09-11-libpfs3-format-fix-research.md`, design
`docs/superpowers/specs/2026-09-13-art-310-libpfs3-format-fix-design.md`):

1. **SUPERINDEX mode writes the super index level.** One more reserved block is allocated after the
   bitmap index blocks and before the anode index block, which is pfs3aio's order. It is written as
   `SBLKID`, datestamp 1, seqnr 0, `index[0]` = the anode index block, and `rext.superindex[0]` names it.
   0.1.3 pointed `superindex[0]` at the anode index block. hst-imager could not mount such a volume, and
   `libpfs3` could not read its own root directory (`anode 5 not found`).
2. **Anodes 0–4 are reserved at every size**: `clustersize 0, blocknr 0xFFFFFFFF, next 0`, what pfs3aio's
   `AllocAnode` leaves. 0.1.3 left them `(0, 0, 0)`. hst-imager's first new directory then failed
   `ERROR_DISK_FULL`, and a later one was given reserved anode 1.

Written in this crate's own idiom from the on-disk layout, not translated from pfs3aio (BSD-4-Clause) or
any port of it. Reserved-area sizing, option flags, datestamps and everything else are 0.1.3's.

**The writer is 0.1.3's, unchanged.** Its two index-block gaps are ART-311.

## Upstream

Not offered yet. The same change is prepared against `metaneutrons/pfs3` `main` for the owner to open,
after a patched volume has been mounted under real pfs3aio.

## Diff against 0.1.3

```diff
````

```bash
cd /home/tolon/Belgeler/Projeler/Art
diff -u --label a/src/format.rs --label b/src/format.rs \
  ~/Belgeler/Projeler/art-experiments/libpfs3-0.1.3-pristine/libpfs3-0.1.3/src/format.rs \
  src-tauri/vendor/libpfs3/src/format.rs > /home/tolon/Belgeler/Projeler/art-experiments/art-310-format.diff
printf '%s\n' "$(cat /home/tolon/Belgeler/Projeler/art-experiments/art-310-format.diff)" '```' >> src-tauri/vendor/libpfs3/ART-PATCH.md
diff -rq ~/Belgeler/Projeler/art-experiments/libpfs3-0.1.3-pristine/libpfs3-0.1.3/src src-tauri/vendor/libpfs3/src
```

Expected: `diff -u` exits 1 (differences). `diff -rq` names only `format.rs`.

- [ ] **Step 7: fmt, clippy, sweep**

- `cd /home/tolon/Belgeler/Projeler/Art/src-tauri && cargo fmt --check`. Expected: exit 0.
- `bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh bash -c 'cd src-tauri && cargo clippy --all-targets -- -D warnings'`.
  Expected: exit 0, or only the Linux-only errors already recorded in Task 1, Step 8.
- `cd /home/tolon/Belgeler/Projeler/Art && python3 scripts/control-byte-sweep.py`. Expected: `clean`.

- [ ] **Step 8: Commit**

```bash
cd /home/tolon/Belgeler/Projeler/Art
git add src-tauri/vendor/libpfs3/src/format.rs src-tauri/vendor/libpfs3/ART-PATCH.md src-tauri/src/core/preload/native.rs
git status --short
git commit -q -F - <<'EOF'
ART-310 task 2: libpfs3's format writes the SB level and reserves anodes 0-4

- vendor/libpfs3/src/format.rs: in SUPERINDEX mode a super index block is allocated before
  the anode index block and rext.superindex[0] names it (0.1.3 pointed it at the index
  block); anodes 0-4 are written (0, 0xFFFFFFFF, 0) at every size (0.1.3 left them free).
- native.rs: anode_chain reads the rootblock, the index chain and anodes 0-5 off the image;
  three tests, each red on 0.1.3 first (anode 0 read (0,0,0); the chain read IB AB;
  copy_in failed "anode 5 not found"), all green on the patch.
- ART-PATCH.md: what changed, why, and the diff against 0.1.3.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
```

- [ ] **Step 9: Mutate each guard and record what fell**

For each mutation:
1. Edit `src-tauri/vendor/libpfs3/src/format.rs` as described.
2. Run the three tests with the command from Step 3.
3. Record which failed and the message under `## Task 2 mutations` in `plan-run.md`.
4. Restore with `git checkout -- src-tauri/vendor/libpfs3/src/format.rs`. This is safe only because
   Step 8 committed the patch.

| # | Mutation | Must fail |
|---|---|---|
| M1 | `for nr in 0..ANODE_ROOTDIR as usize` → `for nr in 1..ANODE_ROOTDIR as usize` (anode 0 left free, the exact first failure) | `a_small_pfs3_format_reserves_anodes_zero_to_four`, `a_large_pfs3_format_writes_the_superblock_level` |
| M2 | `put_u32(&mut rext, 0x40, sb_blk); // superindex[0]` → `put_u32(&mut rext, 0x40, anidx_blk); // superindex[0]` | `a_large_pfs3_format_writes_the_superblock_level`, `a_large_pfs3_volume_takes_its_own_writes` |
| M3 | delete the line `write_reserved_blocks(dev, sb_blk as u64, &sb, rescluster, bs)?;` (the pointer kept, the block never written) | `a_large_pfs3_format_writes_the_superblock_level`, `a_large_pfs3_volume_takes_its_own_writes` |
| M4 | in `src-tauri/Cargo.toml`, delete the two lines `[patch.crates-io]` and `libpfs3 = { path = "vendor/libpfs3" }` (the whole fix gone; restore with `git checkout -- src-tauri/Cargo.toml src-tauri/Cargo.lock`) | all three, and `the_pinned_version_constant_matches_cargo_toml` |

Expected: every "Must fail" test fails. **If one survives, ask which is wrong, the guard or the mutation**
(`docs/testing.md`, *A survivor is one of two things*). Fix a weak guard, then commit it with a message
naming the survivor. Do not write a weak guard down as a survivor. Afterwards `git status --short` must
be empty.

---

### Task 3: The oracle past MAXSMALLDISK, and the anode question

The deliverable: `scripts/pfs3-oracle-check.py` proves against hst-imager that a volume ART formats past
MAXSMALLDISK mounts, lists, extracts byte for byte and takes hst-imager's writes. At both sizes, the
directory hst-imager makes on ART's volume gets an anode above 4. The script runs on Linux as well as
Windows.

**Files:**
- Modify: `src-tauri/src/core/preload/native.rs`:
  - the hook `build_pfs3_volume_for_oracle_when_asked` at `:2063-2160`;
  - a new hook `build_large_pfs3_volume_for_oracle_when_asked`;
  - a new hook `pfs3_anode_for_oracle_when_asked`.
- Modify: `scripts/pfs3-oracle-check.py`:
  - `check_art_writes_hst_reads` (`:287-414`);
  - `main` (`:537-591`);
  - a path helper near `run_hst` (`:141`).

**Interfaces:**
- Consumes: `partition_offset(image: &Path) -> u64` (tests module); `NativeFormatter` from Task 2.
- Produces:
  - `fn write_pfs3_volume_for_oracle(image: &Path, disk_bytes: u64, partition_mb: u32)`;
  - env-gated hooks: `ART_PFS3_WRITE_OUT_LARGE=<image>` prints `json=…` exactly like the small hook;
    `ART_PFS3_ANODE_IN=<image> ART_PFS3_ANODE_PATH=<path>` prints `anode=<n>`;
  - Python `dh0(image: Path) -> str`.

- [ ] **Step 1: Split the write hook**

In `native.rs`, replace the whole function `build_pfs3_volume_for_oracle_when_asked` (from `#[test]` above
`fn build_pfs3_volume_for_oracle_when_asked()` through its closing brace; keep the doc comment above it)
with:

```rust
    #[test]
    fn build_pfs3_volume_for_oracle_when_asked() {
        let Ok(target) = std::env::var("ART_PFS3_WRITE_OUT") else {
            return;
        };
        write_pfs3_volume_for_oracle(&PathBuf::from(&target), 220 * 1024 * 1024, 200);
    }

    /// **ART-310.** The same volume and the same JSON claim as
    /// `build_pfs3_volume_for_oracle_when_asked`, on a partition past
    /// MAXSMALLDISK (5 100 MiB, 10 444 896 blocks) — SUPERINDEX mode, the mode
    /// `libpfs3` 0.1.3's format wrote wrong.
    ///
    /// ```text
    /// ART_PFS3_WRITE_OUT_LARGE=... cargo test build_large_pfs3_volume_for_oracle_when_asked -- --nocapture
    /// ```
    #[test]
    fn build_large_pfs3_volume_for_oracle_when_asked() {
        let Ok(target) = std::env::var("ART_PFS3_WRITE_OUT_LARGE") else {
            return;
        };
        write_pfs3_volume_for_oracle(&PathBuf::from(&target), 5_200 * 1024 * 1024, 5_100);
    }

    /// **ART-310, the oracle's anode question.** Prints the anode number of one
    /// entry on a PFS3 volume, so `pfs3-oracle-check.py` can ask whether
    /// hst-imager — whose allocator is a port of pfs3aio's — handed a new
    /// directory one of the numbers pfs3aio reserves (0–4).
    ///
    /// ```text
    /// ART_PFS3_ANODE_IN=<image> ART_PFS3_ANODE_PATH=<path> cargo test pfs3_anode_for_oracle_when_asked -- --nocapture
    /// ```
    #[test]
    fn pfs3_anode_for_oracle_when_asked() {
        let (Ok(source), Ok(path)) = (
            std::env::var("ART_PFS3_ANODE_IN"),
            std::env::var("ART_PFS3_ANODE_PATH"),
        ) else {
            return;
        };
        let image = PathBuf::from(&source);
        let mut vol = libpfs3::volume::Volume::open(&image, partition_offset(&image)).unwrap();
        let entry = vol
            .lookup(&path)
            .unwrap()
            .unwrap_or_else(|| panic!("{path} is not on the volume"));
        println!("anode={}", entry.anode);
    }

    /// The body both write hooks share: an RDB image of `disk_bytes` with one
    /// PDS\3 partition of `partition_mb`, formatted and filled through the same
    /// two calls G5 makes, then the JSON of every entry it believes it wrote.
    fn write_pfs3_volume_for_oracle(image: &Path, disk_bytes: u64, partition_mb: u32) {
        if let Some(parent) = image.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        crate::core::hdf::create_hdf(
            image,
            disk_bytes,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::Pfs3DirectScsi,
                size_mb: partition_mb,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[],
        )
        .unwrap();

        NativeFormatter
            .format_partition(image, None, 1, "Workbench", &NoProgress)
            .unwrap();

        // MOVED VERBATIM — see Step 1's note below.
    }
```

**Note for the `// MOVED VERBATIM` line.** Replace that comment with the old function's remaining body,
character for character. It runs from the line `// The literal bytes are named once and reused for both
the write` through `println!("json={entries}");`, in the version at commit `4b0950b`
(`git show 4b0950b:src-tauri/src/core/preload/native.rs`, lines 2092-2159). Inside the moved block,
change only the two calls that took `&image`: `.copy_in(&image, …)` becomes `.copy_in(image, …)`. The
old code had `let image = PathBuf::from(&target);`, and the new function receives `image: &Path`.

- [ ] **Step 2: Check the hooks still build and stay silent without their env**

Run: `bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh bash -c 'cd src-tauri && cargo test --lib core::preload::native'`
Expected: `test result: ok. 42 passed; 0 failed; 1 ignored`. The two new hooks return early.

- [ ] **Step 3: Teach the script the separator, the large direction and the anode check**

In `scripts/pfs3-oracle-check.py`, after `def run_hst(…)`:

```python
# hst-imager takes `<image>\rdb\dh0` on Windows and `<image>/rdb/dh0` on Linux.
SEP = "\\" if os.name == "nt" else "/"


def dh0(image: Path) -> str:
    return f"{image.name}{SEP}rdb{SEP}dh0"
```

Replace every `f"{image.name}\\rdb\\dh0"` in the file with `dh0(image)`. There are three: the
`fs dir` listing, the `fs copy` extraction, and `fs copy` in `check_hst_writes_art_reads`.

Change `check_art_writes_hst_reads`'s signature and its first lines:

```python
def check_art_writes_hst_reads(
    hst: str, work: Path, hook: str, env_var: str, image_name: str
) -> tuple[list[tuple[bool, str]], list[str]]:
```

- Inside it, replace `image = work / "art-write.hdf"` with `image = work / image_name`.
- Replace the `run_cargo_test("build_pfs3_volume_for_oracle_when_asked", {"ART_PFS3_WRITE_OUT": str(image)})`
  call with `run_cargo_test(hook, {env_var: str(image)})`.
- Replace `extract_dir = work / "art-write-extract"` with `extract_dir = work / f"{image.stem}-extract"`.

Directly after the `checks.append((True, "ART wrote a PFS3 volume"))` line, add:

```python
    st = image.stat()
    on_disk = getattr(st, "st_blocks", None)  # POSIX only
    print(
        f"  {image.name}: {st.st_size} bytes long"
        + (f", {on_disk * 512} bytes on disk" if on_disk is not None else "")
    )
```

Replace the final `return checks, skipped` of `check_art_writes_hst_reads` (the one after the extraction
loop) with:

```python
    # ART-310: hst-imager's allocator is a port of pfs3aio's. On a volume whose
    # anodes 0-4 are left unreserved it fails its first new directory
    # (ERROR_DISK_FULL) or hands a later one a reserved number.
    made = run_hst(hst, ["fs", "mkdir", f"{dh0(image)}{SEP}HstMade"], work)
    checks.append((made.returncode == 0, "hst-imager made a directory on the volume ART formatted"))
    if made.returncode != 0:
        print(made.stdout[-1500:])
        print(made.stderr[-500:])
        return checks, skipped
    asked = run_cargo_test(
        "pfs3_anode_for_oracle_when_asked",
        {"ART_PFS3_ANODE_IN": str(image), "ART_PFS3_ANODE_PATH": "HstMade"},
    )
    anode = next(
        (int(l[len("anode="):]) for l in asked.stdout.splitlines() if l.startswith("anode=")),
        None,
    )
    checks.append(
        (
            anode is not None and anode > 4,
            f"hst-imager's new directory has an anode pfs3aio does not reserve "
            f"(anode {anode}; 0-4 are reserved, ART-310)",
        )
    )
    return checks, skipped
```

In `main`, replace:

```python
        print("ART writes, hst-imager reads:")
        checks_a, skipped_a = check_art_writes_hst_reads(hst, work)
        report(checks_a)
```

with:

```python
        print("ART writes, hst-imager reads:")
        checks_a, skipped_a = check_art_writes_hst_reads(
            hst, work, "build_pfs3_volume_for_oracle_when_asked", "ART_PFS3_WRITE_OUT",
            "art-write.hdf",
        )
        report(checks_a)

        print("\nART writes past MAXSMALLDISK (SUPERINDEX mode), hst-imager reads:")
        checks_l, skipped_l = check_art_writes_hst_reads(
            hst, work, "build_large_pfs3_volume_for_oracle_when_asked",
            "ART_PFS3_WRITE_OUT_LARGE", "art-write-large.hdf",
        )
        report(checks_l)
        checks_a += checks_l
        skipped_a += skipped_l
```

- [ ] **Step 4: Run the oracle on the fix**

```bash
mkdir -p /home/tolon/Belgeler/Projeler/art-experiments/tmp/oracle
bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh env \
  ART_HST_IMAGER=$HOME/.local/share/art-tools/hst-imager-1.6.616/hst.imager \
  ART_PFS3_DRIVER=$HOME/.local/share/art-tools/pfs3aio-3.1/pfs3aio \
  ART_SCRATCH=/home/tolon/Belgeler/Projeler/art-experiments/tmp/oracle \
  python3 scripts/pfs3-oracle-check.py
```

Expected:
- Three sections, every check `ok`, ending `ART and hst-imager agree, both directions`.
- Both anode lines name an anode above 4.
- The size line for `art-write-large.hdf` reports its length (5 452 554 240 bytes — 10 565 whole cylinders of the 5 200 MiB asked; `create_rdb_layout` rounds down) and its bytes on disk.

Copy the whole output under `## Task 3 oracle, fix` in `plan-run.md`.

If the script refuses the scratch directory or cannot find hst-imager, read its message: it names the env
variable it wanted.

- [ ] **Step 5: Put the defect back and run the oracle again**

Delete the two lines `[patch.crates-io]` and `libpfs3 = { path = "vendor/libpfs3" }` from
`src-tauri/Cargo.toml`, then run Step 4's command again.

Expected:
- The small section's anode check **fails**. hst-imager's `mkdir` either fails, or the directory gets an
  anode ≤ 4.
- The large section fails at hst-imager's listing (`NullReferenceException`).

Copy the output under `## Task 3 oracle, 0.1.3`. Restore with
`git checkout -- src-tauri/Cargo.toml src-tauri/Cargo.lock`, then check `git status --short` shows only
this task's two files.

- [ ] **Step 6: Commit**

```bash
cd /home/tolon/Belgeler/Projeler/Art
git add scripts/pfs3-oracle-check.py src-tauri/src/core/preload/native.rs
git status --short
git commit -q -F - <<'EOF'
ART-310 task 3: the PFS3 oracle past MAXSMALLDISK, and the anode question

- native.rs: the oracle write hook's body is write_pfs3_volume_for_oracle; a second hook
  writes the same tree on a 5 100 MiB partition (SUPERINDEX mode); a third prints an
  entry's anode number.
- pfs3-oracle-check.py: an "ART writes past MAXSMALLDISK" direction; at both sizes
  hst-imager makes a directory on ART's volume and it must get an anode above 4; the size
  on disk of each image ART wrote; hst-imager paths with the platform's separator, so the
  script runs on Linux too.
- Run on Linux with hst-imager 1.6.616: all ok on the fix; with [patch] removed, the anode
  check and the whole large direction fail.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
```

---

### Task 4: Close ART-310 in the documents, file ART-311

The deliverable: every document that described 0.1.3's format as current says what is true now. The
evidence from Tasks 1–3 is in ART-310's Fixed entry, and ART-311 is filed.

**Files:**
- Modify: `docs/ISSUES.md`:
  - ART-310's entry at `:29-88` moves to the top of `## Fixed` (`:316`);
  - ART-311 is added at the top of `## Open` (`:27`).
- Modify: `src-tauri/src/core/card/sizing.rs:456-459` and the doc comment of `many_files_cross_into_superindex_mode` (`:906-931`).
- Modify: `src-tauri/src/core/preload/native.rs:10-16` (module doc).
- Modify: `docs/licenses.md:52` and `:122-125`; `THIRD_PARTY_LICENSES.md:60-65`; `deny.toml:36-41`.
- Modify: `CHANGELOG.md` (`## [Unreleased]` → `### Fixed`).
- Modify: `docs/STATUS.md` (`### Start here`, the Snapshot rows *Last updated* and *Tests — Rust*).
- Modify: `docs/session-log.md` (a new top row).

**Interfaces:**
- Consumes: `plan-run.md` from Tasks 2 and 3 (red messages, mutation results, oracle output, times).
- Produces: nothing code uses.

- [ ] **Step 1: The owner's Windows run**

Hand the owner these lines and wait for the output. The Linux gate cannot stand in for them.

```powershell
git fetch; git checkout art-310-libpfs3-format   # after the owner has the branch
cd src-tauri
$env:TMP='E:\amiga\ProjeART\build\tmp'; $env:TEMP=$env:TMP
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --lib
cargo test --lib
cargo deny check
```

Expected: fmt and clippy exit 0; both `cargo test --lib` runs end `test result: ok.` with **5 more passed**
than 3252 — Task 2's three ART-310 tests and Task 3's two oracle hooks: **3257 passed; 0 failed; 58
ignored**. If the counts differ, find out why before writing them anywhere. Record the
lines under `## Task 4 Windows` in `plan-run.md`.

The branch reaches the owner's machine only by the owner's word (a push). **Ask; do not push.**

- [ ] **Step 2: `sizing.rs` — the two comments that describe the broken format as current**

Replace:

```rust
/// **ART-310:** every Work partition this plans, on every card size, is past
/// MAXSMALLDISK (10 241 440 blocks), where `libpfs3` 0.1.3's format is wrong
/// — a caller must not hand one to `NativeFormatter` while ART-310 is open.
```

with:

```rust
/// Every Work partition this plans, on every card size, is past MAXSMALLDISK
/// (10 241 440 blocks), in PFS3's SUPERINDEX mode. `libpfs3` 0.1.3's format
/// wrote that mode wrong; ART builds the vendored `0.1.3+art.1`, which writes
/// it as pfs3aio does (ART-310, fixed), so `NativeFormatter` may format these
/// partitions.
```

In the doc comment of `fn many_files_cross_into_superindex_mode`, replace everything from the line
`/// This only proves the size crosses the mode boundary, not that the` through
`/// what it can honestly prove today.` with:

```rust
    /// This only proves the size crosses the mode boundary, not that the
    /// content can be written there: filling this content at the returned
    /// estimate is **not** attempted here. The first attempt at such a fill,
    /// against `libpfs3` 0.1.3, failed at once with `"anode 5 not found"`:
    /// 0.1.3's format pointed `superindex[0]` at the anode index block where
    /// every reader expects a super index block. That was ART-310, fixed in
    /// the vendored `0.1.3+art.1`; a SUPERINDEX-mode volume taking its own
    /// writes is `core::preload::native`'s
    /// `a_large_pfs3_volume_takes_its_own_writes`. Filling AGS scale
    /// (~140 000 files) — and SUPERINDEX mode generally — is round 5's concern
    /// per the review's own ruling; this test proves only what it can honestly
    /// prove today.
```

- [ ] **Step 3: `native.rs` module doc**

After the line `//! type — is refused by name; `NativeFormatter` does not guess.`, insert:

```rust
//!
//! `libpfs3` here is ART's vendored copy, `src-tauri/vendor/libpfs3`
//! (`0.1.3+art.1`): 0.1.3's format left out the super index level and left
//! anodes 0–4 unreserved (ART-310), and only `format.rs` differs. Its writer is
//! 0.1.3's, so the writer limits below (ART-113, ART-116) and its two
//! index-block gaps (ART-311) still hold.
```

- [ ] **Step 4: Licences**

- **`docs/licenses.md`, the `libpfs3` row** (`:52`): append to the end of its last cell, before the closing
  `|`:
  ` **Vendored and patched** since ART-310: `src-tauri/vendor/libpfs3`, version `0.1.3+art.1`, only
  `src/format.rs` changed (its `ART-PATCH.md` says what and why); upstream's `LICENSE` and the full LGPL-3.0
  text (`COPYING.LESSER`) travel with it.`
- **`docs/licenses.md`, the pfs3aio paragraph** (`:122-125`, ending `not by copying or linking pfs3aio
  itself.`): append a new paragraph after it:

  ```markdown
  *Added 2026-09-13 (ART-310).* That copy is now patched in ART's tree, and `libpfs3`'s formatter says
  of itself that it is *"ported from pfs3aio/format.c"* — pfs3aio's licence is BSD-4-Clause (Michiel
  Pelt, 2011, with the advertising clause), which the FSF lists as incompatible with the GPL. ART's
  patch is written from the on-disk layout in `libpfs3`'s own idiom, not translated from pfs3aio or its
  ports (hst-amiga, AmigaDiskKit); what that lineage means for the crate as a whole is recorded in
  `docs/superpowers/notes/2026-09-11-libpfs3-format-fix-research.md` § 7 and not settled there.
  ```
- **`THIRD_PARTY_LICENSES.md`**: at the end of the `libpfs3` bullet, after `reasoning and its cost`,
  append:
  `. Vendored and patched as `0.1.3+art.1` in `src-tauri/vendor/libpfs3` (ART-310; `ART-PATCH.md` there
  says what changed)`
- **`deny.toml`**: after the comment line `#   the alternative was a second filesystem writer of ART's
  own (SD-4).`, add:
  `#   Since ART-310 the crate is vendored and patched (src-tauri/vendor/libpfs3, 0.1.3+art.1); its
  licence field is unchanged.`

- [ ] **Step 5: `ISSUES.md`**

**ART-311**, new, at the top of `## Open`:

```markdown
**ART-311** 🟡 **`libpfs3`'s writer cannot grow the anode index itself: a small-mode PFS3 volume ART
writes holds at most 21 246 anodes whatever its size, and a SUPERINDEX-mode one cannot pass its first
super index block** — *found 2026-09-11 as ART-310's "third limit"; filed 2026-09-13, when ART-310's
format fix left the writer untouched by the owner's decision*
`src-tauri/vendor/libpfs3/src/writer.rs` (`alloc_anode_block`) · pfs3aio allocates index blocks on
demand (`NewIndexBlock`, `anodes.c:717-772`: up to `MAXSMALLINDEXNR` + 1 = 99 in small mode, through
`NewSuperBlock` in SUPERINDEX mode). `libpfs3`'s writer does neither. In small mode it returns
`DiskFull("no index block slot available")` as soon as `rootblock.indexblocks[idx_nr]` is unset
(`writer.rs:1024`), so a volume keeps the one index block the format made: 253 × 84 − 6 = 21 246
anodes. In SUPERINDEX mode it returns `DiskFull("no superindex slot available")` when
`superindex[n]` is unset (`writer.rs:980`), which is reached only past 253² anode blocks.
**How it hurts a user:** content of more than ~21 000 files cannot go on a PFS3 partition under
~5.24 GB. `core::card::sizing::pfs3_small_mode_anode_cap` sizes such content up past MAXSMALLDISK, so a
card spends gigabytes of Work on it. Measured 2026-09-13 with ART-310's format fix applied: 20 655 files
into one directory on a 1 GiB volume, then `disk full: no index block slot available`. Not scheduled;
lifting it changes `core::card::sizing` and its tests, which is the owner's to schedule.
```

**ART-310** — cut the whole entry from `## Open` and paste it at the top of `## Fixed`. Then:
- In its first line, change `🔴 **` to `🔴 ✅ **`.
- At the end of its italic *found …* line, before the closing `*`, add `; fixed 2026-09-13 on
  `art-310-libpfs3-format``.
- Append this paragraph to the end of the entry, with the bracketed parts replaced by the recorded
  values from `plan-run.md`:

```markdown
**The fix (route A), 2026-09-13.** Research
`docs/superpowers/notes/2026-09-11-libpfs3-format-fix-research.md` re-ran this entry's experiment on
Linux (hst-imager 1.6.616, pfs3aio 3.1): five arms, each fix its own variable, every cell as predicted.
It also measured that hst-imager hands a directory made after `libpfs3`'s writes anode **1** on an unfixed
volume. Design `docs/superpowers/specs/2026-09-13-art-310-libpfs3-format-fix-design.md`, plan
`docs/superpowers/plans/2026-09-13-art-310-libpfs3-format-fix.md`.

- **What changed.** `libpfs3` 0.1.3 is vendored as `src-tauri/vendor/libpfs3` (`0.1.3+art.1`, through
  `[patch.crates-io]`; `probe()` says so). Only `src/format.rs` differs: in SUPERINDEX mode a super index
  block is allocated before the anode index block and `rext.superindex[0]` names it, and anodes 0–4 are
  written `(0, 0xFFFFFFFF, 0)` at every size. Its `ART-PATCH.md` carries the diff. `libpfs3`'s own tests
  are not vendored (GPL-3.0-only headers, 9.3 MB fixtures, `sevenz-rust` 0.6's advisories).
- **Tests, each red on 0.1.3 first:**
  - `a_small_pfs3_format_reserves_anodes_zero_to_four` (red: *[message]*)
  - `a_large_pfs3_format_writes_the_superblock_level` (red: *[message]*)
  - `a_large_pfs3_volume_takes_its_own_writes` (red: *[message]*)

  Also `the_pinned_version_constant_matches_cargo_toml` and `probe_names_libpfs3`. The two large tests
  take *[time]* on Linux.
- **Mutations:**
  - M1 anode 0 left free: *[result]*
  - M2 `superindex[0]` back at the index block: *[result]*
  - M3 the super index block never written: *[result]*
  - M4 `[patch]` removed: *[result]*
- **Oracle** (`scripts/pfs3-oracle-check.py`, now with a direction past MAXSMALLDISK and an anode check
  at both sizes), on Linux:
  - on the fix, every check ok, hst-imager's new directory at anode *[n small]* / *[n large]*, the large
    image *[bytes on disk]* on disk;
  - with `[patch]` removed, *[what failed]*.
- **Windows, the owner's machine:** fmt and clippy clean; `cargo test --lib` *[line]* twice; `cargo deny
  check` ok.
- **`plan_card_image`'s rule** ("never `NativeFormatter` past MAXSMALLDISK while ART-310 is open") is
  removed from its doc comment. **Owed by a person:** mount a volume ART formatted past MAXSMALLDISK under
  real pfs3aio in WinUAE, `dir` it and make a drawer. The upstream pull request waits for this.
  **Not in this fix:** the writer's index-block gaps, now ART-311.
```

- [ ] **Step 6: `CHANGELOG.md`, `STATUS.md`, `session-log.md`**

`CHANGELOG.md`, first bullet under `## [Unreleased]` → `### Fixed`:

```markdown
- **A PFS3 partition larger than about 4.9 GB that ART formatted could not be
  mounted, and on a PFS3 partition of any size the Amiga's first new drawer
  could fail with "disk full".** ART now formats PFS3 the way the PFS3 handler
  itself does: large partitions get the index level they need, and the five
  anode numbers the handler reserves are marked reserved. A PFS3 partition ART
  formatted before this should be formatted again before the Amiga writes to it.
```

`docs/STATUS.md`:
- Prepend a new first item inside `### Start here (2026-09-11)`, and change that heading's date to
  `2026-09-13`. The new item gets one more leading zero than the current first item:

  ```markdown
  000000000. **ART-310 is fixed on `art-310-libpfs3-format`, not merged.** `libpfs3` 0.1.3 is vendored
  and patched (`src-tauri/vendor/libpfs3`, `0.1.3+art.1`): the format writes the super index level past
  MAXSMALLDISK and reserves anodes 0–4 at every size. Research, design and plan are under
  `docs/superpowers/` (2026-09-11 note, 2026-09-13 spec and plan). The evidence is in ART-310's Fixed
  entry: three tests seen red on 0.1.3, four mutations, the hst-imager oracle at both sizes on Linux, and
  the owner's two Windows suite runs. `plan_card_image`'s "never `NativeFormatter` past MAXSMALLDISK"
  rule is gone, and **round 3 may format planned cards natively**. ART-311 (the writer's index-block
  gaps) is filed and unscheduled. **Owed by a person:** a patched large volume mounted under real
  pfs3aio in WinUAE, which is also what the prepared upstream patch waits for. **Next:** the owner's word
  to merge, then round 2 (content).
  ```
- Snapshot row *Last updated*: prepend
  `2026-09-13 — ART-310 fixed on `art-310-libpfs3-format` (vendored, patched `libpfs3` 0.1.3+art.1), unmerged. Before that, `
- Snapshot row *Tests — Rust*: prepend the owner's Windows line from Step 1:
  `` `cd src-tauri && cargo test --lib` on `art-310-libpfs3-format`, 2026-09-13: `[line]` (twice, the owner's Windows machine); module runs on Linux through the gate script. Before that, ``

`docs/session-log.md`, new first table row:

```markdown
| 2026-09-13 | **ART-310 fixed on `art-310-libpfs3-format`, unmerged.** Research re-ran ART-310's experiment on Linux with each fix as its own variable (every cell as predicted; hst-imager hands a later directory reserved anode 1 on an unfixed volume). The owner chose, in order: format only, `src/` vendored without `libpfs3`'s tests, `=0.1.3` kept with `[patch]` to `0.1.3+art.1`, and upstream prepared for the owner to open. `src-tauri/vendor/libpfs3`'s `format.rs` writes the super index level and reserves anodes 0–4; three tests read the image's own blocks, red on 0.1.3 first; mutations M1–M4 *[result]*; the oracle gains a direction past MAXSMALLDISK and an anode check. ART-311 filed. | Rust `[Windows line]` ×2; Linux module runs `core::preload::native` *[line]*, `core::card::sizing` *[line]*; fmt, clippy, deny, control-byte sweep clean |
```

- [ ] **Step 7: Sweeps and commit**

- `cd /home/tolon/Belgeler/Projeler/Art && python3 scripts/control-byte-sweep.py`. Expected: `clean`.
- `python3 scripts/scratch-root-sweep.py`. Expected: `clean`.
- `python3 scripts/scratch-guard-sweep.py`. Expected: `clean`.
- `grep -rn 'while ART-310 is open' src-tauri/src docs/STATUS.md`. Expected: no output.

```bash
git add docs/ISSUES.md docs/STATUS.md docs/session-log.md docs/licenses.md CHANGELOG.md THIRD_PARTY_LICENSES.md deny.toml src-tauri/src/core/card/sizing.rs src-tauri/src/core/preload/native.rs
git status --short
git commit -q -F - <<'EOF'
docs: ART-310 fixed - vendored libpfs3 0.1.3+art.1; ART-311 filed

- ISSUES: ART-310 moved to Fixed with the tests' red lines, the mutations, the oracle at both
  sizes and the owner's Windows suite runs; the WinUAE mount owed. ART-311 filed: the writer
  never allocates a second index block (small mode) or super index block (large mode).
- sizing.rs: plan_card_image's "never NativeFormatter past MAXSMALLDISK" rule removed; a test
  doc no longer describes 0.1.3's broken format as current.
- native.rs module doc, licenses.md, THIRD_PARTY_LICENSES.md, deny.toml: libpfs3 is vendored
  and patched; the pfs3aio lineage recorded, not settled.
- CHANGELOG (Fixed), STATUS (Start here, snapshot), session log.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
```

---

### Task 5: Prepare the upstream patch for the owner

The deliverable: a local branch in a clone of `metaneutrons/pfs3` that carries the same change and two
tests, passes upstream's own checks, and has a commit message and PR text written to upstream's rules. It
is **not pushed**: the owner opens the pull request after the WinUAE mount.

**Files (outside ART's repository):**
- Create: `~/Belgeler/Projeler/art-experiments/pfs3-upstream/` (a clone), branch `fix/format-superindex-reserved-anodes`
- Modify there: `crates/libpfs3/src/format.rs`, `crates/libpfs3/tests/format.rs`
- Create there: `PR-BODY.md` (untracked, never committed)
- Modify in ART: `src-tauri/vendor/libpfs3/ART-PATCH.md` (`## Upstream`)

**Interfaces:**
- Consumes: `~/Belgeler/Projeler/art-experiments/art-310-format-patch.py` (Task 2, Step 4).
- Produces: nothing ART's code uses.

- [ ] **Step 1: Clone and branch**

```bash
cd ~/Belgeler/Projeler/art-experiments
git clone https://github.com/metaneutrons/pfs3.git pfs3-upstream
cd pfs3-upstream
git switch -c fix/format-superindex-reserved-anodes
git log --oneline -1
```

Expected: `main`'s head. At planning time that was `05f50b06c3cd`; record the one you get.

- [ ] **Step 2: Write the two failing upstream tests**

Append to `crates/libpfs3/tests/format.rs`:

```rust
/// Reads `len` bytes of the image starting at 512-byte sector `sector`.
fn sectors(path: &Path, sector: u32, len: usize) -> Vec<u8> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).unwrap();
    f.seek(SeekFrom::Start(u64::from(sector) * 512)).unwrap();
    let mut buf = vec![0u8; len];
    f.read_exact(&mut buf).unwrap();
    buf
}

/// pfs3aio's format reserves anodes 0-4 by allocating them (`AllocAnode`
/// leaves clustersize 0, blocknr 0xffffffff, next 0, `anodes.c`). An allocator
/// ported from pfs3aio hands out any anode that reads (0, 0, 0): hst-imager's
/// first `mkdir` on such a volume fails ERROR_DISK_FULL, and a later directory
/// is given reserved anode 1.
#[test]
fn format_reserves_anodes_below_rootdir() {
    // Small mode, and past MAXSMALLDISK (10 241 440 blocks): SUPERINDEX mode.
    for blocks in [8192u64, 10_444_896] {
        let (path, dev) = temp_image(blocks);
        format::format_with_size(&dev, blocks, &FormatOptions::default()).unwrap();
        drop(dev);
        let vol = reopen(&path);
        let rbs = usize::from(vol.rootblock.reserved_blksize);
        let first = if vol.rootblock.options & MODE_SUPERINDEX != 0 {
            let sb = sectors(&path, vol.rootblock_ext.as_ref().unwrap().superindex[0], rbs);
            u32::from_be_bytes(sb[12..16].try_into().unwrap())
        } else {
            vol.rootblock.indexblocks[0]
        };
        let ib = sectors(&path, first, rbs);
        let ab = sectors(&path, u32::from_be_bytes(ib[12..16].try_into().unwrap()), rbs);
        assert_eq!(u16::from_be_bytes([ab[0], ab[1]]), ABLKID, "{blocks} blocks");
        for nr in 0..ANODE_ROOTDIR as usize {
            let at = ANODE_BLOCK_HEADER_SIZE + nr * ANODE_SIZE;
            let anode: Vec<u32> = (0..3)
                .map(|k| u32::from_be_bytes(ab[at + 4 * k..at + 4 * k + 4].try_into().unwrap()))
                .collect();
            assert_eq!(anode, [0, 0xFFFF_FFFF, 0], "{blocks} blocks, anode {nr}");
        }
        std::fs::remove_file(&path).ok();
    }
}

/// In SUPERINDEX mode every reader walks superindex[0] -> SB -> IB -> AB:
/// pfs3aio's GetSuperBlock rejects anything that is not SBLKID, and this
/// crate's own AnodeReader expects the same three levels. A format that points
/// superindex[0] at the index block leaves a volume nothing can read.
#[test]
fn format_superindex_mode_writes_the_sb_level_and_accepts_writes() {
    let blocks = 10_444_896u64;
    let (path, dev) = temp_image(blocks);
    format::format_with_size(&dev, blocks, &FormatOptions::default()).unwrap();
    drop(dev);

    let vol = reopen(&path);
    assert_ne!(vol.rootblock.options & MODE_SUPERINDEX, 0);
    let rbs = usize::from(vol.rootblock.reserved_blksize);
    let sb = sectors(&path, vol.rootblock_ext.as_ref().unwrap().superindex[0], rbs);
    assert_eq!(u16::from_be_bytes([sb[0], sb[1]]), SBLKID);
    let ib = sectors(&path, u32::from_be_bytes(sb[12..16].try_into().unwrap()), rbs);
    assert_eq!(u16::from_be_bytes([ib[0], ib[1]]), IBLKID);

    drop(vol);
    // `reopen` is read-only; the writer needs a read-write volume.
    let mut w = libpfs3::writer::Writer::open(Volume::open_rw(&path, 0).unwrap()).unwrap();
    w.create_dir("Dir").unwrap();
    w.write_file("Dir/File", b"hello").unwrap();
    drop(w);
    let mut vol = reopen(&path);
    assert_eq!(vol.read_file("Dir/File").unwrap(), b"hello");
    std::fs::remove_file(&path).ok();
}
```

- [ ] **Step 3: Run them to see them fail**

Run: `cd ~/Belgeler/Projeler/art-experiments/pfs3-upstream && cargo test -p libpfs3 --test format`
(`rust-toolchain.toml` makes rustup fetch 1.98.1 on first use).

Expected: `2 failed`:
- `format_reserves_anodes_below_rootdir`: `left: [0, 0, 0]`, `right: [0, 4294967295, 0]`
- `format_superindex_mode_writes_the_sb_level_and_accepts_writes`: `left: 18754` (`IB`), `right: 21314`

- [ ] **Step 4: Apply the patch, add `SBLKID` to the import list, format**

```bash
cd ~/Belgeler/Projeler/art-experiments/pfs3-upstream
python3 ~/Belgeler/Projeler/art-experiments/art-310-format-patch.py crates/libpfs3/src/format.rs
python3 - <<'EOF'
p = "crates/libpfs3/src/format.rs"
s = open(p, encoding="utf-8").read()
old = "MODE_SPLITTED_ANODES, MODE_SUPERINDEX, put_u16"
assert s.count(old) == 1, s.count(old)
open(p, "w", encoding="utf-8").write(s.replace(old, "MODE_SPLITTED_ANODES, MODE_SUPERINDEX, SBLKID, put_u16"))
EOF
cargo fmt --all
```

- [ ] **Step 5: Upstream's own checks**

Run each, unpiped:
- `cargo fmt --all -- --check`
- `cargo clippy -p libpfs3 --all-targets --all-features -- -D warnings`
- `cargo test -p libpfs3`
- `cargo deny check`

Expected: all exit 0, and `cargo test -p libpfs3` shows the two new tests passing with every existing test.
`--workspace` pulls in `pfs3-fuse`, which needs `libfuse3` headers. If the owner wants the workspace-wide
run, hand them `! sudo pacman -S fuse3` and run `cargo clippy --workspace …` and `cargo nextest run
--workspace …` after.

- [ ] **Step 6: Commit (upstream's rules, no AI trailer) and write the PR body**

```bash
cd ~/Belgeler/Projeler/art-experiments/pfs3-upstream
git add crates/libpfs3/src/format.rs crates/libpfs3/tests/format.rs
git commit -q -F - <<'EOF'
fix(libpfs3): write the super index level and reserve anodes 0-4 on format

format_with_size pointed rootblock_ext.superindex[0] straight at the anode
index block in SUPERINDEX mode. Every reader walks superindex -> SB -> IB -> AB
(pfs3aio's GetSuperBlock rejects a block that is not SBLKID; this crate's own
AnodeReader and writer expect the same), so a partition over MAXSMALLDISK
formatted here could not be read back, by this crate ("anode 5 not found") or
by hst-imager.

It also left anodes 0-4 as (0, 0, 0). pfs3aio's format reserves them through
AllocAnode (clustersize 0, blocknr 0xffffffff, next 0); an allocator ported from
pfs3aio treats (0, 0, 0) as free, so hst-imager's first mkdir on such a volume
fails ERROR_DISK_FULL and a later directory is given anode 1.

The super index block is allocated before the anode index block, which is
pfs3aio's order; reserved-area sizing and everything else are unchanged.
EOF
git log --format='%an <%ae>%n%B' -1
```

Expected: the author is the owner's git identity, and the message has no `Co-Authored-By` line.

Write `PR-BODY.md` in the clone (untracked):

```markdown
## What

`format_with_size` wrote two structures differently from pfs3aio's `format.c`:

1. **SUPERINDEX mode:** `superindex[0]` pointed at the anode index block instead of a super index
   (`SB`) block, while every reader walks `superindex → SB → IB → AB`: pfs3aio's `GetSuperBlock`,
   hst-imager's port, and this crate's own `AnodeReader`.
2. **Anodes 0–4** were left `(0, 0, 0)`. pfs3aio reserves them during format (`AllocAnode` leaves
   `clustersize 0, blocknr 0xffffffff, next 0`).

## Evidence

Measured with hst-imager 1.6.616 (a C# port of pfs3aio) on RDB images carrying pfs3aio 3.1 as the
`PDS\3` driver. At 1 GiB and 6 GiB, each fix was applied as its own variable:

| Format | 1 GiB | 6 GiB |
|---|---|---|
| unpatched | hst lists; hst's first `mkdir` fails `ERROR_DISK_FULL`; a later hst directory gets anode 1 | hst: `NullReferenceException`; libpfs3: `anode 5 not found` |
| SB level only | as unpatched | mounts; hst's first `mkdir` still fails |
| reserved anodes only | all ok | still unreadable |
| both (this PR) | all ok | all ok; the layout matches hst-imager's own format down to the root directory block |

The existing test suite passes unchanged; two tests are added in `tests/format.rs`.

## Not changed

The writer's `alloc_anode_block` still never allocates a second index block in small mode, or a second
super index block, where pfs3aio's `NewIndexBlock` / `NewSuperBlock` would. That is a separate change.
```

- [ ] **Step 7: Record the upstream status in ART**

In `src-tauri/vendor/libpfs3/ART-PATCH.md`, replace the body of `## Upstream` with the following, where
`<sha>` is `git -C ~/Belgeler/Projeler/art-experiments/pfs3-upstream rev-parse --short HEAD` and `<base>` is
Step 1's head:

```markdown
Prepared, not offered. Branch `fix/format-superindex-reserved-anodes` (`<sha>`, on `main` at `<base>`) in
a local clone of `metaneutrons/pfs3`: the same change to `crates/libpfs3/src/format.rs`, two tests in
`tests/format.rs` (seen red on `main`, green on the change), upstream's fmt, clippy, test and deny checks
clean, and a Conventional Commit with no AI attribution, as upstream's `CONTRIBUTING.md` requires. The
owner opens the pull request after a patched volume has been mounted under real pfs3aio.
```

```bash
cd /home/tolon/Belgeler/Projeler/Art
git add src-tauri/vendor/libpfs3/ART-PATCH.md
git commit -q -F - <<'EOF'
ART-310 task 5: upstream patch prepared for the owner to open

ART-PATCH.md records the local metaneutrons/pfs3 branch carrying the same format.rs change
and two tests (red on upstream main, green on the change, upstream's checks clean); not
pushed, offered after the WinUAE mount.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
```

---

## Self-review (done while writing)

- **Spec coverage:**
  - § 2.1 format only; ART-311 → Task 4, Step 5.
  - § 2.2 vendor without tests → Task 1, Steps 4–5.
  - § 2.3 pin and `[patch]` → Task 1, Steps 2, 6.
  - § 2.4 upstream → Task 5.
  - § 3 the change → Task 2, Step 4.
  - § 4 layout and licence files → Task 1, Steps 4–5; fmt, clippy, deny and the sizing time → Task 1, Step 8.
  - § 5 tests, red and mutations → Task 2, Steps 2–3, 9; the large test's cost → Task 2, Step 5 and Task 3, Step 4.
  - § 6 oracle → Task 3.
  - § 7 close and the rule → Task 4, Steps 1–2, 5.
  - § 8 documents → Task 4, Steps 2–6.
  - § 9 exclusions → no task touches them.
- **Deviation from the spec, stated:** § 7 names the oracle *"on the owner's Windows machine"*. This plan
  runs it on Linux with hst-imager's linux-x64 build of the same release (Task 3), after the owner moved
  the work to this machine. The Windows suite run stays the owner's (Task 4, Step 1).
- **Names used across tasks:**
  - `LIBPFS3_VERSION`, `formatted_large_pds3_image`, `anode_chain`/`AnodeChain`
  - `write_pfs3_volume_for_oracle`, `pfs3_anode_for_oracle_when_asked`, `dh0`/`SEP`
  - `art-linux-run.sh`, `art-310-format-patch.py`, `plan-run.md`
