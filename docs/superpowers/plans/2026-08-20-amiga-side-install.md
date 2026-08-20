# Amiga-Side Install Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** run a package's own installer inside the emulator, on a copy of the distribution tree, and read the result back — so the packages ART cannot place from the host can still be installed.

**Architecture:** ART mounts the tree as data and its **own work volume** as the boot device, at the highest boot priority. The work volume carries a generated AmigaDOS script that runs the installer the package's recipe names and writes a result file. The host polls that file — measured to be visible live — and terminates the emulator it started. The copy replaces the original only on success.

**Tech Stack:** Rust (`core/amigainstall`, `core/winuae`, `core/osinstall`), React + i18next, Tauri commands.

**Spec:** [docs/superpowers/specs/2026-08-20-amiga-side-install-design.md](../specs/2026-08-20-amiga-side-install-design.md)
**Research:** [docs/superpowers/specs/2026-08-20-amiga-side-install-research.md](../specs/2026-08-20-amiga-side-install-research.md)

## Global Constraints

- `src-tauri/src/core/` is platform-independent: no `use tauri`, no Windows APIs, no network. Launching a process is allowed and has precedent — `core/winuae.rs::launch_winuae` already does it with a structured argv, never a shell string (§56).
- A lower-level `core/` module must not import a higher-level one. `commands/*.rs` are thin adapters.
- **Every write goes through `core/safety`**; never `std::fs::write` on a user file. **Never destroy the original before successful validation** (§92).
- Long operations run through `spawn_job` off the command thread (§54) and check `is_cancelled()` **only between whole units of work**, never mid-write.
- `core::security::safe_join` is the only route from an untrusted name to a path.
- Recipes are **data**: a fourth package is a JSON file, not a code path.
- Every user-visible string in **both** `src/i18n/en.json` and `src/i18n/tr.json`, same commit. `src/lib/*.ts` is the only place calling `invoke`.
- New commands go in **both** `invoke_handler![]` and a typed wrapper.
- Unused imports are compile errors. Doc comments explain *why*.
- **Spec §89**: never claim what is not implemented and tested. A timeout is reported as a timeout, never as a failure and never as a success.
- **No protection is bypassed.** ART runs the installer the package ships; it decrypts nothing.
- Fixtures are synthetic and built at runtime. Runs against the owner's real material are `#[ignore]`d and environment-gated.

## Two failure shapes this project keeps meeting

Check your own work for both before reporting, every task:

- **A guard that passes vacuously.** Revert your fix; if the test still passes, it is not a test.
- **A fixture more helpful than reality.** A real BoingBag's payload declares no top-level directories; a real 3.9 disc has no Joliet; a real `.lha` has level-1 headers. If your fixture is tidier than the owner's material, it is hiding the defect you are about to ship.

And a third, learned this week: **a written figure nobody re-ran.** If you state a count or a duration, produce it with something checked in.

## File Structure

| File | Responsibility |
|---|---|
| `src-tauri/src/core/amigainstall/mod.rs` (new) | The outcome types and the module's rules |
| `src-tauri/src/core/amigainstall/workvol.rs` (new) | Build ART's own boot volume: the script, the result contract |
| `src-tauri/src/core/amigainstall/run.rs` (new) | Launch, poll, deadline, terminate |
| `src-tauri/src/core/amigainstall/stage.rs` (new) | Copy the tree, swap on success, discard otherwise |
| `src-tauri/src/core/osinstall/package.rs` | `Package` gains the installer declaration |
| `src-tauri/src/core/osinstall/recipes/packages/*.json` | The BoingBags declare theirs |
| `src-tauri/src/commands/amigainstall.rs` (new) | Preview (read-only) and run (a job) |
| `src/lib/amigainstall.ts` (new) | Typed wrappers and mirrored types |
| `src/components/osbuilder/AmigaInstallPanel.tsx` (new) | Preview → confirm → job → report |

---

## Task 1: The work volume, and what it promises

The Amiga side's whole contract lives here, and it is testable **without an emulator** — this task writes a directory and asserts what is in it.

**Files:**
- Create: `src-tauri/src/core/amigainstall/mod.rs`, `src-tauri/src/core/amigainstall/workvol.rs`
- Modify: `src-tauri/src/core/mod.rs`
- Test: inline in `workvol.rs`

**Interfaces:**
- Produces:

```rust
/// What a run ended as. **Three outcomes, not two** — see the spec's §3.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RunOutcome {
    Succeeded { message: String },
    /// The installer ran and said no.
    Failed { message: String },
    /// Nobody answered its question. Not a failure, and explicitly not a
    /// success — the two are fixed by different things.
    TimedOut { waited: Duration },
}

/// Build ART's own boot volume into `at`.
pub fn build(at: &Path, run: &PlannedRun) -> CoreResult<()>;

/// The file the Amiga writes and the host reads.
pub const RESULT_FILE: &str = "art-result.txt";
```

**Read first:** `core/winuae.rs`'s `DirMount` — its `boot_priority` doc comment explains the AmigaDOS `BootPri` convention (*higher boots first*) and why ART's own directory gets the highest of anything mounted. This volume is that mechanism used a second time.

**What the script must do, and the order matters.** The research measured that a line appended *after* a `Startup-Sequence` never runs, because the sequence ends with `LoadWB`/`EndCLI`. This volume's sequence is ART's own, so it ends where ART says — but the rule it teaches still applies: **write the result before anything that might not return.**

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn planned(command: &str) -> PlannedRun {
        // A minimal run: one command, on a package called "test-pack".
        // Fill in against the real struct once Task 2 defines it.
        todo!("build a PlannedRun for {command}")
    }

    /// The volume boots ART's script, not the user's system.
    #[test]
    fn the_work_volume_carries_its_own_startup_sequence() {
        let dir = scratch("workvol-startup");
        build(&dir, &planned("C:Updater")).unwrap();
        let ss = std::fs::read_to_string(dir.join("S/Startup-Sequence")).unwrap();
        assert!(ss.contains("Updater"), "got {ss}");
    }

    /// The result is written **before** the installer runs, then again after,
    /// so a run that never returns is still distinguishable from one that was
    /// never started. A hang and a crash look identical otherwise.
    #[test]
    fn a_started_run_is_marked_before_the_installer_is_invoked() {
        let dir = scratch("workvol-order");
        build(&dir, &planned("C:Updater")).unwrap();
        let ss = std::fs::read_to_string(dir.join("S/Startup-Sequence")).unwrap();
        let started = ss.find("started").expect("a started marker");
        let invoke = ss.find("Updater").expect("the installer");
        assert!(started < invoke, "the marker must be written first:\n{ss}");
    }

    /// The script writes an outcome whether the installer succeeded or not.
    /// Without this a failure and a hang are the same silence.
    #[test]
    fn the_script_records_an_outcome_on_both_paths() {
        let dir = scratch("workvol-both");
        build(&dir, &planned("C:Updater")).unwrap();
        let ss = std::fs::read_to_string(dir.join("S/Startup-Sequence")).unwrap();
        assert!(ss.to_lowercase().contains("if warn") || ss.to_lowercase().contains("if fail"),
            "the script must branch on the installer's return code:\n{ss}");
    }

    /// Nothing ART generates is assembled from a string ART did not author.
    /// A package name is shipped data; an archive's contents are not.
    #[test]
    fn a_command_with_amigados_metacharacters_is_refused() {
        let dir = scratch("workvol-meta");
        for hostile in ["C:Updater ; Delete SYS:#?", "C:Updater\nDelete SYS:#?", "C:Up\"dater"] {
            assert!(build(&dir, &planned(hostile)).is_err(), "{hostile} should be refused");
        }
    }

    /// The volume must not be a place the installer can escape from.
    #[test]
    fn the_work_volume_contains_only_what_art_wrote() {
        let dir = scratch("workvol-contents");
        build(&dir, &planned("C:Updater")).unwrap();
        let mut found: Vec<String> = walk_relative(&dir);
        found.sort();
        assert_eq!(found, vec!["S/Startup-Sequence".to_string()]);
    }
}
```

`scratch` is this codebase's tempdir helper — **and it must carry a counter**, not a bare timestamp. Five separate instances of that defect were fixed this week (ART-164, ART-173, the 26-site sweep, `open_nested`'s temp file, `launch_winuae`'s config); do not add a sixth. `walk_relative` you write.

- [ ] **Step 2: Run them, watch them fail, then write the module**

Run: `cd src-tauri && cargo test amigainstall::workvol`

The script's shape, which the tests above pin:

```
; Written by ART. Runs one installer and reports what happened.
Echo >WORK:art-result.txt "started"
<the installer command>
If Warn
  Echo >WORK:art-result.txt "failed"
Else
  Echo >WORK:art-result.txt "ok"
EndIf
```

Say in a doc comment **why the started marker exists**: without it the host cannot tell a run that never began from one that hung, and those are fixed by different things.

- [ ] **Step 3: Green, and commit**

```
cd src-tauri && cargo test amigainstall && cargo fmt && cargo clippy --all-targets -- -D warnings
git add -A && git commit -m "feat(amigainstall): ART's own boot volume, and what it promises"
```

---

## Task 2: A recipe says what to run

**Files:**
- Modify: `src-tauri/src/core/osinstall/package.rs`, `recipes/packages/boingbag-39-1.json`, `boingbag-39-2.json`
- Test: inline

**Interfaces:**
- Produces: `Package::amiga_installer: Option<AmigaInstaller>`, and

```rust
/// What to run on the Amiga to install this package, when ART cannot place
/// its files from the host. **Data, not a code path** — a fourth package is a
/// JSON file. `None` means this package is not one this round can run, which
/// is the honest answer for a package ART already places directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmigaInstaller {
    /// Where the program sits **inside the package**, `/`-separated.
    pub program: String,
    /// Arguments, each a separate string — never one line to be split, so
    /// nothing can be reinterpreted as a second command.
    #[serde(default)]
    pub args: Vec<String>,
}
```

**Read first:** the ART-166 entry in `docs/ISSUES.md`. It records what was measured about the BoingBags: their `AmigaOS-Update` payloads are ZipCrypto-encrypted and the password lives in the package's own `Updater`. That `Updater` is what this field names.

**Measure before you write the JSON.** The wrapper archive's own listing says where `Updater` is — `scripts/lha-header-census.py` walks all 44 archives and is checked in. Read the real path out of `BoingBag39-1.lha` rather than assuming `C/Updater`, and put what you read in your report. The 3.9 recipe's fourteen paths were all wrong on their first real run for exactly this reason.

- [ ] **Step 1: Write the failing tests**

```rust
    /// Both BoingBags declare an installer; the Turkish pack does not, because
    /// ART already places its files directly.
    #[test]
    fn the_boingbags_declare_an_installer_and_the_locale_pack_does_not() {
        assert!(super::by_id("boingbag-39-1").unwrap().amiga_installer.is_some());
        assert!(super::by_id("boingbag-39-2").unwrap().amiga_installer.is_some());
        assert!(super::by_id("locale-turkish").unwrap().amiga_installer.is_none());
    }

    /// The declared program exists inside the archive it belongs to. A path
    /// nobody checked is how the 3.9 recipe shipped fourteen wrong ones.
    #[test]
    #[ignore = "reads the owner's real packages; run explicitly"]
    fn every_declared_installer_exists_in_its_own_archive_when_asked() {
        let Ok(folder) = std::env::var("ART_PACKAGE_FOLDER") else { return };
        // For each shipped package with an installer: open its archive through
        // ArchiveSource, resolve `program`, and assert it is a file.
    }
```

Write the second test's body. It is `#[ignore]`d and gated, so CI stays green with no packages present.

- [ ] **Step 2: Run, watch fail, implement, commit**

---

## Task 3: The run — launch, poll, deadline, terminate

**Files:**
- Create: `src-tauri/src/core/amigainstall/run.rs`
- Test: inline

**Interfaces:**
- Consumes: `core::winuae::{DirMount, LaunchMedia, generate_uae_config, launch_winuae}`, `AmigaProfile`.
- Produces: `pub fn run(plan: &PlannedRun, sink: &dyn ProgressSink) -> CoreResult<RunOutcome>`.

**The measurement this rests on**, from the research note: a file the Amiga writes into a `filesystem2=rw` directory mount **appears on the host while the emulator is still running** — verified with the pid alive at that moment. So the host polls; it does not wait for exit, and nothing needs the Amiga to quit the emulator.

**Three rules:**

1. **Poll, do not busy-wait.** A sleep between reads, and `is_cancelled()` checked between polls — never in the middle of one.
2. **The deadline is not optional.** When it expires ART terminates the emulator it started and returns `TimedOut { waited }`. Never `Failed`.
3. **ART owns the process it started, and only that one.** Terminate by the pid `launch_winuae` returned. Never by name — the owner may have their own WinUAE open, and killing it would be ART destroying something it does not own.

- [ ] **Step 1: Write the failing tests**

These test the loop, not the emulator: point it at a directory and write the result file yourself from the test.

```rust
    /// The happy path: a result file appears, the run reports what it said.
    #[test]
    fn a_result_written_while_running_is_read_and_reported() { /* … */ }

    /// The deadline produces a third outcome, not a failure.
    #[test]
    fn a_run_that_never_reports_times_out_rather_than_failing() { /* … */ }

    /// "started" alone is not an outcome — it means the run began and did not
    /// finish, which is a timeout, not a success.
    #[test]
    fn a_started_marker_alone_is_not_treated_as_an_outcome() { /* … */ }

    /// Cancellation between polls stops the run and leaves nothing behind.
    #[test]
    fn cancelling_between_polls_terminates_and_reports_cancelled() { /* … */ }
```

Write each body. The emulator launch must be behind something a test can substitute — read how `core/preload`'s `VolumeFormatter` is structured for the same reason and follow it, or explain in your report why a different shape is better here.

- [ ] **Step 2: Implement, green, commit**

---

## Task 4: The original is never the thing being changed

**Files:**
- Create: `src-tauri/src/core/amigainstall/stage.rs`
- Test: inline

**Interfaces:**
- Produces: `pub fn stage(tree: &Path) -> CoreResult<Staged>`, with `Staged::commit(self)` and `Staged::discard(self)`.

§92: the install runs against a **copy**; the copy replaces the original **only** when the result says success. A failed or timed-out run leaves the original untouched and the copy in place for the user to look at — and the report says where it is.

- [ ] **Step 1: The tests, and the one that matters most**

```rust
    /// The whole point. A run that fails must leave the original byte-for-byte
    /// as it was — including its `.uaem` sidecars and its manifest.
    #[test]
    fn a_failed_run_leaves_the_original_byte_for_byte() { /* … */ }

    #[test]
    fn a_successful_run_replaces_the_original_with_the_copy() { /* … */ }

    /// A discarded copy leaves nothing behind.
    #[test]
    fn a_discarded_stage_removes_its_copy() { /* … */ }

    /// The swap must not be a delete-then-move: a crash between the two would
    /// leave the user with neither.
    #[test]
    fn the_swap_never_has_a_moment_with_no_tree() { /* … */ }
```

The last one is the design constraint: say in a doc comment how the swap achieves it, and if the filesystem cannot guarantee it, say precisely what the remaining window is rather than implying there is none.

- [ ] **Step 2: Implement, green, commit**

---

## Task 5: The commands

**Files:**
- Create: `src-tauri/src/commands/amigainstall.rs`, `src/lib/amigainstall.ts`
- Modify: `src-tauri/src/lib.rs`

Two commands: a **read-only preview** answering what would run, on which tree, with which package; and a **run**, through `spawn_job`, returning a job id.

Mirror `RunOutcome` exactly in TS, including the `kind` tag. Remember that `#[serde(rename_all)]` does **not** cascade to struct-variant fields on an enum — that was a real wire bug this week; pin the shape with a test.

- [ ] Steps: the preview, the job, the wrappers, the registration, green, commit.

---

## Task 6: The screen

**Files:**
- Create: `src/components/osbuilder/AmigaInstallPanel.tsx` and its test
- Modify: `src/components/osbuilder/OsInstall.tsx`, both i18n catalogues

**Read `OsInstall.tsx` first.** It already solves what you are about to meet — a catalogue loaded per selection, picks remembered per key, a guard against a late async load overwriting something the user just touched (ART-089), a progress bar fed by `job-progress`, and a failure path that speaks.

What this panel must say, and each is a §89 requirement rather than a nicety:

- **That it will open an emulator window.** A machine appearing on someone's desktop unannounced is the thing the last round got wrong.
- **Which package, which tree, and what will run.**
- **That the original is not touched until it succeeds**, and where the copy is if it does not.
- **The three outcomes as three outcomes.** A timeout says nobody answered the installer's question and suggests watching the window; a failure says the installer said no.

- [ ] Steps: the panel, the four tests (one per outcome plus the announcement), both catalogues, green, commit.

---

## Task 7: Run it against the owner's own BoingBags

Every test so far is synthetic. This is the task that finds what they cannot.

**The owner's material:** `E:\amiga\Amigatolon\paketler\BoingBag39-1.lha` and `BoingBag39-2.lha`; a 3.9 tree built from `E:\amiga\Amigatolon\iso\AmigaOS39.iso`; the licensed ROM at `E:\amiga\Shared\rom\amiga-os-310-a1200.rom`; WinUAE at `C:\Program Files\WinUAE\winuae64.exe`. Outputs under `E:\amiga\ProjeART\`.

- [ ] **Step 1: The gated hook**, mirroring `build_the_real_39_tree_when_asked`: `#[ignore]`d, environment-gated, CI green without it.

- [ ] **Step 2: Measure the deadline rather than choosing it.** Run BoingBag 39-1's `Updater` and **time it**. The spec says the deadline is a multiple of what a real installer takes on this machine, recorded with what it was measured from. Report the number.

- [ ] **Step 3: Run it, and let the packages be right.** If the declared program path is wrong, the archive is right and the recipe is wrong — fix the JSON, re-run, report before and after with real numbers. Do not adjust an assertion to match a disappointing result, and do not fix a second defect while reporting the first.

- [ ] **Step 4: Close the window.** The owner is at this machine. One run at a time, terminated when it is done, and say in the report that you did.

Report, measured: what ran, how long it took, what the result file said, files and bytes different in the tree afterwards, and every recipe change the run forced.

---

## Task 8: Boot it, and the documents

- [ ] **Step 1: Boot the BoingBag'd tree** with `core::winuae::real_boot_hook::boot_a_distribution_tree_when_asked` and the licensed ROM.

**Ask the running system, do not infer.** Read the version off the mount — have the tree write `Version >SYS:version.txt FULL` and read that file from the host — rather than interrupting `Startup-Sequence` to reach a shell. A healthy tree resists interruption by design, which this project learned by measuring, and the last round's biggest defect was found precisely by asking instead of inferring.

The bar: **a BoingBag'd tree boots and shows its update** — a different version from the 3.9 overlay's `45.1`. If it does not boot, that is this task's finding: report it with the same rigour and stop.

- [ ] **Step 2: The documents.** `docs/FEATURES.md`, `docs/STATUS.md`, `docs/ISSUES.md`, `CHANGELOG.md`. Every number from Task 7's report or the code, never from this plan.

Close [ART-166](../../ISSUES.md), [ART-159](../../ISSUES.md), [ART-171](../../ISSUES.md) and [ART-172](../../ISSUES.md) — but only the ones the run actually demonstrated. An issue closed because the code exists, rather than because it was seen to work, is the failure this whole round was built to stop.

- [ ] **Step 3: Verify and commit.** `pnpm test` and `cargo test`, both green.
