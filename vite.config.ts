// `defineConfig` comes from "vitest/config" rather than "vite" so this file
// stays the single source of truth for both the dev server and the test
// runner (Vitest merges its own `test` field in, and otherwise behaves like
// Vite's own `defineConfig`) — no separate vitest.config.ts to drift out of
// sync with the `@/*` alias below.
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import path from "node:path";
import { readFileSync } from "node:fs";

// Tauri expects a fixed port and ignores its own source tree to avoid rebuild loops.
const host = process.env.TAURI_DEV_HOST;

// Single source of truth for the version shown in the UI (Sidebar.tsx): read
// straight from package.json rather than duplicating the number here. Handed
// to the app as `import.meta.env.VITE_APP_VERSION` rather than
// `@tauri-apps/api/app`'s `getVersion()` deliberately — the sidebar is part
// of the shell chrome and must render the same way in `pnpm dev` (no Tauri
// IPC bridge, no packaged app behind it) as in the built app, with no async
// fetch and no "how do I check I'm inside Tauri" branch to keep in sync with
// the other route. It is also not Vite's own `define` config below this
// comment: `define` is textually substituted at build time only — Vite 6
// deliberately skips it for the dev server's client transform (confirmed
// against this project's pinned Vite version; `pnpm dev` would keep serving
// the literal `__APP_VERSION__` token forever) — while `import.meta.env.*`
// is populated for real in both dev and build. Setting it on `process.env`
// here, before Vite's own env loading runs, is what makes a plain env
// variable carry a value computed at config time rather than typed by hand
// into a `.env` file that could drift from package.json.
process.env.VITE_APP_VERSION = (
  JSON.parse(readFileSync(path.resolve(__dirname, "package.json"), "utf-8")) as {
    version: string;
  }
).version;

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  // Mirror the `@/*` path alias declared in tsconfig.json so the bundler
  // (Rollup) can resolve the same imports TypeScript already accepts.
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
    },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // Bind to all interfaces so Tauri's webview (which may connect via
    // 127.0.0.1 even when Vite reports "localhost") can reach the dev server.
    host: host || "127.0.0.1",
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
    watch: {
      // Don't watch the Rust side — it triggers needless frontend rebuilds.
      ignored: ["**/src-tauri/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    // Tauri supports Safari 13+ on macOS and modern Chrome on Windows/Linux.
    target:
      process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari13",
    minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
  test: {
    // Plain Node by default: `src/i18n/parity.test.ts` and friends read the
    // source tree from disk and must not pay for (or accidentally depend on)
    // a DOM. Only React component tests (`*.test.tsx`) get jsdom — scoped by
    // glob rather than globally, so the two kinds of test never share an
    // environment by accident.
    environment: "node",
    environmentMatchGlobs: [["src/**/*.test.tsx", "jsdom"]],
  },
});
