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
// `makine` · `derle`. Round 1 only moves the panels; rounds 2-4 move the
// fields. So `dosyalar` mounts today's install step whole, `secim` mounts the
// two panels that read the tree, `makine` holds the Kickstart and the
// destination since round 2, and `derle` still says in one sentence where its
// fields are today rather than rendering an empty card — a tab that showed
// nothing would hide features that work (CLAUDE.md: register unready, never
// hide). The retired `kaynak` · `paketler` · `amiga-kurulum` ·
// `ilk-acilis` are redirects in `routes.tsx`, not steps.
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
import { osinstallDescribeTree } from "@/lib/osinstall";
import { useBuildSession } from "@/lib/useBuildSession";
import { AppearancePanel } from "@/components/osbuilder/AppearancePanel";
import { CardBuilder } from "@/components/osbuilder/CardBuilder";
import { ChoiceTab } from "@/components/osbuilder/ChoiceTab";
import { FirstBootPanel } from "@/components/osbuilder/FirstBootPanel";
import { MachineTab } from "@/components/osbuilder/MachineTab";
import { OsInstall } from "@/components/osbuilder/OsInstall";
import { PackagePanel } from "@/components/osbuilder/PackagePanel";
import { NetworkPanel } from "@/components/osbuilder/NetworkPanel";
import { VerifyAgainstCard } from "@/components/osbuilder/VerifyAgainstCard";
import { VolumePreload } from "@/components/osbuilder/VolumePreload";

/** What a step says when it has been opened without what it needs. */
function Asks() {
  const { t } = useTranslation();
  return (
    <div
      className="badge badge-warn"
      style={{ display: "block", padding: "8px 12px", marginBottom: 16, fontSize: 12 }}
    >
      {t("osBuilder.step.asksTree")}{" "}
      <Link to="/os-builder/dosyalar">{t("osBuilder.step.dosyalar")}</Link>
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
      <Link to="/os-builder/dosyalar">{t("osBuilder.step.dosyalar")}</Link>
    </div>
  );
}

/**
 * Tab 1 — Amiga files. In round 1 this is the whole of today's source step.
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
    <OsInstall droppedMedia={dropped ? { path: dropped, arrivalKey: location.key } : null} />
  );
}

/**
 * Tab 2 — what to install. In round 1 it holds the two panels round 3 turns
 * into ticks: the package checklist and the first-boot block. Both read the
 * tree, so one readiness banner covers both.
 */
export function StepSecim() {
  const { session, setTree, setPackages } = useBuildSession();
  const isTree = useTreeCheck(session.tree.root);
  const state = readiness(session, "secim", isTree);
  return (
    <>
      {state === "asks" && <Asks />}
      {state === "wrong-folder" && <WrongFolder />}
      {/* The tab's own list (four-tab design § 3.2), first: the release's
          parts and what turning them on would replace. The two panels below
          it are the update and first-boot halves of the same question, still
          in their own cards — round 3 task 3 folds them into this list and
          removes them from here. */}
      <ChoiceTab />
      <PackagePanel
        treeRoot={session.tree.root}
        onTreeRootChange={(root) => setTree({ root, builtHere: false })}
        packageFolder={session.packages.folder}
        onPackageFolderChange={(folder) => setPackages({ folder })}
        chosen={session.packages.chosen}
        onChosenChange={(chosen) => setPackages({ chosen })}
        release={session.release}
      />
      <FirstBootPanel
        treeRoot={session.tree.root}
        onTreeRootChange={(root) => setTree({ root, builtHere: false })}
      />
    </>
  );
}

/**
 * A tab whose fields still live on the files tab — says so, links there.
 *
 * The sentence arrives **already rendered**, from a literal key at the call
 * site rather than one composed here from `id`: `literal-keys.test.ts`
 * checks that every literal key resolves in both catalogues, and a composed
 * key would buy a dynamic call site for nothing — nothing decides the key,
 * it is simply that tab's own.
 *
 * `derle` is the only tab left with nothing of its own since round 2 gave
 * `makine` its two fields; the `id` type says so rather than leaving a
 * second caller nobody would notice had gone.
 */
function NotYet({
  id,
  heading,
  sentence,
}: {
  id: "derle";
  heading: string;
  sentence: string;
}) {
  const { t } = useTranslation();
  return (
    <section className="card" data-testid={`tab-${id}`} style={{ marginBottom: 16 }}>
      <h2 style={{ fontSize: 16, marginTop: 0 }}>{heading}</h2>
      <p className="muted" style={{ fontSize: 12, margin: 0 }}>
        {sentence} <Link to="/os-builder/dosyalar">{t("osBuilder.step.dosyalar")}</Link>
      </p>
    </section>
  );
}

/**
 * Tab 3 — Kickstart and destination. The keymap joins in round 4 with the
 * plan: its option list is `keymapsIn(effectivePlan)`, and the plan is
 * round 4's hook — moving the select alone would mean computing a second
 * plan here. The sentence says so rather than leaving a user hunting.
 */
export function StepMakine() {
  const { t } = useTranslation();
  return (
    <>
      <MachineTab />
      <p
        className="faint"
        style={{ fontSize: 11, margin: "0 0 16px" }}
        data-testid="tab-makine-keymap-note"
      >
        {t("osBuilder.tab.makineKeymapNote")}{" "}
        <Link to="/os-builder/dosyalar">{t("osBuilder.step.dosyalar")}</Link>
      </p>
    </>
  );
}

/** Tab 4 — build. Round 4 moves the plan, the button and the run here. */
export function StepDerle() {
  const { t } = useTranslation();
  return (
    <NotYet
      id="derle"
      heading={t("osBuilder.step.derle")}
      sentence={t("osBuilder.tab.derleNotYet")}
    />
  );
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
