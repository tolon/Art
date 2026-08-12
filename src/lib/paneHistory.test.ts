import { describe, expect, it } from "vitest";

import {
  emptyHistory,
  goBack,
  goForward,
  leaveToHost,
  pushLocation,
  sameLocation,
  type PaneLocation,
} from "@/lib/paneHistory";

const folder = (path: string): PaneLocation => ({ kind: "local", path });

const insideAdf = (dirBlock: number | null): PaneLocation => ({
  kind: "adf",
  path: "D:\\amiga\\Lotus.adf",
  dirBlock,
  trail: [],
  host: { path: "D:\\amiga", name: "Lotus.adf" },
});

describe("sameLocation", () => {
  it("is true for the same folder and false for a different one", () => {
    expect(sameLocation(folder("D:\\a"), folder("D:\\a"))).toBe(true);
    expect(sameLocation(folder("D:\\a"), folder("D:\\b"))).toBe(false);
  });

  it("is false across pane kinds even at the same path", () => {
    // The same file can be a floppy pane or, one day, something else; the
    // pane kind is part of where you are.
    expect(sameLocation(folder("D:\\x.adf"), insideAdf(null))).toBe(false);
  });

  it("distinguishes two directories inside the same image", () => {
    expect(sameLocation(insideAdf(880), insideAdf(881))).toBe(false);
    expect(sameLocation(insideAdf(880), insideAdf(880))).toBe(true);
  });

  it("distinguishes the partition list from a partition, and two partitions", () => {
    const at = (volumeIndex: number | null, dirBlock: number | null): PaneLocation => ({
      kind: "hdf",
      path: "D:\\work.hdf",
      volumeIndex,
      dirBlock,
      trail: [],
      host: null,
    });
    // `volumeIndex: null` is the partition list — a level a pane genuinely
    // sits at, since an HDF opens on it.
    expect(sameLocation(at(null, null), at(0, 880))).toBe(false);
    expect(sameLocation(at(0, 880), at(1, 880))).toBe(false);
    expect(sameLocation(at(0, 880), at(0, 880))).toBe(true);
  });

  it("distinguishes two folders inside one archive, and two disc directories", () => {
    const inArchive = (dir: string): PaneLocation => ({
      kind: "archive",
      path: "D:\\game.lha",
      dir,
      host: null,
    });
    expect(sameLocation(inArchive(""), inArchive("Tools"))).toBe(false);
    expect(sameLocation(inArchive("Tools"), inArchive("Tools"))).toBe(true);

    const onDisc = (extent: number): PaneLocation => ({
      kind: "iso",
      path: "D:\\cd.iso",
      extent,
      length: 2048,
      trail: [],
      host: null,
    });
    expect(sameLocation(onDisc(23), onDisc(24))).toBe(false);
  });

  it("does not depend on the order the object's keys were built in", () => {
    // A `JSON.stringify` comparison would call these different, and the only
    // job of this function is to stop the same place being pushed twice — so
    // a false "different" is a history full of duplicates and a Back key that
    // appears to do nothing.
    const a = { kind: "local", path: "D:\\a" } as PaneLocation;
    const b = JSON.parse(JSON.stringify({ path: "D:\\a", kind: "local" })) as PaneLocation;
    expect(sameLocation(a, b)).toBe(true);
  });
});

describe("pushLocation", () => {
  it("records each new place and lands on it", () => {
    let history = pushLocation(emptyHistory(), folder("D:\\a"));
    history = pushLocation(history, folder("D:\\b"));
    expect(history.entries).toHaveLength(2);
    expect(history.index).toBe(1);
  });

  it("ignores a move to where the pane already is", () => {
    // This is what keeps a refresh (F2) out of the history.
    const first = pushLocation(emptyHistory(), folder("D:\\a"));
    const again = pushLocation(first, folder("D:\\a"));
    expect(again).toBe(first);
  });

  it("records entering a container as its own step", () => {
    // The whole point of the container work: an image is a place, so going
    // back means coming back *inside* it, at the same directory.
    let history = pushLocation(emptyHistory(), folder("D:\\amiga"));
    history = pushLocation(history, insideAdf(null));
    history = pushLocation(history, insideAdf(881));
    expect(history.entries.map((e) => e.kind)).toEqual(["local", "adf", "adf"]);
    expect(history.index).toBe(2);
  });

  it("discards the forward entries when the user branches after going back", () => {
    // Browser semantics, because they are the ones everybody already has:
    // keeping them would offer a Forward to a place the user has left behind.
    let history = pushLocation(emptyHistory(), folder("D:\\a"));
    history = pushLocation(history, folder("D:\\b"));
    history = pushLocation(history, folder("D:\\c"));
    history = goBack(history)!.history; // now at D:\b, with D:\c ahead
    history = pushLocation(history, folder("D:\\d"));

    expect(history.entries.map((e) => (e as { path: string }).path)).toEqual([
      "D:\\a",
      "D:\\b",
      "D:\\d",
    ]);
    expect(history.index).toBe(2);
    expect(goForward(history)).toBeNull();
  });
});

describe("goBack and goForward", () => {
  it("do nothing on an empty history", () => {
    expect(goBack(emptyHistory())).toBeNull();
    expect(goForward(emptyHistory())).toBeNull();
  });

  it("do nothing at the ends", () => {
    const one = pushLocation(emptyHistory(), folder("D:\\a"));
    expect(goBack(one)).toBeNull();
    expect(goForward(one)).toBeNull();
  });

  it("walk back and forward over the same entries", () => {
    let history = pushLocation(emptyHistory(), folder("D:\\a"));
    history = pushLocation(history, folder("D:\\b"));
    history = pushLocation(history, folder("D:\\c"));

    const back1 = goBack(history)!;
    expect((back1.to as { path: string }).path).toBe("D:\\b");
    const back2 = goBack(back1.history)!;
    expect((back2.to as { path: string }).path).toBe("D:\\a");
    expect(goBack(back2.history)).toBeNull();

    const fwd = goForward(back2.history)!;
    expect((fwd.to as { path: string }).path).toBe("D:\\b");
  });
});

describe("leaveToHost", () => {
  it("returns to the host folder with the cursor on the container", () => {
    expect(leaveToHost({ path: "E:\\amiga\\Games", name: "Lotus.adf" })).toEqual({
      path: "E:\\amiga\\Games",
      cursor: "Lotus.adf",
    });
  });

  it("has nowhere to go for an image opened straight from the source combo", () => {
    // Not an error: the pane is where it started, and `[..]` is correctly
    // absent there.
    expect(leaveToHost(null)).toBeNull();
  });
});
