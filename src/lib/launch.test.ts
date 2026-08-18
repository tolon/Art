// ART-151: WHDLoad hit DOS-Error #103 ("not enough memory available") on a
// stock A500 profile — 512 KB Chip, 512 KB Slow, no Fast RAM at all. These
// tests cover the two pieces of this fix that live entirely on the frontend:
// the guard on the remembered `launch.whdloadFastRamMb` setting (a
// hand-edited or stale settings file must fall back to the default rather
// than reach a launch), and `memoryLabel`, which is what puts the machine's
// memory into the confirmation screen's "will use" sentence.

import { describe, expect, it } from "vitest";

import {
  DEFAULT_WHDLOAD_FAST_RAM_MB,
  isWhdloadFastRamMb,
  memoryLabel,
  WHDLOAD_FAST_RAM_MAX_MB,
} from "@/lib/launch";

describe("isWhdloadFastRamMb", () => {
  it("accepts every whole number WinUAE's 24-bit Fast RAM actually offers", () => {
    for (const value of [0, 1, 2, 4, 8]) {
      expect(isWhdloadFastRamMb(value)).toBe(true);
    }
  });

  it("accepts the shipped default", () => {
    expect(isWhdloadFastRamMb(DEFAULT_WHDLOAD_FAST_RAM_MB)).toBe(true);
  });

  // A hand-edited or stale settings.json is data, not a promise
  // (src/lib/remembered.ts's own header) — a nonsense stored value must fall
  // back to the default rather than reach a launch.
  it("rejects a nonsense stored value", () => {
    expect(isWhdloadFastRamMb(-1)).toBe(false);
    expect(isWhdloadFastRamMb(1.5)).toBe(false);
    expect(isWhdloadFastRamMb(WHDLOAD_FAST_RAM_MAX_MB + 1)).toBe(false);
    expect(isWhdloadFastRamMb(1_000_000)).toBe(false);
    expect(isWhdloadFastRamMb(Number.NaN)).toBe(false);
    expect(isWhdloadFastRamMb(Number.POSITIVE_INFINITY)).toBe(false);
    expect(isWhdloadFastRamMb("8")).toBe(false);
    expect(isWhdloadFastRamMb(null)).toBe(false);
    expect(isWhdloadFastRamMb(undefined)).toBe(false);
  });
});

describe("memoryLabel", () => {
  // The exact shape a WHDLoad launch on a stock A500 profile produces once
  // ART-151's headroom is folded in — the confirmation screen must name this
  // rather than leave the user to find out from WHDLoad's own error screen.
  it("names the memory a WHDLoad launch will use", () => {
    const label = memoryLabel({ chip_kb: 512, slow_kb: 512, fast_mb: 8, z3_fast_mb: 0 });
    expect(label).toContain("512 KB Chip");
    expect(label).toContain("512 KB Slow");
    expect(label).toContain("8 MB Fast");
  });

  // The pre-fix shape: exactly what DOS-Error #103 was measured against.
  // Confirms the label states it plainly rather than only the good case.
  it("also names the memory that was not enough", () => {
    const label = memoryLabel({ chip_kb: 512, slow_kb: 512, fast_mb: 0, z3_fast_mb: 0 });
    expect(label).toBe("512 KB Chip + 512 KB Slow");
  });

  it("omits slow and Z3 Fast RAM when the machine carries none", () => {
    const label = memoryLabel({ chip_kb: 2048, slow_kb: 0, fast_mb: 8, z3_fast_mb: 0 });
    expect(label).toBe("2048 KB Chip + 8 MB Fast");
  });
});
