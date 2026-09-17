// One partition of the card, and what fills it (round 4, task 8).
//
// The mock-up's `.prow` (U § 4.2): a 28 px dense row, the size cell right
// aligned in tabular figures, the row's actions as 32 px `.btn`s. The badge
// is the mock-up's `.tag` vocabulary, where **warn means *not verified*, not
// bad** (§48's Healthy / Warning / Problem / Unknown) — a row nobody has
// measured yet says so rather than showing a number it does not have.
//
// **The row is a drop target without being a listener.** It carries
// `data-card-row="<index>"` and nothing else; the one global listener in
// `Layout.tsx` owns the drop, and `CardSection` hit-tests the position it
// carried out (Q3). The wrapper — row *and* its open source list — carries
// the attribute, so a drop onto a listed source lands on the partition it
// belongs to.
//
// Every source also arrives two other ways: `Add…` and the row's own menu.
// That is WCAG 2.2 SC 2.5.7's single-pointer path (keyboard alone does not
// satisfy it) and the master spec § 46's contextual actions, which required
// the menu for its own reasons long before this round.

import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type { ClassifiedSource, MeasuredPartition } from "@/lib/cardOs";
import {
  partitionContentPhrase,
  partitionBytes,
  sizePhrase,
  sourcePhrase,
} from "@/lib/cardOsMeasure";
import type { CardImagePlan } from "@/lib/cardOs";
import type { CardPartitionTarget } from "@/lib/cardTarget";

export interface CardPartitionRowProps {
  /** Index into the card target's own partition list — what a drop resolves
   *  to, so the row that took it is the row under the pointer. */
  index: number;
  partition: CardPartitionTarget;
  /** System and Work: always there, never removed, never sourced by hand. */
  fixed: boolean;
  plan: CardImagePlan | null;
  measured: MeasuredPartition | undefined;
  measuring: boolean;
  classified: Record<string, ClassifiedSource>;
  /** True when the card's sizing refusal named this partition. */
  overflowing: boolean;
  powerMode: boolean;
  onAddFolder: () => void;
  onAddFiles: () => void;
  onRemoveSource: (at: number) => void;
  onRemove: () => void;
  onFloor: (bytes: number | undefined) => void;
}

/** The last segment of a host path — what a row can show without wrapping. */
function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

export function CardPartitionRow({
  index,
  partition,
  fixed,
  plan,
  measured,
  measuring,
  classified,
  overflowing,
  powerMode,
  onAddFolder,
  onAddFiles,
  onRemoveSource,
  onRemove,
  onFloor,
}: CardPartitionRowProps) {
  const { t } = useTranslation();
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const name = partition.name;

  // A press anywhere outside closes it — the rule every menu in ART has
  // (`ColumnMenu.tsx`).
  useEffect(() => {
    if (!menuOpen) return;
    const onDown = (event: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) setMenuOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [menuOpen]);

  const bytes = partitionBytes(plan, name);
  const size = measuring
    ? t("cardSection.measuring")
    : bytes === null
      ? t("cardSection.notMeasured")
      : t(sizePhrase(bytes).key, sizePhrase(bytes).params);

  // Two literal keys rather than one translate call over a ternary, so
  // `literal-keys.test.ts` can check both statically (its own header's rule).
  const fixedRole = name === "System" ? t("cardSection.system.role") : t("cardSection.work.role");
  const role = fixed
    ? fixedRole
    : t(partitionContentPhrase(measured).key, partitionContentPhrase(measured).params);

  const tag = overflowing
    ? { className: "badge badge-err", label: t("cardSection.tag.overflow") }
    : measuring || bytes === null
      ? { className: "badge badge-warn", label: t("cardSection.tag.notMeasured") }
      : { className: "badge badge-ok", label: t("cardSection.tag.measured") };

  return (
    <div {...(fixed ? {} : { "data-card-row": index })} data-testid={`card-row-${name}`}>
      <div className="card-prow">
        <span style={{ fontWeight: 600 }}>{name}</span>
        <span className="muted">{role}</span>
        <span className="card-prow-size" data-testid={`card-row-size-${name}`}>
          {size}
        </span>
        <span className={tag.className} data-testid={`card-row-tag-${name}`}>
          {tag.label}
        </span>
        <span style={{ display: "flex", gap: 4, justifyContent: "flex-end" }}>
          {!fixed && (
            <>
              <button
                type="button"
                className="btn btn-sm"
                data-testid={`card-row-add-${name}`}
                onClick={onAddFolder}
              >
                {t("cardSection.addSource")}
              </button>
              <button
                type="button"
                className="btn btn-sm btn-subtle"
                aria-haspopup="menu"
                aria-expanded={menuOpen}
                aria-label={t("cardSection.rowMenu", { name })}
                data-testid={`card-row-menu-${name}`}
                onClick={() => setMenuOpen((open) => !open)}
              >
                ⋯
              </button>
              <button
                type="button"
                className="btn btn-sm btn-subtle"
                aria-label={t("cardSection.removePartition", { name })}
                data-testid={`card-row-remove-${name}`}
                onClick={onRemove}
              >
                ×
              </button>
            </>
          )}
        </span>
      </div>

      {menuOpen && (
        <div
          ref={menuRef}
          role="menu"
          aria-label={t("cardSection.rowMenu", { name })}
          className="card-row-menu"
          data-testid={`card-row-menu-items-${name}`}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              setMenuOpen(false);
            }
          }}
        >
          <button
            type="button"
            role="menuitem"
            className="btn btn-sm btn-subtle"
            data-testid={`card-row-menu-folder-${name}`}
            onClick={() => {
              setMenuOpen(false);
              onAddFolder();
            }}
          >
            {t("cardSection.menu.addFolder")}
          </button>
          <button
            type="button"
            role="menuitem"
            className="btn btn-sm btn-subtle"
            data-testid={`card-row-menu-files-${name}`}
            onClick={() => {
              setMenuOpen(false);
              onAddFiles();
            }}
          >
            {t("cardSection.menu.addFiles")}
          </button>
          <button
            type="button"
            role="menuitem"
            className="btn btn-sm btn-subtle"
            data-testid={`card-row-menu-remove-${name}`}
            onClick={() => {
              setMenuOpen(false);
              onRemove();
            }}
          >
            {t("cardSection.menu.removePartition")}
          </button>
        </div>
      )}

      {partition.sources.length > 0 && (
        <ul className="card-source-list" data-testid={`card-row-sources-${name}`}>
          {partition.sources.map((path, at) => {
            const phrase = sourcePhrase(classified[path]);
            return (
              <li key={`${path}-${at}`} className="card-prow card-source-row">
                <span title={path}>{baseName(path)}</span>
                <span className="muted">{t(phrase.key, phrase.params)}</span>
                <span />
                <span />
                <span style={{ display: "flex", justifyContent: "flex-end" }}>
                  <button
                    type="button"
                    className="btn btn-sm btn-subtle"
                    aria-label={t("cardSection.removeSource", { name: baseName(path) })}
                    data-testid={`card-source-remove-${name}-${at}`}
                    onClick={() => onRemoveSource(at)}
                  >
                    ×
                  </button>
                </span>
              </li>
            );
          })}
        </ul>
      )}

      {/* § 47: a floor on an automatically sized partition is a power-user
          row on the *same* screen — hidden in beginner mode, never disabled,
          and never a way past the card's own total (design § 4). */}
      {powerMode && !fixed && (
        <label className="card-floor">
          <span className="faint">{t("cardSection.floor.label")}</span>
          <input
            type="number"
            min={0}
            className="input"
            data-testid={`card-row-floor-${name}`}
            value={
              partition.floorBytes === undefined
                ? ""
                : Math.round(partition.floorBytes / (1024 * 1024))
            }
            onChange={(event) => {
              const mib = Number(event.target.value);
              onFloor(
                event.target.value === "" || !Number.isFinite(mib) || mib <= 0
                  ? undefined
                  : mib * 1024 * 1024
              );
            }}
          />
          <span className="faint">{t("cardSection.floor.hint")}</span>
        </label>
      )}
    </div>
  );
}
