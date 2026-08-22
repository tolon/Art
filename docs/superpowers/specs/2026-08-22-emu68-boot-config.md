# Where an Emu68 setting actually goes — and the version that moves it

**Date:** 2026-08-22
**Serves:** work-list items 6 and 7, the two left on the critical path to a
card that boots. The owner's standing decision: *"bizimkisi de çalışmalı."*

Everything below was read or measured today. Nothing here is recalled.

---

## The one that matters most: a hypothesis the artefact refuted

**Read first, and it produced a confident wrong answer.** Emu68's own
`documentation/overlays.md` opens with:

> *"Starting with Emu68 1.1 the use of cmdline.txt for adjusting Emu68
> parameters is obsolete. Instead, device tree overlays can be injected
> through config.txt…"*

and master's `src/aarch64/start.c::parse_cmdline` handles 28 tokens, of which
**none** is `sd.unit0`, `emmc.unit0`, `vbr_move` or `z2_ram_size`. Those live
in `src/overlays/{sdhc,emmc,emu68,z2ram}.dts` as overlay parameters instead.

The owner's Emu68 is **1.1.0-alpha.1**. So the hypothesis was: *ART's whole
`cmdline.txt` is inert on the card the owner actually has* — a second ART-204.

**Then the artefact was asked.** `E:\amiga\ProjeART\emu68\Emu68-pistorm.gz`,
decompressed and searched for every token ART writes:

```
Emu68 1.1.0-alpha.1 (09.02.2026) git:f9a5e33

IN the owner's own kernel (35 of 36 probed):
  sd.unit0, emmc.unit0, sd.verbose, sd.low_speed, sd.clock, emmc.verbose,
  emmc.low_speed, emmc.clock, vbr_move, z2_ram_size, nofpu, enable_cache,
  limit_2g, enable_c0_slow, enable_c8_slow, enable_d0_slow,
  move_slow_to_chip, copy_rom, checksum_rom, vc4.mem, chip_slowdown, cs_dist,
  swap_df0_with_df1/2/3, one_slot, debug, disassemble, async_log,
  fast_serial, buptest, bupiter, brcm-sdhc, brcm-emmc, bootargs

NOT in it (1):
  whole-drive-access          <- master's overlay property
```

**The hypothesis is refuted.** Their 1.1.0-alpha.1 build still parses every
token ART writes, and does *not* carry master's overlay property. ART's
`cmdline.txt` is correct for the Emu68 the owner has. Recorded as an
elimination so nobody walks this road again — and as one more instance of the
project's own rule: the documentation and the master source agreed with each
other and with nothing that was actually on the disk.

**What survives the refutation** is the *forward* half: a newer Emu68 does
move these settings, so a card ART builds today silently stops honouring them
the day the user updates. That is real, and it is what the fix below is for.

---

## `sd.unit0` — what it means, and why ART's default is already right

From Emu68's own `docs/Options.md`:

> `sd.unit0=off|ro|rw` — default **`ro`**. *"Unit 0 of the device represents
> the entire card, including partition table and FAT32 boot partition, so
> this should be used with care."*

Confirmed twice more in `src/overlays/sdhc.dts`, where the same setting is
`whole-drive-access` with `{off=0,ro=1,rw=2,=1}` and a node default of `<1>`.

And Emu68's own SD-preparation tutorial warns:

> *"…do not alter the drive type at address 0 (the entire microSD card), as it
> contains the FAT32 boot partition. Only the partition at address 1 should be
> configured for use as an Amiga hard drive."*

**So ART keeps `ro`.** The Emu68 Imager writes `rw` because *its* Install-folder
mechanism needs the Amiga to write to the boot partition; ART has no such
mechanism, and adopting the number without the reason is how a default becomes
a hazard. It stays a user choice, which it already is
(`Emu68Options::storage_unit0`).

---

## The hazard that decides *how* the config.txt half is written

`src/aarch64/start.c`, verbatim comment:

```c
/* If /emu68/brcm-emmc does not exist yet (was not loaded from overlay)
   create it now with sane defaults */
if ((e = dt_find_node("/emu68/brcm-emmc")) == NULL)
{
    ...
    /* If not injected by user, make brcm-emmc enabled on bcm2711
       (Pi4, CM4, Pi400), disabled otherwise */
    if (is_bcm2711) { status = "okay" } else { status = "disabled" }
```

**Read it the other way round and the hazard is plain: when the node *was*
loaded from an overlay, Emu68 never makes the by-model decision at all**, and
the overlay's own `status = "okay"` stands. So an unconditional
`dtoverlay=emmc,…` would enable `brcm-emmc` on a Pi3, which has no eMMC.

Therefore the overlay lines must be **board-conditional**, which is exactly
what `config.txt` sections are for — and what ART-204 already taught
`merge_config_txt` to key on.

---

## Which sections, exactly

From the Raspberry Pi documentation's own conditional-filter table
(`raspberrypi/documentation`, `config_txt/conditional.adoc`):

| Filter | Applicable models |
|---|---|
| `[pi3]` | 3B, 3B+, 3A+, Compute Module 3, Compute Module 3+ |
| `[pi4]` | 4B, 400, **Compute Module 4, Compute Module 4S** |
| `[pi02]` | Zero 2 W (also sees `[pi0w]` and `[pi0]` contents) |
| `[all]` | *"resets all previously set filters"* |

> *"It is usually a good idea to add an `[all]` filter at the end of groups of
> filtered settings to avoid unintentionally combining filters."*

And Emu68's own model→driver mapping, from its SD-preparation tutorial:

> *"Pi Zero2, Pi3A+, Pi3B, Pi3B+ should use `brcm-sdhc.device`"* … *"Pi 4B+ and
> CM4 should use `brcm-emmc.device`"*

The two line up exactly, and **`[cm4]` is not needed** — `[pi4]` already
covers CM4 and CM4S.

A missing overlay file is safe: the Pi bootloader *"silently ignores it and
keeps going"* rather than failing the boot, so the block below costs an old
Emu68 nothing.

---

## What ART writes, after this

**`cmdline.txt`** — for the Emu68 the owner actually has. Both storage
prefixes on the one line, so a card carried from a Pi3 to a Pi4 does not
silently lose the setting. The Emu68 Imager does exactly this
(`sd.unit0=rw emmc.unit0=rw`), which is the practical evidence that both are
tolerated; Emu68 picks the driver by model regardless, so the prefix that does
not match sets a property on a disabled device.

**`config.txt`** — for the Emu68 the owner will have. Appended as one managed,
board-conditional block, closed with `[all]` so nothing after it inherits a
filter:

```
[pi3]
dtoverlay=sdhc,unit0=ro
[pi02]
dtoverlay=sdhc,unit0=ro
[pi4]
dtoverlay=emmc,unit0=ro
[all]
```

Old Emu68 ignores the overlays (no such `.dtbo` in the archive); new Emu68
ignores the cmdline tokens. **The card works on both**, which is the whole
point, and neither mechanism has to guess which one is live.

---

## Sources, so the next reader can re-run rather than re-trust

- Emu68 `docs/Options.md` — the cmdline option list and `sd.unit0`'s default.
- Emu68 `documentation/overlays.md` — the 1.1 migration and the per-overlay
  parameter tables.
- Emu68 `src/aarch64/start.c` — `parse_cmdline`'s real 28-token list, and the
  by-model driver decision that an overlay bypasses.
- Emu68 `src/overlays/sdhc.dts`, `src/overlays/emmc.dts` — the overlay
  parameter mapping, and the `ro` default a second time.
- Emu68 SD-preparation tutorial — the model→driver mapping, the 200 MB FAT32 +
  `0x76` layout, and the warning about address 0.
- Raspberry Pi documentation, `config_txt/conditional.adoc` — the model filter
  table and `[all]`'s meaning.
- **The owner's own `Emu68-pistorm.gz`** — the measurement that refuted the
  hypothesis the first five sources supported.
- The Emu68 Imager's `DiskDefaults` table, read on 2026-08-22 and recorded in
  the work list.
