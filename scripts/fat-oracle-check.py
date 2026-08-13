#!/usr/bin/env python3
"""Cross-check ART's FAT32 boot partition against 7-Zip's reader.

The card's boot partition is the one filesystem ART writes that ART does not
read: the Raspberry Pi's firmware does. ART's own tests cannot catch a mistake
its writer and a reader of its own would share — that is what ART-032..035 were
on the Amiga side, four bugs that round-tripped through ART perfectly and were
rejected by every real tool. So the FAT32 writer gets the same treatment the
ADF writer gets: something that is not ART reads what ART wrote.

What is checked:

    the filesystem is FAT32          not FAT16, which a small partition would
                                     silently become and which the Pi will not
                                     boot from
    512-byte sectors, 4 KiB clusters the geometry both real cards carry
    the volume label                 what ART said it was
    every file is there              by name, long names and a folder included
    every file's bytes come back     compared, not counted
    no headers complaint             `fatfs` writes two things wrong in every
                                     directory it creates, and this is what
                                     found them — see `repair_directories`

Usage:

    python scripts/fat-oracle-check.py

Requires 7-Zip. It is looked for on PATH and then in the usual place, exactly
as `iso-oracle-check.py` does. Not in CI for the same reason that one is not:
CI has no 7-Zip. Run it when the FAT32 writer changes.
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Where the 300 MB image is built.
#
# **The user's own scratch folder**, not the system temp: the image is a third
# of a gigabyte and the system temp is on the C: drive, which is the one drive
# this project does not put trial output on. Falls back to a temporary
# directory anywhere the folder does not exist — on somebody else's machine it
# will not.
SCRATCH = Path(r"E:\amiga\ProjeART")

# What the export hook in `core/fat32.rs` writes. Kept in step with it by hand,
# which is safe because a mismatch fails loudly here rather than quietly.
EXPECTED = {
    "Emu68.img": b"kernel bytes, for the oracle",
    "config.txt": b"initramfs kick.rom\narm_64bit=1\n",
    "cmdline.txt": b"sd.unit0=ro\n",
    "config_caffeineos.txt": b"a long name, for the oracle\n",
    # A file in a folder, and the entry that earns this whole script its keep:
    # `fatfs` 0.3.6 writes long-filename entries for `.` and `..`, which the
    # format forbids, and 7-Zip reported `Headers Error` on every image with a
    # directory in it until `repair_directories` was written. Without a folder
    # here the oracle would have gone on passing while the card carried it.
    "overlays/emu68.dtbo": b"an overlay, in a folder",
}
EXPECTED_LABEL = "ART CARD"


def find_7z() -> str:
    for name in ("7z", "7za", "7zz"):
        found = shutil.which(name)
        if found:
            return found
    fallback = Path(os.environ.get("ProgramFiles", r"C:\Program Files")) / "7-Zip" / "7z.exe"
    if fallback.exists():
        return str(fallback)

    print("7-Zip not found. Install it and run this again:")
    print("  Windows:  winget install 7zip.7zip")
    print("  Debian:   apt install p7zip-full")
    print("  macOS:    brew install sevenzip")
    sys.exit(2)


def main() -> int:
    seven_zip = find_7z()

    parent = SCRATCH if SCRATCH.is_dir() else None
    with tempfile.TemporaryDirectory(prefix="art-fat-oracle-", dir=parent) as work:
        work_dir = Path(work)
        image = work_dir / "art-fat.img"

        # ART writes the partition through its own hook, so what is checked is
        # what the application would produce and not a copy of it.
        made = subprocess.run(
            ["cargo", "test", "export_fat32_for_oracle_when_asked"],
            cwd=ROOT / "src-tauri",
            env={**os.environ, "ART_FAT_OUT": str(image)},
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        if made.returncode != 0 or not image.exists():
            print("ART did not produce an image to check.")
            print(made.stdout[-2000:])
            print(made.stderr[-2000:])
            return 1

        listing = subprocess.run(
            [seven_zip, "l", str(image)],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        if listing.returncode != 0:
            print("7-Zip could not open what ART wrote as a filesystem at all.")
            print(listing.stdout[-2000:])
            return 1

        checks: list[tuple[bool, str]] = []

        def check(ok: bool, what: str) -> None:
            checks.append((ok, what))

        text = listing.stdout
        check(
            "Headers Error" not in text,
            "no reader complains about the headers (see `repair_directories`)",
        )
        check("File System = FAT32" in text, "it is FAT32, which is what the Pi boots from")
        check("Sector Size = 512" in text, "512-byte sectors")
        check("Cluster Size = 4096" in text, "4 KiB clusters, as on both real cards")
        label = re.search(r"^Label = (.*)$", text, re.M)
        check(
            bool(label) and label.group(1).strip() == EXPECTED_LABEL,
            f"the volume is labelled {EXPECTED_LABEL}",
        )

        out = work_dir / "out"
        extracted = subprocess.run(
            [seven_zip, "x", str(image), f"-o{out}", "-y"],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        check(extracted.returncode == 0, "7-Zip extracts it without complaint")

        for name, contents in EXPECTED.items():
            path = out / name
            if not path.exists():
                check(False, f"{name} is there")
                continue
            check(True, f"{name} is there")
            check(path.read_bytes() == contents, f"{name} comes back byte for byte")

    for ok, what in checks:
        print(f"  {'ok  ' if ok else 'FAIL'} {what}")

    failed = [what for ok, what in checks if not ok]
    if failed:
        print(f"\n{len(failed)} check(s) failed.")
        return 1

    print("\nART's boot partition and an independent implementation agree.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
