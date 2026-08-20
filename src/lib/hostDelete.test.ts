// What ART says after deleting on the user's own disk (ART-080).
//
// The sentence matters as much as the act here. A delete the user cannot find
// is the same as one they cannot undo — so the destination is in every
// sentence that reports a removal, and the one case with nowhere to name is
// the one where nothing was removed.

import { describe, expect, it } from "vitest";

import {
  describeHostDelete,
  recycleTargetPhrase,
  type HostDeleteOutcome,
} from "@/lib/panel";
import en from "@/i18n/en.json";

function row(name: string, removed: boolean, problem: string | null = null) {
  return { name, removed, problem };
}

/** An outcome that reached every name it was asked for. */
function reached(
  rows: ReturnType<typeof row>[],
  target: HostDeleteOutcome["target"]
): HostDeleteOutcome {
  return { rows, target, asked: rows.length, cancelled: false };
}

/** Every leaf key in the catalogue, so a `Phrase` pointing at nothing fails
 *  here rather than rendering the raw key on screen. */
function isLeafKey(dotted: string): boolean {
  let node: unknown = en;
  for (const part of dotted.split(".")) {
    if (typeof node !== "object" || node === null) return false;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === "string";
}

describe("recycleTargetPhrase", () => {
  it("resolves to a real catalogue key", () => {
    // The `never` fallthrough makes a second target a compile error; this is
    // what catches the other half — a key nobody added.
    expect(isLeafKey(recycleTargetPhrase("windows-recycle-bin").key)).toBe(true);
  });
});

describe("describeHostDelete", () => {
  it("says how many went and WHERE, when all of them went", () => {
    const outcome = reached([row("a.adf", true), row("b.adf", true)], "windows-recycle-bin");
    const said = describeHostDelete(outcome);

    expect(said.key).toBe("files.hostDelete.sentTo");
    expect(said.params.count).toBe(2);
    // The destination is never absent from a sentence reporting a removal.
    expect(said.targetPhrase).not.toBeNull();
    expect(isLeafKey(said.targetPhrase!.key)).toBe(true);
    // And the catalogue string really interpolates it, rather than the
    // parameter being carried and never used.
    expect(en.files.hostDelete.sentTo).toContain("{{target}}");
  });

  it("names the ones that did NOT go, when only some did", () => {
    // "Eleven of twelve" is not something a user can act on; the twelfth's
    // name is. A host filesystem has no journal, so this case is not
    // exceptional — it is the one this whole outcome shape exists for.
    const outcome = reached(
      [row("a.adf", true), row("locked.adf", false, "the file is in use"), row("c.adf", true)],
      "windows-recycle-bin"
    );
    const said = describeHostDelete(outcome);

    expect(said.key).toBe("files.hostDelete.partial");
    expect(said.params.removed).toBe(2);
    expect(said.params.asked).toBe(3);
    expect(said.params.names).toBe("locked.adf");
    expect(said.targetPhrase).not.toBeNull();
    expect(en.files.hostDelete.partial).toContain("{{target}}");
  });

  it("names nowhere when nothing was removed", () => {
    // Reporting a destination for a delete that did not happen would be the
    // same class of invention §89 forbids everywhere else.
    const outcome = reached([row("locked.adf", false, "the file is in use")], null);
    const said = describeHostDelete(outcome);

    expect(said.key).toBe("files.hostDelete.noneRemoved");
    expect(said.targetPhrase).toBeNull();
    expect(en.files.hostDelete.noneRemoved).not.toContain("{{target}}");
  });

  it("caps the named failures at three, and still says how many there are", () => {
    // A message listing forty names is one nobody reads. The count is what
    // tells the user the list is a sample.
    const outcome = reached(["a", "b", "c", "d", "e"].map((n) => row(n, false, "no")), null);
    const said = describeHostDelete(outcome);
    expect(said.params.names).toBe("a, b, c");
    expect(said.params.count).toBe(5);
  });

  it("says it was STOPPED, not that it succeeded — review F1", () => {
    // A twelve-name request cancelled after three has three rows, all
    // successful. Counting rows alone reads "3 item(s) went to the Recycle
    // Bin" — true of the three, silent about the nine, and identical to what
    // a complete three-file delete would say. The number asked for and the
    // fact that it stopped are both part of what happened.
    const outcome: HostDeleteOutcome = {
      rows: [row("a", true), row("b", true), row("c", true)],
      target: "windows-recycle-bin",
      asked: 12,
      cancelled: true,
    };
    const said = describeHostDelete(outcome);

    expect(said.key).toBe("files.hostDelete.cancelled");
    expect(said.params.removed).toBe(3);
    expect(said.params.asked).toBe(12);
    expect(said.params.count).toBe(9);
    // And it still says where the three that did go went.
    expect(said.targetPhrase).not.toBeNull();
    expect(en.files.hostDelete.cancelled).toContain("{{target}}");
    // The sentence it must NOT be.
    expect(said.key).not.toBe("files.hostDelete.sentTo");
  });

  it("every sentence it can produce resolves to a real catalogue key", () => {
    const cases: HostDeleteOutcome[] = [
      reached([row("a", true)], "windows-recycle-bin"),
      reached([row("a", true), row("b", false, "no")], "windows-recycle-bin"),
      reached([row("a", false, "no")], null),
      reached([], null),
      // The stopped case, which is its own sentence (review F1).
      { rows: [row("a", true)], target: "windows-recycle-bin", asked: 12, cancelled: true },
    ];
    for (const outcome of cases) {
      const said = describeHostDelete(outcome);
      expect(isLeafKey(said.key), said.key).toBe(true);
    }
  });
});
