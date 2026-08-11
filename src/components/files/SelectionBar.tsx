// A one-line summary of the focused pane's multi-select — how many entries,
// and their total size — shown between the panes and the function-key bar.
// Nothing when the selection is empty: the strip beneath it is already the
// tightest in the app, and an empty bar would just be blank space with a
// border around it.

import { useTranslation } from "react-i18next";

import { formatBytes } from "@/lib/panel";

export function SelectionBar({ count, bytes }: { count: number; bytes: number }) {
  const { t } = useTranslation();
  if (count === 0) return null;
  return (
    <div className="selection-bar">
      {t("files.selection.summary", { count, size: formatBytes(bytes) })}
    </div>
  );
}
