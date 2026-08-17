# Documentation assets

Images the documentation points at. Small, and only what a page actually uses.

## The five screenshots (2026-08-18)

`files.png` · `collection.png` · `pistorm.png` · `gotek.png` · `tools.png` —
ART's own window, Windows 11, **dark theme**, English, on a real 2787-title
library. Captured with `PrintWindow`, which asks the window to draw itself
rather than photographing the screen, so nothing behind or beside it can appear
in the result. Downscaled to 1600 px wide; otherwise untouched — no
compositing, nothing painted over.

`collection.png` is the only cropped one, and the crop is not for looks: two
cards outside the frame show [ART-137](../ISSUES.md), a Kickstart name that is
really machine code. **A full-width Collection shot belongs here as soon as that
is fixed** — cropping around a defect is a stopgap, not a decision.

### The three screens that are missing, and why

- **Dashboard** — its Recent panel carries real paths including a Windows
  username. The earlier `dashboard.png` was a crop made for that reason; it was
  removed with these images rather than kept as an orphan, since nothing points
  at it and it showed the light theme this set does not use.
- **ROM Manager** — it labels expansion-board ROMs (`A4091.rom`,
  `Blizzard_1230-IV.rom`) `CRC ERR`, which claims a damage ART has no way to
  know about. The right label is "not a Kickstart".
- **Aminet** — a Windows username in two places, and its text inputs render
  white in the dark theme.

The last two are defects the photography found, which is a reason to fix them
rather than to shoot around them.

### Light theme

Not photographed. Its palette was widened the same day — a page that is
definitely not white, panels that are, borders strong enough to be a line — but
it wants a proper pass before it represents the product. Contrast is not
decoration here: most of the people this is for are over fifty.

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
