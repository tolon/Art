// ART-301. A job's title is a catalogue key that Rust names —
// `JobTitle::new("components.jobBar.title.…")` in `src-tauri/src/commands` —
// and `JobBar.tsx` renders it through a variable, so `literal-keys.test.ts`
// counts it and skips it. `JOB_TITLE_KEYS` in `@/lib/jobs` is where the keys
// are written out. This file holds that list to both catalogues, and it
// **reads the Rust files themselves**, not a copy of them ("a test that reads
// a table instead of the file is a copy, and copies drift" — CLAUDE.md).
// The precedent is `recipe-component-keys.test.ts`, which reads Rust-tree
// data with `readFileSync` the same way.

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";
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

// --- The Rust side --------------------------------------------------------

const RUST_SRC = resolve(__dirname, "..", "..", "src-tauri", "src");

/** The type's own module and the job runner. Their tests build titles from
 *  keys on purpose, and neither is a place a job starts. */
const MECHANISM = new Set([join("core", "jobs", "mod.rs"), join("commands", "jobs.rs")]);

function rustFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) return rustFiles(path);
    return path.endsWith(".rs") ? [path] : [];
  });
}

interface Site {
  file: string;
  key: string;
  /** The values the chain passes, sorted: every `.text("name", …)`, plus
   *  `count` for a `.count(…)`. */
  params: string[];
}

function scan(): { sites: Site[]; calls: number; letCalls: number } {
  const sites: Site[] = [];
  let calls = 0;
  let letCalls = 0;
  for (const path of rustFiles(RUST_SRC)) {
    const file = relative(RUST_SRC, path);
    if (MECHANISM.has(file)) continue;
    const text = readFileSync(path, "utf8");
    calls += text.match(/JobTitle::new\(/g)?.length ?? 0;
    // rustfmt may wrap a long site so `let title =` and `JobTitle::new(`
    // land on separate lines; `\s` already spans the newline, so this reads
    // the site however rustfmt wraps it, not just the one-line shape.
    letCalls += text.match(/let\s+title\s*=\s*JobTitle::new\(/g)?.length ?? 0;
    // A site is one statement, so everything up to its `;` is its chain.
    for (const m of text.matchAll(/JobTitle::new\(\s*"([^"]+)"\s*\)([^;]*);/g)) {
      const chain = m[2];
      const params = [...chain.matchAll(/\.text\(\s*"(\w+)"/g)].map((p) => p[1]);
      if (/\.count\(/.test(chain)) params.push("count");
      sites.push({ file, key: m[1], params: params.sort() });
    }
  }
  return { sites, calls, letCalls };
}

const RUST = scan();

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

describe("the job titles Rust sets (ART-301)", () => {
  it("finds all forty sites", () => {
    // 40 on 2026-09-14: 33 titles that were `format!` and 7 that were fixed
    // strings, in 19 command files. A new job moves this number on purpose.
    // A moved folder or a renamed type moves it to 0.
    expect(
      RUST.sites.length,
      "The number of JobTitle::new(…) sites in src-tauri/src changed. If you added or removed a " +
        "background job on purpose, change 40 here and in the comment above in the same commit, " +
        "with its key in JOB_TITLE_KEYS and both catalogues. If you did not, a site was lost."
    ).toBe(40);
  });

  it("leaves no listed key without a Rust site", () => {
    // The other direction: a key whose job was removed or renamed would stay
    // in both catalogues, reachable only through this list, and never shown.
    const named = new Set(RUST.sites.map((s) => s.key));
    expect(JOB_TITLE_KEYS.filter((key) => !named.has(key))).toEqual([]);
  });

  it("names every key as a literal, in its own `let title` statement", () => {
    // A key built at run time, or a title built inline inside a spawn call,
    // would escape the two checks below — so neither shape is allowed.
    expect(RUST.sites.length).toBe(RUST.calls);
    expect(RUST.letCalls).toBe(RUST.calls);
  });

  it("names only keys the list holds", () => {
    const known = new Set<string>(JOB_TITLE_KEYS);
    const unknown = RUST.sites.filter((s) => !known.has(s.key)).map((s) => `${s.file} → ${s.key}`);
    expect(unknown).toEqual([]);
  });

  it("passes exactly the values its sentence interpolates", () => {
    // i18next renders a missing value as nothing and ignores an extra one,
    // so both directions are silent on screen.
    const wrong = RUST.sites.flatMap((s) => {
      const sentences = forms(en, s.key);
      if (typeof sentences === "string") return []; // an unknown key is reported above
      const wanted = placeholders(sentences);
      return wanted.join() === s.params.join()
        ? []
        : [`${s.file} → ${s.key}: passes [${s.params.join(", ")}], the sentence names [${wanted.join(", ")}]`];
    });
    expect(wrong).toEqual([]);
  });
});
