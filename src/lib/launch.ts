// Typed wrappers for Play (Collection · wave C, Task 11): turning a
// catalogued title into a running WinUAE session.
//
// Mirrors `core::launch` and `commands/launch.rs`. `launchPlan` starts
// nothing and reads no media — it is what the confirmation screen shows
// before anything happens. `launchTitle` is the same decision made again,
// then acted on.

import { invoke } from "@tauri-apps/api/core";

import type { Media } from "@/lib/gameindex";
import type { Phrase } from "@/lib/phrase";

export type Machine = "a500" | "a1200";

export interface LaunchRom {
  name: string;
  models: string[];
  path: string;
}

/**
 * What was decided ART should mount. `one_click` — not `oneClick` — because
 * that is what `core::launch::LaunchKind` actually serialises: there is no
 * `rename_all = "camelCase"` on it, and this codebase already carries
 * snake_case fields straight from Rust (`checksum_valid`, `file_path`,
 * `compatible_models`) rather than papering over them.
 */
export type LaunchKind =
  | { kind: "floppies"; images: string[] }
  | { kind: "hardfile"; image: string }
  | { kind: "whdload"; drawer: string; slave: string; system: string; one_click: boolean };

export type LaunchNote = { kind: "more-disks-than-drives"; total: number; mounted: number };

export type LaunchRefusal =
  | { kind: "no-suitable-rom"; machine: Machine }
  | { kind: "no-system-volume" }
  | { kind: "file-missing"; path: string };

export interface LaunchPlan {
  machine: Machine;
  rom: LaunchRom;
  kind: LaunchKind;
  notes: LaunchNote[];
}

export interface LaunchPreview {
  plan: LaunchPlan | null;
  refusal: LaunchRefusal | null;
}

export interface LaunchArgs {
  id: string;
  title: string;
  path: string;
  media: Media;
  chipset: string | null;
  rom_dir: string;
  default_machine: Machine;
  system_volume: string | null;
  one_click: boolean;
}

/** Work out what a launch would need. Starts nothing. */
export async function launchPlan(request: LaunchArgs): Promise<LaunchPreview> {
  return invoke<LaunchPreview>("launch_plan", { request });
}

/**
 * Launch the title. Unpacks a `.rp9`'s disks, writes the WHDLoad boot
 * directory for a one-click (Y2) launch, then starts WinUAE. Returns its
 * process id.
 */
export async function launchTitle(request: LaunchArgs): Promise<number> {
  return invoke<number>("launch_title", { request });
}

/** The machine a plan settled on, as a sentence a user reads. */
export function machinePhrase(machine: Machine): Phrase {
  return { key: `collection.detail.play.machine.${machine}` };
}

/** Why a plan could not be made. A refusal offers nothing to confirm. */
export function refusalPhrase(refusal: LaunchRefusal): Phrase {
  switch (refusal.kind) {
    case "no-suitable-rom":
      // The machine code (A500/A1200) is a hardware name, not a sentence —
      // the same reason `WinUAE` and `Kickstart` appear untranslated
      // elsewhere in the catalogue, so this is the plain code rather than a
      // second `Phrase` needing its own translator.
      return {
        key: "collection.detail.play.refusal.noSuitableRom",
        params: { machine: refusal.machine.toUpperCase() },
      };
    case "no-system-volume":
      return { key: "collection.detail.play.refusal.noSystemVolume" };
    case "file-missing":
      return {
        key: "collection.detail.play.refusal.fileMissing",
        params: { path: refusal.path },
      };
  }
}

/** Something true about the plan the user should see before it runs. */
export function notePhrase(note: LaunchNote): Phrase {
  switch (note.kind) {
    case "more-disks-than-drives":
      return {
        key: "collection.detail.play.note.moreDisksThanDrives",
        params: { total: note.total, mounted: note.mounted },
      };
  }
}

/** What a settled plan will actually mount, in one line. */
export function launchKindPhrase(kind: LaunchKind): Phrase {
  switch (kind.kind) {
    case "floppies":
      // The disk names themselves are rendered as their own list beside this
      // line (the same shape the panel already uses for the catalogued
      // media), so this sentence only needs the count.
      return {
        key: "collection.detail.play.kind.floppies",
        params: { count: kind.images.length },
      };
    case "hardfile":
      return { key: "collection.detail.play.kind.hardfile", params: { image: kind.image } };
    case "whdload":
      return {
        key: kind.one_click
          ? "collection.detail.play.kind.whdloadOneClick"
          : "collection.detail.play.kind.whdloadMountOnly",
        params: { slave: kind.slave },
      };
  }
}
