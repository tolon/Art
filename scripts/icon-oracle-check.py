#!/usr/bin/env python3
"""Check `core/amigaicon`'s parser and splice against real `.info` files.

`core/amigaicon`'s own module doc comment admits what it was measured
against: **three** real icons, chosen because they exercised the two shapes
that mattered (landing exactly on end-of-file, and landing on the start of an
appended IFF `FORM`). That is real evidence, but it is evidence about three
files. ART's own test suite — synthetic fixtures built by hand in
`core/amigaicon`'s own `#[cfg(test)]` block — cannot tell that shape apart
from every other real icon Commodore and Hyperion actually shipped; a reader
that only ever sees icons ART itself built to satisfy it is exactly the shape
that let ART-032 … ART-035 (RDB fields on the Amiga side) and ART-104 (this
very module's own reason for existing — `IconEdit.info`'s doubled
`do_StackSize`) ship behind a green suite.

So this script runs `layout` and `merge_tooltypes(x, x)` against **every**
`.info` file inside a folder of real AmigaOS install media — the owner's own
licensed ADFs, not anything ART wrote for itself. Two things are checked per
file:

    layout(bytes) succeeds              the classic DiskObject header and
                                         every optional block after it parse
                                         without running past the buffer, and
                                         the parse lands exactly at
                                         end-of-file or the start of a
                                         trailing IFF block
    merge_tooltypes(bytes, bytes)       splicing an icon's own ToolTypes back
        == bytes                        into itself is a no-op — if it is not,
                                         the splice is wrong, not the input.
                                         Only asked of icons that actually
                                         carry a ToolTypes block: a real,
                                         common icon (a plain `Disk.info`, a
                                         drawer) carries none at all, and
                                         `merge_tooltypes` documents that it
                                         does not attempt to grow one — see
                                         `no_tooltypes` in the report below,
                                         never counted as a failure.

Extraction is done with `xdftool unpack`, amitools' own tool and unrelated to
anything `core/osinstall`'s `AdfSource` reads — an icon this script found by
using ART's own ADF reader to walk the disk would not be independent of
ART at all.

**No copyrighted Amiga content is committed, or even written where this
project's fixture rule would find it.** Every `.info` file this script
extracts goes into a `tempfile.TemporaryDirectory` under `ART_SCRATCH`,
deleted the moment the run finishes, success or failure. Nothing here writes
into the repository.

**Not in CI** — it needs the owner's own media, the same reason
`pfs3-oracle-check.py` and `fat-oracle-check.py` are not either. Run it by
hand when `core/amigaicon` changes, or when new install media arrives.

Usage:

    python scripts/icon-oracle-check.py "E:\\amiga\\Amigatolon\\paketler\\3.2\\AmigaOs 3.2\\ADF" [more folders...]

Environment:

    ART_SCRATCH   where the extracted icons are staged (default:
                  E:\\amiga\\ProjeART); must already exist — see
                  `require_scratch_dir`, copied from `pfs3-oracle-check.py`.
                  This project's standing rule is that C: and D: are never
                  used for scratch output, including by a silent fallback to
                  the system temp directory.
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CARGO_DIR = ROOT / "src-tauri"

DEFAULT_SCRATCH = Path(r"E:\amiga\ProjeART")

TEST_NAME = "round_trip_every_icon_in_a_folder_when_asked"


def require_scratch_dir() -> Path:
    """Same contract as `pfs3-oracle-check.py`'s own helper: `ART_SCRATCH`
    (or the project default) has to name a folder that already exists — no
    silent fallback to the system temp directory, which on this machine (and
    most Windows installs) sits on C:."""
    root = Path(os.environ.get("ART_SCRATCH") or DEFAULT_SCRATCH)
    if not root.is_dir():
        print(f"Scratch directory '{root}' does not exist.")
        print()
        print("This project never writes scratch output to C: or D:, including by")
        print("falling back to the system temp directory. Create the folder above,")
        print("or set ART_SCRATCH to one that already exists.")
        sys.exit(2)
    return root


def find_adfs(folders: list[Path]) -> list[Path]:
    """Every `.adf` under each given folder, case-insensitively, sorted for a
    reproducible run order. Recursive (`rglob`) so a folder holding
    sub-folders of media — not this task's real folder, but not assumed
    against either — is still fully covered."""
    found: list[Path] = []
    for folder in folders:
        if not folder.is_dir():
            print(f"'{folder}' is not a folder.")
            sys.exit(2)
        found.extend(p for p in folder.rglob("*") if p.is_file() and p.suffix.lower() == ".adf")
    return sorted(set(found))


def unpack_adf(adf: Path, dest: Path) -> tuple[bool, str]:
    """`xdftool <adf> unpack <dest>` — amitools' own extractor, not ART's.

    Each ADF gets its own destination folder (named for the ADF) rather than
    a shared flat one: two different disks are free to both carry a
    `Disk.info` at their root, and flattening them would let one silently
    overwrite the other, undercounting what this script actually checks.
    """
    dest.mkdir(parents=True, exist_ok=True)
    result = subprocess.run(
        ["xdftool", str(adf), "unpack", str(dest)],
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    ok = result.returncode == 0
    return ok, result.stdout + result.stderr


def run_cargo_test(icon_dir: Path) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["cargo", "test", "--quiet", TEST_NAME, "--", "--nocapture", "--ignored"],
        cwd=CARGO_DIR,
        env={**os.environ, "ART_ICON_DIR": str(icon_dir)},
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    folders = [Path(arg) for arg in sys.argv[1:]]
    scratch = require_scratch_dir()

    adfs = find_adfs(folders)
    if not adfs:
        print("No .adf files found under:")
        for folder in folders:
            print(f"  {folder}")
        return 2
    print(f"{len(adfs)} ADF(s) found under {len(folders)} folder(s).")

    with tempfile.TemporaryDirectory(prefix="art-icon-oracle-", dir=scratch) as tmp:
        work = Path(tmp)

        extract_failures: list[str] = []
        for adf in adfs:
            dest = work / adf.stem
            ok, output = unpack_adf(adf, dest)
            if not ok:
                extract_failures.append(str(adf))
                print(f"  FAIL extracting {adf}")
                print(output[-1500:])
            else:
                print(f"  ok   extracted {adf}")

        if extract_failures:
            print(f"\n{len(extract_failures)} ADF(s) failed to extract; aborting.")
            for adf in extract_failures:
                print(f"  - {adf}")
            return 1

        print(f"\nRunning `cargo test {TEST_NAME}` against {work} ...")
        result = run_cargo_test(work)

        checked = None
        failed_count = None
        no_tooltypes = None
        fail_lines: list[str] = []
        for line in result.stdout.splitlines():
            if line.startswith("ART_ICON_RESULT "):
                parts = dict(
                    item.split("=", 1) for item in line[len("ART_ICON_RESULT ") :].split()
                )
                checked = int(parts.get("checked", "0"))
                failed_count = int(parts.get("failed", "0"))
                no_tooltypes = int(parts.get("no_tooltypes", "0"))
            elif line.startswith("ART_ICON_FAIL "):
                fail_lines.append(line[len("ART_ICON_FAIL ") :])

        if checked is None:
            print("\nThe test did not print an ART_ICON_RESULT line — something else")
            print("went wrong before it could even run. Full output:")
            print(result.stdout[-4000:])
            print(result.stderr[-2000:])
            return 1

        print(f"\nchecked={checked} failed={failed_count} no_tooltypes={no_tooltypes}")
        if no_tooltypes:
            print(
                f"({no_tooltypes} of them carry no ToolTypes block at all — a real, "
                "common icon shape merge_tooltypes does not attempt to grow a new "
                "block for (see core/amigaicon's own doc comment). Not a failure.)"
            )
        if fail_lines:
            print(f"\n{len(fail_lines)} icon(s) did not round-trip:")
            for path in fail_lines:
                print(f"  - {path}")

        if failed_count:
            print(
                "\ncore/amigaicon disagrees with real material — that means the "
                "parser or the splice is wrong, not the icons: they were written "
                "by Commodore and Hyperion, not by ART."
            )
            return 1

    round_tripped = checked - (no_tooltypes or 0)
    print(
        f"\nAll {checked} real .info file(s) parsed; {round_tripped} of them round-tripped "
        f"through merge_tooltypes ({no_tooltypes or 0} carry no ToolTypes block)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
