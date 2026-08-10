// The function-key bar, and the keyboard that drives it (brief §8).
//
// Norton Commander's conventions, because that is what the user asked for and
// what forty years of file managers trained everyone on:
//
//   F3 View · F4 Edit · F5 Copy · F6 Move/Rename · F7 MkDir · F8 Delete
//
// The bar is not decoration. It is the discoverable half of the keyboard: a
// user who never presses a function key still sees what the eight operations
// are and can click them.
//
// Danger is labelled by the same Safety classes the rest of ART uses — Delete
// is `Destructive` and gets a red key and a double confirmation upstream (§63).

import { useEffect } from "react";
import { useTranslation } from "react-i18next";

export interface FunctionAction {
  key: string;
  label: string;
  /** Disabled keys stay visible with the reason on hover — never hidden. */
  enabled: boolean;
  /** Why it is unavailable. Shown on hover when `enabled` is false. */
  reason?: string;
  danger?: boolean;
  run: () => void;
}

/**
 * Whether `event` belongs to a text field (or carries a modifier) rather
 * than to one of this bar's own shortcuts.
 *
 * Shared by every keyboard hook in this file so the guard can only drift in
 * one place: F6 in a rename box must type nothing and rename nothing, and
 * the same holds for Tab moving pane focus (`usePaneTab` below).
 */
function isShortcutBlocked(event: KeyboardEvent): boolean {
  const target = event.target as HTMLElement | null;
  const tag = target?.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target?.isContentEditable) {
    return true;
  }
  // A modifier means the user is asking the OS or the browser for
  // something, not this pane.
  return event.ctrlKey || event.altKey || event.metaKey;
}

/**
 * Wire the function keys to `actions`.
 *
 * Deliberately ignores keystrokes while focus is in a text field: F6 in a
 * rename box should type nothing and rename nothing.
 */
export function useFunctionKeys(actions: FunctionAction[], active: boolean) {
  useEffect(() => {
    if (!active) return;

    function onKeyDown(event: KeyboardEvent) {
      if (isShortcutBlocked(event)) return;

      const action = actions.find((candidate) => candidate.key === event.key);
      if (!action) return;

      event.preventDefault();
      if (action.enabled) action.run();
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [actions, active]);
}

/**
 * Bind Tab to move keyboard focus to the other pane.
 *
 * A sibling of `useFunctionKeys` rather than a case inside it: Tab is not a
 * `FunctionAction` (it has no label, no enabled/disabled state, nothing to
 * show on the key bar), just a second shortcut that happens to want the same
 * guard. `onTab` is left to the caller to decide what "the other pane" means
 * — this hook only owns the keyboard wiring, not pane state.
 */
export function usePaneTab(onTab: () => void, active: boolean) {
  useEffect(() => {
    if (!active) return;

    function onKeyDown(event: KeyboardEvent) {
      if (event.key !== "Tab") return;
      if (isShortcutBlocked(event)) return;

      event.preventDefault();
      onTab();
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onTab, active]);
}

export function FunctionKeyBar({ actions }: { actions: FunctionAction[] }) {
  const { t } = useTranslation();
  return (
    <div
      style={{
        display: "flex",
        gap: 4,
        marginTop: 12,
        flexWrap: "wrap",
      }}
      role="toolbar"
      aria-label={t("components.fnKeys.ariaLabel")}
    >
      {actions.map((action) => (
        <button
          key={action.key}
          className="btn"
          onClick={action.run}
          disabled={!action.enabled}
          title={
            action.enabled
              ? `${action.key} — ${action.label}`
              : action.reason ?? t("components.fnKeys.notAvailable", { label: action.label })
          }
          style={{
            flex: "1 1 90px",
            fontSize: 12,
            borderColor: action.danger && action.enabled ? "var(--err)" : undefined,
            color: action.danger && action.enabled ? "var(--err)" : undefined,
          }}
        >
          <span className="faint" style={{ marginRight: 5 }}>
            {action.key}
          </span>
          {action.label}
        </button>
      ))}
    </div>
  );
}
