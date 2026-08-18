import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import { Guessed } from "@/components/collection/Guessed";
import { diskList, kindPhrase, mediaPhrase } from "@/lib/collectionDetail";
import {
  artworkAttach,
  artworkDetach,
  artworkDir,
  artworkForTitle,
  isSupportedPicture,
  type ArtKind,
} from "@/lib/artwork";
import type { CatalogueEntry } from "@/lib/gameindex";
import { usePowerMode } from "@/lib/uxmode";

/**
 * The detail panel a title's card opens into (Collection · wave C).
 *
 * `art` is the one picture the screen already resolves per title — the
 * `ArtKind` the artwork cache prefers first for whichever titles are on
 * screen (`CollectionStudio`'s `art` map, built from `artworkKnown()`). It is
 * also this panel's fallback while its own query (`artworkForTitle`) is in
 * flight, or once it resolves to nothing.
 *
 * When a title holds more than one kind of picture — a hand-attached one
 * beside the `.rp9` snap, say — the panel offers a switch between them,
 * defaulting to the first in preference order (the one the grid already
 * shows).
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
  const [pictures, setPictures] = useState<{ kind: ArtKind; src: string }[]>([]);
  const [chosenKind, setChosenKind] = useState<ArtKind | null>(null);

  /**
   * Every picture this title has, built the same way `CollectionStudio`
   * builds the grid's thumbnails: `artworkDir()` plus each `ArtRef.file`,
   * through `convertFileSrc`. The chosen kind resets to the first of the
   * list — the preferred one, the picture the grid was already showing.
   */
  async function loadPictures() {
    try {
      const [dir, refs] = await Promise.all([artworkDir(), artworkForTitle(record.title.value)]);
      const { convertFileSrc } = await import("@tauri-apps/api/core");
      const next = refs.map((ref) => ({
        kind: ref.kind,
        src: convertFileSrc(`${dir}/${ref.file}`),
      }));
      setPictures(next);
      setChosenKind(next[0]?.kind ?? null);
    } catch {
      // A cache that cannot be read is a panel without a switch, not a panel
      // that fails to open — the `art` prop still stands on its own.
      setPictures([]);
      setChosenKind(null);
    }
  }

  useEffect(() => {
    void loadPictures();
    // The chosen kind is meant to reset on every title change, so this
    // depends only on the title, not on `loadPictures` itself.
  }, [record.title.value]);

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
      void loadPictures();
    } catch (e) {
      setArtError(String(e));
    }
  }

  async function detach() {
    setArtError(null);
    try {
      await artworkDetach(record.title.value, record.id);
      onArtChanged();
      void loadPictures();
    } catch (e) {
      setArtError(String(e));
    }
  }

  // The chosen kind's picture, falling back to the `art` prop while the
  // query is in flight or once it resolves to nothing — so nothing regresses
  // for a title the artwork cache does not know about yet.
  const chosenPicture = pictures.find((picture) => picture.kind === chosenKind)?.src ?? art;

  return (
    <section className="card" style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "start" }}>
        <h2 style={{ fontSize: 16, margin: 0 }}>{record.title.value}</h2>
        <button className="btn btn-sm" onClick={onClose}>
          {t("common.close")}
        </button>
      </div>

      {chosenPicture && (
        <img
          src={chosenPicture}
          alt=""
          style={{ display: "block", width: "100%", maxHeight: 260, objectFit: "contain" }}
        />
      )}

      {/* A control with one option is noise — only shown once this title
          holds more than one kind of picture. */}
      {pictures.length > 1 && (
        <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
          {pictures.map((picture) => (
            <button
              key={picture.kind}
              className={`btn btn-sm ${chosenKind === picture.kind ? "btn-primary" : ""}`}
              onClick={() => setChosenKind(picture.kind)}
            >
              {t(kindPhrase(picture.kind).key)}
            </button>
          ))}
        </div>
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
