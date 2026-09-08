# OS Builder intake — research across the established projects, and what ART takes from them

**Date:** 2026-09-08 (night of 2026-09-07)
**Status:** research and proposal — decisions marked *owner's call* are not taken here
**Written under CLAUDE.md's "Research before design"** rule, at the owner's request: *"Tekrar
tam bir araştırma yapmanı, mevcut projeleri incelemeni ve en doğru özellikleri hem işleyiş hem
fonksiyon olarak bizim projemize taşımanı istiyorum."* The trigger was one evening on the
Windows 11 build: two refusals on the OS Builder that a person hit within minutes of driving
the screen with real files, and the feeling that installing the BoingBags "drags the matter
out".

This document is the synthesis. The five underlying reports are local-only under
`.superpowers/sdd/2026-09-07-osbuilder-research/` (`hstwb.md`, `emu68-builders.md`,
`distributions.md`, `art-today.md`, `material.md`); every fact this document relies on is
restated here with its source, so the document stands without them.

---

## 1. What was actually checked

| Subject | How | Where the evidence is |
|---|---|---|
| **HstWB Installer** (henrikstengaard, MIT) | cloned, head `5dfdf488`; `run.ps1`, `setup.ps1`, `modules/*.psm1`, the six AmigaDOS scripts under `amiga/amiga-os-3.9/S/Amiga-OS-3.9/`, both CSV tables, the `.uae` templates read | `E:\amiga\ProjeART\research-2026-09-07\hstwb\` |
| **Emu68 Hatcher** (MIT), **Emu68-Imager-Software** (mja65, MIT, four repositories), **emu68-bootstrap** (jit06, **GPL-3.0**, not MIT as assumed) | cloned; Hatcher `origin/1.1.0` diffed against the 2026-09-05 teardown; Imager's `Startup-Sequence_OneTimeRun`, `Copy-ArchiveinArchiveFiles.ps1`, `Emu68-Updater.rexx` read in full | `…\research-2026-09-07\emu68\` |
| **AmiKit** (owner's licensed copy), **CaffeineOS**, **MultibootOS 2.2** | the owner's material on `E:\amiga\` read; MultibootOS's FAT32 partition listed and its six `put X here.txt` files extracted; AmiKit's host staging tree read (its `.hdf` is a dynamic VHD holding **SFS**, which neither 7-Zip nor ART can open — its Amiga-side scripts were **not** read) | `…\research-2026-09-07\distros\` |
| **ClassicWB**, **PiMiga**, **AmigaSYS** | public repositories and docs; AmigaSYS's site is unreachable (expired certificate) and nothing was established about it | same |
| **The owner's own 3.9 update material** | every archive in `E:\amiga\Amigatolon\os39\` hashed, listed with 7-Zip **at both levels** (wrapper and every stored member), every readme and Installer script extracted and read | `…\research-2026-09-07\bb-material\` |
| **ART today** | read-only audit of the wizard, `core/osinstall`, `core/amigainstall`, the recipes, the specs, ISSUES/FEATURES; the two refusals of the evening traced to their lines | `art-today.md` |

**One elimination was wrong and is corrected in place.** The first pass over the material
reported "no ZIP, no encryption anywhere". It had listed only the wrappers. Extracting the
stored member `BoingBag3.9-1/AmigaOS-Update` and listing it on its own prints `Type = zip` and
`Encrypted = +`, `Method = ZipCrypto Deflate:Maximum` on every entry — 233/233 for BoingBag 1,
147/147 for BoingBag 2's `AmigaOS-Update` and 39/39 for its `XAD-Update`. ART-166 has said so
since 2026-08-19 and is right. The lesson is the one CLAUDE.md already carries: listing a
wrapper is not listing what its members hold.

**Two measurements on this machine, taken because two reports asked for them before any design
relied on them:** `AMIGAFOREVERDATA` is set (`E:\amiga\`), and Amiga Forever's registry key
exists under `HKLM\SOFTWARE\WOW6432Node\Cloanto\Amiga Forever` (not under the 64-bit hive).
Both are one command away and were run rather than recalled.

---

## 2. Ground truth about the material — what the archives say about themselves

From `material.md`, all READ-FROM-MATERIAL:

- **Order is enforced by the packages, not merely advised.** Each BoingBag's own `Install`
  script reads `version.library`'s revision off the target disk: BB1 requires version 45, BB2
  requires revision ≥ 2 (BB1 applied), the community BB3&4 requires revision ≥ 3 and its readme
  states *"You need to install Boing Bags #1 and #2 first"* and *"Requires: OS3.9+BB2"*.
- **The chain the material prescribes:** ISO (`OS-Version3.9/OS3.9Install`, `$VER: Install
  45.0`) → BoingBag 1 → BoingBag 2 → optionally, after BB2 and **before** BB3&4: the per-language
  Locale packages (`Locale3_9.lha`, `BoingBag39-2-turkce.lha`), Contribution, Euro-Update → BB3&4.
  BB3&4's readme says Euro-Update and the ReAction GenesisPrefs update are already folded in, so
  Euro-Update alone is only useful where BB3&4 will not be installed.
- **The emulator fix is redundant with one of the owner's two copies.** `BoingBag39-1.lha`
  carries `C/Updater` 25,588 bytes, 2001-04-03, CRC `0000B9D4`; `BoingBag39-1 (1).lha` carries
  25,732 bytes, 2001-04-17, CRC `00009C29` — **byte-identical** to the `Updater` inside
  `BoingBag39-1-UAE.lha`. The UAE readme's own condition: *"When you downloaded your
  BoingBag3.9-1 after the 20-4-2001, it already contains this fix."* So ART can decide whether
  the fix archive is needed by reading the wrapper's `Updater`, which it already does through
  `minimum_version: "45.15"`; the panel today still asks for the second archive as if it were
  the user's problem to know.
- **No unattended mode exists.** None of the Installer scripts accepts `NOVICE`, `DEFUSER`,
  `LOGFILE`, `MINUSER` or `PROMPTUSER`; every one sets `(user N)` internally and uses
  `"askuser"` on its copies; BB1's script aborts in pretend mode outright. A run needs a live
  Amiga session. (ART bypasses the Installer script and calls `C/Updater` directly, which is why
  its run finishes without a person — see §4.)
- **What is host-placeable, exactly:** only `BoingBag39-2-Contribution.lha` (plain files, no
  script, no program). Everything else invokes a package-bundled 68k binary (`Updater`,
  `GetLocale`, `FixFonts`). BB1 and BB2 carry a second, independent blocker on top: their
  payloads are ZipCrypto ZIPs, opaque to anyone without the password.
- `NDK39.lha` and `OS39FAQ-english.lha` install nothing and are outside the chain.

---

## 3. How the established projects do it

| | HstWB Installer | Emu68 Hatcher | Emu68-Imager | emu68-bootstrap | AmiKit | MultibootOS | CaffeineOS | ClassicWB |
|---|---|---|---|---|---|---|---|---|
| Licence | **MIT** | MIT | MIT | GPL-3.0 | commercial EULA | none | none (grey) | none (permission) |
| ART may reuse | **scripts and CSVs, with attribution** | approach, with citation | approach | yes (same family) | patterns only | patterns only | patterns only | patterns only |
| 3.9 + BoingBags | yes | yes | yes | **no** (3.2 only) | yes (scripts unread, SFS) | n/a | n/a | consumes an installed 3.9 |
| Where the Updater runs | **on the Amiga, in WinUAE/FS-UAE** | nowhere — decrypts on the host with the password, copies a curated list | same, the origin of the mechanism | n/a | on the Amiga (inferred) | n/a | n/a | n/a |
| Media identified by | MD5 (rank 1) → ADF volume label at `0x6E1B0` (2) → filename (3); 98 + 41 CSV rows; two accepted 3.9 ISO hashes; ISO also checked structurally (six directories must exist) | MD5 table + filename fallback | MD5 table (CSVs not in git) + a 7-Zip **member-list** fallback for ISOs | filename only | registry, ROM scan, drop file | filename manifest | n/a | `Version workbench.library 45 FILE` |
| "Which file goes where" | a **per-file readout** — name, path found, *how* matched, provenance, `Not found!` red if required / yellow if optional — over a `'Amiga OS 3.9' (3/4)` set fraction; a `readme.txt` in each expected folder naming the exact files | GUI pickers | GUI | CLI args | `RabbitHole/` drop folder | `+AmigaOS32/put AmigaOS3.2 ADFs here.txt`: `REQUIRED`/`OPTIONAL` on line 1, every filename enumerated, numbered provenance, ordering, degradation, exclusions | README | — |
| Order enforcement | three times: BB2 detected only if BB1 found; counted with a `break` on the first missing; refused at run with the file named | `for bb in _BOINGBAGS` | ordering column | arg order | ? | n/a | n/a | n/a |
| Host↔Amiga channel | a drawer of marker files (`Prefs/Install-Amiga-OS` = `Amiga-OS-390`; empty `Prefs/UAE` = "quit, don't reboot"); success = the Amiga writes `Prefs/Install-Complete` | first-boot script | `Startup-Sequence_OneTimeRun`, hardware probed by `VERSION brcm-emmc.device` | host-side only | — | — | — | — |
| Post-Updater fix-ups | **ten**: seven `+p` and three `+s` protection bits; `AmigaOS ROM Update.BB39-2` promoted with a `.old`…`.old4` backup ladder; a second `Updater` run for `XAD-Update` gated on `xadmaster.library` < 10; BB2's fixed `C/Installer` copied to `C:` and `Utilities:`; WarpUP libs only if present; locale catalogs only for languages already present; Genesis drawer renames | curated file list, renames `.BB39-2` files on the way in | same | n/a | unread | n/a | n/a | n/a |
| Endings | **one sentence for four endings** ("Installation failed"), no timeout on the emulator wait — recorded as what *not* to copy | `BuildError` | — | — | — | — | seven distinct launcher outcomes | — |
| Remembered | everything, on every change, two ini files; Amiga Forever auto-wired from `%AMIGAFOREVERDATA%` | project file | — | — | `amikit.ini`, registry | the card | `winuae.ini` | — |

**The one thing every project that can be read agrees on:** nobody who does not hold the
password runs the BoingBag payload anywhere but on an Amiga, through Haage & Partner's own
`Updater`. HstWB's whole codebase has no decryption except Cloanto ROM keys, and only to hash.
Hatcher and Imager are the exception; they carry the passwords in source (`boingbag.py`,
`Copy-ArchiveinArchiveFiles.ps1`) and copy a curated file list out of the decrypted payload.

---

## 4. Where ART stands — measured, not remembered

From `art-today.md`, and from the code:

**The engine is ahead of the field, not behind it.**

- ART runs `C/Updater AmigaOS-Update <volume>:` the same way HstWB does, but against a **staged
  copy** that is promoted over the user's tree only on `Succeeded`; the original is never the
  thing being changed (§2 of the 2026-08-20 design).
- The run has a **deadline** and four **distinct endings** (succeeded, refused, timed out, the
  window was closed) — HstWB has neither.
- Order is enforced before a byte is copied, from `distribution.json`'s `amiga_installed`
  (`core/osinstall/chain.rs`, ART-186).
- The two fix-ups shown necessary against ART's own result are done, on the host, typed rather
  than scripted (`core/amigainstall/finish.rs`, ART-227): the ten protection bits, and
  `AmigaOS ROM Update.BB39-2` promoted with a four-deep backup ladder — the step without which
  BoingBag 2's ROM update is silently inert. The XAD second pass, the `Installer` copy, the
  WarpUP and locale merges are recorded in ART-227 as "each becomes a variant on the day a
  measurement asks for one".
- Both BoingBags installed on the owner's real material through ART's own `compose → install`:
  BB1 169.1 s, BB2 138.1 s, and the tree **booted and answered** `Workbench 45.3 (07-Dec-01)`,
  `workbench.library 45.127` (ART-193). Five minutes, unattended, once the inputs are right.

**The screen has never been driven by a person until tonight**, and that is where every defect
of the evening lives:

- **ART-276 (proposed).** `add_package_staging_in`'s clash check (`apply.rs:1759-1771`) refuses
  a second component from the same medium: it scans `manifest.files` for `media == package.media
  && component != package.id`. The manifest's `built_from` already records `{volume_name,
  sha256}` per medium, and the archive's SHA-256 is computed **one line after** the check
  (`apply.rs:1820`). The correct test is: same volume name **and different hash** → refuse;
  same hash → two components sharing one archive, which `locale-39-turkish.json:11` calls
  "ordinary". No test covers the ordinary case, which is why the suite is green with the bug
  live.
- **ART-277 (proposed).** `AmigaInstallPanel.tsx` keeps four **global** `useRemembered` keys
  (`amigaInstall.package/archive/overlayArchive/medium`, lines 161-198). Choosing a different
  package resets none of them (`setPackageId` alone, line 567), so BoingBag 1's archive and
  UAE-fix paths ride silently into a BoingBag 2 request, and the refusal that comes back quotes
  an internal overlay path (`'BoingBag3.9-1-UAE/BoingBag3.9-1'`) instead of saying which package
  is selected and which archive was given. The same panel asks three separate questions that are
  all "where is the material" — the archive, the second archive, the CD image — each with its
  own browse button.
- The audit's throughline: every real defect in the Amiga-side work (ART-185 … ART-193, and
  now these two) was found in the first sitting a person drove the thing against real files.
  Unit tests were green each time.

**So the diagnosis of "it drags the matter out" is:** the mechanism takes five minutes and is
proven; the *intake* — knowing which archive goes where, in what order, with what remembered
from last time — is what a person fights, and it is unfixed because no person had used it.

---

## 5. The decision the owner reopened: BoingBags placed from Windows

The owner asked on 2026-09-07: *"Bu boingbag'ları Windows üzerinden yerleştirsek çok iyi olacak,
diğer proje yapmış."* The standing ruling (2026-08-19, re-examined 2026-08-25 and 2026-09-05,
recorded in ART-166 and `docs/superpowers/notes/2026-09-05-emu68hatcher-teardown.md`) is that
no password bypass is written. **This is the owner's call**; the facts on both sides, with
nothing softened:

**What host-side placement would be.** The payload is a ZipCrypto ZIP; the password is held by
`C/Updater` and is published in Hatcher's and Imager's MIT source. Host-side placement means
ART carries that password and decrypts the payload itself, then copies a curated file list —
Hatcher's `_apply_one` / `_apply_item` shape, ~150 lines of Python there. ART's `zip` crate
(8.6) has a decrypting reader; whether ZipCrypto specifically is enabled under ART's
`default-features = false` build was **not** verified in this pass and would be the first thing
to check.

**What it would buy.** BB1 and BB2 without an emulator run: no ROM at that step, no five minutes,
no `ARTPkg:` mount, no `Updater` version question, no UAE-fix archive. The post-Updater fix-ups
ART already does on the host would still apply and would still be needed.

**What it would not buy.** The emulator step does not disappear: `Locale3.9`, the Turkish locale
update, Euro-Update, GenesisPrefs and BB3&4 all run a package-bundled binary (`GetLocale`,
`FixFonts`) and need a live session — §2. Only BB1 and BB2 would move. Two of the eight steps in
the chain.

**What it costs.** The encryption exists so that the update is applied only through the
`Updater`, which checks for a genuine 3.9 (`version.library` 45); Haage & Partner's readme says
redistribution requires their permission. Carrying their password in ART's public source is the
thing the owner decided against three times, with the reason recorded each time: the legitimate
path gives the same result, and — the 2026-08-25 finding — the emulator is the installer's
*native* environment, not a workaround.

**Recommendation, stated so it can be disagreed with:** keep the ruling and fix the intake. The
measured pain of the evening is two defects and a confusing panel, not the mechanism; the
mechanism ran both BoingBags in five minutes on this owner's material. If the ruling changes,
it changes in writing in ART-166 with the date, and the design is Hatcher's shape with the
curated list cross-checked against HstWB's fix-up scripts. Nothing in this document builds it.

**The ruling changed on 2026-09-08, and this paragraph is the record this section asked for.**
The owner reversed it in their own words: *"inatla BoingBag'ı Windows üzerinden yerleştirmedin;
diğer proje yapıyor bu işi, bu yüzden iş kilitlendi"* / *"avukatlık yapma, mühendisiz biz"*. ART
now places BoingBag 1 and 2 from the host, the way Emu68 Hatcher and Emu68-Imager (both MIT) do.
The full record, with the measurements, is in ART-166, now **Fixed**. Four things this section
predicted and got wrong, corrected here rather than left standing:

1. **The `zip` crate question was the right first thing to check, and the answer is yes.** 8.6
   carries ZipCrypto ungated (`src/zipcrypto.rs`; only AES sits behind `aes-crypto`), so
   `by_index_decrypt` is available as ART builds it.
2. **"A curated file list" is not what shipped, and the oracle is why.** Hatcher curates —
   whole drawers plus named `Devs/*` files. The round-3 hashed snapshots of what the real
   `Updater` produced say it does something simpler: of BoingBag 3.9-1's 210 payload files it
   wrote 166 and the other 44 were **already byte-identical in the tree**; 0 differed. So the
   `Updater` copies its payload whole, and ART's existing subtree rules — which say exactly
   that — were already right. Hatcher's curation would have missed
   `Devs/NSDPatch.cfg-BB3.9-1`.
3. **"Only BB1 and BB2 would move — two of the eight steps"** is right, and it was the two the
   owner was blocked on. Nothing else in the chain changed.
4. **The 2026-08-25 finding — *the emulator is the installer's native environment* — is still
   true and is no longer a reason.** ART does not run the installer at all on this route; it
   places the files the installer would have placed, and the oracle says which files those are.

What the oracle answered, on the owner's own material: **4 030 files expected, 4 031 produced;
2 added, 1 missing, 2 differing**, and all five are the deliberate `NSDPatch.cfg` rename (which
keeps the user's file at `.old`) plus ART's own `distribution.json`. Every path the `Updater`
wrote hashes identically.

---

## 6. What ART takes — mechanisms and functions, ranked

Each item names its source, its licence status, the ART problem it solves, and its cost.
"Solves pain 1" = which archive goes where; "pain 2" = two packages sharing a medium; "pain 3"
= a refusal that is not actionable.

### 6.1 The material is one folder and a set of named slots — not three fields *(pain 1, 2)*

**From** HstWB (`AmigaOsDir`, one folder; every medium normalised to one volume name before any
consumer sees it; zero-byte capability flags read by every later step) and MultibootOS (`+`
drop folders). **MIT / pattern.**

ART already has the halves: a media hash table (186 rows, `2026-09-06-media-identification-by-
hash-design.md`), `scan::identify`, `MediaMatch`, `built_from`. What is missing is the *shape*:
the user points at **one folder** (default: the remembered one, or `%AMIGAFOREVERDATA%`'s
`Shared\adf` offered — never applied), ART resolves every artefact it knows into a **slot**
(`system-tree`, `AmigaOS3.9:`, `package:boingbag-39-1`, `overlay:boingbag-39-1-uae`, `rom`), and
every step of the wizard reads the slots. The Amiga-side panel's three browse buttons become
one line each: *found here, matched by hash, from Amiga Forever* — or *not found; expected
`BoingBag39-2.lha`*. Cost: medium; it is a resolver over things that exist plus a component.

### 6.2 The per-file readout with match method and provenance, and a set fraction *(pain 1)*

**From** HstWB `ViewAmigaOsSetFiles` / `FormatAmigaOsSetInfo`. **MIT.** For every expected
artefact: name, path found, `Match MD5` / `Match VolumeName` / `Match FileName`, provenance
comment; `Not found!` red if required, yellow if optional; over `'AmigaOS 3.9' (3/4) — 1 required
file missing`. Always on screen, never only at submit. ART's version adds the thing HstWB cannot:
the state from `distribution.json` (*installed on 2026-09-07*), so a slot also says whether its
work is already done. Cost: small once 6.1 exists.

### 6.3 The chain as a checklist in the material's own order, one Run button *(pain 1, 3)*

**From** the material itself (§2) and HstWB's three-fold order enforcement. The Amiga-side panel
stops being "pick a package, then fill three fields" and becomes the chain rendered top to
bottom: ISO tree → BB1 (UAE fix *needed / not needed / supplied*, decided by ART from the
wrapper's `Updater` version, never asked) → BB2 → Locale / Turkish → Contribution → Euro-Update →
BB3&4. Each row: installed / ready / blocked by the row above / missing `X`. **Run** runs the next
runnable row. The remembered state is **per tree and per package**, so nothing from BB1 rides
into BB2 (closes ART-277 structurally, not by clearing fields). Cost: medium; the engine
already supports every row that ART ships a recipe for.

### 6.4 Refusals in the user's words, naming the slot and the order *(pain 3)*

**From** CLAUDE.md's own rule, unmet tonight; HstWB's *"Boing bags 1 file 'BoingBag39-1.lha'
doesn't exist! Skipping…"* as the shape. Every refusal on these screens names the package, the
artefact, and the next step: *"BoingBag 2 is selected; the archive you gave is BoingBag 1's UAE
fix. Put `BoingBag39-2.lha` in the BoingBag 2 slot."* Internal overlay paths never reach the
screen. Cost: small; it is wording plus the slot model.

### 6.5 The clash check asks the hash, not the component *(pain 2)*

**From** the audit; HstWB's `contentIds` (mark overlap, generate a skip, never refuse) as the
longer-term shape for genuine alternatives. Fix ART-276 as §4 states, with two tests: the
ordinary case (same archive, second component → accepted) and the mutation (same volume name,
different bytes → refused, naming the hash mismatch). Cost: small. A later `alternatives`
field on packages (two recipes providing the same content, one wins, the picker says so) is
recorded, not built now.

### 6.6 Post-Updater fix-ups: the remaining three, gated on a measurement *(correctness)*

**From** HstWB `Install-Boing-Bag-2`. **MIT.** ART-227 already lists them. Take, in this order,
each as a typed `PostStep` variant: the second `Updater` run for `XAD-Update` gated on
`xadmaster.library` < 10 (a genuinely missing patch if the owner's tree has an old xadmaster —
**measure first**: `Version "Libs/xadmaster.library" FILE` on the tree BB2 produced); BB2's
fixed `C/Installer` into `C:` and `Utilities:`; the locale-catalog merge only for languages
already present. WarpUP and Genesis renames only if a tree ever shows the condition. Cost:
small each; the knowledge is the expensive part and HstWB's scripts are the second opinion.

### 6.7 One controlled experiment nobody has run: does `Updater` return a usable code?

**From** `hstwb.md`'s open question. HstWB calls it bare; ART reads `If Warn` after it. Nobody
has shown that a failing `Updater` sets `WARN`. One variable (a deliberately corrupt
`AmigaOS-Update`), a counted result, both arms, the control measured. If the code is unusable,
ART's "refused" ending for a BoingBag is a sentence resting on nothing, and the honest fix is to
verify by asking the artefact afterwards (`Version workbench.library FILE` before and after).
Cost: one afternoon with the owner's material. **The highest-value unknown in this document.**

### 6.8 Identification hardening *(pain 1's other half)*

**From** HstWB (ten of its logical media accept more than one MD5; six named directories checked
on the 3.9 disc) and Imager (`FileCheck`: an ISO identified by a member list plus size/date
triple, because ISO bytes vary by mastering session). ART's table has **no** 3.9 ISO or BoingBag
row today (`media_hashes.json`: 186 entries, 0 mention BoingBag or an ISO). Take: a set of
accepted hashes per medium, not one; the two 3.9 ISO hashes and both BoingBag hashes from
HstWB's CSV (MIT, attributed) cross-checked against the owner's files (`material.md` has the
SHA-256s); structural checks beside the hash; `$VER:` as the tie-breaker. Cost: small–medium;
`scripts/media-table-check.py` already exists to keep it honest.

### 6.9 The drop folder that explains itself *(pain 1, and the SD lane)*

**From** MultibootOS's `put X here.txt` (structure only; ART writes its own words in both
catalogues) and HstWB's per-folder `readme.txt`. When a build starts, ART writes into the
material folder one text per slot: `REQUIRED`/`OPTIONAL`, the exact filenames, where to get
them, the order, what happens without it. Generated from the media table, so it cannot drift.
For the SD lane the same texts go onto the FAT32 partition. Cost: small.

### 6.10 Small things worth their price

- **Amiga Forever discovery — offer, never apply.** `%AMIGAFOREVERDATA%` and the WOW6432Node key
  both exist on this machine (§1). A pre-filled suggestion the user confirms. Cost: tiny.
- **Backup cascade** `.old`…`.old4` generalised from `replace-keeping-backup`'s one level, and
  running out is a refusal. Already ART's shape; confirm it is the loop, not one step.
- **Content-fit pre-check** before staging (Hatcher `_check_partition_space`): required and
  available named in the refusal. Check what the card step already does first.
- **Two marker files, not one exit code**, extended: `art-result.txt` already carries
  `started/ok/failed`; keep it, and add the artefact question (§6.7) beside it.
- **`List … LFORMAT` → generated script → `Execute`** as the sixth measured AmigaDOS rule in
  `architecture.md`, beside the five from the first-boot round (HstWB and Imager both use it;
  ART's `ART-FirstBoot` already does).

### 6.11 Deliberately not taken

- **The password mechanism** (Hatcher/Imager) — the owner's ruling, §5.
- **A live, externally hosted package list** (Imager's `Emu68-Updater.rexx` self-updates from a
  Google Sheets export) — the opposite of ART's mirror-only rule (§41.5.7).
- **HstWB's `Start-Process -Wait` with no timeout**, and its one sentence for four endings.
- **A shipped placeholder that is the wrong file** (MultibootOS's `kick.rom` is a 3.1 ROM beside
  a `WARNING THIS IS KICK3.1.txt`) — ART ships the empty slot.
- **HstWB's bundled m68k binaries** (`UAEquit` on personal permission, MMULib, LhA) — not
  redistributable; ART neither needs nor may ship them.
- **AmiKit's `ForbSysNames` list** — the idea (warn when a volume name shadows a standard
  assign) is worth having; the list is derived from ART's own recipes, not copied.

---

## 7. Proposed order of work

Three rounds, each leaving the tree green and each ending with **a person driving the screen
against the owner's real material** — the audit's throughline is that nothing else finds these
defects.

1. **Fix wave (the evening's list, one branch):** ART-275 cursor keys in the commander; ART-276
   the hash-based clash check with its two tests; ART-277 per-package, per-tree remembered state
   on the Amiga-side panel plus refusals in the user's words (§6.4, §6.5). Then BB1 → BB2 →
   Locale → Turkish driven from the panel, not the test hook.
2. **Intake:** the slot resolver and the readout (§6.1, §6.2), the 3.9 ISO and BoingBag rows
   with multiple accepted hashes and structural checks (§6.8), the drop-folder texts (§6.9),
   Amiga Forever offered (§6.10). The panel's three fields disappear.
3. **The chain:** the checklist-in-order with one Run (§6.3), the three remaining fix-ups gated
   on measurement (§6.6), the `Updater` exit-code experiment (§6.7), BB3&4 as the test of "a
   fourth package is a JSON file". Contribution as a host-placeable package if the owner wants it
   (it is the one thing in the chain that is plain files).

Round 1 needs no decision. Rounds 2 and 3 need a spec each, written against this document.

---

## 8. Owner's calls, stated as questions

1. **The BoingBag ruling** (§5): keep — recommended — or change in writing.
2. **Scope of the chain on screen:** the official four (ISO, BB1, BB2, Locale) only, or the
   whole material including Contribution, Euro-Update and the community BB3&4.
3. **Does the Amiga-side panel stay a wizard step**, or become the single "AmigaOS 3.9 updates"
   screen the chain suggests, reachable from the tree picker as well.

---

## Sources

- HstWB Installer — <https://github.com/henrikstengaard/hstwb-installer> (MIT); `amiga/amiga-os-3.9/S/Amiga-OS-3.9/*`, `data/amiga-os-entries.csv`, `modules/data.psm1`, `run.ps1`, `setup.ps1`
- Emu68 Hatcher — <https://github.com/rootrootde/emu68hatcher> (MIT); `builder/staging/boingbag.py`, `builder/install_extras.py`, `check-package-downloads.py`
- Emu68-Imager-Software — mja65 (MIT); `Assets/AmigaFiles/System/S/Startup-Sequence_OneTimeRun`, `Copy-ArchiveinArchiveFiles.ps1`, `Emu68-Updater.rexx`
- emu68-bootstrap — jit06 (GPL-3.0)
- MultibootOS 2.2 — the owner's image; `+AmigaOS32/put AmigaOS3.2 ADFs here.txt` and five siblings; `MultibootOS-2.2-Readme.pdf`
- AmiKit — the owner's licensed copy; `amikit.ini`, the host staging tree; <https://amikit.amiga.sk/getamigaos>
- BoingBags 3&4 v1.59 — <https://amigan.1emu.net/releases/> (`BoingBags3&4.readme`)
- `BoingBag39-1-UAE.lha` — <https://www.devili.iki.fi/pub/Commodore/amigaos/updates/3.9/>
- RetroPlatform KB 19-106, *Self-Installing Packages* — <https://www.retroplatform.com/kb/19-106>
- ART's own: ISSUES ART-166, ART-186, ART-193, ART-227; specs `2026-08-20-amiga-side-install-{research,design}.md`, `2026-09-06-media-identification-by-hash-design.md`; `docs/superpowers/notes/2026-09-05-emu68hatcher-teardown.md`
