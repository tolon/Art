// The commander's columns: which of Name · Ext · Size · Date · Attr the Files
// screen shows, and how wide each is (design § 5 of
// `docs/superpowers/specs/2026-09-08-os-builder-simplification-design.md`).
//
// Pure — no DOM, no i18next. `FileManager.tsx` reads one remembered object,
// `files.columns`, through `useRememberedShape`, asks this module which
// columns a pane of a given width shows and what its grid template is, and
// sets that template as `--tc-columns` beside `--tc-font-size`.
//
// **Widths are `em` of the listing's own text, never pixels.** The listing's
// text size is a setting (10-28 px) and the pane lives under `.app-shell`'s
// `zoom`; a remembered pixel width would break exactly the way ART-101 and
// ART-174 did. A drag measures pixels and stores `px / font`, the conversion
// `paneWidthInEm` already owns.
//
// **One column set for both panes**, the way Total Commander keeps its own
// columns: `.tc-row` is one class shared by both panes' headers and rows, and
// two panes side by side are a comparison (§ 5.2).

import { PANE_NARROW_BELOW_EM, paneWidthInEm } from "@/lib/dockLayout";
import type { Guard } from "@/lib/remembered";

/** Every column, in the row's own order. */
export const COLUMN_IDS = ["name", "ext", "size", "date", "attr"] as const;
export type ColumnId = (typeof COLUMN_IDS)[number];

/** The columns that carry a width. Name is `minmax(0, 1fr)` and takes what is
 *  left; "resizing" it would mean fixing it (§ 5.7). */
export type SizedColumnId = Exclude<ColumnId, "name">;
export const SIZED_COLUMN_IDS: readonly SizedColumnId[] = ["ext", "size", "date", "attr"];

/** The one remembered key. Its **absence** is the default screen. */
export const COLUMNS_KEY = "files.columns";

/** A column narrower than this is indistinguishable from a hidden one, and two
 *  states that look the same are one state to the person looking (§ 5.7). */
export const COLUMN_WIDTH_MIN_EM = 2;
/** Wider than this is not a preference but a broken screen — the same stated
 *  reason `isWholeNumberBetween` bounds a font size. */
export const COLUMN_WIDTH_MAX_EM = 40;

export interface ColumnLayout {
  /** Which columns show. Always holds `name`. */
  shown: ColumnId[];
  widthsEm: Record<SizedColumnId, number>;
}

/** `FileManager.css`'s own `.tc-row` template, as data. `columns.test.ts`
 *  reads the stylesheet and holds the two equal, so this cannot drift from it. */
export const DEFAULT_COLUMNS: ColumnLayout = {
  shown: [...COLUMN_IDS],
  widthsEm: { ext: 4.3, size: 10.7, date: 9.8, attr: 5.3 },
};

const NAME_TRACK = "minmax(0, 1fr)";
/** The trailing slot `.tc-cell-actions` was reserved for. Used by no row
 *  today (design § 1.6) and kept, because removing it changes the look of
 *  every row — not offered, not resizable. */
const ACTIONS_TRACK = "4.8em";
/** What a narrow pane gives up, in the order the stylesheet always has. */
const NARROW_HIDES: readonly ColumnId[] = ["date", "attr"];

function isColumnId(value: unknown): value is ColumnId {
  return typeof value === "string" && (COLUMN_IDS as readonly string[]).includes(value);
}

function isWidth(value: unknown): value is number {
  return (
    typeof value === "number" &&
    Number.isFinite(value) &&
    value >= COLUMN_WIDTH_MIN_EM &&
    value <= COLUMN_WIDTH_MAX_EM
  );
}

/** Known ids, none twice, Name among them. */
export const isColumnIdList: Guard<ColumnId[]> = (value: unknown): value is ColumnId[] =>
  Array.isArray(value) &&
  value.every(isColumnId) &&
  new Set(value).size === value.length &&
  value.includes("name");

/** A width for every sized column, each inside its bounds. A width stored
 *  for Name is not read by anything. */
export const isColumnWidths: Guard<Record<SizedColumnId, number>> = (
  value: unknown
): value is Record<SizedColumnId, number> =>
  typeof value === "object" &&
  value !== null &&
  !Array.isArray(value) &&
  SIZED_COLUMN_IDS.every((id) => isWidth((value as Record<string, unknown>)[id]));

/** Field by field, so a bad width map costs only the widths (§ 5.6). */
export const COLUMN_SPEC = { shown: isColumnIdList, widthsEm: isColumnWidths };

function clampWidth(em: number): number {
  return Math.max(COLUMN_WIDTH_MIN_EM, Math.min(COLUMN_WIDTH_MAX_EM, em));
}

function tenth(value: number): number {
  return Math.round(value * 10) / 10;
}

/**
 * The columns a pane `paneWidthPx` wide shows, in the row's order.
 *
 * **Narrow hides, it never shrinks**, and it hides the same two columns it
 * always has — Date and Attr — below `PANE_NARROW_BELOW_EM`. An unmeasured
 * pane (0 px) is wide, as `paneWidthClasses` has it. The answer is a view of
 * the user's set and is never written back (§ 5.5, ART-089).
 */
export function visibleColumns(
  chosen: ColumnLayout,
  paneWidthPx: number,
  paneFontPx: number
): ColumnId[] {
  const narrow =
    paneWidthPx > 0 && paneWidthInEm(paneWidthPx, paneFontPx) < PANE_NARROW_BELOW_EM;
  return COLUMN_IDS.filter(
    (id) =>
      (id === "name" || chosen.shown.includes(id)) && !(narrow && NARROW_HIDES.includes(id))
  );
}

/** The `grid-template-columns` for `ids`: Name flexible, each shown column at
 *  its own width, then the reserved trailing slot. */
export function gridTemplate(ids: readonly ColumnId[], chosen: ColumnLayout): string {
  const sized = ids
    .filter((id): id is SizedColumnId => id !== "name")
    .map((id) => `${chosen.widthsEm[id]}em`);
  return [NAME_TRACK, ...sized, ACTIONS_TRACK].join(" ");
}

/** A dragged pixel width as `em` of the pane's text, to one decimal, bounded. */
export function widthEmFromPx(px: number, paneFontPx: number): number {
  return clampWidth(tenth(paneWidthInEm(px, paneFontPx)));
}

/** `layout` with one column at `em`, bounded. */
export function withWidth(layout: ColumnLayout, id: SizedColumnId, em: number): ColumnLayout {
  return { ...layout, widthsEm: { ...layout.widthsEm, [id]: clampWidth(tenth(em)) } };
}

/** `layout` with `id` shown if it was hidden and hidden if it was shown, the
 *  row's order kept. Name is not hideable, so asking changes nothing. */
export function toggleColumn(layout: ColumnLayout, id: ColumnId): ColumnLayout {
  if (id === "name") return layout;
  const on = layout.shown.includes(id);
  const shown = COLUMN_IDS.filter((other) =>
    other === id ? !on : other === "name" || layout.shown.includes(other)
  );
  return { ...layout, shown };
}
