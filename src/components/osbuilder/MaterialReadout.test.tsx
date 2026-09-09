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
// the same way `FilesTab.test.tsx` gets one — without it `useTranslation`
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

vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallSlots: slotsMock,
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

function renderReadout(onChoose?: (slotId: string, path: string) => void) {
  return render(
    <MaterialReadout
      release="AmigaOS 3.9"
      folders={["E:\\amiga\\os39"]}
      treeRoot={null}
      rom={null}
      identifiedPass={0}
      onChoose={onChoose}
    />
  );
}

beforeEach(() => {
  slotsMock.mockReset().mockResolvedValue(report());
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

  /// The guide buttons moved to the folder column on 2026-09-09
  /// (simplification design § 4.2): a guide is an action on a folder, and
  /// the readout is the answer, not the place to act. Rendering them here
  /// again would put the same button on screen twice.
  it("draws no guide button — that is the folder column's", async () => {
    renderReadout();
    await screen.findByTestId("material-set-line");
    expect(screen.queryByTestId("material-guide")).toBeNull();
    expect(screen.queryAllByTestId("material-guide-write")).toHaveLength(0);
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

// ---------------------------------------------------------------------------
// The choice an ambiguous row is asking for (round 4 whole-branch review, C1)
// ---------------------------------------------------------------------------
//
// The owner has two copies of BoingBag 3.9-1. `chain.rs` answers
// `Refused{Ambiguous}`, the row stays tickable on tab 2 because the ambiguity
// is a question the *user* settles — and until this round nothing on this
// screen settled it. Tab 4 told them to "choose its file on the Amiga files
// tab", which is this tab, where the row listed two paths and offered no
// control at all. Three screens agreeing on a next step that did not exist.

describe("an ambiguous row offers its candidates", () => {
  const CANDIDATES: SlotCandidate[] = [
    { path: "D:\disks\BoingBag39-1.lha", bytesRead: NO_ROW },
    { path: "E:\archives\BoingBag39-1.lha", bytesRead: NO_ROW },
  ];

  it("draws one button per candidate and hands the slot and that path up", async () => {
    const chose = vi.fn();
    slotsMock.mockResolvedValue(report({ states: [state({ candidates: CANDIDATES })] }));
    renderReadout(chose);

    const buttons = await screen.findAllByTestId("material-choose");
    expect(buttons).toHaveLength(2);
    // The **full path** is the accessible name, not the file name: both
    // copies are called `BoingBag39-1.lha`, so two names would be two
    // controls nobody using a screen reader could tell apart (ART-240).
    expect(buttons.map((b) => b.getAttribute("aria-label"))).toEqual(
      CANDIDATES.map((c) => i18n.t("osinstall.material.chooseThis", { file: c.path }))
    );

    await userEvent.click(buttons[1]);
    // Asserted as the exact pair, not "was called": handing the wrong slot's
    // id, or the other candidate, is the defect this exists against and both
    // would pass a bare `toHaveBeenCalled`.
    expect(chose.mock.calls).toEqual([
      ["package:boingbag-39-1", "E:\archives\BoingBag39-1.lha"],
    ]);
  });

  it("renders in Turkish as a sentence, not a key", async () => {
    await changeLanguage("tr");
    slotsMock.mockResolvedValue(report({ states: [state({ candidates: CANDIDATES })] }));
    renderReadout(vi.fn());

    const [first] = await screen.findAllByTestId("material-choose");
    expect(first.getAttribute("aria-label")).toBe(
      i18n.t("osinstall.material.chooseThis", { file: CANDIDATES[0].path })
    );
    expect(first.getAttribute("aria-label")).not.toContain("osinstall.");
  });

  it("offers nothing on a row that has already resolved", async () => {
    // The control, measured rather than assumed. A row ART settled by its
    // bytes is not asking anything, and a *Use this* button over it would
    // invite the user to override a find with the very file it found.
    slotsMock.mockResolvedValue(
      report({
        states: [
          state({
            found: {
              path: "D:\disks\BoingBag39-1.lha",
              matchedBy: "hash",
              row: null,
              confirmed: null,
              bytesRead: NO_ROW,
            },
          }),
        ],
      })
    );
    renderReadout(vi.fn());

    await screen.findByTestId("material-row-found-by-hash");
    expect(screen.queryByTestId("material-choose")).toBeNull();
  });

  it("offers nothing on a single-candidate guess, which is not a question", async () => {
    // **The control that has candidates.** A resolved row alone cannot show
    // the rule, because `resolve_one` hands a found slot an empty candidate
    // list — so the mutation "draw a button on every row" survives it,
    // measured. One candidate is a real state with a real list, and it is
    // still not a choice: there is nothing to pick *between*, and the row
    // already says what ART thinks the file is.
    slotsMock.mockResolvedValue(
      report({
        states: [
          state({ candidates: [{ path: "D:\disks\BoingBag39-1.lha", bytesRead: NOT_READ }] }),
        ],
      })
    );
    renderReadout(vi.fn());

    const row = await screen.findByTestId("material-row-guessed-by-filename");
    expect(row.textContent).toContain("BoingBag39-1.lha");
    expect(screen.queryByTestId("material-choose")).toBeNull();
  });

  it("draws no button at all when the caller cannot store a choice", async () => {
    // A readout with no `onChoose` is one mounted where nothing can remember
    // the answer. A control that changes nothing is this project's own named
    // defect, so the row goes back to listing its candidates and no more.
    slotsMock.mockResolvedValue(report({ states: [state({ candidates: CANDIDATES })] }));
    renderReadout();

    const row = await screen.findByTestId("material-row-ambiguous");
    expect(row.textContent).toContain("E:\archives\BoingBag39-1.lha");
    expect(screen.queryByTestId("material-choose")).toBeNull();
  });
});
