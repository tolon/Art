import { describe, expect, it } from "vitest";

import {
  planDelete,
  planDeleteSelection,
  type DeleteEntry,
} from "@/lib/deletePlan";

function entry(name: string, deleteProtected = false): DeleteEntry {
  return { name, deleteProtected };
}

describe("planDelete", () => {
  it("removes just the entry when there is no icon", () => {
    expect(planDelete(entry("Turrican"), null, true)).toEqual({
      names: ["Turrican"],
      overrideProtection: false,
      withIcon: false,
    });
  });

  it("removes just the entry when the user said no to its icon", () => {
    // Saying no is a real answer, not an absence of one — the `.info` stays.
    expect(planDelete(entry("Turrican"), { name: "Turrican.info" }, false)).toEqual({
      names: ["Turrican"],
      overrideProtection: false,
      withIcon: false,
    });
  });

  it("puts the file and its icon in ONE batch — ART-081", () => {
    // The defect this exists for: the screen deleted the file, *then* asked
    // about the icon, then deleted the icon in a second committed operation.
    // A failure between the two left `Turrican` gone and `Turrican.info`
    // behind — an icon that opens nothing, which is the §7.1 clutter the
    // question exists to prevent. One list means one journalled commit
    // (ART-073), so both go or neither does.
    expect(planDelete(entry("Turrican"), { name: "Turrican.info" }, true)).toEqual({
      names: ["Turrican", "Turrican.info"],
      overrideProtection: false,
      withIcon: true,
    });
  });

  it("names the entry the user asked about first", () => {
    // Not decoration: the batch's own report and every message built from it
    // read `names[0]`, and a report leading with `Turrican.info` would
    // describe a delete the user did not ask for.
    const plan = planDelete(entry("Turrican"), { name: "Turrican.info" }, true);
    expect(plan.names[0]).toBe("Turrican");
  });

  it("takes the protection override from the entry, never from the icon", () => {
    // The override says "the user was shown the third question and said yes",
    // and that question named the *entry*. Inferring one from the icon would
    // let a protected file be removed on the strength of a question nobody
    // asked about it (ART-088).
    expect(
      planDelete(entry("Turrican", true), { name: "Turrican.info" }, true).overrideProtection
    ).toBe(true);
    expect(
      planDelete(entry("Turrican", false), { name: "Turrican.info" }, true).overrideProtection
    ).toBe(false);
  });
});

describe("planDeleteSelection", () => {
  it("is the same shape as the single case, with more names", () => {
    // The whole of the owner's ruling on this area: one route. A batch is not
    // a different function with different guarantees — it is this one with a
    // longer list.
    expect(planDeleteSelection([entry("Turrican"), entry("Lotus")])).toEqual({
      names: ["Turrican", "Lotus"],
      overrideProtection: false,
      withIcon: false,
    });
  });

  it("keeps the selection's own order", () => {
    const plan = planDeleteSelection([entry("C"), entry("A"), entry("B")]);
    expect(plan.names).toEqual(["C", "A", "B"]);
  });

  it("overrides protection when any one entry is protected", () => {
    // The screen asks one question naming every protected entry, and the
    // writer takes one answer. Asking per entry would be the "click through
    // it" prompt §63 exists to avoid.
    expect(
      planDeleteSelection([entry("Turrican"), entry("Lotus", true)]).overrideProtection
    ).toBe(true);
    expect(
      planDeleteSelection([entry("Turrican"), entry("Lotus")]).overrideProtection
    ).toBe(false);
  });

  it("a one-entry selection is exactly a one-entry batch", () => {
    // Not a special case anywhere, which is the point: `volumeDelete` was the
    // second route and it is gone.
    expect(planDeleteSelection([entry("Turrican")])).toEqual({
      names: ["Turrican"],
      overrideProtection: false,
      withIcon: false,
    });
  });
});
