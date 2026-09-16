# First boot, phase 3 — the wizard and the reboot — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** On the first boot of a tree ART built, the Amiga opens only the Preferences windows ART did not set (Locale always, Input unless ART wrote a keymap, ScreenMode unless ART wrote a screen depth), and the step wrapper carries out a step's reboot request (`C:Wait 3` + `C:Reboot`, or a logged `reboot unavailable`).

**Architecture:** A fixed AmigaDOS script `S/FirstBoot/90-prefs` decides on the Amiga from two marker files in `Prefs/Env-Archive/` whose names live in `core::amigaprefs::env` (`ART_Set_Input` written by the first-boot write from `S/User-Startup`'s `keymap-selection` block; `ART_Set_ScreenMode` written by `core::appearance` with a depth). `core::firstboot::{plan,write,report}` gain the tick, the reboot availability, the wizard rows and three report fields; `core::amigainstall::rehearse` keeps polling through a reboot and names the window a wizard waits in. Commands stay thin; the OS Builder's Choice tab gets a second tick and two preview lines; the report panel and rehearsal speak the new endings.

**Tech Stack:** Rust (std, serde, serde_json, sha2 in tests), Tauri 2 commands, React + react-i18next, Vitest (jsdom for `*.test.tsx` only), AmigaDOS scripts (ASCII, LF).

**Spec:** `docs/superpowers/specs/2026-09-15-firstboot-phase-3-design.md` (approved, `c22ab52`). Its measurements are in `.superpowers/sdd/2026-09-15-firstboot-phase-3/experiments.md` and `research.md`; its decisions log is `progress.md` there. The older spec it partly replaces is `docs/superpowers/specs/2026-09-07-firstboot-design.md`.

---

## Spec deviations found while planning

Each is written the way the plan resolves it; the controller takes the list to the owner. Evidence was read on 2026-09-15 at `c22ab52`.

1. **§4.4 computes the wizard's Input row "from the markers" — the plan reads the block instead.** §4.3 (spec lines 225–236) makes the *write* derive `ART_Set_Input` from `S/User-Startup`'s block, and names the case "a tree ART built before this round has the block and no marker". A preview that read the marker file on such a tree would show *Input will be asked* and the write, one call later, would set the marker — the screen out-claiming the core. `plan.rs` therefore asks the same block query the write's marker comes from (`FirstBootPlan.input_set_by_art`), and the write uses that value. The ScreenMode row does read its marker file (Appearance writes it; nothing re-derives it).
2. **§3.1/§8.1 "pinned exact-match like every other script" vs the tree's always-written array.** `scripts.rs:25` is `fixed_files() -> [FixedFile; 7]`, which `write.rs:44` writes unconditionally and `plan.rs:92-101` turns into steps; `90-prefs` is conditional. The plan keeps `fixed_files()` unchanged, adds `pub const WIZARD_FILE: FixedFile` and `pub fn every_script() -> [FixedFile; 8]`, and moves the text-level guards (ASCII/LF, no `Quit`, braced reads, hash pin) onto `every_script()`. `every_step_that_skips_carries_its_end_label` (`scripts.rs:216-228`) stays over `fixed_files()`: `90-prefs` has no early exit, and a new guard pins that it never `Skip`s.
3. **§6's report has one new flag; the screen's "the Amiga restarted" needs a second.** `report.rs:151-153` records only the request. From `reboot_requested_by` alone the screen cannot tell *restarted* from *asked and the log stops there* (a rehearsal ended mid-reboot, a card switched off). The plan adds `restarted_after_request: bool` (a `system` line after the latest request that was not `unavailable`) and a **fourth** sentence, *asked for a restart; the log does not show the Amiga coming back up yet*, so the endings stay distinct.
4. **§7's waiting-window rule would name ScreenMode.** "The latest `opened` detail with no `ok` after it" matches `opened ScreenMode`, which §3.2 item 4 runs detached and which never holds the boot. The plan's `FirstBootReport.waiting_in: Option<ForegroundWindow>` names only Locale or Input, and only when that `opened` line is the **last** detail of an unfinished `90-prefs` step. The job-bar progress line is a Rust `sink.report` message that `JobBar.tsx:194-197` renders raw — English in both languages, exactly like today's `rehearse.rs:249` line (ART-060). The timed-out *ending* naming the window goes through `@/lib/firstboot` phrases and is translated.
5. **§5's windows line cannot be drawn for a fresh build.** `ChoiceTab.tsx:518-544` asks `firstboot_preview` only when `treeRoot` resolves, and the ordinary fresh build has none (`ChoiceTab.test.tsx:328-331`). The plan follows the tree and, instead of drawing nothing, says *which windows open is known once the tree exists* when first boot and the second tick are on and there is no tree.
6. **§4.2 "reports it in `AppearanceOutcome.written`" changes a pinned count.** `commands/appearance.rs` test (the one applying `screen_depth: Some(8)`, `assert_eq!(outcome.written.len(), 1)`) becomes 2, and `AppearancePanel`'s *N files written* sentence counts the marker. Task 5 updates the test and says so.
7. **§4.4 "removes ART's own `90-prefs`" names no mechanism; the plan chooses `guarded_remove(.., BackupPolicy::NONE)`, not the Recycle Bin.** `core/hostfs.rs:1-13` is for "a file from the **user's own disk**" chosen on the host; `core/safety/mod.rs:41-53` (`guarded_remove`, "Delete a file ART itself put there") and its callers `core/osinstall/apply.rs:829` and `:1710` are the precedent for ART's own generated files inside a tree it writes. `NONE` rather than those callers' `CONFIG`: they remove a `.uaem` sidecar that is "tiny, irreplaceable"; `90-prefs` is compiled-in text that re-ticking writes back byte for byte, and `write.rs:49` already replaces every fixed file with `atomic_write` and no backup — the removal keeps the write's policy (`core/appearance/mod.rs:781-795`'s C6 reasoning). `ART_Set_Input` is derived from the block on every write and follows the same rule. A backup would also plant `.art-backup/` inside `S/FirstBoot/` and `Prefs/Env-Archive/`, which the card carries to the Amiga.
8. **`Written` gains `removed: Vec<String>`.** A removal the result does not report is a thing ART did and did not say (CLAUDE.md, "never claim what you did not do" in its reverse). The oplog records it; the wire carries it.
9. **The tick travels on the phase, not on `useBuildRun`'s args.** `buildSummary.ts:215-221` builds the sequence from the session; the plan adds `SequenceInputs.firstBootAskPrefs` and `Phase.askPrefs`, so the run writes the tick the user confirmed. §5's "`useBuildRun`'s `firstboot` arm passes the tick to `firstbootWrite`" still holds.
10. **§10 lists "CLAUDE.md's command list if a new gated test command is added".** CLAUDE.md is outside the repository and no agent edits it. The two new gated commands go into STATUS.md's reproduce block (the place CLAUDE.md itself points at); adding them to CLAUDE.md is the owner's.
11. **Observed on the owner's trees, read-only, 2026-09-15 (not a deviation, an expectation for Task 10):** `os39/art5` has `C/Reboot`, `C/Wait`, `Prefs/{Locale,Input,ScreenMode}`, no `L/fat95`, and its `S/User-startup` carries **no** `keymap-selection` block; `sonuclar` has `C/Wait`, no `C/Reboot`, the three editors, `S/ART-FirstBoot` and `S/FirstBoot/{10-hardware,20-aux,30-datatypes}` already written, **no** `S/FirstBoot.log`, and a `User-Startup` holding only the `art-firstboot` block. So on both trees the wizard asks all three windows. The 3.1 ROM is `E:\amiga\Amigatolon\kickstart\Kickstart v3.1 rev 40.68 (1993)(Commodore)(A1200).rom` (`experiments.md` line 18).

---

## Global Constraints

- `core/` is platform-independent: `std`, `serde`, `serde_json`, `sha2`, `log`, `thiserror` and the listed decoders; no `tauri`, no Windows API, no network, no process spawn in `core/firstboot`, `core/appearance`, `core/amigaprefs`.
- The inward rule: `core::appearance` and `core::firstboot` never import each other; both read the marker names from `core::amigaprefs::env`. `core::osinstall` never imports `core::firstboot`.
- Every write or removal in a tree goes through `core::safety` (`atomic_write`, `guarded_write`, `guarded_remove`); never a bare `std::fs::write`/`remove_file` on a tree file.
- A marker's content is `TRUE` with no trailing newline, through `env::encode_value` (spec §3.1).
- Scripts are ASCII with LF, written with the Write tool (never a heredoc); every variable read is `${name}` — a `$` in a comment counts; no line begins with `Quit`; every command by full path (spec §3.2, `experiments.md` consequence 6).
- The reboot block in `ART-FirstBoot-Step` is spec §3.3 verbatim; `Wait 3` is measured on FFS only — nothing claims PFS3 or a real card.
- No new refusal and no new `ART-*` code (spec §4.4); a missing `C/Wait` is the existing `FirstBootNeedsCommand`.
- `src/lib` never renders: helpers return `Phrase { key, params? }`; components call `t()`. Every new key lands in `src/i18n/en.json` **and** `tr.json` in the same commit.
- Nothing changes unless the user changes it: `askPrefs` is optional, **absent means ticked**, rendering writes nothing, a stored `false` survives — the `wanted` rule (`buildSession.ts:171-187`).
- Beginner mode hides file names and AmigaDOS lines; it never disables a control.
- Endings stay distinct: *restarted*, *no reboot command*, *asked and not back yet*, *no restart*; *timed out waiting in a window* is a different sentence from *timed out*.
- Test scratch: `ScratchDir::new(prefix, tag)` bound to a named local, or `let (_guard, dir) = ScratchDir::pair(prefix, tag)` — never `_`. Fixtures are synthetic; ART ships no Amiga content.
- A test is not a guard until its defect is put back and seen to fail. Mutations: back the file up **by absolute path** to the scratchpad, mutate, run, restore with `python -c "import shutil; shutil.copyfile(r'<backup>', r'<file>')"` — never `git checkout --`, never `shutil.move`.
- Cargo is not on PATH: Bash `/c/Users/ismoz/.cargo/bin/cargo`, PowerShell `& "C:\Users\ismoz\.cargo\bin\cargo.exe"`. Iterate with a module filter (`cargo test --lib firstboot::`); the full suite takes ~400 s on E: and is run twice before the round closes. Quote the `test result:` line, never the exit code. Never pipe a command that can fail; redirect to a file and read the result lines.
- Never write to C:. Scratch files go to the session scratchpad or `.superpowers/sdd/2026-09-15-firstboot-phase-3/` (git-ignored); real-material copies go under `E:\amiga\ProjeART\`.
- Commits: `git branch --show-current` must print `art-firstboot-phase-3` first; stage files **by name** (a background agent may be running — never `git add -A`); message written to a file, `git commit -F <file>`, ending with the line `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`.

---

## File structure

**Create**
- `src-tauri/src/core/firstboot/scripts/90-prefs` — the wizard (fixed text, written only when asked).

**Modify — Rust**
- `src-tauri/src/core/amigaprefs/env.rs` — marker names, paths, value.
- `src-tauri/src/core/osinstall/startup.rs` — `pub fn has_block`, on `matched_blocks`.
- `src-tauri/src/core/firstboot/mod.rs` — `WIZARD_STEP`, `WIZARD_PATH`, `WizardWindow`, `ForegroundWindow`.
- `src-tauri/src/core/firstboot/scripts/ART-FirstBoot-Step` — the reboot block.
- `src-tauri/src/core/firstboot/scripts.rs` — `STEP_90_PREFS`, `WIZARD_FILE`, `every_script`, guards, re-pinned hashes.
- `src-tauri/src/core/firstboot/plan.rs` — `ask_prefs`, `RebootCommand`, `WindowState`, `WizardRow`, `WizardPlan`, `input_set_by_art`, `NEEDED_COMMANDS` + `Wait`.
- `src-tauri/src/core/firstboot/write.rs` — wizard write/remove, `ART_Set_Input` write/remove, `Written.removed`.
- `src-tauri/src/core/firstboot/report.rs` — `reboot_unavailable`, `restarted_after_request`, `waiting_in`.
- `src-tauri/src/core/appearance/mod.rs` — `ART_Set_ScreenMode` in the committed list.
- `src-tauri/src/core/amigainstall/rehearse.rs` — comment, `waiting_message`, tests, report literal.
- `src-tauri/src/commands/firstboot.rs` — `ask_prefs` on two commands, oplog `Files removed`, wire tests, three gated tests.
- `src-tauri/src/commands/appearance.rs` — the written-count test.

**Modify — TypeScript**
- `src/lib/firstboot.ts` (+ `firstboot.test.ts`) — wire types, wrappers, mappers.
- `src/lib/buildSession.ts` (+ `buildSession.test.ts`) — `askPrefs`.
- `src/lib/buildRun.ts` (+ `buildRun.test.ts`), `src/components/osbuilder/buildSummary.ts`, `src/lib/useBuildRun.ts` (+ `useBuildRun.test.tsx`) — the tick on the phase.
- `src/components/osbuilder/ChoiceTab.tsx` (+ `ChoiceTab.test.tsx`) — the second tick and its lines.
- `src/components/card/FirstBootReportPanel.tsx` (+ `.test.tsx`) — restart and wizard sentences.
- `src/components/osbuilder/FirstBootRehearse.test.tsx` — the waiting-window ending.
- `src/i18n/en.json`, `src/i18n/tr.json`, `src/i18n/phrase-keys.test.ts`, `src/i18n/literal-keys.test.ts`.
- Fixture-only type updates: `src/components/osbuilder/BuildTab.test.tsx`, `src/pages/HardDiskStudio.test.tsx`.

**Modify — documents** (Task 11): the 2026-09-07 spec, `docs/STATUS.md`, `docs/FEATURES.md`, `CHANGELOG.md`, `docs/session-log.md`, `docs/ISSUES.md` only for a defect found.

---

### Task 1: The marker names, and a public block query on the merge's own pairing

**Files:**
- Modify: `src-tauri/src/core/amigaprefs/env.rs` (after `pub const SHELL_DEFAULTS`, and its `mod tests`)
- Modify: `src-tauri/src/core/osinstall/startup.rs` (after `pub fn merge_user_startup`, and its `mod tests`)

**Interfaces:**
- Consumes: `startup.rs`'s private `fn matched_blocks(haystack: &str, begin_marker: &str, end_marker: &str) -> Vec<(usize, usize)>`; `env::encode_value(&str) -> CoreResult<Vec<u8>>`.
- Produces: `pub const ART_SET_INPUT: &str = "ART_Set_Input"`, `pub const ART_SET_SCREENMODE: &str = "ART_Set_ScreenMode"`, `pub const ART_SET_INPUT_PATH: &str = "Prefs/Env-Archive/ART_Set_Input"`, `pub const ART_SET_SCREENMODE_PATH: &str = "Prefs/Env-Archive/ART_Set_ScreenMode"`, `pub const MARKER_VALUE: &str = "TRUE"` in `core::amigaprefs::env`; `pub fn has_block(existing: &str, component: &str) -> bool` in `core::osinstall::startup`.

- [ ] **Step 1: Write the failing tests**

Append inside `env.rs`'s `mod tests`:

```rust
    /// Phase 3 design §3.1/§4.1: the host writes these, `S:FirstBoot/90-prefs`
    /// asks `IF EXISTS ENVARC:<name>` of them. One name, one place.
    #[test]
    fn the_two_markers_are_the_names_the_design_fixed() {
        assert_eq!(
            (ART_SET_INPUT, ART_SET_SCREENMODE),
            ("ART_Set_Input", "ART_Set_ScreenMode")
        );
    }

    #[test]
    fn a_marker_path_is_its_name_directly_under_env_archive() {
        assert_eq!(ART_SET_INPUT_PATH, format!("Prefs/Env-Archive/{ART_SET_INPUT}"));
        assert_eq!(
            ART_SET_SCREENMODE_PATH,
            format!("Prefs/Env-Archive/{ART_SET_SCREENMODE}")
        );
    }

    #[test]
    fn a_marker_is_true_with_no_trailing_newline() {
        assert_eq!(encode_value(MARKER_VALUE).unwrap(), b"TRUE".to_vec());
    }
```

Append inside `startup.rs`'s `mod tests`:

```rust
    // ---- phase 3: the one public reader of the pairing ----

    #[test]
    fn has_block_answers_yes_for_a_block_merge_wrote() {
        let text = merge_user_startup(
            Some("; mine\n"),
            "keymap-selection",
            &["SetKeyboard usa".into()],
        );
        assert!(has_block(&text, "keymap-selection"));
    }

    /// The module doc's "An opener with no closer at all is left alone": the
    /// merge appends after it rather than calling it a block, so a reader must
    /// not call it one either.
    #[test]
    fn has_block_says_no_to_a_stray_opener_with_no_closer() {
        let stray = ";BEGIN keymap-selection\nSetKeyboard usa\n; the END line is missing\n";
        assert!(!has_block(stray, "keymap-selection"));
    }

    #[test]
    fn has_block_says_no_to_another_components_block_and_to_a_longer_marker() {
        let other = ";BEGIN art-firstboot\nIF EXISTS S:ART-FirstBoot\n  Execute S:ART-FirstBoot\nENDIF\n;END art-firstboot\n";
        assert!(!has_block(other, "keymap-selection"));
        let longer = ";BEGIN keymap-selection-old\nSetKeyboard usa\n;END keymap-selection-old\n";
        assert!(!has_block(longer, "keymap-selection"));
    }

    #[test]
    fn has_block_reads_crlf_the_way_the_merge_does() {
        assert!(has_block(
            ";BEGIN keymap-selection\r\nSetKeyboard usa\r\n;END keymap-selection\r\n",
            "keymap-selection"
        ));
    }
```

- [ ] **Step 2: Run them and see them fail**

Run (Bash): `cd /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri && /c/Users/ismoz/.cargo/bin/cargo test --lib amigaprefs::env:: > /d/Projeler/Amiga/amiga-retro-toolkit/.superpowers/sdd/2026-09-15-firstboot-phase-3/t1.txt 2>&1; echo "exit $?"`
Expected: `exit 101`, and the file names `error[E0425]: cannot find value `ART_SET_INPUT` in this scope` (and `cannot find function `has_block``).

- [ ] **Step 3: Implement**

In `env.rs`, directly after `pub const SHELL_DEFAULTS … ];`:

```rust
/// The first-boot wizard's markers (phase 3 design §3.1, §4.1).
///
/// Written into `Prefs/Env-Archive/` by whichever part of ART set that
/// preference — `core::firstboot::write` for the keymap, `core::appearance`
/// for a screen depth — and asked by `S:FirstBoot/90-prefs` with
/// `IF EXISTS ENVARC:<name>`, so the Amiga skips a window ART already
/// answered whatever order the build's screens ran in. The names live here,
/// in the lower-level module, so those two modules never import each other.
pub const ART_SET_INPUT: &str = "ART_Set_Input";
pub const ART_SET_SCREENMODE: &str = "ART_Set_ScreenMode";
/// Tree-relative, `/`-separated, as `distribution.json` spells paths.
pub const ART_SET_INPUT_PATH: &str = "Prefs/Env-Archive/ART_Set_Input";
pub const ART_SET_SCREENMODE_PATH: &str = "Prefs/Env-Archive/ART_Set_ScreenMode";
/// A marker's content. The Amiga only asks `IF EXISTS`; the word is for a
/// person reading the drawer, and it goes through [`encode_value`] like every
/// other value here, so no newline follows it.
pub const MARKER_VALUE: &str = "TRUE";
```

In `startup.rs`, directly after `merge_user_startup`'s closing brace:

```rust
/// Whether `existing` carries a well-formed `;BEGIN <component>` /
/// `;END <component>` block — asked through the **same** [`matched_blocks`]
/// pairing [`merge_user_startup`] replaces, so a reader and the writer can
/// never disagree about what a block is. A stray opener with no closer is no
/// block (the module doc's "An opener with no closer at all is left alone"),
/// and neither is a marker that is only the start of a longer line.
pub fn has_block(existing: &str, component: &str) -> bool {
    let begin_marker = format!(";BEGIN {component}");
    let end_marker = format!(";END {component}");
    !matched_blocks(existing, &begin_marker, &end_marker).is_empty()
}
```

- [ ] **Step 4: Run them and see them pass**

Run: `… cargo test --lib amigaprefs::env:: > …/t1.txt 2>&1` then `… cargo test --lib osinstall::startup:: > …/t1b.txt 2>&1` (same prefix and folder as Step 2).
Expected: read with Grep `test result:` — `test result: ok. 10 passed; 0 failed` for `env` (7 before + 3) and `test result: ok. 14 passed; 0 failed` for `startup` (10 before + 4).

- [ ] **Step 5: Mutations — each guard seen red**

Back up first (Bash): `cp /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri/src/core/osinstall/startup.rs "C:/Users/ismoz/AppData/Local/Temp/claude/D--Projeler-Amiga/865963bf-6833-4088-89ee-ce72f56aa75c/scratchpad/startup.rs.bak" && cp /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri/src/core/amigaprefs/env.rs "C:/Users/ismoz/AppData/Local/Temp/claude/D--Projeler-Amiga/865963bf-6833-4088-89ee-ce72f56aa75c/scratchpad/env.rs.bak"` (the scratchpad is the session's; the implementer substitutes their own scratchpad path, still absolute).

1. In `has_block`, replace the last line with `existing.contains(&begin_marker)`. Run `cargo test --lib osinstall::startup::has_block`. Expected red: `has_block_says_no_to_a_stray_opener_with_no_closer` **and** `has_block_says_no_to_another_components_block_and_to_a_longer_marker`. Restore: `python -c "import shutil; shutil.copyfile(r'<scratchpad>\startup.rs.bak', r'D:\Projeler\Amiga\amiga-retro-toolkit\src-tauri\src\core\osinstall\startup.rs')"`.
2. Change `ART_SET_SCREENMODE_PATH` to `"Prefs/Env-Archive/Sys/ART_Set_ScreenMode"`. Run `cargo test --lib amigaprefs::env::`. Expected red: `a_marker_path_is_its_name_directly_under_env_archive`. Restore `env.rs` the same way.

Run both filters again; both `test result: ok`.

- [ ] **Step 6: Commit**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && git branch --show-current
git add src-tauri/src/core/amigaprefs/env.rs src-tauri/src/core/osinstall/startup.rs
git commit -F .superpowers/sdd/2026-09-15-firstboot-phase-3/msg-t1.txt
```

`msg-t1.txt` (Write tool):

```
First boot phase 3: marker names and a public block query (task 1)

core::amigaprefs::env gains ART_Set_Input / ART_Set_ScreenMode, their
Env-Archive paths and the TRUE value. core::osinstall::startup gains
has_block on merge_user_startup's own pairing, so a stray ;BEGIN is no
block for a reader either. Mutations: has_block as a substring search ->
two has_block tests red; a marker path under Sys/ -> the path test red.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
```

---

### Task 2: The Amiga side — `90-prefs`, the reboot in the step wrapper, `C/Wait` required

**Files:**
- Create: `src-tauri/src/core/firstboot/scripts/90-prefs`
- Modify: `src-tauri/src/core/firstboot/scripts/ART-FirstBoot-Step` (append after the ok/refused `ENDIF`; replace the three-line `IF ${ART_Reboot} EQ "TRUE"` block)
- Modify: `src-tauri/src/core/firstboot/mod.rs` (constants after `FLAG_PATH`)
- Modify: `src-tauri/src/core/firstboot/scripts.rs` (constants, `WIZARD_FILE`, `every_script`, tests)
- Modify: `src-tauri/src/core/firstboot/plan.rs` (`NEEDED_COMMANDS`, one test)

**Interfaces:**
- Consumes: `env::ART_SET_INPUT`, `env::ART_SET_SCREENMODE` (Task 1).
- Produces: `pub const WIZARD_STEP: &str = "90-prefs"`, `pub const WIZARD_PATH: &str = "S/FirstBoot/90-prefs"` in `core::firstboot`; `pub const STEP_90_PREFS: &str`, `pub const WIZARD_FILE: FixedFile`, `pub fn every_script() -> [FixedFile; 8]` in `core::firstboot::scripts`; `pub const NEEDED_COMMANDS: [&str; 9]` (adds `"Wait"`); the log lines `step 90-prefs detail opened|not-asked|missing Locale|Input|ScreenMode` and `reboot unavailable`.

- [ ] **Step 1: Write the failing tests**

In `mod.rs`, after `pub const FLAG_PATH …;` nothing yet — the test comes first. In `scripts.rs`'s `mod tests`, add `use crate::core::amigaprefs::env;` under `use super::*;`, then append:

```rust
    /// A script's command lines, trimmed, comments and blanks dropped — so a
    /// guard asks what runs and in what order, not where a substring sits.
    fn command_lines(text: &str) -> Vec<&str> {
        text.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with(';'))
            .collect()
    }

    fn at(lines: &[&str], wanted: &str) -> usize {
        lines
            .iter()
            .position(|l| *l == wanted)
            .unwrap_or_else(|| panic!("no line {wanted:?}"))
    }

    #[test]
    fn the_wizard_sits_in_the_step_directory_and_is_not_always_written() {
        assert_eq!(super::super::WIZARD_PATH, format!("{}/{}", super::super::STEP_DIR, super::super::WIZARD_STEP));
        assert_eq!(WIZARD_FILE.tree_path, super::super::WIZARD_PATH);
        assert!(!fixed_files().iter().any(|f| f.tree_path == WIZARD_FILE.tree_path));
        assert!(every_script().iter().any(|f| f.tree_path == WIZARD_FILE.tree_path));
    }

    /// Spec §3.3 and experiment 1b: 8 of 8 FFS volumes needed validation
    /// after a reboot with no wait, 0 of 6 with `Wait 3`.
    #[test]
    fn the_wrapper_waits_before_it_reboots() {
        let lines = command_lines(STEP_WRAPPER);
        assert_eq!(at(&lines, "C:Wait 3") + 1, at(&lines, "C:Reboot"));
    }

    /// Spec §2 decision 4: 3.9 ships no `C:Reboot`; the run logs that and
    /// carries on rather than failing on an unknown command.
    #[test]
    fn the_wrapper_reboots_only_when_the_tree_has_the_command() {
        let lines = command_lines(STEP_WRAPPER);
        let guard = at(&lines, "IF EXISTS C:Reboot");
        let reboot = at(&lines, "C:Reboot");
        let unavailable = at(&lines, "Echo >>S:FirstBoot.log \"reboot unavailable\"");
        let else_after = reboot + lines[reboot..].iter().position(|l| *l == "ELSE").unwrap();
        assert!(guard < reboot && reboot < else_after && else_after < unavailable);
        assert!(!lines[guard..reboot].contains(&"ENDIF"));
    }

    /// Spec §3.3: on 3.2 `ENV:` may be `ENVARC:` itself, so a `TRUE` left
    /// behind would restart the next boot too.
    #[test]
    fn the_wrapper_clears_and_reports_the_request_before_it_reboots() {
        let lines = command_lines(STEP_WRAPPER);
        let request = at(&lines, "IF ${ART_Reboot} EQ \"TRUE\"");
        let clear = at(&lines, "SetEnv ART_Reboot \"FALSE\"");
        let said = at(&lines, "Echo >>S:FirstBoot.log \"reboot requested by [name]\"");
        let guard = at(&lines, "IF EXISTS C:Reboot");
        assert!(request < clear && clear < said && said < guard);
        let deleted = at(&lines, "Delete >NIL: S:FirstBoot/[name]");
        assert!(deleted < request, "the step is reported and deleted before any reboot");
    }

    /// Experiment 3: Locale and Input in the foreground hold the boot (4/4);
    /// under `Run` they do not (4/4). ScreenMode is saved on a Workbench with
    /// no console (experiment 4), so it must not hold the boot.
    #[test]
    fn the_wizard_opens_locale_and_input_in_the_foreground_and_screenmode_detached() {
        let lines = command_lines(STEP_90_PREFS);
        at(&lines, "SYS:Prefs/Locale");
        at(&lines, "SYS:Prefs/Input");
        at(&lines, "Run <NIL: >NIL: SYS:Prefs/ScreenMode");
        assert!(!lines.contains(&"SYS:Prefs/ScreenMode"), "ScreenMode in the foreground");
        assert!(!lines
            .iter()
            .any(|l| l.starts_with("Run ") && (l.ends_with("/Locale") || l.ends_with("/Input"))));
    }

    /// The marker is asked before the editor, exactly as `plan`'s rows decide:
    /// a window ART set is not asked whether or not its editor is there.
    #[test]
    fn the_wizard_asks_the_hosts_marker_before_the_editor() {
        let lines = command_lines(STEP_90_PREFS);
        for (marker, window) in [(env::ART_SET_INPUT, "Input"), (env::ART_SET_SCREENMODE, "ScreenMode")] {
            let asked = at(&lines, &format!("IF EXISTS ENVARC:{marker}"));
            let not_asked = at(&lines, &format!("Echo >>S:FirstBoot.log \"step 90-prefs detail not-asked {window}\""));
            let editor = at(&lines, &format!("IF EXISTS SYS:Prefs/{window}"));
            let opened = at(&lines, &format!("Echo >>S:FirstBoot.log \"step 90-prefs detail opened {window}\""));
            assert!(asked < not_asked && not_asked < editor && editor < opened, "{window}");
        }
        assert!(!STEP_90_PREFS.contains("not-asked Locale"), "ART never sets Locale (spec §0)");
    }

    /// A machine switched off inside a window leaves the `opened` line as the
    /// step's last word, which is what names the window afterwards.
    #[test]
    fn the_wizard_logs_each_window_before_it_opens_it_and_names_a_missing_one() {
        let lines = command_lines(STEP_90_PREFS);
        for (window, run) in [
            ("Locale", "SYS:Prefs/Locale"),
            ("Input", "SYS:Prefs/Input"),
            ("ScreenMode", "Run <NIL: >NIL: SYS:Prefs/ScreenMode"),
        ] {
            let opened = at(&lines, &format!("Echo >>S:FirstBoot.log \"step 90-prefs detail opened {window}\""));
            assert!(opened < at(&lines, run), "{window}");
            at(&lines, &format!("Echo >>S:FirstBoot.log \"step 90-prefs detail missing {window}\""));
        }
    }

    #[test]
    fn the_wizard_banner_is_the_first_boot_flags_and_nothing_skips_a_window() {
        let lines = command_lines(STEP_90_PREFS);
        let gate = at(&lines, "IF ${ART_FirstBootBanner} EQ \"TRUE\"");
        let banner = lines.iter().position(|l| l.starts_with("Echo \"ART first boot:")).unwrap();
        assert!(gate < banner && banner < at(&lines, "IF EXISTS SYS:Prefs/Locale"));
        assert!(!lines.iter().any(|l| l.starts_with("Skip")));
    }
```

Change these four existing tests to iterate `every_script()` instead of `fixed_files()`: `every_fixed_file_is_ascii_with_lf_endings`, `no_script_ever_quits`, `every_variable_read_is_braced`, `every_fixed_file_hash_is_pinned` (in the last, `let expected: [&str; 8]` with a `"S/FirstBoot/90-prefs 0000000000000000000000000000000000000000000000000000000000000000"` line appended last, to be replaced in Step 4).

In `plan.rs`'s `mod tests`, after `a_tree_without_c_rename_is_refused_by_name`:

```rust
    /// Phase 3: the step wrapper waits with `C:Wait 3` before `C:Reboot`.
    #[test]
    fn a_tree_without_c_wait_is_refused_by_name() {
        let d = tree("nowait");
        fs::remove_file(d.join("C/Wait")).unwrap();
        let err = plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
        })
        .unwrap_err();
        match err {
            CoreError::FirstBootNeedsCommand { command } => assert_eq!(command, "Wait"),
            other => panic!("wrong refusal: {other:?}"),
        }
    }
```

- [ ] **Step 2: Run them and see them fail**

Run: `cd /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri && /c/Users/ismoz/.cargo/bin/cargo test --lib firstboot:: > /d/Projeler/Amiga/amiga-retro-toolkit/.superpowers/sdd/2026-09-15-firstboot-phase-3/t2.txt 2>&1; echo "exit $?"`
Expected: `exit 101` with `cannot find value `STEP_90_PREFS``, `cannot find value `WIZARD_FILE``, `cannot find function `every_script`` and `cannot find value `WIZARD_PATH``.

- [ ] **Step 3: Implement**

`mod.rs`, after `pub const FLAG_PATH …;`:

```rust
/// The wizard's step name — its file name in `S:FirstBoot/`, and the name
/// the report uses (phase 3 design §3.1).
pub const WIZARD_STEP: &str = "90-prefs";
pub const WIZARD_PATH: &str = "S/FirstBoot/90-prefs";
```

and in the module doc's `text` block, after the `S/FirstBoot/NN-name` line, add
`//! S/FirstBoot/90-prefs           the wizard; only when asked (phase 3)` and
`//! Prefs/Env-Archive/ART_Set_Input, ART_Set_ScreenMode   what ART set; 90-prefs skips it`.

`scripts/90-prefs` (Write tool, ASCII, LF, final newline):

```
; ART first boot: the Preferences ART did not set, asked on the Amiga.
; Locale always; Input unless ENVARC:ART_Set_Input exists (ART wrote a
; keymap); ScreenMode unless ENVARC:ART_Set_ScreenMode exists (ART wrote a
; screen depth). Locale and Input run in the foreground and the boot waits
; for each; ScreenMode runs detached and is saved once the Workbench is up
; (measured under WinUAE, 2026-09-15). Every command by full path; no Quit.
FailAt 2000000000
IF ${ART_FirstBootBanner} EQ "TRUE"
  Echo "ART first boot: ${ART_System} ${ART_RpiType}, Kickstart ${ART_Kick}"
ENDIF
IF EXISTS SYS:Prefs/Locale
  Echo >>S:FirstBoot.log "step 90-prefs detail opened Locale"
  Echo "Choose your language and country, then press Save."
  SYS:Prefs/Locale
ELSE
  Echo >>S:FirstBoot.log "step 90-prefs detail missing Locale"
ENDIF
IF EXISTS ENVARC:ART_Set_Input
  Echo >>S:FirstBoot.log "step 90-prefs detail not-asked Input"
ELSE
  IF EXISTS SYS:Prefs/Input
    Echo >>S:FirstBoot.log "step 90-prefs detail opened Input"
    Echo "Choose your keyboard, then press Save."
    SYS:Prefs/Input
  ELSE
    Echo >>S:FirstBoot.log "step 90-prefs detail missing Input"
  ENDIF
ENDIF
IF EXISTS ENVARC:ART_Set_ScreenMode
  Echo >>S:FirstBoot.log "step 90-prefs detail not-asked ScreenMode"
ELSE
  IF EXISTS SYS:Prefs/ScreenMode
    Echo >>S:FirstBoot.log "step 90-prefs detail opened ScreenMode"
    Echo "The screen mode window opens on the Workbench: choose a mode, then press Save."
    Run <NIL: >NIL: SYS:Prefs/ScreenMode
  ELSE
    Echo >>S:FirstBoot.log "step 90-prefs detail missing ScreenMode"
  ENDIF
ENDIF
```

`scripts/ART-FirstBoot-Step`: replace its last three lines (`IF ${ART_Reboot} EQ "TRUE"` / `  Echo >>S:FirstBoot.log "reboot requested by [name]"` / `ENDIF`) with (Write the whole file with the Write tool; the lines above them are unchanged):

```
; A step asks for a restart with SetEnv ART_Reboot "TRUE". The request is
; cleared first, because on 3.2 ENV: may be ENVARC: itself and a TRUE left
; behind would restart the next boot too. Wait 3 lets the filesystem finish
; its writes (measured 2026-09-15 on FFS: without it the volume needed
; validation 8 times in 8, with it 0 in 6). A release with no C:Reboot logs
; that it could not, and the run carries on to its end.
IF ${ART_Reboot} EQ "TRUE"
  SetEnv ART_Reboot "FALSE"
  Echo >>S:FirstBoot.log "reboot requested by [name]"
  IF EXISTS C:Reboot
    C:Wait 3
    C:Reboot
  ELSE
    Echo >>S:FirstBoot.log "reboot unavailable"
  ENDIF
ENDIF
```

`scripts.rs`, after `pub const SD0_PI4 …;`:

```rust
pub const STEP_90_PREFS: &str = include_str!("scripts/90-prefs");

/// The wizard (phase 3 design §3.2). Fixed text like every file here, but
/// written **only when asked** — which is why it is not in [`fixed_files`],
/// the list every write puts down.
pub const WIZARD_FILE: FixedFile = FixedFile {
    tree_path: super::WIZARD_PATH,
    text: STEP_90_PREFS,
};

/// Every script ART can put in a tree, the conditional wizard included —
/// what the text-level guards below scan, so a file written only sometimes
/// is held to the same rules as the ones written always.
pub fn every_script() -> [FixedFile; 8] {
    let [a, b, c, d, e, f, g] = fixed_files();
    [a, b, c, d, e, f, g, WIZARD_FILE]
}
```

`plan.rs`: change the constant to `pub const NEEDED_COMMANDS: [&str; 9] = ["List", "Sort", "Delete", "Copy", "Rename", "Version", "Mount", "Assign", "Wait"];` and append to its doc comment: `/// `Wait` joined in phase 3: the step wrapper runs `C:Wait 3` before `C:Reboot` (design §3.4). `Reboot` is not here — it is an availability, not a requirement.`

- [ ] **Step 4: Run, re-pin, see green**

Run the Step 2 command. Expected: every new test passes and exactly one fails, `every_fixed_file_hash_is_pinned`, whose `left` lists the eight got-lines: `S/ART-FirstBoot-Step` differs from the pinned value and `S/FirstBoot/90-prefs` differs from the zero placeholder; the other six are unchanged. Paste those two `left` values into `expected` (review the script diff first — that is the pin's review point), run again.
Expected: `test result: ok.` with 0 failed for `firstboot::`.

- [ ] **Step 5: Mutations — each seen red on its own guard (spec §8.1)**

Back up by absolute path: `ART-FirstBoot-Step`, `90-prefs`, `plan.rs` into the scratchpad. After each, run `cargo test --lib firstboot::`, confirm the named test is red (the hash pin will be red too — it is not the guard being proven), restore with `shutil.copyfile`, confirm green.

1. Delete the line `    C:Wait 3` → `the_wrapper_waits_before_it_reboots` red.
2. Replace the five lines `  IF EXISTS C:Reboot` … `  ENDIF` (inner) with `    C:Wait 3` / `    C:Reboot` → `the_wrapper_reboots_only_when_the_tree_has_the_command` red.
3. Move `  SetEnv ART_Reboot "FALSE"` to the line after `    C:Reboot` → `the_wrapper_clears_and_reports_the_request_before_it_reboots` red.
4. Replace `    Run <NIL: >NIL: SYS:Prefs/ScreenMode` with `    SYS:Prefs/ScreenMode` → `the_wizard_opens_locale_and_input_in_the_foreground_and_screenmode_detached` red.
5. Delete the three lines `IF EXISTS ENVARC:ART_Set_Input`, `  Echo >>S:FirstBoot.log "step 90-prefs detail not-asked Input"`, `ELSE` and the `ENDIF` that closes that outer block → `the_wizard_asks_the_hosts_marker_before_the_editor` red.
6. Remove `"Wait"` from `NEEDED_COMMANDS` (back to `[&str; 8]`) → `a_tree_without_c_wait_is_refused_by_name` red.
7. In `90-prefs`, change `${ART_Kick}` to `$ART_Kick` → `every_variable_read_is_braced` red (proves the scan now reaches the conditional file).

Run `python scripts/control-byte-sweep.py > .superpowers/sdd/2026-09-15-firstboot-phase-3/sweep-t2.txt 2>&1; echo "exit $?"` from the repository root. Expected: `exit 0`.

- [ ] **Step 6: Commit**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && git branch --show-current
git add src-tauri/src/core/firstboot/scripts/90-prefs src-tauri/src/core/firstboot/scripts/ART-FirstBoot-Step src-tauri/src/core/firstboot/mod.rs src-tauri/src/core/firstboot/scripts.rs src-tauri/src/core/firstboot/plan.rs
git commit -F .superpowers/sdd/2026-09-15-firstboot-phase-3/msg-t2.txt
```

`msg-t2.txt`: title `First boot phase 3: 90-prefs and the reboot carried out (task 2)`, a body listing the seven mutations and the test each turned red, and the `Co-Authored-By` line.

---

### Task 3: The plan — the tick, the reboot availability, the wizard rows

**Files:**
- Modify: `src-tauri/src/core/firstboot/mod.rs` (add `WizardWindow` after `WIZARD_PATH`; `use serde::Serialize;` at the top)
- Modify: `src-tauri/src/core/firstboot/plan.rs` (`FirstBootRequest`, new types after `FatMount`, `FirstBootPlan`, `plan`, two helpers, tests)
- Modify: `src-tauri/src/commands/firstboot.rs` (the two `FirstBootRequest` literals in `firstboot_preview`/`firstboot_write`, the wire test, the `#[ignore]`d test's request)
- Modify: `src-tauri/src/core/firstboot/write.rs` (its five test `FirstBootRequest` literals only)

**Interfaces:**
- Consumes: `startup::has_block` (Task 1); `osinstall::plan::KEYMAP_SELECTION: &str = "keymap-selection"` (`osinstall/plan.rs:295`); `env::ART_SET_SCREENMODE_PATH`, `env::MARKER_VALUE` (Task 1); `scripts::WIZARD_FILE`, `WIZARD_STEP`, `WIZARD_PATH` (Task 2).
- Produces (all in `core::firstboot::plan` unless noted):
  - `core::firstboot::WizardWindow { Locale, Input, ScreenMode }` — `#[serde(rename_all = "kebab-case")]`, `pub const ALL: [WizardWindow; 3]`, `pub fn amiga_name(self) -> &'static str` (`"Locale"`, `"Input"`, `"ScreenMode"`).
  - `FirstBootRequest { tree: PathBuf, ask_prefs: bool }`.
  - `pub const REBOOT_PACKAGE: &str = "reboot"`.
  - `RebootCommand { Available, Unavailable { needs: &'static str } }` — `#[serde(tag = "kind", rename_all = "kebab-case")]`.
  - `WindowState { Ask, SetByArt, Missing }` — kebab-case (`ask`, `set-by-art`, `missing`).
  - `WizardRow { window: WizardWindow, state: WindowState }`, `WizardPlan { rows: Vec<WizardRow> }` — camelCase.
  - `FirstBootPlan` gains `reboot: RebootCommand`, `wizard: Option<WizardPlan>`, `input_set_by_art: bool` (wire `reboot`, `wizard`, `inputSetByArt`).
  - Commands pass `ask_prefs: false` until Task 8 threads the tick.

- [ ] **Step 1: Update the existing literals, then write the failing tests**

Add `ask_prefs: false,` to every `FirstBootRequest { tree: … }` in `plan.rs` tests (`a_tree_without_c_sort_is_refused_by_name`, `a_tree_without_c_rename_is_refused_by_name`, `a_tree_without_c_wait_is_refused_by_name`, `a_plain_tree_plans_three_fixed_steps_and_no_fat_mount`, `fat95_in_l_makes_the_fat_mount_available`, `a_startup_sequence_that_never_calls_user_startup_is_refused_by_name`, `the_hook_check_is_case_insensitive_like_amigados`, `a_folder_without_distribution_json_is_not_a_tree`, `an_existing_dispatcher_marks_the_plan_as_already_written`), in `write.rs` tests (every `plan(&FirstBootRequest { … })`, seven literals across five tests), in `commands/firstboot.rs` (`firstboot_preview`, `firstboot_write`, and `plan(&FirstBootRequest { tree: copy.clone(), ask_prefs: false })` in `rehearse_the_real_tree_when_asked` — the wizard would hold that rehearsal until its deadline).

In `a_plain_tree_plans_three_fixed_steps_and_no_fat_mount`, append:

```rust
        assert!(p.wizard.is_none(), "not asked, no wizard");
        assert_eq!(p.reboot, RebootCommand::Unavailable { needs: "reboot" });
        assert!(!p.input_set_by_art);
```

Append to `plan.rs`'s `mod tests` (add `use crate::core::amigaprefs::env;` and `use crate::core::firstboot::WizardWindow;` beside the existing `use`s):

```rust
    fn asked(d: &ScratchDir) -> FirstBootPlan {
        plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
            ask_prefs: true,
        })
        .unwrap()
    }

    fn editors(d: &ScratchDir, names: &[&str]) {
        fs::create_dir_all(d.join("Prefs")).unwrap();
        for name in names {
            fs::write(d.join("Prefs").join(name), b"\x00\x00\x03\xf3").unwrap();
        }
    }

    fn rows(p: &FirstBootPlan) -> Vec<(WizardWindow, WindowState)> {
        p.wizard
            .as_ref()
            .expect("asked, so planned")
            .rows
            .iter()
            .map(|r| (r.window, r.state))
            .collect()
    }

    #[test]
    fn asking_plans_the_wizard_as_the_last_step_and_counts_its_bytes() {
        let d = tree("ask");
        editors(&d, &["Locale", "Input", "ScreenMode"]);
        let off = plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
            ask_prefs: false,
        })
        .unwrap();
        let on = asked(&d);
        let names: Vec<&str> = on.steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["10-hardware", "20-aux", "30-datatypes", "90-prefs"]);
        assert_eq!(on.steps[3].tree_path, "S/FirstBoot/90-prefs");
        assert_eq!(
            on.bytes_added - off.bytes_added,
            super::super::scripts::STEP_90_PREFS.len() as u64
        );
    }

    #[test]
    fn every_editor_present_and_nothing_set_asks_all_three() {
        let d = tree("all-asked");
        editors(&d, &["Locale", "Input", "ScreenMode"]);
        assert_eq!(
            rows(&asked(&d)),
            [
                (WizardWindow::Locale, WindowState::Ask),
                (WizardWindow::Input, WindowState::Ask),
                (WizardWindow::ScreenMode, WindowState::Ask),
            ]
        );
    }

    /// Design §4.3's own case: a tree ART built before phase 3 carries the
    /// block and no marker. The preview must say what the write is about to
    /// make true, not what the drawer holds this second.
    #[test]
    fn a_keymap_block_is_input_set_by_art_before_any_marker_exists() {
        let d = tree("keymap");
        editors(&d, &["Locale", "Input", "ScreenMode"]);
        fs::write(
            d.join("S/User-Startup"),
            b"; mine\n;BEGIN keymap-selection\nSetKeyboard usa\n;END keymap-selection\n",
        )
        .unwrap();
        let p = asked(&d);
        assert!(p.input_set_by_art);
        assert!(!d.join(env::ART_SET_INPUT_PATH).exists());
        assert_eq!(rows(&p)[1], (WizardWindow::Input, WindowState::SetByArt));

        // The marker's four bytes are counted only while the block is there.
        let with_block = p.bytes_added;
        fs::write(d.join("S/User-Startup"), b"; mine\n").unwrap();
        let without_block = asked(&d).bytes_added;
        assert_eq!(with_block - without_block, env::MARKER_VALUE.len() as u64);
    }

    #[test]
    fn a_stray_keymap_opener_is_not_a_keymap_art_set() {
        let d = tree("stray");
        editors(&d, &["Locale", "Input", "ScreenMode"]);
        fs::write(d.join("S/User-Startup"), b";BEGIN keymap-selection\nSetKeyboard usa\n").unwrap();
        let p = asked(&d);
        assert!(!p.input_set_by_art);
        assert_eq!(rows(&p)[1], (WizardWindow::Input, WindowState::Ask));
    }

    /// The script asks the marker before the editor; so does the plan.
    #[test]
    fn a_set_window_is_set_whether_or_not_its_editor_exists_and_a_missing_one_is_missing() {
        let d = tree("mixed");
        editors(&d, &["Locale"]);
        fs::create_dir_all(d.join("Prefs/Env-Archive")).unwrap();
        fs::write(d.join(env::ART_SET_SCREENMODE_PATH), b"TRUE").unwrap();
        assert_eq!(
            rows(&asked(&d)),
            [
                (WizardWindow::Locale, WindowState::Ask),
                (WizardWindow::Input, WindowState::Missing),
                (WizardWindow::ScreenMode, WindowState::SetByArt),
            ]
        );
    }

    #[test]
    fn c_reboot_in_the_tree_makes_the_reboot_available() {
        let d = tree("reboot");
        fs::write(d.join("C/Reboot"), b"\x00\x00\x03\xf3").unwrap();
        assert_eq!(asked(&d).reboot, RebootCommand::Available);
    }

    /// The package the preview names is the catalogue's own id, read from the
    /// file rather than trusted from a copy (CLAUDE.md, "a test that reads a
    /// table instead of the file is a copy").
    #[test]
    fn the_reboot_package_is_the_catalogues_aminet_reboot() {
        let json: serde_json::Value =
            serde_json::from_str(include_str!("../sources/bundle/catalogue/acilis.json")).unwrap();
        let entry = json["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == REBOOT_PACKAGE)
            .expect("the catalogue carries the reboot package");
        assert_eq!(entry["source"]["aminet"]["path"], "util/boot/reboot");
    }
```

In `commands/firstboot.rs`'s `the_plan_crosses_the_wire_in_camel_case_with_kebab_tags`, extend the import to `use crate::core::firstboot::plan::{FatMount, PlannedStep, RebootCommand, WindowState, WizardPlan, WizardRow};` plus `use crate::core::firstboot::WizardWindow;`, add to the literal:

```rust
            reboot: RebootCommand::Unavailable { needs: "reboot" },
            wizard: Some(WizardPlan {
                rows: vec![WizardRow {
                    window: WizardWindow::ScreenMode,
                    state: WindowState::SetByArt,
                }],
            }),
            input_set_by_art: true,
```

and the assertions:

```rust
        assert_eq!(json["reboot"]["kind"], "unavailable");
        assert_eq!(json["reboot"]["needs"], "reboot");
        assert_eq!(json["wizard"]["rows"][0]["window"], "screen-mode");
        assert_eq!(json["wizard"]["rows"][0]["state"], "set-by-art");
        assert_eq!(json["inputSetByArt"], true);
```

- [ ] **Step 2: Run and see it fail**

Run: `cd /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri && /c/Users/ismoz/.cargo/bin/cargo test --lib firstboot:: > …/t3.txt 2>&1; echo "exit $?"` (the `…` is `/d/Projeler/Amiga/amiga-retro-toolkit/.superpowers/sdd/2026-09-15-firstboot-phase-3` throughout this plan).
Expected: `exit 101` — `struct `FirstBootRequest` has no field named `ask_prefs``, `cannot find type `RebootCommand``, `cannot find type `WizardWindow``.

- [ ] **Step 3: Implement**

`mod.rs` (add `use serde::Serialize;` under the module doc):

```rust
/// A Preferences window the wizard may open, in the order `90-prefs` asks
/// them (phase 3 design §3.2). Wire: `locale`, `input`, `screen-mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WizardWindow {
    Locale,
    Input,
    ScreenMode,
}

impl WizardWindow {
    pub const ALL: [WizardWindow; 3] = [Self::Locale, Self::Input, Self::ScreenMode];

    /// The editor's own name in `SYS:Prefs/` — also the word `90-prefs`
    /// writes into its `detail` lines.
    pub fn amiga_name(self) -> &'static str {
        match self {
            Self::Locale => "Locale",
            Self::Input => "Input",
            Self::ScreenMode => "ScreenMode",
        }
    }
}
```

`plan.rs` — imports become:

```rust
use super::scripts::{fixed_files, WIZARD_FILE};
use super::{WizardWindow, WIZARD_PATH, WIZARD_STEP};
use crate::core::amigaprefs::env;
use crate::core::error::{CoreError, CoreResult};
use crate::core::osinstall::plan::KEYMAP_SELECTION;
use crate::core::osinstall::startup::has_block;
```

`FirstBootRequest`:

```rust
#[derive(Debug, Clone)]
pub struct FirstBootRequest {
    pub tree: PathBuf,
    /// Write `S:FirstBoot/90-prefs`, the wizard (phase 3 design §2 decision 5).
    pub ask_prefs: bool,
}
```

After `FatMount`:

```rust
/// The catalogue id of Aminet `util/boot/reboot`
/// (`core/sources/bundle/catalogue/acilis.json`).
pub const REBOOT_PACKAGE: &str = "reboot";

/// Whether the step wrapper can carry out a reboot request. Not a refusal —
/// the `FatMount` rule: first boot is still written, the Amiga logs `reboot
/// unavailable` and carries on, and the preview names the package (design §2
/// decision 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RebootCommand {
    Available,
    Unavailable { needs: &'static str },
}

/// What `90-prefs` will do with one window, read from the tree at preview
/// time. The Amiga decides on the day; Appearance applied after this preview
/// can still turn an `Ask` into a skipped window (design §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WindowState {
    Ask,
    SetByArt,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WizardRow {
    pub window: WizardWindow,
    pub state: WindowState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WizardPlan {
    /// Locale, Input, ScreenMode — the order the script asks them.
    pub rows: Vec<WizardRow>,
}
```

`FirstBootPlan` — after `fat_mount`:

```rust
    pub reboot: RebootCommand,
    /// `None` when the wizard was not asked for.
    pub wizard: Option<WizardPlan>,
    /// `S/User-Startup` carries a well-formed `keymap-selection` block, so
    /// the write puts `ART_Set_Input` down (design §4.3) — decided here, from
    /// the block, so the preview's Input row and the marker cannot disagree.
    pub input_set_by_art: bool,
```

In `plan`, after the `fat_mount` binding:

```rust
    let reboot = if tree.join("C").join("Reboot").is_file() {
        RebootCommand::Available
    } else {
        RebootCommand::Unavailable {
            needs: REBOOT_PACKAGE,
        }
    };
    let input_set_by_art = keymap_block_present(tree)?;
    let wizard = if request.ask_prefs {
        Some(wizard_rows(tree, input_set_by_art))
    } else {
        None
    };
```

After the fixed-files loop and before `bytes_added += b"TRUE\n".len() as u64;`:

```rust
    if wizard.is_some() {
        bytes_added += WIZARD_FILE.text.len() as u64;
        steps.push(PlannedStep {
            name: WIZARD_STEP.to_string(),
            tree_path: WIZARD_PATH.to_string(),
            fixed: true,
        });
    }
```

After the flag's `bytes_added` line:

```rust
    if input_set_by_art {
        bytes_added += env::MARKER_VALUE.len() as u64;
    }
```

and the three new fields in the returned `FirstBootPlan { … reboot, wizard, input_set_by_art, … }`.

After `calls_user_startup`:

```rust
/// Whether `S/User-Startup` carries a well-formed `keymap-selection` block —
/// through `core::osinstall::startup`'s own pairing, never a substring search,
/// so a stray `;BEGIN` is no keymap (design §4.3). Read Latin-1, the way
/// `write` reads the same file.
fn keymap_block_present(tree: &Path) -> CoreResult<bool> {
    let bytes = match std::fs::read(tree.join("S").join("User-Startup")) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err.into()),
    };
    let text: String = bytes.iter().map(|&b| b as char).collect();
    Ok(has_block(&text, KEYMAP_SELECTION))
}

/// The rows `90-prefs` will act on, decided in the script's own order: a
/// window ART set is not asked whether or not its editor exists; otherwise an
/// editor that is there is asked, and one that is not is missing.
fn wizard_rows(tree: &Path, input_set_by_art: bool) -> WizardPlan {
    let rows = WizardWindow::ALL
        .iter()
        .map(|&window| {
            let set_by_art = match window {
                WizardWindow::Locale => false,
                WizardWindow::Input => input_set_by_art,
                WizardWindow::ScreenMode => tree.join(env::ART_SET_SCREENMODE_PATH).is_file(),
            };
            let state = if set_by_art {
                WindowState::SetByArt
            } else if tree.join("Prefs").join(window.amiga_name()).is_file() {
                WindowState::Ask
            } else {
                WindowState::Missing
            };
            WizardRow { window, state }
        })
        .collect();
    WizardPlan { rows }
}
```

`commands/firstboot.rs` — in both commands' requests add `ask_prefs: false,` with the comment `// Task 8 of the phase-3 plan threads the tick; until then no wizard is written.`

- [ ] **Step 4: Run and see it pass**

Run: `cargo fmt` (from `src-tauri`), then the Step 2 command.
Expected: `test result: ok.` for `firstboot::` with 0 failed; then `… cargo test --lib commands::firstboot:: > …/t3b.txt 2>&1` — `test result: ok.` (the `#[ignore]`d test counted as ignored).

- [ ] **Step 5: Mutations**

Back up `plan.rs` by absolute path. After each, run `cargo test --lib firstboot::plan::`, see the named test red, restore with `shutil.copyfile`, see green.

1. In `wizard_rows`, test the editor before `set_by_art` (swap the first two branches) → `a_set_window_is_set_whether_or_not_its_editor_exists_and_a_missing_one_is_missing` red.
2. In `keymap_block_present`, return `Ok(text.contains(";BEGIN keymap-selection"))` → `a_stray_keymap_opener_is_not_a_keymap_art_set` red.
3. Make `reboot` always `RebootCommand::Available` → `a_plain_tree_plans_three_fixed_steps_and_no_fat_mount` red.
4. Delete `bytes_added += WIZARD_FILE.text.len() as u64;` → `asking_plans_the_wizard_as_the_last_step_and_counts_its_bytes` red.
5. Change `REBOOT_PACKAGE` to `"reboot101"` → `the_reboot_package_is_the_catalogues_aminet_reboot` red.

- [ ] **Step 6: Commit**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && git branch --show-current
git add src-tauri/src/core/firstboot/mod.rs src-tauri/src/core/firstboot/plan.rs src-tauri/src/core/firstboot/write.rs src-tauri/src/commands/firstboot.rs
git commit -F .superpowers/sdd/2026-09-15-firstboot-phase-3/msg-t3.txt
```

Message: `First boot phase 3: the plan asks, names the reboot, rows the wizard (task 3)`, the five mutations, the note that commands pass `ask_prefs: false` until task 8, and the `Co-Authored-By` line.

---

### Task 4: The write — the wizard written or removed, `ART_Set_Input` written or removed

**Files:**
- Modify: `src-tauri/src/core/firstboot/write.rs` (imports, `Written`, `write`, tests)
- Modify: `src-tauri/src/commands/firstboot.rs` (`firstboot_write`'s oplog closure)

**Interfaces:**
- Consumes: `FirstBootPlan.wizard`, `FirstBootPlan.input_set_by_art` (Task 3); `scripts::WIZARD_FILE` (Task 2); `env::ART_SET_INPUT_PATH`, `env::MARKER_VALUE`, `env::encode_value` (Task 1); `core::safety::guarded_remove(&Path, BackupPolicy) -> CoreResult<Option<PathBuf>>`.
- Produces: `Written { files: Vec<String>, removed: Vec<String>, user_startup_backup: Option<PathBuf>, user_startup_created: bool }` (wire `removed`).

**The removal rule** (deviation 7): ART's own generated file inside a tree ART writes is removed with `guarded_remove` — `core/safety`'s gate, the `core/osinstall/apply.rs:829`/`:1710` precedent — with `BackupPolicy::NONE`, because `90-prefs` is compiled-in text re-ticking writes back byte for byte and `write` already replaces fixed files with no backup; the marker is re-derived on every write. The Recycle Bin (`core/hostfs.rs`) is for a user's own files chosen on the host.

- [ ] **Step 1: Write the failing tests**

Append to `write.rs`'s `mod tests` (add `use crate::core::amigaprefs::env;` and `use crate::core::firstboot::plan::FirstBootPlan;`):

```rust
    fn planned(d: &ScratchDir, ask_prefs: bool) -> FirstBootPlan {
        plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
            ask_prefs,
        })
        .unwrap()
    }

    #[test]
    fn asking_writes_the_wizard_exactly_and_before_the_block() {
        let d = tree("wizard");
        let w = super::write(&planned(&d, true)).unwrap();
        assert_eq!(
            fs::read_to_string(d.join("S/FirstBoot/90-prefs")).unwrap(),
            crate::core::firstboot::scripts::STEP_90_PREFS
        );
        let wizard = w.files.iter().position(|f| f == "S/FirstBoot/90-prefs").unwrap();
        let block = w.files.iter().position(|f| f == "S/User-Startup").unwrap();
        assert!(wizard < block, "the block is the last thing written");
        assert!(w.removed.is_empty());
    }

    /// Design §4.4: unticking and building again must not leave the wizard.
    #[test]
    fn unticking_and_writing_again_removes_arts_own_wizard_and_nothing_else() {
        let d = tree("untick");
        super::write(&planned(&d, true)).unwrap();
        let w = super::write(&planned(&d, false)).unwrap();
        assert!(!d.join("S/FirstBoot/90-prefs").exists());
        assert_eq!(w.removed, vec!["S/FirstBoot/90-prefs".to_string()]);
        assert!(d.join("S/FirstBoot/10-hardware").is_file());
        assert!(
            !d.join("S/FirstBoot/.art-backup").exists(),
            "no backup drawer inside the step directory the card carries"
        );
    }

    #[test]
    fn a_tree_that_never_had_a_wizard_reports_no_removal() {
        let d = tree("never");
        assert!(super::write(&planned(&d, false)).unwrap().removed.is_empty());
    }

    #[test]
    fn a_keymap_block_writes_the_input_marker() {
        let d = tree("marker");
        fs::write(
            d.join("S/User-Startup"),
            b";BEGIN keymap-selection\nSetKeyboard usa\n;END keymap-selection\n",
        )
        .unwrap();
        let w = super::write(&planned(&d, false)).unwrap();
        assert_eq!(fs::read(d.join(env::ART_SET_INPUT_PATH)).unwrap(), b"TRUE");
        assert!(w.files.contains(&env::ART_SET_INPUT_PATH.to_string()));
    }

    #[test]
    fn a_stray_keymap_opener_writes_no_input_marker() {
        let d = tree("stray");
        fs::write(d.join("S/User-Startup"), b";BEGIN keymap-selection\nSetKeyboard usa\n").unwrap();
        super::write(&planned(&d, false)).unwrap();
        assert!(!d.join(env::ART_SET_INPUT_PATH).exists());
    }

    #[test]
    fn a_tree_whose_keymap_block_is_gone_loses_the_marker() {
        let d = tree("lost");
        fs::create_dir_all(d.join("Prefs/Env-Archive")).unwrap();
        fs::write(d.join(env::ART_SET_INPUT_PATH), b"TRUE").unwrap();
        let w = super::write(&planned(&d, false)).unwrap();
        assert!(!d.join(env::ART_SET_INPUT_PATH).exists());
        assert_eq!(w.removed, vec![env::ART_SET_INPUT_PATH.to_string()]);
    }

    #[test]
    fn writing_twice_with_the_wizard_and_a_keymap_is_byte_identical() {
        let d = tree("twice-wizard");
        fs::write(
            d.join("S/User-Startup"),
            b";BEGIN keymap-selection\nSetKeyboard usa\n;END keymap-selection\n",
        )
        .unwrap();
        super::write(&planned(&d, true)).unwrap();
        let first = snapshot(d.path());
        super::write(&planned(&d, true)).unwrap();
        let second = snapshot(d.path());
        let strip = |m: std::collections::BTreeMap<String, Vec<u8>>| {
            m.into_iter()
                .filter(|(k, _)| !k.contains("User-Startup."))
                .collect::<Vec<_>>()
        };
        assert_eq!(strip(first), strip(second));
    }

    #[test]
    fn a_removal_crosses_the_wire_under_its_own_name() {
        let w = super::Written {
            files: vec![],
            removed: vec!["S/FirstBoot/90-prefs".into()],
            user_startup_backup: None,
            user_startup_created: false,
        };
        assert_eq!(serde_json::to_value(&w).unwrap()["removed"][0], "S/FirstBoot/90-prefs");
    }
```

- [ ] **Step 2: Run and see it fail**

Run: `… cargo test --lib firstboot::write:: > …/t4.txt 2>&1; echo "exit $?"`
Expected: `exit 101`, `struct `Written` has no field named `removed``.

- [ ] **Step 3: Implement**

Imports in `write.rs`:

```rust
use super::scripts::{fixed_files, WIZARD_FILE};
use super::{user_startup_lines, COMPONENT, FLAG_PATH};
use crate::core::amigaprefs::env;
use crate::core::error::CoreResult;
use crate::core::osinstall::startup::merge_user_startup;
use crate::core::safety::{atomic_write, guarded_remove, guarded_write, BackupPolicy};
```

`Written` gains, after `files`:

```rust
    /// Tree-relative paths ART removed, in order: its own `90-prefs` when the
    /// wizard was not asked, `ART_Set_Input` when the keymap block is gone.
    /// Only what was there — a removal nobody is told about is a thing ART
    /// did and did not say.
    pub removed: Vec<String>,
```

In `write`, `let mut removed = Vec::new();` beside `files`; after the fixed-files loop:

```rust
    // The wizard, or its absence (design §4.4). `S/FirstBoot/` is ART's own
    // drawer and `90-prefs` is compiled-in text re-ticking writes back byte
    // for byte, and the loop above replaces every fixed file with no backup —
    // so the removal goes through `core/safety` with the same policy. Not the
    // Recycle Bin: `core/hostfs.rs` is for a user's own files chosen on the
    // host; `core/osinstall/apply.rs`'s sidecar removal is this precedent.
    let wizard = tree.join(WIZARD_FILE.tree_path);
    if plan.wizard.is_some() {
        atomic_write(&wizard, WIZARD_FILE.text.as_bytes())?;
        files.push(WIZARD_FILE.tree_path.to_string());
    } else if wizard.is_file() {
        guarded_remove(&wizard, BackupPolicy::NONE)?;
        removed.push(WIZARD_FILE.tree_path.to_string());
    }
```

after the flag's `files.push(FLAG_PATH.to_string());`:

```rust
    // `ART_Set_Input` (design §4.3), re-derived on every write: the keymap
    // line is written only by the build's tree phase, and the first-boot
    // phase always follows it. `plan` read the block with the merge's own
    // pairing; the marker says what that read found.
    let marker = tree.join(env::ART_SET_INPUT_PATH);
    if plan.input_set_by_art {
        atomic_write(&marker, &env::encode_value(env::MARKER_VALUE)?)?;
        files.push(env::ART_SET_INPUT_PATH.to_string());
    } else if marker.is_file() {
        guarded_remove(&marker, BackupPolicy::NONE)?;
        removed.push(env::ART_SET_INPUT_PATH.to_string());
    }
```

and `removed,` in the returned `Written`. Append to the module doc comment: `//! The wizard and the keymap marker are written or removed before the block too; see `write`.`

`commands/firstboot.rs`, inside `firstboot_write`'s closure, after the `Files written` detail:

```rust
            let record = if done.removed.is_empty() {
                record
            } else {
                record.detail("Files removed", done.removed.join(", "))
            };
```

- [ ] **Step 4: Run and see it pass**

Run `cargo fmt`, then `… cargo test --lib firstboot:: > …/t4.txt 2>&1`.
Expected: `test result: ok.`, 0 failed; `writes_every_fixed_file_the_flag_and_the_block` still sees `w.files.len() == 9`.

- [ ] **Step 5: Mutations**

Back up `write.rs`. Each: run `cargo test --lib firstboot::write::`, named test red, restore with `shutil.copyfile`, green.

1. Delete the `else if wizard.is_file() { … }` arm → `unticking_and_writing_again_removes_arts_own_wizard_and_nothing_else` red.
2. Use `BackupPolicy::CONFIG` in the wizard removal → the same test red on its `.art-backup` assertion.
3. Replace `if plan.input_set_by_art` with `if plan.user_startup_exists` → `a_stray_keymap_opener_writes_no_input_marker` red.
4. Delete the marker's `else if marker.is_file()` arm → `a_tree_whose_keymap_block_is_gone_loses_the_marker` red.
5. Move the wizard block below the `guarded_write` of `S/User-Startup` → `asking_writes_the_wizard_exactly_and_before_the_block` red.

- [ ] **Step 6: Commit**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && git branch --show-current
git add src-tauri/src/core/firstboot/write.rs src-tauri/src/commands/firstboot.rs
git commit -F .superpowers/sdd/2026-09-15-firstboot-phase-3/msg-t4.txt
```

Message: `First boot phase 3: write or remove the wizard and the keymap marker (task 4)`, the removal rule in two sentences, the five mutations, `Co-Authored-By`.

---

### Task 5: Appearance writes `ART_Set_ScreenMode` with the depth

**Files:**
- Modify: `src-tauri/src/core/appearance/mod.rs` (`struct ScreenModePlan`, `fn plan_screen_mode`, the `total` sum and the `if let Some(plan) = screen_plan` commit block in `apply_appearance_with`, tests)
- Modify: `src-tauri/src/commands/appearance.rs` (the test that applies `screen_depth: Some(8)` and asserts `outcome.written.len()`)

**Interfaces:**
- Consumes: `env::ART_SET_SCREENMODE`, `env::MARKER_VALUE`, `env::encode_value` (Task 1); `crate::core::osinstall::find_child_ci(&Path, &str) -> CoreResult<Option<PathBuf>>`; the module's private `resolve_ci`, `ENV_ARCHIVE_REL`, `commit_write`; test-only `crate::core::amigainstall::run::fakes::CancelAfter::new(after: u32)`.
- Produces: with a depth, `AppearanceOutcome.written` ends `[…, <tree>/Prefs/Env-Archive/Sys/ScreenMode.prefs, <tree>/Prefs/Env-Archive/ART_Set_ScreenMode]`; without one, no marker. The marker is never removed (design §4.2).

- [ ] **Step 1: Write the failing tests**

Append to `appearance/mod.rs`'s `mod tests`:

```rust
    fn screenmode_marker_path(tree: &Path) -> PathBuf {
        tree.join("Prefs").join("Env-Archive").join(env::ART_SET_SCREENMODE)
    }

    fn depth_only(depth: Option<u16>) -> AppearanceRequest {
        AppearanceRequest {
            wallpaper: None,
            screen_depth: depth,
            shell_defaults: depth.is_none(),
            arrange_icons: false,
        }
    }

    /// First-boot phase 3 design §4.2: the depth and the marker that tells
    /// `90-prefs` not to ask ScreenMode land in one committed list.
    #[test]
    fn a_screen_depth_writes_the_screenmode_marker_in_the_same_outcome() {
        let (_scratch, tree) = build_tree("screen-marker");
        let outcome = apply_appearance(&tree, &depth_only(Some(8))).unwrap();
        assert_eq!(
            outcome.written,
            vec![screenmode_path(&tree), screenmode_marker_path(&tree)]
        );
        assert_eq!(std::fs::read(screenmode_marker_path(&tree)).unwrap(), b"TRUE");
    }

    #[test]
    fn no_screen_depth_writes_no_screenmode_marker() {
        let (_scratch, tree) = build_tree("no-screen-marker");
        let outcome = apply_appearance(&tree, &depth_only(None)).unwrap();
        assert!(!screenmode_marker_path(&tree).exists());
        assert!(!outcome
            .written
            .iter()
            .any(|p| p.ends_with(env::ART_SET_SCREENMODE)));
    }

    #[test]
    fn a_refused_screen_depth_writes_no_marker() {
        let (_scratch, tree) = build_tree("refused-screen-marker");
        std::fs::remove_file(screenmode_path(&tree)).unwrap();
        assert!(apply_appearance(&tree, &depth_only(Some(8))).is_err());
        assert!(!screenmode_marker_path(&tree).exists());
    }

    /// The marker belongs to the depth: a stop between the two would leave a
    /// depth ART wrote and a wizard that asks for it anyway. No cancellation
    /// check sits between them.
    #[test]
    fn a_stop_is_not_offered_between_the_depth_and_its_marker() {
        let (_scratch, tree) = build_tree("screen-marker-stop");
        let sink = crate::core::amigainstall::run::fakes::CancelAfter::new(1);
        apply_appearance_with(&tree, &depth_only(Some(8)), &sink)
            .expect("the one check before the depth passes; nothing else asks");
        assert!(screenmode_marker_path(&tree).is_file());
    }
```

In `commands/appearance.rs`, in the test with `screen_depth: Some(8)`, replace `assert_eq!(outcome.written.len(), 1);` with:

```rust
        // The depth and ART_Set_ScreenMode (first-boot phase 3 design §4.2).
        assert_eq!(outcome.written.len(), 2);
        assert!(outcome.written[1].ends_with("ART_Set_ScreenMode"));
```

(the `backups.len() == 1` assertion stays — the marker is new, so nothing is backed up for it).

- [ ] **Step 2: Run and see it fail**

Run: `… cargo test --lib appearance:: > …/t5.txt 2>&1; echo "exit $?"`
Expected: `exit 101` with failures `a_screen_depth_writes_the_screenmode_marker_in_the_same_outcome` (left one path, right two) and the command test (`left: 1, right: 2`); the other three new tests pass already — they are the controls.

- [ ] **Step 3: Implement**

`ScreenModePlan`:

```rust
/// A fully validated screen-depth change, and the first-boot marker that
/// travels with it (first-boot phase 3 design §4.2).
struct ScreenModePlan {
    path: PathBuf,
    bytes: Vec<u8>,
    marker: PathBuf,
    marker_bytes: Vec<u8>,
}
```

End of `plan_screen_mode`, replacing `Ok(ScreenModePlan { path, bytes })`:

```rust
    // `ART_Set_ScreenMode`: `S:FirstBoot/90-prefs` skips the ScreenMode window
    // when it exists. Planned here so a tree with no `Prefs/Env-Archive`
    // refuses before anything is written, like every other part of a request.
    let archive = resolve_ci(tree, ENV_ARCHIVE_REL)?;
    let marker = match crate::core::osinstall::find_child_ci(&archive, env::ART_SET_SCREENMODE)? {
        Some(existing) => existing,
        None => archive.join(env::ART_SET_SCREENMODE),
    };
    let marker_bytes = env::encode_value(env::MARKER_VALUE)?;
    Ok(ScreenModePlan {
        path,
        bytes,
        marker,
        marker_bytes,
    })
```

In `apply_appearance_with`, `+ screen_plan.as_ref().map_or(0, |_| 1)` becomes `+ screen_plan.as_ref().map_or(0, |_| 2)`, and the screen commit block becomes:

```rust
    if let Some(plan) = screen_plan {
        check_cancelled(sink, &committed)?;
        let message = plan.path.display().to_string();
        commit_write(plan.path, &plan.bytes, BackupPolicy::CONFIG, &mut committed)?;
        done += 1;
        sink.report(done, Some(total), &message);

        // The marker, with no cancellation check before it: a depth written
        // without its marker would have the Amiga ask a question ART already
        // answered. Never removed afterwards — a depth ART wrote stays written.
        let message = plan.marker.display().to_string();
        commit_write(plan.marker, &plan.marker_bytes, BackupPolicy::CONFIG, &mut committed)?;
        done += 1;
        sink.report(done, Some(total), &message);
    }
```

- [ ] **Step 4: Run and see it pass**

`cargo fmt`; `… cargo test --lib appearance:: > …/t5.txt 2>&1` and `… cargo test --lib commands::appearance:: > …/t5b.txt 2>&1`.
Expected: both `test result: ok.`, 0 failed (`a_write_that_fails_after_an_earlier_one_succeeded_names_what_was_already_written` still fails on the read-only `ScreenMode.prefs`, before the marker).

- [ ] **Step 5: Mutations**

Back up `appearance/mod.rs` by absolute path; after each, run `cargo test --lib appearance::`, named test red, restore with `shutil.copyfile`, green.

1. Delete the marker's `commit_write` (and its two lines) → `a_screen_depth_writes_the_screenmode_marker_in_the_same_outcome` red.
2. Add `check_cancelled(sink, &committed)?;` before the marker's `commit_write` → `a_stop_is_not_offered_between_the_depth_and_its_marker` red (`CancelledPartway { files: 1 }`).
3. Write the marker for every request: in the `for plan in shell_plans` loop's first iteration add `commit_write(resolve_ci(tree, ENV_ARCHIVE_REL)?.join(env::ART_SET_SCREENMODE), b"TRUE", BackupPolicy::CONFIG, &mut committed)?;` → `no_screen_depth_writes_no_screenmode_marker` red.

- [ ] **Step 6: Commit**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && git branch --show-current
git add src-tauri/src/core/appearance/mod.rs src-tauri/src/commands/appearance.rs
git commit -F .superpowers/sdd/2026-09-15-firstboot-phase-3/msg-t5.txt
```

Message: `First boot phase 3: a screen depth writes ART_Set_ScreenMode (task 5)`, the three mutations, the note that the appearance command's written count is 2 for a depth, `Co-Authored-By`.

---

### Task 6: The report reads a restart and a waiting window

**Files:**
- Modify: `src-tauri/src/core/firstboot/mod.rs` (add `ForegroundWindow` after `WizardWindow`)
- Modify: `src-tauri/src/core/firstboot/report.rs` (struct, `parse`, a helper, tests)
- Modify: `src-tauri/src/core/amigainstall/rehearse.rs` (the `FirstBootReport` literal in `a_runaway_rehearsal_reaches_the_wire_with_its_measurement`)
- Modify: `src-tauri/src/commands/firstboot.rs` (the two `FirstBootReport` literals in its tests)

**Interfaces:**
- Consumes: `WIZARD_STEP` (Task 2); the script's detail words `opened Locale` / `opened Input` (Task 2).
- Produces: `core::firstboot::ForegroundWindow { Locale, Input }` (kebab-case; `pub fn amiga_name(self) -> &'static str`); `FirstBootReport` gains `reboot_unavailable: bool`, `restarted_after_request: bool`, `waiting_in: Option<ForegroundWindow>` (wire `rebootUnavailable`, `restartedAfterRequest`, `waitingIn`).

- [ ] **Step 1: Write the failing tests**

Add to the three Rust `FirstBootReport { … }` literals named above, after `reboot_requested_by: None,`:

```rust
                reboot_unavailable: false,
                restarted_after_request: false,
                waiting_in: None,
```

Append to `report.rs`'s `mod tests` (add `use crate::core::firstboot::ForegroundWindow;`):

```rust
    /// Design §6: today this line would fall into `unknown`.
    #[test]
    fn reboot_unavailable_is_its_own_flag_and_the_run_carries_on() {
        let r = parse("art-firstboot 1\nsystem UAE none kick 3.9\nstep 15-probe started\nstep 15-probe ok\nreboot requested by 15-probe\nreboot unavailable\nstep 20-aux started\nstep 20-aux skipped uae\nstep 20-aux ok\ndone all\n");
        assert!(r.reboot_unavailable);
        assert!(!r.restarted_after_request);
        assert_eq!(r.reboot_requested_by.as_deref(), Some("15-probe"));
        assert!(r.unknown.is_empty(), "{:?}", r.unknown);
        assert_eq!(r.ending, Ending::DoneAll);
    }

    /// Design §6: a log spanning boots is read with the latest step of a name
    /// and the latest `done` — a test, not a change.
    #[test]
    fn a_log_spanning_three_boots_reads_the_latest_step_and_the_latest_done() {
        let r = parse("art-firstboot 1\nsystem UAE none kick 3.2\nstep 10-hardware started\nstep 10-hardware ok\nstep 15-probe started\nstep 15-probe ok\nreboot requested by 15-probe\nsystem UAE none kick 3.2\nstep 20-aux started\nstep 20-aux refused rc=20\ndone partial\nsystem UAE none kick 3.2\nstep 20-aux started\nstep 20-aux skipped uae\nstep 20-aux ok\ndone all\n");
        let names: Vec<&str> = r.steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["10-hardware", "15-probe", "20-aux", "20-aux"]);
        assert_eq!(r.steps[2].outcome, StepOutcome::Refused { rc: 20 });
        assert_eq!(r.steps[3].outcome, StepOutcome::Skipped { reason: "uae".into() });
        assert_eq!(r.ending, Ending::DoneAll);
        assert!(r.restarted_after_request);
        assert!(!r.reboot_unavailable);
    }

    /// Asked for, and the log stops: neither restarted nor unavailable — a
    /// third thing, which the screen says as a third sentence.
    #[test]
    fn a_request_the_log_does_not_show_coming_back_is_neither_restarted_nor_unavailable() {
        let r = parse("art-firstboot 1\nsystem UAE none kick 3.2\nstep 15-probe started\nstep 15-probe ok\nreboot requested by 15-probe\n");
        assert_eq!(r.reboot_requested_by.as_deref(), Some("15-probe"));
        assert!(!r.restarted_after_request);
        assert!(!r.reboot_unavailable);
        assert_eq!(r.ending, Ending::Unfinished);
    }

    #[test]
    fn a_later_request_replaces_what_an_earlier_one_said() {
        let r = parse("art-firstboot 1\nsystem UAE none kick 3.9\nreboot requested by a\nreboot unavailable\nsystem UAE none kick 3.9\nreboot requested by b\nsystem UAE none kick 3.9\n");
        assert_eq!(r.reboot_requested_by.as_deref(), Some("b"));
        assert!(!r.reboot_unavailable);
        assert!(r.restarted_after_request);
    }

    #[test]
    fn waiting_in_names_only_a_foreground_window_that_is_the_wizards_last_word() {
        let head = "art-firstboot 1\nsystem UAE none kick 3.2\nstep 90-prefs started\n";
        let at = |tail: &str| parse(&format!("{head}{tail}")).waiting_in;
        assert_eq!(at("step 90-prefs detail opened Locale\n"), Some(ForegroundWindow::Locale));
        assert_eq!(
            at("step 90-prefs detail opened Locale\nstep 90-prefs detail opened Input\n"),
            Some(ForegroundWindow::Input)
        );
        assert_eq!(
            at("step 90-prefs detail opened Locale\nstep 90-prefs detail not-asked Input\nstep 90-prefs detail opened ScreenMode\n"),
            None,
            "ScreenMode runs detached and holds nothing"
        );
        assert_eq!(at("step 90-prefs detail opened Locale\nstep 90-prefs ok\n"), None);
        assert_eq!(parse("art-firstboot 1\nstep 10-hardware started\n").waiting_in, None);
    }
```

In `the_wire_shape_is_what_the_frontend_reads`, append:

```rust
        assert_eq!(json["rebootUnavailable"], false);
        assert_eq!(json["restartedAfterRequest"], false);
        assert!(json["waitingIn"].is_null());
        let waiting = parse("art-firstboot 1\nstep 90-prefs started\nstep 90-prefs detail opened Locale\n");
        assert_eq!(serde_json::to_value(&waiting).unwrap()["waitingIn"], "locale");
```

- [ ] **Step 2: Run and see it fail**

Run: `… cargo test --lib firstboot:: > …/t6.txt 2>&1; echo "exit $?"`
Expected: `exit 101`, `struct `FirstBootReport` has no field named `reboot_unavailable``, `cannot find type `ForegroundWindow``.

- [ ] **Step 3: Implement**

`mod.rs`, after `impl WizardWindow { … }`:

```rust
/// A window `90-prefs` opens in the foreground, so the whole boot waits in it
/// (experiment 3: 4 of 4). ScreenMode runs detached and is not one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ForegroundWindow {
    Locale,
    Input,
}

impl ForegroundWindow {
    pub fn amiga_name(self) -> &'static str {
        match self {
            Self::Locale => "Locale",
            Self::Input => "Input",
        }
    }
}
```

`report.rs`: `use super::{ForegroundWindow, WIZARD_STEP};`. In `FirstBootReport`, after `reboot_requested_by`:

```rust
    /// `reboot unavailable` after the latest request: the tree had no
    /// `C:Reboot`, and the run carried on without restarting.
    pub reboot_unavailable: bool,
    /// A `system` line — a new boot — after the latest request that was not
    /// unavailable. The request alone says a restart was asked for; only a
    /// later boot says one happened.
    pub restarted_after_request: bool,
    /// The foreground window an unfinished `90-prefs` is waiting in: its last
    /// `detail` is `opened Locale` or `opened Input`.
    pub waiting_in: Option<ForegroundWindow>,
```

In `parse`: initialise the three (`false`, `false`, `None`); in the `["system", …]` arm, after `report.system = Some(…);`:

```rust
                if report.reboot_requested_by.is_some() && !report.reboot_unavailable {
                    report.restarted_after_request = true;
                }
```

replace the `["reboot", "requested", "by", name]` arm with:

```rust
            ["reboot", "requested", "by", name] => {
                report.reboot_requested_by = Some(name.to_string());
                report.reboot_unavailable = false;
                report.restarted_after_request = false;
            }
            ["reboot", "unavailable"] => report.reboot_unavailable = true,
```

and after the `report.ending = …;` statement, `report.waiting_in = waiting_in(&report);`. After `step_named`:

```rust
/// See [`FirstBootReport::waiting_in`]. The words are the script's own, built
/// from [`ForegroundWindow::amiga_name`] rather than copied here.
fn waiting_in(report: &FirstBootReport) -> Option<ForegroundWindow> {
    let step = report.steps.iter().rev().find(|s| s.name == WIZARD_STEP)?;
    if step.outcome != StepOutcome::Unfinished {
        return None;
    }
    let last = step.details.last()?;
    [ForegroundWindow::Locale, ForegroundWindow::Input]
        .into_iter()
        .find(|window| *last == format!("opened {}", window.amiga_name()))
}
```

- [ ] **Step 4: Run and see it pass**

`cargo fmt`; the Step 2 command, then `… cargo test --lib amigainstall::rehearse:: > …/t6b.txt 2>&1` and `… cargo test --lib commands::firstboot:: > …/t6c.txt 2>&1`.
Expected: three `test result: ok.` lines, 0 failed.

- [ ] **Step 5: Mutations**

Back up `report.rs`. Each: `cargo test --lib firstboot::report::`, named test red, restore with `shutil.copyfile`, green.

1. Delete the `["reboot", "unavailable"]` arm → `reboot_unavailable_is_its_own_flag_and_the_run_carries_on` red (line in `unknown`).
2. Delete the two reset lines in the request arm → `a_later_request_replaces_what_an_earlier_one_said` red.
3. Set `restarted_after_request = true` in the request arm instead of on a `system` line → `a_request_the_log_does_not_show_coming_back_is_neither_restarted_nor_unavailable` red.
4. In `waiting_in`, use `step.details.first()` → `waiting_in_names_only_a_foreground_window_that_is_the_wizards_last_word` red (the Input case).
5. Delete the `if step.outcome != StepOutcome::Unfinished` check → the same test red (the `ok` case).

- [ ] **Step 6: Commit**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && git branch --show-current
git add src-tauri/src/core/firstboot/mod.rs src-tauri/src/core/firstboot/report.rs src-tauri/src/core/amigainstall/rehearse.rs src-tauri/src/commands/firstboot.rs
git commit -F .superpowers/sdd/2026-09-15-firstboot-phase-3/msg-t6.txt
```

Message: `First boot phase 3: the report reads a restart and a waiting window (task 6)`, the five mutations, `Co-Authored-By`.

---

### Task 7: The rehearsal polls through a reboot and names the window it waits in

**Files:**
- Modify: `src-tauri/src/core/amigainstall/rehearse.rs` (`finished`'s doc comment, the `sink.report` in `poll`, a helper, tests)

**Interfaces:**
- Consumes: `FirstBootReport.waiting_in`, `restarted_after_request`, `reboot_requested_by` (Task 6); `ForegroundWindow::amiga_name` (Task 6); test fakes `FakeLauncher::running_forever()`, `TestClock::new(impl Fn(u32))`, `TestClock::idle()`, `clock.sleeps()`, `launcher.log.terminated`.
- Produces: `fn waiting_message(report: &FirstBootReport) -> String` (private); `RehearsalOutcome` and its five endings unchanged (design §7).

- [ ] **Step 1: Write the failing tests**

Append to `rehearse.rs`'s `mod tests` (add `use crate::core::firstboot::ForegroundWindow;`):

```rust
    /// What the job bar was told, in order.
    #[derive(Default)]
    struct Said(std::sync::Mutex<Vec<String>>);

    impl ProgressSink for Said {
        fn report(&self, _done: u64, _total: Option<u64>, message: &str) {
            self.0.lock().unwrap().push(message.to_string());
        }
        fn is_cancelled(&self) -> bool {
            false
        }
    }

    /// Design §7 and experiment 2 (18 of 18): the same WinUAE process and the
    /// same directory mount carry on across a guest reboot, so a reboot is a
    /// mid-run event and the poll goes on to the next boot's `done`.
    #[test]
    fn a_reboot_mid_run_is_not_an_ending_and_the_next_boots_done_all_finishes() {
        let profile = AmigaProfile::a1200_aga();
        let d = copy_with_report(
            "reboot",
            "art-firstboot 1\nsystem UAE none kick 3.2\nstep 10-hardware started\nstep 10-hardware skipped uae\nstep 10-hardware ok\nstep 15-probe started\nstep 15-probe ok\nreboot requested by 15-probe\n",
        );
        let rom = d.join("kick.rom");
        fs::write(&rom, vec![0u8; 16]).unwrap();
        let mut req = request(d.path(), d.path(), &profile, &rom);
        req.limits = RunLimits {
            deadline: Duration::from_secs(60),
            poll_interval: Duration::from_secs(2),
            ..RunLimits::default()
        };
        let log = d.join("S/FirstBoot.log");
        let clock = TestClock::new(move |n| {
            if n == 2 {
                let mut text = fs::read_to_string(&log).unwrap();
                text.push_str("system UAE none kick 3.2\nstep 20-aux started\nstep 20-aux skipped uae\nstep 20-aux ok\ndone all\n");
                fs::write(&log, text).unwrap();
            }
        });
        let launcher = FakeLauncher::running_forever();

        let outcome = rehearse_with(&req, &launcher, &clock, &NoProgress).unwrap();

        match outcome {
            RehearsalOutcome::Finished { report } => {
                assert_eq!(report.ending, Ending::DoneAll);
                assert_eq!(report.reboot_requested_by.as_deref(), Some("15-probe"));
                assert!(report.restarted_after_request);
            }
            other => panic!("a reboot is a mid-run event, got {other:?}"),
        }
        assert_eq!(clock.sleeps(), 2, "the poll carried on through the reboot");
        assert_eq!(launcher.log.terminated.lock().unwrap().as_slice(), &[4242]);
    }

    /// Design §7: a wizard rehearsal is interactive; the job bar names the
    /// window the boot is holding in, and the deadline carries that report.
    #[test]
    fn a_wizard_waiting_in_locale_is_named_while_polling_and_at_the_deadline() {
        let profile = AmigaProfile::a1200_aga();
        let d = copy_with_report(
            "locale",
            "art-firstboot 1\nsystem UAE none kick 3.2\nstep 90-prefs started\nstep 90-prefs detail opened Locale\n",
        );
        let rom = d.join("kick.rom");
        fs::write(&rom, vec![0u8; 16]).unwrap();
        let mut req = request(d.path(), d.path(), &profile, &rom);
        req.limits = RunLimits {
            deadline: Duration::from_secs(2),
            poll_interval: Duration::from_secs(1),
            ..RunLimits::default()
        };
        let said = Said::default();

        let outcome =
            rehearse_with(&req, &FakeLauncher::running_forever(), &TestClock::idle(), &said).unwrap();

        match outcome {
            RehearsalOutcome::TimedOut { report, .. } => {
                assert_eq!(report.waiting_in, Some(ForegroundWindow::Locale))
            }
            other => panic!("expected the deadline, got {other:?}"),
        }
        let lines = said.0.lock().unwrap();
        assert!(
            lines.iter().any(|l| l
                == "The Amiga is waiting for you in the Locale window — answer it in the WinUAE window"),
            "{lines:?}"
        );
        assert!(!lines.iter().any(|l| l.contains("steps reported so far")), "{lines:?}");
    }

    /// The control: with no window open the line still counts steps.
    #[test]
    fn a_rehearsal_with_no_window_open_counts_steps_as_before() {
        let profile = AmigaProfile::a1200_aga();
        let d = copy_with_report("no-window", "art-firstboot 1\nstep 10-hardware started\n");
        let rom = d.join("kick.rom");
        fs::write(&rom, vec![0u8; 16]).unwrap();
        let mut req = request(d.path(), d.path(), &profile, &rom);
        req.limits = RunLimits {
            deadline: Duration::from_secs(2),
            poll_interval: Duration::from_secs(1),
            ..RunLimits::default()
        };
        let said = Said::default();
        rehearse_with(&req, &FakeLauncher::running_forever(), &TestClock::idle(), &said).unwrap();
        let lines = said.0.lock().unwrap();
        assert!(
            lines.iter().any(|l| l == "Waiting for the Amiga — 1 steps reported so far"),
            "{lines:?}"
        );
        assert!(!lines.iter().any(|l| l.contains("waiting for you")), "{lines:?}");
    }
```

- [ ] **Step 2: Run and see it fail**

Run: `… cargo test --lib amigainstall::rehearse:: > …/t7.txt 2>&1; echo "exit $?"`
Expected: `a_wizard_waiting_in_locale_is_named_while_polling_and_at_the_deadline` FAILED (no *waiting for you* line); the other two pass — the reboot test documents behaviour that already holds (finished ends only on `done`) and becomes a guard in Step 5.

- [ ] **Step 3: Implement**

In `poll`, replace

```rust
        let so_far = report.steps.len();
        sink.report(
            waited.as_secs(),
            deadline_secs(&request.limits),
            &format!("Waiting for the Amiga — {so_far} steps reported so far"),
        );
```

with

```rust
        sink.report(
            waited.as_secs(),
            deadline_secs(&request.limits),
            &waiting_message(&report),
        );
```

After `read_report`:

```rust
/// The job bar's line while the poll waits. A foreground Preferences window
/// holds the whole boot until a person answers it (experiment 3), so a wizard
/// rehearsal names that window rather than counting steps that will not move.
/// English, like every `sink.report` message: the job bar shows it as it is.
fn waiting_message(report: &FirstBootReport) -> String {
    match report.waiting_in {
        Some(window) => format!(
            "The Amiga is waiting for you in the {} window — answer it in the WinUAE window",
            window.amiga_name()
        ),
        None => format!(
            "Waiting for the Amiga — {} steps reported so far",
            report.steps.len()
        ),
    }
}
```

Replace `finished`'s doc comment with:

```rust
/// `done` is the only ending the Amiga writes; which of the two outcomes it
/// is comes from whether any step refused, not from the `all`/`partial` word
/// alone. **A reboot is not an ending** (first-boot phase 3 design §7): the
/// step wrapper carries it out, WinUAE keeps the same process and the same
/// directory mount across it (measured 2026-09-15, 18 of 18), and the log
/// simply continues on the next boot — so the poll goes on until `done`.
```

- [ ] **Step 4: Run and see it pass**

`cargo fmt`; the Step 2 command. Expected: `test result: ok.`, 0 failed.

- [ ] **Step 5: Mutations**

Back up `rehearse.rs`. Each: `cargo test --lib amigainstall::rehearse::`, named test red, restore with `shutil.copyfile`, green.

1. In `waiting_message`, return the `None` arm's text for every report → `a_wizard_waiting_in_locale_is_named_while_polling_and_at_the_deadline` red.
2. In `poll`, directly after `if let Some(done) = finished(&report) { return Ok(done); }`, add `if report.reboot_requested_by.is_some() { return Ok(RehearsalOutcome::EmulatorClosed { waited: clock.elapsed(), report }); }` → `a_reboot_mid_run_is_not_an_ending_and_the_next_boots_done_all_finishes` red.
3. Make `waiting_message` always name a window (`Some(window)` arm for `None` with `"Locale"`) → `a_rehearsal_with_no_window_open_counts_steps_as_before` red.

- [ ] **Step 6: Commit**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && git branch --show-current
git add src-tauri/src/core/amigainstall/rehearse.rs
git commit -F .superpowers/sdd/2026-09-15-firstboot-phase-3/msg-t7.txt
```

Message: `First boot phase 3: the rehearsal polls through a reboot and names the waiting window (task 7)`, the three mutations, `Co-Authored-By`.

---

### Task 8: The tick on the wire — commands, `@/lib/firstboot`, the session, the run

**Files:**
- Modify: `src-tauri/src/commands/firstboot.rs` (`firstboot_preview`, `firstboot_write` signatures)
- Modify: `src/lib/firstboot.ts` (+ `src/lib/firstboot.test.ts`)
- Modify: `src/lib/buildSession.ts` (+ `src/lib/buildSession.test.ts`)
- Modify: `src/lib/buildRun.ts` (+ `src/lib/buildRun.test.ts`), `src/components/osbuilder/buildSummary.ts`
- Modify: `src/lib/useBuildRun.ts` (+ `src/lib/useBuildRun.test.tsx`)
- Modify: `src/components/osbuilder/ChoiceTab.tsx` (the `firstbootPreview` call only)
- Modify: `src/i18n/en.json`, `src/i18n/tr.json` (the mapper keys), `src/i18n/phrase-keys.test.ts`
- Modify (fixture types only): `src/components/osbuilder/BuildTab.test.tsx`, `src/components/osbuilder/ChoiceTab.test.tsx`, `src/components/osbuilder/FirstBootRehearse.test.tsx`, `src/components/card/FirstBootReportPanel.test.tsx`, `src/pages/HardDiskStudio.test.tsx`

**Interfaces:**
- Consumes: Rust wire from Tasks 3, 4, 6.
- Produces:
  - Commands: `firstboot_preview(tree: String, ask_prefs: bool)`, `firstboot_write(tree: String, ask_prefs: bool, oplog)` — Tauri maps `ask_prefs` to the JS key `askPrefs`.
  - TS types: `RebootCommand`, `WizardWindow = "locale" | "input" | "screen-mode"`, `WindowState = "ask" | "set-by-art" | "missing"`, `WizardRow`, `WizardPlan`, `ForegroundWindow = "locale" | "input"`, `WizardLines { ask; setByArt; missing: WizardWindow[] }`, `WizardDetail { phrase: Phrase; window: WizardWindow }`; `FirstBootPlan` + `reboot`, `wizard: WizardPlan | null`, `inputSetByArt`; `FirstBootWritten` + `removed: string[]`; `FirstBootReport` + `rebootUnavailable`, `restartedAfterRequest`, `waitingIn: ForegroundWindow | null`.
  - TS functions: `firstbootPreview(tree: string, askPrefs: boolean)`, `firstbootWrite(tree: string, askPrefs: boolean)`, `rebootCommandPhrase(reboot: RebootCommand): Phrase | null`, `wizardWindowPhrase(window: WizardWindow): Phrase`, `wizardEditorPath(window: WizardWindow): string`, `wizardLines(wizard: WizardPlan): WizardLines`, `wizardDetail(detail: string): WizardDetail | null`, `rebootReportPhrase(report: FirstBootReport): Phrase | null`, `export const WIZARD_STEP_NAME = "90-prefs"`; `rehearsalOutcomePhrase`/`rehearsalNextStepPhrase` gain the waiting-window variants.
  - Session: `FirstBootChoice.askPrefs?: boolean`, `FIRSTBOOT_SPEC.askPrefs: isFlag`.
  - Run: `Phase.askPrefs?: boolean`, `SequenceInputs.firstBootAskPrefs: boolean`; `useBuildRun` calls `firstbootWrite(destination, phase.askPrefs ?? true)`.
  - Keys (both catalogues): `firstboot.reboot.unavailable`; `firstboot.wizard.window.{locale,input,screenMode}`; `firstboot.report.wizard.{opened,notAsked,missing}`; `firstboot.report.reboot.{restarted,unavailable,pending}`; `firstboot.rehearsal.outcome.{timedOutInLocale,timedOutInInput}`; `firstboot.rehearsal.next.{timedOutInLocale,timedOutInInput}`.

- [ ] **Step 1: Write the failing tests**

`src/lib/firstboot.test.ts` — at the top, below the `vitest` import (make it `import { beforeEach, describe, expect, it, vi } from "vitest";`):

```ts
const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
```

extend the `@/lib/firstboot` import with `firstbootPreview, firstbootWrite, rebootCommandPhrase, rebootReportPhrase, wizardDetail, wizardEditorPath, wizardLines, wizardWindowPhrase, WIZARD_STEP_NAME, type WizardPlan`, add three source reads beside `REPORT`:

```ts
const PLAN = readFileSync(
  resolve(__dirname, "..", "..", "src-tauri", "src", "core", "firstboot", "plan.rs"),
  "utf8"
);
const MOD = readFileSync(
  resolve(__dirname, "..", "..", "src-tauri", "src", "core", "firstboot", "mod.rs"),
  "utf8"
);
```

add to `EMPTY_REPORT`: `rebootUnavailable: false, restartedAfterRequest: false, waitingIn: null,` and append:

```ts
describe("the tick crosses the wire under one name, both ways", () => {
  beforeEach(() => invokeMock.mockReset().mockResolvedValue({}));

  it("firstbootPreview sends askPrefs", async () => {
    await firstbootPreview("E:\\dist", false);
    expect(invokeMock).toHaveBeenCalledWith("firstboot_preview", { tree: "E:\\dist", askPrefs: false });
  });

  it("firstbootWrite sends askPrefs", async () => {
    await firstbootWrite("E:\\dist", true);
    expect(invokeMock).toHaveBeenCalledWith("firstboot_write", { tree: "E:\\dist", askPrefs: true });
  });

  it("and the Rust commands take ask_prefs, which Tauri calls askPrefs", () => {
    expect(COMMAND).toMatch(/pub fn firstboot_preview\(\s*tree: String,\s*ask_prefs: bool,?\s*\)/);
    expect(COMMAND).toMatch(/pub fn firstboot_write\(\s*tree: String,\s*ask_prefs: bool,/);
  });

  it("the new enums are tagged the way this file reads them", () => {
    expect(PLAN).toMatch(/#\[serde\(tag = "kind", rename_all = "kebab-case"\)\]\s*\r?\n\s*pub enum RebootCommand/);
    expect(PLAN).toMatch(/#\[serde\(rename_all = "kebab-case"\)\]\s*\r?\n\s*pub enum WindowState/);
    expect(MOD).toMatch(/#\[serde\(rename_all = "kebab-case"\)\]\s*\r?\n\s*pub enum WizardWindow/);
    expect(MOD).toMatch(/#\[serde\(rename_all = "kebab-case"\)\]\s*\r?\n\s*pub enum ForegroundWindow/);
    expect(MOD).toContain(`pub const WIZARD_STEP: &str = "${WIZARD_STEP_NAME}";`);
  });
});

describe("the reboot and the wizard, as phrases", () => {
  it("says nothing about an available reboot and names the package otherwise", () => {
    expect(rebootCommandPhrase({ kind: "available" })).toBeNull();
    expect(rebootCommandPhrase({ kind: "unavailable", needs: "reboot" })).toEqual({
      key: "firstboot.reboot.unavailable",
      params: { needs: "reboot" },
    });
  });

  it("groups the rows by what the Amiga will do", () => {
    const wizard: WizardPlan = {
      rows: [
        { window: "locale", state: "ask" },
        { window: "input", state: "set-by-art" },
        { window: "screen-mode", state: "missing" },
      ],
    };
    expect(wizardLines(wizard)).toEqual({ ask: ["locale"], setByArt: ["input"], missing: ["screen-mode"] });
  });

  it("gives each window its own label and its own editor", () => {
    const windows = ["locale", "input", "screen-mode"] as const;
    expect(new Set(windows.map((w) => wizardWindowPhrase(w).key)).size).toBe(3);
    expect(windows.map(wizardEditorPath)).toEqual(["SYS:Prefs/Locale", "SYS:Prefs/Input", "SYS:Prefs/ScreenMode"]);
  });

  it("reads 90-prefs's three detail words and nothing else", () => {
    expect(wizardDetail("opened Locale")).toEqual({ phrase: { key: "firstboot.report.wizard.opened" }, window: "locale" });
    expect(wizardDetail("not-asked Input")).toEqual({ phrase: { key: "firstboot.report.wizard.notAsked" }, window: "input" });
    expect(wizardDetail("missing ScreenMode")).toEqual({ phrase: { key: "firstboot.report.wizard.missing" }, window: "screen-mode" });
    expect(wizardDetail("sd0-mounted RPi4")).toBeNull();
    expect(wizardDetail("opened constructor")).toBeNull();
    expect(wizardDetail("opened Locale twice")).toBeNull();
  });

  it("keeps four restart states apart: none, restarted, no command, not back yet", () => {
    const asked = { ...EMPTY_REPORT, rebootRequestedBy: "15-probe" };
    expect(rebootReportPhrase(EMPTY_REPORT)).toBeNull();
    expect(rebootReportPhrase({ ...asked, restartedAfterRequest: true })?.key).toBe("firstboot.report.reboot.restarted");
    expect(rebootReportPhrase({ ...asked, rebootUnavailable: true })?.key).toBe("firstboot.report.reboot.unavailable");
    expect(rebootReportPhrase(asked)).toEqual({ key: "firstboot.report.reboot.pending", params: { step: "15-probe" } });
  });

  it("names the window a timed-out rehearsal was waiting in, with its own next step", () => {
    const waited = { secs: 120, nanos: 0 };
    const plain: RehearsalOutcome = { kind: "timed-out", waited, report: EMPTY_REPORT };
    const locale: RehearsalOutcome = { kind: "timed-out", waited, report: { ...EMPTY_REPORT, waitingIn: "locale" } };
    const input: RehearsalOutcome = { kind: "timed-out", waited, report: { ...EMPTY_REPORT, waitingIn: "input" } };
    const said = [plain, locale, input].map((o) => rehearsalOutcomePhrase(o).key);
    const next = [plain, locale, input].map((o) => rehearsalNextStepPhrase(o).key);
    expect(new Set(said).size).toBe(3);
    expect(new Set(next).size).toBe(3);
    expect(rehearsalOutcomePhrase(locale)).toEqual({
      key: "firstboot.rehearsal.outcome.timedOutInLocale",
      params: { seconds: 120 },
    });
  });
});
```

`src/lib/buildSession.test.ts` — append inside `describe("FIRSTBOOT_SPEC", …)`:

```ts
  // First-boot phase 3 design §5: the second tick is the `wanted` rule, guarded the same way.
  it("leaves askPrefs absent for a firstboot stored before the second tick", () => {
    const recalled = recallInto<FirstBootChoice>(
      { [SESSION_KEYS.firstboot]: { written: true, wanted: true } },
      SESSION_KEYS.firstboot,
      FIRSTBOOT_SPEC,
      DEFAULT_FIRSTBOOT
    );
    expect("askPrefs" in recalled).toBe(false);
  });

  it("keeps an askPrefs the user turned off, and one turned back on", () => {
    for (const askPrefs of [false, true]) {
      expect(
        recallInto<FirstBootChoice>(
          { [SESSION_KEYS.firstboot]: { written: false, askPrefs } },
          SESSION_KEYS.firstboot,
          FIRSTBOOT_SPEC,
          DEFAULT_FIRSTBOOT
        ).askPrefs
      ).toBe(askPrefs);
    }
  });

  it("ships no askPrefs in the default, so rendering the second tick stores nothing", () => {
    expect("askPrefs" in DEFAULT_FIRSTBOOT).toBe(false);
  });
```

`src/lib/buildRun.test.ts` — add `firstBootAskPrefs: true,` to `INPUTS` after `firstBootWanted: true,`; add `removed: [],` to `WRITTEN`; append inside the `sequenceFor` describe (the one holding `adds the first-boot phase only when it is wanted`):

```ts
  it("carries the second tick on the first-boot phase, as the user confirmed it", () => {
    const off = sequenceFor({ ...INPUTS, firstBootAskPrefs: false }).find((p) => p.kind === "firstboot");
    const on = sequenceFor(INPUTS).find((p) => p.kind === "firstboot");
    expect(off?.askPrefs).toBe(false);
    expect(on?.askPrefs).toBe(true);
  });
```

`src/lib/useBuildRun.test.tsx` — `WRITTEN` gains `removed: [],`; `FIRSTBOOT_PHASE` gains `askPrefs: true,`; replace `expect(firstbootWriteMock).toHaveBeenCalledWith(DEST);` with `expect(firstbootWriteMock).toHaveBeenCalledWith(DEST, true);`; append inside `describe("the build run's sequence", …)`:

```ts
  it("hands first boot the second tick the phase carries, unticked included", async () => {
    const { result } = setup();
    act(() => {
      result.current.start([{ ...FIRSTBOOT_PHASE, askPrefs: false }], PLAN);
    });
    await waitFor(() => expect(result.current.finished).toBe(true));
    expect(firstbootWriteMock).toHaveBeenCalledWith(DEST, false);
  });
```

`src/i18n/phrase-keys.test.ts` — extend the `@/lib/firstboot` import with `rebootCommandPhrase, rebootReportPhrase, wizardDetail, wizardWindowPhrase`; in `"firstboot rehearsal: every ending resolves…"` add the three report fields to `report` and append `{ kind: "timed-out", waited: { secs: 60, nanos: 0 }, report: { ...report, waitingIn: "locale" as const } }` and the same with `"input" as const` to `endings`; append:

```ts
  it("the reboot, the wizard's windows and details, and the restart lines all resolve", () => {
    const unavailable = rebootCommandPhrase({ kind: "unavailable", needs: "reboot" });
    expect(isLeafKey(unavailable!.key), unavailable!.key).toBe(true);
    for (const window of ["locale", "input", "screen-mode"] as const) {
      expect(isLeafKey(wizardWindowPhrase(window).key), window).toBe(true);
    }
    for (const detail of ["opened Locale", "not-asked Input", "missing ScreenMode"]) {
      const read = wizardDetail(detail);
      expect(isLeafKey(read!.phrase.key), detail).toBe(true);
    }
    const base = {
      version: 1, system: null, steps: [], ending: "unfinished" as const, fatCopyFailed: false,
      rebootRequestedBy: "15-probe", rebootUnavailable: false, restartedAfterRequest: false,
      waitingIn: null, unknown: [],
    };
    for (const report of [base, { ...base, rebootUnavailable: true }, { ...base, restartedAfterRequest: true }]) {
      const phrase = rebootReportPhrase(report);
      expect(isLeafKey(phrase!.key), phrase!.key).toBe(true);
    }
  });
```

Fixture types only (no behaviour asserted; `pnpm lint`'s second run fails without them): add `rebootUnavailable: false, restartedAfterRequest: false, waitingIn: null,` to the `FirstBootReport` fixtures in `FirstBootRehearse.test.tsx` (`REPORT`), `FirstBootReportPanel.test.tsx` (`report()`), `HardDiskStudio.test.tsx` (`report()`); add `reboot: { kind: "available" }, wizard: null, inputSetByArt: false,` to the four `FirstBootPlan` literals in `ChoiceTab.test.tsx` (the `beforeEach` one and the three in `describe("the first-boot tick")`) and to `BuildTab.test.tsx`'s `firstbootPreviewMock` default; add `removed: [],` to `BuildTab.test.tsx`'s `WRITTEN` and its three `firstBootRow({ … })` literals.

- [ ] **Step 2: Run and see it fail**

Run (repository root): `pnpm vitest run src/lib/firstboot.test.ts src/lib/buildSession.test.ts src/lib/buildRun.test.ts src/lib/useBuildRun.test.tsx src/i18n/phrase-keys.test.ts > .superpowers/sdd/2026-09-15-firstboot-phase-3/t8.txt 2>&1; echo "exit $?"`
Expected: `exit 1`; failures include `firstbootPreview sends askPrefs` (called with `{ tree }` only), the Rust-command regexes, `ships no askPrefs…` passing but `keeps an askPrefs the user turned off` failing (`undefined`), `carries the second tick…` (`undefined`), `hands first boot the second tick…` (called with `DEST`), and `rebootCommandPhrase is not a function`.

- [ ] **Step 3: Implement**

`commands/firstboot.rs`:

```rust
#[tauri::command]
pub fn firstboot_preview(tree: String, ask_prefs: bool) -> AppResult<FirstBootPlan> {
    Ok(plan(&FirstBootRequest {
        tree: PathBuf::from(tree.trim()),
        ask_prefs,
    })?)
}
```

and `pub fn firstboot_write(tree: String, ask_prefs: bool, oplog: State<'_, JsonlOperationLog>)` with `ask_prefs,` in its request (delete Task 3's two `ask_prefs: false` comments).

`src/lib/firstboot.ts` — after `export type FatMount …`:

```ts
/** Whether the step wrapper can carry out a reboot request (phase 3 design §2
 *  decision 4). Not a refusal — the `FatMount` shape. */
export type RebootCommand = { kind: "available" } | { kind: "unavailable"; needs: string };

/** A Preferences window the wizard may open, in `90-prefs`'s order. */
export type WizardWindow = "locale" | "input" | "screen-mode";
export type WindowState = "ask" | "set-by-art" | "missing";

export interface WizardRow {
  window: WizardWindow;
  state: WindowState;
}

export interface WizardPlan {
  rows: WizardRow[];
}

/** A window the boot waits in. ScreenMode runs detached and never is. */
export type ForegroundWindow = "locale" | "input";

/** `core::firstboot::WIZARD_STEP` — `firstboot.test.ts` holds it to the Rust. */
export const WIZARD_STEP_NAME = "90-prefs";
```

`FirstBootPlan` gains `reboot: RebootCommand; wizard: WizardPlan | null; inputSetByArt: boolean;` · `FirstBootWritten` gains `removed: string[];` · `FirstBootReport` gains `rebootUnavailable: boolean; restartedAfterRequest: boolean; waitingIn: ForegroundWindow | null;`.

```ts
export async function firstbootPreview(tree: string, askPrefs: boolean): Promise<FirstBootPlan> {
  return invoke<FirstBootPlan>("firstboot_preview", { tree, askPrefs });
}

export async function firstbootWrite(tree: string, askPrefs: boolean): Promise<FirstBootWritten> {
  return invoke<FirstBootWritten>("firstboot_write", { tree, askPrefs });
}
```

After `fatMountPhrase`:

```ts
/** The line beside the tick when the tree has no reboot command; an
 *  available command has nothing to say. */
export function rebootCommandPhrase(reboot: RebootCommand): Phrase | null {
  return reboot.kind === "available"
    ? null
    : { key: "firstboot.reboot.unavailable", params: { needs: reboot.needs } };
}

/** A window's label — what it asks, never its file name (Beginner mode). */
export function wizardWindowPhrase(window: WizardWindow): Phrase {
  switch (window) {
    case "locale":
      return { key: "firstboot.wizard.window.locale" };
    case "input":
      return { key: "firstboot.wizard.window.input" };
    case "screen-mode":
      return { key: "firstboot.wizard.window.screenMode" };
  }
}

/** The editor `90-prefs` runs — for Power mode only. An AmigaDOS path, never translated. */
export function wizardEditorPath(window: WizardWindow): string {
  switch (window) {
    case "locale":
      return "SYS:Prefs/Locale";
    case "input":
      return "SYS:Prefs/Input";
    case "screen-mode":
      return "SYS:Prefs/ScreenMode";
  }
}

export interface WizardLines {
  ask: WizardWindow[];
  setByArt: WizardWindow[];
  missing: WizardWindow[];
}

/** The rows, grouped by what the Amiga will do with each. */
export function wizardLines(wizard: WizardPlan): WizardLines {
  const pick = (state: WindowState) =>
    wizard.rows.filter((row) => row.state === state).map((row) => row.window);
  return { ask: pick("ask"), setByArt: pick("set-by-art"), missing: pick("missing") };
}

export interface WizardDetail {
  phrase: Phrase;
  window: WizardWindow;
}

function windowNamed(name: string | undefined): WizardWindow | null {
  switch (name) {
    case "Locale":
      return "locale";
    case "Input":
      return "input";
    case "ScreenMode":
      return "screen-mode";
    default:
      return null;
  }
}

/** One of `90-prefs`'s own detail lines, read as a sentence: `opened Locale`,
 *  `not-asked Input`, `missing ScreenMode`. `null` for any other detail, which
 *  stays a raw line only Power mode shows. The component fills `window` with
 *  the translated {@link wizardWindowPhrase}. */
export function wizardDetail(detail: string): WizardDetail | null {
  const words = detail.split(" ");
  const window = windowNamed(words[1]);
  if (words.length !== 2 || window === null) return null;
  switch (words[0]) {
    case "opened":
      return { phrase: { key: "firstboot.report.wizard.opened" }, window };
    case "not-asked":
      return { phrase: { key: "firstboot.report.wizard.notAsked" }, window };
    case "missing":
      return { phrase: { key: "firstboot.report.wizard.missing" }, window };
    default:
      return null;
  }
}

/** What the report says about a restart. Four states, three sentences and
 *  silence: a request is not a restart until a later boot shows one. */
export function rebootReportPhrase(report: FirstBootReport): Phrase | null {
  const step = report.rebootRequestedBy;
  if (step === null) return null;
  if (report.rebootUnavailable) return { key: "firstboot.report.reboot.unavailable", params: { step } };
  if (report.restartedAfterRequest) return { key: "firstboot.report.reboot.restarted", params: { step } };
  return { key: "firstboot.report.reboot.pending", params: { step } };
}
```

In `rehearsalOutcomePhrase`, the `"timed-out"` arm becomes:

```ts
    case "timed-out": {
      const params = { seconds: waitedSeconds(outcome.waited) };
      switch (outcome.report.waitingIn) {
        case "locale":
          return { key: "firstboot.rehearsal.outcome.timedOutInLocale", params };
        case "input":
          return { key: "firstboot.rehearsal.outcome.timedOutInInput", params };
        case null:
          return { key: "firstboot.rehearsal.outcome.timedOut", params };
      }
    }
```

and in `rehearsalNextStepPhrase`:

```ts
    case "timed-out":
      switch (outcome.report.waitingIn) {
        case "locale":
          return { key: "firstboot.rehearsal.next.timedOutInLocale" };
        case "input":
          return { key: "firstboot.rehearsal.next.timedOutInInput" };
        case null:
          return { key: "firstboot.rehearsal.next.timedOut" };
      }
```

`src/lib/buildSession.ts` — in `FirstBootChoice`, after `wanted?: boolean;`:

```ts
  /**
   * Whether first boot asks the remaining Preferences on the Amiga — the
   * second tick (first-boot phase 3 design §5). **The `wanted` rule exactly:**
   * absent means ticked, rendering writes nothing, a stored `false` survives.
   * It is a preference, not a fact about a folder, so `setTree` leaves it.
   */
  askPrefs?: boolean;
```

and in `FIRSTBOOT_SPEC`, after `wanted: isFlag,`: `askPrefs: isFlag,` (with the comment `// Guarded, and absent from DEFAULT_FIRSTBOOT, for `wanted`'s reason.`).

`src/lib/buildRun.ts` — in `Phase`, after `folder?: string;`:

```ts
  /** First-boot phases only: the second tick, as confirmed when the run was
   *  started — so a tick changed mid-run does not change what this run writes. */
  askPrefs?: boolean;
```

in `SequenceInputs`, after `firstBootWanted: boolean;`: `firstBootAskPrefs: boolean;` — and in `sequenceFor`:

```ts
  if (inputs.firstBootWanted) {
    phases.push({
      id: "firstboot",
      kind: "firstboot",
      name: FIRST_BOOT_PHASE_NAME,
      askPrefs: inputs.firstBootAskPrefs,
    });
  }
```

`src/components/osbuilder/buildSummary.ts` — after `const firstBootWanted = session.firstboot.wanted ?? true;`: `const firstBootAskPrefs = session.firstboot.askPrefs ?? true;`, and `firstBootAskPrefs,` in the `sequenceFor({ … })` call.

`src/components/osbuilder/ChoiceTab.tsx` — the one call `pnpm lint` would otherwise refuse (Task 9 builds the screen on it): above the first-boot preview effect add `const askPrefs = session.firstboot.askPrefs ?? true;`, call `firstbootPreview(treeRoot, askPrefs)`, and make the effect's dependency list `[treeRoot, treeSettled, askPrefs]`.

`src/lib/useBuildRun.ts` — the `"firstboot"` arm:

```ts
          case "firstboot": {
            // The second tick rides on the phase (`sequenceFor`); absent
            // means ticked, the session's own rule.
            const written = await firstbootWrite(destination, phase.askPrefs ?? true);
```

`src/i18n/en.json`, inside `"firstboot"`: after `"fat": {…},` add

```json
    "reboot": {
      "unavailable": "This release has no reboot command: a step that asks for a restart will leave the restart to you. Aminet util/boot/{{needs}} supplies one."
    },
    "wizard": {
      "window": {
        "locale": "language and country",
        "input": "keyboard",
        "screenMode": "screen mode"
      }
    },
```

in `"rehearsal"."outcome"` after `"timedOut"`:

```json
        "timedOutInLocale": "Nobody answered the language and country window. The Amiga was still waiting in it {{seconds}} seconds in, so ART ended the emulator it had started; the copy is kept.",
        "timedOutInInput": "Nobody answered the keyboard window. The Amiga was still waiting in it {{seconds}} seconds in, so ART ended the emulator it had started; the copy is kept.",
```

in `"rehearsal"."next"` after `"timedOut"`:

```json
        "timedOutInLocale": "Run it again and answer the language and country window inside the WinUAE window: first boot waits there until you press Save.",
        "timedOutInInput": "Run it again and answer the keyboard window inside the WinUAE window: first boot waits there until you press Save.",
```

in `"report"` after `"details"`:

```json
      "wizard": {
        "opened": "{{window}}: the window opened",
        "notAsked": "{{window}}: not asked — ART set it",
        "missing": "{{window}}: not asked — this release has no window for it"
      },
      "reboot": {
        "restarted": "{{step}} asked for a restart, and the Amiga restarted.",
        "unavailable": "{{step}} asked for a restart, and this Amiga has no command for it — restart it yourself to finish.",
        "pending": "{{step}} asked for a restart; the log does not show the Amiga coming back up yet."
      },
```

`src/i18n/tr.json`, the same keys in the same places:

```json
    "reboot": {
      "unavailable": "Bu sürümde yeniden başlatma komutu yok: yeniden başlatma isteyen bir adım bunu size bırakır. Aminet util/boot/{{needs}} bunu sağlar."
    },
    "wizard": {
      "window": {
        "locale": "dil ve ülke",
        "input": "klavye",
        "screenMode": "ekran modu"
      }
    },
```

```json
        "timedOutInLocale": "Dil ve ülke penceresini kimse yanıtlamadı. Amiga {{seconds}} saniye sonra hâlâ o pencerede bekliyordu, bu yüzden ART başlattığı emülatörü sonlandırdı; kopya saklandı.",
        "timedOutInInput": "Klavye penceresini kimse yanıtlamadı. Amiga {{seconds}} saniye sonra hâlâ o pencerede bekliyordu, bu yüzden ART başlattığı emülatörü sonlandırdı; kopya saklandı.",
```

```json
        "timedOutInLocale": "Yeniden çalıştırın ve WinUAE penceresinin içindeki dil ve ülke penceresini yanıtlayın: ilk açılış siz Save düğmesine basana kadar orada bekler.",
        "timedOutInInput": "Yeniden çalıştırın ve WinUAE penceresinin içindeki klavye penceresini yanıtlayın: ilk açılış siz Save düğmesine basana kadar orada bekler.",
```

```json
      "wizard": {
        "opened": "{{window}}: pencere açıldı",
        "notAsked": "{{window}}: sorulmadı — ART ayarladı",
        "missing": "{{window}}: sorulmadı — bu sürümde bunun için pencere yok"
      },
      "reboot": {
        "restarted": "{{step}} yeniden başlatma istedi ve Amiga yeniden başladı.",
        "unavailable": "{{step}} yeniden başlatma istedi, ama bu Amiga'da bunun için komut yok — bitirmek için onu kendiniz yeniden başlatın.",
        "pending": "{{step}} yeniden başlatma istedi; günlük Amiga'nın yeniden açıldığını henüz göstermiyor."
      },
```

(“Save” stays English in Turkish: it is the Amiga editor's own button, which a Turkish catalogue on the host cannot rename.)

- [ ] **Step 4: Run and see it pass**

Run the Step 2 command, then `pnpm lint > …/t8-lint.txt 2>&1; echo "exit $?"`, then (from `src-tauri`) `… cargo test --lib commands::firstboot:: > …/t8-rust.txt 2>&1`.
Expected: vitest `Tests  N passed` with 0 failed in the five files; `pnpm lint` `exit 0` (both `tsc` runs); Rust `test result: ok.`. Then `pnpm vitest run src/i18n > …/t8-i18n.txt 2>&1` — parity, dead-keys and literal-keys green (no component renders a new key yet, and the mapper literals are readers for dead-keys).

- [ ] **Step 5: Mutations**

Back up each file by absolute path before its mutation; run the named file; see the named test red; restore with `shutil.copyfile`; green.

1. `buildSession.ts`: delete `askPrefs: isFlag,` → `keeps an askPrefs the user turned off, and one turned back on` red.
2. `buildSession.ts`: `DEFAULT_FIRSTBOOT = { written: false, askPrefs: true }` → `ships no askPrefs in the default…` red.
3. `useBuildRun.ts`: `firstbootWrite(destination, true)` → `hands first boot the second tick the phase carries, unticked included` red.
4. `firstboot.ts`: `invoke<FirstBootWritten>("firstboot_write", { tree })` → `firstbootWrite sends askPrefs` red.
5. `buildRun.ts`: drop `askPrefs: inputs.firstBootAskPrefs` → `carries the second tick on the first-boot phase…` red.
6. `firstboot.ts`: in `rebootReportPhrase` return the `restarted` phrase where `pending` is returned → `keeps four restart states apart…` red.
7. `firstboot.ts`: in `rehearsalOutcomePhrase` return `timedOut` for `"locale"` → `names the window a timed-out rehearsal was waiting in…` red.

- [ ] **Step 6: Commit**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && git branch --show-current
git add src-tauri/src/commands/firstboot.rs src/components/osbuilder/ChoiceTab.tsx src/lib/firstboot.ts src/lib/firstboot.test.ts src/lib/buildSession.ts src/lib/buildSession.test.ts src/lib/buildRun.ts src/lib/buildRun.test.ts src/components/osbuilder/buildSummary.ts src/lib/useBuildRun.ts src/lib/useBuildRun.test.tsx src/i18n/en.json src/i18n/tr.json src/i18n/phrase-keys.test.ts src/components/osbuilder/BuildTab.test.tsx src/components/osbuilder/ChoiceTab.test.tsx src/components/osbuilder/FirstBootRehearse.test.tsx src/components/card/FirstBootReportPanel.test.tsx src/pages/HardDiskStudio.test.tsx
git commit -F .superpowers/sdd/2026-09-15-firstboot-phase-3/msg-t8.txt
```

Message: `First boot phase 3: the second tick on the wire, the session and the run (task 8)`, the seven mutations, `Co-Authored-By`.

---

### Task 9: The screen — the second tick, its lines, the report's restart and wizard sentences

**Files:**
- Modify: `src/components/osbuilder/ChoiceTab.tsx` (imports, group 3's first-boot preview state and effect, the group-3 JSX block)
- Modify: `src/components/osbuilder/ChoiceTab.test.tsx` (`describe("the first-boot tick")`, `describe("in Turkish")`)
- Modify: `src/components/card/FirstBootReportPanel.tsx` (+ `.test.tsx`)
- Modify: `src/components/osbuilder/FirstBootRehearse.test.tsx`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json` (`osBuilder.choice.*`), `src/i18n/literal-keys.test.ts`

**Interfaces:**
- Consumes (Task 8): `firstbootPreview(tree, askPrefs)`, `rebootCommandPhrase`, `wizardWindowPhrase`, `wizardEditorPath`, `wizardLines`, `wizardDetail`, `rebootReportPhrase`, `WIZARD_STEP_NAME`, types `RebootCommand`, `WizardPlan`, `WizardWindow`, `WizardDetail`; `useBuildSession().setFirstBoot(change: Partial<FirstBootChoice>)`; `usePowerMode()` from `@/lib/uxmode`.
- Produces: test ids `choice-firstboot-askprefs`, `choice-firstboot-askprefs-no-effect`, `choice-firstboot-windows`, `choice-firstboot-windows-files`, `choice-firstboot-windows-after-build`, `choice-firstboot-reboot`, `firstboot-report-reboot`, `firstboot-report-wizard`; keys `osBuilder.choice.{askPrefsRow, askPrefsNoEffect, askPrefsAfterBuild, askPrefsAsk, askPrefsSetByArt, askPrefsMissing, askPrefsAsOf}`. `literal-keys.test.ts`'s dynamic-call count 191 → 196.

- [ ] **Step 1: Write the failing tests**

`ChoiceTab.test.tsx` — import `type WizardPlan` beside `FirstBootPlan`; add after `destinationIsATree()`:

```ts
/** A preview holding all three wizard states and no reboot command. */
function previewWithWizard(wizard: WizardPlan | null, reboot: FirstBootPlan["reboot"]) {
  firstbootPreviewMock.mockResolvedValue({
    tree: "E:\\dist39",
    steps: [],
    fatMount: { kind: "available" },
    userStartupExists: true,
    alreadyWritten: false,
    bytesAdded: 4096,
    reboot,
    wizard,
    inputSetByArt: true,
  } satisfies FirstBootPlan);
}

const ALL_THREE: WizardPlan = {
  rows: [
    { window: "locale", state: "ask" },
    { window: "input", state: "set-by-art" },
    { window: "screen-mode", state: "missing" },
  ],
};
```

Append inside `describe("the first-boot tick", …)`:

```ts
  it("draws the second tick ticked when nothing is remembered, and rendering writes nothing", async () => {
    await renderChoice("AmigaOS 3.9");
    const box = within(screen.getByTestId("choice-firstboot-askprefs")).getByRole("checkbox") as HTMLInputElement;
    expect(box.checked).toBe(true);
    expect(screen.getByTestId("choice-firstboot-askprefs").textContent).toContain(
      i18n.t("osBuilder.choice.askPrefsRow")
    );
    expect(rememberedBag()["buildSession.firstboot"]).toBeUndefined();
  });

  it("unticking the second tick writes askPrefs=false, and it survives a reload", async () => {
    await renderChoice("AmigaOS 3.9");
    await userEvent.click(within(screen.getByTestId("choice-firstboot-askprefs")).getByRole("checkbox"));
    await waitFor(() =>
      expect(rememberedBag()["buildSession.firstboot"]).toMatchObject({ askPrefs: false })
    );
    cleanup();
    render(<ChoiceTab />);
    await screen.findAllByTestId("choice-part-row");
    const again = within(screen.getByTestId("choice-firstboot-askprefs")).getByRole("checkbox") as HTMLInputElement;
    expect(again.checked).toBe(false);
  });

  it("with first boot off, the second tick stays enabled and says it has no effect", async () => {
    seedRemembered({
      ...FULL_FIELDS,
      "buildSession.release": "AmigaOS 3.9",
      "buildSession.firstboot": { written: false, wanted: false },
    });
    render(<ChoiceTab />);
    await screen.findAllByTestId("choice-part-row");
    const box = within(screen.getByTestId("choice-firstboot-askprefs")).getByRole("checkbox") as HTMLInputElement;
    expect(box.disabled).toBe(false);
    expect(screen.getByTestId("choice-firstboot-askprefs-no-effect").textContent).toBe(
      i18n.t("osBuilder.choice.askPrefsNoEffect")
    );
  });

  it("names the windows that will open and why the others will not, without file names", async () => {
    destinationIsATree();
    previewWithWizard(ALL_THREE, { kind: "available" });
    await renderChoice("AmigaOS 3.9");
    const lines = await screen.findByTestId("choice-firstboot-windows");
    expect(lines.textContent).toContain(
      i18n.t("osBuilder.choice.askPrefsAsk", { windows: i18n.t("firstboot.wizard.window.locale") })
    );
    expect(lines.textContent).toContain(
      i18n.t("osBuilder.choice.askPrefsSetByArt", { windows: i18n.t("firstboot.wizard.window.input") })
    );
    expect(lines.textContent).toContain(
      i18n.t("osBuilder.choice.askPrefsMissing", { windows: i18n.t("firstboot.wizard.window.screenMode") })
    );
    expect(lines.textContent).toContain(i18n.t("osBuilder.choice.askPrefsAsOf"));
    expect(lines.textContent).not.toContain("SYS:Prefs");
    expect(screen.queryByTestId("choice-firstboot-windows-files")).toBeNull();
  });

  it("shows the editors' file names in Power mode only", async () => {
    destinationIsATree();
    previewWithWizard(ALL_THREE, { kind: "available" });
    useSettingsStore.setState({
      loaded: true,
      settings: {
        ...DEFAULT_SETTINGS,
        uxMode: "power",
        remembered: { ...FULL_FIELDS, "buildSession.release": "AmigaOS 3.9" },
      },
    });
    render(<ChoiceTab />);
    const files = await screen.findByTestId("choice-firstboot-windows-files");
    expect(files.textContent).toBe("SYS:Prefs/Locale");
  });

  it("asks the preview with the second tick's own value, and draws no windows when it is off", async () => {
    destinationIsATree();
    previewWithWizard(ALL_THREE, { kind: "available" });
    await renderChoice("AmigaOS 3.9");
    await screen.findByTestId("choice-firstboot-windows");
    previewWithWizard(null, { kind: "available" });
    await userEvent.click(within(screen.getByTestId("choice-firstboot-askprefs")).getByRole("checkbox"));
    await waitFor(() => expect(firstbootPreviewMock).toHaveBeenLastCalledWith(expect.any(String), false));
    await waitFor(() => expect(screen.queryByTestId("choice-firstboot-windows")).toBeNull());
  });

  it("names the catalogue's reboot package when the tree has no reboot command", async () => {
    destinationIsATree();
    previewWithWizard(ALL_THREE, { kind: "unavailable", needs: "reboot" });
    await renderChoice("AmigaOS 3.9");
    const said = await screen.findByTestId("choice-firstboot-reboot");
    expect(said.textContent).toBe(i18n.t("firstboot.reboot.unavailable", { needs: "reboot" }));
  });

  it("says nothing about a reboot command the tree has", async () => {
    destinationIsATree();
    previewWithWizard(ALL_THREE, { kind: "available" });
    await renderChoice("AmigaOS 3.9");
    await screen.findByTestId("choice-firstboot-windows");
    expect(screen.queryByTestId("choice-firstboot-reboot")).toBeNull();
  });

  it("for a fresh build, says the windows are known once the tree exists", async () => {
    await renderChoice("AmigaOS 3.9");
    expect(screen.getByTestId("choice-firstboot-windows-after-build").textContent).toBe(
      i18n.t("osBuilder.choice.askPrefsAfterBuild")
    );
    expect(firstbootPreviewMock).not.toHaveBeenCalled();
  });
```

In `describe("in Turkish")`'s test, before `render(<ChoiceTab />);` add `previewWithWizard(ALL_THREE, { kind: "unavailable", needs: "reboot" });`, and after `await screen.findByTestId("choice-update-closed-tick");` add `await screen.findByTestId("choice-firstboot-windows"); await screen.findByTestId("choice-firstboot-reboot");`.

`FirstBootReportPanel.test.tsx` — append:

```tsx
describe("a restart, in its own sentence", () => {
  const asked = { rebootRequestedBy: "15-probe" };

  it("says the Amiga restarted only when a later boot shows it", () => {
    render(<FirstBootReportPanel report={report({ ...asked, restartedAfterRequest: true })} />);
    expect(screen.getByTestId("firstboot-report-reboot").textContent).toBe(
      i18n.t("firstboot.report.reboot.restarted", { step: "15-probe" })
    );
  });

  it("says restart it yourself when this Amiga has no command for it", () => {
    render(<FirstBootReportPanel report={report({ ...asked, rebootUnavailable: true })} />);
    expect(screen.getByTestId("firstboot-report-reboot").textContent).toBe(
      i18n.t("firstboot.report.reboot.unavailable", { step: "15-probe" })
    );
  });

  it("says the log does not show it coming back when neither is known", () => {
    render(<FirstBootReportPanel report={report(asked)} />);
    expect(screen.getByTestId("firstboot-report-reboot").textContent).toBe(
      i18n.t("firstboot.report.reboot.pending", { step: "15-probe" })
    );
  });

  it("says nothing about a restart nobody asked for", () => {
    render(<FirstBootReportPanel report={report()} />);
    expect(screen.queryByTestId("firstboot-report-reboot")).toBeNull();
  });
});

describe("the wizard's own lines", () => {
  const wizardStep = {
    name: "90-prefs",
    outcome: { kind: "ok" as const },
    details: ["opened Locale", "not-asked Input", "missing ScreenMode"],
  };

  it("reads them as sentences in Beginner mode, with no raw line", () => {
    render(<FirstBootReportPanel report={report({ steps: [wizardStep] })} />);
    const said = screen.getByTestId("firstboot-report-wizard").textContent ?? "";
    expect(said).toContain(i18n.t("firstboot.report.wizard.opened", { window: i18n.t("firstboot.wizard.window.locale") }));
    expect(said).toContain(i18n.t("firstboot.report.wizard.notAsked", { window: i18n.t("firstboot.wizard.window.input") }));
    expect(said).toContain(i18n.t("firstboot.report.wizard.missing", { window: i18n.t("firstboot.wizard.window.screenMode") }));
    expect(screen.queryByText("opened Locale")).toBeNull();
  });

  it("does not read another step's details as the wizard's", () => {
    render(
      <FirstBootReportPanel
        report={report({ steps: [{ name: "10-hardware", outcome: { kind: "ok" }, details: ["missing Locale"] }] })}
      />
    );
    expect(screen.queryByTestId("firstboot-report-wizard")).toBeNull();
  });
});
```

`FirstBootRehearse.test.tsx` — append inside `describe("the five rehearsal endings stay five sentences on screen", …)`:

```ts
  it("names the window a timed-out wizard was waiting in, and never the plain timeout beside it", async () => {
    await runRehearsal();
    const outcome: RehearsalOutcome = {
      kind: "timed-out",
      waited: { secs: 120, nanos: 0 },
      report: { ...REPORT, ending: "unfinished", waitingIn: "locale" },
    };
    resolveRehearsal!({ job_id: 42, outcome, copy: "D:/amiga/os39.rehearsal", discarded: false });
    const shown = await screen.findByTestId("firstboot-rehearsal-outcome");
    expect(shown.textContent).toBe(i18n.t("firstboot.rehearsal.outcome.timedOutInLocale", { seconds: 120 }));
    expect(screen.getByTestId("firstboot-rehearsal-next").textContent).toBe(
      i18n.t("firstboot.rehearsal.next.timedOutInLocale")
    );
    expect(screen.getByTestId("firstboot-rehearsal-report").textContent).not.toContain(
      i18n.t("firstboot.rehearsal.outcome.timedOut", { seconds: 120 })
    );
  });
```

- [ ] **Step 2: Run and see it fail**

Run: `pnpm vitest run src/components/osbuilder/ChoiceTab.test.tsx src/components/card/FirstBootReportPanel.test.tsx src/components/osbuilder/FirstBootRehearse.test.tsx > .superpowers/sdd/2026-09-15-firstboot-phase-3/t9.txt 2>&1; echo "exit $?"`
Expected: `exit 1`; the ChoiceTab and report-panel cases fail with `Unable to find an element by: [data-testid="choice-firstboot-askprefs"]` / `"firstboot-report-reboot"` / `"firstboot-report-wizard"`; the three controls (`says nothing about a restart nobody asked for`, `does not read another step's details…`, `says nothing about a reboot command the tree has` — the last fails on its `findByTestId` wait, which is correct until the lines exist); the rehearsal case **passes** already (the mapper is Task 8's) and is kept as the screen-level guard.

- [ ] **Step 3: Implement**

`ChoiceTab.tsx` imports: extend `@/lib/firstboot` to

```ts
import {
  fatMountPhrase,
  firstbootPreview,
  rebootCommandPhrase,
  wizardEditorPath,
  wizardLines,
  wizardWindowPhrase,
  type FatMount,
  type RebootCommand,
  type WizardPlan,
  type WizardWindow,
} from "@/lib/firstboot";
```

and add `import { usePowerMode } from "@/lib/uxmode";`. In `ChoiceTab()`, after `const { t } = useTranslation();` add `const power = usePowerMode();`.

Group 3's state and effect — replace from `const [firstbootFat, setFirstbootFat] = useState<FatMount | null>(null);` through `const firstbootFatPhrase = …;` with (the doc comment above `firstbootFat` stays):

```tsx
  const [firstbootFat, setFirstbootFat] = useState<FatMount | null>(null);
  /** The preview's reboot availability and wizard rows — `null` until it has
   *  answered for this tree, for `firstbootFat`'s reason. */
  const [firstbootReboot, setFirstbootReboot] = useState<RebootCommand | null>(null);
  const [firstbootWizard, setFirstbootWizard] = useState<WizardPlan | null>(null);
  const firstbootWanted = session.firstboot.wanted ?? true;
  /** The second tick (first-boot phase 3 design §5): absent means ticked. */
  const askPrefs = session.firstboot.askPrefs ?? true;
  useEffect(() => {
    if (!treeSettled) return;
    const forget = () => {
      setFirstbootWritten(false);
      setFirstbootFat(null);
      setFirstbootReboot(null);
      setFirstbootWizard(null);
    };
    if (!treeRoot) {
      forget();
      return;
    }
    let cancelled = false;
    firstbootPreview(treeRoot, askPrefs)
      .then((preview) => {
        if (!cancelled) {
          setFirstbootWritten(preview.alreadyWritten);
          setFirstbootFat(preview.fatMount);
          setFirstbootReboot(preview.reboot);
          setFirstbootWizard(preview.wizard);
        }
      })
      .catch(() => {
        if (!cancelled) forget();
      });
    return () => {
      cancelled = true;
    };
  }, [treeRoot, treeSettled, askPrefs]);
  const firstbootFatPhrase = firstbootFat ? fatMountPhrase(firstbootFat) : null;
  const firstbootRebootPhrase = firstbootReboot ? rebootCommandPhrase(firstbootReboot) : null;
  const firstbootWindows = firstbootWizard ? wizardLines(firstbootWizard) : null;
  /** A window's label, never its file name. The one dynamic call site. */
  const windowLabel = (window: WizardWindow) => t(wizardWindowPhrase(window).key);
```

In the group-3 JSX, replace every `session.firstboot.wanted ?? true` with `firstbootWanted`, and after the `{firstbootWanted && firstbootFatPhrase && ( … )}` paragraph (still inside the bordered `div`) insert:

```tsx
          {firstbootWanted && firstbootRebootPhrase && (
            <p data-testid="choice-firstboot-reboot" className="faint" style={{ fontSize: 11, margin: "6px 0 0" }}>
              {t(firstbootRebootPhrase.key, firstbootRebootPhrase.params)}
            </p>
          )}

          {/* **The second tick** (first-boot phase 3 design §5). Absent means
              ticked; only `onChange` writes. Never disabled: with first boot
              off it says it has no effect, because Beginner mode hides and
              nothing here disables a control. */}
          <label
            data-testid="choice-firstboot-askprefs"
            style={{ display: "flex", gap: 8, alignItems: "flex-start", fontSize: 13, marginTop: 8 }}
          >
            <input
              type="checkbox"
              checked={askPrefs}
              onChange={(e) => setFirstBoot({ askPrefs: e.target.checked })}
            />
            <span>{t("osBuilder.choice.askPrefsRow")}</span>
          </label>
          {!firstbootWanted && (
            <p data-testid="choice-firstboot-askprefs-no-effect" className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
              {t("osBuilder.choice.askPrefsNoEffect")}
            </p>
          )}
          {firstbootWanted && askPrefs && treeSettled && !treeRoot && (
            <p data-testid="choice-firstboot-windows-after-build" className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
              {t("osBuilder.choice.askPrefsAfterBuild")}
            </p>
          )}
          {/* What the preview read, and that it read the tree **as it is now**:
              Appearance applied after the build writes its marker then, and
              the Amiga decides on the day (design §5). */}
          {firstbootWanted && askPrefs && firstbootWindows && (
            <div data-testid="choice-firstboot-windows" className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
              {firstbootWindows.ask.length > 0 && (
                <p style={{ margin: 0 }}>
                  {t("osBuilder.choice.askPrefsAsk", { windows: firstbootWindows.ask.map(windowLabel).join(", ") })}
                  {power && (
                    <span data-testid="choice-firstboot-windows-files" style={{ marginLeft: 6 }}>
                      {firstbootWindows.ask.map(wizardEditorPath).join(", ")}
                    </span>
                  )}
                </p>
              )}
              {firstbootWindows.setByArt.length > 0 && (
                <p style={{ margin: 0 }}>
                  {t("osBuilder.choice.askPrefsSetByArt", { windows: firstbootWindows.setByArt.map(windowLabel).join(", ") })}
                </p>
              )}
              {firstbootWindows.missing.length > 0 && (
                <p style={{ margin: 0 }}>
                  {t("osBuilder.choice.askPrefsMissing", { windows: firstbootWindows.missing.map(windowLabel).join(", ") })}
                </p>
              )}
              <p style={{ margin: 0 }}>{t("osBuilder.choice.askPrefsAsOf")}</p>
            </div>
          )}
```

The Power-mode test expects `SYS:Prefs/Locale` alone because only Locale is `ask` in `ALL_THREE`.

`FirstBootReportPanel.tsx` — extend its `@/lib/firstboot` import with `rebootReportPhrase, wizardDetail, wizardWindowPhrase, WIZARD_STEP_NAME, type WizardDetail, type WizardWindow`; after `const sourcePhrase = …;`:

```tsx
  const reboot = rebootReportPhrase(report);
  /** A window's label, never its file name. */
  const windowLabel = (window: WizardWindow) => t(wizardWindowPhrase(window).key);
```

inside the step row's `<td>`, directly after `{t(outcome.key, outcome.params)}`:

```tsx
                    {step.name === WIZARD_STEP_NAME &&
                      (() => {
                        const read = step.details
                          .map(wizardDetail)
                          .filter((d): d is WizardDetail => d !== null);
                        return read.length > 0 ? (
                          <div data-testid="firstboot-report-wizard" className="faint" style={{ fontSize: 11 }}>
                            {read.map((d, i) => (
                              <div key={`${d.window}-${i}`}>{t(d.phrase.key, { window: windowLabel(d.window) })}</div>
                            ))}
                          </div>
                        ) : null;
                      })()}
```

and after the ending paragraph (`data-testid="firstboot-report-ending"`), before the FAT-copy line:

```tsx
      {reboot && (
        <p data-testid="firstboot-report-reboot" style={{ fontSize: 12, margin: "0 0 6px" }}>
          {t(reboot.key, reboot.params)}
        </p>
      )}
```

Add to the file's header comment: `// **A restart is told apart** (first-boot phase 3 design §6): restarted, no command, not back yet — and the wizard's own detail lines read as sentences in both modes, because *which windows opened* is the outcome, not AmigaDOS noise.`

`en.json`, `osBuilder.choice`, keys in alphabetical position:

```json
      "askPrefsAfterBuild": "Which windows open is known once the tree exists.",
      "askPrefsAsk": "The Amiga will open: {{windows}}.",
      "askPrefsAsOf": "Read from the tree as it is now. A screen depth applied on Prepare volumes after the build is set then, and the Amiga skips that window on the day.",
      "askPrefsMissing": "Not asked, because this release has no window for it: {{windows}}.",
      "askPrefsNoEffect": "First boot is off, so this has no effect.",
      "askPrefsRow": "Ask the remaining preferences on the Amiga at first boot",
      "askPrefsSetByArt": "Not asked, because ART set it: {{windows}}.",
```

`tr.json`, same keys:

```json
      "askPrefsAfterBuild": "Hangi pencerelerin açılacağı ağaç oluşunca bilinir.",
      "askPrefsAsk": "Amiga şunları açacak: {{windows}}.",
      "askPrefsAsOf": "Ağacın şu anki hâlinden okundu. Derlemeden sonra Birimleri hazırla sekmesinde uygulanan bir ekran derinliği o zaman ayarlanır ve Amiga o gün o pencereyi atlar.",
      "askPrefsMissing": "Bu sürümde penceresi olmadığı için sorulmayacak: {{windows}}.",
      "askPrefsNoEffect": "İlk açılış kapalı, bu yüzden bunun bir etkisi yok.",
      "askPrefsRow": "Kalan tercihleri ilk açılışta Amiga'da sor",
      "askPrefsSetByArt": "ART ayarladığı için sorulmayacak: {{windows}}.",
```

`literal-keys.test.ts`: change `expect(dynamicCalls).toBe(191);` to `196` and add above it:

```ts
    // 191 → 196 (first-boot phase 3): five. `ChoiceTab.tsx` renders
    // `rebootCommandPhrase` beside the first-boot tick and names each wizard
    // window through one `windowLabel` helper (`t(wizardWindowPhrase(w).key)`);
    // `FirstBootReportPanel.tsx` renders `rebootReportPhrase`, the same helper,
    // and each read detail line (`t(d.phrase.key, { window })`).
    // `phrase-keys.test.ts` resolves every key those mappers return.
```

- [ ] **Step 4: Run and see it pass**

Run the Step 2 command, then `pnpm vitest run src/i18n > …/t9-i18n.txt 2>&1`, then `pnpm lint > …/t9-lint.txt 2>&1; echo "exit $?"`.
Expected: all green, `exit 0`. If `literal-keys` reports a number other than 196, count the new non-literal `t(` sites against the list in the comment — a different number is a different screen from this plan's and must be explained in the comment, not just re-pinned.

- [ ] **Step 5: Mutations**

Back up each component by absolute path; run the named test file; see the named test red; restore with `shutil.copyfile`; green.

1. `ChoiceTab.tsx`: add `useEffect(() => setFirstBoot({ askPrefs: true }), [setFirstBoot]);` → `draws the second tick ticked when nothing is remembered, and rendering writes nothing` red.
2. `ChoiceTab.tsx`: `disabled={!firstbootWanted}` on the second checkbox → `with first boot off, the second tick stays enabled…` red.
3. `ChoiceTab.tsx`: drop `askPrefs &&` from the windows block's condition → `asks the preview with the second tick's own value, and draws no windows when it is off` red.
4. `ChoiceTab.tsx`: drop `power &&` before the files span → `names the windows that will open… without file names` red.
5. `ChoiceTab.tsx`: call `firstbootPreview(treeRoot, true)` → the same `asks the preview…` test red.
6. `FirstBootReportPanel.tsx`: wrap the wizard block in `power &&` → `reads them as sentences in Beginner mode, with no raw line` red.
7. `FirstBootReportPanel.tsx`: drop `step.name === WIZARD_STEP_NAME &&` → `does not read another step's details as the wizard's` red.

- [ ] **Step 6: Commit**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && git branch --show-current
git add src/components/osbuilder/ChoiceTab.tsx src/components/osbuilder/ChoiceTab.test.tsx src/components/card/FirstBootReportPanel.tsx src/components/card/FirstBootReportPanel.test.tsx src/components/osbuilder/FirstBootRehearse.test.tsx src/i18n/en.json src/i18n/tr.json src/i18n/literal-keys.test.ts
git commit -F .superpowers/sdd/2026-09-15-firstboot-phase-3/msg-t9.txt
```

Message: `First boot phase 3: the second tick, its lines, and the report's restart and wizard sentences (task 9)`, the seven mutations, `Co-Authored-By`.

---

### Task 10: Real material — the reboot on 3.2 and 3.9, and the wizard waiting in Locale

**Files:**
- Modify: `src-tauri/src/commands/firstboot.rs` (test module: two helpers and two `#[ignore]`d tests after `rehearse_the_real_tree_when_asked`; that test's doc-comment path)

**Interfaces:**
- Consumes: `stage_with`, `plan`, `write`, `rehearse_with`, `WinUaeLauncher::new`, `RealClock::new`, `RunLimits`, `profile_for`, `REPORT_PATH`, `STEP_DIR` (all already imported in that module or its tests); `FirstBootReport.{reboot_requested_by, reboot_unavailable, restarted_after_request, waiting_in}` (Task 6); `ForegroundWindow` (Task 6).
- Produces: `rehearse_a_reboot_on_the_real_tree_when_asked`, `rehearse_the_wizard_on_the_real_tree_when_asked` — env-gated, printed evidence, never touching the tree they are pointed at.

- [ ] **Step 1: Write the two tests**

In `rehearse_the_real_tree_when_asked`'s doc comment, change `ART_FIRSTBOOT_TREE=E:/amiga/ProjeART/art205-32` to `ART_FIRSTBOOT_TREE=E:/amiga/ProjeART/fb3-trees/art5` (the old tree no longer exists — spec §1.1).

Append to the test module (add `use crate::core::firstboot::ForegroundWindow;` and `use crate::core::firstboot::report::FirstBootReport;` if not already there):

```rust
    /// The three variables every real-material rehearsal reads, or `None`
    /// after printing which are missing.
    fn real_material() -> Option<(PathBuf, PathBuf, PathBuf, AmigaProfile)> {
        match (
            std::env::var("ART_FIRSTBOOT_TREE"),
            std::env::var("ART_FIRSTBOOT_ROM"),
            std::env::var("ART_WINUAE"),
        ) {
            (Ok(tree), Ok(rom), Ok(winuae)) => Some((
                PathBuf::from(tree),
                PathBuf::from(rom),
                PathBuf::from(winuae),
                profile_for(std::env::var("ART_FIRSTBOOT_PROFILE").ok().as_deref()).unwrap(),
            )),
            _ => {
                println!("skipped: set ART_FIRSTBOOT_TREE, ART_FIRSTBOOT_ROM and ART_WINUAE");
                None
            }
        }
    }

    fn report_of(outcome: &RehearsalOutcome) -> &FirstBootReport {
        match outcome {
            RehearsalOutcome::Finished { report }
            | RehearsalOutcome::StepRefused { report }
            | RehearsalOutcome::TimedOut { report, .. }
            | RehearsalOutcome::EmulatorClosed { report, .. }
            | RehearsalOutcome::WroteWithoutStopping { report, .. } => report,
        }
    }

    /// The test-only step that asks for one restart (phase 3 design §8.2: it
    /// lives here, never in `scripts/`). Named `15-` so steps remain after it
    /// and the next boot has to run them.
    const REBOOT_PROBE: &str = "15-reboot-probe";

    /// **Phase 3's reboot, on the owner's own trees** (design §8.2 rows 1–2).
    ///
    /// ```text
    /// ART_FIRSTBOOT_TREE=E:/amiga/ProjeART/fb3-trees/art5 \
    /// ART_FIRSTBOOT_ROM="E:/amiga/Amigatolon/paketler/3.2/AmigaOs 3.2/ROM/kicka1200.rom" \
    /// ART_WINUAE="C:/Program Files/WinUAE/winuae64.exe" ART_FIRSTBOOT_EXPECT_REBOOT=restarted \
    /// cargo test --lib rehearse_a_reboot_on_the_real_tree_when_asked -- --ignored --nocapture
    /// ```
    ///
    /// and for 3.9 `ART_FIRSTBOOT_TREE=E:/amiga/ProjeART/fb3-trees/sonuclar`,
    /// `ART_FIRSTBOOT_ROM="E:/amiga/Amigatolon/kickstart/Kickstart v3.1 rev 40.68 (1993)(Commodore)(A1200).rom"`,
    /// `ART_FIRSTBOOT_EXPECT_REBOOT=unavailable`.
    #[test]
    #[ignore = "opens WinUAE against the owner's own tree and ROM; run explicitly"]
    fn rehearse_a_reboot_on_the_real_tree_when_asked() {
        let Some((tree, rom, winuae, profile)) = real_material() else {
            return;
        };
        let expect = std::env::var("ART_FIRSTBOOT_EXPECT_REBOOT").unwrap_or_default();
        assert!(
            expect == "restarted" || expect == "unavailable",
            "set ART_FIRSTBOOT_EXPECT_REBOOT to restarted (a tree with C/Reboot) or unavailable (one without)"
        );
        let scratch = ScratchDir::new("art-firstboot-real", "reboot");
        let staged = stage_with(&tree, &NoProgress).expect("copying the tree");
        let copy = staged.copy_path().to_path_buf();
        println!("copy: {}", copy.display());
        assert!(
            !copy.join(REPORT_PATH).exists(),
            "the tree already carries S/FirstBoot.log from an earlier boot; the rehearsal would \
             read that boot's ending — point ART_FIRSTBOOT_TREE at a tree that has not booted first boot"
        );

        let planned = plan(&FirstBootRequest {
            tree: copy.clone(),
            ask_prefs: false,
        })
        .expect("planning");
        println!("reboot: {:?}", planned.reboot);
        write(&planned).expect("writing the first boot into the copy");
        std::fs::write(
            copy.join(STEP_DIR).join(REBOOT_PROBE),
            "FailAt 2000000000\nSetEnv ART_Reboot \"TRUE\"\n",
        )
        .unwrap();

        let launcher = WinUaeLauncher::new(&winuae, scratch.path());
        let outcome = rehearse_with(
            &RehearseRequest {
                tree_copy: &copy,
                scratch_root: scratch.path(),
                profile: &profile,
                kickstart_path: &rom,
                limits: RunLimits::default(),
            },
            &launcher,
            &RealClock::new(),
            &NoProgress,
        )
        .expect("the rehearsal itself");

        let log = std::fs::read_to_string(copy.join(REPORT_PATH)).unwrap_or_default();
        println!("--- {REPORT_PATH} ---\n{log}--- as parsed ---\n{outcome:#?}");
        let boots = log.lines().filter(|l| l.starts_with("system ")).count();
        let report = report_of(&outcome);
        assert_eq!(report.reboot_requested_by.as_deref(), Some(REBOOT_PROBE));
        if expect == "restarted" {
            assert_eq!(boots, 2, "a restart is a second boot writing a second system line");
            assert!(report.restarted_after_request && !report.reboot_unavailable);
        } else {
            assert_eq!(boots, 1, "no reboot command, no second boot");
            assert!(report.reboot_unavailable && !report.restarted_after_request);
        }
        let names: Vec<&str> = report.steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["10-hardware", REBOOT_PROBE, "20-aux", "30-datatypes"]);
        assert_eq!(report.ending, Ending::DoneAll);
        assert!(matches!(outcome, RehearsalOutcome::Finished { .. }));
        staged.discard().expect("removing the copy");
        println!("the copy has been discarded");
    }

    /// **The wizard, with nobody at the keyboard** (design §8.2 row 3): the
    /// boot waits in Locale until the deadline, and the ending says so.
    ///
    /// Same variables as above, without `ART_FIRSTBOOT_EXPECT_REBOOT`;
    /// `ART_FIRSTBOOT_DEADLINE_SECS` (default 120) is how long to wait.
    #[test]
    #[ignore = "opens WinUAE against the owner's own tree and ROM; run explicitly"]
    fn rehearse_the_wizard_on_the_real_tree_when_asked() {
        let Some((tree, rom, winuae, profile)) = real_material() else {
            return;
        };
        let deadline = std::env::var("ART_FIRSTBOOT_DEADLINE_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(120);
        let scratch = ScratchDir::new("art-firstboot-real", "wizard");
        let staged = stage_with(&tree, &NoProgress).expect("copying the tree");
        let copy = staged.copy_path().to_path_buf();
        println!("copy: {}", copy.display());
        assert!(!copy.join(REPORT_PATH).exists(), "the tree has booted first boot before");

        let planned = plan(&FirstBootRequest {
            tree: copy.clone(),
            ask_prefs: true,
        })
        .expect("planning");
        println!("wizard: {:?}", planned.wizard);
        write(&planned).expect("writing the first boot into the copy");

        let launcher = WinUaeLauncher::new(&winuae, scratch.path());
        let outcome = rehearse_with(
            &RehearseRequest {
                tree_copy: &copy,
                scratch_root: scratch.path(),
                profile: &profile,
                kickstart_path: &rom,
                limits: RunLimits {
                    deadline: Duration::from_secs(deadline),
                    ..RunLimits::default()
                },
            },
            &launcher,
            &RealClock::new(),
            &NoProgress,
        )
        .expect("the rehearsal itself");

        let log = std::fs::read_to_string(copy.join(REPORT_PATH)).unwrap_or_default();
        println!("--- {REPORT_PATH} ---\n{log}--- as parsed ---\n{outcome:#?}");
        assert!(log.contains("step 90-prefs detail opened Locale"));
        match &outcome {
            RehearsalOutcome::TimedOut { report, .. } => {
                assert_eq!(report.waiting_in, Some(ForegroundWindow::Locale))
            }
            other => panic!("expected the deadline with Locale waiting, got {other:?}"),
        }
        staged.discard().expect("removing the copy");
        println!("the copy has been discarded");
    }
```

- [ ] **Step 2: Compile and confirm they skip without the variables**

Run: `cd /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri && /c/Users/ismoz/.cargo/bin/cargo test --lib rehearse_ -- --ignored > …/t10-skip.txt 2>&1; echo "exit $?"`
Expected: `test result: ok.` with the three `rehearse_…_when_asked` tests passing on their `skipped:` line (no variables set). `cargo clippy --all-targets -- -D warnings > …/t10-clippy.txt 2>&1` → exit 0.

- [ ] **Step 3: Copy the owner's trees onto the experiments disk**

The originals are read only; `stage_with` makes its copy **beside** the tree it is given, so the tests are pointed at copies under `E:\amiga\ProjeART\` rather than at `Amigatolon`:

```bash
test ! -e /e/amiga/ProjeART/fb3-trees && mkdir -p /e/amiga/ProjeART/fb3-trees && cp -r /e/amiga/Amigatolon/os39/art5 /e/amiga/ProjeART/fb3-trees/art5 && cp -r /e/amiga/Amigatolon/sonuclar /e/amiga/ProjeART/fb3-trees/sonuclar; echo "exit $?"
```

Expected: `exit 0`, and `ls /e/amiga/ProjeART/fb3-trees` shows `art5 sonuclar`. If `fb3-trees` already exists, stop and ask — do not overwrite.

- [ ] **Step 4: Run the three proofs, one at a time**

No WinUAE of anyone else's may be running (`tasklist //FI "IMAGENAME eq winuae64.exe"` shows none). Each run writes its whole output to a file; read only the `test result:` line, the `system`/`step`/`reboot`/`done` lines and the parsed outcome.

1. 3.2, reboot:
   `cd /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri && ART_FIRSTBOOT_TREE=E:/amiga/ProjeART/fb3-trees/art5 ART_FIRSTBOOT_ROM="E:/amiga/Amigatolon/paketler/3.2/AmigaOs 3.2/ROM/kicka1200.rom" ART_WINUAE="C:/Program Files/WinUAE/winuae64.exe" ART_FIRSTBOOT_EXPECT_REBOOT=restarted /c/Users/ismoz/.cargo/bin/cargo test --lib rehearse_a_reboot_on_the_real_tree_when_asked -- --ignored --nocapture > /d/Projeler/Amiga/amiga-retro-toolkit/.superpowers/sdd/2026-09-15-firstboot-phase-3/real-reboot-32.txt 2>&1; echo "exit $?"`
   Expected: `test result: ok. 1 passed`; two `system UAE none kick 3.2` lines, `reboot requested by 15-reboot-probe`, `done all`, `Finished`.
2. 3.9, reboot unavailable: the same command with `ART_FIRSTBOOT_TREE=E:/amiga/ProjeART/fb3-trees/sonuclar`, `ART_FIRSTBOOT_ROM="E:/amiga/Amigatolon/kickstart/Kickstart v3.1 rev 40.68 (1993)(Commodore)(A1200).rom"`, `ART_FIRSTBOOT_EXPECT_REBOOT=unavailable`, output `real-reboot-39.txt`.
   Expected: `test result: ok. 1 passed`; one `system UAE none kick 3.9` line, `reboot requested by 15-reboot-probe`, `reboot unavailable`, `done all`.
3. The wizard on 3.2: `rehearse_the_wizard_on_the_real_tree_when_asked`, art5 variables without `ART_FIRSTBOOT_EXPECT_REBOOT`, output `real-wizard-32.txt`; then the same on `sonuclar` with the 3.1 ROM, output `real-wizard-39.txt`.
   Expected each: `test result: ok. 1 passed`; `step 90-prefs detail opened Locale` as the log's last `step` line and `TimedOut` with `waiting_in: Some(Locale)`.

A red run is a finding, not a test to adjust: keep its copy (the test prints where), record the lines in `.superpowers/sdd/2026-09-15-firstboot-phase-3/phase-3-report.md`, and if ART is at fault open an `ART-NNN` in Task 11. Never loosen an assertion to make the owner's tree pass.

- [ ] **Step 5: Commit**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && git branch --show-current
git add src-tauri/src/commands/firstboot.rs
git commit -F .superpowers/sdd/2026-09-15-firstboot-phase-3/msg-t10.txt
```

Message: `First boot phase 3: gated rehearsals of the reboot and the wizard on real trees (task 10)`, quoting each run's `test result:` line and its `system`/`reboot`/`done` lines, `Co-Authored-By`.

---

### Task 11: Documents, the whole suite twice, the sweeps

**Files:**
- Modify: `docs/superpowers/specs/2026-09-07-firstboot-design.md` (§4.2 item 4, §5.1, §5.3 — dated corrections in place)
- Modify: `docs/STATUS.md` (the reproduce block's rehearsal command at the `rehearse_the_real_tree_when_asked` lines; `## Snapshot` numbers; `### Start here` under `## Picking up next session`, updated in place)
- Modify: `docs/FEATURES.md` (the row `First boot on the Amiga (framework + hardware step)`)
- Modify: `CHANGELOG.md` (`## [Unreleased]`)
- Modify: `docs/session-log.md` (a row at the top of the table)
- Modify: `docs/ISSUES.md` — **only** for a defect found in Tasks 1–10 (next free id after ART-331)
- Create: `.superpowers/sdd/2026-09-15-firstboot-phase-3/phase-3-report.md` (git-ignored task report)

**Interfaces:**
- Consumes: every task's commit and the result lines Task 10 recorded.
- Produces: documents that describe the tree as it now is; the round's report with the mutation table.

- [ ] **Step 1: The whole suite, twice, and every gate CI runs**

From the repository root, each to its own file, reading only the summary lines:

```bash
pnpm lint > .superpowers/sdd/2026-09-15-firstboot-phase-3/close-lint.txt 2>&1; echo "exit $?"
pnpm test > .superpowers/sdd/2026-09-15-firstboot-phase-3/close-vitest.txt 2>&1; echo "exit $?"
cd src-tauri && /c/Users/ismoz/.cargo/bin/cargo fmt --check > ../.superpowers/sdd/2026-09-15-firstboot-phase-3/close-fmt.txt 2>&1; echo "exit $?"
cd /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri && /c/Users/ismoz/.cargo/bin/cargo clippy --all-targets -- -D warnings > ../.superpowers/sdd/2026-09-15-firstboot-phase-3/close-clippy.txt 2>&1; echo "exit $?"
cd /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri && /c/Users/ismoz/.cargo/bin/cargo test --lib > ../.superpowers/sdd/2026-09-15-firstboot-phase-3/close-cargo-1.txt 2>&1; echo "exit $?"
cd /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri && /c/Users/ismoz/.cargo/bin/cargo test --lib > ../.superpowers/sdd/2026-09-15-firstboot-phase-3/close-cargo-2.txt 2>&1; echo "exit $?"
cd /d/Projeler/Amiga/amiga-retro-toolkit && python scripts/control-byte-sweep.py > .superpowers/sdd/2026-09-15-firstboot-phase-3/close-control.txt 2>&1; echo "exit $?"
python scripts/scratch-root-sweep.py > .superpowers/sdd/2026-09-15-firstboot-phase-3/close-scratch-root.txt 2>&1; echo "exit $?"
python scripts/scratch-guard-sweep.py > .superpowers/sdd/2026-09-15-firstboot-phase-3/close-scratch-guard.txt 2>&1; echo "exit $?"
python scripts/contrast-check.py --quiet > .superpowers/sdd/2026-09-15-firstboot-phase-3/close-contrast.txt 2>&1; echo "exit $?"
```

Expected: every `exit 0`; Vitest's `Test Files … passed` and `Tests … passed` with 0 failed; **both** cargo files carry a `test result: ok.` line (a run without one did not finish, whatever its exit code — ART-261) with the same passed count. Quote those lines in the report.

Turkish overflow (new Turkish UI strings on tab 2): in a second terminal `pnpm dev`, wait for Vite's `Local:` line, then `python scripts/tr-overflow-check.py > .superpowers/sdd/2026-09-15-firstboot-phase-3/close-tr-overflow.txt 2>&1; echo "exit $?"`, then stop `pnpm dev`. Expected: 0 Turkish-only hits. The script reaches only screens with no backend; the second tick draws without a tree, the windows line needs one — say in the report that the windows line was not measured by it.

- [ ] **Step 2: Correct the 2026-09-07 spec in place**

After §4.2 item 4's last correction paragraph (the one ending "…the same way phase 2 checks for `C:Sort`."), add:

```markdown
   **Correction, 2026-09-15 (phase 3):** the step wrapper now carries the
   reboot out — clear `ART_Reboot`, log `reboot requested by <name>`, then
   `C:Wait 3` and `C:Reboot` when the tree has `C/Reboot`, else log `reboot
   unavailable` and carry on. Plain `Reboot` on PiStorm too; `EMU68INFO
   HARDRESET` is out of scope. The tree names above no longer exist: the
   measured trees are `E:\amiga\Amigatolon\os39\art5` (AmigaOS 3.2, `C/Reboot`
   present), `os39\art1` and `sonuclar` (3.9, absent). See
   [the phase-3 design](2026-09-15-firstboot-phase-3-design.md) §3.3.
```

Under §5.1's existing correction, add:

```markdown
**Correction, 2026-09-15 (phase 3):** superseded, and §5.2 with it. Measured
under WinUAE, a ScreenMode change applies on the same boot with no reboot
(experiment 4), the 3.2 tree has no `WBStartup/`, and a foreground Prefs
editor holds the boot until answered (experiment 3). `90-prefs` is **fixed**
text: Locale and Input in the foreground, ScreenMode through `Run`, each
skipped when ART set it — decided on the Amiga from `ENVARC:ART_Set_Input` /
`ART_Set_ScreenMode`. `ART-FirstBootWB` was never built. See the phase-3
design §1.4 and §3.2.
```

Under §5.3's table paragraph, add:

```markdown
**Correction, 2026-09-15 (phase 3):** the build-session facade's
`written: { keymap, screenmode }` was never built and cannot work — the
Appearance panel is applied after the build's first-boot write. "Answered" is
now two marker files written by whoever set the preference (the first-boot
write from `S/User-Startup`'s `keymap-selection` block; Appearance with the
depth), and the Amiga decides. `AskPrefs` became one tick,
`buildSession.firstboot.askPrefs`. See the phase-3 design §4.
```

Run `python scripts/control-byte-sweep.py` again; `exit 0`.

- [ ] **Step 3: STATUS.md, FEATURES.md, CHANGELOG, session log**

- `STATUS.md`, reproduce block: in the `rehearse_the_real_tree_when_asked` command replace `E:/amiga/ProjeART/art205-32` with `E:/amiga/ProjeART/fb3-trees/art5`, and add beneath it the two Task 10 commands (reboot on 3.2 with `EXPECT_REBOOT=restarted`, on 3.9 `sonuclar` with the 3.1 ROM and `unavailable`; the wizard with `ART_FIRSTBOOT_DEADLINE_SECS`), each with a one-line comment saying what it proves and that it stages beside the tree it is given, so it is pointed at copies under `E:\amiga\ProjeART\fb3-trees\`.
- `STATUS.md`, `## Snapshot`: the Vitest file/test counts and the Rust passed count from Step 1.
- `STATUS.md`, `### Start here (2026-09-15)`: rewrite its opening paragraph **in place** (do not stack a block): first boot phase 3 is on `art-firstboot-phase-3`, not merged; what landed (the wizard, the reboot, the second tick); the four Task 10 results with their lines; what is owed — the owner's closing measurement (Task 12) and the owner's ruling on the amitools finding (Step 4).
- `FEATURES.md`, the first-boot row: replace the sentence *"the reboot request is recorded (`ENV:ART_Reboot`) but phase 2 does not itself reboot — that command is phase 3's"* and the clause *"phases 3 (the reboot itself, the wizard's `90-prefs`) … are not built"* with a dated 2026-09-15 paragraph: the wrapper carries a reboot out (`C:Wait 3` + `C:Reboot`, or `reboot unavailable`), `90-prefs` asks Locale/Input/ScreenMode unless ART set them, the second tick, the guards by test name (`the_wrapper_waits_before_it_reboots`, `the_wizard_opens_locale_and_input_in_the_foreground_and_screenmode_detached`, `a_keymap_block_is_input_set_by_art_before_any_marker_exists`, `unticking_and_writing_again_removes_arts_own_wizard_and_nothing_else`, `a_screen_depth_writes_the_screenmode_marker_in_the_same_outcome`, `a_reboot_mid_run_is_not_an_ending_and_the_next_boots_done_all_finishes`, `a_wizard_waiting_in_locale_is_named_while_polling_and_at_the_deadline`, `draws the second tick ticked when nothing is remembered, and rendering writes nothing`) and the Task 10 measurements. **The row stays 🟡**, and says why: the interactive editor's own Save was not measured, nor PFS3 or a real card under a reboot, nor `HARDRESET` (spec §1.3 "Not measured", §8.3).
- `CHANGELOG.md`, under `## [Unreleased]`:

```markdown
### Added

- **First boot asks, on the Amiga, only the preferences ART did not set.** A second tick on the OS Builder's *What to
  install* tab — on unless you turn it off — has first boot open the language-and-country window, the keyboard window
  unless you chose a keymap, and the screen-mode window unless you applied a screen depth. The boot waits for the first
  two; the screen-mode window opens on the Workbench. Beside the tick, ART says which windows will open on the tree as
  it is now. Rehearsed under WinUAE; saving from the windows by hand has not yet been measured.
- **A first-boot step can restart the Amiga.** The Amiga waits three seconds for its disk and restarts, and the rest of
  first boot runs on the next boot. AmigaOS 3.9 ships no restart command: first boot then logs that it could not, and
  carries on; the tab names the Aminet package that supplies one.

### Changed

- The first-boot report says when a step asked for a restart, and whether the Amiga restarted, had no command for it,
  or has not come back yet. A rehearsal that runs out of time while the Amiga waits in a preferences window names that
  window.
```

- `session-log.md`: a new first row under `|---|---|---|` — `| 2026-09-15 | **First boot phase 3 — the wizard and the reboot**, on `art-firstboot-phase-3` (not merged). … | <Vitest tests> Vitest · <Rust passed> Rust |`, naming the four Task 10 results and the owed owner measurement.

- [ ] **Step 4: ISSUES, and the finding that is the owner's to rule on**

- A defect found in Tasks 1–10 gets `ART-332` onward under `## Open` (or `## Fixed` with its test name if it was fixed in the round), per `## Adding an entry`.
- **Do not number the amitools finding.** Spec §9 hands it to the owner. Put this proposed wording in `phase-3-report.md` under "For the owner" and in STATUS's *Start here* as an open question, marked *needs the owner's ruling*:

  > **amitools `xdftool`-formatted FFS images fail the Kickstart validator after an unclean reboot, AmigaOS's own `Format` does not.** Measured 2026-09-15 under WinUAE on 3.2 and 3.1 ROMs: after a reboot with an FFS write in flight, an `xdftool … format ffs` image showed *"Error validating … Block 1146049281 out of range"* 8 of 8 times (`0x444F5301`, the boot block's `DOS\1` read as a block number); an image made by `SYS:System/Format … FFS QUICK` on the same geometry, 0 of 6. ART's own `core/volume` formatter was not tried. It matters wherever ART's test tooling builds an FFS image with amitools and boots it. *Proposed next step:* the same no-wait reboot arm against an image formatted by `core/volume`, before anything is filed.

- [ ] **Step 5: The round's report**

Write `.superpowers/sdd/2026-09-15-firstboot-phase-3/phase-3-report.md`: per task the commit hash, the mutation table (task · what was mutated · the test that went red · survivors, and for a survivor whether it was a weak guard or a wrong mutation), the Step 1 summary lines, the Task 10 lines, the Turkish-overflow result and its blind spot, and "For the owner".

- [ ] **Step 6: Commit the documents**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && git branch --show-current
git add docs/superpowers/specs/2026-09-07-firstboot-design.md docs/STATUS.md docs/FEATURES.md CHANGELOG.md docs/session-log.md
git commit -F .superpowers/sdd/2026-09-15-firstboot-phase-3/msg-t11.txt
```

(add `docs/ISSUES.md` by name only if Step 4 changed it). Message: `docs: first boot phase 3 — corrections, status, features, changelog (task 11)`, the two cargo `test result:` lines and the Vitest line, `Co-Authored-By`.

---

### Task 12: The owner's closing measurement — a hand-off, not a claim

**Files:** none changed by an agent.

**Interfaces:**
- Consumes: the built branch; the Task 10 commands.
- Produces: the owner's own account, which alone can move the FEATURES row from 🟡.

- [ ] **Step 1: Hand the owner the measurement, in Turkish, and stop**

Spec §8.3. The controller gives the owner, in one message: on 3.2 (`fb3-trees/art5`) and on 3.9 (`fb3-trees/sonuclar`), write first boot with the second tick on from the OS Builder (or run `rehearse_the_wizard_on_the_real_tree_when_asked` with `ART_FIRSTBOOT_DEADLINE_SECS=900`) and **answer the windows by hand** in the WinUAE window. Three things to report:

1. the windows that opened are exactly the rows the *What to install* tab listed for that tree;
2. `S/FirstBoot.log` in the copy and the report on screen say the same — `opened`/`not-asked`/`missing` per window, then `ok` and `done all`;
3. a ScreenMode chosen and **Saved** in the editor changes the Workbench screen **without a reboot**.

- [ ] **Step 2: Record what the owner says, as the owner said it**

No agent marks this done. When the owner reports, the controller writes the answer into FEATURES (the 🟡 → ✅ decision is the owner's), STATUS's *Start here*, and the session log, and commits with a message file. If a window opened that the preview did not list, or the Save needed a reboot, that is a defect: `ART-NNN` under `## Open`, and the row stays 🟡.

---

## Self-review

### Spec coverage, section by section

| Spec section | Covered by |
|---|---|
| §0 The decision | Tasks 2 (Amiga side), 3–5 (markers and plan), 9 (screen) |
| §1 What was measured, §1.4 overturned | Deviations 1, 4, 11; Task 11 Step 2 (older spec corrected) |
| §2 Decisions 1–7 | 1–3: Tasks 2, 10; 4: Task 3 (`RebootCommand`), Task 9 (line); 5: Tasks 8–9; 6: Tasks 1, 3–5; 7: Tasks 3–4 |
| §3.1 Files | Task 1 (marker names), Task 2 (`90-prefs`, wrapper), Task 4 (`ART_Set_Input`), Task 5 (`ART_Set_ScreenMode`) |
| §3.2 `90-prefs` | Task 2 (text and six guards) |
| §3.3 The reboot | Task 2 (verbatim block, three ordering guards), Task 7 (not an ending), Task 10 (3.2 and 3.9) |
| §3.4 `C/` requirements | Task 2 (`NEEDED_COMMANDS` + `Wait`, refusal by name) |
| §4.1 Names in one place | Task 1 |
| §4.2 Appearance marker | Task 5 |
| §4.3 Input marker from the block | Task 1 (`has_block`), Task 3 (plan reads it), Task 4 (write/remove) |
| §4.4 Plan, preview, write | Task 3 (`ask_prefs`, `reboot`, `wizard`, steps, bytes), Task 4 (removal, twice byte-identical), Task 8 (commands) |
| §5 The screen | Task 8 (session guard, `useBuildRun`), Task 9 (tick, lines, reboot line, Beginner, both catalogues) |
| §6 The report | Task 6 (`reboot_unavailable`, `restarted_after_request`, two-boot log), Task 8 (phrases), Task 9 (panel sentences, wizard details) |
| §7 The rehearsal | Task 7 (comment, poll through reboot, waiting message), Task 8 (timed-out-in-window phrases), Task 9 (rehearsal screen test), Task 10 (wizard on real trees) |
| §8.1 Tests by layer | Tasks 1–9, each with its mutations; sweeps in Task 2 Step 5 and Task 11 Step 1 |
| §8.2 Real material | Task 10 |
| §8.3 Closing measurement | Task 12 |
| §9 Out of scope, the amitools finding | Global Constraints (no `HARDRESET`, no PFS3 claim); Task 11 Step 4 (owner's ruling) |
| §10 Documents | Task 11; CLAUDE.md is the owner's (deviation 10) |

### Placeholder scan

Searched the plan for `TBD`, `TODO`, `implement later`, `fill in`, `similar to Task`, `appropriate`, `edge cases`. The only deliberate gap is the eighth hash in `every_fixed_file_hash_is_pinned`, which cannot be known before the script exists: Task 2 Step 4 names the procedure (paste the two `left` values that differ, after reviewing the script diff), the same pinning step the phase 1–2 plan used.

### Names and types across tasks

`WizardWindow` (Tasks 3, 6, 8), `ForegroundWindow` (Tasks 6, 7, 8, 10), `WindowState`/`WizardRow`/`WizardPlan`/`RebootCommand`/`REBOOT_PACKAGE` (Tasks 3, 8), `FirstBootPlan.input_set_by_art` ↔ `inputSetByArt` (Tasks 3, 4, 8), `Written.removed` ↔ `removed` (Tasks 4, 8), `FirstBootReport.{reboot_unavailable, restarted_after_request, waiting_in}` ↔ `{rebootUnavailable, restartedAfterRequest, waitingIn}` (Tasks 6–10), `WIZARD_STEP` ↔ `WIZARD_STEP_NAME` pinned by `firstboot.test.ts` (Tasks 2, 8, 9), `env::ART_SET_INPUT_PATH`/`ART_SET_SCREENMODE_PATH`/`MARKER_VALUE` (Tasks 1, 3, 4, 5), `Phase.askPrefs` ↔ `SequenceInputs.firstBootAskPrefs` (Tasks 8, 9), `firstbootPreview(tree, askPrefs)`/`firstbootWrite(tree, askPrefs)` (Tasks 8, 9) — checked against each other.
