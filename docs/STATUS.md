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
| **Last updated** | 2026-09-18 — card round 4 (every screen) merged into `main` (`6e91ec2`, `--no-ff`, on the owner's word) after its final review, one fix wave, a scoped re-review and the residual round that closed N1, N2, N3 and the M2 comment. The test, lint, sweep and i18n rows below were measured on the branch at the commits they name; `main` carries exactly that code |
| **Version** | **0.9.4**, released 2026-09-15 — tag `v0.9.4` (annotated, on `e91f22e`), release run 34969262720 `success`, published on the owner's word with one NSIS and one MSI installer. Its notes are CHANGELOG's `[0.9.4]` section. The number lives in three files (`package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` — all `0.9.4`, checked 2026-09-17) and `release.yml` refuses a tag that disagrees with any of them, or a version with no CHANGELOG section |
| **`main` / `origin`** | **`main` is `6e91ec2`** — card round 4 merged `--no-ff` on 2026-09-18 on the owner's word and `art-card-round-4` deleted; pushed, and CI's result on it is recorded here once it lands. Round 4 is the screens: the card as a destination on the Machine tab, partition rows filled by drop / Add… / a row menu, sizes measured without staging, the Kickstart agreement, the card run's five phases and four endings, a fourth job ending (`refused`), and every card refusal in Turkish. It was reviewed (3 Critical, 5 Important, 9 Minor), fixed in one wave, re-reviewed (2 new findings), and those fixed in a residual round. Round 3 merged before it as `d421822`. One other local branch, `art-310-windows` (`d851174`, 12 commits not in `main`), is kept unmerged: ART-310 was fixed on `main` by another route, and [ISSUES-archive.md](ISSUES-archive.md) cites its research note through `git show art-310-windows:…` |
| **Tests — Rust** | `cd src-tauri && cargo test --lib` on `art-card-round-4` at `993ecc9` (the residual round), 2026-09-18, **twice — ART-059 satisfied**: `test result: ok. 3729 passed; 0 failed; 66 ignored; 0 measured; 0 filtered out; finished in 537.79s`, then the same result in `500.44s`. The only change committed after those two runs is a corrected doc comment on `card_os_prepare` (the residual round's own re-review, M2), which cannot alter behaviour |
| **Tests — frontend** | `pnpm test` on `art-card-round-4` at the residual round, 2026-09-18: `Test Files 122 passed (122)`, `Tests 1945 passed (1945)` |
| **Lint, format, clippy** | Clean on `art-card-round-4` at the residual round, 2026-09-18: `pnpm lint` (both `tsc --noEmit` passes), `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` (no warnings). `cargo deny check` (`advisories ok, bans ok, licenses ok, sources ok`) was last run at `a0b0415`; no Cargo file has changed since |
| **Sweeps** | Card round 4's residual round, 2026-09-18, all four CI-blocking sweeps clean at the current head; the figures below are that run's and match Task 13's: `control-byte-sweep.py` (7 files allow-listed for AmigaDOS DosType data and 7 for deliberate alignment), `scratch-root-sweep.py` (5 named exceptions), `scratch-guard-sweep.py` (157 guard sources, 2070 call sites, 3 hand-built paths, 9 platform-root arguments, 12 exempt), `contrast-check.py --quiet` (105 pairs, both themes). The two non-sweep oracles were last run at Task 13 and not since (nothing they cover changed): `rom-table-check.py`, 154 identifiable dumps, "the committed table says exactly what the database says"; `oracle-check.py`, 53 checks, both directions, "ART and an independent implementation agree, both ways round" |
| **Build** | CI runs `pnpm tauri build` on every push (last: run 35196578229 on `69d3a40`, `success`), and `release.yml` built 0.9.4's installers. The last local package is 0.9.3, `pnpm tauri build` on `main` at `62f0f81`, 2026-09-13, copied to `E:\amiga\ProjeART\build\`, not code-signed |
| **i18n** | **2512** leaf strings in each of `src/i18n/en.json` and `tr.json`, counted 2026-09-18 on `art-card-round-4` at `a0b0415` (up from 2308 on `main`; card round 4 added `cardSection.*`, `cardKickstart.*`, `cardRun.*`, `osinstall.destinationKind.*` and fourteen new `errors.*` entries, among others). Parity — key sets, empty values, interpolation variables — is enforced by `pnpm test`, so count them rather than quoting this |
| **Open defects** | **10**, re-counted 2026-09-18 on `art-card-round-4`'s `docs/ISSUES.md` at the residual round — ART-062, ART-324, ART-328, ART-329, ART-331, ART-342, ART-343, ART-344, ART-345, ART-346 ([ISSUES.md](ISSUES.md#open)). The last two were filed by round 4's fix wave (this row still said 8, Task 13's figure, until the recount): ART-345, the Machine tab can never show a size in the one-button flow; ART-346, a gate turning on mid-run deletes the card run's report. The re-review's own three findings (N1, N2, N3) were defects **in this unmerged branch's fix diff**, fixed by the residual round rather than filed. ART-343 and ART-344 each gained a note on what round 4's screen now does about them: ART-343's ROM half is rendered in Turkish and one button re-prepares (the tree/sources half stays open); ART-344's exposure is narrowed (`card_os_open` only on the run button, never on mount) though no exit hook or startup sweep was built. Method: `awk '/^## Open/{f=1} /^## Fixed/{f=0} f' docs/ISSUES.md \| grep -c '^\*\*ART-'` |
| **Feature rows** | **236 marked rows** in [FEATURES.md](FEATURES.md) — **180 ✅, 28 🟡, 20 ⏳, 5 🔩, 3 🅥2** — counted 2026-09-18 on `art-card-round-4` (seven rows added by Task 13 for the card's destination choice, the measured drop conversion, the card section, the Kickstart agreement, the card run, the Turkish refusal table and the Power User–mode gating; the end-to-end card row's own marker stays 🟡, its text updated to say the screen now exists): one marker per table row, the first cell in the row that is exactly a State marker, the legend table's five rows excluded. **State the method with the number**; counting marker *cells* gives a different figure |
| **amitools oracle** | 53 checks, both directions (`scripts/oracle-check.py`, blocking in CI; last run locally 2026-09-17 at `bca7794`, "ART and an independent implementation agree, both ways round") — including a filesystem driver ART embedded in an RDB and `rdbtool` extracted back out byte-for-byte |
| **Kickstart table** | 154 dumps (`core/rom/remus.rs::REMUS_ROMS`), generated from amitools' Remus split database and re-verified against it on every CI run (`scripts/rom-table-check.py`, [ART-104](ISSUES.md#fixed)). Licensed Amiga Forever ROMs are first-class input ([ART-128](ISSUES.md#fixed)): decoded with the `rom.key` beside them, then identified like any dump |
| **Install-media table** | 186 rows (`core/osinstall/media_hashes.json`, counted 2026-09-17), adopted from `rootrootde/emu68hatcher` (MIT). `scripts/media-table-check.py` against the owner's own AmigaOS 3.2 ADFs, 2026-09-06: **35 verified, 151 unverified, 0 conflicting**; the Rust twin agrees. Both fail when a directory verifies nothing ([ART-265](ISSUES.md#fixed)) |
| **7-Zip oracles** | The card's FAT32 boot partition read back by 7-Zip — type, geometry, label, names and every file's bytes (`scripts/fat-oracle-check.py`); and 4 disc fixtures — Joliet, ISO9660-only, raw Mode 1, raw Mode 2/XA — names, sizes, SHA-256 per file (`scripts/iso-oracle-check.py`). Neither in CI |
| **LZX oracle** | `scripts/lzx-oracle-check.py` — ART's LZX reader against unar (XADMaster), path set and SHA-256 per file. On the owner's own archives, 2026-09-17 (card round 2): `boingbag1` 998/998 and the WiFi archive 54/54 identical. Not in CI; dates, protection bits and comments are outside what it proves |
| **hst-imager PFS3 oracle** | Both directions, local only (`scripts/pfs3-oracle-check.py`): ART writes a volume through `NativeFormatter` and `hst-imager fs dir -r` reads it back; `hst-imager` formats and fills one and ART reads it back through `libpfs3`, SHA-256 per file plus protection strings. Last run 2026-09-17 at `bca7794` (card round 3, Task 14, default mode, `ART_PFS3_DRIVER` pointed at `E:\amiga\Amigatolon\hstimager\pfs3aio`), exit 0: "ART and hst-imager agree, both directions". Its own `--card` mode (card round 3, Task 13) checked a whole four-partition card against `hst-imager`, separately |
| **ILBM / prefs oracle** | `scripts/ilbm-oracle-check.py` — ffmpeg agrees with ART's ByteRun1 encoder pixel-for-pixel on **8 of 8** fixtures. Against the owner's own AmigaOS 3.9 material, **15 of 24** `.prefs` files are genuine `FORM PREF` containers and all 15 round-trip byte-for-byte through the real rebuilder (`replace_bodies(&[])`); the other 9 are third-party non-IFF formats, reported apart |
| **Icon oracle** | `scripts/icon-oracle-check.py` round-trips real `.info` files through `core/amigaicon`'s reader **and its four writers**, against the owner's own `E:\amiga\Amigatolon\os39` (798 real icons), 2026-09-14: `checked=798 failed=0 no_drawer_data2=0` ([ART-250](ISSUES.md#fixed): ToolTypes are Latin-1, byte-exact). Its first real run found [ART-249](ISSUES.md#fixed), a writer offset no unit test could see |
| **cargo-deny** | advisories, bans, licences, sources — all ok (2026-09-17 at `33d297b`, and in CI on `69d3a40`) |
| **MSRV** | 1.93 (raised from 1.77 on 2026-08-12, for a maintained 7z decoder) |
| **Published** | <https://github.com/tolon/Art> — public, `main`, **GPL-3.0-or-later**. Five releases, v0.9.0 to [v0.9.4](https://github.com/tolon/Art/releases/tag/v0.9.4), each with both installers built by `release.yml` from the tagged commit after CI went green on it. Checked with `gh` on 2026-09-17: no issues filed, 19 installer downloads across the five |
| **Real hardware** | **Bare metal, 2026-08-12**: `test/art-bootable-test.adf` booted a real **A500/A500+** (Kickstart 3.9) from a **Gotek** to an AmigaDOS CLI. Photographed. In emulation, 2026-08-16: a PFS3 volume ART formatted and filled booted a licensed Kickstart 3.1 to `hello from ART`, and a full AmigaOS 3.2 tree ART built booted a licensed V47 A1200 ROM to a clean Workbench. **No card or hard disk ART built has been recorded booting real hardware**, and physical magnetic media is still untouched — a Gotek is not a mechanical drive |
| **Seen on a screen** | The ADF, hard-disk, PiStorm, WHDLoad, Files and Settings screens have been opened and driven by a person (2026-08-12 onwards); the Collection's Play path ran real titles on 2026-08-18/21. **[ART-062](ISSUES.md#open)** narrows to the screens still unopened (Aminet, Collection, Gotek, ROM, WinUAE, Tools) and to the Turkish catalogue, of which a handful of its 2512 strings have been read by someone who speaks it. A headless browser, 2026-09-14 and re-run 2026-09-18 on `art-card-round-4` (`scripts/tr-overflow-check.py`, 48 route×width runs, the 16 top-level routes, 0 Turkish-only hits), mounted every top-level route in both languages and measured layout (not meaning) — not the same claim as a person reading the screen. **The new card screens are OS Builder sub-routes** (`/os-builder/makine`, `/os-builder/derle`) that script's own ROUTES list has never reached (the same scope decision `zoom-check.py` makes); a supplementary, uncommitted probe reusing its technique reached both by a real click into card mode and found nothing Turkish-only there either, but the Kickstart agreement step specifically needs a live card-build session (`card_os_open`) a plain browser cannot open, and stays unmeasured this way |

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

### Start here (2026-09-18)

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

**Card round 3 is merged** (`d421822`, `--no-ff`, pushed; the branch is
deleted): fourteen tasks — `core/cardos/` moved above `core/card` and
`core/preload` (ART-339), a product scratch guard (`OwnedScratch`) a card-OS
session holds and removes on every ending (ART-340), `:`/empty-segment
AmigaDOS names refused before anything is written (ART-341), the PFS3 driver
found, WHDLoad chosen and installed, Kickstarts listed before the build and
placed only on agreement, `card_os_open`/`prepare`/`build`/`close` writing
under `<image>.partial`, and an end-to-end card proven both by ART's own
reader and against `hst-imager` as an independent oracle. The plan is
[docs/superpowers/plans/2026-09-17-one-button-card-round-3.md](superpowers/plans/2026-09-17-one-button-card-round-3.md);
the ledger and every task report are in
`.superpowers/sdd/2026-09-17-one-button-card-round-3/`. The final
whole-branch review (`final-review.md`: 0 Critical, 5 Important, 18 Minor) was
fixed in one wave (`fix-wave-report.md`): a fourth build ending, `refused`, for
everything asked before anything is written; the card writer says what it did
with a file it created; one hst-imager, probed at prepare and gated at build;
a manifest counted off the finished card; System's two refusals with their own
next steps. The scoped re-review (`fix-wave-re-review.md`) found one wave
sentence wrong (N1) and three Minors; the residual round (`d344b26`,
`residual-fix-report.md`) fixed them, split ART-343's ROM half out and fixed it,
and wrote round 1's cylinder note into CHANGELOG. The second whole-suite run
(ART-059) came back green and the merge followed. Two wording Minors the
residual re-review raised were left for round 4 and are now fixed there (Task
12): the `KickstartSourceChanged` sentence's phrasing (the "since the card was
prepared" trailer now sits inside each `why` clause, next to the verb it
modifies, rather than appended after the proposal's own number), and
`CardFinishLeftBothNames` now reads correctly beside a later
`PartialRemoval::Removed` — the screen states both true facts, at their
different moments, rather than contradicting itself.

**Card round 4 — every screen — is merged** (`6e91ec2`, 13 tasks plus a fix
wave and a residual round, on top of round 3's merge): a fourth job ending
(`refused`, so a refused build is never called failed); a typed phase-and-count
event (`CARD_OS_PHASE_EVENT`); every card refusal typed with parameters (round
3's ruling that `CardSourceUnusable.why` was prose reversed); three read-only
commands (`card_os_measure`/`classify`/`check_volume_name`); `buildSession`'s
`destinationKind`/`CardTarget` per release (ART-308's last site gone); the one
global drop listener's position measured before it was converted
(`experiment-drop-coordinates.md`: physical ÷ `devicePixelRatio` alone 15/15
across three real Application Sizes, dividing by zoom too 7/15 — ART-101's
mistake, repeated and this time caught before it shipped); the Machine tab's
*Folder*/*Card image* choice and one card-size question; the card section
(partitions, sources, live sizes, an overflow that names the partition); the
Kickstart agreement step (nothing ticked by ART); the card run itself
(`useCardOsRun`, four endings, one session opened only on the run button and
closed exactly once on every path, which found and fixed two real `jobs.ts`
defects along the way); a 28-row Turkish/English refusal table; and
`CardBuilder`/`VolumePreload` moved behind Power User mode with a pre-run
blocker. Task 13 ran the whole suite twice, every CI-blocking script and
`oracle-check.py`, all green; the Turkish measurement found nothing
Turkish-only on the 16 top-level routes or (by a supplementary, uncommitted
probe) on the new card section and the card run's own button — the Kickstart
agreement specifically needs a live session a plain browser cannot open, and
stays unmeasured this way.

**The final whole-branch review and its one fix wave are in** (`.superpowers/sdd/2026-09-18-card-round-4/`,
`final-review.md` and `fix-wave-report.md`): 3 Critical, 5 Important, 9 Minor.
The wave fixed C1 (the OS Builder rendered `<Outlet />` with no `context`, and
react-router provides `undefined` unconditionally, so the per-row drop never
fired in the running app while every test mocked `useOutletContext` and crossed
nothing — held now by a test that renders the real route tree unmocked), C2
(every refusal `card_os_prepare` raises ended the job *Failed* and reached the
screen as English with the code in parentheses; it now answers a typed
`CardRefusal` on its own event and ends `JobState::Refused`), C3 (a card run
wrote its scratch tree into the persisted `session.tree` and erased it, with
`firstboot.written`, on every path — the run's tree now lives in
`src/lib/cardRunTree.ts`, which nothing persists and no folder screen reads),
I1 (dragging *over* a row added the previous drop's paths: one `lastDrop`
value with its own `seq`), I2 (`hstImagerPath` forwarded to measure and
prepare), I3 (the catalogue's card parameters and Rust's `details()` were two
hand copies; a check in `errorText.test.ts` reads `core/error.rs` per code in
both directions, and found one dropped parameter on its first run), I4's
unmount half (the agreement's promise is rejected on unmount, so the session
closes) and all nine Minors. **I5 and I4's remainder are filed as
[ART-345](ISSUES.md) and [ART-346](ISSUES.md)** — both flow questions for the
owner rather than fixes.

**The scoped re-review and the residual round that closed it are in**
(`fix-wave-re-review.md`, `residual-fix-report.md`). The re-review verdicted
every original finding ADDRESSED or FILED-FAIR and found three new defects in
the wave's own diff; all three are now fixed. **N1** (Critical):
`VolumePreload`'s volume-name check held the names in flight in `useState`
*and* in the effect's own dependency list, so each ask re-ran the effect, whose
cleanup cancelled the answer already on its way — the answer was then
discarded, which removed the name, which asked again: `card_os_check_volume_name`
in a loop at IPC rate, with no verdict ever stored, so a bad name never blocked
Run. The suite was green over it only because a mocked promise resolves as a
microtask; the set is now a ref, nothing cancels an answer, and the new case
answers on a macrotask. **N2** (Important): the prepare job emitted its typed
refusal event for *every* `Err`, including the ones `prepare_refused` excludes,
and the screen branches on that field alone — so an I/O failure read as
*refused*, with a refusal's next step, while the job bar said *failed*. The
event now goes through `prepare_refusal_event`, which answers `Some` only for a
refusal; a failure settles through `JobFailed { code }`, and both arms are
pinned. **N3** (Minor): `hstImagerPath`'s forwarding to measure and prepare is
asserted in the two files that already mock those commands. That round's own
re-review raised one Minor (M2), also fixed: `card_os_prepare`'s doc comment
still read as though every ending travelled on its event, which N2 had just
made false. **Unmerged, and nothing test-shaped is owed** — ART-059's two
whole-suite Rust runs were taken at `993ecc9` and came back green (3729 each).
**Next is the merge, and after it the owner's own card, built through these
screens, flashed and booted on a real PiStorm.**

**Open defects** (10, after round 4's fix wave filed two): ART-062 (Turkish read on screen), ART-324 (a 4 GiB file
on a largefile PFS3 volume, which ART never formats), ART-328 and ART-329
(unreachable), ART-331 (a PFS3 directory block without the `DB` id skipped
silently, only on a damaged card), ART-342 (`core/whdload` ⇄ `core/gameindex`
import each other, found by round 3's research; nothing broken by it), ART-343
(a prepared card's tree and sources not checked against the disk when it is
built; its ROM half fixed, and round 4's screen renders that refusal in the
user's own language and offers a one-press re-run — the tree/sources half
stays open) and ART-344 (a card session's scratch left when the
app exits; round 4's screen narrows the exposure by opening the session only
on the run button, but built no exit hook or startup sweep; its `session.tree`
half is closed by the fix wave's C3) — both still filed as design. Round 4's
fix wave added ART-345 (in the one-button flow the Machine tab can never show
a size, a total or an overflow, because the card's tree exists only inside a
run and the tab is locked while one runs) and ART-346 (a gate turning on
mid-run deletes the card run's report from the screen; the session-leak half
of it is fixed). ART-339, ART-340 and ART-341
(the round's own three) are fixed on `art-card-round-3`, not yet on `main`.

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
- **Card round 3, not proven:** every claim above is a fixture card built in a
  test, or `hst-imager` reading it back — **no card round 3 built has been
  flashed or booted**. Round 4 built the screen; it did not build the proof.
- **Card round 4, not proven:** every screen exists and every unit test is
  green, but no card has yet been built end to end **through the screen** by a
  person, let alone flashed or booted — that is the owner's own run, still
  owed. The Kickstart agreement step's own Turkish has not been read on screen
  either (it needs a live session a headless browser cannot open); the card
  section and the run button's own text were measured clean by a supplementary
  probe (`.superpowers/sdd/2026-09-18-card-round-4/task-13-report.md`), which
  is not the same claim as a person reading it.
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
  and 2 on `main`, and so are rounds 3 (the core and its commands, `d421822`)
  and 4 (every screen, `6e91ec2`), each reviewed, fixed and re-reviewed before
  its merge. The gaps still
  open are one line each in the stage table below. What is left in SD-1 is
  **not code**: a card flashed and an A500 booted, which is also the project's
  1.0 bar.
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
  phases 1–3 (5); the one-button card is the sixth, all four
  rounds merged (`69d3a40`, `d421822`, `6e91ec2`).
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