"""Read-only dump of every RDB block in each 0x76 area (or a plain RDB image).

Opens the image with mode 'rb' only. Reads at most the RDB range of each area.
"""
import struct
import sys

BS = 512


def longs(b):
    return struct.unpack(">128I", b)


def cks_ok(b):
    n = struct.unpack(">I", b[4:8])[0]
    if n == 0 or n > 128:
        return None
    return sum(struct.unpack(">%dI" % n, b[: n * 4])) & 0xFFFFFFFF == 0


def tag(v):
    return bytes([(v >> 24) & 255, (v >> 16) & 255, (v >> 8) & 255, v & 255])


def dump_area(f, base, label):
    def blk(n):
        f.seek(base + n * BS)
        return f.read(BS)

    rdsk_at = None
    for n in range(16):
        if blk(n)[:4] == b"RDSK":
            rdsk_at = n
            break
    if rdsk_at is None:
        print(label, "no RDSK in first 16 blocks")
        return
    b = blk(rdsk_at)
    L = longs(b)
    print("== %s base=%d RDSK at block %d checksum_ok=%s summed=%d" % (label, base, rdsk_at, cks_ok(b), L[1]))
    names = {4: "BlockBytes", 5: "Flags", 6: "BadBlockList", 7: "PartitionList", 8: "FileSysHeaderList",
             9: "DriveInit", 16: "Cylinders", 17: "Sectors", 18: "Heads", 32: "RDBBlocksLo",
             33: "RDBBlocksHi", 34: "LoCylinder", 35: "HiCylinder", 36: "CylBlocks", 38: "HighRDSKBlock"}
    for i, nm in names.items():
        print("  %-18s %d (0x%08x)" % (nm, L[i], L[i]))
    print("  Reserved1[6] =", ["%08x" % x for x in L[10:16]])
    used = {rdsk_at: "RDSK"}
    # partitions
    p = L[7]
    first_low = None
    seen = set()
    while p != 0xFFFFFFFF and p not in seen and len(seen) < 64:
        seen.add(p)
        pb = blk(p)
        if pb[:4] != b"PART":
            print("  PART chain hits non-PART at", p)
            break
        PL = longs(pb)
        name = pb[37:37 + pb[36]].decode("latin-1")
        low, high, dt = PL[41], PL[42], PL[48]
        first_low = low if first_low is None else min(first_low, low)
        print("  PART blk=%d name=%s dostype=%r low=%d high=%d next=%d cks=%s" % (p, name, tag(dt), low, high, PL[4], cks_ok(pb)))
        used[p] = "PART " + name
        p = PL[4]
    # filesystems
    fsb = L[8]
    seen = set()
    while fsb != 0xFFFFFFFF and fsb not in seen and len(seen) < 32:
        seen.add(fsb)
        fb = blk(fsb)
        if fb[:4] != b"FSHD":
            print("  FSHD chain hits non-FSHD at", fsb)
            break
        FL = longs(fb)
        name = fb[172:256].split(b"\0")[0].decode("latin-1")
        print("  FSHD blk=%d dostype=%r ver=%d.%d patchflags=0x%x seglist=%d globalvec=0x%x stack=%d next=%d summed=%d cks=%s name=%r" % (
            fsb, tag(FL[8]), FL[9] >> 16, FL[9] & 0xFFFF, FL[10], FL[18], FL[19], FL[15], FL[4], FL[1], cks_ok(fb), name))
        used[fsb] = "FSHD"
        s = FL[18]
        segs = []
        nbytes = 0
        sseen = set()
        while s != 0xFFFFFFFF and s not in sseen and len(sseen) < 4096:
            sseen.add(s)
            sb = blk(s)
            if sb[:4] != b"LSEG":
                print("   LSEG chain hits non-LSEG at", s)
                break
            SL = longs(sb)
            if not cks_ok(sb):
                print("   LSEG bad checksum at", s)
            segs.append(s)
            nbytes += (SL[1] - 5) * 4
            used[s] = "LSEG"
            s = SL[4]
        contiguous = all(segs[i] + 1 == segs[i + 1] for i in range(len(segs) - 1))
        print("   LSEG count=%d first=%s last=%s contiguous=%s data_bytes=%d" % (
            len(segs), segs[:1], segs[-1:], contiguous, nbytes))
        fsb = FL[4]
    # bad blocks
    bb = L[6]
    seen = set()
    while bb != 0xFFFFFFFF and bb not in seen and len(seen) < 64:
        seen.add(bb)
        bbk = blk(bb)
        print("  BADB blk=%d id=%r" % (bb, bbk[:4]))
        used[bb] = "BADB"
        bb = longs(bbk)[4]
    hi_used = max(used)
    print("  used blocks: count=%d highest=%d (HighRDSKBlock field=%d)" % (len(used), hi_used, L[38]))
    rdb_hi = L[33]
    cyl_blocks = L[36] or (L[17] * L[18])
    first_data_block = first_low * cyl_blocks if first_low is not None else None
    print("  first partition LowCyl=%s -> first data block=%s; LoCylinder*CylBlocks=%d" % (first_low, first_data_block, L[34] * cyl_blocks))
    limit = rdb_hi
    free = [n for n in range(L[32], limit + 1) if n not in used]
    print("  free blocks in [RDBBlocksLo..RDBBlocksHi]=%d" % len(free))
    # stray signatures in the reserved range that are not on any chain
    stray = {}
    top = min(limit, (first_data_block or limit + 1) - 1)
    scan_top = min(top, 4096)
    for n in range(0, scan_top + 1):
        if n in used:
            continue
        sig = blk(n)[:4]
        if sig in (b"RDSK", b"PART", b"FSHD", b"LSEG", b"BADB"):
            stray.setdefault(sig.decode(), []).append(n)
        elif sig != b"\0\0\0\0":
            stray.setdefault("nonzero-other", []).append(n)
    for k, v in stray.items():
        print("  unlinked %s blocks in 0..%d: count=%d first=%s" % (k, scan_top, len(v), v[:8]))
    # gap between HighRDSKBlock+1 and first data block that is all zero
    if first_data_block is not None:
        print("  blocks between HighRDSKBlock+1 and first data block: %d" % (first_data_block - (L[38] + 1)))


def main():
    path = sys.argv[1]
    with open(path, "rb") as f:
        mbr = f.read(BS)
        areas = []
        if mbr[510:512] == b"\x55\xaa" and mbr[:4] != b"RDSK":
            for i in range(4):
                e = mbr[446 + 16 * i: 462 + 16 * i]
                ptype = e[4]
                lba, count = struct.unpack("<II", e[8:16])
                print("MBR slot %d type=0x%02x lba=%d count=%d" % (i, ptype, lba, count))
                if ptype == 0x76:
                    areas.append((lba * BS, "slot%d" % i))
        if not areas:
            areas = [(0, "whole-file")]
        for base, label in areas:
            dump_area(f, base, label)


if __name__ == "__main__":
    main()
