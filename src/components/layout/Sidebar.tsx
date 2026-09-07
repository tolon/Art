import { NavLink } from "react-router-dom";
import { useTranslation } from "react-i18next";

import { usePowerMode } from "@/lib/uxmode";
import { NavIcon, type NavIconName } from "./NavIcon";

/**
 * `powerOnly` marks the studios spec §47 reserves for Power User mode: raw hex
 * inspection and PiStorm hardware configuration. They are hidden in Beginner
 * mode rather than disabled — the mode changes what is *shown*, never what ART
 * is capable of (§48).
 */
const NAV: Array<{ to: string; key: string; icon: NavIconName; powerOnly?: boolean }> = [
  { to: "/", key: "nav.dashboard", icon: "home" },
  { to: "/files", key: "nav.files", icon: "folder" },
  { to: "/disk-tools", key: "nav.diskTools", icon: "floppy" },
  { to: "/archive-tools", key: "nav.archiveTools", icon: "archive" },
  { to: "/whdload", key: "nav.whdload", icon: "gamepad" },
  { to: "/hard-disk", key: "nav.hardDisk", icon: "hdd" },
  { to: "/gotek", key: "nav.gotek", icon: "gotek" },
  { to: "/rom", key: "nav.rom", icon: "chip" },
  { to: "/winuae", key: "nav.winuae", icon: "monitor" },
  { to: "/pistorm", key: "nav.pistorm", icon: "bolt", powerOnly: true },
  { to: "/os-builder", key: "nav.osBuilder", icon: "blocks" },
  { to: "/layout", key: "nav.layout", icon: "layout" },
  // The floppy-stack path, not a plain folder — a shelf of disks, not a drawer.
  { to: "/collection", key: "nav.collection", icon: "books" },
  { to: "/aminet", key: "nav.aminet", icon: "globe" },
  { to: "/tools", key: "nav.tools", icon: "wrench", powerOnly: true },
];

export function Sidebar() {
  const { t } = useTranslation();
  const powerMode = usePowerMode();
  const visible = NAV.filter((item) => powerMode || !item.powerOnly);

  return (
    <aside className="sidebar">
      <div className="sidebar-brand">
        <span className="sidebar-mark" aria-hidden>
          ART
        </span>
        <span className="sidebar-brand-text">
          <span className="sidebar-name">Amiga Retro Toolkit</span>
          <span className="sidebar-version">v{import.meta.env.VITE_APP_VERSION}</span>
        </span>
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
              <NavIcon name={icon} />
            </span>
            <span>{t(key)}</span>
          </NavLink>
        ))}
      </nav>
      <div className="sidebar-spacer" />
      <nav className="sidebar-nav">
        <NavLink
          to="/settings"
          className={({ isActive }) =>
            "sidebar-link" + (isActive ? " sidebar-link-active" : "")
          }
        >
          <span className="sidebar-icon" aria-hidden>
            <NavIcon name="gear" />
          </span>
          <span>{t("nav.settings")}</span>
        </NavLink>
      </nav>
    </aside>
  );
}
