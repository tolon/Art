// What this release needs, and what the user's folders turn out to hold
// (design § 3.3).
//
// **The screen the whole round exists for.** `core::osinstall::slots` derives
// one slot per artefact a release's build can use — the CD, each shipped
// package, each overlay, the ROM — and resolves every one of them against the
// material folders, the chosen tree and the chosen ROM in a single answer.
// `src/lib/slots.ts` turns each resolved slot into exactly one `Phrase`. This
// component draws them, and does nothing else: there is no decision here, and
// deliberately no *choice* either — an ambiguous row lists its candidates and
// leaves the picking to the user (Task 4 puts the override on the panel).
//
// Three rules from CLAUDE.md shape it and are worth naming here, because each
// one is a sentence somebody could have written instead:
//
//   - **Endings stay distinct.** Eight of them, one per row kind, each with
//     its own sentence and its own next step. "Not found" and "not needed"
//     are not the same row; neither are "ART did not look at the bytes" and
//     "ART looked and the table does not know them".
//   - **The screen may not out-claim the core.** Every sentence on a row is a
//     `Phrase` `slots.ts` produced from what Rust actually measured. Nothing
//     here infers, and the `installed` badge in particular comes from the
//     tree's own `distribution.json` and never from a file being present.
//   - **No bar without a total.** While the pass is in flight the readout
//     says how many folders it is reading. `osinstall_slots` is one round
//     trip and reports no progress of its own, so a fraction would be a
//     number ART cannot fill in and a moving sliver would be a picture of
//     work rather than a report of it.
//
// **Superseded answers are dropped, not rendered.** The same cancellation the
// identification pass uses (ART-089's own mechanism): a folder list changed
// while a slow answer is in the air must not have the previous list's report
// land on top of the new one.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { errorText } from "@/lib/errorText";
import { osinstallSlots, type InstallRelease, type SlotReport } from "@/lib/osinstall";
import {
  readoutRunningLine,
  setLine,
  slotLines,
  unreadableFolderLines,
  type SlotLineKind,
} from "@/lib/slots";

/**
 * How a row's own ending is drawn.
 *
 * A find is plain, a guess and an ambiguity are warnings, a missing artefact
 * is an error, and a measured-unnecessary row is faint — the same four
 * weights the design's own sketch uses. Kept beside the `kind` rather than
 * derived from the phrase key, so nothing here re-decides which ending a row
 * is; `slots.ts` already did.
 */
function rowClass(kind: SlotLineKind): string {
  switch (kind) {
    case "found-by-hash":
    case "found-by-name":
    case "chosen":
      return "badge badge-ok";
    case "guessed-by-filename":
    case "ambiguous":
      return "badge badge-warn";
    case "not-found":
    case "chosen-missing":
      return "badge badge-err";
    case "not-needed":
      return "badge badge-muted";
  }
}

/** The mark in front of a row. Text, not colour alone — a readout somebody
 *  reads in greyscale, or with a colour-vision deficiency, still has to say
 *  which rows are the problem. */
function rowMark(kind: SlotLineKind): string {
  switch (kind) {
    case "found-by-hash":
    case "found-by-name":
    case "chosen":
      return "✔";
    case "guessed-by-filename":
    case "ambiguous":
      return "!";
    case "not-found":
    case "chosen-missing":
      return "✖";
    case "not-needed":
      return "—";
  }
}

export interface MaterialReadoutProps {
  release: InstallRelease;
  /** Every folder in the build's material list, in list order. */
  folders: string[];
  /** The distribution tree, when one is chosen — the only source of the
   *  `installed` badge. */
  treeRoot: string | null;
  /** The chosen Kickstart, which fills the ROM slot. */
  rom: string | null;
}

export function MaterialReadout({ release, folders, treeRoot, rom }: MaterialReadoutProps) {
  const { t } = useTranslation();
  const [report, setReport] = useState<SlotReport | null>(null);
  const [running, setRunning] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);

  // A primitive dependency rather than the array: two equal strings are the
  // same value to React's own comparison, two equal arrays are not, and this
  // effect starts disk work (ART-178/ART-195, measured at 2,149 preview jobs
  // in one session).
  const foldersKey = folders.join("\n");

  useEffect(() => {
    const list = foldersKey ? foldersKey.split("\n") : [];
    if (list.length === 0) {
      // Nothing pointed at yet is not "nothing found": it is a question
      // nobody has asked. The readout renders its own empty line instead of
      // a set line claiming a release is missing everything.
      setReport(null);
      setRunning(false);
      setFailed(null);
      return;
    }
    let cancelled = false;
    setRunning(true);
    setFailed(null);
    osinstallSlots(release, list, treeRoot, rom)
      .then((answer) => {
        if (cancelled) return;
        setReport(answer);
        setRunning(false);
      })
      .catch((e) => {
        if (cancelled) return;
        // Named rather than swallowed. A readout that could not be produced
        // must not look like one that found nothing: the second is a claim
        // about the user's folders, and this is a claim about ART.
        setReport(null);
        setRunning(false);
        setFailed(errorText(t, e));
      });
    return () => {
      cancelled = true;
    };
    // `t` is deliberately absent: it changes identity on a language switch,
    // and re-running a folder scan because somebody changed language would be
    // disk work for a translation. The message already on screen is
    // re-rendered in the new language by `errorText`'s own key when it is one
    // ART recognises.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [release, foldersKey, treeRoot, rom]);

  if (folders.length === 0) return null;

  const rows = report ? slotLines(report.states) : [];
  const set = report ? setLine(report.summary, report.states) : null;
  const unreadable = report ? unreadableFolderLines(report.unreadableFolders) : [];

  return (
    <div data-testid="material-readout" style={{ margin: "0 0 12px" }}>
      <p className="faint" style={{ fontSize: 11, margin: "0 0 4px", fontWeight: 600 }}>
        {t("osinstall.material.readoutHeading")}
      </p>

      {running && (
        <p className="faint" data-testid="material-readout-running" style={{ fontSize: 11, margin: "0 0 4px" }}>
          {(() => {
            const phrase = readoutRunningLine(folders.length);
            return t(phrase.key, phrase.params);
          })()}
        </p>
      )}

      {failed && (
        <p
          className="badge badge-err"
          data-testid="material-readout-failed"
          style={{ fontSize: 11, margin: "0 0 4px", display: "inline-block" }}
        >
          {t("osinstall.material.readoutFailed", { detail: failed })}
        </p>
      )}

      {set && (
        <p
          className={set.ready ? "badge badge-ok" : "badge badge-err"}
          data-testid="material-set-line"
          style={{ fontSize: 11, margin: "0 0 6px", display: "inline-block" }}
        >
          {t(set.phrase.key, set.phrase.params)}
        </p>
      )}

      {rows.map((row) => (
        <div key={row.id} data-testid={`material-row-${row.kind}`} style={{ margin: "0 0 3px" }}>
          <p style={{ fontSize: 11, margin: 0, display: "flex", gap: 6, alignItems: "baseline", flexWrap: "wrap" }}>
            <span className={rowClass(row.kind)} style={{ fontSize: 10 }} aria-hidden>
              {rowMark(row.kind)}
            </span>
            <strong>{row.name}</strong>
            <span className="badge badge-muted" style={{ fontSize: 10 }}>
              {row.required
                ? t("osinstall.material.required")
                : t("osinstall.material.optional")}
            </span>
            {row.installed && (
              <span className="badge badge-ok" style={{ fontSize: 10 }} data-testid="material-row-installed">
                {t(row.installed.key, row.installed.params)}
              </span>
            )}
            <span style={{ wordBreak: "break-all" }}>{t(row.phrase.key, row.phrase.params)}</span>
          </p>
          {row.blocked && (
            <p className="faint" style={{ fontSize: 10, margin: "0 0 0 18px" }} data-testid="material-row-blocked">
              {t(row.blocked.key, row.blocked.params)}
            </p>
          )}
        </div>
      ))}

      {/*
        **A folder ART could not read at all, named** (Task 2's fix round, F2,
        whose field this finally renders). The rows above stay true: they were
        resolved against every folder that *could* be read, and a remembered
        path on a drive nobody plugged in must not make the disks in the
        folder beside it read as missing.
      */}
      {unreadable.map((line) => (
        <p
          key={line.folder}
          className="badge badge-err"
          data-testid="material-readout-unreadable"
          style={{ fontSize: 11, margin: "4px 0 0", display: "inline-block" }}
          title={line.folder}
        >
          {t(line.phrase.key, line.phrase.params)}
        </p>
      ))}
    </div>
  );
}
