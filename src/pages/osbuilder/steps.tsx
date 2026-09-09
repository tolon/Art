// The OS Builder's steps, as thin as they can be.
//
// A step's whole job is to mount the panel that already exists and feed it
// from the session. **No panel is rewritten here** — that is wave 2. What
// changes is where a panel's values come from: one session, so a value
// reaches the step that needs it without anyone remembering to wire it
// (ART-197).
//
// **The install lane is four numbered tabs** since the owner's 2026-09-09
// verdict on the five-step lane (four-tab design § 2): `dosyalar` · `secim` ·
// `makine` · `derle`. Round 1 only moved the panels; rounds 2-4 moved the
// fields, and since round 4 every one of them holds its own: `dosyalar` is
// the material, `secim` the one tick list, `makine` the Kickstart, the
// keyboard and the destination, and `derle` the summary, the Build button,
// the run's report and the hand-off. The retired `kaynak` · `paketler` ·
// `amiga-kurulum` · `ilk-acilis` are redirects in `routes.tsx`, not steps.
//
// A step opened on its own **asks** rather than rendering empty, and asking is
// a state rather than a refusal — the panel stays mounted and stays usable, so
// nothing here turns an optional step into a gate. The sentence names the step
// that answers the question, because a user told "no tree" and not told where
// a tree comes from has been given nothing.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useLocation } from "react-router-dom";

import { readiness } from "@/lib/buildSteps";
import { osinstallDescribeTree, rememberedComponentKey } from "@/lib/osinstall";
import { isTextOrNothing } from "@/lib/remembered";
import { useBuildSession } from "@/lib/useBuildSession";
import { useChainTree } from "@/lib/useChainTree";
import { useRemembered } from "@/lib/useRemembered";
import { AppearancePanel } from "@/components/osbuilder/AppearancePanel";
import { BuildTab } from "@/components/osbuilder/BuildTab";
import { CardBuilder } from "@/components/osbuilder/CardBuilder";
import { ChoiceTab } from "@/components/osbuilder/ChoiceTab";
import { FilesTab } from "@/components/osbuilder/FilesTab";
import { MachineTab } from "@/components/osbuilder/MachineTab";
import { NetworkPanel } from "@/components/osbuilder/NetworkPanel";
import { VerifyAgainstCard } from "@/components/osbuilder/VerifyAgainstCard";
import { VolumePreload } from "@/components/osbuilder/VolumePreload";

/**
 * What a step says when it has been opened without what it needs.
 *
 * **It names tab 3** (round 3 fix wave, Critical 1). The sentence used to
 * say "below", where `PackagePanel`'s own tree picker sat; that panel is
 * deleted and there is nothing below this banner to pick a tree with. A
 * refusal must be actionable, and where order matters it must name the
 * order — the destination is chosen on `makine`, and this says so.
 */
function Asks() {
  const { t } = useTranslation();
  return (
    <div
      className="badge badge-warn"
      data-testid="step-asks-tree"
      style={{ display: "block", padding: "8px 12px", marginBottom: 16, fontSize: 12 }}
    >
      {t("osBuilder.step.asksTree")}{" "}
      <Link to="/os-builder/makine">{t("osBuilder.step.makine")}</Link>
    </div>
  );
}

/**
 * What ART makes of the folder the session is pointing at (ART-199).
 *
 * `null` means "not asked yet, or ART could not look" — never "wrong". A
 * failed round trip must not turn into an accusation about the user's folder;
 * `readiness` treats `null` as ready and lets the engine's own refusal stand
 * as the last word, which is where it was before this existed.
 */
function useTreeCheck(root: string | null): boolean | null {
  const [isTree, setIsTree] = useState<boolean | null>(null);
  useEffect(() => {
    if (!root) {
      setIsTree(null);
      return;
    }
    let current = true;
    osinstallDescribeTree(root)
      .then((summary) => {
        if (current) setIsTree(summary.isTree);
      })
      .catch(() => {
        if (current) setIsTree(null);
      });
    return () => {
      current = false;
    };
  }, [root]);
  return isTree;
}

/** What a step says when the folder it was given is not a tree ART built. */
function WrongFolder() {
  const { t } = useTranslation();
  return (
    <div
      className="badge badge-err"
      data-testid="step-wrong-folder"
      style={{ display: "block", padding: "8px 12px", marginBottom: 16, fontSize: 12 }}
    >
      {t("osBuilder.step.notATree")}{" "}
      <Link to="/os-builder/makine">{t("osBuilder.step.makine")}</Link>
    </div>
  );
}

/**
 * Tab 1 — Amiga files: **the material and nothing else** since round 4 task 5
 * (four-tab design § 3.1). In round 1 this mounted the whole of the old source
 * step; `OsInstall.tsx` was renamed to `FilesTab.tsx` once every other section
 * of it had a tab of its own.
 *
 * A disc dropped on the drop panel arrives here through router state, carried
 * on by the shell. `arrivalKey` is `location.key` — unique per navigation — so
 * a second drop of the *same* file is still a distinct value the screen can
 * react to; the path string alone is value-equal and a dependency array would
 * treat it as no change.
 */
export function StepDosyalar() {
  const location = useLocation();
  const dropped = (location.state as { path?: string } | null)?.path ?? null;
  return (
    <FilesTab droppedMedia={dropped ? { path: dropped, arrivalKey: location.key } : null} />
  );
}

/**
 * Tab 2 — what to install: **one list and nothing else** (four-tab design
 * § 3.2), since round 3 task 3.
 *
 * The two panels that used to sit under it are gone from the wizard.
 * `PackagePanel` is deleted — its flat catalogue never joined the chain, so
 * one lane held two lists of one release's packages that could say different
 * things about one file — and `FirstBootPanel` leaves the lane with its own
 * card: the tick is here, and the *write* is round 4's run. The panel's file
 * stays in the tree until then.
 *
 * The readiness banner stays, and it still reads the tree rather than the
 * tab: the list's second group is answered *against* a tree when the
 * destination is one, so "you have not pointed this anywhere yet" is still
 * the sentence a cold step owes. Asking is a state, not a gate — the tab is
 * mounted and usable underneath it.
 *
 * **It judges `useChainTree`'s tree, not `session.tree.root`** (round 3 fix
 * wave, Critical 1). The banner was reading one folder while the list under
 * it worked on another: with the destination pointed at an ART tree and no
 * session tree, the tab answered every row against that tree and the banner
 * above them said no tree had been chosen. One tab, two answers — the shape
 * this round deleted a whole panel to be rid of.
 *
 * The `isTree` question is asked of **that** root rather than taken from the
 * hook's own check: the hook's check is about the *destination*, and when the
 * tree came from the session instead it says nothing about it. One extra
 * `describe_tree` for the destination case is the price of a banner that
 * cannot accuse the wrong folder.
 *
 * And **nothing is drawn until `settled`**. `isTree` and the destination
 * check both arrive by round trip, so for a render or two a destination that
 * is a build looks like no tree at all — long enough to flash *choose a
 * destination* over a list that is about to answer against one.
 */
export function StepSecim() {
  const { session } = useBuildSession();
  // The destination, read exactly as `ChoiceTab` and `MachineTab` read it —
  // the same per-release key, so a second one cannot invent a second folder.
  const [destination] = useRemembered<string | null>(
    rememberedComponentKey("osinstall.destination", session.release),
    isTextOrNothing,
    null
  );
  const { treeRoot, settled } = useChainTree(destination);
  const isTree = useTreeCheck(treeRoot);
  const state = readiness(session, "secim", isTree, treeRoot);
  return (
    <>
      {settled && state === "asks" && <Asks />}
      {settled && state === "wrong-folder" && <WrongFolder />}
      <ChoiceTab />
    </>
  );
}

/**
 * Tab 3 — Kickstart, keyboard layout and destination.
 *
 * **The keymap note is gone** (round 4 task 4): the select itself is on this
 * tab now, and a line pointing somewhere else for a field that is right there
 * is a sentence that has become false. Every tab of the install lane holds
 * its own fields since this round; `NotYet` went with it.
 */
export function StepMakine() {
  return <MachineTab />;
}

/** Tab 4 — build: the summary, the button, the run and the hand-off. */
export function StepDerle() {
  return <BuildTab />;
}

export function StepKart() {
  return <CardBuilder />;
}

/**
 * Preparing the volumes on a card — and then checking one.
 *
 * **ART-197 wave 3.** `VerifyAgainstCard` sat on the install step until
 * 2026-08-24, which is where it was built and not where it belongs: it
 * compares a distribution tree against **a volume that already exists**, so
 * everything it needs is here and nothing it needs is there. On the install
 * step it was a section asking for a card image on a screen whose whole job
 * is to produce a folder, which is the "sections that do not belong on this
 * screen" complaint in its own right.
 *
 * It carries its own tree across by reading `session.tree.root`, so a user who
 * built a tree on the install step and came here to write it finds the field
 * already pointing at it — the carry row 3 widened, doing the work that makes
 * this move cost nothing.
 */
export function StepBirimler() {
  return (
    <>
      <VolumePreload />
      {/* Task 10 of the prefs-and-wallpaper round. Same reasoning as
          `NetworkPanel` below: this is where somebody is setting a card up,
          and both panels write into the system volume the card will carry —
          `WBPattern.prefs`, `ScreenMode.prefs` and the shell defaults here,
          `Wireless.prefs`/`tolunnet.config` there. */}
      <AppearancePanel />
      {/* **SD-3 G14.** The owner's decision, 2026-08-24: ART asks, and the
          WiFi credentials are entered while the card is being set up. It goes
          here rather than on the install step because this is where somebody
          is setting a card up, and it writes into the system volume the card
          will carry. */}
      <NetworkPanel />
      <VerifyAgainstCard />
    </>
  );
}
