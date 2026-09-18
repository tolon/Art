// Where the card run's own system tree lives while the run lives — and
// nowhere else (round 4 final review, C3).
//
// **The defect this replaces.** The run handed `card_os_open`'s tree to
// `setTree`, which writes `buildSession.tree` — a *persisted* key, per the
// settings store, that five screens (`AppearancePanel`, `NetworkPanel`,
// `VerifyAgainstCard`, `TitleDetail`, the step banner) and `FilesTab` read as
// **the user's own distribution root**. So one card run wrote ART's scratch
// path into it, then set it to `null` in the `finally` that runs on every
// path — success included — and `setTree` clears `firstboot.written` whenever
// the root changes. A user who had built a folder distribution and then built
// a card found all of that gone, having changed nothing: the one rule the
// owner states unconditionally ("kullanıcı değiştirmeden hiçbir ayar
// değişmemeli"), and R2's intent reached through `session.tree` instead of
// through `osinstall.destination`.
//
// **Why a store and not a prop.** The two ends are on different tabs: the run
// is `BuildTab`'s and the measurement is `CardSection`'s, on the Machine tab,
// with the whole shell between them. A store is the narrowest thing that
// crosses that gap without touching anything remembered.
//
// **Nothing here is persisted, by construction.** It is plain zustand state,
// not `remembered.ts`, so there is no key, nothing survives a restart, and a
// crash mid-run leaves a scratch path in no file (Task 10's own deferred
// concern, closed with C3). The run sets it on open and clears it on close;
// `CardSection` prefers it over `session.tree.root` while it is set, which is
// exactly the window in which the card's tree exists.

import { create } from "zustand";

interface CardRunTreeState {
  /** The open session's `tree/`, or `null` when no card run holds one. */
  root: string | null;
  /** Said by `useCardOsRun` alone — on open, and again (with `null`) when
   *  the session closes, on every path. */
  setRoot: (root: string | null) => void;
}

export const useCardRunTreeStore = create<CardRunTreeState>((set) => ({
  root: null,
  setRoot: (root) => set({ root }),
}));

/** The run's tree, for a component that only reads it. */
export function useCardRunTree(): string | null {
  return useCardRunTreeStore((state) => state.root);
}
