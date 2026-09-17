#!/usr/bin/env python3
"""Check ART's LZX reader against unar (XADMaster), an implementation sharing no code.

ART's LZX decoder (`src-tauri/src/core/archive/lzx.rs`) and the synthetic
streams its tests build were written from the same description of the format.
They can agree with each other and both be wrong, and every test would still
pass. The member CRC-32 the archiver stored catches a wrong byte, but not a
member written under the wrong name, a member missing, or a member the gate
wrote that the archive does not hold. Only an outside implementation closes
that gap.

    ART unpacks each archive  →  unar unpacks it  →  the two trees must agree

What it checks, per `.lzx` directly in DIR:

  * every relative path either side wrote exists on the other side
    (compared after Unicode NFC: the names are Latin-1 in the archive)
  * every file's bytes, by SHA-256

ART's side runs through the `#[ignore]`d hook
`read_the_owners_lzx_archives_when_asked`, so it is the product's own gate
(`extract_with_backend`) doing the writing, and the hook itself requires
every entry written and no error. unar runs with `-e ISO-8859-1`, because
its encoding guess is not reliable on Amiga names.

What it does not prove: dates (ART claims none — R2 § A3 found three
decoders reading three years), protection bits and comments (unar does not
write them to a host folder), and anything about archives that are not in
DIR. XADMaster is one outside reading, not the Amiga archiver itself.

Usage:

    python scripts/lzx-oracle-check.py DIR --out OUTDIR

OUTDIR must not be on C: and its `art` and `unar` folders must be empty or
absent. Requires unar: `ART_UNAR`, default
`E:\\amiga\\ProjeART\\build\\tmp\\card-r2\\lzx\\unar\\unar.exe` (the Windows
build 1.8.1 from <https://cdn.theunarchiver.com/downloads/unarWindows.zip>).
If unar or the hook cannot run, this script **fails** — an oracle that
quietly skips itself is not an oracle.

Not in CI: it needs the owner's archives and unar.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import subprocess
import sys
import unicodedata
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DEFAULT_UNAR = r"E:\amiga\ProjeART\build\tmp\card-r2\lzx\unar\unar.exe"
HOOK = "read_the_owners_lzx_archives_when_asked"


def tree(root: Path) -> dict[str, str]:
    """Relative path (NFC, `/`) → SHA-256 of every file under root."""
    out: dict[str, str] = {}
    for dirpath, _, files in os.walk(root):
        for name in files:
            path = Path(dirpath) / name
            rel = unicodedata.normalize("NFC", path.relative_to(root).as_posix())
            digest = hashlib.sha256()
            with open(path, "rb") as f:
                for chunk in iter(lambda: f.read(1 << 20), b""):
                    digest.update(chunk)
            out[rel] = digest.hexdigest()
    return out


def require_empty(path: Path) -> None:
    if path.exists() and any(path.iterdir()):
        sys.exit(f"{path} already holds files; give --out a fresh folder")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("dir", type=Path, help="a folder holding .lzx archives")
    parser.add_argument("--out", type=Path, required=True, help="where both sides unpack")
    args = parser.parse_args()

    out = args.out.resolve()
    if out.drive.upper() == "C:":
        sys.exit("--out must not be on C:")
    archives = sorted(p for p in args.dir.iterdir() if p.is_file() and p.suffix.lower() == ".lzx")
    if not archives:
        sys.exit(f"no .lzx directly in {args.dir}")
    unar = Path(os.environ.get("ART_UNAR", DEFAULT_UNAR))
    if not unar.is_file():
        sys.exit(f"unar not found at {unar}; set ART_UNAR")

    art_root, unar_root = out / "art", out / "unar"
    require_empty(art_root)
    require_empty(unar_root)
    out.mkdir(parents=True, exist_ok=True)

    # ART's side: the product's gate, through the #[ignore]d hook.
    env = dict(os.environ)
    env["ART_LZX_DIR"] = str(args.dir.resolve())
    env["ART_LZX_OUT"] = str(art_root)
    scratch = str(out / "tmp")
    Path(scratch).mkdir(exist_ok=True)
    env["TMP"] = env["TEMP"] = scratch
    log = out / "art-hook.txt"
    with open(log, "wb") as f:
        result = subprocess.run(
            ["cargo", "test", "--lib", HOOK, "--", "--ignored", "--nocapture"],
            cwd=REPO / "src-tauri",
            env=env,
            stdout=f,
            stderr=subprocess.STDOUT,
        )
    text = log.read_text(encoding="utf-8", errors="replace")
    for line in text.splitlines():
        if ".lzx:" in line or line.startswith("test result:"):
            print(f"art: {line}")
    if result.returncode != 0 or "test result: ok. 1 passed" not in text:
        print(f"FAIL: the hook did not pass (exit {result.returncode}); see {log}")
        return 1

    failed = False
    for archive in archives:
        stem = archive.stem
        dest = unar_root / stem
        dest.mkdir(parents=True, exist_ok=True)
        unar_log = out / f"unar-{stem}.txt"
        with open(unar_log, "wb") as f:
            result = subprocess.run(
                [str(unar), "-q", "-D", "-e", "ISO-8859-1", "-o", str(dest), str(archive)],
                stdout=f,
                stderr=subprocess.STDOUT,
            )
        if result.returncode != 0:
            print(f"FAIL: unar exited {result.returncode} on {archive.name}; see {unar_log}")
            failed = True
            continue

        a, b = tree(art_root / stem), tree(dest)
        identical = sorted(k for k in a if k in b and a[k] == b[k])
        differ = sorted(k for k in a if k in b and a[k] != b[k])
        only_art = sorted(set(a) - set(b))
        only_unar = sorted(set(b) - set(a))
        print(
            f"{archive.name}: files {len(a)} {len(b)} identical {len(identical)} "
            f"differ {len(differ)} only-art {len(only_art)} only-unar {len(only_unar)}"
        )
        for label, names in (("differ", differ), ("only-art", only_art), ("only-unar", only_unar)):
            for name in names[:10]:
                print(f"  {label}: {name}")
        if differ or only_art or only_unar or not a:
            failed = True

    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
