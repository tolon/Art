// Tab 2 — *Ne kurulacak* (four-tab design § 3.2), round 3 task 2.
//
// One tick list. Its first group is **the release's own parts** — the
// component catalogue `osinstall_components` answers for whatever release
// the session carries, drawn exactly as the install screen drew it: required
// rows ticked and disabled with their sentence, a conditional row stating
// which of exactly four reasons it is in the state it is in, and the
// confirm-off dialog that makes turning off a condition-satisfied component
// a confirmation rather than a plain uncheck. The rows moved here verbatim;
// the reasoning behind every one of them is `@/lib/osinstall`'s, unit-tested
// there, and the three rules the install screen's own module comment states
// (the checklist is the release's own recipe · every conditional tick states
// its reason · turning a condition-satisfied component off is a
// confirmation) are unchanged by the move.
//
// Below the list, **one folded line** saying what the switched-on layering
// components would replace in the tree — the informed-consent half of §92's
// PREVIEW (ART-175). Folded because it is an answer to a question most users
// will not ask, counted in its summary because a fold whose label does not
// say how much is behind it is a control nobody opens.
//
// **The plan is not computed here.** `useInstallPlan` is the one code path
// for it (round 3 task 1), and this tab passes it the same inputs tab 1
// does — the same remembered keys, read through the same guards, so the two
// tabs cannot disagree about what is being planned. The one difference is
// `rescanNonce`: there is no *Scan again* button on this tab, so nothing
// here ever bumps it.
//
// The ticks are the **build session's** (ART-290), never this component's
// own remembered key: `session.components` is what every other screen reads,
// and a panel writing its own copy is what made the session's go stale.

import { useLayoutEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import {
  collisionGroupHeadingKey,
  collisionPhrase,
  componentDef,
  componentLabel,
  conditionalReason,
  conditionalReasonText,
  conditionalToggleAction,
  confirmComponentOff,
  groupCollisionsForPreview,
  hasRomUnknownRefusal,
  isForcedOnByCondition,
  rememberedComponentKey,
  toggleChosen,
  withoutExcluded,
  type ComponentDef,
} from "@/lib/osinstall";
import { isFlag, isText, isTextOrNothing } from "@/lib/remembered";
import { useBuildSession } from "@/lib/useBuildSession";
import { useInstallPlan } from "@/lib/useInstallPlan";
import { useRemembered } from "@/lib/useRemembered";
import { useRomIdentity } from "@/lib/useRomIdentity";

export function ChoiceTab() {
  const { t } = useTranslation();
  const { session, setComponents } = useBuildSession();
  const release = session.release;

  // --- the plan's inputs, read exactly as tab 1 reads them -----------------
  //
  // Read-only here, every one of them: this tab asks no question these
  // answer. The keys and the guards are the install screen's own, because a
  // second key would mean two tabs planning two different builds.
  const [keymap] = useRemembered<string>(
    rememberedComponentKey("osinstall.keymap", release),
    isText,
    ""
  );
  const [reuseScan] = useRemembered<boolean>("osinstall.reuseScan", isFlag, true);
  const [destination] = useRemembered<string | null>(
    rememberedComponentKey("osinstall.destination", release),
    isTextOrNothing,
    null
  );
  const romPath = session.rom.path;
  // Only the identity: a conditional component's own reason line names the
  // Kickstart, and nothing on this tab draws the ROM's outcome sentences
  // (they are tab 3's).
  const { rom } = useRomIdentity(romPath);

  const plan = useInstallPlan({
    release,
    material: session.material,
    keymap,
    rom: romPath,
    destination,
    reuseScan,
    // **No *Scan again* on this tab.** The button that bumps this is the
    // media section's, on tab 1; a constant here says so rather than
    // carrying a counter nothing can move.
    rescanNonce: 0,
    components: session.components,
    setComponents,
  });
  const {
    catalogue,
    componentsError,
    basePlan,
    effectivePlan,
    layeringOn,
    componentPreview,
    componentPreviewError,
  } = plan;

  const chosen = session.components.chosen;
  const excludedConditional = session.components.excludedConditional;
  const setChosen = (next: string[]) => setComponents({ chosen: next });
  const setExcludedConditional = (next: string[]) =>
    setComponents({ excludedConditional: next });

  /** The one component id currently showing the "this will not boot"
   *  confirmation, or `null`. Only one at a time — a second click elsewhere
   *  replaces it. */
  const [pendingExclusion, setPendingExclusion] = useState<string | null>(null);
  /**
   * **A confirmation describes the plan that was on screen when it was
   * asked for**, so a new plan answer retires it. `planVersion` bumps on
   * every answer, a refusal included: a plan that could not be computed is
   * not a plan the user confirmed against either.
   *
   * `useLayoutEffect`, not `useEffect`: a `useEffect` runs *after* the
   * browser has painted, so a stale confirmation would be on screen for one
   * frame over a plan it does not describe — and one frame is enough to
   * click.
   */
  useLayoutEffect(() => {
    setPendingExclusion(null);
  }, [plan.planVersion]);

  const baseRomUnknown = basePlan ? hasRomUnknownRefusal(basePlan) : false;

  /**
   * A component's own name for the screen — the recipe's `labelKey` when it
   * declares one, its media name otherwise. Resolved here rather than in
   * `src/lib`, which holds no i18next singleton: a `labelKey` is a whole key
   * with no parameters, so it needs a `t` call at the place that draws it
   * and no `Phrase` wrapper.
   */
  function label(id: string): string {
    const key = componentDef(catalogue ?? [], id)?.labelKey;
    return key ? t(key) : componentLabel(catalogue ?? [], id);
  }

  function toggleConditional(def: ComponentDef, loaded: ComponentDef[]) {
    const excluded = excludedConditional.includes(def.id);
    const forcedOn = isForcedOnByCondition(loaded, basePlan, chosen, def.id);
    switch (conditionalToggleAction(excluded, forcedOn)) {
      case "undo-exclusion":
        setExcludedConditional(withoutExcluded(excludedConditional, def.id));
        return;
      case "confirm-off":
        // Turning off a condition-satisfied component is a confirmation,
        // not a plain uncheck — it is the user's machine, and it is also a
        // machine that may not boot afterwards.
        setPendingExclusion(def.id);
        return;
      case "toggle-chosen":
        // Off, and not because of the condition: either an ordinary opt-in
        // (the condition does not currently hold) or undoing that same
        // opt-in.
        setChosen(toggleChosen(loaded, chosen, def.id));
    }
  }

  function confirmExclusion(id: string) {
    const next = confirmComponentOff(chosen, excludedConditional, id);
    setChosen(next.chosen);
    setExcludedConditional(next.excluded);
    setPendingExclusion(null);
  }

  return (
    <section className="card" data-testid="choice-tab" style={{ marginBottom: 16 }}>
      <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osBuilder.step.secim")}</h2>
      <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
        {t("osBuilder.choice.lead")}
      </p>

      <div data-testid="choice-parts">
        <h3 style={{ fontSize: 14, margin: "0 0 8px" }}>{t("osBuilder.choice.parts")}</h3>

        {componentsError && (
          <p className="badge badge-err" style={{ fontSize: 11, margin: "0 0 12px", display: "inline-block" }}>
            {t("osinstall.components.unavailable")}
          </p>
        )}
        {!componentsError && catalogue === null && (
          <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
            {t("osinstall.components.loading")}
          </p>
        )}

        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          {(catalogue ?? []).map((def) => {
            const excluded = excludedConditional.includes(def.id);
            const checked = def.required
              ? true
              : !def.available
                ? false
                : def.conditionMajor !== null
                  ? (effectivePlan?.componentsOn.includes(def.id) ?? false)
                  : chosen.includes(def.id);
            const disabled = def.required || !def.available;

            // Non-null inside this map by construction — `catalogue ?? []`
            // above yields no rows at all while it is loading.
            const loaded = catalogue ?? [];

            function handleChange() {
              if (disabled) return;
              if (def.conditionMajor !== null) {
                toggleConditional(def, loaded);
              } else {
                setChosen(toggleChosen(loaded, chosen, def.id));
              }
            }

            // Exactly one of four reasons, computed the same way for every
            // conditional row — see `conditionalReason`'s own doc comment
            // for why this can never fall through to nothing.
            //
            // ART-119 (#4): gated on `!def.required && def.available` —
            // dropped in an earlier fix round and re-added here. Unreachable
            // against today's shipped recipe (no component is both
            // conditional and either required or unavailable), but a
            // required or coming-later row already renders its own
            // "required"/"coming later" line above; a future recipe that
            // combined the two should not additionally show a rom-needed or
            // condition-on/off badge that contradicts it.
            const reason =
              def.conditionMajor !== null && !def.required && def.available
                ? conditionalReason(
                    def.conditionMajor,
                    isForcedOnByCondition(loaded, basePlan, chosen, def.id),
                    excluded,
                    baseRomUnknown,
                    rom?.name ?? null
                  )
                : null;
            const reasonText = reason ? conditionalReasonText(reason) : null;

            return (
              <div
                key={def.id}
                data-testid="choice-part-row"
                data-component={def.id}
                style={{
                  border: "1px solid var(--border)",
                  borderRadius: 4,
                  padding: "6px 10px",
                  background: checked ? "var(--bg-hover)" : "var(--bg)",
                }}
              >
                <label style={{ display: "flex", gap: 8, alignItems: "center", fontSize: 13 }}>
                  <input type="checkbox" checked={checked} disabled={disabled} onChange={handleChange} />
                  <strong>{label(def.id)}</strong>
                  {def.required && (
                    <span className="badge badge-muted" style={{ fontSize: 10 }}>
                      {t("osinstall.components.required")}
                    </span>
                  )}
                  {!def.available && (
                    <span className="badge badge-muted" style={{ fontSize: 10 }}>
                      {t("common.comingLater")}
                    </span>
                  )}
                </label>

                {def.required && (
                  <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
                    {t("osinstall.components.reason.required")}
                  </p>
                )}

                {/* ART-157. A Kickstart *minimum* is a fact about what this
                    component's files need to run, not a switch — it never
                    turns a row on or off, so it is rendered on its own
                    rather than through `conditionalReason`, whose four
                    branches all describe `rom-older-than`'s switching. Shown
                    for every row that declares one, `required` included:
                    AmigaOS 3.9's floor sits on `workbench-base`, which is
                    required, and that is precisely the row a user needs to
                    read it on. */}
                {def.requiresRomMajor !== null && (
                  <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
                    {t("osinstall.components.reason.romAtLeast", { major: def.requiresRomMajor })}
                  </p>
                )}

                {/* ART-119 (#2). This used to be four independent `&&`
                    guards, one per kind, so a fifth kind would have rendered
                    nothing at all — a conditional row ticked with no
                    explanation, the same defect a review already found here
                    once. `conditionalReasonText` is an exhaustive `switch`
                    over the union with a `never` fallthrough, so a fifth kind
                    is now a compile error instead of a blank line, and there
                    is one place deciding the wording rather than four. */}
                {reasonText && (
                  <p
                    className={reasonText.tone === "warn" ? "badge badge-warn" : "faint"}
                    style={{
                      fontSize: 11,
                      margin: "4px 0 0",
                      ...(reasonText.tone === "warn" ? { display: "inline-block" as const } : {}),
                    }}
                  >
                    {t(reasonText.phrase.key, reasonText.phrase.params)}
                  </p>
                )}

                {pendingExclusion === def.id && def.conditionMajor !== null && (
                  <div
                    className="badge badge-err"
                    data-testid="choice-part-confirm-off"
                    style={{ display: "block", padding: "8px 10px", margin: "6px 0 0", fontSize: 11 }}
                  >
                    <p style={{ margin: "0 0 8px" }}>
                      {t("osinstall.components.confirmOff.warning", { major: def.conditionMajor })}
                    </p>
                    <div style={{ display: "flex", gap: 8 }}>
                      <button className="btn" onClick={() => confirmExclusion(def.id)}>
                        {t("osinstall.components.confirmOff.confirm")}
                      </button>
                      <button className="btn" onClick={() => setPendingExclusion(null)}>
                        {t("common.cancel")}
                      </button>
                    </div>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>

      {/* ART-175, folded (design § 3.2). What the switched-on layering
          components would replace, before the build runs. The summary line
          **counts** — a fold labelled only "what this would replace" says
          nothing about whether opening it is worth the click, and "0" and
          "41" are different decisions.

          Drawn only when there is something to replace: a fold over an empty
          table opens onto nothing. What was *placed* rather than replaced is
          the summary sentence inside, which states all three counts in full
          because an empty report means "nothing is in the way", never
          "nothing happens" (§89). */}
      {componentPreview && componentPreview.reports.length > 0 && (
        <details data-testid="choice-replaces-fold" style={{ margin: "12px 0 0" }}>
          <summary
            data-testid="choice-replaces-summary"
            className="muted"
            style={{ fontSize: 12, cursor: "pointer" }}
          >
            {t("osBuilder.choice.replaces", { count: componentPreview.reports.length })}
          </summary>
          <p className="muted" style={{ fontSize: 12, margin: "8px 0 10px" }}>
            {t("osinstall.replaces.summary", {
              components: layeringOn.map(label).join(", "),
              placed: componentPreview.placed,
              // **`contested`, not `reports.length`** (review F4).
              // `collide::preview` drops identical rows before returning,
              // so `placed - reports.length` counted a file landing
              // byte-for-byte on another component's copy as *new* — 130
              // of them on the AmigaOS 3.9 overlay. The three counts
              // partition what would be placed, and each is its own fact:
              // landed on nothing, landed on identical bytes, replaced
              // something.
              fresh: componentPreview.placed - componentPreview.contested,
              unchanged: componentPreview.contested - componentPreview.reports.length,
              replaced: componentPreview.reports.length,
            })}
          </p>
          <div
            style={{
              maxHeight: 260,
              overflowY: "auto",
              border: "1px solid var(--border)",
              borderRadius: 4,
              padding: "6px 10px",
            }}
          >
            {groupCollisionsForPreview(componentPreview.reports).map((group) => (
              <div key={group.kind} style={{ marginBottom: 10 }}>
                <div
                  className={group.kind === "downgrade" ? "badge badge-err" : "muted"}
                  style={{
                    fontSize: 11,
                    fontWeight: 600,
                    margin: "4px 0",
                    display: group.kind === "downgrade" ? "inline-block" : "block",
                  }}
                >
                  {t(collisionGroupHeadingKey(group.kind), { count: group.reports.length })}
                </div>
                {group.reports.map((report) => {
                  const phrase = collisionPhrase(report.collision);
                  return (
                    <div
                      key={report.path}
                      data-testid="component-collision-row"
                      style={{
                        fontSize: 11,
                        padding: "3px 0",
                        borderBottom: "1px solid var(--border)",
                        display: "flex",
                        justifyContent: "space-between",
                        gap: 8,
                      }}
                    >
                      <span style={{ wordBreak: "break-all" }}>{report.path}</span>
                      <span
                        className={group.kind === "downgrade" ? undefined : "faint"}
                        style={
                          group.kind === "downgrade" ? { color: "var(--err-text)" } : undefined
                        }
                      >
                        {t(phrase.key, phrase.params)}
                      </span>
                    </div>
                  );
                })}
              </div>
            ))}
          </div>
        </details>
      )}

      {componentPreviewError && (
        <p
          className="badge badge-err"
          style={{ fontSize: 11, margin: "12px 0 0", display: "inline-block" }}
        >
          {t("osinstall.replaces.failed", { error: componentPreviewError })}
        </p>
      )}
    </section>
  );
}
