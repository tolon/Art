# Amiga Retro Toolkit (ART)

**The Swiss Army Knife for Amiga Files**

A professional Windows desktop toolkit for Commodore Amiga users. ART combines
ADF, HDF, LHA, ROM, Gotek, WinUAE and collection management into one coherent,
drag-and-drop-driven application.

> **DROP IT INTO ART.**

## Status

The application builds and runs on Windows 10/11 x64. Working today: ADF (read,
write, create, validate), LHA (browse, safe extraction, WHDLoad detection),
HDF/RDB, Kickstart ROM identification, Gotek/FlashFloppy, PiStorm/Emu68, WinUAE
launching, collection scanning, and the drag-and-drop Workflow Engine behind
"what can I do with this?".

Data safety is enforced in `core/safety`: every write is atomic, and files are
backed up to `.art-backup/` before being replaced. Hand-tuned configuration
files are edited in place, never regenerated.

Not yet built: background job queue, operation log, Beginner/Power User modes,
PFS3/SFS, DMS/ADZ conversion, recovery tools.

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
├── adf / hdf / lha / rdb / rom / binary
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
