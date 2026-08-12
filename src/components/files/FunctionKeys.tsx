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

import { searchCharacter } from "@/lib/quickSearch";

export interface FunctionAction {
  key: string;
  label: string;
  /** Disabled keys stay visible with the reason on hover — never hidden. */
  enabled: boolean;
  /** Why it is unavailable. Shown on hover when `enabled` is false. */
  reason?: string;
  danger?: boolean;
  /**
   * This action wants Shift held — Total Commander's Shift+F6, rename in
   * place, now that F6 itself is Move.
   *
   * Shift is matched *exactly* in both directions (an action without this
   * flag only fires with Shift up), so the two F6s can never both run off one
   * keystroke. A shifted action is keyboard-only: `FunctionKeyBar` renders
   * the seven keys the bar has always had and mentions the shifted one in the
   * base key's tooltip, rather than growing a second row of buttons nobody
   * asked for.
   */
  shift?: boolean;
  /** A second line for the tooltip — what the shifted variant of this key
   *  does, since the bar itself never renders one. */
  hint?: string;
  run: () => void;
}

/**
 * Whether `event` belongs to a text field (or carries a modifier this
 * shortcut did not ask for) rather than to one of this bar's own shortcuts.
 *
 * Shared by every keyboard hook in this file so the guard can only drift in
 * one place: F6 in a rename box must type nothing and rename nothing, Tab
 * moving pane focus (`usePaneTab`) needs the same check, and so does
 * Ctrl+A selecting everything in a pane (`useSelectAll`) — the one shortcut
 * in this file that *wants* a modifier held, which is why `expectCtrl`
 * exists rather than a second, near-identical guard living next to it.
 */
function isShortcutBlocked(event: KeyboardEvent, expectCtrl = false): boolean {
  const target = event.target as HTMLElement | null;
  const tag = target?.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target?.isContentEditable) {
    return true;
  }
  // A modifier means the user is asking the OS or the browser for
  // something, not this pane — except Ctrl, exactly when the shortcut
  // itself is a Ctrl+ combination.
  if (event.altKey || event.metaKey) return true;
  return event.ctrlKey && !expectCtrl;
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

      // Shift is matched exactly, not ignored: F6 (Move) and Shift+F6
      // (rename) are different operations on the same key, and a `find` that
      // only compared `key` would give the first of the two whichever the
      // user actually pressed.
      const action = actions.find(
        (candidate) => candidate.key === event.key && Boolean(candidate.shift) === event.shiftKey
      );
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

/**
 * Bind Insert to Norton Commander's mark key: toggle the entry the pane's
 * selection anchor sits on and move the anchor down one. Owns no selection
 * state itself — `onInsert` is left to the caller (`FileManager.tsx`, via
 * `insertToggle` in `@/lib/selection`) the same way `usePaneTab`'s `onTab`
 * is.
 */
export function useInsertToggle(onInsert: () => void, active: boolean) {
  useEffect(() => {
    if (!active) return;

    function onKeyDown(event: KeyboardEvent) {
      if (event.key !== "Insert") return;
      if (isShortcutBlocked(event)) return;

      event.preventDefault();
      onInsert();
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onInsert, active]);
}

/**
 * Bind Ctrl+A to select everything in the focused pane (and, pressed again,
 * to clear it) — the one shortcut here that needs `isShortcutBlocked`'s
 * `expectCtrl` escape hatch, since Ctrl is exactly the modifier this key
 * requires rather than one that should block it.
 */
export function useSelectAll(onToggleAll: () => void, active: boolean) {
  useEffect(() => {
    if (!active) return;

    function onKeyDown(event: KeyboardEvent) {
      if (!event.ctrlKey || event.key.toLowerCase() !== "a") return;
      if (isShortcutBlocked(event, true)) return;

      event.preventDefault();
      onToggleAll();
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onToggleAll, active]);
}

/**
 * Bind F2 and Ctrl+R to re-read the focused pane.
 *
 * Brought forward from task 5's keyboard sweep because task 3 hides the pane's
 * button row by default, and Refresh was the one control in it with no other
 * way to reach it — Up is the `[..]` row, New folder is F7, and every source
 * the buttons opened is in the header's combo. A screen where the only way to
 * re-read a directory is to navigate away and back is not one to ship for the
 * length of a task.
 *
 * Two keys, one action: F2 is Total Commander's (his `AltSearch=1` config
 * leaves it free) and Ctrl+R is the reflex a browser trained everyone with.
 */
export function useRefreshKey(onRefresh: () => void, active: boolean) {
  useEffect(() => {
    if (!active) return;

    function onKeyDown(event: KeyboardEvent) {
      const isF2 = event.key === "F2" && !event.ctrlKey;
      const isCtrlR = event.ctrlKey && event.key.toLowerCase() === "r";
      if (!isF2 && !isCtrlR) return;
      if (isShortcutBlocked(event, isCtrlR)) return;

      event.preventDefault();
      onRefresh();
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onRefresh, active]);
}

/**
 * Bind the keys that walk in and out of things (brief §3.1, §3.2).
 *
 * - **Enter** and **Ctrl+PgDn** open what the cursor is on. They are the same
 *   action here: Total Commander separates them because Enter may run a file
 *   association and Ctrl+PgDn forces the *listing* instead, and ART has no
 *   associations to run — Enter already means "step inside", so the second key
 *   exists for the fingers that expect it, not for a second behaviour.
 * - **Backspace** and **Ctrl+PgUp** go up one level, container boundaries
 *   included.
 *
 * Separate from `useFunctionKeys` for the same reason `usePaneTab` is: none of
 * these is a `FunctionAction` — no label, no enabled state, nothing to draw on
 * the bar.
 */
export function useNavigationKeys(
  { onOpen, onUp }: { onOpen: () => void; onUp: () => void },
  active: boolean
) {
  useEffect(() => {
    if (!active) return;

    function onKeyDown(event: KeyboardEvent) {
      const ctrl = event.ctrlKey;
      const open = event.key === "Enter" || (ctrl && event.key === "PageDown");
      const up = event.key === "Backspace" || (ctrl && event.key === "PageUp");
      if (!open && !up) return;
      if (isShortcutBlocked(event, ctrl)) return;

      event.preventDefault();
      if (open) onOpen();
      else onUp();
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onOpen, onUp, active]);
}

/**
 * Bind Alt+Left and Alt+Right to the focused pane's own back/forward history.
 *
 * The one pair of shortcuts in this file that *wants* Alt, which
 * `isShortcutBlocked` treats as "the user is asking the OS for something" —
 * so the guard is written out here rather than given a third escape hatch for
 * a single caller. It still refuses to fire while a text field has focus,
 * which is the half of that guard that matters.
 */
export function usePaneHistoryKeys(
  { onBack, onForward }: { onBack: () => void; onForward: () => void },
  active: boolean
) {
  useEffect(() => {
    if (!active) return;

    function onKeyDown(event: KeyboardEvent) {
      if (!event.altKey || event.ctrlKey || event.metaKey) return;
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;

      const target = event.target as HTMLElement | null;
      const tag = target?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target?.isContentEditable) {
        return;
      }

      event.preventDefault();
      if (event.key === "ArrowLeft") onBack();
      else onForward();
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onBack, onForward, active]);
}

/**
 * The marking keys that are not Insert (brief §3.2).
 *
 * - **Space** marks the row under the cursor and stays there. Insert marks and
 *   steps down; both exist because both are used, for different things.
 * - **Num +** / **Num −** mark and unmark by filename mask,
 *   **Num \*** inverts.
 *
 * The numpad keys are matched on `event.code`, not `event.key`, and that is
 * load-bearing: `+`, `-` and `*` from the main keyboard are ordinary
 * characters that must reach type-to-search. Total Commander draws the same
 * line, for the same reason.
 */
export function useMarkKeys(
  {
    onSpace,
    onMarkByMask,
    onUnmarkByMask,
    onInvert,
  }: {
    onSpace: () => void;
    onMarkByMask: () => void;
    onUnmarkByMask: () => void;
    onInvert: () => void;
  },
  active: boolean
) {
  useEffect(() => {
    if (!active) return;

    function onKeyDown(event: KeyboardEvent) {
      const handler =
        event.key === " "
          ? onSpace
          : event.code === "NumpadAdd"
            ? onMarkByMask
            : event.code === "NumpadSubtract"
              ? onUnmarkByMask
              : event.code === "NumpadMultiply"
                ? onInvert
                : null;
      if (!handler) return;
      if (isShortcutBlocked(event)) return;

      event.preventDefault();
      handler();
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onSpace, onMarkByMask, onUnmarkByMask, onInvert, active]);
}

/**
 * Type-to-search: letters move the cursor to the next matching name
 * (brief §3.2, the user's `AltSearch=1`).
 *
 * Only the keystrokes come from here; every decision about what a letter does
 * lives in `@/lib/quickSearch`. Escape ends a search, and Backspace is handed
 * to the caller rather than acted on, because whether it shortens the search
 * or goes up a directory depends on whether a search is running — a question
 * this hook has no business knowing the answer to.
 */
export function useTypeAhead(
  {
    onCharacter,
    onEscape,
  }: {
    onCharacter: (character: string) => void;
    onEscape: () => void;
  },
  active: boolean
) {
  useEffect(() => {
    if (!active) return;

    function onKeyDown(event: KeyboardEvent) {
      if (isShortcutBlocked(event)) return;

      if (event.key === "Escape") {
        onEscape();
        return;
      }

      const character = searchCharacter(event);
      if (character === null) return;

      event.preventDefault();
      onCharacter(character);
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onCharacter, onEscape, active]);
}

/**
 * Alt+F1 and Alt+F2 — open the left and right pane's source box.
 *
 * Total Commander's own keys for "change this pane's drive", and the last
 * mouse-only affordance on the screen once the button strip went behind a
 * setting: without them a keyboard user could walk anywhere but could not
 * change what a pane was pointed at.
 *
 * Alt is the modifier this pair wants, so the guard is spelled out here
 * rather than adding a third escape hatch to `isShortcutBlocked` — see
 * `usePaneHistoryKeys`, which has the same shape for the same reason.
 */
export function useSourceComboKeys(
  { onLeft, onRight }: { onLeft: () => void; onRight: () => void },
  active: boolean
) {
  useEffect(() => {
    if (!active) return;

    function onKeyDown(event: KeyboardEvent) {
      if (!event.altKey || event.ctrlKey || event.metaKey) return;
      if (event.key !== "F1" && event.key !== "F2") return;

      event.preventDefault();
      if (event.key === "F1") onLeft();
      else onRight();
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onLeft, onRight, active]);
}

/**
 * Tabs (brief §3.3): **Ctrl+T** duplicates, **Ctrl+W** closes, **Ctrl+Tab**
 * cycles.
 *
 * Ctrl+Tab needs its own guard rather than `isShortcutBlocked`'s: that one is
 * shared with `usePaneTab`, where plain Tab moves pane focus, and letting
 * Ctrl+Tab through there would make the two fight over the same key. Here the
 * modifier is required, which is the whole distinction.
 */
export function useTabKeys(
  {
    onNewTab,
    onCloseTab,
    onNextTab,
  }: { onNewTab: () => void; onCloseTab: () => void; onNextTab: () => void },
  active: boolean
) {
  useEffect(() => {
    if (!active) return;

    function onKeyDown(event: KeyboardEvent) {
      if (!event.ctrlKey) return;
      const key = event.key.toLowerCase();
      const handler =
        key === "t" ? onNewTab : key === "w" ? onCloseTab : key === "tab" ? onNextTab : null;
      if (!handler) return;
      if (isShortcutBlocked(event, true)) return;

      event.preventDefault();
      handler();
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onNewTab, onCloseTab, onNextTab, active]);
}

/**
 * The docked function-key row (brief §1.4).
 *
 * **One row, always.** Total Commander's F-keys are a strip along the bottom
 * edge of the window; a strip that wraps onto a second line at a narrow width
 * is not that strip, it is a paragraph of buttons. So the keys share the width
 * equally (`flex: 1 1 0`) and the *label* is what gives way — below the width
 * this app already treats as its floor, each button shrinks to its keycap
 * alone (`.tc-fnkey-label` is hidden in CSS), which still leaves every
 * operation clickable and every one of them named on hover.
 *
 * Shifted actions (Shift+F6) are keyboard-only and are not rendered: they
 * appear in their base key's tooltip instead.
 */
export function FunctionKeyBar({ actions }: { actions: FunctionAction[] }) {
  const { t } = useTranslation();
  return (
    <div className="tc-fnkeys" role="toolbar" aria-label={t("components.fnKeys.ariaLabel")}>
      {actions
        .filter((action) => !action.shift)
        .map((action) => (
          <button
            key={action.key}
            className={`btn tc-fnkey${action.danger && action.enabled ? " tc-fnkey-danger" : ""}`}
            onClick={action.run}
            disabled={!action.enabled}
            title={[
              action.enabled
                ? `${action.key} — ${action.label}`
                : action.reason ?? t("components.fnKeys.notAvailable", { label: action.label }),
              action.hint,
            ]
              .filter(Boolean)
              .join("\n")}
          >
            <span className="faint tc-fnkey-cap">{action.key}</span>
            <span className="tc-fnkey-label">{action.label}</span>
          </button>
        ))}
    </div>
  );
}
