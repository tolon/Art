// OS Builder — the wizard shell, and its first step.
//
// **What changed, and why.** This screen used to put every section of every
// build job in one scrolling column: ten `<h2>`s under a four-way picker, with
// the panels stacked beneath them. The owner drove the release build and said
// two things — *"bu işletim sistemi kurucusunda akış çok karmaşık gereksiz
// derecede uzun"* and, the sentence that mattered, *"dağıtım ağacı için nereyi
// seçmeliyim anlamadım ben."* The second is a person who has read every
// document in this repository being unable to answer a field on its own
// screen.
//
// So the sections became **steps on their own routes** (`@/lib/buildSteps`),
// one question at a time, with a strip above showing only the steps the chosen
// kind actually has. `OsBuilder` is now the shell: the strip, and an `Outlet`.
// The steps themselves are `@/pages/osbuilder/steps`.
//
// **The distro job stays here**, in `StepHedef`, because it is the one kind
// with no second step: every profile is registered `available: false` and
// rendered as Coming Later — declared, not pretended (§96).
//
// **ART never downloads a distro image** (§2). There is no fetch button here
// and there will not be one; the link goes to the project's own page and the
// user comes back with a file. The same rule ART already applies to Kickstart
// ROMs. And ART's output is an **image file**, never a physical card
// (`docs/owner-checklist.md` § 4).

import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import { Link, Outlet, useLocation, useNavigate } from "react-router-dom";

import {
  distroCheckCard,
  distroMeasureImage,
  distroProfiles,
  distroRomFamilyMatches,
  type CardProblem,
  type DistroProfile,
  type SuppliedImage,
} from "@/lib/distro";
import {
  canBuild,
  imageProblem,
  licenceSentence,
  minCardBytes,
  whatYouSupply,
} from "@/lib/osBuilder";
import { pistormIdentifyRom, type RomInfo } from "@/lib/pistorm";
import { isTextOrNothing, isWholeNumberBetween } from "@/lib/remembered";
import { useRemembered } from "@/lib/useRemembered";
import { useBuildSession } from "@/lib/useBuildSession";
import { stepLabelKey, stepsFor, type StepId } from "@/lib/buildSteps";
import type { BuildKind } from "@/lib/buildSession";
import { errorText } from "@/lib/errorText";

/** Card sizes people actually buy. Typed sizes are allowed too. */
const CARD_SIZES_GB = [16, 32, 64, 128, 256];

/** Where a step lives. One place, so a link and a route cannot drift apart. */
function stepPath(step: StepId): string {
  return `/os-builder/${step}`;
}

/**
 * The shell: what is being built, how far along it is, and the step itself.
 *
 * The strip is rendered from `stepsFor(session.kind)`, so a card build is
 * never offered the install's steps and the reverse. That is the owner's
 * fourth complaint — sections that do not belong on this screen — answered
 * structurally rather than by hiding things.
 */
export function OsBuilder() {
  const { t } = useTranslation();
  const { session, setKind } = useBuildSession();
  const location = useLocation();
  const navigate = useNavigate();

  const steps = stepsFor(session.kind);

  // A disc dropped on the drop panel routes here (`os.install-from-disc`)
  // carrying the file in router state. Under sub-routes the shell has to
  // carry it **on** to the step that acts on it, or the workflow reaches this
  // screen and does nothing visible.
  //
  // Keyed on `location.state` itself — not on the path derived from it — the
  // way AdfBrowser does it: React Router hands out a fresh state object on
  // every navigation, even a second drop of the exact same file, so depending
  // on the derived *string* instead would silently swallow a repeat arrival.
  //
  // The pathname guard is what stops this looping: the shell renders on the
  // child route too, so without it every navigation would see the state again
  // and navigate again, for ever.
  useEffect(() => {
    const state = location.state as { path?: string } | null;
    if (!state?.path) return;
    setKind("install");
    if (location.pathname !== stepPath("kaynak")) {
      navigate(stepPath("kaynak"), { state, replace: true });
    }
  }, [location.state, location.key, location.pathname, setKind, navigate]);

  return (
    <div>
      <h1 style={{ fontSize: 20 }}>{t("nav.osBuilder")}</h1>
      <p className="muted" style={{ fontSize: 12, margin: "4px 0 16px" }}>
        {t("osBuilder.intro")}
      </p>

      <nav
        style={{
          display: "flex",
          gap: 8,
          flexWrap: "wrap",
          marginBottom: 16,
          paddingBottom: 12,
          borderBottom: "1px solid var(--border)",
        }}
      >
        {steps.map((step, at) => {
          const here = location.pathname === stepPath(step);
          return (
            <Link
              key={step}
              to={stepPath(step)}
              className="btn"
              style={{
                fontSize: 12,
                textDecoration: "none",
                border: here ? "1px solid var(--accent)" : "1px solid var(--border)",
                background: here ? "var(--bg-hover)" : "var(--bg)",
              }}
            >
              {at + 1}. {t(stepLabelKey(step))}
            </Link>
          );
        })}
      </nav>

      <Outlet />
    </div>
  );
}

/**
 * Step 1 — what are we building.
 *
 * Choosing a kind moves straight on to that kind's own next step, when it has
 * one. `distro` does not, so its own material stays on this step rather than
 * being routed to a page that would have nothing to show.
 */
export function StepHedef() {
  const { t } = useTranslation();
  const { session, setKind } = useBuildSession();
  const navigate = useNavigate();

  const kind = session.kind;

  function choose(next: BuildKind) {
    setKind(next);
    const after = stepsFor(next)[1];
    if (after) navigate(stepPath(after));
  }

  const [profiles, setProfiles] = useState<DistroProfile[]>([]);
  const [selectedId, setSelectedId] = useRemembered<string | null>(
    "osBuilder.profile",
    isTextOrNothing,
    null
  );
  const [cardGb, setCardGb] = useRemembered(
    "osBuilder.cardGb",
    isWholeNumberBetween(1, 2048),
    32
  );
  const [imagePath, setImagePath] = useRemembered<string | null>(
    "osBuilder.imagePath",
    isTextOrNothing,
    null
  );
  const [romPath, setRomPath] = useRemembered<string | null>(
    "osBuilder.romPath",
    isTextOrNothing,
    null
  );

  const [image, setImage] = useState<SuppliedImage | null>(null);
  const [rom, setRom] = useState<RomInfo | null>(null);
  const [romMatches, setRomMatches] = useState<boolean | null>(null);
  const [cardProblem, setCardProblem] = useState<CardProblem | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (kind !== "distro") return;
    distroProfiles().then(setProfiles).catch((e) => setError(errorText(t, e)));
  }, [kind]);

  const selected = profiles.find((entry) => entry.id === selectedId) ?? null;

  // Re-measure whatever was remembered, so a file since deleted is noticed
  // rather than shown as still there.
  useEffect(() => {
    if (!imagePath) {
      setImage(null);
      return;
    }
    distroMeasureImage(imagePath)
      .then(setImage)
      .catch(() => setImage(null));
  }, [imagePath]);

  useEffect(() => {
    if (!romPath) {
      setRom(null);
      return;
    }
    pistormIdentifyRom(romPath)
      .then(setRom)
      .catch(() => setRom(null));
  }, [romPath]);

  useEffect(() => {
    if (!selected || !rom) {
      setRomMatches(null);
      return;
    }
    distroRomFamilyMatches(selected.id, rom.version)
      .then(setRomMatches)
      .catch(() => setRomMatches(null));
  }, [selected, rom]);

  useEffect(() => {
    if (!selected) {
      setCardProblem(null);
      return;
    }
    distroCheckCard(selected.id, cardGb * 1024 * 1024 * 1024)
      .then(setCardProblem)
      .catch(() => setCardProblem(null));
  }, [selected, cardGb]);

  async function chooseImage() {
    const picked = await open({
      multiple: false,
      title: t("osBuilder.material.chooseImageTitle"),
      filters: [{ name: "Disk image", extensions: ["img", "7z", "zip", "gz", "xz"] }],
    });
    if (typeof picked === "string") setImagePath(picked);
  }

  async function chooseRom() {
    const picked = await open({
      multiple: false,
      title: t("osBuilder.material.chooseRomTitle"),
      filters: [{ name: "Kickstart ROM", extensions: ["rom", "bin"] }],
    });
    if (typeof picked === "string") setRomPath(picked);
  }

  const problem = selected ? imageProblem(selected, image) : null;

  return (
    <div>
      {error && (
        <div className="badge badge-err" style={{ margin: "12px 0", padding: "6px 12px" }}>
          {error}
        </div>
      )}

      {/* Which of the jobs this screen does. The working ones are first,
          because they are the ones that produce a file. */}
      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osBuilder.what.heading")}</h2>
        <div style={{ display: "flex", flexDirection: "column", gap: 8, marginTop: 8 }}>
          <KindChoice
            chosen={kind === "boot-card"}
            onChoose={() => choose("boot-card")}
            title={t("osBuilder.what.bootCard")}
            hint={t("osBuilder.what.bootCardHint")}
          />
          <KindChoice
            chosen={kind === "install"}
            onChoose={() => choose("install")}
            title={t("osBuilder.what.install")}
            hint={t("osBuilder.what.installHint")}
          />
          <KindChoice
            chosen={kind === "prepare-volumes"}
            onChoose={() => choose("prepare-volumes")}
            title={t("osBuilder.what.prepareVolumes")}
            hint={t("osBuilder.what.prepareVolumesHint")}
          />
          <KindChoice
            chosen={kind === "distro"}
            onChoose={() => choose("distro")}
            title={t("osBuilder.what.distro")}
            hint={t("osBuilder.what.distroHint")}
            comingLater={t("common.comingLater")}
          />
        </div>
      </section>

      {kind === "distro" && (
        <>
          {/* The state of the whole job, said once and at the top rather than
              discovered at the bottom. */}
          <div
            className="badge badge-warn"
            style={{ display: "block", padding: "8px 12px", marginBottom: 16, fontSize: 12 }}
          >
            {t("osBuilder.notBuiltYet")}
          </div>

          <section className="card" style={{ marginBottom: 16 }}>
            <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osBuilder.profiles.heading")}</h2>
            <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
              {t("osBuilder.profiles.intro")}
            </p>

            <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
              {profiles.map((entry) => {
                const chosen = entry.id === selectedId;
                const licence = licenceSentence(entry);
                return (
                  <div
                    key={entry.id}
                    onClick={() => setSelectedId(entry.id)}
                    style={{
                      padding: "10px 12px",
                      borderRadius: 4,
                      border: chosen ? "1px solid var(--accent)" : "1px solid var(--border)",
                      background: chosen ? "var(--bg-hover)" : "var(--bg)",
                      cursor: "pointer",
                    }}
                  >
                    <div style={{ display: "flex", justifyContent: "space-between", gap: 8 }}>
                      <strong style={{ fontSize: 13 }}>{entry.name}</strong>
                      {!canBuild(entry) && (
                        <span className="badge badge-muted" style={{ fontSize: 10 }}>
                          {t("common.comingLater")}
                        </span>
                      )}
                    </div>
                    <p className="muted" style={{ margin: "4px 0 0", fontSize: 11 }}>
                      {t(licence.key, licence.params)}
                    </p>
                    <ul
                      className="faint"
                      style={{ fontSize: 11, margin: "6px 0 0", paddingLeft: 18 }}
                    >
                      {whatYouSupply(entry).map((phrase) => (
                        <li key={phrase.key}>{t(phrase.key, phrase.params)}</li>
                      ))}
                    </ul>
                  </div>
                );
              })}
            </div>
          </section>

          {selected && (
            <>
              <section className="card" style={{ marginBottom: 16 }}>
                <h2 style={{ fontSize: 16, marginTop: 0 }}>
                  {t("osBuilder.material.heading")}
                </h2>
                <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
                  {t("osBuilder.material.intro", { name: selected.name })}
                </p>

                <p style={{ fontSize: 12, margin: "0 0 12px" }}>
                  {t("osBuilder.material.homepage")}{" "}
                  <code style={{ fontSize: 11 }}>{selected.homepage}</code>
                </p>

                {selected.acquisition === "user-supplies-image" && (
                  <div style={{ marginBottom: 12 }}>
                    <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                      <button className="btn" onClick={() => void chooseImage()}>
                        {t("osBuilder.material.chooseImage")}
                      </button>
                      <span style={{ fontSize: 12, wordBreak: "break-all" }}>
                        {imagePath ?? t("osBuilder.material.noImage")}
                      </span>
                    </div>
                    {image?.is_file && (
                      <p className="faint" style={{ fontSize: 11, margin: "6px 0 0" }}>
                        {t("osBuilder.material.imageSize", {
                          gb:
                            Math.round((image.size_bytes / (1024 * 1024 * 1024)) * 10) / 10,
                        })}
                      </p>
                    )}
                    {problem && (
                      <p
                        className="badge badge-warn"
                        style={{ fontSize: 11, margin: "6px 0 0", display: "inline-block" }}
                      >
                        {t(problem.key, problem.params)}
                      </p>
                    )}
                  </div>
                )}

                {selected.rom_requirement && (
                  <div>
                    <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                      <button className="btn" onClick={() => void chooseRom()}>
                        {t("osBuilder.material.chooseRom")}
                      </button>
                      <span style={{ fontSize: 12, wordBreak: "break-all" }}>
                        {romPath ?? t("osBuilder.material.noRom")}
                      </span>
                    </div>
                    {rom && (
                      <p className="faint" style={{ fontSize: 11, margin: "6px 0 0" }}>
                        {rom.version === "Custom"
                          ? t("osBuilder.material.romUnrecognised")
                          : t("osBuilder.material.romIs", {
                              rom: rom.name,
                              revision: rom.revision,
                            })}
                      </p>
                    )}
                    {romMatches === false && (
                      <p
                        className="badge badge-warn"
                        style={{ fontSize: 11, margin: "6px 0 0", display: "inline-block" }}
                      >
                        {t("osBuilder.material.romWrongFamily", {
                          family: selected.rom_requirement.family,
                          base: selected.name,
                        })}
                      </p>
                    )}
                  </div>
                )}
              </section>

              <section className="card" style={{ marginBottom: 16 }}>
                <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osBuilder.card.heading")}</h2>
                <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                  <span className="muted" style={{ fontSize: 12 }}>
                    {t("osBuilder.card.size")}
                  </span>
                  <select
                    className="btn"
                    value={cardGb}
                    onChange={(e) => setCardGb(Number(e.target.value))}
                    style={{ maxWidth: "12em" }}
                  >
                    {CARD_SIZES_GB.map((gb) => (
                      <option key={gb} value={gb}>
                        {gb} GB
                      </option>
                    ))}
                  </select>
                </label>
                {cardProblem && (
                  <p
                    className="badge badge-warn"
                    style={{ fontSize: 11, margin: "8px 0 0", display: "inline-block" }}
                  >
                    {t("osBuilder.card.tooSmall", {
                      needs: cardProblem.needs_gb,
                      has: cardProblem.has_gb,
                    })}
                  </p>
                )}
                <p className="faint" style={{ fontSize: 11, margin: "8px 0 0" }}>
                  {t("osBuilder.card.hint", { bytes: minCardBytes(selected) })}
                </p>
              </section>

              {selected.post_install_notes.length > 0 && (
                <section className="card" style={{ marginBottom: 16 }}>
                  <h2 style={{ fontSize: 16, marginTop: 0 }}>
                    {t("osBuilder.notes.heading")}
                  </h2>
                  <ul className="muted" style={{ fontSize: 12, margin: 0, paddingLeft: 18 }}>
                    {selected.post_install_notes.map((key) => (
                      <li key={key}>{t(key)}</li>
                    ))}
                  </ul>
                </section>
              )}
            </>
          )}

          {/* Steps 4–6 of §5, declared rather than pretended. */}
          <section className="card" style={{ marginBottom: 16, opacity: 0.75 }}>
            <h2 style={{ fontSize: 16, marginTop: 0 }}>
              {t("osBuilder.prepare.heading")}{" "}
              <span className="badge badge-muted" style={{ fontSize: 10 }}>
                {t("common.comingLater")}
              </span>
            </h2>
            <p className="muted" style={{ fontSize: 12, margin: "4px 0 0" }}>
              {t("osBuilder.prepare.explain")}
            </p>
          </section>
        </>
      )}
    </div>
  );
}

function KindChoice({
  chosen,
  onChoose,
  title,
  hint,
  comingLater,
}: {
  chosen: boolean;
  onChoose: () => void;
  title: string;
  hint: string;
  comingLater?: string;
}) {
  return (
    <div
      onClick={onChoose}
      style={{
        padding: "10px 12px",
        borderRadius: 4,
        border: chosen ? "1px solid var(--accent)" : "1px solid var(--border)",
        background: chosen ? "var(--bg-hover)" : "var(--bg)",
        cursor: "pointer",
      }}
    >
      <div style={{ display: "flex", justifyContent: "space-between", gap: 8 }}>
        <strong style={{ fontSize: 13 }}>{title}</strong>
        {comingLater && (
          <span className="badge badge-muted" style={{ fontSize: 10 }}>
            {comingLater}
          </span>
        )}
      </div>
      <p className="muted" style={{ margin: "4px 0 0", fontSize: 11 }}>
        {hint}
      </p>
    </div>
  );
}
