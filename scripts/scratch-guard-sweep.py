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
  2. **bound without its guard** — a call to a **guard source** whose
     binding lets the guard go. A guard source is any function that hands one
     to its caller, and they are discovered **by return type, never by name**:
     `ScratchDir::new` and `ScratchDir::pair` themselves, every `fn` whose
     return type mentions `ScratchDir` — bare (`-> ScratchDir`), in a tuple
     (`-> (ScratchDir, PathBuf)`, and `write_image`'s three-element form), or
     inside a collection (`-> Vec<(ScratchDir, &str, Box<dyn MediaSource>)>`)
     — and every `fn` returning a struct that owns one in a field
     (`sources::fetch::Fixture`, `pistorm::tests::Card`).

     Keying on the identifier `scratch(` instead, as this rule did until the
     final review, left the 48 direct `ScratchDir::pair(` sites and the 17
     fixture helpers with other names (`tmp`, `tempdir`, `disc`, `write`,
     `write_image`, `planned_with`, `sources`, …) outside the guard
     altogether: 765 call sites were checked where 1691 exist, and a probe
     showed `let (_, dir, _n) = helper("x")`, `let dir = pair(..).1` and a
     non-`scratch` helper dropping a guard all passing. The tree was clean of
     all three, which is what a vacuous guard looks like from the outside.

     What a correct binding is depends on the shape the source returns. A
     bare `ScratchDir` (`core/rom/place.rs`, `core/osinstall/chain.rs`) is
     correct as `let dir = scratch("x")` — `dir` *is* the guard — and
     demanding a tuple there would be this sweep inventing a defect. A tuple
     must be destructured with the guard's own element held by a real name:
     `let (_, dir) = …` drops it at once, deleting the directory before the
     test's first line, and is called out separately. The guard is element 0
     everywhere in ART today; the message names the position when it is not.
     A collection may be bound whole (it owns every guard it holds) or
     destructured per element in a `for` pattern. `pair(..).1` is an offender
     and `pair(..).0` is not: the first drops the guard with the temporary it
     came from, the second keeps it.

     Names are resolved the way Rust resolves them: `Self::new(` and
     `Type::new(` reach that type's associated function and no other
     (`core/amigainstall/run.rs` has four unrelated `fn new`), an unqualified
     call reaches this file's own free functions, and `super::` / `fixtures::`
     reach the directory's `mod.rs`. A method call (`dir.join(`) is never one
     of ours.
  3. **returns a path whose guard it drops** — a helper `fn` that is not a
     `#[test]`, calls a guard source, and returns a `PathBuf`/`Path`-shaped
     type or a struct that does not carry a `ScratchDir`. The guard dies at
     that helper's closing brace while the path it returns is used by the
     caller — so the directory is both leaked *and*, once the helpers are
     converted, gone early. These need the guard threaded out, which is what
     `core::iso::tests::write_image`, `cbm::{d64,t64}::write` and
     `osinstall::source_cd::disc` had to do.
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

     Exempt: **two** things. The first is an occurrence inside the parentheses of
     `scratch_root_for(` or `sweep_stale_preview_scratch_dirs(`, where the
     platform root **is** the subject of the test (a namespace derived from
     it, or the sweeper that cleans it) and substituting a scratch would
     delete the assertion rather than move it. Rule 4's read-only exemption
     deliberately does **not** carry over: `assert!(build_plan(&std::env::
     temp_dir(), …).is_ok())` hands the platform root to product code exactly
     like the un-asserted call does, and the first version of this rule let
     that whole shape through (fix round 1, review I2).

     The second is the body of a **test-only wrapper** in
     `TEST_ONLY_WRAPPERS` — `apply`, `add_package`, `plan_with_cache`,
     `plan_over` and `open_package` (ART-295). Each is `#[cfg(test)]`, so the
     compiler refuses a production call to one, and each exists so a test can
     stage without naming a root; three whole-suite runs measured the staging
     beneath them clean. They are listed by file and by the wrapper's **own**
     name, never by the callee: exempting `apply_staging_in(` would excuse any
     test handing the platform root to it directly, which is the shape this
     rule was written for. They were invisible to this rule while they were
     production code, and one of their siblings hid a production call that
     staged BoingBag payloads on the system drive (ART-296).

## Which lines count as test code

Rules 1-3 once keyed on `fn scratch(` and `scratch(` — tokens that do not
appear in production code, so "everything after the file's first
`#[cfg(test)]`" was close enough for them. It is not close enough now that
they follow return types across every function in the file. Rules 4 and 5 key
on `temp_dir()`, which production code uses legitimately (`src/scratch.rs`'s
own fallback, `commands/preload.rs`),
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

Rules 2 and 3 have three blind spots of their own, all under-reporting:

  - **A guard source reached through a glob import.** Resolution covers this
    file, `super::`/`fixtures::` into the directory's `mod.rs`,
    `Type::`/`Self::` for associated functions, and — since the re-review's
    N1 — every **named** `use` inside the test region, grouped or aliased,
    followed to the module it names (`mod x;` to `x.rs` or `x/mod.rs`, an
    inline `mod x {` to the file it is written in). A `use super::*;` is not
    followed: a glob names nothing, so a helper reachable only that way is
    not checked. The claim that stood here before — that nothing in the tree
    imported a helper and called it bare — was **false**:
    `core/osinstall/scan.rs` (32 calls) and `core/osinstall/source_archive.rs`
    (24) do exactly that, and 56 call sites sat outside the sweep until the
    `use` lines were read.
  - **A guard that leaves a helper inside a type the sweep cannot read** — an
    `InstallPlan` or a `Box<dyn MediaSource>` holding a scratch path. Rule 3
    asks the *shape* of the return type, so only a path-shaped one or a
    struct declared in the same file can be judged; anything else passes. The
    call sites inside such a helper are still checked by rule 2.
  - **A binding spread over more than one statement** — `let pair = helper();
    let dir = pair.1;` — is two statements, and each is read alone.

Rule 1 still keys on the name `fn scratch(`, because "a helper that hands back
a bare path" cannot be recognised from a return type alone: that *is* the
shape of every path-returning function in the crate. Rule 3 is what catches
the same defect under another name, by asking what the helper calls.

Run it from `amiga-retro-toolkit/`:

    python scripts/scratch-guard-sweep.py
    python scripts/scratch-guard-sweep.py --self-test

Exit 0 with `scratch-guard sweep: clean — N helpers, M call sites, …`. Exit 1
lists every offender as `path:line: <why>` and prints the totals.

`--self-test` runs the rules over synthetic source held in this file — no
repository file is read and none is written — and checks each case against the
answer written down beside it. Every case is one arm of a defect this sweep
has actually had, with its clean twin beside it, so a rule that stops
accusing and a rule that starts over-accusing both fail here: a production
line after a `#[cfg(test)] use` that must **not** be accused, a hand-built
path whose *literal* contains the word "assert" that must be, an
`assert!`-wrapped product call that must be, the `JoinHandle::join()` line
that must not, `let (_, dir, _n) = helper("x")`, `let dir = pair(..).1`, a
non-`scratch` helper returning a path from a guard it drops, a carrier struct
and a `for`-destructured collection that must both pass, a helper reached
through a grouped, a relative and an aliased `use`, and a summary asked to
print an offender kind no dictionary knew about. The last two are the
re-review's N1 and N2; the header's own exemption paragraph is checked against
`ALLOWED_HAND_BUILT` in the same run. It is the probe a reader can re-run
instead of trusting this paragraph.

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
IDENT_LET = re.compile(r"^(?:let|for)\s+(?:mut\s+)?(\w+)\s*(?:=|in\b)")
# A return type that is a path and nothing else: `-> PathBuf`,
# `-> std::path::PathBuf`, `-> (PathBuf, PathBuf)`, `-> &Path`.
PATH_SHAPED = re.compile(r"\bPathBuf\b|\bPath\b")
# Rules 2 and 3, generalised: a call to *any* function, with its qualifier read
# off the text in front of it. `scratch(` was never the population — the tree
# holds 17 fixture helpers with other names and 48 direct `ScratchDir::pair(`
# sites (final review, I1).
CALL_TOKEN = re.compile(r"(?<![A-Za-z0-9_])([a-z_]\w*)\s*\(")
QUALIFIER = re.compile(r"([A-Za-z0-9_]+)::\s*$")
# `let (a, b, c) = …` with any arity. The elements are read back out, because
# it is the element at the *guard's* position that has to be a real binding.
TUPLE_LET_ANY = re.compile(
    r"^(?:let|for|if\s+let|while\s+let)\s*\(\s*([^)]*?)\s*\)\s*(?:=|in\b)")
# `= helper(…).0` — the tuple is a temporary, so every element it holds but
# that one is dropped before the next statement.
FIELD_ACCESS = re.compile(r"\)\s*\.(\d+)")
STRUCT_DEF = re.compile(r"^(\s*)(?:pub(?:\([^)]*\))?\s+)?struct\s+(\w+)")
# The two the crate itself provides. Every other guard source is discovered.
BUILTIN_GUARD_SOURCES = {"pair": ("tuple", 0), "new": ("guard", 0)}
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
# Rule 5's second exemption (ART-295): the test-only wrappers themselves, by
# file and by the wrapper's own name — never by the callee, which would excuse
# a test handing the platform root to the `_in` function directly. The header
# names every one, and `--self-test` fails if the two stop agreeing.
TEST_ONLY_WRAPPERS = {
    "src-tauri/src/core/osinstall/apply.rs": {
        "apply": "test-only wrapper over apply_staging_in",
        "add_package": "test-only wrapper over add_package_staging_in",
    },
    "src-tauri/src/core/osinstall/plan.rs": {
        "plan_with_cache": "test-only wrapper over plan_with_cache_in",
        "plan_over": "test-only wrapper over plan_over_with_cache",
    },
    "src-tauri/src/core/osinstall/scan.rs": {
        "open_package": "test-only wrapper over open_package_staging_in",
    },
}
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


def code_of(line: str) -> str:
    """The line without its trailing `//` comment, string literals blanked
    first so a `//` inside one cannot cut the code short."""
    blanked = strip_string_literals(line)
    at = blanked.find("//")
    return line[:at] if at != -1 else line


def statement_start(lines: list[str], index: int) -> int:
    """The first line of the statement containing line `index`. Walks back
    while the previous line looks like a continuation — it does not end a
    statement or a block."""
    i = index
    while i > 0:
        prev = code_of(lines[i - 1]).rstrip()
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


def split_top_level(text: str) -> list[str]:
    """Split a tuple's element list on its top-level commas."""
    out, depth, current = [], 0, ""
    for ch in text:
        if ch in "(<[":
            depth += 1
        elif ch in ")>]":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(current)
            current = ""
            continue
        current += ch
    out.append(current)
    return [part.strip() for part in out if part.strip()]


def return_type(sig: str) -> str:
    """The return type out of a signature line, without its body."""
    if "->" not in sig:
        return ""
    ret = sig.split("->", 1)[1]
    return ret.split("{", 1)[0].strip()


def carrier_structs(lines: list[str], mask: list[bool]) -> set[str]:
    """Structs that carry a `ScratchDir` in a field — `sources::fetch::Fixture`,
    `sources::library::Fixture`, `pistorm::tests::Card`.

    A value of such a type *is* the guard's owner, so a helper may return one
    by value and a call site may bind it with a plain `let`. Both the named
    form (`_guard: ScratchDir`) and the tuple form (`Card(PathBuf,
    ScratchDir)`) count; the guard's field name is not required to be
    `_guard`, because what makes it a carrier is the type."""
    found: set[str] = set()
    for i, line in enumerate(lines):
        m = STRUCT_DEF.match(line)
        if not m or not mask[i]:
            continue
        name, indent = m.group(2), len(m.group(1))
        if line.rstrip().endswith(";") or "ScratchDir" in line:
            # A one-line declaration answers for itself, either way.
            if "ScratchDir" in line:
                found.add(name)
            continue
        j = i + 1
        while j < len(lines):
            if len(lines[j]) - len(lines[j].lstrip()) == indent and lines[j].strip() in ("}", "};"):
                break
            if "ScratchDir" in lines[j]:
                found.add(name)
                break
            if lines[j].strip().startswith("fn ") or STRUCT_DEF.match(lines[j]):
                break
            if lines[j].rstrip().endswith(";"):
                # A one-line tuple struct ends here. Walking past it made
                # `VecDevice` a carrier because a `ScratchDir` appeared later
                # in the file — a false accusation in `volume/write/bitmap.rs`.
                break
            j += 1
    return found


def guard_sources(lines: list[str], mask: list[bool], carriers: set[str]) -> dict:
    """Every function in this file that hands a guard to its caller, keyed by
    name, discovered by **return type** and never by name (final review, I1).

    Three shapes: `("guard", 0)` for `-> ScratchDir`; `("tuple", i)` for a
    tuple whose element `i` is the guard (`-> (ScratchDir, PathBuf)`, and
    `write_image`'s three-element form); `("carrier", 0)` for a function
    returning a struct that owns one in a field (`-> Self` inside such a
    struct's `impl`)."""
    sources: dict = {}
    for i, line in enumerate(lines):
        m = FN_RE.match(line)
        if not m or not mask[i]:
            continue
        name = m.group(5)
        ret = return_type(signature(lines, i))
        if not ret:
            continue
        shape = None
        if "ScratchDir" in ret:
            if ret.startswith("("):
                inner = ret[1:ret.rfind(")")] if ret.endswith(")") else ret[1:]
                parts = split_top_level(inner)
                idx = next((k for k, part in enumerate(parts) if "ScratchDir" in part), 0)
                shape = ("tuple", idx)
            elif "(" in ret:
                # A container of tuples — `Vec<(ScratchDir, &str, Box<dyn
                # MediaSource>)>` in `core/osinstall/source_contract.rs`. The
                # collection owns every guard, so binding it whole is correct
                # and so is destructuring an element in a `for` pattern; the
                # index is the one inside the element tuple.
                inner = ret[ret.index("(") + 1:ret.rindex(")")]
                parts = split_top_level(inner)
                idx = next((k for k, part in enumerate(parts) if "ScratchDir" in part), 0)
                shape = ("container", idx)
            else:
                shape = ("guard", 0)
        else:
            base = ret.lstrip("&").split("<")[0].strip()
            if ret == "Self":
                base = enclosing_impl(lines, i) or "Self"
            if base in carriers:
                shape = ("carrier", 0)
        if shape is None:
            continue
        owner = enclosing_impl(lines, i)
        indent = len(m.group(1))
        if owner is not None and indent > 0:
            # An associated function: `Fixture::new` is not the same function
            # as the `new` three impls further down, and keying by bare name
            # made every `Self::new(` in `core/amigainstall/run.rs` resolve to
            # whichever came last.
            sources[(owner, name)] = shape
        else:
            sources[name] = shape
    return sources


def qualifier_of(line: str, start: int) -> str | None:
    """What sits in front of a call: `None` for an unqualified one, the last
    path segment for `crate::core::ScratchDir::pair(`, and `"."` for a method
    call, which is never one of ours."""
    head = line[:start].rstrip()
    if head.endswith("."):
        return "."
    if head.endswith("::"):
        m = QUALIFIER.search(head)
        return m.group(1) if m else None
    return None


IMPL_RE = re.compile(r"^\s*impl(?:<[^>]*>)?\s+(?:.+\s+for\s+)?([A-Za-z_]\w*)")
MOD_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+")


USE_START = re.compile(r"^(\s*)use\s+(.*)$")


def import_map(lines: list[str], mask: list[bool]) -> dict[str, tuple[list[str], str, bool]]:
    """`local name -> (module segments, the name in that module, nested)`, for
    every `use` inside the test region.

    Two files in ART import a fixture helper and then call it bare —
    `core/osinstall/scan.rs` (`use crate::core::osinstall::fixtures::{media,
    scratch, write_test_iso};`, 32 calls) and `core/osinstall/source_archive.rs`
    (`use super::super::fixtures::scratch;`, 24) — and the fix wave's
    "an unqualified call reaches this file only" dropped all 56 out of the
    sweep, which the re-review measured with a planted defect (N1). Grouped
    braces and `as` aliases are handled; a glob (`use super::*;`) is not, and
    is disclosed in the header.

    `nested` records whether the `use` sits inside a block, because that is
    what `super` counts from: inside `mod tests`, `super` is the file's own
    module."""
    out: dict[str, tuple[list[str], str, bool]] = {}
    i = 0
    while i < len(lines):
        m = USE_START.match(lines[i]) if mask[i] else None
        if not m:
            i += 1
            continue
        nested = len(m.group(1)) > 0
        text = m.group(2)
        while ";" not in text and i + 1 < len(lines):
            i += 1
            text += " " + lines[i].strip()
        text = text.split(";", 1)[0].strip()
        i += 1

        if "{" in text:
            head, group = text.split("{", 1)
            items = split_top_level(group.rsplit("}", 1)[0])
        else:
            head, items = text.rsplit("::", 1)[0] + "::", [text.rsplit("::", 1)[-1]]
        segs = [seg for seg in head.strip().split("::") if seg]
        for item in items:
            item = item.strip()
            if not item or "*" in item or "{" in item:
                continue
            if " as " in item:
                name, alias = (part.strip() for part in item.split(" as ", 1))
            else:
                name = alias = item.rsplit("::", 1)[-1].strip()
            prefix = item.rsplit("::", 1)[0].split(" as ")[0].strip() if "::" in item else ""
            full = segs + [seg for seg in prefix.split("::") if seg]
            out[alias] = (full, name, nested)
    return out


def module_file(path: Path, segs: list[str], nested: bool, source_lines: dict) -> Path | None:
    """The file a module path names, walked the way Rust walks it: `mod x;`
    goes to `x.rs` or `x/mod.rs`, and an inline `mod x {` stays in the file it
    is written in (`core::osinstall::fixtures` is inline in that module's own
    `mod.rs`)."""
    segs = list(segs)
    if not segs:
        return None
    if segs[0] == "crate":
        segs.pop(0)
        current = ROOT / "lib.rs"
    elif segs[0] in ("super", "self"):
        current = path
        if segs[0] == "self":
            segs.pop(0)
        else:
            # Inside `mod tests`, the first `super` is the file's own module.
            if nested:
                segs.pop(0)
            while segs and segs[0] == "super":
                segs.pop(0)
                current = (current.parent.parent if current.name == "mod.rs"
                           else current.parent) / "mod.rs"
    else:
        return None

    for seg in segs:
        lines = source_lines.get(current)
        if lines is None:
            return None
        inline = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+" + re.escape(seg) + r"\s*\{")
        declared = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+" + re.escape(seg) + r"\s*;")
        if any(inline.match(line) for line in lines):
            continue  # an inline module lives in the same file
        if any(declared.match(line) for line in lines):
            folder = current.parent if current.name in ("mod.rs", "lib.rs") else current.parent
            for candidate in (folder / seg / "mod.rs", folder / f"{seg}.rs"):
                if candidate in source_lines:
                    current = candidate
                    break
            else:
                return None
            continue
        return None
    return current


def imported_sources(path: Path, lines: list[str], mask: list[bool],
                     source_lines: dict, sources_of: dict) -> dict:
    """`local name -> shape` for the guard sources this file imports."""
    out: dict = {}
    for alias, (segs, name, nested) in import_map(lines, mask).items():
        target = module_file(path, segs, nested, source_lines)
        if target is None:
            continue
        shape = sources_of.get(target, {}).get(name)
        if shape is not None:
            out[alias] = shape
    return out


def make_resolver(own: dict, theirs: dict, carriers: set[str], imported: dict | None = None):
    """`(name, qualifier, owner) -> shape | None`, looked up the way Rust
    would. Shared by `main` and by `--self-test` rather than written twice: a
    probe that resolves differently from the sweep proves nothing about the
    sweep."""
    near = carriers | {"Self", "super", "self", "fixtures", "tests", "crate"}

    def resolve(name, qualifier, owner=None):
        if qualifier == "ScratchDir":
            return BUILTIN_GUARD_SOURCES.get(name)
        if qualifier == "Self":
            return own.get((owner, name)) if owner else None
        if qualifier is not None and qualifier[:1].isupper():
            return own.get((qualifier, name))
        if qualifier is None:
            return own.get(name) or (imported or {}).get(name)
        if qualifier not in near:
            return None
        return own.get(name) or theirs.get(name)

    return resolve


def analyse_guards(rel: str, lines: list[str], whole_file: bool = False,
                   theirs: dict | None = None, imported: dict | None = None):
    """Everything rules 2 and 3 need for one file, in one call."""
    mask = test_region_mask(lines, whole_file)
    carriers = carrier_structs(lines, mask)
    structs = {m.group(2) for i, m in
               ((i, STRUCT_DEF.match(line)) for i, line in enumerate(lines))
               if m and mask[i]}
    sources = guard_sources(lines, mask, carriers)
    resolve = make_resolver(sources, theirs or {}, carriers, imported)
    return guard_rules(rel, lines, mask, resolve, carriers, structs, sources)


def enclosing_impl(lines: list[str], index: int) -> str | None:
    """The type of the `impl` block that contains line `index`, or `None` when
    the item is free.

    Bounded, unlike the first version, which walked back to the nearest `impl`
    *anywhere* above and so made every free helper inside `mod tests` an
    associated function of a production impl earlier in the file — 48 one-line
    helpers accused in one run. Only a line at a **lower indent** can be the
    container, and the first such line decides: an `impl` header means yes, a
    `mod` header or a closing brace means no."""
    if index >= len(lines):
        return None
    inner = indent_of(lines[index])
    for i in range(index - 1, -1, -1):
        line = lines[i]
        if not line.strip() or indent_of(line) >= inner:
            continue
        m = IMPL_RE.match(line)
        if m:
            return m.group(1)
        return None
    return None


def guard_rules(
    rel: str,
    lines: list[str],
    in_test: list[bool],
    resolve,
    carriers: set[str],
    structs: set[str],
    sources: dict[str, tuple[str, int]],
) -> tuple[list[tuple[str, int, str]], tuple[int, int]]:
    """Rules 2 and 3 over every call of a **guard source** in one file.

    `resolve(name, qualifier)` hands back the shape the callee returns, or
    `None` when the call is not one of ours. Returns the offenders and
    `(call sites, call sites that keep their guard)`."""
    offenders: list[tuple[str, int, str]] = []
    calls = accepted = 0
    rule3_seen: set[int] = set()
    in_guard_impl = scratchdir_impl_lines(lines)

    for i, line in enumerate(lines):
        if not in_test[i] or FN_RE.match(line) or i in in_guard_impl:
            # `ScratchDir`'s own impl is where a guard is created and handed
            # on; rule 4 already exempts it for the same reason.
            continue
        for m in CALL_TOKEN.finditer(line):
            if is_comment(line, m.start()):
                continue
            qual = qualifier_of(line, m.start())
            if qual == ".":
                continue
            fn_name, fn_line = enclosing_fn(lines, i)
            owner = enclosing_impl(lines, fn_line)
            shape = resolve(m.group(1), qual, owner)
            if shape is None:
                continue

            calls += 1
            kind, idx = shape
            enclosing_is_source = fn_name in sources or (owner, fn_name) in sources
            stmt = lines[statement_start(lines, i)].strip()
            tup = TUPLE_LET_ANY.match(stmt)
            ident = IDENT_LET.match(stmt)
            field = FIELD_ACCESS.search(line[m.end():])
            where = "" if idx == 0 else f" (the guard is element {idx})"

            if field and kind in ("tuple", "container"):
                # `pair(..).1` drops the guard with the temporary it came
                # from; `pair(..).0` keeps the guard and drops the path, which
                # is a shape a test may legitimately want.
                if int(field.group(1)) == idx and ident and ident.group(1) != "_":
                    accepted += 1
                else:
                    offenders.append(
                        (rel, i + 1, f"guard discarded by a field access{where}"))
            elif kind == "container" and ident and ident.group(1) != "_":
                # The collection owns every guard it holds.
                accepted += 1
            elif kind in ("tuple", "container") and tup:
                parts = split_top_level(tup.group(1))
                held = idx < len(parts) and parts[idx].split()[-1] != "_"
                if held:
                    accepted += 1
                else:
                    offenders.append((rel, i + 1, f"guard dropped at once{where}"))
            elif kind not in ("tuple", "container") and ident:
                if ident.group(1) == "_":
                    offenders.append((rel, i + 1, "guard dropped at once"))
                else:
                    accepted += 1
            elif ident or tup:
                # A binding of the wrong shape for what the callee returns.
                offenders.append((rel, i + 1, f"bound without its guard{where}"))
            elif enclosing_is_source:
                # A tail expression inside another guard source: the guard is
                # what that function hands on, so it is not dropped here.
                accepted += 1
            else:
                offenders.append((rel, i + 1, "used without binding its guard"))

            # Rule 3: the helper that calls a guard source and returns
            # something that cannot carry it.
            if fn_line in rule3_seen or is_test_fn(lines, fn_line):
                continue
            rule3_seen.add(fn_line)
            ret = return_type(signature(lines, fn_line))
            if not ret or "ScratchDir" in ret:
                continue
            base = ret.lstrip("&").split("<")[0].strip()
            if ret == "Self":
                base = enclosing_impl(lines, fn_line) or "Self"
            if base in carriers:
                continue
            if PATH_SHAPED.search(ret) or base in structs:
                offenders.append(
                    (rel, fn_line + 1,
                     f"returns a path whose guard it drops (fn {fn_name})"))

    return offenders, (calls, accepted)


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
            elif enclosing_fn(lines, i)[0] in TEST_ONLY_WRAPPERS.get(rel, {}):
                exempted += 1
            else:
                offenders.append((rel, i + 1, "hands the platform root to product code"))

    return offenders, (hand_built, root_args, exempted)


# --- the self-test -----------------------------------------------------------
#
# Each case is source this sweep has been wrong about, with the answer written
# down beside it. `expected` is the set of 1-based line numbers that must be
# reported; anything else — an extra accusation or a missing one — fails.

# Rule 5's wrapper exemption, keyed by file — so these cases carry their own
# `rel` rather than the shared probe name the others use.
WRAPPER_CASES: list[tuple[str, str, str, set[int]]] = [
    (
        "a test-only wrapper named in TEST_ONLY_WRAPPERS may hand the platform "
        "root to its `_in` sibling (ART-295)",
        "src-tauri/src/core/osinstall/scan.rs",
        """#[cfg(test)]
pub fn open_package(medium: &PackageMedium) -> CoreResult<Box<dyn MediaSource>> {
    open_package_staging_in(medium, &std::env::temp_dir())
}
""",
        set(),
    ),
    (
        "the same call in any other test function of that file is still an "
        "offender (ART-295)",
        "src-tauri/src/core/osinstall/scan.rs",
        """#[cfg(test)]
mod tests {
    #[test]
    fn stages_somewhere() {
        let _ = open_package_staging_in(&medium, &std::env::temp_dir());
    }
}
""",
        {5},
    ),
    (
        "a wrapper's name exempts nothing in a file the table does not list "
        "(ART-295)",
        "src-tauri/src/probe.rs",
        """#[cfg(test)]
pub fn open_package(medium: &PackageMedium) -> CoreResult<Box<dyn MediaSource>> {
    open_package_staging_in(medium, &std::env::temp_dir())
}
""",
        {3},
    ),
]

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


# The same, for rules 2 and 3. Every case is a shape the final review's probe
# ran against the sweep and found passing (I1) — the guard being vacuous over
# roughly a third of the converted sites — with its clean twin beside it, so a
# rule that stops accusing and a rule that starts over-accusing both fail here.

GUARD_TEST_CASES: list[tuple[str, str, set[int]]] = [
    (
        "a guard element dropped with `_` is an offender whatever the helper "
        "is called, and the same call bound properly is not",
        """#[cfg(test)]
mod tests {
    fn helper(tag: &str) -> (crate::core::ScratchDir, std::path::PathBuf, usize) {
        let (guard, dir) = crate::core::ScratchDir::pair("art-probe", tag);
        (guard, dir, 0)
    }

    #[test]
    fn t() {
        let (_, dir, _n) = helper("x");
        let (_guard, kept, _n) = helper("y");
    }
}
""",
        {10},
    ),
    (
        "a guard reached through `.1` is dropped with the temporary it came "
        "from; `.0` keeps it",
        """#[cfg(test)]
mod tests {
    #[test]
    fn t() {
        let dir = crate::core::ScratchDir::pair("art-probe", "field").1;
        let guard = crate::core::ScratchDir::pair("art-probe", "kept").0;
        let (_guard, both) = crate::core::ScratchDir::pair("art-probe", "ok");
    }
}
""",
        {5},
    ),
    (
        "a helper named anything at all that returns a path from a guard it "
        "drops (rule 3), beside one that hands the guard on",
        """#[cfg(test)]
mod tests {
    fn tmp(tag: &str) -> std::path::PathBuf {
        let (_guard, dir) = crate::core::ScratchDir::pair("art-probe", tag);
        dir
    }

    fn kept(tag: &str) -> (crate::core::ScratchDir, std::path::PathBuf) {
        crate::core::ScratchDir::pair("art-probe", tag)
    }
}
""",
        {3},
    ),
    (
        "a struct that owns the guard in a field is bound with a plain `let`, "
        "and its constructor is not a rule-3 offender",
        """#[cfg(test)]
mod tests {
    struct Fixture {
        dir: std::path::PathBuf,
        _guard: crate::core::ScratchDir,
    }

    impl Fixture {
        fn new(tag: &str) -> Self {
            let (guard, dir) = crate::core::ScratchDir::pair("art-probe", tag);
            Self { dir, _guard: guard }
        }
    }

    #[test]
    fn t() {
        let f = Fixture::new("x");
    }
}
""",
        set(),
    ),
    (
        "a collection of tuples is destructured in a `for` pattern, and the "
        "guard element still has to be held",
        """#[cfg(test)]
mod tests {
    fn sources(tag: &str) -> Vec<(crate::core::ScratchDir, &'static str)> {
        vec![]
    }

    #[test]
    fn t() {
        for (_guard, name) in sources("ok") {
            let _ = name;
        }
        for (_, name) in sources("dropped") {
            let _ = name;
        }
    }
}
""",
        {12},
    ),
]


# The two files that import a fixture helper and call it bare, as the sweep
# sees them: a module that owns the helper, and a consumer of it. The consumer
# is checked against the owner exactly as `main` checks
# `core/osinstall/scan.rs` against `core/osinstall/mod.rs`.

IMPORT_OWNER = """#[cfg(test)]
pub(crate) mod fixtures {
    pub fn scratch(tag: &str) -> (crate::core::ScratchDir, std::path::PathBuf) {
        crate::core::ScratchDir::pair("art-osinstall", tag)
    }
}
"""

IMPORT_CASES: list[tuple[str, str, set[int]]] = [
    (
        "a grouped `use crate::…::fixtures::{media, scratch, …}` then a bare "
        "call: the guard still has to be held (re-review N1)",
        """#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::osinstall::fixtures::{media, scratch, write_test_iso};

    #[test]
    fn t() {
        let (_guard, kept) = scratch("ok");
        let (_, dropped) = scratch("bad");
    }
}
""",
        {9},
    ),
    (
        "`use super::super::fixtures::scratch;` — the relative path, from "
        "inside `mod tests`, where the first `super` is the file's own module",
        """#[cfg(test)]
mod tests {
    use super::super::fixtures::scratch;
    use super::*;

    #[test]
    fn t() {
        let (_guard, kept) = scratch("ok");
        let dir = scratch("bad");
    }
}
""",
        {9},
    ),
    (
        "an `as` alias is followed to the helper it renames",
        """#[cfg(test)]
mod tests {
    use crate::core::osinstall::fixtures::scratch as fixture_dir;

    #[test]
    fn t() {
        let (_, dropped) = fixture_dir("bad");
    }
}
""",
        {7},
    ),
]


def self_test() -> int:
    """Run rules 4 and 5 over the synthetic cases above, plus the header's
    agreement with `ALLOWED_HAND_BUILT`. No repository file is touched."""
    failures = 0
    total = (len(SELF_TEST_CASES) + len(WRAPPER_CASES) + len(GUARD_TEST_CASES)
             + len(IMPORT_CASES) + 3)

    def check(name, found, expected):
        nonlocal failures
        got = {line for _, line, _ in found}
        ok = got == expected
        failures += 0 if ok else 1
        print(f"  {'PASS' if ok else 'FAIL'}  {name}")
        if not ok:
            print(f"        expected lines {sorted(expected)}, got {sorted(got)}")
            for _, line, why in sorted(found, key=lambda o: o[1]):
                print(f"        line {line}: {why}")

    for name, src, expected in SELF_TEST_CASES:
        lines = src.splitlines()
        mask = test_region_mask(lines, whole_file=False)
        found, _ = temp_dir_rules("src-tauri/src/probe.rs", lines, mask)
        check(name, found, expected)

    for name, rel, src, expected in WRAPPER_CASES:
        lines = src.splitlines()
        mask = test_region_mask(lines, whole_file=False)
        found, _ = temp_dir_rules(rel, lines, mask)
        check(name, found, expected)

    for name, src, expected in GUARD_TEST_CASES:
        found, _ = analyse_guards("src-tauri/src/probe.rs", src.splitlines())
        check(name, found, expected)

    # The imported helper lives in another "file", resolved through the `use`
    # line exactly as `main` resolves it.
    owner_lines = IMPORT_OWNER.splitlines()
    owner_mask = test_region_mask(owner_lines, whole_file=False)
    owner = guard_sources(owner_lines, owner_mask, carrier_structs(owner_lines, owner_mask))
    for name, src, expected in IMPORT_CASES:
        lines = src.splitlines()
        mask = test_region_mask(lines, whole_file=False)
        imported = {}
        for alias, (_segs, item, _nested) in import_map(lines, mask).items():
            shape = owner.get(item)
            if shape is not None:
                imported[alias] = shape
        found, _ = analyse_guards("src-tauri/src/probe.rs", lines, imported=imported)
        check(name, found, expected)

    # N2: the summary is built from the offenders it is handed, so a kind the
    # rules learn later cannot turn a report into a traceback.
    kinds = [
        ("src-tauri/src/probe.rs", 5, "guard discarded by a field access"),
        ("src-tauri/src/probe.rs", 9, "guard discarded by a field access (the guard is element 1)"),
        ("src-tauri/src/probe.rs", 12, "a kind no dictionary knows about"),
    ]
    import io
    import contextlib
    buffer = io.StringIO()
    try:
        with contextlib.redirect_stdout(buffer):
            code = report(kinds, 0, 0, 0, 0, 0, 0, 0, 0)
        printed = buffer.getvalue()
        tally = [line.strip() for line in printed.splitlines()]
        ok = (code == 1
              and "guard discarded by a field access: 2" in tally
              and any(line.startswith("a kind no dictionary knows about")
                      for line in tally))
    except Exception as err:  # the defect this case exists for
        ok = False
        printed = f"{type(err).__name__}: {err}"
    failures += 0 if ok else 1
    print(f"  {'PASS' if ok else 'FAIL'}  every offender kind reaches the summary, "
          f"including one the rules learned later (N2)")
    if not ok:
        print("        " + printed.strip().replace("\n", "\n        "))

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

    # And the wrapper table against the header, the same agreement (ART-295).
    wrappers_ok = True
    for wrappers in TEST_ONLY_WRAPPERS.values():
        for fn_name in wrappers:
            if f"`{fn_name}`" not in doc:
                wrappers_ok = False
                print(f"        {fn_name} is exempt but the header never names it")
    failures += 0 if wrappers_ok else 1
    n_wrappers = sum(len(v) for v in TEST_ONLY_WRAPPERS.values())
    print(f"  {'PASS' if wrappers_ok else 'FAIL'}  the header names every "
          f"TEST_ONLY_WRAPPERS entry ({n_wrappers} wrapper(s))")

    print(f"scratch-guard self-test: {total - failures}/{total} passed")
    return 1 if failures else 0


def main() -> int:
    offenders: list[tuple[str, int, str]] = []
    helpers = guarded_helpers = 0
    calls = accepted_calls = 0
    hand_built = exempted = root_args = 0
    guard_sources_found = 0
    rule3_seen: set[tuple[str, int]] = set()

    files = rust_files()
    src = {p: p.read_text(encoding="utf-8").splitlines() for p in files}
    # The whole population, discovered by return type rather than by name
    # (final review, I1): every function that hands a guard to its caller,
    # and every struct that owns one in a field.
    masks = {p: test_region_mask(src[p], gated_at_its_declaration(p)) for p in files}
    carriers_of = {p: carrier_structs(src[p], masks[p]) for p in files}
    structs_of = {
        p: {m.group(2) for i, m in
            ((i, STRUCT_DEF.match(line)) for i, line in enumerate(src[p]))
            if m and masks[p][i]}
        for p in files
    }
    sources_of = {p: guard_sources(src[p], masks[p], carriers_of[p]) for p in files}
    imports_of = {p: imported_sources(p, src[p], masks[p], src, sources_of) for p in files}

    def resolve_for(path: Path):
        """`(name, qualifier) -> shape | None`, looked up the way Rust
        would: `ScratchDir`'s own two constructors, then this file's own
        functions, then the directory module's (`super::`, `fixtures::`).

        A qualifier that is neither of those, nor a carrier struct in this
        file, means the call is somebody else's — `std::fs::write(` must
        not resolve to `core::cbm::d64::tests::write`, which is a guard
        source with the same name in the same file."""
        parent = path.parent / "mod.rs"
        return make_resolver(
            sources_of.get(path, {}),
            sources_of.get(parent, {}) if parent != path else {},
            carriers_of.get(path, set()),
            imports_of.get(path, {}),
        )

    for path in files:
        rel = path.relative_to(ROOT.parent.parent).as_posix()
        lines = src[path]
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
        in_test = masks[path]

        # Rules 4 and 5: the two shapes that carry no `scratch(` call at all.
        found, counted = temp_dir_rules(rel, lines, in_test)
        offenders.extend(found)
        hand_built += counted[0]
        root_args += counted[1]
        exempted += counted[2]

        guard_sources_found += len(sources_of[path])

        # Rules 1-3, over every guard source this file can reach.
        found, counted = guard_rules(rel, lines, in_test, resolve_for(path),
                                     carriers_of[path], structs_of[path],
                                     sources_of[path])
        offenders.extend(found)
        calls += counted[0]
        accepted_calls += counted[1]

        # Rule 1 keeps its name: a `fn scratch(` that hands back a bare path.
        for i, line in enumerate(lines):
            if not in_test[i] or not SCRATCH_DEF.match(line):
                continue
            helpers += 1
            ret = return_type(signature(lines, i))
            if "ScratchDir" in ret:
                guarded_helpers += 1
            elif PATH_SHAPED.search(ret):
                offenders.append((rel, i + 1, "returns a bare path"))
    if offenders:
        return report(offenders, guard_sources_found, helpers, guarded_helpers,
                      calls, accepted_calls, hand_built, root_args, exempted)

    print(f"scratch-guard sweep: clean — {guard_sources_found} guard sources "
          f"(by return type), {calls} call sites, {hand_built} hand-built "
          f"paths, {root_args} platform-root arguments ({exempted} exempt)")
    return 0


def report(offenders, guard_sources_found, helpers, guarded_helpers,
           calls, accepted_calls, hand_built, root_args, exempted) -> int:
    """Print every offender and the totals, and answer 1.

    The per-kind tally is built **from the offenders themselves**, not from a
    dictionary of kinds written down in advance: the fix wave added
    "guard discarded by a field access" and forgot to add its key, so the one
    shape that wave existed to catch was the one whose report the script could
    not print — a `KeyError` and no offender lines at all (re-review N2). A
    kind added later cannot crash this."""
    for rel, line, why in sorted(offenders):
        print(f"{rel}:{line}: {why}")

    counts: dict[str, int] = {}
    for _, _, why in offenders:
        kind = why.split(" (")[0]
        counts[kind] = counts.get(kind, 0) + 1

    print()
    print(f"guard sources (by return type)     : {guard_sources_found}")
    print(f"`fn scratch` helpers               : {helpers}"
          f" ({guarded_helpers} already hand out a guard)")
    print(f"call sites                         : {calls}"
          f" ({accepted_calls} keep their guard)")
    print(f"hand-built paths                   : {hand_built}")
    print(f"platform-root arguments            : {root_args}")
    print(f"exempt, with a reason              : {exempted}")
    for why, n in sorted(counts.items()):
        print(f"  {why:33s}: {n}")
    print(f"offenders                          : {len(offenders)}")
    return 1


if __name__ == "__main__":
    raise SystemExit(self_test() if "--self-test" in sys.argv[1:] else main())
