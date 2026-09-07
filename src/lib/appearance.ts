// Typed wrappers for the appearance commands (Task 9 of the prefs-and-wallpaper
// round) — mirrors src-tauri/src/commands/appearance.rs. Thin only: the
// frontend never calls `invoke` directly from a component, so every field
// here is spelled exactly as the Rust wire type serialises it
// (`#[serde(rename_all = "camelCase")]` on both the request and outcome
// types), and nothing here reshapes what core computed.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** `wbpattern::Which` on the wire. */
export type AppearanceWhich = "root" | "drawer" | "screen";

/** `wbpattern::Placement` on the wire. */
export type AppearancePlacement = "tile" | "center" | "scale" | "scale-good";

/**
 * `WallpaperSource` on the wire — an internally tagged enum
 * (`commands/appearance.rs::WireWallpaperSource`), never two separate
 * optional fields the caller would have to reconcile itself.
 */
export type AppearanceWallpaperSource =
  | { kind: "already-in-tree"; amigaPath: string }
  | { kind: "host-picture"; path: string; colours: number };

export interface AppearanceWallpaperAssignment {
  which: AppearanceWhich;
  source: AppearanceWallpaperSource;
  placement: AppearancePlacement;
}

/** `AppearanceApplyRequest` on the wire. */
export interface AppearanceApplyRequest {
  wallpaper: AppearanceWallpaperAssignment | null;
  screenDepth: number | null;
  shellDefaults: boolean;
  /** `AppearanceRequest::arrange_icons` on the wire (Task 8 of the
   * drawer-icons round) — lay out every icon the tree's drawers carry that
   * has not already been positioned by the release itself
   * (`core::icongrid::arrange`). */
  arrangeIcons: boolean;
}

/**
 * `AppearanceOutcomeWire` on the wire — what `appearance_apply` resolves
 * with. `backups` names where every previous version of a rewritten file
 * went (spec §92: a user told "done" without being told where the previous
 * version went has been given nothing).
 *
 * `iconsPlaced`, `drawersArranged` and `iconsSkipped` are Task 8's own
 * addition: a run that arranged icons but told the user only "done" would be
 * the same failure CLAUDE.md names — a confident sentence that omits what
 * did not work. `iconsSkipped` names the icons `core::amigaicon` could not
 * read, not merely their count.
 */
export interface AppearanceOutcome {
  written: string[];
  backups: string[];
  picturePlaced: string | null;
  amigaPath: string | null;
  iconsPlaced: number;
  drawersArranged: number;
  iconsSkipped: string[];
}

/**
 * At most this many paths joined, with a count for the rest — the same
 * capping convention `core::osinstall::apply::some_of` uses for a package
 * refusal naming up to 211 real files, reused here rather than a second
 * style (C5, final whole-branch review of the drawer-icons round):
 * arranging icons across a 3.9-scale tree can write hundreds of `.info`
 * files in one call, and joining every one of them into a single paragraph
 * is honest and unusable.
 */
export function someOf(paths: string[], max = 5): string {
  if (paths.length <= max) {
    return paths.join(", ");
  }
  return `${paths.slice(0, max).join(", ")}, and ${paths.length - max} more`;
}

/** Every backdrop `tree` already carries under `Prefs/Presets/Backdrops`. */
export async function appearanceBackdrops(tree: string): Promise<string[]> {
  return invoke<string[]>("appearance_backdrops", { tree });
}

/**
 * `commands/appearance.rs::AppearanceApplyResult` on the wire — a finished
 * `appearance_apply` job's own answer (ART-248). `job_id` stays snake_case,
 * matching every other job result in ART (`RehearsalResult`,
 * `AmigaInstallResult`); the rest is `AppearanceOutcome` itself, flattened in
 * beside it by the Rust side rather than nested.
 */
export interface AppearanceApplyResult extends AppearanceOutcome {
  job_id: number;
}

/** The event a finished `appearance_apply` job's own answer arrives on. */
export const APPEARANCE_APPLY_EVENT = "appearance-apply-result";

/**
 * Apply a wallpaper, a screen depth, and/or the shell defaults to `tree`.
 * Core plans every requested part before writing anything and refuses the
 * whole call if any part cannot be done — see
 * `core::appearance::apply_appearance`'s own doc comment.
 *
 * **ART-248: a job, not a plain promise for the outcome.** The wallpaper
 * path alone can mean a median-cut quantise pass over a couple of million
 * pixels, and arranging icons across a 3.9-scale tree can commit hundreds of
 * small files — long enough that §54/§55 apply. Returns a job id
 * immediately; progress arrives on the ordinary `job-progress` event and the
 * finished outcome on [`APPEARANCE_APPLY_EVENT`] — see
 * [`onAppearanceApplyResult`] and `@/lib/jobs`'s `awaitJobResult`, the same
 * shape `firstbootRehearse`/`onFirstBootRehearsalResult` already use.
 */
export async function appearanceApply(
  tree: string,
  request: AppearanceApplyRequest
): Promise<number> {
  return invoke<number>("appearance_apply", { tree, request });
}

/**
 * Subscribe to finished `appearance_apply` jobs. A cancelled or failed job
 * never sends one — the job bar (and this panel's own progress line) is
 * where those are seen.
 *
 * Minor (2026-09-07 final review): kept, not called from anywhere today —
 * `AppearancePanel.tsx` reads the same event through `@/lib/jobs`'s
 * `awaitJobResult(APPEARANCE_APPLY_EVENT, …)` instead, which already gives
 * it a `JobId`-scoped promise it can `await` beside `appearanceApply`'s own
 * return value, rather than a long-lived listener it would have to
 * remember to unsubscribe. This exists for the same reason
 * `firstboot.ts::onFirstBootRehearsalResult` does and is equally unused —
 * a typed listener beside the event constant, matching the shape a caller
 * that does want a standing subscription (rather than one job's result)
 * would need, so the two do not have to diverge the day one shows up.
 */
export async function onAppearanceApplyResult(
  handler: (result: AppearanceApplyResult) => void
): Promise<UnlistenFn> {
  return listen<AppearanceApplyResult>(APPEARANCE_APPLY_EVENT, (event) => handler(event.payload));
}
