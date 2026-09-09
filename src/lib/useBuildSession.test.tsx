// @vitest-environment jsdom
//
// The session's React half, tested where it actually runs.
//
// `@/lib/settings` is mocked one layer below the hook for the reason
// `FilesTab.test.tsx` documents: `useRemembered`'s setter calls
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
  const { session, setTree, setRom, setCard, setFirstBoot } = useBuildSession();
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
      <span data-testid="firstboot-written">{String(session.firstboot.written)}</span>
      <span data-testid="firstboot-wanted">{String(session.firstboot.wanted)}</span>
      <button onClick={() => setTree({ root: "E:\\picked", builtHere: false })}>pick</button>
      <button onClick={() => setTree({ root: "E:\\other", builtHere: false })}>pick other</button>
      <button onClick={() => setTree({ builtHere: true })}>mark built</button>
      <button onClick={() => setFirstBoot({ written: true })}>first boot written</button>
      <button onClick={() => setFirstBoot({ wanted: false })}>first boot not wanted</button>
    </div>
  );
}

/**
 * The material list and the archives folder, side by side — F10's own two
 * values, which are one folder wearing two hats.
 */
function MaterialProbe() {
  const { session, setMaterial, setPackages } = useBuildSession();
  return (
    <div>
      <span data-testid="material">
        {session.material.folders.map((entry) => entry.path).join(",") || "(none)"}
      </span>
      <span data-testid="packagesFolder">{session.packages.folder ?? "(none)"}</span>
      <button onClick={() => setPackages({ folder: "E:\\archives" })}>choose archives</button>
      <button
        onClick={() =>
          setMaterial(
            session.material.folders.filter((entry) => entry.path !== "E:\\archives")
          )
        }
      >
        remove archives
      </button>
      <button
        onClick={() =>
          setMaterial(session.material.folders.filter((entry) => entry.path !== "E:\\disks"))
        }
      >
        remove disks
      </button>
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

describe("first boot's written flag belongs to one tree", () => {
  // The value moving to a different tree is ART-197's own defect running the
  // other way round: a screen would say "already written" about a folder
  // that has never carried a first-boot block.
  it("resets to false when the tree's root changes", async () => {
    seed({
      "buildSession.tree": { root: "E:\\amiga\\dist", builtHere: false },
      "buildSession.firstboot": { written: true },
    });
    render(<Probe />);
    expect(screen.getByTestId("firstboot-written").textContent).toBe("true");

    await userEvent.click(screen.getByRole("button", { name: "pick" }));

    expect(screen.getByTestId("root").textContent).toBe("E:\\picked");
    expect(screen.getByTestId("firstboot-written").textContent).toBe("false");
  });

  // The other arm: a `setTree` call that leaves `root` untouched must not
  // clobber a flag nothing here is claiming to change.
  it("survives a setTree call that does not touch the root", async () => {
    seed({
      "buildSession.tree": { root: "E:\\amiga\\dist", builtHere: false },
      "buildSession.firstboot": { written: true },
    });
    render(<Probe />);

    await userEvent.click(screen.getByRole("button", { name: "mark built" }));

    expect(screen.getByTestId("builtHere").textContent).toBe("true");
    expect(screen.getByTestId("root").textContent).toBe("E:\\amiga\\dist");
    expect(screen.getByTestId("firstboot-written").textContent).toBe("true");
  });

  /**
   * **`written` is a fact about a folder; `wanted` is the user's own tick**
   * (round 3 fix wave, Minor 12). `setTree` resets one and must not touch the
   * other: a user who untucked the first-boot row on tab 2 and then pointed
   * the build at a different tree would find it silently ticked again —
   * *nothing changes unless the user changes it*, broken by the reset that
   * exists for the other field entirely.
   */
  it("resets written but keeps the user's own wanted tick when the root changes", async () => {
    seed({
      "buildSession.tree": { root: "E:\\amiga\\dist", builtHere: false },
      "buildSession.firstboot": { written: true },
    });
    render(<Probe />);

    await userEvent.click(screen.getByRole("button", { name: "first boot not wanted" }));
    expect(screen.getByTestId("firstboot-wanted").textContent).toBe("false");

    await userEvent.click(screen.getByRole("button", { name: "pick other" }));

    expect(screen.getByTestId("root").textContent).toBe("E:\\other");
    expect(screen.getByTestId("firstboot-written").textContent).toBe("false");
    expect(screen.getByTestId("firstboot-wanted").textContent).toBe("false");
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

describe("a folder taken out of the material list is out of the build (F10)", () => {
  /// **The defect.** `packages.folder` keeps a stored value of its own as
  /// well as being a list entry, because `AmigaInstallPanel` hands one folder
  /// to `osinstallCollisions` and `osinstallAddPackage`. So removing that
  /// folder from the list left the stored copy behind, and the step said two
  /// things at once: the Amiga Forever offer — drawn only while the list is
  /// empty, so ART is claiming to have nothing — directly above the package
  /// panels (two of them then; the retired one went in round 3 of the
  /// four-tab rewrite) still reading archives out of the folder just
  /// removed.
  it("drops the stored archives folder when the list stops holding it", async () => {
    seed({ "buildSession.material.AmigaOS 3.2": { folders: [{ path: "E:\\disks", layer: null }] } });
    render(<MaterialProbe />);

    await userEvent.click(screen.getByText("choose archives"));
    expect(screen.getByTestId("packagesFolder").textContent).toBe("E:\\archives");
    expect(screen.getByTestId("material").textContent).toBe("E:\\disks,E:\\archives");

    await userEvent.click(screen.getByText("remove archives"));

    expect(screen.getByTestId("material").textContent).toBe("E:\\disks");
    // Not "E:\archives" any more, and not nothing either: with the stored
    // value gone the view falls back to the list's first untagged folder,
    // which is what a user who never kept a separate archives folder has.
    expect(screen.getByTestId("packagesFolder").textContent).toBe("E:\\disks");
  });

  /// The other half, and the one that decides *where* the fix goes. When
  /// nothing is stored, `packages.folder` is a **view** onto the list's first
  /// untagged entry and follows a removal by itself. Writing `null` into the
  /// store there would create a stored value the user never made and switch
  /// that view off for good — a setting changing without the user changing
  /// it, which is the rule this exists to keep.
  it("writes nothing at all when the folder was only ever the list's own", async () => {
    seed({
      "buildSession.material.AmigaOS 3.2": {
        folders: [
          { path: "E:\\disks", layer: null },
          { path: "E:\\second", layer: null },
        ],
      },
    });
    render(<MaterialProbe />);
    expect(screen.getByTestId("packagesFolder").textContent).toBe("E:\\disks");

    await userEvent.click(screen.getByText("remove disks"));

    expect(screen.getByTestId("packagesFolder").textContent).toBe("E:\\second");
    const bag = useSettingsStore.getState().settings.remembered as Record<string, unknown>;
    expect(bag["buildSession.packages"]).toBeUndefined();
    expect(bag["buildSession.packages.AmigaOS 3.2"]).toBeUndefined();
  });

  /// **M3, the whole-branch review's own reproduction.** `material` is per
  /// release and `packages.folder` was global, so the guard above compared a
  /// global value against *one release's* list: build 3.9 with `E:\archives`
  /// stored, switch to 3.2.2 (whose list seeds that folder too), remove it
  /// there because it holds no 3.2 media, and 3.9's archives folder is
  /// silently repointed at its disks folder. F1's defect through F1's fix.
  it("leaves another release's archives folder alone when a folder is removed here", async () => {
    seed({
      "buildSession.release": "AmigaOS 3.9",
      "buildSession.material.AmigaOS 3.9": {
        folders: [
          { path: "E:\\disks", layer: null },
          { path: "E:\\archives", layer: null },
        ],
      },
      "buildSession.packages.AmigaOS 3.9": { folder: "E:\\archives", chosen: [] },
      "buildSession.material.AmigaOS 3.2.2": {
        folders: [{ path: "E:\\archives", layer: null }],
      },
      "buildSession.packages.AmigaOS 3.2.2": { folder: "E:\\archives", chosen: [] },
    });
    const { unmount } = render(<MaterialProbe />);
    expect(screen.getByTestId("packagesFolder").textContent).toBe("E:\\archives");

    // Building 3.2.2, and its list loses the archives folder.
    useSettingsStore.setState((s) => ({
      settings: {
        ...s.settings,
        remembered: {
          ...(s.settings.remembered as Record<string, unknown>),
          "buildSession.release": "AmigaOS 3.2.2",
        },
      },
    }));
    unmount();
    render(<MaterialProbe />);
    await userEvent.click(screen.getByText("remove archives"));

    const bag = useSettingsStore.getState().settings.remembered as Record<string, unknown>;
    expect((bag["buildSession.packages.AmigaOS 3.2.2"] as { folder?: string }).folder).toBeNull();
    // **3.9's is untouched**, and the user changed nothing about 3.9.
    expect((bag["buildSession.packages.AmigaOS 3.9"] as { folder?: string }).folder).toBe(
      "E:\\archives"
    );
  });

  /// A folder the removal did not name keeps its stored value. The guard that
  /// makes the test above an answer rather than a coincidence: a fix that
  /// simply cleared the folder on every list edit would pass it.
  it("keeps the stored folder when some other folder is removed", async () => {
    seed({
      "buildSession.material.AmigaOS 3.2": {
        folders: [
          { path: "E:\\disks", layer: null },
          { path: "E:\\archives", layer: null },
        ],
      },
      "buildSession.packages": { folder: "E:\\archives", chosen: [] },
    });
    render(<MaterialProbe />);

    await userEvent.click(screen.getByText("remove disks"));

    expect(screen.getByTestId("material").textContent).toBe("E:\\archives");
    // The **store**, not only the view: with the list down to one folder the
    // derived value would answer "E:\archives" whether the stored one
    // survived or not, so asserting the screen alone would pass for a fix
    // that cleared the folder on every list edit.
    const bag = useSettingsStore.getState().settings.remembered as Record<string, unknown>;
    // The **per-release** key the session writes (round 2 review, M3);
    // the global one below it is only the migration source.
    const stored = (bag["buildSession.packages.AmigaOS 3.2"] ??
      bag["buildSession.packages"]) as { folder?: string };
    expect(stored.folder).toBe("E:\\archives");
    expect(screen.getByTestId("packagesFolder").textContent).toBe("E:\\archives");
  });
});
