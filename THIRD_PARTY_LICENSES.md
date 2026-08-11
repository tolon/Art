# Third-Party Licenses

ART (Amiga Retro Toolkit) is MIT-licensed. This file acknowledges the
third-party software ART depends on. For the full, machine-checked inventory
see [docs/licenses.md](docs/licenses.md) and run `cargo deny check`.

## Core dependencies

ART is built on the following open-source projects:

- **Tauri 2** — desktop application framework (MIT / Apache-2.0), plus its
  plugins used here: `tauri-plugin-dialog`, `tauri-plugin-sql`,
  `tauri-plugin-log`, `tauri-plugin-store`, `tauri-plugin-fs` (all MIT /
  Apache-2.0)
- **tauri-plugin-drag** / `@crabnebula/tauri-plugin-drag` — native drag-out-of-
  window support (MIT / Apache-2.0)
- **React** — UI library (MIT)
- **TypeScript** — typed JavaScript (Apache-2.0)
- **Vite** — frontend build tool (MIT)
- **Rust** standard library (MIT / Apache-2.0)
- **SQLite** (via `libsqlite3-sys`, bundled by `tauri-plugin-sql`) — public domain
- **serde / serde_json** — serialization (MIT / Apache-2.0)
- **sha2** — cryptographic hashing (MIT / Apache-2.0)
- **thiserror** — error handling (MIT / Apache-2.0)
- **delharc** — LHA/LZH decompression (MIT / Apache-2.0)
- **zip** — ZIP reading, deflate only (MIT), with `flate2` (MIT / Apache-2.0),
  `zlib-rs` (Zlib), `crc32fast` (MIT / Apache-2.0), `indexmap` (MIT /
  Apache-2.0) and `memchr` (MIT / Unlicense) beneath it. Compression,
  encryption and the other decompressors are switched off — ART only reads,
  and every feature left on is more code parsing a hostile file
- **ureq** — HTTP client used by the Aminet repository mirror (§41.5.3) (MIT / Apache-2.0)
- **i18next / react-i18next** — internationalization (MIT)
- **zustand** — state management (MIT)
- **react-router-dom** — routing (MIT)

Each of these projects is gratefully acknowledged.

## External tools (not bundled)

ART may invoke user-installed external tools. These are **not distributed**
with ART. Users must obtain them from their official sources:

- **WinUAE** — © Toni Wilen (freeware). Source: https://www.winuae.net/
- **FlashFloppy** — MIT, by Keir Fraser. Source:
  https://github.com/keirf/FlashFloppy
- **LHA** — license per upstream. Source: official Amiga archive tool sites.

## Copyrighted content

ART **never** distributes:
- commercial Amiga games,
- commercial AmigaOS / Workbench,
- copyrighted Kickstart ROMs,
- pirated software.

ART works exclusively with **user-owned** files. Users are responsible for
the legal status of the files they manage with ART.
