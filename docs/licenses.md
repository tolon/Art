# License Inventory

ART is licensed **GPL-3.0-or-later**. This document tracks the licenses of its
dependencies.

Every dependency below is permissive (MIT, Apache-2.0, Zlib, BSD, CDLA) and so
is compatible with distributing ART under the GPL. The reverse does not hold —
a GPL *dependency* would be fine for ART's own licence but is still avoided
where it would constrain reuse of a module, which is why ADFlib is read for
understanding and never copied (see the note at the foot of this file).
It is maintained manually and verified with `cargo deny check` (see
`deny.toml`).

> Never include components with incompatible licenses.
> Never distribute copyrighted ROMs or commercial software.

## Application license

- **ART**: [MIT](../LICENSE)

## Rust dependencies (`src-tauri/Cargo.toml`)

| Crate | Purpose | License |
|-------|---------|---------|
| `tauri` | Desktop app framework | MIT / Apache-2.0 |
| `tauri-build` | Build script support | MIT / Apache-2.0 |
| `tauri-plugin-sql` | SQLite access | MIT / Apache-2.0 (wraps `sqlx`) |
| `tauri-plugin-log` | Structured logging | MIT / Apache-2.0 |
| `tauri-plugin-store` | JSON key/value settings | MIT / Apache-2.0 |
| `tauri-plugin-dialog` | Native file dialogs | MIT / Apache-2.0 |
| `tauri-plugin-fs` | File system access | MIT / Apache-2.0 |
| `serde` / `serde_json` | Serialization | MIT / Apache-2.0 |
| `thiserror` | Ergonomic error types | MIT / Apache-2.0 |
| `sha2` | SHA256 hashing | MIT / Apache-2.0 |
| `log` | Logging facade | MIT / Apache-2.0 |
| `delharc` | LHA/LZH decompression | MIT / Apache-2.0 |
| `zip` | ZIP reading (deflate only; compression, encryption and the other codecs are off) | MIT |
| `sevenz-rust2` | 7z reading (LZMA; the encoder is a dev-dependency for fixtures only) | MIT / Apache-2.0 |

(Transitive dependencies are audited via `cargo deny check licenses`.)

## JavaScript dependencies (`package.json`)

| Package | Purpose | License |
|---------|---------|---------|
| `@tauri-apps/api` | Tauri JS API | MIT / Apache-2.0 |
| `@tauri-apps/cli` | Tauri CLI | MIT / Apache-2.0 |
| `@tauri-apps/plugin-sql` | SQLite JS bindings | MIT / Apache-2.0 |
| `@tauri-apps/plugin-log` | Log JS bindings | MIT / Apache-2.0 |
| `@tauri-apps/plugin-store` | Store JS bindings | MIT / Apache-2.0 |
| `@tauri-apps/plugin-dialog` | Dialog JS bindings | MIT / Apache-2.0 |
| `react` / `react-dom` | UI library | MIT |
| `react-router-dom` | Routing | MIT |
| `i18next` / `react-i18next` | Internationalization | MIT |
| `zustand` | State management | MIT |
| `typescript` | Type-checking | Apache-2.0 |
| `vite` | Bundler | MIT |
| `@vitejs/plugin-react` | React plugin for Vite | MIT |

## External tools (user-provided, optional)

ART may invoke these if the user configures them. ART does **not** bundle
them. Users must obtain them from official sources and respect their licenses.

| Tool | Purpose | Source |
|------|---------|--------|
| WinUAE | Amiga emulator | https://www.winuae.net/ |
| LHA | Archive tool (optional) | official Amiga archives |
| FlashFloppy utilities | Gotek firmware config | https://github.com/keirf/FlashFloppy |

## Test-only dependency (never shipped)

| Tool | Purpose | License | Notes |
|------|---------|---------|-------|
| `amitools` (Python) | External oracle for `scripts/oracle-check.py` — an independent implementation with no shared code, used to cross-check ART's own ADF/HDF reading and writing in both directions | GPL | Installed only in CI/dev (`pip install amitools`) and invoked as a separate process; never linked into or distributed with ART. Same arm's-length relationship as the ADFlib note below: read for verification, not for code. |

## Verification

```bash
cd src-tauri
cargo install --locked cargo-deny   # one-time
cargo deny check                    # licenses + advisories + bans + sources
```

CI runs `cargo deny check` on every push, so a dependency with an incompatible
licence or a known advisory fails the build rather than reaching a release
(spec §67).

## Licence notes for planned dependencies

`ART-kaynak-listesi.md` lists reference implementations for work still to come.
Two need care before any of their code is used:

- **ADFlib** is GPL-family. Linking it — even through FFI — would change ART's
  distribution licence. ART's own OFS/FFS implementation exists precisely to
  avoid this; keep it that way, or run ADFlib as a separate process.
- **pfs3aio** and the **SFS** sources must have their terms checked in their own
  repositories before PFS3/SFS support is written.

Permissively licensed candidates (`delharc` MIT/Apache-2.0, `lhasa` ISC,
`xdms-rs`, `hunkfile`) pose no such problem.
