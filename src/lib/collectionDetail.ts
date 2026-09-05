/**
 * The detail panel's pure parts (Collection · wave C).
 *
 * No i18next singleton here, so nothing renders a string: each helper returns
 * a {@link Phrase} and `TitleDetail.tsx` calls `t()` on it.
 */

import type { ArtKind } from "./artwork";
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
    case "whdload-hardfile":
      return { key: "collection.detail.media.whdload", params: { slave: media.slave } };
    case "whdload-drawer":
      return {
        key: "collection.detail.media.whdloadDrawer",
        params: { slave: media.slave },
      };
    case "whdload-archive":
      return {
        key: "collection.detail.media.whdloadArchive",
        params: { slave: media.slave, file: media.file },
      };
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

/**
 * Whether Play can do anything with this medium at all.
 *
 * `whdload-archive` is the one `Media` shape that is real and still cannot be
 * launched: `RequestKind::Whdload` needs a directory on a filesystem, and an
 * archive entry is a path inside a compressed file ART has not unpacked
 * (ART-147, for a different `Media` shape — a `false` here is what stops it
 * happening a second time). An exhaustive switch rather than a boolean chain
 * so the next `Media` variant must be given an explicit answer, not fall
 * through to whichever side of `||` came last.
 */
export function canLaunch(media: Media): boolean {
  switch (media.kind) {
    case "floppies":
    case "hardfile":
    case "whdload-hardfile":
    case "whdload-drawer":
      return true;
    case "whdload-archive":
      return false;
  }
}

/** What to call one kind of picture on a button the user has to read. */
export function kindPhrase(kind: ArtKind): Phrase {
  return { key: `artwork.kind.${kind}` };
}
