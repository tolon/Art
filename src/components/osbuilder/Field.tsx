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
  testId,
}: {
  label: string;
  /**
   * This row's own name, said again for a screen reader — when it needs to
   * be found as a whole (`getByLabelText`) rather than merely read
   * (`getByText`). ART-237: this used to sit as a bare `aria-label` on the
   * wrapping `<div>` below, which has no ARIA role at all — the HTML-ARIA
   * mapping strips the accessible name of a role-less element, so nothing
   * was ever announced; a linter checking only "is there an aria-label
   * somewhere" stayed quiet regardless. The row's own visible `label` above
   * the button is not a `<label htmlFor>` either, so it labels nothing
   * programmatically. The actual control here is the choose button (and the
   * clear button, when there is one) — this is placed on both, appended to
   * their own verb, so a screen reader hears "Browse…, AmigaOS 3.2 media"
   * rather than an unlabelled "Browse…" repeated once per layer. Optional: a
   * row with no `ariaLabel` renders exactly as before — the button's own
   * visible text is its accessible name, which was always a real one, just
   * not a specific one.
   */
  ariaLabel?: string;
  value: string | null;
  empty: string;
  choose: string;
  onChoose: () => void;
  hint?: string;
  clear?: string;
  onClear?: () => void;
  /**
   * A test's own way to find this row as a whole (`getByTestId`), now that
   * `aria-label` no longer sits on this `<div>` — ART-237. Never read by
   * anything but a test; a screen reader ignores `data-*` attributes
   * entirely, so this carries no accessibility meaning of its own.
   */
  testId?: string;
}) {
  return (
    <div style={{ marginBottom: 12 }} data-testid={testId}>
      <div className="muted" style={{ fontSize: 12, marginBottom: 4 }}>
        {label}
      </div>
      <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
        <button
          className="btn"
          onClick={onChoose}
          aria-label={ariaLabel ? `${choose} ${ariaLabel}` : undefined}
        >
          {choose}
        </button>
        <span style={{ fontSize: 12, wordBreak: "break-all" }}>{value ?? empty}</span>
        {onClear && (
          <button
            className="btn"
            onClick={onClear}
            aria-label={ariaLabel && clear ? `${clear} ${ariaLabel}` : undefined}
          >
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
