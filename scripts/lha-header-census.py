"""A complete LHA header census over the owner's own archives.

Whole file, every entry, every header level. The first attempt read only the
first 200 KB of each archive and stopped at 200 entries, which is why it
reported "every non-ASCII name is level 0".

Header layouts, from the LHA format description:

  level 0  [hsize:1][cks:1][method:5][csize:4][usize:4][time:4][attr:1]
           [level:1][namelen:1][name:n][crc:2]
           next = off + 2 + hsize + csize

  level 1  same up to the name, then [crc:2][os:1][next_ext:2], and the
           `csize` field is a *skip size* = compressed size + every extended
           header. Extended headers follow the base header.
           next = off + 2 + hsize + skip

  level 2  [total:2][method:5][csize:4][usize:4][time:4][reserved:1]
           [level:1][crc:2][os:1][next_ext:2][ext...]
           next = off + total

Extended header: [type:1][data:size-3][next_size:2], where `size` came from
the previous next-size field. Type 0x01 is the file name, 0x02 the directory.
"""

import glob
import os
import sys

ROOTS = [
    r"E:\amiga\Amigatolon\paketler",
]


def u16(b, o):
    return int.from_bytes(b[o:o + 2], "little")


def u32(b, o):
    return int.from_bytes(b[o:o + 4], "little")


def read_ext(d, p, first_size, limit):
    """Yield (type, data) for each extended header starting at p."""
    size = first_size
    out = []
    while size and p + size <= limit and len(out) < 4096:
        typ = d[p]
        data = d[p + 1:p + size - 2]
        out.append((typ, data))
        nxt = u16(d, p + size - 2)
        p += size
        size = nxt
    return out, p


def entries(path):
    d = open(path, "rb").read()
    off = 0
    n = len(d)
    while off < n and d[off] != 0:
        if off + 24 > n:
            break
        level = d[off + 20]
        rec = {"level": level, "name": b"", "dir": b"", "ext": []}

        if level in (0, 1):
            hsize = d[off]
            namelen = d[off + 21]
            rec["name"] = d[off + 22:off + 22 + namelen]
            skip = u32(d, off + 7)
            base_end = off + 2 + hsize
            if level == 1:
                first = u16(d, base_end - 2)
                ext, _ = read_ext(d, base_end, first, n)
                rec["ext"] = ext
                for typ, data in ext:
                    if typ == 0x02:
                        rec["dir"] = data
            off = base_end + skip
        elif level == 2:
            total = u16(d, off)
            if total < 26:
                break
            first = u16(d, off + 24)
            ext, _ = read_ext(d, off + 26, first, off + total)
            rec["ext"] = ext
            for typ, data in ext:
                if typ == 0x01:
                    rec["name"] = data
                elif typ == 0x02:
                    rec["dir"] = data
            # The size word covers the header only; the compressed data
            # follows it, unlike level 0/1 where csize/skip is added to a
            # base-header size.
            off += total + u32(d, off + 7)
        else:
            # level 3 (or garbage): stop rather than desync silently.
            rec["level"] = level
            yield rec
            return
        yield rec


def nonascii(b):
    return any(x >= 0x80 for x in b)


def nonascii_dir(b):
    # 0xFF is the *separator* inside a 0x02 directory header, not a
    # character. Counting it made every one of the 880 directory headers
    # look like it carried an accented name.
    return any(x >= 0x80 and x != 0xFF for x in b)


def main():
    files = []
    for root in ROOTS:
        files += glob.glob(os.path.join(root, "**", "*.lha"), recursive=True)
        files += glob.glob(os.path.join(root, "**", "*.lzx"), recursive=True)
    files = sorted(set(files))

    totals = {}
    na_by_level = {}
    naname_by_level = {}
    nadir_by_level = {}
    nul_by_level = {}
    dir_ext = {"present": 0, "absent": 0}
    na_in_dir = 0
    per_file = []

    for f in files:
        if f.lower().endswith(".lzx"):
            continue
        try:
            recs = list(entries(f))
        except Exception as exc:  # a malformed archive is data, not a crash
            print("SKIP", os.path.basename(f), exc)
            continue
        counts = {}
        na = 0
        nul = 0
        for r in recs:
            lv = r["level"]
            totals[lv] = totals.get(lv, 0) + 1
            counts[lv] = counts.get(lv, 0) + 1
            name = r["name"]
            # The level 0/1 field can be `name\0comment` (Amiga LhA).
            base = name.split(b"\0", 1)[0]
            if b"\0" in name:
                nul += 1
                nul_by_level[lv] = nul_by_level.get(lv, 0) + 1
            hit_name = nonascii(base)
            hit_dir = nonascii_dir(r["dir"])
            if hit_name or hit_dir:
                na += 1
                na_by_level[lv] = na_by_level.get(lv, 0) + 1
            if hit_name:
                naname_by_level[lv] = naname_by_level.get(lv, 0) + 1
            if hit_dir:
                nadir_by_level[lv] = nadir_by_level.get(lv, 0) + 1
            if lv == 1:
                if r["dir"]:
                    dir_ext["present"] += 1
                else:
                    dir_ext["absent"] += 1
        per_file.append((os.path.basename(f), counts, na, nul))

    print("=== per archive: {level: entries}, non-ASCII names, NUL-in-field ===")
    for name, counts, na, nul in per_file:
        print(f"  {name:44s} {counts}  non-ascii={na:3d}  nul={nul:3d}")

    print()
    print("=== totals by header level ===")
    for lv in sorted(totals):
        print(f"  level {lv}: {totals[lv]:5d} entries | "
              f"non-ASCII base name {naname_by_level.get(lv, 0):4d} | "
              f"non-ASCII directory {nadir_by_level.get(lv, 0):4d} | "
              f"NUL in name field {nul_by_level.get(lv, 0):4d}")
    print()
    print(f"level-1 entries WITH a 0x02 directory header: {dir_ext['present']}")
    print(f"level-1 entries WITHOUT one:                  {dir_ext['absent']}")


if __name__ == "__main__":
    sys.exit(main())
