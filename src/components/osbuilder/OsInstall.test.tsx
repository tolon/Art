// @vitest-environment jsdom
//
// ART-118: the OS Builder's install screen has never once been seen
// rendering past its five `h2` headings. A headless-Chrome probe (Chrome and
// Edge, headless and headed) reproducibly crashed the renderer
// (`-1073741819`, an access violation) the moment anything past the
// headings was touched — filling the media/ROM/destination fields, ticking
// a component, reading the confirmation or refusals card, running Verify.
// A browser cannot see this screen right now. jsdom can, so this is the
// first automated coverage of `OsInstall.tsx` — not a proxy harness the way
// `FileManagerFilter.test.tsx` and `useRomPairing.test.tsx` are, because the
// whole point here is proving the *real* component mounts, not a stand-in
// that always would have.
//
// Mocked at the same boundary the rest of this test suite mocks at — the
// `@/lib/*` wrappers around `invoke`/`listen`, not `@tauri-apps/api` itself
// (see `useRomPairing.test.tsx`). `@/lib/settings` is mocked too, one layer
// further down than usual: `useRemembered` (which this screen leans on for
// every field and for the component checklist) goes through
// `useSettingsStore`, whose `update()` calls `@/lib/settings`'s
// `saveSettings` — the actual `tauri-plugin-store` IPC boundary — on every
// tick. Left real, that call rejects in jsdom with nothing to catch it
// (`useRemembered`'s setter fires and forgets the promise), which Vitest
// counts as an unhandled rejection and fails the run. `getSettings` is
// mocked for the same reason, though nothing here calls it — this screen
// never calls `load()`, it only ever writes.
//
// What this establishes, one test group per requirement:
//   1. The screen mounts past its headings, with its real controls present.
//   2 & 3. Nothing on screen is a raw i18n key or an unrendered
//      `{{interpolation}}` — in English, and (ART-062, never checked before)
//      in Turkish, whose strings run measurably longer than the English
//      originals.
//   4. Ticking a component in the checklist — the screen's real input —
//      changes the request `osinstallPlan` is asked to plan, and what the
//      plan section shows.
//   5. A refusal renders as an actual sentence, not a blank card.
//
// What this does NOT establish: the access violation itself. jsdom does no
// layout at all, so it cannot reproduce a native renderer crash or measure
// overflow — ART-062's "does a long Turkish string actually fit" is still a
// real-screen job, and a human `pnpm tauri dev` pass over this screen is
// still owed. See the narrowed ART-118 entry in `docs/ISSUES.md`.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";

import { changeLanguage } from "@/i18n";
import { useSettingsStore } from "@/stores/settingsStore";
import type {
  ComponentDef,
  InstallPlan,
  InstallRequest,
  MediaScanResult,
  OsInstallResult,
  PlanItem,
  PlanResult,
  RefusalReason,
} from "@/lib/osinstall";
import type { RomInfo } from "@/lib/pistorm";

const scanMediaMock = vi.hoisted(() => vi.fn());
const componentsMock = vi.hoisted(() => vi.fn());
const planMock = vi.hoisted(() => vi.fn());
const componentCollisionsMock = vi.hoisted(() => vi.fn());
const applyMock = vi.hoisted(() => vi.fn());
const verifyMock = vi.hoisted(() => vi.fn());
const onResultMock = vi.hoisted(() => vi.fn());
const identifyRomMock = vi.hoisted(() => vi.fn());
const dialogOpenMock = vi.hoisted(() => vi.fn());
const onJobProgressMock = vi.hoisted(() => vi.fn());
const rescanMock = vi.hoisted(() => vi.fn());
const releaseForMediaMock = vi.hoisted(() => vi.fn());
const packagesMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallScanMedia: scanMediaMock,
  osinstallComponents: componentsMock,
  osinstallPlan: planMock,
  osinstallRescanMedia: rescanMock,
  osinstallReleaseForMedia: releaseForMediaMock,
  osinstallPackages: packagesMock,
  osinstallComponentCollisions: componentCollisionsMock,
  osinstallApply: applyMock,
  osinstallVerify: verifyMock,
  onOsInstallResult: onResultMock,
}));

// `PackagePanel` (Task 7) is now mounted inside this screen, and both it and
// this screen's own install job subscribe to `onJobProgress` on mount — real
// `listen()` has no Tauri IPC bridge to reach in jsdom and rejects, which
// Vitest counts as an unhandled rejection at teardown (ART-163's own shape).
// Mocked here for the same reason `osinstallPlan`/`osinstallApply` above are.
vi.mock("@/lib/jobs", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/jobs")>()),
  onJobProgress: onJobProgressMock,
}));

vi.mock("@/lib/pistorm", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/pistorm")>()),
  pistormIdentifyRom: identifyRomMock,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: dialogOpenMock,
}));

// The one real Tauri IPC boundary `useRemembered` reaches on every tick
// (see the module comment above) — mocked so a checkbox click never tries a
// real `tauri-plugin-store` round trip.
vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  getSettings: vi.fn(),
  saveSettings: vi.fn().mockResolvedValue(undefined),
}));

const { OsInstall } = await import("@/components/osbuilder/OsInstall");
const { refusalPhrase } = await import("@/lib/osinstall");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");

afterEach(async () => {
  cleanup();
  // Never let a Turkish test bleed its language into the next test file's
  // first render.
  await changeLanguage("en");
});

const ROM: RomInfo = {
  name: "Kickstart 3.2 (47.96)",
  version: "47",
  revision: "96",
  size_bytes: 524288,
  sha256: "a".repeat(64),
  crc32: "12345678",
  is_cloanto: false,
  key_available: false,
  is_aros: false,
  checksum: "valid",
  compatible_models: ["a1200"],
  file_path: "E:\\roms\\kick.rom",
};

/**
 * What `osinstallComponents` answers, per release — the two shipped recipes
 * cut down to what this screen actually reasons about.
 *
 * Two things here are the point rather than the setup. The lists **differ**,
 * and both carry a `workbench-base` naming **different media**: that is the
 * defect the checklist used to have, where a hardcoded AmigaOS 3.2
 * catalogue was rendered whatever the picker said, so 3.9 showed 26
 * components for a one-component recipe and labelled its base component
 * `Workbench3.2`.
 */
const COMPONENTS_32: ComponentDef[] = [
  {
    id: "workbench-base",
    media: "Workbench3.2",
    required: true,
    available: true,
    conditionMajor: null,
    requiresRomMajor: null,
    exclusiveGroup: null,
    overrides: [],
  },
  {
    id: "install-libs",
    media: "Install3.2",
    required: true,
    available: true,
    conditionMajor: null,
    requiresRomMajor: null,
    exclusiveGroup: null,
    overrides: [],
  },
  {
    id: "extras",
    media: "Extras3.2",
    required: false,
    available: true,
    conditionMajor: null,
    requiresRomMajor: null,
    exclusiveGroup: null,
    overrides: [],
  },
  {
    id: "modules-a1200",
    media: "ModulesA1200_3.2",
    required: false,
    available: true,
    conditionMajor: 47,
    requiresRomMajor: null,
    exclusiveGroup: "modules",
    overrides: [],
  },
];

const COMPONENTS_39: ComponentDef[] = [
  {
    id: "workbench-base",
    media: "AmigaOS3.9",
    required: true,
    available: true,
    conditionMajor: null,
    requiresRomMajor: null,
    exclusiveGroup: null,
    overrides: [],
  },
  // The component ART-175 is about: the one that makes a tree AmigaOS 3.9
  // rather than 3.5, by replacing files `workbench-base` placed. Its
  // `overrides` is what the screen keys the preview off, and the shipped
  // recipe really declares `["workbench-base", "locale-base"]` for it —
  // pinned by `src/lib/osinstall.test.ts`'s recipe-parity test.
  {
    id: "workbench-39",
    media: "AmigaOS3.9",
    required: true,
    available: true,
    conditionMajor: null,
    requiresRomMajor: null,
    exclusiveGroup: null,
    overrides: ["workbench-base"],
  },
];

function componentsFor(release: string): ComponentDef[] {
  return release === "AmigaOS 3.9" ? COMPONENTS_39 : COMPONENTS_32;
}

const ITEM_WORKBENCH: PlanItem = {
  component: "workbench-base",
  media: "Workbench3.2",
  from: "DF0:C/Format",
  to: "C/Format",
  isDir: false,
  bytes: 2 * 1024 * 1024,
};

const ITEM_EXTRAS: PlanItem = {
  component: "extras",
  media: "Extras3.2",
  from: "DF0:Tools/HDToolBox",
  to: "Tools/HDToolBox",
  isDir: false,
  bytes: 3 * 1024 * 1024,
};

/** The plan `osinstallPlan` would answer for a given request — items follow
 *  `chosen` for real, so ticking "extras" is visible in what comes back,
 *  the same way it would be against the real engine. */
function planResultFor(req: InstallRequest): PlanResult {
  const items = req.chosen.includes("extras") ? [ITEM_WORKBENCH, ITEM_EXTRAS] : [ITEM_WORKBENCH];
  return {
    outcome: "planned",
    plan: {
      release: "3.2",
      items,
      refusals: [],
      totalBytes: items.reduce((sum, item) => sum + item.bytes, 0),
      totalFiles: items.filter((item) => !item.isDir).length,
      // ROM is V47 here, so "modules-a1200" (conditionMajor: 47) is not
      // forced on — the "condition-off" reasoning branch, not "rom-needed".
      // Release-aware, because ART-175's whole subject is a component that
      // is switched on *by the recipe* and layers over another: for AmigaOS
      // 3.9 both `workbench-base` and `workbench-39` are on without being
      // chosen, exactly as the shipped recipe has them.
      componentsOn:
        req.release === "AmigaOS 3.9"
          ? ["workbench-base", "workbench-39", ...req.chosen]
          : ["workbench-base", "install-libs", ...req.chosen],
      mediaPaths: { "Workbench3.2": "E:\\media\\Disk1.adf" },
      packages: [],
      packageMedia: {},
      userStartup: [],
      activations: [],
      mediaStamps: {},
    },
  };
}

const REFUSAL: RefusalReason = {
  refusal: "media-missing",
  component: "workbench-base",
  volume_name: "Workbench3.2",
};

function seedRemembered(overrides: Record<string, unknown>) {
  useSettingsStore.setState({
    loaded: true,
    settings: { ...DEFAULT_SETTINGS, remembered: { ...overrides } },
  });
}

/**
 * Every field set — for **both** releases, which is what "every field" means
 * since ART-207 keyed the media folder and the destination per release the
 * way `chosen` already was. The bare keys are AmigaOS 3.2's (see
 * `rememberedComponentKey`: 3.2 is "the release before there was a picker"),
 * the suffixed ones are 3.9's, and they hold **different paths** on purpose —
 * a test that switches release can then tell which one the screen is using
 * instead of watching one shared value stay put.
 *
 * The ROM is deliberately shared: a Kickstart belongs to the machine being
 * built for, not to the release being installed.
 */
const FULL_FIELDS = {
  "osinstall.mediaFolder": "E:\\media",
  "osinstall.rom": "E:\\roms\\kick.rom",
  "osinstall.destination": "E:\\dist",
  "osinstall.chosen": [],
  "osinstall.excludedConditional": [],
  "osinstall.mediaFolder.AmigaOS 3.9": "E:\\media39",
  "osinstall.destination.AmigaOS 3.9": "E:\\dist39",
};

beforeEach(() => {
  scanMediaMock.mockReset().mockResolvedValue({
    outcome: "found",
    media: [{ path: "E:\\media\\Disk1.adf", volumeName: "Workbench3.2", kind: "floppy" }],
  } satisfies MediaScanResult);
  componentsMock
    .mockReset()
    .mockImplementation((release: string) => Promise.resolve(componentsFor(release)));
  planMock.mockReset().mockImplementation((req: InstallRequest) => Promise.resolve(planResultFor(req)));
  // Nothing layering is switched on for AmigaOS 3.2, so this is never called
  // in most tests; an empty preview is the honest default for the ones where
  // it is.
  componentCollisionsMock.mockReset().mockResolvedValue({ reports: [], placed: 0 });
  applyMock.mockReset().mockResolvedValue(1);
  verifyMock.mockReset();
  onResultMock.mockReset().mockResolvedValue(() => {});
  identifyRomMock.mockReset().mockResolvedValue(ROM);
  dialogOpenMock.mockReset().mockResolvedValue(null);
  onJobProgressMock.mockReset().mockResolvedValue(() => {});
  rescanMock.mockReset().mockResolvedValue(1);
  releaseForMediaMock.mockReset().mockResolvedValue(null);
  packagesMock.mockReset().mockResolvedValue([]);
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
});

/** Media folder, ROM and destination all set — the state the browser probe
 *  could never reach — then waits for the live plan preview to land. */
async function renderFull() {
  seedRemembered(FULL_FIELDS);
  const utils = render(<OsInstall />);
  await waitFor(() => expect(planMock).toHaveBeenCalled());
  await screen.findByText(i18n.t("osinstall.plan.heading"));
  // The checklist is loaded, not hardcoded, so it arrives on its own round
  // trip — wait for it rather than racing it.
  await screen.findByRole("checkbox", { name: "Extras3.2" });
  return utils;
}

describe("OsInstall renders past its headings", () => {
  it("mounts with the real media, ROM, destination, checklist and action controls", async () => {
    await renderFull();

    // The five headings a browser probe once confirmed are not the news
    // here — everything below them, which the probe never survived to see,
    // is.
    expect(screen.getByText(i18n.t("osinstall.media.label"))).toBeTruthy();
    expect(screen.getByText(i18n.t("osinstall.rom.label"))).toBeTruthy();
    expect(screen.getByText(i18n.t("osinstall.destination.label"))).toBeTruthy();

    // The component checklist is the screen's real input (requirement 4) —
    // one row per component of the release's own loaded recipe. `+ 3` are the
    // tickboxes that are not components: the run card's confirmation,
    // `AmigaInstallPanel`'s own (the Amiga-side install round's task 6), and
    // the media section's "reuse the last scan" (ART-194). `PackagePanel`'s
    // confirmation is not among them — it renders only once a package has been
    // ticked, and nothing here ticks one.
    const checkboxes = screen.getAllByRole("checkbox");
    expect(checkboxes.length).toBe(COMPONENTS_32.length + 3);
    expect(
      screen.getByRole("checkbox", { name: i18n.t("osinstall.media.reuseScan") })
    ).toBeTruthy();

    expect(screen.getByRole("button", { name: i18n.t("osinstall.run.run") })).toBeTruthy();
    expect(screen.getByRole("button", { name: i18n.t("osinstall.verify.run") })).toBeTruthy();
  });
});

// A string that is *only* dot-separated identifier segments, three or more
// of them (two-plus dots) — the shape of an i18next key rendered raw
// (`"osinstall.media.label"`) when the lookup failed. Anchored end to end so
// an ordinary sentence containing a version-numbered name like
// "Workbench3.2" (one dot) never matches: real prose has spaces, and no
// sentence in either catalogue is itself an unbroken run of identifiers and
// dots.
const KEY_SHAPE = /^[a-zA-Z][a-zA-Z0-9]*(\.[a-zA-Z][a-zA-Z0-9]*){2,}$/;

/** Every rendered text node that looks like a raw i18next key, or that
 *  still carries a literal `{{` — i18next's own rendering of a missing
 *  interpolation variable. Walking text nodes individually (rather than
 *  `container.textContent`) is what keeps this from false-positiving on
 *  text that only looks suspicious once two elements' text is concatenated. */
function rawI18nArtifacts(container: HTMLElement): string[] {
  const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
  const offenders: string[] = [];
  let node: Node | null;
  while ((node = walker.nextNode())) {
    const text = node.textContent?.trim() ?? "";
    if (!text) continue;
    if (KEY_SHAPE.test(text) || text.includes("{{")) offenders.push(text);
  }
  return offenders;
}

describe("nothing on screen is a raw i18n key or an unrendered interpolation", () => {
  it("in English", async () => {
    const { container } = await renderFull();
    expect(rawI18nArtifacts(container)).toEqual([]);
  });

  it("in Turkish — ART-062: no language had ever been checked on a running screen", async () => {
    await changeLanguage("tr");
    const { container } = await renderFull();
    expect(rawI18nArtifacts(container)).toEqual([]);

    // Prove this actually rendered in Turkish rather than silently falling
    // back to English — otherwise the assertion above would pass for the
    // wrong reason.
    expect(screen.getByText("AmigaOS Kur")).toBeTruthy();
  });
});

describe("ticking a component changes what the screen will do", () => {
  it("reaches the request osinstallPlan is asked to plan, and what the plan section shows", async () => {
    await renderFull();

    // ART-205: the tree the plan predicts (one file) and the work it
    // describes (one item), which are the same number only while nothing
    // overrides anything.
    expect(document.body.textContent).toContain("1 file, ");
    expect(document.body.textContent).toContain("from 1 planned item.");

    const checkbox = screen.getByRole("checkbox", { name: "Extras3.2" }) as HTMLInputElement;
    expect(checkbox.checked).toBe(false);

    await userEvent.click(checkbox);
    expect(checkbox.checked).toBe(true);

    // The real input this screen has: the tick has to reach the request
    // that gets planned, not just flip local checkbox state.
    await waitFor(() => {
      const askedForExtras = (planMock.mock.calls as [InstallRequest][]).some(([req]) =>
        req.chosen.includes("extras")
      );
      expect(askedForExtras).toBe(true);
    });

    // ...and what the user sees changes with it — a second plan item shown,
    // not just an API call nobody could see the effect of.
    await waitFor(() => expect(document.body.textContent).toContain("from 2 planned items."));
  });
});

describe("the screen does not plan the same thing twice", () => {
  // ART-119 (#1). This screen keeps two plans on purpose — one asked with
  // nothing excluded, so a conditional component's *true* state is knowable,
  // and one asked with the real exclusions — but with nothing excluded the
  // two requests are byte-identical, so the second call planned the same
  // media over again and threw the answer away. `plan()` opens and walks
  // every switched-on component's disc image, so that is real work on every
  // keystroke in the media, ROM and destination fields alike.

  /** How often each distinct request shape was submitted. */
  function requestCounts(): Map<string, number> {
    const seen = new Map<string, number>();
    for (const [req] of planMock.mock.calls as [InstallRequest][]) {
      const key = JSON.stringify(req);
      seen.set(key, (seen.get(key) ?? 0) + 1);
    }
    return seen;
  }

  it("asks once for a request shape it has already asked for", async () => {
    await renderFull();

    // Every request so far carries no exclusions, which is the case where
    // the base and effective requests are identical.
    expect(
      (planMock.mock.calls as [InstallRequest][]).every(([req]) => req.excluded.length === 0)
    ).toBe(true);

    // Measured, not reasoned about: reverting the dedupe to the old
    // `Promise.all([plan(base), plan(effective)])` makes this same render
    // submit **4** requests, all four byte-identical. It is 2 now. (Two and
    // not one because the effect settles twice — `useRemembered` hands back
    // a fresh array identity when the persisted value lands, which is its
    // own duplication and not this one; halving each pass is what this fix
    // does, and it is the half that was a duplicate *within* a pass.)
    expect(planMock.mock.calls.length).toBe(2);
    expect([...requestCounts().values()]).toEqual([2]);
  });

  it("still asks twice when the two requests genuinely differ", async () => {
    // `modules-a1200` has to be *forced on by its condition* for excluding
    // it to mean anything — an unforced component is turned off by plain
    // unticking, which changes `chosen` and never populates `excluded`. The
    // default fixture's V47 ROM leaves it off, so this test supplies a plan
    // where the engine switched it on without it being chosen: exactly what
    // a pre-V47 ROM produces, and the only state in which the screen offers
    // "turn it off anyway".
    planMock.mockImplementation((req: InstallRequest) => {
      const base = planResultFor(req);
      if (base.outcome !== "planned" || req.excluded.includes("modules-a1200")) return Promise.resolve(base);
      return Promise.resolve({
        ...base,
        plan: { ...base.plan, componentsOn: [...base.plan.componentsOn, "modules-a1200"] },
      } satisfies PlanResult);
    });

    await renderFull();

    const modules = screen.getByRole("checkbox", { name: "ModulesA1200_3.2" }) as HTMLInputElement;
    expect(modules.checked).toBe(true);
    await userEvent.click(modules);
    await userEvent.click(
      await screen.findByRole("button", { name: i18n.t("osinstall.components.confirmOff.confirm") })
    );

    // Both requests are made again, because now they differ. The dedupe must
    // have removed the duplicate, never the second plan — dropping the base
    // plan would make "is this condition satisfied" and "is this excluded"
    // indistinguishable, which is the whole reason there are two.
    await waitFor(() => {
      const calls = planMock.mock.calls as [InstallRequest][];
      expect(calls.some(([req]) => req.excluded.includes("modules-a1200"))).toBe(true);
      expect(
        calls.some(([req]) => req.excluded.length === 0 && req.chosen.length === 0)
      ).toBe(true);
    });
    // Two distinct shapes, not one — the base plan is still being asked for
    // alongside the effective one. `requestCounts` has more than one key
    // exactly when both survived.
    expect(requestCounts().size).toBeGreaterThan(1);
  });
});

describe("reusing the last scan, and asking for a fresh one (ART-194)", () => {
  it("plans with the cache on by default, and remembers the user turning it off", async () => {
    await renderFull();

    // Cached by default: nobody had to switch this on, and the very first
    // request the screen ever sends already says so.
    expect(planMock.mock.calls.length).toBeGreaterThan(0);
    for (const [req] of planMock.mock.calls as [InstallRequest][]) {
      expect(req.scanCache).toBe("reuse");
    }

    const box = screen.getByRole("checkbox", {
      name: i18n.t("osinstall.media.reuseScan"),
    }) as HTMLInputElement;
    expect(box.checked).toBe(true);

    planMock.mockClear();
    await userEvent.click(box);

    // Two things, and the second is the one that would go unnoticed: the plan
    // is re-asked *with the new answer*, and the choice was written to the
    // remembered store so tomorrow's run starts where this one ended.
    await waitFor(() =>
      expect(planMock).toHaveBeenCalledWith(expect.objectContaining({ scanCache: "ignore" }))
    );
    await waitFor(() => {
      const bag = useSettingsStore.getState().settings.remembered as Record<string, unknown>;
      expect(bag["osinstall.reuseScan"]).toBe(false);
    });
  });

  it("asks the backend to forget what it remembered, and re-plans against the discs", async () => {
    await renderFull();
    planMock.mockClear();

    await userEvent.click(screen.getByRole("button", { name: i18n.t("osinstall.media.rescan") }));

    // It reaches the command that actually deletes the listings — a rescan
    // that merely skipped the cache for one call would leave the stale answer
    // sitting there for the next one.
    await waitFor(() => expect(rescanMock).toHaveBeenCalled());
    // …and the screen really re-plans afterwards, rather than showing the
    // answer it already had.
    await waitFor(() => expect(planMock).toHaveBeenCalled());
    // …and says what it did, so the button cannot be mistaken for one that
    // ignored the click.
    await screen.findByText(i18n.t("osinstall.media.rescanned", { count: 1 }));
  });
});

describe("the screen settles instead of re-planning for ever (ART-195)", () => {
  // What the owner saw, and the number that made it undeniable: the release
  // build's `%TEMP%` held preview staging roots numbered up to **2,149** from
  // one session, five of them created inside two seconds. That is not a
  // component being toggled — it is an effect firing per render.
  //
  // The cause is `useRemembered`'s inline `[]` fallback. `recall` hands the
  // caller's own fallback back when nothing is stored, so an unstored key
  // yields a *fresh array identity* every render, and the plan effect lists
  // `chosen` and `excludedConditional` among its dependencies. Each pass
  // planned (three full walks of a 468 MB ISO), set state, and re-rendered.
  //
  // **Why every existing test missed it.** `rememberedComponentKey` returns
  // the bare key for AmigaOS 3.2 — "the release before there was a picker" —
  // and `FULL_FIELDS` seeds exactly those bare keys. So on 3.2 both lists are
  // stored, both identities are stable, and nothing loops. On 3.9 the keys are
  // `osinstall.chosen.AmigaOS 3.9` and `osinstall.excludedConditional.AmigaOS
  // 3.9`, which nothing had ever written. The owner was installing 3.9.
  //
  // So this test switches to 3.9 and counts. It is deliberately about the
  // *release the owner used*, not the one the fixture defaults to.
  it("does not keep re-planning a release whose remembered keys are empty", async () => {
    await renderFull();

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");
    await waitFor(() =>
      expect(planMock).toHaveBeenCalledWith(expect.objectContaining({ release: "AmigaOS 3.9" }))
    );

    // Let the screen settle, then measure how much more work it starts while
    // nothing at all is happening. Against the defect this climbs without
    // bound; the assertion is that it climbs by nothing.
    await new Promise((resolve) => setTimeout(resolve, 120));
    const planned = planMock.mock.calls.length;
    const previewed = componentCollisionsMock.mock.calls.length;
    await new Promise((resolve) => setTimeout(resolve, 250));

    expect(planMock.mock.calls.length).toBe(planned);
    expect(componentCollisionsMock.mock.calls.length).toBe(previewed);
  });
});

describe("choosing the release re-plans against it", () => {
  it("plans the release the user chose", async () => {
    await renderFull();

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    expect(picker.value).toBe("AmigaOS 3.2");

    await userEvent.selectOptions(picker, "AmigaOS 3.9");
    expect(picker.value).toBe("AmigaOS 3.9");

    await waitFor(() =>
      expect(planMock).toHaveBeenCalledWith(expect.objectContaining({ release: "AmigaOS 3.9" }))
    );
  });

  it("shows the chosen release's own components, not the previous release's", async () => {
    // The Major finding a whole-branch review called the worst on its list:
    // the picker changed the plan and left a hardcoded AmigaOS 3.2 checklist
    // on screen, so the user was shown one operating system's parts while
    // ART installed another's (§89). The old test asserted only that
    // `planMock` was called with the new release — it never looked at the
    // checklist, which is why this survived.
    await renderFull();
    expect(screen.getByRole("checkbox", { name: "Extras3.2" })).toBeTruthy();

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");

    // The list is re-read for the release actually chosen...
    await waitFor(() => expect(componentsMock).toHaveBeenCalledWith("AmigaOS 3.9"));
    // ...3.2's own components are gone...
    await waitFor(() => expect(screen.queryByRole("checkbox", { name: "Extras3.2" })).toBeNull());
    // ...and 3.9's base component is labelled with 3.9's media, not 3.2's,
    // even though both recipes call it `workbench-base`.
    // `getAllBy`, not `getBy`: every component in the shipped AmigaOS 3.9
    // recipe carries the media name `AmigaOS3.9`, so more than one row
    // matches — which the fixture only started reflecting when ART-175 added
    // `workbench-39` to it. A `getBy` here was passing because the fixture
    // was thinner than the recipe, not because the screen shows one row.
    expect(screen.getAllByRole("checkbox", { name: /AmigaOS3\.9/ }).length).toBeGreaterThan(0);
    expect(screen.queryByRole("checkbox", { name: /Workbench3\.2/ })).toBeNull();
  });

  it("keeps each release's own ticks when the user switches away and back", async () => {
    // "Nothing changes unless the user changes it", applied to a choice made
    // for a release the user then looked away from. The remembered set is
    // keyed per release (`rememberedComponentKey`), so 3.9 — whose recipe
    // holds none of 3.2's ids — cannot sanitize them away.
    await renderFull();

    const extras = screen.getByRole("checkbox", { name: "Extras3.2" }) as HTMLInputElement;
    await userEvent.click(extras);
    expect(extras.checked).toBe(true);

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");
    await waitFor(() => expect(screen.queryByRole("checkbox", { name: "Extras3.2" })).toBeNull());

    await userEvent.selectOptions(picker, "AmigaOS 3.2");
    const backAgain = (await screen.findByRole("checkbox", {
      name: "Extras3.2",
    })) as HTMLInputElement;
    expect(backAgain.checked).toBe(true);
  });
});

describe("a media folder belongs to the release it holds (ART-207)", () => {
  // The owner chose AmigaOS 3.2 with the folder their AmigaOS 3.9 disc was
  // in still remembered, and every one of the 3.2 recipe's sixteen
  // components refused `MediaMissing` — sixteen true sentences that together
  // told them "a lot of programs are missing" about a folder that was simply
  // the wrong one. `chosen` and `excludedConditional` were already keyed per
  // release (`rememberedComponentKey`) for exactly this reason, and the
  // reason given there — "a component id means something only inside the
  // recipe that declares it" — is true word for word of a media folder: a
  // folder holding AmigaOS3.9 means nothing to the 3.2 recipe.
  //
  // The ROM is deliberately NOT part of this: a Kickstart is a property of
  // the machine being built for, not of the release being installed, and the
  // owner's one A1200 ROM is the right answer for both.
  it("shows each release's own media folder, never the other's", async () => {
    await renderFull();
    expect(screen.getByText("E:\\media")).toBeTruthy();

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");
    await waitFor(() => expect(componentsMock).toHaveBeenCalledWith("AmigaOS 3.9"));

    expect(await screen.findByText("E:\\media39")).toBeTruthy();
    expect(screen.queryByText("E:\\media")).toBeNull();
  });

  it("plans a release into its own destination, not the other release's", async () => {
    await renderFull();
    // `getAllByText`: the path is on screen more than once — this field, and
    // the build session's own tree root, which is a different thing (ART-197)
    // and is not what this test is about. So the assertion that matters is
    // made against the request `plan()` is actually given, not the DOM.
    expect(screen.getAllByText("E:\\dist").length).toBeGreaterThan(0);

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");

    await waitFor(() =>
      expect(planMock).toHaveBeenCalledWith(
        expect.objectContaining({ release: "AmigaOS 3.9", destination: "E:\\dist39" })
      )
    );
    // Not one 3.9 plan may name the folder the user set aside for their 3.2
    // build — `toHaveBeenCalledWith` above would be satisfied by a single
    // correct call among wrong ones.
    const planned39 = planMock.mock.calls
      .map((call) => call[0] as InstallRequest)
      .filter((req) => req.release === "AmigaOS 3.9");
    expect(planned39.length).toBeGreaterThan(0);
    for (const req of planned39) {
      expect(req.destination).not.toBe("E:\\dist");
    }
  });

  it("keeps each release's own media folder when the user switches away and back", async () => {
    await renderFull();

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");
    await waitFor(() => expect(componentsMock).toHaveBeenCalledWith("AmigaOS 3.9"));

    // The media row is the first of the three `Field`s on this screen —
    // media, ROM, destination, in that order.
    dialogOpenMock.mockResolvedValue("E:\\os39");
    await userEvent.click(
      screen.getAllByRole("button", { name: i18n.t("common.browse") })[0]
    );
    expect(await screen.findByText("E:\\os39")).toBeTruthy();

    await userEvent.selectOptions(picker, "AmigaOS 3.2");
    expect(await screen.findByText("E:\\media")).toBeTruthy();

    await userEvent.selectOptions(picker, "AmigaOS 3.9");
    expect(await screen.findByText("E:\\os39")).toBeTruthy();
  });
});

describe("a disc dropped on the panel", () => {
  it("takes the folder from a disc dropped on the panel", async () => {
    // A JSX attribute string literal does not process `\\` as a JS escape
    // sequence the way a normal string literal does (unlike the call below,
    // which is an ordinary function argument) — passed as a bare attribute,
    // the brief's literal path would arrive with doubled backslashes. The
    // `{...}` expression form is what makes this an actual JS string.
    render(
      <OsInstall
        droppedMedia={{ path: "E:\\amiga\\Amigatolon\\iso\\AmigaOS39.iso", arrivalKey: "k1" }}
      />
    );
    await waitFor(() =>
      expect(scanMediaMock).toHaveBeenCalledWith("E:\\amiga\\Amigatolon\\iso")
    );
  });

  it("takes effect again when the same disc is dropped a second time", async () => {
    const PATH = "E:\\amiga\\Amigatolon\\iso\\AmigaOS39.iso";
    const FOLDER = "E:\\amiga\\Amigatolon\\iso";

    const { rerender } = render(<OsInstall droppedMedia={{ path: PATH, arrivalKey: "k1" }} />);
    await waitFor(() => expect(scanMediaMock).toHaveBeenCalledWith(FOLDER));
    scanMediaMock.mockClear();

    // The user changes the media folder by hand in between — the same
    // remembered-setter path a Browse click goes through.
    seedRemembered({ "osinstall.mediaFolder": "E:\\somewhere-else" });

    // A second drop of the *same* disc: the path string is identical to the
    // first arrival, so only a change in `arrivalKey` can make this visible.
    // Keying the effect on the path alone (the bug the review caught) would
    // leave the hand-picked folder in place and never re-scan.
    rerender(<OsInstall droppedMedia={{ path: PATH, arrivalKey: "k2" }} />);
    await waitFor(() => expect(scanMediaMock).toHaveBeenCalledWith(FOLDER));
  });
});

describe("a folder that is simply the wrong one (ART-208)", () => {
  // The owner's own screen, reproduced: AmigaOS 3.2 chosen, and the folder
  // still pointing at the one holding their AmigaOS 3.9 disc. Sixteen
  // components, sixteen `MediaMissing` refusals, every one of them true —
  // and what they read off the screen was "a lot of programs are missing".
  const WRONG_FOLDER_REFUSALS: RefusalReason[] = [
    { refusal: "media-missing", component: "workbench-base", volume_name: "Workbench3.2" },
    { refusal: "media-missing", component: "install-libs", volume_name: "Install3.2" },
    { refusal: "media-missing", component: "extras", volume_name: "Extras3.2" },
  ];

  /** A 3.2 build pointed at a folder holding exactly one AmigaOS 3.9 disc. */
  async function renderWrongFolder(releaseHolding: string | null) {
    scanMediaMock.mockReset().mockResolvedValue({
      outcome: "found",
      media: [
        { path: "E:\\media\\AmigaOS39.iso", volumeName: "AmigaOS3.9", kind: "disc" },
      ],
    } satisfies MediaScanResult);
    planMock.mockReset().mockImplementation((req: InstallRequest) =>
      Promise.resolve({
        outcome: "planned",
        plan: {
          release: "3.2",
          items: [],
          refusals: WRONG_FOLDER_REFUSALS,
          totalBytes: 0,
          totalFiles: 0,
          componentsOn: ["workbench-base", "install-libs", ...req.chosen],
          mediaPaths: {},
          packages: [],
          packageMedia: {},
          userStartup: [],
          activations: [],
          mediaStamps: {},
        },
      } satisfies PlanResult)
    );
    releaseForMediaMock.mockReset().mockResolvedValue(releaseHolding);
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);
    await waitFor(() => expect(planMock).toHaveBeenCalled());
  }

  it("says it once, instead of once per component", async () => {
    await renderWrongFolder(null);

    expect(
      await screen.findByText(
        i18n.t("osinstall.blocked.wrongFolder", { found: "AmigaOS3.9" })
      )
    ).toBeTruthy();

    // And the three sentences it replaces are gone — computed the way the
    // screen computes them, not typed out here where they could drift.
    for (const refusal of WRONG_FOLDER_REFUSALS) {
      const phrase = refusalPhrase(refusal);
      expect(screen.queryByText(i18n.t(phrase.key, phrase.params))).toBeNull();
    }
  });

  it("names the release the folder belongs to, and switching is one click", async () => {
    await renderWrongFolder("AmigaOS 3.9");

    expect(
      await screen.findByText(
        i18n.t("osinstall.blocked.wrongFolderIsRelease", {
          release: "AmigaOS 3.9",
          found: "AmigaOS3.9",
        })
      )
    ).toBeTruthy();

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    expect(picker.value).toBe("AmigaOS 3.2");

    await userEvent.click(
      screen.getByRole("button", {
        name: i18n.t("osinstall.blocked.switchRelease", { release: "AmigaOS 3.9" }),
      })
    );

    // The button does the thing it names — the release actually changes,
    // rather than the screen merely suggesting it.
    await waitFor(() => expect(picker.value).toBe("AmigaOS 3.9"));
  });

  it("keeps the per-disk list when the folder is the right one", async () => {
    // One disk absent from an otherwise right folder is a missing disk, and
    // this screen must still say which. The guard that separates the two
    // cases lives in `wrongMediaFolder`; this is it seen from the screen.
    scanMediaMock.mockReset().mockResolvedValue({
      outcome: "found",
      media: [{ path: "E:\\media\\Disk1.adf", volumeName: "Workbench3.2", kind: "floppy" }],
    } satisfies MediaScanResult);
    planMock.mockReset().mockResolvedValue({
      outcome: "planned",
      plan: {
        release: "3.2",
        items: [],
        refusals: [WRONG_FOLDER_REFUSALS[0]],
        totalBytes: 0,
        totalFiles: 0,
        componentsOn: ["workbench-base"],
        mediaPaths: {},
        packages: [],
        packageMedia: {},
        userStartup: [],
        activations: [],
        mediaStamps: {},
      },
    } satisfies PlanResult);
    releaseForMediaMock.mockReset().mockResolvedValue(null);
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);

    const phrase = refusalPhrase(WRONG_FOLDER_REFUSALS[0]);
    expect(await screen.findByText(i18n.t(phrase.key, phrase.params))).toBeTruthy();
  });
});

describe("the release the user picks is the release the whole screen is on (ART-209, ART-211)", () => {
  // The owner chose AmigaOS 3.2 and the update-packages panel below still
  // offered BoingBags — 3.9's archives, of which 3.2 has none: "3.2 ile 3.9
  // secenekleri GUI'de karismis birbirine girmis."
  //
  // Two defects met here. The packages were never scoped by release at all
  // (ART-209), and the release this screen owned was a *different variable*
  // from the one the build session carried (ART-211) — so even a scoped
  // panel mounted from the OS Builder's own step routes, which read the
  // session, would have been handed the wrong answer.
  //
  // Asserted through what the panel actually asks for, not through what is
  // stored: a test reading the remembered key would pass against a screen
  // that stored the release correctly and still showed the other release's
  // packages.
  it("asks for the chosen release's packages, and asks again when it changes", async () => {
    seedRemembered({
      ...FULL_FIELDS,
      "buildSession.packages": { folder: "E:\\archives", chosen: [] },
    });
    render(<OsInstall />);

    await waitFor(() => expect(packagesMock).toHaveBeenCalled());

    // **Every** call, never "some call" — and that is not pedantry, it is
    // what a mutation caught. `PackagePanel` and `AmigaInstallPanel` both ask
    // this same question, so `toHaveBeenCalledWith(...)` is satisfied by
    // either one of them alone: hardcoding the release inside one panel left
    // the first version of this test green, because the other panel still
    // passed the right one. An assertion that one caller is correct says
    // nothing at all about the other.
    const releasesAsked = () => packagesMock.mock.calls.map((call) => call[1]);
    expect(releasesAsked().length).toBeGreaterThan(0);
    for (const asked of releasesAsked()) expect(asked).toBe("AmigaOS 3.2");
    expect(packagesMock).toHaveBeenCalledWith("E:\\archives", "AmigaOS 3.2");

    packagesMock.mockClear();
    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");

    await waitFor(() => expect(packagesMock).toHaveBeenCalled());
    for (const asked of releasesAsked()) expect(asked).toBe("AmigaOS 3.9");
  });

  // **This one deliberately asserts on storage**, which the test above says
  // it will not do — because for ART-211 storage is not an implementation
  // detail, it *is* the channel. The OS Builder's step routes
  // (`pages/osbuilder/steps.tsx`) mount `PackagePanel` and
  // `AmigaInstallPanel` themselves and read `useBuildSession`, so the only
  // thing connecting the picker on this screen to what those steps offer is
  // the session's own remembered key. While this screen owned
  // `osinstall.release` instead, the picker moved one variable and the steps
  // read the other, and nothing in a test of this component alone could see
  // it.
  it("moves the build session's own release, which is what the steps read", async () => {
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);
    await waitFor(() => expect(planMock).toHaveBeenCalled());

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");

    await waitFor(() => {
      const bag = useSettingsStore.getState().settings.remembered as Record<string, unknown>;
      expect(bag["buildSession.release"]).toBe("AmigaOS 3.9");
    });
  });
});

describe("a refusal renders as a sentence, not a blank", () => {
  it("shows the real, translated refusal text", async () => {
    const refusedPlan: InstallPlan = {
      release: "3.2",
      items: [],
      refusals: [REFUSAL],
      totalBytes: 0,
      totalFiles: 0,
      componentsOn: ["workbench-base", "install-libs"],
      mediaPaths: {},
      packages: [],
      packageMedia: {},
      userStartup: [],
      activations: [],
      mediaStamps: {},
    };
    planMock.mockReset().mockResolvedValue({ outcome: "planned", plan: refusedPlan } satisfies PlanResult);

    seedRemembered({
      "osinstall.mediaFolder": "E:\\media",
      "osinstall.destination": "E:\\dist",
    });
    render(<OsInstall />);

    const phrase = refusalPhrase(REFUSAL);
    const expectedSentence = i18n.t(phrase.key, phrase.params);
    // "workbench-base needs Workbench3.2, and no file in the media folder
    // carries that volume name." — a real sentence, computed the same way
    // the screen itself computes it, not a hand-typed copy that could drift.
    const rendered = await screen.findByText(expectedSentence);

    expect(rendered.textContent?.trim().length).toBeGreaterThan(0);
    expect(KEY_SHAPE.test(rendered.textContent ?? "")).toBe(false);
  });
});

describe("the screen says what a layering component would replace (ART-175)", () => {
  // `collide::preview` has been able to answer for a release recipe's own
  // component since ART-170, and nothing asked it. These are the ask, from
  // the screen's side: the request that goes out, and the rows that come
  // back.

  /** Switch to AmigaOS 3.9, whose fixture carries a layering component. */
  async function render39() {
    await renderFull();
    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");
    // The checklist is loaded, not hardcoded, so the 3.9 catalogue arrives on
    // its own round trip — wait for it rather than racing it.
    await waitFor(() =>
      expect(planMock).toHaveBeenCalledWith(expect.objectContaining({ release: "AmigaOS 3.9" }))
    );
  }

  it("asks about the layering components only, never the whole checklist", async () => {
    await render39();

    // The preview reads every file the components it is asked about would
    // place, off real install media — asking about all of them would mean
    // reading a whole AmigaOS install to answer a question about a few dozen
    // files. `workbench-base` declares no override and must not be in here.
    await waitFor(() => expect(componentCollisionsMock).toHaveBeenCalled());
    for (const [, components] of componentCollisionsMock.mock.calls as [InstallPlan, string[]][]) {
      expect(components).toEqual(["workbench-39"]);
    }
  });

  it("shows what would be replaced, and what would merely be placed", async () => {
    // Three files placed: one replaced, one already byte-for-byte identical,
    // one landing on nothing. The middle one is what review F4 was about —
    // `collide::preview` drops it, so counting `placed - reports.length` as
    // "new" would report two new files when there is one.
    componentCollisionsMock.mockResolvedValue({
      placed: 3,
      contested: 2,
      reports: [
        {
          path: "Libs/workbench.library",
          collision: { kind: "upgrade", from: "44.5", to: "45.1" },
          declared: true,
        },
      ],
    });

    await render39();

    await screen.findByText(i18n.t("osinstall.replaces.heading"));
    // The row itself, with the version change read off both files.
    await screen.findByText("Libs/workbench.library");
    expect(document.querySelectorAll('[data-testid="component-collision-row"]').length).toBe(1);

    // And the sentence that keeps "nothing to report" from reading like
    // "nothing to place": three files placed, one of them over something.
    const summary = i18n.t("osinstall.replaces.summary", {
      components: "AmigaOS3.9",
      placed: 3,
      fresh: 1,
      unchanged: 1,
      replaced: 1,
    });
    expect(document.body.textContent).toContain(summary);

    // And explicitly not the miscount: two new files, when one of the two is
    // a file that already exists byte-for-byte.
    expect(document.body.textContent).not.toContain(
      i18n.t("osinstall.replaces.summary", {
        components: "AmigaOS3.9",
        placed: 3,
        fresh: 2,
        unchanged: 0,
        replaced: 1,
      })
    );
  });

  it("says nothing at all when nothing layering is switched on", async () => {
    // AmigaOS 3.2's fixture has no component declaring an override, so the
    // section must not appear — and the engine must not be asked either.
    await renderFull();
    await waitFor(() => expect(planMock).toHaveBeenCalled());

    expect(screen.queryByText(i18n.t("osinstall.replaces.heading"))).toBeNull();
    expect(componentCollisionsMock).not.toHaveBeenCalled();
  });

  it("a preview that failed says so instead of looking like one that found nothing", async () => {
    componentCollisionsMock.mockRejectedValue(new Error("the disc could not be read"));

    await render39();

    // **No stray "Error: " any more** (ART-060). This used to render
    // `String(e)`, which on an `Error` object prepends the word "Error" to a
    // sentence that is already introduced as a failure — the user read it
    // twice. `errorText` takes the message.
    await screen.findByText(
      i18n.t("osinstall.replaces.failed", { error: "the disc could not be read" })
    );
    expect(document.querySelectorAll('[data-testid="component-collision-row"]').length).toBe(0);
  });
});

describe("a release switch does not leave the other release's answers on screen (ART-210)", () => {
  // The owner, driving the screen: "3.2 kurayım diyorsun, 3.9'un seçenekleri,
  // hataları vb ekranda duruyor asla değişmiyor."
  //
  // Only two effects on this screen depended on `release` — the one that
  // loads the component list and the one that re-plans. Everything *else* is
  // a `useState` nothing invalidated, so a finished install's report, a
  // failed preview, a plan error and a pending confirmation all survived a
  // switch and sat there describing an operating system the user had moved
  // away from. The plan updated underneath them, which made it worse: the
  // screen then showed one release's plan beside another release's result.

  /** Hand back the screen's own `osinstall-result` listener, so a finished
   *  install can be announced the way the backend announces one. */
  function captureAnnounce210(): { current: ((r: OsInstallResult) => void) | null } {
    const held: { current: ((r: OsInstallResult) => void) | null } = { current: null };
    onResultMock.mockImplementation((fn: (r: OsInstallResult) => void) => {
      held.current = fn;
      return Promise.resolve(() => {});
    });
    return held;
  }

  const FINISHED_39: OsInstallResult = {
    job_id: 1,
    destination: "E:\\amiga\\dist-3.9",
    outcome: { root: "E:\\amiga\\dist-3.9", files: 1915, directories: 75, bytes: 1024 },
  };

  it("takes a finished install's report down when the release changes", async () => {
    const announce = captureAnnounce210();
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);
    await waitFor(() => expect(announce.current).not.toBeNull());

    act(() => announce.current!(FINISHED_39));
    expect(await screen.findByText(i18n.t("osinstall.result.heading"))).toBeTruthy();

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");

    await waitFor(() =>
      expect(screen.queryByText(i18n.t("osinstall.result.heading"))).toBeNull()
    );
  });

  it("takes an answer a control gave down when the release changes", async () => {
    // A second card, and a different mechanism on purpose: the report above
    // is set by an event from the backend, this one by a button on the
    // screen. One test that clears one piece of state proves one piece of
    // state.
    //
    // "Scan again" is the right second case because **nothing else can
    // clear it**. A plan error would be wiped by the re-plan a release
    // switch triggers anyway, so a test built on one would pass against the
    // defect exactly as happily as against the fix.
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);
    await waitFor(() => expect(planMock).toHaveBeenCalled());

    await userEvent.click(
      screen.getByRole("button", { name: i18n.t("osinstall.media.rescan") })
    );
    const rescanned = i18n.t("osinstall.media.rescanned", { count: 1 });
    expect(await screen.findByText(rescanned)).toBeTruthy();

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");

    await waitFor(() => expect(screen.queryByText(rescanned)).toBeNull());
  });
});

describe("the tree it builds is the tree the next steps get (ART-197)", () => {
  /** Hand back the screen's own `osinstall-result` listener, so a finished
   *  install can be announced the way the backend announces one. */
  function captureAnnounce(): { current: ((r: OsInstallResult) => void) | null } {
    const held: { current: ((r: OsInstallResult) => void) | null } = { current: null };
    onResultMock.mockImplementation((fn: (r: OsInstallResult) => void) => {
      held.current = fn;
      return Promise.resolve(() => {});
    });
    return held;
  }

  const FINISHED: OsInstallResult = {
    job_id: 1,
    destination: "E:\\amiga\\dist-3.9",
    outcome: { root: "E:\\amiga\\dist-3.9", files: 1915, directories: 75, bytes: 1024 },
  };

  it("hands a finished install's destination to the session", async () => {
    // The defect: `OsInstall` remembered where it *wrote* under
    // `osinstall.destination`, and the panels beneath it read a different
    // key, `osinstall.packages.treeRoot`. Nothing joined them, so a user who
    // had just watched ART write 1915 files was asked to go and find them.
    const announce = captureAnnounce();
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);
    await waitFor(() => expect(announce.current).not.toBeNull());

    act(() => announce.current!(FINISHED));

    await waitFor(() => {
      const bag = useSettingsStore.getState().settings.remembered as Record<string, unknown>;
      expect(bag["buildSession.tree"]).toEqual({
        root: "E:\\amiga\\dist-3.9",
        builtHere: true,
      });
    });
  });

  it("says on screen which folder the next steps will act on", async () => {
    // A carry nobody can see is the same failure as a carry that does not
    // happen: the screen must not change what the next step points at
    // without saying so. This project's most expensive defects are the ones
    // that leave the reader not knowing what happened.
    const announce = captureAnnounce();
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);
    await waitFor(() => expect(announce.current).not.toBeNull());

    act(() => announce.current!(FINISHED));

    await screen.findByText(
      i18n.t("osinstall.result.carried", { root: "E:\\amiga\\dist-3.9" })
    );
    // Not the raw key, and not an unrendered interpolation.
    expect(screen.queryByText(/osinstall\.result\.carried/)).toBeNull();
    expect(screen.queryByText(/\{\{/)).toBeNull();
  });

  it("carries a tree the user picked by hand, without a build", async () => {
    // The migration's own case, at the screen: a user upgrading into this
    // build finds the packages panel pointing where they last pointed it.
    seedRemembered({
      ...FULL_FIELDS,
      "osinstall.packages.treeRoot": "E:\\amiga\\picked-by-hand",
    });
    render(<OsInstall />);

    // `PackagePanel` and `AmigaInstallPanel` each render the tree root through
    // `Field`, as plain text beside their Browse button — so the path appears
    // **twice**, once per panel, and that is the point: both are reading the
    // one session value. `findAllByText`, because `findByText` refuses a
    // multiple match.
    const shown = await screen.findAllByText("E:\\amiga\\picked-by-hand");
    expect(shown.length).toBe(2);
  });
});
