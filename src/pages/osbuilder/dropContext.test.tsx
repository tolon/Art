// @vitest-environment jsdom
//
// **The drop context, through the real router** (round 4 final review, C1 and
// I1).
//
// Every other case about the per-row drop mocks `useOutletContext` and hands
// the section a context object directly — which proves what the section does
// with a context it is given, and nothing at all about whether it is given
// one. It was not: `OsBuilder` rendered `<Outlet />` with no `context` prop,
// and react-router's `useOutlet` wraps the child in
// `OutletContext.Provider value={context}` **unconditionally**, so
// `undefined` shadowed the Layout's value for everything below the OS
// Builder's own outlet. The measured conversion, the row attribute and the
// hit test were all correct and the wire between them was missing, in the one
// place no test looked.
//
// So this file mocks **no** router API. It renders `osBuilderRoutes()` under a
// `Layout`-shaped provider — the same `<Outlet context={…}>` shape
// `Layout.tsx` uses, typed by the one exported `DropContext` both ends now
// name — and asserts that a drop reaches a partition row. A second case
// asserts the I1 rule beside it: moving the pointer after a drop, which is
// what `dropPosition` (but not `lastDrop`) changes on, adds nothing.
//
// `cardRowAt` is the one thing mocked, for `CardSection.test.tsx`'s own
// reason: jsdom's `document.elementFromPoint` always answers `null`, so the
// hit test proves nothing here. The conversion is measured in
// `experiment-drop-coordinates.md` and unit-tested in `dropTarget.test.ts`.

import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Outlet, Route, Routes } from "react-router-dom";

import { changeLanguage } from "@/i18n";
import type { DropContext, LastDrop } from "@/lib/dropContext";
import { useSettingsStore } from "@/stores/settingsStore";

const measureMock = vi.hoisted(() => vi.fn());
const onMeasureMock = vi.hoisted(() => vi.fn());
const classifyMock = vi.hoisted(() => vi.fn());
const checkNameMock = vi.hoisted(() => vi.fn());
const cardRowAtMock = vi.hoisted(() => vi.fn());
const dialogOpenMock = vi.hoisted(() => vi.fn());
const dialogSaveMock = vi.hoisted(() => vi.fn());
const imageBytesMock = vi.hoisted(() => vi.fn());
const identifyRomMock = vi.hoisted(() => vi.fn());
const amigaForeverMock = vi.hoisted(() => vi.fn());
const takenMock = vi.hoisted(() => vi.fn());
const describeTreeMock = vi.hoisted(() => vi.fn());
const componentsMock = vi.hoisted(() => vi.fn());
const layersForMock = vi.hoisted(() => vi.fn());
const planMock = vi.hoisted(() => vi.fn());
const collisionsMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/cardOs", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/cardOs")>()),
  cardOsMeasure: measureMock,
  onCardOsMeasureResult: onMeasureMock,
  cardOsClassify: classifyMock,
  cardOsCheckVolumeName: checkNameMock,
}));

vi.mock("@/lib/dropTarget", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/dropTarget")>()),
  cardRowAt: cardRowAtMock,
}));

vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallDestinationTaken: takenMock,
  osinstallDescribeTree: describeTreeMock,
  osinstallComponents: componentsMock,
  layersFor: layersForMock,
  osinstallPlan: planMock,
  osinstallComponentCollisions: collisionsMock,
}));

vi.mock("@/lib/cardBuild", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/cardBuild")>()),
  cardImageBytes: imageBytesMock,
}));

vi.mock("@/lib/pistorm", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/pistorm")>()),
  pistormIdentifyRom: identifyRomMock,
}));

vi.mock("@/lib/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/api")>()),
  hostAmigaForeverFolders: amigaForeverMock,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: dialogOpenMock,
  save: dialogSaveMock,
}));

vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  getSettings: vi.fn(),
  saveSettings: vi.fn().mockResolvedValue(undefined),
}));

const { OsBuilder } = await import("@/pages/OsBuilder");
const { osBuilderRoutes } = await import("@/pages/osbuilder/routes");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");

const RELEASE = "AmigaOS 3.9";

/** The one handle a case uses to make a drop happen — set by the shell. */
let drop: ((next: LastDrop | null) => void) | null = null;
let movePointer: ((at: { x: number; y: number } | null) => void) | null = null;

/**
 * `Layout`'s own shape, and nothing else of `Layout`.
 *
 * The real shell brings a sidebar, a job bar, the settings store's zoom and
 * the scratch-root gate with it, none of which this is about. What must be
 * identical is the **provision**: one `<Outlet context={…}>` carrying a
 * `DropContext`, exactly as `Layout.tsx:230` does.
 */
function ShellShapedProvider() {
  const [lastDrop, setLastDrop] = useState<LastDrop | null>(null);
  const [dropPosition, setDropPosition] = useState<{ x: number; y: number } | null>(null);
  drop = setLastDrop;
  movePointer = setDropPosition;
  const context: DropContext = {
    analyses: lastDrop ? lastDrop.paths.map((path) => ({ path }) as never) : [],
    dragOver: false,
    dropPosition,
    lastDrop,
  };
  return <Outlet context={context} />;
}

function seedRemembered(overrides: Record<string, unknown>) {
  useSettingsStore.setState({
    loaded: true,
    settings: { ...DEFAULT_SETTINGS, remembered: { ...overrides } },
  });
}

function seedCardSession() {
  seedRemembered({
    "buildSession.release": RELEASE,
    "buildSession.kind": "install",
    [`osinstall.destinationKind.${RELEASE}`]: "card-image",
    [`buildSession.material.${RELEASE}`]: { folders: [] },
    [`osinstall.cardTarget.${RELEASE}`]: {
      sizeGb: 64,
      image: null,
      emu68Archive: null,
      pfs3Driver: null,
      partitions: [
        { name: "System", sources: [] },
        { name: "Games", sources: [] },
        { name: "Work", sources: [] },
      ],
    },
  });
}

/** The real route tree, under a shell that provides the drop context. */
function renderTree() {
  return render(
    <MemoryRouter initialEntries={["/os-builder/makine"]}>
      <Routes>
        <Route element={<ShellShapedProvider />}>
          <Route path="os-builder" element={<OsBuilder />}>
            {osBuilderRoutes()}
          </Route>
        </Route>
      </Routes>
    </MemoryRouter>
  );
}

beforeEach(() => {
  drop = null;
  movePointer = null;
  measureMock.mockReset().mockResolvedValue(7);
  onMeasureMock.mockReset().mockResolvedValue(() => {});
  classifyMock
    .mockReset()
    .mockImplementation((paths: string[]) =>
      Promise.resolve(paths.map((path) => ({ path, kind: { kind: "folder" }, why: null })))
    );
  checkNameMock.mockReset().mockResolvedValue({ ok: true });
  cardRowAtMock.mockReset().mockReturnValue(null);
  dialogOpenMock.mockReset();
  dialogSaveMock.mockReset();
  imageBytesMock.mockReset().mockResolvedValue(60_800_000_000);
  identifyRomMock.mockReset().mockResolvedValue(null);
  amigaForeverMock.mockReset().mockResolvedValue([]);
  takenMock.mockReset().mockResolvedValue(false);
  describeTreeMock.mockReset().mockResolvedValue({ exists: false, entries: 0 });
  componentsMock.mockReset().mockResolvedValue([]);
  layersForMock.mockReset().mockResolvedValue([]);
  planMock.mockReset().mockResolvedValue(null);
  collisionsMock.mockReset().mockResolvedValue([]);
});

afterEach(async () => {
  cleanup();
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
  await changeLanguage("en");
});

describe("a drop reaches a partition row through the real route tree (C1)", () => {
  it("adds the dropped path to the row under the pointer", async () => {
    seedCardSession();
    renderTree();

    await screen.findByTestId("card-row-Games");
    expect(screen.queryByTestId("card-row-sources-Games")).toBeNull();

    // The pointer is over Games, and one folder is dropped there.
    cardRowAtMock.mockReturnValue(1);
    await act(async () => {
      drop!({ paths: ["E:\\demos"], at: { x: 120, y: 240 }, seq: 1 });
    });

    await waitFor(() =>
      expect(within(screen.getByTestId("card-row-sources-Games")).getByText("demos")).toBeTruthy()
    );
  });
});

describe("only a drop adds a source, never a drag over one (I1)", () => {
  it("adds nothing when the pointer moves after an earlier drop", async () => {
    seedCardSession();
    renderTree();
    await screen.findByTestId("card-row-Games");

    cardRowAtMock.mockReturnValue(1);
    await act(async () => {
      drop!({ paths: ["E:\\demos"], at: { x: 120, y: 240 }, seq: 1 });
    });
    await waitFor(() =>
      expect(within(screen.getByTestId("card-row-sources-Games")).getByText("demos")).toBeTruthy()
    );

    // Now merely drag across the *next* row: `dropPosition` changes, the
    // analyses do not, and nothing was dropped.
    cardRowAtMock.mockReturnValue(2);
    await act(async () => {
      movePointer!({ x: 120, y: 300 });
    });

    const target = useSettingsStore.getState().settings.remembered as Record<string, unknown>;
    const card = target[`osinstall.cardTarget.${RELEASE}`] as {
      partitions: { name: string; sources: string[] }[];
    };
    expect(card.partitions.map((p) => p.sources)).toEqual([[], ["E:\\demos"], []]);
  });
});
