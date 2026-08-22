#!/usr/bin/env python3
r"""No stray control bytes in ART's own source and documentation.

CLAUDE.md's own warning, made into a build failure:

    Write paths through a file, never through a heredoc. A Windows path in a
    `<<'EOF'` block loses its backslash escapes -- `E:\amiga` arrives as `E:`
    plus a BEL byte.

Nothing fails when that happens. The text is simply wrong, and it stays wrong:
one instance was committed on 2026-08-12 and found ten days later only because
somebody swept for control bytes by hand. A second was found in
`core/card/build.rs` on 2026-08-23, again by hand, again already committed --
three BEL bytes and three eaten line breaks in a doc block telling the reader
how to run a real-hardware hook.

Twice by accident is the signal to stop finding it by accident.

    python scripts/control-byte-sweep.py

Exit 0 means every control byte in the tree is one somebody meant. Exit 1
lists the rest, with the offending line.

## The hard part, and how it is handled

**Some control bytes here are real data.** An AmigaDOS DosType *is* four bytes
with a small integer last -- `DOS\0`, `DOS\1`, `DOS\7`, `PDS\3`, `PFS\3` -- and
those appear inside string literals and prose throughout the codebase. A sweep
that flagged them would cry wolf until somebody switched it off, and a sweep
that allowed 0x00-0x07 everywhere would have missed the BEL this script was
written for, because BEL *is* 0x07.

So the allow-list is **per file, with a reason**, exactly like
`scripts/scratch-root-sweep.py`. Adding a file to it is a decision somebody
writes down, not a switch.
"""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

SKIP_DIRS = {".git", "node_modules", "target", "dist", ".vite", "coverage"}
TEXT_SUFFIXES = {
    ".rs", ".ts", ".tsx", ".js", ".md", ".json", ".toml",
    ".yml", ".yaml", ".py", ".css", ".html",
}

# Files whose control bytes are AmigaDOS DosType data, not corruption. Each
# entry says what the bytes are, so the next reader can check rather than trust.
ALLOWED = {
    "docs/ISSUES.md":
        r"`DOS\3` / `PDS\3` DosTypes quoted from two real PiStorm cards, and a "
        r"`pri=1146049281` reading whose ASCII is `DOS\1`",
    "src/lib/rdbDrivers.ts":
        r"`DOS\0` through `DOS\7` -- the Old and Fast File System DosTypes",
    "src-tauri/src/commands/amigainstall.rs":
        r"a BoingBag fixture's own archive member names carrying `\3`/`\4`",
    "src-tauri/src/core/dirsize.rs":
        r"`DosType::new(*b\"DOS\\1\")` in two FFS fixtures",
    "src-tauri/src/core/mbr.rs":
        r"an `hst.imager rdb info` transcript quoting a `\1` DosType",
    "src-tauri/src/core/rdb.rs":
        r"`PDS\3` in the comment explaining what G4 makes mountable",
    "src-tauri/src/core/preload/mod.rs":
        r"`PFS\3` and `PDS\3` -- the DosTypes the preload path formats",
}

# These are never data here. BEL is what a heredoc turns `\a` into; the others
# are the same accident with a different letter (`\b`, `\v`, `\f`, `\e`).
NEVER_DATA = {0x07: "BEL (a heredoc ate a `\\a`)", 0x08: "BS (`\\b`)",
              0x0B: "VT (`\\v`)", 0x0C: "FF (`\\f`)", 0x1B: "ESC (`\\e`)"}


def offenders() -> list[tuple[str, int, int, str]]:
    found: list[tuple[str, int, int, str]] = []
    for path in sorted(ROOT.rglob("*")):
        if not path.is_file() or path.suffix not in TEXT_SUFFIXES:
            continue
        rel = path.relative_to(ROOT).as_posix()
        if any(part in SKIP_DIRS for part in path.relative_to(ROOT).parts):
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        allowed = rel in ALLOWED
        for i, ch in enumerate(text):
            code = ord(ch)
            if code >= 32 or ch in "\n\t":
                continue
            # An allow-listed file still may not carry a byte that is never
            # data: `DOS\7` is real, but BEL in a *path* is the whole point of
            # this script, and one file being allowed DosTypes must not make
            # it a blind spot.
            if allowed and code not in NEVER_DATA:
                continue
            if allowed and code in NEVER_DATA:
                # 0x07 is genuinely ambiguous: it is BEL and it is `DOS\7`.
                # In an allow-listed file it is treated as data, because that
                # is what the entry attests. Any other never-data byte is not.
                if code == 0x07:
                    continue
            line_no = text.count("\n", 0, i) + 1
            line = text.splitlines()[line_no - 1] if text.splitlines() else ""
            found.append((rel, line_no, code, line.strip()[:120]))
    return found


def main() -> int:
    bad = offenders()
    if not bad:
        print(
            f"control-byte sweep: clean — {len(ALLOWED)} file(s) allow-listed for "
            "AmigaDOS DosType data, no stray control bytes anywhere else"
        )
        return 0

    print("control-byte sweep: stray control bytes in tracked text\n")
    for rel, line_no, code, line in bad:
        what = NEVER_DATA.get(code, f"0x{code:02x}")
        print(f"  {rel}:{line_no}  <{what}>  {line}")
    print(
        "\nAlmost always a Windows path written through a heredoc: `E:\\amiga` becomes\n"
        "`E:` + BEL + `miga`, and nothing fails. Rewrite the file with the Write tool\n"
        "or the editing tools. If the byte really is AmigaDOS data, add the file to\n"
        "ALLOWED in this script with the reason."
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
