// The one-button card's own target (round 4, card-round-4 design § 3, § 7):
// what the Machine tab's *Card image* destination asks the user for, once
// automatic sizing has taken away everything it can.
//
// **A separate module, not a section of `buildSession.ts`** (task 5's own
// brief). `CardTarget` and `DestinationKind` are new to this round — there is
// no legacy key to migrate, unlike every section `buildSession.ts` already
// carries — and `CARD_SIZES_GB` has to be reachable from `CardBuilder.tsx`
// and `OsBuilder.tsx` too, which do not otherwise import from the session
// facade. Owner's decision Q1: those two screens keep their own list for now
// and are pointed here in round 4's task 7; this module does not touch them.

import { isTextList } from "@/lib/remembered";

/**
 * The Machine tab's destination choice (design § 3): the OS Builder's tree
 * written to a folder, as it always has been, or onto a PiStorm card image.
 *
 * `osinstall.destination` keeps its present meaning either way (owner
 * decision 2; pre-flight ruling R2) — this is a new choice sitting beside it,
 * never a replacement for it.
 */
export type DestinationKind = "folder" | "card-image";

/**
 * One content partition and what fills it.
 *
 * `name` is System, Work, or one the user added (Games, Stuff, a typed name —
 * design § 3); `sources` are host paths, each recognised by
 * `cardOsClassify`. `floorBytes` is the *Advanced* override: a floor on an
 * automatically sized partition, never a way past the card's total (design
 * § 4).
 */
export interface CardPartitionTarget {
  name: string;
  sources: string[];
  floorBytes?: number;
}

/**
 * What the Machine tab's card mode asks the user for, once automatic sizing
 * (design § 4) has taken away everything it can derive: the card's printed
 * size, where the image goes, the user's own Emu68 archive, the PFS3 driver
 * found in their material, and the partitions they are filling.
 */
export interface CardTarget {
  sizeGb: number;
  image: string | null;
  emu68Archive: string | null;
  pfs3Driver: string | null;
  partitions: CardPartitionTarget[];
}

/**
 * Card sizes people actually buy, as printed on the card (design § 3's
 * mock-up, plus 256 for the largest real cards the research measured).
 *
 * **The one list** (owner decision Q1) — three existed before this round:
 * `CardBuilder.tsx`'s own, `OsBuilder.tsx`'s own, and the sizes this session
 * remembers. A fourth would make four. `CardBuilder.tsx` and `OsBuilder.tsx`
 * keep their local lists for this task (round 4's task 7 points them here,
 * not this one) — but nothing else in the tree declares a second copy of the
 * five numbers below.
 */
export const CARD_SIZES_GB = [16, 32, 64, 128, 256] as const;

/**
 * System and Work always exist (design § 3); neither starts with a source,
 * because nothing has been dropped onto either yet. 64 GB matches the
 * design's own mock-up default (§ 3: "Card size [64 GB ▾]").
 */
export const DEFAULT_CARD_TARGET: CardTarget = {
  sizeGb: 64,
  image: null,
  emu68Archive: null,
  pfs3Driver: null,
  partitions: [
    { name: "System", sources: [] },
    { name: "Work", sources: [] },
  ],
};

/**
 * One partition, checked the way `isMaterialFolders` checks one folder
 * (`buildSession.ts`): a `name` that is not a non-empty string, or a
 * `sources` that is not an array of strings, is not a slightly-off partition
 * to repair — it is a different shape, and `isCardTarget` rejects the whole
 * target for it rather than dropping just this entry.
 */
function isCardPartitionTarget(value: unknown): value is CardPartitionTarget {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as CardPartitionTarget).name === "string" &&
    (value as CardPartitionTarget).name !== "" &&
    isTextList((value as CardPartitionTarget).sources) &&
    ((value as CardPartitionTarget).floorBytes === undefined ||
      typeof (value as CardPartitionTarget).floorBytes === "number")
  );
}

/**
 * The whole-target guard (T gap 9 of the round's pre-flight scan).
 *
 * A stored `CardTarget` missing `partitions` entirely, or holding one
 * partition whose `sources` is not an array of strings, is rejected
 * **whole** — never repaired field by field. `isMaterialFolders` sets the
 * pattern: the field *is* the list, so one bad entry drops the whole value
 * back to `DEFAULT_CARD_TARGET` rather than handing the screen a card target
 * partly trusted, which is a shape nobody chose.
 */
export function isCardTarget(value: unknown): value is CardTarget {
  if (typeof value !== "object" || value === null) return false;
  const target = value as CardTarget;
  return (
    typeof target.sizeGb === "number" &&
    (target.image === null || typeof target.image === "string") &&
    (target.emu68Archive === null || typeof target.emu68Archive === "string") &&
    (target.pfs3Driver === null || typeof target.pfs3Driver === "string") &&
    Array.isArray(target.partitions) &&
    target.partitions.every(isCardPartitionTarget)
  );
}

/**
 * Where one release's card target persists in `settings.remembered`.
 *
 * Per release (owner decision Q2), the way `SESSION_KEYS.material` is: a
 * 3.9 card and a 3.2.2 card are two cards, with two different partition
 * lists and two different images.
 */
export const CARD_TARGET_KEY = (release: string): string => `osinstall.cardTarget.${release}`;

/**
 * Where one release's destination kind persists.
 *
 * Its own key rather than a rename of `osinstall.destination` — that key
 * keeps its present meaning for the five screens that already read it (R2),
 * and this is a new choice sitting beside it, not inside it.
 */
export const DESTINATION_KIND_KEY = (release: string): string =>
  `osinstall.destinationKind.${release}`;
