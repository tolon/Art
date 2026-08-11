// Row icons for the Total Commander-styled Files screen (task 6b): a folder
// glyph, a generic file glyph, and an archive glyph for the handful of
// extensions ART actually knows how to open as one.
//
// Hand-drawn inline SVG, deliberately simple shapes "in the same spirit"
// rather than a copy of Total Commander's own icon set or anything from
// Workbench — ART ships no third-party content (spec, and this task's own
// brief), and pulling in an icon font/library is explicitly out of scope
// here too. No icon dependency, nothing traced from a screenshot.

import type { PanelEntry } from "@/lib/panel";
import { splitName } from "@/lib/panelName";

/** Extensions ART can open as an archive — visual only, independent of
 * `@/lib/archives`'s `isArchivePath` (which exists for the narrower "can F5
 * install this as a WHDLoad drawer" question, `.lha` only). This is purely
 * "which icon looks right", so it is not shared with that check. */
const ARCHIVE_EXTENSIONS = new Set(["lha", "zip", "7z", "adf", "hdf"]);

export type TcIconKind = "folder" | "archive" | "file";

/** Which icon a row gets: folder for a directory, archive for the handful of
 * extensions ART opens as one, a generic file otherwise. */
export function iconKindFor(entry: Pick<PanelEntry, "name" | "is_dir">): TcIconKind {
  if (entry.is_dir) return "folder";
  const { ext } = splitName(entry.name, false);
  return ARCHIVE_EXTENSIONS.has(ext.toLowerCase()) ? "archive" : "file";
}

const ICON_SIZE = 14;

function FolderGlyph() {
  return (
    <svg width={ICON_SIZE} height={ICON_SIZE} viewBox="0 0 16 16" aria-hidden="true" focusable="false">
      <path
        d="M1 3.5c0-.55.45-1 1-1h3.5l1.2 1.4H14c.55 0 1 .45 1 1V12c0 .55-.45 1-1 1H2c-.55 0-1-.45-1-1V3.5Z"
        fill="currentColor"
        opacity="0.85"
      />
    </svg>
  );
}

function FileGlyph() {
  return (
    <svg width={ICON_SIZE} height={ICON_SIZE} viewBox="0 0 16 16" aria-hidden="true" focusable="false">
      <path
        d="M3 1.5h6.2L13 5.3V14c0 .28-.22.5-.5.5h-9a.5.5 0 0 1-.5-.5v-12c0-.28.22-.5.5-.5Z"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.1"
      />
      <path d="M9.2 1.5V5h3.6" fill="none" stroke="currentColor" strokeWidth="1.1" />
    </svg>
  );
}

function ArchiveGlyph() {
  return (
    <svg width={ICON_SIZE} height={ICON_SIZE} viewBox="0 0 16 16" aria-hidden="true" focusable="false">
      <path
        d="M3 1.5h6.2L13 5.3V14c0 .28-.22.5-.5.5h-9a.5.5 0 0 1-.5-.5v-12c0-.28.22-.5.5-.5Z"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.1"
      />
      <path d="M9.2 1.5V5h3.6" fill="none" stroke="currentColor" strokeWidth="1.1" />
      {/* The zip-stripe: a dashed spine down the middle of the sheet is the
          one thing that reads as "archive" rather than "plain file" without
          borrowing anyone else's icon shape. */}
      <path d="M7.4 5.4v7" stroke="currentColor" strokeWidth="1.1" strokeDasharray="1.4 1.2" />
    </svg>
  );
}

export function TcRowIcon({ entry }: { entry: Pick<PanelEntry, "name" | "is_dir"> }) {
  const kind = iconKindFor(entry);
  if (kind === "folder") return <FolderGlyph />;
  if (kind === "archive") return <ArchiveGlyph />;
  return <FileGlyph />;
}
