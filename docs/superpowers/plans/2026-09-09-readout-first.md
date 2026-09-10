# The readout first (simplification round A) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The OS Builder's `kaynak` step opens on the answer — *what ART found* — with the folder list beside it, and an empty list asks for a folder instead of reporting on none.

**Architecture:** Front-end only. The folder list, its layer tags, Add/Remove, the *unused for this plan* line, the Amiga Forever offer and the guide buttons are cut out of `OsInstall.tsx` into one presentational component, `MaterialFolders.tsx`, that owns no session state (the step keeps every setter and passes callbacks). `MaterialReadout.tsx` loses the guide block and nothing else. `OsInstall.tsx` lays the two out as a wrapping flex row with the readout **first in the DOM**, and renders one ask sentence in the readout's place while the list is empty. No stylesheet, no Rust, no new command, no new remembered key.

**Tech Stack:** React 18 + react-i18next, Vitest + jsdom + Testing Library. Inline styles, as the rest of the builder.

**Spec:** `docs/superpowers/specs/2026-09-08-os-builder-simplification-design.md` § 4 (and § 6 row A, § 8). Read § 4 whole before Task 2.

**What the spec got wrong about the tree on 2026-09-09** (checked before this plan was written — CLAUDE.md, *a work list decays*):

- § 4.2 names `SourceStep.tsx` as the file that lays the columns out. `SourceStep` is round C's; today the step is `OsInstall.tsx` and stays so. The layout lands there, and the tests § 4.3 puts in `SourceStep.test.tsx` land in `OsInstall.test.tsx`.
- § 4.1 does not say where the two ART-256 lines (`osinstall-media-found` / `osinstall-media-empty`, *"N install disks found: …"*) go. They are claims about the **folders'** scans, not about slots, so they go in the folder column under the list. ART-285 (that sentence counts game discs as install disks) is **not** touched here — the sentence moves, its words do not.
- The `media-identity` block (*what the same files are by content*) is below the readout today and stays below the two columns, full width. It is neither the readout nor the list.
- The list's own *"No folder chosen."* line (`osinstall.media.none`) stays inside the list. The new ask sentence is the **readout column's**; the two do not share a sentence and do not share a place.
- § 4.1's third paragraph puts the Amiga Forever offer "beside" the ask in the primary column, while its second paragraph and § 4.2 put it in the secondary column with the folders. It is in the secondary column: the offer is an action on the list, and the ask is the readout's one sentence.

## Global Constraints

- Every new i18n key lands in `src/i18n/en.json` **and** `src/i18n/tr.json` in the same commit; `pnpm vitest run -t "have identical key sets"` is the parity check.
- `src/lib` never renders a string; components call `t()`. Nothing in this round touches `src/lib`.
- **Nothing changes unless the user changes it.** No new remembered key; the guide is written only from its button; the Amiga Forever offer adds a folder only from its button.
- **Endings stay distinct.** The guide's three (`written` / `alreadyThere` / `failed`) and the readout's eleven are moved or untouched, never merged.
- `data-testid`s that exist today keep their names: `material-folders`, `material-folder`, `material-add-folder`, `material-unused`, `amiga-forever-offer`, `material-guide`, `material-guide-write`, `material-guide-{written,alreadyThere,failed}`, `material-readout`, `material-set-line`, `layer-wrong-hint-N`, `material-folder-unreadable-N`. ART-241's `aria-describedby` wiring on each folder row moves with the row, byte for byte.
- A component test is `*.test.tsx` (jsdom applies only there).
- Mutate a file by absolute path with `shutil.copyfile` to the scratchpad and back — never `git checkout --`, never `shutil.move`.
- Commit messages go through a file: `git commit -F <scratchpad>\msg.txt`. `git branch --show-current` before every commit.
- Branch: `art-simplify-a-readout-first`, from `main` at `ad61a7a` or later. Merged `--no-ff` only after Task 6's hand-off has been driven by the owner; **never pushed by this plan**.
- No Rust changes: at the end, `git diff --stat main -- src-tauri` prints nothing.
- Scratchpad for commit messages and mutation copies: `C:\Users\ismoz\AppData\Local\Temp\claude\D--Projeler-Amiga\06212805-e1dd-4b6f-b2ae-dd6f17219be6\scratchpad`.

---

## File structure

**Create**
- `src/components/osbuilder/MaterialFolders.tsx` — the folder column: label, one row per folder (path · layer select · Remove · unreadable line · wrong-layer hint), Add, the *unused for this plan* line, the Amiga Forever offer, the guide block (moved from `MaterialReadout`). Presentational: props in, callbacks out; the only state it owns is the guide's per-folder outcome.
- `src/components/osbuilder/MaterialFolders.test.tsx` — its tests, including the five guide tests moved from `MaterialReadout.test.tsx` by name.

**Modify**
- `src/components/osbuilder/MaterialReadout.tsx` — remove the guide block, its state, `writeGuide`, and the `osinstallWriteMaterialGuide` / `GuideOutcome` imports. Nothing else.
- `src/components/osbuilder/MaterialReadout.test.tsx` — remove the moved describe; add one assertion that no guide block renders inside the readout.
- `src/components/osbuilder/OsInstall.tsx:1750-1925` — the inline list block becomes `<MaterialFolders …/>`; lines 1750-1960 become the two-column row with the ask sentence.
- `src/components/osbuilder/OsInstall.test.tsx` — one new `describe` (order, the ask, the ask going away).
- `src/i18n/en.json`, `src/i18n/tr.json` — `osinstall.material.askFolders`.
- `docs/FEATURES.md` rows for *one material folder list*, *offer Amiga Forever's own folders*, *write "what goes in this folder"* — the file names; `docs/session-log.md`; `docs/STATUS.md` (the "Start here" block, in place); `CHANGELOG.md` `[Unreleased]`.

---

### Task 1: Branch, and the plan on it

**Files:**
- Create: nothing in `src/`
- Commit: `docs/superpowers/plans/2026-09-09-readout-first.md`

- [ ] **Step 1: Branch from `main`**

```powershell
cd D:\Projeler\Amiga\amiga-retro-toolkit
git branch --show-current          # must print: main
git status --short                 # only "M src-tauri/Cargo.toml" (line endings; git diff is empty) — leave it
git checkout -b art-simplify-a-readout-first
git branch --show-current          # must print: art-simplify-a-readout-first
```

- [ ] **Step 2: Commit the plan**

Write `<scratchpad>\msg1.txt` with the Write tool:

```
Plan: simplification round A — the readout first

docs/superpowers/plans/2026-09-09-readout-first.md, from spec
docs/superpowers/specs/2026-09-08-os-builder-simplification-design.md § 4,
checked against the tree on 2026-09-09 (SourceStep does not exist yet; the
layout lands in OsInstall.tsx).
```

```powershell
git add docs/superpowers/plans/2026-09-09-readout-first.md
git commit -F "<scratchpad>\msg1.txt"
```

---

### Task 2: `MaterialFolders.tsx` — the folder column as one component

**Files:**
- Create: `src/components/osbuilder/MaterialFolders.tsx`
- Create: `src/components/osbuilder/MaterialFolders.test.tsx`
- Read first: `src/components/osbuilder/OsInstall.tsx:1750-1925` (the JSX being moved), `src/components/osbuilder/MaterialReadout.tsx:150-190` and `:376-431` (the guide state and block being moved), `src/components/osbuilder/MaterialReadout.test.tsx:1-142` (the mock and render pattern) and `:653-754` (the five guide tests being moved).

**Interfaces:**
- Consumes: `MaterialFolder` from `@/lib/buildSession` (`{ path: string; layer: string | null }`); `InstallLayer` (`{ id: string; labelKey: string | null }`), `InstallRelease`, `MediaScanResult`, `GuideOutcome`, `osinstallWriteMaterialGuide(folder, release, lang)` from `@/lib/osinstall`; `errorText` from `@/lib/errorText`.
- Produces, for Task 3:

```ts
export interface MaterialFoldersProps {
  release: InstallRelease;
  /** The build's material list, in list order. */
  folders: MaterialFolder[];
  /** The layers this release's recipe declares; `[]` means unlayered and no select is drawn. */
  layers: InstallLayer[];
  layerLabel: (layer: InstallLayer) => string;
  /** One scan per folder path; `null` or absent = not scanned / could not be read (the row says which from `outcome`). */
  folderScans: Record<string, MediaScanResult | null>;
  /** The wrong-layer sentence for a row, or `null` (the step computes it — it needs the identification pass). */
  wrongLayerHint: (entry: MaterialFolder) => string | null;
  /** Folders a layered plan will not read, named rather than dropped. */
  unusedForPlan: string[];
  /** Amiga Forever's disks folder while the offer stands, else `null`. */
  amigaForeverOffer: string | null;
  onAdd: () => void;
  onRemove: (path: string) => void;
  onTag: (path: string, layer: string) => void;
  onAmigaForeverAdd: () => void;
  onAmigaForeverDismiss: () => void;
}
export function MaterialFolders(props: MaterialFoldersProps): JSX.Element;
```

- [ ] **Step 1: Write the failing tests**

Create `src/components/osbuilder/MaterialFolders.test.tsx`:

```tsx
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
```

- [ ] **Step 2: Run it and see it fail on the missing module**

```powershell
pnpm vitest run src/components/osbuilder/MaterialFolders.test.tsx
```
Expected: FAIL — `Failed to resolve import "@/components/osbuilder/MaterialFolders"` (or equivalent). Nothing else.

- [ ] **Step 3: Write the component**

Create `src/components/osbuilder/MaterialFolders.tsx`. The list JSX is **moved verbatim** from `OsInstall.tsx:1763-1925` (the `material-folders` div through the Amiga Forever offer) with these substitutions only: `materialFolders` → `folders`; `folderScans[entry.path]` unchanged; `wrongLayerHintFor(entry)` → `wrongLayerHint(entry)`; `tagFolder` → `onTag`; `removeFolder` → `onRemove`; `addFolder()` → `onAdd()`; `plannedFolders.unusedForPlan` → `unusedForPlan`; the offer's two `onClick`s → `onAmigaForeverAdd` / `onAmigaForeverDismiss`. Every comment on the moved block moves with it. The guide block and its state are moved verbatim from `MaterialReadout.tsx` and appended after the offer.

```tsx
// The folder column of the source step (simplification design § 4).
//
// **What this is.** The one material folder list (intake design § 3.1) — a
// row per folder with its layer tag and Remove, Add, the folders a layered
// plan will not read, the Amiga Forever offer, and the guide buttons — as one
// component, so the step can lay it out *beside* the readout with the
// readout first. It was inline in `OsInstall.tsx` until 2026-09-09, two
// hundred lines above the answer it is the question for.
//
// **What this owns.** Nothing about the build. Every value is the step's
// (the session's list, the scans, the layers) and every change goes back
// through a callback; the only state here is what the guide button last
// answered, per folder, which is a fact about this screen and not about the
// build. That is what keeps `remembered.ts`'s rule true from inside: this
// component cannot write a setting because it holds none.
//
// **The guide block moved here from `MaterialReadout.tsx`** because it is an
// action on a folder, and this is where the folders are. Its three endings —
// written, already there, could not — are the same three, kept apart.

import { useState } from "react";
import { useTranslation } from "react-i18next";

import type { MaterialFolder } from "@/lib/buildSession";
import { errorText } from "@/lib/errorText";
import {
  osinstallWriteMaterialGuide,
  type GuideOutcome,
  type InstallLayer,
  type InstallRelease,
  type MediaScanResult,
} from "@/lib/osinstall";

export interface MaterialFoldersProps {
  release: InstallRelease;
  /** The build's material list, in list order. */
  folders: MaterialFolder[];
  /** The layers this release's recipe declares; `[]` means unlayered and no
   *  select is drawn. */
  layers: InstallLayer[];
  layerLabel: (layer: InstallLayer) => string;
  /** One scan per folder path. `null` or absent is "not scanned" or "could
   *  not be read"; the row says which from the scan's own `outcome`. */
  folderScans: Record<string, MediaScanResult | null>;
  /** The wrong-layer sentence for a row, or `null`. Computed by the step,
   *  which is the one that has the identification pass. */
  wrongLayerHint: (entry: MaterialFolder) => string | null;
  /** Folders a layered plan will not read, named rather than dropped. */
  unusedForPlan: string[];
  /** Amiga Forever's disks folder while the offer stands, else `null`. */
  amigaForeverOffer: string | null;
  onAdd: () => void;
  onRemove: (path: string) => void;
  onTag: (path: string, layer: string) => void;
  onAmigaForeverAdd: () => void;
  onAmigaForeverDismiss: () => void;
}

export function MaterialFolders({
  release,
  folders,
  layers,
  layerLabel,
  folderScans,
  wrongLayerHint,
  unusedForPlan,
  amigaForeverOffer,
  onAdd,
  onRemove,
  onTag,
  onAmigaForeverAdd,
  onAmigaForeverDismiss,
}: MaterialFoldersProps) {
  const { t, i18n } = useTranslation();

  /**
   * What the guide button last answered, per folder.
   *
   * Per folder rather than one value for the column: two folders are two
   * files, and a *written* line under the folder that is still empty would
   * be the screen claiming something about a folder ART did not touch. A
   * missing entry is the ordinary state and renders nothing — never "not
   * written yet", which is a sentence about a thing nobody asked for.
   */
  const [guides, setGuides] = useState<
    Record<string, GuideOutcome | { state: "failed"; detail: string }>
  >({});

  /**
   * Write the guide into one folder. **Only from this click** (intake design
   * § 3.7): ART does not write into somebody's folder because they pointed at
   * it.
   *
   * Nothing is overwritten — the Rust side opens the file with `create_new`,
   * so a guide already there keeps every byte — and *there already* is its
   * own answer rather than an error, because "delete it and ask again" is a
   * different next step from "ART could not write it".
   */
  async function writeGuide(folder: string) {
    try {
      const outcome = await osinstallWriteMaterialGuide(folder, release, i18n.language.split("-")[0]);
      setGuides((held) => ({ ...held, [folder]: outcome }));
    } catch (e) {
      setGuides((held) => ({ ...held, [folder]: { state: "failed", detail: errorText(t, e) } }));
    }
  }

  return (
    <>
      {/* ← the `material-folders` div from OsInstall.tsx:1763-1900, verbatim
          with the substitutions listed in the plan, every comment kept → */}
      {/* ← the Amiga Forever offer from OsInstall.tsx:1901-1925, verbatim → */}
      {/* ← the guide block from MaterialReadout.tsx:376-431, verbatim, still
          guarded by `folders.length > 0` → */}
    </>
  );
}
```

The three arrows are where the moved JSX goes; the executor pastes the real blocks (the plan does not reprint 170 lines that already exist in the tree). After the paste there must be **no** reference to `materialFolders`, `plannedFolders`, `tagFolder`, `removeFolder`, `addFolder`, `wrongLayerHintFor`, `addMaterialFolder` or `setAmigaForeverDismissed` in the new file — `grep -nE "materialFolders|plannedFolders|tagFolder|removeFolder|addFolder\(|wrongLayerHintFor|addMaterialFolder|setAmigaForeverDismissed" src/components/osbuilder/MaterialFolders.tsx` prints nothing.

- [ ] **Step 4: Run the new file's tests, then the readout's (still unchanged, still green)**

```powershell
pnpm vitest run src/components/osbuilder/MaterialFolders.test.tsx src/components/osbuilder/MaterialReadout.test.tsx
```
Expected: both files PASS. Quote the `Tests  N passed` line. (The guide block exists in two components for the length of this one commit; nothing mounts both.)

- [ ] **Step 5: Type-check both passes**

```powershell
pnpm lint
```
Expected: clean twice (the second pass is `tsconfig.test.json`).

- [ ] **Step 6: Commit**

`<scratchpad>\msg2.txt`:

```
MaterialFolders: the folder column as one component

Cut out of OsInstall.tsx's kaynak step so the readout can come first
(simplification design § 4.2). Presentational — the step keeps every setter
— and it takes the guide block from MaterialReadout with it, because the
guide is an action on a folder. Its five tests move by name.

Nothing mounts it yet; the next commit wires it in and removes the
duplicate.
```

```powershell
git branch --show-current
git add src/components/osbuilder/MaterialFolders.tsx src/components/osbuilder/MaterialFolders.test.tsx
git commit -F "<scratchpad>\msg2.txt"
```

---

### Task 3: The readout loses the guide; the step mounts `MaterialFolders`

**Files:**
- Modify: `src/components/osbuilder/MaterialReadout.tsx` (lines 41-49 imports, 150-190 state + `writeGuide`, 376-431 the block)
- Modify: `src/components/osbuilder/MaterialReadout.test.tsx` (lines 35, 40, 133, 653-754)
- Modify: `src/components/osbuilder/OsInstall.tsx:1750-1925`
- Test: `src/components/osbuilder/OsInstall.test.tsx` (unchanged in this task — it is the regression net)

**Interfaces:**
- Consumes: `MaterialFolders` + `MaterialFoldersProps` from Task 2.
- Produces: `MaterialReadout` with no guide; `OsInstall` rendering `<MaterialFolders>` where the inline block was. Same DOM order as today (list, then the found line, then the readout) — the reorder is Task 4's one variable.

- [ ] **Step 1: Turn the readout test into the guard first**

In `MaterialReadout.test.tsx`: delete the `describe("the list, written into the folder (design § 3.7)"` block (lines 653-754 — the five cases now live in `MaterialFolders.test.tsx`); delete `guideMock` (line 35), its entry in the `vi.mock` (line 40) and its `mockReset` (line 133). Then add, inside `describe("while the pass is running, and when it fails"`, after the `renders nothing at all before any folder is chosen` case:

```tsx
  /// The guide buttons moved to the folder column on 2026-09-09
  /// (simplification design § 4.2): a guide is an action on a folder, and
  /// the readout is the answer, not the place to act. Rendering them here
  /// again would put the same button on screen twice.
  it("draws no guide button — that is the folder column's", async () => {
    renderReadout();
    await screen.findByTestId("material-set-line");
    expect(screen.queryByTestId("material-guide")).toBeNull();
    expect(screen.queryAllByTestId("material-guide-write")).toHaveLength(0);
  });
```

- [ ] **Step 2: Run it — the new case fails, everything else passes**

```powershell
pnpm vitest run src/components/osbuilder/MaterialReadout.test.tsx
```
Expected: 1 failed (`draws no guide button`), the rest passed.

- [ ] **Step 3: Remove the guide from `MaterialReadout.tsx`**

- Imports (lines 41-49): drop `osinstallWriteMaterialGuide` and `type GuideOutcome`; keep `osinstallSlots`, `InstallRelease`, `SlotOverride`, `SlotReport`. `errorText` stays (the failed line uses it).
- Delete the `guides` state and its comment, and `writeGuide` (lines 150-190).
- Delete the guide block, both comments above it and its `folders.length > 0 &&` guard (lines 376-431), so the component's last rendered child is the `crowded.map(...)` list.
- In the header comment (line 1-33) nothing mentions the guide; leave it.

- [ ] **Step 4: Run the readout tests — green**

```powershell
pnpm vitest run src/components/osbuilder/MaterialReadout.test.tsx
```
Expected: all passed; quote the count.

- [ ] **Step 5: Mount `MaterialFolders` in `OsInstall.tsx`**

Add the import beside `MaterialReadout`'s:

```tsx
import { MaterialFolders } from "@/components/osbuilder/MaterialFolders";
```

Replace `OsInstall.tsx:1750-1925` (from the `{/* **One list, one row per folder** …` comment through the closing `)}` of the Amiga Forever offer) with:

```tsx
        {/*
          **The folder column** — the one material list (intake design
          § 3.1), its tags, Add, the folders a layered plan will not read,
          and the Amiga Forever offer, as one component. Every value is this
          step's and every change comes back through a callback; see
          `MaterialFolders.tsx` for why it holds no setting of its own.
        */}
        <MaterialFolders
          release={release}
          folders={materialFolders}
          layers={layers}
          layerLabel={layerLabel}
          folderScans={folderScans}
          wrongLayerHint={wrongLayerHintFor}
          unusedForPlan={plannedFolders.unusedForPlan}
          amigaForeverOffer={amigaForeverOffer}
          onAdd={() => void addFolder()}
          onRemove={removeFolder}
          onTag={tagFolder}
          onAmigaForeverAdd={() => {
            addMaterialFolder(amigaForeverOffer!);
            setAmigaForeverDismissed(true);
          }}
          onAmigaForeverDismiss={() => setAmigaForeverDismissed(true)}
        />
```

The `!` on `amigaForeverOffer` is safe: the callback is only reachable from a button `MaterialFolders` draws while `amigaForeverOffer` is a string. Write that in a one-line comment above it.

- [ ] **Step 6: Run the step's whole suite — nothing moved on screen yet, so nothing may change**

```powershell
pnpm vitest run src/components/osbuilder/OsInstall.test.tsx
```
Expected: the same count as before the task (measure it on `main` first: `git stash` is **not** allowed here — instead run the file once at Task 2's end and write the number down). Every `material-folder`, `material-add-folder`, `material-unused`, `amiga-forever-offer`, `layer-wrong-hint-*` and `material-folder-unreadable-*` case still passes.

- [ ] **Step 7: Lint, then the whole frontend suite once**

```powershell
pnpm lint
pnpm test
```
Expected: lint clean twice; `pnpm test` all files passed. Quote `Test Files  N passed` and `Tests  N passed`.

- [ ] **Step 8: Commit**

`<scratchpad>\msg3.txt`:

```
kaynak: the step mounts MaterialFolders; the readout draws no guide

OsInstall.tsx renders the folder column through the component instead of
170 inline lines, with the same testids and ART-241's describedby wiring.
MaterialReadout.tsx loses the guide block, its state and its import — the
five cases moved with it in the previous commit, and a new case guards
that the readout draws no guide button. Screen order unchanged in this
commit; the reorder is its own change.
```

```powershell
git branch --show-current
git add src/components/osbuilder/MaterialReadout.tsx src/components/osbuilder/MaterialReadout.test.tsx src/components/osbuilder/OsInstall.tsx
git commit -F "<scratchpad>\msg3.txt"
```

---

### Task 4: Two columns, the readout first, and the ask sentence

**Files:**
- Modify: `src/components/osbuilder/OsInstall.tsx` (the block Task 3 wrote, through the `<MaterialReadout …/>` element ~line 1800 after Task 3)
- Modify: `src/i18n/en.json`, `src/i18n/tr.json` — `osinstall.material.askFolders`
- Test: `src/components/osbuilder/OsInstall.test.tsx`

**Interfaces:**
- Consumes: `MaterialFolders`, `MaterialReadout`.
- Produces: testids `source-columns`, `source-primary`, `source-secondary`, `material-ask`.

- [ ] **Step 1: Write the failing tests**

Append to `OsInstall.test.tsx`, after the `Amiga Forever, offered and never added` describe:

```tsx
describe("the readout first (simplification design § 4)", () => {
  /// **The answer before the question.** The owner's third cause on
  /// 2026-09-08: *which folder holds what* was built and sat two hundred
  /// lines under the folder list. First in the DOM is first for a screen
  /// reader and first at a narrow width, so the DOM order is the guard, not
  /// a pixel position jsdom cannot measure.
  it("puts the readout before the folder list in document order", async () => {
    await renderFull();
    const readout = await screen.findByTestId("material-readout");
    const folders = screen.getByTestId("material-folders");
    expect(readout.compareDocumentPosition(folders) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // Both inside the one row, so a narrow window stacks them rather than
    // scattering them.
    const row = screen.getByTestId("source-columns");
    expect(row.contains(readout)).toBe(true);
    expect(row.contains(folders)).toBe(true);
  });

  /// A question nobody has asked is not an answer of "nothing found". With
  /// no folder the readout renders nothing (its own rule), and what stands
  /// in its place is the next action — never a set line, which would be a
  /// claim about the user's disk.
  it("asks for a folder rather than reporting on none", async () => {
    seedRemembered({});
    render(<OsInstall />);
    const ask = await screen.findByTestId("material-ask");
    expect(ask.textContent).toBe(i18n.t("osinstall.material.askFolders"));
    expect(screen.queryByTestId("material-set-line")).toBeNull();
    expect(screen.queryByTestId("material-readout")).toBeNull();
    // The two sentences do not share a place: the list's own "no folder
    // chosen" line stays in the folder column.
    expect(screen.getByTestId("source-secondary").textContent).toContain(
      i18n.t("osinstall.media.none")
    );
    expect(screen.getByTestId("source-primary").textContent).not.toContain(
      i18n.t("osinstall.media.none")
    );
  });

  it("drops the ask the moment a folder is in the list", async () => {
    await renderFull();
    await screen.findByTestId("material-readout");
    expect(screen.queryByTestId("material-ask")).toBeNull();
  });

  it("renders the ask in Turkish as a sentence, not a key", async () => {
    await changeLanguage("tr");
    seedRemembered({});
    render(<OsInstall />);
    const ask = await screen.findByTestId("material-ask");
    expect(ask.textContent).toBe(i18n.t("osinstall.material.askFolders"));
    expect(ask.textContent).not.toContain("osinstall.");
    expect(ask.textContent).not.toContain("{{");
  });
});
```

If `renderFull()` in this file does not seed a material folder for the release it uses, read `FULL_FIELDS` (`OsInstall.test.tsx:~400-517`) and use whichever helper does; the readout only renders with at least one folder.

- [ ] **Step 2: Run them — four fail**

```powershell
pnpm vitest run src/components/osbuilder/OsInstall.test.tsx -t "the readout first"
```
Expected: 4 failed (`source-columns` / `material-ask` not found), 0 passed.

- [ ] **Step 3: Add the key to both catalogues**

`src/i18n/en.json`, inside `osinstall.material`, after `"readoutHeading"`:

```json
"askFolders": "Nothing to report yet. Add a folder holding this release's disks and archives, and ART will say here what it found.",
```

`src/i18n/tr.json`, same place:

```json
"askFolders": "Henüz bildirilecek bir şey yok. Bu sürümün disklerini ve arşivlerini tutan bir klasör ekleyin; ART ne bulduğunu burada söyler.",
```

Edit with the Edit tool (the files are UTF-8 with Turkish letters; never a shell heredoc). Then:

```powershell
pnpm vitest run -t "have identical key sets"
```
Expected: passed.

- [ ] **Step 4: Lay the two columns out in `OsInstall.tsx`**

Replace, in order: the `<MaterialFolders …/>` element Task 3 wrote, the two ART-256 lines (`osinstall-media-empty` / `osinstall-media-found`) and the `<MaterialReadout …/>` element with its comment, by this single block (the props of both components are exactly as before):

```tsx
        {/*
          **The answer first, the question beside it** (simplification design
          § 4). Two columns at a comfortable width, one under the other at a
          narrow one — a wrapping flex row, no stylesheet, no media query,
          because the builder is inline-styled and a query cannot see the
          shell's zoom (dockLayout.ts's own argument). The readout is the
          primary column and comes **first in the DOM**: first for a screen
          reader, and on top when the row wraps.

          **With no folders the primary column still speaks.** The readout
          renders nothing for an empty list — right, because a set line about
          a release the user has pointed nothing at would be a claim about
          their disk — so the step says what to do next in its place. A
          question nobody has asked is not an answer of "nothing found", and
          the two never share a sentence.
        */}
        <div
          data-testid="source-columns"
          style={{ display: "flex", flexWrap: "wrap", gap: 16, alignItems: "flex-start", margin: "0 0 12px" }}
        >
          <div data-testid="source-primary" style={{ flex: "1 1 26em", minWidth: 0 }}>
            {materialFolders.length === 0 && (
              <p className="muted" data-testid="material-ask" style={{ fontSize: 12, margin: 0 }}>
                {t("osinstall.material.askFolders")}
              </p>
            )}
            {/*
              **The readout: one row per artefact this release can use**
              (intake design § 3.3). Per *slot* — what the release needs and
              which file fills it — where the identity lines further down are
              per *file*.
            */}
            <MaterialReadout
              release={release}
              folders={materialFolders.map((entry) => entry.path)}
              treeRoot={packagesTreeRoot}
              rom={romPath}
              identifiedPass={identifiedPass}
              overrides={materialOverrides}
            />
          </div>
          <div data-testid="source-secondary" style={{ flex: "1 1 18em", minWidth: 0 }}>
            <MaterialFolders
              release={release}
              folders={materialFolders}
              layers={layers}
              layerLabel={layerLabel}
              folderScans={folderScans}
              wrongLayerHint={wrongLayerHintFor}
              unusedForPlan={plannedFolders.unusedForPlan}
              amigaForeverOffer={amigaForeverOffer}
              onAdd={() => void addFolder()}
              onRemove={removeFolder}
              onTag={tagFolder}
              // Only reachable from a button drawn while the offer is a string.
              onAmigaForeverAdd={() => {
                addMaterialFolder(amigaForeverOffer!);
                setAmigaForeverDismissed(true);
              }}
              onAmigaForeverDismiss={() => setAmigaForeverDismissed(true)}
            />
            {/*
              ART-256. Both lines read `foundVolumeNames`, which is every
              folder the request carries. Two sentences on one screen counting
              the same disks differently — "1 install disk found" above an
              evidence line naming two — is a contradiction from the inside.
              They are claims about the *folders'* scans, so they live in the
              folder column (the readout is per slot). ART-285 — that this
              sentence counts game discs as install disks — is open and not
              touched by the move.
            */}
            {materialFolders.length > 0 && foundVolumeNames.length === 0 && (
              <p id="osinstall-media-empty" className="faint" style={{ fontSize: 11, margin: "8px 0 0" }}>
                {t("osinstall.media.empty")}
              </p>
            )}
            {foundVolumeNames.length > 0 && (
              <p id="osinstall-media-found" className="faint" style={{ fontSize: 11, margin: "8px 0 0" }}>
                {t("osinstall.media.found", {
                  count: foundVolumeNames.length,
                  names: foundVolumeNames.join(", "),
                })}
              </p>
            )}
          </div>
        </div>
```

The `media-identity` block that followed the readout stays exactly where it is, now after the closing `</div>` of `source-columns`. Nothing about the release `<select>` above the row changes.

- [ ] **Step 5: Run the four, then the file, then everything**

```powershell
pnpm vitest run src/components/osbuilder/OsInstall.test.tsx -t "the readout first"
pnpm vitest run src/components/osbuilder/OsInstall.test.tsx
pnpm lint
pnpm test
```
Expected: 4 passed; the file's count is Task 3's count + 4; lint clean twice; `pnpm test` all passed. Quote the `Tests  N passed` line of the last run.

- [ ] **Step 6: Commit**

`<scratchpad>\msg4.txt`:

```
kaynak: the readout first, the folders beside it, and an ask for none

Simplification design § 4 (round A). A wrapping flex row with the readout
first in the DOM and the folder column second; the ART-256 found/empty
lines move with the folders because they are about the folders' scans.
With no folder the readout renders nothing, as before, and the step says
what to do next in its place (osinstall.material.askFolders, both
catalogues) — never a set line about a disk nobody pointed at.

Four component tests: DOM order, the ask with no folder, the ask gone with
one, the Turkish sentence.
```

```powershell
git branch --show-current
git add src/components/osbuilder/OsInstall.tsx src/components/osbuilder/OsInstall.test.tsx src/i18n/en.json src/i18n/tr.json
git commit -F "<scratchpad>\msg4.txt"
```

---

### Task 5: Mutations, the sweeps, and the docs

**Files:**
- Create: `.superpowers/sdd/2026-09-09-readout-first/report.md` (local-only; `.superpowers/` is not tracked — confirm with `git check-ignore .superpowers` before assuming)
- Modify: `docs/FEATURES.md` (rows *One material folder list…*, *Offer Amiga Forever's own folders*, *Write "what goes in this folder"…*), `docs/session-log.md`, `docs/STATUS.md`, `CHANGELOG.md`

- [ ] **Step 1: Put each defect back and watch the guard fail**

Run each mutation from the repository root with this script pattern, written to the scratchpad with the Write tool (Windows paths — never a heredoc). One script, three mutations, each restored before the next:

```python
# <scratchpad>\mutate.py
import shutil, subprocess, sys, pathlib
ROOT = pathlib.Path(r"D:\Projeler\Amiga\amiga-retro-toolkit")
SCRATCH = pathlib.Path(r"<scratchpad>")

def mutate(rel, old, new, test_file, test_name):
    src = ROOT / rel
    backup = SCRATCH / (src.name + ".bak")
    shutil.copyfile(src, backup)                      # absolute, before touching
    text = src.read_text(encoding="utf-8")
    assert text.count(old) == 1, f"{rel}: expected exactly one occurrence of {old!r}, found {text.count(old)}"
    src.write_text(text.replace(old, new), encoding="utf-8")
    try:
        r = subprocess.run(["pnpm", "vitest", "run", test_file, "-t", test_name],
                           cwd=ROOT, capture_output=True, text=True, shell=True)
        tail = "\n".join((r.stdout + r.stderr).splitlines()[-25:])
        print(f"=== {rel} :: {test_name} -> exit {r.returncode}\n{tail}\n")
    finally:
        shutil.copyfile(backup, src)                  # copyfile, never move

# M1 — swap the two columns: the readout renders second.
mutate("src/components/osbuilder/OsInstall.tsx",
       'data-testid="source-primary" style={{ flex: "1 1 26em", minWidth: 0 }}',
       'data-testid="source-primary" style={{ flex: "1 1 26em", minWidth: 0, order: 2 }}',
       "src/components/osbuilder/OsInstall.test.tsx", "puts the readout before the folder list")
```

**Stop.** M1 as written above is a *wrong mutation*: `order: 2` changes the paint order, not the DOM order, and the guard is a DOM-order guard on purpose — so it must survive, and the survival is the mutation's fault. Record it as such in the report (*a survivor is one of two things*), and run the real M1: cut the whole `source-primary` div and paste it after the `source-secondary` div. Do that with two `replace`s on the file text in the same `mutate` call (old = the primary div's text, new = `""`, then insert it before the closing `</div>` of `source-columns`) — or by hand with the Edit tool, run the one test, and restore with `shutil.copyfile` from the `.bak`. Expected: `puts the readout before the folder list` **fails**.

```python
# M2 — delete the ask: an empty list reports nothing at all.
mutate("src/components/osbuilder/OsInstall.tsx",
       '{materialFolders.length === 0 && (\n              <p className="muted" data-testid="material-ask"',
       '{false && (\n              <p className="muted" data-testid="material-ask"',
       "src/components/osbuilder/OsInstall.test.tsx", "asks for a folder rather than reporting on none")
# Expected: fails (material-ask not found).

# M3 — collapse "already there" into "failed" (the create_new ending lost).
mutate("src/components/osbuilder/MaterialFolders.tsx",
       'data-testid={`material-guide-${answer.state}`}',
       'data-testid={`material-guide-${answer.state === "alreadyThere" ? "failed" : answer.state}`}',
       "src/components/osbuilder/MaterialFolders.test.tsx", "keeps 'already there' apart from a failure")
# Expected: fails.

# M4 — the readout draws the guide again (a second button on screen).
mutate("src/components/osbuilder/MaterialReadout.tsx",
       "{crowded.map((line) => (",
       '<div data-testid="material-guide" /> \n      {crowded.map((line) => (',
       "src/components/osbuilder/MaterialReadout.test.tsx", "draws no guide button")
# Expected: fails.
```

```powershell
python "<scratchpad>\mutate.py"
git status --short      # must show NO modified source file afterwards — every restore landed
```

- [ ] **Step 2: The full verification, unpiped**

```powershell
pnpm lint
pnpm test
pnpm vitest run -t "have identical key sets"
python scripts/control-byte-sweep.py
python scripts/contrast-check.py --quiet
git diff --stat main -- src-tauri
```
Expected: lint clean twice; `pnpm test` all files passed (quote `Test Files` and `Tests` lines); parity passed; both sweeps clean; the last command prints nothing (no Rust changed, so `cargo` is not re-run — say so in the report).

- [ ] **Step 3: The report**

Write `.superpowers/sdd/2026-09-09-readout-first/report.md`:

```markdown
# Simplification round A — the readout first — report (2026-09-09)

Branch `art-simplify-a-readout-first`, from `main` at `<sha>`. Spec § 4.

## What the tree held vs. the spec
- SourceStep.tsx does not exist (round C's); the layout landed in OsInstall.tsx.
- The ART-256 found/empty lines went to the folder column (about the folders' scans).
- media-identity stays below the row, full width.

## Tests moved, by name (before → after)
| Case | From | To |
|---|---|---|
| writes nothing at all until the button is pressed | MaterialReadout.test.tsx | MaterialFolders.test.tsx |
| writes one guide per folder, in the folder that button names | MaterialReadout.test.tsx | MaterialFolders.test.tsx |
| keeps 'already there' apart from a failure, and names the file to delete | MaterialReadout.test.tsx | MaterialFolders.test.tsx |
| says a real failure is ART's, not a statement about the folder | MaterialReadout.test.tsx | MaterialFolders.test.tsx |
| asks for the guide in the language on screen | MaterialReadout.test.tsx | MaterialFolders.test.tsx |

Counts: MaterialReadout.test.tsx <before> → <after>; MaterialFolders.test.tsx 0 → <n>; OsInstall.test.tsx <before> → <after>. No case dropped.

## Mutations
| # | Mutation | Guard | Result |
|---|---|---|---|
| M1a | `order: 2` on the primary column | puts the readout before the folder list | **survived — wrong mutation** (paint order, not DOM order; the guard is DOM order on purpose) |
| M1b | primary div moved after secondary in JSX | same | killed |
| M2 | ask branch → `false &&` | asks for a folder rather than reporting on none | killed |
| M3 | alreadyThere rendered as failed | keeps 'already there' apart from a failure | killed |
| M4 | readout renders a `material-guide` again | draws no guide button | killed |

## Verification (quoted)
<paste the `Tests  N passed` line, the parity line, both sweep outputs, and the empty `git diff --stat main -- src-tauri`>

## Still owed by a person (spec § 4.3)
Open `kaynak` on a fresh profile; then with the owner's three folders; read the top of the screen without scrolling at 100 % and 200 %. Build: Task 6.
```

Fill every `<…>` with the measured value; a placeholder left in the report is a plan failure.

- [ ] **Step 4: The tracking docs**

- `docs/FEATURES.md`:
  - Row *One material folder list for the whole build*: replace `the \`kaynak\` step's own list` with `\`components/osbuilder/MaterialFolders.tsx\` (the folder column, since 2026-09-09)` and append to the tests cell: ` + 9 component tests on the column`.
  - Row *Offer Amiga Forever's own folders*: replace `the \`kaynak\` step's one suggestion line` with `the suggestion line in \`MaterialFolders.tsx\``.
  - Row *Write "what goes in this folder" into a material folder*: replace `the button on \`MaterialReadout\`` with `the button on \`MaterialFolders\` (moved from the readout on 2026-09-09 — an action on a folder lives with the folders)`.
  - Row *A per-artefact readout on the source step*: append before the final `|`: ` **2026-09-09: first on the screen.** The readout is the primary column of the source step and precedes the folder list in the DOM (simplification design § 4; guard \`puts the readout before the folder list in document order\`), and an empty list is met with the next action rather than a set line (\`asks for a folder rather than reporting on none\`).`
- `docs/session-log.md`: one row at the top:
  `| 2026-09-09 | **Simplification round A — the readout first** (spec § 4). \`kaynak\` opens on the readout, the folder column beside it (\`MaterialFolders.tsx\`, cut from \`OsInstall.tsx\`; the guide block moved from the readout with its five tests by name); an empty list asks instead of reporting. Four mutations, three killed, one disclosed as a wrong mutation (paint order is not DOM order). Not yet driven by a person. | Vitest <files> / <tests> |`
- `docs/STATUS.md` "Start here" block, **in place**: in item 00, replace the first sentence (`art-091-fixes` is live and not merged) with `**\`art-091-fixes\` was merged as \`7ee91fa\` and is 0.9.1; item 0 has the release.**` — it was stale on 2026-09-09 morning. In item 0, replace `**Next: round 4** — \`docs/superpowers/specs/2026-09-08-os-builder-simplification-design.md\` (…)` with `**Round A of the simplification spec is on \`art-simplify-a-readout-first\`, unmerged** (plan \`docs/superpowers/plans/2026-09-09-readout-first.md\`, report \`.superpowers/sdd/2026-09-09-readout-first/report.md\`): the readout first. Owed before merge: the owner drives § 4.3 on the build under \`E:\\amiga\\ProjeART\\build\`. Then rounds B (one updates screen), C (three steps), D (the commander's columns, its own worktree)` and keep the rest of the sentence (ART-281, the chain on the real screen).
- `CHANGELOG.md` `[Unreleased]`: under *Changed*: `- OS Builder, source step: what ART found comes first, with the folder list beside it; with no folder chosen the step says what to do next instead of reporting nothing.`

- [ ] **Step 5: Commit the docs**

`<scratchpad>\msg5.txt`:

```
Docs for simplification round A: FEATURES, session log, STATUS, CHANGELOG

Four mutations run, three killed, one disclosed as a wrong mutation
(order: 2 changes paint order, not DOM order — the guard is DOM order on
purpose). STATUS's stale item 00 corrected in place: art-091-fixes merged
as 7ee91fa.
```

```powershell
git branch --show-current
git add docs/FEATURES.md docs/session-log.md docs/STATUS.md CHANGELOG.md
git commit -F "<scratchpad>\msg5.txt"
```

---

### Task 6: The build the owner drives

**Files:** none in the tree.

- [ ] **Step 1: Build**

```powershell
cd D:\Projeler\Amiga\amiga-retro-toolkit
pnpm tauri build
```
Expected: ends with the two bundle paths under `src-tauri\target\release\bundle\`. **Do not pipe it.** If it fails, the failure is the report's first line and Task 6 stops there.

- [ ] **Step 2: Put the installer where the owner tries things**

```powershell
Copy-Item "src-tauri\target\release\bundle\nsis\Amiga Retro Toolkit_0.9.1_x64-setup.exe" "E:\amiga\ProjeART\build\Amiga Retro Toolkit_0.9.1_x64-setup_simplify-a.exe"
Get-Item "E:\amiga\ProjeART\build\Amiga Retro Toolkit_0.9.1_x64-setup_simplify-a.exe" | Select-Object Length, LastWriteTime
```
(The exact NSIS filename is whatever `pnpm tauri build` printed; use that.)

- [ ] **Step 3: Tell the owner what to look at** — the final message names the file and the three things from spec § 4.3: a fresh profile opens on a sentence, not an empty box; with the three folders the readout is on top without scrolling at 100 % and 200 %; the guide button still says *already there* for a folder that has one. Merge to `main` (`--no-ff`, branch deleted) happens after that, on the owner's word, and `main` stays unpushed.

---

## Self-review

**Spec coverage (§ 4):** § 4.1 layout and DOM order — Task 4; the empty-list ask — Task 4; the guide block with the folders — Tasks 2-3; § 4.2 `MaterialFolders.tsx` new, `MaterialReadout.tsx` guide only, the layout in the step (not `SourceStep`, corrected above) — Tasks 2-4; § 4.3's three guards — order (Task 4 test 1), the ask (Task 4 test 2), the moved guide cases (Task 2); § 4.3's person — Task 6; § 6 row A "one component split out, one layout, three tests" — three tests became four plus the column's own nine, disclosed; § 8 verification — Task 5, `cargo` skipped with the reason.

**Placeholders:** the three arrows in Task 2 Step 3 point at line ranges that exist in the tree and are to be moved verbatim, not written; the report's `<…>` fields are measurements the executor fills and Step 3 says so.

**Type consistency:** `MaterialFoldersProps` is defined once (Task 2) and consumed with the same thirteen names in Task 3 Step 5 and Task 4 Step 4; the four new testids are named in Task 4's Interfaces and used in its tests and its JSX; `guideMock` leaves `MaterialReadout.test.tsx` in Task 3 and lives in `MaterialFolders.test.tsx` from Task 2.
