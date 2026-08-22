# Package Bundles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a curated catalogue of Amiga software as data, grouped into named sets, and let the user pick a set and have ART download its packages in order.

**Architecture:** A new `core/sources/bundle/` module holds the catalogue and the sets as `include_str!`-ed JSON, the same way `core/osinstall/recipes/` already ships install recipes. Nothing new is invented for fetching: an entry resolves to the `PackageMeta` + `Mirror` pair `core/sources/fetch.rs::fetch_package` already takes, and lands in the user's library through `core/sources/library.rs::Library::place`. A bulk download is one `core/jobs` job that walks the entries **in order**, recording a separate outcome per entry.

**Tech Stack:** Rust (`core/sources`, `core/jobs`, `serde`), Tauri commands, React + `react-i18next`.

**Spec:** [docs/superpowers/specs/2026-08-22-package-bundles-design.md](../specs/2026-08-22-package-bundles-design.md)

## Global Constraints

- **Phase 1 downloads only.** Nothing here installs onto an Amiga volume. A task that reaches for `core/osinstall` or `core/amigainstall` is out of scope.
- **`core/` stays platform-independent.** No `use tauri`, no Windows APIs, no direct network. The network is reached only through the existing `MirrorClient` trait (`core/sources/mirror.rs`).
- **No function anywhere takes a caller-supplied URL.** Every fetch is built from a configured `Mirror` plus a validated repository path (§41.5.7). The `source` enum has no URL variant, and that is the enforcement.
- **A fetch is user-initiated.** Nothing downloads on load, on scan, or on render.
- **Every write goes through `core/safety`.** `atomic_write`, never `std::fs::write`.
- **Strings:** Rust-side messages stay English (ART-060). Anything rendered comes from `src/i18n/en.json` **and** `tr.json`, changed in the same commit — `pnpm test` fails the build otherwise.
- **Errors:** `CoreError` in `core/`, wrapped by `AppError` at the command layer.
- **Cancellation is checked between entries, never mid-write.**
- **Test fixtures are synthetic and generated at runtime in a tempdir** through `core::ScratchDir`, which removes itself on `Drop`. ART ships no copyrighted Amiga content.
- **A test is not a guard until the defect has been put back and seen to fail it.** Every task's final step names the mutation to run.

## File Structure

| File | Responsibility |
|---|---|
| `src-tauri/src/core/sources/bundle/mod.rs` | `BundleEntry`, `Bundle`, `EntrySource`, `Permission`; `entries()`, `bundles()`, `entry_by_id()` |
| `src-tauri/src/core/sources/bundle/parse.rs` | `serde` shapes and `validate` — the gate every shipped JSON passes |
| `src-tauri/src/core/sources/bundle/catalogue/*.json` | One JSON per set, each carrying its own entries |
| `src-tauri/src/core/sources/bundle/resolve.rs` | `EntrySource` → `Resolved { meta, mirrors }` or `Refusal` |
| `src-tauri/src/core/sources/bundle/run.rs` | The sequential job: `download_bundle(...) -> BundleReport` |
| `src-tauri/src/commands/bundles.rs` | `bundles_list`, `bundles_download` (a job), `bundles_report` |
| `src/lib/bundles.ts` | Typed wrappers + the pure helpers the panel renders from |
| `src/components/sources/BundlePanel.tsx` | The set list, the permission warning, the report |
| `scripts/catalogue-check.py` | Asks the live sources whether all 62 paths resolve |

---

### Task 1: The bundle shape, and one set that proves it

**Files:**
- Create: `src-tauri/src/core/sources/bundle/mod.rs`
- Create: `src-tauri/src/core/sources/bundle/parse.rs`
- Create: `src-tauri/src/core/sources/bundle/catalogue/arsiv.json`
- Modify: `src-tauri/src/core/sources/mod.rs` (add `pub mod bundle;`)

**Interfaces:**
- Consumes: `crate::core::error::{CoreError, CoreResult}`; `crate::core::sources::PackageRef`.
- Produces:
  - `pub struct BundleEntry { pub id: String, pub name: String, pub source: EntrySource, pub order: u32, pub exclusive_group: Option<String>, pub requires: Vec<String>, pub permission: Option<Permission> }`
  - `pub enum EntrySource { Aminet { path: String }, AminetSearch { query: String }, GithubRelease { repo: String, asset: String }, Mirror { mirror: String, path: String }, UserSupplied { why: String } }`
  - `pub struct Permission { pub holder: String, pub note: String }`
  - `pub struct Bundle { pub id: String, pub order: u32, pub entries: Vec<BundleEntry> }`
  - `pub fn bundles() -> CoreResult<Vec<Bundle>>`
  - `pub fn entries() -> CoreResult<Vec<BundleEntry>>` — every set's entries, flattened, in download order
  - `pub fn entry_by_id(id: &str) -> CoreResult<Option<BundleEntry>>`

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/core/sources/bundle/mod.rs` with only the test module:

```rust
//! The curated catalogue, as data — see
//! `docs/superpowers/specs/2026-08-22-package-bundles-design.md`.

#[cfg(test)]
mod tests {
    #[test]
    fn the_archivers_set_ships_and_parses() {
        let all = super::bundles().expect("the shipped bundles must parse");
        let arsiv = all
            .iter()
            .find(|b| b.id == "arsiv")
            .expect("the archivers set is shipped");
        let ids: Vec<&str> = arsiv.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["lha", "lzx", "unzip", "zip", "xadmaster", "xpkuser"]);
    }

    #[test]
    fn the_archivers_set_is_first_in_download_order() {
        // Everything else arrives as .lha or .lzx; the order is data, not a
        // comment. `entries()` flattens in that order.
        let first = super::entries().unwrap().into_iter().next().unwrap();
        assert_eq!(first.id, "lha");
    }

    #[test]
    fn an_entry_naming_no_source_is_refused() {
        let json = r#"{ "id": "x", "order": 1, "entries": [
            { "id": "e", "name": "E", "order": 1 } ] }"#;
        let err = super::parse::parse(json).expect_err("a source is required");
        assert!(format!("{err}").contains("source"), "got: {err}");
    }
}
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cd src-tauri && cargo test sources::bundle
```
Expected: compile error — `bundles`, `entries`, `parse` do not exist.

- [ ] **Step 3: Write `parse.rs`**

```rust
//! `serde` shapes for a shipped bundle file, and the gate every one passes.
//!
//! Deliberately separate from `mod.rs`'s public types: the wire shape may
//! grow a field the domain type folds away, the same split
//! `core/osinstall/package.rs` already draws between `RawPackage` and
//! `Package`.

use serde::Deserialize;

use super::{Bundle, BundleEntry, EntrySource, Permission};
use crate::core::error::{CoreError, CoreResult};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBundle {
    id: String,
    order: u32,
    entries: Vec<RawEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    id: String,
    name: String,
    /// Required. A missing `source` is a refusal, not a default: a default
    /// would have to invent where a package comes from.
    source: EntrySource,
    order: u32,
    #[serde(default)]
    exclusive_group: Option<String>,
    #[serde(default)]
    requires: Vec<String>,
    #[serde(default)]
    permission: Option<Permission>,
}

pub fn parse(json: &str) -> CoreResult<Bundle> {
    let raw: RawBundle = serde_json::from_str(json).map_err(|e| CoreError::Malformed {
        format: "bundle".into(),
        detail: e.to_string(),
    })?;
    let mut entries: Vec<BundleEntry> = raw
        .entries
        .into_iter()
        .map(|e| BundleEntry {
            id: e.id,
            name: e.name,
            source: e.source,
            order: e.order,
            exclusive_group: e.exclusive_group,
            requires: e.requires,
            permission: e.permission,
        })
        .collect();
    entries.sort_by_key(|e| e.order);
    let bundle = Bundle {
        id: raw.id,
        order: raw.order,
        entries,
    };
    validate(&bundle)?;
    Ok(bundle)
}

/// What a shipped file must get right: an id, and no two entries sharing one.
fn validate(bundle: &Bundle) -> CoreResult<()> {
    if bundle.id.trim().is_empty() {
        return Err(CoreError::Malformed {
            format: "bundle".into(),
            detail: "a bundle names no id".into(),
        });
    }
    let mut seen = std::collections::HashSet::new();
    for entry in &bundle.entries {
        if !seen.insert(entry.id.as_str()) {
            return Err(CoreError::Malformed {
                format: "bundle".into(),
                detail: format!("'{}': two entries share the id '{}'", bundle.id, entry.id),
            });
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Write the types and loaders in `mod.rs`**

Above the test module:

```rust
pub mod parse;

use serde::{Deserialize, Serialize};

use crate::core::error::CoreResult;

/// Where an entry's file comes from. **A closed enum with no URL variant** —
/// that absence is §41.5.7's guarantee, expressed as a type rather than as a
/// rule somebody has to remember.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntrySource {
    /// A fixed Aminet repository path: `disk/misc/PFS3_53`.
    Aminet { path: String },
    /// "Latest version", resolved through the catalog rather than pinned.
    AminetSearch { query: String },
    GithubRelease { repo: String, asset: String },
    /// A **configured** mirror by name, plus a path below its base.
    Mirror { mirror: String, path: String },
    /// ART cannot fetch this, and says so before the user asks it to.
    UserSupplied { why: String },
}

/// A licence or permission condition the screen must state before the tick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permission {
    pub holder: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BundleEntry {
    pub id: String,
    pub name: String,
    pub source: EntrySource,
    pub order: u32,
    pub exclusive_group: Option<String>,
    pub requires: Vec<String>,
    pub permission: Option<Permission>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Bundle {
    pub id: String,
    pub order: u32,
    pub entries: Vec<BundleEntry>,
}

const ARSIV_JSON: &str = include_str!("catalogue/arsiv.json");

/// Every shipped file. Add a set by adding a `const` and a line here — the
/// same "a fourth package is a JSON file, not a code path" rule the install
/// recipes follow.
const SHIPPED: &[&str] = &[ARSIV_JSON];

pub fn bundles() -> CoreResult<Vec<Bundle>> {
    let mut all: Vec<Bundle> = SHIPPED
        .iter()
        .map(|json| parse::parse(json))
        .collect::<CoreResult<Vec<Bundle>>>()?;
    all.sort_by_key(|b| b.order);
    Ok(all)
}

/// Every set's entries, flattened in download order: set order, then entry
/// order.
pub fn entries() -> CoreResult<Vec<BundleEntry>> {
    Ok(bundles()?.into_iter().flat_map(|b| b.entries).collect())
}

pub fn entry_by_id(id: &str) -> CoreResult<Option<BundleEntry>> {
    Ok(entries()?.into_iter().find(|e| e.id == id))
}
```

- [ ] **Step 5: Write `catalogue/arsiv.json`**

```json
{
  "id": "arsiv",
  "order": 20,
  "entries": [
    { "id": "lha", "name": "LHA", "order": 10, "source": { "aminet": { "path": "util/arc/lha_68k" } } },
    { "id": "lzx", "name": "LZX", "order": 20, "source": { "aminet": { "path": "util/arc/lzx121r1" } } },
    { "id": "unzip", "name": "UnZip 5.52", "order": 30, "source": { "aminet": { "path": "util/arc/UnZIP552" } } },
    { "id": "zip", "name": "Zip 2.32", "order": 40, "source": { "aminet": { "path": "util/arc/ZIP232" } } },
    { "id": "xadmaster", "name": "XAD Master", "order": 50, "source": { "aminet": { "path": "util/arc/xadmaster020" } } },
    { "id": "xpkuser", "name": "XPK User", "order": 60, "source": { "aminet": { "path": "util/pack/xpk_User" } } }
  ]
}
```

Add `pub mod bundle;` to `src-tauri/src/core/sources/mod.rs`, beside the existing `pub mod cache;`.

- [ ] **Step 6: Run the tests**

```bash
cd src-tauri && cargo test sources::bundle
```
Expected: 3 passed.

- [ ] **Step 7: Mutate, and watch it fall**

Change `entries.sort_by_key(|e| e.order)` in `parse.rs` to a no-op and re-run: `the_archivers_set_is_first_in_download_order` must fail. Put it back.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/core/sources/bundle src-tauri/src/core/sources/mod.rs
git commit -m "feat(bundle): the catalogue's shape, and the archivers set that proves it"
```

---

### Task 2: The whole catalogue — 14 sets, 62 entries

**Files:**
- Create: `src-tauri/src/core/sources/bundle/catalogue/{emu68,dosya-sistemi,temel,ag,grafik,masaustu,teshis,acilis,kabuk,whdload,medya,amigaos-eki,ibrowse}.json`
- Modify: `src-tauri/src/core/sources/bundle/mod.rs` (the `SHIPPED` list)
- Modify: `THIRD_PARTY_LICENSES.md`

**Interfaces:**
- Consumes: Task 1's `Bundle`, `BundleEntry`, `EntrySource`, `Permission`, `bundles()`, `entries()`.
- Produces: no new API. The data, plus the guards over it.

The entries, their real sources and the four `permission` cases are enumerated in [docs/amiga-package-catalogue.md](../../amiga-package-catalogue.md). Copy them from there, **not** from the Emu68 Imager's page: that document already carries the corrections (ViNCEd's path, WHDLoadWrapper having no usable source, the three that are searches rather than paths).

- [ ] **Step 1: Write the failing tests**

Append to `bundle/mod.rs`'s test module:

```rust
    #[test]
    fn every_shipped_set_is_named_by_the_design() {
        let ids: Vec<String> = super::bundles().unwrap().into_iter().map(|b| b.id).collect();
        assert_eq!(
            ids,
            vec![
                "emu68", "arsiv", "dosya-sistemi", "temel", "ag", "grafik", "masaustu",
                "teshis", "acilis", "kabuk", "whdload", "medya", "amigaos-eki", "ibrowse",
            ],
            "14 sets, in download order"
        );
    }

    #[test]
    fn the_catalogue_holds_sixty_two_entries_and_no_id_twice() {
        let all = super::entries().unwrap();
        assert_eq!(all.len(), 62, "60 from the Imager's own list, plus tolunnet and tolunwifi");
        let mut seen = std::collections::HashSet::new();
        for entry in &all {
            assert!(seen.insert(entry.id.as_str()), "'{}' is declared twice", entry.id);
        }
    }

    #[test]
    fn no_entry_anywhere_carries_a_url() {
        // §41.5.7, as a test rather than as a rule somebody remembers. The
        // enum has no URL variant, so this can only fail if a path or query
        // smuggles one in.
        for entry in super::entries().unwrap() {
            let text = format!("{:?}", entry.source);
            assert!(
                !text.contains("http://") && !text.contains("https://"),
                "'{}' carries a URL: {text}",
                entry.id
            );
        }
    }

    #[test]
    fn every_requirement_names_an_entry_that_exists() {
        let all = super::entries().unwrap();
        let ids: Vec<&str> = all.iter().map(|e| e.id.as_str()).collect();
        for entry in &all {
            for need in &entry.requires {
                assert!(ids.contains(&need.as_str()), "'{}' requires unknown '{need}'", entry.id);
            }
        }
    }

    #[test]
    fn every_permission_entry_is_named_in_the_licence_file() {
        // The owner's own requirement: "lisansa da ekleriz". Bound here so
        // forgetting it is a red suite rather than a quiet omission.
        let licences = include_str!("../../../../../THIRD_PARTY_LICENSES.md");
        let flagged: Vec<String> = super::entries()
            .unwrap()
            .into_iter()
            .filter(|e| e.permission.is_some())
            .map(|e| e.name)
            .collect();
        assert_eq!(flagged.len(), 4, "Picasso96, iBrowse, SetPatch, Workbench-Library");
        for name in flagged {
            assert!(licences.contains(&name), "'{name}' is not in THIRD_PARTY_LICENSES.md");
        }
    }

    #[test]
    fn the_two_tcp_stacks_are_alternatives_and_say_so() {
        let all = super::entries().unwrap();
        let group = |id: &str| {
            all.iter()
                .find(|e| e.id == id)
                .unwrap_or_else(|| panic!("'{id}' is shipped"))
                .exclusive_group
                .clone()
        };
        assert_eq!(group("tolunnet"), Some("tcp".to_string()));
        assert_eq!(group("miamidx"), Some("tcp".to_string()));
    }

    #[test]
    fn whdloadwrapper_is_declared_as_something_art_cannot_fetch() {
        // Its printed source is an FTP search form with query parameters, not
        // a path. Declaring it `user-supplied` is how ART says so *before*
        // the user asks it to fetch, rather than failing at the attempt.
        let entry = super::entry_by_id("whdloadwrapper").unwrap().unwrap();
        assert!(matches!(entry.source, super::EntrySource::UserSupplied { .. }));
    }
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cd src-tauri && cargo test sources::bundle
```
Expected: the set-list, count, licence, exclusive-group and user-supplied tests all fail; Task 1's three still pass.

- [ ] **Step 3: Write the thirteen remaining JSON files**

One per set, following `arsiv.json`'s shape. The four `permission` entries carry, for example:

```json
{ "id": "picasso96", "name": "Picasso96", "order": 10,
  "source": { "aminet": { "path": "driver/video/Picasso96" } },
  "permission": { "holder": "Individual Computers (Jens Schonfeld)",
                  "note": "shareware; the only legal purchase is from Individual Computers" } }
```

`whdloadwrapper` carries no path at all:

```json
{ "id": "whdloadwrapper", "name": "WHDLoadWrapper", "order": 20,
  "source": { "user-supplied": { "why": "its published address is an FTP search form, not a file path" } },
  "permission": { "holder": "Turran FTP", "note": "fetched by hand from ftp2.grandis.nu" } }
```

The owner's own two, in `ag.json`, need no permission:

```json
{ "id": "tolunnet", "name": "tolunnet", "order": 10, "exclusive_group": "tcp",
  "source": { "user-supplied": { "why": "the owner's own TCP/IP stack, built from D:\\Projeler\\tolunnet" } } }
```

- [ ] **Step 4: Extend `SHIPPED`**

```rust
const EMU68_JSON: &str = include_str!("catalogue/emu68.json");
// ...one per file...
const SHIPPED: &[&str] = &[
    EMU68_JSON, ARSIV_JSON, DOSYA_SISTEMI_JSON, TEMEL_JSON, AG_JSON, GRAFIK_JSON,
    MASAUSTU_JSON, TESHIS_JSON, ACILIS_JSON, KABUK_JSON, WHDLOAD_JSON, MEDYA_JSON,
    AMIGAOS_EKI_JSON, IBROWSE_JSON,
];
```

- [ ] **Step 5: Add the four to `THIRD_PARTY_LICENSES.md`**

A new section, one row each, naming the holder and the condition — the exact `name` string the entry carries, because the test matches on it.

- [ ] **Step 6: Run the tests**

```bash
cd src-tauri && cargo test sources::bundle
```
Expected: all pass.

- [ ] **Step 7: Mutate, and watch it fall**

Delete one of the four names from `THIRD_PARTY_LICENSES.md` and re-run: `every_permission_entry_is_named_in_the_licence_file` must fail. Put it back.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/core/sources/bundle THIRD_PARTY_LICENSES.md
git commit -m "feat(bundle): 14 sets, 62 entries, and the licence file bound to the permission field"
```

---

### Task 3: Ask the real sources whether the catalogue is true

**Files:**
- Create: `scripts/catalogue-check.py`
- Modify: whichever `catalogue/*.json` the script proves wrong

**Interfaces:**
- Consumes: the shipped JSON from Task 2. Reads it as data; imports nothing from Rust.
- Produces: no API. A script, and corrected data.

**This task exists because entry 53 exists.** The Imager's own page prints ViNCEd's path with a missing slash, and that was caught by asking Aminet rather than by reading harder. ART's own rule: a card is verified by something that is not ART, and so is a catalogue.

- [ ] **Step 1: Write the script**

```python
#!/usr/bin/env python3
"""Ask the live sources whether every shipped catalogue path resolves.

Not in CI — it leaves the machine. The same place and for the same reason as
scripts/rom-table-check.py and scripts/fat-oracle-check.py.

Exit 0 when every fetchable entry resolved; 1 otherwise, listing what did not.
"""
import json
import pathlib
import sys
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
CATALOGUE = ROOT / "src-tauri/src/core/sources/bundle/catalogue"
AMINET = "https://aminet.net/package/"
TIMEOUT = 20


def head_ok(url: str) -> tuple[bool, str]:
    request = urllib.request.Request(url, method="HEAD")
    try:
        with urllib.request.urlopen(request, timeout=TIMEOUT) as answer:
            return (200 <= answer.status < 300, str(answer.status))
    except urllib.error.HTTPError as err:
        return (False, f"HTTP {err.code}")
    except Exception as err:  # noqa: BLE001 - a network failure is a result
        return (False, type(err).__name__)


def main() -> int:
    checked = skipped = 0
    bad: list[str] = []
    for path in sorted(CATALOGUE.glob("*.json")):
        bundle = json.loads(path.read_text(encoding="utf-8"))
        for entry in bundle["entries"]:
            kind, body = next(iter(entry["source"].items()))
            if kind in ("user-supplied", "aminet-search"):
                skipped += 1
                print(f"  skip  {entry['id']:<20} ({kind})")
                continue
            if kind == "aminet":
                url = AMINET + body["path"]
            else:
                skipped += 1
                print(f"  skip  {entry['id']:<20} ({kind} — needs a configured mirror)")
                continue
            ok, why = head_ok(url)
            checked += 1
            print(f"  {'ok  ' if ok else 'FAIL'}  {entry['id']:<20} {url} [{why}]")
            if not ok:
                bad.append(f"{entry['id']}: {url} ({why})")

    print(f"\nchecked {checked}, skipped {skipped}, failed {len(bad)}")
    for line in bad:
        print(f"  {line}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 2: Run it**

```bash
python scripts/catalogue-check.py
```
Expected: every `aminet` entry reports `ok`. Any `FAIL` is a wrong path in the shipped data — **correct the JSON, not the script**.

- [ ] **Step 3: Re-run until it is clean, then re-run the Rust tests**

```bash
python scripts/catalogue-check.py && cd src-tauri && cargo test sources::bundle
```

- [ ] **Step 4: Add it to the command list**

In `CLAUDE.md`'s Commands block and in `docs/STATUS.md`'s "Reproduce the numbers above", beside `rom-table-check.py`, with the one-line note that it is not in CI.

- [ ] **Step 5: Commit**

```bash
git add scripts/catalogue-check.py CLAUDE.md docs/STATUS.md src-tauri/src/core/sources/bundle/catalogue
git commit -m "test(bundle): ask Aminet whether the shipped catalogue is true"
```

---

### Task 4: Resolving an entry into something the fetcher already understands

**Files:**
- Create: `src-tauri/src/core/sources/bundle/resolve.rs`
- Modify: `src-tauri/src/core/sources/bundle/mod.rs` (`pub mod resolve;`)

**Interfaces:**
- Consumes: Task 1's `BundleEntry`/`EntrySource`; `crate::core::sources::{PackageMeta, PackageRef, PROVIDER_AMINET}`; `crate::core::sources::mirror::Mirror`.
- Produces:
  - `pub enum Resolution { Fetchable { meta: PackageMeta, mirrors: Vec<Mirror> }, Refused { why: String } }`
  - `pub fn resolve(entry: &BundleEntry, aminet: &[Mirror], configured: &[(String, Mirror)]) -> Resolution`

`PackageMeta::size_bytes` is `0` for everything but a catalog-backed Aminet entry, and that is correct rather than lazy: `fetch.rs::check_size` documents `size_bytes == 0` as "the catalog claimed nothing usable; there is nothing to compare to". The empty-file gate still applies.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::sources::bundle::{BundleEntry, EntrySource};

    fn entry(id: &str, source: EntrySource) -> BundleEntry {
        BundleEntry {
            id: id.into(),
            name: id.into(),
            source,
            order: 1,
            exclusive_group: None,
            requires: Vec::new(),
            permission: None,
        }
    }

    fn aminet() -> Vec<Mirror> {
        vec![Mirror::new("Test", "https://aminet.invalid/").unwrap()]
    }

    #[test]
    fn an_aminet_entry_resolves_to_its_repository_path() {
        let e = entry("pfs3", EntrySource::Aminet { path: "disk/misc/PFS3_53".into() });
        match resolve(&e, &aminet(), &[]) {
            Resolution::Fetchable { meta, mirrors } => {
                assert_eq!(meta.reference.provider, crate::core::sources::PROVIDER_AMINET);
                assert_eq!(meta.reference.path, "disk/misc/PFS3_53");
                assert_eq!(mirrors.len(), 1);
            }
            Resolution::Refused { why } => panic!("refused: {why}"),
        }
    }

    #[test]
    fn a_user_supplied_entry_is_refused_with_the_reason_it_declares() {
        let e = entry(
            "whdloadwrapper",
            EntrySource::UserSupplied { why: "its address is a search form".into() },
        );
        match resolve(&e, &aminet(), &[]) {
            Resolution::Refused { why } => assert!(why.contains("search form"), "got: {why}"),
            Resolution::Fetchable { .. } => panic!("ART cannot fetch this"),
        }
    }

    #[test]
    fn a_mirror_entry_naming_no_configured_mirror_is_refused_by_name() {
        let e = entry("setpatch", EntrySource::Mirror {
            mirror: "cloanto-cdn".into(),
            path: "pub/amiga/SetPatch-44-38.lha".into(),
        });
        match resolve(&e, &aminet(), &[]) {
            Resolution::Refused { why } => assert!(why.contains("cloanto-cdn"), "got: {why}"),
            Resolution::Fetchable { .. } => panic!("nothing is configured for it"),
        }
    }

    #[test]
    fn a_mirror_entry_resolves_against_the_mirror_configured_for_it() {
        let cloanto = Mirror::new("Cloanto", "https://cdn.invalid/").unwrap();
        let e = entry("setpatch", EntrySource::Mirror {
            mirror: "cloanto-cdn".into(),
            path: "pub/amiga/SetPatch-44-38.lha".into(),
        });
        match resolve(&e, &aminet(), &[("cloanto-cdn".to_string(), cloanto)]) {
            Resolution::Fetchable { meta, mirrors } => {
                assert_eq!(meta.reference.path, "pub/amiga/SetPatch-44-38.lha");
                assert_eq!(mirrors.len(), 1);
            }
            Resolution::Refused { why } => panic!("refused: {why}"),
        }
    }
}
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cd src-tauri && cargo test sources::bundle::resolve
```
Expected: compile error — `resolve` and `Resolution` do not exist.

- [ ] **Step 3: Write `resolve.rs`**

```rust
//! An entry, turned into the `(PackageMeta, mirrors)` pair
//! `fetch::fetch_package` already takes.
//!
//! Nothing here opens a socket, and nothing here accepts a URL: a
//! `Mirror` arrives already configured and validated by `Mirror::new`.

use super::{BundleEntry, EntrySource};
use crate::core::sources::mirror::Mirror;
use crate::core::sources::{PackageMeta, PackageRef, PROVIDER_AMINET};

pub enum Resolution {
    Fetchable { meta: PackageMeta, mirrors: Vec<Mirror> },
    /// ART will not fetch this, and the sentence says why. English, from
    /// `core/` (ART-060) — the screen translates the *kind*, and shows this
    /// after it.
    Refused { why: String },
}

fn meta_for(entry: &BundleEntry, provider: &str, path: &str) -> PackageMeta {
    PackageMeta {
        reference: PackageRef { provider: provider.into(), path: path.into() },
        name: path.rsplit('/').next().unwrap_or(path).to_string(),
        directory: path.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default(),
        // Nothing claimed a size. `check_size` treats 0 as "no claim to
        // compare against" and still refuses an empty file.
        size_bytes: 0,
        age_weeks: None,
        short: entry.name.clone(),
        version: None,
        requires: Vec::new(),
        author: None,
        distribution: None,
    }
}

pub fn resolve(
    entry: &BundleEntry,
    aminet: &[Mirror],
    configured: &[(String, Mirror)],
) -> Resolution {
    match &entry.source {
        EntrySource::Aminet { path } => Resolution::Fetchable {
            meta: meta_for(entry, PROVIDER_AMINET, path),
            mirrors: aminet.to_vec(),
        },
        EntrySource::AminetSearch { query } => Resolution::Refused {
            why: format!(
                "'{}' is a search ('{query}'), which needs a synced catalogue; sync Aminet first",
                entry.id
            ),
        },
        EntrySource::GithubRelease { repo, asset } => Resolution::Refused {
            why: format!("'{repo}' release asset '{asset}' needs a configured GitHub mirror"),
        },
        EntrySource::Mirror { mirror, path } => {
            match configured.iter().find(|(name, _)| name == mirror) {
                Some((_, m)) => Resolution::Fetchable {
                    meta: meta_for(entry, mirror, path),
                    mirrors: vec![m.clone()],
                },
                None => Resolution::Refused {
                    why: format!("no mirror named '{mirror}' is configured"),
                },
            }
        }
        EntrySource::UserSupplied { why } => Resolution::Refused { why: why.clone() },
    }
}
```

`PackageMeta` may carry fields beyond those listed; fill any remainder with its `Default` where one exists, and read `core/sources/mod.rs` for the current shape rather than assuming this list is complete.

- [ ] **Step 4: Run the tests**

```bash
cd src-tauri && cargo test sources::bundle::resolve
```
Expected: 4 passed.

- [ ] **Step 5: Mutate, and watch it fall**

Make the `Mirror` arm fall back to `aminet` when no configured mirror matches, and re-run: `a_mirror_entry_naming_no_configured_mirror_is_refused_by_name` must fail. Put it back.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/core/sources/bundle
git commit -m "feat(bundle): resolve an entry into what the fetcher already takes"
```

---

### Task 5: The sequential download, and five endings that stay apart

**Files:**
- Create: `src-tauri/src/core/sources/bundle/run.rs`
- Modify: `src-tauri/src/core/sources/bundle/mod.rs` (`pub mod run;`)

**Interfaces:**
- Consumes: Task 4's `resolve`/`Resolution`; `core::sources::fetch::fetch_package`; `core::sources::cache::CacheLayout`; `core::sources::library::Library`; `core::jobs::ProgressSink`.
- Produces:
  - `pub enum EntryOutcome { Downloaded { bytes: u64, path: PathBuf }, AlreadyHave { path: PathBuf }, Refused { why: String }, Failed { error: String }, Skipped }`
  - `pub struct EntryReport { pub id: String, pub name: String, pub outcome: EntryOutcome }`
  - `pub struct BundleReport { pub entries: Vec<EntryReport> }`
  - `pub fn download_entries(entries: &[BundleEntry], ctx: &DownloadContext, sink: &dyn ProgressSink) -> BundleReport`
  - `pub struct DownloadContext<'a> { pub aminet: &'a [Mirror], pub configured: &'a [(String, Mirror)], pub client: &'a dyn MirrorClient, pub cache: &'a CacheLayout, pub library: &'a Library, pub subfolder: &'a str }`

**The five outcomes are the point of this task.** Collapsing them into "did not succeed" is the defect class this project's own rules name as its most expensive: a user told "failed" about an entry ART never attempted has been told something false.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::core::jobs::NoProgress;
    use crate::core::sources::bundle::{BundleEntry, EntrySource};
    use crate::core::sources::mirror::tests::MockMirror;
    use crate::core::ScratchDir;

    const BASE: &str = "https://mirror.invalid/";

    fn url(path: &str) -> String {
        format!("{BASE}{path}")
    }

    fn entry(id: &str, source: EntrySource) -> BundleEntry {
        BundleEntry {
            id: id.into(),
            name: id.into(),
            source,
            order: 1,
            exclusive_group: None,
            requires: Vec::new(),
            permission: None,
        }
    }

    fn aminet_entry(id: &str, path: &str) -> BundleEntry {
        entry(id, EntrySource::Aminet { path: path.into() })
    }

    /// Cancelled from the moment `name` is reported — which
    /// `download_entries` does at the top of the loop, **before** the
    /// cancellation check. So the entry that triggers it is the first one
    /// skipped, deterministically, with no sleeping and no byte counting.
    struct CancelOn {
        name: &'static str,
        hit: AtomicBool,
    }

    impl ProgressSink for CancelOn {
        fn report(&self, _done: u64, _total: Option<u64>, message: &str) {
            if message == self.name {
                self.hit.store(true, Ordering::SeqCst);
            }
        }
        fn is_cancelled(&self) -> bool {
            self.hit.load(Ordering::SeqCst)
        }
    }

    #[test]
    fn one_entry_failing_does_not_take_the_set_with_it() {
        let scratch = ScratchDir::new("bundle-run-mixed");
        let client = MockMirror::new()
            .with_file(&url("util/arc/lha_68k"), b"lha bytes")
            .failing(&url("util/arc/lzx121r1"), "mirror said no");
        let mirrors = vec![Mirror::new("Test", BASE).unwrap()];
        let cache = CacheLayout::new(scratch.join("cache"));
        let library = Library::new(scratch.join("library"));
        let ctx = DownloadContext {
            aminet: &mirrors,
            configured: &[],
            client: &client,
            cache: &cache,
            library: &library,
            subfolder: "sets",
        };

        let entries = vec![
            aminet_entry("lha", "util/arc/lha_68k"),
            entry("tolunnet", EntrySource::UserSupplied { why: "the owner's own".into() }),
            aminet_entry("lzx", "util/arc/lzx121r1"),
        ];

        let report = download_entries(&entries, &ctx, &NoProgress);
        assert!(matches!(report.entries[0].outcome, EntryOutcome::Downloaded { .. }));
        assert!(matches!(report.entries[1].outcome, EntryOutcome::Refused { .. }));
        assert!(matches!(report.entries[2].outcome, EntryOutcome::Failed { .. }));
    }

    #[test]
    fn entries_are_attempted_in_the_order_they_are_given() {
        // `MockMirror` already records every request as (url, from) in its
        // own `requests` mutex — no new mock API is needed.
        let scratch = ScratchDir::new("bundle-run-order");
        let client = MockMirror::new()
            .with_file(&url("util/arc/lha_68k"), b"lha bytes")
            .with_file(&url("util/arc/lzx121r1"), b"lzx bytes");
        let mirrors = vec![Mirror::new("Test", BASE).unwrap()];
        let cache = CacheLayout::new(scratch.join("cache"));
        let library = Library::new(scratch.join("library"));
        let ctx = DownloadContext {
            aminet: &mirrors,
            configured: &[],
            client: &client,
            cache: &cache,
            library: &library,
            subfolder: "sets",
        };

        let entries = vec![
            aminet_entry("lha", "util/arc/lha_68k"),
            aminet_entry("lzx", "util/arc/lzx121r1"),
        ];
        let report = download_entries(&entries, &ctx, &NoProgress);
        assert_eq!(report.entries.len(), 2);

        let asked: Vec<String> = client
            .requests
            .lock()
            .unwrap()
            .iter()
            .map(|(u, _)| u.clone())
            .collect();
        assert_eq!(asked, vec![url("util/arc/lha_68k"), url("util/arc/lzx121r1")]);
    }

    #[test]
    fn a_second_run_over_the_same_set_reports_already_have_rather_than_downloading_again() {
        let scratch = ScratchDir::new("bundle-run-twice");
        let client = MockMirror::new().with_file(&url("util/arc/lha_68k"), b"lha bytes");
        let mirrors = vec![Mirror::new("Test", BASE).unwrap()];
        let cache = CacheLayout::new(scratch.join("cache"));
        let library = Library::new(scratch.join("library"));
        let ctx = DownloadContext {
            aminet: &mirrors,
            configured: &[],
            client: &client,
            cache: &cache,
            library: &library,
            subfolder: "sets",
        };
        let entries = vec![aminet_entry("lha", "util/arc/lha_68k")];

        let first = download_entries(&entries, &ctx, &NoProgress);
        assert!(matches!(first.entries[0].outcome, EntryOutcome::Downloaded { .. }));

        let second = download_entries(&entries, &ctx, &NoProgress);
        assert!(matches!(second.entries[0].outcome, EntryOutcome::AlreadyHave { .. }));
    }

    #[test]
    fn cancelling_stops_between_entries_and_marks_the_rest_skipped() {
        let scratch = ScratchDir::new("bundle-run-cancel");
        let client = MockMirror::new()
            .with_file(&url("util/arc/lha_68k"), b"lha bytes")
            .with_file(&url("util/arc/lzx121r1"), b"lzx bytes");
        let mirrors = vec![Mirror::new("Test", BASE).unwrap()];
        let cache = CacheLayout::new(scratch.join("cache"));
        let library = Library::new(scratch.join("library"));
        let ctx = DownloadContext {
            aminet: &mirrors,
            configured: &[],
            client: &client,
            cache: &cache,
            library: &library,
            subfolder: "sets",
        };
        let entries = vec![
            aminet_entry("lha", "util/arc/lha_68k"),
            aminet_entry("lzx", "util/arc/lzx121r1"),
            aminet_entry("zip", "util/arc/ZIP232"),
        ];

        let sink = CancelOn { name: "lzx", hit: AtomicBool::new(false) };
        let report = download_entries(&entries, &ctx, &sink);

        assert!(matches!(report.entries[0].outcome, EntryOutcome::Downloaded { .. }));
        // Skipped, **not** Failed: nothing was attempted for either.
        assert!(matches!(report.entries[1].outcome, EntryOutcome::Skipped));
        assert!(matches!(report.entries[2].outcome, EntryOutcome::Skipped));
    }
}
```

Two things this fixture leans on that already exist, so nothing new is needed
in the mock: `MockMirror::failing(url, reason)`, and its `requests` mutex,
which records every `(url, from)` pair in order. `ScratchDir` removes itself on
`Drop`, which is the standing rule after ART-184.

**`download_entries` must report the entry's name before checking
cancellation** for `CancelOn` to be deterministic — and that ordering is right
anyway: the screen should be able to say which entry is being considered.

- [ ] **Step 2: Run it and watch it fail**

```bash
cd src-tauri && cargo test sources::bundle::run
```
Expected: compile error — `download_entries` does not exist.

- [ ] **Step 3: Write `run.rs`**

```rust
//! One set, downloaded in order.
//!
//! Sequential rather than parallel, and both halves of that are deliberate:
//! Aminet's mirrors are volunteer-run, and a readable report needs a
//! determinate order.

use std::path::PathBuf;

use super::resolve::{resolve, Resolution};
use super::BundleEntry;
use crate::core::jobs::ProgressSink;
use crate::core::sources::cache::CacheLayout;
use crate::core::sources::fetch::fetch_package;
use crate::core::sources::library::Library;
use crate::core::sources::mirror::{Mirror, MirrorClient};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum EntryOutcome {
    Downloaded { bytes: u64, path: PathBuf },
    AlreadyHave { path: PathBuf },
    /// ART will not fetch it, and said so before the run — a `user-supplied`
    /// entry, or a source with no configured mirror.
    Refused { why: String },
    /// It was tried and the mirror or the network said no.
    Failed { error: String },
    /// The user cancelled before reaching it. **Not** a failure: nothing was
    /// attempted.
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct EntryReport {
    pub id: String,
    pub name: String,
    pub outcome: EntryOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BundleReport {
    pub entries: Vec<EntryReport>,
}

pub struct DownloadContext<'a> {
    pub aminet: &'a [Mirror],
    pub configured: &'a [(String, Mirror)],
    pub client: &'a dyn MirrorClient,
    pub cache: &'a CacheLayout,
    pub library: &'a Library,
    pub subfolder: &'a str,
}

pub fn download_entries(
    entries: &[BundleEntry],
    ctx: &DownloadContext<'_>,
    sink: &dyn ProgressSink,
) -> BundleReport {
    let mut reports = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        // Said before the check, so the screen can name what is being
        // considered — and so a test can make cancellation deterministic by
        // watching for a name rather than by counting bytes or sleeping.
        sink.report(index as u64, Some(entries.len() as u64), &entry.name);

        // Between whole entries, never mid-write. That is what makes
        // cancelling safe: unfinished work, never a half-written file.
        if sink.is_cancelled() {
            reports.push(EntryReport {
                id: entry.id.clone(),
                name: entry.name.clone(),
                outcome: EntryOutcome::Skipped,
            });
            continue;
        }

        let outcome = match resolve(entry, ctx.aminet, ctx.configured) {
            Resolution::Refused { why } => EntryOutcome::Refused { why },
            Resolution::Fetchable { meta, mirrors } => {
                match fetch_package(&meta, &mirrors, ctx.client, ctx.cache, sink) {
                    Ok(fetched) => {
                        match ctx.library.place(&fetched.path, ctx.subfolder, &meta.name) {
                            Ok(placement) if fetched.from_cache => {
                                EntryOutcome::AlreadyHave { path: placement.path }
                            }
                            Ok(placement) => EntryOutcome::Downloaded {
                                bytes: fetched.bytes,
                                path: placement.path,
                            },
                            Err(e) => EntryOutcome::Failed { error: e.to_string() },
                        }
                    }
                    Err(e) => EntryOutcome::Failed { error: e.to_string() },
                }
            }
        };

        reports.push(EntryReport {
            id: entry.id.clone(),
            name: entry.name.clone(),
            outcome,
        });
    }
    BundleReport { entries: reports }
}
```

`Library::place`'s exact signature and its `Placement` shape are in `core/sources/library.rs` — read them and adjust the call rather than assuming this one is right.

- [ ] **Step 4: Run the tests**

```bash
cd src-tauri && cargo test sources::bundle::run
```
Expected: 4 passed.

- [ ] **Step 5: Mutate, and watch each fall**

1. Return `EntryOutcome::Failed` for the `Refused` arm — `one_entry_failing_does_not_take_the_set_with_it` must fail.
2. Make the cancelled arm `break` instead of pushing `Skipped` — `cancelling_stops_between_entries_and_marks_the_rest_skipped` must fail.
3. Ignore `from_cache` and always report `Downloaded` — the second-run test must fail.

Put each back before the next.

- [ ] **Step 6: Run the whole suite twice**

```bash
cd src-tauri && cargo test && cargo test
```
Per the standing rule (ART-059).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/core/sources/bundle
git commit -m "feat(bundle): download a set in order, with five endings that stay apart"
```

---

### Task 6: The commands, and the typed wrappers

**Files:**
- Create: `src-tauri/src/commands/bundles.rs`
- Modify: `src-tauri/src/lib.rs` (`mod` + `invoke_handler![]`)
- Create: `src/lib/bundles.ts`

**Interfaces:**
- Consumes: Task 1's `bundles()`, Task 5's `download_entries`, `BundleReport`; the existing `SourcesState` (`commands/sources.rs`) for the configured mirrors, cache root and library root; `commands/jobs.rs::spawn_job`.
- Produces:
  - `#[tauri::command] pub fn bundles_list() -> AppResult<Vec<BundleSummary>>`
  - `#[tauri::command] pub fn bundles_download(app: AppHandle, state: State<'_, SourcesState>, oplog: State<'_, JsonlOperationLog>, entry_ids: Vec<String>) -> AppResult<u64>` — returns a job id; the report arrives on the `bundle-download-result` event.
  - `fn entry_ids_or_refuse(ids: &[String]) -> CoreResult<Vec<BundleEntry>>` — resolves every chosen id against `bundle::entries()`, refusing an empty selection (`"no packages were chosen"`) and an id ART does not ship, by name. Private to the module; the test below calls it directly.
  - TS: `bundlesList(): Promise<BundleSummary[]>`, `bundlesDownload(entryIds: string[]): Promise<number>`, `onBundleDownloadResult(cb)`.
  - `BundleSummary { id, order, entries: EntrySummary[] }`, `EntrySummary { id, name, kind, permission }` where `kind` is the `EntrySource` tag.

- [ ] **Step 1: Write the failing test**

In `commands/bundles.rs`'s test module:

```rust
    #[test]
    fn every_shipped_set_is_listed_with_its_entries_and_their_kinds() {
        let sets = bundles_list().unwrap();
        assert_eq!(sets.len(), 14);
        let arsiv = sets.iter().find(|s| s.id == "arsiv").unwrap();
        assert_eq!(arsiv.entries.len(), 6);
        assert!(arsiv.entries.iter().all(|e| e.kind == "aminet"));
    }

    #[test]
    fn a_permission_entry_is_listed_with_its_condition_so_the_screen_can_say_it_first() {
        let sets = bundles_list().unwrap();
        let picasso = sets
            .iter()
            .flat_map(|s| &s.entries)
            .find(|e| e.id == "picasso96")
            .expect("Picasso96 is shipped");
        assert!(picasso.permission.is_some());
    }

    #[test]
    fn downloading_nothing_is_refused_rather_than_run_as_an_empty_job() {
        // An empty selection is a mistake worth naming, not a job that does
        // nothing and reports success.
        let err = entry_ids_or_refuse(&[]).expect_err("an empty selection is refused");
        assert!(format!("{err}").contains("no packages"), "got: {err}");
    }
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cd src-tauri && cargo test commands::bundles
```

- [ ] **Step 3: Write the command module**

Follow `commands/sources.rs` for how `SourcesState` is read and `commands/osinstall.rs` for the `spawn_job` + event shape. Every changed-data operation records through `write_result` in `commands/oplog.rs` — a download writes files, so it logs.

- [ ] **Step 4: Register the commands**

Add `commands::bundles::bundles_list` and `bundles_download` to `invoke_handler![]` in `src-tauri/src/lib.rs`. **Both**, or the frontend gets a runtime "command not found" nothing in the build catches.

- [ ] **Step 5: Write `src/lib/bundles.ts`**

```ts
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type EntryKind =
  | "aminet"
  | "aminet-search"
  | "github-release"
  | "mirror"
  | "user-supplied";

export interface EntrySummary {
  id: string;
  name: string;
  kind: EntryKind;
  permission: { holder: string; note: string } | null;
}

export interface BundleSummary {
  id: string;
  order: number;
  entries: EntrySummary[];
}

export async function bundlesList(): Promise<BundleSummary[]> {
  return invoke<BundleSummary[]>("bundles_list");
}

export async function bundlesDownload(entryIds: string[]): Promise<number> {
  return invoke<number>("bundles_download", { entryIds });
}
```

- [ ] **Step 6: Run everything**

```bash
cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings
cd .. && pnpm lint
```

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/commands/bundles.rs src-tauri/src/lib.rs src/lib/bundles.ts
git commit -m "feat(bundle): list the sets and run a download, through the job runner"
```

---

### Task 7: The screen

**Files:**
- Create: `src/components/sources/BundlePanel.tsx`
- Create: `src/components/sources/BundlePanel.test.tsx`
- Modify: `src/pages/AminetStudio.tsx`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json`

**Interfaces:**
- Consumes: Task 6's `bundlesList`, `bundlesDownload`, `onBundleDownloadResult`, `BundleSummary`.
- Produces: no API. A panel.

**i18n keys, both files, same commit:**

| Key | English | Turkish |
|---|---|---|
| `bundles.heading` | Package sets | Paket setleri |
| `bundles.intro` | Pick a set and ART downloads its packages, in order, into your download folder. Nothing is fetched until you press the button. | Bir set seç, ART paketlerini sırayla indirme klasörüne indirsin. Düğmeye basmadan hiçbir şey inmez. |
| `bundles.set.<id>` | (one per set) | (one per set) |
| `bundles.permission.warning` | Some of these are used with the author's permission and carry conditions: {{names}}. The conditions travel with the files. | Bunlardan bazıları yazarın izniyle kullanılıyor ve koşul taşıyor: {{names}}. Koşullar dosyalarla birlikte gider. |
| `bundles.entry.userSupplied` | ART cannot fetch this one — you supply it. {{why}} | Bunu ART getiremez, sen vereceksin. {{why}} |
| `bundles.run` | Download the chosen sets | Seçilen setleri indir |
| `bundles.running` | Downloading… | İndiriliyor… |
| `bundles.blocked.nothingChosen` | Choose at least one set first. | Önce en az bir set seç. |
| `bundles.result.downloaded` | {{count}} downloaded | {{count}} indirildi |
| `bundles.result.alreadyHave` | {{count}} were already here | {{count}} tanesi zaten vardı |
| `bundles.result.refused` | {{count}} ART cannot fetch | {{count}} tanesini ART getiremez |
| `bundles.result.failed` | {{count}} failed | {{count}} tanesi başarısız |
| `bundles.result.skipped` | {{count}} skipped when you cancelled | İptal edince {{count}} tanesi atlandı |

- [ ] **Step 1: Write the failing test**

`BundlePanel.test.tsx`, `// @vitest-environment jsdom`, mocking `@/lib/bundles` the way `OsInstall.test.tsx` mocks `@/lib/osinstall`:

```tsx
it("says the permission condition before the tick, not after", async () => {
  listMock.mockResolvedValue([PICASSO_SET]);
  render(<BundlePanel />);
  const warning = await screen.findByTestId("bundle-permission-warning");
  expect(warning.textContent).toContain("Picasso96");
  // The tick is still there; the sentence is above it, not instead of it.
  expect(screen.getByRole("checkbox", { name: /grafik/i })).toBeTruthy();
});

it("fetches nothing until the button is pressed", async () => {
  listMock.mockResolvedValue([ARSIV_SET]);
  render(<BundlePanel />);
  await screen.findByText(i18n.t("bundles.heading"));
  expect(downloadMock).not.toHaveBeenCalled();
});

it("reports five endings separately, never as two", async () => {
  // Deliver a report with one of each and assert five distinct sentences.
  render(<BundlePanel />);
  act(() => announce.current!(MIXED_REPORT));
  expect(await screen.findByText(i18n.t("bundles.result.downloaded", { count: 1 }))).toBeTruthy();
  expect(screen.getByText(i18n.t("bundles.result.refused", { count: 1 }))).toBeTruthy();
  expect(screen.getByText(i18n.t("bundles.result.failed", { count: 1 }))).toBeTruthy();
  expect(screen.getByText(i18n.t("bundles.result.skipped", { count: 1 }))).toBeTruthy();
  expect(screen.getByText(i18n.t("bundles.result.alreadyHave", { count: 1 }))).toBeTruthy();
});
```

- [ ] **Step 2: Run it and watch it fail**

```bash
pnpm vitest run src/components/sources/BundlePanel.test.tsx
```

- [ ] **Step 3: Write the panel and mount it**

Sets as cards with their entry count; a tick per set; the permission warning rendered **above** the ticks for any set holding a flagged entry, naming them; a `user-supplied` entry rendered with its own sentence rather than a tick; per-entry progress from `onJobProgress`; the five-ending report.

**An `exclusive_group` is shown, never enforced** — the spec's own correction. `tolunnet` and `miamidx` are alternatives *to install*, and downloading both is a legitimate thing to want. Render the pair with a line saying they are alternatives; do not disable either tick.

Mount it in `src/pages/AminetStudio.tsx` as a new section.

- [ ] **Step 4: Add both catalogues**

Both `en.json` and `tr.json`, same commit, or `pnpm test` fails on parity.

- [ ] **Step 5: Run everything**

```bash
pnpm lint && pnpm test
```

- [ ] **Step 6: Mutate, and watch it fall**

Render the permission warning *below* the ticks and re-run: the first test must fail. Collapse `refused` and `failed` into one sentence: the third must fail. Put both back.

- [ ] **Step 7: Commit**

```bash
git add src/components/sources src/pages/AminetStudio.tsx src/i18n
git commit -m "feat(bundle): the set list, the condition said first, and five endings"
```

---

## When the work lands

1. `docs/STATUS.md` — session log line, and the snapshot if the numbers moved.
2. `docs/ISSUES.md` — a new `ART-NNN` for anything found on the way.
3. `docs/FEATURES.md` — flip the Aminet rows only where a test exists.
4. `CHANGELOG.md` — the user-visible sentence.
5. `docs/superpowers/specs/2026-08-22-work-list.md` — item 10 is done; say what of it is not.
