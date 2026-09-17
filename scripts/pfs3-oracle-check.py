#!/usr/bin/env python3
"""Check ART's PFS3 writer and reader against `hst-imager`, both directions.

`libpfs3` is both the crate `core/preload/native.rs` uses to *write* PFS3 and
the crate `core/osinstall/verify.rs` uses to *read* it back — so ART's own
test suite cannot catch a mistake the two would share. A reader and a writer
that agree only with each other and with nothing else is exactly the shape of
four defects that already shipped behind a green suite: ART-032 … ART-035 (RDB
fields ART wrote and read consistently and wrongly), ART-075 (Mode 2/XA), and
ART-079, where a 7z reader handed one entry another entry's bytes while every
fixture ART built for itself passed.

So this script checks ART against `hst-imager` — a C# implementation sharing
no code with ART — in both directions:

    ART writes         →  hst-imager reads     proves ART's PFS3 writer
    hst-imager writes  →  ART reads             proves ART's PFS3 reader

Both halves are needed for the same reason `oracle-check.py` needs both:
ART's own reader and writer agreeing is not evidence of anything.

**Protection bits are part of the comparison, not an extra.** AmigaOS 3.2's
`Startup-Sequence` runs `Resident C:Assign PURE`, and the Pure bit arriving
correctly is the reason this whole phase exists — so both directions check
the eight-letter `HSPARWED` attribute string, not just names and sizes.

**Both directions hash file contents — SHA-256, not a length.** ART-079 gave
every file exactly the right length and another file's bytes, so a length
comparison alone would not have caught the bug this project already shipped
once. In the `ART writes` direction, `hst-imager fs copy` extracts the volume
back out to a directory on the PC and every file's extracted bytes are hashed
against the literal ART's write hook was given — not against anything ART
read back through its own reader, which would prove nothing new. In the
`hst-imager writes` direction, ART's read hook hashes what it reads through
`libpfs3` against the bytes this script handed `hst-imager`.

**This is a local oracle, not a CI one** — exactly like `fat-oracle-check.py`
and `iso-oracle-check.py`: the CI runner has no `hst.imager.exe`. Run it by
hand when the PFS3 writer or reader changes.

Usage:

    python scripts/pfs3-oracle-check.py
    python scripts/pfs3-oracle-check.py --card IMAGE ART_JSON

`--card` (card round 3) checks a whole card ART built: IMAGE and its listing
ART_JSON come from `writes_a_small_card_for_the_hst_imager_oracle`
(`ART_CARD_OS_OUT`, and `ART_HST` for the run whose Stuff hst-imager writes).
Each partition is listed (`fs dir -r <image>\\mbr\\<slot>\\rdb\\<drive>`) and
extracted (`fs copy`) by hst-imager; the path sets, sizes and SHA-256 per file
must equal ART's. Exit 0 all equal, 1 a difference, 2 a tool or file missing.

Environment:

    ART_HST_IMAGER   path to hst.imager.exe (default: the usual place below)
    ART_PFS3_DRIVER  path to a pfs3aio driver binary, for hst-imager's own
                     `format Rdb PDS3` (default: the usual place below)
    ART_SCRATCH      where the images are built (default: E:\\amiga\\ProjeART);
                     must already exist — see `require_scratch_dir`

If either tool is missing this **skips cleanly** (exit 2) rather than crash
or silently report nothing checked — the same contract `fat-oracle-check.py`
gives 7-Zip.

`hst-imager` resolves `<image>\\rdb\\dh0` relative to its own working
directory — an absolute path with forward slashes was rejected in testing —
so every invocation below runs with `cwd` set to the image's folder and
addresses the image by its bare filename.
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CARGO_DIR = ROOT / "src-tauri"

DEFAULT_SCRATCH = Path(r"E:\amiga\ProjeART")
DEFAULT_HST_IMAGER = Path(r"E:\amiga\Amigatolon\hstimager\hst.imager.exe")
DEFAULT_PFS3_DRIVER = Path(r"E:\amiga\ProjeART\pfs3-spike\pfs3aio\pfs3aio")


def require_scratch_dir() -> Path:
    """The scratch directory a 220 MB image is built in — required to already
    exist, with **no silent fallback**.

    This project's own standing rule is that C: and D: are never used for
    scratch output. `tempfile.TemporaryDirectory(dir=None)` falls back to the
    system temp directory, which on this machine — and on most Windows
    installs — sits on C:. Falling back to that silently would be exactly the
    thing the rule forbids, just spelled as a default instead of a choice, so
    this refuses instead: `ART_SCRATCH` (or the project default) has to name
    a folder that is already there.
    """
    root = Path(os.environ.get("ART_SCRATCH") or DEFAULT_SCRATCH)
    if not root.is_dir():
        print(f"Scratch directory '{root}' does not exist.")
        print()
        print("This project never writes scratch output to C: or D:, including by")
        print("falling back to the system temp directory. Create the folder above,")
        print("or set ART_SCRATCH to one that already exists.")
        sys.exit(2)
    return root


def find_hst_imager() -> str:
    override = os.environ.get("ART_HST_IMAGER")
    candidates = [Path(override)] if override else []
    candidates.append(DEFAULT_HST_IMAGER)
    for candidate in candidates:
        if candidate.is_file():
            return str(candidate)

    print("hst-imager was not found, so this oracle cannot run.")
    print()
    print("This is a local oracle only — the CI runner has no hst.imager.exe,")
    print("the same reason fat-oracle-check.py and iso-oracle-check.py are not")
    print("in CI either.")
    print()
    print("Set ART_HST_IMAGER to point at it, or install it at:")
    print(f"  {DEFAULT_HST_IMAGER}")
    sys.exit(2)


def find_pfs3_driver() -> Path:
    override = os.environ.get("ART_PFS3_DRIVER")
    candidates = [Path(override)] if override else []
    candidates.append(DEFAULT_PFS3_DRIVER)
    for candidate in candidates:
        if candidate.is_file():
            return candidate

    print("No PFS3 driver (pfs3aio) was found for hst-imager's own formatter.")
    print()
    print("ART ships no Amiga content — this has to come from the user's own")
    print("archive. Set ART_PFS3_DRIVER, or extract pfs3aio from paketler/pfs3aio.lha")
    print("to:")
    print(f"  {DEFAULT_PFS3_DRIVER}")
    sys.exit(2)


# hst-imager writes its output in the console's OEM code page when redirected,
# not UTF-8: measured 2026-09-17, `fs dir` printed the name `Café` as the
# bytes `Caf\x82` under code page 857. Only a non-ASCII name shows it (the
# card round 3 run through `--card`); decoding as UTF-8 turned `é` into U+FFFD.
HST_ENCODING = "oem" if os.name == "nt" else "utf-8"


def run_hst(exe: str, args: list[str], cwd: Path) -> subprocess.CompletedProcess:
    return subprocess.run(
        [exe, *args],
        cwd=cwd,
        capture_output=True,
        text=True,
        encoding=HST_ENCODING,
        errors="replace",
    )


# hst-imager takes `<image>\rdb\dh0` on Windows and `<image>/rdb/dh0` on Linux.
SEP = "\\" if os.name == "nt" else "/"


def dh0(image: Path) -> str:
    return f"{image.name}{SEP}rdb{SEP}dh0"


def run_cargo_test(test: str, env: dict[str, str]) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["cargo", "test", "--quiet", test, "--", "--nocapture"],
        cwd=CARGO_DIR,
        env={**os.environ, **env},
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )


def report(checks: list[tuple[bool, str]]) -> None:
    for ok, what in checks:
        print(f"  {'ok  ' if ok else 'FAIL'} {what}")


# `hst.imager`'s size column, e.g. `15 B`. Longest suffix first so `KB` is not
# mistaken for a trailing `B`.
_SIZE_SUFFIXES = (("GB", 1024**3), ("MB", 1024**2), ("KB", 1024), ("B", 1))

# ART-114: Windows/MS-DOS reserved device basenames. A real AmigaDOS name can
# collide with one of these — `DOSDrivers/AUX` is a real serial-port device
# definition, not anything Windows-shaped — and `hst.imager.exe fs copy`
# extracting to an NTFS path silently drops the entry rather than erroring,
# because Windows itself refuses to create a file or directory with this
# basename (checked, case-insensitively, on the part before the first `.` —
# so `AUX` and `AUX.info` both collide). This is not ART's bug and not
# `hst-imager`'s to fix here; it is the oracle's job to say so explicitly
# rather than report an unexplained shortfall.
_WINDOWS_RESERVED_STEMS = frozenset(
    {"CON", "PRN", "AUX", "NUL"}
    | {f"COM{i}" for i in range(1, 10)}
    | {f"LPT{i}" for i in range(1, 10)}
)


def is_windows_reserved_component(component: str) -> bool:
    """True if this single path segment is a Windows-reserved device
    basename — case-insensitive, and matched on the part before the first
    `.` the way Windows does (`AUX`, `Aux.info`, `com3.txt` all collide)."""
    stem = component.split(".", 1)[0]
    return stem.upper() in _WINDOWS_RESERVED_STEMS


def path_has_reserved_component(path: str) -> bool:
    """True if any `/`-separated segment of an `hst-imager`-style path
    collides with a Windows reserved device basename — checking every
    segment, not just the last, because a directory ART named that way would
    make everything under it equally unextractable on Windows."""
    return any(is_windows_reserved_component(part) for part in path.split("/"))


def parse_size(text: str) -> int:
    """A size cell from `fs dir`'s table, in bytes.

    Every fixture in this script is well under 16 bytes, so only the `B`
    suffix is ever actually exercised — but a size cell is parsed for real
    units rather than assuming `B` forever, so a future, larger fixture fails
    loudly here (a clear `RuntimeError`) instead of tripping a bare
    `ValueError` three lines away with no context.
    """
    stripped = text.strip()
    for suffix, factor in _SIZE_SUFFIXES:
        if stripped.endswith(suffix):
            number = stripped[: -len(suffix)].strip()
            try:
                return round(float(number) * factor)
            except ValueError:
                break
    raise RuntimeError(f"'{text}' is not a size hst-imager's listing format ART recognises")


def parse_dir_listing(text: str) -> dict[str, dict]:
    """`hst.imager fs dir <partition> -r`'s table, by name.

    ```
    Name           |  Size | Date                | Attributes | Comment
    ---------------|-------|---------------------|------------|--------
    Readme         |   6 B | 08/16/2026 05:28:39 | ----RWED   |
    sub            | <DIR> | 08/16/2026 05:28:40 | ----RWED   |
    sub/Nested.txt |   6 B | 08/16/2026 05:28:39 | ----RWED   |
    ```

    Nested paths already use `/`, not `\\` — measured against a real run
    rather than assumed, unlike `fs copy`'s own progress log, which does use
    `\\`.

    A row this cannot make sense of **raises** rather than being skipped. The
    "nothing extra is on the volume" check downstream depends on this
    function having seen every entry; a row silently dropped here would be
    invisible to that check specifically, which is the one assertion that
    actually depends on completeness (every *expected* path is looked up by
    name and a missing one already fails loudly on its own).
    """
    lines = text.splitlines()
    header = None
    for index, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("Name") and "Size" in stripped and "|" in stripped:
            header = index
            break
    if header is None:
        raise RuntimeError(f"no listing table found in hst-imager's output:\n{text}")

    entries: dict[str, dict] = {}
    for line in lines[header + 2 :]:
        if not line.strip():
            break
        if "|" not in line:
            raise RuntimeError(f"a row in hst-imager's listing has no columns: {line!r}")
        parts = [p.strip() for p in line.split("|")]
        if len(parts) < 4:
            raise RuntimeError(
                f"a row in hst-imager's listing has {len(parts)} column(s), "
                f"expected at least 4: {line!r}"
            )
        name, size, _date, attrs = parts[0], parts[1], parts[2], parts[3]
        # The Comment column is real (it round-trips through `-uae
        # UaeMetafile` for a file hst-imager itself wrote with one) but ART's
        # PFS3 writer never sets it — `copy_in_pfs3` only ever calls
        # `update_dir_entry_protection`, never anything that touches a
        # comment (see `native.rs`'s own doc comment). Parsed here for
        # completeness; deliberately never compared below.
        comment = parts[4] if len(parts) > 4 else ""
        is_dir = size == "<DIR>"
        entries[name] = {
            "kind": "dir" if is_dir else "file",
            "size": None if is_dir else parse_size(size),
            "attributes": attrs,
            "comment": comment,
        }
    return entries


def check_art_writes_hst_reads(
    hst: str, work: Path, hook: str, env_var: str, image_name: str
) -> tuple[list[tuple[bool, str]], list[str]]:
    """ART writes a PFS3 volume through `NativeFormatter`; `hst-imager` reads it.

    `build_pfs3_volume_for_oracle_when_asked` builds the volume through the
    same two calls G5 makes (`format_partition` then `copy_in`) and prints a
    JSON description of every entry it believes it wrote — so what is being
    checked is that claim, not ART's opinion of itself. Checked two ways:
    `fs dir -r` for name, kind, size and protection, and `fs copy` extracting
    the volume back out to disk for the file contents themselves.

    Returns `(checks, skipped)`. `skipped` (ART-114) names every file whose
    path collides with a Windows reserved device basename — `hst-imager`
    cannot extract those to an NTFS destination at all, on this OS, for a
    reason that has nothing to do with the volume ART wrote. Those paths are
    counted separately by the caller: neither a pass nor a failure, so the
    summary cannot present a known, explainable absence as an unexplained
    shortfall the way the plain pass count once did.
    """
    checks: list[tuple[bool, str]] = []
    skipped: list[str] = []
    image = work / image_name

    made = run_cargo_test(hook, {env_var: str(image)})
    if made.returncode != 0 or not image.exists():
        checks.append((False, "ART wrote a PFS3 volume"))
        print(made.stdout[-3000:])
        print(made.stderr[-2000:])
        return checks, skipped
    checks.append((True, "ART wrote a PFS3 volume"))
    st = image.stat()
    on_disk = getattr(st, "st_blocks", None)  # POSIX only
    print(
        f"  {image.name}: {st.st_size} bytes long"
        + (f", {on_disk * 512} bytes on disk" if on_disk is not None else "")
    )

    expected = None
    for line in made.stdout.splitlines():
        if line.startswith("json="):
            expected = json.loads(line[len("json=") :])
            break
    if expected is None:
        checks.append((False, "the write hook printed the JSON it claims to have written"))
        print(made.stdout[-2000:])
        return checks, skipped

    listing = run_hst(hst, ["fs", "dir", dh0(image), "-r"], work)
    if listing.returncode != 0:
        checks.append((False, "hst-imager could open the volume ART wrote"))
        print(listing.stdout[-3000:])
        print(listing.stderr[-1000:])
        return checks, skipped

    try:
        actual = parse_dir_listing(listing.stdout)
    except RuntimeError as err:
        checks.append((False, str(err)))
        return checks, skipped

    for item in expected:
        path = item["path"]
        got = actual.get(path)
        if got is None:
            checks.append((False, f"{path} is on the volume, per hst-imager"))
            continue
        checks.append((got["kind"] == item["kind"], f"{path} is a {item['kind']}"))
        if item["kind"] == "file":
            checks.append(
                (
                    got["size"] == item["size"],
                    f"{path} is {item['size']} bytes (hst-imager says {got['size']})",
                )
            )
        # Protection bits, not just names and sizes. `hst-imager`'s own
        # spelling — HSPARWED, uppercase for granted — is
        # `uaem::format_bits`'s output uppercased.
        checks.append(
            (
                got["attributes"] == item["attributes"],
                f"{path} carries {item['attributes']} (hst-imager says {got['attributes']})",
            )
        )

    extra = sorted(set(actual) - {item["path"] for item in expected})
    checks.append((not extra, f"nothing extra is on the volume{f': {extra}' if extra else ''}"))

    # The content check: extract the whole volume back out with hst-imager
    # and hash what comes out against the literal bytes the write hook was
    # given — not against anything ART's own reader says, which would only
    # prove ART agrees with itself again. `fs copy -r` refuses if the
    # destination directory does not already exist (confirmed by hand), so
    # it is created first.
    extract_dir = work / f"{image.stem}-extract"
    shutil.rmtree(extract_dir, ignore_errors=True)
    extract_dir.mkdir(parents=True)
    extracted = run_hst(
        hst, ["fs", "copy", dh0(image), extract_dir.name, "-r"], work
    )
    if extracted.returncode != 0:
        checks.append((False, "hst-imager extracted the volume ART wrote back to disk"))
        print(extracted.stdout[-2000:])
        print(extracted.stderr[-1000:])
        return checks, skipped
    checks.append((True, "hst-imager extracted the volume ART wrote back to disk"))

    for item in expected:
        if item["kind"] != "file":
            continue
        path = item["path"]
        # ART-114: a Windows reserved device basename (`AUX`, `AUX.info`, …)
        # never lands in `extract_dir` at all — `hst-imager` silently drops
        # it while extracting to NTFS, for a reason that is Windows' and not
        # this volume's. Recorded on its own, not as a failure and not
        # folded into the pass count below.
        if path_has_reserved_component(path):
            skipped.append(path)
            continue
        local = extract_dir.joinpath(*path.split("/"))
        if not local.is_file():
            checks.append((False, f"{path} was extracted back to disk"))
            continue
        got_hash = hashlib.sha256(local.read_bytes()).hexdigest()
        checks.append(
            (
                got_hash == item["sha256"],
                f"{path}'s extracted bytes hash to what NativeFormatter was given",
            )
        )

    # ART-310: hst-imager's allocator is a port of pfs3aio's. On a volume whose
    # anodes 0-4 are left unreserved it fails its first new directory
    # (ERROR_DISK_FULL) or hands a later one a reserved number.
    made = run_hst(hst, ["fs", "mkdir", f"{dh0(image)}{SEP}HstMade"], work)
    checks.append((made.returncode == 0, "hst-imager made a directory on the volume ART formatted"))
    if made.returncode != 0:
        print(made.stdout[-1500:])
        print(made.stderr[-500:])
        return checks, skipped
    asked = run_cargo_test(
        "pfs3_anode_for_oracle_when_asked",
        {"ART_PFS3_ANODE_IN": str(image), "ART_PFS3_ANODE_PATH": "HstMade"},
    )
    anode = next(
        (int(l[len("anode="):]) for l in asked.stdout.splitlines() if l.startswith("anode=")),
        None,
    )
    if asked.returncode != 0 or anode is None:
        print(asked.stdout[-3000:])
        print(asked.stderr[-2000:])
    checks.append(
        (
            anode is not None and anode > 4,
            f"hst-imager's new directory has an anode pfs3aio does not reserve "
            f"(anode {anode}; 0-4 are reserved, ART-310)",
        )
    )
    return checks, skipped


def check_hst_writes_art_reads(
    hst: str, driver: Path, work: Path
) -> list[tuple[bool, str]]:
    """`hst-imager` builds and fills a PFS3 volume; ART reads it back.

    The half that catches the hard bugs (Task 11's brief): a wrong length is
    not what ART-079 looked like, so this hashes every file's *contents*.
    """
    checks: list[tuple[bool, str]] = []
    image = work / "hst-write.hdf"
    src = work / "hst-src"
    for stale in (image, src):
        if stale.is_dir():
            shutil.rmtree(stale, ignore_errors=True)
        elif stale.exists():
            stale.unlink()

    (src / "Devs").mkdir(parents=True)
    readme_bytes = b"written by hst-imager, not by ART\n"
    (src / "Readme").write_bytes(readme_bytes)
    assign_bytes = b"an assign script hst-imager copied in\n"
    (src / "Devs" / "Assign").write_bytes(assign_bytes)
    # The Pure bit again, from the other direction: this sidecar is
    # hst-imager's own input, not ART's, and what is being checked is
    # whether ART's *reader* gets it back right.
    (src / "Devs" / "Assign.uaem").write_text(
        "--p-rwed 2021-04-13 02:43:13.68 kept by hst-imager\n", encoding="utf-8"
    )

    blanked = run_hst(hst, ["blank", image.name, "220mb"], work)
    formatted = run_hst(
        hst,
        ["format", image.name, "Rdb", "PDS3", "--file-system-path", str(driver)],
        work,
    )
    copied = run_hst(
        hst,
        ["fs", "copy", "hst-src", dh0(image), "-r", "-uae", "UaeMetafile"],
        work,
    )
    if (
        blanked.returncode != 0
        or formatted.returncode != 0
        or copied.returncode != 0
        or not image.exists()
    ):
        checks.append((False, "hst-imager built and filled a PFS3 volume"))
        for result in (blanked, formatted, copied):
            print(result.stdout[-1500:])
            print(result.stderr[-500:])
        return checks
    checks.append((True, "hst-imager built and filled a PFS3 volume"))

    read = run_cargo_test(
        "read_foreign_pfs3_for_oracle_when_asked", {"ART_PFS3_READ_IN": str(image)}
    )
    if read.returncode != 0:
        checks.append((False, "ART read the volume hst-imager wrote"))
        print(read.stdout[-3000:])
        print(read.stderr[-2000:])
        return checks
    checks.append((True, "ART read the volume hst-imager wrote"))

    got: dict[str, dict] = {}
    volume = None
    for line in read.stdout.splitlines():
        if line.startswith("volume="):
            volume = line[len("volume=") :]
        elif line.startswith("entry="):
            path, kind, size, digest, attrs = line[len("entry=") :].split("|")
            got[path] = {"kind": kind, "size": size, "sha256": digest, "attributes": attrs}

    checks.append((volume == "Workbench", f"the volume name reads back ({volume})"))

    expected = {
        "Readme": {"dir": False, "bytes": readme_bytes, "attributes": "----RWED"},
        "Devs": {"dir": True, "bytes": b"", "attributes": "----RWED"},
        "Devs/Assign": {"dir": False, "bytes": assign_bytes, "attributes": "--P-RWED"},
    }
    for path, want in expected.items():
        entry = got.get(path)
        if entry is None:
            checks.append((False, f"{path} is on the volume, per ART"))
            continue
        if want["dir"]:
            checks.append((entry["kind"] == "dir", f"{path} is a directory"))
        else:
            checks.append((entry["kind"] == "file", f"{path} is a file"))
            checks.append(
                (
                    int(entry["size"]) == len(want["bytes"]),
                    f"{path} is {len(want['bytes'])} bytes (ART says {entry['size']})",
                )
            )
            # The hash, not the length — ART-079 gave every file exactly the
            # right length and another file's bytes.
            expected_hash = hashlib.sha256(want["bytes"]).hexdigest()
            checks.append(
                (
                    entry["sha256"] == expected_hash,
                    f"{path} hashes to what hst-imager was given",
                )
            )
        checks.append(
            (
                entry["attributes"] == want["attributes"],
                f"{path} carries {want['attributes']} (ART says {entry['attributes']})",
            )
        )

    # Mirrors direction 1's own "nothing extra" check: `got` was built from
    # every `entry=` line ART's read hook printed, so this depends on that
    # walk being complete, not on the three paths this script happens to look
    # up.
    extra = sorted(set(got) - set(expected))
    checks.append((not extra, f"nothing extra is on the volume{f': {extra}' if extra else ''}"))

    return checks


def card_partition(image: Path, slot: int | None, drive: str) -> str:
    """`tools/hst_imager.rs::partition_target`'s address, with the bare image
    name (this script's `cwd` rule): `<image>\\mbr\\<slot>\\rdb\\<drive>`."""
    disk = image.name if slot is None else f"{image.name}{SEP}mbr{SEP}{slot}"
    return f"{disk}{SEP}rdb{SEP}{drive.lower()}"


def size_agrees(listed: int, exact: int) -> bool:
    """`fs dir` prints `2.93 KB`, not bytes, above 1023 — so a listed size is
    only as precise as its unit. Exact bytes are the hash's job, below."""
    if listed == exact:
        return True
    return exact >= 1024 and abs(listed - exact) <= exact * 0.01


# WinUAE's `_UAEFSDB.___`: one 600-byte record per host name the extractor had
# to invent — valid (1 byte), mode (4), the Amiga name (257, Latin-1), the
# host name (257), the comment (81).
_FSDB_NAME = "_UAEFSDB.___"
_FSDB_RECORD = 600


def fsdb_names(folder: Path) -> dict[str, str]:
    """Host name → Amiga name, from `folder`'s `_UAEFSDB.___` if it has one.

    `hst-imager fs copy` to an NTFS folder stores a non-ASCII Amiga name under
    an invented host name and records the real one here: measured 2026-09-17,
    `Café` came out as the folder `__uae___Caf_`, with a record whose Amiga
    name is `Caf\\xe9`."""
    path = folder / _FSDB_NAME
    if not path.is_file():
        return {}
    data = path.read_bytes()
    names = {}
    for start in range(0, len(data) - _FSDB_RECORD + 1, _FSDB_RECORD):
        record = data[start : start + _FSDB_RECORD]
        if record[0] == 0:
            continue
        amiga = record[5:262].split(b"\0", 1)[0].decode("latin-1")
        host = record[262:519].split(b"\0", 1)[0].decode("latin-1")
        names[host] = amiga
    return names


def extracted_files(root: Path) -> dict[str, Path]:
    """Every file `fs copy` extracted under `root`, by its Amiga path."""
    found: dict[str, Path] = {}
    stack = [(root, "")]
    while stack:
        folder, prefix = stack.pop()
        names = fsdb_names(folder)
        for child in folder.iterdir():
            if child.name == _FSDB_NAME:
                continue
            amiga = names.get(child.name, child.name)
            path = f"{prefix}{amiga}"
            if child.is_dir():
                stack.append((child, f"{path}/"))
            else:
                found[path] = child
    return found


def check_card(hst: str, image: Path, art_json: Path) -> tuple[list[tuple[bool, str]], dict]:
    """Card round 3: every PFS3 partition of a whole card ART built, listed and
    extracted by hst-imager, and compared with ART's own listing of it
    (`writes_a_small_card_for_the_hst_imager_oracle`'s `.art.json`): the path
    set, each file's size, and each file's SHA-256 from `fs copy`."""
    checks: list[tuple[bool, str]] = []
    counts: dict[str, dict] = {}
    listing = json.loads(art_json.read_text(encoding="utf-8"))
    work = image.parent
    scratch = require_scratch_dir()

    with tempfile.TemporaryDirectory(prefix="art-pfs3-card-", dir=scratch) as tmp:
        for partition in listing["partitions"]:
            drive = partition["drive"]
            target = card_partition(image, partition["slot"], drive)
            entries = partition["entries"]
            want_files = {e["path"]: e for e in entries if not e.get("dir")}
            want_dirs = {e["path"] for e in entries if e.get("dir")}
            count = counts.setdefault(
                drive, {"art_files": len(want_files), "art_dirs": len(want_dirs),
                        "hst_files": 0, "hst_dirs": 0, "hashed": 0}
            )

            listed = run_hst(hst, ["fs", "dir", target, "-r"], work)
            if listed.returncode != 0:
                checks.append((False, f"{drive}: hst-imager listed {target}"))
                print(listed.stdout[-2000:])
                print(listed.stderr[-1000:])
                continue
            if not want_files and not want_dirs and "Name" not in listed.stdout:
                # An empty volume prints no table at all.
                got = {}
            else:
                try:
                    got = parse_dir_listing(listed.stdout)
                except RuntimeError as err:
                    checks.append((False, f"{drive}: {err}"))
                    continue
            got_files = {p: e for p, e in got.items() if e["kind"] == "file"}
            got_dirs = {p for p, e in got.items() if e["kind"] == "dir"}
            count["hst_files"] = len(got_files)
            count["hst_dirs"] = len(got_dirs)

            checks.append((
                set(got_files) == set(want_files),
                f"{drive}: the same files (only ART: {sorted(set(want_files) - set(got_files))}, "
                f"only hst-imager: {sorted(set(got_files) - set(want_files))})",
            ))
            checks.append((
                got_dirs == want_dirs,
                f"{drive}: the same directories (only ART: {sorted(want_dirs - got_dirs)}, "
                f"only hst-imager: {sorted(got_dirs - want_dirs)})",
            ))
            for path, want in want_files.items():
                if path in got_files and not size_agrees(got_files[path]["size"], want["size"]):
                    checks.append((False, f"{drive}: {path} is {want['size']} bytes "
                                          f"(hst-imager lists {got_files[path]['size']})"))

            if not want_files:
                continue
            out = Path(tmp) / drive.lower()
            out.mkdir()
            copied = run_hst(hst, ["fs", "copy", target, str(out), "-r"], work)
            if copied.returncode != 0:
                checks.append((False, f"{drive}: hst-imager extracted the partition"))
                print(copied.stdout[-2000:])
                print(copied.stderr[-1000:])
                continue
            on_disk = extracted_files(out)
            for path, want in want_files.items():
                if path_has_reserved_component(path):
                    continue
                local = on_disk.get(path)
                if local is None:
                    checks.append((False, f"{drive}: {path} was extracted"))
                    continue
                digest = hashlib.sha256(local.read_bytes()).hexdigest()
                ok = digest == want["sha256"]
                count["hashed"] += ok
                if not ok:
                    checks.append((False, f"{drive}: {path}'s bytes hash to ART's listing"))
    return checks, counts


def main_card(image: Path, art_json: Path) -> int:
    hst = find_hst_imager()
    if not image.is_file() or not art_json.is_file():
        print(f"'{image}' or '{art_json}' does not exist; run "
              "writes_a_small_card_for_the_hst_imager_oracle first.")
        return 2
    print(f"hst-imager: {hst}\ncard: {image}\nART's listing: {art_json}\n")
    checks, counts = check_card(hst, image, art_json)
    for drive, count in counts.items():
        print(f"  {drive}: ART {count['art_files']} files / {count['art_dirs']} dirs; "
              f"hst-imager {count['hst_files']} files / {count['hst_dirs']} dirs; "
              f"{count['hashed']} hashes equal")
    failures = [what for ok, what in checks if not ok]
    if failures:
        print(f"\n{len(failures)} difference(s):")
        for item in failures:
            print(f"  - {item}")
        return 1
    print("\nART and hst-imager agree on every partition of the card: paths, sizes, bytes.")
    return 0


def main() -> int:
    # A difference names a path, and a path can be non-ASCII: never let the
    # console's code page turn a report into a traceback.
    sys.stdout.reconfigure(errors="replace")
    if len(sys.argv) >= 2 and sys.argv[1] == "--card":
        if len(sys.argv) != 4:
            print("usage: pfs3-oracle-check.py --card IMAGE ART_JSON")
            return 2
        return main_card(Path(sys.argv[2]), Path(sys.argv[3]))
    hst = find_hst_imager()
    driver = find_pfs3_driver()
    scratch = require_scratch_dir()
    print(f"hst-imager: {hst}")
    print(f"pfs3 driver: {driver}")
    print(f"scratch: {scratch}")
    print()

    with tempfile.TemporaryDirectory(prefix="art-pfs3-oracle-", dir=scratch) as tmp:
        work = Path(tmp)

        print("ART writes, hst-imager reads:")
        checks_a, skipped_a = check_art_writes_hst_reads(
            hst, work, "build_pfs3_volume_for_oracle_when_asked", "ART_PFS3_WRITE_OUT",
            "art-write.hdf",
        )
        report(checks_a)

        print("\nART writes past MAXSMALLDISK (SUPERINDEX mode), hst-imager reads:")
        checks_l, skipped_l = check_art_writes_hst_reads(
            hst, work, "build_large_pfs3_volume_for_oracle_when_asked",
            "ART_PFS3_WRITE_OUT_LARGE", "art-write-large.hdf",
        )
        report(checks_l)
        checks_a += checks_l
        skipped_a += skipped_l

        print("\nhst-imager writes, ART reads:")
        checks_b = check_hst_writes_art_reads(hst, driver, work)
        report(checks_b)

    # ART-114: named on its own, never as a failure and never folded into the
    # pass count — an oracle that reports "3059 of 3061 matched" for a known,
    # explainable Windows limitation is doing part of the job of the bug it
    # exists to catch.
    if skipped_a:
        print(
            f"\n{len(skipped_a)} file(s) skipped — Windows/MS-DOS reserved device "
            "name(s) (CON, PRN, AUX, NUL, COM1-9, LPT1-9, with or without an "
            "extension). `hst-imager fs copy` cannot extract these to an NTFS "
            "path on this OS; that is a Windows limitation in the oracle's own "
            "tool, not a defect in the volume ART wrote (ART-114):"
        )
        for path in skipped_a:
            print(f"  - {path}")

    failures = [what for ok, what in [*checks_a, *checks_b] if not ok]
    if failures:
        print(f"\n{len(failures)} check(s) failed:")
        for item in failures:
            print(f"  - {item}")
        print(
            "\nART and hst-imager disagree about a PFS3 volume. That means the "
            "volume is wrong, not the reader — libpfs3 backs both ART's writer "
            "and its reader, so only an outside implementation like hst-imager "
            "can tell the two apart."
        )
        return 1

    print("\nART and hst-imager agree, both directions — names, sizes, bytes, and protection bits.")
    if skipped_a:
        print(
            f"({len(skipped_a)} file(s) with Windows-reserved names skipped for "
            "extraction, see above — not a failure.)"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
