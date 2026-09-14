*Copied verbatim on 2026-09-14 from `D:\Projeler\Amiga\scratch-0913\art117-research.md` (outside the repository); only the script's file name is changed, to its copy beside this note.*

# ART-117 research — embedding a filesystem into an existing RDB in place

Research only (2026-09-14). Nothing in the repository was edited. The one thing run against real
material was a **read-only** dump of the owner's card (`open(..., "rb")`), with the script beside
this file: `2026-09-14-art-117-rdbdump.py`. No write experiment was run with any tool. Every claim that was not
read from a source or measured is marked **unverified**.

Sources, pinned to the commit read:

| Source | Commit / version |
|---|---|
| hst-imager | `63b28587192d236ac4dce7f712924e3173488243` (2026-06-30), raw.githubusercontent.com `main` |
| hst-amiga | `6b455841808361bab8e2167fc5aa11c8ba270adf` (2026-07-02) |
| amitools | `3b57f2052ee76c28bbc5e4256227f62dca7b1c9f` (2025-12-30) |
| AROS `compiler/include/devices/hardblocks.h` | `a7cc38cbbd5e48168a39276595584ef2845b0381` |
| Commodore RKRM Devices, ch. 11, "RigidDiskBlock - Fields and Implementation" | amigadev.elowar.com `ADCD_2.1/Devices_Manual_guide/node0079.html` (the NDK `hardblocks.h` with comments), `node007A.html` ("How A Driver Uses RDB"), `node007B.html` ("Alien Filing Systems"). The site's TLS certificate has expired, so these were fetched with `curl -k`. The same node0079 page is mirrored in hst-amiga `docs/Amiga_Developer_Docs_-_RigidDiskBlock.html` and matches it. |

Line numbers below refer to those commits. "hi:" means `hst-imager/src/Hst.Imager.Core/…`, "ha:"
means `hst-amiga/src/Hst.Amiga/…` and "am:" means `amitools/amitools/…`.

---

## 0. What the owner's card actually holds (measured, read-only)

`E:\amiga\Amigatolon\caffeine\CaffeineOS_Storm_9317.img` was read with `python 2026-09-14-art-117-rdbdump.py <img>`.

```
MBR slot 0 type=0x0c lba=2048     count=2299904
MBR slot 1 type=0x76 lba=2301952  count=118235136     -> area base 1178599424
RDSK at area block 0, checksum ok, SummedLongs 64
  BlockBytes 512  Flags 7  BadBlockList -1  PartitionList 1  FileSysHeaderList 3  DriveInit -1
  Cylinders 38488  Sectors 256  Heads 12
  RDBBlocksLo 0  RDBBlocksHi 6143  LoCylinder 2  HiCylinder 38487  CylBlocks 3072  HighRDSKBlock 131
  PART blk=1 SDH0 PDS\3 low=2   high=535   cks ok
  PART blk=2 SDH1 PDS\3 low=536 high=36194 cks ok
  FSHD blk=3 PDS\3 19.2 PatchFlags 0x180 SegList 4 GlobalVec -1 SummedLongs 128 name 'L:pfs3aio040'
   LSEG 4..131, 128 blocks, contiguous, 62,604 data bytes
  first partition data block = 2*3072 = 6144 = RDBBlocksHi+1 = LoCylinder*CylBlocks
  blocks free inside [RDBBlocksLo..RDBBlocksHi] = 6012   (132..6143)
```

Two findings this record did not have before. Both matter to any allocator.

1. **An unlinked second RDB sits inside the reserved range, at blocks 2048–2179.** No chain points
   to it. 129 of its 132 blocks are byte-identical to blocks 0–131. The three that differ:
   - The RDSK: Heads 6 against 12, CylBlocks 1536 against 3072, RDBBlocksHi 3071 against 6143,
     Cylinders 39080, and vendor text `Caffeine_OS` against `CAOS`.
   - The two PART blocks: named `DH0`/`DH1` against `SDH0`/`SDH1`, with different HighCyl and
     NumBuffers, and the second has SectorsPerBlock 8.
2. **Blocks 5120 onwards (912 non-zero blocks in 112 runs, all below 6143) hold PFS3 structures.**
   Block 5120 begins `PFS\1`, followed by `BM`, `DB`, `AB` and `EX` blocks. 5120 is exactly
   2048 + LowCyl 2 × 1536, which is the first block of the old copy's `DH0`. *Inference, not
   proven:* these are leftovers of an earlier layout whose area began 1 MiB later.

**So "not on any chain" does not mean "zero".** Blocks 132–2047 are all zero. Beyond that, the
reserved range carries a plausible-looking dead RDB and stale filesystem data. A scanner that looked
for `RDSK` past block 16 would find the wrong one (ART's 16-block limit is correct). An allocator
must work from the live chains, and must journal whatever it overwrites.

---

## 1. The RDB format

### 1.1 Structures

These come from `hardblocks.h` (AROS lines 16–157, and RKRM node0079 with Commodore's comments).
Longword indices are in brackets.

**RDSK** (`struct RigidDiskBlock`):
- `rdb_ID`[0] = `'RDSK'`; `rdb_SummedLongs`[1]; `rdb_ChkSum`[2]; `rdb_HostID`[3].
- `rdb_BlockBytes`[4]: "size of disk blocks".
- `rdb_Flags`[5].
- The list heads: `rdb_BadBlockList`[6], `rdb_PartitionList`[7], `rdb_FileSysHeaderList`[8],
  `rdb_DriveInit`[9]. Each is optional, and **`$ffffffff` means NULL "as zero is a valid address"**
  (RKRM node0079 "NOTE").
- `rdb_Reserved1[6]`, which the RKRM says to set to `$ffffffff`.
- Physical geometry: `rdb_Cylinders`[16], `rdb_Sectors`[17], `rdb_Heads`[18].
- Logical characteristics:
  - `rdb_RDBBlocksLo`[32]: "low block of range reserved for hardblocks".
  - `rdb_RDBBlocksHi`[33]: "high block of range for these hardblocks".
  - `rdb_LoCylinder`[34]: "low cylinder of partitionable disk area".
  - `rdb_HiCylinder`[35].
  - `rdb_CylBlocks`[36].
  - `rdb_HighRDSKBlock`[38]: "highest block used by RDSK (not including replacement bad blocks)".
- Identification strings at [40..53].

**PART** (`struct PartitionBlock`): `pb_Next`[4], `pb_Flags`[5], `pb_DevFlags`[8],
`pb_DriveName`[9..16] (a BSTR), and `pb_Environment` from [32]. LowCyl is [41], HighCyl [42] and
DosType [48]. These match ART's `rdb.rs:656-692`.

**FSHD** (`struct FileSysHeaderBlock`):
- `fhb_Next`[4] and `fhb_Flags`[5].
- `fhb_DosType`[8]: "match this with partition environment's DE_DOSTYPE".
- `fhb_Version`[9]: "release version of this code", as version<<16 | revision.
- `fhb_PatchFlags`[10]: "bits set for those of the following that need to be substituted into a
  standard device node … e.g. 0x180 to substitute SegList & GlobalVec".
- The DeviceNode fields `fhb_Type`..`fhb_GlobalVec`[11..19]. `fhb_SegListBlocks`[18] is "first of
  linked list of LoadSegBlocks".
- The NDK comment header gives `fhb_Reserved2[23]` and `fhb_Reserved3[21]`. AROS has
  `fhb_FileSysName[84]` in their place, at byte 172. hst-amiga writes the name at `0xac`
  (ha: `RigidDiskBlocks/FileSystemHeaderBlockWriter.cs:54-56`).

**LSEG** (`struct LoadSegBlock`): `lsb_Next`[4] and `lsb_LoadData[123]`, where "[123] assumes 512
byte blocks". **A block carries at most 123 × 4 = 492 data bytes.** The data actually present is
`(SummedLongs − 5) × 4`. That is how ART (`rdb.rs:530-535`), amitools
(am: `fs/block/rdb/LoadSegBlock.py:28,31`) and hst-amiga (ha: `LoadSegBlockWriter.cs:29`) all size it.

**BADB** (`struct BadBlockBlock`): `bbb_Next`[4] and 61 pairs of (bad, good) block numbers.

### 1.2 Checksums and IDs

- The rule is "longword sum to zero" over the first `SummedLongs` longwords (the `hardblocks.h`
  comments). ART implements exactly that (`rdb.rs:405-451`): `summed_longs` is guarded against 0 and
  against more than 128. hst-amiga does the same over `size*4` bytes
  (ha: `RigidDiskBlockReader.cs:135`, `FileSystemHeaderBlockReader.cs:106`, `LoadSegBlockReader.cs:66`).
- **amitools diverges.** `Block._calc_chksum` sums **all 128 longwords**, whatever `SummedLongs` says
  (am: `fs/block/Block.py:166-171`). A foreign block with non-zero bytes past `SummedLongs` therefore
  fails amitools' check while being valid by the spec. Keep this in mind when amitools is the oracle.
- IDs are how a driver validates a block. It "will scan the first RDB_LOCATION_LIMIT (16) blocks
  looking for a block with the 'RDSK' identifier and a correct sum-to-zero checksum". Partition
  blocks are used only if they "have the correct ID and checksum" (RKRM node007A).
- For a non-ROM DosType, the driver scans the FSHD list "for a filesystem of the required DosType and
  version", `LoadSeg()`s it from the LSEG blocks, then patches the DeviceNode "using the patch flags"
  (RKRM node007B, steps 3–4).

### 1.3 Which blocks may take new RDB structures

- **The spec reserves `[rdb_RDBBlocksLo, rdb_RDBBlocksHi]` for hardblocks.** It recommends using "the
  first cylinder(s) to store all the drive data … partition descriptions, file system load images,
  drive bad block maps, spare blocks" (node0079 header comment). `rdb_LoCylinder` is where the
  partitionable area starts.
- **`rdb_HighRDSKBlock` is only the highest block *used*.** Blocks above it and ≤ `RDBBlocksHi` are
  inside the reserved range, but (§0) not necessarily zero.
- **No source states an allocation rule** (lowest free, contiguous, and so on). The RKRM gives field
  meanings only. What the two tools actually do is in §2 and §3.
- *Unverified:* whether any AmigaOS driver (scsi.device, the PiStorm `pi-scsi` driver, HDToolBox)
  reads `RDBBlocksHi` or `HighRDSKBlock` at mount time. The RKRM's description of what a driver does
  (node007A/B) names neither field. They look like tool-side bookkeeping, but that was not confirmed
  in any driver source.

### 1.4 What an in-place add must update

1. The new FSHD (checksum), with `fhb_Next` = −1 (or the old successor, when replacing),
   `fhb_SegListBlocks` = the first LSEG, `PatchFlags` 0x180 and `GlobalVec` −1.
2. Each new LSEG (`SummedLongs` = 5 + ceil(len/4), `lsb_Next`, checksum).
3. The link into the list:
   - `rdb_FileSysHeaderList` when the list is empty, which rewrites the RDSK checksum; **or**
   - the last FSHD's `fhb_Next`, which rewrites that FSHD's checksum.
4. `rdb_HighRDSKBlock`, if the new blocks lie above it (RDSK checksum).

`rdb_RDBBlocksHi` needs changing only if the new blocks do not fit under it. That case should be a
refusal (§6), not an edit.

---

## 2. hst-imager (and hst-amiga, which does the block work)

### 2.1 The commands

- `hst.imager rdb fs add <Path> <FileSystemPath> <DosType> [--name] [--version] [--revision]`
  (`hst-imager/src/Hst.Imager.ConsoleApp/RdbCommandFactory.cs:124-125` for the `fs` alias, `:167-185`)
  runs `RdbFsAddCommand` (hi: `Commands/RdbFsAddCommand.cs`).
- `rdb fs import <Path> <FileSystemPath> [--dos-type] [--name]` (factory `:236-250`) runs
  `RdbFsImportCommand` (hi: `Commands/RdbFsImportCommand.cs`). It can also pull a driver out of an
  ADF or another disk (`AmigaFileSystemHelper.FindFileSystemInMedia`, `:60-61`) or download it from a
  URL (`:51-56`).
- Both open the media through `MediaHelper.GetMediaWithPiStormRdbSupport`
  (hi: `Helpers/MediaHelper.cs:59-70`). That descends into an MBR partition of BIOS type `0x76`
  (`:134`), so the tool handles a PiStorm card directly.
- `rdb backup` and `rdb restore` exist as **separate** commands (factory `:954`, `:979`). `fs add` and
  `fs import` never call them.

### 2.2 What `fs add` does

`RdbFsAddCommand.cs`:
- `:67` reads the whole RDB into a model. `RigidDiskBlockReader.Read` scans 16 sectors and **throws
  on a bad checksum** unless `ignoreChecksum` is set (ha: `RigidDiskBlockReader.cs:16-65,137-140`).
  The same holds for FSHD (`FileSystemHeaderBlockReader.cs:106-111`) and LSEG
  (`LoadSegBlockReader.cs:66-70`).
- `:87-91` refuses a driver over 512 KB.
- `:95-120` requires a `$VER:` string, or `--version` and `--revision`.
- `:122-124` builds an FSHD model: `BlockHelper.CreateFileSystemHeaderBlock` chunks the driver into
  492-byte LSEGs (ha: `BlockHelper.cs:10-31`). The model's defaults are HostId 7, **PatchFlags 384
  (= 0x180)** and GlobalVec −1 (ha: `FileSystemHeaderBlock.cs:42-52`).
- **`:134-136` replaces a same-DosType filesystem silently**: it filters the existing one out and
  appends the new one at the end of the list. `RdbFsImportCommand.cs:110-115` is identical. (The
  library's own `AddFileSystem` extension *refuses* a duplicate unless `overwrite` is set,
  ha: `Extensions/RigidDiskBlockExtensions.cs:35-38`, but neither command uses it.)
- `:139` calls `MediaHelper.WriteRigidDiskBlockToMedia` (`MediaHelper.cs:187-193`), which calls
  `RigidDiskBlockWriter.WriteBlock`.

### 2.3 How it chooses block numbers, and how it rewrites

**It rewrites the entire RDB chain from scratch, renumbering every block contiguously.**
`RigidDiskBlockWriter.WriteBlock` first calls `BlockHelper.UpdateBlockPointers`
(ha: `RigidDiskBlockWriter.cs:132`; `BlockHelper.cs:45-112`). That function:
- starts at `RdbBlockLo`;
- gives the PART blocks RdbBlockLo+1…;
- then gives each FSHD the next block, followed immediately by its LSEGs;
- then the BADB blocks;
- sets `HighRsdkBlock` to the last block used (`:111`).

**No check against `RdbBlockHi`, `LoCylinder` or the first partition's `LowCyl` was found in that
call path.** `UpdateBlockPointers`, `WriteBlock`, `WriteRigidDiskBlockToMedia` and both commands
were read in full. `RdbBlockHi` is read and written back unchanged. The only size limit is the
512 KB per-driver cap. `RdbBlockHi` is used elsewhere only by `rdb backup`/`restore` and the
partition-table report (hi: `Commands/RdbBackupCommand.cs:58`;
`PartitionTables/RigidDiskBlockReader.cs:40-44`).

### 2.4 Write order

`WriteBlock` (ha: `RigidDiskBlockWriter.cs:129-221`):
1. **RDSK first** (`:135-139`).
2. Every PART (`:141-160`).
3. Each FSHD followed by its LSEGs (`:162-199`).
4. The BADBs (`:201-220`).

Other properties of the rewrite:
- Existing blocks are re-serialised over their original `BlockBytes`, so unknown fields survive
  (`RigidDiskBlockWriter.cs:14-17`, `FileSystemHeaderBlockWriter.cs:14-18`,
  `PartitionBlockWriter.cs:15-17`).
- There is no flush or sync in the call path. Blocks freed by a shrink or renumber are not cleared.
- `LoadSegBlockWriter` refuses LSEG data whose length is not a multiple of 4
  (`LoadSegBlockWriter.cs:13-16`).
- `LoadSegBlockReader` loops `while (segListBlock > 0)` with no visited set (`LoadSegBlockReader.cs:46`).

**Consequence on this card.** Blocks 1, 2, 3–131 are already in canonical order, so renumbering is
the identity and a `PDS\3` replacement rewrites blocks 0–131+ in place: RDSK first, then the old
driver's own blocks overwritten. A power loss after block 0 but before the last LSEG leaves a list
that points at a half-written driver. Both partitions then fail to mount until the command is re-run.
The data itself stays intact. On a foreign RDB whose blocks are *not* in canonical order, the rewrite
moves PART blocks as well. The RDSK is written first and already names their new positions, so an
interruption can hide every partition.

---

## 3. amitools `rdbtool`

- **Command.** `rdbtool <image> fsadd <file> [dostype=…] [version=x.y] [flag=value …]`
  (am: `tools/rdbtool.py:917-981`; docs: `docs/tools/rdbtool.rst:580-595`). The version comes from the
  binary's `$VER` and falls back to **0.0** when there is none (`:953-958`). No space prints
  `ERROR adding filesystem! (no space in RDB left)` (`:979`).
- **Opening.**
  - `RawBlockDevice` → `ImageFile(file_name, read_only, block_bytes, fobj)` has **no offset
    parameter** (am: `fs/blkdev/RawBlockDevice.py:6-7`), so block 0 of the file is block 0 of the disk.
  - `RDisk.open` reads `RDBlock(self.rawblk)`, whose `blk_num` defaults to 0
    (am: `fs/rdb/RDisk.py:27-33`; `fs/block/rdb/RDBlock.py:159-160`). There is **no 16-block scan**.
  - So `rdbtool` cannot open a PiStorm card as it stands. The 0x76 area has to be extracted to its own
    file first.
- **Block allocation: in place, lowest free blocks first.**
  - `open` builds `used_blks` from the RDSK, every PART and every FSHD+LSEG (`RDisk.py:44-76`).
  - BADB blocks are **not** counted: `# TODO: add bad block blocks` (`:78`).
  - `max_blks = rdb_blk_hi + 1` (`:81`).
  - `add_filesystem` (`:722-752`) takes `_next_rdb_block()` for the FSHD, checks
    `_has_free_rdb_blocks(n)`, i.e. `len(used)+n <= max_blks` (`:526-527`), and allocates the
    **lowest-numbered free blocks, not necessarily contiguous** (`get_free_blocks`/`_alloc_rdb_blocks`,
    `:529-544`).
  - It then recomputes `high_rdsk_blk` as the maximum used (`_update_hi_blk`, `:553-559`).
- **Range checks.** Every block is ≤ `rdb_blk_hi` by construction. There is **no check against
  `LoCylinder` or partition `LowCyl`**: the tool trusts `RDBBlocksHi`.
  - This makes a real difference on ART-built cards. They declare `RDBBlocksHi = HighRDSKBlock`
    (§4.2), so `max_blks == len(used)`, and `fsadd` on an ART card would fail with "no space in RDB
    left". *Inferred from source, not run.*
- **Same DosType.** **No check at all**: a second `PDS\3` FSHD is appended beside the first.
  - Deleting first is not a safe route to replace a driver. `delete_filesystem` for `fid > 0` relinks
    `self.fs[-1]` (the *last* filesystem) instead of the predecessor `self.fs[fid-1]`
    (`RDisk.py:764-767`). That looks like a genuine amitools bug when a non-first entry among three or
    more is deleted.
- **FSHD content.** `FileSystem.create` sets `seg_list_blk` and `global_vec = 0xFFFFFFFF` through
  `set_flag`, which ORs each field's bit into `patch_flags`. The result is **0x180**
  (am: `fs/rdb/FileSystem.py:61-72`; `fs/block/rdb/FSHeaderBlock.py:254-256`). An LSEG's `size`
  is `(20 + len(data)) // 4` (`LoadSegBlock.py:28`). That rounds **down**, so a driver whose length is
  not a multiple of 4 would lose up to 3 bytes. Real hunk files are multiples of 4.
- **Write order: link last.**
  1. `fs.write()` writes the FSHD, then its LSEGs (`FileSystem.py:98-102`; `RDisk.py:738`).
  2. The link: `rdb.fs_list` when the list was empty, otherwise the last FSHD's `next`, rewritten with
     `write(only_fshd=True)` (`RDisk.py:740-747`).
  3. `self.rdb.write()` (`:749`), which also carries `high_rdsk_blk`.

  Nothing is referenced before it is written. When the list was empty, the link and HighRDSKBlock
  land in one RDSK write. Otherwise the last FSHD is rewritten before the RDSK, so a crash between
  the two leaves the new blocks linked but above `HighRDSKBlock`. No sync happens until close
  (`rdbtool.py:100-108`), and there is no backup.

---

## 4. What ART already has

### 4.1 Reading

- **Card reading.** `core/card/mod.rs:132-214` `read_card` → `read_card_from` finds each `0x76` area,
  reads an **8 MiB window** (`AREA_WINDOW_BYTES`, `:40`; `:189-199`) and calls `parse_rdb`.
  CaffeineOS's reserved range is 3 MiB, so it fits.
- **What `rdb.rs:594-752` `parse_rdb` checks today:**
  - RDSK within the first 16 blocks, found by **ID only** (`find_rdb_location`, `:454-466`). Its
    checksum is recorded but never enforced (`:602`).
  - The PART chain: ID, a visited set, a limit of 64, and containment (`:633-647`). Checksum recorded,
    not enforced (`:649`).
  - The FSHD chain (`parse_file_systems`, `:547-591`): ID, a visited set, `MAX_FILE_SYSTEMS` 32, and
    containment. The LSEG walk (`:510-539`) has `MAX_LSEG_BLOCKS` 4096, a visited set, containment,
    and an ID check that marks the entry `truncated`. Checksums recorded, not enforced (`:583`).
- **What it does not read:**
  - `rdb_BlockBytes`: `block_size = 512` is hard-coded (`:612`).
  - `rdb_RDBBlocksLo`/`Hi`, `rdb_LoCylinder`, `rdb_CylBlocks`, `rdb_HighRDSKBlock` and
    `rdb_BadBlockList`.
  - It builds no used-block set.
  - `free_cylinders` assumes 2 reserved cylinders (`:738`). That is wrong in general for a foreign RDB,
    though CaffeineOS happens to have LoCylinder 2.
- **Its policy is lenient on purpose:** "an RDB with one bad filesystem chain still has partitions
  worth reading" (`:494-498`). **That is right for a reader and wrong for an editor.** An editor needs
  a strict walker that refuses on anything it cannot account for.

### 4.2 Building

- `rdb.rs:778-1100` `create_rdb_layout` lays RDSK, PARTs, FSHDs and LSEGs contiguously from block 0
  (`:856-874`). The FSHD/LSEG serialisation is `:1003-1100`:
  - PatchFlags `0x180` with the ART-126 history (`:1034-1056`);
  - `dn_SegListBlock` and `GlobalVec` −1 (`:1061-1067`);
  - LSEG `SummedLongs = 5 + ceil(len/4)` (`:1085-1093`);
  - checksums via `compute_rdb_checksum` (`:424-436`).

  **This is reusable, but it sits inline.** It would have to be extracted into
  `build_fshd(...)`/`build_lseg_chain(...)`, guarded by the existing `create_rdb_layout` tests as a
  byte-identical-output check. `version_from_ver_string` (`:129`) is reusable as it stands.
- **ART-built RDBs declare `RDBBlocksHi = HighRDSKBlock = last structured block`** (`:883`, `:918`,
  `:922`), with `LoCylinder` 2 (`:919`). By comparison, hst-amiga's default is `RdbBlockHi = 2015`
  (ha: `RigidDiskBlocks/RigidDiskBlock.cs:167`) and amitools uses `rdb_cyls*cyl_blks-1`
  (`RDisk.py:295`). This is legal per the spec, but it leaves **zero** spare blocks inside
  `RDBBlocksHi` on ART's own cards (§3).
- `core/card/build.rs:233-238` writes that layout at the area offset. That happens only at build time.

### 4.3 Writing

- `core/volume/device.rs:269-378` `FileRegionMut`: in-place, bounded writes into a region of a raw
  file, plus `sync()` (`sync_all`).
- `core/volume/journal.rs:1-44`: a write-ahead **undo** journal. `begin(blocks)` saves the old
  contents and fsyncs. `write_block` refuses any block not named in `begin`, so a block that cannot be
  undone is never written. The journal survives the process and is found at mount time.
- `core/safety/backup.rs:68` `backup_file` covers a whole file, which is too big here. A slice backup
  of the RDB range would be new code.
- **Gap:** dynamic VHD cards are read through `core::vhd::DynamicVhd` (`card/mod.rs:125-149`), but
  `FileRegionMut` addresses raw file offsets. Whether an in-place VHD write path exists was **not
  checked**. A first version should refuse a dynamic VHD.

### 4.4 The call site

- `commands/preload.rs:329-344` maps `PreloadStep::ImportFilesystem { slot, driver, dostype, name }`
  to `formatter.import_filesystem`.
- `NativeFormatter::import_filesystem` refuses unconditionally (`core/preload/native.rs:171-186`,
  reasoning at `:29-51`).
- The hst-imager adapter launches the tool (`tools/hst_imager.rs:274-285`).
- The fallback contract is that "both refusals are refused before a single byte is written"
  (`commands/preload.rs:33-40`; `docs/architecture.md:637-644`).

### 4.5 A correction to the ART-117 record

The 2026-08-21 reasoning (`ISSUES.md:67-76`; `native.rs:31-48`) is about **rebuilding** an RDB with
`create_rdb_layout`: its 16/63 geometry, and a `size_mb` that cannot round-trip. It is correct about
that. **An append-only editor needs neither**:
- it never touches a PART block or a cylinder number;
- the only arithmetic it does is on block numbers inside `[RDBBlocksLo, RDBBlocksHi]`;
- geometry enters only as a precondition check (the first partition block).

The refusal's stated reason does not apply to the design below.

---

## 5. Safety: what can go wrong in place

| Hazard | Mechanism | Mitigation |
|---|---|---|
| Overlap with partition data | New blocks ≥ some partition's first block (`LowCyl × Surfaces × BlocksPerTrack` from **that partition's own envec**) | Refuse unless every allocated block is ≤ `RDBBlocksHi` **and** < min(first partition block) **and** ≥ `LoCylinder×CylBlocks` is consistent. Neither tool checks the partitions: hst-imager checks nothing in its path; amitools trusts `RDBBlocksHi`. |
| Overwriting live RDB blocks | The allocator misses a used block (amitools misses BADBs, `RDisk.py:78`) | Build the used set from all four chains, BADBs included. Refuse if `BadBlockList != -1` in version one. |
| Overwriting "free" but meaningful blocks | §0: the dead RDB copy and stale PFS3 blocks sit inside the range | Legitimate to overwrite, since the range is reserved for hardblocks. Journal every target block's old contents anyway, and allocate contiguously from `HighRDSKBlock+1`. On CaffeineOS, 132–2047 are zero. |
| Wrong `HighRDSKBlock` | Left below the new blocks | *Unverified* whether any driver cares. A tool that trusts it as "free above" could overwrite the new driver. **Raise it before the link write**: an over-high value is benign, an under-high one is not. |
| Checksum mistakes | A wrong `SummedLongs` range; a stale checksum after a pointer edit | Recompute on every block ART touches with `compute_rdb_checksum`. Re-read from disk and `verify_rdb_block_checksum` each one. For the LSEG last block, zero the tail so both amitools' 128-long sum and the spec sum agree. |
| Duplicate DosType | amitools adds a second one; hst-imager silently replaces it | Refuse by default, naming the card's version against the file's. Replace only when asked explicitly, via a pointer swap. |
| Power loss or a torn write | A partial sequence; a torn 512-byte sector on flash (*unverified* whether SD sectors are atomic) | Order writes so every prefix is a valid RDB (below), `sync` between stages, undo journal, RDB-range backup. |
| Driver not loadable | A non-hunk file, or truncation | Refuse unless the first longword is `HUNK_HEADER 0x000003F3`, length % 4 == 0, and there is a size cap (hst-imager uses 512 KB). |

**Write order that keeps every intermediate state valid** (append to a non-empty list):

1. The LSEG blocks, into free blocks. Nothing references them.
2. The new FSHD, with `Next=-1` and `SegList` set. Still unreferenced. `sync`.
3. The RDSK, with `HighRDSKBlock` raised. If the list was empty, `FileSysHeaderList` is set in this
   same write, and that is the commit. `sync`.
4. The last existing FSHD's `fhb_Next` → the new FSHD. This one sector write is the commit. `sync`.

Replacing is the same with one change: in step 2, `Next` = the old FSHD's `Next`, and step 4 swaps the
predecessor's pointer (or the RDSK head) from the old FSHD to the new one. The old blocks stay
orphaned until a later, optional zeroing pass after verification.

A crash before the commit leaves the old driver list byte-identical. A crash after it leaves the new
list complete. The one residual risk is a **torn** RDSK or FSHD sector at step 3 or 4, which would fail
its checksum and hide the card's partitions until repaired. The undo journal and the backup exist for
that case.

**How the tools compare:**
- hst-imager writes the RDSK first and rewrites every PART (§2.4).
- amitools writes data first and links last, but can leave the link written with `HighRDSKBlock` not
  yet raised (§3).
- **Neither makes a backup automatically.** hst-imager offers `rdb backup` separately. Its readme only
  advises in general: "it's highly recommended to make a backup of your physical drive or image file"
  (`hst-imager/readme.md:11`). Note also that its backup size is `RdbBlockHi × BlockSize`
  (`RdbBackupCommand.cs:58`), which leaves out block `RdbBlockHi` itself. That looks like an
  off-by-one: *read from source, not run.*
- No source was found saying a backup of the RDB area is established practice before an RDB edit.
  It is the obvious practice, and ART's own §92 BACKUP step asks for it. For this card it costs
  6144 × 512 = **3 MiB**.

---

## 6. Recommendation

**In-place append (amitools' allocation shape, with the link-last order in §5) is safer than
hst-imager's approach for a foreign card.**
- It writes only new blocks plus one or two existing sectors (RDSK, and the last FSHD).
- It never moves or rewrites a PART block.
- Every prefix of the write sequence is a valid RDB.

hst-imager's full renumber-and-rewrite is simpler and produces a canonical layout. But it writes the
RDSK first, rewrites every partition block, and has no range check in its path. Its advantage is years
of field use, which an ART editor would not have. That has to be earned by the verification below,
not assumed.

### 6.1 Outline

1. **`core/rdb` strict walker** (read-only, new). It reads every RDSK logical field and walks PART,
   FSHD+LSEG and BADB with checksums **enforced**. It returns the used-block set, the reserved range,
   each partition's first block from its own envec, and the existing filesystems. It takes block
   access through a reader over the area, not the 8 MiB window alone.
2. **A pure planner** `plan_filesystem_embed(walk, driver, dostype, version, name, mode)`. It returns
   either an ordered list of `(block, bytes)` stages or a typed refusal.
   - **Allocation:** the first contiguous run of free blocks starting at `HighRDSKBlock+1`, all
     ≤ `RDBBlocksHi` and < the first partition block. The FSHD/LSEG bytes come from builders extracted
     from `create_rdb_layout`.
3. **Apply** through `FileRegionMut` over the area, inside a `journal::begin` naming exactly the stage
   blocks. First write a backup of blocks `RDBBlocksLo..=RDBBlocksHi` to a `SAFE_CREATE` file. `sync`
   after each stage. Check `is_cancelled()` only between stages. Log via `oplog::write_result`.
4. **Verify** (§6.3), then commit the journal. On a failed verify, roll back and say where the backup is.

### 6.2 Refuse, each with an actionable sentence

- No `RDSK` with a valid checksum in blocks 0–15. `rdb_BlockBytes != 512`.
- Any chain block with the wrong ID or checksum, a loop, or a pointer outside `[RDBBlocksLo,
  RDBBlocksHi]`. "ART cannot account for every block in this RDB."
- `BadBlockList != -1` (version one).
- Inconsistent ranges, i.e. any of:
  - `RDSK block < RDBBlocksLo`;
  - `HighRDSKBlock > RDBBlocksHi`;
  - `RDBBlocksHi ≥` the first partition's first block;
  - two partitions overlapping.
- Not enough free blocks: name needed against available, as `create_rdb_layout` does (`rdb.rs:868-874`).
  The instruction is to use hst-imager, which renumbers. That also covers ART's own tight cards,
  unless `RDBBlocksHi` may be raised up to `LoCylinder×CylBlocks−1`. That raise is a separate,
  explicit decision.
- The DosType already present, when replace was not requested. Name the card's version against the
  file's, e.g. `19.2` against `19.x`.
- A driver that is not a hunk file, has length % 4 ≠ 0, is over a cap, or lacks a version when none
  was given.
- The image is a dynamic VHD (until a VHD write path exists). The slot is ambiguous or not `0x76`.

`ForeignRdbEmbedNotSupported` then narrows to these typed refusals, and the hst-imager fallback stays
reachable for them (for example "not enough free blocks", which hst-imager's renumbering can sometimes
solve).

### 6.3 Verification

- **Own reader:**
  - Re-open the file.
  - Byte-compare every written block against the plan.
  - Re-run the strict walker.
  - Assert every PART block and every block outside the write set is unchanged. Compare against the
    backup, at least over `0..=RDBBlocksHi`.
  - `read_card` shows the DosType with the right version, `segment_blocks`, and a `size_bytes` equal
    to the driver length.
  - The LSEG chain's bytes equal the driver file.
  - `partitions_missing_driver()` is empty.
- **Crash-point test** (synthetic, `ScratchDir`): inject a failure after each stage. Assert that each
  intermediate image parses, that the old list is intact before the commit and the new one after it,
  and that partitions are unchanged. Put the defect back (link first, HighRDSK last) and watch it fail.
- **Outside oracles**, as `#[ignore]` hooks env-gated on a **copy** of the owner's card:
  - hst-imager `rdb info` on the card, and `rdb fs export` of the new driver, byte-compared.
  - amitools `rdbtool <extracted-area> info` and `fsget <n>`. The area must be extracted, because
    rdbtool has no offset (§3). Mind its 128-long checksum.
  - The controlled experiment: hst-imager `rdb fs add` on copy A, ART on copy B. Report both block
    maps and both listings.
- **Ask the artefact:** mount the PDS partitions in WinUAE or on a PiStorm, and `version full` on the
  loaded handler.

### 6.4 Size

About **8 tasks**:
1. The strict walker, with refusal tests and a CaffeineOS ignored hook.
2. Extract the FSHD/LSEG builders (byte-identical guard).
3. The planner, with typed errors, tests and mutation of each guard.
4. The journaled apply, backup and ordering.
5. Wire `NativeFormatter::import_filesystem`, narrow the fallback, update `preload.rs` tests
   (`:1488-1525`).
6. Refusal and outcome strings in both catalogues, plus the oplog.
7. The crash-point test and the oracle hooks (hst-imager, amitools, WinUAE).
8. Docs: ISSUES, FEATURES, STATUS, and the architecture.md rule at `:637-639`.

A ninth task if replace-same-DosType is in scope rather than refused.

### 6.5 Not verified here

- Whether any driver (scsi.device, pi-scsi, HDToolBox) uses `RDBBlocksHi`/`HighRDSKBlock`.
- HDToolBox's own write order.
- Whether SD or flash sector writes are atomic.
- Whether ART has an in-place write path for dynamic VHDs.
- The origin of the dead RDB copy at 2048 (inferred as an earlier layout).
- The amitools `delete_filesystem` relink issue: read, not run.
- The hst-imager backup off-by-one: read, not run.
- The claim that `rdbtool fsadd` fails on ART-built cards: inferred, not run.
- None of the tools' write behaviour was exercised. Everything in §2–§3 is from source.
