# Debt round after 0.9.3 — research (2026-09-14)

*Collected 2026-09-14 by four read-only research agents over this repository and one over pfs3aio master
`211f7f0` (https://github.com/tonioni/pfs3aio) and hst-amiga `main`. Nothing here was run unless it says so.
The owner's decisions of the same day: D7 local time everywhere; ART-311 both modes; D4 implement the deldir;
D14 format `fnsize` 107; separate plans, executed by subagents.*

## Facts gathered (cite in the plan)

- ART-118 (docs/ISSUES.md:60-101) names `src/components/osbuilder/OsInstall.tsx` and `OsInstall.test.tsx`;
  both are gone (Glob on 2026-09-14: no `OsInstall*` under `src/components/osbuilder/`). STATUS.md:237 records
  the four-tab rewrite deleting `OsInstall.tsx`, `PackagePanel.tsx`, `FirstBootPanel.tsx`.
- The crash ART-118 describes was a headless Chrome/Edge renderer access violation (`-1073741819`), never
  seen in ART's own window (WebView2).
- The screen that replaced it was driven by the owner in packaged builds and produced defects, not crashes:
  ART-297 (2026-09-10, `main-f354f46`, ISSUES.md:729-731), ART-303 (2026-09-10, `main-e2633a6`, :701-703),
  ART-304 (2026-09-11, `main-706de9f`, :632-634). The single-column screen before it: session-log.md:126-128
  (2026-08-22, "The owner drove the OS Builder").
- Successor coverage: `FilesTab.test.tsx`, `ChoiceTab.test.tsx`, `MachineTab.test.tsx`, `BuildTab.test.tsx`
  (plus panels) under `src/components/osbuilder/`.
- STATUS.md:391-395 still lists ART-118 as one of two person-owed items — stale.
- ART-117 (ISSUES.md:103-153): decided by the owner 2026-08-21, "leave it", hst-imager the named fallback;
  "Revisit only if someone actually meets the case". No precedent in ISSUES for closing an entry by decision
  (grep for "by decision / not a defect / won't fix": no match).
- ART-250 (ISSUES.md:197-243): open by design; "No tree ART builds writes new NewIcon tool types today."
- House pattern for an entry whose file the rewrite deleted: ISSUES.md:1068 ("Where it lived when it was fixed").

## PFS3 writer research (Explore agent, 2026-09-14)

- D3: root dir block parent = ANODE_ROOTDIR at `format.rs:321-326`; nothing in libpfs3 reads a dir block's
  parent (`ondisk/direntry.rs:8-34` parses it, unused). `create_dir_in` writes the real parent (`writer.rs:178`);
  **overflow dir blocks get the directory's own anode** (`writer.rs:1135`) — verify against pfs3aio.
- D8: `writer.rs:776-786` bound `disksize + bitmapstart`; valid data blocks are `[bitmapstart, disksize)`
  (`format.rs:120,128,193`). `load_data_bitmap` sized from disksize (`:745-747`); `free_data_block` checks only
  `bm_idx < len` (`:839`). ART's format clears tail bits (`format.rs:268-278`) → not reachable on ART-formatted
  volumes. Out-of-partition write refused by `FileRegionMut::position` (`core/volume/device.rs:336-370`) →
  `Pfs3Error::Io` (`pfs3dev.rs:129-131`). Severity 🟡 (error, not corruption).
- ART-311: loop `0..256u32` (`writer.rs:904`, DiskFull `:950`); small-mode DiskFull `:1018-1028`; large-mode
  IB allocation `:994-1009`, no new SB (`:982-983`). `update_rootblock` (`:1206-1246`) never writes
  `rootblock.indexblocks`; no serializer for Rootblock/RootblockExt; `AnodeReader` clones indexblocks/superindex
  (`anode.rs:41-42`, `volume.rs:61`) → stale after growth. Constants `ondisk/mod.rs:43-50,69-72,112,160,185`.
  Sizing cap `sizing.rs:240-245`, used `:265-269`; only `many_files_cross_into_superindex_mode` (`:938`) depends on
  it; its doc `:913-936` and `:219-239` line refs are already off.
- D4: `FormatOptions { volume_name, enable_deldir }` `format.rs:25-28`, never read; options `:101-111`;
  writer deldir path returns early `writer.rs:661`; callers pass false (`native.rs:189-192`, `sizing.rs:666`).
- D14: fnsize 32 at `format.rs:239`; writer truncates silently `nlen = len.min(107)` `writer.rs:1165`; ART checks
  only non-ASCII (`native.rs:839`), 30-char `check_name` only for the label (`native.rs:170`).
- Test devices: `MemDevice` private in `sizing.rs:614-657` (sparse, no bounds); `VecDevice` +
  `ArtBlockDevice` (`core/volume/device.rs:213/230`, `pfs3dev.rs:278`) refuses past its end.

## ART-302 / ART-300 research (Explore agent, 2026-09-14)

- ART-302: `ShaMemo` `scan.rs:508-548` (static, in-memory; doc :503-506); `dedupe_identical_disks` `:437`,
  `_with` `:450`; SHA-256 `file_sha256` `:551`; callers `scan.rs:329,356`, `commands/osinstall.rs:974`
  (`osinstall_slots` builds `ScanCache::in_dir(scratch::root()?)` at `:948`, not passed to dedupe).
  `CacheFile` `scan_cache.rs:187-209` (schema 2, md5 `#[serde(default)]`, md5 documented "table lookup only" →
  add `sha256: Option<String>` `#[serde(default)]`, no schema bump, note why; fallback builder `:393-400`).
  Tests `scan.rs:1060,1084,1098`; `scan_cache.rs:666-1165`.
- ART-300: refusal `apply.rs:2053-2061` (`SafetyRefused`); `undeclared_overwrites` `:2296-2333` finds owner
  component then discards it; `chain.rs:856-875` already finds packages whose `overrides` contain an id
  (`OvertakenBy`); fixtures `core/osinstall/mod.rs:1581-1748` (`package_test_package_two` overrides test-package);
  tests `apply.rs:6892` (contains OVERWRITTEN_PATH), `:6938` (unrecorded: contains MANIFEST_FILE_NAME, not
  "overrides"), `:7105`, `:7192`.

## pfs3aio source (general-purpose agent, pfs3aio master 211f7f0, 2026-09-14)

- D3: root's own dir blocks parent 0 (`format.c:548`); a subdirectory of root gets parent 5
  (`directory.c:1653`, set `:1707`); continuation blocks copy the directory's *parent* and anodenr = dir's first
  anode (`directory.c:3176, 3204, 3392`); GetParent reads the containing block's parent, 0 = in root
  (`directory.c:645-654`). libpfs3: root parent 5 (`format.rs:321-326`) and continuation parent = dir itself
  (`writer.rs:1135`) → both wrong.
- D14: fnsize is a limit only (`blocks.h:446,516`; `init.c:646-647` default 32; `format.c:520` writes 32; hst
  writes 107). Create truncates to fnsize-1 (`directory.c:1489-1490` …); lookup truncates the search name
  (`:721-722`) and `intlcmp` needs equal length (`assroutines.c:163`) → a >31-byte name lists but cannot be
  opened (reading-derived).
- D4: `format.c:252-255` `SetDeldir(2)`, options `MODE_DELDIR(8) | MODE_SUPERDELDIR(256)`; `SetDeldir`
  `directory.c:4572-4637` → `NewDeldirBlock` seq 0,1 (`:4442-4480`: rext.deldir[seq]=blk, id DELDIRID 0x4444,
  seqnr, protection DELENTRY_PROT 5, creation date = rootblock's), deldirroving 0, deldirsize 2. No deldir is
  valid (`init.c:642-643`).
- ART-311: small `NewIndexBlock` `anodes.c:717-760` (seqnr ≤ MAXSMALLINDEXNR 98; rootblk idx.small.indexblocks
  [seqnr]; rootblock written last `update.c:269` with the reserved bitmap); super `NewSuperBlock`
  `anodes.c:844-870` (≤ MAXSUPER 15; rext.superindex[seqnr]; rext written `update.c:257`); seqnr UWORD →
  ≤ 65 536 anode blocks (`blocks.h:244`, `anodes.c:468,582`); roving `curranseqnr` `anodes.c:389,439-442,465`,
  saved `update.c:247`.
- D8: bound `blocknr >= vol->numblocks` (`allocation.c:344`), numblocks = partition blocks (`volume.c:637`);
  tail bits left free (1) (`allocation.c:1054-1055`); 1 = free (`:336-338`).
- D7: `DateStamp()` at `directory.c:3540`, `format.c:393`, `update.c:250-253`.

## ART-301 research (Explore agent, 2026-09-14)

- 33 `let title = format!` sites in 17 files + 7 fixed titles = **40 titles, 19 files** (entry says 34/18).
- `JobProgress.title: String` `core/jobs/mod.rs:70-73`; `commands/jobs.rs` spawn_job(210)/spawn_job_in_lane(230)/
  spawn_in_lane(243) take `title: &str`, stored `:59`, emitted `job-progress`; TS `src/lib/jobs.ts:36-46`;
  `JobBar.tsx:148` renders `{job.title}`; tests `JobBar.test.tsx:62,82,100`, `commands/jobs.rs:468,502`,
  fixtures `jobs.test.ts:46,180`, `phrase-keys.test.ts:440`.
- `Phrase` `src/lib/phrase.ts:8`; no Rust type serializes to it yet. Keys under `components.jobBar` (en.json:180).
- `dead-keys.test.ts` does not scan `.rs` → keys named only in Rust would be "dead"; needs a TS-side closed union
  (+ phrase-keys test) or scanning `.rs`. Titles do not reach operations.jsonl (`oplog.rs:52` uses its own name).

## D7 research (Explore agent, 2026-09-14) — a product-wide decision, not a libpfs3 patch

- Every Amiga date ART writes is UTC − 252 460 800 s: libpfs3 `util.rs:163-172` (callers `format.rs:114`
  root/rext dates, `writer.rs:571-574` entry re-stamp, `writer.rs:1178-1181` new entries); ART's FFS/OFS
  `core/volume/write/layout.rs:104-120` (`amiga_now`, `amiga_from_unix`), `core/adf/bcpl.rs:5-6` ("Amiga epoch
  1978-01-01 00:00:00 UTC"), `core/preload/native.rs:642-655`, `core/adf/create.rs:192-204`, `core/adf/mutate.rs`,
  host mtime fallback `copy.rs:380-388`.
- No time crate; `Cargo.toml:149-153` turned `chrono` off for fatfs deliberately. `core/` rule: std + listed crates.
  Nothing obtains the local offset (no Windows API call, no setting, no JS `getTimezoneOffset` sent).
- `.uaem` dates are zone-less text (`uaem.rs:193-272`); `distribution.json` carries no dates.
- libpfs3 copy_in never passes host mtimes (`native.rs:895-965`; dates dropped → ART-116); FFS does (`FileMeta.date`).
- No documented local-vs-UTC decision in architecture/ISSUES/lessons.
- A `.uaem` sidecar's own date is zone-less text parsed straight into an `AmigaDate` with no UTC step
  (`uaem.rs:206`, `amiga_from_civil`) and written through unchanged (`copy.rs:372-373`) — already local, not part
  of this defect. The defect is scoped to the two paths that do compute UTC: a date read from the host clock at
  write time, and a date converted from a host file's modification time (`copy.rs:380-388`).
- **Decision for the owner — decided 2026-09-14: local time everywhere.** An offset obtained outside `core/`
  (command layer via a Windows API or the webview's `getTimezoneOffset`) injected into every writer for the two
  paths above. File ART-317 either way, scoped to all writers (not ART-315, which is the allocator bound).
