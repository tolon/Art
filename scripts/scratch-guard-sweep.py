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
declaration is gated the whole file counts as test code. Five rules:

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
  4. **builds a scratch path by hand** — test code that writes
     `std::env::temp_dir().join(…)` itself instead of asking `ScratchDir`.
     The 63 helpers rules 1-3 cover were the dominant shape, not the only
     one: ~48 more sites in test bodies and in helpers *not* named `scratch`
     built the same `art-…-{test_scratch_id()}` path inline, created it, and
     left it (ART-281 task 7a). Every one becomes
     `let (_guard, dir) = ScratchDir::pair("art-…", "<tag>")`.

     Three kinds of line are **not** offenders, because none of them creates
     anything. First, a use that only reads — and "only reads" is decided by
     *where the match sits*, not by what the line mentions: the occurrence
     must be inside the parentheses of an `assert!` / `assert_eq!` /
     `assert_ne!` or of a `read_dir(`, or the joined path must be asked
     `.exists()` / `.is_dir()` / `.is_file()` on that same line with nothing
     on it that creates. The first version tested the whole line for the
     substring `assert`, which let `let dir = temp_dir().join("art-probe-
     assertive")` through on its literal alone (fix round 1, review I2).
     Second, a line inside `ScratchDir`'s own `impl` — where the one
     legitimate `temp_dir().join(` in the crate lives. Third, the two sites
     in `ALLOWED_HAND_BUILT` below — `art-measure-dest` and
     `art-library-does-not-exist` — each of which names a path the test
     deliberately never creates. Those two are listed by file and by the
     literal that identifies them rather than by line number, so an edit that
     changes what the site does stops matching the exemption and comes back
     as an offender — and `--self-test` fails if this paragraph and that
     table stop agreeing.
  5. **hands the platform root to product code** — test code passing
     `&std::env::temp_dir()` as a call argument, which is the *other* half of
     ART-281 (task 7b): ART's own staging then lands directly in the platform
     root under the product's names, and nothing in a test run ever sweeps
     it. Each such test binds one `ScratchDir` and passes `&root` instead.

     Exempt: **one** thing only — an occurrence inside the parentheses of
     `scratch_root_for(` or `sweep_stale_preview_scratch_dirs(`, where the
     platform root **is** the subject of the test (a namespace derived from
     it, or the sweeper that cleans it) and substituting a scratch would
     delete the assertion rather than move it. Rule 4's read-only exemption
     deliberately does **not** carry over: `assert!(build_plan(&std::env::
     temp_dir(), …).is_ok())` hands the platform root to product code exactly
     like the un-asserted call does, and the first version of this rule let
     that whole shape through (fix round 1, review I2).

## Which lines count as test code

Rules 1-3 key on `fn scratch(` and `scratch(` — tokens that do not appear in
production code, so "everything after the file's first `#[cfg(test)]`" was
close enough for them. Rules 4 and 5 key on `temp_dir()`, which production
code uses legitimately (`src/scratch.rs`'s own fallback, `commands/preload.rs`),
and a review probe showed the loose reading accusing two production lines that
merely sat after a `#[cfg(test)] use` (review I1). A lint that can redden CI on
a clean production line is the same defect as the leak it hunts, so the region
is now computed rather than assumed, for **all five** rules:

  - every line of a file whose parent module declares it `#[cfg(test)] mod x;`
    (`core/osinstall/source_contract.rs`);
  - the item each `#[cfg(test)]` attribute is attached to, from the attribute
    through the `}` that closes it — or through its own `;` when it has no
    block (`#[cfg(test)] mod x;`).

The item's end is found by **indent**, not by counting braces, and string
literals are blanked first. Both rules are borrowed wholesale from this
crate's own `core::independence::test_regions` (`src-tauri/src/core/mod.rs`),
which walks the same tree for a different reason and had both defects found in
review: a `format!` assembling an AmigaDOS script or a `.uae` file carries
literal braces, and a signature rustfmt wrapped across lines does not open its
block on the line after the attribute. `cargo fmt --check` is blocking in CI,
so a closing brace is always aligned with the line that opened it, which is
what makes indent the reliable signal here.

## What this sweep cannot see

Rule 3 keys on the *shape* of the return type. A helper that returns an
`InstallPlan` or a `Box<dyn MediaSource>` holding a path built under a scratch
is the same defect and is invisible here — a name cannot be typed. Those come
out of rule 2 instead: the `let` inside them is listed, and threading the
guard out is part of fixing the call site. This is stated rather than
silently assumed: a guard that passes vacuously is the class of defect the
counter sweep already shipped once.

Rules 4 and 5 read one line at a time, so a `temp_dir()` split across two
lines by rustfmt is caught only in the one shape that exists today (the
`.join(` continuing on the next line); a scratch path assembled through an
intermediate variable — `let t = std::env::temp_dir(); t.join(…)` — is
invisible to both. Neither shape is in the tree on 2026-09-10, and both would
make the sweep under-report rather than accuse a clean line.

Rule 4's read-only exemption has two blind spots of its own, both stated
rather than left to be found. An `assert!` that *creates* while it reads —
`assert!(std::fs::create_dir_all(std::env::temp_dir().join("x")).is_ok())` —
is exempt, because the anchor asks where the match sits and not what the
enclosing call does; and the `.exists()` tail is judged on one line, so a path
joined on one line and created on the next escapes. Both are under-reporting
of a shape nothing in the tree has. The creation test that guards the tail
(`create_dir`, `File::create`, `fs::write(`) is a keyword list, not an
analysis.

Neither rule looks inside a macro body or a `build.rs`, and neither can tell a
test helper compiled into the crate for an integration test (`tests/`) from
production — ART has no `tests/` directory today.

Run it from `amiga-retro-toolkit/`:

    python scripts/scratch-guard-sweep.py
    python scripts/scratch-guard-sweep.py --self-test

Exit 0 with `scratch-guard sweep: clean — N helpers, M call sites, …`. Exit 1
lists every offender as `path:line: <why>` and prints the totals.

`--self-test` runs rules 4 and 5 over synthetic source held in this file — no
repository file is read and none is written — and checks each case against the
answer written down beside it. Every case is one of the two arms of a defect
this sweep has actually had: a production line after a `#[cfg(test)] use` that
must **not** be accused, a hand-built path whose *literal* contains the word
"assert" that must be, an `assert!`-wrapped product call that must be, the
`JoinHandle::join()` line that must not, and the header's own exemption
paragraph checked against `ALLOWED_HAND_BUILT`. It is the probe a reader can
re-run instead of trusting this paragraph, and it fails if a rule is loosened
back to any of those shapes.

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
# Rule 4: a scratch path built by hand. `temp_dir()` and `.join(` may be split
# by rustfmt, so the `.join(` is also looked for at the head of the next line.
TEMP_DIR = re.compile(r"(?<![A-Za-z0-9_])temp_dir\(\)")
# Rule 5: the platform root handed to product code as *its* scratch root.
TEMP_ROOT_ARG = re.compile(r"&\s*std::env::temp_dir\(\)")
# Rule 4 only: a use that reads and cannot create. Each is asked of the place
# the match *sits*, never of the whole line — `assert` as a substring exempted
# `let dir = temp_dir().join("art-probe-assertive")` on its literal alone.
ASSERT_MACROS = ("assert!(", "assert_eq!(", "assert_ne!(", "debug_assert!(")
READ_DIR_CALLS = ("read_dir(",)
READ_ONLY_TAILS = (".exists()", ".is_dir()", ".is_file()")
# The keyword list that stops the `.exists()` tail from exempting a line that
# also creates. A list, not an analysis — said in the header.
CREATES = ("create_dir", "File::create", "fs::write(", "write(&", "copy(")
# Rule 5's only exemption: the platform root is the subject of the test — the
# namespace derived from it, or the sweeper that cleans it. Anchored the same
# way: the match must sit inside that call's parentheses.
ROOT_IS_THE_SUBJECT = ("scratch_root_for(", "sweep_stale_preview_scratch_dirs(")
# Rule 4's exemptions, by file and by the literal that identifies the site
# rather than by a line number that drifts. Each names a path the test never
# creates. `--self-test` checks this table against the header paragraph that
# describes it, because the two disagreed once already (fix round 1, I3).
ALLOWED_HAND_BUILT = {
    "src-tauri/src/core/osinstall/scan_cache.rs": [
        ("art-measure-dest", "the same, in an #[ignore]d measurement that "
                             "plans and never applies"),
    ],
    "src-tauri/src/core/sources/library.rs": [
        ("art-library-does-not-exist", "the test's subject is a root that "
                                       "does not exist; creating it would "
                                       "delete the assertion"),
    ],
}


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


def strip_string_literals(line: str) -> str:
    """The line with the *contents* of its string literals blanked out.

    A `format!` that assembles an AmigaDOS script or a `.uae` file carries
    literal `{` and `}`, and `test_regions` below decides where a
    `#[cfg(test)]` item ends by looking at braces and indentation. Blanking
    the contents first means both questions are asked of the code. Ported,
    with its reason, from `core::independence::strip_string_literals` in
    `src-tauri/src/core/mod.rs`, where the same defect was found in review.

    Raw strings (`r"…"`, `r#"…"#`) and char literals are handled crudely: the
    opening quote of a raw string is found, the matching hash-delimited close
    is looked for, and anything unterminated blanks to end of line. That is
    conservative in the safe direction — a line whose literals are blanked can
    only lose braces, never gain them."""
    out: list[str] = []
    i = 0
    n = len(line)
    while i < n:
        ch = line[i]
        if ch == "r" and i + 1 < n and (line[i + 1] == '"' or line[i + 1] == "#"):
            j = i + 1
            hashes = 0
            while j < n and line[j] == "#":
                hashes += 1
                j += 1
            if j < n and line[j] == '"':
                close = '"' + "#" * hashes
                end = line.find(close, j + 1)
                end = n if end == -1 else end + len(close)
                out.append(line[i:j + 1])
                out.append(" " * (end - j - 1))
                i = end
                continue
        if ch == '"':
            j = i + 1
            while j < n:
                if line[j] == "\\":
                    j += 2
                    continue
                if line[j] == '"':
                    break
                j += 1
            end = min(j, n)
            out.append('"')
            out.append(" " * max(0, end - i - 1))
            if end < n:
                out.append('"')
            i = end + 1
            continue
        out.append(ch)
        i += 1
    return "".join(out)


def indent_of(line: str) -> int:
    return len(line) - len(line.lstrip())


def test_region_mask(lines: list[str], whole_file: bool) -> list[bool]:
    """Which lines are test code, one bool per line.

    Not "everything after the first `#[cfg(test)]`": rules 4 and 5 key on
    `temp_dir()`, which production code uses, and that loose reading accused
    two production lines sitting after a `#[cfg(test)] use` in a review probe
    (fix round 1, I1). A region is the item an attribute is attached to, from
    the attribute through the `}` aligned with it — or through the item's own
    `;` when it has no block. Stacked attributes at the same indent and a
    signature rustfmt wrapped across lines are both handled, because
    `core::independence::test_regions` had that exact bug found in its own
    review (`core/volume/write/mod.rs`'s wrapped `commit_blocks`)."""
    if whole_file:
        return [True] * len(lines)
    scan = [strip_string_literals(line) for line in lines]
    mask = [False] * len(lines)
    i = 0
    while i < len(scan):
        if scan[i].strip() != "#[cfg(test)]":
            i += 1
            continue
        indent = indent_of(scan[i])
        j = i + 1
        # Stacked attributes sit at the same indent as the item they gate.
        while j < len(scan) and indent_of(scan[j]) == indent and scan[j].lstrip().startswith("#"):
            j += 1
        # The declaration may wrap; find the line that opens the block.
        sig_end = j
        opens_block = False
        while sig_end < len(scan):
            trimmed = scan[sig_end].rstrip()
            if trimmed.endswith("{"):
                opens_block = True
                break
            if trimmed.endswith(";") or not trimmed.strip():
                break
            sig_end += 1
        if opens_block:
            end = sig_end + 1
            while end < len(scan) and not (
                indent_of(scan[end]) == indent and scan[end].strip() == "}"
            ):
                end += 1
            end = min(end, len(scan) - 1)
        else:
            end = min(sig_end, len(scan) - 1)
        for k in range(i, end + 1):
            mask[k] = True
        i = end + 1
    return mask


def inside_call(line: str, col: int, names: tuple[str, ...]) -> bool:
    """`col` sits inside the parentheses of one of `names`.

    The anchor that replaced a whole-line substring test (fix round 1, I2):
    `assert` *somewhere* on the line is not the same claim as "this match is
    an argument of an assertion", and the difference is a hand-built scratch
    path whose literal happens to contain the word."""
    for name in names:
        start = 0
        while True:
            q = line.find(name, start)
            if q == -1 or q >= col:
                break
            depth = 0
            for ch in line[q + len(name) - 1:col]:
                if ch == "(":
                    depth += 1
                elif ch == ")":
                    depth -= 1
            if depth >= 1:
                return True
            start = q + 1
    return False


def only_reads(line: str, start: int, end: int) -> bool:
    """Rule 4's read-only exemption, anchored. Three shapes, no others: the
    match is an argument of an assertion, or of a `read_dir(`, or the joined
    path is asked `.exists()` / `.is_dir()` / `.is_file()` on this same line
    and nothing on the line creates."""
    if inside_call(line, start, ASSERT_MACROS) or inside_call(line, start, READ_DIR_CALLS):
        return True
    tail = line[end:]
    return any(t in tail for t in READ_ONLY_TAILS) and not any(c in line for c in CREATES)


def scratchdir_impl_lines(lines: list[str]) -> set[int]:
    """The line indexes inside `impl ScratchDir` / `impl Drop for ScratchDir`.

    That impl is the one place in the crate where building a path under
    `std::env::temp_dir()` is the point rather than the defect, so rule 4 must
    not accuse it. Both impls sit at the file's top level, so a `}` in column
    zero ends them."""
    inside: set[int] = set()
    open_block = False
    for i, line in enumerate(lines):
        if re.match(r"^impl\s+(Drop\s+for\s+)?ScratchDir\b", line):
            open_block = True
        if open_block:
            inside.add(i)
            if line.rstrip() == "}":
                open_block = False
    return inside


def exempt_hand_built(rel: str, line: str) -> str | None:
    """The documented reason this hand-built path is not an offender, or
    `None`. Matched on the literal, not the line number, so a site that
    changes what it does loses its exemption."""
    for needle, why in ALLOWED_HAND_BUILT.get(rel, []):
        if needle in line:
            return why
    return None


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


def temp_dir_rules(
    rel: str, lines: list[str], in_test: list[bool]
) -> tuple[list[tuple[str, int, str]], tuple[int, int, int]]:
    """Rules 4 and 5 over one file's lines.

    Split out of `main` so `--self-test` can run it against synthetic source
    without reading or writing anything in the repository. Returns the
    offenders and `(hand-built paths, platform-root arguments, exempt)`."""
    offenders: list[tuple[str, int, str]] = []
    hand_built = root_args = exempted = 0
    in_guard_impl = scratchdir_impl_lines(lines)

    for i, line in enumerate(lines):
        if not in_test[i]:
            continue

        m = TEMP_DIR.search(line)
        if m and not is_comment(line, m.start()) and i not in in_guard_impl:
            # The continuation shape is `…temp_dir()` at the *end* of the
            # line and `.join(` at the head of the next. Accepting any
            # following `.join(` accused
            # `namespace_of(&scratch_root_for(&std::env::temp_dir()))`
            # whose next line was a `JoinHandle`'s own `.join()` — a false
            # accusation is worse than a missed line, so the tail is
            # anchored.
            joined = ".join(" in line[m.end():] or (
                line.rstrip().endswith("temp_dir()")
                and i + 1 < len(lines)
                and lines[i + 1].lstrip().startswith(".join(")
            )
            if joined:
                hand_built += 1
                why = exempt_hand_built(rel, line)
                if why is None and only_reads(line, m.start(), m.end()):
                    why = "reads the path and cannot create it"
                if why is None and line.rstrip().endswith("temp_dir()") and i + 1 < len(lines):
                    # Only the split-line shape may borrow the next line's
                    # literal (fix round 1, M2) — an exempt literal below an
                    # unrelated hand-built path no longer covers it.
                    why = exempt_hand_built(rel, lines[i + 1])
                if why is None:
                    offenders.append((rel, i + 1, "builds a scratch path by hand"))
                else:
                    exempted += 1

        m = TEMP_ROOT_ARG.search(line)
        if m and not is_comment(line, m.start()):
            root_args += 1
            if inside_call(line, m.start(), ROOT_IS_THE_SUBJECT):
                exempted += 1
            else:
                offenders.append((rel, i + 1, "hands the platform root to product code"))

    return offenders, (hand_built, root_args, exempted)


# --- the self-test -----------------------------------------------------------
#
# Each case is source this sweep has been wrong about, with the answer written
# down beside it. `expected` is the set of 1-based line numbers that must be
# reported; anything else — an extra accusation or a missing one — fails.

SELF_TEST_CASES: list[tuple[str, str, set[int]]] = [
    (
        "production code after a `#[cfg(test)] use` is not test code (I1)",
        """#[cfg(test)]
use std::fs;

pub fn production_default_root() -> PathBuf {
    std::env::temp_dir().join("art-runtime-cache")
}

pub fn production_call() -> bool {
    prepare(&std::env::temp_dir())
}
""",
        set(),
    ),
    (
        "a `#[cfg(test)]` item ends at its own closing brace, and the "
        "production line after it is not swept in (I1)",
        """#[cfg(test)]
pub struct ScratchDirLike(PathBuf);

pub fn production_default_root() -> PathBuf {
    std::env::temp_dir().join("art-runtime-cache")
}
""",
        set(),
    ),
    (
        "inside `mod tests`, a hand-built path is an offender — including one "
        "whose literal merely contains the word assert (I2)",
        """#[cfg(test)]
mod tests {
    #[test]
    fn t() {
        let plain = std::env::temp_dir().join("art-probe-plain");
        let assertive = std::env::temp_dir().join("art-probe-assertive");
        std::fs::create_dir_all(&plain).unwrap();
        std::fs::create_dir_all(&assertive).unwrap();
    }
}
""",
        {5, 6},
    ),
    (
        "an assertion that only looks at the platform root is exempt; a "
        "product call wrapped in one is not (I2)",
        """#[cfg(test)]
mod tests {
    #[test]
    fn t() {
        assert!(!std::env::temp_dir().join("escaped.txt").exists());
        assert!(build_plan(&std::env::temp_dir(), "x").is_ok());
    }
}
""",
        {6},
    ),
    (
        "the platform root as the subject of the test is exempt, and a "
        "`JoinHandle::join()` on the next line is not a `.join(` continuation",
        """#[cfg(test)]
mod tests {
    #[test]
    fn t() {
        let mine = namespace_of(&scratch_root_for(&std::env::temp_dir()));
        let theirs = std::thread::spawn(|| namespace_of(&scratch_root_for(&std::env::temp_dir())))
            .join()
            .unwrap();
        let commented = 1; // let d = std::env::temp_dir().join("art-x");
    }
}
""",
        set(),
    ),
    (
        "a brace inside a string literal does not end the test region",
        """#[cfg(test)]
mod tests {
    #[test]
    fn t() {
        let script = format!("if EXISTS {name}\\n}}\\n");
        let dir = std::env::temp_dir().join("art-probe-after-a-brace");
        std::fs::create_dir_all(&dir).unwrap();
    }
}
""",
        {6},
    ),
]


def self_test() -> int:
    """Run rules 4 and 5 over the synthetic cases above, plus the header's
    agreement with `ALLOWED_HAND_BUILT`. No repository file is touched."""
    failures = 0
    for name, src, expected in SELF_TEST_CASES:
        lines = src.split("\n")
        mask = test_region_mask(lines, whole_file=False)
        found, _ = temp_dir_rules("src-tauri/src/probe.rs", lines, mask)
        got = {line for _, line, _ in found}
        ok = got == expected
        failures += 0 if ok else 1
        print(f"  {'PASS' if ok else 'FAIL'}  {name}")
        if not ok:
            print(f"        expected lines {sorted(expected)}, got {sorted(got)}")
            for _, line, why in sorted(found, key=lambda o: o[1]):
                print(f"        line {line}: {why}")

    # The header paragraph and the exemption table must agree (I3): the prose
    # once said "the four sites" and named a file the table did not hold.
    doc = __doc__ or ""
    words = {1: "one", 2: "two", 3: "three", 4: "four", 5: "five", 6: "six"}
    n = sum(len(v) for v in ALLOWED_HAND_BUILT.values())
    claim = f"the {words.get(n, str(n))} sites in `ALLOWED_HAND_BUILT`"
    prose_ok = claim in " ".join(doc.split())
    if not prose_ok:
        print(f'        the header does not say "{claim}"')
    for literals in ALLOWED_HAND_BUILT.values():
        for literal, _ in literals:
            if literal not in doc:
                prose_ok = False
                print(f"        {literal} is exempt but the header never names it")
    failures += 0 if prose_ok else 1
    print(f"  {'PASS' if prose_ok else 'FAIL'}  the header's exemption "
          f"paragraph matches ALLOWED_HAND_BUILT ({n} site(s))")

    print(f"scratch-guard self-test: {len(SELF_TEST_CASES) + 1 - failures}"
          f"/{len(SELF_TEST_CASES) + 1} passed")
    return 1 if failures else 0


def main() -> int:
    offenders: list[tuple[str, int, str]] = []
    helpers = guarded_helpers = 0
    calls = accepted_calls = 0
    rule_counts = {"returns a bare path": 0, "bound without its guard": 0,
                   "guard dropped at once": 0, "used without binding its guard": 0,
                   "returns a path whose guard it drops": 0,
                   "builds a scratch path by hand": 0,
                   "hands the platform root to product code": 0}
    hand_built = exempted = root_args = 0
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
        whole_file = False
        if cut is None:
            # No attribute of its own — but the module may be gated where it
            # is declared, in which case the whole file is test code.
            if not gated_at_its_declaration(path):
                continue
            cut = 0
            whole_file = True
        # The real region, item by item (fix round 1, I1). `cut` still bounds
        # the loops below — it is cheap and never wrong in the other
        # direction — but membership is decided by the mask.
        in_test = test_region_mask(lines, whole_file)
        shape = resolved_shape(path)

        # Rules 4 and 5: the two shapes that carry no `scratch(` call at all.
        found, counted = temp_dir_rules(rel, lines, in_test)
        offenders.extend(found)
        for _, _, why in found:
            rule_counts[why] += 1
        hand_built += counted[0]
        root_args += counted[1]
        exempted += counted[2]

        for i in range(cut, len(lines)):
            line = lines[i]
            if not in_test[i]:
                continue
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
        print(f"scratch-guard sweep: clean — {helpers} helpers, {calls} call "
              f"sites, {hand_built} hand-built paths, {root_args} platform-root "
              f"arguments ({exempted} exempt)")
        return 0

    print()
    print(f"helpers                            : {helpers}"
          f" ({guarded_helpers} already hand out a guard)")
    print(f"call sites                         : {calls}"
          f" ({accepted_calls} keep their guard)")
    print(f"hand-built paths                   : {hand_built}")
    print(f"platform-root arguments            : {root_args}")
    print(f"exempt, with a reason              : {exempted}")
    for why, n in rule_counts.items():
        if n:
            print(f"  {why:33s}: {n}")
    print(f"offenders                          : {len(offenders)}")
    return 1


if __name__ == "__main__":
    raise SystemExit(self_test() if "--self-test" in sys.argv[1:] else main())
