/**
 * The detail panel's pure parts (Collection · wave C).
 *
 * No i18next singleton here, so nothing renders a string: each helper returns
 * a {@link Phrase} and `TitleDetail.tsx` calls `t()` on it.
 */

import type { Media } from "./gameindex";
import type { Phrase } from "./phrase";

/** How this title's media reads in one line. */
export function mediaPhrase(media: Media): Phrase {
  switch (media.kind) {
    case "floppies":
      return {
        key: "collection.detail.media.floppies",
        params: { count: media.ordered.length },
      };
    case "hardfile":
      return { key: "collection.detail.media.hardfile", params: { file: media.file } };
    default:
      return { key: "collection.detail.media.whdload", params: { slave: media.slave } };
  }
}

/**
 * The disks, in the order the catalogue holds them.
 *
 * That order is not decoration: `.rp9` states it through `<floppy priority>`
 * and a game asks for disk two by name.
 */
export function diskList(media: Media): string[] {
  return media.kind === "floppies" ? media.ordered : [];
}

/** Whether Play can do anything with this medium at all. */
export function canLaunch(media: Media): boolean {
  return (
    media.kind === "floppies" || media.kind === "hardfile" || media.kind === "whdload-drawer"
  );
}
