// Keys the catalogues carry that nothing renders.
//
// **Why this exists** (ART-080 review, F2). `files.hostDelete.confirm` was
// written into both catalogues, in both languages, and no code path ever
// called `t()` on it — so the sentence that was supposed to tell the user
// their files were going to the Recycle Bin was never on screen, and a report
// claimed it was. A dead key is worse than a missing one: `pnpm test`'s parity
// check is satisfied, both languages agree, the string reads correctly to
// anyone grepping the JSON, and the screen says nothing.
//
// The parity test asks "do the two catalogues match". This one asks the other
// half: **is anything reaching this key at all.**
//
// ## The allow-list is empty
//
// It was not always (ART-179). An entry is still allowed — a key reachable
// only from a screen state this project cannot reach yet, named with why, so
// the list is a record rather than a place to hide a mistake. But an entry is
// also where a checker's own blind spot goes to look reasonable: ten keys sat
// here under a reason that was simply not true, because this scan could not
// see the file that names them. **Before allow-listing a key, look for the
// reader.** If one exists, teach `DATA_FILES` about it instead.

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";

import en from "./en.json";

const SRC = resolve(__dirname, "..");

/** Every leaf key in the catalogue, dotted. */
function leafKeys(node: unknown, prefix = ""): string[] {
  if (typeof node !== "object" || node === null) return [prefix];
  return Object.entries(node as Record<string, unknown>).flatMap(([key, value]) =>
    leafKeys(value, prefix ? `${prefix}.${key}` : key)
  );
}

/** Every `.ts`/`.tsx` source under `src/`, except the catalogues and tests. */
function sources(dir: string): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) return sources(path);
    if (!/\.tsx?$/.test(path)) return [];
    if (/\.test\.tsx?$/.test(path)) return [];
    return [path];
  });
}

/**
 * Data files **outside `src/`** that name i18n keys.
 *
 * A reader does not have to be a `t()` call. The distro registry is a JSON
 * file in the Rust tree whose `post_install_notes` are catalogue keys, and
 * `OsBuilder.tsx` renders them with `{t(key)}` through a variable — so the
 * ten `distro.note.*` sentences are on screen today while nothing under
 * `src/` mentions them by name.
 *
 * **This scan missed them and they were nearly deleted for it** (ART-179).
 * They spent a round on the allow-list under the reason "the per-distro note
 * panel does not ship", which was not true; the panel shipped. A checker that
 * cannot see a reader produces exactly the sentence this project is most
 * expensive at — confident, wrong, and believed. Teaching it the reader is
 * worth more than ten allow-list lines, because the next profile to name a
 * note needs nobody to remember this.
 *
 * The keys appear here quoted and closed, so `isReachable`'s delimiter rule
 * applies unchanged.
 */
const DATA_FILES = [
  resolve(SRC, "..", "src-tauri", "src", "core", "distro", "registry.json"),
  // The same shape, arriving a second time (ART-224): an install recipe may
  // name an `osinstall.components.name.*` key for a component's own row, and
  // `OsInstall.tsx` resolves it through a variable exactly as `OsBuilder.tsx`
  // resolves a distro note. Added when the first recipe used one rather than
  // after a round nearly deleted five live labels — which is the whole lesson
  // of the paragraph above.
  resolve(SRC, "..", "src-tauri", "src", "core", "osinstall", "recipes", "amigaos-3.2.json"),
  resolve(SRC, "..", "src-tauri", "src", "core", "osinstall", "recipes", "amigaos-3.2.2.json"),
  resolve(SRC, "..", "src-tauri", "src", "core", "osinstall", "recipes", "amigaos-3.9.json"),
];

const HAYSTACK = [
  ...sources(SRC)
    .filter((path) => !path.includes(join("src", "i18n")))
    .map((path) => readFileSync(path, "utf8")),
  ...DATA_FILES.map((path) => readFileSync(path, "utf8")),
].join("\n");

const BACKTICK = String.fromCharCode(96);

/**
 * Whether anything in `src/` could resolve `key`.
 *
 * **The delimiters are the whole test.** A first version matched any *prefix*
 * of the key anywhere in the source, so `files.hostDelete.confirm` counted as
 * reachable because the string `"files` appears a thousand times — and both
 * mutations written to check it (deleting the render, and planting a key
 * nothing reads) passed. That is the vacuous-guard failure this project keeps
 * meeting, met once more while writing the guard against another one.
 *
 * Two shapes count, and nothing else:
 *
 * - **The whole key, closed.** `"files.hostDelete.confirm"` — the closing
 *   quote is what stops a longer key from vouching for a shorter one.
 * - **A template-literal prefix that continues into an interpolation**, e.g.
 *   a mapper building `gameindex.provenance.${from}`. That is a real pattern
 *   this codebase uses on purpose. Only a backtick counts, and only
 *   immediately before the interpolation, so an ordinary quoted prefix cannot
 *   stand in for a key it does not build.
 */
function isReachable(key: string): boolean {
  for (const quote of ['"', "'", BACKTICK]) {
    if (HAYSTACK.includes(quote + key + quote)) return true;
  }
  const parts = key.split(".");
  for (let cut = parts.length - 1; cut >= 1; cut--) {
    if (HAYSTACK.includes(BACKTICK + parts.slice(0, cut).join(".") + ".${")) return true;
  }
  return false;
}

/**
 * Keys with no reader, kept on purpose.
 *
 * **Empty, and that is the point** (ART-179). It held twenty-seven entries on
 * the day this check was written, every one deferred with the same reasoning:
 * removing another feature's translated sentence is not a debt round's call
 * to make in passing. Triaged on 2026-08-23, they came apart into two halves
 * that wanted opposite treatment:
 *
 * - **Ten were not dead.** The `distro.note.*` sentences are rendered by
 *   `OsBuilder.tsx` from keys the Rust-side distro registry names, which this
 *   scan could not see. They are kept, and `DATA_FILES` above now sees them,
 *   so they are held by a reader rather than by a line here.
 * - **Seventeen were dead and are gone** — eighteen leaves, one a pluralised
 *   pair. Each had been superseded or belonged to a screen nobody built:
 *   `app.name` (the shell reads `package.json`), `common.continue` (every
 *   dialog names its own verb), `files.pane.folderSuffix` (the TC
 *   presentation writes `[name]` instead), `files.pane.copyTitle` /
 *   `deleteTitle` (the function-key bar builds its tooltip from the action's
 *   own label), the three `gameindex.*` empty states the studio renders its
 *   own of, both `preload.*.heading`s the panel replaced, and the artwork,
 *   collection, dashboard and PiStorm-card rows for features that do not
 *   exist. One of them, `pistorm.card.kernelFound`, still said "Emu68.img is
 *   on the card" — the ART-103 sentence, wrong since that fix, waiting.
 *
 * **Nothing was lost.** The removed sentences are in the commit that removed
 * them, and a feature that arrives writes the key it actually renders rather
 * than inheriting one written for a screen nobody built.
 *
 * Adding an entry here is still allowed and is a decision a reviewer can see.
 * An empty list means every key in both catalogues has a reader.
 */
const KEPT_WITHOUT_A_READER: Record<string, string> = {};

describe("the catalogue has no dead keys", () => {
  it("every key is reachable from somewhere in src/", () => {
    const dead = leafKeys(en)
      .filter((key) => {
        const base = key.replace(/_(one|other|zero|few|many)$/, "");
        return !(base in KEPT_WITHOUT_A_READER) && !isReachable(base);
      })
      .sort();

    // A dead key is a sentence somebody wrote, in two languages, that nobody
    // will ever read. Either render it or remove it.
    expect(dead).toEqual([]);
  });

  it("the matcher does not vouch for a key by matching a prefix of it", () => {
    // The property the first version got wrong, pinned so it cannot come
    // back: a key that shares every segment but the last with a real one is
    // not reachable just because the real one is.
    const real = leafKeys(en).find((key) => key.split(".").length >= 3 && isReachable(key));
    expect(real, "the catalogue should hold at least one reachable nested key").toBeTruthy();

    const parts = real!.split(".");
    const impostor = [...parts.slice(0, -1), "thisSegmentIsNotInTheSource"].join(".");
    expect(isReachable(impostor)).toBe(false);
  });

  it("the allow-list has no stale entries", () => {
    // An entry that has since gained a reader is an excuse outliving its
    // reason, and the next person will read it as still true. Both halves
    // are asserted, because the first version of this test checked only
    // that the key still existed — which let an entry whose stated reason
    // had become false ("a generic affirmative no dialog in ART uses")
    // survive a screen actually starting to use it. Proved by giving one
    // allow-listed key a real reader: all three tests passed (ART-180).
    const keys = new Set(leafKeys(en).map((k) => k.replace(/_(one|other|zero|few|many)$/, "")));
    for (const key of Object.keys(KEPT_WITHOUT_A_READER)) {
      expect(keys.has(key), `${key} is allow-listed and no longer in the catalogue`).toBe(true);
      expect(
        isReachable(key),
        `${key} is allow-listed as unrendered but something now renders it — delete the entry`,
      ).toBe(false);
    }
  });
});
