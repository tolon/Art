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
