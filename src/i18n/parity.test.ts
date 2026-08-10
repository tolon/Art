import { describe, expect, it } from "vitest";
import en from "./en.json";
import tr from "./tr.json";

/** Every leaf key, as dotted paths, sorted. */
function keysOf(obj: unknown, prefix = ""): string[] {
  if (typeof obj !== "object" || obj === null) return [prefix];
  return Object.entries(obj as Record<string, unknown>)
    .flatMap(([k, v]) => keysOf(v, prefix ? `${prefix}.${k}` : k))
    .sort();
}

describe("the translation catalogues", () => {
  it("have identical key sets", () => {
    const enKeys = keysOf(en);
    const trKeys = keysOf(tr);
    expect(trKeys.filter((k) => !enKeys.includes(k))).toEqual([]);
    expect(enKeys.filter((k) => !trKeys.includes(k))).toEqual([]);
  });

  it("have no empty translations", () => {
    const empty = (obj: unknown, prefix = ""): string[] => {
      if (typeof obj === "string") return obj.trim() === "" ? [prefix] : [];
      if (typeof obj !== "object" || obj === null) return [];
      return Object.entries(obj as Record<string, unknown>).flatMap(([k, v]) =>
        empty(v, prefix ? `${prefix}.${k}` : k),
      );
    };
    expect(empty(en)).toEqual([]);
    expect(empty(tr)).toEqual([]);
  });

  it("interpolate the same variables in both languages", () => {
    // Turkish word order differs from English, so a translation moves
    // `{{count}}` rather than keeping it in place. What it must not do is
    // drop it — i18next renders a missing variable as nothing at all, so
    // "{{count}} packages found" silently becomes " paket bulundu" with no
    // error anywhere. Key parity cannot see that; this can.
    const varsOf = (s: string) =>
      [...s.matchAll(/{{\s*(\w+)/g)].map((m) => m[1]).sort();

    const flat = (obj: unknown, prefix = ""): [string, string][] => {
      if (typeof obj === "string") return [[prefix, obj]];
      if (typeof obj !== "object" || obj === null) return [];
      return Object.entries(obj as Record<string, unknown>).flatMap(([k, v]) =>
        flat(v, prefix ? `${prefix}.${k}` : k),
      );
    };

    const trText = new Map(flat(tr));
    const mismatched = flat(en)
      .filter(([key, text]) => {
        const other = trText.get(key);
        return other !== undefined && varsOf(text).join() !== varsOf(other).join();
      })
      .map(([key]) => key);

    expect(mismatched).toEqual([]);
  });
});
