#!/usr/bin/env python3
"""Ask the live sources whether every shipped catalogue path resolves.

Not in CI — it leaves the machine. The same place and for the same reason as
scripts/rom-table-check.py and scripts/fat-oracle-check.py.

Exit 0 when every fetchable entry resolved; 1 otherwise, listing what did not.
"""
import json
import pathlib
import sys
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
CATALOGUE = ROOT / "src-tauri/src/core/sources/bundle/catalogue"
AMINET = "https://aminet.net/package/"
TIMEOUT = 20


def head_ok(url: str) -> tuple[bool, str]:
    request = urllib.request.Request(url, method="HEAD")
    try:
        with urllib.request.urlopen(request, timeout=TIMEOUT) as answer:
            return (200 <= answer.status < 300, str(answer.status))
    except urllib.error.HTTPError as err:
        return (False, f"HTTP {err.code}")
    except Exception as err:  # noqa: BLE001 - a network failure is a result
        return (False, type(err).__name__)


def main() -> int:
    checked = skipped = 0
    bad: list[str] = []
    for path in sorted(CATALOGUE.glob("*.json")):
        bundle = json.loads(path.read_text(encoding="utf-8"))
        for entry in bundle["entries"]:
            kind, body = next(iter(entry["source"].items()))
            if kind in ("user-supplied", "aminet-search"):
                skipped += 1
                print(f"  skip  {entry['id']:<20} ({kind})")
                continue
            if kind == "aminet":
                url = AMINET + body["path"]
            else:
                skipped += 1
                print(f"  skip  {entry['id']:<20} ({kind} — needs a configured mirror)")
                continue
            ok, why = head_ok(url)
            checked += 1
            print(f"  {'ok  ' if ok else 'FAIL'}  {entry['id']:<20} {url} [{why}]")
            if not ok:
                bad.append(f"{entry['id']}: {url} ({why})")

    print(f"\nchecked {checked}, skipped {skipped}, failed {len(bad)}")
    for line in bad:
        print(f"  {line}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
