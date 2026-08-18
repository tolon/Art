/**
 * Artwork sources and the enrichment job (Collection · wave B).
 *
 * Typed wrappers only. Components never call `invoke` directly, and nothing in
 * `src/lib` renders a string — a helper that builds a message returns a
 * {@link Phrase} and the component calls `t()` on it.
 *
 * Nothing here starts on its own. Opening the Collection touches no network;
 * {@link artworkEnrich} runs when the user asks it to.
 */

import { invoke } from "@tauri-apps/api/core";

import type { Phrase } from "./phrase";

/** The kinds of picture a source can supply. Mirrors `core::artwork::ArtKind`. */
export type ArtKind = "boxart" | "snap" | "title" | "logo" | "icon";

/**
 * Every kind, in the order a screen should prefer them.
 *
 * The same order as `ArtKind::ALL` in Rust. A row shows the first kind it has.
 */
export const ART_KINDS: readonly ArtKind[] = [
  "boxart",
  "title",
  "snap",
  "logo",
  "icon",
] as const;

/** What a title gained. `file` is relative to {@link artworkDir}. */
export interface ArtRef {
  kind: ArtKind;
  source: string;
  file: string;
}

/**
 * One configured source. Mirrors `core::artwork::config::ConfiguredSource`.
 *
 * `imageBase` empty means "the same place as the index" — whdload.de serves
 * both from one host, libretro does not.
 */
export interface ConfiguredSource {
  id: string;
  enabled: boolean;
  mirrorBase: string;
  imageBase: string;
}

/** How one source did on a run. */
export interface SourceOutcome {
  id: string;
  written: number;
  /** Found already on disk from a run that never got to save its index. */
  adopted: number;
  matched: number;
  missed: number;
  reachable: boolean;
  note: string | null;
}

export interface EnrichOutcome {
  perSource: SourceOutcome[];
  cachedBefore: number;
}

/**
 * The sources ART ships with, enabled, with their default mirrors.
 *
 * Asked for rather than carried here: two lists of mirror URLs would drift.
 */
export async function artworkDefaults(): Promise<ConfiguredSource[]> {
  return invoke<ConfiguredSource[]>("artwork_defaults");
}

/**
 * Check an edited source before saving it.
 *
 * Rejects at the door rather than at fetch time, so a base that cannot become a
 * mirror never reaches the settings file.
 */
export async function artworkCheckSource(
  source: ConfiguredSource
): Promise<void> {
  return invoke<void>("artwork_check_source", { source });
}

/** Where cached pictures live on disk. */
export async function artworkDir(): Promise<string> {
  return invoke<string>("artwork_dir");
}

/**
 * Which of these titles already have a picture, one slot each, in order.
 *
 * Asked of Rust rather than worked out here: the two matching rules live in
 * `core/artwork/key.rs`, and a second implementation in TypeScript would drift
 * from them the first time one changed.
 */
export async function artworkKnown(
  titles: string[]
): Promise<(ArtRef | null)[]> {
  return invoke<(ArtRef | null)[]>("artwork_known", { titles });
}

/**
 * Fetch artwork for these titles, on a job.
 *
 * Returns a job id straight away. 1700 titles against two sources at four
 * requests per second is §54/§55 territory — the user gets a Stop, and the
 * result arrives in an `artwork-result` event.
 */
export async function artworkEnrich(
  titles: string[],
  sources: ConfiguredSource[]
): Promise<number> {
  return invoke<number>("artwork_enrich", { titles, sources });
}

/** Payload of the `artwork-result` event. */
export interface ArtworkResult extends EnrichOutcome {
  job_id: number;
}

export const ARTWORK_RESULT_EVENT = "artwork-result";

/** Subscribe to finished enrichment runs. Returns an unlisten function. */
export async function onArtworkResult(
  handler: (result: ArtworkResult) => void
): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  return listen<ArtworkResult>(ARTWORK_RESULT_EVENT, (e) => handler(e.payload));
}

/** One picture to look for inside a package the user already has. */
export interface LocalPreviewArg {
  title: string;
  /** The `.rp9` on disk. */
  package: string;
  /** The entry's name inside it — `GameRecord.preview`, verbatim. */
  entry: string;
}

/** What the offline pass managed. Mirrors `core::artwork::local::LocalOutcome`. */
export interface LocalOutcome {
  written: number;
  adopted: number;
  missed: number;
}

/**
 * Take the pictures the user's own `.rp9` packages carry.
 *
 * Touches no network and asks no source, which is why it needs no
 * configuration and no consent: nothing leaves the machine.
 */
export async function artworkAdoptLocal(previews: LocalPreviewArg[]): Promise<number> {
  return invoke<number>("artwork_adopt_local", { previews });
}

/** Payload of the `artwork-local-result` event. */
export interface LocalResult {
  job_id: number;
  outcome: LocalOutcome;
}

export const ARTWORK_LOCAL_RESULT_EVENT = "artwork-local-result";

/** Subscribe to finished offline passes. Returns an unlisten function. */
export async function onArtworkLocalResult(
  handler: (result: LocalResult) => void
): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  return listen<LocalResult>(ARTWORK_LOCAL_RESULT_EVENT, (e) => handler(e.payload));
}

/**
 * The name to show for a source ART ships.
 *
 * A `Phrase` rather than a string: this file has no i18next singleton, and the
 * component that renders it calls `t()`.
 */
export function sourcePhrase(id: string): Phrase {
  switch (id) {
    case "libretro":
      return { key: "artwork.source.libretro" };
    case "whdload-de":
      return { key: "artwork.source.whdloadDe" };
    default:
      return { key: "artwork.source.unknown", params: { id } };
  }
}

/**
 * What one source managed, as a sentence the panel can show.
 *
 * An unreachable source says so; a reachable one reports what it wrote. The
 * note is deliberately not localised — it carries a `CoreError`'s own words,
 * which stay English whatever the chosen language (ART-060).
 */
export function outcomePhrase(outcome: SourceOutcome): Phrase {
  if (!outcome.reachable) {
    return {
      key: "artwork.outcome.unreachable",
      params: { note: outcome.note ?? "" },
    };
  }
  // Adopted pictures get their own sentence rather than being folded into
  // "fetched": they were already on the disk, and saying ART downloaded them
  // would be a small lie about a number the user can check.
  if (outcome.adopted > 0) {
    return {
      key: "artwork.outcome.doneWithAdopted",
      params: {
        written: outcome.written,
        adopted: outcome.adopted,
        missed: outcome.missed,
      },
    };
  }
  return {
    key: "artwork.outcome.done",
    params: { written: outcome.written, missed: outcome.missed },
  };
}

/**
 * Turn a cache-relative name into something an `<img src>` can load.
 *
 * The asset protocol is scoped to the artwork directory alone
 * (`tauri.conf.json`), so this cannot be pointed at anything else.
 */
export async function artworkSrc(dir: string, file: string): Promise<string> {
  const { convertFileSrc } = await import("@tauri-apps/api/core");
  return convertFileSrc(`${dir}/${file}`);
}
