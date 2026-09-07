# Third-Party Licenses

ART (Amiga Retro Toolkit) is licensed **GPL-3.0-or-later**. This file acknowledges the
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
- **md-5** — MD5, used for exactly one job: looking a user's own install disk
  up in `core/osinstall/media_hashes.json`'s 186-row table of known media
  (MIT / Apache-2.0; `core/osinstall/mediahash.rs` is the lookup — see
  "Derived data" below for the table it reads). It is the key of somebody
  else's existing database, not an integrity check — SHA256 (`sha2`, above)
  remains ART's own hash for verification, duplicates and snapshots, and this
  does not replace it. Same RustCrypto family as `sha2`, same API shape.
  Already present in the dependency tree as a transitive dependency of
  `sqlx`/`tauri-plugin-sql`; this makes it a direct, documented one
- **thiserror** — error handling (MIT / Apache-2.0)
- **delharc** — LHA/LZH decompression (MIT / Apache-2.0)
- **zip** — ZIP reading, deflate only (MIT), with `flate2` (MIT / Apache-2.0),
  `zlib-rs` (Zlib), `crc32fast` (MIT / Apache-2.0), `indexmap` (MIT /
  Apache-2.0) and `memchr` (MIT / Unlicense) beneath it. Compression,
  encryption and the other decompressors are switched off — ART only reads,
  and every feature left on is more code parsing a hostile file
- **sevenz-rust2** — 7z reading (MIT / Apache-2.0), with `lzma-rust2`
  (MIT / Apache-2.0) beneath it. Same rule: the encoder is a dev-dependency
  used to build test fixtures and is not compiled into the application
- **fatfs** — FAT32, created and written for a PiStorm card's boot partition
  (MIT), with `byteorder` (MIT / Unlicense) and `bitflags` (MIT / Apache-2.0)
  beneath it. The one filesystem ART writes that is not an Amiga one: the
  Raspberry Pi's firmware boots from it. `chrono` is switched off, so files
  carry no timestamps — a build that produces the same bytes twice is one a
  manifest can describe
- **quick-xml** — XML reading, for one file: `rp9-manifest.xml` inside a
  Cloanto RetroPlatform `.rp9` package (MIT), with `memchr` (MIT / Unlicense)
  beneath it. Read-only, and reached through `core/archive`'s gate rather than
  from a path, so the manifest's bytes arrive already bounded. It is a crate
  rather than a reader of ART's own because hand-parsing namespaced XML out of
  a file a stranger wrote — entity escapes, CDATA, attribute quoting — is the
  failure class `core/security` exists for
- **ureq** — HTTP client used by the Aminet repository mirror (§41.5.3) (MIT / Apache-2.0)
- **libpfs3** — PFS3 (Professional File System III) reading and writing, the
  filesystem a real PiStorm card uses (LGPL-3.0-or-later), with `byteorder`
  (MIT / Unlicense) and `thiserror` (MIT / Apache-2.0) beneath it. The one
  weak-copyleft dependency inside `core/`, accepted deliberately in place of a
  second filesystem writer of ART's own — see `deny.toml`'s allow list for the
  reasoning and its cost
- **trash** — sending a file on the user's own disk to the Windows Recycle
  Bin (MIT), with `urlencoding` (MIT) and the `windows` 0.56 family —
  `windows-core`, `windows-implement`, `windows-interface`, `windows-result`
  (all MIT / Apache-2.0) — beneath it. **Outside `core/`**, in
  `tools/recycle_bin.rs`, because it calls `IFileOperation`: `core/hostfs`
  declares the trait and this implements it, the same split
  `core::preload::VolumeFormatter` and `tools/hst_imager.rs` already have. It
  is a crate rather than a COM binding of ART's own because the alternative is
  hand-written `IFileOperation` lifetime management for the one operation in
  ART that removes a user's file, and `default-features = false` drops
  `chrono`, which it needs only to *read* the bin back — something ART never
  does
- **png** — PNG reading and writing (MIT / Apache-2.0). Read-only in the
  application: `core/picture` decodes a user's own PNG wallpaper into plain
  RGB pixels ahead of quantisation and ILBM encoding; the encoder half is
  used only by `core/picture`'s own tests, to build a synthetic fixture
  independent of the decode path under test. Pure Rust, launches nothing and
  reaches no network, so it stays inside `core/`'s platform-independence rule
  the same way `delharc` and `sevenz-rust2` do
- **jpeg-decoder** — JPEG reading (MIT / Apache-2.0). Same role as `png`
  above, for the other format `core/picture::decode` accepts.
  `default-features = false` drops the `rayon` feature (on by default
  upstream), which spawns a thread pool to parallelise the IDCT step that a
  one-shot wallpaper conversion does not need — so this crate brings no
  transitive dependency of its own
- **i18next / react-i18next** — internationalization (MIT)
- **zustand** — state management (MIT)
- **react-router-dom** — routing (MIT)

Each of these projects is gratefully acknowledged.

## Derived data

ART ships one table it did not measure itself:

- **`src-tauri/src/core/rom/remus.rs`** — 154 Kickstart dumps, each named and
  placed by the checksum the ROM stores about itself. Generated by
  `scripts/rom-table-check.py` from the **Remus split database** distributed
  with **amitools** (© Christian Vogelgsang, **GPL-2.0-or-later**), and
  verified against it on every CI run so the two cannot drift. GPL-2.0-or-later
  is compatible with ART's GPL-3.0-or-later; amitools itself is not bundled,
  linked or shipped — only this derived table of facts is, and the generator
  that reproduces it is committed beside it.

  Why derived rather than hand-listed: ART's own ten hand-written hashes had no
  recorded provenance and matched none of the 29 Kickstart dumps they were
  measured against (ART-104), and only a per-build identity can tell
  `Kickstart 40.68 (A1200)` from `Kickstart 40.68 (A4000)`.

- **`src-tauri/src/core/osinstall/media_hashes.json`** — 186 rows identifying
  known AmigaOS install media by MD5. Adopted, not measured by ART, from
  [`rootrootde/emu68hatcher`](https://github.com/rootrootde/emu68hatcher)
  (**MIT**), file
  `src/main/python/emu68hatcher/data/reference/install_media_hashes.yaml`,
  fetched 2026-09-06 and converted to JSON without regenerating a single
  value; the file's own `$comment` field carries the same provenance line so
  a reader who opens it directly is told too. `core/osinstall/mediahash.rs`
  is the reader — data, never a code path, so a future correction to the
  table is a JSON edit. ART adds one file of its own beside it,
  `media_hashes_confirmed.json`, recording which of the 186 rows a real disk
  *here* has actually matched (generated by `scripts/media-table-check.py
  --emit-confirmed` from a real run, re-derivable rather than hand-edited) —
  that distinction, confirmed versus merely adopted, is ART's own and is not
  in Hatcher's table.

- **`src-tauri/src/core/firstboot/scripts/SD0pi3`** and **`SD0pi4`** — the two
  DOSDriver mountlists for Emu68's FAT boot partition. Adopted, not measured
  by ART, from [`rootrootde/emu68hatcher`](https://github.com/rootrootde/emu68hatcher)
  (**MIT**), commit `3f38b22`, without changing a value.

## External tools (not bundled)

ART may invoke user-installed external tools. These are **not distributed**
with ART. Users must obtain them from their official sources:

- **WinUAE** — © Toni Wilen (freeware). Source: https://www.winuae.net/
- **FlashFloppy** — MIT, by Keir Fraser. Source:
  https://github.com/keirf/FlashFloppy
- **LHA** — license per upstream. Source: official Amiga archive tool sites.
- **hst-imager** — MIT, © Henrik Nørfjand Stengaard. Source:
  https://github.com/henrikstengaard/hst-imager. ART launches it through
  `tools/hst_imager.rs` as the **named fallback** for the two typed gaps
  `core/preload/native.rs` refuses by name (ART-113, ART-117); the native
  path is the default and runs first for every step. The licence above was
  read from the `license.txt` shipped beside `hst.imager.exe` in the
  1.6.616 build, not recalled.

## Amiga-side packages carried under permission

Four entries in `core/sources/bundle`'s catalogue (§41.5.7) carry a
`permission` field: ART's download screen shows a warning above the tick for
each before it can be fetched, and this is the record the owner's own
requirement — *"lisansa da ekleriz"* — asks for. ART is not the rights holder
for any of these; the owner is obtaining the permissions separately.

- **Picasso96** — © Individual Computers (Jens Schonfeld). Shareware; the
  only legal purchase is from Individual Computers.
- **iBrowse** — © the iBrowse development team. Demo build, distributed under
  the Emu68 Imager's own permission from ibrowse-dev.net.
- **SetPatch 44.38** — © Cloanto Corporation. Amiga Forever files, carried
  under Cloanto's permission.
- **Workbench-Library 40.5** — © Cloanto Corporation. Amiga Forever files,
  carried under Cloanto's permission.

## Copyrighted content

ART **never** distributes:
- commercial Amiga games,
- commercial AmigaOS / Workbench,
- copyrighted Kickstart ROMs,
- pirated software.

ART works exclusively with **user-owned** files. Users are responsible for
the legal status of the files they manage with ART.
