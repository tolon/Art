import { useTranslation } from "react-i18next";

/**
 * "The volume was already damaged before ART wrote to it."
 *
 * Its own component, and tested as one, because the finding this exists for
 * (ART-050's F3, and G2 after it) was that the engine carried this all the way
 * to the frontend's types and **nothing drew it** — a field nothing renders is
 * the same silence with more code behind it. A component can be rendered in a
 * test without standing a whole two-pane commander up around it, so the claim
 * "this reaches the user" is checked rather than asserted.
 *
 * Deliberately a fourth level, not a fifth thing competing for the status
 * line: it is **not an error** — the write landed — and **not a hint** —
 * nothing was declined. It says both halves plainly: what ART found, and that
 * it found it *before* writing.
 *
 * Renders nothing at all when there is no damage, which is the ordinary case.
 */
export function DamageRow({ findings }: { findings: string[] }) {
  const { t } = useTranslation();
  if (findings.length === 0) return null;

  return (
    <div className="tc-chrome-row tc-message-row tc-message-hint" role="status">
      {t("files.damage.foundBeforeWriting", { count: findings.length })}{" "}
      {findings.slice(0, 3).join(" · ")}
    </div>
  );
}
