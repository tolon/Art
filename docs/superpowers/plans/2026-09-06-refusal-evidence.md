# Refusal Evidence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When some install disks are present and some are not, the "Why this cannot be built yet" section says what the folder actually holds and which release it looks like — the context that today only appears when *nothing* matches.

**Architecture:** Frontend only. One pure helper in `src/lib/osinstall.ts` returning a `Phrase`, rendered above the existing refusals list in `OsInstall.tsx` from values already in scope. No Rust, no new command, no `RefusalReason` change.

**Tech Stack:** TypeScript, React, `react-i18next`, Vitest.

**Spec:** [docs/superpowers/specs/2026-09-06-refusal-evidence-design.md](../specs/2026-09-06-refusal-evidence-design.md)

## Global Constraints

- **Nothing in `src-tauri/` changes.** If a task seems to need a Rust change, stop and say so rather than making one — the spec's §1.2.1 records that an earlier draft wanted one and why it does not.
- **`src/lib/*` has no i18next singleton.** A helper there returns `Phrase { key, params? }` and the component calls `t(phrase.key, phrase.params)`. Every variant it can return must be enumerated in `src/i18n/phrase-keys.test.ts` — nothing else in the build catches a `Phrase` pointing at a key nobody added.
- **Both catalogues, same commit.** Every new key in **both** `src/i18n/en.json` and `src/i18n/tr.json`, with matching interpolation variables. `pnpm test` fails if the key sets differ, a value is empty, or a variable present in one is missing from the other. The user of this project is Turkish; write real translations, not English copied across.
- **Component tests are `.tsx`.** jsdom applies only to `src/**/*.test.tsx`; a `.ts` component test silently gets no DOM.
- **Nothing changes unless the user changes it** — this round adds no setting, so nothing to remember.
- Core error text arrives **verbatim** and in English by design (ART-060). This round renders no core errors, but do not introduce a place that reworks one.
- **Never run a destructive git command on a file you are editing.**

## The four states, and why they must not collapse

`MediaVerdict` already distinguishes three, and there is a fourth that is none of them. **Collapsing any two is this project's named failure class** — "endings stay distinct".

| State | What the user must be told |
|---|---|
| **No folder chosen** | "You have not told me where to look" — already `osinstall.blocked.noFolder`; this round must not produce a second, competing sentence for it. |
| **Identified** | Which release the folder is, and whether it is the one being built. |
| **Ambiguous** | The folder could be more than one release; **name the candidates, pick none**. |
| **Unknown** | Nothing in the folder identifies a release. Genuine: `Fonts` and `Locale` carry no version across 3.1, 3.1.4 and 3.2 — `identify.rs` measured that against HstWB Installer's catalogue and two installation guides. |

`releaseHolding` is `null` for both Ambiguous and Unknown (`release_holding` collapses them), so the helper distinguishes them by whether the folder held anything at all — and where it cannot tell them apart, it must say the weaker thing, never the stronger.

---

## File structure

| File | Responsibility |
|---|---|
| `src/lib/osinstall.ts` | **modify** — one new pure helper beside `wrongMediaFolder` |
| `src/lib/osinstall.test.ts` | **modify** — its unit tests |
| `src/components/osbuilder/OsInstall.tsx` | **modify** — render the line above the refusals list |
| `src/components/osbuilder/OsInstall.test.tsx` | **modify** — the rendered states |
| `src/i18n/en.json`, `src/i18n/tr.json` | **modify** — the new keys, both files |
| `src/i18n/phrase-keys.test.ts` | **modify** — enumerate every variant |

---

### Task 1: The helper and its sentences

**Files:**
- Modify: `src/lib/osinstall.ts`, `src/lib/osinstall.test.ts`, `src/i18n/en.json`, `src/i18n/tr.json`, `src/i18n/phrase-keys.test.ts`

**Interfaces:**
- Produces: `export function mediaEvidence(input: { plan: InstallPlan; found: string[]; releaseHolding: string | null; release: string }): Phrase | null`
- Consumes: `InstallPlan`, `RefusalReason`, `Phrase` — all already in this file.

**Read first:** `wrongMediaFolder` in the same file, immediately above where yours goes. It is the sibling that owns the all-or-nothing case, and yours must not overlap it. Read its five conditions; your helper returns `null` in exactly the case it returns non-`null`, so the two can never both speak.

- [ ] **Step 1: Write the failing tests**

```ts
const RELEASE = "AmigaOS 3.2";

function planWith(missing: string[], items: number): InstallPlan {
  // Build a plan whose refusals are `media-missing` for `missing`, and which
  // has `items` installable items. Follow whatever fixture helper this file
  // already uses for InstallPlan; do not invent a second style.
}

it("says nothing when nothing is missing", () => {
  expect(mediaEvidence({ plan: planWith([], 40), found: ["Workbench3.2"], releaseHolding: RELEASE, release: RELEASE })).toBeNull();
});

it("says nothing when wrongMediaFolder owns the case", () => {
  // Its five conditions: folder non-empty, no items at all, every refusal
  // media-missing, none of the missing volumes present. The two helpers must
  // never both produce a sentence.
  const plan = planWith(["Workbench3.2", "Extras3.2"], 0);
  const found = ["Workbench3.1"];
  expect(wrongMediaFolder(plan, found)).not.toBeNull();
  expect(mediaEvidence({ plan, found, releaseHolding: "AmigaOS 3.1", release: RELEASE })).toBeNull();
});

it("names what the folder holds and which disks are absent", () => {
  const phrase = mediaEvidence({
    plan: planWith(["Extras3.2", "Classes3.2"], 12),
    found: ["Workbench3.2", "Fonts", "Locale", "Install3.2"],
    releaseHolding: RELEASE,
    release: RELEASE,
  });
  expect(phrase?.key).toBe("osinstall.evidence.sameRelease");
  expect(phrase?.params?.found).toBe("Workbench3.2, Fonts, Locale, Install3.2");
  expect(phrase?.params?.missing).toBe("Extras3.2, Classes3.2");
});

it("says which release the folder is when it is a different one", () => {
  const phrase = mediaEvidence({
    plan: planWith(["Extras3.2"], 3),
    found: ["Workbench3.1", "Fonts", "Locale"],
    releaseHolding: "AmigaOS 3.1",
    release: RELEASE,
  });
  expect(phrase?.key).toBe("osinstall.evidence.otherRelease");
  expect(phrase?.params?.release).toBe("AmigaOS 3.1");
  // and it still names the missing disk, because that is what the user acts on
  expect(phrase?.params?.missing).toBe("Extras3.2");
});

it("does not name a release it cannot identify", () => {
  // `Fonts` and `Locale` are unversioned across 3.1, 3.1.4 and 3.2, so a
  // folder holding only those identifies nothing. Saying "this looks like
  // 3.2" here would be a confident wrong sentence.
  const phrase = mediaEvidence({
    plan: planWith(["Workbench3.2"], 2),
    found: ["Fonts", "Locale"],
    releaseHolding: null,
    release: RELEASE,
  });
  expect(phrase?.key).toBe("osinstall.evidence.unidentified");
  expect(phrase?.params?.found).toBe("Fonts, Locale");
  expect(JSON.stringify(phrase?.params)).not.toContain(RELEASE);
});

it("says nothing at all when no folder has been chosen", () => {
  // `osinstall.blocked.noFolder` already owns this; a second sentence here
  // would be two answers to one question.
  expect(mediaEvidence({ plan: planWith(["Workbench3.2"], 0), found: [], releaseHolding: null, release: RELEASE })).toBeNull();
});

it("returns a different key for every state", () => {
  // The round's central guard. Every other test checks one state in
  // isolation and would stay green if two of them were merged into one
  // sentence - which is precisely the collapse this project names as its
  // most expensive failure. This is the only test that can see it.
  const keys = [
    mediaEvidence({
      plan: planWith(["Extras3.2"], 12),
      found: ["Workbench3.2", "Fonts"],
      releaseHolding: RELEASE,
      release: RELEASE,
    }),
    mediaEvidence({
      plan: planWith(["Extras3.2"], 3),
      found: ["Workbench3.1", "Fonts"],
      releaseHolding: "AmigaOS 3.1",
      release: RELEASE,
    }),
    mediaEvidence({
      plan: planWith(["Workbench3.2"], 2),
      found: ["Fonts", "Locale"],
      releaseHolding: null,
      release: RELEASE,
    }),
  ].map((phrase) => phrase?.key);

  expect(keys.every((key) => typeof key === "string")).toBe(true);
  expect(new Set(keys).size).toBe(keys.length);
});
```

- [ ] **Step 2: Run to verify they fail** — `pnpm vitest run src/lib/osinstall.test.ts`

- [ ] **Step 3: Implement.** Three keys — `osinstall.evidence.sameRelease`, `.otherRelease`, `.unidentified` — in **both** catalogues. Write real Turkish. Extend `phrase-keys.test.ts` with all three.

- [ ] **Step 4: Run to verify they pass**, then `pnpm lint` and `pnpm test`.

- [ ] **Step 5: Mutate each guard and report the actual failure message**

| Mutation | Must fail |
|---|---|
| return a sentence even when `wrongMediaFolder` does | `says nothing when wrongMediaFolder owns the case` |
| fall back to `release` when `releaseHolding` is `null` | `does not name a release it cannot identify` |
| return `sameRelease` for a different release | `says which release the folder is when it is a different one` |
| return a sentence when `found` is empty | `says nothing at all when no folder has been chosen` |
| use one key for all states | `returns a different key for every state` |

**A trial that fails for the wrong reason is not a pass.** This has happened three times in the last two rounds — a bounds check firing before the guard under test could be reached — and each needed a better fixture, not a better assertion. Say what each failure message actually was.

- [ ] **Step 6: Commit** — only the files you changed, by explicit path; `git commit -F <a message file>`.

---

### Task 2: The line on the screen

**Files:**
- Modify: `src/components/osbuilder/OsInstall.tsx`, `src/components/osbuilder/OsInstall.test.tsx`

**Read first:** `OsInstall.tsx:1718-1723` — the comment there already reasons about why the refusals list stays for the partial case, and your line is the context it lacks. `foundVolumeNames` (`:1164`) and `releaseHolding` (`:1175`) are already in scope.

- [ ] **Step 1: Write the failing tests**

```tsx
it("shows what the folder holds above the refusals when some disks are missing", async () => {
  // Assert the specific text, and that the refusals list is STILL rendered -
  // this line adds context, it does not replace the list that names the disk.
});

it("names the other release when the folder is a different one", async () => { /* ... */ });

it("does not claim a release for a folder that identifies none", async () => {
  // Assert the release string is absent from the section.
});

it("leaves the all-or-nothing case to its own message", async () => {
  // wrongMediaFolder's sentence renders and the evidence line does not - one
  // answer, not two.
});
```

Assert the **specific sentence**, never a state with more than one cause. "The section is rendered" is true for many reasons; put the screen in a state where only the thing under test can produce the text, then assert the text.

- [ ] **Step 2: Run to verify they fail** — `pnpm vitest run src/components/osbuilder/OsInstall.test.tsx`

- [ ] **Step 3: Implement.** One line above the list, inside the existing section. No new section, no new heading.

- [ ] **Step 4: Run to verify they pass**, then `pnpm lint` and `pnpm test`.

- [ ] **Step 5: Mutate** — render the line unconditionally, and confirm `leaves the all-or-nothing case to its own message` fails. Report the actual message.

- [ ] **Step 6: Commit**

---

### Task 3: Land it

- [ ] **Step 1: The suites.** `pnpm test`, `pnpm lint`. Rust is untouched, so `cargo test` is a sanity check rather than the point — run it once and quote the **lib target's** summary (three targets; the last two are empty, so `grep "test result" | tail -1` reads the wrong one).
- [ ] **Step 2: The sweeps.** `control-byte-sweep.py`, `contrast-check.py --quiet`.
- [ ] **Step 3: The documents.** `docs/session-log.md` (a row at the top), `docs/STATUS.md` (the i18n count, and the "Picking up next session" block **updated in place**), `docs/FEATURES.md` only where a test exists, `CHANGELOG.md`. **Record what this round did not need**: no Rust change, because the evidence already existed and was gated — that is the finding, and a future reader should meet it here rather than rediscover it.
- [ ] **Step 4: Stop.** Do **not** merge or push; that is the owner's call.

---

## Self-review against the spec

| Spec section | Task |
|---|---|
| §2.1 one line, `src/lib` helper returning a `Phrase` | 1 |
| §2.2 what the screen gains | 2 |
| §2.2.1 the all-or-nothing case must not regress | 1 (test), 2 (test + mutation) |
| §2.3 the four states kept apart | 1 |
| §2.4 no guessing, no fetching, no rewriting the nineteen | nothing implements them, by design |
| §3 verification and mutation | every task's Step 5 |
| §5 risks | Task 3 states that an unreadable folder is `MediaUnreadable`'s job, not this line's |
