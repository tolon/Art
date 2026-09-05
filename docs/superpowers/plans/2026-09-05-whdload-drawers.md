# WHDLoad drawers implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Catalogue a WHDLoad collection that is shaped as drawers — a directory per title, each holding one `.slave` — and write `igame.data` beside each slave so the Amiga's own launcher can read what ART already knows.

**Architecture:** Two new `Media` variants, deliberately two rather than one with a location field: an **unpacked drawer** is launchable through the `RequestKind::Whdload` path that already exists, and a **drawer inside an archive** is not, and must say so in its own words. A directory scan and an archive scan produce one variant each and never the other. `igame.data` goes onto a copy ART made by default, and into the user's own collection only as an explicit, previewed, backed-up action.

**Tech Stack:** Rust (`src-tauri/src/core/gameindex`, `core/whdload`, `core/amigaicon`, `core/archive`), the existing `core/lha` reader, React for the Collection screen's new verdicts.

**Spec:** [`docs/superpowers/specs/2026-09-05-whdload-drawers-design.md`](../specs/2026-09-05-whdload-drawers-design.md). Read it first; every number in this plan comes from its measurements.

## Global Constraints

- **`src-tauri/src/core/` is platform-independent Rust.** `std` + `serde` + `serde_json` + `sha2` + `log` + `thiserror` + `delharc` + `zip` + `sevenz-rust2` + `quick-xml` + `fatfs` + `libpfs3` only. **Add no dependency.** Never `use tauri` in `core/`.
- **MSRV 1.93.** `cargo clippy --all-targets -- -D warnings` is blocking. `lib.rs` allows only `dead_code` — unused imports and variables are errors.
- **Fixtures are synthetic and generated at runtime in a tempdir. ART ships no copyrighted Amiga content, ever.** `readers::slave::tests_support::build_slave` already builds a slave header; icons are built by hand in the test module.
- **i18n keys go in both `src/i18n/en.json` and `src/i18n/tr.json` in the same commit, with real Turkish.** `dead-keys.test.ts` is blocking: a key with no reader fails the build. `phrase-keys.test.ts` enumerates every `RefusalReason` variant — a new one must be added there or the guard does not exist.
- **A test is not a guard until the defect has been put back and seen to fail it.** Every task with a `Mutations` block runs them and reports what fell. **Report survivors as survivors**, and for each say which of the two it is: a weak guard, or the wrong mutation for that guard.
- **Never `git checkout -- <file>` to undo a mutation** — it destroys uncommitted work; a previous round lost a file that way. Copy the file aside with `shutil.copyfile` and copy it back, then `touch` it so cargo recompiles.
- **Never pass a commit message as a double-quoted shell string** — backticks inside `"..."` run as command substitution and eat words. Write the message to a file and `git commit -F <file>`.
- **ART never writes a drawer icon.** `core::amigaicon::merge_tooltypes` replaces the ToolType array wholesale, and 94 of a real drawer icon's 98 ToolTypes are `IM1=`/`IM2=` NewIcon image data. Reading is allowed; writing is not, anywhere in this plan.
- **Branch:** `art-whdload-drawers`, already created, based on `main`. Merge with `--no-ff` when the plan is done. Do not push without asking.

## Verification commands

```bash
cd src-tauri && cargo test gameindex::         # the module under change
cd src-tauri && cargo test whdload::           # the WHDLoad half
cd src-tauri && cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings
pnpm lint && pnpm test
python scripts/control-byte-sweep.py
```

**Quote the `test result:` line, never the exit code.** A killed harness and a green suite look identical from the shell. Baseline on `main`: **2713 Rust passed, 0 failed, 45 ignored; 1020 frontend across 80 files.**

---

## File structure

| File | Responsibility | Task |
|---|---|---|
| `src-tauri/src/core/gameindex/record.rs` | the two new `Media` variants; `GAMEINDEX_SCHEMA` | 1 |
| `src-tauri/src/commands/launch.rs` | routing each variant to its own ending | 1 |
| `src-tauri/src/core/whdload/mod.rs` | reading a WHDLoad launch configuration out of an icon's ToolTypes | 2 |
| `src-tauri/src/core/gameindex/readers/drawer.rs` | **created** — a directory is a title | 3 |
| `src-tauri/src/core/gameindex/scan.rs` | the scan learns that a directory can be a title | 3 |
| `src-tauri/src/core/gameindex/readers/lhadrawer.rs` | **created** — a drawer inside an archive | 4 |
| `src-tauri/src/core/gameindex/igame.rs` | `igame.data` beside a slave in a tree ART built | 5 |
| `src-tauri/src/core/gameindex/igamewrite.rs` | **created** — writing into the user's own collection, §92 | 6 |
| `src/i18n/{en,tr}.json`, `src/lib/gameindex.ts`, the Collection screen | the new verdicts and refusals | 1, 4, 6 |
| `scripts/` and the `#[ignore]`d hooks | the real-material bar | 7 |

---

## Task 1: Two media shapes, two endings

**Files:**
- Modify: `src-tauri/src/core/gameindex/record.rs:24` (`GAMEINDEX_SCHEMA`), and the `Media` enum at `:210`
- Modify: `src-tauri/src/commands/launch.rs:192-206` (`request_kind_from`)
- Modify: `src/i18n/en.json`, `src/i18n/tr.json`
- Test: inline in `record.rs` and `launch.rs`

**Interfaces:**
- Consumes: nothing
- Produces: `Media::WhdloadDrawer { dir: String, slave: String }`; `Media::WhdloadArchive { file: String, inner: String, slave: String }`; `GAMEINDEX_SCHEMA = 4`; a launch outcome for an archived title that is **not** `RequestKind::Whdload`

**Why two variants and not one with a location field.** [ART-147](../../ISSUES.md#fixed) is the whole reason. That defect folded two physical shapes into one record — `from_hardfile` recorded every self-booting hardfile as `Media::WhdloadDrawer` — which sent Play down the drawer path, asked for a system volume, and left the owner at a CLI concluding they had to install WHDLoad. They did not; the file boots itself. A single variant carrying `location: Dir | InArchive` reproduces that shape exactly: one missed `match` arm and an archived title silently takes the launchable path.

- [ ] **Step 1: Write the failing tests**

In `record.rs`'s `mod tests` (the schema and variant tests; the producer-discipline test in step 5 goes in `scan.rs`, where `from_hardfile` lives):

```rust
#[test]
fn the_two_drawer_shapes_are_two_variants() {
    let dir = Media::WhdloadDrawer {
        dir: "Games/Turrican".into(),
        slave: "Turrican.slave".into(),
    };
    let archived = Media::WhdloadArchive {
        file: "Demos.lha".into(),
        inner: "Demos/T/Tag".into(),
        slave: "Tag.Slave".into(),
    };
    assert_ne!(dir, archived);
    // The wire names are what a stored catalogue carries, so they are pinned.
    assert!(serde_json::to_string(&dir).unwrap().contains("\"whdload-drawer\""));
    assert!(serde_json::to_string(&archived).unwrap().contains("\"whdload-archive\""));
}

#[test]
fn the_schema_moved_so_an_older_catalogue_is_re_read() {
    // ART-147 shipped without this and the Collection screen came up saying
    // `unknown variant 'whdload-drawer'` instead of any title at all. The
    // name is back and an old catalogue's record of that name meant something
    // else, so the bump is what discards it before anything reads it.
    assert_eq!(GAMEINDEX_SCHEMA, 4);
}
```

In `launch.rs`'s test module:

```rust
#[test]
fn an_unpacked_drawer_launches_through_the_whdload_path() {
    let args = launch_args_with(Media::WhdloadDrawer {
        dir: "Games/Turrican".into(),
        slave: "Turrican.slave".into(),
    });
    match request_kind_from(&args) {
        RequestKind::Whdload { drawer, slave } => {
            assert_eq!(drawer, "Games/Turrican");
            assert_eq!(slave, "Turrican.slave");
        }
        other => panic!("a drawer is the one shape that path exists for, got {other:?}"),
    }
}

#[test]
fn an_archived_drawer_is_not_launchable_and_says_which_archive() {
    let args = launch_args_with(Media::WhdloadArchive {
        file: "WHDLoadDemos100.lha".into(),
        inner: "Demos/T/Tag".into(),
        slave: "Tag.Slave".into(),
    });
    let refusal = launch_refusal_for(&args)
        .expect("an archived title cannot be launched and must say so");
    assert!(
        refusal.contains("WHDLoadDemos100.lha"),
        "the refusal names the archive the user has to unpack, got: {refusal}"
    );
    assert!(
        !matches!(request_kind_from(&args), RequestKind::Whdload { .. }),
        "an archived title must never reach the launchable path - that is ART-147"
    );
}
```

`launch_args_with` and `launch_refusal_for` are test-local helpers you write; `launch_refusal_for` calls whatever the command layer already uses to turn an unlaunchable request into a user-facing sentence. If no such thing exists yet, add one — an archived title needs a sentence, and a `panic!` or a silent `RequestKind` is not one.

- [ ] **Step 2: Run them and watch them fail**

Run: `cd src-tauri && cargo test gameindex::record:: && cargo test launch::`
Expected: FAIL — `Media` has three variants.

- [ ] **Step 3: Add the variants**

In `record.rs`, on the `Media` enum (which is `#[serde(tag = "kind", rename_all = "kebab-case")]`):

```rust
    /// An **unpacked** WHDLoad drawer on the host: a directory holding exactly
    /// one slave, plus its icon, a ReadMe and a payload.
    ///
    /// Launches through [`RequestKind::Whdload`] — the drawer mounted as a
    /// directory beside a **separate bootable system volume** with WHDLoad in
    /// `C:`, which is where whdload.de's own installation page says it
    /// belongs. That is the difference from [`Media::WhdloadHardfile`], which
    /// boots itself and needs no system at all, and confusing the two is
    /// exactly [ART-147].
    ///
    /// **Produced only by the directory scan** (`readers::drawer`). The
    /// hardfile reader must never construct it; a test says so.
    WhdloadDrawer { dir: String, slave: String },

    /// A WHDLoad drawer **inside an archive ART has not unpacked**.
    ///
    /// Catalogued so a collection can be browsed without unpacking 663 MB, and
    /// **not launchable**: `RequestKind::Whdload` needs a directory on a
    /// filesystem, and this is a path inside a compressed file. Play answers
    /// with its own sentence — unpack it first, and here is the archive —
    /// rather than a refusal that reads like a broken install.
    ///
    /// **Produced only by the archive scan** (`readers::lhadrawer`).
    WhdloadArchive { file: String, inner: String, slave: String },
```

Bump `GAMEINDEX_SCHEMA` to `4` and extend its doc comment with the ART-147 sentence from the test above.

- [ ] **Step 4: Route each variant to its own ending**

`request_kind_from` gains a real arm for `WhdloadDrawer` — `RequestKind::Whdload { drawer, slave }` — and **must not** gain one for `WhdloadArchive` that reaches a launchable kind. Give the archived case its own refusal path with i18n keys in both catalogues, naming the archive and saying to unpack it. Do not add a catch-all `_ =>`: an exhaustive match is what makes the next variant safe to add.

- [ ] **Step 5: Pin the producer discipline**

ART-147's first half was a *hardfile* reader that assumed a drawer. The rule
that stops it happening again is that each variant has exactly one producer,
and the rule is a test rather than a comment:

```rust
#[test]
fn the_hardfile_reader_produces_neither_drawer_variant() {
    // ART-147: `from_hardfile` recorded self-booting hardfiles as
    // `WhdloadDrawer`, which sent Play looking for a system volume the file
    // never needed. A hardfile is never a drawer, whatever is inside it.
    let root = scratch("producer-discipline");
    let image = synthetic_whdload_hardfile(&root, "1000Miglia", "1000Miglia.Slave");
    let record = from_hardfile(&image).unwrap().expect("this is a title");
    assert!(
        matches!(record.media, Media::WhdloadHardfile { .. }),
        "a self-booting hardfile is WhdloadHardfile and nothing else, got {:?}",
        record.media
    );
}
```

`synthetic_whdload_hardfile` already has an equivalent in `readers::whdhdf`'s
own tests — reuse it rather than writing a second one.

- [ ] **Step 6: Run the tests**

Run: `cd src-tauri && cargo test gameindex:: && cargo test launch:: && pnpm test`
Expected: PASS.

- [ ] **Step 7: Mutations**

| Mutation | Test that must fail |
|---|---|
| route `WhdloadArchive` to `RequestKind::Whdload` | `an_archived_drawer_is_not_launchable_and_says_which_archive` |
| leave `GAMEINDEX_SCHEMA` at 3 | `the_schema_moved_so_an_older_catalogue_is_re_read` |
| rename the archive variant's wire tag | `the_two_drawer_shapes_are_two_variants` |
| make `from_hardfile` produce `WhdloadDrawer` | `the_hardfile_reader_produces_neither_drawer_variant` |

- [ ] **Step 8: Commit**

Message: `feat(gameindex): a WHDLoad drawer is two shapes, and only one of them launches`

---

## Task 2: A WHDLoad launch configuration, read out of an icon

**Files:**
- Modify: `src-tauri/src/core/whdload/mod.rs`
- Test: inline in `whdload/mod.rs`

**Interfaces:**
- Consumes: `core::amigaicon::tooltypes(&[u8]) -> CoreResult<Vec<String>>` (returns the raw array, in order)
- Produces: `whdload::LaunchOptions { slave: Option<String>, options: Vec<String> }`; `whdload::launch_options(tooltypes: &[String]) -> LaunchOptions`

**The measurement this task exists for.** whdload.de's manual says the Workbench slave comes from a `Slave` ToolType. The owner's real icon says `SLAVE=1001StolenIdeas.Slave` — upper case — so the key comparison must be case-insensitive. And **94 of that icon's 98 ToolTypes are `IM1=`/`IM2=` NewIcon image data**, following a line reading `*** DON'T EDIT THE FOLLOWING LINES!! ***`. Read naively, a "launch configuration" is mostly a picture.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn the_slave_comes_from_the_tooltype_whatever_its_case() {
    let tt = vec![
        "SLAVE=1001StolenIdeas.Slave".to_string(),
        "PRELOAD".to_string(),
    ];
    let got = launch_options(&tt);
    assert_eq!(got.slave.as_deref(), Some("1001StolenIdeas.Slave"));
    assert_eq!(got.options, vec!["PRELOAD".to_string()]);

    let lower = vec!["Slave=Turrican.slave".to_string()];
    assert_eq!(launch_options(&lower).slave.as_deref(), Some("Turrican.slave"));
}

#[test]
fn a_newicon_picture_is_not_a_launch_configuration() {
    // The real icon's shape: two real settings, the marker, then 94 image
    // lines. Reading past the marker turns a configuration into a picture.
    let mut tt = vec![
        "SLAVE=Tag.Slave".to_string(),
        "PRELOAD".to_string(),
        " ".to_string(),
        "*** DON'T EDIT THE FOLLOWING LINES!! ***".to_string(),
    ];
    for _ in 0..94 {
        tt.push("IM1=a\"$(0@a\"$(0@a\"$(0@".to_string());
    }
    let got = launch_options(&tt);
    assert_eq!(got.slave.as_deref(), Some("Tag.Slave"));
    assert_eq!(
        got.options,
        vec!["PRELOAD".to_string()],
        "everything from the marker on is image data, and the blank line is not a setting"
    );
}

#[test]
fn image_keys_are_dropped_even_without_the_marker() {
    // Belt and braces: an icon written by a different tool may carry the
    // image keys without the sentence in front of them.
    let tt = vec!["IM2=abc".to_string(), "SLAVE=X.slave".to_string()];
    let got = launch_options(&tt);
    assert_eq!(got.slave.as_deref(), Some("X.slave"));
    assert!(got.options.is_empty());
}

#[test]
fn an_icon_with_no_slave_tooltype_states_none() {
    // WHDLoad's own default is `WHDLoad.Slave` and `Slave=*` searches the
    // drawer - both are decisions for the caller, not guesses to make here.
    assert_eq!(launch_options(&["PRELOAD".to_string()]).slave, None);
}
```

- [ ] **Step 2: Run and watch them fail**

Run: `cd src-tauri && cargo test whdload:: -- launch_options`
Expected: FAIL — `launch_options` does not exist.

- [ ] **Step 3: Implement**

```rust
/// The marker WHDLoad's own installer writes before a NewIcon's image data.
const DONT_EDIT: &str = "DON'T EDIT";

/// What an install's icon says about how it expects to start.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchOptions {
    /// The `Slave` ToolType's value, verbatim. `None` when the icon does not
    /// name one — WHDLoad then defaults to `WHDLoad.Slave`, and `Slave=*`
    /// searches the drawer, but choosing between those is the caller's.
    pub slave: Option<String>,
    /// Every other real setting, in the icon's own order: `PRELOAD`, `NTSC`,
    /// `QuitKey=…` and the rest.
    pub options: Vec<String>,
}

pub fn launch_options(tooltypes: &[String]) -> LaunchOptions {
    let mut out = LaunchOptions::default();
    for entry in tooltypes {
        if entry.contains(DONT_EDIT) {
            break;
        }
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.split('=').next().unwrap_or("").trim();
        if key.eq_ignore_ascii_case("IM1") || key.eq_ignore_ascii_case("IM2") {
            continue;
        }
        if key.eq_ignore_ascii_case("SLAVE") {
            if let Some((_, value)) = trimmed.split_once('=') {
                out.slave = Some(value.trim().to_string());
            }
            continue;
        }
        out.options.push(trimmed.to_string());
    }
    out
}
```

Give the module doc the measurement: the manual's `Slave` against the material's `SLAVE=`, and 94 of 98.

- [ ] **Step 4: Run the tests**

Run: `cd src-tauri && cargo test whdload::`
Expected: PASS.

- [ ] **Step 5: Mutations**

| Mutation | Test that must fail |
|---|---|
| compare the key with `==` instead of `eq_ignore_ascii_case` | `the_slave_comes_from_the_tooltype_whatever_its_case` |
| drop the `DONT_EDIT` break | `a_newicon_picture_is_not_a_launch_configuration` |
| drop the `IM1`/`IM2` skip | `image_keys_are_dropped_even_without_the_marker` |
| default `slave` to `"WHDLoad.Slave"` | `an_icon_with_no_slave_tooltype_states_none` |

- [ ] **Step 6: Commit**

Message: `feat(whdload): read an install's launch options out of its icon`

---

## Task 3: A directory can be a title

**Files:**
- Create: `src-tauri/src/core/gameindex/readers/drawer.rs`
- Modify: `src-tauri/src/core/gameindex/readers/mod.rs`, `src-tauri/src/core/gameindex/scan.rs:132-149`
- Test: inline in `drawer.rs`

**Interfaces:**
- Consumes: Task 1's `Media::WhdloadDrawer`; Task 2's `whdload::launch_options`; `readers::slave` for the header; `core::amigaicon::tooltypes`
- Produces: `readers::drawer::read_drawer(dir: &Path) -> CoreResult<Option<GameRecord>>`; `readers::drawer::is_drawer(dir: &Path) -> bool`; `scan::collect_drawers(root: &Path) -> Vec<PathBuf>`

**The shape, measured.** 893 drawers in the owner's set, **one slave each**, at a uniform depth, alongside a `.info`, a `ReadMe` and a payload that is a bare file, a `Disk.N` image, or a `data/` subdirectory. iGame's own `examineFolder` skips directories named `data`/`Data`; so does this.

- [ ] **Step 1: Write the failing tests**

```rust
/// A synthetic drawer: one slave, an icon naming it, a ReadMe, a payload.
fn synthetic_drawer(root: &Path, name: &str, slave_file: &str) -> PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(dir.join("data")).unwrap();
    std::fs::write(dir.join(slave_file), build_slave(name)).unwrap();
    std::fs::write(dir.join(format!("{name}.info")), icon_naming(slave_file)).unwrap();
    std::fs::write(dir.join("ReadMe"), b"notes").unwrap();
    std::fs::write(dir.join("data").join("01"), b"payload").unwrap();
    dir
}

#[test]
fn a_directory_holding_one_slave_is_a_title() {
    let root = scratch("drawer-one");
    let dir = synthetic_drawer(&root, "Turrican", "Turrican.slave");
    let record = read_drawer(&dir).unwrap().expect("this is a title");
    match record.media {
        Media::WhdloadDrawer { dir: d, slave } => {
            assert!(d.ends_with("Turrican"));
            assert_eq!(slave, "Turrican.slave");
        }
        other => panic!("a drawer is a drawer, got {other:?}"),
    }
}

#[test]
fn a_slave_is_found_whatever_the_extensions_case() {
    let root = scratch("drawer-case");
    let dir = synthetic_drawer(&root, "Tag", "Tag.Slave");
    let record = read_drawer(&dir).unwrap().expect("`.Slave` is a slave");
    assert!(matches!(record.media, Media::WhdloadDrawer { .. }));
}

#[test]
fn a_payload_directory_is_not_a_title() {
    // `Demos/T/Tag/data/01`…`data/82` is payload. iGame skips `data`/`Data`
    // for the same reason, and a scan that descends into it invents titles.
    let root = scratch("drawer-payload");
    synthetic_drawer(&root, "Tag", "Tag.Slave");
    let found = collect_drawers(&root);
    assert_eq!(found.len(), 1, "one title, not one per payload directory");
    assert!(found[0].ends_with("Tag"));
}

#[test]
fn a_drawer_with_two_slaves_is_refused_by_name() {
    let root = scratch("drawer-two");
    let dir = synthetic_drawer(&root, "Ambiguous", "One.slave");
    std::fs::write(dir.join("Two.slave"), build_slave("Two")).unwrap();
    let err = read_drawer(&dir).expect_err("two slaves is not ART's to choose between");
    let text = err.to_string();
    assert!(
        text.contains("Ambiguous") && text.contains("One.slave") && text.contains("Two.slave"),
        "the refusal names the drawer and both candidates, got: {text}"
    );
}

#[test]
fn the_icons_slave_tooltype_settles_a_drawer_that_has_two() {
    // The one case where two slaves is answerable: the icon says which.
    let root = scratch("drawer-two-icon");
    let dir = synthetic_drawer(&root, "Decided", "One.slave");
    std::fs::write(dir.join("Two.slave"), build_slave("Two")).unwrap();
    std::fs::write(dir.join("Decided.info"), icon_naming("Two.slave")).unwrap();
    let record = read_drawer(&dir).unwrap().expect("the icon states it");
    match record.media {
        Media::WhdloadDrawer { slave, .. } => assert_eq!(slave, "Two.slave"),
        other => panic!("got {other:?}"),
    }
}

#[test]
fn a_directory_with_no_slave_is_not_a_title() {
    let root = scratch("drawer-none");
    std::fs::create_dir_all(root.join("Docs")).unwrap();
    std::fs::write(root.join("Docs").join("ReadMe"), b"x").unwrap();
    assert!(read_drawer(&root.join("Docs")).unwrap().is_none());
}
```

`icon_naming(slave)` builds a minimal Project icon whose ToolTypes are
`["SLAVE=<slave>", "PRELOAD"]` — reuse the `DiskObject` shape Task 2's tests
and `core::amigaicon`'s own test helpers already build by hand.

- [ ] **Step 2: Run and watch them fail**

Run: `cd src-tauri && cargo test gameindex::readers::drawer::`
Expected: FAIL — the module does not exist.

- [ ] **Step 3: Implement the reader**

`is_drawer` answers whether a directory holds at least one `#?.slave`, compared
case-insensitively. `read_drawer`:

1. lists the directory's own files (never recursing);
2. collects every `.slave`/`.Slave`;
3. **zero** → `Ok(None)` (not a title, not an error);
4. **one** → that is the slave;
5. **more than one** → read `<dir-name>.info`'s ToolTypes through
   `core::amigaicon::tooltypes` and `whdload::launch_options`; if its `slave`
   names one of the candidates, that settles it; otherwise refuse by name,
   listing the drawer and every candidate;
6. reads the slave header through `readers::slave` for the title, chipset and
   declared Kickstart, and falls back to the directory name as a **suggested**
   title when the header states none.

- [ ] **Step 4: Teach the scan that a directory can be a title**

`scan::collect_drawers(root)` walks with the same depth limit and symlink rule
`collect_indexable` uses, skipping any directory component named `data` or
`Data` case-insensitively, and returns every directory `is_drawer` accepts.
`collect_indexable` is left exactly as it is — a file scan and a directory scan
are two questions, and folding them is how one of them starts guessing.

- [ ] **Step 5: Run the tests**

Run: `cd src-tauri && cargo test gameindex::`
Expected: PASS.

- [ ] **Step 6: Mutations**

| Mutation | Test that must fail |
|---|---|
| compare the extension with `==".slave"` | `a_slave_is_found_whatever_the_extensions_case` |
| descend into `data`/`Data` | `a_payload_directory_is_not_a_title` |
| take the first slave when there are two | `a_drawer_with_two_slaves_is_refused_by_name` |
| ignore the icon's `SLAVE=` | `the_icons_slave_tooltype_settles_a_drawer_that_has_two` |
| return a record for a directory with no slave | `a_directory_with_no_slave_is_not_a_title` |

- [ ] **Step 7: Commit**

Message: `feat(gameindex): a directory holding one slave is a title`

---

## Task 4: A drawer inside an archive

**Files:**
- Create: `src-tauri/src/core/gameindex/readers/lhadrawer.rs`
- Modify: `src-tauri/src/core/gameindex/readers/mod.rs`
- Test: inline in `lhadrawer.rs`

**Interfaces:**
- Consumes: Task 1's `Media::WhdloadArchive`; `core::archive::open(path) -> Box<dyn ArchiveBackend>` with `entries() -> Vec<ArchiveEntry>` and `read(index, limit) -> Vec<u8>`; `readers::slave`
- Produces: `readers::lhadrawer::read_archive_drawers(path: &Path) -> CoreResult<Vec<GameRecord>>`

**Why this is cheap even on 663 MB.** LhA headers are sequential and each carries its packed size, so the backend's `entries()` walk seeks header to header; only the 893 `.slave` members are decompressed, at about a kilobyte each. Nothing decompresses the payload.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn every_drawer_in_an_archive_becomes_a_title() {
    let root = scratch("lha-drawers");
    // Two drawers, each with one slave and a payload, in one synthetic archive.
    let archive = synthetic_lha(
        &root,
        &[
            ("Demos/0-9/One/One.slave", build_slave("One")),
            ("Demos/0-9/One/data/01", b"payload".to_vec()),
            ("Demos/T/Tag/Tag.Slave", build_slave("Tag")),
            ("Demos/T/Tag/ReadMe", b"notes".to_vec()),
        ],
    );
    let found = read_archive_drawers(&archive).unwrap();
    assert_eq!(found.len(), 2);
    let inners: Vec<String> = found
        .iter()
        .map(|r| match &r.media {
            Media::WhdloadArchive { inner, .. } => inner.clone(),
            other => panic!("an archived drawer is WhdloadArchive, got {other:?}"),
        })
        .collect();
    assert!(inners.contains(&"Demos/0-9/One".to_string()));
    assert!(inners.contains(&"Demos/T/Tag".to_string()));
}

#[test]
fn an_archived_title_records_the_archive_it_came_from() {
    let root = scratch("lha-names");
    let archive = synthetic_lha(&root, &[("D/Tag/Tag.Slave", build_slave("Tag"))]);
    let found = read_archive_drawers(&archive).unwrap();
    match &found[0].media {
        Media::WhdloadArchive { file, inner, slave } => {
            assert!(file.ends_with(".lha"));
            assert_eq!(inner, "D/Tag");
            assert_eq!(slave, "Tag.Slave");
        }
        other => panic!("got {other:?}"),
    }
}

#[test]
fn a_payload_directory_inside_an_archive_is_not_a_title() {
    let root = scratch("lha-payload");
    let archive = synthetic_lha(
        &root,
        &[
            ("D/Tag/Tag.Slave", build_slave("Tag")),
            ("D/Tag/data/01", b"payload".to_vec()),
            ("D/Tag/data/02", b"payload".to_vec()),
        ],
    );
    assert_eq!(read_archive_drawers(&archive).unwrap().len(), 1);
}

#[test]
fn an_archive_with_no_slave_yields_no_titles() {
    let root = scratch("lha-empty");
    let archive = synthetic_lha(&root, &[("Docs/ReadMe", b"x".to_vec())]);
    assert!(read_archive_drawers(&archive).unwrap().is_empty());
}
```

`synthetic_lha` builds a real `.lha` at runtime. `core/lha`'s own test module
already writes archives by hand for its parser tests — reuse that helper rather
than shelling out to a packer, so the test runs with no external tool.

- [ ] **Step 2: Run and watch them fail**

Run: `cd src-tauri && cargo test gameindex::readers::lhadrawer::`
Expected: FAIL — the module does not exist.

- [ ] **Step 3: Implement**

Open the archive through `core::archive::open`, take `entries()`, and for every
entry whose name ends `.slave` case-insensitively **and** whose parent path
contains no `data`/`Data` component, read that member's bytes with a bound
(`read(index, MAX_SLAVE_BYTES)`), parse the header with `readers::slave`, and
build a `Media::WhdloadArchive` whose `inner` is the slave's parent path with
LhA's `0xFF` separator already normalised by `core/lha`'s own decoding.

A drawer inside an archive that holds two slaves takes the same answer Task 3
gives: refuse by name. There is no icon to consult without decompressing it,
and decompressing an icon to settle a case the material does not contain is
work for a case nobody has seen.

- [ ] **Step 4: Run the tests**

Run: `cd src-tauri && cargo test gameindex::`
Expected: PASS.

- [ ] **Step 5: Mutations**

| Mutation | Test that must fail |
|---|---|
| build `Media::WhdloadDrawer` here instead | `every_drawer_in_an_archive_becomes_a_title` |
| stop skipping `data` inside the archive | `a_payload_directory_inside_an_archive_is_not_a_title` |
| take the archive's own name as `inner` | `an_archived_title_records_the_archive_it_came_from` |
| read a member without a byte bound | no test — **say so in the report**; the bound is a rule, not a behaviour a test can see |

- [ ] **Step 6: Commit**

Message: `feat(gameindex): catalogue the drawers inside an archive without unpacking it`

---

## Task 5: `igame.data` beside a slave, in a tree ART built

**Files:**
- Modify: `src-tauri/src/core/gameindex/igame.rs`
- Modify: wherever `core::whdload`'s install lays a pack down
- Test: inline

**Interfaces:**
- Consumes: `igame::{IGameData, render, merge_into, FILE_NAME, LINE_BYTES}`, all built
- Produces: `igame::write_beside(slave_dir: &Path, data: &IGameData) -> CoreResult<Rendered>`

**This is the default path and it needs no ceremony**, because the tree is one ART just made. `core::whdload` already works out what inside an archive is the pack and carries the sibling drawer icon that keeps the result visible on Workbench; this adds one file beside the slave.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn igame_data_lands_beside_the_slave() {
    let root = scratch("igame-beside");
    let dir = root.join("Games/Turrican");
    std::fs::create_dir_all(&dir).unwrap();
    let data = IGameData { title: Some("Turrican".into()), ..Default::default() };
    write_beside(&dir, &data).unwrap();
    let text = std::fs::read_to_string(dir.join(FILE_NAME)).unwrap();
    assert!(text.contains("title=Turrican"));
}

#[test]
fn an_existing_file_is_edited_and_its_own_keys_survive() {
    // The FF.CFG rule: somebody may have curated theirs by hand, and iGame
    // silently ignores keys it does not know.
    let root = scratch("igame-merge");
    let dir = root.join("Games/Tag");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(FILE_NAME), "; mine\nfavourite=yes\ntitle=Old\n").unwrap();
    let data = IGameData { title: Some("Tag".into()), ..Default::default() };
    write_beside(&dir, &data).unwrap();
    let text = std::fs::read_to_string(dir.join(FILE_NAME)).unwrap();
    assert!(text.contains("; mine"), "a comment is not ART's to delete");
    assert!(text.contains("favourite=yes"), "an unknown key is not ART's to delete");
    assert!(text.contains("title=Tag"));
    assert!(!text.contains("title=Old"));
}

#[test]
fn a_value_that_will_not_fit_is_left_out_and_named() {
    let root = scratch("igame-long");
    let dir = root.join("Games/Long");
    std::fs::create_dir_all(&dir).unwrap();
    let data = IGameData { title: Some("x".repeat(200)), ..Default::default() };
    let rendered = write_beside(&dir, &data).unwrap();
    assert!(
        rendered.omitted.iter().any(|o| o.key == "title"),
        "a truncated title is a wrong title on the Amiga's screen"
    );
    let text = std::fs::read_to_string(dir.join(FILE_NAME)).unwrap();
    assert!(!text.contains("title="));
}
```

- [ ] **Step 2: Run and watch them fail**

Run: `cd src-tauri && cargo test gameindex::igame::`
Expected: FAIL — `write_beside` does not exist.

- [ ] **Step 3: Implement**

`write_beside` reads an existing `igame.data` when there is one and calls
`merge_into`, otherwise calls `render`, and writes through
`core::safety::atomic_write` — a truncated `igame.data` is one iGame reads
half of. Return the `Rendered` so the caller can report what was omitted.

Wire it into `core::whdload`'s install so a pack laid onto a card gets its
`igame.data` from the catalogue record that named it.

- [ ] **Step 4: Run the tests**

Run: `cd src-tauri && cargo test gameindex:: && cargo test whdload::`
Expected: PASS.

- [ ] **Step 5: Mutations**

| Mutation | Test that must fail |
|---|---|
| call `render` even when a file exists | `an_existing_file_is_edited_and_its_own_keys_survive` |
| truncate an over-long value instead of omitting it | `a_value_that_will_not_fit_is_left_out_and_named` |
| write with `std::fs::write` instead of `atomic_write` | no test — **disclose it**; the guard is the rule, and `scripts/` has no sweep for it |

- [ ] **Step 6: Commit**

Message: `feat(gameindex): write igame.data beside a slave in a tree ART built`

---

## Task 6: Writing into the user's own collection

**Files:**
- Create: `src-tauri/src/core/gameindex/igamewrite.rs`
- Modify: `src-tauri/src/commands/` (a command for it), `src/lib/gameindex.ts`, `src/i18n/{en,tr}.json`, the Collection screen
- Test: inline plus a frontend test

**Interfaces:**
- Consumes: Task 5's `write_beside`; Task 1's variants
- Produces: `igamewrite::plan(titles: &[GameRecord]) -> IGamePlan`; `igamewrite::apply(plan: &IGamePlan, progress: &dyn ProgressSink) -> IGameOutcome` with `Vec<IGameVerdict { dir, state }>` and `state ∈ { Written, Merged, Skipped(reason), Failed(detail) }`

**This is the explicit half**, and it goes through the mandatory pipeline: SOURCE → ANALYZE → VALIDATE → RECOMMEND → PREVIEW → BACKUP → APPLY → VERIFY → REPORT. 893 drawers is **893 results**, not one number.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn an_archived_title_is_refused_and_the_refusal_names_the_archive() {
    let record = archived_record("WHDLoadDemos100.lha", "Demos/T/Tag", "Tag.Slave");
    let plan = plan(&[record]);
    let refusal = plan.refusals.first().expect("an archive cannot be written into");
    assert!(
        refusal.contains("WHDLoadDemos100.lha") && refusal.to_lowercase().contains("unpack"),
        "the refusal names the archive and says what to do, got: {refusal}"
    );
    assert!(plan.items.is_empty(), "nothing may be planned against an archive");
}

#[test]
fn every_drawer_gets_its_own_verdict() {
    let root = scratch("igamewrite-many");
    let a = synthetic_drawer(&root, "One", "One.slave");
    let b = synthetic_drawer(&root, "Two", "Two.slave");
    let outcome = apply(&plan(&[drawer_record(&a), drawer_record(&b)]), &NoProgress);
    assert_eq!(outcome.verdicts.len(), 2, "two drawers is two results, never one number");
    assert!(outcome.verdicts.iter().all(|v| matches!(v.state, IGameState::Written)));
}

#[test]
fn a_backup_is_taken_before_an_existing_file_is_changed() {
    let root = scratch("igamewrite-backup");
    let dir = synthetic_drawer(&root, "Kept", "Kept.slave");
    std::fs::write(dir.join(FILE_NAME), "favourite=yes\n").unwrap();
    let outcome = apply(&plan(&[drawer_record(&dir)]), &NoProgress);
    let verdict = &outcome.verdicts[0];
    assert!(matches!(verdict.state, IGameState::Merged));
    assert!(
        verdict.backup.is_some(),
        "the user is told where their previous version went"
    );
}

#[test]
fn one_failure_does_not_stop_the_rest() {
    // A host filesystem has no journal: nine written and one failed is nine
    // completed operations, and the report says so per entry.
    let root = scratch("igamewrite-partial");
    let ok = synthetic_drawer(&root, "Fine", "Fine.slave");
    let bad = unwritable_drawer(&root, "Locked", "Locked.slave");
    let outcome = apply(&plan(&[drawer_record(&ok), drawer_record(&bad)]), &NoProgress);
    assert_eq!(outcome.verdicts.len(), 2);
    assert!(outcome.verdicts.iter().any(|v| matches!(v.state, IGameState::Written)));
    assert!(outcome.verdicts.iter().any(|v| matches!(v.state, IGameState::Failed(_))));
}
```

- [ ] **Step 2: Run and watch them fail**

Run: `cd src-tauri && cargo test gameindex::igamewrite::`
Expected: FAIL — the module does not exist.

- [ ] **Step 3: Implement**

`plan` turns records into items, refusing every `WhdloadArchive` by name.
`apply` walks the items, takes a backup through `core::safety::guarded_write`
when a file already exists, calls Task 5's `write_beside`, and records one
verdict per entry with the backup path when there is one. It checks
`is_cancelled()` **between** entries — each drawer is a whole unit of work —
and reports through the `ProgressSink` so a job can show `N of M`.

Every verdict reaches the screen, and every state has its own sentence in both
i18n catalogues. A new `RefusalReason` variant goes into
`phrase-keys.test.ts`'s list, or the guard for it does not exist.

- [ ] **Step 4: Run the tests**

Run: `cd src-tauri && cargo test gameindex:: && pnpm test && pnpm lint`
Expected: PASS.

- [ ] **Step 5: Mutations**

| Mutation | Test that must fail |
|---|---|
| plan an archived title instead of refusing it | `an_archived_title_is_refused_and_the_refusal_names_the_archive` |
| collapse the verdicts into a count | `every_drawer_gets_its_own_verdict` |
| skip the backup | `a_backup_is_taken_before_an_existing_file_is_changed` |
| abort the whole run on the first failure | `one_failure_does_not_stop_the_rest` |

- [ ] **Step 6: Commit**

Message: `feat(gameindex): write igame.data into a collection, previewed and per entry`

---

## Task 7: The real-material bar, and the documents

**Files:**
- Modify: `src-tauri/src/core/gameindex/readers/drawer.rs` (an `#[ignore]`d hook)
- Modify: `docs/session-log.md`, `docs/STATUS.md`, `docs/ISSUES.md`, `docs/FEATURES.md`, `CHANGELOG.md`, `docs/superpowers/specs/2026-09-04-work-list.md`

**Interfaces:**
- Consumes: every task above
- Produces: nothing downstream

- [ ] **Step 1: Add the hook**

```rust
#[test]
#[ignore = "needs an unpacked WHDLoad drawer collection"]
fn catalogue_a_real_drawer_collection_when_asked() {
    let Ok(root) = std::env::var("ART_DRAWERS") else { return };
    let found = collect_drawers(Path::new(&root));
    println!("ART_DRAWERS_RESULT drawers={}", found.len());
    let mut read = 0usize;
    for dir in &found {
        if read_drawer(dir).unwrap().is_some() {
            read += 1;
        }
    }
    println!("ART_DRAWERS_READ read={read}");
    assert_eq!(read, found.len(), "every drawer the scan found must read back");
}
```

- [ ] **Step 2: Run it for real**

Unpack `E:\amiga\Amigatolon\paketler\WHDLoadDemos100.lha` into a scratch folder
under `E:\amiga\ProjeART` — **never into `E:\amiga\Amigatolon`, which is the
owner's own material and is read from, never written to** — then:

```bash
cd src-tauri && ART_DRAWERS="E:\amiga\ProjeART\whdload-drawers\Demos" \
  cargo test catalogue_a_real_drawer_collection_when_asked -- --nocapture --ignored
```

**Expected: 893 drawers, 893 read back.** The design rests on that number; if
it differs, **that is the finding** — report it exactly and do not adjust the
expectation to match.

- [ ] **Step 3: Full verification**

```bash
cd src-tauri && cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings
pnpm lint && pnpm test
python scripts/control-byte-sweep.py
python scripts/scratch-counter-sweep.py
```

- [ ] **Step 4: Update the five documents**

`docs/session-log.md` (a row at the top), `docs/STATUS.md` (the snapshot
numbers, re-measured rather than carried forward, and the "Picking up next
session" block **updated in place**), `docs/ISSUES.md` (anything the real run
found), `docs/FEATURES.md` (flip rows only where a test exists), `CHANGELOG.md`
(the user-visible change), and close **item 5** in
`docs/superpowers/specs/2026-09-04-work-list.md` in place, recording that the
entry's own two routes were both about the wrong material.

- [ ] **Step 5: Commit**

Message: `docs: record the WHDLoad drawer round`

---

## Self-review notes

- **Spec coverage.** §1 → Tasks 3, 4 and 7 (the measurement is the hook's
  expectation). §2 → Task 2. §3 → Task 1. §4 → Tasks 3 and 4. §5 → Task 3's
  step 3, item 6. §6 → Tasks 5 and 6. §7 → Task 1 (the schema and the producer
  discipline). §8 → every task's mutation table plus Task 7. §9 is the
  "deliberately not done" list and needs no task: no icon is written anywhere
  in this plan, nothing unpacks, and the 1 697 hardfiles are untouched.
- **The producer-discipline test §7 asks for** was missing when this plan was
  first written — it lived in Task 1's mutation table by implication only. It
  is now Task 1 step 5, with its own mutation. Recorded rather than quietly
  fixed, because a plan that only implies a guard is how a guard goes missing.
- **What this plan cannot promise.** Task 4's archive scan is written against a
  synthetic `.lha`; the owner's real archive is 663 MB and 8 858 entries, and
  the only thing that tests it at that size is running the scan over it. That
  is not in Task 7's hook, because the hook takes an unpacked tree. Whoever
  runs Task 7 should point the archive scan at the real `.lha` once by hand and
  report the count and the time it took.
