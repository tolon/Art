import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Outlet } from "react-router-dom";

import { JobBar } from "@/components/JobBar";
import { Sidebar } from "@/components/layout/Sidebar";
import { TopBar } from "@/components/layout/TopBar";
import { setupDragDrop, type DropHandler } from "@/lib/dnd";
import { useRecentFilesStore } from "@/stores/recentFilesStore";
import { useSettingsStore } from "@/stores/settingsStore";
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
  const { t } = useTranslation();

  // The sidebar is collapsible (Ctrl+B), and the state is a *preference*: it
  // survives a restart, because someone who works with the panes full-width
  // expects them full-width when they come back.
  const collapsed = useSettingsStore((s) => s.settings.sidebarCollapsed);
  const updateSettings = useSettingsStore((s) => s.update);
  const toggleSidebar = useCallback(
    () => updateSettings({ sidebarCollapsed: !collapsed }),
    [collapsed, updateSettings]
  );

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!event.ctrlKey || event.altKey || event.metaKey) return;
      if (event.key.toLowerCase() !== "b") return;
      // Not while typing into something: Ctrl+B is a text shortcut in an
      // input as far as the user's fingers are concerned.
      const target = event.target as HTMLElement | null;
      const tag = target?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || target?.isContentEditable) return;
      event.preventDefault();
      void toggleSidebar();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [toggleSidebar]);

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
    <div
      className={`app-shell${dragOver ? " app-shell-dragover" : ""}${
        collapsed ? " app-shell-collapsed" : ""
      }`}
    >
      <Sidebar />
      <div className="app-main">
        <TopBar />
        <main className="app-content">
          <JobBar />
          <Outlet context={{ analyses, dragOver }} />
        </main>
      </div>
      {/* The way back when the sidebar is hidden. Placed on the shell rather
          than inside the sidebar for the obvious reason: a control that lives
          in the thing it un-hides is unreachable once it works. */}
      <button
        type="button"
        className="app-sidebar-toggle"
        aria-label={t(collapsed ? "nav.showSidebar" : "nav.hideSidebar")}
        title={`${t(collapsed ? "nav.showSidebar" : "nav.hideSidebar")} (Ctrl+B)`}
        onClick={() => void toggleSidebar()}
      >
        {collapsed ? "›" : "‹"}
      </button>
    </div>
  );
}
