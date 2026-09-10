// The commander's columns — the pure half of design § 5
// (`docs/superpowers/specs/2026-09-08-os-builder-simplification-design.md`),
// asked for again by the owner on 2026-09-10: *"ad uzantı boyut tarih vb
// aralıkları manuel ayarlanamıyor. Ayarlanabilmeli ve yapılan ayarı
// unutmamalı."*

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import {
  COLUMN_SPEC,
  COLUMN_WIDTH_MAX_EM,
  COLUMN_WIDTH_MIN_EM,
  DEFAULT_COLUMNS,
  gridTemplate,
  isColumnIdList,
  isColumnWidths,
  toggleColumn,
  visibleColumns,
  widthEmFromPx,
  withWidth,
  type ColumnLayout,
} from "@/lib/columns";
import { recallInto } from "@/lib/remembered";

/** The `.tc-row` template's fallback, read from the stylesheet itself. */
function stylesheetDefault(): string {
  const css = readFileSync(resolve(__dirname, "../pages/FileManager.css"), "utf-8");
  const rule = /\.tc-row \{[^}]*?grid-template-columns:\s*var\(--tc-columns,\s*([^;]+)\);/s.exec(css);
  if (!rule) throw new Error("`.tc-row` no longer reads `var(--tc-columns, …)`");
  // The fallback ends at the `var(` call's own closing parenthesis.
  return rule[1].replace(/\)\s*$/, "").trim();
}

/** A pane `em` wide at 12 px text, in pixels. */
const emWide = (em: number) => em * 12;

describe("the untouched screen", () => {
  it("renders the stylesheet's own template, read from the file", () => {
    const every = visibleColumns(DEFAULT_COLUMNS, emWide(60), 12);
    expect(gridTemplate(every, DEFAULT_COLUMNS)).toBe(stylesheetDefault());
    expect(stylesheetDefault()).toBe("minmax(0, 1fr) 4.3em 10.7em 9.8em 5.3em 4.8em");
  });
});

describe("hiding a column", () => {
  it("takes its track away, not only its cell", () => {
    const noDate = toggleColumn(DEFAULT_COLUMNS, "date");
    const ids = visibleColumns(noDate, emWide(60), 12);
    expect(ids).toEqual(["name", "ext", "size", "attr"]);
    expect(gridTemplate(ids, noDate)).toBe("minmax(0, 1fr) 4.3em 10.7em 5.3em 4.8em");
  });

  it("keeps the row's own order when a column comes back", () => {
    const back = toggleColumn(toggleColumn(DEFAULT_COLUMNS, "ext"), "ext");
    expect(visibleColumns(back, emWide(60), 12)).toEqual(["name", "ext", "size", "date", "attr"]);
  });

  it("never hides Name — a listing with no names is not a listing", () => {
    expect(toggleColumn(DEFAULT_COLUMNS, "name")).toEqual(DEFAULT_COLUMNS);
  });
});

describe("widths are em of the pane's own text", () => {
  it("converts a dragged pixel width at 10, 12 and 28 px text", () => {
    expect(widthEmFromPx(120, 12)).toBe(10);
    expect(widthEmFromPx(120, 10)).toBe(12);
    expect(widthEmFromPx(140, 28)).toBe(5);
    // Rounded to one decimal, the precision the stylesheet's own widths use.
    expect(widthEmFromPx(100, 12)).toBe(8.3);
  });

  it("will not let a column be dragged to nothing, or past any sense", () => {
    expect(widthEmFromPx(5, 12)).toBe(COLUMN_WIDTH_MIN_EM);
    expect(withWidth(DEFAULT_COLUMNS, "ext", 0.5).widthsEm.ext).toBe(COLUMN_WIDTH_MIN_EM);
    expect(withWidth(DEFAULT_COLUMNS, "ext", 500).widthsEm.ext).toBe(COLUMN_WIDTH_MAX_EM);
    expect(COLUMN_WIDTH_MIN_EM).toBe(2);
  });

  it("widens one column and leaves the others alone", () => {
    const wider = withWidth(DEFAULT_COLUMNS, "date", 14);
    expect(wider.widthsEm).toEqual({ ...DEFAULT_COLUMNS.widthsEm, date: 14 });
    expect(wider.shown).toEqual(DEFAULT_COLUMNS.shown);
  });
});

describe("a narrow pane", () => {
  it("hides Date and Attr, by name, and nothing else", () => {
    expect(visibleColumns(DEFAULT_COLUMNS, emWide(25), 12)).toEqual(["name", "ext", "size"]);
    // The user's own set is narrowed the same way — Ext stays hidden, Size stays.
    const noExt = toggleColumn(DEFAULT_COLUMNS, "ext");
    expect(visibleColumns(noExt, emWide(25), 12)).toEqual(["name", "size"]);
  });

  it("hides, never shrinks: the widths in the template are the user's", () => {
    const wide = withWidth(DEFAULT_COLUMNS, "size", 12);
    const ids = visibleColumns(wide, emWide(25), 12);
    expect(gridTemplate(ids, wide)).toBe("minmax(0, 1fr) 4.3em 12em 4.8em");
  });

  it("treats an unmeasured pane as wide, as `paneWidthClasses` does", () => {
    expect(visibleColumns(DEFAULT_COLUMNS, 0, 12)).toEqual(["name", "ext", "size", "date", "attr"]);
  });
});

describe("a hand-edited settings file cannot break the screen", () => {
  it("refuses a width nobody could want, and an id that is not a column", () => {
    expect(isColumnWidths({ ...DEFAULT_COLUMNS.widthsEm, date: 1e9 })).toBe(false);
    expect(isColumnWidths({ ...DEFAULT_COLUMNS.widthsEm, date: 1 })).toBe(false);
    expect(isColumnWidths({ ...DEFAULT_COLUMNS.widthsEm, date: Number.NaN })).toBe(false);
    expect(isColumnWidths({ ext: 4.3, size: 10.7, date: 9.8 })).toBe(false);
    expect(isColumnWidths(DEFAULT_COLUMNS.widthsEm)).toBe(true);

    expect(isColumnIdList(["grd"])).toBe(false);
    expect(isColumnIdList(["ext", "size"])).toBe(false);
    expect(isColumnIdList(["name", "name"])).toBe(false);
    expect(isColumnIdList(["name", "size"])).toBe(true);
  });

  it("costs a bad width map only the widths, not the user's shown columns", () => {
    const store = {
      "files.columns": { shown: ["name", "size"], widthsEm: { date: 1e9 } },
    };
    const layout = recallInto<ColumnLayout>(store, "files.columns", COLUMN_SPEC, DEFAULT_COLUMNS);
    expect(layout.shown).toEqual(["name", "size"]);
    expect(layout.widthsEm).toEqual(DEFAULT_COLUMNS.widthsEm);
  });
});
