# Four tabs, round 2 — tab 1 and tab 3 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Kickstart and the destination move to tab 3 (`makine`) as their own component; tab 1 (`dosyalar`) loses those two fields and folds the per-file identity wall behind one closed line.

**Architecture:** Two small hooks in `src/lib` — `useRomIdentity(path)` and `useDestinationCheck(path)` — take the two effects out of `OsInstall.tsx` so tab 1 (which still plans and runs in this round) and tab 3 (which shows the fields) ask the core once each through the same code and cannot disagree. `MachineTab.tsx` is a new presentational component reading `session.rom` and the remembered destination through the same key `OsInstall` uses. `OsInstall.tsx` keeps components, refusals, replaces, plan, keymap, run and result (rounds 3-4 move them) and the two panels at its foot.

**Tech Stack:** React 18, react-i18next, Vitest + jsdom + Testing Library (`renderHook`), `@tauri-apps/plugin-dialog`.

**Spec:** `docs/superpowers/specs/2026-09-09-os-builder-four-tabs-design.md` § 3.1 (tab 1), § 3.3 (tab 3), § 8 row 2, § 10.5.

**Rulings against the spec, made while planning on 2026-09-09:**

- **The keymap select stays on tab 1 in this round.** § 3.3 puts it on tab 3, but its option list is `keymapsIn(effectivePlan)` (`OsInstall.tsx:1547-1550`) — it is derived from the plan, and the plan is round 4's hook. Moving the select without the plan would mean computing a second plan on tab 3. It moves in round 4 with `useInstallPlan`. Disclosed in the round report and in `StepMakine`'s own sentence.
- **§ 3.3's "this tree will be updated: AmigaOS 3.9, built 2026-09-08" is cut to what `osinstall_describe_tree` answers:** `TreeSummary { isTree, release, files, components, amigaInstalled, problem }` has no build date. The sentence is *"This folder holds a tree ART built: AmigaOS 3.9, 1 915 files."* Whether a run *updates* such a tree is round 4's question; tab 3 only says what is there. `osinstall_destination_taken` still refuses a folder with content, as today — the refusal sentence and the tree sentence can both show, and the refusal stays the last word until round 4 decides update mode.
- **The identification pass keeps running** (spec § 1.2); only the lines fold. The reuse-scan toggle and *Scan again* go inside the fold, because they are about that pass.
- **`hostAmigaForeverFolders()` is called by both tabs** (tab 1 for the disks offer, tab 3 for the ROM offer). One cheap command, no shared state, no hook — YAGNI.

## Global Constraints

- Nothing changes unless the user changes it: `MachineTab` reads `session.rom` and `rememberedComponentKey("osinstall.destination", release)` with `isTextOrNothing` exactly as `OsInstall.tsx:619-623`; no new remembered key; nothing written on render.
- Endings stay distinct: the ROM's *identified* and *unreadable* sentences keep their ids (`osinstall-rom-identified`, `osinstall-rom-unreadable`) and ART-241's `describedBy` wiring; the destination's *taken* refusal and *tree* sentence are two sentences.
- `data-testid`s keep their names: `osinstall-rom-field`, `amiga-forever-rom-offer`, `media-identity`, `media-identity-*`, `media-identity-summary`.
- Both catalogues in one commit; parity, literal-keys tests; `src/lib` returns keys or data, never strings.
- No Rust change (`git diff --stat main -- src-tauri` empty). Branch `art-four-tabs`; `git branch --show-current` before every commit; commit messages via a scratchpad file; mutations by `shutil.copyfile`.
- Scratchpad: `C:\Users\ismoz\AppData\Local\Temp\claude\D--Projeler-Amiga\06212805-e1dd-4b6f-b2ae-dd6f17219be6\scratchpad`.
- Every `it(...)` that leaves `OsInstall.test.tsx` is listed by name in the round report with its new home.

---

## File structure

**Create**
- `src/lib/useRomIdentity.ts` (+ `.test.tsx`) — `useRomIdentity(path: string | null): { rom: RomInfo | null; unreadable: boolean }`, the cancellable effect from `OsInstall.tsx:1103-1125`.
- `src/lib/useDestinationCheck.ts` (+ `.test.tsx`) — `useDestinationCheck(path: string | null): { taken: boolean; tree: TreeSummary | null }`, the effect from `OsInstall.tsx:1336-1352` plus `osinstallDescribeTree`.
- `src/components/osbuilder/MachineTab.tsx` (+ `.test.tsx`) — the Kickstart field with the Amiga Forever ROM offer and the two outcome sentences; the destination field with the taken refusal and the tree sentence.

**Modify**
- `src/components/osbuilder/OsInstall.tsx` — the two effects become the hooks; the ROM field, ROM offer, ROM sentences and destination field are removed from the render; the identity wall is folded.
- `src/components/osbuilder/OsInstall.test.tsx` — six cases move out by name; the mount case and the ART-194 cases adjust; three fold cases added.
- `src/pages/osbuilder/steps.tsx` — `StepMakine` mounts `MachineTab` with a one-line note about the keymap.
- `src/i18n/en.json`, `src/i18n/tr.json` — `osinstall.destination.{taken,tree,fresh}`, `osinstall.mediaId.foldSummary`, `osBuilder.tab.makineKeymapNote`; `osBuilder.tab.makineNotYet` deleted.
- Docs (Task 4).

---

### Task 1: The two hooks, and `OsInstall` on them

**Files:**
- Create: `src/lib/useRomIdentity.ts`, `src/lib/useRomIdentity.test.tsx`, `src/lib/useDestinationCheck.ts`, `src/lib/useDestinationCheck.test.tsx`
- Modify: `src/components/osbuilder/OsInstall.tsx:809-810` (state), `:1103-1125` (ROM effect), `:1336-1352` (taken effect)
- Test: `src/components/osbuilder/OsInstall.test.tsx` (unchanged; the regression net)

**Interfaces:**
- Produces:

```ts
// src/lib/useRomIdentity.ts
export interface RomIdentity { rom: RomInfo | null; unreadable: boolean }
export function useRomIdentity(path: string | null): RomIdentity;

// src/lib/useDestinationCheck.ts
export interface DestinationCheck { taken: boolean; tree: TreeSummary | null }
export function useDestinationCheck(path: string | null): DestinationCheck;
```

- [ ] **Step 1: The hooks' tests**

`src/lib/useRomIdentity.test.tsx`:

```tsx
// @vitest-environment jsdom
//
// One question — "what Kickstart is this file" — asked from one place, so
// tab 1 (which plans) and tab 3 (which shows the field) cannot answer it
// differently. The two outcomes stay two: identified, or unreadable.
// A superseded answer is dropped (ART-089's own mechanism).

import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, renderHook, waitFor } from "@testing-library/react";

const identifyMock = vi.hoisted(() => vi.fn());
vi.mock("@/lib/pistorm", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/pistorm")>()),
  pistormIdentifyRom: identifyMock,
}));

const { useRomIdentity } = await import("@/lib/useRomIdentity");

afterEach(() => {
  cleanup();
  identifyMock.mockReset();
});

describe("useRomIdentity", () => {
  it("answers nothing, and asks nothing, without a path", () => {
    const { result } = renderHook(() => useRomIdentity(null));
    expect(result.current).toEqual({ rom: null, unreadable: false });
    expect(identifyMock).not.toHaveBeenCalled();
  });

  it("identifies the file, and clears the answer when the path goes", async () => {
    identifyMock.mockResolvedValue({ name: "Kickstart 3.2 (47.96)" });
    const { result, rerender } = renderHook(({ p }) => useRomIdentity(p), {
      initialProps: { p: "E:\\rom\\kick.rom" as string | null },
    });
    await waitFor(() => expect(result.current.rom?.name).toBe("Kickstart 3.2 (47.96)"));
    expect(result.current.unreadable).toBe(false);
    rerender({ p: null });
    expect(result.current).toEqual({ rom: null, unreadable: false });
  });

  it("says unreadable when the core refuses, never both", async () => {
    identifyMock.mockRejectedValue(new Error("not a Kickstart"));
    const { result } = renderHook(() => useRomIdentity("E:\\rom\\junk.bin"));
    await waitFor(() => expect(result.current.unreadable).toBe(true));
    expect(result.current.rom).toBeNull();
  });

  it("drops a superseded answer", async () => {
    let resolveFirst: (v: { name: string }) => void = () => {};
    identifyMock
      .mockImplementationOnce(() => new Promise((r) => { resolveFirst = r; }))
      .mockResolvedValueOnce({ name: "second" });
    const { result, rerender } = renderHook(({ p }) => useRomIdentity(p), {
      initialProps: { p: "E:\\a.rom" as string | null },
    });
    rerender({ p: "E:\\b.rom" });
    await waitFor(() => expect(result.current.rom?.name).toBe("second"));
    resolveFirst({ name: "first" });
    await new Promise((r) => setTimeout(r, 0));
    expect(result.current.rom?.name).toBe("second");
  });
});
```

`src/lib/useDestinationCheck.test.tsx`:

```tsx
// @vitest-environment jsdom
//
// Two facts about the destination folder, asked once: is it occupied
// (`osinstall_destination_taken`, which is what `apply` would refuse on) and
// is it a tree ART built (`osinstall_describe_tree`). Both tabs read this;
// neither asks on its own. A path ART cannot examine is not declared taken
// — `apply()` decides, and blocking here would refuse an install the engine
// would have allowed.

import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, renderHook, waitFor } from "@testing-library/react";

const takenMock = vi.hoisted(() => vi.fn());
const describeMock = vi.hoisted(() => vi.fn());
vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallDestinationTaken: takenMock,
  osinstallDescribeTree: describeMock,
}));

const { useDestinationCheck } = await import("@/lib/useDestinationCheck");

const TREE = { isTree: true, release: "AmigaOS 3.9", files: 1915, components: ["workbench-base"], amigaInstalled: [], problem: null };
const NOT_TREE = { isTree: false, release: null, files: 0, components: [], amigaInstalled: [], problem: "no distribution.json" };

afterEach(() => {
  cleanup();
  takenMock.mockReset();
  describeMock.mockReset();
});

describe("useDestinationCheck", () => {
  it("asks nothing without a path", () => {
    const { result } = renderHook(() => useDestinationCheck(null));
    expect(result.current).toEqual({ taken: false, tree: null });
    expect(takenMock).not.toHaveBeenCalled();
    expect(describeMock).not.toHaveBeenCalled();
  });

  it("reports an occupied folder and a tree apart", async () => {
    takenMock.mockResolvedValue(true);
    describeMock.mockResolvedValue(TREE);
    const { result } = renderHook(() => useDestinationCheck("E:\\dist"));
    await waitFor(() => expect(result.current.taken).toBe(true));
    await waitFor(() => expect(result.current.tree?.isTree).toBe(true));
    expect(result.current.tree?.files).toBe(1915);
  });

  it("does not declare a folder taken when ART could not look", async () => {
    takenMock.mockRejectedValue(new Error("access is denied"));
    describeMock.mockResolvedValue(NOT_TREE);
    const { result } = renderHook(() => useDestinationCheck("E:\\dist"));
    await waitFor(() => expect(describeMock).toHaveBeenCalled());
    expect(result.current.taken).toBe(false);
  });

  it("keeps a non-tree's answer as not-a-tree, never null, once ART has looked", async () => {
    takenMock.mockResolvedValue(false);
    describeMock.mockResolvedValue(NOT_TREE);
    const { result } = renderHook(() => useDestinationCheck("E:\\empty"));
    await waitFor(() => expect(result.current.tree).not.toBeNull());
    expect(result.current.tree?.isTree).toBe(false);
  });

  it("clears both when the path goes", async () => {
    takenMock.mockResolvedValue(true);
    describeMock.mockResolvedValue(TREE);
    const { result, rerender } = renderHook(({ p }) => useDestinationCheck(p), {
      initialProps: { p: "E:\\dist" as string | null },
    });
    await waitFor(() => expect(result.current.taken).toBe(true));
    rerender({ p: null });
    expect(result.current).toEqual({ taken: false, tree: null });
  });
});
```

- [ ] **Step 2: Run, fail on the missing modules**

```powershell
pnpm vitest run src/lib/useRomIdentity.test.tsx src/lib/useDestinationCheck.test.tsx
```

- [ ] **Step 3: The hooks**

`src/lib/useRomIdentity.ts`:

```ts
// What Kickstart a file is — asked once, from one place (four-tab design
// § 3.3). Until 2026-09-09 this effect lived in `OsInstall.tsx`; the
// Kickstart field now sits on tab 3 while the plan on tab 1 still needs the
// ROM's name for a component's condition line, and two copies of one effect
// is two answers to one question.

import { useEffect, useState } from "react";

import { pistormIdentifyRom, type RomInfo } from "@/lib/pistorm";

export interface RomIdentity {
  rom: RomInfo | null;
  /** The core could not read the file as a Kickstart. Never true while
   *  `rom` is set: the two endings are exclusive by construction. */
  unreadable: boolean;
}

export function useRomIdentity(path: string | null): RomIdentity {
  const [rom, setRom] = useState<RomInfo | null>(null);
  const [unreadable, setUnreadable] = useState(false);

  useEffect(() => {
    if (!path) {
      setRom(null);
      setUnreadable(false);
      return;
    }
    let cancelled = false;
    pistormIdentifyRom(path)
      .then((r) => {
        if (cancelled) return;
        setRom(r);
        setUnreadable(false);
      })
      .catch(() => {
        if (cancelled) return;
        setRom(null);
        setUnreadable(true);
      });
    return () => {
      cancelled = true;
    };
  }, [path]);

  return { rom, unreadable };
}
```

`src/lib/useDestinationCheck.ts`:

```ts
// Two facts about the destination folder (four-tab design § 3.3): whether
// `apply` would refuse it as occupied, and whether it is a tree ART built.
// Asked once, here, for both tabs. A path ART cannot examine is **not**
// declared taken — `apply()` decides, and blocking here would refuse an
// install the engine would have allowed (the rule the effect carried in
// `OsInstall.tsx` since ART-119).

import { useEffect, useState } from "react";

import { osinstallDescribeTree, osinstallDestinationTaken, type TreeSummary } from "@/lib/osinstall";

export interface DestinationCheck {
  taken: boolean;
  /** `null` until ART has looked, or when there is no path. Once looked, a
   *  non-tree answers `isTree: false` with its `problem` — never `null`,
   *  so "not asked" and "not a tree" stay apart. */
  tree: TreeSummary | null;
}

export function useDestinationCheck(path: string | null): DestinationCheck {
  const [taken, setTaken] = useState(false);
  const [tree, setTree] = useState<TreeSummary | null>(null);

  useEffect(() => {
    if (!path) {
      setTaken(false);
      setTree(null);
      return;
    }
    let cancelled = false;
    void osinstallDestinationTaken(path)
      .then((answer) => {
        if (!cancelled) setTaken(answer);
      })
      .catch(() => {
        if (!cancelled) setTaken(false);
      });
    void osinstallDescribeTree(path)
      .then((summary) => {
        if (!cancelled) setTree(summary);
      })
      .catch(() => {
        if (!cancelled) setTree(null);
      });
    return () => {
      cancelled = true;
    };
  }, [path]);

  return { taken, tree };
}
```

- [ ] **Step 4: `OsInstall.tsx` on the hooks**

Replace `const [rom, setRom] = useState<RomInfo | null>(null); const [romError, setRomError] = useState(false);` (809-810) with `const { rom, unreadable: romError } = useRomIdentity(romPath);` — placed **after** `romPath` is defined (611). Delete the effect at 1103-1125 and its comment. Replace `const [destinationTaken, setDestinationTaken] = useState(false);` and the effect at 1336-1352 with `const { taken: destinationTaken } = useDestinationCheck(destination);` (keep the comment above it, trimmed to say the hook holds the rule). Remove the now-unused imports (`pistormIdentifyRom`, `RomInfo` if unused, `osinstallDestinationTaken`). Add the two hook imports.

- [ ] **Step 5: Green — hooks, then the whole install screen unchanged**

```powershell
pnpm vitest run src/lib/useRomIdentity.test.tsx src/lib/useDestinationCheck.test.tsx src/components/osbuilder/OsInstall.test.tsx
pnpm lint
```
Expected: hooks 4 + 5 passed; `OsInstall.test.tsx` **105 passed, unchanged**; lint clean twice. (`OsInstall.test.tsx` mocks `pistormIdentifyRom` as `identifyRomMock` and `osinstallDestinationTaken` at the `@/lib/*` boundary — the hooks import from the same modules, so the mocks still apply. If a case fails because `osinstallDescribeTree` is not mocked in that file, add `osinstallDescribeTree: describeTreeMock` with a default `mockResolvedValue({ isTree: false, release: null, files: 0, components: [], amigaInstalled: [], problem: null })` to its `@/lib/osinstall` mock and say so in the report.)

- [ ] **Step 6: Commit**

`<scratchpad>\msg-r2-t1.txt`: *"Two hooks for the Kickstart and the destination — useRomIdentity, useDestinationCheck — cut out of OsInstall.tsx so tab 3 can ask the same questions through the same code (four-tab design § 3.3). Behaviour unchanged; OsInstall.test.tsx 105 as before."*

```powershell
git branch --show-current
git add src/lib/useRomIdentity.ts src/lib/useRomIdentity.test.tsx src/lib/useDestinationCheck.ts src/lib/useDestinationCheck.test.tsx src/components/osbuilder/OsInstall.tsx
git commit -F "<scratchpad>\msg-r2-t1.txt"
```

---

### Task 2: `MachineTab` — Kickstart and destination on tab 3

**Files:**
- Create: `src/components/osbuilder/MachineTab.tsx`, `src/components/osbuilder/MachineTab.test.tsx`
- Modify: `src/components/osbuilder/OsInstall.tsx` (remove the ROM field block `:1919-1981` and the destination field `:1983-1990`, the `chooseRom`/`chooseRomIn`/`chooseDestination` functions, the `amigaForeverRom`/`amigaForeverRomDismissed` state and the `amigaForeverRomOffer` derivation, `setRomPath` if unused — keep `romPath`, `rom`, `destination`, `destinationTaken`, which the plan and refusals still read)
- Modify: `src/components/osbuilder/OsInstall.test.tsx` (six cases leave; the mount case adjusts)
- Modify: `src/pages/osbuilder/steps.tsx` (`StepMakine`)
- Modify: `src/i18n/en.json`, `src/i18n/tr.json`

**Interfaces:**
- Consumes: Task 1's hooks; `Field` (`label, value, empty, choose, onChoose, hint?, describedBy?, testId?`); `useBuildSession().session.rom.path` / `setRom(path)`; `rememberedComponentKey` (`@/lib/osinstall`), `useRemembered` (`@/lib/useRemembered`), `isTextOrNothing` (`@/lib/remembered`); `hostAmigaForeverFolders(): Promise<{ adf: string | null; rom: string | null }>` (`@/lib/api`); `open` from `@tauri-apps/plugin-dialog`.
- Produces: `MachineTab()`; testids `machine-tab`, `osinstall-rom-field`, `amiga-forever-rom-offer`, `osinstall-destination-field`, `osinstall-destination-taken`, `osinstall-destination-tree`, `osinstall-destination-fresh`.

- [ ] **Step 1: The tests**

`src/components/osbuilder/MachineTab.test.tsx` — the mock block is copied from `OsInstall.test.tsx` (its `vi.mock` calls for `@/lib/settings`, `@/lib/api` (`hostAmigaForeverFolders` → `amigaForeverMock`), `@/lib/pistorm` (`pistormIdentifyRom` → `identifyRomMock`), `@tauri-apps/plugin-dialog` (`open` → `dialogOpenMock`), and `@/lib/osinstall` with only `osinstallDestinationTaken` → `takenMock` and `osinstallDescribeTree` → `describeTreeMock`), plus `seedRemembered` / `rememberedBag` / `describedByIds` helpers copied by name from that file. Then:

```tsx
const AF_ROM = "E:\\amiga\\Shared\\rom";
const NOT_TREE = { isTree: false, release: null, files: 0, components: [], amigaInstalled: [], problem: "no distribution.json" };
const TREE = { isTree: true, release: "AmigaOS 3.9", files: 1915, components: ["workbench-base"], amigaInstalled: [], problem: null };

beforeEach(() => {
  amigaForeverMock.mockReset().mockResolvedValue({ adf: null, rom: null });
  identifyRomMock.mockReset().mockResolvedValue({ name: "Kickstart 3.2 (47.96)" });
  dialogOpenMock.mockReset();
  takenMock.mockReset().mockResolvedValue(false);
  describeTreeMock.mockReset().mockResolvedValue(NOT_TREE);
});

afterEach(async () => {
  cleanup();
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
  await changeLanguage("en");
});

// Moved from OsInstall.test.tsx by name on 2026-09-09 (four-tab design § 3.3).
describe("Amiga Forever's ROM folder, offered and never chosen", () => {
  it("offers the ROM folder for the Kickstart field, on its own dismissal", async () => {
    amigaForeverMock.mockResolvedValue({ adf: "E:\\amiga\\Shared\\adf", rom: AF_ROM });
    seedRemembered({ "buildSession.release": "AmigaOS 3.9" });
    render(<MachineTab />);
    const offer = await screen.findByTestId("amiga-forever-rom-offer");
    expect(offer.textContent).toContain(AF_ROM);
    expect(rememberedBag()["buildSession.rom"]).toBeUndefined();
    await userEvent.click(within(offer).getByRole("button", { name: i18n.t("osinstall.material.amigaForeverDismiss") }));
    await waitFor(() => expect(screen.queryByTestId("amiga-forever-rom-offer")).toBeNull());
  });

  it("opens the Kickstart picker on that folder, and the user still picks the file", async () => {
    amigaForeverMock.mockResolvedValue({ adf: null, rom: AF_ROM });
    seedRemembered({ "buildSession.release": "AmigaOS 3.9" });
    render(<MachineTab />);
    const offer = await screen.findByTestId("amiga-forever-rom-offer");
    dialogOpenMock.mockResolvedValueOnce("E:\\amiga\\Shared\\rom\\amiga-os-310-a1200.rom");
    await userEvent.click(within(offer).getByRole("button", { name: i18n.t("osinstall.material.amigaForeverAdd") }));
    await waitFor(() => expect(dialogOpenMock).toHaveBeenCalled());
    expect(dialogOpenMock.mock.calls.at(-1)![0]).toMatchObject({ defaultPath: AF_ROM });
    await waitFor(() => expect(rememberedBag()["buildSession.rom"]).toMatchObject({ path: "E:\\amiga\\Shared\\rom\\amiga-os-310-a1200.rom" }));
    expect(screen.queryByTestId("amiga-forever-rom-offer")).toBeNull();
  });

  it("says nothing about ROMs when a Kickstart is already chosen", async () => {
    amigaForeverMock.mockResolvedValue({ adf: null, rom: AF_ROM });
    seedRemembered({ "buildSession.release": "AmigaOS 3.9", "buildSession.rom": { path: "E:\\rom\\kick.rom" } });
    render(<MachineTab />);
    await waitFor(() => expect(amigaForeverMock).toHaveBeenCalled());
    expect(screen.queryByTestId("amiga-forever-rom-offer")).toBeNull();
  });
});

// Moved from OsInstall.test.tsx by name.
describe("ART-241: the ROM field's Browse button is described by its own outcome paragraph", () => {
  it("names the identified paragraph once the ROM resolves", async () => {
    seedRemembered({ "buildSession.release": "AmigaOS 3.9", "buildSession.rom": { path: "E:\\rom\\kick.rom" } });
    render(<MachineTab />);
    const field = screen.getByTestId("osinstall-rom-field");
    const button = within(field).getByRole("button", { name: i18n.t("common.browse") });
    const identified = await waitFor(() => {
      const el = document.getElementById("osinstall-rom-identified");
      if (!el) throw new Error("not rendered yet");
      return el;
    });
    expect(identified.textContent).toContain("Kickstart 3.2 (47.96)");
    expect(describedByIds(button)).toContain("osinstall-rom-identified");
    expect(describedByIds(button)).not.toContain("osinstall-rom-unreadable");
  });

  it("names the unreadable paragraph when the ROM fails to identify, not the identified one", async () => {
    identifyRomMock.mockReset().mockRejectedValue(new Error("not a Kickstart"));
    seedRemembered({ "buildSession.release": "AmigaOS 3.9", "buildSession.rom": { path: "E:\\rom\\junk.bin" } });
    render(<MachineTab />);
    const field = screen.getByTestId("osinstall-rom-field");
    const button = within(field).getByRole("button", { name: i18n.t("common.browse") });
    await screen.findByText(i18n.t("osinstall.rom.unreadable"));
    expect(describedByIds(button)).toContain("osinstall-rom-unreadable");
    expect(describedByIds(button)).not.toContain("osinstall-rom-identified");
    expect(screen.queryByText(/Kickstart 3\.2 \(47\.96\)/)).toBeNull();
  });

  it("names neither outcome paragraph before any ROM has been chosen", () => {
    seedRemembered({ "buildSession.release": "AmigaOS 3.9" });
    render(<MachineTab />);
    const field = screen.getByTestId("osinstall-rom-field");
    const button = within(field).getByRole("button", { name: i18n.t("common.browse") });
    expect(describedByIds(button)).not.toContain("osinstall-rom-identified");
    expect(describedByIds(button)).not.toContain("osinstall-rom-unreadable");
  });
});

describe("the destination (four-tab design § 3.3)", () => {
  it("shows the remembered folder for the release, and writes nothing by rendering", () => {
    seedRemembered({ "buildSession.release": "AmigaOS 3.9", "osinstall.destination.AmigaOS 3.9": "E:\\out\\dist" });
    render(<MachineTab />);
    expect(screen.getByTestId("osinstall-destination-field").textContent).toContain("E:\\out\\dist");
    const keys = Object.keys(rememberedBag());
    expect(keys.filter((k) => k.startsWith("osinstall.destination"))).toEqual(["osinstall.destination.AmigaOS 3.9"]);
  });

  it("says an empty folder gets a fresh tree", async () => {
    seedRemembered({ "buildSession.release": "AmigaOS 3.9", "osinstall.destination.AmigaOS 3.9": "E:\\out\\empty" });
    render(<MachineTab />);
    expect((await screen.findByTestId("osinstall-destination-fresh")).textContent).toBe(i18n.t("osinstall.destination.fresh"));
    expect(screen.queryByTestId("osinstall-destination-taken")).toBeNull();
    expect(screen.queryByTestId("osinstall-destination-tree")).toBeNull();
  });

  it("refuses an occupied folder in its own sentence, and names a tree ART built apart from it", async () => {
    takenMock.mockResolvedValue(true);
    describeTreeMock.mockResolvedValue(TREE);
    seedRemembered({ "buildSession.release": "AmigaOS 3.9", "osinstall.destination.AmigaOS 3.9": "E:\\out\\dist" });
    render(<MachineTab />);
    expect((await screen.findByTestId("osinstall-destination-taken")).textContent).toBe(i18n.t("osinstall.destination.taken"));
    expect((await screen.findByTestId("osinstall-destination-tree")).textContent).toBe(
      i18n.t("osinstall.destination.tree", { release: "AmigaOS 3.9", count: 1915 })
    );
    expect(screen.queryByTestId("osinstall-destination-fresh")).toBeNull();
  });

  it("picks a folder through the dialog and remembers it for this release only", async () => {
    seedRemembered({ "buildSession.release": "AmigaOS 3.9" });
    render(<MachineTab />);
    dialogOpenMock.mockResolvedValueOnce("E:\\out\\new");
    const field = screen.getByTestId("osinstall-destination-field");
    await userEvent.click(within(field).getByRole("button", { name: i18n.t("common.browse") }));
    await waitFor(() => expect(rememberedBag()["osinstall.destination.AmigaOS 3.9"]).toBe("E:\\out\\new"));
    expect(rememberedBag()["osinstall.destination.AmigaOS 3.2"]).toBeUndefined();
  });

  it("renders no raw key in Turkish", async () => {
    await changeLanguage("tr");
    seedRemembered({ "buildSession.release": "AmigaOS 3.9", "osinstall.destination.AmigaOS 3.9": "E:\\out\\empty" });
    render(<MachineTab />);
    const tab = screen.getByTestId("machine-tab");
    await screen.findByTestId("osinstall-destination-fresh");
    expect(tab.textContent).not.toMatch(/osinstall\.|osBuilder\./);
    expect(tab.textContent).not.toContain("{{");
  });
});
```

`describedByIds` in `OsInstall.test.tsx` is `(el) => (el.getAttribute("aria-describedby") ?? "").split(/\s+/).filter(Boolean)` — copy it.

- [ ] **Step 2: Run, fail on the missing module**

- [ ] **Step 3: `MachineTab.tsx`**

```tsx
// Tab 3 — Kickstart and destination (four-tab design § 3.3).
//
// Two fields, moved here from the source step on 2026-09-09. Each reads the
// value the rest of the build reads — `session.rom` (the machine's one
// Kickstart, ART-207's decision) and the remembered destination for this
// release — and each shows the core's own sentence under it: the ROM's
// identity or its unreadability (ART-241 wires the sentence to the Browse
// button), the destination's occupied-refusal, and what ART found there.
//
// **The keymap is not here yet.** Its option list comes from the plan, and
// the plan is round 4's hook; `StepMakine` says so in one line.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";

import { Field } from "@/components/osbuilder/Field";
import { hostAmigaForeverFolders } from "@/lib/api";
import { rememberedComponentKey } from "@/lib/osinstall";
import { isTextOrNothing } from "@/lib/remembered";
import { useBuildSession } from "@/lib/useBuildSession";
import { useDestinationCheck } from "@/lib/useDestinationCheck";
import { useRemembered } from "@/lib/useRemembered";
import { useRomIdentity } from "@/lib/useRomIdentity";

export function MachineTab() {
  const { t } = useTranslation();
  const { session, setRom } = useBuildSession();
  const release = session.release;
  const romPath = session.rom.path;
  const { rom, unreadable: romError } = useRomIdentity(romPath);

  const [destination, setDestination] = useRemembered<string | null>(
    rememberedComponentKey("osinstall.destination", release),
    isTextOrNothing,
    null
  );
  const { taken, tree } = useDestinationCheck(destination);

  // Amiga Forever's ROM folder, offered — never chosen for the user. Its
  // own dismissal, a `useState`: a suggestion is not a setting.
  const [amigaForeverRom, setAmigaForeverRom] = useState<string | null>(null);
  const [romOfferDismissed, setRomOfferDismissed] = useState(false);
  useEffect(() => {
    let current = true;
    hostAmigaForeverFolders()
      .then((found) => {
        if (current) setAmigaForeverRom(found.rom);
      })
      .catch(() => {
        if (current) setAmigaForeverRom(null);
      });
    return () => {
      current = false;
    };
  }, []);
  const amigaForeverRomOffer = !romPath && !romOfferDismissed ? amigaForeverRom : null;

  async function chooseRomIn(defaultPath?: string) {
    const picked = await open({
      multiple: false,
      title: t("osinstall.rom.chooseTitle"),
      defaultPath,
      filters: [{ name: "Kickstart ROM", extensions: ["rom", "bin"] }],
    });
    if (typeof picked === "string") {
      setRom(picked);
      setRomOfferDismissed(true);
    }
  }

  async function chooseDestination() {
    const picked = await open({ directory: true, multiple: false, title: t("osinstall.destination.chooseTitle") });
    if (typeof picked === "string") setDestination(picked);
  }

  return (
    <section className="card" data-testid="machine-tab" style={{ marginBottom: 16 }}>
      <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osBuilder.step.makine")}</h2>

      <Field
        label={t("osinstall.rom.label")}
        value={romPath}
        empty={t("osinstall.rom.none")}
        onChoose={() => void chooseRomIn(undefined)}
        choose={t("common.browse")}
        hint={t("osinstall.rom.hint")}
        testId="osinstall-rom-field"
        describedBy={romError ? "osinstall-rom-unreadable" : rom ? "osinstall-rom-identified" : undefined}
      />
      {amigaForeverRomOffer && (
        <p className="faint" data-testid="amiga-forever-rom-offer" style={{ fontSize: 11, margin: "0 0 12px", display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
          <span>{t("osinstall.material.amigaForeverRom", { path: amigaForeverRomOffer })}</span>
          <button className="btn" style={{ fontSize: 11 }} onClick={() => void chooseRomIn(amigaForeverRomOffer)}>
            {t("osinstall.material.amigaForeverAdd")}
          </button>
          <button className="btn" style={{ fontSize: 11 }} onClick={() => setRomOfferDismissed(true)}>
            {t("osinstall.material.amigaForeverDismiss")}
          </button>
        </p>
      )}
      {romError && (
        <p id="osinstall-rom-unreadable" className="badge badge-err" style={{ fontSize: 11, margin: "0 0 12px", display: "inline-block" }}>
          {t("osinstall.rom.unreadable")}
        </p>
      )}
      {rom && (
        <p id="osinstall-rom-identified" className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
          {t("osinstall.rom.identified", { rom: rom.name })}
        </p>
      )}

      <Field
        label={t("osinstall.destination.label")}
        value={destination}
        empty={t("osinstall.destination.none")}
        onChoose={() => void chooseDestination()}
        choose={t("common.browse")}
        hint={t("osinstall.destination.hint")}
        testId="osinstall-destination-field"
        describedBy={taken ? "osinstall-destination-taken" : tree ? (tree.isTree ? "osinstall-destination-tree" : "osinstall-destination-fresh") : undefined}
      />
      {/* Two sentences, never one: the refusal is what `apply` would say;
          the tree line is what is there. Both can be true of one folder,
          and until round 4 decides update mode the refusal is the last word. */}
      {taken && (
        <p id="osinstall-destination-taken" data-testid="osinstall-destination-taken" className="badge badge-err" style={{ fontSize: 11, margin: "0 0 6px", display: "inline-block" }}>
          {t("osinstall.destination.taken")}
        </p>
      )}
      {tree?.isTree && (
        <p id="osinstall-destination-tree" data-testid="osinstall-destination-tree" className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
          {t("osinstall.destination.tree", { release: tree.release ?? "", count: tree.files })}
        </p>
      )}
      {destination && !taken && tree && !tree.isTree && (
        <p id="osinstall-destination-fresh" data-testid="osinstall-destination-fresh" className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
          {t("osinstall.destination.fresh")}
        </p>
      )}
    </section>
  );
}
```

Check `Field`'s `describedBy` is applied to its Browse button (read `Field.tsx`) — the ART-241 tests depend on it. If `Field` does not expose `testId` on the outer element, read how `osinstall-rom-field` was found in `OsInstall.test.tsx` and match.

Catalogue keys, both files: `osinstall.destination.taken` — en *"This folder already has something in it. ART writes only into a new or empty folder — choose another, or empty this one."*, tr *"Bu klasörde zaten bir şey var. ART yalnızca yeni ya da boş bir klasöre yazar — başka bir klasör seçin ya da bunu boşaltın."*; `osinstall.destination.tree` — en *"This folder holds a tree ART built: {{release}}, {{count}} files."*, tr *"Bu klasörde ART'ın kurduğu bir ağaç var: {{release}}, {{count}} dosya."*; `osinstall.destination.fresh` — en *"Empty folder: a new tree will be written here."*, tr *"Boş klasör: buraya yeni bir ağaç yazılır."* If `osinstall.destination.taken` already exists under a different name in the refusal catalogue (grep `destinationTaken` / `already has something` in `en.json`), reuse that key and say so.

- [ ] **Step 4: `StepMakine` mounts it; the fields leave `OsInstall`**

`steps.tsx`:

```tsx
/** Tab 3 — Kickstart and destination. The keymap joins in round 4 with the plan. */
export function StepMakine() {
  const { t } = useTranslation();
  return (
    <>
      <MachineTab />
      <p className="faint" style={{ fontSize: 11, margin: "0 0 16px" }} data-testid="tab-makine-keymap-note">
        {t("osBuilder.tab.makineKeymapNote")} <Link to="/os-builder/dosyalar">{t("osBuilder.step.dosyalar")}</Link>
      </p>
    </>
  );
}
```

`osBuilder.tab.makineKeymapNote` — en *"The keyboard layout is still chosen on the files tab, under the plan:"*, tr *"Klavye düzeni şimdilik Amiga dosyaları sekmesinde, planın altında seçiliyor:"*. Delete `osBuilder.tab.makineNotYet` from both catalogues; `NotYet` keeps serving `derle` (make its `id` type `"derle"` only).

In `OsInstall.tsx` delete: the ROM `Field` block through the `rom &&` paragraph (1919-1981), the destination `Field` (1983-1990), `chooseRom`, `chooseRomIn`, `chooseDestination`, the `amigaForeverRom` / `amigaForeverRomDismissed` state and `amigaForeverRomOffer` (keep `amigaForeverAdf` / `amigaForeverDismissed` for the disks offer; the `hostAmigaForeverFolders` effect keeps setting only `adf`), `setRomPath` if now unused, the `open` import if now unused. **Keep** `romPath`, `rom`, `romError` (the plan and the conditional reason read them), `destination`/`setDestination` (the plan and `hands a finished install's destination to the session` read/write it), `destinationTaken`.

`OsInstall.test.tsx`: delete the six moved cases (the three in *Amiga Forever…* about the ROM folder and the ART-241 describe; leave the disks-offer cases); in *mounts with the real media, ROM, destination…* rename to *mounts with the real media, checklist and action controls* and delete the two `rom.label` / `destination.label` expectations; adjust its checkbox count comment if the ROM/destination removal changes nothing there (it should not — they were `Field`s, not checkboxes). Any other case that clicked the ROM or destination Browse button on this screen is listed in the report with its fate.

- [ ] **Step 5: Green**

```powershell
pnpm vitest run src/components/osbuilder/MachineTab.test.tsx src/components/osbuilder/OsInstall.test.tsx src/pages/osbuilder/steps.test.tsx
pnpm vitest run -t "have identical key sets"
pnpm vitest run src/i18n/literal-keys.test.ts
pnpm lint
```
Expected: MachineTab 11 passed; OsInstall 105 − 6 = 99 (or fewer if other cases had to go — every one named); steps green (its `makine` case now finds `machine-tab`? — update the `tab-makine` completeness expectation to `machine-tab` and the *makine names the files tab* case to assert `tab-makine-keymap-note` instead); parity, literal-keys, lint clean.

- [ ] **Step 6: Commit**

`<scratchpad>\msg-r2-t2.txt`: *"Tab 3: MachineTab — the Kickstart and the destination, with their sentences. Six OsInstall cases move by name; the destination gains its occupied / tree / fresh sentences (four-tab design § 3.3). The keymap stays on tab 1 until round 4 brings the plan."*

---

### Task 3: Tab 1 — the identity wall folded

**Files:**
- Modify: `src/components/osbuilder/OsInstall.tsx` (the `media-identity` block and the reuse/rescan controls, ~1829-1916 after Task 2)
- Modify: `src/components/osbuilder/OsInstall.test.tsx`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json` (`osinstall.mediaId.foldSummary`)

- [ ] **Step 1: The tests**

Append to `OsInstall.test.tsx` inside (or after) the *identifying install media by content hash* describe:

```tsx
describe("the identity wall is folded (four-tab design § 3.1)", () => {
  it("draws one closed line whose count is the pass's own, and keeps the lines inside", async () => {
    await renderFull();
    const fold = await screen.findByTestId("media-identity-fold");
    expect((fold as HTMLDetailsElement).open).toBe(false);
    const lines = within(fold).queryAllByTestId(/^media-identity-(confirmed|unconfirmed|not-in-table|unreadable|skipped)$/);
    expect(within(fold).getByRole("group").textContent ?? fold.textContent).toBeTruthy();
    expect(fold.querySelector("summary")!.textContent).toBe(
      i18n.t("osinstall.mediaId.foldSummary", { count: lines.length })
    );
    expect(within(fold).getByTestId("media-identity")).toBeTruthy();
  });

  it("keeps the reuse toggle and Scan again inside the fold, still working", async () => {
    await renderFull();
    const fold = await screen.findByTestId("media-identity-fold");
    const toggle = within(fold).getByRole("checkbox", { name: i18n.t("osinstall.media.reuseScan") });
    expect(toggle).toBeTruthy();
    await userEvent.click(within(fold).getByRole("button", { name: i18n.t("osinstall.media.rescan") }));
    await waitFor(() => expect(rescanMock).toHaveBeenCalled());
  });

  it("still runs the identification pass while the fold is closed", async () => {
    await renderFull();
    await screen.findByTestId("media-identity-fold");
    await waitFor(() => expect(identifyMediaMock).toHaveBeenCalled());
  });

  it("draws no fold before any folder is chosen", () => {
    seedRemembered({});
    render(<OsInstall />);
    expect(screen.queryByTestId("media-identity-fold")).toBeNull();
  });
});
```

(`rescanMock` and `identifyMediaMock` are the file's existing mocks. Drop the `getByRole("group")` line if `<details>` has no such role in jsdom; the summary assertion is the guard.) Existing cases that find `media-identity-*` lines or the reuse checkbox keep passing: a closed `<details>` still has its children in the DOM under jsdom; if a case relied on visibility, open the fold in that case with `userEvent.click(summary)`.

- [ ] **Step 2: Run, fail on the missing testid**

- [ ] **Step 3: The fold**

Wrap the `media-identity` div, the reuse/rescan row, the reuse help line and the `rescanned` line in:

```tsx
        {/*
          **The identity wall, folded** (four-tab design § 3.1). One line per
          file over every folder — 45 on the owner's three, 23 of them game
          discs — was half the screen's noise. The pass still runs (the
          readout's rank-2 and rank-3 rows read the cache it fills, F2); only
          the lines are behind a closed line whose count is the pass's own.
          The reuse toggle and Scan again are about that pass, so they live
          inside it.
        */}
        {mediaIdentity.kind !== "not-asked" && (
          <details data-testid="media-identity-fold" style={{ margin: "0 0 12px" }}>
            <summary className="muted" style={{ fontSize: 12, cursor: "pointer" }}>
              {t("osinstall.mediaId.foldSummary", { count: identityLines.length })}
            </summary>
            {/* …the existing media-identity div, unchanged… */}
            {/* …the existing reuse/rescan row, help line and rescanned line, unchanged… */}
          </details>
        )}
```

`osinstall.mediaId.foldSummary` — en `"Details: what {{count}} files in these folders are, by content"` with `_one`/`_other` forms if the catalogue uses them for counts (check `osinstall.media.found_one` — it does; write `foldSummary_one` / `foldSummary_other`), tr `"Ayrıntılar: bu klasörlerdeki {{count}} dosya içeriğine göre ne"` (Turkish has one form; give both keys the same text for parity). Check the guard `mediaIdentity.kind !== "not-asked"` against the state's kinds (`OsInstall.tsx:~967`) and use the kind that means "a pass has run or is running".

- [ ] **Step 4: Green, parity, lint, the suite**

```powershell
pnpm vitest run src/components/osbuilder/OsInstall.test.tsx
pnpm vitest run -t "have identical key sets"
pnpm lint
pnpm test
```

- [ ] **Step 5: Commit** — *"Tab 1: the identity wall behind one closed line (four-tab design § 3.1); the pass still runs."*

---

### Task 4: Mutations, docs, the build

**Files:** `.superpowers/sdd/2026-09-09-four-tabs/round-2-report.md`; `docs/STATUS.md`, `docs/session-log.md`, `docs/FEATURES.md`, `CHANGELOG.md`.

- [ ] **Step 1: Mutations** (script as round 1's, `shutil.copyfile` restore):

| # | File | Mutation | Guard |
|---|---|---|---|
| M1 | `useRomIdentity.ts` | in `.catch`, `setUnreadable(true)` → `setUnreadable(false)` | `says unreadable when the core refuses, never both` |
| M2 | `useDestinationCheck.ts` | in the taken `.catch`, `setTaken(false)` → `setTaken(true)` | `does not declare a folder taken when ART could not look` |
| M3 | `MachineTab.tsx` | render the `fresh` line without `!taken` | `refuses an occupied folder in its own sentence, and names a tree ART built apart from it` |
| M4 | `MachineTab.tsx` | `describedBy` → `undefined` | `names the identified paragraph once the ROM resolves` |
| M5 | `OsInstall.tsx` | `<details … open>` | `draws one closed line…` |
| M6 | `OsInstall.tsx` | skip `osinstallIdentifyMedia` when the fold is closed (guard the effect on a `false`) | `still runs the identification pass while the fold is closed` |

- [ ] **Step 2: Verification, unpiped** — `pnpm lint`, `pnpm test`, parity, `control-byte-sweep.py`, `contrast-check.py --quiet`, `git diff --stat main -- src-tauri` (empty).

- [ ] **Step 3: The report** — rulings (from the header), the by-name fate list for every case that left `OsInstall.test.tsx`, the mutation table with tails, quoted verification, and *owed by a person*: tab 3 on the release build (Kickstart, destination with an empty folder, an occupied folder, an ART tree), tab 1 at 100 % and 200 % with the fold closed and open.

- [ ] **Step 4: Docs** — STATUS "Start here" item 0 **in place** (round 2 landed; the keymap ruling; what a person drives); session-log row with the suite line; FEATURES: the *OS install screen* row (line ~312) appends *"**Round 2 (2026-09-09):** the Kickstart and destination fields are tab 3 (`MachineTab.tsx`, guards `names the identified paragraph once the ROM resolves`, `refuses an occupied folder in its own sentence…`); the per-file identity lines are behind one closed line on tab 1 (`draws one closed line whose count is the pass's own`). The keymap follows in round 4."*; CHANGELOG `[Unreleased] → Changed`: *"- OS Builder: the Kickstart and the destination folder have their own tab; the per-file 'what these files are' list is folded behind one line on the files tab."*

- [ ] **Step 5: Commit the docs; build; copy** — `pnpm tauri build` (unpiped), then copy the NSIS installer to `E:\amiga\ProjeART\build\Amiga Retro Toolkit_0.9.1_x64-setup_four-tabs-r2.exe`.

---

## Self-review

**Spec coverage:** § 3.1 readout first (round A, kept) + the wall folded + the ROM offer leaving tab 1 — Tasks 2-3; § 3.3 Kickstart and destination with the three sentences — Task 2; the keymap — ruled to round 4, disclosed; § 8 row 2 — all; § 10.5 (the pass keeps running) — Task 3 test + M6.

**Placeholders:** none; the two "check and copy" instructions (the `Field` testid, the mock block) name the exact file and lines to copy from.

**Type consistency:** `useRomIdentity → { rom, unreadable }` used as `{ rom, unreadable: romError }` in both consumers; `useDestinationCheck → { taken, tree }` used as `{ taken: destinationTaken }` in `OsInstall` and `{ taken, tree }` in `MachineTab`; `TreeSummary` fields `isTree, release, files` as in `osinstall.ts:2228`; testids consistent between Task 2's tests and JSX and Task 4's mutations.
