// One "here is a path, Browse for another" row.
//
// Extracted from `CardBuilder` when the volume preload screen needed the same
// row: a file picker with the chosen path beside it, an optional hint under it
// and an optional Clear. Both screens ask for paths the same way, and a second
// hand-written copy of this is how two screens start disagreeing about what a
// chosen file looks like.

export function Field({
  label,
  ariaLabel,
  value,
  empty,
  choose,
  onChoose,
  hint,
  clear,
  onClear,
}: {
  label: string;
  /**
   * The row's own accessible name, when it needs to be found as a whole
   * (`getByLabelText`) rather than merely read (`getByText`). Optional: a row
   * with no `ariaLabel` is exactly what this component has always rendered —
   * a visible label with no accessible name of its own — since there is
   * nothing here to label the way a `<label htmlFor>` labels an `<input>`,
   * only a button and a span.
   */
  ariaLabel?: string;
  value: string | null;
  empty: string;
  choose: string;
  onChoose: () => void;
  hint?: string;
  clear?: string;
  onClear?: () => void;
}) {
  return (
    <div style={{ marginBottom: 12 }} {...(ariaLabel ? { "aria-label": ariaLabel } : {})}>
      <div className="muted" style={{ fontSize: 12, marginBottom: 4 }}>
        {label}
      </div>
      <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
        <button className="btn" onClick={onChoose}>
          {choose}
        </button>
        <span style={{ fontSize: 12, wordBreak: "break-all" }}>{value ?? empty}</span>
        {onClear && (
          <button className="btn" onClick={onClear}>
            {clear}
          </button>
        )}
      </div>
      {hint && (
        <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
          {hint}
        </p>
      )}
    </div>
  );
}
