"""Find — and optionally fix — every test scratch directory that is keyed on
`std::process::id()` without a per-call counter.

How it decides, so the next person can re-run this rather than re-trust it:

  1. Every `.rs` file under `src-tauri/src`.
  2. Every occurrence of `std::process::id()`.
  3. A site counts as *test* code when it appears after the first
     `#[cfg(test)]` in the file — production code that named a file by pid
     would be a different question and is reported separately, not touched.
  4. A site is *already safe* when `fetch_add` appears within the enclosing
     `fn` (walking back to the nearest line matching `fn <name>` at a lower
     indent, then forward to the next such line).

The fix is one token: `std::process::id()` becomes
`crate::core::test_scratch_id()`, which is pid **plus** a process-wide
counter. Every call site formats it with `{}` already, so no format string
changes and every edit is reviewable as a one-word diff.

Usage:  python scratch_sweep.py            # report only
        python scratch_sweep.py --apply    # rewrite
"""

import io
import os
import re
import sys

ROOT = 'src'
NEEDLE = 'std::process::id()'
FN_RE = re.compile(r'^(\s*)(pub(\([^)]*\))? )?(async )?fn (\w+)')


def files():
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
    total = safe = fixed = production = 0
    report = []

    for path in files():
        text = io.open(path, encoding='utf-8', newline='').read()
        if NEEDLE not in text:
            continue
        lines = text.split('\n')
        cfg_test = next((i for i, l in enumerate(lines) if '#[cfg(test)]' in l), None)

        changed = False
        for i, line in enumerate(lines):
            if NEEDLE not in line:
                continue
            total += 1
            if cfg_test is None or i < cfg_test:
                production += 1
                report.append(('PRODUCTION', path, i + 1, '-'))
                continue
            name, start, end = enclosing_fn(lines, i)
            body = '\n'.join(lines[start:end])
            if 'fetch_add' in body:
                safe += 1
                report.append(('already-safe', path, i + 1, name))
                continue
            fixed += 1
            report.append(('FIXED' if apply else 'needs-counter', path, i + 1, name))
            if apply:
                lines[i] = line.replace(NEEDLE, 'crate::core::test_scratch_id()')
                changed = True

        if apply and changed:
            io.open(path, 'w', encoding='utf-8', newline='').write('\n'.join(lines))

    for kind, path, line, fn in report:
        if kind in ('needs-counter', 'FIXED', 'PRODUCTION'):
            print(f'  {kind:14s} {path}:{line}  fn {fn}')

    print()
    print(f'sites total          : {total}')
    print(f'  production (skipped): {production}')
    print(f'  already had counter : {safe}')
    print(f'  {"rewritten" if apply else "needing a counter"}   : {fixed}')


if __name__ == '__main__':
    main()
