# How real PiStorm distributions lay out a card (measured, 2026-09-11)

Why this exists: the one-button card design (the OS Builder's four tabs producing a complete `.img`
with System, Work, Games, Stuff … partitions sized automatically) must size the card right first
time. The owner's rule for it: research every time, never trust memory, and use what successful
projects do. This note is the measurements and sources, **as of the day they were taken** — re-run
before building on them. Every command below can be re-run.

## 1. The images themselves

Four images on the owner's disk, the MBR read byte by byte (Python, `struct` over sector 0), the RDBs
read twice — by ART's own reader (`ART_CARD_IN=<img> cargo test --lib read_real_card_when_asked --
--nocapture`) and by hst-imager 1.6.616 (`hst.imager rdb info "<img>\mbr\<n>"`, `n` counted from 1,
the FAT32 being 1).

| Image | Card | File bytes | FAT32 | `0x76` areas | Last partition ends at |
|---|---|---|---|---|---|
| `CaffeineOS_Storm_9317.img` | 64 GB | 63 864 569 856 | 0x0C, 1 177 550 848 | 1 | 61 714 989 056 |
| `MultibootOS128_2.2_65135ad.img` | 128 GB | 127 999 672 320 | 0x0C (boot flag), 1 177 550 848 | 2 | 121 601 048 576 |
| `WHDLoadPiStorm-180224.img` (Zeb/SLP) | 32 GB | 31 104 958 464 | 0x0B, 1 610 612 736 | 1 | 30 657 216 512 |
| ART's own `1card.img` | "64" | **68 719 476 736** | 0x0C (boot flag), 1 177 550 848 | 1 | 68 719 476 736 |

- Every distribution ends its partitions **below the decimal capacity** of the card it is for, and
  leaves the tail unallocated (7-Zip lists that tail as an unnamed item; it is not a partition):
  CaffeineOS leaves 2.15 × 10⁹ bytes, MultibootOS 6.40 × 10⁹, Zeb 0.45 × 10⁹.
- The MultibootOS readme: *"you will need a 128 GB microSD card"*, and WinUAE shows the card as
  *"around 119.1GB"* — 119.1 GiB is ≈ 127.9 × 10⁹ bytes. Its image file (127 999 672 320) is a little
  larger than that and still writes, because everything past 121.6 × 10⁹ is unallocated.
- **ART builds "64" as 64 GiB** — past any 64 GB card. Filed as ART-308.

## 2. Partitions

**CaffeineOS (64 GB), one area:** SDH0 PDS\3 cyl 2–535 (12 heads × 256 sectors) ≈ 0.84 × 10⁹ bytes;
SDH1 PDS\3 ≈ 56 × 10⁹; 600 buffers, one driver in the RDB.

**MultibootOS (128 GB), two areas** (hst-imager):

| Area | Partition | Size | Bootable / pri | Notes |
|---|---|---|---|---|
| 1 (46 GB, 128 heads × 64 sectors, RDB 8 MB) | SDH0 | 800 MB | yes / 1 | CaffeineOS system |
| | SDH1 | 45.1 GB | no / 0 | CaffeineOS data |
| | *unallocated* | 80 MB | | end of area |
| 2 (66.2 GB, 16 × 212, RDB 3.3 MB) | ADH0 | 101 MB | yes / 2 | boot selector |
| | ADH1 | 1.1 GB | yes / 0 | AmigaOS 3.2 |
| | AGS0 | 972 MB | yes / 0 | Amiga Game Selector system |
| | AGS1 … AGS10 | 2.0 – 7.7 GB each | no | games, ten partitions |
| | AK0, AK1 | 8 GB each | AK0 yes | empty, for AmiKit |
| | *unallocated* | 500 MB | | end of area |

All PFS3 (`PDS\3`, pfs3aio 19.2 in area 1's RDB; area 2's RDB carries FFS 45.16 `L:FastFileSystem`),
600 buffers, MaxTransfer 0x1FE00, Mask 0x7FFFFFFE, reserved 2 — **the same values ART's RDB writer
defaults to** (`core/rdb.rs`). SDH0's root: `C Classes Devs Docs Expansion Fonts L Libs Locale Opus5
Prefs Rexxc S Storage System T Tools UHC Utilities WBStartup` and their icons.

**Zeb's WHDLoad pack (32 GB):** DH0 DOS\3 (FFS) cyl 2–43 at 255 × 63 ≈ 345 MB; DH1 PFS\3 ≈ 28.6 GB;
30 buffers; two drivers in the RDB.

## 3. What the MultibootOS readme says about content

`MultibootOS-2.2-Readme.pdf` (text via `pypdf`, 11 pages): AmigaOS 3.2 is installed **on the Amiga at
first boot** from ADFs the user copies to `+AmigaOS32` on the FAT32 partition; AmiKit's files are
copied by the user into the empty AK0/AK1 *"using an emulator, using the Directory Opus file manager"*;
the distribution is chosen at boot by a menu that copies `config_{distro}.txt` over `config.txt`.
Nothing is placed from the host automatically — the owner's request goes further than this example.

## 4. The content inside a WHDLoad HDF (Enzo's per-game sets)

`xdftool "<A Prehistoric Tale v1.1.hdf>" list` (amitools): each HDF is a small bootable FFS volume —
`C/{Assign,SetPatch,WHDLoad}`, `Devs/Keymaps`, `s/{startup-sequence,WHDLoad.prefs}` — beside the game
drawer `PrehistoricTale/` (`.slave`, `Disk.1`, icon) and `PrehistoricTale.info`. Placing a game on a
Games partition means the drawer and its `.info`; the boot scaffold is not needed once WHDLoad is on
System. Zeb's pack readme: the WHDLoad Kickstarts go into `Devs:Kickstarts` by hand.

## 5. How full the MultibootOS partitions are

`hst.imager fs dir -r "<img>\mbr\<n>\rdb\<drive>"`, the tool's own total line (its "GB" is GiB — AGS1
is cylinders 1315–3719 at 16 × 212 × 512 = 1 736 704 bytes each, 4.18 × 10⁹ bytes, shown "3.9 GB"):

| Partition | Content | Partition | Full | Files | Average file |
|---|---|---|---|---|---|
| SDH0 (CaffeineOS system) | 361.9 MB | 800 MB | 45 % | 22 506 | 16 KB |
| ADH0 (boot selector) | 11.7 MB | 101 MB | 12 % | 613 | 19 KB |
| AGS1 (games) | 3.1 GiB | 3.89 GiB | 80 % | 140 602 | 23 KB |
| AGS3 (games) | 5.7 GiB | 7.5 GiB | 76 % | 88 211 | 68 KB |

The game partitions are given about a quarter to a third of their content again as headroom; the
system partitions more than their content. Content is counted by file bytes; what PFS3 spends per
file on top is section 6's question.

**The owner's own AmigaOS 3.9 tree** (`E:/amiga/Amigatolon/sonuclar`, the build ART made, host bytes
via Python `os.walk`): 23 553 356 bytes, 4 599 files, 277 directories; average 5 121 bytes, **median
521**, 2 297 files of 512 bytes or less. So a system partition's size is set by what will be installed
onto it later, not by the tree, and per-file overhead dominates the tree's own footprint.

A real card's own byte count could not be read here: `hst.imager list` needs administrator rights to
see physical drives.

## 6. The established projects' own code, real capacities, PFS3 (outside research, 2026-09-11)

Read from source, not READMEs; downloaded copies were kept in the session scratchpad.

**emu68hatcher** (MIT, `rootrootde/emu68hatcher` @ 3f38b22, `src/main/python/emu68hatcher/`):
card size is *"95% of decimal GB for SD card safety"* — `gb * 1_000_000_000 * 0.95`
(`config/partition_helpers.py:34-36`); EMU68BOOT FAT32 `min(disk/15, 1 GiB)`; Workbench SDH0 PFS3
`disk // 15` rounded down to whole cylinders, bootable, priority 0 (`:136`); Work SDH1 the rest,
split every 101 GiB into Work, Work_1 … (`:138-141`); MBR 1 048 576 + 50 688 bytes and two RDB
cylinders (16 × 63 × 512) reserved (`config/constants.py:7-14`); buffers 30, MaxTransfer 0x1FE00,
Mask 0x7FFFFFFE (`config/partition_models.py:33-37`); partitions + overhead must fit the disk
(`:106-110`). **No content-based sizing.**

**Emu68-Imager** (MIT, code in `mja65/Emu68-Imager-Software` @ 428c3cf): same geometry and overheads
(`Assets/Variables/SetVariables.ps1:8-16`); EMU68BOOT and Workbench each disk/15 capped at 1 GiB, PFS3
10 MiB–101 GiB, System ≥ 100 MiB (`SetupDisk/Set-InitialDiskValues.ps1:18-34`,
`Get-MinimumPartitionSizes.ps1:56-79`); Work the rest, split at 101 GiB; a real card is taken as its
reported size minus 3 076 KiB (`MainWindow/Get-RemovableMedia.ps1:10`); imported files are compared by
raw length, no rounding (`WPF_DP_Button_ImportFiles.ps1:20-21`).

**HstWB:** a 4 GB template is DH0 300 MB + DH1 3.2 GB, PFS3, 512-byte blocks (`images/README.md`).

**Real capacities** (fdisk output quoted in the threads opened): 32 GB 31 914 983 424; 64 GB SanDisk
Extreme PRO 63 864 569 856 (= CaffeineOS's image size exactly); 64 GB other brand 62 534 975 488;
128 GB 127 865 454 592; two 8 GB SanDisk cards 1.5 % apart. Same label, up to ~2 % apart — the margin
has to cover the smallest card, which is what hatcher's 95 % does: 30.4 × 10⁹ for 32 GB, 60.8 × 10⁹ for
64, 121.6 × 10⁹ for 128 (MultibootOS's last partition ends at 121.60 × 10⁹).

**Write tools:** rpi-imager refuses an image larger than the device, no margin
(`src/imagewriter.cpp:1216-1221`); Etcher likewise (`lib/shared/drive-constraints.ts:78-104`);
Win32DiskImager warns and offers to truncate (`src/mainwindow.cpp:396-438`).

**PFS3** (`tonioni/pfs3aio` @ 211f7f0): normal mode up to 213 021 952 blocks = 109 067 239 424 bytes
(101.58 GiB — the imagers' 101 GiB cap); above ~5.24 GB SUPERINDEX mode; above 104 GB experimental
(`blocks.h:106-120`, `format.c:358-431`, the Aminet readme agrees). Fixed format cost from
`CalcNumReserved` (`format.c:553-570`): 2.46 % at 1 GB, 1.62 % at 4 GB, 0.93 % at 30 GB, 0.75 % at
60 GB. **5 % of the free blocks is held back** (`alwaysfree = blocksfree/20`, `format.c:466`, used in
`allocation.c:158`) — one source, to be measured on ART's own `libpfs3`. Per file: data in whole
512-byte blocks, a 12-byte anode per contiguous run, directory entries 17 bytes + name + comment in
1 KB directory blocks from the reserved area — no per-file header block as on FFS.

**Not found:** AmiKit's and CaffeineOS's layout rules (search snippets only), where Zeb's
31 104 958 464 comes from (it is exactly 29 664 MiB), a practical PFS3 file count.

## 7. What this means for ART's sizing rule

- Image total = **95 % of the card's decimal gigabytes** (hatcher's rule; covers the smallest real card
  of each label measured above), and the last partition ends inside it (ART-308, ART-309).
- FAT32 boot keeps ART's measured 1.10 GiB (both of the owner's real cards; CaffeineOS, MultibootOS).
- System: 1 GiB (Emu68-Imager's default on ≥ 16 GB cards; MultibootOS 800 MB with 45 % used).
- A content partition: its content on PFS3 (every file rounded to 512-byte blocks, directory space,
  the format's reserved area, the 5 % always-free), plus a quarter again (MultibootOS's game partitions
  are 76–80 % full), never over 101 GiB.
- Work: the rest, split at 101 GiB like both imagers.
- The estimate is checked against a real `libpfs3` format before it is trusted.
