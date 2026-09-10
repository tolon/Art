# ART-281 — every test scratch removes itself — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** No test in the Rust suite leaves a directory behind: every local `scratch()` helper hands its caller a `core::ScratchDir` guard, a failed removal is said on stderr, a sweep keeps the pattern from coming back, and the fix is proved by counting the scratch root before and after the suite.

**Architecture:** `core::ScratchDir` (ART-184's `Drop`-removed scratch) gains `pair(prefix, tag) -> (ScratchDir, PathBuf)`, and each of the 63 bare-`PathBuf` helpers becomes a one-line wrapper over it, so a call site changes from `let dir = scratch("x")` to `let (_guard, dir) = scratch("x")` and `dir` stays the `PathBuf` the test body already uses (the shape `core/whdload/install.rs` has used since ART-242). A helper that creates a scratch and returns a path derived from it returns the guard too. `scripts/scratch-guard-sweep.py` fails on a test `fn scratch(` returning a bare path and on a `scratch(` call not bound to a tuple pattern; it joins CI's blocking sweeps. The proof is a controlled experiment per batch and for the whole suite: the count of entries under the scratch root before and after `cargo test`, defective arm beside the fixed one.

**Tech Stack:** Rust 1.93 (tests only; no production code changes except `ScratchDir` itself), Python 3 for the two scripts, GitHub Actions for the CI row.

**Spec:** `docs/ISSUES.md` ART-281 (the measured cause, the named fix) and `.superpowers/sdd/2026-09-08-intake/scratch-leak-investigation.md` (the experiment that attributed it); design approved by the owner in chat on 2026-09-10.

## Global Constraints

- Branch `art-281-scratch`, cut from `main` at `ad61a7a`; `git branch --show-current` before every commit; never `git checkout --`, never `git stash`; commit messages through a scratchpad file and `git commit -F <file> -- <paths>` (paths named, so a concurrent worker's staged files are never swept in).
- No production behaviour changes. `core/` stays independent (no new crates). `#[cfg(test)]` only, except `ScratchDir`'s own `Drop`.
- A scratch root entry count is measured with `python scripts/scratch-residue.py` (Task 1) — never `ls | wc -l` through a pipe that can hide a failure — and quoted with both arms.
- `cargo test <module>` output is finished when it says `test result:`; quote that line. Never pipe `cargo test`.
- A helper's prefix stays what it was (`art-osinstall`, `art-mount`, `art-rdb`, …) so an old run's residue can still be attributed by name.
- `_guard` is a real binding and lives to the end of its scope; a bare `_` drops at once and is the defect back in one character. The sweep refuses `let (_, `.
- Scratchpad: `C:\Users\ismoz\AppData\Local\Temp\claude\D--Projeler-Amiga\06212805-e1dd-4b6f-b2ae-dd6f17219be6\scratchpad`.
- `D:\tmp\art-tests` is the scratch root (`src-tauri/.cargo/config.toml`). Nothing under it is deleted by a task; the owner's pre-today sweep runs before Task 1 and is the controller's.

---

### Task 1: `ScratchDir::pair`, a `Drop` that speaks, the residue counter, the sweep

**Files:** modify `src-tauri/src/core/mod.rs` (the `ScratchDir` block); create `scripts/scratch-residue.py`, `scripts/scratch-guard-sweep.py`.

**Interfaces produced:** `ScratchDir::pair(prefix: &str, tag: &str) -> (ScratchDir, PathBuf)`; `python scripts/scratch-residue.py` prints one line `entries under D:\tmp\art-tests: N` and exits 0; `python scripts/scratch-guard-sweep.py` exits 1 while any offender exists and prints each as `path:line: <why>`.

- [ ] In `core/mod.rs`, after `join`, add:

```rust
    /// The guard and the path it guards, for the one-line local helpers every
    /// test module carries (ART-281): `let (_guard, dir) = scratch("x")` keeps
    /// the directory until the test's scope ends and leaves `dir` the plain
    /// `PathBuf` the body already used. `_guard` — never `_`, which drops at
    /// once and is the leak back in one character.
    pub fn pair(prefix: &str, tag: &str) -> (Self, std::path::PathBuf) {
        let guard = Self::new(prefix, tag);
        let dir = guard.0.clone();
        (guard, dir)
    }
```

- [ ] Replace `Drop`:

```rust
#[cfg(test)]
impl Drop for ScratchDir {
    fn drop(&mut self) {
        // Said, not swallowed (ART-281): on Windows `remove_dir_all` fails
        // while any handle in the tree is still open, and a scratch that
        // survives silently is how 763 GB accumulated. A panic here would
        // abort a panicking test twice; stderr is the honest middle.
        if let Err(err) = std::fs::remove_dir_all(&self.0) {
            if self.0.exists() {
                eprintln!("ScratchDir: {} not removed: {err}", self.0.display());
            }
        }
    }
}
```

- [ ] Test beside `independence` in `core/mod.rs`: `scratch_pair_removes_its_directory_when_the_guard_drops` — `let (guard, dir) = ScratchDir::pair("art-core", "pair"); assert!(dir.is_dir()); drop(guard); assert!(!dir.exists());` and `scratch_pair_keeps_the_directory_while_the_guard_lives` — write a file under `dir`, read it back, guard still in scope.
- [ ] `scripts/scratch-residue.py`: reads `TMP` from `src-tauri/.cargo/config.toml` (parse the `TMP = { value = "..." }` line; fall back to `D:/tmp/art-tests`), counts direct entries with `os.scandir`, prints `entries under <root>: <N>`; `--json` prints `{"root":…, "entries":N}`. No deletion, no recursion.
- [ ] `scripts/scratch-guard-sweep.py`, modelled on `scratch-counter-sweep.py`'s header style. Rules over every `.rs` under `src-tauri/src`, test code only (after the file's first `#[cfg(test)]`):
  1. a line matching `fn scratch\(` whose signature (same line) returns `PathBuf` or `std::path::PathBuf` and not `ScratchDir` → offender "returns a bare path";
  2. a call `scratch(` (not the definition, not in a comment) whose statement starts `let <ident> =` rather than `let (<ident>, <ident>) =` → offender "bound without its guard"; `let (_, ` → offender "guard dropped at once";
  3. a helper `fn` (not `#[test]`) whose body calls `scratch(` and whose return type is `PathBuf`/`Path`-shaped without a `ScratchDir` → offender "returns a path whose guard it drops" (walk back to the enclosing `fn` at lower indent, as the counter sweep does).
  Exit 1 with the list, 0 with `scratch-guard sweep: clean — N helpers, M call sites`. Run it now: it must list the 63 helpers and their sites (quote the totals in the report).
- [ ] `cd src-tauri && cargo test core::scratch_pair` → `test result: ok`; `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`.
- [ ] Commit: `ART-281: ScratchDir::pair, a Drop that says when it failed, the residue counter and the guard sweep`.

### Tasks 2–5: the conversion, one batch each

Each batch is the same procedure over its own files. **Before touching a file** in the batch, measure the defective arm: `python scripts/scratch-residue.py` → `N0`; `cd src-tauri && cargo test <module path> --lib` (one per top module in the batch; quote `test result:`); `scratch-residue.py` → `N1`; record `N1 − N0` (expected > 0). Then convert; then the fixed arm the same way (expected `+0`). Both arms and every `test result:` go in the task report.

Conversion per file:

1. The helper: `fn scratch(tag: &str) -> PathBuf { … create_dir_all … dir }` → `fn scratch(tag: &str) -> (crate::core::ScratchDir, PathBuf) { crate::core::ScratchDir::pair("<the prefix the format! had, without the trailing -{tag}>", tag) }` — the prefix string stays verbatim. A file with two helpers (`core/volume/mount.rs`) converts both. A `pub fn scratch` (`core/osinstall/mod.rs`'s `fixtures`) converts in place; every module that calls `fixtures::scratch` is in the same batch.
2. Every `let X = scratch(…)` / `let X = fixtures::scratch(…)` / `super::scratch(…)` / `crate::…::scratch(…)` → `let (_guard, X) = …`. Two in one test both use `_guard`: shadowing does not drop the first.
3. A helper `fn` that calls `scratch` and returns a path or a struct holding one now returns the guard too — as a tuple `(ScratchDir, T)` or a field `_guard: ScratchDir` on the struct — and its callers bind it. The sweep's rule 3 lists these.
4. `cargo test <module> --lib` green (`test result:`), `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `python scripts/scratch-guard-sweep.py` no longer names the batch's files.
5. Commit with `-- <the batch's files>`: `ART-281: <batch name> scratch helpers hand out their guard`.

| Task | Batch | Files (helpers) | Sites (≈) |
|---|---|---|---|
| 2 | **osinstall + preload** | `core/osinstall/{mod,verify}.rs` (+ every `fixtures::scratch` caller under `core/osinstall/`), `commands/osinstall.rs`, `core/preload/{mod,native,pfs3dev}.rs` | 226 + 36 |
| 3 | **volume + card + rom + hdf** | `core/volume/{checkout,device,journal,mount,write/copy,write/mod}.rs`, `core/card/{build,health,manifest,mod,payload}.rs`, `core/rom/mod.rs`, `core/hdf.rs` | 59 + 35 + 19 + 7 |
| 4 | **gameindex + layout + artwork + launch** | `core/gameindex/{igame,igamewrite,scan,store,readers/{drawer,lhadrawer,rp9,whdhdf}}.rs`, `core/layout/{apply,mod,presence,scan}.rs`, `core/artwork/{local,rebind}.rs`, `core/launch/{extract,whdload_boot}.rs` | 98 + 60 + 13 + 8 |
| 5 | **commands + archive + the rest** | `commands/{adf,archive,archives,card,cbm,checkout,launch,panel,preload,volume,volume_write,whdload,winuae}.rs`, `core/archive/{extract,lha,mod,sevenz,zip}.rs`, `core/lha/safe_extract.rs`, `core/safety/{atomic,backup,mod}.rs`, `core/hostfs.rs`, `core/dirsize.rs`, `core/oplog/jsonl.rs`, `core/sources/install.rs`, `tools/recycle_bin.rs` | 133 + 24 + 6 + 18 + 10 + 7 + 6 + 3 + 1 |

Already-converted files are left alone: `core/whdload/install.rs` (tuple), `core/rom/place.rs`, `core/osinstall/chain.rs`, `core/amigainstall/{packagevol,run,workvol}.rs` (return `ScratchDir`; the sweep accepts both shapes).

Tasks 2 and 3 may run concurrently, then 4 and 5 (disjoint files; cargo serialises the build on its own lock; commits name their paths).

### Task 6: the proof, CI, the docs

**Files:** `.github/workflows/ci.yml`, `docs/ISSUES.md`, `docs/STATUS.md`, `docs/session-log.md`, `docs/testing.md`, `CLAUDE.md`'s command list (the sweep line), `CHANGELOG.md` (one line under Internal, if the file has such a heading; else none).

- [ ] `python scripts/scratch-guard-sweep.py` → clean, totals quoted.
- [ ] The whole-suite proof: `scratch-residue.py` → `N2`, `cargo test --lib` unpiped to a file (quote `test result:`), `scratch-residue.py` → `N3`; expected `N3 − N2 = 0`, or exactly the lines the `Drop` reported on stderr (grep the log for `ScratchDir:` and quote every one). Run it **twice** (ART-059). The defective arm of the whole suite is **not** re-run: it was measured on 2026-09-08 (28 568 entries in one day, ISSUES ART-281) and re-running it would put tens of GB back; the per-batch defective arms in Tasks 2–5 are this round's control.
- [ ] `ci.yml`: a step `Check every test scratch hands out its guard` → `python scripts/scratch-guard-sweep.py`, beside `scratch-root-sweep.py`.
- [ ] `docs/ISSUES.md`: ART-281 → Fixed, with the test names, the sweep, and the four numbers; ART-184 → Fixed by the same round if it is still Open (read it first). `docs/testing.md` "Test scratch removes itself on Drop" section gains the `pair` shape and the sweep name. `CLAUDE.md` command block: the new sweep on the line after `scratch-counter-sweep.py`, and the "first three of those four are blocking" sentence corrected to the real count. STATUS "Start here" — the item about ART-281 edited in place; session-log row with the numbers.
- [ ] Commit: `ART-281: the suite leaves nothing behind — the proof, the sweep in CI, the docs`.

## Self-review

**Spec coverage:** ISSUES' four-part fix — guards (Tasks 2–5), a `Drop` that says so (Task 1), the sweep (Task 1, CI in Task 6), the owner's deletion (the controller's, before Task 1). **Placeholders:** the prefixes are read from each file by the implementer; the batch table names every one of the 63 files (68 helpers minus the 6 already converted; ISSUES counted 63 of ~69). **Type consistency:** `pair` returns `(ScratchDir, PathBuf)` everywhere; the sweep's rule 2 accepts exactly the `let (<ident>, <ident>) =` shape `pair` produces.
