// @vitest-environment jsdom
//
// The screen that runs a package's own installer inside an emulator (Task 6
// of the Amiga-side install round). Mocked at the boundary the rest of this
// suite mocks at — the `@/lib/*` wrappers around `invoke`/`listen`, never
// `@tauri-apps/api` itself.
//
// **What these tests are actually for.** This round produced the same defect
// three times and never once as a crash: ART saying a confidently wrong
// sentence. So the assertions below are about *which sentence a person
// reads*, and every one of them is written against what is on screen rather
// than against a catalogue key — a test asserting `outcome.timedOut` would
// pass just as happily if the screen rendered that raw key to the user, which
// is one of the two frontend traps this task was warned about. The other is a
// test that would pass if the component rendered nothing at all; every case
// here therefore asserts on text that must be *present*, and the ones about
// the four endings additionally assert the other three endings' sentences are
// *absent*, which nothing rendering nothing can satisfy.
//
// **No emulator is opened.** `amigaInstallRun` is a mock that answers a job
// id, and the four endings arrive through the mocked result listener — the
// same seam `commands/amigainstall.rs` gave its own tests for exactly this
// reason.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";

// Side-effecting: gives `useTranslation` a real, synchronously-initialised
// instance, the way `OsInstall.test.tsx` and `PackagePanel.test.tsx` do.
import "@/i18n";
import { useSettingsStore } from "@/stores/settingsStore";
import type { AmigaInstallPreview, AmigaInstallResult, RunOutcome } from "@/lib/amigainstall";
import type { PackageSummary } from "@/lib/osinstall";

const previewMock = vi.hoisted(() => vi.fn());
const runMock = vi.hoisted(() => vi.fn());
const onResultMock = vi.hoisted(() => vi.fn());
const packagesMock = vi.hoisted(() => vi.fn());
const onJobProgressMock = vi.hoisted(() => vi.fn());
const saveSettingsMock = vi.hoisted(() => vi.fn(async () => {}));

vi.mock("@/lib/amigainstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/amigainstall")>()),
  amigaInstallPreview: previewMock,
  amigaInstallRun: runMock,
  onAmigaInstallResult: onResultMock,
}));

vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallPackages: packagesMock,
}));

vi.mock("@/lib/jobs", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/jobs")>()),
  onJobProgress: onJobProgressMock,
}));

// `useRemembered` writes through `useSettingsStore.update()`, which calls the
// real `tauri-plugin-store` IPC on every tick — an unhandled rejection in
// jsdom (see `OsInstall.test.tsx`'s own note).
vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  saveSettings: saveSettingsMock,
  getSettings: vi.fn(async () => ({})),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

const { AmigaInstallPanel } = await import("@/components/osbuilder/AmigaInstallPanel");

/** The two shipped BoingBags plus the one package that has no Amiga-side
 *  installer — the real catalogue's own shape, so the "only what a recipe
 *  declares is offered" assertion has something that must *not* appear. */
const PACKAGES: PackageSummary[] = [
  {
    id: "boingbag-39-1",
    name: "BoingBag 3.9-1",
    requires: [],
    requiresComponents: [],
    available: true,
    hostPlacementBlock: "encrypted-payload",
    amigaInstallable: true,
    refusedNames: [],
  },
  {
    id: "boingbag-39-2",
    name: "BoingBag 3.9-2",
    requires: ["boingbag-39-1"],
    requiresComponents: [],
    available: true,
    hostPlacementBlock: "encrypted-payload",
    amigaInstallable: true,
    refusedNames: [],
  },
  {
    id: "locale-turkish",
    name: "Türkçe catalogs (BoingBag 3.9-2)",
    requires: [],
    requiresComponents: ["locale-base"],
    available: true,
    hostPlacementBlock: null,
    amigaInstallable: false,
    refusedNames: [],
  },
];

function preview(over: Partial<AmigaInstallPreview> = {}): AmigaInstallPreview {
  return {
    packageId: "boingbag-39-1",
    packageName: "BoingBag 3.9-1",
    tree: "D:/amiga/os39",
    systemVolume: "DH0",
    workingDirectory: "ARTPkg:BoingBag3.9-1",
    program: "ARTPkg:BoingBag3.9-1/C/Updater",
    args: ["AmigaOS-Update", "DH0:"],
    workVolume: "ARTWork",
    packageVolume: "ARTPkg",
    packageArchives: ["D:/pkg/BoingBag39-1.lha"],
    packageArchivesPresent: true,
    declaredOverlays: ["BoingBag3.9-1-UAE/BoingBag3.9-1"],
    minimumInstallerVersion: "45.15",
    packageDir: "BoingBag3.9-1",
    resultFile: "art-result.txt",
    deadlineSeconds: 1800,
    kickstart: "D:/roms/kick31.rom",
    kickstartPresent: true,
    emulator: "C:/Program Files/WinUAE/winuae64.exe",
    profileId: "a1200-aga",
    profileName: "Amiga 1200 (AGA)",
    ...over,
  };
}

/** The choices `AmigaInstallPanel` remembers, put in place directly rather
 *  than driven through four file dialogs: the dialogs are the plugin's, not
 *  this screen's, and what these tests are about is what the screen *says*
 *  once a complete request exists. */
function withChoices() {
  useSettingsStore.setState((state) => ({
    settings: {
      ...state.settings,
      winuaePath: "C:/Program Files/WinUAE/winuae64.exe",
      // `Settings.remembered` is `unknown` by design — `@/lib/remembered`'s
      // guards are what give it a shape — so it is replaced whole here
      // rather than spread.
      remembered: {
        "amigaInstall.package": "boingbag-39-1",
        "amigaInstall.archive": "D:/pkg/BoingBag39-1.lha",
        "amigaInstall.kickstart": "D:/roms/kick31.rom",
      },
    },
  }));
}

/** The one live `onAmigaInstallResult` handler, so a test can deliver an
 *  ending the way the backend would. */
let deliver: ((result: AmigaInstallResult) => void) | null = null;

beforeEach(() => {
  vi.clearAllMocks();
  deliver = null;
  useSettingsStore.setState((state) => ({
    settings: { ...state.settings, uxMode: "beginner", winuaePath: null, remembered: {} },
  }));
  packagesMock.mockResolvedValue(PACKAGES);
  previewMock.mockResolvedValue(preview());
  runMock.mockResolvedValue(7);
  onJobProgressMock.mockResolvedValue(() => {});
  onResultMock.mockImplementation(async (handler: (r: AmigaInstallResult) => void) => {
    deliver = handler;
    return () => {};
  });
});

afterEach(() => {
  cleanup();
});

/** Render, wait for the preview, tick the confirmation and press Run. */
async function runToConfirmation() {
  withChoices();
  render(<AmigaInstallPanel treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);
  await screen.findByTestId("amiga-install-preview");
  const user = userEvent.setup();
  await user.click(screen.getByRole("checkbox"));
  await user.click(screen.getByRole("button", { name: i18n.t("osinstall.amigaInstall.run") }));
  await waitFor(() => expect(runMock).toHaveBeenCalled());
}

describe("before anything opens", () => {
  // "The emulator is a window on the owner's desktop. The confirmation says
  // so *before* it opens." An earlier round opened one repeatedly without
  // warning and that was a real annoyance.
  it("says an emulator window will open, before the run and in beginner mode", async () => {
    render(<AmigaInstallPanel treeRoot={null} packageFolder={null} />);
    const warning = screen.getByTestId("emulator-window-warning");
    expect(warning.textContent).toBe(i18n.t("osinstall.amigaInstall.emulatorWindow"));
    expect(warning.textContent).toMatch(/emulator window/i);
    expect(runMock).not.toHaveBeenCalled();
    expect(previewMock).not.toHaveBeenCalled();
  });

  it("says the tree is copied and only replaced on success", () => {
    render(<AmigaInstallPanel treeRoot={null} packageFolder={null} />);
    expect(screen.getByText(i18n.t("osinstall.amigaInstall.copyNote"))).toBeTruthy();
  });

  // The prerequisite chain is legible *before* the run, not only inside a
  // refusal — "refused" without the order is not a next step.
  it("names the order the packages go on in", () => {
    render(<AmigaInstallPanel treeRoot={null} packageFolder={null} />);
    const note = screen.getByText(i18n.t("osinstall.amigaInstall.chainNote"));
    expect(note.textContent).toMatch(/BoingBag 3\.9-1.*BoingBag 3\.9-2/s);
  });

  it("offers only the packages whose own recipe declares an Amiga-side installer", async () => {
    render(<AmigaInstallPanel treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);
    await waitFor(() => expect(screen.getAllByTestId("amiga-package-row")).toHaveLength(2));
    const rows = screen.getAllByTestId("amiga-package-row").map((row) => row.textContent ?? "");
    expect(rows[0]).toContain("BoingBag 3.9-1");
    expect(rows[1]).toContain("BoingBag 3.9-2");
    // `locale-turkish` has no `amiga_installer`; offering it would be a pick
    // `compose` refuses by name a moment later.
    expect(screen.queryByText(/Türkçe catalogs/)).toBeNull();
    // And BoingBag 2's own prerequisite is on its row, from the catalogue's
    // data rather than from a sentence written here.
    expect(
      screen.getByText(i18n.t("osinstall.packages.requiresPackages", { list: "BoingBag 3.9-1" }))
    ).toBeTruthy();
  });

  it("will not let the run be confirmed until a preview exists", () => {
    render(<AmigaInstallPanel treeRoot={null} packageFolder={null} />);
    expect(screen.getByRole("checkbox").hasAttribute("disabled")).toBe(true);
    expect(
      screen.getByRole("button", { name: i18n.t("osinstall.amigaInstall.run") }).hasAttribute("disabled")
    ).toBe(true);
  });
});

describe("a refusal says which reason applies", () => {
  // ART-060: the sentence is Rust's and is English. Replacing it with one
  // translated "it was refused" would lose the half that matters — *which*
  // reason, and what to do about it.
  it("shows the prerequisite refusal verbatim, naming what is missing and in what order", async () => {
    const said =
      "'BoingBag 3.9-2' has to go on after BoingBag 3.9-1, and 'D:/amiga/os39' does not have it " +
      "yet — install BoingBag 3.9-1 first, in that order.";
    previewMock.mockRejectedValue(said);
    withChoices();
    render(<AmigaInstallPanel treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);

    const refusal = await screen.findByTestId("amiga-install-refusal");
    expect(refusal.textContent).toContain("install BoingBag 3.9-1 first, in that order");
    // And the half ART *can* say in the user's own language: nothing was
    // copied. A refusal happens before the tree is touched at all.
    expect(refusal.textContent).toContain(i18n.t("osinstall.amigaInstall.refused.nothingCopied"));
    // Nothing offers a run for something that cannot happen.
    expect(screen.queryByTestId("amiga-install-preview")).toBeNull();
    expect(screen.getByRole("checkbox").hasAttribute("disabled")).toBe(true);
  });

  it("shows a too-old installer refusal verbatim, naming the archive that fixes it", async () => {
    previewMock.mockRejectedValue(
      "'D:/pkg/BoingBag39-1.lha' carries Updater 45.13, and this package's installer has to be " +
        "at least 45.15 to run inside an emulator. Supply the package's update archive as well " +
        "— the one carrying BoingBag3.9-1-UAE/BoingBag3.9-1"
    );
    withChoices();
    render(<AmigaInstallPanel treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);

    const refusal = await screen.findByTestId("amiga-install-refusal");
    expect(refusal.textContent).toContain("45.15");
    expect(refusal.textContent).toContain("BoingBag3.9-1-UAE/BoingBag3.9-1");
  });
});

describe("the second archive, before the refusal rather than after it (ART-186)", () => {
  it("names the version and the archive to go and find when only the wrapper is chosen", async () => {
    withChoices();
    render(<AmigaInstallPanel treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);

    const advice = await screen.findByTestId("amiga-install-overlay-advice");
    expect(advice.textContent).toContain("45.15");
    expect(advice.textContent).toContain("BoingBag3.9-1-UAE/BoingBag3.9-1");
    expect(advice.textContent).toBe(
      i18n.t("osinstall.amigaInstall.overlay.needed", {
        version: "45.15",
        overlays: "BoingBag3.9-1-UAE/BoingBag3.9-1",
      })
    );
  });

  it("says something else once that archive is supplied, and invents nothing for a package that declares no minimum", async () => {
    previewMock.mockResolvedValue(
      preview({ packageArchives: ["D:/pkg/a.lha", "D:/pkg/BoingBag39-1-UAE.lha"] })
    );
    withChoices();
    const { unmount } = render(<AmigaInstallPanel treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);
    let advice = await screen.findByTestId("amiga-install-overlay-advice");
    expect(advice.textContent).toBe(
      i18n.t("osinstall.amigaInstall.overlay.supplied", {
        version: "45.15",
        overlays: "BoingBag3.9-1-UAE/BoingBag3.9-1",
      })
    );
    unmount();

    previewMock.mockResolvedValue(
      preview({ minimumInstallerVersion: null, declaredOverlays: [], packageName: "BoingBag 3.9-2" })
    );
    render(<AmigaInstallPanel treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);
    await screen.findByTestId("amiga-install-preview");
    expect(screen.queryByTestId("amiga-install-overlay-advice")).toBeNull();
  });
});

describe("what a previewed run still lacks", () => {
  it("names each missing thing and refuses the confirmation until they are there", async () => {
    previewMock.mockResolvedValue(
      preview({ kickstartPresent: false, emulator: null, packageArchivesPresent: false })
    );
    withChoices();
    render(<AmigaInstallPanel treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);

    const blockers = await screen.findByTestId("amiga-install-blockers");
    expect(blockers.textContent).toContain("D:/roms/kick31.rom");
    expect(blockers.textContent).toContain(i18n.t("osinstall.amigaInstall.blocker.noEmulator"));
    expect(screen.getByRole("checkbox").hasAttribute("disabled")).toBe(true);
  });
});

describe("the four endings stay four sentences on screen", () => {
  /** Every ending's own sentence, as the user reads it. */
  const SAID: Record<string, string> = {
    succeeded: i18n.t("osinstall.amigaInstall.outcome.succeeded"),
    failed: i18n.t("osinstall.amigaInstall.outcome.failed"),
    "timed-out": i18n.t("osinstall.amigaInstall.outcome.timedOut", { seconds: 1800 }),
    "emulator-closed": i18n.t("osinstall.amigaInstall.outcome.emulatorClosed", { seconds: 12 }),
  };

  const ENDINGS: RunOutcome[] = [
    { kind: "succeeded" },
    { kind: "failed" },
    { kind: "timed-out", waited: { secs: 1800, nanos: 0 } },
    { kind: "emulator-closed", waited: { secs: 12, nanos: 0 } },
  ];

  for (const ending of ENDINGS) {
    it(`says its own sentence for '${ending.kind}' and none of the other three`, async () => {
      await runToConfirmation();
      const settlement =
        ending.kind === "succeeded"
          ? ({ kind: "promoted", tree: "D:/amiga/os39", leftBehind: null } as const)
          : ({ kind: "kept", copy: "D:/amiga/os39.art-run", original: "D:/amiga/os39" } as const);
      deliver!({ job_id: 7, outcome: ending, settlement });

      const outcome = await screen.findByTestId("amiga-install-outcome");
      expect(outcome.textContent).toBe(SAID[ending.kind]);
      // The whole point: no other ending's sentence is on screen. A screen
      // rendering nothing at all fails the line above, and a screen
      // collapsing two endings fails this one.
      const report = screen.getByTestId("amiga-install-report").textContent ?? "";
      for (const [kind, sentence] of Object.entries(SAID)) {
        if (kind === ending.kind) continue;
        expect(report).not.toContain(sentence);
      }
      // And a different next step for each — "watch the window next time" is
      // the wrong advice for a window the owner shut themselves.
      const next = screen.getByTestId("amiga-install-next").textContent ?? "";
      expect(next.length).toBeGreaterThan(0);
      expect(next).not.toBe(outcome.textContent);
    });
  }

  it("gives the four endings four different next steps", async () => {
    const steps = new Set<string>();
    for (const ending of ENDINGS) {
      await runToConfirmation();
      deliver!({
        job_id: 7,
        outcome: ending,
        settlement: { kind: "kept", copy: "D:/c", original: "D:/o" },
      });
      steps.add((await screen.findByTestId("amiga-install-next")).textContent ?? "");
      cleanup();
    }
    expect(steps.size).toBe(4);
  });
});

describe("where the copy is", () => {
  // "A user told 'it failed' and not told where the evidence went has been
  // given nothing."
  it("names the copy and says the original was untouched, for every ending that is not success", async () => {
    for (const ending of [
      { kind: "failed" } as const,
      { kind: "timed-out", waited: { secs: 1800, nanos: 0 } } as const,
      { kind: "emulator-closed", waited: { secs: 12, nanos: 0 } } as const,
    ]) {
      await runToConfirmation();
      deliver!({
        job_id: 7,
        outcome: ending,
        settlement: { kind: "kept", copy: "D:/amiga/os39.art-run", original: "D:/amiga/os39" },
      });
      const settlement = await screen.findByTestId("amiga-install-settlement");
      expect(settlement.textContent, ending.kind).toContain("D:/amiga/os39.art-run");
      expect(settlement.textContent, ending.kind).toContain("D:/amiga/os39");
      cleanup();
    }
  });

  it("names a retired tree it could not delete, rather than staying silent about it", async () => {
    await runToConfirmation();
    deliver!({
      job_id: 7,
      outcome: { kind: "succeeded" },
      settlement: { kind: "promoted", tree: "D:/amiga/os39", leftBehind: "D:/amiga/os39.art-old" },
    });
    const settlement = await screen.findByTestId("amiga-install-settlement");
    expect(settlement.textContent).toContain("D:/amiga/os39.art-old");
  });
});

describe("beginner mode hides and never disables", () => {
  it("keeps the warning, the copy note and the run available, and hides only the machinery", async () => {
    withChoices();
    render(<AmigaInstallPanel treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);
    await screen.findByTestId("amiga-install-preview");

    // Hidden in beginner mode: the AmigaDOS command line and the volumes.
    expect(screen.queryByTestId("amiga-install-detail")).toBeNull();
    // Never hidden, and never disabled: the announcement, and the run itself.
    expect(screen.getByTestId("emulator-window-warning")).toBeTruthy();
    expect(
      screen.getByRole("button", { name: i18n.t("osinstall.amigaInstall.run") }).hasAttribute("disabled")
    ).toBe(true); // …until confirmed, which is a confirmation and not the mode
    const user = userEvent.setup();
    await user.click(screen.getByRole("checkbox"));
    expect(
      screen.getByRole("button", { name: i18n.t("osinstall.amigaInstall.run") }).hasAttribute("disabled")
    ).toBe(false);
  });

  it("shows the command line and the three volumes in power mode", async () => {
    withChoices();
    useSettingsStore.setState((state) => ({ settings: { ...state.settings, uxMode: "power" } }));
    render(<AmigaInstallPanel treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);

    const detail = await screen.findByTestId("amiga-install-detail");
    expect(detail.textContent).toContain("ARTPkg:BoingBag3.9-1/C/Updater AmigaOS-Update DH0:");
    expect(detail.textContent).toContain("ARTWork");
  });
});

describe("the run itself", () => {
  it("sends the tree, the package and both archives exactly as chosen", async () => {
    useSettingsStore.setState((state) => ({
      settings: {
        ...state.settings,
        winuaePath: "C:/WinUAE/winuae64.exe",
        remembered: {
          "amigaInstall.package": "boingbag-39-1",
          "amigaInstall.archive": "D:/pkg/BoingBag39-1.lha",
          "amigaInstall.overlayArchive": "D:/pkg/BoingBag39-1-UAE.lha",
          "amigaInstall.kickstart": "D:/roms/kick31.rom",
        },
      },
    }));
    render(<AmigaInstallPanel treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);
    await screen.findByTestId("amiga-install-preview");
    const user = userEvent.setup();
    await user.click(screen.getByRole("checkbox"));
    await user.click(screen.getByRole("button", { name: i18n.t("osinstall.amigaInstall.run") }));

    await waitFor(() => expect(runMock).toHaveBeenCalled());
    expect(runMock.mock.calls[0][0]).toEqual({
      tree: "D:/amiga/os39",
      packageId: "boingbag-39-1",
      // Wrapper first, overlay second — the order is the wire's own.
      packageArchives: ["D:/pkg/BoingBag39-1.lha", "D:/pkg/BoingBag39-1-UAE.lha"],
      kickstart: "D:/roms/kick31.rom",
    });
    expect(runMock.mock.calls[0][1]).toBe("C:/WinUAE/winuae64.exe");
  });

  it("shows a refusal raised at the run instead of a job that could only go red", async () => {
    runMock.mockRejectedValue("WinUAE was not found in a standard install location");
    await runToConfirmation();
    const refusal = await screen.findByTestId("amiga-install-refusal");
    expect(refusal.textContent).toContain("WinUAE was not found");
  });
});
