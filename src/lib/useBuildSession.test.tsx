// @vitest-environment jsdom
//
// The session's React half, tested where it actually runs.
//
// `@/lib/settings` is mocked one layer below the hook for the reason
// `OsInstall.test.tsx` documents: `useRemembered`'s setter calls
// `saveSettings` — the real `tauri-plugin-store` IPC boundary — and fires the
// promise without catching it. Left real, that rejects in jsdom with nothing
// to catch it, which Vitest counts as an unhandled rejection and fails the
// run.

import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  getSettings: vi.fn(),
  saveSettings: vi.fn().mockResolvedValue(undefined),
}));

const { useSettingsStore } = await import("@/stores/settingsStore");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");
const { useBuildSession } = await import("@/lib/useBuildSession");

function seed(remembered: Record<string, unknown>) {
  useSettingsStore.setState({
    loaded: true,
    settings: { ...DEFAULT_SETTINGS, remembered },
  });
}

function Probe() {
  const { session, setTree, setRom, setCard } = useBuildSession();
  return (
    <div>
      <span data-testid="rom">{session.rom.path ?? "(none)"}</span>
      <button onClick={() => setRom("E:\\roms\\chosen.rom")}>choose rom</button>
      <span data-testid="card">{session.card.image ?? "(none)"}</span>
      <button onClick={() => setCard("E:\\amiga\\built.img")}>write card</button>
      <span data-testid="root">{session.tree.root ?? "(none)"}</span>
      <span data-testid="builtHere">{String(session.tree.builtHere)}</span>
      <span data-testid="chosen">{session.components.chosen.join(",")}</span>
      <span data-testid="release">{session.release}</span>
      <span data-testid="kind">{session.kind}</span>
      <span data-testid="mediaFolder">{session.media.folder ?? "(none)"}</span>
      <button onClick={() => setTree({ root: "E:\\picked", builtHere: false })}>pick</button>
    </div>
  );
}

/** A second panel. Nothing connects it to `Probe` but the session itself. */
function OtherPanel() {
  const { session } = useBuildSession();
  return (
    <>
      <span data-testid="other-rom">{session.rom.path ?? "(none)"}</span>
      <span data-testid="other-card">{session.card.image ?? "(none)"}</span>
    </>
  );
}

afterEach(() => {
  cleanup();
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
});

describe("useBuildSession", () => {
  it("hands the packages step the tree ART last wrote, with nothing wired by hand", () => {
    // ART-197 in one assertion: the user never picked a tree, and the step
    // that needs one is handed the folder ART wrote into.
    seed({ "osinstall.destination": "E:\\amiga\\dist-3.9" });
    render(<Probe />);
    expect(screen.getByTestId("root").textContent).toBe("E:\\amiga\\dist-3.9");
  });

  it("leaves a tree the user picked by hand exactly where they put it", () => {
    seed({
      "osinstall.destination": "E:\\amiga\\dist-3.9",
      "osinstall.packages.treeRoot": "E:\\amiga\\somewhere-else",
    });
    render(<Probe />);
    expect(screen.getByTestId("root").textContent).toBe("E:\\amiga\\somewhere-else");
  });

  it("writes the user's pick into the session's own key and leaves the legacy one", async () => {
    seed({ "osinstall.destination": "E:\\amiga\\dist-3.9" });
    render(<Probe />);
    await userEvent.click(screen.getByRole("button", { name: "pick" }));
    const bag = useSettingsStore.getState().settings.remembered as Record<string, unknown>;
    expect(bag["buildSession.tree"]).toEqual({ root: "E:\\picked", builtHere: false });
    // The legacy key is left as it was — a rollback to an earlier ART must
    // still find the user's folder where that version looks for it.
    expect(bag["osinstall.destination"]).toBe("E:\\amiga\\dist-3.9");
  });

  it("reads the components of the release it is on", () => {
    seed({
      "buildSession.release": "AmigaOS 3.9",
      "osinstall.chosen": ["workbench-base"],
      "osinstall.chosen.AmigaOS 3.9": ["os39-base"],
    });
    render(<Probe />);
    expect(screen.getByTestId("release").textContent).toBe("AmigaOS 3.9");
    expect(screen.getByTestId("chosen").textContent).toBe("os39-base");
  });

  it("carries the other keys it took over", () => {
    seed({
      "osBuilder.kind": "prepare-volumes",
      "osinstall.mediaFolder": "E:\\media\\3.9",
    });
    render(<Probe />);
    expect(screen.getByTestId("kind").textContent).toBe("prepare-volumes");
    expect(screen.getByTestId("mediaFolder").textContent).toBe("E:\\media\\3.9");
  });

  it("hands back the same section object when nothing changed", () => {
    // ART-178/ART-195: a fresh identity per render turns this screen's
    // effects into a loop — 2,149 preview jobs in one session, each walking a
    // 468 MB ISO. `useRememberedShape` stabilises; this proves the facade
    // does not undo that by rebuilding on top of it.
    seed({ "osinstall.destination": "E:\\dist" });
    const seen: unknown[] = [];
    function Identity() {
      const { session } = useBuildSession();
      seen.push(session.tree);
      return null;
    }
    const { rerender } = render(<Identity />);
    rerender(<Identity />);
    expect(seen.length).toBeGreaterThanOrEqual(2);
    expect(seen[0]).toBe(seen[1]);
  });
});

describe("one Kickstart for the build (ART-197's fourth row)", () => {
  /// **The point of the row.** Three panels asked for the same ROM and each
  /// remembered its own, so a user chose it three times. Choosing it in one
  /// place now shows it in the other, with nothing wired between them.
  it("a ROM chosen in one panel is the ROM the next panel already has", async () => {
    seed({});
    render(
      <>
        <Probe />
        <OtherPanel />
      </>
    );
    expect(screen.getByTestId("other-rom").textContent).toBe("(none)");

    await userEvent.click(screen.getByText("choose rom"));

    expect(screen.getByTestId("rom").textContent).toBe("E:\\roms\\chosen.rom");
    expect(screen.getByTestId("other-rom").textContent).toBe(
      "E:\\roms\\chosen.rom",
      );
  });

  /// The migration, from the panel that would otherwise have lost it: a user
  /// who only ever ran a package installer never touched `osinstall.rom`.
  it("finds a ROM a user only ever chose on the Amiga-side install step", () => {
    seed({ "amigaInstall.kickstart": "E:\\roms\\kick31.rom" });
    render(<Probe />);
    expect(screen.getByTestId("rom").textContent).toBe("E:\\roms\\kick31.rom");
  });

  it("and one they only ever chose on the card step", () => {
    seed({ "cardBuilder.kickstart": "E:\\roms\\kick47.rom" });
    render(<Probe />);
    expect(screen.getByTestId("rom").textContent).toBe("E:\\roms\\kick47.rom");
  });
});

describe("one card for the build (ART-197's remaining duplicate)", () => {
  /// **The defect, in one assertion.** The card builder wrote an image and the
  /// volumes step asked the user to go and find it. Writing it in one panel
  /// now shows it in the other, with nothing wired between them.
  it("a card written in one panel is the card the next panel already has", async () => {
    seed({});
    render(
      <>
        <Probe />
        <OtherPanel />
      </>
    );
    expect(screen.getByTestId("other-card").textContent).toBe("(none)");

    await userEvent.click(screen.getByText("write card"));

    expect(screen.getByTestId("card").textContent).toBe("E:\\amiga\\built.img");
    expect(screen.getByTestId("other-card").textContent).toBe("E:\\amiga\\built.img");
  });

  /// The migration ART-197 is actually about: this user never picked a card on
  /// the volumes step, because nothing ever told them they had to.
  it("hands the volumes step the image the card builder last wrote", () => {
    seed({ "cardBuilder.dest": "E:\\amiga\\card.img" });
    render(<Probe />);
    expect(screen.getByTestId("card").textContent).toBe("E:\\amiga\\card.img");
  });

  /// And the other direction, which the order exists to protect: a card
  /// somebody went and chose is not moved by this.
  it("leaves a card the user picked by hand exactly where they put it", () => {
    seed({
      "cardBuilder.dest": "E:\\amiga\\card.img",
      "preload.image": "E:\\amiga\\somewhere-else.img",
    });
    render(<Probe />);
    expect(screen.getByTestId("card").textContent).toBe("E:\\amiga\\somewhere-else.img");
  });
});
