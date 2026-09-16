# One-button card, round 2 — research before the plan

*2026-09-16, branch `art-card-round-2` (from `main` dbc0566). Research only: no production code
touched. Describes the tree on this day.*

## Question

Round 2 is `core/card/content.rs` (classify · measure · prepare a source) and `core/preload`
taking a list of sources per partition. The design
(`docs/superpowers/specs/2026-09-11-one-button-card-design.md`) is five days old and the tree has
moved (libpfs3 `0.1.3+art.1` → `+art.11`, ART-242, ART-310…327). Before the plan: does the design
still hold, what already exists that must be reused, and what do real WHDLoad HDFs and archives
look like?

## Findings

### 1. Entry size and the sizing mirror — still matches; the citations are stale

- `src-tauri/vendor/libpfs3/Cargo.toml:6` is `0.1.3+art.11`.
- `build_dir_entry` is now `vendor/libpfs3/src/writer.rs:2104-2151`. Entry =
  `extra_fields_offset(nlen, 0)` + `ExtraFields::encode()`.
  `extra_fields_offset` = `(20 + nlen + clen) & !1` (`ondisk/direntry.rs:106-108`); `encode()`
  emits each non-zero word plus the 2-byte flags word (`direntry.rs:181-206`). With no `fsizex`
  (always, below 4 GiB; `fsizex` only on a largefile volume, `writer.rs:2123-2127`) the entry is
  `((20+n)&!1) + 2`. Even n → `22+n`; odd n → `21+n`. `sizing.rs:151-152`
  (`18 + n + 1 + 2`, padded even) gives `22+n` / `21+n`. **Byte for byte the same.**
- Packing rule: `writer.rs:2061` `pos + entry_bytes.len() < self.resblocksize` — strictly less,
  as `sizing.rs:186` (`- 1`) assumes.
- Anode ceiling: `writer.rs:1842-1850` `anode_block_limit` (small: `(MAXSMALLINDEXNR+1) * ipb`,
  large: `(MAXSUPER+1) * ipb²`, both `min(1<<16)`) = `sizing.rs:248-260` `pfs3_anode_cap`.
- `alwaysfree`: still written only by `format.rs:239` (`data_blocks / 20`), read nowhere else in
  `vendor/libpfs3/src` (grep) — the design's "estimate keeps the 5 % the writer does not enforce"
  still applies.
- **Ran:** `cd src-tauri && cargo test --lib card::sizing` (TMP/TEMP on E:) →
  `test result: ok. 26 passed; 0 failed; … 3577 filtered out; finished in 18.37s`. This includes
  the calibration tests that fill a real `libpfs3::writer::Writer` in memory
  (`sizing.rs:669-685` `fill`) and check one cylinder smaller fails —
  `a_tree_shaped_like_the_owners_fits_its_estimate`, `many_small_files_do_not_run_out_of_reserved_blocks`,
  `large_directories_with_long_names_fit_their_estimate`, `many_files_stay_in_small_mode_and_fit_their_estimate`,
  `the_ported_reserved_area_matches_libpfs3s_own_format`, `the_anode_ceiling_is_pfs3aios`.
- Stale: the doc comments in `sizing.rs:143-147, 176` still cite `writer.rs:1153-1194`,
  `1166-1169`, `1115` (0.1.3 line numbers). Correct, not wrong in substance.
- Subtle: sizing counts `name.chars()`, the writer `name.len()` (UTF-8 bytes). Equal for ASCII;
  for a non-ASCII name the native writer never runs (finding 2) and the Amiga stores Latin-1, one
  byte a char, so `chars()` is the right count for the hst-imager path too.

### 2. Non-ASCII names — the gap is still open; one refusal falls back, the other does not

- ART-113 ✅ in `docs/ISSUES.md:15712` means **"refused by name"**, not fixed.
  `vendor/libpfs3/src/writer.rs:2112,2150` still writes `name.as_bytes()` (UTF-8) for a new entry;
  only `rename_in` writes Latin-1 (`writer.rs:1237-1244`, M1).
- The rule today: `core/preload/native.rs:719-750` `non_ascii_entries` / `non_ascii_refusal`
  (bounded to `MAX_NAMED_NON_ASCII = 20`, `native.rs:709`) runs in `can_copy_in`
  (`native.rs:248-270`) and in `copy_in_pfs3` before the volume is opened (`native.rs:942`).
- `commands/preload.rs:261-269` `FallbackReason::from_native_error` maps **only**
  `CoreError::NonAsciiPfs3Names` to a fallback. `long_name_refusal` (ART-314, name > `fnsize-1`
  = 106 bytes, `native.rs:259-262`) is **not** a fallback reason — it is a plain refusal.
- `run_with_fallback` (`commands/preload.rs:483`) is private to `commands/preload.rs`; its product
  caller is `preload_run` (`:742`). The choice is per partition, made before the format through
  `paired_copy_forces_fallback` (`:415-442`) — which asks `can_copy_in` of the **first** `CopyIn`
  step for that drive only (`find_map`, `:427-434`).
- `libpfs3` is now ART's own vendored copy (ART-310, CLAUDE.md), so the ART-113 doc's "cannot be
  repaired through the crate's public API" (`native.rs:63-66`) is no longer a hard wall — a
  Latin-1 `build_dir_entry` is a vendor patch away. Not this round's scope; recorded.
- **What content.rs must do:** measure non-ASCII leaf names per source with the *same* predicate
  (`!leaf_name(..).is_ascii()`) and the same 20-name bound — reuse, not a copy (make
  `non_ascii_entries` or a name-level helper `pub(crate)`); also measure over-long names (> 106
  bytes), which have no fallback and must be a refusal in phase 4.

### 3. WHDLoad overlap — three pieces already exist

- `core/whdload/mod.rs:134` `analyse(&[Entry]) -> PackLayout { root, name, slave, icon, outside,
  needs_installer }` — pure, over a name list; the drawer is the slave's parent, the icon is
  `<name>.info` **beside** it (`find_icon`, `:209`). Bounds `MAX_SLAVE_DEPTH = 3`,
  `MAX_ENTRIES = 20_000`. **It picks one slave** (shallowest, shortest name, `pick_slave` `:190`):
  a collection archive is analysed as one pack with everything else `outside`.
- `core/whdload/install.rs:206-367` `build_plan_with_scratch`: `unpack_for_install` → host `walk`
  (`:477`) → `analyse` → `safe_join(scratch, layout.root)` → `HostFolder` + icon. That is exactly
  "archive → drawer + icon in staging"; content.rs should call `analyse` over the same `walk` and
  not re-derive the drawer (the `walk` is private — needs `pub(crate)`).
- `core/gameindex/readers/whdhdf.rs:156` `read_whdload_hardfile(path) -> HardfileGame { drawer,
  slave_name, slave, icon }`: mounts a bare/RDB image, walks to depth 3 ≤ 20 000 entries, accepts
  AmigaDOS's **30-char truncation** (`.Slav`, `names_a_slave` `:129-146`, 32 of 1697 real HDFs),
  and lets the slave's bytes decide (`read_slave`). `analyse`'s `has_extension(.., "slave")` does
  **not** accept `.Slav` — so for an HDF, the HDF reader is the one to reuse, not `analyse`.
- `core/gameindex/readers/lhadrawer.rs:179` `read_archive_drawers` lists **every** drawer in an
  archive without unpacking (excludes `data` components, settles ties by icon, cancellable) —
  the right way to tell "one pack" from "a collection" before unpacking.

### 4. The archive gate — `unpack_for_install` is LHA-only; the real gate is one level down

- The one gate: `core/archive/extract.rs` `extract_with_backend` (`:159`) — module doc `:1-24`
  names it "the one gate every archive's bytes pass through": `safe_join` per entry, total output
  `MAX_TOTAL_OUTPUT = 2 GiB` (`:39`), per entry `MAX_ENTRY_OUTPUT = 256 MiB` (`:47`),
  `MAX_ENTRIES = 100_000` (`:50`), declared-size lies refused, `OverwritePolicy` (default Skip).
- Format dispatch: `core/archive/mod.rs:110-140` `open` → `detect` by bytes → `lha` / `zip` /
  `7z`; anything else `UnsupportedFormat`.
- `core/sources/install.rs:62` `unpack_for_install` → `core/lha/safe_extract.rs:34`
  `extract_archive_with` → **`LhaBackend::open`** (`:40`). So `unpack_for_install` refuses a ZIP
  or 7z, and it makes its own `Scratch` under a root rather than using a caller's directory. The
  doors that dispatch all three: `archive::open` + `extract_with_backend`, as
  `core/amigainstall/packagevol.rs:506` and `core/layout/apply.rs:210` already do.
- **LZX: not supported.** No decoder in `Cargo.toml`/`core` (grep `lzx` in `src/core` hits only
  `sources/bundle` package names like `util/arc/lzx121r1`); `detect.rs` has no LZX signature. Real
  LZX exists in the owner's material (`E:\amiga\Amigatolon\amikitdev\WiFi_WPA_for_AmiKit_PiStorm.lzx`).

### 5. `extract_from_volume`

- `core/volume/write/copy.rs:889-918`:
  `extract_from_volume<D: BlockDevice + ?Sized>(device, geometry, dir_block: u32, dest: &Path,
  write_sidecars: bool, policy: OverwritePolicy, sink) -> CoreResult<ExtractReport>`.
- It copies **the contents of** `dir_block` (0 = root) into `dest` — not the directory itself and
  not a single file. The drawer's `.info` beside it needs a separate read
  (`core/adf/extract.rs:28` `extract_file_on(device, header, fs_type)`, as `whdhdf.rs:258` uses).
  The caller finds the drawer's block by name (`adf::fs::list_directory_on`); `commands/
  volume_write.rs:1677-1703` `copy_out_folder` is the existing shape (mount → extract into
  `dest/<name>`).
- Opening: `core/volume/mount.rs:143` `scan_image` handles **RDB and bare** (`DOS`, and names
  `PFS`/`PDS`/`SFS` as unsupported), `:373` `mount`. OFS/FFS only, 512-byte blocks only
  (`why_not`, `mount.rs:288-296`).
- `ExtractReport` (`copy.rs:788-797`) carries `renamed` (NTFS-escaped names, `amiga → windows`)
  and `skipped`. `.uaem` sidecars (when `write_sidecars`) are read back by the native copy
  (`native.rs:1067-1070, 1189-1192`) and never copied as files (`native.rs:863`) — so sidecars
  round-trip protection/comment/date. **Escaped names do not round-trip:** `AmigaNames`
  (`core/preload/amiga_names.rs`, ART-160) only reads a tree's `distribution.json`.

### 6. The medium's own root listing

Scratch copies under `E:\amiga\ProjeART\build\tmp\card-r2\` (md5 of the Prehistoric Tale copy
equals the original, `43300b48…`). Enzo's folders: `E:\amiga\Amigatolon\WHDload\
HDF_Games_WHDLoad_by_Enzo_[#]`, `[A]` … (204 files in `[A]`). Magic of all three:
`44 4f 53 01` — **bare FFS, no RDB, 512-byte blocks.** `xdftool <hdf> list`, root and one level:

```
A Prehistoric Tale v1.1.hdf   (943 616 bytes)
A.Prehistoric.Tale   VOLUME  DOS1:ffs #512
  C                  DIR      Assign 3220 · SetPatch 14868 · WHDLoad 148547
  C.info             4434
  Devs               DIR      Keymaps/ · Keymaps.info
  Devs.info          628
  Disk.info          4116
  PrehistoricTale    DIR      Disk.1 681472 · PrehistoricTale.info 12577 · PrehistoricTale.slave 628 · ReadMe · ReadMe.info
  PrehistoricTale.info 900
  s                  DIR      startup-sequence 62 · startup-sequence.info · WHDLoad.prefs 1603
  s.info             2100
  Test               DIR      (empty)
  Test.info          2657

A-10 Tank Killer v2.0 2-Disk.hdf   (2 044 416 bytes)
A-10.Tank.Killer(2.Disk)   VOLUME  DOS1:ffs #512
  A10TankKiller2Disk        DIR   A10TankKiller2Disk.info 8098 · A10TankKiller2Disk.Slave 3656 · data/ · Manual · Manual.info · ReadMe · ReadMe.info
  A10TankKiller2Disk.info   900
  C  DIR (Assign, SetPatch, WHDLoad 148547) · C.info
  Devs DIR (Keymaps/, Keymaps.info, Kickstarts/) · Devs.info
  Disk.info · s DIR (startup-sequence 68, .info, WHDLoad.prefs) · s.info · Test DIR · Test.info

B-17 Flying Fortress v1.0.hdf
B-17.Flying.Fortress   VOLUME  DOS1:ffs #512
  B17FlyingFortress        DIR   B17FlyingFortress.info 20025 · B17FlyingFortress.Slave 4440 · game/ · ReadMe · ReadMe.info
  B17FlyingFortress.info   900
  C DIR · C.info · Devs DIR (Keymaps/, Kickstarts/: kick34005.A500 262144, kick34005.A500.PAT 52, kick34005.A500.RTB 4020) · Devs.info
  Disk.info · s DIR · s.info · Test DIR · Test.info
```

`s/startup-sequence` of B-17: `CD B17FlyingFortress` / `WHDLoad B17FlyingFortress.Slave Preload`
(Prehistoric Tale the same shape). Observations: the scaffold is `C Devs s Test` + their `.info`
+ `Disk.info`; the game drawer is at the root with a 900-byte drawer icon beside it; the slave is
spelled `.slave` or `.Slave`; **the HDF carries `C/WHDLoad` (148 547 bytes, same size in all
three) and, in two of three, a Kickstart ROM in `Devs/Kickstarts`** — the scaffold ART leaves
behind contains a copyrighted ROM, so "left behind" is also a legal line, not just tidiness.

Archive: `E:\amiga\Amigatolon\paketler\WHDLoadDemos100.lha` (663 506 299 bytes), `7z l`:
`Type = Lzh`, **8 860 files, 917 116 059 bytes unpacked**, **893 slaves, every one at
`Demos\<letter>\<title>\<title>.slave`**, no non-ASCII names, longest path ≈ 67 chars. Root:

```
InstallScript          1232
Demos.info             1494
Demos\0-9.info · Demos\0-9\ · Demos\A.info · Demos\A\ · … · Demos\Z\
Demos\0-9\1001StolenIdeas.info  900
Demos\0-9\1001StolenIdeas\{1001StolenIdeas, 1001StolenIdeas.info, 1001StolenIdeas.slave, ReadMe, ReadMe.info}
```

It is a **collection**, not a pack. A single-title WHDLoad `.lha` was not found in a bounded search
(`find -maxdepth 5/6` over `Titles`, `ProjeART`, `Shared`, `Amigatolon`; `Titles` is 847 ADF +
242 rp9; `Amigatolon\paketler` holds 68 files, only this one WHDLoad).

### 7. How the established projects do it

**Emu68-Imager** (`mja65/Emu68-Imager-Software` @ 428c3cf): a partition's user content is **one
host folder** — `Assets/UIActions/DiskPartition/WPF_DP_Button_ImportFiles.ps1:18-27` asks for a
folder, sums its bytes and refuses if larger than the partition; `Assets/Functions/
ProcessInstallFiles/Get-CopyFilestoAmigaDiskCommands.ps1:42` copies it with
`hst.imager fs copy "<interim>\<Disk>\*" <dest> --recursive TRUE --uaemetadata UaeFsDb`.
Separately, a partition can be *imported whole* from an existing disk/HDF
(`ImportedPartitionMethod` `Direct`/`Derived`, `Assets/Functions/SetupDisk/New-GUIPartition.ps1`),
and then files cannot be added to it. No drawer is extracted from a WHDLoad HDF and no user
archive is unpacked; Kickstarts reach `DEVS:Kickstarts` at boot via its `PiStorm/TransferKick`
ARexx script. <https://github.com/mja65/Emu68-Imager-Software>

**emu68hatcher** (`rootrootde/emu68hatcher` @ 8d2cef4): each Amiga partition has one optional
`extra_content_directory: Path` (`config/partition_models.py:39`);
`builder/pipeline/install_extras.py:44-86` mirrors it into `staging/<device>/` with
`copy_contained_tree` (`builder/staging/tree_copy.py:98`, symlink-contained walk, measured first,
`_check_partition_space` `:28-43` refuses by name with both sizes). Archives are unpacked only for
its own package catalogue (`builder/pipeline/extract.py`); no WHDLoad HDF handling (repo tree has
only `data/packages/whdload.yaml`, `whdloadwrapper.yaml`). <https://github.com/rootrootde/emu68hatcher>

Neither extracts drawers from HDFs or unpacks user archives: **ART's content step goes beyond both
prior-art tools**, so there is no outside behaviour to copy — only the "folder per partition,
measured and refused by name before writing" shape, which ART already has.

## Eliminations

- *sizing.rs drifted from the writer* — eliminated: arithmetic re-derived from `writer.rs:2104-2151`
  + `direntry.rs:106,181` and the 26 sizing tests (with real-writer calibration) pass today.
- *ART-113 is fixed* — eliminated: ✅ is "refused by name"; `writer.rs:2150` still UTF-8.
- *`unpack_for_install` is the gate for all formats* — eliminated: LHA only (`safe_extract.rs:40`).
- *LZX is supported* — eliminated: no decoder, no detect signature.
- *`extract_from_volume` extracts a drawer with its icon* — eliminated: it copies a directory's
  children into `dest`; the icon beside the drawer is another read.
- *WHDLoad HDFs have an RDB* — eliminated for the 3 sampled (bare `DOS\1`); `whdhdf.rs:3-5` says
  1697 of the owner's files are this shape (not re-counted here).
- *`whdload::analyse` can find the drawer in an HDF* — eliminated for truncated names (`.Slav`);
  `whdhdf::read_whdload_hardfile` is the reader that handles it.
- *Emu68-Imager / emu68hatcher extract WHDLoad drawers* — eliminated (finding 7).

## What the plan must change vs the design

1. **Archive gate:** content.rs unpacks through `core::archive::open` + `archive::extract::
   extract_with_backend` into the caller's staging dir — not `unpack_for_install` (LHA-only, own
   scratch). If `unpack_for_install` is to stay the named door, it must first be made to dispatch
   by `archive::open`; either way one gate, and the design's "archive gate" row names this function.
2. **LZX is "not usable, with reason"** (a typed refusal naming the file and "ART cannot read LZX"),
   not a supported archive kind; the design's `.lzx` in the classify list is dropped or refused.
   Bounds to state in refusals: 2 GiB total, 256 MiB per entry, 100 000 entries per archive.
3. **Two archive kinds, not one:** decide *pack* vs *collection* before unpacking with
   `gameindex::readers::lhadrawer::read_archive_drawers` (or a slave count). One drawer →
   `whdload::analyse` layout (drawer + beside-icon, `outside` left behind and listed). Many drawers
   (WHDLoadDemos100: 893) → the archive tree placed as is (`Demos/` + `Demos.info`), root files
   such as `InstallScript` listed as left behind. `analyse` alone would silently pick one demo.
4. **WHDLoad HDF:** find the drawer with `gameindex::readers::whdhdf::read_whdload_hardfile`
   (handles `.Slav` truncation and bare/RDB), then `mount` + look up the drawer's block +
   `extract_from_volume(dir_block, staging/<drawer>, write_sidecars: true, ..)` + the beside-icon via
   `extract_file_on`. Do not re-implement slave finding. Scaffold left behind = `C`, `Devs`
   (incl. `Devs/Kickstarts` ROMs), `s`, `Test`, `Disk.info`, their `.info`.
5. **Escaped names from an HDF:** `ExtractReport.renamed` must not be ignored — either refuse the
   HDF naming them, or record the Amiga names in a form `preload::amiga_names::AmigaNames` reads
   for the staging tree; otherwise `_AUX`-style host names reach the card (ART-160's defect shape).
6. **Measure names with preload's own rules:** non-ASCII via the predicate in
   `native.rs:719` (made `pub(crate)`, not copied) and over-long names via `pfs3_name_limit`
   (`native.rs:259-262`). Non-ASCII → fallback-eligible (ART-113); over-long → **refusal with no
   fallback** (not in `from_native_error`). The design's § 6 only names the first.
7. **Several sources into one partition cannot be several `CopyIn` steps:** `copy_in_pfs3` refuses a
   non-empty volume (`native.rs:954-960`), and `paired_copy_forces_fallback` only checks the first
   `CopyIn` per drive (`commands/preload.rs:427`). The plan needs one copy step carrying the source
   list — `PreloadPartition.content: Vec<PathBuf>` (serde: accept the old `Option<PathBuf>`/`null`
   shape or update `src/lib/preload.ts:90` in the same commit), `PreloadStep::CopyIn { sources }`,
   `plan_copy`/`collect_entries` over all sources, `can_copy_in` over all sources (the
   `VolumeFormatter` trait signature changes, and `tools/hst_imager.rs` with it).
8. **Collisions across sources are refused before the format**, case-insensitively (AmigaDOS), by
   name and by which two sources — the writer would otherwise stop mid-copy with `AlreadyExists`
   (ART-326, `writer.rs:401`) after the partition is erased.
9. **Fix the stale citations** in `sizing.rs:143-147,176` to `writer.rs:2104-2151, 2061` when the
   file is next touched (no behaviour change).
10. **The `#[ignore]` hook's material:** use `[A]`'s `A Prehistoric Tale v1.1.hdf` (no ROM in
    scaffold) and `B-17 Flying Fortress v1.0.hdf` (ROM in scaffold, proves it is left behind), and a
    subset of `WHDLoadDemos100.lha` — which as a whole is 917 MB unpacked, so the hook must not
    unpack it all (token/time economy and "toplu iş varsayılan değil").

## What this does not prove

- Entry-size agreement is arithmetic plus the existing calibration tests; no new card was
  formatted and read back by hst-imager or pfs3aio today.
- Three HDFs from `[A]`/`[B]` are not 1697; the bare-FFS shape of the rest is `whdhdf.rs`'s claim,
  not re-measured. Multi-volume or RDB WHDLoad HDFs were not looked for.
- No single-title WHDLoad `.lha` was found in a bounded search, so the one-pack archive shape is
  known from `whdload/mod.rs`'s doc and tests, not from the owner's material today. No ZIP/7z
  WHDLoad archive was examined.
- `7z l` truncates names containing spaces in the `awk $NF` counts; the slave count (893, via
  `grep -ic '\.slave$'` on whole lines) is unaffected, the max-path estimate is approximate.
- The prior-art reading is a code search at the named commits, not a run of either tool.
