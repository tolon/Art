// The Kickstart agreement step: what the titles want, what ART found, and
// only what the user ticks (round 4, task 9; the owner's rule, 2026-08-21).
//
// **Nothing is ticked by ART.** The agreed set starts empty regardless of
// what the proposal offers, and only a press adds a name to it. A row whose
// offer is not `supplied`, or whose `.RTB` is `missing`
// (`cardOsKickstarts.ts::canAgree`), cannot be ticked at all — its checkbox
// is disabled and the row's own two lines already say why and, where there
// is one, what to do instead: an encrypted offer names `rom.key`; a missing
// `.RTB` names the package it ships in. Ticking nothing is allowed — the
// summary line says so, and `card_os_build` proceeds with an empty
// `agreedKickstarts` (Task 10's concern, not this component's).
//
// **Presentational.** This component derives nothing from a session or a
// job: it takes the proposal it was prepared with and reports the agreed
// names upward through `onAgreedChange`, the full set each time rather than
// a delta. Task 10 mounts it in the run and is the one that actually calls
// `card_os_build` with what came back.
//
// The row follows the card path's own idiom rather than inventing a new
// one: `.card-prow` (`CardPartitionRow.tsx`) for the dense row, `.badge` for
// the mock-up's `.tag` states (design § 4.4 — *warn* means "not ready",
// never a claim that something is wrong), and `.infobar warn` for the
// unreadable-slaves notice, the same as `CardSection.tsx`'s own refusal
// strip.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import type { KickstartProposal, ProposedKickstart } from "@/lib/cardOs";
import { canAgree, offerPhrase, rtbPhrase, summaryPhrase, titlesPhrase } from "@/lib/cardOsKickstarts";

export interface KickstartAgreementProps {
  proposal: KickstartProposal;
  /** Called with the full agreed set every time it changes. */
  onAgreedChange: (agreed: string[]) => void;
}

const ROW_COLUMNS = "24px 1.2fr 1.4fr 1.4fr 90px";

export function KickstartAgreement({ proposal, onAgreedChange }: KickstartAgreementProps) {
  const { t } = useTranslation();
  // Nothing ticked by ART (owner's rule): the set starts empty no matter
  // what the proposal offers.
  const [agreed, setAgreed] = useState<Set<string>>(new Set());

  /**
   * **And it starts empty again for every new proposal** (round 4 final
   * review, minor).
   *
   * This set is this component's own, and `BuildTab` mounts it once for the
   * whole tab: a second run in the same mount inherited the first run's ticks
   * — the run's `setAgreed([])` clears the *parent's* copy, which is what the
   * build is sent, not this one, which is what the boxes draw. So the screen
   * showed a name ticked that the parent had forgotten, and the build was
   * refused by `check_agreed` for a name the new proposal does not offer: the
   * user refused rather than told. Keyed on the proposal **object**, not on
   * its names, so two runs offering the same Kickstart still start clean.
   */
  useEffect(() => {
    setAgreed(new Set());
    onAgreedChange([]);
    // `onAgreedChange` is the parent's own callback and is not what decides
    // this; a new proposal is.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [proposal]);

  function toggle(item: ProposedKickstart) {
    if (!canAgree(item)) return;
    setAgreed((before) => {
      const next = new Set(before);
      if (next.has(item.name)) next.delete(item.name);
      else next.add(item.name);
      onAgreedChange(Array.from(next));
      return next;
    });
  }

  const summary = summaryPhrase(agreed.size, proposal.items.length);

  return (
    <section data-testid="kickstart-agreement">
      <h3 style={{ fontSize: 14, margin: "0 0 4px" }}>{t("cardKickstart.heading")}</h3>
      <p className="faint" style={{ fontSize: 11, margin: "0 0 8px" }}>
        {t("cardKickstart.intro")}
      </p>

      {proposal.items.length === 0 ? (
        <p className="faint" style={{ fontSize: 12 }} data-testid="kickstart-empty">
          {t("cardKickstart.noneNeeded")}
        </p>
      ) : (
        <>
          <div className="card-prow card-pane-head" style={{ gridTemplateColumns: ROW_COLUMNS }}>
            <span />
            <span>{t("cardKickstart.column.name")}</span>
            <span>{t("cardKickstart.column.offer")}</span>
            <span>{t("cardKickstart.column.rtb")}</span>
            <span />
          </div>

          {proposal.items.map((item) => {
            const eligible = canAgree(item);
            const offer = offerPhrase(item.offer);
            const rtb = rtbPhrase(item.rtb);
            const titles = titlesPhrase(item);
            const tag = eligible
              ? { className: "badge badge-ok", label: t("cardKickstart.tag.ready") }
              : { className: "badge badge-warn", label: t("cardKickstart.tag.blocked") };

            return (
              <div
                key={item.name}
                className="card-prow"
                style={{ gridTemplateColumns: ROW_COLUMNS, alignItems: "start" }}
                data-testid={`kickstart-row-${item.name}`}
              >
                <input
                  type="checkbox"
                  aria-label={t("cardKickstart.agree", { name: item.name })}
                  checked={agreed.has(item.name)}
                  disabled={!eligible}
                  onChange={() => toggle(item)}
                  data-testid={`kickstart-check-${item.name}`}
                  style={{ marginTop: 3 }}
                />
                <span>
                  <code>{item.name}</code>
                  <div className="faint" style={{ fontSize: 11 }}>
                    {t(titles.key, titles.params)}
                  </div>
                </span>
                <span data-testid={`kickstart-offer-${item.name}`}>
                  {t(offer.key, offer.params)}
                </span>
                <span data-testid={`kickstart-rtb-${item.name}`}>{t(rtb.key, rtb.params)}</span>
                <span className={tag.className} data-testid={`kickstart-tag-${item.name}`}>
                  {tag.label}
                </span>
              </div>
            );
          })}
        </>
      )}

      {proposal.unreadableSlaves.length > 0 && (
        <div data-testid="kickstart-unreadable" style={{ marginTop: 8 }}>
          <p className="infobar warn">
            {t("cardKickstart.unreadable.heading", { count: proposal.unreadableSlaves.length })}
          </p>
          <ul style={{ fontSize: 11, margin: "4px 0 0", paddingLeft: 16 }}>
            {proposal.unreadableSlaves.map((path) => (
              <li key={path}>{path}</li>
            ))}
          </ul>
        </div>
      )}

      <p className="card-pane-status" data-testid="kickstart-summary">
        {t(summary.key, summary.params)}
      </p>
    </section>
  );
}
