# PiStorm under Emu68: which Kickstart, and the 4 GB line (research, 2026-09-11)

Why this exists: the owner wrote a card from `main-09d52fb` with the A1200's Kickstart 3.1
(`amiga-os-310-a1200.rom`, 40.068) on an A500 + classic PiStorm, and ART's card plan warned *"not a ROM
for the Amiga you chose — usually a machine that does not come up"*. The owner: *"pistrom o kickten
boot eder"*. This note is what was checked from outside before ART's rule was changed (ART-307). It
describes the sources **on the day they were read**; re-check before building on it.

Not readable on the day: eab.abime.net (bot wall), amibay.com, retro32.com, retrobuddys.com (HTTP
403), pistorm.neocities.org (no DNS), wiki.classicamiga.com (TLS), the RetroSix wiki (login). The
forum side is thinner than it should be.

## 1. Which Kickstart boots on a classic PiStorm under Emu68

**Two or more sources**

- **The A1200 ROM is the recommended one on every model, A500 included.**
  - <https://mja65.github.io/Emu68-Imager/instructions.html> — *"only A1200 versions of the Kickstart
    ROMs are supported as these are recommended for Emu68 regardless of the Amiga model you are using"*.
  - <https://www.edsa.uk/blog/the-emu-and-the-storm> — PiStorm A500 under Emu68, *"opted for the KS3.1
    image I made from my 1200"*.
  - <https://github.com/michalsc/Emu68/issues/148> — an A2000 running the A1200 3.2.1 ROM *"without
    issue"*.
  - Musashi-era, before Emu68: <https://github.com/captain-amygdala/pistorm> README (*"A1200 3.1+
    Kickstart ROM is currently recommended"*), <https://tech.webit.nu/pistorm-basic-configuration/>.
  - And the owner's own A500, 2026-09-11.
- **3.2.x boots** — the physical 3.2 chip in an A500
  (<https://www.epsilonsworld.com/2021/08/pistorm-accelerator-with-amigaos-32-on.html>), the A1200 3.2.1
  ROM in an A2000 (#148), and Emu68-Imager supports 3.2, 3.2.2.1 and 3.2.3 (A1200 files only).
- **Older ROMs on A500/A600 were a bug, fixed.** #148 (Feb 2022): an A600 black-screened on 3.1. The
  maintainer closed it on 17 Sep 2022 — stray data on D0–15, *"Big Amigas are using pull-up resistors
  there, A500/A600 do not"* — and wrote *"I have tested Kickstarts from 2.0 up and with recent builds
  all of them were working properly."*

**One source**

- **A4000 ROM:** <https://mja65.github.io/Emu68-Imager/faqs.html> — *"ROM files MUST be from the A1200
  and NOT the A4000"*, no reason given.
- **3.1.4:** a purple screen with both the A1200 and A600 3.1.4 ROMs (#148, July 2022, before the fix);
  nothing after it.
- **A500/A600/A2000 3.1 (40.063):** only the maintainer's "2.0 up".

**Not found:** any Emu68 statement on the A3000 or CD32 ROM; any source giving the *reason* the A1200
ROM works on an A500 (the 68020+ CPU Emu68 provides is the obvious candidate, unstated anywhere read).

## 2. How Emu68 takes the ROM (its own code)

<https://github.com/michalsc/Emu68/blob/master/src/aarch64/start.c>, master, read 2026-09-11:
`initramfs` is the Pi firmware's option (it loads the file and reports where); Emu68 accepts exactly
256 KB (mirrored), 512 KB, 1 MB and 2 MB; a byte-swapped file is detected and fixed (*"Byte-swapped ROM
detected. Fixing..."*); there is no decryption, so an encrypted Amiga Forever ROM cannot be used
(also <https://pistorm.github.io/tutorials/sd_setup/> and the Imager FAQ). No file: the physical ROM is
used (edsa.uk). Emu68's own default is `initramfs kick.rom`
(<https://github.com/michalsc/Emu68/blob/master/scripts/config_pistorm.txt>).

## 3. The SD device and the 4 GB line

- **Driver:** `brcm-sdhc.device` on a Pi Zero 2 / Pi 3, `brcm-emmc.device` on a Pi 4 / CM4
  (<https://github.com/michalsc/Emu68/blob/master/docs/tutorials/SD_Preparation.md>, pistorm.github.io,
  webit). Only MBR type `0x76` areas are exposed; unit 0 is the whole card, read-only by default.
- **TD64 and NSD are supported** — by code and binary only, no document says so:
  <https://github.com/michalsc/Emu68-tools-old/blob/master/sdcard.device/src/beginio.c> handles TD64,
  NSD and HD_SCSICMD and answers NSCMD_DEVICEQUERY; the embedded `brcm-sdhc 1.9 (25.04.2026)` in
  `src/boards/brcm-sdhc.device.h` carries the same command table.
- **FFS 40.1 (Kickstart 3.1) cannot address past the first 4 GB of the device** — a position limit, not
  a size limit (<https://handwiki.org/wiki/Amiga_Fast_File_System>,
  <https://wiki.icomp.de/wiki/Harddrive_setup_with_TD64>, the PFS3aio readme
  <https://aminet.net/package/disk/misc/pfs3aio>). FFS V46 (3.1.4) and later use TD64/NSD natively
  (<https://www.hyperion-entertainment.com/index.php/frequently-asked-questions/55-amigaos-314/202-can-i-use-partitions-beyond-the-4gb-boundary>).
  So ART's FFS-past-4-GB warning for a pre-46 Kickstart stands.
- **Inference, untested:** each `0x76` area is its own unit from offset 0, so the 4 GB rule applies
  inside each area, not to its position on the card.
- **One source (the binary's strings):** the driver loads a filesystem from the RDB (*"Checking FSHD
  for DosType"*, *"LoadSegBlock"*), so an FFS 46/47 in the RDB would lift the limit under a 3.1 ROM.

## What ART changed on this

ART-307: `core::pistorm::rom_suits` — used only on the PiStorm screens — treats the A1200 Kickstart as
suiting every PiStorm Amiga, and the card plan's sentence for a ROM listed for another machine no longer
says the machine will not come up.
