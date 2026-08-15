import { describe, expect, it } from "vitest";

import { layoutBlocker, retarget, type LayoutPlan } from "@/lib/layout";

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
