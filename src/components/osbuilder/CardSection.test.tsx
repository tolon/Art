// @vitest-environment jsdom
//
// The card section (round 4, task 8): the partitions, what fills them, what
// they measure, and the one line that says whether it all fits.
//
// Mocked at the `@/lib/*` boundary — `@/lib/cardOs`'s three read-only
// commands and `@/lib/dropTarget`'s hit test — never `@tauri-apps/api`
// itself (the house pattern, `CardBuilder.test.tsx`). `@/lib/settings` is
// mocked one layer down for the reason that file records: `useRemembered`
// writes through `useSettingsStore` and the real `saveSettings` rejects in
// jsdom with nothing to catch it.
//
// `cardRowAt` is mocked rather than exercised: jsdom's
// `document.elementFromPoint` always answers `null`, so hit-testing here
// would prove nothing about the real conversion — which is measured in
// `experiment-drop-coordinates.md` and unit-tested in `dropTarget.test.ts`.
// What these cases are about is what the section *does* with the index it is
// given: the row under the pointer takes the drop and no other row does.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";

import { changeLanguage } from "@/i18n";
import type { CardOsMeasureResult, MeasuredCard } from "@/lib/cardOs";
import { useSettingsStore } from "@/stores/settingsStore";

const measureMock = vi.hoisted(() => vi.fn());
const onMeasureMock = vi.hoisted(() => vi.fn());
const classifyMock = vi.hoisted(() => vi.fn());
const checkNameMock = vi.hoisted(() => vi.fn());
const cardRowAtMock = vi.hoisted(() => vi.fn());
const dialogOpenMock = vi.hoisted(() => vi.fn());
const dialogSaveMock = vi.hoisted(() => vi.fn());
const outletContextMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/cardOs", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/cardOs")>()),
  cardOsMeasure: measureMock,
  onCardOsMeasureResult: onMeasureMock,
  cardOsClassify: classifyMock,
  cardOsCheckVolumeName: checkNameMock,
}));

vi.mock("@/lib/dropTarget", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/dropTarget")>()),
  cardRowAt: cardRowAtMock,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: dialogOpenMock,
  save: dialogSaveMock,
}));

vi.mock("react-router-dom", async (importOriginal) => ({
  ...(await importOriginal<typeof import("react-router-dom")>()),
  useOutletContext: outletContextMock,
}));

vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  getSettings: vi.fn(),
  saveSettings: vi.fn().mockResolvedValue(undefined),
}));

const { CardSection } = await import("@/components/osbuilder/CardSection");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");

const RELEASE = "AmigaOS 3.9";
const TREE = "E:\\dist39";

/** The handler the section subscribed with, so a test can answer its job. */
let answerMeasure: ((result: CardOsMeasureResult) => void) | null = null;

function seedRemembered(overrides: Record<string, unknown>) {
  useSettingsStore.setState({
    loaded: true,
    settings: { ...DEFAULT_SETTINGS, remembered: { ...overrides } },
  });
}

function rememberedBag(): Record<string, unknown> {
  return useSettingsStore.getState().settings.remembered as Record<string, unknown>;
}

/** A session with a built tree, a material folder and the partitions given. */
function seedCard(partitions: { name: string; sources: string[] }[]) {
  seedRemembered({
    "buildSession.release": RELEASE,
    "buildSession.tree": { root: TREE, builtHere: true },
    [`buildSession.material.${RELEASE}`]: { folders: [{ path: "E:\\media", layer: null }] },
    [`osinstall.cardTarget.${RELEASE}`]: {
      sizeGb: 64,
      image: null,
      emu68Archive: null,
      pfs3Driver: null,
      partitions,
    },
  });
}

/** What `card_os_measure` answers for a card that fits. */
function measuredCard(sizes: [string, number][]): MeasuredCard {
  return {
    plan: {
      image_bytes: 60_800_000_000,
      area_bytes: 59_618_883_994,
      partitions: sizes.map(([volume_name, bytes], index) => ({
        drive_name: `SDH${index}`,
        volume_name,
        bytes,
        spec: {} as MeasuredCard["plan"]["partitions"][number]["spec"],
      })),
    },
    system: {
      volumeName: "System",
      driveName: "SDH0",
      sources: [{ path: TREE, kind: { kind: "folder" }, files: 1915, bytes: 23_500_000 }],
      writer: { writer: "native" },
    },
    partitions: [],
    driver: { path: "E:\\media\\pfs3aio", fromArchive: "E:\\media\\pfs3aio.lha", version: 19, revision: 2 },
    stagingBytes: 0,
  };
}

beforeEach(() => {
  answerMeasure = null;
  measureMock.mockReset().mockResolvedValue(7);
  onMeasureMock.mockReset().mockImplementation((handler: (r: CardOsMeasureResult) => void) => {
    answerMeasure = handler;
    return Promise.resolve(() => {});
  });
  classifyMock.mockReset().mockImplementation((paths: string[]) =>
    Promise.resolve(paths.map((path) => ({ path, kind: { kind: "folder" }, why: null })))
  );
  checkNameMock.mockReset().mockResolvedValue({ ok: true });
  cardRowAtMock.mockReset().mockReturnValue(null);
  dialogOpenMock.mockReset();
  dialogSaveMock.mockReset();
  outletContextMock.mockReset().mockReturnValue({});
});

afterEach(async () => {
  cleanup();
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
  await changeLanguage("en");
});

/** Deliver the answer for the job the section last asked for. */
async function deliver(result: Omit<CardOsMeasureResult, "jobId">) {
  await waitFor(() => expect(answerMeasure).not.toBeNull());
  await waitFor(() => expect(measureMock).toHaveBeenCalled());
  const jobId = (await measureMock.mock.results.at(-1)!.value) as number;
  answerMeasure!({ jobId, ...result });
}

describe("the partitions a card always has", () => {
  it("shows System and Work, and offers to remove neither", async () => {
    seedCard([
      { name: "System", sources: [] },
      { name: "Work", sources: [] },
    ]);
    render(<CardSection />);
    expect(await screen.findByTestId("card-row-System")).toBeTruthy();
    expect(screen.getByTestId("card-row-Work")).toBeTruthy();
    expect(screen.queryByTestId("card-row-remove-System")).toBeNull();
    expect(screen.queryByTestId("card-row-remove-Work")).toBeNull();
    // Neither takes sources — the core builds System from the tree and Work
    // from what is left over, so neither is in the request at all.
    expect(screen.queryByTestId("card-row-add-System")).toBeNull();
    expect(screen.queryByTestId("card-row-add-Work")).toBeNull();
  });

  it("adds one of the offered names and keeps it for this release", async () => {
    seedCard([
      { name: "System", sources: [] },
      { name: "Work", sources: [] },
    ]);
    render(<CardSection />);
    await userEvent.click(await screen.findByTestId("card-add-partition"));
    await userEvent.click(screen.getByTestId("card-add-preset-Games"));

    expect(await screen.findByTestId("card-row-Games")).toBeTruthy();
    await waitFor(() => {
      const target = rememberedBag()[`osinstall.cardTarget.${RELEASE}`] as {
        partitions: { name: string }[];
      };
      expect(target.partitions.map((p) => p.name)).toEqual(["System", "Games", "Work"]);
    });
  });

  /**
   * **Q7: the core owns the name rule.** `check_name` is the authority; a
   * screen that re-derived it would be a second answer, and the one the user
   * reads would be the wrong one exactly when they disagree.
   */
  it("refuses a typed name the core refuses, and says which rule", async () => {
    checkNameMock.mockResolvedValue({ ok: false, why: "reserved-character", maxBytes: 30 });
    seedCard([
      { name: "System", sources: [] },
      { name: "Work", sources: [] },
    ]);
    render(<CardSection />);
    await userEvent.click(await screen.findByTestId("card-add-partition"));
    await userEvent.type(screen.getByTestId("card-new-name"), "Games:");
    await userEvent.click(screen.getByTestId("card-new-name-add"));

    expect((await screen.findByTestId("card-new-name-problem")).textContent).toBe(
      i18n.t("cardSection.nameProblem.reservedCharacter")
    );
    expect(screen.queryByTestId("card-row-Games:")).toBeNull();
    expect(checkNameMock).toHaveBeenCalledWith("Games:");
  });
});

describe("what fills a partition", () => {
  /**
   * **Q3: the row under the pointer, and no other.** The one global listener
   * still owns the drop (`Layout.tsx`); the section reads the position it
   * carried out and asks `cardRowAt` which row it landed on.
   */
  it("gives a dropped folder to the row under the pointer and to nowhere else", async () => {
    seedCard([
      { name: "System", sources: [] },
      { name: "Games", sources: [] },
      { name: "Stuff", sources: [] },
      { name: "Work", sources: [] },
    ]);
    // The pointer was over Stuff (index 2 of the target's own list).
    cardRowAtMock.mockReturnValue(2);
    outletContextMock.mockReturnValue({
      analyses: [{ path: "E:\\demos" }],
      dropPosition: { x: 400, y: 320 },
    });
    render(<CardSection />);

    await waitFor(() =>
      expect(
        within(screen.getByTestId("card-row-sources-Stuff")).queryByText(/demos/)
      ).toBeTruthy()
    );
    expect(screen.queryByTestId("card-row-sources-Games")).toBeNull();
    await waitFor(() => {
      const target = rememberedBag()[`osinstall.cardTarget.${RELEASE}`] as {
        partitions: { name: string; sources: string[] }[];
      };
      expect(target.partitions.map((p) => p.sources)).toEqual([[], [], ["E:\\demos"], []]);
    });
  });

  it("ignores a drop that landed on no row at all", async () => {
    seedCard([
      { name: "System", sources: [] },
      { name: "Games", sources: [] },
      { name: "Work", sources: [] },
    ]);
    cardRowAtMock.mockReturnValue(null);
    outletContextMock.mockReturnValue({
      analyses: [{ path: "E:\\demos" }],
      dropPosition: { x: 10, y: 10 },
    });
    render(<CardSection />);
    await screen.findByTestId("card-row-Games");
    await waitFor(() => expect(cardRowAtMock).toHaveBeenCalled());
    expect(screen.queryByTestId("card-row-sources-Games")).toBeNull();
  });

  /** Q4: WCAG 2.2 SC 2.5.7 — a single-pointer path that is not a drag. */
  it("adds through Add… to its own row, not to the first one", async () => {
    seedCard([
      { name: "System", sources: [] },
      { name: "Games", sources: [] },
      { name: "Stuff", sources: [] },
      { name: "Work", sources: [] },
    ]);
    render(<CardSection />);
    dialogOpenMock.mockResolvedValueOnce("E:\\stuff");
    await userEvent.click(await screen.findByTestId("card-row-add-Stuff"));

    await waitFor(() =>
      expect(within(screen.getByTestId("card-row-sources-Stuff")).queryByText(/stuff/)).toBeTruthy()
    );
    expect(screen.queryByTestId("card-row-sources-Games")).toBeNull();
  });

  /** Q4 again: §46's contextual actions, which is also the keyboard path. */
  it("offers the same additions from the row's own menu", async () => {
    seedCard([
      { name: "System", sources: [] },
      { name: "Games", sources: [] },
      { name: "Work", sources: [] },
    ]);
    render(<CardSection />);
    await userEvent.click(await screen.findByTestId("card-row-menu-Games"));
    const menu = screen.getByTestId("card-row-menu-items-Games");
    dialogOpenMock.mockResolvedValueOnce(["E:\\games\\Turrican.lha"]);
    await userEvent.click(within(menu).getByTestId("card-row-menu-files-Games"));

    await waitFor(() =>
      expect(
        within(screen.getByTestId("card-row-sources-Games")).queryByText(/Turrican\.lha/)
      ).toBeTruthy()
    );
  });

  /**
   * **Q6: a refusal is not an absence.** A source the core cannot use has a
   * reason and a next step; "unknown" is neither.
   */
  it("says why a source cannot be used, by its own kind", async () => {
    classifyMock.mockResolvedValue([
      {
        path: "E:\\games\\broken.lha",
        kind: null,
        why: { reason: "archive-unreadable", detail: "unexpected end of file" },
      },
    ]);
    seedCard([
      { name: "System", sources: [] },
      { name: "Games", sources: ["E:\\games\\broken.lha"] },
      { name: "Work", sources: [] },
    ]);
    render(<CardSection />);

    const sources = await screen.findByTestId("card-row-sources-Games");
    await waitFor(() =>
      expect(sources.textContent).toContain(
        i18n.t("cardSection.unusable.archiveUnreadable", { detail: "unexpected end of file" })
      )
    );
    expect(sources.textContent).not.toContain("unknown");
  });
});

describe("sizes, the total, and an overflow", () => {
  it("states the measured total and the card's own bytes", async () => {
    seedCard([
      { name: "System", sources: [] },
      { name: "Games", sources: ["E:\\games"] },
      { name: "Work", sources: [] },
    ]);
    render(<CardSection />);
    await deliver({
      measured: measuredCard([
        ["System", 838_860_800],
        ["Games", 4_900_000_000],
        ["Work", 50_000_000_000],
      ]),
      refusal: null,
    });

    const total = await screen.findByTestId("card-total");
    await waitFor(() => expect(total.textContent).toContain("55.7"));
    expect(total.textContent).toContain("60.8");
    // And the row itself carries its own planned size.
    expect(screen.getByTestId("card-row-size-Games").textContent).toContain("4.9");
  });

  /**
   * **A number that is no longer true is worse than no number.** While a
   * measure is in flight the row says it is measuring — it never leaves the
   * previous card's figure on screen under the new inputs.
   */
  it("shows no number while a measure is in flight", async () => {
    seedCard([
      { name: "System", sources: [] },
      { name: "Games", sources: ["E:\\games"] },
      { name: "Work", sources: [] },
    ]);
    render(<CardSection />);
    await deliver({
      measured: measuredCard([
        ["System", 838_860_800],
        ["Games", 4_900_000_000],
        ["Work", 50_000_000_000],
      ]),
      refusal: null,
    });
    await waitFor(() =>
      expect(screen.getByTestId("card-row-size-Games").textContent).toContain("4.9")
    );

    // A second source changes the inputs, so the measurement is asked again.
    dialogOpenMock.mockResolvedValueOnce("E:\\more-games");
    await userEvent.click(screen.getByTestId("card-row-add-Games"));

    await waitFor(() => expect(measureMock).toHaveBeenCalledTimes(2));
    const size = screen.getByTestId("card-row-size-Games");
    expect(size.textContent).toBe(i18n.t("cardSection.measuring"));
    expect(size.textContent).not.toContain("4.9");
    expect(screen.getByTestId("card-total").textContent).not.toContain("55.7");
    // **Every figure, not only the two obvious ones.** The free line and the
    // heading's cost are drawn from the same measurement, and a stale number
    // in a caption is as wrong as a stale number in a row: the whole section
    // carries nothing from the measurement that has just been superseded.
    const section = screen.getByTestId("card-section");
    expect(section.textContent).not.toContain("4.9");
    expect(section.textContent).not.toContain("55.7");
    expect(section.textContent).not.toContain("60.8");
    expect(screen.queryByTestId("card-free")).toBeNull();
    expect(screen.queryByTestId("card-driver")).toBeNull();
    expect(screen.getByTestId("card-heading-cost").textContent).toBe(
      i18n.t("cardSection.headingCostPending")
    );
  });

  it("asks again when a source is removed", async () => {
    seedCard([
      { name: "System", sources: [] },
      { name: "Games", sources: ["E:\\games", "E:\\more"] },
      { name: "Work", sources: [] },
    ]);
    render(<CardSection />);
    await waitFor(() => expect(measureMock).toHaveBeenCalledTimes(1));

    await userEvent.click(await screen.findByTestId("card-source-remove-Games-0"));
    await waitFor(() => expect(measureMock).toHaveBeenCalledTimes(2));
    const asked = measureMock.mock.calls.at(-1)![0] as { partitions: { sources: string[] }[] };
    expect(asked.partitions[0].sources).toEqual(["E:\\more"]);
  });

  /** The request carries what the core adds itself — nothing else. */
  it("asks about the user's own partitions only", async () => {
    seedCard([
      { name: "System", sources: [] },
      { name: "Games", sources: ["E:\\games"] },
      { name: "Work", sources: [] },
    ]);
    render(<CardSection />);
    await waitFor(() => expect(measureMock).toHaveBeenCalled());
    const asked = measureMock.mock.calls.at(-1)![0] as {
      cardGb: number;
      tree: string;
      partitions: { volumeName: string }[];
      material: string[];
    };
    expect(asked.partitions.map((p) => p.volumeName)).toEqual(["Games"]);
    expect(asked.tree).toBe(TREE);
    expect(asked.cardGb).toBe(64);
    expect(asked.material).toEqual(["E:\\media"]);
  });

  /**
   * **An overflow names the partition and the bytes** (design § 4): "does not
   * fit" alone tells a user to guess which of their partitions to cut.
   */
  it("turns the total line red and names the partition when it does not fit", async () => {
    seedCard([
      { name: "System", sources: [] },
      { name: "Games", sources: ["E:\\games"] },
      { name: "Work", sources: [] },
    ]);
    render(<CardSection />);
    await deliver({
      measured: null,
      refusal: {
        code: "ART-CARD-DOES-NOT-FIT",
        message: "the card does not fit",
        params: {
          kind: "does-not-fit",
          needed: "65000000000",
          available: "60000000000",
          largest: "Games",
        },
      },
    });

    const overflow = await screen.findByTestId("card-overflow");
    expect(overflow.textContent).toContain("Games");
    expect(overflow.textContent).toContain("65");
    expect(screen.getByTestId("card-total").className).toContain("err");
  });

  it("says the sizes wait on the tree rather than asking for a measurement it cannot make", async () => {
    seedRemembered({
      "buildSession.release": RELEASE,
      [`osinstall.cardTarget.${RELEASE}`]: {
        sizeGb: 64,
        image: null,
        emu68Archive: null,
        pfs3Driver: null,
        partitions: [
          { name: "System", sources: [] },
          { name: "Work", sources: [] },
        ],
      },
    });
    render(<CardSection />);
    expect((await screen.findByTestId("card-total")).textContent).toBe(
      i18n.t("cardSection.needTree")
    );
    expect(measureMock).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// The three rows above the partitions (design § 3's own sketch), added on the
// controller's ruling after task 8's first commit: the image file the card is
// written into, the user's Emu68 archive, and the PFS3 driver line.
// ---------------------------------------------------------------------------

describe("the image file the card is written into", () => {
  it("says the build cannot start without one, and stops saying it once there is one", async () => {
    seedCard([
      { name: "System", sources: [] },
      { name: "Work", sources: [] },
    ]);
    render(<CardSection />);
    expect((await screen.findByTestId("card-image-blocker")).textContent).toBe(
      i18n.t("cardSection.image.blocker")
    );

    dialogSaveMock.mockResolvedValueOnce("E:\\amiga\\Kartlar\\amiga39.img");
    await userEvent.click(
      within(screen.getByTestId("card-image-field")).getByRole("button", {
        name: new RegExp(i18n.t("common.browse"), "i"),
      })
    );

    await waitFor(() => {
      const target = rememberedBag()[`osinstall.cardTarget.${RELEASE}`] as { image: string | null };
      expect(target.image).toBe("E:\\amiga\\Kartlar\\amiga39.img");
    });
    await waitFor(() => expect(screen.queryByTestId("card-image-blocker")).toBeNull());
    expect(screen.getByTestId("card-image-field").textContent).toContain("amiga39.img");
  });
});

describe("the Emu68 archive", () => {
  /**
   * **The same value the card builder already remembers.** Asking for it
   * twice is how two screens come to build two different cards; this reads
   * that key and never writes it (ART-089: a late read may not overwrite a
   * key the user touched).
   */
  it("shows the one the card builder remembers when the card target has none", async () => {
    seedRemembered({
      "buildSession.release": RELEASE,
      "cardBuilder.archive": "E:\\amiga\\Emu68-pistorm-20260101.zip",
      [`osinstall.cardTarget.${RELEASE}`]: {
        sizeGb: 64,
        image: null,
        emu68Archive: null,
        pfs3Driver: null,
        partitions: [
          { name: "System", sources: [] },
          { name: "Work", sources: [] },
        ],
      },
    });
    render(<CardSection />);
    expect((await screen.findByTestId("card-emu68-field")).textContent).toContain(
      "Emu68-pistorm-20260101.zip"
    );
  });

  it("stores a chosen archive on the card target and leaves the builder's key alone", async () => {
    seedRemembered({
      "buildSession.release": RELEASE,
      "cardBuilder.archive": "E:\\amiga\\Emu68-old.zip",
      [`osinstall.cardTarget.${RELEASE}`]: {
        sizeGb: 64,
        image: null,
        emu68Archive: null,
        pfs3Driver: null,
        partitions: [
          { name: "System", sources: [] },
          { name: "Work", sources: [] },
        ],
      },
    });
    render(<CardSection />);
    dialogOpenMock.mockResolvedValueOnce("E:\\amiga\\Emu68-new.zip");
    await userEvent.click(
      within(await screen.findByTestId("card-emu68-field")).getByRole("button", {
        name: new RegExp(i18n.t("common.browse"), "i"),
      })
    );

    await waitFor(() => {
      const target = rememberedBag()[`osinstall.cardTarget.${RELEASE}`] as {
        emu68Archive: string | null;
      };
      expect(target.emu68Archive).toBe("E:\\amiga\\Emu68-new.zip");
    });
    expect(rememberedBag()["cardBuilder.archive"]).toBe("E:\\amiga\\Emu68-old.zip");
  });
});

describe("the PFS3 driver", () => {
  it("names the driver the measurement found and where it came from", async () => {
    seedCard([
      { name: "System", sources: [] },
      { name: "Work", sources: [] },
    ]);
    render(<CardSection />);
    await deliver({ measured: measuredCard([["System", 838_860_800]]), refusal: null });

    const line = await screen.findByTestId("card-driver");
    await waitFor(() => expect(line.textContent).toContain("19.2"));
    expect(line.textContent).toContain("pfs3aio.lha");
    expect(screen.queryByTestId("card-driver-missing")).toBeNull();
  });

  /** A refusal must be actionable: what to add, and everywhere ART looked. */
  it("says what to add and where it looked when the measurement found none", async () => {
    seedCard([
      { name: "System", sources: [] },
      { name: "Work", sources: [] },
    ]);
    render(<CardSection />);
    await deliver({
      measured: null,
      refusal: {
        code: "ART-PFS3-DRIVER-NOT-FOUND",
        message: "No PFS3 driver was found.",
        params: { searched: "E:\\media, E:\\paketler", unreadable: "" },
      },
    });

    const missing = await screen.findByTestId("card-driver-missing");
    expect(missing.textContent).toContain("E:\\paketler");
    expect(missing.textContent).toContain("pfs3aio");
    // Not the generic refusal strip: this one has its own sentence.
    expect(screen.queryByTestId("card-refusal")).toBeNull();
    expect(screen.queryByTestId("card-driver")).toBeNull();
  });

  /** § 47: the explicit override is a power-user row on the same screen —
   *  hidden in beginner mode, never disabled. */
  it("offers the explicit driver override in power mode only", async () => {
    seedCard([
      { name: "System", sources: [] },
      { name: "Work", sources: [] },
    ]);
    render(<CardSection />);
    await screen.findByTestId("card-row-System");
    expect(screen.queryByTestId("card-pfs3-field")).toBeNull();

    cleanup();
    useSettingsStore.setState({
      loaded: true,
      settings: {
        ...useSettingsStore.getState().settings,
        uxMode: "power",
      },
    });
    render(<CardSection />);
    const field = await screen.findByTestId("card-pfs3-field");
    expect(
      (
        within(field).getByRole("button", {
          name: new RegExp(i18n.t("common.browse"), "i"),
        }) as HTMLButtonElement
      ).disabled
    ).toBe(false);
  });
});

describe("the section reads in Turkish too", () => {
  it("renders no raw key and no unfilled placeholder", async () => {
    await changeLanguage("tr");
    seedCard([
      { name: "System", sources: [] },
      { name: "Games", sources: ["E:\\games"] },
      { name: "Work", sources: [] },
    ]);
    render(<CardSection />);
    await deliver({
      measured: measuredCard([
        ["System", 838_860_800],
        ["Games", 4_900_000_000],
        ["Work", 50_000_000_000],
      ]),
      refusal: null,
    });
    const section = await screen.findByTestId("card-section");
    await waitFor(() => expect(section.textContent).toContain("4.9"));
    expect(section.textContent).not.toMatch(/cardSection\./);
    expect(section.textContent).not.toContain("{{");
  });
});
