import { NavLink } from "react-router-dom";
import { useTranslation } from "react-i18next";

import { usePowerMode } from "@/lib/uxmode";

/**
 * `powerOnly` marks the studios spec §47 reserves for Power User mode: raw hex
 * inspection and PiStorm hardware configuration. They are hidden in Beginner
 * mode rather than disabled — the mode changes what is *shown*, never what ART
 * is capable of (§48).
 */
const NAV: Array<{ to: string; key: string; icon: string; powerOnly?: boolean }> = [
  { to: "/", key: "nav.dashboard", icon: "🏠" },
  { to: "/files", key: "nav.files", icon: "🗂" },
  { to: "/disk-tools", key: "nav.diskTools", icon: "📀" },
  { to: "/archive-tools", key: "nav.archiveTools", icon: "📦" },
  { to: "/whdload", key: "nav.whdload", icon: "🕹" },
  { to: "/hard-disk", key: "nav.hardDisk", icon: "💾" },
  { to: "/gotek", key: "nav.gotek", icon: "🎮" },
  { to: "/rom", key: "nav.rom", icon: "🧠" },
  { to: "/winuae", key: "nav.winuae", icon: "🖥" },
  { to: "/pistorm", key: "nav.pistorm", icon: "⚡", powerOnly: true },
  { to: "/os-builder", key: "nav.osBuilder", icon: "🧱" },
  { to: "/layout", key: "nav.layout", icon: "🗃" },
  { to: "/collection", key: "nav.collection", icon: "📚" },
  { to: "/aminet", key: "nav.aminet", icon: "🌐" },
  { to: "/tools", key: "nav.tools", icon: "🔧", powerOnly: true },
  { to: "/settings", key: "nav.settings", icon: "⚙" },
];

export function Sidebar() {
  const { t } = useTranslation();
  const powerMode = usePowerMode();
  const visible = NAV.filter((item) => powerMode || !item.powerOnly);

  return (
    <aside className="sidebar">
      <div className="sidebar-brand">
        <span className="sidebar-logo">ART</span>
        <span className="sidebar-version">v{import.meta.env.VITE_APP_VERSION}</span>
      </div>
      <nav className="sidebar-nav">
        {visible.map(({ to, key, icon }) => (
          <NavLink
            key={to}
            to={to}
            end={to === "/"}
            className={({ isActive }) =>
              "sidebar-link" + (isActive ? " sidebar-link-active" : "")
            }
          >
            <span className="sidebar-icon" aria-hidden>
              {icon}
            </span>
            <span>{t(key)}</span>
          </NavLink>
        ))}
      </nav>
    </aside>
  );
}
