// The folder a host path sits in.
//
// Three studios already cut a path at its last separator by hand
// (`fsDriver.ts`, `pistormRom.ts`, `isoPane.ts`); this is the same cut with
// the two cases those inline versions do not answer — a file at a drive root,
// where dropping the separator turns an absolute path into a relative one,
// and a bare name, which has no parent to give.
//
// Host paths only. An AmigaDOS path is `VOL:dir/file` and does not split this
// way; `isoPane.ts` keeps its own POSIX-only cut for paths inside a disc.

/**
 * The folder containing `path`, or `null` when the path names no folder.
 *
 * The separator is kept on a root (`"E:\\"`, `"/"`) and dropped everywhere
 * else, which is what both Windows and POSIX mean by the containing folder.
 */
export function hostParentDir(path: string): string | null {
  const cut = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  if (cut < 0) return null;
  const parent = path.slice(0, cut);
  // A leading `/` or a `C:` prefix alone is a root, not a folder name.
  if (parent === "" || /^[A-Za-z]:$/.test(parent)) return path.slice(0, cut + 1);
  return parent;
}
