// Logging bridge: route Rust logs into the webview console (dev only) and
// expose TS-side log helpers that forward to the Rust log targets.

import { attachConsole, info, warn, error } from "@tauri-apps/plugin-log";

let _attached = false;

/** Call once at app startup. Safe to call multiple times. */
export async function initLogging(): Promise<void> {
  if (_attached) return;
  _attached = true;
  // Rust logs → webview console, but only while developing.
  if (import.meta.env?.DEV) {
    await attachConsole().catch(() => {
      /* plugin may be unavailable in non-Tauri contexts */
    });
  }
}

export const log = { info, warn, error };
