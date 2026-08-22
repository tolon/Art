# Package bundles — a curated catalogue ART can fetch in one go

**Status:** approved 2026-08-22 by the owner, not yet implemented.
**Work-list item:** 10, *"The package catalogue as data"*.
**Phase 1 scope:** download only. Installing these onto an Amiga volume is a
separate design.

---

## What the owner asked for

> *"Bunları bir dağıtım paketi olarak gösterelim, paket olarak seçsin topluca
> sıra ile indirsin ART en güzeli. Bu şekilde dağıtımda kullanabileceğimiz
> program paketleri oluştururuz."*

Named **sets** of Amiga software. The user picks a set, ART downloads its
packages **in order**, and the result is a building block for a distribution.

Two decisions the owner made during the design, both recorded here because
they narrow the work:

- **All sources, not just Aminet.** *"bütün kaynakları al, biz open source
  topluluk için bir program hazırlıyoruz."*
- **Permissions are the owner's to obtain, but ART must still say so.** *"İzin
  işlerini ben hallederim … gene de izin isteyen programlar için bir uyarı
  yazısı GUI'de olsun. Bu programlar izne tabidir diye. Lisansa da ekleriz."*
  The largest Amiga forum in Germany found the project on GitHub and wrote to
  them.

---

## The research, and what it settled

Three real implementations answer "how does a distribution get its software?"
three different ways. All three were **read**, not recalled; sources at the
foot of this section.

| | How software arrives | Who fetches it |
|---|---|---|
| **Emu68 Imager** | downloaded *during image creation* from real URLs | the tool |
| **AmiKit** | pre-installed in the image; **Live Update** afterwards, plus a **RabbitHole** drop-folder (put an archive in, it installs itself) | the vendor, then the tool |
| **HstWB Installer** | user picks from named packages | **the user** downloads the package archives first |

ART's answer is the Imager's — the tool fetches — because that is what the
owner asked for and because `core/sources` already does it.

**The Imager's list, enumerated rather than summarised: 60 entries** in its
*"Downloaded and Installed During Image Creation"* section. 45 Aminet, 15
elsewhere (3 GitHub, 2 Cloanto CDN, 2 UHC Tools, 2 Thomas Rapp, and one each
from ibrowse-dev.net, dopus.free.fr, whdload.de, download.d0.se,
ftp2.grandis.nu). **`docs/amiga-package-catalogue.md` says 47 packages, 41
Aminet — it undercounts and must be replaced by the real list.**

**HstWB's package manifest was fetched, not guessed** —
`classicwb-lite-package/package/hstwb-package.json`:

```json
{
    "contentIds": ["classicwb", "amigaos"],
    "name": "ClassicWB LITE",
    "version": "28.2.1",
    "priority": 1,
    "assigns": [{ "name": "SYSTEMDIR" }],
    "amigaOsVersions": ["3.1", "3.1.4", "3.2"]
}
```

It confirms three shapes ART already has, from an independent implementation:
`priority` is install **order**, `contentIds` is conflict detection (ART's
`overrides`), and `amigaOsVersions` is ART's `release`. The design below reuses
ART's own names rather than importing these.

**AmiKit ships 420 programs and no AmigaOS or ROM** — the same division ART
keeps. Its changelog names its packages with versions, and most are Aminet.

### Three things the source list cannot be copied from

Found by reading, and each one is a reason the catalogue is verified data
rather than a transcription:

1. **A broken path.** The Imager's page gives ViNCEd as
   `aminet.net/packageutil/shell/ViNCEd` — a missing slash. Checked against
   Aminet: the real package is `util/shell/ViNCEd`, v3.109, `ViNCEd.lha`,
   881 447 bytes compressed, released 2025-11-02.
2. **An address that is not an address.** WHDLoadWrapper points at an FTP
   *search form* with query parameters, `username=ftp%2Cany` among them. ART's
   rule — configured mirrors, never a caller-supplied URL (§41.5.7) — cannot
   accept it as written.
3. **Three entries are searches, not paths.** PeterK's icon.library, AmiSSL and
   WHDLoadWrapper are given as *"latest version"* queries. Pinning a fixed path
   for these would be wrong the week after it was written; ART's own version
   resolution is the right answer, so the schema has to express the difference.

### Not settled, and said so

**What Screentext does is unverified.** Thomas Rapp's downloads page could not
be fetched and no search result described it. It is filed under `kabuk` on the
strength of nothing; the catalogue entry must not be written until somebody
looks. `ChangeBootPri` is filed by its name, which is self-describing, and that
is a weaker claim than a read one — noted rather than hidden.

**Sources:** [Emu68 Imager — What's
Included](https://mja65.github.io/Emu68-Imager/included.html) ·
[AmiKit changelog](https://file.amiga.sk/amikit/doc/changelog_win.html) ·
[AmiKit](https://www.amikit.amiga.sk/) ·
[HstWB Installer](https://hstwb.firstrealize.com/) ·
[classicwb-lite-package](https://github.com/henrikstengaard/classicwb-lite-package) ·
[Aminet ViNCEd](https://aminet.net/package/util/shell/ViNCEd)

---

## The sets

**14 sets, 62 entries.** The arithmetic, left visible: the Imager's 60, plus
the owner's own **tolunnet** and **tolunwifi**. Roadshow is absent — tolunnet
replaces it, which removes the hardest of the licensed entries because the
owner holds the distribution right.

`⚠` marks an entry whose catalogue record carries a `permission` field: a GUI
warning before the tick, and a line in `THIRD_PARTY_LICENSES.md`.

### 1 · `emu68` — the card and its kernel (4)

Emu68 (PiStorm) · Emu68 (PiStorm32-lite) · Emu68 Tools · GENet.device.
All GitHub release assets. ART already reads the Emu68 archive
(`core/card/intake.rs`).

### 2 · `arsiv` — archivers (6) · **first in order**

LHA · LZX · UnZip · ZIP · XAD Master · XPK User.
Everything else arrives as `.lha` or `.lzx`; without these the Amiga side
cannot open what it was sent.

### 3 · `dosya-sistemi` — disks and filesystems (6)

PFS3 · Fat95 · **File Sys Box → SMBFS** (a real dependency) · CFD 1.33 ·
IDEfix 97.

### 4 · `temel` — shared libraries (5)

MUI 3.8 · Reqtools · Wizard Library · Installer 43.3 · PeterK's icon.library.
MUI before anything that needs it.

### 5 · `ag` — networking (9)

**tolunnet ⟷ MiamiDX — an `exclusiveGroup`**: both are TCP/IP stacks and only
one may be on, the same shape the AmigaOS 3.2 recipe already uses for its
Modules disks. Plus MiamiDX Main, MiamiDX MUI (after MUI), AmiSSL, Prism2,
aget, Sntp, AmiSpeedTest, **tolunwifi**.

### 6 · `grafik` — RTG and datatypes (5)

Picasso96 ⚠ · akGIF · akJFIF · akPNG · akTIFF

### 7 · `masaustu` — shell and file manager (4)

**Directory Opus 4.16JR ⟷ 4.17pre21 — an `exclusiveGroup`** · ViNCEd · CLICon

### 8a · `teshis` — diagnostics (4)

SnoopDOS · SysInfo · BusTest · MD5Sum

### 8b · `acilis` — boot and system control (5)

Reboot · ChangeBootPri · SKick · Sysvars · SetDST

### 8c · `kabuk` — CLI, scripting and text (8)

SRename · CopyReplace · SearchReplace · Mecho · TTTool · RexxTricks ·
Jano Editor · Screentext *(purpose unverified — see above)*

### 9 · `whdload` (2)

WHDLoad · WHDLoadWrapper *(address is a search form, not a path)*

### 10 · `medya` (1)

Hippo Player

### 11 · `amigaos-eki` ⚠ (2)

SetPatch 44.38 · Workbench-Library 40.5 — both Cloanto CDN.

### 12 · `ibrowse` ⚠ (1)

iBrowse demo.

### `hepsi` — the union

**Computed, never listed.** A test asserts every catalogue entry belongs to at
least one set, so nothing can be orphaned and the union cannot drift from the
catalogue.

---

## The catalogue schema

Data, not code — the rule the install recipes already follow: *a fourth package
is a JSON file, not a code path.* Lives in `core/sources/bundle/`, with the
shipped JSON beside it in `bundles/`.

`bundle` and not `catalog`: `core/sources/catalog` is already the **synced
Aminet index**, and a second meaning for one word in one module is how two
things start being confused for each other.

```json
{
  "id": "pfs3",
  "name": "PFS3",
  "source": { "aminet": { "path": "disk/misc/PFS3_53" } },
  "order": 10,
  "exclusiveGroup": null,
  "requires": [],
  "permission": null
}
```

### `source` is a closed enum, and the fifth variant is the point

- **`aminet { path }`** — resolved through the existing mirror engine.
- **`aminet-search { query }`** — the *"latest version"* entries. ART's own
  version resolution, rather than a fixed path that is wrong a week later.
- **`github-release { repo, asset }`** — Emu68, Emu68-tools, GENet.
- **`mirror { mirror, path }`** — Cloanto CDN, whdload.de, UHC Tools and the
  rest, each a **configured** mirror. There is no variant that takes a URL, so
  there is no function anywhere that fetches a caller-supplied one — §41.5.7's
  guarantee is worthless if it lives anywhere but the type.
- **`user-supplied { why }`** — **an entry ART cannot fetch, saying so before
  the user asks it to.** WHDLoadWrapper is the case: its address is a search
  form. ART does not paper over it and does not silently drop it; it names it
  and says why. The precedent is `HostPlacementBlock` (ART-166) — a closed enum
  whose whole purpose is to state a known impossibility *before* the tick
  rather than let a raw error arrive after it. §10 and §89 both require this.

### `permission`

Non-null means: the screen shows a warning **before** the entry can be ticked,
and `THIRD_PARTY_LICENSES.md` carries a line for it.

**A test binds the two.** Every permission-flagged entry must appear in the
licence file or the build fails — the same shape as the i18n parity test. The
owner's *"lisansa da ekleriz"* then cannot be forgotten, because forgetting it
is a red suite.

Permission is recorded even though the owner is obtaining it. The reason is
ART's own rule about telling the user what it is doing: a permission may be
granted to ART's distribution rather than to the file, and it does not travel
with the file when the user moves it somewhere else.

### The set schema

```json
{ "id": "dosya-sistemi", "order": 30,
  "entries": ["pfs3", "fat95", "filesysbox", "smbfs", "cfd133", "idefix97"] }
```

Sets reference catalogue **ids**; they never redeclare a package. One
catalogue, many sets, and a package in two sets is one file.

Download order = set `order`, then entry `order`.

### One correction, made during the spec's own review

The paragraphs above justify `order`, `exclusiveGroup` and `requires` with
reasons that are true of **installing** and not of downloading. Archivers are
needed first because nothing can be unpacked without them; SMBFS needs File Sys
Box present; two TCP/IP stacks cannot both be active. **None of that is true of
a folder of downloaded files.** Fetching MUI after MiamiDX-MUI produces exactly
the same bytes on disk.

So, stated plainly rather than left to be discovered:

- **`order` does one thing in phase 1** — it fixes the sequence the job runs
  in, which is what the owner asked for and what makes the report readable.
- **`exclusiveGroup` and `requires` do nothing in phase 1.** They are recorded
  because they are properties of the *packages*, established here from real
  material while it is in front of us, and because phase 2 cannot be written
  without them. A field that exists and is not yet read is honest; a field
  invented later from memory is not.

The screen may still *show* an exclusive group — "these two are alternatives" is
useful when choosing what to download — but it does not enforce one. Downloading
both stacks is a legitimate thing to want.

---

## The bulk download job

Built on what exists: `core/jobs`' `ProgressSink` and cancel flag,
`core/sources/fetch.rs`' resumable download, `library.rs`' record of what has
been fetched.

**Sequential, not parallel.** The owner asked for *"sıra ile"* and it is also
right: Aminet's mirrors are volunteer-run, and a readable report needs a
determinate order.

**One entry failing does not fail the set.** Each entry carries its own
outcome and **the outcomes stay distinct** — five endings, five sentences, five
different next steps:

| Outcome | What it means |
|---|---|
| `Downloaded` | fetched and recorded |
| `AlreadyHave` | the library already holds this exact file |
| `Refused { why }` | ART cannot fetch it and said so up front — today only a `user-supplied` entry |
| `Failed { error }` | it was tried and the mirror or the network said no |
| `Skipped` | the user cancelled before reaching it |

Collapsing these into "did not succeed" is precisely the defect class this
project's own rules name as its most expensive. A user told "failed" about an
entry ART never attempted has been told something false.

**Cancellation is checked between entries, never mid-write** — the standing
rule, and what makes cancelling safe: unfinished work, never a half-written
file.

**Nothing is fetched until the user presses the button.**

Downloads land in the existing user-chosen folder, and `library.rs` records
which set asked for them. Two sets naming the same package download it once.

---

## The screen

A new section inside the Aminet studio (`/aminet`), where the download folder
and the library already live.

Sets as cards: entry count, total size where known, a tick per set and per
entry. Per-entry progress while the job runs, and a report at the end that names
each of the five outcomes separately.

**The permission warning renders above the tick**, for any set containing a `⚠`
entry, naming the entries it applies to. It is **informational, not a gate**:
there is no separate "I accept" control, because the owner is obtaining the
permissions and a second confirmation would imply the user is the one granting
something. Ticking is the acknowledgement; the sentence is there so nobody
learns afterwards that a file they now hold came with a condition. This is the
ART-166 rule — say the known thing *before* the tick — applied to a condition
rather than to an impossibility.

Whether this eventually deserves its own screen rather than a section of
`/aminet` is left open; it is a move, not a redesign, and the owner flagged it
as arguable.

---

## Testing

**Over every shipped JSON, in Rust:** ids unique; every set entry resolves to a
real catalogue id; every catalogue entry belongs to at least one set; every
`permission` entry appears in `THIRD_PARTY_LICENSES.md`; every Aminet path
passes `PackageRef`'s existing validation; **no variant anywhere carries a raw
URL**.

**The job, against the existing in-memory mirror double:** the full sequence in
order, one entry failing without taking the set with it, cancellation between
entries, and a resumed partial download. No socket opens.

**Frontend:** the set list, the permission warning rendering before the tick,
and the report showing five distinct endings rather than two.

**And something that is not ART:** `scripts/catalogue-check.py` asks live
Aminet and the configured mirrors whether all 62 paths actually resolve. Run by
hand, not in CI — the same place and for the same reason as
`rom-table-check.py`, `fat-oracle-check.py` and `pfs3-oracle-check.py`. It is
the scripted form of the check that caught ViNCEd.

---

## What phase 1 does not do

**It does not install anything on an Amiga.** It downloads, records, and says
where the files are. Installing them is a separate design: each package has its
own installer, and ART-166's wall — a package whose payload only its own
Amiga-side installer can open — stands there too. Tying a working download to
an unbuilt install is how a working thing gets held hostage by an unfinished
one.

**It does not reach parity with AmiKit's 420 programs.** The catalogue is a
list of what *could* be fetched; growing it is adding JSON files, which is the
whole point of making it data.
