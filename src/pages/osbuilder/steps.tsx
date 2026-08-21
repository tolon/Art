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

import { useTranslation } from "react-i18next";
import { Link, useLocation } from "react-router-dom";

import { readiness } from "@/lib/buildSteps";
import { useBuildSession } from "@/lib/useBuildSession";
import { AmigaInstallPanel } from "@/components/osbuilder/AmigaInstallPanel";
import { CardBuilder } from "@/components/osbuilder/CardBuilder";
import { OsInstall } from "@/components/osbuilder/OsInstall";
import { PackagePanel } from "@/components/osbuilder/PackagePanel";
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
  return (
    <>
      {readiness(session, "paketler") === "asks" && <Asks />}
      <PackagePanel
        treeRoot={session.tree.root}
        onTreeRootChange={(root) => setTree({ root, builtHere: false })}
        packageFolder={session.packages.folder}
        onPackageFolderChange={(folder) => setPackages({ folder })}
        chosen={session.packages.chosen}
        onChosenChange={(chosen) => setPackages({ chosen })}
      />
    </>
  );
}

export function StepAmigaKurulum() {
  const { session, setTree } = useBuildSession();
  return (
    <>
      {readiness(session, "amiga-kurulum") === "asks" && <Asks />}
      <AmigaInstallPanel
        treeRoot={session.tree.root}
        onTreeRootChange={(root) => setTree({ root, builtHere: false })}
        packageFolder={session.packages.folder}
      />
    </>
  );
}

export function StepKart() {
  return <CardBuilder />;
}

export function StepBirimler() {
  return <VolumePreload />;
}
