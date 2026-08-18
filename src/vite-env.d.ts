/// <reference types="vite/client" />

// Allow importing plain CSS files as side-effect imports.
declare module "*.css";

// Set by vite.config.ts from package.json's own `version` field — the one
// place the app's version is declared. Never hardcode a version literal
// anywhere the interface renders; read `import.meta.env.VITE_APP_VERSION`
// instead. Declared via the standard Vite `ImportMetaEnv` augmentation
// (rather than `define`) because it is populated for real, in both `pnpm
// dev` and a production build — see vite.config.ts for why `define` cannot
// be used for this.
interface ImportMetaEnv {
  readonly VITE_APP_VERSION: string;
}
