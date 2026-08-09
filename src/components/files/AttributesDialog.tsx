// The attributes dialog: protection bits, comment and date (brief §7.2).
//
// These bits are functional, not decorative. WHDLoad packs rely on **S**
// (script) and **P** (pure), and a Slave with the wrong bits is a game that
// does not start. So each one gets a one-line explanation rather than a bare
// checkbox with a letter next to it.
//
// Beginner Mode sees them and cannot change them (§48). That is the usual
// "hide, never disable" rule applied to the one case where showing matters
// more than editing: a beginner needs to be able to *see* why a file behaves
// the way it does.

import { useEffect, useState } from "react";

import {
  volumeAttributes,
  volumeSetAttributes,
  type AttributesView,
} from "@/lib/volumeWrite";

/** `HSPARWED`, most significant first, with what each one actually does. */
const BITS: Array<{
  letter: string;
  name: string;
  /** Bit value in the stored field. */
  value: number;
  /** True when the stored bit is inverted — a clear bit grants the right. */
  inverted: boolean;
  explanation: string;
}> = [
  {
    letter: "H",
    name: "Hold",
    value: 1 << 7,
    inverted: false,
    explanation: "Keep resident after running. Rarely used.",
  },
  {
    letter: "S",
    name: "Script",
    value: 1 << 6,
    inverted: false,
    explanation: "Run as a shell script. WHDLoad installers rely on this.",
  },
  {
    letter: "P",
    name: "Pure",
    value: 1 << 5,
    inverted: false,
    explanation: "Re-entrant, so it can be made resident. Slaves use it.",
  },
  {
    letter: "A",
    name: "Archived",
    value: 1 << 4,
    inverted: false,
    explanation: "Unchanged since the last backup.",
  },
  {
    letter: "R",
    name: "Read",
    value: 1 << 3,
    inverted: true,
    explanation: "The file can be read.",
  },
  {
    letter: "W",
    name: "Write",
    value: 1 << 2,
    inverted: true,
    explanation: "The file can be changed.",
  },
  {
    letter: "E",
    name: "Execute",
    value: 1 << 1,
    inverted: true,
    explanation: "The file can be run.",
  },
  {
    letter: "D",
    name: "Delete",
    value: 1,
    inverted: true,
    explanation: "The file can be deleted.",
  },
];

/** Whether a bit reads as set, accounting for the RWED inversion. */
function isOn(protection: number, bit: (typeof BITS)[number]): boolean {
  return bit.inverted
    ? (protection & bit.value) === 0
    : (protection & bit.value) !== 0;
}

/** Flip a bit, writing the stored field rather than the displayed one. */
function toggle(protection: number, bit: (typeof BITS)[number]): number {
  const on = isOn(protection, bit);
  // Turning a bit on means clearing it when it is inverted.
  const shouldStore = bit.inverted ? on : !on;
  return shouldStore ? protection | bit.value : protection & ~bit.value;
}

export function AttributesDialog({
  path,
  volumeIndex,
  entryBlock,
  canEdit,
  onClose,
  onChanged,
}: {
  path: string;
  volumeIndex: number;
  entryBlock: number;
  /** Power User Mode. Beginner sees the same values, read-only. */
  canEdit: boolean;
  onClose: () => void;
  onChanged: () => void;
}) {
  const [view, setView] = useState<AttributesView | null>(null);
  const [protection, setProtection] = useState(0);
  const [comment, setComment] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    volumeAttributes(path, volumeIndex, entryBlock)
      .then((found) => {
        if (cancelled) return;
        setView(found);
        setProtection(found.protection);
        setComment(found.comment);
      })
      .catch((e) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [path, volumeIndex, entryBlock]);

  const dirty =
    view !== null && (protection !== view.protection || comment !== view.comment);

  async function apply() {
    if (!view) return;
    setBusy(true);
    setError(null);
    try {
      await volumeSetAttributes(
        path,
        volumeIndex,
        entryBlock,
        // Only what changed, so applying a comment edit does not restamp the
        // bits and vice versa.
        protection === view.protection ? undefined : protection,
        comment === view.comment ? undefined : comment
      );
      onChanged();
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      role="dialog"
      aria-label="Attributes"
      className="card"
      style={{
        position: "fixed",
        top: "50%",
        left: "50%",
        transform: "translate(-50%, -50%)",
        zIndex: 50,
        width: 460,
        maxWidth: "92vw",
        maxHeight: "86vh",
        overflow: "auto",
        boxShadow: "0 8px 32px rgba(0,0,0,.5)",
      }}
    >
      <div style={{ display: "flex", justifyContent: "space-between", gap: 8 }}>
        <strong style={{ wordBreak: "break-all" }}>
          {view?.name ?? "Attributes"}
        </strong>
        <button className="btn" style={{ fontSize: 12 }} onClick={onClose}>
          Close
        </button>
      </div>

      {error && (
        <div className="badge badge-err" style={{ display: "block", margin: "8px 0" }}>
          {error}
        </div>
      )}

      {view && (
        <>
          <div className="faint" style={{ fontSize: 11, marginTop: 4 }}>
            {view.is_dir ? "Folder" : "File"} · {view.date_text} · bits{" "}
            <code>{view.bits}</code>
          </div>

          <div style={{ marginTop: 10 }}>
            {BITS.map((bit) => (
              <label
                key={bit.letter}
                style={{
                  display: "flex",
                  gap: 8,
                  alignItems: "baseline",
                  padding: "3px 0",
                  cursor: canEdit ? "pointer" : "default",
                }}
              >
                <input
                  type="checkbox"
                  checked={isOn(protection, bit)}
                  disabled={!canEdit || busy}
                  onChange={() => setProtection(toggle(protection, bit))}
                  aria-label={bit.name}
                />
                <span style={{ width: 76, fontSize: 12 }}>
                  <strong>{bit.letter}</strong> {bit.name}
                </span>
                <span className="faint" style={{ fontSize: 11, flex: 1 }}>
                  {bit.explanation}
                </span>
              </label>
            ))}
          </div>

          <label style={{ display: "block", marginTop: 10, fontSize: 12 }}>
            Comment
            <input
              value={comment}
              onChange={(event) => setComment(event.target.value)}
              disabled={!canEdit || busy}
              maxLength={79}
              placeholder="AmigaDOS keeps up to 79 characters"
              style={{ width: "100%", marginTop: 4 }}
            />
          </label>

          {!canEdit && (
            <div className="faint" style={{ fontSize: 11, marginTop: 8 }}>
              Shown for reference. Switch to Power User Mode in Settings to
              change them.
            </div>
          )}

          {canEdit && (
            <div style={{ display: "flex", gap: 6, marginTop: 12 }}>
              <button
                className="btn btn-primary"
                onClick={() => void apply()}
                disabled={busy || !dirty}
                style={{ fontSize: 12 }}
              >
                Apply
              </button>
              <button
                className="btn"
                style={{ fontSize: 12 }}
                disabled={busy || !dirty}
                onClick={() => {
                  setProtection(view.protection);
                  setComment(view.comment);
                }}
              >
                Reset
              </button>
            </div>
          )}
        </>
      )}
    </div>
  );
}
