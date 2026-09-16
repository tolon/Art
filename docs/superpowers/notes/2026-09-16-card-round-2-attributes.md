# One-button card, round 2 — Amiga attributes: where they live and what consumes them

*2026-09-16/17, branch `art-card-round-2` (HEAD 29a671e). Research only: no production code touched.
Describes the tree, the crates and hst-imager 1.6.616 on this day. Follows
[2026-09-16-card-round-2-research.md](2026-09-16-card-round-2-research.md) and
[2026-09-16-card-round-2-lzx-and-names.md](2026-09-16-card-round-2-lzx-and-names.md).*

## Question

The owner decided (2026-09-16, decision 8) that protection bits, comment and date travel from
archive entries (LHA, ZIP, 7z, LZX) and from HDF-extracted files **and folders** into staging, so
they land on the card. Before the plan: (1) where does each format store them, (2) what does ART's
`.uaem` format hold and who reads it, (3) does ART's native PFS3 copy apply them, (4) does
hst-imager, and (5) what do the owner's real archives and HDFs actually carry?

Scratch for everything run: `E:\amiga\ProjeART\build\tmp\card-r2\attr\` (`lha_attr_dump.py`,
`zip_attr_dump.py`, `hst_uaem_probe.py`, `hst_dir.py`, `xdf-*.txt`). Owner files were only read.

## Findings

### 1. LHA — the attribute byte is the Amiga protection byte; comment after the NUL; date per level

- **Protection.** XADMaster `XADLZHParser.m` (<https://github.com/MacPaw/XADMaster/blob/master/XADLZHParser.m>)
  reads one attribute byte for every level (`int attrs=[fh readUInt8]`) and stores it **unchanged**
  as `XADAmigaProtectionBitsKey` when the OS is Amiga, as `XADDOSFileAttributesKey` when it is
  MS-DOS. The OS is the header's OS id; when a level-0 header carries none, it is *guessed*:
  `if([name matchedByPattern:@"\\.(lha|run)$" options:REG_ICASE]) guessedos='A'; else guessedos='M';`
- `delharc` 0.8.0 exposes the byte as `LhaHeader::msdos_attrs` (`MsDosAttrs::from_bits_retain`,
  `src/header/parser.rs:310` — unknown bits are **kept**), the OS id as `os_type`, the level as
  `level`, and the raw date as `last_modified` (`src/header.rs:34-57`). `parse_os_type` reads a
  level-0 OS id from the extended area's first byte (`header.rs:141-150`).
- **Comment.** ART already reads it: `core/lha/mod.rs` `entry_name` splits a level-0/1 name at the
  first NUL, Latin-1 (`split_at_nul`, `decode_latin1`) and `LhaEntry.comment` carries it. XADMaster
  does the same (`namebuffer[actualnamelen]==0` → `XADCommentKey`) and also reads extension headers
  `0x3f`/`0x71` as a comment. `ArchiveBackend::entries` (`core/archive/lha.rs:123-127`) drops it.
- **Date.** Level 0/1: MS-DOS date/time, no time zone; level 2/3: Unix seconds, UTC (`delharc`
  `header.rs:48-55`; XADMaster `XADDateWithMSDOSDateTime` vs `dateWithTimeIntervalSince1970`).
- **Separators.** Level-0 names on the owner's `BoingBag39-1.lha` use `\` between components
  (dump below). `core/lha/mod.rs::decode_path` turns only `0xFF` into `/` and keeps `\` verbatim.
- **Measured** (`lha_attr_dump.py`, raw headers, no delharc):

  ```
  SnoopDos.lha        entries 9     levels {1: 9}     os {'A': 9}      attr {0x0: 9}  comments 0
  pfs3aio.lha         entries 2     levels {1: 2}     os {'A': 2}      attr {0x0: 2}  comments 0
  BoingBag39-1.lha    entries 1112  levels {0: 1112} os {none: 1112}  attr {0x0: 826, 0x2: 284, 0x40: 1, 0x20: 1}  comments 23
    0x2  BoingBag3.9-1\C\Catalogs\dansk\Updater.catalog
    0x40 BoingBag3.9-1\Contribution\BladeEnc_WOS\README_Amiga
    0x20 BoingBag3.9-1\Contribution\SmartFilesystem\L\SmartFilesystem
    0x0  ...\ToolsDaemon22Patch\spatch  comment '6.50 (26.8.93) '
    0x0  ...\Manuals\English\FAQ\HTML\about.html  comment 'gdonner@www.gregdonner.org//gregdonner.org/os39faq'
  ```

  284 `.catalog` files with `0x02` read as Amiga "E not granted" (`----rw-d`, a data file), which
  is plausible; as MS-DOS "hidden" on 284 catalogs it is not. The byte is the FIB byte, RWED stored
  inverted, exactly what `uaem::parse_bits` produces.

### 2. ZIP — Amiga host: external attributes' high word; comment is the per-file comment

- APPNOTE "version made by" host 1 is Amiga; `zip` 8.6.0 names it `System::Amiga = 1`
  (`src/types.rs:41-45`). `ZipFileData` is public through the public `HasZipMetadata` trait
  (`src/read.rs:727-731`, re-exported `lib.rs:19`) with public `system` and `external_attributes`
  (`types.rs:214`); `ZipFile::comment()` (`read.rs:971`) and `last_modified()` (`read.rs:996`,
  MS-DOS date/time, `datetime.rs:315-327` `datepart`/`timepart`). `unix_mode()` returns `None` for
  an Amiga host with a zero high word (`types.rs:320-350`) — it is not the accessor to use.
- Info-ZIP UnZip, the Amiga port (`amiga/amiga.c:181-187`, <https://github.com/madler/unzip/blob/master/amiga/amiga.c>):
  ```c
  case AMIGA_:
      if ((tmp & 1) == (tmp>>18 & 1))
          tmp ^= 0x000F0000;      /* PKAZip compatibility kluge */
      /* turn off archive bit for restored Amiga files */
      G.pInfo->file_attr = (unsigned)((tmp>>16) & (~S_IARCHIVE));
  ```
  and it restores the per-file comment as the filenote (`SetComment`, `:568-572`, with `-N`).
- Info-ZIP Zip's Amiga side writes the high word from `stat()`, not from the FIB directly
  (`amiga/amigazip.c`, `filetime()`, ~`:310-315`, <https://raw.githubusercontent.com/LuaDist/zip/master/amiga/amigazip.c>):
  `*a = ((ulg)s.st_mode << 16) | !(s.st_mode & S_IWRITE);` — the low bit is MS-DOS read-only, set
  when **not** writable. The kluge above therefore reads: when the read-only bit equals bit 18 (the
  high word's `0x04`, W), the high word cannot be in Zip's own set-means-granted form and is flipped
  into it (PKAZip stored FIB-style bits). After it, `RWED` in the high word are *set = granted*, so
  the FIB form ART stores is `((high & 0xFF) & !0x10) ^ 0x0F`. Worked: Info-ZIP, a `----rwed` file →
  high `0x0F`, low bit 0, bit 18 = 1 → no flip → FIB 0; PKAZip, FIB 0 stored, low bit 0 → equal →
  flip → `0x0F` → FIB 0. That `st_mode`'s `RWED` are set-means-granted under the Amiga C library is
  **inferred** from the kluge's own consistency, not read from a header.
- Info-ZIP's `extrafld.txt` (<https://raw.githubusercontent.com/LuaDist/zip/master/proginfo/extrafld.txt>)
  lists **no Amiga extra field**; the hypothesised `0x4D49 "AMIGA"` field is not in it.
- **Measured** (`zip_attr_dump.py`, Python `zipfile`): every ZIP in `Amigatolon\paketler` was made on
  a PC — `create_system` 0 (MS-DOS), 3 (Unix) or 11 (NTFS); **no host-1 (Amiga) ZIP and no per-file
  comment** in the owner's material.

### 3. 7z — no Amiga attributes; a UTC date

- `sevenz-rust2` 0.21.4 `ArchiveEntry` (`src/archive.rs:90-112`): `name`, `has_last_modified_date`,
  `last_modified_date: NtTime` (a FILETIME, `time.rs:14`, `impl From<NtTime> for u64` `:67-70`),
  `has_windows_attributes`, `windows_attributes`. No protection, no comment. The date is UTC.

### 4. LZX — attribute byte in its own bit order; comment; no trustworthy date

- Record layout (`unlzx.c` 1.1 struct comment `:1328-1345`, cross-read with XADMaster
  `XADLZXParser.m:51-66`): `[0]` attributes, `[2..6]` unpacked LE, `[6..10]` packed LE, `[10]` machine,
  `[11]` pack mode, `[12]` flags (bit 0 merged), `[14]` comment length (0–79), `[15]` version,
  `[18..22]` date, `[22..26]` data CRC LE, `[26..30]` header CRC LE (computed with those four bytes
  zeroed, then over the name and comment, `unlzx.c:1049-1072`), `[30]` name length.
- **The attribute byte is not the FIB byte.** Two independent sources agree on its bit order:
  `unlzx.c:1101-1108` prints `h`=0x20, `s`=0x40, `p`=0x80, `a`=0x10, `r`=0x01, `w`=0x02, `e`=0x08,
  `d`=0x04, a set bit meaning *granted*; XADMaster `XADLZXParser.m:126-137` converts it to FIB bits:
  `!(a&0x01)→0x08`, `!(a&0x02)→0x04`, `!(a&0x04)→0x01`, `!(a&0x08)→0x02`, `a&0x10→0x10`,
  `a&0x20→0x80`, `a&0x40→0x40`, `a&0x80→0x20`.
- Comment: carried (WiFi archive: 2 comments, research 2 § A3). Date: **not carried** — three
  decoders give three years for the same bytes (research 2 § A3); no source settles the year rule.

### 5. OFS/FFS (HDF) — files get a sidecar, directories do not

- `core/volume/write/copy.rs::write_one_file` (`:1015-1036`) reads protection, date and comment
  from the header block and writes `<file>.uaem` through `sidecar_for` (`:754-769`, skipped when
  bits are default, comment empty and date the epoch). `extract_dir`'s `HostTarget::Descend` branch
  (`:1076-1090`) creates the directory and writes **no** sidecar. `adf::fs::read_header_on` →
  `HeaderBlock { protection, comment, date }` (`adf/blocks.rs:189-212`) reads the same fields for the
  drawer's `.info` read separately through `extract_file_on`.
- **Measured** (`xdftool <hdf> list`, amitools): `A Prehistoric Tale v1.1.hdf` — in the game drawer
  `ReadMe.info` is `h---rwed`; `Disk.1` 23.10.2021; `PrehistoricTale.slave`, `.info`, `ReadMe` carry
  the epoch date 01.01.1978; the drawer itself 23.10.2021 06:32:58; root `Disk.info` `----rw-d`. No
  comments. `B-17 Flying Fortress v1.0.hdf`: 72 lines, only `Disk.info` (`----rw-d`) outside
  `----rwed`. `s/startup-sequence` has **no** `s` bit in either.

### 6. ART's `.uaem` and who reads it

- Format: `core/volume/write/uaem.rs:12-30` — one line `hsparwed YYYY-MM-DD HH:MM:SS.TT comment`,
  WinUAE's; `Sidecar { protection: u32 (FIB, RWED inverted), date: AmigaDate, comment }`, comment ≤ 79
  (`MAX_COMMENT_LEN`), file ≤ 4096 bytes. `sidecar_path(x)` = `x.uaem` — for a directory too.
- Native copy (`core/preload/native.rs`): `collect_into` skips `*.uaem` (`:860-865`). PFS3
  (`copy_in_pfs3`, `:1060-1090`): applies **protection only** through `update_dir_entry_protection`;
  counts `comments_lost`/`dates_lost`; every entry is stamped `clock.amiga_now()` (`:1024`). FFS
  (`copy_in_ffs`, `:1185-1210`): a directory gets `set_attributes(block, Some(protection), None,
  Some(date))` and a file `FileMeta { protection, date }` — **the comment is dropped for both and not
  counted** (`FileMeta` has no comment field, `volume/write/mod.rs:120-125`).
- **ART-116's "no fix available" is stale.** `libpfs3` is ART's vendored copy (`0.1.3+art.11`,
  `vendor/libpfs3/ART-PATCH.md`). Its writer already has `set_entry_date(Option<(u16,u16,u16)>)`
  (`writer.rs:171-179`, added for ART-317), used by `build_dir_entry` for every new entry
  (`writer.rs:2139-2142`) — so a sidecar's date can be carried by setting it before
  `write_file_in`/`create_dir_in`, with no patch. The comment is written as length 0
  (`writer.rs:2149`); the entry layout already reserves it (`extra_fields_offset(nlen, clen)`,
  `ondisk/direntry.rs:102-108`) and the reader decodes it Latin-1 (`direntry.rs:287-294`), so
  carrying it is a small vendor patch (a comment setter consumed by `build_dir_entry`). Its entry
  then grows by the comment's bytes — `core/card/sizing.rs::entry_bytes` (`:142-153`) counts none.

### 7. hst-imager — `.uaem` is read only with `--uaemetadata UaeMetafile`

- `hst.imager fs copy --help` (1.6.616): `-uae, --uaemetadata <None|UaeFsDb|UaeMetafile>  Type of UAE
  metadata to read and write. [default: UaeFsDb]`. ART's `copy_args` (`tools/hst_imager.rs:166-175`)
  passes no such option, i.e. `UaeFsDb` — WinUAE's `_UAEFSDB.___` database, **not** `.uaem` files.
- **Controlled experiment** (`hst_uaem_probe.py`). One variable: the option (absent vs
  `UaeMetafile`). Decided beforehand what to count: does `C/Assign.uaem` land as a file, and what
  protection/date/comment do `C/Assign` (sidecar `--p-rwed 2021-04-13 02:43:13.68 hello comment`) and
  the directory `C/Sub` (sidecar `--p-rwed … sub note`) get. Control: `C/Plain`, no sidecar. Target: an
  ADF made by `hst.imager adf create --format` (FFS), source `src\*`, `--recursive --makedir`.

  ```
  default      C: Sub <DIR> 09/16/2026 23:54:16 ----RWED   (no comment)
                  Assign 100 B 09/16/2026 23:54:16 ----RWED   (no comment)
                  Plain   10 B 09/16/2026 23:54:16 ----RWED
                  1 directory, 2 files, 110 B
  UaeMetafile  C: Sub <DIR> 09/16/2026 23:54:17 --P-RWED   sub note
                  Assign 100 B 04/13/2021 02:43:13 --P-RWED   hello comment
                  Plain   10 B 09/16/2026 23:54:16 ----RWED
                  1 directory, 2 files, 110 B
  ```

  Both arms: 2 files — the `.uaem` files are **not** copied as files. Default arm: bits, date and
  comment silently dropped. `UaeMetafile`: file bits, date and comment applied; directory bits and
  comment applied, **directory date not** (stamped now). Control unchanged in both.
- Emu68-Imager passes `--uaemetadata UaeFsDb` (research 1 § 7) because its interim tree comes from
  hst-imager's own extraction, which writes that database; ART's staging writes `.uaem`.

### 8. Prior art

- WinUAE's `.uaem` is what ART writes (`uaem.rs` module doc) and what hst-imager's `UaeMetafile`
  reads (§ 7). Emu68-Imager keeps attributes by staying inside hst-imager's own metadata format;
  emu68hatcher mirrors a host folder with `copy_contained_tree` and no Amiga metadata (research 1 § 7).

## Eliminations

- *ZIP stores Amiga attributes in an extra field `0x4D49`* — eliminated: `extrafld.txt` lists no Amiga
  field; Info-ZIP's Amiga port uses the external attributes' high word.
- *7z carries protection bits or comments* — eliminated: `sevenz-rust2`'s entry has neither.
- *The LZX attribute byte is the FIB byte* — eliminated: two sources give a different bit order.
- *An LHA attribute byte could be MS-DOS attributes on the owner's archives* — eliminated for the three
  dumped: OS id `A` on two, and on the level-0 one the 284×`0x02` catalogs only make sense as Amiga.
- *hst-imager reads `.uaem` by default* — eliminated by the experiment's default arm.
- *hst-imager copies `.uaem` files onto the volume* — eliminated for both arms (2 files each).
- *`libpfs3`'s writer cannot take a date* — eliminated: `set_entry_date` exists and is honoured by
  `build_dir_entry`.
- *The native FFS copy keeps comments* — eliminated by reading `copy_in_ffs`.

## What the plan must do

1. Give `ArchiveEntry` an Amiga attribute record (protection in FIB form, comment, and a raw date:
   MS-DOS bits or Unix seconds) filled by every backend: LHA (OS `A`, or level 0 with no OS id and a
   `.lha`/`.run` archive name — XADMaster's rule; comment from `entry_name`; date by level), ZIP (host
   Amiga only for bits and comment: UnZip's kluge, archive bit cleared, then `^ 0x0F` into FIB form;
   DOS date for every host), 7z (date only), LZX (bits converted by XADMaster's table, comment,
   **no date**).
2. Convert dates where the clock is known: MS-DOS bits are wall-clock time
   (`clock::amiga_from_wall`); Unix seconds are UTC (`AmigaClock::amiga_from_unix`).
3. Write `.uaem` sidecars in staging from those records (content.rs, beside the extracted entry), never
   over a `.uaem` the archive itself carried; refuse a source whose `.uaem` does not parse.
4. Write a directory's sidecar in `extract_dir` (HDF drawers and every sub-drawer).
5. Native PFS3 copy: read the sidecar **before** creating the entry; `set_entry_date` from it; a vendor
   patch `Writer::set_entry_comment` (Latin-1, ≤ 79 bytes) consumed by `build_dir_entry`; stop counting
   what is now carried; `ART-PATCH.md` in the same commit. `sizing::entry_bytes` gains the comment's
   bytes, with the calibration test extended.
6. Native FFS copy: carry the comment (`set_attributes(.., Some(comment), ..)`) for files and dirs.
7. hst-imager: `copy_args` passes `--uaemetadata UaeMetafile`.
8. Normalise `\` to `/` when placing LHA level-0 entry names for the card (content.rs placement only,
   never in `decode_path`, whose verbatim rule is a security decision).

## What this does not prove

- The hst-imager experiment wrote an **FFS ADF**, not a PFS3 partition inside an RDB; that
  `UaeMetafile` behaves the same on PFS3 is assumed, not measured. Its root listing crashed
  (`IndexOutOfRangeException` in `FsDirCommand` for `adf` and `adf\`), so only `C` was listed.
- No Amiga-made ZIP exists in the owner's material: the ZIP rule is Info-ZIP's code, not a measurement.
- The LHA reading is three archives; an LHA made by MS-DOS LHA with a `.lha` name would have its DOS
  attributes read as Amiga bits by XADMaster's guess, and so by ART's.
- LZX's attribute conversion is two sources read, not a byte compared against an Amiga's `list`.
- No PFS3 volume holding a comment written by the patched writer has been read by pfs3aio on an Amiga.
