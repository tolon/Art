// The per-pane status line the Total Commander-styled Files screen shows
// under each pane's list (task 6b): selected bytes / total bytes, then
// selected/total files, then selected/total directories — e.g.
// "0 k / 52.977.436 k in 0 / 1307 file(s), 0 / 23 dir(s)".
//
// Pure, like `src/lib/selection.ts` this reads: entries and a selection Set
// in, counts out. `[..]`, the synthetic "go up" row `FileManager.tsx` may
// render first, is never part of `entries` here — it lives only in the JSX,
// so it never has to be filtered back out of a count.

import type { PanelEntry } from "@/lib/panel";

export interface PaneStatusCounts {
  selectedBytes: number;
  totalBytes: number;
  selectedFiles: number;
  totalFiles: number;
  selectedDirs: number;
  totalDirs: number;
}

/** `entries`' status line counts, given which of their names are selected. */
export function paneStatusCounts(entries: PanelEntry[], selected: Set<string>): PaneStatusCounts {
  const counts: PaneStatusCounts = {
    selectedBytes: 0,
    totalBytes: 0,
    selectedFiles: 0,
    totalFiles: 0,
    selectedDirs: 0,
    totalDirs: 0,
  };

  for (const entry of entries) {
    const isSelected = selected.has(entry.name);
    if (entry.is_dir) {
      counts.totalDirs++;
      if (isSelected) counts.selectedDirs++;
    } else {
      counts.totalFiles++;
      counts.totalBytes += entry.bytes;
      if (isSelected) {
        counts.selectedFiles++;
        counts.selectedBytes += entry.bytes;
      }
    }
  }

  return counts;
}
