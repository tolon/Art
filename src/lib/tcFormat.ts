// Formatting for the Total Commander-styled Files screen (task 6b): a fixed
// date layout TC itself uses regardless of locale, and a thousands-grouped
// size that *does* follow the user's locale rather than hardcoding the dots
// the reference screenshot happens to show (that screenshot's Windows was
// set to a locale that groups with dots — English groups with commas, and
// this app ships both, so the grouping has to come from the caller's active
// language, not be baked in here).
//
// Pure, like the rest of `src/lib` — no i18next singleton. The caller (a
// component that already has `useTranslation()`) supplies `i18n.language` as
// the locale.

/**
 * `DD.MM.YYYY HH:MM`, in local time — Total Commander's own date column
 * format, independent of the active UI language. `null` in, `null` out: a
 * `PanelEntry` with no date (an ADF entry ART could not read one from) has
 * nothing to show, and the caller decides what an empty date cell looks like
 * rather than this function inventing a placeholder.
 */
export function formatDateTC(unixSeconds: number | null): string | null {
  if (unixSeconds === null) return null;

  const date = new Date(unixSeconds * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");

  return (
    `${pad(date.getDate())}.${pad(date.getMonth() + 1)}.${date.getFullYear()} ` +
    `${pad(date.getHours())}:${pad(date.getMinutes())}`
  );
}

/**
 * A byte count with the active locale's thousands grouping — Turkish and
 * German-family locales group with `.`, matching the reference screenshot;
 * English groups with `,`. `Math.trunc` first: a `PanelEntry.bytes` is
 * already a whole number, but this stays correct even if a future source
 * ever hands one a fractional value, rather than rendering a decimal point where
 * the grouping character normally goes.
 */
export function formatGroupedSize(bytes: number, locale: string): string {
  return Math.trunc(bytes).toLocaleString(locale);
}
