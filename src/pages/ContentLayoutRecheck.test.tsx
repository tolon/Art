// @vitest-environment jsdom
//
// `ContentLayout.tsx` calls Tauri (dialog, `invoke`, event `listen`) at mount
// through `useRemembered` and `onLayoutResult`, so — following
// `FileManagerFocus.test.tsx` — it cannot be rendered directly in a test yet.
// This is a harness wired the way `moveChecked` and `blocker` actually are,
// with `layoutRecheck` swapped for an injectable async function so the
// failure path can be forced.
//
// What this pins: a failed recheck must not leave `plan.collisions` reading
// as an all-clear. Before the fix, `moveChecked`'s `catch` only set `error`;
// `plan.collisions` still held `retarget`'s in-plan-only list, `layoutBlocker`
// saw no collisions in it, and Apply was reachable while ART genuinely did
// not know whether the new destinations were free — the false all-clear §89
// forbids. It also pins the second residual: two overlapping rechecks must
// not let a stale response overwrite a plan retargeted again in the meantime.

import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";

import { layoutBlocker, retarget, type Collision, type LayoutPlan } from "@/lib/layout";

afterEach(cleanup);

const PLAN: LayoutPlan = {
  root: "E:\\staging",
  items: [
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
  bytes: 50,
};

/** The slice of `ContentLayout` this is about: `moveChecked` and `blocker`. */
function Harness({
  recheck,
}: {
  recheck: (plan: LayoutPlan) => Promise<{ collisions: Collision[]; already_in_place: string[] }>;
}) {
  const [plan, setPlan] = useState<LayoutPlan>(PLAN);
  const [collisionsUnknown, setCollisionsUnknown] = useState(false);
  const [busy, setBusy] = useState(false);

  async function moveChecked(drawer: string) {
    const retargeted = retarget(plan, [0], drawer);
    setPlan(retargeted);
    setBusy(true);
    try {
      const rechecked = await recheck(retargeted);
      setPlan((current) =>
        current === retargeted
          ? {
              ...current,
              collisions: rechecked.collisions,
              alreadyInPlace: rechecked.already_in_place,
            }
          : current
      );
      setCollisionsUnknown(false);
    } catch {
      setCollisionsUnknown(true);
    } finally {
      setBusy(false);
    }
  }

  const blocker =
    layoutBlocker({ root: "E:\\staging", paths: ["E:\\a"], plan }) ??
    (collisionsUnknown ? { key: "layout.blocked.couldNotRecheck" } : null);

  return (
    <div>
      <p>collisions: {plan.collisions.length}</p>
      <p>blocker: {blocker ? blocker.key : "none"}</p>
      <button onClick={() => void moveChecked("Floppies")}>move to Floppies</button>
      <button onClick={() => void moveChecked("CDs")}>move to CDs</button>
      <button disabled={busy || blocker !== null}>apply</button>
    </div>
  );
}

describe("a failed layoutRecheck", () => {
  it("blocks Apply instead of leaving the last-known collision list as an all-clear", async () => {
    render(<Harness recheck={() => Promise.reject(new Error("offline"))} />);

    await userEvent.click(screen.getByText("move to Floppies"));

    await waitFor(() => {
      expect(screen.getByText("blocker: layout.blocked.couldNotRecheck")).toBeTruthy();
    });
    expect(screen.getByText("apply")).toHaveProperty("disabled", true);
    // The false all-clear this guards against: `retarget` itself found no
    // in-plan collision, so a version of this code that only looked at
    // `plan.collisions.length` would say zero and let Apply through.
    expect(screen.getByText("collisions: 0")).toBeTruthy();
  });

  it("clears once a later recheck succeeds", async () => {
    let fail = true;
    render(
      <Harness
        recheck={() =>
          fail
            ? Promise.reject(new Error("offline"))
            : Promise.resolve({ collisions: [], already_in_place: [] })
        }
      />
    );

    await userEvent.click(screen.getByText("move to Floppies"));
    await waitFor(() => {
      expect(screen.getByText("blocker: layout.blocked.couldNotRecheck")).toBeTruthy();
    });

    fail = false;
    await userEvent.click(screen.getByText("move to CDs"));
    await waitFor(() => {
      expect(screen.getByText("blocker: none")).toBeTruthy();
    });
  });
});

describe("two overlapping rechecks", () => {
  it("does not let the first response land on a plan retargeted again meanwhile", async () => {
    type Answer = { collisions: Collision[]; already_in_place: string[] };
    let resolveFirst: ((answer: Answer) => void) | undefined;
    const recheck = vi
      .fn<
        (plan: LayoutPlan) => Promise<{ collisions: Collision[]; already_in_place: string[] }>
      >()
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveFirst = resolve;
          })
      )
      .mockImplementationOnce(() =>
        Promise.resolve({ collisions: [], already_in_place: [] })
      );

    render(<Harness recheck={recheck} />);

    // First retarget starts a recheck that never resolves on its own.
    await userEvent.click(screen.getByText("move to Floppies"));
    // A second retarget lands before the first recheck answers.
    await userEvent.click(screen.getByText("move to CDs"));

    expect(recheck).toHaveBeenCalledTimes(2);

    // The first recheck now answers, with a collision that would be wrong
    // to apply to the plan on screen — it describes the "Floppies" plan the
    // second retarget already replaced.
    resolveFirst?.({
      collisions: [{ destination: "Floppies/Mega.lha", sources: ["E:\a\Mega.lha"] }],
      already_in_place: [],
    });

    // Give the stale promise a turn to (not) take effect.
    await new Promise((r) => setTimeout(r, 0));

    expect(screen.getByText("collisions: 0")).toBeTruthy();
  });
});
