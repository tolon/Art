# Amiga Retro Toolkit (ART)

**The Swiss Army Knife for Amiga Files**

A professional Windows desktop toolkit for Commodore Amiga users. ART combines
ADF, HDF, LHA, ROM, Gotek, WinUAE and collection management into one coherent,
drag-and-drop-driven application.

> **DROP IT INTO ART.**

## Status

The application builds and runs on Windows 10/11 x64. Working today: DD/HD
floppy images and hard-disk (RDB/HDF) partitions — read, write, create and
validate through one volume driver, including boot code that starts a real
Amiga (verified by booting a disk under a licensed Kickstart/Workbench, not
bare metal) — with a Total Commander-style dual pane (browse, multi-select,
batch copy in/out/delete, sort, filter by filename mask, rename, mkdir,
attributes) over FFS/OFS volumes; LHA (browse, safe extraction, WHDLoad
detection, several archives installed to a disk at once); Kickstart ROM
identification; Gotek/FlashFloppy; PiStorm/Emu68; WinUAE launching;
collection scanning; a background job queue with progress/cancel; an
operation log; Beginner/Power User modes; and the drag-and-drop Workflow
Engine behind "what can I do with this?".

Two directions a multi-selection cannot yet move in: image-to-image (copy
one at a time instead) and, when copying a selection out of an image, as one
atomic operation rather than several running together — see
[docs/ISSUES.md](docs/ISSUES.md) (ART-064, ART-065). The restyled Files
screen is covered by automated tests only; nobody has opened it in a running
window yet.

Data safety is enforced in `core/safety`: every write is atomic, and files are
backed up to `.art-backup/` before being replaced (or, for images too large to
hold in memory, journaled block-by-block). Hand-tuned configuration files are
edited in place, never regenerated.

The interface ships in English and Turkish. The language is chosen in
Settings and remembered across restarts. Error messages coming from the Rust
core are still English regardless of the chosen language.

Not yet built: PFS3/SFS (partitions using them are listed but their contents
are not readable), DMS/ADZ conversion, recovery tools.

| | |
|---|---|
| Where the project is, what is next | [docs/STATUS.md](docs/STATUS.md) |
| Feature-by-feature state | [docs/FEATURES.md](docs/FEATURES.md) |
| Known defects | [docs/ISSUES.md](docs/ISSUES.md) |
| Phase definitions | [docs/roadmap.md](docs/roadmap.md) |
| Released changes | [CHANGELOG.md](CHANGELOG.md) |

## Requirements

| Tool | Version | Notes |
|------|---------|-------|
| **Rust** | 1.77+ (stable) | MSVC toolchain (`x86_64-pc-windows-msvc`) |
| **MSVC Build Tools** | VS 2022 | "Desktop development with C++" workload |
| **Node.js** | 20+ | for the frontend |
| **pnpm** | 9+ | package manager |
| **WebView2 Runtime** | any | preinstalled on Windows 10/11 |

## Setup

### 1. Rust toolchain (MSVC)

```powershell
# Install via https://rustup.rs, then ensure the MSVC target is default:
rustup default stable-x86_64-pc-windows-msvc
rustc -vV   # should show: host: x86_64-pc-windows-msvc
```

### 2. MSVC C++ Build Tools

Install **Visual Studio 2022** (Community is fine) or **Build Tools for Visual
Studio 2022** with the **"Desktop development with C++"** workload selected.
This provides `cl.exe`/`link.exe` that the MSVC Rust target requires.

### 3. Frontend dependencies

```bash
pnpm install
```

## Development

```bash
# Run the app in dev mode (hot-reload frontend + Rust rebuild)
pnpm tauri dev

# Type-check the frontend
pnpm lint

# Run Rust unit tests
cd src-tauri && cargo test

# Production build (produces .msi + .exe installers)
pnpm tauri build
```

Build output:

```
src-tauri/target/release/bundle/
├── msi/   Amiga Retro Toolkit_0.1.0_x64_en-US.msi
└── nsis/  Amiga Retro Toolkit_0.1.0_x64-setup.exe
```

## Architecture

```
UI (React + TypeScript)
        ↓  Tauri commands
Application Services / Commands
        ↓
Workflow Engine  ←  Detection
        ↓
Amiga Core (Rust, platform-independent)
├── adf / volume (the filesystem driver + writer) / hdf / lha / rdb / rom / binary
├── analysis / recovery / compatibility
├── hashing / conversion / validation
        ↓
Platform Services → Windows
```

The Amiga core is **platform-independent Rust** — no Tauri types, no
Windows APIs. This keeps it unit-testable and reusable. See
[docs/architecture.md](docs/architecture.md).

## License

MIT. See [LICENSE](LICENSE).
