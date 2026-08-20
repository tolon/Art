"""A SUSP/Rock Ridge signature census over the owner's own ISO images.

ART-078 asks whether an Amiga CD carries the Amiga-specific `AS` System Use
entry, or only POSIX Rock Ridge. That is a question about real discs, not
about the standard, so it is measured rather than assumed — and it is a script
rather than a one-off command so the answer can be re-run instead of
re-trusted (`CLAUDE.md`, "Research before design").

What it walks
-------------

Every directory record on the disc, depth-first from the root of the *Primary*
volume descriptor (SUSP lives on the primary tree; a Joliet tree carries its
own records and normally no System Use area at all).

The System Use area of a directory record is whatever follows the identifier:

    [0]      record length
    [2..6]   extent LBA (little endian)
    [10..14] data length (little endian)
    [32]     identifier length
    [33..]   identifier, then one pad byte when the identifier length is even
    then     the System Use area, to the end of the record

SUSP (IEEE P1281) fills that area with entries:

    [0..2]   two-character signature
    [2]      entry length, covering the whole entry including this header
    [3]      entry version
    [4..]    payload

`SP` (in the `.` record of the root only) declares a *skip* count: bytes to
ignore at the start of every System Use area. `CE` points at a continuation
area elsewhere on the disc — block, offset and length, each stored both-endian
(8 bytes per field) — and a disc may chain them, so the walk below caps the
chain length rather than trusting it.

What it prints
--------------

Per disc: the descriptor list, whether Joliet is present, how many records
carry a System Use area at all, and the count of every signature seen. Then a
sample hex dump of the first few `AS` entries, if any, and of an `NM`/`PX`
pair, so the byte layout can be read off real material.

Usage
-----

    python scripts/iso-susp-census.py [image.iso ...]

With no arguments it uses the owner's own collection under E:\\amiga.
"""

import os
import sys

SECTOR = 2048

DEFAULT_IMAGES = [
    r"E:\amiga\Amigatolon\iso\AmigaOS39.iso",
    r"E:\amiga\Amigatolon\iso\AmigaOS3.2CD(ZaP).iso",
    r"E:\amiga\Amigatolon\iso\Amiga Developer CD v1.1 - May 1996 (1996)(Schatztruhe)[!].iso",
    r"E:\amiga\Amigatolon\iso\Amiga Developer CD v2.1.iso",
]

# A crafted or damaged disc must not be able to make this script loop or
# allocate without bound. Same spirit as the Rust reader's own caps.
MAX_DIRECTORIES = 20_000
MAX_ENTRIES_PER_DIR = 20_000
MAX_CE_CHAIN = 32
MAX_SUSP_ENTRIES = 4096


def u16le(b, o):
    return int.from_bytes(b[o:o + 2], "little")


def u32le(b, o):
    return int.from_bytes(b[o:o + 4], "little")


class Image:
    def __init__(self, path):
        self.path = path
        self.f = open(path, "rb")
        self.size = os.path.getsize(path)

    def sectors(self, lba, count):
        off = lba * SECTOR
        if off < 0 or off >= self.size:
            return b""
        self.f.seek(off)
        return self.f.read(count * SECTOR)

    def close(self):
        self.f.close()


def descriptors(img):
    """Yield (type, identifier, raw) for each volume descriptor."""
    lba = 16
    out = []
    while lba < 512:
        d = img.sectors(lba, 1)
        if len(d) < SECTOR or d[1:6] != b"CD001":
            break
        out.append((d[0], d, lba))
        if d[0] == 255:
            break
        lba += 1
    return out


def descriptor_name(t):
    return {0: "Boot", 1: "Primary", 2: "Supplementary", 3: "Partition",
            255: "Terminator"}.get(t, f"Type{t}")


def is_joliet(raw):
    # A supplementary descriptor is Joliet when its escape sequences field
    # (offset 88, 32 bytes) holds one of the three UCS-2 designators.
    esc = raw[88:120]
    return any(e in esc for e in (b"%/@", b"%/C", b"%/E"))


def susp_entries(area, img, depth=0, seen=None):
    """Parse one System Use area, following CE continuations.

    Returns a list of (signature, version, payload). Every length field comes
    off the disc, so each is checked against what is actually there before it
    is used.
    """
    if seen is None:
        seen = set()
    out = []
    pos = 0
    guard = 0
    while pos + 4 <= len(area):
        guard += 1
        if guard > MAX_SUSP_ENTRIES:
            break
        sig = area[pos:pos + 2]
        length = area[pos + 2]
        version = area[pos + 3]
        if length < 4 or pos + length > len(area):
            # Padding (a run of NULs) or a length that does not fit: either
            # way there is nothing more to read in this area.
            break
        if sig in (b"\0\0", b"  "):
            break
        payload = area[pos + 4:pos + length]
        out.append((sig.decode("latin-1"), version, payload))

        if sig == b"CE" and depth < MAX_CE_CHAIN and len(payload) >= 24:
            block = u32le(payload, 0)
            offset = u32le(payload, 8)
            clen = u32le(payload, 16)
            key = (block, offset, clen)
            if key not in seen and 0 < clen <= 8 * SECTOR:
                seen.add(key)
                want = (offset + clen + SECTOR - 1) // SECTOR
                data = img.sectors(block, max(1, want))
                if len(data) >= offset + clen:
                    out += susp_entries(data[offset:offset + clen], img,
                                        depth + 1, seen)
        pos += length
    return out


def decode_as(payload):
    """Decode one `AS` payload the way ODFileSystem's `rr_parse_as` does.

    `backends/rock_ridge/rock_ridge.c` (BSD-2-Clause), read at commit HEAD on
    2026-08-20, and its own unit vectors in `tests/unit/test_rock_ridge.c`:

        'A','S', len, version, flags
          flags & 0x01 → 4 bytes of protection follow
          flags & 0x02 → a count byte then count-1 comment characters
          flags & 0x04 → the comment continues in the next AS entry

    Returns (flags, protection|None, comment|None, consumed) where `consumed`
    is how many payload bytes the layout accounts for. A layout that is wrong
    shows up as `consumed != len(payload)` across real discs, which is the
    second source this script is: the spec is confirmed by 44796 real entries
    agreeing with it, not by prose.
    """
    if not payload:
        return None, None, None, 0
    flags = payload[0]
    pos = 1
    prot = None
    comment = None
    if flags & 0x01:
        if pos + 4 > len(payload):
            return flags, None, None, -1
        prot = payload[pos:pos + 4]
        pos += 4
    if flags & 0x02:
        if pos + 1 > len(payload):
            return flags, prot, None, -1
        count = payload[pos]
        pos += 1
        if count == 0 or pos + count - 1 > len(payload):
            return flags, prot, None, -1
        comment = payload[pos:pos + count - 1]
        pos += count - 1
    return flags, prot, comment, pos


def records_in(buf):
    """Yield (record_bytes,) for every record in a directory extent."""
    start = 0
    while start < len(buf):
        end = min(start + SECTOR, len(buf))
        pos = start
        n = 0
        while pos < end:
            length = buf[pos]
            if length == 0:
                break
            if length < 34 or pos + length > end:
                break
            n += 1
            if n > MAX_ENTRIES_PER_DIR:
                break
            yield buf[pos:pos + length]
            pos += length
        start += SECTOR


def system_use_of(rec, skip):
    id_len = rec[32]
    su = 33 + id_len
    if id_len % 2 == 0:
        su += 1  # pad byte so the System Use area starts on an even offset
    if su >= len(rec):
        return b""
    area = rec[su:]
    return area[skip:] if skip < len(area) else b""


def census(path, sample_limit=6):
    img = Image(path)
    try:
        descs = descriptors(img)
        kinds = [descriptor_name(t) for t, _, _ in descs]
        joliet = any(t == 2 and is_joliet(raw) for t, raw, _ in descs)
        primary = next((raw for t, raw, _ in descs if t == 1), None)
        print(f"\n=== {os.path.basename(path)} ===")
        print(f"  descriptors: {kinds}")
        print(f"  joliet: {joliet}")
        if primary is None:
            print("  no primary descriptor — not an ISO9660 image")
            return None

        root = primary[156:156 + 34]
        root_extent = u32le(root, 2)
        root_len = u32le(root, 10)

        # The SP entry lives in the `.` record of the root directory and
        # states how many bytes to skip at the start of every System Use
        # area. Read it first, unskipped, or every later area is misaligned.
        skip = 0
        first = img.sectors(root_extent, max(1, (root_len + SECTOR - 1) // SECTOR))
        for rec in records_in(first):
            if rec[32] == 1 and rec[33] == 0x00:
                for sig, _ver, payload in susp_entries(system_use_of(rec, 0), img):
                    if sig == "SP" and len(payload) >= 3:
                        if payload[0] == 0xBE and payload[1] == 0xEF:
                            skip = payload[2]
                    if sig == "ER" and len(payload) >= 4:
                        idl, dsl, srcl = payload[0], payload[1], payload[2]
                        ident = payload[4:4 + idl].decode("latin-1", "replace")
                        desc = payload[4 + idl:4 + idl + dsl].decode("latin-1", "replace")
                        src = payload[4 + idl + dsl:4 + idl + dsl + srcl]
                        print(f"  ER: id={ident!r} desc={desc!r} "
                              f"src={src.decode('latin-1', 'replace')!r}")
            break
        print(f"  SUSP skip (SP): {skip}")

        counts = {}
        as_flags = {}
        as_prot = {}
        as_upper = {}
        as_comments = []
        as_bad = 0
        as_slack = 0
        records = 0
        with_su = 0
        dirs_done = 0
        samples = {}
        queue = [(root_extent, root_len, "/")]
        visited = {root_extent}

        while queue and dirs_done < MAX_DIRECTORIES:
            extent, length, prefix = queue.pop(0)
            dirs_done += 1
            want = max(1, (length + SECTOR - 1) // SECTOR)
            buf = img.sectors(extent, want)
            if not buf:
                continue
            for rec in records_in(buf):
                id_len = rec[32]
                ident = rec[33:33 + id_len]
                dot = id_len == 1 and ident[0] in (0x00, 0x01)
                is_dir = bool(rec[25] & 0x02)
                records += 1
                area = system_use_of(rec, skip)
                if area:
                    with_su += 1
                entries = susp_entries(area, img) if area else []
                for sig, ver, payload in entries:
                    counts[sig] = counts.get(sig, 0) + 1
                    bucket = samples.setdefault(sig, [])
                    if len(bucket) < sample_limit:
                        name = ident.decode("latin-1", "replace")
                        bucket.append((prefix + name, ver, payload))
                    if sig == "AS":
                        flags, prot, comment, used = decode_as(payload)
                        as_flags[flags] = as_flags.get(flags, 0) + 1
                        if used < 0:
                            as_bad += 1
                        elif used != len(payload):
                            as_slack += 1
                        if prot is not None:
                            as_prot[prot[3]] = as_prot.get(prot[3], 0) + 1
                            upper = prot[:3]
                            as_upper[upper.hex()] = as_upper.get(upper.hex(), 0) + 1
                        if comment:
                            if len(as_comments) < 12:
                                name = ident.decode("latin-1", "replace")
                                as_comments.append(
                                    (prefix + name,
                                     comment.decode("latin-1", "replace")))
                if is_dir and not dot:
                    child = u32le(rec, 2)
                    clen = u32le(rec, 10)
                    if child and child not in visited:
                        visited.add(child)
                        name = ident.decode("latin-1", "replace")
                        queue.append((child, clen, prefix + name + "/"))

        print(f"  directories walked: {dirs_done}   records: {records}   "
              f"with a System Use area: {with_su}")
        if not counts:
            print("  signatures: NONE — this disc carries no System Use data")
        else:
            print("  signatures: " + ", ".join(
                f"{s}={c}" for s, c in sorted(counts.items(),
                                              key=lambda kv: -kv[1])))
        if as_flags:
            print("  --- AS decoded ---")
            print("    flags: " + ", ".join(
                f"0x{f:02x}={c}" for f, c in sorted(as_flags.items())))
            print(f"    payloads the layout does not fit: {as_bad}   "
                  f"payloads with bytes left over: {as_slack}")
            print("    protection[3] (the classic HSPARWED byte): " + ", ".join(
                f"0x{p:02x}={c}" for p, c in sorted(as_prot.items())))
            print("    protection[0..3] (multiuser half): " + ", ".join(
                f"{h}={c}" for h, c in sorted(as_upper.items())))
            print(f"    entries carrying a comment: {len(as_comments)} shown")
            for where, text in as_comments:
                print(f"      {where}: {text!r}")

        for sig in ("AS", "NM", "PX", "RR", "TF", "CE"):
            if sig in samples:
                print(f"  --- {sig} samples ---")
                for where, ver, payload in samples[sig]:
                    print(f"    {where}  v{ver}  len={len(payload)}  "
                          f"{payload.hex(' ')}")
                    printable = "".join(
                        chr(b) if 32 <= b < 127 else "." for b in payload)
                    print(f"      {printable!r}")
        return counts
    finally:
        img.close()


def main(argv):
    images = argv[1:] or DEFAULT_IMAGES
    missing = [p for p in images if not os.path.exists(p)]
    for p in missing:
        print(f"MISSING {p}")
    present = [p for p in images if os.path.exists(p)]
    if not present:
        print("no images to census")
        return 1
    any_as = False
    for p in present:
        counts = census(p)
        if counts and "AS" in counts:
            any_as = True
    print()
    print("=== verdict ===")
    print("at least one disc carries the Amiga `AS` entry" if any_as
          else "NO disc in this set carries the Amiga `AS` entry")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
