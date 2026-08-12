// `parity.test.ts` proves en.json and tr.json agree with each other.
// `phrase-keys.test.ts` proves every `src/lib` mapper's `Phrase` points at a
// real key. Neither looks at the ~700 literal `t("…")` calls scattered
// through `src/pages` and `src/components` — a typo there (`t("files.functionKeys.viwe")`)
// renders the raw dotted key on screen and nothing here catches it. This
// test does: it reads every `.ts`/`.tsx` file under `src`, finds every call
// to a bare `t(` (or `i18n.t(`) whose first argument is a string literal,
// and asserts the key resolves to a leaf in en.json.
//
// Only literal single-argument-string calls can be checked statically.
// Calls whose first argument is a template literal or a variable
// (`t(\`status.${x}\`)`, `t(phrase.key)`, `t(m.titleKey)`, …) are skipped —
// but counted, so a new dynamic call site has to make this number move
// deliberately rather than silently widening the blind spot. If that
// happens, look at whether the new dynamic key needs the same kind of
// compiler-checked mapping `LhaBrowser.tsx`'s `confidenceLevelKey` uses.

import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import en from "./en.json";

const SRC_ROOT = resolve(__dirname, "..");

/** Every `.ts`/`.tsx` file under `dir`, recursively. */
function sourceFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) {
      out.push(...sourceFiles(full));
    } else if (/\.(ts|tsx)$/.test(entry) && !entry.endsWith(".test.ts")) {
      out.push(full);
    }
  }
  return out;
}

/**
 * Every call to a bare `t(` — including `i18n.t(` — found in `text`.
 *
 * The word boundary before `t` means an identifier merely ending in `t`
 * (`format(`, `.at("x")`) cannot match: `\b` only holds between a
 * non-word character (or start of string) and `t`, and requiring `(`
 * immediately after that `t` rules out longer identifiers like `trim(`.
 */
const CALL_RE = /\bt\(\s*(['"])((?:\\.|(?!\1)[^\\])*)\1|\bt\(\s*(?!['"])/g;

/** Whether `dotted` (e.g. "whdload.outcome.installed") names a string leaf. */
function isLeafKey(dotted: string): boolean {
  const parts = dotted.split(".");
  let node: unknown = en;
  for (const part of parts) {
    if (typeof node !== "object" || node === null) return false;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === "string";
}

/**
 * Whether `dotted` names a leaf, accounting for i18next pluralisation:
 * `t("a.b.filesWrittenOut", { count })` resolves at runtime against
 * `filesWrittenOut_one` / `filesWrittenOut_other` — there is no bare
 * `filesWrittenOut` key in the catalogue at all.
 */
function resolvesAtRuntime(dotted: string): boolean {
  return isLeafKey(dotted) || isLeafKey(`${dotted}_one`) || isLeafKey(`${dotted}_other`);
}

const files = sourceFiles(join(SRC_ROOT, "pages")).concat(
  sourceFiles(join(SRC_ROOT, "components"))
);

const literalKeys: { file: string; key: string }[] = [];
let dynamicCalls = 0;

for (const file of files) {
  const text = readFileSync(file, "utf8");
  for (const match of text.matchAll(CALL_RE)) {
    const literal = match[2];
    if (literal !== undefined) {
      literalKeys.push({ file, key: literal });
    } else {
      dynamicCalls++;
    }
  }
}

describe("literal t(\"…\") calls in src/pages and src/components", () => {
  it("found a plausible number of call sites (sanity check the scan itself ran)", () => {
    // Guards against the scan silently matching nothing (e.g. a moved
    // directory) and the suite passing vacuously.
    expect(literalKeys.length).toBeGreaterThan(500);
  });

  it("every literal key resolves to a real catalogue leaf", () => {
    const broken = literalKeys
      .filter(({ key }) => !resolvesAtRuntime(key))
      .map(({ file, key }) => `${key}  (${file})`);
    expect(broken).toEqual([]);
  });

  it("has exactly the expected number of dynamic (non-literal) call sites", () => {
    // Widening this number on purpose means a new t() call whose key isn't
    // a plain string literal — see the file header for what to do about it.
    // 36 → 39 (Task 3): FileManager.tsx's `writeRefusal`, `copyTo`'s
    // `copyDirection` refusal and the disc footer badge all read `t()` off
    // a `Phrase`'s `.key` — `ISO_WRITE_REFUSAL` (`@/lib/isoPane`) and
    // `copyDirection`'s `"refused"` reason — the same reason
    // `describeCopy`'s callers already do, not a new pattern.
    // 39 → 41 (Task 4): the archive pane reads `ARCHIVE_WRITE_REFUSAL.key`
    // in the same two places the disc reads its own — `writeRefusal` and the
    // footer's read-only badge. Same pattern, one more read-only container.
    // 41 → 43 (Task 5): and the Commodore pane, in those same two places.
    // 43 → 45 (phase 2b): the shell's sidebar toggle reads its label and its
    // title from whichever of two keys the current state calls for.
    // 45 → 46 (2b task 3): the pane header's source combo labels each option
    // from the `labelKey` `@/lib/paneSources` gave it. The keys themselves
    // are literals in that module's own table, and `paneSources.test.ts`
    // proves every non-mount option carries one — so the scan below sees a
    // variable where the catalogue still sees a fixed, tested set.
    // 46 → 49 (2b task 3): F6's refusal (twice — the key's own `reason`, and
    // `moveSelection` re-checking after the icons are added) and the command
    // line's, all reading a `Phrase.key` the same way `copyDirection`'s
    // refusal already does. Both modules' every refusal is enumerated in
    // `phrase-keys.test.ts`, which is the check this one cannot make.
    // 49 → 48 (same task): the permanent collision-policy footer is gone, and
    // its `t(labelKey)` over a three-entry table with it. Settings names the
    // same three keys literally now.
    // 48 → 50: the HDF wizard's custom-size refusal and its size warning, both
    // reading a `Phrase.key` from `@/lib/hdfSize` — same pattern as every
    // other refusal on this list, and every branch of both is enumerated in
    // that module's own test file.
    expect(dynamicCalls).toBe(50);
  });
});
