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
    } else if (/\.(ts|tsx)$/.test(entry) && !entry.endsWith(".test.ts")) {
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
    expect(dynamicCalls).toBe(101);
  });
});
