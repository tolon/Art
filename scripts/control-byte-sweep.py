#!/usr/bin/env python3
r"""No corrupted text in ART's own source and documentation.

Two shapes of the same accident, both found by hand before anything found them,
and both already committed when they were found.

**One: a control byte.** CLAUDE.md's own warning ---

    Write paths through a file, never through a heredoc. A Windows path in a
    `<<'EOF'` block loses its backslash escapes -- `E:\amiga` arrives as `E:`
    plus a BEL byte.

Nothing fails when that happens. The text is simply wrong and stays wrong: one
instance was committed on 2026-08-12 and found ten days later only because
somebody swept by hand; a second was found in `core/card/build.rs` on
2026-08-23, in a doc block telling the reader how to run a real-hardware hook.

**Two: a lost line continuation.** A Rust string that wraps ends its line with
`\`, and the compiler eats the newline *and* the next line's indentation. Lose
the `\` and the indentation stays, in the middle of the sentence the user
reads. Found on 2026-08-23 in `core/amigainstall/packagevol.rs` -- a refusal a
real person could meet -- and then in eleven more places, one of them the
refusal that appears most often in the owner's own operation log.

    python scripts/control-byte-sweep.py

Exit 0 means every control byte in the tree is one somebody meant and no string
carries a run of spaces nobody typed. Exit 1 lists the rest.

## The hard part, and how it is handled

**Some of both are real.** An AmigaDOS DosType *is* four bytes with a small
integer last -- `DOS\0`, `DOS\7`, `PDS\3`, `PFS\3` -- and a fixture reproducing
Aminet's fixed-width INDEX really does line its columns up with spaces. A sweep
that flagged those would be switched off within a week.

So both allow-lists are **per file, with a reason**, exactly like
`scripts/scratch-root-sweep.py`. Adding a file is a decision somebody writes
down, not a switch. An allow-listed file is still not a blind spot for a byte
that is never data.
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

# Files whose control bytes are AmigaDOS DosType data, not corruption.
ALLOWED = {
    "docs/ISSUES.md":
        r"`DOS\3` / `PDS\3` DosTypes quoted from two real PiStorm cards, and a "
        r"`pri=1146049281` reading whose ASCII is `DOS\1`",
    "src/lib/rdbDrivers.ts":
        r"`DOS\0` through `DOS\7` -- the Old and Fast File System DosTypes",
    "src-tauri/src/commands/amigainstall.rs":
        r"a BoingBag fixture's own archive member names carrying `\3`/`\4`",
    "src-tauri/src/core/dirsize.rs":
        r"`DosType::new(*b\"DOS\1\")` in two FFS fixtures",
    "src-tauri/src/core/mbr.rs":
        r"an `hst.imager rdb info` transcript quoting a `\1` DosType",
    "src-tauri/src/core/rdb.rs":
        r"`PDS\3` in the comment explaining what G4 makes mountable",
    "src-tauri/src/core/preload/mod.rs":
        r"`PFS\3` and `PDS\3` -- the DosTypes the preload path formats",
}

# Never data here. BEL is what a heredoc turns `\a` into; the rest are the same
# accident with a different letter (`\b`, `\v`, `\f`, `\e`).
NEVER_DATA = {
    0x07: r"BEL (a heredoc ate a `\a`)",
    0x08: r"BS (`\b`)",
    0x0B: r"VT (`\v`)",
    0x0C: r"FF (`\f`)",
    0x1B: r"ESC (`\e`)",
}

# A run of this many spaces *inside a Rust string literal* is a `\` line
# continuation that went missing. Three would catch deliberate alignment; the
# real thing leaves a run as long as the indentation, which is never small.
GAP_RUN = 6

# Files whose string literals line text up on purpose.
GAP_ALLOWED = {
    "src-tauri/src/core/sources/index.rs":
        "a fixture reproducing Aminet's own fixed-width INDEX columns",
    "src-tauri/src/core/sources/readme.rs":
        "a fixture reproducing a readme's own wrapped, indented field",
    "src-tauri/src/tools/hst_imager.rs":
        "a fixture reproducing hst-imager's own table output",
    "src-tauri/src/commands/osinstall.rs":
        "a debug `println!` that lines its own labels up",
    "src-tauri/src/core/sources/catalog/mod.rs":
        "a fixture list whose entries carry aligned trailing size comments",
    "src-tauri/src/core/osinstall/mod.rs":
        "a fixture whose protection bits are noted in aligned trailing comments",
}


def control_offenders() -> list[tuple[str, int, int, str]]:
    found: list[tuple[str, int, int, str]] = []
    for path in sorted(ROOT.rglob("*")):
        if not path.is_file() or path.suffix not in TEXT_SUFFIXES:
            continue
        parts = path.relative_to(ROOT).parts
        if any(part in SKIP_DIRS for part in parts):
            continue
        rel = path.relative_to(ROOT).as_posix()
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        allowed = rel in ALLOWED
        lines = text.splitlines()
        for i, ch in enumerate(text):
            code = ord(ch)
            if code >= 32 or ch in "\n\t":
                continue
            # An allow-listed file attests to DosType data. 0x07 is genuinely
            # ambiguous -- it is BEL and it is `DOS\7` -- so there it is taken
            # as data. Every other never-data byte is still reported, so the
            # entry does not turn the file into a blind spot.
            if allowed and (code not in NEVER_DATA or code == 0x07):
                continue
            line_no = text.count("\n", 0, i) + 1
            line = lines[line_no - 1] if line_no <= len(lines) else ""
            found.append((rel, line_no, code, line.strip()[:120]))
    return found


def string_literals(line: str) -> list[str]:
    """The contents of each `"..."` on the line, quotes paired properly.

    Written rather than regexed because the naive pattern matched from a
    *closing* quote and reported the comment after it -- which is how six of
    its first nineteen findings were things nobody had written.
    """
    out: list[str] = []
    i, n = 0, len(line)
    while i < n:
        if line[i] != '"':
            i += 1
            continue
        j = i + 1
        buf: list[str] = []
        while j < n:
            if line[j] == "\\":
                buf.append(line[j:j + 2])
                j += 2
                continue
            if line[j] == '"':
                break
            buf.append(line[j])
            j += 1
        if j < n:
            out.append("".join(buf))
        i = j + 1
    return out


def gap_offenders() -> list[tuple[str, int, str]]:
    run = " " * GAP_RUN
    found: list[tuple[str, int, str]] = []
    for path in sorted(ROOT.rglob("*.rs")):
        parts = path.relative_to(ROOT).parts
        if any(part in SKIP_DIRS for part in parts):
            continue
        rel = path.relative_to(ROOT).as_posix()
        if rel in GAP_ALLOWED:
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        for i, line in enumerate(text.split("\n")):
            stripped = line.strip()
            if stripped.startswith(("//", "/*", "*")):
                continue
            for literal in string_literals(line):
                if run in literal.strip():
                    found.append((rel, i + 1, stripped[:120]))
                    break
    return found


def main() -> int:
    bad = control_offenders()
    gaps = gap_offenders()

    if not bad and not gaps:
        print(
            "control-byte sweep: clean - %d file(s) allow-listed for AmigaDOS DosType "
            "data and %d for deliberate alignment; no stray control bytes and no lost "
            "line continuations anywhere else" % (len(ALLOWED), len(GAP_ALLOWED))
        )
        return 0

    if bad:
        print("control-byte sweep: stray control bytes in tracked text\n")
        for rel, line_no, code, line in bad:
            what = NEVER_DATA.get(code, "0x%02x" % code)
            print("  %s:%d  <%s>  %s" % (rel, line_no, what, line))
        print(
            "\nAlmost always a Windows path written through a heredoc: `E:\\amiga` becomes\n"
            "`E:` + BEL + `miga`, and nothing fails. Rewrite the file with the Write tool\n"
            "or the editing tools. If the byte really is AmigaDOS data, add the file to\n"
            "ALLOWED in this script with the reason."
        )

    if gaps:
        if bad:
            print()
        print("control-byte sweep: Rust strings carrying a run of spaces nobody typed\n")
        for rel, line_no, line in gaps:
            print("  %s:%d  %s" % (rel, line_no, line))
        print(
            "\nA wrapped Rust string keeps a `\\` at the end of the line; without it the\n"
            "next line's indentation lands in the middle of the sentence the user reads.\n"
            "If the alignment is deliberate, add the file to GAP_ALLOWED with the reason."
        )

    return 1


if __name__ == "__main__":
    sys.exit(main())
