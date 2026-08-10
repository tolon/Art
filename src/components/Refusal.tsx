// "ART will not do this, and here is why."
//
// Deliberately not the error banner. An error means something broke and
// carries an ART-* identifier so it can be looked up (§68). A refusal is the
// normal answer to a question the user asked — it broke nothing, it needs no
// identifier, and presenting it in red with a code teaches the user to read
// ART's real failures as noise.

import type { ReactNode } from "react";

export function Refusal({
  title,
  reason,
  suggestion,
}: {
  /** What ART will not do, in the user's words. */
  title: string;
  /** Why not. Complete sentences; this is the whole explanation. */
  reason: string;
  /** What they can do instead, when there is something. */
  suggestion?: ReactNode;
}) {
  return (
    <div
      role="status"
      className="card"
      style={{
        borderColor: "var(--warn)",
        background: "color-mix(in srgb, var(--warn) 8%, var(--bg-panel))",
        marginTop: 8,
      }}
    >
      <strong style={{ fontSize: 13 }}>{title}</strong>
      <div style={{ fontSize: 13, marginTop: 4 }}>{reason}</div>
      {suggestion && (
        <div className="faint" style={{ fontSize: 12, marginTop: 6 }}>
          {suggestion}
        </div>
      )}
    </div>
  );
}
