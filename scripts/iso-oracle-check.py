#!/usr/bin/env python3
"""Check ART's disc reader against 7-Zip, an implementation sharing no code.

ART's ISO9660 reader and the synthetic ISO builder its tests run on were
written from the same offsets. That means they can agree with each other and
both be wrong, and every test would still pass — which is exactly how
ART-032 … ART-035 shipped behind a green suite. `cargo test` cannot close that
gap; only an outside implementation can.

The original plan's outside implementation was a real AmigaOS CD in front of a
real Amiga. That was cancelled (plan amendment A1, 2026-08-11): it assumed
licensed media reliably to hand. 7-Zip reads ISO9660 and Joliet with its own
implementation, in user space, on any machine — so it is the maintained rung
here.

    ART writes a disc  →  7-Zip lists and extracts it  →  the two must agree

What it checks, per fixture:

  * every name 7-Zip sees, ART sees, and nothing extra on either side
  * every file's size
  * every file's bytes, by SHA-256 of what each side produced

Raw (2352-byte sector) images are checked too, and that matters more than the
cooked ones: no host mounts a track dump and 7-Zip will not open one either,
so without this the raw path rests entirely on ART agreeing with itself
(ART-075). This script strips such an image down to 2048-byte sectors
**itself**, from the layout's documented offsets — 12 bytes of sync, a 4-byte
header, then user data for Mode 1 — and hands 7-Zip the result. Nothing in
that conversion comes from ART's code, or it would not be independent.

Only well-formed fixtures are ever handed to 7-Zip. The malformed and hostile
discs (records that loop, lengths past the end of the file, depth bombs) stay
inside `cargo test` against ART's own reader: handing those to an outside tool
proves nothing about ART.

Usage:

    python scripts/iso-oracle-check.py

Requires 7-Zip. It is looked for on `PATH` as `7z`/`7za`, then in the usual
Windows install location. If it is not found this script **fails** — an oracle
that quietly skips itself is not an oracle, it is a green tick nobody earned.

Runs outside `cargo test`, like `scripts/oracle-check.py`.
"""

from __future__ import annotations

import hashlib
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CARGO_DIR = REPO / "src-tauri"

#: A raw CD sector is 2352 bytes: 12 of sync, a 4-byte header, then user data.
#: Mode 1 puts 2048 bytes of data at offset 16. (Mode 2/XA Form 1 adds an
#: 8-byte subheader, so its data starts at 24 — that layout arrives with
#: Task 3b and gets its own fixture here.)
RAW_SECTOR_SIZE = 2352
COOKED_SECTOR_SIZE = 2048
MODE1_DATA_OFFSET = 16

failures: list[str] = []


def fail(label: str, detail: str = "") -> None:
    print(f"  FAIL {label}")
    for line in detail.splitlines()[:10]:
        print(f"       {line}")
    failures.append(label)


def ok(label: str) -> None:
    print(f"  ok   {label}")


def find_7z() -> str:
    """The oracle itself. Absent means this script fails, never skips."""
    for name in ("7z", "7za", "7zz"):
        found = shutil.which(name)
        if found:
            return found

    for candidate in (
        Path(os.environ.get("ProgramFiles", r"C:\Program Files")) / "7-Zip" / "7z.exe",
        Path(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)"))
        / "7-Zip"
        / "7z.exe",
    ):
        if candidate.exists():
            return str(candidate)

    print("7-Zip was not found, so this oracle cannot run.")
    print()
    print("It is the only check that can catch ART's ISO reader and its own")
    print("fixtures being wrong in the same way, so it does not skip.")
    print()
    print("  Windows:  winget install 7zip.7zip")
    print("  Debian:   apt install p7zip-full")
    print("  macOS:    brew install sevenzip")
    print()
    print("Looked for: 7z, 7za, 7zz on PATH; %ProgramFiles%\\7-Zip\\7z.exe")
    sys.exit(2)


def cargo(test: str, env: dict[str, str]) -> str:
    """Run one of ART's oracle hooks and return what it printed.

    The hooks are `#[test]`s that do nothing unless their environment variable
    is set, so this depends on ART only through those few names.
    """
    result = subprocess.run(
        ["cargo", "test", "--quiet", test, "--", "--nocapture"],
        cwd=CARGO_DIR,
        env={**os.environ, **env},
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"cargo test {test} exited {result.returncode}\n{result.stdout}\n{result.stderr}"
        )
    return result.stdout + result.stderr


def art_listing(image: Path) -> tuple[dict[str, tuple[int, str]], set[str], str]:
    """What ART makes of a disc: files → (size, sha256), directories, volume."""
    printed = cargo("read_iso_for_oracle_when_asked", {"ART_ISO_READ_IN": str(image)})

    files: dict[str, tuple[int, str]] = {}
    directories: set[str] = set()
    volume = ""
    for line in printed.splitlines():
        if line.startswith("volume="):
            volume = line[len("volume=") :]
        elif line.startswith("dir="):
            directories.add(line[len("dir=") :])
        elif line.startswith("file="):
            path, size, digest = line[len("file=") :].split("|")
            files[path] = (int(size), digest)
    if not volume and not files:
        raise RuntimeError(f"ART reported nothing for {image}:\n{printed}")
    return files, directories, volume


def sevenzip_listing(
    seven: str, image: Path
) -> tuple[dict[str, int], set[str]]:
    """What 7-Zip makes of the same disc: files → size, and directories."""
    result = subprocess.run(
        [seven, "l", "-slt", "-sccUTF-8", str(image)],
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    if result.returncode != 0:
        raise RuntimeError(f"7z could not list {image}:\n{result.stdout}{result.stderr}")

    files: dict[str, int] = {}
    directories: set[str] = set()

    # `-slt` prints a blank-line-separated record per entry, after a line of
    # dashes. The archive's own header uses the same `Path =` key, which is
    # why the split matters rather than scanning for `Path =` throughout.
    _, _, body = result.stdout.partition("\n----------\n")
    for record in body.split("\n\n"):
        fields = {}
        for line in record.splitlines():
            key, sep, value = line.partition(" = ")
            if sep:
                fields[key.strip()] = value.strip()
        path = fields.get("Path")
        if not path:
            continue
        path = path.replace("\\", "/")
        if fields.get("Folder") == "+":
            directories.add(path)
        else:
            files[path] = int(fields.get("Size") or 0)
    return files, directories


def sevenzip_bytes(seven: str, image: Path, path: str) -> bytes:
    """One file's contents, as 7-Zip decodes them."""
    result = subprocess.run(
        [seven, "e", "-so", str(image), path.replace("/", "\\")],
        capture_output=True,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"7z could not extract {path} from {image}:\n"
            f"{result.stderr.decode('utf-8', 'replace')}"
        )
    return result.stdout


def strip_raw_sectors(source: Path, dest: Path, data_offset: int) -> None:
    """Turn a raw 2352-byte-sector image into a plain 2048-byte-sector one.

    Fifteen lines that make the raw path checkable at all. The offsets come
    from the sector layout as documented — 12 bytes of sync, a 4-byte header,
    and for Mode 2/XA an 8-byte subheader — and deliberately **not** from
    `core::iso`: a stripper that asked ART where the data was would inherit
    whatever ART is wrong about, which is the entire failure this oracle
    exists to catch.
    """
    raw = source.read_bytes()
    if len(raw) % RAW_SECTOR_SIZE != 0:
        raise RuntimeError(
            f"{source} is {len(raw)} bytes, not a whole number of 2352-byte sectors"
        )
    out = bytearray()
    for start in range(0, len(raw), RAW_SECTOR_SIZE):
        sector = raw[start : start + RAW_SECTOR_SIZE]
        out += sector[data_offset : data_offset + COOKED_SECTOR_SIZE]
    dest.write_bytes(bytes(out))


def check(seven: str, label: str, image: Path, stripped: Path | None) -> None:
    """One fixture, both ways: names, sizes, then bytes."""
    print(f"{label}:")

    # ART reads the raw image itself; 7-Zip only ever sees the stripped copy.
    for_sevenzip = stripped if stripped is not None else image

    # A side that cannot read the disc at all is a disagreement like any
    # other, and is reported as one. Letting it raise would end the run with a
    # traceback and leave the remaining fixtures unchecked, which reads as
    # "the script is broken" rather than "these two do not agree".
    try:
        art_files, art_dirs, volume = art_listing(image)
    except RuntimeError as err:
        fail(f"ART reads {image.name}", str(err))
        return
    try:
        seven_files, seven_dirs = sevenzip_listing(seven, for_sevenzip)
    except RuntimeError as err:
        fail(f"7-Zip reads {for_sevenzip.name}", str(err))
        return

    if not seven_files:
        fail(f"7-Zip found nothing in {for_sevenzip.name}")
        return

    missing = sorted(set(seven_files) - set(art_files))
    extra = sorted(set(art_files) - set(seven_files))
    if missing:
        fail("ART sees every file 7-Zip sees", f"missing: {missing}")
    elif extra:
        fail("ART sees no file 7-Zip does not", f"extra: {extra}")
    else:
        ok(f"the same {len(art_files)} files, by name")

    if art_dirs != seven_dirs:
        fail(
            "the same directories",
            f"ART: {sorted(art_dirs)}\n7-Zip: {sorted(seven_dirs)}",
        )
    else:
        ok(f"the same {len(art_dirs)} directories")

    sizes_agree = True
    for path, size in sorted(seven_files.items()):
        if path in art_files and art_files[path][0] != size:
            fail(f"{path} is the same size", f"ART {art_files[path][0]}, 7-Zip {size}")
            sizes_agree = False
    if sizes_agree:
        ok("every file's size agrees")

    for path in sorted(seven_files):
        if path not in art_files:
            continue
        try:
            theirs = hashlib.sha256(sevenzip_bytes(seven, for_sevenzip, path)).hexdigest()
        except RuntimeError as err:
            fail(f"7-Zip extracts {path}", str(err))
            continue
        if theirs != art_files[path][1]:
            fail(f"{path} reads back byte for byte", f"ART {art_files[path][1]}\n7-Zip {theirs}")
        else:
            ok(f"{path} reads back byte for byte")

    if volume:
        ok(f"volume name: {volume}")
    print()


def main() -> int:
    seven = find_7z()
    print(f"Oracle: {seven}")
    print()

    with tempfile.TemporaryDirectory(prefix="art-iso-oracle-") as tmp:
        work = Path(tmp)
        joliet = work / "joliet.iso"
        plain = work / "plain.iso"
        raw = work / "raw.iso"

        # One invocation writes them all: the fixtures come from ART's own
        # builder, so the script never reimplements the format it is checking.
        cargo(
            "export_iso_for_oracle_when_asked",
            {
                "ART_ISO_OUT": str(joliet),
                "ART_ISO_PLAIN_OUT": str(plain),
                "ART_ISO_RAW_OUT": str(raw),
            },
        )
        for built in (joliet, plain, raw):
            if not built.exists():
                print(f"  FAIL ART did not write {built.name}")
                failures.append(f"missing fixture {built.name}")
        if failures:
            return 1

        raw_stripped = work / "raw-stripped.iso"
        strip_raw_sectors(raw, raw_stripped, MODE1_DATA_OFFSET)

        check(seven, "A Joliet disc, 2048-byte sectors", joliet, None)
        check(seven, "An ISO9660-only disc, 2048-byte sectors", plain, None)
        check(
            seven,
            "A raw 2352-byte track dump (Mode 1), stripped to 2048 by this script",
            raw,
            raw_stripped,
        )

    if failures:
        print(f"{len(failures)} disagreement(s) between ART and 7-Zip:")
        for item in failures:
            print(f"  - {item}")
        return 1

    print("ART and 7-Zip agree on every disc.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
