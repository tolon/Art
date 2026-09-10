// @vitest-environment jsdom
//
// `HostPlacementPreview` — what a host-placed set would replace, and why it
// was refused (round 5, task 4).
//
// **Why this file exists.** `collision-row` has been on screen since Task 7
// and **no test has ever asserted it** (inventory round 5, § 6): it moved
// from `PackagePanel` into this shared component in round 3, and the two
// screens that mount it — the install panel and the chain rows — assert the
// things *they* decide (which packages are in the set, whether the set may be
// previewed at all, what the Run button may say), never the block itself. So
// the one piece of JSX that tells a user their existing files are about to be
// written over was covered by nothing. This is the presentational half, and
// it is asserted here, on its own.
//
// **The component is rendered directly, with a `HostPlacement` literal.**
// `useHostPlacement` is the state machine and it has its own coverage through
// both mounting screens; what these two cases are about is the render, and
// feeding it a hand-built placement is what makes "one row per report entry"
// assertable at all — through the hook the count would be whatever the
// `osinstall_collisions` mock returned, which is the same fixture one layer
// further away. The fixtures are shaped after `AmigaInstallPanel.test.tsx`'s
// own collisions mocks: `CollisionReport[]` for the preview, and
// `AddPackageResult`'s `refusals` for the refusal.
//
// **Two questions, and they are different ones.** A preview that dropped the
// grouping would still show every row (so a count alone proves nothing — the
// group headings are asserted with it), and a refusal is **not** a preview
// with an error in it: nothing was written, and the block that says so is its
// own (`host-placement-refusals`, Task 7's F2).

import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import i18n from "i18next";

// Side-effecting: a real, synchronously-initialised i18next instance.
import "@/i18n";
import { HostPlacementPreview, type HostPlacement } from "@/components/osbuilder/HostPlacement";
import type { CollisionReport, RefusalReason } from "@/lib/osinstall";
// The same formatter the component itself uses: a literal "320 B" here would
// be a copy of `formatBytes`, and copies drift.
import { formatBytes } from "@/lib/panel";

afterEach(() => {
  cleanup();
});

/** A placement holding nothing, with the two fields each case fills in. */
function placement(over: Partial<HostPlacement>): HostPlacement {
  return {
    collisions: null,
    collisionsError: null,
    refusals: null,
    confirmed: false,
    setConfirmed: () => {},
    busy: false,
    progress: null,
    applyError: null,
    outcome: null,
    run: async () => {},
    ...over,
  };
}

/**
 * Six entries over all four classes, **deliberately not in group order** —
 * an unversioned row first, a downgrade last. `groupCollisionsForPreview`
 * puts downgrades first; a component that rendered `placement.collisions` in
 * the order it arrived would still draw six rows.
 */
const REPORTS: CollisionReport[] = [
  {
    path: "Devs/DOSDrivers/PIPE",
    collision: { kind: "unversioned", fromBytes: 320, toBytes: 344 },
    declared: false,
  },
  {
    path: "C/Version",
    collision: { kind: "upgrade", from: "40.1", to: "45.1" },
    declared: true,
  },
  {
    path: "L/FastFileSystem",
    collision: { kind: "same-version", version: "45.9", fromBytes: 24_000, toBytes: 24_064 },
    declared: true,
  },
  {
    path: "C/Assign",
    collision: { kind: "upgrade", from: "40.2", to: "45.2" },
    declared: true,
  },
  {
    path: "Libs/workbench.library",
    collision: { kind: "unversioned", fromBytes: 100, toBytes: 120 },
    declared: true,
  },
  {
    path: "Prefs/Font",
    collision: { kind: "downgrade", from: "45.3", to: "40.3" },
    declared: true,
  },
];

describe("what the placement would replace", () => {
  it("draws one row per report entry, grouped by class with downgrades first", () => {
    render(<HostPlacementPreview placement={placement({ collisions: REPORTS })} />);

    const block = screen.getByTestId("host-placement-collisions");
    const rows = within(block).getAllByTestId("collision-row");
    // One per entry — no folding, no truncation. Six in, six out.
    expect(rows).toHaveLength(REPORTS.length);

    // Every path, and each beside its own class's sentence: a block that
    // rendered six rows and put one path under the wrong heading would pass
    // a count.
    expect(rows.map((row) => row.textContent)).toEqual([
      `Prefs/Font${i18n.t("osinstall.packages.collision.downgrade", { from: "45.3", to: "40.3" })}`,
      `C/Version${i18n.t("osinstall.packages.collision.upgrade", { from: "40.1", to: "45.1" })}`,
      `C/Assign${i18n.t("osinstall.packages.collision.upgrade", { from: "40.2", to: "45.2" })}`,
      `L/FastFileSystem${i18n.t("osinstall.packages.collision.sameVersion", {
        version: "45.9",
      })}`,
      `Devs/DOSDrivers/PIPE${i18n.t("osinstall.packages.collision.unversioned", {
        fromBytes: formatBytes(320),
        toBytes: formatBytes(344),
      })}` + i18n.t("osinstall.packages.undeclared"),
      `Libs/workbench.library${i18n.t("osinstall.packages.collision.unversioned", {
        fromBytes: formatBytes(100),
        toBytes: formatBytes(120),
      })}`,
    ]);

    // **The grouping is marked by more than the order.** Each class has its
    // own heading with its own count, and the downgrade heading says
    // "Downgrades", not just red (F1) — colour is never the only signal.
    expect(block.textContent).toContain(
      i18n.t("osinstall.packages.preview.group.downgrade", { count: 1 })
    );
    expect(block.textContent).toContain(
      i18n.t("osinstall.packages.preview.group.upgrade", { count: 2 })
    );
    expect(block.textContent).toContain(
      i18n.t("osinstall.packages.preview.group.sameVersion", { count: 1 })
    );
    expect(block.textContent).toContain(
      i18n.t("osinstall.packages.preview.group.unversioned", { count: 2 })
    );

    // A refusal is a different state; nothing was refused here.
    expect(screen.queryByTestId("host-placement-refusals")).toBeNull();
  });

  it("says nothing would be replaced rather than drawing an empty list", () => {
    // The control, measured rather than assumed. `[]` is a **real** preview —
    // "ART looked and nothing on the tree would be replaced" — and is not the
    // same answer as `null`, which is "nothing has been asked yet". A block
    // that drew rows unconditionally would pass the case above; one that
    // treated `[]` as `null` would leave a confirmed user with no evidence
    // that the question had been put at all.
    render(<HostPlacementPreview placement={placement({ collisions: [] })} />);

    const block = screen.getByTestId("host-placement-collisions");
    expect(within(block).queryAllByTestId("collision-row")).toHaveLength(0);
    expect(block.textContent).toBe(i18n.t("osinstall.packages.preview.nothing"));
  });
});

/**
 * **A refusal is not a preview that failed** (Task 7's F2, and CLAUDE.md's
 * "endings stay distinct"). `osinstall_add_package` answers `refused` with
 * typed reasons **before a byte moves**; the block that renders them is its
 * own, it never appears over a job that went red later, and each reason is
 * rendered through `refusalPhrase` so it names what is missing and, where
 * order matters, the order.
 */
describe("a refusal, before anything was written", () => {
  const REFUSALS: RefusalReason[] = [
    { refusal: "package-folder-missing", packages: ["boingbag-39-1"] },
    {
      refusal: "package-requirement-missing",
      package: "BoingBag 3.9-2",
      requires: "BoingBag 3.9-1",
    },
  ];

  it("lists every reason, under its own heading", () => {
    render(<HostPlacementPreview placement={placement({ refusals: REFUSALS })} />);

    const box = screen.getByTestId("host-placement-refusals");
    expect(box.textContent).toContain(i18n.t("osinstall.packages.refused.heading"));
    // The prerequisite refusal names which package goes first, and in what
    // order — the half a translated "it was refused" would lose.
    expect(box.textContent).toContain(
      i18n.t("osinstall.refusal.packageRequirementMissing", {
        package: "BoingBag 3.9-2",
        requires: "BoingBag 3.9-1",
      })
    );
    expect(box.textContent).toContain(
      i18n.t("osinstall.refusal.packageFolderMissing", { packages: "boingbag-39-1" })
    );
    expect(within(box).getAllByRole("listitem")).toHaveLength(REFUSALS.length);

    // And it is not drawn as a preview: a refusal happened before the tree
    // was touched, so there is no collision list to stand over.
    expect(screen.queryByTestId("host-placement-collisions")).toBeNull();
  });

  it("draws no refusal box for an empty refusal list", () => {
    // The control: `[]` is what the hook holds after a run that was *not*
    // refused, and a heading saying "ART would not do this" over no reasons
    // is the screen refusing something the core allowed.
    render(<HostPlacementPreview placement={placement({ refusals: [], collisions: REPORTS })} />);
    expect(screen.queryByTestId("host-placement-refusals")).toBeNull();
  });
});
