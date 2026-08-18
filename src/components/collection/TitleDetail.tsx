import { useTranslation } from "react-i18next";

import { Guessed } from "@/components/collection/Guessed";
import { diskList, mediaPhrase } from "@/lib/collectionDetail";
import type { CatalogueEntry } from "@/lib/gameindex";
import { usePowerMode } from "@/lib/uxmode";

/**
 * The detail panel a title's card opens into (Collection · wave C).
 *
 * `art` is the one picture the screen already resolves per title — the
 * `ArtKind` the artwork cache prefers first for whichever titles are on
 * screen (`CollectionStudio`'s `art` map, built from `artworkKnown()`).
 * The wave-C design also describes a row of small buttons for switching
 * between *every* kind the cache holds for a title, shown only when there is
 * more than one. That needs a per-kind cache query — `Cache::get` already
 * exists in `core/artwork/cache.rs`, but nothing exposes it as a command the
 * way `artwork_known` exposes `Cache::best` — and adding one is a
 * `commands/artwork.rs` change outside this task's files (reserved for a
 * later task per the wave-C plan's cross-task file table). So today a title
 * never carries more than one kind through to this panel, and the "single
 * kind, no row" rule in the design already covers that case correctly —
 * there is nothing to switch between yet.
 */
export function TitleDetail({
  entry,
  art,
  onClose,
}: {
  entry: CatalogueEntry;
  art: string | undefined;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const power = usePowerMode();
  const record = entry.record;
  const disks = diskList(record.media);
  const media = mediaPhrase(record.media);

  return (
    <section className="card" style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "start" }}>
        <h2 style={{ fontSize: 16, margin: 0 }}>{record.title.value}</h2>
        <button className="btn btn-sm" onClick={onClose}>
          {t("common.close")}
        </button>
      </div>

      {art && (
        <img
          src={art}
          alt=""
          style={{ display: "block", width: "100%", maxHeight: 260, objectFit: "contain" }}
        />
      )}

      <div className="muted" style={{ fontSize: 13 }}>
        {t(media.key, media.params)}
      </div>

      {/* The facts, each keeping the `Guessed` mark the card already uses —
          a value ART inferred must not read as one it was told. */}
      <dl style={{ display: "grid", gridTemplateColumns: "auto 1fr", gap: "4px 10px", margin: 0, fontSize: 13 }}>
        <dt className="muted">{t("collection.detail.publisher")}</dt>
        <dd style={{ margin: 0 }}>
          {record.publisher?.value ?? t("common.unknown")}
          {record.publisher && <Guessed from={record.publisher.from} />}
        </dd>
        <dt className="muted">{t("collection.detail.year")}</dt>
        <dd style={{ margin: 0 }}>
          {record.year?.value ?? t("common.unknown")}
          {record.year && <Guessed from={record.year.from} />}
        </dd>
        <dt className="muted">{t("collection.detail.genre")}</dt>
        <dd style={{ margin: 0 }}>
          {record.genre?.value ?? t("common.unknown")}
          {record.genre && <Guessed from={record.genre.from} />}
        </dd>
        <dt className="muted">{t("collection.detail.rating")}</dt>
        <dd style={{ margin: 0 }}>{record.rating?.value ?? t("common.unknown")}</dd>
      </dl>

      {/* `KickstartNeed.image` is nullable — a slave can declare a size and a
          CRC and no name at all — so the guard is on the image, not on the
          need. Rendering `null` into the sentence is the bug this avoids. */}
      {record.kickstart?.value.image && (
        <div className="faint" style={{ fontSize: 12 }}>
          {t("gameindex.kickstartNeeded", { image: record.kickstart.value.image })}
        </div>
      )}

      {disks.length > 0 && (
        <ol style={{ fontSize: 12, margin: 0, paddingLeft: 20 }}>
          {disks.map((disk) => (
            <li key={disk}>{disk}</li>
          ))}
        </ol>
      )}

      {/* Beginner mode hides the raw path — and hides only. No action below
          is disabled by the mode (§47, §48). */}
      {power && (
        <div className="faint" style={{ fontSize: 11, wordBreak: "break-all" }}>
          {entry.path}
        </div>
      )}
    </section>
  );
}
