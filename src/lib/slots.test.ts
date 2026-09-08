// The seven endings a resolved slot can produce, and the one thing that has
// to be true of all of them: the sentence is in **both** catalogues.
//
// A `Phrase` whose key is not a leaf in `en.json` renders the raw dotted key
// on screen; one that is in `en.json` and not in `tr.json` renders English to
// a Turkish user. Neither fails to compile, and neither is caught by
// `parity.test.ts` (which compares the two catalogues to each other, not to
// this file) — so the key check here is the same one
// `src/i18n/literal-keys.test.ts` makes for `t("…")` call sites, applied to
// the keys a `src/lib` mapper builds.

import { describe, expect, it } from "vitest";

import en from "@/i18n/en.json";
import tr from "@/i18n/tr.json";
import type { Installed, SlotKind, SlotState } from "@/lib/osinstall";
import { slotLines } from "@/lib/slots";

/** Whether `dotted` names a string leaf in `catalogue`. */
function isLeafKey(catalogue: unknown, dotted: string): boolean {
  let node: unknown = catalogue;
  for (const part of dotted.split(".")) {
    if (typeof node !== "object" || node === null) return false;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === "string";
}

interface StateOptions {
  id?: string;
  kind?: SlotKind;
  name?: string;
  identity?: string;
  required?: boolean;
  filenames?: string[];
  position?: number;
  requires?: string[];
  found?: SlotState["found"];
  candidates?: string[];
  installed?: Installed;
  blockedBy?: string[];
  notNeeded?: string | null;
}

function state(options: StateOptions = {}): SlotState {
  return {
    slot: {
      id: options.id ?? "package:boingbag-39-1",
      kind: options.kind ?? "package",
      name: options.name ?? "BoingBag 3.9-1",
      identity: options.identity ?? "BoingBag3.9-1",
      artefact: "boingbag-39-1",
      required: options.required ?? false,
      filenames: options.filenames ?? ["BoingBag39-1.lha"],
      provenance: "Haage and Partners (3.9)",
      position: options.position ?? 1,
      requires: options.requires ?? [],
      supersededBy: [],
    },
    found: options.found ?? null,
    candidates: options.candidates ?? [],
    installed: options.installed ?? { state: "no" },
    blockedBy: options.blockedBy ?? [],
    notNeeded: options.notNeeded ?? null,
  };
}

const foundBy = (matchedBy: SlotState["found"] extends null ? never : string, path: string) =>
  ({
    path,
    matchedBy,
    row: null,
    confirmed: null,
  }) as NonNullable<SlotState["found"]>;

describe("slotLines", () => {
  it("gives each of the seven endings its own kind and its own key", () => {
    const cases: [string, SlotState][] = [
      ["found-by-hash", state({ found: foundBy("hash", "D:\\a\\BoingBag39-1.lha") })],
      [
        "found-by-name",
        state({ found: foundBy("top-level-directory", "D:\\a\\renamed.lha") }),
      ],
      [
        "chosen",
        state({
          kind: "rom",
          id: "rom",
          name: "Kickstart",
          identity: "40",
          found: foundBy("chosen", "D:\\roms\\kick.rom"),
        }),
      ],
      ["guessed-by-filename", state({ candidates: ["D:\\a\\BoingBag39-1.lha"] })],
      [
        "ambiguous",
        state({ candidates: ["D:\\a\\one.lha", "D:\\b\\two.lha"] }),
      ],
      ["not-found", state()],
      ["not-needed", state({ notNeeded: "Updater 45.15" })],
    ];

    // Every ending is distinct — no two of these produce the same kind, which
    // is the property that keeps "ART could not confirm it" from being read
    // as "found" and "not needed" from being read as "missing".
    expect(cases.map(([kind]) => kind)).toEqual([...new Set(cases.map(([kind]) => kind))]);

    for (const [kind, one] of cases) {
      const [line] = slotLines([one]);
      expect(line.kind, `${kind} came out as ${line.kind}`).toBe(kind);
      expect(isLeafKey(en, line.phrase.key), `${line.phrase.key} missing from en.json`).toBe(true);
      expect(isLeafKey(tr, line.phrase.key), `${line.phrase.key} missing from tr.json`).toBe(true);
    }
  });

  it("says what it expected only when ART actually knows a name", () => {
    const [named] = slotLines([state()]);
    expect(named.phrase.key).toBe("osinstall.slots.notFound");
    expect(named.phrase.params?.filenames).toBe("BoingBag39-1.lha");

    // "Expected " with nothing after it is a sentence nobody can act on.
    const [unnamed] = slotLines([state({ filenames: [] })]);
    expect(unnamed.phrase.key).toBe("osinstall.slots.notFoundUnnamed");
    expect(unnamed.kind).toBe("not-found");
    expect(isLeafKey(en, unnamed.phrase.key)).toBe(true);
    expect(isLeafKey(tr, unnamed.phrase.key)).toBe(true);
  });

  it("carries the installed badge from the manifest, and nothing when it is silent", () => {
    const ran = slotLines([
      state({ installed: { state: "ran", command: "PKG:C/Updater AmigaOS-Update" } }),
    ])[0];
    expect(ran.installed?.key).toBe("osinstall.slots.installedRan");
    expect(ran.installed?.params?.command).toBe("PKG:C/Updater AmigaOS-Update");

    const placed = slotLines([state({ installed: { state: "placed", at: "C/Foo" } })])[0];
    expect(placed.installed?.key).toBe("osinstall.slots.installedPlaced");

    // Never "not installed": the manifest saying nothing is not a claim, and
    // there may be no tree chosen at all.
    expect(slotLines([state()])[0].installed).toBeNull();

    for (const key of ["osinstall.slots.installedRan", "osinstall.slots.installedPlaced"]) {
      expect(isLeafKey(en, key)).toBe(true);
      expect(isLeafKey(tr, key)).toBe(true);
    }
  });

  it("names what a blocked row waits for, in both catalogues", () => {
    const [line] = slotLines([
      state({ id: "package:boingbag-39-2", blockedBy: ["package:boingbag-39-1"] }),
    ]);
    expect(line.blocked?.key).toBe("osinstall.slots.blocked");
    expect(line.blocked?.params?.needs).toBe("package:boingbag-39-1");
    expect(isLeafKey(en, "osinstall.slots.blocked")).toBe(true);
    expect(isLeafKey(tr, "osinstall.slots.blocked")).toBe(true);
    expect(slotLines([state()])[0].blocked).toBeNull();
  });

  it("sorts by the chain position, never by name or by the order it was handed", () => {
    const lines = slotLines([
      state({ id: "rom", kind: "rom", name: "Kickstart", identity: "40", position: 7 }),
      state({ id: "medium:AmigaOS3.9", kind: "medium", name: "AmigaOS3.9", position: 0 }),
      state({ id: "package:boingbag-39-1", position: 1 }),
    ]);
    expect(lines.map((line) => line.id)).toEqual([
      "medium:AmigaOS3.9",
      "package:boingbag-39-1",
      "rom",
    ]);
    // A Kickstart is the one slot the recipe names only by the major it asks
    // for, so the row says which.
    expect(lines[2].name).toBe("Kickstart 40");
  });

  it("shows every candidate of an ambiguous row rather than picking one", () => {
    const [line] = slotLines([
      state({ candidates: ["D:\\a\\first.lha", "D:\\b\\second.lha"] }),
    ]);
    expect(line.kind).toBe("ambiguous");
    expect(line.file).toBeNull();
    expect(line.candidates).toEqual(["first.lha", "second.lha"]);
    expect(line.phrase.params?.count).toBe(2);
    expect(line.phrase.params?.candidates).toBe("first.lha, second.lha");
  });

  it("a not-needed slot is never reported as missing, even with nothing found", () => {
    // The row the design shows for BoingBag 1's UAE fix: present or absent,
    // an artefact ART has measured as unnecessary is neither found nor
    // missing, and "not found" here would be the confident wrong sentence.
    const [line] = slotLines([
      state({ kind: "overlay", filenames: ["BoingBag39-1-UAE.lha"], notNeeded: "Updater 45.15" }),
    ]);
    expect(line.kind).toBe("not-needed");
    expect(line.phrase.key).toBe("osinstall.slots.notNeeded");
    expect(line.phrase.params?.carries).toBe("Updater 45.15");
  });
});
