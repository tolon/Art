# OS Builder Flow — Wave 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** make the OS Builder carry its own output forward — one session value for the AmigaOS folder ART builds, read by every step that needs it — and put the builder's panels on real sub-routes with a progress strip above them.

**Architecture:** a typed **facade over the settings store**, not a second store. `src/lib/buildSession.ts` declares the session shape, its persisted keys and the one-time seeding from the legacy keys; `src/lib/useBuildSession.ts` reads and writes it through the existing `useRememberedShape` machinery, so the late-landing-load guard (ART-089), the identity guard (ART-178/ART-195) and the per-guard fallback all come for free. `src/lib/buildSteps.ts` is pure TypeScript that answers which steps a build kind has and whether a step can act yet. `OsBuilder.tsx` becomes a wizard shell — progress strip plus `<Outlet/>` — and the panels that used to be stacked in one scrolling column become sub-routes.

**Tech Stack:** React 18 + React Router (`HashRouter`), Zustand (via the existing `settingsStore`), react-i18next, Vitest + Testing Library (jsdom). **No Rust in this wave.**

**Spec:** [docs/superpowers/specs/2026-08-21-os-builder-flow-design.md](../specs/2026-08-21-os-builder-flow-design.md)

**Closes:** [ART-197](../../ISSUES.md) · [ART-198](../../ISSUES.md)

---

## Global Constraints

- **Every user-visible string goes into both `src/i18n/en.json` and `src/i18n/tr.json`, in the same commit.** `pnpm test` fails the build if the key sets differ, a value is empty, or an interpolation variable in one is missing from the other.
- **`src/lib/*` never renders a string.** A helper that builds a message returns a `Phrase { key, params? }`; the component calls `t(phrase.key, phrase.params)`. `src/lib/buildSteps.ts` returns i18n **keys**, never text.
- **Nothing changes unless the user changes it.** A remembered value that is *moved* must arrive at its new home carrying the user's old value. A user upgrading into this build must not find one field emptied.
- **The identity guard stays.** Every read goes through `useRemembered`/`useRememberedShape`, which stabilise with `sameRemembered`. A fresh object identity per render turns the OS Builder's effects into a loop — that was ART-195, measured at 2,149 preview jobs in one session.
- **ART never writes a physical card. The output is an image file.** No task in this wave may add a device path, a drive letter picker, or a raw write.
- **The Amiga-side install stays optional.** It is a step the user may skip, never a gate.
- **Frontend tests sit next to the source.** `src/lib/x.test.ts` runs in `node`; a test that needs a DOM must be named `*.test.tsx` — jsdom applies only to `src/**/*.test.tsx` (`vite.config.ts`), and a component test written as `.ts` gets no DOM and fails confusingly.
- **A test is not a guard until the defect has been put back and seen to fail it.** Tasks 2, 4, 5 and 7 each name their mutation. Run it, watch it fail, put it back.
- **`pnpm lint` is `tsc --noEmit` twice** — the app, then `tsconfig.test.json`. Test-file type errors appear only in the second run.
- **Unused imports and variables are errors.** Delete what you replace; do not leave the old state declaration behind.
- Routes live in `src/App.tsx`; `builtin.rs::route` values must remain real routes (`every_workflow_route_is_a_real_app_route`). `route::OS_BUILDER` is `/os-builder` and **stays** a rendering route in this wave — do not change it to a bare redirect target without re-reading that test.

---

## What was measured before this plan was written

Under CLAUDE.md's "Research before design" rule, everything below was counted in the sources on 2026-08-21, not recalled. Two of the numbers disagree with the spec, and the disagreement changes the work.

### The spec says 22 remembered keys. There are 36.

Extracted by matching `useRemembered` / `useRememberedShape` call sites across `src/pages/OsBuilder.tsx` and `src/components/osbuilder/*.tsx` (excluding tests):

| File | Count | Keys |
|---|---|---|
| `src/pages/OsBuilder.tsx` | 5 | `osBuilder.kind` (:66) · `osBuilder.profile` (:94) · `osBuilder.cardGb` (:99) · `osBuilder.imagePath` (:104) · `osBuilder.romPath` (:109) |
| `AmigaInstallPanel.tsx` | 5 | `amigaInstall.package` (:114) · `.archive` (:119) · `.overlayArchive` (:124) · `.kickstart` (:129) · `.medium` (:137) |
| `CardBuilder.tsx` | 14 | `cardBuilder.archive` (:88) · `.kickstart` (:93) · `.dest` (:98) · `.cardGb` (:103) · `.label` (:108) · `.bootMib` (:111) · `.driveName` (:116) · `.partitionMb` (:121) · `.fsType` (:126) · `.advanced` (:150) · `pistorm.hardware` (:133) · `pistorm.firmware` (:138) · `pistorm.options` (:143) · `pistorm.line` (:148) |
| `OsInstall.tsx` | 10 | `osinstall.mediaFolder` (:217) · `.rom` (:239) · `.destination` (:240) · `.release` (:252) · `.chosen` (:265, **per release**) · `.excludedConditional` (:277, **per release**) · `.reuseScan` (:294) · `.packages.treeRoot` (:327) · `.packages.folder` (:332) · `.packages.chosen` (:337) |
| `VolumePreload.tsx` | 2 | `preload.image` (:65) · `preload.driver` (:70) |
| | **36** | |

**Why it matters:** "migrate the 22 keys" taken literally would leave fourteen behind. This plan instead states, per key, whether the session **takes it over** (and therefore must migrate it) or **leaves it exactly where it is** (and therefore needs no migration and cannot lose anything). See Task 2's table.

### Two of those keys are not literal keys

`osinstall.chosen` and `osinstall.excludedConditional` are built by
`rememberedComponentKey(base, release)` (`src/lib/osinstall.ts:1006`):

```ts
const RELEASE_BEFORE_THE_PICKER = "AmigaOS 3.2";
export function rememberedComponentKey(base: string, release: string): string {
  return release === RELEASE_BEFORE_THE_PICKER ? base : `${base}.${release}`;
}
```

So the real stored keys are `osinstall.chosen`, `osinstall.chosen.AmigaOS 3.9`, and one more per release added later. **A migration that reads a fixed key list silently drops every non-3.2 selection.** The session must re-read these two whenever the release changes, or switching release and switching back stops finding the earlier ticks — behaviour `OsInstall.tsx:255-263` documents as deliberate and a test already covers.

### The panels already accept the tree as a prop

`PackagePanel` (`:147`) and `AmigaInstallPanel` (`:105`) both take `treeRoot` and `onTreeRootChange`. `OsInstall.tsx:1359-1379` passes its own `packagesTreeRoot` state into both. **Nothing has to be rewritten to close ART-197** — the prop has to be fed from the session instead of from a second remembered key.

### The code carries an objection to the fix

`OsInstall.tsx:317-326` says, of those three package keys:

> Deliberately **not** auto-filled from a just-finished install the way `verifyDistRoot` below is … overwriting a remembered pick the moment a build finishes is exactly the "something changed without the user changing it" shape CLAUDE.md's own rule forbids.

The spec overrides this deliberately: step 3 writes `tree.root` when the build finishes. **The objection is answered rather than ignored, and Task 5 is where that happens** — pressing Build *is* the user changing it (they chose that folder and asked ART to fill it), and the change is **stated on screen** rather than made silently. Task 5 adds that sentence. Do not delete the comment; replace it with what is now true.

---

## Deviations from the spec, and why

Three. Each is a judgement call this plan makes explicitly so the owner can reject it before any code is written.

**1. One facade over `settingsStore`, not a second Zustand store.**
The spec says "`src/stores/buildSession.ts`, one Zustand store". A separate store would hold a *copy* of values that also live in `settings.json`, and would have to re-solve three bugs the settings store has already solved: a load landing after the user acted (ART-089, solved by `changedByUser`), a fresh identity per render driving effects (ART-178/ART-195, solved by `sameRemembered`), and a bad persisted value reaching the screen (solved by the guards). This plan builds the session as a **typed facade** — `src/lib/buildSession.ts` (pure) plus `src/lib/useBuildSession.ts` (the hook) — over the store that already exists. The spec's stated goal, *"there is one variable, not two"*, is met; only the file it lives in differs.

**2. Wave 1's session type carries only the fields wave 1 wires.**
The spec's `BuildSession` lists `amigaInstall`, `card` and `output`. Those panels keep their own remembered keys until wave 2 rehomes them, so declaring the fields now would add state nothing reads or writes. The type gains them in wave 2, in the task that wires them.

**3. Six routes in this wave, not eight.**
`bilesenler` (components) and `ozet` (summary) have no component to mount: components live inside `OsInstall.tsx`, which wave 2 splits, and the summary is wave 3's own scope. Declaring two routes that render nothing would be the "Coming Later" pattern applied to a place it does not fit — §96 is about *offered actions*, not about empty pages. `src/lib/buildSteps.ts` therefore returns six steps in this wave and the strip shows six.

---

## File structure

| File | Status | Responsibility |
|---|---|---|
| `src/i18n/en.json`, `src/i18n/tr.json` | modify | ART-198's sentence; the six step labels; the carry sentence |
| `src/lib/buildSession.ts` | **create** | The session shape, its persisted keys, the guards, and `seedTreeRoot` — the one-time legacy fallback. Pure; no React, no i18next. |
| `src/lib/buildSession.test.ts` | **create** | node |
| `src/lib/useBuildSession.ts` | **create** | The hook: reads/writes each section through `useRememberedShape`, re-reads the per-release component keys when the release changes |
| `src/lib/buildSteps.ts` | **create** | `StepId`, `stepsFor(kind)`, `readiness(session, step)`, `stepLabelKey(step)`. Pure; returns keys. |
| `src/lib/buildSteps.test.ts` | **create** | node |
| `src/pages/OsBuilder.tsx` | modify | Becomes the wizard shell: kind picker at `hedef`, progress strip, `<Outlet/>` |
| `src/pages/osbuilder/steps.tsx` | **create** | The six step elements — thin wrappers that mount an existing panel and feed it from the session |
| `src/App.tsx` | modify | Six child routes under `os-builder` |
| `src/components/osbuilder/OsInstall.tsx` | modify | Stops owning `packages.treeRoot`; stops rendering the two panels; writes `tree.root` on a finished build and says so |
| `src/components/osbuilder/OsInstall.test.tsx` | modify | The carry test and its mutation |
| `src/pages/osbuilder/steps.test.tsx` | **create** | jsdom: a step opened cold asks; the strip renders |
| `docs/ISSUES.md`, `docs/STATUS.md`, `docs/FEATURES.md`, `CHANGELOG.md` | modify | Task 8 |

---

## Task 1: ART-198 — the sentence that offers an unofficial pack as an example of an official one

**Files:**
- Modify: `src/i18n/en.json` (key `osinstall.packages.intro`)
- Modify: `src/i18n/tr.json` (same key)
- Test: `src/i18n/copy.test.ts` (create)

**Interfaces:**
- Consumes: nothing.
- Produces: nothing. Independent of every other task; do it first because it is one line and it is already filed.

Current values, read from the catalogues on 2026-08-21:

```
EN: Add an official update — a BoingBag, or an unofficial pack like the Turkish
    catalogs — onto a distribution tree ART, or you, already built.
TR: ART'ın ya da senin zaten kurduğun bir dağıtım ağacına resmi bir güncelleme —
    bir BoingBag, ya da Türkçe katalog paketi gibi resmi olmayan bir paket — ekle.
```

Two defects in one sentence: the em-dash pair reads as an appositive to "an official update", so an **unofficial** pack is offered as an example of an **official** one; and the sentence glosses "BoingBag", which the owner ruled needs no glossing (*"BoingBag'ı bütün Amiga camiası bilir"*).

- [ ] **Step 1: Write the failing test**

Create `src/i18n/copy.test.ts`:

```ts
// Sentences whose *wording* is the defect, kept honest by a test.
//
// The parity test next door proves both catalogues carry the same keys. It
// cannot prove a sentence is true: ART-198 passed parity for months because
// both catalogues were equally wrong. These assertions name the specific
// contradiction each string used to hold, so putting the old sentence back
// fails the run.
import { describe, expect, it } from "vitest";

import en from "./en.json";
import tr from "./tr.json";

describe("osinstall.packages.intro (ART-198)", () => {
  it("does not offer an unofficial pack as an example of an official update", () => {
    const english = en.osinstall.packages.intro.toLowerCase();
    // The defect was both words in one sentence: "an official update — … an
    // unofficial pack …". Either word alone is fine; the pair is the bug.
    const promisesOfficial = english.includes("official update");
    const offersUnofficial = english.includes("unofficial");
    expect(promisesOfficial && offersUnofficial).toBe(false);
  });

  it("does not carry the same contradiction in Turkish", () => {
    const turkish = tr.osinstall.packages.intro.toLowerCase();
    const promisesOfficial = turkish.includes("resmi bir güncelleme");
    const offersUnofficial = turkish.includes("resmi olmayan");
    expect(promisesOfficial && offersUnofficial).toBe(false);
  });

  it("names a BoingBag without explaining what one is", () => {
    // The owner's ruling: the name is known across the Amiga community. It is
    // used, not glossed — so the sentence must not introduce it with a
    // "like …" example list.
    expect(en.osinstall.packages.intro).toContain("BoingBag");
    expect(tr.osinstall.packages.intro).toContain("BoingBag");
    expect(en.osinstall.packages.intro.toLowerCase()).not.toContain("like the turkish");
    expect(tr.osinstall.packages.intro.toLowerCase()).not.toContain("türkçe katalog paketi gibi");
  });
});
```

- [ ] **Step 2: Run it and watch it fail**

Run: `pnpm vitest run src/i18n/copy.test.ts`
Expected: FAIL — all three, against the sentences as they stand today.

- [ ] **Step 3: Correct both catalogues**

`src/i18n/en.json`, key `osinstall.packages.intro`:

```
Add a BoingBag or another update pack onto a distribution tree ART, or you, already built.
```

`src/i18n/tr.json`, same key:

```
ART'ın ya da senin zaten kurduğun bir dağıtım ağacına bir BoingBag ya da başka bir güncelleme paketi ekle.
```

Both drop the official/unofficial pair and the gloss. Neither takes an interpolation variable, so parity is unaffected. **"distribution tree" / "dağıtım ağacı" stays for now** — renaming it is wave 3's scope and doing it here would leave two names in circulation for one thing.

- [ ] **Step 4: Run the tests**

Run: `pnpm vitest run src/i18n/`
Expected: PASS — `copy.test.ts`, the parity test and the phrase-key test together.

- [ ] **Step 5: Commit**

```bash
git add src/i18n/en.json src/i18n/tr.json src/i18n/copy.test.ts
git commit -m "fix(i18n): a BoingBag is an update, not an example of an official one (ART-198)"
```

---

## Task 2: The session — shape, keys, and the seeding that loses nothing

**Files:**
- Create: `src/lib/buildSession.ts`
- Test: `src/lib/buildSession.test.ts`

**Interfaces:**
- Consumes: `InstallRelease` and `rememberedComponentKey` from `@/lib/osinstall`; the guards from `@/lib/remembered`.
- Produces:
  - `type BuildKind = "distro" | "boot-card" | "install" | "prepare-volumes"`
  - `interface BuildSession { kind; media; rom; release; tree; components; packages }` — exact fields below
  - `const SESSION_KEYS` — the persisted key names
  - `seedTreeRoot(bag: unknown): string | null`
  - `seededComponents(bag: unknown, release: InstallRelease): { chosen: string[]; excludedConditional: string[] }`
  - `isBuildKind`, `isTreeShape` and the other guards Task 4 hands to `useRememberedShape`

### What the session takes over, and what it leaves alone

Of the 36 keys measured above, this wave takes over **eleven** — the ten in `OsInstall.tsx` plus `osBuilder.kind`. The other twenty-five stay exactly where they are: untouched, still read and written by their own panels, so there is nothing for a migration to lose.

| Legacy key | Wave 1 | New home |
|---|---|---|
| `osBuilder.kind` | **taken over** | `buildSession.kind` |
| `osinstall.mediaFolder` | **taken over** | `buildSession.media.folder` |
| `osinstall.reuseScan` | **taken over** | `buildSession.media.reuseScan` |
| `osinstall.rom` | **taken over** | `buildSession.rom.path` |
| `osinstall.release` | **taken over** | `buildSession.release` |
| `osinstall.destination` | **taken over** | `buildSession.tree.root` (fallback source) |
| `osinstall.packages.treeRoot` | **taken over** | `buildSession.tree.root` (preferred source) |
| `osinstall.chosen[.release]` | **taken over** | `buildSession.components.<release>` |
| `osinstall.excludedConditional[.release]` | **taken over** | `buildSession.components.<release>` |
| `osinstall.packages.folder` | **taken over** | `buildSession.packages.folder` |
| `osinstall.packages.chosen` | **taken over** | `buildSession.packages.chosen` |
| the other 25 | **left in place** | unchanged — wave 2 |

**The legacy keys are read, never written again, and never deleted.** Deleting them would make a rollback to 0.8.5 lose the user's paths; leaving them costs a few hundred bytes in `settings.json` and makes the upgrade reversible. A new key wins when it exists, so the fallback fires exactly once.

### `tree.root`: which of the two old keys wins

`osinstall.packages.treeRoot` first, `osinstall.destination` second. The packages and Amiga-install steps were reading `packages.treeRoot`; keeping their pointer where the user last put it is what "nothing changes unless the user changes it" requires. `destination` seeds the session only for a user who never picked a tree by hand — which is the user ART-197 is about.

- [ ] **Step 1: Write the failing test**

Create `src/lib/buildSession.test.ts`:

```ts
import { describe, expect, it } from "vitest";

import { seedTreeRoot, seededComponents, SESSION_KEYS } from "./buildSession";

describe("seedTreeRoot", () => {
  it("prefers the session's own key once it exists", () => {
    const bag = {
      [SESSION_KEYS.tree]: { root: "E:\\new", builtHere: false },
      "osinstall.packages.treeRoot": "E:\\old-packages",
      "osinstall.destination": "E:\\old-destination",
    };
    expect(seedTreeRoot(bag)).toBe("E:\\new");
  });

  it("falls back to the tree the packages step was pointing at", () => {
    const bag = {
      "osinstall.packages.treeRoot": "E:\\old-packages",
      "osinstall.destination": "E:\\old-destination",
    };
    expect(seedTreeRoot(bag)).toBe("E:\\old-packages");
  });

  it("falls back to the destination for a user who never picked a tree", () => {
    // ART-197's own user: they watched ART write a tree and were then asked
    // to go and find it. The destination is the answer they could not give.
    const bag = { "osinstall.destination": "E:\\amiga\\dist-3.9" };
    expect(seedTreeRoot(bag)).toBe("E:\\amiga\\dist-3.9");
  });

  it("answers null when there is nothing to seed from", () => {
    expect(seedTreeRoot({})).toBeNull();
  });

  it("rejects a non-string that a hand-edited settings file could hold", () => {
    expect(seedTreeRoot({ "osinstall.destination": 42 })).toBeNull();
  });
});

describe("seededComponents", () => {
  it("reads the unsuffixed key for the release that predates the picker", () => {
    const bag = {
      "osinstall.chosen": ["workbench-base", "extras"],
      "osinstall.excludedConditional": ["modules-a1200"],
    };
    expect(seededComponents(bag, "AmigaOS 3.2")).toEqual({
      chosen: ["workbench-base", "extras"],
      excludedConditional: ["modules-a1200"],
    });
  });

  it("reads the per-release key for every other release", () => {
    // The migration defect this test exists for: a fixed key list reads
    // `osinstall.chosen` and silently drops every 3.9 tick.
    const bag = {
      "osinstall.chosen": ["workbench-base"],
      "osinstall.chosen.AmigaOS 3.9": ["os39-base"],
    };
    expect(seededComponents(bag, "AmigaOS 3.9").chosen).toEqual(["os39-base"]);
  });

  it("keeps two releases apart rather than merging them", () => {
    const bag = {
      "osinstall.chosen": ["workbench-base"],
      "osinstall.chosen.AmigaOS 3.9": ["os39-base"],
    };
    expect(seededComponents(bag, "AmigaOS 3.2").chosen).toEqual(["workbench-base"]);
  });

  it("answers empty lists rather than throwing on a bad value", () => {
    const bag = { "osinstall.chosen": "not a list" };
    expect(seededComponents(bag, "AmigaOS 3.2")).toEqual({
      chosen: [],
      excludedConditional: [],
    });
  });
});
```

- [ ] **Step 2: Run it and watch it fail**

Run: `pnpm vitest run src/lib/buildSession.test.ts`
Expected: FAIL — `Failed to resolve import "./buildSession"`.

- [ ] **Step 3: Write the module**

Create `src/lib/buildSession.ts`:

```ts
// The OS Builder's session: the values one build carries from step to step.
//
// **The defect this exists for (ART-197).** ART wrote a distribution tree and
// remembered where under `osinstall.destination`; the two panels directly
// beneath it read the tree they operate on from `osinstall.packages.treeRoot`.
// Nothing joined them, so a user who had just watched ART write 1915 files
// into a folder was asked, immediately below, to locate a "distribution tree".
// The owner — who has read every document in this repository — could not
// answer the field.
//
// The fix is structural rather than a wire: there is now **one** `tree.root`,
// so the carry holds because there is nothing to forget. Hand-wiring
// `destination → treeRoot` would have left two variables and a habit.
//
// **A facade, not a second store.** Everything here persists through
// `settings.remembered`, the same bag `useRemembered` uses, because that bag
// already answers three problems a parallel store would have to answer again:
// a settings load landing after the user acted (ART-089), a fresh object
// identity per render turning an effect into a loop (ART-178/ART-195), and a
// hand-edited value reaching the screen (the guards). This module is pure —
// the React half is `@/lib/useBuildSession`.
//
// **Legacy keys are read once and then left alone.** They are never written
// again and never deleted: a user who rolls back to 0.8.5 finds their paths
// where that version looks for them. A session key wins whenever it exists,
// so each fallback fires exactly once.

import { rememberedComponentKey, type InstallRelease } from "@/lib/osinstall";
import { isFlag, isText, isTextList, isTextOrNothing, type Guard } from "@/lib/remembered";

/** What the screen is being asked for. Unchanged from `OsBuilder.tsx`. */
export type BuildKind = "distro" | "boot-card" | "install" | "prepare-volumes";

/**
 * The AmigaOS folder this build is about.
 *
 * `builtHere` records that ART wrote this tree in this session rather than the
 * user pointing at one — the summary step (wave 3) says different sentences
 * for the two, and a step that offers to overwrite needs to know which it has.
 */
export interface TreeChoice {
  root: string | null;
  builtHere: boolean;
}

export interface MediaChoice {
  folder: string | null;
  reuseScan: boolean;
}

export interface ComponentChoice {
  chosen: string[];
  excludedConditional: string[];
}

export interface PackageChoice {
  folder: string | null;
  chosen: string[];
}

/**
 * One build, as the steps see it.
 *
 * Wave 1 carries what wave 1 wires. `amigaInstall`, `card` and `output` join
 * it in wave 2, in the task that rehomes those panels — declaring them now
 * would add fields nothing reads.
 */
export interface BuildSession {
  kind: BuildKind;
  media: MediaChoice;
  rom: { path: string | null };
  release: InstallRelease;
  tree: TreeChoice;
  components: ComponentChoice;
  packages: PackageChoice;
}

/** Where each section persists inside `settings.remembered`. */
export const SESSION_KEYS = {
  kind: "buildSession.kind",
  media: "buildSession.media",
  rom: "buildSession.rom",
  release: "buildSession.release",
  tree: "buildSession.tree",
  packages: "buildSession.packages",
  /** Per release, for the reason `rememberedComponentKey` exists. */
  components: (release: InstallRelease): string => `buildSession.components.${release}`,
} as const;

/** The legacy keys this wave reads from, once. Listed so the fallback is
 *  auditable rather than scattered through the file. */
export const LEGACY_KEYS = {
  kind: "osBuilder.kind",
  mediaFolder: "osinstall.mediaFolder",
  reuseScan: "osinstall.reuseScan",
  rom: "osinstall.rom",
  release: "osinstall.release",
  destination: "osinstall.destination",
  packagesTreeRoot: "osinstall.packages.treeRoot",
  packagesFolder: "osinstall.packages.folder",
  packagesChosen: "osinstall.packages.chosen",
  chosen: "osinstall.chosen",
  excludedConditional: "osinstall.excludedConditional",
} as const;

export function isBuildKind(value: unknown): value is BuildKind {
  return (
    value === "distro" ||
    value === "boot-card" ||
    value === "install" ||
    value === "prepare-volumes"
  );
}

/** The shape guards `useRememberedShape` needs, one per section.
 *  `isTextOrNothing` is `@/lib/remembered`'s own guard — do not declare a
 *  second one here, or two names end up meaning the same check. */
export const TREE_SPEC: { [K in keyof TreeChoice]: Guard<TreeChoice[K]> } = {
  root: isTextOrNothing,
  builtHere: isFlag,
};

export const MEDIA_SPEC: { [K in keyof MediaChoice]: Guard<MediaChoice[K]> } = {
  folder: isTextOrNothing,
  reuseScan: isFlag,
};

export const COMPONENT_SPEC: { [K in keyof ComponentChoice]: Guard<ComponentChoice[K]> } = {
  chosen: isTextList,
  excludedConditional: isTextList,
};

export const PACKAGE_SPEC: { [K in keyof PackageChoice]: Guard<PackageChoice[K]> } = {
  folder: isTextOrNothing,
  chosen: isTextList,
};

export const DEFAULT_TREE: TreeChoice = { root: null, builtHere: false };
export const DEFAULT_MEDIA: MediaChoice = { folder: null, reuseScan: true };
export const DEFAULT_COMPONENTS: ComponentChoice = { chosen: [], excludedConditional: [] };
export const DEFAULT_PACKAGES: PackageChoice = { folder: null, chosen: [] };

function bagOf(store: unknown): Record<string, unknown> {
  return typeof store === "object" && store !== null && !Array.isArray(store)
    ? (store as Record<string, unknown>)
    : {};
}

function textAt(bag: Record<string, unknown>, key: string): string | null {
  const held = bag[key];
  return isText(held) ? held : null;
}

/**
 * The tree this session starts on.
 *
 * Order matters and is the whole migration: the session's own key first, then
 * the folder the packages step was pointing at, then the folder ART last
 * wrote a tree into. The middle one comes before the last because a user who
 * picked a tree by hand must find that pick unchanged; the last one is
 * ART-197's own user, who never picked because they were never told they
 * could not.
 */
export function seedTreeRoot(store: unknown): string | null {
  const bag = bagOf(store);
  const held = bagOf(bag[SESSION_KEYS.tree]);
  if (isText(held.root)) return held.root;
  return textAt(bag, LEGACY_KEYS.packagesTreeRoot) ?? textAt(bag, LEGACY_KEYS.destination);
}

/**
 * The components ticked for one release.
 *
 * Per release, because a component id means something only inside the recipe
 * that declares it — both shipped recipes carry a `workbench-base`, for
 * different media. A migration reading a fixed key list would drop every
 * non-3.2 selection, since `rememberedComponentKey` suffixes all of them.
 */
export function seededComponents(store: unknown, release: InstallRelease): ComponentChoice {
  const bag = bagOf(store);
  const held = bagOf(bag[SESSION_KEYS.components(release)]);
  if (isTextList(held.chosen) || isTextList(held.excludedConditional)) {
    return {
      chosen: isTextList(held.chosen) ? held.chosen : [],
      excludedConditional: isTextList(held.excludedConditional) ? held.excludedConditional : [],
    };
  }
  const chosen = bag[rememberedComponentKey(LEGACY_KEYS.chosen, release)];
  const excluded = bag[rememberedComponentKey(LEGACY_KEYS.excludedConditional, release)];
  return {
    chosen: isTextList(chosen) ? chosen : [],
    excludedConditional: isTextList(excluded) ? excluded : [],
  };
}
```

- [ ] **Step 4: Run the test**

Run: `pnpm vitest run src/lib/buildSession.test.ts`
Expected: PASS, 10 tests.

- [ ] **Step 5: Run the mutation that matters**

**Mutation 2 of the spec's four — "remove the migration".** In `seedTreeRoot`, delete the final line's fallback so it reads `return textAt(bag, LEGACY_KEYS.packagesTreeRoot);`.

Run: `pnpm vitest run src/lib/buildSession.test.ts`
Expected: FAIL — *"falls back to the destination for a user who never picked a tree"*. If it passes, the test is not guarding the migration and must be fixed before going on.

Then in `seededComponents`, replace `rememberedComponentKey(LEGACY_KEYS.chosen, release)` with the bare `LEGACY_KEYS.chosen`.
Expected: FAIL — *"reads the per-release key for every other release"*.

Put both back. Re-run: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/lib/buildSession.ts src/lib/buildSession.test.ts
git commit -m "feat(osbuilder): one build session, seeded from the keys it takes over (ART-197)"
```

---

## Task 3: Which steps a build has, and whether one can act yet

**Files:**
- Create: `src/lib/buildSteps.ts`
- Test: `src/lib/buildSteps.test.ts`

**Interfaces:**
- Consumes: `BuildKind`, `BuildSession` from `@/lib/buildSession`.
- Produces:
  - `type StepId = "hedef" | "kaynak" | "paketler" | "amiga-kurulum" | "kart" | "birimler"`
  - `stepsFor(kind: BuildKind): StepId[]`
  - `type Readiness = "ready" | "asks"`
  - `readiness(session: BuildSession, step: StepId): Readiness`
  - `stepLabelKey(step: StepId): string`

Pure TypeScript, no DOM, no i18next — `stepLabelKey` returns a **key**, per the two-catalogue rule.

- [ ] **Step 1: Write the failing test**

Create `src/lib/buildSteps.test.ts`:

```ts
import { describe, expect, it } from "vitest";

import { DEFAULT_MEDIA, DEFAULT_COMPONENTS, DEFAULT_PACKAGES, type BuildSession } from "./buildSession";
import { readiness, stepLabelKey, stepsFor, STEP_IDS } from "./buildSteps";

function sessionWith(over: Partial<BuildSession> = {}): BuildSession {
  return {
    kind: "install",
    media: DEFAULT_MEDIA,
    rom: { path: null },
    release: "AmigaOS 3.2",
    tree: { root: null, builtHere: false },
    components: DEFAULT_COMPONENTS,
    packages: DEFAULT_PACKAGES,
    ...over,
  };
}

describe("stepsFor", () => {
  it("gives the install job its own steps and not the card's", () => {
    expect(stepsFor("install")).toEqual(["hedef", "kaynak", "paketler", "amiga-kurulum"]);
  });

  it("gives the card job the card step and none of the install's", () => {
    expect(stepsFor("boot-card")).toEqual(["hedef", "kart"]);
  });

  it("gives volume preparation its own", () => {
    expect(stepsFor("prepare-volumes")).toEqual(["hedef", "birimler"]);
  });

  it("leaves the unbuilt distro job at the picker", () => {
    expect(stepsFor("distro")).toEqual(["hedef"]);
  });

  it("always begins at the picker, whatever the kind", () => {
    for (const kind of ["distro", "boot-card", "install", "prepare-volumes"] as const) {
      expect(stepsFor(kind)[0]).toBe("hedef");
    }
  });
});

describe("readiness", () => {
  it("says a packages step with no tree must ask", () => {
    expect(readiness(sessionWith(), "paketler")).toBe("asks");
  });

  it("says a packages step with a tree is ready", () => {
    const s = sessionWith({ tree: { root: "E:\\dist", builtHere: true } });
    expect(readiness(s, "paketler")).toBe("ready");
  });

  it("says the Amiga-side install must ask without a tree, and is ready with one", () => {
    expect(readiness(sessionWith(), "amiga-kurulum")).toBe("asks");
    const s = sessionWith({ tree: { root: "E:\\dist", builtHere: false } });
    expect(readiness(s, "amiga-kurulum")).toBe("ready");
  });

  it("never makes the first step ask — it is where a build begins", () => {
    expect(readiness(sessionWith(), "hedef")).toBe("ready");
  });

  it("treats an empty string as no tree at all", () => {
    const s = sessionWith({ tree: { root: "", builtHere: false } });
    expect(readiness(s, "paketler")).toBe("asks");
  });
});

describe("stepLabelKey", () => {
  it("answers a key for every step, never a sentence", () => {
    for (const step of STEP_IDS) {
      const key = stepLabelKey(step);
      expect(key.startsWith("osBuilder.step.")).toBe(true);
      expect(key).not.toContain(" ");
    }
  });
});
```

- [ ] **Step 2: Run it and watch it fail**

Run: `pnpm vitest run src/lib/buildSteps.test.ts`
Expected: FAIL — `Failed to resolve import "./buildSteps"`.

- [ ] **Step 3: Write the module**

Create `src/lib/buildSteps.ts`:

```ts
// The OS Builder's steps: which ones a build has, and whether one can act.
//
// **One step asks one question.** The screen this replaces put ten `<h2>`
// sections in a single scrolling column and the owner's verdict was "çok
// karmaşık gereksiz derecede uzun". The steps are sub-routes rather than
// internal state so browser back/forward and a jump to a step work at the
// router level.
//
// **A step opens standalone.** Navigating straight to a step is legal:
// `readiness` is how the step knows whether it can act on what the session
// already holds or has to ask. It never *blocks* — asking is a state, not a
// refusal, and no step is a gate in front of another.
//
// Pure: no DOM, no i18next. `stepLabelKey` returns a key because `src/lib`
// never renders a string.

import type { BuildKind, BuildSession } from "@/lib/buildSession";

/**
 * The steps that exist today.
 *
 * Turkish path segments, matching the design. Paths are not translated: a URL
 * that changed with the language would break every remembered link and every
 * `builtin.rs::route` value.
 *
 * `bilesenler` (components) and `ozet` (summary) are **not here yet** — the
 * components live inside `OsInstall.tsx` until wave 2 splits it, and the
 * summary is wave 3. A route that renders nothing is worse than a route that
 * does not exist.
 */
export const STEP_IDS = [
  "hedef",
  "kaynak",
  "paketler",
  "amiga-kurulum",
  "kart",
  "birimler",
] as const;

export type StepId = (typeof STEP_IDS)[number];

/** Whether a step can act on what the session holds, or has to ask first. */
export type Readiness = "ready" | "asks";

/**
 * The steps one kind of build has.
 *
 * Not every kind has every step, and showing a card step to someone building
 * a distribution tree is the "sections that do not belong on this screen"
 * complaint the owner made. `hedef` is always first: it is where the kind is
 * chosen, so it is the one step every build has.
 */
export function stepsFor(kind: BuildKind): StepId[] {
  switch (kind) {
    case "install":
      return ["hedef", "kaynak", "paketler", "amiga-kurulum"];
    case "boot-card":
      return ["hedef", "kart"];
    case "prepare-volumes":
      return ["hedef", "birimler"];
    case "distro":
      // Registered `available: false` on the engine side and Coming Later on
      // screen (§96); there is no second step to offer yet.
      return ["hedef"];
  }
}

/**
 * Whether a step has what it needs.
 *
 * Only the two tree-consuming steps can be short of anything in this wave.
 * `kaynak`, `kart` and `birimler` each own their own inputs and ask for them
 * inline, exactly as they do today.
 */
export function readiness(session: BuildSession, step: StepId): Readiness {
  switch (step) {
    case "paketler":
    case "amiga-kurulum":
      return hasTree(session) ? "ready" : "asks";
    default:
      return "ready";
  }
}

function hasTree(session: BuildSession): boolean {
  // An empty string is a folder nobody picked — a `Field` that has been
  // cleared writes one, and treating it as a path sends `""` to the backend.
  return typeof session.tree.root === "string" && session.tree.root.length > 0;
}

/** The i18n key for a step's name in the progress strip. */
export function stepLabelKey(step: StepId): string {
  return `osBuilder.step.${step}`;
}
```

- [ ] **Step 4: Run the test**

Run: `pnpm vitest run src/lib/buildSteps.test.ts`
Expected: PASS, 12 tests.

- [ ] **Step 5: Add the six step labels to both catalogues**

`src/i18n/en.json`, under `osBuilder`, a new `step` object:

```json
"step": {
  "hedef": "What are we building",
  "kaynak": "Install media",
  "paketler": "Update packages",
  "amiga-kurulum": "Install on the Amiga",
  "kart": "The card image",
  "birimler": "Prepare volumes"
}
```

`src/i18n/tr.json`, same place:

```json
"step": {
  "hedef": "Ne yapıyoruz",
  "kaynak": "Kurulum ortamı",
  "paketler": "Güncelleme paketleri",
  "amiga-kurulum": "Amiga'da kur",
  "kart": "Kart imajı",
  "birimler": "Birimleri hazırla"
}
```

- [ ] **Step 6: Run the catalogue tests**

Run: `pnpm vitest run src/i18n/`
Expected: PASS — parity holds, no empty value, no interpolation mismatch.

- [ ] **Step 7: Commit**

```bash
git add src/lib/buildSteps.ts src/lib/buildSteps.test.ts src/i18n/en.json src/i18n/tr.json
git commit -m "feat(osbuilder): the step registry — which steps a kind has, and what each needs"
```

---

## Task 4: The hook — reading and writing the session

**Files:**
- Create: `src/lib/useBuildSession.ts`
- Test: `src/lib/useBuildSession.test.tsx` (**`.tsx`** — it renders a hook, so it needs jsdom)

**Interfaces:**
- Consumes: everything Task 2 produced; `useRemembered`/`useRememberedShape` from `@/lib/useRemembered`.
- Produces:
  ```ts
  interface BuildSessionApi {
    session: BuildSession;
    setKind(next: BuildKind): void;
    setMedia(change: Partial<MediaChoice>): void;
    setRom(path: string | null): void;
    setRelease(next: InstallRelease): void;
    setTree(change: Partial<TreeChoice>): void;
    setComponents(change: Partial<ComponentChoice>): void;
    setPackages(change: Partial<PackageChoice>): void;
  }
  function useBuildSession(): BuildSessionApi;
  ```

- [ ] **Step 1: Write the failing test**

Create `src/lib/useBuildSession.test.tsx`:

```tsx
// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
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
  const { session, setTree } = useBuildSession();
  return (
    <div>
      <span data-testid="root">{session.tree.root ?? "(none)"}</span>
      <span data-testid="builtHere">{String(session.tree.builtHere)}</span>
      <span data-testid="chosen">{session.components.chosen.join(",")}</span>
      <span data-testid="release">{session.release}</span>
      <button onClick={() => setTree({ root: "E:\\picked", builtHere: false })}>pick</button>
    </div>
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

  it("writes the user's pick into the session's own key, not the legacy one", async () => {
    seed({ "osinstall.destination": "E:\\amiga\\dist-3.9" });
    render(<Probe />);
    await userEvent.click(screen.getByRole("button", { name: "pick" }));
    const bag = useSettingsStore.getState().settings.remembered as Record<string, unknown>;
    expect(bag["buildSession.tree"]).toEqual({ root: "E:\\picked", builtHere: false });
    // The legacy key is left as it was — a rollback must still find it.
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

  it("hands back the same session object when nothing changed", () => {
    // ART-178/ART-195: a fresh identity per render turns this screen's
    // effects into a loop. `useRememberedShape` stabilises; this proves the
    // facade does not undo that by rebuilding the object on top of it.
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
```

- [ ] **Step 2: Run it and watch it fail**

Run: `pnpm vitest run src/lib/useBuildSession.test.tsx`
Expected: FAIL — `Failed to resolve import "@/lib/useBuildSession"`.

- [ ] **Step 3: Write the hook**

Create `src/lib/useBuildSession.ts`:

```ts
// The React half of `@/lib/buildSession`.
//
// Every section reads through `useRememberedShape`, which rebuilds an object
// field by field (so a key added in a later ART costs the user nothing) and
// stabilises its identity with `sameRemembered` (so an effect depending on the
// session does not re-run on every render — ART-178/ART-195).
//
// `tree` is the one section with a **seeded** default rather than a constant
// one: its fallback is `seedTreeRoot`, which reaches the legacy keys. That is
// the whole migration, and it lives here because `useRememberedShape` already
// does exactly the right thing with a fallback — it uses it when the stored
// value is absent, and stops using it the moment there is one.

import { useCallback, useMemo } from "react";

import {
  DEFAULT_MEDIA,
  DEFAULT_PACKAGES,
  MEDIA_SPEC,
  COMPONENT_SPEC,
  PACKAGE_SPEC,
  SESSION_KEYS,
  LEGACY_KEYS,
  TREE_SPEC,
  isBuildKind,
  seedTreeRoot,
  seededComponents,
  type BuildKind,
  type BuildSession,
  type ComponentChoice,
  type MediaChoice,
  type PackageChoice,
  type TreeChoice,
} from "@/lib/buildSession";
import { isInstallRelease, type InstallRelease } from "@/lib/osinstall";
import { isFlag, isTextList, isTextOrNothing, recall } from "@/lib/remembered";
import { useRemembered, useRememberedShape } from "@/lib/useRemembered";
import { useSettingsStore } from "@/stores/settingsStore";

export interface BuildSessionApi {
  session: BuildSession;
  setKind: (next: BuildKind) => void;
  setMedia: (change: Partial<MediaChoice>) => void;
  setRom: (path: string | null) => void;
  setRelease: (next: InstallRelease) => void;
  setTree: (change: Partial<TreeChoice>) => void;
  setComponents: (change: Partial<ComponentChoice>) => void;
  setPackages: (change: Partial<PackageChoice>) => void;
}

export function useBuildSession(): BuildSessionApi {
  const bag = useSettingsStore((s) => s.settings.remembered);

  const [kind, setKind] = useRemembered<BuildKind>(
    SESSION_KEYS.kind,
    isBuildKind,
    recall(bag, LEGACY_KEYS.kind, isBuildKind, "boot-card")
  );

  const [release, setRelease] = useRemembered<InstallRelease>(
    SESSION_KEYS.release,
    isInstallRelease,
    recall(bag, LEGACY_KEYS.release, isInstallRelease, "AmigaOS 3.2")
  );

  const [media, setMedia] = useRememberedShape<MediaChoice>(SESSION_KEYS.media, MEDIA_SPEC, {
    folder: recall(bag, LEGACY_KEYS.mediaFolder, isTextOrNothing, DEFAULT_MEDIA.folder),
    reuseScan: recall(bag, LEGACY_KEYS.reuseScan, isFlag, true),
  });

  const [rom, setRomShape] = useRememberedShape<{ path: string | null }>(
    SESSION_KEYS.rom,
    { path: isTextOrNothing },
    { path: recall(bag, LEGACY_KEYS.rom, isTextOrNothing, null) }
  );

  // The migration, in one line: an absent `buildSession.tree` falls back to
  // whichever legacy key the user's own history put a folder in.
  const [tree, setTree] = useRememberedShape<TreeChoice>(SESSION_KEYS.tree, TREE_SPEC, {
    root: seedTreeRoot(bag),
    builtHere: false,
  });

  // Per release: switching release and switching back must find the earlier
  // ticks, which is why this key is derived rather than fixed.
  const [components, setComponents] = useRememberedShape<ComponentChoice>(
    SESSION_KEYS.components(release),
    COMPONENT_SPEC,
    seededComponents(bag, release)
  );

  const [packages, setPackages] = useRememberedShape<PackageChoice>(
    SESSION_KEYS.packages,
    PACKAGE_SPEC,
    {
      folder: recall(bag, LEGACY_KEYS.packagesFolder, isTextOrNothing, DEFAULT_PACKAGES.folder),
      chosen: recall(bag, LEGACY_KEYS.packagesChosen, isTextList, DEFAULT_PACKAGES.chosen),
    }
  );

  const setRom = useCallback((path: string | null) => setRomShape({ path }), [setRomShape]);

  const session = useMemo<BuildSession>(
    () => ({ kind, media, rom, release, tree, components, packages }),
    [kind, media, rom, release, tree, components, packages]
  );

  return {
    session,
    setKind,
    setMedia,
    setRom,
    setRelease,
    setTree,
    setComponents,
    setPackages,
  };
}
```

> **Note for the implementer:** every guard used above already exists and was
> checked on 2026-08-21 — `isInstallRelease` at `src/lib/osinstall.ts:112`
> (`InstallRelease` itself at `:103`), and `isFlag` / `isTextList` /
> `isTextOrNothing` / `recall` in `src/lib/remembered.ts`. Do not write a
> second guard that means the same thing as one of these.

- [ ] **Step 4: Run the test**

Run: `pnpm vitest run src/lib/useBuildSession.test.tsx`
Expected: PASS, 5 tests.

- [ ] **Step 5: Run the mutation**

**Mutation 2 again, at the layer that matters.** Replace the `tree` fallback with a constant: `{ root: null, builtHere: false }`.

Run: `pnpm vitest run src/lib/useBuildSession.test.tsx`
Expected: FAIL — both *"hands the packages step the tree ART last wrote"* and *"leaves a tree the user picked by hand"*. Put it back; re-run: PASS.

- [ ] **Step 6: Lint and commit**

```bash
pnpm lint
git add src/lib/useBuildSession.ts src/lib/useBuildSession.test.tsx
git commit -m "feat(osbuilder): the session hook, over the settings store rather than beside it"
```

---

## Task 5: ART-197 — OsInstall stops owning the tree, and says when it sets one

**Files:**
- Modify: `src/components/osbuilder/OsInstall.tsx` (:317-341 the three package keys; :605-625 the result subscription; :1359-1379 the two panel mounts)
- Modify: `src/components/osbuilder/OsInstall.test.tsx`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json` (one new key)

**Interfaces:**
- Consumes: `useBuildSession` from Task 4.
- Produces: `OsInstall` no longer holds `packagesTreeRoot`; the tree it writes is the session's.

This is the task the round exists for. Three changes:

1. `packagesTreeRoot` / `setPackagesTreeRoot` are deleted. Both panel mounts read `session.tree.root` and write through `setTree`.
2. The `onOsInstallResult` subscription writes the finished tree into the session: `setTree({ root: r.destination, builtHere: true })`.
3. **The screen says so.** The result card gains one sentence naming what the next steps will now act on. A carry the user cannot see is the same defect class as a carry that does not happen.

`packagesFolder` and `packagesChosen` also move to the session in this task (they are two of the eleven, and `PackagePanel` takes both as props already) — leaving them behind would mean two sources of truth for one panel.

- [ ] **Step 1: Write the failing test**

Add to `src/components/osbuilder/OsInstall.test.tsx`, in a new `describe`:

```tsx
describe("the tree it builds is the tree the next steps get (ART-197)", () => {
  it("hands a finished install's destination to the session", async () => {
    // The defect: `OsInstall` remembered where it *wrote* under
    // `osinstall.destination`, and the panels beneath it read a different
    // key. A user who had just watched ART write 1915 files was asked to go
    // and find them.
    let announce: ((r: OsInstallResult) => void) | null = null;
    onResultMock.mockImplementation((fn: (r: OsInstallResult) => void) => {
      announce = fn;
      return Promise.resolve(() => {});
    });

    renderWith({ "osinstall.mediaFolder": "E:\\media" });
    await waitFor(() => expect(announce).not.toBeNull());

    act(() => {
      announce!({
        destination: "E:\\amiga\\dist-3.9",
        outcome: { files: 1915, directories: 75, bytes: 1024 },
      } as OsInstallResult);
    });

    await waitFor(() => {
      const bag = useSettingsStore.getState().settings.remembered as Record<string, unknown>;
      expect(bag["buildSession.tree"]).toEqual({
        root: "E:\\amiga\\dist-3.9",
        builtHere: true,
      });
    });
  });

  it("says on screen which folder the next steps will act on", async () => {
    // A carry nobody can see is the same failure as a carry that does not
    // happen: the screen must not change what the next step points at
    // without saying so.
    let announce: ((r: OsInstallResult) => void) | null = null;
    onResultMock.mockImplementation((fn: (r: OsInstallResult) => void) => {
      announce = fn;
      return Promise.resolve(() => {});
    });

    renderWith({ "osinstall.mediaFolder": "E:\\media" });
    await waitFor(() => expect(announce).not.toBeNull());
    act(() => {
      announce!({
        destination: "E:\\amiga\\dist-3.9",
        outcome: { files: 1915, directories: 75, bytes: 1024 },
      } as OsInstallResult);
    });

    expect(await screen.findByText(/E:\\amiga\\dist-3\.9/)).toBeTruthy();
    // Not the raw key, and not an unrendered interpolation.
    expect(screen.queryByText(/osinstall\.result\.carried/)).toBeNull();
    expect(screen.queryByText(/\{\{/)).toBeNull();
  });
});
```

> **Implementer:** `renderWith(remembered)` is this file's existing helper at
> `OsInstall.test.tsx:282`; `act` comes from `@testing-library/react` — add it
> to the existing import if it is not already there. Match `OsInstallResult`'s
> real field names by reading `src/lib/osinstall.ts` rather than trusting the
> cast above; the cast exists so the test states only the fields it cares about.

- [ ] **Step 2: Run it and watch it fail**

Run: `pnpm vitest run src/components/osbuilder/OsInstall.test.tsx -t "ART-197"`
Expected: FAIL — `buildSession.tree` is undefined (nothing writes it) and the sentence does not exist.

- [ ] **Step 3: Add the sentence to both catalogues**

`src/i18n/en.json`, under `osinstall.result`:

```json
"carried": "The update and Amiga-side install steps will act on this folder: {{root}}"
```

`src/i18n/tr.json`, same place:

```json
"carried": "Güncelleme ve Amiga'da kurulum adımları bu klasör üzerinde çalışacak: {{root}}"
```

Both carry `{{root}}` — the parity test checks interpolation variables match, so neither may drop it.

- [ ] **Step 4: Rewire `OsInstall.tsx`**

Delete the three `useRemembered` declarations at `:327-341` and the doc comment above them at `:317-326`. In their place:

```ts
  /**
   * The tree, the package folder and the ticked packages now live in the
   * **session** (`@/lib/buildSession`), not in three keys of this screen's own.
   *
   * **ART-197.** They used to be `osinstall.packages.treeRoot` and friends,
   * while the folder this screen *wrote* into was `osinstall.destination` —
   * two variables for one folder, joined by nothing, so a user who had just
   * watched ART write 1915 files was asked immediately below to go and find
   * them.
   *
   * The comment that stood here objected to filling the tree in from a
   * finished build, on the grounds that overwriting a remembered pick without
   * the user acting is exactly what this project's remembered-settings rule
   * forbids. That objection is answered rather than dropped: pressing Build
   * **is** the user acting — they chose the folder and asked ART to fill it —
   * and the change is stated on screen (`osinstall.result.carried`) instead of
   * being made silently. A user who wants a different tree still picks one;
   * the picker never goes away.
   */
  const { session, setTree, setPackages } = useBuildSession();
  const treeRoot = session.tree.root;
```

Change the result subscription (`:616-623`) to add one line:

```ts
      onOsInstallResult((r) => {
        setResult(r);
        setBusy(false);
        setConfirmed(false);
        setVerifyDistRoot(r.destination);
        // The tree ART has just written is the tree the next steps act on.
        // This is ART-197's fix, and it is a write the screen announces.
        setTree({ root: r.destination, builtHere: true });
        installJob.current = null;
        setProgress(null);
      })
```

> `setTree` must go in that effect's dependency array, or `tsc`'s exhaustive-deps
> lint will flag it. It is stable (`useCallback` inside `useRememberedShape`), so
> adding it does not re-subscribe on every render — verify by running the
> existing "does not keep re-planning" test at `OsInstall.test.tsx:586`, which is
> the guard against exactly that regression.

Add the sentence to the result card (after the `osinstall.result.root` line, `:1350`):

```tsx
          <p className="muted" style={{ fontSize: 12, margin: "0 0 8px", wordBreak: "break-all" }}>
            {t("osinstall.result.carried", { root: result.destination })}
          </p>
```

Change both panel mounts (`:1359-1379`):

```tsx
      <PackagePanel
        treeRoot={treeRoot}
        onTreeRootChange={(root) => setTree({ root, builtHere: false })}
        packageFolder={session.packages.folder}
        onPackageFolderChange={(folder) => setPackages({ folder })}
        chosen={session.packages.chosen}
        onChosenChange={(chosen) => setPackages({ chosen })}
      />
```

```tsx
      <AmigaInstallPanel
        treeRoot={treeRoot}
        onTreeRootChange={(root) => setTree({ root, builtHere: false })}
        packageFolder={session.packages.folder}
      />
```

`builtHere: false` on a hand-pick is the point of the flag: a tree the user pointed at is not a tree ART built.

- [ ] **Step 5: Run the tests**

Run: `pnpm vitest run src/components/osbuilder/`
Expected: PASS — the two new tests and every existing one in `OsInstall.test.tsx`, `PackagePanel.test.tsx` and `AmigaInstallPanel.test.tsx`.

If an existing test seeded `osinstall.packages.treeRoot` directly, it will now fail. **Do not delete it** — change its seed to `buildSession.tree` and keep a second case seeding the legacy key, because that case is the migration.

- [ ] **Step 6: Run the mutation that matters most**

**Mutation 1 of the spec's four — "break the carry".** Delete the `setTree({ root: r.destination, builtHere: true })` line.

Run: `pnpm vitest run src/components/osbuilder/OsInstall.test.tsx -t "ART-197"`
Expected: FAIL — *"hands a finished install's destination to the session"*.

Then put that line back and instead delete the `osinstall.result.carried` paragraph.
Expected: FAIL — *"says on screen which folder the next steps will act on"*.

Put both back. Re-run: PASS. **If either survives, the test is not a guard — fix it before going on and say so in the report.**

- [ ] **Step 7: Lint and commit**

```bash
pnpm lint
pnpm test
git add src/components/osbuilder/OsInstall.tsx src/components/osbuilder/OsInstall.test.tsx src/i18n/en.json src/i18n/tr.json
git commit -m "fix(osbuilder): the tree ART builds is the tree the next steps get (ART-197)"
```

---

## Task 6: Sub-routes and the wizard shell

**Files:**
- Modify: `src/App.tsx` (:74, the `os-builder` route)
- Modify: `src/pages/OsBuilder.tsx`
- Create: `src/pages/osbuilder/steps.tsx`

**Interfaces:**
- Consumes: `stepsFor`, `stepLabelKey`, `readiness` (Task 3); `useBuildSession` (Task 4).
- Produces: `<OsBuilderShell/>`, `<StepHedef/>`, `<StepKaynak/>`, `<StepPaketler/>`, `<StepAmigaKurulum/>`, `<StepKart/>`, `<StepBirimler/>`.

**Two hazards to carry through this task, both real:**

1. **The drop path.** `OsBuilder.tsx:76-96` sets `kind` to `install` when a disc is dropped, and hands `OsInstall` a `droppedMedia` keyed on `location.key` so a second drop of the same file still registers. Under sub-routes the *parent* still receives that state and must both set the kind **and** navigate to `kaynak`, passing the state on. Losing this makes the "disc dropped on the panel offers the OS Builder" workflow do nothing visible.
2. **`route::OS_BUILDER` stays real.** `/os-builder` keeps rendering — as the shell with an index element that redirects to the furthest step reached. `every_workflow_route_is_a_real_app_route` (`builtin.rs:923`) reads a hand-kept list; it does not need changing, but it does need to stay true.

- [ ] **Step 1: Write the failing test**

Create `src/pages/osbuilder/steps.test.tsx`:

```tsx
// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";

vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  getSettings: vi.fn(),
  saveSettings: vi.fn().mockResolvedValue(undefined),
}));

// The panels each reach Tauri on mount; this file is about routing, so they
// are replaced by markers. `OsInstall`'s own file tests the real component.
vi.mock("@/components/osbuilder/PackagePanel", () => ({
  PackagePanel: ({ treeRoot }: { treeRoot: string | null }) => (
    <div data-testid="packages">{treeRoot ?? "(no tree)"}</div>
  ),
}));
vi.mock("@/components/osbuilder/AmigaInstallPanel", () => ({
  AmigaInstallPanel: ({ treeRoot }: { treeRoot: string | null }) => (
    <div data-testid="amiga">{treeRoot ?? "(no tree)"}</div>
  ),
}));
vi.mock("@/components/osbuilder/CardBuilder", () => ({
  CardBuilder: () => <div data-testid="card" />,
}));
vi.mock("@/components/osbuilder/VolumePreload", () => ({
  VolumePreload: () => <div data-testid="volumes" />,
}));
vi.mock("@/components/osbuilder/OsInstall", () => ({
  OsInstall: () => <div data-testid="install" />,
}));

const { useSettingsStore } = await import("@/stores/settingsStore");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");
const { OsBuilder } = await import("@/pages/OsBuilder");
const { StepPaketler } = await import("@/pages/osbuilder/steps");

function seed(remembered: Record<string, unknown>) {
  useSettingsStore.setState({ loaded: true, settings: { ...DEFAULT_SETTINGS, remembered } });
}

function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route path="/os-builder" element={<OsBuilder />}>
          <Route path="paketler" element={<StepPaketler />} />
        </Route>
      </Routes>
    </MemoryRouter>
  );
}

afterEach(() => {
  cleanup();
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
});

describe("a step opened on its own", () => {
  it("acts on the session's tree when there is one", () => {
    seed({ "buildSession.kind": "install", "buildSession.tree": { root: "E:\\dist", builtHere: true } });
    renderAt("/os-builder/paketler");
    expect(screen.getByTestId("packages").textContent).toBe("E:\\dist");
  });

  it("asks rather than rendering empty when there is no tree", () => {
    // The spec's fourth mutation: a step navigated to cold must *ask*, never
    // throw and never render a blank card.
    seed({ "buildSession.kind": "install" });
    renderAt("/os-builder/paketler");
    expect(screen.getByTestId("packages").textContent).toBe("(no tree)");
    // And it says which step answers the question — as a rendered sentence.
    // Asserting on the key would pass on the very failure this catches: a
    // missing catalogue entry renders as the raw key.
    expect(screen.getByText(/AmigaOS folder/i)).toBeTruthy();
    expect(screen.queryByText(/osBuilder\.step\./)).toBeNull();
    expect(screen.getByRole("link", { name: /Install media/i })).toBeTruthy();
  });

  it("shows the strip with the steps this kind has, and not the others", () => {
    seed({ "buildSession.kind": "install", "buildSession.tree": { root: "E:\\dist", builtHere: true } });
    renderAt("/os-builder/paketler");
    // `install` has no card step.
    expect(screen.queryByTestId("card")).toBeNull();
    expect(screen.getAllByRole("link").length).toBe(4);
  });
});
```

> **Implementer:** the third assertion's exact matcher depends on how you render
> the strip. If a step is a `<button>` rather than a `<Link>`, assert on
> `getAllByRole("button")` and say so. Do not weaken it to "at least one" — the
> point is that a kind's steps and only a kind's steps are offered.

- [ ] **Step 2: Run it and watch it fail**

Run: `pnpm vitest run src/pages/osbuilder/steps.test.tsx`
Expected: FAIL — `Failed to resolve import "@/pages/osbuilder/steps"`.

- [ ] **Step 3: Write the step components**

Create `src/pages/osbuilder/steps.tsx`:

```tsx
// The six steps, as thin as they can be.
//
// A step's whole job is to mount the panel that already exists and feed it
// from the session. **No panel is rewritten here** — that is wave 2. What
// changes is where a panel's values come from: the session, so a value
// reaches the step that needs it without anyone remembering to wire it.
//
// A step opened on its own asks rather than rendering empty. `readiness`
// decides; the sentence names the step that answers the question, because a
// user told "no tree" and not told where a tree comes from has been given
// nothing.

import { useTranslation } from "react-i18next";
import { Link } from "react-router-dom";

import { readiness } from "@/lib/buildSteps";
import { useBuildSession } from "@/lib/useBuildSession";
import { AmigaInstallPanel } from "@/components/osbuilder/AmigaInstallPanel";
import { CardBuilder } from "@/components/osbuilder/CardBuilder";
import { OsInstall } from "@/components/osbuilder/OsInstall";
import { PackagePanel } from "@/components/osbuilder/PackagePanel";
import { VolumePreload } from "@/components/osbuilder/VolumePreload";

/** What a step says when it has been opened without what it needs. */
function Asks({ messageKey }: { messageKey: string }) {
  const { t } = useTranslation();
  return (
    <div
      className="badge badge-warn"
      style={{ display: "block", padding: "8px 12px", marginBottom: 16, fontSize: 12 }}
    >
      {t(messageKey)}{" "}
      <Link to="/os-builder/kaynak">{t("osBuilder.step.kaynak")}</Link>
    </div>
  );
}

export function StepKaynak() {
  return <OsInstall droppedMedia={null} />;
}

export function StepPaketler() {
  const { session, setTree, setPackages } = useBuildSession();
  const asks = readiness(session, "paketler") === "asks";
  return (
    <>
      {asks && <Asks messageKey="osBuilder.step.asksTree" />}
      <PackagePanel
        treeRoot={session.tree.root}
        onTreeRootChange={(root) => setTree({ root, builtHere: false })}
        packageFolder={session.packages.folder}
        onPackageFolderChange={(folder) => setPackages({ folder })}
        chosen={session.packages.chosen}
        onChosenChange={(chosen) => setPackages({ chosen })}
      />
    </>
  );
}

export function StepAmigaKurulum() {
  const { session, setTree } = useBuildSession();
  const asks = readiness(session, "amiga-kurulum") === "asks";
  return (
    <>
      {asks && <Asks messageKey="osBuilder.step.asksTree" />}
      <AmigaInstallPanel
        treeRoot={session.tree.root}
        onTreeRootChange={(root) => setTree({ root, builtHere: false })}
        packageFolder={session.packages.folder}
      />
    </>
  );
}

export function StepKart() {
  return <CardBuilder />;
}

export function StepBirimler() {
  return <VolumePreload />;
}
```

`StepHedef` — the kind picker — stays in `OsBuilder.tsx`, because it is the only
step that changes what the shell itself shows.

- [ ] **Step 4: Add the two step sentences to both catalogues**

`src/i18n/en.json`, inside `osBuilder.step`:

```json
"asksTree": "This step works on an AmigaOS folder, and none has been chosen yet. Pick one below, or build one first:"
```

`src/i18n/tr.json`:

```json
"asksTree": "Bu adım bir AmigaOS klasörü üzerinde çalışır, henüz seçilmedi. Aşağıdan birini seç ya da önce bir tane kur:"
```

- [ ] **Step 5: Turn `OsBuilder.tsx` into the shell**

The existing file keeps its `distro` content and its kind picker; what changes:

- It renders `<Outlet/>` where it used to render `<CardBuilder/>` / `<OsInstall/>` / `<VolumePreload/>` by kind.
- The kind picker moves under a `hedef` step, and choosing a kind navigates to that kind's second step (`stepsFor(kind)[1]`) when it has one.
- The progress strip renders `stepsFor(session.kind)` as links, marking the current one.
- `kind` comes from `useBuildSession()`, not from `useRemembered("osBuilder.kind")`.
- The drop effect keeps its `location.state` shape and additionally navigates:

```ts
  useEffect(() => {
    const state = location.state as { path?: string } | null;
    if (!state?.path) return;
    setKind("install");
    // Carry the drop through to the step that acts on it — the state object
    // and `location.key` both, so a second drop of the same file still
    // registers downstream (the reason `droppedMedia` is keyed on the key).
    navigate("/os-builder/kaynak", { state });
  }, [location.state, location.key, setKind, navigate]);
```

`StepKaynak` then reads `useLocation().state` for its `droppedMedia` instead of
receiving it as a prop from the parent. **Keep the `arrivalKey` pairing** — it is
`OsInstall.tsx:227-238`'s documented reason for existing.

- [ ] **Step 6: Add the child routes**

`src/App.tsx`, replacing line 74:

```tsx
          <Route path="os-builder" element={<OsBuilder />}>
            <Route index element={<Navigate to="hedef" replace />} />
            <Route path="hedef" element={<StepHedef />} />
            <Route path="kaynak" element={<StepKaynak />} />
            <Route path="paketler" element={<StepPaketler />} />
            <Route path="amiga-kurulum" element={<StepAmigaKurulum />} />
            <Route path="kart" element={<StepKart />} />
            <Route path="birimler" element={<StepBirimler />} />
          </Route>
```

- [ ] **Step 7: Run everything that touches routing**

```bash
pnpm vitest run src/pages/osbuilder/steps.test.tsx
pnpm test
pnpm lint
cd src-tauri && cargo test every_workflow_route_is_a_real_app_route -- --exact
```

Expected: all PASS. The Rust test proves `/os-builder` is still a route a
workflow may point at.

- [ ] **Step 8: Mutation — a step opened cold**

**Mutation 4 of the spec's four.** In `StepPaketler`, delete the `{asks && <Asks …/>}` line.

Run: `pnpm vitest run src/pages/osbuilder/steps.test.tsx`
Expected: FAIL — *"asks rather than rendering empty when there is no tree"*.

Put it back. Re-run: PASS.

- [ ] **Step 9: Commit**

```bash
git add src/App.tsx src/pages/OsBuilder.tsx src/pages/osbuilder/ src/i18n/en.json src/i18n/tr.json
git commit -m "feat(osbuilder): eight sections become steps on their own routes, with a strip above"
```

---

## Task 7: Drive it, in the real application

**Files:** none — this task produces evidence, not code.

The rule this task exists for: *"the tests keep being right about the code and wrong about the world."* Seven defects in one round were found by driving the screen and none by 1614 tests. jsdom does no layout, so it cannot see an overflow, a strip that wraps, or a Turkish label that does not fit.

- [ ] **Step 1: Build and launch**

```bash
pnpm tauri build --no-bundle
```

**Close any running ART first** — `cargo` cannot replace a locked
`amiga-retro-toolkit.exe` and fails with `os error 5`. Then launch
`src-tauri/target/release/amiga-retro-toolkit.exe`.

- [ ] **Step 2: Walk the carry, in both languages**

1. Open the OS Builder. Confirm the strip shows the steps for the remembered kind and no others.
2. Build a tree (or, if no media is to hand, use a folder that already holds a `distribution.json`).
3. Confirm the result card names the folder **and** says the next steps will act on it.
4. Go to `Güncelleme paketleri`. **The folder must already be there.** This is ART-197; if the field is empty, the round has not landed.
5. Navigate straight to `/os-builder/paketler` in a fresh run with no tree. Confirm it *asks* and links to the step that answers.
6. Switch to Turkish and repeat 3–5. Confirm no raw key, no `{{root}}`, and that the strip's labels fit.

- [ ] **Step 3: Confirm nothing was forgotten**

Open `settings.json` and confirm the legacy keys are still present and the
`buildSession.*` keys now exist beside them. A user's paths must be readable by
both this build and the one before it.

- [ ] **Step 4: Write down what was seen**

Screenshots or a plain list, into the session report. **State plainly if a step
was not driven** — an undriven step is not a passed one.

---

## Task 8: The documents

**Files:** `docs/ISSUES.md` · `docs/STATUS.md` · `docs/FEATURES.md` · `CHANGELOG.md`

- [ ] **Step 1: Move ART-197 and ART-198 to Fixed**

Each keeps its id and gains the test that proves it:

- **ART-197** — `hands a finished install's destination to the session` and `says on screen which folder the next steps will act on` (`OsInstall.test.tsx`), plus `hands the packages step the tree ART last wrote, with nothing wired by hand` (`useBuildSession.test.tsx`). Record that mutation 1 was run and what failed.
- **ART-198** — `does not offer an unofficial pack as an example of an official update` and its two neighbours (`src/i18n/copy.test.ts`).

- [ ] **Step 2: Record the count that was wrong**

The spec says 22 remembered keys; there are 36. Note it in ART-197's entry —
the number is the reason the migration table in this plan exists, and a later
reader following the spec alone would migrate two thirds of what there is.

- [ ] **Step 3: STATUS.md**

A session log line with the real numbers (Rust unchanged at 2260; frontend up
from 784 by whatever this round added — **count them, do not estimate**), and
the "Picking up next session" block rewritten to point at **wave 2**.

- [ ] **Step 4: FEATURES.md**

Only if a row's claim actually changed. The OS Builder rows describing a single
scrolling column now describe steps. **Do not flip a row that has no test.**

- [ ] **Step 5: CHANGELOG.md**

User-visible: the builder is now a sequence of steps; the folder ART builds is
carried to the steps that use it; the packages sentence no longer contradicts
itself.

- [ ] **Step 6: Full verification, then commit**

```bash
pnpm lint
pnpm test
pnpm test          # twice — the standing rule
cd src-tauri && cargo test
git add -u
git commit -m "docs: wave 1 of the OS Builder flow — ART-197 and ART-198 closed"
```

> `git add -u` rather than `git add -A`: a background agent's scratch must not
> ride along in the commit.

---

## Self-review against the spec

| Spec requirement (§ Scope, wave 1) | Task |
|---|---|
| The store | 2, 4 (as a facade — deviation 1, stated) |
| The 22-key migration | 2 (36 keys measured; 11 taken over, 25 left in place) |
| The sub-route table | 6 (six routes — deviation 3, stated) |
| The progress strip | 6 |
| No panel rewritten; each mounted at its step, reading the session | 5, 6 |
| **ART-197 closes here** | 5 |
| **ART-198 closes here** | 1 |
| Mutation 1 — break the carry | 5, step 6 |
| Mutation 2 — remove the migration | 2 step 5, 4 step 5 |
| Mutation 4 — open a step with no precondition | 6, step 8 |
| Mutation 3 — silence a skipped step | **wave 3** (the skipped-step sentences are wave 3's scope) |
| `sameRemembered` identity guard kept | 4, its fifth test |
| Both catalogues in one commit | 1, 3, 5, 6 |
| Rust untouched except `describe_tree` | `describe_tree` is **wave 2**; this wave adds no Rust |
| ART never writes a physical card | nothing in this wave touches a device path |

**Not in this wave, and deliberately:** the artefact picker with its list of
trees ART already built (wave 2), `describe_tree` (wave 2), splitting
`OsInstall.tsx` (wave 2), deleting the duplicated ROM and card fields (wave 2),
the summary step and the field renames (wave 3).
