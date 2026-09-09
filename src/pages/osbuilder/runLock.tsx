// One fact the whole install lane has to agree on: **is a build running right
// now** (round 4 whole-branch review, I2).
//
// **The defect.** `useBuildRun` sequences a build — the tree, then each ticked
// update, then first boot — inside `BuildTab`. Leaving tab 4 mid-run unmounts
// that component, and with it the loop that starts the next phase and the
// hand-off that gives the finished tree to the session: a run stops halfway,
// silently, because somebody clicked *Amiga dosyaları* to check which folder
// they had pointed at. Nothing said so, before or after. That is this
// project's named failure in its quietest form — no crash, no message, and a
// half-written tree the screen still describes as a build in progress.
//
// **The answer is a lock, not a rescue.** Keeping the run alive across a
// navigation would mean lifting the whole sequencer out of the tab, and a run
// nobody is looking at is a second way to be wrong about what is happening.
// What the shell does instead is refuse the navigation while a run is in
// flight, and say why: *Stop it first*. Stop is a real, offered control — the
// refusal is actionable, which is the other half of CLAUDE.md's rule.
//
// **Where the state lives.** `Layout` holds it — the application shell —
// because the strip inside the OS Builder was never the only way out of the
// lane. Fifteen sidebar entries and the dashboard's drop cards navigate away
// just as effectively, and a lock that covers four chips out of twenty is a
// screen that has refused where it was watched and allowed it everywhere
// else. `OsBuilder` only *reads* the flag now; `BuildTab` sets it while its
// run is running and clears it on unmount, so a tab that goes away for any
// other reason cannot leave the shell locked for ever. Anything mounted
// outside a provider — every panel test, every screen rendered without the
// shell — reads the default, which is *not running*: a lock has to be
// switched on by a run, and never by an absent one.

import { createContext, useContext } from "react";

export interface RunLock {
  /** Whether a build is in flight in this lane. */
  running: boolean;
  /** Said by the tab that owns the run, and by nothing else. */
  setRunning: (value: boolean) => void;
}

/**
 * The default is a lock that is **off and cannot be turned on**.
 *
 * Not a throwing "used outside its provider" guard: `BuildTab` is rendered on
 * its own by its own test file, and by any future screen that wants the build
 * summary without the lane's shell around it. A missing provider means there
 * is no strip to lock, which is a situation rather than a fault.
 */
export const RunLockContext = createContext<RunLock>({
  running: false,
  setRunning: () => {},
});

/** The lane's run lock. Read `running` to draw; call `setRunning` only from
 *  the component that actually owns the run. */
export function useRunLock(): RunLock {
  return useContext(RunLockContext);
}
