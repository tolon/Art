// Tab 3 — Kickstart and destination (four-tab design § 3.3).
//
// Two fields, moved here from the source step on 2026-09-09. Each reads the
// value the rest of the build reads — `session.rom` (the machine's one
// Kickstart, ART-207's decision) and the remembered destination for this
// release — and each shows the core's own sentence under it: the ROM's
// identity or its unreadability (ART-241 wires the sentence to the Browse
// button), the destination's occupied-refusal, and what ART found there.
//
// **The keymap is not here yet.** Its option list comes from the plan, and
// the plan is round 4's hook; `StepMakine` says so in one line.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";

import { Field } from "@/components/osbuilder/Field";
import { hostAmigaForeverFolders } from "@/lib/api";
import { rememberedComponentKey } from "@/lib/osinstall";
import { isTextOrNothing } from "@/lib/remembered";
import { useBuildSession } from "@/lib/useBuildSession";
import { useDestinationCheck } from "@/lib/useDestinationCheck";
import { useRemembered } from "@/lib/useRemembered";
import { useRomIdentity } from "@/lib/useRomIdentity";

export function MachineTab() {
  const { t } = useTranslation();
  const { session, setRom } = useBuildSession();
  const release = session.release;
  const romPath = session.rom.path;
  const { rom, unreadable: romError } = useRomIdentity(romPath);

  /**
   * Where the tree goes — per release, and through the very key the install
   * step reads (`OsInstall.tsx`'s own `useRemembered` call): a second key
   * would mean a destination chosen here and a build written somewhere else.
   */
  const [destination, setDestination] = useRemembered<string | null>(
    rememberedComponentKey("osinstall.destination", release),
    isTextOrNothing,
    null
  );
  const { taken, tree } = useDestinationCheck(destination);

  // Amiga Forever's ROM folder, offered — never chosen for the user. Its
  // own dismissal, a `useState`: a suggestion is not a setting, and writing
  // "they said no once" to disk would store a choice about every future
  // build from one click.
  const [amigaForeverRom, setAmigaForeverRom] = useState<string | null>(null);
  const [romOfferDismissed, setRomOfferDismissed] = useState(false);
  useEffect(() => {
    let current = true;
    hostAmigaForeverFolders()
      .then((found) => {
        if (current) setAmigaForeverRom(found.rom);
      })
      // A host that cannot answer is a host with nothing to suggest.
      .catch(() => {
        if (current) setAmigaForeverRom(null);
      });
    return () => {
      current = false;
    };
  }, []);
  const amigaForeverRomOffer = !romPath && !romOfferDismissed ? amigaForeverRom : null;

  /** The Kickstart picker, optionally opened on a folder ART already knows
   *  about (Amiga Forever's `Shared\rom`). The **user** still picks the
   *  file; ART only says where to look. */
  async function chooseRomIn(defaultPath?: string) {
    const picked = await open({
      multiple: false,
      title: t("osinstall.rom.chooseTitle"),
      defaultPath,
      filters: [{ name: "Kickstart ROM", extensions: ["rom", "bin"] }],
    });
    if (typeof picked === "string") {
      setRom(picked);
      setRomOfferDismissed(true);
    }
  }

  async function chooseDestination() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: t("osinstall.destination.chooseTitle"),
    });
    if (typeof picked === "string") setDestination(picked);
  }

  return (
    <section className="card" data-testid="machine-tab" style={{ marginBottom: 16 }}>
      <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osBuilder.step.makine")}</h2>

      <Field
        label={t("osinstall.rom.label")}
        value={romPath}
        empty={t("osinstall.rom.none")}
        onChoose={() => void chooseRomIn(undefined)}
        choose={t("common.browse")}
        hint={t("osinstall.rom.hint")}
        testId="osinstall-rom-field"
        // ART-241: the Browse button is described by whichever of the two
        // paragraphs below actually renders — `romError` and `rom` are
        // mutually exclusive (a ROM is either unreadable or identified,
        // never both), so exactly one id or none applies.
        describedBy={
          romError ? "osinstall-rom-unreadable" : rom ? "osinstall-rom-identified" : undefined
        }
      />
      {amigaForeverRomOffer && (
        <p
          className="faint"
          data-testid="amiga-forever-rom-offer"
          style={{
            fontSize: 11,
            margin: "0 0 12px",
            display: "flex",
            gap: 8,
            alignItems: "center",
            flexWrap: "wrap",
          }}
        >
          <span>{t("osinstall.material.amigaForeverRom", { path: amigaForeverRomOffer })}</span>
          <button
            className="btn"
            style={{ fontSize: 11 }}
            onClick={() => void chooseRomIn(amigaForeverRomOffer)}
          >
            {t("osinstall.material.amigaForeverAdd")}
          </button>
          <button
            className="btn"
            style={{ fontSize: 11 }}
            onClick={() => setRomOfferDismissed(true)}
          >
            {t("osinstall.material.amigaForeverDismiss")}
          </button>
        </p>
      )}
      {romError && (
        <p
          id="osinstall-rom-unreadable"
          className="badge badge-err"
          style={{ fontSize: 11, margin: "0 0 12px", display: "inline-block" }}
        >
          {t("osinstall.rom.unreadable")}
        </p>
      )}
      {rom && (
        <p id="osinstall-rom-identified" className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
          {t("osinstall.rom.identified", { rom: rom.name })}
        </p>
      )}

      <Field
        label={t("osinstall.destination.label")}
        value={destination}
        empty={t("osinstall.destination.none")}
        onChoose={() => void chooseDestination()}
        choose={t("common.browse")}
        hint={t("osinstall.destination.hint")}
        testId="osinstall-destination-field"
        // `tree` is null whenever the path is — `useDestinationCheck`'s own
        // rule — so the fresh id is never named for an unrendered paragraph.
        describedBy={
          taken
            ? "osinstall-destination-taken"
            : tree
              ? tree.isTree
                ? "osinstall-destination-tree"
                : "osinstall-destination-fresh"
              : undefined
        }
      />
      {/* Two sentences, never one: the refusal is what `apply` would say;
          the tree line is what is there. Both can be true of one folder,
          and until round 4 decides update mode the refusal is the last word. */}
      {taken && (
        <p
          id="osinstall-destination-taken"
          data-testid="osinstall-destination-taken"
          className="badge badge-err"
          style={{ fontSize: 11, margin: "0 0 6px", display: "inline-block" }}
        >
          {t("osinstall.destination.taken")}
        </p>
      )}
      {tree?.isTree && (
        <p
          id="osinstall-destination-tree"
          data-testid="osinstall-destination-tree"
          className="faint"
          style={{ fontSize: 11, margin: "0 0 12px" }}
        >
          {t("osinstall.destination.tree", { release: tree.release ?? "", count: tree.files })}
        </p>
      )}
      {destination && !taken && tree && !tree.isTree && (
        <p
          id="osinstall-destination-fresh"
          data-testid="osinstall-destination-fresh"
          className="faint"
          style={{ fontSize: 11, margin: "0 0 12px" }}
        >
          {t("osinstall.destination.fresh")}
        </p>
      )}
    </section>
  );
}
