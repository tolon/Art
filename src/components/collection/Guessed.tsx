import { useTranslation } from "react-i18next";

import { isStated, provenancePhrase, type Provenance } from "@/lib/gameindex";

/**
 * A small mark on any value the index **guessed** rather than read.
 *
 * This is the feature the provenance in the record exists for. `Agassi Tennis`
 * reads as AGA because the letters are in its filename; a slave that states
 * `ReqAGA` is a different claim entirely, and a screen showing the two the
 * same way throws away the only thing that separates them.
 */
export function Guessed({ from }: { from: Provenance | null }) {
  const { t } = useTranslation();
  if (!from || isStated(from)) return null;
  const source = t(provenancePhrase(from).key);
  return (
    <span
      className="badge badge-muted"
      title={t("gameindex.guessedFrom", { source })}
      style={{ fontSize: 9, marginLeft: 4, verticalAlign: "middle" }}
    >
      ~{t("gameindex.guessed")}
    </span>
  );
}
