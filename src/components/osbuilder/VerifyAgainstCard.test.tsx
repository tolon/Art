// @vitest-environment jsdom
//
// `VerifyAgainstCard` came out of `OsInstall.tsx` in ART-197 wave 2's row 3.
// A new component file with no test file is the survivor ART-223 disclosed and
// ART-229's round paid for, so it gets one in the same commit that creates it.
//
// Mocked at the `@/lib/*` boundary, the house pattern — never
// `@tauri-apps/api` itself. `@/lib/settings` is mocked one layer down for the
// reason `OsInstall.test.tsx` records: `useRemembered` writes through
// `useSettingsStore` and the real `saveSettings` rejects in jsdom with nothing
// to catch it, which Vitest counts as an unhandled rejection.
//
// What this establishes:
//
//   1. **"ART did not look" is not "ART found nothing wrong"** (§89). A report
//      with `failed: 0` and `notChecked: 3` must not read as verified. This is
//      the one that matters: it is the difference between a true sentence and
//      the confident wrong one this project pays most for, and it is now in a
//      file that can be edited without the install screen's 1 500 lines in
//      front of it.
//   2. The tree carry, both directions — ART writes a tree and this offers to
//      check it; a tree the user picked themselves is never moved under them.
//   3. Nothing runs until there is something to run against.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import "@/i18n";
import type { VerifyReport } from "@/lib/osinstall";
import { useSettingsStore } from "@/stores/settingsStore";

const verifyMock = vi.hoisted(() => vi.fn());
const dialogOpenMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallVerify: verifyMock,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: dialogOpenMock,
}));

vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  getSettings: vi.fn(),
  saveSettings: vi.fn().mockResolvedValue(undefined),
}));

const { VerifyAgainstCard } = await import("@/components/osbuilder/VerifyAgainstCard");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");

function seed(remembered: Record<string, unknown>) {
  useSettingsStore.setState({
    loaded: true,
    settings: { ...DEFAULT_SETTINGS, remembered },
  });
}

beforeEach(() => {
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
  verifyMock.mockReset();
  dialogOpenMock.mockReset().mockResolvedValue(null);
});

afterEach(() => {
  cleanup();
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
});

function report(over: Partial<VerifyReport> = {}): VerifyReport {
  return {
    passed: 10,
    failed: 0,
    notChecked: 0,
    files: [],
    ...over,
  } as VerifyReport;
}

/** Fill both fields and press the button. */
async function runAgainst(result: VerifyReport) {
  verifyMock.mockResolvedValue(result);
  seed({ "buildSession.tree": { root: "E:\\amiga\\dist-3.9", builtHere: true } });
  render(<VerifyAgainstCard />);
  await screen.findByText("E:\\amiga\\dist-3.9");

  dialogOpenMock.mockResolvedValue("E:\\amiga\\card.img");
  const browse = screen.getAllByRole("button", { name: /browse/i });
  await userEvent.click(browse[browse.length - 1]);
  await screen.findByText("E:\\amiga\\card.img");

  const run = screen.getByRole("button", { name: /^verify$/i });
  await waitFor(() => expect((run as HTMLButtonElement).disabled).toBe(false));
  await userEvent.click(run);
  await waitFor(() => expect(verifyMock).toHaveBeenCalled());
}

describe("ART did not look is not ART found nothing wrong (§89)", () => {
  // **The sentence, and the colour.** Asserting only on `.badge-ok` /
  // `.badge-warn` was the first version of these and it let a real defect
  // through: mutating the *text* to always read "Verified" while leaving the
  // class computed left the badge orange and the words wrong, and nothing
  // failed. A person reads the words.
  const VERDICT = {
    yes: "Verified — every file matched the manifest.",
    no: "Not verified — see the counts and the list below.",
  };

  it("calls a clean report verified", async () => {
    await runAgainst(report({ passed: 10, failed: 0, notChecked: 0 }));
    expect(document.body.textContent).toContain(VERDICT.yes);
    expect(document.body.textContent).not.toContain(VERDICT.no);
    expect(document.querySelector(".badge-ok")).not.toBeNull();
  });

  it("refuses to call a report with unchecked files verified", async () => {
    // **The whole point of the section.** `failed: 0` alone is not a pass:
    // three files ART never compared are three files nobody has checked, and
    // a green tick over them is a sentence ART cannot stand behind.
    await runAgainst(report({ passed: 7, failed: 0, notChecked: 3 }));
    expect(document.body.textContent).toContain(VERDICT.no);
    expect(document.body.textContent).not.toContain(VERDICT.yes);
    expect(document.querySelector(".badge-warn")).not.toBeNull();
  });

  it("does not call a report with failures verified either", async () => {
    await runAgainst(report({ passed: 7, failed: 3, notChecked: 0 }));
    expect(document.body.textContent).toContain(VERDICT.no);
    expect(document.body.textContent).not.toContain(VERDICT.yes);
  });

  it("says all three counts, so the summary cannot hide the unchecked", async () => {
    await runAgainst(report({ passed: 7, failed: 1, notChecked: 3 }));
    const text = document.body.textContent ?? "";
    // **The whole sentence.** `toContain("3")` was the first version and it
    // passed on the path `E:\amiga\dist-3.9` alone — the same trap the card
    // builder's tests were caught in on the same day.
    expect(text).toContain("7 passed, 1 failed, 3 not checked.");
    expect(text).not.toMatch(/osinstall\.verify/);
    expect(text).not.toMatch(/\{\{[^}]+\}\}/);
  });
});

describe("the tree carry (ART-197)", () => {
  it("offers the tree the session is already holding", async () => {
    seed({ "buildSession.tree": { root: "E:\\amiga\\dist-3.9", builtHere: true } });
    render(<VerifyAgainstCard />);
    // Before the split this field was filled only by `OsInstall`'s own result
    // handler, so it knew a tree this run had built and nothing about one the
    // user had picked. The session covers both.
    expect(await screen.findByText("E:\\amiga\\dist-3.9")).toBeTruthy();
  });

  it("never moves a tree the user picked here themselves", async () => {
    render(<VerifyAgainstCard />);
    dialogOpenMock.mockResolvedValue("E:\\amiga\\my-own-tree");
    await userEvent.click(screen.getAllByRole("button", { name: /browse/i })[0]);
    await screen.findByText("E:\\amiga\\my-own-tree");

    // The session then changes underneath — another step builds a tree.
    act(() => {
      seed({ "buildSession.tree": { root: "E:\\amiga\\built-later", builtHere: true } });
    });

    // A field that rewrites itself under somebody is the one outcome the
    // remembered-settings rule forbids outright, and it does not stop being
    // that because the value was never persisted.
    await waitFor(() => {
      expect(screen.queryByText("E:\\amiga\\built-later")).toBeNull();
    });
    expect(screen.getByText("E:\\amiga\\my-own-tree")).toBeTruthy();
  });
});

describe("nothing runs against nothing", () => {
  it("keeps Verify disabled until both a tree and a card are named", async () => {
    render(<VerifyAgainstCard />);
    const run = await screen.findByRole("button", { name: /^verify$/i });
    expect((run as HTMLButtonElement).disabled).toBe(true);
    // And says so on screen rather than leaving a dead button.
    expect(document.body.textContent).toContain(
      "Choose the distribution tree, the image and a partition index first."
    );

    seed({ "buildSession.tree": { root: "E:\\amiga\\dist-3.9", builtHere: true } });
    await screen.findByText("E:\\amiga\\dist-3.9");
    // A tree alone is still not enough — there is nothing to compare it with.
    expect((screen.getByRole("button", { name: /^verify$/i }) as HTMLButtonElement).disabled).toBe(
      true
    );
    expect(verifyMock).not.toHaveBeenCalled();
  });
});
