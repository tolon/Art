// The drag handle at a header cell's right edge: it resizes the column it
// sits on (design § 5.7). Live while the pointer moves, written once when it
// is released — a settings write per pointer event is the ART-178 shape of
// defect wearing a different hat.

import { useRef } from "react";

import { widthEmFromPx } from "@/lib/columns";

export interface ColumnGripProps {
  /** The column's accessible name for the handle. */
  label: string;
  /** Its width when the drag starts, in `em`. */
  startEm: number;
  /** The listing's text size, which the width is measured in. */
  fontPx: number;
  /** Each move: the width to draw now. Nothing is stored. */
  onPreview: (em: number) => void;
  /** The release: the one width to keep. */
  onCommit: (em: number) => void;
}

export function ColumnGrip({ label, startEm, fontPx, onPreview, onCommit }: ColumnGripProps) {
  const drag = useRef<{ startX: number; pointerId: number } | null>(null);

  const widthAt = (clientX: number) =>
    drag.current
      ? widthEmFromPx(startEm * fontPx + (clientX - drag.current.startX), fontPx)
      : startEm;

  return (
    <span
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      className="tc-header-grip"
      onPointerDown={(event) => {
        // Not a sort, not a row click, not a text selection.
        event.preventDefault();
        event.stopPropagation();
        drag.current = { startX: event.clientX, pointerId: event.pointerId };
        event.currentTarget.setPointerCapture?.(event.pointerId);
      }}
      onPointerMove={(event) => {
        if (!drag.current) return;
        onPreview(widthAt(event.clientX));
      }}
      onPointerUp={(event) => {
        if (!drag.current) return;
        const em = widthAt(event.clientX);
        event.currentTarget.releasePointerCapture?.(drag.current.pointerId);
        drag.current = null;
        onCommit(em);
      }}
      onClick={(event) => event.stopPropagation()}
    />
  );
}
