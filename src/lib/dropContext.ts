// What the one global drop listener hands to the screens, in one type.
//
// **Why this is its own module** (round 4 final review, C1). `Layout.tsx`
// provided this object and `CardSection.tsx` declared its own private shape
// for reading it back, so nothing connected the two: the shell could stop
// sending a field and the reader would go on compiling, reading `undefined`
// for ever. One exported type, named by both ends, is what makes a missing
// field a build error instead of a screen that quietly does nothing.
//
// **Why `lastDrop` exists beside `analyses`/`dropPosition`** (I1). Those two
// change at different moments: `dropPosition` is set on `enter` and `over` as
// well as on `drop`, while `analyses` changes only on `drop`. A reader that
// joins them itself therefore sees *the previous drop's paths* at *the current
// pointer position* on every mouse move of every later drag — which, for the
// card's per-row intake, silently appended an hour-old path to whichever row
// the pointer crossed. The position a drop happened at belongs **to** that
// drop, so it travels with it in one value, stamped with a sequence number the
// reader keys its effect on. `seq` rather than a timestamp: two drops in the
// same millisecond are two drops.

import type { DroppedAnalysis } from "@/types";

/** A point in CSS pixels — `dnd.ts` converts once, with `cssPointOf`. */
export interface DropPoint {
  x: number;
  y: number;
}

/** One completed drop: what was dropped, where, and which drop it is. */
export interface LastDrop {
  /** The dropped paths, in the order Tauri reported them. */
  paths: string[];
  /** Where the pointer was, in CSS pixels, or `null` when Tauri said
   *  nothing — some platforms send no position at all. */
  at: DropPoint | null;
  /** Monotonic, one per drop. What a reader keys its effect on, so a repeat
   *  drop of the same paths at the same point is still a second drop. */
  seq: number;
}

/** Everything the shell's `<Outlet context={…}>` carries. */
export interface DropContext {
  analyses: DroppedAnalysis[];
  dragOver: boolean;
  /** The live pointer position during a drag — `enter`, `over` and `drop`.
   *  A highlight may follow it; nothing that *acts* may, because it moves
   *  without anything having been dropped (I1). */
  dropPosition: DropPoint | null;
  /** The last completed drop, or `null` before one and after a `leave`. */
  lastDrop: LastDrop | null;
}
