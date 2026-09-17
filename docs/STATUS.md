# Project Status

**The single source of truth for where ART actually is.**

Every other document describes intent (the spec, `roadmap.md`) or mechanics
(`architecture.md`, `CLAUDE.md`). This one describes reality. When they
disagree, this file wins — and if this file disagrees with the code, fix this
file.

Update it at the end of any session that changes what works. Keep it short:
each row below is **one current statement** — replace it, never prepend a
"Before that…" to it. History is the [session log](session-log.md) and git; a
round's reasoning is under `docs/superpowers/` and `.superpowers/sdd/`; the
defect register is [ISSUES.md](ISSUES.md), closed entries in
[ISSUES-archive.md](ISSUES-archive.md). Nothing here is retold from those.

- Known defects and technical debt: [ISSUES.md](ISSUES.md)
- Feature-by-feature implementation state: [FEATURES.md](FEATURES.md)

---

## Snapshot

Every row is one current fact and the measurement behind it. A claim here is
only valid if the command that proves it was actually run — do not carry a
PASS forward on faith. A row that quotes an older measurement says when and
where it was taken.

| | |
|---|---|
| **Last updated** | 2026-09-17 — the documentation cleanup is merged (`c866021`), and this file's `main`/`origin` row and "Picking up next session" block were brought up to it the same day. The last code change on `main` is card round 2's merge, `69d3a40` |
| **Version** | **0.9.4**, released 2026-09-15 — tag `v0.9.4` (annotated, on `e91f22e`), release run 34969262720 `success`, published on the owner's word with one NSIS and one MSI installer. Its notes are CHANGELOG's `[0.9.4]` section. The number lives in three files (`package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` — all `0.9.4`, checked 2026-09-17) and `release.yml` refuses a tag that disagrees with any of them, or a version with no CHANGELOG section |
| **`main` / `origin`** | `origin/main` is `c866021` (`git ls-remote`, 2026-09-17) — `docs-cleanup-0917` merged `--no-ff` and pushed, after `art-card-round-2` (`69d3a40`, CI run 35196578229 `success`); CI run 35203099787 on `c866021` `success`. One other local branch, `art-310-windows` (`d851174`, 12 commits not in `main`), is kept unmerged: ART-310 was fixed on `main` by another route, and [ISSUES-archive.md](ISSUES-archive.md) cites its research note through `git show art-310-windows:…` |
| **Tests — Rust** | `cd src-tauri && cargo test --lib` at `952f217` (card round 2's fix wave), 2026-09-17: `test result: ok. 3626 passed; 0 failed; 64 ignored; 0 measured; 0 filtered out` (one run; the code commit before it, `33d297b`, ran green twice). The residual commit `df71f37` touched `core/archive/lzx.rs` only and ran that module; CI run 35196578229 ran the whole suite on the merge, `success` |
| **Tests — frontend** | `pnpm test` after `3193e36` (card round 2, Task 12), 2026-09-17: `Test Files 112 passed (112)`, `Tests 1723 passed (1723)`. The only frontend change since is a doc comment in `src/lib/archive.ts` (`952f217`); CI on `69d3a40` ran `pnpm test`, `success` |
| **Lint, format, clippy** | Clean locally at `33d297b`, 2026-09-17: `pnpm lint`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo deny check` (`advisories ok, bans ok, licenses ok, sources ok`). CI run 35196578229 on `69d3a40` ran all four, `success` |
| **Sweeps** | Card round 2, Task 12, 2026-09-17, all clean: `control-byte-sweep.py` (7 files allow-listed for AmigaDOS DosType data and 7 for deliberate alignment), `scratch-root-sweep.py` (5 named exceptions), `scratch-guard-sweep.py` (151 guard sources, 1970 call sites, 3 hand-built paths, 9 platform-root arguments, 12 exempt), `contrast-check.py --quiet` (105 pairs, both themes). `rom-table-check.py` re-run 2026-09-17 on `docs-cleanup-0917`: 154 identifiable dumps, "the committed table says exactly what the database says" |
| **Build** | CI runs `pnpm tauri build` on every push (last: run 35196578229 on `69d3a40`, `success`), and `release.yml` built 0.9.4's installers. The last local package is 0.9.3, `pnpm tauri build` on `main` at `62f0f81`, 2026-09-13, copied to `E:\amiga\ProjeART\build\`, not code-signed |
| **i18n** | **2308** leaf strings in each of `src/i18n/en.json` and `tr.json`, counted 2026-09-17 on `docs-cleanup-0917` (no catalogue change since `main`). Parity — key sets, empty values, interpolation variables — is enforced by `pnpm test`, so count them rather than quoting this |
| **Open defects** | **8**, counted 2026-09-17 on `main`'s `docs/ISSUES.md` — ART-062, ART-324, ART-328, ART-329, ART-331, ART-339, ART-340, ART-341 ([ISSUES.md](ISSUES.md#open)). Method: `awk '/^## Open/{f=1} /^## Fixed/{f=0} f' docs/ISSUES.md \| grep -c '^\*\*ART-'` |
| **Feature rows** | **226 marked rows** in [FEATURES.md](FEATURES.md) — **171 ✅, 27 🟡, 20 ⏳, 5 🔩, 3 🅥2** — counted 2026-09-17: one marker per table row, the first cell in the row that is exactly a State marker, the legend table's five rows excluded. **State the method with the number**; counting marker *cells* gives a different figure |
| **amitools oracle** | 53 checks, both directions (`scripts/oracle-check.py`, blocking in CI; last run locally 2026-09-17 at `3193e36`, 53 `ok`) — including a filesystem driver ART embedded in an RDB and `rdbtool` extracted back out byte-for-byte |
| **Kickstart table** | 154 dumps (`core/rom/remus.rs::REMUS_ROMS`), generated from amitools' Remus split database and re-verified against it on every CI run (`scripts/rom-table-check.py`, [ART-104](ISSUES.md#fixed)). Licensed Amiga Forever ROMs are first-class input ([ART-128](ISSUES.md#fixed)): decoded with the `rom.key` beside them, then identified like any dump |
| **Install-media table** | 186 rows (`core/osinstall/media_hashes.json`, counted 2026-09-17), adopted from `rootrootde/emu68hatcher` (MIT). `scripts/media-table-check.py` against the owner's own AmigaOS 3.2 ADFs, 2026-09-06: **35 verified, 151 unverified, 0 conflicting**; the Rust twin agrees. Both fail when a directory verifies nothing ([ART-265](ISSUES.md#fixed)) |
| **7-Zip oracles** | The card's FAT32 boot partition read back by 7-Zip — type, geometry, label, names and every file's bytes (`scripts/fat-oracle-check.py`); and 4 disc fixtures — Joliet, ISO9660-only, raw Mode 1, raw Mode 2/XA — names, sizes, SHA-256 per file (`scripts/iso-oracle-check.py`). Neither in CI |
| **LZX oracle** | `scripts/lzx-oracle-check.py` — ART's LZX reader against unar (XADMaster), path set and SHA-256 per file. On the owner's own archives, 2026-09-17 (card round 2): `boingbag1` 998/998 and the WiFi archive 54/54 identical. Not in CI; dates, protection bits and comments are outside what it proves |
| **hst-imager PFS3 oracle** | Both directions, local only (`scripts/pfs3-oracle-check.py`): ART writes a volume through `NativeFormatter` and `hst-imager fs dir -r` reads it back; `hst-imager` formats and fills one and ART reads it back through `libpfs3`, SHA-256 per file plus protection strings. Last run 2026-09-15, exit 0 |
| **ILBM / prefs oracle** | `scripts/ilbm-oracle-check.py` — ffmpeg agrees with ART's ByteRun1 encoder pixel-for-pixel on **8 of 8** fixtures. Against the owner's own AmigaOS 3.9 material, **15 of 24** `.prefs` files are genuine `FORM PREF` containers and all 15 round-trip byte-for-byte through the real rebuilder (`replace_bodies(&[])`); the other 9 are third-party non-IFF formats, reported apart |
| **Icon oracle** | `scripts/icon-oracle-check.py` round-trips real `.info` files through `core/amigaicon`'s reader **and its four writers**, against the owner's own `E:\amiga\Amigatolon\os39` (798 real icons), 2026-09-14: `checked=798 failed=0 no_drawer_data2=0` ([ART-250](ISSUES.md#fixed): ToolTypes are Latin-1, byte-exact). Its first real run found [ART-249](ISSUES.md#fixed), a writer offset no unit test could see |
| **cargo-deny** | advisories, bans, licences, sources — all ok (2026-09-17 at `33d297b`, and in CI on `69d3a40`) |
| **MSRV** | 1.93 (raised from 1.77 on 2026-08-12, for a maintained 7z decoder) |
| **Published** | <https://github.com/tolon/Art> — public, `main`, **GPL-3.0-or-later**. Five releases, v0.9.0 to [v0.9.4](https://github.com/tolon/Art/releases/tag/v0.9.4), each with both installers built by `release.yml` from the tagged commit after CI went green on it. Checked with `gh` on 2026-09-17: no issues filed, 19 installer downloads across the five |
| **Real hardware** | **Bare metal, 2026-08-12**: `test/art-bootable-test.adf` booted a real **A500/A500+** (Kickstart 3.9) from a **Gotek** to an AmigaDOS CLI. Photographed. In emulation, 2026-08-16: a PFS3 volume ART formatted and filled booted a licensed Kickstart 3.1 to `hello from ART`, and a full AmigaOS 3.2 tree ART built booted a licensed V47 A1200 ROM to a clean Workbench. **No card or hard disk ART built has been recorded booting real hardware**, and physical magnetic media is still untouched — a Gotek is not a mechanical drive |
| **Seen on a screen** | The ADF, hard-disk, PiStorm, WHDLoad, Files and Settings screens have been opened and driven by a person (2026-08-12 onwards); the Collection's Play path ran real titles on 2026-08-18/21. **[ART-062](ISSUES.md#open)** narrows to the screens still unopened (Aminet, Collection, Gotek, ROM, WinUAE, Tools) and to the Turkish catalogue, of which a handful of its 2308 strings have been read by someone who speaks it. A headless browser, 2026-09-14, mounted every top-level route in both languages and measured layout (not meaning) — not the same claim as a person reading the screen |

Reproduce the numbers above:

```bash
pnpm lint                                              # TypeScript (run unpiped)
pnpm test                                              # frontend unit tests (i18n parity, phrase keys)
cd src-tauri && cargo fmt --check                      # formatting
cd src-tauri && cargo clippy --all-targets -- -D warnings
cd src-tauri && cargo test --lib                       # the full suite, twice — ART-059 (ART-261 is closed)
pip install amitools && python scripts/oracle-check.py # independent cross-check
python scripts/iso-oracle-check.py                     # the disc reader vs 7-Zip (needs 7z; not in CI)
python scripts/fat-oracle-check.py                     # the card's boot partition vs 7-Zip (needs 7z; not in CI)
python scripts/pfs3-oracle-check.py                    # PFS3, both directions, vs hst-imager (needs hst.imager.exe; not in CI)
python scripts/catalogue-check.py                      # every shipped bundle path vs Aminet itself (not in CI, it leaves the machine)
python scripts/zoom-check.py                           # the shell's widths, in a real browser (needs `pnpm dev`)
python scripts/tr-overflow-check.py                     # Turkish text clipping vs English, per route and width
                                                       #   (ART-062; needs `pnpm dev`)
python scripts/osbuilder-strip-check.py                # the OS Builder's step strip: per kind, both languages,
                                                       #   three Application Sizes (needs `pnpm dev`)
python scripts/contrast-check.py --quiet               # every colour pair in both themes, against WCAG (in CI)
python scripts/control-byte-sweep.py                   # no stray BEL/BS/VT/FF/ESC in tracked text (ART-216; in CI)
python scripts/scratch-root-sweep.py                   # every production staging site goes through the chosen
                                                       #   scratch root, not %TEMP% (ART-196; in CI)
python scripts/scratch-guard-sweep.py                  # every test scratch hands out its guard (ART-281; in CI)
python scripts/scratch-counter-sweep.py                # test scratch names unique *within one process*
                                                       #   (ART-059/164/173; not in CI)
python scripts/lzx-oracle-check.py DIR                 # ART's LZX reader vs unar (needs unar; not in CI)
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

# ART-117: replace the PFS3 driver on a byte copy of the owner's card. Make the copy first — it is written; the
# original is only read, and the test asserts its reserved range and mtime unchanged. The driver must state a newer
# $VER: than the card's 19.2. The backup and the post-edit range are kept in
# ART_RDB_EMBED_OUT (an existing folder; nothing in it is overwritten). TMP/TEMP come from the
# machine-local src-tauri/.cargo/config.toml (ART-320).
cd src-tauri && \
  ART_CARD_IN="E:\amiga\Amigatolon\caffeine\CaffeineOS_Storm_9317.img" \
  ART_RDB_EMBED_COPY="E:\amiga\ProjeART\build\tmp\caffeine-copy.img" \
  ART_RDB_EMBED_DRIVER="E:\amiga\ProjeART\build\tmp\pfs3aio-newer" \
  ART_RDB_EMBED_OUT="E:\amiga\ProjeART\art117-owner" \
  cargo test --lib core::preload::embed::tests::replace_the_driver_on_a_copy_of_the_owners_card_when_asked \
  -- --exact --nocapture --ignored

# ART-159's two language components, against the disc they were read off.
# Not re-run since ART-261's fix (2026-09-07); a clean run is still owed.
cd src-tauri && ART_159_ISO="E:\amiga\Amigatolon\iso\AmigaOS39.iso" \
  ART_159_DEST="E:\amiga\ProjeART\art159-tree" \
  cargo test --release build_the_real_39_language_components_when_asked -- --nocapture --ignored

# Builds AmigaOS 3.2.2 from the owner's own base and update media and then
# **asks the tree what it is** — `Prefs/Env-Archive/Versions/Release` is
# written by the release, so the claim comes from a file Hyperion wrote rather
# than from ART's own dropdown. Last measured 2026-09-07: 4066 files, 295
# drawers, 20706449 bytes, `Release 3.2.2`. The ROM matters —
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
# and prints where it is. TMP/TEMP must be on E: (std::env::temp_dir()
# defaults to C:, and the rules forbid writing there); every run stages
# beside the tree it is given, so point ART_FIRSTBOOT_TREE at a copy under
# E:\amiga\ProjeART\fb3-trees\.
cd src-tauri && TMP=E:/amiga/ProjeART/build/tmp TEMP=E:/amiga/ProjeART/build/tmp \
  ART_FIRSTBOOT_TREE=E:/amiga/ProjeART/fb3-trees/art5 \
  ART_FIRSTBOOT_ROM="E:/amiga/Amigatolon/paketler/3.2/AmigaOs 3.2/ROM/kicka1200.rom" \
  ART_WINUAE="C:/Program Files/WinUAE/winuae64.exe" \
  cargo test rehearse_the_real_tree_when_asked -- --ignored --nocapture

# Phase 3, Task 10: the reboot on a real 3.2 tree that has C/Reboot — proves
# the wrapper waits 3s and reboots, and the next boot finishes done all.
cd src-tauri && TMP=E:/amiga/ProjeART/build/tmp TEMP=E:/amiga/ProjeART/build/tmp \
  ART_FIRSTBOOT_TREE=E:/amiga/ProjeART/fb3-trees/art5 \
  ART_FIRSTBOOT_ROM="E:/amiga/Amigatolon/paketler/3.2/AmigaOs 3.2/ROM/kicka1200.rom" \
  ART_WINUAE="C:/Program Files/WinUAE/winuae64.exe" ART_FIRSTBOOT_EXPECT_REBOOT=restarted \
  cargo test --lib rehearse_a_reboot_on_the_real_tree_when_asked -- --ignored --nocapture

# Phase 3, Task 10: the same reboot step on a real 3.9 tree with no
# C/Reboot — proves the wrapper logs "reboot unavailable" and carries on in
# the same boot.
cd src-tauri && TMP=E:/amiga/ProjeART/build/tmp TEMP=E:/amiga/ProjeART/build/tmp \
  ART_FIRSTBOOT_TREE=E:/amiga/ProjeART/fb3-trees/sonuclar \
  ART_FIRSTBOOT_ROM="E:/amiga/Amigatolon/kickstart/Kickstart v3.1 rev 40.68 (1993)(Commodore)(A1200).rom" \
  ART_WINUAE="C:/Program Files/WinUAE/winuae64.exe" ART_FIRSTBOOT_EXPECT_REBOOT=unavailable \
  cargo test --lib rehearse_a_reboot_on_the_real_tree_when_asked -- --ignored --nocapture

# Phase 3, Task 10: the wizard on a real tree with nobody at the keyboard —
# proves Locale opens and the rehearsal ends TimedOut at its deadline naming
# Locale as the waiting window. ART_FIRSTBOOT_DEADLINE_SECS defaults to 120;
# raise it (Task 12 uses 900) to answer the windows by hand instead.
cd src-tauri && TMP=E:/amiga/ProjeART/build/tmp TEMP=E:/amiga/ProjeART/build/tmp \
  ART_FIRSTBOOT_TREE=E:/amiga/ProjeART/fb3-trees/art5 \
  ART_FIRSTBOOT_ROM="E:/amiga/Amigatolon/paketler/3.2/AmigaOs 3.2/ROM/kicka1200.rom" \
  ART_WINUAE="C:/Program Files/WinUAE/winuae64.exe" \
  cargo test --lib rehearse_the_wizard_on_the_real_tree_when_asked -- --ignored --nocapture

# The real Aminet — every shipped mirror asked separately, because failover
# stops at the first that answers and a dead one in position two is invisible
# in an ordinary sync. Leaves the machine; never in CI.
cd src-tauri && cargo test live_aminet -- --ignored --nocapture

# ART-244's own measurement: `readers::lhadrawer`'s claim that reading an
# archive's drawers "seeks header to header rather than decompressing" is
# cheap, run against the owner's own 663 MB, 893-drawer archive rather than a
# fixture. Prints the count and elapsed time; asserts neither. Measured
# 2026-09-07: 3118 ms cold, ~1600-1631 ms warm, for one archive alone — which
# is what motivated `refresh_root`'s Update-mode archive cache (size+mtime,
# the same shape the file walk already had).
cd src-tauri && ART_LHA_ARCHIVE="E:\amiga\Amigatolon\paketler\WHDLoadDemos100.lha" \
  cargo test --lib real_archive_scan_is_fast -- --ignored --nocapture

# Card round 2: real LZX archives through the product's own extraction gate.
# `scripts/lzx-oracle-check.py` drives this and compares the result with unar.
cd src-tauri && ART_LZX_DIR="E:\amiga\Amigatolon\paketler" \
  ART_LZX_OUT="E:\amiga\ProjeART\build\tmp\card-r2\lzx-oracle\art" \
  cargo test --lib read_the_owners_lzx_archives_when_asked -- --ignored --nocapture

# Card round 2: the owner's card material through classify, measure and
# prepare (core/card/content.rs). Any variable left unset skips its part.
cd src-tauri && \
  ART_CARD_HDF_A="E:\amiga\Amigatolon\WHDload\HDF_Games_WHDLoad_by_Enzo_[A]\A Prehistoric Tale v1.1.hdf" \
  ART_CARD_HDF_B="E:\amiga\Amigatolon\WHDload\HDF_Games_WHDLoad_by_Enzo_[B]\B-17 Flying Fortress v1.0.hdf" \
  ART_CARD_COLLECTION="E:\amiga\Amigatolon\paketler\WHDLoadDemos100.lha" \
  ART_CARD_LHA="E:\amiga\Amigatolon\paketler\BoingBag39-1.lha" \
  cargo test --lib prepare_the_owners_card_material_when_asked -- --ignored --nocapture
```

---

## Picking up next session

*One block, kept current.* **Update this block in place — do not stack another
on top of it.** What each round did is the [session log](session-log.md); what
is broken is [ISSUES.md](ISSUES.md).

### Start here (2026-09-17)

**Where `main` is.** The one-button card's round 2 is merged (`69d3a40`, pushed,
CI green): ART's own LZX reader, Amiga protection bits, comments and dates on
every archive entry, escaped names restored, `core/card/content.rs` (classify,
measure, check, prepare), and a partition taking a list of sources in one copy
step. The plan is
[docs/superpowers/plans/2026-09-16-one-button-card-round-2.md](superpowers/plans/2026-09-16-one-button-card-round-2.md);
the design is
[2026-09-11-one-button-card-design.md](superpowers/specs/2026-09-11-one-button-card-design.md).
First boot phase 3 (the wizard and the reboot, `4b72d7b`) and the third debt
round (`e060e76`, shipped as 0.9.4) are merged before it.

A documentation cleanup followed it (`c866021`, pushed, CI green): closed
ISSUES entries moved to `docs/ISSUES-archive.md`, executed plans and specs
pruned, session log and CHANGELOG summarised. It changed no code.

**In progress.** Nothing on a branch. Card round 3's research starts next.

**Next: card round 3** — the WHDLoad phase, the `card_os_prepare` /
`card_os_build` commands, `.partial` output, `run_with_fallback` reachable from
a card build, and the end-to-end card (design § 8.3). Round 3's command must
own [ART-340](ISSUES.md#open): a card source refused after unpacking began
leaves staging part-filled, so staging needs a `ScratchDir`-style guard. Every
screen is round 4.

**Open defects** (8): ART-062 (Turkish read on screen), ART-324 (a 4 GiB file
on a largefile PFS3 volume, which ART never formats), ART-328 and ART-329
(unreachable), ART-331 (a PFS3 directory block without the `DB` id skipped
silently, only on a damaged card), ART-339 (`core/card` ⇄ `core/preload`
import each other), ART-340 (above), ART-341 (`core/osinstall` keeps `:` names
that `check_name` refuses mid-copy).

**First boot phase 4** (packages) is not built and is narrowed to the two
packages still Amiga-only: `boingbags-39-3-4` (`needs-installer-script`) and
`euro-update` (`needs-fixfonts`).

**Settled 2026-09-16, no ART id:** amitools' `xdftool`-formatted FFS images
fail the Kickstart validator after an unclean reboot; ART's `NativeFormatter`
measured beside it (3.2 ROM, 20160 × 512 blocks, 3 runs per arm) was 3/3
clean, as was AmigaOS `Format`, and `xdftool` 0/3. Not measured: 3.1/3.9 ROMs,
other sizes, `DOS\3`, a real card. Report
`.superpowers/sdd/2026-09-16-art-ffs-validator/experiment.md`; the fixture rule
is in [testing.md](testing.md#real-material-and-the-ignored-hooks).

**Still owed by a person** (each as last recorded; none is code):

- **A card flashed and an A500 booted** — nothing SD-1 or SD-2 built has been
  recorded booting real hardware; the project's 1.0 bar. That includes the
  owner's 2026-09-11 card (`E:/amiga/Amigatolon/Kartlar/1card.img`) on the
  PiStorm.
- **First boot phase 3's closing measurement** — write first boot on 3.2 and
  3.9 and answer the Locale/Input/ScreenMode windows by hand; only this moves
  the FEATURES row off 🟡.
- **ART-117:** both PDS partitions of the edited card copy mounted in WinUAE or
  on a PiStorm, the loaded handler asked for `version full`.
- **PFS3 under the real handler:** a volume ART wrote, and a patched volume past
  MAXSMALLDISK, mounted under real pfs3aio in WinUAE; a file deleted and
  undeleted on a real Amiga from a PFS3 partition ART formatted; a comment
  written by libpfs3 `+art.12` read back by pfs3aio.
- **Card round 2, not proven:** hst-imager's `.uaem` handling on PFS3 (the
  measurement was FFS); an Amiga-made ZIP (every ZIP the owner has is PC-made);
  an LZX match running past a block end (never met in real material).
- **ART-062:** the Turkish catalogue read on screen; `hardDisk.bootablePri` and
  job status "Done"/"Failed" need a live Tauri session; with Turkish chosen,
  start a job and read the bar.
- **The OS Builder, driven** (four-tab spec § 7 and round 5's additions, listed
  in `.superpowers/sdd/2026-09-09-four-tabs/round-5-report.md`): BoingBag 1,
  BoingBag 2, the Locale update and the Türkçe catalogs ticked from a clean
  destination, one press, then the tree booted under WinUAE and asked
  `version full`; the update-mode run on an existing 3.9 tree;
  `scripts/osbuilder-strip-check.py` in both languages; the WinUAE studio's
  install section and the rehearsal driven, shutting the window yourself.
- **The Files screen's columns** (2026-09-08 simplification spec § 5.10): hide
  Ext, widen Date, restart and see both kept; 28 px text; narrow a pane; Reset.

**Owner decision still open:** the unreferenced trial images under
`E:\amiga\ProjeART` (about 80 GB, nothing deleted).

**Deliberately open, disclosed:** `scripts/control-byte-sweep.py` walks the real
filesystem, not `git ls-files` (its header says why); the WinUAE studio asks
`osinstall_describe_tree` twice per destination (four-tabs round 5 report § 8).

### Where the work stands

One paragraph per live area. Per-feature state is [FEATURES.md](FEATURES.md),
per-defect state is [ISSUES.md](ISSUES.md), per-round narrative is the
[session log](session-log.md). None of it is retold here.

- **The PiStorm card path** — SD-0, SD-1, SD-2 and SD-4 are built, SD-3's two
  named gaps are merged, SD-5 is part-built. The one-button card has rounds 1
  and 2 on `main`; round 3 is next. The gaps still open are one line each in
  the stage table below. What is left in SD-1 is **not code**: a card flashed
  and an A500 booted, which is also the project's 1.0 bar.
- **The OS Builder** — builds a distribution tree from the owner's own
  AmigaOS 3.2, 3.2.2 (layered base + update) and 3.9 media, carries wallpaper,
  screen mode, shell defaults, network config and arranged drawer icons, and
  identifies install media by content hash. What it still lacks is its
  FEATURES rows.
- **First boot on the Amiga** — phases 1 to 3 are on `main` (dispatcher, the
  `10-hardware`/`20-aux`/`30-datatypes` steps, the `S/FirstBoot.log` report
  read back off a card's FAT, FFS or PFS3, the wizard and the reboot); phase 3
  awaits the owner's hand-answered rehearsal. Phase 4 is narrowed (above).
- **Aminet and the package catalogue** — Stage A and Stage B are both built
  and tested, catalogue sync through install-to-HDF and the update view. The
  AI layer (§45.5) is deliberately deferred to v2, the owner's decision of
  2026-08-24.
- **The Emu68 Hatcher intake** (`rootrootde/emu68hatcher`, MIT) — scoped at six
  rounds. Five have landed on `main`: prefs and wallpaper (1), drawer icons
  (2), refusal evidence (3), media identification by hash (4), and first boot
  phases 1–3 (5); the one-button card is the sixth, rounds 1–2 merged.
- **Releases in the community's hands** — reading what came back is still the
  first thing a session does: issues on the repository, and whatever the owner
  was told directly. A report that went *well* counts: the gaps the README
  lists are unverified in both directions. As of 2026-09-17 no issue has been
  filed.

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
   been recorded booting real hardware. The software side is complete; what is
   missing is physical — a microSD card, a USB reader and an HDMI cable, the
   last plugged in **before** power or the VPU never configures the port and
   there is no RTG that session.

It needs no design decision or more code first.

---

## What ART is, and the ceiling that comes with it

**Every established Amiga distribution builder runs the install inside an
emulator** — HstWB Installer, AmiKit, AmigaSYS and ClassicWB all do, using the
user's own OS files. ART is not in that family: `amitools`'s `xdftool` does
what ART does — host-side placement plus a metadata sidecar (`.xdfmeta`, ART's
`.uaem`) — so ART sits in the **tooling** family. That has a ceiling, and it
is a live constraint rather than a defect:

- **[ART-166](ISSUES.md)** (**fixed 2026-09-08**) — this paragraph used to
  say a BoingBag's payload is a password-encrypted ZIP whose password lives in
  an Amiga executable, so a host-side placer cannot read it, and that the
  owner had ruled out writing a bypass. The owner reversed that ruling on
  2026-09-08, and the ceiling was lower than it looked: **Emu68 Hatcher** and
  **Emu68-Imager** (both MIT) publish the key, and the round-3 hashed
  snapshots show the `Updater` simply copies its payload onto the tree — of
  BoingBag 3.9-1's 210 payload files it wrote 166 and the other 44 were
  already byte-identical. ART now places both BoingBags from Windows, and the
  result hashes to the Updater's own tree (4 030 files; the only differences
  are one step ART deliberately does more of, and ART's own manifest).
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

This is the scheduling order — *what order the remaining work happens in and
why*. [roadmap.md](roadmap.md) points here and carries no phase list of its own.

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
| **Phase 2a** — Content-first detection and containers | ✅ | 2026-08-12 | [session log](session-log.md) |
| **Phase 2b** — The Files screen, done right | ✅ | 2026-08-12 | [session log](session-log.md) |
| **Stage 5 / §41.5** — Aminet: catalog sync, search, download, readme, Collection, install to HDF, update view, and the 14-set package-bundle catalogue | ✅ | 2026-08-09, extended to 2026-08-24 | [FEATURES.md](FEATURES.md) · [design-software-sources.md](design-software-sources.md). Bundles are download-only in phase 1 — nothing installs onto an Amiga volume yet; `net/live_aminet.rs` (2026-08-24) is the re-runnable check against the real mirrors |
| **Stage 5 / §45.5** — the AI workflow layer | 🅥2 | deferred 2026-08-24 | [design-ai-layer.md](design-ai-layer.md) — the owner's decision: the only planned feature that adds no capability ART lacks, so it waits until the ground under it has been driven |

Two caveats from Phase 2b that nobody has closed since: **the light theme has
never been looked at** (its tokens derive from the dark ones by role), and
**session restore is only half verified** — the write half reaches
`settings.json` and has tests (`src/lib/paneSession.ts`), but nothing has
closed and reopened ART to watch the read half. Both sit under
[ART-062](ISSUES.md).

### PiStorm Image Builder — SD-0 … SD-5

**The project's largest feature and, per the owner, its point.** SD-0, SD-1,
SD-2 and SD-4 are built. SD-3's two named gaps are both built and merged —
G14 (prefs, wallpaper, screen mode, shell defaults, `core/amiganet/`'s network
half) and G16 (multiboot) — and SD-5 is part-built. Only the gaps still open
are listed below; a built gap is a [FEATURES.md](FEATURES.md) row, not a line
here.

Gap analysis: [sd-appliance-gap-analysis.md](sd-appliance-gap-analysis.md).
Prior-art teardown: [sd0-prior-art.md](sd0-prior-art.md). Card layout measured
off two real cards: [sd2-card-layout.md](sd2-card-layout.md).

| Gap | Stage | What is still missing (files and the distro count checked against the tree 2026-09-17; the hardware and screen claims as last recorded) |
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
- The test suite's own scratch goes to `E:/amiga/ProjeART/build/tmp`, forced
  by the machine-local, gitignored `src-tauri/.cargo/config.toml`
  ([ART-184](ISSUES.md#fixed), [ART-320](ISSUES.md#fixed)); CI never has that
  file (or an `E:` drive) at all, so `cargo test` there sets both `value` and
  `force` itself on the command line. The old `D:\tmp\art-tests` was deleted
  on the owner's word on 2026-09-15.
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