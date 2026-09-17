# One-button card — Round 3: the WHDLoad phase, `card_os_prepare` / `card_os_build`, `.partial`, the end-to-end card

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** From a system tree ART has already built and a list of named partitions with their sources, ART prepares (measures, stages, proposes Kickstarts) and then builds one complete PiStorm card image — boot partition, RDB, every PFS3 partition formatted and filled, WHDLoad and the agreed Kickstarts in System — under `<image>.partial`, renamed only when the check passes.

**Architecture:** A new top-level core module `core/cardos/` sits above `core/card`, `core/preload`, `core/whdload`, `core/gameindex` and `core/rom`, and holds everything this round adds to the core: `content.rs` moves into it (ART-339), and it gains `driver.rs` (find `pfs3aio`), `whdload.rs` (choose WHDLoad by `$VER:`, write it into a tree), `kickstarts.rs` (what the titles want, the proposal, the agreed placements with their `.RTB`), `partial.rs` (the `.partial` name) and `prepare.rs` (measure → free-space needs → stage). The command layer (`commands/cardos.rs`) owns a **session** — one scratch folder under the scratch root, holding the tree, the staging and the driver — from `card_os_open` to the build's end or `card_os_close`, through a new product guard `core::scratch_guard::OwnedScratch` that names a folder it could not remove (ART-340). The build maps the prepared card onto the existing `card_build` path and `commands/preload.rs::run_with_fallback`, made `pub(crate)`.

**Tech Stack:** Rust (MSRV 1.93). One new **direct** dependency outside `core/`: `windows-sys` **0.61.2** (already in `Cargo.lock` through other crates) with feature `Win32_Storage_FileSystem`, for `GetDiskFreeSpaceExW` in `tools/free_space.rs`. TypeScript: a typed wrapper `src/lib/cardOs.ts` only — no screen.

**Spec:** `docs/superpowers/specs/2026-09-11-one-button-card-design.md` (§ 5, § 6, § 7, § 8.3, § 8.6 are this round).
**Research (binding unless the owner's decisions override):**
`.superpowers/sdd/2026-09-17-one-button-card-round-3/research-tree.md` (T — the tree on 2026-09-17),
`.superpowers/sdd/2026-09-17-one-button-card-round-3/research-whdload.md` (W — WHDLoad's own documents and three builders' code),
`.superpowers/sdd/2026-09-17-one-button-card-round-3/partial-exp.py` + `partial-exp-run2.out` (P — hst-imager and the `.partial` name).

## The owner's decisions (2026-09-17)

1. **Kickstarts: listed before, placed on the build.** `card_os_prepare` lists every Kickstart the titles want, which ROM of the user's answers it and the name it would be written under; `card_os_build` places **only the names the caller passes back as agreed**. The 2026-08-21 rule ("always a proposal, never a silent copy", `rom/offer.rs:9-19`) stands; design § 5 phase 5's automatic placement is overridden.
2. **`.RTB` files: from the material, else the refusal names Aminet.** ART looks for `skick346.lha` (or a loose `<name>.RTB`) in the material folders; when none is there the proposal says so and names `util/boot/skick346` — which ART's own package catalogue already carries (`core/sources/bundle/catalogue/acilis.json:7`, id `skick`) and the user starts. ART downloads nothing during a build.
3. **`S/WHDLoad.prefs`: the one beside the chosen WHDLoad**, copied unchanged, and only when the tree has none (W § 1.2: the WHDLoad Installer and HstWB both refuse to overwrite one).
4. **Into this round:** ART-339 (card ⇄ preload), ART-341 (`:` names in osinstall), unpacking `pfs3aio.lha`, and filing the `core/whdload` ⇄ `core/gameindex` mutual import as a new ART id.

## Decisions made while planning (the owner confirms at plan approval)

| # | Decision | Why |
|---|---|---|
| P1 | The build's scratch is a **session** a command opens (`card_os_open`) and a registry in Tauri `State` holds; the build's every ending and `card_os_close` remove it and name what could not be removed. | Phases 1–3 are three commands the frontend runs (T § 6); no single Rust stack frame spans phases 1–8, so a stack guard cannot own the folder. |
| P2 | The image is written as `<image>.partial` — the whole name plus `.partial`. | Measured (P): `hst.imager info` and `fs dir <img>\rdb\dh0` gave identical output (16 lines; exit 0, 18 lines, "9 directories") for `control.hdf`, `control.img`, `arm.img.partial`, `arm.partial.img`, the same bytes. **Reads only**; hst-imager *writing* under that name is proven by Task 13's local run, not assumed. A `.vhd` destination is refused (design § 9). |
| P3 | A `<image>.partial` already on disk is **refused**, named, never removed. | ART removes only a file it created in this run. |
| P4 | The Kickstart collection is every material folder plus the folder of the card's boot Kickstart, each scanned by `rom::scan_rom_directory`. | No ROM-folder setting exists for the card flow (T § 2). |
| P5 | System stays **800 MiB** (`MEASURED_SYSTEM_MB`); a tree + WHDLoad + proposed Kickstarts that does not fit is refused **naming System**. | The design's measured constant; nothing checked the tree against it (T § 4). |
| P6 | Titles that need WHDLoad and no WHDLoad found anywhere (tree, material, the partitions' hardfiles) is a **refusal**. | A card whose games cannot start is the confident wrong sentence. |
| P7 | Free space: `GetDiskFreeSpaceExW` via `windows-sys` in `tools/free_space.rs`; the check itself is a pure function in `core/cardos/prepare.rs` taking a closure. | No new crate in the lock; `core/` stays free of platform code. Two paths on one volume are summed by comparing their path prefixes (a mount point inside a folder is not detected — disclosed). |
| P8 | `SizingRefusal::DoesNotFit` gains `largest: Option<String>` — the user partition with the most content — and a new `PartitionContentDoesNotFit { volume_name, needed_blocks, available_blocks }` covers System. | Round 1's owed I4 (T § 7 D5), and a card total that overflows has no single partition to blame. |
| P9 | The WHDLoad phase and the Kickstart proposal live in `core/cardos/`, not `core/whdload/`. | `core/whdload` ⇄ `core/gameindex` already import each other (T § 1); a `system.rs` there would deepen it. |

## What this round satisfies, and what it deliberately does not

| Item | Where |
|---|---|
| Design § 5 phase 4 (content: measure, stage, check) wired to a command | Tasks 10, 12 |
| § 5 phase 5 (WHDLoad + Kickstarts into the tree) — decisions 1–3 | Tasks 8, 9, 12 |
| § 5 phases 6–8 (card image, partitions, check), `.partial`, rename on success | Tasks 5, 6, 12 |
| § 5 "system tree and staging removed in every ending; a folder that cannot be removed is named" | Tasks 3, 12 (ART-340) |
| § 6 refusals before anything is written: no `pfs3aio`, no Emu68 archive, does not fit, image exists, not enough space, unreadable source | Tasks 4, 7, 10, 11, 12 |
| § 6 non-ASCII names: hst-imager when configured, otherwise refused in prepare, typed | Tasks 10, 12 |
| § 6 a stop or failure removes `<image>.partial` and says so; § 53 log | Task 12 |
| § 7 `run_with_fallback` reachable from the card build | Task 12 |
| § 8.3 end to end, read back by ART and by hst-imager | Task 13 |
| § 8.6 mutations | every task; listed in Task 14 |
| T § 4 `build_card` leaves the file after a failed read-back | Task 6 |
| T § 4 `card_check_image` says "volumes need formatting" of a filled card | Task 5 |
| T § 4 the manifest's `os` is always empty; nothing records partitions' contents | Task 5 |
| T § 7 D5 `DoesNotFit` names no partition | Task 4 |
| ART-339, ART-341, ART-340; new id for whdload ⇄ gameindex | Tasks 1, 2, 3, 12, 1 |

**Deliberately not in this round:** every screen (round 4) — the Machine tab's card destination, the Build tab's new phase kinds and their phrase keys; `onTreeWritten` remembering a scratch tree as the user's (round 4, T § 6); fetching `skick346` automatically; WHDLoadWrapper, `WHDLoad.ori`, `NoNetwork`/`NoMMU` edits (W § 2, § 3.7–3.9 — none was asked for); `rom.key` beside placed Kickstarts (`place` writes the **decoded** image, so none is needed); `WHDCOMMON:`; the ART-113 missing-tool error staying untyped in `run_with_fallback` (unreachable from the card path after Task 10's prepare-time refusal; `preload_run` keeps it); ART-324/328/329/331; the round-1 CHANGELOG note (T § 7 D12) — checked and recorded in Task 14, not written unless missing.

## Global Constraints

- `src-tauri/src/core/` is platform-independent: `std`, `serde`, `serde_json`, `sha2`, `md-5`, `log`, `thiserror`, `delharc`, `zip`, `sevenz-rust2`, `quick-xml`, `fatfs`, `libpfs3` — **no new crate in `core/`**; never `use tauri`, no network, no process spawn outside a test, no Windows API.
- **Layering after this round:** `core/cardos` may import `core/card`, `core/preload`, `core/whdload`, `core/gameindex`, `core/rom`, `core/archive`, `core/amigaver`, `core/volume`, `core/adf`, `core/osinstall` (for `DistributionManifest` only). **Nothing below imports `core/cardos`.** `core/card` no longer imports `core/preload` (ART-339 closed); `core/preload` importing `core/card`'s reader stays (downward).
- The one new dependency is `windows-sys = { version = "0.61.2", features = ["Win32_Storage_FileSystem"] }` under `[target.'cfg(windows)'.dependencies]` in `src-tauri/Cargo.toml`, used only in `src-tauri/src/tools/free_space.rs`; `THIRD_PARTY_LICENSES.md` names it in the same commit; `cargo deny check` stays `advisories ok, bans ok, licenses ok, sources ok`.
- Test scratch: `let (_guard, dir) = crate::core::ScratchDir::pair("<prefix>", "<tag>");` — `_guard`, never `_`, tuple guard first; a struct `_guard` field last. Tags unique within one process.
- Fixtures are synthetic and built at runtime: HDFs through `core::cardos::content::test_support::build_hdf` (Task 1), LHA through `core::lha` test writers, ROMs as arbitrary bytes whose CRC-16/ARC the test computes with `core::hashing::crc16_arc`, a slave through `core::gameindex::readers::slave` test builders. **No Amiga content in the repository.** Owner material only in `#[ignore]` hooks gated by an env var.
- Every shell: `TMP` and `TEMP` are `E:\amiga\ProjeART\build\tmp` (`src-tauri/.cargo/config.toml` sets them for cargo); never write to `C:`. Owner material under `E:\amiga\` is read-only. Bash needs `export PATH="/c/Users/ismoz/.cargo/bin:$PATH"`.
- Run commands from `D:\Projeler\Amiga\amiga-retro-toolkit\src-tauri` (Rust) or the repository root (pnpm, scripts), by absolute `cd`. Per task: `cargo test --lib <module path>` only. The whole suite, twice, only in Task 14.
- A test run is finished when it prints `test result:`. **Never pipe** a command whose status matters; redirect to a file under `E:\amiga\ProjeART\build\tmp\card-r3\` and read the `test result:` line.
- Red first: every new test is seen failing for the stated reason before the implementation; every guard that matters gets a **mutation** (back the file up by absolute path to `E:\amiga\ProjeART\build\tmp\card-r3\`, mutate, run, restore with `shutil.copyfile`, never `git checkout --`), and survivors are disclosed in the commit message.
- Commits: `git branch --show-current` must print `art-card-round-3` first; stage files **by name** (never `git add -A`); message written to a file and `git commit -F <file>`; end with `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`. Before each commit run `python scripts/control-byte-sweep.py` unpiped.
- Creating a file is `SAFE_CREATE`. ART removes only what it created in this run: the session folder, `<image>.partial`. A user's file — a stale `.partial`, an existing image, an existing `C/WHDLoad` that differs — is refused or kept, and named.
- Never index a buffer built from file data directly; bound every read (`WHDLOAD_MAX_BYTES = 1 MiB`, `PREFS_MAX_BYTES = 64 KiB`, `RTB_MAX_BYTES = 64 KiB`, `SLAVE_MAX_BYTES = 1 MiB`, `DRIVER_MAX_BYTES = 1 MiB`), and bound every walk (`MAX_WALK_NODES = 200_000`).
- **The failure that does not crash:** endings stay distinct (succeeded · refused · failed · stopped); every refusal names what is missing and the next step; nothing claims a removal that did not happen.
- A new command goes in **both** `invoke_handler![]` (`src-tauri/src/lib.rs:142` area) and `src/lib/cardOs.ts`; a new job title key goes in **both** `src/i18n/en.json` and `tr.json` under `components.jobBar.title`.
- New `CoreError` variants get a distinct `code()` and join `every_variant_has_a_distinct_code` (`src-tauri/src/core/error.rs:822`).
- Paths in scripts are written with the Write tool, never through a heredoc.

## File structure

| File | Responsibility this round |
|---|---|
| `src-tauri/src/core/cardos/mod.rs` (create) | module doc (the layering rule above), `pub mod content; driver; kickstarts; partial; prepare; whdload;` |
| `src-tauri/src/core/cardos/content.rs` (move from `core/card/content.rs`) | unchanged behaviour; a `#[cfg(test)] pub(crate) mod test_support` exposing `build_hdf` |
| `src-tauri/src/core/card/mod.rs` (modify) | `pub mod content;` removed |
| `src-tauri/src/core/mod.rs` (modify) | `pub mod cardos; pub mod scratch_guard;` |
| `src-tauri/src/core/scratch_guard.rs` (create) | `OwnedScratch` — the product guard that reports what it could not remove |
| `src-tauri/src/core/osinstall/apply.rs`, `mod.rs` (modify) | ART-341: `/` and `:` names refused before anything is written; the invented `Prices: 1993` replaced |
| `src-tauri/src/core/preload/amiga_names.rs` (modify) | ART-341: the manifest half refuses `:` too |
| `src-tauri/src/commands/volume_write.rs` (modify) | ART-341: its two `Prices: 1993` tests use `Prices? 1993` |
| `src-tauri/src/core/card/sizing.rs` (modify) | `DoesNotFit { needed, available, largest }`, `PartitionContentDoesNotFit`, `plan_card_image(card_gb, system, content)` |
| `src-tauri/src/core/card/manifest.rs` (modify) | `PartitionContent`, `CardManifest.partitions`, `describe_card(.., os, partitions)` |
| `src-tauri/src/core/card/health.rs` (modify) | `VolumesNeedFormatting` counts only partitions the manifest does not record as filled |
| `src-tauri/src/core/card/build.rs` (modify) | a failed read-back removes the file it created |
| `src-tauri/src/core/cardos/partial.rs` (create) | `partial_path_for`, `refuse_partial_destination`, `finish_partial` |
| `src-tauri/src/core/cardos/driver.rs` (create) | `find_pfs3_driver` — loose or inside `pfs3aio.lha`, version from `$VER:` |
| `src-tauri/src/core/cardos/whdload.rs` (create) | `find_whdload`, `choose_whdload`, `install_whdload` |
| `src-tauri/src/core/cardos/kickstarts.rs` (create) | `wanted_images` (moved from `commands/gameindex.rs`), `find_title_needs`, `propose_kickstarts`, `place_agreed` |
| `src-tauri/src/core/cardos/prepare.rs` (create) | `measure_card`, `space_needs`, `check_free_space`, `stage_card` |
| `src-tauri/src/core/error.rs` (modify) | `CardSourceUnusable`, `CardNamesNeedHstImager`, `CardDoesNotFit`, `NotEnoughSpace`, `WhdloadNotFound`, `Pfs3DriverNotFound`, `PartialImageExists`, `KickstartNotProposed` |
| `src-tauri/src/commands/gameindex.rs` (modify) | `wanted_images` now `core::cardos::kickstarts::wanted_images` |
| `src-tauri/src/tools/free_space.rs`, `tools/mod.rs` (create/modify) | `available_bytes(path)` |
| `src-tauri/Cargo.toml`, `Cargo.lock`, `THIRD_PARTY_LICENSES.md` (modify) | `windows-sys` direct dependency |
| `src-tauri/src/commands/card.rs` (modify) | `write_card_image` split out of `build_requested_card`; `pub(crate)` on it and `report_for` |
| `src-tauri/src/commands/preload.rs` (modify) | `run_with_fallback`, `Stopped`, `StepReport` → `pub(crate)` |
| `src-tauri/src/commands/cardos.rs` (create) | sessions; `card_os_open`, `card_os_prepare`, `card_os_build`, `card_os_close`; `build_card_os` |
| `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs` (modify) | module, `State`, handlers |
| `src/lib/cardOs.ts`, `src/lib/cardOs.test.ts` (create) | typed wrappers, event subscriptions |
| `src/i18n/en.json`, `src/i18n/tr.json` (modify) | `components.jobBar.title.prepareCardOs`, `buildCardOs` |
| `scripts/pfs3-oracle-check.py` (modify) | `--card IMAGE` mode: every PFS3 partition of a card listed by hst-imager and compared with ART's own listing |
| docs | ISSUES (ART-339/340/341 closed, ART-342 new, stale round-2 SDD citations corrected), FEATURES, STATUS, session-log, CHANGELOG, `architecture.md` (the `core/cardos` layer) |

**Order.** The move (Task 1) goes first because every later core task creates files beside `content.rs`. ART-341 (Task 2) is independent and small. The guard (3), sizing (4), records (5) and the build fix (6) are leaves the orchestration needs. Driver, WHDLoad, Kickstarts (7–9) are independent of each other. `prepare.rs` (10) composes 4, 7, 8, 9. Free space (11) is the one platform piece. The command (12) composes everything; the end-to-end card (13) proves it; Task 14 closes.

---

### Task 1: `core/cardos/` — `content.rs` moves out of `core/card` (ART-339), and the whdload ⇄ gameindex cycle is filed

**Files:**
- Create: `src-tauri/src/core/cardos/mod.rs`
- Move: `src-tauri/src/core/card/content.rs` → `src-tauri/src/core/cardos/content.rs` (`git mv`)
- Modify: `src-tauri/src/core/card/mod.rs:19` (remove `pub mod content;`), `src-tauri/src/core/mod.rs` (add `pub mod cardos;`)
- Modify (doc references to `core::card::content`): `src-tauri/src/core/gameindex/readers/lhadrawer.rs:178`, `src-tauri/src/core/preload/mod.rs:897`, `src-tauri/src/core/preload/native.rs:788`, `src-tauri/src/core/cardos/content.rs:38-42`
- Modify: `docs/ISSUES.md` (ART-342 filed; ART-339 moved to Fixed with its test), `docs/FEATURES.md:225` (the path)
- Test: `src-tauri/src/core/independence.rs` (new test beside the existing ones)

**Interfaces:**
- Produces: `crate::core::cardos::content::*` — every item `core::card::content` had, same names and signatures. `#[cfg(test)] pub(crate) mod test_support { pub(crate) fn build_hdf(path: &Path, dirs: &[&str], files: &[(&str, &[u8])]); }` re-exporting the existing private helper of the same name.

- [ ] **Step 1: Write the failing guard.** In `src-tauri/src/core/independence.rs`, add a test that reads every `.rs` file under `src/core/card/` and `src/core/preload/` (walk as the neighbouring `core_never_spawns_a_process_outside_a_test` does) and fails on any line outside a `#[cfg(test)]` block that contains `crate::core::preload` inside `core/card/` **or** `crate::core::cardos` anywhere outside `core/cardos/`:

```rust
#[test]
fn core_card_does_not_import_preload_and_nothing_below_imports_cardos() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("core");
    let mut offenders = Vec::new();
    for file in rust_files_under(&root) {
        let rel = file.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
        let text = std::fs::read_to_string(&file).unwrap();
        for (number, line) in production_lines(&text) {
            let card_imports_preload = rel.starts_with("card/") && line.contains("crate::core::preload");
            let below_imports_cardos = !rel.starts_with("cardos/") && line.contains("crate::core::cardos");
            if card_imports_preload || below_imports_cardos {
                offenders.push(format!("{rel}:{number}: {}", line.trim()));
            }
        }
    }
    assert!(offenders.is_empty(), "layering (ART-339): {offenders:#?}");
}
```

`rust_files_under` and `production_lines` are the helpers the process-spawn test already uses; if they are local closures there, lift them to private `fn`s in the same file first (no behaviour change).

- [ ] **Step 2: Run it, see it fail.**
Run: `cd /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri && cargo test --lib core::independence > /e/amiga/ProjeART/build/tmp/card-r3/t1-red.txt 2>&1`
Expected: FAIL listing `card/content.rs:62`, `:63`, `:67` (the three `crate::core::preload` imports).

- [ ] **Step 3: Move the file.** `git mv src-tauri/src/core/card/content.rs src-tauri/src/core/cardos/content.rs`. Create `core/cardos/mod.rs`:

```rust
//! The one-button card's top: everything that turns a built system tree and a
//! list of named partitions into one card image (design
//! `docs/superpowers/specs/2026-09-11-one-button-card-design.md`).
//!
//! **This module is the top of the card path and nothing below imports it.**
//! It reads sources (`content`), finds the PFS3 driver (`driver`), chooses and
//! writes WHDLoad (`whdload`), proposes and places Kickstarts (`kickstarts`),
//! and measures, checks and stages a card (`prepare`). `core/card` builds the
//! image's shape and `core/preload` fills a volume; neither knows this module
//! exists. `core::independence` holds that (ART-339).

pub mod content;
pub mod driver;
pub mod kickstarts;
pub mod partial;
pub mod prepare;
pub mod whdload;
```

Until Tasks 6–10 create them, declare only `pub mod content;` and add each `pub mod` in the task that creates the file. Remove `pub mod content;` from `core/card/mod.rs`; add `pub mod cardos;` to `core/mod.rs` in alphabetical position. In `content.rs`'s module doc, replace the paragraph "`core/card` importing `core/preload` is accepted for this round (R6) …" with:

```rust
//! This module lives in `core/cardos`, above both `core/card` and
//! `core/preload`, so it may use the copy's own name rules — the ART-113
//! non-ASCII test, the PFS3 name limit, the case fold — without making those
//! two modules import each other (ART-339, fixed 2026-09-17).
```

Fix the three doc references (`core::card::content` → `core::cardos::content`). Expose the fixture: in `content.rs`'s test module the helper `build_hdf` stays where it is; add at the end of the file

```rust
#[cfg(test)]
pub(crate) mod test_support {
    pub(crate) use super::tests::build_hdf;
}
```

and make `fn build_hdf` in `mod tests` `pub(crate) fn build_hdf` (and `mod tests` itself `pub(crate) mod tests` if the compiler requires it).

- [ ] **Step 4: Run the guard and the moved module.**
Run: `cargo test --lib core::independence > …/t1-green-a.txt 2>&1` then `cargo test --lib core::cardos::content > …/t1-green-b.txt 2>&1`
Expected: both `test result: ok.`; the content count equals what `core::card::content` had (read it from `git show HEAD:src-tauri/src/core/card/content.rs | grep -c '#\[test\]'` — **state both numbers in the commit message**).

- [ ] **Step 5: Mutation.** Back up `core/cardos/content.rs`; add `use crate::core::cardos::content as _;` to `src-tauri/src/core/card/sizing.rs` production code (a below-imports-cardos case) → the guard must go red naming `card/sizing.rs`. Restore with `shutil.copyfile`; restore `sizing.rs` the same way. Second mutation: in a copy of `core/card/mod.rs` add `use crate::core::preload as _;` → red naming `card/mod.rs`. Restore.

- [ ] **Step 6: File ART-342 and close ART-339.** In `docs/ISSUES.md` Open, add after ART-341:

```markdown
**ART-342** 🔵 **`core/whdload` and `core/gameindex` import each other** — *found 2026-09-17 by card round 3's research
(`.superpowers/sdd/2026-09-17-one-button-card-round-3/research-tree.md` § 1), by reading; not fixed*
`src-tauri/src/core/whdload/install.rs` imports `core::gameindex` (`:624`, `:713-716`, `:1238-1240`) while
`core/gameindex/readers/drawer.rs:24`, `lhadrawer.rs:67` and `whdhdf.rs:37` import `core::whdload` — the shape ART-339
was. **Nothing is broken by it**; neither module can be lifted into a crate alone. Card round 3 put its own WHDLoad work
in `core/cardos/` rather than deepen it. **Fix direction:** move `install.rs` (the volume installer, which needs the
catalogue) up out of `core/whdload`, leaving `core/whdload` the pure layout analysis both use.
```

Move ART-339 to Fixed with: "Fixed 2026-09-17 by card round 3, Task 1: `content.rs` moved to `core/cardos/`, the top of the card path. Guard: `core::independence::core_card_does_not_import_preload_and_nothing_below_imports_cardos` (red first on the three imports; two mutations red)." Correct ART-339's text where it says "on `art-card-round-2`, unmerged" and where it cites the round-2 ledger: "(the round-2 SDD folder is not on disk; the plan `docs/superpowers/plans/2026-09-16-one-button-card-round-2.md` records ruling R6)". Also add the two import sites the entry missed (`preload/mod.rs:57`, `preload/native.rs:106`), noting they are downward and stay. FEATURES.md:225: `core/card/content.rs` → `core/cardos/content.rs`.

- [ ] **Step 7: Commit.** `git branch --show-current` → `art-card-round-3`. Stage by name: `src-tauri/src/core/cardos/mod.rs src-tauri/src/core/cardos/content.rs src-tauri/src/core/card/content.rs src-tauri/src/core/card/mod.rs src-tauri/src/core/mod.rs src-tauri/src/core/independence.rs src-tauri/src/core/gameindex/readers/lhadrawer.rs src-tauri/src/core/preload/mod.rs src-tauri/src/core/preload/native.rs docs/ISSUES.md docs/FEATURES.md`. Message: `core/cardos: content.rs above card and preload (ART-339); ART-342 filed`.

---

### Task 2: `:` and `/` are not AmigaDOS names — osinstall refuses them before anything is written (ART-341)

**Files:**
- Modify: `src-tauri/src/core/osinstall/apply.rs` (`refuse_host_name_collisions`, called at `:1381` and `:2055`; the fixture `planned_with_host_hostile_names` ~`:2596`; tests at ~`:2666`, `:2749`, `:2768`, `:2787`, `:2918`, `:2968`; doc comments `:275-277`, `:958`, `:2591`, `:2660-2661`)
- Modify: `src-tauri/src/core/osinstall/mod.rs` (doc comments `:676`, `:744-745`, `:791`)
- Modify: `src-tauri/src/core/preload/amiga_names.rs` (`from_records`, `:226-250`)
- Modify: `src-tauri/src/commands/volume_write.rs` (`:1655`, `:3687-3688`, `:4212`, `:4234`)
- Test: inline in `apply.rs` and `amiga_names.rs`

**Interfaces:**
- Produces: `CoreError::AmigaNameReserved { path: String }` — Display: `"'{path}' cannot be an AmigaDOS name: a colon (:) or a slash (/) is reserved in AmigaDOS file and drawer names. Rename it in the source and run again."`, code `"ART-AMIGA-NAME-RESERVED"`.

- [ ] **Step 1: Failing tests.** In `apply.rs` tests:

```rust
#[test]
fn a_name_with_a_colon_is_refused_before_anything_is_written() {
    let (_guard, dir) = scratch("colon-name");
    let destination = dir.join("tree");
    // The fixture every name test here uses, with one AmigaDOS-illegal name.
    let plan = planned_with_names(&dir, &["Devs/Prices: 1993"]);
    let err = apply_staging_in(&plan, &destination, &dir, &crate::core::clock::UTC_FOR_TESTS, &NoProgress)
        .unwrap_err();
    assert_eq!(err.code(), "ART-AMIGA-NAME-RESERVED");
    assert!(err.to_string().contains("'Devs/Prices: 1993'"), "{err}");
    assert!(!destination.exists(), "nothing may be written before the refusal");
}
```

(`planned_with_names` is `planned_with_host_hostile_names` generalised to take the list; if the existing fixture hard-codes its names, add the parameterised form beside it and make the old one call it. Use whichever test clock the neighbouring tests use.) In `amiga_names.rs` tests:

```rust
#[test]
fn the_manifest_half_refuses_a_colon_as_the_record_half_does() {
    let records = vec![record_with_amiga_path("Devs/Prices: 1993")];
    let names = AmigaNames::from_records(&records);
    assert_eq!(names.name_for("Devs/Prices_ 1993"), None);
}
```

(`record_with_amiga_path` is whatever builder the neighbouring `from_records` tests use.)

- [ ] **Step 2: Run, see both fail.** `cargo test --lib core::osinstall::apply > …/t2-red-a.txt 2>&1` — the new test fails because apply writes the tree (or fails later with `check_name`'s sentence); `cargo test --lib core::preload::amiga_names > …/t2-red-b.txt 2>&1` — the record is accepted.

- [ ] **Step 3: Implement.** In `error.rs` add the variant, its code, and add it to `every_variant_has_a_distinct_code`. In `apply.rs::refuse_host_name_collisions`, before the collision fold, add:

```rust
for item in items {
    let amiga = item.to.as_str();
    if amiga.split('/').any(|segment| segment.contains(':')) {
        return Err(CoreError::AmigaNameReserved { path: amiga.to_string() });
    }
}
```

(`item.to` is the tree-relative Amiga path the function already reads; `/` inside a segment cannot occur because the path is split on `/` — a leading, trailing or doubled `/` is an empty segment, refused with the same variant: add `|| segment.is_empty()`.) In `amiga_names.rs::from_records`, beside `is_one_segment`, reject a record whose name contains `:`. Replace every `Prices: 1993` literal in the listed tests with `Prices? 1993` (legal on the Amiga, hostile to Windows) and adjust each test's expected escaped host name to what `windows_safe_name` produces for `?` — read it off the red run, never guess. Correct the doc comments to say `:` and `/` are reserved (AmigaOS Manual, *AmigaDOS: Working With AmigaDOS*, § Naming Conventions).

- [ ] **Step 4: Green.** Both module runs `test result: ok.`; also `cargo test --lib commands::volume_write > …/t2-green-c.txt 2>&1`.

- [ ] **Step 5: Mutations.** M2a: the `contains(':')` check removed → the apply test red. M2b: the `from_records` `:` filter removed → the amiga_names test red. Restore each with `shutil.copyfile`.

- [ ] **Step 6: Commit.** Stage the five files by name. Message: `osinstall: ':' and '/' names refused before the copy (ART-341)`. ART-341 moves to Fixed in Task 14 with these test names.

---

### Task 3: `OwnedScratch` — a product scratch guard that names what it could not remove

**Files:**
- Create: `src-tauri/src/core/scratch_guard.rs`; Modify: `src-tauri/src/core/mod.rs`
- Test: inline

**Interfaces:**
- Produces:

```rust
/// A folder ART created under the scratch root and must remove on every ending.
pub struct OwnedScratch { path: PathBuf, finished: bool }

/// What removing it left behind.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeftBehind { pub path: String, pub why: String }

impl OwnedScratch {
    /// A fresh, empty folder `<root>/<prefix>-<pid>-<n>`. Refuses (SafetyRefused) when that name exists.
    pub fn create_in(root: &Path, prefix: &str) -> CoreResult<Self>;
    pub fn path(&self) -> &Path;
    /// Remove it now. `Ok(())` when it is gone; `Err(LeftBehind)` naming it when it is not.
    pub fn finish(mut self) -> Result<(), LeftBehind>;
}
impl Drop for OwnedScratch { /* best effort when finish was not called; logs with log::warn! when it fails */ }
```

- [ ] **Step 1: Failing tests.**

```rust
#[test]
fn finish_removes_the_folder_and_everything_in_it() {
    let (_guard, root) = crate::core::ScratchDir::pair("art-owned-scratch", "finish");
    let owned = OwnedScratch::create_in(&root, "card-os").unwrap();
    let inner = owned.path().join("staging").join("a.txt");
    std::fs::create_dir_all(inner.parent().unwrap()).unwrap();
    std::fs::write(&inner, b"x").unwrap();
    let path = owned.path().to_path_buf();
    assert_eq!(owned.finish(), Ok(()));
    assert!(!path.exists());
}

#[test]
fn a_folder_that_cannot_be_removed_is_named_not_claimed_gone() {
    let (_guard, root) = crate::core::ScratchDir::pair("art-owned-scratch", "held");
    let owned = OwnedScratch::create_in(&root, "card-os").unwrap();
    let held = owned.path().join("held.bin");
    // An open handle keeps a file (and so its folder) on Windows.
    let _handle = std::fs::File::create(&held).unwrap();
    let path = owned.path().display().to_string();
    let left = owned.finish().unwrap_err();
    assert_eq!(left.path, path);
    assert!(!left.why.is_empty());
}

#[test]
fn two_guards_in_one_process_never_share_a_folder() {
    let (_guard, root) = crate::core::ScratchDir::pair("art-owned-scratch", "unique");
    let a = OwnedScratch::create_in(&root, "card-os").unwrap();
    let b = OwnedScratch::create_in(&root, "card-os").unwrap();
    assert_ne!(a.path(), b.path());
}

#[test]
fn dropping_without_finish_still_removes_it() {
    let (_guard, root) = crate::core::ScratchDir::pair("art-owned-scratch", "drop");
    let path = OwnedScratch::create_in(&root, "card-os").unwrap().path().to_path_buf();
    assert!(!path.exists());
}
```

The held-file test is Windows-only in effect; mark it `#[cfg(windows)]` (CI is Windows).

- [ ] **Step 2: Red** (the module does not exist → compile error is the expected red; record it).

- [ ] **Step 3: Implement.** `create_in`: a process-wide `AtomicU64`, name `format!("{prefix}-{}-{}", std::process::id(), n)`; if the path exists → `SafetyRefused` naming it (never remove something ART did not just make); `create_dir_all`. `finish`: set `finished = true`; `std::fs::remove_dir_all`; on error, if `path.exists()` → `Err(LeftBehind { path, why: err.to_string() })`, else `Ok(())`. `Drop`: if not finished, `remove_dir_all` and `log::warn!` naming the path on failure.

- [ ] **Step 4: Green.** `cargo test --lib core::scratch_guard`.

- [ ] **Step 5: Mutations.** M3a: `finish` returns `Ok(())` without checking `exists()` → the held-file test red. M3b: the counter removed (name from pid only) → the uniqueness test red (or `create_in` refuses — either red is accepted, record which).

- [ ] **Step 6: Commit.** `core::scratch_guard: OwnedScratch names a folder it could not remove (ART-340 groundwork)`.

---

### Task 4: Sizing names the partition — `DoesNotFit.largest`, and System checked against its content

**Files:**
- Modify: `src-tauri/src/core/card/sizing.rs:359-364` (enum), `:492-586` (`plan_card_image`), its tests
- Modify: every caller of `plan_card_image` (tests only today — `grep -rn "plan_card_image(" src-tauri/src`)

**Interfaces:**
- Produces:

```rust
pub enum SizingRefusal {
    DoesNotFit { needed: u64, available: u64, largest: Option<String> },
    PartitionTooLarge { volume_name: String, bytes: u64 },
    CardTooSmall { card_gb: u32 },
    /// A partition's measured content does not fit the size it is given — today only System, whose size is fixed.
    PartitionContentDoesNotFit { volume_name: String, needed_blocks: u64, available_blocks: u64 },
}
pub fn plan_card_image(card_gb: u32, system: Option<&ContentMeasure>, content: &[RequestedPartition])
    -> Result<CardImagePlan, SizingRefusal>;
```

- [ ] **Step 1: Failing tests.**

```rust
#[test]
fn a_card_that_overflows_names_its_largest_partition() {
    let big = measure_of(10_000, 2 * 1024 * 1024); // 10 000 files of 2 MiB
    let small = measure_of(10, 1024);
    let request = [
        RequestedPartition { volume_name: "Stuff".into(), content: Some(small), floor_bytes: 0 },
        RequestedPartition { volume_name: "Games".into(), content: Some(big), floor_bytes: 0 },
    ];
    match plan_card_image(16, None, &request) {
        Err(SizingRefusal::DoesNotFit { largest, .. }) => assert_eq!(largest.as_deref(), Some("Games")),
        other => panic!("expected DoesNotFit naming Games, got {other:?}"),
    }
}

#[test]
fn a_system_tree_larger_than_system_is_refused_naming_system() {
    let tree = measure_of(1_000, 1024 * 1024); // ~1 GiB
    match plan_card_image(16, Some(&tree), &[]) {
        Err(SizingRefusal::PartitionContentDoesNotFit { volume_name, needed_blocks, available_blocks }) => {
            assert_eq!(volume_name, "System");
            assert!(needed_blocks > available_blocks);
        }
        other => panic!("expected System refused, got {other:?}"),
    }
}

#[test]
fn a_system_tree_that_fits_changes_nothing_in_the_plan() {
    let tree = measure_of(4_066, 5_093); // the owner's 3.2.2 tree: 4 066 files, ~20.7 MB (STATUS)
    let with = plan_card_image(16, Some(&tree), &[]).unwrap();
    let without = plan_card_image(16, None, &[]).unwrap();
    assert_eq!(with.partitions.iter().map(|p| p.bytes).collect::<Vec<_>>(),
               without.partitions.iter().map(|p| p.bytes).collect::<Vec<_>>());
}
```

`measure_of(files, bytes_each)` builds a `ContentMeasure` through `add_file` with names `f00000…` in one directory; add it to the test module if no equivalent exists. The overflow test's numbers must overflow a 16 GB card (~20 GiB of content) — check that `content_partition_bytes` for it is `Some` (under the ceiling) or the refusal is `PartitionTooLarge`; adjust `files` so it is `DoesNotFit`, and say so in a comment.

- [ ] **Step 2: Red** — compile errors on the new signature/fields, then (after stubbing) the assertions.

- [ ] **Step 3: Implement.** In `plan_card_image`: after computing `sys_built`, `if let Some(tree) = system { let blocks = sys_built / PFS3_BLOCK; if !pfs3_fits(blocks, tree) { return Err(PartitionContentDoesNotFit { volume_name: "System".into(), needed_blocks: tree.data_blocks, available_blocks: pfs3_data_blocks(blocks) }) } }`. `largest`: the `content` element with the greatest `content.as_ref().map(|m| m.data_blocks)` (ties: the first), `None` when `content` is empty or none is measured. Update every existing call to pass `None` for `system`.

- [ ] **Step 4: Green.** `cargo test --lib core::card::sizing`.

- [ ] **Step 5: Mutations.** M4a: `largest` always `None` → overflow test red. M4b: the System check's `!` removed → both System tests red. M4c: `>` for ties in `largest` (last wins) — survives unless a tie test exists; add `two_equal_partitions_name_the_first` if M4c survives, and say which.

- [ ] **Step 6: Commit.** `card sizing: an overflow names the largest partition, System checked against its tree (round 1 I4)`.

---

### Task 5: The card records what its partitions hold, and the health check stops saying a filled card needs formatting

**Files:**
- Modify: `src-tauri/src/core/card/manifest.rs:133-151` (struct), `:289-309` (`describe_card`), its tests (the `os` assertion at `:523`)
- Modify: `src-tauri/src/core/card/health.rs:154-178`
- Modify: `src-tauri/src/commands/card.rs:649-654` (the one production caller of `describe_card`)

**Interfaces:**
- Produces:

```rust
/// What ART put into one partition. Names only, never host paths: a manifest is shareable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionContent {
    pub drive_name: String,
    pub volume_name: String,
    /// Each source's file name (`WHDLoadDemos100.lha`, `Games`) — `system tree` for System.
    pub sources: Vec<String>,
    pub files: u64,
    pub bytes: u64,
    /// `"native"` or the fallback tool's probed version.
    pub writer: String,
}
// CardManifest gains:  #[serde(default)] pub partitions: Vec<PartitionContent>,
pub fn describe_card(image: &Path, source: SourceFacts, boot_files: Vec<ManifestFile>,
    built_at: Option<String>, os: Vec<String>, partitions: Vec<PartitionContent>) -> CoreResult<CardManifest>;
```

`MANIFEST_SCHEMA` stays 1: both additions are `#[serde(default)]` and a manifest written before them reads.

- [ ] **Step 1: Failing tests.** In `manifest.rs`:

```rust
#[test]
fn a_manifest_written_before_partitions_existed_still_reads() {
    let old = r#"{"schema":1,"art_version":"0.9.4","built_at":null,"total_bytes":1,"mbr_sha256":"x",
                 "slots":[],"source":SOURCE_FACTS_JSON,"boot_files":[],"areas":[],"os":[]}"#;
    // SOURCE_FACTS_JSON: paste the serialised SourceFacts the neighbouring render test produces.
    let manifest: CardManifest = serde_json::from_str(old).unwrap();
    assert!(manifest.partitions.is_empty());
}
```

In `health.rs`:

```rust
#[test]
fn a_card_whose_manifest_records_every_partition_filled_asks_for_no_formatting() {
    let (_guard, dir) = scratch("filled");
    let image = built_card_with_partitions(&dir, &["SDH0", "SDH1"]); // the existing build helper
    let mut manifest = manifest_for(&image);
    manifest.partitions = ["SDH0", "SDH1"].iter().map(|d| PartitionContent {
        drive_name: d.to_string(), volume_name: d.to_string(), sources: vec![],
        files: 1, bytes: 1, writer: "native".into() }).collect();
    let report = check_image(&image, Some(&manifest), "pi4").unwrap();
    assert!(!report.by_hand.iter().any(|s| matches!(s, ManualStep::VolumesNeedFormatting { .. })), "{:?}", report.by_hand);
}

#[test]
fn a_card_with_one_partition_unfilled_asks_for_exactly_that_one() {
    // as above, manifest.partitions names SDH0 only
    // assert by_hand contains VolumesNeedFormatting { count: 1 }
}
```

(Use the helpers `health.rs` tests already have for building an image and its manifest; if they build through `commands`, build through `core::card::build::build_card` + `describe_card` instead.)

- [ ] **Step 2: Red.** Compile error (field), then `VolumesNeedFormatting { count: 2 }` present.

- [ ] **Step 3: Implement.** Add the struct and field; `describe_card` takes and stores `os` and `partitions`; the `commands/card.rs` caller passes `Vec::new(), Vec::new()` (the Card Builder screen still builds unformatted volumes, so its manifest is unchanged in meaning). In `check_image`: `let filled: HashSet<&str> = manifest.map(|m| m.partitions.iter().map(|p| p.drive_name.as_str()).collect()).unwrap_or_default();` count partitions whose `drive_name` is not in `filled`; push `VolumesNeedFormatting` only when the count is non-zero. Update the enum doc: "SD-1 builds the shape; a partition the manifest records as filled is not counted".

- [ ] **Step 4: Green.** `cargo test --lib core::card::manifest` and `core::card::health`, and `commands::card`.

- [ ] **Step 5: Mutations.** M5a: `filled` ignored → the all-filled test red. M5b: the step pushed even at count 0 → the all-filled test red. M5c: `#[serde(default)]` removed from `partitions` → the old-manifest test red.

- [ ] **Step 6: Commit.** `card manifest: what each partition holds; health counts only unfilled volumes`.

---

### Task 6: `.partial` — a half-built image never carries a finished one's name, and a failed read-back removes the file

**Files:**
- Modify: `src-tauri/src/core/card/build.rs:185-197`
- Create: `src-tauri/src/core/cardos/partial.rs`; Modify: `core/cardos/mod.rs` (`pub mod partial;`)
- Modify: `src-tauri/src/core/error.rs` (`PartialImageExists`)
- Test: inline in both

**Interfaces:**
- Produces:

```rust
/// `E:\x\amiga.img` → `E:\x\amiga.img.partial`.
pub fn partial_path_for(image: &Path) -> PathBuf;
/// Refuse before anything is written: a `.vhd` destination (design § 9), an existing image (SAFE_CREATE),
/// an existing `<image>.partial` (P3, named, never removed).
pub fn refuse_partial_destination(image: &Path) -> CoreResult<()>;
/// Rename `<image>.partial` to `<image>`; refuses (and leaves both) when `<image>` appeared meanwhile.
pub fn finish_partial(image: &Path) -> CoreResult<()>;
/// What happened to the partial file on a failed or stopped build.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum PartialRemoval { NotCreated, Removed { path: String }, NotRemoved { path: String, why: String } }
pub fn remove_partial(image: &Path, created: bool) -> PartialRemoval;
```

`CoreError::PartialImageExists { path: String }` — Display: `"'{path}' is a half-built card image from an earlier run. ART does not remove a file it did not create in this run: delete it yourself, or choose another image name, and build again."`, code `"ART-CARD-PARTIAL-EXISTS"`.

- [ ] **Step 1: Failing tests.** In `build.rs`:

```rust
#[test]
fn a_card_that_does_not_read_back_is_removed_not_left() {
    let (_guard, dir) = scratch("readback");
    let dest = dir.join("card.img");
    // Two areas asked, and a spec whose second area the reader will not find:
    // an area of 0 bytes before a trailing one is what plan_card lays out and read_card skips.
    // If no spec reaches the read-back failure honestly, inject it: see Step 3's seam.
    let err = build_card_with_reader(&dest, &two_area_spec(), &NoProgress, |_| Err(CoreError::Malformed {
        format: "card".into(), detail: "injected".into() })).unwrap_err();
    assert!(err.to_string().contains("injected"));
    assert!(!dest.exists(), "a build that cannot be read is removed");
}
```

In `partial.rs`:

```rust
#[test]
fn the_partial_name_is_the_whole_name_plus_partial() {
    assert_eq!(partial_path_for(Path::new(r"E:\x\amiga.img")), PathBuf::from(r"E:\x\amiga.img.partial"));
}
#[test]
fn a_stale_partial_is_refused_by_name_and_left_alone() {
    let (_guard, dir) = scratch("stale");
    let image = dir.join("amiga.img");
    std::fs::write(partial_path_for(&image), b"old").unwrap();
    let err = refuse_partial_destination(&image).unwrap_err();
    assert_eq!(err.code(), "ART-CARD-PARTIAL-EXISTS");
    assert_eq!(std::fs::read(partial_path_for(&image)).unwrap(), b"old");
}
#[test]
fn an_existing_image_and_a_vhd_are_refused_before_anything_is_written() { /* two arms, SafetyRefused / UnsupportedFormat */ }
#[test]
fn finish_renames_and_refuses_when_the_image_appeared_meanwhile() {
    // arm 1: partial exists, image absent → Ok, image exists, partial gone
    // arm 2: partial and image both exist → Err, both files unchanged
}
#[test]
fn removal_is_reported_as_it_happened() {
    // NotCreated when created=false (and a file there is untouched); Removed when created=true.
}
```

- [ ] **Step 2: Red.**

- [ ] **Step 3: Implement.** `build.rs`: split the reader into a seam — `pub fn build_card(dest, spec, progress)` calls `fn build_card_with_reader(dest, spec, progress, read: impl Fn(&Path) -> CoreResult<CardImage>)` with `read_card`; in the latter, after `step("Checking what was built")`, both the reader's error and the area-count mismatch run `let _ = std::fs::remove_file(dest);` before returning, with the comment "the file is ART's own from this call — see the `lay_out` failure above". `partial.rs` as specified: `refuse_partial_destination` order — `.vhd` (case-insensitive extension) → `UnsupportedFormat("a card image built with its partitions filled cannot be a .vhd: ART's PFS3 writer cannot fill a dynamic VHD — choose a .img name")`; image exists → `SafetyRefused` (same sentence as `build_card`); partial exists → `PartialImageExists`. `finish_partial`: if image exists → `SafetyRefused` naming both; else `std::fs::rename`. `remove_partial(image, created)`: `created == false` → `NotCreated`; else `remove_file`; `Ok` or `NotFound` → `Removed`; other error → `NotRemoved`.

- [ ] **Step 4: Green.** `cargo test --lib core::card::build` and `core::cardos::partial`.

- [ ] **Step 5: Mutations.** M6a: the read-back `remove_file` removed → build test red. M6b: `refuse_partial_destination` skips the partial check → stale test red. M6c: `remove_partial` removes when `created == false` → removal test red.

- [ ] **Step 6: Commit.** `card: .partial name, a stale one refused; a card that does not read back is removed`.

---

### Task 7: The PFS3 driver — found loose or inside `pfs3aio.lha`, its version from its own `$VER:`

**Files:**
- Create: `src-tauri/src/core/cardos/driver.rs`; Modify: `core/cardos/mod.rs`, `core/error.rs` (`Pfs3DriverNotFound`)

**Interfaces:**
- Produces:

```rust
pub const DRIVER_MAX_BYTES: u64 = 1024 * 1024;
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundDriver {
    /// A loose file ART reads in place, or the copy `find_pfs3_driver` wrote into `stage_dir`.
    pub path: PathBuf,
    /// The archive it came out of, when it did.
    pub from_archive: Option<PathBuf>,
    pub version: u32,
    pub revision: u32,
}
/// `explicit` (the user's own choice) wins; otherwise the material folders in order: a loose `pfs3aio`
/// (top level, then `L/`), then an archive whose name starts `pfs3aio` (case-insensitive) holding a member
/// whose last segment is `pfs3aio`. Among several, the highest `$VER:`; a file with no `$VER:` is skipped.
pub fn find_pfs3_driver(explicit: Option<&Path>, material: &[PathBuf], stage_dir: &Path) -> CoreResult<FoundDriver>;
```

`CoreError::Pfs3DriverNotFound { searched: Vec<String> }` — Display: `"No PFS3 driver was found. ART looked for 'pfs3aio' or 'pfs3aio.lha' in: {searched joined by ', '}. Put pfs3aio.lha (Aminet disk/misc/pfs3aio) in one of your material folders, or choose the driver file yourself."`, code `"ART-PFS3-DRIVER-NOT-FOUND"`.

- [ ] **Step 1: Failing tests.**

```rust
fn driver_bytes(version: &str) -> Vec<u8> {
    let mut bytes = vec![0u8; 64];
    bytes.extend_from_slice(format!("$VER: pfs3aio {version} (01.01.2026)\0").as_bytes());
    bytes
}

#[test]
fn a_loose_driver_is_found_in_place_with_its_version() {
    let (_guard, dir) = scratch("loose");
    std::fs::write(dir.join("pfs3aio"), driver_bytes("19.2")).unwrap();
    let found = find_pfs3_driver(None, &[dir.clone()], &dir.join("stage")).unwrap();
    assert_eq!((found.version, found.revision, found.from_archive), (19, 2, None));
    assert_eq!(found.path, dir.join("pfs3aio"));
}

#[test]
fn a_driver_inside_an_lha_is_unpacked_into_the_stage_folder() {
    let (_guard, dir) = scratch("lha");
    write_lha(&dir.join("pfs3aio.lha"), &[("pfs3aio/pfs3aio", &driver_bytes("19.2"))]); // core::lha test writer
    let stage = dir.join("stage");
    std::fs::create_dir(&stage).unwrap();
    let found = find_pfs3_driver(None, &[dir.clone()], &stage).unwrap();
    assert_eq!(found.from_archive.as_deref(), Some(dir.join("pfs3aio.lha").as_path()));
    assert!(found.path.starts_with(&stage));
    assert_eq!(std::fs::read(&found.path).unwrap(), driver_bytes("19.2"));
}

#[test]
fn the_highest_version_wins_and_an_unversioned_file_is_skipped() {
    // folder A: loose pfs3aio 18.5; folder B: pfs3aio.lha with 19.2; folder C: loose "pfs3aio" with no $VER:
    // → 19.2 from B's archive
}

#[test]
fn nothing_found_names_every_folder_searched() {
    let (_guard, dir) = scratch("none");
    let err = find_pfs3_driver(None, &[dir.clone()], &dir).unwrap_err();
    assert_eq!(err.code(), "ART-PFS3-DRIVER-NOT-FOUND");
    assert!(err.to_string().contains(&dir.display().to_string()));
}

#[test]
fn an_explicit_driver_wins_over_the_material() { /* explicit 18.0 beside material 19.2 → 18.0 */ }
```

(`write_lha`: use the LHA fixture writer `core::lha` tests already expose — `level0_entry` became `pub(crate)` in round 2; if a whole-archive writer is not exposed, build the archive bytes from `level0_entry` entries plus the terminating zero byte as `archive/lha.rs` tests do.)

- [ ] **Step 2: Red.**

- [ ] **Step 3: Implement.** Loose: `std::fs::metadata(p).len() <= DRIVER_MAX_BYTES`, read, `amigaver::read`. Archive: `archive::open(path)?`, `entries()`, find a non-dir entry whose name split on `/` or `\` ends (case-insensitive) with `pfs3aio`, `read(index, DRIVER_MAX_BYTES)`, `amigaver::read`; keep bytes in memory; after choosing the winner, if it came from an archive write it to `stage_dir.join("pfs3aio")` with `atomic_write` (refuse if it exists — `SAFE_CREATE`). An unreadable archive is skipped **and** listed: extend `Pfs3DriverNotFound` with `unreadable: Vec<String>` and say "could not read: …" when non-empty. Choice with `AmigaVersion::compare_version`; first wins a tie.

- [ ] **Step 4: Green.** `cargo test --lib core::cardos::driver`.

- [ ] **Step 5: Mutations.** M7a: `compare_version` reversed → highest-version test red. M7b: an unversioned file accepted as 0.0 → survives unless it outranks; make the test's C folder come first so M7b is red.

- [ ] **Step 6: Commit.** `cardos: the PFS3 driver found loose or in pfs3aio.lha, by its own $VER:`.

---

### Task 8: WHDLoad — chosen by `$VER:` from the material and the partitions' hardfiles, written into the tree

**Files:**
- Create: `src-tauri/src/core/cardos/whdload.rs`; Modify: `core/cardos/mod.rs`, `core/error.rs` (`WhdloadNotFound`)

**Interfaces:**
- Produces:

```rust
pub const WHDLOAD_MAX_BYTES: u64 = 1024 * 1024;
pub const PREFS_MAX_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum WhdloadOrigin {
    /// `C/WHDLoad` beside a material folder's `S/WHDLoad.prefs`, or a top-level `WHDLoad` file.
    Loose { path: PathBuf },
    /// A member `…/C/WHDLoad` of an archive in a material folder (`WHDLoad_usr.lha`).
    Archive { archive: PathBuf, member: String },
    /// `C/WHDLoad` on a WHDLoad hardfile that is one of a partition's sources.
    Hardfile { image: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhdloadChoice {
    pub origin: WhdloadOrigin,
    pub version: u32,
    pub revision: u32,
    /// Stated as the file states it: `WHDLoad 20.0`.
    pub name: String,
    #[serde(skip)] pub binary: Vec<u8>,
    /// The `S/WHDLoad.prefs` from the same package, when it has one.
    #[serde(skip)] pub prefs: Option<Vec<u8>>,
    /// Every candidate ART looked at and did not choose, with why — so the choice is never silent.
    pub passed_over: Vec<String>,
}

/// Every WHDLoad ART can read: each material folder (top level: a `WHDLoad` file, `C/WHDLoad`;
/// archives whose name starts `whdload`) and each hardfile source. Unreadable candidates are listed in `passed_over`.
pub fn find_whdload(material: &[PathBuf], hardfiles: &[PathBuf], clock: &dyn AmigaClock) -> CoreResult<Option<WhdloadChoice>>;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum TreeWrite { Written { path: String }, AlreadyThere { path: String }, KeptExisting { path: String } }

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhdloadInstalled { pub binary: TreeWrite, pub prefs: Option<TreeWrite> }

/// `C/WHDLoad` and `S/WHDLoad.prefs` into `tree`. An identical file is `AlreadyThere`; a different one is
/// kept (`KeptExisting`), never replaced — prefs are user data (W § 1.2).
pub fn install_whdload(choice: &WhdloadChoice, tree: &Path) -> CoreResult<WhdloadInstalled>;

/// Whether `tree` already carries a `C/WHDLoad` (case-insensitive walk of `C`).
pub fn tree_has_whdload(tree: &Path) -> bool;
```

`CoreError::WhdloadNotFound { titles: usize, searched: Vec<String> }` — Display: `"{titles} WHDLoad title(s) are on this card and no WHDLoad was found, so they would not start. ART looked in: {searched}. Put WHDLoad_usr.lha (from whdload.de) in one of your material folders and prepare again."`, code `"ART-WHDLOAD-NOT-FOUND"`.

- [ ] **Step 1: Failing tests.**

```rust
fn whdload_bytes(version: &str) -> Vec<u8> {
    let mut bytes = vec![0u8; 286];
    bytes.extend_from_slice(format!("$VER: WHDLoad {version} [build 7051] (27.03.2026)\0").as_bytes());
    bytes
}

#[test]
fn the_highest_whdload_wins_across_archive_loose_and_hardfile_and_the_rest_are_named() {
    let (_guard, dir) = scratch("choose");
    let material = dir.join("material");
    std::fs::create_dir_all(material.join("C")).unwrap();
    std::fs::write(material.join("C").join("WHDLoad"), whdload_bytes("18.9")).unwrap();
    write_lha(&material.join("WHDLoad_usr.lha"), &[
        ("WHDLoad/C/WHDLoad", &whdload_bytes("20.0")),
        ("WHDLoad/S/WHDLoad.prefs", b";all comments\n"),
    ]);
    let hdf = dir.join("Game.hdf");
    build_hdf(&hdf, &["C", "S", "Game"], &[("C/WHDLoad", &whdload_bytes("19.1")), ("Game/Game.slave", SLAVE)]);
    let choice = find_whdload(&[material.clone()], &[hdf], &UTC_FOR_TESTS).unwrap().unwrap();
    assert_eq!((choice.version, choice.revision), (20, 0));
    assert!(matches!(choice.origin, WhdloadOrigin::Archive { .. }));
    assert_eq!(choice.prefs.as_deref(), Some(&b";all comments\n"[..]));
    assert_eq!(choice.passed_over.len(), 2, "{:?}", choice.passed_over);
}

#[test]
fn no_whdload_anywhere_is_none_not_a_guess() { /* empty material, a hardfile without C/WHDLoad → Ok(None) */ }

#[test]
fn installing_writes_both_files_and_keeps_a_different_prefs() {
    let (_guard, dir) = scratch("install");
    let tree = dir.join("tree");
    std::fs::create_dir_all(tree.join("S")).unwrap();
    std::fs::write(tree.join("S").join("WHDLoad.prefs"), b"QuitKey=$59\n").unwrap();
    let choice = WhdloadChoice { /* binary whdload_bytes("20.0"), prefs Some(b";x\n") … */ };
    let done = install_whdload(&choice, &tree).unwrap();
    assert!(matches!(done.binary, TreeWrite::Written { .. }));
    assert!(matches!(done.prefs, Some(TreeWrite::KeptExisting { .. })));
    assert_eq!(std::fs::read(tree.join("S").join("WHDLoad.prefs")).unwrap(), b"QuitKey=$59\n");
    assert_eq!(std::fs::read(tree.join("C").join("WHDLoad")).unwrap(), whdload_bytes("20.0"));
}

#[test]
fn an_identical_binary_is_already_there_and_a_different_one_is_kept() { /* two arms */ }
```

`SLAVE` is a minimal valid slave from `core::gameindex::readers::slave`'s test builder (expose it `pub(crate)` under `#[cfg(test)]` if it is private).

- [ ] **Step 2: Red.**

- [ ] **Step 3: Implement.** Loose: `material/WHDLoad`, `material/C/WHDLoad` (host names matched case-insensitively by listing the folder, not by probing a spelling); prefs `material/S/WHDLoad.prefs`. Archive: files in `material` (top level) whose name starts `whdload` (case-insensitive) and whose `detect` category is an archive; member names normalised `\`→`/`; binary member ends `/c/whdload` or equals `c/whdload`; prefs member in the same top directory ends `/s/whdload.prefs`; reads bounded by the constants. Hardfile: `scan_image` → the one mountable volume → `mount` → walk `C` then `WHDLoad` by `list_directory_on` from `geometry.root_block` (names compared with `amiga_fold`), `read_header_on` + `extract_file_on(…, fs_type_of(&geometry))`; prefs `S/WHDLoad.prefs` likewise; a file over the bound is passed over with the reason. Choose with `compare_version`; ties go to the first found (material before hardfiles). `install_whdload`: `create_dir_all(tree/C)`, `tree/S`; for each file: absent → `atomic_write`, `Written`; present and equal → `AlreadyThere`; present and different → `KeptExisting`. Existing names in `C`/`S` are found case-insensitively (a tree may spell `c/whdload`).

- [ ] **Step 4: Green.** `cargo test --lib core::cardos::whdload`.

- [ ] **Step 5: Mutations.** M8a: `compare_version` reversed → choose test red. M8b: prefs overwritten when different → install test red. M8c: `passed_over` not filled → choose test red.

- [ ] **Step 6: Commit.** `cardos: WHDLoad chosen by $VER: and written into the tree, prefs never replaced`.

---

### Task 9: Kickstarts — what the titles want, the proposal with its `.RTB`, and only the agreed placements

**Files:**
- Create: `src-tauri/src/core/cardos/kickstarts.rs`; Modify: `core/cardos/mod.rs`, `core/error.rs` (`KickstartNotProposed`)
- Modify: `src-tauri/src/commands/gameindex.rs:127-150` (use the moved `wanted_images`)

**Interfaces:**
- Consumes: `core::gameindex::readers::slave::read_slave`, `core::gameindex::record::KickstartNeed`, `core::rom::offer::{offer_for, Offer, WantedImage}`, `core::rom::place::{place, Placement, PlaceOutcome, KICKSTART_DRAWER}`, `core::rom::{scan_rom_directory, RomInfo}`, `core::whdload::has_extension`.
- Produces:

```rust
pub const SLAVE_MAX_BYTES: u64 = 1024 * 1024;
pub const RTB_MAX_BYTES: u64 = 64 * 1024;
pub const MAX_WALK_NODES: usize = 200_000;

/// Moved verbatim from `commands/gameindex.rs` (its doc comment with it).
pub fn wanted_images(need: &KickstartNeed) -> Vec<WantedImage>;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TitleNeeds {
    /// Slave path (relative to its root, `/`-separated) → the images it will accept, in the slave's order.
    pub titles: Vec<(String, Vec<WantedImage>)>,
    /// Slaves ART could not read, named — never dropped silently.
    pub unreadable: Vec<String>,
}
/// Walk each prepared partition root on the host for `.slave` files (bounded).
pub fn find_title_needs(roots: &[PathBuf], progress: &dyn ProgressSink) -> CoreResult<TitleNeeds>;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum RtbSource { Loose { path: PathBuf }, InArchive { archive: PathBuf, member: String }, Missing }

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposedKickstart {
    /// The name WHDLoad looks for, `kick40068.A1200` — the key the build's agreement uses.
    pub name: String,
    /// The titles that name it (at most 20) and how many more.
    pub titles: Vec<String>,
    pub titles_more: usize,
    pub offer: Offer,
    pub rtb: RtbSource,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KickstartProposal {
    pub items: Vec<ProposedKickstart>,
    pub unreadable_slaves: Vec<String>,
    /// True when some item's `.RTB` is `Missing`: the screen names Aminet `util/boot/skick346` (owner's decision 2).
    pub rtb_missing: bool,
}

/// One item per distinct wanted name, in first-seen order. `collection_dirs` are scanned with
/// `scan_rom_directory`; a folder that is not a directory is skipped.
pub fn propose_kickstarts(needs: &TitleNeeds, collection_dirs: &[PathBuf], material: &[PathBuf]) -> CoreResult<KickstartProposal>;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacedKickstart { pub name: String, pub image: PlaceOutcome, pub rtb: PlaceOutcome }

/// Place exactly the `agreed` names. A name not in the proposal, or not `Supplied`, or whose `.RTB` is
/// `Missing`, is refused before anything is written (`KickstartNotProposed`).
pub fn place_agreed(proposal: &KickstartProposal, agreed: &[String], tree: &Path) -> CoreResult<Vec<PlacedKickstart>>;
```

`CoreError::KickstartNotProposed { name: String, why: String }` — Display `"'{name}' cannot be placed: {why}. Prepare the card again and agree only to what the proposal offers."`, code `"ART-KICKSTART-NOT-PROPOSED"`.

- [ ] **Step 1: Failing tests.**

```rust
#[test]
fn every_slave_under_the_roots_is_read_and_an_unreadable_one_is_named() {
    let (_guard, dir) = scratch("needs");
    let games = dir.join("Games");
    std::fs::create_dir_all(games.join("Turrican")).unwrap();
    std::fs::write(games.join("Turrican").join("Turrican.slave"), slave_needing("kick34005.A500", 0xf20b, 262_144)).unwrap();
    std::fs::create_dir_all(games.join("Broken")).unwrap();
    std::fs::write(games.join("Broken").join("Broken.Slave"), b"not a slave").unwrap();
    let needs = find_title_needs(&[dir.clone()], &NoProgress).unwrap();
    assert_eq!(needs.titles.len(), 1);
    assert_eq!(needs.titles[0].1[0].name, "kick34005.A500");
    assert_eq!(needs.unreadable, vec!["Games/Broken/Broken.Slave".to_string()]);
}

#[test]
fn the_proposal_matches_by_crc_and_finds_the_rtb_in_skick346() {
    let (_guard, dir) = scratch("propose");
    let roms = dir.join("roms");
    std::fs::create_dir_all(&roms).unwrap();
    let rom = vec![0x11u8; 262_144];
    std::fs::write(roms.join("my-13.rom"), &rom).unwrap();
    let crc = crate::core::hashing::crc16_arc(&rom);
    let material = dir.join("material");
    std::fs::create_dir_all(&material).unwrap();
    write_lha(&material.join("skick346.lha"), &[("Kickstarts/kick34005.A500.RTB", &[1u8; 4000][..])]);
    let needs = TitleNeeds { titles: vec![("Games/T/T.slave".into(),
        vec![WantedImage { name: "kick34005.A500".into(), crc16: Some(crc), size: Some(262_144) }])], unreadable: vec![] };
    let proposal = propose_kickstarts(&needs, &[roms.clone()], &[material.clone()]).unwrap();
    let item = &proposal.items[0];
    assert!(matches!(item.offer, Offer::Supplied { .. }), "{:?}", item.offer);
    assert!(matches!(&item.rtb, RtbSource::InArchive { member, .. } if member == "Kickstarts/kick34005.A500.RTB"));
    assert!(!proposal.rtb_missing);
}
```

Before relying on `Supplied` with arbitrary bytes, check `identify_rom`/`scan_rom_directory`: if an unidentified dump gets `whdload_crc16: None` (T § 2: `Some` only for identified images), build the collection for this test from a `RomInfo` literal instead and give `propose_kickstarts` a seam `propose_kickstarts_from(needs, collection: &[RomInfo], material)` that the directory-scanning form calls — the test then calls the seam. **Record which in the commit message.**

```rust
#[test]
fn a_missing_rtb_is_said_and_that_image_cannot_be_agreed() {
    // proposal with offer Supplied and rtb Missing → rtb_missing true;
    // place_agreed(&proposal, &["kick34005.A500".into()], &tree) → Err ART-KICKSTART-NOT-PROPOSED, tree untouched
}

#[test]
fn only_the_agreed_names_are_placed_with_their_rtb_under_devs_kickstarts() {
    // two Supplied items with RTBs; agree to one → Devs/Kickstarts/<name> is the decoded ROM,
    // Devs/Kickstarts/<name>.RTB the RTB bytes; the other name absent
}

#[test]
fn a_name_the_proposal_does_not_carry_is_refused_before_anything_is_written() { /* agreed ["kick99999.A9"] */ }

#[test]
fn the_moved_mapping_reads_a_list_before_the_single_image() { /* the ART-137 case: alternatives win */ }
```

`slave_needing(name, crc, size)` builds a v16 slave with those fields through the slave reader's test builder.

- [ ] **Step 2: Red.**

- [ ] **Step 3: Implement.** Move `wanted_images` verbatim; `commands/gameindex.rs` calls `crate::core::cardos::kickstarts::wanted_images`. `find_title_needs`: iterative stack walk of each root with a node counter (`LimitExceeded` past `MAX_WALK_NODES`, naming the root), files where `whdload::has_extension(name, "slave")` (use the exact helper signature at `whdload/mod.rs:261`), skip `.uaem` sidecars, read ≤ `SLAVE_MAX_BYTES` (larger → unreadable), `read_slave` → `wanted_images(&facts.kickstart)`; an empty list is a title with no need (not recorded). Report `progress` between files; check `is_cancelled()`. `propose_kickstarts`: collection = concatenation of `scan_rom_directory` over existing dirs; for each distinct name in first-seen order, the first `WantedImage` carrying it; `offer_for(&[wanted], &collection)`; RTB: in each material folder a loose `<name>.RTB` at top level, in `Kickstarts/`, or in `Devs/Kickstarts/` (case-insensitive listing), else an archive at top level whose name starts `skick` with a member whose normalised name ends `/<name>.rtb` (case-insensitive), else `Missing`. `place_agreed`: validate every agreed name first (present, `Supplied`, RTB not `Missing`) — refuse the first failure before any write; then for each: `place(&Placement { from: by.path.into(), as_name: name, tree })`, and the RTB bytes (loose read or archive member, bounded) written through `place`'s semantics: destination `destination_of` with `as_name = format!("{name}.RTB")`; absent → `atomic_write`; equal → `AlreadyThere`; different → `Occupied`.

- [ ] **Step 4: Green.** `cargo test --lib core::cardos::kickstarts` and `commands::gameindex`.

- [ ] **Step 5: Mutations.** M9a: `place_agreed` skips the RTB-missing check → missing-RTB test red. M9b: places every `Supplied` item regardless of `agreed` → only-agreed test red (**the owner's rule — must not survive**). M9c: an unreadable slave dropped silently → needs test red.

- [ ] **Step 6: Commit.** `cardos: Kickstarts the titles want, proposed with their .RTB, placed only when agreed`.

---

### Task 10: `prepare.rs` — measure the card, say what space it needs, stage it

**Files:**
- Create: `src-tauri/src/core/cardos/prepare.rs`; Modify: `core/cardos/mod.rs`, `core/error.rs` (`CardSourceUnusable`, `CardNamesNeedHstImager`, `CardDoesNotFit`, `NotEnoughSpace`)

**Interfaces:**
- Consumes: Tasks 4, 7, 8, 9; `content::{classify, measure, check_partition, prepare, Classified, SourceKind, SourceMeasure}`.
- Produces:

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionInput { pub volume_name: String, pub sources: Vec<PathBuf>, #[serde(default)] pub floor_bytes: u64 }

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "writer", rename_all = "kebab-case")]
pub enum PartitionWriter { Native, HstImager }

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasuredSource { pub path: PathBuf, pub kind: SourceKind, #[serde(skip)] pub measure: SourceMeasure, pub files: u64, pub bytes: u64 }

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasuredPartition { pub volume_name: String, pub drive_name: String, pub sources: Vec<MeasuredSource>, pub writer: PartitionWriter }

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasuredCard {
    pub plan: CardImagePlan,
    pub system: MeasuredPartition,          // the tree, drive SDH0
    pub partitions: Vec<MeasuredPartition>, // the user's, in order
    pub driver: FoundDriver,
    /// Bytes the non-folder sources will take in staging, from their measures.
    pub staging_bytes: u64,
}

/// Classify and measure every source, check every partition, choose each partition's writer, find the
/// driver, and lay the card out. Writes only `driver_stage` (a driver out of an archive).
pub fn measure_card(card_gb: u32, tree: &Path, partitions: &[PartitionInput], material: &[PathBuf],
    explicit_driver: Option<&Path>, driver_stage: &Path, hst_imager_available: bool,
    clock: &dyn AmigaClock, progress: &dyn ProgressSink) -> CoreResult<MeasuredCard>;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceNeed { pub place: PathBuf, pub bytes: u64 }
/// The image (plan.image_bytes) on the image's folder; staging on the scratch folder. Two places on one
/// volume (same path prefix) are summed into one need.
pub fn space_needs(card: &MeasuredCard, image: &Path, scratch: &Path) -> Vec<SpaceNeed>;
/// Refuse the first need `available` cannot meet, naming the place and both numbers.
pub fn check_free_space(needs: &[SpaceNeed], available: impl Fn(&Path) -> std::io::Result<u64>) -> CoreResult<()>;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedCard {
    pub measured: MeasuredCard,
    /// Per user partition, in order: the folders its copy takes (a folder source itself, or its staging folder).
    pub roots: Vec<Vec<PathBuf>>,
    pub left_behind: Vec<String>,
    pub whdload: Option<WhdloadChoice>,
    pub kickstarts: KickstartProposal,
}

/// Stage every non-folder source into `staging_root/<p>-<s>`, find the titles, choose WHDLoad, propose
/// Kickstarts, and check System again with WHDLoad and every proposed image and RTB added (upper bound).
pub fn stage_card(measured: MeasuredCard, tree: &Path, staging_root: &Path, material: &[PathBuf],
    collection_dirs: &[PathBuf], clock: &dyn AmigaClock, progress: &dyn ProgressSink) -> CoreResult<PreparedCard>;
```

Errors: `CardSourceUnusable { partition: String, source_path: String, why: Unusable }` (Display from `why`: missing → "does not exist", unreadable → the detail, …; each with "remove it from {partition} or fix it"), code `"ART-CARD-SOURCE-UNUSABLE"`. `CardNamesNeedHstImager { partition: String, paths: Vec<String>, more: usize }` — "…{partition} holds names ART's own PFS3 writer cannot write yet (ART-113): {paths}. Point ART at hst-imager in Settings, or rename them." code `"ART-CARD-NAMES-NEED-HST"`. `CardDoesNotFit(SizingRefusal)` — Display per variant, naming `largest` / `volume_name` and the bytes, code `"ART-CARD-DOES-NOT-FIT"`. `NotEnoughSpace { place: String, needed: u64, available: u64 }` code `"ART-NOT-ENOUGH-SPACE"`.

- [ ] **Step 1: Failing tests.** (Small synthetic sources; `card_gb` 16 — `measure_card` writes no image, so the size costs nothing.)

```rust
#[test]
fn a_card_is_measured_with_system_first_and_each_partition_s_writer() {
    let (_guard, dir) = scratch("measure");
    let tree = small_tree(&dir);                       // C/Dir, S/Startup-Sequence
    let games = folder_with(&dir, "Games", &[("Turrican/Turrican.slave", SLAVE)]);
    let stuff = folder_with(&dir, "Stuff", &[("Café/readme", b"x")]); // non-ASCII
    let material = material_with_driver(&dir);         // a loose pfs3aio 19.2
    let parts = [
        PartitionInput { volume_name: "Games".into(), sources: vec![games], floor_bytes: 0 },
        PartitionInput { volume_name: "Stuff".into(), sources: vec![stuff], floor_bytes: 0 },
    ];
    let card = measure_card(16, &tree, &parts, &[material], None, &dir.join("drv"), true, &UTC_FOR_TESTS, &NoProgress).unwrap();
    assert_eq!(card.system.drive_name, "SDH0");
    assert_eq!(card.partitions.iter().map(|p| (&p.volume_name[..], &p.drive_name[..], p.writer.clone())).collect::<Vec<_>>(),
               vec![("Games", "SDH1", PartitionWriter::Native), ("Stuff", "SDH2", PartitionWriter::HstImager)]);
}

#[test]
fn non_ascii_names_without_hst_imager_are_refused_naming_the_partition() {
    // same, hst_imager_available = false → Err ART-CARD-NAMES-NEED-HST, message contains "Stuff" and "Café"
}

#[test]
fn an_unusable_source_is_refused_naming_its_partition_and_why() {
    // a source path that does not exist → Err ART-CARD-SOURCE-UNUSABLE, message contains the partition and the path
}

#[test]
fn free_space_refuses_the_first_short_place_with_both_numbers_and_sums_one_volume() {
    let needs = vec![SpaceNeed { place: r"E:\cards".into(), bytes: 100 }, SpaceNeed { place: r"E:\scratch".into(), bytes: 50 }];
    // space_needs sums these when both are on E: — test space_needs separately with a MeasuredCard stub;
    // here: available returns 120 for E: → Err NotEnoughSpace { needed: 150, available: 120 } after summing
}

#[test]
fn staging_puts_archive_contents_in_staging_and_uses_folders_in_place() {
    // Games: one folder + one .lha pack → roots[0] == [folder, staging_root/0-1]; the pack's drawer is under the staging folder
}

#[test]
fn staging_finds_titles_chooses_whdload_and_proposes_kickstarts() {
    // material: WHDLoad_usr.lha 20.0 + skick346.lha; roms dir with the ROM the slave names →
    // prepared.whdload Some(20.0); prepared.kickstarts.items[0].offer Supplied, rtb InArchive
}

#[test]
fn titles_and_no_whdload_anywhere_is_refused() { /* ART-WHDLOAD-NOT-FOUND naming the count */ }

#[test]
fn a_tree_that_already_has_whdload_needs_none_from_the_material() { /* tree has C/WHDLoad → whdload None, no refusal */ }
```

- [ ] **Step 2: Red.**

- [ ] **Step 3: Implement.**
  - `measure_card`: `driver = find_pfs3_driver(explicit_driver, material, driver_stage)?` **first** (§ 6 order: a missing driver before anything slow). System: `classify(tree)` must be `Folder`; `measure(tree, &Folder, …)`. For each partition, each source: `classify` → `NotUsable { why }` → `CardSourceUnusable`; `measure`; then `check_partition(&pairs)?`. Writer: non-ASCII count > 0 → `HstImager` if available else `CardNamesNeedHstImager` (bounded list via the measures' `non_ascii` + `non_ascii_more`). System with non-ASCII names follows the same rule (a tree ART built carries none, ART-113's refusal happens there first; handle it anyway). Sizing: `plan_card_image(card_gb, Some(&system.content), &requested)` with `RequestedPartition { volume_name, content: Some(sum of the partition's source contents), floor_bytes }` — summing `ContentMeasure` needs `ContentMeasure::absorb(&mut self, other: &ContentMeasure)`: add it to `sizing.rs` (fields add; a unit test `absorbing_two_measures_adds_every_field`) — `Err(refusal)` → `CardDoesNotFit(refusal)`. Drive names from `plan.partitions` in order (System is index 0; the user's partitions follow; Work is not in `partitions`). `staging_bytes`: sum of `content.data_blocks * 512` over non-folder sources.
  - `space_needs`: key by `path.components().next()` (the Windows prefix); same key → one `SpaceNeed` at the first place with the summed bytes.
  - `check_free_space`: for each need, `available(&need.place)?` → `< bytes` → `NotEnoughSpace { place: display, needed, available }`.
  - `stage_card`: for partition `p`, source `s`: folder → root is the path; else `let dir = staging_root.join(format!("{p}-{s}")); create_dir_all(&dir); content::prepare(path, kind, &dir, clock, progress)?.root`, accumulating `left_behind`. `needs = find_title_needs(&all_roots)`; titles non-empty and `!tree_has_whdload(tree)` → `find_whdload(material, hardfile_sources)` → `None` → `WhdloadNotFound { titles, searched }`. `kickstarts = propose_kickstarts(&needs, collection_dirs, material)`. Upper-bound System recheck: clone `system.measure.content`, `add_file("WHDLoad", binary.len)`, `add_file("WHDLoad.prefs", …)`, and for every `Supplied` item `add_file(name, rom size)` + `add_file(name.RTB, 8192)` (Devs/Kickstarts as one extra `add_directory`); `pfs3_fits(system_built_blocks, &content)` false → `CardDoesNotFit(PartitionContentDoesNotFit { "System", … })`. System's built blocks: `measured.plan.partitions[0].bytes / 512`.
  - Cancellation: `progress.is_cancelled()` between sources → `Err(CoreError::Cancelled)`.

- [ ] **Step 4: Green.** `cargo test --lib core::cardos::prepare` and `core::card::sizing` (the `absorb` test).

- [ ] **Step 5: Mutations.** M10a: the writer rule always `Native` → both writer tests red. M10b: `space_needs` does not sum one volume → free-space test red. M10c: the System recheck removed → add `a_whdload_and_kickstarts_that_push_system_over_are_refused` (a tree measured just under the limit) and see it red. M10d: the driver found after the measures (order) — survives by nature (same result); **disclosed as a survivor, judged: the order is a latency property, not a correctness one**.

- [ ] **Step 6: Commit.** `cardos: measure, space needs and staging for one card`.

---

### Task 11: Free space on the host — `tools/free_space.rs`

**Files:**
- Create: `src-tauri/src/tools/free_space.rs`; Modify: `src-tauri/src/tools/mod.rs`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `THIRD_PARTY_LICENSES.md`

**Interfaces:**
- Produces: `pub fn available_bytes(path: &Path) -> std::io::Result<u64>` — the bytes the calling user may write on the volume holding `path` (`lpFreeBytesAvailableToCaller`); `path` must exist (the nearest existing ancestor is used when it does not).

- [ ] **Step 1: Failing tests.**

```rust
#[test]
fn the_scratch_volume_reports_some_free_bytes() {
    let dir = std::env::temp_dir(); // TMP is E: by .cargo/config.toml
    assert!(available_bytes(&dir).unwrap() > 0);
}
#[test]
fn a_path_that_does_not_exist_yet_is_answered_for_its_nearest_existing_folder() {
    let dir = std::env::temp_dir().join("art-free-space-not-there").join("deeper");
    assert!(available_bytes(&dir).unwrap() > 0);
}
#[test]
fn it_agrees_with_what_windows_reports_for_the_same_volume() {
    // Run `fsutil volume diskfree <drive>` is not allowed in a unit test (a process spawn outside core is
    // fine here — this is tools/, not core — but the output is localised). Instead: two calls a moment apart
    // on the same folder differ by less than 1 GiB. Weak; the local check in Step 4 is the real one.
}
```

- [ ] **Step 2: Red** (module missing).

- [ ] **Step 3: Implement.** Cargo.toml:

```toml
[target.'cfg(windows)'.dependencies]
windows-sys = { version = "0.61.2", features = ["Win32_Storage_FileSystem"] }
```

(merge into an existing `[target.'cfg(windows)'.dependencies]` table if there is one). `cargo update -p windows-sys@0.61.2 --offline` only if the lock needs the feature recorded — the version must stay 0.61.2; check `git diff src-tauri/Cargo.lock` adds no new package. Implementation:

```rust
pub fn available_bytes(path: &Path) -> std::io::Result<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let mut at = path;
    while !at.exists() {
        at = at.parent().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound,
            format!("no part of '{}' exists", path.display())))?;
    }
    let wide: Vec<u16> = at.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut free: u64 = 0;
    // SAFETY: `wide` is NUL-terminated and outlives the call; the two null out-pointers are optional.
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut()) };
    if ok == 0 { Err(std::io::Error::last_os_error()) } else { Ok(free) }
}
```

`lib.rs` allows only `dead_code`; if `unsafe_code` is denied anywhere, add a module-level `#[allow(unsafe_code)]` here only, with the reason. `THIRD_PARTY_LICENSES.md`: a bullet **windows-sys** (MIT / Apache-2.0) — "**Outside `core/`**, in `tools/free_space.rs`, for `GetDiskFreeSpaceExW`: a card build refuses before it writes when the image's or the scratch's volume is short (design § 6). Already in the build through other crates; this makes it a direct dependency with one feature."

- [ ] **Step 4: Green, and one outside check.** `cargo test --lib tools::free_space`; then `cargo deny check > …/t11-deny.txt 2>&1` → `advisories ok, bans ok, licenses ok, sources ok`. Outside check (record both numbers in the commit message): PowerShell `(Get-PSDrive E).Free` and a one-off `#[ignore]` test printing `available_bytes("E:\\")` — within 1 GiB.

- [ ] **Step 5: Mutation.** M11: return `total` instead of `free` (pass the second out-pointer) — survives the unit tests (both > 0); the outside check catches it. **Disclosed survivor; the outside check is the guard.**

- [ ] **Step 6: Commit.** `tools: free space on the host volume (windows-sys, one feature)`.

---

### Task 12: `commands/cardos.rs` — the session, `card_os_prepare`, `card_os_build`, `card_os_close`

**Files:**
- Create: `src-tauri/src/commands/cardos.rs`; Modify: `commands/mod.rs`, `src-tauri/src/lib.rs` (manage `CardOsSessions`, four handlers)
- Modify: `src-tauri/src/commands/preload.rs` (`run_with_fallback`, `Stopped` and its fields, `StepReport` → `pub(crate)`)
- Modify: `src-tauri/src/commands/card.rs` (`write_card_image` split out; `report_for`, `CardReport` usable)
- Create: `src/lib/cardOs.ts`, `src/lib/cardOs.test.ts`; Modify: `src/i18n/en.json`, `src/i18n/tr.json`

**Interfaces:**
- Consumes: Tasks 3, 5, 6, 8, 9, 10, 11; `commands::card::{CardBuildRequest, write_card_image}`; `commands::preload::run_with_fallback`; `core::preload::{plan, PreloadRequest, PreloadPartition}`; `core::card::health::check_image`; `core::card::manifest::{describe_card, manifest_path_for, render_manifest, PartitionContent}`.
- Produces (Rust):

```rust
pub struct CardOsSession { scratch: OwnedScratch, prepared: Option<PreparedCard>, busy: bool }
#[derive(Default)] pub struct CardOsSessions(std::sync::Mutex<HashMap<u64, CardOsSession>>);

#[derive(Serialize)] #[serde(rename_all = "camelCase")]
pub struct CardOsOpened { pub session: u64, pub tree: String }
#[tauri::command] pub fn card_os_open(sessions: State<'_, Arc<CardOsSessions>>) -> AppResult<CardOsOpened>;

#[derive(Deserialize)] #[serde(rename_all = "camelCase")]
pub struct CardOsPrepareRequest {
    pub session: u64, pub card_gb: u32, pub image: String,
    pub partitions: Vec<PartitionInput>, pub material: Vec<String>,
    #[serde(default)] pub pfs3_driver: Option<String>,
    /// The card's boot Kickstart; its folder joins the Kickstart collection (P4).
    #[serde(default)] pub kickstart: Option<String>,
    #[serde(default)] pub hst_imager_path: String,
}
pub const CARD_OS_PREPARE_EVENT: &str = "card-os-prepare-result";
#[derive(Serialize)] #[serde(rename_all = "camelCase")]
pub struct CardOsPrepareResult { pub job_id: JobId, pub session: u64, pub prepared: PreparedCard }
#[tauri::command] pub fn card_os_prepare(request: CardOsPrepareRequest, app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>, sessions: State<'_, Arc<CardOsSessions>>, oplog: State<'_, JsonlOperationLog>) -> AppResult<JobId>;

#[derive(Deserialize)] #[serde(rename_all = "camelCase")]
pub struct CardOsBuildRequest {
    pub session: u64,
    /// The Kickstart names the user agreed to (decision 1). Never paths.
    pub agreed_kickstarts: Vec<String>,
    /// Everything `card_build` takes except `dest`, `partitions`, `file_systems`, `first_disk_bytes`,
    /// `extra_disks`, `total_bytes`, `card_gb` — those come from the prepared card.
    pub archive: String, pub kickstart: Option<String>, pub label: String,
    pub hardware: PistormHardware, pub line: Emu68Line,
    #[serde(default)] pub firmware: FirmwareConfig, #[serde(default)] pub options: Emu68Options,
    #[serde(default)] pub built_at: Option<String>, #[serde(default)] pub hst_imager_path: String,
}
pub const CARD_OS_BUILD_EVENT: &str = "card-os-build-result";
#[derive(Serialize)] #[serde(tag = "ending", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum CardOsEnding { Succeeded, Failed { phase: BuildPhase, code: String, message: String }, Stopped { phase: BuildPhase } }
#[derive(Serialize, Clone, Copy)] #[serde(rename_all = "kebab-case")]
pub enum BuildPhase { Whdload, Card, Partitions, Check }
#[derive(Serialize)] #[serde(rename_all = "camelCase")]
pub struct CardOsBuildResult {
    pub job_id: JobId, pub session: u64, pub image: String, pub ending: CardOsEnding,
    pub whdload: Option<WhdloadInstalled>, pub kickstarts: Vec<PlacedKickstart>,
    pub steps: Vec<StepReport>, pub manifest_path: Option<String>,
    pub health: Option<HealthReport>,
    /// Always said: what happened to `<image>.partial`.
    pub partial: PartialRemoval,
    /// The session folder, when it could not be removed.
    pub scratch_left: Option<LeftBehind>,
}
#[tauri::command] pub fn card_os_build(request: CardOsBuildRequest, app: AppHandle, registry: State<'_, Arc<JobRegistry>>,
    sessions: State<'_, Arc<CardOsSessions>>, oplog: State<'_, JsonlOperationLog>) -> AppResult<JobId>;

#[derive(Serialize)] #[serde(rename_all = "camelCase")]
pub struct CardOsClosed { pub scratch_left: Option<LeftBehind> }
#[tauri::command] pub fn card_os_close(session: u64, sessions: State<'_, Arc<CardOsSessions>>) -> AppResult<CardOsClosed>;

/// The build, without Tauri: what `card_os_build` runs on its job thread and Task 13 runs in a test.
pub(crate) fn build_card_os(prepared: &PreparedCard, tree: &Path, image: &Path, request: &CardOsBuildRequest,
    native: &dyn VolumeFormatter, fallback: Option<&dyn VolumeFormatter>, progress: &dyn ProgressSink) -> BuiltCardOs;
pub(crate) struct BuiltCardOs { pub ending: CardOsEnding, pub whdload: Option<WhdloadInstalled>,
    pub kickstarts: Vec<PlacedKickstart>, pub steps: Vec<StepReport>, pub manifest_path: Option<String>,
    pub health: Option<HealthReport>, pub partial: PartialRemoval, pub error: Option<CoreError> }
```

- Produces (TypeScript, `src/lib/cardOs.ts`): `cardOsOpen(): Promise<CardOsOpened>`, `cardOsPrepare(request): Promise<number>`, `onCardOsPrepareResult(handler)`, `cardOsBuild(request): Promise<number>`, `onCardOsBuildResult(handler)`, `cardOsClose(session): Promise<CardOsClosed>`, with interfaces mirroring the serde shapes above (camelCase), and the two event name constants.

- [ ] **Step 1: Failing tests (Rust, in `commands/cardos.rs`).** No Tauri in these: they drive `build_card_os` and the session registry's plain methods (`CardOsSessions::open_in(root)`, `take_for_build`, `close`) which the commands wrap.

```rust
#[test]
fn a_build_that_fails_after_the_image_exists_removes_the_partial_and_says_so() {
    let (_guard, dir) = scratch("fail-after-image");
    let (prepared, tree) = small_prepared_card(&dir);        // Task 13's fixture, 2 GiB card spec (see there)
    let image = dir.join("card.img");
    let failing = FailingFormatter { on: "SDH1" };           // format_partition errors for SDH1
    let built = build_card_os(&prepared, &tree, &image, &request_for(&dir), &failing, None, &NoProgress);
    assert!(matches!(built.ending, CardOsEnding::Failed { phase: BuildPhase::Partitions, .. }));
    assert!(matches!(built.partial, PartialRemoval::Removed { .. }));
    assert!(!partial_path_for(&image).exists() && !image.exists());
}

#[test]
fn a_stop_between_partitions_is_stopped_not_failed_and_removes_the_partial() {
    // a progress sink that cancels after the first FormatPartition step → CardOsEnding::Stopped { phase: Partitions }
}

#[test]
fn a_refusal_before_the_image_leaves_no_partial_and_reports_not_created() {
    // agreed_kickstarts names one not in the proposal → Failed { phase: Whdload, code "ART-KICKSTART-NOT-PROPOSED" },
    // partial NotCreated, nothing at image or image.partial, the tree unchanged
}

#[test]
fn a_missing_emu68_archive_is_refused_before_the_image() {
    // request.archive points at nothing → Failed { phase: Card }, partial NotCreated, no image, no image.partial
}

#[test]
fn closing_a_session_removes_its_folder_and_a_second_close_is_a_refusal() { /* open_in → close → Ok(None); close again → Err naming the id */ }

#[test]
fn a_session_cannot_be_built_twice_at_once() { /* take_for_build twice → second Err "busy" */ }
```

`FailingFormatter` implements `VolumeFormatter` by delegating to `NativeFormatter` except `format_partition` for the named drive. The success path is Task 13.

**TypeScript (`src/lib/cardOs.test.ts`):** mock `@tauri-apps/api/core` `invoke` as the neighbouring `cardBuild.test.ts` does; assert each wrapper calls the right command name with the right argument key (`request`, `session`), and that `onCardOsBuildResult` listens on `"card-os-build-result"`.

- [ ] **Step 2: Red** (Rust: module missing; TS: file missing).

- [ ] **Step 3: Implement.**
  - `commands/card.rs`: extract from `build_requested_card` the part up to `build_card` into `pub(crate) fn write_card_image(request: &CardBuildRequest, dest: &Path, progress) -> CoreResult<(BuiltCard, SourceFacts, Vec<ManifestFile>)>`; `build_requested_card` calls it with `request.dest` and writes the manifest as before (behaviour unchanged; its tests stay green). `report_for` → `pub(crate)`.
  - `commands/preload.rs`: `pub(crate)` on `run_with_fallback`, `Stopped` and its three fields, `StepReport` (already `pub`), no logic change.
  - Sessions: `open_in(root) -> CoreResult<(u64, PathBuf)>` creates `OwnedScratch::create_in(root, "card-os")`, and inside it `tree/`, `staging/`, `driver/`; ids from an `AtomicU64`. `card_os_open` resolves `crate::scratch::root()?` on the command thread.
  - `card_os_prepare`: on the command thread — the session exists and is not busy; `refuse_partial_destination(image)?`. Job (`JobTitle::new("components.jobBar.title.prepareCardOs").text("target", &image)`): `measure_card(…, hst_imager_available = !hst_imager_path.trim().is_empty(), clock = &LOCAL_TIME)`; `check_free_space(&space_needs(&card, image, session_root), available_bytes)`; `stage_card(card, tree, session/staging, material, collection_dirs)` where `collection_dirs` = material + the boot Kickstart's parent; store `prepared` in the session (replacing a previous one only after emptying `staging/` — a re-prepare with a non-empty staging folder removes its contents first, since they are ART's own from this session); emit `CARD_OS_PREPARE_EVENT`; oplog record "Prepare a card image" with each partition's source count and the proposal's item count, `write_to_path`. Errors end the job as errors (the job bar carries the code), as `card_build` does.
  - `build_card_os` phases, each mapping its error to `Failed { phase }` (or `Stopped { phase }` for `CoreError::Cancelled`):
    1. **Whdload** — `install_whdload` if `prepared.whdload` is `Some`; `place_agreed(&prepared.kickstarts, &request.agreed_kickstarts, tree)`.
    2. **Card** — build a `CardBuildRequest` from the request plus `card_gb: Some(plan's label)`, `partitions: plan.partitions.iter().map(|p| p.spec.clone())`, `file_systems: vec![FileSystemInput { path: driver.path, dos_type: "PDS3", version: None, revision: None }]`, `first_disk_bytes: 0`, `extra_disks: vec![]`, `boot_bytes: 0`, `dest: partial`; `write_card_image(&req, &partial, progress)`; from here `created = true`.
    3. **Partitions** — `PreloadRequest { image: partial, driver: None, rdb_backup: None, partitions: [System → content [tree]] ++ user partitions → roots, each `area: 1`, `index: i + 1`, `volume_name` }` → `plan(&request)?` → `run_with_fallback(&plan, native, fallback, progress)`; a `Stopped` carries its `steps` into the result.
    4. **Check** — `describe_card(&partial, source_facts, boot_files, built_at, os, partitions)` where `os` = `[release]` read from `tree/distribution.json` (`DistributionManifest.release`; absent or unreadable → empty, never guessed) and `partitions` from the plan, the measures and each step's tool; `check_image(&partial, Some(&manifest), pi)`; `!report.ok()` → `Failed { phase: Check, code: "ART-CARD-CHECK-FAILED", message: failures }`; then `finish_partial(image)?`; then `atomic_write(manifest_path_for(image), render_manifest(&manifest)?)` — the manifest is written **after** the rename, under the final name.
    On any non-success ending after `created`: `remove_partial(image, true)`.
  - `card_os_build` (command): takes the session out of the map for the job's duration (`busy`), runs `build_card_os` with `NativeFormatter::new(&LOCAL_TIME)` and `HstImager::at(path)` when set; **after every ending** removes the session: `scratch.finish()` → `scratch_left`; emits `CARD_OS_BUILD_EVENT` for every ending; writes the oplog record (§53: image, phase reached, ending, partial removal, scratch left); returns `Err(error)` for Failed/Stopped so the job bar does not say done.
  - `card_os_close`: removes the session and returns `CardOsClosed { scratch_left }`; an unknown id → `InvalidInput("no card session {id} is open")`.
  - `lib.rs`: `.manage(Arc::new(CardOsSessions::default()))`, handlers `commands::cardos::card_os_open`, `card_os_prepare`, `card_os_build`, `card_os_close`.
  - i18n: `components.jobBar.title.prepareCardOs`: en `"Preparing {{target}}"`, tr `"{{target}} hazırlanıyor"`; `buildCardOs`: en `"Building {{target}} with its partitions"`, tr `"{{target}} bölümleriyle oluşturuluyor"`.

- [ ] **Step 4: Green.** `cargo test --lib commands::cardos`, `commands::card`, `commands::preload`; `pnpm vitest run src/lib/cardOs.test.ts`; `pnpm lint` (unpiped).

- [ ] **Step 5: Mutations.** M12a: `remove_partial` not called on failure → fail test red. M12b: `Cancelled` mapped to `Failed` → stop test red (**endings stay distinct**). M12c: the manifest written before `finish_partial` under the partial's name → Task 13's end-to-end test red (run it for this mutation). M12d: `place_agreed` called with every `Supplied` name instead of `agreed_kickstarts` → the refusal test stays green; add `only_agreed_kickstarts_reach_the_tree` in Task 13 and run it here.

- [ ] **Step 6: Commit.** `commands: card_os_open/prepare/build/close — one session, .partial, run_with_fallback, every ending reported (ART-340)`.

---

### Task 13: The end-to-end card — built, read back by ART, and by hst-imager

**Files:**
- Test: `src-tauri/src/commands/cardos.rs` (the fixture `small_prepared_card` and the end-to-end tests)
- Modify: `scripts/pfs3-oracle-check.py` (a `--card IMAGE` mode)

**Interfaces:**
- Consumes: everything above.
- Produces: `#[cfg(test)] fn small_prepared_card(dir: &Path) -> (PreparedCard, PathBuf)` — a 2 GiB card (not 16 GB: `plan_card_image` cannot plan below a 16 GB label and a raw image costs its whole size, T § 4), built by calling `plan_card_image` **only** for the partition specs' shape and then scaling: build the `CardImagePlan` by hand with System 200 MiB, Games 100 MiB, Stuff 60 MiB, Work the rest of a 2 GiB image minus the 1.10 GiB boot partition, through the same `spec()` helper (make `sizing::spec` `pub(crate)`), and run `stage_card` on it so everything after the plan is the real path.

- [ ] **Step 1: Failing tests.**

```rust
#[test]
fn a_small_card_is_built_whole_and_reads_back_as_it_was_asked() {
    let (_guard, dir) = scratch("e2e");
    let (prepared, tree) = small_prepared_card(&dir); // tree: C/Dir + distribution.json release "AmigaOS 3.2.2";
                                                      // Games: a WHDLoad HDF (Turrican drawer + slave naming kick34005.A500);
                                                      // Stuff: a folder; material: pfs3aio, WHDLoad_usr.lha 20.0, skick346.lha; roms: the 1.3 ROM
    let image = dir.join("card.img");
    let native = NativeFormatter::new(&UTC_FOR_TESTS_STATIC);
    let built = build_card_os(&prepared, &tree, &image, &request_agreeing(&["kick34005.A500"]), &native, None, &NoProgress);
    assert!(matches!(built.ending, CardOsEnding::Succeeded), "{:?}", built.error);
    assert!(image.exists() && !partial_path_for(&image).exists());

    let card = crate::core::card::read_card(&image).unwrap();
    let parts: Vec<_> = card.areas[0].rdb.partitions.iter().map(|p| p.drive_name.clone()).collect();
    assert_eq!(parts, ["SDH0", "SDH1", "SDH2", "SDH3"]);

    // System through libpfs3: C/WHDLoad is WHDLoad 20.0's bytes, S/WHDLoad.prefs the package's,
    // Devs/Kickstarts/kick34005.A500 the ROM, .RTB the skick bytes.
    let system = read_pfs3_files(&image, &card, 0);   // helper over libpfs3's reader at the partition offset
    assert_eq!(system["C/WHDLoad"], whdload_bytes("20.0"));
    assert_eq!(system["Devs/Kickstarts/kick34005.A500"], rom_13());
    assert!(system.contains_key("Devs/Kickstarts/kick34005.A500.RTB"));
    let games = read_pfs3_files(&image, &card, 1);
    assert!(games.keys().any(|k| k.ends_with("Turrican.slave")));

    let manifest = read_manifest(&manifest_path_for(&image)).unwrap();
    assert_eq!(manifest.os, vec!["AmigaOS 3.2.2".to_string()]);
    assert_eq!(manifest.partitions.len(), 3);
    assert!(!built.health.unwrap().by_hand.iter().any(|s| matches!(s, ManualStep::VolumesNeedFormatting { .. })));
}

#[test]
fn only_agreed_kickstarts_reach_the_tree() {
    // same fixture, agreed = [] → Succeeded; System has no Devs/Kickstarts/kick34005.A500
}

#[test]
#[ignore = "writes a card for scripts/pfs3-oracle-check.py --card; set ART_CARD_OS_OUT to an existing folder on E:"]
fn writes_a_small_card_for_the_hst_imager_oracle() {
    let out = std::path::PathBuf::from(std::env::var("ART_CARD_OS_OUT").expect("ART_CARD_OS_OUT"));
    // build as above into out.join("card-os-e2e.img") (refuse if it exists), print every partition's
    // file list with SHA-256 to out.join("card-os-e2e.art.json")
}
```

`read_pfs3_files` exists in some form for round 2's content tests (`native::test_support`); reuse it, or read through `libpfs3::Volume` at `area.offset + partition.low_cyl * blocks_per_cyl * 512` as `firstboot/cardread.rs` does.

- [ ] **Step 2: Red** (fixture missing), then run with the fixture and see the real first failure — record it.

- [ ] **Step 3: Implement** the fixture and helpers; fix whatever the real run exposes **in the task that owns it** (a fix in Task 8's file is committed with a message naming Task 13's finding).

- [ ] **Step 4: Green, twice.** `cargo test --lib commands::cardos > …/t13-a.txt 2>&1`, then again `> …/t13-b.txt`.

- [ ] **Step 5: The outside check.** Extend `scripts/pfs3-oracle-check.py` with `--card IMAGE ART_JSON`: for each partition in `ART_JSON`, run `hst.imager fs dir -r <image-name>\mbr\<slot>\rdb\<drive lowercase>` with `cwd` the image's folder (the script's existing rule), and compare the path set and sizes with ART's list; `fs copy` each partition out to a temp folder under `ART_SCRATCH` and compare SHA-256 per file. Exit 0 all equal, 1 a difference (print it), 2 tool missing. Run locally:
`$env:ART_CARD_OS_OUT='E:\amiga\ProjeART\build\tmp\card-r3'; cargo test --lib writes_a_small_card_for_the_hst_imager_oracle -- --ignored` then `python scripts/pfs3-oracle-check.py --card E:\amiga\ProjeART\build\tmp\card-r3\card-os-e2e.img E:\amiga\ProjeART\build\tmp\card-r3\card-os-e2e.art.json`. **Record the counts per partition in the commit message.** Delete the 2 GiB image afterwards (ART's own test output).

Also, to prove P2's write half: a second ignored run with Stuff holding a non-ASCII name and `ART_HST` set, so Stuff goes through `HstImager` **under the `.partial` name**; the oracle reads it back. If hst-imager refuses to write under `.partial`, stop and report — P2 changes to `<stem>.partial.img` and Task 6 is amended.

- [ ] **Step 6: Commit.** `card round 3: the end-to-end card, read back by ART and by hst-imager`.

---

### Task 14: Records, and the whole suite twice

**Files:** `docs/ISSUES.md`, `docs/FEATURES.md`, `docs/STATUS.md`, `docs/session-log.md`, `CHANGELOG.md`, `docs/architecture.md`

- [ ] **Step 1: The whole suite, twice.** `cargo test --lib > …/t14-full-1.txt 2>&1` and `> …/t14-full-2.txt`; both `test result: ok.` — quote both lines. `pnpm test`, `pnpm lint`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo deny check`, `python scripts/control-byte-sweep.py`, `scratch-root-sweep.py`, `scratch-guard-sweep.py`, `contrast-check.py --quiet`, `rom-table-check.py`, `oracle-check.py` — each unpiped, each result line quoted.
- [ ] **Step 2: `scratch-root-sweep.py`** must accept `OwnedScratch::create_in(root, …)` as going through the scratch root; if it flags `commands/cardos.rs`, the call site is wrong, not the sweep.
- [ ] **Step 3: ISSUES.** ART-340 and ART-341 to Fixed with their test names and mutations; ART-339 already moved (Task 1); ART-342 stays open. Correct every Open/Fixed citation of `.superpowers/sdd/2026-09-16-one-button-card-round-2/` to say the folder is not on disk and name the plan instead. Recount Open with STATUS's method.
- [ ] **Step 4: FEATURES** — flip only rows with a test: the one-button card's prepare/build core and commands (🟡 until round 4's screen and the owner's card on a PiStorm); WHDLoad phase; Kickstart proposal.
- [ ] **Step 5: STATUS** (in place), **session-log** (a row on top), **CHANGELOG** `[Unreleased]` (user-visible: none until round 4 — say "no screen yet"), **architecture.md** (the `core/cardos` layer and why it sits on top; the session model). Check whether CHANGELOG carries round 1's "saved tables one cylinder smaller" note (T § 7 D12); record the answer in session-log.
- [ ] **Step 6: Commit.** `docs: card round 3 recorded`.

---

## Self-review (done while writing)

1. **Spec coverage.** § 5 phases 4–8 → Tasks 10, 8/9, 6/12, 12, 5/12. § 6 each refusal → Tasks 7 (no pfs3aio), 12 (no Emu68 archive: `write_card_image` → `payload_for` already refuses; `a_missing_emu68_archive_is_refused_before_the_image`), 4/10 (does not fit), 6 (exists), 10/11 (space), 10 (unreadable source), 10 (non-ASCII). § 7 components → the File structure table. § 8.3 → Task 13. § 8.4 (owner material hook) → **not in this round**: round 2's hooks cover content; a card from `sonuclar` + Enzo HDFs is a 16 GB+ image — added to "owed by the owner". § 8.5 frontend → round 4. § 8.6 mutations → per task.
2. **Placeholder scan.** The fixture helpers named from neighbouring test modules (`write_lha`, `SLAVE`, `slave_needing`, `read_pfs3_files`, `small_tree`) are instructions to reuse or expose an existing helper, each with the fallback construction stated.
3. **Type consistency.** `PartitionInput`, `PreparedCard`, `KickstartProposal`, `PlacedKickstart`, `WhdloadInstalled`, `PartialRemoval`, `LeftBehind`, `StepReport` are defined once (Tasks 3, 6, 8, 9, 10) and used with the same names in 12 and 13. `plan_card_image`'s new middle parameter appears in Tasks 4 and 10.

## Owed by the owner after this round

- The image written to a real card, booted on the PiStorm, `version full`, one game run (design § 8.7).
- A card from the owner's own material (design § 8.4): the 3.2.2 tree, a few Enzo `[A]` HDFs, part of `WHDLoadDemos100.lha`, listed by hst-imager.
