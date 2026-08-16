#!/usr/bin/env python3
"""ART's Kickstart table, checked against an independent database (ART-104).

ART used to carry ten hand-listed SHA-256 hashes with no recorded provenance.
Measured against the user's own 29 Kickstart dumps, they matched **none** of
them: `identify_rom` fell through to the version the ROM states about itself,
which cannot name a machine, so `rom_suits` — the check that says "this ROM is
not for the machine you chose" — could never fire at all.

This script is where the table comes from now. It reads the **Remus split
database** shipped with `amitools` (GPL-2.0-or-later; ART is
GPL-3.0-or-later, so compatible — recorded in THIRD_PARTY_LICENSES.md) and
either emits `src-tauri/src/core/rom/remus.rs` or verifies that the committed
file still says what the database says.

Two properties make this worth having rather than a longer hand-list:

* **It tells machines apart from the bytes.** Kickstart 40.68 exists as an
  A1200 build and an A4000 build; they share a revision and differ in content,
  and the database keys on a value the ROM stores about itself, so the two are
  distinguished without trusting a filename. That is ART's own content-first
  rule (phase 2a) applied to ROMs.
* **It cannot grow a claim quietly.** Every parenthetical in the database's
  names is mapped to machines explicitly below. A parenthetical this script
  has never seen is an error, not a guess — see MACHINES.

Usage:
    python scripts/rom-table-check.py            # verify the committed table
    python scripts/rom-table-check.py --emit     # rewrite it from the database
    python scripts/rom-table-check.py --scan DIR # what it makes of real ROMs
"""

from __future__ import annotations

import argparse
import glob
import os
import re
import struct
import sys

try:
    import amitools
    from amitools.rom.remusfile import RemusSplitFile
except ImportError:  # pragma: no cover - the message is the point
    sys.exit(
        "amitools is not installed. `pip install amitools` — the same "
        "dependency scripts/oracle-check.py already needs."
    )

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GENERATED = os.path.join(REPO, "src-tauri", "src", "core", "rom", "remus.rs")

# Where a name's parenthetical maps to actual Amiga models.
#
# **A whitelist on purpose.** Splitting the string on `/` looked fine until it
# met the real data: `A500/2000` gave only A500 (the bare number is not
# matched), and `A1200_R2` gave nothing at all. A *partial* list is worse than
# none here — `rom_suits` would then warn "wrong machine" about a ROM that
# suits the machine perfectly well, which is ART-104's own complaint in
# reverse. So each string is answered once, by hand, and anything new stops
# the script.
#
# An empty list is a real answer: it means the database named something that
# is not a machine (a distribution, an accelerator's own ROM, a variant), and
# ART then claims no machine rather than inventing one.
MACHINES: dict[str, list[str]] = {
    "": [],
    # `Kickstart 45.61 AmigaForever (1200)` — the model without its `A`, which
    # is why this map is a map and not a pattern.
    "1200": ["A1200"],
    "A1000": ["A1000"],
    "A1000 NTSC": ["A1000"],
    "A1000 PAL": ["A1000"],
    "A1200": ["A1200"],
    "A1200 Apollo Gold2.12 'Main'": ["A1200"],
    "A1200 Apollo Gold2.13 'Main'": ["A1200"],
    "A1200_R2": ["A1200"],
    "A3000": ["A3000"],
    "A3000_R2": ["A3000"],
    "A4000": ["A4000"],
    "A4000T": ["A4000T"],
    "A4000T_R2": ["A4000T"],
    "A4000_R2": ["A4000"],
    "A500 Apollo Gold2.12 'Main'": ["A500"],
    "A500/2000": ["A500", "A2000"],
    "A500/2000 'Rekick'": ["A500", "A2000"],
    "A500/600/2000": ["A500", "A600", "A2000"],
    "A500/A2000/A1000": ["A500", "A2000", "A1000"],
    "A500/A600/A2000": ["A500", "A600", "A2000"],
    "A500/A600/A2000 Rekick": ["A500", "A600", "A2000"],
    "A500/A600/A2000_R2": ["A500", "A600", "A2000"],
    "A580 Apollo Gold2.12 'Main'": [],
    "A600": ["A600"],
    "A600 'Rekick'": ["A600"],
    "AmigaForever": [],
    "Amithlon": [],
    "Apollo Gold1 'Main'": [],
    "Apollo Gold2 'Main'": [],
    "Apollo Gold2.10 'Main'": [],
    "Apollo Gold2.5 'Main'": [],
    "Apollo Gold2.7 'Main'": [],
    "Apollo Silver5 'Main'": [],
    "Apollo Silver6 'Main'": [],
    "Apollo Silver7 'Main'": [],
    "CD32 Extended": ["CD32"],
    "CD32 Main": ["CD32"],
    "Cloanto": [],
    "Coffin R56": [],
    "Coffin R58": [],
    "Rekick": [],
    "Walker": [],
}

# `Kickstart 40.68 (A1200)` — the version is claimed only from this exact
# prefix. The database also holds names like `Kickstart 40.63(Apollo Gold2.12
# 'Main')`, where a looser search would read `2.12` as the version.
VERSION_PREFIX = re.compile(r"^Kickstart\s+(\d+)\.(\d+)")
PARENTHETICAL = re.compile(r"\(([^)]*)\)\s*$")


class Entry:
    def __init__(self, stored_checksum, size, name, major, minor, models):
        self.stored_checksum = stored_checksum
        self.size = size
        self.name = name
        self.major = major
        self.minor = minor
        self.models = models

    def key(self):
        return (self.stored_checksum, self.size, self.name, self.major, self.minor, tuple(self.models))


def read_database() -> tuple[list[Entry], list[str]]:
    """Every ROM in the shipped split data that ART can identify.

    Skipped, and reported rather than silently dropped: entries with no
    `sum_off` (`0xFFFFFFFF` — CD32 FMV modules and the like, which store no
    checksum of their own, so ART has nothing to read), and any parenthetical
    the map above has never seen.
    """
    data_dir = os.path.join(os.path.dirname(amitools.__file__), "data", "splitdata")
    entries: list[Entry] = []
    skipped: list[str] = []
    seen: dict[int, str] = {}

    for path in sorted(glob.glob(os.path.join(data_dir, "*.dat"))):
        split = RemusSplitFile()
        try:
            split.load(path)
        except Exception as err:  # a .dat this version cannot read is not fatal
            skipped.append(f"{os.path.basename(path)}: {err}")
            continue

        for rom in split.roms:
            if rom.sum_off == 0xFFFFFFFF:
                skipped.append(f"{rom.name}: stores no checksum of its own")
                continue
            if rom.sum_off != rom.size - 24:
                # Every entry in the shipped data puts it at size-24, which is
                # what lets ART read one value with one rule. A new layout is
                # worth stopping for rather than reading the wrong longword.
                sys.exit(
                    f"'{rom.name}' keeps its checksum at {rom.sum_off:#x}, not "
                    f"size-24 ({rom.size - 24:#x}). ART reads size-24; teach it "
                    "the other offset before adding this."
                )

            paren = PARENTHETICAL.search(rom.name)
            token = paren.group(1) if paren else ""
            if token not in MACHINES:
                sys.exit(
                    f"'{rom.name}' names '{token}', which this script has never "
                    "seen. Add it to MACHINES — with an empty list if it is not "
                    "a machine — rather than letting ART guess."
                )

            version = VERSION_PREFIX.match(rom.name)
            major = int(version.group(1)) if version else 0
            minor = int(version.group(2)) if version else 0

            if rom.chk_sum in seen:
                sys.exit(
                    f"two entries share the checksum {rom.chk_sum:#010x}: "
                    f"'{seen[rom.chk_sum]}' and '{rom.name}'. ART identifies by "
                    "that value, so it has to be unique."
                )
            seen[rom.chk_sum] = rom.name
            entries.append(
                Entry(rom.chk_sum, rom.size, rom.name, major, minor, MACHINES[token])
            )

    entries.sort(key=lambda e: (e.size, e.stored_checksum))
    return entries, skipped


def render(entries: list[Entry]) -> str:
    lines = [
        "// @generated by scripts/rom-table-check.py — do not edit by hand.",
        "//",
        "// Kickstart dumps, identified by the checksum each ROM stores about",
        "// itself (ART-104). Derived from the Remus split database shipped with",
        "// `amitools` (GPL-2.0-or-later, compatible with ART's own licence — see",
        "// THIRD_PARTY_LICENSES.md). Re-run the script to regenerate, and CI runs",
        "// it in verify mode so this file cannot drift from its source.",
        "//",
        "// `models` is empty when the database named something that is not a",
        "// machine — a distribution, an accelerator's own ROM. Empty means ART",
        "// claims no machine, never that it suits none.",
        "",
        "/// One catalogued dump.",
        "pub struct RemusRom {",
        "    /// The longword the ROM stores 24 bytes before its end. Unique",
        "    /// across the database, and the value that tells an A1200 build",
        "    /// from an A4000 one at the same revision.",
        "    pub stored_checksum: u32,",
        "    pub size: usize,",
        "    /// Verbatim from the database, so ART's answer can be traced to it.",
        "    pub name: &'static str,",
        "    /// `0` when the name states no Kickstart version (the CDTV extended",
        "    /// ROMs). ART then names the dump without claiming a version.",
        "    pub major: u16,",
        "    pub minor: u16,",
        "    pub models: &'static [&'static str],",
        "}",
        "",
        f"/// {len(entries)} dumps, ordered by size then checksum.",
        "pub const REMUS_ROMS: &[RemusRom] = &[",
    ]
    for entry in entries:
        models = ", ".join(f'"{m}"' for m in entry.models)
        lines.append("    RemusRom {")
        lines.append(f"        stored_checksum: 0x{entry.stored_checksum:08X},")
        lines.append(f"        size: {entry.size},")
        lines.append(f'        name: "{entry.name}",')
        lines.append(f"        major: {entry.major},")
        lines.append(f"        minor: {entry.minor},")
        lines.append(f"        models: &[{models}],")
        lines.append("    },")
    lines.append("];")
    lines.append("")
    return "\n".join(lines)


def scan(directory: str, entries: list[Entry]) -> int:
    """What ART would now say about every ROM in a folder. Reports, never writes."""
    table = {e.stored_checksum: e for e in entries}
    hit = miss = 0
    for name in sorted(os.listdir(directory)):
        path = os.path.join(directory, name)
        if not os.path.isfile(path):
            continue
        raw = open(path, "rb").read()
        if raw.startswith(b"AMIROMTYPE1"):
            raw = raw[11:]
        if len(raw) < 32:
            continue
        stored = struct.unpack_from(">I", raw, len(raw) - 24)[0]
        found = table.get(stored)
        if found:
            hit += 1
            machines = ", ".join(found.models) or "no machine claimed"
            print(f"  ok   {name}\n         -> {found.name}  [{machines}]")
        else:
            miss += 1
            print(f"  ?    {name}  (stored {stored:08x}, {len(raw)} bytes)")
    print(f"\n{hit} identified, {miss} not in the database")
    return hit


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--emit", action="store_true", help="rewrite the Rust table")
    parser.add_argument("--scan", metavar="DIR", help="report on real ROM files")
    args = parser.parse_args()

    entries, skipped = read_database()
    print(f"Remus split data: {len(entries)} identifiable dump(s), {len(skipped)} skipped")

    if args.scan:
        return 0 if scan(args.scan, entries) else 1

    rendered = render(entries)
    if args.emit:
        os.makedirs(os.path.dirname(GENERATED), exist_ok=True)
        with open(GENERATED, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(rendered)
        print(f"wrote {os.path.relpath(GENERATED, REPO)}")
        return 0

    if not os.path.exists(GENERATED):
        print(f"FAIL {os.path.relpath(GENERATED, REPO)} does not exist — run with --emit")
        return 1
    with open(GENERATED, encoding="utf-8", newline="") as handle:
        committed = handle.read().replace("\r\n", "\n")
    if committed != rendered:
        print(
            "FAIL the committed table no longer matches the database.\n"
            "     Re-run with --emit and read the diff before committing it: a\n"
            "     change here means amitools' data changed, which is worth\n"
            "     knowing rather than absorbing silently."
        )
        return 1
    print("ok   the committed table says exactly what the database says")
    return 0


if __name__ == "__main__":
    sys.exit(main())
