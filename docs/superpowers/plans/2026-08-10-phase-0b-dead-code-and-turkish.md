# Phase 0b — Dead Code and Turkish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove every dead command, wrapper and page left behind by Stage W and Phase 0a, then ship ART in Turkish and English together.

**Architecture:** Slice 0.3 is pure subtraction — six code paths that are registered, typed and reachable from nothing. Slice 0.4 adds `tr.json` beside `en.json` and moves the twelve screens that still carry their English in the source onto `t()`. The language plumbing already exists end to end (`Settings` select → `settingsStore.update` → `changeLanguage` → `i18next`, with `App.tsx` applying the saved language at startup), so 0.4 adds a catalogue and a migration, not a mechanism. The guard that makes it safe is a **key-parity test**: `en.json` and `tr.json` must have identical key sets, so a half-translated build cannot ship.

**Tech Stack:** Rust (Tauri 2 commands), React 18 + TypeScript, `react-i18next`, Vite, pnpm.

## Global Constraints

- `src-tauri/src/core/` is platform-independent: `std` + `serde` + `sha2` + `thiserror` + `delharc` only. Never `use tauri`, never call Windows APIs, never touch the network. `commands/` is where Tauri lives.
- **MSRV is 1.77.** `Option::is_none_or` (1.82) and trait-object upcasting (1.86) compile locally and fail CI.
- Release profile is `panic = "abort"`: never index a block directly — use `blocks::block_slice`, `block_slice_mut`, `read_u32_at`, `write_u32_at`. Chain walks need a step limit. Never allocate from an unchecked length; use `checked_add` / `checked_mul`.
- Every write to a user file goes through `core/safety` (`atomic_write` / `guarded_write`). A failed operation leaves the original byte-for-byte unchanged.
- Fixtures are synthetic and generated at runtime in a tempdir. **ART ships no copyrighted Amiga content, ever.**
- Commands are registered in **both** `lib.rs`'s `invoke_handler![]` and a typed wrapper in `src/lib/*.ts`. Components never call `invoke` directly.
- Errors reaching the UI are readable sentences carrying a stable `ART-*` id from `CoreError::code()`.
- `lib.rs` allows only `dead_code`. Unused imports and variables are hard errors.
- Never claim support that is not implemented and tested (spec §10, §89).

**Gates — every task ends green on all of these, run from `amiga-retro-toolkit`:**

```
pnpm lint
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
python scripts/oracle-check.py
```

**Baseline at the start of this plan: 687 tests passing, oracle 48 checks.** Neither may drop except by the exact count of tests deliberately removed with a dead code path — and every such removal must be named in the task's report with its reason.

**Run the Rust suite twice before declaring a task done.** ART-059 was a race that failed about one run in five; a single green run is not evidence of a green suite.

---

## File Structure

**Slice 0.3 — removal**

| File | Change |
|---|---|
| `src-tauri/src/lib.rs` | drop 4 entries from `invoke_handler![]` |
| `src-tauri/src/commands/panel.rs` | delete `adf_extract_to`, `panel_plan_folder_copy` and their tests |
| `src-tauri/src/commands/lha.rs` | delete `lha_extract_job` |
| `src-tauri/src/commands/volume_write.rs` | delete the `volume_write_bytes` command wrapper, keep `write_bytes_into` |
| `src/lib/panel.ts` | delete `adfExtractTo`, `panelPlanFolderCopy` |
| `src/lib/volumeWrite.ts` | delete `volumeWriteBytes` |
| `src/lib/sources.ts` | delete `sourcesGet` |
| `src/pages/ComingLater.tsx` | delete the file |
| `src-tauri/src/core/adf/blocks.rs` | ART-047 — resolve the four orphaned helpers |
| `src-tauri/src/commands/adf.rs` | ART-048 — one stale comment |
| `docs/FEATURES.md` | ART-051 — eight raw control bytes |

**Slice 0.4 — translation**

| File | Responsibility |
|---|---|
| `src/i18n/en.json` | the English catalogue, grown from 7 namespaces to one per screen |
| `src/i18n/tr.json` | **new** — the Turkish catalogue, same key set |
| `src/i18n/index.ts` | register `tr`, widen `SUPPORTED_LANGUAGES` |
| `src/i18n/parity.test.ts` | **new** — the key-parity guard |
| `src/pages/*.tsx` (12 screens) | English literals replaced with `t()` |
| `src/components/**/*.tsx` (6 files) | same |
| `src/pages/Settings.tsx` | the switcher shows native language names |
| `src/App.tsx` | drop the `as "en"` cast |

---

## Slice 0.3 — Dead code

### Task 1: Remove the six dead code paths

**Files:**
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/commands/panel.rs`, `src-tauri/src/commands/lha.rs`, `src-tauri/src/commands/volume_write.rs`
- Modify: `src/lib/panel.ts`, `src/lib/volumeWrite.ts`, `src/lib/sources.ts`
- Delete: `src/pages/ComingLater.tsx`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: nothing later tasks depend on. This task only removes.

**Context.** Each of these was verified dead end to end before this plan was written — the Rust command exists, a typed wrapper may exist, and **no component calls it**. `ComingLater` is exported and never imported; `App.tsx` has no route for it. Do not take that on trust: re-verify each before deleting, because a caller may have been added since.

- [ ] **Step 1: Prove each path is still dead**

Run each of these and confirm the only hits are the definition itself, this plan, and historical entries in `CHANGELOG.md` / `docs/`:

```bash
grep -rn "adfExtractTo\|adf_extract_to" src src-tauri/src
grep -rn "panelPlanFolderCopy\|panel_plan_folder_copy" src src-tauri/src
grep -rn "volumeWriteBytes\|volume_write_bytes" src src-tauri/src
grep -rn "lha_extract_job" src src-tauri/src
grep -rn "sourcesGet" src
grep -rn "ComingLater" src
```

**If any of them has a real caller, stop and report it — do not delete a live path.** `panel_plan_folder_copy` has tests in `commands/panel.rs`; tests are not callers for this purpose, and they go with the function.

- [ ] **Step 2: Remove the frontend wrappers and the page**

Delete `adfExtractTo` and `panelPlanFolderCopy` from `src/lib/panel.ts`, `volumeWriteBytes` from `src/lib/volumeWrite.ts`, `sourcesGet` from `src/lib/sources.ts`, and the file `src/pages/ComingLater.tsx`. Remove any type that only those functions used — but only if nothing else references it.

`en.json` has a `common.comingLater` key. **Leave it.** Spec §96 says planned-but-unimplemented workflows render as "Coming Later" through `WorkflowInfo::available: false`; the string is still used for that badge even though the standalone page is not. Confirm that is true (`grep -rn "comingLater" src`) and say so in your report; if it turns out to have no user, remove it too.

- [ ] **Step 3: Run the frontend gate**

Run: `pnpm lint`
Expected: PASS. If it fails with "declared but never used" on an import inside a file you edited, remove that import too.

- [ ] **Step 4: Remove the Rust commands**

Delete `adf_extract_to` and `panel_plan_folder_copy` (and their tests) from `commands/panel.rs`, `lha_extract_job` from `commands/lha.rs`, and the `volume_write_bytes` command from `commands/volume_write.rs`.

**Keep `write_bytes_into`** in `volume_write.rs` — it is the shared implementation used by the checkout check-in path, not part of the dead command. Verify with `grep -rn "write_bytes_into" src-tauri/src` before touching it.

Then remove the four matching lines from `invoke_handler![]` in `lib.rs`:

```
commands::lha::lha_extract_job,
commands::panel::adf_extract_to,
commands::panel::panel_plan_folder_copy,
commands::volume_write::volume_write_bytes,
```

- [ ] **Step 5: Run the Rust gates**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: PASS. The test count drops by exactly the number of tests you deleted with `panel_plan_folder_copy` — count them and state the number in your report. Any other drop means you removed something live.

Run `cargo test` a second time and confirm the same number.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "Remove six code paths nothing reaches

adf_extract_to, panel_plan_folder_copy and volume_write_bytes were
superseded by their Stage W equivalents; lha_extract_job was registered
with no typed wrapper; sourcesGet and the ComingLater page have no
callers and no route. Each was re-verified dead end to end before
deletion."
```

### Task 2: Close the three hygiene issues

**Files:**
- Modify: `src-tauri/src/core/adf/blocks.rs` (ART-047)
- Modify: `src-tauri/src/commands/adf.rs` (ART-048)
- Modify: `docs/FEATURES.md` (ART-051)
- Modify: `docs/ISSUES.md`

**Interfaces:**
- Consumes: nothing.
- Produces: nothing.

- [ ] **Step 1: ART-047 — decide the fate of the four bounds-checking helpers**

`core/adf/blocks.rs` defines `block_slice`, `block_slice_mut`, `read_u32_at` and `write_u32_at`. `core/adf/mutate.rs` was their last production caller and Phase 0a deleted it. `lib.rs`'s `#![allow(dead_code)]` means clippy cannot see this.

There is a real tension here and the report must resolve it explicitly rather than silently picking a side:

- CLAUDE.md instructs *"Never index the image directly — use `blocks::block_slice`, `block_slice_mut`, `read_u32_at`, `write_u32_at`"*. Deleting them makes that instruction point at nothing.
- Keeping audited code with no production caller is exactly what ART-020 exists to stop CI from hiding.

`core/adf/validate.rs` around lines 191–212 currently bounds-checks by hand before indexing directly. **First check whether it can use these helpers instead.** If it can, that is the resolution: give them a real caller, and CLAUDE.md's rule stays true. If it genuinely cannot — say precisely why in the report — then delete the four functions, keep `block_offset` if anything still uses it, and note in the report that CLAUDE.md's bounds-checking paragraph needs rewording to name the surviving API (`core/volume`'s `BlockSet` and `layout` helpers).

Do not weaken any bounds check to make a helper fit. Whichever way you go, the tests in `blocks.rs` go with the decision: they move to the caller, or they go with the functions.

- [ ] **Step 2: Run the Rust gates**

Run: `cargo clippy --all-targets -- -D warnings && cargo test`
Expected: PASS.

- [ ] **Step 3: ART-048 — the stale comment**

`commands/adf.rs` near line 369 has a test doc comment reading "a later task deleting `core/adf/mutate` would be unsafe". That task happened. Reword it to describe what the test actually guards now. Search the whole tree for any other surviving mention: `grep -rn "mutate_disk_file\|adf/mutate" src src-tauri/src`. Historical entries in `docs/ISSUES.md` and `CHANGELOG.md` are the record and stay.

- [ ] **Step 4: ART-051 — the control bytes in FEATURES.md**

`docs/FEATURES.md` carries eight bytes in the range `0x00`–`0x07` as literal control characters, in the DosType write-matrix table around line 140, where the escaped text `\x00` … `\x07` was meant. They render as empty backticks and make git classify the file as binary, so its diffs cannot be read.

Find them:

```bash
python -c "d=open('docs/FEATURES.md','rb').read(); print([(i,b) for i,b in enumerate(d) if b<9])"
```

Replace each with its escaped text so the table reads `DOS\x00`, `DOS\x01` … matching the style the rest of the file uses. Preserve CRLF endings and change nothing else.

- [ ] **Step 5: Verify the file is text again**

Run: `git diff --stat -- docs/FEATURES.md`
Expected: a line count such as `docs/FEATURES.md | 8 ++--`, **not** `Bin 14510 -> 14502 bytes`. If it still says `Bin`, a control byte remains — re-run the finder.

- [ ] **Step 6: Move the three issues to Fixed and commit**

Move ART-047, ART-048 and ART-051 from Open to Fixed in `docs/ISSUES.md`, each with the resolution and, for ART-047, the reasoning. Match the file's existing house style.

```bash
git add -A
git commit -m "Close three hygiene issues left by Phase 0a

ART-047 the orphaned bounds-checking helpers, ART-048 a comment
describing a deleted module, ART-051 the control bytes that made
FEATURES.md a binary file to git."
```

---

## Slice 0.4 — Turkish and i18n

### Task 3: The catalogue infrastructure and the parity guard

**Files:**
- Create: `src/i18n/tr.json`
- Create: `src/i18n/parity.test.ts`
- Modify: `src/i18n/index.ts`, `src/i18n/en.json`, `src/pages/Settings.tsx`, `src/App.tsx`
- Modify: `package.json` (a test runner, if none is configured)

**Interfaces:**
- Consumes: nothing.
- Produces: for every later task — `SUPPORTED_LANGUAGES: readonly ["en", "tr"]`, `type Language = "en" | "tr"`, `LANGUAGE_NAMES: Record<Language, string>`, and the rule that **every key added to `en.json` must be added to `tr.json` in the same commit**, enforced by `parity.test.ts`.

**Context.** The switcher already works end to end: `Settings.tsx` renders a `<select>` over `SUPPORTED_LANGUAGES`, `settingsStore.update` calls `changeLanguage`, and `App.tsx` applies the saved language on startup. This task widens that from one language to two and adds the guard.

- [ ] **Step 1: Check whether a frontend test runner exists**

Run: `cat package.json`

If there is no `test` script and no `vitest` dependency, add Vitest — it is the standard runner for a Vite project and needs no extra config to run a plain `.test.ts`:

```bash
pnpm add -D vitest
```

Then add to `package.json`'s `scripts`:

```json
"test": "vitest run"
```

If a runner is already configured, use it and adapt the test in Step 2 to its API.

- [ ] **Step 2: Write the failing parity test**

Create `src/i18n/parity.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import en from "./en.json";
import tr from "./tr.json";

/** Every leaf key, as dotted paths, sorted. */
function keysOf(obj: unknown, prefix = ""): string[] {
  if (typeof obj !== "object" || obj === null) return [prefix];
  return Object.entries(obj as Record<string, unknown>)
    .flatMap(([k, v]) => keysOf(v, prefix ? `${prefix}.${k}` : k))
    .sort();
}

describe("the translation catalogues", () => {
  it("have identical key sets", () => {
    const enKeys = keysOf(en);
    const trKeys = keysOf(tr);
    expect(trKeys.filter((k) => !enKeys.includes(k))).toEqual([]);
    expect(enKeys.filter((k) => !trKeys.includes(k))).toEqual([]);
  });

  it("have no empty translations", () => {
    const empty = (obj: unknown, prefix = ""): string[] => {
      if (typeof obj === "string") return obj.trim() === "" ? [prefix] : [];
      if (typeof obj !== "object" || obj === null) return [];
      return Object.entries(obj as Record<string, unknown>).flatMap(([k, v]) =>
        empty(v, prefix ? `${prefix}.${k}` : k),
      );
    };
    expect(empty(en)).toEqual([]);
    expect(empty(tr)).toEqual([]);
  });
});
```

This is the guard that makes the rest of the slice safe: a screen migrated to `t()` with no Turkish behind it fails the build instead of silently showing English to a Turkish user.

- [ ] **Step 3: Run it to verify it fails**

Run: `pnpm test`
Expected: FAIL — `Cannot find module './tr.json'`.

- [ ] **Step 4: Create `tr.json` as a full translation of today's `en.json`**

Translate all seven existing namespaces. **Keep Amiga terms of art in their original form** — they are the names of things, and a Turkish Amiga user calls them exactly this: `ADF`, `ADZ`, `DMS`, `HDF`, `LHA`, `ROM`, `Kickstart`, `Workbench`, `WHDLoad`, `Gotek`, `PiStorm`, `WinUAE`, `Aminet`, `bootblock`, `RDB`, `OFS`, `FFS`. Translate the surrounding sentence, not the term.

```json
{
  "app": {
    "name": "Amiga Retro Toolkit",
    "tagline": "Amiga Dosyaları İçin İsviçre Çakısı"
  },
  "nav": {
    "dashboard": "Panel",
    "diskTools": "Disket Araçları",
    "archiveTools": "Arşiv Araçları",
    "hardDisk": "Sabit Disk",
    "gotek": "Gotek",
    "rom": "ROM Yöneticisi",
    "winuae": "WinUAE",
    "pistorm": "PiStorm",
    "collection": "Koleksiyon",
    "tools": "Araçlar",
    "settings": "Ayarlar",
    "aminet": "Aminet",
    "files": "Dosyalar",
    "whdload": "WHDLoad Kur"
  },
  "dashboard": {
    "dropHere": "AMIGA DOSYALARINI BURAYA BIRAKIN",
    "dropHint": "ADF • ADZ • DMS • HDF • LHA • ROM • KLASÖR",
    "dropSubhint": "ART bunları çözümleyecek.",
    "recent": "Son Kullanılanlar",
    "noRecent": "Henüz dosya yok. Başlamak için bir şey bırakın.",
    "quickActions": "Hızlı İşlemler",
    "statistics": "Koleksiyon İstatistikleri",
    "noStats": "Koleksiyon boş."
  },
  "common": {
    "comingLater": "Sonra Gelecek",
    "cancel": "İptal",
    "continue": "Devam",
    "ok": "Tamam",
    "open": "Aç",
    "close": "Kapat",
    "unknown": "Bilinmiyor"
  },
  "settings": {
    "title": "Ayarlar",
    "general": "Genel",
    "appearance": "Görünüm",
    "paths": "Yollar",
    "theme": "Tema",
    "themeDark": "Koyu",
    "themeLight": "Açık",
    "uxMode": "Deneyim Modu",
    "uxBeginner": "Başlangıç",
    "uxPower": "İleri Kullanıcı",
    "language": "Dil",
    "winuaePath": "WinUAE Yolu",
    "collectionDir": "Koleksiyon Klasörü"
  },
  "status": {
    "healthy": "Sağlıklı",
    "warning": "Uyarı",
    "problem": "Sorun"
  },
  "phase0": {
    "engineReady": "İş Akışı Motoru: HAZIR",
    "dragDropReady": "Sürükle & Bırak: HAZIR",
    "dbReady": "Veritabanı: HAZIR"
  }
}
```

- [ ] **Step 5: Run the parity test to verify it passes**

Run: `pnpm test`
Expected: PASS, both cases.

- [ ] **Step 6: Register Turkish**

In `src/i18n/index.ts`, import `tr.json`, widen the language list, add native display names, and correct the file's opening comment — it currently says "v1.x ships English only":

```ts
import en from "./en.json";
import tr from "./tr.json";

export const SUPPORTED_LANGUAGES = ["en", "tr"] as const;
export type Language = (typeof SUPPORTED_LANGUAGES)[number];

/** Shown in the switcher. A language is named in itself, never translated. */
export const LANGUAGE_NAMES: Record<Language, string> = {
  en: "English",
  tr: "Türkçe",
};
```

and add `tr: { translation: tr }` to `resources`.

- [ ] **Step 7: Show native names in the switcher**

`src/pages/Settings.tsx` currently renders `{lng.toUpperCase()}`, which would show "EN" and "TR". Import `LANGUAGE_NAMES` and render `{LANGUAGE_NAMES[lng]}` instead. A user who cannot read the current interface language must still be able to find their own language in that list — that is why the names are not translated.

- [ ] **Step 8: Drop the narrowing cast**

`src/App.tsx` calls `changeLanguage((settings.language ?? "en") as "en")`. That cast now lies. Replace it so a stored `"tr"` is honoured, and fall back to `"en"` when the stored value is not a supported language:

```ts
const stored = settings.language;
const lang: Language = (SUPPORTED_LANGUAGES as readonly string[]).includes(stored ?? "")
  ? (stored as Language)
  : "en";
void safe(changeLanguage(lang), "language");
```

Import `SUPPORTED_LANGUAGES` and `type Language` from `@/i18n`.

- [ ] **Step 9: Run the gates**

Run: `pnpm lint && pnpm test`
Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "Ship Turkish beside English

Adds tr.json, registers it, and shows both languages by their own names
in the switcher. A parity test fails the build if the two catalogues
drift apart or a translation is left empty, so a half-translated screen
cannot ship."
```

### Task 4: Translate the two largest screens

**Files:**
- Modify: `src/pages/AminetStudio.tsx`, `src/pages/AdfBrowser.tsx`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json`

**Interfaces:**
- Consumes: `SUPPORTED_LANGUAGES`, `LANGUAGE_NAMES`, `type Language` from Task 3; the parity test.
- Produces: the namespace convention every later screen follows — one top-level key per screen, named after the screen in camelCase (`aminet`, `adf`), with nested groups for repeated areas.

**Context.** These two carry the most English: roughly 25 and 19 directly visible strings. They set the pattern, which is why they come first rather than last.

- [ ] **Step 1: Add the two namespaces to both catalogues**

Work one screen at a time. Read the screen, list every user-visible string, and add a key for each under its namespace in `en.json` **and** `tr.json` together. Never add to one alone — the parity test will fail, which is the point.

Naming: `aminet.search.placeholder`, `aminet.table.size`, `adf.validate.title`. Group by area, not by component nesting depth. Reuse `common.*` for genuinely shared words (Cancel, OK, Close) rather than creating `aminet.cancel`.

Translation rules for the Turkish side:
- Keep Amiga terms of art unchanged: `ADF`, `HDF`, `LHA`, `ROM`, `Kickstart`, `Workbench`, `WHDLoad`, `Aminet`, `bootblock`, `RDB`, `OFS`, `FFS`, `slave`.
- Translate the sentence around them naturally. Do not translate word by word.
- Use the imperative for buttons (`Tara`, `Kur`, `Doğrula`), not the infinitive.
- Keep it short — Turkish runs longer than English and these sit in fixed-width buttons and table headers.

- [ ] **Step 2: Replace the literals**

Add `const { t } = useTranslation();` where a screen does not already have it, then replace each literal with its `t("…")` call. For a string with a value in it, use interpolation rather than concatenation:

```tsx
// Before
<p>Found {count} packages</p>
// After
<p>{t("aminet.foundPackages", { count })}</p>
```

with `"foundPackages": "{{count}} paket bulundu"` in `tr.json` and `"{{count}} packages found"` in `en.json`.

**Do not translate:** file paths, `ART-*` error ids, format names in a technical table, or any string that Rust produced. Error sentences come from `CoreError` and are out of scope for this slice — leave them.

- [ ] **Step 3: Verify no English is left**

Run: `grep -nE '>[A-Za-z][^<>{}]{6,}<' src/pages/AminetStudio.tsx src/pages/AdfBrowser.tsx`
Expected: no user-visible English sentences. Hits that are `{t("…")}` calls, numbers, or Amiga terms of art are fine. Report anything you left and why.

- [ ] **Step 4: Run the gates**

Run: `pnpm lint && pnpm test`
Expected: PASS. The parity test proves both catalogues grew together.

- [ ] **Step 5: Check both languages actually render**

Run: `pnpm tauri dev`, open Aminet and ADF Studio, switch the language in Settings, and confirm the screen changes without a reload and nothing overflows its button or column. Turkish is the longer language; this is where a fixed width breaks. Report anything that clips — fixing the layout is in scope for this task.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "Translate the Aminet and ADF screens

Sets the namespace convention: one top-level key per screen, common
words shared, Amiga terms of art left in their own names."
```

### Task 5: Translate the hardware screens

**Files:**
- Modify: `src/pages/HardDiskStudio.tsx`, `src/pages/PistormStudio.tsx`, `src/pages/GotekStudio.tsx`, `src/pages/WinuaeStudio.tsx`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json`

**Interfaces:**
- Consumes: the namespace convention from Task 4 — one top-level key per screen in camelCase, `common.*` for shared words, both catalogues edited together.
- Produces: nothing later tasks depend on.

Roughly 16, 11, 11 and 5 visible strings. Namespaces: `hardDisk`, `pistorm`, `gotek`, `winuae`.

- [ ] **Step 1: Add the four namespaces to both catalogues**

Same rules as Task 4: read each screen, list every user-visible string, add the key to `en.json` and `tr.json` in the same edit. Keep the Amiga and hardware terms of art unchanged — `Gotek`, `FlashFloppy`, `FF.CFG`, `PiStorm`, `Emu68`, `WinUAE`, `RDB`, `HDF`, `Kickstart`, `config.txt`, `cmdline.txt`. Filenames are never translated.

- [ ] **Step 2: Replace the literals**

Add `const { t } = useTranslation();` where missing and replace each literal. Use interpolation for embedded values, never string concatenation.

- [ ] **Step 3: Verify no English is left**

Run: `grep -nE '>[A-Za-z][^<>{}]{6,}<' src/pages/HardDiskStudio.tsx src/pages/PistormStudio.tsx src/pages/GotekStudio.tsx src/pages/WinuaeStudio.tsx`
Expected: no user-visible English sentences; report anything deliberately left.

- [ ] **Step 4: Run the gates**

Run: `pnpm lint && pnpm test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "Translate the hard disk, PiStorm, Gotek and WinUAE screens"
```

### Task 6: Translate the remaining screens

**Files:**
- Modify: `src/pages/RomStudio.tsx`, `src/pages/HexTools.tsx`, `src/pages/LhaBrowser.tsx`, `src/pages/FileManager.tsx`, `src/pages/CollectionStudio.tsx`, `src/pages/WhdloadInstall.tsx`, `src/pages/Settings.tsx`, `src/pages/Dashboard.tsx`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json`

**Interfaces:**
- Consumes: the namespace convention from Task 4.
- Produces: nothing later tasks depend on.

Namespaces: `rom`, `hex`, `lha`, `files`, `collection`, `whdload`. `settings` and `dashboard` already exist — extend them rather than creating a second namespace.

- [ ] **Step 1: Add the namespaces to both catalogues**

Same rules. Terms of art to leave alone here: `Kickstart`, `AROS`, `SHA256`, `LHA`, `WHDLoad`, `slave`, `Workbench`, and every filename and path.

`WhdloadInstall.tsx` is a special case: its refusal reasons and suggestions come from Rust as data (`WhdloadRefusal { reason, suggestion }`) and are **out of scope** — do not try to translate them from the frontend. Translate only the surrounding page chrome, and note the Rust-side strings in your report as owed work for a later slice.

- [ ] **Step 2: Replace the literals**

Add `const { t } = useTranslation();` where missing and replace each literal.

- [ ] **Step 3: Verify no English is left**

Run: `grep -nE '>[A-Za-z][^<>{}]{6,}<' src/pages/*.tsx`
Expected: across all pages, no user-visible English sentences remain except the Rust-sourced ones named above. Report the full list of what you left and why.

- [ ] **Step 4: Run the gates**

Run: `pnpm lint && pnpm test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "Translate the remaining screens

Leaves the WHDLoad refusal reasons alone: they are data from Rust, and
translating them belongs with the Rust error strings, not here."
```

### Task 7: Translate the shared components and close the slice

**Files:**
- Modify: `src/components/files/CopyPlanDialog.tsx`, `src/components/files/CheckoutPanel.tsx`, `src/components/files/AttributesDialog.tsx`, `src/components/files/FunctionKeys.tsx`, `src/components/layout/Sidebar.tsx`, `src/components/ErrorBoundary.tsx`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json`
- Modify: `docs/FEATURES.md`, `docs/STATUS.md`, `docs/ISSUES.md`, `CHANGELOG.md`, `README.md`

**Interfaces:**
- Consumes: the namespace convention from Task 4.
- Produces: the finished slice.

- [ ] **Step 1: Translate the components**

Namespace `components.*`, grouped by component: `components.copyPlan.*`, `components.attributes.*`, `components.fnKeys.*`.

`FunctionKeys.tsx` is the Norton Commander function-key bar. Its labels are short by necessity (`Copy`, `Move`, `MkDir`, `Delete`) and Turkish equivalents must be equally short or the bar wraps: `Kopyala`, `Taşı`, `Klasör`, `Sil`. Check the rendered width, do not just translate.

`ErrorBoundary.tsx` is the last thing a user sees when the UI has crashed. Its message must be translated, but it must also still render if i18n itself failed to initialise — so give it a hardcoded English fallback rather than a bare `t()` call, and say in your report how you did that.

- [ ] **Step 2: Verify no English is left anywhere**

Run: `grep -rnE '>[A-Za-z][^<>{}]{6,}<' src/components src/pages`
Expected: only Rust-sourced strings and Amiga terms of art. Produce the complete list in your report — this is the evidence the slice is finished.

- [ ] **Step 3: Run every gate**

Run:
```
pnpm lint && pnpm test
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
python scripts/oracle-check.py
```
Expected: all PASS. Run `cargo test` twice.

- [ ] **Step 4: Check both languages end to end**

Run `pnpm tauri dev`. Walk every screen in Turkish, then switch to English and walk them again. Confirm the language survives a restart of the app (it is stored in `settings.json`). Report any string that did not change, anything that overflows, and any screen where switching the language left stale text.

- [ ] **Step 5: Update the documents**

- `docs/FEATURES.md` — flip the i18n row, but only for what is really done. The Rust-side error strings and the WHDLoad refusal reasons are still English; say so rather than claiming a fully bilingual application (spec §10, §89).
- `docs/STATUS.md` — snapshot numbers and a session-log line.
- `docs/ISSUES.md` — open an `ART-NNN` for the Rust-side strings that remain English, so the gap is recorded rather than forgotten.
- `CHANGELOG.md` — a user-visible entry.
- `README.md` — mention that ART ships in English and Turkish.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "Translate the shared components and record what is still English

The Rust-side error sentences and the WHDLoad refusal reasons are data
from the core and stay English for now; recorded rather than claimed as
done."
```

---

## Self-Review

**Spec coverage.** The roadmap's slice 0.3 lists `adf_extract_to`, `panel_plan_folder_copy`, `volume_write_bytes`, `lha_extract_job`, `sourcesGet`, the `ComingLater` page, and stale `FEATURES.md` rows — Task 1 covers the six code paths, Task 2 covers `FEATURES.md`'s corruption, and the stale rows were already corrected in Phase 0a's Task 11. ART-047 and ART-048, which the roadmap could not know about because Phase 0a created them, are folded into Task 2. Slice 0.4 lists "twelve screens moved onto `t()`, `tr.json`, a language switcher, both languages shipped" — Tasks 3–7 cover all four, and Task 3 adds the parity guard the roadmap did not ask for but which is what stops the slice half-landing.

**Placeholders.** None: every step names its files, its command and its expected result, and the translation task carries the full `tr.json` for the existing catalogue rather than describing it.

**Type consistency.** `SUPPORTED_LANGUAGES`, `Language` and `LANGUAGE_NAMES` are defined once in Task 3 Step 6 and used with those exact names in Steps 7 and 8 and in every later task's Interfaces block. The namespace convention is stated once in Task 4's Produces and referenced by name in Tasks 5, 6 and 7.

**One deliberate scope exclusion,** stated in three places so it cannot be missed: strings that Rust produces — `CoreError` sentences and the WHDLoad refusal reasons — stay English in this slice, and Task 7 opens an issue for them. Translating them means translating in the core, and `core/` may not depend on the frontend's catalogue. That is a design question this plan does not answer.
