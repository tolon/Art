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


def run_hst(exe: str, args: list[str], cwd: Path) -> subprocess.CompletedProcess:
    return subprocess.run(
        [exe, *args],
        cwd=cwd,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )


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


def check_art_writes_hst_reads(hst: str, work: Path) -> list[tuple[bool, str]]:
    """ART writes a PFS3 volume through `NativeFormatter`; `hst-imager` reads it.

    `build_pfs3_volume_for_oracle_when_asked` builds the volume through the
    same two calls G5 makes (`format_partition` then `copy_in`) and prints a
    JSON description of every entry it believes it wrote — so what is being
    checked is that claim, not ART's opinion of itself. Checked two ways:
    `fs dir -r` for name, kind, size and protection, and `fs copy` extracting
    the volume back out to disk for the file contents themselves.
    """
    checks: list[tuple[bool, str]] = []
    image = work / "art-write.hdf"

    made = run_cargo_test(
        "build_pfs3_volume_for_oracle_when_asked", {"ART_PFS3_WRITE_OUT": str(image)}
    )
    if made.returncode != 0 or not image.exists():
        checks.append((False, "ART wrote a PFS3 volume"))
        print(made.stdout[-3000:])
        print(made.stderr[-2000:])
        return checks
    checks.append((True, "ART wrote a PFS3 volume"))

    expected = None
    for line in made.stdout.splitlines():
        if line.startswith("json="):
            expected = json.loads(line[len("json=") :])
            break
    if expected is None:
        checks.append((False, "the write hook printed the JSON it claims to have written"))
        print(made.stdout[-2000:])
        return checks

    listing = run_hst(hst, ["fs", "dir", f"{image.name}\\rdb\\dh0", "-r"], work)
    if listing.returncode != 0:
        checks.append((False, "hst-imager could open the volume ART wrote"))
        print(listing.stdout[-3000:])
        print(listing.stderr[-1000:])
        return checks

    try:
        actual = parse_dir_listing(listing.stdout)
    except RuntimeError as err:
        checks.append((False, str(err)))
        return checks

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
    extract_dir = work / "art-write-extract"
    shutil.rmtree(extract_dir, ignore_errors=True)
    extract_dir.mkdir(parents=True)
    extracted = run_hst(
        hst, ["fs", "copy", f"{image.name}\\rdb\\dh0", extract_dir.name, "-r"], work
    )
    if extracted.returncode != 0:
        checks.append((False, "hst-imager extracted the volume ART wrote back to disk"))
        print(extracted.stdout[-2000:])
        print(extracted.stderr[-1000:])
        return checks
    checks.append((True, "hst-imager extracted the volume ART wrote back to disk"))

    for item in expected:
        if item["kind"] != "file":
            continue
        path = item["path"]
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

    return checks


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
        ["fs", "copy", "hst-src", f"{image.name}\\rdb\\dh0", "-r", "-uae", "UaeMetafile"],
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


def main() -> int:
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
        checks_a = check_art_writes_hst_reads(hst, work)
        report(checks_a)

        print("\nhst-imager writes, ART reads:")
        checks_b = check_hst_writes_art_reads(hst, driver, work)
        report(checks_b)

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
    return 0


if __name__ == "__main__":
    sys.exit(main())
