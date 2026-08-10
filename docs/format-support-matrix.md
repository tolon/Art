# Format Support Matrix

> Never claim support until it is implemented and tested.
> Never fake format support by simply renaming extensions.

This file documents **what each format is** and how ART detects it. For **what
is currently implemented**, see the format table in
[FEATURES.md](FEATURES.md#format-support) — that is the single source of truth
and the only one kept up to date.

The per-format notes below describe the formats themselves; where they mention a
phase, treat it as the original plan, not a status claim.

## Format details

### ADF — Amiga Disk File
- Standard AmigaDOS floppy image.
- DD disk: 901120 bytes (80 cylinders × 2 heads × 11 sectors × 512).
- HD disk: 1802240 bytes, 22 sectors per track (rare).
- Filesystems: OFS, FFS, with optional international and dircache flags.
- Detected by size plus `.adf` extension.

### ADZ — gzip-compressed ADF
- A plain ADF inside a gzip stream.
- Detected by extension only; decompression is required before anything else.

### DMS — DiskMasher
- Compressed floppy format with its own track-level container.
- Read-only conversion to ADF is the safe target; in-place editing is not.
- Detected by extension only.

### HDF — Hard Disk File
- May contain an RDB (Rigid Disk Block) with partition + filesystem info.
- Filesystems: OFS, FFS, PFS3, SFS.
- Detected by extension only. RDB presence is confirmed by parsing (`RDSK`).

### HDZ — compressed HDF
- Detected by extension only.

### LHA — archive
- Primary Amiga software distribution format.
- Magic signature: `-l` (level 1 header).
- May contain WHDLoad packages (`.slave`, executable, data, icons).
- Detected by signature (high confidence) or extension (low).

### ROM — Kickstart
- Common sizes: 256 KB, 512 KB, 1 MB.
- Detected by extension plus size; known sizes raise confidence.
- ART never distributes ROMs — it manages user-provided files (spec §32).

### Optical images — planned, not implemented

No code for any of the following exists yet. There is no `optical-image`
category, no ISO9660/Joliet/Rock Ridge reader, and no CD32/CDTV handling
anywhere in `core/`. This is Phase 3.2 of
[the roadmap](superpowers/specs/2026-08-09-art-roadmap-design.md):

- ISO9660 for AmigaOS install CDs, including Rock Ridge/Joliet and the Amiga
  `AS` System Use entry that carries protection bits and file comments.
- CD32 and CDTV discs.
- `.cue`+`.bin`, `.nrg`, `.ccd`/`.img`/`.sub`, `.mdf`/`.mds`; both 2048- and
  2352-byte sectors.
- Planned as read-only browsing and extraction first. Writing a bootable
  Amiga CD is a separate, later problem.

## Detection confidence

Detection today is entirely extension- and size-based (see `core/detect.rs`);
there is no content-first resolution yet. A generic container like `.img`,
`.dsk`, `.ima` or `.raw` is not resolved by looking at what is actually
inside it — that is planned as Phase 3.1 of the roadmap above, and until it
lands, an ambiguous extension is reported at low confidence rather than
guessed.

Detection reports a `confidence` value (0.0–1.0):

| Signal | Confidence |
|--------|-----------|
| Magic signature match | ~0.9–0.95 |
| Known size + correct extension | ~0.7–0.9 |
| Extension only | ~0.4–0.5 |
| Unrecognized | 0.0 |

The Workflow Engine never promotes low-confidence detections into HIGH
compatibility claims.
