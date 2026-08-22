// Where ART stages work it will throw away (ART-196).
// Mirrors src-tauri/src/commands/scratch.rs.
//
// Everything ART stages — preview extractions, install staging roots,
// unpacked packages, a launch's disks, the emulator's own configuration —
// used to go to the platform's temp directory, which on Windows is on the
// **system drive**. The owner's standing rule is that ART writes nothing to
// `C:`, and until this the product could not honour it.
//
// Two things this module will never do, because the Rust side will not
// either: move what is already staged, and delete anything. Repointing the
// root leaves the old one exactly as it was, and `previous` is how the screen
// says where that is.

import { invoke } from "@tauri-apps/api/core";

/** What ART is staging into, and how it came to be that. */
export interface ScratchRootState {
  /**
   * The folder ART will stage into right now — `null` when the chosen one
   * cannot be reached, in which case **nothing stages at all**.
   *
   * Not "the default, then": every operation that needs to stage refuses
   * while this is `null`, and a screen that showed the default here would be
   * claiming something the core is refusing to do.
   */
  inUse: string | null;
  /** The folder the user chose, or `null` while they are on the default. */
  chosen: string | null;
  /** What "the default" means on this machine. Shown, never described. */
  default: string;
  /** Why the chosen folder cannot be used, when it cannot — the whole
   *  refusal, error id and remedy included. */
  unreachable: string | null;
}

/** The answer to changing the root: the new state, and where the old was. */
export interface ScratchRootChange {
  root: ScratchRootState;
  /** Where ART was staging until this call. Nothing there was moved or
   *  removed. */
  previous: string;
}

/** What ART is staging into. Never throws for an unreachable root — that is
 *  reported in `unreachable`, so the screen that fixes it still renders. */
export async function scratchRoot(): Promise<ScratchRootState> {
  return invoke<ScratchRootState>("scratch_root");
}

/**
 * Take `path` as the scratch root, or `null` to go back to the default.
 *
 * Throws when the folder is not there or cannot be written to — and the root
 * ART had stays in force, so a refused choice never leaves it with nowhere
 * to stage.
 */
export async function scratchSetRoot(path: string | null): Promise<ScratchRootChange> {
  return invoke<ScratchRootChange>("scratch_set_root", { path });
}
