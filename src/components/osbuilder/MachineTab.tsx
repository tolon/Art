// Tab 3 — Kickstart and destination (four-tab design § 3.3).
//
// Two fields, moved here from the source step on 2026-09-09. Each reads the
// value the rest of the build reads — `session.rom` (the machine's one
// Kickstart, ART-207's decision) and the remembered destination for this
// release — and each shows the core's own sentence under it: the ROM's
// identity or its unreadability (ART-241 wires the sentence to the Browse
// button), the destination's occupied-refusal, and what ART found there.
//
// **The keymap joined in round 4** (task 4). Its option list is the plan's
// own items — `keymapsIn(effectivePlan)` — so a layout offered here cannot
// then be refused for not being there, which means this tab computes a plan.
// That makes it the fourth consumer of `useInstallPlan` (tabs 1, 2 and 4 are
// the others) and so the fourth `osinstall_plan` per path; the tabs of the
// lane are mounted **one at a time**, so at most one of them is asking at
// once, and the alternative — a second, cheaper source for the keymap list —
// is a second answer to what this build will place, which is the defect this
// whole round exists to remove.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";

import { Field } from "@/components/osbuilder/Field";
import { hostAmigaForeverFolders } from "@/lib/api";
import { keymapsIn, rememberedComponentKey } from "@/lib/osinstall";
import { isFlag, isText, isTextOrNothing } from "@/lib/remembered";
import { useBuildSession } from "@/lib/useBuildSession";
import { useDestinationCheck } from "@/lib/useDestinationCheck";
import { useInstallPlan } from "@/lib/useInstallPlan";
import { useRemembered } from "@/lib/useRemembered";
import { useRomIdentity } from "@/lib/useRomIdentity";

export function MachineTab() {
  const { t } = useTranslation();
  const { session, setRom, setComponents } = useBuildSession();
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

  /**
   * **The keyboard the finished system boots with** (ART-226's other half).
   *
   * The `keymaps` component places every layout the media carries; until this
   * existed nothing selected one, so a Turkish tree rendered `ç ü ş Ğ` in its
   * menus and still typed on an American keyboard.
   *
   * Per release, like the destination above (ART-207): a layout is a name in
   * *that* release's `Devs/Keymaps`. Empty means the ROM's `usa`. **No
   * default** — choosing somebody's keyboard for them is not ART's to do.
   */
  const [keymap, setKeymap] = useRemembered<string>(
    rememberedComponentKey("osinstall.keymap", release),
    isText,
    ""
  );
  const [reuseScan] = useRemembered<boolean>("osinstall.reuseScan", isFlag, true);

  // The plan, with the very inputs tabs 1, 2 and 4 pass — the same remembered
  // keys through the same guards, so no two tabs can be planning two builds.
  // Nothing on this tab bumps `rescanNonce`: the *Scan again* button is tab
  // 1's.
  const { effectivePlan } = useInstallPlan({
    release,
    material: session.material,
    keymap,
    rom: romPath,
    destination,
    reuseScan,
    rescanNonce: 0,
    components: session.components,
    setComponents,
  });

  /**
   * What the plan would really put in `Devs/Keymaps` — the picker's options,
   * so nothing offered can be refused for not being there.
   *
   * **Held across a re-plan**, and that is not a nicety. `effectivePlan` goes
   * `null` while a new plan is in the air, and reading the list straight off
   * it made the whole section vanish and come back on every keystroke
   * elsewhere on the screen — including on the user's own choice of keyboard,
   * which unmounted the control they had just used.
   *
   * A plan that **has** arrived always supersedes, empty included: turning the
   * `keymaps` component off has to empty the list, not leave a stale one.
   */
  const [availableKeymaps, setAvailableKeymaps] = useState<string[]>([]);
  useEffect(() => {
    if (effectivePlan) setAvailableKeymaps(keymapsIn(effectivePlan));
  }, [effectivePlan]);

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

      {/* **ART-226's other half: choose the keyboard, having placed it.**
          Moved here verbatim from `OsInstall.tsx` in round 4 task 4. The
          options are read off the plan's own items, so a layout offered here
          cannot then be refused for not being there. Nothing selected leaves
          the system on the ROM's `usa`, which is what every ART tree did
          until ART-226; a default would be ART choosing somebody's keyboard.
          The section is absent — rather than empty — when the build places no
          keymap at all, because there is nothing to choose between. */}
      {availableKeymaps.length > 0 && (
        <div data-testid="keymap-section" style={{ marginTop: 12 }}>
          <h3 style={{ fontSize: 14, margin: "0 0 4px" }}>{t("osinstall.keymap.heading")}</h3>
          <p className="muted" style={{ fontSize: 12, margin: "4px 0 8px" }}>
            {t("osinstall.keymap.intro", { count: availableKeymaps.length })}
          </p>
          <label style={{ display: "flex", flexDirection: "column", gap: 4, maxWidth: "24em" }}>
            <span className="muted" style={{ fontSize: 12 }}>
              {t("osinstall.keymap.label")}
            </span>
            <select
              className="input"
              value={keymap}
              onChange={(e) => setKeymap(e.target.value)}
              aria-label={t("osinstall.keymap.label")}
            >
              <option value="">{t("osinstall.keymap.rom")}</option>
              {availableKeymaps.map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
            </select>
            <span className="faint" style={{ fontSize: 10 }}>
              {t("osinstall.keymap.hint")}
            </span>
          </label>
        </div>
      )}
    </section>
  );
}
