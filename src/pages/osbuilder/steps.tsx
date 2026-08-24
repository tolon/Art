// The OS Builder's steps, as thin as they can be.
//
// A step's whole job is to mount the panel that already exists and feed it
// from the session. **No panel is rewritten here** — that is wave 2. What
// changes is where a panel's values come from: one session, so a value
// reaches the step that needs it without anyone remembering to wire it
// (ART-197).
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
import { AmigaInstallPanel } from "@/components/osbuilder/AmigaInstallPanel";
import { CardBuilder } from "@/components/osbuilder/CardBuilder";
import { OsInstall } from "@/components/osbuilder/OsInstall";
import { PackagePanel } from "@/components/osbuilder/PackagePanel";
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
      <Link to="/os-builder/kaynak">{t("osBuilder.step.kaynak")}</Link>
    </div>
  );
}

/**
 * Building the tree.
 *
 * A disc dropped on the drop panel arrives here through router state, carried
 * on by the shell. `arrivalKey` is `location.key` — unique per navigation — so
 * a second drop of the *same* file is still a distinct value the screen can
 * react to; the path string alone is value-equal and a dependency array would
 * treat it as no change.
 */
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
      <Link to="/os-builder/kaynak">{t("osBuilder.step.kaynak")}</Link>
    </div>
  );
}

export function StepKaynak() {
  const location = useLocation();
  const dropped = (location.state as { path?: string } | null)?.path ?? null;
  return (
    <OsInstall
      droppedMedia={dropped ? { path: dropped, arrivalKey: location.key } : null}
    />
  );
}

export function StepPaketler() {
  const { session, setTree, setPackages } = useBuildSession();
  const isTree = useTreeCheck(session.tree.root);
  const state = readiness(session, "paketler", isTree);
  return (
    <>
      {state === "asks" && <Asks />}
      {state === "wrong-folder" && <WrongFolder />}
      <PackagePanel
        treeRoot={session.tree.root}
        onTreeRootChange={(root) => setTree({ root, builtHere: false })}
        packageFolder={session.packages.folder}
        onPackageFolderChange={(folder) => setPackages({ folder })}
        chosen={session.packages.chosen}
        onChosenChange={(chosen) => setPackages({ chosen })}
        release={session.release}
      />
    </>
  );
}

export function StepAmigaKurulum() {
  const { session, setTree } = useBuildSession();
  const isTree = useTreeCheck(session.tree.root);
  const state = readiness(session, "amiga-kurulum", isTree);
  return (
    <>
      {state === "asks" && <Asks />}
      {state === "wrong-folder" && <WrongFolder />}
      <AmigaInstallPanel
        treeRoot={session.tree.root}
        onTreeRootChange={(root) => setTree({ root, builtHere: false })}
        packageFolder={session.packages.folder}
        release={session.release}
      />
    </>
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
      <VerifyAgainstCard />
    </>
  );
}
