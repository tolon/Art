#!/usr/bin/env python3
"""Check ART's ILBM writer with an independent decoder.

ART's own ILBM tests (`core::ilbm`) prove the encoder against itself: they
pack a `BODY` and then unpack it with the same `packbits` module that packed
it. That catches a compressor that cannot round-trip its own output; it
cannot catch a `BMHD` field in the wrong place, a plane laid out backwards, or
a chunk boundary that only this encoder's own reader would tolerate — the
same class of defect `scripts/oracle-check.py` exists for on the disk-image
side (ART-032 .. ART-035).

So this script hands ART's own ILBM files to something that shares no code
with ART: ffmpeg's `iff_ilbm` decoder.

    ART writes  ->  ffmpeg reads     proves ART's ILBM writer

There is no reverse direction here (ffmpeg has no ILBM *encoder* to write one
for ART to read) — the same one-way shape `fat-oracle-check.py` and
`iso-oracle-check.py` use, both read-only checks against a tool that only
reads the format.

ffmpeg was chosen by running it, not by listing it: five real Amiga `.iff`
files decoded correctly here, one of them matching a `BMHD` read by hand byte
for byte. Pillow 12.3.0 was tried first and refuted — `UnidentifiedImageError`
on every one of those files; it cannot read ILBM at all. Pillow *is* used
below, but only to read the PNG ffmpeg produces, which is a wholly different
question.

Usage:

    python scripts/ilbm-oracle-check.py

The fixtures come from **the product's own encoder**, through one mechanism:
an `#[ignore]`d Rust test (`write_the_ilbm_oracle_fixtures`, in
`core/ilbm/mod.rs`) writes a set of `.iff` files plus a `manifest.json`
recording the RGB pixels each one means, into the directory this script names
with `ART_ILBM_OUT`. No second encoder exists anywhere in this repository.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CARGO_DIR = REPO / "src-tauri"


def find_ffmpeg() -> str | None:
    """PATH first; then the one place this machine is known to have it."""
    on_path = shutil.which("ffmpeg")
    if on_path:
        return on_path

    packages = Path(os.environ.get("LOCALAPPDATA", "")) / "Microsoft" / "WinGet" / "Packages"
    if not packages.is_dir():
        return None
    for candidate in sorted(packages.glob("Gyan.FFmpeg*/**/ffmpeg.exe")):
        return str(candidate)
    return None


def run_cargo(test: str, env: dict[str, str]) -> str:
    merged = {**os.environ, **env}
    result = subprocess.run(
        ["cargo", "test", "--quiet", test, "--", "--ignored", "--nocapture"],
        cwd=CARGO_DIR,
        env=merged,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    output = result.stdout + result.stderr
    if result.returncode != 0:
        raise RuntimeError(f"cargo test {test} exited {result.returncode}\n{output}")
    return output


def decode_with_ffmpeg(ffmpeg: str, iff_path: Path, png_path: Path) -> None:
    result = subprocess.run(
        [ffmpeg, "-y", "-loglevel", "error", "-i", str(iff_path), str(png_path)],
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    if result.returncode != 0 or not png_path.exists():
        raise RuntimeError(
            f"ffmpeg could not decode {iff_path.name}:\n{result.stdout}\n{result.stderr}"
        )


def compare_pixels(name: str, png_path: Path, width: int, height: int, want_rgb: list[list[int]]) -> str | None:
    """Return a failure message, or None if every pixel matches."""
    try:
        from PIL import Image
    except ImportError:
        return "Pillow is not installed. Run: pip install Pillow"

    import warnings

    with warnings.catch_warnings():
        warnings.simplefilter("ignore")
        image = Image.open(png_path).convert("RGB")

        if image.size != (width, height):
            return f"{name}: ffmpeg decoded {image.size}, fixture is {(width, height)}"

        got = list(image.getdata())
    for index, (pixel, want) in enumerate(zip(got, want_rgb)):
        if list(pixel) != want:
            x, y = index % width, index // width
            return (
                f"{name}: first mismatch at ({x}, {y}): "
                f"ffmpeg decoded {tuple(pixel)}, ART meant {tuple(want)}"
            )
    return None


def main() -> int:
    ffmpeg = find_ffmpeg()
    if ffmpeg is None:
        print(
            "ffmpeg not found, nothing was checked. Looked on PATH and under "
            "%LOCALAPPDATA%\\Microsoft\\WinGet\\Packages\\Gyan.FFmpeg*. "
            "Install it (winget install Gyan.FFmpeg) and re-run."
        )
        return 2
    print(f"using ffmpeg: {ffmpeg}")

    work = Path(tempfile.mkdtemp(prefix="art-ilbm-oracle-"))
    failures: list[str] = []
    try:
        fixtures_dir = work / "fixtures"
        print("Writing fixtures from ART's own ILBM encoder:")
        run_cargo("write_the_ilbm_oracle_fixtures", {"ART_ILBM_OUT": str(fixtures_dir)})

        manifest_path = fixtures_dir / "manifest.json"
        if not manifest_path.exists():
            print("  FAIL the fixture-writer hook produced no manifest.json")
            return 1
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        if not manifest:
            print("  FAIL the manifest is empty; nothing was checked")
            return 1
        print(f"  ok   {len(manifest)} fixture(s) written")

        print("\nDecoding each with ffmpeg's iff_ilbm decoder and comparing every pixel:")
        for entry in manifest:
            iff_path = fixtures_dir / entry["file"]
            png_path = fixtures_dir / (Path(entry["file"]).stem + ".png")
            decode_with_ffmpeg(ffmpeg, iff_path, png_path)
            failure = compare_pixels(
                entry["file"], png_path, entry["width"], entry["height"], entry["rgb"]
            )
            if failure:
                print(f"  FAIL {failure}")
                failures.append(failure)
            else:
                print(f"  ok   {entry['file']} ({entry['width']}x{entry['height']}) - every pixel matches")
    finally:
        shutil.rmtree(work, ignore_errors=True)

    if failures:
        print(f"\n{len(failures)} fixture(s) disagreed with an independent decoder:")
        for failure in failures:
            print(f"  - {failure}")
        print(
            "\nffmpeg and ART's own ILBM encoder disagree about what a file's "
            "pixels are. That means the file is wrong, not the decoder."
        )
        return 1

    print(f"\nffmpeg agrees with ART's ILBM encoder on all {len(manifest)} fixture(s).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
