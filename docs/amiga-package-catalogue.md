# The Amiga packages a finished PiStorm system carries

**What this is:** every Amiga-side package the **Emu68 Imager** installs onto a
card, read from its own *What's Included* page on 2026-08-22, with its real
source URL. It is a catalogue to build from, not a wish list — every entry here
is on a card somebody is using today.

**Why it exists:** the owner asked for it — *"Emu68-Imager'ın kullandığı Amiga
paketleri var, onların da listesini ekleyelim; hatta daha güzelleri varsa
onları da alalım."*

**Where ART stands, stated first so the table is not misread:** ART installs
**none** of these today. Its content layer places the user's own material and
runs a package's own installer on the Amiga (`core/osinstall`,
`core/amigainstall`); it has no catalogue of third-party Amiga software. What
it does have is the machinery to get one — `core/sources` fetches from
**configured mirrors, never an arbitrary URL** (§41.5.7), and **41 of the 47
entries below are Aminet paths**, which is exactly the shape that engine takes.

**Nothing here is downloaded without the user asking.** ART's standing rule is
that a fetch is user-initiated; a catalogue is a list of what *could* be
fetched, and this document does not change that.

---

## How to read the Source column

| Marking | Meaning for ART |
|---|---|
| **Aminet** | `aminet.net/package/…` — reachable through `core/sources/mirror.rs` as it stands |
| **Vendor** | A specific site. Each needs its own configured mirror entry, or the user supplies the file |
| **GitHub** | A release asset |
| ⚠️ | A licence or distribution constraint that has to be answered before ART touches it |

---

## Emu68 itself

| Package | Source | What it is |
|---|---|---|
| Emu68 (PiStorm) | GitHub `michalsc/Emu68/releases` | The kernel. ART already handles this archive (`core/card/intake.rs`) |
| Emu68 (PiStorm32-lite) | GitHub, same | The other board's kernel — see [ART-204](ISSUES.md) |
| Emu68 Tools | GitHub `michalsc/Emu68-tools/releases/tag/nightly` | The Amiga-side drivers and tools that go with it |
| Emu68Updater | bundled (Shaytan, mod. SupremeTurnip) | Updates Emu68 from the Amiga |
| Emu68Info, Emu68meter | bundled (Flype) | Status and performance readouts |

## Filesystems and storage drivers

| Package | Source | What it is |
|---|---|---|
| **PFS3** (`PFS3_53`) | Aminet `disk/misc/PFS3_53` | The filesystem ART already **writes** through `libpfs3`. The card needs the 68k handler itself |
| Fat95 | Aminet `disk/misc/fat95` | Lets the Amiga read the FAT32 boot partition |
| File Sys Box | Aminet `util/libs/filesysbox.m68k-amigaos` | The filesystem framework SMBFS and others need |
| SMBFS (`smb2fs`) | Aminet `disk/misc/smb2fs.m68k-amigaos` | Windows shares from the Amiga |
| CFD 1.33 | Aminet `driver/media/CFD133` | CompactFlash driver |
| IDEfix 97 | Aminet `driver/media/IDEfix97` | IDE/ATAPI |

## Networking

| Package | Source | What it is |
|---|---|---|
| Roadshow (demo) | bundled ⚠️ *with permission; full version at roadshow.apc-tcp.de* | The TCP/IP stack. **The owner's own `tolunnet`/`tolunwifi` work sits in this space** |
| MiamiDX 1.0c (main + MUI) | Aminet `comm/tcp/MiamiDx10cmain`, `…-MUI` | The alternative stack |
| AmiSSL | Aminet, latest `amissl+os3` | TLS — without it nothing modern is reachable |
| iBrowse (demo) | Vendor `ibrowse-dev.net` ⚠️ demo | The browser the imager pre-installs |
| GENet.device | GitHub `rondoval/emu68-genet-driver` | The Pi's own Ethernet, as an Amiga device |
| Prism 2 | Aminet `driver/net/prism2v2` | WiFi driver for Prism2 cards |
| Ami Speed Test | Aminet `comm/net/AmiSpeedTest` | |
| aget, Sntp | Vendor `uhc.driar.se/uhctools_os3.lha` | Fetch a URL; set the clock from a time server |
| SetDST | Aminet `util/time/SetDST` | Daylight saving |

## Graphics and the desktop

| Package | Source | What it is |
|---|---|---|
| **Picasso96** | Aminet `driver/video/Picasso96` ⚠️ | RTG. The imager installs the free version and states plainly that *"the only location to purchase a legal copy of P96 is Individual Computers"*, on Jens Schönfeld's request. **Any ART use must carry that sentence** |
| PeterK's `icon.library` | Aminet, latest `iconlib` | The icon library everything modern expects. [ART-127](ISSUES.md) was ART's tree lacking `icon.library` at all |
| MUI 3.8 | Aminet `util/libs/mui38usr` | The toolkit MiamiDX and much else need |
| Reqtools (Wide) | Aminet `util/libs/Reqtools-Wide` | |
| Directory Opus 4.16JR | Aminet `util/dopus/DOpus416JRbin` | The file manager. ART's own Files screen is a commander in the same tradition |
| Directory Opus 4.17pre21 | Vendor `dopus.free.fr` | The newer build |
| CLICon | Aminet `util/wb/CLICon` | A shell from Workbench |
| ViNCEd | Aminet `util/shell/ViNCEd` | A better console handler |
| Hippo Player | Aminet `mus/play/hippoplayerupdate` | Module player |
| Jano Editor | Aminet `text/edit/JanoEditor` | Text editor |

## Datatypes — the thing that bit ART already

| Package | Source |
|---|---|
| akGIF, akJFIF, akPNG, akTIFF | Aminet `util/dtype/akGIF-dt`, `akJFIF-dt`, `akPNG-dt`, `akTIFF-dt` |

**Worth its own note.** [ART-193](ISSUES.md) was an install that hung for ever
because `datatypes.library` had an **empty** descriptor list — ART's generated
`Startup-Sequence` never ran `C:AddDataTypes`. These four are the descriptors a
real system carries. A tree ART builds that then installs them is a tree whose
datatypes list is not empty by accident.

## Archivers

| Package | Source |
|---|---|
| LHA | Aminet `util/arc/lha_68k` |
| LZX | Aminet `util/arc/lzx121r1` |
| UnZip 5.52 | Aminet `util/arc/UnZIP552` |
| Zip 2.32 | Aminet `util/arc/ZIP232` |
| XAD Master | Aminet `util/arc/xadmaster020` |
| XPK User | Aminet `util/pack/xpk_User` |

## System, shell and boot

| Package | Source | Note |
|---|---|---|
| **SetPatch 44.38** | Vendor `cdn.cloanto.com/pub/amiga/SetPatch-44-38.lha` ⚠️ | [ART-159](ISSUES.md) is about `SetPatch` being unplaced in ART's own tree |
| **Workbench Library 40.5** | Vendor `cdn.cloanto.com/pub/amiga/Workbench-Library-40-5.lha` ⚠️ | [ART-127](ISSUES.md) again — the tree lacked `workbench.library` |
| Installer 43.3 | Aminet `util/misc/Installer-43_3` | **The Amiga Installer itself.** `core/amigainstall` runs package installers; several need this present |
| SKick 3.46 | Aminet `util/boot/skick346` | Soft-kick a different Kickstart |
| ChangeBootPri | Vendor `thomas-rapp.hier-im-netz.de` | Boot priority — ART writes this field in the RDB |
| Reboot | Aminet `util/boot/reboot` | |
| Sysvars | Aminet `util/boot/sysvars` | |
| SnoopDOS | Aminet `util/moni/SnoopDos` | What a program is really opening — the Amiga-side equivalent of ART's operation log |
| BusTest | Aminet `util/moni/bustest` | |
| SysInfo | Vendor `download.d0.se/pub/SysInfo.lha` | |
| MD5SUM | Aminet `util/crypt/MD5SUM` | Checks on the Amiga what ART checked on the host |
| SRename, SearchReplace, CopyReplace, Mecho, TTTool, RexxTricks, Screentext, Wizard Library | Aminet / Thomas Rapp | Scripting and batch tools the imager's own install scripts lean on |

## WHDLoad

| Package | Source | Note |
|---|---|---|
| **WHDLoad** | Vendor `whdload.de/whdload/WHDLoad_usr.lha` | ART already installs WHDLoad packages onto images (its own WHDLoad screen) but does not place WHDLoad itself |
| WHDLoadWrapper | Vendor `ftp2.grandis.nu` (Turran search) | *"The Imager already installs the latest WHDLoad Wrapper so they will have the most compatible settings applied."* ART's Collection knows 1698 titles and could use this |

## Bundled small tools (Tom-Cat, Flype)

`ArewePal`, `AreWeOnline`, `CE`, `TomCopy`, `TomDelete`, `WaitforTask` — shipped
in the imager itself rather than downloaded. Not on Aminet; obtaining them
means asking their authors.

---

## What ART should take from this, and what it must not assume

**Take:** the *shape*. A catalogue is **data**, exactly as
`core/osinstall/recipes/*.json` already is — a package is a name, a source, a
destination and an install rule, not a code path. ART already has: a mirror
client that refuses arbitrary URLs, an archive gate, `safe_join`, a job runner,
and a manifest that records provenance per file. A catalogue entry is a small
JSON file on top of machinery that exists.

**Do not assume better exists.** The owner asked whether there are *"daha
güzelleri"* — nicer ones. **This document deliberately proposes none**, because
recommending an Amiga package on recalled reputation is precisely the kind of
claim this project has been burned by. What can be said honestly:

- Several entries are pinned to **old versions** (`UnZIP552`, `ZIP232`,
  `lzx121r1`, `MUI 3.8`, `MiamiDX 1.0c`). Whether a newer build exists and is
  better is a question for the Aminet index, which **ART can already query** —
  `core/sources` exists to answer exactly that.
- **Roadshow vs MiamiDX** is a real choice the imager makes by shipping both.
  The owner's own networking work is in this area and their preference
  outranks any survey.
- **AmiSSL** and **`icon.library`** are the two entries whose "latest version"
  the imager resolves at run time rather than pinning — the maintainers ship
  often enough that a pin goes stale. Anything ART does here needs the same.

**Answer the licences before the code.** Four entries carry a constraint that
is not about redistribution alone: Picasso96 (Jens Schönfeld's statement must
travel with it), Roadshow and iBrowse (demos), and the two Cloanto CDN files.
The imager's own approach — **download at install time rather than bundle** —
is the one that keeps it *"free and legal to distribute"*, and it is the same
answer ART's own source policy already reached.

## Sources

- <https://mja65.github.io/Emu68-Imager/included.html> — read 2026-08-22; every URL above is quoted from it
- <https://mja65.github.io/Emu68-Imager/packages.html> — the add-on mechanism and the WHDLoad Demos pack
- <https://mja65.github.io/Emu68-Imager/amigautilities.html> — what runs on first boot
- Full research: [superpowers/specs/2026-08-22-emu68-imager-research.md](superpowers/specs/2026-08-22-emu68-imager-research.md)
