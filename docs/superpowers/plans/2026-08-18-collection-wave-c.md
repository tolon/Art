# Collection wave C Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render the 242 pictures ART already holds, let the user attach their
own, open a title in a detail panel, and launch it in WinUAE — including
WHDLoad drawers.

**Architecture:** Four parts, each landing something visible. An offline
artwork pass extracts a `.rp9`'s embedded preview through `core/archive`'s
security gate into the existing artwork cache, so no screen needs a new
rendering path. A hand-attached picture splits in two: bytes in the cache
(derived, rebuildable), the *choice* in `core/gameindex`'s user layer (which no
refresh touches). A detail panel on the Collection screen carries what a card
cannot and hosts the new actions. `core/launch/` turns a catalogued record into
a `LaunchPlan` — profile, ROM, media, and for WHDLoad the contents of a boot
directory ART owns — and `commands/` hands that to the existing
`core::winuae::launch_winuae`.

**Tech Stack:** Rust (`std` + `serde` + `zip` via `core/archive`), Tauri
commands, React + TypeScript, `react-i18next`, Vitest, `cargo test`.

**Spec:** [../specs/2026-08-18-collection-wave-c-design.md](../specs/2026-08-18-collection-wave-c-design.md)

## Global Constraints

- **`core/` is platform-independent.** No `use tauri`, no Windows APIs, no
  network. Anything platform-specific is a trait implemented outside `core/`.
  `core/launch` produces a plan; it starts no process.
- **A lower `core/` module must not import a higher one.** `core/launch`
  declares its **own** ROM record and `commands/` maps `RomInfo` into it — the
  shape `core/rom/pairing.rs` documents and `commands/preload.rs` performs.
- **Untrusted archive entries reach disk only through
  `core/security/path.rs::safe_join`.** Reads are bounded; a declared size is a
  claim, never a budget.
- **Every write goes through `core/safety`** — `atomic_write` for a file ART
  owns, `guarded_write` where a user file could be replaced. `std::fs::write`
  on a user file is forbidden.
- **New commands go in `invoke_handler![]` in `lib.rs` *and* get a typed
  wrapper in `src/lib/*.ts`.** Components never call `invoke` directly.
- **i18n:** every new key goes in **both** `src/i18n/en.json` and `tr.json` in
  the same commit; `pnpm test` fails on a key-set difference, an empty value or
  a mismatched interpolation variable.
- **`src/lib/*` never renders a string.** A helper returns
  `Phrase { key, params? }` and the component calls `t(phrase.key, …)`. Every
  variant of every new mapper is enumerated in `src/i18n/phrase-keys.test.ts`.
- **Settings are remembered.** Anything the user chooses — the panel's open
  state, the selected title, a per-title launch choice — goes through
  `src/lib/remembered.ts` with a guard (`isOneOf`, `isWholeNumberBetween`,
  `nullOr`).
- **Beginner mode hides, never disables.** No action is unavailable because of
  the mode.
- **Nothing may claim what ART cannot know** (spec §89). A refusal states the
  reason; an unavailable action is registered rather than hidden.
- **Verify with:** `pnpm lint`, `pnpm test`, `cd src-tauri && cargo fmt --check
  && cargo clippy --all-targets -- -D warnings && cargo test` (twice — ART-059),
  and `python scripts/contrast-check.py --quiet` for any colour touched.

---

## File Structure

**Created:**

| File | Responsibility |
|---|---|
| `src-tauri/src/core/artwork/local.rs` | The offline pass: package + entry name → cached picture. No client, no network. |
| `src-tauri/src/core/launch/mod.rs` | `LaunchPlan`, `LaunchRom`, `PlanRefusal`, and `plan_for()` |
| `src-tauri/src/core/launch/extract.rs` | `.rp9` floppies out of the package into ART's launch directory |
| `src-tauri/src/core/launch/whdload_boot.rs` | The Y2 boot directory: `S/Startup-Sequence` for one slave |
| `src-tauri/src/commands/launch.rs` | Maps records + settings + `RomInfo` into a plan, runs it, logs it |
| `src/lib/launch.ts` | Typed wrappers and the plan's `Phrase` mappers |
| `src/lib/collectionDetail.ts` | The panel's pure logic: which kinds a title has, how its media reads |
| `src/components/collection/TitleDetail.tsx` | The panel itself |

**Modified:**

| File | Change |
|---|---|
| `src-tauri/src/core/artwork/mod.rs` | `pub mod local;` |
| `src-tauri/src/core/artwork/enrich.rs` | Skip a title whose picture the user pinned |
| `src-tauri/src/core/gameindex/store.rs` | `RecordOverride.art: Option<ArtBinding>` |
| `src-tauri/src/core/winuae.rs` | `LaunchMedia.directories: Vec<DirMount>` and its `filesystem2=` emission |
| `src-tauri/src/commands/artwork.rs` | `artwork_adopt_local`, `artwork_attach`, `artwork_detach` |
| `src-tauri/src/lib.rs` | `mod launch;` and the new commands in `invoke_handler![]` |
| `src/lib/artwork.ts`, `src/lib/gameindex.ts` | Wrappers and types for the above |
| `src/pages/CollectionStudio.tsx` | Selection state, the panel, the offline-pictures button |
| `src/i18n/en.json`, `src/i18n/tr.json` | New keys, both files, every commit |
| `src/i18n/literal-keys.test.ts` | The dynamic-call count, with its reason |
| `docs/STATUS.md`, `docs/FEATURES.md`, `CHANGELOG.md` | Task 12 |

---

## Task 1: The offline pass — a `.rp9` preview becomes a cached picture

**Files:**
- Create: `src-tauri/src/core/artwork/local.rs`
- Modify: `src-tauri/src/core/artwork/mod.rs` (add `pub mod local;`)
- Test: inline `#[cfg(test)]` in `local.rs`

**Interfaces:**
- Consumes: `core::archive::open`, `core::artwork::cache::Cache`,
  `core::artwork::ArtKind`, `core::jobs::ProgressSink`, `core::error::CoreError`.
- Produces:
  ```rust
  pub struct LocalPreview { pub title: String, pub package: PathBuf, pub entry: String }
  pub struct LocalOutcome { pub written: u32, pub adopted: u32, pub missed: u32 }
  pub fn adopt_local(cache_dir: &Path, previews: &[LocalPreview], sink: &dyn ProgressSink)
      -> CoreResult<LocalOutcome>;
  pub const SOURCE_ID: &str = "rp9";
  pub const MAX_PREVIEW_BYTES: u64 = 4 * 1024 * 1024;
  ```

- [ ] **Step 1: Write the failing tests**

Add to `src-tauri/src/core/artwork/local.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jobs::NoProgress;

    /// A `.rp9` is a zip. Only two entries matter here: the picture and
    /// something else to prove the right one is picked.
    fn package(dir: &Path, name: &str, entries: &[(&str, &[u8])]) -> PathBuf {
        use std::io::Write;
        let path = dir.join(name);
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (entry, bytes) in entries {
            zip.start_file(*entry, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("art-local-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_preview_inside_a_package_becomes_a_cached_picture() {
        let dir = scratch("adopt");
        let cache_dir = dir.join("cache");
        let pkg = package(
            &dir,
            "Turrican.rp9",
            &[
                ("rp9-manifest.xml", b"<application/>"),
                ("rp9-preview.png", b"PNGDATA"),
            ],
        );

        let outcome = adopt_local(
            &cache_dir,
            &[LocalPreview {
                title: "Turrican".into(),
                package: pkg,
                entry: "rp9-preview.png".into(),
            }],
            &NoProgress,
        )
        .unwrap();

        assert_eq!(outcome.written, 1);
        let cache = Cache::open(&cache_dir).unwrap();
        let art = cache.best("Turrican").expect("the cache holds it now");
        assert_eq!(art.source, "rp9");
        assert_eq!(art.kind, ArtKind::Snap);
        let bytes = std::fs::read(cache_dir.join(&art.file)).unwrap();
        assert_eq!(bytes, b"PNGDATA", "the picture's own bytes, not a placeholder");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The second run must not rewrite 242 files to reach the same place.
    #[test]
    fn a_second_pass_adopts_rather_than_rewrites() {
        let dir = scratch("second");
        let cache_dir = dir.join("cache");
        let pkg = package(&dir, "Agony.rp9", &[("rp9-preview.png", b"PNGDATA")]);
        let ask = || LocalPreview {
            title: "Agony".into(),
            package: pkg.clone(),
            entry: "rp9-preview.png".into(),
        };

        assert_eq!(adopt_local(&cache_dir, &[ask()], &NoProgress).unwrap().written, 1);
        let second = adopt_local(&cache_dir, &[ask()], &NoProgress).unwrap();

        assert_eq!(second.written, 0, "nothing was written the second time");
        assert_eq!(second.adopted, 1, "and the picture was found rather than lost");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The entry name comes out of a file somebody else made.
    #[test]
    fn an_entry_that_escapes_the_cache_is_refused() {
        let dir = scratch("traversal");
        let cache_dir = dir.join("cache");
        let pkg = package(
            &dir,
            "Evil.rp9",
            &[("../../evil.png", b"PNGDATA"), ("rp9-manifest.xml", b"<x/>")],
        );

        let outcome = adopt_local(
            &cache_dir,
            &[LocalPreview {
                title: "Evil".into(),
                package: pkg,
                entry: "../../evil.png".into(),
            }],
            &NoProgress,
        )
        .unwrap();

        assert_eq!(outcome.written, 0);
        assert_eq!(outcome.missed, 1, "refused, and counted as a miss");
        assert!(
            !dir.join("evil.png").exists() && !cache_dir.join("../../evil.png").exists(),
            "nothing was written outside the cache"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A catalogue can outlive the file it describes.
    #[test]
    fn a_package_that_is_no_longer_there_is_a_miss_and_not_an_error() {
        let dir = scratch("missing");
        let outcome = adopt_local(
            &dir.join("cache"),
            &[LocalPreview {
                title: "Gone".into(),
                package: dir.join("nothing-here.rp9"),
                entry: "rp9-preview.png".into(),
            }],
            &NoProgress,
        )
        .unwrap();

        assert_eq!(outcome.missed, 1);
        assert_eq!(outcome.written, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancelling_stops_between_packages_and_says_so() {
        struct StopAtOnce;
        impl ProgressSink for StopAtOnce {
            fn report(&self, _done: u64, _total: Option<u64>, _message: &str) {}
            fn is_cancelled(&self) -> bool {
                true
            }
        }

        let dir = scratch("cancel");
        let pkg = package(&dir, "One.rp9", &[("rp9-preview.png", b"PNGDATA")]);
        let err = adopt_local(
            &dir.join("cache"),
            &[LocalPreview {
                title: "One".into(),
                package: pkg,
                entry: "rp9-preview.png".into(),
            }],
            &StopAtOnce,
        )
        .unwrap_err();

        assert!(matches!(err, CoreError::Cancelled), "{err:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test artwork::local`
Expected: FAIL — `local.rs` does not exist yet, so this is a compile error
naming `adopt_local`.

- [ ] **Step 3: Write the implementation**

`src-tauri/src/core/artwork/local.rs`:

```rust
//! Pictures ART already has, and never asked anybody for (wave C).
//!
//! Every `.rp9` in the user's collection carries an embedded `screen-running`
//! PNG — 242 of 242 in the folder this was written against — and G10's reader
//! already records its **name inside the zip** as `GameRecord::preview`.
//! Rendering it is therefore an extraction, not a download.
//!
//! **Why this is not a source in the wave B sense.** `enrich()` takes a
//! `MirrorClient` because every source it knows fetches; this one must be
//! callable with none at all. Nothing leaves the machine, so nothing here asks
//! the user's permission, consults a configured mirror or can fail because a
//! host is down. It shares wave B's `Cache`, so every screen renders the
//! result through the path it already uses.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::archive::open;
use crate::core::artwork::cache::Cache;
use crate::core::artwork::ArtKind;
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;

/// The source id these pictures are cached under.
pub const SOURCE_ID: &str = "rp9";

/// A preview is a screenshot, not a disk image. Four megabytes is far above
/// any real one and far below anything that could exhaust memory — the same
/// reasoning `MAX_MANIFEST_BYTES` uses one module over.
pub const MAX_PREVIEW_BYTES: u64 = 4 * 1024 * 1024;

/// One picture to look for: which title it belongs to, which package holds it,
/// and what it is called in there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalPreview {
    pub title: String,
    pub package: PathBuf,
    pub entry: String,
}

/// What a pass managed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalOutcome {
    pub written: u32,
    /// Already on disk from an earlier pass.
    pub adopted: u32,
    /// No package, no such entry, or a name that would have escaped the cache.
    pub missed: u32,
}

/// Pull each preview out of its package and into the cache.
///
/// Never fails for one bad package: a catalogue outlives the files it
/// describes, and one missing `.rp9` must not cost the other 241 their
/// pictures. The only error it returns is [`CoreError::Cancelled`].
pub fn adopt_local(
    cache_dir: &Path,
    previews: &[LocalPreview],
    sink: &dyn ProgressSink,
) -> CoreResult<LocalOutcome> {
    let mut cache = Cache::open(cache_dir)?;
    let mut outcome = LocalOutcome::default();
    let total = previews.len() as u64;

    for (done, want) in previews.iter().enumerate() {
        // Between whole units of work, never mid-write.
        if sink.is_cancelled() {
            let _ = cache.save();
            return Err(CoreError::Cancelled);
        }
        sink.report(done as u64, Some(total), &want.title);

        if cache.adopt(&want.title, ArtKind::Snap, SOURCE_ID, "png").is_some() {
            outcome.adopted += 1;
            continue;
        }
        match extract(want) {
            Some(bytes) => {
                match cache.store(&want.title, ArtKind::Snap, SOURCE_ID, "png", &bytes) {
                    Ok(_) => outcome.written += 1,
                    Err(_) => outcome.missed += 1,
                }
            }
            None => outcome.missed += 1,
        }
    }

    sink.report(total, Some(total), "");
    cache.save()?;
    Ok(outcome)
}

/// The named entry's bytes, or `None` for every ordinary reason it might not
/// be there. `Cache::store` is what turns the title into a path, and it goes
/// through `safe_join`, so a hostile entry name is refused there rather than
/// sanitised here.
fn extract(want: &LocalPreview) -> Option<Vec<u8>> {
    if want.entry.contains("..") {
        return None;
    }
    let mut archive = open(&want.package).ok()?;
    let entries = archive.entries().ok()?;
    let index = entries.iter().position(|entry| {
        !entry.is_dir && entry.name.replace('\\', "/") == want.entry.replace('\\', "/")
    })?;
    archive.read(index, MAX_PREVIEW_BYTES).ok()
}
```

Then add to `src-tauri/src/core/artwork/mod.rs`, beside the other `pub mod`
lines:

```rust
pub mod local;
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test artwork::local`
Expected: PASS, 5 tests.

- [ ] **Step 5: Check formatting and lints**

Run: `cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/core/artwork/local.rs src-tauri/src/core/artwork/mod.rs
git commit -m "feat(artwork): the pictures already inside a .rp9, extracted into the cache"
```

---

## Task 2: Wire the offline pass to a button

**Files:**
- Modify: `src-tauri/src/commands/artwork.rs`, `src-tauri/src/lib.rs`,
  `src/lib/artwork.ts`, `src/pages/CollectionStudio.tsx`,
  `src/i18n/en.json`, `src/i18n/tr.json`
- Test: `src-tauri/src/commands/artwork.rs` needs none (thin adapter); the
  frontend gains no pure logic here.

**Interfaces:**
- Consumes: `core::artwork::local::{adopt_local, LocalPreview, LocalOutcome}`.
- Produces:
  ```rust
  #[tauri::command]
  pub fn artwork_adopt_local(previews: Vec<LocalPreviewArg>, app: AppHandle,
      registry: State<'_, Arc<JobRegistry>>) -> AppResult<JobId>;
  pub struct LocalPreviewArg { pub title: String, pub package: String, pub entry: String }
  pub const LOCAL_RESULT_EVENT: &str = "artwork-local-result";
  ```
  TypeScript:
  ```ts
  export interface LocalPreviewArg { title: string; package: string; entry: string }
  export interface LocalOutcome { written: number; adopted: number; missed: number }
  export async function artworkAdoptLocal(previews: LocalPreviewArg[]): Promise<number>;
  export const ARTWORK_LOCAL_RESULT_EVENT = "artwork-local-result";
  ```

- [ ] **Step 1: Add the command**

In `src-tauri/src/commands/artwork.rs`, beside `artwork_enrich`:

```rust
/// The argument shape: a path is a string on the wire, a `PathBuf` in `core`.
#[derive(Debug, Clone, Deserialize)]
pub struct LocalPreviewArg {
    pub title: String,
    pub package: String,
    pub entry: String,
}

/// Emitted when the offline pass finishes.
pub const LOCAL_RESULT_EVENT: &str = "artwork-local-result";

#[derive(Debug, Clone, Serialize)]
pub struct LocalResult {
    pub job_id: JobId,
    pub outcome: LocalOutcome,
}

/// Take the pictures the user's own packages already carry.
///
/// A job rather than a plain command because it opens one archive per title
/// and there are 242 of them (§54).
#[tauri::command]
pub fn artwork_adopt_local(
    previews: Vec<LocalPreviewArg>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
) -> AppResult<JobId> {
    let dir = artwork_dir_for(&app);
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let previews: Vec<LocalPreview> = previews
        .into_iter()
        .map(|arg| LocalPreview {
            title: arg.title,
            package: PathBuf::from(arg.package),
            entry: arg.entry,
        })
        .collect();

    let id = spawn_job(
        &app,
        registry,
        "Reading pictures from your files",
        move |job_id, progress| {
            let outcome = adopt_local(&dir, &previews, progress)?;
            let _ = emit_app.emit(LOCAL_RESULT_EVENT, LocalResult { job_id, outcome });
            Ok(())
        },
    );

    Ok(id)
}
```

Register it in `src-tauri/src/lib.rs` inside `invoke_handler![]`, beside
`commands::artwork::artwork_enrich`:

```rust
commands::artwork::artwork_adopt_local,
```

- [ ] **Step 2: Add the typed wrapper**

In `src/lib/artwork.ts`:

```ts
/** One picture to look for inside a package the user already has. */
export interface LocalPreviewArg {
  title: string;
  /** The `.rp9` on disk. */
  package: string;
  /** The entry's name inside it — `GameRecord.preview`, verbatim. */
  entry: string;
}

/** What the offline pass managed. Mirrors `core::artwork::local::LocalOutcome`. */
export interface LocalOutcome {
  written: number;
  adopted: number;
  missed: number;
}

export const ARTWORK_LOCAL_RESULT_EVENT = "artwork-local-result";

/**
 * Take the pictures the user's own `.rp9` packages carry.
 *
 * Touches no network and asks no source, which is why it needs no
 * configuration and no consent: nothing leaves the machine.
 */
export async function artworkAdoptLocal(previews: LocalPreviewArg[]): Promise<number> {
  return invoke<number>("artwork_adopt_local", { previews });
}
```

- [ ] **Step 3: Add the button and its strings**

In `src/pages/CollectionStudio.tsx`, beside the existing artwork button, add a
handler that builds the list from the rows on screen and calls it:

```tsx
async function handleLocalPictures() {
  const previews = filteredItems
    .filter((item) => item.preview)
    .map((item) => ({ title: item.title, package: item.path, entry: item.preview as string }));
  if (previews.length === 0) {
    setError(t("artwork.local.none"));
    return;
  }
  setStatusMsg(t("artwork.local.running", { count: previews.length }));
  await artworkAdoptLocal(previews);
}
```

and a button rendered next to *Fetch artwork*:

```tsx
<button className="btn btn-sm" onClick={() => void handleLocalPictures()} disabled={busy}>
  🖼 {t("artwork.local.action")}
</button>
```

Listen for `ARTWORK_LOCAL_RESULT_EVENT` the same way the screen already listens
for `ARTWORK_RESULT_EVENT`, and re-read the cache when it arrives so the
pictures appear without a reload.

New keys, **both catalogues**, under `artwork.local`:

`src/i18n/en.json`:
```json
"local": {
  "action": "Use the pictures in my files",
  "running": "Reading {{count}} picture(s) out of your packages…",
  "none": "None of the titles on screen carries a picture of its own.",
  "done": "{{written}} added, {{adopted}} already there, {{missed}} without one."
}
```

`src/i18n/tr.json`:
```json
"local": {
  "action": "Dosyalarımdaki resimleri kullan",
  "running": "{{count}} resim paketlerinden okunuyor…",
  "none": "Ekrandaki başlıkların hiçbiri kendi resmini taşımıyor.",
  "done": "{{written}} eklendi, {{adopted}} zaten vardı, {{missed}} resimsiz."
}
```

- [ ] **Step 4: Verify**

Run: `pnpm lint && pnpm test`
Expected: PASS, including the i18n parity test.

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: clean, all green.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/artwork.rs src-tauri/src/lib.rs src/lib/artwork.ts \
        src/pages/CollectionStudio.tsx src/i18n/en.json src/i18n/tr.json
git commit -m "feat(collection): a button for the pictures your own files already carry"
```

---

## Task 3: The panel's pure logic

**Files:**
- Create: `src/lib/collectionDetail.ts`, `src/lib/collectionDetail.test.ts`
- Modify: `src/i18n/phrase-keys.test.ts`, `src/i18n/en.json`, `src/i18n/tr.json`

**Interfaces:**
- Consumes: `Media`, `CatalogueEntry` from `@/lib/gameindex`; `Phrase` from
  `@/lib/phrase`.
- Produces:
  ```ts
  export function mediaPhrase(media: Media): Phrase;
  export function diskList(media: Media): string[];
  export function canLaunch(media: Media): boolean;
  ```

- [ ] **Step 1: Write the failing tests**

`src/lib/collectionDetail.test.ts`:

```ts
import { describe, expect, it } from "vitest";

import { canLaunch, diskList, mediaPhrase } from "./collectionDetail";
import type { Media } from "./gameindex";

const floppies: Media = { kind: "floppies", ordered: ["Dune2 Disk1.adf", "Dune2 Disk2.adf"] };
const hardfile: Media = { kind: "hardfile", file: "Agony.hdf" };
const drawer: Media = { kind: "whdload-drawer", slave: "Turrican.slave" };

describe("mediaPhrase", () => {
  it("counts the disks of a floppy set", () => {
    expect(mediaPhrase(floppies)).toEqual({
      key: "collection.detail.media.floppies",
      params: { count: 2 },
    });
  });

  it("names the slave of a WHDLoad drawer", () => {
    expect(mediaPhrase(drawer)).toEqual({
      key: "collection.detail.media.whdload",
      params: { slave: "Turrican.slave" },
    });
  });

  it("names the image of a hardfile", () => {
    expect(mediaPhrase(hardfile)).toEqual({
      key: "collection.detail.media.hardfile",
      params: { file: "Agony.hdf" },
    });
  });
});

describe("diskList", () => {
  it("keeps the order the catalogue recorded — it is the order the game asks for", () => {
    expect(diskList(floppies)).toEqual(["Dune2 Disk1.adf", "Dune2 Disk2.adf"]);
  });

  it("is empty for media that is not a disk set", () => {
    expect(diskList(hardfile)).toEqual([]);
    expect(diskList(drawer)).toEqual([]);
  });
});

describe("canLaunch", () => {
  it("says yes for every medium this wave launches", () => {
    expect(canLaunch(floppies)).toBe(true);
    expect(canLaunch(hardfile)).toBe(true);
    expect(canLaunch(drawer)).toBe(true);
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm vitest run src/lib/collectionDetail.test.ts`
Expected: FAIL — cannot resolve `./collectionDetail`.

- [ ] **Step 3: Write the implementation**

`src/lib/collectionDetail.ts`:

```ts
/**
 * The detail panel's pure parts (Collection · wave C).
 *
 * No i18next singleton here, so nothing renders a string: each helper returns
 * a {@link Phrase} and `TitleDetail.tsx` calls `t()` on it.
 */

import type { Media } from "./gameindex";
import type { Phrase } from "./phrase";

/** How this title's media reads in one line. */
export function mediaPhrase(media: Media): Phrase {
  switch (media.kind) {
    case "floppies":
      return {
        key: "collection.detail.media.floppies",
        params: { count: media.ordered.length },
      };
    case "hardfile":
      return { key: "collection.detail.media.hardfile", params: { file: media.file } };
    default:
      return { key: "collection.detail.media.whdload", params: { slave: media.slave } };
  }
}

/**
 * The disks, in the order the catalogue holds them.
 *
 * That order is not decoration: `.rp9` states it through `<floppy priority>`
 * and a game asks for disk two by name.
 */
export function diskList(media: Media): string[] {
  return media.kind === "floppies" ? media.ordered : [];
}

/** Whether Play can do anything with this medium at all. */
export function canLaunch(media: Media): boolean {
  return (
    media.kind === "floppies" || media.kind === "hardfile" || media.kind === "whdload-drawer"
  );
}
```

`Media` in `src/lib/gameindex.ts` already discriminates on `kind` with exactly
these three variants (`"floppies"`, `"hardfile"`, `"whdload-drawer"`) — this
matches it rather than introducing a second shape.

- [ ] **Step 4: Add the strings, both catalogues**

`src/i18n/en.json`, under `collection.detail`:
```json
"media": {
  "floppies_one": "{{count}} floppy disk",
  "floppies_other": "{{count}} floppy disks",
  "hardfile": "Hard disk image — {{file}}",
  "whdload": "WHDLoad — {{slave}}"
}
```

`src/i18n/tr.json`:
```json
"media": {
  "floppies_one": "{{count}} disket",
  "floppies_other": "{{count}} disket",
  "hardfile": "Sabit disk imajı — {{file}}",
  "whdload": "WHDLoad — {{slave}}"
}
```

- [ ] **Step 5: Enumerate the keys**

In `src/i18n/phrase-keys.test.ts`, add the import and a case beside the others:

```ts
import { mediaPhrase } from "@/lib/collectionDetail";
import type { Media } from "@/lib/gameindex";

it("mediaPhrase: every medium resolves", () => {
  const media: Media[] = [
    { kind: "floppies", ordered: ["a.adf"] },
    { kind: "hardfile", file: "a.hdf" },
    { kind: "whdload-drawer", slave: "a.slave" },
  ];
  for (const one of media) {
    const phrase = mediaPhrase(one);
    expect(isLeafKey(phrase.key), phrase.key).toBe(true);
  }
});
```

Note: a plural key resolves through `_one`/`_other`, so `isLeafKey` is checked
against the base key the same way the existing plural cases in that file are —
follow whichever of those it already does rather than inventing a second rule.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `pnpm test`
Expected: PASS, including i18n parity and the phrase-key enumeration.

- [ ] **Step 7: Commit**

```bash
git add src/lib/collectionDetail.ts src/lib/collectionDetail.test.ts \
        src/i18n/en.json src/i18n/tr.json src/i18n/phrase-keys.test.ts
git commit -m "feat(collection): the detail panel's pure logic, with its phrases"
```

---

## Task 4: The detail panel

**Files:**
- Create: `src/components/collection/TitleDetail.tsx`
- Modify: `src/pages/CollectionStudio.tsx`, `src/i18n/en.json`,
  `src/i18n/tr.json`, `src/i18n/literal-keys.test.ts`

**Interfaces:**
- Consumes: `mediaPhrase`, `diskList` from `@/lib/collectionDetail`;
  `usePowerMode` from `@/lib/uxmode`; `recall`/`recallInto` from
  `@/lib/remembered`.
- Produces: `<TitleDetail entry={…} art={…} onClose={…} />`, and a
  `selectedId` on the Collection screen that later tasks hang actions off.

- [ ] **Step 1: Add the selection state, remembered**

In `src/pages/CollectionStudio.tsx`:

```tsx
const [selectedId, setSelectedId] = useState<string | null>(null);

// A choice the user made does not reset itself between runs (the settings
// rule, and it holds for a screen's own state too).
useEffect(() => {
  void recallInto("collection.selectedId", nullOr(isNonEmptyString), null, setSelectedId);
}, []);
```

Use whichever guard names `src/lib/remembered.ts` actually exports — read that
file first and use its own vocabulary rather than adding a new guard.

Clicking a card sets `selectedId`; clicking the same card again clears it.

- [ ] **Step 2: Write the panel**

`src/components/collection/TitleDetail.tsx`:

```tsx
import { useTranslation } from "react-i18next";

import { diskList, mediaPhrase } from "@/lib/collectionDetail";
import type { CatalogueEntry } from "@/lib/gameindex";
import { usePowerMode } from "@/lib/uxmode";

export function TitleDetail({
  entry,
  art,
  onClose,
}: {
  entry: CatalogueEntry;
  art: string | undefined;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const power = usePowerMode();
  const record = entry.record;
  const disks = diskList(record.media);
  const media = mediaPhrase(record.media);

  return (
    <section className="card" style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "start" }}>
        <h2 style={{ fontSize: 16, margin: 0 }}>{record.title.value}</h2>
        <button className="btn btn-sm" onClick={onClose}>
          {t("common.close")}
        </button>
      </div>

      {art && (
        <img
          src={art}
          alt=""
          style={{ display: "block", width: "100%", maxHeight: 260, objectFit: "contain" }}
        />
      )}

      <div className="muted" style={{ fontSize: 13 }}>
        {t(media.key, media.params)}
      </div>

      {/* The facts, each keeping the `Guessed` mark the card already uses —
          a value ART inferred must not read as one it was told. */}
      <dl style={{ display: "grid", gridTemplateColumns: "auto 1fr", gap: "4px 10px", margin: 0, fontSize: 13 }}>
        <dt className="muted">{t("collection.detail.publisher")}</dt>
        <dd style={{ margin: 0 }}>
          {record.publisher?.value ?? t("common.unknown")}
          {record.publisher && <Guessed from={record.publisher.from} />}
        </dd>
        <dt className="muted">{t("collection.detail.year")}</dt>
        <dd style={{ margin: 0 }}>
          {record.year?.value ?? t("common.unknown")}
          {record.year && <Guessed from={record.year.from} />}
        </dd>
        <dt className="muted">{t("collection.detail.genre")}</dt>
        <dd style={{ margin: 0 }}>
          {record.genre?.value ?? t("common.unknown")}
          {record.genre && <Guessed from={record.genre.from} />}
        </dd>
        <dt className="muted">{t("collection.detail.rating")}</dt>
        <dd style={{ margin: 0 }}>{record.rating?.value ?? t("common.unknown")}</dd>
      </dl>

      {/* `KickstartNeed.image` is nullable — a slave can declare a size and a
          CRC and no name at all — so the guard is on the image, not on the
          need. Rendering `null` into the sentence is the bug this avoids. */}
      {record.kickstart?.value.image && (
        <div className="faint" style={{ fontSize: 12 }}>
          {t("gameindex.kickstartNeeded", { image: record.kickstart.value.image })}
        </div>
      )}

      {disks.length > 0 && (
        <ol style={{ fontSize: 12, margin: 0, paddingLeft: 20 }}>
          {disks.map((disk) => (
            <li key={disk}>{disk}</li>
          ))}
        </ol>
      )}

      {/* Beginner mode hides the raw path — and hides only. No action below
          is disabled by the mode (§47, §48). */}
      {power && (
        <div className="faint" style={{ fontSize: 11, wordBreak: "break-all" }}>
          {entry.path}
        </div>
      )}
    </section>
  );
}
```

`Guessed` is currently a local component in `CollectionStudio.tsx`. Move it to
`src/components/collection/Guessed.tsx` and import it in both places rather
than writing a second copy — a mark that means "ART inferred this" must not
have two implementations that can drift.

**The picture switch.** When the cache holds more than one kind for this title,
render a row of small buttons — one per kind the title actually has, labelled
from `artwork.kind.<kind>` — above the large picture, and show the selected
one. With a single kind, no row at all: a control with one option is noise.
Which kind was last chosen is per-title state and is **not** remembered — the
panel opens on the preferred kind (`ART_KINDS` order) every time, which is the
one the grid is already showing.

Render it from `CollectionStudio.tsx` in a two-column grid when a title is
selected — the grid on the left, the panel on the right — collapsing to one
column below the width the screen already uses for its other breakpoints. Do
not set a fixed pixel height on it: Application Size scales this screen like
every other (`src/lib/appZoom.ts`).

- [ ] **Step 3: Add the strings, both catalogues**

`src/i18n/en.json`, under `collection.detail` (beside the `media` block from
Task 3):

```json
"publisher": "Publisher",
"year": "Year",
"genre": "Genre",
"rating": "Rating",
"close": "Close",
"pathLabel": "File"
```

`src/i18n/tr.json`:

```json
"publisher": "Yayıncı",
"year": "Yıl",
"genre": "Tür",
"rating": "Puan",
"close": "Kapat",
"pathLabel": "Dosya"
```

Check `en.json` for an existing `common.unknown` and `common.close` before
adding a second spelling of either; reuse whichever is already there and drop
the duplicate from the block above.

- [ ] **Step 4: Update the dynamic-call count**

`t(media.key, media.params)` is a dynamic call site. In
`src/i18n/literal-keys.test.ts`, raise the expected number by one and add a
comment in the same voice as the entries above it, naming what the call is and
where its keys are enumerated.

- [ ] **Step 5: Verify**

Run: `pnpm lint && pnpm test`
Expected: PASS.

- [ ] **Step 6: Look at it**

Run `pnpm tauri dev`, open the Collection, click a card. Confirm: the panel
opens beside the grid, the picture from Task 1 is in it, the disk order is
right, Beginner mode hides the path and Power User shows it, and Ctrl +/- still
scales the screen.

- [ ] **Step 7: Commit**

```bash
git add src/components/collection/TitleDetail.tsx src/pages/CollectionStudio.tsx \
        src/i18n/en.json src/i18n/tr.json src/i18n/literal-keys.test.ts
git commit -m "feat(collection): a title opens into a detail panel"
```

---

## Task 5: A hand-attached picture, in the layer that survives a refresh

**Files:**
- Modify: `src-tauri/src/core/gameindex/store.rs`,
  `src-tauri/src/core/artwork/enrich.rs`
- Test: inline in both

**Interfaces:**
- Produces:
  ```rust
  pub struct ArtBinding { pub chosen: String, pub cached: String }
  // RecordOverride gains: pub art: Option<ArtBinding>
  ```
  and `enrich` gains a `pinned: &[String]` field on `EnrichRequest` — titles
  whose picture the user chose, which no source may overwrite.

- [ ] **Step 1: Write the failing tests**

In `src-tauri/src/core/gameindex/store.rs`'s test module:

```rust
/// The user's picture is a choice, not derived data: a refresh re-reads every
/// file on disk and must not touch it.
#[test]
fn a_hand_attached_picture_survives_a_refresh() {
    let dir = scratch("art-binding");
    set_override(
        &dir,
        "turrican-1a2b3c4d",
        RecordOverride {
            art: Some(ArtBinding {
                chosen: r"D:\pictures\turrican.png".into(),
                cached: "turrican/manual-snap.png".into(),
            }),
            ..Default::default()
        },
    )
    .unwrap();

    let overrides = read_overrides(&dir).unwrap();
    let kept = overrides.edits.get("turrican-1a2b3c4d").expect("still there");

    assert_eq!(
        kept.art.as_ref().map(|art| art.cached.as_str()),
        Some("turrican/manual-snap.png")
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// And an override that says nothing is still deleted rather than stored.
#[test]
fn removing_the_picture_leaves_no_trace() {
    let dir = scratch("art-binding-removed");
    assert!(RecordOverride {
        art: None,
        ..Default::default()
    }
    .is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}
```

Use the test module's existing helpers (`scratch`, `set_override`,
`read_overrides`) exactly as the neighbouring tests do — read them first.

In `src-tauri/src/core/artwork/enrich.rs`'s test module:

```rust
/// A source may not overwrite a picture the user chose by hand.
#[test]
fn a_pinned_title_is_left_alone() {
    let dir = scratch("pinned");
    let outcome = enrich(
        EnrichRequest {
            titles: &["Turrican".to_string()],
            sources: &[source_that_always_answers()],
            cache_dir: &dir,
            wanted: &[ArtKind::Boxart],
            pinned: &["Turrican".to_string()],
        },
        &client_that_would_answer(),
        &NoProgress,
    )
    .unwrap();

    assert_eq!(
        outcome.sources.iter().map(|s| s.written).sum::<u32>(),
        0,
        "the user's own picture is not something a source gets to replace"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
```

Build `source_that_always_answers` and `client_that_would_answer` from the fake
client the module's existing tests already use — read them rather than writing
a second fake.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test gameindex::store && cargo test artwork::enrich`
Expected: FAIL — `ArtBinding` and `pinned` do not exist.

- [ ] **Step 3: Implement**

In `store.rs`:

```rust
/// A picture the user attached by hand.
///
/// **Two halves with two owners.** The bytes live in the artwork cache, which
/// is derived data and can be rebuilt; the *choice* lives here, in the layer
/// no refresh touches. `chosen` is the file the user picked, kept so the
/// binding can be re-materialised if the cache is ever cleared; `cached` is
/// the cache-relative name the screen renders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtBinding {
    pub chosen: String,
    pub cached: String,
}
```

Add `pub art: Option<ArtBinding>` to `RecordOverride`, extend `is_empty()` with
`&& self.art.is_none()`, and leave `apply_override` alone: a picture is not a
`Fact` on the record and the screen reads it from the cache.

Adding `pinned` to `EnrichRequest` breaks its one existing caller,
`commands/artwork.rs::artwork_enrich`. **Pass `pinned: &[]` there for now** —
Task 6 step 5 is where the real list arrives, and a command that pins nothing
behaves exactly as it does today.

In `enrich.rs`, add `pub pinned: &'a [String]` to `EnrichRequest` with a
doc-comment saying why, and skip a title whose normalised key is in it before
any source is asked.

Every existing construction of `EnrichRequest` and `RecordOverride` must be
updated — `cargo test` will name them all.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/core/gameindex/store.rs src-tauri/src/core/artwork/enrich.rs
git commit -m "feat(collection): the picture you chose lives in the layer a refresh cannot touch"
```

---

## Task 6: Attaching and removing a picture from the panel

**Files:**
- Modify: `src-tauri/src/commands/artwork.rs`, `src-tauri/src/lib.rs`,
  `src/lib/artwork.ts`, `src/components/collection/TitleDetail.tsx`,
  `src/i18n/en.json`, `src/i18n/tr.json`
- Test: inline in `commands/artwork.rs` for the format gate

**Interfaces:**
- Produces:
  ```rust
  #[tauri::command] pub fn artwork_attach(title: String, id: String, file: String,
      app: AppHandle) -> AppResult<ArtRef>;
  #[tauri::command] pub fn artwork_detach(title: String, id: String,
      app: AppHandle) -> AppResult<()>;
  ```
  ```ts
  export async function artworkAttach(title: string, id: string, file: string): Promise<ArtRef>;
  export async function artworkDetach(title: string, id: string): Promise<void>;
  ```

- [ ] **Step 1: Write the failing test for the format gate**

In `src-tauri/src/commands/artwork.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// PNG and JPEG are what the webview draws. An IFF would be accepted here
    /// and then not render, which is worse than an honest refusal.
    #[test]
    fn only_the_formats_the_screen_can_draw_are_accepted() {
        assert_eq!(picture_extension(Path::new("cover.png")), Some("png"));
        assert_eq!(picture_extension(Path::new("cover.PNG")), Some("png"));
        assert_eq!(picture_extension(Path::new("cover.jpg")), Some("jpg"));
        assert_eq!(picture_extension(Path::new("cover.jpeg")), Some("jpg"));
        assert_eq!(picture_extension(Path::new("cover.iff")), None);
        assert_eq!(picture_extension(Path::new("cover")), None);
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd src-tauri && cargo test only_the_formats_the_screen_can_draw`
Expected: FAIL — `picture_extension` is not defined.

- [ ] **Step 3: Implement**

```rust
/// The cache extension for a picture the screen can actually draw.
fn picture_extension(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => Some("png"),
        Some("jpg" | "jpeg") => Some("jpg"),
        _ => None,
    }
}

/// Attach a picture the user picked to a title.
///
/// The bytes are copied into the cache under source `manual`; the choice is
/// written into the catalogue's user layer, which is what makes it survive a
/// refresh and stops a later fetch from replacing it.
#[tauri::command]
pub fn artwork_attach(
    title: String,
    id: String,
    file: String,
    app: AppHandle,
) -> AppResult<ArtRef> {
    let source = PathBuf::from(&file);
    let ext = picture_extension(&source)
        .ok_or("ART can show PNG and JPEG pictures. This file is neither.")?;
    let bytes = std::fs::read(&source)?;

    let dir = artwork_dir_for(&app);
    let mut cache = Cache::open(&dir)?;
    let art = cache.store(&title, ArtKind::Boxart, "manual", ext, &bytes)?;
    cache.save()?;

    let catalogue = catalogue_dir(&app);
    let mut edit = read_overrides(&catalogue)?
        .edits
        .get(&id)
        .cloned()
        .unwrap_or_default();
    edit.art = Some(ArtBinding {
        chosen: file,
        cached: art.file.clone(),
    });
    set_override(&catalogue, &id, edit)?;

    Ok(art)
}
```

`ArtKind::Boxart` on purpose: `ART_KINDS` prefers it first, so a hand-attached
picture outranks the `.rp9` snap from Task 1 — which is what the user meant by
attaching one.

Write `artwork_detach` as the mirror: remove the cache entry, clear
`edit.art`, and let `set_override`'s existing empty-override rule delete the
record if nothing else is in it.

`catalogue_dir` currently lives in `commands/gameindex.rs` and is private —
make it `pub(crate)` there rather than writing a second copy, and say why in a
one-line comment.

Register both commands in `lib.rs`.

- [ ] **Step 4: Add the wrappers, the buttons and the strings**

`src/lib/artwork.ts`:

```ts
/**
 * Attach a picture the user picked to a title.
 *
 * `id` is the record's id, because the choice is written into the catalogue's
 * user layer — the one a refresh does not touch. `title` is what the artwork
 * cache is keyed by. They are different things and both are needed.
 */
export async function artworkAttach(
  title: string,
  id: string,
  file: string,
): Promise<ArtRef> {
  return invoke<ArtRef>("artwork_attach", { title, id, file });
}

/** Undo that. An override with nothing left in it deletes itself. */
export async function artworkDetach(title: string, id: string): Promise<void> {
  await invoke("artwork_detach", { title, id });
}
```

`TitleDetail.tsx`:

```tsx
async function attach() {
  const chosen = await open({
    multiple: false,
    filters: [{ name: t("collection.detail.art.filter"), extensions: ["png", "jpg", "jpeg"] }],
    title: t("collection.detail.art.dialog"),
  });
  if (typeof chosen !== "string") return;
  await artworkAttach(record.title.value, record.id, chosen);
  onArtChanged();
}
```

with an **Attach a picture…** button always present and a **Remove** button
rendered only when this title has a manual picture. `onArtChanged` is a new
prop the Collection screen passes so it can re-read the cache — the same
re-read it already does after an artwork run.

`src/i18n/en.json`, under `collection.detail`:

```json
"art": {
  "attach": "Attach a picture…",
  "remove": "Remove the picture",
  "dialog": "Choose a picture for this title",
  "filter": "Picture (PNG, JPEG)",
  "rejected": "ART can show PNG and JPEG pictures. This file is neither."
}
```

`src/i18n/tr.json`:

```json
"art": {
  "attach": "Resim bağla…",
  "remove": "Resmi kaldır",
  "dialog": "Bu başlık için bir resim seç",
  "filter": "Resim (PNG, JPEG)",
  "rejected": "ART, PNG ve JPEG resimleri gösterebilir. Bu dosya ikisi de değil."
}
```

- [ ] **Step 5: Feed the pinned list to the fetcher**

Task 5 gave `EnrichRequest` a `pinned` field and nothing fills it yet. In
`commands/artwork.rs::artwork_enrich`, read the catalogue's overrides and pass
the titles whose `art` is set:

```rust
let pinned: Vec<String> = read_overrides(&catalogue_dir(&app))
    .map(|overrides| {
        overrides
            .edits
            .values()
            .filter(|edit| edit.art.is_some())
            .filter_map(|_| None) // see below
            .collect()
    })
    .unwrap_or_default();
```

The override layer is keyed by **record id** and the cache by **title**, and
`Overrides` carries no title. So the screen — which has both — sends the pinned
titles with the request instead: add `pinned: Vec<String>` to
`artwork_enrich`'s arguments and to `artworkEnrich`'s wrapper, and build it in
`CollectionStudio.tsx` from the rows whose override names a picture. Delete the
sketch above; it is here to show why the obvious version does not work.

- [ ] **Step 6: Verify**

Run: `pnpm lint && pnpm test && cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: all green.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(collection): attach your own picture to a title, and keep it"
```

---

## Task 7: `core/launch` — what a title needs to run

**Files:**
- Create: `src-tauri/src/core/launch/mod.rs`
- Modify: `src-tauri/src/core/mod.rs` (add `pub mod launch;`)
- Test: inline in `mod.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct LaunchRom { pub name: String, pub models: Vec<String>, pub path: String }
  pub enum Machine { A500, A1200 }
  pub enum LaunchKind {
      Floppies { images: Vec<String> },
      Hardfile { image: String },
      Whdload { drawer: String, slave: String, system: String, one_click: bool },
  }
  pub struct LaunchPlan {
      pub machine: Machine, pub rom: LaunchRom, pub kind: LaunchKind,
      pub notes: Vec<LaunchNote>,
  }
  pub enum LaunchNote { MoreDisksThanDrives { total: usize, mounted: usize } }
  pub enum LaunchRefusal { NoSuitableRom { machine: Machine }, NoSystemVolume, FileMissing { path: String } }
  pub enum Chipset { Ocs, Ecs, Aga }
  pub enum RequestKind {
      Floppies { images: Vec<String> },
      Hardfile { image: String },
      Whdload { drawer: String, slave: String },
  }
  pub struct LaunchRequest<'a> {
      pub machine: Machine,
      pub roms: &'a [LaunchRom],
      pub kind: RequestKind,
      /// The user's own bootable system, for a WHDLoad drawer. `None` is a
      /// refusal, not a guess.
      pub system_volume: Option<String>,
      /// Y2 by default; `false` is the user asking for Y1.
      pub one_click: bool,
  }
  pub fn machine_for(stated: Option<Chipset>, default: Machine) -> Machine;
  pub fn plan_for(request: &LaunchRequest) -> Result<LaunchPlan, LaunchRefusal>;
  pub const MAX_FLOPPY_DRIVES: usize = 4;
  ```

  `Machine` derives `Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize`
  and serialises kebab-case (`"a500"`, `"a1200"`), like every other enum that
  crosses to the frontend. `LaunchNote` and `LaunchRefusal` derive the same
  plus `#[serde(rename_all = "kebab-case", tag = "kind")]`, so the TypeScript
  side discriminates on `kind` exactly as `Media` does.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn a1200_rom() -> LaunchRom {
        LaunchRom {
            name: "Kickstart 40.68 (A1200)".into(),
            models: vec!["A1200".into()],
            path: r"D:\roms\kick40068.A1200".into(),
        }
    }

    fn a500_rom() -> LaunchRom {
        LaunchRom {
            name: "Kickstart 34.5 (A500/A2000/A1000)".into(),
            models: vec!["A500".into(), "A2000".into()],
            path: r"D:\roms\kick34005.A500".into(),
        }
    }

    /// The chipset the catalogue recorded picks the machine.
    #[test]
    fn an_aga_title_asks_for_an_a1200_and_an_ocs_one_for_an_a500() {
        assert_eq!(machine_for(Some(Chipset::Aga), Machine::A500), Machine::A1200);
        assert_eq!(machine_for(Some(Chipset::Ocs), Machine::A500), Machine::A500);
    }

    /// 1536 WHDLoad titles state no chipset at all, so the default is the
    /// common case and not the edge.
    #[test]
    fn a_title_that_states_no_chipset_takes_the_users_default() {
        assert_eq!(machine_for(None, Machine::A1200), Machine::A1200);
    }

    /// A ROM that does not suit is a refusal, not a black screen.
    #[test]
    fn no_suitable_rom_refuses_rather_than_launching() {
        let refusal = plan_for(&LaunchRequest {
            machine: Machine::A1200,
            roms: &[a500_rom()],
            kind: RequestKind::Floppies { images: vec![r"D:\g\a.adf".into()] },
            system_volume: None,
            one_click: true,
        })
        .unwrap_err();

        assert!(matches!(refusal, LaunchRefusal::NoSuitableRom { machine: Machine::A1200 }));
    }

    /// WinUAE has four drives. Saying so is the whole fix; pretending
    /// otherwise is what §89 forbids.
    #[test]
    fn a_set_larger_than_four_disks_mounts_four_and_says_so() {
        let plan = plan_for(&LaunchRequest {
            machine: Machine::A500,
            roms: &[a500_rom()],
            kind: RequestKind::Floppies {
                images: (1..=6).map(|n| format!(r"D:\g\disk{n}.adf")).collect(),
            },
            system_volume: None,
            one_click: true,
        })
        .unwrap();

        match plan.kind {
            LaunchKind::Floppies { ref images } => assert_eq!(images.len(), 4),
            ref other => panic!("{other:?}"),
        }
        assert!(plan
            .notes
            .contains(&LaunchNote::MoreDisksThanDrives { total: 6, mounted: 4 }));
    }

    /// A WHDLoad drawer cannot run without a booting system, and ART owns none.
    #[test]
    fn a_whdload_title_without_a_system_volume_refuses() {
        let refusal = plan_for(&LaunchRequest {
            machine: Machine::A1200,
            roms: &[a1200_rom()],
            kind: RequestKind::Whdload {
                drawer: r"D:\games\Turrican".into(),
                slave: "Turrican.slave".into(),
            },
            system_volume: None,
            one_click: true,
        })
        .unwrap_err();

        assert!(matches!(refusal, LaunchRefusal::NoSystemVolume));
    }

    #[test]
    fn a_whdload_title_with_a_system_volume_plans_both_mounts() {
        let plan = plan_for(&LaunchRequest {
            machine: Machine::A1200,
            roms: &[a1200_rom()],
            kind: RequestKind::Whdload {
                drawer: r"D:\games\Turrican".into(),
                slave: "Turrican.slave".into(),
            },
            system_volume: Some(r"E:\amiga\amikit\AmiKit.hdf".into()),
            one_click: true,
        })
        .unwrap();

        match plan.kind {
            LaunchKind::Whdload { ref system, ref slave, one_click, .. } => {
                assert_eq!(system, r"E:\amiga\amikit\AmiKit.hdf");
                assert_eq!(slave, "Turrican.slave");
                assert!(one_click, "Y2 by default, with Y1 always one switch away");
            }
            ref other => panic!("{other:?}"),
        }
    }
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cd src-tauri && cargo test core::launch`
Expected: FAIL — the module does not exist.

- [ ] **Step 3: Implement**

Write `core/launch/mod.rs` with the types from the Interfaces block plus
`LaunchRequest`, `RequestKind`, `Chipset` and `machine_for`. Two rules to hold
while writing it:

- **It declares its own ROM record.** `LaunchRom` is this module's, carrying
  the two fields the decision reads. It must not take `core::rom::RomInfo` —
  that is the mistake `core/rom/pairing.rs`'s header describes, and
  `commands/launch.rs` does the mapping.
- **It starts nothing and reads no files.** Every path arrives as a string
  from the caller; whether the file exists is the command layer's question.

`MAX_FLOPPY_DRIVES` is `4`, named, with WinUAE's `floppy0..3` in the comment.

- [ ] **Step 4: Run them to verify they pass**

Run: `cd src-tauri && cargo test core::launch`
Expected: PASS, 6 tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/core/launch/mod.rs src-tauri/src/core/mod.rs
git commit -m "feat(launch): what a catalogued title needs before anything starts"
```

---

## Task 8: Getting the floppies out of a `.rp9`

**Files:**
- Create: `src-tauri/src/core/launch/extract.rs`
- Modify: `src-tauri/src/core/launch/mod.rs` (`pub mod extract;`)
- Test: inline

**Interfaces:**
- Produces:
  ```rust
  pub const MAX_FLOPPY_BYTES: u64 = 8 * 1024 * 1024;
  pub fn unpack_floppies(package: &Path, ordered: &[String], into: &Path)
      -> CoreResult<Vec<PathBuf>>;
  ```

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_disks_come_out_in_the_order_the_manifest_gave() {
        let dir = scratch("unpack");
        let pkg = package(
            &dir,
            "Dune2.rp9",
            &[("b.adf", b"SECOND"), ("a.adf", b"FIRST")],
        );

        let written = unpack_floppies(&pkg, &["a.adf".into(), "b.adf".into()], &dir.join("out"))
            .unwrap();

        assert_eq!(written.len(), 2);
        assert_eq!(std::fs::read(&written[0]).unwrap(), b"FIRST");
        assert_eq!(std::fs::read(&written[1]).unwrap(), b"SECOND");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_entry_that_escapes_the_destination_is_refused() {
        let dir = scratch("unpack-traversal");
        let pkg = package(&dir, "Evil.rp9", &[("../../evil.adf", b"NOPE")]);

        let err = unpack_floppies(&pkg, &["../../evil.adf".into()], &dir.join("out")).unwrap_err();

        assert!(matches!(err, CoreError::InvalidInput(_)), "{err:?}");
        assert!(!dir.join("evil.adf").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_disk_the_package_does_not_carry_is_an_error_not_a_gap() {
        let dir = scratch("unpack-missing");
        let pkg = package(&dir, "Half.rp9", &[("a.adf", b"FIRST")]);

        assert!(unpack_floppies(&pkg, &["a.adf".into(), "b.adf".into()], &dir.join("out")).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

Reuse the `package` and `scratch` helpers from Task 1 by copying them into this
test module — they are four lines each and a shared test helper across modules
is not a pattern this codebase uses.

- [ ] **Step 2: Run them to verify they fail**

Run: `cd src-tauri && cargo test launch::extract`
Expected: FAIL — no such module.

- [ ] **Step 3: Implement**

Read through `core::archive::open`, resolve each wanted name to its entry
index, read it bounded by `MAX_FLOPPY_BYTES`, and write it with
`core::security::path::safe_join(into, name)` followed by
`core::safety::atomic_write`. A wanted name that is not in the package, or that
`safe_join` refuses, is a `CoreError::InvalidInput` naming the entry — a
half-unpacked game is not something to launch.

- [ ] **Step 4: Run them to verify they pass**

Run: `cd src-tauri && cargo test launch::extract`
Expected: PASS, 3 tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/core/launch/extract.rs src-tauri/src/core/launch/mod.rs
git commit -m "feat(launch): a .rp9's disks, unpacked in the order the manifest gave"
```

---

## Task 9: Directory mounts in the generated WinUAE configuration

**Files:**
- Modify: `src-tauri/src/core/winuae.rs`
- Test: inline, beside the existing configuration-text tests

**Interfaces:**
- Produces:
  ```rust
  pub struct DirMount { pub host_path: String, pub volume: String, pub label: String,
      pub boot_priority: i8, pub read_only: bool }
  // LaunchMedia gains: #[serde(default)] pub directories: Vec<DirMount>
  ```

- [ ] **Step 1: Write the failing test**

```rust
/// Y1 and Y2 both need a host folder to appear as an Amiga volume, and the
/// system image beside it must not be writable.
#[test]
fn a_directory_mount_and_a_write_protected_system_reach_the_configuration() {
    let profile = AmigaProfile::a1200_aga();
    let media = LaunchMedia {
        hardfile_paths: vec![r"E:\amiga\amikit\AmiKit.hdf".into()],
        write_protect_hardfiles: true,
        directories: vec![
            DirMount {
                host_path: r"D:\games\Turrican".into(),
                volume: "DH1".into(),
                label: "Game".into(),
                boot_priority: 0,
                read_only: false,
            },
            DirMount {
                host_path: r"C:\Users\x\AppData\Roaming\art\launch\boot".into(),
                volume: "DH2".into(),
                label: "ARTBoot".into(),
                boot_priority: 10,
                read_only: false,
            },
        ],
        ..Default::default()
    };

    let uae = generate_uae_config(&profile, &media).unwrap();

    assert!(uae.contains(r"filesystem2=rw,DH1:Game:D:\games\Turrican,0"), "{uae}");
    assert!(
        uae.contains(r"filesystem2=rw,DH2:ARTBoot:C:\Users\x\AppData\Roaming\art\launch\boot,10"),
        "the boot directory outranks everything, which is what makes Y2 one click"
    );
    assert!(
        uae.contains("hardfile2=ro,"),
        "the user's own system image is mounted read-only"
    );
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd src-tauri && cargo test a_directory_mount_and_a_write_protected_system`
Expected: FAIL — `LaunchMedia` has no field `directories`.

- [ ] **Step 3: Implement**

Add the field and the emission loop beside the existing `hardfile2=` loop, and
run every host path through the same `checked_config_value` guard the hardfile
paths use — a configuration file is a text format and a path with a newline in
it is an injection.

`#[serde(default)]` on the new field so a stored configuration written by an
older build still deserialises.

- [ ] **Step 4: Run it to verify it passes**

Run: `cd src-tauri && cargo test winuae`
Expected: PASS, including the tests that were there before.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/core/winuae.rs
git commit -m "feat(winuae): mount a host folder as an Amiga volume, and boot from it"
```

---

## Task 10: The Y2 boot directory

**Files:**
- Create: `src-tauri/src/core/launch/whdload_boot.rs`
- Modify: `src-tauri/src/core/launch/mod.rs` (`pub mod whdload_boot;`)
- Test: inline

**Interfaces:**
- Produces:
  ```rust
  pub fn write_boot_dir(into: &Path, slave: &str, system_volume: &str, game_volume: &str)
      -> CoreResult<PathBuf>;
  pub fn startup_sequence(slave: &str, system_volume: &str, game_volume: &str)
      -> CoreResult<String>;
  ```

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_startup_sequence_assigns_from_the_system_and_runs_the_slave() {
        let text = startup_sequence("Turrican.slave", "DH0", "DH1").unwrap();

        assert!(text.contains("Assign C: DH0:C"), "{text}");
        assert!(text.contains("Assign LIBS: DH0:Libs"), "{text}");
        assert!(text.contains("Assign DEVS: DH0:Devs"), "{text}");
        assert!(text.contains("CD DH1:"), "{text}");
        assert!(text.contains("WHDLoad Turrican.slave"), "{text}");
    }

    /// A slave's name comes out of a file somebody else made, and this text
    /// becomes commands an Amiga executes.
    #[test]
    fn a_slave_name_that_could_add_a_command_is_refused() {
        assert!(startup_sequence("Turrican.slave\nDelete DH0:#?", "DH0", "DH1").is_err());
        assert!(startup_sequence("Turrican.slave\rFormat", "DH0", "DH1").is_err());
    }

    #[test]
    fn the_boot_directory_is_written_where_art_owns_it() {
        let dir = scratch("boot");
        let written = write_boot_dir(&dir, "Turrican.slave", "DH0", "DH1").unwrap();

        assert!(written.ends_with("Startup-Sequence"));
        assert!(dir.join("S").join("Startup-Sequence").is_file());
        let text = std::fs::read_to_string(dir.join("S").join("Startup-Sequence")).unwrap();
        assert!(text.contains("WHDLoad Turrican.slave"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cd src-tauri && cargo test launch::whdload_boot`
Expected: FAIL — no such module.

- [ ] **Step 3: Implement**

```rust
//! The five lines that make Y2 one click (wave C).
//!
//! A WHDLoad slave is not a program WinUAE can run: it needs an Amiga that has
//! booted, with WHDLoad installed. ART owns no such system and does not build
//! one for this wave — the user points at their own. What ART owns is *this*:
//! a boot directory of its own, mounted at the highest boot priority, whose
//! startup-sequence assigns from the mounted system and runs the slave.
//!
//! **Nothing here is written to the user's files.** The only path this module
//! writes to is ART's own launch directory. The user's system image is
//! mounted read-only; their game drawer is mounted writable, because WHDLoad
//! keeps save games beside the game and a launcher that discards a saved
//! position is not one.
//!
//! Y1 — mount the system, boot to Workbench, let the user start the game — is
//! always one switch away on the panel, and is what this falls back to. That
//! pairing is the shape `commands/preload.rs::run_with_fallback` already uses:
//! the good path first, a named alternative behind it, never a silent one.
```

`startup_sequence` refuses a slave name containing a control character,
`"` or `*` — the AmigaDOS escape — with a `CoreError::InvalidInput` naming the
slave. `write_boot_dir` creates `S/` and writes through
`core::safety::atomic_write`.

- [ ] **Step 4: Run them to verify they pass**

Run: `cd src-tauri && cargo test launch::whdload_boot`
Expected: PASS, 3 tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/core/launch/whdload_boot.rs src-tauri/src/core/launch/mod.rs
git commit -m "feat(launch): the boot directory that turns a WHDLoad slave into one click"
```

---

## Task 11: Play

**Files:**
- Create: `src-tauri/src/commands/launch.rs`, `src/lib/launch.ts`
- Modify: `src-tauri/src/lib.rs`, `src/components/collection/TitleDetail.tsx`,
  `src/i18n/en.json`, `src/i18n/tr.json`, `src/i18n/phrase-keys.test.ts`,
  `src/i18n/literal-keys.test.ts`
- Test: inline in `commands/launch.rs` for the `RomInfo` → `LaunchRom` mapping

**Interfaces:**
- Consumes: everything from Tasks 7-9, `core::winuae::{generate_uae_config,
  launch_winuae, detect_winuae, LaunchMedia, DirMount}`,
  `core::rom::{scan_rom_directory, RomInfo}`, `commands/oplog.rs::write_result`.
- Produces:
  ```rust
  #[tauri::command] pub fn launch_plan(request: LaunchArgs, app: AppHandle)
      -> AppResult<LaunchPreview>;
  #[tauri::command] pub fn launch_title(request: LaunchArgs, app: AppHandle,
      oplog: State<'_, JsonlOperationLog>) -> AppResult<u32>;
  ```
  ```rust
  #[derive(Deserialize)]
  pub struct LaunchArgs {
      pub id: String,
      pub title: String,
      /// The catalogued file: the `.adf`, the `.hdf`, the `.rp9`, or the
      /// WHDLoad drawer.
      pub path: String,
      pub media: crate::core::gameindex::record::Media,
      pub chipset: Option<String>,
      /// From Settings: the ROM folder, the user's default machine, and the
      /// bootable system a WHDLoad title needs.
      pub rom_dir: String,
      pub default_machine: Machine,
      pub system_volume: Option<String>,
      pub one_click: bool,
  }

  #[derive(Serialize)]
  pub struct LaunchPreview {
      pub plan: Option<LaunchPlan>,
      pub refusal: Option<LaunchRefusal>,
  }
  ```
  ```ts
  export type Machine = "a500" | "a1200";
  export interface LaunchRom { name: string; models: string[]; path: string }
  export type LaunchKind =
    | { kind: "floppies"; images: string[] }
    | { kind: "hardfile"; image: string }
    | { kind: "whdload"; drawer: string; slave: string; system: string; oneClick: boolean };
  export type LaunchNote = { kind: "more-disks-than-drives"; total: number; mounted: number };
  export type LaunchRefusal =
    | { kind: "no-suitable-rom"; machine: Machine }
    | { kind: "no-system-volume" }
    | { kind: "file-missing"; path: string };
  export interface LaunchPlan {
    machine: Machine;
    rom: LaunchRom;
    kind: LaunchKind;
    notes: LaunchNote[];
  }
  export interface LaunchPreview { plan: LaunchPlan | null; refusal: LaunchRefusal | null }
  export interface LaunchArgs {
    id: string; title: string; path: string; media: Media;
    chipset: string | null; rom_dir: string; default_machine: Machine;
    system_volume: string | null; one_click: boolean;
  }
  export async function launchPlan(request: LaunchArgs): Promise<LaunchPreview>;
  export async function launchTitle(request: LaunchArgs): Promise<number>;
  export function refusalPhrase(refusal: LaunchRefusal): Phrase;
  export function notePhrase(note: LaunchNote): Phrase;
  ```

- [ ] **Step 1: Write the failing test for the mapping**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::rom::RomChecksum;

    /// `core/launch` must not know what a `RomInfo` is — that is the
    /// lower-imports-higher mistake `core/rom/pairing.rs` documents. The
    /// translation happens here, which is where this codebase puts it.
    #[test]
    fn a_rom_info_becomes_the_two_fields_the_launcher_reads() {
        let info = RomInfo {
            name: "Kickstart 40.68 (A1200)".into(),
            version: "3.1".into(),
            revision: "40.068".into(),
            size_bytes: 524_288,
            sha256: String::new(),
            crc32: String::new(),
            is_cloanto: false,
            key_available: false,
            is_aros: false,
            checksum: RomChecksum::Valid,
            compatible_models: vec!["A1200".into()],
            file_path: r"D:\roms\kick.rom".into(),
        };

        let rom = launch_rom_from(&info);

        assert_eq!(rom.name, "Kickstart 40.68 (A1200)");
        assert_eq!(rom.models, vec!["A1200".to_string()]);
        assert_eq!(rom.path, r"D:\roms\kick.rom");
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd src-tauri && cargo test a_rom_info_becomes_the_two_fields`
Expected: FAIL — `commands/launch.rs` does not exist.

- [ ] **Step 3: Implement the command layer**

`launch_plan` gathers what the decision needs and returns it **without
starting anything**: the machine, the chosen ROM, the media, the notes, and the
refusal when there is one. That is what the confirmation screen renders.

```rust
/// `RomInfo` → the two fields `core/launch` reads. The lower module must not
/// know the higher one's type; this is where the translation lives.
fn launch_rom_from(info: &RomInfo) -> LaunchRom {
    LaunchRom {
        name: info.name.clone(),
        models: info.compatible_models.clone(),
        path: info.file_path.clone(),
    }
}

#[tauri::command]
pub fn launch_plan(request: LaunchArgs, app: AppHandle) -> AppResult<LaunchPreview> {
    let roms: Vec<LaunchRom> = scan_rom_directory(Path::new(&request.rom_dir))
        .unwrap_or_default()
        .iter()
        .map(launch_rom_from)
        .collect();

    let machine = machine_for(chipset_from(request.chipset.as_deref()), request.default_machine);
    let plan = plan_for(&LaunchRequest {
        machine,
        roms: &roms,
        kind: request_kind_from(&request),
        system_volume: request.system_volume.clone(),
        one_click: request.one_click,
    });

    Ok(match plan {
        Ok(plan) => LaunchPreview { plan: Some(plan), refusal: None },
        Err(refusal) => LaunchPreview { plan: None, refusal: Some(refusal) },
    })
}
```

`chipset_from` turns the catalogue's string into `core/launch`'s own enum —
`Some("aga") => Chipset::Aga`, `Some("ocs") => Chipset::Ocs`,
`Some("ecs") => Chipset::Ecs`, anything else `None`, which is the 1536-title
case and takes `default_machine`.

`request_kind_from` maps `Media` to `RequestKind`: a `Floppies` whose file is a
`.rp9` still arrives as `RequestKind::Floppies`, with the entry names as they
are — the unpacking in `launch_title` turns them into real paths, and the
preview shows the disk names the user recognises rather than a temporary
directory they have never seen.

`launch_title` then:

1. calls `plan_for` again (the screen may have been open for a while);
2. unpacks a `.rp9`'s disks into `<app data>\launch\<id>\` (Task 8);
3. for WHDLoad, writes `<app data>\launch\boot\` (Task 10);
4. builds `LaunchMedia` — game drawer `rw`, system image `ro`, boot directory
   at the highest priority;
5. calls `launch_winuae`;
6. records the result through `write_result`, success or failure, with the
   title and the medium — an external process against the user's files is
   exactly what §53 logs.

- [ ] **Step 4: Wire the screen**

`TitleDetail.tsx` gets a **Play** button that calls `launchPlan` first and
shows what it found — machine, ROM, disks, and for WHDLoad which of Y2/Y1 it
will use — with the alternative always visible. Confirming calls
`launchTitle`. A refusal renders `refusalPhrase(...)` and offers nothing to
confirm.

Every variant of `refusalPhrase` and `notePhrase` goes into
`phrase-keys.test.ts`; the dynamic-call count in `literal-keys.test.ts` rises
by the number of new dynamic `t()` calls, with a comment naming them.

Per-title choices (machine, ROM, Y1-vs-Y2) are remembered through
`src/lib/remembered.ts`, keyed by record id.

- [ ] **Step 5: Verify**

Run: `pnpm lint && pnpm test`
Expected: PASS.

Run: `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: clean, all green. Run `cargo test` a second time (ART-059).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(collection): Play — a catalogued title reaches WinUAE"
```

---

## Task 12: The real-material pass, and the documents

**Files:**
- Modify: `docs/STATUS.md`, `docs/FEATURES.md`, `docs/ISSUES.md`,
  `CHANGELOG.md`

- [ ] **Step 1: Drive it against the user's own material**

Three things no test can answer, in `pnpm tauri dev`:

1. **The 242 pictures.** Open the Collection on `E:\amiga\Titles`, press
   *Use the pictures in my files*, and count what appears. Record the real
   number — `written`, `adopted`, `missed` — in STATUS.
2. **A floppy title.** Play a bare `.adf` title and a `.rp9` title. Both should
   reach a game.
3. **A WHDLoad title**, against `E:\amiga\amikit\AmiKit.hdf`. Try Y2 first. If
   it does not reach the game, record **what the screen actually showed** and
   file it as an `ART-NNN` — the fallback existing is not the same as the
   fallback being needed, and which one happened is the finding.

- [ ] **Step 2: Write down what happened**

- `docs/FEATURES.md`: flip the rows for the offline pictures, the manual
  attach, the detail panel and Play — **only** where a test exists, and mark
  Play with what was actually verified on which material.
- `docs/ISSUES.md`: anything step 1 found, with its `ART-NNN`.
- `docs/STATUS.md`: a session-log row with the measured numbers, and the
  snapshot if the test counts moved.
- `CHANGELOG.md`: the user-visible half — pictures that were already on their
  disk, a panel, and a game that starts.

- [ ] **Step 3: Commit**

```bash
git add docs CHANGELOG.md
git commit -m "docs: wave C, and what driving it on real material found"
```
