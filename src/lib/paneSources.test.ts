import { describe, expect, it } from "vitest";

import {
  currentPaneSourceValue,
  paneSourceOptions,
  parsePaneSource,
} from "@/lib/paneSources";

describe("the pane source combo's options", () => {
  it("lists the enumerated mounts first, then Folder…, then the five image kinds", () => {
    const options = paneSourceOptions(["C:\\", "D:\\"]);

    expect(options.map((option) => option.value)).toEqual([
      "root:C:\\",
      "root:D:\\",
      "folder",
      "image:adf",
      "image:hdf",
      "image:iso",
      "image:archive",
      "image:c64",
    ]);
  });

  it("labels a mount with its own path and everything else with an i18n key", () => {
    const options = paneSourceOptions(["D:\\"]);

    // A drive letter is not a sentence — it is shown verbatim, in every
    // language, which is why it travels as `literal` rather than as a key.
    expect(options[0]).toMatchObject({ labelKey: null, literal: "D:\\" });
    // Everything else carries a key and no literal, so nothing here can leak
    // an untranslated English label onto the screen.
    for (const option of options.slice(1)) {
      expect(option.labelKey).toBeTruthy();
      expect(option.literal).toBeNull();
    }
  });

  it("hardcodes no drive letters — no mounts means no mounts", () => {
    // The brief is explicit: enumerate, never assume a `C:\`. A build that
    // quietly fell back to one would show a drive that may not exist.
    const values = paneSourceOptions([]).map((option) => option.value);
    expect(values.filter((value) => value.startsWith("root:"))).toEqual([]);
  });
});

describe("parsing an option's value back", () => {
  it("round-trips every option the combo offers", () => {
    for (const option of paneSourceOptions(["C:\\", "D:\\"])) {
      expect(parsePaneSource(option.value)).toEqual(option.choice);
    }
  });

  it("refuses a value that is not one of ours rather than guessing", () => {
    // The placeholder the combo shows for a pane under no enumerated mount
    // has an empty value; selecting it must navigate nowhere.
    expect(parsePaneSource("")).toBeNull();
    expect(parsePaneSource("root:")).toBeNull();
    expect(parsePaneSource("image:pfs3")).toBeNull();
    expect(parsePaneSource("drive:D")).toBeNull();
  });

  it("keeps a mount path with a colon in it intact", () => {
    // `root:` is stripped by length, not by splitting on `:` — a Windows path
    // is full of colons and splitting would hand back `C`.
    expect(parsePaneSource("root:C:\\Amiga\\Games")).toEqual({
      kind: "root",
      path: "C:\\Amiga\\Games",
    });
  });
});

describe("which option a pane is showing", () => {
  const roots = ["C:\\", "D:\\"];

  it("names the image kind for an image pane", () => {
    expect(currentPaneSourceValue("adf", "D:\\games\\Lotus.adf", roots)).toBe("image:adf");
    expect(currentPaneSourceValue("c64", "D:\\games\\Boulder.d64", roots)).toBe("image:c64");
  });

  it("names the mount a local folder is under, whatever its case", () => {
    expect(currentPaneSourceValue("local", "D:\\Projeler\\Amiga", roots)).toBe("root:D:\\");
    expect(currentPaneSourceValue("local", "d:\\projeler", roots)).toBe("root:D:\\");
  });

  it("prefers the longest matching mount", () => {
    // `/` and `/media/sd` both match on a Unix build; the answer is the one
    // the folder is actually on.
    expect(currentPaneSourceValue("local", "/media/sd/games", ["/", "/media/sd"])).toBe(
      "root:/media/sd"
    );
  });

  it("shows no mount for a folder under none of them", () => {
    // A UNC share is a real place a pane can be; claiming it is on `C:\`
    // would be a combo that lies about where the user is.
    expect(currentPaneSourceValue("local", "\\\\nas\\amiga", roots)).toBe("");
  });
});
