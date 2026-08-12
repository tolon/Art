import { describe, expect, it } from "vitest";

import {
  COMMAND_HISTORY_LIMIT,
  highestTabNumber,
  readSession,
  toSession,
} from "@/lib/paneSession";
import { singleTabSet, type PaneTab, type TabSet } from "@/lib/paneTabs";
import { defaultSortState } from "@/lib/sort";

function tab(id: string, path = "D:\\amiga"): PaneTab {
  return {
    id,
    location: { kind: "local", path },
    sort: defaultSortState(),
    filter: "",
  };
}

const LEFT = singleTabSet(tab("tab-1", "D:\\amiga\\Games"));
const RIGHT: TabSet = { tabs: [tab("tab-2"), tab("tab-7", "E:\\dl")], active: 1 };

/** What actually happens between two runs of the application: the object is
 *  written to a JSON file and parsed back out of one. */
function throughJson(value: unknown): unknown {
  return JSON.parse(JSON.stringify(value));
}

describe("the session round trip", () => {
  it("survives being written to JSON and read back", () => {
    // This is the whole point of the module: session restore is the one claim
    // in phase 2b that cannot be true by construction, and the only thing
    // that can check it without launching the application is this.
    const saved = toSession(LEFT, RIGHT, "right", ["cd D:\\amiga", "*.adf"]);
    const restored = readSession(throughJson(saved));

    expect(restored).not.toBeNull();
    expect(restored!.focused).toBe("right");
    expect(restored!.commandHistory).toEqual(["cd D:\\amiga", "*.adf"]);
    expect(restored!.left.tabs).toHaveLength(1);
    expect(restored!.right.tabs.map((t) => t.id)).toEqual(["tab-2", "tab-7"]);
    // The active tab, its path, its sort order and its mask all come back —
    // which is exactly what acceptance point 11 asks for.
    expect(restored!.right.active).toBe(1);
    expect(restored!.right.tabs[1].location.path).toBe("E:\\dl");
    expect(restored!.right.tabs[1].sort).toEqual(defaultSortState());
  });

  it("brings a tab back inside the image it was living in", () => {
    // A tab is a place, and a place can be inside `Lotus.adf`. If the host
    // did not survive the trip, `[..]` would have nowhere to go and the user
    // would be stuck inside a disk after every restart.
    const inside: PaneTab = {
      id: "tab-3",
      location: {
        kind: "adf",
        path: "D:\\amiga\\Lotus.adf",
        dirBlock: 881,
        trail: [{ name: "Data", block: 880 }],
        host: { path: "D:\\amiga", name: "Lotus.adf" },
      },
      sort: defaultSortState(),
      filter: "*.info",
    };
    const saved = toSession(singleTabSet(inside), RIGHT, "left", []);
    const restored = readSession(throughJson(saved));

    const location = restored!.left.tabs[0].location;
    expect(location.kind).toBe("adf");
    expect(location.kind === "adf" && location.dirBlock).toBe(881);
    expect(location.kind === "adf" && location.host).toEqual({
      path: "D:\\amiga",
      name: "Lotus.adf",
    });
    expect(restored!.left.tabs[0].filter).toBe("*.info");
  });

  it("caps the command history on the way out and on the way back", () => {
    const long = Array.from({ length: 50 }, (_, i) => `cd D:\\${i}`);
    expect(toSession(LEFT, RIGHT, "left", long).commandHistory).toHaveLength(
      COMMAND_HISTORY_LIMIT
    );
    const restored = readSession({ left: LEFT, right: RIGHT, commandHistory: long });
    expect(restored!.commandHistory).toHaveLength(COMMAND_HISTORY_LIMIT);
  });
});

describe("readSession refuses what it cannot vouch for", () => {
  it("returns null for anything that is not a session", () => {
    // `settings.json` is a file a user can edit, an older ART may have
    // written, and a half-finished write may have truncated.
    for (const value of [null, undefined, 42, "left", [], {}]) {
      expect(readSession(value), JSON.stringify(value) ?? "undefined").toBeNull();
    }
  });

  it("refuses the whole session when either pane is unusable", () => {
    // One restored pane and one empty one is a state the screen has no way to
    // explain, so it is not a state this produces.
    expect(readSession({ left: LEFT, right: { tabs: [], active: 0 } })).toBeNull();
    expect(readSession({ left: { tabs: [tab("a")], active: 5 }, right: RIGHT })).toBeNull();
    expect(readSession({ right: RIGHT })).toBeNull();
  });

  it("drops a malformed command history without losing the tabs", () => {
    // A dropdown is not worth losing your tabs over.
    const restored = readSession({ left: LEFT, right: RIGHT, commandHistory: [1, 2, 3] });
    expect(restored).not.toBeNull();
    expect(restored!.commandHistory).toEqual([]);
    expect(restored!.left.tabs).toHaveLength(1);
  });

  it("falls back to the left pane for a focus value it does not recognise", () => {
    expect(readSession({ left: LEFT, right: RIGHT, focused: "middle" })!.focused).toBe("left");
    expect(readSession({ left: LEFT, right: RIGHT })!.focused).toBe("left");
  });
});

describe("highestTabNumber", () => {
  it("finds the largest id in use, so a new tab cannot collide with a restored one", () => {
    // The counter that mints ids starts at 1 on every launch, and a duplicate
    // React key is a rendering bug that looks like a state bug.
    const session = toSession(LEFT, RIGHT, "left", []);
    expect(highestTabNumber(session)).toBe(7);
  });

  it("ignores an id that is not a number", () => {
    const odd = singleTabSet({ ...tab("tab-1"), id: "hand-edited" });
    expect(highestTabNumber(toSession(odd, odd, "left", []))).toBe(0);
  });
});
