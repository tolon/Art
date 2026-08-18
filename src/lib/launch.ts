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
import { isOneOf } from "@/lib/remembered";

export type Machine = "a500" | "a1200";

/**
 * Shared by every screen that remembers a plain `Machine` choice — Settings'
 * global default and `TitleDetail`'s per-title override both guard with this
 * rather than each defining their own copy of the same two strings.
 */
export const isMachine = isOneOf<Machine>("a500", "a1200");

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
  | { kind: "file-missing"; path: string }
  | { kind: "nothing-to-mount" };

export interface LaunchPlan {
  machine: Machine;
  rom: LaunchRom;
  kind: LaunchKind;
  notes: LaunchNote[];
}

/**
 * What will be mounted and whether it can be written to (design §4.4) — the
 * confirmation screen must state this rather than leave it assumed, which is
 * why it travels apart from `LaunchKind`: `LaunchKind` says *what* the plan
 * mounts, this says *how*.
 */
export type MountNote =
  | { kind: "floppies"; count: number }
  | { kind: "hardfile"; read_only: boolean }
  | { kind: "whdload"; one_click: boolean };

export interface LaunchPreview {
  plan: LaunchPlan | null;
  refusal: LaunchRefusal | null;
  mounts: MountNote[];
}

export interface LaunchArgs {
  id: string;
  title: string;
  path: string;
  media: Media;
  chipset: string | null;
  rom_dir: string;
  default_machine: Machine;
  /** The user's own choice for this title — outranks the stated chipset the
   *  way every explicit choice in ART outranks an inference. `null` is
   *  "auto": let the backend infer from the catalogue's chipset and fall
   *  back to `default_machine`. */
  machine_override: Machine | null;
  system_volume: string | null;
  one_click: boolean;
}

/** Work out what a launch would need. Starts nothing. */
export async function launchPlan(request: LaunchArgs): Promise<LaunchPreview> {
  return invoke<LaunchPreview>("launch_plan", { request });
}

/**
 * Launch the title. Unpacks a `.rp9`'s disk or hardfile, writes the WHDLoad
 * boot directory for a one-click (Y2) launch, then starts WinUAE. Returns
 * its process id.
 *
 * `winuaePath` is the user's configured path from Settings
 * (`settings.winuaePath`) — the same argument `winuaeLaunch` already takes.
 * `undefined`/`null` falls back to WinUAE's standard install locations.
 */
export async function launchTitle(
  request: LaunchArgs,
  winuaePath?: string | null
): Promise<number> {
  return invoke<number>("launch_title", { request, winuaePath: winuaePath ?? null });
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
    case "nothing-to-mount":
      return { key: "collection.detail.play.refusal.nothingToMount" };
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

/**
 * What is mounted and whether it can be written to — design §4.4's own
 * requirement, shown on the confirmation screen before Start rather than
 * assumed. A floppy set gets no read/write statement: ART does not
 * write-protect floppies differently from one another today.
 */
export function mountNotePhrase(note: MountNote): Phrase {
  switch (note.kind) {
    case "floppies":
      return {
        key: "collection.detail.play.mount.floppies",
        params: { count: note.count },
      };
    case "hardfile":
      return {
        key: note.read_only
          ? "collection.detail.play.mount.hardfileReadOnly"
          : "collection.detail.play.mount.hardfileWritable",
      };
    case "whdload":
      return {
        key: note.one_click
          ? "collection.detail.play.mount.whdloadOneClick"
          : "collection.detail.play.mount.whdloadMountOnly",
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
