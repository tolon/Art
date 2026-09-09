// @vitest-environment jsdom
//
// The OS Builder's steps, as routes.
//
// What this file is for: a step navigated to **on its own** must act on what
// the session holds, or *ask* — never render an empty card and never throw.
// That is the design's fourth named mutation, and the reason the steps are
// sub-routes at all: the owner's verdict on the single scrolling column was
// "çok karmaşık gereksiz derecede uzun".
//
// The panels are replaced by markers. Each reaches Tauri on mount, and this
// file is about routing and what a step hands its panel — `FilesTab.test.tsx`
// and the two panel test files cover the real components.

import { useMemo, useState, type ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import i18n from "i18next";

// Side-effecting: a real, synchronously-initialised i18next instance, so a
// case that names a sentence names the sentence and not the key.
import "@/i18n";

// ART-199: the steps now ask ART what the folder is. Mocked at the `@/lib`
// wrapper, the boundary this suite mocks at everywhere else.
const describeTreeMock = vi.hoisted(() => vi.fn());
// Round 3 fix wave: `StepSecim` reads the chain's tree, so `useChainTree`'s
// own `useDestinationCheck` runs here too and asks both questions. Mocked
// beside `describe_tree` for the same reason it is everywhere else — the
// `@/lib` wrapper is this suite's boundary, never `@tauri-apps/api`.
const destinationTakenMock = vi.hoisted(() => vi.fn());
vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallDescribeTree: describeTreeMock,
  osinstallDestinationTaken: destinationTakenMock,
}));

vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  getSettings: vi.fn(),
  saveSettings: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@/components/osbuilder/ChoiceTab", () => ({
  ChoiceTab: () => <div data-testid="choice-tab" />,
}));
vi.mock("@/components/osbuilder/CardBuilder", () => ({
  CardBuilder: () => <div data-testid="card" />,
}));
vi.mock("@/components/osbuilder/VolumePreload", () => ({
  VolumePreload: () => <div data-testid="volumes" />,
}));
// ART-197 wave 3: this used to be rendered by the install step. It is mocked
// like every other panel — what this file is about is *which step renders it*.
vi.mock("@/components/osbuilder/VerifyAgainstCard", () => ({
  VerifyAgainstCard: () => <div data-testid="verify" />,
}));
// Round 2, task 2: `makine` now mounts a real component. Mocked like every
// other panel — `MachineTab.test.tsx` owns what it renders; what this file is
// about is which step renders it.
vi.mock("@/components/osbuilder/MachineTab", () => ({
  MachineTab: () => <div data-testid="machine-tab" />,
}));
// Round 4, task 4: `derle` mounts the real tab. Mocked like every other
// panel — `BuildTab.test.tsx` owns what it renders; what this file is about
// is which step renders it.
//
// **It takes the run lock, because that is a thing the shell draws** (round 4
// whole-branch review, I2). The real tab sets the lock while `useBuildRun` is
// running; the marker sets it when a case asks for a run in flight, so what
// is under test here stays the shell's own strip rather than the tab's
// sequencer.
const lockedRun = vi.hoisted(() => ({ value: false }));
vi.mock("@/components/osbuilder/BuildTab", async () => {
  const { useEffect } = await import("react");
  const { useRunLock } = await import("@/pages/osbuilder/runLock");
  return {
    BuildTab: () => {
      const { setRunning } = useRunLock();
      useEffect(() => {
        setRunning(lockedRun.value);
        return () => setRunning(false);
      }, [setRunning]);
      return <div data-testid="build-tab" />;
    },
  };
});
// The bar the shell mounts on every tab of this lane needs no mock of its
// own: since round 4 task 5 it reads the session and asks nothing, so
// mounting it here reaches no `invoke`. (It used to compute a plan, a
// destination check and the chain through `buildSummary`, which this file
// mocked out for exactly that reason.) What is under test here is that the
// bar is mounted at all, and by which route.
vi.mock("@/components/osbuilder/FilesTab", () => ({
  FilesTab: ({ droppedMedia }: { droppedMedia?: { path: string } | null }) => (
    <div data-testid="files-tab">{droppedMedia?.path ?? "(no drop)"}</div>
  ),
}));

const { useSettingsStore } = await import("@/stores/settingsStore");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");
const { OsBuilder } = await import("@/pages/OsBuilder");
// The application's own route table, not a copy of it: a redirect that is
// deleted fails a case here rather than silently sending a remembered URL to
// the home screen through the `*` catch-all.
const { osBuilderRoutes } = await import("@/pages/osbuilder/routes");
const { STEP_IDS } = await import("@/lib/buildSteps");
const { RunLockContext } = await import("@/pages/osbuilder/runLock");

function seed(remembered: Record<string, unknown>) {
  useSettingsStore.setState({
    loaded: true,
    settings: { ...DEFAULT_SETTINGS, remembered },
  });
}

/**
 * The shell's half of the run lock (round 5, task 3).
 *
 * The provider used to be `OsBuilder`'s own, so rendering the lane rendered
 * the lock with it. It lives in `Layout` now — the sidebar has to go dead
 * too, and the sidebar is not inside this screen — which means a test that
 * mounts `OsBuilder` under a bare router is mounting it under the *default*
 * lock: off, and un-settable. This harness is the piece of `Layout` that
 * matters here, and nothing more: one settable `running`.
 */
function RunLockHarness({ children }: { children: ReactNode }) {
  const [running, setRunning] = useState(false);
  const value = useMemo(() => ({ running, setRunning }), [running]);
  return <RunLockContext.Provider value={value}>{children}</RunLockContext.Provider>;
}

function renderAt(path: string, state?: unknown) {
  return render(
    <MemoryRouter initialEntries={[{ pathname: path, state }]}>
      <RunLockHarness>
        <Routes>
          <Route path="/os-builder" element={<OsBuilder />}>
            {osBuilderRoutes()}
          </Route>
        </Routes>
      </RunLockHarness>
    </MemoryRouter>
  );
}

beforeEach(() => {
  // Nothing is running unless a case says so: a lock that were on by default
  // would make every other case in this file assert about a locked strip
  // without meaning to.
  lockedRun.value = false;
  // Answers "yes, a tree" unless a test says otherwise, so the cases that are
  // not about ART-199 are unaffected by it.
  destinationTakenMock.mockReset().mockResolvedValue(false);
  describeTreeMock.mockReset().mockResolvedValue({
    isTree: true,
    release: "AmigaOS 3.9",
    files: 1915,
    components: ["workbench-base"],
    amigaInstalled: [],
    problem: null,
  });
});

afterEach(() => {
  cleanup();
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
});

describe("a step opened on its own", () => {
  it("renders its own list rather than asking, when there is a tree", () => {
    // The tab reads the destination itself since round 3 task 3, so what
    // this step still owns is the banner — and what it has to prove is that
    // a step with a tree draws the list and says nothing over it.
    seed({
      "buildSession.kind": "install",
      "buildSession.tree": { root: "E:\\dist", builtHere: true },
    });
    renderAt("/os-builder/secim");
    expect(screen.getByTestId("choice-tab")).toBeTruthy();
    expect(screen.queryByTestId("step-asks-tree")).toBeNull();
  });

  it("asks rather than rendering empty when there is no tree", () => {
    // The design's fourth mutation: a step navigated to cold must *ask*,
    // never throw and never render a blank card.
    seed({ "buildSession.kind": "install" });
    renderAt("/os-builder/secim");

    expect(screen.getByTestId("choice-tab")).toBeTruthy();
    // A rendered sentence, not the raw key — asserting on the key would pass
    // on the very failure this catches, a missing catalogue entry.
    expect(screen.getByText(/AmigaOS tree/i)).toBeTruthy();
    expect(screen.queryByText(/osBuilder\.step\./)).toBeNull();
  });

  it("asks on the choice tab too, and does not gate it", () => {
    // Optional stays optional: asking is a state, not a refusal. The panel is
    // still mounted and still usable.
    seed({ "buildSession.kind": "install" });
    renderAt("/os-builder/secim");

    expect(screen.getByText(/AmigaOS tree/i)).toBeTruthy();
    expect(screen.getByTestId("choice-tab")).toBeTruthy();
  });

  it("does not ask on a step that never reads a tree", () => {
    seed({ "buildSession.kind": "boot-card" });
    renderAt("/os-builder/kart");

    expect(screen.getByTestId("card")).toBeTruthy();
    expect(screen.queryByTestId("step-asks-tree")).toBeNull();
  });
});

/**
 * **The banner judges the tree the tab's list works on** (round 3 fix wave,
 * Critical 1). `ChoiceTab` reads `useChainTree(destination)`; `StepSecim`
 * read `session.tree.root`. With the destination pointed at an ART tree and
 * nothing in the session, the list answered every row against that tree while
 * the banner above it said none had been chosen — and sent the user to a
 * picker deleted this round.
 */
describe("the secim banner judges the chain's tree, not the session's copy", () => {
  it("says nothing when the destination is the tree, and the session holds none", async () => {
    seed({
      "buildSession.kind": "install",
      "buildSession.release": "AmigaOS 3.9",
      "osinstall.destination.AmigaOS 3.9": "E:\\dist39",
    });
    renderAt("/os-builder/secim");

    await screen.findByTestId("choice-tab");
    // Waited on properly rather than asserted on the first frame: the banner
    // is gated on `settled`, so "absent" has to survive the round trip
    // landing, not merely precede it.
    await waitFor(() => expect(describeTreeMock).toHaveBeenCalled());
    expect(screen.queryByTestId("step-asks-tree")).toBeNull();
    expect(screen.queryByTestId("step-wrong-folder")).toBeNull();
  });

  it("says nothing for a session tree with no destination, as it always did", async () => {
    seed({
      "buildSession.kind": "install",
      "buildSession.tree": { root: "E:\\dist", builtHere: true },
    });
    renderAt("/os-builder/secim");

    await screen.findByTestId("choice-tab");
    await waitFor(() => expect(describeTreeMock).toHaveBeenCalled());
    expect(screen.queryByTestId("step-asks-tree")).toBeNull();
    expect(screen.queryByTestId("step-wrong-folder")).toBeNull();
  });

  it("asks, and sends the user to tab 3, when there is neither", async () => {
    seed({ "buildSession.kind": "install" });
    renderAt("/os-builder/secim");

    const asks = await screen.findByTestId("step-asks-tree");
    expect(asks.textContent).toContain(i18n.t("osBuilder.step.asksTree"));
    // The link the sentence promises. It used to go to the files tab, under
    // a sentence that said "below" — where `PackagePanel`'s picker was.
    expect(within(asks).getByRole("link").getAttribute("href")).toBe("/os-builder/makine");
    expect(within(asks).getByRole("link").textContent).toBe(i18n.t("osBuilder.step.makine"));
  });

  it("draws no banner at all until the destination check has landed", async () => {
    // A destination that *is* a build looks like no tree for a render or two.
    // Flashing "choose a destination" over a list about to answer against one
    // is the confident wrong sentence in miniature.
    //
    // The gate has to *delay* the banner, not suppress it — so the same case
    // holds the destination in flight, asserts silence, then lets it answer
    // "not a build" and requires the banner to arrive.
    let answer: (summary: unknown) => void = () => {};
    describeTreeMock.mockReturnValue(
      new Promise((resolve) => {
        answer = resolve;
      })
    );
    seed({
      "buildSession.kind": "install",
      "buildSession.release": "AmigaOS 3.9",
      "osinstall.destination.AmigaOS 3.9": "E:\\dist39",
    });
    renderAt("/os-builder/secim");

    expect(screen.queryByTestId("step-asks-tree")).toBeNull();

    answer({
      isTree: false,
      release: null,
      files: 0,
      components: [],
      amigaInstalled: [],
      problem: "holds no distribution.json",
    });
    // No session tree behind it, so the chain has no tree at all and the
    // honest sentence is "choose one" — arriving once, and only once ART has
    // stopped looking.
    await screen.findByTestId("step-asks-tree");
  });
});

describe("the progress strip", () => {
  it("shows the steps this kind has, and not the others", () => {
    seed({
      "buildSession.kind": "install",
      "buildSession.tree": { root: "E:\\dist", builtHere: true },
    });
    renderAt("/os-builder/secim");

    // `install` is the hedef chip plus four numbered tabs, and no card step.
    expect(screen.getAllByRole("link").length).toBe(5);
    expect(screen.queryByTestId("card")).toBeNull();
    expect(screen.getByRole("link", { name: /^1\. Amiga files/ })).toBeTruthy();
    expect(screen.getByRole("link", { name: /^4\. Build/ })).toBeTruthy();
    // The chip is not a numbered tab: numbering it would say the kind is a
    // step of the build rather than the choice the build begins from.
    expect(screen.queryByRole("link", { name: /^1\. What are we building/ })).toBeNull();
    expect(screen.queryByRole("link", { name: /card image/i })).toBeNull();
  });

  it("shows the card job's own steps instead", () => {
    seed({ "buildSession.kind": "boot-card" });
    renderAt("/os-builder/kart");

    expect(screen.getAllByRole("link").length).toBe(2);
    expect(screen.getByRole("link", { name: /card image/i })).toBeTruthy();
  });

  it("draws the chip alone for the distro kind, which has no numbered tab", () => {
    seed({ "buildSession.kind": "distro" });
    renderAt("/os-builder/hedef");
    const links = screen.getAllByRole("link");
    expect(links).toHaveLength(1);
    expect(links[0].getAttribute("data-testid")).toBe("strip-hedef");
  });
});

describe("the build bar is mounted by the shell", () => {
  it("is under the install lane's tabs and not under the card lane", () => {
    seed({ "buildSession.kind": "install", "buildSession.tree": { root: "E:\\dist", builtHere: true } });
    const install = renderAt("/os-builder/secim");
    expect(screen.getByTestId("build-bar")).toBeTruthy();
    install.unmount();

    seed({ "buildSession.kind": "boot-card" });
    renderAt("/os-builder/kart");
    expect(screen.queryByTestId("build-bar")).toBeNull();
  });
});

describe("a disc dropped on the panel", () => {
  it("reaches the media step rather than stopping at the shell", () => {
    // The drop workflow routes to `/os-builder` carrying the file in router
    // state. Under sub-routes the shell has to carry it on to the step that
    // acts on it, or the whole "disc dropped on the panel offers the OS
    // Builder" path does nothing visible.
    seed({ "buildSession.kind": "boot-card" });
    renderAt("/os-builder", { path: "E:\\amiga\\iso\\AmigaOS39.iso" });

    expect(screen.getByTestId("files-tab").textContent).toBe("E:\\amiga\\iso\\AmigaOS39.iso");
  });
});

describe("a folder that is not a tree (ART-199)", () => {
  it("says so at the step instead of leaving it to a refusal on the button", async () => {
    // The owner pointed this step at their own AmigaOS folder. It showed no
    // warning at all, and the refusal arrived when they pressed run — seven
    // times, as their operation log records.
    describeTreeMock.mockResolvedValue({
      isTree: false,
      release: null,
      files: 0,
      components: [],
      amigaInstalled: [],
      problem: "holds no distribution.json",
    });
    seed({
      "buildSession.kind": "install",
      "buildSession.tree": { root: "E:\\amiga\\os39", builtHere: false },
    });
    renderAt("/os-builder/secim");

    const said = await screen.findByTestId("step-wrong-folder");
    expect(said.textContent).toMatch(/distribution\.json/);
    expect(screen.queryByText(/osBuilder\.step\./)).toBeNull();
  });

  it("says nothing about a folder that is a tree", async () => {
    seed({
      "buildSession.kind": "install",
      "buildSession.tree": { root: "E:\dist", builtHere: true },
    });
    renderAt("/os-builder/secim");
    await screen.findByTestId("choice-tab");
    expect(screen.queryByTestId("step-wrong-folder")).toBeNull();
  });

  it("does not accuse the folder when ART could not look at all", async () => {
    // A failed round trip is not evidence about the user's folder.
    describeTreeMock.mockRejectedValue(new Error("no IPC bridge"));
    seed({
      "buildSession.kind": "install",
      "buildSession.tree": { root: "E:\dist", builtHere: true },
    });
    renderAt("/os-builder/secim");
    await screen.findByTestId("choice-tab");
    expect(screen.queryByTestId("step-wrong-folder")).toBeNull();
  });
});

describe("Verify against a card is on the volumes step (ART-197 wave 3)", () => {
  /// **Where it belongs, rather than where it was built.** It compares a
  /// distribution tree against a volume that already *exists*, so everything
  /// it needs is here: the card is on this step. On the install step it was a
  /// section asking for a card image on a screen whose whole job is to produce
  /// a folder.
  it("renders it under the volumes panel", () => {
    seed({});
    renderAt("/os-builder/birimler");
    expect(screen.getByTestId("volumes")).toBeTruthy();
    expect(screen.getByTestId("verify")).toBeTruthy();
    // Fix round 1, Minor: unmocked like `NetworkPanel` beside it, so this is
    // the one route-level check that a deleted `<AppearancePanel />` in
    // `StepBirimler` would actually fail.
    expect(screen.getByTestId("appearance-panel")).toBeTruthy();
  });

  /// The other half, and the one that would make the move a loss if it were
  /// wrong: it is not on the card *builder* step either. That step creates an
  /// image; this compares against one that has been written.
  it("is not on the card step, which builds an image rather than checking one", () => {
    seed({});
    renderAt("/os-builder/kart");
    expect(screen.getByTestId("card")).toBeTruthy();
    expect(screen.queryByTestId("verify")).toBeNull();
  });

  /// And not on the step it came from. `FilesTab.test.tsx` asserts the same
  /// thing from the other side, against the real component rather than a
  /// marker.
  it("is not on the install step any more", () => {
    seed({});
    renderAt("/os-builder/dosyalar");
    expect(screen.getByTestId("files-tab")).toBeTruthy();
    expect(screen.queryByTestId("verify")).toBeNull();
  });
});

describe("the retired routes still resolve (four-tab design § 2)", () => {
  // A URL that stopped resolving is a link in the operation log, a remembered
  // position and a habit that now goes nowhere; the `*` catch-all would send
  // it home, which is the confident-wrong form of "this moved". So the guard
  // asserts the specific tab, never "something rendered".
  beforeEach(() => {
    seed({
      "buildSession.kind": "install",
      "buildSession.tree": { root: "E:\\dist", builtHere: true },
    });
  });

  it("sends kaynak to the files tab", () => {
    renderAt("/os-builder/kaynak");
    expect(screen.getByTestId("files-tab")).toBeTruthy();
    expect(
      screen.getByRole("link", { name: /^1\. Amiga files/ }).getAttribute("aria-current")
    ).toBe("page");
  });

  it("sends paketler, amiga-kurulum and ilk-acilis to the choice tab", () => {
    for (const old of ["paketler", "amiga-kurulum", "ilk-acilis"]) {
      const view = renderAt(`/os-builder/${old}`);
      expect(screen.getByTestId("choice-tab")).toBeTruthy();
      expect(
        screen.getByRole("link", { name: /^2\. What to install/ }).getAttribute("aria-current")
      ).toBe("page");
      view.unmount();
    }
  });
});

describe("every step id in the lane has a route", () => {
  // A step id with no row in the route table would give the strip a link
  // that falls through to the `*` catch-all and lands on the home screen —
  // the confident-wrong form of "this moved".
  it("renders each step's own content, never the home screen", () => {
    seed({
      "buildSession.kind": "install",
      "buildSession.tree": { root: "E:\\dist", builtHere: true },
    });
    const expected: Record<string, string> = {
      dosyalar: "files-tab",
      // The tab's **own** list. Written this way in round 3 task 2, while
      // `secim` still carried two panels below `ChoiceTab`, precisely so
      // that task 3 removing them could not go on passing here over a tab
      // that rendered nothing of its own — which is what asserting on
      // `packages` would have done. Task 3 removed them; this row did not
      // have to change.
      secim: "choice-tab",
      makine: "machine-tab",
      derle: "build-tab",
      kart: "card",
      birimler: "volumes",
    };
    for (const id of STEP_IDS) {
      if (id === "hedef") continue;
      const view = renderAt(`/os-builder/${id}`);
      expect(screen.getByTestId(expected[id])).toBeTruthy();
      view.unmount();
    }
  });
});

// **Both "a field of mine is elsewhere" cases are gone** (round 4, task 4).
// `makine names the files tab for the keymap it does not hold yet` and
// `derle names the files tab` described a lane in which two tabs pointed at a
// third; the keymap select is on `makine` now and the Build button is on
// `derle`, so both sentences and both keys were deleted rather than left to
// say something that had become false. What replaces them is the row in the
// completeness map above: `derle` renders `build-tab`, and `makine` renders
// `machine-tab`, whose own file proves the select is there.
describe("every tab of the install lane holds its own fields", () => {
  beforeEach(() => seed({ "buildSession.kind": "install" }));

  it("points nowhere else for a field of its own", () => {
    for (const id of ["makine", "derle"]) {
      const view = renderAt(`/os-builder/${id}`);
      expect(screen.queryByTestId("tab-makine-keymap-note")).toBeNull();
      expect(screen.queryByTestId("tab-derle")).toBeNull();
      view.unmount();
    }
  });
});

// ---------------------------------------------------------------------------
// The strip while a build is running (round 4 whole-branch review, I2)
// ---------------------------------------------------------------------------
//
// The sequencer is `BuildTab`'s own state, so leaving tab 4 mid-run unmounts
// it and the rest of the phases — and the ART-197 hand-off of the finished
// tree — simply stop. Nothing said so, before or after: the run was there and
// then it was not. The shell refuses the navigation instead, and says why.
describe("the strip is locked while a build runs", () => {
  it("draws the tabs as dead chips, not links, and says why", async () => {
    lockedRun.value = true;
    seed({ "buildSession.kind": "install" });
    renderAt("/os-builder/derle");
    await screen.findByTestId("build-tab");

    // **Not links at all.** A `NavLink` styled to look disabled still
    // navigates on Enter, on a middle click and through a screen reader —
    // what has to go is the element that navigates.
    await waitFor(() => expect(screen.queryAllByRole("link")).toHaveLength(0));
    const chip = screen.getByTestId("strip-hedef");
    expect(chip.tagName).toBe("SPAN");
    expect(chip.getAttribute("aria-disabled")).toBe("true");
    // …and the chip still says what it is: a closed destination, not a
    // control that has vanished.
    expect(chip.textContent).toBe(i18n.t("osBuilder.what.install"));

    // A refusal must be actionable, and this one names the control that
    // lifts it — Stop, on the tab the person is already looking at.
    expect(screen.getByTestId("strip-locked").textContent).toBe(
      i18n.t("osBuilder.build.navigationLocked")
    );
  });

  it("leaves the strip alone when nothing is running", async () => {
    // The control, measured rather than assumed: a strip that were always
    // dead would pass the case above and prove nothing.
    seed({ "buildSession.kind": "install" });
    renderAt("/os-builder/derle");
    await screen.findByTestId("build-tab");

    expect(screen.getAllByRole("link").length).toBeGreaterThan(1);
    expect(screen.getByTestId("strip-hedef").tagName).toBe("A");
    expect(screen.queryByTestId("strip-locked")).toBeNull();
  });
});
