# The Amiga packages a finished PiStorm system carries

**What this is:** every Amiga-side package the **Emu68 Imager** puts on a card,
read from its own *What's Included* page and enumerated entry by entry, with
its real source. It is a catalogue to build from, not a wish list — every entry
here is on a card somebody is using today.

**Why it exists:** the owner asked for it — *"Emu68-Imager'ın kullandığı Amiga
paketleri var, onların da listesini ekleyelim; hatta daha güzelleri varsa
onları da alalım."*

**This document was rewritten on 2026-08-22, and the reason is the point.** Its
first version said *"47 packages; 41 of them Aminet paths"*. That was a summary
of the page, not a reading of it: the install list alone is **60 entries**, and
the page has two more sections the first version folded into the same count.
A catalogue assembled by summarising is a catalogue that is wrong in ways
nobody can see. Everything below is enumerated in the page's own order.

**Where ART stands, stated first so the tables are not misread:** ART installs
**none** of these today. Its content layer places the user's own material and
runs a package's own installer on the Amiga (`core/osinstall`,
`core/amigainstall`). The design that turns this document into something ART
can act on is
[2026-08-22-package-bundles-design.md](superpowers/specs/2026-08-22-package-bundles-design.md)
— 14 sets, 62 entries, download only in phase 1.

**Nothing here is downloaded without the user asking.** ART's standing rule is
that a fetch is user-initiated; a catalogue is a list of what *could* be
fetched, and this document does not change that.

---

## The page has three sections, and only one of them is ART's business

| Section | Entries | Is it ART's? |
|---|---:|---|
| Programs used by the tool | 6 | **No** — host-side Windows tools |
| Included with the tool | 12 | **Mostly no** — the author's own or carried under a permission granted to *them* |
| **Downloaded during image creation** | **60** | **Yes** — this is the catalogue |

Folding these together is how the first version reached 47.

### Programs used by the tool (host-side — not Amiga software)

HST.Imager · HST.Amiga (both Henrik Nørfjand Stengaard) · HDF2Emu68 (Claude
Schwartz, Tom-Cat) · DDTC · FindFreeSpace (both Tom-Cat) ·
[Unlzx](https://aminet.net/util/arc/W95unlzx.lha).

ART's own equivalents already exist (`core/card`, `core/volume`, `libpfs3`,
`core/lha`) or are the `hst-imager` fallback it already keeps.

### Included with the tool (bundled, not fetched)

7Zip (LGPL) · **Roadshow Demo** ⚠ *"included with permission from Olaf Barthel
and Andreas Magerl"* · **Picasso96 configuration file** ⚠ *"included with
permission from Jens Schönfeld"* · ArewePal · AreWeOnline · CE · TomCopy ·
TomDelete (Tom-Cat) · Emu68Info · Emu68meter · WaitforTask (Flype) ·
Emu68Updater (Shaytan, mod. SupremeTurnip).

**A permission granted to the Emu68 Imager is not a permission granted to ART.**
The two ⚠ entries are carried by that tool under its own arrangement.

**Roadshow does not appear in ART's catalogue at all** — the owner wrote
**tolunnet** in its place, and holds the distribution right to it.

---

## The catalogue: what the Imager downloads and installs (60)

`Aminet` = reachable through `core/sources/mirror.rs` as it stands ·
`GitHub` = a release asset · `Mirror` = a specific site needing its own
configured mirror entry · `Search` = "latest version", resolved rather than
pinned · ⚠ = a licence or permission constraint · ❗ = the source as printed
cannot be used as written.

| # | Package | Kind | Source |
|---:|---|---|---|
| 1 | Emu68 Tools | GitHub | `michalsc/Emu68-tools` releases (nightly) |
| 2 | Emu68 (PiStorm) | GitHub | `michalsc/Emu68` releases |
| 3 | Emu68 (PiStorm32-lite) | GitHub | `michalsc/Emu68` releases |
| 4 | aget (UHC Tools) | Mirror | `uhc.driar.se/uhctools_os3.lha` |
| 5 | akGIF | Aminet | `util/dtype/akGIF-dt` |
| 6 | akJFIF | Aminet | `util/dtype/akJFIF-dt` |
| 7 | akPNG | Aminet | `util/dtype/akPNG-dt` |
| 8 | akTIFF | Aminet | `util/dtype/akTIFF-dt` |
| 9 | PeterK's icon.library | Search | Aminet query `iconlib` |
| 10 | Ami Speed Test | Aminet | `comm/net/AmiSpeedTest` |
| 11 | iBrowse (demo) ⚠ | Mirror | `ibrowse-dev.net` |
| 12 | AmiSSL | Search | Aminet query `amissl os3` |
| 13 | BusTest | Aminet | `util/moni/bustest` |
| 14 | CFD 1.33 | Aminet | `driver/media/CFD133` |
| 15 | ChangeBootPri | Mirror | `thomas-rapp.hier-im-netz.de/downloads/` |
| 16 | CLICon | Aminet | `util/wb/CLICon` |
| 17 | CopyReplace | Aminet | `util/sys/CopyReplace` |
| 18 | Directory Opus 4.16JR | Aminet | `util/dopus/DOpus416JRbin` |
| 19 | Directory Opus 4.17pre21 | Mirror | `dopus.free.fr/betas/DOpus417pre21.lzx` |
| 20 | Mecho | Aminet | `util/batch/mecho` |
| 21 | Fat95 | Aminet | `disk/misc/fat95` |
| 22 | File Sys Box | Aminet | `util/libs/filesysbox.m68k-amigaos` |
| 23 | GENet.device | GitHub | `rondoval/emu68-genet-driver` |
| 24 | Hippo Player | Aminet | `mus/play/hippoplayerupdate` |
| 25 | IDEfix 97 | Aminet | `driver/media/IDEfix97` |
| 26 | Installer 43.3 | Aminet | `util/misc/Installer-43_3` |
| 27 | Jano Editor | Aminet | `text/edit/JanoEditor` |
| 28 | LHA | Aminet | `util/arc/lha_68k` |
| 29 | LZX | Aminet | `util/arc/lzx121r1` |
| 30 | MD5 Sum | Aminet | `util/crypt/MD5SUM` |
| 31 | MiamiDX — Main | Aminet | `comm/tcp/MiamiDx10cmain` |
| 32 | MiamiDX — MUI | Aminet | `comm/tcp/MiamiDx10c-MUI` |
| 33 | MUI 3.8 | Aminet | `util/libs/mui38usr` |
| 34 | PFS3 | Aminet | `disk/misc/PFS3_53` |
| 35 | Picasso96 (shareware) ⚠ | Aminet | `driver/video/Picasso96` |
| 36 | Prism 2 | Aminet | `driver/net/prism2v2` |
| 37 | Reboot | Aminet | `util/boot/reboot` |
| 38 | Reqtools | Aminet | `util/libs/Reqtools-Wide` |
| 39 | RexxTricks | Aminet | `util/rexx/RexxTricks_386` |
| 40 | Screentext | Mirror | `thomas-rapp.hier-im-netz.de/downloads/` |
| 41 | SearchReplace | Aminet | `text/misc/SearchReplace` |
| 42 | SetDST | Aminet | `util/time/SetDST` |
| 43 | SetPatch 44.38 ⚠ | Mirror | `cdn.cloanto.com/pub/amiga/SetPatch-44-38.lha` |
| 44 | SKick 3.46 | Aminet | `util/boot/skick346` |
| 45 | SMBFS | Aminet | `disk/misc/smb2fs.m68k-amigaos` |
| 46 | SnoopDOS | Aminet | `util/moni/SnoopDos` |
| 47 | Sntp (UHC Tools) | Mirror | `uhc.driar.se/uhctools_os3.lha` |
| 48 | SRename | Aminet | `util/cli/SRename` |
| 49 | SysInfo | Mirror | `download.d0.se/pub/SysInfo.lha` |
| 50 | Sysvars | Aminet | `util/boot/sysvars` |
| 51 | TTTool | Aminet | `util/batch/TTTool` |
| 52 | UnZip 5.52 | Aminet | `util/arc/UnZIP552` |
| 53 | ViNCEd ❗ | Aminet | `util/shell/ViNCEd` — **the page prints `packageutil/shell/ViNCEd`, a missing slash.** Verified against Aminet: v3.109, `ViNCEd.lha`, 881 447 bytes compressed, 2025-11-02 |
| 54 | WHDLoad | Mirror | `whdload.de/whdload/WHDLoad_usr.lha` |
| 55 | WHDLoadWrapper ❗⚠ | — | The page gives an **FTP search form** with query parameters (`ftp2.grandis.nu/…search.php?…&username=ftp%2Cany`), not a path. ART's "configured mirrors, never a caller-supplied URL" rule (§41.5.7) cannot accept it as written |
| 56 | Wizard Library | Aminet | `util/libs/WizardLibrary` |
| 57 | Workbench-Library 40.5 ⚠ | Mirror | `cdn.cloanto.com/pub/amiga/Workbench-Library-40-5.lha` |
| 58 | XAD Master | Aminet | `util/arc/xadmaster020` |
| 59 | XPK User | Aminet | `util/pack/xpk_User` |
| 60 | ZIP 2.32 | Aminet | `util/arc/ZIP232` |

**Counted:** 45 Aminet · 3 GitHub · 10 Mirror · 2 Search (a third, #55, has no
usable source at all). 45 + 3 + 10 + 2 = 60.

### Plus two the Imager does not have

| Package | Kind | Source | Note |
|---|---|---|---|
| **tolunnet** | own | `D:\Projeler\tolunnet` | The owner's own TCP/IP stack, in Roadshow's place. Distribution right held, so no ⚠ |
| **tolunwifi** | own | the same | The owner's own WiFi package |

**62 entries in ART's catalogue.**

---

## What still has to be answered before any of this becomes data

- **Four ⚠ entries** — Picasso96, iBrowse, SetPatch, Workbench-Library. The
  owner is obtaining the permissions; ART records them and warns on screen
  regardless, and the catalogue entry carries the field that drives it.
- **One ❗ with no usable source** — WHDLoadWrapper. Either a proper mirror
  entry for grandis.nu, or the entry becomes `user-supplied`: ART names it and
  says why it cannot fetch it.
- **Screentext's purpose is unverified.** Thomas Rapp's downloads page could
  not be fetched and no search result described it. It must not be filed under
  a category on a guess.
- **All 60 paths need checking against the live sources**, not against this
  document. `scripts/catalogue-check.py` in the design is that check, and
  entry 53 is why it exists.

---

## Beyond the Imager: what a larger distribution carries

[AmiKit](https://www.amikit.amiga.sk/) ships **420 pre-installed programs** and
**no AmigaOS or ROM** — the same division ART keeps. Its
[changelog](https://file.amiga.sk/amikit/doc/changelog_win.html) names them with
versions, and the great majority are Aminet packages. Two of its mechanisms are
worth knowing about rather than copying: **Live Update**, which delivers updates
after installation, and **RabbitHole**, a drop folder — *"simply place the
archive into the RabbitHole folder and it gets installed automagically"* — which
is a shape ART's own drag & drop pipeline already resembles.

[HstWB Installer](https://hstwb.firstrealize.com/) takes the third road: the
user picks from named packages (BestWB, BetterWB, ClassicWB, HstWB, Picasso96,
DOpus, MUI, iGame, WHDLoad) but **downloads the archives themselves**. Its
package manifest is worth reading — it was fetched rather than guessed, and it
confirms three shapes ART already has. See the design document.

**Sources:** [Emu68 Imager — What's
Included](https://mja65.github.io/Emu68-Imager/included.html) (read 2026-08-22,
enumerated) · [Aminet ViNCEd](https://aminet.net/package/util/shell/ViNCEd) ·
[AmiKit changelog](https://file.amiga.sk/amikit/doc/changelog_win.html) ·
[HstWB Installer](https://hstwb.firstrealize.com/) ·
[classicwb-lite-package](https://github.com/henrikstengaard/classicwb-lite-package)
