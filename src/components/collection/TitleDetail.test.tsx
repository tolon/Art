// @vitest-environment jsdom
//
// The detail panel's two behaviours that live in the component rather than in
// `@/lib` — both of them ART-144, both of them invisible to a pure test.
//
//   #2. A hand-attached picture rewrites `overrides.json`, the file holding
//       every correction the user has ever made to their catalogue.
//       `set_override` goes through `core/safety`'s `guarded_write` and hands
//       back where it preserved the previous version; both commands used to
//       drop that on the floor, which made them the only override writers in
//       the collection that did. The rule is CLAUDE.md's: a write that takes
//       a backup tells the user where it put it.
//   #3. `loadPictures()` is two round trips and nothing serialises them, so
//       clicking down a list faster than they resolve let a slow answer for a
//       *previous* title land last and paint another game's box art with no
//       error and nothing to retry.
//
// Mocked at the `@/lib/*` boundary, the house pattern (see
// `OsInstall.test.tsx`'s own note for why that layer and not
// `@tauri-apps/api` itself). `@/lib/launch` is mocked because the panel plans
// a launch on mount when asked to; nothing here asks, but its module-level
// imports still have to resolve without a Tauri bridge.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";

import { useSettingsStore } from "@/stores/settingsStore";
import { DEFAULT_SETTINGS } from "@/lib/settings";
import type { ArtRef } from "@/lib/artwork";
import type { CatalogueEntry } from "@/lib/gameindex";

const attachMock = vi.hoisted(() => vi.fn());
const detachMock = vi.hoisted(() => vi.fn());
const artworkDirMock = vi.hoisted(() => vi.fn());
const artworkForTitleMock = vi.hoisted(() => vi.fn());
const dialogOpenMock = vi.hoisted(() => vi.fn());
const offersMock = vi.hoisted(() => vi.fn());
const placeMock = vi.hoisted(() => vi.fn());
const igamewritePlanMock = vi.hoisted(() => vi.fn());
const igamewriteApplyMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/artwork", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/artwork")>()),
  artworkAttach: attachMock,
  artworkDetach: detachMock,
  artworkDir: artworkDirMock,
  artworkForTitle: artworkForTitleMock,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: dialogOpenMock }));

// `convertFileSrc` is reached through a dynamic `import()` inside
// `loadPictures`, so it has to exist even though nothing here reads a picture.
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `art://${path}`,
  invoke: vi.fn().mockResolvedValue(null),
}));

// ART-130: the offers section asks on mount whenever the title declares a
// Kickstart and a ROM folder is remembered.
vi.mock("@/lib/gameindex", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/gameindex")>()),
  kickstartOffersFor: offersMock,
  placeKickstart: placeMock,
  igamewritePlan: igamewritePlanMock,
  igamewriteApply: igamewriteApplyMock,
}));

vi.mock("@/lib/launch", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/launch")>()),
  launchPlan: vi.fn().mockResolvedValue(null),
  launchTitle: vi.fn().mockResolvedValue(0),
}));

// Imported after the mocks so the component picks them up.
const { TitleDetail } = await import("@/components/collection/TitleDetail");

function entryFor(id: string, title: string): CatalogueEntry {
  return {
    path: `E:\\games\\${title}.rp9`,
    record: {
      schema: 1,
      id,
      title: { value: title, from: "rp9-manifest" },
      kind: null,
      year: null,
      publisher: null,
      genre: null,
      rating: null,
      chipset: null,
      kickstart: null,
      media: { kind: "floppies", ordered: [`${title}.adf`] },
      preview: null,
      source: { name: `${title}.rp9`, sha256: "a".repeat(64), bytes: 1024 },
    },
  };
}

const BOXART: ArtRef = { kind: "boxart", file: "boxart/turrican.png", source: "manual" };

function renderPanel(entry: CatalogueEntry) {
  return render(
    <TitleDetail
      entry={entry}
      art={undefined}
      hasManualArt={true}
      onArtChanged={() => {}}
      onClose={() => {}}
      playRequest={0}
    />
  );
}

beforeEach(() => {
  useSettingsStore.setState({ loaded: true, settings: { ...DEFAULT_SETTINGS, remembered: {} } });
  artworkDirMock.mockReset().mockResolvedValue("E:\\art");
  artworkForTitleMock.mockReset().mockResolvedValue([]);
  attachMock.mockReset();
  detachMock.mockReset();
  dialogOpenMock.mockReset();
  offersMock.mockReset().mockResolvedValue([]);
  placeMock.mockReset();
  igamewritePlanMock.mockReset().mockResolvedValue({ items: [], refusals: [] });
  igamewriteApplyMock.mockReset();
});

afterEach(() => cleanup());

describe("a write that takes a backup says where it put it", () => {
  it("shows the backup path after attaching a picture", async () => {
    dialogOpenMock.mockResolvedValue("E:\\pics\\turrican.png");
    attachMock.mockResolvedValue({
      art: BOXART,
      backup: "E:\\catalogue\\overrides.json.20260820-1400.bak",
    });

    renderPanel(entryFor("t1", "Turrican"));
    await userEvent.click(screen.getByRole("button", { name: i18n.t("collection.detail.art.attach") }));

    await screen.findByText(
      i18n.t("collection.detail.art.backedUp", {
        path: "E:\\catalogue\\overrides.json.20260820-1400.bak",
      })
    );
  });

  it("shows it after detaching too", async () => {
    detachMock.mockResolvedValue({ backup: "E:\\catalogue\\overrides.json.20260820-1401.bak" });

    renderPanel(entryFor("t1", "Turrican"));
    await userEvent.click(screen.getByRole("button", { name: i18n.t("collection.detail.art.remove") }));

    await screen.findByText(
      i18n.t("collection.detail.art.backedUp", {
        path: "E:\\catalogue\\overrides.json.20260820-1401.bak",
      })
    );
  });

  it("says nothing when there was nothing to back up", async () => {
    // The first correction ever made to a title writes a file that did not
    // exist, so `guarded_write` preserves nothing and returns `null`. A note
    // pointing at a backup that was never taken would be worse than silence.
    dialogOpenMock.mockResolvedValue("E:\\pics\\turrican.png");
    attachMock.mockResolvedValue({ art: BOXART, backup: null });

    const { container } = renderPanel(entryFor("t1", "Turrican"));
    await userEvent.click(screen.getByRole("button", { name: i18n.t("collection.detail.art.attach") }));

    await waitFor(() => expect(attachMock).toHaveBeenCalled());
    expect(container.textContent).not.toContain("saved to");
  });
});

describe("a slow answer for the previous title cannot paint over this one", () => {
  it("keeps the pictures of the title on screen, not of the one before it", async () => {
    // Two titles, and the *first* one's query resolves last — the ordering a
    // user produces by clicking down a list faster than the artwork cache
    // answers. Without a request ticket the late answer wins and the panel
    // shows Turrican's picture while saying "Lotus".
    let releaseFirst: (() => void) | null = null;
    artworkForTitleMock.mockImplementation((title: string) => {
      if (title === "Turrican") {
        return new Promise((resolve) => {
          releaseFirst = () =>
            resolve([{ kind: "boxart", file: "turrican.png", source: "manual" }]);
        });
      }
      return Promise.resolve([{ kind: "screenshot", file: "lotus.png", source: "manual" }]);
    });

    const { container, rerender } = renderPanel(entryFor("t1", "Turrican"));
    await waitFor(() => expect(artworkForTitleMock).toHaveBeenCalledWith("Turrican"));

    // Switch to the second title while the first is still in flight.
    rerender(
      <TitleDetail
        entry={entryFor("t2", "Lotus")}
        art={undefined}
        hasManualArt={true}
        onArtChanged={() => {}}
        onClose={() => {}}
        playRequest={0}
      />
    );
    // `alt=""` makes the picture presentational, so it has no `img` role to
    // query by — the element itself is what this is about anyway.
    const picture = () => container.querySelector("img") as HTMLImageElement | null;
    await waitFor(() => expect(picture()?.src).toContain("lotus.png"));

    // Now let the *first* title's query finish, last.
    expect(releaseFirst).not.toBeNull();
    releaseFirst!();
    await new Promise((resolve) => setTimeout(resolve, 0));

    // Still Lotus. Removing the ticket check in `loadPictures` makes this
    // line read `turrican.png`.
    expect(picture()?.src).toContain("lotus.png");
    expect(picture()?.src).not.toContain("turrican.png");
  });

  it("a stale failure cannot blank the current title either", async () => {
    // The other face of the same bug: the previous title's query *rejecting*
    // last would run the catch arm and clear the pictures that belong to the
    // title now on screen.
    let failFirst: (() => void) | null = null;
    artworkForTitleMock.mockImplementation((title: string) => {
      if (title === "Turrican") {
        return new Promise((_resolve, reject) => {
          failFirst = () => reject(new Error("cache unreadable"));
        });
      }
      return Promise.resolve([{ kind: "screenshot", file: "lotus.png", source: "manual" }]);
    });

    const { container, rerender } = renderPanel(entryFor("t1", "Turrican"));
    await waitFor(() => expect(artworkForTitleMock).toHaveBeenCalledWith("Turrican"));

    rerender(
      <TitleDetail
        entry={entryFor("t2", "Lotus")}
        art={undefined}
        hasManualArt={true}
        onArtChanged={() => {}}
        onClose={() => {}}
        playRequest={0}
      />
    );
    const picture = () => container.querySelector("img") as HTMLImageElement | null;
    await waitFor(() => expect(picture()?.src).toContain("lotus.png"));

    expect(failFirst).not.toBeNull();
    failFirst!();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(picture()?.src).toContain("lotus.png");
  });
});

// ---------------------------------------------------------------------------
// Task 6 review fix (I4): the igame.data verdict section had no component
// test at all, though this file already existed. The four verdict sentences
// and the cancelled ending are the exact risk surface CLAUDE.md's "the
// failure that does not crash" names — a wrong sentence here would pass
// every Rust test and every `phrase-keys` check, which only prove the four
// keys *resolve*, not that the right one is *rendered* for the right state.
// ---------------------------------------------------------------------------

function drawerEntry(id: string, title: string): CatalogueEntry {
  const entry = entryFor(id, title);
  return {
    ...entry,
    record: {
      ...entry.record,
      media: { kind: "whdload-drawer", dir: `E:\\games\\${title}`, slave: `${title}.slave` },
    },
  };
}

function planWithOneItem(dir: string, title: string) {
  return { items: [{ dir, title, data: { title, chipset: null, genre: null, year: null, players: null, exe: null } }], refusals: [] };
}

describe("the igame.data verdict section (I4)", () => {
  it("says a fresh file was written", async () => {
    igamewritePlanMock.mockResolvedValue(planWithOneItem("E:\\games\\Turrican", "Turrican"));
    igamewriteApplyMock.mockResolvedValue({
      verdicts: [{ dir: "E:\\games\\Turrican", state: { state: "written" }, backup: null, omitted: [] }],
      cancelled: false,
    });
    renderPanel(drawerEntry("t1", "Turrican"));

    await userEvent.click(
      await screen.findByRole("button", { name: i18n.t("collection.detail.igamewrite.action") })
    );
    const said = await screen.findByTestId("igamewrite-result");
    expect(said.textContent).toBe(i18n.t("collection.detail.igamewrite.result.written"));
    expect(said.className).toContain("badge-ok");
  });

  it("says the existing file was merged", async () => {
    igamewritePlanMock.mockResolvedValue(planWithOneItem("E:\\games\\Lotus", "Lotus"));
    igamewriteApplyMock.mockResolvedValue({
      verdicts: [{ dir: "E:\\games\\Lotus", state: { state: "merged" }, backup: "E:\\games\\Lotus\\.art-backup\\igame.data.1.bak", omitted: [] }],
      cancelled: false,
    });
    renderPanel(drawerEntry("t2", "Lotus"));

    await userEvent.click(
      await screen.findByRole("button", { name: i18n.t("collection.detail.igamewrite.action") })
    );
    const said = await screen.findByTestId("igamewrite-result");
    expect(said.textContent).toContain(i18n.t("collection.detail.igamewrite.result.merged"));
    expect(said.textContent).toContain("E:\\games\\Lotus\\.art-backup\\igame.data.1.bak");
    expect(said.className).toContain("badge-ok");
  });

  it("says nothing changed, and it is not styled as an error", async () => {
    igamewritePlanMock.mockResolvedValue(planWithOneItem("E:\\games\\Twice", "Twice"));
    igamewriteApplyMock.mockResolvedValue({
      verdicts: [
        {
          dir: "E:\\games\\Twice",
          state: { state: "skipped", detail: "igame.data already says this; nothing was changed" },
          backup: null,
          omitted: [],
        },
      ],
      cancelled: false,
    });
    renderPanel(drawerEntry("t3", "Twice"));

    await userEvent.click(
      await screen.findByRole("button", { name: i18n.t("collection.detail.igamewrite.action") })
    );
    const said = await screen.findByTestId("igamewrite-result");
    expect(said.textContent).toBe(
      `${i18n.t("collection.detail.igamewrite.result.skipped")} igame.data already says this; nothing was changed`
    );
    expect(said.className).not.toContain("badge-err");
  });

  it("says the write failed, and only this ending is styled as an error", async () => {
    igamewritePlanMock.mockResolvedValue(planWithOneItem("E:\\games\\Locked", "Locked"));
    igamewriteApplyMock.mockResolvedValue({
      verdicts: [
        { dir: "E:\\games\\Locked", state: { state: "failed", detail: "permission denied" }, backup: null, omitted: [] },
      ],
      cancelled: false,
    });
    renderPanel(drawerEntry("t4", "Locked"));

    await userEvent.click(
      await screen.findByRole("button", { name: i18n.t("collection.detail.igamewrite.action") })
    );
    const said = await screen.findByTestId("igamewrite-result");
    expect(said.textContent).toBe(`${i18n.t("collection.detail.igamewrite.result.failed")} permission denied`);
    expect(said.className).toContain("badge-err");
  });

  /// **I2's own UI half.** A title too long for iGame's line is named, not
  /// silently dropped, alongside whichever ending the write actually had.
  it("names what did not fit, beside the ending", async () => {
    igamewritePlanMock.mockResolvedValue(planWithOneItem("E:\\games\\Long", "Long"));
    igamewriteApplyMock.mockResolvedValue({
      verdicts: [
        {
          dir: "E:\\games\\Long",
          state: { state: "written" },
          backup: null,
          omitted: ["title is 203 bytes — too long for iGame's 64-byte line"],
        },
      ],
      cancelled: false,
    });
    renderPanel(drawerEntry("t5", "Long"));

    await userEvent.click(
      await screen.findByRole("button", { name: i18n.t("collection.detail.igamewrite.action") })
    );
    const said = await screen.findByTestId("igamewrite-result");
    expect(said.textContent).toContain(
      i18n.t("collection.detail.igamewrite.omitted", {
        fields: "title is 203 bytes — too long for iGame's 64-byte line",
      })
    );
  });

  /// **M5.** A run stopped before this title was reached must not look like
  /// nothing happened at all — the sentence has to say the run was stopped,
  /// not just fall through to no badge rendering anything.
  it("says the run was stopped, not nothing at all", async () => {
    igamewritePlanMock.mockResolvedValue(planWithOneItem("E:\\games\\Stopped", "Stopped"));
    igamewriteApplyMock.mockResolvedValue({ verdicts: [], cancelled: true });
    renderPanel(drawerEntry("t6", "Stopped"));

    await userEvent.click(
      await screen.findByRole("button", { name: i18n.t("collection.detail.igamewrite.action") })
    );
    const said = await screen.findByTestId("igamewrite-result");
    expect(said.textContent).toBe(i18n.t("collection.detail.igamewrite.result.cancelled"));
  });
});

// ---------------------------------------------------------------------------
// ART-130: the Kickstart a title asks for
// ---------------------------------------------------------------------------
//
// The loop's other end. G10 read what a slave declares; this is the screen
// that says whether the user has it and offers to put it where WHDLoad looks.
// **A proposal and never a copy** - the owner's decision, 2026-08-21 - so
// every placement is its own button press.

function withKickstart(entry: CatalogueEntry): CatalogueEntry {
  return {
    ...entry,
    record: {
      ...entry.record,
      kickstart: {
        value: {
          image: "kick34005.A500",
          size: 262144,
          crc16: 0xabcd,
          rom_version: null,
          alternatives: [],
        },
        from: "whdload-slave",
      },
    },
  } as CatalogueEntry;
}

function seedRomDirAndTree() {
  useSettingsStore.setState({
    loaded: true,
    settings: {
      ...DEFAULT_SETTINGS,
      remembered: {
        "launch.romDir": "E:\\roms",
        "buildSession.tree": { root: "E:\\amiga\\dist-3.2", builtHere: true },
      },
    },
  });
}

const SUPPLIED = {
  outcome: "supplied" as const,
  wanted: { name: "kick34005.A500", crc16: 0xabcd, size: 262144 },
  by: { path: "E:\\roms\\kick13.rom", name: "Kickstart 1.3 (34.005)", sizeDisagrees: null },
};

describe("the Kickstart a title asks for (ART-130)", () => {
  it("says nothing at all when the title declares no Kickstart", async () => {
    seedRomDirAndTree();
    renderPanel(entryFor("t1", "Turrican"));
    await waitFor(() => expect(artworkForTitleMock).toHaveBeenCalled());
    expect(offersMock).not.toHaveBeenCalled();
    expect(screen.queryByTestId("kickstart-offers")).toBeNull();
  });

  it("says nothing when no ROM folder has been chosen", async () => {
    // Not "you do not have it" - ART has not looked, and those are different
    // sentences.
    useSettingsStore.setState({
      loaded: true,
      settings: { ...DEFAULT_SETTINGS, remembered: {} },
    });
    renderPanel(withKickstart(entryFor("t2", "Lotus")));
    await waitFor(() => expect(artworkForTitleMock).toHaveBeenCalled());
    expect(offersMock).not.toHaveBeenCalled();
    expect(screen.queryByTestId("kickstart-offers")).toBeNull();
  });

  it("names the ROM when the user already has it, and offers to place it", async () => {
    seedRomDirAndTree();
    offersMock.mockResolvedValue([SUPPLIED]);
    renderPanel(withKickstart(entryFor("t3", "Lotus")));

    const row = await screen.findByTestId("kickstart-offer");
    expect(row.textContent).toContain("kick34005.A500");
    expect(row.textContent).toContain("Kickstart 1.3 (34.005)");
    expect(screen.getByRole("button", { name: /place it/i })).toBeTruthy();
  });

  it("hands the core the ROM path, the name the title asks for and the tree", async () => {
    seedRomDirAndTree();
    offersMock.mockResolvedValue([SUPPLIED]);
    placeMock.mockResolvedValue({ outcome: "placed", to: "E:\\amiga\\dist-3.2\\Devs\\Kickstarts\\kick34005.A500", bytes: 262144 });
    renderPanel(withKickstart(entryFor("t4", "Lotus")));

    await screen.findByTestId("kickstart-offer");
    await userEvent.click(screen.getByRole("button", { name: /place it/i }));

    await waitFor(() => expect(placeMock).toHaveBeenCalled());
    expect(placeMock).toHaveBeenCalledWith(
      "E:\\roms\\kick13.rom",
      "kick34005.A500",
      "E:\\amiga\\dist-3.2"
    );
  });

  /// **Three endings, three sentences.** "Refused" is not "failed" and
  /// "already there" is neither - collapsing them is this project's own named
  /// defect class.
  it("says which of the three things happened", async () => {
    seedRomDirAndTree();
    offersMock.mockResolvedValue([SUPPLIED]);

    for (const [outcome, expected] of [
      ["placed", "Placed at"],
      ["already-there", "Already there, unchanged"],
      ["occupied", "Refused"],
    ] as const) {
      placeMock.mockResolvedValue({ outcome, to: "E:\\dist\\Devs\\Kickstarts\\k", bytes: 1 });
      const { unmount } = renderPanel(withKickstart(entryFor("t5", "Lotus")));
      await screen.findByTestId("kickstart-offer");
      await userEvent.click(screen.getByRole("button", { name: /place it/i }));

      const said = await screen.findByTestId("kickstart-placed");
      expect(said.textContent).toContain(expected);
      // And they are not each other.
      if (outcome !== "occupied") expect(said.textContent).not.toContain("Refused");
      unmount();
    }
  });

  /// A file the user **has**. Saying "missing" would send them looking for
  /// something already on their disk.
  it("distinguishes a ROM ART cannot read from one that is not there", async () => {
    seedRomDirAndTree();
    offersMock.mockResolvedValue([
      {
        outcome: "encrypted",
        wanted: { name: "kick34005.A500", crc16: 0xabcd, size: null },
        candidates: ["E:\\Amiga Forever\\a500.rom"],
      },
    ]);
    renderPanel(withKickstart(entryFor("t6", "Lotus")));

    const row = await screen.findByTestId("kickstart-offer");
    expect(row.textContent).toContain("rom.key");
    expect(row.textContent).not.toContain("not in your ROM folder");
    // Nothing to place: ART cannot read it.
    expect(screen.queryByRole("button", { name: /place it/i })).toBeNull();
  });

  /// Without a system volume there is nowhere to put it, and a button that
  /// exists to fail is what section 46 and section 89 both forbid.
  it("asks for a system volume instead of offering a button that cannot work", async () => {
    useSettingsStore.setState({
      loaded: true,
      settings: { ...DEFAULT_SETTINGS, remembered: { "launch.romDir": "E:\\roms" } },
    });
    offersMock.mockResolvedValue([SUPPLIED]);
    renderPanel(withKickstart(entryFor("t7", "Lotus")));

    const row = await screen.findByTestId("kickstart-offer");
    expect(screen.queryByRole("button", { name: /place it/i })).toBeNull();
    expect(row.textContent).toContain("system volume");
  });

  it("renders no raw key and no unrendered interpolation", async () => {
    seedRomDirAndTree();
    offersMock.mockResolvedValue([
      SUPPLIED,
      { outcome: "not-here", wanted: { name: "kick40068.A1200", crc16: 1, size: null } },
      { outcome: "unmatchable", wanted: { name: "a list", crc16: null, size: null } },
    ]);
    renderPanel(withKickstart(entryFor("t8", "Lotus")));

    const section = await screen.findByTestId("kickstart-offers");
    expect(section.textContent).not.toMatch(/collection\.detail\.kickstart/);
    expect(section.textContent).not.toMatch(/\{\{[^}]+\}\}/);
  });
});
