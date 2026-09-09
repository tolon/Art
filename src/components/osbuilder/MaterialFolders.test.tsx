// @vitest-environment jsdom
//
// The folder column of the source step (simplification design § 4): the
// list, its tags, Add/Remove, the Amiga Forever offer and the guide buttons,
// cut out of `OsInstall.tsx` so the readout can come first. This component
// owns no session state — every change goes back to the step through a
// callback — so what is tested here is that each control names the folder
// it acts on and calls out with it, and that the guide block (moved from
// `MaterialReadout.tsx`) still keeps its three endings apart.
//
// Mocked at the `@/lib/*` boundary the rest of this suite mocks at.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";

import "@/i18n";
import { changeLanguage } from "@/i18n";
import type { MaterialFolder } from "@/lib/buildSession";

const guideMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallWriteMaterialGuide: guideMock,
}));

const { MaterialFolders } = await import("@/components/osbuilder/MaterialFolders");
type Props = import("@/components/osbuilder/MaterialFolders").MaterialFoldersProps;

const OS39 = "E:\\amiga\\os39";
const one: MaterialFolder[] = [{ path: OS39, layer: null }];

function renderFolders(over: Partial<Props> = {}) {
  const props: Props = {
    release: "AmigaOS 3.9",
    folders: one,
    layers: [],
    layerLabel: (layer) => layer.labelKey ?? layer.id,
    folderScans: {},
    wrongLayerHint: () => null,
    unusedForPlan: [],
    amigaForeverOffer: null,
    foundVolumeNames: [],
    onAdd: vi.fn(),
    onRemove: vi.fn(),
    onTag: vi.fn(),
    onAmigaForeverAdd: vi.fn(),
    onAmigaForeverDismiss: vi.fn(),
    ...over,
  };
  return { ...render(<MaterialFolders {...props} />), props };
}

beforeEach(() => {
  guideMock.mockReset();
});

afterEach(async () => {
  cleanup();
  await changeLanguage("en");
});

describe("the list", () => {
  it("renders one row per folder, and Remove names the folder it removes", async () => {
    const { props } = renderFolders({
      folders: [{ path: "E:\\first", layer: null }, { path: "E:\\second", layer: null }],
    });
    expect(screen.getAllByTestId("material-folder")).toHaveLength(2);
    await userEvent.setup().click(
      screen.getByRole("button", {
        name: i18n.t("osinstall.media.removeFolderAriaLabel", { folder: "E:\\second" }),
      })
    );
    expect(props.onRemove).toHaveBeenCalledWith("E:\\second");
    expect(props.onRemove).toHaveBeenCalledTimes(1);
  });

  it("says 'no folder chosen' with an empty list, and draws no row and no guide", () => {
    renderFolders({ folders: [] });
    expect(screen.getByText(i18n.t("osinstall.media.none"))).toBeTruthy();
    expect(screen.queryAllByTestId("material-folder")).toHaveLength(0);
    expect(screen.queryByTestId("material-guide")).toBeNull();
  });

  it("Add asks the step, and opens no dialog of its own", async () => {
    const { props } = renderFolders();
    await userEvent.setup().click(screen.getByTestId("material-add-folder"));
    expect(props.onAdd).toHaveBeenCalledTimes(1);
  });

  it("offers a layer select only for a layered release, named by the folder, and tags through the step", async () => {
    const layers = [
      { id: "base", labelKey: null },
      { id: "update", labelKey: null },
    ];
    const { props } = renderFolders({ layers });
    const select = screen.getByRole("combobox", {
      name: i18n.t("osinstall.material.layerAriaLabel", { folder: OS39 }),
    });
    await userEvent.setup().selectOptions(select, "update");
    expect(props.onTag).toHaveBeenCalledWith(OS39, "update");

    cleanup();
    renderFolders({ layers: [] });
    expect(screen.queryByRole("combobox")).toBeNull();
  });

  it("names a folder ART could not read, and describes that row's controls by it (ART-241)", () => {
    renderFolders({
      folderScans: { [OS39]: { outcome: "folder-unreadable", folder: OS39 } },
    });
    const line = screen.getByTestId("material-folder-unreadable-0");
    expect(line.textContent).toBe(i18n.t("osinstall.media.unreadable"));
    const remove = screen.getByRole("button", {
      name: i18n.t("osinstall.media.removeFolderAriaLabel", { folder: OS39 }),
    });
    expect(remove.getAttribute("aria-describedby")).toContain("material-folder-unreadable-0");
  });

  it("shows the wrong-layer hint the step computed, under its own row", () => {
    renderFolders({
      folders: [{ path: OS39, layer: "base" }],
      layers: [{ id: "base", labelKey: null }],
      wrongLayerHint: (entry) => (entry.path === OS39 ? "looks like the update" : null),
    });
    expect(screen.getByTestId("layer-wrong-hint-0").textContent).toBe("looks like the update");
  });

  it("names the folders a layered plan will not read", () => {
    renderFolders({ unusedForPlan: ["E:\\spare"] });
    expect(screen.getByTestId("material-unused").textContent).toContain("E:\\spare");
  });
});

describe("what the folders hold (ART-256)", () => {
  it("says what was found, under the list and before the guide", () => {
    renderFolders({ foundVolumeNames: ["Workbench3.1", "Install3.1"] });
    const found = document.getElementById("osinstall-media-found");
    expect(found).toBeTruthy();
    expect(found!.textContent).toBe(
      i18n.t("osinstall.media.found", { count: 2, names: "Workbench3.1, Install3.1" })
    );
    const guide = screen.getByTestId("material-guide");
    expect(found!.compareDocumentPosition(guide) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("says nothing was found while a folder is there to read, and neither line with no folders", () => {
    renderFolders({ foundVolumeNames: [] });
    const empty = document.getElementById("osinstall-media-empty");
    expect(empty).toBeTruthy();
    expect(empty!.textContent).toBe(i18n.t("osinstall.media.empty"));
    expect(document.getElementById("osinstall-media-found")).toBeNull();

    cleanup();
    renderFolders({ folders: [], foundVolumeNames: [] });
    expect(document.getElementById("osinstall-media-empty")).toBeNull();
    expect(document.getElementById("osinstall-media-found")).toBeNull();
  });
});

describe("Amiga Forever, offered — never added here", () => {
  it("shows the path, and both buttons only call the step", async () => {
    const { props } = renderFolders({ folders: [], amigaForeverOffer: "E:\\amiga\\Shared\\adf" });
    const offer = screen.getByTestId("amiga-forever-offer");
    expect(offer.textContent).toContain("E:\\amiga\\Shared\\adf");
    const user = userEvent.setup();
    await user.click(
      within(offer).getByRole("button", { name: i18n.t("osinstall.material.amigaForeverAdd") })
    );
    expect(props.onAmigaForeverAdd).toHaveBeenCalledTimes(1);
    expect(props.onAmigaForeverAdd).toHaveBeenCalledWith("E:\\amiga\\Shared\\adf");
    await user.click(
      within(offer).getByRole("button", { name: i18n.t("osinstall.material.amigaForeverDismiss") })
    );
    expect(props.onAmigaForeverDismiss).toHaveBeenCalledTimes(1);
    // Nothing rendered a row: adding is the step's, from the click.
    expect(screen.queryAllByTestId("material-folder")).toHaveLength(0);
  });

  it("renders no offer when there is none", () => {
    renderFolders({ folders: [] });
    expect(screen.queryByTestId("amiga-forever-offer")).toBeNull();
  });
});

// Moved from `MaterialReadout.test.tsx` on 2026-09-09, by name, with the
// guide block itself (simplification design § 4.2). The five cases are the
// same five; only the component under test changed.
describe("the list, written into the folder (design § 3.7)", () => {
  /// **Nothing is written until somebody asks.** The button is the ask; the
  /// list rendering is not. ART does not write into a user's folder because
  /// they pointed at it — `remembered.ts`'s rule about the user's own
  /// settings, applied to the user's own disk.
  it("writes nothing at all until the button is pressed", () => {
    renderFolders();
    screen.getByTestId("material-guide");
    expect(guideMock).not.toHaveBeenCalled();
  });

  it("writes one guide per folder, in the folder that button names", async () => {
    guideMock.mockResolvedValue({
      state: "written",
      path: "E:\\second\\ART - what goes here.txt",
    });
    renderFolders({
      folders: [{ path: "E:\\first", layer: null }, { path: "E:\\second", layer: null }],
    });

    const buttons = screen.getAllByTestId("material-guide-write");
    expect(buttons).toHaveLength(2);
    await userEvent.setup().click(buttons[1]);

    // The folder that button belongs to, the chosen release, and the UI's own
    // language — the filename is the guide's own data and never travels.
    await waitFor(() =>
      expect(guideMock).toHaveBeenCalledWith("E:\\second", "AmigaOS 3.9", "en")
    );
    expect(
      await screen.findByText(
        i18n.t("osinstall.material.guide.written", {
          path: "E:\\second\\ART - what goes here.txt",
        })
      )
    ).toBeTruthy();
    // And only that folder's line says anything: a "written" line under a
    // folder ART did not touch is the screen claiming what it did not do.
    expect(screen.queryAllByTestId("material-guide-written")).toHaveLength(1);
  });

  /// `SAFE_CREATE`, said as its own ending. "Already there" and "ART could
  /// not write it" are two different next steps — delete a file, or something
  /// is wrong — and collapsing them is this project's named defect.
  it("keeps 'already there' apart from a failure, and names the file to delete", async () => {
    guideMock.mockResolvedValue({
      state: "alreadyThere",
      path: "E:\\amiga\\os39\\ART - what goes here.txt",
    });
    renderFolders();
    await userEvent.setup().click(screen.getByTestId("material-guide-write"));

    const line = await screen.findByTestId("material-guide-alreadyThere");
    expect(line.textContent).toBe(
      i18n.t("osinstall.material.guide.alreadyThere", {
        path: "E:\\amiga\\os39\\ART - what goes here.txt",
      })
    );
    expect(screen.queryByTestId("material-guide-failed")).toBeNull();
    expect(screen.queryByTestId("material-guide-written")).toBeNull();
  });

  it("says a real failure is ART's, not a statement about the folder", async () => {
    guideMock.mockRejectedValue(new Error("access is denied"));
    renderFolders();
    await userEvent.setup().click(screen.getByTestId("material-guide-write"));

    const line = await screen.findByTestId("material-guide-failed");
    expect(line.textContent).toContain("access is denied");
    expect(screen.queryByTestId("material-guide-written")).toBeNull();
  });

  /// The language the *user* is reading in decides which guide is written —
  /// the Rust side owns the filename and the words, and this is the one thing
  /// the screen has to get right about it.
  it("asks for the guide in the language on screen", async () => {
    await changeLanguage("tr");
    guideMock.mockResolvedValue({
      state: "written",
      path: "E:\\amiga\\os39\\ART - buraya ne konur.txt",
    });
    renderFolders();
    await userEvent.setup().click(screen.getByTestId("material-guide-write"));

    await waitFor(() =>
      expect(guideMock).toHaveBeenCalledWith(OS39, "AmigaOS 3.9", "tr")
    );
    expect(
      await screen.findByText(
        i18n.t("osinstall.material.guide.written", {
          path: "E:\\amiga\\os39\\ART - buraya ne konur.txt",
        })
      )
    ).toBeTruthy();
  });
});
