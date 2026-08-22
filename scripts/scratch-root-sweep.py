#!/usr/bin/env python3
"""Every place ART stages work goes through the chosen scratch root (ART-196).

The defect this closes was not one call site, it was *eighteen*: everything
ART unpacked, staged or built on the way to somewhere else went through
`std::env::temp_dir()`, which on Windows is `%TEMP%` on the **system drive**.
The owner's standing rule is that ART writes nothing to `C:`, and the shipped
product could not honour it.

A fix like that is only as good as the nineteenth call site nobody adds. This
sweep is what makes that a build failure instead of a discovery: it reads the
crate's own source, ignores everything inside a `#[cfg(test)]` region, and
fails on any `std::env::temp_dir()` outside the short, named allow-list below.

Run it from `amiga-retro-toolkit/`:

    python scripts/scratch-root-sweep.py

Exit code 0 means every production staging site goes through
`crate::scratch::root()`. Exit code 1 lists the ones that do not.

Deliberately a script and not a Rust test, matching
`scripts/scratch-counter-sweep.py`: a test that reads the source of the crate
it is compiled into is a strange thing, and this is a lint.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "src-tauri" / "src"

# Each entry says *why* it is allowed. A path with no reason is not an
# exception, it is an omission.
ALLOWED = {
    # `crate::scratch` is the module that answers the question, so it is the
    # one place allowed to ask the platform.
    "scratch.rs": "the module that decides the root",
    # `commands/scratch.rs` reports what "the default" means on this machine
    # so the Settings screen can show it rather than describe it (ART-214's
    # lesson).
    "commands/scratch.rs": "reports the default so the screen can show it",
    # Tauri's own per-app directories — the operation log, the catalogue, the
    # artwork library, the checkout cache. These are **kept** data, not
    # scratch, and the `temp_dir()` here is the last-resort fallback for a
    # machine where the app directory cannot be resolved at all: a missing
    # history is a far smaller problem than refusing to start.
    "lib.rs": "app_log_dir / app_data_dir / app_cache_dir fallback, not scratch",
    "commands/artwork.rs": "app_data_dir fallback for the artwork library",
    "commands/gameindex.rs": "app_data_dir fallback for the catalogue",
    # The thin wrappers. Each has an explicit `*_staging_in` counterpart that
    # the product calls; the bare name exists for tests and for a future CLI
    # shell that has not chosen a directory. Same split as
    # `plan` / `plan_with_cache`.
    "core/osinstall/apply.rs": "thin wrapper over apply_staging_in / add_package_staging_in",
    "core/osinstall/plan.rs": "thin wrapper over plan_with_cache_in",
    "core/osinstall/scan.rs": "thin wrapper over open_package_staging_in",
}

CALL = re.compile(r"(?<![\w:])std::env::temp_dir\s*\(")
TEST_REGION = re.compile(r"^#\[cfg\(test\)\]", re.M)
# A line that only talks about it.
COMMENT = re.compile(r"^\s*(//|/\*|\*)")


def offenders() -> list[tuple[str, int, str]]:
    found: list[tuple[str, int, str]] = []
    for path in sorted(ROOT.rglob("*.rs")):
        rel = path.relative_to(ROOT).as_posix()
        if rel in ALLOWED:
            continue
        text = path.read_text(encoding="utf-8")
        first_test = TEST_REGION.search(text)
        cut = text[: first_test.start()].count("\n") if first_test else None
        for i, line in enumerate(text.split("\n")):
            if cut is not None and i >= cut:
                break
            if COMMENT.match(line):
                continue
            if CALL.search(line):
                found.append((rel, i + 1, line.strip()))
    return found


def main() -> int:
    bad = offenders()
    if not bad:
        print(
            f"scratch-root sweep: clean — {len(ALLOWED)} named exception(s), "
            "every other production staging site goes through crate::scratch::root()"
        )
        return 0

    print("scratch-root sweep: production code reaching std::env::temp_dir() directly\n")
    for rel, line_no, text in bad:
        print(f"  {rel}:{line_no}  {text}")
    print(
        "\nStage under `crate::scratch::root()?` instead (a `core/` function takes the\n"
        "directory from its caller — see src-tauri/src/scratch.rs). If the path is\n"
        "genuinely kept data rather than scratch, add it to ALLOWED in this script\n"
        "with the reason."
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
