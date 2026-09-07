# Project Status

**The single source of truth for where ART actually is.**

Every other document describes intent (the spec, `roadmap.md`) or mechanics
(`architecture.md`, `CLAUDE.md`). This one describes reality. When they
disagree, this file wins — and if this file disagrees with the code, fix this
file.

Update it at the end of any session that changes what works. Keep it short:
each round's own story is one row in the [session log](session-log.md), its
reasoning is under `docs/superpowers/` and `.superpowers/sdd/`, and the defect
register is [ISSUES.md](ISSUES.md). Nothing here is retold from those.

- Known defects and technical debt: [ISSUES.md](ISSUES.md)
- Feature-by-feature implementation state: [FEATURES.md](FEATURES.md)

---

## Snapshot

Every row is one current fact and the measurement behind it. A claim here is
only valid if the command that proves it was actually run — do not carry a
PASS forward on faith.

| | |
|---|---|
| **Last updated** | 2026-09-07 — round 5 of the Emu68 Hatcher intake (first boot on the Amiga, phases 1–2) merged to `main` as `1cb5ce1` and pushed. Per-round detail is the [session log](session-log.md) |
| **Version** | **0.9.0**, released 2026-09-04. The number lives in three files (`package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`) and `release.yml` refuses a tag that disagrees with them. Deliberately not 1.0: the bar is a card ART built, flashed and booting a real A500 — not a bigger feature count |
| **`main` / `origin`** | Identical at `32651c9`, pushed 2026-09-07 (`git rev-parse main origin/main`; the push was `722a8c5..32651c9`). No live phase branch. CI run 34116056492 on that commit was still `in_progress` when this was written — **result not yet recorded** |
| **Tests — Rust** | `cd src-tauri && cargo test --lib -- --skip artwork` on the merged `main`, 2026-09-07: `test result: ok. 2948 passed; 0 failed; 51 ignored; 0 measured; 89 filtered out; finished in 35.35s`. **`--skip artwork` is the only honest number here** until [ART-261](ISSUES.md)'s antivirus exclusions land — a full `cargo test --lib` dies mid-run with exit 0 and no summary. **Quote the `test result:` line, never the exit code**, and run the suite twice before merging ([ART-059](ISSUES.md#fixed)). The 51 ignored are the real-material hooks, env-gated and run by hand — command lines below |
| **Tests — frontend** | `pnpm test` on the merged `main`, 2026-09-07: `Test Files 87 passed (87)`, `Tests 1178 passed (1178)` |
| **Lint, format, clippy** | Clean on the merged `main`, 2026-09-07: `pnpm lint` (run unpiped — a pipe reports `tail`'s status, not `tsc`'s), `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` |
| **Sweeps** | Re-run 2026-09-07 on the merged `main`, all clean: `scripts/control-byte-sweep.py`, `scripts/scratch-root-sweep.py`, `scripts/contrast-check.py --quiet` (**113/113** pairs, both themes). `scripts/scratch-counter-sweep.py`, same day: `already had a counter: 24`, `needing a counter: 1` — and that one is a known false positive, a helper keyed on the thread id ([ART-235](ISSUES.md)) |
| **Build** | The last full `pnpm tauri build` was the 0.9.0 bundle, 2026-09-04 (5m 47s) — `Amiga Retro Toolkit_0.9.0_x64_en-US.msi` and `_x64-setup.exe`, **not code-signed**, which the README says rather than leaving to SmartScreen. Not re-run since |
| **i18n** | `src/i18n/en.json` and `tr.json`: **2078** leaf keys each, counted 2026-09-07 on the merged `main`. Parity — key sets, empty values, interpolation variables — is enforced by `pnpm test`, so count them rather than quoting this |
| **Open defects** | **19**, newest [ART-274](ISSUES.md), counted 2026-09-07 with `awk '/^## Open/{f=1} /^## Fixed/{f=0} f' docs/ISSUES.md \| grep -c '^\*\*ART-'` |
| **Feature rows** | **216 marked rows** in [FEATURES.md](FEATURES.md) — **159 green, 26 amber, 21 not started, 6 stubs, 4 deferred to v2** — counted 2026-09-07, one marker per table row, taken from the first cell in that row that is exactly a State marker. **State the method with the number**: counting marker *cells* instead gives a different figure, and an earlier count of "233 rows" left behind no method and cannot now be reproduced |
| **amitools oracle** | 53 checks, both directions (`scripts/oracle-check.py`, blocking in CI) — including a filesystem driver ART embedded in an RDB and `rdbtool` extracted back out byte-for-byte |
| **Kickstart table** | 154 dumps (`core/rom/remus.rs::REMUS_ROMS`, counted 2026-09-07), generated from amitools' Remus split database and re-verified against it on every CI run (`scripts/rom-table-check.py`, [ART-104](ISSUES.md#fixed)). Licensed Amiga Forever ROMs are first-class input ([ART-128](ISSUES.md#fixed)): decoded with the `rom.key` beside them, then identified like any dump |
| **Install-media table** | 186 rows, 186 distinct MD5s, adopted from `rootrootde/emu68hatcher` (MIT). `scripts/media-table-check.py` against the owner's own AmigaOS 3.2 ADFs, 2026-09-06: **35 verified, 151 unverified, 0 conflicting**; the Rust twin agrees. Both fail when a directory verifies nothing ([ART-265](ISSUES.md#fixed)) |
| **7-Zip oracles** | The card's FAT32 boot partition read back by 7-Zip — type, geometry, label, names and every file's bytes (`scripts/fat-oracle-check.py`); and 4 disc fixtures — Joliet, ISO9660-only, raw Mode 1, raw Mode 2/XA — names, sizes, SHA-256 per file (`scripts/iso-oracle-check.py`). Neither in CI |
| **hst-imager PFS3 oracle** | Both directions, local only (`scripts/pfs3-oracle-check.py`): ART writes a volume through `NativeFormatter` and `hst-imager fs dir -r` reads it back; `hst-imager` formats and fills one and ART reads it back through `libpfs3`, SHA-256 per file plus protection strings |
| **ILBM / prefs oracle** | `scripts/ilbm-oracle-check.py` — ffmpeg agrees with ART's ByteRun1 encoder pixel-for-pixel on **8 of 8** fixtures. Against the owner's own AmigaOS 3.9 material, **15 of 24** `.prefs` files are genuine `FORM PREF` containers and all 15 round-trip byte-for-byte through the real rebuilder (`replace_bodies(&[])`); the other 9 are third-party non-IFF formats, reported apart |
| **Icon oracle** | `scripts/icon-oracle-check.py` round-trips real `.info` files through `core/amigaicon`'s reader **and its four writers**, against the owner's own material: `checked=798 failed=0 no_drawer_data2=0 lossy_tooltypes=69`. Its first real run found [ART-249](ISSUES.md#fixed), a writer offset landing inside the wrong struct that no unit test could see. The 69 lossy ToolTypes icons are counted apart, never as passes ([ART-250](ISSUES.md)) |
| **cargo-deny** | advisories, bans, licences, sources — all ok |
| **MSRV** | 1.93 (raised from 1.77 on 2026-08-12, for a maintained 7z decoder) |
| **Published** | <https://github.com/tolon/Art> — public, `main`, **GPL-3.0-or-later**. [v0.9.0](https://github.com/tolon/Art/releases/tag/v0.9.0) is released with both installers attached (NSIS 5.1 MB, MSI 6.3 MB), built by `release.yml` from the tagged commit after CI went green on it. The built `.exe` was launched and answered before the draft was published |
| **Real hardware** | **Bare metal, 2026-08-12**: `test/art-bootable-test.adf` booted a real **A500/A500+** (Kickstart 3.9) from a **Gotek** to an AmigaDOS CLI. Photographed. In emulation, 2026-08-16: a PFS3 volume ART formatted and filled booted a licensed Kickstart 3.1 to `hello from ART`, and a full AmigaOS 3.2 tree ART built booted a licensed V47 A1200 ROM to a clean Workbench. **No card or hard disk ART built has reached real hardware**, and physical magnetic media is still untouched — a Gotek is not a mechanical drive |
| **Seen on a screen** | The ADF, hard-disk, PiStorm, WHDLoad, Files and Settings screens have been opened and driven by a person (2026-08-12 onwards); the Collection's Play path ran real titles on 2026-08-18/21. **[ART-062](ISSUES.md)** narrows to the screens still unopened (Aminet, Collection, Gotek, ROM, WinUAE, Tools) and to the Turkish catalogue, of which a handful of strings out of 2078 have been read by someone who speaks it |

Reproduce the numbers above:

```bash
pnpm lint                                              # TypeScript (run unpiped)
pnpm test                                              # frontend unit tests (i18n parity, phrase keys)
cd src-tauri && cargo fmt --check                      # formatting
cd src-tauri && cargo clippy --all-targets -- -D warnings
cd src-tauri && cargo test --lib -- --skip artwork     # twice — ART-059; --skip artwork per ART-261
pip install amitools && python scripts/oracle-check.py # independent cross-check
python scripts/iso-oracle-check.py                     # the disc reader vs 7-Zip (needs 7z; not in CI)
python scripts/fat-oracle-check.py                     # the card's boot partition vs 7-Zip (needs 7z; not in CI)
python scripts/pfs3-oracle-check.py                    # PFS3, both directions, vs hst-imager (needs hst.imager.exe; not in CI)
python scripts/catalogue-check.py                      # every shipped bundle path vs Aminet itself (not in CI, it leaves the machine)
python scripts/zoom-check.py                           # the shell's widths, in a real browser (needs `pnpm dev`)
python scripts/osbuilder-strip-check.py                # the OS Builder's step strip: per kind, both languages,
                                                       #   three Application Sizes (needs `pnpm dev`)
python scripts/contrast-check.py --quiet               # every colour pair in both themes, against WCAG (in CI)
python scripts/control-byte-sweep.py                   # no stray BEL/BS/VT/FF/ESC in tracked text (ART-216; in CI)
python scripts/scratch-root-sweep.py                   # every production staging site goes through the chosen
                                                       #   scratch root, not %TEMP% (ART-196; in CI)
python scripts/scratch-counter-sweep.py                # test scratch names unique *within one process*
                                                       #   (ART-059/164/173; not in CI — one known false
                                                       #   positive, ART-235)
python scripts/rom-table-check.py                      # the Kickstart table vs amitools' Remus data (ART-104; in CI)
python scripts/vhd-oracle-check.py                     # the dynamic VHD writer vs Microsoft's Get-VHD
                                                       #   (needs the Hyper-V PowerShell module; not in CI)
cd src-tauri && cargo deny check                       # licences and advisories
pnpm tauri build                                       # full bundle (slow)
```

**The `#[ignore]`d hooks are the other half**, and they are where ART meets
material nobody here wrote. `cargo test -- --ignored` lists them; each is
env-gated on the owner's own disks and none is in CI.

```bash
# The OS-install engine against the owner's own AmigaOS 3.2 ADFs.
cd src-tauri && ART_OSINSTALL_MEDIA="E:\amiga\Amigatolon\paketler\3.2\AmigaOs 3.2\ADF" \
  ART_OSINSTALL_ROM="E:\amiga\Amigatolon\kickstart\Kickstart v3.1 rev 40.68 (1993)(Commodore)(A1200).rom" \
  ART_OSINSTALL_DEST="E:\amiga\ProjeART\dist-3.2" \
  cargo test run_the_real_engine_against_the_users_own_media_when_asked -- --nocapture --ignored

# Puts an existing tree onto a card; it does not build one. Point it at
# dist-3.2 above and it reproduces ART-113's non-ASCII PFS3 refusal exactly.
cd src-tauri && ART_OSINSTALL_DEST="E:\amiga\ProjeART\dist-3.2-witness" \
  ART_CARD_OUT="E:\amiga\ProjeART\dist-3.2-witness-card.hdf" \
  cargo test build_the_real_dist_tree_onto_a_card_when_asked -- --nocapture --ignored

# The only one that carries the whole tree through the path the product
# actually runs — `run_with_fallback`, native first. `ART_HST` is optional:
# without it the run measures the native path alone and ends on the refusal a
# user with no hst-imager would see. Delete the output image first; SAFE_CREATE
# refuses an existing one.
cd src-tauri && ART_OSINSTALL_DEST="E:\amiga\ProjeART\dist-3.2" \
  ART_CARD_OUT="E:\amiga\ProjeART\dist-3.2-fallback.hdf" \
  ART_PFS3="E:\amiga\Amigatolon\hstimager\pfs3aio" \
  ART_HST="E:\amiga\Amigatolon\hstimager\hst.imager.exe" \
  cargo test carry_the_real_dist_tree_through_the_fallback_path_when_asked -- --nocapture --ignored

# ART-159's two language components, against the disc they were read off.
# Died mid-run four times in four on 2026-09-06 — exit 0, no `test result:`
# line, always at 561 of 2391 files. ART-261 names the cause (the scanner
# reacting to `art_lib`), so a clean run here is still owed rather than had:
# exclude src-tauri\target\ and D:\tmp\art-tests\ from the scanner first.
cd src-tauri && ART_159_ISO="E:\amiga\Amigatolon\iso\AmigaOS39.iso" \
  ART_159_DEST="E:\amiga\ProjeART\art159-tree" \
  cargo test --release build_the_real_39_language_components_when_asked -- --nocapture --ignored

# Builds AmigaOS 3.2.2 from the owner's own base and update media and then
# **asks the tree what it is** — `Prefs/Env-Archive/Versions/Release` is
# written by the release, so the claim comes from a file Hyperion wrote rather
# than from ART's own dropdown. Measured 2026-09-05: 4 052 files, 295 drawers,
# 20 667 671 bytes, and the tree says `Release 3.2.2`. The ROM matters —
# `kicka1200.rom` is 47.96, so both update Modules components switch on and the
# base's own stays off. Unpack `Update3.2.2.lha` yourself; ART never reads it.
cd src-tauri && ART_322_BASE="E:\amiga\Amigatolon\paketler\3.2\AmigaOs 3.2\ADF" \
  ART_322_UPDATE="E:\amiga\ProjeART\layer-research\Update3.2.2\ADFs" \
  ART_322_ROM="E:\amiga\Amigatolon\paketler\3.2\AmigaOs 3.2\ROM\kicka1200.rom" \
  ART_322_DEST="E:\amiga\ProjeART\dist-3.2.2" \
  cargo test build_the_real_322_tree_when_asked -- --nocapture --ignored

# The icon oracle's Rust half. `scripts/icon-oracle-check.py` extracts the
# `.info` files and drives this; run it directly only when debugging that script.
cd src-tauri && ART_ICON_DIR="<a folder of extracted .info files>" \
  cargo test round_trip_every_icon_in_a_folder_when_asked -- --nocapture --ignored

# The install-media hash table's outside check — rom-table-check.py's sibling.
# Read-only: it only ever opens a file to hash it. `--expect-verified N` holds
# it to a number the CALLER states; 35 is this machine's measurement against
# this machine's disks and is deliberately never hardcoded, because a user with
# a 3.1 set must not meet a failing check.
python scripts/media-table-check.py "E:\amiga\Amigatolon\paketler\3.2\AmigaOs 3.2\ADF" \
  --expect-verified 35

# The same run, rewriting ART's own record of which rows this machine has
# confirmed — core/osinstall/media_hashes_confirmed.json, which the screen's
# "checked against a real disk here" sentence reads. `--against` is that
# sentence, so word it as a claim ART will put on screen. It appends rather
# than replacing (ART-267).
python scripts/media-table-check.py "E:\amiga\Amigatolon\paketler\3.2\AmigaOs 3.2\ADF" \
  --emit-confirmed --against "the ART author's own AmigaOS 3.2 install set, 35 ADFs"

# The Rust twin of the same check, through core's own row_for/rows rather than
# the script's independent re-implementation.
cd src-tauri && ART_MEDIA_DIR="E:\amiga\Amigatolon\paketler\3.2\AmigaOs 3.2\ADF" \
  ART_MEDIA_EXPECT_VERIFIED=35 \
  cargo test every_real_disk_in_a_folder_is_looked_up_through_cores_own_code_when_asked -- --nocapture --ignored

# The only test in ART that opens WinUAE against a real first-boot tree. Never
# touches the tree it is pointed at — `stage_with` copies it, first boot is
# written into the copy, and the copy is what boots; a failure keeps the copy
# and prints where it is.
cd src-tauri && ART_FIRSTBOOT_TREE=E:/amiga/ProjeART/art205-32 \
  ART_FIRSTBOOT_ROM="E:/amiga/Amigatolon/paketler/3.2/AmigaOs 3.2/ROM/kicka1200.rom" \
  ART_WINUAE="C:/Program Files/WinUAE/winuae64.exe" \
  cargo test rehearse_the_real_tree_when_asked -- --ignored --nocapture

# The real Aminet — every shipped mirror asked separately, because failover
# stops at the first that answers and a dead one in position two is invisible
# in an ordinary sync. Leaves the machine; never in CI.
cd src-tauri && cargo test live_aminet -- --ignored --nocapture
```

---

## Picking up next session

*One block, kept current.* Every round used to add its own "Start here" and
leave the previous one below it; on 2026-09-04 four such blocks were collapsed
into this one, because the oldest still contradicted the newest. **Update this
block — do not stack another on top of it.**

### Start here (2026-09-07)

1. **`main` is pushed and CI has not answered yet.** `main` and `origin/main`
   are both `32651c9`; the push was `722a8c5..32651c9`. GitHub Actions run
   **34116056492** on that commit was `in_progress` when this was written, so
   **its result is not recorded here** — check it before treating `main` as
   green on CI. Local lint, fmt, clippy, both suites and the three blocking
   sweeps were clean on the merged tree the same day (Snapshot above).
2. **Round 5 of the Emu68 Hatcher intake — first boot on the Amiga — has
   phases 1 and 2 merged** (`1cb5ce1`, `--no-ff`, branch deleted). What it
   built is two [FEATURES.md](FEATURES.md) rows and one
   [session log](session-log.md) entry; the reasoning and the twelve task
   reports are local-only under
   `.superpowers/sdd/2026-09-07-firstboot-phase-1-2/`.
3. **Phase 3 is next: the reboot itself, plus the wizard's `90-prefs` /
   `ART-FirstBootWB`.** It inherits one measured fact from phase 2 —
   **`C:Reboot` is present on the owner's AmigaOS 3.2 tree (`art205-32`) and
   absent on both AmigaOS 3.9 trees (`art159-boot`, `art205-39c`)** — so phase
   3 cannot assume the reboot command exists and must check for it the way
   phase 2 checks for `C:Sort`. Phase 4 (`run`/`unpack` package steps) is not
   designed yet.
4. **[ART-261](ISSUES.md) is open and still changes how you quote numbers.**
   The owner's antivirus kills a full `cargo test --lib` — exit 0, no
   `test result:` line, no `FAILED`. `cargo test --lib -- --skip artwork` is
   the only honest Rust number here until exclusions are added for
   `src-tauri\target\`, the test scratch root and `art_lib`'s build output.
   An exit code cannot tell a finished suite from a killed one.
5. **[ART-274](ISSUES.md) is the newest open entry** and it is architectural:
   `core/winuae.rs` spawns `winuae64.exe` from inside `core/`, which is the
   exact shape the trait rule exists to prevent, and this round added a second
   consumer of it. Not a user-visible defect; the fix is an
   `EmulatorLauncher` trait in `core/` with the spawn in `tools/`.

### Where the work stands

One paragraph per live area. Per-feature state is [FEATURES.md](FEATURES.md),
per-defect state is [ISSUES.md](ISSUES.md), per-round narrative is the
[session log](session-log.md). None of it is retold here.

- **The PiStorm card path** — SD-0, SD-1, SD-2 and SD-4 are built, SD-3's two
  named gaps are merged, SD-5 is part-built. The gaps still open are one line
  each in the stage table below. What is left in SD-1 is **not code**: a card
  flashed and an A500 booted, which is also the project's 1.0 bar.
- **The OS Builder** — builds a distribution tree from the owner's own
  AmigaOS 3.2, 3.2.2 (layered base + update) and 3.9 media, carries wallpaper,
  screen mode, shell defaults, network config and arranged drawer icons, and
  identifies install media by content hash. Its remaining recipe gap is
  [ART-251](ISSUES.md) (no rule for `Utilities` or `WBStartup`).
- **First boot on the Amiga** — phases 1 and 2 built (dispatcher, the
  `10-hardware`/`20-aux`/`30-datatypes` steps, the `S/FirstBoot.log` report,
  reading that report back off a card's FAT, FFS or PFS3). Phases 3 and 4 are
  not built; the Pi3/Pi4 branch of `10-hardware` is unmeasured because it needs
  a real card.
- **Aminet and the package catalogue** — Stage A and Stage B are both built
  and tested, catalogue sync through install-to-HDF and the update view. The
  AI layer (§45.5) is deliberately deferred to v2, the owner's decision of
  2026-08-24.
- **The Emu68 Hatcher intake** (`rootrootde/emu68hatcher`, MIT) — scoped at six
  rounds of taking what is worth taking. Five have landed on `main`: prefs and
  wallpaper (1), drawer icons (2), refusal evidence (3), media identification
  by hash (4), and first boot phases 1–2 (5). Round 5's own phases 3 and 4 are
  the next code work; nothing else is queued.
- **The 2026-09-04 work list**
  ([superpowers/specs/2026-09-04-work-list.md](superpowers/specs/2026-09-04-work-list.md))
  — items 3, 5 and 7 are closed. **Every item left on it is the owner's**:
  items 1, 2 and 4 need a person (a card, a screen driven by hand, a real
  catalogue looked at) and item 6 is blocked on the owner bringing a real
  distribution image.
- **v0.9.0 is out and in the community's hands.** Reading what came back is
  still the first thing a session does — issues on the repository, and whatever
  the owner was told directly. A report that went *well* counts: the nine gaps
  the README lists are unverified in both directions. Nothing had come back as
  of 2026-09-05: no issues, three release downloads.

### How a release goes out now

Recorded because it did not exist before 2026-09-04 and nobody should have to
work it out twice. Raise the version in **all three** files
(`package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`), close
the CHANGELOG's `[Unreleased]` section under the new number, push, **wait for
CI to go green on that commit**, then push a `vX.Y.Z` tag.
`.github/workflows/release.yml` does the rest and refuses rather than shipping
something wrong: it stops if the tag and the three version fields disagree,
and it stops if the CHANGELOG has no section for that version. It publishes a
**draft**, so a person presses the last button — and before pressing it, launch
the built `.exe` once. "It compiled" is not the same claim as "it opens".

### Blocked on a person, not on code

1. **A card flashed and an A500 booted.** Nothing SD-1 or SD-2 has built has
   reached real hardware. The software side is complete; what is missing is
   physical — a microSD card, a USB reader and an HDMI cable, the last plugged
   in **before** power or the VPU never configures the port and there is no
   RTG that session.
2. **[ART-118](ISSUES.md) — the OS Builder's install screen in a real
   window.** Deeper interaction than a smoke test (filling the fields, ticking
   a component, running Plan or Verify) crashes the renderer reproducibly with
   an access violation in every headless Chrome and Edge combination tried.
   This one needs `pnpm tauri dev` and a human, not a substitute.

Neither needs a design decision or more code first.

---

## What ART is, and the ceiling that comes with it

**Every established Amiga distribution builder runs the install inside an
emulator** — HstWB Installer, AmiKit, AmigaSYS and ClassicWB all do, using the
user's own OS files. ART is not in that family: `amitools`'s `xdftool` does
what ART does — host-side placement plus a metadata sidecar (`.xdfmeta`, ART's
`.uaem`) — so ART sits in the **tooling** family. That has a ceiling, and it
is a live constraint rather than a defect:

- **[ART-166](ISSUES.md)** (open) — a BoingBag's payload is a
  password-encrypted ZIP and the password lives in an Amiga executable. A
  host-side placer cannot read it. **The owner's decision, 2026-08-19, so it
  is not re-litigated by accident: no bypass of the BoingBag password will be
  written.** The way past it is to run the package's own installer on an Amiga.
- **The Amiga side is no longer hypothetical.** `core/amigainstall` runs a
  package's own installer under WinUAE, and `core/firstboot` runs a tree's own
  setup on the real machine at first boot. What that path can and cannot do
  yet is the FEATURES rows for those two, not this paragraph.

Two rules came out of the same round and outlive it, both now in CLAUDE.md:
**ask the artefact what it is** — a booted system answers `version full`,
where a copyright line, a directory name and a file size are each consistent
with several answers (this is how a 3.5 tree was recorded as 3.9 for most of a
day, [ART-169](ISSUES.md#fixed)) — and **a file with no `$VER:` marker is
invisible to a collision classifier**, so any "did the update take?" check
built on collision classes alone will be confidently wrong about the one file
that matters ([ART-170](ISSUES.md#fixed)).

---

## Stage plan

This supersedes the phase numbering in `roadmap.md` for scheduling.
`roadmap.md` still defines *what each phase contains*; this defines *what order
the remaining work happens in and why*.

### Closed stages

Each of these is finished. The detail is the [session log](session-log.md) row
for its date and the FEATURES rows it produced; nothing about them is retold
here.

| Stage | Status | Closed | Where the detail lives |
|---|---|---|---|
| **Stage 1** — Data safety (`core/safety`, atomic writes, generational backups, bounds-checked blocks, the AmigaDOS hash fix) | ✅ | 2026-08-09 | [session log](session-log.md) · [ART-021…ART-029](ISSUES.md#fixed) |
| **Stage 2** — Workflow Engine (`core/workflow/builtin.rs`, `Navigate`/`Execute`, the drop panel) | ✅ | 2026-08-09 | [session log](session-log.md) · [architecture.md](architecture.md) |
| **Stage 3** — Systematic audit (HDF/RDB held nine defects, three critical; sparse HDF creation) | ✅ | 2026-08-09 | [ISSUES.md](ISSUES.md#fixed) |
| **Stage 4** — Shared infrastructure (jobs, oplog, UX modes — the hard prerequisites Stage 5 names) | ✅ | 2026-08-09 | [session log](session-log.md) |
| **Stage W** — Writing into volumes (`core/volume/write/`: OFS, FFS, INTL at any geometry) | ✅ | 2026-08-09 | [FEATURES.md](FEATURES.md) |
| **§82** — One-click WHDLoad install, end to end | ✅ | 2026-08-09 | [FEATURES.md](FEATURES.md) |
| **Phase 0a** — Live bugs fixed, one filesystem writer | ✅ | 2026-08-10 | [session log](session-log.md) |
| **Phase 0b** — Dead code removed, the interface speaks Turkish | ✅ | 2026-08-10 | [session log](session-log.md) |
| **Phase 1a** — The Commander (real selection, batch operations, sorting, filter) | ✅ | 2026-08-11 | [session log](session-log.md) |
| **Phase 2a** — Content-first detection and containers | ✅ | 2026-08-12 | [plan](superpowers/plans/2026-08-11-phase-2a-content-detection-and-optical.md) |
| **Phase 2b** — The Files screen, done right | ✅ | 2026-08-12 | [plan](superpowers/plans/2026-08-12-phase-2b-commander-ui.md) · [brief](brief-files-commander-ui.md) |
| **Stage 5 / §41.5** — Aminet: catalog sync, search, download, readme, Collection, install to HDF, update view, and the 14-set package-bundle catalogue | ✅ | 2026-08-09, extended to 2026-08-24 | [FEATURES.md](FEATURES.md) · [design-software-sources.md](design-software-sources.md). Bundles are download-only in phase 1 — nothing installs onto an Amiga volume yet; `net/live_aminet.rs` (2026-08-24) is the re-runnable check against the real mirrors |
| **Stage 5 / §45.5** — the AI workflow layer | 🅥2 | deferred 2026-08-24 | [design-ai-layer.md](design-ai-layer.md) — the owner's decision: the only planned feature that adds no capability ART lacks, so it waits until the ground under it has been driven |

Two caveats from Phase 2b that nobody has closed since: **the light theme has
never been looked at** (its tokens derive from the dark ones by role), and
**session restore is only half verified** — the write half reaches
`settings.json` and has tests (`src/lib/paneSession.ts`), but nothing has
closed and reopened ART to watch the read half. Both sit under
[ART-062](ISSUES.md).

### 🟡 PiStorm Image Builder — SD-0 … SD-5

**The project's largest feature and, per the owner, its point.** SD-0, SD-1,
SD-2 and SD-4 are built. SD-3's two named gaps are both built and merged —
G14 (prefs, wallpaper, screen mode, shell defaults, `core/amiganet/`'s network
half) and G16 (multiboot) — and SD-5 is part-built. Only the gaps still open
are listed below; a built gap is a [FEATURES.md](FEATURES.md) row, not a line
here.

Gap analysis: [sd-appliance-gap-analysis.md](sd-appliance-gap-analysis.md).
Prior-art teardown: [sd0-prior-art.md](sd0-prior-art.md). Card layout measured
off two real cards: [sd2-card-layout.md](sd2-card-layout.md).

| Gap | Stage | What is still missing (checked against the tree 2026-09-07) |
|---|---|---|
| **SD-1 exit** | SD-1 | Not code. Every gap in SD-1 is a ✅ row in [FEATURES.md](FEATURES.md); what is left is a card ART built, flashed, and an A500 booted from it. This is the 1.0 bar |
| **G10** | SD-2 | Built and closed 2026-09-05 (`core/gameindex/igame.rs`, `igamewrite.rs`). The one outstanding claim: **no `igame.data` ART wrote has been read by iGame on a real Amiga.** The AGS half was assessed 2026-08-25 and the recommendation is not to build it — `core/artwork` handles PNG and JPEG and not IFF, so the launcher would show a collection with no pictures |
| **G11** | SD-2 | Built (`core/layout/`, the `/layout` screen). **Not yet driven in `pnpm tauri dev` against real material, and no staging tree it built has been carried onto a card** |
| **SD-5 planner** | SD-5 | All five entries in the distro registry are `available: false` (`grep -c '"available": false' src-tauri/src/core/distro/*.json` → 5), so nothing yet plans a card from a named distribution. The capacity half is built — `core/card/capacity.rs` refuses an FFS partition past the 4 GB a pre-v46 Kickstart can address, which is a corrupted drive rather than an inconvenience |

Four decisions behind it, worth knowing before reading the code they produced:

- **ART never touches physical media.** Everything is built into a sparse image
  file through the existing tested paths, and there the job ends: the user
  flashes it with the imager they already have. §56's raw-device guard stays in
  the spec as the reason ART does not do this, rather than as a problem ART has
  to solve. This deleted the original G1 and G12 outright.
- **v1 targets Emu68 only.** The classic Linux/Musashi route is a different
  build and is out of scope in writing, not by omission.
- **Which Amiga is a parameter, not an assumption.** What varies by machine is
  data the build already carries — the Kickstart, the Emu68 config, the OS
  release, the partition geometry — not a code path. The hardware in the room
  is a classic PiStorm on a Pi 3A+, verified on an A500 and an A500+.
- **A distribution is configured in ART, not afterwards on the Amiga.**
  Wallpaper, WiFi, prefs and Startup-Sequence are build inputs (G14), each
  **edited in place rather than regenerated** — the rule §39/§40 already impose
  on `FF.CFG` and `cmdline.txt`, applied to AmigaOS. The WiFi passphrase is
  deliberately not remembered and stays out of the oplog, the manifest and any
  AI prompt.

---

## Phase completion criteria

Carried over from `roadmap.md`; a stage is not done until all of these hold.

- Build: PASS
- Tests: PASS
- No critical errors
- No obvious data-loss risk
- UI remains responsive
- Documentation updated (this file, [ISSUES.md](ISSUES.md), [FEATURES.md](FEATURES.md))
- CHANGELOG updated

---

## The machine, and where things live

- `E:\amiga\Amigatolon` — the owner's own material. **Read from, never written
  to.** Inventoried 2026-08-13: A1200 Kickstarts in both families (3.1 rev
  40.68, 3.2's `kicka1200.rom`, 3.2.1's `A1200.47.102.rom`), the full AmigaOS
  3.2 ADF set with its CD and the 3.2.1/3.2.2 updates, the 3.9 ISO with both
  BoingBags, every release from 1.0 to 3.1 as ADFs, and both real PiStorm
  distributions (CaffeineOS 9317, MultibootOS 2.2) as images. WinUAE, 7-Zip
  and amitools are installed.
- `E:\amiga\ProjeART` — where trial output goes. **Not C:, not D:** (the
  owner's rule, 2026-08-14), and not F: either, which is a 499 MB drive a
  300 MB oracle image will not fit on. `ART_SCRATCH` overrides it in the
  scripts. It holds `card.img` (a card ART built from the real release),
  `dist-3.2\` (the real AmigaOS 3.2 tree — read from, not rebuilt idly, since
  rebuilding costs the same real-media run), and `screen-test.img` +
  `preload-tree\`, staged for a screen run. That card's one Amiga partition is
  `SDH0`, FFS, 0.90 GiB, in MBR slot 2 — so a correct preload plan has **two**
  steps and no driver-embed step, because Kickstart carries FFS. If the plan
  shows three, something is wrong before anything is formatted.
- The test suite's own scratch goes to `D:/tmp/art-tests`, forced by
  `src-tauri/.cargo/config.toml` ([ART-184](ISSUES.md#fixed)) — machine-local
  relief, not a fix.
- Rebuilding the card is the fastest way to see the whole engine work at once:

  ```bash
  cd src-tauri
  ART_CARD_ZIP="E:\amiga\Amigatolon\Emu68\Emu68-pistorm.zip" \
  ART_CARD_ROM="E:\amiga\Amigatolon\kickstart\Kickstart v3.1 rev 40.68 (1993)(Commodore)(A1200).rom" \
  ART_CARD_OUT="E:\amiga\ProjeART\card.img" \
    cargo test build_real_card_when_asked -- --nocapture
  ```

  It prints the payload it chose, where each partition landed, and reads the
  finished card back. Delete `card.img` first — `build_card` refuses to write
  over one that is already there.

## What the owner brings, and what each thing settles

Each of these changed a decision rather than merely being nice to know.

- **The target board is settled by hardware, not preference**: an A500 with a
  classic PiStorm on a Raspberry Pi 3A+, plus a Gotek — which is
  `PistormHardware::default()` already. Consequences that are facts rather
  than parameters: the kernel archive is `Emu68-pistorm.zip` on the stable
  line (**not** the same file as on the 1.1 alpha — [ART-091](ISSUES.md#fixed)),
  the SD driver is `brcm-sdhc.device`, and `genet.device` is irrelevant (the
  3A+ has no ethernet; its WiFi is `wifipi.device`).
- **More than one real Amiga.** The rule is unchanged — a claim records
  *which* machine proved it — but there is more than one machine to prove
  things on.
- **Licensed Amiga Forever, desktop and mobile.** There is no Kickstart
  licence problem to design around, and the Cloanto-headered dumps are
  available — which `core/rom` already strips (`AMIROMTYPE1`). It is also why
  [ART-104](ISSUES.md#fixed) mattered: a licensed collection carries dumps
  whose checksums are not the ones a table copied from anywhere else will hold.
- **The owner wrote two Amiga-side programs and both are meant to ship in
  ART-built distributions**: **tolunnet**, a TCP/IP stack in Roadshow's place,
  and **tolunwifi**. Source at `D:\Projeler\tolunnet`. A stack the owner
  wrote is theirs to distribute, so ART Baseline can carry one outright rather
  than as a fetch task — which is what SD-0 concluded for anything not clearly
  redistributable. G14's network half is **built** against that program's own
  source (`core/amiganet/`), so nothing was reverse-engineered and Roadshow is
  not assumed as the default stack.

---

## Session log

Moved to [session-log.md](session-log.md) on 2026-09-04, and it is history: a
row describes the tree on the day it was written and is never rewritten.

**Add a row there, at the top, when work lands.** Nothing is duplicated back
here on purpose — two copies of a log is how this file's own "Last updated"
cell once ended up a day behind it.
