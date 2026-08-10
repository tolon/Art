// Two-pane file manager, Norton Commander style.
//
// Each pane independently shows a local folder, an ADF image or an HDF image.
// Copying runs between panes, by button or by dragging, and a local file can be
// dragged out to Explorer.
//
// ## One kind of volume, two kinds of file
//
// An ADF is a bare volume; an HDF opens on its partition list and picking one
// browses inside it. Below that line they are the same thing, so every
// operation — copy, rename, delete, new folder, attributes — runs through the
// same commands whichever the pane holds. An ADF is simply volume 0.
//
// A partition ART cannot read — PFS3, SFS, long filenames — is still listed,
// with its size and the exact reason. Never "cannot see HDF": a user must be
// able to learn their disk is healthy even when ART cannot walk it. The same
// rule covers writing: a volume ART will not write to keeps its lock badge and
// the reason on hover, rather than a pane that quietly does nothing (§96).
//
// ## Function keys
//
// F3 View · F5 Copy · F6 Rename · F7 New folder · F8 Delete · F9 Attributes,
// on the keyboard and on a bar under the panes, because a key nobody knows
// about is a feature nobody has.

import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { startDrag } from "@crabnebula/tauri-plugin-drag";

import { AttributesDialog } from "@/components/files/AttributesDialog";
import { CheckoutPanel } from "@/components/files/CheckoutPanel";
import { CopyPlanDialog } from "@/components/files/CopyPlanDialog";
import { FileViewer } from "@/components/files/FileViewer";
import {
  FunctionKeyBar,
  useFunctionKeys,
  type FunctionAction,
} from "@/components/files/FunctionKeys";
import { adfOpen, type AdfInfo } from "@/lib/adf";
import { checkoutEdit, checkoutOpen, volumeIconFor } from "@/lib/checkout";
import { onJobProgress } from "@/lib/jobs";
import {
  formatBytes,
  panelListAdf,
  panelListLocal,
  panelLocalRoots,
  type PanelEntry,
} from "@/lib/panel";
import type { OverwritePolicy } from "@/lib/sources";
import {
  describeLayout,
  isMountable,
  volumeExtractTo,
  volumeList,
  volumeScan,
  type ImageVolumes,
} from "@/lib/volume";
import {
  describeCopy,
  onVolumeWriteResult,
  volumeCopyBetween,
  volumeCopyIn,
  volumeCopyOut,
  volumeDelete,
  volumeMakeDir,
  volumePlanCopy,
  volumePutFile,
  volumeRecover,
  volumeRename,
  volumeWriteCapability,
  type CopyPlan,
  type CopyReport,
  type WriteCapability,
} from "@/lib/volumeWrite";
import { usePowerMode } from "@/lib/uxmode";

type PaneKind = "local" | "adf" | "hdf";
type Side = "left" | "right";

/** One pane's state. */
interface PaneState {
  kind: PaneKind;
  /** Local folder path, or the image file path. */
  location: string;
  /** Image panes: the directory being shown. */
  dirBlock: number | null;
  /** Image panes: folders walked into, so "up" can go back. */
  trail: Array<{ name: string; block: number | null }>;
  entries: PanelEntry[];
  parent: string | null;
  truncated: boolean;
  adf: AdfInfo | null;
  /** HDF: what the image holds. */
  image: ImageVolumes | null;
  /**
   * Which volume is open, or null while choosing one.
   *
   * An ADF is a bare volume, so it is index 0 — which is what lets every write
   * go through the same commands whether the pane holds a floppy or a
   * partition two gigabytes into a hard disk.
   */
  volumeIndex: number | null;
  volumeName: string;
  warnings: string[];
  /** Whether ART can write here, and what the footer shows (§8). */
  capability: WriteCapability | null;
  error: string | null;
}

function emptyPane(): PaneState {
  return {
    kind: "local",
    location: "",
    dirBlock: null,
    trail: [],
    entries: [],
    parent: null,
    truncated: false,
    adf: null,
    image: null,
    volumeIndex: null,
    volumeName: "",
    warnings: [],
    capability: null,
    error: null,
  };
}

/**
 * Whether this pane holds a volume that can be written to.
 *
 * Both halves matter: an image pane with no volume open (an HDF still showing
 * its partition list) and a volume ART refuses to write (dircache, PFS3, or
 * one with an unfinished operation waiting).
 */
function writableVolume(
  state: PaneState
): { path: string; volumeIndex: number; dirBlock: number | null } | null {
  if (state.kind === "local" || state.volumeIndex === null) return null;
  if (!state.capability?.writable) return null;
  return {
    path: state.location,
    volumeIndex: state.volumeIndex,
    dirBlock: state.dirBlock,
  };
}

/** Why writing is unavailable in this pane, for a disabled key's tooltip. */
function writeRefusal(state: PaneState, t: (key: string) => string): string {
  if (state.kind === "local") return t("files.writeRefusal.local");
  if (state.volumeIndex === null) return t("files.writeRefusal.noPartition");
  return state.capability?.reason ?? t("files.writeRefusal.default");
}

type TranslateFn = (key: string, params?: Record<string, string | number>) => string;

/**
 * How to describe a copy's outcome, in one line.
 *
 * `describeCopy` (in `@/lib/volumeWrite`) has no translator to render or join
 * the "N files and N folders" clause with, so it returns only the outer
 * sentence key; this resolves `what` here, where a translator is available,
 * and supplies it as the missing param.
 */
function copyResultText(report: CopyReport, t: TranslateFn): string {
  const hasFiles = report.files_copied > 0;
  const hasDirs = report.directories_created > 0;
  const what = !hasFiles && !hasDirs
    ? t("files.status.copyResult.nothing")
    : hasFiles && hasDirs
    ? t("files.status.copyResult.filesCount", { count: report.files_copied }) +
      t("files.status.copyResult.andFoldersCount", { count: report.directories_created })
    : hasFiles
    ? t("files.status.copyResult.filesCount", { count: report.files_copied })
    : t("files.status.copyResult.foldersCount", { count: report.directories_created });

  const phrase = describeCopy(report);
  return t(phrase.key, { ...phrase.params, what });
}

export function FileManager() {
  const { t } = useTranslation();
  const powerMode = usePowerMode();
  const [left, setLeft] = useState<PaneState>(emptyPane());
  const [right, setRight] = useState<PaneState>(emptyPane());
  const [roots, setRoots] = useState<string[]>([]);
  const [selection, setSelection] = useState<Record<Side, string | null>>({
    left: null,
    right: null,
  });
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  /** What to do about names already taken. Sticky across operations. */
  const [policy, setPolicy] = useState<OverwritePolicy>("skip");
  /** The pre-flight report, while the user is deciding about it. */
  const [plan, setPlan] = useState<{
    plan: CopyPlan;
    source: string;
    side: Side;
    name: string;
  } | null>(null);
  const [viewing, setViewing] = useState<{
    path: string;
    volumeIndex: number;
    entryBlock: number;
    name: string;
  } | null>(null);
  const [attributes, setAttributes] = useState<{
    path: string;
    volumeIndex: number;
    entryBlock: number;
  } | null>(null);
  /** An unfinished operation the user has been offered a recovery for. */
  const [recovery, setRecovery] = useState<{
    side: Side;
    path: string;
    description: string;
  } | null>(null);

  // Which copy job this screen is waiting on, and where to refresh when it
  // lands. Refs rather than state: the listener below is registered once, and
  // re-subscribing on every id change would race with the async unlisten.
  const pendingCopy = useRef<number | null>(null);
  const copyDestination = useRef<Side | null>(null);

  const setPane = useCallback(
    (side: Side, next: PaneState) => (side === "left" ? setLeft(next) : setRight(next)),
    []
  );
  const pane = useCallback((side: Side) => (side === "left" ? left : right), [left, right]);

  useEffect(() => {
    panelLocalRoots()
      .then(async (found) => {
        setRoots(found);
        if (found[0]) {
          const listing = await panelListLocal(found[0]);
          const base = {
            ...emptyPane(),
            location: listing.path,
            entries: listing.entries,
            parent: listing.parent,
            truncated: listing.truncated,
          };
          setLeft(base);
          setRight({ ...base });
        }
      })
      .catch((e) => setError(String(e)));
  }, []);

  // ---- opening things ----

  const openLocal = useCallback(
    async (side: Side, path: string) => {
      try {
        const listing = await panelListLocal(path);
        setPane(side, {
          ...emptyPane(),
          kind: "local",
          location: listing.path,
          entries: listing.entries,
          parent: listing.parent,
          truncated: listing.truncated,
        });
        setSelection((current) => ({ ...current, [side]: null }));
      } catch (e) {
        setPane(side, { ...emptyPane(), location: path, error: String(e) });
      }
    },
    [setPane]
  );

  const openAdf = useCallback(
    async (side: Side, path: string, dirBlock: number | null, trail: PaneState["trail"]) => {
      try {
        const [info, entries, capability] = await Promise.all([
          adfOpen(path),
          panelListAdf(path, dirBlock),
          // An ADF is a bare volume at index 0, so the same capability report
          // serves it as serves a partition — free space, filesystem, and any
          // unfinished operation waiting to be undone.
          volumeWriteCapability(path, 0).catch(() => null),
        ]);
        setPane(side, {
          ...emptyPane(),
          kind: "adf",
          location: path,
          dirBlock,
          trail,
          entries,
          adf: info,
          volumeIndex: 0,
          volumeName: capability?.volume_name ?? info.volume_name ?? "",
          capability,
        });
        setSelection((current) => ({ ...current, [side]: null }));
      } catch (e) {
        setPane(side, { ...emptyPane(), kind: "adf", location: path, error: String(e) });
      }
    },
    [setPane]
  );

  /** Open an HDF on its partition list. */
  const openHdf = useCallback(
    async (side: Side, path: string) => {
      try {
        const image = await volumeScan(path);
        setPane(side, { ...emptyPane(), kind: "hdf", location: path, image });
        setSelection((current) => ({ ...current, [side]: null }));
      } catch (e) {
        setPane(side, { ...emptyPane(), kind: "hdf", location: path, error: String(e) });
      }
    },
    [setPane]
  );

  /** Browse inside one partition of an already-scanned image. */
  const openVolume = useCallback(
    async (
      side: Side,
      path: string,
      image: ImageVolumes,
      volumeIndex: number,
      dirBlock: number | null,
      trail: PaneState["trail"]
    ) => {
      try {
        const [listing, capability] = await Promise.all([
          volumeList(path, volumeIndex, dirBlock),
          volumeWriteCapability(path, volumeIndex).catch(() => null),
        ]);
        setPane(side, {
          ...emptyPane(),
          kind: "hdf",
          location: path,
          image,
          volumeIndex,
          volumeName: listing.volume_name,
          warnings: listing.warnings,
          capability,
          dirBlock: dirBlock ?? listing.root_block,
          trail,
          entries: listing.entries,
        });
        setSelection((current) => ({ ...current, [side]: null }));
      } catch (e) {
        setPane(side, {
          ...emptyPane(),
          kind: "hdf",
          location: path,
          image,
          error: String(e),
        });
      }
    },
    [setPane]
  );

  async function chooseImage(side: Side, kind: "adf" | "hdf") {
    const picked = await open({
      multiple: false,
      filters:
        kind === "adf"
          ? [{ name: "Amiga floppy image", extensions: ["adf"] }]
          : [{ name: "Amiga hard disk image", extensions: ["hdf", "hda", "img"] }],
    });
    if (typeof picked !== "string") return;
    if (kind === "adf") await openAdf(side, picked, null, []);
    else await openHdf(side, picked);
  }

  async function chooseFolder(side: Side) {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked === "string") await openLocal(side, picked);
  }

  /** Double-click on a row. */
  async function activate(side: Side, entry: PanelEntry) {
    const state = pane(side);
    if (!entry.is_dir) return;

    if (state.kind === "local" && entry.path) {
      await openLocal(side, entry.path);
    } else if (state.kind === "adf" && entry.header_block !== null) {
      await openAdf(side, state.location, entry.header_block, [
        ...state.trail,
        { name: entry.name, block: state.dirBlock },
      ]);
    } else if (
      state.kind === "hdf" &&
      state.image &&
      state.volumeIndex !== null &&
      entry.header_block !== null
    ) {
      await openVolume(side, state.location, state.image, state.volumeIndex, entry.header_block, [
        ...state.trail,
        { name: entry.name, block: state.dirBlock },
      ]);
    }
  }

  async function goUp(side: Side) {
    const state = pane(side);

    if (state.kind === "local" && state.parent) {
      await openLocal(side, state.parent);
    } else if (state.kind === "adf" && state.trail.length > 0) {
      const trail = [...state.trail];
      const previous = trail.pop()!;
      await openAdf(side, state.location, previous.block, trail);
    } else if (state.kind === "hdf" && state.volumeIndex !== null && state.image) {
      if (state.trail.length > 0) {
        const trail = [...state.trail];
        const previous = trail.pop()!;
        await openVolume(
          side,
          state.location,
          state.image,
          state.volumeIndex,
          previous.block,
          trail
        );
      } else {
        // Out of the partition, back to the list of them.
        await openHdf(side, state.location);
      }
    }
  }

  const refresh = useCallback(
    async (side: Side) => {
      const state = pane(side);
      if (state.kind === "local" && state.location) {
        await openLocal(side, state.location);
      } else if (state.kind === "adf") {
        await openAdf(side, state.location, state.dirBlock, state.trail);
      } else if (state.kind === "hdf") {
        if (state.image && state.volumeIndex !== null) {
          await openVolume(
            side,
            state.location,
            state.image,
            state.volumeIndex,
            state.dirBlock,
            state.trail
          );
        } else {
          await openHdf(side, state.location);
        }
      }
    },
    [pane, openLocal, openAdf, openHdf, openVolume]
  );

  // A copy job's result arrives here (§54). One listener, registered once.
  useEffect(() => {
    const unlisten = onVolumeWriteResult((result) => {
      if (result.job_id !== pendingCopy.current) return;
      pendingCopy.current = null;
      setBusy(null);

      if (result.kind === "copy_in") {
        setMessage(copyResultText(result.report, t));
        if (result.report.skipped.length > 0) {
          setError(result.report.skipped.slice(0, 3).join(" · "));
        }
      } else {
        const { report } = result;
        setMessage(
          report.sidecars_written > 0
            ? t("files.status.filesWrittenOutSidecars", {
                count: report.files_written,
                sidecars: report.sidecars_written,
              })
            : t("files.status.filesWrittenOut", { count: report.files_written })
        );
        if (report.skipped.length > 0) {
          setError(report.skipped.slice(0, 3).join(" · "));
        }
      }

      const side = copyDestination.current;
      copyDestination.current = null;
      if (side) void refresh(side);
    });

    return () => {
      void unlisten.then((off) => off());
    };
  }, [refresh, t]);

  // A job that fails or is cancelled emits no result, so the listener above
  // alone would leave this screen waiting forever. The job's own terminal
  // state is what says "stop waiting", and it carries the error id (§68).
  useEffect(() => {
    const unlisten = onJobProgress((job) => {
      if (job.state.state === "running") return;
      if (job.id !== pendingCopy.current) return;

      pendingCopy.current = null;
      setBusy(null);
      if (job.state.state === "failed") {
        setError(`${job.state.message} (${job.state.error_code})`);
      }
      const side = copyDestination.current;
      copyDestination.current = null;
      if (side) void refresh(side);
    });

    return () => {
      void unlisten.then((off) => off());
    };
  }, [refresh]);

  // ---- copying ----

  /**
   * F5 — copy `entry` from one pane into the other.
   *
   * All four directions now work, because a volume is a volume whether it came
   * from a floppy or a partition two gigabytes into a hard disk:
   *
   * ```text
   * folder → volume    planned first, then one journalled write per file
   * volume → folder    with .uaem sidecars for what NTFS cannot hold
   * volume → volume    staged through a temp folder, verified at both ends
   * folder → folder    refused; that is what Explorer is for
   * ```
   */
  const copyTo = useCallback(
    async (from: Side, entry: PanelEntry) => {
      const to: Side = from === "left" ? "right" : "left";
      const source = pane(from);
      const target = pane(to);

      setError(null);
      setMessage(null);

      if (source.kind === "local" && target.kind === "local") {
        setError(t("files.err.bothLocal"));
        return;
      }

      // ---- into a volume ----
      if (target.kind !== "local") {
        const destination = writableVolume(target);
        if (!destination) {
          setError(writeRefusal(target, t));
          return;
        }

        // From another volume: staged through a temp folder, so both halves
        // are the paths already tested on their own (§4.3).
        if (source.kind !== "local") {
          if (source.volumeIndex === null) {
            setError(t("files.err.openOtherSide"));
            return;
          }
          if (source.location === destination.path) {
            setError(t("files.err.sameImage"));
            return;
          }
          setBusy(t("files.status.copyingBetween", { name: entry.name }));
          try {
            pendingCopy.current = await volumeCopyBetween(
              source.location,
              source.volumeIndex,
              entry.is_dir ? entry.header_block : source.dirBlock,
              destination.path,
              destination.volumeIndex,
              destination.dirBlock,
              { overwrite: policy, sidecars: powerMode }
            );
            copyDestination.current = to;
          } catch (e) {
            setError(String(e));
            setBusy(null);
          }
          return;
        }

        // From the user's disk. A folder is planned and confirmed first; a
        // single file goes straight in.
        if (!entry.path) {
          setError(t("files.err.noLocalPath"));
          return;
        }

        if (entry.is_dir) {
          setBusy(t("files.status.planning", { name: entry.name }));
          try {
            const found = await volumePlanCopy(
              destination.path,
              destination.volumeIndex,
              destination.dirBlock,
              entry.path
            );
            setPlan({ plan: found, source: entry.path, side: to, name: entry.name });
          } catch (e) {
            setError(String(e));
          } finally {
            setBusy(null);
          }
          return;
        }

        setBusy(t("files.status.copying", { name: entry.name }));
        try {
          const outcome = await volumePutFile(
            destination.path,
            destination.volumeIndex,
            destination.dirBlock,
            entry.path,
            entry.name,
            policy
          );
          setMessage(
            outcome.backup
              ? t("files.status.writtenToBackedUp", { name: entry.name, volume: target.volumeName })
              : t("files.status.writtenTo", { name: entry.name, volume: target.volumeName })
          );
          await refresh(to);
        } catch (e) {
          setError(String(e));
        } finally {
          setBusy(null);
        }
        return;
      }

      // ---- out of a volume ----
      if (source.volumeIndex === null) {
        setError(t("files.err.openPartitionFirst"));
        return;
      }

      if (entry.is_dir) {
        if (entry.header_block === null) return;
        setBusy(t("files.status.copyingOut", { name: entry.name }));
        try {
          pendingCopy.current = await volumeCopyOut(
            source.location,
            source.volumeIndex,
            entry.header_block,
            `${target.location}/${entry.name}`,
            { overwrite: policy, sidecars: powerMode }
          );
          copyDestination.current = to;
        } catch (e) {
          setError(String(e));
          setBusy(null);
        }
        return;
      }

      if (entry.header_block === null) return;
      setBusy(t("files.status.copying", { name: entry.name }));
      try {
        const outcome = await volumeExtractTo(
          source.location,
          source.volumeIndex,
          entry.header_block,
          target.location
        );
        setMessage(
          outcome.skipped_existing
            ? t("files.status.alreadyThere", { name: entry.name })
            : t("files.status.copiedOut", {
                name: entry.name,
                volume: source.volumeName,
                size: formatBytes(outcome.bytes),
              })
        );
        await refresh(to);
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(null);
      }
    },
    [pane, refresh, policy, powerMode, t]
  );

  /** Run the copy the plan dialog just had confirmed. */
  async function runPlannedCopy() {
    const pending = plan;
    if (!pending) return;

    const target = writableVolume(pane(pending.side));
    setPlan(null);
    if (!target) {
      setError(writeRefusal(pane(pending.side), t));
      return;
    }

    setBusy(t("files.status.copying", { name: pending.name }));
    try {
      pendingCopy.current = await volumeCopyIn(
        target.path,
        target.volumeIndex,
        target.dirBlock,
        pending.source,
        { overwrite: policy, sidecars: powerMode }
      );
      copyDestination.current = pending.side;
    } catch (e) {
      setError(String(e));
      setBusy(null);
    }
  }

  /**
   * F8 — delete an entry from a volume.
   *
   * `Destructive` (§63), so it confirms twice: once for the act and once
   * naming what is about to go. The second confirmation carries the name
   * because the first one is the reflex the user has already learned to click
   * through.
   */
  async function deleteEntry(side: Side, entry: PanelEntry) {
    const state = pane(side);
    const target = writableVolume(state);
    if (!target || entry.header_block === null) return;

    if (!window.confirm(t("files.dialog.delete.confirm1", { name: entry.name, volume: state.volumeName }))) {
      return;
    }
    if (!window.confirm(t("files.dialog.delete.confirm2", { name: entry.name }))) {
      return;
    }

    setError(null);
    setBusy(t("files.status.deleting", { name: entry.name }));
    try {
      // Its icon, if it has one: an orphan `.info` left behind clutters
      // Workbench with an icon that opens nothing (§7.1).
      const icon = await volumeIconFor(
        target.path,
        target.volumeIndex,
        target.dirBlock,
        entry.name
      ).catch(() => null);

      const outcome = await volumeDelete(
        target.path,
        target.volumeIndex,
        target.dirBlock,
        entry.header_block
      );

      let alsoIcon = false;
      if (icon && window.confirm(t("files.dialog.delete.confirmIcon", { icon: icon.icon_name }))) {
        await volumeDelete(
          target.path,
          target.volumeIndex,
          target.dirBlock,
          icon.icon_block
        );
        alsoIcon = true;
      }

      setMessage(
        alsoIcon
          ? outcome.backup
            ? t("files.status.deletedWithIconBackedUp", { name: entry.name })
            : t("files.status.deletedWithIcon", { name: entry.name })
          : outcome.backup
            ? t("files.status.deletedBackedUp", { name: entry.name })
            : t("files.status.deleted", { name: entry.name })
      );
      await refresh(side);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  /**
   * F4 — check a file out for editing.
   *
   * The file comes out to a working copy and the user's editor opens it. The
   * panel below the panes is where it goes back in — deliberately not
   * automatic, because a write into a disk image is not something to do on a
   * file-save the user may not have finished (§6).
   */
  async function editEntry(side: Side, entry: PanelEntry) {
    const state = pane(side);
    const target = writableVolume(state);
    if (!target || entry.header_block === null || entry.is_dir) {
      setError(writeRefusal(state, t));
      return;
    }

    setError(null);
    setBusy(t("files.status.checkingOut", { name: entry.name }));
    try {
      const row = await checkoutOpen(
        target.path,
        target.volumeIndex,
        target.dirBlock,
        entry.header_block
      );
      setMessage(t("files.status.checkedOut", { name: row.name }));
      // Best-effort: an editor that will not start is worth saying, but the
      // checkout itself succeeded and its path is on screen either way.
      await checkoutEdit(row.id).catch((e) => setError(String(e)));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  /** F7 — create a folder inside a volume. */
  async function newFolder(side: Side) {
    const state = pane(side);
    const target = writableVolume(state);
    if (!target) {
      setError(writeRefusal(state, t));
      return;
    }

    const name = window.prompt(t("files.dialog.newFolder.prompt"));
    if (!name) return;

    setError(null);
    setBusy(t("files.status.creatingFolder"));
    try {
      await volumeMakeDir(target.path, target.volumeIndex, target.dirBlock, name);
      await refresh(side);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  /** F6 — rename an entry in place. */
  async function renameEntry(side: Side, entry: PanelEntry) {
    const state = pane(side);
    const target = writableVolume(state);
    if (!target || entry.header_block === null) {
      setError(writeRefusal(state, t));
      return;
    }

    const name = window.prompt(t("files.dialog.rename.prompt"), entry.name);
    if (!name || name === entry.name) return;

    setError(null);
    setBusy(t("files.status.renaming", { name: entry.name }));
    try {
      // §7.1: Workbench shows an object only when its `.info` is next to it,
      // so renaming `Game` without renaming `Game.info` makes the game
      // invisible on a real Amiga while looking fine here. Offered rather than
      // done silently — the user may have a reason.
      const icon = await volumeIconFor(
        target.path,
        target.volumeIndex,
        target.dirBlock,
        entry.name
      ).catch(() => null);

      await volumeRename(
        target.path,
        target.volumeIndex,
        target.dirBlock,
        entry.header_block,
        name
      );

      let alsoIcon = false;
      if (
        icon &&
        window.confirm(
          t("files.dialog.rename.confirmIcon", {
            name: entry.name,
            icon: icon.icon_name,
            newName: name,
          })
        )
      ) {
        await volumeRename(
          target.path,
          target.volumeIndex,
          target.dirBlock,
          icon.icon_block,
          `${name}.info`
        );
        alsoIcon = true;
      }

      setMessage(
        alsoIcon
          ? t("files.status.renamedWithIcon", { name: entry.name, newName: name })
          : t("files.status.renamed", { name: entry.name, newName: name })
      );
      await refresh(side);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  /** Drag a local file out to Explorer. */
  async function dragOut(entry: PanelEntry) {
    if (!entry.path || entry.is_dir) return;
    try {
      await startDrag({ item: [entry.path], icon: "" });
    } catch (e) {
      setError(String(e));
    }
  }

  function paneProps(side: Side) {
    const state = pane(side);
    return {
      side,
      state,
      roots,
      selected: selection[side],
      powerMode,
      onSelect: (name: string) => setSelection((s) => ({ ...s, [side]: name })),
      onActivate: (entry: PanelEntry) => void activate(side, entry),
      onUp: () => void goUp(side),
      onOpenFolder: () => void chooseFolder(side),
      onOpenImage: (kind: "adf" | "hdf") => void chooseImage(side, kind),
      onOpenRoot: (root: string) => void openLocal(side, root),
      onOpenVolume: (index: number) => {
        if (state.image) void openVolume(side, state.location, state.image, index, null, []);
      },
      onRefresh: () => void refresh(side),
      onCopy: (entry: PanelEntry) => void copyTo(side, entry),
      onDelete: (entry: PanelEntry) => void deleteEntry(side, entry),
      onNewFolder: () => void newFolder(side),
      onDragOut: (entry: PanelEntry) => void dragOut(entry),
      onDropped: (entry: PanelEntry, from: Side) => {
        if (from !== side) void copyTo(from, entry);
      },
    };
  }

  // ---- the function keys ----

  /**
   * The pane the keys act on.
   *
   * Whichever side has a selection. A commander normally tracks focus; this
   * uses the selection instead, which is the same thing said in a way that
   * survives a click on a button between the panes.
   */
  const active: Side = selection.left !== null ? "left" : "right";
  const activePane = pane(active);
  const selected = activePane.entries.find((e) => e.name === selection[active]) ?? null;
  const canWrite = writableVolume(activePane) !== null;
  const inVolume = activePane.kind !== "local" && activePane.volumeIndex !== null;

  const actions: FunctionAction[] = [
    {
      key: "F3",
      label: t("files.functionKeys.view"),
      enabled: Boolean(selected && !selected.is_dir && inVolume),
      reason: inVolume
        ? t("files.functionKeys.viewReasonSelect")
        : t("files.functionKeys.viewReasonNeedsImage"),
      run: () => {
        if (!selected || selected.header_block === null || activePane.volumeIndex === null) {
          return;
        }
        setViewing({
          path: activePane.location,
          volumeIndex: activePane.volumeIndex,
          entryBlock: selected.header_block,
          name: selected.name,
        });
      },
    },
    {
      key: "F4",
      label: t("files.functionKeys.edit"),
      enabled: Boolean(selected && !selected.is_dir && canWrite) && busy === null,
      reason: canWrite ? t("files.functionKeys.editReasonSelect") : writeRefusal(activePane, t),
      run: () => {
        if (selected) void editEntry(active, selected);
      },
    },
    {
      key: "F5",
      label: t("files.functionKeys.copy"),
      enabled: Boolean(selected) && busy === null,
      reason: t("files.functionKeys.copyReasonSelect"),
      run: () => {
        if (selected) void copyTo(active, selected);
      },
    },
    {
      key: "F6",
      label: t("files.functionKeys.rename"),
      enabled: Boolean(selected) && canWrite && busy === null,
      reason: canWrite ? t("files.functionKeys.renameReasonSelect") : writeRefusal(activePane, t),
      run: () => {
        if (selected) void renameEntry(active, selected);
      },
    },
    {
      key: "F7",
      label: t("files.functionKeys.newFolder"),
      enabled: canWrite && busy === null,
      reason: writeRefusal(activePane, t),
      run: () => void newFolder(active),
    },
    {
      key: "F8",
      label: t("files.functionKeys.delete"),
      enabled: Boolean(selected) && canWrite && busy === null,
      reason: canWrite ? t("files.functionKeys.deleteReasonSelect") : writeRefusal(activePane, t),
      danger: true,
      run: () => {
        if (selected) void deleteEntry(active, selected);
      },
    },
    {
      key: "F9",
      label: t("files.functionKeys.attributes"),
      enabled: Boolean(selected) && inVolume,
      reason: inVolume
        ? t("files.functionKeys.attributesReasonSelect")
        : t("files.functionKeys.attributesReasonNeedsImage"),
      run: () => {
        if (!selected || selected.header_block === null || activePane.volumeIndex === null) {
          return;
        }
        setAttributes({
          path: activePane.location,
          volumeIndex: activePane.volumeIndex,
          entryBlock: selected.header_block,
        });
      },
    },
  ];

  // Off while a dialog is open: F8 behind a modal would delete something the
  // user cannot see.
  useFunctionKeys(
    actions,
    plan === null && viewing === null && attributes === null && recovery === null
  );

  // An unfinished operation is offered as soon as a pane shows an image that
  // has one — before the user tries to write and is refused.
  useEffect(() => {
    for (const side of ["left", "right"] as Side[]) {
      const state = pane(side);
      const pending = state.capability?.pending_recovery;
      if (pending && recovery?.path !== state.location) {
        setRecovery({ side, path: state.location, description: pending });
        return;
      }
    }
  }, [left, right, pane, recovery]);

  async function resolveRecovery(apply: boolean) {
    const pending = recovery;
    if (!pending) return;
    setRecovery(null);
    setBusy(apply ? t("files.status.undoing") : t("files.status.removingJournal"));
    try {
      const report = await volumeRecover(pending.path, apply);
      setMessage(
        report
          ? t("files.status.undone", {
              description: report.description,
              count: report.blocks_restored,
            })
          : t("files.status.journalRemoved")
      );
      await refresh(pending.side);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="app-content-wide">
      <h1 style={{ fontSize: 20 }}>{t("nav.files")}</h1>
      <p className="muted" style={{ marginTop: 4 }}>
        {t("files.intro")}
      </p>

      {/* §2: an unfinished operation blocks every write until it is decided
          about, so it is offered first and cannot be scrolled past. */}
      {recovery && (
        <div
          className="card"
          style={{ margin: "10px 0", borderColor: "var(--warn)" }}
          role="alertdialog"
          aria-label={t("files.recovery.ariaLabel")}
        >
          <strong>{t("files.recovery.title")}</strong>
          <div style={{ fontSize: 13, marginTop: 4 }}>
            {t("files.recovery.body", { description: recovery.description })}
          </div>
          <div className="faint" style={{ fontSize: 11, marginTop: 4, wordBreak: "break-all" }}>
            {recovery.path}
          </div>
          <div style={{ display: "flex", gap: 6, marginTop: 10, flexWrap: "wrap" }}>
            <button className="btn btn-primary" onClick={() => void resolveRecovery(true)}>
              {t("files.recovery.undo")}
            </button>
            <button
              className="btn"
              onClick={() => void resolveRecovery(false)}
              title={t("files.recovery.leaveTitle")}
            >
              {t("files.recovery.leave")}
            </button>
          </div>
        </div>
      )}

      {error && (
        <div className="badge badge-err" style={{ display: "block", margin: "10px 0" }}>
          {error}
        </div>
      )}
      {message && !error && (
        <div className="badge badge-ok" style={{ display: "block", margin: "10px 0" }}>
          {message}
        </div>
      )}
      {busy && (
        <div className="muted" style={{ fontSize: 12, margin: "8px 0" }}>
          {t("files.busy.suffix", { busy })}
        </div>
      )}

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "1fr auto 1fr",
          gap: 10,
          alignItems: "start",
        }}
      >
        <Pane {...paneProps("left")} />

        <div style={{ display: "flex", flexDirection: "column", gap: 6, paddingTop: 90 }}>
          <button
            className="btn"
            title={t("files.arrows.toRightTitle")}
            disabled={!selection.left || busy !== null}
            onClick={() => {
              const entry = left.entries.find((e) => e.name === selection.left);
              if (entry) void copyTo("left", entry);
            }}
          >
            &rarr;
          </button>
          <button
            className="btn"
            title={t("files.arrows.toLeftTitle")}
            disabled={!selection.right || busy !== null}
            onClick={() => {
              const entry = right.entries.find((e) => e.name === selection.right);
              if (entry) void copyTo("right", entry);
            }}
          >
            &larr;
          </button>
        </div>

        <Pane {...paneProps("right")} />
      </div>

      <FunctionKeyBar actions={actions} />

      <CheckoutPanel
        onChanged={(row) => {
          setMessage(t("files.status.writtenBack", { name: row.name }));
          // Whichever pane holds that image needs re-listing: an edit that
          // changed a file's size moved its blocks.
          for (const side of ["left", "right"] as Side[]) {
            if (pane(side).location === row.image) void refresh(side);
          }
        }}
        onError={setError}
      />

      {/* What a copy will do to names already taken, kept where the user can
          see it rather than buried in the plan dialog they may not see. */}
      <div className="faint" style={{ fontSize: 11, marginTop: 6 }}>
        {t("files.policy.label")}{" "}
        {(
          [
            ["skip", "files.policy.skip"],
            ["overwrite", "files.policy.overwrite"],
            ["rename", "files.policy.rename"],
          ] as Array<[OverwritePolicy, string]>
        ).map(([value, labelKey]) => (
          <button
            key={value}
            className={`btn${policy === value ? " btn-primary" : ""}`}
            style={{ fontSize: 11, marginLeft: 4 }}
            onClick={() => setPolicy(value)}
          >
            {t(labelKey)}
          </button>
        ))}
      </div>

      {plan && (
        <CopyPlanDialog
          plan={plan.plan}
          destination={`${pane(plan.side).volumeName || pane(plan.side).location}`}
          policy={policy}
          onPolicyChange={setPolicy}
          onConfirm={() => void runPlannedCopy()}
          onCancel={() => setPlan(null)}
        />
      )}

      {viewing && (
        <FileViewer
          path={viewing.path}
          volumeIndex={viewing.volumeIndex}
          entryBlock={viewing.entryBlock}
          name={viewing.name}
          onClose={() => setViewing(null)}
        />
      )}

      {attributes && (
        <AttributesDialog
          path={attributes.path}
          volumeIndex={attributes.volumeIndex}
          entryBlock={attributes.entryBlock}
          canEdit={powerMode}
          onClose={() => setAttributes(null)}
          onChanged={() => void refresh(active)}
        />
      )}
    </div>
  );
}

/**
 * What a volume pane says about itself (brief S8).
 *
 * Name, filesystem and free space, always. When ART will not write, a lock and
 * the exact reason on hover -- a pane that looked the same and quietly refused
 * every operation would read as ART being broken.
 */
function VolumeFooter({
  state,
  powerMode,
}: {
  state: PaneState;
  powerMode: boolean;
}) {
  const { t } = useTranslation();
  const capability = state.capability;
  const filesystem =
    capability?.filesystem ??
    (state.volumeIndex !== null
      ? state.image?.volumes[state.volumeIndex]?.filesystem
      : undefined) ??
    state.adf?.fs_type.toUpperCase() ??
    "";

  return (
    <div
      className="muted"
      style={{
        fontSize: 11,
        marginBottom: 6,
        display: "flex",
        gap: 6,
        alignItems: "baseline",
        flexWrap: "wrap",
      }}
    >
      <strong>{capability?.volume_name || state.volumeName || t("files.footer.unnamed")}</strong>
      <span>{filesystem}</span>
      {capability && <span>{t("files.footer.free", { size: formatBytes(capability.free_bytes) })}</span>}

      {capability && !capability.writable && (
        <span
          className="badge badge-warn"
          style={{ fontSize: 10 }}
          title={capability.reason ?? t("files.writeRefusal.default")}
        >
          {t("files.footer.readOnly")}
        </span>
      )}

      {powerMode && capability && (
        <span className="faint">
          {t("files.footer.blocksStrategy", {
            blocks: capability.free_blocks.toLocaleString(),
            strategy:
              capability.strategy === "whole-file"
                ? t("files.footer.strategyWhole")
                : t("files.footer.strategyJournal"),
          })}
        </span>
      )}
    </div>
  );
}

function Pane({
  side,
  state,
  roots,
  selected,
  powerMode,
  onSelect,
  onActivate,
  onUp,
  onOpenFolder,
  onOpenImage,
  onOpenRoot,
  onOpenVolume,
  onRefresh,
  onCopy,
  onDelete,
  onNewFolder,
  onDragOut,
  onDropped,
}: {
  side: Side;
  state: PaneState;
  roots: string[];
  selected: string | null;
  powerMode: boolean;
  onSelect: (name: string) => void;
  onActivate: (entry: PanelEntry) => void;
  onUp: () => void;
  onOpenFolder: () => void;
  onOpenImage: (kind: "adf" | "hdf") => void;
  onOpenRoot: (root: string) => void;
  onOpenVolume: (index: number) => void;
  onRefresh: () => void;
  onCopy: (entry: PanelEntry) => void;
  onDelete: (entry: PanelEntry) => void;
  onNewFolder: () => void;
  onDragOut: (entry: PanelEntry) => void;
  onDropped: (entry: PanelEntry, from: Side) => void;
}) {
  const { t } = useTranslation();
  const [dragOver, setDragOver] = useState(false);

  const showingFiles = state.kind !== "hdf" || state.volumeIndex !== null;
  const canGoUp =
    (state.kind === "local" && state.parent !== null) ||
    (state.kind === "adf" && state.trail.length > 0) ||
    (state.kind === "hdf" && state.volumeIndex !== null);

  // An HDF is never a destination: writing into a partition is not implemented.
  const acceptsDrops = state.kind !== "hdf";

  return (
    <section
      className="card"
      style={{
        minHeight: 420,
        outline: dragOver ? "2px solid var(--accent)" : "none",
      }}
      onDragOver={(event) => {
        if (!acceptsDrops) return;
        event.preventDefault();
        setDragOver(true);
      }}
      onDragLeave={() => setDragOver(false)}
      onDrop={(event) => {
        event.preventDefault();
        setDragOver(false);
        try {
          const payload = JSON.parse(event.dataTransfer.getData("application/art-entry"));
          if (payload?.entry && payload?.side) onDropped(payload.entry, payload.side);
        } catch {
          // A drop from outside ART carries no payload of ours; the global
          // handler in Layout deals with those.
        }
      }}
    >
      <div style={{ display: "flex", gap: 6, flexWrap: "wrap", marginBottom: 8 }}>
        <button className="btn" style={{ fontSize: 12 }} onClick={onOpenFolder}>
          {t("files.toolbar.folder")}
        </button>
        <button className="btn" style={{ fontSize: 12 }} onClick={() => onOpenImage("adf")}>
          {t("files.toolbar.adf")}
        </button>
        <button className="btn" style={{ fontSize: 12 }} onClick={() => onOpenImage("hdf")}>
          {t("files.toolbar.hdf")}
        </button>
        <button className="btn" style={{ fontSize: 12 }} onClick={onUp} disabled={!canGoUp}>
          {t("files.toolbar.up")}
        </button>
        <button className="btn" style={{ fontSize: 12 }} onClick={onRefresh}>
          {t("files.toolbar.refresh")}
        </button>
        {writableVolume(state) !== null && (
          <button className="btn" style={{ fontSize: 12 }} onClick={onNewFolder}>
            {t("files.toolbar.newFolder")}
          </button>
        )}
      </div>

      {state.kind === "local" && roots.length > 0 && (
        <div style={{ display: "flex", gap: 4, marginBottom: 8, flexWrap: "wrap" }}>
          {roots.map((root) => (
            <button
              key={root}
              className="btn"
              style={{ fontSize: 11 }}
              onClick={() => onOpenRoot(root)}
            >
              {root}
            </button>
          ))}
        </div>
      )}

      <div className="faint" style={{ fontSize: 11, marginBottom: 6, wordBreak: "break-all" }}>
        {state.location || t("files.pane.nothingOpen")}
        {state.kind === "hdf" && state.volumeName && ` > ${state.volumeName}:`}
        {state.trail.length > 0 && ` > ${state.trail.map((crumb) => crumb.name).join(" > ")}`}
      </div>

      {state.error && (
        <div className="badge badge-err" style={{ display: "block" }}>
          {state.error}
        </div>
      )}

      {state.kind === "hdf" && state.image && state.volumeIndex === null && (
        <PartitionList image={state.image} powerMode={powerMode} onOpen={onOpenVolume} />
      )}

      {/* §8: the footer always carries the volume's name, its filesystem and
          how much room is left — and a lock with the reason when ART will not
          write, rather than a pane that quietly does nothing. */}
      {state.kind !== "local" && state.volumeIndex !== null && (
        <VolumeFooter state={state} powerMode={powerMode} />
      )}

      {state.warnings.map((warning) => (
        <div
          key={warning}
          className="badge badge-warn"
          style={{ display: "block", fontSize: 11, marginBottom: 4 }}
        >
          {warning}
        </div>
      ))}

      {state.truncated && (
        <div className="badge badge-warn" style={{ display: "block", fontSize: 11 }}>
          {t("files.pane.truncated")}
        </div>
      )}

      {showingFiles && (
        <ul style={{ listStyle: "none", padding: 0, margin: 0, maxHeight: 420, overflow: "auto" }}>
          {state.entries.map((entry) => (
            <li
              key={`${entry.name}-${entry.header_block ?? entry.path}`}
              className="recent-item"
              draggable={!entry.is_dir}
              onDragStart={(event) => {
                event.dataTransfer.setData(
                  "application/art-entry",
                  JSON.stringify({ entry, side })
                );
                event.dataTransfer.effectAllowed = "copy";
                // A local file can also leave the window entirely; the native
                // drag starts from the same gesture.
                if (entry.path) onDragOut(entry);
              }}
              onClick={() => onSelect(entry.name)}
              onDoubleClick={() => onActivate(entry)}
              style={{
                cursor: entry.is_dir ? "pointer" : "grab",
                background:
                  selected === entry.name ? "var(--surface-2, rgba(255,255,255,0.06))" : undefined,
                display: "flex",
                justifyContent: "space-between",
                gap: 8,
              }}
            >
              <span style={{ minWidth: 0 }}>
                <span className="recent-name">{entry.name}</span>
                {entry.is_dir && (
                  <span className="faint" style={{ fontSize: 10 }}>
                    {" "}
                    {t("files.pane.folderSuffix")}
                  </span>
                )}
                {entry.is_link && (
                  <span className="faint" style={{ fontSize: 10 }}>
                    {" "}
                    {t("files.pane.linkSuffix")}
                  </span>
                )}
              </span>
              <span style={{ display: "flex", gap: 6, alignItems: "center" }}>
                {!entry.is_dir && (
                  <span className="faint" style={{ fontSize: 11 }}>
                    {formatBytes(entry.bytes)}
                  </span>
                )}
                {!entry.is_dir && (
                  <button
                    className="btn"
                    style={{ fontSize: 10 }}
                    title={t("files.pane.copyTitle")}
                    onClick={(event) => {
                      event.stopPropagation();
                      onCopy(entry);
                    }}
                  >
                    {side === "left" ? "→" : "←"}
                  </button>
                )}
                {writableVolume(state) !== null && (
                  <button
                    className="btn"
                    style={{ fontSize: 10 }}
                    title={t("files.pane.deleteTitle")}
                    onClick={(event) => {
                      event.stopPropagation();
                      onDelete(entry);
                    }}
                  >
                    X
                  </button>
                )}
              </span>
            </li>
          ))}

          {state.entries.length === 0 && !state.error && (
            <li className="muted" style={{ fontSize: 12, padding: "8px 0" }}>
              {t("files.pane.empty")}
            </li>
          )}
        </ul>
      )}
    </section>
  );
}

/**
 * The partitions in an image, before one is opened.
 *
 * Every partition appears, whether or not ART can read it. A pane that hid the
 * ones it cannot walk would tell the user their disk is broken when it is
 * merely using a filesystem ART has not implemented.
 */
function PartitionList({
  image,
  powerMode,
  onOpen,
}: {
  image: ImageVolumes;
  powerMode: boolean;
  onOpen: (index: number) => void;
}) {
  const { t } = useTranslation();
  const layoutPhrase = describeLayout(image.layout);
  return (
    <div>
      <div className="faint" style={{ fontSize: 11, marginBottom: 6 }}>
        {t(layoutPhrase.key, layoutPhrase.params)}
        {image.volumes.length > 0 &&
          ` · ${t("files.partitions.volumeCount", { count: image.volumes.length })}`}
      </div>

      {image.note && (
        <div className="badge badge-warn" style={{ display: "block", fontSize: 11 }}>
          {image.note}
        </div>
      )}

      <ul style={{ listStyle: "none", padding: 0, margin: 0 }}>
        {image.volumes.map((volume, index) => (
          <li
            key={`${volume.name}-${volume.byte_offset}`}
            className="recent-item"
            style={{ cursor: isMountable(volume) ? "pointer" : "default" }}
            onDoubleClick={() => isMountable(volume) && onOpen(index)}
          >
            <div>
              <span className="recent-name">{volume.name}</span>{" "}
              <span className="faint" style={{ fontSize: 11 }}>
                {formatBytes(volume.byte_length)} &middot; {volume.filesystem}
                {volume.bootable && ` · ${t("files.partitions.bootable")}`}
                {powerMode &&
                  ` · ${t("files.partitions.atByte", { offset: volume.byte_offset.toLocaleString() })}`}
              </span>
            </div>

            {volume.unsupported ? (
              <div className="muted" style={{ fontSize: 11 }}>
                {volume.unsupported}
              </div>
            ) : (
              <button
                className="btn"
                style={{ fontSize: 11, marginTop: 4 }}
                onClick={() => onOpen(index)}
              >
                {t("files.partitions.open")}
              </button>
            )}

            {volume.clamped && (
              <div className="badge badge-warn" style={{ display: "block", fontSize: 11 }}>
                {t("files.partitions.clamped")}
              </div>
            )}
          </li>
        ))}

        {image.volumes.length === 0 && (
          <li className="muted" style={{ fontSize: 12 }}>
            {t("files.partitions.none")}
          </li>
        )}
      </ul>
    </div>
  );
}
