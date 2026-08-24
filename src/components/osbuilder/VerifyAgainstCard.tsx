// Verify a distribution tree against a volume that already exists.
//
// **ART-197 wave 2, row 3 — the first cut of `OsInstall.tsx`.** That file was
// 1 722 lines and four `<section>`s: the sources, the component checklist, the
// run, and this. The other three are the install itself and share every piece
// of its state; this one shares none. Seven `useState`s, three handlers, two
// derived values and one section, referring to nothing above it — which is why
// it is the cut that costs nothing and the one wave 3 needs, since wave 3 moves
// this off the install step entirely and a section that is already a component
// can be moved by changing one line.
//
// **Its state is session-only, deliberately**, and that is not an oversight
// against the remembered-settings rule. Every field here names something the
// user is *checking*, not something they are building — the tree they want
// compared and the card they want it compared against. A remembered card
// number that came back pointing at a different card, under a heading that
// says "verified", is the confident wrong sentence this project pays most for.
// `OsInstall.tsx`'s own module comment recorded that decision before this file
// existed and it is carried across unchanged.
//
// **`isVerified` is `failed === 0 && notChecked === 0`, never `failed === 0`
// alone** (§89). "ART did not look" is not "ART found nothing wrong", so a
// `notChecked` above zero keeps this a warning and never a tick.

import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import {
  isVerified,
  osinstallVerify,
  parseOptionalSlot,
  parsePartitionIndex,
  type VerifyReport,
} from "@/lib/osinstall";
import { Field } from "@/components/osbuilder/Field";
import { errorText } from "@/lib/errorText";
import { useBuildSession } from "@/lib/useBuildSession";

export function VerifyAgainstCard() {
  const { t } = useTranslation();

  /**
   * **The tree ART just wrote is the tree this offers to check** (ART-197).
   *
   * Before the split, `OsInstall.tsx` called `setVerifyDistRoot(r.destination)`
   * from its own result handler. That carry is kept and is now made of the
   * session instead of a direct call, which is both what lets this file stand
   * alone and a slightly wider promise: a tree chosen on *any* step arrives
   * here, not only one this run happened to build.
   *
   * **A pick the user made themselves is never moved.** `touched` is set the
   * moment they browse, and after that the session can change as it likes —
   * a field that rewrites itself under somebody is the one outcome the
   * remembered-settings rule forbids outright, and it does not stop being
   * that because the value was not persisted.
   *
   * **One mutation survives here and it is disclosed rather than worked
   * around**: replacing the initial `useState(session.tree.root)` with `null`
   * fails nothing, because the effect below sets it on mount anyway. The
   * initial value is kept for one frame's worth of reason — without it the
   * field renders "No distribution tree chosen" and then corrects itself, and
   * a screen that flickers a wrong answer is a small version of the thing
   * this project is most careful about. No test can tell the two apart, and
   * writing one that pretended to would be worse than saying so.
   */
  const { session } = useBuildSession();
  const [distRoot, setDistRoot] = useState<string | null>(session.tree.root);
  const touched = useRef(false);
  useEffect(() => {
    if (!touched.current) setDistRoot(session.tree.root);
  }, [session.tree.root]);
  const [image, setImage] = useState<string | null>(null);
  const [slotText, setSlotText] = useState("");
  const [indexText, setIndexText] = useState("1");
  const [verifying, setVerifying] = useState(false);
  const [report, setReport] = useState<VerifyReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function chooseDistRoot() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: t("osinstall.verify.distRoot.chooseTitle"),
    });
    if (typeof picked === "string") {
      touched.current = true;
      setDistRoot(picked);
    }
  }

  async function chooseImage() {
    const picked = await open({
      multiple: false,
      title: t("osinstall.verify.image.chooseTitle"),
      filters: [{ name: "Amiga volume image", extensions: ["img", "hdf"] }],
    });
    if (typeof picked === "string") setImage(picked);
  }

  const slot = parseOptionalSlot(slotText);
  const index = parsePartitionIndex(indexText);
  const ready = !!distRoot && !!image && slot.ok && index !== null;

  async function runVerify() {
    if (!distRoot || !image || !slot.ok || index === null) return;
    setVerifying(true);
    setError(null);
    setReport(null);
    try {
      setReport(await osinstallVerify(image, slot.value, index, distRoot));
    } catch (e) {
      setError(errorText(t, e));
    } finally {
      setVerifying(false);
    }
  }

  return (
    <section className="card" style={{ marginBottom: 16 }}>
      <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osinstall.verify.heading")}</h2>
      <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
        {t("osinstall.verify.intro")}
      </p>

      <Field
        label={t("osinstall.verify.distRoot.label")}
        value={distRoot}
        empty={t("osinstall.verify.distRoot.none")}
        onChoose={() => void chooseDistRoot()}
        choose={t("common.browse")}
        hint={t("osinstall.verify.distRoot.hint")}
      />
      <Field
        label={t("osinstall.verify.image.label")}
        value={image}
        empty={t("osinstall.verify.image.none")}
        onChoose={() => void chooseImage()}
        choose={t("common.browse")}
        hint={t("osinstall.verify.image.hint")}
      />

      <div style={{ display: "flex", gap: 16, marginBottom: 12, flexWrap: "wrap" }}>
        <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          <span className="muted" style={{ fontSize: 12 }}>
            {t("osinstall.verify.slot.label")}
          </span>
          <input
            className="input"
            value={slotText}
            onChange={(e) => setSlotText(e.target.value)}
            style={{ maxWidth: "8em" }}
          />
          <span className="faint" style={{ fontSize: 10 }}>
            {t("osinstall.verify.slot.hint")}
          </span>
        </label>
        <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          <span className="muted" style={{ fontSize: 12 }}>
            {t("osinstall.verify.index.label")}
          </span>
          <input
            className="input"
            value={indexText}
            onChange={(e) => setIndexText(e.target.value)}
            style={{ maxWidth: "8em" }}
          />
        </label>
      </div>

      {error && (
        <div className="badge badge-err" style={{ display: "block", padding: "6px 12px", fontSize: 12, marginBottom: 12 }}>
          {t("osinstall.verify.error", { message: error })}
        </div>
      )}

      <div style={{ display: "flex", gap: 8, alignItems: "center", marginBottom: 12 }}>
        <button className="btn" onClick={() => void runVerify()} disabled={verifying || !ready}>
          {t(verifying ? "osinstall.verify.running" : "osinstall.verify.run")}
        </button>
        {!ready && (
          <span className="faint" style={{ fontSize: 11 }}>
            {t("osinstall.verify.needsInputs")}
          </span>
        )}
      </div>

      {report && (
        <>
          {/* isVerified is failed === 0 && notChecked === 0 — never
              failed === 0 alone. "ART did not look" is not "ART found
              nothing wrong" (§89), so a NotChecked count above zero keeps
              this a warning, never a tick. */}
          <p
            className={isVerified(report) ? "badge badge-ok" : "badge badge-warn"}
            style={{ display: "block", padding: "8px 12px", fontSize: 12, marginBottom: 8 }}
          >
            {t(isVerified(report) ? "osinstall.verify.verified" : "osinstall.verify.notVerified")}
          </p>
          <p className="muted" style={{ fontSize: 12, margin: "0 0 8px" }}>
            {t("osinstall.verify.summary", {
              passed: report.passed,
              failed: report.failed,
              notChecked: report.notChecked,
            })}
          </p>
          <div
            style={{
              maxHeight: 280,
              overflowY: "auto",
              border: "1px solid var(--border)",
              borderRadius: 4,
              padding: "6px 10px",
            }}
          >
            {report.files.map((file, i) => (
              <div key={i} style={{ fontSize: 11, padding: "2px 0" }}>
                <div style={{ display: "flex", justifyContent: "space-between", gap: 8 }}>
                  <span style={{ wordBreak: "break-all" }}>{file.path}</span>
                  {file.state === "pass" && (
                    <span className="badge badge-ok" style={{ fontSize: 10 }}>
                      {t("osinstall.verify.state.pass")}
                    </span>
                  )}
                  {file.state === "fail" && (
                    <span className="badge badge-err" style={{ fontSize: 10 }}>
                      {t("osinstall.verify.state.fail")}
                    </span>
                  )}
                  {file.state === "not-checked" && (
                    <span className="badge badge-warn" style={{ fontSize: 10 }}>
                      {t("osinstall.verify.state.notChecked")}
                    </span>
                  )}
                </div>
                {/* Rust-side detail text stays English (ART-060) — the
                    same rule CoreError messages and WhdloadRefusal follow. */}
                {file.detail && (
                  <div className="faint" style={{ fontSize: 10 }}>
                    {file.detail}
                  </div>
                )}
              </div>
            ))}
          </div>
        </>
      )}
    </section>
  );
}
