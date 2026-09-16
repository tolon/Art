# One-button card — Round 2: content sources, LZX, restored names, attributes

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The core of the card's content step — `core/card/content.rs` classifies, measures and prepares a source (folder · archive `.lha/.zip/.7z/.lzx` · WHDLoad HDF · ADF); ART reads Amiga LZX with its own decoder; names Windows cannot hold and Amiga protection bits, comments and dates reach the card; a partition takes several sources in one copy step.

**Architecture:** Bottom-up, each task green on its own. The archive entry grows an Amiga attribute record every backend fills; a new `core/archive/lzx.rs` backend sits behind the same gate; HDF extraction records escaped names as (host path, Amiga name) pairs and writes directory sidecars; the native PFS3 copy carries dates and (through a vendored `libpfs3` patch) comments, and hst-imager is told to read `.uaem`; `preload::amiga_names` reads an ART-private names record; `content.rs` places archive entries (plain · single WHDLoad pack · collection), measures without unpacking, and prepares into a caller's staging folder through the one archive gate; `preload` copies a list of sources as one step, refusing collisions before the format.

**Tech Stack:** Rust (MSRV 1.93) in `src-tauri/src/core` — `delharc` 0.8, `zip` 8.6, `sevenz-rust2` 0.21.4, vendored `libpfs3` 0.1.3+art.11 — no new crate. TypeScript only where `PreloadPartition`'s serde shape changes (`src/lib/preload.ts`).

**Spec:** `docs/superpowers/specs/2026-09-11-one-button-card-design.md` (§ 4, § 6, § 7, § 8.2 are this round).
**Research (binding unless the owner's decisions override):**
`docs/superpowers/notes/2026-09-16-card-round-2-research.md` (R1),
`docs/superpowers/notes/2026-09-16-card-round-2-lzx-and-names.md` (R2),
`docs/superpowers/notes/2026-09-16-card-round-2-attributes.md` (R3).

## What this round satisfies, and what it deliberately does not

| Item | Where |
|---|---|
| R1-1 archive gate is `archive::open` + `extract_selection`, not `unpack_for_install` | Task 9 |
| R1-2 "LZX is a refusal" | **overridden** by the owner's decision 6 — Tasks 2, 3 |
| R1-3 pack vs collection decided before unpacking | Tasks 8, 9 |
| R1-4 WHDLoad HDF via `read_whdload_hardfile` + `extract_from_volume` + `extract_file_on` | Tasks 8, 9 |
| R1-5 escaped names from an HDF not ignored | Tasks 4, 7, 9 |
| R1-6 names measured with preload's own rules, over-long a refusal | Task 8 |
| R1-7 one copy step carrying the source list | Task 10 |
| R1-8 collisions across sources refused before the format | Tasks 8, 10 |
| R1-9 stale `sizing.rs` citations | Task 6 |
| R1-10 owner-material hook (Prehistoric Tale, B-17, a subset of WHDLoadDemos100) | Task 11 |
| R2-1…3, R2-6 LZX decoder, merged groups once, bounds, CRCs, Latin-1, implied dirs | Tasks 2, 3 |
| R2-4 attributes for the whole gate | Tasks 1, 9 (R3) |
| R2-5 no LZX dates | Task 2 (`date: None`) |
| R2-7 `scripts/lzx-oracle-check.py` | Task 11 |
| R2-8 LZX content measured with preload's rules | Tasks 8, 11 |
| R2-9 escaped names travel (pair record, private file, read by native and hst-imager) | Tasks 4, 7, 9 |
| R2-10 HDF names colliding after escaping refused; non-ASCII **and** escaped refused naming both | Tasks 8, 9 |
| R2-11 directory `.uaem` in `extract_dir` | Task 4 |
| R3-1…8 attribute sources, dates, sidecars, PFS3/FFS/hst-imager consumption, `\` in level-0 names | Tasks 1, 4, 5, 6, 7, 8, 9 |

**Deliberately not in this round** (owner's decision 1): the WHDLoad phase, `card_os_prepare`/`card_os_build`, `.partial`, `run_with_fallback` reachable from a card build, the end-to-end card (design § 8.3) — round 3; every screen — round 4. Also not here: `unpack_for_install` stays LHA-only (content.rs does not use it; nothing else changes); `archive::extract_with_backend`'s other callers (OS install, package volumes, layout) keep writing Windows-reserved names as they do today — Task 9 escapes only on the card route, and Task 12 records the rest if Task 9's red step proves the defect; `StagedTree` (Amiga→Amiga) escaped names were not examined; a Latin-1 `build_dir_entry` for names (ART-113) stays out of scope.

## Global Constraints

- `src-tauri/src/core/` is platform-independent: `std`, `serde`, `serde_json`, `sha2`, `log`, `thiserror`, `delharc`, `zip`, `sevenz-rust2`, `quick-xml`, `fatfs`, `libpfs3` — **no new crate** this round; never `use tauri`, no network, no process spawn outside a test.
- A lower-level `core/` module never imports a higher one: `core/archive` does not import `core/volume` or `core/card`; `core/preload` does not import `core/card`. `core/card/content.rs` is the top of this round and may import all of them.
- `vendor/libpfs3`: a change updates `vendor/libpfs3/ART-PATCH.md` **in the same commit** and the changed file's header comment; the crate version in `vendor/libpfs3/Cargo.toml` goes to `0.1.3+art.12` and `cargo update -p libpfs3 --precise 0.1.3+art.12` is run so `Cargo.lock` follows.
- Test scratch: `let (_guard, dir) = crate::core::ScratchDir::pair("<prefix>", "<tag>");` — `_guard`, never `_`, tuple guard first; a struct `_guard` field last. Names unique within one process (the module's existing `scratch()` helper, or a process-wide atomic counter).
- Fixtures are synthetic and built at runtime (ART's own FFS writer, hand-built LZX/LHA bytes, `zip::ZipWriter`, `sevenz_rust2::ArchiveWriter`). **No Amiga content in the repository.** Owner material only in `#[ignore]` hooks gated by an env var.
- Every shell: `TMP` and `TEMP` on `E:\amiga\ProjeART\build\tmp`; never write to `C:`. Owner material under `E:\amiga\` is read-only.
- Run commands from `D:\Projeler\Amiga\amiga-retro-toolkit\src-tauri` (Rust) or the repository root (pnpm, scripts), by absolute `cd`. Per task: `cargo test --lib <module path>` only. The whole suite, twice, only in Task 12.
- A test run is finished when it prints `test result:`. **Never pipe** a command whose status matters; redirect to a file under `E:\amiga\ProjeART\build\tmp\card-r2\` and read the `test result:` line.
- Red first: every new test is seen failing for the stated reason before the implementation; every guard that matters gets a **mutation** (back up the file by absolute path first, mutate, run, restore with `shutil.copyfile`, never `git checkout --`), and survivors are disclosed in the commit message.
- Commits: `git branch --show-current` must print `art-card-round-2` first; stage files **by name** (never `git add -A`); message written to a file and `git commit -F <file>`; end with `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`.
- Creating a file is `SAFE_CREATE`: refuse when the target exists. User data is never destroyed; staging is ART's, but content.rs still refuses a non-empty staging folder.
- Never index a disk image or a buffer built from file data directly: `get(..)`/`get_mut(..)` with a `Malformed` error, `checked_*` arithmetic on header values, never size an allocation from a header value.
- Paths in scripts are written with the Write tool, never through a heredoc.
- i18n: `src/lib` never renders; no catalogue key is added this round (no UI). If one turns out to be needed, both catalogues change in the same commit.
- New `CoreError` variants get a distinct `code()` and join `every_variant_has_a_distinct_code` (`src-tauri/src/core/error.rs:743`).

## File structure

| File | Responsibility this round |
|---|---|
| `src-tauri/src/core/hashing.rs` (modify) | `Crc32` (table, incremental) and `crc32_ieee` |
| `src-tauri/src/core/rom/mod.rs:859` (modify) | `compute_crc32` delegates to `hashing::crc32_ieee` |
| `src-tauri/src/core/archive/mod.rs` (modify) | `AmigaAttributes`, `EntryDate`, `ArchiveEntry.amiga`; `pub mod lzx`; `"lzx"` dispatch |
| `src-tauri/src/core/archive/lha.rs`, `zip.rs`, `sevenz.rs` (modify) | fill `amiga` |
| `src-tauri/src/core/archive/lzx.rs` (create) | Amiga LZX backend: container, groups, stored and compressed blocks |
| `src-tauri/src/core/detect.rs` (modify) | `LZX` signature → `format_hint "lzx"` |
| every other `ArchiveEntry { .. }` literal (modify) | `amiga: AmigaAttributes::default()` |
| `src-tauri/src/core/volume/write/copy.rs` (modify) | `EscapedName`, `ExtractReport.escaped`, directory sidecars |
| `src-tauri/vendor/libpfs3/src/writer.rs`, `src/error.rs`, `Cargo.toml`, `ART-PATCH.md` (modify) | `Writer::set_entry_comment`, `Error::CommentTooLong` |
| `src-tauri/src/core/preload/native.rs` (modify) | PFS3 carries date + comment; FFS carries comment; `pub(crate)` name helpers; `copy_in_sources` |
| `src-tauri/src/core/card/sizing.rs` (modify) | comment bytes in `entry_bytes`; `add_file_with_comment`, `add_directory_with_comment`; citations |
| `src-tauri/src/core/preload/amiga_names.rs` (modify) | `AMIGA_NAMES_RECORD`, `write_record`, `read` of both inputs |
| `src-tauri/src/tools/hst_imager.rs` (modify) | `--uaemetadata UaeMetafile`; `copy_in_sources` |
| `src-tauri/src/core/gameindex/readers/lhadrawer.rs` (modify) | `pub(crate) fn slave_drawers` factored out |
| `src-tauri/src/core/gameindex/readers/whdhdf.rs` (modify) | `pub(crate) fn fs_type_of` factored out |
| `src-tauri/src/core/volume/write/uaem.rs` (modify) | `days_from_civil` → `pub(crate)` |
| `src-tauri/src/core/card/content.rs` (create) | classify · place · measure · check · prepare |
| `src-tauri/src/core/card/mod.rs` (modify) | `pub mod content;` |
| `src-tauri/src/core/error.rs` (modify) | `SourceNamesCollide`, `EscapedNamesCollide`, `NamesNoWriterCanCopy` |
| `src-tauri/src/core/preload/mod.rs` (modify) | `PreloadPartition.content: Vec<PathBuf>`, `PreloadStep::CopyIn { sources }`, trait `*_sources`, `check_source_collisions`, `amiga_fold` |
| `src-tauri/src/commands/preload.rs` (modify) | the three `CopyIn` sites; test literals |
| `src/lib/preload.ts`, `src/lib/preload.test.ts` (modify) | `content: string[]`, `sources: string[]` |
| `scripts/lzx-oracle-check.py` (create) | ART vs `unar -e ISO-8859-1`, names + SHA-256 + counts |
| docs | ISSUES, FEATURES, STATUS, session-log, CHANGELOG, THIRD_PARTY/CLAUDE.md only if touched |

**Order, and why it differs from the suggested one.** The attribute record goes **first** (Task 1) because the LZX backend fills it — building LZX first would mean writing `ArchiveEntry` twice. The PFS3/FFS consumption (Task 5) and sizing (Task 6) come before `content.rs` because `content.rs` measures comment bytes and its tests read attributes back from a written volume. The names record (Task 7) comes before `content.rs` for the same reason.

---

### Task 1: CRC-32 in one place, and an Amiga attribute record on every archive entry

**Files:**
- Modify: `src-tauri/src/core/hashing.rs` (add after `crc16_arc`, `:96`), `src-tauri/src/core/rom/mod.rs:858-872`
- Modify: `src-tauri/src/core/archive/mod.rs:36-50` (types), `src-tauri/src/core/archive/lha.rs:115-135`, `src-tauri/src/core/archive/zip.rs:147-165`, `src-tauri/src/core/archive/sevenz.rs:68-83`
- Modify (add `amiga: AmigaAttributes::default()`): `core/amigainstall/packagevol.rs:715`, `core/archive/extract.rs:430`, `core/archive/tree.rs:250,258`, `core/osinstall/source_archive.rs:260` — and any other site the compiler names.
- Modify: `src-tauri/src/core/lha/mod.rs` tests — `level0_entry` becomes `pub(crate) fn` so `archive/lha.rs` tests can use it.
- Test: inline `#[cfg(test)]` in `hashing.rs`, `archive/lha.rs`, `archive/zip.rs`, `archive/sevenz.rs`

**Interfaces:**
- Produces:
  - `pub struct Crc32` with `new() -> Self`, `update(&mut self, &[u8])`, `finish(&self) -> u32`; `pub fn crc32_ieee(bytes: &[u8]) -> u32` in `core::hashing`.
  - In `core::archive`:
    ```rust
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct AmigaAttributes {
        /// `HSPARWED` as a header block stores it — RWED inverted, `uaem::parse_bits`' form.
        pub protection: Option<u32>,
        /// The file note, Latin-1 decoded; `None` when absent or empty.
        pub comment: Option<String>,
        pub date: Option<EntryDate>,
    }
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum EntryDate {
        /// MS-DOS date (high word) and time (low word): wall-clock time, no zone.
        MsDos(u32),
        /// Seconds since 1970, UTC.
        Unix(i64),
    }
    pub struct ArchiveEntry { pub name: String, pub is_dir: bool, pub declared_bytes: u64, pub amiga: AmigaAttributes }
    ```
  - `pub(crate) fn zip_amiga_protection(external: u32) -> u32` (zip.rs), `pub(crate) fn filetime_to_unix(ft: u64) -> i64` (sevenz.rs), `fn lha_amiga_bits(header: &delharc::LhaHeader, archive: &Path) -> bool` (lha.rs).

- [ ] **Step 1: Failing tests.** In `hashing.rs` tests:

```rust
    #[test]
    fn crc32_matches_the_standard_vector_whole_and_in_pieces() {
        assert_eq!(crc32_ieee(b""), 0);
        assert_eq!(crc32_ieee(b"123456789"), 0xCBF4_3926);
        let mut split = Crc32::new();
        split.update(b"1234");
        split.update(b"56789");
        assert_eq!(split.finish(), 0xCBF4_3926);
    }
```

In `archive/lha.rs` tests (R3 § 1; the helpers are `crate::core::lha::tests::make_level1_lha` — attr `0x20`, OS `A`, DOS date `(45<<9)|(1<<5)|1`, time 0 — and `level0_entry` — attr `0x20`, no OS id):

```rust
    /// R3 § 1: an Amiga LhA's attribute byte is the FIB protection byte and is
    /// carried as it is; its date is MS-DOS wall-clock bits.
    #[test]
    fn an_amiga_level_one_entry_carries_its_protection_byte_and_dos_date() {
        let (_guard, dir) = scratch("lha-attrs-l1");
        let path = dir.join("pack.lha");
        std::fs::write(&path, crate::core::lha::tests::make_level1_lha(b"C", b"Assign", b"x")).unwrap();
        let entries = LhaBackend::open(&path).unwrap().entries().unwrap();
        let dos = (((45u32 << 9) | (1 << 5) | 1) << 16) | 0;
        assert_eq!(entries[0].amiga.protection, Some(0x20));
        assert_eq!(entries[0].amiga.date, Some(EntryDate::MsDos(dos)));
    }

    /// XADMaster's guess (R3 § 1): a level-0 header without an OS id is Amiga
    /// when the archive is called `.lha`/`.run`, MS-DOS otherwise — so the same
    /// bytes carry bits under one name and none under the other.
    #[test]
    fn a_level_zero_entry_without_an_os_id_is_amiga_only_by_the_archive_name() {
        let (_guard, dir) = scratch("lha-attrs-l0");
        let mut bytes = crate::core::lha::tests::level0_entry(b"README", b"x");
        bytes.push(0);
        let lha = dir.join("a.lha");
        let lzh = dir.join("a.lzh");
        std::fs::write(&lha, &bytes).unwrap();
        std::fs::write(&lzh, &bytes).unwrap();
        assert_eq!(LhaBackend::open(&lha).unwrap().entries().unwrap()[0].amiga.protection, Some(0x20));
        assert_eq!(LhaBackend::open(&lzh).unwrap().entries().unwrap()[0].amiga.protection, None);
    }
```

(Adapt `scratch(..)` to the module's existing scratch helper; if `archive/lha.rs` has none, use `crate::core::ScratchDir::pair("art-archive-lha", tag)`.)

In `archive/zip.rs` tests — the rule is UnZip's `amiga/amiga.c:181-187` plus `^ 0x0F` (R3 § 2):

```rust
    #[test]
    fn an_amiga_host_s_attributes_become_fib_protection_bits() {
        // Info-ZIP: `----rwed` stored set-means-granted, read-only bit clear.
        assert_eq!(zip_amiga_protection(0x000F_0000), 0x00);
        // PKAZip: FIB bits stored as they are (0 = `----rwed`), read-only bit clear.
        assert_eq!(zip_amiga_protection(0x0000_0000), 0x00);
        // Info-ZIP `-s--rwed`: S set, RWED granted.
        assert_eq!(zip_amiga_protection(0x004F_0000), 0x40);
        // Info-ZIP `----rw-d` (E not granted): high 0x0D, read-only bit clear.
        assert_eq!(zip_amiga_protection(0x000D_0000), 0x02);
        // The archive bit is cleared on the way out, as UnZip does.
        assert_eq!(zip_amiga_protection(0x001F_0000), 0x00);
    }

    /// A ZIP made on a PC carries no Amiga bits and no comment claim, only its date.
    #[test]
    fn a_pc_made_zip_entry_has_a_date_and_no_amiga_bits() {
        let (_guard, dir) = scratch("zip-attrs-pc");
        let path = dir.join("pc.zip");
        {
            let file = std::fs::File::create(&path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            writer.start_file("Readme", zip::write::SimpleFileOptions::default()).unwrap();
            std::io::Write::write_all(&mut writer, b"x").unwrap();
            writer.finish().unwrap();
        }
        let entries = ZipBackend::open(&path).unwrap().entries().unwrap();
        assert_eq!(entries[0].amiga.protection, None);
        assert_eq!(entries[0].amiga.comment, None);
        assert!(matches!(entries[0].amiga.date, Some(EntryDate::MsDos(_))));
    }
```

In `archive/sevenz.rs` tests:

```rust
    #[test]
    fn a_filetime_becomes_unix_seconds() {
        // 2025-01-01T00:00:00Z
        assert_eq!(filetime_to_unix(133_801_632_000_000_000), 1_735_689_600);
    }

    #[test]
    fn a_7z_entry_never_claims_amiga_bits_or_a_comment() {
        let (_guard, dir) = scratch("7z-attrs");
        let path = dir.join("a.7z");
        std::fs::write(&path, make_7z_with(&[("C/Assign", b"x")])).unwrap();
        let entries = SevenZBackend::open(&path).unwrap().entries().unwrap();
        assert_eq!(entries[0].amiga.protection, None);
        assert_eq!(entries[0].amiga.comment, None);
    }
```

- [ ] **Step 2: Run red.** `cargo test --lib core::hashing` and `cargo test --lib core::archive` → compile errors (`Crc32`, `amiga`, `EntryDate`, the three functions not found). That is the red for new API; after Step 3's *types only* (fields defaulted everywhere), run again and see the attribute tests fail on `None != Some(0x20)` etc.

- [ ] **Step 3: Implement.**
  - `hashing.rs`: a `const CRC32_TABLE: [u32; 256]` built with a `while` loop over the reflected polynomial `0xEDB8_8320`; `Crc32(u32)` starting `0xFFFF_FFFF`, `update` = `crc = TABLE[((crc ^ b) & 0xFF) as usize] ^ (crc >> 8)`, `finish` = `!crc`. `rom::compute_crc32` body becomes `crate::core::hashing::crc32_ieee(bytes)` (its own test `crc32_empty_and_known_string` proves the delegate).
  - `archive/mod.rs`: the two types above, `amiga` on `ArchiveEntry`, doc comment citing R3.
  - `lha.rs` `entries()`: use `crate::core::lha::entry_name(header)` (not `entry_path`) to keep the comment; then
    ```rust
    let amiga_bits = lha_amiga_bits(header, &self.path);
    let date = match header.level {
        0 | 1 => Some(EntryDate::MsDos(header.last_modified)),
        2 => Some(EntryDate::Unix(i64::from(header.last_modified))),
        _ => None,
    };
    amiga: AmigaAttributes {
        protection: amiga_bits.then(|| u32::from(header.msdos_attrs.bits() as u8)),
        comment: (!name.comment.is_empty()).then(|| name.comment.clone()),
        date,
    },
    ```
    with
    ```rust
    /// XADMaster `XADLZHParser.m`: the OS id decides; a level-0 header with none is
    /// Amiga when the archive is named `.lha`/`.run` (R3 § 1).
    fn lha_amiga_bits(header: &delharc::LhaHeader, archive: &Path) -> bool {
        match header.parse_os_type() {
            Ok(delharc::OsType::Amiga) => true,
            Ok(delharc::OsType::Generic) if header.level == 0 => archive
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("lha") || e.eq_ignore_ascii_case("run")),
            _ => false,
        }
    }
    ```
    (`delharc` 0.8 re-exports `OsType` at its root, `lib.rs:178-186`.)
  - `zip.rs` `entries()`: `use zip::HasZipMetadata;` then
    ```rust
    let data = entry.get_metadata();
    let amiga_host = data.system == zip::System::Amiga;
    let comment = entry.comment();
    amiga: AmigaAttributes {
        protection: amiga_host.then(|| zip_amiga_protection(data.external_attributes)),
        comment: (amiga_host && !comment.is_empty()).then(|| comment.to_string()),
        date: entry.last_modified().map(|dt| {
            EntryDate::MsDos((u32::from(dt.datepart()) << 16) | u32::from(dt.timepart()))
        }),
    },
    ```
    and
    ```rust
    /// UnZip's Amiga port (`amiga/amiga.c:181-187`): flip PKAZip's FIB-style high
    /// word into Zip's set-means-granted form, clear the archive bit, then invert
    /// RWED back into the FIB form ART stores (R3 § 2). Low byte only: `.uaem`
    /// and PFS3 hold eight bits.
    pub(crate) fn zip_amiga_protection(external: u32) -> u32 {
        let mut tmp = external;
        if (tmp & 1) == ((tmp >> 18) & 1) {
            tmp ^= 0x000F_0000;
        }
        (((tmp >> 16) & 0xFF) & !0x10) ^ 0x0F
    }
    ```
  - `sevenz.rs` `entries()`: `amiga: AmigaAttributes { protection: None, comment: None, date: file.has_last_modified_date.then(|| EntryDate::Unix(filetime_to_unix(u64::from(file.last_modified_date)))) }` and
    ```rust
    /// A FILETIME (100 ns since 1601-01-01 UTC) as Unix seconds.
    pub(crate) fn filetime_to_unix(ft: u64) -> i64 {
        (ft / 10_000_000) as i64 - 11_644_473_600
    }
    ```
  - Every other literal: `amiga: AmigaAttributes::default()`.

- [ ] **Step 4: Green.** `cargo test --lib core::hashing`, `cargo test --lib core::archive`, `cargo test --lib core::rom`, `cargo test --lib core::lha` → each `test result: ok`.

- [ ] **Step 5: Mutations** (each: back up, mutate, run the named module, expect FAILED, restore):
  1. `lha_amiga_bits` `Generic if level == 0 => …` → `=> true` → kills `a_level_zero_entry_without_an_os_id_is_amiga_only_by_the_archive_name`.
  2. drop `^ 0x0F` in `zip_amiga_protection` → kills `an_amiga_host_s_attributes_become_fib_protection_bits`.
  3. drop the kluge `if` → kills the PKAZip line of the same test.
  4. `11_644_473_600` → `11_644_473_601` → kills `a_filetime_becomes_unix_seconds`.
  5. `Crc32::update` shift `>> 8` → `>> 7` → kills the CRC test and `crc32_empty_and_known_string`.

- [ ] **Step 6: Commit** — stage the files above by name; message "Archive entries carry Amiga protection, comment and date; one CRC-32" + mutations and survivors.

---

### Task 2: Amiga LZX — the container, merged groups, stored data

**Files:**
- Create: `src-tauri/src/core/archive/lzx.rs`
- Modify: `src-tauri/src/core/archive/mod.rs` (`pub mod lzx;` alphabetical; `open_with_password` arm `("lzx", None)`; password arm `format @ ("lha" | "7z" | "lzx")`; trait doc `"lha"`, `"zip"`, `"7z"`, `"lzx"`)
- Modify: `src-tauri/src/core/detect.rs:328-345` (LZX before LHA)
- Test: inline in `lzx.rs`

**Interfaces:**
- Consumes: `AmigaAttributes`, `ArchiveEntry`, `hashing::Crc32`/`crc32_ieee` (Task 1); `archive::extract::{MAX_ENTRIES, MAX_ENTRY_OUTPUT}`.
- Produces: `pub struct LzxBackend` with `pub fn open(path: &Path) -> CoreResult<Self>`, `impl ArchiveBackend` (`format() == "lzx"`, `read` via `read_selected`); `pub const MAX_GROUP_OUTPUT: u64 = MAX_ENTRY_OUTPUT;` `pub(crate) fn protection_from_lzx(attributes: u8) -> u32`; a private `fn decode_lzx<R: Read>(packed: R, stop_at: u64) -> CoreResult<Vec<u8>>` that **this task** leaves returning `Err(CoreError::UnsupportedFormat("compressed LZX blocks arrive in the next commit"))`; Task 3 replaces the body.

**The container, as the plan's sources state it** (R2 § A1, R3 § 4; `unlzx.c:1049-1072, 1328-1345`; XADMaster `XADLZXParser.m:51-66,126-137`). Write the code from this description, not by translating `unlzx.c` (no licence) or XADMaster:

```text
info header   10 bytes: "LZX" + 7 bytes ART does not read
record        31 bytes, then name (len [30]), then comment (len [14]), then `packed` bytes of data
  [0]      attributes: 0x01 r · 0x02 w · 0x04 d · 0x08 e · 0x10 a · 0x20 h · 0x40 s · 0x80 p, set = granted
  [2..6]   unpacked size, u32 LE
  [6..10]  packed size, u32 LE
  [11]     pack mode: 0 stored, 2 LZX
  [12]     flags, bit 0 merged
  [14]     comment length (0..=79)
  [18..22] date — NOT read (R2 § A3: three decoders, three years)
  [22..26] data CRC-32 LE, of this member's unpacked bytes
  [26..30] header CRC-32 LE, over the 31 bytes with [26..30] zeroed, then name, then comment
  [30]     name length
group         records with packed == 0 wait; the next record with packed > 0 closes the group
              and its data is the whole group's stream (members concatenated in record order)
names         Latin-1, `/` between components, no directory records (drawers are implied)
```

- [ ] **Step 1: Failing tests** (all in `lzx.rs` `mod tests`; the builder is the fixture — no Amiga software):

```rust
    use super::*;
    use crate::core::archive::extract::{extract_with_backend, OverwritePolicy};
    use crate::core::jobs::NoProgress;

    fn scratch(tag: &str) -> (crate::core::ScratchDir, PathBuf) {
        crate::core::ScratchDir::pair("art-lzx", tag)
    }

    pub(crate) struct Rec<'a> {
        pub name: &'a str,
        pub comment: &'a str,
        pub attrs: u8,
        pub unpacked: u32,
        pub method: u8,
        pub data_crc: u32,
        pub packed: &'a [u8],
    }

    /// An LZX archive, byte for byte as the container description above.
    pub(crate) fn archive(records: &[Rec]) -> Vec<u8> {
        let mut out = b"LZX".to_vec();
        out.extend_from_slice(&[0u8; 7]);
        for r in records {
            let mut h = [0u8; 31];
            h[0] = r.attrs;
            h[2..6].copy_from_slice(&r.unpacked.to_le_bytes());
            h[6..10].copy_from_slice(&(r.packed.len() as u32).to_le_bytes());
            h[10] = 10;
            h[11] = r.method;
            h[12] = u8::from(r.packed.is_empty());
            h[14] = r.comment.len() as u8;
            h[15] = 10;
            h[22..26].copy_from_slice(&r.data_crc.to_le_bytes());
            h[30] = r.name.len() as u8;
            let mut crc = crate::core::hashing::Crc32::new();
            crc.update(&h);
            crc.update(r.name.as_bytes());
            crc.update(r.comment.as_bytes());
            h[26..30].copy_from_slice(&crc.finish().to_le_bytes());
            out.extend_from_slice(&h);
            out.extend_from_slice(r.name.as_bytes());
            out.extend_from_slice(r.comment.as_bytes());
            out.extend_from_slice(r.packed);
        }
        out
    }

    fn crc(bytes: &[u8]) -> u32 {
        crate::core::hashing::crc32_ieee(bytes)
    }

    /// Three members in one stored group: listed with their Amiga attributes,
    /// written with the right bytes, drawers created from the paths alone.
    #[test]
    fn a_stored_merged_group_lists_and_writes_every_member() {
        let (_guard, dir) = scratch("stored-group");
        let path = dir.join("group.lzx");
        std::fs::write(
            &path,
            archive(&[
                Rec { name: "C/One", comment: "", attrs: 0x0F, unpacked: 3, method: 0, data_crc: crc(b"aaa"), packed: b"" },
                Rec { name: "C/Two", comment: "Prism2", attrs: 0x4F, unpacked: 2, method: 0, data_crc: crc(b"bb"), packed: b"" },
                Rec { name: "Three", comment: "", attrs: 0x0F, unpacked: 4, method: 0, data_crc: crc(b"cccc"), packed: b"aaabbcccc" },
            ]),
        )
        .unwrap();

        let mut backend = LzxBackend::open(&path).unwrap();
        let entries = backend.entries().unwrap();
        assert_eq!(entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["C/One", "C/Two", "Three"]);
        assert_eq!(entries[1].amiga.protection, Some(0x40));
        assert_eq!(entries[1].amiga.comment.as_deref(), Some("Prism2"));
        assert_eq!(entries[1].amiga.date, None, "LZX dates are not claimed (R2 § A3)");

        let out = dir.join("out");
        let outcome = extract_with_backend(&mut backend, &out, OverwritePolicy::Skip, &NoProgress).unwrap();
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(std::fs::read(out.join("C/One")).unwrap(), b"aaa");
        assert_eq!(std::fs::read(out.join("C/Two")).unwrap(), b"bb");
        assert_eq!(std::fs::read(out.join("Three")).unwrap(), b"cccc");
    }

    #[test]
    fn the_protection_byte_is_converted_to_the_amigados_order() {
        assert_eq!(protection_from_lzx(0x0F), 0x00); // ----rwed
        assert_eq!(protection_from_lzx(0x00), 0x0F); // --------
        assert_eq!(protection_from_lzx(0x0E), 0x08); // r not granted
        assert_eq!(protection_from_lzx(0x0D), 0x04); // w
        assert_eq!(protection_from_lzx(0x0B), 0x01); // d
        assert_eq!(protection_from_lzx(0x07), 0x02); // e
        assert_eq!(protection_from_lzx(0x1F), 0x10); // a
        assert_eq!(protection_from_lzx(0x2F), 0x80); // h
        assert_eq!(protection_from_lzx(0x4F), 0x40); // s
        assert_eq!(protection_from_lzx(0x8F), 0x20); // p
    }

    #[test]
    fn a_record_whose_header_crc_does_not_match_is_refused_by_number() {
        let (_guard, dir) = scratch("header-crc");
        let mut bytes = archive(&[Rec { name: "Two", comment: "", attrs: 0x0F, unpacked: 1, method: 0, data_crc: crc(b"x"), packed: b"x" }]);
        bytes[10 + 31] ^= 0x20; // "Two" → "two" after the CRC was computed
        let path = dir.join("bad.lzx");
        std::fs::write(&path, bytes).unwrap();
        let err = LzxBackend::open(&path).err().expect("refused");
        assert!(err.to_string().contains("record 0") && err.to_string().contains("header CRC"), "{err}");
    }

    #[test]
    fn a_member_whose_data_crc_does_not_match_is_not_written() {
        let (_guard, dir) = scratch("data-crc");
        let path = dir.join("bad.lzx");
        std::fs::write(
            &path,
            archive(&[
                Rec { name: "Good", comment: "", attrs: 0x0F, unpacked: 2, method: 0, data_crc: crc(b"gg"), packed: b"" },
                Rec { name: "Bad", comment: "", attrs: 0x0F, unpacked: 2, method: 0, data_crc: crc(b"XX"), packed: b"ggbb" },
            ]),
        )
        .unwrap();
        let out = dir.join("out");
        let mut backend = LzxBackend::open(&path).unwrap();
        let outcome = extract_with_backend(&mut backend, &out, OverwritePolicy::Skip, &NoProgress).unwrap();
        assert!(out.join("Good").is_file());
        assert!(!out.join("Bad").exists());
        assert!(outcome.errors.iter().any(|e| e.contains("Bad") && e.contains("data CRC")), "{:?}", outcome.errors);
    }

    #[test]
    fn an_archive_cut_short_is_refused() {
        let (_guard, dir) = scratch("cut");
        let whole = archive(&[Rec { name: "File", comment: "", attrs: 0x0F, unpacked: 4, method: 0, data_crc: crc(b"abcd"), packed: b"abcd" }]);
        let mid_record = dir.join("mid-record.lzx");
        std::fs::write(&mid_record, &whole[..10 + 20]).unwrap();
        assert!(LzxBackend::open(&mid_record).err().unwrap().to_string().contains("cut short"));
        let mid_data = dir.join("mid-data.lzx");
        std::fs::write(&mid_data, &whole[..whole.len() - 2]).unwrap();
        assert!(LzxBackend::open(&mid_data).err().unwrap().to_string().contains("file ends first"));
    }

    /// Refused on the declared sizes, before a byte is decoded: the packed
    /// bytes here are not a valid stream, so any attempt to decode would say
    /// something else.
    #[test]
    fn a_group_past_the_limit_is_refused_before_anything_is_decoded() {
        let (_guard, dir) = scratch("big-group");
        let path = dir.join("big.lzx");
        std::fs::write(
            &path,
            archive(&[
                Rec { name: "A", comment: "", attrs: 0x0F, unpacked: 200 << 20, method: 2, data_crc: 0, packed: b"" },
                Rec { name: "B", comment: "", attrs: 0x0F, unpacked: 100 << 20, method: 2, data_crc: 0, packed: &[0xFF; 8] },
            ]),
        )
        .unwrap();
        let mut backend = LzxBackend::open(&path).unwrap();
        let mut seen = Vec::new();
        backend
            .read_selected(&[true, true], MAX_ENTRY_OUTPUT, &mut |i, r| {
                seen.push((i, r.err().map(|e| e.to_string()).unwrap_or_default()));
                Ok(())
            })
            .unwrap();
        assert_eq!(seen.len(), 2);
        assert!(seen.iter().all(|(_, e)| e.contains("merged LZX group") && e.contains("limit")), "{seen:?}");
    }

    #[test]
    fn a_member_larger_than_the_limit_asked_is_refused() {
        let (_guard, dir) = scratch("limit");
        let path = dir.join("l.lzx");
        std::fs::write(&path, archive(&[Rec { name: "F", comment: "", attrs: 0x0F, unpacked: 3, method: 0, data_crc: crc(b"abc"), packed: b"abc" }])).unwrap();
        let mut backend = LzxBackend::open(&path).unwrap();
        assert!(backend.read(0, 2).is_err());
        assert_eq!(backend.read(0, 3).unwrap(), b"abc");
    }

    #[test]
    fn members_left_waiting_at_the_end_are_refused_unless_empty() {
        let (_guard, dir) = scratch("trailing");
        let path = dir.join("t.lzx");
        std::fs::write(
            &path,
            archive(&[
                Rec { name: "Empty", comment: "", attrs: 0x0F, unpacked: 0, method: 0, data_crc: 0, packed: b"" },
                Rec { name: "Lost", comment: "", attrs: 0x0F, unpacked: 5, method: 0, data_crc: 0, packed: b"" },
            ]),
        )
        .unwrap();
        let out = dir.join("out");
        let mut backend = LzxBackend::open(&path).unwrap();
        let outcome = extract_with_backend(&mut backend, &out, OverwritePolicy::Skip, &NoProgress).unwrap();
        assert_eq!(std::fs::read(out.join("Empty")).unwrap(), b"");
        assert!(!out.join("Lost").exists());
        assert!(outcome.errors.iter().any(|e| e.contains("Lost") && e.contains("no packed data")), "{:?}", outcome.errors);
    }

    /// The bytes decide, not the name (`core/archive/mod.rs::open`'s rule).
    #[test]
    fn an_lzx_is_opened_as_lzx_whatever_it_is_called() {
        let (_guard, dir) = scratch("dispatch");
        let path = dir.join("really.lha");
        std::fs::write(&path, archive(&[Rec { name: "F", comment: "", attrs: 0x0F, unpacked: 1, method: 0, data_crc: crc(b"z"), packed: b"z" }])).unwrap();
        assert_eq!(crate::core::detect::detect(&path).unwrap().format_hint, "lzx");
        assert_eq!(crate::core::archive::open(&path).unwrap().format(), "lzx");
    }
```

- [ ] **Step 2: Run red.** `cargo test --lib core::archive::lzx` → compile error (module absent). Create `lzx.rs` with the types and `unimplemented`-free stubs that return `Err(CoreError::NotImplemented("lzx".into()))` from `open`, run again: every test FAILS on that error; `an_lzx_is_opened_as_lzx_whatever_it_is_called` fails on `format_hint` (`"lha"`/`"unknown"` ≠ `"lzx"`).

- [ ] **Step 3: Implement** (`lzx.rs`; module doc cites R2, R3 § 4 and the owner's decision 6, and says in one paragraph why ART has its own decoder — R2 § A5):

```rust
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::ops::Range;
use std::path::{Path, PathBuf};

use super::extract::{MAX_ENTRIES, MAX_ENTRY_OUTPUT};
use super::{AmigaAttributes, ArchiveBackend, ArchiveEntry};
use crate::core::error::{CoreError, CoreResult};
use crate::core::hashing::{crc32_ieee, Crc32};

const INFO_HEADER_LEN: u64 = 10;
const RECORD_LEN: usize = 31;
const METHOD_STORED: u8 = 0;
const METHOD_LZX: u8 = 2;

/// The most one merged group may unpack to. A group is decoded whole — its
/// members share one stream — so the per-entry cap is the group's cap.
pub const MAX_GROUP_OUTPUT: u64 = MAX_ENTRY_OUTPUT;

fn malformed(detail: impl Into<String>) -> CoreError {
    CoreError::Malformed { format: "lzx".into(), detail: detail.into() }
}

#[derive(Debug, Clone)]
struct Record {
    entry: ArchiveEntry,
    data_crc: u32,
}

#[derive(Debug, Clone)]
struct Group {
    members: Range<usize>,
    method: u8,
    packed: u32,
    data_offset: u64,
}

#[derive(Debug)]
pub struct LzxBackend {
    path: PathBuf,
    records: Vec<Record>,
    groups: Vec<Group>,
    /// Records after the last group: nothing follows them.
    trailing: Range<usize>,
}

/// XADMaster `XADLZXParser.m:126-137`, the same bit order `unlzx.c:1101-1108`
/// prints: LZX stores "granted", AmigaDOS stores RWED inverted.
pub(crate) fn protection_from_lzx(attributes: u8) -> u32 {
    let a = u32::from(attributes);
    let mut p = 0;
    if a & 0x01 == 0 { p |= 0x08; }
    if a & 0x02 == 0 { p |= 0x04; }
    if a & 0x04 == 0 { p |= 0x01; }
    if a & 0x08 == 0 { p |= 0x02; }
    if a & 0x10 != 0 { p |= 0x10; }
    if a & 0x20 != 0 { p |= 0x80; }
    if a & 0x40 != 0 { p |= 0x40; }
    if a & 0x80 != 0 { p |= 0x20; }
    p
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

fn read_exact_or(reader: &mut impl Read, buf: &mut [u8], what: impl FnOnce() -> String) -> CoreResult<()> {
    reader.read_exact(buf).map_err(|e| match e.kind() {
        std::io::ErrorKind::UnexpectedEof => malformed(what()),
        _ => CoreError::Io(e),
    })
}

impl LzxBackend {
    pub fn open(path: &Path) -> CoreResult<Self> {
        let file = File::open(path)?;
        let file_len = file.metadata()?.len();
        let mut reader = BufReader::new(file);
        let mut info = [0u8; INFO_HEADER_LEN as usize];
        read_exact_or(&mut reader, &mut info, || "shorter than the 10-byte LZX info header".into())?;
        if &info[..3] != b"LZX" {
            return Err(malformed("does not start with 'LZX'"));
        }
        let mut at = INFO_HEADER_LEN;
        let mut records = Vec::new();
        let mut groups = Vec::new();
        let mut open_from = 0usize;
        // One past the gate's cap is enough for the gate to refuse the archive.
        while at < file_len && records.len() <= MAX_ENTRIES {
            let index = records.len();
            let mut head = [0u8; RECORD_LEN];
            read_exact_or(&mut reader, &mut head, || format!("record {index} is cut short"))?;
            let mut name = vec![0u8; usize::from(head[30])]; // at most 255: a u8, not a size claim
            read_exact_or(&mut reader, &mut name, || format!("record {index}'s name is cut short"))?;
            let mut comment = vec![0u8; usize::from(head[14])];
            read_exact_or(&mut reader, &mut comment, || format!("record {index}'s comment is cut short"))?;

            let stored = u32::from_le_bytes([head[26], head[27], head[28], head[29]]);
            let mut zeroed = head;
            zeroed[26..30].fill(0);
            let mut crc = Crc32::new();
            crc.update(&zeroed);
            crc.update(&name);
            crc.update(&comment);
            if crc.finish() != stored {
                return Err(malformed(format!(
                    "record {index} ('{}') fails its header CRC",
                    latin1(&name)
                )));
            }

            let unpacked = u32::from_le_bytes([head[2], head[3], head[4], head[5]]);
            let packed = u32::from_le_bytes([head[6], head[7], head[8], head[9]]);
            at += (RECORD_LEN + name.len() + comment.len()) as u64;
            let data_offset = at;
            let end = at
                .checked_add(u64::from(packed))
                .filter(|end| *end <= file_len)
                .ok_or_else(|| malformed(format!(
                    "record {index} says {packed} packed bytes follow and the file ends first"
                )))?;

            records.push(Record {
                entry: ArchiveEntry {
                    name: latin1(&name),
                    is_dir: false,
                    declared_bytes: u64::from(unpacked),
                    amiga: AmigaAttributes {
                        protection: Some(protection_from_lzx(head[0])),
                        comment: (!comment.is_empty()).then(|| latin1(&comment)),
                        date: None,
                    },
                },
                data_crc: u32::from_le_bytes([head[22], head[23], head[24], head[25]]),
            });

            if packed > 0 {
                groups.push(Group { members: open_from..index + 1, method: head[11], packed, data_offset });
                open_from = index + 1;
                reader.seek(SeekFrom::Start(end))?;
                at = end;
            }
        }
        let trailing = open_from..records.len();
        Ok(Self { path: path.to_path_buf(), records, groups, trailing })
    }
}
```

`ArchiveBackend`:
- `format()` → `"lzx"`; `entries()` → `self.records.iter().map(|r| r.entry.clone()).collect()`.
- `read(index, limit)`: reject `index >= records.len()` with `malformed`; build `wanted` with only `index`, call `read_selected`, return the one result (same shape as `sevenz.rs:85-105`).
- `read_selected(wanted, limit, sink)`:
  ```rust
  let mut reader = BufReader::new(File::open(&self.path)?);
  for group in &self.groups {
      let is_wanted = |i: usize| wanted.get(i).copied().unwrap_or(false);
      let Some(last_wanted) = group.members.clone().rev().find(|i| is_wanted(*i)) else { continue };
      let sizes: Vec<u64> = group.members.clone().map(|i| self.records[i].entry.declared_bytes).collect();
      let total: u64 = sizes.iter().sum(); // ≤ 100 001 × u32::MAX: no overflow in u64
      if total > MAX_GROUP_OUTPUT {
          for i in group.members.clone().filter(|i| is_wanted(*i)) {
              sink(i, Err(CoreError::InvalidInput(format!(
                  "'{}' is inside a merged LZX group that unpacks to {total} bytes, past the \
                   {MAX_GROUP_OUTPUT} byte limit ART decodes at once",
                  self.records[i].entry.name
              ))))?;
          }
          continue;
      }
      let stop_at: u64 = group.members.clone().take_while(|i| *i <= last_wanted).map(|i| self.records[i].entry.declared_bytes).sum();
      reader.seek(SeekFrom::Start(group.data_offset))?;
      let packed = (&mut reader).take(u64::from(group.packed));
      let decoded = match group.method {
          METHOD_STORED => read_stored(packed, stop_at),
          METHOD_LZX => decode_lzx(packed, stop_at),
          other => Err(CoreError::UnsupportedFormat(format!("LZX pack mode {other} is not one ART reads"))),
      };
      let mut offset = 0u64;
      for i in group.members.clone() {
          let size = self.records[i].entry.declared_bytes;
          let range = offset..offset + size;
          offset += size;
          if !is_wanted(i) { continue; }
          let name = &self.records[i].entry.name;
          let result = match &decoded {
              Err(e) => Err(malformed(format!("'{name}': {e}"))),
              Ok(_) if size > limit => Err(CoreError::InvalidInput(format!("'{name}' is {size} bytes, past the {limit} bytes asked for"))),
              Ok(bytes) => match bytes.get(range.start as usize..range.end as usize) {
                  None => Err(malformed(format!("'{name}' lies past the data its group decoded"))),
                  Some(slice) if crc32_ieee(slice) != self.records[i].data_crc => Err(malformed(format!("'{name}' fails its data CRC"))),
                  Some(slice) => Ok(slice.to_vec()),
              },
          };
          sink(i, result)?;
          if i == last_wanted { break; }
      }
  }
  for i in self.trailing.clone().filter(|i| wanted.get(*i).copied().unwrap_or(false)) {
      let entry = &self.records[i].entry;
      let result = if entry.declared_bytes == 0 { Ok(Vec::new()) } else {
          Err(malformed(format!("'{}' has no packed data after it — the archive ends first", entry.name)))
      };
      sink(i, result)?;
  }
  Ok(())
  ```
  (the trailing message must contain "no packed data" for the test.)
- `fn read_stored(packed: impl Read, stop_at: u64) -> CoreResult<Vec<u8>>`: `let mut out = Vec::new(); packed.take(stop_at).read_to_end(&mut out)?;` then `malformed("stored data ends early")` unless `out.len() as u64 == stop_at`. Growth follows the bytes present, never a header value.
- `fn decode_lzx<R: Read>(_packed: R, _stop_at: u64) -> CoreResult<Vec<u8>>` returns `Err(CoreError::UnsupportedFormat("compressed LZX blocks are not read yet".into()))` in this commit.
- `detect.rs`, before `is_lha_header`: `if head.len() >= 3 && &head[..3] == b"LZX" { return Ok(Detection { category: FormatCategory::Archive, format_hint: "lzx".to_string(), confidence: 0.9, size, is_dir: false }); }`

- [ ] **Step 4: Green.** `cargo test --lib core::archive` and `cargo test --lib core::detect` → `test result: ok`.

- [ ] **Step 5: Mutations:** (1) skip the header-CRC comparison → kills `a_record_whose_header_crc_does_not_match_is_refused_by_number`; (2) remove the `total > MAX_GROUP_OUTPUT` branch → kills `a_group_past_the_limit_is_refused_before_anything_is_decoded` (message changes to the decode error); (3) `size > limit` → `size > limit + 1` → kills `a_member_larger_than_the_limit_asked_is_refused`; (4) drop the data-CRC arm → kills `a_member_whose_data_crc_does_not_match_is_not_written`; (5) swap `0x20 → 0x80` and `0x80 → 0x20` in `protection_from_lzx` → kills the conversion test; (6) move the LZX detect check after the LHA check — expected **survivor** (no LZX header matches LHA's `-lh?-` at offset 2); disclose it.

- [ ] **Step 6: Commit** — `lzx.rs`, `archive/mod.rs`, `detect.rs`; message "LZX archives: container, merged groups, stored data (ART's own reader)".

---

### Task 3: Amiga LZX — compressed blocks

**Files:**
- Modify: `src-tauri/src/core/archive/lzx.rs` (replace `decode_lzx`; add `Bits`, `Huffman`, `read_literal_lengths`)
- Test: inline in `lzx.rs`

**Interfaces:**
- Consumes: Task 2's `archive()`/`Rec` test builders, `LzxBackend`.
- Produces: `fn decode_lzx<R: Read>(packed: R, stop_at: u64) -> CoreResult<Vec<u8>>` (private) — output exactly `stop_at` bytes or an error.

**The stream, as a format description** (checked against `unlzx.c:189-666` and XADMaster; write from this, not by translating either):

```text
bits      the stream is 16-bit big-endian words; bits are taken least-significant first from each
          word, word after word. A read of n bits returns bit 0 first (LSB-first value).
          Huffman codes are read one bit at a time, the code's MOST significant bit first.
state     reset per group: every literal length 0, every offset length 0, last_offset = 1, no table.
block     method = bits(3): 1 = reuse the previous literal table, 2 = new table, 3 = new table + aligned
          offsets; any other value is malformed.
          if method 3: 8 × bits(3) offset-code lengths → a Huffman table over 8 symbols.
          length = bits(8) << 16 | bits(8) << 8 | bits(8)  (bytes this block produces)
          if method 2 or 3: literal lengths (768 symbols), below.
literal   two passes: pass A fills positions 0..256 with fix = 1, pass B continues to 768 with fix = 0.
lengths   each pass first reads 20 × bits(4) pretree lengths → a Huffman table over 20 symbols, then
          decodes pretree symbols until the pass's end position:
            0..=16  length[pos] = (length[pos] + 17 − s) mod 17;  pos += 1
            17      run = 3 + bits(4) + fix zeros
            18      run = 19 + bits(6 − fix) + fix zeros
            19      run = 3 + bits(1) + fix; s2 = next pretree symbol (must be ≤ 16);
                    v = (length[pos] + 17 − s2) mod 17; run of v
          runs stop at the pass's end. Lengths persist between blocks (they are deltas).
huffman   canonical: codes assigned in order of (length, symbol), shortest first; every table must be
          complete (Kraft sum exactly 1) — an over-subscribed or incomplete table is malformed.
symbols   s < 256: a literal byte.
          else s −= 256; slot = s & 31; footer = ONE[slot]; offset = TWO[slot];
            if method 3 and footer ≥ 3: offset += bits(footer − 3) << 3; offset += aligned symbol
            else: offset += bits(footer); if offset == 0 { offset = last_offset }
          last_offset = offset
          len_slot = (s >> 5) & 15; length = TWO[len_slot] + 3 + bits(ONE[len_slot])
          copy `length` bytes from `offset` back, byte by byte (overlap allowed)
ONE = [0,0,0,0,1,1,2,2,3,3,4,4,5,5,6,6,7,7,8,8,9,9,10,10,11,11,12,12,13,13,14,14]
TWO = [0,1,2,3,4,6,8,12,16,24,32,48,64,96,128,192,256,384,512,768,1024,1536,2048,3072,4096,
       6144,8192,12288,16384,24576,32768,49152]
```

A match reaching before the group's first byte is malformed. A block's remaining length is reduced saturating; the next block header is read when it reaches 0 (a match overshooting a block end — which `unlzx.c` cannot read either — is tolerated rather than guessed at; the owner-material hook in Task 11 is what shows real archives never do it).

- [ ] **Step 1: Failing tests.** Add a bit writer and a block encoder to `mod tests`:

```rust
    struct BitWriter { bytes: Vec<u8>, word: u16, used: u32 }

    impl BitWriter {
        fn new() -> Self { Self { bytes: Vec::new(), word: 0, used: 0 } }
        fn bit(&mut self, b: u32) {
            self.word |= ((b & 1) as u16) << self.used;
            self.used += 1;
            if self.used == 16 {
                self.bytes.extend_from_slice(&self.word.to_be_bytes());
                self.word = 0;
                self.used = 0;
            }
        }
        /// An n-bit value, least significant bit first.
        fn put(&mut self, value: u32, n: u32) { for i in 0..n { self.bit(value >> i); } }
        /// A Huffman code, most significant bit first.
        fn code(&mut self, code: u32, len: u32) { for i in (0..len).rev() { self.bit(code >> i); } }
        fn finish(mut self) -> Vec<u8> {
            if self.used > 0 { self.bytes.extend_from_slice(&self.word.to_be_bytes()); }
            self.bytes
        }
    }

    /// One block whose literal table gives symbols 0..512 nine-bit codes
    /// (code = symbol) and 512..768 none. The pretree gives symbol 0 and 8
    /// one-bit codes (0 → `0`, 8 → `1`): 8 turns a 0 into 9, 0 leaves a 0.
    fn nine_bit_table(w: &mut BitWriter) {
        let pretree = |w: &mut BitWriter| {
            for s in 0..20 { w.put(if s == 0 || s == 8 { 1 } else { 0 }, 4); }
        };
        pretree(w); // pass A: 256 × "9"
        for _ in 0..256 { w.code(1, 1); }
        pretree(w); // pass B: 256 × "9", then 256 × "0"
        for _ in 0..256 { w.code(1, 1); }
        for _ in 0..256 { w.code(0, 1); }
    }

    fn put_len(w: &mut BitWriter, length: u32) {
        w.put((length >> 16) & 0xFF, 8);
        w.put((length >> 8) & 0xFF, 8);
        w.put(length & 0xFF, 8);
    }

    fn one_member(path: &Path, data: &[u8], stream: &[u8]) {
        std::fs::write(path, archive(&[Rec { name: "Out", comment: "", attrs: 0x0F, unpacked: data.len() as u32, method: 2, data_crc: crc(data), packed: stream }])).unwrap();
    }

    /// Literals, a match with three LSB-first extra offset bits (value 6, not a
    /// palindrome, so a reversed read gives 3), then a repeat of the last offset.
    #[test]
    fn a_compressed_block_of_literals_and_matches_decodes() {
        let expected = b"0123456789ABCDEFGHIJKLMN2345678";
        let mut w = BitWriter::new();
        w.put(2, 3);
        put_len(&mut w, expected.len() as u32);
        nine_bit_table(&mut w);
        for &b in &expected[..24] { w.code(u32::from(b), 9); }
        w.code(256 + 8, 9);    // offset slot 8: 16 + bits(3); length slot 0: 3
        w.put(6, 3);           // offset 22 → "234"
        w.code(256 + 32, 9);   // offset slot 0 → last offset 22; length slot 1: 4 → "5678"
        let (_guard, dir) = scratch("lzx-block");
        let path = dir.join("b.lzx");
        one_member(&path, expected, &w.finish());
        assert_eq!(LzxBackend::open(&path).unwrap().read(0, 1 << 20).unwrap(), expected);
    }

    #[test]
    fn an_aligned_offset_block_decodes() {
        let expected = b"0123456789ABCDEFGHIJKLMN234";
        let mut w = BitWriter::new();
        w.put(3, 3);
        for _ in 0..8 { w.put(3, 3); } // eight 3-bit aligned codes: code = symbol
        put_len(&mut w, expected.len() as u32);
        nine_bit_table(&mut w);
        for &b in &expected[..24] { w.code(u32::from(b), 9); }
        w.code(256 + 8, 9);    // slot 8 (footer 3): 16 + (bits(0) << 3) + aligned
        w.code(6, 3);          // aligned symbol 6 (code 110, not a palindrome) → offset 22 → "234"
        let (_guard, dir) = scratch("lzx-aligned");
        let path = dir.join("a.lzx");
        one_member(&path, expected, &w.finish());
        assert_eq!(LzxBackend::open(&path).unwrap().read(0, 1 << 20).unwrap(), expected);
    }

    #[test]
    fn a_first_block_that_reuses_a_table_is_refused() {
        let mut w = BitWriter::new();
        w.put(1, 3);
        put_len(&mut w, 1);
        w.put(0, 16);
        let (_guard, dir) = scratch("lzx-reuse");
        let path = dir.join("r.lzx");
        one_member(&path, b"x", &w.finish());
        let err = LzxBackend::open(&path).unwrap().read(0, 16).unwrap_err();
        assert!(err.to_string().contains("no earlier block built"), "{err}");
    }

    #[test]
    fn a_match_before_the_first_byte_is_refused() {
        let mut w = BitWriter::new();
        w.put(2, 3);
        put_len(&mut w, 3);
        nine_bit_table(&mut w);
        w.code(u32::from(b'a'), 9);
        w.code(256 + 4, 9);    // slot 4: offset 4 + bits(1)
        w.put(0, 1);           // offset 4, with one byte written
        let (_guard, dir) = scratch("lzx-before");
        let path = dir.join("m.lzx");
        one_member(&path, b"aaa", &w.finish());
        let err = LzxBackend::open(&path).unwrap().read(0, 16).unwrap_err();
        assert!(err.to_string().contains("before the start"), "{err}");
    }

    #[test]
    fn a_stream_that_ends_early_is_refused() {
        let mut w = BitWriter::new();
        w.put(2, 3);
        put_len(&mut w, 10);
        nine_bit_table(&mut w);
        w.code(u32::from(b'a'), 9);
        let (_guard, dir) = scratch("lzx-short");
        let path = dir.join("s.lzx");
        one_member(&path, b"aaaaaaaaaa", &w.finish());
        let err = LzxBackend::open(&path).unwrap().read(0, 16).unwrap_err();
        assert!(err.to_string().contains("ends before its data does"), "{err}");
    }

    #[test]
    fn an_incomplete_pretree_is_refused() {
        let mut w = BitWriter::new();
        w.put(2, 3);
        put_len(&mut w, 1);
        for s in 0..20 { w.put(if s == 0 { 1 } else { 0 }, 4); } // one 1-bit code: Kraft ½
        w.put(0, 16);
        let (_guard, dir) = scratch("lzx-incomplete");
        let path = dir.join("i.lzx");
        one_member(&path, b"x", &w.finish());
        let err = LzxBackend::open(&path).unwrap().read(0, 16).unwrap_err();
        assert!(err.to_string().contains("incomplete"), "{err}");
    }
```

- [ ] **Step 2: Run red.** `cargo test --lib core::archive::lzx` → the six new tests FAIL with "compressed LZX blocks are not read yet".

- [ ] **Step 3: Implement.**

```rust
const ONE: [u8; 32] = [0,0,0,0,1,1,2,2,3,3,4,4,5,5,6,6,7,7,8,8,9,9,10,10,11,11,12,12,13,13,14,14];
const TWO: [u32; 32] = [0,1,2,3,4,6,8,12,16,24,32,48,64,96,128,192,256,384,512,768,1024,1536,2048,3072,4096,6144,8192,12288,16384,24576,32768,49152];
const MAX_CODE_LEN: usize = 16;

struct Bits<R> { inner: R, word: u16, left: u8 }

impl<R: Read> Bits<R> {
    fn new(inner: R) -> Self { Self { inner, word: 0, left: 0 } }

    fn bit(&mut self) -> CoreResult<u32> {
        if self.left == 0 {
            let mut two = [0u8; 2];
            let mut got = 0;
            while got < 2 {
                let n = self.inner.read(&mut two[got..])?;
                if n == 0 { break; }
                got += n;
            }
            if got == 0 {
                return Err(malformed("the compressed stream ends before its data does"));
            }
            self.word = u16::from_be_bytes(two);
            self.left = 16;
        }
        let b = u32::from(self.word & 1);
        self.word >>= 1;
        self.left -= 1;
        Ok(b)
    }

    fn bits(&mut self, n: u8) -> CoreResult<u32> {
        let mut value = 0;
        for i in 0..n { value |= self.bit()? << i; }
        Ok(value)
    }
}

struct Huffman { counts: [u16; MAX_CODE_LEN + 1], symbols: Vec<u16> }

impl Huffman {
    fn new(lengths: &[u8]) -> CoreResult<Self> {
        let mut counts = [0u16; MAX_CODE_LEN + 1];
        for &len in lengths {
            let slot = counts.get_mut(usize::from(len)).ok_or_else(|| malformed(format!("a code length of {len}")))?;
            *slot += 1;
        }
        counts[0] = 0;
        let mut left: i64 = 1;
        for len in 1..=MAX_CODE_LEN {
            left = (left << 1) - i64::from(counts[len]);
            if left < 0 { return Err(malformed("a Huffman table is over-subscribed")); }
        }
        if left != 0 { return Err(malformed("a Huffman table is incomplete")); }
        let mut symbols = Vec::with_capacity(lengths.len()); // a slice this code owns: 8, 20 or 768
        for len in 1..=MAX_CODE_LEN {
            for (symbol, &l) in lengths.iter().enumerate() {
                if usize::from(l) == len { symbols.push(symbol as u16); }
            }
        }
        Ok(Self { counts, symbols })
    }

    fn decode<R: Read>(&self, bits: &mut Bits<R>) -> CoreResult<u16> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..=MAX_CODE_LEN {
            code |= bits.bit()? as i32;
            let count = i32::from(self.counts[len]);
            if code - count < first {
                return self.symbols.get((index + code - first) as usize).copied()
                    .ok_or_else(|| malformed("a Huffman code names no symbol"));
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err(malformed("a Huffman code runs past 16 bits"))
    }
}

fn delta(previous: u8, symbol: u16) -> u8 {
    ((u16::from(previous) + 17 - symbol) % 17) as u8
}

fn read_literal_lengths<R: Read>(bits: &mut Bits<R>, lengths: &mut [u8; 768]) -> CoreResult<()> {
    let mut pos = 0usize;
    for (fix, end) in [(1u32, 256usize), (0u32, 768usize)] {
        let mut pre = [0u8; 20];
        for l in pre.iter_mut() { *l = bits.bits(4)? as u8; }
        let pretree = Huffman::new(&pre)?;
        while pos < end {
            let symbol = pretree.decode(bits)?;
            let (run, value) = match symbol {
                17 => (3 + bits.bits(4)? + fix, None),
                18 => (19 + bits.bits((6 - fix) as u8)? + fix, None),
                19 => {
                    let run = 3 + bits.bits(1)? + fix;
                    let next = pretree.decode(bits)?;
                    if next > 16 { return Err(malformed("a length repeat names another repeat")); }
                    (run, Some(delta(lengths[pos], next)))
                }
                s => { lengths[pos] = delta(lengths[pos], s); pos += 1; continue; }
            };
            let value = value.unwrap_or(0);
            for _ in 0..run {
                if pos >= end { break; }
                lengths[pos] = value;
                pos += 1;
            }
        }
    }
    Ok(())
}

fn decode_lzx<R: Read>(packed: R, stop_at: u64) -> CoreResult<Vec<u8>> {
    let mut bits = Bits::new(packed);
    let mut out: Vec<u8> = Vec::new();
    let mut literal_len = [0u8; 768];
    let mut literal: Option<Huffman> = None;
    let mut aligned: Option<Huffman> = None;
    let mut method = 0u32;
    let mut block_left = 0u64;
    let mut last_offset = 1usize;

    while (out.len() as u64) < stop_at {
        if block_left == 0 {
            method = bits.bits(3)?;
            if !(1..=3).contains(&method) {
                return Err(malformed(format!("block type {method} is not one Amiga LZX writes")));
            }
            if method == 3 {
                let mut offset_len = [0u8; 8];
                for l in offset_len.iter_mut() { *l = bits.bits(3)? as u8; }
                aligned = Some(Huffman::new(&offset_len)?);
            }
            block_left = u64::from(bits.bits(8)?) << 16;
            block_left |= u64::from(bits.bits(8)?) << 8;
            block_left |= u64::from(bits.bits(8)?);
            if method != 1 {
                read_literal_lengths(&mut bits, &mut literal_len)?;
                literal = Some(Huffman::new(&literal_len)?);
            }
            if literal.is_none() {
                return Err(malformed("a block reuses a literal table no earlier block built"));
            }
            continue;
        }
        let table = literal.as_ref().ok_or_else(|| malformed("no literal table"))?;
        let symbol = usize::from(table.decode(&mut bits)?);
        if symbol < 256 {
            out.push(symbol as u8);
            block_left = block_left.saturating_sub(1);
            continue;
        }
        let symbol = symbol - 256;
        let slot = symbol & 31;
        let footer = ONE[slot];
        let mut offset = TWO[slot] as usize;
        if method == 3 && footer >= 3 {
            offset += (bits.bits(footer - 3)? as usize) << 3;
            let aligned = aligned.as_ref().ok_or_else(|| malformed("an aligned block has no offset table"))?;
            offset += usize::from(aligned.decode(&mut bits)?);
        } else {
            offset += bits.bits(footer)? as usize;
            if offset == 0 { offset = last_offset; }
        }
        last_offset = offset;
        let len_slot = (symbol >> 5) & 15;
        let length = TWO[len_slot] as usize + 3 + bits.bits(ONE[len_slot])? as usize;
        let start = out.len().checked_sub(offset).ok_or_else(|| malformed(format!(
            "a match reaches {offset} bytes back, before the start of the data"
        )))?;
        for k in 0..length {
            if out.len() as u64 >= stop_at { break; }
            let byte = *out.get(start + k).ok_or_else(|| malformed("a match reads past what was decoded"))?;
            out.push(byte);
        }
        block_left = block_left.saturating_sub(length as u64);
    }
    Ok(out)
}
```

`stop_at ≤ MAX_GROUP_OUTPUT` (Task 2) is the loop's bound: `out` never grows past it.

- [ ] **Step 4: Green.** `cargo test --lib core::archive::lzx` → `test result: ok`.

- [ ] **Step 5: Mutations:** (1) `Bits::bits` fill MSB-first (`value = value << 1 | bit`) → kills `a_compressed_block_of_literals_and_matches_decodes` (offset 19, not 22); (2) `Huffman::decode` reading LSB-first (reverse the code accumulation) → kills both decode tests; (3) delete `if offset == 0 { offset = last_offset; }` → kills the first decode test (the "5678" match); (4) remove the Kraft `left != 0` check → kills `an_incomplete_pretree_is_refused`; (5) `(1u32, 256)` → `(0u32, 256)` (fix wrong in pass A) → expected **survivor** in these fixtures (no run symbols used); disclose it and note Task 11's hook over real archives is its guard; (6) `method == 3 && footer >= 3` → `footer >= 3` → kills `a_compressed_block_of_literals_and_matches_decodes` (slot 8 read as aligned without a table).

- [ ] **Step 6: Commit** — `lzx.rs`; message "LZX archives: compressed blocks" + survivors.

---

### Task 4: HDF extraction — directory sidecars and escaped-name pairs

**Files:**
- Modify: `src-tauri/src/core/volume/write/copy.rs` — `ExtractReport` (`:786-797`), `host_target` (`:822-846`), `extract_dir` Descend branch (`:1076-1090`), `write_one_file` (`:1015-1036`), `extract_selection_from_volume` Descend branch (`~:1145`)
- Test: inline in `copy.rs`

**Interfaces:**
- Produces:
  ```rust
  /// One node written under a host name that differs from its AmigaDOS name.
  #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
  pub struct EscapedName {
      /// Where it landed on the host — the full path `host_target` made.
      pub host_path: PathBuf,
      /// That one node's own AmigaDOS name.
      pub amiga_name: String,
  }
  // ExtractReport gains: pub escaped: Vec<EscapedName>,   (beside `renamed`, which stays as it is)
  pub(crate) fn header_sidecar<D: BlockDevice + ?Sized>(device: &D, set: &BlockSet, block: u32) -> CoreResult<Option<Sidecar>>
  ```

- [ ] **Step 1: Failing tests** (use the module's existing volume fixture helpers — read `copy.rs`'s `mod tests` first and reuse its image builder and `VolumeWriter` setup, as `an_amiga_name_ntfs_refuses_is_escaped` does):

```rust
    /// R2 B1/B3 item 1: `renamed` is a display string of the leaf only; the pair
    /// says where the node landed, so a caller can put the AmigaDOS name back.
    #[test]
    fn an_escaped_drawer_and_the_file_in_it_are_recorded_with_their_host_paths() {
        // volume: AUX/ (dir) holding "a?b" (file) — `:` and `/` are not legal AmigaDOS name
        // characters (`dir::check_name`), `?` is and NTFS refuses it
        // extract root into dest with write_sidecars = false
        // assert report.escaped == [
        //   EscapedName { host_path: dest.join("_AUX"), amiga_name: "AUX" },
        //   EscapedName { host_path: dest.join("_AUX").join("a_b"), amiga_name: "a?b" },
        // ]
    }

    /// R2 item 11 / R3 § 5: a drawer's own bits, date and comment survive.
    #[test]
    fn a_drawer_s_protection_date_and_comment_go_into_its_own_sidecar() {
        // volume: Game/ with set_attributes(block, Some(0x40), Some("note"), Some(AmigaDate { days: 15_000, mins: 600, ticks: 50 }))
        //         and Game/File
        // extract root into dest with write_sidecars = true
        // let text = std::fs::read_to_string(dest.join("Game.uaem")).unwrap();
        // let parsed = uaem::parse(&text).unwrap();
        // assert_eq!(parsed.protection, 0x40); assert_eq!(parsed.comment, "note");
        // assert_eq!(parsed.date, AmigaDate { days: 15_000, mins: 600, ticks: 50 });
        // assert_eq!(report.sidecars_written, 1);
    }

    /// The negative control: a drawer with default bits, no comment and the
    /// epoch date gets no sidecar (`sidecar_for`'s rule, as for files).
    #[test]
    fn a_plain_drawer_gets_no_sidecar() { /* set_attributes(block, Some(0), None, Some(AmigaDate::default())); assert !dest.join("Plain.uaem").exists() */ }
```

Write the bodies concretely with the module's fixture (the comments give the exact volume and assertions); `set_attributes` is `VolumeWriter::set_attributes(entry_block, protection, comment, date)` (`volume/write/mod.rs:899`).

- [ ] **Step 2: Run red.** `cargo test --lib core::volume::write::copy` → compile error on `escaped`; after adding the field, the escaped test fails `[] != [...]` and the drawer test fails "No such file" on `Game.uaem`.

- [ ] **Step 3: Implement.**
  - `ExtractReport`: `pub escaped: Vec<EscapedName>,` with doc "Every node written under a different name, where it landed — `renamed` says the same for a person to read".
  - `host_target`: inside `if safe != name`, after the `renamed` push: `report.escaped.push(EscapedName { host_path: dest.join(&safe), amiga_name: name.to_string() });`
  - Factor the header read out of `write_one_file`:
    ```rust
    pub(crate) fn header_sidecar<D: BlockDevice + ?Sized>(device: &D, set: &BlockSet, block: u32) -> CoreResult<Option<Sidecar>> {
        let header = set.view(device, block)?;
        let protection = super::layout::get_u32(&header, PROTECT_OFFSET)?;
        let date = crate::core::adf::bcpl::AmigaDate {
            days: super::layout::get_u32(&header, super::layout::DAYS_OFFSET)?,
            mins: super::layout::get_u32(&header, super::layout::MINS_OFFSET)?,
            ticks: super::layout::get_u32(&header, super::layout::TICKS_OFFSET)?,
        };
        let comment = crate::core::adf::bcpl::read_bcpl_string(&header, super::layout::COMMENT_OFFSET).unwrap_or_default();
        Ok(sidecar_for(protection, date, &comment))
    }
    ```
    `write_one_file` calls it. In both `HostTarget::Descend(target)` branches (`extract_dir` and `extract_selection_from_volume`), before recursing: `if write_sidecars { if let Some(sidecar) = header_sidecar(device, &set, entry.block)? { atomic_write(&uaem::sidecar_path(&target), uaem::render(&sidecar).as_bytes())?; report.sidecars_written += 1; } }` (a user directory header uses the same protection/date/comment offsets as a file header — the OFS/FFS layout `layout.rs` already names).

- [ ] **Step 4: Green.** `cargo test --lib core::volume` → `test result: ok` (other tests that compare whole `ExtractReport` values get `escaped` added — say which in the commit).

- [ ] **Step 5: Mutations:** (1) remove the `escaped.push` → kills the pairs test; (2) push `dest.join(name)` instead of `dest.join(&safe)` → kills it; (3) remove the Descend sidecar write → kills the drawer test; (4) write the sidecar unconditionally (ignore `sidecar_for`'s `None`) → kills `a_plain_drawer_gets_no_sidecar`.

- [ ] **Step 6: Commit** — `copy.rs` (+ any test file adjusted); message "HDF extraction: escaped names recorded as pairs, drawers keep their sidecars".

---

### Task 5: The native copy carries what a sidecar says — PFS3 date and comment, FFS comment

**Files:**
- Modify: `src-tauri/vendor/libpfs3/src/writer.rs` (field `entry_comment`, `set_entry_comment`, `build_dir_entry` `:2104-2151`), `src-tauri/vendor/libpfs3/src/error.rs` (`CommentTooLong`), `src-tauri/vendor/libpfs3/Cargo.toml` (`0.1.3+art.12`), `src-tauri/vendor/libpfs3/ART-PATCH.md` (a numbered entry), `src-tauri/Cargo.lock` (via `cargo update -p libpfs3 --precise 0.1.3+art.12`)
- Modify: `src-tauri/src/core/preload/native.rs` — module doc § ART-116 (`:75-89`), `copy_in_pfs3` loop (`:1022-1090`), `copy_in_ffs` (`:1185-1210`), `from_pfs3` (`:375`)
- Test: inline in `native.rs` (the existing PFS3 read-back helpers, e.g. the one `copy_in_carries_the_protection_bits_out_of_the_uaem_sidecars` uses, `:1730`)

**Interfaces:**
- Produces: `libpfs3::writer::Writer::set_entry_comment(&mut self, comment: &[u8]) -> Result<()>`; `libpfs3::error::Error::CommentTooLong { len: usize, max: usize }`; `pub const MAX_COMMENT_BYTES: usize = 79;` in `writer.rs`.
- Changes: `CopySummary::comments_lost` / `dates_lost` stay as fields (serialised) but the PFS3 branch no longer increments them for what it now writes.

- [ ] **Step 1: Failing tests** in `native.rs`:

```rust
    /// ART-335 (this round): a sidecar's date and comment reach a PFS3 volume.
    /// Read back through libpfs3's own reader, not recomputed.
    #[test]
    fn copy_in_carries_the_date_and_comment_out_of_the_uaem_sidecars() {
        // tree: C/Assign (bytes b"x") + C/Assign.uaem "--p-rwed 2021-04-13 02:43:13.68 hello comment\n"
        //       C.uaem "----rwed 2020-01-02 03:04:05.00 drawer note\n"
        // format a PFS3 test image and copy_in exactly as copy_in_carries_the_protection_bits_out_of_the_uaem_sidecars does
        // let entry = <list "C" through libpfs3>.find("Assign")
        // assert_eq!(entry.comment, "hello comment");
        // let expected = uaem::parse("--p-rwed 2021-04-13 02:43:13.68 x\n").unwrap().date;
        // assert_eq!((entry.creation_day, entry.creation_minute, entry.creation_tick), pfs3_datestamp(expected));
        // let c = <list root>.find("C"); assert_eq!(c.comment, "drawer note");
        // assert_eq!(summary.comments_lost, 0); assert_eq!(summary.dates_lost, 0);
    }

    /// The negative control: no sidecar — or a sidecar whose date is the epoch,
    /// which `sidecar_for` treats as "no date" — gets no comment and the clock's "now".
    #[test]
    fn an_entry_without_a_sidecar_date_gets_no_comment_and_the_clock_s_date() {
        // C/Plain with no sidecar; C/Epoch with "--p-rwed 1978-01-01 00:00:00.00 \n"; clock = a FixedClock
        // for both: assert comment == "" and the datestamp equals pfs3_datestamp(clock.amiga_now())
    }

    #[test]
    fn a_comment_the_amiga_cannot_store_is_refused_by_name() {
        // C/Assign.uaem comment "日本" (not Latin-1) → copy_in Err, message names "C/Assign"
    }

    /// ART-337 (this round): FFS used to drop comments without counting them.
    #[test]
    fn ffs_copy_in_carries_the_comment_of_a_file_and_a_drawer() {
        // as ffs_copy_in_carries_the_protection_bits_out_of_the_uaem_sidecars (:3072), with comments;
        // read back through ART's FFS reader: attrs.comment == "hello comment" for C/Assign and "drawer note" for C
    }
```

And in `vendor/libpfs3/src/writer.rs` — no test module is carried (ART-PATCH.md "Not carried: tests/"), so the writer's own proof is the native test above plus:

```rust
    // native.rs
    #[test]
    fn libpfs3_refuses_a_comment_longer_than_the_amiga_stores() {
        // open a writer on a formatted in-memory/test volume
        // assert!(matches!(writer.set_entry_comment(&[b'c'; 80]), Err(libpfs3::error::Error::CommentTooLong { len: 80, max: 79 })));
        // assert!(writer.set_entry_comment(&[b'c'; 79]).is_ok());
    }
```

Write every body concretely from the named existing tests' setup.

- [ ] **Step 2: Run red.** `cargo test --lib core::preload::native` → compile error on `set_entry_comment`/`CommentTooLong`; after the vendor API exists but before native uses it: comment `"" != "hello comment"`, date equals "now" not 2021, FFS comment empty.

- [ ] **Step 3: Implement.**
  - `writer.rs`: field `entry_comment: Vec<u8>` (init `Vec::new()` beside `entry_date: None`, `:150`), doc citing card round 2 / ART-335:
    ```rust
    /// ART (card round 2, ART-335): at most this many comment bytes — pfs3aio's
    /// `CMSIZE` 80 as a BSTR, WinUAE's and `uaem::MAX_COMMENT_LEN`'s 79.
    pub const MAX_COMMENT_BYTES: usize = 79;

    /// ART (card round 2, ART-335): the comment of each **new** directory entry,
    /// Latin-1 bytes as AmigaDOS stores them; empty is none, as 0.1.3 wrote.
    /// Like `set_entry_date`, it stays until changed.
    pub fn set_entry_comment(&mut self, comment: &[u8]) -> Result<()> {
        if comment.len() > MAX_COMMENT_BYTES {
            return Err(Error::CommentTooLong { len: comment.len(), max: MAX_COMMENT_BYTES });
        }
        self.entry_comment = comment.to_vec();
        Ok(())
    }
    ```
    `build_dir_entry`: `let clen = self.entry_comment.len();` `let fields = extra_fields_offset(nlen, clen);` and replace `entry[18 + nlen] = 0; // comment length` with `entry[18 + nlen] = clen as u8; entry[19 + nlen..19 + nlen + clen].copy_from_slice(&self.entry_comment);` (`19 + nlen + clen ≤ (20 + nlen + clen) & !1` always holds). File header comment: "Modified 2026-09-17 by ART for card round 2 (ART-335)".
  - `error.rs`: `#[error("a comment of {len} bytes is longer than the {max} an AmigaDOS entry holds")] CommentTooLong { len: usize, max: usize },` + its header note.
  - `ART-PATCH.md`: add the date to the Modified row and a numbered change describing the setter, the entry layout it fills, and that 0.1.3's entries are unchanged when no comment is set.
  - `native.rs` `copy_in_pfs3` loop body, replacing the fixed `set_entry_date(now)` and the after-the-fact comment/date accounting:
    ```rust
    let sidecar = read_sidecar(&entry.host_path)?;
    let date = sidecar.as_ref().map(|s| s.date).filter(|d| *d != AmigaDate::default()).unwrap_or_else(|| clock.amiga_now());
    writer.set_entry_date(Some(pfs3_datestamp(date)));
    let comment = match &sidecar {
        Some(s) => latin1_comment(&s.comment, &entry.relative)?,
        None => Vec::new(),
    };
    writer.set_entry_comment(&comment).map_err(from_pfs3)?;
    // … create_dir_in / write_file_in as today …
    if let Some(s) = &sidecar {
        writer.update_dir_entry_protection(parent, name, pfs3_protection(s.protection)?).map_err(from_pfs3)?;
    }
    ```
    with
    ```rust
    fn read_sidecar(host: &Path) -> CoreResult<Option<uaem::Sidecar>> {
        let path = uaem::sidecar_path(host);
        if !path.is_file() { return Ok(None); }
        if std::fs::metadata(&path)?.len() > uaem::MAX_UAEM_BYTES {
            return Err(CoreError::InvalidInput(format!("'{}' is too large to be a .uaem sidecar", path.display())));
        }
        uaem::parse(&std::fs::read_to_string(&path)?).map(Some)
    }

    /// The comment as AmigaDOS stores it: Latin-1, the first 79 characters
    /// (what `uaem::render` itself writes).
    fn latin1_comment(comment: &str, relative: &str) -> CoreResult<Vec<u8>> {
        comment.chars().take(uaem::MAX_COMMENT_LEN).map(|c| u8::try_from(u32::from(c)).map_err(|_| CoreError::InvalidInput(format!(
            "the comment of '{relative}' holds '{c}', which an Amiga cannot store"
        )))).collect()
    }
    ```
    `from_pfs3`: map `CommentTooLong { len, max }` to `CoreError::InvalidInput(format!("a comment of {len} bytes is longer than the {max} an Amiga stores"))`.
    Rewrite the module doc's ART-116 section: dates and comments are carried; the counters stay for callers that still read them and are zero on this path.
  - `copy_in_ffs`: directory — `writer.set_attributes(block, Some(parsed.protection), (!parsed.comment.is_empty()).then_some(parsed.comment.as_str()), Some(parsed.date))?;` file — keep `add_file(.., meta)`, then `if !parsed.comment.is_empty() { if let Some(block) = outcome.block { writer.set_attributes(block, None, Some(&parsed.comment), None)?; } }` (bind `outcome` from `add_file`).
  - Existing tests that asserted `comments_lost == 1` / `dates_lost == 1` for PFS3 (`native.rs:2771-2841`): change to `0` with the comment `// ART-335: carried now — the counter counts what could not be written`, and keep their falsification intent by asserting the read-back comment/date instead.
  - `cd src-tauri && cargo update -p libpfs3 --precise 0.1.3+art.12`.

- [ ] **Step 4: Green.** `cargo test --lib core::preload` → `test result: ok`; `cargo test --lib core::card::sizing` → ok (no sizing change yet; entries without comments are byte-identical).

- [ ] **Step 5: Mutations:** (1) `build_dir_entry` writes `clen` but not the bytes → kills the PFS3 comment test; (2) `set_entry_date(Some(pfs3_datestamp(clock.amiga_now())))` restored → kills the date assertion; (3) `.filter(|d| *d != AmigaDate::default())` removed → kills `an_entry_without_a_sidecar_date_gets_no_comment_and_the_clock_s_date` (its `C/Epoch` case); (4) `> MAX_COMMENT_BYTES` → `>= MAX_COMMENT_BYTES + 2` → kills `libpfs3_refuses_a_comment_longer_than_the_amiga_stores`; (5) FFS file `set_attributes` removed → kills the FFS test; (6) `latin1_comment` accepting any char via `as u8` → kills `a_comment_the_amiga_cannot_store_is_refused_by_name`.

- [ ] **Step 6: Commit** — `vendor/libpfs3/src/writer.rs`, `vendor/libpfs3/src/error.rs`, `vendor/libpfs3/Cargo.toml`, `vendor/libpfs3/ART-PATCH.md`, `src-tauri/Cargo.lock`, `native.rs`; message "Native copy carries sidecar dates and comments to PFS3, comments to FFS (libpfs3 +art.12)".

---

### Task 6: Sizing counts comment bytes

**Files:**
- Modify: `src-tauri/src/core/card/sizing.rs` — `entry_bytes` (`:142-153`), `ContentMeasure` impl (`:155-170`), doc citations `:143-147, :176` → `writer.rs:2104-2151, :2061` (R1-9), test helper `fill` (`:669-685`)
- Test: inline in `sizing.rs`

**Interfaces:**
- Consumes: `Writer::set_entry_comment` (Task 5).
- Produces: `ContentMeasure::add_file_with_comment(&mut self, name: &str, comment_bytes: u64, bytes: u64)`, `ContentMeasure::add_directory_with_comment(&mut self, name: &str, comment_bytes: u64)`; `add_file`/`add_directory` delegate with `0`.

- [ ] **Step 1: Failing test.** Add `fill_commented` (a copy of `fill` that calls `writer.set_entry_comment(&[b'c'; 79])?` before each `create_dir` and `write_file`) and:

```rust
    /// Comments live inside the directory entry (`extra_fields_offset(nlen, clen)`,
    /// `direntry.rs:106`): 10 drawers of 1 600 files with 100-character names and
    /// 79-byte comments must still fit the estimate.
    #[test]
    fn large_directories_with_long_names_and_comments_fit_their_estimate() {
        let mut measure = ContentMeasure::default();
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        for d in 0..10 {
            let dir = format!("Drawer{d:06}");
            measure.add_directory_with_comment(&dir, 79);
            for f in 0..1_600 {
                let name = format!("{:0>100}", d * 1_600 + f);
                measure.add_file_with_comment(&name, 79, 200);
                files.push((format!("{dir}/{name}"), 200));
            }
            dirs.push(dir);
        }
        assert_estimate_holds_with("10 large drawers, long names, 79-byte comments", &dirs, &files, &measure, fill_commented);
    }
```

(`assert_estimate_holds_with` is `assert_estimate_holds` taking the fill function as a parameter; make the existing one delegate with `fill`.) Write `add_*_with_comment` first as `self.add_file(name, bytes)` / `self.add_directory(name)` — ignoring the comment.

- [ ] **Step 2: Run red.** `cargo test --lib card::sizing::tests::large_directories_with_long_names_and_comments_fit_their_estimate` → FAILED: the writer runs out of room (or breaches the always-free twentieth) at the estimated size.

- [ ] **Step 3: Implement.** `fn entry_bytes(name: &str, comment_bytes: u64) -> u64 { let raw = 18 + name.chars().count() as u64 + 1 + comment_bytes + 2; raw + raw % 2 }` and the two methods charging it; fix the doc citations (`build_dir_entry` `writer.rs:2104-2151`, packing `writer.rs:2061`) and say that a comment is now written (Task 5).

- [ ] **Step 4: Green.** `cargo test --lib core::card::sizing` → `test result: ok` (the 26 existing tests plus the new one).

- [ ] **Step 5: Mutation:** drop `+ comment_bytes` → kills the new test. `+ 1` (the comment-length byte) → `+ 0`: expected to kill `a_tree_shaped_like_the_owners_fits_its_estimate` or the long-name test; if it survives, disclose.

- [ ] **Step 6: Commit** — `sizing.rs`; message "Card sizing: directory entries count their comments".

---

### Task 7: An ART-private names record, and hst-imager reads `.uaem`

**Files:**
- Modify: `src-tauri/src/core/preload/amiga_names.rs`
- Modify: `src-tauri/src/core/preload/native.rs` `collect_into` (`:848-930`) — skip the record at the source root
- Modify: `src-tauri/src/tools/hst_imager.rs` `copy_args` (`:166-175`) and its test (`:380-392`)
- Test: inline in each

**Interfaces:**
- Produces (in `core::preload::amiga_names`):
  ```rust
  /// ART's own record of escaped names in a staging folder — never Amiga content.
  pub const AMIGA_NAMES_RECORD: &str = ".art-amiga-names.json";
  /// `names`: host path (`/`-separated, relative to `source`) → that node's AmigaDOS name.
  /// Refuses an empty map (nothing to record) and an existing record (SAFE_CREATE).
  pub fn write_record(source: &Path, names: &BTreeMap<String, String>) -> CoreResult<()>
  // AmigaNames::read(source) = distribution.json pairs + the record's nodes (the record wins on the same key)
  ```
  File shape: `{"version":1,"names":[{"host":"Drawer/_AUX","amiga":"AUX"}]}`.

- [ ] **Step 1: Failing tests.**

```rust
    // amiga_names.rs
    #[test]
    fn a_staged_record_puts_the_amiga_names_back_node_by_node() {
        let (_guard, dir) = crate::core::ScratchDir::pair("art-amiga-names", "record");
        let names = BTreeMap::from([("_AUX".to_string(), "AUX".to_string()), ("_AUX/a_b".to_string(), "a?b".to_string())]);
        write_record(&dir, &names).unwrap();
        let read = AmigaNames::read(&dir);
        assert_eq!(read.name_for("_AUX"), Some("AUX"));
        assert_eq!(read.name_for("_AUX/a_b"), Some("a?b"));
        assert!(write_record(&dir, &names).is_err(), "SAFE_CREATE: an existing record is not replaced");
    }

    #[test]
    fn an_empty_record_is_never_written() {
        let (_guard, dir) = crate::core::ScratchDir::pair("art-amiga-names", "empty");
        assert!(write_record(&dir, &BTreeMap::new()).is_err());
        assert!(!dir.join(AMIGA_NAMES_RECORD).exists());
    }

    // native.rs
    /// The record is ART's, like `.art-backup`: it never reaches the card.
    #[test]
    fn the_names_record_is_not_copied_and_its_names_are_used() {
        // tree: _AUX (file) + record {"_AUX": "AUX"}
        // let entries = collect_entries(&tree).unwrap();
        // assert_eq!(entries.iter().map(|e| e.relative.as_str()).collect::<Vec<_>>(), ["AUX"]);
    }

    // hst_imager.rs
    /// R3 § 7, measured: without `--uaemetadata UaeMetafile` hst-imager ignores
    /// every `.uaem` — bits, dates and comments silently lost.
    #[test]
    fn a_copy_asks_hst_imager_to_read_uaem_sidecars() {
        let args = copy_args(Path::new(r"E:\x.img"), None, "DH0", Path::new(r"E:\tree"));
        let at = args.iter().position(|a| a == "--uaemetadata").expect("the option is passed");
        assert_eq!(args.get(at + 1).map(String::as_str), Some("UaeMetafile"));
    }

    /// R2 B3 item 3: the record written by content staging feeds the same refusal.
    #[test]
    fn a_staged_names_record_is_refused_before_the_tool_runs() {
        // tree with record {"_AUX":"AUX"}; HstImager::new(<a path that does not exist>).copy_in(..)
        // → Err(CoreError::EscapedNamesNeedNativeCopy { pairs }) with ("_AUX","AUX") — before any spawn
        // (model on a_tree_with_escaped_names_is_refused_before_the_tool_runs)
    }
```

- [ ] **Step 2: Run red.** `cargo test --lib core::preload::amiga_names`, `cargo test --lib core::preload::native::tests::the_names_record`, `cargo test --lib tools::hst_imager` → compile errors, then assertion failures (`None`, record copied as an entry, option absent).

- [ ] **Step 3: Implement.**
  - `amiga_names.rs`: `#[derive(Serialize, Deserialize)] struct Record { version: u32, names: Vec<NodeName> }`, `struct NodeName { host: String, amiga: String }`; `write_record` refuses empty (`CoreError::InvalidInput("no escaped names to record")`), refuses an existing file (`CoreError::SafetyRefused`), writes through `crate::core::safety::atomic::atomic_write`. `read`: start from the manifest map (as today), then read `AMIGA_NAMES_RECORD` (absent or unparsable → nothing, the module's rule) and `insert(host, amiga)` for each node. Module doc: a second input, why a private file (R2 B3 item 2), and that it is written only when non-empty so hst-imager never copies one (its refusal fires first).
  - `native.rs` `collect_into`: `if host_prefix.is_empty() && host_name == AMIGA_NAMES_RECORD { continue; }` beside the `BACKUP_DIR` skip.
  - `hst_imager.rs` `copy_args`: append `"--uaemetadata".into(), "UaeMetafile".into()` with a doc line citing R3 § 7's experiment; the existing arg test keeps its assertions.

- [ ] **Step 4: Green.** `cargo test --lib core::preload` and `cargo test --lib tools::hst_imager` → ok.

- [ ] **Step 5: Mutations:** (1) `read` ignores the record → kills the record test and the hst refusal test; (2) record skip removed in `collect_into` → kills the native test; (3) `"UaeMetafile"` → `"UaeFsDb"` → kills the args test; (4) `write_record` accepting an empty map → kills `an_empty_record_is_never_written`.

- [ ] **Step 6: Commit** — `amiga_names.rs`, `native.rs`, `hst_imager.rs`; message "Escaped names travel in an ART-private record; hst-imager reads .uaem".

---

### Task 8: `content.rs` — classify, place, measure, check

**Files:**
- Create: `src-tauri/src/core/card/content.rs`; Modify: `src-tauri/src/core/card/mod.rs` (`pub mod content;` after `capacity`)
- Modify: `src-tauri/src/core/gameindex/readers/lhadrawer.rs` — factor `pub(crate) fn slave_drawers(entries: &[archive::ArchiveEntry]) -> Vec<String>` out of `read_archive_drawers` (`:192-202`: the `by_inner` keys, sorted, deduplicated); `read_archive_drawers` uses it
- Modify: `src-tauri/src/core/gameindex/readers/whdhdf.rs` — `pub(crate) fn fs_type_of(geometry: &VolumeGeometry) -> FileSystemType` from `:179-183`
- Modify: `src-tauri/src/core/preload/native.rs` — `pub(crate)`: `MAX_NAMED_NON_ASCII`, `pfs3_name_limit`, and a new `pub(crate) fn needs_latin1(name: &str) -> bool { !name.is_ascii() }` used by `non_ascii_entries`
- Modify: `src-tauri/src/core/preload/mod.rs` — `pub(crate) fn amiga_fold(name: &str) -> String`
- Modify: `src-tauri/src/core/error.rs` — three variants
- Test: inline in `content.rs`

**Interfaces:**
- Consumes: Tasks 1-7; `detect::detect`, `archive::open`, `whdload::analyse`/`Entry`/`PackLayout`, `whdhdf::read_whdload_hardfile`, `volume::mount::{scan_image, mount}`, `adf::fs::list_directory_on`, `sizing::ContentMeasure`, `copy::windows_safe_name`.
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Serialize)] #[serde(tag = "kind", rename_all = "kebab-case")]
  pub enum SourceKind { Folder, Archive { format: String }, WhdloadHardfile, Adf }

  #[derive(Debug, Clone, PartialEq, Eq, Serialize)] #[serde(tag = "reason", rename_all = "kebab-case")]
  pub enum Unusable {
      Missing,
      Unreadable { detail: String },
      ArchiveUnreadable { detail: String },
      HardfileNotWhdload { detail: String },
      NotAnAmigaSource { format_hint: String },
  }

  #[derive(Debug, Clone, PartialEq, Eq, Serialize)] #[serde(tag = "verdict", rename_all = "kebab-case")]
  pub enum Classified { Usable { kind: SourceKind }, NotUsable { why: Unusable } }

  pub fn classify(path: &Path) -> Classified

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Placed { pub index: usize, pub amiga_path: String }
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum ArchiveShape { Plain, Pack { drawer: String }, Collection { drawers: usize } }
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct ArchivePlacement { pub shape: ArchiveShape, pub placed: Vec<Placed>, pub left_behind: Vec<String> }
  pub fn place_archive(entries: &[ArchiveEntry]) -> CoreResult<ArchivePlacement>

  #[derive(Debug, Clone, Default, PartialEq, Eq)]
  pub struct SourceMeasure {
      pub content: ContentMeasure,
      pub non_ascii: Vec<String>, pub non_ascii_more: usize,
      pub too_long: Vec<String>, pub too_long_more: usize,
      /// Amiga paths whose host name will differ.
      pub escaped: Vec<String>, pub escaped_more: usize,
      pub top_level: Vec<String>,
      pub left_behind: Vec<String>,
  }
  pub fn measure(path: &Path, kind: &SourceKind, progress: &dyn ProgressSink) -> CoreResult<SourceMeasure>
  pub fn check_partition(sources: &[(PathBuf, SourceMeasure)]) -> CoreResult<()>
  ```
  New `CoreError` variants (with `Display` sentences that name what to do, and codes):
  - `SourceNamesCollide { name: String, first: String, second: String }` — "ART-CARD-SOURCE-COLLISION": "'{first}' and '{second}' both put '{name}' at the top of this partition. Rename one, or give them different partitions."
  - `EscapedNamesCollide { source: String, first: String, second: String, host: String }` — "ART-ESCAPED-NAME-COLLISION": "'{first}' and '{second}' in '{source}' would both be staged as '{host}' on Windows, so neither could be copied as itself. Rename one on the Amiga side first."
  - `NamesNoWriterCanCopy { non_ascii: Vec<String>, escaped: Vec<String> }` — "ART-NAMES-NO-WRITER": names both lists: the native PFS3 writer cannot write the first (ART-113) and hst-imager cannot rename the second (ART-160).

**Rules** (R1-3, R1-6, R2-8, R2-10, R3-8):
- `classify`: missing → `Missing`; directory → `Folder`; `detect` error → `Unreadable`; category `Archive` with hint `lha|zip|7z|lzx` → `archive::open` ok → `Archive { format }`, else `ArchiveUnreadable`; hint `adf` → `Adf`; hint `hdf` or `rdb` → `read_whdload_hardfile` ok **and** its `drawer` non-empty → `WhdloadHardfile`, else `HardfileNotWhdload` (an empty drawer means the slave is at the volume root: the game cannot be told from the boot scaffold); anything else → `NotAnAmigaSource { format_hint }`.
- `place_archive`: names normalised `\` → `/` and trailing `/` trimmed (level-0 LhA separators, R3 § 1 — placement only, never `decode_path`); `drawers = slave_drawers(normalised)`; none, or more than one with any at the archive root → `Plain` (every non-empty entry at its own path); exactly one → `Pack` via `whdload::analyse` over the normalised names (refuse `needs_installer` by name: "'…' is a WHDLoad install source; run its installer first"): entries under `layout.root` (or everything when `root` is empty) go under `layout.name`, `layout.icon` goes to `layout.icon_name()`, `left_behind = layout.outside`; two or more → `Collection`: tops = first components of the drawers; an entry whose first component is a top is placed at its own path, a root-level `<top>.info` too; every other first component is `left_behind` (sorted, once).
- `measure`: every placed/walked node goes through one tally keyed by Amiga path (explicit entries win over implied parents); at the end each node charges `ContentMeasure::add_*_with_comment` once, and each **leaf name** is checked: `needs_latin1` → `non_ascii`; `chars().count() > pfs3_name_limit(libpfs3::format::FORMAT_FNSIZE)` → `too_long`; `windows_safe_name(leaf) != leaf` → `escaped`; each list bounded by `MAX_NAMED_NON_ASCII` with a `*_more` count; `top_level` = distinct first components.
  - Folder: `native::collect_entries(path)` (make it and `CopyEntry`'s fields `pub(crate)`), comment bytes from each entry's sidecar (`uaem::parse`, bounded by `MAX_UAEM_BYTES`), `left_behind` empty.
  - Archive: `archive::open` → `entries` → `place_archive`; bytes = `declared_bytes`; comment = `amiga.comment` Latin-1 length (`chars().count()`, capped at 79).
  - WHDLoad HDF: `read_whdload_hardfile` → mount the one mountable volume → walk the drawer with `list_directory_on` (depth ≤ `MAX_COPY_DEPTH`, ≤ 100 000 nodes, else refuse by name) — the drawer as `<leaf>`, its contents under it, `<leaf>.info` when `game.icon` is `Some`; `left_behind` = every root name except the drawer's first component and the icon.
  - ADF: one file named by the file's own name, its size.
- `check_partition`, in this order: any `too_long` → `CoreError::Pfs3NamesTooLong { paths, more, max_bytes }` (no fallback exists, ART-314); a `top_level` name appearing in two sources under `amiga_fold` → `SourceNamesCollide`; any `non_ascii` **and** any `escaped` in the partition → `NamesNoWriterCanCopy`.
- `amiga_fold`: ASCII letters and Latin-1 `à..=þ` except `÷` to upper case; everything else unchanged.

- [ ] **Step 1: Failing tests** (`content.rs`; synthetic material only — `ArchiveEntry` lists for placement; `zip::ZipWriter`/`lzx` test builder for archives on disk; a WHDLoad HDF built with `make_ffs_volume` + `VolumeWriter` + `build_slave`, as `whdhdf.rs:315-390` does):

```rust
    fn file(name: &str, bytes: u64) -> ArchiveEntry { ArchiveEntry { name: name.into(), is_dir: false, declared_bytes: bytes, amiga: Default::default() } }

    #[test]
    fn a_single_title_archive_places_its_drawer_and_the_icon_beside_it() {
        let entries = [file("Games/Lotus3/Lotus3.slave", 600), file("Games/Lotus3/data/disk.1", 900), file("Games/Lotus3.info", 900), file("ReadMe.txt", 10)];
        let placed = place_archive(&entries).unwrap();
        assert_eq!(placed.shape, ArchiveShape::Pack { drawer: "Lotus3".into() });
        assert_eq!(placed.placed.iter().map(|p| p.amiga_path.as_str()).collect::<Vec<_>>(), ["Lotus3/Lotus3.slave", "Lotus3/data/disk.1", "Lotus3.info"]);
        assert_eq!(placed.left_behind, ["ReadMe.txt"]);
    }

    /// R1 § 6: WHDLoadDemos100 is 893 drawers under `Demos/<letter>/`; `analyse`
    /// alone would pick one demo and leave 892 behind.
    #[test]
    fn a_collection_goes_in_as_its_tree_and_root_files_stay_behind() {
        let entries = [file("InstallScript", 1232), file("Demos.info", 1494), file("Demos/A/One/One.slave", 600), file("Demos/A/One.info", 900), file("Demos/B/Two/Two.slave", 600)];
        let placed = place_archive(&entries).unwrap();
        assert_eq!(placed.shape, ArchiveShape::Collection { drawers: 2 });
        assert_eq!(placed.placed.iter().map(|p| p.amiga_path.as_str()).collect::<Vec<_>>(), ["Demos.info", "Demos/A/One/One.slave", "Demos/A/One.info", "Demos/B/Two/Two.slave"]);
        assert_eq!(placed.left_behind, ["InstallScript"]);
    }

    #[test]
    fn an_archive_with_no_slave_goes_in_whole() {
        let entries = [file("SnoopDos/SnoopDos", 100), file("SnoopDos.info", 10)];
        let placed = place_archive(&entries).unwrap();
        assert_eq!(placed.shape, ArchiveShape::Plain);
        assert_eq!(placed.placed.len(), 2);
    }

    /// R3 § 1: `BoingBag39-1.lha`'s 1 112 level-0 names use `\`.
    #[test]
    fn level_zero_backslashes_are_drawers_on_the_card() {
        let entries = [file(r"BoingBag3.9-1\C\Catalogs\dansk\Updater.catalog", 10)];
        let placed = place_archive(&entries).unwrap();
        assert_eq!(placed.placed[0].amiga_path, "BoingBag3.9-1/C/Catalogs/dansk/Updater.catalog");
    }

    #[test]
    fn measuring_finds_non_ascii_long_and_escaped_names_before_anything_is_written() {
        // folder source: "français/türkiye.country", "<107 × 'x'>", "Plain", and "_AUX" with a names
        // record {"_AUX": "AUX"} (Windows cannot create a file called AUX; the record is how a
        // staged folder says it) — the record itself is not measured
        // let m = measure(&dir, &SourceKind::Folder, &NoProgress).unwrap();
        // assert_eq!(m.non_ascii, ["français", "français/türkiye.country"]);
        // assert_eq!(m.too_long.len(), 1); assert_eq!(m.escaped, ["AUX"]);
        // assert_eq!(m.content.files, 4); assert_eq!(m.content.directories, 1);
    }

    #[test]
    fn a_whdload_hardfile_is_measured_as_its_drawer_and_icon_only() {
        // HDF: C/WHDLoad, Devs/Kickstarts/kick34005.A500 (a few synthetic bytes), s/startup-sequence, Disk.info,
        //      Lotus3HD/Lotus3.slave (build_slave), Lotus3HD/Disk.1, Lotus3HD.info
        // classify → Usable { kind: WhdloadHardfile }
        // measure → top_level == ["Lotus3HD", "Lotus3HD.info"]; files == 3 (slave, Disk.1, icon); directories == 1
        // left_behind == ["C", "Devs", "Disk.info", "s"]
    }

    #[test]
    fn classify_says_what_each_source_is_or_why_not() {
        // folder → Folder; zip → Archive{ "zip" }; lzx (Task 2 builder) → Archive{ "lzx" };
        // 901 120 zero bytes named x.adf → Adf; HDF without a slave → NotUsable{ HardfileNotWhdload }; missing → Missing;
        // a .txt → NotUsable{ NotAnAmigaSource }
    }

    #[test]
    fn two_sources_putting_the_same_name_at_the_top_are_refused_naming_both() {
        let a = SourceMeasure { top_level: vec!["Demos".into()], ..Default::default() };
        let b = SourceMeasure { top_level: vec!["DEMOS".into()], ..Default::default() };
        let err = check_partition(&[(PathBuf::from(r"E:\one.lha"), a), (PathBuf::from(r"E:\two.lha"), b)]).unwrap_err();
        assert!(matches!(err, CoreError::SourceNamesCollide { .. }));
        let text = err.to_string();
        assert!(text.contains("one.lha") && text.contains("two.lha") && text.contains("DEMOS"), "{text}");
    }

    #[test]
    fn non_ascii_and_escaped_names_in_one_partition_are_refused_naming_both() {
        let a = SourceMeasure { non_ascii: vec!["français".into()], top_level: vec!["x".into()], ..Default::default() };
        let b = SourceMeasure { escaped: vec!["AUX".into()], top_level: vec!["y".into()], ..Default::default() };
        let err = check_partition(&[(PathBuf::from("a"), a), (PathBuf::from("b"), b)]).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("français") && text.contains("AUX"), "{text}");
    }

    #[test]
    fn an_over_long_name_is_refused_with_no_fallback() {
        let a = SourceMeasure { too_long: vec!["x".repeat(107)], ..Default::default() };
        assert!(matches!(check_partition(&[(PathBuf::from("a"), a)]), Err(CoreError::Pfs3NamesTooLong { .. })));
    }

    /// Negative control: two sources with different names, ASCII only, pass.
    #[test]
    fn distinct_plain_sources_pass_the_partition_check() { /* top_level ["A"] and ["B"] → Ok(()) */ }
```

Write every commented body concretely.

- [ ] **Step 2: Run red.** `cargo test --lib core::card::content` → compile error; with stubs returning `Default`/`Plain`/`Ok(())`, each test fails on its first assertion.

- [ ] **Step 3: Implement** by the rules above. Module doc: the design § 7 paragraph it implements, R1/R2/R3 citations, why placement is decided from the listing (measure before unpack), and that `left_behind` is listed because "nothing is dropped silently".

- [ ] **Step 4: Green.** `cargo test --lib core::card::content`, `cargo test --lib core::gameindex`, `cargo test --lib core::preload`, `cargo test --lib core::error` → ok.

- [ ] **Step 5: Mutations:** (1) `place_archive` treats ≥ 2 drawers as `Pack` → kills the collection test; (2) drop the `\` normalisation → kills the backslash test; (3) `> limit` → `>= limit + 2` → kills the long-name measure assertion; (4) `amiga_fold` → identity → kills the collision test (`Demos` vs `DEMOS`); (5) `check_partition` checks `non_ascii || escaped` → kills the negative control; (6) HDF measure includes the root → kills the hardfile test.

- [ ] **Step 6: Commit** — `content.rs`, `card/mod.rs`, `lhadrawer.rs`, `whdhdf.rs`, `native.rs`, `preload/mod.rs`, `error.rs`; message "Card content: classify, place, measure and check a partition's sources".

---

### Task 9: `content.rs` — prepare into staging

**Files:**
- Modify: `src-tauri/src/core/card/content.rs`
- Modify: `src-tauri/src/core/volume/write/uaem.rs:280` — `days_from_civil` becomes `pub(crate)`
- Test: inline in `content.rs`

**Interfaces:**
- Consumes: everything from Task 8; `archive::extract::{extract_selection, Wanted, OverwritePolicy}`; `copy::{extract_from_volume, sidecar_for, EscapedName}`; `adf::fs::read_header_on`, `adf::extract::extract_file_on`; `amiga_names::write_record`; `clock::{AmigaClock, amiga_from_wall}`.
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
  pub struct Prepared {
      /// The folder the partition copy takes: the source itself for a folder, `staging` otherwise.
      pub root: PathBuf,
      pub staged: bool,
      pub sidecars_written: usize,
      pub escaped: usize,
      pub left_behind: Vec<String>,
  }
  pub fn prepare(path: &Path, kind: &SourceKind, staging: &Path, clock: &dyn AmigaClock, progress: &dyn ProgressSink) -> CoreResult<Prepared>
  ```

**Rules:**
- `staging` must exist and be empty, else `CoreError::SafetyRefused("'…' is not an empty folder; ART stages a source only into an empty one")` — except `Folder`, which ignores `staging` and returns `root = path`, `staged = false`.
- ADF: refuse a file over 4 MiB by name; `atomic_write(staging.join(windows_safe_name(file_name)), &fs::read(path)?)`; a record if the name was escaped.
- Archive: `archive::open` → `entries` → `place_archive`; each placed Amiga path becomes a host path by escaping **each segment** with `windows_safe_name`, recording `(host path so far, Amiga segment)` for every segment that changed; two placed paths with the same lower-cased host path and different Amiga paths → `EscapedNamesCollide`; `extract_selection(backend, &entries, &wanted, staging, OverwritePolicy::Skip, progress)`; `aborted`, any `errors`, or any skipped extracted entry → `CoreError::InvalidInput("'…' could not be unpacked for the card: {first reason}. Nothing from it will be copied.")`; then for each extracted entry (matched back through its host name): an archive-carried `*.uaem` must `uaem::parse` or the source is refused by name; otherwise, unless `<target>.uaem` already exists (the archive's own sidecar wins), write `sidecar_for(protection.unwrap_or(file::default_protection()), date, comment)` when it is `Some`, where the date is `EntryDate::MsDos(bits)` → `dos_wall_seconds(bits).map(amiga_from_wall)`, `EntryDate::Unix(s)` → `clock.amiga_from_unix(s)`; finally `write_record(staging, &names)` when `names` is non-empty.
- WHDLoad HDF: `read_whdload_hardfile` → mount → find the drawer's block by walking `game.drawer`'s segments with `list_directory_on` (case-insensitive) → refuse `EscapedNamesCollide` if any directory under the drawer (bounded as in Task 8) holds two names whose `windows_safe_name` lower-cases alike → `extract_from_volume(&device, &geometry, drawer_block, &staging.join(safe_leaf), true, OverwritePolicy::Skip, progress)`; `report.cancelled` → `CoreError::Cancelled`; `!report.is_complete()` → refuse naming `report.skipped[0]`; the drawer's own sidecar from `read_header_on(drawer_block)`; the icon `<leaf>.info` (found in the drawer's parent directory) through `extract_file_on(&device, &header, whdhdf::fs_type_of(&geometry))` → `atomic_write` + its sidecar; names record = the drawer leaf and icon if escaped, plus every `report.escaped` pair made relative to `staging` (`/`-joined); `left_behind` as `measure` computes it.
- `fn dos_wall_seconds(bits: u32) -> Option<i64>`: date = high word (year `1980 + (d >> 9)`, month `(d >> 5) & 0xF` in 1..=12, day `d & 0x1F` in 1..=31), time = low word (hour `t >> 11` ≤ 23, minute `(t >> 5) & 0x3F` ≤ 59, second `(t & 0x1F) * 2` ≤ 59), `days_from_civil(y, m, d) * 86_400 + h * 3600 + mi * 60 + s`; out of range → `None`.

- [ ] **Step 1: Failing tests.**

```rust
    #[test]
    fn a_dos_date_is_wall_clock_seconds() {
        assert_eq!(dos_wall_seconds((((45 << 9) | (1 << 5) | 1) << 16) | 0), Some(1_735_689_600));
        assert_eq!(dos_wall_seconds(0), None, "month 0 is not a date");
    }

    /// The end of R2's name problem, through the real copy: an archive entry
    /// called `AUX` is staged as `_AUX`, the record says `AUX`, and the native
    /// copy's entry list carries `AUX`.
    #[test]
    fn an_archive_entry_windows_refuses_reaches_the_copy_under_its_amiga_name() {
        // LZX (Task 2 builder, stored): "Drawer/AUX" (attrs 0x4F, comment "note"), "Drawer/Plain"
        // let prepared = prepare(&lzx, &SourceKind::Archive { format: "lzx".into() }, &staging, &UtcClock, &NoProgress).unwrap();
        // assert!(staging.join("Drawer/_AUX").is_file());
        // assert_eq!(AmigaNames::read(&staging).name_for("Drawer/_AUX"), Some("AUX"));
        // let entries = native::collect_entries(&staging).unwrap();
        // assert!(entries.iter().any(|e| e.relative == "Drawer/AUX"));
        // assert!(entries.iter().all(|e| !e.relative.contains(".art-amiga-names")));
        // let sidecar = uaem::parse(&fs::read_to_string(staging.join("Drawer/_AUX.uaem")).unwrap()).unwrap();
        // assert_eq!(sidecar.protection, 0x40); assert_eq!(sidecar.comment, "note");
        // assert!(!staging.join("Drawer/Plain.uaem").exists(), "default bits, no comment, no date: no sidecar");
    }

    /// The first of R3's claims measured end to end: bits, comment and date
    /// from an archive land on a PFS3 volume. Read back through libpfs3.
    #[test]
    fn archive_attributes_land_on_a_pfs3_partition() {
        // LHA level 1 via make_level1_lha (attr 0x20 → `--p-rwed`, DOS 2025-01-01) → prepare → NativeFormatter
        // format + copy_in(staging) on the PFS3 test image native.rs's tests use
        // → the entry's protection string is `--p-rwed` and its datestamp is pfs3_datestamp(amiga_from_wall(1_735_689_600))
    }

    #[test]
    fn a_whdload_hardfile_stages_only_its_drawer_and_icon() {
        // HDF as in Task 8, plus Lotus3HD/ReadMe.info with protection 0x80 (h) and Lotus3HD dir comment "drawer"
        // prepare → staging holds exactly {"Lotus3HD", "Lotus3HD.info", "Lotus3HD.uaem"} at its root
        // staging/Lotus3HD/ReadMe.info.uaem parses to protection 0x80
        // !staging.join("Devs").exists() && !staging.join("C").exists()   — the scaffold, and its Kickstart, stay behind (R1 § 6)
        // prepared.left_behind == ["C", "Devs", "Disk.info", "s"]
    }

    #[test]
    fn a_hardfile_whose_names_collide_once_escaped_is_refused_naming_both() {
        // HDF drawer Game/ holding "A?B" and "A_B" → Err(EscapedNamesCollide) naming "Game/A?B" and "Game/A_B"; staging stays empty
    }

    #[test]
    fn an_archive_s_own_sidecar_is_kept_and_a_broken_one_is_refused() {
        // ZIP (PC host, so no bits): "Game.slave" + "Game.slave.uaem" = "--p-rwed 2021-04-13 02:43:13.68 x\n"
        //   → staging/Game.slave.uaem unchanged (still has comment "x")
        // ZIP with "Bad.uaem" = "not a sidecar" → Err naming "Bad.uaem"
    }

    #[test]
    fn staging_that_is_not_empty_is_refused() { /* staging holds a file → Err(SafetyRefused); the file untouched */ }

    #[test]
    fn a_folder_is_used_where_it_is() { /* prepare(folder, Folder, staging) → root == folder, staged == false, staging still empty */ }
```

Write every commented body concretely.

- [ ] **Step 2: Run red.** `cargo test --lib core::card::content` → compile error; with `prepare` stubbed to `Err(NotImplemented)`, every new test fails on it.

- [ ] **Step 3: Implement** by the rules above.

- **Before Step 4 — the archive-gate question (R2 A4, "not run").** In a scratch test (not committed), extract through `extract_with_backend` an LZX entry named `AUX` into a scratch folder on E: and record what Windows did (error, device write, or a file). If it misbehaves, Task 12 files it for the gate's other callers; if Windows 11 writes an ordinary file, record that instead and file nothing.

- [ ] **Step 4: Green.** `cargo test --lib core::card::content` → ok.

- [ ] **Step 5: Mutations:** (1) escape only the leaf, not every segment → kills the `AUX` test when the drawer is renamed to `CON` in a second case (add it); (2) skip `write_record` → kills the `AUX` test; (3) overwrite an existing `.uaem` → kills the archive-sidecar test; (4) extract the volume root instead of the drawer → kills the hardfile test; (5) drop the collision walk → kills the collision test (the second file then shows as `skipped` and the refusal names only one); (6) `MsDos` treated as UTC (`clock.amiga_from_unix`) with a non-UTC `FixedClock` → kills `archive_attributes_land_on_a_pfs3_partition` when that test uses `FixedClock` with an offset (write it so).

- [ ] **Step 6: Commit** — `content.rs`, `uaem.rs`; message "Card content: prepare archives and WHDLoad hardfiles into staging with names and attributes".

---

### Task 10: A partition takes a list of sources

**Files:**
- Modify: `src-tauri/src/core/preload/mod.rs` — `PreloadPartition.content` (`:194-204`), `PreloadStep::CopyIn` (`:262-266`), `plan` (`:571-577`), `run` (`:721-728`), `step_label` (`:749`), `VolumeFormatter` (`:137-187`), new `check_source_collisions`; tests at `:1639, :1757, :1793`
- Modify: `src-tauri/src/core/preload/native.rs` — `impl VolumeFormatter for NativeFormatter` (`can_copy_in_sources`, `copy_in_sources`), `plan_copy` over a list, `copy_in_pfs3`/`copy_in_ffs` take `sources: &[PathBuf]` for their sentences
- Modify: `src-tauri/src/tools/hst_imager.rs` — `copy_in_sources`
- Modify: `src-tauri/src/commands/preload.rs` — `:370-378`, `:420-437`; test literals `:1019, 1039, 1092, 1240, 1737, 1805, 1967, 2408`
- Modify: `src/lib/preload.ts` — `PreloadPartition.content: string[]` (`:90`), `copy-in` step `sources: string[]` (`:146`), `toRequest` (`:472`), `stepPhrase` (`:646`); `src/lib/preload.test.ts` (`:153, :169-174, :268, :338`)
- Test: inline in `preload/mod.rs`, `native.rs`, `hst_imager.rs`; `src/lib/preload.test.ts`

**Interfaces:**
- Produces:
  ```rust
  pub struct PreloadPartition { …, #[serde(default, deserialize_with = "one_or_many_paths")] pub content: Vec<PathBuf> }
  PreloadStep::CopyIn { slot: Option<usize>, drive_name: String, #[serde(alias = "source", deserialize_with = "one_or_many_paths")] sources: Vec<PathBuf> }
  pub fn check_source_collisions(sources: &[PathBuf]) -> CoreResult<()>   // top-level Amiga names, amiga_fold, SourceNamesCollide
  trait VolumeFormatter {
      fn can_copy_in_sources(&self, image: &Path, slot: Option<usize>, drive: &str, sources: &[PathBuf]) -> CoreResult<()>
      { for s in sources { self.can_copy_in(image, slot, drive, s)?; } Ok(()) }
      fn copy_in_sources(&self, image: &Path, slot: Option<usize>, drive: &str, sources: &[PathBuf], sink: &dyn ProgressSink) -> CoreResult<CopySummary>
      { check_source_collisions(sources)?; let mut total = CopySummary::default(); for s in sources { total.absorb(&self.copy_in(image, slot, drive, s, sink)?); } Ok(total) }
  }
  ```
  `fn one_or_many_paths<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<PathBuf>, D::Error>`: `#[derive(Deserialize)] #[serde(untagged)] enum Shape { Many(Vec<PathBuf>), One(PathBuf) }`; `Option::<Shape>::deserialize(d)` → `None` → `vec![]`, `One(p)` → `vec![p]`, `Many(v)` → `v`.
  TypeScript: `content: string[]`; `{ step: "copy-in"; slot: number | null; drive_name: string; sources: string[] }`; `toRequest` maps `content: pick.content ? [pick.content] : []`; `stepPhrase` passes `source: step.sources.join(", ")` (no catalogue change).

- [ ] **Step 1: Failing tests.**

```rust
    // preload/mod.rs
    #[test]
    fn a_request_saved_with_one_folder_or_none_still_reads() {
        let one: PreloadPartition = serde_json::from_str(r#"{"area":1,"index":1,"volume_name":"Work","content":"E:\\tree"}"#).unwrap();
        assert_eq!(one.content, vec![PathBuf::from(r"E:\tree")]);
        let none: PreloadPartition = serde_json::from_str(r#"{"area":1,"index":1,"volume_name":"Work","content":null}"#).unwrap();
        assert!(none.content.is_empty());
        let many: PreloadPartition = serde_json::from_str(r#"{"area":1,"index":1,"volume_name":"Work","content":["E:\\a","E:\\b"]}"#).unwrap();
        assert_eq!(many.content.len(), 2);
    }

    /// R1-7: one copy step carrying the list — two steps would meet a volume
    /// that is no longer empty (`native.rs:954-960`).
    #[test]
    fn a_partition_with_two_sources_plans_one_copy_step() {
        // plan a request whose partition has content [a, b] (two scratch folders with distinct names)
        // assert exactly one CopyIn for that drive, with sources == [a, b]
    }

    /// R1-8: refused at planning — before any format runs.
    #[test]
    fn sources_that_collide_are_refused_before_the_plan_formats_anything() {
        // a/Demos and b/DEMOS → plan(..) is Err(SourceNamesCollide) naming both folders
    }

    // native.rs
    #[test]
    fn two_sources_fill_one_pfs3_partition() {
        // format a PFS3 test image; copy_in_sources(&[a, b]) where a holds A/One and b holds B/Two
        // → listing root has A and B, summary.files == 2
    }

    #[test]
    fn a_non_ascii_name_in_the_second_source_makes_the_whole_partition_fall_back() {
        // can_copy_in_sources(&[plain, with "français"]) → Err(NonAsciiPfs3Names) naming "français"
    }

    // hst_imager.rs
    #[test]
    fn every_source_is_checked_for_escaped_names_before_the_first_copy_runs() {
        // HstImager at a non-existent exe; copy_in_sources(&[plain, with_record]) → Err(EscapedNamesNeedNativeCopy) — no spawn attempted
    }
```

```ts
// preload.test.ts
it("sends a chosen partition's folder as a one-element list", () => {
  const picks = [{ area: 1, index: 1, driveName: "DH0", chosen: true, volumeName: "Work", content: "E:\\tree" }];
  expect(toRequest("E:\\c.img", null, picks, null).partitions[0].content).toEqual(["E:\\tree"]);
});
it("sends no folder as an empty list", () => {
  const picks = [{ area: 1, index: 1, driveName: "DH0", chosen: true, volumeName: "Work", content: null }];
  expect(toRequest("E:\\c.img", null, picks, null).partitions[0].content).toEqual([]);
});
```

- [ ] **Step 2: Run red.** `cargo test --lib core::preload`, `cargo test --lib tools::hst_imager`, `pnpm vitest run src/lib/preload.test.ts` (from the repository root) → compile/type errors, then assertion failures.

- [ ] **Step 3: Implement.**
  - `check_source_collisions`: for each source, `read_dir` its root (skip `*.uaem`, `BACKUP_DIR`, `AMIGA_NAMES_RECORD`), map through `AmigaNames::read(source).name_for(host)`, fold with `amiga_fold`, first repeat → `SourceNamesCollide { name, first, second }`.
  - `plan`: `if !wanted.content.is_empty() { check_source_collisions(&wanted.content)?; steps.push(PreloadStep::CopyIn { slot, drive_name, sources: wanted.content.clone() }); }`; `run`: `formatter.copy_in_sources(&plan.image, *slot, drive_name, sources, sink)?`.
  - `NativeFormatter`: `plan_copy_sources(image, slot, drive, sources)` = one card read + `collect_entries` for each source concatenated in order, after `check_source_collisions`; `can_copy_in_sources` runs today's `can_copy_in` checks over the concatenation; `copy_in_sources` calls `copy_in_pfs3`/`copy_in_ffs` once with every entry; `copy_in(source)` = `copy_in_sources(&[source.to_path_buf()])`, `can_copy_in(source)` likewise. Sentences that said `source.display()` now say every source (`'a', 'b'`).
  - `HstImager::copy_in_sources`: `check_source_collisions`, then the escaped-names refusal for **every** source, then one `fs copy` per source, then one listing.
  - `commands/preload.rs`: the two `CopyIn` matches use `sources` with `copy_in_sources` / `can_copy_in_sources`; test literals `Some(tree)` → `vec![tree]`, `None` → `vec![]`, `content, None` → `Vec::<PathBuf>::new()`.
  - TypeScript as in the interface block; existing test data updated.

- [ ] **Step 4: Green.** `cargo test --lib core::preload`, `cargo test --lib tools::hst_imager`, `cargo test --lib commands::preload`, `pnpm vitest run src/lib/preload.test.ts`, `pnpm lint` → all pass.

- [ ] **Step 5: Mutations:** (1) `plan` pushes one `CopyIn` per source → kills `a_partition_with_two_sources_plans_one_copy_step`; (2) remove `check_source_collisions` from `plan` → kills the collision test; (3) `can_copy_in_sources` checks only `sources[0]` → kills the fallback test; (4) hst checks escaped names inside the copy loop (after the first copy) → kills the hst test only if the test's first source is the plain one — keep that order; (5) `one_or_many_paths` without the `One` arm → kills the serde test.

- [ ] **Step 6: Commit** — the Rust and TypeScript files above; message "Preload: a partition's content is a list of sources, copied as one step, collisions refused first".

---

### Task 11: The owner's material — `#[ignore]` hooks and the LZX oracle

**Files:**
- Modify: `src-tauri/src/core/archive/lzx.rs` (hook), `src-tauri/src/core/card/content.rs` (hook)
- Create: `scripts/lzx-oracle-check.py`

**Interfaces:**
- Consumes: `LzxBackend`, `extract_with_backend`, `content::{classify, measure, prepare, check_partition}`.
- Produces: `read_the_owners_lzx_archives_when_asked` (`ART_LZX_DIR`, optional `ART_LZX_OUT`), `prepare_the_owners_card_material_when_asked` (`ART_CARD_HDF_A`, `ART_CARD_HDF_B`, `ART_CARD_COLLECTION`, `ART_CARD_LHA`).

- [ ] **Step 1: Write the hooks** — each `#[test] #[ignore]`, returning with an `eprintln!` when its variable is unset, doc comment with the exact command line (as `whdhdf.rs:558-575`):
  - LZX: for each `*.lzx` directly in `ART_LZX_DIR`: open, count entries, groups (records with `packed > 0`), non-ASCII names, comments; extract with `extract_with_backend` into `ART_LZX_OUT/<stem>` (or a scratch dir); print `"<file>: entries N groups G non-ascii A comments C written W errors E"`; assert `errors == 0` and `written == entries`.
  - Card: classify + measure + prepare `A Prehistoric Tale v1.1.hdf` and `B-17 Flying Fortress v1.0.hdf` into scratch staging (`ScratchDir::pair`): print `top_level`, `left_behind`, `sidecars_written`; assert B-17's staging has no `Devs` and no file named `kick*` anywhere (R1 § 6: the ROM stays behind). **Measure only** (never prepare) `WHDLoadDemos100.lha`: assert `Collection`, print drawer count and `left_behind` (R1-10: 917 MB is not unpacked). Measure `BoingBag39-1.lha` (`ART_CARD_LHA`): print `top_level` and the count of comment-carrying entries.

- [ ] **Step 2: Write `scripts/lzx-oracle-check.py`** (Write tool; module docstring in the shape of `scripts/iso-oracle-check.py`: what it proves, what it needs, "not in CI"): args `DIR` (holding `.lzx`) and `--out OUTDIR` (required, must not be on `C:`); env `ART_UNAR` (default `E:\amiga\ProjeART\build\tmp\card-r2\lzx\unar\unar.exe`). Steps: run `cargo test --lib read_the_owners_lzx_archives_when_asked -- --ignored --nocapture` from `src-tauri` with `ART_LZX_DIR=DIR`, `ART_LZX_OUT=OUTDIR\art`, `TMP`/`TEMP` on E:, output to a file, require `test result: ok`; run `unar -q -e ISO-8859-1 -o OUTDIR\unar\<stem> <archive>` per archive; compare the two trees by relative path (NFC) and SHA-256; print per archive `files A B identical I differ D only-art X only-unar Y`; exit 1 on any difference. No pipes on the commands whose status matters.

- [ ] **Step 3: Run them.** From `src-tauri` with `TMP`/`TEMP` on E::
  - `ART_LZX_DIR=E:\amiga\Amigatolon\paketler cargo test --lib read_the_owners_lzx_archives_when_asked -- --ignored --nocapture > E:\amiga\ProjeART\build\tmp\card-r2\hook-lzx.txt 2>&1` — expected, from R2 § A3: `boingbag1.lzx: entries 998 groups 68 non-ascii 36 …` and `WiFi_WPA_for_AmiKit_PiStorm.lzx: entries 54 groups 6 … comments 2`, errors 0.
  - `python scripts/lzx-oracle-check.py E:\amiga\Amigatolon\paketler --out E:\amiga\ProjeART\build\tmp\card-r2\lzx-oracle > E:\amiga\ProjeART\build\tmp\card-r2\lzx-oracle.txt 2>&1` — expected: both archives `differ 0`.
  - The card hook with `ART_CARD_HDF_A="E:\amiga\Amigatolon\WHDload\HDF_Games_WHDLoad_by_Enzo_[A]\A Prehistoric Tale v1.1.hdf"`, `ART_CARD_HDF_B` = B-17's path under `[B]`, `ART_CARD_COLLECTION=E:\amiga\Amigatolon\paketler\WHDLoadDemos100.lha`, `ART_CARD_LHA=E:\amiga\Amigatolon\paketler\BoingBag39-1.lha`.
  Record the literal result lines in the commit message. **A red hook is a finding, not something to tune away**: if LZX decoding fails on a real archive, stop, record the entry and the message, and report it — do not loosen a check to pass.

- [ ] **Step 4: Sweep.** `python scripts/control-byte-sweep.py` and `python scripts/scratch-guard-sweep.py` (no pipes) → clean.

- [ ] **Step 5: Commit** — `lzx.rs`, `content.rs`, `scripts/lzx-oracle-check.py`; message "Owner-material hooks for LZX and card content; LZX oracle against unar" + the result lines.

---

### Task 12: Records, and the whole suite twice

**Files:** `docs/ISSUES.md`, `docs/FEATURES.md`, `docs/STATUS.md`, `docs/session-log.md`, `CHANGELOG.md`; `THIRD_PARTY_LICENSES.md` and `CLAUDE.md` **only if** touched (they should not be: no crate added; the `libpfs3` vendoring rule in CLAUDE.md is unchanged).

- [ ] **Step 1: The full checks**, from `src-tauri` with `TMP`/`TEMP` on E:, each to its own file and read for `test result:`: `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`; `cargo test --lib` **twice** (`r2-full-1.txt`, `r2-full-2.txt`); from the root: `pnpm lint`, `pnpm test`, `python scripts/oracle-check.py`, `python scripts/control-byte-sweep.py`, `python scripts/scratch-root-sweep.py`, `python scripts/scratch-guard-sweep.py`, `python scripts/contrast-check.py --quiet`, `cargo deny check` (from where CI runs it). Quote the `test result:` lines.

- [ ] **Step 2: ISSUES.md.** Check the next free `ART-NNN` against the file first (the numbers below are the ones free on 2026-09-17 — a work list decays). New entries, each with where it was found and moved to Fixed with its test name:
  - ART-332 — the archive gate dropped Amiga protection bits, comments and dates for every format (R2 A4, R3) → `an_amiga_level_one_entry_carries_its_protection_byte_and_dos_date`, `archive_attributes_land_on_a_pfs3_partition`.
  - ART-333 — HDF extraction wrote no sidecar for directories (R2 B1) → `a_drawer_s_protection_date_and_comment_go_into_its_own_sidecar`.
  - ART-334 — `ExtractReport.renamed` held leaf-only display strings; escaped names from an HDF could not be restored (R2 B1) → `an_escaped_drawer_and_the_file_in_it_are_recorded_with_their_host_paths`.
  - ART-335 — the native PFS3 copy stamped "now" and dropped comments though the vendored writer could take dates (ART-116's "no fix available" was stale) → `copy_in_carries_the_date_and_comment_out_of_the_uaem_sidecars`.
  - ART-336 — hst-imager ignored every `.uaem` (default `UaeFsDb`), silently (R3 § 7, measured) → `a_copy_asks_hst_imager_to_read_uaem_sidecars`.
  - ART-337 — the native FFS copy dropped comments without counting them (R3 § 6) → `ffs_copy_in_carries_the_comment_of_a_file_and_a_drawer`.
  - ART-338 — only if Task 9's scratch check showed it: `extract_with_backend` writes Windows-reserved names as given for its non-card callers. Open, with the measured behaviour.
  - Update ART-116's entry: superseded by ART-335 for PFS3.

- [ ] **Step 3: FEATURES.md** — flip rows only where a test exists: LZX archives (read); card content classify/measure/prepare (core); several sources per partition (core); attributes carried to PFS3/FFS/hst-imager.

- [ ] **Step 4: STATUS.md** — snapshot numbers (test counts from Step 1) and the "Picking up next session" block **updated in place**: round 2 core done; round 3 = WHDLoad phase, `card_os_prepare`/`card_os_build`, `.partial`, end-to-end card; the owner's open confirmations from this plan's header.

- [ ] **Step 5: session-log.md** — one row at the top: the round, the tasks, the measured hook lines, survivors disclosed.

- [ ] **Step 6: CHANGELOG.md** — user-visible: LZX archives open; attributes (bits, comments, dates) reach Amiga volumes from archives, HDF drawers and folders with `.uaem`; hst-imager fallback now applies `.uaem`; a folder copied out of a volume gets a sidecar for each drawer; the preload request takes several folders per partition.

- [ ] **Step 7: Commit** — the docs by name; message "Card round 2: records".

---

## Self-review (done while writing)

- **Spec coverage:** design § 7 `content.rs` (classify · measure incl. non-ASCII · prepare, refuse by name) → Tasks 8-9; § 7 `core/preload` list of sources → Task 10; § 6 non-ASCII found in advance → Task 8; § 8.2 content tests (synthetic HDF drawer only, archive drawer + icon, folder as is, ADF as file, non-ASCII in advance) → Tasks 8-9; § 8.4 owner material → Task 11; § 8.6 mutations → every task. § 4 sizing is round 1's, extended by Task 6. § 5 phases 5-8, § 7 commands and frontend → rounds 3-4 (owner's decision 1).
- **Placeholders:** test bodies given as commented steps name the exact existing test whose setup they copy and the exact assertions; no "TBD".
- **Type consistency:** `AmigaAttributes`/`EntryDate` (Task 1) are what Tasks 2, 8, 9 read; `EscapedName`/`ExtractReport.escaped` (Task 4) are what Task 9 reads; `write_record`/`AMIGA_NAMES_RECORD` (Task 7) are what Tasks 9-10 use; `add_*_with_comment` (Task 6) is what Task 8 calls; `SourceNamesCollide` (Task 8) is what Task 10 returns.

## Decisions the owner should confirm

1. **Collisions are refused, not merged:** two sources that both have a `Demos/` drawer at the top are refused (naming both), even if their contents do not overlap.
2. **ZIP bits and comments are carried only from an Amiga-made ZIP** (host 1, UnZip's rule); a PC-made ZIP carries its date only. 7z carries its date only. The owner's ZIPs are all PC-made (R3 § 2).
3. **An LHA with no OS id is read as Amiga when it is called `.lha`** (XADMaster's guess): an MS-DOS LHA named `.lha` would have its DOS attributes read as Amiga bits.
4. **A `.uaem` inside an archive wins** over the sidecar ART would write from the archive's headers; a broken one refuses the source.
5. **Two behaviour changes outside the card:** hst-imager's fallback in the existing VolumePreload screen now applies `.uaem` (Task 7), and "copy out" of a volume folder now writes a `.uaem` beside each drawer that carries bits, a comment or a date (Task 4).
6. **Scope additions:** the FFS comment fix (ART-337) and the `libpfs3` comment patch (+art.12) are in this round because the owner's decision 8 asks for comments on the card.
7. **Escaping extends to archive entries on the card route** (decision 5 named HDFs): an archive entry called `AUX` is staged as `_AUX` and restored by the same record.
