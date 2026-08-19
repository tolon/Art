# Content Layer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Put official AmigaOS updates and a language pack onto a built distribution tree, from the user's own archives, without ever overwriting a file silently.

**Architecture:** A package is a `MediaSource` like a floppy or a disc — one more implementation, not a second engine. Two of them: an archive read directly, and an archive read out of another archive. The placer gains a preview that classifies every collision before one happens, and two entry points (build a tree with packages, or add a package to a tree) that must produce byte-identical results.

**Tech Stack:** Rust (`core/osinstall`, `core/archive`), React + i18next, Tauri commands.

**Spec:** [docs/superpowers/specs/2026-08-19-content-layer-design.md](../specs/2026-08-19-content-layer-design.md)

## Global Constraints

- `src-tauri/src/core/` is platform-independent: no `use tauri`, no Windows APIs, no network. A lower-level `core/` module must not import a higher-level one.
- **Never read a whole image into memory.** An archive member's declared length is a claim; `core/archive`'s gate budgets from it and checks what actually arrives.
- `core::security::safe_join` is the only way from an untrusted entry name to a destination path. Bound reads; `checked_add`/`checked_mul` on running totals; a step limit on any chain walk.
- Every write goes through `core/safety`; every data-changing command is logged through `commands/oplog.rs`.
- **No two components may claim the same destination** without one declaring an `overrides` relationship. A test enforces it over every shipped recipe.
- Recipes are **data**. A new package adds a JSON file, not a code path.
- Spec §89: never claim support that is not implemented and tested. This round supports exactly the packages it ships recipes for; a package without one is refused **with its reason**.
- Unused imports and variables are compile errors in the Rust crate. Doc comments explain *why*, not what.
- Every user-visible string goes in **both** `src/i18n/en.json` and `src/i18n/tr.json` in the same commit.
- `src/lib/*.ts` is the only place that calls `invoke`. Frontend tests sit next to their source; `.tsx` gets jsdom, `.ts` does not.
- No setting may change without the user changing it. `useRemembered(key, guard, fallback)` is the only way to persist one.
- Fixtures are synthetic and generated at runtime in a tempdir. ART ships no copyrighted Amiga content. Runs against the owner's real packages are `#[ignore]`d and environment-gated.

## File Structure

| File | Responsibility |
|---|---|
| `src-tauri/src/core/amigaver.rs` (new) | Read a `$VER:` string out of bytes; compare two of them |
| `src-tauri/src/core/osinstall/source_archive.rs` (new) | `ArchiveSource` — a package as a `MediaSource`, direct and nested |
| `src-tauri/src/core/osinstall/collide.rs` (new) | Classify one destination collision into the four classes |
| `src-tauri/src/core/osinstall/recipes/packages/*.json` (new) | The three curated package recipes |
| `src-tauri/src/core/osinstall/package.rs` (new) | A package recipe: parsing, ordering, `requires` |
| `src-tauri/src/core/osinstall/plan.rs` | Gains package components and the collision preview |
| `src-tauri/src/core/osinstall/apply.rs` | Gains the Add entry point |
| `src-tauri/src/commands/osinstall.rs` | Two new read-only commands, one mutating |
| `src/lib/osinstall.ts` | Typed wrappers and mirrored types |
| `src/components/osbuilder/PackagePanel.tsx` (new) | Choose packages, read the preview, confirm |

---

## Task 1: Reading a version out of an Amiga file

Everything the preview promises rests on this, and it is pure — no I/O, no engine. It goes first so the rest can use it.

**Files:**
- Create: `src-tauri/src/core/amigaver.rs`
- Modify: `src-tauri/src/core/mod.rs` (declare the module)
- Test: inline in `amigaver.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `pub struct AmigaVersion { pub name: String, pub version: u32, pub revision: u32 }`, `pub fn read(bytes: &[u8]) -> Option<AmigaVersion>`, `impl PartialOrd for AmigaVersion`.

**What the real strings look like**, read out of the owner's own 3.9 tree — use these in the tests rather than inventing any:

```
assign 37.4 (25.4.91)
adddatatypes 44.4 (4.8.99)
ConClip 44.3 (22.9.99)
addbuffers 37.2 (21.1.91)
binddrivers 38.2 (31.3.92)
```

The marker in the file is `$VER:` followed by whitespace, then the name, then `version.revision`, then a date in brackets. **181 of the tree's 588 files carry one — 31%.** The other 69% is the normal case, not an error.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Real strings, read out of the owner's own AmigaOS 3.9 tree rather
    /// than invented — a parser tested only against what its author
    /// imagined is a parser tested against its author.
    #[test]
    fn reads_the_version_strings_real_amiga_files_carry() {
        for (raw, name, version, revision) in [
            ("$VER: assign 37.4 (25.4.91)", "assign", 37, 4),
            ("$VER: adddatatypes 44.4 (4.8.99)", "adddatatypes", 44, 4),
            ("$VER: ConClip 44.3 (22.9.99)", "ConClip", 44, 3),
            ("$VER: binddrivers 38.2 (31.3.92)", "binddrivers", 38, 2),
        ] {
            let got = read(raw.as_bytes()).unwrap_or_else(|| panic!("no version in {raw}"));
            assert_eq!(got.name, name, "{raw}");
            assert_eq!((got.version, got.revision), (version, revision), "{raw}");
        }
    }

    /// The marker is found wherever it sits, because it sits wherever the
    /// compiler put it — never at a fixed offset.
    #[test]
    fn finds_the_marker_anywhere_in_the_file() {
        let mut bytes = vec![0u8; 4096];
        bytes.extend_from_slice(b"$VER: assign 37.4 (25.4.91)");
        bytes.extend_from_slice(&[0u8; 512]);
        assert_eq!(read(&bytes).unwrap().version, 37);
    }

    /// 69% of a real tree. Absence is the common case and not an error.
    #[test]
    fn a_file_with_no_version_string_answers_none() {
        assert!(read(b"just some bytes, no marker here").is_none());
        assert!(read(&[0u8; 8192]).is_none());
    }

    /// A marker ART cannot parse into two integers is treated as absent.
    /// Guessing at a malformed one is exactly the invention the fourth
    /// collision class exists to prevent (spec §3).
    #[test]
    fn a_malformed_version_is_treated_as_absent() {
        for raw in [
            "$VER: nameonly",
            "$VER: thing x.y (1.1.99)",
            "$VER: thing 44 (1.1.99)",
            "$VER:",
            "$VER: thing 44. (1.1.99)",
        ] {
            assert!(read(raw.as_bytes()).is_none(), "{raw} should not parse");
        }
    }

    /// Version first, then revision — and the date is not part of it, so a
    /// rebuild carrying a later date and the same version is not an update.
    #[test]
    fn compares_version_then_revision_and_ignores_the_date() {
        let older = read(b"$VER: assign 37.4 (25.4.91)").unwrap();
        let newer_revision = read(b"$VER: assign 37.9 (1.1.99)").unwrap();
        let newer_version = read(b"$VER: assign 45.1 (1.1.99)").unwrap();
        let rebuilt = read(b"$VER: assign 37.4 (31.12.99)").unwrap();

        assert!(newer_revision > older);
        assert!(newer_version > newer_revision);
        assert!(!(rebuilt > older), "a later date alone is not a newer version");
        assert!(!(older > rebuilt));
    }

    /// A version marker can run to the end of the file with no date and no
    /// terminator. Real files do this.
    #[test]
    fn a_version_with_no_date_still_reads() {
        assert_eq!(read(b"$VER: thing 45.12").unwrap().revision, 12);
    }

    /// A hostile file must not make this expensive or make it allocate:
    /// the marker's own text is bounded, whatever follows it.
    #[test]
    fn a_marker_followed_by_megabytes_of_digits_is_bounded() {
        let mut bytes = b"$VER: thing 44.".to_vec();
        bytes.extend(std::iter::repeat(b'9').take(4 * 1024 * 1024));
        // Either it parses a bounded prefix or it declines. It must not hang
        // and must not try to hold a four-megabyte integer.
        let _ = read(&bytes);
    }
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cd src-tauri && cargo test amigaver`
Expected: FAIL — the module does not exist.

- [ ] **Step 3: Write the module**

```rust
//! AmigaOS `$VER:` strings — what a file says about its own version.
//!
//! AmigaOS embeds a version marker in a file's own bytes so that `Version`
//! can report it without knowing the format. ART reads the same marker for
//! one reason: when a package is about to overwrite a file, "45.1 → 45.127"
//! is an answer a person can act on and "different bytes" is not.
//!
//! **It is not a fact about every file.** 181 of the 588 files in a real
//! AmigaOS 3.9 tree carry one — 31%. The other 69% have nothing to say, and
//! saying nothing about them is the correct behaviour, not a gap
//! (spec §89).

use std::cmp::Ordering;

/// The marker, exactly as AmigaOS writes it.
const MARKER: &[u8] = b"$VER:";

/// The most of a marker ART will look at.
///
/// A name and two integers is tens of bytes; anything past this is not a
/// version string, and reading further would let a hostile file decide how
/// much work ART does.
const MAX_MARKER_TEXT: usize = 128;

/// What a file says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmigaVersion {
    pub name: String,
    pub version: u32,
    pub revision: u32,
}

impl PartialOrd for AmigaVersion {
    /// Version, then revision. **The date is deliberately not compared**: a
    /// rebuilt binary can carry a later date and the same version, and
    /// calling that an update would put a downgrade behind a green arrow.
    ///
    /// Names are not compared either. Whether two files are the same thing
    /// is decided by where they land in the tree, not by what they call
    /// themselves — `LoadWB` and `loadwb` are one file to AmigaDOS.
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(
            self.version
                .cmp(&other.version)
                .then(self.revision.cmp(&other.revision)),
        )
    }
}

/// Read the first `$VER:` marker in `bytes`, if there is one ART can parse.
///
/// Returns `None` for a file with no marker **and** for a marker ART cannot
/// turn into two integers. Those two are deliberately the same answer: a
/// half-understood version on screen is worse than none, because the reader
/// cannot tell which half was guessed.
pub fn read(bytes: &[u8]) -> Option<AmigaVersion> {
    let at = bytes
        .windows(MARKER.len())
        .position(|w| w == MARKER)?;
    let start = at + MARKER.len();
    let end = bytes.len().min(start.saturating_add(MAX_MARKER_TEXT));
    let text = std::str::from_utf8(&bytes[start..end]).ok().or_else(|| {
        // A marker is ASCII; a file whose bytes after it are not valid UTF-8
        // is answered from the ASCII prefix rather than refused outright.
        std::str::from_utf8(&bytes[start..end])
            .ok()
    })?;
    parse(text)
}

/// The text after the marker: ` name version.revision (date)`.
fn parse(text: &str) -> Option<AmigaVersion> {
    let mut words = text.split_whitespace();
    let name = words.next()?;
    if name.is_empty() {
        return None;
    }
    let number = words.next()?;
    let (version, revision) = number.split_once('.')?;
    Some(AmigaVersion {
        name: name.to_string(),
        version: version.parse().ok()?,
        revision: revision.parse().ok()?,
    })
}
```

`read`'s UTF-8 handling above is written twice on purpose in this sketch — **fix it**: decide once whether a non-UTF-8 tail is refused or read as Latin-1, say why in the doc comment, and make the test suite cover the case you chose. `core/iso/descriptor.rs` decodes Latin-1 for AmigaDOS text and its reasoning applies here; follow it or argue against it, but do not leave the duplicate.

- [ ] **Step 4: Green**

Run: `cd src-tauri && cargo test amigaver && cargo fmt && cargo clippy --all-targets -- -D warnings`
Expected: all seven tests pass.

- [ ] **Step 5: Measure it against the real tree**

Add an `#[ignore]`d, environment-gated hook beside the tests that walks a real distribution tree and reports how many files carry a version:

```rust
    /// The 31% figure the spec rests on, re-measurable rather than quoted.
    #[test]
    #[ignore = "reads a real distribution tree; run explicitly"]
    fn count_version_strings_in_a_real_tree_when_asked() {
        let Ok(root) = std::env::var("ART_VER_TREE") else {
            return;
        };
        // Walk it, read the first 1 MiB of each file, count and print.
        // Print a handful of examples with their paths.
    }
```

Write the body. Run it against `E:\amiga\ProjeART\AmigaOS39-release-timing` and put the numbers in your report — if they are not close to 181 of 588, the parser disagrees with reality and reality wins.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/core/amigaver.rs src-tauri/src/core/mod.rs
git commit -m "feat(amigaver): read what an Amiga file says about its own version"
```

---

## Task 2: A package is a medium

**Files:**
- Create: `src-tauri/src/core/osinstall/source_archive.rs`
- Modify: `src-tauri/src/core/osinstall/mod.rs` (declare it)
- Test: inline

**Interfaces:**
- Consumes: `core::archive::{open, ArchiveBackend, ArchiveEntry}`; `MediaSource`, `MediaEntry` from `core/osinstall/source.rs`.
- Produces: `ArchiveSource::open(path: &Path) -> CoreResult<Self>`, `impl MediaSource for ArchiveSource`.

**Read first:** `core/osinstall/source_cd.rs`. It is the most recent `MediaSource` and the one to parallel — how it opens, how it answers `volume_name`, how it maps entries, how its tests are written. And `core/osinstall/source_contract.rs`, which asks every implementation the same questions: **your new type joins that list**, and if it fails one of those questions the answer is to fix the type, not the contract.

**The volume name comes from inside, not from the filename.** `AdfSource` reads it from the root block and `CdSource` from the volume descriptor, both so a renamed file still identifies itself. An archive has no volume label, so the equivalent is **its single top-level directory**. Measured on the owner's own packages:

```
BoingBag39-1.lha        →  BoingBag3.9-1     (plus BoingBag3.9-1.info at the root)
BoingBag39-2.lha        →  BoingBag3.9-2     (plus BoingBag3.9-2.info at the root)
BoingBag39-2-turkce.lha →  LocaleUpdate
```

Note the `.info` files sitting at the archive root beside the directory: they are Workbench icons, they are not the payload, and the rule is *the single top-level **directory***, not the single top-level entry.

An archive with **no** top-level directory, or with **more than one**, has no volume name ART can state. Refuse it with a message that says which it was — this round supports the packages it ships recipes for, and an archive of an unexpected shape is exactly the case §89 requires ART to name rather than guess at.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Build a ZIP in a tempdir. Synthetic, generated at runtime — ART ships
    /// no Amiga content, and these names are the *shape* of the owner's real
    /// packages, not their contents.
    fn package_zip(dir: &std::path::Path, name: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(
            &path,
            crate::core::archive::zip::tests::make_zip_with(files),
        )
        .unwrap();
        path
    }

    #[test]
    fn the_volume_name_is_the_single_top_level_directory() {
        let dir = scratch("archive-volume");
        let p = package_zip(
            &dir,
            "renamed-by-the-user.zip",
            &[
                ("BoingBag3.9-1.info", b"icon"),
                ("BoingBag3.9-1/C/Assign", b"assign"),
                ("BoingBag3.9-1/Libs/x.library", b"lib"),
            ],
        );
        let src = ArchiveSource::open(&p).unwrap();
        assert_eq!(src.volume_name(), "BoingBag3.9-1");
    }

    #[test]
    fn two_top_level_directories_are_refused_by_name() {
        let dir = scratch("archive-two-tops");
        let p = package_zip(
            &dir,
            "two.zip",
            &[("One/a", b"a"), ("Two/b", b"b")],
        );
        let err = ArchiveSource::open(&p).unwrap_err().to_string();
        assert!(err.contains("One") && err.contains("Two"), "got {err}");
    }

    #[test]
    fn no_top_level_directory_is_refused() {
        let dir = scratch("archive-flat");
        let p = package_zip(&dir, "flat.zip", &[("a", b"a"), ("b", b"b")]);
        assert!(ArchiveSource::open(&p).is_err());
    }

    /// Paths are relative to the top-level directory, not to the archive:
    /// a rule's `from` says `C/Assign`, never `BoingBag3.9-1/C/Assign`.
    #[test]
    fn paths_are_relative_to_the_top_level_directory() {
        let dir = scratch("archive-rel");
        let p = package_zip(
            &dir,
            "bb.zip",
            &[("BB/C/Assign", b"assign"), ("BB/Libs/x.library", b"lib")],
        );
        let mut src = ArchiveSource::open(&p).unwrap();
        assert!(src.entry("C/Assign").unwrap().is_some());
        assert!(src.entry("BB/C/Assign").unwrap().is_none());
        assert_eq!(src.read("C/Assign").unwrap(), b"assign");
    }

    /// A traversing entry name never becomes a path. The gate is
    /// `core::security::safe_join`, and this proves the source does not go
    /// round it.
    #[test]
    fn a_traversing_entry_name_is_refused() {
        let dir = scratch("archive-traversal");
        let p = package_zip(
            &dir,
            "bad.zip",
            &[("BB/../../etc/passwd", b"x"), ("BB/ok", b"ok")],
        );
        // Either `open` refuses the archive or the entry never appears.
        match ArchiveSource::open(&p) {
            Err(_) => {}
            Ok(mut src) => {
                let all = src.walk("").unwrap();
                assert!(
                    all.iter().all(|e| !e.path.contains("..")),
                    "a traversing name survived: {:?}",
                    all.iter().map(|e| &e.path).collect::<Vec<_>>()
                );
            }
        }
    }
}
```

`scratch` is this codebase's tempdir helper — find how `source_cd.rs`'s tests get one and use the same. `make_zip_with` is `core/archive/zip.rs`'s existing test helper (line ~123); if it is not reachable from here, say so in your report and use the smallest change that makes it reachable, following how `core::iso::fixture` was made `pub(crate)`.

- [ ] **Step 2: Run them and watch them fail**

Run: `cd src-tauri && cargo test osinstall::source_archive`

- [ ] **Step 3: Write `ArchiveSource`**

The shape, with the reasoning that has to be in the doc comments:

```rust
pub struct ArchiveSource {
    path: PathBuf,
    volume_name: String,
    /// Every entry, path **relative to the top-level directory**, with the
    /// backend index that reads it. Listing is cheap and reading is not, so
    /// the listing is held and the bytes are not.
    entries: Vec<(String, usize, ArchiveEntry)>,
    backend: Box<dyn ArchiveBackend>,
}
```

`open` lists once, finds the single top-level directory, strips it from every path, and keeps the index alongside. `read` calls `backend.read(index, limit)` with a limit — never an unbounded read. `walk` returns the listing; `entry("")` answers a synthetic root, matching `AdfSource::root_entry` and `CdSource::root_entry`. `walk` on a path naming a file **refuses**, as both siblings do.

- [ ] **Step 4: Find a package archive in a folder**

Nothing does this today. `scan::find_media` tries `AdfSource::open` and then
`CdSource::open` on every file in a folder; a `.lha` is neither, so it is
skipped. And the owner's own layout keeps the two apart —
`E:\amiga\Amigatolon\iso` holds install media, `E:\amiga\Amigatolon\paketler`
holds fifty-eight packages — so packages are not simply more files in the
media folder.

Add, in `scan.rs` beside `find_media`:

```rust
/// Every package archive in `folder`, identified from **inside** each file.
///
/// Deliberately separate from `find_media` rather than a third arm of its
/// probe. Install media and packages are different questions asked of
/// different folders — the owner keeps discs in one and archives in another —
/// and folding them together would mean opening every 469 MiB disc in the
/// package folder to find out it is not a package.
///
/// A file that is not an archive, or is an archive of a shape
/// `ArchiveSource` refuses, is **skipped rather than fatal** — the same rule
/// `find_media` follows, and for the same reason: one unreadable candidate
/// in a folder of fifty-eight must not fail the scan.
pub fn find_packages(folder: &Path) -> CoreResult<Vec<FoundPackage>>;

/// One archive `find_packages` opened, and the name it gave for itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundPackage {
    pub path: PathBuf,
    /// The archive's single top-level directory — never its filename.
    pub media: String,
}
```

Tests: a folder holding one valid package and one file that is not an
archive reports the valid one and does not fail; a folder holding two
packages reports both **in a deterministic order** (`find_media` sorts its
entries and says why — do the same); an unreadable folder is an error, not
an empty list.

- [ ] **Step 5: Join the contract test**

Add `ArchiveSource` to `core/osinstall/source_contract.rs`'s list of implementations. Read that file first: it exists because three divergences between `AdfSource` and `CdSource` were found one at a time, and a fourth implementation joining without being asked the same questions would be the fourth.

- [ ] **Step 6: Green, and commit**

Run: `cd src-tauri && cargo test osinstall && cargo fmt && cargo clippy --all-targets -- -D warnings`

```bash
git add -A
git commit -m "feat(osinstall): a package archive is a medium, and can be found"
```

---

## Task 3: A medium inside a medium

A BoingBag's payload is a ZIP stored **uncompressed** inside an LHA, and its 234 paths map straight onto a system volume. Reading it means opening an archive over a member of another archive.

**Files:**
- Modify: `src-tauri/src/core/osinstall/source_archive.rs`
- Test: inline

**Interfaces:**
- Produces: `ArchiveSource::open_nested(outer: &Path, member: &str) -> CoreResult<Self>`.

**The decision, and why.** `core::archive::open` takes a path, and `ZipBackend` holds a `ZipArchive<BufReader<File>>` — there is no constructor over bytes. Rather than make every backend generic, **extract the member to a temporary file through the existing gate and open that**. Three things fall out of it for free:

1. `core::detect` runs on the extracted bytes, so a member that claims to be a ZIP and is not is caught rather than trusted.
2. Every backend stays unchanged.
3. The member is bounded on the way out by the gate that already bounds it.

The cost is one temporary file. The measured member is 1.7 MB, and the temp file must be cleaned up whether the open succeeds or fails.

- [ ] **Step 1: Write the failing tests**

```rust
    /// The BoingBag shape: an archive whose payload is another archive.
    #[test]
    fn a_nested_member_becomes_the_medium() {
        let dir = scratch("archive-nested");
        let inner = crate::core::archive::zip::tests::make_zip_with(&[
            ("Libs/version.library", b"lib bytes"),
            ("C/Version", b"cmd bytes"),
        ]);
        let outer = package_zip(
            &dir,
            "BoingBagLike.zip",
            &[
                ("BB/AmigaOS-Update", &inner),
                ("BB/C/Updater", b"an amiga program ART does not run"),
            ],
        );

        let mut src = ArchiveSource::open_nested(&outer, "AmigaOS-Update").unwrap();
        assert_eq!(src.read("Libs/version.library").unwrap(), b"lib bytes");
        // The outer archive's own files are *not* visible: the medium is the
        // payload, and `C/Updater` belongs to the wrapper.
        assert!(src.entry("C/Updater").unwrap().is_none());
    }

    /// A member that is not an archive at all is refused, not treated as an
    /// empty medium — a silently empty medium is a silently short plan.
    #[test]
    fn a_member_that_is_not_an_archive_is_refused() {
        let dir = scratch("archive-nested-bad");
        let outer = package_zip(
            &dir,
            "bad.zip",
            &[("BB/AmigaOS-Update", b"not an archive, just bytes")],
        );
        assert!(ArchiveSource::open_nested(&outer, "AmigaOS-Update").is_err());
    }

    /// A member the outer archive does not hold is refused by name.
    #[test]
    fn a_missing_member_is_refused_by_name() {
        let dir = scratch("archive-nested-missing");
        let outer = package_zip(&dir, "x.zip", &[("BB/Something", b"x")]);
        let err = ArchiveSource::open_nested(&outer, "AmigaOS-Update")
            .unwrap_err()
            .to_string();
        assert!(err.contains("AmigaOS-Update"), "got {err}");
    }

    /// Nothing is left behind, whether the open worked or not.
    #[test]
    fn the_temporary_extraction_is_cleaned_up_either_way() {
        // Count the entries in the temp directory this uses before and after
        // both a successful and a failed `open_nested`, and assert they match.
        // Read how `core/safety/atomic.rs` names its temporary files and where
        // this codebase puts scratch, and follow it.
    }
```

Write the fourth test's body against whatever this codebase actually does for temporary files — do not invent a new convention.

- [ ] **Step 2: Run them, watch them fail, then implement**

Run: `cd src-tauri && cargo test osinstall::source_archive`

`open_nested` lists the outer archive, finds `member` **relative to the top-level directory** (the same stripping `open` does — the recipe says `AmigaOS-Update`, not `BoingBag3.9-1/AmigaOS-Update`), reads it bounded, writes it to a temporary file, and opens that with `ArchiveSource::open`'s own logic.

**Watch out:** the inner archive may have no single top-level directory — the measured BoingBag payload's top level is `C`, `Classes`, `Devs`, `Libs`, … which is thirteen directories, not one. So the nested case must **not** apply the single-top-level rule to the inner archive: its paths are already volume-relative. The volume name of a nested source comes from the **outer** archive's top-level directory. Getting this backwards produces an unopenable BoingBag, and the test above is what catches it.

- [ ] **Step 3: Green, and commit**

Run: `cd src-tauri && cargo test osinstall && cargo fmt && cargo clippy --all-targets -- -D warnings`

```bash
git add -A
git commit -m "feat(osinstall): read a package payload out of its wrapper"
```

---

## Task 4: What a package recipe says

**Files:**
- Create: `src-tauri/src/core/osinstall/package.rs`
- Create: `src-tauri/src/core/osinstall/recipes/packages/boingbag-39-1.json`
- Create: `src-tauri/src/core/osinstall/recipes/packages/boingbag-39-2.json`
- Create: `src-tauri/src/core/osinstall/recipes/packages/locale-turkish.json`
- Modify: `src-tauri/src/core/osinstall/mod.rs`
- Test: inline in `package.rs`

**Interfaces:**
- Consumes: `Component`, `PathRule`, `RuleKind` from `core/osinstall/mod.rs`; `recipe::parse`'s validation shape.
- Produces: `pub fn packages() -> CoreResult<Vec<Package>>`, `pub fn by_id(id: &str) -> CoreResult<Package>`, `pub fn order(chosen: &[String]) -> CoreResult<Vec<String>>`, and:

```rust
pub struct Package {
    pub id: String,
    /// Shown on screen. Not the id, and not translated — a package's name is
    /// its own, the way a volume name is (ART-060).
    pub name: String,
    /// The archive's single top-level directory, read from inside it.
    pub media: String,
    /// The member holding the payload, for a package whose files sit inside
    /// a second archive. `None` for loose files at direct paths.
    pub member: Option<String>,
    pub requires: Vec<String>,
    /// The rules and `overrides`, in the shape the placer already takes.
    pub component: Component,
}
```

The JSON is flat — `id`, `name`, `media`, `member`, `requires`, `overrides`, `rules` — and parsing maps `id`/`media`/`rules`/`overrides` into `component`. The file a person edits should not have to know the engine's struct layout. `Component.required` is always `false` for a package and `condition` always `None`: a package is something the user chooses, never something a ROM version switches on.

**Read first:** `core/osinstall/recipe.rs`, including how `AMIGAOS_39_JSON` is embedded with `include_str!` and how `shipped_recipes()` drives the invariant tests from `releases()`/`by_release()`. **Follow that mechanism exactly.** Packages get the same treatment: a `packages()` list that the invariant tests iterate, so a fourth package cannot be added without the tests reaching it.

**The three recipes, from the measured archives:**

```json
{
  "id": "boingbag-39-1",
  "name": "BoingBag 3.9-1",
  "media": "BoingBag3.9-1",
  "member": "AmigaOS-Update",
  "requires": [],
  "overrides": ["workbench-base"],
  "rules": [
    { "from": "C",         "to": "C",         "kind": "subtree" },
    { "from": "Classes",   "to": "Classes",   "kind": "subtree" },
    { "from": "Devs",      "to": "Devs",      "kind": "subtree" },
    { "from": "Fonts",     "to": "Fonts",     "kind": "subtree" },
    { "from": "L",         "to": "L",         "kind": "subtree" },
    { "from": "Libs",      "to": "Libs",      "kind": "subtree" },
    { "from": "Prefs",     "to": "Prefs",     "kind": "subtree" },
    { "from": "S",         "to": "S",         "kind": "subtree" },
    { "from": "Storage",   "to": "Storage",   "kind": "subtree" },
    { "from": "System",    "to": "System",    "kind": "subtree" },
    { "from": "Tools",     "to": "Tools",     "kind": "subtree" },
    { "from": "Utilities", "to": "Utilities", "kind": "subtree" },
    { "from": "WBStartup", "to": "WBStartup", "kind": "subtree" }
  ]
}
```

Those thirteen names are the measured top level of the payload ZIP; **check them against the real archive before shipping the file** and correct any that differ — the 3.9 recipe's own paths were all wrong on the first real run and only a real run found it.

`boingbag-39-2.json` is the same shape with `"media": "BoingBag3.9-2"` and **`"requires": ["boingbag-39-1"]`**. Its own top-level names must be read off the real archive rather than copied from 39-1.

`locale-turkish.json` has no `member` — Shape B, loose files:

```json
{
  "id": "locale-turkish",
  "name": "Türkçe catalogs (BoingBag 3.9-2)",
  "media": "LocaleUpdate",
  "requires": [],
  "overrides": [],
  "rules": [
    { "from": "locale/catalogs", "to": "Locale/Catalogs", "kind": "subtree" }
  ]
}
```

Read the archive and confirm the source path's real spelling and the destination the base tree actually uses (`Locale` vs `LOCALE` — the base tree from the disc is upper-case; resolution is case-insensitive but the *destination* spelling is what lands on the volume, so decide it deliberately and say why in your report).

- [ ] **Step 1: Write the failing tests**

```rust
    /// Every shipped package parses and validates. This is the test that
    /// fails when a JSON file is added and its rules are wrong.
    #[test]
    fn every_shipped_package_parses() {
        for p in super::packages().expect("the shipped packages must parse") {
            assert!(!p.id.is_empty());
            assert!(!p.media.is_empty());
            assert!(!p.component.rules.is_empty(), "{} has no rules", p.id);
        }
    }

    /// `requires` names a package that exists. A dependency on something ART
    /// does not ship is a recipe that can never be satisfied.
    #[test]
    fn every_requirement_names_a_shipped_package() {
        let all = super::packages().unwrap();
        let ids: Vec<&str> = all.iter().map(|p| p.id.as_str()).collect();
        for p in &all {
            for need in &p.requires {
                assert!(ids.contains(&need.as_str()), "{} requires unknown {need}", p.id);
            }
        }
    }

    /// BoingBag 2 assumes BoingBag 1. Applying them the other way round is a
    /// wrong system, not a warning (spec §8).
    #[test]
    fn boingbag_two_requires_boingbag_one() {
        let two = super::by_id("boingbag-39-2").unwrap();
        assert!(two.requires.contains(&"boingbag-39-1".to_string()));
    }

    /// An unknown id is refused by name, never defaulted to some other
    /// package — the same rule `recipe::by_release` follows, for the same
    /// reason.
    #[test]
    fn an_unknown_package_is_refused_by_name() {
        let err = super::by_id("boingbag-39-9").unwrap_err().to_string();
        assert!(err.contains("boingbag-39-9"), "got {err}");
    }

    /// Ordering is derived from `requires`, not from the order the user
    /// happened to tick the boxes in.
    #[test]
    fn selection_order_does_not_decide_application_order() {
        let a = super::order(&["boingbag-39-2".into(), "boingbag-39-1".into()]).unwrap();
        let b = super::order(&["boingbag-39-1".into(), "boingbag-39-2".into()]).unwrap();
        assert_eq!(a, b);
        assert_eq!(a, vec!["boingbag-39-1".to_string(), "boingbag-39-2".to_string()]);
    }

    /// Choosing a package without what it requires is refused, saying what
    /// is missing — not silently added, because adding a whole package the
    /// user did not ask for is a bigger surprise than a refusal.
    #[test]
    fn a_requirement_that_was_not_chosen_is_refused_by_name() {
        let err = super::order(&["boingbag-39-2".into()]).unwrap_err().to_string();
        assert!(err.contains("boingbag-39-1"), "got {err}");
    }
```

- [ ] **Step 2: Run, watch fail, implement**

Run: `cd src-tauri && cargo test osinstall::package`

`order` is a topological sort over `requires` with a **cycle check** — a cycle in shipped data is a bug in the data, and it must be an error rather than a hang. Add a test for a synthetic cycle using a hand-built `Vec<Package>` if `order` can take one; if it can only read the shipped list, say so in your report and explain how a cycle would be caught.

- [ ] **Step 3: Widen the shipped-recipe invariant tests**

`recipe.rs`'s `shipped_recipes()` drives four tests that police destination names, collisions, `overrides` and escapes. **Packages must be inside those same tests.** Extend the helper so it yields every package's component alongside every release's, and make it structurally impossible to add a package that the invariants do not reach — that is the defect those helpers were widened to fix once already.

- [ ] **Step 4: Green, and commit**

Run: `cd src-tauri && cargo test osinstall && cargo fmt && cargo clippy --all-targets -- -D warnings`

```bash
git add -A
git commit -m "feat(osinstall): the three curated package recipes"
```

---

## Task 5: What a package would land on

The preview. Nothing is applied here — this task is entirely read-only and entirely about telling the truth.

**Files:**
- Create: `src-tauri/src/core/osinstall/collide.rs`
- Modify: `src-tauri/src/core/osinstall/mod.rs`
- Test: inline

**Interfaces:**
- Consumes: `core::amigaver::{read, AmigaVersion}`.
- Produces:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Collision {
    /// The bytes are the same. Not an overwrite at all.
    Identical,
    /// Both sides say what they are, and the incoming one is newer.
    Upgrade { from: String, to: String },
    /// Both sides say what they are, and the incoming one is **older**.
    Downgrade { from: String, to: String },
    /// One side or both says nothing. Sizes, and no invented version.
    Unversioned { from_bytes: u64, to_bytes: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollisionReport {
    /// Where in the tree, `/`-separated.
    pub path: String,
    pub collision: Collision,
    /// Whether the package's recipe declared it may write over what is there.
    pub declared: bool,
}

pub fn classify(existing: &[u8], incoming: &[u8]) -> Collision;
```

- [ ] **Step 1: Write the failing tests**

```rust
    /// Same bytes is not an overwrite, and a user asked to confirm one has
    /// been asked a question with no content.
    #[test]
    fn identical_bytes_are_not_an_overwrite() {
        assert_eq!(classify(b"same", b"same"), Collision::Identical);
    }

    #[test]
    fn a_newer_version_is_an_upgrade() {
        let old = b"$VER: assign 37.4 (25.4.91)".as_slice();
        let new = b"$VER: assign 45.9 (1.1.99)".as_slice();
        assert_eq!(
            classify(old, new),
            Collision::Upgrade { from: "37.4".into(), to: "45.9".into() }
        );
    }

    /// The case the whole engine exists because of: `ModulesA1200_3.2.adf`
    /// holds fourteen commands and thirteen are *older* than the ones
    /// `Workbench3.2` already carries.
    #[test]
    fn an_older_version_is_a_downgrade() {
        let new = b"$VER: assign 45.9 (1.1.99)".as_slice();
        let old = b"$VER: assign 37.4 (25.4.91)".as_slice();
        assert!(matches!(classify(new, old), Collision::Downgrade { .. }));
    }

    /// Equal versions, different bytes. Not an upgrade — and specifically
    /// not silently called one.
    #[test]
    fn the_same_version_with_different_bytes_is_not_an_upgrade() {
        let a = b"$VER: assign 37.4 (25.4.91)\x00A".as_slice();
        let b = b"$VER: assign 37.4 (25.4.91)\x00B".as_slice();
        assert!(!matches!(classify(a, b), Collision::Upgrade { .. }));
    }

    /// 69% of a real tree. One side saying nothing is enough.
    #[test]
    fn a_file_with_no_version_reports_sizes_and_no_version() {
        let existing = b"plain bytes".as_slice();
        let incoming = b"$VER: thing 45.1 (1.1.99)".as_slice();
        assert_eq!(
            classify(existing, incoming),
            Collision::Unversioned {
                from_bytes: existing.len() as u64,
                to_bytes: incoming.len() as u64
            }
        );
    }

    /// A date alone never moves the verdict.
    #[test]
    fn a_rebuild_with_a_later_date_is_not_an_upgrade() {
        let a = b"$VER: thing 44.1 (1.1.99)\x00x".as_slice();
        let b = b"$VER: thing 44.1 (31.12.02)\x00y".as_slice();
        assert!(!matches!(classify(a, b), Collision::Upgrade { .. }));
    }
```

- [ ] **Step 2: Run, watch fail, implement**

Run: `cd src-tauri && cargo test osinstall::collide`

- [ ] **Step 3: Build the report over a real plan**

Add `pub fn preview(tree_root: &Path, items: &[PlanItem]) -> CoreResult<Vec<CollisionReport>>` in the same module: for every planned item whose destination already exists in the tree, read both sides **bounded** and classify. `Identical` entries are **excluded from the report** — they are not overwrites and listing them would bury the ones that matter.

`declared` comes from whether the package's component names the existing file's owning component in its `overrides`. The owner of an existing file is in `distribution.json`; read it, do not guess.

Write tests over a real tempdir tree for: an item that lands on nothing (absent from the report), one that lands on identical bytes (absent), one that lands on an older version (present, `Downgrade`), and one whose collision the recipe did not declare (`declared: false`).

- [ ] **Step 4: Green, and commit**

Run: `cd src-tauri && cargo test osinstall && cargo fmt && cargo clippy --all-targets -- -D warnings`

```bash
git add -A
git commit -m "feat(osinstall): say what a package would land on before it lands"
```

---

## Task 6: Produce, Add, and the test that binds them

**Files:**
- Modify: `src-tauri/src/core/osinstall/plan.rs`
- Modify: `src-tauri/src/core/osinstall/apply.rs`
- Test: inline in both

**Interfaces:**
- Consumes: everything from Tasks 2–5, including `scan::find_packages` and `scan::FoundPackage`.
- Produces: `InstallRequest.packages: Vec<String>`, `InstallRequest.package_folder: Option<PathBuf>`; `pub fn add_package(tree_root: &Path, package: &Package, archive: &Path, sink: &dyn ProgressSink) -> CoreResult<ApplyOutcome>`.

`add_package` takes the archive's path rather than looking it up: **Add**
works on one package the caller already resolved, and a second discovery
pass inside it would be a second place for "which file is this package" to
be decided.

**`InstallRequest` gains two fields, not one.** `packages: Vec<String>` says
which, and **`package_folder: Option<PathBuf>`** says where to look — the
owner keeps discs in `Amigatolon\iso` and archives in `Amigatolon\paketler`,
so the media folder cannot answer it. `None` means no packages were asked
for; a request naming packages with no folder is a refusal that says so, not
an empty result.

Resolution mirrors what already exists: `find_packages(folder)` gives
`Vec<FoundPackage>`, a package's `media` is matched against it, and the
existing `MediaMatch` shape answers **missing** and **ambiguous** — two
archives in one folder claiming the same top-level directory is a real case
(the owner has `BoingBag39-2.lha` and eight language variants beside it) and
must be refused by name rather than resolved by whichever sorted first.

**Produce** extends the existing path: `plan()` resolves the chosen packages
in `order()`'s order, after the release's own components, and `apply()`
places them in that order. **Add** takes an existing tree and one package,
and writes into it.

Both must record what they did in `distribution.json`: a file that a package overwrote records the package as its source **and** what it overwrote, so the manifest stays a true account of where every byte came from.

- [ ] **Step 1: Write the equivalence test first**

This is the test the owner asked for, and the one that catches what reading either path cannot:

```rust
    /// Every file in a tree, path -> bytes, with the manifest left out.
    ///
    /// `distribution.json` records *when* an install happened and in how
    /// many steps, which legitimately differs between the two paths;
    /// everything it says about the tree's contents is checked separately
    /// below. Every other file, `.uaem` sidecars included, must match
    /// exactly.
    fn tree_contents(root: &std::path::Path) -> std::collections::BTreeMap<String, Vec<u8>> {
        let mut out = std::collections::BTreeMap::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let rel = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                if rel == MANIFEST_FILE_NAME {
                    continue;
                }
                out.insert(rel, std::fs::read(&path).unwrap());
            }
        }
        out
    }

    /// `produce(base + A)` and `add(produce(base), A)` must give the same
    /// tree, byte for byte. Two entry points into one placer that disagree
    /// mean one of them is wrong, and nothing short of comparing the
    /// results says which.
    #[test]
    fn producing_with_a_package_equals_adding_it_afterwards() {
        let dir = scratch("equivalence");
        let media = synthetic_media_folder(&dir);
        let left = dir.join("produced");
        let right = dir.join("added");

        // produce(base + package)
        let with_package = install_request(&media, &left, &["test-package"]);
        let planned = plan(&with_package, &test_recipe()).unwrap();
        apply(&planned, &left, &NoProgress).unwrap();

        // add(produce(base), package)
        let base_only = install_request(&media, &right, &[]);
        let base_plan = plan(&base_only, &test_recipe()).unwrap();
        apply(&base_plan, &right, &NoProgress).unwrap();
        add_package(
            &right,
            &package::by_id("test-package").unwrap(),
            &NoProgress,
        )
        .unwrap();

        let a = tree_contents(&left);
        let b = tree_contents(&right);

        // Name what differs rather than asserting equality over a wall of
        // bytes: the first line of a failure should be a path.
        let only_left: Vec<&String> = a.keys().filter(|k| !b.contains_key(*k)).collect();
        let only_right: Vec<&String> = b.keys().filter(|k| !a.contains_key(*k)).collect();
        assert!(only_left.is_empty(), "only in the built tree: {only_left:?}");
        assert!(only_right.is_empty(), "only in the added tree: {only_right:?}");
        for (path, left_bytes) in &a {
            assert_eq!(left_bytes, &b[path], "{path} differs between the two paths");
        }

        // The manifest's account of the tree must agree even though its
        // timestamps do not: every file records the same source component
        // and the same thing overwritten.
        assert_eq!(
            sources_by_path(&read_manifest(&left)),
            sources_by_path(&read_manifest(&right))
        );
    }
```

`scratch`, `synthetic_media_folder`, `install_request`, `test_recipe`, `read_manifest` and `sources_by_path` are helpers you write here or reuse — **read `apply.rs`'s existing tests first**, because several of them very likely exist already under other names, and a second copy of a fixture builder is how two tests start disagreeing about what a tree looks like.

**The package this test applies must overwrite at least one base file.** A package that only adds files would make the test pass while proving nothing about the case the whole round is about.

- [ ] **Step 2: Run it, watch it fail, implement both paths**

Run: `cd src-tauri && cargo test osinstall::plan osinstall::apply`

- [ ] **Step 3: The refusals**

Three, each with a test:

- A package whose `requires` is not satisfied — refused by name (Task 4's `order` already does this; make sure the refusal reaches the plan's `refusals` rather than becoming a hard error).
- A package whose archive is not in the package folder — the existing `MediaMissing` shape, and its **ambiguous** sibling for two archives claiming one name.
- A request naming packages with no `package_folder` — refused saying which packages were asked for.
- **Add** onto a tree that has no `distribution.json` — refused. Without the manifest ART cannot say what it is overwriting, and the whole preview rests on knowing.

- [ ] **Step 4: Green, and commit**

Run: `cd src-tauri && cargo test && cargo fmt && cargo clippy --all-targets -- -D warnings`

```bash
git add -A
git commit -m "feat(osinstall): build with packages, or add one to a built tree"
```

---

## Task 7: The screen

**Files:**
- Create: `src/components/osbuilder/PackagePanel.tsx`
- Create: `src/components/osbuilder/PackagePanel.test.tsx`
- Modify: `src/components/osbuilder/OsInstall.tsx`
- Modify: `src/lib/osinstall.ts`
- Modify: `src-tauri/src/commands/osinstall.rs`, `src-tauri/src/lib.rs`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json`

**Interfaces:**
- Produces: commands `osinstall_packages` (read-only, the shipped list), `osinstall_collisions` (read-only, the preview), `osinstall_add_package` (mutating, a job id).

**Read first:** how the release picker and the component checklist were built in `OsInstall.tsx` — the catalogue is loaded per release, picks are remembered per release, and a late-landing load must not overwrite something the user just touched. Packages follow the same shape.

- [ ] **Step 1: The two read-only commands**

Both take no destructive argument and write nothing. `osinstall_packages`
takes the package folder and returns the shipped packages **paired with
whether each one's archive was actually found there** — a checkbox for a
package whose file is absent is a promise ART cannot keep.
`osinstall_collisions` takes the tree root, the package folder and the chosen
package ids, and returns `Vec<CollisionReport>`. It has to build the plan
items to do that; it must build them the same way `plan()` does rather than
growing a second, nearly-identical resolver.

Register both in `invoke_handler![]` **and** add typed wrappers in `src/lib/osinstall.ts`. Mirror `Collision` and `CollisionReport` exactly, including the `kind` tag.

- [ ] **Step 2: The panel**

It shows, in this order:

1. The shipped packages, each with a checkbox, its name, and what it requires.
2. When at least one is chosen: **the preview**, grouped by class — downgrades first, then upgrades, then unversioned. Identical files never appear.
3. A single confirmation for the whole set. **Not one per file** — a BoingBag exists to replace things, and fifty-seven confirmations teach a user to click through.
4. An undeclared collision is marked as such beside its row.
5. **A line saying what ART does not do.** The spec's §5 requires it and no
   other task carries it: this round installs the packages it ships recipes
   for and nothing else, so the panel must say so rather than leaving a user
   to conclude their own archive is broken. Name the count — ART knows three —
   and say that a package with no recipe, or one whose installation is an
   Amiga Installer script, cannot be placed in this version. Do **not** imply
   it is coming; say what is true today.

The counts go in the heading (`3 downgrades, 41 upgrades, 13 unversioned`) so the shape is legible before the list is read.

- [ ] **Step 3: The component test**

```tsx
  const REPORTS: CollisionReport[] = [
    {
      path: "Libs/x.library",
      declared: true,
      collision: { kind: "upgrade", from: "44.1", to: "45.9" },
    },
    {
      path: "C/Assign",
      declared: true,
      collision: { kind: "downgrade", from: "45.9", to: "37.4" },
    },
    {
      path: "Devs/y.device",
      declared: false,
      collision: { kind: "unversioned", fromBytes: 1024, toBytes: 2048 },
    },
  ];

  function renderPanel() {
    vi.mocked(osinstallCollisions).mockResolvedValue(REPORTS);
    render(<PackagePanel treeRoot="E:/tree" chosen={["boingbag-39-1"]} />);
  }

  it("lists downgrades before upgrades", async () => {
    renderPanel();
    const rows = await screen.findAllByTestId("collision-row");
    expect(rows[0]).toHaveTextContent("C/Assign");
  });

  it("renders every collision the core reported and invents none", async () => {
    // `Identical` never reaches the panel — the core excludes it — so the
    // panel must render exactly what it was given. A filter here would be a
    // second place for that rule to live, and the two would drift.
    renderPanel();
    expect(await screen.findAllByTestId("collision-row")).toHaveLength(3);
  });

  it("asks once for the whole set, not once per file", async () => {
    renderPanel();
    await screen.findAllByTestId("collision-row");
    expect(screen.getAllByRole("checkbox", { name: /confirm/i })).toHaveLength(1);
  });

  it("marks a collision the recipe did not declare", async () => {
    renderPanel();
    const rows = await screen.findAllByTestId("collision-row");
    const row = rows.find((r) => r.textContent?.includes("Devs/y.device"));
    expect(row).toHaveTextContent(/undeclared/i);
  });
```

Match the query helpers and the mocking `OsInstall.test.tsx` already uses; if it does not use `data-testid`, follow whatever it does instead rather than introducing a second style. The prop names above are a proposal — settle them when you write the component and keep the tests in step.

- [ ] **Step 4: Both catalogues, then green**

Every new string in `en.json` **and** `tr.json`. Then:

Run: `pnpm lint && pnpm test && cd src-tauri && cargo test`

```bash
git add -A
git commit -m "feat(os-builder): choose packages, and see what they would replace"
```

---

## Task 8: Run it against the owner's own packages, and boot it

**Files:**
- Modify: `src-tauri/src/core/osinstall/apply.rs` (a gated hook)
- Modify: `docs/ISSUES.md` for whatever the run finds

The engine's own tests are synthetic and cannot catch a recipe-data mistake. The 3.9 round's real run found four defects nothing else could, and corrected its own recipe's fourteen paths.

- [ ] **Step 1: The hook**

Mirror `build_the_real_39_tree_when_asked`: `#[ignore]`d, gated on environment variables naming the package folder and a destination. It builds a base 3.9 tree, applies BoingBag 39-1, then 39-2, then the Turkish pack, and reports **for each**: files written, directories written, bytes, elapsed, and the collision counts by class.

- [ ] **Step 2: Run it, and let the packages be right**

The owner's packages are at `E:\amiga\Amigatolon\paketler`. Run it. **If a recipe's paths are wrong, the archive is right and the recipe is wrong** — fix the JSON, re-run, and report both the before and the after with real numbers. Do not adjust an assertion to match a disappointing result.

Report the collision classes honestly. A BoingBag that reports zero upgrades has not been applied; one that reports downgrades is telling you something about either the recipe or the package.

- [ ] **Step 3: Boot it**

Use `core::winuae::real_boot_hook::boot_a_distribution_tree_when_asked` with the updated tree and the owner's licensed Kickstart 3.1. **The tree must reach a clean Workbench**, as the base tree did on 2026-08-19.

Then say what changed: a BoingBag-2 system reports a different Workbench version than a base one. Capture what the booted system actually says and put it in your report. If it does not boot, that is this task's finding — report it with the same rigour, and stop rather than fixing the next thing.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "test(osinstall): the real packages, and what the run corrected"
```

---

## Task 9: The documents

**Files:**
- Modify: `docs/FEATURES.md`, `docs/STATUS.md`, `docs/ISSUES.md`, `CHANGELOG.md`

- [ ] **Step 1: Write down what is true**

Take every number from Task 8's report, not from this plan. A row for the content layer with the measured counts; new `ART-NNN` entries for anything the run found and left open; a session log line; the user-visible half in `CHANGELOG.md` in a person's language.

- [ ] **Step 2: State plainly what has not happened**

Name it explicitly: **inspect-and-propose is not in this round**, so ART supports exactly the three packages it ships recipes for and refuses the rest with their reason. The owner's folder holds 58 items. Say so — a reader who assumes any archive works will meet the refusal and think ART is broken.

If the boot in Task 8 did not happen or did not succeed, the feature row stays amber and says why, exactly as the 3.9 row did until it booted.

- [ ] **Step 3: Verify and commit**

Run: `pnpm test` and `cd src-tauri && cargo test`
Expected: both green — documentation must not break either.

```bash
git add docs CHANGELOG.md
git commit -m "docs: what the content layer does, and which packages it knows"
```
