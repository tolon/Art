import { useEffect, useState } from "react";
import { Outlet } from "react-router-dom";

import { JobBar } from "@/components/JobBar";
import { Sidebar } from "@/components/layout/Sidebar";
import { TopBar } from "@/components/layout/TopBar";
import { setupDragDrop, type DropHandler } from "@/lib/dnd";
import { useRecentFilesStore } from "@/stores/recentFilesStore";
import type { DroppedAnalysis } from "@/types";

/**
 * App shell: sidebar + topbar + routed content.
 *
 * Also owns the single global drag & drop listener. Concrete modules receive
 * dropped analyses via the `useDroppedAnalyses` hook below (extensible to
 * per-target routing in later phases).
 */
export function Layout() {
  const [dragOver, setDragOver] = useState(false);
  const [analyses, setAnalyses] = useState<DroppedAnalysis[]>([]);
  const record = useRecentFilesStore((s) => s.record);
  const reloadRecent = useRecentFilesStore((s) => s.load);

  useEffect(() => {
    const handler: DropHandler = {
      onPhase: (phase) => setDragOver(phase === "enter" || phase === "over"),
      onDrop: (results) => {
        setDragOver(false);
        setAnalyses(results);
        // Record successful analyses into recent files.
        for (const r of results) {
          if (r.ok && r.plan) {
            const name = r.path.split(/[\\/]/).pop() ?? r.path;
            const kind = r.plan.detection.format_hint;
            void record(r.path, name, kind).then(() => reloadRecent());
          }
        }
      },
    };
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void setupDragDrop(handler).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [record, reloadRecent]);

  return (
    <div className={`app-shell ${dragOver ? "app-shell-dragover" : ""}`}>
      <Sidebar />
      <div className="app-main">
        <TopBar />
        <main className="app-content">
          <JobBar />
          <Outlet context={{ analyses, dragOver }} />
        </main>
      </div>
    </div>
  );
}
