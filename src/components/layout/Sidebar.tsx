import type { ReactNode } from "react";
import { NavLink, useLocation } from "react-router-dom";
import { useTranslation } from "react-i18next";

import { usePowerMode } from "@/lib/uxmode";
import { useRunLock } from "@/lib/runLock";
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
  // The shell's run lock (`@/lib/runLock`, round 5): while a
  // build is in flight every entry here is a way to abandon it silently, so
  // every entry stops being a link. Nothing is *hidden* — a destination that
  // is temporarily closed still has to read as a destination (§48: the mode
  // hides, the lock refuses, and neither pretends the route is gone).
  const { running } = useRunLock();

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
          <Entry key={to} to={to} icon={icon} locked={running}>
            {t(key)}
          </Entry>
        ))}
      </nav>
      <div className="sidebar-spacer" />
      <nav className="sidebar-nav">
        <Entry to="/settings" icon="gear" locked={running}>
          {t("nav.settings")}
        </Entry>
      </nav>
      {/* **Why the whole sidebar has gone dead, under the sidebar.** A column
          of disabled entries with no sentence beside them is a screen that
          has refused and not said so. The refusal names the control that
          lifts it — Stop — and, from here, the screen that carries it.

          **The lane's wording, not the strip's** (round 5, task 4). The strip
          inside the OS Builder says *"before leaving this tab"*, which is
          true where it is drawn: over the tabs, an arm's length from the Stop
          it means. This sidebar is on every screen in ART, so *this tab*
          named whichever tab the reader happened to be standing on and the
          Stop it pointed at was nowhere in sight — a refusal that is not
          actionable from where it is read. `navigationLockedLane` names both
          the screen and the control. */}
      {running && (
        <p
          className="faint"
          data-testid="sidebar-locked"
          style={{ fontSize: 11, margin: "8px 12px 0", lineHeight: 1.4 }}
        >
          {t("osBuilder.build.navigationLockedLane")}
        </p>
      )}
    </aside>
  );
}

/**
 * One sidebar entry — a link, or a dead span while a build is running.
 *
 * **A `NavLink` styled to look disabled would behave differently**: it stays
 * in the tab order, it stays an `<a href>` a keyboard or a screen reader
 * still activates, and middle-click still opens it. What the lock has to
 * remove is the navigation, so the element that navigates is the element
 * that goes — the same answer `StripChip` gives in `OsBuilder.tsx`.
 *
 * The active class is computed here rather than dropped, so the entry the
 * user is standing on still looks like the one they are standing on: a lock
 * changes what a control *does*, never where the user is.
 */
function Entry({
  to,
  icon,
  locked,
  children,
}: {
  to: string;
  icon: NavIconName;
  locked: boolean;
  children: ReactNode;
}) {
  const { pathname } = useLocation();

  if (locked) {
    // `end` semantics, matching the `NavLink` below: `/` is only itself.
    const active =
      to === "/" ? pathname === "/" : pathname === to || pathname.startsWith(`${to}/`);
    return (
      <span
        className={"sidebar-link" + (active ? " sidebar-link-active" : "")}
        aria-disabled="true"
        style={{ opacity: 0.5 }}
      >
        <span className="sidebar-icon" aria-hidden>
          <NavIcon name={icon} />
        </span>
        <span>{children}</span>
      </span>
    );
  }

  return (
    <NavLink
      to={to}
      end={to === "/"}
      className={({ isActive }) => "sidebar-link" + (isActive ? " sidebar-link-active" : "")}
    >
      <span className="sidebar-icon" aria-hidden>
        <NavIcon name={icon} />
      </span>
      <span>{children}</span>
    </NavLink>
  );
}
