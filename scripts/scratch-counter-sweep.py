"""Find — and optionally fix — every test scratch directory whose name is not
guaranteed unique within the process.

Cargo runs tests in parallel threads of **one** process, so two tests that
build the same directory name share it and whichever writes second hands the
other its fixture. That has been diagnosed four times in this codebase:
`net`'s test server (ART-059), `core::iso` (ART-164/ART-115, 5 failures in 40
runs across four different tests), and `core::cbm` (ART-173, 4 in 40).

## What counts as "not guaranteed unique"

Two shapes, and the second is the worse one:

  * `std::process::id()` — shared by every thread in the run, so on its own it
    distinguishes nothing between parallel tests.
  * `SystemTime::now()…as_nanos()` — **worse**: two threads can genuinely land
    in the same nanosecond reading (the Windows clock is coarse), and unlike
    the pid it *looks* unique, so it reads as if the problem were solved.

The first version of this script grepped only `std::process::id()` and
reported a clean zero while ~20 helpers keyed on `as_nanos()` alone were
invisible to it — a guard that passes vacuously, which is the same class of
defect it exists to find. Both shapes are searched now.

## How a site is classified

  1. Every `.rs` file under `src-tauri/src`.
  2. Every occurrence of either shape.
  3. *test* code — the site falls after the file's first `#[cfg(test)]`.
     Production code that names a file this way is a different question,
     reported and never touched.
  4. *already safe* — `fetch_add` or `test_scratch_id` appears inside the
     enclosing `fn` (walk back to the nearest `fn` at a lower indent, then
     forward to the next one).
  5. *path-building* — the enclosing `fn` mentions `temp_dir`, or the site is
     inside a `format!` whose literal begins `"art-`. An `as_nanos()` used for
     timing or as a seed is not a scratch name and is left alone.

## The fix

Both shapes are replaced by `crate::core::test_scratch_id()`, which is the
process id **plus** a process-wide atomic counter. Every call site already
formats the value with `{}`, so no format string changes and each edit reads
as a one-expression diff.

Usage:  python scripts/scratch-counter-sweep.py            # report only
        python scripts/scratch-counter-sweep.py --apply    # rewrite
"""

import io
import os
import re
import sys

ROOT = os.path.join('src-tauri', 'src')
REPLACEMENT = 'crate::core::test_scratch_id()'

PID = re.compile(r'std::process::id\(\)')
# `SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()`, with or
# without the `std::time::` prefixes and across any line breaks.
NANOS = re.compile(
    r'(?:std::time::)?SystemTime::now\(\)\s*'
    r'\.duration_since\(\s*(?:std::time::)?UNIX_EPOCH\s*\)\s*'
    r'\.unwrap\(\)\s*'
    r'\.as_nanos\(\)'
)
FN_RE = re.compile(r'^(\s*)(pub(\([^)]*\))? )?(async )?fn (\w+)')


def rust_files():
    out = []
    for base, _, names in os.walk(ROOT):
        for name in names:
            if name.endswith('.rs'):
                out.append(os.path.join(base, name))
    return sorted(out)


def enclosing_fn(lines, index):
    """(name, start, end) of the fn containing line `index`."""
    start = index
    while start > 0 and not FN_RE.match(lines[start]):
        start -= 1
    m = FN_RE.match(lines[start])
    name = m.group(5) if m else '<file>'
    indent = len(m.group(1)) if m else 0
    end = start + 1
    while end < len(lines):
        m2 = FN_RE.match(lines[end])
        if m2 and len(m2.group(1)) <= indent:
            break
        end += 1
    return name, start, end


def main():
    apply = '--apply' in sys.argv
    counts = {'production': 0, 'safe': 0, 'not-a-path': 0, 'fixed': 0}
    report = []

    for path in rust_files():
        text = io.open(path, encoding='utf-8', newline='').read()
        if not (PID.search(text) or NANOS.search(text)):
            continue

        lines = text.split('\n')
        cfg_test = next((i for i, l in enumerate(lines) if '#[cfg(test)]' in l), None)
        cfg_off = len('\n'.join(lines[:cfg_test])) if cfg_test is not None else None

        # Collect every match across both shapes, right to left so earlier
        # offsets stay valid while rewriting.
        hits = sorted(
            [(m.start(), m.end(), 'pid') for m in PID.finditer(text)]
            + [(m.start(), m.end(), 'nanos') for m in NANOS.finditer(text)],
            reverse=True,
        )

        changed = False
        for begin, finish, shape in hits:
            line_no = text.count('\n', 0, begin)
            if cfg_off is None or begin < cfg_off:
                counts['production'] += 1
                report.append(('PRODUCTION', path, line_no + 1, '-', shape))
                continue

            name, fn_start, fn_end = enclosing_fn(lines, line_no)
            body = '\n'.join(lines[fn_start:fn_end])

            if 'fetch_add' in body or 'test_scratch_id' in body:
                counts['safe'] += 1
                report.append(('already-safe', path, line_no + 1, name, shape))
                continue

            # Is this a scratch *path* at all?
            window = text[max(0, begin - 400):finish]
            if 'temp_dir' not in body and '"art-' not in window:
                counts['not-a-path'] += 1
                report.append(('not-a-path', path, line_no + 1, name, shape))
                continue

            counts['fixed'] += 1
            report.append(('FIXED' if apply else 'needs-counter',
                           path, line_no + 1, name, shape))
            if apply:
                text = text[:begin] + REPLACEMENT + text[finish:]
                changed = True

        if apply and changed:
            io.open(path, 'w', encoding='utf-8', newline='').write(text)

    for kind, path, line, fn, shape in sorted(report):
        if kind in ('needs-counter', 'FIXED', 'PRODUCTION', 'not-a-path'):
            print(f'  {kind:14s} {shape:6s} {path}:{line}  fn {fn}')

    total = sum(counts.values())
    print()
    print(f'sites total                 : {total}')
    print(f'  production (never touched): {counts["production"]}')
    print(f'  not a scratch path        : {counts["not-a-path"]}')
    print(f'  already had a counter     : {counts["safe"]}')
    label = 'rewritten' if apply else 'needing a counter'
    print(f'  {label:26s}: {counts["fixed"]}')


if __name__ == '__main__':
    main()
