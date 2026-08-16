// OS Builder — distro profiles for the SD builder
// (`ART-research-distro-profiles.md` §5).
//
// **What this screen does today, and what it does not.** It knows the
// distributions, what each one requires of you, and whether the ROM and card
// you have suit it. It does **not** write a card: the adaptation checklist
// depends on inspecting a real distribution's layout by hand, which the
// research parks deliberately rather than guesses at (§8.2). Every profile is
// registered `available: false` and rendered as Coming Later — declared, not
// pretended (§96).
//
// **ART never downloads a distro image** (§2). There is no fetch button here
// and there will not be one; the link goes to the project's own page and the
// user comes back with a file. The same rule ART already applies to Kickstart
// ROMs.

import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

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
import { isOneOf, isTextOrNothing, isWholeNumberBetween } from "@/lib/remembered";
import { useRemembered } from "@/lib/useRemembered";
import { CardBuilder } from "@/components/osbuilder/CardBuilder";
import { OsInstall } from "@/components/osbuilder/OsInstall";
import { VolumePreload } from "@/components/osbuilder/VolumePreload";

/**
 * What the screen is being asked for.
 *
 * Three of the four work today, and they are three steps of the same story:
 * `boot-card` writes a card with the Pi's firmware, a Kickstart and a
 * partition table an Amiga can see; `install` turns the user's own AmigaOS
 * 3.2 install disks into a distribution tree with a manifest (SD-2 · G5);
 * `prepare-volumes` puts a filesystem into the card's partitions and copies
 * a tree like that one in, so the card arrives ready rather than offering to
 * format itself on first boot. Every distribution is still Coming Later, and
 * all four live side by side rather than the working ones being hidden
 * behind the one that is not (§96).
 */
type BuildKind = "distro" | "boot-card" | "install" | "prepare-volumes";

/** Card sizes people actually buy. Typed sizes are allowed too. */
const CARD_SIZES_GB = [16, 32, 64, 128, 256];

export function OsBuilder() {
  const { t } = useTranslation();

  const [kind, setKind] = useRemembered<BuildKind>(
    "osBuilder.kind",
    isOneOf<BuildKind>("distro", "boot-card", "install", "prepare-volumes"),
    "boot-card"
  );

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
    distroProfiles().then(setProfiles).catch((e) => setError(String(e)));
  }, []);

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
      <h1 style={{ fontSize: 20 }}>{t("nav.osBuilder")}</h1>
      <p className="muted" style={{ fontSize: 12, margin: "4px 0 16px" }}>
        {t("osBuilder.intro")}
      </p>

      {error && (
        <div className="badge badge-err" style={{ margin: "12px 0", padding: "6px 12px" }}>
          {error}
        </div>
      )}

      {/* Which of the two things this screen does. The working one is first,
          because it is the one that produces a file. */}
      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osBuilder.what.heading")}</h2>
        <div style={{ display: "flex", flexDirection: "column", gap: 8, marginTop: 8 }}>
          <KindChoice
            chosen={kind === "boot-card"}
            onChoose={() => setKind("boot-card")}
            title={t("osBuilder.what.bootCard")}
            hint={t("osBuilder.what.bootCardHint")}
          />
          <KindChoice
            chosen={kind === "install"}
            onChoose={() => setKind("install")}
            title={t("osBuilder.what.install")}
            hint={t("osBuilder.what.installHint")}
          />
          <KindChoice
            chosen={kind === "prepare-volumes"}
            onChoose={() => setKind("prepare-volumes")}
            title={t("osBuilder.what.prepareVolumes")}
            hint={t("osBuilder.what.prepareVolumesHint")}
          />
          <KindChoice
            chosen={kind === "distro"}
            onChoose={() => setKind("distro")}
            title={t("osBuilder.what.distro")}
            hint={t("osBuilder.what.distroHint")}
            comingLater={t("common.comingLater")}
          />
        </div>
      </section>

      {kind === "boot-card" && <CardBuilder />}

      {kind === "install" && <OsInstall />}

      {kind === "prepare-volumes" && <VolumePreload />}

      {kind === "distro" && (
        <>
      {/* The state of the whole screen, said once and at the top rather than
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
                <div
                  style={{ display: "flex", justifyContent: "space-between", gap: 8 }}
                >
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
                      gb: Math.round((image.size_bytes / (1024 * 1024 * 1024)) * 10) / 10,
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
              <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osBuilder.notes.heading")}</h2>
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
