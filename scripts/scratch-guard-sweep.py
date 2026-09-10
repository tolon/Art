#!/usr/bin/env python3
"""Every test scratch helper hands out its guard, and every call site keeps it
(ART-281).

`core::ScratchDir` removes its directory on `Drop`, which is the only cleanup
that also runs when a test panics — and a red suite is exactly when leaking
hurts most. But 63 test modules never used it: each carries its own one-line

    fn scratch(tag: &str) -> PathBuf { … create_dir_all(&dir) … dir }

which creates the directory and hands back a **bare path**, so nothing owns it
and nothing removes it. A scratch name is unique per run, so nothing removes a
previous run's either. Those two together put 169,291 directories (~987 GB)
into `%TEMP%` in one session and filled a 2 TB system drive (ART-184), and
after the root was moved to `D:\\tmp\\art-tests` the same leak simply piled up
there instead — 763 GB by 2026-09-10.

`ScratchDir::pair` makes the fix a one-line change per helper:

    fn scratch(tag: &str) -> (crate::core::ScratchDir, PathBuf) {
        crate::core::ScratchDir::pair("art-thing", tag)
    }

and one word per call site: `let dir = scratch("x")` becomes
`let (_guard, dir) = scratch("x")`, leaving `dir` the same `PathBuf` the test
body already used.

A fix like that is only as good as the sixty-fourth helper nobody converts.
This sweep is what makes that a build failure instead of a discovery, the same
way `scratch-root-sweep.py` and `scratch-counter-sweep.py` do for their own
defects.

## What counts as an offender

Only **test** code. For most files that means everything before the file's
first `#[cfg(test)]` is production and is never examined — but a module can
be gated where it is *declared* instead: `core/osinstall/source_contract.rs`
carries no `#[cfg(test)]` at all, because `core/osinstall/mod.rs` says
`#[cfg(test)] mod source_contract;`. The first version of this sweep cut on
the file's own attribute alone and skipped that file whole, three unguarded
call sites and ~90 leaked directories with it. So a file with no attribute of
its own is checked against its parent module's declaration, and when the
declaration is gated the whole file counts as test code. Three rules:

  1. **returns a bare path** — a `fn scratch(` whose return type on the
     signature line is `PathBuf` / `std::path::PathBuf` / `&Path` and does not
     mention `ScratchDir`. Both accepted shapes name the guard in the
     signature: `-> ScratchDir` (`core/rom/place.rs`) and
     `-> (ScratchDir, PathBuf)` (`core/whdload/install.rs`, ART-242).
  2. **bound without its guard** — a call to `scratch(` (bare, or qualified:
     `fixtures::scratch(`, `super::scratch(`, `crate::…::scratch(`) whose
     statement is `let <ident> = …` rather than `let (<ident>, <ident>) = …`.
     `let (_, dir) = …` is worse than either and is called out separately:
     a bare `_` drops the guard at once, deleting the directory before the
     test's first line, which is the leak back in one character wearing the
     fix's clothes.

     This rule reads the helper first, because the shape a call site must have
     is the shape its helper returns. A helper that returns a plain
     `ScratchDir` (`core/rom/place.rs`, `core/osinstall/chain.rs`,
     `core/amigainstall/{packagevol,run,workvol}.rs`) is *already* correct with
     `let dir = scratch("x")` — `dir` is the guard — and demanding a tuple
     there would be this sweep inventing a defect. Only a helper that returns
     a tuple or a bare path makes `let <ident> =` wrong. The helper is looked
     up the way Rust would: the calling file's own `fn scratch`, else the
     directory's `mod.rs` (`super::`, and every `fixtures::scratch` call under
     `core/osinstall/`), else — nothing found — the strict reading.
  3. **returns a path whose guard it drops** — a helper `fn` that is not a
     `#[test]`, calls `scratch(`, and returns a `PathBuf`/`Path`-shaped type
     with no `ScratchDir` in it. The guard it creates dies at that helper's
     closing brace while the path it returns is used by the caller — so the
     directory is both leaked *and*, once the helpers are converted, gone
     early. These need the guard threaded out to the caller.

## What this sweep cannot see

Rule 3 keys on the *shape* of the return type. A helper that returns an
`InstallPlan` or a `Box<dyn MediaSource>` holding a path built under a scratch
is the same defect and is invisible here — a name cannot be typed. Those come
out of rule 2 instead: the `let` inside them is listed, and threading the
guard out is part of fixing the call site. This is stated rather than
silently assumed: a guard that passes vacuously is the class of defect the
counter sweep already shipped once.

Run it from `amiga-retro-toolkit/`:

    python scripts/scratch-guard-sweep.py

Exit 0 with `scratch-guard sweep: clean — N helpers, M call sites`. Exit 1
lists every offender as `path:line: <why>` and prints the totals.

Deliberately a script and not a Rust test, matching the two sweeps beside it:
a test that reads the source of the crate it is compiled into is a strange
thing, and this is a lint.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "src-tauri" / "src"

# A call to a function *named* `scratch`, bare or path-qualified. The
# look-behind rejects `download_from_scratch(` and `build_plan_with_scratch(`
# while allowing `fixtures::scratch(` — `:` is not a word character.
CALL = re.compile(r"(?<![A-Za-z0-9_])scratch\(")
FN_RE = re.compile(r"^(\s*)(pub(\([^)]*\))? )?(async )?fn (\w+)")
SCRATCH_DEF = re.compile(r"^\s*(pub(\([^)]*\))? )?(async )?fn scratch\(")
# `let (a, b) = …` — two plain identifiers, `_guard` included. The first
# element is read back out, because `let (_ , dir) = …` — a space before the
# comma — is `let (_, dir)` in rustfmt's clothes and must not pass as a tuple.
TUPLE_LET = re.compile(r"^let\s*\(\s*(\w+)\s*,\s*(\w+)\s*\)\s*=")
IDENT_LET = re.compile(r"^let\s+(?:mut\s+)?(\w+)\s*=")
# A return type that is a path and nothing else: `-> PathBuf`,
# `-> std::path::PathBuf`, `-> (PathBuf, PathBuf)`, `-> &Path`.
PATH_SHAPED = re.compile(r"\bPathBuf\b|\bPath\b")


def rust_files() -> list[Path]:
    return sorted(ROOT.rglob("*.rs"))


def helper_shapes(path: Path, lines: list[str]) -> str | None:
    """What this file's own `fn scratch` helpers hand back: "guard" (a bare
    `ScratchDir`), "pair" (`(ScratchDir, PathBuf)`), "path" (no guard at all),
    or `None` when the file defines none. Helpers that disagree read as
    "path" — the strict answer, which is what a half-converted file deserves."""
    shapes = set()
    for i, line in enumerate(lines):
        if not SCRATCH_DEF.match(line):
            continue
        sig = signature(lines, i)
        ret = sig.split("->", 1)[1] if "->" in sig else ""
        if "ScratchDir" not in ret:
            shapes.add("path")
        elif "(" in ret.split("{")[0]:
            shapes.add("pair")
        else:
            shapes.add("guard")
    if not shapes:
        return None
    return shapes.pop() if len(shapes) == 1 else "path"


def first_cfg_test(lines: list[str]) -> int | None:
    for i, line in enumerate(lines):
        if line.strip() == "#[cfg(test)]":
            return i
    return None


def gated_at_its_declaration(path: Path) -> bool:
    """A file with no `#[cfg(test)]` of its own can still be test code from its
    first line: `core/osinstall/source_contract.rs` is declared

        #[cfg(test)]
        mod source_contract;

    in `core/osinstall/mod.rs`. The first version of this sweep cut on the
    file's own attribute alone and therefore skipped that module whole — three
    unguarded call sites, ~90 leaked `art-osinstall-contract-*` directories,
    and a sweep that would have certified the file converted without ever
    reading it. That is the vacuous guard this script's own header warns
    about, one level up.

    So: look in the parent module for the declaration. `mod.rs` first, then
    the 2018-style sibling (`core/osinstall.rs` for `core/osinstall/…`), then
    `lib.rs` / `main.rs` for a file sitting directly in `src/`."""
    stem = path.stem
    if stem in ("mod", "lib", "main"):
        return False
    parents = [path.parent / "mod.rs", path.parent.with_suffix(".rs")]
    if path.parent == ROOT:
        parents += [ROOT / "lib.rs", ROOT / "main.rs"]
    decl = re.compile(
        r"#\[cfg\(test\)\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+" + re.escape(stem) + r"\s*;"
    )
    for parent in parents:
        if parent == path or not parent.is_file():
            continue
        if decl.search(parent.read_text(encoding="utf-8")):
            return True
    return False


def is_comment(line: str, col: int) -> bool:
    """The match at `col` sits inside a `//` comment on its own line.

    Deliberately crude, and its two blind spots are named rather than left to
    be found: a `//` inside a *string literal* earlier on the same line
    suppresses a real offender, and `/* … */` blocks are not handled at all.
    Neither shape exists anywhere in the tree today; both would make the sweep
    under-report, so if one ever appears it is a missing line, not a false
    accusation."""
    head = line[:col]
    return "//" in head or line.strip().startswith("//")


def signature(lines: list[str], start: int) -> str:
    """The `fn` signature from `start` up to its opening brace."""
    out = []
    for i in range(start, min(start + 12, len(lines))):
        out.append(lines[i])
        if "{" in lines[i]:
            break
    return " ".join(out)


def statement_start(lines: list[str], index: int) -> int:
    """The first line of the statement containing line `index`. Walks back
    while the previous line looks like a continuation — it does not end a
    statement or a block."""
    i = index
    while i > 0:
        prev = lines[i - 1].rstrip()
        if prev == "" or prev.endswith((";", "{", "}", ",")) or prev.lstrip().startswith("//"):
            break
        i -= 1
    return i


def enclosing_fn(lines: list[str], index: int) -> tuple[str, int]:
    """(name, line) of the nearest `fn` at or above `index`."""
    i = index
    while i > 0 and not FN_RE.match(lines[i]):
        i -= 1
    m = FN_RE.match(lines[i])
    return (m.group(5) if m else "<file>"), i


def is_test_fn(lines: list[str], fn_line: int) -> bool:
    """A `#[test]` / `#[tokio::test]` attribute above the signature."""
    i = fn_line - 1
    while i >= 0:
        s = lines[i].strip()
        if s.startswith("#["):
            if s.startswith("#[test]") or s.startswith("#[tokio::test"):
                return True
            i -= 1
            continue
        if s.startswith("//"):
            i -= 1
            continue
        return False
    return False


def main() -> int:
    offenders: list[tuple[str, int, str]] = []
    helpers = guarded_helpers = 0
    calls = accepted_calls = 0
    rule_counts = {"returns a bare path": 0, "bound without its guard": 0,
                   "guard dropped at once": 0, "used without binding its guard": 0,
                   "returns a path whose guard it drops": 0}
    rule3_seen: set[tuple[str, int]] = set()

    files = rust_files()
    sources = {p: p.read_text(encoding="utf-8").split("\n") for p in files}
    shape_of = {p: helper_shapes(p, sources[p]) for p in files}

    def resolved_shape(path: Path) -> str:
        """The helper a call in `path` reaches: its own file's, else its
        directory module's (`super::…`, `fixtures::…`), else the strict
        reading."""
        own = shape_of.get(path)
        if own is not None:
            return own
        parent = path.parent / "mod.rs"
        return shape_of.get(parent) or "path"

    for path in files:
        rel = path.relative_to(ROOT.parent.parent).as_posix()
        lines = sources[path]
        cut = first_cfg_test(lines)
        if cut is None:
            # No attribute of its own — but the module may be gated where it
            # is declared, in which case the whole file is test code.
            if not gated_at_its_declaration(path):
                continue
            cut = 0
        shape = resolved_shape(path)

        for i in range(cut, len(lines)):
            line = lines[i]
            m = CALL.search(line)
            if not m or is_comment(line, m.start()):
                continue

            # Rule 1: the definition itself.
            if SCRATCH_DEF.match(line):
                helpers += 1
                sig = signature(lines, i)
                ret = sig.split("->", 1)[1] if "->" in sig else ""
                if "ScratchDir" in ret:
                    guarded_helpers += 1
                elif PATH_SHAPED.search(ret):
                    offenders.append((rel, i + 1, "returns a bare path"))
                    rule_counts["returns a bare path"] += 1
                continue
            if FN_RE.match(line):
                # Some other `fn` whose signature mentions `scratch(` — not a
                # call site and not the helper.
                continue

            fn_name, fn_line = enclosing_fn(lines, i)
            # The body of a `fn scratch` is where a scratch is legitimately
            # created; rule 1 has already judged that helper by its signature.
            if fn_name == "scratch":
                continue

            # Rule 2: how the call is bound — judged against what the helper
            # it reaches actually returns.
            calls += 1
            stmt = lines[statement_start(lines, i)].strip()
            tuple_let = TUPLE_LET.match(stmt)
            if tuple_let and tuple_let.group(1) == "_":
                # Read out of the match, not off the raw prefix, so
                # `let (_ , dir) = …` cannot slip through on one space.
                offenders.append((rel, i + 1, "guard dropped at once"))
                rule_counts["guard dropped at once"] += 1
            elif tuple_let:
                accepted_calls += 1
            elif shape == "guard" and IDENT_LET.match(stmt):
                # `let dir = scratch("x")` where `dir` *is* the guard.
                accepted_calls += 1
            elif IDENT_LET.match(stmt):
                offenders.append((rel, i + 1, "bound without its guard"))
                rule_counts["bound without its guard"] += 1
            else:
                offenders.append((rel, i + 1, "used without binding its guard"))
                rule_counts["used without binding its guard"] += 1

            # Rule 3: the helper that returns the path and drops the guard.
            if (rel, fn_line) in rule3_seen or is_test_fn(lines, fn_line):
                continue
            rule3_seen.add((rel, fn_line))
            sig = signature(lines, fn_line)
            ret = sig.split("->", 1)[1] if "->" in sig else ""
            if PATH_SHAPED.search(ret) and "ScratchDir" not in ret:
                offenders.append(
                    (rel, fn_line + 1,
                     f"returns a path whose guard it drops (fn {fn_name})"))
                rule_counts["returns a path whose guard it drops"] += 1

    for rel, line, why in sorted(offenders):
        print(f"{rel}:{line}: {why}")

    if not offenders:
        print(f"scratch-guard sweep: clean — {helpers} helpers, {calls} call sites")
        return 0

    print()
    print(f"helpers                            : {helpers}"
          f" ({guarded_helpers} already hand out a guard)")
    print(f"call sites                         : {calls}"
          f" ({accepted_calls} keep their guard)")
    for why, n in rule_counts.items():
        if n:
            print(f"  {why:33s}: {n}")
    print(f"offenders                          : {len(offenders)}")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
