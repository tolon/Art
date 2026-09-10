import { useEffect } from "react";
import {
  Navigate,
  Route,
  RouterProvider,
  createHashRouter,
  createRoutesFromElements,
} from "react-router-dom";

import { Layout } from "@/components/layout/Layout";
import { Dashboard } from "@/pages/Dashboard";
import { SettingsPage } from "@/pages/Settings";
import { AdfBrowser } from "@/pages/AdfBrowser";
import { LhaBrowser } from "@/pages/LhaBrowser";
import { WinuaeStudio } from "@/pages/WinuaeStudio";
import { RomStudio } from "@/pages/RomStudio";
import { HardDiskStudio } from "@/pages/HardDiskStudio";
import { GotekStudio } from "@/pages/GotekStudio";
import { PistormStudio } from "@/pages/PistormStudio";
import { OsBuilder } from "@/pages/OsBuilder";
import { osBuilderRoutes } from "@/pages/osbuilder/routes";
import { ContentLayout } from "@/pages/ContentLayout";
import { HexTools } from "@/pages/HexTools";
import { CollectionStudio } from "@/pages/CollectionStudio";
import { AminetStudio } from "@/pages/AminetStudio";
import { FileManager } from "@/pages/FileManager";
import { WhdloadInstall } from "@/pages/WhdloadInstall";

import { changeLanguage, SUPPORTED_LANGUAGES, type Language } from "@/i18n";
import { initLogging } from "@/lib/log";
import { useSettingsStore } from "@/stores/settingsStore";
import { useRecentFilesStore } from "@/stores/recentFilesStore";

/**
 * The application's routes, as a **data router** (ART-292).
 *
 * `HashRouter` could not do the one thing the run lock needed from it: refuse
 * the browser's own Back and Forward while a build is in flight. React
 * Router's `useBlocker` works only inside a data router — in 7.18 it says so
 * itself, *"must be used within a data router"* — so the routes are the same
 * and only the container changed. Created once, at module load, which is what
 * `createHashRouter` expects; `App` itself renders the provider.
 */
const router = createHashRouter(
  createRoutesFromElements(
    <Route element={<Layout />}>
      <Route index element={<Dashboard />} />
      <Route path="settings" element={<SettingsPage />} />
      {/* Phase 1 — ADF + LHA browsers */}
      <Route path="disk-tools" element={<AdfBrowser />} />
      <Route path="archive-tools" element={<LhaBrowser />} />
      {/* Phase 2 — WinUAE + Kickstart ROM Studios */}
      <Route path="winuae" element={<WinuaeStudio />} />
      <Route path="rom" element={<RomStudio />} />
      {/* Phase 3/4 — Hard Disk Studio */}
      <Route path="hard-disk" element={<HardDiskStudio />} />
      {/* Phase 5 — Gotek Studio */}
      <Route path="gotek" element={<GotekStudio />} />
      {/* Phase 6 & 7 — PiStorm & Forensic Hex Tools */}
      <Route path="pistorm" element={<PistormStudio />} />
      {/* The OS Builder is a sequence of steps, each its own sub-route, so
          back/forward and a jump to a step work at the router level. The
          parent still renders, which is what keeps `route::OS_BUILDER` a
          real route for `builtin.rs` to point a workflow at. */}
      <Route path="os-builder" element={<OsBuilder />}>
        {osBuilderRoutes()}
      </Route>
      <Route path="layout" element={<ContentLayout />} />
      <Route path="tools" element={<HexTools />} />
      {/* Phase 8 — Collection Studio */}
      <Route path="collection" element={<CollectionStudio />} />
      <Route path="aminet" element={<AminetStudio />} />
      <Route path="files" element={<FileManager />} />
      <Route path="whdload" element={<WhdloadInstall />} />
      <Route path="*" element={<Navigate to="/" replace />} />
    </Route>
  )
);

export default function App() {
  const settings = useSettingsStore((s) => s.settings);
  const loadSettings = useSettingsStore((s) => s.load);
  const loadRecent = useRecentFilesStore((s) => s.load);

  useEffect(() => {
    // Non-blocking async initializations
    const safe = (p: Promise<unknown>, label: string) =>
      p.catch((e) => console.warn(`[ART] ${label} init skipped:`, e));

    void safe(initLogging(), "logging");
    void safe(loadSettings(), "settings");
    void safe(loadRecent(), "recent");
    const stored = settings.language;
    const lang: Language = (SUPPORTED_LANGUAGES as readonly string[]).includes(
      stored ?? "",
    )
      ? (stored as Language)
      : "en";
    void safe(changeLanguage(lang), "language");
  }, [loadSettings, loadRecent, settings.language]);

  // Apply the theme class to <html> whenever it changes.
  useEffect(() => {
    const cls = settings.theme === "light" ? "theme-light" : "theme-dark";
    document.documentElement.classList.remove("theme-light", "theme-dark");
    document.documentElement.classList.add(cls);
  }, [settings.theme]);

  return (
    <RouterProvider router={router} />
  );
}
