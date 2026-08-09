// F4 — the checkout panel (brief §6).
//
// ART does not implement an editor, so this is the whole feature: the file is
// out, here is where it went, open it in whatever you use, and put it back
// when you are done.
//
// Refreshed on a timer rather than by watching the filesystem: a poll every
// couple of seconds costs one hash of a file the user is actively editing, and
// a watcher would mean a filesystem-notification dependency and a permission
// prompt for a feature that does not need either.

import { useCallback, useEffect, useState } from "react";

import {
  checkoutCheckin,
  checkoutDiscard,
  checkoutEdit,
  checkoutList,
  describeCheckout,
  type CheckoutRow,
} from "@/lib/checkout";

/** How often to re-hash the working copies while the panel is on screen. */
const POLL_MS = 2000;

export function CheckoutPanel({
  editor,
  onChanged,
  onError,
}: {
  /** The user's configured editor, or undefined for the OS default. */
  editor?: string;
  /** Called after a successful checkin, so the pane can re-list. */
  onChanged: (row: CheckoutRow) => void;
  onError: (message: string) => void;
}) {
  const [rows, setRows] = useState<CheckoutRow[]>([]);
  const [busy, setBusy] = useState<string | null>(null);

  const refresh = useCallback(() => {
    checkoutList()
      .then(setRows)
      .catch((e) => onError(String(e)));
  }, [onError]);

  useEffect(() => {
    refresh();
    const timer = window.setInterval(refresh, POLL_MS);
    return () => window.clearInterval(timer);
  }, [refresh]);

  if (rows.length === 0) return null;

  async function checkin(row: CheckoutRow, convert: boolean) {
    setBusy(row.id);
    try {
      await checkoutCheckin(row.id, convert);
      onChanged(row);
      refresh();
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function discard(row: CheckoutRow) {
    if (
      row.state.state === "modified" &&
      !window.confirm(
        `Throw away your changes to ${row.name}? The working copy will be deleted.`
      )
    ) {
      return;
    }
    setBusy(row.id);
    try {
      await checkoutDiscard(row.id);
      refresh();
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(null);
    }
  }

  return (
    <section className="card" style={{ marginTop: 12 }}>
      <strong style={{ fontSize: 14 }}>
        Being edited ({rows.length})
      </strong>
      <div className="faint" style={{ fontSize: 11, marginTop: 2 }}>
        Each of these is a copy on your disk. Nothing goes back into the image
        until you put it back, and only if it actually changed.
      </div>

      {rows.map((row) => {
        const modified = row.state.state === "modified";
        const missing = row.state.state === "missing";
        const crlf = row.state.state === "modified" && row.state.gained_crlf;

        return (
          <div
            key={row.id}
            style={{
              borderTop: "1px solid var(--border)",
              padding: "6px 0",
              display: "flex",
              gap: 8,
              alignItems: "baseline",
              flexWrap: "wrap",
            }}
          >
            <span style={{ minWidth: 140, fontSize: 12, wordBreak: "break-all" }}>
              {row.name}
            </span>
            <span
              className={
                missing ? "badge badge-warn" : modified ? "badge badge-ok" : "badge"
              }
              style={{ fontSize: 11 }}
            >
              {describeCheckout(row)}
            </span>
            <span style={{ flex: 1 }} />

            <button
              className="btn"
              style={{ fontSize: 11 }}
              disabled={busy !== null || missing}
              onClick={() => {
                checkoutEdit(row.id, editor).catch((e) => onError(String(e)));
              }}
            >
              Open in editor
            </button>

            {/* §6: a size change is normal, and CRLF is offered rather than
                applied — converting unbidden would change the user's file
                behind their back, and for a startup-sequence that matters. */}
            {crlf && (
              <button
                className="btn btn-primary"
                style={{ fontSize: 11 }}
                disabled={busy !== null}
                title="Windows line endings behave differently on a real Amiga"
                onClick={() => void checkin(row, true)}
              >
                Put back, Amiga line endings
              </button>
            )}
            <button
              className={crlf ? "btn" : "btn btn-primary"}
              style={{ fontSize: 11 }}
              disabled={busy !== null || !modified}
              title={
                modified
                  ? "Write it back into the image"
                  : "Nothing has changed, so there is nothing to write back"
              }
              onClick={() => void checkin(row, false)}
            >
              {crlf ? "Put back as it is" : "Put back"}
            </button>

            <button
              className="btn"
              style={{ fontSize: 11 }}
              disabled={busy !== null}
              onClick={() => void discard(row)}
              title="Throw the working copy away. The image is not touched."
            >
              Discard
            </button>

            <div
              className="faint"
              style={{ fontSize: 10, width: "100%", wordBreak: "break-all" }}
            >
              {row.temp_path}
            </div>
          </div>
        );
      })}
    </section>
  );
}
