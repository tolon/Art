# Documentation assets

Images the documentation points at. Small, and only what a page actually uses.

## The seven screenshots (2026-09-08, two retaken 2026-09-09)

`files.png` · `collection.png` · `material.png` · `chain.png` · `pistorm.png` ·
`gotek.png` · `tools.png` — ART's own window, Windows 11, **dark theme**,
**English**, Power User mode, Application Size 120 %, maximised. Every file is
**3840 × 2088**. The sidebar caption reads `v0.9.1` in all seven.

Captured with `PrintWindow`, which asks the window to draw itself rather than
photographing the screen, so nothing behind or beside it can appear in the
result. Windows' invisible 11-pixel resize border is trimmed on each side and
the file re-encoded losslessly; **nothing is downsampled**, composited or
painted over. The largest is `collection.png` at 697 KB.

`material.png` and `chain.png` were **retaken on 2026-09-09** from a rebuilt
executable, because the code under those two screens changed after the first
pass. The other six are the 2026-09-08 captures.

Two mechanical notes for whoever shoots the next set on this machine: the
display runs at 144 dpi, and a DPI-unaware capture process is told the window is
1440 × 900 while `PrintWindow` renders 2160 × 1350 — the first attempt was
silently a top-left crop, with no error anywhere. And PowerToys FancyZones
re-snaps a window a second or two after any move, which produced two captures at
the wrong size; the maximised state it leaves alone.

### What each one shows

| File | Screen | State |
|---|---|---|
| `files.png` | Files | left pane a Windows folder of 29 `.adf` images, right pane the root of a Windows drive |
| `collection.png` | Collection | 2815 titles over three folders, grid with cover art |
| `material.png` | OS Builder → Install media | three material folders and the ten-row readout, *"10 of 10 found"* |
| `chain.png` | OS Builder → Install on the Amiga | the nine chain rows, *"3 of 9 applied"* |
| `pistorm.png` | PiStorm | hardware chosen; **no card folder chosen** |
| `gotek.png` | Gotek | OLED preview showing `DF0:` with no disk; **quickslot rack empty** |
| `tools.png` | Tools | **empty state** — no sector view opens until a file is chosen |

Three of those are empty states and are kept as such deliberately: they are the
screen a person actually meets on arrival, and a caption that describes a filled
screen beside a picture of an empty one is exactly the defect this project pays
for.

`collection.png` is kept for the pair of `1869` AGA cards. One is named by its
WHDLoad slave and one by its filename, so one carries the `~guessed` mark and
the other does not: a real example of the distinction the whole provenance layer
exists for, which is worth more than a tidier picture. The folder list carries
ART's own *"read by an older version of ART"* notice on all three roots — the
owner's real state, left alone rather than cleared for the photograph.

### The settings store was put back byte for byte

Both passes ran against the owner's own installation. The one setting
deliberately changed was the language, `tr` → `en`, made by a single
byte-targeted replacement in a **copy** of the store while no ART process was
running. Afterwards the untouched original was copied back with
`shutil.copyfile` — never a re-serialisation of parsed JSON — and the store's
SHA-256 is identical to what it was before either pass.

No operation was run for any of these captures: no copy, install, format,
download, scan-again, artwork fetch or Play. The folder scans visible in
`material.png` and `chain.png` are those screens' own read-only work on arrival.

Two things the photography found and that are filed rather than tidied away:
opening the Files screen rewrites a remembered tab's location with nothing
clicked ([ART-283](../ISSUES.md#open)), and `material.png`'s *"23 install disks
found"* counts game and CD32 discs as install disks
([ART-285](../ISSUES.md#open)).

### The screens not in this set

**The Dashboard is deliberately not here, and this is the second time it has been
left out for the same reason.** Its Recent panel lists real paths — the owner's
Windows username, and game disk images under `Downloads` — and none of that
belongs in a public repository. A Dashboard capture was taken with this set on
2026-09-08 and was **not committed**; the rule the 2026-08-18 note wrote down
still holds. If that screen is ever wanted, it needs a Recent list that is empty
or synthetic, not a crop.

**ROM Manager** and **Aminet** are not photographed either, because nothing in
the README points at them. The two defects that kept them out of the 2026-08-18
set — `CRC ERR` claimed about expansion-board ROMs ART simply did not recognise
([ART-138](../ISSUES.md#fixed)) and Aminet's text inputs rendering white in the
dark theme ([ART-139](../ISSUES.md#fixed)) — are both fixed, so shooting either
one is now a matter of a page needing the picture.

### Light theme

Not photographed. Its palette is measured against WCAG in CI along with the dark
one (105 pairs, both themes), but it wants a proper pass before it represents
the product. Contrast is not decoration here: most of the people this is for are
over fifty.

## The photograph of ART's output on real hardware

**It was taken on 2026-08-12.** `test/art-bootable-test.adf`, served from a
**Gotek** as `DF0:`, cold-booted a real **A500 / A500+** running **Kickstart
3.9** (the screen's copyright line reads `1985-2002`) to an AmigaDOS `1>`
prompt.

`STATUS.md`'s "Real hardware" row, `README.md`'s "Verified how, exactly"
section and `test/README.md`'s rung list all changed in the same commit,
because a picture in a README is not a verification record — the record is.

**The image file itself is not committed yet.** The photograph as taken is a
picture of the author's room, not of a screen, and what belongs in a public
repository is the screen: a crop showing the monitor and the machine, with
nothing else identifiable in frame. Whoever crops it puts it here as
`real-amiga-boot.jpg` and captions it with the machine, the Kickstart and the
image name — the three facts above and nothing softer.

Until then the claim stands on the written record, which is where it belongs
anyway.

Screenshots of ART's own window are a different thing and are welcome here
without any of the above ceremony.
