import { describe, expect, it } from "vitest";

import {
  FFS_PARTITION_LIMIT_MB,
  HDF_MIN_MB,
  hdfSizeWarning,
  largestPartitionMb,
  parseCustomSize,
} from "@/lib/hdfSize";

describe("parseCustomSize", () => {
  it("takes a plain number of megabytes", () => {
    expect(parseCustomSize("512", "mb")).toEqual({ ok: true, mb: 512 });
  });

  it("takes gigabytes and converts them", () => {
    expect(parseCustomSize("16", "gb")).toEqual({ ok: true, mb: 16384 });
    expect(parseCustomSize("0.5", "gb")).toEqual({ ok: true, mb: 512 });
  });

  it("accepts a comma as the decimal separator", () => {
    // The user's own locale is Turkish, where `1,5` is what gets typed.
    expect(parseCustomSize("1,5", "gb")).toEqual({ ok: true, mb: 1536 });
  });

  it("has no upper limit — the old 8 GB ceiling was five buttons, not a rule", () => {
    // ART-083. `create_rdb_layout` fails only when the cylinder count will not
    // fit a u32, which at 516,096 bytes per cylinder is measured in petabytes.
    expect(parseCustomSize("500", "gb")).toEqual({ ok: true, mb: 512000 });
    expect(parseCustomSize("2048", "gb")).toEqual({ ok: true, mb: 2097152 });
  });

  it("refuses what the engine would refuse, with the same floor", () => {
    const tooSmall = parseCustomSize("9", "mb");
    expect(tooSmall.ok).toBe(false);
    expect(!tooSmall.ok && tooSmall.reason.key).toBe("hardDisk.modal.custom.tooSmall");
    expect(!tooSmall.ok && tooSmall.reason.params).toEqual({ min: HDF_MIN_MB });

    expect(parseCustomSize("10", "mb").ok).toBe(true);
  });

  it("refuses nonsense rather than guessing at it", () => {
    for (const text of ["", "   ", "abc", "-5", "0"]) {
      expect(parseCustomSize(text, "mb").ok, text).toBe(false);
    }
  });

  it("refuses a fraction of a megabyte rather than rounding it away", () => {
    // Silently turning 12.5 MB into 12 would hand back a different disk from
    // the one that was asked for, and the size cannot be changed afterwards.
    const result = parseCustomSize("12.5", "mb");
    expect(result.ok).toBe(false);
    expect(!result.ok && result.reason.key).toBe("hardDisk.modal.custom.notWholeMegabytes");
  });
});

describe("largestPartitionMb", () => {
  it("is the whole disk for a single partition", () => {
    expect(largestPartitionMb(6000, "single")).toBe(6000);
  });

  it("mirrors the wizard's own split: 500 MB or a third, then the rest", () => {
    expect(largestPartitionMb(6000, "split")).toBe(5500);
    // A small disk gets a third rather than 500 MB, and DH1 still wins.
    expect(largestPartitionMb(900, "split")).toBe(600);
  });
});

describe("hdfSizeWarning", () => {
  it("says nothing about the size of a PFS3 or SFS disk", () => {
    // Addressing large disks is most of why anyone picks them, so the 4 GB
    // ceiling below does not apply. This used to return "no driver behind it"
    // for every size, which was true of every image ART could make (ART-084)
    // and is now true only of one the user declined to embed a driver into —
    // a question `@/lib/fsDriver` answers, and a size function cannot.
    for (const fs of ["pfs3directscsi", "sfs0"] as const) {
      expect(hdfSizeWarning(1024, "split", fs)).toBeNull();
      expect(hdfSizeWarning(32768, "single", fs)).toBeNull();
    }
  });

  it("warns when an FFS partition passes the 4 GB addressing ceiling", () => {
    const warning = hdfSizeWarning(6144, "single", "ffsdircache");
    expect(warning?.key).toBe("hardDisk.modal.warnOver4Gb");
    expect(warning?.params).toEqual({ size: 6 });
  });

  it("measures that ceiling per partition, not per image", () => {
    // 4.4 GB as one partition is over it; the same disk split is not, because
    // DH0 takes 500 MB off the top and DH1 lands at exactly 4 GB.
    expect(hdfSizeWarning(4500, "single", "ffsstandard")?.key).toBe(
      "hardDisk.modal.warnOver4Gb"
    );
    expect(hdfSizeWarning(4500, "split", "ffsstandard")).toBeNull();
  });

  it("says nothing about an ordinary FFS disk", () => {
    expect(hdfSizeWarning(FFS_PARTITION_LIMIT_MB, "single", "ffsstandard")).toBeNull();
    expect(hdfSizeWarning(1024, "split", "ffsdircache")).toBeNull();
  });
});
