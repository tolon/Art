// The folder column of the source step (simplification design § 4).
//
// **What this is.** The one material folder list (intake design § 3.1) — a
// row per folder with its layer tag and Remove, Add, the folders a layered
// plan will not read, the Amiga Forever offer, and the guide buttons — as one
// component, so the step can lay it out *beside* the readout with the
// readout first. It was inline in `OsInstall.tsx` until 2026-09-09, two
// hundred lines above the answer it is the question for.
//
// **What this owns.** Nothing about the build. Every value is the step's
// (the session's list, the scans, the layers) and every change goes back
// through a callback; the only state here is what the guide button last
// answered, per folder, which is a fact about this screen and not about the
// build. That is what keeps `remembered.ts`'s rule true from inside: this
// component cannot write a setting because it holds none.
//
// **The guide block moved here from `MaterialReadout.tsx`** because it is an
// action on a folder, and this is where the folders are. Its three endings —
// written, already there, could not — are the same three, kept apart.

import { useState } from "react";
import { useTranslation } from "react-i18next";

import type { MaterialFolder } from "@/lib/buildSession";
import { errorText } from "@/lib/errorText";
import {
  osinstallWriteMaterialGuide,
  type GuideOutcome,
  type InstallLayer,
  type InstallRelease,
  type MediaScanResult,
} from "@/lib/osinstall";

export interface MaterialFoldersProps {
  release: InstallRelease;
  /** The build's material list, in list order. */
  folders: MaterialFolder[];
  /** The layers this release's recipe declares; `[]` means unlayered and no
   *  select is drawn. */
  layers: InstallLayer[];
  layerLabel: (layer: InstallLayer) => string;
  /** One scan per folder path. `null` or absent is "not scanned" or "could
   *  not be read"; the row says which from the scan's own `outcome`. */
  folderScans: Record<string, MediaScanResult | null>;
  /** The wrong-layer sentence for a row, or `null`. Computed by the step,
   *  which is the one that has the identification pass. */
  wrongLayerHint: (entry: MaterialFolder) => string | null;
  /** Folders a layered plan will not read, named rather than dropped. */
  unusedForPlan: string[];
  /** Amiga Forever's disks folder while the offer stands, else `null`. */
  amigaForeverOffer: string | null;
  onAdd: () => void;
  onRemove: (path: string) => void;
  onTag: (path: string, layer: string) => void;
  onAmigaForeverAdd: () => void;
  onAmigaForeverDismiss: () => void;
}

export function MaterialFolders({
  release,
  folders,
  layers,
  layerLabel,
  folderScans,
  wrongLayerHint,
  unusedForPlan,
  amigaForeverOffer,
  onAdd,
  onRemove,
  onTag,
  onAmigaForeverAdd,
  onAmigaForeverDismiss,
}: MaterialFoldersProps) {
  const { t, i18n } = useTranslation();

  /**
   * What the guide button last answered, per folder.
   *
   * Per folder rather than one value for the column: two folders are two
   * files, and a *written* line under the folder that is still empty would
   * be the screen claiming something about a folder ART did not touch. A
   * missing entry is the ordinary state and renders nothing — never "not
   * written yet", which is a sentence about a thing nobody asked for.
   */
  const [guides, setGuides] = useState<
    Record<string, GuideOutcome | { state: "failed"; detail: string }>
  >({});

  /**
   * Write the guide into one folder. **Only from this click** (intake design
   * § 3.7): ART does not write into somebody's folder because they pointed at
   * it.
   *
   * Nothing is overwritten — the Rust side opens the file with `create_new`,
   * so a guide already there keeps every byte — and *there already* is its
   * own answer rather than an error, because "delete it and ask again" is a
   * different next step from "ART could not write it".
   */
  async function writeGuide(folder: string) {
    try {
      const outcome = await osinstallWriteMaterialGuide(folder, release, i18n.language.split("-")[0]);
      setGuides((held) => ({ ...held, [folder]: outcome }));
    } catch (e) {
      setGuides((held) => ({ ...held, [folder]: { state: "failed", detail: errorText(t, e) } }));
    }
  }

  return (
    <>
      {/*
        **One list, one row per folder** (design § 3.1). This replaced three
        controls: the install-disks field, the bag of added folders under
        it, and one labelled field per media layer. They were three shapes
        for one question, and a person who simply has the files had to work
        out which sentence each field wanted.

        A row is the folder's own path, the layer it holds (a select, and
        only for a release whose recipe declares layers), and Remove. The
        layer select is what carries the labelled question the per-layer
        fields used to ask — it is the same information, attached to the
        folder rather than to a field.
      */}
      <div data-testid="material-folders" style={{ margin: "0 0 8px" }}>
        <p className="muted" style={{ fontSize: 12, margin: "0 0 6px" }}>
          {t("osinstall.material.label")}
        </p>
        {folders.length === 0 && (
          <p className="faint" style={{ fontSize: 11, margin: "0 0 8px" }}>
            {t("osinstall.media.none")}
          </p>
        )}
        {folders.map((entry, index) => {
          const scan = folderScans[entry.path];
          const hint = wrongLayerHint(entry);
          // **The row's index, not its layer tag** (fix round 1, F5). Two
          // rows may carry one tag — `unusedForPlan` exists because that
          // state is reachable — and keying the id by the tag gave two
          // DOM elements one `id`, which is an accessibility fault in its
          // own right as well as a `data-testid` that matched either.
          const hintId = `layer-wrong-hint-${index}`;
          const unreadableId = `material-folder-unreadable-${index}`;
          // ART-241: the controls on this row are described by whichever
          // of the row's own paragraphs actually renders, so a screen
          // reader user hears the warning with the control rather than
          // having to hunt forward in the page for it. The fix originally
          // landed on the `Field`s these rows replaced, and putting it back
          // is not optional: the entry stays Fixed only while it is true.
          const describedBy =
            [scan?.outcome === "folder-unreadable" ? unreadableId : null, hint ? hintId : null]
              .filter(Boolean)
              .join(" ") || undefined;
          return (
            <div key={entry.path} data-testid="material-folder" style={{ margin: "0 0 6px" }}>
              <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
                <span
                  style={{ fontSize: 12, wordBreak: "break-all", flex: 1, minWidth: "12em" }}
                >
                  {entry.path}
                </span>
                {layers.length > 0 && (
                  <select
                    className="btn"
                    style={{ fontSize: 11 }}
                    value={entry.layer ?? ""}
                    // Named by the folder, not by the word "layer": with
                    // several rows every select would otherwise carry the
                    // identical accessible name, which is ART-240's own
                    // defect one control along.
                    aria-label={t("osinstall.material.layerAriaLabel", { folder: entry.path })}
                    aria-describedby={describedBy}
                    onChange={(e) => onTag(entry.path, e.target.value)}
                  >
                    <option value="">{t("osinstall.material.layerAny")}</option>
                    {layers.map((option) => (
                      <option key={option.id} value={option.id}>
                        {layerLabel(option)}
                      </option>
                    ))}
                  </select>
                )}
                <button
                  className="btn"
                  style={{ fontSize: 11 }}
                  onClick={() => onRemove(entry.path)}
                  // ART-240: every Remove button on this screen used to
                  // carry the identical visible text, so a screen reader
                  // user tabbing through heard the same word once per row
                  // with nothing to tell them apart.
                  aria-label={t("osinstall.media.removeFolderAriaLabel", { folder: entry.path })}
                  aria-describedby={describedBy}
                >
                  {t("osinstall.media.removeFolder")}
                </button>
              </div>
              {scan?.outcome === "folder-unreadable" && (
                <p
                  id={unreadableId}
                  className="badge badge-err"
                  data-testid={unreadableId}
                  style={{ fontSize: 11, margin: "2px 0 0", display: "inline-block" }}
                >
                  {t("osinstall.media.unreadable")}
                </p>
              )}
              {hint && (
                <p
                  id={hintId}
                  className="badge badge-err"
                  style={{ fontSize: 11, margin: "2px 0 0", display: "inline-block" }}
                  data-testid={hintId}
                >
                  {hint}
                </p>
              )}
            </div>
          );
        })}
        <div style={{ margin: "6px 0 0" }}>
          <button
            className="btn"
            style={{ fontSize: 11 }}
            data-testid="material-add-folder"
            onClick={() => void onAdd()}
          >
            {t("osinstall.media.addFolder")}
          </button>
          <span className="faint" style={{ fontSize: 10, marginLeft: 8 }}>
            {t("osinstall.media.addHint")}
          </span>
        </div>
        {/*
          **Folders the plan will not read, named rather than dropped.** A
          layered request is a map of one folder per layer, so an untagged
          folder, a second folder under a tag already taken and a tag this
          release does not declare are all folders ART resolves in the
          readout and never plans from. Dropping one silently would be the
          screen contradicting the core about what it is going to do.
        */}
        {unusedForPlan.length > 0 && (
          <p
            className="badge badge-warn"
            data-testid="material-unused"
            style={{ fontSize: 11, margin: "6px 0 0", display: "inline-block" }}
          >
            {t("osinstall.material.unusedForPlan", {
              count: unusedForPlan.length,
              folders: unusedForPlan.join(", "),
            })}
          </p>
        )}
      </div>
      {/*
        **Amiga Forever, offered — never added** (design § 3.5). Shown only
        while the list is empty, and only when the environment variable is
        set and the folders are really there; the Add button is the user
        acting, which is the whole of `remembered.ts`'s rule. Dismissing it
        is a `useState` rather than a remembered key: a suggestion is not a
        setting, and remembering a refusal would be storing a choice nobody
        made about their next machine.
      */}
      {amigaForeverOffer && (
        <p
          className="faint"
          data-testid="amiga-forever-offer"
          style={{ fontSize: 11, margin: "0 0 12px", display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}
        >
          <span>{t("osinstall.material.amigaForever", { path: amigaForeverOffer })}</span>
          <button
            className="btn"
            style={{ fontSize: 11 }}
            onClick={onAmigaForeverAdd}
          >
            {t("osinstall.material.amigaForeverAdd")}
          </button>
          <button
            className="btn"
            style={{ fontSize: 11 }}
            onClick={onAmigaForeverDismiss}
          >
            {t("osinstall.material.amigaForeverDismiss")}
          </button>
        </p>
      )}
      {/*
        **The list, in the folder** (design § 3.7). The readout is on screen
        while ART is open; a person filling a folder over a weekend is at
        their file manager. The same slots compose a text file saying what
        goes there — REQUIRED or OPTIONAL, the names ART expects, where each
        came from, what has to come first and what happens without it — so it
        cannot drift from what the code accepts.

        **Only on the click.** ART does not write into a folder because
        somebody pointed at it; that is `remembered.ts`'s rule about the
        user's own settings, applied to the user's own disk.
      */}
      {/* **The intro belongs to the buttons** (fix round 1, L8): it is a
          sentence about pressing them, so it is inside the same guard rather
          than beside it. The early return above already covers today's only
          empty case; this makes it true of the block itself. */}
      {folders.length > 0 && (
      <div data-testid="material-guide" style={{ margin: "8px 0 0" }}>
        <p className="faint" style={{ fontSize: 10, margin: "0 0 4px" }}>
          {t("osinstall.material.guide.intro")}
        </p>
        {folders.map((entry) => {
          const folder = entry.path;
          const answer = guides[folder];
          return (
            <div key={folder} style={{ margin: "0 0 4px" }}>
              {/* ART-240: the folder is **in** the button rather than beside
                  it. One button per folder with the same three words on each
                  is the identical-accessible-name defect ART-240 is about,
                  and a path repeated next to the button is the same path this
                  step already lists a few lines up. */}
              <button
                className="btn"
                style={{ fontSize: 11, textAlign: "left", wordBreak: "break-all" }}
                data-testid="material-guide-write"
                onClick={() => void writeGuide(folder)}
              >
                {t("osinstall.material.guide.button", { folder })}
              </button>
              {answer && (
                <p
                  className={
                    answer.state === "written"
                      ? "badge badge-ok"
                      : answer.state === "alreadyThere"
                        ? "badge badge-warn"
                        : "badge badge-err"
                  }
                  data-testid={`material-guide-${answer.state}`}
                  style={{ fontSize: 10, margin: "2px 0 0", display: "inline-block", wordBreak: "break-all" }}
                >
                  {answer.state === "written"
                    ? t("osinstall.material.guide.written", { path: answer.path })
                    : answer.state === "alreadyThere"
                      ? t("osinstall.material.guide.alreadyThere", { path: answer.path })
                      : t("osinstall.material.guide.failed", { detail: answer.detail })}
                </p>
              )}
            </div>
          );
        })}
      </div>
      )}
    </>
  );
}
