# Debt round 4 — ART-301, job titles in the user's language — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The job bar names every background job in the chosen language: a Rust job carries a catalogue key and its values (`JobTitle`) instead of an English sentence, and `JobBar.tsx` renders it with `t()`.

**Architecture:** `core/jobs` gains a `JobTitle` type, serialised to exactly the TypeScript `Phrase` shape. `JobProgress.title` changes type from `String` to `JobTitle`. `commands/jobs.rs` stores it. All 40 title sites in 19 command files move to `JobTitle::new("components.jobBar.title.…")`. A migration bridge (`From<&str>`) keeps every intermediate commit compiling, and Task 7 deletes it. On the TypeScript side a closed list `JOB_TITLE_KEYS` in `src/lib/jobs.ts` names the keys, and `src/i18n/job-title-keys.test.ts` reads the Rust tree and holds the list and the Rust sites to each other in both directions.

**Tech Stack:** Rust (serde, serde_json in tests), React + TypeScript, react-i18next, Vitest (+ jsdom for `*.test.tsx`).

**Spec:** `docs/ISSUES.md` § Open, ART-301 (filed 2026-09-10). The research is in `D:\Projeler\Amiga\scratch-0913\plan-draft-records.md`, section "ART-301 research (Explore agent, 2026-09-14)". Debt plan 1 copies it to `docs/superpowers/notes/2026-09-14-debt-round-research.md`. Every line number below was re-measured against the tree on 2026-09-14 while writing this plan.

## Global Constraints

- Branch `art-debt-0914`. Run `git branch --show-current` before every commit; it must print `art-debt-0914`. Debt plans 2, 3 and 5 share this branch.
- **Match by quoted text, not by line number.** Line numbers are from 2026-09-14, before the other debt plans ran. When a quote is not found, re-grep for it; never edit a line by its number alone.
- Stage named files only (`git add <path> …`), never `git add -A`: a background agent's mutation could be committed with it.
- Write commit messages with the Write tool to `D:\Projeler\Amiga\scratch-0913\commit-msg-art301.txt` and commit with `git commit -F` that file. Every message ends with:
  ```
  Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01KZGHnMHaXd1F9eYT5G6ieE
  ```
- Nothing is written to `C:`. Before any `cargo test`, run `export TMP='E:\amiga\ProjeART\tmp' TEMP='E:\amiga\ProjeART\tmp'`, and create the folder first if it is missing. STATUS records every recent suite run with `TMP`/`TEMP` on `E:`.
- Never pipe a command that can fail. A Rust run counts as finished only when it prints `test result:`. A Vitest run counts as finished only when it prints `Test Files`.
- To mutate a file, first back it up to `D:\Projeler\Amiga\scratch-0913\art301-bak\` by absolute path. Restore it with `cp` from that backup. Never use `git checkout --`, and never `mv`.
- Two string catalogues change in the same commit (`src/i18n/en.json`, `src/i18n/tr.json`). `src/lib` never renders a string.
- **Out of scope, stays English on purpose:**
  - `CoreError` messages under a failed job (ART-060).
  - The operation log's record names, `user_operation("…")`, which go to `operations.jsonl`, not to the screen.
  - The progress `message`, which is a file name.
- **Values are never translated.** Paths, package names and releases pass through as given. A count always goes in as `count`, so i18next picks `_one` / `_other`.
- **Turkish: never attach a case suffix to a placeholder.** The suffix's vowel harmony depends on the value. Use postpositions instead, as the existing catalogue does (`içine`, `içinden`, `üzerinde`, `için`).
- The evidence log `D:\Projeler\Amiga\scratch-0913\art301-evidence.md` collects every red line and every mutation result as it is seen. Task 8 copies it into ISSUES.

## Decisions (settled here, with the reason)

1. **`JobProgress.title` changes type; no field is added.** An added field would keep the English sentence on the wire and in every consumer.
   - `job_list` returns the same struct, so no command signature changes and no `invoke_handler![]` entry is needed.
   - Nothing on the Rust side deserialises `JobProgress` (grep 2026-09-14).
   - The only TypeScript reader of `title` is `src/components/JobBar.tsx:148` (grep `\.title\b` over `src/`, 2026-09-14).
   - Test fixtures that build a `JobProgress` change with it (Task 2 lists them).
2. **The key is private, and `JobTitle::new` takes `&'static str`.** No key can be assembled at run time. The field is a `Cow<'static, str>` only so the type can still derive `Deserialize`.
3. **`text(name, &dyn Display)`.** A path goes in as `&path.display()`, a `String` as `&s`, a number as `&n`. A trait object rather than a generic, so clippy's generic-argument borrow lints cannot fire under `-D warnings`.
4. **A closed TypeScript list rather than scanning `.rs` in `dead-keys.test.ts`.**
   - `JOB_TITLE_KEYS` writes every key out quoted, so `dead-keys.test.ts` already counts the keys as reachable.
   - It types the fixtures.
   - `job-title-keys.test.ts` reads the Rust sources themselves ("a test that reads a table instead of the file is a copy", CLAUDE.md). The precedent is `recipe-component-keys.test.ts`, which reads Rust-tree data files with `readFileSync` and checks both catalogues and the reverse direction.
5. **Migration bridge.**
   - Until Task 7, `spawn_job` / `spawn_job_in_lane` take `impl Into<JobTitle>`, and `commands/jobs.rs` holds `From<&str>` and `From<&String>`. The 40 sites can move in four compiling batches.
   - Task 7 deletes the bridge, so the compiler proves no site was missed.
   - **Between Task 1 and Task 7 the branch is not shippable.** A bridged title reaches the bar as a "key" that is an English sentence, and i18next may mangle one that contains `:`. No package is built from an intermediate commit.
6. **One key per sentence, shared by sites that say the same thing.** For example, `copyOutOf` serves four "Copying out of …" sites. This gives 36 keys for 40 sites. Eleven of them count, so each catalogue gains 47 leaves.
7. **The English sentences keep today's wording, with two exceptions.** Counts become real plurals instead of `(s)`. `Syncing the Aminet catalog` becomes `Syncing the Aminet catalogue`, matching `Refreshing the catalogue`: the owner's ruling of 2026-09-14. The Turkish `Aminet kataloğu eşitleniyor` needs no change. A grep of the repository on 2026-09-14 found the old title quoted only in `src-tauri/src/commands/sources.rs:488`, which C1 replaces, and in this plan. No test and no file under `docs/` quotes it. `docs/FEATURES.md:399`'s "Aminet catalog sync / search / fetch" is a feature name, not the job title, and stays.

## The 40 sites and their keys

Every key is `components.jobBar.title.<key>`. `Today` is the exact English the site builds on 2026-09-14.

| # | Site (2026-09-14) | Today | Key | Values |
|---|---|---|---|---|
| A1 | `commands/archive.rs:301` | `Copying out of {file}` | `copyOutOf` | `source` = `file.display()` |
| A2 | `commands/archive.rs:431` | `Copying an archive into {image}` | `copyArchiveInto` | `target` = `image.display()` |
| A3 | `commands/cbm.rs:409` | `Copying out of {file}` | `copyOutOf` | `source` = `file.display()` |
| A4 | `commands/iso.rs:300` | `Copying out of {iso_path}` | `copyOutOf` | `source` = `iso_path.display()` |
| A5 | `commands/iso.rs:410` | `Copying a disc into {image}` | `copyDiscInto` | `target` = `image.display()` |
| A6 | `commands/panel.rs:263` | `Counting {key}` | `countFolder` | `path` = `key` |
| A7 | `commands/panel.rs:329` | `Deleting {n} item(s) from {dir}` | `deleteItems` | `count` = `names.len()`, `source` = `dir.display()` |
| A8 | `commands/volume.rs:127` | `Counting block {block} in {entry.name}` | `countVolumeBlock` | `block` = `block`, `volume` = `entry.name` |
| A9 | `commands/volume_write.rs:1207` | `Copying into {image}` | `copyInto` | `target` = `image.display()` |
| A10 | `commands/volume_write.rs:1328` | `Copying a selection into {image}` | `copySelectionInto` | `target` = `image.display()` |
| A11 | `commands/volume_write.rs:1685` | `Copying out of {image}` | `copyOutOf` | `source` = `image.display()` |
| A12 | `commands/volume_write.rs:1791` | `Copying a selection out of {image}` | `copySelectionOutOf` | `source` = `image.display()` |
| A13 | `commands/volume_write.rs:1943` | `Copying a selection between {from_path} and {to_path}` | `copySelectionBetween` | `source` = `from_path`, `target` = `to_path` |
| B1 | `commands/osinstall.rs:486` | `Identifying media in {folder}` | `identifyMedia` | `source` = `folder.display()` |
| B2 | `commands/osinstall.rs:2291` | `Previewing {n} component(s) of {plan.release}` | `previewComponents` | `count` = `components.len()`, `release` = `plan.release` |
| B3 | `commands/osinstall.rs:2422` | `Previewing {n} package(s) against {tree_root}` | `previewPackages` | `count` = `ordered.len()`, `target` = `tree_root.display()` |
| B4 | `commands/osinstall.rs:2635` | `Adding {n} package(s) to {for_log}` | `addPackages` | `count` = `resolved.len()`, `target` = `for_log` |
| B5 | `commands/osinstall.rs:2780` | `Installing {release} into {destination}` | `installRelease` | `release` = `request.plan.release`, `target` = `destination` |
| B6 | `commands/amigainstall.rs:1157` | `Installing {package_name} on the Amiga` | `installOnAmiga` | `name` = `composed.package_name` |
| B7 | `commands/firstboot.rs:306` | `Rehearsing the first boot on {profile.name}` | `rehearseFirstBoot` | `machine` = `profile.name` |
| B8 | `commands/appearance.rs:328` (fixed) | `Applying appearance to distribution tree` | `applyAppearance` | — |
| B9 | `commands/preload.rs:607` | `Preparing {n} volume(s) on {image}` | `preparePartitions` | `count` = `made.formats()`, `target` = `image` |
| B10 | `commands/card.rs:808` | `Building {dest}` | `buildCard` | `target` = `dest` |
| C1 | `commands/sources.rs:488` (fixed) | `Syncing the Aminet catalog` | `syncAminet` | — (the English sentence becomes `Syncing the Aminet catalogue`, Decision 7) |
| C2 | `commands/sources.rs:552` | `Downloading {meta.name}` | `downloadPackage` | `name` = `meta.name` |
| C3 | `commands/sources.rs:726` | `Installing {file name}` | `installArchive` | `name` = the file name expression |
| C4 | `commands/sources.rs:812` | `Reading {meta.name}` | `readReadme` | `name` = `meta.name` |
| C5 | `commands/sources.rs:963` | `Installing {file name} into {image_path}` | `installArchiveInto` | `name` = the file name expression, `target` = `image_path.display()` |
| C6 | `commands/bundles.rs:166` | `Downloading {n} package{s}` | `downloadPackages` | `count` = `entries.len()` |
| C7 | `commands/archives.rs:137` | `Planning {n} archives` | `planArchives` | `count` = `archives.len()` |
| C8 | `commands/archives.rs:231` | `Installing {n} archives into {image_path}` | `installArchivesInto` | `count` = `archives.len()`, `target` = `image_path.display()` |
| C9 | `commands/whdload.rs:169` | `Installing {file name}` | `installArchive` | `name` = the file name expression |
| C10 | `commands/layout.rs:67` | `Working out what {n} sources need` | `planLayout` | `count` = `request.paths.len()` |
| C11 | `commands/layout.rs:134` | `Laying {n} item(s) out in {root}` | `applyLayout` | `count` = `plan.items.len()`, `target` = `root` |
| D1 | `commands/artwork.rs:191` (fixed) | `Fetching artwork` | `fetchArtwork` | — |
| D2 | `commands/artwork.rs:392` (fixed) | `Reading pictures from your files` | `readLocalPictures` | — |
| D3 | `commands/artwork.rs:455` (fixed) | `Restoring your own pictures` | `restorePictures` | — |
| D4 | `commands/gameindex.rs:246` (fixed) | `Refreshing the catalogue` | `refreshCatalogue` | — |
| D5 | `commands/gameindex.rs:509` (fixed) | `Indexing titles` | `indexTitles` | — |
| D6 | `commands/gameindex.rs:616` | `Writing igame.data for {n} title(s)` | `writeIgameData` | `count` = `plan.items.len()` |

In every one of the 40 functions, the `title` variable has exactly one other use: the `&title` argument of the spawn call right after it (grep of `title` over the 18 files, 2026-09-14). Moving the value into the call is safe.

---

### Task 1: `JobTitle` in the core, and the job runner stores it

**Files:**
- Modify: `src-tauri/src/core/jobs/mod.rs:18-21` (imports), `:68-82` (`JobProgress`), after `:66` (new type), tests `:152-239`
- Modify: `src-tauri/src/commands/jobs.rs:20`, `:48-72`, `:204-259`, tests `:322-518`

**Interfaces:**
- Produces (used by every later task):
  - `crate::core::jobs::JobTitle`
  - `JobTitle::new(key: &'static str) -> JobTitle`
  - `.text(self, name: &'static str, value: &dyn std::fmt::Display) -> JobTitle`
  - `.count(self, n: usize) -> JobTitle`
  - `.key(&self) -> &str`
  - `.params(&self) -> &BTreeMap<String, JobParam>`
  - `crate::core::jobs::JobParam::{Count(u64), Text(String)}`
  - `JobProgress.title: JobTitle`
  - Serialised form: `{"key": "…", "params": {…}}`, with `params` omitted when empty.
  - `spawn_job(app, registry, title: impl Into<JobTitle>, work)` and `spawn_job_in_lane(app, registry, title: impl Into<JobTitle>, lane, work)`. Task 7 narrows both to `title: JobTitle`.
  - Bridge: `JobTitle::untranslated(&str)` (`pub(crate)`) plus `From<&str>` / `From<&String>`, deleted by Task 7.

- [ ] **Step 0: Record the base commit**

Run from Git Bash:
```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && git branch --show-current && git rev-parse HEAD
```
Expected: `art-debt-0914`, then a SHA. Write that SHA to a new file, `D:\Projeler\Amiga\scratch-0913\art301-base.txt`, with the Write tool. Task 8's review diffs against it.

- [ ] **Step 1: Write the core's failing tests**

In `src-tauri/src/core/jobs/mod.rs`, inside `mod tests`, replace the `title: "Scanning".into(),` line of `fraction_is_none_without_a_total` with:
```rust
            title: JobTitle::new("components.jobBar.title.indexTitles"),
```
Append these tests at the end of `mod tests`, before its closing `}`:
```rust
    /// ART-301. The job bar renders a title with `t(title.key, title.params)`,
    /// so the wire shape is the TypeScript `Phrase` shape, exactly.
    #[test]
    fn a_job_title_serializes_to_the_phrase_shape() {
        let title = JobTitle::new("components.jobBar.title.copyOutOf").text("source", &"DH0.hdf");
        assert_eq!(
            serde_json::to_value(&title).unwrap(),
            serde_json::json!({
                "key": "components.jobBar.title.copyOutOf",
                "params": { "source": "DH0.hdf" }
            })
        );
    }

    /// A count travels as a JSON number named `count` — the shape
    /// `JobTitle.params` declares on the TypeScript side, and the name
    /// i18next reads to choose `_one` or `_other`.
    #[test]
    fn a_count_is_sent_as_a_number_named_count() {
        let title = JobTitle::new("components.jobBar.title.downloadPackages").count(1);
        assert_eq!(
            serde_json::to_value(&title).unwrap(),
            serde_json::json!({
                "key": "components.jobBar.title.downloadPackages",
                "params": { "count": 1 }
            })
        );
    }

    /// Nothing to interpolate sends no `params` at all — `Phrase.params?`.
    #[test]
    fn a_title_without_values_sends_no_params_field() {
        let title = JobTitle::new("components.jobBar.title.syncAminet");
        assert_eq!(
            serde_json::to_value(&title).unwrap(),
            serde_json::json!({ "key": "components.jobBar.title.syncAminet" })
        );
    }

    /// The event the job bar receives carries the phrase, not a sentence.
    #[test]
    fn a_job_progress_sends_its_title_as_a_phrase_not_a_sentence() {
        let progress = JobProgress {
            id: 7,
            title: JobTitle::new("components.jobBar.title.addPackages")
                .count(2)
                .text("target", &"E:/tree"),
            done: 0,
            total: None,
            message: String::new(),
            state: JobState::Running,
        };
        let sent = serde_json::to_value(&progress).unwrap();
        assert_eq!(
            sent["title"],
            serde_json::json!({
                "key": "components.jobBar.title.addPackages",
                "params": { "count": 2, "target": "E:/tree" }
            })
        );
    }

    /// `JobProgress` derives `Deserialize`, so the title must read back as
    /// itself — a count as a count, a text as a text.
    #[test]
    fn a_title_reads_back_as_itself() {
        let title = JobTitle::new("components.jobBar.title.previewPackages")
            .count(3)
            .text("target", &"Work.hdf");
        let back: JobTitle =
            serde_json::from_value(serde_json::to_value(&title).unwrap()).unwrap();
        assert_eq!(back, title);
    }
```

- [ ] **Step 2: Write the runner's failing test, and move its tests onto `JobTitle`**

In `src-tauri/src/commands/jobs.rs`, inside `mod tests`, directly after `use super::*;`, add:
```rust

    // Real catalogue keys, so no test builds a title the job bar could not
    // render. `src/i18n/job-title-keys.test.ts` skips this file on purpose:
    // these are fixtures, not places a job starts.
    const COMPONENTS: &str = "components.jobBar.title.previewComponents";
    const PACKAGES: &str = "components.jobBar.title.previewPackages";
    const INSTALL: &str = "components.jobBar.title.installRelease";
    const INDEX: &str = "components.jobBar.title.indexTitles";
    const REFRESH: &str = "components.jobBar.title.refreshCatalogue";

    fn title(key: &'static str) -> JobTitle {
        JobTitle::new(key)
    }

    /// ART-301. The registry keeps the title it was handed, values and all,
    /// and hands it back unchanged — the bar renders what Rust named.
    #[test]
    fn the_registry_keeps_the_title_it_was_given() {
        let registry = JobRegistry::new();
        let given = JobTitle::new("components.jobBar.title.addPackages")
            .count(1)
            .text("target", &"E:/tree");
        let (id, _) = registry.open(given.clone());

        let snapshot = registry.snapshot();
        let kept = &snapshot.iter().find(|p| p.id == id).unwrap().title;
        assert_eq!(kept, &given);
        assert_eq!(kept.key(), "components.jobBar.title.addPackages");
        assert_eq!(kept.params().len(), 2, "the count and the target both survive");
    }
```
Then make these exact replacements in the same `mod tests`:

| Old | New |
|---|---|
| `registry.open_in_lane("Preview 1", Some("preview"))` | `registry.open_in_lane(title(COMPONENTS), Some("preview"))` |
| `registry.open_in_lane(&format!("Preview {round}"), Some("preview"))` | `registry.open_in_lane(title(COMPONENTS), Some("preview"))` |
| `registry.open_in_lane("Preview", Some("preview"))` (two places) | `registry.open_in_lane(title(COMPONENTS), Some("preview"))` |
| `registry.open_in_lane("Packages", Some("packages"))` | `registry.open_in_lane(title(PACKAGES), Some("packages"))` |
| `registry.open("Installing AmigaOS")` | `registry.open(title(INSTALL))` |
| `registry.open_in_lane("Components", Some("components"))` | `registry.open_in_lane(title(COMPONENTS), Some("components"))` |
| `registry.open("A")` | `registry.open(title(INDEX))` |
| `registry.open("B")` | `registry.open(title(REFRESH))` |
| `registry.open("Finished one")` | `registry.open(title(INDEX))` |
| `registry.open("Still going")` | `registry.open(title(REFRESH))` |
| `assert_eq!(snapshot[0].title, "Still going");` | `assert_eq!(snapshot[0].title.key(), REFRESH);` |
| `registry.open("Scanning")` (two places) | `registry.open(title(INDEX))` |
| `registry.open("Done")` | `registry.open(title(INDEX))` |
| `registry.open("Running")` | `registry.open(title(REFRESH))` |
| `assert_eq!(snapshot[0].title, "Running");` | `assert_eq!(snapshot[0].title.key(), REFRESH);` |
| `registry.open("Hashing")` | `registry.open(title(INDEX))` |

- [ ] **Step 3: Run the tests and see them fail**

```bash
export TMP='E:\amiga\ProjeART\tmp' TEMP='E:\amiga\ProjeART\tmp'
cd /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri && cargo test --lib jobs::
```
Expected: the build fails with `error[E0433]: failed to resolve: use of undeclared type `JobTitle`` (and/or `cannot find type `JobTitle` in this scope`). Copy the first error line into `D:\Projeler\Amiga\scratch-0913\art301-evidence.md` under a heading `Task 1 red`.

- [ ] **Step 4: Add `JobTitle` to the core**

In `src-tauri/src/core/jobs/mod.rs`, replace the imports
```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
```
with
```rust
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::Display;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
```
Directly after the `impl JobState { … }` block (it ends with `}` after `is_terminal`), insert:
```rust

/// What a job is, as a catalogue key and the values its sentence needs —
/// never a sentence (ART-301).
///
/// The job bar used to show an English sentence the command layer composed
/// with `format!`, so a Turkish screen said *"Adding 1 package(s) to …"*. The
/// sentence belongs to the UI's catalogue, in the user's language (§68) — the
/// reasoning [`JobState::Cancelled`] already follows for its count.
///
/// Serialises to exactly the TypeScript `Phrase` shape, `{ "key": …,
/// "params": { … } }` with `params` left out when empty, so the job bar
/// renders it with `t(title.key, title.params)`.
///
/// **A key is a literal, by construction.** [`JobTitle::new`] takes a
/// `&'static str` and the field is private. `src/i18n/job-title-keys.test.ts`
/// reads every `JobTitle::new("…")` in the Rust tree and fails on a key the
/// catalogue list does not hold, a listed key no Rust site names, or a value
/// the sentence does not use.
///
/// Values are what the user gave or what ART found — a path, a package name,
/// a release — and are never translated. A count goes in through
/// [`JobTitle::count`], because i18next chooses `_one` / `_other` only from a
/// value named `count`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobTitle {
    /// A `Cow` only so the type can derive `Deserialize`; every key ART
    /// builds is borrowed from a literal.
    key: Cow<'static, str>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    params: BTreeMap<String, JobParam>,
}

/// One value a job title interpolates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JobParam {
    /// A JSON number. Listed first so a number reads back as a count.
    Count(u64),
    Text(String),
}

impl JobTitle {
    pub fn new(key: &'static str) -> Self {
        Self {
            key: Cow::Borrowed(key),
            params: BTreeMap::new(),
        }
    }

    /// A value the sentence names as `{{name}}`, rendered through `Display` —
    /// pass a path as `&path.display()`.
    pub fn text(mut self, name: &'static str, value: &dyn Display) -> Self {
        self.params
            .insert(name.to_string(), JobParam::Text(value.to_string()));
        self
    }

    /// The `{{count}}` that chooses the sentence's plural form.
    pub fn count(mut self, n: usize) -> Self {
        self.params
            .insert("count".to_string(), JobParam::Count(n as u64));
        self
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn params(&self) -> &BTreeMap<String, JobParam> {
        &self.params
    }

    /// **ART-301 migration bridge — Task 7 of the plan deletes this.** Carries
    /// a sentence from a call site that has not moved to [`JobTitle::new`] yet.
    pub(crate) fn untranslated(sentence: &str) -> Self {
        Self {
            key: Cow::Owned(sentence.to_string()),
            params: BTreeMap::new(),
        }
    }
}
```
In `pub struct JobProgress`, replace
```rust
    /// What this job is, in the user's language: "Scanning collection".
    pub title: String,
```
with
```rust
    /// What this job is: a catalogue key and its values, rendered in the
    /// user's language by the job bar (ART-301).
    pub title: JobTitle,
```

- [ ] **Step 5: Make the runner store a `JobTitle`, with the bridge**

In `src-tauri/src/commands/jobs.rs`:

Replace `use crate::core::jobs::{CancelToken, JobId, JobProgress, JobState, ProgressSink};` with
```rust
use crate::core::jobs::{CancelToken, JobId, JobProgress, JobState, JobTitle, ProgressSink};
```
Replace
```rust
    fn open(&self, title: &str) -> (JobId, CancelToken) {
        self.open_in_lane(title, None)
    }

    fn open_in_lane(&self, title: &str, lane: Option<&'static str>) -> (JobId, CancelToken) {
```
with
```rust
    fn open(&self, title: JobTitle) -> (JobId, CancelToken) {
        self.open_in_lane(title, None)
    }

    fn open_in_lane(&self, title: JobTitle, lane: Option<&'static str>) -> (JobId, CancelToken) {
```
and, inside it, replace `                title: title.to_string(),` with `                title,`.

Replace
```rust
pub fn spawn_job<F>(app: &AppHandle, registry: Arc<JobRegistry>, title: &str, work: F) -> JobId
where
    F: FnOnce(JobId, &dyn ProgressSink) -> Result<(), CoreError> + Send + 'static,
{
    spawn_in_lane(app, registry, title, None, work)
}
```
with
```rust
pub fn spawn_job<F>(
    app: &AppHandle,
    registry: Arc<JobRegistry>,
    title: impl Into<JobTitle>,
    work: F,
) -> JobId
where
    F: FnOnce(JobId, &dyn ProgressSink) -> Result<(), CoreError> + Send + 'static,
{
    spawn_in_lane(app, registry, title.into(), None, work)
}
```
In `spawn_job_in_lane`, replace `    title: &str,` with `    title: impl Into<JobTitle>,`, and replace `    spawn_in_lane(app, registry, title, Some(lane), work)` with `    spawn_in_lane(app, registry, title.into(), Some(lane), work)`.
In `fn spawn_in_lane<F>(`, replace `    title: &str,` with `    title: JobTitle,`.
In the doc comment of `spawn_job`, after the line `/// check `is_cancelled` between units of work.`, add:
```rust
///
/// `title` is what the job bar shows: a catalogue key and its values, never
/// an English sentence (ART-301).
```
Directly above the `// Commands` separator block, insert:
```rust
// ---------------------------------------------------------------------------
// ART-301 migration bridge — Task 7 of the plan deletes this block
//
// Forty call sites pass a sentence today. These two conversions keep them
// compiling while they move to `JobTitle::new("…")` a batch at a time; once
// the last has moved, deleting this block is what proves none was missed.
// ---------------------------------------------------------------------------

impl From<&str> for JobTitle {
    fn from(sentence: &str) -> Self {
        JobTitle::untranslated(sentence)
    }
}

impl From<&String> for JobTitle {
    fn from(sentence: &String) -> Self {
        JobTitle::untranslated(sentence)
    }
}

```

- [ ] **Step 6: Run the tests and see them pass**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri && cargo test --lib jobs::
```
Expected: `test result: ok.` with `0 failed`. The list includes the six new tests: `core::jobs::tests::a_job_title_serializes_to_the_phrase_shape`, `…a_count_is_sent_as_a_number_named_count`, `…a_title_without_values_sends_no_params_field`, `…a_job_progress_sends_its_title_as_a_phrase_not_a_sentence`, `…a_title_reads_back_as_itself`, and `commands::jobs::tests::the_registry_keeps_the_title_it_was_given`.

- [ ] **Step 7: The whole crate compiles, formatted and clippy-clean**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri && cargo fmt && cargo fmt --check && cargo clippy --all-targets -- -D warnings
```
Expected: `cargo fmt --check` prints nothing, and clippy ends `Finished` with no `warning:` or `error:` line. The 40 unmigrated sites compile through the bridge.

- [ ] **Step 8: Mutate the two serde attributes and see the guards fail**

Back up: `mkdir -p /d/Projeler/Amiga/scratch-0913/art301-bak && cp /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri/src/core/jobs/mod.rs /d/Projeler/Amiga/scratch-0913/art301-bak/mod.rs`

- M1: change `#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]` to `#[serde(default)]`. Run `cargo test --lib core::jobs::`. Expected: `a_title_without_values_sends_no_params_field` FAILS (the value carries `"params": {}`). Restore: `cp /d/Projeler/Amiga/scratch-0913/art301-bak/mod.rs /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri/src/core/jobs/mod.rs`.
- M2: delete the line `#[serde(untagged)]`. Run the same command. Expected: `a_count_is_sent_as_a_number_named_count` FAILS (`{"Count":1}`). Restore the same way.

After restoring, run `cargo test --lib core::jobs::` once more. Expected: `test result: ok.` Record both mutations and the failing test names in the evidence log under `Task 1 mutations`.

- [ ] **Step 9: Commit**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && git branch --show-current && git add src-tauri/src/core/jobs/mod.rs src-tauri/src/commands/jobs.rs && git commit -F /d/Projeler/Amiga/scratch-0913/commit-msg-art301.txt
```
Message file content:
```
ART-301: a job carries a JobTitle, a catalogue key and its values

JobProgress.title is a JobTitle serialised to the Phrase shape instead of an
English sentence. spawn_job accepts impl Into<JobTitle>; a From<&str> bridge
keeps the 40 unmigrated sites compiling until the plan's Task 7 deletes it.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01KZGHnMHaXd1F9eYT5G6ieE
```

---

### Task 2: The key list, both catalogues, and the job bar renders `t()`

**Files:**
- Modify: `src/lib/jobs.ts:36-46`
- Modify: `src/components/JobBar.tsx:148`
- Rewrite: `src/components/JobBar.test.tsx`
- Create: `src/i18n/job-title-keys.test.ts`
- Modify: `src/i18n/en.json:189-190`, `src/i18n/tr.json:189-190` (inside `components.jobBar`)
- Modify fixtures:
  - `src/i18n/phrase-keys.test.ts:440`
  - `src/lib/jobs.test.ts:46`, `:180`
  - `src/components/osbuilder/AppearancePanel.test.tsx:493`
  - `src/components/osbuilder/BuildTab.test.tsx:883`, `:911`
  - `src/components/osbuilder/AmigaInstallPanel.test.tsx:1020`, `:1041`, `:1062`, `:1084`, `:1114`
  - `src/lib/useBuildRun.test.tsx:133`
- Modify: `src/i18n/literal-keys.test.ts:715-718` (dynamic call count)

**Interfaces:**
- Consumes: the wire shape from Task 1, `{ key, params? }`.
- Produces:
  - `export const JOB_TITLE_KEYS` (readonly, sorted, 36 entries)
  - `export type JobTitleKey`
  - `export interface JobTitle { key: JobTitleKey; params?: Record<string, string | number> }`
  - `JobProgress.title: JobTitle`, all in `@/lib/jobs`
  - Catalogue leaves under `components.jobBar.title.*`
  - The helpers `forms` and `placeholders` inside `src/i18n/job-title-keys.test.ts`, which Task 3 extends.

- [ ] **Step 1: Rewrite the job bar's test**

Replace the whole of `src/components/JobBar.test.tsx` with:
```tsx
// @vitest-environment jsdom
//
// ART-195. The owner photographed the job bar with four preview rows stacked
// on it, counts jumping about, and pressing Stop appeared to add a fifth. The
// producing side is fixed elsewhere (`spawn_job_in_lane` cancels the previous
// preview; `useRemembered` no longer re-fires the effect that starts them).
// This file is about the bar itself: when ART supersedes a job, the row has to
// come **off**.
//
// Getting that wrong is not a cosmetic miss. `JobBar` keeps failed *and
// cancelled* jobs on screen as "notable", so a superseded preview reported as
// `cancelled` would have swapped four stacked running rows for four stacked
// cancelled ones — the complaint, wearing the fix's clothes.
//
// ART-301. A job's title is a catalogue key Rust names, rendered in the user's
// language. The owner saw *"Adding 1 package(s) to …"* on a Turkish screen.

import { render, screen, act, cleanup } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { changeLanguage } from "@/i18n";
import type { JobProgress, JobTitle } from "@/lib/jobs";

const jobListMock = vi.fn();
const jobCancelMock = vi.fn();
const jobClearFinishedMock = vi.fn();
let emit: ((job: JobProgress) => void) | null = null;

vi.mock("@/lib/jobs", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/jobs")>()),
  jobList: () => jobListMock(),
  jobCancel: (id: number) => jobCancelMock(id),
  jobClearFinished: () => jobClearFinishedMock(),
  onJobProgress: (handler: (job: JobProgress) => void) => {
    emit = handler;
    return Promise.resolve(() => {
      emit = null;
    });
  },
}));

const { JobBar } = await import("@/components/JobBar");

const COMPONENTS: JobTitle = {
  key: "components.jobBar.title.previewComponents",
  params: { count: 3, release: "AmigaOS 3.2" },
};
const PACKAGES: JobTitle = {
  key: "components.jobBar.title.previewPackages",
  params: { count: 1, target: "Work.hdf" },
};
const INSTALL: JobTitle = {
  key: "components.jobBar.title.installRelease",
  params: { release: "AmigaOS 3.2", target: "Tree" },
};

function running(id: number, title: JobTitle): JobProgress {
  return { id, title, done: 100 + id, total: null, message: "", state: { state: "running" } };
}

beforeEach(async () => {
  // Vitest is not configured with `globals`, so Testing Library's automatic
  // cleanup never runs — without this the previous test's bar is still in the
  // document and `getAllByRole` finds its buttons too.
  cleanup();
  emit = null;
  jobListMock.mockReset().mockResolvedValue([]);
  jobCancelMock.mockReset().mockResolvedValue(true);
  jobClearFinishedMock.mockReset().mockResolvedValue(undefined);
  await changeLanguage("en");
});

async function send(job: JobProgress) {
  await act(async () => {
    emit?.(job);
  });
}

describe("the job bar", () => {
  it("takes a superseded job off the bar instead of restating it", async () => {
    render(<JobBar />);
    await act(async () => {});

    await send(running(1, COMPONENTS));
    await send(running(2, PACKAGES));

    // The bar is genuinely populated first. Without this the test would pass
    // against a bar that never showed anything at all — one of the two
    // vacuous shapes this round has been producing.
    expect(screen.getByText("Previewing 3 components of AmigaOS 3.2")).toBeTruthy();
    expect(screen.getByText("Previewing 1 package against Work.hdf")).toBeTruthy();

    await send({ ...running(1, COMPONENTS), state: { state: "superseded" } });

    expect(screen.queryByText("Previewing 3 components of AmigaOS 3.2")).toBeNull();
    // …and only that one. The newer preview is still running and must stay.
    expect(screen.getByText("Previewing 1 package against Work.hdf")).toBeTruthy();
  });

  it("still keeps a job the user cancelled, which is news", async () => {
    render(<JobBar />);
    await act(async () => {});

    await send(running(1, INSTALL));
    expect(screen.getByText("Installing AmigaOS 3.2 into Tree")).toBeTruthy();

    await send({ ...running(1, INSTALL), state: { state: "cancelled", files_landed: null } });

    // The contrast is the point: superseded disappears, cancelled does not.
    // A fix that simply hid every terminal job would pass the test above and
    // fail this one.
    expect(screen.getByText("Installing AmigaOS 3.2 into Tree")).toBeTruthy();
  });

  it("wires its Stop button to jobCancel with that row's own id", async () => {
    // The owner reported that stopping "started a new job". It did not: the
    // button was always wired to `jobCancel`, and what produced the new job
    // was the render loop that is fixed in `useRemembered`. Pinned here so
    // the innocent half stays innocent.
    render(<JobBar />);
    await act(async () => {});
    await send(running(7, COMPONENTS));

    const stop = screen.getAllByRole("button").find((b) => b.textContent?.length);
    expect(stop).toBeTruthy();
    await act(async () => {
      stop!.click();
    });
    expect(jobCancelMock).toHaveBeenCalledWith(7);
  });

  it("names a job in Turkish when Turkish is chosen (ART-301)", async () => {
    await changeLanguage("tr");
    render(<JobBar />);
    await act(async () => {});

    await send(
      running(1, { key: "components.jobBar.title.addPackages", params: { count: 1, target: "Work.hdf" } })
    );

    expect(screen.getByText("Work.hdf içine 1 paket ekleniyor")).toBeTruthy();
    // The owner's screenshot, exactly: an English title on a Turkish screen.
    expect(screen.queryByText(/package/)).toBeNull();
  });

  it("picks the plural from count, not from a hand-built (s)", async () => {
    render(<JobBar />);
    await act(async () => {});

    await send(
      running(1, { key: "components.jobBar.title.addPackages", params: { count: 1, target: "Work.hdf" } })
    );
    await send(
      running(2, { key: "components.jobBar.title.addPackages", params: { count: 2, target: "Work.hdf" } })
    );

    expect(screen.getByText("Adding 1 package to Work.hdf")).toBeTruthy();
    expect(screen.getByText("Adding 2 packages to Work.hdf")).toBeTruthy();
    expect(screen.queryByText(/\(s\)/)).toBeNull();
  });
});
```

- [ ] **Step 2: Write the catalogue half of the key test**

Create `src/i18n/job-title-keys.test.ts`:
```ts
// ART-301. A job's title is a catalogue key that Rust names —
// `JobTitle::new("components.jobBar.title.…")` in `src-tauri/src/commands` —
// and `JobBar.tsx` renders it through a variable, so `literal-keys.test.ts`
// counts it and skips it. `JOB_TITLE_KEYS` in `@/lib/jobs` is where the keys
// are written out. This file checks that list against both catalogues. A key
// that exists only in `en.json` is a Turkish screen with an English title on
// it, which is the defect this round is named for.

import { describe, expect, it } from "vitest";

import { JOB_TITLE_KEYS } from "@/lib/jobs";
import en from "./en.json";
import tr from "./tr.json";

const PREFIX = "components.jobBar.title.";

function leaf(catalogue: unknown, key: string): string | undefined {
  let node: unknown = catalogue;
  for (const part of key.split(".")) {
    if (typeof node !== "object" || node === null) return undefined;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === "string" ? node : undefined;
}

/** The sentences a key renders as: the key itself, or a complete `_one` /
 *  `_other` pair. Anything else is a string naming what is wrong. */
function forms(catalogue: unknown, key: string): string[] | "missing" | "mixed" {
  const bare = leaf(catalogue, key);
  const one = leaf(catalogue, `${key}_one`);
  const other = leaf(catalogue, `${key}_other`);
  if (bare !== undefined && one === undefined && other === undefined) return [bare];
  if (bare === undefined && one !== undefined && other !== undefined) return [one, other];
  if (bare === undefined && one === undefined && other === undefined) return "missing";
  return "mixed";
}

/** The `{{name}}`s a set of sentences interpolates, sorted, once each. */
function placeholders(sentences: string[]): string[] {
  return [
    ...new Set(sentences.flatMap((s) => [...s.matchAll(/{{\s*(\w+)/g)].map((m) => m[1]))),
  ].sort();
}

function broken(catalogue: unknown): string[] {
  return JOB_TITLE_KEYS.flatMap((key) => {
    const found = forms(catalogue, key);
    return typeof found === "string" ? [`${key}: ${found}`] : [];
  });
}

describe("job title keys (ART-301)", () => {
  it("lists each key once, sorted, under the job bar's own namespace", () => {
    expect([...JOB_TITLE_KEYS]).toEqual([...new Set(JOB_TITLE_KEYS)].sort());
    for (const key of JOB_TITLE_KEYS) expect(key.startsWith(PREFIX), key).toBe(true);
  });

  it("resolves every key in English, as one sentence or a complete plural pair", () => {
    expect(broken(en)).toEqual([]);
  });

  it("resolves every key in Turkish, as one sentence or a complete plural pair", () => {
    // The half parity cannot reach: a key present in *neither* catalogue is a
    // pair `parity.test.ts` is perfectly happy with.
    expect(broken(tr)).toEqual([]);
  });

  it("uses a plural pair exactly when the sentence counts", () => {
    const wrong: string[] = [];
    for (const key of JOB_TITLE_KEYS) {
      const sentences = forms(en, key);
      if (typeof sentences === "string") continue; // reported by the test above
      const counts = placeholders(sentences).includes("count");
      if (counts !== (sentences.length === 2)) wrong.push(key);
    }
    expect(wrong).toEqual([]);
  });
});
```

- [ ] **Step 3: Run both and see them fail**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && pnpm vitest run src/components/JobBar.test.tsx src/i18n/job-title-keys.test.ts
```
Expected:
- `JobBar.test.tsx` fails: React's `Objects are not valid as a React child (found: object with keys {key, params})`. The bar still renders `{job.title}`.
- `job-title-keys.test.ts` fails: `JOB_TITLE_KEYS` is not exported yet, so it fails with a `TypeError` (`… is not iterable` or `… undefined`).

Record both first failure lines in the evidence log under `Task 2 red`.

- [ ] **Step 4: Add the list and the type to `@/lib/jobs`**

In `src/lib/jobs.ts`, replace
```ts
export interface JobProgress {
  id: number;
  /** What the job is, in the user's language. */
  title: string;
```
with
```ts
/**
 * Every catalogue key a job title may name (ART-301).
 *
 * A job's title is decided in Rust — `JobTitle::new("…")` in
 * `src-tauri/src/commands/*.rs` — and rendered by `JobBar.tsx` through a
 * variable, so no TypeScript code would otherwise name these keys. This list
 * is where they are named. `dead-keys.test.ts` counts them as reachable
 * because they are written out here. `src/i18n/job-title-keys.test.ts`
 * resolves each one in both catalogues and reads the Rust tree. It fails on a
 * Rust key missing from this list, on a key here that no Rust site names, and
 * on values a sentence does not use.
 *
 * Sorted, one entry per key. A plural key is named without `_one` / `_other`.
 */
export const JOB_TITLE_KEYS = [
  "components.jobBar.title.addPackages",
  "components.jobBar.title.applyAppearance",
  "components.jobBar.title.applyLayout",
  "components.jobBar.title.buildCard",
  "components.jobBar.title.copyArchiveInto",
  "components.jobBar.title.copyDiscInto",
  "components.jobBar.title.copyInto",
  "components.jobBar.title.copyOutOf",
  "components.jobBar.title.copySelectionBetween",
  "components.jobBar.title.copySelectionInto",
  "components.jobBar.title.copySelectionOutOf",
  "components.jobBar.title.countFolder",
  "components.jobBar.title.countVolumeBlock",
  "components.jobBar.title.deleteItems",
  "components.jobBar.title.downloadPackage",
  "components.jobBar.title.downloadPackages",
  "components.jobBar.title.fetchArtwork",
  "components.jobBar.title.identifyMedia",
  "components.jobBar.title.indexTitles",
  "components.jobBar.title.installArchive",
  "components.jobBar.title.installArchiveInto",
  "components.jobBar.title.installArchivesInto",
  "components.jobBar.title.installOnAmiga",
  "components.jobBar.title.installRelease",
  "components.jobBar.title.planArchives",
  "components.jobBar.title.planLayout",
  "components.jobBar.title.preparePartitions",
  "components.jobBar.title.previewComponents",
  "components.jobBar.title.previewPackages",
  "components.jobBar.title.readLocalPictures",
  "components.jobBar.title.readReadme",
  "components.jobBar.title.refreshCatalogue",
  "components.jobBar.title.rehearseFirstBoot",
  "components.jobBar.title.restorePictures",
  "components.jobBar.title.syncAminet",
  "components.jobBar.title.writeIgameData",
] as const;

export type JobTitleKey = (typeof JOB_TITLE_KEYS)[number];

/**
 * What a job is: a catalogue key and the values its sentence needs, never a
 * sentence (ART-301). The same shape as `Phrase`, narrowed to the keys above.
 * Values are paths, names and counts, and pass through untranslated.
 */
export interface JobTitle {
  key: JobTitleKey;
  params?: Record<string, string | number>;
}

export interface JobProgress {
  id: number;
  /** What the job is — render with `t(title.key, title.params)`. */
  title: JobTitle;
```

- [ ] **Step 5: Run the key test and see the catalogue half fail**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && pnpm vitest run src/i18n/job-title-keys.test.ts
```
Expected: 2 failed, 2 passed. Both "resolves every key in …" tests list 36 entries of the form `components.jobBar.title.addPackages: missing`. Record the first line in the evidence log.

- [ ] **Step 6: Add the sentences to both catalogues**

In `src/i18n/en.json`, inside `"jobBar"`, the `"status"` block ends with
```json
        "failed": "Failed ({{code}})"
      }
    },
```
Replace that with:
```json
        "failed": "Failed ({{code}})"
      },
      "title": {
        "addPackages_one": "Adding {{count}} package to {{target}}",
        "addPackages_other": "Adding {{count}} packages to {{target}}",
        "applyAppearance": "Applying appearance to distribution tree",
        "applyLayout_one": "Laying {{count}} item out in {{target}}",
        "applyLayout_other": "Laying {{count}} items out in {{target}}",
        "buildCard": "Building {{target}}",
        "copyArchiveInto": "Copying an archive into {{target}}",
        "copyDiscInto": "Copying a disc into {{target}}",
        "copyInto": "Copying into {{target}}",
        "copyOutOf": "Copying out of {{source}}",
        "copySelectionBetween": "Copying a selection between {{source}} and {{target}}",
        "copySelectionInto": "Copying a selection into {{target}}",
        "copySelectionOutOf": "Copying a selection out of {{source}}",
        "countFolder": "Counting {{path}}",
        "countVolumeBlock": "Counting block {{block}} in {{volume}}",
        "deleteItems_one": "Deleting {{count}} item from {{source}}",
        "deleteItems_other": "Deleting {{count}} items from {{source}}",
        "downloadPackage": "Downloading {{name}}",
        "downloadPackages_one": "Downloading {{count}} package",
        "downloadPackages_other": "Downloading {{count}} packages",
        "fetchArtwork": "Fetching artwork",
        "identifyMedia": "Identifying media in {{source}}",
        "indexTitles": "Indexing titles",
        "installArchive": "Installing {{name}}",
        "installArchiveInto": "Installing {{name}} into {{target}}",
        "installArchivesInto_one": "Installing {{count}} archive into {{target}}",
        "installArchivesInto_other": "Installing {{count}} archives into {{target}}",
        "installOnAmiga": "Installing {{name}} on the Amiga",
        "installRelease": "Installing {{release}} into {{target}}",
        "planArchives_one": "Planning {{count}} archive",
        "planArchives_other": "Planning {{count}} archives",
        "planLayout_one": "Working out what {{count}} source needs",
        "planLayout_other": "Working out what {{count}} sources need",
        "preparePartitions_one": "Preparing {{count}} volume on {{target}}",
        "preparePartitions_other": "Preparing {{count}} volumes on {{target}}",
        "previewComponents_one": "Previewing {{count}} component of {{release}}",
        "previewComponents_other": "Previewing {{count}} components of {{release}}",
        "previewPackages_one": "Previewing {{count}} package against {{target}}",
        "previewPackages_other": "Previewing {{count}} packages against {{target}}",
        "readLocalPictures": "Reading pictures from your files",
        "readReadme": "Reading {{name}}",
        "refreshCatalogue": "Refreshing the catalogue",
        "rehearseFirstBoot": "Rehearsing the first boot on {{machine}}",
        "restorePictures": "Restoring your own pictures",
        "syncAminet": "Syncing the Aminet catalogue",
        "writeIgameData_one": "Writing igame.data for {{count}} title",
        "writeIgameData_other": "Writing igame.data for {{count}} titles"
      }
    },
```
In `src/i18n/tr.json`, the `"status"` block ends with
```json
        "failed": "Başarısız ({{code}})"
      }
    },
```
Replace that with:
```json
        "failed": "Başarısız ({{code}})"
      },
      "title": {
        "addPackages_one": "{{target}} içine {{count}} paket ekleniyor",
        "addPackages_other": "{{target}} içine {{count}} paket ekleniyor",
        "applyAppearance": "Görünüm dağıtım ağacına uygulanıyor",
        "applyLayout_one": "{{target}} içine {{count}} öğe yerleştiriliyor",
        "applyLayout_other": "{{target}} içine {{count}} öğe yerleştiriliyor",
        "buildCard": "{{target}} oluşturuluyor",
        "copyArchiveInto": "{{target}} içine bir arşiv kopyalanıyor",
        "copyDiscInto": "{{target}} içine bir disk kopyalanıyor",
        "copyInto": "{{target}} içine kopyalanıyor",
        "copyOutOf": "{{source}} içinden kopyalanıyor",
        "copySelectionBetween": "Seçilenler {{source}} içinden {{target}} içine kopyalanıyor",
        "copySelectionInto": "Seçilenler {{target}} içine kopyalanıyor",
        "copySelectionOutOf": "Seçilenler {{source}} içinden kopyalanıyor",
        "countFolder": "{{path}} içeriği sayılıyor",
        "countVolumeBlock": "{{volume}} içinde {{block}} numaralı blok sayılıyor",
        "deleteItems_one": "{{source}} içinden {{count}} öğe siliniyor",
        "deleteItems_other": "{{source}} içinden {{count}} öğe siliniyor",
        "downloadPackage": "{{name}} indiriliyor",
        "downloadPackages_one": "{{count}} paket indiriliyor",
        "downloadPackages_other": "{{count}} paket indiriliyor",
        "fetchArtwork": "Görseller getiriliyor",
        "identifyMedia": "{{source}} içindeki kurulum diskleri tanınıyor",
        "indexTitles": "Başlıklar dizinleniyor",
        "installArchive": "{{name}} kuruluyor",
        "installArchiveInto": "{{target}} içine {{name}} kuruluyor",
        "installArchivesInto_one": "{{target}} içine {{count}} arşiv kuruluyor",
        "installArchivesInto_other": "{{target}} içine {{count}} arşiv kuruluyor",
        "installOnAmiga": "{{name}} Amiga üzerinde kuruluyor",
        "installRelease": "{{target}} içine {{release}} kuruluyor",
        "planArchives_one": "{{count}} arşiv planlanıyor",
        "planArchives_other": "{{count}} arşiv planlanıyor",
        "planLayout_one": "{{count}} kaynak için gerekenler hesaplanıyor",
        "planLayout_other": "{{count}} kaynak için gerekenler hesaplanıyor",
        "preparePartitions_one": "{{target}} üzerinde {{count}} birim hazırlanıyor",
        "preparePartitions_other": "{{target}} üzerinde {{count}} birim hazırlanıyor",
        "previewComponents_one": "{{release}} için {{count}} bileşen önizleniyor",
        "previewComponents_other": "{{release}} için {{count}} bileşen önizleniyor",
        "previewPackages_one": "{{target}} için {{count}} paket önizleniyor",
        "previewPackages_other": "{{target}} için {{count}} paket önizleniyor",
        "readLocalPictures": "Dosyalarınızdaki resimler okunuyor",
        "readReadme": "{{name}} okunuyor",
        "refreshCatalogue": "Katalog yenileniyor",
        "rehearseFirstBoot": "İlk açılış {{machine}} üzerinde deneniyor",
        "restorePictures": "Kendi resimleriniz geri yükleniyor",
        "syncAminet": "Aminet kataloğu eşitleniyor",
        "writeIgameData_one": "{{count}} başlık için igame.data yazılıyor",
        "writeIgameData_other": "{{count}} başlık için igame.data yazılıyor"
      }
    },
```
Turkish nouns take no plural after a number, so `_one` and `_other` read the same. That is the pattern the existing `components.jobBar.status.cancelledPartway_one/_other` pair already uses.

- [ ] **Step 7: Render the title through `t()`**

In `src/components/JobBar.tsx`, replace
```tsx
          <strong>{job.title}</strong>
```
with
```tsx
          {/* A catalogue key Rust named, never a sentence (ART-301). */}
          <strong>{t(job.title.key, job.title.params)}</strong>
```

- [ ] **Step 8: Move the fixtures onto `JobTitle`**

`pnpm lint` type-checks tests (`tsconfig.test.json`), so every fixture that builds a `JobProgress` must use a real key. Make these replacements:

| File | Old | New |
|---|---|---|
| `src/i18n/phrase-keys.test.ts` (in `jobStatusLabel: every JobState variant resolves`) | `      title: "Copying",` | `      title: { key: "components.jobBar.title.copyInto" as const, params: { target: "DH0" } },` |
| `src/lib/jobs.test.ts` (in `function job`) | `    title: "Copying",` | `    title: { key: "components.jobBar.title.copyInto", params: { target: "DH0" } },` |
| `src/lib/jobs.test.ts` (in `rejects from a buffered failed job-progress update`) | `      title: "x",` | `      title: { key: "components.jobBar.title.copyInto", params: { target: "DH0" } },` |
| `src/components/osbuilder/AppearancePanel.test.tsx` | `      title: "Applying appearance to distribution tree",` | `      title: { key: "components.jobBar.title.applyAppearance" },` |
| `src/components/osbuilder/BuildTab.test.tsx` (both places) | `        title: "Building",` | `        title: { key: "components.jobBar.title.installRelease", params: { release: "AmigaOS 3.2", target: "Tree" } },` |
| `src/components/osbuilder/AmigaInstallPanel.test.tsx` (four places, two of them indented by one more level) | `title: "Installing BoingBag 3.9-1 on the Amiga",` | `title: { key: "components.jobBar.title.installOnAmiga", params: { name: "BoingBag 3.9-1" } },` |
| `src/components/osbuilder/AmigaInstallPanel.test.tsx` | `title: "Something else entirely",` | `title: { key: "components.jobBar.title.syncAminet" },` |
| `src/lib/useBuildRun.test.tsx` | `  return { id, title: "Building", done, total: 1980, message: "SYS:C/Assign", state };` | `  return { id, title: { key: "components.jobBar.title.installRelease", params: { release: "AmigaOS 3.2", target: "Tree" } }, done, total: 1980, message: "SYS:C/Assign", state };` |

None of these screens renders `title`; the grep of `\.title\b` over `src/` on 2026-09-14 found only `JobBar.tsx`. So no assertion depends on the old strings.

- [ ] **Step 9: Count the new dynamic `t()` call**

In `src/i18n/literal-keys.test.ts`, find the last `expect(dynamicCalls).toBe(N);` (187 on 2026-09-14; use whatever number is there now). Directly above it, add:
```ts
    // N → N+1 (ART-301): `JobBar.tsx` renders a job's title as
    // `t(job.title.key, job.title.params)` — a key Rust names. The keys are the
    // closed `JOB_TITLE_KEYS` list in `@/lib/jobs`, and
    // `src/i18n/job-title-keys.test.ts` resolves every one in both catalogues
    // and holds the list to the Rust sources, which is the check this scan
    // cannot make.
```
Write the real numbers in place of `N → N+1`, and change the assertion to `N + 1`.

- [ ] **Step 10: Run the affected tests and lint**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && pnpm vitest run src/components/JobBar.test.tsx src/i18n/job-title-keys.test.ts src/i18n/parity.test.ts src/i18n/dead-keys.test.ts src/i18n/literal-keys.test.ts src/i18n/phrase-keys.test.ts src/lib/jobs.test.ts
```
Expected: `Test Files  7 passed (7)`. `JobBar.test.tsx` shows 5 passed and `job-title-keys.test.ts` 4 passed.

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && pnpm lint
```
Expected: both `tsc --noEmit` runs finish with no `error TS` line (run unpiped).

- [ ] **Step 11: Mutate the render and see the bar's tests fail**

Back up `src/components/JobBar.tsx` to `/d/Projeler/Amiga/scratch-0913/art301-bak/JobBar.tsx`. Change `{t(job.title.key, job.title.params)}` to `{job.title.key}`. Run `pnpm vitest run src/components/JobBar.test.tsx`.

Expected: 4 failed and 1 passed. Every test that reads a title fails because it finds the raw key; the Stop-button test passes. Restore:
```bash
cp /d/Projeler/Amiga/scratch-0913/art301-bak/JobBar.tsx /d/Projeler/Amiga/amiga-retro-toolkit/src/components/JobBar.tsx
```
Re-run it: 5 passed. Record the mutation in the evidence log.

- [ ] **Step 12: Commit**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && git branch --show-current && git add src/lib/jobs.ts src/components/JobBar.tsx src/components/JobBar.test.tsx src/i18n/job-title-keys.test.ts src/i18n/en.json src/i18n/tr.json src/i18n/phrase-keys.test.ts src/i18n/literal-keys.test.ts src/lib/jobs.test.ts src/components/osbuilder/AppearancePanel.test.tsx src/components/osbuilder/BuildTab.test.tsx src/components/osbuilder/AmigaInstallPanel.test.tsx src/lib/useBuildRun.test.tsx && git commit -F /d/Projeler/Amiga/scratch-0913/commit-msg-art301.txt
```
Message:
```
ART-301: the job bar renders a job's title through the catalogue

JOB_TITLE_KEYS names the 36 job-title keys, each in en.json and tr.json
(47 leaves each, counts as _one/_other). JobBar renders
t(title.key, title.params); the fixtures carry real keys.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01KZGHnMHaXd1F9eYT5G6ieE
```

---

### Task 3: The test reads the Rust tree; batch A, the Files screen (13 sites)

**Files:**
- Modify: `src/i18n/job-title-keys.test.ts` (whole file below)
- Modify: `src-tauri/src/commands/archive.rs`, `cbm.rs`, `iso.rs`, `panel.rs`, `volume.rs`, `volume_write.rs`

**Interfaces:**
- Consumes: `JobTitle::new/text/count` (Task 1), `JOB_TITLE_KEYS` (Task 2).
- Produces: the scan `RUST` in `job-title-keys.test.ts` (`{ sites: Site[]; calls: number; letCalls: number }`). It enforces the site rule for every later batch: **a title is built in its own `let title = JobTitle::new("<literal>")…;` statement, and `title` is passed by value to the spawn call.**

- [ ] **Step 1: Extend the key test to read the Rust sources**

Replace the whole of `src/i18n/job-title-keys.test.ts` with:
```ts
// ART-301. A job's title is a catalogue key that Rust names —
// `JobTitle::new("components.jobBar.title.…")` in `src-tauri/src/commands` —
// and `JobBar.tsx` renders it through a variable, so `literal-keys.test.ts`
// counts it and skips it. `JOB_TITLE_KEYS` in `@/lib/jobs` is where the keys
// are written out. This file holds that list to both catalogues, and it
// **reads the Rust files themselves**, not a copy of them ("a test that reads
// a table instead of the file is a copy, and copies drift" — CLAUDE.md).
// The precedent is `recipe-component-keys.test.ts`, which reads Rust-tree
// data with `readFileSync` the same way.

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { JOB_TITLE_KEYS } from "@/lib/jobs";
import en from "./en.json";
import tr from "./tr.json";

const PREFIX = "components.jobBar.title.";

function leaf(catalogue: unknown, key: string): string | undefined {
  let node: unknown = catalogue;
  for (const part of key.split(".")) {
    if (typeof node !== "object" || node === null) return undefined;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === "string" ? node : undefined;
}

/** The sentences a key renders as: the key itself, or a complete `_one` /
 *  `_other` pair. Anything else is a string naming what is wrong. */
function forms(catalogue: unknown, key: string): string[] | "missing" | "mixed" {
  const bare = leaf(catalogue, key);
  const one = leaf(catalogue, `${key}_one`);
  const other = leaf(catalogue, `${key}_other`);
  if (bare !== undefined && one === undefined && other === undefined) return [bare];
  if (bare === undefined && one !== undefined && other !== undefined) return [one, other];
  if (bare === undefined && one === undefined && other === undefined) return "missing";
  return "mixed";
}

/** The `{{name}}`s a set of sentences interpolates, sorted, once each. */
function placeholders(sentences: string[]): string[] {
  return [
    ...new Set(sentences.flatMap((s) => [...s.matchAll(/{{\s*(\w+)/g)].map((m) => m[1]))),
  ].sort();
}

function broken(catalogue: unknown): string[] {
  return JOB_TITLE_KEYS.flatMap((key) => {
    const found = forms(catalogue, key);
    return typeof found === "string" ? [`${key}: ${found}`] : [];
  });
}

// --- The Rust side --------------------------------------------------------

const RUST_SRC = resolve(__dirname, "..", "..", "src-tauri", "src");

/** The type's own module and the job runner. Their tests build titles from
 *  keys on purpose, and neither is a place a job starts. */
const MECHANISM = new Set([join("core", "jobs", "mod.rs"), join("commands", "jobs.rs")]);

function rustFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) return rustFiles(path);
    return path.endsWith(".rs") ? [path] : [];
  });
}

interface Site {
  file: string;
  key: string;
  /** The values the chain passes, sorted: every `.text("name", …)`, plus
   *  `count` for a `.count(…)`. */
  params: string[];
}

function scan(): { sites: Site[]; calls: number; letCalls: number } {
  const sites: Site[] = [];
  let calls = 0;
  let letCalls = 0;
  for (const path of rustFiles(RUST_SRC)) {
    const file = relative(RUST_SRC, path);
    if (MECHANISM.has(file)) continue;
    const text = readFileSync(path, "utf8");
    calls += text.match(/JobTitle::new\(/g)?.length ?? 0;
    letCalls += text.match(/let title = JobTitle::new\(/g)?.length ?? 0;
    // A site is one statement, so everything up to its `;` is its chain.
    for (const m of text.matchAll(/JobTitle::new\(\s*"([^"]+)"\s*\)([^;]*);/g)) {
      const chain = m[2];
      const params = [...chain.matchAll(/\.text\(\s*"(\w+)"/g)].map((p) => p[1]);
      if (/\.count\(/.test(chain)) params.push("count");
      sites.push({ file, key: m[1], params: params.sort() });
    }
  }
  return { sites, calls, letCalls };
}

const RUST = scan();

describe("job title keys (ART-301)", () => {
  it("lists each key once, sorted, under the job bar's own namespace", () => {
    expect([...JOB_TITLE_KEYS]).toEqual([...new Set(JOB_TITLE_KEYS)].sort());
    for (const key of JOB_TITLE_KEYS) expect(key.startsWith(PREFIX), key).toBe(true);
  });

  it("resolves every key in English, as one sentence or a complete plural pair", () => {
    expect(broken(en)).toEqual([]);
  });

  it("resolves every key in Turkish, as one sentence or a complete plural pair", () => {
    // The half parity cannot reach: a key present in *neither* catalogue is a
    // pair `parity.test.ts` is perfectly happy with.
    expect(broken(tr)).toEqual([]);
  });

  it("uses a plural pair exactly when the sentence counts", () => {
    const wrong: string[] = [];
    for (const key of JOB_TITLE_KEYS) {
      const sentences = forms(en, key);
      if (typeof sentences === "string") continue; // reported by the test above
      const counts = placeholders(sentences).includes("count");
      if (counts !== (sentences.length === 2)) wrong.push(key);
    }
    expect(wrong).toEqual([]);
  });
});

describe("the job titles Rust sets (ART-301)", () => {
  it("finds the Rust sites", () => {
    // A moved folder or a renamed type would make every assertion below
    // vacuously true.
    expect(RUST.sites.length).toBeGreaterThan(0);
  });

  it("names every key as a literal, in its own `let title` statement", () => {
    // A key built at run time, or a title built inline inside a spawn call,
    // would escape the two checks below — so neither shape is allowed.
    expect(RUST.sites.length).toBe(RUST.calls);
    expect(RUST.letCalls).toBe(RUST.calls);
  });

  it("names only keys the list holds", () => {
    const known = new Set<string>(JOB_TITLE_KEYS);
    const unknown = RUST.sites.filter((s) => !known.has(s.key)).map((s) => `${s.file} → ${s.key}`);
    expect(unknown).toEqual([]);
  });

  it("passes exactly the values its sentence interpolates", () => {
    // i18next renders a missing value as nothing and ignores an extra one,
    // so both directions are silent on screen.
    const wrong = RUST.sites.flatMap((s) => {
      const sentences = forms(en, s.key);
      if (typeof sentences === "string") return []; // an unknown key is reported above
      const wanted = placeholders(sentences);
      return wanted.join() === s.params.join()
        ? []
        : [`${s.file} → ${s.key}: passes [${s.params.join(", ")}], the sentence names [${wanted.join(", ")}]`];
    });
    expect(wrong).toEqual([]);
  });
});
```

- [ ] **Step 2: Run it and see the scan find nothing**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && pnpm vitest run src/i18n/job-title-keys.test.ts
```
Expected: 1 failed, 7 passed. `finds the Rust sites` fails with `expected 0 to be greater than 0`. Record it in the evidence log under `Task 3 red`.

- [ ] **Step 3: Move batch A**

In each of the six files, add `use crate::core::jobs::JobTitle;` with the file's other `use crate::core::…` lines. If the file already has a `use crate::core::jobs::{…}` line, add `JobTitle` to that line instead. Then make each replacement below. In every site, the `&title` argument of the spawn call right after it becomes `title`.

A1, A3 — `archive.rs` and `cbm.rs`, both
```rust
    let title = format!("Copying out of {}", file.display());
```
become
```rust
    let title = JobTitle::new("components.jobBar.title.copyOutOf").text("source", &file.display());
```
A2 — `archive.rs`:
```rust
    let title = format!("Copying an archive into {}", image.display());
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.copyArchiveInto").text("target", &image.display());
```
A4 — `iso.rs`:
```rust
    let title = format!("Copying out of {}", iso_path.display());
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.copyOutOf").text("source", &iso_path.display());
```
A5 — `iso.rs`:
```rust
    let title = format!("Copying a disc into {}", image.display());
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.copyDiscInto").text("target", &image.display());
```
A6 — `panel.rs`:
```rust
    let title = format!("Counting {key}");
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.countFolder").text("path", &key);
```
A7 — `panel.rs`:
```rust
    let title = format!("Deleting {} item(s) from {}", names.len(), dir.display());
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.deleteItems")
        .count(names.len())
        .text("source", &dir.display());
```
A8 — `volume.rs`:
```rust
    let title = format!("Counting block {block} in {}", entry.name);
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.countVolumeBlock")
        .text("block", &block)
        .text("volume", &entry.name);
```
A9 — `volume_write.rs`:
```rust
    let title = format!("Copying into {}", image.display());
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.copyInto").text("target", &image.display());
```
A10 — `volume_write.rs`:
```rust
    let title = format!("Copying a selection into {}", image.display());
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.copySelectionInto").text("target", &image.display());
```
A11 — `volume_write.rs`:
```rust
    let title = format!("Copying out of {}", image.display());
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.copyOutOf").text("source", &image.display());
```
A12 — `volume_write.rs`:
```rust
    let title = format!("Copying a selection out of {}", image.display());
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.copySelectionOutOf").text("source", &image.display());
```
A13 — `volume_write.rs`:
```rust
    let title = format!("Copying a selection between {from_path} and {to_path}");
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.copySelectionBetween")
        .text("source", &from_path)
        .text("target", &to_path);
```

- [ ] **Step 4: Format, compile, clippy**

```bash
export TMP='E:\amiga\ProjeART\tmp' TEMP='E:\amiga\ProjeART\tmp'
cd /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri && cargo fmt && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --lib jobs::
```
Expected:
- `fmt --check` prints nothing.
- clippy finishes with no warnings. rustfmt may re-wrap a long `let title` over lines; the scan reads up to `;`, so that is fine.
- `test result: ok.`

- [ ] **Step 5: Count the moved sites, and see the test pass**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && rg --count-matches "JobTitle::new\(" src-tauri/src/commands -g "!jobs.rs"
```
Expected, in any order: `archive.rs:2`, `cbm.rs:1`, `iso.rs:2`, `panel.rs:2`, `volume.rs:1`, `volume_write.rs:5` (13).
```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && rg -c "let title = format!" src-tauri/src/commands
```
Expected: the per-file counts add up to **20** (33 before this batch).
```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && pnpm vitest run src/i18n/job-title-keys.test.ts
```
Expected: `Tests  8 passed (8)`.

- [ ] **Step 6: Mutate a key and a value, and see each guard fail alone**

Back up `cbm.rs` to `/d/Projeler/Amiga/scratch-0913/art301-bak/cbm.rs`.
- M1: in `cbm.rs`, change `components.jobBar.title.copyOutOf` to `components.jobBar.title.copyOutOff`. Run the vitest command above. Expected: only `names only keys the list holds` fails, with `commands\cbm.rs → components.jobBar.title.copyOutOff`. Restore with `cp` from the backup.
- M2: in `cbm.rs`, delete `.text("source", &file.display())` from that site. Run again. Expected: only `passes exactly the values its sentence interpolates` fails, with `commands\cbm.rs → components.jobBar.title.copyOutOf: passes [], the sentence names [source]`. Restore with `cp`.

Re-run: 8 passed. Record both in the evidence log.

- [ ] **Step 7: Commit**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && git branch --show-current && git add src/i18n/job-title-keys.test.ts src-tauri/src/commands/archive.rs src-tauri/src/commands/cbm.rs src-tauri/src/commands/iso.rs src-tauri/src/commands/panel.rs src-tauri/src/commands/volume.rs src-tauri/src/commands/volume_write.rs && git commit -F /d/Projeler/Amiga/scratch-0913/commit-msg-art301.txt
```
Message:
```
ART-301: the Files screen's 13 jobs name themselves by key

job-title-keys.test.ts reads the Rust tree: every JobTitle::new names a
listed key as a literal in its own let statement, and passes exactly the
values its sentence interpolates.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01KZGHnMHaXd1F9eYT5G6ieE
```

---

### Task 4: Batch B, the OS Builder and the card (10 sites)

**Files:**
- Modify: `src-tauri/src/commands/osinstall.rs`, `amigainstall.rs`, `firstboot.rs`, `appearance.rs`, `preload.rs`, `card.rs`

**Interfaces:**
- Consumes: `JobTitle` (Task 1). The site rule and `src/i18n/job-title-keys.test.ts` (Task 3): a `let title = JobTitle::new("<literal>")…;` statement, with `title` passed by value.

- [ ] **Step 1: Confirm the starting count**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && rg -c "let title = format!" src-tauri/src/commands
```
Expected: the counts add up to 20, including `osinstall.rs:5`, `amigainstall.rs:1`, `firstboot.rs:1`, `preload.rs:1`, `card.rs:1`.

- [ ] **Step 2: Move batch B**

Add `use crate::core::jobs::JobTitle;` to each of the six files, or add `JobTitle` to an existing `use crate::core::jobs::{…}`. In each site the `&title` of the spawn call right after it becomes `title`. In `osinstall.rs`, `amigainstall.rs` and `firstboot.rs` that argument sits on its own line as `        &title,`, which becomes `        title,`.

B1 — `osinstall.rs`:
```rust
    let title = format!("Identifying media in {}", folder.display());
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.identifyMedia").text("source", &folder.display());
```
B2 — `osinstall.rs`:
```rust
    let title = format!(
        "Previewing {} component(s) of {}",
        components.len(),
        plan.release
    );
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.previewComponents")
        .count(components.len())
        .text("release", &plan.release);
```
B3 — `osinstall.rs`:
```rust
    let title = format!(
        "Previewing {} package(s) against {}",
        ordered.len(),
        tree_root.display()
    );
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.previewPackages")
        .count(ordered.len())
        .text("target", &tree_root.display());
```
B4 — `osinstall.rs`:
```rust
    let title = format!("Adding {} package(s) to {for_log}", resolved.len());
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.addPackages")
        .count(resolved.len())
        .text("target", &for_log);
```
B5 — `osinstall.rs`:
```rust
    let title = format!("Installing {} into {destination}", request.plan.release);
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.installRelease")
        .text("release", &request.plan.release)
        .text("target", &destination);
```
B6 — `amigainstall.rs`:
```rust
    let title = format!("Installing {} on the Amiga", composed.package_name);
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.installOnAmiga").text("name", &composed.package_name);
```
B7 — `firstboot.rs`:
```rust
    let title = format!("Rehearsing the first boot on {}", profile.name);
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.rehearseFirstBoot").text("machine", &profile.name);
```
B8 — `appearance.rs`, a fixed string:
```rust
    let id = spawn_job(
        &app,
        Arc::clone(&registry),
        "Applying appearance to distribution tree",
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.applyAppearance");
    let id = spawn_job(
        &app,
        Arc::clone(&registry),
        title,
```
B9 — `preload.rs`:
```rust
    let title = format!("Preparing {} volume(s) on {image}", made.formats());
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.preparePartitions")
        .count(made.formats())
        .text("target", &image);
```
B10 — `card.rs`:
```rust
    let title = format!("Building {dest}");
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.buildCard").text("target", &dest);
```

- [ ] **Step 3: Format, compile, clippy, runner tests**

```bash
export TMP='E:\amiga\ProjeART\tmp' TEMP='E:\amiga\ProjeART\tmp'
cd /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri && cargo fmt && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --lib jobs::
```
Expected: `fmt --check` prints nothing, clippy is clean, and the run ends `test result: ok.`

- [ ] **Step 4: Count, and run the key test**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && rg --count-matches "JobTitle::new\(" src-tauri/src/commands -g "!jobs.rs"
```
Expected: batch A's six files unchanged, plus `osinstall.rs:5`, `amigainstall.rs:1`, `firstboot.rs:1`, `appearance.rs:1`, `preload.rs:1`, `card.rs:1` (23 in all).
```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && rg -c "let title = format!" src-tauri/src/commands
```
Expected: the counts add up to **11**.
```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && pnpm vitest run src/i18n/job-title-keys.test.ts
```
Expected: `Tests  8 passed (8)`. If a key is mistyped, `names only keys the list holds` names the file and the key. If a value is wrong, `passes exactly the values …` names the file, what was passed and what the sentence names.

- [ ] **Step 5: Commit**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && git branch --show-current && git add src-tauri/src/commands/osinstall.rs src-tauri/src/commands/amigainstall.rs src-tauri/src/commands/firstboot.rs src-tauri/src/commands/appearance.rs src-tauri/src/commands/preload.rs src-tauri/src/commands/card.rs && git commit -F /d/Projeler/Amiga/scratch-0913/commit-msg-art301.txt
```
Message:
```
ART-301: the OS Builder's and the card's 10 jobs name themselves by key

Includes the owner's "Adding 1 package(s) to ..." (osinstall.rs), now a
count-pluralised addPackages.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01KZGHnMHaXd1F9eYT5G6ieE
```

---

### Task 5: Batch C, packages, archives and layout (11 sites)

**Files:**
- Modify: `src-tauri/src/commands/sources.rs`, `bundles.rs`, `archives.rs`, `whdload.rs`, `layout.rs`

**Interfaces:**
- Consumes: `JobTitle` (Task 1), and the site rule and key test (Task 3).

- [ ] **Step 1: Confirm the starting count**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && rg -c "let title = format!" src-tauri/src/commands
```
Expected: the counts add up to 11, including `sources.rs:4`, `bundles.rs:1`, `archives.rs:2`, `whdload.rs:1`, `layout.rs:2`.

- [ ] **Step 2: Move batch C**

Add the `JobTitle` import to each of the five files as in the earlier batches. In each site the `&title` of the spawn call after it becomes `title`.

C1 — `sources.rs`, a fixed string:
```rust
    let id = spawn_job(
        &app,
        registry,
        "Syncing the Aminet catalog",
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.syncAminet");
    let id = spawn_job(
        &app,
        registry,
        title,
```
C2 — `sources.rs`:
```rust
    let title = format!("Downloading {}", meta.name);
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.downloadPackage").text("name", &meta.name);
```
C3 — `sources.rs`:
```rust
    let title = format!(
        "Installing {}",
        archive_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| archive.clone())
    );
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.installArchive").text(
        "name",
        &archive_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| archive.clone()),
    );
```
C4 — `sources.rs`:
```rust
    let title = format!("Reading {}", meta.name);
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.readReadme").text("name", &meta.name);
```
C5 — `sources.rs`:
```rust
    let title = format!(
        "Installing {} into {}",
        archive_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        image_path.display()
    );
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.installArchiveInto")
        .text(
            "name",
            &archive_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
        )
        .text("target", &image_path.display());
```
C6 — `bundles.rs`:
```rust
    let title = format!(
        "Downloading {} package{}",
        entries.len(),
        if entries.len() == 1 { "" } else { "s" }
    );
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.downloadPackages").count(entries.len());
```
C7 — `archives.rs`:
```rust
    let title = format!("Planning {} archives", archives.len());
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.planArchives").count(archives.len());
```
C8 — `archives.rs`:
```rust
    let title = format!(
        "Installing {} archives into {}",
        archives.len(),
        image_path.display()
    );
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.installArchivesInto")
        .count(archives.len())
        .text("target", &image_path.display());
```
C9 — `whdload.rs`:
```rust
    let title = format!(
        "Installing {}",
        archive_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default()
    );
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.installArchive").text(
        "name",
        &archive_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
    );
```
C10 — `layout.rs`:
```rust
    let title = format!("Working out what {} sources need", request.paths.len());
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.planLayout").count(request.paths.len());
```
C11 — `layout.rs`:
```rust
    let title = format!("Laying {} item(s) out in {root}", plan.items.len());
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.applyLayout")
        .count(plan.items.len())
        .text("target", &root);
```

- [ ] **Step 3: Format, compile, clippy, runner tests**

```bash
export TMP='E:\amiga\ProjeART\tmp' TEMP='E:\amiga\ProjeART\tmp'
cd /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri && cargo fmt && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --lib jobs::
```
Expected: `fmt --check` prints nothing, clippy is clean, and the run ends `test result: ok.`

- [ ] **Step 4: Count, and run the key test**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && rg --count-matches "JobTitle::new\(" src-tauri/src/commands -g "!jobs.rs"
```
Expected: the earlier files unchanged, plus `sources.rs:5`, `bundles.rs:1`, `archives.rs:2`, `whdload.rs:1`, `layout.rs:2` (34 in all).
```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && rg -c "let title = format!" src-tauri/src/commands
```
Expected: exactly one line, `src-tauri/src/commands/gameindex.rs:1`.
```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && pnpm vitest run src/i18n/job-title-keys.test.ts
```
Expected: `Tests  8 passed (8)`.

- [ ] **Step 5: Commit**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && git branch --show-current && git add src-tauri/src/commands/sources.rs src-tauri/src/commands/bundles.rs src-tauri/src/commands/archives.rs src-tauri/src/commands/whdload.rs src-tauri/src/commands/layout.rs && git commit -F /d/Projeler/Amiga/scratch-0913/commit-msg-art301.txt
```
Message:
```
ART-301: package, archive and layout jobs name themselves by key

Eleven sites; bundles.rs no longer builds "package{s}" by hand.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01KZGHnMHaXd1F9eYT5G6ieE
```

---

### Task 6: Batch D, the Collection (6 sites)

**Files:**
- Modify: `src-tauri/src/commands/artwork.rs`, `gameindex.rs`

**Interfaces:**
- Consumes: `JobTitle` (Task 1), and the site rule and key test (Task 3).

- [ ] **Step 1: Move batch D**

Add the `JobTitle` import to both files.

D1 — `artwork.rs`:
```rust
    let id = spawn_job(
        &app,
        registry,
        "Fetching artwork",
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.fetchArtwork");
    let id = spawn_job(
        &app,
        registry,
        title,
```
D2 — `artwork.rs`:
```rust
    let id = spawn_job(
        &app,
        registry,
        "Reading pictures from your files",
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.readLocalPictures");
    let id = spawn_job(
        &app,
        registry,
        title,
```
D3 — `artwork.rs`:
```rust
    let id = spawn_job(
        &app,
        registry,
        "Restoring your own pictures",
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.restorePictures");
    let id = spawn_job(
        &app,
        registry,
        title,
```
D4 — `gameindex.rs`:
```rust
    let id = spawn_job(
        &app,
        registry,
        "Refreshing the catalogue",
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.refreshCatalogue");
    let id = spawn_job(
        &app,
        registry,
        title,
```
D5 — `gameindex.rs`:
```rust
    let id = spawn_job(
        &app,
        registry,
        "Indexing titles",
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.indexTitles");
    let id = spawn_job(
        &app,
        registry,
        title,
```
D6 — `gameindex.rs`:
```rust
    let title = format!("Writing igame.data for {} title(s)", plan.items.len());
```
→
```rust
    let title = JobTitle::new("components.jobBar.title.writeIgameData").count(plan.items.len());
```
and the `&title` of the spawn call after it becomes `title`.

`artwork.rs` has a test-module `let title = "The Settlers";` (line 668) in another scope, and it is not a job. Leave it. Check that none of the five functions already binds a `title`; if the compiler reports a shadowing clash, stop and report it rather than renaming, because the key test requires the name `title`.

- [ ] **Step 2: Format, compile, clippy, runner tests**

```bash
export TMP='E:\amiga\ProjeART\tmp' TEMP='E:\amiga\ProjeART\tmp'
cd /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri && cargo fmt && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --lib jobs::
```
Expected: `fmt --check` prints nothing, clippy is clean, and the run ends `test result: ok.`

- [ ] **Step 3: Count, and run the key test**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && rg --count-matches "JobTitle::new\(" src-tauri/src/commands -g "!jobs.rs"
```
Expected: 19 files adding up to **40**:
- `archive.rs:2`, `cbm.rs:1`, `iso.rs:2`, `panel.rs:2`, `volume.rs:1`, `volume_write.rs:5`
- `osinstall.rs:5`, `amigainstall.rs:1`, `firstboot.rs:1`, `appearance.rs:1`, `preload.rs:1`, `card.rs:1`
- `sources.rs:5`, `bundles.rs:1`, `archives.rs:2`, `whdload.rs:1`, `layout.rs:2`
- `artwork.rs:3`, `gameindex.rs:3`

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && rg -n "let title = format!" src-tauri/src/commands
```
Expected: no output (`rg` exits 1 on no match, which here is the success case; do not chain another command after it).
```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && pnpm vitest run src/i18n/job-title-keys.test.ts
```
Expected: `Tests  8 passed (8)`.

- [ ] **Step 4: Commit**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && git branch --show-current && git add src-tauri/src/commands/artwork.rs src-tauri/src/commands/gameindex.rs && git commit -F /d/Projeler/Amiga/scratch-0913/commit-msg-art301.txt
```
Message:
```
ART-301: the Collection's 6 jobs name themselves by key

All 40 job titles are JobTitle::new("components.jobBar.title.*") now.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01KZGHnMHaXd1F9eYT5G6ieE
```

---

### Task 7: Delete the bridge; the list and the Rust tree hold each other both ways

**Files:**
- Modify: `src-tauri/src/commands/jobs.rs` (bridge block, the three spawn signatures)
- Modify: `src-tauri/src/core/jobs/mod.rs` (`untranslated`)
- Modify: `src/i18n/job-title-keys.test.ts` (the `the job titles Rust sets` describe block)

**Interfaces:**
- Produces:
  - `spawn_job(app, registry, title: JobTitle, work)`
  - `spawn_job_in_lane(app, registry, title: JobTitle, lane, work)`
  - No `From<&str> for JobTitle`, and no `JobTitle::untranslated`.

- [ ] **Step 1: Pin the count and the reverse direction in the key test**

In `src/i18n/job-title-keys.test.ts`, replace
```ts
  it("finds the Rust sites", () => {
    // A moved folder or a renamed type would make every assertion below
    // vacuously true.
    expect(RUST.sites.length).toBeGreaterThan(0);
  });
```
with
```ts
  it("finds all forty sites", () => {
    // 40 on 2026-09-14: 33 titles that were `format!` and 7 that were fixed
    // strings, in 19 command files. A new job moves this number on purpose.
    // A moved folder or a renamed type moves it to 0.
    expect(
      RUST.sites.length,
      "The number of JobTitle::new(…) sites in src-tauri/src changed. If you added or removed a " +
        "background job on purpose, change 40 here and in the comment above in the same commit, " +
        "with its key in JOB_TITLE_KEYS and both catalogues. If you did not, a site was lost."
    ).toBe(40);
  });

  it("leaves no listed key without a Rust site", () => {
    // The other direction: a key whose job was removed or renamed would stay
    // in both catalogues, reachable only through this list, and never shown.
    const named = new Set(RUST.sites.map((s) => s.key));
    expect(JOB_TITLE_KEYS.filter((key) => !named.has(key))).toEqual([]);
  });
```
Run:
```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && pnpm vitest run src/i18n/job-title-keys.test.ts
```
Expected: `Tests  9 passed (9)`.

- [ ] **Step 2: See the reverse direction fail on a mutation**

Back up `src-tauri/src/commands/sources.rs` to `/d/Projeler/Amiga/scratch-0913/art301-bak/sources.rs`. In it, change `components.jobBar.title.syncAminet` to `components.jobBar.title.fetchArtwork`; both take no values, so only one thing changes. Run the vitest command above.

Expected: only `leaves no listed key without a Rust site` fails, with `[ "components.jobBar.title.syncAminet" ]`. Restore:
```bash
cp /d/Projeler/Amiga/scratch-0913/art301-bak/sources.rs /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri/src/commands/sources.rs
```
Re-run: 9 passed. Record it in the evidence log.

- [ ] **Step 3: Delete the bridge**

In `src-tauri/src/commands/jobs.rs`:
- Delete the whole block from `// ART-301 migration bridge — Task 7 of the plan deletes this block` through the closing `}` of `impl From<&String> for JobTitle`, including its separator comment lines and the blank line after it.
- In `spawn_job`, replace `    title: impl Into<JobTitle>,` with `    title: JobTitle,`, and `spawn_in_lane(app, registry, title.into(), None, work)` with `spawn_in_lane(app, registry, title, None, work)`.
- In `spawn_job_in_lane`, replace `    title: impl Into<JobTitle>,` with `    title: JobTitle,`, and `spawn_in_lane(app, registry, title.into(), Some(lane), work)` with `spawn_in_lane(app, registry, title, Some(lane), work)`.

In `src-tauri/src/core/jobs/mod.rs`, delete the `untranslated` function with its doc comment (from `    /// **ART-301 migration bridge` to its closing `    }`).

- [ ] **Step 4: Compile, and prove nothing is left**

```bash
export TMP='E:\amiga\ProjeART\tmp' TEMP='E:\amiga\ProjeART\tmp'
cd /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri && cargo fmt && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --lib jobs::
```
Expected: clippy is clean and the run ends `test result: ok.`
```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && rg -n "untranslated|From<&str> for JobTitle|From<&String> for JobTitle|title\.into\(\)" src-tauri/src
```
Expected: no output.

- [ ] **Step 5: See the compiler refuse a sentence**

Back up `src-tauri/src/commands/sources.rs` again. Change the `syncAminet` site to pass the old string: replace the `let title = JobTitle::new("components.jobBar.title.syncAminet");` line with `let title = "Syncing the Aminet catalog";`. Run:
```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri && cargo check
```
Expected: `error[E0308]: mismatched types` (`expected `JobTitle`, found `&str``) at that spawn call. Restore with `cp` from the backup, then run `cargo check`: `Finished`. Record it in the evidence log.

- [ ] **Step 6: Commit**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && git branch --show-current && git add src-tauri/src/commands/jobs.rs src-tauri/src/core/jobs/mod.rs src/i18n/job-title-keys.test.ts && git commit -F /d/Projeler/Amiga/scratch-0913/commit-msg-art301.txt
```
Message:
```
ART-301: spawn_job takes a JobTitle only; the bridge is gone

The key test pins all 40 sites and fails on a listed key no Rust site
names; the compiler now refuses a sentence where a title belongs.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01KZGHnMHaXd1F9eYT5G6ieE
```

---

### Task 8: Records, full verification, whole-branch review

**Files:**
- Modify: `docs/ISSUES.md` (ART-301 Open → Fixed), `docs/architecture.md:834-836`, `docs/FEATURES.md:362`, `CHANGELOG.md` (`## [Unreleased]`), `docs/session-log.md` (table top), `docs/STATUS.md` (Snapshot rows, `## Picking up next session`)

**Interfaces:**
- Consumes: the evidence log `D:\Projeler\Amiga\scratch-0913\art301-evidence.md`, the base SHA in `D:\Projeler\Amiga\scratch-0913\art301-base.txt`.

- [ ] **Step 1: Full verification, before writing a number anywhere**

Run each command separately and unpiped, and keep its final lines:
```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && pnpm lint
cd /d/Projeler/Amiga/amiga-retro-toolkit && pnpm test
export TMP='E:\amiga\ProjeART\tmp' TEMP='E:\amiga\ProjeART\tmp'
cd /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri && cargo fmt --check
cd /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri && cargo clippy --all-targets -- -D warnings
cd /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri && cargo test --lib
cd /d/Projeler/Amiga/amiga-retro-toolkit/src-tauri && cargo test --lib
cd /d/Projeler/Amiga/amiga-retro-toolkit && python scripts/control-byte-sweep.py
```
Expected:
- `pnpm lint`: no `error TS`.
- `pnpm test`: `Test Files  N passed (N)` and `Tests  M passed (M)`, with 0 failed.
- `fmt --check`: silent. Clippy: clean.
- Both suite runs: `test result: ok. X passed; 0 failed; Y ignored; 0 measured; 0 filtered out`. X is 6 more than the branch's count before Task 1, unless another debt plan also added tests.
- The sweep is clean.

Write every final line into the evidence log under `Task 8 verification`. Count the catalogue leaves:
```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && node -e "const f=o=>typeof o==='object'?Object.values(o).reduce((a,v)=>a+f(v),0):1;console.log(f(require('./src/i18n/en.json')),f(require('./src/i18n/tr.json')))"
```
Expected: two equal numbers, 47 more than before Task 2.

- [ ] **Step 2: ISSUES — move ART-301 to Fixed**

Cut the ART-301 entry out of `## Open`: the paragraph that starts `**ART-301** 🔵 **The job bar's titles are English sentences composed in Rust**` and ends `A round of its own.` Directly under the `## Fixed` heading and its blank line, insert:
```markdown
**ART-301** 🔵 ✅ **The job bar's titles were English sentences composed in Rust** — *found 2026-09-10 in the
owner's screenshot; fixed 2026-09-14 on `art-debt-0914`*
`src-tauri/src/core/jobs/mod.rs` (`JobTitle`) · `src-tauri/src/commands/jobs.rs` · `src/lib/jobs.ts`
(`JOB_TITLE_KEYS`) · `src/components/JobBar.tsx` · The owner saw *"Adding 1 package(s) to …"* twice on a Turkish
screen. **Corrected count:** there were 40 titles in 19 command files, not 34 in 18 as this entry first said —
33 built with `format!`, 7 fixed strings. A job now carries a `JobTitle`: a catalogue key under
`components.jobBar.title.*` and the values its sentence names, serialised to the TypeScript `Phrase` shape.
The job bar renders it with `t()`. A count goes in as `count`, so i18next picks `_one` / `_other` and no title
builds `(s)` by hand. Paths, names and releases pass through untranslated. That makes 36 keys and 47 leaves in each
catalogue. `spawn_job` takes a `JobTitle` and nothing else, so a sentence no longer compiles there. Unchanged
on purpose: the refusal under a failed job is still the core's English sentence (ART-060), and the operation
log's record names are not titles.
Guards: `core::jobs::tests::{a_job_title_serializes_to_the_phrase_shape,
a_count_is_sent_as_a_number_named_count, a_title_without_values_sends_no_params_field,
a_job_progress_sends_its_title_as_a_phrase_not_a_sentence, a_title_reads_back_as_itself}`,
`commands::jobs::tests::the_registry_keeps_the_title_it_was_given`; `src/components/JobBar.test.tsx` (*names a
job in Turkish when Turkish is chosen*, *picks the plural from count, not from a hand-built (s)*);
`src/i18n/job-title-keys.test.ts`, which reads the Rust tree. It checks both catalogues, literal keys in
their own `let title` statement, exactly the values each sentence interpolates, all 40 sites, and no listed
key without a site.
```
Directly after it, append the red lines and mutation results from the evidence log (Tasks 1, 2, 3, 7), verbatim, as:
```markdown
Red before the fix: <each recorded red line, one per bullet, with its task>. Mutations put back and seen to
fail: <each recorded mutation and the test that failed>; no survivor.
```
Only lines actually recorded go there. If a mutation survived, write that too, and say whether the guard was weak or the mutation was wrong (testing.md).

- [ ] **Step 3: architecture.md — the rule now has a Rust-side case**

In `docs/architecture.md`, replace
```markdown
Rust-side strings (`CoreError` messages, `WhdloadRefusal.reason` /
`.suggestion`) are not in this system yet and stay English whatever the chosen
language — ART-060.
```
with
```markdown
A background job's title is the one place Rust names a catalogue key itself:
`JobTitle::new("components.jobBar.title.…")` with `.text(name, value)` and
`.count(n)`, serialised to the `Phrase` shape and rendered by `JobBar.tsx`
(ART-301). The keys are written out in `JOB_TITLE_KEYS` (`src/lib/jobs.ts`).
`src/i18n/job-title-keys.test.ts` reads the Rust sources and holds the two to
each other in both directions, including the values each sentence
interpolates. A new job adds its key there and to both catalogues.

Other Rust-side strings (`CoreError` messages, `WhdloadRefusal.reason` /
`.suggestion`) are not in this system yet and stay English whatever the chosen
language — ART-060.
```

- [ ] **Step 4: FEATURES — the i18n row names the change, and stays amber**

In `docs/FEATURES.md`, in the row that starts `| i18n architecture |`, replace
```markdown
`CoreError` and `WhdloadRefusal` sentences from Rust still reach the UI in English regardless of the chosen language ([ART-060](ISSUES.md#fixed))
```
with
```markdown
`CoreError` and `WhdloadRefusal` sentences from Rust still reach the UI in English regardless of the chosen language ([ART-060](ISSUES.md#fixed)); background-job titles are catalogue keys in both languages since [ART-301](ISSUES.md#fixed) (`src/i18n/job-title-keys.test.ts`)
```
The row keeps 🟡, because `CoreError` is still English.

- [ ] **Step 5: CHANGELOG**

Under `## [Unreleased]` in `CHANGELOG.md`, add the bullet below to a `### Fixed` section. If one already exists there (another debt plan may have added it), add to it; otherwise create it:
```markdown
- **The job bar names each job in the language you chose.** Titles such as
  "Adding 1 package(s) to …" were English on a Turkish screen and counted
  with "(s)"; they now read, for example, "Work.hdf içine 1 paket ekleniyor",
  or "Adding 1 package to Work.hdf" in English. A failed job's reason is
  still in English.
```

- [ ] **Step 6: session-log and STATUS**

In `docs/session-log.md`, insert a row directly under `|---|---|---|`:
```markdown
| 2026-09-14 | **ART-301 fixed on `art-debt-0914`: job titles are catalogue keys.** `JobTitle` in `core/jobs` (the `Phrase` shape), 40 sites in 19 command files moved in four batches behind a bridge Task 7 deleted, 36 keys / 47 leaves in each catalogue, `src/i18n/job-title-keys.test.ts` reading the Rust tree both ways. Plan `docs/superpowers/plans/2026-09-14-debt-4-art-301-job-titles.md`. | Rust <X> / 0 / <Y> ignored ×2; Vitest <N> / <M> |
```
Replace `<X>`, `<Y>`, `<N>`, `<M>` with the numbers from Step 1.

In `docs/STATUS.md`:
- **Snapshot → Tests — Rust:** prepend `` `cd src-tauri && cargo test --lib` on `art-debt-0914` after ART-301, 2026-09-14: `<the exact test result line>` (twice, `TMP`/`TEMP` on `E:`; clippy clean). Before that, `` to the existing text.
- **Tests — frontend:** prepend `` `pnpm test` on `art-debt-0914` after ART-301, 2026-09-14: `<N>` files / `<M>` tests passed, `pnpm lint` clean. Before that, ``.
- **i18n:** prepend `**<count>** leaf strings in each of `en.json` and `tr.json`, counted 2026-09-14 on `art-debt-0914` (ART-301 added 47). Before that, `.
- **Open defects:** count with `awk '/^## Open/{f=1} /^## Fixed/{f=0} f' docs/ISSUES.md | grep -c '^\*\*ART-'` (read-only; if it errors, count by eye) and prepend the number with `ART-301 moved to Fixed`.
- **Picking up next session:** under `### Start here`, change the heading's date to `2026-09-14`. Add one item at the top of the list, numbered with one more leading `0` than the current top item:
  ```markdown
  **ART-301 is fixed on `art-debt-0914`, not merged: the job bar names every job in the chosen language.** A job carries a `JobTitle` (a catalogue key and its values) instead of an English sentence; all 40 sites moved, and `src/i18n/job-title-keys.test.ts` holds `JOB_TITLE_KEYS` and the Rust tree to each other both ways. ISSUES has the guards, red lines and mutations. **Owed by a person:** with Turkish chosen, start a job (Files: copy a folder into an ADF, or add a package on the OS Builder) and read the bar. **Merging is the owner's word.**
  ```

Run `python scripts/control-byte-sweep.py` again. Expected: clean.

- [ ] **Step 7: Commit the records**

```bash
cd /d/Projeler/Amiga/amiga-retro-toolkit && git branch --show-current && git add docs/ISSUES.md docs/architecture.md docs/FEATURES.md CHANGELOG.md docs/session-log.md docs/STATUS.md && git commit -F /d/Projeler/Amiga/scratch-0913/commit-msg-art301.txt
```
Message:
```
docs: ART-301 fixed - job titles are catalogue keys

ISSUES entry moved to Fixed with its guards, red lines and mutations;
architecture's strings rule names the Rust-side case; FEATURES, CHANGELOG,
session log and STATUS updated with the measured counts.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01KZGHnMHaXd1F9eYT5G6ieE
```

- [ ] **Step 8: Whole-branch review of this round**

Load the `superpowers:requesting-code-review` skill and request a review of `$(cat /d/Projeler/Amiga/scratch-0913/art301-base.txt)..HEAD`, limited to the files this plan touched. Ask the reviewer to check specifically:
1. Every one of the 40 sites passes the value the old sentence used, with nothing dropped. Compare with this plan's site table.
2. No Turkish sentence attaches a suffix to a placeholder.
3. No `JobTitle` construction bypasses `JobTitle::new`.
4. `JobBar.tsx` is the only reader of `title`: re-grep `\.title\b` over `src/`.
5. The operation log's `user_operation("…")` names are unchanged.
6. The ISSUES entry claims nothing the evidence log does not show.

Fix what the review finds, re-run the affected tests from Step 1, and commit the fixes. If a fix changes code, run `cargo test --lib` twice again and update the numbers in STATUS and session-log. Merging is the owner's decision; do not merge.
