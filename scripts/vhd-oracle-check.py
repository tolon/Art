#!/usr/bin/env python3
"""ART writes a dynamic VHD; Microsoft's own reader says what it is.

`core/card/build.rs`'s rule, applied to the format that replaces its 32 GiB
files: **a card is verified by something that is not ART.** ART's own tests
can only say that its writer and its reader agree, and a format mistake they
both share is invisible to them -- which is exactly what caught ART-032..035
on the ADF side and ART-102 on the FAT32 one.

The oracle here is the **Hyper-V PowerShell module**, which ships with Windows
Pro: `Get-VHD` is Microsoft's own implementation of the format ART is writing,
and `Test-VHD` is their validator. Neither needs administrator rights.

    python scripts/vhd-oracle-check.py

Not in CI: the Hyper-V module is not on a plain GitHub runner. Run it whenever
`core/vhd/write.rs` changes.

What is compared, and why each one:

  VhdType    Dynamic, not Fixed. The whole point of the exercise -- a fixed
             image would cost the full size and the file would look right.
  Size       the disk ART said it was making. Reads the footer's own field.
  FileSize   what it actually costs. Asserted to be a small fraction of Size:
             if the arithmetic that grows the file were wrong, this is where a
             32 GiB file would show up wearing a dynamic footer.
  BlockSize  from the dynamic disk header, so the header is being read too.
  Test-VHD   Microsoft's own validator, which checks the checksums ART
             computes by hand.
"""

import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SRC_TAURI = REPO / "src-tauri"

# 32 GiB: the size the work list's premise is about, and big enough that a
# writer which quietly allocated everything would be unmissable.
DISK_BYTES = 32 * 1024 * 1024 * 1024


def fail(message: str) -> None:
    print(f"  FAIL  {message}")


def powershell(script: str) -> str:
    result = subprocess.run(
        ["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
        capture_output=True,
        text=True,
    )
    return (result.stdout or "") + (result.stderr or "")


def main() -> int:
    if os.name != "nt":
        print("vhd-oracle-check: Windows only (the oracle is the Hyper-V module)")
        return 0

    have = powershell("[bool](Get-Command Get-VHD -ErrorAction SilentlyContinue)").strip()
    if "True" not in have:
        print(
            "vhd-oracle-check: SKIPPED -- Get-VHD is not on this machine.\n"
            "  Install the Hyper-V PowerShell module (Windows Pro):\n"
            "    Enable-WindowsOptionalFeature -Online "
            "-FeatureName Microsoft-Hyper-V-Management-PowerShell"
        )
        return 0

    problems = 0
    with tempfile.TemporaryDirectory(prefix="art-vhd-oracle-") as work:
        out = Path(work) / "art.vhd"
        print(f"vhd-oracle-check: writing a {DISK_BYTES:,}-byte dynamic VHD with ART")

        env = dict(os.environ)
        env["ART_VHD_OUT"] = str(out)
        env["ART_VHD_SIZE"] = str(DISK_BYTES)
        written = subprocess.run(
            [
                "cargo",
                "test",
                "write_a_vhd_for_the_oracle",
                "--",
                "--ignored",
                "--nocapture",
            ],
            cwd=SRC_TAURI,
            env=env,
            capture_output=True,
            text=True,
        )
        if written.returncode != 0:
            print(written.stdout)
            print(written.stderr)
            fail("ART could not write the image at all")
            return 1

        said = re.search(r"file=(\d+) blocks=(\d+)", written.stdout or "")
        if said:
            print(f"  ART says: {int(said.group(1)):,} bytes, {said.group(2)} blocks allocated")

        on_disk = out.stat().st_size
        print(f"  on disk : {on_disk:,} bytes")
        if said and int(said.group(1)) != on_disk:
            problems += 1
            fail(
                f"ART reported {int(said.group(1)):,} bytes and the file is {on_disk:,} "
                "-- `file_size()` and the writer disagree"
            )

        # --- the oracle ---------------------------------------------------
        raw = powershell(
            "$ErrorActionPreference='Stop'; "
            f"$v = Get-VHD -Path '{out}'; "
            "[pscustomobject]@{"
            "  VhdFormat = [string]$v.VhdFormat;"
            "  VhdType = [string]$v.VhdType;"
            "  Size = [int64]$v.Size;"
            "  FileSize = [int64]$v.FileSize;"
            "  BlockSize = [int64]$v.BlockSize;"
            "  LogicalSectorSize = [int64]$v.LogicalSectorSize;"
            f"  Valid = [bool](Test-VHD -Path '{out}')"
            "} | ConvertTo-Json -Compress"
        )
        match = re.search(r"\{.*\}", raw, re.S)
        if not match:
            print(raw)
            fail("Get-VHD refused the file ART wrote")
            return 1

        said_by_microsoft = json.loads(match.group(0))
        for key, value in said_by_microsoft.items():
            shown = f"{value:,}" if isinstance(value, int) else value
            print(f"  {key:<18} {shown}")

        checks = [
            ("VhdFormat", "VHD", "the file is not even a VHD to Microsoft's reader"),
            (
                "VhdType",
                "Dynamic",
                "a fixed image would cost the full size -- this is the whole exercise",
            ),
            ("Size", DISK_BYTES, "the footer's disk size is not what ART was asked for"),
            ("BlockSize", 2 * 1024 * 1024, "the dynamic disk header's block size is wrong"),
            ("LogicalSectorSize", 512, "the geometry ART computes gives the wrong sector size"),
            ("Valid", True, "Test-VHD rejects it -- most likely a checksum"),
        ]
        for key, want, why in checks:
            got = said_by_microsoft.get(key)
            if got != want:
                problems += 1
                fail(f"{key}: Microsoft says {got!r}, ART meant {want!r} -- {why}")

        # The saving itself, stated as a ratio rather than a byte count so the
        # message says what it is about.
        cost = said_by_microsoft.get("FileSize", on_disk)
        if cost > DISK_BYTES // 100:
            problems += 1
            fail(
                f"the image costs {cost:,} bytes for a {DISK_BYTES:,}-byte disk -- "
                "that is not a saving, and a dynamic writer that allocates everything "
                "is the defect this format exists to avoid"
            )
        else:
            print(
                f"  saving  : {cost:,} bytes for a {DISK_BYTES:,}-byte disk "
                f"({DISK_BYTES / max(cost, 1):,.0f}x)"
            )

    if problems:
        print(f"\nvhd-oracle-check: {problems} disagreement(s) with Microsoft's reader")
        return 1
    print("\nvhd-oracle-check: clean -- Microsoft's own reader agrees with every field")
    return 0


if __name__ == "__main__":
    sys.exit(main())
