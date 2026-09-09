// Tab 2 — *Ne kurulacak* (four-tab design § 3.2), round 3 tasks 2 and 3.
//
// **One tick list, three groups, one order.** The release's parts, the
// updates in the material's own order, and one first-boot tick — the whole
// of what this build will put on the Amiga, on one screen, with one tick
// column. It replaces two panels that each drew their own list of one
// release's packages from two unrelated sources: `PackagePanel`'s flat
// catalogue never joined the chain, so one lane could say two different
// things about one file (ART-289's own shape). That panel is deleted.
//
// Its first group is **the release's own parts** — the
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
// Its second group is **the updates, in the material's own order** —
// `osinstall_chain`'s rows through `chainLines`, one tick each, each row's
// own sentence beneath it and **no sentence composed here**: the eight
// endings are `core::osinstall::chain`'s. Whether a tick is the user's at all
// is `choiceRowState`, one ordered decision in `@/lib/chain` rather than four
// guards in this file's JSX. Its third is **one first-boot tick**, absent
// meaning ticked, because the first-boot block is what makes a PiStorm tree
// boot its own hardware.
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

import { useEffect, useLayoutEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { slotOverrides } from "@/lib/amigainstall";
import { chainLines, choiceRowState, type ChainLine } from "@/lib/chain";
import { firstbootPreview } from "@/lib/firstboot";
import type { Phrase } from "@/lib/phrase";
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
  osinstallChain,
  rememberedComponentKey,
  toggleChosen,
  withoutExcluded,
  type ChainReport,
  type ComponentDef,
  type SlotOverride,
} from "@/lib/osinstall";
import { isFlag, isText, isTextOrNothing } from "@/lib/remembered";
import { useBuildSession } from "@/lib/useBuildSession";
import { useChainTree } from "@/lib/useChainTree";
import { useInstallPlan } from "@/lib/useInstallPlan";
import { useRemembered } from "@/lib/useRemembered";
import { useRomIdentity } from "@/lib/useRomIdentity";
import { useSettingsStore } from "@/stores/settingsStore";

export function ChoiceTab() {
  const { t } = useTranslation();
  const { session, setComponents, setPackages, setFirstBoot } = useBuildSession();
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

  // --- group 2: the updates, in the material's own order -------------------
  //
  // `osinstall_chain` answers the whole AmigaOS 3.9 chain against the same
  // folders, tree and ROM the material readout is answered against, so the
  // two screens cannot say different things about one file. The rows arrive
  // already sorted by `(position, id)` in Rust and are **not** re-sorted
  // here: the order is the order the material goes on, which is information.

  /**
   * The user's own per-slot file choices, as `osinstall_chain` takes them
   * (ART-284/ART-289).
   *
   * **A string, deposited into the dependency array**, exactly as
   * `AmigaInstallPanel` and `MaterialReadout` already do it: `slotOverrides`
   * builds a fresh array on every render, and the effect below starts disk
   * work — an array identity there is ART-178's loop.
   */
  const rememberedBag = useSettingsStore((s) => s.settings.remembered);
  const overridesKey = JSON.stringify(slotOverrides(rememberedBag));

  /**
   * The tree the chain is asked about — **the same one tab 1 asks about**
   * (`useChainTree`, fix round 1's Important 2). The rule is that hook's, in
   * one place, because two lanes with two trees is two `installed` answers
   * about one file.
   *
   * `settled` gates the ask rather than merely the render: `isTree` is a
   * round trip, so asking before it lands would put *ready* on a row the
   * tree already carries and then swap it for *installed* under the reader.
   */
  const { treeRoot, settled: treeSettled } = useChainTree(destination);
  /** A primitive dependency rather than the array, for the reason above. */
  const materialKey = session.material.folders.map((folder) => folder.path).join("\n");

  /** `null` means *not answered yet*, never *no chain*. A release ART knows
   *  no chain for answers with **no rows**, which is a different fact and
   *  draws no group at all (spec § 1.5: AmigaOS 3.2 has none). */
  const [chain, setChain] = useState<ChainReport | null>(null);
  useEffect(() => {
    // Not until the tree is settled — see `treeRoot` above. One ask, about
    // the right folder, rather than two about two.
    if (!treeSettled) return;
    const folders = materialKey ? materialKey.split("\n") : [];
    let cancelled = false;
    osinstallChain(release, folders, treeRoot, romPath, JSON.parse(overridesKey) as SlotOverride[])
      .then((answer) => {
        if (!cancelled) setChain(answer);
      })
      .catch(() => {
        // Silent, and for the readout's own reason: a red box here would
        // report a fault in ART as though it were a statement about the
        // user's files.
        if (!cancelled) setChain(null);
      });
    return () => {
      cancelled = true;
    };
  }, [release, materialKey, treeRoot, treeSettled, romPath, overridesKey]);

  const updateRows = chain?.rows ?? [];
  // Paired **by index**: `chainLines` maps one line per row, in order, and a
  // row's `position` is a rank the material may give twice (`locale-39` and
  // `locale-39-turkish` are both 4), which makes it useless as a key.
  const updateLines = chainLines(updateRows);
  const packagesChosen = session.packages.chosen;

  /**
   * Remembered ids no row in this release's chain accounts for — a stale
   * choice from an older ART, or a package since removed (`PackagePanel`'s
   * N4, kept).
   *
   * **Only once the chain has actually answered.** While `chain` is `null`
   * every chosen id looks unknown, for a moment that says nothing true.
   */
  const strayChosen = chain
    ? packagesChosen.filter((id) => !updateLines.some((line) => line.id === id))
    : [];

  function toggleUpdate(id: string) {
    setPackages({
      chosen: packagesChosen.includes(id)
        ? packagesChosen.filter((held) => held !== id)
        : [...packagesChosen, id],
    });
  }

  /**
   * A row's sentence, with the one parameter `@/lib/chain` cannot fill.
   *
   * A row blocked on a component names that component by the **key** the
   * parts group above labels it with, because `@/lib/chain` never renders
   * (CLAUDE.md). A component with no `labelKey` shows its id — which is what
   * the parts group shows for it too, so the two cannot disagree.
   */
  function sentenceFor(line: ChainLine): Phrase {
    if (line.kind !== "blocked-component") return line.phrase;
    return {
      ...line.phrase,
      params: {
        ...line.phrase.params,
        components: line.components
          .map((component) => (component.labelKey ? t(component.labelKey) : component.id))
          .join(", "),
      },
    };
  }

  // --- group 3: the first-boot tick ---------------------------------------
  //
  // `alreadyWritten` is a fact about the **folder**, and a different question
  // from the tick: one says what is there, the other says what this build
  // should do. Asked only of a real tree — `firstboot_preview` reads one, and
  // a refusal is not something a row whose whole content is a tick can
  // explain.
  const [firstbootWritten, setFirstbootWritten] = useState(false);
  useEffect(() => {
    // Same gate as the chain's: a folder the destination check has not
    // answered for is not a folder to read a first-boot block out of.
    if (!treeSettled) return;
    if (!treeRoot) {
      setFirstbootWritten(false);
      return;
    }
    let cancelled = false;
    firstbootPreview(treeRoot)
      .then((preview) => {
        if (!cancelled) setFirstbootWritten(preview.alreadyWritten);
      })
      .catch(() => {
        if (!cancelled) setFirstbootWritten(false);
      });
    return () => {
      cancelled = true;
    };
  }, [treeRoot, treeSettled]);

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


      {/* **Group 2 — the updates, in the material's own order** (design
          § 3.2). One tick per chain row, each row's own sentence under it,
          and nothing composed here: the eight endings are `chain.rs`'s and
          `@/lib/chain` only chooses which of them says so.

          Drawn only when there are rows. A release ART knows no chain for —
          AmigaOS 3.2 (spec § 1.5) — gets no heading, because a heading over
          an empty list says a release has updates and none of them apply.
          The stray remembered ids below are their own reason to draw it:
          without the group there is nowhere to untick one from. */}
      {(updateLines.length > 0 || strayChosen.length > 0) && (
        <div data-testid="choice-updates" style={{ marginTop: 16 }}>
          <h3 style={{ fontSize: 14, margin: "0 0 8px" }}>{t("osBuilder.choice.updates")}</h3>

          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            {updateLines.map((line, at) => {
              // By index — see `updateLines`' own comment for why `position`
              // cannot be the pairing key.
              const state = choiceRowState(line, updateRows[at]);
              const checked = state.tick === "user" ? packagesChosen.includes(line.id) : state.tick === "on";
              const said = sentenceFor(line);
              // **A tick the row can no longer honour is still the user's to
              // take back** (fix round 1, Important 1 — the rule
              // `PackagePanel`'s F3 and M3 encoded: only *checking* is ever
              // refused, unticking a stale pick is always allowed). The
              // archive a ticked row was ticked for can leave the folder
              // between two runs, and the row then draws unticked and
              // disabled while its id sits on in `packages.chosen`, invisible
              // and unremovable: `strayChosen` below catches only an id with
              // no row at all. The box stays honest — the row cannot be
              // written, so it is not drawn as though it will be — and the
              // way out is a control of its own beside it.
              const closedButTicked = state.tick === "off" && packagesChosen.includes(line.id);
              return (
                <div
                  key={line.id}
                  data-testid="choice-update-row"
                  data-package={line.id}
                  style={{
                    border: "1px solid var(--border)",
                    borderRadius: 4,
                    padding: "6px 10px",
                    background: checked ? "var(--bg-hover)" : "var(--bg)",
                  }}
                >
                  <label style={{ display: "flex", gap: 8, alignItems: "center", fontSize: 13 }}>
                    <input
                      type="checkbox"
                      checked={checked}
                      disabled={!state.enabled}
                      onChange={() => toggleUpdate(line.id)}
                    />
                    <span className="faint" style={{ minWidth: 14 }}>
                      {line.position}
                    </span>
                    <strong>{line.name}</strong>
                  </label>
                  {/* The row's own state sentence, always — a disabled box
                      with nothing beside it is a refusal nobody can act on.
                      `where` is its own fact, true of the row whatever state
                      the row is in, and it is what makes the dead box on a
                      row that runs on the Amiga make sense. */}
                  <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
                    {t(said.key, said.params)}
                    {line.where && (
                      <span style={{ marginLeft: 6 }}>{t(line.where.key, line.where.params)}</span>
                    )}
                  </p>
                  {closedButTicked && (
                    <div
                      data-testid="choice-update-closed-tick"
                      className="badge badge-warn"
                      style={{
                        display: "flex",
                        gap: 8,
                        alignItems: "center",
                        justifyContent: "space-between",
                        padding: "4px 8px",
                        margin: "6px 0 0",
                        fontSize: 11,
                      }}
                    >
                      {/* Two ways out, and the sentence names both: drop the
                          tick, or fix what the row above says is wrong. It
                          does not repeat *why* — the row's own sentence is
                          directly above it and is the one place that decides
                          the wording. */}
                      <span>{t("osBuilder.choice.tickedButClosed")}</span>
                      <button className="btn" onClick={() => toggleUpdate(line.id)}>
                        {t("osBuilder.choice.untick")}
                      </button>
                    </div>
                  )}
                </div>
              );
            })}

            {/* N4, kept from `PackagePanel`: a remembered id this release's
                chain has no row for used to render nothing at all —
                invisible, and so impossible to clear, since there was no box
                to click. It is not a tick (there is nothing to tick), so it
                is a sentence and a button that does the one thing available. */}
            {strayChosen.map((id) => (
              <div
                key={id}
                data-testid="choice-update-unknown"
                style={{
                  border: "1px solid var(--border)",
                  borderRadius: 4,
                  padding: "6px 10px",
                  background: "var(--bg-hover)",
                  display: "flex",
                  gap: 8,
                  alignItems: "center",
                  justifyContent: "space-between",
                  fontSize: 12,
                }}
              >
                <span style={{ wordBreak: "break-all" }}>
                  {t("osBuilder.choice.notInList", { id })}
                </span>
                <button className="btn" onClick={() => toggleUpdate(id)}>
                  {t("osBuilder.choice.untick")}
                </button>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* **Group 3 — the first-boot tick** (design § 3.2). One tick, and the
          only one on this tab that is not per release: the block belongs to
          the tree this build produces.

          `wanted ?? true` — **absent means ticked**, because the first-boot
          block is what makes a PiStorm tree boot its own hardware
          (first-boot design § 3). Rendering it writes nothing: only
          `onChange` reaches `setFirstBoot`, so a user who never touches this
          row leaves no `wanted` in `settings.json` and ART cannot later
          mistake its own default for their decision. */}
      <div style={{ marginTop: 16 }}>
        <h3 style={{ fontSize: 14, margin: "0 0 8px" }}>{t("osBuilder.choice.firstboot")}</h3>
        <div
          style={{
            border: "1px solid var(--border)",
            borderRadius: 4,
            padding: "6px 10px",
            background: (session.firstboot.wanted ?? true) ? "var(--bg-hover)" : "var(--bg)",
          }}
        >
          <label
            data-testid="choice-firstboot"
            style={{ display: "flex", gap: 8, alignItems: "flex-start", fontSize: 13 }}
          >
            <input
              type="checkbox"
              checked={session.firstboot.wanted ?? true}
              onChange={(e) => setFirstBoot({ wanted: e.target.checked })}
            />
            <span>{t("osBuilder.choice.firstbootRow")}</span>
          </label>
          <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
            {t("osBuilder.choice.firstbootHint")}
          </p>
          {/* A fact about the folder, not about the tick — `written` and
              `wanted` are different questions, and saying the first where it
              is true makes a second write a decision rather than a surprise.
              The sentence is `FirstBootPanel`'s own, so the two screens
              cannot word one fact differently. */}
          {firstbootWritten && (
            <p
              data-testid="choice-firstboot-already-written"
              className="badge badge-warn"
              style={{ display: "block", padding: "4px 8px", fontSize: 11, margin: "6px 0 0" }}
            >
              {t("firstboot.panel.alreadyWritten")}
            </p>
          )}
        </div>
      </div>
      {/* ART-175, folded (design § 3.2). What the switched-on layering
          components would replace, before the build runs. The summary line
          **counts** — a fold labelled only "what this would replace" says
          nothing about whether opening it is worth the click, and "3" and
          "41" are different decisions.

          **What the guard does** (round 3 task 2's review, carried into task
          3). `reports.length > 0` means an empty report draws no fold at all:
          the fold's whole subject is files that would be *replaced*, and one
          labelled "would replace 0 files (open to see which)" is a control
          that opens onto an empty table. Nothing is claimed by its absence —
          how much this build places is the plan's own line on tab 1, which
          is where a user reads what will happen rather than what will be
          overwritten. The sentence **inside** the fold is the one that must
          not collapse (§89): once there is something to state, it states all
          three counts, because "landed on nothing", "landed on identical
          bytes" and "replaced something" are three different facts and a
          reader who is shown one of them infers the other two wrongly. */}
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
