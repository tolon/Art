import { describe, expect, it } from "vitest";

import { collidingNames, planMove, type MoveEntry, type MoveInput } from "@/lib/movePlan";

function dir(name: string): MoveEntry {
  return { name, isDir: true };
}
function file(name: string): MoveEntry {
  return { name, isDir: false };
}

/** A move that is allowed: an ADF on the left, a host folder on the right,
 *  one entry marked, nothing in the way. Each test below changes exactly the
 *  one thing it is about. */
function base(): MoveInput {
  return {
    sourceKind: "adf",
    targetKind: "local",
    sourceWritable: true,
    targetWritable: true,
    entries: [dir("Lotus")],
    takenNames: [],
    sameImage: false,
    sourceIsRoot: false,
  };
}

describe("planMove", () => {
  it("allows a volume→folder move and hands back the entries it will move", () => {
    expect(planMove(base())).toEqual({ kind: "move", entries: [dir("Lotus")] });
  });

  it("allows a single folder between two images", () => {
    expect(planMove({ ...base(), targetKind: "hdf" })).toEqual({
      kind: "move",
      entries: [dir("Lotus")],
    });
  });

  it("refuses an empty selection", () => {
    expect(planMove({ ...base(), entries: [] })).toEqual({
      kind: "refused",
      reason: { key: "files.move.refuseNothing" },
    });
  });

  it("allows a move out of a host folder — ART-080", () => {
    // The refusal that stood here was for want of a decision, not of work:
    // where a deleted host file goes. The owner's ruling is the **Windows
    // Recycle Bin** — ART invents no recovery mechanism of its own and uses
    // the one the operating system already has.
    expect(planMove({ ...base(), sourceKind: "local", targetKind: "adf" })).toEqual({
      kind: "move",
      entries: [dir("Lotus")],
    });
  });

  it("does not need a writable *volume* when the source is a host folder", () => {
    // `writableVolume` is null for every local pane by definition, the same
    // way it already is for a local *target*. A host folder's delete goes to
    // the Recycle Bin, not through the volume writer.
    expect(
      planMove({ ...base(), sourceKind: "local", targetKind: "adf", sourceWritable: false })
    ).toEqual({ kind: "move", entries: [dir("Lotus")] });
  });

  it("refuses a host folder to a host folder — review F8", () => {
    // This used to be *planned as allowed* and then refused by the page after
    // **both** confirmations: the user answered "yes, move these" and "yes,
    // overwrite the protected one" and only then learnt ART would not. A
    // refusal a plan can reach has to be reached in the plan.
    expect(
      planMove({ ...base(), sourceKind: "local", targetKind: "local" })
    ).toEqual({ kind: "refused", reason: { key: "files.move.refuseHostToHost" } });
  });

  it("still refuses to move out of a drive root", () => {
    // `C:\` is where `Windows` and `Program Files` live, and the two
    // confirmations a user learns to click through for a game are the same
    // two here. The one case a host move is still refused in.
    expect(
      planMove({ ...base(), sourceKind: "local", targetKind: "adf", sourceIsRoot: true })
    ).toEqual({ kind: "refused", reason: { key: "files.move.refuseLocalRoot" } });
  });

  it("a drive root is only about a host source, never a volume one", () => {
    // A volume pane has no `parent === null` state that means "root" in this
    // sense — its root directory is an ordinary place to move things out of.
    expect(planMove({ ...base(), sourceIsRoot: true })).toEqual({
      kind: "move",
      entries: [dir("Lotus")],
    });
  });

  it("refuses to move out of a disc, an archive or a Commodore image", () => {
    for (const sourceKind of ["iso", "archive", "c64"] as const) {
      expect(planMove({ ...base(), sourceKind })).toEqual({
        kind: "refused",
        reason: { key: "files.move.refuseReadOnlySource" },
      });
    }
  });

  it("refuses when the source volume will not take a delete", () => {
    // A dircache or PFS3 volume, or one with an unfinished operation waiting:
    // the copy half would work and the delete half would not, which is a
    // "move" that silently duplicates.
    expect(planMove({ ...base(), sourceWritable: false })).toEqual({
      kind: "refused",
      reason: { key: "files.move.refuseSourceNotWritable" },
    });
  });

  it("refuses a read-only destination, and an image with nothing open in it", () => {
    for (const targetKind of ["iso", "archive", "c64"] as const) {
      expect(planMove({ ...base(), targetKind })).toEqual({
        kind: "refused",
        reason: { key: "files.move.refuseReadOnlyTarget" },
      });
    }
    expect(planMove({ ...base(), targetKind: "hdf", targetWritable: false })).toEqual({
      kind: "refused",
      reason: { key: "files.move.refuseTargetNotWritable" },
    });
  });

  it("does not need a writable *target* when the target is a host folder", () => {
    // `writableVolume` is null for every local pane by definition; a move out
    // to a folder goes through the extract path, not the volume writer.
    expect(planMove({ ...base(), targetWritable: false })).toEqual({
      kind: "move",
      entries: [dir("Lotus")],
    });
  });

  it("allows several entries between two images — ART-081", () => {
    // Both halves have the same shape and the same guarantee now:
    // `volumeCopyBetweenMany` stages exactly what was marked into one
    // operation (ART-176), and `volumeDeleteMany` removes exactly those names
    // as one journalled operation that rolls back whole on either write
    // strategy (ART-073). The refusal that stood here was for want of the
    // second, not of the first.
    expect(
      planMove({ ...base(), targetKind: "adf", entries: [dir("Lotus"), dir("Turrican")] })
    ).toEqual({ kind: "move", entries: [dir("Lotus"), dir("Turrican")] });
  });

  it("allows a lone file between two images — ART-081, and by the same route", () => {
    // The whole of the owner's ruling on this area: a single entry is a
    // one-entry batch through the route the batch uses, never a second path
    // with its own promises. A file and a folder are the same case here.
    expect(planMove({ ...base(), targetKind: "adf", entries: [file("Readme")] })).toEqual({
      kind: "move",
      entries: [file("Readme")],
    });
  });

  it("refuses a move between two directories of the *same* image", () => {
    // ART-081's own new hazard, and the reason it needed a guard rather than
    // just a lifted restriction: a move within one image is a relink, not a
    // copy-and-delete. Doing it the long way stages the tree out, writes it
    // back into the same volume and then removes the original — twice the
    // free space, and a failure between the halves losing the only copy. F5
    // has always refused this; F6 could reach it and did not, which only
    // stopped mattering because F6 was restricted to one directory until now.
    expect(planMove({ ...base(), targetKind: "adf", sameImage: true })).toEqual({
      kind: "refused",
      reason: { key: "files.err.sameImage" },
    });
    // ...and out to a host folder it is not the same case at all, so it must
    // still be allowed.
    expect(planMove({ ...base(), targetKind: "local", sameImage: true })).toEqual({
      kind: "move",
      entries: [dir("Lotus")],
    });
  });

  it("allows several entries, files included, out to a host folder", () => {
    expect(
      planMove({ ...base(), entries: [dir("Lotus"), file("Readme")] })
    ).toEqual({ kind: "move", entries: [dir("Lotus"), file("Readme")] });
  });

  it("refuses when a name is already taken, rather than asking about overwriting", () => {
    // The whole point: "leave it alone" would skip the copy and then delete
    // the source, and "replace it" would destroy the destination's copy. A
    // move is not the place to be offered either.
    const plan = planMove({ ...base(), takenNames: ["Lotus", "Readme"] });
    expect(plan.kind).toBe("refused");
    expect(plan.kind === "refused" && plan.reason.key).toBe("files.move.refuseCollision");
    expect(plan.kind === "refused" && plan.reason.params).toEqual({
      names: "Lotus",
      count: 1,
    });
  });

  it("catches a collision that differs only in case — AmigaDOS does not care either", () => {
    const plan = planMove({ ...base(), takenNames: ["lotus"] });
    expect(plan.kind).toBe("refused");
    expect(plan.kind === "refused" && plan.reason.key).toBe("files.move.refuseCollision");
  });

  it("names at most three collisions, and counts them all", () => {
    const plan = planMove({
      ...base(),
      entries: ["a", "b", "c", "d"].map(file),
      takenNames: ["a", "b", "c", "d"],
    });
    expect(plan.kind === "refused" && plan.reason.params).toEqual({
      names: "a, b, c",
      count: 4,
    });
  });
});

describe("collidingNames", () => {
  it("finds nothing in an empty destination", () => {
    expect(collidingNames(["a", "b"], [])).toEqual([]);
  });

  it("compares case-insensitively, in pane order", () => {
    expect(collidingNames(["Alpha", "Beta", "Gamma"], ["GAMMA", "alpha"])).toEqual([
      "Alpha",
      "Gamma",
    ]);
  });
});
