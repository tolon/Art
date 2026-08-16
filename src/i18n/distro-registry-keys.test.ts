// The distro registry is data a person edits — a JSON file in the Rust tree,
// with no compiler between it and the screen. Its `post_install_notes` are
// **i18n keys**, and a typo in one renders the raw dotted string to a user.
//
// `literal-keys.test.ts` cannot catch it: the screen reads those keys through
// a variable, which that scan counts and skips. This is the check it hands off
// to, and it runs against *both* catalogues rather than just English, because
// a note that exists only in `en.json` is a Turkish screen with an English
// sentence on it.
//
// The Rust side has its own tests over the same file — that every token a
// profile names is one Emu68 has, that ids and config sets are unique, that
// nothing claims to be buildable yet. Between the two, every field of every
// profile is checked by something.

import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import en from "./en.json";
import tr from "./tr.json";

const REGISTRY = resolve(
  __dirname,
  "..",
  "..",
  "src-tauri",
  "src",
  "core",
  "distro",
  "registry.json"
);

interface Profile {
  id: string;
  name: string;
  homepage: string;
  post_install_notes: string[];
}

function profiles(): Profile[] {
  const parsed = JSON.parse(readFileSync(REGISTRY, "utf8")) as { profiles: Profile[] };
  return parsed.profiles;
}

/** Whether a dotted key resolves to a string in a catalogue. */
function resolves(catalogue: unknown, key: string): boolean {
  let node: unknown = catalogue;
  for (const part of key.split(".")) {
    if (typeof node !== "object" || node === null) return false;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === "string";
}

describe("the distro registry's i18n keys", () => {
  it("has profiles to check", () => {
    // A registry that failed to parse, or one emptied by a bad edit, would
    // make every assertion below vacuously true.
    expect(profiles().length).toBeGreaterThan(0);
  });

  it("names only notes that exist in English", () => {
    const missing: string[] = [];
    for (const profile of profiles()) {
      for (const key of profile.post_install_notes) {
        if (!resolves(en, key)) missing.push(`${profile.id} → ${key}`);
      }
    }
    expect(missing).toEqual([]);
  });

  it("names only notes that exist in Turkish", () => {
    // The half `literal-keys.test.ts` never reaches: parity between the two
    // catalogues is proved elsewhere, but a key present in *neither* is a key
    // parity is perfectly happy with.
    const missing: string[] = [];
    for (const profile of profiles()) {
      for (const key of profile.post_install_notes) {
        if (!resolves(tr, key)) missing.push(`${profile.id} → ${key}`);
      }
    }
    expect(missing).toEqual([]);
  });

  it("keeps its notes under the distro namespace", () => {
    // So a profile cannot quietly borrow a sentence written for another screen
    // and have it change underneath it.
    for (const profile of profiles()) {
      for (const key of profile.post_install_notes) {
        expect(key.startsWith("distro.note."), `${profile.id} → ${key}`).toBe(true);
      }
    }
  });

  it("gives every profile a page for the user to go to, and never a file", () => {
    // ART never downloads a distribution. A `homepage` that pointed at an
    // image would be an invitation to grow a fetch button.
    for (const profile of profiles()) {
      expect(profile.homepage).toMatch(/^https:\/\//);
      expect(profile.homepage).not.toMatch(/\.(img|zip|7z|gz|xz)$/i);
    }
  });
});
