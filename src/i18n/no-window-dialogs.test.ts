// ART-133: `window.confirm` asks nothing in this application.
//
// wry disables WebView2's own script dialogs, so the browser's `confirm`
// returns without a dialog ever appearing — and every guard shaped like
// `if (!window.confirm(…)) return;` therefore never fires. Thirteen of them
// were in this codebase, four standing between a user and a deleted file on an
// Amiga volume, and the double confirmation before a delete carried a comment
// explaining why two were needed while neither was asked.
//
// It was found by opening a screen, not by a test, because no test here could
// see it: `window.confirm` is a browser API and every unit test runs in node or
// jsdom, where it behaves. This test cannot check the behaviour either — so it
// checks the thing it can, which is that nobody reaches for the API again.

import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";

const SRC = resolve(__dirname, "..");

function sourceFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) out.push(...sourceFiles(full));
    else if (/\.(ts|tsx)$/.test(entry) && !entry.endsWith(".test.ts")) out.push(full);
  }
  return out;
}

function scan(pattern: RegExp): string[] {
  const hits: string[] = [];
  for (const file of sourceFiles(SRC)) {
    readFileSync(file, "utf8")
      .split("\n")
      .forEach((line, index) => {
        if (pattern.test(line)) hits.push(`${relative(SRC, file)}:${index + 1}`);
      });
  }
  return hits;
}

/** `window.alert(` or `window.confirm(` — outside a comment. */
const BANNED = /^(?!\s*(\/\/|\*)).*\bwindow\.(confirm|alert)\s*\(/;

/**
 * `window.prompt` is **not** banned here yet, and the omission is deliberate.
 *
 * The evidence for ART-133 is one observation: a folder was removed with no
 * dialog shown, which means `window.confirm` returned without asking. Nothing
 * has been observed about `prompt`, and the four places that use it — new
 * folder, rename, mark-by-mask, and Aminet's partition picker — sit in screens
 * that have been driven against real material before. A suppressed `prompt`
 * returns null, so those features would silently do nothing, and somebody might
 * have noticed.
 *
 * There is also no drop-in replacement: the dialog plugin offers `confirm`,
 * `ask` and `message`, but no text input. Fixing them means building an input
 * dialog, which is not a change to make on a guess.
 *
 * So they are counted rather than banned, and the count moves only when
 * somebody has looked.
 */
const PROMPTS = /^(?!\s*(\/\/|\*)).*\bwindow\.prompt\s*\(/;

describe("browser dialogs in src/", () => {
  it("nobody calls window.confirm or window.alert", () => {
    expect(
      scan(BANNED),
      "Use `confirm` from @tauri-apps/plugin-dialog instead — it is async, and " +
        "it actually shows a dialog. See ART-133."
    ).toEqual([]);
  });

  it("has exactly the window.prompt call sites somebody has looked at", () => {
    // Four, all predating ART-133 and none yet observed failing. A fifth means
    // somebody reached for the API again; one fewer means somebody fixed one,
    // and this number should move with the evidence that justified it.
    expect(scan(PROMPTS)).toHaveLength(4);
  });

  it("found enough files to prove the scan ran", () => {
    // Guards against the scan silently matching nothing and passing vacuously.
    expect(sourceFiles(SRC).length).toBeGreaterThan(50);
  });
});
