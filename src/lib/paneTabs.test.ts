import { describe, expect, it } from "vitest";

import {
  activeTab,
  closeTab,
  duplicateTab,
  isUsableTabSet,
  nextTab,
  selectTab,
  singleTabSet,
  tabTitle,
  updateActiveTab,
  TAB_LIMIT,
  type PaneTab,
  type TabSet,
} from "@/lib/paneTabs";
import type { PaneLocation } from "@/lib/paneHistory";
import { defaultSortState } from "@/lib/sort";

function tab(id: string, path = "D:\\amiga"): PaneTab {
  return {
    id,
    location: { kind: "local", path } as PaneLocation,
    sort: defaultSortState(),
    filter: "",
  };
}

function setOf(...ids: string[]): TabSet {
  return { tabs: ids.map((id) => tab(id)), active: 0 };
}

describe("duplicateTab (Ctrl+T)", () => {
  it("copies the active tab and lands on the copy, next to the original", () => {
    // Duplicating rather than opening a blank tab: a new tab is nearly always
    // wanted *near* where you are, to go two different ways from it.
    const set = duplicateTab(singleTabSet(tab("a", "D:\\games")), "b");
    expect(set.tabs.map((t) => t.id)).toEqual(["a", "b"]);
    expect(set.active).toBe(1);
    expect(activeTab(set).location).toEqual(activeTab(singleTabSet(tab("a", "D:\\games"))).location);
  });

  it("inserts after the active tab, not at the end", () => {
    const set = duplicateTab({ tabs: [tab("a"), tab("b"), tab("c")], active: 1 }, "new");
    expect(set.tabs.map((t) => t.id)).toEqual(["a", "b", "new", "c"]);
    expect(set.active).toBe(2);
  });

  it("refuses quietly at the limit", () => {
    // Thirty-two is not a number anyone reaches by accident, and a dialog
    // about it would be noise.
    const full: TabSet = {
      tabs: Array.from({ length: TAB_LIMIT }, (_, i) => tab(`t${i}`)),
      active: 0,
    };
    expect(duplicateTab(full, "one-more")).toBe(full);
  });
});

describe("closeTab (Ctrl+W, middle-click)", () => {
  it("never closes the last tab", () => {
    // A pane with no tabs is not a state this model has, and giving it one
    // would mean a null check in every reader of `activeTab`.
    const one = singleTabSet(tab("a"));
    expect(closeTab(one, 0)).toBe(one);
  });

  it("lands on the left-hand neighbour when the active tab goes", () => {
    const set = closeTab({ tabs: [tab("a"), tab("b"), tab("c")], active: 1 }, 1);
    expect(set.tabs.map((t) => t.id)).toEqual(["a", "c"]);
    expect(set.active).toBe(0);
  });

  it("stays on the first tab when the first one goes", () => {
    const set = closeTab({ tabs: [tab("a"), tab("b")], active: 0 }, 0);
    expect(set.tabs.map((t) => t.id)).toEqual(["b"]);
    expect(set.active).toBe(0);
  });

  it("keeps pointing at the same tab when an earlier one goes", () => {
    const set = closeTab({ tabs: [tab("a"), tab("b"), tab("c")], active: 2 }, 0);
    expect(activeTab(set).id).toBe("c");
  });

  it("keeps pointing at the same tab when a later one goes", () => {
    const set = closeTab({ tabs: [tab("a"), tab("b"), tab("c")], active: 0 }, 2);
    expect(activeTab(set).id).toBe("a");
  });

  it("ignores an index that is not there", () => {
    const set = setOf("a", "b");
    expect(closeTab(set, 5)).toBe(set);
    expect(closeTab(set, -1)).toBe(set);
  });
});

describe("nextTab and selectTab", () => {
  it("cycles, wrapping", () => {
    let set: TabSet = setOf("a", "b", "c");
    set = nextTab(set);
    expect(set.active).toBe(1);
    set = nextTab(nextTab(set));
    expect(set.active).toBe(0);
  });

  it("does nothing with a single tab", () => {
    const one = singleTabSet(tab("a"));
    expect(nextTab(one)).toBe(one);
  });

  it("ignores an out-of-range selection rather than clamping it", () => {
    const set = setOf("a", "b");
    expect(selectTab(set, 9)).toBe(set);
    expect(selectTab(set, 0)).toBe(set);
    expect(selectTab(set, 1).active).toBe(1);
  });
});

describe("updateActiveTab", () => {
  it("writes the pane's state into the active tab", () => {
    const set = updateActiveTab({ tabs: [tab("a"), tab("b")], active: 1 }, { filter: "*.adf" });
    expect(set.tabs[1].filter).toBe("*.adf");
    expect(set.tabs[0].filter).toBe("");
  });

  it("returns the same object when nothing changed, so an effect cannot loop", () => {
    const set = setOf("a");
    expect(updateActiveTab(set, { filter: "" })).toBe(set);
  });
});

describe("tabTitle", () => {
  it("names the folder, not the path", () => {
    // A tab bar is a row of short labels or it is not a tab bar.
    expect(tabTitle(tab("a", "D:\\amiga\\Games"))).toBe("Games");
    expect(tabTitle(tab("a", "/media/sd/games"))).toBe("games");
  });

  it("names the image for a tab living inside a container", () => {
    // Which is what the tab *is* to the user, whatever directory inside the
    // image it happens to be sitting in.
    const inside: PaneTab = {
      id: "a",
      location: {
        kind: "adf",
        path: "D:\\amiga\\Lotus.adf",
        dirBlock: 881,
        trail: [],
        host: { path: "D:\\amiga", name: "Lotus.adf" },
      },
      sort: defaultSortState(),
      filter: "",
    };
    expect(tabTitle(inside)).toBe("Lotus.adf");
  });

  it("falls back to the whole path when there is no segment to take", () => {
    expect(tabTitle(tab("a", "D:\\"))).toBe("D:");
    expect(tabTitle(tab("a", ""))).toBe("");
  });
});

describe("isUsableTabSet", () => {
  it("accepts what this module writes", () => {
    expect(isUsableTabSet(setOf("a", "b"))).toBe(true);
  });

  it("rejects anything that would leave activeTab undefined", () => {
    // The settings store is a file a user can edit and an older ART may have
    // written. A commander that opens blank because of it is worse than one
    // that forgot your tabs.
    expect(isUsableTabSet(null)).toBe(false);
    expect(isUsableTabSet({ tabs: [], active: 0 })).toBe(false);
    expect(isUsableTabSet({ tabs: [tab("a")], active: 1 })).toBe(false);
    expect(isUsableTabSet({ tabs: [tab("a")], active: -1 })).toBe(false);
    expect(isUsableTabSet({ tabs: [tab("a")] })).toBe(false);
    expect(isUsableTabSet({ tabs: [{ id: "a" }], active: 0 })).toBe(false);
    expect(isUsableTabSet({ tabs: [{ ...tab("a"), location: null }], active: 0 })).toBe(false);
  });
});
