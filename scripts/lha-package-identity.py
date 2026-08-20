"""ART-167: what a package archive says its own identity is, and what does not.

`core::osinstall::scan::find_packages` identifies a package archive by the
single **top-level directory** inside it — never by its filename — so a
renamed file still resolves. Eight of the owner's own archives carry
`LocaleUpdate` and two carry `BoingBag3.9-2`, which is the whole of ART-167:
the rule is right and is not sufficient on its own.

This script is the *outside* check for that. It re-implements the top-level
rule from `source_archive.rs`'s own module doc rather than calling ART, so
ART is not the only witness to what its own archives contain, and it prints:

  1. every `.lha` under the roots below, with the top-level directory the
     rule reads from inside it;
  2. every top-level name claimed by more than one archive;
  3. for each such collision group, what actually separates the members —
     the entries common to all of them, and each one's own paths.

Run with no arguments for the default root, or pass one or more folders.

    python scripts/lha-package-identity.py [FOLDER ...]

The LHA header parser is `scripts/lha-header-census.py`'s, imported rather
than copied, so a fix to one is a fix to both.
"""

import glob
import importlib.util
import os
import sys

DEFAULT_ROOTS = [r"E:\amiga\Amigatolon\paketler"]

_HERE = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location(
    "lha_header_census", os.path.join(_HERE, "lha-header-census.py")
)
census = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(census)


def entry_path(rec):
    """One entry's full path, `/`-separated.

    A level-0 name carries the whole path in one field; a level-1/2 entry
    keeps its drawer in a `0x02` extended header whose separator is `0xFF`.
    Decoded Latin-1, which is what `core::lha::entry_path` does since
    ART-168 and what an Amiga archiver actually wrote.
    """
    base = rec["name"].split(b"\0", 1)[0]
    drawer = rec["dir"]
    if drawer:
        parts = [p for p in drawer.split(b"\xff") if p]
        raw = b"/".join(parts + [base]) if base else b"/".join(parts) + b"/"
    else:
        raw = base
    return raw.replace(b"\\", b"/").decode("latin-1")


def top_level(paths):
    """`source_archive.rs`'s rule, restated.

    An entry that nests something (two or more segments) or that is itself a
    directory contributes its first segment as a candidate; a bare file at
    the archive's own root (`BoingBag3.9-1.info`) contributes nothing. Zero
    candidates or more than one is a refusal, not a guess.
    """
    candidates = set()
    for path in paths:
        segments = [s for s in path.split("/") if s]
        if not segments:
            continue
        if len(segments) >= 2 or path.endswith("/"):
            candidates.add(segments[0])
    if len(candidates) == 1:
        return next(iter(candidates))
    if not candidates:
        return "<refused: no top-level directory>"
    return "<refused: %d top-level: %s>" % (
        len(candidates),
        ", ".join(sorted(candidates)),
    )


def main(argv):
    roots = argv[1:] or DEFAULT_ROOTS
    files = []
    for root in roots:
        files += glob.glob(os.path.join(root, "**", "*.lha"), recursive=True)
    files = sorted(set(files))
    if not files:
        print("no .lha archives under: %s" % ", ".join(roots))
        return 1

    rows = []
    for path in files:
        try:
            paths = [entry_path(r) for r in census.entries(path)]
        except Exception as exc:  # a malformed archive is data, not a crash
            rows.append((path, "<unreadable: %s>" % exc, []))
            continue
        rows.append((path, top_level(paths), paths))

    common_root = os.path.commonpath([os.path.dirname(p) for p in files])
    print("%d .lha archives under %s" % (len(rows), ", ".join(roots)))
    print()
    print("%-50s  %s" % ("archive", "top-level directory, read from inside"))
    print("-" * 100)
    for path, top, paths in rows:
        print(
            "%-50s  %-34s (%d entries)"
            % (os.path.relpath(path, common_root), top, len(paths))
        )

    groups = {}
    for path, top, paths in rows:
        if not top.startswith("<"):
            groups.setdefault(top, []).append((path, paths))

    print()
    print("=== top-level names claimed by more than one archive ===")
    collisions = {t: g for t, g in groups.items() if len(g) > 1}
    if not collisions:
        print("  none")
    for top, group in sorted(collisions.items()):
        print("  %s -- %d archives" % (top, len(group)))
        for path, _ in group:
            print("      %s" % os.path.basename(path))

    print()
    print("=== what separates them, read from inside ===")
    for top, group in sorted(collisions.items()):
        shared = set(group[0][1])
        for _, paths in group[1:]:
            shared &= set(paths)
        print("  %s" % top)
        print("    entries common to all %d: %d" % (len(group), len(shared)))
        for name in sorted(shared):
            print("        %s" % name)
        for path, paths in group:
            own = sorted(set(paths) - shared)
            second = sorted({p.split("/")[1] for p in own if len(p.split("/")) > 1})
            print(
                "    %s: %d entries of its own, second level %s"
                % (os.path.basename(path), len(own), second)
            )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
