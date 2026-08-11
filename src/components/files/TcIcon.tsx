// Row icons for the Total Commander-styled Files screen (task 6b, folded in
// task 6b's second reference): a folder glyph, a generic file glyph, an
// archive glyph for the handful of extensions ART actually knows how to open
// as one, a hidden/system glyph, and a distinct up-arrow for the synthetic
// `[..]` row.
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
 * "which icon/colour looks right", so it is not shared with that check. */
const ARCHIVE_EXTENSIONS = new Set(["lha", "zip", "7z", "adf", "hdf"]);

/** Extensions the reference colours light blue/cyan — a short, fixed list
 * (task 6b's second brief: "three classes is what the reference shows", not
 * a colour per extension). */
const TEXT_EXTENSIONS = new Set(["txt", "md", "readme", "nfo", "doc"]);

/** Names the reference dims as hidden/system (`Desktop.ini`). `PanelEntry.attrs`
 * now carries real attribute bits (task attr), but the hidden/system
 * classification here is still name-based rather than reading them: a
 * leading dot (the Unix/most-editors convention for "hidden") or one of a
 * short list of well-known Windows system files. */
const HIDDEN_SYSTEM_NAMES = new Set(["desktop.ini", "thumbs.db", "ntuser.dat"]);

function isHiddenByName(name: string): boolean {
  return name.startsWith(".") || HIDDEN_SYSTEM_NAMES.has(name.toLowerCase());
}

export type TcFileClass = "dir" | "archive" | "hidden" | "text" | "plain";

/**
 * One classification, shared by the icon and the text colour — they are the
 * same judgement about what a row is, not two independent ones. A directory
 * short-circuits before any name/extension logic runs, the same way
 * `splitName` never grows an extension for one.
 */
export function classifyEntry(entry: Pick<PanelEntry, "name" | "is_dir">): TcFileClass {
  if (entry.is_dir) return "dir";
  if (isHiddenByName(entry.name)) return "hidden";

  const { ext } = splitName(entry.name, false);
  const lowerExt = ext.toLowerCase();
  if (ARCHIVE_EXTENSIONS.has(lowerExt)) return "archive";
  if (TEXT_EXTENSIONS.has(lowerExt)) return "text";
  return "plain";
}

export type TcIconKind = "folder" | "archive" | "hidden" | "file";

/** Which icon a row gets, from the shared classification. */
export function iconKindFor(entry: Pick<PanelEntry, "name" | "is_dir">): TcIconKind {
  switch (classifyEntry(entry)) {
    case "dir":
      return "folder";
    case "archive":
      return "archive";
    case "hidden":
      return "hidden";
    default:
      return "file";
  }
}

/**
 * The CSS custom property a row's *un-selected, non-cursor* text should use,
 * from the same classification: plain files and directories stay
 * `--tc-text` (white), text files get `--tc-text-file` (light blue/cyan),
 * hidden/system files get `--tc-text-hidden` (dimmed). Selection (red) and
 * the cursor (black-on-yellow) both take precedence over this in
 * `FileManager.tsx` — this is only the row's "otherwise" colour, the same
 * role `--tc-text` played before this classification existed.
 */
export function fileTextColorVar(entry: Pick<PanelEntry, "name" | "is_dir">): string {
  switch (classifyEntry(entry)) {
    case "text":
      return "var(--tc-text-file)";
    case "hidden":
      return "var(--tc-text-hidden)";
    default:
      return "var(--tc-text)";
  }
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

/** A page with a red exclamation mark — hidden/system. The mark is a fixed
 * warning red rather than `currentColor`, so it stays legible whatever the
 * row's text colour is doing (white, red-selected, or black-on-yellow). */
function HiddenGlyph() {
  return (
    <svg width={ICON_SIZE} height={ICON_SIZE} viewBox="0 0 16 16" aria-hidden="true" focusable="false">
      <path
        d="M3 1.5h6.2L13 5.3V14c0 .28-.22.5-.5.5h-9a.5.5 0 0 1-.5-.5v-12c0-.28.22-.5.5-.5Z"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.1"
        opacity="0.6"
      />
      <path d="M9.2 1.5V5h3.6" fill="none" stroke="currentColor" strokeWidth="1.1" opacity="0.6" />
      <path d="M8 5.5v3.2" stroke="#ff3b3b" strokeWidth="1.3" strokeLinecap="round" />
      <circle cx="8" cy="10.7" r="0.8" fill="#ff3b3b" />
    </svg>
  );
}

/** The synthetic `[..]` row's icon — a plain up-arrow, distinct from the
 * folder glyph so "go up" doesn't read as just another directory. Not
 * driven by `classifyEntry`/`iconKindFor`: `[..]` is chrome, not a
 * `PanelEntry` (see `FileManager.tsx`'s comment on that row). */
export function UpDirIcon() {
  return (
    <svg width={ICON_SIZE} height={ICON_SIZE} viewBox="0 0 16 16" aria-hidden="true" focusable="false">
      <path
        d="M8 13V3.4M4 6.8 8 2.8l4 4"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.4"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function TcRowIcon({ entry }: { entry: Pick<PanelEntry, "name" | "is_dir"> }) {
  switch (iconKindFor(entry)) {
    case "folder":
      return <FolderGlyph />;
    case "archive":
      return <ArchiveGlyph />;
    case "hidden":
      return <HiddenGlyph />;
    default:
      return <FileGlyph />;
  }
}
