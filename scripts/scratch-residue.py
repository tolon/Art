#!/usr/bin/env python3
"""Count what the Rust test suite left behind in the scratch root (ART-281).

This is the measuring instrument for the experiment, and nothing else: it
**counts**, it never deletes, and it never recurses. One number, one line,
exit 0 — so it can be run before a suite and after it and the difference
attributed to that suite.

Why it exists rather than `ls D:\\tmp\\art-tests | wc -l`: a pipeline's exit
code is the *last* command's, so a failing `ls` reads as a clean zero and the
defective arm of the experiment silently becomes the fixed one. This project
has been burned by exactly that (CLAUDE.md, "Never pipe a command that can
fail"). A script that prints its own root and its own count cannot lie about
which directory it looked in.

The root is read from `src-tauri/.cargo/config.toml` — the same file Cargo
reads to point `TMP` off the system drive — so moving the root moves this
counter with it and no second copy of the path drifts. If that file cannot be
parsed the fall-back is the value it holds today, `D:/tmp/art-tests`, and the
line says *why* it fell back — a config that could not be read and a config
with no `TMP` line are different problems with different fixes, and one
sentence for both would send someone to repair a file that is fine.

Usage:  python scripts/scratch-residue.py          # entries under <root>: N
        python scripts/scratch-residue.py --json    # {"root": …, "entries": N}
"""

from __future__ import annotations

import json
import os
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CONFIG = REPO / "src-tauri" / ".cargo" / "config.toml"
FALLBACK = "D:/tmp/art-tests"

# `TMP = { value = "D:/tmp/art-tests", force = true }`
TMP_RE = re.compile(r'^\s*TMP\s*=\s*\{\s*value\s*=\s*"([^"]+)"')


def scratch_root() -> tuple[str, str]:
    """(root, where it came from).

    Three endings, three sentences: the config named the root ("config"), the
    config was there but held no `TMP = { value = "…" }` line ("unparsed" — a
    reformatted file, or a plain-string `TMP = "…"`), or the config could not
    be read at all ("unreadable"). Collapsing the last two into one message
    would tell someone to go fix a file that is fine."""
    try:
        text = CONFIG.read_text(encoding="utf-8")
    except OSError:
        return FALLBACK, "unreadable"
    for line in text.splitlines():
        m = TMP_RE.match(line)
        if m:
            return m.group(1), "config"
    return FALLBACK, "unparsed"


def count_entries(root: str) -> int:
    """Direct children of `root`. Not recursive: one scratch directory is one
    entry however many files a test put inside it, and recursing would make
    the number depend on the fixtures rather than on the leak."""
    try:
        with os.scandir(root) as it:
            return sum(1 for _ in it)
    except FileNotFoundError:
        return 0
    except OSError as err:
        print(f"scratch-residue: cannot read {root}: {err}", file=sys.stderr)
        return -1


def main() -> int:
    root, source = scratch_root()
    entries = count_entries(root)
    if "--json" in sys.argv:
        print(json.dumps({"root": root, "entries": entries, "source": source}))
    else:
        suffix = {
            "config": "",
            "unparsed": f" (fallback: no TMP line found in {CONFIG.name})",
            "unreadable": f" (fallback: {CONFIG.name} could not be read)",
        }[source]
        print(f"entries under {root}: {entries}{suffix}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
