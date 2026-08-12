// Two-pane file manager, Norton Commander style.
//
// Each pane independently shows a local folder, an ADF image, an HDF image or
// an optical disc (ISO9660/Joliet). Copying runs between panes, by button or
// by dragging, and a local file can be dragged out to Explorer.
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
// ## A disc is not a volume
//
// An ISO pane looks like a third kind of volume but is not one: it carries no
// `volumeIndex` (there is nothing to index — a disc is a bare, single-volume
// read), no `capability` (there is nothing to write, ever), and its own
// `isoExtent`/`isoLength`/`isoTrail` rather than the ADF/HDF `dirBlock`/
// `trail` — a disc's directory is `(extent, length)`, not a block number, and
// giving it a second meaning inside `dirBlock` is how the wrong block gets
// read. `writableVolume` already returns `null` for it (no `volumeIndex`),
// which is what makes F4/F6/F7/F8 and F5-in refuse without any extra check at
// their call sites; see `@/lib/isoPane`'s `copyDirection` for the one place
// that routes every copy direction, including refusing every one that would
// write *into* a disc.
//
// ## Function keys
//
// F3 View · F5 Copy · F6 Rename · F7 New folder · F8 Delete · F9 Attributes,
// on the keyboard and on a bar under the panes, because a key nobody knows
// about is a feature nobody has.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useLocation } from "react-router-dom";

import { analyzePaths } from "@/lib/api";
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
  useInsertToggle,
  useMarkKeys,
  useNavigationKeys,
  usePaneHistoryKeys,
  usePaneTab,
  useRefreshKey,
  useSelectAll,
  useSourceComboKeys,
  useTypeAhead,
  type FunctionAction,
} from "@/components/files/FunctionKeys";
import { fileTextColorVar, TcRowIcon, UpDirIcon } from "@/components/files/TcIcon";
import "@/pages/FileManager.css";
import { adfOpen, type AdfInfo } from "@/lib/adf";
import {
  archivesInstall,
  archivesPlanInstall,
  isArchivePath,
  type ArchiveDrawer,
} from "@/lib/archives";
import { checkoutEdit, checkoutOpen, volumeIconFor } from "@/lib/checkout";
import { planFunctionKeys } from "@/lib/functionKeyPlan";
import { onJobProgress, type JobProgress } from "@/lib/jobs";
import { filterEntries } from "@/lib/mask";
import { planMove } from "@/lib/movePlan";
import { extendSearch, shortenSearch } from "@/lib/quickSearch";
import { parseCommandLine } from "@/lib/commandLine";
import {
  formatBytes,
  panelListAdf,
  panelListLocal,
  panelLocalRoots,
  type PanelEntry,
} from "@/lib/panel";
import { splitName } from "@/lib/panelName";
import {
  currentPaneSourceValue,
  paneSourceOptions,
  parsePaneSource,
  type PaneSourceOption,
} from "@/lib/paneSources";
import {
  asksWhatItIs,
  containerBreadcrumb,
  containerFor,
  type ContainerKind,
  type HostReturn,
} from "@/lib/containerStep";
import {
  emptyHistory,
  goBack,
  goForward,
  leaveToHost,
  pushLocation,
  type PaneHistory,
  type PaneLocation,
} from "@/lib/paneHistory";
import { paneStatusCounts } from "@/lib/panelStatus";
import { formatDateTC, formatGroupedSize } from "@/lib/tcFormat";
import {
  emptySelectionUpdate,
  entriesIn,
  insertToggle,
  invertSelection,
  markByMask,
  selectOnly,
  selectRange,
  spaceToggle,
  toggleOne,
  toggleSelectAll,
  type SelectionUpdate,
} from "@/lib/selection";
import {
  clickColumn,
  defaultSortState,
  sortEntries,
  type SortColumn,
  type SortState,
} from "@/lib/sort";
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
  volumeCopyInMany,
  volumeCopyOut,
  volumeDelete,
  volumeDeleteMany,
  volumeMakeDir,
  volumePlanCopy,
  volumePlanCopyMany,
  volumePutFile,
  volumeRecover,
  volumeRename,
  volumeWriteCapability,
  type CopyPlan,
  type CopyReport,
  type WriteCapability,
} from "@/lib/volumeWrite";
import { usePowerMode } from "@/lib/uxmode";
import { useSettingsStore } from "@/stores/settingsStore";
import { isoCopyToVolume, isoExtract, isoExtractFile, isoList, isoOpen, type IsoInfo } from "@/lib/iso";
import {
  archiveEnter,
  archiveLeave,
  copyDirection,
  enterIsoTrail,
  leaveIsoTrail,
  ARCHIVE_WRITE_REFUSAL,
  C64_WRITE_REFUSAL,
  ISO_WRITE_REFUSAL,
  type IsoTrailEntry,
  type PaneKind,
} from "@/lib/isoPane";
import {
  archiveCopyToVolume,
  archiveExtract,
  archiveExtractFile,
  archiveList,
  archiveOpen,
  type ArchiveInfo,
} from "@/lib/archive";
import { cbmExtract, cbmExtractFile, cbmList, cbmOpen, type CbmInfo } from "@/lib/cbm";

type Side = "left" | "right";

/** One pane's state. */
interface PaneState {
  kind: PaneKind;
  /** Local folder path, or the image file path. */
  location: string;
  /** ADF/HDF panes: the directory being shown. Never meaningful for an
   * `"iso"` pane — a disc's directory is `(extent, length)`, not a block
   * number, so it gets its own `isoExtent`/`isoLength` below rather than
   * overloading this field with a second meaning. */
  dirBlock: number | null;
  /** ADF/HDF panes: folders walked into, so "up" can go back. */
  trail: Array<{ name: string; block: number | null }>;
  /** ISO panes: the directory being shown — see `dirBlock`'s comment for why
   * this is not that field. `null` until the disc has been opened. */
  isoExtent: number | null;
  isoLength: number | null;
  /** ISO panes: folders walked into, so "up" can go back — see `trail`'s
   * comment for why this is not that field. */
  isoTrail: IsoTrailEntry[];
  /**
   * Archive panes: the folder being shown, as a path inside the archive
   * (`""` is the root). `null` for every other pane kind.
   *
   * One string and no trail, unlike the two fields above, because an
   * archive's folders come from the entry *names* — `Tools/Sub/Deep.txt` — so
   * the path is the address and "up" is arithmetic on it
   * (`archiveLeave` in `@/lib/isoPane`). A disc needs a trail because
   * `(extent, length)` says nothing about what is above it.
   */
  archiveDir: string | null;
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
  /**
   * The volume's total capacity in bytes, for the Total Commander-styled
   * drive row's "free of total" (task 6b) — `null` for a local folder (ART
   * has no free/total-space command for a Windows drive) and for an HDF
   * still showing its partition list (no single volume open yet). Read
   * straight out of data `openAdf`/`openVolume` already fetch — `AdfInfo`'s
   * `capacity_bytes`, `VolumeListing`'s `total_blocks * block_size` — never
   * a new call of its own.
   */
  totalBytes: number | null;
  /**
   * The host folder this pane's container was entered from, and the container
   * file's own name (brief §3.1).
   *
   * Set when Enter on a row opened an image *in the same pane*; `null` for a
   * plain folder, and for an image opened straight from the source combo,
   * which has nowhere to go back out to. It is what makes `[..]` at a
   * container's root leave the image and land the cursor back on the file it
   * came from, rather than leaving the user inside a disk with no way out but
   * the combo.
   *
   * A single value rather than a stack, and that is not an oversight: ART
   * opens an image by path, so a container can only ever be entered from a
   * host *folder* — there is no route from inside one container into another
   * (see `asksWhatItIs` in `@/lib/containerStep`). One level is all there can
   * be.
   */
  host: HostReturn | null;
  error: string | null;
}

function emptyPane(): PaneState {
  return {
    kind: "local",
    location: "",
    dirBlock: null,
    trail: [],
    isoExtent: null,
    isoLength: null,
    isoTrail: [],
    archiveDir: null,
    entries: [],
    parent: null,
    truncated: false,
    adf: null,
    image: null,
    volumeIndex: null,
    volumeName: "",
    warnings: [],
    capability: null,
    totalBytes: null,
    host: null,
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
  if (state.kind === "iso") return t(ISO_WRITE_REFUSAL.key);
  if (state.kind === "archive") return t(ARCHIVE_WRITE_REFUSAL.key);
  if (state.kind === "c64") return t(C64_WRITE_REFUSAL.key);
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

/** How [`runJob`] settled: `"finished"` alone must never be read as success —
 * callers still need to check the job's own report for what actually
 * landed — but `"cancelled"` must never be read as `"finished"` either. */
type JobOutcome = "finished" | "cancelled";

/**
 * Start a background job (§54) and wait for it to reach a terminal state.
 *
 * A multi-selection copied out of a volume runs one job per selected folder
 * (`volumeCopyOut` is job-based; a plain file goes straight through
 * `volumeExtractTo` and needs no waiting at all). Those jobs are otherwise
 * tracked through the screen-wide `pendingCopy` ref, which assumes exactly
 * one job in flight — the wrong shape for "several, run together" — so this
 * gives the batch path its own, independent wait per job instead of
 * reusing that ref.
 *
 * Takes the job's *starter* rather than an already-known id, and subscribes
 * to both event streams before calling it — a small folder can finish
 * inside the two async round-trips `start` itself takes (invoke, then the
 * id coming back), and a Tauri event emitted before anything is listening is
 * lost for good. Events that arrive before the id is known are buffered and
 * replayed once it is, rather than the old shape (subscribe *after* the
 * caller already has the id), which could leave this promise waiting
 * forever with no error and no way for the screen to recover (finding 3 of
 * the phase-1a whole-branch review).
 */
function runJob(start: () => Promise<number>): Promise<JobOutcome> {
  return new Promise<JobOutcome>((resolve, reject) => {
    let jobId: number | null = null;
    let settled = false;
    let offResult: (() => void) | undefined;
    let offProgress: (() => void) | undefined;
    const pendingProgress: JobProgress[] = [];

    const finish = (action: () => void) => {
      if (settled) return;
      settled = true;
      offResult?.();
      offProgress?.();
      action();
    };

    const handleProgress = (job: JobProgress) => {
      if (jobId === null || job.id !== jobId || job.state.state === "running") return;
      if (job.state.state === "failed") {
        // Captured in a local so the narrowing survives into the closure
        // `finish` calls later — TS does not carry it through `job.state`
        // itself across the function boundary.
        const failure = job.state;
        finish(() => reject(new Error(`${failure.message} (${failure.error_code})`)));
      } else if (job.state.state === "cancelled") {
        // A cancelled batch must be reported as cancelled, never folded into
        // "finished" — a caller that cannot tell the two apart reports a
        // stopped copy as a full success.
        finish(() => resolve("cancelled"));
      } else {
        finish(() => resolve("finished"));
      }
    };

    Promise.all([
      onVolumeWriteResult((result) => {
        if (jobId !== null && result.job_id === jobId) finish(() => resolve("finished"));
      }),
      onJobProgress((job) => {
        if (jobId === null) {
          pendingProgress.push(job);
          return;
        }
        handleProgress(job);
      }),
    ]).then(([offR, offP]) => {
      offResult = offR;
      offProgress = offP;
      if (settled) {
        offR();
        offP();
        return;
      }

      start()
        .then((id) => {
          if (settled) return;
          jobId = id;
          // Replay whatever arrived in the window between subscribing and
          // learning the id — including a terminal state reached before
          // `start()` even resolved.
          for (const job of pendingProgress) {
            if (settled) break;
            handleProgress(job);
          }
        })
        .catch((err) => {
          finish(() => reject(err instanceof Error ? err : new Error(String(err))));
        });
    });
  });
}

export function FileManager() {
  const { t } = useTranslation();
  const powerMode = usePowerMode();
  /** The optional button strip above each pane (brief §1.3). Off unless the
   * user turns it on: the header's source combo reaches everything in it. */
  const showSourceButtons = useSettingsStore((s) => s.settings.showSourceButtons);
  const [left, setLeft] = useState<PaneState>(emptyPane());
  const [right, setRight] = useState<PaneState>(emptyPane());
  const [roots, setRoots] = useState<string[]>([]);
  /** The header combo's options — the enumerated mounts plus the six things
   * ART opens with a picker (`@/lib/paneSources`). Both panes share one list;
   * only which option is *current* differs between them. */
  const sourceOptions = useMemo(() => paneSourceOptions(roots), [roots]);
  /** Which entries (by name) are marked in each pane. See `@/lib/selection`. */
  const [selection, setSelection] = useState<Record<Side, Set<string>>>({
    left: new Set(),
    right: new Set(),
  });
  /**
   * The entry each pane's mouse or keyboard last landed on.
   *
   * Not the same thing as being selected: a row can anchor a future
   * Shift+click range, or be where Insert's "cursor" sits, without itself
   * being marked. Kept separate so the two can be shown with different
   * highlights — see `Pane` below.
   */
  const [anchor, setAnchor] = useState<Record<Side, string | null>>({
    left: null,
    right: null,
  });
  /**
   * Which column each pane is sorted by, and which direction.
   *
   * Per pane, like `selection` and `anchor` above, and reset alongside them
   * on navigation — see `resetSelection` — so a sort chosen in one folder
   * does not silently carry into the next one shown.
   */
  const [sort, setSort] = useState<Record<Side, SortState>>({
    left: defaultSortState(),
    right: defaultSortState(),
  });
  /**
   * Each pane's filename mask — the `*.*` in the reference's path row
   * (`@/lib/mask`). Per pane, like `sort` above, and reset alongside it on
   * navigation: a mask typed in one folder that silently kept hiding files
   * in the next one shown would read as a broken listing, not a filter the
   * user forgot was on.
   */
  const [filter, setFilter] = useState<Record<Side, string>>({
    left: "",
    right: "",
  });
  /**
   * Which pane the keyboard is talking to.
   *
   * Tracked directly rather than derived from `selection`: a pane can hold
   * many selections or none (multi-select) and still be the one F-keys and
   * Tab act on. Every F-key action reads this, never `selection` — see the
   * function-key table below.
   */
  const [focused, setFocused] = useState<Side>("left");
  /** What is typed into the command line above the F-key bar. One box for
   * the whole screen, acting on whichever pane is focused — Total Commander
   * has one too, for the same reason. */
  const [commandLine, setCommandLine] = useState("");
  /** What has been typed into type-to-search, and the timer that ends it. */
  const [search, setSearch] = useState("");
  const searchTimer = useRef<number | null>(null);
  /** The two source combos, so Alt+F1 / Alt+F2 can open them. */
  const sourceCombos = {
    left: useRef<HTMLSelectElement>(null),
    right: useRef<HTMLSelectElement>(null),
  };
  /**
   * Each pane's own back/forward list (`@/lib/paneHistory`).
   *
   * Recorded by watching the panes rather than by every `openX` remembering to
   * call something: there are seven of those and a new one would silently not
   * be in the history. `historyNav` is what keeps a Back from being recorded
   * as a move of its own — without it, going back would truncate the forward
   * entries it had just created, and Forward would never work once.
   */
  const [history, setHistory] = useState<Record<Side, PaneHistory>>({
    left: emptyHistory(),
    right: emptyHistory(),
  });
  const historyNav = useRef(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  /**
   * ART declining something, as opposed to something breaking.
   *
   * "Both panes are local folders — use Explorer for that" is not a failure:
   * nothing broke, nothing has an `ART-*` id to look up, and the red alert
   * banner it used to get taught the user to read ART's real errors as noise.
   * Same distinction `Refusal.tsx` draws for the WHDLoad panel; this is its
   * one-line form for the commander's status strip (brief §1.4).
   */
  const [hint, setHint] = useState<string | null>(null);

  /**
   * What to do about names already taken.
   *
   * A *setting* since task 3, not a control on this screen: the permanent
   * footer that used to ask the question — whether or not it was about to
   * come up — is gone, and `CopyPlanDialog` asks it where Total Commander
   * does, at the moment a collision is actually found. Changing it there
   * changes the default, which is what a user who answers the same way every
   * time expects.
   */
  const policy = useSettingsStore((s) => s.settings.overwritePolicy) as OverwritePolicy;
  const updateSettings = useSettingsStore((s) => s.update);
  const setPolicy = useCallback(
    (next: OverwritePolicy) => void updateSettings({ overwritePolicy: next }),
    [updateSettings]
  );
  /**
   * The pre-flight report, while the user is deciding about it.
   *
   * `sources`/`names` hold one entry for a single-folder copy and several for
   * a batch — the same shape either way, so `CopyPlanDialog` reads one path
   * regardless of how many roots are in it (a one-entry batch must still
   * read naturally, not as "1 items").
   */
  const [plan, setPlan] = useState<{
    plan: CopyPlan;
    sources: string[];
    names: string[];
    side: Side;
    /**
     * Set only for a batch of `.lha` archives — the drawer each one will
     * create. Its presence is what tells `runPlannedCopy` to confirm through
     * `archivesInstall` instead of `volumeCopyIn`/`volumeCopyInMany`, and what
     * tells `CopyPlanDialog` to show the drawer names (§92).
     */
    drawers?: ArchiveDrawer[];
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

  /**
   * This pane's entries in the order actually shown on screen: the server's
   * listing (already folders-first, case-insensitive name — see
   * `commands/panel.rs` and its ADF/HDF equivalents) narrowed by the pane's
   * filename mask (`@/lib/mask`) and *then* run through the column sort the
   * user picked — filter first, sort second, so the visible list is always
   * sorted, through the one comparator `@/lib/sort.ts` owns rather than a
   * second one grown here. Every order-sensitive action — rendering,
   * Shift+click ranges, Insert, Ctrl+A, "in pane order" — reads through this
   * rather than `pane(side).entries` directly, so none of them can disagree
   * with what is drawn. It is also what makes the per-pane status line
   * (`paneStatusCounts` in `Pane`, below) count the *filtered* view rather
   * than the whole directory — Total Commander's own convention, and what
   * keeps the totals on screen matching what is actually listed.
   */
  const paneEntries = useCallback(
    (side: Side): PanelEntry[] => sortEntries(filterEntries(pane(side).entries, filter[side]), sort[side]),
    [pane, sort, filter]
  );

  /**
   * The entries `selection[side]` actually names, in pane order.
   *
   * Because this intersects `selection[side]` with `paneEntries(side)` —
   * the *filtered* list — an entry the mask currently hides can never come
   * back out of this, even if the raw `Set` still names it. That is what
   * makes F5/F8 safe against a stale selection on its own; `setPaneFilter`
   * below additionally clears the selection outright on every filter
   * change, so the "N selected" count on screen never lies about a
   * selection the user can no longer see (see its own comment).
   */
  const selectedEntries = useCallback(
    (side: Side): PanelEntry[] => entriesIn(paneEntries(side), selection[side]),
    [paneEntries, selection]
  );

  /** Apply a `SelectionUpdate` (from `@/lib/selection`) to one side. */
  const applySelection = useCallback((side: Side, update: SelectionUpdate) => {
    setSelection((s) => ({ ...s, [side]: update.selected }));
    setAnchor((a) => ({ ...a, [side]: update.anchor }));
  }, []);

  /**
   * Move a pane's cursor without touching what is marked.
   *
   * Type-to-search's whole contract: a user typing a name to get to it must
   * not lose the selection they spent a minute building. `applySelection`
   * cannot do this — it sets both — which is exactly why this exists.
   */
  const moveCursor = useCallback((side: Side, name: string) => {
    setAnchor((a) => ({ ...a, [side]: name }));
  }, []);

  /** The names a pane is showing, in the order it is showing them. */
  const paneNames = useCallback(
    (side: Side): string[] => paneEntries(side).map((entry) => entry.name),
    [paneEntries]
  );

  /**
   * Note what has been typed into the type-to-search prefix, and arm the idle
   * timer that ends the search.
   *
   * The timer is the only stateful half of the feature — every decision about
   * what a letter *does* is in `@/lib/quickSearch`, where it is tested. One
   * and a half seconds is Total Commander's own feel: long enough to think
   * about the next letter of a name, short enough that a search you have
   * forgotten about is not still swallowing keys.
   */
  const noteSearch = useCallback((prefix: string) => {
    setSearch(prefix);
    if (searchTimer.current !== null) window.clearTimeout(searchTimer.current);
    searchTimer.current =
      prefix === "" ? null : window.setTimeout(() => setSearch(""), 1500);
  }, []);

  /**
   * Change one pane's filename mask.
   *
   * Filtering is display-only and must never change what an action
   * operates on — `selectedEntries` above already guarantees that on its
   * own, since it can only ever resolve to entries the filtered view still
   * shows. But a selection made before the mask narrowed the list would
   * still sit in `selection[side]` invisibly: the pane's status line would
   * keep reporting "20 selected" over a pane now showing three, which is a
   * surprise the user has no way to see through.
   * Clearing the selection on every keystroke here — rather than keeping
   * the hidden names and merely showing their count — is the simpler rule
   * and the one Total Commander users expect: a filter change starts the
   * selection over, the same way navigating to a new folder does.
   */
  const setPaneFilter = useCallback(
    (side: Side, mask: string) => {
      setFilter((f) => ({ ...f, [side]: mask }));
      applySelection(side, emptySelectionUpdate());
    },
    [applySelection]
  );

  /** Selection, sort and the filter mask all reset on navigation: a Set, a
   * sort order, or a mask that survived a directory change would let an
   * action reach an entry the user has since left behind, show an order
   * that quietly stopped matching what the user actually clicked, or hide
   * files in a folder the user never typed a mask for. */
  const resetSelection = useCallback((side: Side) => {
    setSelection((s) => ({ ...s, [side]: new Set() }));
    setAnchor((a) => ({ ...a, [side]: null }));
    setSort((s) => ({ ...s, [side]: defaultSortState() }));
    setFilter((f) => ({ ...f, [side]: "" }));
  }, []);

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

  /**
   * Show a host folder.
   *
   * `cursor` is the one addition task 4 needed: leaving a container puts the
   * cursor back on the file it came from, so a user who steps into
   * `Lotus.adf`, looks around and comes out again finds themself exactly where
   * they were rather than at the top of a folder of four hundred names. It is
   * set *after* `resetSelection`, which clears the anchor — the order matters,
   * and reversing it would silently do nothing.
   */
  const openLocal = useCallback(
    async (side: Side, path: string, cursor?: string) => {
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
        resetSelection(side);
        if (cursor) setAnchor((a) => ({ ...a, [side]: cursor }));
        setFocused(side);
      } catch (e) {
        setPane(side, { ...emptyPane(), location: path, error: String(e) });
      }
    },
    [setPane, resetSelection]
  );

  const openAdf = useCallback(
    async (
      side: Side,
      path: string,
      dirBlock: number | null,
      trail: PaneState["trail"],
      host: HostReturn | null
    ) => {
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
          host,
          entries,
          adf: info,
          volumeIndex: 0,
          volumeName: capability?.volume_name ?? info.volume_name ?? "",
          capability,
          totalBytes: info.capacity_bytes,
        });
        resetSelection(side);
        setFocused(side);
      } catch (e) {
        setPane(side, { ...emptyPane(), kind: "adf", location: path, host, error: String(e) });
      }
    },
    [setPane, resetSelection]
  );

  /** Open an HDF on its partition list. */
  const openHdf = useCallback(
    async (side: Side, path: string, host: HostReturn | null) => {
      try {
        const image = await volumeScan(path);
        setPane(side, { ...emptyPane(), kind: "hdf", location: path, image, host });
        resetSelection(side);
        setFocused(side);
      } catch (e) {
        setPane(side, { ...emptyPane(), kind: "hdf", location: path, host, error: String(e) });
      }
    },
    [setPane, resetSelection]
  );

  /** Browse inside one partition of an already-scanned image. */
  const openVolume = useCallback(
    async (
      side: Side,
      path: string,
      image: ImageVolumes,
      volumeIndex: number,
      dirBlock: number | null,
      trail: PaneState["trail"],
      host: HostReturn | null
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
          totalBytes: listing.total_blocks * listing.block_size,
          dirBlock: dirBlock ?? listing.root_block,
          trail,
          host,
          entries: listing.entries,
        });
        resetSelection(side);
        setFocused(side);
      } catch (e) {
        setPane(side, {
          ...emptyPane(),
          kind: "hdf",
          location: path,
          image,
          host,
          error: String(e),
        });
      }
    },
    [setPane, resetSelection]
  );

  /**
   * Open a disc on its root directory — a bare volume, the same shape an ADF
   * is, just addressed by `(extent, length)` instead of a block number.
   *
   * A disc is read-only end to end: no `capability` is fetched, because
   * there is no `volumeIndex` to fetch one for (`writableVolume` already
   * returns `null` for any pane with `volumeIndex === null`, which is what
   * makes F6/F7/F8 and F5-in refuse without any extra check at their call
   * sites — see `writeRefusal` for the message that refusal shows).
   */
  const openIso = useCallback(
    async (
      side: Side,
      path: string,
      extent: number | null,
      length: number | null,
      trail: IsoTrailEntry[],
      host: HostReturn | null
    ) => {
      try {
        // `extent`/`length` are `null` only when the disc has not been
        // opened this session yet (first open, or "up" past a stale pane) —
        // `isoOpen` is what finds the root in that case.
        const info: IsoInfo = await isoOpen(path);
        const rootExtent = extent ?? info.root_extent;
        const rootLength = length ?? info.root_length;
        const entries: PanelEntry[] = await isoList(path, rootExtent, rootLength);
        setPane(side, {
          ...emptyPane(),
          kind: "iso",
          location: path,
          isoExtent: rootExtent,
          isoLength: rootLength,
          isoTrail: trail,
          host,
          entries,
          volumeName: info.volume_name,
        });
        resetSelection(side);
        setFocused(side);
      } catch (e) {
        setPane(side, { ...emptyPane(), kind: "iso", location: path, host, error: String(e) });
      }
    },
    [setPane, resetSelection]
  );

  /**
   * Open an archive as a pane — the same container model as a disc, over a
   * format that has no directories at all.
   *
   * `dir` is a path inside the archive (`""` is the root). The folders it
   * walks are built from the entry names by `core::archive::tree`, which
   * refuses to show a name that is not a plain relative path; `archive_open`
   * reports how many it left out, so a listing that is missing entries says
   * so instead of quietly being short.
   *
   * Read-only end to end, like a disc: no `capability` is fetched and no
   * `volumeIndex` is set, which is what makes F6/F7/F8 and every copy *into*
   * this pane refuse without a single extra check at their call sites.
   */
  const openArchive = useCallback(
    async (side: Side, path: string, dir: string, host: HostReturn | null) => {
      try {
        const info: ArchiveInfo = await archiveOpen(path);
        const entries: PanelEntry[] = await archiveList(path, dir);
        setPane(side, {
          ...emptyPane(),
          kind: "archive",
          location: path,
          archiveDir: dir,
          host,
          entries,
          volumeName: info.format.toUpperCase(),
          warnings:
            info.unusable_names > 0
              ? [t("files.archive.unusableNames", { count: info.unusable_names })]
              : [],
        });
        resetSelection(side);
        setFocused(side);
      } catch (e) {
        setPane(side, { ...emptyPane(), kind: "archive", location: path, host, error: String(e) });
      }
    },
    [setPane, resetSelection, t]
  );

  /**
   * Open a Commodore 8-bit disk or tape as a pane.
   *
   * **Flat, so there is no navigation.** A 1541 directory has no
   * subdirectories and neither does a T64: what opens is the whole image, and
   * `goUp` has nothing to do here on purpose rather than by omission.
   *
   * Read-only like the disc and archive panes, and by the same mechanism —
   * no `volumeIndex`, so every write action refuses without a check of its
   * own. Whatever the image says about itself that ART had to work around
   * (a T64 header that disagrees with its records, entries whose declared
   * length the file cannot hold) arrives as `notes` and is shown, because an
   * image that lists differently from what its own header claims should say
   * so.
   */
  const openCbm = useCallback(
    async (side: Side, path: string, host: HostReturn | null) => {
      try {
        const info: CbmInfo = await cbmOpen(path);
        const entries: PanelEntry[] = await cbmList(path);
        setPane(side, {
          ...emptyPane(),
          kind: "c64",
          location: path,
          host,
          entries,
          volumeName: info.disk_id
            ? `${info.volume_name} ${info.disk_id}`
            : info.volume_name,
          warnings: info.notes,
        });
        resetSelection(side);
        setFocused(side);
      } catch (e) {
        setPane(side, { ...emptyPane(), kind: "c64", location: path, host, error: String(e) });
      }
    },
    [setPane, resetSelection]
  );

  async function chooseImage(side: Side, kind: "adf" | "hdf" | "iso" | "archive" | "c64") {
    const picked = await open({
      multiple: false,
      filters:
        kind === "adf"
          ? [{ name: "Amiga floppy image", extensions: ["adf"] }]
          : kind === "hdf"
            ? [{ name: "Amiga hard disk image", extensions: ["hdf", "hda", "img"] }]
            : kind === "iso"
              ? [{ name: "Optical disc image", extensions: ["iso"] }]
              : kind === "archive"
                ? // The dialog filters by extension because that is all a file
                  // dialog can do; what the file *is* is decided from its bytes
                  // once it is open, so a `.lha` holding a ZIP still opens.
                  [{ name: "Archive", extensions: ["lha", "lzh", "zip", "7z"] }]
                : [
                    {
                      name: "Commodore disk or tape",
                      extensions: ["d64", "d71", "d81", "t64"],
                    },
                  ],
    });
    if (typeof picked !== "string") return;
    // `host: null` throughout — an image opened from the source combo was not
    // entered *from* anywhere, so `[..]` at its root correctly has nowhere to
    // go and is not offered.
    if (kind === "adf") await openAdf(side, picked, null, [], null);
    else if (kind === "hdf") await openHdf(side, picked, null);
    else if (kind === "iso") await openIso(side, picked, null, null, [], null);
    else if (kind === "archive") await openArchive(side, picked, "", null);
    else await openCbm(side, picked, null);
  }

  /**
   * Open whatever the workflow engine sent here, in the left pane.
   *
   * Every `Navigate` workflow hands its object over the same way — a route
   * plus `{ state: { path } }` — and every other studio reads it on mount
   * (`AdfBrowser`, `CollectionStudio`, …). This screen did not, so
   * `iso.browse` (Task 3) and `archive.browse` (Task 4) both landed the user
   * on the file manager with the pane they already had, and the dropped
   * object nowhere in sight.
   *
   * What it is decides which pane opens, and that answer comes from
   * `analyze_paths` — the same detection the drop panel used to offer the
   * action in the first place — rather than from the extension.
   */
  const location = useLocation();
  useEffect(() => {
    const wanted = (location.state as { path?: string } | null)?.path;
    if (!wanted) return;

    let cancelled = false;
    void (async () => {
      try {
        const [analysis] = await analyzePaths([wanted]);
        if (cancelled || !analysis?.plan) return;
        const category = analysis.plan.detection.category;
        // Also `host: null`: the object came from a drop, not from a folder
        // this pane was standing in.
        if (category === "optical-image") await openIso("left", wanted, null, null, [], null);
        else if (category === "archive") await openArchive("left", wanted, "", null);
        else if (category === "commodore-8bit") await openCbm("left", wanted, null);
        else if (category === "floppy-image") await openAdf("left", wanted, null, [], null);
        else if (category === "harddisk-image") await openHdf("left", wanted, null);
        else if (category === "directory") await openLocal("left", wanted);
      } catch {
        // A path that cannot be analysed is not worth an error banner on a
        // screen the user may have navigated to for something else; the pane
        // simply stays as it was.
      }
    })();

    return () => {
      cancelled = true;
    };
    // `location.state` is the trigger: navigating here again with a different
    // object must open that one.
  }, [location.state, openIso, openArchive, openCbm, openAdf, openHdf, openLocal]);

  async function chooseFolder(side: Side) {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked === "string") await openLocal(side, picked);
  }

  /**
   * Enter, or a double-click, on a row (brief §3.1).
   *
   * A directory walks into itself, as it always has. A **file in a host
   * folder** is the new half: ART asks what it actually is, and if the answer
   * is something ART can list, **this pane becomes that container** — the
   * pane kind changes underneath and `[..]` comes back out.
   *
   * The question goes to `analyze_paths`, the same content-first detection the
   * drop panel uses, never to the extension: an `.img` holding a floppy opens
   * as a floppy and an LHA somebody renamed `.dat` still opens (phase 2a, and
   * ART-076 for what happens when that is got wrong). A file ART has no pane
   * for — a ROM, anything unrecognised — does nothing, quietly: Enter is not
   * a place to put an error about a file the user may simply have been
   * cursoring past.
   */
  async function activate(side: Side, entry: PanelEntry) {
    const state = pane(side);

    if (!entry.is_dir) {
      if (asksWhatItIs(entry, state.kind) && entry.path) {
        await enterContainer(side, entry.path, entry.name, state.location);
      }
      return;
    }

    if (state.kind === "local" && entry.path) {
      await openLocal(side, entry.path);
    } else if (state.kind === "adf" && entry.header_block !== null) {
      await openAdf(
        side,
        state.location,
        entry.header_block,
        [...state.trail, { name: entry.name, block: state.dirBlock }],
        state.host
      );
    } else if (
      state.kind === "hdf" &&
      state.image &&
      state.volumeIndex !== null &&
      entry.header_block !== null
    ) {
      await openVolume(
        side,
        state.location,
        state.image,
        state.volumeIndex,
        entry.header_block,
        [...state.trail, { name: entry.name, block: state.dirBlock }],
        state.host
      );
    } else if (
      state.kind === "iso" &&
      entry.iso_extent !== null &&
      state.isoExtent !== null &&
      state.isoLength !== null
    ) {
      await openIso(
        side,
        state.location,
        entry.iso_extent,
        entry.bytes,
        enterIsoTrail(state.isoTrail, entry.name, state.isoExtent, state.isoLength),
        state.host
      );
    } else if (state.kind === "archive" && state.archiveDir !== null) {
      await openArchive(
        side,
        state.location,
        archiveEnter(state.archiveDir, entry.name),
        state.host
      );
    }
  }

  /**
   * Open `path` as a container in this pane, remembering where to come back to.
   *
   * The detection round trip is the only thing that makes this slower than
   * walking into a folder, and it is not optional: it is what decides *which*
   * pane opens, and it is the one place a wrong answer would put the user
   * inside the wrong reader. A file that turns out not to be a container
   * leaves the pane exactly as it was.
   */
  async function enterContainer(
    side: Side,
    path: string,
    name: string,
    hostPath: string
  ) {
    setBusy(t("files.status.opening", { name }));
    try {
      const [analysis] = await analyzePaths([path]);
      const category = analysis?.plan?.detection.category;
      const container: ContainerKind | null = category
        ? containerFor(category as Parameters<typeof containerFor>[0])
        : null;
      if (!container) return;

      const host: HostReturn = { path: hostPath, name };
      if (container === "adf") await openAdf(side, path, null, [], host);
      else if (container === "hdf") await openHdf(side, path, host);
      else if (container === "iso") await openIso(side, path, null, null, [], host);
      else if (container === "archive") await openArchive(side, path, "", host);
      else await openCbm(side, path, host);
    } catch (e) {
      setError(String(e));
      setHint(null);
    } finally {
      setBusy(null);
    }
  }

  /**
   * `[..]`, Backspace and Ctrl+PgUp — up one level, whatever "level" means
   * here.
   *
   * The levels a pane can climb, innermost first: a directory inside a
   * volume, a partition back to its image's partition list, a folder inside a
   * disc or an archive — and then, at the container's own root, **out of the
   * image entirely** to the host folder it was entered from, with the cursor
   * landing back on the container file. That last step is what makes Enter
   * into a container a place you can leave rather than a one-way door
   * (brief §3.1).
   */
  /**
   * Where a pane is, as the serialisable shape history and (task 6) session
   * restore both read. `null` before anything has been opened.
   */
  function locationOf(state: PaneState): PaneLocation | null {
    if (!state.location) return null;
    switch (state.kind) {
      case "local":
        return { kind: "local", path: state.location };
      case "adf":
        return {
          kind: "adf",
          path: state.location,
          dirBlock: state.dirBlock,
          trail: state.trail,
          host: state.host,
        };
      case "hdf":
        return {
          kind: "hdf",
          path: state.location,
          volumeIndex: state.volumeIndex,
          dirBlock: state.dirBlock,
          trail: state.trail,
          host: state.host,
        };
      case "iso":
        return {
          kind: "iso",
          path: state.location,
          extent: state.isoExtent,
          length: state.isoLength,
          trail: state.isoTrail,
          host: state.host,
        };
      case "archive":
        return {
          kind: "archive",
          path: state.location,
          dir: state.archiveDir ?? "",
          host: state.host,
        };
      case "c64":
        return { kind: "c64", path: state.location, host: state.host };
    }
  }

  /** Put a pane back at a location the history remembered. */
  async function openLocation(side: Side, target: PaneLocation) {
    historyNav.current = true;
    switch (target.kind) {
      case "local":
        await openLocal(side, target.path);
        return;
      case "adf":
        await openAdf(side, target.path, target.dirBlock, target.trail, target.host);
        return;
      case "hdf": {
        if (target.volumeIndex === null) {
          await openHdf(side, target.path, target.host);
          return;
        }
        // A partition needs its image's volume table. The pane usually still
        // holds it; re-scanning is the fallback for a history step that
        // crossed to a different image.
        const current = pane(side);
        const image =
          current.image && current.location === target.path
            ? current.image
            : await volumeScan(target.path);
        await openVolume(
          side,
          target.path,
          image,
          target.volumeIndex,
          target.dirBlock,
          target.trail,
          target.host
        );
        return;
      }
      case "iso":
        await openIso(side, target.path, target.extent, target.length, target.trail, target.host);
        return;
      case "archive":
        await openArchive(side, target.path, target.dir, target.host);
        return;
      case "c64":
        await openCbm(side, target.path, target.host);
        return;
    }
  }

  // Record every move. Watching the panes rather than trusting seven `openX`
  // functions to each remember is what makes a new one impossible to forget.
  useEffect(() => {
    if (historyNav.current) {
      historyNav.current = false;
      return;
    }
    setHistory((current) => {
      let next = current;
      for (const side of ["left", "right"] as Side[]) {
        const target = locationOf(side === "left" ? left : right);
        if (!target) continue;
        const pushed = pushLocation(next[side], target);
        if (pushed !== next[side]) next = { ...next, [side]: pushed };
      }
      return next;
    });
    // Only the panes themselves are the trigger: `locationOf` is pure over
    // them, and adding it to the deps would re-run this on every render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [left, right]);

  async function goUp(side: Side) {
    const state = pane(side);

    if (state.kind === "local" && state.parent) {
      await openLocal(side, state.parent);
      return;
    }

    if (state.kind === "adf" && state.trail.length > 0) {
      const trail = [...state.trail];
      const previous = trail.pop()!;
      await openAdf(side, state.location, previous.block, trail, state.host);
      return;
    }

    if (state.kind === "hdf" && state.volumeIndex !== null && state.image) {
      if (state.trail.length > 0) {
        const trail = [...state.trail];
        const previous = trail.pop()!;
        await openVolume(
          side,
          state.location,
          state.image,
          state.volumeIndex,
          previous.block,
          trail,
          state.host
        );
      } else {
        // Out of the partition, back to the list of them — a level of its
        // own, and the reason an HDF's partitions are browsable at all.
        await openHdf(side, state.location, state.host);
      }
      return;
    }

    if (state.kind === "iso" && state.isoTrail.length > 0) {
      const back = leaveIsoTrail(state.isoTrail);
      if (back) {
        await openIso(side, state.location, back.extent, back.length, back.trail, state.host);
      }
      return;
    }

    if (state.kind === "archive" && state.archiveDir) {
      const up = archiveLeave(state.archiveDir);
      if (up !== null) await openArchive(side, state.location, up, state.host);
      return;
    }

    // Nothing left inside the container: leave the image for the folder it
    // was entered from. `leaveToHost` is `null` for a pane that was opened
    // from the source combo or by a workflow, which is exactly the case where
    // `[..]` is not offered.
    const back = leaveToHost(state.host);
    if (back) await openLocal(side, back.path, back.cursor);
  }

  const refresh = useCallback(
    async (side: Side) => {
      const state = pane(side);
      if (state.kind === "local" && state.location) {
        await openLocal(side, state.location);
      } else if (state.kind === "adf") {
        await openAdf(side, state.location, state.dirBlock, state.trail, state.host);
      } else if (state.kind === "iso") {
        await openIso(
          side,
          state.location,
          state.isoExtent,
          state.isoLength,
          state.isoTrail,
          state.host
        );
      } else if (state.kind === "archive") {
        await openArchive(side, state.location, state.archiveDir ?? "", state.host);
      } else if (state.kind === "c64") {
        await openCbm(side, state.location, state.host);
      } else if (state.kind === "hdf") {
        if (state.image && state.volumeIndex !== null) {
          await openVolume(
            side,
            state.location,
            state.image,
            state.volumeIndex,
            state.dirBlock,
            state.trail,
            state.host
          );
        } else {
          await openHdf(side, state.location, state.host);
        }
      }
    },
    [pane, openLocal, openAdf, openHdf, openVolume, openIso, openArchive, openCbm]
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
      } else if (result.kind === "archive_out") {
        // An archive's own report: it counts entries the gate refused by
        // name and entries whose declared size was a lie, neither of which a
        // volume's report has a field for.
        const { report } = result;
        setMessage(t("files.status.filesWrittenOut", { count: report.total_files }));
        if (report.aborted && report.abort_reason) {
          setError(report.abort_reason);
        } else if (report.errors.length > 0) {
          setError(report.errors.slice(0, 3).join(" · "));
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
   * All four Amiga-volume directions work, because a volume is a volume
   * whether it came from a floppy or a partition two gigabytes into a hard
   * disk, and both directions out of a disc work too — the whole point of
   * Task 3:
   *
   * ```text
   * folder → volume    planned first, then one journalled write per file
   * volume → folder    with .uaem sidecars for what NTFS cannot hold
   * volume → volume    staged through a temp folder, verified at both ends
   * folder → folder    refused; that is what Explorer is for
   * disc   → volume    IsoSource through the same copy_into_volume
   * disc   → folder    isoExtract/isoExtractFile, `core::iso::IsoImage`
   * *      → disc      refused; a disc is read-only (see `copyDirection`)
   * ```
   */
  const copyTo = useCallback(
    async (from: Side, entry: PanelEntry) => {
      const to: Side = from === "left" ? "right" : "left";
      const source = pane(from);
      const target = pane(to);

      setError(null);
      setHint(null);
      setMessage(null);

      // `copyDirection` (`@/lib/isoPane`) is the routing: which pipeline a
      // source/target pane pair needs, and the one place that knows every
      // direction into a disc is refused. `"local-to-local"` and
      // `"refused"` are handled right here; the two `iso-*` directions get
      // their own blocks below, before the untouched ADF/HDF logic — a disc
      // never reaches that code, because both directions it can take are
      // resolved and returned before it would.
      const direction = copyDirection(source.kind, target.kind);

      if (direction.kind === "refused") {
        setError(t(direction.reason.key));
        return;
      }
      if (direction.kind === "local-to-local") {
        setHint(t("files.err.bothLocal"));
        return;
      }

      // ---- out of a disc, into a volume ----
      if (direction.kind === "iso-to-volume") {
        const destination = writableVolume(target);
        if (!destination) {
          setError(writeRefusal(target, t));
          return;
        }
        // The picked row carries its own address now, so the pane's open
        // directory is no longer part of what gets copied.
        if (entry.iso_extent === null) return;
        // A subfolder copies its subtree, a file copies exactly itself:
        // Rust decides which from `is_dir`, so a single picked file no
        // longer drags the whole open folder across with it — on an install
        // CD that was hundreds of megabytes while the status line named one
        // file.
        setBusy(t("files.status.copying", { name: entry.name }));
        try {
          pendingCopy.current = await isoCopyToVolume(
            source.location,
            entry.iso_extent,
            entry.bytes,
            entry.name,
            entry.is_dir,
            entry.date,
            destination.path,
            destination.volumeIndex,
            destination.dirBlock,
            { overwrite: policy }
          );
          copyDestination.current = to;
        } catch (e) {
          setError(String(e));
          setBusy(null);
        }
        return;
      }

      // ---- out of a disc, to the user's disk ----
      if (direction.kind === "iso-to-local") {
        if (entry.iso_extent === null) return;

        if (entry.is_dir) {
          setBusy(t("files.status.copyingOut", { name: entry.name }));
          try {
            pendingCopy.current = await isoExtract(
              source.location,
              entry.iso_extent,
              entry.bytes,
              target.location,
              entry.name,
              { overwrite: policy }
            );
            copyDestination.current = to;
          } catch (e) {
            setError(String(e));
            setBusy(null);
          }
          return;
        }

        setBusy(t("files.status.copying", { name: entry.name }));
        try {
          const outcome = await isoExtractFile(
            source.location,
            entry.iso_extent,
            entry.bytes,
            entry.name,
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
        return;
      }

      // ---- out of a Commodore image, to the user's disk ----
      //
      // Flat media, so there is no folder to pick: a row is a file, and F5 on
      // one copies that file. The Commodore type becomes the extension on the
      // way out (Rust decides that), because a folder of extensionless files
      // called `LOADER` helps nobody.
      if (direction.kind === "c64-to-local") {
        setBusy(t("files.status.copying", { name: entry.name }));
        try {
          const outcome = await cbmExtractFile(source.location, entry.name, target.location);
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
        return;
      }

      // ---- out of an archive, into a volume ----
      //
      // The direction this pane exists for: a `.lha`, `.zip` or `.7z` on the
      // user's disk, its contents on an Amiga volume. Rust unpacks the chosen
      // folder into a scratch directory and hands it to the Stage W writer,
      // which is what installing a downloaded package already does — so
      // there is no second copy engine and no second set of guarantees.
      if (direction.kind === "archive-to-volume") {
        const destination = writableVolume(target);
        if (!destination) {
          setError(writeRefusal(target, t));
          return;
        }
        if (source.archiveDir === null) return;

        setBusy(t("files.status.copying", { name: entry.name }));
        try {
          pendingCopy.current = await archiveCopyToVolume(
            source.location,
            source.archiveDir,
            entry.name,
            destination.path,
            destination.volumeIndex,
            destination.dirBlock,
            { overwrite: policy }
          );
          copyDestination.current = to;
        } catch (e) {
          setError(String(e));
          setBusy(null);
        }
        return;
      }

      // ---- out of an archive, to the user's disk ----
      if (direction.kind === "archive-to-local") {
        if (source.archiveDir === null) return;

        if (entry.is_dir) {
          setBusy(t("files.status.copyingOut", { name: entry.name }));
          try {
            pendingCopy.current = await archiveExtract(
              source.location,
              source.archiveDir,
              entry.name,
              target.location,
              { overwrite: policy }
            );
            copyDestination.current = to;
          } catch (e) {
            setError(String(e));
            setBusy(null);
          }
          return;
        }

        setBusy(t("files.status.copying", { name: entry.name }));
        try {
          const outcome = await archiveExtractFile(
            source.location,
            source.archiveDir,
            entry.name,
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
            setPlan({ plan: found, sources: [entry.path], names: [entry.name], side: to });
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
            target.location,
            entry.name,
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

  /**
   * Run the copy the plan dialog just had confirmed.
   *
   * One source keeps the exact call `volumeCopyIn` has always made — a
   * folder's *contents* land flat, the tested behaviour nothing here may
   * change. More than one goes through `volumeCopyInMany`, where each root
   * keeps its own name at the destination instead (`HostSelection`). A batch
   * of `.lha` archives — `pending.drawers` set — goes through
   * `archivesInstall` instead of either: it is not a file copy, it is
   * several archives each unpacked into its own drawer.
   */
  async function runPlannedCopy() {
    const pending = plan;
    if (!pending) return;

    const target = writableVolume(pane(pending.side));
    setPlan(null);
    if (!target) {
      setError(writeRefusal(pane(pending.side), t));
      return;
    }

    setBusy(
      pending.drawers
        ? t("files.status.copyingArchives", { count: pending.drawers.length })
        : pending.names.length === 1
          ? t("files.status.copying", { name: pending.names[0] })
          : t("files.status.copyingSelection", { count: pending.names.length })
    );
    try {
      pendingCopy.current = pending.drawers
        ? await archivesInstall(
            pending.sources,
            target.path,
            target.volumeIndex,
            target.dirBlock,
            policy
          )
        : pending.sources.length === 1
          ? await volumeCopyIn(
              target.path,
              target.volumeIndex,
              target.dirBlock,
              pending.sources[0],
              { overwrite: policy, sidecars: powerMode }
            )
          : await volumeCopyInMany(
              target.path,
              target.volumeIndex,
              target.dirBlock,
              pending.sources,
              { overwrite: policy, sidecars: powerMode }
            );
      copyDestination.current = pending.side;
    } catch (e) {
      setError(String(e));
      setBusy(null);
    }
  }

  /**
   * F5 on a multi-selection — copy every selected entry from `from` at once.
   *
   * One entry is exactly `copyTo` (called with an array of one), so the
   * tested single-entry path never changes. More than one:
   *
   * ```text
   * local pick → volume    one atomic operation: plan-many, then copy-in-many
   * volume pick → local    each entry its own extract, run together
   * volume pick → volume   not supported yet — a runtime refusal, the same
   *                        shape as "both panes are local" below, rather
   *                        than a disabled key (§96: F5 still *runs*)
   * ```
   */
  async function copySelectionTo(from: Side, entries: PanelEntry[]) {
    if (entries.length === 0) return;
    if (entries.length === 1) {
      await copyTo(from, entries[0]);
      return;
    }

    const to: Side = from === "left" ? "right" : "left";
    const source = pane(from);
    const target = pane(to);

    setError(null);
    setHint(null);
    setMessage(null);

    if (source.kind === "local" && target.kind === "local") {
      setHint(t("files.err.bothLocal"));
      return;
    }

    // ---- a local pick, into a volume: one atomic operation ----
    if (source.kind === "local" && target.kind !== "local") {
      const destination = writableVolume(target);
      if (!destination) {
        setError(writeRefusal(target, t));
        return;
      }

      const paths = entries.map((entry) => entry.path).filter((p): p is string => Boolean(p));
      if (paths.length !== entries.length) {
        setError(t("files.err.noLocalPath"));
        return;
      }

      // A selection of `.lha` archives is not a file copy: each one is
      // unpacked and given its own drawer, not merged into the destination
      // flat. A mix of archives and ordinary files is refused rather than
      // guessed at — silently treating the archives as plain files would
      // copy their raw `.lha` bytes onto the disk instead of installing them.
      const archiveCount = paths.filter(isArchivePath).length;
      if (archiveCount > 0 && archiveCount !== paths.length) {
        setError(t("files.err.mixedArchiveSelection"));
        return;
      }

      setBusy(
        archiveCount > 0
          ? t("files.status.planningArchives", { count: entries.length })
          : t("files.status.planningSelection", { count: entries.length })
      );
      try {
        if (archiveCount > 0) {
          const found = await archivesPlanInstall(
            paths,
            destination.path,
            destination.volumeIndex,
            destination.dirBlock
          );
          setPlan({
            plan: found.cost,
            sources: paths,
            names: entries.map((entry) => entry.name),
            side: to,
            drawers: found.drawers,
          });
        } else {
          const found = await volumePlanCopyMany(
            destination.path,
            destination.volumeIndex,
            destination.dirBlock,
            paths
          );
          setPlan({
            plan: found,
            sources: paths,
            names: entries.map((entry) => entry.name),
            side: to,
          });
        }
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(null);
      }
      return;
    }

    // ---- a Commodore pick, out to the user's disk: one job for the lot ----
    //
    // Unlike the volume case below, this is a single job rather than one per
    // entry: the image is small enough to walk once (a D81 is 800 KB), the
    // rows are files with no folders among them, and one job means one
    // progress bar and one Stop that actually stops everything.
    if (source.kind === "c64" && target.kind === "local") {
      setBusy(t("files.status.copyingSelectionOut", { count: entries.length }));
      try {
        const outcome = await runJob(() =>
          cbmExtract(
            source.location,
            entries.map((entry) => entry.name),
            target.location,
            { overwrite: policy }
          )
        );
        setMessage(
          outcome === "cancelled"
            ? t("files.status.selectionCopyOutCancelled", {
                done: 0,
                total: entries.length,
              })
            : t("files.status.selectionCopiedOut", { count: entries.length })
        );
        await refresh(to);
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(null);
      }
      return;
    }

    // ---- a volume pick, out to the user's disk: each entry its own extract ----
    if (source.kind !== "local" && target.kind === "local" && source.volumeIndex !== null) {
      const volumeIndex = source.volumeIndex;
      setBusy(t("files.status.copyingSelectionOut", { count: entries.length }));
      try {
        // Each outcome is tracked, not just awaited: a job the user cancelled
        // partway through must not be folded into the same success message
        // as one that actually finished (finding 3 of the phase-1a
        // whole-branch review — `runJob` resolving "cancelled" as success was
        // half the defect; reporting it honestly here is the other half).
        const outcomes = await Promise.all(
          entries
            .filter((entry) => entry.header_block !== null)
            .map(async (entry): Promise<JobOutcome> => {
              const headerBlock = entry.header_block as number;
              if (entry.is_dir) {
                return runJob(() =>
                  volumeCopyOut(
                    source.location,
                    volumeIndex,
                    headerBlock,
                    target.location,
                    entry.name,
                    { overwrite: policy, sidecars: powerMode }
                  )
                );
              }
              await volumeExtractTo(source.location, volumeIndex, headerBlock, target.location);
              return "finished";
            })
        );
        const cancelled = outcomes.filter((outcome) => outcome === "cancelled").length;
        if (cancelled > 0) {
          setMessage(
            t("files.status.selectionCopyOutCancelled", {
              done: outcomes.length - cancelled,
              total: outcomes.length,
            })
          );
        } else {
          setMessage(t("files.status.selectionCopiedOut", { count: entries.length }));
        }
        await refresh(to);
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(null);
      }
      return;
    }

    // Two volumes and more than one entry: not supported yet.
    setError(t("files.err.batchBetweenVolumes"));
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
    setHint(null);
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
   * F8 on a multi-selection — delete every selected entry from a volume as
   * one operation.
   *
   * One entry is exactly `deleteEntry` (called with an array of one). More
   * than one goes through `volumeDeleteMany`: all-or-nothing (§92), so a
   * batch that cannot fully succeed removes nothing rather than leaving the
   * user unsure which half of their selection is still there. The two
   * confirmations mirror `deleteEntry`'s, naming the count and total size
   * instead of one file — no per-entry icon offer, since that prompt does
   * not scale to a batch and the icon stays selectable on its own.
   */
  async function deleteSelection(side: Side, entries: PanelEntry[]) {
    if (entries.length === 0) return;
    if (entries.length === 1) {
      await deleteEntry(side, entries[0]);
      return;
    }

    const state = pane(side);
    const target = writableVolume(state);
    if (!target) return;

    const names = entries.map((entry) => entry.name);
    const totalBytes = entries.reduce((sum, entry) => sum + entry.bytes, 0);

    if (
      !window.confirm(
        t("files.dialog.deleteMany.confirm1", {
          count: names.length,
          volume: state.volumeName,
          size: formatBytes(totalBytes),
        })
      )
    ) {
      return;
    }
    if (!window.confirm(t("files.dialog.deleteMany.confirm2", { count: names.length }))) {
      return;
    }

    setError(null);
    setHint(null);
    setBusy(t("files.status.deletingMany", { count: names.length }));
    try {
      const outcome = await volumeDeleteMany(
        target.path,
        target.volumeIndex,
        target.dirBlock,
        names
      );
      setMessage(
        outcome.backup
          ? t("files.status.deletedManyBackedUp", { count: outcome.deleted })
          : t("files.status.deletedMany", { count: outcome.deleted })
      );
      await refresh(side);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  /**
   * F6 — move the marked entries to the other pane.
   *
   * The one function key that can destroy the original, so it runs §92's
   * pipeline in full and in that order:
   *
   * ```text
   * VALIDATE   planMove — every case where a move could end with less data
   *            than it started with, decided before a byte moves
   * RECOMMEND  the icons left behind, offered rather than assumed (§7.1)
   * PREVIEW    one confirmation naming what moves and where
   * APPLY      the copy, through the exact commands F5 already uses
   * VERIFY     the destination is re-listed and every name looked for
   * REPORT     ...and only then is anything deleted
   * ```
   *
   * The VERIFY step is the point. A copy that reported success and a
   * destination that does not actually hold the file are the same thing as
   * far as the user's data is concerned, and the delete that follows is what
   * makes the difference matter. Nothing is deleted unless the destination's
   * own listing names every entry that was supposed to land in it — a
   * cancelled copy, a job that failed, a name the writer refused all stop
   * here, leaving a plain copy behind and saying so.
   *
   * A cancelled move is therefore always safe: the worst it can leave is a
   * duplicate.
   */
  async function moveSelection(from: Side, entries: PanelEntry[]) {
    const to: Side = from === "left" ? "right" : "left";
    const source = pane(from);
    const target = pane(to);

    setError(null);
    setHint(null);
    setMessage(null);

    const volume = writableVolume(source);
    if (!volume) {
      setError(writeRefusal(source, t));
      return;
    }

    // §7.1: Workbench shows an object only when its `.info` sits beside it, so
    // moving `Game` and leaving `Game.info` behind gives a disk that looks
    // right here and has an invisible game on a real Amiga — the same failure
    // §82's install exists to prevent. Offered, not done silently: the user
    // may be moving the icon on purpose.
    const marked = new Set(entries.map((entry) => entry.name.toLowerCase()));
    const icons: PanelEntry[] = [];
    for (const entry of entries) {
      const icon = await volumeIconFor(
        volume.path,
        volume.volumeIndex,
        volume.dirBlock,
        entry.name
      ).catch(() => null);
      if (!icon || marked.has(icon.icon_name.toLowerCase())) continue;
      const row = source.entries.find((candidate) => candidate.name === icon.icon_name);
      if (row) icons.push(row);
    }

    let moving = entries;
    if (
      icons.length > 0 &&
      window.confirm(
        t("files.dialog.move.confirmIcons", {
          count: icons.length,
          names: icons.slice(0, 3).map((icon) => icon.name).join(", "),
        })
      )
    ) {
      moving = [...entries, ...icons];
    }

    // Re-planned over what is *actually* about to move — the icons the user
    // just agreed to bring along have names of their own, and one of those
    // colliding at the destination is the same refusal as any other.
    const plan = planMove({
      sourceKind: source.kind,
      targetKind: target.kind,
      sourceWritable: true,
      targetWritable: writableVolume(target) !== null,
      entries: moving.map((entry) => ({ name: entry.name, isDir: entry.is_dir })),
      takenNames: target.entries.map((entry) => entry.name),
    });
    if (plan.kind === "refused") {
      setError(t(plan.reason.key, plan.reason.params));
      return;
    }

    if (
      !window.confirm(
        t("files.dialog.move.confirm", {
          count: moving.length,
          source: source.volumeName || source.location,
          destination: target.volumeName || target.location,
        })
      )
    ) {
      return;
    }

    setBusy(t("files.status.moving", { count: moving.length }));
    try {
      // ---- APPLY: the copy half, through the commands F5 already uses ----
      if (target.kind === "local") {
        for (const entry of moving) {
          if (entry.header_block === null) continue;
          if (entry.is_dir) {
            const outcome = await runJob(() =>
              volumeCopyOut(
                volume.path,
                volume.volumeIndex,
                entry.header_block as number,
                target.location,
                entry.name,
                { overwrite: policy, sidecars: powerMode }
              )
            );
            if (outcome === "cancelled") {
              setMessage(t("files.status.moveCancelled"));
              await refresh(to);
              return;
            }
          } else {
            await volumeExtractTo(
              volume.path,
              volume.volumeIndex,
              entry.header_block,
              target.location
            );
          }
        }
      } else {
        // Exactly one directory, and `planMove` is what guarantees that:
        // `volume_copy_between` addresses a *directory*, so anything else
        // would copy more than was marked.
        const destination = writableVolume(target);
        const entry = moving[0];
        if (!destination || entry.header_block === null) {
          setError(writeRefusal(target, t));
          return;
        }
        const outcome = await runJob(() =>
          volumeCopyBetween(
            volume.path,
            volume.volumeIndex,
            entry.header_block as number,
            destination.path,
            destination.volumeIndex,
            destination.dirBlock,
            { overwrite: policy, sidecars: powerMode }
          )
        );
        if (outcome === "cancelled") {
          setMessage(t("files.status.moveCancelled"));
          await refresh(to);
          return;
        }
      }

      // ---- VERIFY: the destination's own listing, not the copy's word ----
      const landed = await destinationNames(target);
      const missing = moving.filter((entry) => !landed.has(entry.name.toLowerCase()));
      if (missing.length > 0) {
        setError(
          t("files.status.moveNotVerified", {
            count: missing.length,
            names: missing.slice(0, 3).map((entry) => entry.name).join(", "),
          })
        );
        await refresh(to);
        return;
      }

      // ---- and only now the delete half ----
      const names = moving.map((entry) => entry.name);
      const outcome =
        names.length === 1 && moving[0].header_block !== null
          ? await volumeDelete(
              volume.path,
              volume.volumeIndex,
              volume.dirBlock,
              moving[0].header_block
            )
          : await volumeDeleteMany(volume.path, volume.volumeIndex, volume.dirBlock, names);

      setMessage(
        outcome.backup
          ? t("files.status.movedBackedUp", { count: names.length })
          : t("files.status.moved", { count: names.length })
      );
      await refresh(from);
      await refresh(to);
    } catch (e) {
      setError(String(e));
      // Whatever failed, the panes are the truth about what is where now.
      await refresh(from);
      await refresh(to);
    } finally {
      setBusy(null);
    }
  }

  /**
   * Every name a pane's destination directory holds right now, lowercased.
   *
   * Read back from the source of truth rather than from `state.entries`,
   * which is a snapshot taken before the copy ran — the whole point of the
   * VERIFY step is to ask the filesystem, not to trust what ART already
   * believed.
   */
  async function destinationNames(target: PaneState): Promise<Set<string>> {
    if (target.kind === "local") {
      const listing = await panelListLocal(target.location);
      return new Set(listing.entries.map((entry) => entry.name.toLowerCase()));
    }
    if (target.volumeIndex === null) return new Set();
    const listing = await volumeList(target.location, target.volumeIndex, target.dirBlock);
    return new Set(listing.entries.map((entry) => entry.name.toLowerCase()));
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
    setHint(null);
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
    setHint(null);
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
    setHint(null);
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

  /**
   * Enter on the command line.
   *
   * Every branch acts on the *focused* pane, the same source every F-key
   * reads — a command line that navigated whichever pane it felt like would
   * be worse than none. The line is cleared only when something actually
   * happened; a refusal leaves the text where the user can see, and fix,
   * what they typed.
   */
  async function runCommandLine() {
    const action = parseCommandLine(commandLine);
    setError(null);
    setHint(null);

    switch (action.kind) {
      case "none":
        return;
      case "refused":
        setError(t(action.reason.key, action.reason.params));
        return;
      case "filter":
        setPaneFilter(focused, action.mask);
        setCommandLine("");
        return;
      case "up":
        setCommandLine("");
        await goUp(focused);
        return;
      case "open":
        setCommandLine("");
        await openLocal(focused, action.path);
        return;
    }
  }

  /**
   * Num + / Num − — mark or unmark by filename mask.
   *
   * The same `*`/`?` matcher the filter box uses, so a user who has learned
   * one has learned both, and it *adds to* the selection rather than
   * replacing it — see `markByMask` for why that matters.
   */
  async function markBy(mark: boolean) {
    const mask = window.prompt(
      mark ? t("files.dialog.markMask.mark") : t("files.dialog.markMask.unmark"),
      "*.*"
    );
    if (!mask) return;
    applySelection(focused, markByMask(paneEntries(focused), selection[focused], mask, mark));
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
      sortedEntries: paneEntries(side),
      sort: sort[side],
      onSortChange: (column: SortColumn) =>
        setSort((s) => ({ ...s, [side]: clickColumn(s[side], column) })),
      filter: filter[side],
      onFilterChange: (mask: string) => setPaneFilter(side, mask),
      roots,
      sourceOptions,
      sourceValue: currentPaneSourceValue(state.kind, state.location, roots),
      // One place decides what a combo value means (`@/lib/paneSources`), and
      // it refuses anything it does not recognise rather than guessing — so
      // re-picking the "not on a listed drive" placeholder navigates nowhere
      // instead of dropping the pane on the first mount in the list.
      sourceRef: sourceCombos[side],
      onChooseSource: (value: string) => {
        const choice = parsePaneSource(value);
        if (!choice) return;
        if (choice.kind === "root") void openLocal(side, choice.path);
        else if (choice.kind === "folder") void chooseFolder(side);
        else void chooseImage(side, choice.image);
      },
      showSourceButtons,
      selectedNames: selection[side],
      cursorName: anchor[side],
      focused: focused === side,
      powerMode,
      onFocus: () => setFocused(side),
      // Plain click selects only this entry; Ctrl/Cmd+click toggles it and
      // keeps the rest; Shift+click extends the range from the anchor. A
      // click always focuses this pane too — the row's own onClick does not
      // stopPropagation, so it bubbles to the pane's onClick={onFocus} — but
      // the per-row Copy/Delete buttons below do stopPropagation and must
      // focus explicitly instead.
      onSelect: (entry: PanelEntry, event: { shiftKey: boolean; ctrlKey: boolean; metaKey: boolean }) => {
        const entries = paneEntries(side);
        const update = event.shiftKey
          ? selectRange(entries, selection[side], anchor[side], entry.name)
          : event.ctrlKey || event.metaKey
            ? toggleOne(selection[side], entry.name)
            : selectOnly(entry.name);
        applySelection(side, update);
      },
      onActivate: (entry: PanelEntry) => void activate(side, entry),
      onUp: () => void goUp(side),
      onOpenFolder: () => void chooseFolder(side),
      onOpenImage: (kind: "adf" | "hdf" | "iso" | "archive" | "c64") =>
        void chooseImage(side, kind),
      onOpenRoot: (root: string) => void openLocal(side, root),
      onOpenVolume: (index: number) => {
        // Entering a partition from the list: still inside the same image, so
        // the host it was entered from travels with it — `[..]` out of the
        // partition goes to the list, and `[..]` again leaves the image.
        if (state.image) {
          void openVolume(side, state.location, state.image, index, null, [], state.host);
        }
      },
      onRefresh: () => void refresh(side),
      onNewFolder: () => void newFolder(side),
      onDragOut: (entry: PanelEntry) => void dragOut(entry),
      onDropped: (entry: PanelEntry, from: Side) => {
        if (from !== side) void copyTo(from, entry);
      },
    };
  }

  // ---- the function keys ----

  // The pane the keys act on: `focused`, tracked above — never derived from
  // `selection`. Every action below reads `focused` and `focusedPane`.
  const focusedPane = pane(focused);
  const canWrite = writableVolume(focusedPane) !== null;
  const inVolume = focusedPane.kind !== "local" && focusedPane.volumeIndex !== null;

  // The single-entry keys' availability and target come from `planFunctionKeys`
  // (`@/lib/functionKeyPlan`), not derived here, so the same "null when
  // anything but exactly one entry is selected" answer — never "the first of
  // several", which would be the kind of guess that destroys the wrong file —
  // is what both `enabled` and `run` below read, and is what
  // `functionKeyPlan.test.ts` exercises without rendering this page.
  const keyPlan = planFunctionKeys({
    entries: paneEntries(focused),
    selected: selection[focused],
    inVolume,
    canWrite,
    busy: busy !== null,
  });
  const { multipleSelected, hasSelection } = keyPlan;

  // F6 (Move) needs both panes, not one, so it is planned here rather than in
  // `planFunctionKeys`: what it is allowed to do depends on what the *other*
  // pane is and on what it already holds. `takenNames` comes from the
  // destination's unfiltered `entries` — a filename mask hiding a colliding
  // name must not make the collision invisible.
  const moveTarget = pane(focused === "left" ? "right" : "left");
  const movePlan = planMove({
    sourceKind: focusedPane.kind,
    targetKind: moveTarget.kind,
    sourceWritable: canWrite,
    targetWritable: writableVolume(moveTarget) !== null,
    entries: selectedEntries(focused).map((entry) => ({
      name: entry.name,
      isDir: entry.is_dir,
    })),
    takenNames: moveTarget.entries.map((entry) => entry.name),
  });

  const actions: FunctionAction[] = [
    {
      key: "F3",
      label: t("files.functionKeys.view"),
      enabled: keyPlan.f3.enabled,
      reason: multipleSelected
        ? t("files.functionKeys.reasonMultiple")
        : inVolume
          ? t("files.functionKeys.viewReasonSelect")
          : t("files.functionKeys.viewReasonNeedsImage"),
      run: () => {
        const target = keyPlan.f3.target;
        if (!target || target.header_block === null || focusedPane.volumeIndex === null) {
          return;
        }
        setViewing({
          path: focusedPane.location,
          volumeIndex: focusedPane.volumeIndex,
          entryBlock: target.header_block,
          name: target.name,
        });
      },
    },
    {
      key: "F4",
      label: t("files.functionKeys.edit"),
      enabled: keyPlan.f4.enabled,
      reason: multipleSelected
        ? t("files.functionKeys.reasonMultiple")
        : canWrite
          ? t("files.functionKeys.editReasonSelect")
          : writeRefusal(focusedPane, t),
      run: () => {
        if (keyPlan.f4.target) void editEntry(focused, keyPlan.f4.target);
      },
    },
    {
      key: "F5",
      label: t("files.functionKeys.copy"),
      enabled: hasSelection && busy === null,
      reason: t("files.functionKeys.copyReasonSelect"),
      run: () => {
        const entries = selectedEntries(focused);
        if (entries.length > 0) void copySelectionTo(focused, entries);
      },
    },
    {
      // F6 is Move, Total Commander's own semantics and what twenty years of
      // this user's muscle memory expects. Its rules — every case where a
      // move could end with less data than it started with — are
      // `planMove`'s (`@/lib/movePlan`), computed once above and read here for
      // both `enabled` and the reason on hover, so a refusal is never
      // discovered halfway through.
      key: "F6",
      label: t("files.functionKeys.move"),
      hint: t("files.functionKeys.moveHintRename"),
      enabled: movePlan.kind === "move" && busy === null,
      reason:
        movePlan.kind === "refused"
          ? t(movePlan.reason.key, movePlan.reason.params)
          : t("files.functionKeys.moveReasonBusy"),
      run: () => {
        const entries = selectedEntries(focused);
        if (entries.length > 0) void moveSelection(focused, entries);
      },
    },
    {
      // Shift+F6 — rename in place. Keyboard only: `FunctionKeyBar` renders
      // the seven keys the bar has always had, and this one is named in F6's
      // own tooltip instead of growing a second row.
      key: "F6",
      shift: true,
      label: t("files.functionKeys.rename"),
      enabled: keyPlan.shiftF6.enabled,
      reason: multipleSelected
        ? t("files.functionKeys.reasonMultiple")
        : canWrite
          ? t("files.functionKeys.renameReasonSelect")
          : writeRefusal(focusedPane, t),
      run: () => {
        if (keyPlan.shiftF6.target) void renameEntry(focused, keyPlan.shiftF6.target);
      },
    },
    {
      key: "F7",
      label: t("files.functionKeys.newFolder"),
      enabled: canWrite && busy === null,
      reason: writeRefusal(focusedPane, t),
      run: () => void newFolder(focused),
    },
    {
      key: "F8",
      label: t("files.functionKeys.delete"),
      enabled: hasSelection && canWrite && busy === null,
      reason: canWrite ? t("files.functionKeys.deleteReasonSelect") : writeRefusal(focusedPane, t),
      danger: true,
      run: () => {
        const entries = selectedEntries(focused);
        if (entries.length > 0) void deleteSelection(focused, entries);
      },
    },
    {
      key: "F9",
      label: t("files.functionKeys.attributes"),
      enabled: keyPlan.f9.enabled,
      reason: multipleSelected
        ? t("files.functionKeys.reasonMultiple")
        : inVolume
          ? t("files.functionKeys.attributesReasonSelect")
          : t("files.functionKeys.attributesReasonNeedsImage"),
      run: () => {
        const target = keyPlan.f9.target;
        if (!target || target.header_block === null || focusedPane.volumeIndex === null) {
          return;
        }
        setAttributes({
          path: focusedPane.location,
          volumeIndex: focusedPane.volumeIndex,
          entryBlock: target.header_block,
        });
      },
    },
  ];

  // Off while a dialog is open: F8 behind a modal would delete something the
  // user cannot see.
  const keysActive = plan === null && viewing === null && attributes === null && recovery === null;
  useFunctionKeys(actions, keysActive);

  // Tab swaps which pane is focused. Same gate as the function keys — Tab
  // underneath a modal should not silently change which pane a background
  // F-key would land on — and the same input/textarea/modifier guard, shared
  // via `usePaneTab` itself.
  usePaneTab(() => setFocused((side) => (side === "left" ? "right" : "left")), keysActive);

  // Insert marks the entry under the pane's selection anchor and steps the
  // anchor down one (Norton Commander's mark key); Ctrl+A marks everything
  // in the focused pane, and clears it again on a second press. Both act on
  // `focused`, same as every F-key above, and share the F-keys' gate so a
  // modal on top cannot be marked through.
  useInsertToggle(() => {
    applySelection(focused, insertToggle(paneEntries(focused), selection[focused], anchor[focused]));
  }, keysActive);
  useSelectAll(() => {
    applySelection(focused, toggleSelectAll(paneEntries(focused), selection[focused]));
  }, keysActive);

  // F2 / Ctrl+R re-read the focused pane. Task 5's job, brought forward
  // because task 3 hides the button strip Refresh used to live in — see
  // `useRefreshKey`'s own comment for why that could not wait a task.
  useRefreshKey(() => void refresh(focused), keysActive);

  // Enter / Ctrl+PgDn walk into whatever the cursor is on — a folder, or an
  // image, which becomes this pane. Backspace / Ctrl+PgUp walk back out,
  // container boundaries included (brief §3.1).
  //
  // The cursor, not the selection: Total Commander's Enter opens the row you
  // are standing on, whether or not it is marked, and a user who has five
  // files marked for F5 and presses Enter means "open this one", not "open
  // the first of those five".
  useNavigationKeys(
    {
      onOpen: () => {
        const name = anchor[focused];
        if (!name) return;
        const entry = paneEntries(focused).find((candidate) => candidate.name === name);
        if (entry) void activate(focused, entry);
      },
      // Backspace shortens a running search before it means "up one level" —
      // Total Commander's own precedence, and the only sane one: a user
      // half-way through typing a name who mistypes expects to fix it, not to
      // be thrown into the parent directory.
      onUp: () => {
        if (search !== "") {
          const step = shortenSearch(paneNames(focused), search);
          noteSearch(step.prefix);
          if (step.match) moveCursor(focused, step.match);
          return;
        }
        void goUp(focused);
      },
    },
    keysActive
  );

  // Space marks where you stand; the numpad marks by mask and inverts.
  useMarkKeys(
    {
      onSpace: () => applySelection(focused, spaceToggle(selection[focused], anchor[focused])),
      onMarkByMask: () => void markBy(true),
      onUnmarkByMask: () => void markBy(false),
      onInvert: () =>
        applySelection(focused, invertSelection(paneEntries(focused), selection[focused])),
    },
    keysActive
  );

  // Letters move the cursor to the next matching name. The cursor only — a
  // search must never change what is marked, or typing a name would quietly
  // throw away a selection the user spent a minute building.
  useTypeAhead(
    {
      onCharacter: (character) => {
        const step = extendSearch(paneNames(focused), search, character, anchor[focused]);
        if (!step.accepted) return;
        noteSearch(step.prefix);
        if (step.match) moveCursor(focused, step.match);
      },
      onEscape: () => noteSearch(""),
    },
    keysActive
  );

  // Alt+F1 / Alt+F2 — the last mouse-only affordance left on the screen once
  // the button strip went behind a setting.
  useSourceComboKeys(
    {
      onLeft: () => sourceCombos.left.current?.focus(),
      onRight: () => sourceCombos.right.current?.focus(),
    },
    keysActive
  );

  // Alt+Left / Alt+Right — the focused pane's own history, container steps
  // included: going back into `Lotus.adf` reopens the image at the directory
  // it was left at, because that is what a location *is* here.
  usePaneHistoryKeys(
    {
      onBack: () => {
        const step = goBack(history[focused]);
        if (!step) return;
        setHistory((current) => ({ ...current, [focused]: step.history }));
        void openLocation(focused, step.to);
      },
      onForward: () => {
        const step = goForward(history[focused]);
        if (!step) return;
        setHistory((current) => ({ ...current, [focused]: step.history }));
        void openLocation(focused, step.to);
      },
    },
    keysActive
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
    // Full-bleed: the commander *is* the window (brief §1.1). No page title
    // and no explainer paragraph — Total Commander needs neither, and the
    // room they took is room the panes now have.
    <div className="app-content-full">

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

      {/*
       * The Total Commander presentation (task 6b) lives entirely inside
       * `.tc-commander` — see `src/pages/FileManager.css`'s header for how
       * that scoping works and why the rest of the app never sees it. Only
       * this wrapper and its descendants read the `--tc-*` custom
       * properties; nothing outside it does either. Since task 3 the only
       * thing left outside is the recovery card — a decision that blocks
       * every write until it is made, so it is deliberately not chrome —
       * plus the modals, which are the app's own dialogs. The error, message
       * and busy lines used to sit out here and push the panes down; they are
       * one status strip inside the dock now.
       */}
      <div className="tc-commander">
        {/* `minmax(0, 1fr)` twice and `align-items: stretch` (in the CSS, not
            here) are the height contract: the two panes are the same height
            because the grid says so, not because their contents happen to
            match. A content-sized flex row is what let the right pane come up
            short in the first screenshot. */}
        <div className="tc-pane-grid">
          <Pane {...paneProps("left")} />

          <div className="tc-transfer-buttons">
            <button
              className="btn"
              title={t("files.arrows.toRightTitle")}
              disabled={selection.left.size === 0 || busy !== null}
              onClick={() => void copySelectionTo("left", selectedEntries("left"))}
            >
              &rarr;
            </button>
            <button
              className="btn"
              title={t("files.arrows.toLeftTitle")}
              disabled={selection.right.size === 0 || busy !== null}
              onClick={() => void copySelectionTo("right", selectedEntries("right"))}
            >
              &larr;
            </button>
          </div>

          <Pane {...paneProps("right")} />
        </div>

        {/* The files currently checked out for editing (F4). Renders nothing
            when there are none. Inside the commander, above the status strip,
            so the F-key row stays the last thing on the screen — brief §1.4
            asks for it docked to the window bottom, and a panel underneath it
            would be a strip that is not at the bottom. */}
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

        {/*
         * One status strip for the whole screen, docked with the rest of the
         * chrome rather than stacked above the panes (brief §1.4, §1.1).
         *
         * Three things used to push the panes down from the top of the page:
         * a red error banner, a green message banner and a "busy…" line. They
         * are one line now, and they are *inside* the commander — a message
         * that appears must never move the thing the user is looking at.
         *
         * Three levels, deliberately distinct (see `Refusal.tsx` for the same
         * distinction drawn elsewhere in ART): **busy** is what is happening,
         * **error** means something broke and carries an `ART-*` id,
         * **hint** is ART declining a question that broke nothing — "both
         * panes are local folders" is the second kind, and used to shout in
         * red. The separate "N items selected" bar is gone: the per-pane
         * status line already carries selected-of-total in Total Commander's
         * own format, which is where task 3 was told to put it.
         */}
        {(busy || error || hint || message) && (
          <div
            className={`tc-chrome-row tc-message-row${
              busy ? "" : error ? " tc-message-error" : hint ? " tc-message-hint" : " tc-message-ok"
            }`}
            role="status"
          >
            {busy ? t("files.busy.suffix", { busy }) : (error ?? hint ?? message)}
          </div>
        )}

        {/*
         * The command line (brief §1.4): full width, directly above the F-key
         * bar, always visible, and reflecting whichever pane is focused.
         *
         * It **navigates and filters**. It does not run programs — §56, and
         * `parseCommandLine` (`@/lib/commandLine`) says so out loud for
         * anything it will not do rather than swallowing the keystroke, which
         * is the one behaviour a prompt-shaped box must never have.
         */}
        <div className="tc-chrome-row tc-command-line">
          <span className="tc-command-prompt">{`${pane(focused).location}>`}</span>
          <input
            type="text"
            className="tc-command-input"
            value={commandLine}
            aria-label={t("files.commandLine.ariaLabel")}
            placeholder={t("files.commandLine.placeholder")}
            onChange={(event) => setCommandLine(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.preventDefault();
                event.stopPropagation();
                setCommandLine("");
                event.currentTarget.blur();
                return;
              }
              if (event.key !== "Enter") return;
              event.preventDefault();
              void runCommandLine();
            }}
          />
        </div>

        <div className="tc-chrome-row tc-fnkey-row">
          <FunctionKeyBar actions={actions} />
        </div>
      </div>

      {plan && (
        <CopyPlanDialog
          plan={plan.plan}
          names={plan.names}
          destination={`${pane(plan.side).volumeName || pane(plan.side).location}`}
          policy={policy}
          onPolicyChange={setPolicy}
          onConfirm={() => void runPlannedCopy()}
          onCancel={() => setPlan(null)}
          drawers={plan.drawers}
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
          onChanged={() => void refresh(focused)}
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
  const { t, i18n } = useTranslation();
  const capability = state.capability;
  // "ISO9660" is a format name, not a sentence — shown as-is, the same way
  // an ADF/HDF's "FFS INTL" filesystem string is never translated.
  const filesystem =
    state.kind === "iso"
      ? "ISO9660"
      : state.kind === "archive"
        ? // The format ART decided from the file's bytes, already uppercased
          // by `openArchive` — "ZIP", "LHA", "7Z".
          state.volumeName
        : state.kind === "c64"
          ? "CBM DOS"
        : capability?.filesystem ??
        (state.volumeIndex !== null
          ? state.image?.volumes[state.volumeIndex]?.filesystem
          : undefined) ??
        state.adf?.fs_type.toUpperCase() ??
        "";

  // Total Commander's drive row shows "free of total", in kilobytes grouped by
  // the active locale (`@/lib/tcFormat`). That row is gone by default in phase
  // 2b — the pane header is a combo, a path and a filter — so the pair moved
  // here, where the volume already says what it is. ART has the *total* only
  // for a volume it has actually opened, so a pane that knows its free space
  // but not its size still says so, in bytes, rather than showing a
  // fabricated total.
  const freeSpace =
    capability && state.totalBytes !== null
      ? t("files.tc.freeOfTotal", {
          free: formatGroupedSize(Math.round(capability.free_bytes / 1024), i18n.language),
          total: formatGroupedSize(Math.round(state.totalBytes / 1024), i18n.language),
        })
      : capability
        ? t("files.footer.free", { size: formatBytes(capability.free_bytes) })
        : null;

  return (
    <div className="tc-chrome-row tc-volume-row">
      <strong>{capability?.volume_name || state.volumeName || t("files.footer.unnamed")}</strong>
      <span>{filesystem}</span>
      {freeSpace && <span className="tc-drive-free">{freeSpace}</span>}

      {/* A disc has no `capability` to read `writable` off — it is read-only
          by construction, so the badge shows unconditionally rather than
          waiting on a fetch that would never happen (§8: never a pane that
          looks the same and quietly refuses everything). */}
      {(state.kind === "iso" ||
        state.kind === "archive" ||
        state.kind === "c64" ||
        (capability && !capability.writable)) && (
        <span
          className="badge badge-warn"
          style={{ fontSize: 10 }}
          title={
            state.kind === "iso"
              ? t(ISO_WRITE_REFUSAL.key)
              : state.kind === "archive"
                ? t(ARCHIVE_WRITE_REFUSAL.key)
                : state.kind === "c64"
                  ? t(C64_WRITE_REFUSAL.key)
                  : capability?.reason ?? t("files.writeRefusal.default")
          }
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
  sortedEntries,
  sort,
  onSortChange,
  filter,
  onFilterChange,
  roots,
  sourceOptions,
  sourceValue,
  onChooseSource,
  sourceRef,
  showSourceButtons,
  selectedNames,
  cursorName,
  focused,
  powerMode,
  onFocus,
  onSelect,
  onActivate,
  onUp,
  onOpenFolder,
  onOpenImage,
  onOpenRoot,
  onOpenVolume,
  onRefresh,
  onNewFolder,
  onDragOut,
  onDropped,
}: {
  side: Side;
  state: PaneState;
  /** `state.entries`, sorted for display — see `paneEntries` in
   * `FileManager` for why this and not `state.entries` is what the row list
   * below renders. */
  sortedEntries: PanelEntry[];
  sort: SortState;
  onSortChange: (column: SortColumn) => void;
  /** This pane's filename mask (`@/lib/mask`) — the `*.*` in the reference's
   * path row. Display-only: see `setPaneFilter` in `FileManager` for what a
   * change to it does to the selection. */
  filter: string;
  onFilterChange: (mask: string) => void;
  roots: string[];
  /** What the header's source combo offers (`@/lib/paneSources`). */
  sourceOptions: PaneSourceOption[];
  /** Which of them is the one this pane is showing, or `""` for a folder
   * under no enumerated mount. */
  sourceValue: string;
  onChooseSource: (value: string) => void;
  /** So Alt+F1 / Alt+F2 can open this pane's combo from the keyboard. */
  sourceRef: React.RefObject<HTMLSelectElement | null>;
  /** Whether the optional button strip is on (Settings, default off). */
  showSourceButtons: boolean;
  /** Every marked entry's name in this pane — see `@/lib/selection`. */
  selectedNames: Set<string>;
  /** The entry the mouse/keyboard last landed on: a future Shift+click's
   * range start, or Insert's "cursor". Highlighted distinctly from a
   * selected row so a user can tell the two apart at a glance. */
  cursorName: string | null;
  /** Whether this is the pane the keyboard (F-keys, Tab) is talking to. */
  focused: boolean;
  powerMode: boolean;
  onFocus: () => void;
  onSelect: (
    entry: PanelEntry,
    event: { shiftKey: boolean; ctrlKey: boolean; metaKey: boolean }
  ) => void;
  onActivate: (entry: PanelEntry) => void;
  onUp: () => void;
  onOpenFolder: () => void;
  onOpenImage: (kind: "adf" | "hdf" | "iso" | "archive" | "c64") => void;
  onOpenRoot: (root: string) => void;
  onOpenVolume: (index: number) => void;
  onRefresh: () => void;
  onNewFolder: () => void;
  onDragOut: (entry: PanelEntry) => void;
  onDropped: (entry: PanelEntry, from: Side) => void;
}) {
  const { t, i18n } = useTranslation();
  const [dragOver, setDragOver] = useState(false);

  const showingFiles = state.kind !== "hdf" || state.volumeIndex !== null;
  // Somewhere to go up *to* — one more level inside the container, or, at its
  // root, out of the image to the folder it was entered from (`state.host`).
  // That last clause is what gives a Commodore image, which is flat and has no
  // level of its own, a `[..]` at all: it is how you leave a `.d64` you walked
  // into. An image opened from the source combo has no host, so it correctly
  // shows none.
  const canGoUp =
    (state.kind === "local" && state.parent !== null) ||
    (state.kind === "adf" && state.trail.length > 0) ||
    (state.kind === "hdf" && state.volumeIndex !== null) ||
    (state.kind === "iso" && state.isoTrail.length > 0) ||
    (state.kind === "archive" && !!state.archiveDir) ||
    state.host !== null;

  // An HDF is never a destination: writing into a partition is not
  // implemented. A disc never is either — it is read-only end to end
  // (`copyDirection` in `@/lib/isoPane` is what actually refuses a drop
  // that lands here anyway; this only controls the drag-over affordance).
  const acceptsDrops = state.kind !== "hdf" && state.kind !== "iso";

  // The per-pane status line (row 5 of the reference): selected/total bytes,
  // then selected/total files, then selected/total directories. Reads
  // `sortedEntries`/`selectedNames` through the same pure `paneStatusCounts`
  // a unit test covers on its own (`@/lib/panelStatus`) — `[..]` is never
  // part of the count, since it is not a real entry.
  //
  // `sortedEntries` is `FileManager`'s `paneEntries(side)` — filtered by the
  // mask, then sorted — so this line counts the *filtered* view, not the
  // whole directory: Total Commander's own convention, and the only choice
  // that keeps the totals shown here matching what is actually listed above
  // them. A mask that hides seventeen of twenty files and a status line
  // still reading "20" would be the exact kind of mismatch task 7 exists to
  // avoid.
  const status = paneStatusCounts(sortedEntries, selectedNames);

  // Where this pane is, as a list of steps. Each kind keeps its position in a
  // field of its own (see `PaneState` for why `dirBlock`, `isoExtent` and
  // `archiveDir` are three fields rather than one), so the interior is
  // assembled here and the container step is joined on by `containerBreadcrumb`.
  const interior: string[] = [
    ...(state.kind === "hdf" || state.kind === "iso"
      ? [state.volumeName ? `${state.volumeName}:` : ""]
      : []),
    ...state.trail.map((crumb) => crumb.name),
    ...state.isoTrail.map((crumb) => crumb.name),
    ...(state.kind === "archive" && state.archiveDir ? state.archiveDir.split("/") : []),
  ];
  const breadcrumb = containerBreadcrumb(
    state.host,
    state.location || t("files.pane.nothingOpen"),
    interior
  );

  return (
    <section
      className={`tc-pane${focused ? " tc-pane-focused" : ""}`}
      // Focus is shown by the path row across the pane's whole width (see
      // `.tc-pane-focused` in the stylesheet), not by a ring: a commander
      // where you cannot see which side the keyboard is talking to is worse
      // than one with no keyboard at all, and a two-pixel outline is
      // something you have to look for. The drag ring stays — that one is
      // about a pointer already in flight.
      style={{ outline: dragOver ? "2px solid var(--tc-focus-ring)" : "none" }}
      aria-current={focused ? "true" : undefined}
      onClick={onFocus}
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
      {/* Row 1: the pane header — Total Commander's `[drive ▾] [path]
          [filter]`, in one row (brief §1.3). His own `[Layout]` runs with no
          button bar and no drive bar, just the combo, so that is the default
          here; the button strip below is a Settings toggle, off unless asked
          for.

          The combo carries both halves of what a pane can hold: the real,
          enumerated mounts (`panelLocalRoots`, never a hardcoded letter) and
          the six things ART opens with a picker. Which option it shows as
          current, and what a chosen value means, are decided in
          `@/lib/paneSources` where a test can reach them. */}
      <div className="tc-chrome-row tc-path-row">
        <select
          ref={sourceRef}
          className="tc-source-combo"
          aria-label={t("files.tc.sourceAriaLabel")}
          value={sourceValue}
          onChange={(event) => onChooseSource(event.target.value)}
        >
          {/* A pane can sit somewhere no enumerated mount covers — a UNC
              share. Rather than have the combo claim a drive the folder is
              not on, it says so, and `parsePaneSource` refuses the empty
              value so re-picking it navigates nowhere. */}
          {sourceValue === "" && <option value="">{t("files.tc.sourceUnlisted")}</option>}
          {sourceOptions.map((option) => (
            <option key={option.value} value={option.value}>
              {option.literal !== null ? option.literal : t(option.labelKey as string)}
            </option>
          ))}
        </select>
        {/* The breadcrumb, container step and all (brief §3.1). A pane entered
            from a folder leads with the container's full path — the image
            reads as the folder it now behaves like — and one opened from the
            combo leads with its own location, which is the same string.
            `containerBreadcrumb` (`@/lib/containerStep`) joins the head; the
            interior steps are assembled here because each pane kind keeps its
            position in its own field, for the reasons `PaneState` gives. */}
        <span className="tc-path-text" title={breadcrumb.join(" > ")}>
          {breadcrumb.join(" > ")}
        </span>
        <input
          type="text"
          className="tc-mask-input"
          value={filter}
          placeholder={t("files.tc.maskPlaceholder")}
          aria-label={t("files.tc.maskAriaLabel")}
          onChange={(event) => onFilterChange(event.target.value)}
          onKeyDown={(event) => {
            // Escape clears the mask and hands keyboard focus back to the
            // pane — the F-key guard (`isShortcutBlocked` in
            // `FunctionKeys.tsx`) already ignores every shortcut while this
            // `<input>` has DOM focus, so leaving it focused after Escape
            // would leave F5/F8/etc. silently dead until the user clicked
            // away. `stopPropagation` keeps this Escape from also reaching
            // any other Escape handler (a dialog, say) further up the tree.
            if (event.key !== "Escape") return;
            event.preventDefault();
            event.stopPropagation();
            onFilterChange("");
            event.currentTarget.blur();
            onFocus();
          }}
        />
      </div>

      {/* The button strip the header replaced, kept behind a Settings toggle
          (`showSourceButtons`, default off). Every control in it is reachable
          without it — the sources from the combo above, Up from the `[..]`
          row, New folder from F7, Refresh from F2/Ctrl+R — so hiding it
          removes chrome rather than capability. */}
      {showSourceButtons && (
      <div className="tc-chrome-row tc-drive-row">
        <button className="btn btn-sm" onClick={onOpenFolder}>
          {t("files.toolbar.folder")}
        </button>
        <button className="btn btn-sm" onClick={() => onOpenImage("adf")}>
          {t("files.toolbar.adf")}
        </button>
        <button className="btn btn-sm" onClick={() => onOpenImage("hdf")}>
          {t("files.toolbar.hdf")}
        </button>
        <button className="btn btn-sm" onClick={() => onOpenImage("iso")}>
          {t("files.toolbar.disc")}
        </button>
        <button className="btn btn-sm" onClick={() => onOpenImage("archive")}>
          {t("files.toolbar.archive")}
        </button>
        <button className="btn btn-sm" onClick={() => onOpenImage("c64")}>
          {t("files.toolbar.c64")}
        </button>
        {state.kind === "local" &&
          roots.map((root) => (
            <button key={root} className="btn btn-sm" onClick={() => onOpenRoot(root)}>
              {root}
            </button>
          ))}
        <span className="tc-drive-spacer" />
        {writableVolume(state) !== null && (
          <button className="btn btn-sm" onClick={onNewFolder}>
            {t("files.toolbar.newFolder")}
          </button>
        )}
        <button className="btn btn-sm" onClick={onRefresh}>
          {t("files.toolbar.refresh")}
        </button>
        <button className="btn btn-sm" onClick={onUp} disabled={!canGoUp}>
          {t("files.toolbar.up")}
        </button>
      </div>
      )}

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
          write, rather than a pane that quietly does nothing. A disc has no
          `volumeIndex` to gate on (it is not addressed by one — see
          `PaneState`), so it gets its own clause; `VolumeFooter` itself
          always shows the disc's lock badge, unconditionally, since
          `capability` — the ADF/HDF path's source for that badge — is never
          fetched for a disc there is nothing to ask. */}
      {((state.kind !== "local" && state.volumeIndex !== null) ||
        state.kind === "iso" ||
        state.kind === "archive" ||
        state.kind === "c64") && <VolumeFooter state={state} powerMode={powerMode} />}

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

      {showingFiles && (sortedEntries.length > 0 || canGoUp) && (
        <TcHeaderRow sort={sort} onSortChange={onSortChange} />
      )}

      {showingFiles && (
        <ul className="tc-row-list">
          {/* `[..]` — Total Commander's "go up" row, always first when there
              is somewhere to go. It is chrome, not a `PanelEntry`: it has no
              place in the selection Set or the sort order, so it is rendered
              once here rather than being synthesised into `sortedEntries`. */}
          {canGoUp && (
            <li className="tc-row tc-row-updir" onClick={onUp} onDoubleClick={onUp}>
              <span className="tc-cell tc-cell-name">
                <UpDirIcon />
                <span className="tc-name-text">[..]</span>
              </span>
              <span className="tc-cell tc-cell-ext" />
              <span className="tc-cell tc-cell-size" />
              <span className="tc-cell tc-cell-date" />
              <span className="tc-cell tc-cell-attr" />
            </li>
          )}

          {sortedEntries.map((entry) => {
            const isSelected = selectedNames.has(entry.name);
            const isCursor = cursorName === entry.name;
            const { ext } = splitName(entry.name, entry.is_dir);
            const formattedDate = formatDateTC(entry.date);
            // The cursor and the selection are shown two different ways on
            // purpose (second reference: selection is red text on the
            // normal dark background; the cursor is a full-row yellow fill,
            // black text — `tc-row-cursor`, in CSS). Selection wins over the
            // cursor's black when a row is both, so it still reads as both:
            // red text on the yellow bar, rather than the two affordances
            // fighting for the same pixels. Only the *un-selected,
            // non-cursor* case falls through to the row's own file-type
            // colour (`fileTextColorVar` — white/light-blue/dimmed, the same
            // classification `TcRowIcon` uses for its glyph).
            const rowTextColor = isSelected
              ? "var(--tc-selected-text)"
              : isCursor
                ? "var(--tc-cursor-text)"
                : fileTextColorVar(entry);

            return (
              <li
                key={`${entry.name}-${entry.header_block ?? entry.path}`}
                className={`tc-row${isCursor ? " tc-row-cursor" : ""}`}
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
                onClick={(event) => onSelect(entry, event)}
                onDoubleClick={() => onActivate(entry)}
                style={{ color: rowTextColor, cursor: entry.is_dir ? "pointer" : "grab" }}
              >
                <span className="tc-cell tc-cell-name">
                  <TcRowIcon entry={entry} />
                  <span className="tc-name-text">
                    {entry.is_dir ? `[${entry.name}]` : entry.name}
                  </span>
                  {entry.is_link && (
                    <span className="faint" style={{ fontSize: 10 }}>
                      {" "}
                      {t("files.pane.linkSuffix")}
                    </span>
                  )}
                </span>
                <span className="tc-cell tc-cell-ext">{ext}</span>
                <span className="tc-cell tc-cell-size">
                  {entry.is_dir ? t("files.tc.dirSize") : formatGroupedSize(entry.bytes, i18n.language)}
                </span>
                <span className="tc-cell tc-cell-date">{formattedDate ?? "—"}</span>
                <span className="tc-cell tc-cell-attr">{entry.attrs ?? "—"}</span>
              </li>
            );
          })}

          {/* A mask matching nothing says so, rather than showing the same
              blank pane a genuinely empty directory would — that would read
              as ART having failed to open the disk, not as a filter doing
              its job. Told apart by whether the *unfiltered* directory
              (`state.entries`, not `sortedEntries`) actually had anything
              in it. */}
          {sortedEntries.length === 0 && !state.error && (
            <li className="muted" style={{ fontSize: 12, padding: "8px 0 8px 8px" }}>
              {filter.trim() !== "" && state.entries.length > 0
                ? t("files.pane.filterNoMatch", { mask: filter })
                : t("files.pane.empty")}
            </li>
          )}
        </ul>
      )}

      {/* Row 5 of the reference: the status line. */}
      {showingFiles && (
        <div className="tc-chrome-row tc-status-row">
          {t("files.tc.statusLine", {
            selectedBytes: formatGroupedSize(Math.round(status.selectedBytes / 1024), i18n.language),
            totalBytes: formatGroupedSize(Math.round(status.totalBytes / 1024), i18n.language),
            selectedFiles: status.selectedFiles,
            totalFiles: status.totalFiles,
            selectedDirs: status.selectedDirs,
            totalDirs: status.totalDirs,
          })}
        </div>
      )}
    </section>
  );
}

/**
 * The clickable Name / Ext / Size / Date headers above a pane's entry list —
 * Total Commander's column row (task 6b), the sorted column carrying an
 * arrow immediately before its label rather than after (`↓Date`, not
 * `Date↓`).
 *
 * Clicking a header sorts by it; clicking the active one again reverses —
 * `onSortChange` (`clickColumn` in `@/lib/sort`) already knows which. Folders
 * stay first regardless of which header is active or which direction is
 * chosen; that rule lives in the comparator itself (`compareEntries`), not
 * here, so this component only has to reflect `sort`, never enforce it.
 *
 * Ext and Attr have no column of their own in `@/lib/sort`'s `SortColumn` —
 * the reference screenshot never sorts by either — so both render as a
 * plain, unclickable label rather than growing a second sort mechanism
 * alongside the existing one. The trailing cell lines up with each row's
 * copy/delete buttons — an ART addition with no reference equivalent — and
 * carries no label.
 */
function TcHeaderRow({
  sort,
  onSortChange,
}: {
  sort: SortState;
  onSortChange: (column: SortColumn) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="tc-row tc-header-row">
      <span className="tc-cell tc-cell-name">
        <SortHeaderButton column="name" sort={sort} onSortChange={onSortChange} />
      </span>
      <span className="tc-cell tc-cell-ext">{t("files.sort.ext")}</span>
      <span className="tc-cell tc-cell-size">
        <SortHeaderButton column="size" sort={sort} onSortChange={onSortChange} />
      </span>
      <span className="tc-cell tc-cell-date">
        <SortHeaderButton column="date" sort={sort} onSortChange={onSortChange} />
      </span>
      <span className="tc-cell tc-cell-attr">{t("files.sort.attrs")}</span>
    </div>
  );
}

function SortHeaderButton({
  column,
  sort,
  onSortChange,
}: {
  column: SortColumn;
  sort: SortState;
  onSortChange: (column: SortColumn) => void;
}) {
  const { t } = useTranslation();
  const active = sort.column === column;
  // A compiler-checked mapping rather than building the key from `column`
  // with a template literal: `src/i18n/literal-keys.test.ts` scans for
  // literal, quoted translator calls and cannot see a key assembled at
  // runtime, so this keeps every key the header can render a plain literal
  // the scan actually checks — the same reasoning as `confidenceLevelKey` in
  // `LhaBrowser.tsx`.
  const label =
    column === "name"
      ? t("files.sort.name")
      : column === "size"
        ? t("files.sort.size")
        : t("files.sort.date");

  return (
    <button
      type="button"
      className="tc-header-btn"
      style={{ fontWeight: active ? 700 : 400 }}
      title={t("files.sort.title", {
        column: label,
        direction: sort.direction === "asc" ? t("files.sort.ascending") : t("files.sort.descending"),
      })}
      onClick={(event) => {
        // Sorting is display-only and must not steal the pane's focus away
        // from whatever F-keys are currently talking to — unlike a row click,
        // this never calls `onFocus`.
        event.stopPropagation();
        onSortChange(column);
      }}
    >
      {/* The arrow sits immediately before the label — TC's own convention
          (`↓Date`, not `Date↓`) — rather than the trailing arrow this
          button used before the palette redesign. */}
      {active && <span aria-hidden="true">{sort.direction === "asc" ? "▲" : "▼"} </span>}
      {label}
    </button>
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
