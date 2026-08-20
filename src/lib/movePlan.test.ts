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

  it("refuses to move out of a host folder — ART deletes nothing on your own disk", () => {
    // ART-080. Every delete ART owns goes into a disk image; there is no
    // command that removes a file from the user's own filesystem, and a UI
    // task is not where one gets invented.
    expect(planMove({ ...base(), sourceKind: "local", targetKind: "adf" })).toEqual({
      kind: "refused",
      reason: { key: "files.move.refuseLocalSource" },
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

  it("refuses several entries between two images — ART-064's gap, not a move bug", () => {
    expect(
      planMove({ ...base(), targetKind: "adf", entries: [dir("Lotus"), dir("Turrican")] })
    ).toEqual({ kind: "refused", reason: { key: "files.err.batchBetweenVolumes" } });
  });

  it("refuses a lone file between two images — ART-081, whose delete half is missing", () => {
    // The *copy* half works since ART-176: one route between two images, and
    // it stages exactly what was marked. What a move also needs is the
    // delete, sequenced after the destination verifies — recorded rather than
    // improvised, so this refusal stands.
    expect(planMove({ ...base(), targetKind: "adf", entries: [file("Readme")] })).toEqual({
      kind: "refused",
      reason: { key: "files.move.refuseFileBetweenImages" },
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
