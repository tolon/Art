//! Grid arithmetic for laying out icons inside an AmigaDOS drawer window —
//! where each icon's `Gadget` position goes, given the icons' rendered sizes
//! and names. Pure arithmetic: no filesystem, no [`crate::core::amigaicon`],
//! no I/O, no [`crate::core::CoreError`]. That is what makes it testable with
//! three numbers, and it is why the plan keeps it separate from the module
//! that reads and writes `.info` files.
//!
//! # The constants, field by field: adopted, or ART-specific
//!
//! Every numeric constant below except one is **adopted from Emu68 Hatcher**
//! (`rootrootde/emu68hatcher`, MIT — `docs/superpowers/notes/2026-09-05-emu68hatcher-teardown.md`),
//! not measured by ART against real icons the way `core/amigaicon`'s rendered-size
//! rule was. The same honesty `core/ilbm`'s `BMHD` doc applies to its header
//! applies here: this module states which of these ART has evidence for, and
//! which it does not. **None of these numbers have been checked against a
//! real, rendered Workbench window** — they are adopted from Hatcher's own
//! source, not measured by ART the way `core/amigaicon`'s rendered-size rule
//! was against 798 real icons. That remains true even where the composition
//! below is cited to an exact source line.
//!
//! - **`TOPAZ_CHAR_WIDTH = 8`** — adopted. Topaz, AmigaOS's default system
//!   font, is fixed-width at 8 pixels per character. Used to turn a drawer
//!   entry's *name* into a pixel width, because a `.info`'s `Gadget` size
//!   describes the icon image, not the label Workbench draws under it.
//! - **`EMBOSS = 3`** — adopted, formula included. Found in
//!   `builder/staging/icon_grid.py` (Emu68 Hatcher):
//!   ```text
//!   27:  _EMBOSS = 3  # 3D frame workbench draws around the bitmap (IControl default)
//!   173: return _Icon(path, do_type, width + _EMBOSS, height + _EMBOSS, png=True)   # frameless
//!   192: pad = _EMBOSS * 2 if framed else _EMBOSS
//!   ```
//!   So the padding a cell's *icon* carries, in each dimension, is
//!   `2 * EMBOSS` when the icon is framed and `EMBOSS` when it is not — never
//!   an offset on the icon's own placed `(x, y)`, only on the footprint used
//!   to size columns, rows and the window (confirmed by this module's own
//!   `EMBOSS`-difference test, below). This was first written up as "ART's
//!   own reasoned choice" because the round's initial read of Hatcher found
//!   only the teardown note, which names the *value* but not the formula,
//!   and looked for the composition in the grid-layout module
//!   (`staging/icon_grid.py`'s own grid-arranging code) rather than in the
//!   per-icon *sizing* function the three lines above actually live in. A
//!   later review supplied the exact lines; this doc — and [`Cell::framed`]
//!   — is the correction, not a fresh guess.
//! - **`MARGIN_X = 10`, `MARGIN_Y = 4`** — adopted: the inset from the
//!   window's inner edge to the first icon, asymmetric across vs down.
//! - **`COLUMN_GAP = 8`** — adopted: horizontal pixels between two columns'
//!   footprints.
//! - **`ROW_PAD = 18`** — adopted: vertical pixels reserved below a row's
//!   tallest (already emboss-padded) icon for its label line (one line of
//!   Topaz plus breathing room).
//! - **`TARGET_ASPECT = 2.0`** — adopted from iTidy, a widely-used Workbench
//!   icon-tidying tool: the width:height ratio a "tidied" icon grid aims
//!   for. Used only to choose a column count; it is a preference, not a
//!   constraint, and [`arrange`] will land on a different aspect when the
//!   width budget forces it.
//! - **`DRAWER_INNER_WIDTH = 520`** — adopted: the inner width budget an
//!   ordinary drawer window is given.
//! - **`ROOT_INNER_WIDTH = 420`** — **not adopted; ART-specific.** The `SYS:`
//!   window's actual geometry lives in the volume's own `disk.info`, a file
//!   ART does not write (`core/osinstall`'s design doc, §2.1: a `Disk.info`
//!   is placed only when a recipe names it explicitly, never inferred). So
//!   the root grid cannot know the real window width the way a drawer's own
//!   `DrawerData.NewWindow` can be read and matched — it has to fit a
//!   *narrower*, safer budget instead. 420 rather than 520 is the number
//!   this round carries forward from Hatcher's own root-grid budget, kept
//!   distinct from the drawer one specifically because the reason the two
//!   differ is ART's own (no `disk.info` to read), not Hatcher's.
//!
//! # What "cell" means
//!
//! A cell's icon footprint, in each dimension, is its rendered size plus
//! [`EMBOSS`] padding — doubled when [`Cell::framed`] is `true`, single when
//! it is not (Hatcher's own rule; see the `EMBOSS` entry above). **Only
//! after** that padding is applied does the width footprint get compared
//! against the label: a cell's final width is the wider of its padded icon
//! and its label at [`TOPAZ_CHAR_WIDTH`] pixels per character — a
//! 15-character name is 120px, wider than a typical 46px icon even after
//! padding, and a grid that sizes columns from the image alone overlaps
//! every label in the row. The padding never touches the icon's own placed
//! `(x, y)` — a single cell always lands exactly at `(MARGIN_X, MARGIN_Y)`,
//! never at `(MARGIN_X + EMBOSS, ...)`; it exists only to keep one icon's
//! frame clear of its neighbours' columns, rows and the window's own edge.
//!
//! `Cell::framed` is exactly the field `core::amigaicon`'s Task 3 added as
//! `Rendered::framed` (set from the ColorIcon `FACE` chunk's frameless bit,
//! `true` otherwise) — this module is that field's reader. A caller (Task 6)
//! is expected to copy `Rendered::framed` straight into `Cell::framed`
//! rather than leaving it at some fixed default.
//!
//! # Degenerate inputs
//!
//! This module never panics and never divides by zero, regardless of what a
//! caller hands it:
//!
//! - **No cells** — [`arrange`] returns no placements and a `(0, 0)` window.
//!   There is nothing to lay out and nothing to reserve room for.
//! - **One cell** — the "choose a column count closest to the target aspect"
//!   step never runs (there is only one possible column count: one), so it
//!   lands at the margin origin with no aspect reasoning involved.
//! - **`inner_width` narrower than the margins, or `0`** — the usable budget
//!   (`inner_width` minus both margins) saturates to `0` rather than
//!   underflowing. The shrink loop still runs and still stops at the column
//!   floor; it simply never finds a column count that fits, and the result
//!   overflows the (impossibly small) budget rather than looping forever or
//!   panicking. A caller asking for an inner width smaller than one icon's
//!   footprint has asked for something no layout can satisfy; this module
//!   reports its best attempt rather than refusing, since it has no
//!   `CoreError` to refuse *with*.
//! - **A cell wider than the whole budget** — same answer: the shrink loop
//!   bottoms out at the column floor and the placement overflows. Nothing
//!   indexes out of range and nothing multiplies or adds without a
//!   saturating guard, so the overflow is a wrong-looking layout, never a
//!   crash.
//! - **An extremely long label** — pixel width is computed in `u64` and
//!   saturated back into `u32` before use, so a label with millions of
//!   characters cannot wrap a width calculation around to a small number.
//! - **A window bigger than `i16` can hold** — real Workbench window
//!   geometry cannot get anywhere near this, but the conversion at the end
//!   of [`arrange`] saturates to `i16::MAX` rather than wrapping or
//!   panicking.

/// Pixels per character in Topaz, AmigaOS's default system font. Adopted
/// from Emu68 Hatcher — see the module doc.
const TOPAZ_CHAR_WIDTH: u32 = 8;

/// Pixels of padding for the 3D frame IControl draws around a selected
/// icon. Adopted from Emu68 Hatcher, formula included — see the module doc
/// and [`icon_pad`]: a framed icon is padded `2 * EMBOSS` in each dimension,
/// a frameless one `EMBOSS`.
const EMBOSS: u32 = 3;

/// Horizontal inset from the window's inner edge to the first column.
/// Adopted from Emu68 Hatcher.
const MARGIN_X: u32 = 10;

/// Vertical inset from the window's inner edge to the first row. Adopted
/// from Emu68 Hatcher.
const MARGIN_Y: u32 = 4;

/// Horizontal pixels between two adjacent columns' footprints. Adopted from
/// Emu68 Hatcher.
const COLUMN_GAP: u32 = 8;

/// Vertical pixels reserved below a row's tallest (already emboss-padded)
/// icon for its label line. Adopted from Emu68 Hatcher.
const ROW_PAD: u32 = 18;

/// The width:height ratio [`arrange`] prefers when choosing a column count,
/// before the shrink loop can override it to fit the budget. Adopted from
/// iTidy by way of Emu68 Hatcher.
const TARGET_ASPECT: f64 = 2.0;

/// Inner width budget for an ordinary drawer window. Adopted from Emu68
/// Hatcher — see the module doc.
pub const DRAWER_INNER_WIDTH: u16 = 520;

/// Inner width budget for the root (`SYS:`) window. **Not adopted** — this
/// is narrower than [`DRAWER_INNER_WIDTH`] for a reason specific to ART, not
/// to Hatcher: the real `SYS:` window geometry lives in the volume's own
/// `disk.info`, which ART does not write. See the module doc.
pub const ROOT_INNER_WIDTH: u16 = 420;

/// One icon to be placed: its rendered size (see
/// `core::amigaicon`'s rendered-size rule) and the name Workbench will
/// print under it. `is_container` distinguishes a drawer/volume icon from a
/// plain file icon, since drawers sort before files in the layout this
/// module produces. `framed` is `core::amigaicon`'s `Rendered::framed`
/// (Task 3) carried straight through — see the module doc's `EMBOSS` entry
/// for why it changes a cell's footprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub label: String,
    pub width: u16,
    pub height: u16,
    pub is_container: bool,
    pub framed: bool,
}

/// Where one input [`Cell`] lands. `index` is the cell's position in the
/// slice [`arrange`] was given — never a position in some internal sorted
/// order — so a caller can zip a placement straight back to the icon file it
/// describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub index: usize,
    pub x: i32,
    pub y: i32,
}

/// The result of [`arrange`]: every placed cell, and the window size (as
/// `DrawerData.NewWindow` width/height are stored: signed 16-bit) that
/// contains them all with margins on every side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridResult {
    pub placements: Vec<Placement>,
    pub window: (i16, i16),
}

/// One sorted cell's pre-computed numbers: its original index (into the
/// slice [`arrange`] was given), its final width footprint (icon-vs-label,
/// already emboss-padded on the icon side), its raw rendered height (used
/// only for bottom-alignment within a row), and its emboss-padded icon
/// height (used only for row/window sizing).
type SortedCell = (usize, u32, u32, u32);

/// The measurements of a candidate column count: how wide each column is,
/// how tall each row is (the tallest raw icon height, for bottom-aligning;
/// and the tallest padded-icon-plus-label-pad height, for spacing rows),
/// and the grid's total footprint.
struct GridShape {
    col_widths: Vec<u32>,
    row_icon_heights: Vec<u32>,
    row_footprint_heights: Vec<u32>,
    total_width: u32,
    total_height: u32,
}

/// A label's width in Topaz pixels, saturating rather than overflowing for
/// a pathologically long name.
fn label_px_width(label: &str) -> u32 {
    let chars = label.chars().count() as u64;
    chars
        .saturating_mul(u64::from(TOPAZ_CHAR_WIDTH))
        .min(u64::from(u32::MAX)) as u32
}

/// The emboss padding added to an icon's own footprint, in each dimension:
/// `2 * EMBOSS` when framed, `EMBOSS` when not. Adopted from Emu68 Hatcher's
/// `builder/staging/icon_grid.py:192` — see the module doc's `EMBOSS` entry.
fn icon_pad(framed: bool) -> u32 {
    if framed {
        2 * EMBOSS
    } else {
        EMBOSS
    }
}

/// A cell's width footprint: its rendered width plus [`icon_pad`], then
/// widened to its label's pixel width if that label is wider still.
fn footprint_width(cell: &Cell) -> u32 {
    let icon = u32::from(cell.width).saturating_add(icon_pad(cell.framed));
    icon.max(label_px_width(&cell.label))
}

/// A cell's padded icon height: its rendered height plus [`icon_pad`]. Used
/// only for row spacing and window sizing — never for the bottom-alignment
/// offset, which uses the raw rendered height so that only real icon-height
/// differences shift a cell within its row.
fn padded_icon_height(cell: &Cell) -> u32 {
    u32::from(cell.height).saturating_add(icon_pad(cell.framed))
}

/// Lay `sorted` cells into a row-major grid of `columns` columns and report
/// its shape. `columns` is always at least 1 — never a division by zero.
fn shape_for(sorted: &[SortedCell], columns: usize) -> GridShape {
    debug_assert!(columns >= 1);
    let rows = sorted.len().div_ceil(columns);
    let mut col_widths = vec![0u32; columns];
    let mut row_icon_heights = vec![0u32; rows];
    let mut row_padded_heights = vec![0u32; rows];
    for (k, &(_, fw, raw_h, padded_h)) in sorted.iter().enumerate() {
        let col = k % columns;
        let row = k / columns;
        col_widths[col] = col_widths[col].max(fw);
        row_icon_heights[row] = row_icon_heights[row].max(raw_h);
        row_padded_heights[row] = row_padded_heights[row].max(padded_h);
    }
    let row_footprint_heights: Vec<u32> = row_padded_heights
        .iter()
        .map(|&h| h.saturating_add(ROW_PAD))
        .collect();
    let total_width = col_widths
        .iter()
        .fold(0u32, |acc, &w| acc.saturating_add(w))
        .saturating_add(COLUMN_GAP.saturating_mul(columns.saturating_sub(1) as u32));
    let total_height = row_footprint_heights
        .iter()
        .fold(0u32, |acc, &h| acc.saturating_add(h));
    GridShape {
        col_widths,
        row_icon_heights,
        row_footprint_heights,
        total_width,
        total_height,
    }
}

/// Choose the column count in `min_columns..=max_columns` whose grid shape
/// lands closest to [`TARGET_ASPECT`], ignoring the width budget — the
/// budget is enforced afterwards by shrinking. Ties keep the first (lowest)
/// column count found, so the search is deterministic.
fn choose_columns(sorted: &[SortedCell], min_columns: usize, max_columns: usize) -> usize {
    let mut best = min_columns;
    let mut best_diff = f64::INFINITY;
    for columns in min_columns..=max_columns {
        let shape = shape_for(sorted, columns);
        // total_height is always > 0: every row carries ROW_PAD (18) even
        // when every cell's rendered height is 0, so this never divides by
        // zero.
        let aspect = f64::from(shape.total_width) / f64::from(shape.total_height);
        let diff = (aspect - TARGET_ASPECT).abs();
        if diff < best_diff {
            best_diff = diff;
            best = columns;
        }
    }
    best
}

/// Arrange `cells` into a grid that fits within `inner_width`: drawers
/// (`is_container: true`) first, each group alphabetical by label; rows
/// bottom-aligned so a row's labels share a baseline; columns chosen to land
/// near [`TARGET_ASPECT`] and then shrunk — never below two columns, unless
/// there is only one cell — until the grid fits the budget.
///
/// Pure: no filesystem, no `core::amigaicon`, no I/O. See the module doc for
/// exactly what every degenerate input (no cells, one cell, a budget smaller
/// than a single icon) is defined to mean.
pub fn arrange(cells: &[Cell], inner_width: u16) -> GridResult {
    if cells.is_empty() {
        return GridResult {
            placements: Vec::new(),
            window: (0, 0),
        };
    }

    // Containers first, then alphabetical within each group. Stable sort
    // keeps equal-key cells in their original relative order.
    let mut order: Vec<usize> = (0..cells.len()).collect();
    order.sort_by(|&a, &b| {
        let ca = &cells[a];
        let cb = &cells[b];
        let key_a = (!ca.is_container, ca.label.to_ascii_lowercase());
        let key_b = (!cb.is_container, cb.label.to_ascii_lowercase());
        key_a.cmp(&key_b)
    });

    let sorted: Vec<SortedCell> = order
        .iter()
        .map(|&i| {
            let cell = &cells[i];
            (
                i,
                footprint_width(cell),
                u32::from(cell.height),
                padded_icon_height(cell),
            )
        })
        .collect();

    let n = sorted.len();
    let min_columns = if n == 1 { 1 } else { 2 };
    let max_columns = n;

    let mut columns = choose_columns(&sorted, min_columns, max_columns);
    let available = u32::from(inner_width).saturating_sub(2 * MARGIN_X);
    // Shrink until the grid's total width fits the budget, or until the
    // column floor is reached. If even the floor doesn't fit (an
    // impossibly small inner_width, or a cell wider than the budget), this
    // loop stops at the floor and the caller gets an overflowing layout
    // rather than an infinite loop — see the module doc's "Degenerate
    // inputs" section.
    while columns > min_columns && shape_for(&sorted, columns).total_width > available {
        columns -= 1;
    }

    let shape = shape_for(&sorted, columns);

    let mut x_col = vec![0u32; columns];
    x_col[0] = MARGIN_X;
    for c in 1..columns {
        x_col[c] = x_col[c - 1]
            .saturating_add(shape.col_widths[c - 1])
            .saturating_add(COLUMN_GAP);
    }

    let rows = shape.row_footprint_heights.len();
    let mut y_row = vec![0u32; rows];
    y_row[0] = MARGIN_Y;
    for r in 1..rows {
        y_row[r] = y_row[r - 1].saturating_add(shape.row_footprint_heights[r - 1]);
    }

    let placements = sorted
        .iter()
        .enumerate()
        .map(|(k, &(original_index, _fw, raw_height, _padded_height))| {
            let col = k % columns;
            let row = k / columns;
            let x = x_col[col];
            // Bottom-align: row_icon_heights[row] is the tallest *raw*
            // rendered icon in this row, always >= this cell's own raw
            // height, so the subtraction below never underflows.
            let y = y_row[row].saturating_add(shape.row_icon_heights[row] - raw_height);
            Placement {
                index: original_index,
                x: x as i32,
                y: y as i32,
            }
        })
        .collect();

    let window_width = (2 * MARGIN_X).saturating_add(shape.total_width);
    let window_height = (2 * MARGIN_Y).saturating_add(shape.total_height);
    let window = (
        window_width.min(i16::MAX as u32) as i16,
        window_height.min(i16::MAX as u32) as i16,
    );

    GridResult { placements, window }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cell_is_as_wide_as_its_label_when_the_label_is_wider_than_the_image() {
        // Topaz is 8px per character, so "AReallyLongName" (15 chars) is 120px -
        // wider than a 46px icon. A grid using the image width overlaps labels.
        let cells = vec![
            Cell {
                label: "AReallyLongName".into(),
                width: 46,
                height: 46,
                is_container: false,
                framed: true,
            },
            Cell {
                label: "X".into(),
                width: 46,
                height: 46,
                is_container: false,
                framed: true,
            },
        ];
        let g = arrange(&cells, DRAWER_INNER_WIDTH);
        assert!(
            g.placements[1].x >= 120,
            "the second column clears the first label"
        );
    }

    #[test]
    fn containers_come_before_everything_else_and_each_group_is_alphabetical() {
        let cells = vec![
            Cell {
                label: "Zeta".into(),
                width: 20,
                height: 20,
                is_container: false,
                framed: true,
            },
            Cell {
                label: "Beta".into(),
                width: 20,
                height: 20,
                is_container: false,
                framed: true,
            },
            Cell {
                label: "MDrawer".into(),
                width: 20,
                height: 20,
                is_container: true,
                framed: true,
            },
            Cell {
                label: "Alpha_drawer".into(),
                width: 20,
                height: 20,
                is_container: true,
                framed: true,
            },
        ];
        let g = arrange(&cells, DRAWER_INNER_WIDTH);
        let order: Vec<&str> = g
            .placements
            .iter()
            .map(|p| cells[p.index].label.as_str())
            .collect();
        assert_eq!(
            order,
            vec!["Alpha_drawer", "MDrawer", "Beta", "Zeta"],
            "containers first (alphabetical), then files (alphabetical)"
        );
    }

    #[test]
    fn nothing_is_placed_outside_the_inner_width() {
        // Every x plus its cell width stays within the budget, for 1..40 cells.
        for n in 1..40usize {
            let cells: Vec<Cell> = (0..n)
                .map(|i| Cell {
                    label: format!("N{i}"),
                    width: 20 + (i % 40) as u16,
                    height: 20 + (i % 30) as u16,
                    is_container: i % 5 == 0,
                    framed: i % 2 == 0,
                })
                .collect();
            let g = arrange(&cells, DRAWER_INNER_WIDTH);
            for p in &g.placements {
                let w = i32::from(cells[p.index].width);
                assert!(
                    p.x + w <= i32::from(DRAWER_INNER_WIDTH),
                    "n={n}: placement at x={} width={} overflows budget {}",
                    p.x,
                    w,
                    DRAWER_INNER_WIDTH
                );
            }
        }
    }

    #[test]
    fn rows_are_bottom_aligned_so_labels_share_a_baseline() {
        // Two cells of different heights in one row must end at the same y + height.
        let cells = vec![
            Cell {
                label: "A".into(),
                width: 20,
                height: 20,
                is_container: false,
                framed: true,
            },
            Cell {
                label: "B".into(),
                width: 20,
                height: 46,
                is_container: false,
                framed: true,
            },
        ];
        let g = arrange(&cells, DRAWER_INNER_WIDTH);
        assert_eq!(g.placements.len(), 2);
        let bottom = |p: &Placement| p.y + i32::from(cells[p.index].height);
        assert_eq!(
            bottom(&g.placements[0]),
            bottom(&g.placements[1]),
            "both cells' labels sit on the same baseline"
        );
    }

    #[test]
    fn a_single_cell_lands_at_the_margin_and_the_window_contains_it() {
        // "sane" cannot fail, so name the property: one placement, at the margin
        // origin, and a window at least as large as the cell plus its margins.
        let cells = vec![Cell {
            label: "One".into(),
            width: 46,
            height: 46,
            is_container: false,
            framed: true,
        }];
        let g = arrange(&cells, DRAWER_INNER_WIDTH);
        assert_eq!(g.placements.len(), 1);
        assert_eq!((g.placements[0].x, g.placements[0].y), (10, 4));
        assert!(
            g.window.0 as i32 >= 10 + 46 + 10,
            "the window contains the cell"
        );
    }

    #[test]
    fn the_column_count_shrinks_until_the_row_fits() {
        // Cells wide enough that the aspect-ratio choice would overflow.
        // Six 200x200 cells: the aspect-nearest column count alone (4)
        // needs 848px, far past the 520px drawer budget; only the shrink
        // loop brings it down to something that fits.
        let cells: Vec<Cell> = (0..6)
            .map(|i| Cell {
                label: format!("C{i}"),
                width: 200,
                height: 200,
                is_container: false,
                framed: true,
            })
            .collect();
        let g = arrange(&cells, DRAWER_INNER_WIDTH);
        for p in &g.placements {
            let w = i32::from(cells[p.index].width);
            assert!(
                p.x + w <= i32::from(DRAWER_INNER_WIDTH),
                "placement at x={} width={} overflows the 520px budget",
                p.x,
                w
            );
        }
    }

    #[test]
    fn arranging_no_cells_returns_no_placements_and_does_not_panic() {
        let g = arrange(&[], DRAWER_INNER_WIDTH);
        assert!(g.placements.is_empty());
    }

    #[test]
    fn a_zero_inner_width_does_not_panic_or_divide_by_zero() {
        let cells = vec![
            Cell {
                label: "A".into(),
                width: 46,
                height: 46,
                is_container: false,
                framed: true,
            },
            Cell {
                label: "B".into(),
                width: 46,
                height: 46,
                is_container: false,
                framed: true,
            },
        ];
        let g = arrange(&cells, 0);
        assert_eq!(g.placements.len(), 2);
    }

    #[test]
    fn a_framed_icon_takes_three_more_pixels_each_way_than_a_frameless_one() {
        // Adopted from Hatcher's icon_grid.py:192 - framed pads 2*EMBOSS,
        // frameless pads EMBOSS, on both dimensions. Label is short enough
        // ("Z", 8px) that it never dominates the width footprint, so the
        // whole difference is attributable to the icon's own padding.
        let framed = Cell {
            label: "Z".into(),
            width: 100,
            height: 100,
            is_container: false,
            framed: true,
        };
        let frameless = Cell {
            label: "Z".into(),
            width: 100,
            height: 100,
            is_container: false,
            framed: false,
        };
        assert_eq!(
            footprint_width(&framed) - footprint_width(&frameless),
            EMBOSS,
            "a framed cell's width footprint is exactly EMBOSS wider"
        );
        assert_eq!(
            padded_icon_height(&framed) - padded_icon_height(&frameless),
            EMBOSS,
            "a framed cell's padded icon height is exactly EMBOSS taller"
        );
    }
}
