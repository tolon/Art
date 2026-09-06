// The install-media hash table's own two counts — "186 rows" and "151 of
// them unchecked" — are stated as literal digits inside
// `osinstall.mediaId.unconfirmed`, in both catalogues (final-review.md M3).
// `mediahash.rs`'s own
// `the_shipped_record_confirms_35_of_the_186_rows_and_not_the_other_151`
// test already fails when `media_hashes_confirmed.json` is regenerated
// against a different disk set — but that test points at itself, not at
// these two sentences, so a person who "fixes" it by editing 35/151 to a new
// pair ships a screen that states the old numbers.
//
// This is the guard `docs/session-log.md` asked for: read the same two data
// files `mediahash.rs` reads, compute the same two counts, and fail unless
// both catalogues' prose still names them. Same family as
// `distro-registry-keys.test.ts` and `rom-table-check.py` — read the table
// instead of copying it into a comment.

import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import en from "./en.json";
import tr from "./tr.json";

const CORE = resolve(__dirname, "..", "..", "src-tauri", "src", "core", "osinstall");
const TABLE_PATH = resolve(CORE, "media_hashes.json");
const CONFIRMED_PATH = resolve(CORE, "media_hashes_confirmed.json");

interface MediaRow {
  md5: string;
}

interface ConfirmedCheck {
  md5: string[];
}

/** Mirrors `mediahash.rs::parse_confirmed`: every check's hashes, flattened
 *  and lowercased, first occurrence wins on a repeat — though for a *count*
 *  a repeat inside one shipped file would be a duplicate row's problem, not
 *  this test's, and de-duplicating is what makes the count immune to it. */
function confirmedMd5s(): Set<string> {
  const parsed = JSON.parse(readFileSync(CONFIRMED_PATH, "utf8")) as { checks: ConfirmedCheck[] };
  const seen = new Set<string>();
  for (const check of parsed.checks) {
    for (const md5 of check.md5) seen.add(md5.toLowerCase());
  }
  return seen;
}

function tableRows(): MediaRow[] {
  const parsed = JSON.parse(readFileSync(TABLE_PATH, "utf8")) as { media: MediaRow[] };
  return parsed.media;
}

/** The two counts the prose claims, computed from the data rather than
 *  quoted from a previous run — exactly `confirmations().len()` and the
 *  `unconfirmed` count `mediahash.rs`'s own test asserts. */
function standingCounts(): { total: number; unconfirmed: number } {
  const rows = tableRows();
  const confirmed = confirmedMd5s();
  const unconfirmed = rows.filter((row) => !confirmed.has(row.md5.toLowerCase())).length;
  return { total: rows.length, unconfirmed };
}

/** Whether `text` names `n` as a standalone number — not merely as a
 *  substring of some other, larger figure. */
function namesNumber(text: string, n: number): boolean {
  return new RegExp(`(?<!\\d)${n}(?!\\d)`).test(text);
}

describe("the install-media unconfirmed count is a fact about the data, not a memory of it", () => {
  it("has rows to count", () => {
    // A table emptied or unparsable would make every assertion below
    // vacuously true.
    expect(tableRows().length).toBeGreaterThan(0);
  });

  it("states the standing counts, today", () => {
    // Not a regression guard by itself — a note for whoever reads a failure
    // below, so "151 of 186" reads as a measurement rather than a mystery.
    expect(standingCounts()).toEqual({ total: 186, unconfirmed: 151 });
  });

  it.each([
    ["English", en],
    ["Turkish", tr],
  ])("%s's unconfirmed sentence names the table's real total and real unconfirmed count", (_lang, catalogue) => {
    const { total, unconfirmed } = standingCounts();
    const sentence = (catalogue as { osinstall: { mediaId: { unconfirmed: string } } }).osinstall
      .mediaId.unconfirmed;
    expect(sentence).toBeTruthy();
    expect(namesNumber(sentence, total)).toBe(true);
    expect(namesNumber(sentence, unconfirmed)).toBe(true);
  });
});
