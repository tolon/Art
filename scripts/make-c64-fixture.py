#!/usr/bin/env python3
"""Write a small D64 disk image, without using any of ART's own code.

Two jobs, and the second is the interesting one:

1. Give a human something to open in the Files screen, so the C64 pane can be
   looked at rather than only tested.
2. Be an **independent writer**. ART's own D64 tests build their fixtures with
   ART's builder, from the same table its reader uses — so they can agree with
   each other and both be wrong, which is the failure mode that produced
   ART-032 … ART-035 and ART-075. This script is written from the published
   1541 layout instead, so a disk it writes and ART reads is a small check
   across an implementation boundary.

It is not a full oracle: it writes, and nothing here reads back. A real one
would be `c1541` (VICE) or DirMaster listing a disk ART wrote, which needs a
tool this project does not ship.

    python scripts/make-c64-fixture.py test/sample.d64
"""

from __future__ import annotations

import sys
from pathlib import Path

SECTOR = 256
PAD = 0xA0

#: Sectors per track, by zone — the 1541's four speed zones.
def sectors_on(track: int) -> int:
    if 1 <= track <= 17:
        return 21
    if 18 <= track <= 24:
        return 19
    if 25 <= track <= 30:
        return 18
    return 17


def offset_of(track: int, sector: int) -> int:
    before = sum(sectors_on(t) for t in range(1, track))
    return (before + sector) * SECTOR


def petscii(text: str, width: int) -> bytes:
    """Upper-case PETSCII, padded with 0xA0 the way a drive pads it."""
    encoded = bytes(
        (ord(c) if 0x20 <= ord(c) <= 0x5F else ord("_")) for c in text.upper()
    )[:width]
    return encoded + bytes([PAD] * (width - len(encoded)))


def build(name: str, disk_id: str, files: list[tuple[str, bytes]]) -> bytearray:
    total = sum(sectors_on(t) for t in range(1, 36))
    data = bytearray(total * SECTOR)

    # --- BAM, track 18 sector 0 -------------------------------------------
    bam = offset_of(18, 0)
    data[bam + 0] = 18  # first directory sector
    data[bam + 1] = 1
    data[bam + 2] = ord("A")  # DOS version
    data[bam + 0x90 : bam + 0xA0] = petscii(name, 16)
    data[bam + 0xA0] = PAD
    data[bam + 0xA1] = PAD
    data[bam + 0xA2 : bam + 0xA4] = petscii(disk_id, 2)
    data[bam + 0xA4] = PAD
    data[bam + 0xA5 : bam + 0xA7] = b"2A"

    # --- the files, one per track-17 sector, and their directory entries ---
    directory = offset_of(18, 1)
    for slot, (filename, contents) in enumerate(files):
        if slot >= 8:
            raise SystemExit("this fixture writes one directory sector: 8 files")

        chunks = [contents[i : i + 254] for i in range(0, len(contents), 254)] or [b""]
        first = (17, slot * 3)
        for i, chunk in enumerate(chunks):
            track, sector = 17, slot * 3 + i
            at = offset_of(track, sector)
            if i + 1 < len(chunks):
                data[at] = 17
                data[at + 1] = slot * 3 + i + 1
            else:
                data[at] = 0
                data[at + 1] = len(chunk) + 1
            data[at + 2 : at + 2 + len(chunk)] = chunk

        entry = directory + slot * 32
        data[entry + 2] = 0x82  # closed PRG
        data[entry + 3] = first[0]
        data[entry + 4] = first[1]
        data[entry + 5 : entry + 21] = petscii(filename, 16)
        blocks = len(chunks)
        data[entry + 30] = blocks & 0xFF
        data[entry + 31] = blocks >> 8

    return data


def main() -> int:
    target = Path(sys.argv[1] if len(sys.argv) > 1 else "test/sample.d64")
    target.parent.mkdir(parents=True, exist_ok=True)

    image = build(
        "ART TEST DISK",
        "01",
        [
            ("LOADER", b"\x01\x08" + b"loader program bytes\n" * 4),
            ("LEVEL 1", bytes(range(256)) * 2),
            ("README", b"Written by make-c64-fixture.py, not by ART.\n"),
        ],
    )
    target.write_bytes(image)
    print(f"wrote {target} ({len(image)} bytes, 35 tracks)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
