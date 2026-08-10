// `defineConfig` comes from "vitest/config" rather than "vite" so this file
// stays the single source of truth for both the dev server and the test
// runner (Vitest merges its own `test` field in, and otherwise behaves like
// Vite's own `defineConfig`) — no separate vitest.config.ts to drift out of
// sync with the `@/*` alias below.
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import path from "node:path";

// Tauri expects a fixed port and ignores its own source tree to avoid rebuild loops.
const host = process.env.TAURI_DEV_HOST;

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
