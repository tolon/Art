// What ART currently has open, per kind of thing (ART-085).
//
// The defect this exists for: every studio held its open file in a local
// `useState`, so leaving the screen unmounted the component and took the file
// with it. Coming back gave the "open an .adf to begin" page again — while the
// Dashboard's Recent list, which *is* persisted, still showed the file that had
// been open a second earlier. That contrast is what made it read as a fault
// rather than as a design.
//
// **Session-scoped, deliberately.** This is not `@/lib/remembered` and does not
// reach `settings.json`: closing ART forgets what was open, and the next run
// starts empty. A path that outlives the run is a path that can name a file
// somebody has since deleted, moved, or unplugged with the drive it was on —
// and answering for that is a bigger design than the one this issue asked for.
// The choices a studio *makes* (its view mode, its filesystem, its folder) are
// still `useRemembered`'s job and still survive a restart; what is open is only
// as durable as the session.
//
// **Per kind, not one global object.** The studios address different kinds of
// thing: opening an ADF must not change what the Hard Disk studio is looking
// at. One entry each, so a user who has an ADF and an HDF on the go keeps both.
//
// Only the path is held. Nothing parsed: a studio re-reads its file when it
// comes back, which costs an ADF-sized read and is the only version of this
// that cannot show stale contents for a file that changed on disk meanwhile.

import { create } from "zustand";

/**
 * A kind of thing a screen can have open.
 *
 * A union rather than a free string so a typo is a compile error instead of a
 * second, silently empty entry.
 */
export type OpenKind =
  | "adf"
  | "harddisk"
  | "lha"
  | "hex"
  // WHDLoad installs from an archive *into* an image: two objects, and losing
  // either half way through setting the job up is the same annoyance twice.
  | "whdload-archive"
  | "whdload-image"
  // WinUAE's slots are separate from the studios' own images on purpose:
  // opening an HDF to look at it must not silently change what the emulator
  // would launch.
  | "winuae-floppy"
  | "winuae-harddisk";

interface OpenObjectState {
  /** The open path per kind. Absent and `null` mean the same thing: nothing. */
  open: Partial<Record<OpenKind, string | null>>;
  setOpen: (kind: OpenKind, path: string | null) => void;
}

export const useOpenObjectStore = create<OpenObjectState>((set) => ({
  open: {},
  setOpen: (kind, path) => set((s) => ({ open: { ...s.open, [kind]: path } })),
}));

/**
 * Reset everything — **for tests**, which share one module instance and would
 * otherwise leak one case's open file into the next.
 *
 * Not called by the application: there is no "close everything" the user can
 * ask for, and if there ever is, it should be a deliberate command rather than
 * this.
 */
export function resetOpenObjects(): void {
  useOpenObjectStore.setState({ open: {} });
}

/**
 * The open path for one kind, and a setter — a drop-in for the
 * `useState<string | null>(null)` every studio used to hold.
 *
 * The setter goes through the store rather than a closure over the rendered
 * value, so two screens mounted at once cannot overwrite each other with a
 * stale copy.
 */
export function useOpenObject(kind: OpenKind): [string | null, (next: string | null) => void] {
  const path = useOpenObjectStore((s) => s.open[kind] ?? null);
  const setOpen = useOpenObjectStore((s) => s.setOpen);

  const set = (next: string | null) => setOpen(kind, next);

  return [path, set];
}
