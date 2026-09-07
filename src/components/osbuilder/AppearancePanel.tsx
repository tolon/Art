// The Görünüm (Appearance) panel — Task 10 of the prefs-and-wallpaper round.
//
// **Why this exists at all.** Tasks 1-9 built the whole engine —
// `core::amigaprefs` reads and rewrites a release's own `WBPattern.prefs` and
// `ScreenMode.prefs`, `core::ilbm` turns a host PNG/JPEG into a real Amiga
// backdrop, `core::appearance::apply_appearance` plans and commits all of it,
// and `commands/appearance.rs` exposes two thin Tauri commands. None of that
// is reachable from the product without a screen that calls it — the exact
// failure this project has shipped twice before (CLAUDE.md: "a feature that
// compiles, tests green, and is unreachable"). This panel is that screen.
//
// **Where it sits.** Beside `NetworkPanel`, on the volumes step
// (`src/pages/osbuilder/steps.tsx::StepBirimler`) — both write into the
// system volume a card is about to carry, and both need the tree ART just
// built rather than an image somewhere else.
//
// **Refuse, never substitute, on this side too.** `core::appearance` plans
// everything before writing a byte and refuses the whole call by name if any
// part cannot be done (a missing `WBPattern.prefs`, a picture that will not
// decode, a backdrop name already taken). This panel does not repeat that
// logic — it sends the request and renders whatever core says back, verbatim
// (ART-060 / CLAUDE.md): the sentence names the exact file and, on success,
// the exact backup path, and rewording either would destroy what the user
// needs.
//
// **Nothing changes unless the user changes it.** Every choice here —
// which chunk to set, where the picture comes from, how many colours, how it
// is placed, whether to touch the screen depth or the shell defaults — goes
// through `src/lib/remembered.ts` with a guard. These are consumed by this
// same step, not carried to a later one, so they are remembered keys and not
// `buildSession` fields (CLAUDE.md's own test: can the value ART wrote and
// the value the next step operates on drift apart? Here, no — the value ART
// wrote is a file inside the tree, and the next thing that reads it is
// AmigaOS itself, not another step of this wizard).

import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import {
  APPEARANCE_APPLY_EVENT,
  appearanceApply,
  appearanceBackdrops,
  someOf,
  type AppearanceApplyResult,
  type AppearanceOutcome,
  type AppearancePlacement,
  type AppearanceWallpaperSource,
  type AppearanceWhich,
} from "@/lib/appearance";
import { errorText } from "@/lib/errorText";
import {
  awaitJobResult,
  fraction,
  isJobCancellation,
  jobCancel,
  onJobProgress,
  subscribeSafely,
  type JobProgress,
} from "@/lib/jobs";
import { isFlag, isOneOf, isTextOrNothing, isWholeNumberBetween } from "@/lib/remembered";
import { useBuildSession } from "@/lib/useBuildSession";
import { useRemembered } from "@/lib/useRemembered";
import { Field } from "@/components/osbuilder/Field";

/** Mirrors `wbpattern::Which` — the three slots a release's own
 * `WBPattern.prefs` carries. */
const WHICHES: AppearanceWhich[] = ["root", "drawer", "screen"];

/** Mirrors `wbpattern::Placement`. */
const PLACEMENTS: AppearancePlacement[] = ["tile", "center", "scale", "scale-good"];

/** How many colours a host picture is quantised to before it becomes ILBM.
 * `core::picture::quantise` clamps to `1..=256` regardless — this is a sane
 * palette for a screen, not a limit the core relies on. */
const COLOUR_MIN = 2;
const COLOUR_MAX = 256;

/** A plain Amiga chunky depth — `core::amigaprefs::screenmode` carries the
 * real range; this is a sane UI bound, not one the core enforces. */
const DEPTH_MIN = 1;
const DEPTH_MAX = 8;

/** Where the wallpaper's picture comes from, before it is turned into the
 * wire's tagged union. Kept separate from `AppearanceWallpaperSource` because
 * the two paths need different remembered fields (an Amiga path versus a
 * host path and a colour count), and only one is complete at a time. */
export type AppearanceSourceKind = "already-in-tree" | "host-picture";

/**
 * Why the Apply button cannot be pressed yet, or `null`.
 *
 * Exported and pure so it can be tested without a screen, and so the
 * sentence is the same one the button's `title` and the line beneath it use
 * — the same shape `NetworkPanel.tsx::networkBlocker` already established
 * for its sibling panel on this step.
 */
export function appearanceBlocker(input: {
  tree: string | null;
  wallpaperOn: boolean;
  sourceKind: AppearanceSourceKind;
  amigaPath: string | null;
  hostPath: string | null;
  screenDepthOn: boolean;
  shellDefaultsOn: boolean;
  arrangeIconsOn: boolean;
}): { key: string; params?: Record<string, unknown> } | null {
  if (!input.tree) return { key: "appearance.blocked.noTree" };
  if (
    !input.wallpaperOn &&
    !input.screenDepthOn &&
    !input.shellDefaultsOn &&
    !input.arrangeIconsOn
  ) {
    return { key: "appearance.blocked.nothingChosen" };
  }
  if (input.wallpaperOn) {
    if (input.sourceKind === "already-in-tree" && !input.amigaPath) {
      return { key: "appearance.blocked.noBackdropChosen" };
    }
    if (input.sourceKind === "host-picture" && !input.hostPath) {
      return { key: "appearance.blocked.noPictureChosen" };
    }
  }
  return null;
}

export function AppearancePanel() {
  const { t } = useTranslation();
  const tree = useBuildSession().session.tree.root;

  const [which, setWhich] = useRemembered<AppearanceWhich>(
    "appearance.which",
    isOneOf(...WHICHES),
    "root"
  );
  const [wallpaperOn, setWallpaperOn] = useRemembered("appearance.wallpaperOn", isFlag, false);
  const [sourceKind, setSourceKind] = useRemembered<AppearanceSourceKind>(
    "appearance.sourceKind",
    isOneOf("already-in-tree", "host-picture"),
    "already-in-tree"
  );
  const [amigaPath, setAmigaPath] = useRemembered<string | null>(
    "appearance.amigaPath",
    isTextOrNothing,
    null
  );
  const [hostPath, setHostPath] = useRemembered<string | null>(
    "appearance.hostPath",
    isTextOrNothing,
    null
  );
  const [colours, setColours] = useRemembered(
    "appearance.colours",
    isWholeNumberBetween(COLOUR_MIN, COLOUR_MAX),
    32
  );
  const [placement, setPlacement] = useRemembered<AppearancePlacement>(
    "appearance.placement",
    isOneOf(...PLACEMENTS),
    "scale-good"
  );

  const [screenDepthOn, setScreenDepthOn] = useRemembered(
    "appearance.screenDepthOn",
    isFlag,
    false
  );
  const [screenDepth, setScreenDepth] = useRemembered(
    "appearance.screenDepth",
    isWholeNumberBetween(DEPTH_MIN, DEPTH_MAX),
    4
  );

  const [shellDefaultsOn, setShellDefaultsOn] = useRemembered(
    "appearance.shellDefaultsOn",
    isFlag,
    false
  );

  // Task 8 of the drawer-icons round: `core::appearance::apply_appearance`
  // already plans and commits icon arrangement whenever
  // `AppearanceRequest::arrange_icons` is set — this is the checkbox that
  // sets it. Consumed by this same step (the value ART writes and the value
  // the next thing reads — AmigaOS itself, not another wizard step — cannot
  // drift apart, CLAUDE.md's own test), so it is a remembered key rather
  // than a `buildSession` field, the same reasoning every other flag on this
  // panel already follows.
  const [arrangeIconsOn, setArrangeIconsOn] = useRemembered(
    "appearance.arrangeIconsOn",
    isFlag,
    false
  );

  const [backdrops, setBackdrops] = useState<string[]>([]);
  const applyJob = useRef<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<JobProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [cancelled, setCancelled] = useState(false);
  const [done, setDone] = useState<AppearanceOutcome | null>(null);

  // The job's own progress (ART-248). `job-progress` is application-wide, so
  // every update is checked against this panel's own job id first — the
  // same shape `AmigaInstallPanel` already uses for its own long-running job.
  useEffect(() => {
    return subscribeSafely(() =>
      onJobProgress((update) => {
        if (update.id !== applyJob.current) return;
        setProgress(update);
      })
    );
  }, []);

  // Only what the tree actually carries — never a fixed list. A release that
  // has never had a picture placed on it lists nothing, and that emptiness
  // is itself information (see `appearance.wallpaper.backdrop.none` below).
  useEffect(() => {
    if (!tree) {
      setBackdrops([]);
      return;
    }
    let cancelled = false;
    appearanceBackdrops(tree)
      .then((names) => {
        if (!cancelled) setBackdrops(names);
      })
      .catch(() => {
        if (!cancelled) setBackdrops([]);
      });
    return () => {
      cancelled = true;
    };
  }, [tree]);

  const blocker = appearanceBlocker({
    tree,
    wallpaperOn,
    sourceKind,
    amigaPath,
    hostPath,
    screenDepthOn,
    shellDefaultsOn,
    arrangeIconsOn,
  });

  async function choosePicture() {
    const picked = await open({
      multiple: false,
      title: t("appearance.wallpaper.picture.chooseTitle"),
      filters: [{ name: "PNG / JPEG", extensions: ["png", "jpg", "jpeg"] }],
    });
    if (typeof picked === "string") setHostPath(picked);
  }

  async function apply() {
    if (!tree || blocker) return;
    setBusy(true);
    setError(null);
    setDone(null);
    setCancelled(false);
    setProgress(null);
    try {
      let source: AppearanceWallpaperSource | null = null;
      if (wallpaperOn) {
        source =
          sourceKind === "already-in-tree"
            ? { kind: "already-in-tree", amigaPath: amigaPath as string }
            : { kind: "host-picture", path: hostPath as string, colours };
      }
      const outcome = await awaitJobResult<AppearanceApplyResult, AppearanceOutcome>(
        APPEARANCE_APPLY_EVENT,
        async () => {
          const id = await appearanceApply(tree, {
            wallpaper: wallpaperOn && source ? { which, source, placement } : null,
            screenDepth: screenDepthOn ? screenDepth : null,
            shellDefaults: shellDefaultsOn,
            arrangeIcons: arrangeIconsOn,
          });
          applyJob.current = id;
          return id;
        },
        (payload) => payload
      );
      setDone(outcome);
    } catch (e) {
      // Endings stay distinct (CLAUDE.md): the user stopping the job and
      // ART refusing it are two different sentences with two different next
      // steps, never collapsed into one "did not succeed".
      if (isJobCancellation(e)) {
        setCancelled(true);
      } else {
        // Verbatim, on purpose (ART-060): `errorText` renders whatever core
        // wrote, English and all — this round's own sentences name the
        // exact file and backup path a rewrite would destroy.
        setError(errorText(t, e));
      }
    } finally {
      setBusy(false);
      applyJob.current = null;
    }
  }

  function stopApply() {
    if (applyJob.current !== null) void jobCancel(applyJob.current);
  }

  const pct = progress ? fraction(progress) : null;

  return (
    <section className="card" style={{ marginBottom: 16 }} data-testid="appearance-panel">
      <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("appearance.heading")}</h2>
      <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
        {t("appearance.intro")}
      </p>

      <label style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 8 }}>
        <input
          type="checkbox"
          checked={wallpaperOn}
          onChange={(e) => setWallpaperOn(e.target.checked)}
        />
        <span style={{ fontSize: 13 }}>{t("appearance.wallpaper.enable")}</span>
      </label>

      {wallpaperOn && (
        <div style={{ marginLeft: 24, marginBottom: 12 }} data-testid="appearance-wallpaper">
          <label style={{ display: "flex", flexDirection: "column", gap: 4, marginBottom: 8 }}>
            <span className="muted" style={{ fontSize: 12 }}>
              {t("appearance.wallpaper.whichLabel")}
            </span>
            <select
              className="input"
              value={which}
              onChange={(e) => setWhich(e.target.value as AppearanceWhich)}
              aria-label={t("appearance.wallpaper.whichLabel")}
              style={{ maxWidth: "16em" }}
            >
              {WHICHES.map((w) => (
                <option key={w} value={w}>
                  {t(`appearance.wallpaper.which.${w}`)}
                </option>
              ))}
            </select>
          </label>

          <div role="radiogroup" style={{ marginBottom: 8 }}>
            <label style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 13 }}>
              <input
                type="radio"
                name="appearance-source-kind"
                checked={sourceKind === "already-in-tree"}
                onChange={() => setSourceKind("already-in-tree")}
              />
              {t("appearance.wallpaper.source.existing")}
            </label>
            <label
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                fontSize: 13,
                marginTop: 4,
              }}
            >
              <input
                type="radio"
                name="appearance-source-kind"
                checked={sourceKind === "host-picture"}
                onChange={() => setSourceKind("host-picture")}
              />
              {t("appearance.wallpaper.source.host")}
            </label>
          </div>

          {sourceKind === "already-in-tree" && (
            <label style={{ display: "flex", flexDirection: "column", gap: 4, marginBottom: 8 }}>
              <span className="muted" style={{ fontSize: 12 }}>
                {t("appearance.wallpaper.backdropLabel")}
              </span>
              <select
                className="input"
                value={amigaPath ?? ""}
                onChange={(e) => setAmigaPath(e.target.value || null)}
                aria-label={t("appearance.wallpaper.backdropLabel")}
                style={{ maxWidth: "24em" }}
              >
                <option value="">{t("appearance.wallpaper.backdrop.choose")}</option>
                {backdrops.map((name) => (
                  <option key={name} value={`Sys:Prefs/Presets/Backdrops/${name}`}>
                    {name}
                  </option>
                ))}
              </select>
              {tree && backdrops.length === 0 && (
                <span className="faint" style={{ fontSize: 10 }}>
                  {t("appearance.wallpaper.backdrop.none")}
                </span>
              )}
            </label>
          )}

          {sourceKind === "host-picture" && (
            <>
              <Field
                label={t("appearance.wallpaper.pictureLabel")}
                value={hostPath}
                empty={t("appearance.wallpaper.picture.none")}
                onChoose={() => void choosePicture()}
                choose={t("common.browse")}
                hint={t("appearance.wallpaper.picture.hint")}
                onClear={hostPath ? () => setHostPath(null) : undefined}
                clear={t("common.clear")}
              />
              <label
                style={{ display: "flex", flexDirection: "column", gap: 4, marginBottom: 8 }}
              >
                <span className="muted" style={{ fontSize: 12 }}>
                  {t("appearance.wallpaper.colours")}
                </span>
                <input
                  className="input"
                  value={String(colours)}
                  onChange={(e) => {
                    const parsed = Number.parseInt(e.target.value, 10);
                    if (Number.isFinite(parsed) && parsed >= COLOUR_MIN && parsed <= COLOUR_MAX) {
                      setColours(parsed);
                    }
                  }}
                  aria-label={t("appearance.wallpaper.colours")}
                  style={{ maxWidth: "8em" }}
                />
              </label>
            </>
          )}

          <label style={{ display: "flex", flexDirection: "column", gap: 4, marginBottom: 8 }}>
            <span className="muted" style={{ fontSize: 12 }}>
              {t("appearance.wallpaper.placementLabel")}
            </span>
            <select
              className="input"
              value={placement}
              onChange={(e) => setPlacement(e.target.value as AppearancePlacement)}
              aria-label={t("appearance.wallpaper.placementLabel")}
              style={{ maxWidth: "16em" }}
            >
              {PLACEMENTS.map((p) => (
                <option key={p} value={p}>
                  {t(`appearance.wallpaper.placement.${p}`)}
                </option>
              ))}
            </select>
          </label>
        </div>
      )}

      <label style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 8 }}>
        <input
          type="checkbox"
          checked={screenDepthOn}
          onChange={(e) => setScreenDepthOn(e.target.checked)}
        />
        <span style={{ fontSize: 13 }}>{t("appearance.screen.enable")}</span>
      </label>

      {screenDepthOn && (
        <div style={{ marginLeft: 24, marginBottom: 12 }} data-testid="appearance-screen">
          <label style={{ display: "flex", flexDirection: "column", gap: 4, marginBottom: 8 }}>
            <span className="muted" style={{ fontSize: 12 }}>
              {t("appearance.screen.depth")}
            </span>
            <input
              className="input"
              value={String(screenDepth)}
              onChange={(e) => {
                const parsed = Number.parseInt(e.target.value, 10);
                if (Number.isFinite(parsed) && parsed >= DEPTH_MIN && parsed <= DEPTH_MAX) {
                  setScreenDepth(parsed);
                }
              }}
              aria-label={t("appearance.screen.depth")}
              style={{ maxWidth: "8em" }}
            />
          </label>
        </div>
      )}

      <label style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 8 }}>
        <input
          type="checkbox"
          checked={shellDefaultsOn}
          onChange={(e) => setShellDefaultsOn(e.target.checked)}
        />
        <span style={{ fontSize: 13 }}>{t("appearance.shell.enable")}</span>
      </label>

      <label style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 8 }}>
        <input
          type="checkbox"
          checked={arrangeIconsOn}
          onChange={(e) => setArrangeIconsOn(e.target.checked)}
        />
        <span style={{ fontSize: 13 }}>{t("appearance.icons.enable")}</span>
      </label>

      {error && (
        <p
          data-testid="appearance-error"
          className="badge badge-err"
          style={{ display: "block", fontSize: 11, padding: "4px 8px", marginBottom: 8 }}
        >
          {error}
        </p>
      )}

      {/* ART-248: the user's own ending, not an error — collapsing this
          into `error` would be exactly the sentence CLAUDE.md warns against
          ("Endings stay distinct"). */}
      {cancelled && (
        <p
          data-testid="appearance-cancelled"
          className="badge badge-warn"
          style={{ display: "block", fontSize: 11, padding: "4px 8px", marginBottom: 8 }}
        >
          {t("appearance.cancelled")}
        </p>
      )}

      <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
        <button
          className="btn btn-primary"
          onClick={() => void apply()}
          disabled={busy || blocker !== null}
          title={blocker ? t(blocker.key, blocker.params) : undefined}
        >
          {t(busy ? "appearance.applying" : "appearance.apply")}
        </button>
        {busy && (
          <button className="btn" onClick={stopApply}>
            {t("appearance.stop")}
          </button>
        )}
        {blocker && (
          <span className="muted" style={{ fontSize: 11 }}>
            {t(blocker.key, blocker.params)}
          </span>
        )}
      </div>

      {/* A count, never a fixed-width bar for an unknown total (CLAUDE.md):
          the planning phase reports no total at all, and only once it
          finishes does the commit phase know how many files it will write. */}
      {busy && (
        <p className="faint" data-testid="appearance-progress" style={{ fontSize: 11, marginTop: 6 }}>
          {pct === null
            ? t("appearance.progressStarting")
            : t("appearance.progressPercent", {
                percent: Math.round(pct * 100),
                done: progress?.done ?? 0,
                total: progress?.total ?? 0,
              })}
          {progress?.message?.trim() ? (
            <span data-testid="appearance-progress-phase" style={{ marginLeft: 8 }}>
              {progress.message}
            </span>
          ) : null}
        </p>
      )}

      {done && (
        <div
          data-testid="appearance-done"
          className="badge badge-ok"
          style={{ display: "block", fontSize: 11, padding: "4px 8px", marginTop: 8 }}
        >
          {/* C5/C7 (final whole-branch review): a run that arranges icons
              across a 3.9-scale tree can write hundreds of files, and a run
              that touches none (every icon already placed, nothing else
              requested) writes zero. "Written: " with nothing after it and
              a genuine failure must not read as the same screen, and a list
              of 361 paths joined into one paragraph is honest and
              unusable — capped with `someOf`, the same convention
              `core::osinstall::apply::some_of` uses on the Rust side. */}
          {done.written.length > 0 ? (
            <p style={{ margin: 0 }}>
              {t("appearance.done.written", {
                count: done.written.length,
                files: someOf(done.written),
              })}
            </p>
          ) : (
            <p data-testid="appearance-nothing-written" style={{ margin: 0 }}>
              {t("appearance.done.nothingWritten")}
            </p>
          )}
          {/* spec §92 / CLAUDE.md: a user told "done" without being told
              where the previous version went has been given nothing. */}
          {done.backups.length > 0 && (
            <p data-testid="appearance-backup" style={{ margin: "4px 0 0" }}>
              {t("appearance.done.backup", {
                count: done.backups.length,
                files: someOf(done.backups),
              })}
            </p>
          )}
          {/* Task 8 of the drawer-icons round: a run that arranged icons and
              said only "done" would be the same failure CLAUDE.md names — a
              confident sentence that omits what actually happened. Gated on
              the counts rather than the checkbox: it is the outcome, not the
              request, that says whether anything was actually placed. */}
          {(done.iconsPlaced > 0 || done.drawersArranged > 0) && (
            <p data-testid="appearance-icons-placed" style={{ margin: "4px 0 0" }}>
              {t("appearance.done.iconsPlaced", {
                count: done.iconsPlaced,
                drawers: done.drawersArranged,
              })}
            </p>
          )}
          {done.iconsSkipped.length > 0 && (
            <p data-testid="appearance-icons-skipped" style={{ margin: "4px 0 0" }}>
              {t("appearance.done.iconsSkipped", {
                count: done.iconsSkipped.length,
                files: done.iconsSkipped.join(", "),
              })}
            </p>
          )}
        </div>
      )}
    </section>
  );
}
