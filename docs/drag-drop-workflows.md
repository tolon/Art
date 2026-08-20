# Drag & Drop Workflows

> Drag-and-drop is not a convenience feature. It is a fundamental
> architectural requirement. *"Can the user accomplish this by dragging the
> object?"* — if yes, implement drag-and-drop.

## The Universal Drag & Drop Manager

There is **one** central drag & drop system, not per-module implementations.

### Architecture

```
┌─────────────────────────────────────────────┐
│  Webview (single global onDragDropEvent)     │
│  src/lib/dnd.ts                              │
└──────────────────┬──────────────────────────┘
                   │ dropped paths
                   ▼
        invoke('analyze_paths', { paths })
                   │
                   ▼
┌──────────────────────────────────────────────┐
│  commands::dragdrop::analyze_paths           │
│  (normalises paths, one analysis per object) │
└──────────────────┬───────────────────────────┘
                   │
                   ▼
┌──────────────────────────────────────────────┐
│  WorkflowEngine::plan(path)                  │
│  detect → registry.candidates → Plan         │
└──────────────────┬───────────────────────────┘
                   │ Plan[] (JSON)
                   ▼
┌──────────────────────────────────────────────┐
│  Frontend renders "What can I do?" panel     │
│  (recommendations + advanced actions)        │
└──────────────────────────────────────────────┘
```

### Why a single listener

- Avoids duplicated, inconsistent drop handling across modules.
- Routes everything through the Workflow Engine so recommendations are
  uniform.
- One place to enforce security (path normalisation, rejection of unsafe
  forms).

## Supported drop objects

- ADF, ADZ, DMS (floppy images)
- HDF, HDZ (hard disk images)
- LHA, ZIP, 7z (archives)
- ISO9660 discs, including raw 2352-byte track dumps (Mode 1 and Mode 2/XA)
- D64, D71, D81, T64 (Commodore 8-bit disks and tapes); TAP, PRG and CRT are
  identified and described rather than opened
- ROM (Kickstart)
- Amiga executables / WHDLoad packages
- Folders (smart folder analysis)
- Multiple files at once
- Removable media / Gotek USB (where technically possible)

## Drop phases

The frontend tracks four phases for visual feedback:

| Phase | Meaning | UI effect |
|-------|---------|-----------|
| `enter` | Cursor enters with files | Drop target highlights. |
| `over` | Cursor moving over window | Highlight maintained. |
| `leave` | Cursor leaves | Highlight removed. |
| `drop` | Files released | Analyse; render results. |

## Multi-file drops

Each dropped path is analysed independently. One bad file does not abort the
whole drop. Results surface per-file with ok/error status.

## Drag-and-drop must never be the only method

Per the accessibility rule, every drag-and-drop operation has an equivalent
keyboard/menu path. Drag is the *fast* path, not the *only* path.

## What each drop offers

The catalogue lives in `src-tauri/src/core/workflow/builtin.rs` — that file is
the source of truth, and tests enforce that every recognised format offers at
least one starred action and that no workflow crosses formats.

**LHA Studio, WHDLoad detection and install-to-hard-disk are offered for LHA
only**, not for every `Archive`. They are written against `core/lha` and would
fail on a ZIP; a test asserts no `lha.*` action reaches one. Every archive gets
the file manager instead, which reads all three formats.

| Drop | Starred actions | Also offered |
|------|-----------------|--------------|
| ADF / ADZ / DMS | Open in ADF Studio · Launch in WinUAE · Add to Collection | Copy to Gotek (raw ADF only) · Check Disk Health (raw ADF only) · Copy into a hard disk image · Hex Viewer† |
| LHA | Open in the file manager · Open in LHA Studio · Extract Files · Add to Collection · Install to a hard disk | Launch in WinUAE* |
| ZIP / 7z | Open in the file manager | Hex Viewer† |
| ISO9660 disc | Open in the file manager | Hex Viewer† |
| D64 / D71 / D81 / T64 | Open in the file manager | Hex Viewer† · SHA-256 |
| TAP / PRG / CRT | What is this? | Hex Viewer† · SHA-256 |
| HDF / HDZ | Open in Hard Disk Studio · Launch in WinUAE | Add to Collection · Hex Viewer† |
| ROM | Identify in ROM Studio · Use in a Machine Profile | Hex Viewer† |
| Folder | Scan into Collection · Organise onto a card · Build an HDF from this folder* | Prepare as Gotek drive |

"Copy into a hard disk image" and "Install to a hard disk" both route to the
two-pane file manager / WHDLoad install screen and copy between real volumes
— there is no separate conversion step.

\* Registered but not implemented yet — shown as "Coming Later" rather than
hidden, so the user can see it is planned (§96).
† Advanced: only shown in Power User mode (§47).

Two kinds of action, decided by the engine and not the UI:

- **Navigate** — opens a studio with the object loaded. Most actions are this.
- **Execute** — the engine does the work and returns an outcome (`adf.validate`,
  `analyze.hash`). Only read-only actions may run straight from the drop panel;
  anything that changes data must go through its studio's preview/backup/verify
  flow (§92).
