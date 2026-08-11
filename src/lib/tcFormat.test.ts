// Covers the pure formatters `tcFormat.ts` adds for the Total
// Commander-styled Files screen — see that file's comment for why dates are
// fixed-format and sizes are not.
import { describe, expect, it } from "vitest";

import { formatDateTC, formatGroupedSize } from "./tcFormat";

/** Build a unix-seconds timestamp from local wall-clock fields, the same way
 * `formatDateTC` reads one back with `Date#getDate`/`getHours`/etc. — so this
 * test is not sensitive to the timezone the test runner happens to be in. */
function localUnix(year: number, month: number, day: number, hour: number, minute: number): number {
  return Math.floor(new Date(year, month - 1, day, hour, minute, 0).getTime() / 1000);
}

describe("formatDateTC", () => {
  it("renders DD.MM.YYYY HH:MM, zero-padded", () => {
    expect(formatDateTC(localUnix(2024, 3, 5, 9, 7))).toBe("05.03.2024 09:07");
  });

  it("pads neither year nor drops leading zeros on day/month/hour/minute", () => {
    expect(formatDateTC(localUnix(1999, 12, 31, 23, 59))).toBe("31.12.1999 23:59");
  });

  it("returns null for a null date rather than inventing a placeholder", () => {
    expect(formatDateTC(null)).toBeNull();
  });
});

describe("formatGroupedSize", () => {
  it("groups with a dot under a Turkish locale", () => {
    expect(formatGroupedSize(63158597683, "tr")).toBe("63.158.597.683");
  });

  it("groups with a comma under an English locale", () => {
    expect(formatGroupedSize(63158597683, "en")).toBe("63,158,597,683");
  });

  it("truncates a fractional byte count instead of rendering a decimal point", () => {
    expect(formatGroupedSize(1234.7, "en")).toBe("1,234");
  });

  it("has no separator below the grouping threshold", () => {
    expect(formatGroupedSize(512, "en")).toBe("512");
  });
});
