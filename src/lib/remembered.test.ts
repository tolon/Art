import { describe, expect, it } from "vitest";

import {
  forget,
  isFlag,
  isNumberBetween,
  isOneOf,
  isText,
  isTextList,
  isTextOrNothing,
  isWholeNumberBetween,
  recall,
  recallInto,
  remember,
} from "@/lib/remembered";

describe("recall", () => {
  it("gives back what the user chose", () => {
    expect(recall({ viewMode: "list" }, "viewMode", isText, "grid")).toBe("list");
  });

  it("falls back when nothing was ever chosen", () => {
    expect(recall({}, "viewMode", isText, "grid")).toBe("grid");
    expect(recall(null, "viewMode", isText, "grid")).toBe("grid");
  });

  it("falls back rather than handing on something it cannot vouch for", () => {
    // `settings.json` is a file a user can edit and an older ART may have
    // written. A view mode of `7` reaching a `switch` is a blank screen.
    expect(recall({ viewMode: 7 }, "viewMode", isText, "grid")).toBe("grid");
    expect(recall({ viewMode: null }, "viewMode", isText, "grid")).toBe("grid");
  });

  it("is not fooled by a store that is not a store", () => {
    // A truncated write, or a hand-edited file.
    for (const broken of ["", 3, [], true, undefined]) {
      expect(recall(broken, "viewMode", isText, "grid")).toBe("grid");
    }
  });

  it("keeps `false` and `0`, which are choices and not absences", () => {
    // The bug this pins: `held ?? fallback` would hand back the fallback for
    // both, so a user who turned something off would find it on again.
    expect(recall({ advanced: false }, "advanced", isFlag, true)).toBe(false);
    expect(recall({ size: 0 }, "size", isWholeNumberBetween(0, 10), 5)).toBe(0);
  });
});

describe("recallInto", () => {
  interface Config {
    label: string;
    enabled: boolean;
    slots: number;
  }
  const spec = {
    label: isText,
    enabled: isFlag,
    slots: isWholeNumberBetween(1, 100),
  };
  const fallback: Config = { label: "FF", enabled: false, slots: 10 };

  it("rebuilds what was saved", () => {
    const store = { gotek: { label: "Amiga", enabled: true, slots: 40 } };
    expect(recallInto<Config>(store, "gotek", spec, fallback)).toEqual({
      label: "Amiga",
      enabled: true,
      slots: 40,
    });
  });

  it("keeps every good field when one is bad", () => {
    // A whole-object guard would drop the label and the flag too, and the user
    // would silently lose choices they never touched.
    const store = { gotek: { label: "Amiga", enabled: true, slots: -5 } };
    expect(recallInto<Config>(store, "gotek", spec, fallback)).toEqual({
      label: "Amiga",
      enabled: true,
      slots: 10,
    });
  });

  it("gives a field ART has just gained its default, keeping the rest", () => {
    // What an older settings file looks like. Losing the user's whole config
    // because ART grew a field is the failure this shape exists to avoid.
    const store = { gotek: { label: "Amiga", enabled: true } };
    expect(recallInto<Config>(store, "gotek", spec, fallback)).toEqual({
      label: "Amiga",
      enabled: true,
      slots: 10,
    });
  });

  it("drops fields ART no longer has", () => {
    const store = { gotek: { label: "Amiga", removedLongAgo: 1 } };
    expect(recallInto<Config>(store, "gotek", spec, fallback)).not.toHaveProperty(
      "removedLongAgo"
    );
  });

  it("is the fallback when there is nothing there at all", () => {
    expect(recallInto<Config>({}, "gotek", spec, fallback)).toEqual(fallback);
    expect(recallInto<Config>({ gotek: "nonsense" }, "gotek", spec, fallback)).toEqual(
      fallback
    );
  });
});

describe("remember", () => {
  it("writes one value and leaves the rest alone", () => {
    const next = remember({ a: 1, b: 2 }, "b", 3);
    expect(next).toEqual({ a: 1, b: 3 });
  });

  it("does not mutate what it was given", () => {
    // Two screens writing in the same tick must not see each other's half-done
    // work — the whole reason this returns a new object.
    const before = { a: 1 };
    remember(before, "a", 2);
    expect(before).toEqual({ a: 1 });
  });

  it("starts a store when there was none", () => {
    expect(remember(null, "a", 1)).toEqual({ a: 1 });
    expect(remember("broken", "a", 1)).toEqual({ a: 1 });
  });
});

describe("forget", () => {
  it("removes one value", () => {
    expect(forget({ a: 1, b: 2 }, "a")).toEqual({ b: 2 });
  });

  it("says nothing about a value that was never there", () => {
    expect(forget({ a: 1 }, "b")).toEqual({ a: 1 });
  });
});

describe("guards", () => {
  it("isOneOf takes the code's list, not the file's", () => {
    const guard = isOneOf("grid", "list");
    expect(guard("grid")).toBe(true);
    // Legal in some older ART, meaningless now.
    expect(guard("gallery")).toBe(false);
    expect(guard(0)).toBe(false);
  });

  it("isWholeNumberBetween refuses the numbers that break a layout", () => {
    const guard = isWholeNumberBetween(10, 28);
    expect(guard(12)).toBe(true);
    expect(guard(9)).toBe(false);
    expect(guard(29)).toBe(false);
    expect(guard(12.5)).toBe(false);
    // `typeof NaN === "number"`, which is exactly why this checks more.
    expect(guard(NaN)).toBe(false);
    expect(guard(Infinity)).toBe(false);
  });

  it("isNumberBetween allows a fraction but still refuses NaN", () => {
    const guard = isNumberBetween(0, 1);
    expect(guard(0.5)).toBe(true);
    expect(guard(NaN)).toBe(false);
    expect(guard(2)).toBe(false);
  });

  it("isTextOrNothing tells a cleared path from a broken one", () => {
    // `null` is a real answer here: "no folder chosen" is a state the user can
    // put ART into deliberately.
    expect(isTextOrNothing(null)).toBe(true);
    expect(isTextOrNothing("F:\\Amiga")).toBe(true);
    expect(isTextOrNothing(undefined)).toBe(false);
    expect(isTextOrNothing(0)).toBe(false);
  });

  it("isTextList refuses a list with one bad entry", () => {
    expect(isTextList(["a", "b"])).toBe(true);
    expect(isTextList([])).toBe(true);
    expect(isTextList(["a", 2])).toBe(false);
    expect(isTextList("a")).toBe(false);
  });
});
