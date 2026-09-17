import { describe, expect, it } from "vitest";

import { recall } from "@/lib/remembered";
import {
  CARD_SIZES_GB,
  CARD_TARGET_KEY,
  DEFAULT_CARD_TARGET,
  DESTINATION_KIND_KEY,
  isCardTarget,
  type CardTarget,
} from "./cardTarget";

describe("CARD_SIZES_GB", () => {
  it("is the one list — 16, 32, 64, 128, 256", () => {
    expect(CARD_SIZES_GB).toEqual([16, 32, 64, 128, 256]);
  });
});

describe("DEFAULT_CARD_TARGET", () => {
  it("has System and Work, and neither carries a source", () => {
    expect(DEFAULT_CARD_TARGET.partitions).toEqual([
      { name: "System", sources: [] },
      { name: "Work", sources: [] },
    ]);
  });

  it("starts with no image, archive or driver chosen", () => {
    expect(DEFAULT_CARD_TARGET.image).toBeNull();
    expect(DEFAULT_CARD_TARGET.emu68Archive).toBeNull();
    expect(DEFAULT_CARD_TARGET.pfs3Driver).toBeNull();
  });
});

describe("isCardTarget", () => {
  const valid: CardTarget = {
    sizeGb: 64,
    image: "E:\\amiga\\Kartlar\\amiga39.img",
    emu68Archive: "E:\\amiga\\Emu68-pistorm-classic.zip",
    pfs3Driver: "pfs3aio",
    partitions: [
      { name: "System", sources: [] },
      { name: "Games", sources: ["E:\\whd\\Games"], floorBytes: 5_000_000_000 },
      { name: "Work", sources: [] },
    ],
  };

  it("round-trips a stored target with a partition list", () => {
    expect(recall({ [CARD_TARGET_KEY("AmigaOS 3.2")]: valid }, CARD_TARGET_KEY("AmigaOS 3.2"), isCardTarget, DEFAULT_CARD_TARGET)).toEqual(
      valid
    );
  });

  it("accepts the value directly", () => {
    expect(isCardTarget(valid)).toBe(true);
  });

  it("rejects a stored value missing partitions — whole, not repaired", () => {
    const { partitions: _partitions, ...withoutPartitions } = valid;
    expect(isCardTarget(withoutPartitions)).toBe(false);
    expect(
      recall({ key: withoutPartitions }, "key", isCardTarget, DEFAULT_CARD_TARGET)
    ).toEqual(DEFAULT_CARD_TARGET);
  });

  it("rejects a partition whose sources is not an array of strings — whole, not repaired", () => {
    const broken = {
      ...valid,
      partitions: [
        { name: "System", sources: [] },
        { name: "Games", sources: "E:\\whd\\Games" },
      ],
    };
    expect(isCardTarget(broken)).toBe(false);
    // The good "System" entry does not survive on its own — the whole stored
    // value is rejected and the fallback is returned entire.
    expect(recall({ key: broken }, "key", isCardTarget, DEFAULT_CARD_TARGET)).toEqual(
      DEFAULT_CARD_TARGET
    );
  });

  it("rejects a bag that is not an object at all", () => {
    expect(isCardTarget(null)).toBe(false);
    expect(isCardTarget("nonsense")).toBe(false);
    expect(isCardTarget(42)).toBe(false);
  });

  it("rejects a partition list that is not an array", () => {
    expect(isCardTarget({ ...valid, partitions: { name: "System", sources: [] } })).toBe(false);
  });
});

describe("CARD_TARGET_KEY / DESTINATION_KIND_KEY", () => {
  it("are per release", () => {
    expect(CARD_TARGET_KEY("AmigaOS 3.2")).toBe("osinstall.cardTarget.AmigaOS 3.2");
    expect(CARD_TARGET_KEY("AmigaOS 3.9")).toBe("osinstall.cardTarget.AmigaOS 3.9");
    expect(DESTINATION_KIND_KEY("AmigaOS 3.2")).toBe("osinstall.destinationKind.AmigaOS 3.2");
    expect(DESTINATION_KIND_KEY("AmigaOS 3.9")).toBe("osinstall.destinationKind.AmigaOS 3.9");
  });
});
