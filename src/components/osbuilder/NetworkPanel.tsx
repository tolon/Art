// The card's network, entered while the card is being set up (SD-3 G14).
//
// The owner's decision, 2026-08-24: *"ART sorsun, kart kurarken WiFi
// bilgilerini girelim."* It sits on the volumes step, beside the panel that
// writes the card, because that is when somebody is setting one up — and it
// writes into the system volume the card will carry.
//
// **The passphrase goes one way.** `Secret` on the Rust side deserialises and
// does not serialise, so there is no "read what is there" call to render, and
// nothing sends a passphrase back to this screen. What is on screen is what
// the user has typed in this session and nothing else.
//
// **What is replaced is said before the button.** `Wireless.prefs` is
// rewritten rather than merged — merging two lists of networks means deciding
// which of somebody's to keep — so the count of what is already there is
// fetched when the tree is known and shown above the action, never after it.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import {
  networksAlreadyThere,
  seedNetwork,
  type Seeded,
  type TolunnetAddress,
  type WifiProfile,
} from "@/lib/amiganet";
import { errorText } from "@/lib/errorText";
import { isFlag, isText, isWholeNumberBetween } from "@/lib/remembered";
import { useBuildSession } from "@/lib/useBuildSession";
import { useRemembered } from "@/lib/useRemembered";

/** The supplicant's own range, mirrored from `core::amiganet::wpa`. */
const PASSPHRASE_MIN = 8;
const PASSPHRASE_MAX = 63;
const PMK_HEX = 64;

/** A 64-character hex key rather than a passphrase — the same test Rust makes. */
function isHexKey(psk: string): boolean {
  return psk.length === PMK_HEX && /^[0-9a-fA-F]+$/.test(psk);
}

/**
 * Why the button cannot be pressed yet, or `null`.
 *
 * Exported and pure so it can be tested without a screen, and so the sentence
 * is the same one the button's `title` and the line beneath it use.
 */
export function networkBlocker(input: {
  tree: string | null;
  wifiOn: boolean;
  ssid: string;
  security: "open" | "wpa";
  psk: string;
  stackOn: boolean;
  device: string;
  dhcp: boolean;
  ip: string;
  netmask: string;
  gateway: string;
  dns: string;
}): { key: string; params?: Record<string, unknown> } | null {
  if (!input.tree) return { key: "network.blocked.noTree" };
  if (!input.wifiOn && !input.stackOn) return { key: "network.blocked.nothingChosen" };

  if (input.wifiOn) {
    if (!input.ssid.trim()) return { key: "network.blocked.noSsid" };
    if (input.security === "wpa") {
      const psk = input.psk;
      if (
        !isHexKey(psk) &&
        (psk.length < PASSPHRASE_MIN || psk.length > PASSPHRASE_MAX)
      ) {
        // Says the range and the length, never the value.
        return {
          key: "network.blocked.passphraseLength",
          params: { min: PASSPHRASE_MIN, max: PASSPHRASE_MAX, key: PMK_HEX, got: psk.length },
        };
      }
    }
  }

  if (input.stackOn) {
    if (!input.device.trim()) return { key: "network.blocked.noDevice" };
    if (!input.dhcp) {
      for (const [field, value] of [
        ["ip", input.ip],
        ["netmask", input.netmask],
        ["gateway", input.gateway],
        ["dns", input.dns],
      ] as const) {
        if (!value.trim()) return { key: "network.blocked.staticIncomplete", params: { field } };
      }
    }
  }
  return null;
}

export function NetworkPanel() {
  const { t } = useTranslation();
  const tree = useBuildSession().session.tree.root;

  // Everything except the passphrase is remembered, which is the whole of the
  // decision: a WiFi password kept in ART's settings file is a WiFi password
  // in a file nobody thinks of as a secret. It is typed each time it is
  // written, and that is the cost of not storing it.
  const [wifiOn, setWifiOn] = useRemembered("network.wifiOn", isFlag, false);
  const [ssid, setSsid] = useRemembered("network.ssid", isText, "");
  const [security, setSecurity] = useRemembered<string>("network.security", isText, "wpa");
  const [psk, setPsk] = useState("");
  const [showPsk, setShowPsk] = useState(false);

  const [stackOn, setStackOn] = useRemembered("network.stackOn", isFlag, false);
  const [device, setDevice] = useRemembered("network.device", isText, "wifipi.device");
  const [unit, setUnit] = useRemembered("network.unit", isWholeNumberBetween(0, 255), 0);
  const [dhcp, setDhcp] = useRemembered("network.dhcp", isFlag, true);
  const [ip, setIp] = useRemembered("network.ip", isText, "");
  const [netmask, setNetmask] = useRemembered("network.netmask", isText, "255.255.255.0");
  const [gateway, setGateway] = useRemembered("network.gateway", isText, "");
  const [dns, setDns] = useRemembered("network.dns", isText, "");

  const [replacing, setReplacing] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<Seeded | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Asked as soon as the tree is known, so what a write costs is on screen
  // before the button rather than after it.
  useEffect(() => {
    if (!tree) {
      setReplacing(null);
      return;
    }
    let cancelled = false;
    networksAlreadyThere(tree)
      .then((had) => {
        if (!cancelled) setReplacing(had);
      })
      .catch(() => {
        if (!cancelled) setReplacing(null);
      });
    return () => {
      cancelled = true;
    };
  }, [tree, done]);

  const wifiSecurity: "open" | "wpa" = security === "open" ? "open" : "wpa";
  const blocker = networkBlocker({
    tree,
    wifiOn,
    ssid,
    security: wifiSecurity,
    psk,
    stackOn,
    device,
    dhcp,
    ip,
    netmask,
    gateway,
    dns,
  });

  async function write() {
    if (!tree || blocker) return;
    setBusy(true);
    setError(null);
    setDone(null);
    try {
      const networks: WifiProfile[] = wifiOn
        ? [
            {
              ssid,
              security: wifiSecurity,
              psk: wifiSecurity === "open" ? "" : psk,
              priority: 0,
            },
          ]
        : [];
      const address: TolunnetAddress = dhcp
        ? { how: "dhcp" }
        : { how: "static", ip, netmask, gateway, dns };
      setDone(
        await seedNetwork(tree, networks, stackOn ? { device, unit, address } : null)
      );
      // Typed again next time: it was never remembered.
      setPsk("");
    } catch (e) {
      setError(errorText(t, e));
    } finally {
      setBusy(false);
    }
  }

  const field = (
    label: string,
    value: string,
    onChange: (next: string) => void,
    hint?: string
  ) => (
    <label style={{ display: "flex", flexDirection: "column", gap: 4, marginBottom: 8 }}>
      <span className="muted" style={{ fontSize: 12 }}>
        {label}
      </span>
      <input
        className="input"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        aria-label={label}
      />
      {hint && (
        <span className="faint" style={{ fontSize: 10 }}>
          {hint}
        </span>
      )}
    </label>
  );

  return (
    <section className="card" style={{ marginBottom: 16 }} data-testid="network-panel">
      <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("network.heading")}</h2>
      <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
        {t("network.intro")}
      </p>

      <label style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 8 }}>
        <input type="checkbox" checked={wifiOn} onChange={(e) => setWifiOn(e.target.checked)} />
        <span style={{ fontSize: 13 }}>{t("network.wifi.enable")}</span>
      </label>

      {wifiOn && (
        <div style={{ marginLeft: 24, marginBottom: 12 }} data-testid="network-wifi">
          {field(t("network.wifi.ssid"), ssid, setSsid)}
          <label style={{ display: "flex", flexDirection: "column", gap: 4, marginBottom: 8 }}>
            <span className="muted" style={{ fontSize: 12 }}>
              {t("network.wifi.security")}
            </span>
            <select
              className="input"
              value={wifiSecurity}
              onChange={(e) => setSecurity(e.target.value)}
              aria-label={t("network.wifi.security")}
              style={{ maxWidth: "16em" }}
            >
              <option value="wpa">{t("network.wifi.wpa")}</option>
              <option value="open">{t("network.wifi.open")}</option>
            </select>
          </label>
          {wifiSecurity === "wpa" && (
            <label style={{ display: "flex", flexDirection: "column", gap: 4, marginBottom: 8 }}>
              <span className="muted" style={{ fontSize: 12 }}>
                {t("network.wifi.passphrase")}
              </span>
              <div style={{ display: "flex", gap: 6 }}>
                <input
                  className="input"
                  type={showPsk ? "text" : "password"}
                  value={psk}
                  onChange={(e) => setPsk(e.target.value)}
                  aria-label={t("network.wifi.passphrase")}
                  style={{ flex: 1 }}
                />
                <button
                  className="btn"
                  style={{ fontSize: 11 }}
                  onClick={() => setShowPsk(!showPsk)}
                >
                  {t(showPsk ? "network.wifi.hide" : "network.wifi.show")}
                </button>
              </div>
              {/* The one thing about this field somebody should know before
                  they type into it. */}
              <span className="faint" style={{ fontSize: 10 }}>
                {t("network.wifi.notRemembered")}
              </span>
            </label>
          )}
        </div>
      )}

      <label style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 8 }}>
        <input type="checkbox" checked={stackOn} onChange={(e) => setStackOn(e.target.checked)} />
        <span style={{ fontSize: 13 }}>{t("network.stack.enable")}</span>
      </label>

      {stackOn && (
        <div style={{ marginLeft: 24, marginBottom: 12 }} data-testid="network-stack">
          {field(t("network.stack.device"), device, setDevice, t("network.stack.deviceHint"))}
          <label style={{ display: "flex", flexDirection: "column", gap: 4, marginBottom: 8 }}>
            <span className="muted" style={{ fontSize: 12 }}>
              {t("network.stack.unit")}
            </span>
            <input
              className="input"
              value={String(unit)}
              onChange={(e) => {
                const parsed = Number.parseInt(e.target.value, 10);
                if (Number.isFinite(parsed) && parsed >= 0 && parsed <= 255) setUnit(parsed);
                else if (e.target.value === "") setUnit(0);
              }}
              aria-label={t("network.stack.unit")}
              style={{ maxWidth: "8em" }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 8 }}>
            <input type="checkbox" checked={dhcp} onChange={(e) => setDhcp(e.target.checked)} />
            <span style={{ fontSize: 13 }}>{t("network.stack.dhcp")}</span>
          </label>
          {!dhcp && (
            <div data-testid="network-static">
              {field(t("network.stack.ip"), ip, setIp)}
              {field(t("network.stack.netmask"), netmask, setNetmask)}
              {field(t("network.stack.gateway"), gateway, setGateway)}
              {field(t("network.stack.dns"), dns, setDns)}
            </div>
          )}
        </div>
      )}

      {/* **Said before the button, never after.** `Wireless.prefs` is
          rewritten rather than merged. */}
      {wifiOn && replacing !== null && replacing > 0 && (
        <p
          data-testid="network-replacing"
          className="badge badge-warn"
          style={{ display: "block", fontSize: 11, padding: "4px 8px", marginBottom: 8 }}
        >
          {t("network.replacing", { count: replacing })}
        </p>
      )}

      {error && (
        <p
          className="badge badge-err"
          style={{ display: "block", fontSize: 11, padding: "4px 8px", marginBottom: 8 }}
        >
          {error}
        </p>
      )}

      <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
        <button
          className="btn btn-primary"
          onClick={() => void write()}
          disabled={busy || blocker !== null}
          title={blocker ? t(blocker.key, blocker.params) : undefined}
        >
          {t(busy ? "network.writing" : "network.write")}
        </button>
        {blocker && (
          <span className="muted" style={{ fontSize: 11 }}>
            {t(blocker.key, blocker.params)}
          </span>
        )}
      </div>

      {done && (
        <p
          data-testid="network-done"
          className="badge badge-ok"
          style={{ display: "block", fontSize: 11, padding: "4px 8px", marginTop: 8 }}
        >
          {t("network.done", { files: done.written.join(", ") })}
        </p>
      )}
    </section>
  );
}
