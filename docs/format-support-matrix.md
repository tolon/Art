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

### ZIP — archive
- `PK\x03\x04` at offset 0 (`PK\x05\x06` for an empty archive, `PK\x07\x08`
  for a spanned one). Detected by signature.
- Read-only in ART, deliberately: nothing here writes an archive.
- Deflate only. Encrypted entries, and the other compression methods, are
  refused by name rather than half-read.

### 7z — archive
- `7z BC AF 27 1C` at offset 0. Detected by signature.
- Read-only, LZMA. **Solid by default**, meaning entries share one compressed
  block — which is why the extraction gate reads a whole archive in one
  forward pass rather than entry by entry.

### ISO9660 — optical disc image
- `CD001` at sector 16. Where that lands says which sector layout the file has:
  `0x8001` for a plain 2048-byte image, `0x9311` for a raw 2352-byte Mode 1
  track, `0x9319` for Mode 2/XA Form 1 (an 8-byte subheader moves the data).
- Joliet (UCS-2 big-endian names) is preferred when the disc carries it; the
  Primary descriptor's uppercase 8.3 names are the fallback.
- Read-only. Mode 2 **Form 2** is refused rather than misread — it carries
  audio or video and no filesystem.

### Optical images — what is still not implemented

ISO9660 with Joliet is built and read-only (above). These are not:

- **Rock Ridge**, and the Amiga `AS` System Use entry that carries protection
  bits and file comments. A Unix-mastered Amiga CD with no Joliet descriptor
  falls back to uppercase 8.3 names today.
- `.cue`+`.bin`, `.nrg`, `.ccd`/`.img`/`.sub`, `.mdf`/`.mds` — container
  formats around the same filesystem.
- Writing a disc. Not planned; ART reads optical media.

### Commodore 64 disk and tape images — read-only

Built in `core/cbm` (2026-08-12). Read-only: ART opens these, copies files out
of them, and writes none of them. The shape of each is what the reader is
built on:

- **D64** — 1541 disk image, 256-byte blocks, no header: the file *is* the
  sectors. 35 tracks = 174,848 bytes (175,531 with error bytes); 40 tracks =
  196,608 (197,376). Sectors per track vary by zone, so a track/sector pair
  becomes an offset only through a table. Directory at track 18.
- **D71** — 1571, double-sided: 349,696 bytes (351,062 with error bytes).
- **D81** — 1581, 3.5″: 819,200 bytes, 80 tracks × 40 sectors.
- **T64** — a tape *archive* with a real header and directory, despite the
  name. Written by tools that get their own header wrong often enough that the
  records, not the counts, are what ART trusts: a `used` count of zero still
  lists the records, and an end address the file cannot support is clamped.
- **TAP** — **identify only, permanently.** A TAP holds the tape signal
  sampled as pulse widths: no directory, no file table. Listing one means
  demodulating the ROM tape format, and most commercial titles shipped their
  own turbo loader. ART says what it is and its length, and stops there
  (§10, §89).
- **PRG** — one program; the first two bytes are the load address. Identify
  only.
- **CRT** — cartridge image. Identify only.

Names are PETSCII, not ASCII, and `0xA0` is padding rather than a character —
though only at the end of a field: inside a name it *is* a character, and
stripping it would merge two files into one name. The graphics set renders as
`·`: each Unicode mapping would be a claim about a specific byte with nothing
to check it against, and a name drawn with the wrong symbols looks correct
while being wrong.

**Detection has nothing to go on but size** for D64/D71/D81 — these formats
have no header and no signature — so the accepted sizes are exact and few, and
anything else is refused with its size in the message. The header sector is
then read to *raise* confidence, never to gate.

## Detection confidence

**Detection is content-first** (`core/detect.rs`): signatures are checked
before the extension is looked at, so an `.img` holding a floppy is a floppy,
an `.adf` holding an ISO is an ISO, and an LHA renamed to `.dat` still opens.
The extension is the fallback for a file no signature matched — a hint, and
reported as the weaker evidence it is.

That was not always true, and the way it failed is worth keeping written down:
LHA's method field was matched at offset 0, where no LHA tool writes it, so the
format ART is built around was recognised by extension alone
([ART-076](ISSUES.md)).

Detection reports a `confidence` value (0.0–1.0):

| Signal | Confidence |
|--------|-----------|
| Magic signature match | ~0.9–0.95 |
| Known size + correct extension | ~0.7–0.9 |
| Extension only | ~0.4–0.5 |
| Unrecognized | 0.0 |

The Workflow Engine never promotes low-confidence detections into HIGH
compatibility claims.
