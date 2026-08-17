// Does the Kickstart on this card suit the folders about to go onto it? (G9)
//
// The preload screen's half of `core::rom::pairing`. Extracted from
// `VolumePreload.tsx` so the question — which folders are asked about, what
// happens when the command fails, and what is on screen while the answer is
// in flight — is testable without a Tauri window.

import { useEffect, useRef, useState } from "react";

import {
  foldersToCheck,
  pairingStillApplies,
  preloadRomPairing,
  type Pairing,
  type PairingFor,
  type PartitionPick,
} from "@/lib/preload";

/** What a folder's verdict is when the command itself would not answer. */
const CHECK_FAILED: Pairing = { verdict: "not-checked", why: "check-failed" };

/** What the screen renders: the verdicts, and whether more are coming. */
export interface RomPairingState {
  /** `true` from the moment folders are asked about until every answer is in. */
  checking: boolean;
  results: PairingFor[];
}

/**
 * Ask about every chosen folder, and hold the answers until the question
 * changes.
 *
 * The question is the card plus the folders — **not** the whole preload
 * request. Typing a volume name changes the request and changes nothing about
 * which ROM suits which folder, and the deps used to include it: nine
 * keystrokes into the volume-name box issued nine reads of a manifest that is
 * a megabyte on real material.
 */
export function useRomPairing(
  imagePath: string | null,
  picks: PartitionPick[]
): RomPairingState {
  const [state, setState] = useState<RomPairingState>({ checking: false, results: [] });
  const heldFor = useRef<string | null>(null);

  const folders = foldersToCheck(picks);
  // The whole question, and the only thing the effect depends on: the card
  // and the folders. `folders` is read inside the effect from this render's
  // closure, which is the render `question` was built from — so the two
  // cannot disagree, and nothing else in the preload request can make this
  // run again.
  const question = JSON.stringify([imagePath, folders]);

  useEffect(() => {
    // Held verdicts are dropped *before* the new answers are asked for, not
    // when they arrive: a verdict left on screen until its replacement
    // resolves describes a card or a folder that is no longer chosen.
    if (!pairingStillApplies(heldFor.current, question)) {
      setState({ checking: false, results: [] });
      heldFor.current = null;
    }
    if (!imagePath || folders.length === 0) return;

    let cancelled = false;
    setState({ checking: true, results: [] });
    void Promise.all(
      folders.map(async (folder) => ({
        driveName: folder.driveName,
        // Per folder, so one unreadable folder does not take the other
        // folders' verdicts down with it — and never `null`: a rejection
        // that renders as silence is the reassuring verdict spelt out of a
        // failure (§89).
        pairing: await preloadRomPairing(imagePath, folder.content).catch(() => CHECK_FAILED),
      }))
    ).then((results) => {
      if (cancelled) return;
      setState({ checking: false, results });
      heldFor.current = question;
    });
    return () => {
      cancelled = true;
    };
  }, [question]);

  return state;
}
