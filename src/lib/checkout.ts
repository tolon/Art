// F4 — editing a file inside an image. Mirrors src-tauri/src/commands/checkout.rs.
//
// ART does not implement an editor. It implements the round trip: the file
// comes out to a temp folder, the user's own editor opens it, and the bytes go
// back in through the same journalled write path as any other copy.
//
// Nothing is written back unless the file's SHA-256 changed, so an editor that
// opens a file and closes it cannot cause a write into a disk image.

import { invoke } from "@tauri-apps/api/core";

import type { Phrase } from "@/lib/phrase";
import type { MutationResult } from "@/lib/volumeWrite";

export type CheckoutState =
  /** Byte-for-byte what came out. Nothing to check in. */
  | { state: "unchanged" }
  | {
      state: "modified";
      bytes: number;
      /** The edit put CRLF into a text file that had none (§6). */
      gained_crlf: boolean;
    }
  /** The working copy is gone — deleted, or the temp folder was cleaned. */
  | { state: "missing" };

export interface CheckoutRow {
  id: string;
  image: string;
  volume_index: number;
  dir_block: number;
  entry_block: number;
  name: string;
  temp_path: string;
  bytes: number;
  state: CheckoutState;
}

export interface IconPair {
  icon_name: string;
  icon_block: number;
}

/**
 * Check a file out for editing.
 *
 * A second checkout of the same file reopens the existing copy rather than
 * overwriting it: the first may hold an edit that has not gone back yet.
 */
export async function checkoutOpen(
  path: string,
  volumeIndex: number,
  dirBlock: number | null,
  entryBlock: number
): Promise<CheckoutRow> {
  return invoke<CheckoutRow>("checkout_open", {
    path,
    volumeIndex,
    dirBlock,
    entryBlock,
  });
}

/** Every file currently checked out, with its state. */
export async function checkoutList(): Promise<CheckoutRow[]> {
  return invoke<CheckoutRow[]>("checkout_list");
}

/** Open the working copy in an editor. Omit `editor` for the OS default. */
export async function checkoutEdit(id: string, editor?: string): Promise<void> {
  return invoke<void>("checkout_edit", { id, editor: editor ?? null });
}

/**
 * Write an edited file back into its image.
 *
 * `convertLineEndings` turns CRLF into LF. Only ever pass true when the user
 * asked — converting unbidden changes their file behind their back.
 */
export async function checkoutCheckin(
  id: string,
  convertLineEndings = false
): Promise<MutationResult> {
  return invoke<MutationResult>("checkout_checkin", {
    id,
    convertLineEndings,
  });
}

/** Throw an edit away. The working copy goes with it. */
export async function checkoutDiscard(id: string): Promise<void> {
  return invoke<void>("checkout_discard", { id });
}

/**
 * The `.info` icon that belongs to an entry, when the volume holds one (§7.1).
 *
 * Asks the volume rather than guessing from the name: an object without an
 * icon is completely ordinary, and offering to rename a file that is not there
 * would be noise.
 */
export async function volumeIconFor(
  path: string,
  volumeIndex: number,
  dirBlock: number | null,
  name: string
): Promise<IconPair | null> {
  return invoke<IconPair | null>("volume_icon_for", {
    path,
    volumeIndex,
    dirBlock,
    name,
  });
}

/** How to describe a checkout's state in one line. */
export function describeCheckout(row: CheckoutRow): Phrase {
  switch (row.state.state) {
    case "unchanged":
      return { key: "components.checkout.status.unchanged" };
    case "missing":
      return { key: "components.checkout.status.missing" };
    case "modified":
      return row.state.gained_crlf
        ? {
            key: "components.checkout.status.editedCrlf",
            params: { bytes: row.state.bytes.toLocaleString() },
          }
        : {
            key: "components.checkout.status.edited",
            params: { bytes: row.state.bytes.toLocaleString() },
          };
  }
}
