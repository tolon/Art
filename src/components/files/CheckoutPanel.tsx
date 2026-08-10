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
import { useTranslation } from "react-i18next";

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
  const { t } = useTranslation();
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
      !window.confirm(t("components.checkout.discardConfirm", { name: row.name }))
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
        {t("components.checkout.beingEdited", { count: rows.length })}
      </strong>
      <div className="faint" style={{ fontSize: 11, marginTop: 2 }}>
        {t("components.checkout.intro")}
      </div>

      {rows.map((row) => {
        const modified = row.state.state === "modified";
        const missing = row.state.state === "missing";
        const crlf = row.state.state === "modified" && row.state.gained_crlf;
        const statusPhrase = describeCheckout(row);

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
              {t(statusPhrase.key, statusPhrase.params)}
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
              {t("components.checkout.openInEditor")}
            </button>

            {/* §6: a size change is normal, and CRLF is offered rather than
                applied — converting unbidden would change the user's file
                behind their back, and for a startup-sequence that matters. */}
            {crlf && (
              <button
                className="btn btn-primary"
                style={{ fontSize: 11 }}
                disabled={busy !== null}
                title={t("components.checkout.crlfTitle")}
                onClick={() => void checkin(row, true)}
              >
                {t("components.checkout.putBackAmigaLineEndings")}
              </button>
            )}
            <button
              className={crlf ? "btn" : "btn btn-primary"}
              style={{ fontSize: 11 }}
              disabled={busy !== null || !modified}
              title={
                modified
                  ? t("components.checkout.putBackTitleModified")
                  : t("components.checkout.putBackTitleUnmodified")
              }
              onClick={() => void checkin(row, false)}
            >
              {crlf ? t("components.checkout.putBackAsIs") : t("components.checkout.putBack")}
            </button>

            <button
              className="btn"
              style={{ fontSize: 11 }}
              disabled={busy !== null}
              onClick={() => void discard(row)}
              title={t("components.checkout.discardTitle")}
            >
              {t("components.checkout.discard")}
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
