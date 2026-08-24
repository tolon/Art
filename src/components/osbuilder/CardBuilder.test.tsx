// @vitest-environment jsdom
//
// **This panel had no test file at all** — a survivor disclosed by ART-223 and
// carried on the work list since. `src/lib/cardBuild.test.ts` covers the pure
// half well (27 cases over `buildBlocker`, `healthVerdict`, `intakeFills`,
// the two partitions, PFS3's driver requirement); what nothing covered is the
// 974-line component that renders it, and a screen nobody has proved *mounts*
// is a screen whose every other guarantee is conditional.
//
// Mocked at the `@/lib/*` boundary, the house pattern (`OsInstall.test.tsx`,
// `useRomPairing.test.tsx`) — never `@tauri-apps/api` itself. `@/lib/settings`
// is mocked one layer further down for the reason `OsInstall.test.tsx`
// records: `useRemembered` writes through `useSettingsStore` on every tick and
// the real `saveSettings` rejects in jsdom with nothing to catch it, which
// Vitest counts as an unhandled rejection and fails the run.
//
// What this establishes:
//   1. The panel mounts past its heading, with the four questions and both
//      actions actually present.
//   2. Nothing on screen is a raw i18n key or an unrendered `{{…}}` — in
//      English **and** Turkish (ART-062, whose strings run longer).
//   3. **`SAFE_CREATE` is answered before the button, not by a job that
//      fails.** The panel's own module comment claims that in as many words;
//      this is the first thing to check it. A plan that says the destination
//      is already there must disable Build *and* say why on screen.
//   4. The blocker reaches the screen at all: with nothing chosen, Build is
//      disabled and the reason is rendered rather than left to a tooltip.
//
// The blocker sentences below are written out **literally** rather than
// fetched with `t()`. Asking i18next for them would make the test agree with
// whatever the catalogue says, including agreeing that a deleted key renders
// as its own name — which is the failure the file is here to catch. The cost
// is that rewording an English sentence fails this file, and that is the
// intended trade: a reworded blocker is a change somebody should see.
//
// What this does NOT establish: layout. jsdom measures nothing, so "does the
// Turkish sentence fit" is still a real-screen job — the same limit
// `OsInstall.test.tsx` records for ART-062.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { changeLanguage } from "@/i18n";
import type { CardBuildPlan } from "@/lib/cardBuild";
import { useSettingsStore } from "@/stores/settingsStore";

const planBuildMock = vi.hoisted(() => vi.fn());
const buildMock = vi.hoisted(() => vi.fn());
const checkImageMock = vi.hoisted(() => vi.fn());
const intakeMock = vi.hoisted(() => vi.fn());
const onResultMock = vi.hoisted(() => vi.fn());
const dialogOpenMock = vi.hoisted(() => vi.fn());
const dialogSaveMock = vi.hoisted(() => vi.fn());
const subscribeSafelyMock = vi.hoisted(() => vi.fn());
const outletContextMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/cardBuild", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/cardBuild")>()),
  cardPlanBuild: planBuildMock,
  cardBuild: buildMock,
  cardCheckImage: checkImageMock,
  cardIntake: intakeMock,
  onCardBuildResult: onResultMock,
}));

vi.mock("@/lib/jobs", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/jobs")>()),
  subscribeSafely: subscribeSafelyMock,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: dialogOpenMock,
  save: dialogSaveMock,
}));

// The panel reads the drop panel's analyses through the router. Mocking the
// hook rather than wrapping in a `MemoryRouter` keeps the test about this
// component instead of about routing.
vi.mock("react-router-dom", async (importOriginal) => ({
  ...(await importOriginal<typeof import("react-router-dom")>()),
  useOutletContext: outletContextMock,
}));

vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  getSettings: vi.fn(),
  saveSettings: vi.fn().mockResolvedValue(undefined),
}));

const { CardBuilder } = await import("@/components/osbuilder/CardBuilder");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");

beforeEach(() => {
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
});

afterEach(async () => {
  cleanup();
  vi.clearAllMocks();
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
  // Never let a Turkish test bleed its language into the next file's first
  // render.
  await changeLanguage("en");
});

/**
 * Answer the panel's questions the way a returning user already has.
 *
 * `useRemembered` reads through `useSettingsStore`, so seeding the store is
 * the same thing as the user having chosen these last time — which is what
 * makes the later blockers reachable at all. Without it every test stops at
 * the first one and `dest_exists` is unreachable, which is exactly how the
 * first version of the `SAFE_CREATE` test below came to assert nothing.
 */
function seedRemembered(overrides: Record<string, unknown>) {
  useSettingsStore.setState({
    loaded: true,
    settings: { ...DEFAULT_SETTINGS, remembered: { ...overrides } },
  });
}

/** Archive and destination chosen; the filesystem left at its FFS default,
 *  which carries its driver in Kickstart and so does not block. */
const ANSWERED = {
  "cardBuilder.archive": "E:\amiga\Emu68-pistorm-20240101.zip",
  "cardBuilder.dest": "E:\amiga\card.img",
};

function mount() {
  outletContextMock.mockReturnValue({});
  onResultMock.mockReturnValue(Promise.resolve(() => {}));
  subscribeSafelyMock.mockReturnValue(undefined);
  intakeMock.mockResolvedValue([]);
  return render(<CardBuilder />);
}

/**
 * A real plan, the same shape `src/lib/cardBuild.test.ts` pins — the panel
 * renders the layout as soon as one arrives, so a thinner stand-in throws
 * rather than testing anything.
 */
function planWith(destExists: boolean): CardBuildPlan {
  const slot = (index: number, start_lba: number, sector_count: number) => ({
    index,
    kind: index === 1 ? ({ kind: "fat32" } as const) : ({ kind: "amiga-rdb" } as const),
    type_byte: index === 1 ? 0x0c : 0x76,
    bootable: index === 1,
    start_lba,
    sector_count,
  });
  return {
    layout: {
      total_sectors: 4194304,
      boot: slot(1, 2048, 2299904),
      areas: [slot(2, 2301952, 1892352)],
    },
    boot_files: [
      { name: "Emu68-pistorm.gz", bytes: 1_000_000 },
      { name: "config.txt", bytes: 512 },
    ],
    kernel_file: "Emu68-pistorm.gz",
    kickstart_file: null,
    rom: null,
    warnings: [],
    dest_exists: destExists,
  };
}

describe("the card builder mounts", () => {
  it("renders its heading and both actions", async () => {
    mount();
    // Several: the panel renders a heading per section (the questions, the
    // plan, the result, the health check). That there is more than one is
    // itself the point — a screen stuck at its first heading is what
    // ART-118 describes on the install step.
    await waitFor(() => {
      expect(screen.getAllByRole("heading", { level: 2 }).length).toBeGreaterThan(1);
    });
    // The two the module comment names: everything else is behind Advanced.
    expect(screen.getByRole("button", { name: /preview/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /build/i })).toBeTruthy();
  });

  it("says why Build is unavailable rather than only disabling it", async () => {
    mount();
    const build = await screen.findByRole("button", { name: /build/i });
    expect((build as HTMLButtonElement).disabled).toBe(true);
    // Nothing has been chosen, so the first blocker is the archive — and the
    // sentence has to be *on screen*, not only in a `title` a mouse finds.
    //
    // **The whole sentence, not a word of it.** The first version of this
    // matched `/Emu68|archive/i`, and deleting the on-screen `<span>` while
    // leaving the `title=` did not fail it: those words are all over the
    // panel's own labels. A guard that a real defect walks past is not a
    // guard, so this asks for the string the blocker actually renders.
    const sentence = "Choose the Emu68 release archive first.";
    await waitFor(() => {
      expect(document.body.textContent).toContain(sentence);
    });
    expect(build.getAttribute("title")).toBe(sentence);
  });
});

describe("SAFE_CREATE is answered before the button", () => {
  it("refuses a destination that already exists, on screen", async () => {
    // The panel's own module comment claims `SAFE_CREATE` is settled before
    // the button rather than by a job that fails. To check that, the earlier
    // blockers have to be out of the way and a plan has to exist — so this
    // answers the questions and presses Preview, which is what a person does.
    seedRemembered(ANSWERED);
    planBuildMock.mockResolvedValue(planWith(true));
    mount();

    const preview = await screen.findByRole("button", { name: /preview/i });
    await waitFor(() => expect((preview as HTMLButtonElement).disabled).toBe(false));
    await userEvent.click(preview);
    await waitFor(() => expect(planBuildMock).toHaveBeenCalled());

    // The plan came back saying the file is already there. Nothing has been
    // written, and nothing may be.
    const sentence =
      "That file is already there. ART will not build over it; choose another name.";
    await waitFor(() => {
      expect(document.body.textContent).toContain(sentence);
    });
    const build = screen.getByRole("button", { name: /build/i });
    expect((build as HTMLButtonElement).disabled).toBe(true);
    expect(buildMock).not.toHaveBeenCalled();
  });

  it("lets Build through once the destination is free", async () => {
    // The other arm. Without it the test above passes on a panel whose Build
    // button is disabled for some entirely different reason — or always.
    seedRemembered(ANSWERED);
    planBuildMock.mockResolvedValue(planWith(false));
    mount();

    const preview = await screen.findByRole("button", { name: /preview/i });
    await waitFor(() => expect((preview as HTMLButtonElement).disabled).toBe(false));
    await userEvent.click(preview);
    await waitFor(() => expect(planBuildMock).toHaveBeenCalled());

    const build = screen.getByRole("button", { name: /build/i });
    await waitFor(() => expect((build as HTMLButtonElement).disabled).toBe(false));
    expect(document.body.textContent).not.toContain("already there");
  });
});

describe("what a person actually reads", () => {
  for (const language of ["en", "tr"] as const) {
    it(`carries no raw key and no unrendered interpolation in ${language}`, async () => {
      await changeLanguage(language);
      mount();
      await waitFor(() => {
        expect(screen.getAllByRole("heading", { level: 2 }).length).toBeGreaterThan(1);
      });
      const text = document.body.textContent ?? "";
      // A missing key renders as the key itself; a missing variable renders
      // as a literal `{{name}}`. Both are what this catches.
      expect(text).not.toMatch(/cardBuilder\.[a-zA-Z0-9.]+/);
      expect(text).not.toMatch(/\{\{[^}]+\}\}/);
      expect(text.length).toBeGreaterThan(40);
    });
  }
});
