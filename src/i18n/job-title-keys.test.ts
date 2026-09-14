// ART-301. A job's title is a catalogue key that Rust names —
// `JobTitle::new("components.jobBar.title.…")` in `src-tauri/src/commands` —
// and `JobBar.tsx` renders it through a variable, so `literal-keys.test.ts`
// counts it and skips it. `JOB_TITLE_KEYS` in `@/lib/jobs` is where the keys
// are written out. This file checks that list against both catalogues. A key
// that exists only in `en.json` is a Turkish screen with an English title on
// it, which is the defect this round is named for.

import { describe, expect, it } from "vitest";

import { JOB_TITLE_KEYS } from "@/lib/jobs";
import en from "./en.json";
import tr from "./tr.json";

const PREFIX = "components.jobBar.title.";

function leaf(catalogue: unknown, key: string): string | undefined {
  let node: unknown = catalogue;
  for (const part of key.split(".")) {
    if (typeof node !== "object" || node === null) return undefined;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === "string" ? node : undefined;
}

/** The sentences a key renders as: the key itself, or a complete `_one` /
 *  `_other` pair. Anything else is a string naming what is wrong. */
function forms(catalogue: unknown, key: string): string[] | "missing" | "mixed" {
  const bare = leaf(catalogue, key);
  const one = leaf(catalogue, `${key}_one`);
  const other = leaf(catalogue, `${key}_other`);
  if (bare !== undefined && one === undefined && other === undefined) return [bare];
  if (bare === undefined && one !== undefined && other !== undefined) return [one, other];
  if (bare === undefined && one === undefined && other === undefined) return "missing";
  return "mixed";
}

/** The `{{name}}`s a set of sentences interpolates, sorted, once each. */
function placeholders(sentences: string[]): string[] {
  return [
    ...new Set(sentences.flatMap((s) => [...s.matchAll(/{{\s*(\w+)/g)].map((m) => m[1]))),
  ].sort();
}

function broken(catalogue: unknown): string[] {
  return JOB_TITLE_KEYS.flatMap((key) => {
    const found = forms(catalogue, key);
    return typeof found === "string" ? [`${key}: ${found}`] : [];
  });
}

describe("job title keys (ART-301)", () => {
  it("lists each key once, sorted, under the job bar's own namespace", () => {
    expect([...JOB_TITLE_KEYS]).toEqual([...new Set(JOB_TITLE_KEYS)].sort());
    for (const key of JOB_TITLE_KEYS) expect(key.startsWith(PREFIX), key).toBe(true);
  });

  it("resolves every key in English, as one sentence or a complete plural pair", () => {
    expect(broken(en)).toEqual([]);
  });

  it("resolves every key in Turkish, as one sentence or a complete plural pair", () => {
    // The half parity cannot reach: a key present in *neither* catalogue is a
    // pair `parity.test.ts` is perfectly happy with.
    expect(broken(tr)).toEqual([]);
  });

  it("uses a plural pair exactly when the sentence counts", () => {
    const wrong: string[] = [];
    for (const key of JOB_TITLE_KEYS) {
      const sentences = forms(en, key);
      if (typeof sentences === "string") continue; // reported by the test above
      const counts = placeholders(sentences).includes("count");
      if (counts !== (sentences.length === 2)) wrong.push(key);
    }
    expect(wrong).toEqual([]);
  });
});
