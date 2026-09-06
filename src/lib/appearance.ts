// Typed wrappers for the appearance commands (Task 9 of the prefs-and-wallpaper
// round) — mirrors src-tauri/src/commands/appearance.rs. Thin only: the
// frontend never calls `invoke` directly from a component, so every field
// here is spelled exactly as the Rust wire type serialises it
// (`#[serde(rename_all = "camelCase")]` on both the request and outcome
// types), and nothing here reshapes what core computed.

import { invoke } from "@tauri-apps/api/core";

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
}

/**
 * `AppearanceOutcomeWire` on the wire — what `appearance_apply` resolves
 * with. `backups` names where every previous version of a rewritten file
 * went (spec §92: a user told "done" without being told where the previous
 * version went has been given nothing).
 */
export interface AppearanceOutcome {
  written: string[];
  backups: string[];
  picturePlaced: string | null;
  amigaPath: string | null;
}

/** Every backdrop `tree` already carries under `Prefs/Presets/Backdrops`. */
export async function appearanceBackdrops(tree: string): Promise<string[]> {
  return invoke<string[]>("appearance_backdrops", { tree });
}

/**
 * Apply a wallpaper, a screen depth, and/or the shell defaults to `tree`.
 * Core plans every requested part before writing anything and refuses the
 * whole call if any part cannot be done — see
 * `core::appearance::apply_appearance`'s own doc comment.
 */
export async function appearanceApply(
  tree: string,
  request: AppearanceApplyRequest
): Promise<AppearanceOutcome> {
  return invoke<AppearanceOutcome>("appearance_apply", { tree, request });
}
