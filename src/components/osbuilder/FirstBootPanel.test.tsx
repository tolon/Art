// @vitest-environment jsdom
//
// The OS Builder step that writes first boot into a distribution tree and
// lets a person prove it under WinUAE before ever touching a card (Task 9 of
// the first-boot round). Mocked at the `@/lib/*` wrapper boundary, the same
// seam `AmigaInstallPanel.test.tsx` uses.
//
// **The four rehearsal endings are the whole point of this file's middle
// section.** Succeeded, a step refused, nobody answered, the window was
// closed — four different sentences and four different next steps
// (CLAUDE.md, "Endings stay distinct"). Every assertion below is against
// rendered text, never a raw catalogue key, and the four-endings tests
// additionally check that no other ending's sentence is on screen — which
// nothing rendering nothing, and nothing collapsing two endings, could pass.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";

// Side-effecting: a real, synchronously-initialised i18next instance.
import "@/i18n";
import { useSettingsStore } from "@/stores/settingsStore";
import {
  rehearsalOutcomePhrase,
  type FirstBootPlan,
  type FirstBootReport,
  type FirstBootWritten,
  type RehearsalOutcome,
  type RehearsalResult,
} from "@/lib/firstboot";
import { JOB_CANCELLED_MESSAGE } from "@/lib/jobs";

const previewMock = vi.hoisted(() => vi.fn());
const writeMock = vi.hoisted(() => vi.fn());
const rehearseMock = vi.hoisted(() => vi.fn());
const awaitJobResultMock = vi.hoisted(() => vi.fn());
const jobCancelMock = vi.hoisted(() => vi.fn());
const saveSettingsMock = vi.hoisted(() => vi.fn(async () => {}));

vi.mock("@/lib/firstboot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/firstboot")>()),
  firstbootPreview: previewMock,
  firstbootWrite: writeMock,
  firstbootRehearse: rehearseMock,
}));

// `awaitJobResult` and `jobCancel` are the two pieces of `@/lib/jobs` this
// panel drives directly; `isJobCancellation` stays real — it is a pure
// predicate and the whole point of test 7 below is that the panel's own use
// of it is correct.
vi.mock("@/lib/jobs", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/jobs")>()),
  awaitJobResult: awaitJobResultMock,
  jobCancel: jobCancelMock,
}));

// `useRemembered`/`useRememberedShape` write through `useSettingsStore.update()`,
// which calls the real `tauri-plugin-store` IPC on every tick — an unhandled
// rejection in jsdom otherwise (see `FilesTab.test.tsx`'s own note).
vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  saveSettings: saveSettingsMock,
  getSettings: vi.fn(async () => ({})),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

const { FirstBootPanel } = await import("@/components/osbuilder/FirstBootPanel");

function plan(over: Partial<FirstBootPlan> = {}): FirstBootPlan {
  return {
    tree: "D:/amiga/os39",
    steps: [
      { name: "10-hardware", treePath: "S/FirstBoot/10-hardware", fixed: true },
      { name: "20-aux", treePath: "S/FirstBoot/20-aux", fixed: true },
      { name: "30-datatypes", treePath: "S/FirstBoot/30-datatypes", fixed: true },
    ],
    fatMount: { kind: "unavailable", needs: "fat95" },
    userStartupExists: false,
    alreadyWritten: false,
    bytesAdded: 4096,
    ...over,
  };
}

function written(over: Partial<FirstBootWritten> = {}): FirstBootWritten {
  return {
    files: ["S/ART-FirstBoot", "S/FirstBoot/10-hardware"],
    userStartupBackup: null,
    userStartupCreated: false,
    ...over,
  };
}

const REPORT: FirstBootReport = {
  version: 1,
  system: null,
  steps: [],
  ending: "done-all",
  fatCopyFailed: false,
  rebootRequestedBy: null,
  unknown: [],
};

let resolveRehearsal: ((value: RehearsalResult) => void) | null = null;
let rejectRehearsal: ((error: unknown) => void) | null = null;

beforeEach(() => {
  vi.clearAllMocks();
  resolveRehearsal = null;
  rejectRehearsal = null;
  useSettingsStore.setState((state) => ({
    settings: {
      ...state.settings,
      uxMode: "beginner",
      winuaePath: "C:/Program Files/WinUAE/winuae64.exe",
      remembered: { "buildSession.rom": { path: "D:/roms/kick31.rom" } },
    },
  }));
  previewMock.mockResolvedValue(plan());
  writeMock.mockResolvedValue(written());
  rehearseMock.mockResolvedValue(42);
  // `awaitJobResult`'s own contract: it calls `start` itself and hands back a
  // promise this test settles by hand, the way `AmigaInstallPanel.test.tsx`'s
  // `deliver`/`report` do for its own job channel.
  awaitJobResultMock.mockImplementation((_event: string, start: () => Promise<number>) => {
    void start();
    return new Promise<RehearsalResult>((resolve, reject) => {
      resolveRehearsal = resolve;
      rejectRehearsal = reject;
    });
  });
});

afterEach(() => {
  cleanup();
});

/** Render, wait for the preview, and press Rehearse. */
async function runRehearsal() {
  render(<FirstBootPanel treeRoot="D:/amiga/os39" onTreeRootChange={() => {}} />);
  await screen.findByTestId("firstboot-preview");
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: i18n.t("firstboot.panel.run") }));
  await waitFor(() => expect(rehearseMock).toHaveBeenCalled());
}

describe("what first boot will write", () => {
  it("names the three fixed steps", async () => {
    render(<FirstBootPanel treeRoot="D:/amiga/os39" onTreeRootChange={() => {}} />);
    const rows = await screen.findAllByTestId("firstboot-step-row");
    expect(rows).toHaveLength(3);
    const names = rows.map((row) => row.textContent ?? "");
    expect(names.some((t) => t.includes("10-hardware"))).toBe(true);
    expect(names.some((t) => t.includes("20-aux"))).toBe(true);
    expect(names.some((t) => t.includes("30-datatypes"))).toBe(true);
  });

  it("says the FAT partition will not mount, naming fat95", async () => {
    render(<FirstBootPanel treeRoot="D:/amiga/os39" onTreeRootChange={() => {}} />);
    const said = await screen.findByTestId("firstboot-fat-mount");
    expect(said.textContent).toContain("fat95");
  });

  it("says the FAT partition will mount when the tree carries the driver", async () => {
    previewMock.mockResolvedValue(plan({ fatMount: { kind: "available" } }));
    render(<FirstBootPanel treeRoot="D:/amiga/os39" onTreeRootChange={() => {}} />);
    const said = await screen.findByTestId("firstboot-fat-mount");
    expect(said.textContent).toBe(i18n.t("firstboot.fat.available"));
  });
});

describe("a tree that already carries a first-boot block", () => {
  it("says so, and offers to write again rather than write", async () => {
    previewMock.mockResolvedValue(plan({ alreadyWritten: true }));
    render(<FirstBootPanel treeRoot="D:/amiga/os39" onTreeRootChange={() => {}} />);
    await screen.findByTestId("firstboot-already-written");
    expect(
      screen.getByRole("button", { name: i18n.t("firstboot.panel.writeAgain") })
    ).toBeTruthy();
  });

  it("offers to write, not write again, on a tree that has never carried one", async () => {
    render(<FirstBootPanel treeRoot="D:/amiga/os39" onTreeRootChange={() => {}} />);
    await screen.findByTestId("firstboot-preview");
    expect(screen.queryByTestId("firstboot-already-written")).toBeNull();
    expect(screen.getByRole("button", { name: i18n.t("firstboot.panel.write") })).toBeTruthy();
  });
});

describe("pressing Write", () => {
  it("calls firstbootWrite once with the tree, and shows the backup it made", async () => {
    writeMock.mockResolvedValue(
      written({ userStartupBackup: "S/User-Startup.bak", userStartupCreated: false })
    );
    render(<FirstBootPanel treeRoot="D:/amiga/os39" onTreeRootChange={() => {}} />);
    await screen.findByTestId("firstboot-preview");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: i18n.t("firstboot.panel.write") }));

    await waitFor(() => expect(writeMock).toHaveBeenCalledTimes(1));
    expect(writeMock).toHaveBeenCalledWith("D:/amiga/os39");

    const backup = await screen.findByTestId("firstboot-write-backup");
    expect(backup.textContent).toContain("S/User-Startup.bak");
  });

  it("says User-Startup was created when there was none to back up", async () => {
    writeMock.mockResolvedValue(
      written({ userStartupBackup: null, userStartupCreated: true })
    );
    render(<FirstBootPanel treeRoot="D:/amiga/os39" onTreeRootChange={() => {}} />);
    await screen.findByTestId("firstboot-preview");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: i18n.t("firstboot.panel.write") }));

    const created = await screen.findByTestId("firstboot-write-created");
    expect(created.textContent).toBe(i18n.t("firstboot.panel.created"));
  });

  // Leftover round: the post-write `firstbootPreview(treeRoot).then(setPreview)`
  // in `runWrite` used to carry no guard at all -- unlike the mount effect's
  // own preview fetch, which drops a stale answer via its `cancelled` flag.
  // Write on tree A, switch to tree B before A's refresh resolves, and A's
  // stale answer must not land under B's own fresh preview.
  it("does not let tree A's post-write preview refresh land under tree B", async () => {
    let resolveStaleRefresh: ((value: FirstBootPlan) => void) | null = null;
    let calls = 0;
    previewMock.mockImplementation((tree: string) => {
      calls += 1;
      if (calls === 2) {
        // The post-write refresh for tree A, held open on purpose so the
        // tree switch below happens before it resolves.
        return new Promise<FirstBootPlan>((resolve) => {
          resolveStaleRefresh = resolve;
        });
      }
      return Promise.resolve(plan({ tree, alreadyWritten: false }));
    });

    const { rerender } = render(
      <FirstBootPanel treeRoot="D:/amiga/os39" onTreeRootChange={() => {}} />
    );
    await screen.findByTestId("firstboot-preview"); // call 1: tree A's own mount preview
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: i18n.t("firstboot.panel.write") }));
    await waitFor(() => expect(writeMock).toHaveBeenCalledTimes(1));
    // The post-write refresh (call 2) has been asked for, and is pending.
    await waitFor(() => expect(calls).toBe(2));

    // The user picks a different folder before tree A's refresh answers.
    rerender(<FirstBootPanel treeRoot="D:/amiga/os40" onTreeRootChange={() => {}} />);
    await waitFor(() => expect(previewMock).toHaveBeenLastCalledWith("D:/amiga/os40"));
    expect(screen.queryByTestId("firstboot-already-written")).toBeNull();

    // Tree A's stale refresh answers only now, claiming "already written" --
    // the one field on screen that distinguishes it from tree B's own
    // preview, which never set that flag.
    await act(async () => {
      resolveStaleRefresh!(plan({ tree: "D:/amiga/os39", alreadyWritten: true, bytesAdded: 999 }));
      await Promise.resolve();
      await Promise.resolve();
    });

    // Still tree B's preview: the stale "already written" answer never rendered.
    expect(screen.queryByTestId("firstboot-already-written")).toBeNull();
  });
});

describe("the four rehearsal endings stay four sentences on screen", () => {
  const OUTCOMES: RehearsalOutcome[] = [
    { kind: "finished", report: REPORT },
    { kind: "step-refused", report: REPORT },
    { kind: "timed-out", waited: { secs: 900, nanos: 0 }, report: REPORT },
    { kind: "emulator-closed", waited: { secs: 30, nanos: 0 }, report: REPORT },
  ];

  for (const outcome of OUTCOMES) {
    it(`says its own sentence for '${outcome.kind}' and none of the other three`, async () => {
      await runRehearsal();
      resolveRehearsal!({
        job_id: 42,
        outcome,
        copy: "D:/amiga/os39.rehearsal",
        discarded: outcome.kind === "finished",
      });

      const said = rehearsalOutcomePhrase(outcome);
      const shownOutcome = await screen.findByTestId("firstboot-rehearsal-outcome");
      expect(shownOutcome.textContent).toBe(i18n.t(said.key, said.params));

      const card = screen.getByTestId("firstboot-rehearsal-report");
      for (const other of OUTCOMES) {
        if (other.kind === outcome.kind) continue;
        const otherSaid = rehearsalOutcomePhrase(other);
        expect(card.textContent).not.toContain(i18n.t(otherSaid.key, otherSaid.params));
      }
    });
  }

  it("names the copy when the rehearsal did not discard it", async () => {
    await runRehearsal();
    resolveRehearsal!({
      job_id: 42,
      outcome: { kind: "step-refused", report: REPORT },
      copy: "D:/amiga/os39.rehearsal",
      discarded: false,
    });
    const copy = await screen.findByTestId("firstboot-rehearsal-copy");
    expect(copy.textContent).toContain("D:/amiga/os39.rehearsal");
  });

  // C1: the Rust side deliberately reports `discarded: false` when the
  // discard itself failed after a *finished* rehearsal — the one ending
  // whose own sentence used to claim the discard regardless of what the
  // core reported. This fixture is the case the vacuous one above never
  // produced: `finished` paired with `discarded: false`.
  it("does not claim a discard for a finished rehearsal when the core kept the copy", async () => {
    await runRehearsal();
    resolveRehearsal!({
      job_id: 42,
      outcome: { kind: "finished", report: REPORT },
      copy: "D:/amiga/os39.rehearsal",
      discarded: false,
    });
    const outcomeShown = await screen.findByTestId("firstboot-rehearsal-outcome");
    expect(outcomeShown.textContent).toBe(i18n.t("firstboot.rehearsal.outcome.finished"));
    expect(outcomeShown.textContent).not.toContain("discard");

    const copy = await screen.findByTestId("firstboot-rehearsal-copy");
    expect(copy.textContent).toBe(
      i18n.t("firstboot.panel.copyKept", { path: "D:/amiga/os39.rehearsal" })
    );
    expect(copy.textContent).not.toBe(i18n.t("firstboot.panel.copyDiscarded"));

    const card = screen.getByTestId("firstboot-rehearsal-report");
    expect(card.textContent).not.toContain(i18n.t("firstboot.panel.copyDiscarded"));
  });
});

describe("stopping a rehearsal", () => {
  it("cancels the job and says the rehearsal was stopped, not that it failed", async () => {
    await runRehearsal();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: i18n.t("firstboot.panel.stop") }));
    expect(jobCancelMock).toHaveBeenCalledWith(42);

    rejectRehearsal!(new Error("cancelled"));

    const cancelled = await screen.findByTestId("firstboot-rehearsal-cancelled");
    expect(cancelled.textContent).toBe(i18n.t("firstboot.panel.cancelled"));
    expect(screen.queryByTestId("firstboot-rehearsal-error")).toBeNull();
  });
});

describe("changing the tree clears a stale rehearsal", () => {
  // Fix round 1: a rehearsal's outcome, report and copy path describe the
  // tree that was on screen when it ran. Picking a different folder must not
  // leave that sentence sitting beside the new folder's fresh preview with
  // nothing saying it belongs elsewhere — CLAUDE.md's "the failure that does
  // not crash", here as "still true, just not about this folder any more".
  // The screen is put into a state (a finished rehearsal on screen) where
  // only the reset under test can make the sentence disappear.
  it("removes the previous tree's rehearsal outcome when the tree field changes", async () => {
    const { rerender } = render(
      <FirstBootPanel treeRoot="D:/amiga/os39" onTreeRootChange={() => {}} />
    );
    await screen.findByTestId("firstboot-preview");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: i18n.t("firstboot.panel.run") }));
    await waitFor(() => expect(rehearseMock).toHaveBeenCalled());
    resolveRehearsal!({
      job_id: 42,
      outcome: { kind: "finished", report: REPORT },
      copy: "D:/amiga/os39.rehearsal",
      discarded: true,
    });
    await screen.findByTestId("firstboot-rehearsal-outcome");
    // The copy line now renders for every outcome, discarded or not (C1), so
    // this assertion is real: it was on screen for tree A before the switch,
    // proving the absence below comes from clearing on the tree change and
    // not from a fixture that never rendered the element in the first place.
    expect(screen.getByTestId("firstboot-rehearsal-copy")).toBeTruthy();

    rerender(<FirstBootPanel treeRoot="D:/amiga/os40" onTreeRootChange={() => {}} />);

    await waitFor(() => expect(previewMock).toHaveBeenLastCalledWith("D:/amiga/os40"));
    expect(screen.queryByTestId("firstboot-rehearsal-outcome")).toBeNull();
    expect(screen.queryByTestId("firstboot-rehearsal-report")).toBeNull();
    expect(screen.queryByTestId("firstboot-rehearsal-copy")).toBeNull();
  });

  // I3, final review: the fix above (this describe block's first test) closes
  // the case where the tree changes *after* a result is already on screen.
  // It does not by itself close the case where the tree changes *while a
  // rehearsal is still in flight* — that result resolves later and must not
  // land under the new tree's fresh preview.
  it("drops a rehearsal's result if it resolves only after the tree has since changed", async () => {
    const { rerender } = render(
      <FirstBootPanel treeRoot="D:/amiga/os39" onTreeRootChange={() => {}} />
    );
    await screen.findByTestId("firstboot-preview");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: i18n.t("firstboot.panel.run") }));
    await waitFor(() => expect(rehearseMock).toHaveBeenCalled());

    // The user picks a different folder before tree A's rehearsal has answered.
    rerender(<FirstBootPanel treeRoot="D:/amiga/os40" onTreeRootChange={() => {}} />);
    await waitFor(() => expect(previewMock).toHaveBeenLastCalledWith("D:/amiga/os40"));
    // The button is usable again for tree B rather than stuck on "Rehearsing…"
    // for a job that belongs to tree A.
    expect(screen.getByRole("button", { name: i18n.t("firstboot.panel.run") })).toBeTruthy();

    // Tree A's rehearsal answers only now.
    await act(async () => {
      resolveRehearsal!({
        job_id: 42,
        outcome: { kind: "finished", report: REPORT },
        copy: "D:/amiga/os39.rehearsal",
        discarded: true,
      });
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(screen.queryByTestId("firstboot-rehearsal-outcome")).toBeNull();
    expect(screen.queryByTestId("firstboot-rehearsal-report")).toBeNull();
    expect(screen.queryByTestId("firstboot-rehearsal-copy")).toBeNull();
    // Still usable — a dropped stale result must not leave the button
    // disabled behind it either.
    expect(screen.getByRole("button", { name: i18n.t("firstboot.panel.run") })).toBeTruthy();
  });

  // Leftover round: the test above covers `runRehearsal`'s success branch
  // (`if (treeRootRef.current !== forTree) return;` before `setRehearsalResult`).
  // The same guard sits in the `catch` block, split two ways
  // (`isJobCancellation(e)` vs a genuine failure) — neither was exercised.
  it("drops a rehearsal's error if it rejects only after the tree has since changed", async () => {
    const { rerender } = render(
      <FirstBootPanel treeRoot="D:/amiga/os39" onTreeRootChange={() => {}} />
    );
    await screen.findByTestId("firstboot-preview");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: i18n.t("firstboot.panel.run") }));
    await waitFor(() => expect(rehearseMock).toHaveBeenCalled());

    // The user picks a different folder before tree A's rehearsal has answered.
    rerender(<FirstBootPanel treeRoot="D:/amiga/os40" onTreeRootChange={() => {}} />);
    await waitFor(() => expect(previewMock).toHaveBeenLastCalledWith("D:/amiga/os40"));
    expect(screen.getByRole("button", { name: i18n.t("firstboot.panel.run") })).toBeTruthy();

    // Tree A's rehearsal fails only now — a genuine error, not a cancellation.
    await act(async () => {
      rejectRehearsal!(new Error("winuae.exe could not be started"));
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(screen.queryByTestId("firstboot-rehearsal-error")).toBeNull();
    expect(screen.queryByTestId("firstboot-rehearsal-cancelled")).toBeNull();
    expect(screen.getByRole("button", { name: i18n.t("firstboot.panel.run") })).toBeTruthy();
  });

  it("drops a rehearsal's cancellation if it rejects only after the tree has since changed", async () => {
    const { rerender } = render(
      <FirstBootPanel treeRoot="D:/amiga/os39" onTreeRootChange={() => {}} />
    );
    await screen.findByTestId("firstboot-preview");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: i18n.t("firstboot.panel.run") }));
    await waitFor(() => expect(rehearseMock).toHaveBeenCalled());

    // The user picks a different folder before tree A's rehearsal has answered.
    rerender(<FirstBootPanel treeRoot="D:/amiga/os40" onTreeRootChange={() => {}} />);
    await waitFor(() => expect(previewMock).toHaveBeenLastCalledWith("D:/amiga/os40"));

    // Tree A's rehearsal is cancelled only now — `JOB_CANCELLED_MESSAGE`,
    // the exact string `isJobCancellation` matches on.
    await act(async () => {
      rejectRehearsal!(new Error(JOB_CANCELLED_MESSAGE));
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(screen.queryByTestId("firstboot-rehearsal-cancelled")).toBeNull();
    expect(screen.queryByTestId("firstboot-rehearsal-error")).toBeNull();
    expect(screen.getByRole("button", { name: i18n.t("firstboot.panel.run") })).toBeTruthy();
  });
});

describe("beginner mode hides the AmigaDOS path and never disables the write", () => {
  it("hides the tree-path column and keeps Write enabled", async () => {
    render(<FirstBootPanel treeRoot="D:/amiga/os39" onTreeRootChange={() => {}} />);
    const rows = await screen.findAllByTestId("firstboot-step-row");
    expect(rows[0].textContent).not.toContain("S/FirstBoot/10-hardware");
    expect(
      screen.getByRole("button", { name: i18n.t("firstboot.panel.write") }).hasAttribute("disabled")
    ).toBe(false);
  });

  it("shows the tree-path column in power mode", async () => {
    useSettingsStore.setState((state) => ({ settings: { ...state.settings, uxMode: "power" } }));
    render(<FirstBootPanel treeRoot="D:/amiga/os39" onTreeRootChange={() => {}} />);
    const rows = await screen.findAllByTestId("firstboot-step-row");
    expect(rows[0].textContent).toContain("S/FirstBoot/10-hardware");
  });
});
