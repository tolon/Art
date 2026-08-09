#!/usr/bin/env python3
"""Check ART's disk images with an independent implementation.

ART's own test suite cannot catch a format mistake that its reader and writer
*share*: a wrong checksum algorithm, a field one longword out of place, a
bitmap laid out backwards. Every one of those round-trips through ART
perfectly and is rejected by AmigaOS. Four of them shipped before anyone asked
an outside tool (ART-032 … ART-035).

So this script checks ART against `amitools` — a separate implementation with
no shared code — in **both** directions, and fails if the two disagree:

    ART writes  →  amitools reads     proves ART's writer
    amitools writes  →  ART reads     proves ART's reader

Both halves are needed. A reader and a writer that agree with each other and
with nothing else is exactly what ART-032 … ART-035 were.

Usage:

    pip install amitools
    python scripts/oracle-check.py

The fixtures are built through `#[test]` hooks that do nothing unless their
environment variable is set, so nothing here depends on ART internals beyond
those few paths.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CARGO_DIR = REPO / "src-tauri"

failures: list[str] = []


def run(command: list[str], env: dict[str, str] | None = None) -> str:
    """Run a command and return its output, or raise with the output attached."""
    merged = {**os.environ, **(env or {})}
    result = subprocess.run(
        command,
        cwd=CARGO_DIR,
        env=merged,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"{' '.join(command)} exited {result.returncode}\n"
            f"{result.stdout}\n{result.stderr}"
        )
    return result.stdout + result.stderr


def oracle(command: list[str]) -> str:
    """Run an amitools command. Its failures are results, not crashes."""
    result = subprocess.run(
        command, capture_output=True, text=True, encoding="utf-8", errors="replace"
    )
    return result.stdout + result.stderr


def expect(label: str, haystack: str, needle: str) -> None:
    if needle in haystack:
        print(f"  ok   {label}")
    else:
        print(f"  FAIL {label}: expected {needle!r}")
        print("       got:")
        for line in haystack.splitlines()[:12]:
            print(f"         {line}")
        failures.append(label)


def check_adf(work: Path) -> None:
    """A floppy ART created, with a file ART added."""
    print("ADF written by ART, read by amitools:")
    image = work / "art.adf"
    run(
        ["cargo", "test", "--quiet", "export_adf_for_oracle_when_asked"],
        {"ART_ADF_OUT": str(image)},
    )
    if not image.exists():
        failures.append("the ADF export hook wrote nothing")
        print("  FAIL the ADF export hook wrote nothing")
        return

    listing = oracle(["xdftool", str(image), "list"])
    # A wrong checksum (ART-033) makes this "Invalid Root Block".
    expect("volume is readable", listing, "Work")
    # A misplaced size field (ART-034) makes this 0.
    expect("file is listed with its real size", listing, "Readme")
    expect("size is not zero", listing, "14")

    contents = oracle(["xdftool", str(image), "type", "Readme"])
    expect("file contents survive the round trip", contents, "hello from ART")


def check_hdf(work: Path) -> None:
    """A partitioned hard disk ART created, with a volume inside it."""
    print("HDF written by ART, read by amitools:")
    image = work / "art.hdf"
    run(
        ["cargo", "test", "--quiet", "export_rdb_for_oracle_when_asked"],
        {"ART_ORACLE_OUT": str(image)},
    )
    if not image.exists():
        failures.append("the RDB export hook wrote nothing")
        print("  FAIL the RDB export hook wrote nothing")
        return

    # The partition table. A DosType in the wrong longword (ART-032) shows up
    # here as 0x00000000, and the priority field holds the ASCII of "DOS\1".
    table = oracle(["rdbtool", str(image), "show"])
    expect("partition declares its filesystem", table, "0x444f5301")
    expect("boot priority is a priority", table, "boot_pri:       0")

    # The filesystem inside the partition.
    listing = oracle(["xdftool", str(image), "open", "part=0", "+", "list"])
    expect("partition volume is readable", listing, "Work")
    expect("file inside the partition is listed", listing, "Readme")

    contents = oracle(["xdftool", str(image), "open", "part=0", "+", "type", "Readme"])
    expect("file contents survive inside a partition", contents, "hello from ART")


def export(variable: str, test: str, target: Path) -> bool:
    """Run one export hook and say whether it produced a file."""
    run(["cargo", "test", "--quiet", test], {variable: str(target)})
    if target.exists():
        return True
    print(f"  FAIL the {test} hook wrote nothing")
    failures.append(f"{test} wrote nothing")
    return False


def check_written_volume(work: Path) -> None:
    """A whole tree written by the Stage W writer, read back by amitools.

    The four bugs this exists to catch all had the same shape: ART's writer and
    ART's reader agreed, and nothing else did. So none of these assertions goes
    through ART — the image is produced by ART and every claim about it comes
    from a separate implementation.
    """
    print("FFS volume written by ART's volume writer:")
    image = work / "written.adf"
    if not export("ART_W_ADF_OUT", "export_written_adf_for_oracle_when_asked", image):
        return

    listing = oracle(["xdftool", str(image), "list"])
    expect("volume is readable", listing, "Work")
    expect("a file in the root is listed", listing, "Readme")
    expect("a folder is listed", listing, "Tools")
    # A directory ART created has to be walkable, not merely present.
    expect("a nested file is listed", listing, "Notes.txt")
    # A file long enough to need an extension block: 100 blocks of 512.
    expect("a file with an extension chain is listed", listing, "Large.bin")
    expect("its size is right", listing, "51200")

    contents = oracle(["xdftool", str(image), "type", "Readme"])
    expect("file contents survive", contents, "hello from ART")

    nested = oracle(["xdftool", str(image), "type", "Tools/Notes.txt"])
    expect("a nested file's contents survive", nested, "nested file")


def check_written_ofs(work: Path) -> None:
    """The same tree on OFS, where every data block carries its own header."""
    print("OFS volume written by ART's volume writer:")
    image = work / "written-ofs.adf"
    if not export("ART_W_OFS_OUT", "export_written_ofs_adf_for_oracle_when_asked", image):
        return

    listing = oracle(["xdftool", str(image), "list"])
    expect("OFS volume is readable", listing, "Work")
    expect("OFS file is listed", listing, "Readme")
    expect("OFS extension chain is listed", listing, "Large.bin")

    # OFS data blocks carry a sequence number and a checksum of their own. A
    # wrong one shows up here as unreadable content, not as a bad listing.
    contents = oracle(["xdftool", str(image), "type", "Readme"])
    expect("OFS file contents survive", contents, "hello from ART")

    medium = oracle(["xdftool", str(image), "type", "Medium.bin"])
    expect(
        "an OFS file spanning several data blocks reads back",
        # 1500 bytes at 488 per block is four blocks; any of them wrong and
        # this errors instead of producing output.
        "error" if "rror" in medium else "ok",
        "ok",
    )


def check_written_large(work: Path) -> None:
    """A volume that is not floppy-shaped: 64 MiB, 33 bitmap blocks.

    This is the whole point of the generalisation. Floppy code that assumes one
    bitmap block and a root at 880 cannot produce a readable image at this
    size, and only an outside implementation can say whether it did.
    """
    print("64 MiB volume written by ART's volume writer:")
    image = work / "written-large.hdf"
    if not export(
        "ART_W_LARGE_OUT", "export_written_large_volume_for_oracle_when_asked", image
    ):
        return

    listing = oracle(["xdftool", str(image), "list"])
    expect("a non-floppy volume is readable", listing, "Work")
    expect("its root block is where ART put it", listing, "Readme")
    expect("a folder on it is listed", listing, "Tools")
    expect("an extension chain on it is listed", listing, "Large.bin")

    contents = oracle(["xdftool", str(image), "type", "Tools/Notes.txt"])
    expect("a nested file on a large volume reads back", contents, "nested file")


def check_written_attributes(work: Path) -> None:
    """Protection bits, the single easiest thing to get backwards.

    `RWED` are stored inverted — a clear bit grants the right. Writing them the
    obvious way round produces files nobody can read, and it looks correct
    until someone boots the disk.
    """
    print("Protection bits written by ART:")
    image = work / "written-attrs.adf"
    if not export(
        "ART_W_ATTR_OUT", "export_written_attributes_for_oracle_when_asked", image
    ):
        return

    listing = oracle(["xdftool", str(image), "list"])
    expect("the file is listed", listing, "Game.slave")
    # amitools spells the bits the same way WinUAE does.
    expect("the S and P bits are set and RWED are granted", listing, "-sp-rwed")
    expect("the comment survives", listing, "kept by ART")


def check_whdload_install(work: Path) -> None:
    """A WHDLoad pack ART installed, read by amitools (spec §82).

    The claim §82 makes is that the game is *installed*, and only an outside
    implementation can say whether the drawer, its contents and its icon are
    where AmigaOS looks for them. A drawer whose `.info` ended up inside it
    reads back perfectly in ART and is invisible on Workbench.
    """
    print("WHDLoad pack installed by ART, read by amitools:")
    image = work / "whdload.adf"
    if not export("ART_WHD_OUT", "export_whdload_install_for_oracle_when_asked", image):
        return

    listing = oracle(["xdftool", str(image), "list"])
    expect("the drawer is on the disk", listing, "Turrican")
    # Beside the drawer, not inside it. `xdftool list` indents contents, so a
    # misplaced icon still appears — the path check below is what pins it.
    expect("the icon is on the disk", listing, "Turrican.info")
    expect("the pack's contents are in the drawer", listing, "Turrican.slave")
    expect("a nested data drawer arrived", listing, "data")
    # The readme is not part of the game and must not have been installed.
    if "Turrican.readme" in listing:
        print("  FAIL the readme was installed; it is not part of the game")
        failures.append("the readme was installed")
    else:
        print("  ok   the readme was left out, as it is not part of the game")

    # The icon's real path is the claim that matters: `Turrican.info` at the
    # root, not `Turrican/Turrican.info`.
    icon = oracle(["xdftool", str(image), "type", "Turrican.info"])
    expect("the icon sits beside the drawer, not inside it", icon, "icon")

    slave = oracle(["xdftool", str(image), "type", "Turrican/Turrican.slave"])
    expect("the slave's bytes survived the install", slave, "WHDLOADSLAVE")


def check_read_back_the_other_way(work: Path) -> None:
    """The reverse direction: amitools writes, ART reads (brief §9.1).

    Everything above proves ART's *writer* against an independent reader. This
    proves ART's *reader* against an independent writer, which is the other
    half of the same argument — a reader and writer that agree with each other
    and with nothing else is exactly the failure ART-032 … ART-035 were.
    """
    print("Volume written by amitools, read by ART:")
    image = work / "by-amitools.adf"
    payload = work / "payload.txt"
    payload.write_bytes(b"written by amitools, not by ART\n")

    made = oracle(["xdftool", str(image), "create", "+", "format", "Oracle", "ffs"])
    if not image.exists():
        print("  FAIL amitools could not create an image")
        print(f"       {made.strip()[:300]}")
        failures.append("amitools could not create an image")
        return

    oracle(["xdftool", str(image), "makedir", "Tools"])
    oracle(["xdftool", str(image), "write", str(payload), "Readme"])
    oracle(["xdftool", str(image), "write", str(payload), "Tools/Nested"])

    # ART reads it through the same code path the file manager uses.
    # `--nocapture`: the hook reports what it found on stdout, and the test
    # harness swallows that by default.
    listing = run(
        [
            "cargo",
            "test",
            "--quiet",
            "read_foreign_volume_for_oracle_when_asked",
            "--",
            "--nocapture",
        ],
        {"ART_W_READ_IN": str(image)},
    )

    expect("ART reads the volume amitools formatted", listing, "volume=Oracle")
    expect("ART sees the folder amitools made", listing, "dir:Tools")
    expect("ART sees the file amitools wrote", listing, "file:Readme")
    expect("ART sees the nested file", listing, "nested:Nested")
    expect(
        "and reads its contents byte for byte",
        listing,
        "contents-match=true",
    )


def boot_block_checksum(bootblock_bytes: bytes) -> int:
    """Mirror `BootBlock::compute_checksum` in `core/adf/bootblock.rs` exactly.

    A 32-bit sum of every longword across the first 1024 bytes, with
    end-around carry (the checksum field itself, offset 4, treated as zero
    while summing), then complemented. Verified against amitools' own
    checksum on an untouched `xdftool`-written boot block before trusting it
    here — the two agreed (`0xe33d0e72`), so this is not an invented variant.
    """
    total = 0
    for i in range(0, 1024, 4):
        word = 0 if i == 4 else int.from_bytes(bootblock_bytes[i : i + 4], "big")
        added = total + word
        carry = 1 if added >= (1 << 32) else 0
        total = (added % (1 << 32) + carry) % (1 << 32)
    return (~total) & 0xFFFFFFFF


def check_bootable_adf_opens(work: Path) -> None:
    """A *bootable* floppy amitools wrote, opened by ART.

    ART used to read bytes 8..11 of the boot block as a root-block pointer.
    On a bootable disk those bytes are 68000 boot code, so every real bootable
    ADF failed to open. ART's own fixtures never caught it: they are zeros
    there. Only a disk ART did not write can prove the fix.

    `xdftool ... format ... + boot install` is not quite enough by itself:
    amitools' own `BootBlock.write()` unconditionally echoes the *correct*
    root block number into bytes 8..11 on every DOS bootblock it writes —
    with or without `boot install` — because that is a legacy field of its
    own model, not part of the real AmigaDOS format (real boot code starts
    right there). For a standard DD floppy that value is 880, which is also
    the right answer, so the pre-fix bug happened to compute the right root
    block by coincidence on this exact fixture and the check would have
    passed against the bug it exists to catch. Confirmed by running this
    check's assertions against the pre-fix `AdfImage::from_bytes` (the commit
    before d622412, in a throwaway worktree): it also printed `root=880`.

    So the whole boot-code region (byte 12 onward — genuine 68000
    instructions amitools generated, not fabricated) is moved 4 bytes
    earlier, overwriting that coincidental echo; the 4 bytes vacated at the
    very end of the block become zero. That move invalidates the boot-block
    checksum amitools computed, so the checksum is recomputed with ART's own
    algorithm and rewritten — otherwise this would test ART against a disk
    no real Amiga would accept as bootable, which is a weaker claim than the
    check's name makes. `xdftool boot show` on the altered file confirms
    amitools itself still considers it checksum-valid before ART ever sees
    it. Against the pre-fix reader the moved bytes decode as a block number
    far outside the image and fail with "block ... out of range"; against the
    fix they are never read as a block number at all.
    """
    print("Bootable ADF written by amitools, opened by ART:")
    image = work / "bootable.adf"

    oracle(["xdftool", str(image), "create", "+", "format", "Boot", "ffs", "+", "boot", "install"])
    if not image.exists():
        print("  FAIL amitools could not create a bootable image")
        failures.append("amitools could not create a bootable image")
        return

    data = bytearray(image.read_bytes())

    # Prove boot code was actually installed. Bytes 8..11 alone are not a
    # useful signal — see the docstring, amitools writes those regardless of
    # `boot install`. The discriminating region is byte 12 onward, where real
    # boot code lands only if `boot install` actually did something.
    if not any(data[12:1024]):
        print("  FAIL the fixture has no boot code, so it does not exercise the bug")
        failures.append("the bootable fixture was not bootable")
        return
    print("  ok   the fixture really carries boot code")

    root_block_echo = bytes(data[8:12])
    if root_block_echo == bytes(data[12:16]):
        print("  FAIL boot code is indistinguishable from the root-block echo")
        failures.append("the bootable fixture could not be made to exercise the bug")
        return

    # Move (not copy) the boot code 4 bytes earlier: what was at 12..1024 is
    # now at 8..1020, and the last 4 bytes of the block — now vacated — are
    # zeroed rather than left duplicated.
    data[8:1024] = data[12:1024] + bytearray(4)
    if bytes(data[8:12]) == root_block_echo:
        print("  FAIL the move did not disturb the root-block echo")
        failures.append("the bootable fixture could not be made to exercise the bug")
        return
    print("  ok   the root-block-shaped coincidence at 8..11 was overwritten by real boot code")

    # The move invalidated amitools' checksum; recompute it with ART's own
    # algorithm so the altered fixture is still one amitools — and a real
    # Amiga — would accept as bootable.
    data[4:8] = boot_block_checksum(bytes(data)).to_bytes(4, "big")
    image.write_bytes(bytes(data))

    boot_info = oracle(["xdftool", str(image), "boot", "show"])
    expect(
        "the altered fixture's checksum still validates under amitools",
        boot_info,
        "bootable: True",
    )

    out = run(
        [
            "cargo",
            "test",
            "--quiet",
            "open_foreign_adf_for_oracle_when_asked",
            "--",
            "--nocapture",
        ],
        {"ART_ADF_READ_IN": str(image)},
    )
    expect("ART opens a bootable ADF", out, "volume=Boot")
    expect("and puts its root block at 880", out, "root=880")
    expect("and reports a DD capacity", out, "capacity=901120")


def main() -> int:
    if shutil.which("xdftool") is None or shutil.which("rdbtool") is None:
        print("amitools is not installed. Run: pip install amitools")
        return 2

    work = Path(tempfile.mkdtemp(prefix="art-oracle-"))
    try:
        check_adf(work)
        check_hdf(work)
        check_written_volume(work)
        check_written_ofs(work)
        check_written_large(work)
        check_written_attributes(work)
        check_whdload_install(work)
        check_read_back_the_other_way(work)
        check_bootable_adf_opens(work)
    finally:
        shutil.rmtree(work, ignore_errors=True)

    if failures:
        print(f"\n{len(failures)} check(s) failed:")
        for failure in failures:
            print(f"  - {failure}")
        print(
            "\nART and an independent implementation disagree about the disks ART "
            "writes. That means the images are wrong, not the reader."
        )
        return 1

    print("\nART and an independent implementation agree, both ways round.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
