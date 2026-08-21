// `parity.test.ts` proves en.json and tr.json agree with each other.
// `phrase-keys.test.ts` proves every `src/lib` mapper's `Phrase` points at a
// real key. Neither looks at the ~700 literal `t("…")` calls scattered
// through `src/pages` and `src/components` — a typo there (`t("files.functionKeys.viwe")`)
// renders the raw dotted key on screen and nothing here catches it. This
// test does: it reads every `.ts`/`.tsx` file under `src`, finds every call
// to a bare `t(` (or `i18n.t(`) whose first argument is a string literal,
// and asserts the key resolves to a leaf in en.json.
//
// Only literal single-argument-string calls can be checked statically.
// Calls whose first argument is a template literal or a variable
// (`t(\`status.${x}\`)`, `t(phrase.key)`, `t(m.titleKey)`, …) are skipped —
// but counted, so a new dynamic call site has to make this number move
// deliberately rather than silently widening the blind spot. If that
// happens, look at whether the new dynamic key needs the same kind of
// compiler-checked mapping `LhaBrowser.tsx`'s `confidenceLevelKey` uses.

import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import en from "./en.json";

const SRC_ROOT = resolve(__dirname, "..");

/** Every `.ts`/`.tsx` file under `dir`, recursively. */
function sourceFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) {
      out.push(...sourceFiles(full));
    } else if (/\.(ts|tsx)$/.test(entry) && !/\.test\.tsx?$/.test(entry)) {
      out.push(full);
    }
  }
  return out;
}

/**
 * Every call to a bare `t(` — including `i18n.t(` — found in `text`.
 *
 * The word boundary before `t` means an identifier merely ending in `t`
 * (`format(`, `.at("x")`) cannot match: `\b` only holds between a
 * non-word character (or start of string) and `t`, and requiring `(`
 * immediately after that `t` rules out longer identifiers like `trim(`.
 */
const CALL_RE = /\bt\(\s*(['"])((?:\\.|(?!\1)[^\\])*)\1|\bt\(\s*(?!['"])/g;

/** Whether `dotted` (e.g. "whdload.outcome.installed") names a string leaf. */
function isLeafKey(dotted: string): boolean {
  const parts = dotted.split(".");
  let node: unknown = en;
  for (const part of parts) {
    if (typeof node !== "object" || node === null) return false;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === "string";
}

/**
 * Whether `dotted` names a leaf, accounting for i18next pluralisation:
 * `t("a.b.filesWrittenOut", { count })` resolves at runtime against
 * `filesWrittenOut_one` / `filesWrittenOut_other` — there is no bare
 * `filesWrittenOut` key in the catalogue at all.
 */
function resolvesAtRuntime(dotted: string): boolean {
  return isLeafKey(dotted) || isLeafKey(`${dotted}_one`) || isLeafKey(`${dotted}_other`);
}

const files = sourceFiles(join(SRC_ROOT, "pages")).concat(
  sourceFiles(join(SRC_ROOT, "components"))
);

const literalKeys: { file: string; key: string }[] = [];
let dynamicCalls = 0;

for (const file of files) {
  const text = readFileSync(file, "utf8");
  for (const match of text.matchAll(CALL_RE)) {
    const literal = match[2];
    if (literal !== undefined) {
      literalKeys.push({ file, key: literal });
    } else {
      dynamicCalls++;
    }
  }
}

describe("literal t(\"…\") calls in src/pages and src/components", () => {
  it("found a plausible number of call sites (sanity check the scan itself ran)", () => {
    // Guards against the scan silently matching nothing (e.g. a moved
    // directory) and the suite passing vacuously.
    expect(literalKeys.length).toBeGreaterThan(500);
  });

  it("every literal key resolves to a real catalogue leaf", () => {
    const broken = literalKeys
      .filter(({ key }) => !resolvesAtRuntime(key))
      .map(({ file, key }) => `${key}  (${file})`);
    expect(broken).toEqual([]);
  });

  it("has exactly the expected number of dynamic (non-literal) call sites", () => {
    // Widening this number on purpose means a new t() call whose key isn't
    // a plain string literal — see the file header for what to do about it.
    // 36 → 39 (Task 3): FileManager.tsx's `writeRefusal`, `copyTo`'s
    // `copyDirection` refusal and the disc footer badge all read `t()` off
    // a `Phrase`'s `.key` — `ISO_WRITE_REFUSAL` (`@/lib/isoPane`) and
    // `copyDirection`'s `"refused"` reason — the same reason
    // `describeCopy`'s callers already do, not a new pattern.
    // 39 → 41 (Task 4): the archive pane reads `ARCHIVE_WRITE_REFUSAL.key`
    // in the same two places the disc reads its own — `writeRefusal` and the
    // footer's read-only badge. Same pattern, one more read-only container.
    // 41 → 43 (Task 5): and the Commodore pane, in those same two places.
    // 43 → 45 (phase 2b): the shell's sidebar toggle reads its label and its
    // title from whichever of two keys the current state calls for.
    // 45 → 46 (2b task 3): the pane header's source combo labels each option
    // from the `labelKey` `@/lib/paneSources` gave it. The keys themselves
    // are literals in that module's own table, and `paneSources.test.ts`
    // proves every non-mount option carries one — so the scan below sees a
    // variable where the catalogue still sees a fixed, tested set.
    // 46 → 49 (2b task 3): F6's refusal (twice — the key's own `reason`, and
    // `moveSelection` re-checking after the icons are added) and the command
    // line's, all reading a `Phrase.key` the same way `copyDirection`'s
    // refusal already does. Both modules' every refusal is enumerated in
    // `phrase-keys.test.ts`, which is the check this one cannot make.
    // 49 → 48 (same task): the permanent collision-policy footer is gone, and
    // its `t(labelKey)` over a three-entry table with it. Settings names the
    // same three keys literally now.
    // 48 → 50: the HDF wizard's custom-size refusal and its size warning, both
    // reading a `Phrase.key` from `@/lib/hdfSize` — same pattern as every
    // other refusal on this list, and every branch of both is enumerated in
    // that module's own test file.
    // 50 → 54 (ART-090, the PiStorm rebuild): the screen is now generated from
    // Rust's own tables rather than from a list of cards written by hand, so
    // its labels are read by id — the hardware notes, the four profiles' title
    // and description, the display presets, and each option group and row in
    // the full inventory. Every one of those ids is an enum in
    // `core/pistorm`, and `pistormOptions.test.ts` proves the inventory
    // matches the engine field for field, which is the check this scan cannot
    // make. It replaces four dynamic calls the old screen made over a table of
    // cards it invented.
    // 54 → 57 (the PiStorm fix round): the Emu68 release-line dropdown labels
    // its two options by id, and the Kickstart picker and its suitability note
    // both read a `Phrase.key` from `@/lib/pistormRom` — the same pattern every
    // other refusal on this list follows, with every branch enumerated in that
    // module's own test file.
    // 57 → 61 (the OS Builder): a profile's licence sentence, what it asks the
    // user to supply, what may be wrong with the file they pointed at, and its
    // post-install notes are all data in the distro registry rather than
    // sentences on the screen — so they are read by key. Every branch of the
    // first three is enumerated in `osBuilder.test.ts`, and the notes are
    // checked against both catalogues by `distro-registry-keys.test.ts`, which
    // is the check this scan cannot make.
    // 61 → 64 (the card builder): a build's warnings and the reason the Build
    // button is off are `Phrase`s from `@/lib/cardBuild` — the kinds are Rust
    // enums, and putting their sentences on this side is what keeps them in
    // the user's language (ART-060). Every variant of both mappers is
    // enumerated in `phrase-keys.test.ts`.
    // 64 → 70 (G7's manifest, then G8's health report): the verdict line,
    // every check, every manifest finding under it, and each step only the
    // user can take are `Phrase`s from `@/lib/cardBuild`, mapped from Rust
    // enums for the same reason the build warnings are.
    // `phrase-keys.test.ts` enumerates every variant of all four mappers.
    // The state word beside each check (`…state.pass`) is the one genuinely
    // computed key here, and its three values are a closed TypeScript union.
    // 70 → 71 (G15): what a dropped file becomes on this card is a `Phrase`
    // too, and `phrase-keys.test.ts` enumerates its five variants.
    // 71 → 72: the health section's second button reads one of two literal
    // keys depending on whether a check is running, the same shape the
    // sidebar toggle uses.
    // 72 → 78 (SD-2 G3's preload screen): two `Phrase` reads — the reason the
    // preload cannot run yet and the sentence for each planned step, both
    // mapped from Rust shapes in `@/lib/preload` and both enumerated in
    // `phrase-keys.test.ts` — plus four one-of-two-literal-keys ternaries
    // (probing, previewing, running, and "no card yet" vs "this card has no
    // Amiga partition"), the same shape the sidebar toggle and the health
    // section's button already use.
    // 78 → 84 (SD-2 G11's layout screen, Task 7): a row's `kindPhrase` and a
    // refusal's `refusalPhrase`, both `Phrase` reads from `@/lib/layout` and
    // both enumerated in `phrase-keys.test.ts` — plus `layoutBlocker`'s
    // `Phrase`, read twice (the disabled Apply button's `title` and the
    // sentence beside it, the same double-read `buildBlocker` already uses in
    // `CardBuilder.tsx`) — plus two one-of-two-literal-keys ternaries
    // (Preview running/idle, Apply running/idle), the same shape every other
    // busy-button on this list already uses.
    // 84 → 89 (SD-2 G5's install screen, Task 13): `osinstallBlocker`'s and a
    // refusal's `refusalPhrase`, both `Phrase` reads from `@/lib/osinstall`
    // (`osinstallRefusalPhrase`'s seven variants are enumerated in
    // `phrase-keys.test.ts`, same as every other refusal on this list) —
    // plus three one-of-two-literal-keys ternaries (Build running/idle,
    // Verify running/idle, and the verify report's own Verified/Not
    // verified line), the same shape every other busy-button and outcome
    // badge on this list already uses.
    // 89 → 91 (ART-120, native by default with `hst-imager` a named
    // fallback): the preload result panel now lists every step with the
    // tool that ran it, so `stepPhrase` is read a second time — once for the
    // plan preview, once for the result — plus `fallbackPhrase`, a `Phrase`
    // read from `@/lib/preload` for the step(s) where the fallback fired.
    // Both are enumerated in `phrase-keys.test.ts`.
    // 91 → 92 (ART-120 fix wave, finding 3): the plan preview also reads
    // `plannedToolPhrase` beside each step, so the confirmation screen says
    // which writer is expected to run it *before* the user confirms, not
    // only after the fact in the result panel. Enumerated in
    // `phrase-keys.test.ts`.
    // 92 → 93 (ART-125): the result panel's "copied in" line is a `Phrase`
    // now rather than one literal key, because a copy `hst-imager` performed
    // has no byte total to print — `copiedPhrase` picks the sentence with
    // the bytes or the one without, and both are enumerated in
    // `phrase-keys.test.ts`.
    // 93 → 94 (SD-2 G9): the preload screen reads `pairingPhrase` above the
    // confirmation checkbox — whether the card's Kickstart suits the tree
    // about to go onto it. A `Phrase` read from `@/lib/preload`, and all six
    // `Pairing` variants (five phrases plus the deliberate `null` for
    // `paired`) are enumerated in `phrase-keys.test.ts`.
    // 94 → 95 (SD-2 G10): the Collection screen marks any value the index
    // *guessed* rather than read, and names the source in the mark's tooltip
    // — `t(provenancePhrase(from).key)`. One read, and the closed set behind
    // it is checked twice: `gameindex.test.ts` pins `ALL_PROVENANCES` to the
    // four variants `core::gameindex::record::Provenance` defines, and
    // `phrase-keys.test.ts` proves each of the four resolves to a real leaf.
    // 95 → 98 (Collection wave B): three reads across two screens, all of
    // artwork `Phrase`s. Settings names each source beside its switch;
    // the Collection names it again beside what that source managed, and
    // reads `outcomePhrase` for the sentence itself — which of two it is
    // depends on whether the source could be reached at all, so it cannot
    // be a literal. `sourcePhrase`'s three arms (both shipped ids and the
    // default, for a settings file naming a source this ART dropped) and
    // `outcomePhrase`'s two are enumerated in `phrase-keys.test.ts`.
    // 98 → 99 (ART-138): the ROM library's checksum badge. Which of four
    // sentences it is — OK, CRC ERR, "not a Kickstart", "encrypted" — is
    // `checksumBadge`'s decision in `@/lib/rom`, because the whole point of
    // the fix is that ART must not call a file damaged when it has no
    // checksum to check. Its four keys are enumerated in
    // `phrase-keys.test.ts`.
    // 99 → 100 (Collection wave C, Task 4): the new detail panel reads its
    // media line — floppy count, hardfile name or WHDLoad slave — from
    // `mediaPhrase()` in `@/lib/collectionDetail` (Task 3), the same `Phrase`
    // read `t(media.key, media.params)` every other mapper on this list
    // uses. All three arms are enumerated in `phrase-keys.test.ts`
    // (`mediaPhrase: every medium resolves`).
    // 100 → 101 (Collection wave C, Task 6b): the detail panel's picture
    // switch labels each button with `t(kindPhrase(kind).key)` —
    // `kindPhrase()` in `@/lib/collectionDetail`, the same `Phrase` shape
    // `mediaPhrase` above already uses. Its five variants are enumerated in
    // `phrase-keys.test.ts` (`collectionDetail's kindPhrase: every ArtKind
    // resolves`).
    // 101 → 106 (Collection wave C, Task 11 — Play): the detail panel's new
    // Play section reads five `Phrase`s from `@/lib/launch` — the machine
    // picker shared between the global default and the per-title override
    // (`t(machinePhrase(option).key)`, one source occurrence doing the work
    // of two thanks to the shared `MachineChoiceButtons` component), the
    // refusal shown instead of a confirmation, the machine named inside the
    // "will use" sentence, what a settled plan will mount, and the note
    // beside it when a title has more disks than WinUAE has drives. All four
    // mappers are enumerated in `phrase-keys.test.ts`.
    // 106 → 108 (Task 11 fix round, finding 3): `Settings.tsx`'s new Play
    // section (`launch.romDir`/`launch.defaultMachine`/`launch.systemVolume`,
    // the same remembered keys `TitleDetail`'s panel already uses) labels its
    // A500/A1200 `<select>` options with `t(machinePhrase("a500").key)` and
    // `t(machinePhrase("a1200").key)` — two literal arguments to
    // `machinePhrase`, but the call is still `t(...)` over a `.key` read
    // rather than a string literal, so each counts as its own dynamic site
    // the same way `MachineChoiceButtons`' single shared occurrence already
    // does above. `machinePhrase`'s two variants are enumerated in
    // `phrase-keys.test.ts` (`launch machinePhrase: every Machine resolves`).
    // 108 → 109 (whole-branch review, finding 3): the confirmation screen did
    // not say what a plan would mount or whether it could be written to
    // (design §4.4). `TitleDetail.tsx` gained one more site,
    // `t(mountNotePhrase(mount).key, mountNotePhrase(mount).params)`, reading
    // `mountNotePhrase()` from `@/lib/launch` the same way the note above it
    // reads `notePhrase()`. Its three variants are enumerated in
    // `phrase-keys.test.ts` (`launch mountNotePhrase: every MountNote variant
    // resolves`).
    // 109 → 111 (Task 7, content layer — the packages screen):
    // `PackagePanel.tsx` gained two dynamic sites. One reads
    // `t(phrase.key, phrase.params)` from `collisionPhrase()` in
    // `@/lib/osinstall`, the same `Phrase` shape every other mapper here
    // reads — its four variants are enumerated in `phrase-keys.test.ts`
    // (`collisionPhrase: every Collision variant resolves`). The other is
    // `t(busy ? "osinstall.packages.apply.running" : "osinstall.packages.apply.run")`,
    // the same ternary-over-two-literals shape `OsInstall.tsx`'s own run
    // button already uses (`t(busy ? "osinstall.run.running" : "osinstall.run.run")`)
    // — the `t(` is not immediately followed by a quote, so the regex above
    // counts it as dynamic even though both branches are literal strings.
    // 111 → 113 (Task 7's own fix round, F1/F2): `PackagePanel.tsx` gained
    // two more. `t(collisionGroupHeadingKey(group.kind), { count: … })`
    // reads a key from a small switch in `@/lib/osinstall`, the same shape
    // `kindPhrase`/`machinePhrase` above already use for a key-only (no
    // `Phrase` object) mapper — its four `Collision["kind"]` values are
    // exhaustive by the function's own return-type switch, so there is
    // nothing further to enumerate in `phrase-keys.test.ts`. The other is a
    // second `t(phrase.key, phrase.params)` site, now reading `refusalPhrase()`
    // for the refused-selection list (F2) rather than only `collisionPhrase()` —
    // its own variants are already enumerated in `phrase-keys.test.ts`
    // (`osinstall refusalPhrase: every RefusalReason variant resolves`).
    // 113 → 114 (final whole-branch review, M3): `PackagePanel.tsx` gained
    // one more, `t(hostPlacementBlockKey(pkg.hostPlacementBlock))` — the
    // sentence explaining why a package cannot be placed from the host at
    // all (ART-166). Key-only, from a `switch` over `HostPlacementBlock` in
    // `@/lib/osinstall` that is exhaustive by its own return type, and its
    // single value today is enumerated in `phrase-keys.test.ts`
    // (`hostPlacementBlockKey: every HostPlacementBlock resolves`).
    // 114 → 115 (ART-119 #2, debt-wave-c2): `OsInstall.tsx` gained one,
    // `t(reasonText.phrase.key, reasonText.phrase.params)`, and *lost four
    // literal ones* in the same edit — the four independent `&&` guards that
    // rendered a conditional component's reason are now one exhaustive
    // `switch` in `@/lib/osinstall` (`conditionalReasonText`), so a fifth
    // `ConditionalReason` kind is a compile error rather than a row with no
    // explanation on it. Its four variants are enumerated in
    // `phrase-keys.test.ts` (`conditionalReasonText: every ConditionalReason
    // variant resolves`), which is what makes trading four literal sites for
    // one dynamic one a net gain rather than a loss of coverage.
    // 115 → 116 (ART-143, debt-wave-c2): `CollectionStudio.tsx` gained one,
    // `t(rebindProblemPhrase(miss.problem).key)` — why a picture the user
    // attached by hand could not be put back after the artwork cache was
    // deleted. Key-only, from a `switch` over `RebindProblem` in
    // `@/lib/artwork` that is exhaustive by its own `never` fallthrough, and
    // its four values are enumerated in `phrase-keys.test.ts`
    // (`rebindProblemPhrase: every RebindProblem resolves`).
    // 116 → 118 (ART-175, debt-wave-c2): `OsInstall.tsx` gained two, both in
    // the new "what this would replace" section and both the same shapes
    // `PackagePanel.tsx` already uses for the identical rows —
    // `t(collisionGroupHeadingKey(group.kind), { count })` and
    // `t(phrase.key, phrase.params)` over `collisionPhrase`. Their variants
    // are already enumerated in `phrase-keys.test.ts` (`collisionPhrase:
    // every Collision variant resolves`), and `collisionGroupHeadingKey` is
    // exhaustive by its own return type.
    // 118 → 120 (ART-080, debt-wave-c2 fix round 1): `FileManager.tsx` gained
    // two, and they are one sentence between them —
    // `t(said.key, { ...said.params, target: t(said.targetPhrase.key) })`,
    // the message after a host delete. It is two `t(` calls because the
    // destination is itself a catalogue key: `src/lib` has no translator to
    // resolve a nested one with (`@/lib/phrase`'s `PartialPhrase` shape,
    // applied by hand for one parameter). Both mappers are enumerated in
    // `src/lib/hostDelete.test.ts` — `describeHostDelete`'s four sentences and
    // `recycleTargetPhrase`'s one target — which also asserts the catalogue
    // string really interpolates `{{target}}` rather than the parameter being
    // carried and never used.
    // 120 → 126 (the Amiga-side install round, task 6):
    // `AmigaInstallPanel.tsx` is six of them, and five are the point of the
    // screen rather than an accident of it. `t(outcome.key, …)`,
    // `t(nextStep.key)` and `t(settlement.key, …)` render the four endings a
    // run can have — succeeded, the installer refused, the deadline expired,
    // the window was closed — as four different sentences with four
    // different next steps, which is exactly what three separate defects in
    // that round were filed for collapsing; every variant of all three
    // mappers is enumerated in `phrase-keys.test.ts` (`amigainstall: every
    // ending, every settlement, every blocker resolves`) and asserted
    // *distinct* in `src/lib/amigainstall.test.ts`. `t(blocker.key, …)` and
    // `t(overlayAdvice.key, …)` are `readinessBlockers` and
    // `overlayAdvicePhrase`, enumerated in the same place. The sixth is the
    // run button's `t(busy ? … : …)`, the same shape `PackagePanel.tsx`
    // already uses for its own.
    expect(dynamicCalls).toBe(126);
  });
});
