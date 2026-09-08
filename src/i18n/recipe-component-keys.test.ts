// ART-224. An install recipe is data a person edits — a JSON file in the Rust
// tree, with no compiler between it and the screen — and since this round it
// may name an **i18n key** for a component's own row on the install screen. A
// typo in one renders the raw dotted string beside a checkbox the user is
// being asked to decide about.
//
// The same hand-off `distro-registry-keys.test.ts` describes, for the same
// reason and against both catalogues: the screen reads these keys through a
// variable, so `literal-keys.test.ts` counts and skips them, and a key that
// exists only in `en.json` is a Turkish screen with an English label on it.
//
// The Rust side holds the other half — that a recipe whose components share a
// medium labels *every* row
// (`recipe::tests::a_recipe_whose_components_share_a_medium_labels_every_row`),
// which is the rule that makes declaring one non-optional where it matters.
// This file checks that whatever was declared actually resolves.

import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import en from "./en.json";
import tr from "./tr.json";

const RECIPES = resolve(
  __dirname,
  "..",
  "..",
  "src-tauri",
  "src",
  "core",
  "osinstall",
  "recipes"
);

interface Component {
  id: string;
  media: string;
  label_key?: string;
}

/** Every shipped release recipe, read off disk rather than listed here — a
 *  recipe added to the folder is checked without anybody remembering to add
 *  it. Package recipes live in `recipes/packages/` and are a subdirectory, so
 *  `readdirSync` on the top level picks up releases only.
 *
 *  The folder also holds `guide.en.json` / `guide.tr.json` — the drop-folder
 *  guide's own two languages (design § 3.7), which live beside the recipes
 *  because that is what they are composed from. They are excluded **by
 *  name**, not by the absence of the very field being guarded (round 2
 *  whole-branch review, L10): filtering on `Array.isArray(components)` meant
 *  a *release* recipe that lost its `components` array was silently dropped
 *  from every check below instead of failing loudly, and the only backstop
 *  fired when all of them went at once. A recipe with no components now
 *  throws here, which is the behaviour that existed before the guide files
 *  arrived. */
const NOT_A_RECIPE = /^guide\./;

function recipes(): { file: string; components: Component[] }[] {
  return readdirSync(RECIPES)
    .filter((name) => name.endsWith(".json") && !NOT_A_RECIPE.test(name))
    .map((file) => {
      const parsed = JSON.parse(readFileSync(resolve(RECIPES, file), "utf8")) as {
        components?: Component[];
      };
      if (!Array.isArray(parsed.components)) {
        throw new Error(`${file} is in the recipes folder and declares no components`);
      }
      return { file, components: parsed.components };
    });
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

function labelled(): { file: string; id: string; key: string }[] {
  return recipes().flatMap(({ file, components }) =>
    components
      .filter((component) => component.label_key)
      .map((component) => ({ file, id: component.id, key: component.label_key! }))
  );
}

describe("an install recipe's component label keys", () => {
  it("has recipes, and rows that declare a label", () => {
    // A folder that failed to read, or a rename that emptied the list, would
    // make every assertion below vacuously true.
    expect(recipes().length).toBeGreaterThan(0);
    expect(labelled().length).toBeGreaterThan(0);
    // **Every release recipe in the folder is checked, and the count says
    // so** (L10). A file quietly excluded is a file nothing below looks at,
    // and the `> 0` guard above only fires if they all go at once.
    const shipped = readdirSync(RECIPES).filter(
      (name) => name.endsWith(".json") && !NOT_A_RECIPE.test(name)
    );
    expect(recipes().map((r) => r.file).sort()).toEqual(shipped.sort());
  });

  it("names only labels that exist in English", () => {
    const missing = labelled()
      .filter(({ key }) => !resolves(en, key))
      .map(({ file, id, key }) => `${file}: ${id} → ${key}`);
    expect(missing).toEqual([]);
  });

  it("names only labels that exist in Turkish", () => {
    // The half parity cannot reach: `en`/`tr` parity is proved elsewhere, but
    // a key present in *neither* catalogue is a pair parity is happy with.
    const missing = labelled()
      .filter(({ key }) => !resolves(tr, key))
      .map(({ file, id, key }) => `${file}: ${id} → ${key}`);
    expect(missing).toEqual([]);
  });

  it("keeps every label under the namespace this screen owns", () => {
    // So a row cannot quietly borrow a sentence written for another screen
    // and have it change underneath it.
    for (const { file, id, key } of labelled()) {
      expect(key.startsWith("osinstall.components.name."), `${file}: ${id} → ${key}`).toBe(true);
    }
  });

  it("writes no label nothing claims", () => {
    // The direction ART-179 was filed for, from the other side: ten keys sat
    // three days on a dead-key allow-list under a reason that was untrue,
    // because the scanner could not see the Rust-side data that named them.
    // This one can — so a label left behind by a renamed component fails
    // here rather than living on as an exemption.
    const declared = new Set(labelled().map(({ key }) => key));
    const catalogue = (en as unknown as {
      osinstall: { components: { name?: Record<string, Record<string, string>> } };
    }).osinstall.components.name;
    const orphans: string[] = [];
    for (const [group, names] of Object.entries(catalogue ?? {})) {
      for (const leaf of Object.keys(names)) {
        const key = `osinstall.components.name.${group}.${leaf}`;
        if (!declared.has(key)) orphans.push(key);
      }
    }
    expect(orphans).toEqual([]);
  });
});
