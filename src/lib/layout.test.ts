import { describe, expect, it } from "vitest";

import { droppedTotal, layoutBlocker, retarget, type LayoutPlan } from "@/lib/layout";

const PLAN: LayoutPlan = {
  root: "E:\\staging",
  items: [
    {
      source: "E:\\a\\Turrican.lha",
      kind: { kind: "whdload-archive", name: "Turrican" },
      destination: "Games/Turrican",
      placement: "unpack-whdload",
      bytes: 100,
    },
    {
      source: "E:\\a\\Mega.lha",
      kind: { kind: "archive" },
      destination: "Unsorted/Mega.lha",
      placement: "copy-file",
      bytes: 50,
    },
  ],
  refused: [],
  collisions: [],
  tooDeep: { paths: [], more: 0 },
  duplicates: { paths: [], more: 0 },
  alreadyInPlace: [],
  bytes: 150,
};

describe("retarget", () => {
  it("moves the chosen rows to another drawer, keeping each leaf name", () => {
    const next = retarget(PLAN, [1], "Demos");
    expect(next.items[1].destination).toBe("Demos/Mega.lha");
    expect(next.items[0].destination).toBe("Games/Turrican");
  });

  it("recomputes the collisions, because a retarget can make one", () => {
    const plan: LayoutPlan = {
      ...PLAN,
      items: [
        { ...PLAN.items[0], destination: "Games/Same" },
        { ...PLAN.items[1], destination: "Unsorted/Same" },
      ],
    };
    const next = retarget(plan, [1], "Games");
    expect(next.collisions.map((c) => c.destination)).toEqual(["Games/Same"]);
  });

  it("leaves the plan alone when no row was chosen", () => {
    expect(retarget(PLAN, [], "Demos")).toEqual(PLAN);
  });

  it("drops a single-source on-disk collision, which is why the screen must re-ask", () => {
    // A single-source collision (one row, one source) only ever comes from
    // the engine having found that destination already on disk — `retarget`
    // itself never produces one, since a collision it computes always has
    // two or more sources. Retargeting a *different*, unrelated row must not
    // silently make this collision vanish from the plan the screen trusts;
    // it is not resolved, ART simply has not asked the engine again yet.
    const plan: LayoutPlan = {
      ...PLAN,
      collisions: [{ destination: "Unsorted/Mega.lha", sources: ["E:\\a\\Mega.lha"] }],
    };

    const next = retarget(plan, [0], "Floppies");

    expect(next.collisions).toEqual([]);
  });
});

describe("layoutBlocker", () => {
  const ready = { root: "E:\\staging", paths: ["E:\\a"], plan: PLAN };

  it("is clear when a root, some paths and a plan are in hand", () => {
    expect(layoutBlocker(ready)).toBeNull();
  });

  it("asks for the staging folder first", () => {
    expect(layoutBlocker({ ...ready, root: null })?.key).toBe("layout.blocked.noRoot");
  });

  it("asks for something to lay out", () => {
    expect(layoutBlocker({ ...ready, paths: [] })?.key).toBe("layout.blocked.nothingToPlace");
  });

  it("asks for a preview before writing anything", () => {
    expect(layoutBlocker({ ...ready, plan: null })?.key).toBe("layout.blocked.notPlanned");
  });

  it("will not apply a plan with a collision in it", () => {
    const clashing: LayoutPlan = {
      ...PLAN,
      collisions: [{ destination: "Games/Turrican", sources: ["a", "b"] }],
    };
    expect(layoutBlocker({ ...ready, plan: clashing })?.key).toBe("layout.blocked.collisions");
  });

  it("will not apply a plan that would place nothing", () => {
    expect(layoutBlocker({ ...ready, plan: { ...PLAN, items: [] } })?.key).toBe(
      "layout.blocked.nothingToPlace"
    );
  });
});

/**
 * ART-107. `Dropped.paths` is capped at twenty on the Rust side, so anything
 * that prints `paths.length` as "how many were dropped" understates it the
 * moment there are twenty-one. `droppedTotal` is the only number a sentence
 * may use.
 */
describe("droppedTotal", () => {
  it("counts what was named plus what was not", () => {
    expect(droppedTotal({ paths: [], more: 0 })).toBe(0);
    expect(droppedTotal({ paths: ["a", "b"], more: 0 })).toBe(2);
    expect(droppedTotal({ paths: ["a", "b"], more: 7 })).toBe(9);
  });

  it("is not the length of the named list", () => {
    const capped = { paths: Array.from({ length: 20 }, (_, i) => `d${i}`), more: 41 };
    expect(droppedTotal(capped)).toBe(61);
    expect(droppedTotal(capped)).not.toBe(capped.paths.length);
  });
});

/**
 * Neither ART-107 report blocks apply. A plan that is short in one corner, or
 * that dropped a source the user named twice, is still worth applying — the
 * defect was that it happened in silence, not that it happened.
 */
describe("layoutBlocker and the ART-107 reports", () => {
  it("does not block on a folder that was too deep to look inside", () => {
    const plan: LayoutPlan = { ...PLAN, tooDeep: { paths: ["E:/deep"], more: 0 } };
    expect(layoutBlocker({ root: "E:\staging", paths: ["E:\a"], plan })).toBeNull();
  });

  it("does not block on a source that was added twice", () => {
    const plan: LayoutPlan = { ...PLAN, duplicates: { paths: ["E:/Games/x.lha"], more: 0 } };
    expect(layoutBlocker({ root: "E:\staging", paths: ["E:\a"], plan })).toBeNull();
  });
});
