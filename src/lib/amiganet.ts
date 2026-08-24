// Pre-seeding an Amiga's network before its first boot (SD-3 G14).
//
// The owner's decision, 2026-08-24: *"ART sorsun, kart kurarken WiFi
// bilgilerini girelim"* — ART asks, and the credentials are entered while the
// card is being set up.
//
// **Which files, and why not the obvious one.** `DEVS:NetInterfaces` is
// Roadshow's. This owner's stack is their own: `tolunwifi` writes
// `ENVARC:Sys/Wireless.prefs` (the credentials) and `tolunnet` writes
// `DEVS:tolunnet.config` (device, unit, address). Both formats were read out
// of their own C — see `core/amiganet`'s module docs.
//
// **The passphrase goes one way only.** On the Rust side it is a `Secret`,
// which deserialises and does not serialise, so nothing sends it back: not to
// this file, not into a manifest, not into the operation log. There is
// deliberately no "read the current WiFi settings" call here — ART can write
// them and cannot report them.

import { invoke } from "@tauri-apps/api/core";

/** How a network is secured. */
export type WifiSecurity = "open" | "wpa";

/** One network to join. */
export interface WifiProfile {
  ssid: string;
  security: WifiSecurity;
  /** A passphrase (8–63) or a 64-character key. Empty for an open network. */
  psk: string;
  /** Only written when non-zero, for picking between several networks. */
  priority: number;
}

/** How the card gets its address. */
export type TolunnetAddress =
  | { how: "dhcp" }
  | { how: "static"; ip: string; netmask: string; gateway: string; dns: string };

/** tolunnet's own configuration. Nothing here is a secret. */
export interface TolunnetConfig {
  /** `wifipi.device` on a PiStorm card. */
  device: string;
  unit: number;
  address: TolunnetAddress;
}

/** What was written — counts and filenames, never an SSID or a passphrase. */
export interface Seeded {
  /** The files ART wrote, in AmigaDOS spelling. */
  written: string[];
  /** How many `network={}` blocks the old file held, when there was one. */
  replacedNetworks: number | null;
  /** Whether `tolunnet.config` was edited rather than created. */
  tolunnetMerged: boolean;
  networks: number;
}

/**
 * How many networks a rewrite would replace, **before** the button.
 *
 * `Wireless.prefs` is replaced rather than merged — merging two lists of
 * networks means deciding which of somebody's to keep, which is a decision and
 * not a merge — so the screen says what that costs first.
 */
export async function networksAlreadyThere(tree: string): Promise<number | null> {
  return invoke<number | null>("amiganet_networks_already_there", { tree });
}

/** Put the two files into a system volume. */
export async function seedNetwork(
  tree: string,
  networks: WifiProfile[],
  tolunnet: TolunnetConfig | null
): Promise<Seeded> {
  return invoke<Seeded>("amiganet_seed", { request: { tree, networks, tolunnet } });
}
