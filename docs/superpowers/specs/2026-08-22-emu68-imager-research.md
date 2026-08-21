# How the established tools build a PiStorm card — read, not recalled

**Date:** 2026-08-22
**Status:** research. No design decision is taken here; this is the material a
design would argue from.
**Why:** the owner asked for it in as many words — *"Bazı şeyleri yanlış
yapmış olabiliriz… tam olarak incele, ihtiyacımız olan bilgileri topla hata
yapmayalım."* CLAUDE.md's first rule under "Research before design" says to run
and read the thing you are about to depend on rather than trust a recollection
of it. This is that, for the card layout ART already writes.

---

## What was fetched, and how — so this can be re-run rather than re-trusted

Two of the four sources do **not** give up their content to an ordinary fetch,
and both failures are silent ones that would leave a reader thinking the page
was thin rather than unread.

| Source | What it is | How to actually read it |
|---|---|---|
| [retrosix.wiki/wiki/pistorm-emu68-imager-amiga](https://retrosix.wiki/wiki/pistorm-emu68-imager-amiga) | RetroSix's guide to the Emu68 Imager | **The body is not in the DOM.** It is embedded in the HTML as an escaped ProseMirror JSON document (`"`-quoted), near byte 533 000. `scripts/`-style extraction: unescape, `raw_decode` from the first `{"type": "doc"`, walk it. A plain fetch and a headless-Chrome `--dump-dom` both return **navigation only** |
| [github.com/mja65/Emu68-Imager-Software](https://github.com/mja65/Emu68-Imager-Software) | The imager itself — PowerShell, MIT | `raw.githubusercontent.com` for files; the GitHub trees API for the listing. The README is four lines and points at the docs site |
| [mja65.github.io/Emu68-Imager](https://mja65.github.io/Emu68-Imager/) | The imager's own documentation | The index lists the pages; each page has to be fetched on its own |
| [retro32.com … pistorm-installation-and-setup-guide](https://www.retro32.com/amiga-resources/240820213135-pistorm-installation-and-setup-guide-apps-pidisk-networking-and-rtg-a314) | A 2021 setup guide | **403 to a plain fetch.** Returns 200 with a browser `User-Agent` |

Everything below is quoted from those files, not summarised from memory.

---

## 1. The imager drives `hst-imager`, and the commands are the layout

`Assets/Functions/ProcessInstallFiles/Get-DiskStructurestoMBRGPTDiskorImageCommands.ps1`
builds a list of `hst-imager` commands. Stripped of PowerShell quoting, the
shape is:

```
mbr part add    <image> 0xb  <bytes> --start-sector <n>     # the FAT32 boot partition
mbr part format <image> <n> <VolumeName>

mbr part add    <image> 0x76 <bytes> --start-sector <n>     # an Amiga area
rdb init            <image>\mbr\<n>
rdb filesystem add  <image>\mbr\<n> <FileSystemPath> <DosType>
rdb part add        <image>\mbr\<n> <DeviceName> <DosType> <bytes> \
                    --buffers <b> --max-transfer <mt> --mask <m> \
                    --no-mount <T|F> --bootable <T|F> --boot-priority <p>
rdb part format     <image>\mbr\<n> <index> <VolumeName>
fs mkdir / fs copy  … --makedir --recursive TRUE --force TRUE
```

**This confirms ART's own model from an independent implementation.**
`docs/sd2-card-layout.md`'s central claim — *"a card is a list of disks, not a
disk"*, an MBR whose `0x76` primaries each carry **their own RDB at a byte
offset inside the card** — is exactly what these commands construct:
`rdb init` is run **per `0x76` partition**, not once for the card. ART worked
that out from two real cards; the established tool builds it the same way.

**It also means ART's `hst-imager` fallback is not a lesser path.** The
reference tool for this job *is* `hst-imager` with a GUI on top; ART's
`NativeFormatter` is the unusual part, not the fallback ([ART-120](../../ISSUES.md)).

### Constants, from `Assets/Variables/SetVariables.ps1`

| Name | Value | Note |
|---|---|---|
| `Emu68BootCmdline` | `sd.unit0=rw emmc.unit0=rw` | **this is the whole `cmdline.txt`** |
| `MBRSectorSizeBytes` | `512` | |
| `MBRFirstPartitionStartSector` | `2048` | |
| `MBRPartitionsMaximum` | `4` | primaries only, as ART assumes |
| `AmigaPartitionsperDiskMaximum` | `10` | per `0x76` area |
| `AmigaRDBHeads` / `AmigaRDBSectors` / `AmigaRDBSides` | `16` / `63` / `2` | |
| `AmigaRDBBlockSize` | `512` | |
| `MBROverheadBytes` | `1048576 + 50688` | comment: *"Allowing for partition to start at sector 2048 and leave space HST Imager appears to require"* |

### Sizes, from `Get-MinimumPartitionSizes.ps1`

The live values arrive from CSVs that ship in the **release**, not the
repository, so these are the file's own commented reference block — indicative,
not authoritative:

```
FAT32Divider = 15            Fat32DefaultMaximum      = 1 GB
WorkbenchDivider = 15        WorkbenchDefaultMaximum  = 1 GB
Fat32Minimum = 35 MB         SystemMinimum            = 100 MB
PFS3Minimum  = 10 GB         PFS3Maximum              = 101 GB
```

and the rule around them, which *is* in code:

> boot default = `min(diskBytes / 15, 1 GB)`; Workbench default =
> `min(diskBytes / 15, 1 GB)`

Only `'PiStorm - MBR'` is implemented. `'PiStorm - GPT'` and `'Amiga - RDB'`
both call `Write-ErrorMessage -Message "Error in coding"` and close the window.

---

## 2. `config.txt` selects **three different kernels by GPIO** — one file, all boards

`Assets/AmigaFiles/EMU68Boot/config.txt`, verbatim in the parts that matter:

```
#-Pistorm detection-#
gpio=0-27=ip
gpio=0-27=pu

##If keyboard reset is kept hold, then put pistorm32 into stealth mode, boots stock A1200
[gpio4=0]
kernel=Emu68-pistorm32lite
initramfs ps32lite-stealth-firmware.gz

[all]
##PiStorm32lite Boot
[gpio24=0]
kernel=Emu68-pistorm32lite
[ROMPATH]

[all]
# PiStorm16 Boot
[gpio24=1]
kernel=Emu68-pistorm16
[ROMPATH]

[all]
## PiStorm Boot
[gpio17=0]
kernel=Emu68-pistorm
[ROMPATH]
```

with, elsewhere in the same file: `arm_64bit=1`, `total_mem=2048`
(*"Necessary for CM4/Pi4"*), `gpu_mem=32`, `hdmi_force_hotplug=1`,
`avoid_warnings=1`, `boot_delay=0`, `disable_splash=1`, `bootcode_delay=1`,
and `[pi4] arm_boost=1`.

**Three things here are worth ART's attention.**

1. **The board is detected at boot, not chosen when the card is written.** One
   card boots a PiStorm, a PiStorm32-lite or a PiStorm16 depending on which
   GPIO is pulled. [ART-091](../../ISSUES.md) is about an archive name meaning
   different boards in the two release lines, and [ART-103](../../ISSUES.md) is
   about naming the kernel the release actually ships — **neither asks what
   this file asks**, which is whether the config ART writes keeps all three
   stanzas or collapses them to the one board the user said they had. A card
   that boots only one board is not wrong, but it is a narrowing, and ART
   should know which it is doing. **Unverified against ART's own writer — that
   check has not been run.**
2. **`[ROMPATH]` is a placeholder the imager substitutes**, and the mechanism
   is `initramfs`: the comment says *"The ROM is selected through initramfs
   parameter… give the full name of your kickstart file which you have placed
   on the RasPi boot partition"*, with the firmware, when there is one,
   **before** the ROM name and comma-separated —
   `initramfs firmware.bin.gz,kick.rom`.
3. **Stealth mode exists**: holding keyboard reset boots the stock A1200 with
   `ps32lite-stealth-firmware.gz`. ART writes no such stanza and nothing in
   `docs/` mentions it.

---

## 3. What the Emu68 Imager asks the user for — and the two patterns ART is copying

From RetroSix's guide (the body extracted as described above):

> This tool will: Pre-format and partition the SD card · Install Emu68 files ·
> Install Workbench · Install WHDLoad · Install many useful default tools ·
> Copy any additional folder you like to the Workbench partition · Installs a
> browser and sets up WiFi using the Raspberry Pi WiFi chip

Run `PistormImagerGui.cmd` (the repo's own entry point is `Emu68Imager.cmd`);
the docs site says the supported releases are **AmigaOS 3.1, 3.2, 3.2.2.1 and
3.9**.

**The `Check` button, confirmed at the source.** The
[flow design](2026-08-21-os-builder-flow-design.md) cites it as prior art for
validating a picked folder at the moment of picking. RetroSix states it as an
instruction, and as a warning:

> Once the folders are selected, you can click Check to confirm all files are
> located. **Make sure to click Check not just set the folders**, as it will
> search and confirm the files needed are present.

That is [ART-199](../../ISSUES.md)'s fix, arrived at independently by the tool
this project compared itself against.

**The 3.2.2.1 media is spread across three folders, and the user must merge
them by hand:**

> - `AmigaOS3.2CD\ADF`
> - `AmigaOS3.2CD\Hotfix3.2.2.1\Update3.2.2\ADFs\Hotfix`
> - `AmigaOS3.2CD\Update3.2.2\ADFs`
>
> In a folder that then contains all of those files, select that as your ADF
> folder and click Check.

**This is the finding with the most direct bearing on ART's OS install.**
`core/osinstall/scan.rs` scans **one** media folder. For AmigaOS 3.2.2.1 there
is no one folder to scan — the release's own layout puts base ADFs, an update
and a hotfix in three places, and even the reference tool does not resolve it,
it tells the user to copy them together first. ART ships a 3.2 recipe and a 3.9
recipe and has never been pointed at 3.2.2.1. Whether ART should scan several
folders, or say the same thing the imager says, is a design question — but a
recipe that assumes one folder is a recipe that cannot express this release.

---

## 3a. The instructions page, read in full — ten things ART can act on

`instructions.html` is the detailed page and it answers more about ART than
anything else read this session. Each item below is quoted or paraphrased from
it, with what it means for ART named.

### 1. Media is identified by **content**, not by name — and ART identifies by volume name

> The names of the ADF files and Kickstart ROMs are **not important**. You can
> name them anything since the imager tool will **checksum and compare them to
> a database of known good disk and ROM images**. This system prevents corrupt
> or altered installations from causing issues.

ART already does exactly this **for ROMs** — 154 Kickstart dumps generated from
amitools' Remus database, verified on every CI run ([ART-104](../../ISSUES.md)).
It does **not** do it for ADFs: `core/osinstall/scan.rs` matches media by the
volume name inside the image. That is weaker in a specific way — a *modified*
`Workbench3.2` still calls itself `Workbench3.2`.

[ART-136](../../ISSUES.md) is the same lesson from the other side: the ADF path
once assumed a **filename** convention no real file used, zero of 847. Volume
names were the fix and they were the right fix; a content database is the next
step, not a contradiction of it.

### 2. Escom ADFs are defective, and the imager prefers Commodore/Cloanto

> ADF images from the **Escom** Amiga distribution are missing, among other
> things, **language files (keyboard layouts too)** and some tools. These
> images, while usable, should be avoided — if Commodore or Cloanto ADF images
> are available then the imager tool will **prioritise** their use.

ART has no notion of an ADF's *provenance* and would use whichever it found.
For a Turkish-facing tool, "missing keyboard layouts" is not a small defect.

### 3. A named tool silently modifies ADFs

> Certain tools (e.g. **DiskFlashback**) were noted as causing modifications to
> the ADF, that while still useable, could cause the tool not to recognise
> them.

Worth knowing before ART's own hashes disagree with a user's disk and ART
blames the disk.

### 4. Emu68 wants **A1200** Kickstarts, whatever Amiga you have

> Please note that **only A1200 versions** of the Kickstart ROMs are supported
> as these are recommended for Emu68 **regardless of the Amiga model you are
> using**.

ART's ROM pairing (G9) reasons about version and machine from the tree and the
card. This is a flat constraint of the *platform* that ART does not encode.

### 5. The ROM each release wants, exactly

| AmigaOS | Kickstart |
|---|---|
| 3.1 | 3.1 A1200 **40.68** |
| 3.2 | 3.2 A1200 **47.96** |
| 3.2.2.1 | 3.2.2 A1200 **47.111** |
| 3.2.3 | 3.2.3 A1200 **47.115** |
| 3.9 | 3.1 A1200 **40.68**, or **Kickstart 3.x A1200 Cloanto** |

The owner's own material includes 40.68 and a licensed V47. Note 3.9 explicitly
accepts a **Cloanto** ROM.

### 6. The default layout, stated plainly

> Emu68 loads files from a **FAT32** partition. Additionally, MBR partitions
> with the Partition ID of **0x76** are identified by Emu68 as being
> **separate Amiga drives**. … there can be a **maximum of 4 MBR partitions**.
> … **By default, the Emu68 Imager creates one FAT32 partition and one 0x76
> partition.** … You can have a maximum of **10 Amiga partitions** within the
> 0x76 partition. This is **not** a limitation of AmigaOS — rather it's placed
> within Emu68 Imager such that the interface does not become unwieldy.

Three things settled: ART's model is right; the **default is 1 + 1**; and the
ten-partition cap is a **UI choice, not a format rule** — so ART must not copy
it as though it were one.

### 7. `.vhd` as well as `.img` — ART writes only `.img`

> a **.img** file is the fixed size of the image. For example, if you select
> 32GiB you will need 32 GiB of space… Alternatively, you can use a **.vhd**
> file which will **dynamically resize based on the contents**… recognised both
> in WinUAE and Windows.

ART's output is always a full-size `.img`. A 32 GiB card image costs 32 GiB of
the user's disk — on a machine whose `C:` ART has already filled once
([ART-196](../../ISSUES.md)). This is the cheapest real improvement on the list.

### 8. UNC paths do not work

> **UNC links will not work** due to limitations in one of the supporting tools
> (HST-Imager).

ART keeps `hst-imager` as a named fallback, so this limitation is ART's too on
that path — and ART says nothing about it.

### 9. WiFi credentials live on the image

> Please bear in mind that your **wifi name and password are stored on the
> image** should you use this option!

If ART ever does G14's WiFi pre-seeding, that sentence has to come with it.

### 10. The HDMI mode is fixed at boot

> the output of your RaspberryPi is **fixed after boot**, and cannot be changed
> without altering the `config.txt` file on the FAT32 partition.

Which is another reason [ART-204](../../ISSUES.md) matters: `config.txt` is the
only place some of these decisions can ever be changed.

### And one about shape rather than facts

The imager has a **Select Packages** screen — icon set (Standard vs
GlowIcons), which languages, which software — and a **load/save settings** file
written automatically *"if you ever need to re-run the install"*. ART's
remembered settings already do the second. The first is what
[the package catalogue](../../amiga-package-catalogue.md) would be the data
for.

---

## 4. `hdf2emu68` — the narrow tool, and what it does *not* do

[github.com/PiStorm/hdf2emu68](https://github.com/PiStorm/hdf2emu68), README in
full:

> Usage: `hdf2emu68 <source_image>` … The resulting EMU68 compatible image will
> be named `emu68_converted.img`
>
> **Don't forget to copy your EMU68 and Kickstart file to the FAT32 partition
> after you have written the final image to your SD card.**

So the conversion produces the MBR-plus-`0x76` shape and leaves the boot
partition's contents to the user. It is the same layout, reached from an
existing HDF instead of from install media — which is a route ART does not
offer and could.

---

## 5. The 2021 guide is a **different software stack**, and mixing them is the trap

The retro32 article (Aug 2021) is not about Emu68 at all. It sets up
**Raspberry Pi OS Lite** and runs the userspace emulator:

```
cd pistorm
sudo emulator
sudo nano pistorm/default.cfg     # setvar RTG / setvar piscsi / setvar A314
                                  # setvar piscsi0 32wb.hdf
```

with the Kickstart copied into the `pistorm` folder **named `kick.rom`**, a
`pistorm.service` systemd unit for autostart, A314 for networking and Picasso96
for RTG. It also advises *"creating a partition of around 500MB"* for a
Workbench built in WinUAE.

**None of that applies to an Emu68 card.** Emu68 is bare metal: no Linux, no
`default.cfg`, no `piscsi`, and the Kickstart is passed by `initramfs` from the
FAT32 boot partition rather than sitting beside an emulator binary. The two
stacks share a board and nothing else, and a guide for one read as a guide for
the other is exactly the shape of mistake the owner asked to avoid. Recorded
here so nobody has to discover it twice.

---

## What this changes for ART — questions, not answers

Nothing here is acted on yet. Each of these is a check ART has not run:

1. **Does ART's generated `config.txt` keep all three GPIO stanzas?** If it
   writes one `kernel=` line, a card built for one board will not boot another.
   Read `core/card/payload.rs` against § 2 above.
2. **Does ART write a `cmdline.txt` equal to `sd.unit0=rw emmc.unit0=rw`?**
   The imager's is exactly that string.
3. **Does ART pass the Kickstart by `initramfs`,** with the firmware before the
   ROM when there is one?
4. **Do ART's default sizes agree with `min(disk/15, 1 GB)`** for both the boot
   and the system partition, and with a 35 MB FAT32 floor?
5. **Does ART leave `1048576 + 50688` bytes of MBR overhead**, or its own
   number? The imager's comment says the extra 50 688 is what `hst-imager`
   appears to want.
6. **Can a recipe express AmigaOS 3.2.2.1's three media folders at all?**
7. **Stealth mode** — is a card ART builds able to boot the stock A1200, and
   should it be?

The first five are all one `grep` and one comparison each. They are listed
rather than answered because this document is research, and because answering
them by reading ART's code is how the last five rounds each produced a
confident wrong sentence.

## Sources

- <https://retrosix.wiki/wiki/pistorm-emu68-imager-amiga> — fetched 2026-08-22, body extracted from the embedded document as described above
- <https://github.com/mja65/Emu68-Imager-Software> — `README.md`, `Assets/Variables/SetVariables.ps1`, `Assets/Functions/SetupDisk/Get-MinimumPartitionSizes.ps1`, `Assets/Functions/ProcessInstallFiles/Get-DiskStructurestoMBRGPTDiskorImageCommands.ps1`, `Assets/AmigaFiles/EMU68Boot/config.txt`, all read at `main` on 2026-08-22
- <https://mja65.github.io/Emu68-Imager/> — index page, 2026-08-22
- <https://github.com/PiStorm/hdf2emu68> — `README.md`
- <https://www.retro32.com/amiga-resources/240820213135-pistorm-installation-and-setup-guide-apps-pidisk-networking-and-rtg-a314> — Aug 2021, fetched 2026-08-22 with a browser `User-Agent`
