# ART-117 — embedding or replacing a filesystem driver in a foreign card's RDB: design

*2026-09-14, on `art-debt-2-0914` (tree at `c7edbc6`). The owner's decisions of the same day, reopening the 2026-08-21
"leave it" (`.superpowers/sdd/2026-09-14-debt-2-round/progress.md:3-5`): **(1) "in-place append", not hst-imager's
whole-chain rewrite, not continued refusal; (2) "a filesystem with the same DosType already on the card is replaced when
the supplied driver is newer"; (3) "the RDB-area backup location is chosen by the user"** — then "onaylıyorum başla"
to the six-section design (`.superpowers/sdd/2026-09-14-debt-2-round/art117-design-approved.md`). Research:
`docs/superpowers/notes/2026-09-14-art-117-rdb-embed-research.md` (cited below as *R §n*). Plan, to come:
`docs/superpowers/plans/2026-09-14-art-117-rdb-embed.md`. Nothing below was run; tree facts are read and cited.*

*Amended 2026-09-14, after approval, with one further owner decision: ART may raise `rdb_RDBBlocksHi` over zero
blocks when the reserved range has no room (decision 12). That replaces the open question this spec ended with; the
places it touches (decisions 2, 6, 7, 8, Testing, Accepted limits) say so. Tree facts re-read at `62b5e63`.*

**The problem.** `NativeFormatter::import_filesystem` refuses every card (`core/preload/native.rs:171-186`, reasoning
`:29-51`) with `CoreError::ForeignRdbEmbedNotSupported` (`core/error.rs:151-158`, code `ART-NATIVE-EMBED-UNSUPPORTED`
at `:334`), and `commands/preload.rs:259-268` hands that step to `hst-imager`. The refusal's reason — `create_rdb_layout`
rebuilds on a fixed 16/63 geometry from `size_mb` (`core/rdb.rs:795-805`) — is true of a rebuild and does not apply to an
edit that never touches a PART block or a cylinder number (R §4.5). The owner's card
`CaffeineOS_Storm_9317.img` (R §0, read-only): one `0x76` area at byte 1 178 599 424; RDSK at block 0, **12 heads /
256 sectors**, `CylBlocks` 3072; **reserved range `RDBBlocksLo` 0 – `RDBBlocksHi` 6143**, `LoCylinder` 2, so the first
partition block is 6144; **live structures in blocks 0–131** (RDSK, PART 1–2, FSHD 3 `PDS\3` 19.2 `PatchFlags` 0x180
`SummedLongs` 128 named `L:pfs3aio040`, LSEG 4–131), `HighRDSKBlock` 131; blocks 132–2047 zero; **an unlinked stale RDB
copy at 2048–2179** (Heads 6, `DH0`/`DH1`); **old PFS3 structures from block 5120** (912 non-zero blocks below 6143).
Not on a chain does not mean zero.

## Decisions

1. **When it runs.** `plan()` (`core/preload/mod.rs:275-371`) gains one case beside the gate at `:320`.
   - **Append** — the card provides no driver for the partition's DosType and Kickstart does not carry it
     (`CardImage::provides_file_system`, `core/card/mod.rs:87-89`; `kickstart_carries`, `core/preload/mod.rs:465-467`):
     today's `PreloadStep::ImportFilesystem` (`:228-238`), now carried out. Once per plan, as today (`imported`, `:284`).
   - **Replace** — the card provides the DosType and the driver file states a newer version: a new
     `PreloadStep::ReplaceFilesystem { slot, driver, dostype, name, card_version, file_version }`. The card's version is
     the FSHD `CardImage::file_systems()` returns for that DosType — the first in area order (`core/card/mod.rs:74-84`),
     the same record the gate reads — and the step targets that FSHD's area. **One RDB edit per run.**
   - **The version** is `core::rdb::version_from_ver_string` (`core/rdb.rs:129-169`): the first `digits.digits` token
     within 200 bytes of the first `$VER:`, each half a `u16`, an over-large half skipped (tests `:1504-1541`). Compared
     as the tuple `(version, revision)` against the FSHD's `(version, revision)` (`:344-347`, read at `:578-579`):
     `19.10` > `19.9`. Strictly greater replaces; equal, older, or no `$VER:` plans no step, and the preview says so in a
     plan note (`PreloadPlan.notes`, e.g. card 19.2, file 19.2, kept) — an ignored driver is never silent.
   - An append with no `$VER:` is refused (decision 6): the FSHD must state a version, and `commands/hdf.rs:93-102`
     already refuses to guess one (`rdb.rs:117-128`).
   - *Rejected:* `core::amigaver::read` (`core/amigaver.rs:94-100`) — `u32` halves the FSHD cannot hold; the number
     compared must be the number written. A card-wide "newest wins" across areas — one edit, one backup, one sentence.

2. **Block selection** — a pure planner over the reserved range and the strict walk (decision 5).
   - **Used set**: the RDSK, every PART, every FSHD and each one's LSEG chain, and the `rdb_DriveInit` LSEG chain when
     `DriveInit` is not −1. The research counted BADB but not `DriveInit` (longword 9, a `LoadSegBlock` list per
     `hardblocks.h` [1][4], not re-read for this spec — if it is not, the strict walk refuses its first non-`LSEG` block
     rather than miscounting). A BADB list is refused instead (decision 6).
   - **Start** `k` = max(`HighRDSKBlock`, highest used block) + 1. **End** `k + n` where `n` = ceil(len / 492) LSEGs
     (`LSEG_DATA_BYTES`, `rdb.rs:107`) plus one FSHD. Contiguous by construction: nothing live lies above `k`.
   - **Bound**: `k + n` ≤ `RDBBlocksHi` **and** < the lowest first block of any partition, each from its own envec —
     `LowCyl × Surfaces × BlocksPerTrack × SizeBlock×4 / BlockBytes` with checked arithmetic
     (`ParsedPartition::byte_offset`, `rdb.rs:306-312`). On CaffeineOS: `k` = 132, and a 62 604-byte driver needs 129
     blocks, 132–260; the stale copy at 2048 is not reached. When `k + n` > `RDBBlocksHi`, decision 12 may raise it;
     when that is refused too, the answer is `NO-ROOM`.
   - The FSHD takes `k`, LSEGs `k+1..=k+n`, the layout `create_rdb_layout` writes (`rdb.rs:1011-1017`) and the card has.
   - *Rejected:* amitools' lowest-free, non-contiguous allocation (R §3 [8]) — it would fill holes below
     `HighRDSKBlock` that ART cannot tell from a previous tool's bookkeeping. hst-imager's renumbering from `RdbBlockLo`
     (R §2.3 [6]) — it moves every block, PARTs included. Overwriting a replaced driver's own blocks — the working
     driver would stop being linked before the new one is.

3. **Write order**, each stage followed by a sync, every prefix a valid RDB (R §5).
   - **Append**: S1 the LSEGs → S2 the FSHD (`Next` −1, `SegListBlocks` `k+1`) → S3 the RDSK with `HighRDSKBlock` =
     `k + n` → S4 **the commit, one sector**: `FileSysHeaderList` in the RDSK when the list is empty, otherwise the last
     FSHD's `Next`.
   - **Replace**: S1 the LSEGs → S2 the new FSHD (`Next` = the old FSHD's `Next`) → S3 `HighRDSKBlock` → S4 one
     pointer swapped from the old FSHD to `k`: `FileSysHeaderList` if the old one heads the list, else its
     predecessor's `Next`. The old FSHD and LSEGs drop out of the chain and stay on disk unzeroed.
   - S3 is its own write even when S4 is also the RDSK: an over-high `HighRDSKBlock` is harmless, an under-high one
     is not (R §5), and four uniform stages give four crash points to test.
   - PART blocks and cylinder numbers are never written.
   - **The new FSHD on replace is the old FSHD's 512 bytes** with only `Next`, `Version`, `SegListBlocks` and the checksum
     rewritten — `PatchFlags`, the DeviceNode fields, `SummedLongs` and the name stay as the card had them (MultibootOS
     writes `0x190`, `rdb.rs:1050-1052`; "never regenerate user data from scratch"). **On append** the FSHD and LSEG
     bytes come from builders extracted from `create_rdb_layout` (`rdb.rs:1022-1100`: `PatchFlags` 0x180, `GlobalVec` −1,
     HostID 7), guarded by byte-identical output of the existing layout tests.
   - *Rejected:* hst-imager's order — RDSK first, then every PART, FSHD and LSEG, no sync (R §2.4 [6]); a torn write
     after block 0 names a half-written driver or moved PARTs. amitools' order links the last FSHD before raising
     `HighRDSKBlock` (R §3 [10]). One combined RDSK write for an empty list — a fifth shape to test for one saved sync.

4. **Checksums and IDs.** Every written block is built whole in memory, zero past its data, `SummedLongs` stated (LSEG
   `5 + len/4`, `rdb.rs:1092`), checksum from `compute_rdb_checksum` (`rdb.rs:424-436`) over the first `SummedLongs`
   longwords — the RKRM's sum-to-zero [1][4]. IDs `RDSK`/`FSHD`/`LSEG` as the builders write them. Each written block is
   re-read and checked with `verify_rdb_block_checksum` (`:439-451`).
   - *Rejected:* amitools' 128-longword sum (`Block.py:166-171` [9]) — valid foreign blocks with bytes past
     `SummedLongs` fail it. The zeroed tail makes ART's own blocks pass both.

5. **A strict reader for the editor.** `parse_rdb` stays lenient on purpose (`rdb.rs:492-498`): it finds RDSK by ID
   alone (`:454-466`), records checksums without enforcing them (`:583`, `:602`, `:649`), hard-codes a 512-byte block
   (`:612`), treats a nonsensical `SummedLongs` as 64 (`:405-414`), and never reads `RDBBlocksLo/Hi`, `LoCylinder`,
   `CylBlocks`, `HighRDSKBlock`, `BadBlockList` or `DriveInit`. The new strict walk runs over the reserved range and
   refuses (decision 6) on:
   - not exactly one block with `RDSK` in blocks 0–15, or a bad checksum on it (a driver takes ID **and** checksum [2]);
   - `rdb_BlockBytes` ≠ 512; `RDSK block < RDBBlocksLo`; `RDBBlocksLo > RDBBlocksHi`; `HighRDSKBlock > RDBBlocksHi`;
     `RDBBlocksHi` ≥ the first partition block; `(RDBBlocksHi + 1) × 512` > 8 MiB;
   - any chain block with the wrong ID, a bad checksum, `SummedLongs` 0 or over 128 (LSEG under 5), a pointer outside
     `[RDBBlocksLo, RDBBlocksHi]`, a repeat, or past the walker's limits (64 PART, 32 FSHD, 4096 LSEG — `rdb.rs:498-501`);
   - two FSHDs of the replace target's DosType in one RDB.
   - The 8 MiB cap is `AREA_WINDOW_BYTES` (`core/card/mod.rs:35-40`): the whole range is read once into memory, and the
     same bytes are walked, planned from and written as the backup. CaffeineOS is 3 MiB; hst-amiga's default is 2016
     blocks [7].
   - *Rejected:* making `parse_rdb` strict — a card with one bad chain still has partitions worth showing.

6. **Refusals — all before the first write, each a typed `CoreError`.** One variant, `CoreError::RdbEditRefused(RdbEditRefusal)`,
   the enum declared in `core/error.rs` (which imports nothing above it), a stable code per reason. The retired
   `ART-NATIVE-EMBED-UNSUPPORTED` is not reused.

   | Reason | Code `ART-RDB-EDIT-…` | The sentence says |
   |---|---|---|
   | strict walk failed (decision 5) | `UNACCOUNTED` | which block and which check; ART edits only an RDB it can account for block by block |
   | `BadBlockList` ≠ −1 | `BAD-BLOCKS` | this RDB carries a bad-block list, which ART does not edit; hst-imager can |
   | no room below both bounds, and decision 12's raise refused | `NO-ROOM` | blocks needed against blocks free, the two bounds by number; hst-imager rewrites the whole RDB and checks neither bound (R §2.3), so back the card up first |
   | not a hunk file | `NOT-EXECUTABLE` | the file does not start with `HUNK_HEADER` `0x000003F3` or its length is not a multiple of 4; an `.lha` must be unpacked first (the picker offers `.lha`, `VolumePreload.tsx:215`) |
   | over 512 KiB | `DRIVER-TOO-LARGE` | the size and the cap (hst-imager's [5]); read from metadata before the file is |
   | append, no `$VER:` | `DRIVER-NO-VERSION` | why a version matters (`rdb.rs:117-126`) |
   | dynamic VHD | `DYNAMIC-VHD` | ART edits a raw card or HDF only: a write can grow a dynamic VHD, and the journal identifies its image by size (`core/volume/journal.rs:37-44`); detected by `read_card`'s footer cookie (`core/card/mod.rs:136-148`) |
   | journal pending beside the image | `JOURNAL-PENDING` | the journal's path; undo it first, as `commands/volume_write.rs:132-141` says for a volume |
   | no backup location | `NO-BACKUP` | choose where the RDB backup goes |
   | backup file exists | `BACKUP-EXISTS` | the path; ART never overwrites a file (SAFE_CREATE) |

   - **Append refusals fail the plan**, as a missing driver does today (`core/preload/mod.rs:332-341`): formatting on
     would make a volume an Amiga ignores. **Replace refusals do not**: the step is omitted and the refusal becomes a
     plan note ("the card's 19.2 stays: …"), because the card's own driver still mounts and the chosen driver is a
     remembered choice shared with the card builder (`src/lib/preload.ts:26-40`), not proof of intent.
   - `NO-BACKUP` is a run-time refusal: the preview has to show the step before a location can be asked for. A new
     `PreloadPlan::ready_to_run()` in `core/preload` refuses it, called by `preload_run` on the command thread before the
     job (`commands/preload.rs:600-602`) and by `core::preload::run`, so the engine does not depend on a screen.
   - *Rejected:* the research's refusal of overlapping partitions (R §6.2) — ART writes nothing there, and refusing a
     layout its owner mounts daily is a refusal the user cannot act on.

7. **The backup file.** The request carries `rdb_backup: Option<PathBuf>`; `PreloadPlan` echoes it. Before the journal
   is begun, the in-memory range — area blocks `0..=RDBBlocksHi` as the edit leaves it (decision 12: a raised value
   takes the backup with it), so a file offset is a block number × 512 — is written
   with `core::safety::atomic::atomic_create_new` (`core/safety/atomic.rs:104-128`): the name reserved with `create_new`,
   the bytes written to a temporary in the same folder, `sync_all`, renamed over the reservation; `Created::AlreadyThere`
   is `BACKUP-EXISTS`. The file is then read back and compared with the buffer; only then does any RDB write happen.
   **Its path is in the report on success and in every failure sentence after it exists** (decision 10).
   - *Rejected:* `safety::backup::backup_file` (`core/safety/backup.rs:68`) — a whole 64 GB card. A path remembered
     between runs — the next run would meet `BACKUP-EXISTS`.

8. **The journal.** `Journalled::begin(device, image, area.offset_bytes, description, blocks)`
   (`core/volume/journal.rs:125-176`) names exactly the S1–S4 blocks, saves their old contents — the zeros, or the stale
   copy's bytes if a driver ever reaches them — and fsyncs before the first write; `write_block` refuses any other block
   (`:201-208`). The device is a `FileRegionMut` (`core/volume/device.rs:284-328`) over **only** `(RDBBlocksHi + 1) × 512`
   bytes at the area's offset — `RDBBlocksHi` as the edit leaves it (decision 12) — so no block number can land past
   the reserved range (`:330-341`). `Journalled` gains a
   `sync()` — today only `commit` and `roll_back` sync (`:215-232`), and decision 3 needs one per stage. Success:
   `commit` after verification. Any error or failed verification: `roll_back`. A crash leaves `<image>.artjournal`
   (`:77-82`); `PendingJournal::roll_back` resolves blocks against `volume_offset` on the raw file (`:323-373`), so the
   File Manager's `volume_recover` (`commands/volume_write.rs:1107-1128`) undoes it; `plan()` refuses while it exists.
   - Cancellation is checked before the step, not inside it (R §6.1 proposed between stages): the edit is a few hundred
     KB, and a cancel between S1 and S4 would add an ending that must roll back.

9. **Read-back verification**, before `commit`: a fresh read-only `FileRegion` (`core/volume/device.rs:96`) over the
   range; every written block byte-equal to the plan; the strict walk passes; every block outside the write set
   byte-equal to the backup buffer — PARTs, the stale copy, the old PFS3 blocks; the chain yields the DosType at the file's
   version with an LSEG payload byte-equal to the driver (length from `SummedLongs`, `rdb.rs:530-535`); and `read_card`
   shows `partitions_missing_driver()` empty (`core/card/mod.rs:111-120`).

10. **How the step is reported.** Embedding leaves `VolumeFormatter`: `import_filesystem` is removed from the trait
    (`core/preload/mod.rs:137-146`), from `tools/hst_imager.rs:274-285` and from both runners; `run_step`
    (`commands/preload.rs:329-363`) and `core::preload::run` (`core/preload/mod.rs:405-411`) call one
    `core::preload::embed::run` for both step kinds. The `StepReport` (`commands/preload.rs:211-218`) says `tool:
    "native"`, `fallback_reason: None`; `FallbackReason::ForeignRdbEmbed` (`:230-231`, `:261`, `:274-279`) and
    `ForeignRdbEmbedNotSupported` go. `PreloadOutcome` (`core/preload/mod.rs:374-380`) gains `embedded:
    Option<EmbedReport>` — area slot, DosType, card and file versions, blocks written, the backup path — and the oplog
    record (`commands/preload.rs:624-652`) gains the driver and the backup lines. The endings stay distinct, one sentence
    each:
    - **refused** — nothing written, no backup made (decision 6);
    - **backup failed** — the path and the OS reason, card untouched;
    - **failed before the first RDB write** — card untouched, backup at *path*;
    - **failed or not verified, rolled back** — card restored as it was, backup at *path*
      (`CoreError::RdbEditFailed { backup, restored: true, detail }`);
    - **failed and the rollback failed** — the journal's path, undo it in the File Manager, backup at *path*
      (`restored: false`);
    - **succeeded** — `EmbedReport`.
    - *Rejected:* keeping hst-imager as the automatic fallback for a refusal (R §6.2 kept it for `NO-ROOM`) — it would run
      the whole-chain rewrite the owner rejected, silently, with no backup. The refusal names it; the user decides.

11. **The UI** (`src/components/osbuilder/VolumePreload.tsx`, `src/lib/preload.ts`).
    - A "RDB backup file" field beside the driver field (`VolumePreload.tsx:361-367`), shown when a driver is chosen,
      filled by `save({ title, defaultPath: "<card stem>-rdb-backup.bin" })` — the pattern `CardBuilder.tsx:481-488`
      uses. Held in `useState`, not `useRemembered` (decision 7; the same reasoning as `picks`, `:93-101`). It is part of
      `toRequest` (`preload.ts:380-397`), so changing it clears the plan like any other request change (`:140-151`).
    - `preloadBlocker` (`preload.ts:429-460`) gains `preload.blocked.noBackup` when the plan has an embed or replace step
      and no backup path; Run is disabled on the blocker (`VolumePreload.tsx:566`).
    - `stepPhrase` (`preload.ts:553-571`): the replace step says both versions ("PDS\3 19.2 on the card → 19.3 from the
      file"), and both embed steps name the backup path and the block range. `plannedToolPhrase` (`:523-534`) labels
      them native. Plan notes render in the preview; `EmbedReport` renders in the result panel.
    - Removed: `needsExternalTool` (`:411-413`), the `foreign-rdb-embed` reason (`:141`, `:480-481`) and keys
      `preload.blocked.noTool`, `preload.fallback.foreignRdbEmbed`, `preload.plan.step.tool.hstImager` (`en.json:2164`,
      `:2186`, `:2194`). **`en.json` and `tr.json` change in one commit**; lib helpers return a `Phrase`, never a string.

12. **Raising `RDBBlocksHi` when the reserved range has no room** — the owner's decision of 2026-09-14, added after
    approval, one task in the plan.
    - **When.** Only when decision 2's `k + n` > `rdb_RDBBlocksHi`. A range with room is never widened.
    - **Over zero blocks only.** Every block from `RDBBlocksHi + 1` to `k + n` must read as 512 zero bytes in the
      in-memory range decision 5 already read. The first non-zero one is `NO-ROOM`, naming that block.
    - **Bounds**, each checked, each a `NO-ROOM` naming the bound when crossed:
      - `k + n` < the lowest `LowCyl` of any PART, in blocks through the RDB's own geometry: `LowCyl × rdb_Heads ×
        rdb_Sectors`, checked arithmetic;
      - `k + n` < decision 2's bound from each partition's own envec, which still holds;
      - `k + n` < `rdb_LoCylinder × rdb_CylBlocks`, where `hardblocks.h` says the partitionable area begins [1][4].
        *This third bound is the plan's addition, not the owner's*: it only narrows, and on both shapes measured it
        equals the first (CaffeineOS 2 × 3072 = 6144; an ART card 2 × 1008 = 2016);
      - `(k + n + 1) × 512` ≤ 8 MiB, the window decision 5 reads (`AREA_WINDOW_BYTES`, `core/card/mod.rs:40`).
    - **Raised to exactly `k + n`**, the least value that fits.
    - **Journalled and in the backup like any other block.** Because `k` ≤ `RDBBlocksHi + 1` (decision 5 keeps every
      used block and `HighRDSKBlock` inside the range), every block the raise takes is one S1 or S2 writes, so it is
      already named to `Journalled::begin`; the backup and the `FileRegionMut` cover `0..=` the raised value.
    - **Written in S3**, in the one RDSK write that raises `HighRDSKBlock`, before S4's link. A crash after S3 leaves
      a range that holds unlinked LSEG and FSHD bytes: still a valid RDB.
    - **Still no room**: the existing `NO-ROOM`, which names hst-imager.
    - **What it makes possible: a driver update on a card ART built.** `create_rdb_layout` sets `RDBBlocksHi` and
      `HighRDSKBlock` both to `last_rdb_block` (`rdb.rs:883`, `:918`, `:922`), with `LoCylinder` 2 (`:919`) on 16/63
      geometry (`:796-798`) and the first partition at `LowCyl` 2 (`:934`, `:977`) — first partition block 2016. One
      PART and a 62 604-byte driver make 131 structured blocks (`:856-860`), `RDBBlocksHi` 130; a replace needs 129
      blocks, 131–259, and raises `RDBBlocksHi` to 259. A driverless ART-built card (`RDBBlocksHi` 1) takes an append
      at 2–130 the same way.
    - *Rejected:* raising over non-zero blocks — CaffeineOS's stale RDB copy at 2048 shows that bytes nobody links to
      are not bytes nobody wrote, and above `RDBBlocksHi` no tool has promised anything. Moving `LowCyl` — it moves a
      partition, the rebuild the owner rejected. Raising straight to `LoCylinder × CylBlocks − 1` — it claims blocks
      ART never wrote as hardblocks. A separate RDSK write for `RDBBlocksHi` — a fifth stage to test for nothing, since
      an over-high `RDBBlocksHi` over written blocks is as harmless as an over-high `HighRDSKBlock`.

## Where it lives

`core/` stays std-only. New edges only point down: `core/preload` already imports `card`, `rdb`, `volume` and `safety`
(`core/preload/native.rs:105-122`); `core/card` imports `rdb` and not `volume` (`core/card/mod.rs:31-33`); `core/rdb.rs`
imports only `adf::bcpl` and `error` (`:9-10`).

- **`core/rdb.rs`** — the extracted FSHD/LSEG builders; nothing else changes there.
- **`core/rdbedit.rs`** (new, imports `rdb` and `error`) — pure: the strict walk (decision 5), the version comparison,
  and the planner returning either staged `(block, [u8; 512])` writes plus the used set and bounds, or an
  `RdbEditRefusal`. No I/O, so every refusal and every allocation is a unit test over a byte vector.
- **`core/preload/embed.rs`** (new) — the I/O: read the range, refuse a dynamic VHD or a pending journal, the backup,
  `Journalled` over `FileRegionMut`, the stages and syncs, verification, commit or rollback, `EmbedReport`. `plan()`
  calls its read-only half to build the step or the note.
- **`core/error.rs`** — `RdbEditRefused`, `RdbEditFailed`, their codes; `ForeignRdbEmbedNotSupported` removed.
- **`commands/preload.rs`** — `run_step` maps both steps to `embed::run`; `preload_run` calls `ready_to_run()`; the
  oplog lines; `FallbackReason` loses a variant. No policy.
- **Frontend** — decision 11. `tools/hst_imager.rs` loses `import_filesystem` and its argv builder.
- **Docs owed by the plan**: the rule at `docs/architecture.md:630-645` (two capability gaps become one), the module
  docs at `core/preload/native.rs:29-51`, `commands/preload.rs:24-40` and `src/lib/preload.ts:5-16`, ISSUES ART-117
  (to Fixed with its tests), FEATURES, STATUS (`:465-466` still calls ART-117 a standing decision), CHANGELOG.

## Testing

- **Fixtures are synthetic, built in each test in a `ScratchDir`** (`let (_guard, dir) = ScratchDir::pair(..)`). The
  driver is a made-up hunk: `0x000003F3`, padding, `$VER: pfs3aio 19.3 (1.1.26)`, a length that is a multiple of 4.
  No Amiga binary ships.
- **Two geometries.** An ART-shaped RDB from `create_rdb_layout`, and a hand-built **CaffeineOS-shaped** one: 12/256,
  `CylBlocks` 3072, `RDBBlocksHi` 6143, `LoCylinder` 2, PARTs `SDH0` 2–535 and `SDH1` 536–36194, FSHD `SummedLongs` 128
  with a name at byte 172, LSEG 4–131, `HighRDSKBlock` 131, **a stale RDSK/PART/FSHD/LSEG copy at 2048–2179 with Heads 6,
  and non-zero blocks from 5120**. Both append (a card with no driver) and replace (19.2 → 19.3) run on it; the tests
  assert the new blocks are 132–260 and that 2048–2179 and 5120+ are byte-identical afterwards.
- **Planner unit tests** in `core/rdbedit.rs`: bounds, `DriveInit` in the used set, the partition bound from each envec,
  the version tuple (`19.10` > `19.9`, equal is no step, no `$VER:` is no step), the FSHD kept on replace.
- **The raise (decision 12)**: an ART-shaped replace lands at 131–259 with `RDBBlocksHi` 259; one test per refusal —
  a non-zero block above `RDBBlocksHi`, each of the three partition-area bounds on its own, the 8 MiB window — each
  asserting the `NO-ROOM` sentence names the bound; and a crash after S3 on a raising edit still walks.
- **One test per refusal** in the table, each asserting the variant **and** its sentence, never merely `is_err()`.
- **Crash points.** A test `BlockDeviceMut` that stops after stage *s* (1–4) without rollback, as a crash would. After
  each: the strict walk passes; before S4 the chain is the old one byte for byte, after S4 the new one; PARTs unchanged;
  the `.artjournal` is there and `PendingJournal::roll_back` restores the pre-image byte for byte. Both arms reported.
- **Endings.** A fault injected at each point of decision 10 yields its own variant and sentence, and the backup path is
  asserted in every sentence after the backup exists.
- **Mutations, disclosed with survivors**: link before data (S4 first); `HighRDSKBlock` raised after the commit; the
  `RDBBlocksHi` bound; the partition bound; `DriveInit` dropped from the used set; checksum enforcement off; `create_new`
  replaced by `create`; the backup written after the first RDB write; `>` made `>=`; the zeroed tail; the builders'
  byte-identical guard; the raise's zero check; each of its three partition-area bounds; the raise left out of S3.
  `cargo test` run more than once before merging.
- **Frontend**: `preload.test.ts` for the blocker, phrases and `toRequest`; a jsdom `VolumePreload.test.tsx` that Run is
  disabled until a backup path is chosen and that the replace step shows both versions.
- **The owner's card, on a copy — `#[ignore]`**, gated on three variables: `ART_CARD_IN` (the original, already the
  read-only hook's variable, `core/card/mod.rs:489-499`), `ART_RDB_EMBED_COPY` (a byte copy the owner makes under
  `E:\amiga\ProjeART`) and `ART_RDB_EMBED_DRIVER`. The test refuses unless both card paths are set, canonicalise
  differently and have equal sizes; it never opens `ART_CARD_IN` for writing, and asserts its reserved range and mtime
  unchanged. It runs a replace on the copy, prints the block map and the `EmbedReport`, and writes the post-edit range
  to scratch. Then, outside the test, `hst.imager rdb info <copy>` and `rdbtool <range file> info` / `fsget` list it
  (rdbtool has no offset, R §3; mind its 128-long checksum [9]). **Where the path is recorded:** `STATUS.md` has no entry
  for `CaffeineOS_Storm_9317.img`; the name is in `ISSUES.md:47` and `core/card/mod.rs:493`, the full path
  `E:\amiga\Amigatolon\caffeine\CaffeineOS_Storm_9317.img` in R §0. The plan adds the reproduce lines to STATUS. The
  card carries 19.2; if no newer `pfs3aio` exists, the driver is a scratch copy with its `$VER:` digits edited.

## Accepted limits

- **Unverified whether scsi.device, pi-scsi or HDToolBox read `RDBBlocksHi` or `HighRDSKBlock`** (R §1.3); the RKRM's
  mount description names neither [2][3]. ART raises `HighRDSKBlock`, and raises `RDBBlocksHi` only under decision
  12; a driver that did read `RDBBlocksHi` would see a larger reserved range, still below the first partition.
- **Unverified whether an SD card writes a 512-byte sector atomically.** A torn S3 or S4 sector fails its checksum and
  hides the partitions until the journal or the backup restores it.
- **Every replace consumes n + 1 new blocks**; orphaned blocks are not zeroed or reused. CaffeineOS has about 46
  replaces of room.
- **ART's own cards have room only through decision 12**: `create_rdb_layout` sets `RDBBlocksHi` = `HighRDSKBlock` =
  the last structured block (`rdb.rs:883`, `:918`, `:922`). Each raise moves `RDBBlocksHi` up by the edit's own
  blocks, so repeated replaces walk it towards block 2016 and then meet `NO-ROOM` (about 14 replaces of a 62 604-byte
  driver).
- **One RDB edit per run**, in the first area carrying the DosType; a card with it in several areas keeps the others.
- **ART offers no restore command.** The backup is raw area blocks `0..=RDBBlocksHi`; whether hst-imager's
  `rdb restore` accepts it is unverified (its own backup looks one block short, R §5 [11]).
- A crash after S4 but before `commit` rolls a complete edit back on recovery. Conservative, and still a valid RDB.
- BADB lists, dynamic VHDs and archived drivers are refused, not handled.
- **The committed `rdbdump.py` scans unlinked blocks only up to 4096** (its `scan_top`), so it reproduces the copy at
  2048 but not the PFS3 blocks from 5120; the research does not record how that count was made.
- **Owed by the owner**: mounting both PDS partitions from the edited copy in WinUAE or on a PiStorm, and asking the
  loaded handler for `version full`; running the `#[ignore]` hook.

## Sources

[1] Commodore RKRM Devices ch. 11, *RigidDiskBlock — Fields and Implementation* (NDK `hardblocks.h` with comments) —
amigadev.elowar.com `ADCD_2.1/Devices_Manual_guide/node0079.html` (expired certificate, fetched `curl -k`); mirrored as
hst-amiga `docs/Amiga_Developer_Docs_-_RigidDiskBlock.html`.
[2] Same, *How A Driver Uses RDB* — `node007A.html`: 16-block scan for `RDSK` with ID and checksum; PARTs used only with
ID and checksum.
[3] Same, *Alien Filing Systems* — `node007B.html`: the FSHD list scanned for DosType and version, `LoadSeg`, patch flags.
[4] AROS `compiler/include/devices/hardblocks.h` @ `a7cc38cbbd5e48168a39276595584ef2845b0381`, lines 16–157.
[5] hst-imager @ `63b28587192d236ac4dce7f712924e3173488243`, `src/Hst.Imager.Core/Commands/RdbFsAddCommand.cs`:
`:67` read, `:87-91` 512 KB cap, `:95-120` `$VER:` required, `:134-136` same DosType replaced silently.
[6] hst-amiga @ `6b455841808361bab8e2167fc5aa11c8ba270adf`, `src/Hst.Amiga/RigidDiskBlocks/BlockHelper.cs:45-112`
(renumber from `RdbBlockLo`), `RigidDiskBlockWriter.cs:129-221` (RDSK first, no flush).
[7] hst-amiga, same commit, `RigidDiskBlocks/FileSystemHeaderBlock.cs:42-52` (PatchFlags 384, GlobalVec −1),
`RigidDiskBlock.cs:167` (`RdbBlockHi` 2015).
[8] amitools @ `3b57f2052ee76c28bbc5e4256227f62dca7b1c9f`, `amitools/fs/rdb/RDisk.py`: `:44-81` used set without BADB,
`:526-559` lowest-free allocation and `_update_hi_blk`, `:722-752` `add_filesystem`, `:764-767` delete relinks the last.
[9] amitools, same commit, `amitools/fs/block/Block.py:166-171` — checksum over all 128 longwords.
[10] amitools, same commit, `amitools/fs/rdb/FileSystem.py:61-72, 98-102`; `tools/rdbtool.py:917-981` (`fsadd`).
[11] hst-imager, same commit, `readme.md:11`; `Commands/RdbBackupCommand.cs:58` (size `RdbBlockHi × BlockSize`).
[12] The owner's card, read-only: `docs/superpowers/notes/2026-09-14-art-117-rdbdump.py` on
`E:\amiga\Amigatolon\caffeine\CaffeineOS_Storm_9317.img`, output in R §0.
