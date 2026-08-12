// What a pane's source combo offers, and which of its options is the one
// currently open (brief §1.3).
//
// Total Commander's pane header is a drive combo, a path and a filter box —
// `[Layout] DriveCombo=1, ButtonBar=0, DriveBar1=0` in the user's own config.
// ART's pane is not a Windows drive, though: it can hold a local folder, an
// ADF, an HDF partition, a disc, an archive or a Commodore image. So the combo
// carries both — the real, enumerated mounts *and* the six things ART can open
// — in one list, with the mounts first because that is what a path is usually
// switched to.
//
// Pure and self-contained, like `@/lib/selection`, `@/lib/sort` and
// `@/lib/mask` before it: `FileManager.tsx` calls Tauri commands on mount, so
// anything that can be decided without one is decided here instead, where a
// test can reach it.
//
// No label text lives here — options carry an i18n key, or the literal mount
// path where the label *is* the path (`D:\`). A `src/lib` helper never imports
// the i18n singleton (see `@/lib/phrase`).

/** The five image kinds a pane can be pointed at, plus a host folder. */
export type PaneImageKind = "adf" | "hdf" | "iso" | "archive" | "c64";

/** What picking an option asks the pane to do. */
export type PaneSourceChoice =
  /** Open an enumerated mount — `D:\` on Windows, `/` elsewhere. */
  | { kind: "root"; path: string }
  /** Ask for a host folder with the system picker. */
  | { kind: "folder" }
  /** Ask for a file of that kind with the system picker. */
  | { kind: "image"; image: PaneImageKind };

export interface PaneSourceOption {
  /**
   * The `<option>`'s value, and the only thing that travels through the DOM.
   * Parsed back by `parsePaneSource` rather than being an index into this
   * list: an index would silently point at a different source the moment the
   * enumerated mounts change underneath it (a card inserted, a share
   * disconnected).
   */
  value: string;
  choice: PaneSourceChoice;
  /** The i18n key for this option's label, or `null` when the label is the
   *  literal mount path in `literal` — a drive letter is not translated. */
  labelKey: string | null;
  literal: string | null;
}

const IMAGE_KINDS: Array<[PaneImageKind, string]> = [
  ["adf", "files.toolbar.adf"],
  ["hdf", "files.toolbar.hdf"],
  ["iso", "files.toolbar.disc"],
  ["archive", "files.toolbar.archive"],
  ["c64", "files.toolbar.c64"],
];

/**
 * Every source the combo offers: the enumerated mounts, then "Folder…", then
 * the five image kinds.
 *
 * `roots` comes from `panelLocalRoots` (`commands/panel.rs`), which probes for
 * drives that are actually there — the brief is explicit that no drive letter
 * is hardcoded, so an empty list here renders a combo with no mounts in it
 * rather than a fictional `C:\`.
 */
export function paneSourceOptions(roots: string[]): PaneSourceOption[] {
  return [
    ...roots.map(
      (path): PaneSourceOption => ({
        value: `root:${path}`,
        choice: { kind: "root", path },
        labelKey: null,
        literal: path,
      })
    ),
    {
      value: "folder",
      choice: { kind: "folder" },
      labelKey: "files.toolbar.folder",
      literal: null,
    },
    ...IMAGE_KINDS.map(
      ([image, labelKey]): PaneSourceOption => ({
        value: `image:${image}`,
        choice: { kind: "image", image },
        labelKey,
        literal: null,
      })
    ),
  ];
}

/**
 * Turn an option's `value` back into what it opens, or `null` when it is not
 * one of ours.
 *
 * `null` rather than a thrown error or a guessed default: the placeholder
 * option the combo shows when a pane holds something no mount covers (see
 * `currentPaneSourceValue`) has an empty value, and selecting nothing must do
 * nothing rather than navigate somewhere the user did not ask for.
 */
export function parsePaneSource(value: string): PaneSourceChoice | null {
  if (value === "folder") return { kind: "folder" };

  if (value.startsWith("root:")) {
    const path = value.slice("root:".length);
    return path === "" ? null : { kind: "root", path };
  }

  if (value.startsWith("image:")) {
    const image = value.slice("image:".length);
    return IMAGE_KINDS.some(([kind]) => kind === image)
      ? { kind: "image", image: image as PaneImageKind }
      : null;
  }

  return null;
}

/**
 * Which option the combo should show as selected for a pane.
 *
 * An image pane names its kind. A local pane names the mount its path is
 * under — the longest matching one, and case-insensitively, because Windows
 * hands back `D:\Projects` for a `D:\` root and a user may well have typed
 * `d:\`. A pane holding a folder under no enumerated mount (a UNC share, say)
 * gets `""`: the combo then shows its own placeholder rather than claiming
 * a drive the path is not on.
 */
export function currentPaneSourceValue(
  kind: "local" | PaneImageKind,
  location: string,
  roots: string[]
): string {
  if (kind !== "local") return `image:${kind}`;

  const lower = location.toLowerCase();
  const match = roots
    .filter((root) => lower.startsWith(root.toLowerCase()))
    .sort((a, b) => b.length - a.length)[0];

  return match ? `root:${match}` : "";
}
