# License Inventory

ART is licensed **GPL-3.0-or-later**. This document tracks the licenses of its
dependencies.

Almost every dependency below is permissive (MIT, Apache-2.0, Zlib, BSD,
CDLA) and so is compatible with distributing ART under the GPL. One is not:
`libpfs3` is **LGPL-3.0-or-later**, weak copyleft rather than permissive —
still compatible with ART's GPL-3.0-or-later, but the one exception to the
"permissive only" preference stated below, and marked as such in the table.
The reverse does not hold — a GPL *dependency* would be fine for ART's own
licence but is still avoided where it would constrain reuse of a module,
which is why ADFlib is read for understanding and never copied (see the note
at the foot of this file).
It is maintained manually and verified with `cargo deny check` (see
`deny.toml`).

> Never include components with incompatible licenses.
> Never distribute copyrighted ROMs or commercial software.

## Application license

- **ART**: [GPL-3.0-or-later](../LICENSE), Copyright (C) 2026 tolon.

  This page said MIT until 2026-08-13, months after the project moved to the
  GPL — the one line in the inventory that was about ART itself, and the one
  nobody re-read.

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
| `ureq` | HTTP client — the repository mirrors (§41.5.3), pinned `=3.2.1`, `gzip` off | MIT / Apache-2.0 |
| `tauri-plugin-drag` | Dragging a file *out* of the window into Explorer (Windows has no webview route for it) | MIT / Apache-2.0 |
| `zip` | ZIP reading (deflate only; compression, encryption and the other codecs are off) | MIT |
| `sevenz-rust2` | 7z reading (LZMA; the encoder is a dev-dependency for fixtures only) | MIT / Apache-2.0 |
| `quick-xml` | XML reading, for one file: `rp9-manifest.xml` inside an `.rp9` package, read through `core/archive`'s gate rather than from a path | MIT |
| `fatfs` | FAT32 — the PiStorm card's boot partition, the one filesystem ART writes that is not an Amiga one (`chrono` off, so a build repeats byte for byte) | MIT |
| `trash` | The Windows Recycle Bin — **outside `core/`**, in `tools/recycle_bin.rs`, because it calls `IFileOperation` | MIT |
| `libpfs3` | PFS3 filesystem implementation — writes and reads the volumes AmigaOS install (SD-2 G5) puts on a PiStorm card | **LGPL-3.0-or-later** (weak copyleft; the one non-permissive dependency in `core/`, compatible with ART's GPL-3.0-or-later but noted deliberately — `core/` is meant to be promotable to a standalone crate, which is exactly the reuse the project otherwise avoids constraining) |

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
| hst-imager | The named fallback for two typed gaps in ART's own PFS3/RDB writing ([ART-113](ISSUES.md), [ART-117](ISSUES.md)) — launched by `tools/hst_imager.rs`, never bundled | https://github.com/henrikstengaard/hst-imager — **MIT**, © Henrik Nørfjand Stengaard, read from the `license.txt` shipped beside `hst.imager.exe` in the 1.6.616 build on this machine rather than recalled |

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

**It did not, from the day the step was added until 2026-08-13**
([ART-098](ISSUES.md)). The workflow used a container action on a Windows
runner, which cannot run at all, so the step failed every time — and this page
went on claiming otherwise. Kept here rather than quietly corrected: a document
that says a check runs is worth exactly as much as the check, and this one was
worth nothing for a while.

## Licence notes for planned dependencies

`ART-kaynak-listesi.md` lists reference implementations for work still to come.
Two need care before any of their code is used:

- **ADFlib** is GPL-family. Linking it — even through FFI — would change ART's
  distribution licence. ART's own OFS/FFS implementation exists precisely to
  avoid this; keep it that way, or run ADFlib as a separate process.
- The **SFS** sources must have their terms checked in their own repository
  before SFS support is written.

**pfs3aio** was on this list too, once. It no longer needs care: PFS3 support
has been written — SD-2 G5 — through `libpfs3`, an independent crate
(LGPL-3.0-or-later, see the Rust dependency table above), not by copying or
linking pfs3aio itself.

Permissively licensed candidates (`delharc` MIT/Apache-2.0, `lhasa` ISC,
`xdms-rs`, `hunkfile`) pose no such problem.
