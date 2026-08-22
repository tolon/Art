// @vitest-environment jsdom
//
// Task 7 (package bundles — "the screen"). Mocked at the same boundary the
// rest of this project's component tests mock at — the `@/lib/*` wrappers
// around `invoke`/`listen`, never `@tauri-apps/api` itself (see
// `OsInstall.test.tsx`'s own note on this).
//
// **Six outcomes, not five.** Task 5's review found a Critical defect: a
// cold-cache download over an occupied library slot is `not-placed`, never
// `downloaded` — nobody established whether the file already there is the
// same one. `EntryOutcome` (`@/lib/bundles`) carries six variants and this
// screen must show all six as distinct sentences. A report that only ever
// shows five is the exact defect this file guards against.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";

import { changeLanguage } from "@/i18n";
import { useSettingsStore } from "@/stores/settingsStore";
import type { BundleDownloadResult, BundleSummary } from "@/lib/bundles";

const listMock = vi.hoisted(() => vi.fn());
const downloadMock = vi.hoisted(() => vi.fn());
const onResultMock = vi.hoisted(() => vi.fn());
const onJobProgressMock = vi.hoisted(() => vi.fn());
const saveSettingsMock = vi.hoisted(() => vi.fn(async () => {}));

vi.mock("@/lib/bundles", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/bundles")>()),
  bundlesList: listMock,
  bundlesDownload: downloadMock,
  onBundleDownloadResult: onResultMock,
}));

// No real Tauri IPC bridge exists in jsdom — `onJobProgress` would reject
// with nothing to catch it, which Vitest counts as an unhandled rejection
// (the same shape `OsInstall.test.tsx` mocks this same module for).
vi.mock("@/lib/jobs", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/jobs")>()),
  onJobProgress: onJobProgressMock,
}));

// Finding 9's `useRemembered` writes through `useSettingsStore.update()`,
// which calls the real `tauri-plugin-store` IPC on every tick — the same
// unhandled-rejection shape `AmigaInstallPanel.test.tsx` mocks this same
// module for.
vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  saveSettings: saveSettingsMock,
  getSettings: vi.fn(async () => ({})),
}));

const { BundlePanel } = await import("@/components/sources/BundlePanel");

beforeEach(() => {
  useSettingsStore.setState((state) => ({
    settings: { ...state.settings, remembered: {} },
  }));
});

afterEach(async () => {
  cleanup();
  await changeLanguage("en");
});

/** Hand back the panel's own `onBundleDownloadResult` listener, so a
 *  finished download can be announced the way the backend announces one. */
function captureAnnounce(): {
  current: ((r: BundleDownloadResult) => void) | null;
} {
  const held: { current: ((r: BundleDownloadResult) => void) | null } = {
    current: null,
  };
  onResultMock.mockImplementation((fn: (r: BundleDownloadResult) => void) => {
    held.current = fn;
    return Promise.resolve(() => {});
  });
  return held;
}

const PICASSO_SET: BundleSummary = {
  id: "grafik",
  order: 60,
  entries: [
    {
      id: "picasso96",
      name: "Picasso96",
      kind: "aminet",
      permission: {
        holder: "Individual Computers (Jens Schonfeld)",
        note: "shareware; the only legal purchase is from Individual Computers",
      },
      exclusiveGroup: null,
    },
    {
      id: "akgif",
      name: "akGIF",
      kind: "aminet",
      permission: null,
      exclusiveGroup: null,
    },
  ],
};

const ARSIV_SET: BundleSummary = {
  id: "arsiv",
  order: 10,
  entries: [
    { id: "lha", name: "LHA", kind: "aminet", permission: null, exclusiveGroup: null },
    { id: "lzx", name: "LZX", kind: "aminet", permission: null, exclusiveGroup: null },
  ],
};

const AG_SET: BundleSummary = {
  id: "ag",
  order: 50,
  entries: [
    {
      id: "tolunnet",
      name: "tolunnet",
      kind: "user-supplied",
      permission: null,
      exclusiveGroup: "tcp",
    },
    {
      id: "miamidx",
      name: "MiamiDX — Main",
      kind: "aminet",
      permission: null,
      exclusiveGroup: "tcp",
    },
  ],
};

const IBROWSE_SET: BundleSummary = {
  id: "ibrowse",
  order: 140,
  entries: [
    {
      id: "ibrowse",
      name: "iBrowse",
      kind: "mirror",
      permission: {
        holder: "iBrowse development team",
        note: "demo build",
      },
      exclusiveGroup: null,
    },
  ],
};

/** Finding 2's own example: 4/4 `github-release` entries, exactly like the
 *  shipped `emu68` set — a set ART cannot fetch a single entry from. */
const EMU68_SET: BundleSummary = {
  id: "emu68",
  order: 5,
  entries: [
    { id: "emu68", name: "Emu68 (PiStorm)", kind: "github-release", permission: null, exclusiveGroup: null },
    {
      id: "emu68-lite",
      name: "Emu68 (PiStorm32-lite)",
      kind: "github-release",
      permission: null,
      exclusiveGroup: null,
    },
    { id: "emu68tools", name: "Emu68 Tools", kind: "github-release", permission: null, exclusiveGroup: null },
    { id: "genet", name: "GENet.device", kind: "github-release", permission: null, exclusiveGroup: null },
  ],
};

/** One resolvable entry (`aminet`) plus one the catalogue names an
 *  `aminet-search` query for — the "latest version" kind, so it must be
 *  refused the same honest way `mirror`/`user-supplied`/`github-release`
 *  already are, with its own reason sentence. */
const AMISSL_SET: BundleSummary = {
  id: "temel",
  order: 40,
  entries: [
    { id: "mui", name: "MUI 3.8", kind: "aminet", permission: null, exclusiveGroup: null },
    {
      id: "amissl",
      name: "AmiSSL",
      kind: "aminet-search",
      permission: null,
      exclusiveGroup: null,
    },
  ],
};

const MIXED_REPORT: BundleDownloadResult = {
  job_id: 1,
  report: {
    entries: [
      {
        id: "a",
        name: "A",
        outcome: { outcome: "downloaded", bytes: 100, path: "p1" },
      },
      { id: "b", name: "B", outcome: { outcome: "already-have", path: "p2" } },
      {
        id: "c",
        name: "C",
        outcome: { outcome: "not-placed", existing: "p3" },
      },
      { id: "d", name: "D", outcome: { outcome: "refused", why: "no mirror configured" } },
      { id: "e", name: "E", outcome: { outcome: "failed", error: "boom" } },
      { id: "f", name: "F", outcome: { outcome: "skipped" } },
    ],
  },
};

beforeEach(() => {
  listMock.mockReset().mockResolvedValue([]);
  downloadMock.mockReset().mockResolvedValue(1);
  onResultMock.mockReset().mockResolvedValue(() => {});
  onJobProgressMock.mockReset().mockResolvedValue(() => {});
});

describe("the permission condition is said before the tick, not after", () => {
  it("says the permission condition before the tick, not after", async () => {
    listMock.mockResolvedValue([PICASSO_SET]);
    render(<BundlePanel />);
    const warning = await screen.findByTestId("bundle-permission-warning");
    expect(warning.textContent).toContain("Picasso96");

    // The tick is still there; the sentence is above it, not instead of it.
    const checkbox = screen.getByRole("checkbox", { name: /grafik/i });
    expect(checkbox).toBeTruthy();

    // "Above" is not just presence — the warning must precede the tick in
    // the document itself, which is what Step 6's mutation (rendering it
    // below instead) has to break for this test to catch it.
    const relation = warning.compareDocumentPosition(checkbox);
    expect(Boolean(relation & Node.DOCUMENT_POSITION_FOLLOWING)).toBe(true);
  });
});

describe("nothing is fetched until the user presses the button", () => {
  it("fetches nothing until the button is pressed", async () => {
    listMock.mockResolvedValue([ARSIV_SET]);
    render(<BundlePanel />);
    await screen.findByText(i18n.t("bundles.heading"));
    // Give the initial `bundlesList()` round trip a chance to settle too.
    await screen.findByRole("checkbox", { name: /arsiv/i });
    expect(downloadMock).not.toHaveBeenCalled();
  });
});

describe("the report shows six endings separately, never as five or fewer", () => {
  it("reports six endings separately, never collapsed into fewer", async () => {
    listMock.mockResolvedValue([]);
    const announce = captureAnnounce();
    render(<BundlePanel />);
    await screen.findByText(i18n.t("bundles.heading"));

    act(() => announce.current!(MIXED_REPORT));

    expect(
      await screen.findByText(i18n.t("bundles.result.downloaded", { count: 1 }))
    ).toBeTruthy();
    expect(
      screen.getByText(i18n.t("bundles.result.alreadyHave", { count: 1 }))
    ).toBeTruthy();
    expect(
      screen.getByText(i18n.t("bundles.result.notPlaced", { count: 1 }))
    ).toBeTruthy();
    expect(
      screen.getByText(i18n.t("bundles.result.refused", { count: 1 }))
    ).toBeTruthy();
    expect(
      screen.getByText(i18n.t("bundles.result.failed", { count: 1 }))
    ).toBeTruthy();
    expect(
      screen.getByText(i18n.t("bundles.result.skipped", { count: 1 }))
    ).toBeTruthy();

    // Six distinct sentences, not five sharing a value by accident.
    const sentences = new Set([
      i18n.t("bundles.result.downloaded", { count: 1 }),
      i18n.t("bundles.result.alreadyHave", { count: 1 }),
      i18n.t("bundles.result.notPlaced", { count: 1 }),
      i18n.t("bundles.result.refused", { count: 1 }),
      i18n.t("bundles.result.failed", { count: 1 }),
      i18n.t("bundles.result.skipped", { count: 1 }),
    ]);
    expect(sentences.size).toBe(6);
  });
});

// Second re-review, item 1: a mirror entry used to be wrapped in
// `bundles.entry.userSupplied` ("you supply it") the same as a genuinely
// user-supplied one -- which is false for `mirror`. ART has simply not been
// pointed at a mirror for it; that is not the user's to fix. This block
// asserts the two now render *different*, individually true sentences, not
// the same composed one (CLAUDE.md, "The failure that does not crash").
describe("a mirror-kind entry gets its own true sentence, distinct from user-supplied", () => {
  it("says no mirror is configured, never that the user must supply it", async () => {
    listMock.mockResolvedValue([IBROWSE_SET]);
    render(<BundlePanel />);
    await screen.findByText(i18n.t("bundles.heading"));

    const sentence = i18n.t("bundles.entry.mirror");
    expect(await screen.findByText((text) => text.includes(sentence))).toBeTruthy();
    // The false claim this item fixes: a mirror entry is not the user's to
    // supply, so "you supply it" must not appear anywhere for it.
    expect(screen.queryByText((text) => text.includes("you supply it"))).toBeNull();
  });

  it("a user-supplied entry keeps its own true 'you supply it' sentence", async () => {
    listMock.mockResolvedValue([AG_SET]);
    render(<BundlePanel />);
    await screen.findByText(i18n.t("bundles.heading"));

    const sentence = i18n.t("bundles.entry.userSupplied");
    expect(await screen.findByText((text) => text.includes(sentence))).toBeTruthy();
  });
});

describe("an exclusive_group is shown, never enforced", () => {
  it("says the two are alternatives without disabling either tick", async () => {
    listMock.mockResolvedValue([AG_SET]);
    render(<BundlePanel />);
    await screen.findByText(i18n.t("bundles.heading"));

    const note = i18n.t("bundles.entry.alternatives", {
      names: "tolunnet / MiamiDX — Main",
    });
    expect(await screen.findByText(note)).toBeTruthy();

    // The set's own tick — the only tick this screen offers — stays enabled.
    const checkbox = screen.getByRole("checkbox", { name: /ag/i });
    expect((checkbox as HTMLInputElement).disabled).toBe(false);
  });
});

describe("choosing sets and pressing the button", () => {
  it("downloads every entry of every ticked set, and refuses when nothing is ticked", async () => {
    listMock.mockResolvedValue([ARSIV_SET]);
    render(<BundlePanel />);
    const checkbox = await screen.findByRole("checkbox", { name: /arsiv/i });

    const runButton = screen.getByRole("button", { name: i18n.t("bundles.run") });
    await userEvent.click(runButton);
    expect(
      await screen.findByText(i18n.t("bundles.blocked.nothingChosen"))
    ).toBeTruthy();
    expect(downloadMock).not.toHaveBeenCalled();

    await userEvent.click(checkbox);
    await userEvent.click(runButton);

    expect(downloadMock).toHaveBeenCalledWith(["lha", "lzx"]);
  });
});

// Finding 1 of the final review: the panel used to render only the six
// count badges and threw away every string an outcome itself carries —
// which four entries ART could not fetch and why, which one failed and how,
// where the ones that worked landed. A count alone cannot answer any of
// that (CLAUDE.md: "a refusal must be actionable").
describe("the report names each entry, not just a count", () => {
  it("lists every entry with the string its own outcome carries", async () => {
    listMock.mockResolvedValue([]);
    const announce = captureAnnounce();
    render(<BundlePanel />);
    await screen.findByText(i18n.t("bundles.heading"));

    act(() => announce.current!(MIXED_REPORT));

    const detail = await screen.findByTestId("bundle-report-detail");
    // Downloaded — its path.
    expect(detail.textContent).toContain("A");
    expect(detail.textContent).toContain("p1");
    // AlreadyHave — its path.
    expect(detail.textContent).toContain("B");
    expect(detail.textContent).toContain("p2");
    // NotPlaced — what already occupied the slot.
    expect(detail.textContent).toContain("C");
    expect(detail.textContent).toContain("p3");
    // Refused — why, verbatim from the core.
    expect(detail.textContent).toContain("D");
    expect(detail.textContent).toContain("no mirror configured");
    // Failed — the error, verbatim.
    expect(detail.textContent).toContain("E");
    expect(detail.textContent).toContain("boom");
    // Skipped — named too, even though it carries no string of its own.
    expect(detail.textContent).toContain("F");
  });
});

// Finding 2 of the final review: `cannotFetch` used to answer only `mirror`
// and `user-supplied`, but `resolve.rs` refuses every `aminet-search` and
// every `github-release` unconditionally too. The shipped `emu68` set is
// 4/4 `github-release` — offering it a working-looking tick is offering
// what ART cannot do (§10/§89).
describe("github-release and aminet-search are unfetchable too, and say so honestly", () => {
  // Second re-review, item 1: this used to assert the composed
  // `bundles.entry.userSupplied` sentence ("you supply it") for a
  // `github-release` entry, which is false — ART simply has not built a
  // GitHub-release fetch path; that is not the user's to bring. It now
  // asserts the true, kind-specific sentence and that the false one is
  // nowhere on screen.
  it("a set that is entirely github-release names why, truthfully, and offers no working tick", async () => {
    listMock.mockResolvedValue([EMU68_SET]);
    render(<BundlePanel />);
    await screen.findByText(i18n.t("bundles.heading"));

    const sentence = i18n.t("bundles.entry.githubRelease");
    // All four entries share the same reason — one sentence, four times.
    expect((await screen.findAllByText((text) => text.includes(sentence))).length).toBe(4);
    // The false claim this item fixes: none of these is the user's to
    // supply, so "you supply it" must not appear anywhere for them.
    expect(screen.queryByText((text) => text.includes("you supply it"))).toBeNull();

    expect(
      await screen.findByText(i18n.t("bundles.fetchableCount", { fetchable: 0, total: 4 }))
    ).toBeTruthy();

    const checkbox = screen.getByRole("checkbox", { name: /emu68/i });
    expect((checkbox as HTMLInputElement).disabled).toBe(true);
  });

  it("an aminet-search entry gets its own true sentence, and a partly-fetchable set keeps its tick", async () => {
    listMock.mockResolvedValue([AMISSL_SET]);
    render(<BundlePanel />);
    await screen.findByText(i18n.t("bundles.heading"));

    const sentence = i18n.t("bundles.entry.aminetSearch");
    expect(await screen.findByText((text) => text.includes(sentence))).toBeTruthy();
    expect(screen.queryByText((text) => text.includes("you supply it"))).toBeNull();

    expect(
      await screen.findByText(i18n.t("bundles.fetchableCount", { fetchable: 1, total: 2 }))
    ).toBeTruthy();

    // One of the two is still fetchable (MUI), so the set's own tick must
    // stay enabled — only a *wholly* unfetchable set is offered no tick.
    const checkbox = screen.getByRole("checkbox", { name: /temel/i });
    expect((checkbox as HTMLInputElement).disabled).toBe(false);
  });

  it("a fully-fetchable set shows no fetchable-count line at all", async () => {
    listMock.mockResolvedValue([ARSIV_SET]);
    render(<BundlePanel />);
    const checkbox = await screen.findByRole("checkbox", { name: /arsiv/i });
    expect((checkbox as HTMLInputElement).disabled).toBe(false);
    expect(
      screen.queryByText(i18n.t("bundles.fetchableCount", { fetchable: 2, total: 2 }))
    ).toBeNull();
  });
});

// Finding 8 of the final review: the design specifies `hepsi` — "everything"
// — as a computed union, never listed as catalogue data, so it cannot drift
// from the 14 shipped sets. There was no control for it at all.
describe("a select-all control ticks every shipped set at once", () => {
  it("selects every set, and the run downloads every one of their entries", async () => {
    listMock.mockResolvedValue([ARSIV_SET, PICASSO_SET]);
    render(<BundlePanel />);
    const selectAll = await screen.findByRole("checkbox", { name: i18n.t("bundles.set.hepsi") });

    await userEvent.click(selectAll);

    expect((screen.getByRole("checkbox", { name: /arsiv/i }) as HTMLInputElement).checked).toBe(
      true
    );
    expect((screen.getByRole("checkbox", { name: /grafik/i }) as HTMLInputElement).checked).toBe(
      true
    );

    const runButton = screen.getByRole("button", { name: i18n.t("bundles.run") });
    await userEvent.click(runButton);
    expect(downloadMock).toHaveBeenCalledWith(["lha", "lzx", "picasso96", "akgif"]);
  });

  it("ticking it again clears every set", async () => {
    listMock.mockResolvedValue([ARSIV_SET, PICASSO_SET]);
    render(<BundlePanel />);
    const selectAll = await screen.findByRole("checkbox", { name: i18n.t("bundles.set.hepsi") });

    await userEvent.click(selectAll);
    await userEvent.click(selectAll);

    expect((screen.getByRole("checkbox", { name: /arsiv/i }) as HTMLInputElement).checked).toBe(
      false
    );
    expect((screen.getByRole("checkbox", { name: /grafik/i }) as HTMLInputElement).checked).toBe(
      false
    );
  });

  // Second re-review, item 3: `emu68`'s own card checkbox is individually
  // `disabled` (0/4 entries fetchable), so select-all must not tick it —
  // a control that ticks a box it has itself just greyed out is telling
  // the user two contradictory things at once. Select-all here ticks fewer
  // than all three shipped sets; that is the honest outcome, and nothing
  // on screen may claim it selected "everything" when it selected two.
  it("does not tick a set whose own checkbox is disabled because nothing in it is fetchable", async () => {
    listMock.mockResolvedValue([ARSIV_SET, PICASSO_SET, EMU68_SET]);
    render(<BundlePanel />);
    const selectAll = await screen.findByRole("checkbox", { name: i18n.t("bundles.set.hepsi") });
    const emu68Checkbox = screen.getByRole("checkbox", { name: /emu68/i }) as HTMLInputElement;
    expect(emu68Checkbox.disabled).toBe(true);

    await userEvent.click(selectAll);

    expect((screen.getByRole("checkbox", { name: /arsiv/i }) as HTMLInputElement).checked).toBe(
      true
    );
    expect((screen.getByRole("checkbox", { name: /grafik/i }) as HTMLInputElement).checked).toBe(
      true
    );
    // The disabled set stays unticked — checked-and-disabled at once is
    // exactly the contradiction this item fixes.
    expect(emu68Checkbox.checked).toBe(false);
    // The select-all control itself still reads as fully checked, because
    // every set it is honestly able to tick now is ticked — it does not
    // claim emu68 is selected too; that set's own checkbox, visibly
    // unchecked and disabled, is what tells the truth about emu68.
    expect((selectAll as HTMLInputElement).checked).toBe(true);

    // Running only downloads the two fetchable sets' entries — emu68 was
    // never ticked, so none of its entries are handed to the download call.
    const runButton = screen.getByRole("button", { name: i18n.t("bundles.run") });
    await userEvent.click(runButton);
    expect(downloadMock).toHaveBeenCalledWith(["lha", "lzx", "picasso96", "akgif"]);
  });
});

// Finding 9 of the final review: `chosen` lived in a plain `useState`, so
// navigating away and back — or simply closing and reopening ART — cleared
// every tick. The owner's own rule, stated for the whole product: nothing
// ART shows changes unless the user changes it ("ürünün tamamı için").
describe("the ticked sets are remembered, not forgotten on navigation", () => {
  it("a tick reaches the settings store", async () => {
    listMock.mockResolvedValue([ARSIV_SET]);
    render(<BundlePanel />);
    const checkbox = await screen.findByRole("checkbox", { name: /arsiv/i });

    await userEvent.click(checkbox);

    expect(useSettingsStore.getState().settings.remembered).toMatchObject({
      "bundles.chosenSets": ["arsiv"],
    });
  });

  it("a previously remembered tick is restored on a fresh mount", async () => {
    useSettingsStore.setState((state) => ({
      settings: { ...state.settings, remembered: { "bundles.chosenSets": ["arsiv"] } },
    }));
    listMock.mockResolvedValue([ARSIV_SET]);
    render(<BundlePanel />);

    const checkbox = await screen.findByRole("checkbox", { name: /arsiv/i });
    expect((checkbox as HTMLInputElement).checked).toBe(true);
  });

  it("a hand-edited, non-array value falls back to nothing chosen rather than crashing", async () => {
    useSettingsStore.setState((state) => ({
      settings: { ...state.settings, remembered: { "bundles.chosenSets": "arsiv" } },
    }));
    listMock.mockResolvedValue([ARSIV_SET]);
    render(<BundlePanel />);

    const checkbox = await screen.findByRole("checkbox", { name: /arsiv/i });
    expect((checkbox as HTMLInputElement).checked).toBe(false);
  });
});
