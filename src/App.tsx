import { useEffect } from "react";
import { HashRouter, Route, Routes, Navigate } from "react-router-dom";

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
import { HexTools } from "@/pages/HexTools";
import { CollectionStudio } from "@/pages/CollectionStudio";
import { AminetStudio } from "@/pages/AminetStudio";
import { FileManager } from "@/pages/FileManager";
import { WhdloadInstall } from "@/pages/WhdloadInstall";

import { changeLanguage, SUPPORTED_LANGUAGES, type Language } from "@/i18n";
import { initLogging } from "@/lib/log";
import { useSettingsStore } from "@/stores/settingsStore";
import { useRecentFilesStore } from "@/stores/recentFilesStore";

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
    <HashRouter>
      <Routes>
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
          <Route path="tools" element={<HexTools />} />
          {/* Phase 8 — Collection Studio */}
          <Route path="collection" element={<CollectionStudio />} />
          <Route path="aminet" element={<AminetStudio />} />
          <Route path="files" element={<FileManager />} />
          <Route path="whdload" element={<WhdloadInstall />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Route>
      </Routes>
    </HashRouter>
  );
}
