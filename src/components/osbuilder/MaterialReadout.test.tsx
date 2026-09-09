// @vitest-environment jsdom
//
// The readout is the screen this whole round exists for, and its risk is not
// that it crashes — it is that one row says a confident wrong thing about
// somebody's disks. So the tests here are almost all about **which sentence**
// a row gets, in **both** languages: a `Phrase` key that resolves in `en.json`
// and not in `tr.json` renders English to a Turkish user, compiles, and is
// invisible to `parity.test.ts` (which compares the catalogues to each other,
// never to a call site).
//
// Mocked at the `@/lib/*` boundary the rest of this suite mocks at — the
// wrapper around `invoke`, not `@tauri-apps/api` itself.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";

// Side-effecting import: initialises the real i18next instance synchronously,
// the same way `PackagePanel.test.tsx` gets one — without it `useTranslation`
// has nothing to read and every string renders as its own raw key.
import "@/i18n";
import { changeLanguage } from "@/i18n";
import type {
  BytesRead,
  Installed,
  SetSummary,
  SlotCandidate,
  SlotKind,
  SlotReport,
  SlotState,
} from "@/lib/osinstall";

const slotsMock = vi.hoisted(() => vi.fn());
const guideMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallSlots: slotsMock,
  osinstallWriteMaterialGuide: guideMock,
}));

const { MaterialReadout } = await import("@/components/osbuilder/MaterialReadout");

const NOT_READ: BytesRead = { state: "not-read" };
const NO_ROW: BytesRead = { state: "read-no-row" };
const OTHER_ROW: BytesRead = {
  state: "read-row",
  artefact: "boingbag-39-2",
  name: "BoingBag 3.9-2",
};

interface StateOptions {
  id?: string;
  kind?: SlotKind;
  name?: string;
  identity?: string;
  required?: boolean;
  filenames?: string[];
  position?: number;
  found?: SlotState["found"];
  candidates?: SlotCandidate[];
  installed?: Installed;
  chosenMissing?: string | null;
  blockedBy?: string[];
  incomplete?: string | null;
  expectsDirectories?: string[];
}

function state(options: StateOptions = {}): SlotState {
  return {
    slot: {
      id: options.id ?? "package:boingbag-39-1",
      kind: options.kind ?? "package",
      name: options.name ?? "BoingBag 3.9-1",
      identity: options.identity ?? "BoingBag3.9-1",
      artefact: "boingbag-39-1",
      required: options.required ?? false,
      filenames: options.filenames ?? ["BoingBag39-1.lha"],
      provenance: "Haage and Partners (3.9)",
      position: options.position ?? 1,
      requires: [],
      supersededBy: [],
      expectsDirectories: options.expectsDirectories ?? [],
    },
    found: options.found ?? null,
    candidates: options.candidates ?? [],
    installed: options.installed ?? { state: "no" },
    chosenMissing: options.chosenMissing ?? null,
    blockedBy: options.blockedBy ?? [],
    incomplete: options.incomplete ?? null,
  };
}

const foundBy = (matchedBy: string, path: string, bytesRead: BytesRead = NO_ROW) =>
  ({ path, matchedBy, row: null, confirmed: null, bytesRead }) as NonNullable<SlotState["found"]>;

function summary(over: Partial<SetSummary> = {}): SetSummary {
  return {
    release: "AmigaOS 3.9",
    requiredTotal: 1,
    requiredFound: 1,
    optionalTotal: 1,
    optionalFound: 1,
    ...over,
  };
}

function report(over: Partial<SlotReport> = {}): SlotReport {
  return {
    states: [],
    summary: summary(),
    unreadableFolders: [],
    crowdedFolders: [],
    ...over,
  };
}

function renderReadout() {
  return render(
    <MaterialReadout
      release="AmigaOS 3.9"
      folders={["E:\\amiga\\os39"]}
      treeRoot={null}
      rom={null}
      identifiedPass={0}
    />
  );
}

beforeEach(() => {
  slotsMock.mockReset().mockResolvedValue(report());
  guideMock.mockReset();
});

afterEach(async () => {
  cleanup();
  // Never let a Turkish test bleed its language into the next test file's
  // first render.
  await changeLanguage("en");
});

describe("every ending gets its own sentence, in both languages", () => {
  /**
   * One row per ending, each with the state that produces it. This is the
   * whole point of the component: "not found" and "not needed" are not the
   * same row, and neither are "ART did not look at the bytes" and "ART looked
   * and these bytes are a different artefact".
   */
  const endings: [string, SlotState][] = [
    ["found-by-hash", state({ found: foundBy("hash", "D:\\a\\BoingBag39-1.lha") })],
    ["found-by-name", state({ found: foundBy("top-level-directory", "D:\\a\\renamed.lha") })],
    [
      "chosen",
      state({
        kind: "rom",
        id: "rom",
        name: "Kickstart",
        identity: "40",
        found: foundBy("chosen", "D:\\roms\\kick.rom"),
      }),
    ],
    [
      "chosen-missing",
      state({
        kind: "rom",
        id: "rom",
        name: "Kickstart",
        identity: "40",
        chosenMissing: "E:\\roms\\kick.rom",
      }),
    ],
    [
      "guessed-by-filename",
      state({ candidates: [{ path: "D:\\a\\BoingBag39-1.lha", bytesRead: NOT_READ }] }),
    ],
    [
      "ambiguous",
      state({
        candidates: [
          { path: "D:\\a\\one.lha", bytesRead: NO_ROW },
          { path: "D:\\b\\two.lha", bytesRead: NO_ROW },
        ],
      }),
    ],
    ["not-found", state()],
  ];

  for (const language of ["en", "tr"] as const) {
    it(`renders a distinct, translated sentence for each ending in ${language}`, async () => {
      await changeLanguage(language);
      slotsMock.mockResolvedValue(
        report({
          states: endings.map(([, one], index) => ({
            ...one,
            slot: { ...one.slot, id: `slot-${index}`, position: index },
          })),
        })
      );
      renderReadout();

      const rows = await screen.findAllByTestId(/^material-row-/);
      expect(rows).toHaveLength(endings.length);

      const sentences = rows.map((row) => row.textContent ?? "");
      // Every row is real prose in this language, not a raw key and not an
      // unrendered `{{interpolation}}`.
      for (const sentence of sentences) {
        expect(sentence).not.toMatch(/osinstall\.slots\./);
        expect(sentence).not.toContain("{{");
      }
      // And no two endings collapse into one sentence, which is the property
      // the whole file exists for.
      expect(new Set(sentences).size).toBe(endings.length);
    });
  }

  it("keeps the three answers about a file's bytes apart on screen", async () => {
    // F13: a relabelled disk whose bytes the table knows under another
    // artefact used to be told its bytes were "in no table ART has".
    slotsMock.mockResolvedValue(
      report({
        states: [
          state({
            id: "a",
            position: 0,
            found: foundBy("volume-name", "D:\\a\\one.iso", NOT_READ),
          }),
          state({
            id: "b",
            position: 1,
            found: foundBy("volume-name", "D:\\a\\two.iso", NO_ROW),
          }),
          state({
            id: "c",
            position: 2,
            found: foundBy("volume-name", "D:\\a\\three.iso", OTHER_ROW),
          }),
        ],
      })
    );
    renderReadout();

    const rows = await screen.findAllByTestId("material-row-found-by-name");
    expect(rows).toHaveLength(3);
    const [unread, noRow, otherRow] = rows.map((row) => row.textContent ?? "");
    expect(new Set([unread, noRow, otherRow]).size).toBe(3);
    // The one that used to be a lie now names what the bytes actually are.
    expect(otherRow).toContain("BoingBag 3.9-2");
  });

  it("lists every candidate of an ambiguous row by its full path, never by name", async () => {
    // Fix round 1, F3: the ambiguity this row exists for is one artefact in
    // two folders, and its commonest shape is two copies under the *same*
    // name -- which file names rendered as one string twice.
    slotsMock.mockResolvedValue(
      report({
        states: [
          state({
            candidates: [
              { path: "D:\\disks\\BoingBag39-1.lha", bytesRead: NO_ROW },
              { path: "E:\\archives\\BoingBag39-1.lha", bytesRead: NO_ROW },
            ],
          }),
        ],
      })
    );
    renderReadout();

    const row = await screen.findByTestId("material-row-ambiguous");
    expect(row.textContent).toContain("D:\\disks\\BoingBag39-1.lha");
    expect(row.textContent).toContain("E:\\archives\\BoingBag39-1.lha");
  });

  it("shows the installed badge from the manifest, and never 'not installed'", async () => {
    slotsMock.mockResolvedValue(
      report({ states: [state({ installed: { state: "placed", at: "C/Foo" } })] })
    );
    renderReadout();
    expect((await screen.findByTestId("material-row-installed")).textContent).toBe(
      i18n.t("osinstall.slots.installedPlaced")
    );

    // A manifest that says nothing is not a claim, and there may be no tree
    // chosen at all.
    cleanup();
    slotsMock.mockResolvedValue(report({ states: [state()] }));
    renderReadout();
    await screen.findByTestId("material-row-not-found");
    expect(screen.queryByTestId("material-row-installed")).toBeNull();
  });

  it("says what a blocked row waits for", async () => {
    slotsMock.mockResolvedValue(
      report({
        states: [
          state({ id: "package:bb2", name: "BoingBag 3.9-2", position: 1, blockedBy: ["m"] }),
          state({ id: "m", kind: "medium", name: "AmigaOS3.9", position: 0 }),
        ],
      })
    );
    renderReadout();
    const blocked = await screen.findAllByTestId("material-row-blocked");
    expect(blocked).toHaveLength(1);
    // By name, never by slot id.
    expect(blocked[0].textContent).toContain("AmigaOS3.9");
    expect(blocked[0].textContent).not.toContain("package:bb2");
  });
});

describe("the set line", () => {
  it("counts the whole set and names the required shortfall apart", async () => {
    slotsMock.mockResolvedValue(
      report({
        states: [state()],
        summary: summary({
          requiredTotal: 2,
          requiredFound: 2,
          optionalTotal: 4,
          optionalFound: 3,
        }),
      })
    );
    renderReadout();
    const line = await screen.findByTestId("material-set-line");
    expect(line.textContent).toContain("AmigaOS 3.9");
    expect(line.textContent).toContain("5");
    expect(line.textContent).toContain("6");
    // Green: nothing *required* is missing, which is the only thing the
    // colour is allowed to mean. A set short of an optional file still
    // builds.
    expect(line.className).toContain("badge-ok");
  });

  it("goes red the moment a required slot is missing", async () => {
    slotsMock.mockResolvedValue(
      report({
        states: [state()],
        summary: summary({ requiredTotal: 2, requiredFound: 1 }),
      })
    );
    renderReadout();
    const line = await screen.findByTestId("material-set-line");
    expect(line.className).toContain("badge-err");
  });

  it("says a complete set is ready rather than counting a shortfall that is not there", async () => {
    // Round 2 review, L8. The "not needed" clause this test used to be about
    // went with the overlay slot kind on 2026-09-08 — no slot leaves the
    // totals any more, so the denominator cannot shrink between two calls
    // and there is nothing for that clause to explain.
    slotsMock.mockResolvedValue(
      report({
        states: [state(), state({ id: "other", position: 2 })],
        summary: summary({ optionalTotal: 2, optionalFound: 2 }),
      })
    );
    renderReadout();
    const line = await screen.findByTestId("material-set-line");
    expect(line.textContent).toBe(
      i18n.t("osinstall.slots.setLineReady", {
        release: "AmigaOS 3.9",
        found: 3,
        total: 3,
        missingRequired: 0,
      })
    );
  });
});

describe("while the pass is running, and when it fails", () => {
  it("says how many folders it is reading, never a bar with no total", async () => {
    // CLAUDE.md: a bar showing a fixed width with no total looks like
    // progress and carries none. `osinstall_slots` is one round trip and
    // reports no progress of its own, so the honest statement is the count.
    let settle: (value: SlotReport) => void = () => {};
    slotsMock.mockReturnValue(
      new Promise<SlotReport>((resolve) => {
        settle = resolve;
      })
    );
    render(
      <MaterialReadout
        release="AmigaOS 3.9"
        folders={["E:\\one", "E:\\two", "E:\\three"]}
        treeRoot={null}
        rom={null}
        identifiedPass={0}
      />
    );

    const running = await screen.findByTestId("material-readout-running");
    expect(running.textContent).toBe(
      i18n.t("osinstall.slots.reading", { count: 3 })
    );
    expect(running.textContent).toContain("3");

    settle(report());
    await waitFor(() => expect(screen.queryByTestId("material-readout-running")).toBeNull());
  });

  it("says a readout that could not be produced is ART's fault, not the folders'", async () => {
    slotsMock.mockRejectedValue(new Error("no scratch root"));
    renderReadout();
    const failed = await screen.findByTestId("material-readout-failed");
    expect(failed.textContent).toContain("no scratch root");
    // Never a set line beside it: "0 of 6 found" would be a claim about the
    // user's folders that ART has not made.
    expect(screen.queryByTestId("material-set-line")).toBeNull();
  });

  it("renders nothing at all before any folder is chosen", () => {
    const { container } = render(
      <MaterialReadout
        release="AmigaOS 3.9"
        folders={[]}
        treeRoot={null}
        rom={null}
        identifiedPass={0}
      />
    );
    expect(container.textContent).toBe("");
    expect(slotsMock).not.toHaveBeenCalled();
  });
});

describe("a folder ART could not read", () => {
  it("names it, and leaves every row resolved against the other folders standing", async () => {
    slotsMock.mockResolvedValue(
      report({
        states: [state({ found: foundBy("hash", "D:\\a\\BoingBag39-1.lha") })],
        unreadableFolders: ["E:\\gone"],
      })
    );
    renderReadout();

    const lines = await screen.findAllByTestId("material-readout-unreadable");
    expect(lines).toHaveLength(1);
    expect(lines[0].textContent).toBe(
      i18n.t("osinstall.slots.unreadableFolder", { folder: "E:\\gone" })
    );
    // The disks that *were* read are still reported as found: a remembered
    // path on a drive nobody plugged in must not make the material sitting in
    // the folder beside it read as missing.
    expect(screen.getByTestId("material-row-found-by-hash")).toBeTruthy();
  });

  it("says nothing when every folder could be read", async () => {
    slotsMock.mockResolvedValue(report({ states: [state()] }));
    renderReadout();
    await screen.findByTestId("material-row-not-found");
    expect(screen.queryByTestId("material-readout-unreadable")).toBeNull();
  });
});

describe("a superseded answer is dropped, never rendered", () => {
  /// ART-089's own mechanism. A folder list changed while a slow answer is in
  /// the air must not have the previous list's report land on top of the new
  /// one — the readout would then describe folders nobody is looking at.
  it("keeps the newer folder list's answer when an older one settles late", async () => {
    let settleFirst: (value: SlotReport) => void = () => {};
    slotsMock.mockReturnValueOnce(
      new Promise<SlotReport>((resolve) => {
        settleFirst = resolve;
      })
    );
    slotsMock.mockResolvedValue(
      report({ states: [state({ name: "the second answer", found: foundBy("hash", "D:\\b.lha") })] })
    );

    const { rerender } = render(
      <MaterialReadout
        release="AmigaOS 3.9"
        folders={["E:\\one"]}
        treeRoot={null}
        rom={null}
        identifiedPass={0}
      />
    );
    rerender(
      <MaterialReadout
        release="AmigaOS 3.9"
        folders={["E:\\two"]}
        treeRoot={null}
        rom={null}
        identifiedPass={0}
      />
    );
    await screen.findByTestId("material-row-found-by-hash");

    // The first call now settles, with a report about a folder nobody is
    // looking at any more.
    settleFirst(report({ states: [state({ name: "the first answer" })] }));
    await waitFor(() =>
      expect(screen.queryByTestId("material-row-not-found")).toBeNull()
    );
    expect(screen.getByTestId("material-row-found-by-hash").textContent).toContain(
      "the second answer"
    );
  });
});

describe("the identification pass changes what the readout may say", () => {
  /// **Fix round 1, F2.** `osinstall_slots` hashes nothing: it reads the scan
  /// cache the identify job fills. Nothing in the readout's dependency list
  /// changed when that job finished, so a first visit with a cold cache said
  /// "nobody has read its bytes yet -- identify this folder by content"
  /// directly above a section reporting that it had just hashed them.
  it("re-asks once the pass has landed, and the row's sentence changes with it", async () => {
    const cold = report({
      states: [state({ found: foundBy("volume-name", "D:\\a\\one.iso", NOT_READ) })],
    });
    const warm = report({
      states: [state({ found: foundBy("volume-name", "D:\\a\\one.iso", NO_ROW) })],
    });
    slotsMock.mockResolvedValueOnce(cold).mockResolvedValue(warm);

    const props = {
      release: "AmigaOS 3.9" as const,
      folders: ["E:\\amiga\\os39"],
      treeRoot: null,
      rom: null,
    };
    const { rerender } = render(<MaterialReadout {...props} identifiedPass={0} />);

    // Before the pass: the honest "ART did not look" sentence.
    await screen.findByTestId("material-row-found-by-name");
    expect(screen.getByTestId("material-row-found-by-name").textContent).toContain(
      i18n.t("osinstall.slots.foundByNameUnread", {
        file: "one.iso",
        name: "BoingBag 3.9-1",
        identity: "BoingBag3.9-1",
      })
    );

    // The pass completes.
    rerender(<MaterialReadout {...props} identifiedPass={1} />);

    await waitFor(() =>
      expect(screen.getByTestId("material-row-found-by-name").textContent).toContain(
        i18n.t("osinstall.slots.foundByName", {
          file: "one.iso",
          name: "BoingBag 3.9-1",
          identity: "BoingBag3.9-1",
        })
      )
    );
    expect(slotsMock).toHaveBeenCalledTimes(2);
  });

  /// **Fix round 1, F9.** A set line and a row list left standing under a
  /// "Reading 3 folders..." heading are counts of a folder set the reader has
  /// already changed -- numbers that look current and are not.
  it("takes the previous answer down while a new pass runs", async () => {
    slotsMock.mockResolvedValueOnce(
      report({ states: [state({ found: foundBy("hash", "D:\\a\\one.lha") })] })
    );
    let settle: (value: SlotReport) => void = () => {};
    slotsMock.mockReturnValue(
      new Promise<SlotReport>((resolve) => {
        settle = resolve;
      })
    );

    const props = { release: "AmigaOS 3.9" as const, treeRoot: null, rom: null };
    const { rerender } = render(
      <MaterialReadout {...props} folders={["E:\\one"]} identifiedPass={0} />
    );
    await screen.findByTestId("material-set-line");

    rerender(<MaterialReadout {...props} folders={["E:\\one", "E:\\two"]} identifiedPass={0} />);

    await screen.findByTestId("material-readout-running");
    expect(screen.queryByTestId("material-set-line")).toBeNull();
    expect(screen.queryByTestId("material-row-found-by-hash")).toBeNull();

    settle(report({ states: [state()] }));
    await screen.findByTestId("material-set-line");
  });
});

describe("a file the user picked by hand (design § 3.4, review M5)", () => {
  /// The readout could not see the panel's overrides at all, so a user who
  /// chose BoingBag 3.9-1's archive on the Amiga-side step and stepped back
  /// read *"not in the folders you named"* about the file the run was going
  /// to use. Two screens, one artefact, opposite sentences — and the red one
  /// was on the screen this round exists to make authoritative.
  it("asks with the overrides it was given, and says the file was chosen", async () => {
    slotsMock.mockResolvedValue(
      report({
        states: [state({ found: foundBy("chosen", "D:\\pkg\\my-own-copy.lha") })],
      })
    );
    render(
      <MaterialReadout
        release="AmigaOS 3.9"
        folders={["E:\\amiga\\os39"]}
        treeRoot={null}
        rom={null}
        identifiedPass={0}
        overrides={[["package:boingbag-39-1", "D:\\pkg\\my-own-copy.lha"]]}
      />
    );

    await waitFor(() =>
      expect(slotsMock).toHaveBeenCalledWith("AmigaOS 3.9", ["E:\\amiga\\os39"], null, null, [
        ["package:boingbag-39-1", "D:\\pkg\\my-own-copy.lha"],
      ])
    );
    expect(await screen.findByTestId("material-row-chosen")).toBeTruthy();
    expect(
      screen.getByText(
        i18n.t("osinstall.slots.chosen", {
          file: "my-own-copy.lha",
          name: "BoingBag 3.9-1",
        })
      )
    ).toBeTruthy();
    // Never the red row about a file ART is holding a path for.
    expect(screen.queryByTestId("material-row-not-found")).toBeNull();
  });

  it("sends an empty list when the user has chosen nothing", async () => {
    renderReadout();
    await waitFor(() =>
      expect(slotsMock).toHaveBeenCalledWith("AmigaOS 3.9", ["E:\\amiga\\os39"], null, null, [])
    );
  });
});

describe("a folder ART stopped reading (design § 6, review L7)", () => {
  /// Design § 6: *"bound the count and **name the bound**."* Silence would
  /// make an artefact ART never reached read as an artefact that is not
  /// there.
  it("names the folder and the number, and says nothing when there is none", async () => {
    slotsMock.mockResolvedValue(report({ crowdedFolders: [["E:\\aminet", 200]] }));
    renderReadout();

    const line = await screen.findByTestId("material-readout-crowded");
    expect(line.textContent).toBe(
      i18n.t("osinstall.slots.crowdedFolder", { folder: "E:\\aminet", bound: 200 })
    );
    expect(line.textContent).toContain("200");

    cleanup();
    slotsMock.mockResolvedValue(report());
    renderReadout();
    await screen.findByTestId("material-set-line");
    expect(screen.queryByTestId("material-readout-crowded")).toBeNull();
  });
});

describe("the list, written into the folder (design § 3.7)", () => {
  /// **Nothing is written until somebody asks.** The button is the ask; the
  /// readout rendering is not. ART does not write into a user's folder
  /// because they pointed at it — `remembered.ts`'s rule about the user's own
  /// settings, applied to the user's own disk.
  it("writes nothing at all until the button is pressed", async () => {
    renderReadout();
    await screen.findByTestId("material-guide");
    expect(guideMock).not.toHaveBeenCalled();
  });

  it("writes one guide per folder, in the folder that button names", async () => {
    guideMock.mockResolvedValue({
      state: "written",
      path: "E:\\second\\ART - what goes here.txt",
    });
    render(
      <MaterialReadout
        release="AmigaOS 3.9"
        folders={["E:\\first", "E:\\second"]}
        treeRoot={null}
        rom={null}
        identifiedPass={0}
      />
    );

    const buttons = await screen.findAllByTestId("material-guide-write");
    expect(buttons).toHaveLength(2);
    await userEvent.setup().click(buttons[1]);

    // The folder that button belongs to, the chosen release, and the UI's own
    // language — the filename is the guide's own data and never travels.
    await waitFor(() =>
      expect(guideMock).toHaveBeenCalledWith("E:\\second", "AmigaOS 3.9", "en")
    );
    expect(
      await screen.findByText(
        i18n.t("osinstall.material.guide.written", {
          path: "E:\\second\\ART - what goes here.txt",
        })
      )
    ).toBeTruthy();
    // And only that folder's line says anything: a "written" line under a
    // folder ART did not touch is the screen claiming what it did not do.
    expect(screen.queryAllByTestId("material-guide-written")).toHaveLength(1);
  });

  /// `SAFE_CREATE`, said as its own ending. "Already there" and "ART could
  /// not write it" are two different next steps — delete a file, or something
  /// is wrong — and collapsing them is this project's named defect.
  it("keeps 'already there' apart from a failure, and names the file to delete", async () => {
    guideMock.mockResolvedValue({
      state: "alreadyThere",
      path: "E:\\amiga\\os39\\ART - what goes here.txt",
    });
    renderReadout();
    await userEvent.setup().click(await screen.findByTestId("material-guide-write"));

    const line = await screen.findByTestId("material-guide-alreadyThere");
    expect(line.textContent).toBe(
      i18n.t("osinstall.material.guide.alreadyThere", {
        path: "E:\\amiga\\os39\\ART - what goes here.txt",
      })
    );
    expect(screen.queryByTestId("material-guide-failed")).toBeNull();
    expect(screen.queryByTestId("material-guide-written")).toBeNull();
  });

  it("says a real failure is ART's, not a statement about the folder", async () => {
    guideMock.mockRejectedValue(new Error("access is denied"));
    renderReadout();
    await userEvent.setup().click(await screen.findByTestId("material-guide-write"));

    const line = await screen.findByTestId("material-guide-failed");
    expect(line.textContent).toContain("access is denied");
    expect(screen.queryByTestId("material-guide-written")).toBeNull();
  });

  /// The language the *user* is reading in decides which guide is written —
  /// the Rust side owns the filename and the words, and this is the one thing
  /// the screen has to get right about it.
  it("asks for the guide in the language on screen", async () => {
    await changeLanguage("tr");
    guideMock.mockResolvedValue({
      state: "written",
      path: "E:\\amiga\\os39\\ART - buraya ne konur.txt",
    });
    renderReadout();
    await userEvent.setup().click(await screen.findByTestId("material-guide-write"));

    await waitFor(() =>
      expect(guideMock).toHaveBeenCalledWith("E:\\amiga\\os39", "AmigaOS 3.9", "tr")
    );
    expect(
      await screen.findByText(
        i18n.t("osinstall.material.guide.written", {
          path: "E:\\amiga\\os39\\ART - buraya ne konur.txt",
        })
      )
    ).toBeTruthy();
  });
});
