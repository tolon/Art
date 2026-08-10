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
});
