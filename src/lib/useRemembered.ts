// The React side of `@/lib/remembered`: a choice that outlives the screen.
//
// Reach for these instead of `useState` whenever the value is something the
// *user decided* — a view mode, a filter, a filesystem, a folder. Use plain
// `useState` for what the screen is merely doing: the file it has open, a busy
// flag, an error, the text of a dialog being typed into.
//
// The test for which one a value is: would the user be annoyed to set it
// again tomorrow? Then it belongs here.

import { useCallback } from "react";

import { useSettingsStore } from "@/stores/settingsStore";
import { type Guard, recall, recallInto, remember } from "@/lib/remembered";

/**
 * One remembered value and a setter that persists it.
 *
 * The setter reads the store through `getState()` rather than closing over the
 * rendered value, so two screens — or two controls in one screen — writing in
 * the same tick cannot overwrite each other with a stale copy of the bag.
 *
 * There is no "wait until settings have loaded" here on purpose: writes happen
 * only when the user acts, and `settingsStore` keeps a user's change from being
 * undone by a load that lands afterwards. What the screen shows before the load
 * arrives is the fallback, and it re-renders when the real value comes in.
 */
export function useRemembered<T>(
  key: string,
  isValid: Guard<T>,
  fallback: T
): [T, (next: T) => void] {
  const stored = useSettingsStore((s) => s.settings.remembered);
  const update = useSettingsStore((s) => s.update);

  const value = recall(stored, key, isValid, fallback);

  const set = useCallback(
    (next: T) => {
      const latest = useSettingsStore.getState().settings.remembered;
      void update({ remembered: remember(latest, key, next) });
    },
    [key, update]
  );

  return [value, set];
}

/**
 * A remembered object, rebuilt field by field, with a setter that takes a
 * partial change — the shape a settings panel actually wants.
 *
 * `apply({ autoBoot: true })` rather than spreading the whole object at every
 * call site, because spreading a stale copy is how one control quietly reverts
 * another.
 */
export function useRememberedShape<T extends object>(
  key: string,
  spec: { [K in keyof T]: Guard<T[K]> },
  fallback: T
): [T, (change: Partial<T>) => void] {
  const stored = useSettingsStore((s) => s.settings.remembered);
  const update = useSettingsStore((s) => s.update);

  const value = recallInto<T>(stored, key, spec, fallback);

  const apply = useCallback(
    (change: Partial<T>) => {
      const latest = useSettingsStore.getState().settings.remembered;
      const current = recallInto<T>(latest, key, spec, fallback);
      void update({ remembered: remember(latest, key, { ...current, ...change }) });
    },
    // `spec` and `fallback` are module constants at every call site; listing
    // them would make this callback change identity on every render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [key, update]
  );

  return [value, apply];
}
