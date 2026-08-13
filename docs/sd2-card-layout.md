# What a real PiStorm card looks like

The adaptation checklist `ART-research-distro-profiles.md` §8.2 parked until a
real download could be read. Two were, on 2026-08-13 — **CaffeineOS_Storm 9317**
(59.48 GiB) and **MultibootOS128 2.2** (119.21 GiB) — with headers only, the
images opened read-only and never written to.

Everything below is measured off those two files. Where they agree, that is the
convention ART should follow; where they differ, that is a thing ART must not
assume.

---

## 1. The shape, and the thing that broke first

Both cards are **MBR-partitioned**, and the Amiga's RDB is **not at the start
of the file**:

| | CaffeineOS 9317 | MultibootOS 2.2 |
|---|---|---|
| MBR #0 | `0x0C` FAT32 LBA, 1.10 GiB at byte 1 048 576 | the same, byte-for-byte |
| MBR #1 | `0x76`, 46.00–56.38 GiB at byte **1 178 599 424** | 46.00 GiB at byte **1 178 599 424** |
| MBR #2 | — | `0x76`, 66.15 GiB at byte 50 570 723 328 |

The FAT32 partition is identical in both: LBA 2048, 2 299 904 sectors,
`MSDOS5.0`, label `NO NAME`, 512-byte sectors, 8 sectors per cluster. It is the
boot partition Emu68 reads `config.txt`, `cmdline.txt` and the Kickstart from.

Type **`0x76`** is what both use for an Amiga area. Each one starts with its own
`RDSK` at block 0 *of that area*.

**This is what ART could not read.** `find_rdb_location` scans the first 16
blocks **of the file**, and on a real card those are the MBR and the start of
the FAT32 partition — so ART found no RDB on either image. Filed as
[ART-095](ISSUES.md); it is the single thing that blocked SD-2a, and it is
not a distribution quirk but how every PiStorm card is laid out.

## 2. Geometry is not standardised — do not assume it

| | cylinders | sectors | heads | blocks/cylinder |
|---|---|---|---|---|
| CaffeineOS | 38 488 | 256 | 12 | 3 072 |
| MultibootOS area 1 | 11 776 | 64 | 128 | 8 192 |
| MultibootOS area 2 | 40 899 | 212 | 16 | 3 392 |

Three areas, three geometries, none of them ART's own (16 heads × 63 sectors =
1 008). They are all legal: an RDB describes its own geometry and everything
else is derived from it. The lesson is that an adapter must **read** the
geometry and never compute it — which `core/rdb` already does on the way in,
and `core/volume` already does through `VolumeGeometry`.

## 3. The DosEnvec values the field actually uses

Every partition on both cards, without exception:

```
maxtransfer = 0x0001FE00      mask = 0x7FFFFFFE      buffers = 600
```

**ART wrote `maxtransfer = 0` and `mask = 0`**, and 100 buffers by default —
[ART-096](ISSUES.md), fixed 2026-08-13. A mask of zero says no memory is acceptable
for a transfer, which is not what anybody means by it, and it is the sort of
field that costs nothing to get right and is very hard to diagnose when wrong.

Partition names are `SDH0`, `SDH1` — not `DH0`. On a machine that also has an
IDE drive, `DH0` is already taken; the SD card's partitions are named apart from
it on purpose. ART's wizard offers `DH0`.

## 4. Filesystems and drivers

Both cards are **PFS3 throughout** (`PDS\3`, `0x50445303`) with **PFS3 19.2** in
the RDB — 128 LSEG blocks, 62 604 bytes. (Not the same build as the `pfs3aio`
in the `pfs3aio.lha` ART was tested against, which is 59 120 bytes; same
version, different binary.)

**And one finding that matters more than it looks.** MultibootOS's *second*
RDB carries a `DOS\3` driver at version 45.16 and **no PFS3** — while all
fifteen of its partitions are `PDS\3`. That card works. So the drivers a
partition can use are the **union across every RDB on the disk**, not the ones
in its own. ART's `partitionsMissingDriver` looks at one RDB, so on this card it
would report fifteen partitions as unmountable when none of them is. Filed as
[ART-097](ISSUES.md) together with the multiple-RDB question.

## 5. How MultibootOS actually multiboots

Two mechanisms, and only one of them was in the research:

- **Two separate Amiga areas**, each with its own RDB and its own partition set.
  Area 1 is a CaffeineOS-shaped pair (`SDH0` bootable pri 1, `SDH1`); area 2
  holds fifteen partitions in three families — `ADH0`/`ADH1` (bootable pri 2 and
  0), `AGS0`…`AGS10` (a game-selector library, `AGS0` bootable pri 0), and
  `AK0`/`AK1` (bootable pri 0).
- **Boot priority** decides which of the bootable ones wins: `ADH0` at 2 beats
  `SDH0` at 1 beats everything at 0.

The `config_<name>.txt` switching the research documented is the *third* layer,
on the FAT32 side. ART implements that one already.

## 6. The checklist an adapter has to satisfy

Derived from the above, in the order the work has to happen:

1. **Read an MBR** and find the `0x76` areas. Without this nothing else starts
   ([ART-095](ISSUES.md)).
2. **One RDB per area**, each with its own geometry read from the disk.
3. **Union the filesystem drivers** across areas before deciding a partition
   cannot mount ([ART-097](ISSUES.md)).
4. ✅ **Write `maxtransfer`, `mask` and `buffers`** as the field does
   ([ART-096](ISSUES.md), done 2026-08-13).
5. **Adapt, do not regenerate**: the FAT32 side is `config.txt` and
   `cmdline.txt`, which `core/pistorm` already merges rather than rewrites, plus
   the Kickstart, which ROM Manager already identifies and places.
6. **Leave the Amiga side alone** in the first slice. Both cards ship their
   volumes populated; adaptation is a FAT32-side job. Resizing or repartitioning
   somebody's 56 GiB PFS3 volume is a different feature with a different risk.

## 7. What is still not known

- **The FAT32 contents** of either card: `config.txt`, `cmdline.txt`, the
  Kickstart's name on the card, and what differs between the per-board variants.
  Reading them needs step 1 above, or a mounted card.
- **MultibootOS's own documentation** is a PDF whose text layer is largely
  rendered images; extracting it gave one usable line. The card itself is better
  evidence and is what the tables above come from.
- **Whether CaffeineOS ships per-board variants at all**, or one image adapted
  after writing. The 9318 preview folder listing suggests the latter.
