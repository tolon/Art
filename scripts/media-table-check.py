#!/usr/bin/env python3
"""ART's install-media hash tables, checked against the owner's own media (§4.2).

Sibling of `scripts/rom-table-check.py` (ART-104): that script re-verifies the
Kickstart table against an independent database. This one re-verifies the
install-media tables — **two of them**, since
`docs/superpowers/specs/2026-09-08-os-builder-intake-design.md` §3.6: the
186-row adopted table (`core/osinstall/media_hashes.json`, from
`rootrootde/emu68hatcher`, MIT) and ART's own 8-row table
(`core/osinstall/media_hashes_own.json`, the AmigaOS 3.9 CD-ROM and its update
archives the adopted table does not carry) — against real disks. The check
ART's own unit tests cannot do, because a mistake shared between
`mediahash.rs`'s reader and the JSON it reads would pass every test in the
suite and still be wrong. Rows from both tables are checked together, adopted
first, mirroring `mediahash.rs::rows()`; each reported row says which table
answered.

**Read-only where it matters: the media directory is never written to.** This
script only ever opens a file inside the given directory in `"rb"` mode to hash
it (`hashlib.md5`, streamed in 1 MiB chunks) or lists directory entries
(`os.walk`). Nothing here calls `os.remove`, `os.rename` or `shutil.move` at
all, and the single `open(..., "w")` in the file is `--emit-confirmed`'s, which
writes exactly one path — `core/osinstall/media_hashes_confirmed.json` inside
this repository — and can never name a path under the media directory. The
directory on the command line is the owner's own irreplaceable install media
and this script treats it exactly like `fat-oracle-check.py` and
`icon-oracle-check.py` treat theirs: read, never touched.

`--emit-confirmed` is `rom-table-check.py --emit`'s pattern (ART-104): the
data ART ships about *its own* checking is generated from a real run rather
than hand-written, so it can be re-derived instead of re-trusted. It records
which rows a real disk here hashed to, on what day, and against what — and
nothing else. It never edits `media_hashes.json`, which stays exactly as
adopted from Hatcher; ART's own measurement lives in its own file so the two
provenances cannot be confused for one another.

Three outcomes, kept distinct (this project's named failure class is
collapsing them into one):

    verified     a file under the directory hashes to exactly this row's md5.
                 Strong evidence *about the row*, not about the disk (§4.3 of
                 the design) — reported with what confirmed it.
    unverified   no file under the directory hashes to this row. Expected for
                 most rows — the owner does not own every disk Hatcher's table
                 knows about — and is **not** a failure. 151 of 186 rows were
                 unverified on the day this script was written, and that
                 number describes what the owner happens to own, not a defect.
    conflicting  something matched but disagrees. Checked two ways, both
                 independent of which files happen to be on hand:
                   - a real file's md5 lands on more than one row (only
                     possible if two rows share an md5, which
                     mediahash.rs's own `every_md5_in_the_table_is_distinct`
                     test already asserts never happens in the shipped table
                     — checked again here anyway, against the file actually
                     read, rather than trusted from that test);
                   - a row's own required fields are missing or malformed
                     (an empty version/volume/name/source, or an md5 that is
                     not 32 lowercase hex characters) — a row whose fields
                     contradict the table's own schema, independent of
                     whether the owner has a copy of it at all.

What makes it a check rather than a report
------------------------------------------

**A test is not a guard until the defect has been put back and seen to fail
it** (`CLAUDE.md`). The first version of this script printed the three numbers
above and then exited 0 whatever they were, so it passed with **0 verified** —
the one result that means the table and the media no longer agree at all. Two
assertions close that, and they are deliberately different in kind:

    at least one row verifies    A directory was given, so files were meant to
                                 be found in it and looked up. Zero is the
                                 outcome a broken reader, a renamed hash field
                                 or a table gutted down to nothing all produce,
                                 and it is exactly what the old script called
                                 success. This is `rom-table-check.py --scan`'s
                                 own condition (`return hit`, so no ROM
                                 identified is exit 1), applied to the same
                                 kind of question.
    --expect-verified N          When the *caller* states a number, it is held
                                 to that number exactly. This is where a
                                 specific claim about specific media belongs —
                                 never a constant in this file. The owner's 35
                                 is this machine's measurement against this
                                 machine's disks; a user with a 3.1 set must
                                 not meet a check that fails because their
                                 media is not the author's.

`conflicting` remains a failure on its own, independent of both.

The standing measurement (2026-09-06), against
`E:\\amiga\\Amigatolon\\paketler\\3.2\\AmigaOs 3.2\\ADF`: **35 verified, 151
unverified, 0 conflicting** — every one of the owner's 35 ADFs matched a row,
to the right name and the right source (`Hyperion (3.2 base)`), zero misses.
That number is the owner's own and lives in `media_hashes_confirmed.json` and
in `--expect-verified`, not in this script's pass condition. If a re-run gets
a different number, that is a finding to report, not something to adjust this
script until it agrees.

Not in CI, ever: it needs media ART must never ship — the same reason
`fat-oracle-check.py` and `icon-oracle-check.py` are not in CI.

Usage:
    python scripts/media-table-check.py DIR
    python scripts/media-table-check.py DIR --expect-verified 35
    python scripts/media-table-check.py DIR --kind disc
    python scripts/media-table-check.py DIR --emit-confirmed --against "..."

DIR is walked recursively for `.adf`, `.iso`, `.lha`, `.lzh`, `.zip` and `.7z`
files (case-insensitive extension match, per `mediahash.rs::MEDIA_EXTENSIONS`
and the design's own recipe-media shapes).
"""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
import sys
from pathlib import Path

TODAY = datetime.date.today().isoformat()

REPO = Path(__file__).resolve().parent.parent
TABLE_PATH = REPO / "src-tauri" / "src" / "core" / "osinstall" / "media_hashes.json"
OWN_TABLE_PATH = REPO / "src-tauri" / "src" / "core" / "osinstall" / "media_hashes_own.json"
CONFIRMED_PATH = (
    REPO / "src-tauri" / "src" / "core" / "osinstall" / "media_hashes_confirmed.json"
)
MEDIA_EXTENSIONS = {".adf", ".iso", ".lha", ".lzh", ".zip", ".7z"}
CHUNK_SIZE = 1024 * 1024
MEDIA_KINDS = ("floppy", "disc", "archive")


def load_table() -> list[dict]:
    """The adopted table's rows, each tagged `_table` = `"adopted"` — an
    internal bookkeeping key, never part of either file's real schema, that
    lets every row in the combined list say which file it came from (mirrors
    `mediahash.rs::MediaRow::table_origin`).
    """
    data = json.loads(TABLE_PATH.read_text(encoding="utf-8"))
    rows = data["media"]
    for row in rows:
        row["_table"] = "adopted"
    return rows


def load_own_table() -> list[dict]:
    """ART's own table's rows, tagged `_table` = `"own"` — see [`load_table`]."""
    data = json.loads(OWN_TABLE_PATH.read_text(encoding="utf-8"))
    rows = data["rows"]
    for row in rows:
        row["_table"] = "own"
    return rows


def load_all_rows() -> list[dict]:
    """Both tables, adopted first — the same order `mediahash.rs::rows()`
    serves them in, so a row's position here matches its position there.
    """
    return load_table() + load_own_table()


def md5_of(path: Path) -> str:
    """Streamed, so a multi-hundred-MB `.iso` does not have to fit in memory
    at once — the same reason `core/hashing.rs::md5_file` chunks rather than
    reading a file whole. Opens the file for reading only.
    """
    digest = hashlib.md5()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(CHUNK_SIZE), b""):
            digest.update(chunk)
    return digest.hexdigest()


def find_media(directory: Path) -> list[Path]:
    """Every `.adf`/`.iso`/`.lha` file under `directory`, recursively.
    `os.walk` only lists names; nothing here touches an entry it finds.
    """
    found: list[Path] = []
    for root, _dirs, files in os.walk(directory):
        for name in files:
            if Path(name).suffix.lower() in MEDIA_EXTENSIONS:
                found.append(Path(root) / name)
    return sorted(found)


def row_kind(row: dict) -> str:
    """The row's own `kind`, or `"floppy"` when it states none — the same
    default `mediahash.rs::MediaKindTag` gives an adopted row. Never treated
    as a fact worth printing for an adopted row (see `describe_row` below):
    this default exists so filtering and schema-checking have one answer to
    ask, not so a screen can claim Hatcher's data says something it does not.
    """
    return row.get("kind") or "floppy"


def row_schema_problem(row: dict) -> str | None:
    """Mirrors `mediahash.rs`'s own invariant tests
    (`every_md5_is_32_lowercase_hex_characters`,
    `every_row_has_its_descriptive_fields_filled_in`) — checked again here,
    independently, against whatever table this script is actually pointed at,
    rather than trusted from the Rust suite. Extended for the two-table shape
    (design's §3.6): `kind`, `artefact` and `filenames` are optional — an
    adopted row states none of them — but when a row does state one, its
    shape is checked the same way `mediahash.rs`'s own `MediaKindTag`/
    `Option<String>`/`Vec<String>` fields are.
    """
    md5 = row.get("md5", "")
    if len(md5) != 32 or not all(c in "0123456789abcdef" for c in md5):
        return f"md5 {md5!r} is not 32 lowercase hex characters"
    for field in ("version", "volume", "name", "source"):
        if not row.get(field):
            return f"'{field}' is empty"
    if "kind" in row and row["kind"] not in MEDIA_KINDS:
        return f"'kind' {row['kind']!r} is not one of {MEDIA_KINDS}"
    if "artefact" in row and row["artefact"] is not None and not isinstance(row["artefact"], str):
        return "'artefact' is neither a string nor null"
    if "filenames" in row and not isinstance(row["filenames"], list):
        return "'filenames' is not a list"
    return None


def describe_row(row: dict) -> str:
    """The bracketed tail of a report line: which table answered, and — only
    for an own-table row, which is the only one that really states them —
    its kind and artefact id. Printing `kind`/`artefact` for an *adopted* row
    would put a claim on screen that Hatcher's data never made (this
    project's own named failure: a confident sentence with nothing behind
    it) — `row_kind`'s "floppy" default is for filtering, not for a report.
    """
    table = row.get("_table", "?")
    if table != "own":
        return f"[{table}]"
    bits = [f"[{table}", f"kind={row_kind(row)}"]
    artefact = row.get("artefact")
    if artefact:
        bits.append(f"artefact={artefact}")
    return " ".join(bits) + "]"


CONFIRMED_COMMENT = (
    "ART's OWN record of which rows in media_hashes.json have been checked "
    "against real install media, and what checked them. NOT part of the adopted "
    "Hatcher table and never to be merged into it: that file is somebody else's "
    "data, this one is ART's measurement of it. Generated by "
    "`python scripts/media-table-check.py DIR --emit-confirmed --against \"...\"` "
    "-- do not hand-edit; re-run the check instead. A row absent from here is "
    "NOT suspect: it means nobody here owns a disk that hashes to it, which is "
    "true of most of the table and says nothing about the row."
)


def load_existing_checks() -> list[dict]:
    """The file's own `checks` list, or none if it does not parse.

    A file that fails to parse is treated as empty rather than raised on: the
    shipped file always parses (it is generated by this same function and read
    by `mediahash.rs`'s own tests), and a hand-broken one should not stop a
    real check run from recording ART's own measurement — it should simply
    stop accumulating onto whatever was already broken.
    """
    try:
        parsed = json.loads(CONFIRMED_PATH.read_text(encoding="utf-8"))
        checks = parsed.get("checks", [])
        return checks if isinstance(checks, list) else []
    except (OSError, json.JSONDecodeError, AttributeError):
        return []


def emit_confirmed(verified: list[dict], against: str) -> None:
    """Add ART's own record of which rows a real disk here hashed to.

    A separate file from `media_hashes.json` on purpose. That table is
    Hatcher's and is adopted verbatim; this is ART's measurement of it, and
    the round's whole discipline is that two facts from two sources are shown
    as two facts. Mixing ART's annotation into the adopted rows would make the
    provenance of a field a question nobody could answer from the file.

    Only `md5` is written per row — never the row's name, version or source.
    Those live in the table, and a second copy of them here is a copy that can
    drift (ART-249's shape). What is recorded is the *fact of the check*: this
    hash, this day, this material.

    **Adds a check event; never replaces the file.** The format is
    `checks: [...]` precisely so several people's media can accumulate — the
    owner's 3.2 set today, somebody else's 3.1 set next month — and a version
    that always wrote a single-element list silently discarded every earlier
    run's record the moment a second one was made. `parse_confirmed` on the
    Rust side already keeps the *first* check to name a given hash, so an
    earlier run's provenance for a hash wins over a later one that happens to
    re-confirm it — appending here, rather than prepending, is what keeps that
    true.
    """
    existing = load_existing_checks()
    existing.append(
        {
            "checked": TODAY,
            "against": against,
            "md5": [row["md5"] for row in verified],
        }
    )
    payload = {"$comment": CONFIRMED_COMMENT, "checks": existing}
    with open(CONFIRMED_PATH, "w", encoding="utf-8", newline="\n") as handle:
        json.dump(payload, handle, indent=2, ensure_ascii=False)
        handle.write("\n")
    try:
        shown = os.path.relpath(CONFIRMED_PATH, REPO)
    except ValueError:
        shown = str(CONFIRMED_PATH)
    print(
        f"\nwrote {shown}: {len(verified)} confirmed row(s) added, checked {TODAY} "
        f"({len(existing)} check(s) on file now)"
    )
    print("     Read the diff before committing it — this is a claim ART will put on screen.")


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "directory", help="a folder of real install media (.adf/.iso/.lha) — read-only"
    )
    parser.add_argument(
        "--emit-confirmed",
        action="store_true",
        help="rewrite core/osinstall/media_hashes_confirmed.json from this run's verified rows",
    )
    parser.add_argument(
        "--expect-verified",
        type=int,
        default=None,
        metavar="N",
        help="fail unless exactly N rows verify. The caller's own claim about the caller's "
        "own media -- never a constant in this script, which must stay true for a user "
        "whose disks are not the author's",
    )
    parser.add_argument(
        "--against",
        default="",
        help="what this run checked the table against, in the words the screen will show "
        '(e.g. "the ART author\'s own AmigaOS 3.2 install set, 35 ADFs")',
    )
    parser.add_argument(
        "--kind",
        choices=MEDIA_KINDS,
        default=None,
        help="only check rows of this kind (design's §3.6) -- an adopted row that states "
        "none defaults to 'floppy', the same default mediahash.rs::MediaKindTag gives it",
    )
    args = parser.parse_args()

    directory = Path(args.directory)
    if not directory.is_dir():
        print(f"{directory} is not a directory")
        return 2

    adopted_rows = load_table()
    own_rows = load_own_table()
    rows = adopted_rows + own_rows
    if args.kind is not None:
        rows = [row for row in rows if row_kind(row) == args.kind]
    try:
        table_shown = os.path.relpath(TABLE_PATH, REPO)
        own_table_shown = os.path.relpath(OWN_TABLE_PATH, REPO)
    except ValueError:  # a different drive on Windows -- relpath cannot express it
        table_shown = str(TABLE_PATH)
        own_table_shown = str(OWN_TABLE_PATH)
    print(f"adopted table: {len(adopted_rows)} row(s) from {table_shown}")
    print(f"own table:     {len(own_rows)} row(s) from {own_table_shown}")
    if args.kind is not None:
        print(f"(--kind {args.kind}: {len(rows)} row(s) considered)")

    # Which real hash a row's md5 belongs to more than one row for — a table
    # defect, not something a missing disk could ever produce.
    rows_by_md5: dict[str, list[dict]] = {}
    for row in rows:
        rows_by_md5.setdefault(row.get("md5", ""), []).append(row)
    duplicated_md5 = {md5: rs for md5, rs in rows_by_md5.items() if len(rs) > 1}

    files = find_media(directory)
    print(f"media directory: {len(files)} file(s) with a recognised extension\n")

    files_by_hash: dict[str, list[Path]] = {}
    hash_of_file: dict[Path, str] = {}
    for path in files:
        digest = md5_of(path)
        files_by_hash.setdefault(digest, []).append(path)
        hash_of_file[path] = digest

    verified: list[tuple[dict, list[Path]]] = []
    unverified: list[dict] = []
    conflicting: list[tuple[dict, str]] = []

    for row in rows:
        schema_problem = row_schema_problem(row)
        md5 = row.get("md5", "")
        matches = files_by_hash.get(md5, [])

        if schema_problem is not None:
            conflicting.append((row, f"row's own fields contradict the schema: {schema_problem}"))
        elif md5 in duplicated_md5:
            other_names = ", ".join(
                r["name"] for r in duplicated_md5[md5] if r is not row
            )
            conflicting.append(
                (row, f"this md5 is also claimed by: {other_names} (a match here would be ambiguous)")
            )
        elif matches:
            verified.append((row, matches))
        else:
            unverified.append(row)

    print(f"verified:    {len(verified)}")
    print(f"unverified:  {len(unverified)}")
    print(f"conflicting: {len(conflicting)}\n")

    for row, matches in verified:
        names = ", ".join(str(p.relative_to(directory)) for p in matches)
        print(
            f"  ok   {row['name']} ({row['version']}, {row['source']}) "
            f"{describe_row(row)}  <- {names}"
        )

    for row, why in conflicting:
        print(f"  FAIL {row['name']} ({row.get('md5', '')}) {describe_row(row)}: {why}")

    for row in unverified:
        print(
            f"  --   {row['name']} ({row['version']}, {row['source']}, {row['md5']}) "
            f"{describe_row(row)}"
        )

    # Files in the folder that match no row **in either table**, regardless
    # of `--kind` — a file must never be called "not in the table" just
    # because this run was scoped to one kind. This is the other half of the
    # question `unverified` answers (a row nobody's file claims); this is a
    # file nobody's row claims, and the two are not the same list read
    # backwards: a schema-broken or cross-table-duplicated row's file would
    # show up here too, correctly, rather than being silently counted as a
    # match.
    every_md5 = {row.get("md5", "") for row in adopted_rows + own_rows}
    claimed_paths = {p for _row, matches in verified for p in matches}
    not_in_table = [
        p for p in files if p not in claimed_paths and hash_of_file[p] not in every_md5
    ]
    if not_in_table:
        print(f"\nnot in the table: {len(not_in_table)} file(s)")
        for path in not_in_table:
            print(f"  ??   {path.relative_to(directory)}")

    print(
        f"\n{len(files)} file(s) hashed against {len(rows)} row(s): "
        f"{len(verified)} verified, {len(unverified)} unverified "
        f"(no file here claims them - not a failure), {len(conflicting)} conflicting"
    )

    if conflicting:
        print("\nFAIL: conflicting row(s) found - see above.")
        return 1

    # The assertion that makes this a check. A directory was given, so files
    # were meant to be hashed and looked up; zero rows verifying is what a
    # broken reader, a renamed field or an emptied table all look like, and it
    # is what the first version of this script called success.
    if not verified:
        print(
            f"\nFAIL: no row in the table verified against anything in {directory}."
            f"\n      {len(files)} file(s) with a recognised extension were hashed and none"
            "\n      of them matched a row. Either this is not a folder of install media,"
            "\n      or the table and the reader no longer agree - which is the whole"
            "\n      reason this script exists."
        )
        return 1

    # And where the caller stated a number, it is held to it exactly. The
    # owner's 35 is the owner's, and it belongs on the command line and in
    # media_hashes_confirmed.json - never in this file's pass condition.
    if args.expect_verified is not None and len(verified) != args.expect_verified:
        print(
            f"\nFAIL: expected exactly {args.expect_verified} verified row(s), got {len(verified)}."
            "\n      A difference here is a finding to report, not a number to adjust."
        )
        return 1

    if args.emit_confirmed:
        if not args.against:
            print("\nFAIL: --emit-confirmed needs --against, which is the sentence a")
            print("      user will read. A confirmation that cannot say what confirmed")
            print("      it is not a citation.")
            return 2
        emit_confirmed([row for row, _matches in verified], args.against)

    expected = (
        f", and exactly the {args.expect_verified} the caller expected"
        if args.expect_verified is not None
        else ""
    )
    print(
        f"\nok   {len(verified)} row(s) verified against real media{expected}, every match was "
        "clean,\n     and nothing in the table contradicts itself"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
