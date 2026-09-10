// The Files screen's column menu: right-click the header row, or Shift+F10 /
// the Menu key on it (design § 5.1, § 5.7). A checkbox per optional column —
// Name has none, a listing with no names is not a listing — and Reset.
//
// Each item's accessible name is its column's own label, never a repeated
// "Show" (ART-240). The tick is drawn by CSS from `aria-checked`, so the name
// is the label and nothing else.

import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";

import { SIZED_COLUMN_IDS, type ColumnId, type ColumnLayout } from "@/lib/columns";

export interface ColumnMenuProps {
  /** Where to open, from the positioned header row's top-left corner, in
   *  its own pixels. */
  x: number;
  y: number;
  layout: ColumnLayout;
  onToggle: (id: ColumnId) => void;
  onReset: () => void;
  /** Escape, a click outside, or a choice made. The caller returns focus. */
  onClose: () => void;
}

export function ColumnMenu({ x, y, layout, onToggle, onReset, onClose }: ColumnMenuProps) {
  const { t } = useTranslation();
  const menuRef = useRef<HTMLDivElement>(null);

  // The menu takes the focus when it opens, so a keyboard user is in it.
  useEffect(() => {
    menuRef.current?.querySelector<HTMLElement>('[role^="menuitem"]')?.focus();
  }, []);

  // A press anywhere outside closes it — the same rule every context menu has.
  useEffect(() => {
    const onDown = (event: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) onClose();
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [onClose]);

  // Literal keys, so `literal-keys.test.ts` can see every one of them.
  const label = (id: ColumnId) =>
    id === "ext"
      ? t("files.sort.ext")
      : id === "size"
        ? t("files.sort.size")
        : id === "date"
          ? t("files.sort.date")
          : t("files.sort.attrs");

  const items = () =>
    Array.from(menuRef.current?.querySelectorAll<HTMLElement>('[role^="menuitem"]') ?? []);

  return (
    <div
      ref={menuRef}
      role="menu"
      aria-label={t("files.columns.menuTitle")}
      className="tc-column-menu"
      style={{ left: x, top: y }}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          onClose();
          return;
        }
        if (event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault();
          event.stopPropagation();
          const all = items();
          const at = all.indexOf(document.activeElement as HTMLElement);
          const step = event.key === "ArrowDown" ? 1 : -1;
          all[(at + step + all.length) % all.length]?.focus();
        }
      }}
    >
      {SIZED_COLUMN_IDS.map((id) => (
        <button
          key={id}
          type="button"
          role="menuitemcheckbox"
          aria-checked={layout.shown.includes(id)}
          className="tc-column-menu-item"
          onClick={() => onToggle(id)}
        >
          {label(id)}
        </button>
      ))}
      <div role="separator" className="tc-column-menu-sep" />
      <button type="button" role="menuitem" className="tc-column-menu-item" onClick={onReset}>
        {t("files.columns.reset")}
      </button>
    </div>
  );
}
