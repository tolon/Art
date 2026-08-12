import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Outlet } from "react-router-dom";

import { JobBar } from "@/components/JobBar";
import { Sidebar } from "@/components/layout/Sidebar";
import { TopBar } from "@/components/layout/TopBar";
import {
  belongsToFileListing,
  stepZoom,
  zoomCssValue,
  ZOOM_DEFAULT,
} from "@/lib/appZoom";
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

  // ---------------------------------------------------------------------
  // How big the application is drawn (`@/lib/appZoom`).
  //
  // One CSS variable, read by `.app-shell` — which is where the `zoom` is
  // applied and why, see `layout.css`. Everything ART draws lives inside the
  // shell, dialogs included, so one declaration covers every screen: text,
  // inputs, icons and buttons alike, without a single size written twice.
  // ---------------------------------------------------------------------
  const zoom = useSettingsStore((s) => s.settings.appZoom);

  useEffect(() => {
    const root = document.documentElement;
    root.style.setProperty("--app-zoom", zoomCssValue(zoom));
    return () => {
      root.style.removeProperty("--app-zoom");
    };
  }, [zoom]);

  const stepAppZoom = useCallback(
    (direction: number) => {
      const current = useSettingsStore.getState().settings.appZoom;
      void updateSettings({ appZoom: stepZoom(current, direction) });
    },
    [updateSettings]
  );

  useEffect(() => {
    // Non-passive, and a native listener rather than React's: `preventDefault`
    // on a wheel event is what stops WebView2 doing its own page zoom on top
    // of ours, and React attaches wheel handlers passively at the root.
    const onWheel = (event: WheelEvent) => {
      if (!event.ctrlKey) return;
      // The commander's own Ctrl+wheel means something narrower — the listing's
      // text alone — and the nearer gesture wins.
      if (belongsToFileListing(event.target as Element | null)) return;
      event.preventDefault();
      stepAppZoom(-event.deltaY);
    };

    const onKey = (event: KeyboardEvent) => {
      if (!event.ctrlKey || event.altKey || event.metaKey) return;
      // `code` rather than `key`: on a Turkish keyboard the plus and minus a
      // user presses are not the characters `key` reports.
      const grow = event.key === "+" || event.code === "Equal" || event.code === "NumpadAdd";
      const shrink =
        event.key === "-" || event.code === "Minus" || event.code === "NumpadSubtract";
      const reset = event.code === "Digit0" || event.code === "Numpad0";
      if (!grow && !shrink && !reset) return;

      event.preventDefault();
      if (reset) void updateSettings({ appZoom: ZOOM_DEFAULT });
      else stepAppZoom(grow ? 1 : -1);
    };

    window.addEventListener("wheel", onWheel, { passive: false });
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("wheel", onWheel);
      window.removeEventListener("keydown", onKey);
    };
  }, [stepAppZoom, updateSettings]);

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
