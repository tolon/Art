#!/usr/bin/env python3
"""Check `core/amigaicon`'s parser and its writers against real `.info` files.

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
`do_StackSize`) ship behind a green suite. The same trap applies twice over
to a *writer*: a reader and a writer built by the same person, checked only
against each other, can share a mistake and agree with each other perfectly.

So this script runs `layout`, `merge_tooltypes(x, x)` and, since the
drawer-icons round (Task 7), every writer `core/amigaicon` grew —
`set_tooltypes`, `set_position`, `set_show_all_files` and
`render::rendered_size` — against **every** `.info` file inside a folder of
real AmigaOS install media — the owner's own licensed ADFs and already-
unpacked distribution trees, not anything ART wrote for itself. Checked per
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
    set_tooltypes(bytes,                the sharpest check in the set: it
        tooltypes(bytes)) == bytes      exercises the whole splice path —
                                         including growing or clearing a
                                         block, which merge_tooltypes never
                                         does — and asks it of every icon,
                                         with or without an existing
                                         ToolTypes block.
    set_position(Some((37, 11)))        reads back as (37, 11); bytes 58..66
                                         differ and nothing else does.
    set_position(None)                  reads back as unplaced — both
                                         coordinates carry the NO_POSITION
                                         sentinel (i32::MIN), never a plain 0.
    set_show_all_files(true) then       touches nothing outside the computed
        (false)                         DrawerData2 range, and settles
                                         dd_Flags at exactly the documented
                                         default — never "back to the
                                         original bytes", which is
                                         core/amigaicon's own documented
                                         limitation (a boolean cannot ask to
                                         restore DDFLAGS_SHOWICONS), not a
                                         residue of anything this script
                                         found wrong. This script's first
                                         real run *did* find something wrong
                                         here — the offset itself, fixed since
                                         (see core/amigaicon's own doc
                                         comment on DRAWER_DATA2_LEN) — which
                                         is exactly the class of defect this
                                         oracle exists to catch: the old
                                         reader and writer agreed with each
                                         other perfectly and still both read
                                         the wrong field. Only asked of icons
                                         that actually carry a DrawerData2
                                         extension; a DrawerData block with
                                         none is counted in `no_drawer_data2`
                                         (and still checked to refuse by
                                         name, never a silent wrong write); a
                                         plain Tool or Project icon with no
                                         DrawerData block at all is counted
                                         in `no_drawer_data`. Neither is a
                                         failure.
    render::rendered_size(bytes)        never smaller than the Gadget
                                         width/height, and never zero in
                                         either dimension.

Extraction is done with `xdftool unpack`, amitools' own tool and unrelated to
anything `core/osinstall`'s `AdfSource` reads — an icon this script found by
using ART's own ADF reader to walk the disk would not be independent of
ART at all. A folder that already holds unpacked `.info` files directly —
media unpacked by hand, or an OS Builder distribution tree copied byte-for-
byte from real media — is staged the same way, without needing an ADF or
xdftool at all: see `find_direct_info_files`.

**No copyrighted Amiga content is committed, or even written where this
project's fixture rule would find it.** Every `.info` file this script
extracts or stages goes into a `tempfile.TemporaryDirectory` under
`ART_SCRATCH`, deleted the moment the run finishes, success or failure.
Nothing here writes into the repository.

**Not in CI** — it needs the owner's own media, the same reason
`pfs3-oracle-check.py` and `fat-oracle-check.py` are not either. Run it by
hand when `core/amigaicon` changes, or when new install media arrives.

Usage:

    python scripts/icon-oracle-check.py "E:\\amiga\\Amigatolon\\paketler\\3.2\\AmigaOs 3.2\\ADF" [more folders...]
    python scripts/icon-oracle-check.py "E:\\amiga\\Amigatolon\\os39"

The second form points straight at a folder of already-unpacked media (no
`.adf` files at all) — this script stages its `.info` files exactly as if
they had just been extracted; the corpus is the union of whatever `.adf`
files and whatever already-unpacked `.info` files it finds under the given
folder(s). Given neither, it refuses to report anything rather than print a
false "0 failures" over zero icons checked (see `main`'s exit code below).

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
import shutil
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


def find_direct_info_files(folders: list[Path]) -> list[Path]:
    """Every `.info` file that already sits on disk under one of the given
    folders — real material that never needed extracting from an ADF at all:
    media somebody unpacked by hand, or an OS Builder distribution tree
    copied byte-for-byte from real install media (`core/osinstall` never
    rewrites a file's bytes — see CLAUDE.md's "a component is a set of
    paths" section — so a `.info` sitting in one of those trees is exactly as
    real as one still living inside its original `.adf`).

    Recursive and case-insensitive, same as `find_adfs`. Deliberately
    separate from `find_adfs`'s own `.rglob` so an ADF's own unpacked
    contents (staged later, under the scratch directory, not under any of
    `folders`) are never double-counted here.
    """
    found: list[Path] = []
    for folder in folders:
        found.extend(p for p in folder.rglob("*") if p.is_file() and p.suffix.lower() == ".info")
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


def stage_direct_info_file(info: Path, dest_root: Path) -> Path:
    """Copy one already-unpacked `.info` file under `dest_root`, preserving
    its full path (minus the drive letter) as the destination's directory
    structure.

    This is the direct-file equivalent of `unpack_adf`'s "each ADF gets its
    own destination folder": two different source trees are free to both
    carry `C/Foo.info` at the same relative position (`os39/art1/C/Foo.info`
    and `os39/art2/C/Foo.info` really do, in the owner's own material), and
    mirroring the *absolute* path rather than a relative one guarantees they
    land at different destinations without this script needing to know which
    of possibly several `folders` arguments a given file came from.
    """
    parts = info.parts
    relative = Path(*parts[1:]) if info.drive else info.relative_to(info.anchor)
    dest = dest_root / relative
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(info, dest)
    return dest


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
    direct_infos = find_direct_info_files(folders)

    if not adfs and not direct_infos:
        # Refuse to report anything rather than print a confident "0
        # failures" over a corpus of zero icons — the CLAUDE.md failure this
        # project pays most for is a screen that looks like success and
        # carries no information.
        print("No .adf files and no already-unpacked .info files found under:")
        for folder in folders:
            print(f"  {folder}")
        print()
        print("Nothing was checked. This is not the same sentence as success.")
        return 2

    print(
        f"{len(adfs)} ADF(s) and {len(direct_infos)} already-unpacked .info file(s) "
        f"found under {len(folders)} folder(s)."
    )

    with tempfile.TemporaryDirectory(prefix="art-icon-oracle-", dir=scratch) as tmp:
        work = Path(tmp)

        stage_failures: list[str] = []
        for adf in adfs:
            dest = work / adf.stem
            ok, output = unpack_adf(adf, dest)
            if not ok:
                stage_failures.append(str(adf))
                print(f"  FAIL extracting {adf}")
                print(output[-1500:])
            else:
                print(f"  ok   extracted {adf}")

        direct_dest_root = work / "direct"
        staged = 0
        for info in direct_infos:
            try:
                stage_direct_info_file(info, direct_dest_root)
                staged += 1
            except OSError as err:
                stage_failures.append(str(info))
                print(f"  FAIL staging {info}: {err}")
        if staged:
            print(f"  ok   staged {staged} already-unpacked .info file(s)")

        if stage_failures:
            print(f"\n{len(stage_failures)} file(s) failed to extract or stage; aborting.")
            for path in stage_failures:
                print(f"  - {path}")
            return 1

        print(f"\nRunning `cargo test {TEST_NAME}` against {work} ...")
        result = run_cargo_test(work)

        checked = None
        failed_count = None
        no_tooltypes = None
        no_drawer_data = None
        no_drawer_data2 = None
        lossy_tooltypes = None
        fail_lines: list[str] = []
        for line in result.stdout.splitlines():
            if line.startswith("ART_ICON_RESULT "):
                parts = dict(
                    item.split("=", 1) for item in line[len("ART_ICON_RESULT ") :].split()
                )
                checked = int(parts.get("checked", "0"))
                failed_count = int(parts.get("failed", "0"))
                no_tooltypes = int(parts.get("no_tooltypes", "0"))
                no_drawer_data = int(parts.get("no_drawer_data", "0"))
                no_drawer_data2 = int(parts.get("no_drawer_data2", "0"))
                lossy_tooltypes = int(parts.get("lossy_tooltypes", "0"))
            elif line.startswith("ART_ICON_FAIL "):
                fail_lines.append(line[len("ART_ICON_FAIL ") :])

        if checked is None:
            print("\nThe test did not print an ART_ICON_RESULT line — something else")
            print("went wrong before it could even run. Full output:")
            print(result.stdout[-4000:])
            print(result.stderr[-2000:])
            return 1

        print(
            f"\nchecked={checked} failed={failed_count} no_tooltypes={no_tooltypes} "
            f"no_drawer_data={no_drawer_data} no_drawer_data2={no_drawer_data2} "
            f"lossy_tooltypes={lossy_tooltypes}"
        )
        if no_tooltypes:
            print(
                f"({no_tooltypes} of them carry no ToolTypes block at all — a real, "
                "common icon shape merge_tooltypes does not attempt to grow a new "
                "block for (see core/amigaicon's own doc comment). Not a failure — "
                "set_tooltypes(existing) is still checked on these.)"
            )
        if no_drawer_data:
            print(
                f"({no_drawer_data} of them carry no DrawerData block at all — a "
                "plain Tool or Project icon, say — so set_show_all_files has "
                "nothing to toggle. Not a failure.)"
            )
        if no_drawer_data2:
            print(
                f"({no_drawer_data2} of them carry a DrawerData block but no "
                "DrawerData2 extension — an editor old enough to predate it, or "
                "appended artwork leaving no room for one — so set_show_all_files "
                "has nothing to toggle either. Not a failure, but still checked: "
                "set_show_all_files must refuse these by name, not silently write "
                "into bytes that are not DrawerData2 at all.)"
            )
        if lossy_tooltypes:
            print(
                f"({lossy_tooltypes} of them carry a tool type (a NewIcon IM1=/IM2= "
                "pixel encoding, typically) whose raw bytes are not valid UTF-8, so "
                "tooltypes()'s lossy decode cannot be re-encoded byte-identically — "
                "documented in core/amigaicon's own doc comment. Not a failure: the "
                "tool-type *text* is still checked to round-trip exactly, only the "
                "underlying bytes cannot be.)"
            )
        if fail_lines:
            print(f"\n{len(fail_lines)} icon(s) did not round-trip:")
            for path in fail_lines:
                print(f"  - {path}")

        if failed_count:
            print(
                "\ncore/amigaicon disagrees with real material — that means the "
                "parser or a writer is wrong, not the icons: they were written "
                "by Commodore and Hyperion, not by ART."
            )
            return 1

    round_tripped = checked - (no_tooltypes or 0)
    print(
        f"\nAll {checked} real .info file(s) parsed and round-tripped through every "
        f"writer checked; {round_tripped} of them also round-tripped through "
        f"merge_tooltypes ({no_tooltypes or 0} carry no ToolTypes block, "
        f"{no_drawer_data or 0} carry no DrawerData block)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
