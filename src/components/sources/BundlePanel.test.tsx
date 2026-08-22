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
import type { BundleDownloadResult, BundleSummary } from "@/lib/bundles";

const listMock = vi.hoisted(() => vi.fn());
const downloadMock = vi.hoisted(() => vi.fn());
const onResultMock = vi.hoisted(() => vi.fn());
const onJobProgressMock = vi.hoisted(() => vi.fn());

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

const { BundlePanel } = await import("@/components/sources/BundlePanel");

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

describe("a mirror-kind entry is rendered like a user-supplied one, honestly", () => {
  it("says ART cannot fetch it rather than offering a tick that cannot work", async () => {
    listMock.mockResolvedValue([IBROWSE_SET]);
    render(<BundlePanel />);
    await screen.findByText(i18n.t("bundles.heading"));

    const sentence = i18n.t("bundles.entry.userSupplied", {
      why: i18n.t("bundles.entry.reason.mirror"),
    });
    expect(await screen.findByText((text) => text.includes(sentence))).toBeTruthy();
  });

  it("a user-supplied entry gets its own sentence too", async () => {
    listMock.mockResolvedValue([AG_SET]);
    render(<BundlePanel />);
    await screen.findByText(i18n.t("bundles.heading"));

    const sentence = i18n.t("bundles.entry.userSupplied", {
      why: i18n.t("bundles.entry.reason.userSupplied"),
    });
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
