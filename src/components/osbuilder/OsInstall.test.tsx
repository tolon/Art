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
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";

import { changeLanguage } from "@/i18n";
import { useSettingsStore } from "@/stores/settingsStore";
import type {
  InstallPlan,
  InstallRequest,
  MediaScanResult,
  PlanItem,
  PlanResult,
  RefusalReason,
} from "@/lib/osinstall";
import type { RomInfo } from "@/lib/pistorm";

const scanMediaMock = vi.hoisted(() => vi.fn());
const planMock = vi.hoisted(() => vi.fn());
const applyMock = vi.hoisted(() => vi.fn());
const verifyMock = vi.hoisted(() => vi.fn());
const onResultMock = vi.hoisted(() => vi.fn());
const identifyRomMock = vi.hoisted(() => vi.fn());
const dialogOpenMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallScanMedia: scanMediaMock,
  osinstallPlan: planMock,
  osinstallApply: applyMock,
  osinstallVerify: verifyMock,
  onOsInstallResult: onResultMock,
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
      // ROM is V47 here, so "modules-a1200" (conditionMajor: 47) is not
      // forced on — the "condition-off" reasoning branch, not "rom-needed".
      componentsOn: ["workbench-base", "install-libs", ...req.chosen],
      mediaPaths: { "Workbench3.2": "E:\\media\\Disk1.adf" },
      userStartup: [],
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

const FULL_FIELDS = {
  "osinstall.mediaFolder": "E:\\media",
  "osinstall.rom": "E:\\roms\\kick.rom",
  "osinstall.destination": "E:\\dist",
  "osinstall.chosen": [],
  "osinstall.excludedConditional": [],
};

beforeEach(() => {
  scanMediaMock.mockReset().mockResolvedValue({
    outcome: "found",
    media: [{ path: "E:\\media\\Disk1.adf", volumeName: "Workbench3.2", kind: "floppy" }],
  } satisfies MediaScanResult);
  planMock.mockReset().mockImplementation((req: InstallRequest) => Promise.resolve(planResultFor(req)));
  applyMock.mockReset().mockResolvedValue(1);
  verifyMock.mockReset();
  onResultMock.mockReset().mockResolvedValue(() => {});
  identifyRomMock.mockReset().mockResolvedValue(ROM);
  dialogOpenMock.mockReset().mockResolvedValue(null);
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
});

/** Media folder, ROM and destination all set — the state the browser probe
 *  could never reach — then waits for the live plan preview to land. */
async function renderFull() {
  seedRemembered(FULL_FIELDS);
  const utils = render(<OsInstall />);
  await waitFor(() => expect(planMock).toHaveBeenCalled());
  await screen.findByText(i18n.t("osinstall.plan.heading"));
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
    // more than one entry, matching the 26-component shipped recipe.
    const checkboxes = screen.getAllByRole("checkbox");
    expect(checkboxes.length).toBeGreaterThan(1);

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

    expect(document.body.textContent).toContain("1 items,");

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
    await waitFor(() => expect(document.body.textContent).toContain("2 items,"));
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
});

describe("a disc dropped on the panel", () => {
  it("takes the folder from a disc dropped on the panel", async () => {
    // A JSX attribute string literal does not process `\\` as a JS escape
    // sequence the way a normal string literal does (unlike the call below,
    // which is an ordinary function argument) — passed as a bare attribute,
    // the brief's literal path would arrive with doubled backslashes. The
    // `{...}` expression form is what makes this an actual JS string.
    render(<OsInstall droppedMedia={"E:\\amiga\\Amigatolon\\iso\\AmigaOS39.iso"} />);
    await waitFor(() =>
      expect(scanMediaMock).toHaveBeenCalledWith("E:\\amiga\\Amigatolon\\iso")
    );
  });
});

describe("a refusal renders as a sentence, not a blank", () => {
  it("shows the real, translated refusal text", async () => {
    const refusedPlan: InstallPlan = {
      release: "3.2",
      items: [],
      refusals: [REFUSAL],
      totalBytes: 0,
      componentsOn: ["workbench-base", "install-libs"],
      mediaPaths: {},
      userStartup: [],
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
