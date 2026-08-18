import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import { Guessed } from "@/components/collection/Guessed";
import { diskList, mediaPhrase } from "@/lib/collectionDetail";
import { artworkAttach, artworkDetach, isSupportedPicture } from "@/lib/artwork";
import type { CatalogueEntry } from "@/lib/gameindex";
import { usePowerMode } from "@/lib/uxmode";

/**
 * The detail panel a title's card opens into (Collection · wave C).
 *
 * `art` is the one picture the screen already resolves per title — the
 * `ArtKind` the artwork cache prefers first for whichever titles are on
 * screen (`CollectionStudio`'s `art` map, built from `artworkKnown()`).
 *
 * One picture, not a switch between kinds — deliberately, for now. A title
 * holds more than one kind only once a hand-attached picture joins the
 * `.rp9` snap (wave C, the attach task), and `artworkKnown` answers with the
 * preferred one rather than the set. The switch belongs with the second
 * kind, not ahead of it.
 */
export function TitleDetail({
  entry,
  art,
  hasManualArt,
  onArtChanged,
  onClose,
}: {
  entry: CatalogueEntry;
  art: string | undefined;
  /** Whether this title's cached picture is one the user attached by hand. */
  hasManualArt: boolean;
  /** Re-read the artwork cache — the same re-read the screen already does
   *  after an artwork job finishes. */
  onArtChanged: () => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const power = usePowerMode();
  const record = entry.record;
  const disks = diskList(record.media);
  const media = mediaPhrase(record.media);
  const [artError, setArtError] = useState<string | null>(null);

  async function attach() {
    setArtError(null);
    const chosen = await open({
      multiple: false,
      filters: [{ name: t("collection.detail.art.filter"), extensions: ["png", "jpg", "jpeg"] }],
      title: t("collection.detail.art.dialog"),
    });
    if (typeof chosen !== "string") return;
    // The dialog's own filter already narrows to PNG/JPEG, but a translated
    // refusal here — rather than surfacing Rust's English-only one (ART-060)
    // — needs its own check, kept identical to the Rust gate on purpose.
    if (!isSupportedPicture(chosen)) {
      setArtError(t("collection.detail.art.rejected"));
      return;
    }
    try {
      await artworkAttach(record.title.value, record.id, chosen);
      onArtChanged();
    } catch (e) {
      setArtError(String(e));
    }
  }

  async function detach() {
    setArtError(null);
    try {
      await artworkDetach(record.title.value, record.id);
      onArtChanged();
    } catch (e) {
      setArtError(String(e));
    }
  }

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

      <div style={{ display: "flex", gap: 6 }}>
        <button className="btn btn-sm" onClick={() => void attach()}>
          {t("collection.detail.art.attach")}
        </button>
        {hasManualArt && (
          <button className="btn btn-sm" onClick={() => void detach()}>
            {t("collection.detail.art.remove")}
          </button>
        )}
      </div>
      {artError && (
        <div className="badge badge-err" style={{ fontSize: 11 }}>
          {artError}
        </div>
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
