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
// ## Why an allow-list rather than zero
//
// Some keys are genuinely reachable only from a screen state this project
// cannot reach yet, and deleting them would lose a translated sentence
// somebody wrote for a reason. Each entry below is named with why, so the list
// is a record rather than a place to hide a mistake — and adding to it is a
// deliberate act a reviewer can see.

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

const HAYSTACK = sources(SRC)
  .filter((path) => !path.includes(join("src", "i18n")))
  .map((path) => readFileSync(path, "utf8"))
  .join("\n");

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
 * Keys with no reader today, each with the reason it is kept.
 *
 * i18next's plural suffixes are stripped before the lookup, so a pluralised
 * key is checked under its base name and does not need an entry here.
 */
const KEPT_WITHOUT_A_READER: Record<string, string> = {
  // ---------------------------------------------------------------------
  // The 27 this check found on the day it was written (ART-179) — 28 leaf
  // keys, of which two are one pluralised pair.
  //
  // Every one is **pre-existing** and belongs to a feature outside the round
  // that added this test. Each was checked by hand: none appears anywhere
  // under `src/` in any form. They are listed rather than deleted because
  // removing another feature's translated sentence — written in two
  // languages, by someone, for a screen that was designed — is not a debt
  // round's call to make in passing. ART-179 is where they get triaged:
  // render it, or remove it, one feature at a time.
  //
  // **The list is closed.** A *new* dead key fails this test, which is the
  // whole point of F2 — the defect that prompted it was a key added and
  // never rendered, and that cannot happen again without someone adding a
  // line here on purpose.
  // ---------------------------------------------------------------------
  "app.name":
    "the shell's own name, rendered from `package.json` rather than the catalogue",
  "artwork.enabled":
    "artwork wave B; a source-enabled toggle and a cache-hit count no screen shows yet",
  "artwork.outcome.cachedBefore":
    "artwork wave B; a source-enabled toggle and a cache-hit count no screen shows yet",
  "collection.status.indexed":
    "collection wave C; a status line the studio never grew",
  "common.continue":
    "a generic affirmative no dialog in ART uses — every one names its own verb",
  "dashboard.noStats":
    "the home screen's statistics panel, designed and not built",
  "dashboard.statistics":
    "the home screen's statistics panel, designed and not built",
  "distro.note.amikit.adaptOnly":
    "SD-2's distribution notes; the profiles ship, the per-distro note panel does not",
  "distro.note.amikit.ownIt":
    "SD-2's distribution notes; the profiles ship, the per-distro note panel does not",
  "distro.note.baseline.boingBags":
    "SD-2's distribution notes; the profiles ship, the per-distro note panel does not",
  "distro.note.baseline.recipe":
    "SD-2's distribution notes; the profiles ship, the per-distro note panel does not",
  "distro.note.baseline.romFamily":
    "SD-2's distribution notes; the profiles ship, the per-distro note panel does not",
  "distro.note.baseline.yourMedia":
    "SD-2's distribution notes; the profiles ship, the per-distro note panel does not",
  "distro.note.caffeineos.download":
    "SD-2's distribution notes; the profiles ship, the per-distro note panel does not",
  "distro.note.caffeineos.network":
    "SD-2's distribution notes; the profiles ship, the per-distro note panel does not",
  "distro.note.coffinos.demoware":
    "SD-2's distribution notes; the profiles ship, the per-distro note panel does not",
  "distro.note.coffinos.romFamily":
    "SD-2's distribution notes; the profiles ship, the per-distro note panel does not",
  "files.pane.copyTitle":
    "commander chrome — per-row tooltips and a folder suffix the TC presentation dropped",
  "files.pane.deleteTitle":
    "commander chrome — per-row tooltips and a folder suffix the TC presentation dropped",
  "files.pane.folderSuffix":
    "commander chrome — per-row tooltips and a folder suffix the TC presentation dropped",
  "gameindex.empty":
    "G10's empty and no-match states; the studio renders its own today",
  "gameindex.noMatch":
    "G10's empty and no-match states; the studio renders its own today",
  "gameindex.statedBy":
    "G10's empty and no-match states; the studio renders its own today",
  "pistorm.card.configSets":
    "PiStorm card panel rows for facts `read_card` does not yet report",
  "pistorm.card.kernelFound":
    "PiStorm card panel rows for facts `read_card` does not yet report",
  "preload.card.heading":
    "preload screen headings the panel replaced with its own",
  "preload.tool.heading":
    "preload screen headings the panel replaced with its own",
};

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
